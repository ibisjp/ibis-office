// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Word 97-2003 documents (`.doc`, [MS-DOC]) read into a WordprocessingML package, so that the `.docx` reader lays them
//! out: the text from the piece table (2.4.1), the fonts (`SttbfFfn`, 2.9.272), the styles (`STSH`, 2.9.271), the
//! character and paragraph formatting of each run and paragraph (the `CHPX` and `PAPX` pages, 2.9.43 and 2.9.175), the
//! sections (`PlcfSed`, 2.8.26) and the tables (the rows' `TAP`, 2.6.4). Each property (`Sprm`, 2.2.5) is written as
//! the WordprocessingML element of the same meaning ([MS-DOC] 2.6 names them; ECMA-376 Part 1, 17.3 and 17.6).
//!
//! Lists, headers and footers, notes and pictures are not read yet.

use ibis_ooxml::cfb;
use ibis_ooxml::zip::{EntryMeta, Writer};

use crate::Error;

fn u8_at(b: &[u8], o: usize) -> Option<u8> {
    b.get(o).copied()
}
fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|x| u16::from_le_bytes([x[0], x[1]]))
}
fn i16_at(b: &[u8], o: usize) -> Option<i16> {
    u16_at(b, o).map(|v| v as i16)
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

/// A property and its operand ([MS-DOC] 2.2.5.1: the operand's size is given by the `spra` bits).
struct Sprm<'a> {
    op: u16,
    arg: &'a [u8],
}

impl Sprm<'_> {
    fn u8(&self) -> u8 {
        self.arg.first().copied().unwrap_or(0)
    }
    fn u16(&self) -> u16 {
        u16_at(self.arg, 0).unwrap_or(0)
    }
    fn i16(&self) -> i16 {
        i16_at(self.arg, 0).unwrap_or(0)
    }
}

fn sprms(grpprl: &[u8]) -> Vec<Sprm<'_>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 2 <= grpprl.len() {
        let op = u16_at(grpprl, at).unwrap_or(0);
        at += 2;
        let (start, len) = match op >> 13 {
            0 | 1 => (at, 1),
            2 | 4 | 5 => (at, 2),
            3 => (at, 4),
            7 => (at, 3),
            _ => {
                // sprmTDefTable's size is 16-bit and counts itself less one; sprmPChgTabs' 255 is followed by its own
                // counts (2.6.2); the others give their size in the first byte.
                if op == 0xD608 || op == 0xD606 {
                    let Some(cb) = u16_at(grpprl, at) else { break };
                    (at + 2, (cb as usize).saturating_sub(1))
                } else {
                    let Some(cb) = u8_at(grpprl, at) else { break };
                    (at + 1, cb as usize)
                }
            }
        };
        let Some(arg) = grpprl.get(start..start + len) else { break };
        out.push(Sprm { op, arg });
        at = start + len;
    }
    out
}

/// Properties as WordprocessingML elements, in the schema's order (`order`), a later one of the same element replacing
/// an earlier.
#[derive(Clone, Default)]
struct Props {
    items: Vec<(u16, &'static str, String)>,
}

impl Props {
    fn set(&mut self, order: u16, name: &'static str, xml: String) {
        self.items.retain(|(_, n, _)| *n != name);
        self.items.push((order, name, xml));
    }
    fn get(&self, name: &str) -> Option<&str> {
        self.items.iter().find(|(_, n, _)| *n == name).map(|(_, _, x)| x.as_str())
    }
    fn xml(&self) -> String {
        let mut items: Vec<&(u16, &str, String)> = self.items.iter().collect();
        items.sort_by_key(|(o, _, _)| *o);
        items.iter().map(|(_, _, x)| x.as_str()).collect()
    }
}

/// The run fonts (`w:rFonts`), kept apart since four properties make the one element.
#[derive(Clone, Default)]
struct Fonts {
    ascii: Option<String>,
    east_asia: Option<String>,
    h_ansi: Option<String>,
    cs: Option<String>,
}

impl Fonts {
    fn xml(&self) -> Option<String> {
        let mut attrs = String::new();
        for (name, value) in [("ascii", &self.ascii), ("eastAsia", &self.east_asia), ("hAnsi", &self.h_ansi), ("cs", &self.cs)] {
            if let Some(v) = value {
                attrs.push_str(&format!(r#" w:{name}="{}""#, esc(v)));
            }
        }
        (!attrs.is_empty()).then(|| format!("<w:rFonts{attrs}/>"))
    }
}

/// A run's properties (`w:rPr`).
#[derive(Clone, Default)]
struct RunProps {
    props: Props,
    fonts: Fonts,
    lang: (Option<String>, Option<String>),
    /// sprmCFSpec: the run's characters are special (a picture, a note's reference, a field's marks).
    special: bool,
    /// sprmCSymbol: a character of a symbol font, and the font.
    symbol: Option<(String, u16)>,
    /// sprmCPicLocation: where the run's picture is in the Data stream.
    pic_location: Option<u32>,
}

impl RunProps {
    fn xml(&self) -> String {
        let mut p = self.props.clone();
        if let Some(f) = self.fonts.xml() {
            p.set(1, "rFonts", f);
        }
        let mut lang = String::new();
        if let Some(l) = &self.lang.0 {
            lang.push_str(&format!(r#" w:val="{l}""#));
        }
        if let Some(l) = &self.lang.1 {
            lang.push_str(&format!(r#" w:eastAsia="{l}""#));
        }
        if !lang.is_empty() {
            p.set(37, "lang", format!("<w:lang{lang}/>"));
        }
        p.xml()
    }
}

/// The colours of `ico` ([MS-DOC] 2.9.119).
const ICO: [&str; 17] = [
    "auto", "000000", "0000FF", "00FFFF", "00FF00", "FF00FF", "FF0000", "FFFF00", "FFFFFF", "000080", "008080", "008000", "800080",
    "800000", "808000", "808080", "C0C0C0",
];
/// The highlight colours of `ico` (ECMA-376 17.18.40).
const HIGHLIGHT: [&str; 17] = [
    "none", "black", "blue", "cyan", "green", "magenta", "red", "yellow", "white", "darkBlue", "darkCyan", "darkGreen", "darkMagenta",
    "darkRed", "darkYellow", "darkGray", "lightGray",
];

fn lcid_tag(lid: u16) -> Option<&'static str> {
    Some(match lid {
        0x0411 => "ja-JP",
        0x0409 => "en-US",
        0x0809 => "en-GB",
        0x0804 => "zh-CN",
        0x0404 => "zh-TW",
        0x0412 => "ko-KR",
        0x0407 => "de-DE",
        0x040C => "fr-FR",
        0x0C0A => "es-ES",
        0x0410 => "it-IT",
        0x0401 => "ar-SA",
        0x040D => "he-IL",
        0x0419 => "ru-RU",
        _ => return None,
    })
}

fn underline(kul: u8) -> &'static str {
    match kul {
        0 => "none",
        1 => "single",
        2 => "words",
        3 => "double",
        4 => "dotted",
        6 => "thick",
        7 => "dash",
        9 => "dotDash",
        10 => "dotDotDash",
        11 => "wave",
        20 => "dottedHeavy",
        23 => "dashedHeavy",
        25 => "dashDotHeavy",
        26 => "dashDotDotHeavy",
        27 => "wavyHeavy",
        39 => "dashLong",
        43 => "wavyDouble",
        55 => "dashLongHeavy",
        _ => "single",
    }
}

/// The schema order of the `w:rPr` elements (ECMA-376 17.3.2.28).
const R_ORDER: &[&str] = &[
    "rStyle", "rFonts", "b", "bCs", "i", "iCs", "caps", "smallCaps", "strike", "dstrike", "outline", "shadow", "emboss", "imprint",
    "noProof", "snapToGrid", "vanish", "webHidden", "color", "spacing", "w", "kern", "position", "sz", "szCs", "highlight", "u",
    "effect", "bdr", "shd", "fitText", "vertAlign", "rtl", "cs", "em", "lang", "eastAsianLayout",
];

fn r_order(name: &str) -> u16 {
    R_ORDER.iter().position(|n| *n == name).unwrap_or(R_ORDER.len()) as u16
}

/// The toggle properties ([MS-DOC] 2.9.341 ToggleOperand): 0 and 1 set them, 0x80 keeps the style's, 0x81 is the
/// opposite of the style's.
const TOGGLES: &[(u16, &str)] = &[
    (0x0835, "b"),
    (0x085C, "bCs"),
    (0x0836, "i"),
    (0x085D, "iCs"),
    (0x0837, "strike"),
    (0x0838, "outline"),
    (0x0839, "shadow"),
    (0x083A, "smallCaps"),
    (0x083B, "caps"),
    (0x083C, "vanish"),
    (0x0858, "emboss"),
    (0x0854, "imprint"),
];

/// Applies character properties to `rp`; `style_on` tells whether the style has a toggle property on.
fn apply_chp(rp: &mut RunProps, grpprl: &[u8], fonts: &[String], styles: &Styles, style_on: &dyn Fn(&str) -> bool) {
    for s in sprms(grpprl) {
        if let Some((_, name)) = TOGGLES.iter().find(|(op, _)| *op == s.op) {
            let on = match s.u8() {
                0 => false,
                1 => true,
                0x80 => style_on(name),
                0x81 => !style_on(name),
                _ => continue,
            };
            rp.props.set(r_order(name), name, if on { format!("<w:{name}/>") } else { format!(r#"<w:{name} w:val="0"/>"#) });
            continue;
        }
        let font = |i: u16| fonts.get(i as usize).cloned();
        match s.op {
            0x4A30 => {
                if let Some(id) = styles.id(s.u16()) {
                    rp.props.set(r_order("rStyle"), "rStyle", format!(r#"<w:rStyle w:val="{id}"/>"#));
                }
            }
            0x4A4F => rp.fonts.ascii = font(s.u16()),
            0x4A50 => rp.fonts.east_asia = font(s.u16()),
            0x4A51 => rp.fonts.h_ansi = font(s.u16()),
            0x4A5E => rp.fonts.cs = font(s.u16()),
            0x2A53 => rp.props.set(r_order("dstrike"), "dstrike", if s.u8() == 1 { "<w:dstrike/>".into() } else { r#"<w:dstrike w:val="0"/>"#.into() }),
            0x4A43 => rp.props.set(r_order("sz"), "sz", format!(r#"<w:sz w:val="{}"/>"#, s.u16())),
            0x4A61 => rp.props.set(r_order("szCs"), "szCs", format!(r#"<w:szCs w:val="{}"/>"#, s.u16())),
            0x2A42 => {
                if let Some(c) = ICO.get(s.u8() as usize) {
                    rp.props.set(r_order("color"), "color", format!(r#"<w:color w:val="{c}"/>"#));
                }
            }
            0x6870 => {
                let a = s.arg;
                if a.len() >= 4 {
                    let color = if a[3] == 0xFF { "auto".to_string() } else { format!("{:02X}{:02X}{:02X}", a[0], a[1], a[2]) };
                    rp.props.set(r_order("color"), "color", format!(r#"<w:color w:val="{color}"/>"#));
                }
            }
            0x2A3E => rp.props.set(r_order("u"), "u", format!(r#"<w:u w:val="{}"/>"#, underline(s.u8()))),
            0x8840 => rp.props.set(r_order("spacing"), "spacing", format!(r#"<w:spacing w:val="{}"/>"#, s.i16())),
            0x4852 => rp.props.set(r_order("w"), "w", format!(r#"<w:w w:val="{}"/>"#, s.u16())),
            0x484B => rp.props.set(r_order("kern"), "kern", format!(r#"<w:kern w:val="{}"/>"#, s.u16())),
            0x4845 => rp.props.set(r_order("position"), "position", format!(r#"<w:position w:val="{}"/>"#, s.i16())),
            0x2A0C => {
                if let Some(h) = HIGHLIGHT.get(s.u8() as usize) {
                    rp.props.set(r_order("highlight"), "highlight", format!(r#"<w:highlight w:val="{h}"/>"#));
                }
            }
            0x2A48 => {
                let v = match s.u8() {
                    1 => "superscript",
                    2 => "subscript",
                    _ => "baseline",
                };
                rp.props.set(r_order("vertAlign"), "vertAlign", format!(r#"<w:vertAlign w:val="{v}"/>"#));
            }
            0x2A34 => {
                let v = match s.u8() {
                    1 => "dot",
                    2 => "comma",
                    3 => "circle",
                    4 => "underDot",
                    _ => "none",
                };
                rp.props.set(r_order("em"), "em", format!(r#"<w:em w:val="{v}"/>"#));
            }
            0x486D | 0x4873 => rp.lang.0 = lcid_tag(s.u16()).map(str::to_string),
            0x486E | 0x4874 => rp.lang.1 = lcid_tag(s.u16()).map(str::to_string),
            0x0855 => rp.special = s.u8() != 0,
            0x6A03 => rp.pic_location = u32_at(s.arg, 0),
            // sprmCFitText: the width to fit the text to and the id that ties the runs of one fitting together.
            0xCA76 => {
                if let (Some(w), Some(id)) = (u32_at(s.arg, 0), u32_at(s.arg, 4)) {
                    rp.props.set(r_order("fitText"), "fitText", format!(r#"<w:fitText w:val="{}" w:id="{}"/>"#, w as i32, id as i32));
                }
            }
            // sprmCFELayout ([MS-DOC] FarEastLayoutOperand: UFEL, then the id that ties the runs together): UFEL's fTNY (bit 0,
            // 縦中横), fWarichu (bit 1, 二行割り), iWarichuBracket (bits 8-10: none, round, square, angle, curly) and fTNYCompress
            // (bit 12), as ECMA-376's eastAsianLayout names them.
            0xCA78 => {
                if let (Some(ufel), Some(id)) = (u16_at(s.arg, 0), u32_at(s.arg, 2)) {
                    let mut a = format!(r#" w:id="{}""#, id as i32);
                    if ufel & 1 != 0 {
                        a.push_str(r#" w:vert="1""#);
                        if ufel & 0x1000 != 0 {
                            a.push_str(r#" w:vertCompress="1""#);
                        }
                    }
                    if ufel & 2 != 0 {
                        a.push_str(r#" w:combine="1""#);
                        let bracket = match (ufel >> 8) & 7 {
                            1 => Some("round"),
                            2 => Some("square"),
                            3 => Some("angle"),
                            4 => Some("curly"),
                            _ => None,
                        };
                        if let Some(b) = bracket {
                            a.push_str(&format!(r#" w:combineBrackets="{b}""#));
                        }
                    }
                    if ufel & 3 != 0 {
                        rp.props.set(r_order("eastAsianLayout"), "eastAsianLayout", format!("<w:eastAsianLayout{a}/>"));
                    }
                }
            }
            // sprmCFUsePgsuSettings: whether the run keeps to the document grid.
            0x0868 => rp.props.set(r_order("snapToGrid"), "snapToGrid", on_off("snapToGrid", s.u8())),
            // sprmCSymbol: the run's characters are this font's character.
            0x6A09 => {
                if let (Some(ftc), Some(ch)) = (u16_at(s.arg, 0), u16_at(s.arg, 2)) {
                    rp.symbol = Some((fonts.get(ftc as usize).cloned().unwrap_or_default(), ch));
                }
            }
            0x0875 => rp.props.set(r_order("noProof"), "noProof", if s.u8() != 0 { "<w:noProof/>".into() } else { r#"<w:noProof w:val="0"/>"#.into() }),
            0x0811 => rp.props.set(r_order("webHidden"), "webHidden", "<w:webHidden/>".into()),
            0x085A => rp.props.set(r_order("rtl"), "rtl", if s.u8() != 0 { "<w:rtl/>".into() } else { r#"<w:rtl w:val="0"/>"#.into() }),
            0x0882 => rp.props.set(r_order("cs"), "cs", "<w:cs/>".into()),
            _ => {}
        }
    }
}

/// A paragraph's properties (`w:pPr`) and its table marks.
#[derive(Clone, Default)]
struct ParaProps {
    props: Props,
    ind: [Option<i32>; 6], // left, right, firstLine (negative: hanging), leftChars, rightChars, firstLineChars
    spacing: [Option<String>; 7], // before, after, line, lineRule, beforeLines, afterLines, beforeAutospacing/afterAutospacing
    in_table: bool,
    ttp: bool,
    itap: u32,
    /// sprmPFInnerTableCell and sprmPFInnerTtp: the end of a cell, and of a row, of a table in a table's cell (`itap` 2 or more),
    /// whose marks are paragraph marks.
    inner_cell: bool,
    inner_ttp: bool,
    /// The frame (sprmPPc, sprmPDxaAbs, sprmPDyaAbs, sprmPDxaWidth, sprmPWHeightAbs, sprmPWr, sprmPDxaFromText,
    /// sprmPDyaFromText): its anchors, place, size, wrapping and distance from the text.
    frame: Frame,
    /// The paragraph's borders: top, left, bottom, right, between.
    borders: [Option<String>; 5],
    /// sprmPIlvl and sprmPIlfo: the list level, and the list (1-based into the overrides; 0 for none).
    ilvl: Option<u8>,
    ilfo: Option<u16>,
}

const P_ORDER: &[&str] = &[
    "pStyle", "keepNext", "keepLines", "pageBreakBefore", "framePr", "widowControl", "numPr", "suppressLineNumbers", "pBdr", "shd",
    "tabs", "suppressAutoHyphens", "kinsoku", "wordWrap", "overflowPunct", "topLinePunct", "autoSpaceDE", "autoSpaceDN", "bidi",
    "adjustRightInd", "snapToGrid", "spacing", "ind", "contextualSpacing", "mirrorIndents", "suppressOverlap", "jc",
    "textDirection", "textAlignment", "textboxTightWrap", "outlineLvl", "divId", "cnfStyle", "rPr",
];

fn p_order(name: &str) -> u16 {
    P_ORDER.iter().position(|n| *n == name).unwrap_or(P_ORDER.len()) as u16
}

fn on_off(name: &'static str, v: u8) -> String {
    if v != 0 { format!("<w:{name}/>") } else { format!(r#"<w:{name} w:val="0"/>"#) }
}

fn jc(v: u8) -> &'static str {
    match v {
        1 => "center",
        2 => "right",
        3 => "both",
        4 => "distribute",
        5 => "mediumKashida",
        7 => "highKashida",
        8 => "lowKashida",
        9 => "thaiDistribute",
        _ => "left",
    }
}

fn apply_pap(pp: &mut ParaProps, grpprl: &[u8], styles: &Styles) {
    for s in sprms(grpprl) {
        match s.op {
            0x4600 => {
                if let Some(id) = styles.id(s.u16()) {
                    pp.props.set(p_order("pStyle"), "pStyle", format!(r#"<w:pStyle w:val="{id}"/>"#));
                }
            }
            0x2403 | 0x2461 => pp.props.set(p_order("jc"), "jc", format!(r#"<w:jc w:val="{}"/>"#, jc(s.u8()))),
            0x2405 => pp.props.set(p_order("keepLines"), "keepLines", on_off("keepLines", s.u8())),
            0x2406 => pp.props.set(p_order("keepNext"), "keepNext", on_off("keepNext", s.u8())),
            0x2407 => pp.props.set(p_order("pageBreakBefore"), "pageBreakBefore", on_off("pageBreakBefore", s.u8())),
            0x2431 => pp.props.set(p_order("widowControl"), "widowControl", on_off("widowControl", s.u8())),
            0x2433 => pp.props.set(p_order("kinsoku"), "kinsoku", on_off("kinsoku", s.u8())),
            0x2434 => pp.props.set(p_order("wordWrap"), "wordWrap", on_off("wordWrap", s.u8())),
            0x2435 => pp.props.set(p_order("overflowPunct"), "overflowPunct", on_off("overflowPunct", s.u8())),
            0x2436 => pp.props.set(p_order("topLinePunct"), "topLinePunct", on_off("topLinePunct", s.u8())),
            0x2437 => pp.props.set(p_order("autoSpaceDE"), "autoSpaceDE", on_off("autoSpaceDE", s.u8())),
            0x2438 => pp.props.set(p_order("autoSpaceDN"), "autoSpaceDN", on_off("autoSpaceDN", s.u8())),
            0x2441 => pp.props.set(p_order("bidi"), "bidi", on_off("bidi", s.u8())),
            0x2448 => pp.props.set(p_order("adjustRightInd"), "adjustRightInd", on_off("adjustRightInd", s.u8())),
            0x2447 => pp.props.set(p_order("snapToGrid"), "snapToGrid", on_off("snapToGrid", s.u8())),
            0x246D => pp.props.set(p_order("contextualSpacing"), "contextualSpacing", on_off("contextualSpacing", s.u8())),
            0x242A => pp.props.set(p_order("suppressAutoHyphens"), "suppressAutoHyphens", on_off("suppressAutoHyphens", s.u8())),
            0x4439 => {
                let v = match s.u16() {
                    0 => "top",
                    1 => "center",
                    2 => "baseline",
                    3 => "bottom",
                    _ => "auto",
                };
                pp.props.set(p_order("textAlignment"), "textAlignment", format!(r#"<w:textAlignment w:val="{v}"/>"#));
            }
            0x2640 => {
                if s.u8() < 9 {
                    pp.props.set(p_order("outlineLvl"), "outlineLvl", format!(r#"<w:outlineLvl w:val="{}"/>"#, s.u8()));
                }
            }
            0x840F | 0x845E => pp.ind[0] = Some(s.i16() as i32),
            0x840E | 0x845D => pp.ind[1] = Some(s.i16() as i32),
            0x8411 | 0x8460 => pp.ind[2] = Some(s.i16() as i32),
            0x4456 => pp.ind[3] = Some(s.i16() as i32),
            0x4455 => pp.ind[4] = Some(s.i16() as i32),
            0x4457 => pp.ind[5] = Some(s.i16() as i32),
            0xA413 => pp.spacing[0] = Some(s.u16().to_string()),
            0xA414 => pp.spacing[1] = Some(s.u16().to_string()),
            0x6412 => {
                let line = i16_at(s.arg, 0).unwrap_or(240);
                let mult = i16_at(s.arg, 2).unwrap_or(1);
                let (value, rule) = if mult != 0 {
                    (line as i32, "auto")
                } else if line >= 0 {
                    (line as i32, "atLeast")
                } else {
                    (-(line as i32), "exact")
                };
                pp.spacing[2] = Some(value.to_string());
                pp.spacing[3] = Some(rule.to_string());
            }
            0x4458 => pp.spacing[4] = Some(s.i16().to_string()),
            0x4459 => pp.spacing[5] = Some(s.i16().to_string()),
            0x245B | 0x245C => {
                let name = if s.op == 0x245B { "beforeAutospacing" } else { "afterAutospacing" };
                let cur = pp.spacing[6].take().unwrap_or_default();
                let cur: String = cur.split(' ').filter(|p| !p.is_empty() && !p.starts_with(&format!("w:{name}="))).map(|p| format!(" {p}")).collect();
                pp.spacing[6] = Some(format!(r#"{cur} w:{name}="{}""#, u8::from(s.u8() != 0)));
            }
            0xC60D | 0xC615 => {
                // PChgTabsPapxOperand: the stops removed, then the stops added with their alignment and leader.
                let a = s.arg;
                let del = a.first().copied().unwrap_or(0) as usize;
                let mut xml = String::new();
                for k in 0..del {
                    if let Some(pos) = i16_at(a, 1 + k * 2) {
                        xml.push_str(&format!(r#"<w:tab w:val="clear" w:pos="{pos}"/>"#));
                    }
                }
                let at = 1 + del * 2;
                let add = a.get(at).copied().unwrap_or(0) as usize;
                for k in 0..add {
                    let (Some(pos), Some(tbd)) = (i16_at(a, at + 1 + k * 2), a.get(at + 1 + add * 2 + k)) else { break };
                    let val = match tbd & 7 {
                        1 => "center",
                        2 => "right",
                        3 => "decimal",
                        4 => "bar",
                        _ => "left",
                    };
                    let leader = match (tbd >> 3) & 7 {
                        1 => "dot",
                        2 => "hyphen",
                        3 => "underscore",
                        4 => "heavy",
                        5 => "middleDot",
                        _ => "none",
                    };
                    xml.push_str(&format!(r#"<w:tab w:val="{val}" w:leader="{leader}" w:pos="{pos}"/>"#));
                }
                if !xml.is_empty() {
                    pp.props.set(p_order("tabs"), "tabs", format!("<w:tabs>{xml}</w:tabs>"));
                }
            }
            0x6424..=0x6428 | 0xC64E..=0xC652 => {
                let side = if s.op < 0xC000 { s.op - 0x6424 } else { s.op - 0xC64E };
                if let Some(b) = border(s.arg) {
                    pp.borders[side as usize] = Some(b);
                }
            }
            0x442D | 0xC64D => {
                if let Some(shd) = shading(s.arg) {
                    pp.props.set(p_order("shd"), "shd", format!("<w:shd {shd}/>"));
                }
            }
            0x261B => pp.frame.pc = Some(s.u8()),
            0x8418 => pp.frame.x = Some(s.i16()),
            0x8419 => pp.frame.y = Some(s.i16()),
            0x841A => pp.frame.w = Some(s.u16()),
            0x442B => pp.frame.h = Some(s.u16()),
            0x2423 => pp.frame.wrap = Some(s.u8()),
            0x842F => pp.frame.h_space = Some(s.i16()),
            0x842E => pp.frame.v_space = Some(s.i16()),
            0x260A => pp.ilvl = Some(s.u8()),
            0x460B => pp.ilfo = Some(s.u16()),
            0x2416 => pp.in_table = s.u8() != 0,
            0x2417 => pp.ttp = s.u8() != 0,
            0x6649 => pp.itap = u32_at(s.arg, 0).unwrap_or(0),
            0x244B => pp.inner_cell = s.u8() != 0,
            0x244C => pp.inner_ttp = s.u8() != 0,
            _ => {}
        }
    }
}

impl ParaProps {
    fn xml(&self) -> String {
        let mut p = self.props.clone();
        // A table's paragraphs carry the place of a floating table ([MS-DOC]: the table's position is that of its
        // paragraphs), which the table takes as its `tblpPr` (`row_def`); they are not frames of their own.
        if let Some(f) = self.frame.xml().filter(|_| !self.in_table) {
            p.set(p_order("framePr"), "framePr", f);
        }
        let borders: String = ["top", "left", "bottom", "right", "between"]
            .iter()
            .zip(&self.borders)
            .filter_map(|(side, b)| b.as_ref().map(|b| format!("<w:{side} {b}/>")))
            .collect();
        if !borders.is_empty() {
            p.set(p_order("pBdr"), "pBdr", format!("<w:pBdr>{borders}</w:pBdr>"));
        }
        if let Some(ilfo) = self.ilfo {
            let ilvl = self.ilvl.unwrap_or(0);
            let num = if ilfo >= 0xF801 { 0 } else { ilfo };
            p.set(p_order("numPr"), "numPr", format!(r#"<w:numPr><w:ilvl w:val="{ilvl}"/><w:numId w:val="{num}"/></w:numPr>"#));
        }
        let spacing: String = [("before", 0), ("after", 1), ("line", 2), ("lineRule", 3), ("beforeLines", 4), ("afterLines", 5)]
            .iter()
            .filter_map(|(name, i)| self.spacing[*i].as_ref().map(|v| format!(r#" w:{name}="{v}""#)))
            .collect::<String>()
            + self.spacing[6].as_deref().unwrap_or("");
        if !spacing.is_empty() {
            p.set(p_order("spacing"), "spacing", format!("<w:spacing{spacing}/>"));
        }
        let mut ind = String::new();
        if let Some(v) = self.ind[0] {
            ind.push_str(&format!(r#" w:left="{v}""#));
        }
        if let Some(v) = self.ind[3] {
            ind.push_str(&format!(r#" w:leftChars="{v}""#));
        }
        if let Some(v) = self.ind[1] {
            ind.push_str(&format!(r#" w:right="{v}""#));
        }
        if let Some(v) = self.ind[4] {
            ind.push_str(&format!(r#" w:rightChars="{v}""#));
        }
        if let Some(v) = self.ind[2] {
            if v < 0 {
                ind.push_str(&format!(r#" w:hanging="{}""#, -v));
            } else {
                ind.push_str(&format!(r#" w:firstLine="{v}""#));
            }
        }
        if let Some(v) = self.ind[5] {
            if v < 0 {
                ind.push_str(&format!(r#" w:hangingChars="{}""#, -v));
            } else {
                ind.push_str(&format!(r#" w:firstLineChars="{v}""#));
            }
        }
        if !ind.is_empty() {
            p.set(p_order("ind"), "ind", format!("<w:ind{ind}/>"));
        }
        p.xml()
    }
}

#[derive(Clone, Default)]
struct Frame {
    pc: Option<u8>,
    x: Option<i16>,
    y: Option<i16>,
    w: Option<u16>,
    h: Option<u16>,
    wrap: Option<u8>,
    h_space: Option<i16>,
    v_space: Option<i16>,
}

impl Frame {
    fn xml(&self) -> Option<String> {
        if self.pc.is_none() && self.x.is_none() && self.y.is_none() && self.w.is_none() && self.wrap.is_none() {
            return None;
        }
        let mut a = String::new();
        if let Some(w) = self.w.filter(|w| *w > 0) {
            a.push_str(&format!(r#" w:w="{w}""#));
        }
        if let Some(h) = self.h.filter(|h| *h & 0x7FFF > 0) {
            // The high bit: an exact height, else at least.
            a.push_str(&format!(r#" w:h="{}" w:hRule="{}""#, h & 0x7FFF, if h & 0x8000 != 0 { "exact" } else { "atLeast" }));
        }
        if let Some(v) = self.h_space {
            a.push_str(&format!(r#" w:hSpace="{v}""#));
        }
        if let Some(v) = self.v_space {
            a.push_str(&format!(r#" w:vSpace="{v}""#));
        }
        let wrap = match self.wrap {
            Some(1) => "notBeside",
            Some(2) => "around",
            Some(3) => "none",
            Some(4) => "tight",
            Some(5) => "through",
            _ => "auto",
        };
        a.push_str(&format!(r#" w:wrap="{wrap}""#));
        // PositionCodeOperand: the vertical anchor in bits 4-5 (margin, page, paragraph), the horizontal in bits 6-7
        // (column, margin, page).
        let pc = self.pc.unwrap_or(0);
        let v_anchor = match (pc >> 4) & 3 {
            1 => "page",
            2 => "text",
            _ => "margin",
        };
        let h_anchor = match (pc >> 6) & 3 {
            1 => "margin",
            2 => "page",
            _ => "text",
        };
        a.push_str(&format!(r#" w:vAnchor="{v_anchor}" w:hAnchor="{h_anchor}""#));
        match self.x.unwrap_or(0) {
            0 => {}
            -4 => a.push_str(r#" w:xAlign="center""#),
            -8 => a.push_str(r#" w:xAlign="right""#),
            -12 => a.push_str(r#" w:xAlign="inside""#),
            -16 => a.push_str(r#" w:xAlign="outside""#),
            x => a.push_str(&format!(r#" w:x="{}""#, x - 1)),
        }
        match self.y.unwrap_or(0) {
            0 => {}
            -4 => a.push_str(r#" w:yAlign="top""#),
            -8 => a.push_str(r#" w:yAlign="center""#),
            -12 => a.push_str(r#" w:yAlign="bottom""#),
            -16 => a.push_str(r#" w:yAlign="inside""#),
            -20 => a.push_str(r#" w:yAlign="outside""#),
            y => a.push_str(&format!(r#" w:y="{y}""#)),
        }
        Some(format!("<w:framePr{a}/>"))
    }
}

/// The styles of the stylesheet ([MS-DOC] 2.9.271): their ids for the package, and each one's properties.
#[derive(Default)]
struct Styles {
    styles: Vec<Option<Style>>,
    /// The fonts the stylesheet gives text with no font (`rgftcStandardChpStsh`: ASCII, East Asian, other).
    default_fonts: [u16; 3],
}

struct Style {
    sti: u16,
    kind: u8,
    base: u16,
    next: u16,
    name: String,
    papx: Vec<u8>,
    chpx: Vec<u8>,
}

impl Styles {
    fn id(&self, istd: u16) -> Option<String> {
        self.styles.get(istd as usize)?.as_ref().map(|_| format!("S{istd}"))
    }

    fn get(&self, istd: u16) -> Option<&Style> {
        self.styles.get(istd as usize)?.as_ref()
    }

    /// Whether the style (or one it is based on) turns on the toggle property `name`.
    fn toggle_on(&self, istd: u16, name: &str, fonts: &[String]) -> bool {
        let mut chain = Vec::new();
        let mut at = Some(istd);
        while let Some(i) = at {
            if chain.contains(&i) || chain.len() > 20 {
                break;
            }
            let Some(s) = self.get(i) else { break };
            chain.push(i);
            at = (s.base != 0x0FFF).then_some(s.base);
        }
        let mut rp = RunProps::default();
        for i in chain.iter().rev() {
            if let Some(s) = self.get(*i) {
                apply_chp(&mut rp, &s.chpx, fonts, self, &|_| false);
            }
        }
        rp.props.get(name).is_some_and(|x| !x.contains(r#"w:val="0""#))
    }
}

/// The canonical names of the built-in styles whose identity matters to the `.docx` reader ([MS-DOC] 2.9.270 `sti`).
fn builtin_name(sti: u16) -> Option<&'static str> {
    Some(match sti {
        0 => "Normal",
        1 => "heading 1",
        2 => "heading 2",
        3 => "heading 3",
        4 => "heading 4",
        5 => "heading 5",
        6 => "heading 6",
        7 => "heading 7",
        8 => "heading 8",
        9 => "heading 9",
        65 => "Default Paragraph Font",
        105 => "Normal Table",
        107 => "No List",
        _ => return None,
    })
}

fn read_styles(table: &[u8], fc: usize, lcb: usize) -> Styles {
    let mut styles = Styles::default();
    let Some(stsh) = table.get(fc..fc + lcb) else { return styles };
    let Some(cb_stshi) = u16_at(stsh, 0) else { return styles };
    let stshi = &stsh[2..(2 + cb_stshi as usize).min(stsh.len())];
    let cstd = u16_at(stshi, 0).unwrap_or(0);
    let cb_base = u16_at(stshi, 2).unwrap_or(10) as usize;
    for k in 0..3 {
        styles.default_fonts[k] = u16_at(stshi, 12 + k * 2).unwrap_or(0);
    }
    let mut at = 2 + cb_stshi as usize;
    for _ in 0..cstd {
        let Some(cb) = u16_at(stsh, at) else { break };
        at += 2;
        let Some(std) = stsh.get(at..at + cb as usize) else { break };
        at += cb as usize;
        if cb == 0 {
            styles.styles.push(None);
            continue;
        }
        styles.styles.push(read_std(std, cb_base));
    }
    styles
}

fn read_std(std: &[u8], cb_base: usize) -> Option<Style> {
    let sti = u16_at(std, 0)? & 0x0FFF;
    let w2 = u16_at(std, 2)?;
    let kind = (w2 & 0x000F) as u8;
    let base = w2 >> 4;
    let w3 = u16_at(std, 4)?;
    let cupx = (w3 & 0x000F) as usize;
    let next = w3 >> 4;
    let mut at = cb_base;
    // Xstz: a 16-bit count and that many UTF-16 characters, then a 0.
    let cch = u16_at(std, at)? as usize;
    let units: Vec<u16> = (0..cch).filter_map(|i| u16_at(std, at + 2 + i * 2)).collect();
    let name = String::from_utf16_lossy(&units);
    at += 2 + cch * 2 + 2;
    let mut upx = Vec::new();
    for _ in 0..cupx {
        let cb = u16_at(std, at)? as usize;
        let data = std.get(at + 2..at + 2 + cb)?.to_vec();
        upx.push(data);
        at += 2 + cb;
        at += at % 2;
    }
    let (papx, chpx) = match kind {
        // A paragraph style: its paragraph properties (after the 16-bit istd) and its character properties.
        1 => (upx.first().map(|u| u.get(2..).unwrap_or(&[]).to_vec()).unwrap_or_default(), upx.get(1).cloned().unwrap_or_default()),
        2 => (Vec::new(), upx.first().cloned().unwrap_or_default()),
        // A table style: its table, paragraph and character properties.
        3 => (upx.get(1).map(|u| u.get(2..).unwrap_or(&[]).to_vec()).unwrap_or_default(), upx.get(2).cloned().unwrap_or_default()),
        _ => (Vec::new(), Vec::new()),
    };
    Some(Style { sti, kind, base, next, name, papx, chpx })
}

/// The font names of `SttbfFfn` ([MS-DOC] 2.9.272, each an `FFN`, 2.9.86, whose name is at its 40th byte).
fn read_fonts(table: &[u8], fc: usize, lcb: usize) -> Vec<String> {
    let mut out = Vec::new();
    let Some(sttb) = table.get(fc..fc + lcb) else { return out };
    let count = u16_at(sttb, 0).unwrap_or(0);
    let mut at = 4;
    for _ in 0..count {
        let Some(cb) = u8_at(sttb, at) else { break };
        let Some(ffn) = sttb.get(at + 1..at + 1 + cb as usize) else { break };
        at += 1 + cb as usize;
        let units: Vec<u16> = (0..).map_while(|i| u16_at(ffn, 39 + i * 2).filter(|u| *u != 0)).collect();
        out.push(String::from_utf16_lossy(&units));
    }
    out
}

/// The formatted runs of the `CHPX` or `PAPX` pages: each a range of file positions in the WordDocument stream and its
/// properties ([MS-DOC] 2.8.6 `PlcBteChpx`, 2.9.44 `ChpxFkp`, 2.8.7 `PlcBtePapx`, 2.9.176 `PapxFkp`).
struct Fkps {
    runs: Vec<(u32, u32, Vec<u8>, u16)>,
}

impl Fkps {
    fn find(&self, fc: u32) -> Option<&(u32, u32, Vec<u8>, u16)> {
        let i = self.runs.partition_point(|r| r.1 <= fc);
        self.runs.get(i).filter(|r| r.0 <= fc)
    }
}

fn read_fkps(wd: &[u8], table: &[u8], data_stream: &[u8], fc: usize, lcb: usize, para: bool) -> Fkps {
    let mut runs = Vec::new();
    if let Some(plc) = table.get(fc..fc + lcb) {
        let n = lcb.saturating_sub(4) / 8;
        for i in 0..n {
            let Some(pn) = u32_at(plc, (n + 1) * 4 + i * 4) else { break };
            let page_at = (pn & 0x003F_FFFF) as usize * 512;
            let Some(page) = wd.get(page_at..page_at + 512) else { continue };
            let count = page[511] as usize;
            for k in 0..count {
                let (Some(start), Some(end)) = (u32_at(page, k * 4), u32_at(page, (k + 1) * 4)) else { break };
                if para {
                    let bx_at = (count + 1) * 4 + k * 13;
                    let offset = page.get(bx_at).copied().unwrap_or(0) as usize * 2;
                    if offset == 0 {
                        runs.push((start, end, Vec::new(), 0));
                        continue;
                    }
                    let cb = page.get(offset).copied().unwrap_or(0) as usize;
                    let (data_at, len) = if cb != 0 { (offset + 1, cb * 2 - 1) } else { (offset + 2, page.get(offset + 1).copied().unwrap_or(0) as usize * 2) };
                    let data = page.get(data_at..data_at + len).unwrap_or(&[]);
                    let istd = u16_at(data, 0).unwrap_or(0);
                    let mut grpprl = data.get(2..).unwrap_or(&[]).to_vec();
                    // sprmPHugePapx: the rest of the properties, too many for the page (a row of many cells), are in the Data
                    // stream at the operand's offset, as a PrcData (the size, two bytes, then the sprms).
                    let huge = sprms(&grpprl).into_iter().find(|s| s.op == 0x6646).and_then(|s| u32_at(s.arg, 0)).map(|at| at as usize);
                    if let Some(at) = huge {
                        let cb = u16_at(data_stream, at).unwrap_or(0) as usize;
                        grpprl.extend_from_slice(data_stream.get(at + 2..at + 2 + cb).unwrap_or(&[]));
                    }
                    runs.push((start, end, grpprl, istd));
                } else {
                    let offset = page.get((count + 1) * 4 + k).copied().unwrap_or(0) as usize * 2;
                    let grpprl = if offset == 0 {
                        Vec::new()
                    } else {
                        let cb = page.get(offset).copied().unwrap_or(0) as usize;
                        page.get(offset + 1..offset + 1 + cb).unwrap_or(&[]).to_vec()
                    };
                    runs.push((start, end, grpprl, 0));
                }
            }
        }
    }
    runs.sort_by_key(|r| r.0);
    Fkps { runs }
}

/// A character of the text: its code, and where it is in the WordDocument stream (the key to its formatting), and the
/// piece's own properties (`Prm`) when it has some.
struct Ch {
    c: u16,
    fc: u32,
    prm: Option<usize>,
}

/// The piece table ([MS-DOC] 2.9.38 `Clx`): the characters of the document and the property lists of `Prc`.
fn read_text(wd: &[u8], table: &[u8], fc: usize, lcb: usize) -> Option<(Vec<Ch>, Vec<Vec<u8>>)> {
    let clx = table.get(fc..fc + lcb)?;
    let mut prcs = Vec::new();
    let mut at = 0;
    while *clx.get(at)? == 0x01 {
        let cb = u16_at(clx, at + 1)? as usize;
        prcs.push(clx.get(at + 3..at + 3 + cb)?.to_vec());
        at += 3 + cb;
    }
    if *clx.get(at)? != 0x02 {
        return None;
    }
    let plc_len = u32_at(clx, at + 1)? as usize;
    let plc = clx.get(at + 5..at + 5 + plc_len)?;
    let n = plc_len.checked_sub(4)? / 12;
    let mut text = Vec::new();
    for i in 0..n {
        let cp_start = u32_at(plc, i * 4)? as usize;
        let cp_end = u32_at(plc, (i + 1) * 4)? as usize;
        let count = cp_end.saturating_sub(cp_start);
        let pcd = (n + 1) * 4 + i * 8;
        let fc = u32_at(plc, pcd + 2)?;
        let prm = u16_at(plc, pcd + 6)?;
        let prm = (prm & 1 == 1).then_some((prm >> 1) as usize);
        if fc & 0x4000_0000 != 0 {
            let start = (fc & 0x3FFF_FFFF) / 2;
            let bytes = wd.get(start as usize..start as usize + count)?;
            let (decoded, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
            // Windows-1252 decodes each byte to one character.
            for (k, ch) in decoded.chars().enumerate() {
                text.push(Ch { c: ch as u32 as u16, fc: start + k as u32, prm });
            }
        } else {
            for k in 0..count {
                let c = u16_at(wd, fc as usize + k * 2)?;
                text.push(Ch { c, fc: fc + (k * 2) as u32, prm });
            }
        }
    }
    Some((text, prcs))
}

/// A section's properties (`w:sectPr`, [MS-DOC] 2.6.4 for the `Sprm`s, with the defaults of 2.4.5 `SEP`).
fn sect_pr(grpprl: &[u8], refs: &str) -> String {
    let (mut w, mut h, mut orient) = (12240i32, 15840i32, false);
    let (mut top, mut bottom, mut left, mut right, mut header, mut footer, mut gutter) = (1440i32, 1440i32, 1800i32, 1800i32, 720i32, 720i32, 0i32);
    let (mut cols, mut col_space) = (1u16, 720i32);
    let (mut grid_type, mut line_pitch, mut char_space) = (0u16, None::<i32>, None::<i32>);
    let mut text_flow = None;
    let mut v_align = None;
    let mut title_pg = false;
    let mut bkc = 2u8;
    let (mut pgn_restart, mut pgn_start, mut pgn_fmt) = (false, 1u16, None::<u8>);
    for s in sprms(grpprl) {
        match s.op {
            0xB01F => w = s.u16() as i32,
            0xB020 => h = s.u16() as i32,
            0x301D => orient = s.u8() == 2,
            0x9023 => top = s.i16() as i32,
            0x9024 => bottom = s.i16() as i32,
            0xB021 => left = s.u16() as i32,
            0xB022 => right = s.u16() as i32,
            0xB017 => header = s.u16() as i32,
            0xB018 => footer = s.u16() as i32,
            0xB025 => gutter = s.u16() as i32,
            0x500B => cols = s.u16() + 1,
            0x900C => col_space = s.i16() as i32,
            0x5032 => grid_type = s.u16(),
            0x9031 => line_pitch = Some(s.i16() as i32),
            0x7030 => char_space = u32_at(s.arg, 0).map(|v| v as i32),
            0x5033 => text_flow = Some(s.u16()),
            0x301A => v_align = Some(s.u8()),
            0x300A => title_pg = s.u8() != 0,
            0x3009 => bkc = s.u8(),
            0x3011 => pgn_restart = s.u8() != 0,
            0x501C => pgn_start = s.u16(),
            0x300E => pgn_fmt = Some(s.u8()),
            _ => {}
        }
    }
    let kind = match bkc {
        0 => "continuous",
        1 => "nextColumn",
        3 => "evenPage",
        4 => "oddPage",
        _ => "nextPage",
    };
    let mut x = format!(r#"<w:sectPr>{refs}<w:type w:val="{kind}"/><w:pgSz w:w="{w}" w:h="{h}"{}/>"#, if orient { r#" w:orient="landscape""# } else { "" });
    x.push_str(&format!(r#"<w:pgMar w:top="{top}" w:right="{right}" w:bottom="{bottom}" w:left="{left}" w:header="{header}" w:footer="{footer}" w:gutter="{gutter}"/>"#));
    // The page numbers: their format, and their start when the section restarts them.
    let mut pgn = String::new();
    if let Some(f) = pgn_fmt.filter(|f| *f != 0) {
        pgn.push_str(&format!(r#" w:fmt="{}""#, num_fmt(f)));
    }
    if pgn_restart {
        pgn.push_str(&format!(r#" w:start="{pgn_start}""#));
    }
    if !pgn.is_empty() {
        x.push_str(&format!("<w:pgNumType{pgn}/>"));
    }
    x.push_str(&format!(r#"<w:cols w:space="{col_space}"{}/>"#, if cols > 1 { format!(r#" w:num="{cols}""#) } else { String::new() }));
    if title_pg {
        x.push_str("<w:titlePg/>");
    }
    if let Some(v) = text_flow {
        let v = match v {
            1 => "tbRl",
            3 => "btLr",
            4 => "lrTbV",
            5 => "tbRlV",
            _ => "lrTb",
        };
        x.push_str(&format!(r#"<w:textDirection w:val="{v}"/>"#));
    }
    if let Some(v) = v_align {
        let v = match v {
            1 => "center",
            2 => "both",
            3 => "bottom",
            _ => "top",
        };
        x.push_str(&format!(r#"<w:vAlign w:val="{v}"/>"#));
    }
    // sprmSClm: 1 lines and characters, 2 lines only, 3 characters snapped to the grid.
    let grid = match grid_type {
        1 => Some("linesAndChars"),
        2 => Some("lines"),
        3 => Some("snapToChars"),
        _ => None,
    };
    let mut g = String::new();
    if let Some(t) = grid {
        g.push_str(&format!(r#" w:type="{t}""#));
    }
    if let Some(p) = line_pitch {
        g.push_str(&format!(r#" w:linePitch="{p}""#));
    }
    if let Some(c) = char_space.filter(|_| matches!(grid_type, 1 | 3)) {
        g.push_str(&format!(r#" w:charSpace="{c}""#));
    }
    if !g.is_empty() {
        x.push_str(&format!("<w:docGrid{g}/>"));
    }
    x.push_str("</w:sectPr>");
    x
}

/// The kinds of border of `brcType` ([MS-DOC] 2.9.21 Brc's brcType, as ECMA-376 17.18.2 names them).
fn border_kind(t: u8) -> &'static str {
    match t {
        1 => "single",
        2 => "thick",
        3 => "double",
        5 => "single",
        6 => "dotted",
        7 => "dashed",
        8 => "dotDash",
        9 => "dotDotDash",
        10 => "triple",
        11 => "thinThickSmallGap",
        12 => "thickThinSmallGap",
        13 => "thinThickThinSmallGap",
        14 => "thinThickMediumGap",
        15 => "thickThinMediumGap",
        16 => "thinThickThinMediumGap",
        17 => "thinThickLargeGap",
        18 => "thickThinLargeGap",
        19 => "thinThickThinLargeGap",
        20 => "wave",
        21 => "doubleWave",
        22 => "dashSmallGap",
        23 => "dashDotStroked",
        24 => "threeDEmboss",
        25 => "threeDEngrave",
        26 => "outset",
        27 => "inset",
        _ => "single",
    }
}

/// A border's attributes from a `Brc80` (4 bytes: width in eighths of a point, type, `ico`, space) or a `Brc` (8 bytes:
/// a `COLORREF`, then width, type, space). `None` for no border given; `Some("nil")` for one taken away.
fn border(b: &[u8]) -> Option<String> {
    let (color, width, kind, space) = match b.len() {
        4 => {
            if b == [0xFF; 4] {
                return Some(r#"w:val="nil""#.into());
            }
            (ICO.get(b[2] as usize).copied().unwrap_or("auto").to_string(), b[0], b[1], b[3] & 0x1F)
        }
        8 => {
            if b[4..] == [0xFF; 4] || b == [0xFF; 8] {
                return Some(r#"w:val="nil""#.into());
            }
            let color = if b[3] == 0xFF { "auto".to_string() } else { format!("{:02X}{:02X}{:02X}", b[0], b[1], b[2]) };
            (color, b[4], b[5], b[6] & 0x1F)
        }
        _ => return None,
    };
    if kind == 0 {
        return if width == 0 { None } else { Some(r#"w:val="nil""#.into()) };
    }
    if kind == 0xFF {
        return Some(r#"w:val="nil""#.into());
    }
    let size = if kind == 5 { 2 } else { width };
    Some(format!(r#"w:val="{}" w:sz="{size}" w:space="{space}" w:color="{color}""#, border_kind(kind)))
}

/// A shading's attributes from a `Shd80` (2 bytes: fore and back `ico`, pattern) or a `Shd` (10 bytes: fore and back
/// `COLORREF`, pattern).
fn shading(b: &[u8]) -> Option<String> {
    let (fore, back, pat) = match b.len() {
        2 => {
            let v = u16::from_le_bytes([b[0], b[1]]);
            let ico = |i: u16| ICO.get(i as usize).copied().unwrap_or("auto").to_string();
            (ico(v & 0x1F), ico((v >> 5) & 0x1F), v >> 10)
        }
        10 => {
            let cv = |c: &[u8]| if c[3] == 0xFF { "auto".to_string() } else { format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2]) };
            (cv(&b[0..4]), cv(&b[4..8]), u16::from_le_bytes([b[8], b[9]]))
        }
        _ => return None,
    };
    if pat == 0xFFFF {
        return None;
    }
    let val = match pat {
        0 => "clear",
        1 => "solid",
        2 => "pct5",
        3 => "pct10",
        4 => "pct20",
        5 => "pct25",
        6 => "pct30",
        7 => "pct40",
        8 => "pct50",
        9 => "pct60",
        10 => "pct70",
        11 => "pct75",
        12 => "pct80",
        13 => "pct90",
        14 => "horzStripe",
        15 => "vertStripe",
        16 => "reverseDiagStripe",
        17 => "diagStripe",
        18 => "horzCross",
        19 => "diagCross",
        _ => "clear",
    };
    if val == "clear" && back == "auto" {
        return None;
    }
    Some(format!(r#"w:val="{val}" w:color="{fore}" w:fill="{back}""#))
}

/// A table row's definition (`sprmTDefTable`, [MS-DOC] 2.9.316 `TDefTableOperand`): the cells' edges, and the row's
/// other table properties.
#[derive(Default)]
struct RowDef {
    edges: Vec<i16>,
    merges: Vec<u16>,
    gap_half: Option<i16>,
    dxa_left: Option<i16>,
    height: Option<i16>,
    jc: Option<u16>,
    /// sprmTTableWidth and sprmTWidthIndent: (unit, value) as `FtsWWidth` (2.9.95): 1 auto, 2 fiftieths of a percent, 3 twips.
    width: Option<(u8, u16)>,
    indent: Option<(u8, i16)>,
    /// sprmTCellPadding for every cell: top, left, bottom, right (twips).
    padding: [Option<u16>; 4],
    cant_split: bool,
    header: bool,
    /// sprmTFAutofit: the columns fit their contents (else the widths are fixed).
    autofit: bool,
    /// sprmTIstd: the table's style.
    style: Option<u16>,
    /// The table's borders (top, left, bottom, right, inside horizontal, inside vertical).
    borders: [Option<String>; 6],
    /// Each cell's borders (top, left, bottom, right), shading and vertical alignment.
    cell_borders: Vec<[Option<String>; 4]>,
    cell_shading: Vec<Option<String>>,
    cell_valign: Vec<u8>,
    /// A floating table's anchors (sprmTPc), place (sprmTDxaAbs, sprmTDyaAbs) and distance from the text (left, top, right,
    /// bottom).
    float_pc: Option<u8>,
    float_x: Option<i16>,
    float_y: Option<i16>,
    float_space: [Option<i16>; 4],
}

fn row_def(grpprl: &[u8]) -> RowDef {
    let mut r = RowDef::default();
    for s in sprms(grpprl) {
        match s.op {
            0xD608 => {
                let count = s.u8() as usize;
                r.edges = (0..=count).filter_map(|i| i16_at(s.arg, 1 + i * 2)).collect();
                let tc_at = 1 + (count + 1) * 2;
                r.merges = (0..count).map(|i| u16_at(s.arg, tc_at + i * 20).unwrap_or(0)).collect();
                // Tc80: tcgrf, the width, then the top, left, bottom and right Brc80.
                r.cell_borders = (0..count)
                    .map(|i| {
                        let at = tc_at + i * 20 + 4;
                        std::array::from_fn(|k| s.arg.get(at + k * 4..at + k * 4 + 4).and_then(border))
                    })
                    .collect();
                r.cell_shading = vec![None; count];
                // tcgrf's vertAlign (bits 7-8).
                r.cell_valign = r.merges.iter().map(|g| ((g >> 7) & 3) as u8).collect();
            }
            0x9602 => r.gap_half = Some(s.i16()),
            0x9601 => r.dxa_left = Some(s.i16()),
            0x9407 => r.height = Some(s.i16()),
            0x5400 | 0x548A => r.jc = Some(s.u16()),
            0xF614 => r.width = Some((s.u8(), u16_at(s.arg, 1).unwrap_or(0))),
            0xF661 => r.indent = Some((s.u8(), i16_at(s.arg, 1).unwrap_or(0))),
            0x3403 => r.cant_split = s.u8() != 0,
            0x3615 => r.autofit = s.u8() != 0,
            0x3404 => r.header = s.u8() != 0,
            0x563A => r.style = Some(s.u16()),
            0x360D => r.float_pc = Some(s.u8()),
            0x940E => r.float_x = Some(s.i16()),
            0x940F => r.float_y = Some(s.i16()),
            0x9410 => r.float_space[0] = Some(s.i16()),
            0x9411 => r.float_space[1] = Some(s.i16()),
            0x941E => r.float_space[2] = Some(s.i16()),
            0x941F => r.float_space[3] = Some(s.i16()),
            0xD605 | 0xD613 => {
                let size = if s.op == 0xD613 { 8 } else { 4 };
                for k in 0..6 {
                    if let Some(b) = s.arg.get(k * size..(k + 1) * size).and_then(border) {
                        r.borders[k] = Some(b);
                    }
                }
            }
            // sprmTSetBrc80 / sprmTSetBrc: cells first..lim, the sides (1 top, 2 left, 4 bottom, 8 right), the border.
            0xD620 | 0xD62F => {
                let size = if s.op == 0xD62F { 8 } else { 4 };
                let (Some(first), Some(lim), Some(sides)) = (s.arg.first(), s.arg.get(1), s.arg.get(2)) else { continue };
                let b = s.arg.get(3..3 + size).and_then(border);
                for c in *first as usize..(*lim as usize).min(r.cell_borders.len()) {
                    for (bit, side) in [(1, 0), (2, 1), (4, 2), (8, 3)] {
                        if sides & bit != 0 {
                            r.cell_borders[c][side] = b.clone();
                        }
                    }
                }
            }
            // sprmTDefTableShd80 / sprmTDefTableShd: a shading for each cell.
            0xD609 | 0xD612 => {
                let size = if s.op == 0xD612 { 10 } else { 2 };
                for (c, chunk) in s.arg.chunks_exact(size).enumerate() {
                    if c < r.cell_shading.len() {
                        r.cell_shading[c] = shading(chunk);
                    }
                }
            }
            // sprmTVertAlign: cells first..lim, the alignment.
            0xD62C => {
                let (Some(first), Some(lim), Some(v)) = (s.arg.first(), s.arg.get(1), s.arg.get(2)) else { continue };
                for c in *first as usize..(*lim as usize).min(r.cell_valign.len()) {
                    r.cell_valign[c] = *v;
                }
            }
            0xD634 | 0xD632 => {
                // CSSAOperand: the cells (first, limit), the sides (1 top, 2 left, 4 bottom, 8 right), the unit (3: twips) and the width.
                let (Some(sides), Some(unit), Some(w)) = (s.arg.get(2), s.arg.get(3), u16_at(s.arg, 4)) else { continue };
                if *unit == 3 || w == 0 {
                    for (bit, side) in [(1, 0), (2, 1), (4, 2), (8, 3)] {
                        if sides & bit != 0 {
                            r.padding[side] = Some(w);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    r
}

/// The number formats of `nfc` ([MS-DOC] 2.9.165 MSONFC, as ECMA-376 17.18.59 names them).
fn num_fmt(nfc: u8) -> &'static str {
    const NAMES: [&str; 40] = [
        "decimal", "upperRoman", "lowerRoman", "upperLetter", "lowerLetter", "ordinal", "cardinalText", "ordinalText", "hex", "chicago",
        "ideographDigital", "japaneseCounting", "aiueo", "iroha", "decimalFullWidth", "decimalHalfWidth", "japaneseLegal",
        "japaneseDigitalTenThousand", "decimalEnclosedCircle", "decimalFullWidth2", "aiueoFullWidth", "irohaFullWidth", "decimalZero",
        "bullet", "ganada", "chosung", "decimalEnclosedFullstop", "decimalEnclosedParen", "decimalEnclosedCircleChinese",
        "ideographEnclosedCircle", "ideographTraditional", "ideographZodiac", "ideographZodiacTraditional", "taiwaneseCounting",
        "ideographLegalTraditional", "taiwaneseCountingThousand", "taiwaneseDigital", "chineseCounting", "chineseLegalSimplified",
        "chineseCountingThousand",
    ];
    match nfc {
        0xFF => "none",
        // msonfcNumberInDash ("- 1 -"), as the section of corpus 023 saved as .doc by Word has it.
        0x39 => "numberInDash",
        0..=0x27 => NAMES[nfc as usize],
        _ => "decimal",
    }
}

/// `numbering.xml` from the lists ([MS-DOC] 2.8.27 `PlfLst`, with the levels, `LVL`, after it, and 2.8.28 `PlfLfo`): an
/// abstract number for each list, a number for each override (the paragraphs' `ilfo`, 1-based).
fn numbering_xml(table: &[u8], (fc_lst, lcb_lst): (usize, usize), (fc_lfo, lcb_lfo): (usize, usize), fonts: &[String], styles: &Styles) -> Option<String> {
    if lcb_lst == 0 || lcb_lfo == 0 {
        return None;
    }
    let count = u16_at(table, fc_lst)? as usize;
    let mut lists: Vec<(i32, bool)> = Vec::new();
    for k in 0..count {
        let at = fc_lst + 2 + k * 28;
        let lsid = u32_at(table, at)? as i32;
        let simple = table.get(at + 26)? & 1 != 0;
        lists.push((lsid, simple));
    }
    let mut x = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#);
    // The levels follow the list definitions.
    let mut at = fc_lst + lcb_lst;
    for (k, (_, simple)) in lists.iter().enumerate() {
        x.push_str(&format!(r#"<w:abstractNum w:abstractNumId="{k}"><w:multiLevelType w:val="{}"/>"#, if *simple { "singleLevel" } else { "multilevel" }));
        for ilvl in 0..if *simple { 1 } else { 9 } {
            let lvlf = table.get(at..at + 28)?;
            let start = u32_at(lvlf, 0)? as i32;
            let nfc = lvlf[4];
            let jc = match lvlf[5] & 3 {
                1 => "center",
                2 => "right",
                _ => "left",
            };
            let follow = match lvlf[15] {
                1 => "space",
                2 => "nothing",
                _ => "tab",
            };
            let (cb_chpx, cb_papx) = (lvlf[24] as usize, lvlf[25] as usize);
            let papx = table.get(at + 28..at + 28 + cb_papx)?;
            let chpx = table.get(at + 28 + cb_papx..at + 28 + cb_papx + cb_chpx)?;
            let xst_at = at + 28 + cb_papx + cb_chpx;
            let cch = u16_at(table, xst_at)? as usize;
            let units: Vec<u16> = (0..cch).filter_map(|i| u16_at(table, xst_at + 2 + i * 2)).collect();
            at = xst_at + 2 + cch * 2;
            // The number text: a character below 9 stands for that level's number.
            let mut text = String::new();
            for ch in char::decode_utf16(units.iter().copied()).filter_map(Result::ok) {
                if (ch as u32) < 9 {
                    text.push_str(&format!("%{}", ch as u32 + 1));
                } else {
                    text.push(ch);
                }
            }
            x.push_str(&format!(
                r#"<w:lvl w:ilvl="{ilvl}"><w:start w:val="{start}"/><w:numFmt w:val="{}"/><w:suff w:val="{follow}"/><w:lvlText w:val="{}"/><w:lvlJc w:val="{jc}"/>"#,
                num_fmt(nfc),
                esc(&text)
            ));
            let mut pp = ParaProps::default();
            apply_pap(&mut pp, papx, styles);
            let ppr = pp.xml();
            if !ppr.is_empty() {
                x.push_str(&format!("<w:pPr>{ppr}</w:pPr>"));
            }
            let mut rp = RunProps::default();
            apply_chp(&mut rp, chpx, fonts, styles, &|_| false);
            let rpr = rp.xml();
            if !rpr.is_empty() {
                x.push_str(&format!("<w:rPr>{rpr}</w:rPr>"));
            }
            x.push_str("</w:lvl>");
        }
        x.push_str("</w:abstractNum>");
    }
    let lfo_count = u32_at(table, fc_lfo)? as usize;
    let mut data_at = fc_lfo + 4 + lfo_count * 16;
    for k in 0..lfo_count {
        let at = fc_lfo + 4 + k * 16;
        let lsid = u32_at(table, at)? as i32;
        let clfolvl = *table.get(at + 12)? as usize;
        let abs = lists.iter().position(|(id, _)| *id == lsid).unwrap_or(0);
        x.push_str(&format!(r#"<w:num w:numId="{}"><w:abstractNumId w:val="{abs}"/>"#, k + 1));
        // LFOData: a character position, then the level overrides (an LFOLVL, and a LVL when it formats the level).
        data_at += 4;
        for _ in 0..clfolvl {
            let start = u32_at(table, data_at)? as i32;
            let flags = u32_at(table, data_at + 4)?;
            data_at += 8;
            let ilvl = flags & 0xF;
            if flags & 0x10 != 0 {
                x.push_str(&format!(r#"<w:lvlOverride w:ilvl="{ilvl}"><w:startOverride w:val="{start}"/></w:lvlOverride>"#));
            }
            if flags & 0x20 != 0 {
                // fFormatting: a whole LVL follows; it is skipped (the level as the list has it).
                let lvlf = table.get(data_at..data_at + 28)?;
                let (cb_chpx, cb_papx) = (lvlf[24] as usize, lvlf[25] as usize);
                let xst_at = data_at + 28 + cb_papx + cb_chpx;
                let cch = u16_at(table, xst_at)? as usize;
                data_at = xst_at + 2 + cch * 2;
            }
        }
        x.push_str("</w:num>");
    }
    x.push_str("</w:numbering>");
    Some(x)
}

/// The drawings of the document ([MS-ODRAW] in [MS-DOC] 2.9.172 OfficeArtContent): the pictures (by `pib`, 1-based), the
/// shapes of the main document (by `spid`), where each floating one is anchored (`PlcSpaMom`, by character position), the
/// text boxes' contents (by their text id), and the images written to the package.
#[derive(Default)]
struct Art {
    data: Vec<u8>,
    blips: Vec<Option<(&'static str, Vec<u8>)>>,
    shapes: std::collections::HashMap<u32, Shape>,
    /// The groups: a group shape's id, and the ids of the shapes and groups in it.
    groups: std::collections::HashMap<u32, Vec<u32>>,
    spa: Vec<(usize, Fspa)>,
    textboxes: Vec<(u32, String)>,
    media: std::cell::RefCell<Vec<(String, Vec<u8>)>>,
    next_id: std::cell::Cell<u32>,
}

#[derive(Clone, Default)]
struct Shape {
    kind: u16,
    pib: Option<u32>,
    txid: Option<u32>,
    fill: Option<u32>,
    filled: Option<bool>,
    line: Option<u32>,
    line_on: Option<bool>,
    line_width: Option<u32>,
    insets: [Option<i32>; 4],
    /// txflTextFlow ([MS-ODRAW] 2.3.21.13, an `MSOTXFL`, 2.4.32): how the text runs.
    text_flow: Option<u32>,
    /// A shape in a group: its place in the group's coordinates (`OfficeArtChildAnchor`: left, top, right, bottom).
    anchor: Option<[i32; 4]>,
    /// A group: its coordinates (`OfficeArtFSPGR`: left, top, right, bottom).
    group_rect: Option<[i32; 4]>,
    /// The rotation (`rotation`, 0x0004: degrees in 16.16 fixed point), and `OfficeArtFSP`'s fFlipH and fFlipV.
    rotation: i32,
    flip: (bool, bool),
    /// The shape's place in its drawing, back to front (1 the first).
    order: u32,
}

impl Shape {
    /// The rotation in degrees, 0 to 360.
    fn degrees(&self) -> f64 {
        (self.rotation as f64 / 65536.0).rem_euclid(360.0)
    }

    /// The shape's own box from its anchor (left, top, right, bottom): turned by 45 to 135 degrees (or 225 to 315) the anchor is
    /// the box turned a quarter, about its centre ([MS-ODRAW] 2.2.3 OfficeArtChildAnchor and FSPA), so its width and height swap.
    fn unturned(&self, [l, t, r, b]: [i32; 4]) -> [i32; 4] {
        let d = self.degrees() % 180.0;
        if (45.0..135.0).contains(&d) {
            let (cx, cy) = ((l as i64 + r as i64) / 2, (t as i64 + b as i64) / 2);
            let (hw, hh) = ((b as i64 - t as i64) / 2, (r as i64 - l as i64) / 2);
            [(cx - hw) as i32, (cy - hh) as i32, (cx + hw) as i32, (cy + hh) as i32]
        } else {
            [l, t, r, b]
        }
    }

    /// `a:xfrm`'s rot, flipH and flipV.
    fn xfrm_attrs(&self) -> String {
        let mut a = String::new();
        let deg = self.degrees();
        if deg > 0.0 {
            a.push_str(&format!(r#" rot="{}""#, (deg * 60000.0).round() as i64));
        }
        if self.flip.0 {
            a.push_str(r#" flipH="1""#);
        }
        if self.flip.1 {
            a.push_str(r#" flipV="1""#);
        }
        a
    }
}

#[derive(Clone, Copy, Default)]
struct Fspa {
    spid: u32,
    rect: [i32; 4],
    flags: u16,
}

/// An OfficeArt record's header: the version and instance, the type, and where its body is and how long.
fn record(buf: &[u8], at: usize) -> Option<(u16, u16, usize, usize)> {
    let vi = u16_at(buf, at)?;
    let kind = u16_at(buf, at + 2)?;
    let len = u32_at(buf, at + 4)? as usize;
    Some((vi >> 4, kind, at + 8, len))
}

/// The records `at..end` of a container.
fn children(buf: &[u8], mut at: usize, end: usize) -> Vec<(u16, u16, usize, usize)> {
    let mut out = Vec::new();
    while at + 8 <= end {
        let Some((inst, kind, body, len)) = record(buf, at) else { break };
        out.push((inst, kind, body, len));
        at = body + len;
    }
    out
}

/// A BLIP ([MS-ODRAW] 2.2.23): the image file's extension and bytes. A metafile's data is deflated; a DIB is a bitmap
/// file without its file header.
fn blip(buf: &[u8], at: usize) -> Option<(&'static str, Vec<u8>)> {
    let (inst, kind, body, len) = record(buf, at)?;
    let data = buf.get(body..body + len)?;
    let two_uids = |base: u16| inst == base + 1;
    match kind {
        0xF01A | 0xF01B | 0xF01C => {
            let (base, ext) = match kind {
                0xF01A => (0x3D4, "emf"),
                0xF01B => (0x216, "wmf"),
                _ => (0x542, "pct"),
            };
            let header = 16 + if two_uids(base) { 16 } else { 0 };
            let compressed = *data.get(header + 32)? == 0;
            let bytes = data.get(header + 34..)?;
            let bytes = if compressed {
                miniz_oxide::inflate::decompress_to_vec_zlib(bytes).or_else(|_| miniz_oxide::inflate::decompress_to_vec(bytes)).ok()?
            } else {
                bytes.to_vec()
            };
            Some((ext, bytes))
        }
        0xF01D | 0xF01E | 0xF01F | 0xF029 | 0xF02A => {
            let (base, ext) = match kind {
                0xF01D => (0x46A, "jpeg"),
                0xF02A => (0x6E2, "jpeg"),
                0xF01E => (0x6E0, "png"),
                0xF01F => (0x7A8, "bmp"),
                _ => (0x6E4, "tiff"),
            };
            let skip = 16 + if two_uids(base) { 16 } else { 0 } + 1;
            let bytes = data.get(skip..)?;
            if kind == 0xF01F {
                // A BITMAPINFOHEADER and its pixels: the file header goes before them.
                let header_size = u32_at(bytes, 0)?;
                let bpp = u16_at(bytes, 14)?;
                let compression = u32_at(bytes, 16)?;
                let used = u32_at(bytes, 32)?;
                let colors = if used > 0 { used } else if bpp <= 8 { 1 << bpp } else { 0 };
                let masks = if compression == 3 && header_size == 40 { 12 } else { 0 };
                let offset = 14 + header_size + colors * 4 + masks;
                let mut file = b"BM".to_vec();
                file.extend_from_slice(&(14 + bytes.len() as u32).to_le_bytes());
                file.extend_from_slice(&[0, 0, 0, 0]);
                file.extend_from_slice(&offset.to_le_bytes());
                file.extend_from_slice(bytes);
                return Some((ext, file));
            }
            Some((ext, bytes.to_vec()))
        }
        _ => None,
    }
}

/// A shape's properties (`OfficeArtFOPT`): the ones read are its picture, its text, its fill and its line.
fn shape_props(buf: &[u8], body: usize, count: u16, shape: &mut Shape) {
    for k in 0..count as usize {
        let (Some(id), Some(value)) = (u16_at(buf, body + k * 6), u32_at(buf, body + k * 6 + 2)) else { break };
        match id & 0x3FFF {
            0x0004 => shape.rotation = value as i32,
            0x0080 => shape.txid = Some(value),
            0x0081 => shape.insets[0] = Some(value as i32),
            0x0082 => shape.insets[1] = Some(value as i32),
            0x0083 => shape.insets[2] = Some(value as i32),
            0x0084 => shape.insets[3] = Some(value as i32),
            0x0088 => shape.text_flow = Some(value),
            0x0104 => shape.pib = Some(value),
            0x0181 => shape.fill = Some(value),
            // The fill and line Boolean properties: a bit and the bit that says it is set, 16 above.
            0x01BF => {
                if value & 0x0010_0000 != 0 {
                    shape.filled = Some(value & 0x10 != 0);
                }
            }
            0x01C0 => shape.line = Some(value),
            0x01CB => shape.line_width = Some(value),
            0x01FF => {
                if value & 0x0008_0000 != 0 {
                    shape.line_on = Some(value & 0x8 != 0);
                }
            }
            _ => {}
        }
    }
}

/// A shape container (`OfficeArtSpContainer`): its shape's id, with the shape recorded.
fn read_shape(buf: &[u8], body: usize, len: usize, shapes: &mut std::collections::HashMap<u32, Shape>, groups: &mut std::collections::HashMap<u32, Vec<u32>>) -> Option<u32> {
    let mut shape = Shape::default();
    let mut spid = None;
    let rect = |b: usize| -> [i32; 4] { std::array::from_fn(|i| u32_at(buf, b + i * 4).unwrap_or(0) as i32) };
    for (inst, kind, b, _) in children(buf, body, body + len) {
        match kind {
            0xF00A => {
                shape.kind = inst;
                spid = u32_at(buf, b);
                // OfficeArtFSP's flags: fFlipH (bit 6) and fFlipV (bit 7).
                let flags = u32_at(buf, b + 4).unwrap_or(0);
                shape.flip = (flags & 0x40 != 0, flags & 0x80 != 0);
            }
            0xF00B | 0xF122 => shape_props(buf, b, inst, &mut shape),
            0xF009 => shape.group_rect = Some(rect(b)),
            0xF00F => shape.anchor = Some(rect(b)),
            _ => {}
        }
    }
    if let Some(id) = spid {
        shape.order = shapes.len() as u32 + 1;
        shapes.insert(id, shape);
    }
    // A group's shapes are in the container too.
    read_shapes(buf, body, body + len, shapes, groups);
    spid
}

/// The shapes of a container; a group container (`OfficeArtSpgrContainer`) is its first shape, the group, and the shapes
/// and groups after it, which are the group's. The group's id is returned.
fn read_shapes(buf: &[u8], at: usize, end: usize, shapes: &mut std::collections::HashMap<u32, Shape>, groups: &mut std::collections::HashMap<u32, Vec<u32>>) -> Option<u32> {
    let mut group = None;
    let mut members = Vec::new();
    for (_, kind, body, len) in children(buf, at, end) {
        let id = match kind {
            0xF003 => read_shapes(buf, body, body + len, shapes, groups),
            0xF004 => read_shape(buf, body, len, shapes, groups),
            _ => continue,
        };
        match (group, id) {
            (None, Some(id)) if kind == 0xF004 => group = Some(id),
            (Some(_), Some(id)) => members.push(id),
            _ => {}
        }
    }
    if let Some(g) = group.filter(|_| !members.is_empty()) {
        groups.insert(g, members);
    }
    group
}

impl Art {
    fn read(wd: &[u8], table: &[u8], data: Vec<u8>, dgg: (usize, usize), spa: (usize, usize)) -> Art {
        let mut art = Art { data, ..Default::default() };
        let (fc, lcb) = dgg;
        let end = fc + lcb;
        if lcb > 8 {
            // The drawing group (its BStore of pictures), then each drawing: a byte saying whose (0: the main document), and
            // its container.
            if let Some((_, 0xF000, body, len)) = record(table, fc) {
                for (_, kind, b, l) in children(table, body, body + len) {
                    if kind != 0xF001 {
                        continue;
                    }
                    for (_, kind, fbse, flen) in children(table, b, b + l) {
                        if kind != 0xF007 {
                            art.blips.push(None);
                            continue;
                        }
                        let size = u32_at(table, fbse + 20).unwrap_or(0) as usize;
                        let delay = u32_at(table, fbse + 28).unwrap_or(u32::MAX) as usize;
                        let name_len = table.get(fbse + 33).copied().unwrap_or(0) as usize;
                        let image = if flen > 36 + name_len {
                            blip(table, fbse + 36 + name_len)
                        } else if size > 0 && delay != u32::MAX as usize {
                            blip(wd, delay)
                        } else {
                            None
                        };
                        art.blips.push(image);
                    }
                }
                let mut at = body + len;
                while at < end {
                    let Some(&owner) = table.get(at) else { break };
                    let Some((_, kind, b, l)) = record(table, at + 1) else { break };
                    if kind == 0xF002 && owner == 0 {
                        read_shapes(table, b, b + l, &mut art.shapes, &mut art.groups);
                    }
                    at = b + l;
                }
            }
        }
        // PlcSpaMom: the anchors' character positions and an FSPA (26 bytes) for each.
        let (fc, lcb) = spa;
        if let Some(plc) = table.get(fc..fc + lcb) {
            let n = lcb.saturating_sub(4) / 30;
            for k in 0..n {
                let cp = u32_at(plc, k * 4).unwrap_or(0) as usize;
                let at = (n + 1) * 4 + k * 26;
                let fspa = Fspa {
                    spid: u32_at(plc, at).unwrap_or(0),
                    rect: std::array::from_fn(|i| u32_at(plc, at + 4 + i * 4).unwrap_or(0) as i32),
                    flags: u16_at(plc, at + 20).unwrap_or(0),
                };
                art.spa.push((cp, fspa));
            }
        }
        art
    }

    /// The image of picture `pib` written to the package; its relationship id.
    fn image(&self, image: &(&'static str, Vec<u8>)) -> String {
        let mut media = self.media.borrow_mut();
        let n = media.len() + 1;
        media.push((format!("image{n}.{}", image.0), image.1.clone()));
        format!("rIdImg{n}")
    }

    fn id(&self) -> u32 {
        let id = self.next_id.get() + 1;
        self.next_id.set(id);
        id
    }

    /// An inline picture: the PICF at `fc` in the Data stream ([MS-DOC] 2.9.192 PICFAndOfficeArtData), its size, and the
    /// BLIP of the FBSE after its shape.
    fn inline_picture(&self, fc: u32) -> Option<String> {
        let at = fc as usize;
        let lcb = u32_at(&self.data, at)? as usize;
        let cb_header = u16_at(&self.data, at + 4)? as usize;
        let mm = u16_at(&self.data, at + 6)?;
        let (dxa, dya) = (i16_at(&self.data, at + 28)? as i64, i16_at(&self.data, at + 30)? as i64);
        let (mx, my) = (u16_at(&self.data, at + 32)? as i64, u16_at(&self.data, at + 34)? as i64);
        let mut p = at + cb_header;
        if mm == 0x66 {
            let n = *self.data.get(p)? as usize;
            p += 1 + n;
        }
        let end = at + lcb;
        let mut image = None;
        for (_, kind, body, len) in children(&self.data, p, end) {
            if kind == 0xF007 {
                let name_len = self.data.get(body + 33).copied().unwrap_or(0) as usize;
                image = blip(&self.data, body + 36 + name_len);
                if image.is_none() && len > 36 {
                    continue;
                }
                break;
            }
        }
        let image = image?;
        let rid = self.image(&image);
        let cx = dxa * mx / 1000 * 635;
        let cy = dya * my / 1000 * 635;
        let id = self.id();
        Some(format!(
            r#"<w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{id}" name="Picture {id}"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="{id}" name="Picture {id}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing>"#
        ))
    }

    /// A shape (not a picture) as a `wps:wsp` at `xfrm` (x, y, width, height) in its parent's space: its preset, its fill and
    /// line, and its text box.
    fn wsp_xml(&self, shape: &Shape, xfrm: [i64; 4]) -> String {
        let [x, y, cx, cy] = xfrm;
        // [MS-ODRAW] MSOSPT, as ECMA-376 names the same shapes (ST_ShapeType).
        let geom = match shape.kind {
            2 => "roundRect",
            3 => "ellipse",
            4 => "diamond",
            5 => "triangle",
            6 => "rtTriangle",
            7 => "parallelogram",
            9 => "hexagon",
            10 => "octagon",
            11 => "plus",
            12 => "star5",
            13 => "rightArrow",
            15 => "homePlate",
            16 => "cube",
            19 => "arc",
            20 | 32 => "line",
            21 => "plaque",
            22 => "can",
            23 => "donut",
            56 => "pentagon",
            66 => "leftArrow",
            67 => "downArrow",
            68 => "upArrow",
            69 => "leftRightArrow",
            70 => "upDownArrow",
            85 => "leftBracket",
            86 => "rightBracket",
            87 => "leftBrace",
            88 => "rightBrace",
            185 => "bracketPair",
            186 => "bracePair",
            _ => "rect",
        };
        // The brackets, the braces and the arc are not filled unless the shape says so (their shape types' fFilled is off).
        let unfilled_kind = matches!(shape.kind, 19 | 85 | 86 | 87 | 88 | 185 | 186);
        let color = |c: u32| format!("{:02X}{:02X}{:02X}", c & 0xFF, (c >> 8) & 0xFF, (c >> 16) & 0xFF);
        let fill = if shape.filled == Some(false) || (shape.filled.is_none() && unfilled_kind) {
            "<a:noFill/>".to_string()
        } else {
            format!(r#"<a:solidFill><a:srgbClr val="{}"/></a:solidFill>"#, shape.fill.filter(|c| c >> 24 == 0).map_or("FFFFFF".to_string(), color))
        };
        let line = if shape.line_on == Some(false) {
            "<a:ln><a:noFill/></a:ln>".to_string()
        } else {
            format!(
                r#"<a:ln w="{}"><a:solidFill><a:srgbClr val="{}"/></a:solidFill></a:ln>"#,
                shape.line_width.unwrap_or(9525),
                shape.line.filter(|c| c >> 24 == 0).map_or("000000".to_string(), color)
            )
        };
        let text = shape.txid.and_then(|txid| self.textboxes.iter().find(|(id, _)| *id == txid >> 16).map(|(_, x)| x.clone()));
        let [li, ti, ri, bi] = shape.insets;
        // MSOTXFL as ECMA-376 names it (20.1.10.83 ST_TextVerticalType): msotxflTtoBA (top to bottom, East Asian characters upright),
        // msotxflBtoT, msotxflTtoBN (all characters turned) and msotxflVertN (stacked); horizontal otherwise.
        let vert = match shape.text_flow {
            Some(1) => r#" vert="eaVert""#,
            Some(2) => r#" vert="vert270""#,
            Some(3) => r#" vert="vert""#,
            Some(5) => r#" vert="wordArtVert""#,
            _ => "",
        };
        let body_pr = format!(
            r#"<wps:bodyPr{vert} lIns="{}" tIns="{}" rIns="{}" bIns="{}"/>"#,
            li.unwrap_or(91440),
            ti.unwrap_or(45720),
            ri.unwrap_or(91440),
            bi.unwrap_or(45720)
        );
        let txbx = text.map(|t| format!("<wps:txbx><w:txbxContent>{t}</w:txbxContent></wps:txbx>")).unwrap_or_default();
        format!(
            r#"<wps:wsp><wps:cNvSpPr{}/><wps:spPr><a:xfrm{}><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="{geom}"><a:avLst/></a:prstGeom>{fill}{line}</wps:spPr>{txbx}{body_pr}</wps:wsp>"#,
            if shape.txid.is_some() { r#" txBox="1""# } else { "" },
            shape.xfrm_attrs()
        )
    }

    /// A group as a `wpg:wgp` of `cx` by `cy`: the shapes in it placed in its coordinates (`OfficeArtFSPGR`) by their
    /// child anchors (pictures as `pic:pic`), and the groups in it as `wpg:grpSp`, each in its parent's coordinates with its
    /// own.
    fn group_xml(&self, spid: u32, cx: i64, cy: i64) -> String {
        let rect = self.shapes.get(&spid).and_then(|s| s.group_rect).unwrap_or([0, 0, cx as i32, cy as i32]);
        let x = self.members_xml(spid, 0);
        let (gw, gh) = ((rect[2] - rect[0]) as i64, (rect[3] - rect[1]) as i64);
        format!(
            r#"<a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/><a:chOff x="{}" y="{}"/><a:chExt cx="{gw}" cy="{gh}"/></a:xfrm></wpg:grpSpPr>{x}</wpg:wgp></a:graphicData>"#,
            rect[0], rect[1]
        )
    }

    /// The members of group `spid` in its coordinates: shapes as `wps:wsp`, groups as `wpg:grpSp` (their anchor in this
    /// group's coordinates as off/ext, their `OfficeArtFSPGR` as chOff/chExt).
    fn members_xml(&self, spid: u32, depth: u32) -> String {
        let mut x = String::new();
        let Some(members) = self.groups.get(&spid) else { return x };
        if depth > 8 {
            return x;
        }
        for &id in members {
            let Some(shape) = self.shapes.get(&id) else { continue };
            match (self.groups.contains_key(&id), shape.group_rect, shape.anchor) {
                (true, Some([gl, gt, gr, gb]), Some([l, t, r, b])) => {
                    let inner = self.members_xml(id, depth + 1);
                    if inner.is_empty() {
                        continue;
                    }
                    x.push_str(&format!(
                        r#"<wpg:grpSp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="{l}" y="{t}"/><a:ext cx="{}" cy="{}"/><a:chOff x="{gl}" y="{gt}"/><a:chExt cx="{}" cy="{}"/></a:xfrm></wpg:grpSpPr>{inner}</wpg:grpSp>"#,
                        r - l,
                        b - t,
                        gr - gl,
                        gb - gt
                    ));
                }
                (false, _, Some(anchor)) => {
                    let [l, t, r, b] = shape.unturned(anchor);
                    let image = shape.pib.and_then(|p| self.blips.get((p as usize).wrapping_sub(1))).and_then(|b| b.as_ref());
                    match (shape.pib, image) {
                        (None, _) => x.push_str(&self.wsp_xml(shape, [l as i64, t as i64, (r - l) as i64, (b - t) as i64])),
                        // A picture in the group: `pic:pic` placed in the group's coordinates as a shape is.
                        (Some(_), Some(image)) => {
                            let (rid, pid) = (self.image(image), self.id());
                            x.push_str(&format!(
                                r#"<pic:pic><pic:nvPicPr><pic:cNvPr id="{pid}" name="Picture {pid}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm{}><a:off x="{l}" y="{t}"/><a:ext cx="{}" cy="{}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>"#,
                                shape.xfrm_attrs(),
                                r - l,
                                b - t
                            ));
                        }
                        (Some(_), None) => {}
                    }
                }
                _ => {}
            }
        }
        x
    }

    /// The floating shape anchored at `cp`: a picture, or a shape with its text box.
    /// `inline`: the shape is the result of a SHAPE field, as a `.doc` keeps a drawing in line with the text (its FSPA at 0, 0):
    /// it is written in line (`wp:inline`), as the `.docx` it was saved from had it.
    fn floating(&self, cp: usize, inline: bool) -> Option<String> {
        let (_, fspa) = self.spa.iter().find(|(c, _)| *c == cp)?;
        let shape = self.shapes.get(&fspa.spid).cloned().unwrap_or_default();
        let [l, t, r, b] = if self.groups.contains_key(&fspa.spid) { fspa.rect } else { shape.unturned(fspa.rect) };
        let (cx, cy) = (((r - l).max(0) as i64) * 635, ((b - t).max(0) as i64) * 635);
        // FSPA's flags: bx (bits 1-2) and by (bits 3-4) are what the place is from (margin, page, column or paragraph);
        // wr (bits 5-8) the wrapping; fBelowText (bit 14).
        let from_h = match (fspa.flags >> 1) & 3 {
            1 => "page",
            2 => "column",
            _ => "margin",
        };
        let from_v = match (fspa.flags >> 3) & 3 {
            1 => "page",
            2 => "paragraph",
            _ => "margin",
        };
        let wrap = match (fspa.flags >> 5) & 0xF {
            1 => "<wp:wrapTopAndBottom/>".to_string(),
            2 => r#"<wp:wrapSquare wrapText="bothSides"/>"#.to_string(),
            3 => "<wp:wrapNone/>".to_string(),
            4 => r#"<wp:wrapTight wrapText="bothSides"><wp:wrapPolygon edited="0"><wp:start x="0" y="0"/><wp:lineTo x="0" y="21600"/><wp:lineTo x="21600" y="21600"/><wp:lineTo x="21600" y="0"/><wp:lineTo x="0" y="0"/></wp:wrapPolygon></wp:wrapTight>"#.to_string(),
            5 => r#"<wp:wrapThrough wrapText="bothSides"><wp:wrapPolygon edited="0"><wp:start x="0" y="0"/><wp:lineTo x="0" y="21600"/><wp:lineTo x="21600" y="21600"/><wp:lineTo x="21600" y="0"/><wp:lineTo x="0" y="0"/></wp:wrapPolygon></wp:wrapThrough>"#.to_string(),
            _ => r#"<wp:wrapSquare wrapText="bothSides"/>"#.to_string(),
        };
        let behind = u8::from(fspa.flags & 0x4000 != 0);
        // Stacked in the drawing's order (the shapes read after are over those before), not the order of their anchors in the text.
        let z = if shape.order > 0 { shape.order } else { self.next_id.get() + 1 };
        let id = self.id();
        let graphic = if let Some(image) = shape.pib.and_then(|p| self.blips.get(p as usize - 1)).and_then(|b| b.as_ref()) {
            let rid = self.image(image);
            format!(
                r#"<a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="{id}" name="Picture {id}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm{}><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData>"#,
                shape.xfrm_attrs()
            )
        } else if self.groups.contains_key(&fspa.spid) {
            self.group_xml(fspa.spid, cx, cy)
        } else {
            format!(r#"<a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape">{}</a:graphicData>"#, self.wsp_xml(&shape, [0, 0, cx, cy]))
        };
        if inline {
            return Some(format!(
                r#"<w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="{id}" name="Shape {id}"/><wp:cNvGraphicFramePr/><a:graphic>{graphic}</a:graphic></wp:inline></w:drawing>"#
            ));
        }
        Some(format!(
            r#"<w:drawing><wp:anchor distT="0" distB="0" distL="114300" distR="114300" simplePos="0" relativeHeight="{z}" behindDoc="{behind}" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="{from_h}"><wp:posOffset>{}</wp:posOffset></wp:positionH><wp:positionV relativeFrom="{from_v}"><wp:posOffset>{}</wp:posOffset></wp:positionV><wp:extent cx="{cx}" cy="{cy}"/><wp:effectExtent l="0" t="0" r="0" b="0"/>{wrap}<wp:docPr id="{id}" name="Shape {id}"/><wp:cNvGraphicFramePr/><a:graphic>{graphic}</a:graphic></wp:anchor></w:drawing>"#,
            l as i64 * 635,
            t as i64 * 635
        ))
    }
}

/// What makes up the body: paragraphs, and tables (rows of cells of paragraphs).
enum Block {
    Para(String),
    Row { cells: Vec<String>, def: RowDef },
}

/// The paragraphs and table rows of the characters `from..to` (a story: the body, a header), in the formatting of the
/// files' pages. `sections` and `sect_xml` give the body's section breaks (none in the other stories).
#[allow(clippy::too_many_arguments)]
fn blocks_of(
    text: &[Ch],
    from: usize,
    to: usize,
    prcs: &[Vec<u8>],
    chpx: &Fkps,
    papx: &Fkps,
    styles: &Styles,
    fonts: &[String],
    sections: &[(usize, Vec<u8>)],
    sect_xml: &dyn Fn(usize) -> String,
    art: Option<&Art>,
    note: &dyn Fn(usize) -> Option<String>,
) -> Vec<Block> {
    let (styles_ref, fonts_ref) = (&styles, &fonts);
    let style_on = |istd: u16| move |name: &str| styles_ref.toggle_on(istd, name, fonts_ref);
    let mut blocks: Vec<Block> = Vec::new();
    let mut cells: Vec<String> = Vec::new();
    let mut cell = String::new();
    // The tables in a table's cell, from depth 2 down: each one's cells, the cell being filled, and its rows.
    let mut nest: Vec<(Vec<String>, String, Vec<Block>)> = Vec::new();
    let mut i = from;
    let mut field_depth: Vec<bool> = Vec::new();
    // Each open field's instruction (its first word tells a SHAPE field, whose result is a drawing in line).
    let mut field_code: Vec<String> = Vec::new();
    while i < to {
        // A paragraph runs to its mark: 0x0D, or 0x07 (a cell's or a row's end).
        // A section break (0x0C at the end of a section) ends its paragraph too.
        let end = (i..to)
            .find(|&k| text[k].c == 0x0D || text[k].c == 0x07 || (text[k].c == 0x0C && sections.iter().any(|(e, _)| *e == k + 1)))
            .unwrap_or(to - 1);
        let mark = &text[end];
        let (pap_grpprl, istd) = papx.find(mark.fc).map(|r| (r.2.as_slice(), r.3)).unwrap_or((&[], 0));
        let mut pp = ParaProps::default();
        if let Some(s) = styles.get(istd) {
            let _ = s;
            if let Some(id) = styles.id(istd) {
                pp.props.set(p_order("pStyle"), "pStyle", format!(r#"<w:pStyle w:val="{id}"/>"#));
            }
        }
        apply_pap(&mut pp, pap_grpprl, &styles);
        if let Some(prm) = mark.prm.and_then(|p| prcs.get(p)) {
            apply_pap(&mut pp, prm, &styles);
        }
        let para_style = istd;
        // The runs: characters of the same formatting together.
        let mut runs = String::new();
        let mut sect_after: Option<String> = None;
        let mut k = i;
        while k < end {
            let fc_run = chpx.find(text[k].fc);
            let key = fc_run.map(|r| r.0);
            let prm = text[k].prm;
            let mut j = k;
            while j < end && chpx.find(text[j].fc).map(|r| r.0) == key && text[j].prm == prm {
                j += 1;
            }
            let mut rp = RunProps::default();
            let on = style_on(para_style);
            if let Some(r) = fc_run {
                apply_chp(&mut rp, &r.2, &fonts, &styles, &on);
            }
            if let Some(p) = prm.and_then(|p| prcs.get(p)) {
                apply_chp(&mut rp, p, &fonts, &styles, &on);
            }
            let rpr = rp.xml();
            let rpr = if rpr.is_empty() { String::new() } else { format!("<w:rPr>{rpr}</w:rPr>") };
            let mut buf = String::new();
            let mut instr = String::new();
            let flush = |runs: &mut String, buf: &mut String, instr: &mut String| {
                if !buf.is_empty() {
                    runs.push_str(&format!(r#"<w:r>{rpr}<w:t xml:space="preserve">{}</w:t></w:r>"#, esc(buf)));
                    buf.clear();
                }
                if !instr.is_empty() {
                    runs.push_str(&format!(r#"<w:r>{rpr}<w:instrText xml:space="preserve">{}</w:instrText></w:r>"#, esc(instr)));
                    instr.clear();
                }
            };
            let mut idx = k;
            while idx < j {
                let c = text[idx].c;
                let cp = idx;
                idx += 1;
                match c {
                    0x13 => {
                        flush(&mut runs, &mut buf, &mut instr);
                        field_depth.push(true);
                        field_code.push(String::new());
                        runs.push_str(&format!(r#"<w:r>{rpr}<w:fldChar w:fldCharType="begin"/></w:r>"#));
                    }
                    0x14 => {
                        flush(&mut runs, &mut buf, &mut instr);
                        if let Some(last) = field_depth.last_mut() {
                            *last = false;
                        }
                        runs.push_str(&format!(r#"<w:r>{rpr}<w:fldChar w:fldCharType="separate"/></w:r>"#));
                    }
                    0x15 => {
                        flush(&mut runs, &mut buf, &mut instr);
                        field_depth.pop();
                        field_code.pop();
                        runs.push_str(&format!(r#"<w:r>{rpr}<w:fldChar w:fldCharType="end"/></w:r>"#));
                    }
                    _ if field_depth.last() == Some(&true) => {
                        // A field's instruction.
                        if let Some(ch) = char::from_u32(c as u32).filter(|ch| !ch.is_control()) {
                            instr.push(ch);
                            if let Some(code) = field_code.last_mut() {
                                code.push(ch);
                            }
                        }
                    }
                    0x09 => {
                        flush(&mut runs, &mut buf, &mut instr);
                        runs.push_str(&format!("<w:r>{rpr}<w:tab/></w:r>"));
                    }
                    0x0B => {
                        flush(&mut runs, &mut buf, &mut instr);
                        runs.push_str(&format!("<w:r>{rpr}<w:br/></w:r>"));
                    }
                    0x0C => {
                        flush(&mut runs, &mut buf, &mut instr);
                        // The end of a section, or a page break.
                        if let Some(index) = sections.iter().position(|(e, _)| *e == cp + 1) {
                            sect_after = Some(sect_xml(index));
                        } else {
                            runs.push_str(&format!(r#"<w:r>{rpr}<w:br w:type="page"/></w:r>"#));
                        }
                    }
                    0x0E => {
                        flush(&mut runs, &mut buf, &mut instr);
                        runs.push_str(&format!(r#"<w:r>{rpr}<w:br w:type="column"/></w:r>"#));
                    }
                    0x1E => {
                        flush(&mut runs, &mut buf, &mut instr);
                        runs.push_str(&format!("<w:r>{rpr}<w:noBreakHyphen/></w:r>"));
                    }
                    0x1F => {
                        flush(&mut runs, &mut buf, &mut instr);
                        runs.push_str(&format!("<w:r>{rpr}<w:softHyphen/></w:r>"));
                    }
                    // A picture in the line, and a floating shape anchored here.
                    0x01 if rp.special => {
                        flush(&mut runs, &mut buf, &mut instr);
                        if let Some(x) = art.zip(rp.pic_location).and_then(|(a, fc)| a.inline_picture(fc)) {
                            runs.push_str(&format!("<w:r>{rpr}{x}</w:r>"));
                        }
                    }
                    0x08 if rp.special => {
                        flush(&mut runs, &mut buf, &mut instr);
                        if let Some(x) = art.and_then(|a| a.floating(cp, field_depth.last() == Some(&false) && field_code.last().is_some_and(|c| c.trim_start().starts_with("SHAPE")))) {
                            runs.push_str(&format!("<w:r>{rpr}{x}</w:r>"));
                        }
                    }
                    // A footnote's reference mark: in the text, the note it names; in the note, its own number.
                    0x02 if rp.special => {
                        flush(&mut runs, &mut buf, &mut instr);
                        if let Some(x) = note(cp) {
                            runs.push_str(&format!("<w:r>{rpr}{x}</w:r>"));
                        }
                    }
                    _ if c < 0x20 => {}
                    _ if rp.symbol.is_some() => {
                        flush(&mut runs, &mut buf, &mut instr);
                        let (font, ch) = rp.symbol.as_ref().unwrap();
                        runs.push_str(&format!(r#"<w:r>{rpr}<w:sym w:font="{}" w:char="{ch:04X}"/></w:r>"#, esc(font)));
                    }
                    0xD800..=0xDBFF => {
                        // A surrogate pair is one character.
                        let low = text.get(idx).map(|t| t.c).filter(|l| (0xDC00..=0xDFFF).contains(l));
                        if let Some(low) = low.filter(|_| idx < j) {
                            idx += 1;
                            if let Some(ch) = char::decode_utf16([c, low]).next().and_then(Result::ok) {
                                buf.push(ch);
                            }
                        }
                    }
                    _ => buf.push(char::from_u32(c as u32).unwrap_or('\u{FFFD}')),
                }
            }
            flush(&mut runs, &mut buf, &mut instr);
            k = j;
        }
        // The paragraph mark's own formatting (the empty line's height, the mark's font).
        let mut mark_rp = RunProps::default();
        if let Some(r) = chpx.find(mark.fc) {
            apply_chp(&mut mark_rp, &r.2, &fonts, &styles, &style_on(para_style));
        }
        let mut ppr = pp.xml();
        let mark_xml = mark_rp.xml();
        if !mark_xml.is_empty() {
            ppr.push_str(&format!("<w:rPr>{mark_xml}</w:rPr>"));
        }
        let cp_end = end + 1;
        if sect_after.is_none() && (mark.c == 0x0D || mark.c == 0x0C) {
            if let Some(index) = sections.iter().position(|(e, _)| *e == cp_end).filter(|_| cp_end < to) {
                sect_after = Some(sect_xml(index));
            }
        }
        if let Some(s) = &sect_after {
            ppr.push_str(s);
        }
        let para = format!("<w:p>{}{runs}</w:p>", if ppr.is_empty() { String::new() } else { format!("<w:pPr>{ppr}</w:pPr>") });
        // A table in a table's cell ([MS-DOC] sprmPItap): its paragraphs are deeper than the cell's. A table that ends (the
        // next paragraph is shallower) goes into the cell around it.
        let depth = if pp.in_table { pp.itap.max(1) as usize } else { 0 };
        while nest.len() + 1 > depth.max(1) {
            let Some((rest_cells, rest_cell, rows)) = nest.pop() else { break };
            let mut xml = if rows.is_empty() { String::new() } else { table_xml(&rows) };
            for c in rest_cells.into_iter().chain(std::iter::once(rest_cell)) {
                xml.push_str(&c);
            }
            match nest.last_mut() {
                Some(outer) => outer.1.push_str(&xml),
                None => cell.push_str(&xml),
            }
        }
        if depth >= 2 {
            while nest.len() + 1 < depth {
                nest.push((Vec::new(), String::new(), Vec::new()));
            }
            let level = nest.last_mut().expect("a level");
            if pp.inner_ttp {
                if !level.1.is_empty() {
                    level.0.push(std::mem::take(&mut level.1));
                }
                level.2.push(Block::Row { cells: std::mem::take(&mut level.0), def: row_def(pap_grpprl) });
            } else if pp.inner_cell {
                level.1.push_str(&para);
                level.0.push(std::mem::take(&mut level.1));
            } else {
                level.1.push_str(&para);
            }
        } else if pp.in_table && mark.c == 0x07 && pp.ttp {
            // The end of a row: its properties are the mark's paragraph's.
            if !cell.is_empty() {
                cells.push(std::mem::take(&mut cell));
            }
            let def = row_def(pap_grpprl);
            blocks.push(Block::Row { cells: std::mem::take(&mut cells), def });
        } else if pp.in_table && mark.c == 0x07 {
            cell.push_str(&para);
            cells.push(std::mem::take(&mut cell));
        } else if pp.in_table {
            cell.push_str(&para);
        } else {
            if !cells.is_empty() || !cell.is_empty() {
                // A row left unfinished: its cells as paragraphs.
                for c in cells.drain(..) {
                    blocks.push(Block::Para(c));
                }
                blocks.push(Block::Para(std::mem::take(&mut cell)));
            }
            blocks.push(Block::Para(para));
        }
        i = end + 1;
    }
    blocks
}

/// The `.docx` package of a Word 97-2003 document, or `None` when it is not one this reader takes (encrypted, or an
/// older Word's).
pub(crate) fn to_docx(data: &[u8]) -> Option<Vec<u8>> {
    let wd = cfb::stream(data, "WordDocument").ok()?;
    if u16_at(&wd, 0)? != 0xA5EC {
        return None;
    }
    let flags = u16_at(&wd, 10)?;
    if flags & 0x0100 != 0 {
        return None;
    }
    let table = cfb::stream(data, if flags & 0x0200 != 0 { "1Table" } else { "0Table" }).ok()?;
    let cb_rg = u16_at(&wd, 152)? as usize;
    let pair = |i: usize| -> (usize, usize) {
        if i >= cb_rg {
            return (0, 0);
        }
        (u32_at(&wd, 154 + i * 8).unwrap_or(0) as usize, u32_at(&wd, 158 + i * 8).unwrap_or(0) as usize)
    };
    let ccp_text = u32_at(&wd, 76)? as usize;
    let (fc_stshf, lcb_stshf) = pair(1);
    let (fc_sed, lcb_sed) = pair(6);
    let (fc_chpx, lcb_chpx) = pair(12);
    let (fc_papx, lcb_papx) = pair(13);
    let (fc_ffn, lcb_ffn) = pair(15);
    let (fc_hdd, lcb_hdd) = pair(11);
    let (fc_dop, lcb_dop) = pair(31);
    let (fc_clx, lcb_clx) = pair(33);
    let lists = (pair(73), pair(74));

    let fonts = read_fonts(&table, fc_ffn, lcb_ffn);
    let styles = read_styles(&table, fc_stshf, lcb_stshf);
    let data_stream = cfb::stream(data, "Data").map(|d| d.to_vec()).unwrap_or_default();
    let chpx = read_fkps(&wd, &table, &data_stream, fc_chpx, lcb_chpx, false);
    let papx = read_fkps(&wd, &table, &data_stream, fc_papx, lcb_papx, true);
    let (all_text, prcs) = read_text(&wd, &table, fc_clx, lcb_clx)?;
    let all_text = all_text.as_slice();
    let text = &all_text[..ccp_text.min(all_text.len())];

    // The sections: where each ends (a character position) and its properties.
    let mut sections: Vec<(usize, Vec<u8>)> = Vec::new();
    if let Some(plc) = table.get(fc_sed..fc_sed + lcb_sed) {
        let n = lcb_sed.saturating_sub(4) / 16;
        for i in 0..n {
            let end = u32_at(plc, (i + 1) * 4).unwrap_or(0) as usize;
            let fc_sepx = u32_at(plc, (n + 1) * 4 + i * 12 + 2).unwrap_or(u32::MAX) as usize;
            let grpprl = if fc_sepx == 0xFFFF_FFFF {
                Vec::new()
            } else {
                let cb = u16_at(&wd, fc_sepx).unwrap_or(0) as usize;
                wd.get(fc_sepx + 2..fc_sepx + 2 + cb).unwrap_or(&[]).to_vec()
            };
            sections.push((end, grpprl));
        }
    }

    // The headers and footers ([MS-DOC] 2.8.25 PlcfHdd): six stories of separators, then six for each section (even and
    // odd header, even and odd footer, first page header and footer), in the header story after the main text and the
    // footnotes. An empty one is the previous section's.
    let ccp_ftn = u32_at(&wd, 80).unwrap_or(0) as usize;
    let ccp_hdd = u32_at(&wd, 84).unwrap_or(0) as usize;
    let ccp_atn = u32_at(&wd, 92).unwrap_or(0) as usize;
    let ccp_edn = u32_at(&wd, 96).unwrap_or(0) as usize;
    // The drawings, and the text boxes' stories ([MS-DOC] 2.8.31 PlcftxbxTxt): each ends with a paragraph mark of its own;
    // a shape names its text box by its place in the list, in the high 16 bits of its text id (`lTxid`).
    let mut art = Art::read(&wd, &table, data_stream, pair(50), pair(40));
    let txbx_base = ccp_text + ccp_ftn + ccp_hdd + ccp_atn + ccp_edn;
    let (fc_txbx, lcb_txbx) = pair(56);
    if let Some(plc) = table.get(fc_txbx..fc_txbx + lcb_txbx) {
        let n = lcb_txbx.saturating_sub(4) / 26;
        for k in 0..n {
            let (Some(a), Some(b)) = (u32_at(plc, k * 4), u32_at(plc, (k + 1) * 4)) else { break };
            let (a, b) = (txbx_base + a as usize, txbx_base + b as usize);
            if b <= a + 1 || b > all_text.len() {
                continue;
            }
            let blocks = blocks_of(all_text, a, b - 1, &prcs, &chpx, &papx, &styles, &fonts, &[], &|_| String::new(), None, &|_| None);
            art.textboxes.push((k as u32 + 1, blocks_xml(&blocks)));
        }
    }
    let header_base = ccp_text + ccp_ftn;
    let hdd: Vec<usize> = table.get(fc_hdd..fc_hdd + lcb_hdd).map(|p| (0..lcb_hdd / 4).filter_map(|i| u32_at(p, i * 4).map(|v| v as usize)).collect()).unwrap_or_default();
    let no_sections = |_: usize| String::new();
    let mut parts: Vec<(String, String)> = Vec::new();
    let mut refs: Vec<String> = Vec::new();
    for sec in 0..sections.len() {
        let mut r = String::new();
        for (k, (kind, footer)) in [("even", false), ("default", false), ("even", true), ("default", true), ("first", false), ("first", true)].into_iter().enumerate() {
            let at = 6 + sec * 6 + k;
            let (Some(a), Some(b)) = (hdd.get(at), hdd.get(at + 1)) else { continue };
            if *b <= *a + 1 || header_base + b > all_text.len() {
                continue;
            }
            // The story's last paragraph mark ends the story; the paragraphs are before it.
            let blocks = blocks_of(all_text, header_base + a, header_base + b - 1, &prcs, &chpx, &papx, &styles, &fonts, &[], &no_sections, Some(&art), &|_| None);
            let n = parts.len() + 1;
            let (root, tag) = if footer { ("ftr", "footer") } else { ("hdr", "header") };
            let xml = format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:{root} {{NS}}>{}</w:{root}>"#,
                blocks_xml(&blocks)
            )
            .replace("{NS}", NAMESPACES);
            parts.push((format!("{tag}{n}.xml"), xml));
            r.push_str(&format!(r#"<w:{tag}Reference w:type="{kind}" r:id="rIdH{n}"/>"#));
        }
        refs.push(r);
    }
    // The footnotes ([MS-DOC] PlcffndRef and PlcffndTxt): where each is referred to in the main text, and its
    // story in the footnote document (after the main text), which starts with its own reference mark and ends with a
    // paragraph mark. They are numbered from 1 in the order of the text; -1 and 0 are the separators.
    let (fc_fnd_ref, lcb_fnd_ref) = pair(2);
    let (fc_fnd_txt, lcb_fnd_txt) = pair(3);
    let cps = |fc: usize, lcb: usize, count: usize| -> Vec<usize> {
        table.get(fc..fc + lcb).map(|p| (0..count).filter_map(|i| u32_at(p, i * 4).map(|v| v as usize)).collect()).unwrap_or_default()
    };
    let n_notes = lcb_fnd_ref.saturating_sub(4) / 6;
    let note_refs = cps(fc_fnd_ref, lcb_fnd_ref, n_notes);
    let note_txts = cps(fc_fnd_txt, lcb_fnd_txt, lcb_fnd_txt / 4);
    let mut notes = String::new();
    for k in 0..n_notes {
        let (Some(a), Some(b)) = (note_txts.get(k), note_txts.get(k + 1)) else { break };
        let (a, b) = (ccp_text + a, ccp_text + b);
        if b <= a || b > all_text.len() {
            continue;
        }
        let own = |_: usize| Some("<w:footnoteRef/>".to_string());
        let blocks = blocks_of(all_text, a, b, &prcs, &chpx, &papx, &styles, &fonts, &[], &no_sections, Some(&art), &own);
        notes.push_str(&format!(r#"<w:footnote w:id="{}">{}</w:footnote>"#, k + 1, blocks_xml(&blocks)));
    }
    if !notes.is_empty() {
        let xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:footnotes {{NS}}><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>{notes}</w:footnotes>"#
        )
        .replace("{NS}", NAMESPACES);
        parts.push(("footnotes.xml".to_string(), xml));
    }
    let note_ref = |cp: usize| note_refs.iter().position(|&r| r == cp).map(|k| format!(r#"<w:footnoteReference w:id="{}"/>"#, k + 1));
    let sect_xml = |index: usize| sect_pr(&sections[index].1, refs.get(index).map_or("", String::as_str));
    let blocks = blocks_of(text, 0, text.len(), &prcs, &chpx, &papx, &styles, &fonts, &sections, &sect_xml, Some(&art), &note_ref);

    let mut body = blocks_xml(&blocks);
    let media = art.media.take();
    let last_sect = if sections.is_empty() { sect_pr(&[], "") } else { sect_xml(sections.len() - 1) };
    body.push_str(&last_sect);

    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {{NS}}><w:body>{body}</w:body></w:document>"#
    );
    let styles_xml = styles_xml(&styles, &fonts);
    let dop = table.get(fc_dop..fc_dop + lcb_dop).unwrap_or(&[]);
    let settings = settings_xml(dop);
    let numbering = numbering_xml(&table, lists.0, lists.1, &fonts, &styles);
    let document = document.replace("{NS}", NAMESPACES);
    package(&document, &styles_xml, &settings, numbering.as_deref(), &parts, &media).ok()
}

/// The XML of paragraphs and rows: rows in a run make a table.
fn blocks_xml(blocks: &[Block]) -> String {
    let mut out = String::new();
    let mut k = 0;
    while k < blocks.len() {
        match &blocks[k] {
            Block::Para(p) => {
                out.push_str(p);
                k += 1;
            }
            Block::Row { .. } => {
                let start = k;
                while k < blocks.len() && matches!(blocks[k], Block::Row { .. }) {
                    k += 1;
                }
                out.push_str(&table_xml(&blocks[start..k]));
            }
        }
    }
    if out.is_empty() {
        out.push_str("<w:p/>");
    }
    out
}

/// `settings.xml` from the document properties (`Dop`, [MS-DOC] 2.7): the default tab stop (`DopBase.dxaTab`), how
/// punctuation is compressed and kerned (`DopTypography`, 2.9.65, at byte 90 of a `Dop97` or later) and the layout
/// options of `Copts60` (2.9.51, byte 84) and `Copts` (2.9.48, whose bits after its `Copts80` are at byte 512 of a `Dop2000`
/// or later), as ECMA-376 names them. A `.doc` is laid out as Word 2003 lays it out (compatibilityMode 11).
fn settings_xml(dop: &[u8]) -> String {
    let byte = |at: usize| dop.get(at).copied();
    let bit = |at: usize, b: u8| byte(at).map(|v| v >> b & 1 == 1);
    let default_tab = u16_at(dop, 10).filter(|t| *t > 0).unwrap_or(720);
    let mut x = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:defaultTabStop w:val="{default_tab}"/>"#);
    if let Some(typography) = byte(90) {
        // fKerningPunct (bit 0) and iJustification (bits 1-2).
        if typography & 1 == 0 {
            x.push_str("<w:noPunctuationKerning/>");
        }
        let control = match typography >> 1 & 3 {
            1 => "compressPunctuation",
            2 => "compressPunctuationAndJapaneseKana",
            _ => "doNotCompress",
        };
        x.push_str(&format!(r#"<w:characterSpacingControl w:val="{control}"/>"#));
    }
    x.push_str("<w:compat>");
    // Copts60, its second byte: fLeaveBackslashAlone (4), fExpShRtn (5), fDntULTrlSpc (6), fDntBlnSbDbWid (7). Each is
    // the opposite of the ECMA-376 element.
    if bit(85, 7) == Some(false) {
        x.push_str("<w:balanceSingleByteDoubleByteWidth/>");
    }
    if bit(85, 4) == Some(false) {
        x.push_str("<w:doNotLeaveBackslashAlone/>");
    }
    if bit(85, 6) == Some(false) {
        x.push_str("<w:ulTrailSpace/>");
    }
    if bit(85, 5) == Some(false) {
        x.push_str("<w:doNotExpandShiftReturn/>");
    }
    // Copts: fDontUseHTMLParagraphAutoSpacing (bit 2) and fDontAdjustLineHeightInTable (bit 3).
    if bit(512, 3) == Some(false) {
        x.push_str("<w:adjustLineHeightInTable/>");
    }
    if bit(512, 2) == Some(true) {
        x.push_str("<w:doNotUseHTMLParagraphAutoSpacing/>");
    }
    // Word lays out a `.doc` with the mark of a paragraph that ends with a page break on the next page (Word's PDFs of the same
    // documents saved as `.docx` and as `.doc`: a page break and then an empty paragraph that ends a section leave a blank page only in the
    // `.doc`), which a `.docx` asks for with `splitPgBreakAndParaMark`.
    x.push_str(r#"<w:useFELayout/><w:splitPgBreakAndParaMark/><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="11"/></w:compat></w:settings>"#);
    x
}

fn table_xml(rows: &[Block]) -> String {
    // The grid: every edge of every row.
    let mut edges: Vec<i16> = rows
        .iter()
        .flat_map(|r| match r {
            Block::Row { def, .. } => def.edges.clone(),
            _ => Vec::new(),
        })
        .collect();
    edges.sort();
    edges.dedup();
    let mut x = String::from("<w:tbl><w:tblPr>");
    if let Some(Block::Row { def, .. }) = rows.first() {
        if let Some(st) = def.style {
            x.push_str(&format!(r#"<w:tblStyle w:val="S{st}"/>"#));
        }
        if def.float_pc.is_some() || def.float_x.is_some() || def.float_y.is_some() {
            let pc = def.float_pc.unwrap_or(0);
            let v_anchor = match (pc >> 4) & 3 {
                1 => "page",
                2 => "text",
                _ => "margin",
            };
            let h_anchor = match (pc >> 6) & 3 {
                1 => "margin",
                2 => "page",
                _ => "text",
            };
            let mut a = String::new();
            for (name, v) in ["leftFromText", "topFromText", "rightFromText", "bottomFromText"].iter().zip(&def.float_space) {
                if let Some(v) = v {
                    a.push_str(&format!(r#" w:{name}="{v}""#));
                }
            }
            a.push_str(&format!(r#" w:vertAnchor="{v_anchor}" w:horzAnchor="{h_anchor}""#));
            match def.float_x.unwrap_or(0) {
                0 => {}
                -4 => a.push_str(r#" w:tblpXSpec="center""#),
                -8 => a.push_str(r#" w:tblpXSpec="right""#),
                -12 => a.push_str(r#" w:tblpXSpec="inside""#),
                -16 => a.push_str(r#" w:tblpXSpec="outside""#),
                v => a.push_str(&format!(r#" w:tblpX="{v}""#)),
            }
            match def.float_y.unwrap_or(0) {
                0 => {}
                -4 => a.push_str(r#" w:tblpYSpec="top""#),
                -8 => a.push_str(r#" w:tblpYSpec="center""#),
                -12 => a.push_str(r#" w:tblpYSpec="bottom""#),
                -16 => a.push_str(r#" w:tblpYSpec="inside""#),
                -20 => a.push_str(r#" w:tblpYSpec="outside""#),
                v => a.push_str(&format!(r#" w:tblpY="{v}""#)),
            }
            x.push_str(&format!("<w:tblpPr{a}/>"));
        }
        match def.width {
            Some((2, w)) => x.push_str(&format!(r#"<w:tblW w:w="{w}" w:type="pct"/>"#)),
            Some((3, w)) => x.push_str(&format!(r#"<w:tblW w:w="{w}" w:type="dxa"/>"#)),
            _ => x.push_str(r#"<w:tblW w:w="0" w:type="auto"/>"#),
        }
        if let Some(jc) = def.jc {
            x.push_str(&format!(r#"<w:jc w:val="{}"/>"#, match jc { 1 => "center", 2 => "right", _ => "left" }));
        }
        let borders: String = ["top", "left", "bottom", "right", "insideH", "insideV"]
            .iter()
            .zip(&def.borders)
            .filter_map(|(side, b)| b.as_ref().map(|b| format!("<w:{side} {b}/>")))
            .collect();
        let gap = def.gap_half.unwrap_or(108);
        let left = def.padding[1].map_or(gap as i32, i32::from);
        let right = def.padding[3].map_or(gap as i32, i32::from);
        // The indent is to the text of the first cell: the left edge and its padding.
        let indent = match def.indent {
            Some((3, w)) => Some(w as i32),
            _ => def.edges.first().map(|first| *first as i32 + left),
        };
        if let Some(ind) = indent {
            x.push_str(&format!(r#"<w:tblInd w:w="{ind}" w:type="dxa"/>"#));
        }
        if !borders.is_empty() {
            x.push_str(&format!("<w:tblBorders>{borders}</w:tblBorders>"));
        }
        if !def.autofit {
            x.push_str(r#"<w:tblLayout w:type="fixed"/>"#);
        }
        x.push_str("<w:tblCellMar>");
        if let Some(t) = def.padding[0] {
            x.push_str(&format!(r#"<w:top w:w="{t}" w:type="dxa"/>"#));
        }
        x.push_str(&format!(r#"<w:left w:w="{left}" w:type="dxa"/>"#));
        if let Some(b) = def.padding[2] {
            x.push_str(&format!(r#"<w:bottom w:w="{b}" w:type="dxa"/>"#));
        }
        x.push_str(&format!(r#"<w:right w:w="{right}" w:type="dxa"/></w:tblCellMar>"#));
    }
    x.push_str("</w:tblPr><w:tblGrid>");
    for w in edges.windows(2) {
        x.push_str(&format!(r#"<w:gridCol w:w="{}"/>"#, w[1] - w[0]));
    }
    x.push_str("</w:tblGrid>");
    for row in rows {
        let Block::Row { cells, def } = row else { continue };
        x.push_str("<w:tr>");
        let mut trpr = String::new();
        if def.cant_split {
            trpr.push_str("<w:cantSplit/>");
        }
        if let Some(h) = def.height.filter(|h| *h != 0) {
            trpr.push_str(&format!(r#"<w:trHeight w:val="{}"{}/>"#, h.unsigned_abs(), if h < 0 { r#" w:hRule="exact""# } else { "" }));
        }
        if def.header {
            trpr.push_str("<w:tblHeader/>");
        }
        if !trpr.is_empty() {
            x.push_str(&format!("<w:trPr>{trpr}</w:trPr>"));
        }
        for (ci, content) in cells.iter().enumerate() {
            let (Some(a), Some(b)) = (def.edges.get(ci), def.edges.get(ci + 1)) else {
                x.push_str(&format!("<w:tc>{}</w:tc>", if content.is_empty() { "<w:p/>".to_string() } else { content.clone() }));
                continue;
            };
            let span = edges.iter().filter(|e| **e >= *a && **e < *b).count().max(1);
            let tcgrf = def.merges.get(ci).copied().unwrap_or(0);
            let mut tcpr = format!(r#"<w:tcW w:w="{}" w:type="dxa"/>"#, b - a);
            if span > 1 {
                tcpr.push_str(&format!(r#"<w:gridSpan w:val="{span}"/>"#));
            }
            // Tc80's tcgrf: fVertMerge (bit 5) and fVertRestart (bit 6).
            if tcgrf & 0x0020 != 0 {
                tcpr.push_str(if tcgrf & 0x0040 != 0 { r#"<w:vMerge w:val="restart"/>"# } else { "<w:vMerge/>" });
            }
            if let Some(b) = def.cell_borders.get(ci) {
                let xml: String = ["top", "left", "bottom", "right"].iter().zip(b).filter_map(|(side, b)| b.as_ref().map(|b| format!("<w:{side} {b}/>"))).collect();
                if !xml.is_empty() {
                    tcpr.push_str(&format!("<w:tcBorders>{xml}</w:tcBorders>"));
                }
            }
            if let Some(Some(shd)) = def.cell_shading.get(ci) {
                tcpr.push_str(&format!("<w:shd {shd}/>"));
            }
            // Tc80's tcgrf: textFlow (bits 2-4, [MS-DOC] TextFlow): 1 top to bottom, 3 bottom to top, 4 and 5 the same with
            // the East Asian characters upright (V).
            let flow = match (tcgrf >> 2) & 7 {
                1 => Some("tbRl"),
                3 => Some("btLr"),
                4 => Some("lrTbV"),
                5 => Some("tbRlV"),
                _ => None,
            };
            if let Some(flow) = flow {
                tcpr.push_str(&format!(r#"<w:textDirection w:val="{flow}"/>"#));
            }
            match def.cell_valign.get(ci) {
                Some(1) => tcpr.push_str(r#"<w:vAlign w:val="center"/>"#),
                Some(2) => tcpr.push_str(r#"<w:vAlign w:val="bottom"/>"#),
                _ => {}
            }
            let body = if content.is_empty() { "<w:p/>".to_string() } else { content.clone() };
            x.push_str(&format!("<w:tc><w:tcPr>{tcpr}</w:tcPr>{body}</w:tc>"));
        }
        x.push_str("</w:tr>");
    }
    x.push_str("</w:tbl>");
    x
}

fn styles_xml(styles: &Styles, fonts: &[String]) -> String {
    let font = |i: u16| fonts.get(i as usize).map(|f| esc(f)).unwrap_or_default();
    let [a, fe, other] = styles.default_fonts;
    let mut x = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="{}" w:eastAsia="{}" w:hAnsi="{}" w:cs="{}"/><w:sz w:val="20"/><w:szCs w:val="20"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults>"#,
        font(a),
        font(fe),
        font(other),
        font(other),
    );
    for (istd, s) in styles.styles.iter().enumerate() {
        let Some(s) = s else { continue };
        let kind = match s.kind {
            1 => "paragraph",
            2 => "character",
            3 => "table",
            4 => "numbering",
            _ => continue,
        };
        let default = matches!(s.sti, 0 | 65 | 105 | 107);
        let name = builtin_name(s.sti).map(str::to_string).unwrap_or_else(|| s.name.clone());
        x.push_str(&format!(r#"<w:style w:type="{kind}"{} w:styleId="S{istd}"><w:name w:val="{}"/>"#, if default { r#" w:default="1""# } else { "" }, esc(&name)));
        if s.base != 0x0FFF && styles.get(s.base).is_some() {
            x.push_str(&format!(r#"<w:basedOn w:val="S{}"/>"#, s.base));
        }
        if s.next != 0x0FFF && styles.get(s.next).is_some() && s.kind == 1 {
            x.push_str(&format!(r#"<w:next w:val="S{}"/>"#, s.next));
        }
        if s.kind == 1 || s.kind == 3 {
            let mut pp = ParaProps::default();
            apply_pap(&mut pp, &s.papx, styles);
            pp.props.items.retain(|(_, n, _)| *n != "pStyle");
            let ppr = pp.xml();
            if !ppr.is_empty() {
                x.push_str(&format!("<w:pPr>{ppr}</w:pPr>"));
            }
        }
        let mut rp = RunProps::default();
        let base = s.base;
        apply_chp(&mut rp, &s.chpx, fonts, styles, &|name| base != 0x0FFF && styles.toggle_on(base, name, fonts));
        rp.props.items.retain(|(_, n, _)| *n != "rStyle");
        let rpr = rp.xml();
        if !rpr.is_empty() {
            x.push_str(&format!("<w:rPr>{rpr}</w:rPr>"));
        }
        x.push_str("</w:style>");
    }
    x.push_str("</w:styles>");
    x
}

/// The namespaces of the document and its headers.
const NAMESPACES: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup""#;

fn package(document: &str, styles: &str, settings: &str, numbering: Option<&str>, extra: &[(String, String)], media: &[(String, Vec<u8>)]) -> Result<Vec<u8>, Error> {
    let mut media_rels = String::new();
    let mut media_ct = String::new();
    for (k, (name, _)) in media.iter().enumerate() {
        media_rels.push_str(&format!(r#"<Relationship Id="rIdImg{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/{name}"/>"#, k + 1));
    }
    for ext in ["emf", "wmf", "png", "jpeg", "bmp", "tiff", "pct"] {
        if media.iter().any(|(n, _)| n.ends_with(ext)) {
            let ct = match ext {
                "emf" => "image/x-emf",
                "wmf" => "image/x-wmf",
                "png" => "image/png",
                "jpeg" => "image/jpeg",
                "bmp" => "image/bmp",
                "tiff" => "image/tiff",
                _ => "image/pict",
            };
            media_ct.push_str(&format!(r#"<Default Extension="{ext}" ContentType="{ct}"/>"#));
        }
    }
    let mut extra_ct = String::new();
    let mut extra_rels = String::new();
    for (k, (name, _)) in extra.iter().enumerate() {
        let kind = if name.starts_with("footer") {
            "footer"
        } else if name.starts_with("footnotes") {
            "footnotes"
        } else {
            "header"
        };
        extra_ct.push_str(&format!(r#"<Override PartName="/word/{name}" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.{kind}+xml"/>"#));
        extra_rels.push_str(&format!(r#"<Relationship Id="rIdH{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}" Target="{name}"/>"#, k + 1));
    }
    let numbering_ct = if numbering.is_some() { r#"<Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>"# } else { "" };
    let numbering_rel = if numbering.is_some() { r#"<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>"# } else { "" };
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/>{N}</Types>"#.replace("{N}", &(numbering_ct.to_string() + &extra_ct + &media_ct));
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>{N}</Relationships>"#.replace("{N}", &(numbering_rel.to_string() + &extra_rels + &media_rels));
    let mut writer = Writer::new();
    let mut parts = vec![
        ("[Content_Types].xml", ct.as_str()),
        ("_rels/.rels", rels),
        ("word/_rels/document.xml.rels", doc_rels.as_str()),
        ("word/document.xml", document),
        ("word/styles.xml", styles),
        ("word/settings.xml", settings),
    ];
    if let Some(n) = numbering {
        parts.push(("word/numbering.xml", n));
    }
    let names: Vec<String> = extra.iter().map(|(name, _)| format!("word/{name}")).collect();
    for ((_, xml), name) in extra.iter().zip(&names) {
        parts.push((name.as_str(), xml.as_str()));
    }
    for (name, content) in parts {
        writer.add(name, EntryMeta::new(), content.as_bytes()).map_err(|e| Error::new(e.to_string()))?;
    }
    for (name, bytes) in media {
        writer.add(&format!("word/media/{name}"), EntryMeta::new(), bytes).map_err(|e| Error::new(e.to_string()))?;
    }
    writer.finish().map_err(|e| Error::new(e.to_string()))
}

#[cfg(test)]
mod tests {
    /// A group in a group (`wpg:grpSp` in `wpg:wgp`) is read with its shapes placed through both groups' child spaces,
    /// and kept as a group in the outer one.
    #[test]
    fn a_group_in_a_group_keeps_its_shapes_and_its_place() {
        use super::NAMESPACES;
        let rect = |id: u32, x: i64, y: i64, cx: i64, cy: i64| {
            format!(r#"<wps:wsp><wps:cNvPr id="{id}" name="r{id}"/><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:bodyPr/></wps:wsp>"#)
        };
        // Outer: 2,000,000 x 1,000,000 EMU at (100,000, 200,000) from the column and paragraph, child space 0..200 x 0..100.
        // Inner group at (100, 0) 100 x 100 of the outer's space, child space 1000..2000 x 1000..2000, with a picture in it.
        let picture = r#"<pic:pic><pic:nvPicPr><pic:cNvPr id="8" name="p8" descr="a dot"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdImg1"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="1000" y="1500"/><a:ext cx="500" cy="500"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>"#;
        let inner = format!(
            r#"<wpg:grpSp><wpg:cNvPr id="5" name="inner"/><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="100" y="0"/><a:ext cx="100" cy="100"/><a:chOff x="1000" y="1000"/><a:chExt cx="1000" cy="1000"/></a:xfrm></wpg:grpSpPr>{}{}{picture}</wpg:grpSp>"#,
            rect(6, 1000, 1000, 500, 500),
            rect(7, 1500, 1500, 500, 500)
        );
        let body = format!(
            r#"<w:p><w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>100000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>200000</wp:posOffset></wp:positionV><wp:extent cx="2000000" cy="1000000"/><wp:wrapNone/><wp:docPr id="3" name="outer"/><wp:cNvGraphicFramePr/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="2000000" cy="1000000"/><a:chOff x="0" y="0"/><a:chExt cx="200" cy="100"/></a:xfrm></wpg:grpSpPr>{}{inner}</wpg:wgp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#,
            rect(4, 0, 0, 100, 100)
        );
        let document = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NAMESPACES}><w:body>{body}<w:sectPr/></w:body></w:document>"#);
        let styles = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles {NAMESPACES}/>"#);
        let settings = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings {NAMESPACES}/>"#);
        let data = super::package(&document, &styles, &settings, None, &[], &[("image1.png".to_string(), vec![0x89, b'P', b'N', b'G'])]).unwrap();
        let p = crate::docx::read(&data).unwrap();
        let flow = p.flow.as_deref().unwrap();
        let shapes: Vec<_> = flow
            .body
            .iter()
            .filter_map(|b| match b {
                ibis_doc_model::Block::Paragraph(p) => Some(p),
                _ => None,
            })
            .flat_map(|p| &p.runs)
            .filter_map(|r| match &r.kind {
                ibis_doc_model::FlowRunKind::Shape(s) => Some((s.id.0, s.frame)),
                _ => None,
            })
            .collect();
        let frames: Vec<_> = shapes.iter().map(|(id, f)| (*id, f.x, f.y, f.width, f.height)).collect();
        // r4: outer child (0, 0) 100 x 100 -> 1,000,000 x 1,000,000 at the anchor.
        // r6: inner child (1000, 1000) 500 x 500 -> inner (100, 0) 50 x 50 -> outer (1,100,000, 200,000) 500,000 x 500,000.
        // r7: inner child (1500, 1500) -> inner (150, 50) -> (1,600,000, 700,000).
        assert_eq!(
            frames,
            vec![
                (4, 100_000, 200_000, 1_000_000, 1_000_000),
                (6, 1_100_000, 200_000, 500_000, 500_000),
                (7, 1_600_000, 700_000, 500_000, 500_000),
                // p8: inner child (1000, 1500) 500 x 500 -> (1,100,000, 700,000) 500,000 x 500,000.
                (8, 1_100_000, 700_000, 500_000, 500_000)
            ]
        );
        let [group] = flow.shape_groups.as_slice() else { panic!("one group: {:?}", flow.shape_groups) };
        use ibis_doc_model::{GroupMember, ShapeId};
        assert_eq!(group.children.len(), 2);
        assert_eq!(group.children[0], GroupMember::Shape(ShapeId(4)));
        let GroupMember::Group(inner) = &group.children[1] else { panic!("the inner group: {:?}", group.children[1]) };
        assert_eq!(inner.name, "inner");
        assert_eq!(inner.children, vec![GroupMember::Shape(ShapeId(6)), GroupMember::Shape(ShapeId(7)), GroupMember::Shape(ShapeId(8))]);
        let picture = flow
            .body
            .iter()
            .filter_map(|b| match b {
                ibis_doc_model::Block::Paragraph(p) => Some(p),
                _ => None,
            })
            .flat_map(|p| &p.runs)
            .find_map(|r| match &r.kind {
                ibis_doc_model::FlowRunKind::Shape(s) if s.id.0 == 8 => Some(s),
                _ => None,
            })
            .unwrap();
        let ibis_doc_model::ShapeKind::Picture(picture) = &picture.kind else { panic!("p8 is a picture: {:?}", picture.kind) };
        assert_eq!(picture.alt_text.description, "a dot");
    }

    /// Writes the package a `.doc` reads into, to look at: `IBIS_DOC_IN=a.doc IBIS_DOC_OUT=a.docx cargo test -p ibis-pptx
    /// --lib msdoc::tests::write_package -- --ignored`.
    #[test]
    #[ignore]
    fn write_package() {
        let (Ok(input), Ok(output)) = (std::env::var("IBIS_DOC_IN"), std::env::var("IBIS_DOC_OUT")) else { return };
        let data = std::fs::read(input).expect("the .doc");
        let docx = super::to_docx(&data).expect("a Word 97-2003 document");
        std::fs::write(output, docx).expect("written");
    }

    /// The document properties (`Dop`, [MS-DOC] 2.7.1) of each `.doc` in a folder, as hex, one file a line:
    /// `IBIS_DOC_DIR=dir cargo test -p ibis-pptx --lib msdoc::tests::dop_census -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn dop_census() {
        let Ok(dir) = std::env::var("IBIS_DOC_DIR") else { return };
        let mut names: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "doc")).collect();
        names.sort();
        for path in names {
            let data = std::fs::read(&path).unwrap();
            let Ok(wd) = ibis_ooxml::cfb::stream(&data, "WordDocument") else { continue };
            let flags = super::u16_at(&wd, 10).unwrap();
            let Ok(table) = ibis_ooxml::cfb::stream(&data, if flags & 0x0200 != 0 { "1Table" } else { "0Table" }) else { continue };
            let (fc, lcb) = (super::u32_at(&wd, 154 + 31 * 8).unwrap() as usize, super::u32_at(&wd, 158 + 31 * 8).unwrap() as usize);
            let dop = table.get(fc..fc + lcb).unwrap_or(&[]);
            let hex: String = dop.iter().map(|b| format!("{b:02x}")).collect();
            println!("DOP {} {}", path.file_stem().unwrap().to_string_lossy(), hex);
        }
    }

    /// The character and paragraph properties a `.doc` uses, with how often, and one operand of each:
    /// `IBIS_DOC_IN=a.doc cargo test -p ibis-pptx --lib msdoc::tests::census -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn census() {
        let Ok(input) = std::env::var("IBIS_DOC_IN") else { return };
        let data = std::fs::read(input).expect("the .doc");
        let wd = ibis_ooxml::cfb::stream(&data, "WordDocument").expect("WordDocument");
        let flags = super::u16_at(&wd, 10).unwrap();
        let table = ibis_ooxml::cfb::stream(&data, if flags & 0x0200 != 0 { "1Table" } else { "0Table" }).unwrap();
        let pair = |i: usize| (super::u32_at(&wd, 154 + i * 8).unwrap() as usize, super::u32_at(&wd, 158 + i * 8).unwrap() as usize);
        // The sections' properties.
        let (fc, lcb) = pair(6);
        let n = lcb.saturating_sub(4) / 16;
        for i in 0..n {
            let fc_sepx = super::u32_at(&table, fc + (n + 1) * 4 + i * 12 + 2).unwrap() as usize;
            let cb = super::u16_at(&wd, fc_sepx).unwrap_or(0) as usize;
            for s in super::sprms(wd.get(fc_sepx + 2..fc_sepx + 2 + cb).unwrap_or(&[])) {
                println!("sep{i} {:04X} {:02X?}", s.op, s.arg);
            }
        }
        for (name, index, para) in [("chp", 12, false), ("pap", 13, true)] {
            let (fc, lcb) = pair(index);
            let fkps = super::read_fkps(&wd, &table, &[], fc, lcb, para);
            let mut seen: std::collections::BTreeMap<u16, (usize, Vec<u8>)> = Default::default();
            for r in &fkps.runs {
                for s in super::sprms(&r.2) {
                    let e = seen.entry(s.op).or_insert((0, s.arg.to_vec()));
                    e.0 += 1;
                }
            }
            for (op, (n, arg)) in seen {
                println!("{name} {op:04X} x{n} {:02X?}", &arg[..arg.len().min(16)]);
            }
        }
    }
}
