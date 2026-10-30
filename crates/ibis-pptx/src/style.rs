// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Paragraph and character properties and their inheritance.
//!
//! A run's effective format is built up from, in order: the master's text
//! styles (`p:txStyles`, or the presentation's default for non-placeholders),
//! the master and layout placeholders' list styles, the shape's own list
//! style, the paragraph's `a:pPr` and finally the run's `a:rPr`. Each layer
//! only overrides what it states, so every field here is optional.

use ibis_doc_model::{Color, Emu, FontAlign, LineSpacing, TextAlign};
use roxmltree::Node;

use crate::theme::Colors;
use crate::xml::{attr, attr_bool, attr_i64, child, elements};

#[derive(Clone, Debug, Default)]
pub(crate) struct RunProps {
    pub(crate) size: Option<u32>,
    pub(crate) bold: Option<bool>,
    pub(crate) italic: Option<bool>,
    pub(crate) underline: Option<bool>,
    pub(crate) underline_kind: Option<ibis_doc_model::Underline>,
    pub(crate) baseline: Option<i32>,
    /// `spc`, in hundredths of a point.
    pub(crate) spacing: Option<i32>,
    pub(crate) highlight: Option<Color>,
    /// `a:ln`: `Some(None)` is no outline.
    pub(crate) outline: Option<Option<(Color, i64)>>,
    /// A gradient or pattern fill (`Some(None)`: a solid fill stated over an
    /// inherited one).
    pub(crate) fill: Option<Option<ibis_doc_model::Fill>>,
    pub(crate) color: Option<Color>,
    pub(crate) latin: Option<String>,
    pub(crate) east_asian: Option<String>,
    pub(crate) strike: Option<ibis_doc_model::Strike>,
    pub(crate) caps: Option<ibis_doc_model::Caps>,
    /// The theme's Japanese font (`<a:font script="Jpan">`) when the East
    /// Asian font is the theme's and the theme leaves it empty: PowerPoint
    /// draws Japanese in it when the run's language is Japanese (`lang`),
    /// and in ＭＳ ゴシック otherwise (the renderer's default; measured on its
    /// PDF).
    pub(crate) japanese: Option<String>,
    /// `lang`.
    pub(crate) lang: Option<String>,
    /// `kern`, in hundredths of a point.
    pub(crate) kern: Option<u32>,
    pub(crate) shadow: Option<Option<ibis_doc_model::Shadow>>,
    /// `a:effectLst/a:glow` (`Some(None)`: an effect list without one).
    pub(crate) glow: Option<Option<(ibis_doc_model::Color, i64)>>,
}

/// Added to a bullet size given in points (see `buSzPts`), to tell it from a percentage.
pub(crate) const BULLET_POINTS: u32 = 10_000_000;

impl RunProps {
    /// Override with the properties stated on `a:rPr`, `a:defRPr` or `a:endParaRPr`.
    pub(crate) fn merge(&mut self, node: Node, colors: &Colors) {
        if let Some(sz) = attr_i64(node, "sz") {
            self.size = Some(sz.max(1) as u32);
        }
        if let Some(b) = attr_bool(node, "b") {
            self.bold = Some(b);
        }
        if let Some(i) = attr_bool(node, "i") {
            self.italic = Some(i);
        }
        if let Some(u) = attr(node, "u") {
            self.underline = Some(u != "none");
            if let Some(kind) = ibis_doc_model::Underline::parse(u) {
                self.underline_kind = Some(kind);
            }
        }
        if let Some(b) = attr_i64(node, "baseline") {
            self.baseline = Some(b.clamp(-100_000, 100_000) as i32);
        }
        if let Some(lang) = attr(node, "lang") {
            self.lang = Some(lang.to_string());
        }
        if let Some(k) = attr_i64(node, "kern") {
            self.kern = Some(k.max(0) as u32);
        }
        if let Some(s) = attr_i64(node, "spc") {
            self.spacing = Some(s.clamp(-400_000, 400_000) as i32);
        }
        if let Some(v) = attr(node, "strike") {
            self.strike = Some(match v {
                "sngStrike" => ibis_doc_model::Strike::Single,
                "dblStrike" => ibis_doc_model::Strike::Double,
                _ => ibis_doc_model::Strike::None,
            });
        }
        if let Some(v) = attr(node, "cap") {
            self.caps = Some(match v {
                "small" => ibis_doc_model::Caps::Small,
                "all" => ibis_doc_model::Caps::All,
                _ => ibis_doc_model::Caps::None,
            });
        }
        for n in elements(node) {
            match n.tag_name().name() {
                "solidFill" => {
                    if let Some(c) = colors.in_parent(n, None) {
                        self.color = Some(c);
                        self.fill = Some(None);
                    }
                }
                "gradFill" => {
                    // The first colour too, for what draws in one colour.
                    let first = child(n, "gsLst").and_then(|l| elements(l).next());
                    if let Some(c) = first.and_then(|gs| colors.in_parent(gs, None)) {
                        self.color = Some(c);
                    }
                    if let Some(fill) = text_gradient(n, colors) {
                        self.fill = Some(Some(fill));
                    }
                }
                "pattFill" => {
                    let fg = child(n, "fgClr").and_then(|c| colors.in_parent(c, None)).unwrap_or(Color::BLACK);
                    let bg = child(n, "bgClr").and_then(|c| colors.in_parent(c, None)).unwrap_or(Color::WHITE);
                    let preset = attr(n, "prst").unwrap_or("pct5");
                    self.color = Some(fg);
                    if ibis_doc_model::pattern_tile(preset).is_some() {
                        self.fill = Some(Some(ibis_doc_model::Fill::Pattern { preset: preset.to_string(), foreground: fg, background: bg }));
                    }
                }
                "ln" => {
                    let color = child(n, "solidFill").and_then(|f| colors.in_parent(f, None));
                    let width = attr_i64(n, "w").unwrap_or(9_525);
                    self.outline = Some(color.filter(|_| child(n, "noFill").is_none() && width > 0).map(|c| (c, width)));
                }
                "highlight" => self.highlight = colors.in_parent(n, None).or(self.highlight.take()),
                // A hyperlink is underlined and drawn in the theme's link
                // colour, unless it keeps the text's (`ahyp:hlinkClr="tx"`).
                "hlinkClick" => {
                    self.underline = Some(true);
                    let keeps_color = n.descendants().any(|d| d.tag_name().name() == "hlinkClr" && attr(d, "val") == Some("tx"));
                    if !keeps_color && let Some(rgb) = colors.scheme("hlink") {
                        self.color = Some(Color::opaque(rgb));
                    }
                }
                "latin" => self.latin = attr(n, "typeface").and_then(|t| colors.theme().typeface(t)).or(self.latin.take()),
                "ea" => {
                    let typeface = attr(n, "typeface");
                    self.east_asian = typeface.and_then(|t| colors.theme().typeface(t)).or(self.east_asian.take());
                    if let Some(t) = typeface {
                        self.japanese = colors.theme().empty_east_asian_japanese(t);
                    }
                }
                "outerShdw" => {
                    let blur = attr_i64(n, "blurRad").unwrap_or(0);
                    let distance = attr_i64(n, "dist").unwrap_or(0);
                    let direction = attr_i64(n, "dir").unwrap_or(0) as i32;
                    let sx = attr_i64(n, "sx").map(|v| v as i32).unwrap_or(100_000);
                    let sy = attr_i64(n, "sy").map(|v| v as i32).unwrap_or(100_000);
                    let kx = attr_i64(n, "kx").map(|v| v as i32).unwrap_or(0);
                    let ky = attr_i64(n, "ky").map(|v| v as i32).unwrap_or(0);
                    let align = attr(n, "algn").map(ibis_doc_model::TileAlign::parse).unwrap_or(ibis_doc_model::TileAlign::Bottom);
                    let rot_with_shape = attr(n, "rotWithShape").map(|v| v != "0").unwrap_or(true);
                    if let Some(color) = colors.in_parent(n, None) {
                        self.shadow = Some(Some(ibis_doc_model::Shadow {
                            color,
                            blur,
                            distance,
                            direction,
                            sx,
                            sy,
                            kx,
                            ky,
                            align,
                            rot_with_shape,
                        }));
                    }
                }
                "effectLst" => {
                    // An effect list replaces the inherited one: without a shadow in it, there is none.
                    self.glow = Some(child(n, "glow").and_then(|g| Some((colors.in_parent(g, None)?, attr_i64(g, "rad").unwrap_or(0)))).filter(|(_, rad)| *rad > 0));
                    if let Some(s) = child(n, "outerShdw") {
                        let blur = attr_i64(s, "blurRad").unwrap_or(0);
                        let distance = attr_i64(s, "dist").unwrap_or(0);
                        let direction = attr_i64(s, "dir").unwrap_or(0) as i32;
                        let sx = attr_i64(s, "sx").map(|v| v as i32).unwrap_or(100_000);
                        let sy = attr_i64(s, "sy").map(|v| v as i32).unwrap_or(100_000);
                        let kx = attr_i64(s, "kx").map(|v| v as i32).unwrap_or(0);
                        let ky = attr_i64(s, "ky").map(|v| v as i32).unwrap_or(0);
                        let align = attr(s, "algn").map(ibis_doc_model::TileAlign::parse).unwrap_or(ibis_doc_model::TileAlign::Bottom);
                        let rot_with_shape = attr(s, "rotWithShape").map(|v| v != "0").unwrap_or(true);
                        if let Some(color) = colors.in_parent(s, None) {
                            self.shadow = Some(Some(ibis_doc_model::Shadow {
                                color,
                                blur,
                                distance,
                                direction,
                                sx,
                                sy,
                                kx,
                                ky,
                                align,
                                rot_with_shape,
                            }));
                        }
                    } else if let Some(s) = child(n, "innerShdw") {
                        let blur = attr_i64(s, "blurRad").unwrap_or(0);
                        let distance = attr_i64(s, "dist").unwrap_or(0);
                        let direction = attr_i64(s, "dir").unwrap_or(0) as i32;
                        if let Some(color) = colors.in_parent(s, None) {
                            // `sx = 0` marks an inner shadow (`a:innerShdw`, which has no `sx` attribute in OOXML).
                            self.shadow = Some(Some(ibis_doc_model::Shadow {
                                color,
                                blur,
                                distance,
                                direction,
                                sx: 0,
                                sy: 100_000,
                                kx: 0,
                                ky: 0,
                                align: ibis_doc_model::TileAlign::Bottom,
                                rot_with_shape: true,
                            }));
                        }
                    } else {
                        self.shadow = Some(None);
                    }
                }
                _ => {}
            }
        }
    }

    /// The font of the other script than `font_for` picks for `text`.
    pub(crate) fn fallback_for(&self, text: &str) -> Option<String> {
        if text.chars().any(is_east_asian) { self.latin.clone() } else { self.east_asian.clone() }
    }

    /// The font for `text`: the East Asian font when the text has East Asian
    /// characters, otherwise the Latin one.
    pub(crate) fn font_for(&self, text: &str) -> Option<String> {
        let east_asian = text.chars().any(is_east_asian);
        let japanese = self.lang.as_deref().is_some_and(|l| l.starts_with("ja"));
        if east_asian && japanese && self.japanese.is_some() {
            self.japanese.clone()
        } else if east_asian {
            self.east_asian.clone().or_else(|| self.latin.clone())
        } else {
            self.latin.clone().or_else(|| self.east_asian.clone())
        }
    }
}

/// A text's `a:gradFill`, as a shape's (`Fill::Linear` or `Fill::Path`).
fn text_gradient(node: Node, colors: &Colors) -> Option<ibis_doc_model::Fill> {
    use ibis_doc_model::{GradientPath, GradientStop};
    let mut stops: Vec<GradientStop> = child(node, "gsLst")
        .into_iter()
        .flat_map(|l| crate::xml::children(l, "gs"))
        .filter_map(|gs| Some(GradientStop { position: attr_i64(gs, "pos").unwrap_or(0).clamp(0, 100_000) as u32, color: colors.in_parent(gs, None)? }))
        .collect();
    stops.sort_by_key(|s| s.position);
    if stops.is_empty() {
        return None;
    }
    if let Some(path) = child(node, "path") {
        let kind = attr(path, "path").and_then(GradientPath::parse).unwrap_or(GradientPath::Circle);
        let rect = child(path, "fillToRect");
        let focus = ["l", "t", "r", "b"].map(|k| rect.and_then(|r| attr_i64(r, k)).unwrap_or(0) as i32);
        return Some(ibis_doc_model::Fill::Path { path: kind, focus, stops });
    }
    let lin = child(node, "lin");
    let angle = lin.and_then(|l| attr_i64(l, "ang")).unwrap_or(5_400_000) as i32;
    let scaled = lin.and_then(|l| attr_bool(l, "scaled")).unwrap_or(false);
    Some(ibis_doc_model::Fill::Linear { angle, scaled, stops })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BulletProps {
    None,
    Char(String),
    AutoNumber { scheme: String, start_at: u32 },
    /// A picture, by relationship id.
    Picture(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Spacing {
    /// Hundredths of a point.
    Points(u32),
    /// Of the font size, in 100 000ths.
    Percent(u32),
}

/// Whether PowerPoint draws `c` with the East Asian font.
pub(crate) fn is_east_asian(c: char) -> bool {
    ibis_doc_model::is_east_asian(c)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ParaProps {
    pub(crate) align: Option<TextAlign>,
    pub(crate) margin_left: Option<Emu>,
    pub(crate) indent: Option<Emu>,
    pub(crate) line_spacing: Option<LineSpacing>,
    pub(crate) font_align: Option<FontAlign>,
    pub(crate) space_before: Option<Spacing>,
    pub(crate) space_after: Option<Spacing>,
    pub(crate) bullet: Option<BulletProps>,
    /// `a:buClr`; `None` follows the text (`a:buClrTx` or unset).
    pub(crate) bullet_color: Option<Option<Color>>,
    /// `a:buFont`. It comes before `a:buChar` in `a:pPr`, and may come from
    /// a different level of the inheritance than the character.
    pub(crate) bullet_font: Option<String>,
    /// `a:buSzPct`, in 100 000ths; `None` follows the text (`a:buSzTx`).
    pub(crate) bullet_size: Option<Option<u32>>,
    pub(crate) tabs: Option<Vec<ibis_doc_model::TextTabStop>>,
    pub(crate) default_tab: Option<Emu>,
    pub(crate) run: RunProps,
}

impl ParaProps {
    /// Override with the properties stated on `a:pPr` or `a:lvlNpPr`.
    pub(crate) fn merge(&mut self, node: Node, colors: &Colors) {
        if let Some(algn) = attr(node, "algn") {
            self.align = Some(match algn {
                "ctr" => TextAlign::Center,
                "r" => TextAlign::Right,
                "just" | "justLow" => TextAlign::Justify,
                "dist" | "thaiDist" => TextAlign::Distributed,
                _ => TextAlign::Left,
            });
        }
        if let Some(v) = attr(node, "fontAlgn") {
            self.font_align = Some(match v {
                "auto" => FontAlign::Auto,
                "t" => FontAlign::Top,
                "ctr" => FontAlign::Center,
                "b" => FontAlign::Bottom,
                _ => FontAlign::Baseline,
            });
        }
        if let Some(v) = attr_i64(node, "marL") {
            self.margin_left = Some(v);
        }
        if let Some(v) = attr_i64(node, "indent") {
            self.indent = Some(v);
        }
        if let Some(v) = attr_i64(node, "defTabSz") {
            self.default_tab = Some(v.max(0));
        }
        for n in elements(node) {
            match n.tag_name().name() {
                "lnSpc" => {
                    if let Some(s) = spacing(n) {
                        self.line_spacing = Some(match s {
                            Spacing::Percent(p) => LineSpacing::Percent(p),
                            Spacing::Points(p) => LineSpacing::Points(p),
                        });
                    }
                }
                "spcBef" => self.space_before = spacing(n).or(self.space_before.take()),
                "spcAft" => self.space_after = spacing(n).or(self.space_after.take()),
                "buNone" => self.bullet = Some(BulletProps::None),
                "buBlip" => {
                    if let Some(rid) = child(n, "blip").and_then(|b| attr(b, "embed")) {
                        self.bullet = Some(BulletProps::Picture(rid.to_string()));
                    }
                }
                "buChar" => self.bullet = Some(BulletProps::Char(attr(n, "char").unwrap_or("•").to_string())),
                "buAutoNum" => {
                    self.bullet = Some(BulletProps::AutoNumber {
                        scheme: attr(n, "type").unwrap_or("arabicPeriod").to_string(),
                        start_at: attr_i64(n, "startAt").unwrap_or(1).max(1) as u32,
                    })
                }
                "buFont" => self.bullet_font = attr(n, "typeface").map(str::to_string),
                "buFontTx" => self.bullet_font = None,
                "buSzPct" => self.bullet_size = Some(attr_i64(n, "val").map(|v| v.clamp(25_000, 400_000) as u32)),
                // A size in points (hundredths) is kept as `BULLET_POINTS + value`: it becomes a percentage once the
                // text's size is known.
                "buSzPts" => self.bullet_size = Some(attr_i64(n, "val").map(|v| BULLET_POINTS + v.clamp(100, 400_000) as u32)),
                "buSzTx" => self.bullet_size = Some(None),
                "buClr" => self.bullet_color = Some(colors.in_parent(n, None)),
                "buClrTx" => self.bullet_color = Some(None),
                "tabLst" => {
                    let mut tabs = Vec::new();
                    for tab in elements(n) {
                        if tab.tag_name().name() == "tab" {
                            if let Some(pos) = attr_i64(tab, "pos") {
                                let align = match attr(tab, "algn") {
                                    Some("c") => ibis_doc_model::TextTabAlign::Center,
                                    Some("r") => ibis_doc_model::TextTabAlign::Right,
                                    Some("dec") => ibis_doc_model::TextTabAlign::Decimal,
                                    _ => ibis_doc_model::TextTabAlign::Left,
                                };
                                tabs.push(ibis_doc_model::TextTabStop { position: pos, align });
                            }
                        }
                    }
                    self.tabs = Some(tabs);
                }
                "defRPr" => self.run.merge(n, colors),
                _ => {}
            }
        }
    }
}

fn spacing(node: Node) -> Option<Spacing> {
    let n = elements(node).next()?;
    let val = attr_i64(n, "val")?.max(0) as u32;
    match n.tag_name().name() {
        "spcPts" => Some(Spacing::Points(val)),
        "spcPct" => Some(Spacing::Percent(val)),
        _ => None,
    }
}

/// Paragraph properties for outline levels 1 to 9.
#[derive(Clone, Debug, Default)]
pub(crate) struct ListStyle {
    pub(crate) levels: [ParaProps; 9],
}

impl ListStyle {
    /// Override with a list style element (`p:titleStyle`, `p:defaultTextStyle`, …).
    pub(crate) fn merge(&mut self, node: Node, colors: &Colors) {
        self.merge_levels(node, colors, true);
    }

    /// Override with a shape's `a:lstStyle`: its `a:defPPr` is not used
    /// (PowerPoint draws its text as without it: the coverage suite PTXS).
    pub(crate) fn merge_shape(&mut self, node: Node, colors: &Colors) {
        self.merge_levels(node, colors, false);
    }

    fn merge_levels(&mut self, node: Node, colors: &Colors, with_default: bool) {
        // `a:defPPr` applies to every level, before the level's own properties.
        if let Some(def) = child(node, "defPPr").filter(|_| with_default) {
            for level in &mut self.levels {
                level.merge(def, colors);
            }
        }
        for (i, level) in self.levels.iter_mut().enumerate() {
            if let Some(n) = child(node, &format!("lvl{}pPr", i + 1)) {
                level.merge(n, colors);
            }
        }
    }
}

/// Map a symbol-font bullet character to the Unicode character it shows.
/// How big a bullet from `font` is drawn, relative to its text: the
/// characters standing in for Wingdings and Symbol glyphs are larger in
/// ordinary fonts than the glyphs they replace (measured against
/// PowerPoint's PDF output: Wingdings' circle is about 0.6 of the text size).
/// Whether `font` is a symbol typeface (its codes draw signs, not letters).
pub(crate) fn is_symbol_font(font: &str) -> bool {
    font.contains("Wingdings") || font.contains("Symbol")
}

pub(crate) fn bullet_scale(font: Option<&str>) -> f32 {
    if font.is_some_and(is_symbol_font) { 0.75 } else { 1.0 }
}

pub(crate) fn bullet_glyph(text: &str, font: Option<&str>) -> String {
    let symbol_font = font.is_some_and(|f| f.contains("Wingdings") || f.contains("Symbol"));
    if !symbol_font {
        return text.to_string();
    }
    // A symbol font's code points are also written in the private use range U+F020..=U+F0FF (U+F000 plus the code):
    // PowerPoint saves a bullet drawn with Wingdings as U+F0FC for the code 0xFC (ü).
    let code = {
        let mut chars = text.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if ('\u{f020}'..='\u{f0ff}').contains(&c) => char::from(u32::from(c) as u8).to_string(),
            _ => text.to_string(),
        }
    };
    match code.as_str() {
        "l" | "Ÿ" | "·" => "●",
        "§" | "n" => "■",
        "p" | "q" => "❑",
        "v" => "❖",
        "Ø" => "➢",
        // Text presentation (U+FE0E): without it Windows draws the heavy check mark as a colour emoji, which ignores the bullet's colour.
        "ü" => "✔\u{fe0e}",
        "ð" | "è" => "➔",
        _ => "•",
    }
    .to_string()
}

/// `n` in Chinese numerals by place (十一, 二十, 百五).
fn kanji_by_place(n: u32) -> String {
    const DIGITS: [char; 10] = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
    if n == 0 {
        return "〇".into();
    }
    let mut s = String::new();
    for (unit, name) in [(1000, "千"), (100, "百"), (10, "十")] {
        let d = n / unit % 10;
        if d > 0 {
            if d > 1 {
                s.push(DIGITS[d as usize]);
            }
            s.push_str(name);
        }
    }
    if n % 10 > 0 {
        s.push(DIGITS[(n % 10) as usize]);
    }
    s
}

/// `n` in kanji digit by digit (一○ for 10, as PowerPoint draws them).
fn kanji_by_digit(n: u32) -> String {
    const DIGITS: [char; 10] = ['○', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
    n.to_string().chars().map(|c| DIGITS[c.to_digit(10).unwrap_or(0) as usize]).collect()
}

/// `n` in the digits starting at `zero` (full-width, Thai, Devanagari).
fn digits_from(n: u32, zero: u32) -> String {
    n.to_string().chars().filter_map(|c| char::from_u32(zero + c.to_digit(10).unwrap_or(0))).collect()
}

/// The `n`th of `letters` (from 1), repeated past the end.
fn nth_letter(letters: &[&str], n: u32) -> String {
    if letters.is_empty() || n == 0 {
        return String::new();
    }
    let i = (n - 1) as usize;
    letters[i % letters.len()].repeat(i / letters.len() + 1)
}

const ARABIC_ALPHABET: [&str; 28] = ["أ", "ب", "ت", "ث", "ج", "ح", "خ", "د", "ذ", "ر", "ز", "س", "ش", "ص", "ض", "ط", "ظ", "ع", "غ", "ف", "ق", "ك", "ل", "م", "ن", "ه", "و", "ي"];
const ARABIC_ABJAD: [&str; 28] = ["أ", "ب", "ج", "د", "ه", "و", "ز", "ح", "ط", "ي", "ك", "ل", "م", "ن", "س", "ع", "ف", "ص", "ق", "ر", "ش", "ت", "ث", "خ", "ذ", "ض", "ظ", "غ"];
const HEBREW: [&str; 22] = ["א", "ב", "ג", "ד", "ה", "ו", "ז", "ח", "ט", "י", "כ", "ל", "מ", "נ", "ס", "ע", "פ", "צ", "ק", "ר", "ש", "ת"];
/// Thai consonants in PowerPoint's order (without ฃ, ฅ and ฆ).
const THAI_ALPHA: [&str; 41] = [
    "ก", "ข", "ค", "ง", "จ", "ฉ", "ช", "ซ", "ฌ", "ญ", "ฎ", "ฏ", "ฐ", "ฑ", "ฒ", "ณ", "ด", "ต", "ถ", "ท", "ธ", "น", "บ", "ป", "ผ", "ฝ", "พ",
    "ฟ", "ภ", "ม", "ย", "ร", "ล", "ว", "ศ", "ษ", "ส", "ห", "ฬ", "อ", "ฮ",
];

/// The label of an automatically numbered bullet (`ST_TextAutonumberScheme`;
/// the East Asian, Arabic, Hebrew, Thai and Hindi ones as PowerPoint draws
/// them: the coverage suite PTXT).
pub(crate) fn auto_number(scheme: &str, n: u32) -> String {
    let consecutive = |first: u32| char::from_u32(first + n.saturating_sub(1)).map(String::from).unwrap_or_default();
    match scheme {
        "arabicDbPeriod" => return format!("{}．", digits_from(n, 0xFF10)),
        "arabicDbPlain" => return digits_from(n, 0xFF10),
        "ea1ChsPeriod" | "ea1ChtPeriod" => return format!("{}.", kanji_by_place(n)),
        "ea1ChsPlain" | "ea1ChtPlain" => return kanji_by_place(n),
        "ea1JpnChsDbPeriod" => return format!("{}．", kanji_by_digit(n)),
        "ea1JpnKorPeriod" => return format!("{}.", kanji_by_digit(n)),
        "ea1JpnKorPlain" => return kanji_by_digit(n),
        "arabic1Minus" => return format!("{}-", nth_letter(&ARABIC_ALPHABET, n)),
        "arabic2Minus" => return format!("{}-", nth_letter(&ARABIC_ABJAD, n)),
        "hebrew2Minus" => return format!("{}-", nth_letter(&HEBREW, n)),
        "thaiAlphaPeriod" => return format!("{}.", nth_letter(&THAI_ALPHA, n)),
        "thaiAlphaParenR" => return format!("{})", nth_letter(&THAI_ALPHA, n)),
        "thaiAlphaParenBoth" => return format!("({})", nth_letter(&THAI_ALPHA, n)),
        "thaiNumPeriod" => return format!("{}.", digits_from(n, 0x0E50)),
        "thaiNumParenR" => return format!("{})", digits_from(n, 0x0E50)),
        "thaiNumParenBoth" => return format!("({})", digits_from(n, 0x0E50)),
        "hindiAlphaPeriod" if (1..=11).contains(&n) => return format!("{}.", consecutive(0x0905)),
        "hindiAlpha1Period" if (1..=36).contains(&n) => return format!("{}.", consecutive(0x0915)),
        "hindiNumPeriod" => return format!("{}.", digits_from(n, 0x0966)),
        "hindiNumParenR" => return format!("{})", digits_from(n, 0x0966)),
        "circleNumWdBlackPlain" if (1..=10).contains(&n) => return consecutive(0x2776),
        "circleNumWdBlackPlain" if (11..=20).contains(&n) => return char::from_u32(0x24EB + n - 11).map(String::from).unwrap_or_default(),
        "circleNumWdWhitePlain" if (1..=10).contains(&n) => return consecutive(0x2780),
        _ => {}
    }
    let alpha = |upper: bool| {
        let mut s = String::new();
        let mut k = n;
        while k > 0 {
            let c = ((k - 1) % 26) as u8 + if upper { b'A' } else { b'a' };
            s.insert(0, c as char);
            k = (k - 1) / 26;
        }
        s
    };
    let roman = |upper: bool| {
        let table = [
            (1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"),
            (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i"),
        ];
        let mut s = String::new();
        let mut k = n;
        for (value, digits) in table {
            while k >= value {
                s.push_str(digits);
                k -= value;
            }
        }
        if upper { s.to_uppercase() } else { s }
    };
    let (body, suffix) = if let Some(base) = scheme.strip_suffix("Period") {
        (base, ".")
    } else if let Some(base) = scheme.strip_suffix("ParenR") {
        (base, ")")
    } else if let Some(base) = scheme.strip_suffix("ParenBoth") {
        return format!("({})", auto_number(&format!("{base}Plain"), n));
    } else if let Some(base) = scheme.strip_suffix("Plain") {
        (base, "")
    } else {
        ("arabic", ".")
    };
    let label = match body {
        "alphaLc" => alpha(false),
        "alphaUc" => alpha(true),
        "romanLc" => roman(false),
        "romanUc" => roman(true),
        "circleNumDb" | "circleNumWd" if (1..=20).contains(&n) => {
            return char::from_u32(0x2460 + n - 1).map(String::from).unwrap_or_default();
        }
        _ => n.to_string(),
    };
    format!("{label}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_number_schemes() {
        assert_eq!(auto_number("arabicPeriod", 3), "3.");
        // As PowerPoint draws them (the coverage suite PTXT).
        assert_eq!(auto_number("ea1JpnKorPeriod", 1), "一.");
        assert_eq!(auto_number("ea1JpnKorPeriod", 10), "一○.");
        assert_eq!(auto_number("ea1JpnChsDbPeriod", 11), "一一．");
        assert_eq!(auto_number("ea1ChsPeriod", 11), "十一.");
        assert_eq!(auto_number("ea1ChtPlain", 9), "九");
        assert_eq!(auto_number("arabicDbPeriod", 10), "１０．");
        assert_eq!(auto_number("arabic1Minus", 9), "ذ-");
        assert_eq!(auto_number("arabic2Minus", 11), "ك-");
        assert_eq!(auto_number("hebrew2Minus", 10), "י-");
        assert_eq!(auto_number("thaiAlphaParenBoth", 9), "(ฌ)");
        assert_eq!(auto_number("thaiNumPeriod", 10), "๑๐.");
        assert_eq!(auto_number("hindiAlphaPeriod", 9), "ऍ.");
        assert_eq!(auto_number("hindiAlpha1Period", 11), "ट.");
        assert_eq!(auto_number("hindiNumParenR", 9), "९)");
        assert_eq!(auto_number("alphaLcParenR", 2), "b)");
        assert_eq!(auto_number("romanUcPeriod", 4), "IV.");
        assert_eq!(auto_number("arabicParenBoth", 5), "(5)");
        assert_eq!(auto_number("circleNumDbPlain", 1), "①");
    }

    #[test]
    fn bullet_font_before_char_is_kept() {
        let doc = roxmltree::Document::parse(
            r#"<pPr><buFont typeface="Wingdings"/><buChar char="l"/></pPr>"#,
        )
        .unwrap();
        let theme = crate::theme::Theme::default();
        let colors = Colors::new(&theme, None, &[]);
        let mut props = ParaProps::default();
        props.merge(doc.root_element(), &colors);
        let Some(BulletProps::Char(text)) = &props.bullet else { panic!("no bullet") };
        assert_eq!(bullet_glyph(text, props.bullet_font.as_deref()), "●");
    }

    #[test]
    fn wingdings_bullets_map_to_unicode() {
        assert_eq!(bullet_glyph("§", Some("Wingdings")), "■");
        assert_eq!(bullet_glyph("•", Some("Arial")), "•");
    }

    /// PowerPoint writes a Wingdings bullet's code in the private use range (U+F000 + the code): U+F0FC is ü, the check
    /// mark, as U+F0A7 is §, the square.
    #[test]
    fn wingdings_bullets_in_the_private_use_range_map_like_their_codes() {
        assert_eq!(bullet_glyph("\u{f0fc}", Some("Wingdings")), "✔\u{fe0e}");
        assert_eq!(bullet_glyph("\u{f0a7}", Some("Wingdings")), "■");
        assert_eq!(bullet_glyph("\u{f06c}", Some("Wingdings")), "●");
        assert_eq!(bullet_glyph("\u{f0fc}", Some("Arial")), "\u{f0fc}");
    }
}
