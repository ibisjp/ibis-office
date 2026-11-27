// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Bounded decoding of the JTD block store and logical records. Derived from
//! published format observations, not another project's parser implementation.
use ibis_formats::Error;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::Arc,
};

const LIMIT: usize = 32 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Piece {
    pub substream: u32,
    pub units: Range<usize>,
    /// Physical block ids in logical order, including the TextV header, if any.
    pub blocks: Vec<u32>,
    pub text_byte_offset: usize,
}

#[derive(Clone, Debug)]
pub struct StyleSpan {
    pub units: Range<usize>,
    pub properties: Arc<BTreeMap<u8, Vec<u8>>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordKind {
    Text,
    Inline {
        selector: [u16; 2],
        text: Range<usize>,
    },
    Record {
        class: u16,
        payload: Range<usize>,
    },
    Control(u16),
}

#[derive(Clone, Debug)]
pub struct Record {
    /// UTF-16 units in the reconstructed logical text, never physical offsets.
    pub units: Range<usize>,
    pub kind: RecordKind,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub units: Range<usize>,
    pub message: &'static str,
}

/// Retains every logical text unit, unknown record, property and undecoded
/// style tail. Consumers can trace a conversion back to the original units.
#[derive(Clone, Debug)]
pub struct DecodedText {
    pub block_size: usize,
    pub units: Vec<u16>,
    pub pieces: Vec<Piece>,
    pub styles: Vec<StyleSpan>,
    pub records: Vec<Record>,
    pub diagnostics: Vec<Diagnostic>,
    pub undecoded_styles: Vec<(u32, Vec<u8>)>,
    pub piece_list_trailer: Option<u32>,
}

fn invalid() -> Error {
    Error::new("JTD本文の構造または長さが不正です")
}
fn u32_at(data: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_be_bytes(
        data.get(at..at.checked_add(4).ok_or_else(invalid)?)
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())?,
    ))
}
fn take32(data: &[u8], at: &mut usize) -> Result<u32, Error> {
    let value = u32_at(data, *at)?;
    *at += 4;
    Ok(value)
}

struct Substream {
    bytes: Vec<u8>,
    blocks: Vec<u32>,
}

pub fn decode(data: &[u8]) -> Result<DecodedText, Error> {
    if data.len() > LIMIT || data.get(..8) != Some(b"SsmgV.01") {
        return Err(invalid());
    }
    let count = u32_at(data, 8)? as usize;
    let size = u32_at(data, 12)? as usize;
    let blocks = u32_at(data, 16)? as usize;
    if count == 0
        || count > data.len() / 20
        || !(64..=65536).contains(&size)
        || !size.is_power_of_two()
    {
        return Err(invalid());
    }
    let mut at = 20usize
        .checked_add(size.checked_mul(blocks).ok_or_else(invalid)?)
        .ok_or_else(invalid)?;
    if at > data.len() {
        return Err(invalid());
    }
    let mut streams = BTreeMap::new();
    let mut used = BTreeSet::new();
    let mut total = 0usize;
    for _ in 0..count {
        let index = take32(data, &mut at)?;
        let reserved = take32(data, &mut at)?;
        let length = take32(data, &mut at)? as usize;
        let n = take32(data, &mut at)? as usize;
        let repeated = take32(data, &mut at)? as usize;
        if reserved != 0
            || n != repeated
            || n > blocks
            || length > n.checked_mul(size).ok_or_else(invalid)?
        {
            return Err(invalid());
        }
        total = total.checked_add(length).ok_or_else(invalid)?;
        if total > LIMIT {
            return Err(invalid());
        }
        let mut bytes = Vec::with_capacity(length);
        let mut ids = Vec::with_capacity(n);
        for _ in 0..n {
            let id = take32(data, &mut at)?;
            if id as usize >= blocks || !used.insert(id) {
                return Err(invalid());
            }
            ids.push(id);
            let start = 20 + id as usize * size;
            let length = (length - bytes.len()).min(size);
            bytes.extend_from_slice(data.get(start..start + length).ok_or_else(invalid)?);
        }
        if streams
            .insert(index, Substream { bytes, blocks: ids })
            .is_some()
        {
            return Err(invalid());
        }
    }
    if data.get(at..) != Some(&[0, 0, 0, 0]) {
        return Err(invalid());
    }
    let root = &streams.get(&0).ok_or_else(invalid)?.bytes;
    let mut trailer = None;
    let pieces = match root.get(..8) {
        Some(b"TextV.01") => vec![(u32_at(root, 8)? as usize, 0, 12usize)],
        Some(b"QLSTV.01") => {
            let n = u32_at(root, 8)? as usize;
            let end = 12usize
                .checked_add(n.checked_mul(8).ok_or_else(invalid)?)
                .ok_or_else(invalid)?;
            if n == 0 || n > count || (root.len() != end && root.len() != end + 4) {
                return Err(invalid());
            }
            if root.len() == end + 4 {
                trailer = Some(u32_at(root, end)?);
            }
            let mut pieces = Vec::with_capacity(n);
            let mut seen = BTreeSet::new();
            for i in 0..n {
                let count = u32_at(root, 12 + i * 8)? as usize;
                let index = u32_at(root, 16 + i * 8)?;
                if index == 0 || !seen.insert(index) {
                    return Err(invalid());
                }
                pieces.push((count, index, 0));
            }
            pieces
        }
        _ => return Err(Error::new("JTDの論理本文形式が未対応です")),
    };
    let mut result = DecodedText {
        block_size: size,
        units: Vec::new(),
        pieces: Vec::new(),
        styles: Vec::new(),
        records: Vec::new(),
        diagnostics: Vec::new(),
        undecoded_styles: Vec::new(),
        piece_list_trailer: trailer,
    };
    let mut style_allocation = 0usize;
    for (count, index, offset) in pieces {
        let substream = streams.get(&index).ok_or_else(invalid)?;
        let end = offset
            .checked_add(count.checked_mul(2).ok_or_else(invalid)?)
            .ok_or_else(invalid)?;
        let text = substream.bytes.get(offset..end).ok_or_else(invalid)?;
        let start = result.units.len();
        if start
            .checked_add(count)
            .filter(|n| *n <= LIMIT / 2)
            .is_none()
        {
            return Err(invalid());
        }
        result.units.extend(
            text.chunks_exact(2)
                .map(|b| u16::from_be_bytes([b[0], b[1]])),
        );
        let range = start..start + count;
        let style = &substream.bytes[end..];
        match parse_styles(style, range.clone(), &mut style_allocation) {
            Ok((spans, consumed)) => {
                result.styles.extend(spans);
                if consumed < style.len() {
                    result.diagnostics.push(Diagnostic {
                        units: range.clone(),
                        message: "本文全体を覆う書式の後に未知の末尾情報があります",
                    });
                    result
                        .undecoded_styles
                        .push((index, style[consumed..].to_vec()));
                }
            }
            Err(_) => {
                result.diagnostics.push(Diagnostic {
                    units: range.clone(),
                    message: "未対応の文字書式を保持しました",
                });
                result.undecoded_styles.push((index, style.to_vec()));
            }
        }
        result.pieces.push(Piece {
            substream: index,
            units: range,
            blocks: substream.blocks.clone(),
            text_byte_offset: offset,
        });
    }
    result.records = records(&result.units)?;
    for record in &result.records {
        let unknown = match &record.kind {
            RecordKind::Record { class, .. } => !matches!(class, 0 | 0x10 | 0x20 | 0x30),
            RecordKind::Control(n) => !matches!(n, 0 | 9 | 10 | 12 | 13 | 14),
            RecordKind::Inline { selector, text } => {
                !matches!(selector, [0, 1 | 3] | [1, 0 | 0x82])
                    || result.units[text.clone()]
                        .iter()
                        .any(|n| *n < 0x20 && !matches!(n, 9 | 10 | 13))
            }
            _ => false,
        };
        if unknown {
            result.diagnostics.push(Diagnostic {
                units: record.units.clone(),
                message: "未解読の本文要素を保持しました",
            });
        }
    }
    Ok(result)
}

fn parse_styles(
    data: &[u8],
    range: Range<usize>,
    allocation: &mut usize,
) -> Result<(Vec<StyleSpan>, usize), Error> {
    let mut at = 0usize;
    let mut position = range.start;
    let mut state = Arc::new(BTreeMap::new());
    let mut spans = Vec::new();
    while at < data.len() {
        if position == range.end {
            if data.get(at..at + 5) == Some(&[0, 0, 0, 0, 0]) {
                at += 5;
                continue;
            }
            if data.get(at..) == Some(&[0xff]) {
                at += 1;
            }
            break;
        }
        let tag = data[at];
        at += 1;
        let n = match tag {
            0 => take32(data, &mut at)? as usize,
            0xfe => {
                loop {
                    let id = *data.get(at).ok_or_else(invalid)?;
                    at += 1;
                    if id == 0xff {
                        break;
                    }
                    let len = *data.get(at).ok_or_else(invalid)? as usize;
                    at += 1;
                    let end = at.checked_add(len).ok_or_else(invalid)?;
                    let value = data.get(at..end).ok_or_else(invalid)?;
                    Arc::make_mut(&mut state).insert(id, value.to_vec());
                    at = end;
                }
                if data.get(at) != Some(&0) {
                    return Err(invalid());
                }
                at += 1;
                *allocation += state
                    .iter()
                    .map(|(_, value)| value.len() + 48)
                    .sum::<usize>();
                if *allocation > 64 * 1024 * 1024 {
                    return Err(invalid());
                }
                1
            }
            0xff if at == data.len() => break,
            _ => return Err(invalid()),
        };
        let end = position
            .checked_add(n)
            .filter(|end| *end <= range.end)
            .ok_or_else(invalid)?;
        if n > 0 {
            spans.push(StyleSpan {
                units: position..end,
                properties: state.clone(),
            });
        }
        position = end;
    }
    if position != range.end {
        return Err(invalid());
    }
    Ok((spans, at))
}

fn records(units: &[u16]) -> Result<Vec<Record>, Error> {
    let mut at = 0usize;
    let mut result = Vec::new();
    while at < units.len() {
        if result.len() >= 1_000_000 {
            return Err(invalid());
        }
        let start = at;
        let kind = match units[at] {
            0x1c => {
                let class = *units.get(at + 1).ok_or_else(invalid)?;
                let count = *units.get(at + 2).ok_or_else(invalid)? as usize;
                if class == 1 {
                    if count != 7
                        || units.get(at + 3) != Some(&0)
                        || units.get(at + 6) != Some(&0x1d)
                    {
                        return Err(invalid());
                    }
                    let text = at + 7;
                    let end = text
                        + units
                            .get(text..)
                            .ok_or_else(invalid)?
                            .iter()
                            .position(|n| *n == 0x1e)
                            .ok_or_else(invalid)?;
                    if units.get(end..end + 5) != Some(&[0x1e, 5, 0, 1, 0x1f]) {
                        return Err(invalid());
                    }
                    let selector = [units[at + 4], units[at + 5]];
                    at = end + 5;
                    RecordKind::Inline {
                        selector,
                        text: text..end,
                    }
                } else {
                    let end = at
                        .checked_add(count)
                        .filter(|end| *end <= units.len())
                        .ok_or_else(invalid)?;
                    if count < 7 || units.get(end - 4..end) != Some(&[count as u16, 0, class, 0x1f])
                    {
                        return Err(invalid());
                    }
                    let payload = at + 3..end - 4;
                    at = end;
                    RecordKind::Record { class, payload }
                }
            }
            n if n < 0x20 => {
                at += 1;
                RecordKind::Control(n)
            }
            _ => {
                at += 1;
                while units.get(at).is_some_and(|n| *n >= 0x20) {
                    at += 1;
                }
                RecordKind::Text
            }
        };
        result.push(Record {
            units: start..at,
            kind,
        });
    }
    Ok(result)
}

impl DecodedText {
    /// Physical byte ranges within the original DocumentText stream. A
    /// logical record may cross blocks and pieces, so it can have many ranges.
    pub fn physical_ranges(&self, units: Range<usize>) -> Option<Vec<Range<usize>>> {
        if units.start > units.end || units.end > self.units.len() {
            return None;
        }
        let mut ranges = Vec::new();
        for piece in &self.pieces {
            let start = units.start.max(piece.units.start);
            let end = units.end.min(piece.units.end);
            if start >= end {
                continue;
            }
            let mut at = piece.text_byte_offset + (start - piece.units.start) * 2;
            let stop = piece.text_byte_offset + (end - piece.units.start) * 2;
            while at < stop {
                let id = *piece.blocks.get(at / self.block_size)? as usize;
                let inside = at % self.block_size;
                let length = (stop - at).min(self.block_size - inside);
                let physical = 20 + id * self.block_size + inside;
                ranges.push(physical..physical + length);
                at += length;
            }
        }
        Some(ranges)
    }
    pub fn text(&self, range: Range<usize>) -> Result<String, Error> {
        String::from_utf16(self.units.get(range).ok_or_else(invalid)?)
            .map_err(|_| Error::new("JTDのUTF-16文字列が不正です"))
    }
    pub fn style(&self, position: usize) -> Option<&BTreeMap<u8, Vec<u8>>> {
        let index = self
            .styles
            .partition_point(|span| span.units.end <= position);
        self.styles
            .get(index)
            .filter(|span| span.units.contains(&position))
            .map(|span| span.properties.as_ref())
    }
    pub fn tlv(&self, payload: Range<usize>) -> Result<Vec<(u16, Range<usize>)>, Error> {
        let words = self.units.get(payload.clone()).ok_or_else(invalid)?;
        if words.first() != Some(&0) {
            return Err(invalid());
        }
        let mut at = payload.start + 1;
        let mut result = Vec::new();
        while at + 2 <= payload.end {
            let tag = self.units[at];
            let count = self.units[at + 1] as usize;
            at += 2;
            if tag == 0xffff && count == 0 && at == payload.end {
                return Ok(result);
            }
            let end = at
                .checked_add(count)
                .filter(|end| *end <= payload.end)
                .ok_or_else(invalid)?;
            result.push((tag, at..end));
            at = end;
        }
        Err(invalid())
    }
}

#[cfg(test)]
pub(super) fn fixture(units: &[u16]) -> Vec<u8> {
    let mut styles = vec![0];
    styles.extend_from_slice(&(units.len() as u32).to_be_bytes());
    fixture_with_styles(units, &styles)
}

#[cfg(test)]
pub(super) fn fixture_with_styles(units: &[u16], styles: &[u8]) -> Vec<u8> {
    let mut text = b"TextV.01".to_vec();
    text.extend_from_slice(&(units.len() as u32).to_be_bytes());
    for n in units {
        text.extend_from_slice(&n.to_be_bytes());
    }
    text.extend_from_slice(styles);
    let length = text.len();
    let n = length.div_ceil(256);
    text.resize(n * 256, 0);
    let mut data = b"SsmgV.01".to_vec();
    for value in [1, 256, n as u32] {
        data.extend_from_slice(&value.to_be_bytes());
    }
    data.extend(text);
    for value in [0, 0, length as u32, n as u32, n as u32] {
        data.extend_from_slice(&value.to_be_bytes());
    }
    for id in 0..n as u32 {
        data.extend_from_slice(&id.to_be_bytes());
    }
    data.extend_from_slice(&0u32.to_be_bytes());
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn store(parts: Vec<(u32, Vec<u8>)>) -> Vec<u8> {
        let mut blocks = Vec::new();
        let mut directory = Vec::new();
        for (id, bytes) in &parts {
            let first = blocks.len() / 256;
            let n = bytes.len().div_ceil(256);
            blocks.extend_from_slice(bytes);
            blocks.resize((first + n) * 256, 0);
            for value in [*id, 0, bytes.len() as u32, n as u32, n as u32] {
                directory.extend_from_slice(&value.to_be_bytes());
            }
            for b in first..first + n {
                directory.extend_from_slice(&(b as u32).to_be_bytes());
            }
        }
        let mut result = b"SsmgV.01".to_vec();
        for value in [parts.len() as u32, 256, (blocks.len() / 256) as u32] {
            result.extend_from_slice(&value.to_be_bytes());
        }
        result.extend(blocks);
        result.extend(directory);
        result.extend_from_slice(&[0; 4]);
        result
    }

    fn piece(units: &[u16]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for n in units {
            bytes.extend_from_slice(&n.to_be_bytes());
        }
        bytes.push(0);
        bytes.extend_from_slice(&(units.len() as u32).to_be_bytes());
        bytes
    }

    #[test]
    fn physical_block_order_and_unused_bytes_do_not_change_text() {
        let words = vec![0x7532; 400];
        let original = fixture(&words);
        let mut reordered = original.clone();
        let a = original[20..276].to_vec();
        let b = original[276..532].to_vec();
        reordered[20..276].copy_from_slice(&b);
        reordered[276..532].copy_from_slice(&a);
        let blocks = u32_at(&original, 16).unwrap() as usize;
        let at = 20 + blocks * 256 + 20;
        reordered[at..at + 4].copy_from_slice(&1u32.to_be_bytes());
        reordered[at + 4..at + 8].copy_from_slice(&0u32.to_be_bytes());
        assert_eq!(decode(&reordered).unwrap().units, words);
        let decoded = decode(&reordered).unwrap();
        let physical = decoded.physical_ranges(0..words.len()).unwrap();
        let recovered: Vec<u8> = physical
            .iter()
            .flat_map(|r| reordered[r.clone()].iter().copied())
            .collect();
        assert_eq!(
            recovered,
            words
                .iter()
                .flat_map(|n| n.to_be_bytes())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            decode(&original).unwrap().units,
            decode(&reordered).unwrap().units
        );
        // A fake text record in padding is never tokenized.
        let logical_length = u32_at(&original, 20 + blocks * 256 + 8).unwrap() as usize;
        let mut with_padding = original.clone();
        with_padding[20 + logical_length..20 + logical_length + 14]
            .copy_from_slice(&[0, 28, 0, 16, 0, 7, 0, 7, 0, 0, 0, 16, 0, 31]);
        assert_eq!(decode(&with_padding).unwrap().units, words);
    }

    #[test]
    fn pieces_are_joined_before_record_and_surrogate_decoding() {
        let units = [
            0x1c, 1, 7, 0, 0, 3, 0x1d, 0x7532, 0x1e, 5, 0, 1, 0x1f, 0xd83d, 0xde00,
        ];
        let mut list = b"QLSTV.01".to_vec();
        for v in [3u32, 9, 1, 5, 2, 1, 3, 2] {
            list.extend_from_slice(&v.to_be_bytes());
        }
        let data = store(vec![
            (0, list),
            (1, piece(&units[..9])),
            (2, piece(&units[9..14])),
            (3, piece(&units[14..])),
        ]);
        let decoded = decode(&data).unwrap();
        assert_eq!(decoded.units, units);
        assert_eq!(decoded.piece_list_trailer, Some(2));
        assert_eq!(decoded.records.len(), 2);
        assert_eq!(decoded.text(13..15).unwrap(), "😀");
    }

    #[test]
    fn malformed_directory_and_record_are_rejected() {
        let mut allocation = 64 * 1024 * 1024;
        assert!(parse_styles(&[0xfe, 1, 2, 0, 1, 0xff, 0], 0..1, &mut allocation).is_err());
        let original = fixture(&vec![0x7532; 400]);
        let mut duplicate = original.clone();
        let blocks = u32_at(&original, 16).unwrap() as usize;
        let second_id = 20 + blocks * 256 + 24;
        duplicate[second_id..second_id + 4].copy_from_slice(&0u32.to_be_bytes());
        assert!(decode(&duplicate).is_err());
        let mut overflow = original;
        overflow[16..20].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(decode(&overflow).is_err());
        assert!(decode(&fixture(&[0x1c, 0x10, 20, 0])).is_err());
        assert!(decode(&fixture(&[0x1c, 1, 7, 0, 0, 3, 0x1d, 0x7532, 0x1e])).is_err());
    }

    #[test]
    fn style_coverage_and_unknown_tail_are_separate() {
        let mut bytes = b"TextV.01".to_vec();
        bytes.extend_from_slice(&2u32.to_be_bytes());
        bytes.extend_from_slice(&[0x75, 0x32, 0x4e, 0x59]);
        bytes.extend_from_slice(&[
            0xfe, 20, 4, 0x84, 0, 0, 0, 2, 2, 1, 0x89, 0xff, 0, 0, 0, 0, 0, 1,
        ]);
        // Unspecified tail after exactly two styled units; do not discard
        // the valid styles or consume it as another run.
        bytes.extend_from_slice(&[0, 0x27]);
        let decoded = decode(&store(vec![(0, bytes)])).unwrap();
        assert_eq!(decoded.styles.last().unwrap().units.end, 2);
        assert_eq!(decoded.style(1).unwrap()[&20], [0x84, 0, 0, 0]);
        assert_eq!(decoded.undecoded_styles, [(0, vec![0, 0x27])]);
        assert_eq!(decoded.diagnostics.len(), 1);
    }

    #[test]
    fn unknown_records_and_properties_keep_source_ranges() {
        let decoded = decode(&fixture(&[0x1c, 0x77, 8, 0xabcd, 8, 0, 0x77, 0x1f, 0x7532])).unwrap();
        assert_eq!(decoded.records[0].units, 0..8);
        assert_eq!(decoded.units[3], 0xabcd);
        assert_eq!(decoded.diagnostics.len(), 1);
    }
}
