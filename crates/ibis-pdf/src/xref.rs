// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! PDF cross-reference (`xref` table & XRef stream) and revision history parser.

use std::collections::{HashMap, HashSet};

use crate::cos::{
    CosDict, CosObj, Lexer, StreamData, Token, find_bytes, is_whitespace, parse_indirect_object,
};
use crate::{Error, Result};

/// A single entry in a PDF cross-reference section.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XrefEntry {
    InUse { offset: usize, generation: u16 },
    Compressed { obj_stm_num: u32, index: u16 },
    Free { next_free: u32, generation: u16 },
}

/// One revision (save / incremental update) in a PDF's history.
#[derive(Clone, Debug)]
pub struct Revision {
    /// 0 for initial save, 1.. for each incremental update.
    pub index: usize,
    pub xref_offset: usize,
    /// Byte offset right after this revision's `%%EOF` marker (plus optional single EOL).
    pub eof_offset: usize,
    pub trailer: CosDict,
    pub entries: HashMap<u32, XrefEntry>,
}

/// Parse all revisions from oldest (index 0) to newest (last) by walking `/Prev`.
pub(crate) fn parse_revisions(raw: &[u8]) -> Result<Vec<Revision>> {
    let (latest_xref, latest_eof) = find_last_startxref(raw)?;
    let mut visited = HashSet::new();
    let mut rev_chain: Vec<(usize, usize, CosDict, HashMap<u32, XrefEntry>)> = Vec::new();

    let mut next_xref = Some((latest_xref, Some(latest_eof)));
    while let Some((xref_offset, known_eof)) = next_xref {
        if !visited.insert(xref_offset) {
            return Err(Error::new("cyclic /Prev chain in PDF xref"));
        }
        let (trailer, mut entries, section_end) = parse_xref_section(raw, xref_offset)?;

        // If hybrid PDF specifies /XRefStm, merge its compressed entries too.
        if let Some(xref_stm_off) = trailer.get_int(b"XRefStm").filter(|&o| o >= 0) {
            let stm_off = xref_stm_off as usize;
            if visited.insert(stm_off)
                && let Ok((_, stm_entries, _)) = parse_xref_stream(raw, stm_off)
            {
                for (num, entry) in stm_entries {
                    entries.entry(num).or_insert(entry);
                }
            }
        }

        let eof_offset = match known_eof {
            Some(off) => off,
            None => find_next_eof(raw, section_end).unwrap_or(section_end),
        };

        let prev = trailer
            .get_int(b"Prev")
            .filter(|&p| p >= 0)
            .map(|p| (p as usize, None));

        rev_chain.push((xref_offset, eof_offset, trailer, entries));
        next_xref = prev;
    }

    rev_chain.reverse();
    Ok(rev_chain
        .into_iter()
        .enumerate()
        .map(
            |(index, (xref_offset, eof_offset, trailer, entries))| Revision {
                index,
                xref_offset,
                eof_offset,
                trailer,
                entries,
            },
        )
        .collect())
}

fn find_last_startxref(raw: &[u8]) -> Result<(usize, usize)> {
    let tail_start = raw.len().saturating_sub(2048);
    let tail = &raw[tail_start..];
    let rel_pos = tail
        .windows(b"startxref".len())
        .rposition(|w| w == b"startxref")
        .ok_or_else(|| Error::new("missing startxref near end of PDF"))?;
    let pos = tail_start + rel_pos + b"startxref".len();
    let mut lexer = Lexer::new(raw, pos);
    let Token::Obj(CosObj::Int(offset)) = lexer.next_token()? else {
        return Err(Error::new("invalid offset after startxref"));
    };
    if offset < 0 || (offset as usize) >= raw.len() {
        return Err(Error::new("startxref offset out of bounds"));
    }
    let eof_offset = find_next_eof(raw, lexer.pos).unwrap_or(raw.len());
    Ok((offset as usize, eof_offset))
}

fn find_next_eof(raw: &[u8], from: usize) -> Option<usize> {
    let p = find_bytes(raw, b"%%EOF", from)?;
    let mut end = p + b"%%EOF".len();
    if raw.get(end) == Some(&b'\r') {
        end += 1;
    }
    if raw.get(end) == Some(&b'\n') {
        end += 1;
    }
    Some(end)
}

fn parse_xref_section(
    raw: &[u8],
    offset: usize,
) -> Result<(CosDict, HashMap<u32, XrefEntry>, usize)> {
    let mut lexer = Lexer::new(raw, offset);
    lexer.skip_ws();
    if raw
        .get(lexer.pos..)
        .is_some_and(|s| s.starts_with(b"xref"))
    {
        parse_xref_table(raw, lexer.pos)
    } else {
        parse_xref_stream(raw, offset)
    }
}

fn parse_xref_table(
    raw: &[u8],
    xref_pos: usize,
) -> Result<(CosDict, HashMap<u32, XrefEntry>, usize)> {
    let mut lexer = Lexer::new(raw, xref_pos + 4);
    let mut entries = HashMap::new();

    loop {
        lexer.skip_ws();
        if raw
            .get(lexer.pos..)
            .is_some_and(|s| s.starts_with(b"trailer"))
        {
            lexer.pos += b"trailer".len();
            break;
        }
        let Token::Obj(CosObj::Int(start_num)) = lexer.next_token()? else {
            return Err(Error::new("expected subsection start object number in xref"));
        };
        let Token::Obj(CosObj::Int(count)) = lexer.next_token()? else {
            return Err(Error::new("expected subsection count in xref"));
        };
        if start_num < 0 || count < 0 {
            return Err(Error::new("negative xref subsection range"));
        }
        for i in 0..(count as u32) {
            let Token::Obj(CosObj::Int(field1)) = lexer.next_token()? else {
                return Err(Error::new("expected offset in xref entry"));
            };
            let Token::Obj(CosObj::Int(field2)) = lexer.next_token()? else {
                return Err(Error::new("expected generation in xref entry"));
            };
            let Token::Keyword(flag) = lexer.next_token()? else {
                return Err(Error::new("expected 'n' or 'f' in xref entry"));
            };
            let obj_num = start_num as u32 + i;
            let generation = (field2.clamp(0, u16::MAX as i64)) as u16;
            let entry = if flag == b"n" {
                XrefEntry::InUse {
                    offset: field1.max(0) as usize,
                    generation,
                }
            } else {
                XrefEntry::Free {
                    next_free: field1.max(0) as u32,
                    generation,
                }
            };
            entries.insert(obj_num, entry);
        }
    }

    let Token::Obj(CosObj::Dict(trailer)) = lexer.next_token()? else {
        return Err(Error::new("expected trailer dictionary after xref"));
    };
    Ok((trailer, entries, lexer.pos))
}

fn parse_xref_stream(
    raw: &[u8],
    offset: usize,
) -> Result<(CosDict, HashMap<u32, XrefEntry>, usize)> {
    let (_, obj) = parse_indirect_object(raw, offset, 0)?;
    let CosObj::Stream { dict, data } = obj else {
        return Err(Error::new("expected XRef stream object"));
    };
    if dict.get_name(b"Type") != Some(b"XRef") {
        return Err(Error::new("stream at xref offset is not /Type /XRef"));
    }

    let decoded = decode_stream_bytes(&dict, &data, raw)?;
    let w_arr = dict
        .get_array(b"W")
        .ok_or_else(|| Error::new("XRef stream missing /W array"))?;
    if w_arr.len() != 3 {
        return Err(Error::new("XRef stream /W must have 3 elements"));
    }
    let w: [usize; 3] = [
        w_arr[0].as_int().unwrap_or(0).max(0) as usize,
        w_arr[1].as_int().unwrap_or(0).max(0) as usize,
        w_arr[2].as_int().unwrap_or(0).max(0) as usize,
    ];
    let entry_len = w[0] + w[1] + w[2];
    if entry_len == 0 {
        return Err(Error::new("XRef stream entry width is 0"));
    }

    let size = dict.get_int(b"Size").unwrap_or(0).max(0) as u32;
    let index_pairs: Vec<(u32, u32)> = match dict.get_array(b"Index") {
        Some(arr) => arr
            .chunks_exact(2)
            .map(|c| {
                (
                    c[0].as_int().unwrap_or(0).max(0) as u32,
                    c[1].as_int().unwrap_or(0).max(0) as u32,
                )
            })
            .collect(),
        None => vec![(0, size)],
    };

    let mut entries = HashMap::new();
    let mut cursor = 0usize;
    for (start_num, count) in index_pairs {
        for i in 0..count {
            if cursor + entry_len > decoded.len() {
                break;
            }
            let slice = &decoded[cursor..cursor + entry_len];
            cursor += entry_len;

            let f0 = if w[0] == 0 {
                1
            } else {
                read_be_uint(&slice[..w[0]])
            };
            let f1 = read_be_uint(&slice[w[0]..w[0] + w[1]]);
            let f2 = read_be_uint(&slice[w[0] + w[1]..]);

            let obj_num = start_num + i;
            let entry = match f0 {
                0 => XrefEntry::Free {
                    next_free: f1 as u32,
                    generation: f2 as u16,
                },
                1 => XrefEntry::InUse {
                    offset: f1 as usize,
                    generation: f2 as u16,
                },
                2 => XrefEntry::Compressed {
                    obj_stm_num: f1 as u32,
                    index: f2 as u16,
                },
                _ => continue,
            };
            entries.insert(obj_num, entry);
        }
    }

    let section_end = match data {
        StreamData::RawSlice { range, .. } => range.end,
        StreamData::Owned { .. } => offset,
    };
    let eof_search = find_bytes(raw, b"endobj", section_end).unwrap_or(section_end);
    // Skip whitespace just past endobj
    let mut end_pos = eof_search;
    while raw.get(end_pos).is_some_and(|&b| !is_whitespace(b)) {
        end_pos += 1;
    }
    Ok((dict, entries, end_pos))
}

fn read_be_uint(bytes: &[u8]) -> u64 {
    let mut v = 0u64;
    for &b in bytes {
        v = (v << 8) | u64::from(b);
    }
    v
}

/// Decode stream bytes for supported filters (`/FlateDecode` + optional PNG predictor).
pub(crate) fn decode_stream_bytes(
    dict: &CosDict,
    data: &StreamData,
    raw: &[u8],
) -> Result<Vec<u8>> {
    let encoded_flag = match data {
        StreamData::RawSlice { encoded, .. } | StreamData::Owned { encoded, .. } => *encoded,
    };
    let slice: &[u8] = match data {
        StreamData::RawSlice { range, .. } => raw
            .get(range.clone())
            .ok_or_else(|| Error::new("stream range out of bounds"))?,
        StreamData::Owned { bytes, .. } => bytes.as_slice(),
    };
    if !encoded_flag {
        return Ok(slice.to_vec());
    }

    let filters: Vec<&[u8]> = match dict.get(b"Filter") {
        None => return Ok(slice.to_vec()),
        Some(CosObj::Name(n)) => vec![n.as_slice()],
        Some(CosObj::Array(arr)) => arr
            .iter()
            .filter_map(|o| match o {
                CosObj::Name(n) => Some(n.as_slice()),
                _ => None,
            })
            .collect(),
        _ => return Err(Error::new("unsupported /Filter type")),
    };

    let mut current = slice.to_vec();
    for filter in filters {
        match filter {
            b"FlateDecode" | b"Fl" => {
                current = miniz_oxide::inflate::decompress_to_vec_zlib(&current)
                    .or_else(|_| miniz_oxide::inflate::decompress_to_vec(&current))
                    .map_err(|_| Error::new("FlateDecode decompression failed"))?;
            }
            other => {
                return Err(Error::new(format!(
                    "unsupported stream filter: {}",
                    String::from_utf8_lossy(other)
                )));
            }
        }
    }

    if let Some(parms) = dict.get(b"DecodeParms").and_then(|o| o.as_dict())
        && let Some(predictor) = parms.get_int(b"Predictor")
        && predictor >= 10
    {
        let columns = parms.get_int(b"Columns").unwrap_or(1).max(1) as usize;
        let colors = parms.get_int(b"Colors").unwrap_or(1).max(1) as usize;
        let bpc = parms.get_int(b"BitsPerComponent").unwrap_or(8).max(1) as usize;
        let bpp = (colors * bpc).div_ceil(8).max(1);
        let row_bytes = (columns * colors * bpc).div_ceil(8);
        current = apply_png_predictor(&current, row_bytes, bpp)?;
    }

    Ok(current)
}

fn apply_png_predictor(data: &[u8], row_bytes: usize, bpp: usize) -> Result<Vec<u8>> {
    let stride = row_bytes + 1;
    if stride == 0 {
        return Ok(Vec::new());
    }
    let rows = data.len() / stride;
    let mut out = Vec::with_capacity(rows * row_bytes);
    let mut prev = vec![0u8; row_bytes];
    let mut curr = vec![0u8; row_bytes];

    for r in 0..rows {
        let row = &data[r * stride..(r + 1) * stride];
        let filter = row[0];
        let src = &row[1..];
        for i in 0..row_bytes {
            let left = if i >= bpp { curr[i - bpp] } else { 0 };
            let up = prev[i];
            let up_left = if i >= bpp { prev[i - bpp] } else { 0 };
            curr[i] = match filter {
                0 => src[i],
                1 => src[i].wrapping_add(left),
                2 => src[i].wrapping_add(up),
                3 => src[i].wrapping_add(((u16::from(left) + u16::from(up)) / 2) as u8),
                4 => src[i].wrapping_add(paeth(left, up, up_left)),
                _ => {
                    return Err(Error::new(format!(
                        "unsupported PNG predictor filter {filter}"
                    )));
                }
            };
        }
        out.extend_from_slice(&curr);
        prev.copy_from_slice(&curr);
    }
    Ok(out)
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (a, b, c) = (i32::from(a), i32::from(b), i32::from(c));
    let p = a + b - c;
    let pa = (p - a).abs();
    let pb = (p - b).abs();
    let pc = (p - c).abs();
    if pa <= pb && pa <= pc {
        a as u8
    } else if pb <= pc {
        b as u8
    } else {
        c as u8
    }
}
