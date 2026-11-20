// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A workbook as an old Excel file (`.xls`, BIFF8 in a compound file), written from MS-XLS and MS-CFB.
//!
//! What is written: the sheets (names, hidden), cells (numbers, text, logical values, errors; formulas as tokens with
//! the value last calculated), merged cells, column widths and row heights (hidden, outline levels), the cell formats
//! (font, fill, borders, alignment, rotation, indent, shrink, number formats, protection), hyperlinks, defined names
//! and print areas, freeze panes, gridlines and headings, zoom, page setup. What is not written (the file format of
//! 1997 cannot hold it or it is not done yet): pictures, charts, shapes, notes, conditional formats, validations and
//! tables are left out, and a formula it has no token for (a function of a later Excel, structured references, array
//! constants…) keeps its value only; [`write_report`] counts what was left out.

mod drawing;
mod features;
mod formula;
mod function_params;

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ibis_doc_model::{BorderLine, CellAlign, CellStyle, CellUnderline, CellValue, CellVerticalAlign, Presentation, Sheet, SheetState, VerticalPosition, sheet_calc::sheet_of_slide};

use crate::Error;
use formula::Refs;

/// What was left out of the file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Formulas written as their values.
    pub formulas_as_values: usize,
    /// Cells past row 65 536 or column IV.
    pub cells_out_of_range: usize,
    /// Pictures, charts and shapes.
    pub drawings: usize,
    /// Notes, conditional formats, validations and tables.
    pub other: usize,
}

// ---------------------------------------------------------------- records

#[derive(Default)]
struct Out {
    buf: Vec<u8>,
}

impl Out {
    fn rec(&mut self, id: u16, data: &[u8]) {
        debug_assert!(data.len() <= 8224, "record {id:#x} is {} bytes", data.len());
        self.buf.extend(id.to_le_bytes());
        self.buf.extend((data.len() as u16).to_le_bytes());
        self.buf.extend(data);
    }

    /// A record longer than a record can be: its first part, then `CONTINUE` records.
    fn rec_split(&mut self, id: u16, data: &[u8]) {
        let mut chunks = data.chunks(8224);
        if let Some(first) = chunks.next() {
            self.rec(id, first);
        }
        for c in chunks {
            self.rec(0x3C, c);
        }
    }

    fn rec_u16(&mut self, id: u16, v: u16) {
        self.rec(id, &v.to_le_bytes());
    }
}

/// An `XLUnicodeString` (a count of 16 bits, the flag, the characters).
fn unicode_string(s: &str) -> Vec<u8> {
    let units: Vec<u16> = s.encode_utf16().take(32767).collect();
    let mut out = (units.len() as u16).to_le_bytes().to_vec();
    if units.iter().all(|&u| u < 0x100) {
        out.push(0);
        out.extend(units.iter().map(|&u| u as u8));
    } else {
        out.push(1);
        out.extend(units.iter().flat_map(|u| u.to_le_bytes()));
    }
    out
}

/// The characters of a string with their flag byte (no count).
fn flagged_chars(s: &str) -> Vec<u8> {
    let u = unicode_string(s);
    u[2..].to_vec()
}

// ------------------------------------------------------------------ styles

#[derive(Clone, PartialEq, Eq, Hash)]
struct FontKey {
    name: String,
    /// Twips.
    height: u16,
    bold: bool,
    italic: bool,
    strike: bool,
    underline: u8,
    script: u16,
    color: u16,
}

struct Palette {
    colors: Vec<u32>,
    used: Vec<bool>,
}

impl Palette {
    fn new() -> Self {
        Self { colors: crate::xls::DEFAULT_PALETTE.to_vec(), used: vec![false; 56] }
    }

    /// The colour number (8 to 63) of `rgb`: the palette's own, or one of its colours changed to it.
    fn index(&mut self, rgb: u32) -> u16 {
        if let Some(i) = self.colors.iter().position(|&c| c == rgb) {
            self.used[i] = true;
            return 8 + i as u16;
        }
        // A colour of the default palette that no format uses (from the last) becomes this one.
        if let Some(i) = (0..56).rev().find(|&i| !self.used[i]) {
            self.colors[i] = rgb;
            self.used[i] = true;
            return 8 + i as u16;
        }
        // All 56 are in use: the nearest.
        let dist = |c: u32| {
            let d = |s: u32| ((c >> s & 0xFF) as i32 - (rgb >> s & 0xFF) as i32).pow(2);
            d(16) + d(8) + d(0)
        };
        let i = (0..56).min_by_key(|&i| dist(self.colors[i])).unwrap_or(0);
        8 + i as u16
    }
}

pub(super) fn border_style(line: BorderLine) -> u8 {
    match line {
        BorderLine::Thin => 1,
        BorderLine::Medium => 2,
        BorderLine::Dashed => 3,
        BorderLine::Dotted => 4,
        BorderLine::Thick => 5,
        BorderLine::Double => 6,
        BorderLine::Hair => 7,
        BorderLine::MediumDashed => 8,
        BorderLine::DashDot => 9,
        BorderLine::MediumDashDot => 10,
        BorderLine::DashDotDot => 11,
        BorderLine::MediumDashDotDot => 12,
        BorderLine::SlantDashDot => 13,
    }
}

pub(super) fn pattern_number(name: &str) -> u8 {
    match name {
        "solid" => 1,
        "mediumGray" => 2,
        "darkGray" => 3,
        "lightGray" => 4,
        "darkHorizontal" => 5,
        "darkVertical" => 6,
        "darkDown" => 7,
        "darkUp" => 8,
        "darkGrid" => 9,
        "darkTrellis" => 10,
        "lightHorizontal" => 11,
        "lightVertical" => 12,
        "lightDown" => 13,
        "lightUp" => 14,
        "lightGrid" => 15,
        "lightTrellis" => 16,
        "gray125" => 17,
        "gray0625" => 18,
        _ => 0,
    }
}

fn builtin_format(code: &str) -> Option<u16> {
    Some(match code {
        "General" | "" => 0,
        "0" => 1,
        "0.00" => 2,
        "#,##0" => 3,
        "#,##0.00" => 4,
        "0%" => 9,
        "0.00%" => 10,
        "0.00E+00" => 11,
        "# ?/?" => 12,
        "# ??/??" => 13,
        "@" => 49,
        _ => return None,
    })
}

struct Book {
    default_font: String,
    fonts: Vec<FontKey>,
    font_index: HashMap<FontKey, usize>,
    formats: Vec<String>,
    xfs: Vec<Vec<u8>>,
    xf_index: HashMap<Vec<u8>, u16>,
    palette: Palette,
}

/// The number of the first cell format of the cells (the ones before it are the styles' formats).
const FIRST_XF: u16 = 15;

impl Book {
    fn new(default: &CellStyle) -> Self {
        let mut b = Self { default_font: default.font.clone().unwrap_or_else(|| "Arial".into()), fonts: Vec::new(), font_index: HashMap::new(), formats: Vec::new(), xfs: Vec::new(), xf_index: HashMap::new(), palette: Palette::new() };
        // The default font is the first (BIFF8 numbers the fonts 0 to 3, then from 5: it is also used for 1 to 3).
        let key = b.font_key(default);
        b.font_index.insert(key.clone(), 0);
        b.fonts.push(key);
        // The first cell format (number 15) is the default one.
        b.xf(default);
        b
    }

    fn font_key(&mut self, st: &CellStyle) -> FontKey {
        let color = if st.color.0 == 0 { 0x7FFF } else { self.palette.index(st.color.0) };
        FontKey {
            name: st.font.clone().unwrap_or_else(|| self.default_font.clone()),
            height: (st.font_size / 5).clamp(20, 8192) as u16,
            bold: st.bold,
            italic: st.italic,
            strike: st.strike,
            underline: match st.underline {
                CellUnderline::None => 0,
                CellUnderline::Single => 1,
                CellUnderline::Double => 2,
                CellUnderline::SingleAccounting => 0x21,
                CellUnderline::DoubleAccounting => 0x22,
            },
            script: match st.vert_align {
                VerticalPosition::Superscript => 1,
                VerticalPosition::Subscript => 2,
                _ => 0,
            },
            color,
        }
    }

    /// The font's number in the file (0 to 3, then from 5).
    fn font_number(&mut self, st: &CellStyle) -> u16 {
        let key = self.font_key(st);
        let at = match self.font_index.get(&key) {
            Some(&i) => i,
            None => {
                self.fonts.push(key.clone());
                self.font_index.insert(key, self.fonts.len() - 1);
                self.fonts.len() - 1
            }
        };
        // Stored from the fifth place on: positions 0 to 3 are the default font again.
        if at == 0 { 0 } else { at as u16 + 4 }
    }

    fn format_number(&mut self, code: &str) -> u16 {
        if let Some(n) = builtin_format(code) {
            return n;
        }
        match self.formats.iter().position(|c| c == code) {
            Some(i) => 164 + i as u16,
            None => {
                self.formats.push(code.to_string());
                164 + self.formats.len() as u16 - 1
            }
        }
    }

    /// The cell format (`XF`) of a style.
    fn xf(&mut self, st: &CellStyle) -> u16 {
        let font = self.font_number(st);
        let format = self.format_number(&st.number_format);
        let mut x = Vec::with_capacity(20);
        x.extend(font.to_le_bytes());
        x.extend(format.to_le_bytes());
        // Locked, hidden formula; a cell format (not a style's) based on the style 0.
        x.extend((u16::from(!st.unlocked) | u16::from(st.formula_hidden) << 1).to_le_bytes());
        let alc = match st.align {
            CellAlign::General => 0,
            CellAlign::Left => 1,
            CellAlign::Center => 2,
            CellAlign::Right => 3,
            CellAlign::Fill => 4,
            CellAlign::Justify => 5,
            CellAlign::CenterAcross => 6,
            CellAlign::Distributed => 7,
        };
        let alc_v = match st.vertical {
            CellVerticalAlign::Top => 0,
            CellVerticalAlign::Center => 1,
            CellVerticalAlign::Bottom => 2,
            CellVerticalAlign::Justify => 3,
            CellVerticalAlign::Distributed => 4,
        };
        x.push(alc | u8::from(st.wrap) << 3 | alc_v << 4);
        x.push(st.rotation);
        x.push((st.indent.min(15) as u8) | u8::from(st.shrink) << 4);
        // All attributes are this format's own (number, font, alignment, borders, pattern, protection).
        x.push(0xFC);
        let side = |b: &Option<ibis_doc_model::Border>| b.as_ref().map(|b| border_style(b.line)).unwrap_or(0);
        let icv = |b: &Option<ibis_doc_model::Border>, pal: &mut Palette| b.as_ref().map(|b| pal.index(b.color.0)).unwrap_or(0);
        let borders = u16::from(side(&st.left)) | u16::from(side(&st.right)) << 4 | u16::from(side(&st.top)) << 8 | u16::from(side(&st.bottom)) << 12;
        x.extend(borders.to_le_bytes());
        let (l, r) = (icv(&st.left, &mut self.palette), icv(&st.right, &mut self.palette));
        let diag = u16::from(st.diagonal_up.is_some()) << 14 | u16::from(st.diagonal_down.is_some()) << 15;
        x.extend((l | r << 7 | diag).to_le_bytes());
        let (t, b) = (icv(&st.top, &mut self.palette), icv(&st.bottom, &mut self.palette));
        let diagonal = st.diagonal_up.as_ref().or(st.diagonal_down.as_ref());
        let (dc, dg) = diagonal.map(|d| (self.palette.index(d.color.0), border_style(d.line))).unwrap_or((0, 0));
        // The fill: a solid colour, or a pattern of two.
        let (fls, fore, back) = match (&st.pattern, st.fill) {
            (Some((name, fg, bg)), _) if pattern_number(name) > 0 => (pattern_number(name), self.palette.index(fg.0), self.palette.index(bg.0)),
            (_, Some(c)) => (1, self.palette.index(c.0), 0x41),
            _ => (0, 0x40, 0x41),
        };
        let d3 = u32::from(t) | u32::from(b) << 7 | u32::from(dc) << 14 | u32::from(dg) << 21 | u32::from(fls) << 26;
        x.extend(d3.to_le_bytes());
        x.extend((fore | back << 7).to_le_bytes());
        if let Some(&i) = self.xf_index.get(&x) {
            return i;
        }
        let i = FIRST_XF + self.xfs.len() as u16;
        self.xfs.push(x.clone());
        self.xf_index.insert(x, i);
        i
    }
}

// ----------------------------------------------------------------- strings

#[derive(Default)]
struct Strings {
    list: Vec<String>,
    index: HashMap<String, u32>,
    total: u32,
}

impl Strings {
    fn add(&mut self, s: &str) -> u32 {
        self.total += 1;
        let s: String = s.chars().take(32767).collect();
        if let Some(&i) = self.index.get(&s) {
            return i;
        }
        let i = self.list.len() as u32;
        self.list.push(s.clone());
        self.index.insert(s, i);
        i
    }

    /// `SST` and `EXTSST` (the records), when the first is to be at stream offset `at`.
    fn records(&self, at: usize) -> Vec<u8> {
        const MAX: usize = 8224;
        let mut out: Vec<u8> = Vec::new();
        // The record being filled: its id, where it starts in `out`, and its data.
        let mut data: Vec<u8> = Vec::new();
        data.extend(self.total.to_le_bytes());
        data.extend((self.list.len() as u32).to_le_bytes());
        let mut id = 0xFCu16;
        // (absolute offset of the string's first byte, offset in its record from the record's start including its header).
        let mut starts: Vec<(usize, usize)> = Vec::new();
        let bucket = ((self.list.len() + 127) / 128).max(8);
        let flush = |out: &mut Vec<u8>, id: u16, data: &mut Vec<u8>| {
            out.extend(id.to_le_bytes());
            out.extend((data.len() as u16).to_le_bytes());
            out.extend(data.iter());
            data.clear();
        };
        for (n, s) in self.list.iter().enumerate() {
            let units: Vec<u16> = s.encode_utf16().collect();
            let wide = !units.iter().all(|&u| u < 0x100);
            let char_size = if wide { 2 } else { 1 };
            // The header (count, flag) and at least one character go in the record that has room for them.
            if data.len() + 3 + char_size > MAX && !units.is_empty() || data.len() + 3 > MAX {
                flush(&mut out, id, &mut data);
                id = 0x3C;
            }
            if n % bucket == 0 {
                starts.push((at + out.len() + 4 + data.len(), 4 + data.len()));
            }
            data.extend((units.len() as u16).to_le_bytes());
            data.push(u8::from(wide));
            let mut i = 0;
            while i < units.len() {
                if data.len() + char_size > MAX {
                    flush(&mut out, id, &mut data);
                    id = 0x3C;
                    // A continued string starts again with its flag.
                    data.push(u8::from(wide));
                }
                if wide {
                    data.extend(units[i].to_le_bytes());
                } else {
                    data.push(units[i] as u8);
                }
                i += 1;
            }
        }
        flush(&mut out, id, &mut data);
        // EXTSST.
        let mut ext = (bucket as u16).to_le_bytes().to_vec();
        for (ib, cb) in starts {
            ext.extend((ib as u32).to_le_bytes());
            ext.extend((cb as u16).to_le_bytes());
            ext.extend(0u16.to_le_bytes());
        }
        // (A very long table is cut: the table is only an index.)
        ext.truncate(8224);
        out.extend(0xFFu16.to_le_bytes());
        out.extend((ext.len() as u16).to_le_bytes());
        out.extend(ext);
        out
    }
}

// ------------------------------------------------------------------ sheets

fn error_code(e: ibis_calc::CellError) -> u8 {
    use ibis_calc::CellError as E;
    match e {
        E::Null => 0x00,
        E::Div0 => 0x07,
        E::Value => 0x0F,
        E::Ref => 0x17,
        E::Name => 0x1D,
        E::Num => 0x24,
        E::NA => 0x2A,
        _ => 0x0F,
    }
}

fn utf16z(s: &str) -> Vec<u8> {
    s.encode_utf16().chain(std::iter::once(0)).flat_map(|u| u.to_le_bytes()).collect()
}

fn hyperlink(range: &ibis_doc_model::CellRange, target: Option<&str>, location: Option<&str>) -> Option<Vec<u8>> {
    if range.first_row > 65535 || range.first_column > 255 {
        return None;
    }
    let mut d = Vec::new();
    for v in [range.first_row.min(65535), range.last_row.min(65535), range.first_column.min(255), range.last_column.min(255)] {
        d.extend((v as u16).to_le_bytes());
    }
    d.extend([0xD0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B]);
    d.extend(2u32.to_le_bytes());
    let display = target.or(location)?;
    match (target, location) {
        (Some(t), loc) => {
            // A web address or file: a moniker (and the place in it).
            let flags: u32 = 0x1 | 0x2 | 0x4 | 0x10 | if loc.is_some() { 0x8 } else { 0 };
            d.extend(flags.to_le_bytes());
            let z = utf16z(display);
            d.extend(((z.len() / 2) as u32).to_le_bytes());
            d.extend(z);
            d.extend([0xE0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B]);
            let url = utf16z(t);
            d.extend((url.len() as u32).to_le_bytes());
            d.extend(url);
            if let Some(l) = loc {
                let z = utf16z(l);
                d.extend(((z.len() / 2) as u32).to_le_bytes());
                d.extend(z);
            }
        }
        (None, Some(l)) => {
            // A place in the workbook.
            let flags: u32 = 0x4 | 0x8 | 0x10;
            d.extend(flags.to_le_bytes());
            let z = utf16z(display);
            d.extend(((z.len() / 2) as u32).to_le_bytes());
            d.extend(z);
            let z = utf16z(l);
            d.extend(((z.len() / 2) as u32).to_le_bytes());
            d.extend(z);
        }
        (None, None) => return None,
    }
    Some(d)
}

/// What the sheets share while they are written.
struct Shared<'a> {
    /// The shapes each drawing has (a sheet with notes or pictures has one), in order.
    drawings: Vec<usize>,
    /// The pictures of the workbook (each once).
    blips: Vec<drawing::Blip>,
    /// The default font's maximum digit width, in points (how widths in characters are counted).
    digit: f64,
    book: Book,
    strings: Strings,
    refs: Refs,
    report: Report,
    date1904: bool,
    default_style: &'a CellStyle,
}

fn paper_code(paper: (i64, i64)) -> u16 {
    let (w, h) = (paper.0.min(paper.1) as f64 / 36000.0, paper.0.max(paper.1) as f64 / 36000.0); // mm
    let is = |a: f64, b: f64| (w - a).abs() < 2.0 && (h - b).abs() < 2.0;
    if is(210.0, 297.0) {
        9
    } else if is(215.9, 279.4) {
        1
    } else if is(215.9, 355.6) {
        5
    } else if is(297.0, 420.0) {
        8
    } else if is(148.0, 210.0) {
        11
    } else if is(176.0, 250.0) {
        13
    } else if is(184.2, 266.7) {
        7
    } else if is(257.0, 364.0) {
        12
    } else {
        9
    }
}

type Pictures = Vec<(ibis_doc_model::EmuRect, u8, std::sync::Arc<[u8]>)>;

fn sheet_stream(shared: &mut Shared, number: u16, sheet: &Sheet, pictures: &Pictures, selected: bool, workbook: &ibis_doc_model::Workbook) -> Vec<u8> {
    let mut o = Out::default();
    o.rec(0x809, &[0x00, 0x06, 0x10, 0x00, 0xBB, 0x0D, 0xCC, 0x07, 0, 0, 0, 0, 0x06, 0, 0, 0]);
    o.rec_u16(0x0C, 100);
    o.rec_u16(0x0F, 1);
    o.rec_u16(0x11, 0);
    o.rec(0x10, &0.001f64.to_le_bytes());
    o.rec_u16(0x5F, 1);
    o.rec_u16(0x2A, u16::from(sheet.print.headings));
    o.rec_u16(0x2B, u16::from(sheet.print.grid_lines));
    o.rec_u16(0x82, 1);
    // GUTS: the widths of the outline's margins and the deepest levels.
    let row_levels = sheet.outline_rows.values().copied().max().unwrap_or(0) as u16;
    let col_levels = sheet.outline_columns.values().copied().max().unwrap_or(0) as u16;
    let mut guts = Vec::new();
    for v in [if row_levels > 0 { row_levels * 12 + 5 } else { 0 }, if col_levels > 0 { col_levels * 12 + 5 } else { 0 }, row_levels, col_levels] {
        guts.extend(v.to_le_bytes());
    }
    o.rec(0x80, &guts);
    let twips = |e: i64| (e / 635).clamp(0, 8191) as u16;
    let default_height = twips(sheet.default_row).max(1);
    o.rec(0x225, &[0, 0, default_height.to_le_bytes()[0], default_height.to_le_bytes()[1]]);
    let fit = sheet.print.fit_to.is_some();
    o.rec_u16(0x81, 0x04C1 | if fit { 0x100 } else { 0 });
    o.rec_u16(0x83, u16::from(sheet.print.centered.0));
    o.rec_u16(0x84, u16::from(sheet.print.centered.1));
    for (id, k) in [(0x26u16, 0usize), (0x27, 1), (0x28, 2), (0x29, 3)] {
        o.rec(id, &(sheet.print.margins[k] as f64 / 914_400.0).to_le_bytes());
    }
    let mut setup = Vec::new();
    setup.extend(paper_code(sheet.print.paper).to_le_bytes());
    setup.extend((sheet.print.scale.clamp(10, 400) as u16).to_le_bytes());
    setup.extend((sheet.print.first_page_number.unwrap_or(1) as u16).to_le_bytes());
    let (fw, fh) = sheet.print.fit_to.unwrap_or((1, 1));
    setup.extend((fw as u16).to_le_bytes());
    setup.extend((fh as u16).to_le_bytes());
    let grbit: u16 = (if sheet.print.landscape { 0x2 } else { 0 }) | 0x4 | (if sheet.print.black_and_white { 0x8 } else { 0 }) | (if sheet.print.draft { 0x10 } else { 0 }) | (if sheet.print.use_first_page_number { 0x80 } else { 0 });
    setup.extend(grbit.to_le_bytes());
    setup.extend(600u16.to_le_bytes());
    setup.extend(600u16.to_le_bytes());
    setup.extend((sheet.print.header_margin as f64 / 914_400.0).to_le_bytes());
    setup.extend((sheet.print.footer_margin as f64 / 914_400.0).to_le_bytes());
    setup.extend(1u16.to_le_bytes());
    o.rec(0xA1, &setup);
    // The columns' widths are counted in characters of the default font (the maximum digit width, `digit` points, and
    // 5 points of padding): what the file can say for a width, and the default width it can say.
    let digit = shared.digit;
    let points = |emu: i64| emu as f64 / 12700.0;
    // The padding is 5 pixels, a pixel being `pixel` points (the same metrics the reader counts a width in).
    let pixel = crate::xlsx::EXCEL_METRICS.pixel_pt();
    let padding = 5.0 * pixel;
    let base_columns = ((points(sheet.default_column) - padding) / digit).round().clamp(1.0, 255.0);
    o.rec_u16(0x55, base_columns as u16);
    let represented_default = ((base_columns * digit + padding) / pixel).trunc() * pixel;
    let width_word = |emu: i64| ((points(emu) * 256.0 / digit - (128.0 / (digit / pixel)).floor()).ceil()).clamp(0.0, 65535.0) as u16;
    let ncols = sheet.columns.len().min(256);
    let mut c = 0;
    while c < ncols {
        let w = sheet.columns[c];
        let level = sheet.outline_columns.get(&(c as u32)).copied().unwrap_or(0);
        let mut e = c + 1;
        while e < ncols && sheet.columns[e] == w && sheet.outline_columns.get(&(e as u32)).copied().unwrap_or(0) == level {
            e += 1;
        }
        let hidden = w == 0;
        if (points(w).floor() != represented_default && w != 0) || hidden || level > 0 {
            let real = if hidden { sheet.hidden_columns.get(&(c as u32)).copied().unwrap_or(sheet.default_column) } else { w };
            let mut d = Vec::new();
            d.extend((c as u16).to_le_bytes());
            d.extend(((e - 1) as u16).to_le_bytes());
            d.extend(width_word(real).to_le_bytes());
            d.extend(15u16.to_le_bytes());
            d.extend((u16::from(hidden) | (level.min(7) as u16) << 8).to_le_bytes());
            d.extend(0u16.to_le_bytes());
            o.rec(0x7D, &d);
        }
        c = e;
    }
    // The cells by row.
    let style_xf: Vec<u16> = sheet.styles.iter().map(|s| shared.book.xf(s)).collect();
    let default_xf = shared.book.xf(shared.default_style);
    let mut by_row: BTreeMap<u32, Vec<&ibis_doc_model::SheetCell>> = BTreeMap::new();
    for cell in &sheet.cells {
        if cell.row > 65535 || cell.column > 255 {
            shared.report.cells_out_of_range += 1;
            continue;
        }
        by_row.entry(cell.row).or_default().push(cell);
    }
    let mut rows: BTreeSet<u32> = by_row.keys().copied().collect();
    for r in 0..sheet.rows.len().min(65536) {
        if sheet.rows[r] != sheet.default_row {
            rows.insert(r as u32);
        }
    }
    rows.extend(sheet.outline_rows.keys().copied().filter(|&r| r <= 65535));
    let first_row = rows.iter().next().copied().unwrap_or(0);
    let last_row = rows.iter().next_back().map_or(0, |&r| r + 1);
    let (mut min_col, mut max_col) = (u32::MAX, 0u32);
    for cell in by_row.values().flatten() {
        min_col = min_col.min(cell.column);
        max_col = max_col.max(cell.column + 1);
    }
    if min_col == u32::MAX {
        min_col = 0;
    }
    let mut dim = Vec::new();
    dim.extend(first_row.to_le_bytes());
    dim.extend(last_row.to_le_bytes());
    dim.extend((min_col as u16).to_le_bytes());
    dim.extend((max_col as u16).to_le_bytes());
    dim.extend(0u16.to_le_bytes());
    o.rec(0x200, &dim);
    let row_list: Vec<u32> = rows.into_iter().collect();
    for block in row_list.chunks(32) {
        for &r in block {
            let cells = by_row.get(&r);
            let (lo, hi) = cells.map_or((0, 0), |cs| (cs.iter().map(|c| c.column).min().unwrap_or(0), cs.iter().map(|c| c.column + 1).max().unwrap_or(0)));
            let height = sheet.rows.get(r as usize).copied().unwrap_or(sheet.default_row);
            let hidden = height == 0;
            let real = if hidden { sheet.hidden_rows.get(&r).copied().unwrap_or(sheet.default_row) } else { height };
            let custom = real != sheet.default_row;
            let level = sheet.outline_rows.get(&r).copied().unwrap_or(0).min(7) as u32;
            let mut d = Vec::new();
            d.extend((r as u16).to_le_bytes());
            d.extend((lo as u16).to_le_bytes());
            d.extend((hi as u16).to_le_bytes());
            d.extend(twips(real).max(1).to_le_bytes());
            d.extend(0u16.to_le_bytes());
            d.extend(0u16.to_le_bytes());
            let flags: u32 = 0x100 | level | (if hidden { 0x20 } else { 0 }) | (if custom { 0x40 } else { 0 }) | (u32::from(default_xf) << 16);
            d.extend(flags.to_le_bytes());
            o.rec(0x208, &d);
        }
        for &r in block {
            let Some(cells) = by_row.get(&r) else { continue };
            let mut cells = cells.clone();
            cells.sort_by_key(|c| c.column);
            for cell in cells {
                let xf = style_xf.get(cell.style as usize).copied().unwrap_or(default_xf);
                write_cell(&mut o, shared, number, cell, xf);
            }
        }
    }
    // The notes: their drawing, then the records that tie them to their cells.
    let notes: Vec<&ibis_doc_model::SheetComment> = sheet.comments.iter().filter(|c| c.row <= 65535 && c.column <= 255).collect();
    let mut shapes: Vec<drawing::Shape> = Vec::new();
    for (frame, kind, data) in pictures {
        let bid = match shared.blips.iter().position(|b| b.kind == *kind && (std::sync::Arc::ptr_eq(&b.data, data) || *b.data == **data)) {
            Some(i) => i as u32 + 1,
            None => {
                shared.blips.push(drawing::Blip { kind: *kind, data: data.clone() });
                shared.blips.len() as u32
            }
        };
        shapes.push(drawing::Shape::Picture { bid, anchor: drawing::anchor_words(*frame, sheet) });
    }
    shapes.extend(notes.iter().map(|n| drawing::Shape::Note(n)));
    if !shapes.is_empty() {
        shared.drawings.push(shapes.len());
        drawing::write_shapes(&mut o, &shapes, shared.drawings.len() as u32);
        drawing::write_note_records(&mut o, &notes);
    }
    // The window.
    let mut flags: u16 = 0x0010 | 0x0020 | 0x0080 | 0x0400;
    if sheet.grid_lines {
        flags |= 0x2;
    }
    if sheet.headings {
        flags |= 0x4;
    }
    if selected {
        flags |= 0x200;
    }
    let frozen = sheet.frozen.filter(|&(r, c)| r > 0 || c > 0);
    if frozen.is_some() {
        flags |= 0x8 | 0x100;
    }
    let mut w2 = Vec::new();
    w2.extend(flags.to_le_bytes());
    w2.extend(0u16.to_le_bytes());
    w2.extend(0u16.to_le_bytes());
    w2.extend(0x40u32.to_le_bytes());
    w2.extend(0u16.to_le_bytes());
    w2.extend(100u16.to_le_bytes());
    w2.extend(0u32.to_le_bytes());
    o.rec(0x23E, &w2);
    if let Some(z) = sheet.zoom.filter(|&z| z != 100) {
        o.rec(0xA0, &[&z.to_le_bytes()[..], &100u16.to_le_bytes()[..]].concat());
    }
    let mut active_pane = 3u8;
    if let Some((rows_frozen, cols_frozen)) = frozen {
        active_pane = match (rows_frozen > 0, cols_frozen > 0) {
            (true, true) => 0,
            (true, false) => 2,
            (false, _) => 1,
        };
        let mut p = Vec::new();
        p.extend((cols_frozen as u16).to_le_bytes());
        p.extend((rows_frozen as u16).to_le_bytes());
        p.extend((rows_frozen as u16).to_le_bytes());
        p.extend((cols_frozen as u16).to_le_bytes());
        p.extend((active_pane as u16).to_le_bytes());
        o.rec(0x41, &p);
    }
    // The selection: the first cell.
    let mut sel = vec![if frozen.is_some() { active_pane } else { 3 }];
    sel.extend(0u16.to_le_bytes());
    sel.extend(0u16.to_le_bytes());
    sel.extend(0u16.to_le_bytes());
    sel.extend(1u16.to_le_bytes());
    sel.extend([0, 0, 0, 0, 0, 0]);
    sel.truncate(15);
    o.rec(0x1D, &sel);
    // Merged cells.
    let merges: Vec<&ibis_doc_model::CellRange> = sheet.merges.iter().filter(|m| m.last_row <= 65535 && m.last_column <= 255).collect();
    for chunk in merges.chunks(1000) {
        let mut d = (chunk.len() as u16).to_le_bytes().to_vec();
        for m in chunk {
            for v in [m.first_row, m.last_row, m.first_column, m.last_column] {
                d.extend((v as u16).to_le_bytes());
            }
        }
        o.rec(0xE5, &d);
    }
    // Conditional formats, hyperlinks, validations.
    features::conditional(sheet, number, &mut shared.refs, &mut shared.book.palette, &mut o, &mut shared.report);
    for h in &sheet.hyperlinks {
        if let Some(d) = hyperlink(&h.range, h.target.as_deref(), h.location.as_deref()) {
            o.rec(0x1B8, &d);
        }
    }
    features::validations(sheet, number, &mut shared.refs, &mut o, &mut shared.report);
    shared.report.other += sheet.comments.len() - notes.len() + sheet.tables.len();
    let _ = workbook;
    o.rec(0x0A, &[]);
    o.buf
}

fn write_cell(o: &mut Out, shared: &mut Shared, sheet_number: u16, cell: &ibis_doc_model::SheetCell, xf: u16) {
    let mut head = Vec::new();
    head.extend((cell.row as u16).to_le_bytes());
    head.extend((cell.column as u16).to_le_bytes());
    head.extend(xf.to_le_bytes());
    if let Some(f) = cell.formula.as_deref() {
        match formula::compile(f, sheet_number, &mut shared.refs) {
            Some(tokens) => {
                let mut d = head.clone();
                let mut text_result = None;
                match &cell.value {
                    CellValue::Number(n) => d.extend(n.to_le_bytes()),
                    CellValue::Text(t) if t.is_empty() => d.extend([3, 0, 0, 0, 0, 0, 0xFF, 0xFF]),
                    CellValue::Text(t) => {
                        d.extend([0, 0, 0, 0, 0, 0, 0xFF, 0xFF]);
                        text_result = Some(t.clone());
                    }
                    CellValue::Bool(b) => d.extend([1, 0, u8::from(*b), 0, 0, 0, 0xFF, 0xFF]),
                    CellValue::Error(e) => d.extend([2, 0, error_code(*e), 0, 0, 0, 0xFF, 0xFF]),
                    CellValue::Empty => d.extend([3, 0, 0, 0, 0, 0, 0xFF, 0xFF]),
                }
                d.extend(0u16.to_le_bytes());
                d.extend(0u32.to_le_bytes());
                d.extend((tokens.len() as u16).to_le_bytes());
                d.extend(tokens);
                o.rec(0x06, &d);
                if let Some(t) = text_result {
                    o.rec(0x207, &unicode_string(&t));
                }
                return;
            }
            None => shared.report.formulas_as_values += 1,
        }
    }
    match &cell.value {
        CellValue::Number(n) => {
            let mut d = head;
            d.extend(n.to_le_bytes());
            o.rec(0x203, &d);
        }
        CellValue::Text(t) => {
            let mut d = head;
            d.extend(shared.strings.add(t).to_le_bytes());
            o.rec(0xFD, &d);
        }
        CellValue::Bool(b) => {
            let mut d = head;
            d.extend([u8::from(*b), 0]);
            o.rec(0x205, &d);
        }
        CellValue::Error(e) => {
            let mut d = head;
            d.extend([error_code(*e), 1]);
            o.rec(0x205, &d);
        }
        CellValue::Empty => o.rec(0x201, &head),
    }
}

// ---------------------------------------------------------------- the file

/// The workbook of `presentation` as an `.xls`.
pub fn write(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    write_report(presentation).map(|(bytes, _)| bytes)
}

/// The workbook as an `.xls`, and what had to be left out.
pub fn write_report(presentation: &Presentation) -> Result<(Vec<u8>, Report), Error> {
    let workbook = presentation.workbook.as_ref().ok_or_else(|| Error::new("ブックではありません"))?;
    let mut sheets: Vec<(String, SheetState, std::sync::Arc<Sheet>, Option<ibis_doc_model::SlideId>)> = Vec::new();
    let mut skipped_sheets = 0;
    for ws in &workbook.sheets {
        if ws.other {
            // A chart sheet (a dialog or macro sheet) is not written.
            skipped_sheets += 1;
            continue;
        }
        let sheet = match ws.slide {
            Some(id) => presentation.slides.iter().find(|s| s.id == id).and_then(sheet_of_slide).cloned(),
            None => ws.hidden.clone(),
        };
        if let Some(sheet) = sheet {
            sheets.push((ws.name.chars().take(31).collect(), ws.state, sheet, ws.slide));
        }
    }
    if sheets.is_empty() {
        return Err(Error::new("シートがありません"));
    }
    if sheets.len() > 255 {
        return Err(Error::new("旧形式（.xls）に保存できるシートは 255 枚までです"));
    }
    let default_style = workbook.default_style.clone().unwrap_or_default();
    let mut shared = Shared {
        drawings: Vec::new(),
        blips: Vec::new(),
        digit: crate::xlsx::EXCEL_METRICS.digit_pt(default_style.font.as_deref(), default_style.font_size as f64 / 100.0),
        book: Book::new(&default_style),
        strings: Strings::default(),
        refs: Refs { sheets: sheets.iter().enumerate().map(|(i, s)| (s.0.to_lowercase(), i as u16)).collect(), names: HashMap::new(), xti: Vec::new() },
        report: Report::default(),
        date1904: workbook.date1904,
        default_style: &default_style,
    };
    shared.report.drawings += skipped_sheets;
    let mut sheet_pictures: Vec<Pictures> = Vec::new();
    for (_, _, sheet, slide) in &sheets {
        let (pictures, left_out) = presentation_drawings(presentation, sheet, *slide);
        shared.report.drawings += left_out;
        sheet_pictures.push(pictures);
    }
    // The defined names get their numbers before the formulas use them: the workbook's, then the sheets' print areas.
    let mut name_records: Vec<(Option<u16>, String, Option<u8>, String)> = Vec::new();
    for n in &workbook.names {
        let scope = n.sheet.filter(|&s| s < sheets.len()).map(|s| s as u16);
        if n.name.starts_with("_xlnm.") {
            continue;
        }
        name_records.push((scope, n.name.clone(), None, n.formula.clone()));
    }
    for (i, n) in name_records.iter().enumerate() {
        shared.refs.names.entry((n.1.to_lowercase(), n.0)).or_insert(i as u32 + 1);
    }
    // The cells of each sheet, as streams (the formulas fill in the references the file has).
    let selected = sheets.iter().position(|s| s.1 == SheetState::Visible).unwrap_or(0);
    let mut streams: Vec<Vec<u8>> = Vec::new();
    for (i, (_, _, sheet, _)) in sheets.iter().enumerate() {
        streams.push(sheet_stream(&mut shared, i as u16, sheet, &sheet_pictures[i], i == selected, workbook));
    }
    // The names' tokens (they may add to the references too).
    let mut names_out = Out::default();
    for (scope, name, builtin, text) in &name_records {
        let Some(tokens) = formula::compile(text, scope.unwrap_or(0), &mut shared.refs) else {
            shared.report.formulas_as_values += 1;
            continue;
        };
        let _ = builtin;
        names_out.rec(0x18, &name_record(0, *scope, name, None, &tokens));
    }
    for (i, (_, _, sheet, _)) in sheets.iter().enumerate() {
        if let Some(area) = &sheet.print_area {
            let mut t = vec![0x3B];
            let ixti = shared.refs.xti.iter().position(|&x| x == (i as u16, i as u16)).unwrap_or_else(|| {
                shared.refs.xti.push((i as u16, i as u16));
                shared.refs.xti.len() - 1
            });
            t.extend((ixti as u16).to_le_bytes());
            for v in [area.first_row.min(65535), area.last_row.min(65535)] {
                t.extend((v as u16).to_le_bytes());
            }
            for v in [area.first_column.min(255), area.last_column.min(255)] {
                t.extend((v as u16).to_le_bytes());
            }
            names_out.rec(0x18, &name_record(0x20, Some(i as u16), "", Some(0x06), &t));
        }
    }
    // The workbook's globals.
    let mut g = Out::default();
    g.rec(0x809, &[0x00, 0x06, 0x05, 0x00, 0xBB, 0x0D, 0xCC, 0x07, 0, 0, 0, 0, 0x06, 0, 0, 0]);
    g.rec_u16(0xE1, 0x04B0);
    g.rec_u16(0xC1, 0);
    g.rec(0xE2, &[]);
    let mut writer = vec![4, 0, 0];
    writer.extend(b"ibis");
    writer.resize(112, b' ');
    g.rec(0x5C, &writer);
    g.rec_u16(0x42, 0x04B0);
    g.rec_u16(0x161, 0);
    g.rec(0x1C0, &[]);
    let tab_ids: Vec<u8> = (1..=sheets.len() as u16).flat_map(|n| n.to_le_bytes()).collect();
    g.rec(0x13D, &tab_ids);
    g.rec_u16(0x9C, 0x0E);
    g.rec_u16(0x19, 0);
    g.rec_u16(0x12, 0);
    g.rec_u16(0x13, 0);
    g.rec_u16(0x1AF, 0);
    g.rec_u16(0x1BC, 0);
    let mut w1 = Vec::new();
    for v in [0u16, 0, 0x4000, 0x2000, 0x0038, selected as u16, 0, 1, 600] {
        w1.extend(v.to_le_bytes());
    }
    g.rec(0x3D, &w1);
    g.rec_u16(0x40, 0);
    g.rec_u16(0x8D, 0);
    g.rec_u16(0x22, u16::from(shared.date1904));
    g.rec_u16(0x0E, 1);
    g.rec_u16(0x1B7, 0);
    g.rec_u16(0xDA, 0);
    // The fonts: the default one four times (numbers 0 to 3), then each (numbers from 5).
    let font = |f: &FontKey| {
        let mut d = Vec::new();
        d.extend(f.height.to_le_bytes());
        d.extend((u16::from(f.italic) << 1 | u16::from(f.strike) << 3).to_le_bytes());
        d.extend(f.color.to_le_bytes());
        d.extend((if f.bold { 700u16 } else { 400 }).to_le_bytes());
        d.extend(f.script.to_le_bytes());
        d.push(f.underline);
        d.extend([0, 1, 0]);
        let name = flagged_chars(&f.name);
        d.push(f.name.encode_utf16().count().min(255) as u8);
        d.extend(name);
        d
    };
    let fonts = shared.book.fonts.clone();
    for _ in 0..4 {
        g.rec(0x31, &font(&fonts[0]));
    }
    for f in fonts.iter().skip(1) {
        g.rec(0x31, &font(f));
    }
    if fonts.len() == 1 {
        g.rec(0x31, &font(&fonts[0]));
    }
    for (i, code) in shared.book.formats.iter().enumerate() {
        let mut d = (164 + i as u16).to_le_bytes().to_vec();
        d.extend(unicode_string(code));
        g.rec(0x41E, &d);
    }
    // The cell formats: 15 of the styles' (alike), then ours (the first is the default cell format).
    let style_xf = {
        let mut x = Vec::new();
        x.extend(0u16.to_le_bytes());
        x.extend(0u16.to_le_bytes());
        x.extend(0xFFF5u16.to_le_bytes());
        x.extend([0x20, 0, 0, 0xF4]);
        x.extend([0, 0, 0, 0, 0, 0]);
        x.extend(0x20C0u16.to_le_bytes());
        x
    };
    for _ in 0..15 {
        g.rec(0xE0, &style_xf);
    }
    // The 16th: the cell format of the default style.
    let xfs = shared.book.xfs.clone();
    for x in &xfs {
        g.rec(0xE0, x);
    }
    g.rec(0x293, &[0x00, 0x80, 0x00, 0xFF]);
    // The palette, if any colour was added.
    if shared.book.palette.colors != crate::xls::DEFAULT_PALETTE.to_vec() {
        let mut d = 56u16.to_le_bytes().to_vec();
        for c in &shared.book.palette.colors {
            d.extend([(c >> 16) as u8, (c >> 8) as u8, *c as u8, 0]);
        }
        g.rec(0x92, &d);
    }
    g.rec_u16(0x160, 1);
    // The sheets (their positions are filled in below).
    let mut position_at = Vec::new();
    for (name, state, ..) in &sheets {
        let mut d = Vec::new();
        position_at.push(g.buf.len() + 4);
        d.extend(0u32.to_le_bytes());
        d.push(match state {
            SheetState::Visible => 0,
            SheetState::Hidden => 1,
            SheetState::VeryHidden => 2,
        });
        d.push(0);
        d.push(name.encode_utf16().count() as u8);
        d.extend(flagged_chars(name));
        g.rec(0x85, &d);
    }
    g.rec(0x8C, &[1, 0, 1, 0]);
    if !shared.refs.xti.is_empty() {
        let mut d = (sheets.len() as u16).to_le_bytes().to_vec();
        d.extend([0x01, 0x04]);
        g.rec(0x1AE, &d);
        let mut d = (shared.refs.xti.len() as u16).to_le_bytes().to_vec();
        for &(a, b) in &shared.refs.xti {
            d.extend(0u16.to_le_bytes());
            d.extend(a.to_le_bytes());
            d.extend(b.to_le_bytes());
        }
        // (A table longer than a record is not made: the count of the references is kept below it.)
        g.rec(0x17, &d);
    }
    g.buf.extend(names_out.buf);
    if !shared.drawings.is_empty() {
        g.rec_split(0xEB, &drawing::group_record(&shared.drawings, &shared.blips));
    }
    let sst_at = g.buf.len();
    let sst = shared.strings.records(sst_at);
    g.buf.extend(sst);
    g.rec(0x0A, &[]);
    // The streams follow, each from the position its sheet says.
    let mut at = g.buf.len();
    for (i, s) in streams.iter().enumerate() {
        g.buf[position_at[i]..position_at[i] + 4].copy_from_slice(&(at as u32).to_le_bytes());
        at += s.len();
    }
    let mut bytes = g.buf;
    for s in streams {
        bytes.extend(s);
    }
    let file = ibis_ooxml::cfb::write(&[ibis_ooxml::cfb::Item::stream("Workbook", bytes)]);
    Ok((file, shared.report))
}

fn name_record(grbit: u16, scope: Option<u16>, name: &str, builtin: Option<u8>, tokens: &[u8]) -> Vec<u8> {
    let mut d = Vec::new();
    d.extend(grbit.to_le_bytes());
    d.push(0);
    let chars = match builtin {
        Some(b) => vec![0, b],
        None => flagged_chars(name),
    };
    d.push(if builtin.is_some() { 1 } else { name.encode_utf16().count().min(255) as u8 });
    d.extend((tokens.len() as u16).to_le_bytes());
    d.extend(0u16.to_le_bytes());
    d.extend(scope.map_or(0, |s| s + 1).to_le_bytes());
    d.extend([0, 0, 0, 0]);
    d.extend(chars);
    d.extend(tokens);
    d
}

/// The pictures of the sheet's slide the file can hold (PNG and JPEG: frame, kind, bytes), and how many pictures,
/// charts and shapes it cannot (they are not written).
fn presentation_drawings(presentation: &Presentation, _sheet: &Sheet, slide: Option<ibis_doc_model::SlideId>) -> (Pictures, usize) {
    use ibis_doc_model::{MediaFormat, ShapeKind};
    let Some(slide) = slide.and_then(|id| presentation.slides.iter().find(|s| s.id == id)) else { return (Vec::new(), 0) };
    let (mut pictures, mut left_out) = (Vec::new(), 0);
    for shape in &slide.shapes {
        match &shape.kind {
            ShapeKind::Picture(p) => match presentation.media.iter().find(|m| m.id == p.media) {
                Some(m) if m.format == MediaFormat::Png => pictures.push((shape.frame, 6u8, m.data.clone())),
                Some(m) if m.format == MediaFormat::Jpeg => pictures.push((shape.frame, 5u8, m.data.clone())),
                _ => left_out += 1,
            },
            ShapeKind::Chart(_) | ShapeKind::Auto(_) | ShapeKind::TextBox(_) | ShapeKind::Line(_) => left_out += 1,
            _ => {}
        }
    }
    (pictures, left_out)
}
