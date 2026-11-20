// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Low-level PDF COS (Carousel Object System) types, parser, and serializer.

use std::ops::Range;

use crate::{Error, Result};

/// Indirect object identifier `(object_number, generation_number)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId {
    pub num: u32,
    pub generation: u16,
}

impl ObjectId {
    pub const fn new(num: u32, generation: u16) -> Self {
        Self { num, generation }
    }
}

/// PDF string representation preserving literal vs hexadecimal encoding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CosString {
    Literal(Vec<u8>),
    Hex(Vec<u8>),
}

impl CosString {
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Literal(b) | Self::Hex(b) => b,
        }
    }

    /// Best-effort UTF-16BE (with BOM) or PDFDocEncoding/UTF-8 text decoding.
    pub fn to_text_lossy(&self) -> String {
        let bytes = self.as_bytes();
        if bytes.starts_with(&[0xFE, 0xFF]) {
            let utf16: Vec<u16> = bytes[2..]
                .chunks_exact(2)
                .map(|c| u16::from_be_bytes([c[0], c[1]]))
                .collect();
            return String::from_utf16_lossy(&utf16);
        }
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Order-preserving PDF dictionary keeping unknown keys intact.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CosDict {
    entries: Vec<(Vec<u8>, CosObj)>,
}

impl CosDict {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[(Vec<u8>, CosObj)] {
        &self.entries
    }

    pub fn get(&self, key: &[u8]) -> Option<&CosObj> {
        self.entries
            .iter()
            .rev()
            .find(|(k, _)| k.as_slice() == key)
            .map(|(_, v)| v)
    }

    pub fn get_name(&self, key: &[u8]) -> Option<&[u8]> {
        match self.get(key)? {
            CosObj::Name(n) => Some(n.as_slice()),
            _ => None,
        }
    }

    pub fn get_int(&self, key: &[u8]) -> Option<i64> {
        match self.get(key)? {
            CosObj::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn get_num(&self, key: &[u8]) -> Option<f64> {
        self.get(key)?.as_f64()
    }

    pub fn get_ref(&self, key: &[u8]) -> Option<ObjectId> {
        match self.get(key)? {
            CosObj::Ref(id) => Some(*id),
            _ => None,
        }
    }

    pub fn get_array(&self, key: &[u8]) -> Option<&[CosObj]> {
        match self.get(key)? {
            CosObj::Array(arr) => Some(arr.as_slice()),
            _ => None,
        }
    }

    pub fn set(&mut self, key: impl Into<Vec<u8>>, value: CosObj) {
        let key = key.into();
        if let Some(pos) = self.entries.iter().position(|(k, _)| k == &key) {
            self.entries[pos].1 = value;
        } else {
            self.entries.push((key, value));
        }
    }

    pub fn remove(&mut self, key: &[u8]) -> Option<CosObj> {
        let pos = self.entries.iter().position(|(k, _)| k.as_slice() == key)?;
        Some(self.entries.remove(pos).1)
    }
}

/// Stream payload: either a zero-copy slice of the original PDF or newly owned bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamData {
    /// Byte range in `PdfContainer::raw` (still encoded with `/Filter`).
    RawSlice { range: Range<usize>, encoded: bool },
    /// Newly created or modified bytes.
    Owned { bytes: Vec<u8>, encoded: bool },
}

/// PDF COS object value.
#[derive(Clone, Debug, PartialEq)]
pub enum CosObj {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    Name(Vec<u8>),
    Str(CosString),
    Array(Vec<CosObj>),
    Dict(CosDict),
    Stream { dict: CosDict, data: StreamData },
    Ref(ObjectId),
}

impl CosObj {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Int(n) => Some(*n as f64),
            Self::Real(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_dict(&self) -> Option<&CosDict> {
        match self {
            Self::Dict(d) | Self::Stream { dict: d, .. } => Some(d),
            _ => None,
        }
    }

    pub fn as_dict_mut(&mut self) -> Option<&mut CosDict> {
        match self {
            Self::Dict(d) | Self::Stream { dict: d, .. } => Some(d),
            _ => None,
        }
    }

    pub fn as_ref(&self) -> Option<ObjectId> {
        match self {
            Self::Ref(id) => Some(*id),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[CosObj]> {
        match self {
            Self::Array(a) => Some(a.as_slice()),
            _ => None,
        }
    }
}

pub(crate) fn is_whitespace(b: u8) -> bool {
    matches!(b, 0 | 9 | 10 | 12 | 13 | 32)
}

pub(crate) fn is_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Token {
    Obj(CosObj),
    Keyword(Vec<u8>),
    End,
}

/// Byte-slice PDF lexer.
pub(crate) struct Lexer<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    pub fn skip_ws(&mut self) {
        while let Some(&b) = self.data.get(self.pos) {
            if is_whitespace(b) {
                self.pos += 1;
            } else if b == b'%' {
                while self
                    .data
                    .get(self.pos)
                    .is_some_and(|&c| c != b'\n' && c != b'\r')
                {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn read_word(&mut self) -> &'a [u8] {
        let start = self.pos;
        while self
            .data
            .get(self.pos)
            .is_some_and(|&b| !is_whitespace(b) && !is_delimiter(b))
        {
            self.pos += 1;
        }
        &self.data[start..self.pos]
    }

    fn read_name(&mut self) -> Vec<u8> {
        self.pos += 1; // skip '/'
        let raw = self.read_word();
        let mut out = Vec::with_capacity(raw.len());
        let mut i = 0;
        while i < raw.len() {
            if raw[i] == b'#'
                && i + 2 < raw.len()
                && let (Some(hi), Some(lo)) = (hex_val(raw[i + 1]), hex_val(raw[i + 2]))
            {
                out.push((hi << 4) | lo);
                i += 3;
            } else {
                out.push(raw[i]);
                i += 1;
            }
        }
        out
    }

    fn read_literal_string(&mut self) -> Vec<u8> {
        self.pos += 1; // skip '('
        let mut out = Vec::new();
        let mut depth = 1usize;
        while let Some(&b) = self.data.get(self.pos) {
            self.pos += 1;
            match b {
                b'(' => {
                    depth += 1;
                    out.push(b);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(b);
                }
                b'\\' => {
                    let Some(&esc) = self.data.get(self.pos) else {
                        break;
                    };
                    self.pos += 1;
                    match esc {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(0x08),
                        b'f' => out.push(0x0C),
                        b'(' | b')' | b'\\' => out.push(esc),
                        b'\r' => {
                            if self.data.get(self.pos) == Some(&b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut oct = (esc - b'0') as u16;
                            for _ in 0..2 {
                                if let Some(&d) = self.data.get(self.pos)
                                    && (b'0'..=b'7').contains(&d)
                                {
                                    oct = (oct << 3) | ((d - b'0') as u16);
                                    self.pos += 1;
                                } else {
                                    break;
                                }
                            }
                            out.push((oct & 0xFF) as u8);
                        }
                        other => out.push(other),
                    }
                }
                b'\r' => {
                    if self.data.get(self.pos) == Some(&b'\n') {
                        self.pos += 1;
                    }
                    out.push(b'\n');
                }
                other => out.push(other),
            }
        }
        out
    }

    fn read_hex_string(&mut self) -> Vec<u8> {
        self.pos += 1; // skip '<'
        let mut nibbles = Vec::new();
        while let Some(&b) = self.data.get(self.pos) {
            self.pos += 1;
            if b == b'>' {
                break;
            }
            if let Some(v) = hex_val(b) {
                nibbles.push(v);
            }
        }
        if nibbles.len() % 2 == 1 {
            nibbles.push(0);
        }
        nibbles.chunks_exact(2).map(|c| (c[0] << 4) | c[1]).collect()
    }

    pub fn next_token(&mut self) -> Result<Token> {
        self.skip_ws();
        let Some(&b) = self.data.get(self.pos) else {
            return Ok(Token::End);
        };
        match b {
            b'/' => Ok(Token::Obj(CosObj::Name(self.read_name()))),
            b'(' => Ok(Token::Obj(CosObj::Str(CosString::Literal(
                self.read_literal_string(),
            )))),
            b'<' if self.data.get(self.pos + 1) == Some(&b'<') => {
                self.pos += 2;
                let mut dict = CosDict::new();
                loop {
                    match self.next_token()? {
                        Token::Keyword(k) if k == b">>" => break,
                        Token::Obj(CosObj::Name(key)) => match self.next_token()? {
                            Token::Obj(val) => dict.set(key, val),
                            Token::Keyword(k) if k == b">>" => break,
                            Token::End => {
                                return Err(Error::new("unexpected EOF inside PDF dictionary"));
                            }
                            _ => {}
                        },
                        Token::End => {
                            return Err(Error::new("unterminated PDF dictionary"));
                        }
                        _ => {}
                    }
                }
                Ok(Token::Obj(CosObj::Dict(dict)))
            }
            b'<' => Ok(Token::Obj(CosObj::Str(CosString::Hex(
                self.read_hex_string(),
            )))),
            b'>' if self.data.get(self.pos + 1) == Some(&b'>') => {
                self.pos += 2;
                Ok(Token::Keyword(b">>".to_vec()))
            }
            b'[' => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    match self.next_token()? {
                        Token::Keyword(k) if k == b"]" => break,
                        Token::Obj(o) => items.push(o),
                        Token::End => return Err(Error::new("unterminated PDF array")),
                        _ => {}
                    }
                }
                Ok(Token::Obj(CosObj::Array(items)))
            }
            b']' => {
                self.pos += 1;
                Ok(Token::Keyword(b"]".to_vec()))
            }
            _ => {
                let word = self.read_word();
                match word {
                    b"true" => Ok(Token::Obj(CosObj::Bool(true))),
                    b"false" => Ok(Token::Obj(CosObj::Bool(false))),
                    b"null" => Ok(Token::Obj(CosObj::Null)),
                    _ => {
                        if let Some(obj) = self.try_parse_number_or_ref(word) {
                            Ok(Token::Obj(obj))
                        } else {
                            Ok(Token::Keyword(word.to_vec()))
                        }
                    }
                }
            }
        }
    }

    fn try_parse_number_or_ref(&mut self, word: &[u8]) -> Option<CosObj> {
        let s = std::str::from_utf8(word).ok()?;
        if let Ok(int_val) = s.parse::<i64>() {
            if int_val >= 0 {
                let saved = self.pos;
                self.skip_ws();
                let w2 = self.read_word();
                if let Ok(gen_str) = std::str::from_utf8(w2)
                    && let Ok(gen_val) = gen_str.parse::<u16>()
                {
                    self.skip_ws();
                    let w3 = self.read_word();
                    if w3 == b"R" {
                        return Some(CosObj::Ref(ObjectId::new(int_val as u32, gen_val)));
                    }
                }
                self.pos = saved;
            }
            return Some(CosObj::Int(int_val));
        }
        if let Ok(real_val) = s.parse::<f64>()
            && real_val.is_finite()
        {
            return Some(CosObj::Real(real_val));
        }
        None
    }
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Parse an indirect object starting at `offset` (`N G obj ... endobj`).
/// `base_offset` is the offset of `data[0]` in the overall file (used for `StreamData::RawSlice`).
pub(crate) fn parse_indirect_object(
    data: &[u8],
    offset: usize,
    base_offset: usize,
) -> Result<(ObjectId, CosObj)> {
    let mut lexer = Lexer::new(data, offset);
    let num = match lexer.next_token()? {
        Token::Obj(CosObj::Int(n)) if n >= 0 => n as u32,
        _ => return Err(Error::new("expected object number")),
    };
    let generation = match lexer.next_token()? {
        Token::Obj(CosObj::Int(g)) if (0..=u16::MAX as i64).contains(&g) => g as u16,
        _ => return Err(Error::new("expected generation number")),
    };
    match lexer.next_token()? {
        Token::Keyword(k) if k == b"obj" => {}
        _ => return Err(Error::new("expected 'obj' keyword")),
    }
    let Token::Obj(obj) = lexer.next_token()? else {
        return Err(Error::new("expected object body"));
    };
    let obj = match obj {
        CosObj::Dict(dict) => {
            lexer.skip_ws();
            if data.get(lexer.pos..).is_some_and(|s| s.starts_with(b"stream")) {
                let mut start = lexer.pos + 6;
                if data.get(start) == Some(&b'\r') {
                    start += 1;
                }
                if data.get(start) == Some(&b'\n') {
                    start += 1;
                }
                let end = match dict.get_int(b"Length").filter(|&l| l >= 0) {
                    Some(len) => (start + len as usize).min(data.len()),
                    None => find_bytes(data, b"endstream", start).unwrap_or(data.len()),
                };
                CosObj::Stream {
                    dict,
                    data: StreamData::RawSlice {
                        range: (base_offset + start)..(base_offset + end),
                        encoded: true,
                    },
                }
            } else {
                CosObj::Dict(dict)
            }
        }
        other => other,
    };
    Ok((ObjectId::new(num, generation), obj))
}

pub(crate) fn find_bytes(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Serialize a `CosObj` to PDF syntax (using `raw` when writing `StreamData::RawSlice`).
pub fn write_cos(obj: &CosObj, raw: &[u8], out: &mut Vec<u8>) {
    use std::io::Write as _;
    match obj {
        CosObj::Null => out.extend_from_slice(b"null"),
        CosObj::Bool(true) => out.extend_from_slice(b"true"),
        CosObj::Bool(false) => out.extend_from_slice(b"false"),
        CosObj::Int(n) => {
            let _ = write!(out, "{n}");
        }
        CosObj::Real(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                let _ = write!(out, "{:.1}", n);
            } else {
                let _ = write!(out, "{n}");
            }
        }
        CosObj::Name(name) => {
            out.push(b'/');
            for &b in name {
                if b <= 0x20 || b >= 0x7F || is_delimiter(b) || b == b'#' {
                    let _ = write!(out, "#{b:02X}");
                } else {
                    out.push(b);
                }
            }
        }
        CosObj::Str(CosString::Literal(bytes)) => {
            out.push(b'(');
            for &b in bytes {
                match b {
                    b'(' | b')' | b'\\' => {
                        out.push(b'\\');
                        out.push(b);
                    }
                    b'\n' => out.extend_from_slice(b"\\n"),
                    b'\r' => out.extend_from_slice(b"\\r"),
                    b'\t' => out.extend_from_slice(b"\\t"),
                    _ => out.push(b),
                }
            }
            out.push(b')');
        }
        CosObj::Str(CosString::Hex(bytes)) => {
            out.push(b'<');
            for &b in bytes {
                let _ = write!(out, "{b:02X}");
            }
            out.push(b'>');
        }
        CosObj::Array(arr) => {
            out.push(b'[');
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                write_cos(item, raw, out);
            }
            out.push(b']');
        }
        CosObj::Dict(dict) => write_dict(dict, raw, out),
        CosObj::Stream { dict, data } => {
            let payload: &[u8] = match data {
                StreamData::RawSlice { range, .. } => &raw[range.clone()],
                StreamData::Owned { bytes, .. } => bytes.as_slice(),
            };
            let mut d = dict.clone();
            d.set(b"Length".to_vec(), CosObj::Int(payload.len() as i64));
            write_dict(&d, raw, out);
            out.extend_from_slice(b"\nstream\n");
            out.extend_from_slice(payload);
            out.extend_from_slice(b"\nendstream");
        }
        CosObj::Ref(id) => {
            let _ = write!(out, "{} {} R", id.num, id.generation);
        }
    }
}

fn write_dict(dict: &CosDict, raw: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(b"<<");
    for (k, v) in dict.entries() {
        out.push(b' ');
        write_cos(&CosObj::Name(k.clone()), raw, out);
        out.push(b' ');
        write_cos(v, raw, out);
    }
    out.extend_from_slice(b" >>");
}
