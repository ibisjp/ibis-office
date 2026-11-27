// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! DocuWorks document reader based on the EZPZ File XDW format notes.
//! Dimensions and drawing coordinates are in hundredths of a millimetre.

mod bitmap;
mod path_clip;
mod scan;
mod scan_emf;
mod vector_emf;
mod wmf_text;

use std::borrow::Cow;

use delharc::decode::{Decoder, Lh5Decoder};

const MAX_BLOCK: usize = 128 << 20;
const MAX_FILE: usize = 512 << 20;
const MAX_PAGES: usize = 20_000;
const MAX_OBJECTS: usize = 1_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Protected,
    Binder,
    UnsupportedDrawing,
    Limit,
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy)]
struct Element<'a> {
    tag: u32,
    class: u8,
    value: &'a [u8],
}

fn byte(input: &mut &[u8]) -> Result<u8> {
    let (&value, rest) = input.split_first().ok_or(Error::Invalid)?;
    *input = rest;
    Ok(value)
}

fn element<'a>(input: &mut &'a [u8]) -> Result<Element<'a>> {
    let id = byte(input)?;
    let mut tag = u32::from(id & 31);
    if tag == 31 {
        tag = 0;
        loop {
            let digit = byte(input)?;
            tag = tag
                .checked_mul(128)
                .and_then(|tag| tag.checked_add(u32::from(digit & 127)))
                .ok_or(Error::Invalid)?;
            if digit < 128 {
                break;
            }
        }
    }
    let length = byte(input)?;
    let length = if length < 128 {
        usize::from(length)
    } else {
        let count = length & 127;
        if !(1..=4).contains(&count) {
            return Err(Error::Invalid);
        }
        let mut n = 0usize;
        for _ in 0..count {
            n = n * 256 + usize::from(byte(input)?);
        }
        n
    };
    let (value, rest) = input.split_at_checked(length).ok_or(Error::Invalid)?;
    *input = rest;
    Ok(Element {
        tag,
        class: id & 224,
        value,
    })
}

fn field(mut input: &[u8], tag: u32) -> Result<Option<Element<'_>>> {
    while !input.is_empty() {
        let value = element(&mut input)?;
        if value.tag == tag {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn integer(bytes: &[u8]) -> Result<i64> {
    if bytes.is_empty() || bytes.len() > 8 {
        return Err(Error::Invalid);
    }
    let mut value = if bytes[0] & 128 != 0 { -1i64 } else { 0 };
    for &digit in bytes {
        value = (value << 8) | i64::from(digit);
    }
    Ok(value)
}

fn number(input: &[u8], tag: u32) -> Result<usize> {
    let value = field(input, tag)?.ok_or(Error::Invalid)?;
    usize::try_from(integer(value.value)?).map_err(|_| Error::Invalid)
}

fn expand(input: &[u8], length: usize) -> Result<Vec<u8>> {
    if length > MAX_BLOCK || length > input.len().saturating_mul(1000).saturating_add(65536) {
        return Err(Error::Limit);
    }
    let mut output = vec![0; length];
    Lh5Decoder::new(input)
        .fill_buffer(&mut output)
        .map_err(|_| Error::Invalid)?;
    Ok(output)
}

fn trailing_element<'a>(
    input: &'a [u8],
    length: usize,
    tags: &[u8],
) -> Result<(usize, Element<'a>)> {
    let value_start = input.len().checked_sub(length).ok_or(Error::Invalid)?;
    for header in 2..=6 {
        let Some(start) = value_start.checked_sub(header) else {
            break;
        };
        let mut candidate = &input[start..];
        if tags.contains(&candidate[0])
            && let Ok(value) = element(&mut candidate)
            && candidate.is_empty()
            && value.value.len() == length
        {
            return Ok((start, value));
        }
    }
    Err(Error::Invalid)
}

pub struct Document<B> {
    storage: B,
    entries: Vec<u8>,
    body_end: usize,
    properties: Vec<u8>,
}

pub struct Page<'a> {
    pub width: u32,
    pub height: u32,
    pub objects: Vec<PageObject<'a>>,
}

pub struct PageObject<'a> {
    pub kind: i64,
    pub x: i64,
    pub y: i64,
    pub rotation: i64,
    pub size: Option<(u32, u32)>,
    drawing: Option<DrawingSource<'a>>,
    attributes: &'a [u8],
}

enum DrawingSource<'a> {
    Entry(usize),
    Inline(&'a [u8]),
}

pub struct Drawing<'a> {
    pub kind: usize,
    pub bytes: Cow<'a, [u8]>,
}

impl<B: AsRef<[u8]>> Document<B> {
    pub fn open(storage: B) -> Result<Self> {
        let bytes = storage.as_ref();
        if bytes.len() > MAX_FILE {
            return Err(Error::Limit);
        }
        let header = element(&mut &*bytes)?;
        if header.class != 0x60 || header.tag != 0 || !matches!(number(header.value, 2)?, 7 | 10) {
            return Err(Error::Invalid);
        }
        let length = bytes.last_chunk::<4>().ok_or(Error::Invalid)?;
        let (at, trailer) =
            trailing_element(bytes, u32::from_le_bytes(*length) as usize, &[0x65, 0x68])?;
        let entries = field(trailer.value, 1)?.map_or(&[][..], |item| item.value);
        if entries.len() % 4 != 0 || entries.len() / 4 != number(trailer.value, 0)? {
            return Err(Error::Invalid);
        }
        if entries.len() / 4 > MAX_OBJECTS {
            return Err(Error::Limit);
        }
        let properties_size = number(trailer.value, 4)?;
        let (_, stored) = trailing_element(&bytes[..at], properties_size, &[0x63, 0x66])?;
        if stored.tag == 6 {
            return Err(Error::Protected);
        }
        let checksum = field(trailer.value, 5)?.ok_or(Error::Invalid)?;
        let actual = stored.value.chunks_exact(4).fold(0, |acc, chunk| {
            acc ^ u32::from_le_bytes(chunk.try_into().unwrap())
        });
        if actual != integer(checksum.value)? as u32 {
            return Err(Error::Invalid);
        }
        let properties = expand(stored.value, number(trailer.value, 3)?)?;
        Ok(Self {
            entries: entries.to_vec(),
            storage,
            body_end: at,
            properties,
        })
    }

    pub fn pages(&self) -> Result<Vec<Page<'_>>> {
        let mut input = self.properties.as_slice();
        let mut parents = Vec::new();
        let mut pages = Vec::new();
        let mut position = (0, 0);
        let mut object_count = 0;
        while !input.is_empty() {
            let record = element(&mut input)?;
            if record.class != 0x60 || record.tag != 2 {
                return Err(Error::Invalid);
            }
            let mut attributes = record.value;
            let depth = element(&mut attributes)?;
            let kind = element(&mut attributes)?;
            if depth.class != 0x80 || depth.tag != 0 || kind.class != 0x60 || kind.tag != 2 {
                return Err(Error::Invalid);
            }
            let &[depth] = depth.value else {
                return Err(Error::Invalid);
            };
            let depth = usize::from(depth);
            let kind = integer(kind.value)?;
            if depth > parents.len() || (depth == 0 && !parents.is_empty()) {
                return Err(Error::Invalid);
            }
            parents.truncate(depth);
            match (parents.as_slice(), kind) {
                ([], 0xc014) => return Err(Error::Binder),
                ([], kind) if kind != 0xc013 => return Err(Error::Invalid),
                ([0xc013, 0x1303], 0x1301) => {
                    if pages.len() == MAX_PAGES {
                        return Err(Error::Limit);
                    }
                    let mut dimensions = field(attributes, 5)?.ok_or(Error::Invalid)?.value;
                    let width = dimension(&mut dimensions)?;
                    let height = dimension(&mut dimensions)?;
                    pages.push(Page {
                        width,
                        height,
                        objects: Vec::new(),
                    });
                }
                ([0xc013, 0x1303, 0x1301], 0x1302) => {
                    position = if let Some(field) = field(attributes, 52)? {
                        let mut values = field.value;
                        (next_integer(&mut values)?, next_integer(&mut values)?)
                    } else {
                        (0, 0)
                    };
                }
                ([0xc013, 0x1303, 0x1301, 0x1302], _) => {
                    if object_count == MAX_OBJECTS {
                        return Err(Error::Limit);
                    }
                    object_count += 1;
                    let drawing = field(attributes, 7)?
                        .map(|item| {
                            if item.class & 0xc0 == 0xc0 {
                                number(item.value, 1).map(DrawingSource::Entry)
                            } else {
                                Ok(DrawingSource::Inline(item.value))
                            }
                        })
                        .transpose()?;
                    let rotation = field(attributes, 61)?
                        .map(|item| {
                            let mut bytes = item.value;
                            next_integer(&mut bytes)
                        })
                        .transpose()?
                        .unwrap_or(0);
                    let size = field(attributes, 5)?.and_then(|item| {
                        let mut values = item.value;
                        Some((dimension(&mut values).ok()?, dimension(&mut values).ok()?))
                    });
                    pages
                        .last_mut()
                        .ok_or(Error::Invalid)?
                        .objects
                        .push(PageObject {
                            kind,
                            x: position.0,
                            y: position.1,
                            rotation,
                            size,
                            drawing,
                            attributes,
                        });
                }
                _ => {}
            }
            parents.push(kind);
        }
        if pages.is_empty() {
            return Err(Error::Invalid);
        }
        Ok(pages)
    }

    pub fn object_drawing<'b>(&'b self, object: &'b PageObject<'_>) -> Result<Option<Drawing<'b>>> {
        match object.drawing {
            Some(DrawingSource::Entry(index)) => self.entry_drawing(index).map(Some),
            Some(DrawingSource::Inline(bytes)) => decode_drawing(bytes).map(Some),
            None => Ok(None),
        }
    }

    /// Load the drawing body addressed by a zero-based trailer entry number.
    fn entry_drawing(&self, index: usize) -> Result<Drawing<'_>> {
        decode_drawing(self.entry_body(index)?)
    }

    fn entry_body(&self, index: usize) -> Result<&[u8]> {
        let slot = index.checked_mul(4).ok_or(Error::Invalid)?;
        let offset = self
            .entries
            .get(slot..)
            .and_then(<[u8]>::first_chunk::<4>)
            .ok_or(Error::Invalid)?;
        let offset = u32::from_le_bytes(*offset) as usize;
        let entry = element(
            &mut self
                .storage
                .as_ref()
                .get(offset..self.body_end)
                .ok_or(Error::Invalid)?,
        )?;
        if entry.class == 0x60 && entry.tag == 7 {
            return Err(Error::Protected);
        }
        if entry.class != 0x60 || entry.tag != 4 {
            return Err(Error::Invalid);
        }
        Ok(field(entry.value, 2)?.ok_or(Error::Invalid)?.value)
    }
}

fn next_integer(input: &mut &[u8]) -> Result<i64> {
    let n = usize::from(byte(input)?);
    let (value, rest) = input.split_at_checked(n).ok_or(Error::Invalid)?;
    *input = rest;
    integer(value)
}

fn dimension(input: &mut &[u8]) -> Result<u32> {
    let value = u32::try_from(next_integer(input)?).map_err(|_| Error::Invalid)?;
    if value == 0 {
        return Err(Error::Invalid);
    }
    Ok(value)
}

fn decode_drawing(body: &[u8]) -> Result<Drawing<'_>> {
    let kind = number(body, 0)?;
    let data = field(body, 6)?.ok_or(Error::Invalid)?.value;
    let compression = field(body, 10)?;
    let length = field(body, 1)?
        .map(|item| integer(item.value))
        .transpose()?;
    let compressed = match compression {
        Some(flag) => integer(flag.value)? == 1,
        None => kind != 7 && length.is_some_and(|length| length > data.len() as i64),
    };
    let bytes = if compressed {
        let length = length
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(Error::Invalid)?;
        Cow::Owned(expand(data, length)?)
    } else {
        Cow::Borrowed(data)
    };
    Ok(Drawing { kind, bytes })
}
