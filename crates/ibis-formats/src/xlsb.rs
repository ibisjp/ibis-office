// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A binary workbook (`.xlsb`, MS-XLSB) read as the `.xlsx` made from it: the
//! package is the same (OPC), only the workbook, the sheets, the shared strings
//! and the styles are records instead of XML; they are written back as XML and
//! the rest (drawings, charts, pictures, the theme) is copied. What the
//! records hold that this does not carry over (tables, comments, conditional
//! formats, data validation, pivot tables, external links) is left out.

use std::collections::HashMap;
use std::fmt::Write as _;

use roxmltree::Document;

use crate::xls_formula::{Context, column_letters};
use crate::xlsb_formula::decompile;
use crate::Error;

fn bad(message: &str) -> Error {
    Error::new(format!("Excel バイナリ ブック：{message}"))
}

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const WORKSHEET_TYPE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";

/// Whether `data` is a binary workbook: a package whose workbook part is `xl/workbook.bin`.
pub fn is_xlsb(data: &[u8]) -> bool {
    if !data.starts_with(b"PK") {
        return false;
    }
    let Ok(archive) = crate::zip::Archive::new(data) else { return false };
    archive.contains("xl/workbook.bin")
}

// ---- records ----

fn records(d: &[u8]) -> Vec<(u32, &[u8])> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < d.len() {
        let b = d[i];
        i += 1;
        let mut id = (b & 0x7F) as u32;
        if b & 0x80 != 0 {
            let Some(&b2) = d.get(i) else { break };
            i += 1;
            id |= ((b2 & 0x7F) as u32) << 7;
        }
        let mut size = 0usize;
        for k in 0..4 {
            let Some(&b) = d.get(i) else { return out };
            i += 1;
            size |= ((b & 0x7F) as usize) << (7 * k);
            if b & 0x80 == 0 {
                break;
            }
        }
        let Some(data) = d.get(i..i + size) else { break };
        i += size;
        out.push((id, data));
    }
    out
}

struct Cur<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Cur<'a> {
    fn new(d: &'a [u8]) -> Self {
        Self { d, p: 0 }
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.d.get(self.p..self.p + n)?;
        self.p += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(self.u32()? as i32)
    }
    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    /// An `XLWideString` (a 32-bit length in characters, then UTF-16); a null string (length `0xFFFFFFFF`) is empty.
    fn wstr(&mut self) -> Option<String> {
        let n = self.u32()?;
        if n == 0xFFFF_FFFF {
            return Some(String::new());
        }
        let bytes = self.take(n as usize * 2)?;
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        Some(String::from_utf16_lossy(&units))
    }
}

/// A `CellParsedFormula` / `NameParsedFormula`: the tokens and their extra data.
fn parsed_formula<'a>(c: &mut Cur<'a>) -> Option<(&'a [u8], &'a [u8])> {
    let cce = c.u32()? as usize;
    let rgce = c.take(cce)?;
    let cb = c.u32().unwrap_or(0) as usize;
    let rgcb = c.take(cb).unwrap_or(&[]);
    Some((rgce, rgcb))
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => {}
            c => o.push(c),
        }
    }
    o
}

fn text_element(tag: &str, s: &str) -> String {
    let preserve = s.starts_with(char::is_whitespace) || s.ends_with(char::is_whitespace) || s.contains('\n');
    format!("<{tag}{}>{}</{tag}>", if preserve { " xml:space=\"preserve\"" } else { "" }, esc(s))
}

fn error_text(code: u8) -> &'static str {
    match code {
        0x00 => "#NULL!",
        0x07 => "#DIV/0!",
        0x0F => "#VALUE!",
        0x17 => "#REF!",
        0x1D => "#NAME?",
        0x24 => "#NUM!",
        0x2A => "#N/A",
        _ => "#VALUE!",
    }
}

fn cell_ref(row: u32, col: u32) -> String {
    format!("{}{}", column_letters(col), row + 1)
}

fn range_ref(r1: u32, r2: u32, c1: u32, c2: u32) -> String {
    if r1 == r2 && c1 == c2 { cell_ref(r1, c1) } else { format!("{}:{}", cell_ref(r1, c1), cell_ref(r2, c2)) }
}

// ---- styles ----

/// A `BrtColor` as an element named `tag`; `None` for an automatic colour.
fn color_xml(tag: &str, b: &[u8]) -> Option<String> {
    if b.len() < 8 {
        return None;
    }
    let kind = b[0] >> 1;
    let index = b[1];
    let tint = i16::from_le_bytes([b[2], b[3]]);
    let tint = if tint == 0 { String::new() } else { format!(" tint=\"{:.4}\"", tint as f64 / 32767.0) };
    match kind {
        1 => Some(format!("<{tag} indexed=\"{index}\"{tint}/>")),
        2 => Some(format!("<{tag} rgb=\"FF{:02X}{:02X}{:02X}\"{tint}/>", b[4], b[5], b[6])),
        3 => Some(format!("<{tag} theme=\"{index}\"{tint}/>")),
        _ => None,
    }
}

fn border_style(dg: u8) -> Option<&'static str> {
    Some(match dg {
        1 => "thin",
        2 => "medium",
        3 => "dashed",
        4 => "dotted",
        5 => "thick",
        6 => "double",
        7 => "hair",
        8 => "mediumDashed",
        9 => "dashDot",
        10 => "mediumDashDot",
        11 => "dashDotDot",
        12 => "mediumDashDotDot",
        13 => "slantDashDot",
        _ => return None,
    })
}

fn pattern_name(fls: u32) -> &'static str {
    match fls {
        1 => "solid",
        2 => "mediumGray",
        3 => "darkGray",
        4 => "lightGray",
        5 => "darkHorizontal",
        6 => "darkVertical",
        7 => "darkDown",
        8 => "darkUp",
        9 => "darkGrid",
        10 => "darkTrellis",
        11 => "lightHorizontal",
        12 => "lightVertical",
        13 => "lightDown",
        14 => "lightUp",
        15 => "lightGrid",
        16 => "lightTrellis",
        17 => "gray125",
        18 => "gray0625",
        _ => "none",
    }
}

/// An `XFPropColor` as an element named `tag`.
fn xf_color(tag: &str, d: &[u8]) -> Option<String> {
    if d.len() < 8 {
        return None;
    }
    let tint = i16::from_le_bytes([d[2], d[3]]);
    let tint = if tint == 0 { String::new() } else { format!(" tint=\"{:.4}\"", tint as f64 / 32767.0) };
    match d[0] >> 1 {
        1 => Some(format!("<{tag} indexed=\"{}\"{tint}/>", d[1])),
        2 => Some(format!("<{tag} rgb=\"FF{:02X}{:02X}{:02X}\"{tint}/>", d[4], d[5], d[6])),
        3 => Some(format!("<{tag} theme=\"{}\"{tint}/>", d[1])),
        _ => None,
    }
}

/// A `BrtDXF` (formatting a conditional format applies) as a `dxf` element.
fn dxf_xml(rec: &[u8]) -> String {
    let mut c = Cur::new(rec);
    let _ = (c.u16(), c.u16());
    let n = c.u16().unwrap_or(0);
    let (mut bold, mut italic, mut strike, mut underline, mut text_color) = (None, None, None, None, None);
    let (mut pattern, mut fg, mut bg) = (None, None, None);
    let mut sides: [Option<String>; 4] = [None, None, None, None];
    for _ in 0..n {
        let (Some(t), Some(cb)) = (c.u16(), c.u16()) else { break };
        let Some(d) = c.take((cb as usize).saturating_sub(4)) else { break };
        match t {
            0 => pattern = d.first().copied(),
            1 => fg = xf_color("fgColor", d),
            2 => bg = xf_color("bgColor", d),
            5 => text_color = xf_color("color", d),
            6..=9 => {
                let dg = d.get(8..10).map(|b| u16::from_le_bytes([b[0], b[1]])).unwrap_or(0);
                let tag = ["top", "bottom", "left", "right"][(t - 6) as usize];
                sides[(t - 6) as usize] = Some(match border_style(dg as u8) {
                    Some(style) => format!("<{tag} style=\"{style}\">{}</{tag}>", xf_color("color", d).unwrap_or_else(|| "<color auto=\"1\"/>".into())),
                    None => format!("<{tag}/>"),
                });
            }
            0x19 => bold = d.get(..2).map(|b| u16::from_le_bytes([b[0], b[1]]) >= 600),
            0x1A => underline = d.get(..2).map(|b| u16::from_le_bytes([b[0], b[1]])),
            0x1C => italic = d.first().map(|&b| b != 0),
            0x1D => strike = d.first().map(|&b| b != 0),
            _ => {}
        }
    }
    let mut x = String::from("<dxf>");
    if bold.is_some() || italic.is_some() || strike.is_some() || underline.is_some() || text_color.is_some() {
        x.push_str("<font>");
        if let Some(b) = bold {
            x.push_str(if b { "<b/>" } else { "<b val=\"0\"/>" });
        }
        if let Some(i) = italic {
            x.push_str(if i { "<i/>" } else { "<i val=\"0\"/>" });
        }
        if let Some(s) = strike {
            x.push_str(if s { "<strike/>" } else { "<strike val=\"0\"/>" });
        }
        match underline {
            Some(1) => x.push_str("<u/>"),
            Some(2) => x.push_str("<u val=\"double\"/>"),
            Some(0) => x.push_str("<u val=\"none\"/>"),
            _ => {}
        }
        if let Some(col) = text_color {
            x.push_str(&col);
        }
        x.push_str("</font>");
    }
    if pattern.is_some() || fg.is_some() || bg.is_some() {
        x.push_str("<fill><patternFill");
        if let Some(p) = pattern {
            let _ = write!(x, " patternType=\"{}\"", pattern_name(p as u32));
        }
        x.push('>');
        x.push_str(&fg.unwrap_or_default());
        x.push_str(&bg.unwrap_or_default());
        x.push_str("</patternFill></fill>");
    }
    if sides.iter().any(|s| s.is_some()) {
        let [top, bottom, left, right] = sides;
        let _ = write!(x, "<border>{}{}{}{}</border>", left.unwrap_or_default(), right.unwrap_or_default(), top.unwrap_or_default(), bottom.unwrap_or_default());
    }
    x.push_str("</dxf>");
    x
}

fn styles_xml(data: &[u8]) -> String {
    let mut dxfs = String::new();
    let mut dxf_count = 0;
    let mut num_fmts = String::new();
    let mut num_fmt_count = 0;
    let mut fonts = String::new();
    let mut font_count = 0;
    let mut fills = String::new();
    let mut fill_count = 0;
    let mut borders = String::new();
    let mut border_count = 0;
    let mut style_xfs = String::new();
    let mut style_xf_count = 0;
    let mut cell_xfs = String::new();
    let mut cell_xf_count = 0;
    let mut cell_styles = String::new();
    let mut cell_style_count = 0;
    let mut in_cell_xfs = false;
    for (id, rec) in records(data) {
        let mut c = Cur::new(rec);
        match id {
            44 => {
                if let (Some(ifmt), Some(code)) = (c.u16(), c.wstr()) {
                    let _ = write!(num_fmts, "<numFmt numFmtId=\"{ifmt}\" formatCode=\"{}\"/>", esc(&code));
                    num_fmt_count += 1;
                }
            }
            43 => {
                let mut f = || -> Option<String> {
                    let height = c.u16()?;
                    let flags = c.u16()?;
                    let weight = c.u16()?;
                    let sss = c.u16()?;
                    let uls = c.u8()?;
                    let family = c.u8()?;
                    let charset = c.u8()?;
                    c.u8()?;
                    let color = c.take(8)?;
                    let scheme = c.u8()?;
                    let name = c.wstr()?;
                    let mut x = String::from("<font>");
                    if weight >= 600 {
                        x.push_str("<b/>");
                    }
                    if flags & 0x0002 != 0 {
                        x.push_str("<i/>");
                    }
                    if flags & 0x0008 != 0 {
                        x.push_str("<strike/>");
                    }
                    if flags & 0x0010 != 0 {
                        x.push_str("<outline/>");
                    }
                    if flags & 0x0020 != 0 {
                        x.push_str("<shadow/>");
                    }
                    match uls {
                        1 => x.push_str("<u/>"),
                        2 => x.push_str("<u val=\"double\"/>"),
                        0x21 => x.push_str("<u val=\"singleAccounting\"/>"),
                        0x22 => x.push_str("<u val=\"doubleAccounting\"/>"),
                        _ => {}
                    }
                    match sss {
                        1 => x.push_str("<vertAlign val=\"superscript\"/>"),
                        2 => x.push_str("<vertAlign val=\"subscript\"/>"),
                        _ => {}
                    }
                    let _ = write!(x, "<sz val=\"{}\"/>", height as f64 / 20.0);
                    if let Some(col) = color_xml("color", color) {
                        x.push_str(&col);
                    }
                    let _ = write!(x, "<name val=\"{}\"/>", esc(&name));
                    if family != 0 {
                        let _ = write!(x, "<family val=\"{family}\"/>");
                    }
                    if charset != 0 && charset != 1 {
                        let _ = write!(x, "<charset val=\"{charset}\"/>");
                    }
                    match scheme {
                        1 => x.push_str("<scheme val=\"major\"/>"),
                        2 => x.push_str("<scheme val=\"minor\"/>"),
                        _ => {}
                    }
                    x.push_str("</font>");
                    Some(x)
                };
                fonts.push_str(&f().unwrap_or_else(|| "<font><sz val=\"11\"/><name val=\"Calibri\"/></font>".to_string()));
                font_count += 1;
            }
            45 => {
                let mut f = || -> Option<String> {
                    let fls = c.u32()?;
                    let fg = c.take(8)?;
                    let bg = c.take(8)?;
                    if fls == 0x28 {
                        let kind = c.u32()?;
                        let degree = c.f64()?;
                        let (l, r, t, b) = (c.f64()?, c.f64()?, c.f64()?, c.f64()?);
                        let n = c.u32()?;
                        let mut x = if kind == 0 { format!("<fill><gradientFill degree=\"{degree}\">") } else { format!("<fill><gradientFill type=\"path\" left=\"{l}\" right=\"{r}\" top=\"{t}\" bottom=\"{b}\">") };
                        for _ in 0..n.min(64) {
                            let col = c.take(8)?;
                            let pos = c.f64()?;
                            let _ = write!(x, "<stop position=\"{pos}\">{}</stop>", color_xml("color", col).unwrap_or_else(|| "<color auto=\"1\"/>".into()));
                        }
                        x.push_str("</gradientFill></fill>");
                        return Some(x);
                    }
                    let mut x = format!("<fill><patternFill patternType=\"{}\">", pattern_name(fls));
                    if fls != 0 {
                        if let Some(col) = color_xml("fgColor", fg) {
                            x.push_str(&col);
                        }
                        if let Some(col) = color_xml("bgColor", bg) {
                            x.push_str(&col);
                        }
                    }
                    x.push_str("</patternFill></fill>");
                    Some(x)
                };
                fills.push_str(&f().unwrap_or_else(|| "<fill><patternFill patternType=\"none\"/></fill>".to_string()));
                fill_count += 1;
            }
            46 => {
                let mut f = || -> Option<String> {
                    let flags = c.u8()?;
                    let mut side = |tag: &str| -> Option<String> {
                        let dg = c.u8()?;
                        c.u8()?;
                        let col = c.take(8)?;
                        Some(match border_style(dg) {
                            Some(style) => format!("<{tag} style=\"{style}\">{}</{tag}>", color_xml("color", col).unwrap_or_else(|| "<color auto=\"1\"/>".into())),
                            None => format!("<{tag}/>"),
                        })
                    };
                    let (top, bottom, left, right, diag) = (side("top")?, side("bottom")?, side("left")?, side("right")?, side("diagonal")?);
                    let mut x = String::from("<border");
                    if flags & 1 != 0 {
                        x.push_str(" diagonalDown=\"1\"");
                    }
                    if flags & 2 != 0 {
                        x.push_str(" diagonalUp=\"1\"");
                    }
                    let _ = write!(x, ">{left}{right}{top}{bottom}{diag}</border>");
                    Some(x)
                };
                borders.push_str(&f().unwrap_or_else(|| "<border><left/><right/><top/><bottom/><diagonal/></border>".to_string()));
                border_count += 1;
            }
            47 => {
                let mut f = || -> Option<(String, bool)> {
                    let parent = c.u16()?;
                    let fmt = c.u16()?;
                    let font = c.u16()?;
                    let fill = c.u16()?;
                    let border = c.u16()?;
                    let rot = c.u8()?;
                    let indent = c.u8()?;
                    let f1 = c.u16()?;
                    let alc = f1 & 7;
                    let alcv = (f1 >> 3) & 7;
                    let wrap = f1 & 0x40 != 0;
                    let shrink = f1 & 0x100 != 0;
                    let locked = f1 & 0x1000 != 0;
                    let hidden = f1 & 0x2000 != 0;
                    let mut x = format!("<xf numFmtId=\"{fmt}\" fontId=\"{font}\" fillId=\"{fill}\" borderId=\"{border}\"");
                    if parent != 0xFFFF {
                        let _ = write!(x, " xfId=\"{parent}\"");
                    }
                    x.push_str(" applyNumberFormat=\"1\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\" applyAlignment=\"1\">");
                    let horizontal = match alc {
                        1 => Some("left"),
                        2 => Some("center"),
                        3 => Some("right"),
                        4 => Some("fill"),
                        5 => Some("justify"),
                        6 => Some("centerContinuous"),
                        7 => Some("distributed"),
                        _ => None,
                    };
                    let vertical = match alcv {
                        0 => Some("top"),
                        1 => Some("center"),
                        3 => Some("justify"),
                        4 => Some("distributed"),
                        _ => None,
                    };
                    let mut a = String::new();
                    if let Some(h) = horizontal {
                        let _ = write!(a, " horizontal=\"{h}\"");
                    }
                    if let Some(v) = vertical {
                        let _ = write!(a, " vertical=\"{v}\"");
                    }
                    if rot != 0 && rot != 254 {
                        let _ = write!(a, " textRotation=\"{rot}\"");
                    }
                    if wrap {
                        a.push_str(" wrapText=\"1\"");
                    }
                    if indent != 0 {
                        let _ = write!(a, " indent=\"{indent}\"");
                    }
                    if shrink {
                        a.push_str(" shrinkToFit=\"1\"");
                    }
                    if !a.is_empty() {
                        let _ = write!(x, "<alignment{a}/>");
                    }
                    if !locked || hidden {
                        let _ = write!(x, "<protection locked=\"{}\" hidden=\"{}\"/>", locked as u8, hidden as u8);
                    }
                    x.push_str("</xf>");
                    Some((x, parent == 0xFFFF))
                };
                let x = f().map(|(x, _)| x).unwrap_or_else(|| "<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/>".to_string());
                // The records after BrtBeginCellStyleXFs are the styles' ones, the ones after BrtBeginCellXFs the cells'.
                if !in_cell_xfs {
                    style_xfs.push_str(&x);
                    style_xf_count += 1;
                } else {
                    cell_xfs.push_str(&x);
                    cell_xf_count += 1;
                }
            }
            507 => {
                dxfs.push_str(&dxf_xml(rec));
                dxf_count += 1;
            }
            617 => in_cell_xfs = true,
            626 => in_cell_xfs = false,
            48 => {
                if let (Some(ixf), Some(flags), Some(built_in), Some(level), Some(name)) = (c.u32(), c.u16(), c.u8(), c.u8(), c.wstr()) {
                    let mut x = format!("<cellStyle name=\"{}\" xfId=\"{ixf}\"", esc(&name));
                    if flags & 1 != 0 {
                        let _ = write!(x, " builtinId=\"{built_in}\"");
                        if built_in == 1 || built_in == 2 {
                            let _ = write!(x, " iLevel=\"{level}\"");
                        }
                    }
                    x.push_str("/>");
                    cell_styles.push_str(&x);
                    cell_style_count += 1;
                }
            }
            _ => {}
        }
    }
    if font_count == 0 {
        fonts.push_str("<font><sz val=\"11\"/><name val=\"Calibri\"/></font>");
        font_count = 1;
    }
    if fill_count < 2 {
        fills = "<fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill>".to_string();
        fill_count = 2;
    }
    if border_count == 0 {
        borders.push_str("<border><left/><right/><top/><bottom/><diagonal/></border>");
        border_count = 1;
    }
    if style_xf_count == 0 {
        style_xfs.push_str("<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\"/>");
        style_xf_count = 1;
    }
    if cell_xf_count == 0 {
        cell_xfs.push_str("<xf numFmtId=\"0\" fontId=\"0\" fillId=\"0\" borderId=\"0\" xfId=\"0\"/>");
        cell_xf_count = 1;
    }
    let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<styleSheet xmlns=\"{MAIN}\">");
    if num_fmt_count > 0 {
        let _ = write!(x, "<numFmts count=\"{num_fmt_count}\">{num_fmts}</numFmts>");
    }
    let _ = write!(x, "<fonts count=\"{font_count}\">{fonts}</fonts><fills count=\"{fill_count}\">{fills}</fills><borders count=\"{border_count}\">{borders}</borders>");
    let _ = write!(x, "<cellStyleXfs count=\"{style_xf_count}\">{style_xfs}</cellStyleXfs><cellXfs count=\"{cell_xf_count}\">{cell_xfs}</cellXfs>");
    if cell_style_count > 0 {
        let _ = write!(x, "<cellStyles count=\"{cell_style_count}\">{cell_styles}</cellStyles>");
    }
    let _ = write!(x, "<dxfs count=\"{dxf_count}\">{dxfs}</dxfs></styleSheet>");
    x
}

// ---- shared strings ----

fn shared_strings(data: &[u8]) -> (String, Vec<String>) {
    let mut items: Vec<String> = Vec::new();
    for (id, rec) in records(data) {
        if id == 19 {
            let mut c = Cur::new(rec);
            let _flags = c.u8();
            items.push(c.wstr().unwrap_or_default());
        }
    }
    let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<sst xmlns=\"{MAIN}\" count=\"{n}\" uniqueCount=\"{n}\">", n = items.len());
    for s in &items {
        let _ = write!(x, "<si>{}</si>", text_element("t", s));
    }
    x.push_str("</sst>");
    (x, items)
}

// ---- the workbook ----

struct SheetRef {
    state: u32,
    id: u32,
    rel: String,
    name: String,
}

struct NameRec {
    flags: u32,
    scope: u32,
    name: String,
    rgce: Vec<u8>,
    rgcb: Vec<u8>,
}

struct BookProtectionRec {
    lock_structure: bool,
    lock_windows: bool,
    lock_revision: bool,
    password: Option<String>,
    revisions_password: Option<String>,
}

#[derive(Default)]
struct Book {
    date1904: bool,
    active: u32,
    protection: Option<BookProtectionRec>,
    sheets: Vec<SheetRef>,
    names: Vec<NameRec>,
    cx: Context,
}

fn read_workbook(data: &[u8]) -> Book {
    let mut b = Book::default();
    let mut in_externals = false;
    for (id, rec) in records(data) {
        let mut c = Cur::new(rec);
        match id {
            153 => b.date1904 = c.u32().is_some_and(|f| f & 1 != 0),
            158 => {
                // BrtBookView: the active sheet is the 7th 32-bit field.
                for _ in 0..6 {
                    c.u32();
                }
                if let Some(cur) = c.u32() {
                    b.active = cur;
                }
            }
            156 => {
                if let (Some(state), Some(tab), Some(rel), Some(name)) = (c.u32(), c.u32(), c.wstr(), c.wstr()) {
                    b.sheets.push(SheetRef { state, id: tab, rel, name });
                }
            }
            39 => {
                let mut f = || -> Option<NameRec> {
                    let flags = c.u32()?;
                    c.u8()?;
                    let scope = c.u32()?;
                    let name = c.wstr()?;
                    let (rgce, rgcb) = parsed_formula(&mut c)?;
                    Some(NameRec { flags, scope, name, rgce: rgce.to_vec(), rgcb: rgcb.to_vec() })
                };
                match f() {
                    Some(n) => b.names.push(n),
                    // Keep the numbering of the names: later formulas count them.
                    None => b.names.push(NameRec { flags: 0, scope: 0xFFFF_FFFF, name: String::new(), rgce: Vec::new(), rgcb: Vec::new() }),
                }
            }
            353 => in_externals = true,
            354 => in_externals = false,
            // BrtSupSelf, BrtSupSame: this workbook; BrtSupBookSrc, BrtSupAddin: another.
            357 | 358 if in_externals => b.cx.internal_book.push(true),
            355 | 667 if in_externals => b.cx.internal_book.push(false),
            362 => {
                if let Some(n) = c.u32() {
                    for _ in 0..n {
                        let (Some(book), Some(first), Some(last)) = (c.u32(), c.i32(), c.i32()) else { break };
                        b.cx.xti.push((book as u16, first.max(0) as u16, last.max(0) as u16));
                    }
                }
            }
            534 => {
                let mut f = || -> Option<BookProtectionRec> {
                    let protpwd_book = c.u16()?;
                    let protpwd_rev = c.u16()?;
                    let flags = c.u16()?;
                    let lock_structure = flags & 1 != 0;
                    let lock_windows = flags & 2 != 0;
                    let lock_revision = flags & 4 != 0;
                    let password = if protpwd_book != 0 { Some(format!("{:04X}", protpwd_book)) } else { None };
                    let revisions_password = if protpwd_rev != 0 { Some(format!("{:04X}", protpwd_rev)) } else { None };
                    Some(BookProtectionRec {
                        lock_structure,
                        lock_windows,
                        lock_revision,
                        password,
                        revisions_password,
                    })
                };
                if let Some(p) = f() {
                    b.protection = Some(p);
                }
            }
            _ => {}
        }
    }
    b.cx.sheets = b.sheets.iter().map(|s| s.name.clone()).collect();
    // The names' texts for ptgName, then their formulas.
    b.cx.names = b.names.iter().map(|n| n.name.clone()).collect();
    b
}

fn workbook_xml(book: &Book) -> String {
    let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook xmlns=\"{MAIN}\" xmlns:r=\"{REL}\">");
    x.push_str(if book.date1904 { "<workbookPr date1904=\"1\"/>" } else { "<workbookPr/>" });
    if let Some(p) = &book.protection {
        let mut prot = String::from("<workbookProtection");
        if let Some(pwd) = &p.password {
            let _ = write!(prot, " workbookPassword=\"{pwd}\"");
        }
        if let Some(rev_pwd) = &p.revisions_password {
            let _ = write!(prot, " revisionsPassword=\"{rev_pwd}\"");
        }
        if p.lock_structure {
            prot.push_str(" lockStructure=\"1\"");
        }
        if p.lock_windows {
            prot.push_str(" lockWindows=\"1\"");
        }
        if p.lock_revision {
            prot.push_str(" lockRevision=\"1\"");
        }
        prot.push_str("/>");
        x.push_str(&prot);
    }
    let _ = write!(x, "<bookViews><workbookView activeTab=\"{}\"/></bookViews><sheets>", book.active);
    for s in &book.sheets {
        let state = match s.state {
            1 => " state=\"hidden\"",
            2 => " state=\"veryHidden\"",
            _ => "",
        };
        let _ = write!(x, "<sheet name=\"{}\" sheetId=\"{}\"{state} r:id=\"{}\"/>", esc(&s.name), s.id, esc(&s.rel));
    }
    x.push_str("</sheets>");
    let mut names = String::new();
    for n in &book.names {
        if n.name.is_empty() {
            continue;
        }
        let Some(text) = decompile(&n.rgce, &n.rgcb, (0, 0), &book.cx) else { continue };
        let name = if n.flags & 0x20 != 0 && !n.name.starts_with("_xlnm.") { format!("_xlnm.{}", n.name) } else { n.name.clone() };
        let scope = if n.scope == 0xFFFF_FFFF { String::new() } else { format!(" localSheetId=\"{}\"", n.scope) };
        let hidden = if n.flags & 1 != 0 { " hidden=\"1\"" } else { "" };
        let _ = write!(names, "<definedName name=\"{}\"{scope}{hidden}>{}</definedName>", esc(&name), esc(&text));
    }
    if !names.is_empty() {
        let _ = write!(x, "<definedNames>{names}</definedNames>");
    }
    x.push_str("</workbook>");
    x
}

// ---- a sheet ----

struct CellOut {
    row: u32,
    col: u32,
    style: u32,
    /// The attributes' `t` and the value element's text.
    kind: &'static str,
    value: String,
    inline: Option<String>,
    formula: Option<String>,
    /// The `ref` of an array formula.
    array: Option<String>,
}

fn rk_number(v: u32) -> f64 {
    let n = if v & 2 != 0 { (v as i32 >> 2) as f64 } else { f64::from_bits(((v & 0xFFFF_FFFC) as u64) << 32) };
    if v & 1 != 0 { n / 100.0 } else { n }
}

/// A conditional formatting rule being read: its header and the records inside it.
#[derive(Default)]
struct Rule {
    ty: u32,
    template: u32,
    dxf: u32,
    priority: i32,
    param: u32,
    flags: u16,
    text: String,
    formulas: Vec<String>,
    /// `cfvo` elements and colours of a colour scale, a data bar or an icon set.
    values: Vec<String>,
    colors: Vec<String>,
    /// 0 none, 1 colour scale, 2 data bar, 3 icon set.
    kind: u8,
    bar: (u8, u8, bool),
    icons: (u32, bool),
}

fn icon_set_name(i: u32) -> &'static str {
    [
        "3Arrows", "3ArrowsGray", "3Flags", "3TrafficLights1", "3TrafficLights2", "3Signs", "3Symbols", "3Symbols2", "4Arrows", "4ArrowsGray", "4RedToBlack", "4Rating", "4TrafficLights", "5Arrows",
        "5ArrowsGray", "5Rating", "5Quarters",
    ]
    .get(i as usize)
    .copied()
    .unwrap_or("3TrafficLights1")
}

fn rule_xml(r: &Rule) -> Option<String> {
    let stop = if r.flags & 0x0002 != 0 { " stopIfTrue=\"1\"" } else { "" };
    let dxf = if r.dxf != 0xFFFF_FFFF { format!(" dxfId=\"{}\"", r.dxf) } else { String::new() };
    let formulas: String = r.formulas.iter().map(|f| format!("<formula>{}</formula>", esc(f))).collect();
    Some(match r.kind {
        1 => format!("<cfRule type=\"colorScale\" priority=\"{}\"><colorScale>{}{}</colorScale></cfRule>", r.priority, r.values.concat(), r.colors.concat()),
        2 => format!(
            "<cfRule type=\"dataBar\" priority=\"{}\"><dataBar minLength=\"{}\" maxLength=\"{}\"{}>{}{}</dataBar></cfRule>",
            r.priority,
            r.bar.0,
            r.bar.1,
            if r.bar.2 { "" } else { " showValue=\"0\"" },
            r.values.concat(),
            r.colors.first().cloned().unwrap_or_default()
        ),
        3 => format!("<cfRule type=\"iconSet\" priority=\"{}\"><iconSet iconSet=\"{}\"{}>{}</iconSet></cfRule>", r.priority, icon_set_name(r.icons.0), if r.icons.1 { "" } else { " showValue=\"0\"" }, r.values.concat()),
        _ => match (r.ty, r.template) {
            (1, _) => {
                let op = ["", "between", "notBetween", "equal", "notEqual", "greaterThan", "lessThan", "greaterThanOrEqual", "lessThanOrEqual"].get(r.param as usize).copied().filter(|o| !o.is_empty())?;
                format!("<cfRule type=\"cellIs\"{dxf} priority=\"{}\"{stop} operator=\"{op}\">{formulas}</cfRule>", r.priority)
            }
            (2, 7) => format!("<cfRule type=\"uniqueValues\"{dxf} priority=\"{}\"{stop}/>", r.priority),
            (2, _) if !r.formulas.is_empty() => format!("<cfRule type=\"expression\"{dxf} priority=\"{}\"{stop}>{}</cfRule>", r.priority, formulas),
            (5, _) => format!(
                "<cfRule type=\"top10\"{dxf} priority=\"{}\"{stop}{}{} rank=\"{}\"/>",
                r.priority,
                if r.flags & 0x0010 != 0 { " percent=\"1\"" } else { "" },
                if r.flags & 0x0008 != 0 { " bottom=\"1\"" } else { "" },
                r.param
            ),
            _ => return None,
        },
    })
}

/// A `BrtCFVO` as a `cfvo` element.
fn cfvo_xml(rec: &[u8], cx: &Context) -> Option<String> {
    let mut c = Cur::new(rec);
    let ty = c.u32()?;
    let num = c.f64()?;
    c.u32()?;
    c.u32()?;
    let cb = c.u32()?;
    let formula = if cb > 0 {
        let (rgce, rgcb) = parsed_formula(&mut c)?;
        decompile(rgce, rgcb, (0, 0), cx)
    } else {
        None
    };
    let val = formula.unwrap_or_else(|| ibis_calc::number_to_xml(num));
    Some(match ty {
        1 => format!("<cfvo type=\"num\" val=\"{}\"/>", esc(&val)),
        2 => "<cfvo type=\"min\"/>".to_string(),
        3 => "<cfvo type=\"max\"/>".to_string(),
        4 => format!("<cfvo type=\"percent\" val=\"{}\"/>", esc(&val)),
        5 => format!("<cfvo type=\"percentile\" val=\"{}\"/>", esc(&val)),
        _ => format!("<cfvo type=\"formula\" val=\"{}\"/>", esc(&val)),
    })
}

/// A `UncheckedSqRfX` as a `sqref` text.
fn sqref(c: &mut Cur) -> Option<String> {
    let n = c.u32()?;
    let mut parts = Vec::new();
    for _ in 0..n.min(8192) {
        let (r1, r2, c1, c2) = (c.u32()?, c.u32()?, c.u32()?, c.u32()?);
        parts.push(range_ref(r1, r2, c1, c2));
    }
    Some(parts.join(" "))
}

struct SheetXml {
    xml: String,
}

fn sheet_xml(data: &[u8], strings: &[String], cx: &Context) -> SheetXml {
    let mut view = String::new();
    let mut pane = String::new();
    let mut format_pr = String::from("<sheetFormatPr defaultRowHeight=\"15\"/>");
    let mut cols = String::new();
    let mut merges: Vec<String> = Vec::new();
    let mut links = String::new();
    let mut margins = String::new();
    let mut page_setup = String::new();
    let mut drawing = String::new();
    let mut rows: Vec<(u32, String)> = Vec::new();
    let mut cells: Vec<CellOut> = Vec::new();
    // Shared formulas seen: (rows, columns, tokens, extra data).
    let mut shared: Vec<((u32, u32, u32, u32), Vec<u8>, Vec<u8>)> = Vec::new();
    let mut pending: Option<usize> = None;
    let mut row = 0u32;
    let mut zoom = 100u16;
    let mut grid = true;
    let mut selected = false;
    let mut rtl = false;
    let mut formulas_shown = false;
    let mut show_zeros = true;
    let mut fit_to_page = false;
    let mut tab_color = String::new();
    let mut auto_filter = String::new();
    let mut validations = String::new();
    let mut validation_count = 0;
    let mut conditional = String::new();
    let mut cf_ranges = String::new();
    let mut cf_rules = String::new();
    let mut rule: Option<Rule> = None;
    let mut legacy = String::new();
    let mut sheet_protection = String::new();
    let mut in_row_breaks = false;
    let mut in_col_breaks = false;
    let mut row_breaks: Vec<(u32, u32, bool)> = Vec::new();
    let mut col_breaks: Vec<(u32, u32, bool)> = Vec::new();

    for (id, rec) in records(data) {
        let mut c = Cur::new(rec);
        match id {
            0 => {
                let mut f = || -> Option<()> {
                    row = c.u32()?;
                    let ixfe = c.u32()?;
                    let height = c.u16()?;
                    let b0 = c.u8()?;
                    let b1 = c.u8()?;
                    let _ = b0;
                    let level = b1 & 7;
                    let collapsed = b1 & 0x08 != 0;
                    let hidden = b1 & 0x10 != 0;
                    let custom_height = b1 & 0x20 != 0;
                    let styled = b1 & 0x40 != 0;
                    let mut a = format!(" r=\"{}\"", row + 1);
                    if styled {
                        let _ = write!(a, " s=\"{ixfe}\" customFormat=\"1\"");
                    }
                    if custom_height {
                        let _ = write!(a, " ht=\"{}\" customHeight=\"1\"", height as f64 / 20.0);
                    }
                    if hidden {
                        a.push_str(" hidden=\"1\"");
                    }
                    if level > 0 {
                        let _ = write!(a, " outlineLevel=\"{level}\"");
                    }
                    if collapsed {
                        a.push_str(" collapsed=\"1\"");
                    }
                    rows.push((row, a));
                    Some(())
                };
                f();
            }
            1..=11 => {
                let mut f = || -> Option<()> {
                    let col = c.u32()?;
                    let style = c.u32()? & 0x00FF_FFFF;
                    let mut cell = CellOut { row, col, style, kind: "", value: String::new(), inline: None, formula: None, array: None };
                    let mut formula: Option<(&[u8], &[u8])> = None;
                    match id {
                        1 => {}
                        2 => cell.value = ibis_calc::number_to_xml(rk_number(c.u32()?)),
                        3 => {
                            cell.kind = "e";
                            cell.value = error_text(c.u8()?).to_string();
                        }
                        4 => {
                            cell.kind = "b";
                            cell.value = (c.u8()? & 1).to_string();
                        }
                        5 => cell.value = ibis_calc::number_to_xml(c.f64()?),
                        6 => cell.inline = Some(c.wstr()?),
                        7 => {
                            let s = strings.get(c.u32()? as usize)?;
                            cell.inline = Some(s.clone());
                        }
                        8 => {
                            cell.kind = "str";
                            cell.value = c.wstr()?;
                            c.u16()?;
                            formula = parsed_formula(&mut c);
                        }
                        9 => {
                            cell.value = ibis_calc::number_to_xml(c.f64()?);
                            c.u16()?;
                            formula = parsed_formula(&mut c);
                        }
                        10 => {
                            cell.kind = "b";
                            cell.value = (c.u8()? & 1).to_string();
                            c.u16()?;
                            formula = parsed_formula(&mut c);
                        }
                        11 => {
                            cell.kind = "e";
                            cell.value = error_text(c.u8()?).to_string();
                            c.u16()?;
                            formula = parsed_formula(&mut c);
                        }
                        _ => return None,
                    }
                    if let Some((rgce, rgcb)) = formula {
                        let base = (row, col);
                        if rgce.first() == Some(&0x01) {
                            // A cell of a shared or array formula: the tokens are in the record that holds the range.
                            let (r, k) = (u32::from_le_bytes(rgce.get(1..5)?.try_into().ok()?), if rgcb.len() >= 4 { u32::from_le_bytes(rgcb[..4].try_into().ok()?) } else { col });
                            match shared.iter().find(|(rf, _, _)| (rf.0..=rf.1).contains(&r) && (rf.2..=rf.3).contains(&k)) {
                                Some((_, tokens, extra)) => cell.formula = decompile(tokens, extra, base, cx),
                                None => pending = Some(cells.len()),
                            }
                        } else {
                            cell.formula = decompile(rgce, rgcb, base, cx);
                        }
                    }
                    cells.push(cell);
                    Some(())
                };
                f();
            }
            427 | 426 => {
                let mut f = || -> Option<()> {
                    let rf = (c.u32()?, c.u32()?, c.u32()?, c.u32()?);
                    if id == 426 {
                        c.u8()?;
                    }
                    let (rgce, rgcb) = parsed_formula(&mut c)?;
                    if let Some(at) = pending.take() {
                        let cell = &mut cells[at];
                        cell.formula = decompile(rgce, rgcb, (cell.row, cell.col), cx);
                        if id == 426 {
                            cell.array = Some(range_ref(rf.0, rf.1, rf.2, rf.3));
                        }
                    }
                    if id == 427 {
                        shared.push((rf, rgce.to_vec(), rgcb.to_vec()));
                    }
                    Some(())
                };
                f();
            }
            60 => {
                let mut f = || -> Option<()> {
                    let first = c.u32()?;
                    let last = c.u32()?.min(16383);
                    let width = c.u32()?;
                    let style = c.u32()?;
                    let b0 = c.u8()?;
                    let b1 = c.u8()?;
                    let hidden = b0 & 1 != 0;
                    let custom = b0 & 2 != 0;
                    let level = b1 & 7;
                    let collapsed = b1 & 0x10 != 0;
                    let _ = write!(cols, "<col min=\"{}\" max=\"{}\" width=\"{}\"", first + 1, last + 1, width as f64 / 256.0);
                    if custom {
                        cols.push_str(" customWidth=\"1\"");
                    }
                    if style != 0 {
                        let _ = write!(cols, " style=\"{style}\"");
                    }
                    if hidden {
                        cols.push_str(" hidden=\"1\"");
                    }
                    if level > 0 {
                        let _ = write!(cols, " outlineLevel=\"{level}\"");
                    }
                    if collapsed {
                        cols.push_str(" collapsed=\"1\"");
                    }
                    cols.push_str("/>");
                    Some(())
                };
                f();
            }
            176 => {
                if let (Some(r1), Some(r2), Some(c1), Some(c2)) = (c.u32(), c.u32(), c.u32(), c.u32()) {
                    merges.push(range_ref(r1, r2, c1, c2));
                }
            }
            147 => {
                // BrtWsProp: fFitToPage is bit 8 of the first 16 bits; the tab's colour follows the 3 bytes of flags.
                fit_to_page = c.u16().is_some_and(|f| f & 0x0100 != 0);
                c.u8();
                if let Some(col) = c.take(8) {
                    if let Some(x) = color_xml("tabColor", col) {
                        tab_color = x;
                    }
                }
            }
            161 => {
                if let (Some(r1), Some(r2), Some(c1), Some(c2)) = (c.u32(), c.u32(), c.u32(), c.u32()) {
                    auto_filter = format!("<autoFilter ref=\"{}\"/>", range_ref(r1, r2, c1, c2));
                }
            }
            551 => {
                if let Some(rel) = c.wstr() {
                    legacy = format!("<legacyDrawing r:id=\"{}\"/>", esc(&rel));
                }
            }
            64 => {
                let mut f = || -> Option<()> {
                    let flags = c.u32()?;
                    let sq = sqref(&mut c)?;
                    let titles: Vec<String> = (0..4).map(|_| c.wstr()).collect::<Option<_>>()?;
                    let mut formula = || -> Option<String> {
                        let (rgce, rgcb) = parsed_formula(&mut c)?;
                        if rgce.is_empty() { Some(String::new()) } else { decompile(rgce, rgcb, (0, 0), cx) }
                    };
                    let f1 = formula().unwrap_or_default();
                    let f2 = formula().unwrap_or_default();
                    let kind = match flags & 0xF {
                        1 => "whole",
                        2 => "decimal",
                        3 => "list",
                        4 => "date",
                        5 => "time",
                        6 => "textLength",
                        7 => "custom",
                        _ => return Some(()),
                    };
                    let op = ["between", "notBetween", "equal", "notEqual", "greaterThan", "lessThan", "greaterThanOrEqual", "lessThanOrEqual"].get(((flags >> 20) & 0xF) as usize).copied().unwrap_or("between");
                    let mut x = format!("<dataValidation type=\"{kind}\"");
                    match (flags >> 4) & 7 {
                        1 => x.push_str(" errorStyle=\"warning\""),
                        2 => x.push_str(" errorStyle=\"information\""),
                        _ => {}
                    }
                    if op != "between" && !matches!(kind, "list" | "custom") {
                        let _ = write!(x, " operator=\"{op}\"");
                    }
                    if flags & 0x100 != 0 {
                        x.push_str(" allowBlank=\"1\"");
                    }
                    if flags & 0x200 != 0 {
                        x.push_str(" showDropDown=\"1\"");
                    }
                    if flags & 0x40000 != 0 {
                        x.push_str(" showInputMessage=\"1\"");
                    }
                    if flags & 0x80000 != 0 {
                        x.push_str(" showErrorMessage=\"1\"");
                    }
                    for (attr, text) in ["errorTitle", "error", "promptTitle", "prompt"].iter().zip(&titles) {
                        if !text.is_empty() {
                            let _ = write!(x, " {attr}=\"{}\"", esc(text));
                        }
                    }
                    let _ = write!(x, " sqref=\"{sq}\">");
                    if !f1.is_empty() {
                        let _ = write!(x, "<formula1>{}</formula1>", esc(&f1));
                    }
                    if !f2.is_empty() {
                        let _ = write!(x, "<formula2>{}</formula2>", esc(&f2));
                    }
                    x.push_str("</dataValidation>");
                    validations.push_str(&x);
                    validation_count += 1;
                    Some(())
                };
                f();
            }
            461 => {
                cf_rules.clear();
                cf_ranges = (|| {
                    c.u32()?;
                    c.u32()?;
                    sqref(&mut c)
                })()
                .unwrap_or_default();
            }
            462 => {
                if !cf_rules.is_empty() && !cf_ranges.is_empty() {
                    let _ = write!(conditional, "<conditionalFormatting sqref=\"{cf_ranges}\">{cf_rules}</conditionalFormatting>");
                }
                cf_rules.clear();
            }
            463 => {
                let mut f = || -> Option<Rule> {
                    let mut r = Rule { ty: c.u32()?, template: c.u32()?, dxf: c.u32()?, priority: c.i32()?, param: c.u32()?, ..Default::default() };
                    c.u32()?;
                    c.u32()?;
                    r.flags = c.u16()?;
                    let lens = [c.u32()?, c.u32()?, c.u32()?];
                    r.text = c.wstr()?;
                    for len in lens {
                        if len > 0 {
                            let formula = parsed_formula(&mut c).and_then(|(rgce, rgcb)| decompile(rgce, rgcb, (0, 0), cx));
                            r.formulas.push(formula.unwrap_or_default());
                        }
                    }
                    Some(r)
                };
                rule = f();
            }
            464 => {
                if let Some(r) = rule.take() {
                    if let Some(x) = rule_xml(&r) {
                        cf_rules.push_str(&x);
                    }
                }
            }
            469 => {
                if let Some(r) = rule.as_mut() {
                    r.kind = 1;
                }
            }
            467 => {
                if let Some(r) = rule.as_mut() {
                    r.kind = 2;
                    r.bar = (c.u8().unwrap_or(0), c.u8().unwrap_or(100), c.u8().unwrap_or(1) != 0);
                }
            }
            465 => {
                if let Some(r) = rule.as_mut() {
                    r.kind = 3;
                    let set = c.u32().unwrap_or(0);
                    let flags = c.u16().unwrap_or(0);
                    r.icons = (set, flags & 0x0002 == 0);
                }
            }
            471 => {
                if let (Some(r), Some(x)) = (rule.as_mut(), cfvo_xml(rec, cx)) {
                    r.values.push(x);
                }
            }
            564 => {
                if let (Some(r), Some(col)) = (rule.as_mut(), c.take(8)) {
                    if let Some(x) = color_xml("color", col) {
                        r.colors.push(x);
                    }
                }
            }
            485 => {
                let mut f = || -> Option<()> {
                    let dx = c.u32()?;
                    let _cch = c.u16()?;
                    let height = c.u16()?;
                    let flags = c.u16()?;
                    let mut a = String::from("<sheetFormatPr");
                    if dx != 0xFFFF_FFFF {
                        let _ = write!(a, " defaultColWidth=\"{}\"", dx as f64 / 256.0);
                    }
                    if flags & 1 != 0 {
                        let _ = write!(a, " defaultRowHeight=\"{}\" customHeight=\"1\"", height as f64 / 20.0);
                    } else {
                        a.push_str(" defaultRowHeight=\"15\"");
                    }
                    if flags & 2 != 0 {
                        a.push_str(" zeroHeight=\"1\"");
                    }
                    a.push_str("/>");
                    format_pr = a;
                    Some(())
                };
                f();
            }
            137 => {
                let mut f = || -> Option<()> {
                    let flags = c.u16()?;
                    c.u32()?;
                    c.u32()?;
                    c.u32()?;
                    c.take(4)?;
                    zoom = c.u16()?;
                    formulas_shown = flags & 0x0002 != 0;
                    grid = flags & 0x0004 != 0;
                    show_zeros = flags & 0x0010 != 0;
                    rtl = flags & 0x0020 != 0;
                    selected = flags & 0x0040 != 0;
                    Some(())
                };
                f();
            }
            151 => {
                let mut f = || -> Option<()> {
                    let x = c.f64()?;
                    let y = c.f64()?;
                    let top = c.u32()?;
                    let left = c.u32()?;
                    let active = c.u32()?;
                    let flags = c.u8()?;
                    let frozen = flags & 3 != 0;
                    let mut a = String::from("<pane");
                    if x > 0.0 {
                        let _ = write!(a, " xSplit=\"{x}\"");
                    }
                    if y > 0.0 {
                        let _ = write!(a, " ySplit=\"{y}\"");
                    }
                    let _ = write!(a, " topLeftCell=\"{}\"", cell_ref(top, left));
                    let pane_name = match active {
                        0 => "bottomRight",
                        1 => "topRight",
                        2 => "bottomLeft",
                        _ => "topLeft",
                    };
                    let _ = write!(a, " activePane=\"{pane_name}\"");
                    if frozen {
                        a.push_str(" state=\"frozen\"");
                    }
                    a.push_str("/>");
                    pane = a;
                    Some(())
                };
                f();
            }
            476 => {
                let mut f = || -> Option<()> {
                    let v: Vec<f64> = (0..6).map(|_| c.f64()).collect::<Option<_>>()?;
                    margins = format!("<pageMargins left=\"{}\" right=\"{}\" top=\"{}\" bottom=\"{}\" header=\"{}\" footer=\"{}\"/>", v[0], v[1], v[2], v[3], v[4], v[5]);
                    Some(())
                };
                f();
            }
            478 => {
                let mut f = || -> Option<()> {
                    let paper = c.u32()?;
                    let scale = c.u32()?;
                    c.u32()?;
                    c.u32()?;
                    c.u32()?;
                    c.i32()?;
                    let fit_w = c.u32()?;
                    let fit_h = c.u32()?;
                    let flags = c.u16()?;
                    let mut a = String::from("<pageSetup");
                    if paper != 0 {
                        let _ = write!(a, " paperSize=\"{paper}\"");
                    }
                    if scale != 0 && scale != 100 {
                        let _ = write!(a, " scale=\"{scale}\"");
                    }
                    let _ = write!(a, " fitToWidth=\"{fit_w}\" fitToHeight=\"{fit_h}\"");
                    a.push_str(if flags & 2 != 0 { " orientation=\"landscape\"" } else { " orientation=\"portrait\"" });
                    a.push_str("/>");
                    page_setup = a;
                    Some(())
                };
                f();
            }
            494 => {
                let mut f = || -> Option<()> {
                    let (r1, r2, c1, c2) = (c.u32()?, c.u32()?, c.u32()?, c.u32()?);
                    let rel = c.wstr()?;
                    let location = c.wstr()?;
                    let tooltip = c.wstr()?;
                    let display = c.wstr()?;
                    let _ = write!(links, "<hyperlink ref=\"{}\"", range_ref(r1, r2, c1, c2));
                    if !rel.is_empty() {
                        let _ = write!(links, " r:id=\"{}\"", esc(&rel));
                    }
                    if !location.is_empty() {
                        let _ = write!(links, " location=\"{}\"", esc(&location));
                    }
                    if !tooltip.is_empty() {
                        let _ = write!(links, " tooltip=\"{}\"", esc(&tooltip));
                    }
                    if !display.is_empty() {
                        let _ = write!(links, " display=\"{}\"", esc(&display));
                    }
                    links.push_str("/>");
                    Some(())
                };
                f();
            }
            550 => {
                if let Some(rel) = c.wstr() {
                    drawing = format!("<drawing r:id=\"{}\"/>", esc(&rel));
                }
            }
            535 => {
                let mut f = || -> Option<()> {
                    let protpwd = c.u16()?;
                    let f_locked = c.u32()? != 0;
                    let f_objects = c.u32().unwrap_or(0) != 0;
                    let f_scenarios = c.u32().unwrap_or(0) != 0;
                    let f_format_cells = c.u32().unwrap_or(0) != 0;
                    let f_format_columns = c.u32().unwrap_or(0) != 0;
                    let f_format_rows = c.u32().unwrap_or(0) != 0;
                    let f_insert_columns = c.u32().unwrap_or(0) != 0;
                    let f_insert_rows = c.u32().unwrap_or(0) != 0;
                    let f_insert_hyperlinks = c.u32().unwrap_or(0) != 0;
                    let f_delete_columns = c.u32().unwrap_or(0) != 0;
                    let f_delete_rows = c.u32().unwrap_or(0) != 0;
                    let f_sel_locked_cells = c.u32().unwrap_or(1) != 0;
                    let f_sort = c.u32().unwrap_or(0) != 0;
                    let f_auto_filter = c.u32().unwrap_or(0) != 0;
                    let f_pivot_tables = c.u32().unwrap_or(0) != 0;
                    let f_sel_unlocked_cells = c.u32().unwrap_or(1) != 0;

                    if !f_locked {
                        return Some(());
                    }

                    let mut a = String::from("<sheetProtection sheet=\"1\"");
                    if protpwd != 0 {
                        let _ = write!(a, " password=\"{:04X}\"", protpwd);
                    }
                    if f_objects {
                        a.push_str(" objects=\"1\"");
                    }
                    if f_scenarios {
                        a.push_str(" scenarios=\"1\"");
                    }
                    if f_format_cells {
                        a.push_str(" formatCells=\"0\"");
                    }
                    if f_format_columns {
                        a.push_str(" formatColumns=\"0\"");
                    }
                    if f_format_rows {
                        a.push_str(" formatRows=\"0\"");
                    }
                    if f_insert_columns {
                        a.push_str(" insertColumns=\"0\"");
                    }
                    if f_insert_rows {
                        a.push_str(" insertRows=\"0\"");
                    }
                    if f_insert_hyperlinks {
                        a.push_str(" insertHyperlinks=\"0\"");
                    }
                    if f_delete_columns {
                        a.push_str(" deleteColumns=\"0\"");
                    }
                    if f_delete_rows {
                        a.push_str(" deleteRows=\"0\"");
                    }
                    if !f_sel_locked_cells {
                        a.push_str(" selectLockedCells=\"1\"");
                    }
                    if f_sort {
                        a.push_str(" sort=\"0\"");
                    }
                    if f_auto_filter {
                        a.push_str(" autoFilter=\"0\"");
                    }
                    if f_pivot_tables {
                        a.push_str(" pivotTables=\"0\"");
                    }
                    if !f_sel_unlocked_cells {
                        a.push_str(" selectUnlockedCells=\"1\"");
                    }
                    a.push_str("/>");
                    sheet_protection = a;
                    Some(())
                };
                f();
            }
            392 => in_row_breaks = true,
            393 => in_row_breaks = false,
            394 => in_col_breaks = true,
            395 => in_col_breaks = false,
            396 => {
                let mut f = || -> Option<()> {
                    let un_rw_col = c.u32()?;
                    let _un_col_rw_strt = c.u32()?;
                    let un_col_rw_end = c.u32()?;
                    let f_man = c.u32().unwrap_or(1) != 0;
                    let _f_pivot = c.u32();
                    if in_row_breaks {
                        row_breaks.push((un_rw_col, un_col_rw_end, f_man));
                    } else if in_col_breaks {
                        col_breaks.push((un_rw_col, un_col_rw_end, f_man));
                    }
                    Some(())
                };
                f();
            }
            _ => {}
        }
    }

    let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"{MAIN}\" xmlns:r=\"{REL}\">");
    if fit_to_page || !tab_color.is_empty() {
        let _ = write!(x, "<sheetPr>{tab_color}{}</sheetPr>", if fit_to_page { "<pageSetUpPr fitToPage=\"1\"/>" } else { "" });
    }
    let mut v = String::from("<sheetView");
    if !grid {
        v.push_str(" showGridLines=\"0\"");
    }
    if !show_zeros {
        v.push_str(" showZeros=\"0\"");
    }
    if formulas_shown {
        v.push_str(" showFormulas=\"1\"");
    }
    if rtl {
        v.push_str(" rightToLeft=\"1\"");
    }
    if selected {
        v.push_str(" tabSelected=\"1\"");
    }
    if zoom != 100 && zoom != 0 {
        let _ = write!(v, " zoomScale=\"{zoom}\"");
    }
    v.push_str(" workbookViewId=\"0\"");
    if pane.is_empty() {
        v.push_str("/>");
    } else {
        let _ = write!(v, ">{pane}</sheetView>");
    }
    view.push_str(&v);
    let _ = write!(x, "<sheetViews>{view}</sheetViews>{format_pr}");
    if !cols.is_empty() {
        let _ = write!(x, "<cols>{cols}</cols>");
    }
    x.push_str("<sheetData>");
    // Cells grouped by row, in the order they came.
    let mut at = 0;
    for (r, attrs) in &rows {
        let _ = write!(x, "<row{attrs}>");
        while at < cells.len() && cells[at].row == *r {
            let cell = &cells[at];
            at += 1;
            let _ = write!(x, "<c r=\"{}\"", cell_ref(cell.row, cell.col));
            if cell.style != 0 {
                let _ = write!(x, " s=\"{}\"", cell.style);
            }
            match &cell.inline {
                Some(s) => {
                    let _ = write!(x, " t=\"inlineStr\"><is>{}</is></c>", text_element("t", s));
                }
                None => {
                    if !cell.kind.is_empty() {
                        let _ = write!(x, " t=\"{}\"", cell.kind);
                    }
                    x.push('>');
                    if let Some(f) = &cell.formula {
                        match &cell.array {
                            Some(r) => {
                                let _ = write!(x, "<f t=\"array\" ref=\"{r}\">{}</f>", esc(f));
                            }
                            None => {
                                let _ = write!(x, "<f>{}</f>", esc(f));
                            }
                        }
                    }
                    if !cell.value.is_empty() || cell.kind == "str" {
                        let _ = write!(x, "<v>{}</v>", esc(&cell.value));
                    }
                    x.push_str("</c>");
                }
            }
        }
        x.push_str("</row>");
    }
    x.push_str("</sheetData>");
    x.push_str(&sheet_protection);
    x.push_str(&auto_filter);
    if !merges.is_empty() {
        let _ = write!(x, "<mergeCells count=\"{}\">", merges.len());
        for m in &merges {
            let _ = write!(x, "<mergeCell ref=\"{m}\"/>");
        }
        x.push_str("</mergeCells>");
    }
    x.push_str(&conditional);
    if validation_count > 0 {
        let _ = write!(x, "<dataValidations count=\"{validation_count}\">{validations}</dataValidations>");
    }
    if !links.is_empty() {
        let _ = write!(x, "<hyperlinks>{links}</hyperlinks>");
    }
    x.push_str(&margins);
    x.push_str(&page_setup);
    if !row_breaks.is_empty() {
        let manual_count = row_breaks.iter().filter(|(_, _, m)| *m).count();
        let _ = write!(x, "<rowBreaks count=\"{}\" manualBreakCount=\"{manual_count}\">", row_breaks.len());
        for (id, max, man) in &row_breaks {
            let max_val = if *max > 0 { *max } else { 16383 };
            let _ = write!(x, "<brk id=\"{id}\" max=\"{max_val}\"");
            if *man {
                x.push_str(" man=\"1\"");
            }
            x.push_str("/>");
        }
        x.push_str("</rowBreaks>");
    }
    if !col_breaks.is_empty() {
        let manual_count = col_breaks.iter().filter(|(_, _, m)| *m).count();
        let _ = write!(x, "<colBreaks count=\"{}\" manualBreakCount=\"{manual_count}\">", col_breaks.len());
        for (id, max, man) in &col_breaks {
            let max_val = if *max > 0 { *max } else { 1048575 };
            let _ = write!(x, "<brk id=\"{id}\" max=\"{max_val}\"");
            if *man {
                x.push_str(" man=\"1\"");
            }
            x.push_str("/>");
        }
        x.push_str("</colBreaks>");
    }
    x.push_str(&drawing);
    x.push_str(&legacy);
    x.push_str("</worksheet>");
    SheetXml { xml: x }
}

// ---- the package ----

struct Relationship {
    id: String,
    kind: String,
    target: String,
    external: bool,
}

fn parse_rels(data: &[u8]) -> Vec<Relationship> {
    let Ok(text) = std::str::from_utf8(data) else { return Vec::new() };
    let Ok(doc) = Document::parse(text) else { return Vec::new() };
    doc.root_element()
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "Relationship")
        .map(|n| Relationship {
            id: n.attribute("Id").unwrap_or_default().to_string(),
            kind: n.attribute("Type").unwrap_or_default().to_string(),
            target: n.attribute("Target").unwrap_or_default().to_string(),
            external: n.attribute("TargetMode") == Some("External"),
        })
        .collect()
}

/// The part a relationship of `base`'s part points at, as a path in the package.
fn resolve(base: &str, target: &str) -> String {
    if let Some(t) = target.strip_prefix('/') {
        return t.to_string();
    }
    let mut parts: Vec<&str> = base.split('/').collect();
    parts.pop();
    for seg in target.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn xml_name(path: &str) -> String {
    match path.strip_suffix(".bin") {
        Some(stem) => format!("{stem}.xml"),
        None => path.to_string(),
    }
}

/// The rels file of `part`, and the other way round.
fn rels_path(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, name)) => format!("{dir}/_rels/{name}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

pub fn to_xlsx(data: &[u8]) -> Result<Vec<u8>, Error> {
    let archive = crate::zip::Archive::new(data)?;
    let workbook = archive.read("xl/workbook.bin")?.ok_or_else(|| bad("workbook.bin がありません"))?;
    let book = read_workbook(&workbook);
    let book_rels = parse_rels(&archive.read("xl/_rels/workbook.bin.rels")?.unwrap_or_default());

    // The parts made from records: their path (in the xlsb) and the XML.
    let mut made: HashMap<String, String> = HashMap::new();
    let mut content_types: HashMap<String, &'static str> = HashMap::new();
    made.insert("xl/workbook.xml".into(), workbook_xml(&book));
    content_types.insert("/xl/workbook.xml".into(), "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml");

    let mut strings: Vec<String> = Vec::new();
    let mut converted_bins: Vec<String> = vec!["xl/workbook.bin".into()];
    for r in &book_rels {
        if r.external {
            continue;
        }
        let path = resolve("xl/workbook.bin", &r.target);
        if r.kind.ends_with("/sharedStrings") {
            if let Some(d) = archive.read(&path)? {
                let (xml, items) = shared_strings(&d);
                strings = items;
                made.insert(xml_name(&path), xml);
                content_types.insert(format!("/{}", xml_name(&path)), "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml");
                converted_bins.push(path);
            }
        } else if r.kind.ends_with("/styles") {
            if let Some(d) = archive.read(&path)? {
                made.insert(xml_name(&path), styles_xml(&d));
                content_types.insert(format!("/{}", xml_name(&path)), "application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml");
                converted_bins.push(path);
            }
        }
    }
    // The sheets (a chart sheet or a macro sheet becomes an empty worksheet).
    let mut sheet_kind_rel: HashMap<String, ()> = HashMap::new();
    for s in &book.sheets {
        let Some(r) = book_rels.iter().find(|r| r.id == s.rel) else { continue };
        let path = resolve("xl/workbook.bin", &r.target);
        let xml = if r.kind.ends_with("/worksheet") {
            match archive.read(&path)? {
                Some(d) => sheet_xml(&d, &strings, &book.cx).xml,
                None => continue,
            }
        } else {
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"{MAIN}\"><sheetData/></worksheet>")
        };
        made.insert(xml_name(&path), xml);
        content_types.insert(format!("/{}", xml_name(&path)), "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml");
        // The notes of the sheet.
        for rel in parse_rels(&archive.read(&rels_path(&path))?.unwrap_or_default()) {
            if rel.kind.ends_with("/comments") && !rel.external {
                let comments = resolve(&path, &rel.target);
                if let Some(d) = archive.read(&comments)? {
                    made.insert(xml_name(&comments), comments_xml(&d));
                    content_types.insert(format!("/{}", xml_name(&comments)), "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml");
                    converted_bins.push(comments);
                }
            }
        }
        sheet_kind_rel.insert(r.id.clone(), ());
        converted_bins.push(path);
    }

    let mut zip = ibis_ooxml::zip::Writer::new();
    let meta = ibis_ooxml::zip::EntryMeta::default();
    let put = |zip: &mut ibis_ooxml::zip::Writer, name: &str, bytes: &[u8]| zip.add(name, meta.clone(), bytes).map_err(|e| bad(&format!("{e}")));

    // [Content_Types].xml
    let ct_text = archive.read("[Content_Types].xml")?.map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let mut ct = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">");
    let mut have_xml_default = false;
    let mut have_rels_default = false;
    if let Ok(doc) = Document::parse(&ct_text) {
        for n in doc.root_element().children().filter(|n| n.is_element()) {
            match n.tag_name().name() {
                "Default" => {
                    let ext = n.attribute("Extension").unwrap_or_default();
                    if ext.eq_ignore_ascii_case("bin") {
                        continue;
                    }
                    have_xml_default |= ext == "xml";
                    have_rels_default |= ext == "rels";
                    let _ = write!(ct, "<Default Extension=\"{}\" ContentType=\"{}\"/>", esc(ext), esc(n.attribute("ContentType").unwrap_or_default()));
                }
                "Override" => {
                    let part = n.attribute("PartName").unwrap_or_default();
                    if part.ends_with(".bin") {
                        continue;
                    }
                    let _ = write!(ct, "<Override PartName=\"{}\" ContentType=\"{}\"/>", esc(part), esc(n.attribute("ContentType").unwrap_or_default()));
                }
                _ => {}
            }
        }
    }
    if !have_xml_default {
        ct.push_str("<Default Extension=\"xml\" ContentType=\"application/xml\"/>");
    }
    if !have_rels_default {
        ct.push_str("<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>");
    }
    let mut ordered: Vec<_> = content_types.iter().collect();
    ordered.sort();
    for (part, kind) in ordered {
        let _ = write!(ct, "<Override PartName=\"{part}\" ContentType=\"{kind}\"/>");
    }
    ct.push_str("</Types>");
    put(&mut zip, "[Content_Types].xml", ct.as_bytes())?;

    let mut names = archive.names();
    names.sort();
    for name in &names {
        if name == "[Content_Types].xml" {
            continue;
        }
        if name.ends_with(".rels") {
            // The rels of a part: kept (with the .bin parts renamed or dropped) when the part is.
            let part = if name == "_rels/.rels" {
                String::new()
            } else {
                let Some((dir, file)) = name.rsplit_once("/_rels/") else { continue };
                let file = file.trim_end_matches(".rels");
                format!("{dir}/{file}")
            };
            if part.ends_with(".bin") && !converted_bins.contains(&part) {
                continue;
            }
            let rels = parse_rels(&archive.read(name)?.unwrap_or_default());
            let base = if part.is_empty() { "x".to_string() } else { part.clone() };
            let mut x = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">");
            for r in &rels {
                let mut kind = r.kind.clone();
                let mut target = r.target.clone();
                if !r.external {
                    let path = resolve(&base, &r.target);
                    if path.ends_with(".bin") {
                        if !converted_bins.contains(&path) {
                            continue;
                        }
                        // The same place, as .xml.
                        target = xml_name(&target);
                        if part == "xl/workbook.bin" && sheet_kind_rel.contains_key(&r.id) {
                            kind = WORKSHEET_TYPE.to_string();
                        }
                    } else if !archive.contains(&path) {
                        continue;
                    }
                }
                let _ = write!(x, "<Relationship Id=\"{}\" Type=\"{}\" Target=\"{}\"{}/>", esc(&r.id), esc(&kind), esc(&target), if r.external { " TargetMode=\"External\"" } else { "" });
            }
            x.push_str("</Relationships>");
            let out = if part.is_empty() { name.clone() } else { rels_path(&xml_name(&part)) };
            put(&mut zip, &out, x.as_bytes())?;
            continue;
        }
        if name.ends_with(".bin") {
            continue;
        }
        if let Some(bytes) = archive.read(name)? {
            put(&mut zip, name, &bytes)?;
        }
    }
    let mut made_names: Vec<_> = made.keys().cloned().collect();
    made_names.sort();
    for name in made_names {
        put(&mut zip, &name, made[&name].as_bytes())?;
    }
    zip.finish().map_err(|e| bad(&format!("{e}")))
}

/// The comments part (`comments1.bin`) as the XML one.
fn comments_xml(data: &[u8]) -> String {
    let mut authors: Vec<String> = Vec::new();
    let mut list = String::new();
    let mut current: Option<(u32, String)> = None;
    for (id, rec) in records(data) {
        let mut c = Cur::new(rec);
        match id {
            632 => authors.push(c.wstr().unwrap_or_default()),
            635 => {
                let mut f = || -> Option<(u32, String)> {
                    let author = c.u32()?;
                    let (r1, _, c1, _) = (c.u32()?, c.u32()?, c.u32()?, c.u32()?);
                    Some((author, cell_ref(r1, c1)))
                };
                current = f();
            }
            637 => {
                if let Some((author, cell)) = current.take() {
                    c.u8();
                    let text = c.wstr().unwrap_or_default();
                    let _ = write!(list, "<comment ref=\"{cell}\" authorId=\"{author}\"><text>{}</text></comment>", format!("<r>{}</r>", text_element("t", &text)));
                }
            }
            _ => {}
        }
    }
    let mut x = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<comments xmlns=\"{MAIN}\"><authors>");
    for a in &authors {
        let _ = write!(x, "<author>{}</author>", esc(a));
    }
    let _ = write!(x, "</authors><commentList>{list}</commentList></comments>");
    x
}
