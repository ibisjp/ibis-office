// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The first page of a PDF read into an SVG (paths, text as glyph outlines, JPEG and raw pictures), so a PDF on the clipboard
//! (what Office for Mac puts there for a drawing or a copied picture of cells) is pasted as a vector picture, not as the bitmap
//! beside it. Only what a picture of cells or a drawing holds is read: path and text operators, device and ICC colours,
//! transparency constants, form XObjects; shadings, patterns and fonts that are not TrueType / OpenType are left out.
//! One unit of the SVG is a point of the page; its size is in pixels (96 per inch), so the picture keeps its size.

use std::collections::HashMap;
use std::fmt::Write;

#[derive(Clone, Debug)]
enum Obj {
    Null,
    Bool,
    Num(f64),
    Name(String),
    Str(Vec<u8>),
    Arr(Vec<Obj>),
    Dict(HashMap<String, Obj>),
    Stream(HashMap<String, Obj>, std::rc::Rc<Vec<u8>>),
    Ref(u32),
}

impl Obj {
    fn num(&self) -> Option<f64> {
        if let Obj::Num(n) = self { Some(*n) } else { None }
    }
}

enum Token {
    Obj(Obj),
    Keyword(String),
    End,
}

struct Lexer<'a> {
    data: &'a [u8],
    pos: usize,
    /// In a content stream `N G R` is three operands, not a reference.
    content: bool,
}

fn is_space(b: u8) -> bool {
    matches!(b, 0 | 9 | 10 | 12 | 13 | 32)
}
fn is_delim(b: u8) -> bool {
    matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

impl<'a> Lexer<'a> {
    fn new(data: &'a [u8], pos: usize, content: bool) -> Self {
        Self { data, pos, content }
    }
    fn skip(&mut self) {
        while let Some(&b) = self.data.get(self.pos) {
            if is_space(b) {
                self.pos += 1;
            } else if b == b'%' {
                while self.data.get(self.pos).is_some_and(|&c| c != b'\n' && c != b'\r') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }
    fn word(&mut self) -> &'a [u8] {
        let start = self.pos;
        while self.data.get(self.pos).is_some_and(|&b| !is_space(b) && !is_delim(b)) {
            self.pos += 1;
        }
        &self.data[start..self.pos]
    }
    fn next(&mut self) -> Token {
        self.skip();
        let Some(&b) = self.data.get(self.pos) else { return Token::End };
        match b {
            b'/' => {
                self.pos += 1;
                let raw = self.word();
                let mut name = String::new();
                let mut i = 0;
                while i < raw.len() {
                    if raw[i] == b'#'
                        && i + 2 < raw.len()
                        && let Some(v) = std::str::from_utf8(&raw[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())
                    {
                        name.push(v as char);
                        i += 3;
                    } else {
                        name.push(raw[i] as char);
                        i += 1;
                    }
                }
                Token::Obj(Obj::Name(name))
            }
            b'(' => Token::Obj(Obj::Str(self.string())),
            b'<' if self.data.get(self.pos + 1) == Some(&b'<') => {
                self.pos += 2;
                let mut dict = HashMap::new();
                loop {
                    match self.next() {
                        Token::Obj(Obj::Name(key)) => match self.next() {
                            Token::Obj(value) => {
                                dict.insert(key, value);
                            }
                            _ => break,
                        },
                        Token::Keyword(k) if k == ">>" => break,
                        Token::End => break,
                        _ => {}
                    }
                }
                Token::Obj(Obj::Dict(dict))
            }
            b'<' => {
                self.pos += 1;
                let mut digits = Vec::new();
                while let Some(&c) = self.data.get(self.pos) {
                    self.pos += 1;
                    if c == b'>' {
                        break;
                    }
                    if let Some(v) = (c as char).to_digit(16) {
                        digits.push(v as u8);
                    }
                }
                let bytes = digits.chunks(2).map(|c| (c[0] << 4) | c.get(1).copied().unwrap_or(0)).collect();
                Token::Obj(Obj::Str(bytes))
            }
            b'>' if self.data.get(self.pos + 1) == Some(&b'>') => {
                self.pos += 2;
                Token::Keyword(">>".into())
            }
            b'[' => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    match self.next() {
                        Token::Obj(o) => items.push(o),
                        Token::Keyword(k) if k == "]" => break,
                        Token::End => break,
                        Token::Keyword(_) => {}
                    }
                }
                Token::Obj(Obj::Arr(items))
            }
            b']' => {
                self.pos += 1;
                Token::Keyword("]".into())
            }
            b')' | b'{' | b'}' | b'>' => {
                self.pos += 1;
                Token::Keyword((b as char).to_string())
            }
            _ => {
                let word = self.word();
                if word.is_empty() {
                    self.pos += 1;
                    return Token::Keyword(String::new());
                }
                let text = String::from_utf8_lossy(word).to_string();
                if let Ok(n) = text.parse::<f64>() {
                    // `12 0 R`: a reference (an object's body, not a content stream).
                    if !self.content && n >= 0.0 && n.fract() == 0.0 {
                        let save = self.pos;
                        self.skip();
                        let generation = self.word();
                        if !generation.is_empty() && generation.iter().all(u8::is_ascii_digit) {
                            self.skip();
                            if self.data.get(self.pos) == Some(&b'R') && self.data.get(self.pos + 1).is_none_or(|&c| is_space(c) || is_delim(c)) {
                                self.pos += 1;
                                return Token::Obj(Obj::Ref(n as u32));
                            }
                        }
                        self.pos = save;
                    }
                    return Token::Obj(Obj::Num(n));
                }
                match text.as_str() {
                    "true" | "false" => Token::Obj(Obj::Bool),
                    "null" => Token::Obj(Obj::Null),
                    _ => Token::Keyword(text),
                }
            }
        }
    }
    fn string(&mut self) -> Vec<u8> {
        self.pos += 1;
        let mut depth = 1;
        let mut out = Vec::new();
        while let Some(&c) = self.data.get(self.pos) {
            self.pos += 1;
            match c {
                b'\\' => {
                    let Some(&e) = self.data.get(self.pos) else { break };
                    self.pos += 1;
                    match e {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'0'..=b'7' => {
                            let mut v = u32::from(e - b'0');
                            for _ in 0..2 {
                                match self.data.get(self.pos) {
                                    Some(&d @ b'0'..=b'7') => {
                                        v = v * 8 + u32::from(d - b'0');
                                        self.pos += 1;
                                    }
                                    _ => break,
                                }
                            }
                            out.push(v as u8);
                        }
                        b'\r' => {
                            if self.data.get(self.pos) == Some(&b'\n') {
                                self.pos += 1;
                            }
                        }
                        b'\n' => {}
                        other => out.push(other),
                    }
                }
                b'(' => {
                    depth += 1;
                    out.push(c);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    out.push(c);
                }
                _ => out.push(c),
            }
        }
        out
    }
}

struct Doc {
    objects: HashMap<u32, Obj>,
    trailer_root: Option<u32>,
}

fn find(data: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    data.get(from..)?.windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

impl Doc {
    fn parse(data: &[u8]) -> Option<Doc> {
        let mut doc = Doc { objects: HashMap::new(), trailer_root: None };
        // Every `N G obj` in the file, the last of a number winning (an updated file repeats objects).
        let mut at = 0;
        while let Some(p) = find(data, b" obj", at) {
            at = p + 4;
            if data.get(at).is_some_and(|&c| !is_space(c) && !is_delim(c)) {
                continue;
            }
            // Back over `N G`.
            let mut s = p;
            let mut words = 0;
            while s > 0 && words < 2 {
                while s > 0 && data[s - 1].is_ascii_digit() {
                    s -= 1;
                }
                words += 1;
                if words < 2 {
                    if s > 0 && is_space(data[s - 1]) {
                        while s > 0 && is_space(data[s - 1]) {
                            s -= 1;
                        }
                    } else {
                        words = 9;
                    }
                }
            }
            if words != 2 {
                continue;
            }
            let Some(number) = std::str::from_utf8(&data[s..p]).ok().and_then(|t| t.split_whitespace().next()?.parse::<u32>().ok()) else { continue };
            let mut lexer = Lexer::new(data, at, false);
            let Token::Obj(obj) = lexer.next() else { continue };
            let obj = match obj {
                Obj::Dict(dict) => {
                    lexer.skip();
                    if data[lexer.pos..].starts_with(b"stream") {
                        let mut start = lexer.pos + 6;
                        if data.get(start) == Some(&b'\r') {
                            start += 1;
                        }
                        if data.get(start) == Some(&b'\n') {
                            start += 1;
                        }
                        let declared = dict.get("Length").and_then(Obj::num).map(|n| n as usize);
                        let end = match declared {
                            Some(n) if data.get(start + n..).is_some_and(|rest| rest.iter().take(4).any(|&c| c == b'e')) && find(data, b"endstream", start + n).is_some_and(|e| e <= start + n + 4) => start + n,
                            _ => find(data, b"endstream", start).unwrap_or(data.len()),
                        };
                        at = end;
                        Obj::Stream(dict, std::rc::Rc::new(data[start..end.min(data.len())].to_vec()))
                    } else {
                        Obj::Dict(dict)
                    }
                }
                other => other,
            };
            doc.objects.insert(number, obj);
        }
        // Objects inside object streams, and the catalogue named by the trailer or a cross-reference stream.
        let streams: Vec<u32> = doc.objects.iter().filter(|(_, o)| matches!(o, Obj::Stream(d, _) if matches!(d.get("Type"), Some(Obj::Name(n)) if n == "ObjStm"))).map(|(n, _)| *n).collect();
        for number in streams {
            let Some(Obj::Stream(dict, _)) = doc.objects.get(&number).cloned() else { continue };
            let Some(bytes) = doc.stream_bytes(&Obj::Stream(dict.clone(), match doc.objects.get(&number) { Some(Obj::Stream(_, d)) => d.clone(), _ => continue })) else { continue };
            let count = dict.get("N").and_then(Obj::num).unwrap_or(0.0) as usize;
            let first = dict.get("First").and_then(Obj::num).unwrap_or(0.0) as usize;
            let mut head = Lexer::new(&bytes, 0, true);
            let mut entries = Vec::new();
            for _ in 0..count {
                let (Token::Obj(Obj::Num(n)), Token::Obj(Obj::Num(o))) = (head.next(), head.next()) else { break };
                entries.push((n as u32, o as usize));
            }
            for (n, offset) in entries {
                let mut lexer = Lexer::new(&bytes, first + offset, false);
                if let Token::Obj(obj) = lexer.next() {
                    doc.objects.entry(n).or_insert(obj);
                }
            }
        }
        let mut at = 0;
        while let Some(p) = find(data, b"trailer", at) {
            at = p + 7;
            let mut lexer = Lexer::new(data, at, false);
            if let Token::Obj(Obj::Dict(d)) = lexer.next()
                && let Some(Obj::Ref(r)) = d.get("Root")
            {
                doc.trailer_root = Some(*r);
            }
        }
        if doc.trailer_root.is_none() {
            for obj in doc.objects.values() {
                if let Obj::Stream(d, _) = obj
                    && matches!(d.get("Type"), Some(Obj::Name(n)) if n == "XRef")
                    && let Some(Obj::Ref(r)) = d.get("Root")
                {
                    doc.trailer_root = Some(*r);
                }
            }
        }
        Some(doc)
    }

    fn get<'a>(&'a self, obj: &'a Obj) -> &'a Obj {
        let mut obj = obj;
        for _ in 0..16 {
            match obj {
                Obj::Ref(n) => match self.objects.get(n) {
                    Some(o) => obj = o,
                    None => return &Obj::Null,
                },
                other => return other,
            }
        }
        &Obj::Null
    }
    fn dict_of<'a>(&'a self, obj: &'a Obj) -> Option<&'a HashMap<String, Obj>> {
        match self.get(obj) {
            Obj::Dict(d) | Obj::Stream(d, _) => Some(d),
            _ => None,
        }
    }
    fn entry<'a>(&'a self, dict: &'a HashMap<String, Obj>, key: &str) -> &'a Obj {
        dict.get(key).map_or(&Obj::Null, |o| self.get(o))
    }
    fn numbers(&self, obj: &Obj) -> Vec<f64> {
        match self.get(obj) {
            Obj::Arr(a) => a.iter().filter_map(|o| self.get(o).num()).collect(),
            _ => Vec::new(),
        }
    }

    /// A stream's bytes with its filters undone (Flate, with a PNG predictor; JPEG data is left as it is: see `is_jpeg`).
    fn stream_bytes(&self, stream: &Obj) -> Option<Vec<u8>> {
        let Obj::Stream(dict, raw) = self.get(stream) else { return None };
        let filters: Vec<String> = match self.entry(dict, "Filter") {
            Obj::Name(n) => vec![n.clone()],
            Obj::Arr(a) => a.iter().filter_map(|o| if let Obj::Name(n) = self.get(o) { Some(n.clone()) } else { None }).collect(),
            _ => Vec::new(),
        };
        let params: Vec<Option<&HashMap<String, Obj>>> = match self.entry(dict, "DecodeParms") {
            Obj::Arr(a) => a.iter().map(|o| self.dict_of(o)).collect(),
            Obj::Dict(d) => vec![Some(d)],
            _ => Vec::new(),
        };
        let mut data = raw.as_ref().clone();
        for (i, filter) in filters.iter().enumerate() {
            match filter.as_str() {
                "FlateDecode" | "Fl" => {
                    data = match miniz_oxide::inflate::decompress_to_vec_zlib(&data) {
                        Ok(d) => d,
                        Err(e) => e.output,
                    };
                    if let Some(Some(p)) = params.get(i) {
                        let predictor = self.entry(p, "Predictor").num().unwrap_or(1.0) as u32;
                        if predictor >= 10 {
                            let columns = self.entry(p, "Columns").num().unwrap_or(1.0) as usize;
                            let colors = self.entry(p, "Colors").num().unwrap_or(1.0) as usize;
                            let bits = self.entry(p, "BitsPerComponent").num().unwrap_or(8.0) as usize;
                            data = png_predictor(&data, columns, (colors * bits).div_ceil(8).max(1), (colors * bits * columns).div_ceil(8));
                        }
                    }
                }
                // A picture's own compression: left to the one that reads the picture.
                "DCTDecode" | "DCT" => {}
                _ => return None,
            }
        }
        Some(data)
    }

    fn first_page(&self) -> Option<&HashMap<String, Obj>> {
        if let Some(root) = self.trailer_root
            && let Some(Obj::Dict(catalog)) = self.objects.get(&root)
        {
            let mut node = self.dict_of(catalog.get("Pages")?)?;
            for _ in 0..32 {
                if matches!(self.entry(node, "Type"), Obj::Name(n) if n == "Page") || !node.contains_key("Kids") {
                    return Some(node);
                }
                let Obj::Arr(kids) = self.entry(node, "Kids") else { return None };
                node = self.dict_of(kids.first()?)?;
            }
        }
        // No catalogue: the page of the lowest object number.
        let mut pages: Vec<(&u32, &HashMap<String, Obj>)> = self.objects.iter().filter_map(|(n, o)| if let Obj::Dict(d) = o && matches!(d.get("Type"), Some(Obj::Name(t)) if t == "Page") { Some((n, d)) } else { None }).collect();
        pages.sort_by_key(|(n, _)| **n);
        pages.first().map(|(_, d)| *d)
    }

    /// A page attribute, inherited from the page tree.
    fn inherited<'a>(&'a self, page: &'a HashMap<String, Obj>, key: &str) -> &'a Obj {
        let mut node = page;
        for _ in 0..32 {
            if let Some(v) = node.get(key) {
                return self.get(v);
            }
            match node.get("Parent").and_then(|p| self.dict_of(p)) {
                Some(parent) => node = parent,
                None => break,
            }
        }
        &Obj::Null
    }
}

/// The rows of a PNG-predicted stream (filters 0..4 per row) unfiltered.
fn png_predictor(data: &[u8], _columns: usize, bpp: usize, row: usize) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(data.len());
    let mut previous = vec![0u8; row];
    for chunk in data.chunks(row + 1) {
        if chunk.len() < 2 {
            break;
        }
        let kind = chunk[0];
        let mut line = chunk[1..].to_vec();
        line.resize(row, 0);
        for i in 0..row {
            let left = if i >= bpp { line[i - bpp] } else { 0 };
            let up = previous[i];
            let up_left = if i >= bpp { previous[i - bpp] } else { 0 };
            let add = match kind {
                1 => left,
                2 => up,
                3 => ((u16::from(left) + u16::from(up)) / 2) as u8,
                4 => {
                    let (a, b, c) = (i32::from(left), i32::from(up), i32::from(up_left));
                    let p = a + b - c;
                    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                    (if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c }) as u8
                }
                _ => 0,
            };
            line[i] = line[i].wrapping_add(add);
        }
        out.extend_from_slice(&line);
        previous = line;
    }
    out
}

type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
/// `a` then `b` (the PDF product: a user-space point goes through `a`, then `b`).
fn multiply(a: Matrix, b: Matrix) -> Matrix {
    [a[0] * b[0] + a[1] * b[2], a[0] * b[1] + a[1] * b[3], a[2] * b[0] + a[3] * b[2], a[2] * b[1] + a[3] * b[3], a[4] * b[0] + a[5] * b[2] + b[4], a[4] * b[1] + a[5] * b[3] + b[5]]
}
fn matrix_attr(m: Matrix) -> String {
    format!("matrix({:.5} {:.5} {:.5} {:.5} {:.4} {:.4})", m[0], m[1], m[2], m[3], m[4], m[5])
}

#[derive(Clone)]
struct State {
    ctm: Matrix,
    fill: String,
    stroke: String,
    fill_alpha: f64,
    stroke_alpha: f64,
    line_width: f64,
    cap: u8,
    join: u8,
    miter: f64,
    dash: Vec<f64>,
    dash_phase: f64,
    font: Option<std::rc::Rc<Font>>,
    size: f64,
    char_space: f64,
    word_space: f64,
    scale: f64,
    leading: f64,
    rise: f64,
    render: i64,
    /// Clip groups opened since the matching `q`.
    clips: usize,
}

impl State {
    fn new(ctm: Matrix) -> Self {
        Self { ctm, fill: "#000000".into(), stroke: "#000000".into(), fill_alpha: 1.0, stroke_alpha: 1.0, line_width: 1.0, cap: 0, join: 0, miter: 10.0, dash: Vec::new(), dash_phase: 0.0, font: None, size: 0.0, char_space: 0.0, word_space: 0.0, scale: 1.0, leading: 0.0, rise: 0.0, render: 0, clips: 0 }
    }
}

struct Font {
    data: Vec<u8>,
    index: u32,
    /// Two-byte codes (a Type 0 font) rather than one.
    wide: bool,
    /// A CIDToGIDMap stream; none means the code is the glyph.
    cid_to_gid: Option<Vec<u8>>,
    widths: HashMap<u32, f64>,
    default_width: f64,
    first_char: u32,
    /// A symbolic or Mac-encoded font: the code goes through the font's own cmap.
    glyph_cache: std::cell::RefCell<HashMap<u16, Option<String>>>,
}

impl Font {
    fn load(doc: &Doc, dict: &HashMap<String, Obj>) -> Option<Font> {
        let subtype = match doc.entry(dict, "Subtype") {
            Obj::Name(n) => n.clone(),
            _ => String::new(),
        };
        let (wide, face_dict, descendant) = if subtype == "Type0" {
            let Obj::Arr(d) = doc.entry(dict, "DescendantFonts") else { return None };
            let descendant = doc.dict_of(d.first()?)?;
            (true, descendant, Some(descendant))
        } else {
            (false, dict, None)
        };
        let descriptor = doc.dict_of(face_dict.get("FontDescriptor")?)?;
        let file = ["FontFile2", "FontFile3", "FontFile"].iter().find_map(|k| descriptor.get(*k))?;
        let data = doc.stream_bytes(file)?;
        // Only an sfnt (TrueType or OpenType) is read; a bare CFF or Type 1 program is not.
        if !(data.starts_with(&[0, 1, 0, 0]) || data.starts_with(b"OTTO") || data.starts_with(b"true") || data.starts_with(b"ttcf")) {
            return None;
        }
        let mut widths = HashMap::new();
        let default_width;
        let mut first_char = 0;
        if let Some(d) = descendant {
            default_width = doc.entry(d, "DW").num().unwrap_or(1000.0);
            if let Obj::Arr(w) = doc.entry(d, "W") {
                let mut i = 0;
                while i < w.len() {
                    let Some(first) = doc.get(&w[i]).num() else { break };
                    match w.get(i + 1).map(|o| doc.get(o)) {
                        Some(Obj::Arr(list)) => {
                            for (k, v) in list.iter().enumerate() {
                                if let Some(v) = doc.get(v).num() {
                                    widths.insert(first as u32 + k as u32, v);
                                }
                            }
                            i += 2;
                        }
                        Some(Obj::Num(last)) => {
                            if let Some(v) = w.get(i + 2).and_then(|o| doc.get(o).num()) {
                                for c in first as u32..=*last as u32 {
                                    widths.insert(c, v);
                                }
                            }
                            i += 3;
                        }
                        _ => break,
                    }
                }
            }
        } else {
            first_char = doc.entry(dict, "FirstChar").num().unwrap_or(0.0) as u32;
            for (k, w) in doc.numbers(dict.get("Widths").unwrap_or(&Obj::Null)).into_iter().enumerate() {
                widths.insert(first_char + k as u32, w);
            }
            default_width = doc.entry(descriptor, "MissingWidth").num().unwrap_or(0.0);
        }
        let cid_to_gid = descendant.and_then(|d| match doc.entry(d, "CIDToGIDMap") {
            s @ Obj::Stream(..) => doc.stream_bytes(s),
            _ => None,
        });
        Some(Font { data, index: 0, wide, cid_to_gid, widths, default_width, first_char, glyph_cache: Default::default() })
    }

    fn glyph_id(&self, face: &ttf_parser::Face, code: u32) -> Option<u16> {
        if self.wide {
            return Some(match &self.cid_to_gid {
                Some(map) => u16::from_be_bytes([*map.get(code as usize * 2)?, *map.get(code as usize * 2 + 1)?]),
                None => code as u16,
            });
        }
        let cmap = face.tables().cmap?;
        let ch = char::from_u32(code)?;
        // The font's own cmaps, in the order a reader tries them for a simple font: Windows symbol (0xF000 + code), Mac Roman, Unicode.
        for table in cmap.subtables {
            if table.platform_id == ttf_parser::PlatformId::Windows && table.encoding_id == 0 {
                if let Some(g) = table.glyph_index(0xF000 + code).or_else(|| table.glyph_index(code)) {
                    return Some(g.0);
                }
            }
        }
        for table in cmap.subtables {
            if table.platform_id == ttf_parser::PlatformId::Macintosh
                && let Some(g) = table.glyph_index(code)
            {
                return Some(g.0);
            }
        }
        for table in cmap.subtables {
            if let Some(g) = table.glyph_index(ch as u32) {
                return Some(g.0);
            }
        }
        // A subset whose glyphs are in code order.
        (code != 0 && code < u32::from(face.number_of_glyphs())).then_some(code as u16)
    }

    /// The outline of a glyph in font units, as the data of an SVG path.
    fn outline(&self, face: &ttf_parser::Face, gid: u16) -> Option<String> {
        if let Some(cached) = self.glyph_cache.borrow().get(&gid) {
            return cached.clone();
        }
        struct Svg(String);
        impl ttf_parser::OutlineBuilder for Svg {
            fn move_to(&mut self, x: f32, y: f32) {
                let _ = write!(self.0, "M{x} {y}");
            }
            fn line_to(&mut self, x: f32, y: f32) {
                let _ = write!(self.0, "L{x} {y}");
            }
            fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
                let _ = write!(self.0, "Q{x1} {y1} {x} {y}");
            }
            fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
                let _ = write!(self.0, "C{x1} {y1} {x2} {y2} {x} {y}");
            }
            fn close(&mut self) {
                self.0.push('Z');
            }
        }
        let mut builder = Svg(String::new());
        let d = face.outline_glyph(ttf_parser::GlyphId(gid), &mut builder).map(|_| builder.0);
        self.glyph_cache.borrow_mut().insert(gid, d.clone());
        d
    }

    /// A glyph's advance in thousandths of an em.
    fn width(&self, face: &ttf_parser::Face, code: u32, gid: u16) -> f64 {
        if let Some(w) = self.widths.get(&code) {
            return *w;
        }
        if self.wide || code < self.first_char || self.default_width == 0.0 {
            if let Some(a) = face.glyph_hor_advance(ttf_parser::GlyphId(gid)) {
                return f64::from(a) * 1000.0 / f64::from(face.units_per_em());
            }
        }
        self.default_width
    }
}

fn rgb_of(values: &[f64]) -> String {
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    match values.len() {
        1 => {
            let g = byte(values[0]);
            format!("#{g:02x}{g:02x}{g:02x}")
        }
        3 => format!("#{:02x}{:02x}{:02x}", byte(values[0]), byte(values[1]), byte(values[2])),
        4 => {
            let k = values[3];
            format!("#{:02x}{:02x}{:02x}", byte((1.0 - values[0]) * (1.0 - k)), byte((1.0 - values[1]) * (1.0 - k)), byte((1.0 - values[2]) * (1.0 - k)))
        }
        _ => "#000000".into(),
    }
}

struct Writer<'a> {
    doc: &'a Doc,
    body: String,
    clip_count: usize,
    fonts: HashMap<(u32, String), Option<std::rc::Rc<Font>>>,
    budget: usize,
}

impl Writer<'_> {
    fn run(&mut self, content: &[u8], resources: Option<&HashMap<String, Obj>>, state: &mut State, depth: usize) {
        let mut stack: Vec<State> = Vec::new();
        let mut operands: Vec<Obj> = Vec::new();
        let mut lexer = Lexer::new(content, 0, true);
        let mut path = String::new();
        let mut current = (0.0, 0.0);
        let mut pending_clip: Option<bool> = None;
        let (mut tm, mut tlm) = (IDENTITY, IDENTITY);
        let base_clips = state.clips;
        loop {
            if self.budget == 0 {
                break;
            }
            let op = match lexer.next() {
                Token::End => break,
                Token::Obj(o) => {
                    operands.push(o);
                    continue;
                }
                Token::Keyword(k) => k,
            };
            let n: Vec<f64> = operands.iter().filter_map(Obj::num).collect();
            let get = |i: usize| n.get(i).copied().unwrap_or(0.0);
            match op.as_str() {
                "q" => {
                    stack.push(state.clone());
                    state.clips = 0;
                }
                "Q" => {
                    for _ in 0..state.clips {
                        self.body.push_str("</g>");
                    }
                    if let Some(s) = stack.pop() {
                        *state = s;
                    }
                }
                "cm" if n.len() == 6 => state.ctm = multiply([n[0], n[1], n[2], n[3], n[4], n[5]], state.ctm),
                "w" => state.line_width = get(0),
                "J" => state.cap = get(0) as u8,
                "j" => state.join = get(0) as u8,
                "M" => state.miter = get(0),
                "d" => {
                    state.dash = match operands.first() {
                        Some(Obj::Arr(a)) => a.iter().filter_map(Obj::num).collect(),
                        _ => Vec::new(),
                    };
                    state.dash_phase = operands.get(1).and_then(Obj::num).unwrap_or(0.0);
                }
                "gs" => {
                    if let (Some(Obj::Name(name)), Some(res)) = (operands.first(), resources)
                        && let Obj::Dict(states) = self.doc.entry(res, "ExtGState")
                        && let Some(g) = self.doc.dict_of(states.get(name).unwrap_or(&Obj::Null))
                    {
                        if let Some(a) = self.doc.entry(g, "ca").num() {
                            state.fill_alpha = a;
                        }
                        if let Some(a) = self.doc.entry(g, "CA").num() {
                            state.stroke_alpha = a;
                        }
                        if let Some(w) = self.doc.entry(g, "LW").num() {
                            state.line_width = w;
                        }
                    }
                }
                "g" | "rg" | "k" => state.fill = rgb_of(&n),
                "G" | "RG" | "K" => state.stroke = rgb_of(&n),
                "sc" | "scn" if !n.is_empty() => state.fill = rgb_of(&n),
                "SC" | "SCN" if !n.is_empty() => state.stroke = rgb_of(&n),
                "cs" | "CS" => {
                    let device = |name: &str| match name {
                        "DeviceGray" | "G" | "CalGray" => "#000000",
                        _ => "#000000",
                    };
                    if let Some(Obj::Name(name)) = operands.first() {
                        if op == "cs" { state.fill = device(name).into() } else { state.stroke = device(name).into() }
                    }
                }
                "m" => {
                    let _ = write!(path, "M{} {} ", get(0), get(1));
                    current = (get(0), get(1));
                }
                "l" => {
                    let _ = write!(path, "L{} {} ", get(0), get(1));
                    current = (get(0), get(1));
                }
                "c" => {
                    let _ = write!(path, "C{} {} {} {} {} {} ", get(0), get(1), get(2), get(3), get(4), get(5));
                    current = (get(4), get(5));
                }
                "v" => {
                    let _ = write!(path, "C{} {} {} {} {} {} ", current.0, current.1, get(0), get(1), get(2), get(3));
                    current = (get(2), get(3));
                }
                "y" => {
                    let _ = write!(path, "C{} {} {} {} {} {} ", get(0), get(1), get(2), get(3), get(2), get(3));
                    current = (get(2), get(3));
                }
                "h" => path.push_str("Z "),
                "re" => {
                    let (x, y, w, h) = (get(0), get(1), get(2), get(3));
                    let _ = write!(path, "M{x} {y}L{} {y}L{} {}L{x} {}Z ", x + w, x + w, y + h, y + h);
                    current = (x, y);
                }
                "W" => pending_clip = Some(false),
                "W*" => pending_clip = Some(true),
                "S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*" | "n" => {
                    if matches!(op.as_str(), "s" | "b" | "b*") {
                        path.push_str("Z ");
                    }
                    if !path.is_empty() {
                        let fill = matches!(op.as_str(), "f" | "F" | "f*" | "B" | "B*" | "b" | "b*");
                        let stroke = matches!(op.as_str(), "S" | "s" | "B" | "B*" | "b" | "b*");
                        let even_odd = matches!(op.as_str(), "f*" | "B*" | "b*");
                        if fill || stroke {
                            self.emit_path(&path, state, fill, stroke, even_odd);
                        }
                        if let Some(even) = pending_clip.take() {
                            self.clip_count += 1;
                            let id = self.clip_count;
                            let _ = write!(self.body, "<clipPath id=\"c{id}\" clipPathUnits=\"userSpaceOnUse\"><path d=\"{path}\" transform=\"{}\" clip-rule=\"{}\"/></clipPath><g clip-path=\"url(#c{id})\">", matrix_attr(state.ctm), if even { "evenodd" } else { "nonzero" });
                            state.clips += 1;
                        }
                    }
                    pending_clip = None;
                    path.clear();
                }
                "BT" => {
                    tm = IDENTITY;
                    tlm = IDENTITY;
                }
                "Tc" => state.char_space = get(0),
                "Tw" => state.word_space = get(0),
                "Tz" => state.scale = get(0) / 100.0,
                "TL" => state.leading = get(0),
                "Ts" => state.rise = get(0),
                "Tr" => state.render = get(0) as i64,
                "Tf" => {
                    state.size = get(0);
                    state.font = match (operands.first(), resources) {
                        (Some(Obj::Name(name)), Some(res)) => self.font(res, name),
                        _ => None,
                    };
                }
                "Td" | "TD" => {
                    if op == "TD" {
                        state.leading = -get(1);
                    }
                    tlm = multiply([1.0, 0.0, 0.0, 1.0, get(0), get(1)], tlm);
                    tm = tlm;
                }
                "Tm" if n.len() == 6 => {
                    tm = [n[0], n[1], n[2], n[3], n[4], n[5]];
                    tlm = tm;
                }
                "T*" => {
                    tlm = multiply([1.0, 0.0, 0.0, 1.0, 0.0, -state.leading], tlm);
                    tm = tlm;
                }
                "Tj" | "'" | "\"" | "TJ" => {
                    if op != "Tj" && op != "TJ" {
                        tlm = multiply([1.0, 0.0, 0.0, 1.0, 0.0, -state.leading], tlm);
                        tm = tlm;
                        if op == "\"" {
                            state.word_space = get(0);
                            state.char_space = get(1);
                        }
                    }
                    let items: Vec<Obj> = match operands.last() {
                        Some(Obj::Arr(a)) => a.clone(),
                        Some(o) => vec![o.clone()],
                        None => Vec::new(),
                    };
                    for item in items {
                        match item {
                            Obj::Str(bytes) => self.show(&bytes, state, &mut tm),
                            Obj::Num(adjust) => tm = multiply([1.0, 0.0, 0.0, 1.0, -adjust / 1000.0 * state.size * state.scale, 0.0], tm),
                            _ => {}
                        }
                    }
                }
                "Do" => {
                    if let (Some(Obj::Name(name)), Some(res)) = (operands.first(), resources)
                        && let Obj::Dict(xobjects) = self.doc.entry(res, "XObject")
                        && let Some(x) = xobjects.get(name)
                    {
                        self.xobject(x, resources, state, depth);
                    }
                }
                "BI" => {
                    // An inline picture is skipped to its `EI`.
                    let rest = &content[lexer.pos.min(content.len())..];
                    if let Some(p) = find(rest, b"EI", 0) {
                        lexer.pos += p + 2;
                    }
                }
                _ => {}
            }
            operands.clear();
        }
        for _ in 0..state.clips {
            self.body.push_str("</g>");
        }
        state.clips = base_clips;
        for s in stack.into_iter().rev() {
            for _ in 0..s.clips {
                self.body.push_str("</g>");
            }
        }
    }

    fn style(state: &State, fill: bool, stroke: bool, even_odd: bool) -> String {
        let mut s = String::new();
        if fill {
            let _ = write!(s, " fill=\"{}\"", state.fill);
            if state.fill_alpha < 1.0 {
                let _ = write!(s, " fill-opacity=\"{:.3}\"", state.fill_alpha);
            }
            if even_odd {
                s.push_str(" fill-rule=\"evenodd\"");
            }
        } else {
            s.push_str(" fill=\"none\"");
        }
        if stroke {
            let _ = write!(s, " stroke=\"{}\" stroke-width=\"{}\" stroke-linecap=\"{}\" stroke-linejoin=\"{}\" stroke-miterlimit=\"{}\"", state.stroke, state.line_width.max(0.0), ["butt", "round", "square"][(state.cap as usize).min(2)], ["miter", "round", "bevel"][(state.join as usize).min(2)], state.miter);
            if state.stroke_alpha < 1.0 {
                let _ = write!(s, " stroke-opacity=\"{:.3}\"", state.stroke_alpha);
            }
            if !state.dash.is_empty() && state.dash.iter().any(|d| *d > 0.0) {
                let dashes: Vec<String> = state.dash.iter().map(f64::to_string).collect();
                let _ = write!(s, " stroke-dasharray=\"{}\" stroke-dashoffset=\"{}\"", dashes.join(" "), state.dash_phase);
            }
        }
        s
    }

    fn emit_path(&mut self, path: &str, state: &State, fill: bool, stroke: bool, even_odd: bool) {
        self.budget = self.budget.saturating_sub(1);
        let _ = write!(self.body, "<path d=\"{path}\" transform=\"{}\"{}/>", matrix_attr(state.ctm), Self::style(state, fill, stroke, even_odd));
    }

    fn font(&mut self, resources: &HashMap<String, Obj>, name: &str) -> Option<std::rc::Rc<Font>> {
        let Obj::Dict(fonts) = self.doc.entry(resources, "Font") else { return None };
        let reference = fonts.get(name)?;
        let key = (if let Obj::Ref(n) = reference { *n } else { 0 }, name.to_string());
        if let Some(f) = self.fonts.get(&key) {
            return f.clone();
        }
        let font = self.doc.dict_of(reference).and_then(|d| Font::load(self.doc, d)).map(std::rc::Rc::new);
        self.fonts.insert(key, font.clone());
        font
    }

    fn show(&mut self, bytes: &[u8], state: &State, tm: &mut Matrix) {
        let Some(font) = state.font.clone() else { return };
        let Ok(face) = ttf_parser::Face::parse(&font.data, font.index) else { return };
        let upem = f64::from(face.units_per_em()).max(1.0);
        let step = if font.wide { 2 } else { 1 };
        for chunk in bytes.chunks(step) {
            let code = chunk.iter().fold(0u32, |a, b| (a << 8) | u32::from(*b));
            let gid = font.glyph_id(&face, code);
            if let Some(gid) = gid
                && state.render != 3
                && let Some(d) = font.outline(&face, gid)
            {
                // Font units to text space (size per em), the text rise, then the text matrix and the page's.
                let glyph = multiply([state.size * state.scale / upem, 0.0, 0.0, state.size / upem, 0.0, state.rise], multiply(*tm, state.ctm));
                let (fill, stroke) = match state.render {
                    1 | 5 => (false, true),
                    2 | 6 => (true, true),
                    _ => (true, false),
                };
                self.budget = self.budget.saturating_sub(1);
                // A stroke's width is in user space; the glyph's matrix scales by the size, so it is divided back.
                let mut style_state = state.clone();
                let scale = ((glyph[0] * glyph[3] - glyph[1] * glyph[2]).abs()).sqrt();
                let user = ((state.ctm[0] * state.ctm[3] - state.ctm[1] * state.ctm[2]).abs()).sqrt();
                if scale > 0.0 {
                    style_state.line_width = state.line_width * user / scale;
                }
                let _ = write!(self.body, "<path d=\"{d}\" transform=\"{}\"{}/>", matrix_attr(glyph), Self::style(&style_state, fill, stroke, false));
            }
            let w = font.width(&face, code, gid.unwrap_or(0));
            let space = if !font.wide && code == 32 { state.word_space } else { 0.0 };
            let advance = (w / 1000.0 * state.size + state.char_space + space) * state.scale;
            *tm = multiply([1.0, 0.0, 0.0, 1.0, advance, 0.0], *tm);
        }
    }

    fn xobject(&mut self, x: &Obj, resources: Option<&HashMap<String, Obj>>, state: &State, depth: usize) {
        let Some(dict) = self.doc.dict_of(x) else { return };
        match self.doc.entry(dict, "Subtype") {
            Obj::Name(n) if n == "Form" && depth < 8 => {
                let Some(content) = self.doc.stream_bytes(x) else { return };
                let mut inner = state.clone();
                inner.clips = 0;
                let m = self.doc.numbers(dict.get("Matrix").unwrap_or(&Obj::Null));
                if m.len() == 6 {
                    inner.ctm = multiply([m[0], m[1], m[2], m[3], m[4], m[5]], inner.ctm);
                }
                let own = self.doc.dict_of(dict.get("Resources").unwrap_or(&Obj::Null)).or(resources);
                self.run(&content, own, &mut inner, depth + 1);
            }
            Obj::Name(n) if n == "Image" => {
                let (w, h) = (self.doc.entry(dict, "Width").num().unwrap_or(0.0) as usize, self.doc.entry(dict, "Height").num().unwrap_or(0.0) as usize);
                if w == 0 || h == 0 {
                    return;
                }
                let filters = match self.doc.entry(dict, "Filter") {
                    Obj::Name(n) => vec![n.clone()],
                    Obj::Arr(a) => a.iter().filter_map(|o| if let Obj::Name(n) = self.doc.get(o) { Some(n.clone()) } else { None }).collect(),
                    _ => Vec::new(),
                };
                let uri = if filters.last().is_some_and(|f| f == "DCTDecode" || f == "DCT") {
                    let Obj::Stream(_, raw) = self.doc.get(x) else { return };
                    // Filters before the JPEG (none in practice) are not undone.
                    format!("data:image/jpeg;base64,{}", crate::legacy_password::base64(raw))
                } else {
                    let Some(pixels) = self.doc.stream_bytes(x) else { return };
                    let components = match self.doc.entry(dict, "ColorSpace") {
                        Obj::Name(n) if n == "DeviceGray" => 1,
                        Obj::Name(n) if n == "DeviceRGB" => 3,
                        Obj::Arr(a) => match a.first().map(|o| self.doc.get(o)) {
                            Some(Obj::Name(n)) if n == "ICCBased" => a.get(1).and_then(|s| self.doc.dict_of(s)).and_then(|d| self.doc.entry(d, "N").num()).unwrap_or(3.0) as usize,
                            Some(Obj::Name(n)) if n == "CalRGB" => 3,
                            Some(Obj::Name(n)) if n == "CalGray" => 1,
                            _ => return,
                        },
                        _ => return,
                    };
                    if self.doc.entry(dict, "BitsPerComponent").num().unwrap_or(8.0) != 8.0 || (components != 1 && components != 3) || pixels.len() < w * h * components {
                        return;
                    }
                    // The soft mask, a gray picture of the same size, is the alpha.
                    let alpha = dict.get("SMask").and_then(|m| {
                        let md = self.doc.dict_of(m)?;
                        let same = self.doc.entry(md, "Width").num()? as usize == w && self.doc.entry(md, "Height").num()? as usize == h;
                        let bytes = self.doc.stream_bytes(m)?;
                        (same && bytes.len() >= w * h).then_some(bytes)
                    });
                    let mut rgba = Vec::with_capacity(w * h * 4);
                    for i in 0..w * h {
                        let (r, g, b) = if components == 1 { (pixels[i], pixels[i], pixels[i]) } else { (pixels[i * 3], pixels[i * 3 + 1], pixels[i * 3 + 2]) };
                        rgba.extend_from_slice(&[r, g, b, alpha.as_ref().map_or(255, |a| a[i])]);
                    }
                    format!("data:image/png;base64,{}", crate::legacy_password::base64(&crate::dib_picture::rgba_to_png(w as u32, h as u32, &rgba)))
                };
                self.budget = self.budget.saturating_sub(1);
                // The unit square, upside down: a picture's first row is its top.
                let m = multiply([1.0, 0.0, 0.0, -1.0, 0.0, 1.0], state.ctm);
                let _ = write!(self.body, "<image width=\"1\" height=\"1\" preserveAspectRatio=\"none\" transform=\"{}\" href=\"{uri}\"/>", matrix_attr(m));
            }
            _ => {}
        }
    }
}

/// The first page of `pdf` as an SVG, or `None` for bytes that are not a PDF this reads or a page that shows nothing.
pub fn pdf_to_svg(pdf: &[u8]) -> Option<String> {
    if !pdf.starts_with(b"%PDF") {
        return None;
    }
    let doc = Doc::parse(pdf)?;
    let page = doc.first_page()?;
    let media = doc.numbers(doc.inherited(page, "MediaBox"));
    let [x0, y0, x1, y1] = <[f64; 4]>::try_from(media).ok()?;
    let (left, bottom, width, height) = (x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs());
    if width < 1.0 || height < 1.0 {
        return None;
    }
    let mut content = Vec::new();
    let contents = doc.get(page.get("Contents")?);
    match contents {
        Obj::Arr(parts) => {
            for part in parts {
                content.extend(doc.stream_bytes(part)?);
                content.push(b'\n');
            }
        }
        stream => content = doc.stream_bytes(stream)?,
    }
    let resources = match doc.inherited(page, "Resources") {
        Obj::Dict(d) => Some(d),
        _ => None,
    };
    let mut writer = Writer { doc: &doc, body: String::new(), clip_count: 0, fonts: HashMap::new(), budget: 2_000_000 };
    let mut state = State::new(IDENTITY);
    writer.run(&content, resources, &mut state, 0);
    for _ in 0..state.clips {
        writer.body.push_str("</g>");
    }
    if writer.body.is_empty() {
        return None;
    }
    // PDF space is y up from the page's lower left; the SVG's is y down from the upper left. A point is 4/3 of a pixel.
    let pixel = 96.0 / 72.0;
    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.3}\" height=\"{:.3}\" viewBox=\"0 0 {width:.3} {height:.3}\"><g transform=\"matrix(1 0 0 -1 {:.4} {:.4})\">{}</g></svg>",
        width * pixel,
        height * pixel,
        -left,
        bottom + height,
        writer.body
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The PDF Office for Mac put on the pasteboard for a copied drawing (a freeform) reads into an SVG whose path is the drawing.
    #[::core::prelude::v1::test]
    fn the_pdf_of_a_copied_drawing_is_read() {
        let pdf = include_bytes!("../../../test/editing/mac-office-shape.pdf");
        let svg = pdf_to_svg(pdf).expect("svg");
        std::fs::create_dir_all("../../target/pdfpicture").unwrap();
        std::fs::write("../../target/pdfpicture/mac-office-shape.svg", &svg).unwrap();
        assert!(svg.contains("<path") && svg.contains("fill=\"#"), "{svg}");
    }
}

#[cfg(test)]
mod excel_tests {
    use super::*;

    /// The PDF Excel for Mac puts on the pasteboard for a copied range (a TrueType subset, WinAnsi): its text is read as glyph outlines.
    /// The file is in test/github-issue/83 (not committed); the test is skipped where it is not.
    #[::core::prelude::v1::test]
    fn the_pdf_of_copied_cells_is_read() {
        for file in ["excel-mac-copy.pdf", "copyAsFigure/0-com.adobe.pdf.bin"] {
            let Ok(pdf) = std::fs::read(format!("../../test/github-issue/83/{file}")) else { continue };
            let svg = pdf_to_svg(&pdf).expect("svg");
            // The lines of the cells and the glyphs of "A", "B", "1,000", "2,000".
            assert!(svg.matches("<path").count() > 10, "{file}: {svg}");
        }
    }
}
