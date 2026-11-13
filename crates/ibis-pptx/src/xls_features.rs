// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What an old workbook (`.xls`) keeps beside its cells, read from the
//! public specification (MS-XLS): conditional formats (`CFHEADER`, `CF`,
//! with their `DXFN`), data validations (`DV`) and hyperlinks (`HLINK`),
//! each as the `.xlsx` XML of the same thing.

use crate::xls::{u16_at, u32_at};

/// A cell range of the file: first row, last row, first column, last column.
pub(crate) type Ref8 = [u16; 4];

fn ref8(d: &[u8], at: usize) -> Option<Ref8> {
    Some([u16_at(d, at)?, u16_at(d, at + 2)?, u16_at(d, at + 4)?, u16_at(d, at + 6)?])
}

/// The cells of a list of ranges as a `sqref`.
pub(crate) fn sqref(ranges: &[Ref8]) -> String {
    let name = |row: u16, col: u16| format!("{}{}", crate::xls_formula::column_letters(col as u32), row as u32 + 1);
    ranges.iter().map(|r| if r[0] == r[1] && r[2] == r[3] { name(r[0], r[2]) } else { format!("{}:{}", name(r[0], r[2]), name(r[1], r[3])) }).collect::<Vec<_>>().join(" ")
}

fn escape(s: &str) -> String {
    s.chars().filter(|&c| c == '\t' || c == '\n' || c == '\r' || c >= ' ').map(|c| match c {
        '&' => "&amp;".to_string(),
        '<' => "&lt;".to_string(),
        '>' => "&gt;".to_string(),
        '"' => "&quot;".to_string(),
        c => c.to_string(),
    }).collect()
}

// ---- differential formatting (DXFN)

/// A format a conditional format applies (`DXFN`): only what it sets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Dxf {
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub strike: Option<bool>,
    /// 0 none, 1 single, 2 double (`uls`).
    pub underline: Option<u8>,
    pub font_color: Option<u16>,
    /// The fill's pattern (`fls`) when it is set, and its foreground and background colours.
    pub pattern: Option<u8>,
    pub fore: Option<u16>,
    pub back: Option<u16>,
    /// left, right, top, bottom: (style, colour index).
    pub borders: [Option<(u8, u16)>; 4],
    /// The full colours of the extension (`CFEx`), over the palette's: a `color` element's attributes
    /// (`rgb="FF336699"`, `theme="4" tint="0.8"`).
    pub font_rgb: Option<String>,
    pub fore_rgb: Option<String>,
    pub back_rgb: Option<String>,
}

/// A `DXFN` at the start of `d`, and the bytes it takes. `None` for one
/// holding a number format or others this does not read.
fn read_dxfn(d: &[u8]) -> Option<Dxf> {
    let flags = u32_at(d, 0)?;
    let ninch = |bit: u32| flags >> bit & 1 != 0;
    let (has_num, has_font, has_alc, has_bdr, has_pat, has_prot) = (flags >> 25 & 1 != 0, flags >> 26 & 1 != 0, flags >> 27 & 1 != 0, flags >> 28 & 1 != 0, flags >> 29 & 1 != 0, flags >> 30 & 1 != 0);
    if has_num {
        return None;
    }
    let mut at = 6;
    let mut dxf = Dxf::default();
    if has_font {
        let f = d.get(at..at + 118)?;
        at += 118;
        let ts = u32_at(f, 68)?;
        let ts_ninch = u32_at(f, 88)?;
        let bls = u16_at(f, 72)?;
        let uls = *f.get(76)?;
        let icv = u32_at(f, 80)? as i32;
        if u32_at(f, 100)? == 0 {
            dxf.bold = Some(bls >= 700);
        }
        if ts_ninch & 2 == 0 {
            dxf.italic = Some(ts & 2 != 0);
        }
        if ts_ninch & 8 == 0 {
            dxf.strike = Some(ts & 8 != 0);
        }
        if u32_at(f, 96)? == 0 {
            dxf.underline = Some(uls);
        }
        if (0..64).contains(&icv) {
            dxf.font_color = Some(icv as u16);
        }
    }
    if has_alc {
        at += 8;
    }
    if has_bdr {
        let b = d.get(at..at + 8)?;
        at += 8;
        let (w0, w1) = (u32_at(b, 0)?, u32_at(b, 4)?);
        let styles = [(w0 & 15) as u8, (w0 >> 4 & 15) as u8, (w0 >> 8 & 15) as u8, (w0 >> 12 & 15) as u8];
        let colors = [(w0 >> 16 & 127) as u16, (w0 >> 23 & 127) as u16, (w1 & 127) as u16, (w1 >> 7 & 127) as u16];
        for k in 0..4 {
            if !ninch(10 + k as u32) && styles[k] != 0 {
                dxf.borders[k] = Some((styles[k], colors[k]));
            }
        }
    }
    if has_pat {
        let v = u32_at(d, at)?;
        let (fls, fg, bg) = ((v >> 10 & 63) as u8, (v >> 16 & 127) as u16, (v >> 23 & 127) as u16);
        if !ninch(16) {
            dxf.pattern = Some(fls);
        }
        if !ninch(17) {
            dxf.fore = Some(fg);
        }
        if !ninch(18) {
            dxf.back = Some(bg);
        }
    }
    let _ = has_prot;
    Some(dxf)
}

/// The `dxf` element of a format; `color` gives a colour index as `AARRGGBB`.
pub(crate) fn dxf_xml(dxf: &Dxf, color: &dyn Fn(u16) -> Option<String>) -> String {
    let mut x = String::from("<dxf>");
    let mut font = String::new();
    if let Some(b) = dxf.bold {
        font += if b { "<b/>" } else { "<b val=\"0\"/>" };
    }
    if let Some(i) = dxf.italic {
        font += if i { "<i/>" } else { "<i val=\"0\"/>" };
    }
    if let Some(s) = dxf.strike {
        font += if s { "<strike/>" } else { "<strike val=\"0\"/>" };
    }
    match dxf.underline {
        Some(1 | 0x21) => font += "<u/>",
        Some(2 | 0x22) => font += "<u val=\"double\"/>",
        _ => {}
    }
    if let Some(c) = dxf.font_rgb.clone().or_else(|| dxf.font_color.and_then(color).map(|c| format!("rgb=\"{c}\""))) {
        font += &format!("<color {c}/>");
    }
    if !font.is_empty() {
        x += &format!("<font>{font}</font>");
    }
    if dxf.pattern.is_some() || dxf.fore.is_some() || dxf.back.is_some() || dxf.fore_rgb.is_some() || dxf.back_rgb.is_some() {
        // A solid fill of a conditional format keeps its colour as the background's.
        const NAMES: [&str; 19] = ["none", "solid", "mediumGray", "darkGray", "lightGray", "darkHorizontal", "darkVertical", "darkDown", "darkUp", "darkGrid", "darkTrellis", "lightHorizontal", "lightVertical", "lightDown", "lightUp", "lightGrid", "lightTrellis", "gray125", "gray0625"];
        let name = dxf.pattern.filter(|&p| p > 1).and_then(|p| NAMES.get(p as usize));
        let fg = dxf.fore_rgb.clone().or_else(|| dxf.fore.and_then(color).map(|c| format!("rgb=\"{c}\""))).map(|c| format!("<fgColor {c}/>")).unwrap_or_default();
        let bg = dxf.back_rgb.clone().or_else(|| dxf.back.and_then(color).map(|c| format!("rgb=\"{c}\""))).map(|c| format!("<bgColor {c}/>")).unwrap_or_default();
        match name {
            Some(n) => x += &format!("<fill><patternFill patternType=\"{n}\">{fg}{bg}</patternFill></fill>"),
            None => x += &format!("<fill><patternFill>{bg}</patternFill></fill>"),
        }
    }
    if dxf.borders.iter().any(Option::is_some) {
        const STYLES: [&str; 14] = ["none", "thin", "medium", "dashed", "dotted", "thick", "double", "hair", "mediumDashed", "dashDot", "mediumDashDot", "dashDotDot", "mediumDashDotDot", "slantDashDot"];
        let side = |name: &str, b: Option<(u8, u16)>| match b {
            Some((s, c)) => format!("<{name} style=\"{}\">{}</{name}>", STYLES.get(s as usize).copied().unwrap_or("thin"), color(c).map(|c| format!("<color rgb=\"{c}\"/>")).unwrap_or_default()),
            None => String::new(),
        };
        x += &format!("<border>{}{}{}{}</border>", side("left", dxf.borders[0]), side("right", dxf.borders[1]), side("top", dxf.borders[2]), side("bottom", dxf.borders[3]));
    }
    x += "</dxf>";
    x
}

// ---- conditional formats

/// A rule (`CF`): `ct` 1 a value's comparison (`cp` its operator), 2 a formula.
#[derive(Clone, Debug)]
pub(crate) struct CfRule {
    pub ct: u8,
    pub cp: u8,
    pub f1: Vec<u8>,
    pub f2: Vec<u8>,
    pub dxf: Option<Dxf>,
    /// A data bar's colour (`RRGGBB`): the rule is a data bar (`CF12`).
    pub bar: Option<String>,
}

/// The rules of a range (`CFHEADER` and the `CF`s after it).
#[derive(Clone, Debug)]
pub(crate) struct CfGroup {
    pub bound: Ref8,
    pub ranges: Vec<Ref8>,
    pub rules: Vec<CfRule>,
}

pub(crate) fn read_cf_header(d: &[u8]) -> Option<CfGroup> {
    let bound = ref8(d, 4)?;
    let n = u16_at(d, 12)? as usize;
    let ranges = (0..n).map(|k| ref8(d, 14 + k * 8)).collect::<Option<Vec<_>>>()?;
    Some(CfGroup { bound, ranges, rules: Vec::new() })
}

pub(crate) fn read_cf(d: &[u8]) -> Option<CfRule> {
    let (ct, cp, cce1, cce2) = (*d.first()?, *d.get(1)?, u16_at(d, 2)? as usize, u16_at(d, 4)? as usize);
    let rest = d.get(6..)?;
    let dxf = read_dxfn(rest);
    // The formulas close the record.
    let f2 = d.get(d.len().checked_sub(cce2)?..)?.to_vec();
    let f1 = d.get(d.len().checked_sub(cce1 + cce2)?..d.len() - cce2)?.to_vec();
    Some(CfRule { ct, cp, f1, f2, dxf, bar: None })
}

/// A `CF12` after a `CondFmt12` (read as a group, from byte 12): only a data bar
/// whose limits are the lowest and the highest value is read.
pub(crate) fn read_cf12(d: &[u8]) -> Option<CfRule> {
    let ct = *d.get(12)?;
    if ct != 4 {
        return None;
    }
    // The data bar's structure closes the record: 4 bytes of flags, the two lengths, a 16-byte colour, two 3-byte limits.
    let at = d.len().checked_sub(28)?;
    let color_at = at + 6;
    if u32_at(d, color_at)? != 2 || *d.get(at + 22)? != 2 || *d.get(at + 25)? != 3 {
        return None;
    }
    let rgb = d.get(color_at + 4..color_at + 7)?;
    Some(CfRule { ct, cp: 0, f1: Vec::new(), f2: Vec::new(), dxf: None, bar: Some(format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])) })
}

/// The extension (`CFEx`) of a group's rule: the full colours of its format.
pub(crate) fn read_cf_ex(d: &[u8], groups: &mut [CfGroup]) {
    let Some(bound) = ref8(d, 4) else { return };
    if u32_at(d, 12) != Some(0) {
        return;
    }
    let Some(group) = groups.iter_mut().rev().find(|g| g.bound == bound && g.rules.len() == 1) else { return };
    let rule = &mut group.rules[0];
    let mut dxf = rule.dxf.take().unwrap_or_default();
    let mut i = 16;
    while i + 12 <= d.len() {
        // An ExtProp (MS-XLS 2.5.108) of a FullColorExt (2.5.155): its kind, size 20, the colour's type (1 an index,
        // 2 RGB, 3 a theme colour), its tint (-32767 to 32767) and its value.
        let (kind, size, color_type, tint) = (u16_at(d, i).unwrap_or(0), u16_at(d, i + 2).unwrap_or(0), u16_at(d, i + 4).unwrap_or(0), u16_at(d, i + 6).unwrap_or(1));
        if size == 20 && (1..=3).contains(&color_type) && [4, 5, 13].contains(&kind) {
            let tint = tint as i16;
            let tint_attr = if tint != 0 { format!(" tint=\"{}\"", ibis_calc::number_to_xml((tint as f64 / 32767.0 * 1e6).round() / 1e6)) } else { String::new() };
            let value = u32_at(d, i + 8).unwrap_or(0);
            let attr = match color_type {
                1 => format!("indexed=\"{value}\"{tint_attr}"),
                2 => format!("rgb=\"FF{:02X}{:02X}{:02X}\"{tint_attr}", d[i + 8], d[i + 9], d[i + 10]),
                _ => format!("theme=\"{value}\"{tint_attr}"),
            };
            match kind {
                4 => dxf.fore_rgb = Some(attr),
                5 => dxf.back_rgb = Some(attr),
                _ => dxf.font_rgb = Some(attr),
            }
            i += 20;
        } else {
            i += 1;
        }
    }
    rule.dxf = Some(dxf);
}

/// The `conditionalFormatting` elements of a sheet's groups; `dxf_ids` is
/// the `dxfs` index of each rule's format, `formula` makes a formula's text
/// (its tokens and the cell they are counted from).
pub(crate) fn cf_xml(groups: &[CfGroup], dxf_ids: &[Vec<Option<usize>>], formula: &dyn Fn(&[u8], (u32, u32)) -> Option<String>) -> String {
    let mut out = String::new();
    let mut priority = 1;
    for (g, ids) in groups.iter().zip(dxf_ids) {
        let mut rules = String::new();
        let base = (g.bound[0] as u32, g.bound[2] as u32);
        for (rule, id) in g.rules.iter().zip(ids) {
            let dxf = id.map(|i| format!(" dxfId=\"{i}\"")).unwrap_or_default();
            let f1 = formula(&rule.f1, base);
            let f2 = (!rule.f2.is_empty()).then(|| formula(&rule.f2, base)).flatten();
            if let Some(rgb) = &rule.bar {
                rules += &format!("<cfRule type=\"dataBar\" priority=\"{priority}\"><dataBar><cfvo type=\"min\"/><cfvo type=\"max\"/><color rgb=\"FF{rgb}\"/></dataBar></cfRule>");
                priority += 1;
                continue;
            }
            match rule.ct {
                1 => {
                    const OPS: [&str; 9] = ["", "between", "notBetween", "equal", "notEqual", "greaterThan", "lessThan", "greaterThanOrEqual", "lessThanOrEqual"];
                    let (Some(op), Some(f1)) = (OPS.get(rule.cp as usize).filter(|o| !o.is_empty()), f1) else { continue };
                    let two = f2.map(|f| format!("<formula>{}</formula>", escape(&ibis_calc::formula::to_file(&f)))).unwrap_or_default();
                    rules += &format!("<cfRule type=\"cellIs\"{dxf} priority=\"{priority}\" operator=\"{op}\"><formula>{}</formula>{two}</cfRule>", escape(&ibis_calc::formula::to_file(&f1)));
                }
                2 => {
                    let Some(f1) = f1 else { continue };
                    rules += &format!("<cfRule type=\"expression\"{dxf} priority=\"{priority}\"><formula>{}</formula></cfRule>", escape(&ibis_calc::formula::to_file(&f1)));
                }
                _ => continue,
            }
            priority += 1;
        }
        if !rules.is_empty() {
            out += &format!("<conditionalFormatting sqref=\"{}\">{rules}</conditionalFormatting>", sqref(&g.ranges));
        }
    }
    out
}

// ---- data validations

/// A rule of data validation (`DV`).
#[derive(Clone, Debug)]
pub(crate) struct Dv {
    pub flags: u32,
    pub prompt_title: String,
    pub error_title: String,
    pub prompt: String,
    pub error: String,
    pub f1: Vec<u8>,
    pub f2: Vec<u8>,
    pub ranges: Vec<Ref8>,
}

/// An `XLUnicodeString` with a 2-byte length: its text (a lone NUL is no text) and its size.
fn string(d: &[u8], at: usize) -> Option<(String, usize)> {
    let n = u16_at(d, at)? as usize;
    let wide = *d.get(at + 2)? & 1 != 0;
    let size = if wide { n * 2 } else { n };
    let body = d.get(at + 3..at + 3 + size)?;
    let text: String = if wide { String::from_utf16_lossy(&(0..n).filter_map(|k| u16_at(body, k * 2)).collect::<Vec<_>>()) } else { body.iter().map(|&b| b as char).collect() };
    Some((text.trim_end_matches('\0').to_string(), 3 + size))
}

pub(crate) fn read_dv(d: &[u8]) -> Option<Dv> {
    let flags = u32_at(d, 0)?;
    let mut at = 4;
    let mut texts = Vec::new();
    for _ in 0..4 {
        let (t, n) = string(d, at)?;
        texts.push(t);
        at += n;
    }
    let cce1 = u16_at(d, at)? as usize;
    let f1 = d.get(at + 4..at + 4 + cce1)?.to_vec();
    at += 4 + cce1;
    let cce2 = u16_at(d, at)? as usize;
    let f2 = d.get(at + 4..at + 4 + cce2)?.to_vec();
    at += 4 + cce2;
    let n = u16_at(d, at)? as usize;
    let ranges = (0..n).map(|k| ref8(d, at + 2 + k * 8)).collect::<Option<Vec<_>>>()?;
    let mut t = texts.into_iter();
    Some(Dv { flags, prompt_title: t.next()?, error_title: t.next()?, prompt: t.next()?, error: t.next()?, f1, f2, ranges })
}

/// The `dataValidations` element of a sheet's rules.
pub(crate) fn dv_xml(rules: &[Dv], formula: &dyn Fn(&[u8], (u32, u32)) -> Option<String>) -> String {
    let mut out = String::new();
    let mut count = 0;
    for r in rules {
        let kind = match r.flags & 15 {
            1 => "whole",
            2 => "decimal",
            3 => "list",
            4 => "date",
            5 => "time",
            6 => "textLength",
            7 => "custom",
            _ => "none",
        };
        if kind == "none" || r.ranges.is_empty() {
            continue;
        }
        let base = (r.ranges[0][0] as u32, r.ranges[0][2] as u32);
        // A list typed in is a string of the items between NULs; the sheet's text is the items between commas, in quotes.
        let text = |tokens: &[u8]| -> Option<String> {
            if kind == "list" && r.flags & 0x80 != 0 && tokens.first() == Some(&0x17) {
                let (s, _) = string_after(tokens)?;
                let items: Vec<&str> = s.split('\0').filter(|i| !i.is_empty()).collect();
                return Some(format!("\"{}\"", items.join(",").replace('"', "\"\"")));
            }
            formula(tokens, base).map(|f| ibis_calc::formula::to_file(&f))
        };
        let Some(f1) = text(&r.f1) else { continue };
        let f2 = (!r.f2.is_empty()).then(|| text(&r.f2)).flatten();
        let mut attrs = format!(" type=\"{kind}\"");
        match r.flags >> 4 & 7 {
            1 => attrs += " errorStyle=\"warning\"",
            2 => attrs += " errorStyle=\"information\"",
            _ => {}
        }
        const OPS: [&str; 8] = ["between", "notBetween", "equal", "notEqual", "greaterThan", "lessThan", "greaterThanOrEqual", "lessThanOrEqual"];
        let op = OPS[(r.flags >> 20 & 15).min(7) as usize];
        if kind != "list" && kind != "custom" && op != "between" {
            attrs += &format!(" operator=\"{op}\"");
        }
        if r.flags & 0x100 != 0 {
            attrs += " allowBlank=\"1\"";
        }
        if r.flags & 0x200 != 0 && kind == "list" {
            attrs += " showDropDown=\"1\"";
        }
        if r.flags & 0x40000 != 0 {
            attrs += " showInputMessage=\"1\"";
        }
        if r.flags & 0x80000 != 0 {
            attrs += " showErrorMessage=\"1\"";
        }
        for (name, v) in [("errorTitle", &r.error_title), ("error", &r.error), ("promptTitle", &r.prompt_title), ("prompt", &r.prompt)] {
            if !v.is_empty() {
                attrs += &format!(" {name}=\"{}\"", escape(v));
            }
        }
        let second = f2.map(|f| format!("<formula2>{}</formula2>", escape(&f))).unwrap_or_default();
        out += &format!("<dataValidation{attrs} sqref=\"{}\"><formula1>{}</formula1>{second}</dataValidation>", sqref(&r.ranges), escape(&f1));
        count += 1;
    }
    if count == 0 { String::new() } else { format!("<dataValidations count=\"{count}\">{out}</dataValidations>") }
}

/// The text of a `ptgStr` token (1 byte of the token, the length, a flag, the characters).
fn string_after(tokens: &[u8]) -> Option<(String, usize)> {
    let n = *tokens.get(1)? as usize;
    let wide = *tokens.get(2)? & 1 != 0;
    let body = tokens.get(3..3 + if wide { n * 2 } else { n })?;
    let text: String = if wide { String::from_utf16_lossy(&(0..n).filter_map(|k| u16_at(body, k * 2)).collect::<Vec<_>>()) } else { body.iter().map(|&b| b as char).collect() };
    Some((text, 3 + body.len()))
}

// ---- hyperlinks

/// A hyperlink (`HLINK`): a web address or a file (`target`), or a place in
/// the workbook (`location`), and the text shown.
#[derive(Clone, Debug, Default)]
pub(crate) struct Link {
    pub range: Ref8,
    pub target: Option<String>,
    pub location: Option<String>,
    pub display: Option<String>,
    #[allow(dead_code)]
    pub screen_tip: Option<String>,
}

/// A string of a hyperlink: a 4-byte count of the characters (with the NUL) and them.
fn link_string(d: &[u8], at: usize) -> Option<(String, usize)> {
    let n = u32_at(d, at)? as usize;
    let body = d.get(at + 4..at + 4 + n * 2)?;
    let text = String::from_utf16_lossy(&(0..n).filter_map(|k| u16_at(body, k * 2)).collect::<Vec<_>>());
    Some((text.trim_end_matches('\0').to_string(), 4 + n * 2))
}

pub(crate) fn read_hlink(d: &[u8]) -> Option<Link> {
    let range = ref8(d, 0)?;
    // The hyperlink object: its GUID (16), version (4), then its flags.
    let flags = u32_at(d, 28)?;
    let mut at = 32;
    let mut link = Link { range, ..Default::default() };
    if flags & 0x10 != 0 {
        let (s, n) = link_string(d, at)?;
        link.display = Some(s);
        at += n;
    }
    if flags & 0x80 != 0 {
        let (_, n) = link_string(d, at)?;
        at += n;
    }
    if flags & 1 != 0 {
        if flags & 0x100 != 0 {
            let (s, n) = link_string(d, at)?;
            link.target = Some(s);
            at += n;
        } else {
            let clsid = d.get(at..at + 16)?;
            at += 16;
            const URL_MONIKER: [u8; 16] = [0xE0, 0xC9, 0xEA, 0x79, 0xF9, 0xBA, 0xCE, 0x11, 0x8C, 0x82, 0x00, 0xAA, 0x00, 0x4B, 0xA9, 0x0B];
            const FILE_MONIKER: [u8; 16] = [0x03, 0x03, 0, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46];
            if clsid == URL_MONIKER {
                let length = u32_at(d, at)? as usize;
                let body = d.get(at + 4..at + 4 + length)?;
                // The address, then (when the length says) a GUID, a version and flags.
                let units: Vec<u16> = (0..body.len() / 2).filter_map(|k| u16_at(body, k * 2)).collect();
                let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
                link.target = Some(String::from_utf16_lossy(&units[..end]));
                at += 4 + length;
            } else if clsid == FILE_MONIKER {
                // cAnti (2), the ANSI path (4 + bytes with NUL), 24 bytes, then a long path.
                let path_len = u32_at(d, at + 2)? as usize;
                let path = d.get(at + 6..at + 6 + path_len)?;
                link.target = Some(path.iter().take_while(|&&b| b != 0).map(|&b| b as char).collect());
                let up = u16_at(d, at)? as usize;
                if up > 0
                    && let Some(t) = link.target.as_mut()
                {
                    *t = format!("{}{}", "../".repeat(up), t);
                }
                at += 6 + path_len;
                // 24 reserved bytes, the size of the extra (4), then it.
                at += 24;
                let extra = u32_at(d, at)? as usize;
                if extra > 0
                    && let Some(body) = d.get(at + 8..at + 4 + extra)
                {
                    // 2 bytes of the length of the characters, 2 bytes: then UTF-16 of the path.
                    let units: Vec<u16> = (0..body.len() / 2).filter_map(|k| u16_at(body, k * 2)).collect();
                    let text = String::from_utf16_lossy(&units);
                    if !text.trim_end_matches('\0').is_empty() {
                        link.target = Some(text.trim_end_matches('\0').to_string());
                    }
                }
                at += 4 + extra;
            } else {
                return None;
            }
        }
    }
    if flags & 8 != 0
        && let Some((s, _)) = link_string(d, at)
    {
        link.location = Some(s);
    }
    Some(link)
}

/// The `hyperlinks` element of a sheet's links; `rid` makes a relationship's id for an address (and records it).
pub(crate) fn hyperlinks_xml(links: &[Link], rid: &mut dyn FnMut(&str) -> String) -> String {
    let mut out = String::new();
    for l in links {
        let mut attrs = format!(" ref=\"{}\"", sqref(&[l.range]));
        match (&l.target, &l.location) {
            (Some(t), loc) => {
                attrs += &format!(" r:id=\"{}\"", rid(t));
                if let Some(loc) = loc {
                    attrs += &format!(" location=\"{}\"", escape(loc));
                }
            }
            (None, Some(loc)) => attrs += &format!(" location=\"{}\"", escape(loc)),
            (None, None) => continue,
        }
        if let Some(d) = l.display.as_ref().filter(|d| !d.is_empty()) {
            attrs += &format!(" display=\"{}\"", escape(d));
        }
        out += &format!("<hyperlink{attrs}/>");
    }
    if out.is_empty() { out } else { format!("<hyperlinks>{out}</hyperlinks>") }
}

// ---- notes

/// A note (`NOTE`) with the text of its text box (`TXO`).
#[derive(Clone, Debug, Default)]
pub(crate) struct Note {
    pub row: u16,
    pub column: u16,
    pub author: String,
    pub object: u16,
}

/// The state of reading the notes of a sheet: the id of the last comment object,
/// the text boxes being read (their characters still to come), and the texts.
#[derive(Debug, Default)]
pub(crate) struct NoteReader {
    last_comment: Option<u16>,
    reading: Option<(u16, usize, String)>,
    pub texts: std::collections::HashMap<u16, String>,
}

impl NoteReader {
    /// An `OBJ`: its common data (`ftCmo`) says what it is and its id.
    pub(crate) fn object(&mut self, d: &[u8]) {
        if u16_at(d, 0) == Some(0x15) && u16_at(d, 4) == Some(0x19) {
            self.last_comment = u16_at(d, 6);
        } else {
            self.last_comment = None;
        }
    }

    /// A `TXO`: the number of characters its text has.
    pub(crate) fn text_box(&mut self, d: &[u8]) {
        self.reading = match (self.last_comment.take(), u16_at(d, 10)) {
            (Some(id), Some(n)) if n > 0 => Some((id, n as usize, String::new())),
            _ => None,
        };
    }

    /// A `CONTINUE`: the characters of the text (after a flag byte) while some are to come.
    pub(crate) fn continued(&mut self, d: &[u8]) {
        let Some((id, want, text)) = self.reading.as_mut() else { return };
        let have = text.chars().count();
        if have >= *want {
            return;
        }
        let Some(&flag) = d.first() else { return };
        let body = &d[1..];
        let chunk: String = if flag & 1 != 0 { String::from_utf16_lossy(&(0..body.len() / 2).filter_map(|k| u16_at(body, k * 2)).collect::<Vec<_>>()) } else { body.iter().map(|&b| b as char).collect() };
        text.push_str(&chunk);
        if text.chars().count() >= *want {
            let id = *id;
            let full: String = text.chars().take(*want).collect();
            self.texts.insert(id, full);
            self.reading = None;
        }
    }
}

pub(crate) fn read_note(d: &[u8]) -> Option<Note> {
    let (row, column, object) = (u16_at(d, 0)?, u16_at(d, 2)?, u16_at(d, 6)?);
    let author = string(d, 8).map(|(a, _)| a).unwrap_or_default();
    Some(Note { row, column, author, object })
}

/// The comments part of a sheet's notes (with their text).
pub(crate) fn comments_xml(notes: &[Note], texts: &std::collections::HashMap<u16, String>) -> String {
    let mut authors: Vec<&str> = Vec::new();
    let mut list = String::new();
    for n in notes {
        let Some(text) = texts.get(&n.object) else { continue };
        let at = authors.iter().position(|a| *a == n.author).unwrap_or_else(|| {
            authors.push(&n.author);
            authors.len() - 1
        });
        list += &format!("<comment ref=\"{}\" authorId=\"{at}\"><text><r><t xml:space=\"preserve\">{}</t></r></text></comment>", sqref(&[[n.row, n.row, n.column, n.column]]), escape(text));
    }
    if list.is_empty() {
        return String::new();
    }
    let authors: String = authors.iter().map(|a| format!("<author>{}</author>", escape(a))).collect();
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><comments xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><authors>{authors}</authors><commentList>{list}</commentList></comments>")
}

/// The legacy drawing (VML) Excel keeps the notes' boxes in: one hidden box
/// beside each note's cell, which Excel wants beside the comments part.
pub(crate) fn notes_vml(notes: &[Note], texts: &std::collections::HashMap<u16, String>, sheet_number: usize) -> String {
    let mut shapes = String::new();
    let mut n = 0;
    for note in notes.iter().filter(|n| texts.contains_key(&n.object)) {
        n += 1;
        let (row, col) = (note.row, note.column);
        shapes += &format!(
            "<v:shape id=\"_x0000_s{}\" type=\"#_x0000_t202\" style=\"position:absolute;margin-left:80pt;margin-top:2pt;width:108pt;height:60pt;z-index:{n};visibility:hidden\" fillcolor=\"#ffffe1\" o:insetmode=\"auto\"><v:fill color2=\"#ffffe1\"/><v:shadow on=\"t\" color=\"black\" obscured=\"t\"/><v:path o:connecttype=\"none\"/><v:textbox style=\"mso-direction-alt:auto\"><div style=\"text-align:left\"></div></v:textbox><x:ClientData ObjectType=\"Note\"><x:MoveWithCells/><x:SizeWithCells/><x:Anchor>{}, 15, {}, 2, {}, 15, {}, 16</x:Anchor><x:AutoFill>False</x:AutoFill><x:Row>{row}</x:Row><x:Column>{col}</x:Column></x:ClientData></v:shape>",
            1024 * sheet_number + n,
            col as u32 + 1,
            row.saturating_sub(1),
            col as u32 + 3,
            row as u32 + 3,
        );
    }
    format!("<xml xmlns:v=\"urn:schemas-microsoft-com:vml\" xmlns:o=\"urn:schemas-microsoft-com:office:office\" xmlns:x=\"urn:schemas-microsoft-com:office:excel\"><o:shapelayout v:ext=\"edit\"><o:idmap v:ext=\"edit\" data=\"{sheet_number}\"/></o:shapelayout><v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\" path=\"m,l,21600r21600,l21600,xe\"><v:stroke joinstyle=\"miter\"/><v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/></v:shapetype>{shapes}</xml>")
}

// ---- AutoFilter

/// A condition of an `AUTOFILTER` record (`DOper`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Condition {
    Number(f64),
    Text(String),
    Boolean(bool),
    Blanks,
    NonBlanks,
}

/// One column's filter (`AUTOFILTER`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ColumnFilter {
    /// The column, counted from the first of the filter's range.
    pub column: u16,
    /// Both conditions must hold (else either).
    pub and: bool,
    /// The conditions with their comparison (`grbitSign`: 1 <, 2 =, 3 <=, 4 >, 5 <>, 6 >=).
    pub conditions: Vec<(u8, Condition)>,
    /// A top (bottom) ten: the count, from the top, as a percent.
    pub top10: Option<(u16, bool, bool)>,
}

/// An `AUTOFILTER` record: a column's conditions. `None` for one with none (the column shows all).
pub(crate) fn read_autofilter(d: &[u8]) -> Option<ColumnFilter> {
    let column = u16_at(d, 0)?;
    let flags = u16_at(d, 2)?;
    let and = flags & 1 == 0;
    let (simple1, simple2, top10, top, percent) = (flags & 2 != 0, flags & 4 != 0, flags & 8 != 0, flags & 0x10 != 0, flags & 0x20 != 0);
    let _ = (simple1, simple2);
    if top10 {
        return Some(ColumnFilter { column, and, conditions: Vec::new(), top10: Some((flags >> 6, top, percent)) });
    }
    // The strings of the conditions follow both: each a flag byte and its characters.
    let mut text_at = 4 + 20;
    let mut conditions = Vec::new();
    for k in 0..2 {
        let at = 4 + k * 10;
        let (vt, sign) = (*d.get(at)?, *d.get(at + 1)?);
        let condition = match vt {
            0 => continue,
            2 => Condition::Number(crate::xls::rk_value(u32_at(d, at + 2)?)),
            4 => Condition::Number(f64::from_le_bytes(d.get(at + 2..at + 10)?.try_into().ok()?)),
            6 => {
                let n = *d.get(at + 6)? as usize;
                let wide = *d.get(text_at)? & 1 != 0;
                let size = if wide { n * 2 } else { n };
                let body = d.get(text_at + 1..text_at + 1 + size)?;
                text_at += 1 + size;
                Condition::Text(if wide { String::from_utf16_lossy(&(0..n).filter_map(|j| u16_at(body, j * 2)).collect::<Vec<_>>()) } else { body.iter().map(|&b| b as char).collect() })
            }
            8 if *d.get(at + 3)? == 0 => Condition::Boolean(*d.get(at + 2)? != 0),
            0x0C => Condition::Blanks,
            0x0E => Condition::NonBlanks,
            _ => return None,
        };
        conditions.push((sign, condition));
    }
    (!conditions.is_empty()).then_some(ColumnFilter { column, and, conditions, top10: None })
}

fn filter_value(c: &Condition) -> String {
    match c {
        Condition::Number(n) => ibis_calc::number_to_xml(*n),
        Condition::Text(t) => escape(t),
        Condition::Boolean(b) => if *b { "1".into() } else { "0".into() },
        Condition::Blanks | Condition::NonBlanks => String::new(),
    }
}

/// The `autoFilter` element over `range` (`A1:C8`) with the columns' conditions.
pub(crate) fn autofilter_xml(range: &str, filters: &[ColumnFilter]) -> String {
    let mut columns = String::new();
    for f in filters {
        let inner = if let Some((count, top, percent)) = f.top10 {
            format!("<top10{}{} val=\"{count}\"/>", if top { "" } else { " top=\"0\"" }, if percent { " percent=\"1\"" } else { "" })
        } else if f.conditions.len() == 1 && f.conditions[0].0 == 2 && !matches!(f.conditions[0].1, Condition::Blanks | Condition::NonBlanks) {
            format!("<filters><filter val=\"{}\"/></filters>", filter_value(&f.conditions[0].1))
        } else if f.conditions.len() == 2 && !f.and && f.conditions.iter().all(|c| c.0 == 2 && !matches!(c.1, Condition::Blanks | Condition::NonBlanks)) {
            format!("<filters>{}</filters>", f.conditions.iter().map(|c| format!("<filter val=\"{}\"/>", filter_value(&c.1))).collect::<String>())
        } else if f.conditions.len() == 1 && f.conditions[0].1 == Condition::Blanks {
            "<filters blank=\"1\"/>".to_string()
        } else {
            let custom: String = f
                .conditions
                .iter()
                .map(|(sign, c)| {
                    let (operator, value) = match c {
                        Condition::NonBlanks => (" operator=\"notEqual\"", " val=\" \"".to_string()),
                        Condition::Blanks => ("", " val=\"\"".to_string()),
                        _ => (
                            match sign {
                                1 => " operator=\"lessThan\"",
                                3 => " operator=\"lessThanOrEqual\"",
                                4 => " operator=\"greaterThan\"",
                                5 => " operator=\"notEqual\"",
                                6 => " operator=\"greaterThanOrEqual\"",
                                _ => "",
                            },
                            format!(" val=\"{}\"", filter_value(c)),
                        ),
                    };
                    format!("<customFilter{operator}{value}/>")
                })
                .collect();
            format!("<customFilters{}>{custom}</customFilters>", if f.and && f.conditions.len() > 1 { " and=\"1\"" } else { "" })
        };
        columns.push_str(&format!("<filterColumn colId=\"{}\">{inner}</filterColumn>", f.column));
    }
    format!("<autoFilter ref=\"{range}\">{columns}</autoFilter>")
}

#[cfg(test)]
mod cf_tests {
    use super::*;

    #[test]
    fn a_conditional_formats_theme_colour_with_a_tint_is_kept() {
        // Corpus 103: the banding of MOD(ROW(),2) is accent 1 lightened by 0.8 (theme 4, tint 0.79998); the CF's palette
        // colour is only the nearest (light turquoise).
        let mut groups = vec![CfGroup { bound: [4, 27, 1, 4], ranges: vec![[4, 27, 1, 4]], rules: vec![CfRule { ct: 2, cp: 0, f1: Vec::new(), f2: Vec::new(), dxf: Some(Dxf { pattern: Some(1), back: Some(0x29), ..Default::default() }), bar: None }] }];
        // CFEx: FrtRefHeaderNoGrbit (12, the bound at 4), fIsCF12 = 0, then an ExtProp of the background: kind 5,
        // 20 bytes, a theme colour (3), tint 26214 (0.8), theme 4.
        let mut d = vec![0x7B, 0x08, 0, 0, 4, 0, 27, 0, 1, 0, 4, 0, 0, 0, 0, 0];
        d.extend([5, 0, 20, 0, 3, 0]);
        d.extend(26214i16.to_le_bytes());
        d.extend(4u32.to_le_bytes());
        d.extend([0; 8]);
        read_cf_ex(&d, &mut groups);
        let xml = dxf_xml(groups[0].rules[0].dxf.as_ref().unwrap(), &|_| Some("FFCCFFFF".into()));
        assert!(xml.contains("<bgColor theme=\"4\" tint=\"0.800012\"/>"), "{xml}");
    }
}
