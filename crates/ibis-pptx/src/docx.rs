// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! WordprocessingML (`.docx`) to a [`FlowDocument`]: paragraphs, runs and
//! tables with their styles resolved, the default header and footer, and
//! the page setup of the (last) section.
//!
//! Formatting is folded in the order Word applies it: document defaults,
//! the table's style, the paragraph's style (and the styles it is based
//! on), the run's character style, then direct formatting.

use std::collections::HashMap;
use std::sync::Arc;

use ibis_doc_model::{
    AbstractList, ArtBorder, AutoFit, AutoShape, Block, BorderStyle, CellAnchor, Comment, DropCap, DropCapLocation, EmuRect, EmuSize, Fill, FlowAlign, FlowBorder, FlowCell, FlowDocument, FlowParagraph, FlowRow, FlowRun,
    FlowRunKind, FlowStyle, FlowStyles, FlowTable, FontAlign, Insets, LineCap, LineDash, LineHeight, Link, ListInstance, ListLevel, ListSuffix, MediaFormat, MediaId,
    Note, Numbering, PageSetup, Paragraph, PageColumns, ParaFormat, PartKind, PreferredWidth, Presentation, Revision, RevisionKind, Rgb, RowHeight, Run, RunFormat, RunStyle, SectionBreak,
    SectionSetup, ShadingPattern, Shape, ShapeId, ShapeKind, Slide, Stroke, StyleKind, TabAlign, TabChange, TabStop, TextBody, TextAlign, TextStyle, UnderlineStyle, VerticalAnchor,
    VerticalMerge, VerticalPosition,
};
use roxmltree::{Document, Node};

use crate::Error;
use crate::save::docx::{AtomSource, ParaSource, SourceMap, TableSource};
use crate::package::{Package, REL_OFFICE_DOCUMENT, Relationship};
use crate::theme::Theme;
use crate::xml::{attr, attr_bool, attr_i64, child, children, elements};

type Rels = HashMap<String, Relationship>;

/// EMU per twentieth of a point (twip).
const EMU_PER_TWIP: i64 = 635;

/// WordprocessingML qualifies attributes by URI, independently of prefix.
fn wattr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes().find(|a| a.name() == name && a.namespace() == Some("http://schemas.openxmlformats.org/wordprocessingml/2006/main")).map(|a| a.value())
}

/// Relationship attributes use their own URI; Word attributes are separate.
fn rattr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes().find(|a| a.name() == name && a.namespace() == Some("http://schemas.openxmlformats.org/officeDocument/2006/relationships")).map(|a| a.value())
}

/// An attribute of a `w14:` element by its name (they have their own URI).
fn w14attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes().find(|a| a.name() == name).map(|a| a.value())
}

/// The colour in a `w14:` colour element (`w14:srgbClr`) and its opacity (`w14:alpha`, 0 to 255).
fn w14color(node: Node) -> Option<(Rgb, u8)> {
    let c = child(node, "srgbClr")?;
    let rgb = w14attr(c, "val").and_then(hex)?;
    let alpha = child(c, "alpha").and_then(|a| w14attr(a, "val")).and_then(|v| v.parse::<f64>().ok()).map_or(255, |v| (v / 100_000.0 * 255.0).round().clamp(0.0, 255.0) as u8);
    Some((rgb, alpha))
}

/// A run's `w14:` text effects (outline, glow, shadow, gradient fill), `None` when it has none.
fn text_effects(rpr: Node) -> Option<ibis_doc_model::TextEffects> {
    let mut e = ibis_doc_model::TextEffects::default();
    if let Some(o) = child(rpr, "textOutline") {
        if let Some((color, _)) = child(o, "solidFill").and_then(w14color) {
            e.outline = Some((w14attr(o, "w").and_then(|w| w.parse().ok()).unwrap_or(9525), color));
        }
    }
    if let Some(g) = child(rpr, "glow") {
        if let Some((color, alpha)) = w14color(g) {
            e.glow = Some((w14attr(g, "rad").and_then(|w| w.parse().ok()).unwrap_or(0), color, alpha));
        }
    }
    if let Some(s) = child(rpr, "shadow") {
        if let Some((color, alpha)) = w14color(s) {
            let num = |n: &str| w14attr(s, n).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            e.shadow = Some(ibis_doc_model::TextShadow { blur: num("blurRad"), distance: num("dist"), direction: num("dir") as i32, color, alpha });
        }
    }
    if let Some(grad) = child(rpr, "textFill").and_then(|f| child(f, "gradFill")) {
        let stops: Vec<(u32, Rgb)> = child(grad, "gsLst").into_iter().flat_map(|l| children(l, "gs")).filter_map(|gs| Some((w14attr(gs, "pos")?.parse().ok()?, w14color(gs)?.0))).collect();
        if stops.len() >= 2 {
            e.gradient = Some((stops, child(grad, "lin").and_then(|l| w14attr(l, "ang")).and_then(|a| a.parse().ok()).unwrap_or(0)));
        }
    }
    (e != ibis_doc_model::TextEffects::default()).then_some(e)
}

/// A content control's title, tag and lock (`w:alias`, `w:tag`, `w:lock`).
fn control_info(sdt: Node) -> Option<Box<ibis_doc_model::ControlInfo>> {
    let pr = child(sdt, "sdtPr")?;
    let title = child(pr, "alias").and_then(|x| wattr(x, "val")).unwrap_or("").to_string();
    let tag = child(pr, "tag").and_then(|x| wattr(x, "val")).unwrap_or("").to_string();
    let lock = child(pr, "lock").and_then(|x| wattr(x, "val")).map_or(ibis_doc_model::ControlLock::None, ibis_doc_model::ControlLock::from_name);
    if title.is_empty() && tag.is_empty() && lock == ibis_doc_model::ControlLock::None {
        return None;
    }
    Some(Box::new(ibis_doc_model::ControlInfo { title, tag, lock }))
}

fn wval<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    child(node, name).and_then(|n| wattr(n, "val"))
}

fn twips(node: Node, name: &str) -> Option<i64> {
    wattr(node, name).and_then(|v| v.parse::<f64>().ok()).map(|v| (v * EMU_PER_TWIP as f64).round() as i64)
}

/// An on/off property (`<w:b/>`, `<w:b w:val="0"/>`).
fn toggle(node: Node, name: &str) -> Option<bool> {
    child(node, name).map(|n| !matches!(wattr(n, "val"), Some("0" | "false" | "off")))
}

fn hex(value: &str) -> Option<Rgb> {
    if value == "auto" || value.len() != 6 {
        return None;
    }
    u32::from_str_radix(value, 16).ok().map(Rgb)
}

/// A font name as the file spells it, or what it was meant to be when a Shift_JIS name was saved as Windows-1252 text (`‚l‚r –¾’©`
/// for ＭＳ 明朝: Word reads those files with the Japanese code page).
fn font_name(name: &str) -> String {
    if name.is_ascii() || !name.chars().all(|c| (c as u32) < 0x100 || matches!(c as u32, 0x152 | 0x153 | 0x160 | 0x161 | 0x178 | 0x17d | 0x17e | 0x192 | 0x2c6 | 0x2dc | 0x2013..=0x2014 | 0x2018..=0x201e | 0x2020..=0x2022 | 0x2026 | 0x2030 | 0x2039 | 0x203a | 0x20ac | 0x2122)) {
        return name.to_string();
    }
    let (bytes, _, bad) = encoding_rs::WINDOWS_1252.encode(name);
    if bad {
        return name.to_string();
    }
    let (text, _, bad) = encoding_rs::SHIFT_JIS.decode(&bytes);
    if bad || !text.chars().any(|c| c as u32 >= 0x3000) {
        return name.to_string();
    }
    text.into_owned()
}

/// Run properties, each unset until some level sets it.
/// A run's properties as its `w:rPr` (or a style's) sets them.
fn run_format(rpr: Node, theme: &Theme) -> RunFormat {
    let mut f = RunFormat::default();
    if let Some(fonts) = child(rpr, "rFonts") {
        let pick = |plain: &str, themed: &str| -> Option<String> {
            if let Some(t) = wattr(fonts, themed) {
                if t.ends_with("EastAsia") {
                    return theme.east_asian_typeface(t.starts_with("major"));
                }
                if t.ends_with("Bidi") || t.ends_with("Cs") {
                    return theme.complex_typeface(t.starts_with("major"));
                }
                return theme.typeface(if t.starts_with("major") { "+mj-lt" } else { "+mn-lt" });
            }
            wattr(fonts, plain).map(font_name)
        };
        f.font = pick("ascii", "asciiTheme");
        f.east_asian_font = pick("eastAsia", "eastAsiaTheme");
        f.cs_font = pick("cs", "cstheme").or_else(|| pick("cs", "csTheme"));
        if let Some(h) = wattr(fonts, "hint") {
            f.east_asia_hint = Some(h == "eastAsia");
        }
    }
    if let Some(l) = child(rpr, "lang") {
        f.lang = wattr(l, "val").map(str::to_string);
        f.lang_east_asia = wattr(l, "eastAsia").map(str::to_string);
        f.lang_bidi = wattr(l, "bidi").map(str::to_string);
    }
    f.size = wval(rpr, "sz").and_then(|v| v.parse::<u32>().ok()).map(|sz| sz * 50);
    f.cs_size = wval(rpr, "szCs").and_then(|v| v.parse::<u32>().ok()).map(|sz| sz * 50);
    f.bold = toggle(rpr, "b");
    f.italic = toggle(rpr, "i");
    if let Some(u) = child(rpr, "u") {
        let val = wattr(u, "val").unwrap_or("single");
        f.underline = Some(val != "none");
        // A single line is the default (`None`), as an underline turned on
        // by the editor is.
        f.underline_style = UnderlineStyle::from_name(val).filter(|s| *s != UnderlineStyle::Single);
        f.underline_color = if let Some(t) = wattr(u, "themeColor")
            && let Some(rgb) = theme_color(theme, t, wattr(u, "themeTint"), wattr(u, "themeShade"))
        {
            Some(Some(rgb))
        } else {
            wattr(u, "color").map(hex)
        };
    }
    f.strike = toggle(rpr, "strike");
    f.double_strike = toggle(rpr, "dstrike");
    f.caps = toggle(rpr, "caps");
    f.small_caps = toggle(rpr, "smallCaps");
    f.outline = toggle(rpr, "outline");
    f.shadow = toggle(rpr, "shadow");
    f.emboss = toggle(rpr, "emboss");
    f.imprint = toggle(rpr, "imprint");
    f.no_proof = toggle(rpr, "noProof");
    f.emphasis = wval(rpr, "em").and_then(ibis_doc_model::EmphasisMark::from_name);
    // 組み文字と縦中横 (`w:eastAsianLayout`).
    if let Some(layout) = child(rpr, "eastAsianLayout") {
        if wattr(layout, "combine").is_some_and(|v| matches!(v, "1" | "true" | "on")) {
            f.combine = Some(Some(ibis_doc_model::Combine::from_brackets(wattr(layout, "combineBrackets"))));
        }
        if wattr(layout, "vert").is_some_and(|v| matches!(v, "1" | "true" | "on" | "")) {
            f.tate_chu_yoko = Some(true);
        } else if wattr(layout, "vert").is_some_and(|v| matches!(v, "0" | "false" | "off")) {
            f.tate_chu_yoko = Some(false);
        }
    }
    if let Some(c) = child(rpr, "color") {
        let theme_color_attr = wattr(c, "themeColor");
        let tint_attr = wattr(c, "themeTint");
        let shade_attr = wattr(c, "themeShade");
        if let Some(t) = theme_color_attr {
            let tc = ibis_doc_model::FlowThemeColor::new(t, tint_attr, shade_attr);
            if let Some(rgb) = theme_color(theme, t, tint_attr, shade_attr) {
                f.color = Some(rgb);
                f.theme_color = Some(tc);
            } else {
                f.color = wattr(c, "val").and_then(hex);
                f.theme_color = Some(tc);
            }
        } else {
            f.color = wattr(c, "val").map(|v| hex(v).unwrap_or(Rgb::BLACK));
            f.theme_color = None;
        }
    }
    // Word 2010's text effects.
    f.effects = text_effects(rpr).map(|e| Some(std::sync::Arc::new(e)));
    let h = wval(rpr, "highlight");
    let h_color = h.and_then(highlight);
    let shd_color = child(rpr, "shd").and_then(|s| shading(s, theme));
    // A stripe or a grid: the fill is the highlight, the pattern is drawn over it.
    let shd_pattern = child(rpr, "shd").map(|s| shading_detail(s, theme)).and_then(|(fill, pattern, color)| Some((fill, pattern?, color)));
    if let Some(color) = h_color {
        f.highlight = Some(Some(color));
    } else if let Some((fill, pattern, color)) = shd_pattern {
        f.highlight = Some(Some(fill.unwrap_or(Rgb(0xffffff))));
        f.shd_pattern = Some(Some((pattern, color.unwrap_or(Rgb::BLACK))));
    } else if let Some(color) = shd_color {
        f.highlight = Some(Some(color));
    } else if h == Some("none") {
        f.highlight = Some(None);
    }
    f.position = wval(rpr, "vertAlign").map(|v| match v {
        "superscript" => VerticalPosition::Superscript,
        "subscript" => VerticalPosition::Subscript,
        _ => VerticalPosition::Baseline,
    });
    f.spacing = child(rpr, "spacing").and_then(|s| twips(s, "val"));
    f.scale = wval(rpr, "w").and_then(|v| v.parse::<u32>().ok());
    f.fit_text = child(rpr, "fitText").and_then(|e| {
        let width = wattr(e, "val").and_then(|v| v.parse::<u32>().ok())?;
        let id = wattr(e, "id").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        Some(Some((width, id)))
    });
    f.hidden = toggle(rpr, "vanish");
    f.border = child(rpr, "bdr").map(|b| border(b, Some(theme)));
    f.kern = wval(rpr, "kern").and_then(|v| v.parse::<u32>().ok());
    // OpenType features (w14:ligatures, w14:numForm, w14:numSpacing, w14:cntxtAlts).
    f.ligatures = child(rpr, "ligatures").and_then(|l| w14attr(l, "val")).and_then(ibis_doc_model::Ligatures::from_ooxml);
    f.num_form = child(rpr, "numForm").and_then(|n| w14attr(n, "val")).and_then(ibis_doc_model::NumberForm::from_ooxml);
    f.num_spacing = child(rpr, "numSpacing").and_then(|n| w14attr(n, "val")).and_then(ibis_doc_model::NumberSpacing::from_ooxml);
    if let Some(ca) = child(rpr, "cntxtAlts") {
        f.contextual_alternates = Some(!matches!(w14attr(ca, "val"), Some("0" | "false" | "off")));
    }
    f
}

fn highlight(name: &str) -> Option<Rgb> {
    Some(Rgb(match name {
        "yellow" => 0xffff00,
        "green" => 0x00ff00,
        "cyan" => 0x00ffff,
        "magenta" => 0xff00ff,
        "blue" => 0x0000ff,
        "red" => 0xff0000,
        "darkBlue" => 0x00008b,
        "darkCyan" => 0x008b8b,
        "darkGreen" => 0x006400,
        "darkMagenta" => 0x8b008b,
        "darkRed" => 0x8b0000,
        "darkYellow" => 0x808000,
        "darkGray" => 0xa9a9a9,
        "lightGray" => 0xd3d3d3,
        "black" => 0x000000,
        "white" => 0xffffff,
        _ => return None,
    }))
}

/// A paragraph's properties as its `w:pPr` (or a style's) sets them.
fn para_format(ppr: Node, theme: &Theme) -> ParaFormat {
    let mut f = ParaFormat::default();
    if let Some(jc) = wval(ppr, "jc") {
        f.align = Some(match jc {
            "center" => FlowAlign::Center,
            "right" | "end" => FlowAlign::Right,
            "both" | "mediumKashida" | "highKashida" | "lowKashida" => FlowAlign::Justify,
            "distribute" | "thaiDistribute" => FlowAlign::Distribute,
            _ => FlowAlign::Left,
        });
    }
    if let Some(ta) = wval(ppr, "textAlignment") {
        f.text_alignment = Some(match ta {
            "top" => FontAlign::Top,
            "center" => FontAlign::Center,
            "bottom" => FontAlign::Bottom,
            "auto" => FontAlign::Auto,
            _ => FontAlign::Baseline,
        });
    }
    if let Some(ind) = child(ppr, "ind") {
        let char_twips = child(ppr, "rPr")
            .and_then(|r| child(r, "sz"))
            .and_then(|s| wattr(s, "val"))
            .and_then(|v| v.parse::<f64>().ok())
            .map(|sz| sz * 10.0)
            .unwrap_or(210.0);
        // A count of 0 characters sets nothing: Word keeps the indent the style or the numbering gives.
        let char_units = |name: &str| -> Option<i64> {
            wattr(ind, name)
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| *v != 0.0)
                .map(|v| (v / 100.0 * char_twips * EMU_PER_TWIP as f64).round() as i64)
        };
        let chars_val = |name: &str| -> Option<i32> {
            wattr(ind, name).and_then(|v| v.parse::<i32>().ok()).filter(|v| *v != 0)
        };
        f.left_chars = chars_val("leftChars").or_else(|| chars_val("startChars"));
        f.right_chars = chars_val("rightChars").or_else(|| chars_val("endChars"));
        f.first_line_chars = chars_val("hangingChars")
            .map(|v| -v)
            .or_else(|| chars_val("firstLineChars"));
        f.left = twips(ind, "left")
            .or_else(|| twips(ind, "start"))
            // With `hangingChars`, `leftChars` is where the first line starts: the text's left is that and the hanging.
            .or_else(|| char_units("leftChars").map(|l| l + char_units("hangingChars").unwrap_or(0)))
            .or_else(|| char_units("startChars"));
        f.right = twips(ind, "right")
            .or_else(|| twips(ind, "end"))
            .or_else(|| char_units("rightChars"))
            .or_else(|| char_units("endChars"));
        f.first_line = twips(ind, "hanging").map(|v| -v)
            .or_else(|| twips(ind, "firstLine"))
            .or_else(|| char_units("hangingChars").map(|v| -v))
            .or_else(|| char_units("firstLineChars"));
    }
    if let Some(sp) = child(ppr, "spacing") {
        f.before = twips(sp, "before");
        f.after = twips(sp, "after");
        // Word's automatic space is 14 pt.
        let auto = |name: &str| matches!(wattr(sp, name), Some("1" | "true" | "on"));
        if auto("beforeAutospacing") {
            f.before = Some(280 * EMU_PER_TWIP);
            f.auto_before = Some(true);
        }
        if auto("afterAutospacing") {
            f.after = Some(280 * EMU_PER_TWIP);
            f.auto_after = Some(true);
        }
        if let Some(line) = wattr(sp, "line").and_then(|v| v.parse::<i64>().ok()) {
            f.line = Some(match wattr(sp, "lineRule") {
                Some("exact") => LineHeight::Exact(line * EMU_PER_TWIP),
                Some("atLeast") => LineHeight::AtLeast(line * EMU_PER_TWIP),
                _ => LineHeight::Auto(line.max(0) as u32),
            });
        }
    }
    f.snap = toggle(ppr, "snapToGrid");
    f.kinsoku = toggle(ppr, "kinsoku");
    f.word_wrap = toggle(ppr, "wordWrap");
    f.overflow_punct = toggle(ppr, "overflowPunct");
    f.auto_space_letters = toggle(ppr, "autoSpaceDE");
    f.auto_space_digits = toggle(ppr, "autoSpaceDN");
    f.bidi = toggle(ppr, "bidi");
    f.collapsed = child(ppr, "collapsed").map(|n| !matches!(attr(n, "val"), Some("0" | "false" | "off")));
    f.page_break_before = toggle(ppr, "pageBreakBefore");
    f.suppress_line_numbers = toggle(ppr, "suppressLineNumbers");
    f.suppress_auto_hyphens = toggle(ppr, "suppressAutoHyphens");
    f.contextual_spacing = toggle(ppr, "contextualSpacing");
    f.widow_control = toggle(ppr, "widowControl");
    f.keep_next = toggle(ppr, "keepNext");
    f.keep_lines = toggle(ppr, "keepLines");
    if let Some(frame) = child(ppr, "framePr") {
        // A frame placed by `xAlign` (a page number in a footer): the paragraph is aligned so in the text area.
        if f.align.is_none() {
            f.align = match wattr(frame, "xAlign") {
                Some("center") => Some(FlowAlign::Center),
                Some("right") => Some(FlowAlign::Right),
                _ => None,
            };
        }
        let location = match wattr(frame, "dropCap") {
            Some("drop" | "dropped") => DropCapLocation::Drop,
            Some("margin") => DropCapLocation::Margin,
            _ => DropCapLocation::None,
        };
        if location != DropCapLocation::None {
            let lines = wattr(frame, "lines").and_then(|s| s.parse::<u32>().ok()).unwrap_or(3).clamp(1, 10);
            let space = twips(frame, "hSpace").or_else(|| twips(frame, "space")).unwrap_or(0);
            f.drop_cap = Some(Some(DropCap { location, lines, space }));
        } else {
            f.framed = Some(true);
        }
    }
    if let Some(tabs) = child(ppr, "tabs") {
        for tab in children(tabs, "tab") {
            let Some(position) = twips(tab, "pos") else { continue };
            let stop = (wattr(tab, "val") != Some("clear")).then(|| {
                let align = match wattr(tab, "val") {
                    Some("center") => TabAlign::Center,
                    Some("right" | "end") => TabAlign::Right,
                    Some("decimal") => TabAlign::Decimal,
                    _ => TabAlign::Left,
                };
                let leader = !matches!(wattr(tab, "leader"), None | Some("none"));
                TabStop { position, align, leader }
            });
            f.tabs.push(TabChange { position, stop });
        }
    }
    if let Some(shd) = child(ppr, "shd") {
        let (fill, pat, color) = shading_detail(shd, theme);
        f.shading = Some(shading(shd, theme));
        if pat.is_some() {
            f.shading_fill = fill.map(Some);
            f.shading_pattern = pat.map(Some);
            f.shading_color = color.map(Some);
        }
    }
    if let Some(bdr) = child(ppr, "pBdr") {
        for (i, name) in ["top", "right", "bottom", "left"].into_iter().enumerate() {
            if let Some(edge) = child(bdr, name) {
                f.borders[i] = Some(border(edge, Some(theme)));
            }
        }
    }
    if let Some(num) = child(ppr, "numPr") {
        f.num_id = wval(num, "numId").and_then(|v| v.parse().ok());
        f.level = wval(num, "ilvl").and_then(|v| v.parse().ok());
    }
    f
}

/// A theme colour named as WordprocessingML names them (`accent1`,
/// `text1`, …), lightened by `tint` or darkened by `shade` (hex bytes).
fn theme_color(theme: &Theme, name: &str, tint: Option<&str>, shade: Option<&str>) -> Option<Rgb> {
    let key = match name {
        "text1" | "dark1" => "dk1",
        "background1" | "light1" => "lt1",
        "text2" | "dark2" => "dk2",
        "background2" | "light2" => "lt2",
        "hyperlink" => "hlink",
        "followedHyperlink" => "folHlink",
        other => other,
    };
    let base = theme.color(key)?;
    let byte = |v: Option<&str>| v.and_then(|v| u8::from_str_radix(v, 16).ok()).map(|b| b as f32 / 255.0);
    let channels = [(base.0 >> 16) & 0xff, (base.0 >> 8) & 0xff, base.0 & 0xff].map(|c| {
        let mut c = c as f32;
        if let Some(t) = byte(tint) {
            c += (255.0 - c) * (1.0 - t);
        }
        if let Some(s) = byte(shade) {
            c *= s;
        }
        c.round().clamp(0.0, 255.0) as u32
    });
    Some(Rgb((channels[0] << 16) | (channels[1] << 8) | channels[2]))
}

/// The components of a shading (`w:shd`): (fill, pattern, pattern color).
fn shading_detail(shd: Node, theme: &Theme) -> (Option<Rgb>, Option<ShadingPattern>, Option<Rgb>) {
    let fill = match wattr(shd, "themeFill") {
        Some(t) => theme_color(theme, t, wattr(shd, "themeFillTint"), wattr(shd, "themeFillShade"))
            .or_else(|| wattr(shd, "fill").and_then(hex)),
        None => wattr(shd, "fill").and_then(hex),
    };
    // `themeColor="none"` makes the pattern colour automatic (black), whatever `w:color` is (DCMB-01791: a solid
    // shading with `w:color="808080"` and `themeColor="none"` is black in Word's PDF; ECMA-376 §17.18.78 ST_Shd:
    // solid is all of `w:color`). The sRGB value is only the fallback of a theme colour Word knows.
    let color = match wattr(shd, "themeColor") {
        Some("none") => None,
        Some(t) => theme_color(theme, t, wattr(shd, "themeTint"), wattr(shd, "themeShade"))
            .or_else(|| wattr(shd, "color").and_then(hex)),
        None => wattr(shd, "color").and_then(hex),
    };
    let val = wattr(shd, "val").unwrap_or("clear");
    let pattern = ShadingPattern::from_ooxml(val);
    (fill, pattern, color)
}

/// The colour of a shading (`w:shd`): the fill, with the pattern's
/// colour (`w:color`) over it in the pattern's share (a percentage, or a
/// stripe's or grid's share of the area). `None` for no shading.
fn shading(shd: Node, theme: &Theme) -> Option<Rgb> {
    let (fill, _, color) = shading_detail(shd, theme);
    let val = wattr(shd, "val").unwrap_or("clear");
    let share = match val {
        "nil" => return None,
        "clear" => 0.0,
        "solid" => 1.0,
        v if v.starts_with("pct") => v[3..].parse::<f32>().unwrap_or(0.0) / 100.0,
        "thinHorzStripe" | "thinVertStripe" | "thinReverseDiagStripe" | "thinDiagStripe" => 0.25,
        "thinHorzCross" | "thinDiagCross" => 0.4375,
        "horzStripe" | "vertStripe" | "reverseDiagStripe" | "diagStripe" => 0.5,
        "horzCross" | "diagCross" => 0.75,
        _ => 0.0,
    };
    if share == 0.0 {
        return fill;
    }
    // An automatic pattern colour is black; an automatic fill, white.
    let (c, f) = (color.unwrap_or(Rgb::BLACK), fill.unwrap_or(Rgb(0xffffff)));
    let mix = |shift: u32| {
        let (a, b) = (((c.0 >> shift) & 0xff) as f32, ((f.0 >> shift) & 0xff) as f32);
        ((a * share + b * (1.0 - share)).round() as u32) << shift
    };
    Some(Rgb(mix(16) | mix(8) | mix(0)))
}

/// A glyph of a symbol font (`Symbol`, `Wingdings`) as the Unicode character it shows.
fn symbol_char(c: char, font: Option<&str>) -> Option<char> {
    let code = c as u32;
    let code = if (0xf000..=0xf0ff).contains(&code) { code - 0xf000 } else { code };
    match font? {
        "Symbol" => Some(match code {
            0xb7 => '•',
            0xa8 => '♦',
            0xd8 => '¬',
            _ if c > '\u{ff}' => c,
            _ => char::from_u32(code)?,
        }),
        "Wingdings" => match code {
            0x6c => Some('●'),
            0x6e => Some('■'),
            0x6f => Some('□'),
            0x71 => Some('❑'),
            0x75 => Some('◆'),
            0x76 => Some('❖'),
            0x9f => Some('•'),
            0xa7 => Some('▪'),
            0xa8 => Some('◻'),
            0xd8 => Some('➢'),
            0xfb => Some('✗'),
            0xfc => Some('✓'),
            0xfd => Some('☒'),
            0xfe => Some('☑'),
            _ if c > '\u{ff}' => Some(c),
            _ => None,
        },
        _ => None,
    }
}

/// A bullet of a symbol font as the Unicode character it shows.
fn bullet_char(c: char, font: Option<&str>) -> char {
    if let Some(mapped) = symbol_char(c, font) {
        return mapped;
    }
    if matches!(font, Some("Wingdings")) || (0xf000..=0xf0ff).contains(&(c as u32)) {
        '●'
    } else {
        c
    }
}

/// The lists of `numbering.xml`.
/// The pictures of the picture bullets (`w:numPicBullet`): id, the relationship
/// of the image and the size the bullet's shape says (EMU).
fn pic_bullet_sources(root: Node) -> Vec<(u32, String, Option<EmuSize>)> {
    let mut out = Vec::new();
    for bullet in children(root, "numPicBullet") {
        let Some(id) = wattr(bullet, "numPicBulletId").and_then(|v| v.parse().ok()) else { continue };
        let image = bullet.descendants().find(|n| n.is_element() && n.tag_name().name() == "imagedata").and_then(|n| rattr(n, "id"))
            .or_else(|| bullet.descendants().find(|n| n.is_element() && n.tag_name().name() == "blip").and_then(blip_embed));
        let Some(rid) = image else { continue };
        // v:shape's style="width:9pt;height:9pt".
        let style = bullet.descendants().find(|n| n.is_element() && n.tag_name().name() == "shape").and_then(|n| n.attribute("style"));
        let pt = |name: &str| style.and_then(|st| st.split(';').find_map(|kv| kv.trim().strip_prefix(&format!("{name}:")).map(str::to_string))).and_then(|v| v.trim().strip_suffix("pt").and_then(|n| n.trim().parse::<f64>().ok()));
        let size = pt("width").zip(pt("height")).map(|(w, h)| EmuSize::new((w * 12700.0) as i64, (h * 12700.0) as i64));
        out.push((id, rid.to_string(), size));
    }
    out
}

fn numbering(root: Node, theme: &Theme) -> Numbering {
    let mut out = Numbering::default();
    for abs in children(root, "abstractNum") {
        let Some(id) = wattr(abs, "abstractNumId").and_then(|v| v.parse().ok()) else { continue };
        let mut levels = Vec::new();
        for lvl in children(abs, "lvl") {
            let run = child(lvl, "rPr").map(|r| run_format(r, theme)).unwrap_or_default();
            let format = wval(lvl, "numFmt").unwrap_or("decimal").to_string();
            let text: String = wval(lvl, "lvlText").unwrap_or("").chars().map(|c| if format == "bullet" { bullet_char(c, run.font.as_deref()) } else { c }).collect();
            // A symbol font's bullet is drawn as the Unicode character, in
            // the paragraph's font.
            let mut run = run;
            if format == "bullet" && matches!(run.font.as_deref(), Some("Symbol" | "Wingdings")) {
                run.font = None;
            }
            levels.push(ListLevel {
                start: wval(lvl, "start").and_then(|v| v.parse().ok()).unwrap_or(1),
                format,
                text,
                align: match wval(lvl, "lvlJc") {
                    Some("center") => FlowAlign::Center,
                    Some("right" | "end") => FlowAlign::Right,
                    _ => FlowAlign::Left,
                },
                suffix: match wval(lvl, "suff") {
                    Some("space") => ListSuffix::Space,
                    Some("nothing") => ListSuffix::Nothing,
                    _ => ListSuffix::Tab,
                },
                para: child(lvl, "pPr").map(|p| para_format(p, theme)).unwrap_or_default(),
                run,
                pic_bullet: wval(lvl, "lvlPicBulletId").and_then(|v| v.parse().ok()),
            });
        }
        let num_style_link = wval(abs, "numStyleLink").map(str::to_string);
        out.abstracts.insert(id, AbstractList { levels, num_style_link });
    }
    for num in children(root, "num") {
        let Some(id) = wattr(num, "numId").and_then(|v| v.parse().ok()) else { continue };
        let Some(abstract_id) = wval(num, "abstractNumId").and_then(|v| v.parse().ok()) else { continue };
        let mut start_overrides = std::collections::BTreeMap::new();
        for o in children(num, "lvlOverride") {
            if let (Some(l), Some(s)) = (wattr(o, "ilvl").and_then(|v| v.parse().ok()), wval(o, "startOverride").and_then(|v| v.parse().ok())) {
                start_overrides.insert(l, s);
            }
        }
        out.nums.insert(id, ListInstance { abstract_id, start_overrides });
    }
    out
}

/// A border element (`w:top`, `w:insideH`, …); `None` for no line.
fn border(edge: Node, theme: Option<&Theme>) -> Option<FlowBorder> {
    let kind = wattr(edge, "val")?;
    use BorderStyle::*;
    let style = match kind {
        "nil" | "none" => return None,
        "thick" => Thick,
        "double" => Double,
        "triple" => Triple,
        "dotted" => Dotted,
        "dashed" => Dashed,
        "dotDash" => DotDash,
        "dotDotDash" => DotDotDash,
        "dashSmallGap" => DashSmallGap,
        "dashDotStroked" => DashDotStroked,
        "thinThickSmallGap" => ThinThickSmallGap,
        "thickThinSmallGap" => ThickThinSmallGap,
        "thinThickThinSmallGap" => ThinThickThinSmallGap,
        "thinThickMediumGap" => ThinThickMediumGap,
        "thickThinMediumGap" => ThickThinMediumGap,
        "thinThickThinMediumGap" => ThinThickThinMediumGap,
        "thinThickLargeGap" => ThinThickLargeGap,
        "thickThinLargeGap" => ThickThinLargeGap,
        "thinThickThinLargeGap" => ThinThickThinLargeGap,
        "wave" => Wave,
        "doubleWave" => DoubleWave,
        "threeDEmboss" => ThreeDEmboss,
        "threeDEngrave" => ThreeDEngrave,
        "outset" => Outset,
        "inset" => Inset,
        _ => Single,
    };
    // Eighths of a point.
    let size = wattr(edge, "sz").and_then(|v| v.parse::<i64>().ok()).unwrap_or(4);
    let color = if let Some(t) = wattr(edge, "themeColor")
        && let Some(theme) = theme
        && let Some(rgb) = theme_color(theme, t, wattr(edge, "themeTint"), wattr(edge, "themeShade"))
    {
        rgb
    } else {
        wattr(edge, "color").and_then(hex).unwrap_or(Rgb::BLACK)
    };
    Some(FlowBorder {
        width: size * 12_700 / 8,
        color,
        style,
        space: wattr(edge, "space").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0) * 12_700,
        // A cell's art border is drawn as a plain line; the room it takes is its `w:sz` in points
        // (see `art_room`).
        art: ArtBorder::from_name(kind),
    })
}

/// The path of `node` below `ancestor` (see `crate::xml::element_path`).
fn relative_path(node: Node, ancestor: Node) -> Vec<u32> {
    let mut path = Vec::new();
    let mut at = node;
    while at != ancestor {
        let Some(parent) = at.parent() else { break };
        path.push(elements(parent).position(|c| c == at).unwrap_or(0) as u32);
        at = parent;
    }
    path.reverse();
    path
}

struct Reader<'p, 'd, 'i> {
    package: &'p Package<'p>,
    theme: &'p Theme,
    styles: &'p FlowStyles,
    /// The styles' elements (for tables' properties).
    style_nodes: HashMap<String, Node<'d, 'i>>,
    presentation: &'p mut Presentation,
    media: HashMap<String, MediaId>,
    /// The last id given to a paragraph or table.
    last_id: u32,
    /// The part being read, and where things came from.
    part: String,
    sources: SourceMap,
    /// The complex fields open where reading is.
    fields: FieldState,
    /// The paragraphs that end a section, in order.
    sections: Vec<u32>,
    /// The tracked change being read in (`w:ins`, `w:del`).
    revision: Option<Revision>,
    /// Grouped shapes (`wpg:wgp`, DOCX-FLOAT-016).
    shape_groups: Vec<ibis_doc_model::ShapeGroup>,
}

/// The page setup of a section (`w:sectPr`).
fn page_setup(sect: Node) -> PageSetup {
    let mut page = PageSetup::default();
    if let Some(size) = child(sect, "pgSz") {
        page.size = EmuSize::new(twips(size, "w").unwrap_or(page.size.width), twips(size, "h").unwrap_or(page.size.height));
    }
    if let Some(m) = child(sect, "pgMar") {
        page.top = twips(m, "top").unwrap_or(page.top).abs();
        page.right = twips(m, "right").unwrap_or(page.right);
        page.bottom = twips(m, "bottom").unwrap_or(page.bottom).abs();
        page.left = twips(m, "left").unwrap_or(page.left);
        page.header = twips(m, "header").unwrap_or(page.header);
        page.footer = twips(m, "footer").unwrap_or(page.footer);
        page.gutter = twips(m, "gutter").unwrap_or(0).max(0);
    }
    if let Some(n) = child(sect, "footnotePr") {
        page.footnotes = note_settings(n, page.footnotes);
    }
    if let Some(n) = child(sect, "endnotePr") {
        page.endnotes = note_settings(n, page.endnotes);
    }
    page.v_align = match child(sect, "vAlign").and_then(|v| wattr(v, "val")) {
        Some("center") => ibis_doc_model::PageVAlign::Center,
        Some("bottom") => ibis_doc_model::PageVAlign::Bottom,
        Some("both") => ibis_doc_model::PageVAlign::Both,
        _ => ibis_doc_model::PageVAlign::Top,
    };
    if let Some(grid) = child(sect, "docGrid") {
        let grid_type = match wattr(grid, "type") {
            Some("lines") => ibis_doc_model::DocGridType::Lines,
            Some("linesAndChars") => ibis_doc_model::DocGridType::LinesAndChars,
            Some("snapToChars") => ibis_doc_model::DocGridType::SnapToChars,
            Some("default") => ibis_doc_model::DocGridType::Default,
            // Without a type the grid is the default one (none): Word does not line paragraphs up on its pitch.
            _ => ibis_doc_model::DocGridType::Default,
        };
        page.doc_grid_type = grid_type;
        if grid_type != ibis_doc_model::DocGridType::Default {
            page.line_pitch = twips(grid, "linePitch");
            // In 1/4096 pt.
            page.char_space = wattr(grid, "charSpace").and_then(|v| v.parse::<i64>().ok()).map(|v| v * 12_700 / 4096);
        }
    }
    if let Some(start) = child(sect, "pgNumType").and_then(|n| wattr(n, "start")).and_then(|v| v.parse().ok()) {
        page.first_page = start;
    }
    page.direction = child(sect, "textDirection").and_then(|n| wattr(n, "val")).map_or_default(ibis_doc_model::CellDirection::from_name);
    if let Some(n) = child(sect, "lnNumType") {
        page.line_numbers = Some(ibis_doc_model::LineNumbers {
            count_by: wattr(n, "countBy").and_then(|v| v.parse().ok()).unwrap_or(1).max(1),
            // w:start is one less than the first number.
            start: wattr(n, "start").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0) + 1,
            distance: twips(n, "distance").unwrap_or(0),
            restart: match wattr(n, "restart") {
                Some("newSection") => ibis_doc_model::LineNumberRestart::NewSection,
                Some("continuous") => ibis_doc_model::LineNumberRestart::Continuous,
                _ => ibis_doc_model::LineNumberRestart::NewPage,
            },
        });
    }
    if let Some(cols) = child(sect, "cols") {
        let c = &mut page.columns;
        let listed: Vec<(i64, i64)> = children(cols, "col").map(|col| (twips(col, "w").unwrap_or(0), twips(col, "space").unwrap_or(0))).collect();
        c.count = wattr(cols, "num").and_then(|v| v.parse::<u8>().ok()).unwrap_or(1).clamp(1, PageColumns::MAX as u8);
        c.space = twips(cols, "space").unwrap_or(c.space);
        c.separator = wattr(cols, "sep").is_some_and(|v| v == "1" || v == "true" || v == "on");
        // Columns of their own widths (w:equalWidth off, w:col each).
        if wattr(cols, "equalWidth").is_some_and(|v| v == "0" || v == "false" || v == "off") && listed.len() >= c.count as usize && c.count > 1 {
            let mut widths = [(0, 0); PageColumns::MAX];
            for (w, l) in widths.iter_mut().zip(&listed) {
                *w = *l;
            }
            c.widths = Some(widths);
        }
    }
    if let Some(b) = child(sect, "pgBorders") {
        page.borders_from_text = wattr(b, "offsetFrom") == Some("text");
        for (i, name) in ["top", "right", "bottom", "left"].into_iter().enumerate() {
            let edge = child(b, name);
            let val = edge.and_then(|e| wattr(e, "val"));
            let art = val.and_then(ArtBorder::from_name);
            page.borders[i] = if let Some(art_border) = art {
                let e = edge.unwrap();
                // For art borders, w:sz specifies width in points (default is 24 pt).
                let sz_pt = wattr(e, "sz").and_then(|v| v.parse::<i64>().ok()).unwrap_or(24);
                let color = wattr(e, "color").and_then(hex).unwrap_or(Rgb::BLACK);
                let space_pt = wattr(e, "space").and_then(|v| v.parse::<i64>().ok()).unwrap_or(24);
                Some(FlowBorder {
                    width: sz_pt * 12_700,
                    color,
                    style: BorderStyle::Single,
                    space: space_pt * 12_700,
                    art: Some(art_border),
                })
            } else {
                edge.and_then(|e| border(e, None))
            };
        }
    }
    page
}

/// Notes' numbering and place (`w:footnotePr` / `w:endnotePr`), over `base`.
fn note_settings(n: Node, base: ibis_doc_model::NoteSettings) -> ibis_doc_model::NoteSettings {
    use ibis_doc_model::{NoteFormat, NotePosition, NoteRestart};
    let mut out = base;
    if let Some(f) = child(n, "numFmt").and_then(|e| wattr(e, "val")).and_then(NoteFormat::from_ooxml) {
        out.format = f;
    }
    if let Some(v) = child(n, "numStart").and_then(|e| wattr(e, "val")).and_then(|v| v.parse().ok()) {
        out.start = v;
    }
    if let Some(v) = child(n, "numRestart").and_then(|e| wattr(e, "val")) {
        out.restart = match v {
            "eachSect" => NoteRestart::EachSection,
            "eachPage" => NoteRestart::EachPage,
            _ => NoteRestart::Continuous,
        };
    }
    if let Some(v) = child(n, "pos").and_then(|e| wattr(e, "val")) {
        out.position = match v {
            "beneathText" => NotePosition::BeneathText,
            "sectEnd" => NotePosition::SectionEnd,
            "docEnd" => NotePosition::DocumentEnd,
            _ => NotePosition::PageBottom,
        };
    }
    out
}

/// The chapter number in the page numbers (`w:pgNumType`'s `chapStyle` and `chapSep`).
fn page_chapter(numbers: Option<Node>) -> Option<(u32, String)> {
    let numbers = numbers?;
    let level = wattr(numbers, "chapStyle")?.parse::<u32>().ok().filter(|l| (1..=9).contains(l))?;
    Some((level, wattr(numbers, "chapSep").unwrap_or("hyphen").to_string()))
}

/// A section's setup without its headers and footers (read afterwards).
fn section_setup(sect: Node) -> SectionSetup {
    let numbers = child(sect, "pgNumType");
    SectionSetup {
        page: page_setup(sect),
        restart_numbering: numbers.and_then(|n| wattr(n, "start")).is_some(),
        page_number_format: numbers.and_then(|n| wattr(n, "fmt")).map(str::to_string),
        page_chapter: page_chapter(numbers),
        title_page: toggle(sect, "titlePg").unwrap_or(false),
        parts: Default::default(),
    }
}

/// The part a section's header or footer of `kind` is in.
fn part_reference(sect: Node, kind: PartKind, rels: &Rels) -> Option<String> {
    let name = if kind.is_footer() { "footerReference" } else { "headerReference" };
    let wanted = kind.reference_type();
    let node = children(sect, name).find(|n| match wattr(*n, "type") {
        None => wanted == "default",
        Some(t) => t == wanted,
    })?;
    let rid = rattr(node, "id")?;
    rels.get(rid).filter(|r| !r.external).map(|r| r.target.clone())
}

/// Give section paragraph `id` (directly in `blocks`) its part of `kind`.
fn set_section_part(blocks: &mut [Block], id: u32, kind: PartKind, content: Vec<Block>) {
    for b in blocks {
        if let Block::Paragraph(p) = b
            && p.id == id
            && let Some(section) = p.section.as_mut()
        {
            Arc::make_mut(section).parts[kind.index()] = Some(content);
            return;
        }
    }
}

/// What is being read of a paragraph: its element and where each position
/// of its runs came from.
struct ParaRead<'d, 'i> {
    p: Node<'d, 'i>,
    atoms: Vec<AtomSource>,
    /// A table of contents' field starts at the paragraph's start, or ends
    /// in it.
    field_start: Option<String>,
    field_end: bool,
}

impl ParaRead<'_, '_> {
    fn in_run(&mut self, run: Node, node: Node, count: usize) {
        let run_path = relative_path(run, self.p);
        let node_path = relative_path(node, self.p);
        for _ in 0..count {
            self.atoms.push(AtomSource::InRun { run: run_path.clone(), node: node_path.clone() });
        }
    }
}

/// The picture an `a:blip` shows: its SVG version (`asvg:svgBlip`), else its own `r:embed`.
fn blip_embed<'a>(blip: Node<'a, '_>) -> Option<&'a str> {
    let svg = blip.descendants().find(|n| n.is_element() && n.tag_name().name() == "svgBlip").and_then(|n| rattr(n, "embed").or_else(|| attr(n, "embed")));
    svg.or_else(|| rattr(blip, "embed"))
}

impl<'p, 'd, 'i> Reader<'p, 'd, 'i> {
    /// The elements of the chain of styles from the root to `id`.
    fn chain_nodes(&self, id: &str) -> Vec<Node<'d, 'i>> {
        let mut out = Vec::new();
        let mut at = Some(id.to_string());
        while let Some(id) = at {
            let Some(node) = self.style_nodes.get(&id) else { break };
            if out.len() > 16 {
                break;
            }
            out.push(*node);
            at = wval(*node, "basedOn").map(str::to_string);
        }
        out.reverse();
        out
    }

    fn new_id(&mut self) -> u32 {
        self.last_id += 1;
        self.last_id
    }

    fn blocks(&mut self, parent: Node, parent_path: &[u32], rels: &Rels, table_style: Option<&str>, out: &mut Vec<Block>) {
        for (index, node) in elements(parent).enumerate() {
            let mut path = parent_path.to_vec();
            path.push(index as u32);
            match node.tag_name().name() {
                "p" => {
                    let p = self.paragraph(node, &path, rels, table_style);
                    out.push(Block::Paragraph(p));
                }
                "tbl" => {
                    if let Some(table) = self.table(node, &path, rels) {
                        out.push(Block::Table(table));
                    }
                }
                "sdt" => {
                    if let Some(content) = child(node, "sdtContent") {
                        let i = elements(node).position(|c| c == content).unwrap_or(0);
                        path.push(i as u32);
                        self.blocks(content, &path, rels, table_style, out);
                    }
                }
                "AlternateContent" => {
                    if let Some(branch) = child(node, "Choice").or_else(|| child(node, "Fallback")) {
                        let i = elements(node).position(|c| c == branch).unwrap_or(0);
                        path.push(i as u32);
                        self.blocks(branch, &path, rels, table_style, out);
                    }
                }
                _ => {}
            }
        }
    }

    fn paragraph(&mut self, p: Node, path: &[u32], rels: &Rels, table_style: Option<&str>) -> FlowParagraph {
        let ppr = child(p, "pPr");
        let style = ppr.and_then(|p| wval(p, "pStyle")).map(str::to_string);
        let (_, base_run) = self.styles.paragraph_base(style.as_deref(), table_style);
        let mut read = ParaRead { p, atoms: Vec::new(), field_start: None, field_end: false };
        let mut runs = Vec::new();
        // Fields can span paragraphs (a table of contents): their state is
        // the document's; runs and positions are this paragraph's.
        let mut field = std::mem::take(&mut self.fields);
        for frame in &mut field.frames {
            frame.runs.clear();
            frame.atom = None;
        }
        self.inline(p, rels, &base_run, &mut field, &mut read, &mut runs, &None);
        self.fields = field;
        let id = self.new_id();
        let subdoc = child(p, "subDoc").and_then(|sd| {
            let rid = rattr(sd, "id")?;
            let target = rels.get(rid).map(|r| r.target.clone()).unwrap_or_else(|| rid.to_string());
            let title = wattr(sd, "title").map(str::to_string).unwrap_or_else(|| target.clone());
            let locked = wattr(sd, "locked").is_some_and(|v| v == "1" || v == "true");
            let expanded = wattr(sd, "expanded").is_some_and(|v| v == "1" || v == "true");
            let mut content_paragraphs = Vec::new();
            for idx in 0..64 {
                if let Some(line) = wattr(sd, &format!("subPara{idx}")) {
                    content_paragraphs.push(line.to_string());
                } else {
                    break;
                }
            }
            Some(ibis_doc_model::SubdocumentInfo {
                target,
                title,
                expanded,
                locked,
                content_paragraphs,
            })
        });
        let mut paragraph = FlowParagraph {
            id,
            style,
            table_style: table_style.map(str::to_string),
            direct: ppr.map(|p| para_format(p, self.theme)).unwrap_or_default(),
            mark_format: ppr.and_then(|p| child(p, "rPr")).map(|r| run_format(r, self.theme)).unwrap_or_default(),
            runs,
            bookmarks: p.descendants().filter(|n| n.is_element() && n.tag_name().name() == "bookmarkStart").filter_map(|n| wattr(n, "name")).map(str::to_string).collect(),
            bookmark_ends: p.descendants().filter(|n| n.is_element() && n.tag_name().name() == "bookmarkEnd").filter_map(|n| wattr(n, "id")).filter_map(|id| {
                p.document().descendants().find(|s| s.is_element() && s.tag_name().name() == "bookmarkStart" && wattr(*s, "id") == Some(id)).and_then(|s| wattr(s, "name")).map(str::to_string)
            }).filter(|name| !p.descendants().any(|n| n.is_element() && n.tag_name().name() == "bookmarkStart" && wattr(n, "name") == Some(name.as_str()))).collect(),
            field_start: read.field_start.take(),
            field_end: read.field_end,
            subdoc,
            section: ppr.and_then(|p| child(p, "sectPr")).map(|s| {
                self.sections.push(id);
                Arc::new(section_setup(s))
            }),
            section_break: match ppr.and_then(|p| child(p, "sectPr")) {
                None => SectionBreak::None,
                Some(s) => match wval(s, "type") {
                    Some("continuous") => SectionBreak::Continuous,
                    Some("oddPage") => SectionBreak::OddPage,
                    Some("evenPage") => SectionBreak::EvenPage,
                    Some("nextColumn") => SectionBreak::NextColumn,
                    _ => SectionBreak::NextPage,
                },
            },
            ..FlowParagraph::default()
        };
        paragraph.resolve(self.styles);
        // One atom for each position the save tells apart (`save::docx::atoms`): a content control that shows text is one, whatever its text's length
        // (`FlowParagraph::len` counts its characters).
        debug_assert_eq!(read.atoms.len(), crate::save::docx::atom_count(&paragraph));
        self.sources.paragraphs.insert(id, ParaSource { part: self.part.clone(), path: path.to_vec(), atoms: read.atoms });
        paragraph
    }

    /// The runs in `parent` (a paragraph, a hyperlink, a field, …).
    fn inline<'e, 'j>(&mut self, parent: Node<'e, 'j>, rels: &Rels, base: &RunFormat, field: &mut FieldState, read: &mut ParaRead<'e, 'j>, out: &mut Vec<FlowRun>, link: &Option<Link>) {
        for node in elements(parent) {
            match node.tag_name().name() {
                "r" => self.run(node, rels, base, field, read, out, link),
                "hyperlink" => {
                    // Where it goes: a relationship's target, or a bookmark.
                    let url = rattr(node, "id").and_then(|id| rels.get(id)).map(|r| Link::Url(r.target.clone()));
                    let inner = url.or_else(|| wattr(node, "anchor").map(|a| Link::Anchor(a.to_string())));
                    self.inline(node, rels, base, field, read, out, &inner);
                }
                "oMath" | "oMathPara" if node.tag_name().namespace() == Some(crate::math_xml::M) => {
                    // A formula: one atom, its element kept as it is when the paragraph is saved.
                    // Its size is its runs' (`w:rPr/w:sz` inside the `m:r`), the paragraph's when they say none.
                    let mut format = RunFormat::default();
                    if let Some(sz) = node.descendants().filter(|d| d.is_element() && d.tag_name().name() == "rPr").find_map(|r| wval(r, "sz")).and_then(|v| v.parse::<u32>().ok()) {
                        format.size = Some(sz * 50);
                    }
                    let style = self.styles.run_format(base, None, &format).style();
                    let math = crate::math_xml::read_math(node);
                    read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                    out.push(FlowRun { kind: FlowRunKind::Math { math, display: node.tag_name().name() == "oMathPara" }, style, format, char_style: None, link: link.clone(), revision: self.revision.clone(), show_field_code: false, field_locked: false, control: None });
                }
                // A tracked move is a moveFrom where the text was and a moveTo where it went.
                "ins" | "del" | "moveFrom" | "moveTo" => {
                    let kind = match node.tag_name().name() {
                        "ins" => RevisionKind::Insert,
                        "del" => RevisionKind::Delete,
                        "moveTo" => RevisionKind::MoveTo,
                        "moveFrom" => RevisionKind::MoveFrom,
                        _ => RevisionKind::Insert,
                    };
                    let revision = Revision { kind, author: wattr(node, "author").unwrap_or("").to_string(), date: wattr(node, "date").unwrap_or("").to_string() };
                    let outer = self.revision.replace(revision);
                    self.inline(node, rels, base, field, read, out, link);
                    self.revision = outer;
                }
                "smartTag" | "customXml" | "fldSimple" | "bdo" | "dir" => {
                    let is_fld_simple = node.tag_name().name() == "fldSimple";
                    let simple = is_fld_simple.then(|| upcase_field_name(wattr(node, "instr").unwrap_or("").trim()));
                    let simple_locked = is_fld_simple && wattr(node, "fldLock").map_or(false, |v| v == "1" || v == "true");
                    let page = simple.as_deref().is_some_and(|i| i.split_whitespace().next() == Some("PAGE") && !computed_field(i));
                    // Word writes a simple field's result again in the
                    // paragraph's formatting (it has no code's run to take
                    // it from), unless \* MERGEFORMAT keeps the result's.
                    let merge = simple.as_deref().is_some_and(|i| i.to_ascii_uppercase().contains("MERGEFORMAT"));
                    let result_rpr = || node.descendants().find(|n| n.is_element() && n.tag_name().name() == "rPr").filter(|_| merge);
                    if let Some(instruction) = simple.clone().filter(|i| computed_field(i)) {
                        // A field computed when shown (its result kept as
                        // what it showed when saved).
                        let rpr = result_rpr();
                        let direct = rpr.map(|r| run_format(r, self.theme)).unwrap_or_default();
                        let char_style = rpr.and_then(|r| wval(r, "rStyle")).map(str::to_string);
                        let style = self.styles.run_format(base, None, &direct).style();
                        let shown: String = node.descendants().filter(|n| n.is_element() && n.tag_name().name() == "t").filter_map(|n| n.text()).collect();
                        let field_link = link.clone().or_else(|| {
                            ibis_doc_model::field_format::parse_hyperlink_field(&instruction).and_then(|info| {
                                if !info.target.is_empty() {
                                    Some(ibis_doc_model::Link::Url(info.target))
                                } else {
                                    info.sub_address.map(ibis_doc_model::Link::Anchor)
                                }
                            })
                        });
                        out.push(FlowRun { kind: FlowRunKind::Field { instruction, shown }, style, format: direct, char_style, link: field_link, revision: None, show_field_code: false, field_locked: simple_locked, control: None });
                        read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                    } else if page {
                        let rpr = result_rpr();
                        let direct = rpr.map(|r| run_format(r, self.theme)).unwrap_or_default();
                        let char_style = rpr.and_then(|r| wval(r, "rStyle")).map(str::to_string);
                        let style = self.styles.run_format(base, None, &direct).style();
                        out.push(FlowRun { kind: FlowRunKind::PageNumber, style, format: direct, char_style, link: link.clone(), revision: None, show_field_code: false, field_locked: simple_locked, control: None });
                        read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                    } else {
                        self.inline(node, rels, base, field, read, out, link);
                    }
                }
                "commentRangeStart" | "commentRangeEnd" if field.frames.iter().all(|f| f.showing && !f.replaced) => {
                    let Some(id) = wattr(node, "id").and_then(|v| v.parse::<u32>().ok()) else { continue };
                    let kind = if node.tag_name().name() == "commentRangeStart" { FlowRunKind::CommentStart(id) } else { FlowRunKind::CommentEnd(id) };
                    let style = self.styles.run_format(base, None, &RunFormat::default()).style();
                    out.push(FlowRun { kind, style, format: RunFormat::default(), char_style: None, link: link.clone(), revision: None, show_field_code: false, field_locked: false, control: None });
                    read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                }
                "sdt" => {
                    if let Some(pr) = child(node, "sdtPr")
                        && let Some(cb) = child(pr, "checkbox")
                    {
                        let checked = child(cb, "checked")
                            .map_or(false, |c| attr(c, "val").map_or(true, |v| matches!(v, "1" | "true" | "on")));
                        let checked_char = child(cb, "checkedState")
                            .and_then(|s| attr(s, "val"))
                            .and_then(|v| u32::from_str_radix(v, 16).ok())
                            .and_then(char::from_u32)
                            .unwrap_or('\u{2611}');
                        let unchecked_char = child(cb, "uncheckedState")
                            .and_then(|s| attr(s, "val"))
                            .and_then(|v| u32::from_str_radix(v, 16).ok())
                            .and_then(char::from_u32)
                            .unwrap_or('\u{2610}');
                        let content = child(node, "sdtContent");
                        let content_rpr = content.and_then(|c| c.descendants().find(|d| d.is_element() && d.tag_name().name() == "rPr"));
                        let mut format = RunFormat::default();
                        if let Some(rpr) = content_rpr {
                            format.overlay(&run_format(rpr, self.theme));
                        }
                        let char_style = content_rpr.and_then(|x| wval(x, "rStyle")).map(str::to_string);
                        let style = self.styles.run_format(base, char_style.as_deref(), &format).style();
                        let kind = FlowRunKind::CheckBox {
                            checked,
                            checked_char,
                            unchecked_char,
                        };
                        read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                        out.push(FlowRun { kind, style, format, char_style, link: link.clone(), revision: self.revision.clone(), show_field_code: false, field_locked: false, control: control_info(node) });
                    } else if let Some(pr) = child(node, "sdtPr")
                        && let Some(dd) = child(pr, "dropDownList").or_else(|| child(pr, "comboBox"))
                    {
                        let combo = dd.tag_name().name() == "comboBox";
                        let mut items = Vec::new();
                        for item in children(dd, "listItem") {
                            let display = attr(item, "displayText").unwrap_or("").to_string();
                            let value = attr(item, "value").unwrap_or(&display).to_string();
                            items.push((display, value));
                        }
                        let content = child(node, "sdtContent");
                        let content_text: String = content.map(|c| {
                            c.descendants()
                                .filter(|d| d.is_element() && d.tag_name().name() == "t")
                                .filter_map(|d| d.text())
                                .collect()
                        }).unwrap_or_default();
                        let selected = items.iter().position(|(d, _)| *d == content_text).unwrap_or(0);
                        let content_rpr = content.and_then(|c| c.descendants().find(|d| d.is_element() && d.tag_name().name() == "rPr"));
                        let mut format = RunFormat::default();
                        if let Some(rpr) = content_rpr {
                            format.overlay(&run_format(rpr, self.theme));
                        }
                        let char_style = content_rpr.and_then(|x| wval(x, "rStyle")).map(str::to_string);
                        let style = self.styles.run_format(base, char_style.as_deref(), &format).style();
                        let kind = FlowRunKind::DropDownList {
                            items,
                            selected,
                            combo,
                        };
                        read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                        out.push(FlowRun { kind, style, format, char_style, link: link.clone(), revision: self.revision.clone(), show_field_code: false, field_locked: false, control: control_info(node) });
                    } else if let Some(pr) = child(node, "sdtPr")
                        && let Some(date_el) = child(pr, "date")
                    {
                        let full_date = attr(date_el, "fullDate").unwrap_or("").to_string();
                        let date_format = child(date_el, "dateFormat").and_then(|x| wval(x, "val")).unwrap_or("yyyy/M/d").to_string();
                        let content = child(node, "sdtContent");
                        let content_text: String = content.map(|c| {
                            c.descendants()
                                .filter(|d| d.is_element() && d.tag_name().name() == "t")
                                .filter_map(|d| d.text())
                                .collect()
                        }).unwrap_or_default();
                        let text = if content_text.is_empty() {
                            if let Some(t) = ibis_doc_model::date_field::parse_iso8601(&full_date) {
                                ibis_doc_model::date_field::format_picture(&t, &date_format)
                            } else {
                                "日付を選択".to_string()
                            }
                        } else {
                            content_text
                        };
                        let content_rpr = content.and_then(|c| c.descendants().find(|d| d.is_element() && d.tag_name().name() == "rPr"));
                        let mut format = RunFormat::default();
                        if let Some(rpr) = content_rpr {
                            format.overlay(&run_format(rpr, self.theme));
                        }
                        let char_style = content_rpr.and_then(|x| wval(x, "rStyle")).map(str::to_string);
                        let style = self.styles.run_format(base, char_style.as_deref(), &format).style();
                        let kind = FlowRunKind::DatePicker {
                            full_date,
                            date_format,
                            text,
                        };
                        read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                        out.push(FlowRun { kind, style, format, char_style, link: link.clone(), revision: self.revision.clone(), show_field_code: false, field_locked: false, control: control_info(node) });
                    } else if let Some(pr) = child(node, "sdtPr")
                        && child(pr, "text").is_some()
                    {
                        let content = child(node, "sdtContent");
                        let content_text: String = content.map(|c| {
                            c.descendants()
                                .filter(|d| d.is_element() && d.tag_name().name() == "t")
                                .filter_map(|d| d.text())
                                .collect()
                        }).unwrap_or_default();
                        let showing = child(pr, "showingPlcHdr").is_some() || content_text.is_empty();
                        let text = if content_text.is_empty() {
                            ibis_doc_model::flow::DEFAULT_CONTROL_PROMPT.to_string()
                        } else {
                            content_text
                        };
                        let content_rpr = content.and_then(|c| c.descendants().find(|d| d.is_element() && d.tag_name().name() == "rPr"));
                        let mut format = RunFormat::default();
                        if let Some(rpr) = content_rpr {
                            format.overlay(&run_format(rpr, self.theme));
                        }
                        let char_style = content_rpr.and_then(|x| wval(x, "rStyle")).map(str::to_string);
                        let style = self.styles.run_format(base, char_style.as_deref(), &format).style();
                        let kind = FlowRunKind::PlainText { text, placeholder: showing };
                        read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                        out.push(FlowRun { kind, style, format, char_style, link: link.clone(), revision: self.revision.clone(), show_field_code: false, field_locked: false, control: control_info(node) });
                    } else if let Some(pr) = child(node, "sdtPr")
                        && child(pr, "picture").is_some()
                    {
                        let content = child(node, "sdtContent");
                        let showing = child(pr, "showingPlcHdr").is_some();
                        let mut temp_runs = Vec::new();
                        let mut sub_read = ParaRead { p: read.p, atoms: Vec::new(), field_start: None, field_end: false };
                        if let Some(c) = content {
                            self.inline(c, rels, base, field, &mut sub_read, &mut temp_runs, link);
                        }
                        if let Some(mut pic_run) = temp_runs.into_iter().find(|r| matches!(r.kind, FlowRunKind::Picture { .. } | FlowRunKind::Shape(_))) {
                            if let FlowRunKind::Picture { media, size, look } = pic_run.kind {
                                pic_run.kind = FlowRunKind::PictureControl { media, size, look, placeholder: showing };
                                pic_run.control = control_info(node);
                                read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                                out.push(pic_run);
                            } else if let FlowRunKind::Shape(shape) = pic_run.kind {
                                if let ibis_doc_model::ShapeKind::Picture(pic) = shape.kind {
                                    let look = ibis_doc_model::PictureLook {
                                        crop: pic.crop,
                                        stroke: pic.stroke,
                                        luminance: pic.luminance,
                                        duotone: pic.duotone,
                                        alt_text: pic.alt_text,
                                        decorative: pic.decorative,
                                        name: pic.name,
                                        effects: pic.effects,
                                        soft_edge: pic.soft_edge,
                                        geometry: pic.geometry,
                                        ..Default::default()
                                    };
                                    let size = ibis_doc_model::EmuSize::new(shape.frame.width, shape.frame.height);
                                    pic_run.kind = FlowRunKind::PictureControl { media: pic.media, size, look, placeholder: showing };
                                    pic_run.control = control_info(node);
                                    read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                                    out.push(pic_run);
                                }
                            }
                        } else {
                            let data = ibis_doc_model::DEFAULT_PICTURE_CONTROL_PLACEHOLDER_PNG.to_vec();
                            let media = self.presentation.add_media(ibis_doc_model::MediaFormat::Png, data);
                            let size = ibis_doc_model::EmuSize::new(
                                ibis_doc_model::DEFAULT_PICTURE_CONTROL_WIDTH_EMU,
                                ibis_doc_model::DEFAULT_PICTURE_CONTROL_HEIGHT_EMU,
                            );
                            read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                            out.push(FlowRun {
                                kind: FlowRunKind::PictureControl {
                                    media,
                                    size,
                                    look: ibis_doc_model::PictureLook::default(),
                                    placeholder: true,
                                },
                                style: self.styles.run_format(base, None, &RunFormat::default()).style(),
                                format: RunFormat::default(),
                                char_style: None,
                                link: link.clone(),
                                revision: self.revision.clone(),
                                show_field_code: false,
                                field_locked: false,
                                control: control_info(node),
                            });
                        }
                    } else if let Some(pr) = child(node, "sdtPr")
                        && let Some(dp) = child(pr, "docPartList").or_else(|| child(pr, "docPartObj"))
                    {
                        let gallery = wval(dp, "docPartGallery").unwrap_or(ibis_doc_model::DEFAULT_BUILDING_BLOCK_GALLERY_NAME).to_string();
                        let category = wval(dp, "docPartCategory").unwrap_or(ibis_doc_model::DEFAULT_BUILDING_BLOCK_GALLERY_CATEGORY).to_string();
                        let content = child(node, "sdtContent");
                        let content_text: String = content.map(|c| {
                            c.descendants()
                                .filter(|d| d.is_element() && d.tag_name().name() == "t")
                                .filter_map(|d| d.text())
                                .collect()
                        }).unwrap_or_default();
                        let showing = child(pr, "showingPlcHdr").is_some() || content_text.is_empty();
                        let text = if content_text.is_empty() {
                            ibis_doc_model::DEFAULT_BUILDING_BLOCK_GALLERY_PROMPT.to_string()
                        } else {
                            content_text
                        };
                        let content_rpr = content.and_then(|c| c.descendants().find(|d| d.is_element() && d.tag_name().name() == "rPr"));
                        let mut format = RunFormat::default();
                        if let Some(rpr) = content_rpr {
                            format.overlay(&run_format(rpr, self.theme));
                        }
                        let char_style = content_rpr.and_then(|x| wval(x, "rStyle")).map(str::to_string);
                        let style = self.styles.run_format(base, char_style.as_deref(), &format).style();
                        let kind = FlowRunKind::BuildingBlockGallery {
                            gallery,
                            category,
                            text,
                            placeholder: showing,
                        };
                        read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                        out.push(FlowRun { kind, style, format, char_style, link: link.clone(), revision: self.revision.clone(), show_field_code: false, field_locked: false, control: control_info(node) });
                    } else if let Some(pr) = child(node, "sdtPr")
                        && let Some(rep) = child(pr, "repeatingSection")
                    {
                        let section_title = child(rep, "sectionTitle")
                            .and_then(|st| wattr(st, "val").or_else(|| attr(st, "val")))
                            .unwrap_or(ibis_doc_model::DEFAULT_REPEATING_SECTION_TITLE)
                            .to_string();
                        let do_not_allow_insert_delete = child(rep, "doNotAllowInsertDelete").is_some();
                        let content = child(node, "sdtContent");
                        let mut items = Vec::new();
                        if let Some(c) = content {
                            for child_sdt in c.children().filter(|ch| ch.is_element() && ch.tag_name().name() == "sdt") {
                                let item_pr = child(child_sdt, "sdtPr");
                                let is_item = item_pr.map_or(false, |p| child(p, "repeatingSectionItem").is_some());
                                if is_item {
                                    let item_content = child(child_sdt, "sdtContent");
                                    let text: String = item_content.map(|ic| {
                                        ic.descendants()
                                            .filter(|d| d.is_element() && d.tag_name().name() == "t")
                                            .filter_map(|d| d.text())
                                            .collect()
                                    }).unwrap_or_default();
                                    items.push(text);
                                }
                            }
                            if items.is_empty() {
                                let text: String = c.descendants()
                                    .filter(|d| d.is_element() && d.tag_name().name() == "t")
                                    .filter_map(|d| d.text())
                                    .collect();
                                if !text.is_empty() {
                                    items.push(text);
                                }
                            }
                        }
                        if items.is_empty() {
                            items.push(ibis_doc_model::DEFAULT_REPEATING_SECTION_ITEM_PROMPT.to_string());
                        }
                        let content_rpr = content.and_then(|c| c.descendants().find(|d| d.is_element() && d.tag_name().name() == "rPr"));
                        let mut format = RunFormat::default();
                        if let Some(rpr) = content_rpr {
                            format.overlay(&run_format(rpr, self.theme));
                        }
                        let char_style = content_rpr.and_then(|x| wval(x, "rStyle")).map(str::to_string);
                        let style = self.styles.run_format(base, char_style.as_deref(), &format).style();
                        let kind = FlowRunKind::RepeatingSection {
                            section_title,
                            items,
                            selected: 0,
                            do_not_allow_insert_delete,
                        };
                        read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                        out.push(FlowRun { kind, style, format, char_style, link: link.clone(), revision: self.revision.clone(), show_field_code: false, field_locked: false, control: control_info(node) });
                    } else if let Some(pr) = child(node, "sdtPr")
                        && child(pr, "group").is_some()
                    {
                        let content = child(node, "sdtContent");
                        let content_text: String = content.map(|c| {
                            c.descendants()
                                .filter(|d| d.is_element() && d.tag_name().name() == "t")
                                .filter_map(|d| d.text())
                                .collect()
                        }).unwrap_or_default();
                        let showing = child(pr, "showingPlcHdr").is_some() || content_text.is_empty();
                        let text = if content_text.is_empty() {
                            ibis_doc_model::DEFAULT_GROUP_PROMPT.to_string()
                        } else {
                            content_text
                        };
                        let content_rpr = content.and_then(|c| c.descendants().find(|d| d.is_element() && d.tag_name().name() == "rPr"));
                        let mut format = RunFormat::default();
                        if let Some(rpr) = content_rpr {
                            format.overlay(&run_format(rpr, self.theme));
                        }
                        let char_style = content_rpr.and_then(|x| wval(x, "rStyle")).map(str::to_string);
                        let style = self.styles.run_format(base, char_style.as_deref(), &format).style();
                        let kind = FlowRunKind::Group {
                            text,
                            placeholder: showing,
                        };
                        read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                        out.push(FlowRun { kind, style, format, char_style, link: link.clone(), revision: self.revision.clone(), show_field_code: false, field_locked: false, control: control_info(node) });
                    } else if let Some(pr) = child(node, "sdtPr")
                        && child(pr, "placeholder").is_some()
                        && child(pr, "docPartObj").is_none()
                        && child(pr, "group").is_none()
                        && child(pr, "citation").is_none()
                        && child(pr, "bibWord").is_none()
                    {
                        let content = child(node, "sdtContent");
                        let content_text: String = content.map(|c| {
                            c.descendants()
                                .filter(|d| d.is_element() && d.tag_name().name() == "t")
                                .filter_map(|d| d.text())
                                .collect()
                        }).unwrap_or_default();
                        let showing = child(pr, "showingPlcHdr").is_some() || content_text.is_empty();
                        let text = if content_text.is_empty() {
                            ibis_doc_model::flow::DEFAULT_CONTROL_PROMPT.to_string()
                        } else {
                            content_text
                        };
                        let content_rpr = content.and_then(|c| c.descendants().find(|d| d.is_element() && d.tag_name().name() == "rPr"));
                        let mut format = RunFormat::default();
                        if let Some(rpr) = content_rpr {
                            format.overlay(&run_format(rpr, self.theme));
                        }
                        let char_style = content_rpr.and_then(|x| wval(x, "rStyle")).map(str::to_string);
                        let style = self.styles.run_format(base, char_style.as_deref(), &format).style();
                        let kind = FlowRunKind::RichText { text, placeholder: showing };
                        read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                        out.push(FlowRun { kind, style, format, char_style, link: link.clone(), revision: self.revision.clone(), show_field_code: false, field_locked: false, control: control_info(node) });
                    } else if let Some(content) = child(node, "sdtContent") {
                        self.inline(content, rels, base, field, read, out, link);
                    }
                }
                "AlternateContent" => {
                    if let Some(branch) = child(node, "Choice").or_else(|| child(node, "Fallback")) {
                        self.inline(branch, rels, base, field, read, out, link);
                    }
                }
                "ruby" => {
                    let pr = child(node, "rubyPr");
                    let half = |name: &str| pr.and_then(|p| wval(p, name)).and_then(|v| v.parse::<u32>().ok()).map(|v| v * 50);
                    let text_of = |part: Option<Node>| -> String { part.map(|n| n.descendants().filter(|d| d.is_element() && matches!(d.tag_name().name(), "t")).filter_map(|d| d.text()).collect()).unwrap_or_default() };
                    let base_node = child(node, "rubyBase");
                    let base_text = text_of(base_node);
                    let base_rpr = base_node.and_then(|b| b.descendants().find(|d| d.is_element() && d.tag_name().name() == "rPr"));
                    let mut format = RunFormat::default();
                    if let Some(rpr) = base_rpr {
                        format.overlay(&run_format(rpr, self.theme));
                    }
                    let char_style = base_rpr.and_then(|x| wval(x, "rStyle")).map(str::to_string);
                    let style = self.styles.run_format(base, char_style.as_deref(), &format).style();
                    let size = half("hps").unwrap_or(style.size / 2);
                    let kind = FlowRunKind::Ruby {
                        base: base_text,
                        text: text_of(child(node, "rt")),
                        size,
                        raise: half("hpsRaise").unwrap_or(style.size),
                        align: ibis_doc_model::RubyAlign::from_name(pr.and_then(|p| wval(p, "rubyAlign")).unwrap_or("center")),
                    };
                    read.atoms.push(AtomSource::Field { nodes: vec![relative_path(node, read.p)] });
                    out.push(FlowRun { kind, style, format, char_style, link: link.clone(), revision: self.revision.clone(), show_field_code: false, field_locked: false, control: None });
                }
                _ => {}
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run<'e, 'j>(&mut self, r: Node<'e, 'j>, rels: &Rels, base: &RunFormat, field: &mut FieldState, read: &mut ParaRead<'e, 'j>, out: &mut Vec<FlowRun>, link: &Option<Link>) {
        let rpr = child(r, "rPr");
        let direct = rpr.map(|x| run_format(x, self.theme)).unwrap_or_default();
        let char_style = rpr.and_then(|x| wval(x, "rStyle")).map(str::to_string);
        let resolved = self.styles.run_format(base, char_style.as_deref(), &direct);
        let here = relative_path(r, read.p);
        for frame in &mut field.frames {
            frame.runs.push(here.clone());
        }
        let style = resolved.style();
        // Inserted or deleted (around it), or formatted differently.
        let revision = self.revision.clone().or_else(|| {
            let change = rpr.and_then(|x| child(x, "rPrChange"))?;
            let old = child(change, "rPr").map(|x| run_format(x, self.theme)).unwrap_or_default();
            Some(Revision { kind: RevisionKind::Format(old), author: wattr(change, "author").unwrap_or("").to_string(), date: wattr(change, "date").unwrap_or("").to_string() })
        });
        let make = |kind: FlowRunKind| FlowRun { kind, style: style.clone(), format: direct.clone(), char_style: char_style.clone(), link: link.clone(), revision: revision.clone(), show_field_code: false, field_locked: false, control: None };
        let push_text = |out: &mut Vec<FlowRun>, text: &str| {
            if text.is_empty() {
                return;
            }
            if let Some(FlowRun { kind: FlowRunKind::Text(t), style: s, format: f, char_style: c, link: l, revision: v, .. }) = out.last_mut()
                && *s == style
                && *f == direct
                && *c == char_style
                && l == link
                && *v == revision
            {
                t.push_str(text);
                return;
            }
            out.push(make(FlowRunKind::Text(text.to_string())));
        };
        for node in elements(r) {
            match node.tag_name().name() {
                "fldChar" => match wattr(node, "fldCharType") {
                    Some("begin") => {
                        let locked = wattr(node, "fldLock").map_or(false, |v| v == "1" || v == "true");
                        let form_field = child(node, "ffData").and_then(parse_ff_data);
                        field.frames.push(Frame { locked, form_field, runs: vec![relative_path(r, read.p)], ..Frame::default() });
                    }
                    Some("separate") => {
                        let empty = read.atoms.is_empty();
                        // A field in another's code (`= { PAGE } + 10`): its
                        // result is not shown, it goes into the code.
                        let in_code = field.frames.len() > 1 && field.frames[..field.frames.len() - 1].iter().any(|f| !f.showing);
                        let Some(frame) = field.frames.last_mut() else { continue };
                        frame.showing = true;
                        if in_code {
                            continue;
                        }
                        if let Some(ff) = frame.form_field.clone() {
                            match ff.kind {
                                ibis_doc_model::FormFieldKind::CheckBox { .. } | ibis_doc_model::FormFieldKind::DropDown { .. } => {
                                    frame.atom = Some(read.atoms.len());
                                    read.atoms.push(AtomSource::Field { nodes: Vec::new() });
                                    let mut run = make(FlowRunKind::FormField(ff));
                                    run.style.highlight = Some(ibis_doc_model::Rgb(0xd9d9d9));
                                    out.push(run);
                                    frame.form_field_emitted = true;
                                    frame.replaced = true;
                                }
                                ibis_doc_model::FormFieldKind::TextInput { .. } => {
                                    frame.replaced = true;
                                    frame.form_field_text = true;
                                    frame.atom = Some(read.atoms.len());
                                    read.atoms.push(AtomSource::Field { nodes: Vec::new() });
                                }
                            }
                            continue;
                        }
                        // Word does not tell the field names' cases apart (`page`, `Page`).
                        frame.instruction = upcase_field_name(&frame.instruction);
                        let mut words = frame.instruction.split_whitespace();
                        let computed = computed_field(&frame.instruction);
                        let locked = frame.locked;
                        match words.next() {
                            _ if computed => {
                                frame.atom = Some(read.atoms.len());
                                read.atoms.push(AtomSource::Field { nodes: Vec::new() });
                                let mut run = make(FlowRunKind::Field { instruction: frame.instruction.trim().to_string(), shown: String::new() });
                                run.format.enclosure_sizes = enclosure_sizes(&frame.instruction, &frame.instr_sizes);
                                run.style.enclosure_sizes = run.format.enclosure_sizes;
                                if run.link.is_none() {
                                    run.link = ibis_doc_model::field_format::parse_hyperlink_field(&frame.instruction).and_then(|info| {
                                        if !info.target.is_empty() {
                                            Some(ibis_doc_model::Link::Url(info.target))
                                        } else {
                                            info.sub_address.map(ibis_doc_model::Link::Anchor)
                                        }
                                    });
                                }
                                run.field_locked = locked;
                                out.push(run);
                                frame.page_ref = true;
                            }
                            Some("PAGE") => {
                                frame.atom = Some(read.atoms.len());
                                read.atoms.push(AtomSource::Field { nodes: Vec::new() });
                                let mut run = make(FlowRunKind::PageNumber);
                                run.field_locked = locked;
                                out.push(run);
                                frame.replaced = true;
                            }
                            Some("FILENAME") => {
                                frame.atom = Some(read.atoms.len());
                                read.atoms.push(AtomSource::Field { nodes: Vec::new() });
                                let mut run = make(FlowRunKind::FileName(String::new()));
                                run.field_locked = locked;
                                out.push(run);
                                frame.file_name = true;
                            }
                            Some("PAGEREF") => {
                                let bookmark = words.next().unwrap_or("").to_string();
                                frame.atom = Some(read.atoms.len());
                                read.atoms.push(AtomSource::Field { nodes: Vec::new() });
                                let mut run = make(FlowRunKind::PageRef { bookmark, shown: String::new() });
                                run.field_locked = locked;
                                out.push(run);
                                frame.page_ref = true;
                            }
                            Some("TOC" | "INDEX" | "TOA" | "BIBLIOGRAPHY") if empty => {
                                read.field_start = Some(frame.instruction.trim().to_string());
                                frame.toc = true;
                            }
                            _ => {}
                        }
                    }
                    Some("end") => {
                        if let Some(frame) = field.frames.pop() {
                            // A field in another's code: into its code.
                            if let Some(outer) = field.frames.last_mut()
                                && !outer.showing
                            {
                                outer.instruction.push_str(&format!("{{ {} }}", frame.instruction.trim()));
                            }
                            if let Some(mut ff) = frame.form_field {
                                if !frame.form_field_emitted {
                                    if let ibis_doc_model::FormFieldKind::TextInput { value, .. } = &mut ff.kind {
                                        let clean = frame.result_text.trim_matches('\u{a0}').trim().to_string();
                                        if !clean.is_empty() {
                                            *value = clean;
                                        }
                                    }
                                    if let Some(i) = frame.atom {
                                        let mut runs = frame.runs;
                                        runs.push(relative_path(r, read.p));
                                        runs.dedup();
                                        read.atoms[i] = AtomSource::Field { nodes: runs };
                                    } else {
                                        let mut runs = frame.runs;
                                        runs.push(relative_path(r, read.p));
                                        runs.dedup();
                                        read.atoms.push(AtomSource::Field { nodes: runs });
                                    }
                                    let mut run = make(FlowRunKind::FormField(ff));
                                    run.style.highlight = Some(ibis_doc_model::Rgb(0xd9d9d9));
                                    out.push(run);
                                }
                            } else if let Some(i) = frame.atom {
                                let mut runs = frame.runs;
                                runs.push(relative_path(r, read.p));
                                runs.dedup();
                                read.atoms[i] = AtomSource::Field { nodes: runs };
                            } else if !frame.showing
                                && field.frames.iter().all(|f| f.showing)
                                && upcase_field_name(&frame.instruction).split_whitespace().next() == Some("PAGE")
                                && !computed_field(&frame.instruction)
                            {
                                // A PAGE field with no w:fldCharType="separate" still displays the page number.
                                let mut runs = frame.runs;
                                runs.push(relative_path(r, read.p));
                                runs.dedup();
                                read.atoms.push(AtomSource::Field { nodes: runs });
                                let mut run = make(FlowRunKind::PageNumber);
                                run.field_locked = frame.locked;
                                out.push(run);
                            } else if !frame.showing
                                && field.frames.iter().all(|f| f.showing)
                                && (enclosed_field(&frame.instruction).is_some() || ibis_doc_model::field_format::overstrike(&frame.instruction).is_some() || ["XE ", "TC ", "TA "].iter().any(|f| frame.instruction.trim_start().starts_with(f)))
                            {
                                // 囲い文字 (EQ \o): a field without a result,
                                // one position.
                                let mut runs = frame.runs;
                                runs.push(relative_path(r, read.p));
                                runs.dedup();
                                read.atoms.push(AtomSource::Field { nodes: runs });
                                let mut run = make(FlowRunKind::Field { instruction: frame.instruction.trim().to_string(), shown: String::new() });
                                // 囲い文字: the sizes the runs of its code give the enclosure and the character.
                                run.format.enclosure_sizes = enclosure_sizes(&frame.instruction, &frame.instr_sizes);
                                run.style.enclosure_sizes = run.format.enclosure_sizes;
                                out.push(run);
                            }
                            if frame.toc {
                                read.field_end = true;
                            }
                        }
                    }
                    _ => {}
                },
                "instrText" => {
                    if let Some(frame) = field.frames.last_mut() {
                        frame.instruction.push_str(node.text().unwrap_or(""));
                        frame.instr_sizes.push((node.text().unwrap_or("").to_string(), style.size));
                        if frame.form_field.is_none() {
                            let trimmed = frame.instruction.trim_start();
                            if trimmed.starts_with("FORMCHECKBOX") {
                                frame.form_field = Some(ibis_doc_model::FormFieldData {
                                    name: String::new(),
                                    enabled: true,
                                    calc_on_exit: false,
                                    kind: ibis_doc_model::FormFieldKind::CheckBox {
                                        checked: false,
                                        default: false,
                                        size_auto: true,
                                    },
                                });
                            } else if trimmed.starts_with("FORMTEXT") {
                                frame.form_field = Some(ibis_doc_model::FormFieldData {
                                    name: String::new(),
                                    enabled: true,
                                    calc_on_exit: false,
                                    kind: ibis_doc_model::FormFieldKind::TextInput {
                                        value: String::new(),
                                        default: String::new(),
                                        max_length: 0,
                                    },
                                });
                            } else if trimmed.starts_with("FORMDROPDOWN") {
                                frame.form_field = Some(ibis_doc_model::FormFieldData {
                                    name: String::new(),
                                    enabled: true,
                                    calc_on_exit: false,
                                    kind: ibis_doc_model::FormFieldKind::DropDown {
                                        items: Vec::new(),
                                        selected: 0,
                                        default: 0,
                                    },
                                });
                            }
                        }
                    }
                }
                // The file name's, page reference's, and form field's saved text goes
                // into their fields.
                "t" if field.frames.last().is_some_and(|f| f.showing && (f.file_name || f.page_ref || f.form_field_text)) => {
                    let text = node.text().unwrap_or("");
                    if field.frames.last().is_some_and(|f| f.form_field_text) {
                        if let Some(frame) = field.frames.last_mut() {
                            frame.result_text.push_str(text);
                        }
                    } else {
                        match out.iter_mut().rev().find(|r| matches!(r.kind, FlowRunKind::FileName(_) | FlowRunKind::PageRef { .. } | FlowRunKind::Field { .. })) {
                            Some(FlowRun { kind: FlowRunKind::FileName(t), .. }) => t.push_str(text),
                            Some(FlowRun { kind: FlowRunKind::PageRef { shown, .. } | FlowRunKind::Field { shown, .. }, .. }) => shown.push_str(text),
                            _ => {}
                        }
                    }
                }
                // Text of a field's instructions, or a result replaced by
                // the field itself, is not shown.
                _ if field.frames.iter().any(|f| !f.showing || f.replaced) => {}
                "t" | "delText" => {
                    let raw = node.text().unwrap_or("");
                    let font = style.font.as_deref();
                    let mapped: String = raw
                        .chars()
                        .map(|c| {
                            if (0xf000..=0xf0ff).contains(&(c as u32)) {
                                symbol_char(c, font).unwrap_or_else(|| {
                                    if (0xf020..=0xf0ff).contains(&(c as u32)) { char::from_u32(c as u32 - 0xf000).unwrap_or(c) } else { c }
                                })
                            } else {
                                c
                            }
                        })
                        .collect();
                    read.in_run(r, node, mapped.chars().count());
                    push_text(out, &mapped);
                }
                "tab" => {
                    read.in_run(r, node, 1);
                    out.push(make(FlowRunKind::Tab));
                }
                "ptab" => {
                    let align = match wattr(node, "alignment") {
                        Some("right") => TabAlign::Right,
                        Some("center") => TabAlign::Center,
                        _ => TabAlign::Left,
                    };
                    let relative_to_margin = wattr(node, "relativeTo") != Some("indent");
                    let leader = wattr(node, "leader").is_some_and(|v| v != "none");
                    read.in_run(r, node, 1);
                    out.push(make(FlowRunKind::PositionalTab { align, relative_to_margin, leader }));
                }
                "br" => {
                    let kind = if wattr(node, "type") == Some("column") { FlowRunKind::ColumnBreak } else if wattr(node, "type") == Some("page") { FlowRunKind::PageBreak } else { FlowRunKind::LineBreak };
                    read.in_run(r, node, 1);
                    out.push(make(kind));
                }
                "cr" => {
                    read.in_run(r, node, 1);
                    out.push(make(FlowRunKind::LineBreak));
                }
                "noBreakHyphen" => {
                    read.in_run(r, node, 1);
                    push_text(out, "\u{2011}");
                }
                "footnoteReference" | "endnoteReference" => {
                    if let Some(id) = wattr(node, "id").and_then(|v| v.parse::<u32>().ok()) {
                        read.in_run(r, node, 1);
                        out.push(make(FlowRunKind::NoteRef { endnote: node.tag_name().name() == "endnoteReference", id }));
                    }
                }
                "commentReference" => {
                    if let Some(id) = wattr(node, "id").and_then(|v| v.parse::<u32>().ok()) {
                        read.in_run(r, node, 1);
                        out.push(make(FlowRunKind::CommentRef(id)));
                    }
                }
                "footnoteRef" | "endnoteRef" => {
                    read.in_run(r, node, 1);
                    out.push(make(FlowRunKind::NoteMark));
                }
                "sym" => {
                    if let Some(c) = wattr(node, "char").and_then(|c| u32::from_str_radix(c, 16).ok()).and_then(char::from_u32) {
                        // Symbol fonts map their glyphs into the private use area.
                        let c = symbol_char(c, wattr(node, "font")).unwrap_or_else(|| {
                            if (0xf020..=0xf0ff).contains(&(c as u32)) { char::from_u32(c as u32 - 0xf000).unwrap_or(c) } else { c }
                        });
                        read.in_run(r, node, 1);
                        push_text(out, &c.to_string());
                    }
                }
                // ルビ: its base (in the base's runs' formatting) and the
                // text over it, one position.
                "ruby" => {
                    let pr = child(node, "rubyPr");
                    let half = |name: &str| pr.and_then(|p| wval(p, name)).and_then(|v| v.parse::<u32>().ok()).map(|v| v * 50);
                    let text_of = |part: Option<Node>| -> String { part.map(|n| n.descendants().filter(|d| d.is_element() && matches!(d.tag_name().name(), "t")).filter_map(|d| d.text()).collect()).unwrap_or_default() };
                    let base_node = child(node, "rubyBase");
                    let base_text = text_of(base_node);
                    let base_rpr = base_node.and_then(|b| b.descendants().find(|d| d.is_element() && d.tag_name().name() == "rPr"));
                    let mut format = direct.clone();
                    if let Some(rpr) = base_rpr {
                        format.overlay(&run_format(rpr, self.theme));
                    }
                    let char_style = base_rpr.and_then(|x| wval(x, "rStyle")).map(str::to_string).or_else(|| char_style.clone());
                    let style = self.styles.run_format(base, char_style.as_deref(), &format).style();
                    let size = half("hps").unwrap_or(style.size / 2);
                    let kind = FlowRunKind::Ruby {
                        base: base_text,
                        text: text_of(child(node, "rt")),
                        size,
                        raise: half("hpsRaise").unwrap_or(style.size),
                        align: ibis_doc_model::RubyAlign::from_name(pr.and_then(|p| wval(p, "rubyAlign")).unwrap_or("center")),
                    };
                    read.in_run(r, node, 1);
                    // The base's formatting is the run's (styles are
                    // resolved from it again).
                    out.push(FlowRun { kind, style, format, char_style, link: link.clone(), revision: revision.clone(), show_field_code: false, field_locked: false, control: None });
                }
                "drawing" => {
                    let mut drawings = self.drawings(node, rels);
                    if !drawings.is_empty() {
                        for drawing in &mut drawings {
                            // A picture that is the result of an `INCLUDEPICTURE "file"` field remembers the file.
                            if let FlowRunKind::Picture { look, .. } = drawing {
                                look.linked_file = field.frames.iter().rev().filter(|f| f.showing).find_map(|f| {
                                    let words: Vec<&str> = f.instruction.split_whitespace().collect();
                                    (words.first().is_some_and(|w| w.eq_ignore_ascii_case("INCLUDEPICTURE"))).then(|| words.get(1).map(|w| w.trim_matches('"').replace("\\\\", "/")))?
                                });
                            }
                        }
                        read.in_run(r, node, drawings.len());
                        for drawing in drawings {
                            out.push(make(drawing));
                        }
                    }
                }
                "pict" => {
                    if let Some(shape) = self.vml_picture(node, rels).or_else(|| self.vml_shape(node)) {
                        read.in_run(r, node, 1);
                        out.push(make(shape));
                    }
                }
                "AlternateContent" => {
                    if let Some(drawing) = child(node, "Choice").and_then(|c| child(c, "drawing")) {
                        let drawings = self.drawings(drawing, rels);
                        if !drawings.is_empty() {
                            read.in_run(r, node, drawings.len());
                            for d in drawings {
                                out.push(make(d));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn drawings(&mut self, drawing: Node, rels: &Rels) -> Vec<FlowRunKind> {
        if let Some(holder) = elements(drawing).find(|n| matches!(n.tag_name().name(), "inline" | "anchor"))
            && let Some(wgp) = holder.descendants().find(|n| n.is_element() && n.tag_name().name() == "wgp")
        {
            let grouped = self.group_shapes(holder, wgp, rels);
            if !grouped.is_empty() {
                return grouped;
            }
        }
        self.drawing(drawing, rels).into_iter().collect()
    }

    fn drawing(&mut self, drawing: Node, rels: &Rels) -> Option<FlowRunKind> {
        if let Some(picture) = self.picture(drawing, rels) {
            return Some(picture);
        }
        if let Some(chart) = self.chart(drawing, rels) {
            return Some(chart);
        }
        self.shape(drawing, rels)
    }

    /// A WordprocessingGroup (`wpg:wgp`, DOCX-FLOAT-016): its child shapes (`wps:wsp`, `wps:cxnSp`)
    /// transformed from the group's child space (`a:chOff`, `a:chExt`) into the anchor's space,
    /// and recorded in `self.shape_groups`.
    fn group_shapes(&mut self, holder: Node, wgp: Node, rels: &Rels) -> Vec<FlowRunKind> {
        let extent = child(holder, "extent");
        let cx = extent.and_then(|e| e.attribute("cx")).and_then(|v| v.parse().ok()).unwrap_or(0i64);
        let cy = extent.and_then(|e| e.attribute("cy")).and_then(|v| v.parse().ok()).unwrap_or(0i64);
        let base_floating = Self::parse_floating_anchor(holder).unwrap_or_else(|| ibis_doc_model::FloatingAnchor {
            relative_h: "column".to_string(),
            relative_v: "paragraph".to_string(),
            wrap: "none".to_string(),
            allow_overlap: true,
            layout_in_cell: true,
            relative_height: ibis_doc_model::FloatingAnchor::DEFAULT_RELATIVE_HEIGHT,
            ..Default::default()
        });
        let doc_pr = child(holder, "docPr");
        let group_id = doc_pr
            .and_then(|d| attr(d, "id"))
            .map(str::to_string)
            .unwrap_or_else(|| self.new_id().to_string());
        let group_name = doc_pr
            .and_then(|d| attr(d, "name"))
            .unwrap_or("")
            .to_string();

        let grp_xfrm = child(wgp, "grpSpPr").and_then(|p| child(p, "xfrm"));
        let ext_cx = grp_xfrm.and_then(|x| child(x, "ext")).and_then(|e| attr(e, "cx")).and_then(|v| v.parse().ok()).unwrap_or(cx);
        let ext_cy = grp_xfrm.and_then(|x| child(x, "ext")).and_then(|e| attr(e, "cy")).and_then(|v| v.parse().ok()).unwrap_or(cy);
        let dest_cx = if cx > 0 { cx } else { ext_cx };
        let dest_cy = if cy > 0 { cy } else { ext_cy };
        let dest_x = base_floating.offset_x;
        let dest_y = base_floating.offset_y;

        let ch_x = grp_xfrm.and_then(|x| child(x, "chOff")).and_then(|o| attr(o, "x")).and_then(|v| v.parse().ok()).unwrap_or(0i64);
        let ch_y = grp_xfrm.and_then(|x| child(x, "chOff")).and_then(|o| attr(o, "y")).and_then(|v| v.parse().ok()).unwrap_or(0i64);
        let ch_cx = grp_xfrm.and_then(|x| child(x, "chExt")).and_then(|e| attr(e, "cx")).and_then(|v| v.parse().ok()).unwrap_or(dest_cx);
        let ch_cy = grp_xfrm.and_then(|x| child(x, "chExt")).and_then(|e| attr(e, "cy")).and_then(|v| v.parse().ok()).unwrap_or(dest_cy);

        let mut runs = Vec::new();
        let mut members = Vec::new();
        let mut used_ids = std::collections::HashSet::new();
        for wsp in elements(wgp).filter(|n| matches!(n.tag_name().name(), "wsp" | "sp" | "cxnSp")) {
            let sp_pr = child(wsp, "spPr");
            let xfrm = sp_pr.and_then(|p| child(p, "xfrm"));
            let c_off_x = xfrm.and_then(|x| child(x, "off")).and_then(|o| attr(o, "x")).and_then(|v| v.parse().ok()).unwrap_or(ch_x);
            let c_off_y = xfrm.and_then(|x| child(x, "off")).and_then(|o| attr(o, "y")).and_then(|v| v.parse().ok()).unwrap_or(ch_y);
            let c_ext_cx = xfrm.and_then(|x| child(x, "ext")).and_then(|e| attr(e, "cx")).and_then(|v| v.parse().ok()).unwrap_or(dest_cx);
            let c_ext_cy = xfrm.and_then(|x| child(x, "ext")).and_then(|e| attr(e, "cy")).and_then(|v| v.parse().ok()).unwrap_or(dest_cy);

            let mapped_x = dest_x + if ch_cx > 0 && dest_cx > 0 { (c_off_x - ch_x) * dest_cx / ch_cx } else { c_off_x - ch_x };
            let mapped_y = dest_y + if ch_cy > 0 && dest_cy > 0 { (c_off_y - ch_y) * dest_cy / ch_cy } else { c_off_y - ch_y };
            let mapped_cx = if ch_cx > 0 && dest_cx > 0 { c_ext_cx * dest_cx / ch_cx } else { c_ext_cx };
            let mapped_cy = if ch_cy > 0 && dest_cy > 0 { c_ext_cy * dest_cy / ch_cy } else { c_ext_cy };

            let raw_id = child(wsp, "cNvPr")
                .and_then(|d| attr(d, "id"))
                .and_then(|v| v.parse::<u64>().ok())
                .filter(|&id| id > 0 && !used_ids.contains(&id))
                .unwrap_or_else(|| self.new_id() as u64);
            used_ids.insert(raw_id);
            let id = ShapeId(raw_id);
            let mut child_floating = base_floating.clone();
            child_floating.offset_x = mapped_x;
            child_floating.offset_y = mapped_y;
            let shape = self.shape_from_wsp(wsp, id, EmuRect::new(mapped_x, mapped_y, mapped_cx, mapped_cy), Some(child_floating), rels);
            members.push(ibis_doc_model::GroupMember::Shape(id));
            runs.push(FlowRunKind::Shape(Box::new(shape)));
        }
        if !members.is_empty() {
            self.shape_groups.push(ibis_doc_model::ShapeGroup {
                has_unmodelled_children: false,
                native_id: group_id,
                name: group_name,
                children: members,
            });
        }
        runs
    }

    /// A chart (`c:chart` in `a:graphicData`): its part read as
    /// PowerPoint's and Excel's are, drawn in its extent (inline, or
    /// floating where its anchor puts it).
    fn chart(&mut self, drawing: Node, rels: &Rels) -> Option<FlowRunKind> {
        let holder = elements(drawing).find(|n| matches!(n.tag_name().name(), "inline" | "anchor"))?;
        let data = holder.descendants().find(|n| n.is_element() && n.tag_name().name() == "graphicData")?;
        if !attr(data, "uri").is_some_and(|u| u.contains("/chart")) {
            return None;
        }
        let reference = data.descendants().find(|n| n.is_element() && n.tag_name().name() == "chart")?;
        let rid = reference.attributes().find(|a| a.name() == "id").map(|a| a.value())?;
        let part = rels.get(rid).filter(|r| !r.external)?.target.clone();
        let xml = self.package.read_xml(&part).ok()?;
        let doc = parse(&part, &xml).ok()?;
        let colors = crate::theme::Colors::new(self.theme, None, &[]);
        // As Excel draws a chart of the old defaults (its thin grey border, its
        // 10 pt text): Word draws them so (DCHT).
        let mut chart = crate::chart::read(doc.root_element(), &colors)?;
        chart.word_layout = true;
        let extent = child(holder, "extent");
        let cx = extent.and_then(|e| e.attribute("cx")).and_then(|v| v.parse().ok()).unwrap_or(0i64);
        let cy = extent.and_then(|e| e.attribute("cy")).and_then(|v| v.parse().ok()).unwrap_or(0i64);
        let floating = Self::parse_floating_anchor(holder);
        let (x, y) = floating.as_ref().map_or((0, 0), |f| (f.offset_x, f.offset_y));
        let id = child(holder, "docPr").and_then(|d| attr(d, "id")).and_then(|v| v.parse::<u64>().ok()).map(ShapeId).unwrap_or_else(|| ShapeId(self.new_id() as u64));
        Some(FlowRunKind::Shape(Box::new(Shape { id, frame: EmuRect::new(x, y, cx, cy), rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Chart(Box::new(chart)), floating })))
    }

fn parse_floating_anchor(holder: Node) -> Option<ibis_doc_model::FloatingAnchor> {
    if !holder.has_tag_name("anchor") {
        return None;
    }
    let pos_h = child(holder, "positionH");
    let relative_h = pos_h.and_then(|p| attr(p, "relativeFrom")).unwrap_or("column").to_string();
    let offset_x = pos_h.and_then(|p| child(p, "posOffset")).and_then(|o| o.text()).and_then(|t| t.trim().parse().ok()).unwrap_or(0);
    let align_h = pos_h.and_then(|p| child(p, "align")).and_then(|a| a.text()).map(|t| t.trim().to_string());

    let pos_v = child(holder, "positionV");
    let relative_v = pos_v.and_then(|p| attr(p, "relativeFrom")).unwrap_or("paragraph").to_string();
    let offset_y = pos_v.and_then(|p| child(p, "posOffset")).and_then(|o| o.text()).and_then(|t| t.trim().parse().ok()).unwrap_or(0);
    let align_v = pos_v.and_then(|p| child(p, "align")).and_then(|a| a.text()).map(|t| t.trim().to_string());

    let wrap_elem = child(holder, "wrapSquare")
        .or_else(|| child(holder, "wrapTight"))
        .or_else(|| child(holder, "wrapThrough"))
        .or_else(|| child(holder, "wrapTopAndBottom"));
    let wrap = if child(holder, "wrapSquare").is_some() {
        "square".to_string()
    } else if child(holder, "wrapTight").is_some() {
        "tight".to_string()
    } else if child(holder, "wrapThrough").is_some() {
        "through".to_string()
    } else if child(holder, "wrapTopAndBottom").is_some() {
        "topAndBottom".to_string()
    } else {
        "none".to_string()
    };
    let wrap_text = wrap_elem.and_then(|w| attr(w, "wrapText")).map(str::to_string);
    let behind_doc = attr(holder, "behindDoc").is_some_and(|v| v == "1" || v == "true");
    let dist_t = wrap_elem
        .and_then(|w| attr(w, "distT"))
        .or_else(|| attr(holder, "distT"))
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0);
    let dist_b = wrap_elem
        .and_then(|w| attr(w, "distB"))
        .or_else(|| attr(holder, "distB"))
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0);
    let dist_l = wrap_elem
        .and_then(|w| attr(w, "distL"))
        .or_else(|| attr(holder, "distL"))
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0);
    let dist_r = wrap_elem
        .and_then(|w| attr(w, "distR"))
        .or_else(|| attr(holder, "distR"))
        .and_then(|t| t.trim().parse().ok())
        .unwrap_or(0);

    let locked = attr(holder, "locked").is_some_and(|v| v == "1" || v == "true");
    let allow_overlap = attr(holder, "allowOverlap").map_or(true, |v| v != "0" && v != "false");
    let layout_in_cell = attr(holder, "layoutInCell").map_or(true, |v| v != "0" && v != "false");
    let relative_height = attr(holder, "relativeHeight").and_then(|v| v.parse::<u32>().ok()).unwrap_or(ibis_doc_model::FloatingAnchor::DEFAULT_RELATIVE_HEIGHT);
    let wrap_polygon = wrap_elem.and_then(|w| child(w, "wrapPolygon")).and_then(|poly| {
        let edited = attr(poly, "edited").is_some_and(|v| v == "1" || v == "true");
        let pt = |n: Node| {
            let x = attr(n, "x").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            let y = attr(n, "y").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            ibis_doc_model::WrapPoint::new(x, y)
        };
        let start = pt(child(poly, "start")?);
        let line_to: Vec<_> = children(poly, "lineTo").map(pt).collect();
        Some(ibis_doc_model::WrapPolygon::new(start, line_to, edited))
    });

    Some(ibis_doc_model::FloatingAnchor {
        offset_x,
        offset_y,
        align_h,
        align_v,
        relative_h,
        relative_v,
        wrap,
        behind_doc,
        dist_t,
        dist_b,
        dist_l,
        dist_r,
        wrap_text,
        locked,
        allow_overlap,
        layout_in_cell,
        relative_height,
        wrap_polygon,
        glue: Default::default(),
    })
}

    /// An inline picture (`w:drawing/wp:inline`) or anchored floating picture (`wp:anchor`).
    fn picture(&mut self, drawing: Node, rels: &Rels) -> Option<FlowRunKind> {
        let holder = elements(drawing).find(|n| matches!(n.tag_name().name(), "inline" | "anchor"))?;
        let extent = child(holder, "extent")?;
        let cx = extent.attribute("cx").and_then(|v| v.parse().ok()).unwrap_or(0);
        let cy = extent.attribute("cy").and_then(|v| v.parse().ok()).unwrap_or(0);
        let size = EmuSize::new(cx, cy);
        let blip = holder.descendants().find(|n| n.is_element() && n.tag_name().name() == "blip")?;
        let rid = blip_embed(blip)?;
        let rel = rels.get(rid).filter(|r| !r.external)?;
        let media = match self.media.get(&rel.target) {
            Some(id) => *id,
            None => {
                let data = self.package.read(&rel.target).ok()?;
                let id = self.presentation.add_media(MediaFormat::from_path(&rel.target), data);
                self.media.insert(rel.target.clone(), id);
                id
            }
        };
        let floating = Self::parse_floating_anchor(holder);
        let colors = crate::theme::Colors::new(self.theme, None, &[]);
        let mut look = self.picture_look(holder, &colors);
        let doc_pr = child(holder, "docPr").or_else(|| holder.descendants().find(|n| n.is_element() && n.tag_name().name() == "docPr"));
        let alt_title = doc_pr.and_then(|d| attr(d, "title"))
            .or_else(|| holder.descendants().find(|n| n.is_element() && n.tag_name().name() == "cNvPr").and_then(|c| attr(c, "title")))
            .unwrap_or("")
            .to_string();
        let alt_descr = doc_pr.and_then(|d| attr(d, "descr"))
            .or_else(|| holder.descendants().find(|n| n.is_element() && n.tag_name().name() == "cNvPr").and_then(|c| attr(c, "descr")))
            .unwrap_or("")
            .to_string();
        look.alt_text = ibis_doc_model::AltText {
            title: alt_title,
            description: alt_descr,
        };
        look.decorative = doc_pr.map_or(false, |d| {
            d.descendants().any(|n| n.is_element() && n.tag_name().name() == "decorative" && attr(n, "val").map_or(true, |v| v == "1" || v == "true"))
        });
        look.name = doc_pr.and_then(|d| attr(d, "name"))
            .or_else(|| holder.descendants().find(|n| n.is_element() && n.tag_name().name() == "cNvPr").and_then(|c| attr(c, "name")))
            .unwrap_or("")
            .to_string();
        if floating.is_some() {
            let id = doc_pr
                .and_then(|d| attr(d, "id"))
                .and_then(|v| v.parse::<u64>().ok())
                .map(ShapeId)
                .unwrap_or_else(|| ShapeId(self.new_id() as u64));
            let (pos_x, pos_y) = floating.as_ref().map_or((0, 0), |f| (f.offset_x, f.offset_y));
            let frame = EmuRect::new(pos_x, pos_y, cx, cy);
            let picture = ibis_doc_model::Picture {
                media,
                crop: look.crop,
                stroke: look.stroke.clone(),
                corner: 0,
                duotone: look.duotone,
                clear: None,
                luminance: look.luminance,
                clip: None,
                shadow: None,
                reflection: None,
                soft_edge: look.soft_edge,
                alt_text: look.alt_text.clone(),
                decorative: look.decorative,
                name: look.name.clone(),
                geometry: None,
                effects: look.effects, tile: None, fill_rect: None,
                artistic_effect: None,
            };
            let shape = Shape {
                id,
                frame,
                rotation: look.rotation,
                flip_h: look.flip_h,
                flip_v: look.flip_v,
                kind: ShapeKind::Picture(picture),
                floating,
            };
            Some(FlowRunKind::Shape(Box::new(shape)))
        } else {
            Some(FlowRunKind::Picture { media, size, look })
        }
    }

    /// How the picture is turned, flipped, cut and outlined: `pic:spPr/a:xfrm`'s rot,
    /// flipH and flipV, `pic:blipFill/a:srcRect` and `pic:spPr/a:ln`.
    fn picture_look(&self, holder: Node, colors: &crate::theme::Colors) -> ibis_doc_model::PictureLook {
        let mut look = ibis_doc_model::PictureLook::default();
        let pic = holder.descendants().find(|n| n.is_element() && n.tag_name().name() == "pic");
        if let Some(pic) = pic {
            let sp_pr = child(pic, "spPr");
            if let Some(xfrm) = sp_pr.and_then(|p| child(p, "xfrm")) {
                look.rotation = xfrm.attribute("rot").and_then(|v| v.parse::<i64>().ok()).map_or(0, |v| v.rem_euclid(21_600_000) as i32);
                let on = |n: &str| xfrm.attribute(n).is_some_and(|v| v == "1" || v == "true");
                (look.flip_h, look.flip_v) = (on("flipH"), on("flipV"));
            }
            if let Some(src) = child(pic, "blipFill").and_then(|b| child(b, "srcRect")) {
                let side = |n: &str| src.attribute(n).and_then(|v| v.parse::<i32>().ok()).unwrap_or(0);
                look.crop = ibis_doc_model::Crop { left: side("l"), top: side("t"), right: side("r"), bottom: side("b") };
            }
            if let Some(ln) = sp_pr.and_then(|p| child(p, "ln")) {
                look.stroke = self.shape_stroke(Some(ln), None, colors);
            }
            let blip = child(pic, "blipFill").and_then(|b| child(b, "blip"));
            look.luminance = blip.and_then(|b| child(b, "lum")).and_then(|l| {
                let value = |n: &str| l.attribute(n).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0).clamp(-100_000, 100_000) as i32;
                let (bright, contrast) = (value("bright"), value("contrast"));
                (bright != 0 || contrast != 0).then_some((bright, contrast))
            });
            look.duotone = blip.and_then(|b| child(b, "duotone")).and_then(|d| {
                let found: Vec<_> = elements(d).filter_map(|c| colors.resolve(c, None)).collect();
                Some((found.first()?.rgb, found.get(1)?.rgb))
            });
            look.effects.opacity = blip.and_then(|b| child(b, "alphaModFix")).and_then(|a| a.attribute("amt")).and_then(|v| v.parse::<i64>().ok()).map(|v| v.clamp(0, 100_000) as i32);
            look.effects.saturation = blip.and_then(|b| child(b, "satMod")).and_then(|s| s.attribute("amt")).and_then(|v| v.parse::<i64>().ok()).map(|v| v.clamp(0, 400_000) as i32);
            look.effects.bilevel = blip.and_then(|b| child(b, "biLevel")).and_then(|b| b.attribute("thresh")).and_then(|v| v.parse::<i64>().ok()).map(|v| v.clamp(0, 100_000) as i32);
            look.effects.grayscale = blip.and_then(|b| child(b, "grayscl")).is_some();
            look.soft_edge = sp_pr.and_then(|p| child(p, "effectLst")).and_then(|e| child(e, "softEdge")).and_then(|s| crate::xml::attr_i64(s, "rad")).filter(|&r| r > 0);
            look.geometry = sp_pr.and_then(|p| child(p, "prstGeom")).map(|g| crate::geometry::geometry(Some(g))).filter(|g| !matches!(g, ibis_doc_model::Geometry::Rect));
        }
        look
    }

    fn shape(&mut self, drawing: Node, rels: &Rels) -> Option<FlowRunKind> {
        let holder = elements(drawing).find(|n| matches!(n.tag_name().name(), "inline" | "anchor"))?;
        let extent = child(holder, "extent");
        let mut cx = extent.and_then(|e| e.attribute("cx")).and_then(|v| v.parse().ok()).unwrap_or(0i64);
        let mut cy = extent.and_then(|e| e.attribute("cy")).and_then(|v| v.parse().ok()).unwrap_or(0i64);

        let wsp = holder.descendants().find(|n| n.is_element() && matches!(n.tag_name().name(), "wsp" | "sp" | "cxnSp"))?;
        let sp_pr = child(wsp, "spPr");
        let xfrm = sp_pr.and_then(|p| child(p, "xfrm"));
        if cx == 0 || cy == 0 {
            if let Some(ext) = xfrm.and_then(|x| child(x, "ext")) {
                if cx == 0 {
                    cx = ext.attribute("cx").and_then(|v| v.parse().ok()).unwrap_or(0);
                }
                if cy == 0 {
                    cy = ext.attribute("cy").and_then(|v| v.parse().ok()).unwrap_or(0);
                }
            }
        }
        let floating = Self::parse_floating_anchor(holder);
        let (pos_x, pos_y) = floating.as_ref().map_or((0, 0), |f| (f.offset_x, f.offset_y));
        let frame = EmuRect::new(pos_x, pos_y, cx, cy);
        let doc_pr = child(holder, "docPr");
        let id = doc_pr
            .and_then(|d| attr(d, "id"))
            .and_then(|v| v.parse::<u64>().ok())
            .map(ShapeId)
            .unwrap_or_else(|| ShapeId(self.new_id() as u64));
        let shape = self.shape_from_wsp(wsp, id, frame, floating, rels);
        Some(FlowRunKind::Shape(Box::new(shape)))
    }

    fn shape_from_wsp(&mut self, wsp: Node, id: ShapeId, frame: EmuRect, mut floating: Option<ibis_doc_model::FloatingAnchor>, rels: &Rels) -> Shape {
        let sp_pr = child(wsp, "spPr");
        let xfrm = sp_pr.and_then(|p| child(p, "xfrm"));
        let rotation = xfrm.and_then(|x| attr(x, "rot")).and_then(|v| v.parse().ok()).unwrap_or(0);
        let flip_h = xfrm.and_then(|x| attr(x, "flipH")).is_some_and(|v| v == "1" || v == "true");
        let flip_v = xfrm.and_then(|x| attr(x, "flipV")).is_some_and(|v| v == "1" || v == "true");

        let style = child(wsp, "style");
        let colors = crate::theme::Colors::new(self.theme, None, &[]);

        let geometry_node = sp_pr.and_then(|p| child(p, "prstGeom").or_else(|| child(p, "custGeom")));
        let geometry = crate::geometry::geometry(geometry_node);

        let fill = sp_pr.and_then(|p| self.shape_fill(p, style, &colors, rels));
        let stroke = self.shape_stroke(sp_pr.and_then(|p| child(p, "ln")), style, &colors);

        let text = self.shape_text(wsp, &colors);

        let shadow = sp_pr
            .and_then(|p| child(p, "effectLst"))
            .and_then(|e| child(e, "outerShdw"))
            .and_then(|s| {
                Some(ibis_doc_model::Shadow::simple(
                    colors.in_parent(s, None)?,
                    crate::xml::attr_i64(s, "blurRad").unwrap_or(0),
                    crate::xml::attr_i64(s, "dist").unwrap_or(0),
                    crate::xml::attr_i64(s, "dir").unwrap_or(0) as i32,
                ))
            });

        // A glow, soft edges and a reflection (`a:effectLst`).
        let effect_list = sp_pr.and_then(|p| child(p, "effectLst"));
        let number = |n: Node, name: &str, default: i64| crate::xml::attr_i64(n, name).unwrap_or(default);
        let glow = effect_list.and_then(|e| child(e, "glow")).and_then(|g| {
            let radius = number(g, "rad", 0);
            (radius > 0).then(|| colors.in_parent(g, None).map(|c| (c, radius))).flatten()
        });
        let soft_edge = effect_list.and_then(|e| child(e, "softEdge")).map(|e| number(e, "rad", 0)).filter(|&r| r > 0);
        let reflection = effect_list.and_then(|e| child(e, "reflection")).map(|e| ibis_doc_model::Reflection {
            blur: number(e, "blurRad", 0),
            start_alpha: number(e, "stA", 100_000) as i32,
            end_alpha: number(e, "endA", 0) as i32,
            start_pos: number(e, "stPos", 0) as i32,
            end_pos: number(e, "endPos", 100_000) as i32,
            distance: number(e, "dist", 0),
            direction: number(e, "dir", 0) as i32,
            fade_direction: number(e, "fadeDir", 5_400_000) as i32,
            sx: number(e, "sx", 100_000) as i32,
            sy: number(e, "sy", 100_000) as i32,
            kx: number(e, "kx", 0) as i32,
            ky: number(e, "ky", 0) as i32,
            align: attr(e, "algn").map(ibis_doc_model::TileAlign::parse).unwrap_or(ibis_doc_model::TileAlign::Bottom),
            rot_with_shape: attr(e, "rotWithShape").is_none_or(|v| v != "0" && v != "false"),
        });
        // A connector's glue: the shapes its ends are attached to (`a:stCxn`, `a:endCxn`).
        if let (Some(f), Some(cnv)) = (floating.as_mut(), child(wsp, "cNvCnPr")) {
            let end_of = |name: &str| child(cnv, name).and_then(|c| Some((attr(c, "id")?.parse::<u64>().ok()?, attr(c, "idx")?.parse::<u32>().ok()?)));
            f.glue = ibis_doc_model::Glue { start: end_of("stCxn"), end: end_of("endCxn") };
        }
        let preset = geometry_node.filter(|g| g.has_tag_name("prstGeom")).and_then(|g| attr(g, "prst"));
        let is_line = matches!(preset, Some("line" | "straightConnector1"))
            || (wsp.has_tag_name("cxnSp") && geometry_node.is_none());

        if is_line && let Some(s) = stroke {
            Shape { id, frame, rotation, flip_h, flip_v, kind: ShapeKind::Line(s), floating }
        } else {
            Shape {
                id,
                frame,
                rotation,
                flip_h,
                flip_v,
                kind: ShapeKind::Auto(AutoShape {
                    geometry,
                    fill: fill.unwrap_or(Fill::None),
                    stroke,
                    text,
                    tilt: None,
                    shadow,
                    glow,
                    inner_shadow: None,
                    soft_edge,
                    reflection,
                    bevel: None,
                }),
                floating,
            }
        }
    }

    /// A VML shape (`w:pict` with a `v:rect`, `v:roundrect`, `v:oval` or `v:line`, the old drawing format), as a shape: its
    /// size, fill, outline, text box and place.
    /// A picture of the old (VML) kind: `w:pict/v:shape` with a `v:imagedata` that names the picture (a part of
    /// the package, a bitmap or a metafile) and a style of its own size (`width:450.75pt;height:258.75pt`), in
    /// the text. One placed at a place of its own is left to `vml_shape`.
    fn vml_picture(&mut self, pict: Node, rels: &Rels) -> Option<FlowRunKind> {
        let shape = elements(pict).find(|n| n.tag_name().name() == "shape")?;
        let data = child(shape, "imagedata")?;
        let style = vml_style(shape.attribute("style").unwrap_or(""));
        if style.get("position").is_some_and(|p| p == "absolute") {
            return None;
        }
        let length = |key: &str| style.get(key).and_then(|v| vml_length(v)).filter(|v| *v > 0);
        let size = EmuSize::new(length("width")?, length("height")?);
        let rel = rels.get(rattr(data, "id")?).filter(|r| !r.external)?;
        let media = match self.media.get(&rel.target) {
            Some(id) => *id,
            None => {
                let bytes = self.package.read(&rel.target).ok()?;
                let id = self.presentation.add_media(MediaFormat::from_path(&rel.target), bytes);
                self.media.insert(rel.target.clone(), id);
                id
            }
        };
        Some(FlowRunKind::Picture { media, size, look: ibis_doc_model::PictureLook::default() })
    }

    fn vml_shape(&mut self, pict: Node) -> Option<FlowRunKind> {
        let vml = elements(pict).find(|n| matches!(n.tag_name().name(), "rect" | "roundrect" | "oval" | "line" | "polyline" | "shape"))?;
        let name = vml.tag_name().name();
        let type_id = vml.attribute("type").map(|t| t.trim_start_matches('#')).filter(|t| !t.is_empty());
        let shapetype = type_id.and_then(|id| {
            elements(pict)
                .find(|n| n.tag_name().name() == "shapetype" && n.attribute("id") == Some(id))
                .or_else(|| {
                    pict.document()
                        .descendants()
                        .find(|n| n.is_element() && n.tag_name().name() == "shapetype" && n.attribute("id") == Some(id))
                })
        });
        let vml_attr = |key: &str| vml.attribute(key).or_else(|| shapetype.and_then(|st| st.attribute(key)));
        let style = vml_style(vml.attribute("style").unwrap_or(""));
        let length = |key: &str| style.get(key).and_then(|v| vml_length(v));
        let point = |v: Option<&str>| -> Option<(i64, i64)> {
            let (x, y) = v?.split_once(',')?;
            Some((vml_length(x)?, vml_length(y)?))
        };
        let (width, height);
        let (mut flip_h, mut flip_v) = (false, false);
        let mut line_origin = (0i64, 0i64);
        let mut polyline: Option<(Vec<(i64, i64)>, (i64, i64))> = None;
        if name == "line" {
            let (from, to) = (point(vml.attribute("from"))?, point(vml.attribute("to"))?);
            line_origin = (from.0.min(to.0), from.1.min(to.1));
            (width, height) = ((to.0 - from.0).abs(), (to.1 - from.1).abs());
            (flip_h, flip_v) = (to.0 < from.0, to.1 < from.1);
        } else if name == "polyline" {
            // The points are the shape's own: its size is theirs.
            let nums: Vec<i64> = vml.attribute("points")?.split(',').filter_map(vml_length).collect();
            let points: Vec<(i64, i64)> = nums.chunks_exact(2).map(|c| (c[0], c[1])).collect();
            let (min_x, max_x) = (points.iter().map(|p| p.0).min()?, points.iter().map(|p| p.0).max()?);
            let (min_y, max_y) = (points.iter().map(|p| p.1).min()?, points.iter().map(|p| p.1).max()?);
            (width, height) = ((max_x - min_x).max(1), (max_y - min_y).max(1));
            polyline = Some((points, (min_x, min_y)));
        } else {
            width = length("width")?;
            height = length("height")?;
        }
        let off = |v: &str| matches!(v, "f" | "false" | "0" | "no");
        let color = |key: &str, default: Option<u32>| vml_attr(key).and_then(vml_color).or(default.map(ibis_doc_model::Rgb));
        // The fill (white unless it is said otherwise) and the outline (black, 0.75 pt).
        let fill_el = child(vml, "fill").or_else(|| shapetype.and_then(|st| child(st, "fill")));
        let filled = !vml_attr("filled").is_some_and(off) && !fill_el.and_then(|f| f.attribute("on")).is_some_and(off);
        let fill = if name == "line" || !filled {
            Fill::None
        } else {
            let first = color("fillcolor", Some(0xFFFFFF)).unwrap_or(ibis_doc_model::Rgb(0xFFFFFF));
            let second = fill_el.and_then(|f| f.attribute("color2")).and_then(vml_color);
            let kind = fill_el.and_then(|f| f.attribute("type")).unwrap_or("");
            let alpha = fill_el.and_then(|f| f.attribute("opacity")).map_or(255, vml_opacity);
            let with = |rgb: ibis_doc_model::Rgb| ibis_doc_model::Color { rgb, alpha };
            let stop = |position: u32, rgb| ibis_doc_model::GradientStop { position, color: with(rgb) };
            match (kind.starts_with("gradient"), second) {
                (true, Some(second)) if kind == "gradientRadial" => Fill::Path { path: ibis_doc_model::GradientPath::Rect, focus: [50_000; 4], stops: vec![stop(0, second), stop(100_000, first)] },
                (true, Some(second)) => {
                    // VML's angle is clockwise from "up", the first colour at the far end: OOXML's is from the left.
                    let degrees = fill_el.and_then(|f| f.attribute("angle")).and_then(|a| a.parse::<f64>().ok()).unwrap_or(0.0);
                    let angle = ((270.0 - degrees).rem_euclid(360.0) * 60_000.0) as i32;
                    // A focus of 50%: the second colour in the middle, the first at both ends.
                    let centred = fill_el.and_then(|f| f.attribute("focus")).is_some_and(|f| f.trim() == "50%");
                    let stops = if centred { vec![stop(0, first), stop(50_000, second), stop(100_000, first)] } else { vec![stop(0, first), stop(100_000, second)] };
                    Fill::Linear { angle, scaled: false, stops }
                }
                _ => Fill::Solid(with(first)),
            }
        };
        let stroke_el = child(vml, "stroke").or_else(|| shapetype.and_then(|st| child(st, "stroke")));
        let stroked = !vml_attr("stroked").is_some_and(off) && !stroke_el.and_then(|s| s.attribute("on")).is_some_and(off);
        let stroke = stroked.then(|| {
            let stroke_color = color("strokecolor", Some(0x000000)).unwrap_or(ibis_doc_model::Rgb(0));
            let weight = vml_attr("strokeweight").and_then(vml_length).or_else(|| stroke_el.and_then(|s| s.attribute("weight")).and_then(vml_length)).unwrap_or(9525);
            let dash = stroke_el.and_then(|s| s.attribute("dashstyle")).map_or(LineDash::Solid, |d| match d {
                "dot" => LineDash::SysDot,
                "dash" => LineDash::SysDash,
                "shortdash" => LineDash::SysDash,
                "shortdot" => LineDash::SysDot,
                "dashdot" | "shortdashdot" => LineDash::DashDot,
                "longdash" => LineDash::LgDash,
                "longdashdot" => LineDash::LgDashDot,
                "longdashdotdot" | "shortdashdotdot" => LineDash::LgDashDotDot,
                _ => LineDash::Solid,
            });
            let end = |kind: &str, w: &str, l: &str| {
                let size = |v: Option<&str>| match v {
                    Some("narrow") | Some("short") => Some("sm"),
                    Some("wide") | Some("long") => Some("lg"),
                    _ => Some("med"),
                };
                let ooxml = match stroke_el.and_then(|s| s.attribute(kind)) {
                    Some("block") => "triangle",
                    Some("classic") => "stealth",
                    Some("open") => "arrow",
                    Some("oval") => "oval",
                    Some("diamond") => "diamond",
                    _ => "",
                };
                ibis_doc_model::LineEnd::parse(Some(ooxml), size(stroke_el.and_then(|s| s.attribute(w))), size(stroke_el.and_then(|s| s.attribute(l))))
            };
            let compound = match stroke_el.and_then(|s| s.attribute("linestyle")) {
                Some("thinThin") => ibis_doc_model::LineCompound::Double,
                Some("thinThick") => ibis_doc_model::LineCompound::ThinThick,
                Some("thickThin") => ibis_doc_model::LineCompound::ThickThin,
                Some("thickBetweenThin") => ibis_doc_model::LineCompound::Triple,
                _ => ibis_doc_model::LineCompound::Single,
            };
            let cap = match stroke_el.and_then(|s| s.attribute("endcap")) {
                Some("round") => LineCap::Round,
                Some("square") => LineCap::Square,
                _ => LineCap::Flat,
            };
            let join = match stroke_el.and_then(|s| s.attribute("joinstyle")) {
                Some("round") => ibis_doc_model::LineJoin::Round,
                Some("bevel") => ibis_doc_model::LineJoin::Bevel,
                _ => ibis_doc_model::LineJoin::Miter,
            };
            Stroke {
                color: ibis_doc_model::Color::opaque(stroke_color),
                width: weight,
                dashed: dash != LineDash::Solid,
                head: end("startarrow", "startarrowwidth", "startarrowlength"),
                tail: end("endarrow", "endarrowwidth", "endarrowlength"),
                dash,
                cap,
                join,
                compound,
                alignment: ibis_doc_model::LineAlignment::Center,
                fill: None,
            }
        });
        let geometry = match name {
            "polyline" => {
                let (points, (min_x, min_y)) = polyline.take().unwrap_or_default();
                let path: Vec<VmlPath> = points.iter().enumerate().map(|(i, p)| if i == 0 { VmlPath::Move(p.0 - min_x, p.1 - min_y) } else { VmlPath::Line(p.0 - min_x, p.1 - min_y) }).collect();
                vml_custom_geometry(width, height, &path, filled)
            }
            "shape" => {
                let spt = attr(vml, "spt")
                    .or_else(|| shapetype.and_then(|st| attr(st, "spt")))
                    .and_then(|v| v.trim().parse::<u32>().ok())
                    .or_else(|| type_id.and_then(|id| id.strip_prefix("_x0000_t")).and_then(|v| v.parse::<u32>().ok()));
                let coordsize = || {
                    vml_attr("coordsize")
                        .and_then(|c| c.split_once(','))
                        .and_then(|(a, b)| Some((a.trim().parse::<i64>().ok()?, b.trim().parse::<i64>().ok()?)))
                        .unwrap_or((21_600, 21_600))
                };
                if let Some(p) = vml.attribute("path").filter(|p| !p.contains('@')) {
                    let (cw, ch) = coordsize();
                    let path = vml_path(p);
                    vml_custom_geometry(cw, ch, &path, filled)
                } else if let Some(preset_name) = spt.and_then(vml_spt_preset)
                    && let Some(geom) = crate::geometry::preset(preset_name)
                {
                    geom
                } else if let Some(p) = shapetype.and_then(|st| st.attribute("path")).filter(|p| !p.contains('@')) {
                    let (cw, ch) = coordsize();
                    let path = vml_path(p);
                    vml_custom_geometry(cw, ch, &path, filled)
                } else if child(vml, "textbox").is_some() {
                    ibis_doc_model::Geometry::Rect
                } else {
                    return None;
                }
            }
            "roundrect" => {
                let arc = vml.attribute("arcsize").map(|a| a.trim().trim_end_matches('f').parse::<f64>().map(|v| if a.trim().ends_with('f') { v / 65536.0 } else { v }).unwrap_or(0.2)).unwrap_or(0.2);
                ibis_doc_model::Geometry::RoundRect { radius: (arc.clamp(0.0, 0.5) * 100_000.0).round() as u32 }
            }
            "oval" => ibis_doc_model::Geometry::Ellipse,
            _ => ibis_doc_model::Geometry::Rect,
        };
        // 影: an offset copy in the shadow's colour.
        let shadow = child(vml, "shadow").filter(|sh| sh.attribute("on").is_some_and(|v| matches!(v, "t" | "true" | "1" | "on"))).and_then(|sh| {
            let (ox, oy) = point(Some(sh.attribute("offset").unwrap_or("2pt,2pt")))?;
            let rgb = sh.attribute("color").and_then(vml_color).unwrap_or(ibis_doc_model::Rgb(0x808080));
            let alpha = sh.attribute("opacity").map_or(255, vml_opacity);
            let distance = ((ox as f64).hypot(oy as f64)).round() as i64;
            let direction = ((oy as f64).atan2(ox as f64).to_degrees().rem_euclid(360.0) * 60_000.0) as i32;
            Some(ibis_doc_model::Shadow::simple(ibis_doc_model::Color { rgb, alpha }, 0, distance, direction))
        });
        let rotation = style.get("rotation").and_then(|r| r.trim().parse::<f64>().ok()).map_or(0, |d| (d.rem_euclid(360.0) * 60_000.0) as i32);
        if let Some(f) = style.get("flip") {
            flip_h |= f.contains('x');
            flip_v |= f.contains('y');
        }
        // A text box: its paragraphs, its margins and where the text sits.
        let colors = crate::theme::Colors::new(self.theme, None, &[]);
        let mut text = self.shape_text(vml, &colors);
        if let (Some(body), Some(tb)) = (text.as_mut(), child(vml, "textbox")) {
            if let Some(inset) = tb.attribute("inset") {
                let parts: Vec<Option<i64>> = inset.split(',').map(|p| if p.trim().is_empty() { None } else { vml_length(p) }).collect();
                let get = |i: usize, default: i64| parts.get(i).copied().flatten().unwrap_or(default);
                body.insets = Insets { left: get(0, 91440), top: get(1, 45720), right: get(2, 91440), bottom: get(3, 45720) };
            }
            // 文字列の方向 (`layout-flow`): turned a quarter turn, or the Japanese upright.
            let flow = tb.attribute("style").map(vml_style).unwrap_or_default();
            match flow.get("layout-flow").map(String::as_str) {
                Some("vertical") => body.sideways = if flow.get("mso-layout-flow-alt").is_some_and(|a| a == "bottom-to-top") { 16_200_000 } else { 5_400_000 },
                Some("vertical-ideographic") => {
                    body.vertical = true;
                    body.vertical_kind = ibis_doc_model::VerticalKind::EastAsian;
                }
                _ => {}
            }
            body.anchor = match style.get("v-text-anchor").map(String::as_str) {
                Some("middle") => VerticalAnchor::Middle,
                Some("bottom") => VerticalAnchor::Bottom,
                _ => VerticalAnchor::Top,
            };
        }
        // Where it is: in the text, or at a place on the page (`position:absolute`).
        let floating = (style.get("position").map(String::as_str) == Some("absolute")).then(|| ibis_doc_model::FloatingAnchor {
            offset_x: length("margin-left").unwrap_or(0) + line_origin.0,
            offset_y: length("margin-top").unwrap_or(0) + line_origin.1,
            relative_h: style.get("mso-position-horizontal-relative").cloned().unwrap_or_else(|| "margin".to_string()),
            relative_v: match style.get("mso-position-vertical-relative").map(String::as_str) {
                Some("text") | None => "paragraph".to_string(),
                Some(v) => v.to_string(),
            },
            wrap: "none".to_string(),
            behind_doc: style.get("z-index").and_then(|z| z.trim().parse::<i64>().ok()).is_some_and(|z| z < 0),
            ..Default::default()
        });
        let frame = EmuRect::new(floating.as_ref().map_or(0, |f| f.offset_x), floating.as_ref().map_or(0, |f| f.offset_y), width, height);
        let id = ShapeId(self.new_id() as u64);
        let shape = match (name, &stroke) {
            ("line", Some(s)) => Shape { id, frame, rotation, flip_h, flip_v, kind: ShapeKind::Line(s.clone()), floating },
            ("line", None) => return None,
            _ => Shape {
                id,
                frame,
                rotation,
                flip_h,
                flip_v,
                kind: ShapeKind::Auto(AutoShape { geometry, fill, stroke, text, tilt: None, shadow, glow: None, inner_shadow: None, soft_edge: None, reflection: None, bevel: None }),
                floating,
            },
        };
        Some(FlowRunKind::Shape(Box::new(shape)))
    }

    fn shape_fill(&mut self, sp_pr: Node, style: Option<Node>, colors: &crate::theme::Colors, rels: &Rels) -> Option<Fill> {
        let fill_el = elements(sp_pr).find(|n| matches!(n.tag_name().name(), "noFill" | "solidFill" | "gradFill" | "blipFill" | "pattFill"));
        if let Some(f) = fill_el {
            match f.tag_name().name() {
                "noFill" => return Some(Fill::None),
                "solidFill" => {
                    if let Some(c) = colors.in_parent(f, None) {
                        return Some(Fill::Solid(c));
                    }
                }
                "gradFill" => {
                    let mut stops: Vec<ibis_doc_model::GradientStop> = child(f, "gsLst")
                        .into_iter()
                        .flat_map(|l| children(l, "gs"))
                        .filter_map(|gs| {
                            let color = colors.in_parent(gs, None)?;
                            Some(ibis_doc_model::GradientStop {
                                position: crate::xml::attr_i64(gs, "pos").unwrap_or(0).clamp(0, 100_000) as u32,
                                color,
                            })
                        })
                        .collect();
                    stops.sort_by_key(|s| s.position);
                    if !stops.is_empty() {
                        let angle = child(f, "lin").and_then(|l| crate::xml::attr_i64(l, "ang")).unwrap_or(5_400_000) as i32;
                        let scaled = child(f, "lin").and_then(|l| l.attribute("scaled")).is_some_and(|v| v == "1" || v == "true");
                        return Some(Fill::Linear { angle, scaled, stops });
                    }
                }
                "blipFill" => {
                    let blip = child(f, "blip");
                    if let Some(rid) = blip.and_then(blip_embed)
                        && let Some(rel) = rels.get(rid).filter(|r| !r.external)
                    {
                        let media = match self.media.get(&rel.target) {
                            Some(id) => *id,
                            None => {
                                let data = self.package.read(&rel.target).ok()?;
                                let id = self.presentation.add_media(MediaFormat::from_path(&rel.target), data);
                                self.media.insert(rel.target.clone(), id);
                                id
                            }
                        };
                        return Some(Fill::Image(media));
                    }
                }
                "pattFill" => {
                    let fg = child(f, "fgClr").and_then(|c| colors.in_parent(c, None)).unwrap_or(ibis_doc_model::Color::BLACK);
                    let bg = child(f, "bgClr").and_then(|c| colors.in_parent(c, None)).unwrap_or(ibis_doc_model::Color::WHITE);
                    let preset = attr(f, "prst").unwrap_or("pct5");
                    return Some(match ibis_doc_model::pattern_tile(preset) {
                        Some(_) => Fill::Pattern { preset: preset.to_string(), foreground: fg, background: bg },
                        None => Fill::Solid(fg),
                    });
                }
                _ => {}
            }
        }
        if let Some(fill_ref) = style.and_then(|s| child(s, "fillRef")) {
            let idx = crate::xml::attr_i64(fill_ref, "idx").unwrap_or(0);
            if idx > 0 {
                if let Some(c) = colors.in_parent(fill_ref, None) {
                    return Some(Fill::Solid(c));
                }
            }
        }
        None
    }

    /// A shape's outline: its `a:ln` over the style's line (`wps:style/a:lnRef`),
    /// which alone draws a shape that has no `a:ln` (with the theme's line width).
    fn shape_stroke(&self, ln: Option<Node>, style: Option<Node>, colors: &crate::theme::Colors) -> Option<Stroke> {
        let reference = style.and_then(|s| child(s, "lnRef"));
        let ref_color = reference.and_then(|r| colors.in_parent(r, None));
        let ref_idx = reference.and_then(|r| crate::xml::attr_i64(r, "idx")).unwrap_or(0);

        let mut stroke_fill = None;
        let color = if let Some(fill) = ln.and_then(|ln| elements(ln).find(|n| matches!(n.tag_name().name(), "noFill" | "solidFill" | "gradFill"))) {
            match fill.tag_name().name() {
                "noFill" => return None,
                "solidFill" => colors.in_parent(fill, ref_color),
                "gradFill" => {
                    let mut stops: Vec<ibis_doc_model::GradientStop> = child(fill, "gsLst")
                        .into_iter()
                        .flat_map(|l| children(l, "gs"))
                        .filter_map(|gs| {
                            let color = colors.in_parent(gs, None)?;
                            Some(ibis_doc_model::GradientStop {
                                position: crate::xml::attr_i64(gs, "pos").unwrap_or(0).clamp(0, 100_000) as u32,
                                color,
                            })
                        })
                        .collect();
                    stops.sort_by_key(|s| s.position);
                    let first_color = stops.first().map(|s| s.color).or(ref_color);
                    if !stops.is_empty() {
                        let angle = child(fill, "lin").and_then(|l| crate::xml::attr_i64(l, "ang")).unwrap_or(0) as i32;
                        let scaled = child(fill, "lin").and_then(|l| l.attribute("scaled")).is_some_and(|v| v == "1" || v == "true");
                        stroke_fill = Some(Fill::Linear { angle, scaled, stops });
                    }
                    first_color
                }
                _ => ref_color,
            }
        } else if ref_idx > 0 {
            ref_color
        } else {
            None
        };
        let color = color?;
        let width = ln.and_then(|ln| crate::xml::attr_i64(ln, "w")).or_else(|| self.theme.line_width(ref_idx)).unwrap_or(Stroke::DEFAULT_WIDTH);
        let end = |name: &str| {
            let e = ln.and_then(|ln| child(ln, name));
            ibis_doc_model::LineEnd::parse(e.and_then(|e| attr(e, "type")), e.and_then(|e| attr(e, "w")), e.and_then(|e| attr(e, "len")))
        };
        let dash = ln
            .and_then(|ln| child(ln, "prstDash"))
            .and_then(|d| attr(d, "val"))
            .and_then(LineDash::parse)
            .unwrap_or(LineDash::Solid);
        let cap = match ln.and_then(|ln| attr(ln, "cap")) {
            Some("rnd") => LineCap::Round,
            Some("sq") => LineCap::Square,
            _ => LineCap::Flat,
        };
        let join = if ln.is_some_and(|ln| child(ln, "round").is_some()) {
            ibis_doc_model::LineJoin::Round
        } else if ln.is_some_and(|ln| child(ln, "bevel").is_some()) {
            ibis_doc_model::LineJoin::Bevel
        } else {
            ibis_doc_model::LineJoin::Miter
        };
        Some(Stroke {
            color,
            width,
            dashed: dash != LineDash::Solid,
            head: end("headEnd"),
            tail: end("tailEnd"),
            dash,
            cap,
            join,
            compound: ln.and_then(|ln| attr(ln, "cmpd")).and_then(ibis_doc_model::LineCompound::parse).unwrap_or_default(),
            alignment: ibis_doc_model::LineAlignment::Center,
            fill: stroke_fill,
        })
    }

    /// The paragraphs of a text box's content (`w:txbxContent`): each
    /// paragraph as Word resolves it, its runs (also inside hyperlinks,
    /// fields and content controls), a line break starting a paragraph that
    /// continues the line before, an empty paragraph kept at its mark's size,
    /// and a table as one line of text for each row.
    fn txbx_paragraphs(&self, container: Node, out: &mut Vec<Paragraph>) {
        for block in elements(container) {
            match block.tag_name().name() {
                "p" => self.txbx_paragraph(block, out),
                "tbl" => {
                    for row in elements(block).filter(|n| n.tag_name().name() == "tr") {
                        let mut cells: Vec<Vec<Paragraph>> = Vec::new();
                        for cell in elements(row).filter(|n| n.tag_name().name() == "tc") {
                            let mut inside = Vec::new();
                            self.txbx_paragraphs(cell, &mut inside);
                            cells.push(inside);
                        }
                        // The row on one line: its cells' text side by side.
                        let mut line = Paragraph::default();
                        for (i, cell) in cells.into_iter().enumerate() {
                            if i > 0 {
                                let mut gap = line.runs.last().map(|r| r.style.clone()).unwrap_or_default();
                                gap.underline = false;
                                line.runs.push(Run { text: "  ".to_string(), style: gap });
                            }
                            for (j, para) in cell.into_iter().enumerate() {
                                if i == 0 && j == 0 {
                                    line.align = para.align;
                                    line.end_style = para.end_style.clone();
                                    line.space_before = para.space_before;
                                    line.space_after = para.space_after;
                                    line.line_spacing = para.line_spacing;
                                    line.no_snap_to_grid = para.no_snap_to_grid;
                                }
                                line.runs.extend(para.runs);
                            }
                        }
                        out.push(line);
                    }
                }
                "sdt" => {
                    if let Some(content) = child(block, "sdtContent") {
                        self.txbx_paragraphs(content, out);
                    }
                }
                _ => {}
            }
        }
    }

    fn txbx_paragraph(&self, p: Node, out: &mut Vec<Paragraph>) {
        let ppr = child(p, "pPr");
        // The paragraph as Word resolves it: the document's defaults, its style, then its own
        // properties (the spacing between paragraphs, the line spacing, the typeface).
        let (mut para, base_run) = self.styles.paragraph_base(ppr.and_then(|pr| wval(pr, "pStyle")), None);
        if let Some(pr) = ppr {
            para.overlay(&para_format(pr, self.theme));
        }
        let align = Some(match para.align {
            Some(FlowAlign::Center) => TextAlign::Center,
            Some(FlowAlign::Right) => TextAlign::Right,
            Some(FlowAlign::Justify) => TextAlign::Justify,
            _ => TextAlign::Left,
        });
        let hundredths = |emu: Option<i64>| emu.map_or(0, |e| (e.max(0) / 127) as u32);
        let line_spacing = match para.line {
            Some(ibis_doc_model::LineHeight::Auto(n)) if n != 240 => Some(ibis_doc_model::LineSpacing::Percent(n * 100_000 / 240)),
            Some(ibis_doc_model::LineHeight::Exact(e)) => Some(ibis_doc_model::LineSpacing::Points(hundredths(Some(e)))),
            _ => None,
        };
        let style_of = |rpr: Option<Node>, cjk: bool| -> RunStyle {
            let mut f = base_run.clone();
            if let Some(rpr) = rpr {
                f.overlay(&run_format(rpr, self.theme));
            }
            // Match the body run font selection (ECMA-376 §17.3.2.26): East Asian text or w:hint="eastAsia"
            // uses eastAsia font (with no Latin fallback_font that would override East Asian font substitution).
            let (font, fallback_font) = if cjk || f.east_asia_hint == Some(true) {
                (f.east_asian_font.clone().or_else(|| f.font.clone()), None)
            } else {
                (f.font.clone().or_else(|| f.east_asian_font.clone()), None)
            };
            let underline = f.underline.unwrap_or(false);
            let underline_kind = f.underline_style.unwrap_or_default().to_drawing_underline();
            RunStyle {
                font_size: Some(f.size.unwrap_or(1050)),
                bold: Some(f.bold.unwrap_or(false)),
                italic: Some(f.italic.unwrap_or(false)),
                underline,
                underline_kind,
                color: f.color,
                font,
                fallback_font,
                border: f.border.flatten(),
                // `w:kern` is in half points and kerns text of that size and larger; the renderer's `kern` is in hundredths of a point and
                // kerns text above it (PowerPoint's rule), so one hundredth less makes a run of exactly that size kerned as Word does.
                kern: ibis_doc_model::KernMin(f.kern.filter(|k| *k > 0).map(|k| k * 50 - 1)),
                ..Default::default()
            }
        };
        // The paragraph mark's own size, for an empty paragraph's line.
        let end_style = style_of(ppr.and_then(|pr| child(pr, "rPr")), true);
        let new_paragraph = |line_break: bool| Paragraph {
            align,
            space_before: hundredths(para.before),
            space_before_exact: true,
            space_after: hundredths(para.after),
            line_spacing,
            end_style: end_style.clone(),
            line_break,
            no_snap_to_grid: para.snap == Some(false),
            ..Default::default()
        };
        let mut current = new_paragraph(false);
        let mut done: Vec<Paragraph> = Vec::new();
        // Runs in the order they come, inside hyperlinks, fields and content controls too.
        fn runs_of<'a>(node: Node<'a, 'a>, out: &mut Vec<Node<'a, 'a>>) {
            for c in elements(node) {
                match c.tag_name().name() {
                    "r" => out.push(c),
                    "hyperlink" | "fldSimple" | "sdt" | "sdtContent" | "ins" | "smartTag" | "customXml" | "moveTo" => runs_of(c, out),
                    _ => {}
                }
            }
        }
        let mut runs = Vec::new();
        runs_of(p, &mut runs);
        for r in runs {
            let rpr = child(r, "rPr");
            let mut text = String::new();
            let flush = |text: &mut String, current: &mut Paragraph| {
                if !text.is_empty() {
                    let cjk = text.chars().any(|c| c as u32 >= 0x2e80);
                    current.runs.push(Run { text: std::mem::take(text), style: style_of(rpr, cjk) });
                }
            };
            for c in elements(r) {
                match c.tag_name().name() {
                    "t" => text.push_str(c.text().unwrap_or("")),
                    "sym" => {
                        if let Some(ch) = wattr(c, "char").and_then(|v| u32::from_str_radix(v, 16).ok()).and_then(char::from_u32) {
                            let ch = symbol_char(ch, wattr(c, "font")).unwrap_or_else(|| {
                                if (0xf020..=0xf0ff).contains(&(ch as u32)) { char::from_u32(ch as u32 - 0xf000).unwrap_or(ch) } else { ch }
                            });
                            text.push(ch);
                        }
                    }
                    "noBreakHyphen" => text.push('-'),
                    "br" if attr(c, "type").is_none_or(|t| t == "textWrapping") => {
                        flush(&mut text, &mut current);
                        done.push(std::mem::replace(&mut current, new_paragraph(true)));
                    }
                    _ => {}
                }
            }
            flush(&mut text, &mut current);
        }
        done.push(current);
        out.extend(done);
    }

    fn shape_text(&self, wsp: Node, colors: &crate::theme::Colors) -> Option<TextBody> {
        let txbx = child(wsp, "txbx").or_else(|| child(wsp, "textbox"));
        let tx_body = child(wsp, "txBody");
        let body_pr = child(wsp, "bodyPr");

        let mut paragraphs = Vec::new();

        if let Some(tb) = txbx {
            let container = child(tb, "txbxContent").unwrap_or(tb);
            self.txbx_paragraphs(container, &mut paragraphs);
        } else if let Some(tb) = tx_body {
            for p in elements(tb).filter(|n| n.tag_name().name() == "p") {
                let ppr = child(p, "pPr");
                let align = ppr.and_then(|pr| attr(pr, "algn")).map(|v| match v {
                    "ctr" | "center" => TextAlign::Center,
                    "r" | "right" => TextAlign::Right,
                    _ => TextAlign::Left,
                });
                let mut runs = Vec::new();
                for r in elements(p).filter(|n| n.tag_name().name() == "r") {
                    let rpr = child(r, "rPr");
                    let bold = rpr.and_then(|p| attr_bool(p, "b")).unwrap_or(false);
                    let italic = rpr.and_then(|p| attr_bool(p, "i")).unwrap_or(false);
                    let font_size = rpr
                        .and_then(|p| attr_i64(p, "sz"))
                        .map(|sz| sz as u32)
                        .unwrap_or(2100);
                    let color = rpr.and_then(|p| colors.in_parent(p, None)).map(|c| c.rgb);
                    let font = rpr
                        .and_then(|p| child(p, "ea").or_else(|| child(p, "latin")))
                        .and_then(|f| attr(f, "typeface"))
                        .map(str::to_string);
                    let mut text = String::new();
                    for t in elements(r).filter(|n| n.tag_name().name() == "t") {
                        text.push_str(t.text().unwrap_or(""));
                    }
                    if !text.is_empty() {
                        runs.push(Run {
                            text,
                            style: RunStyle {
                                font_size: Some(font_size),
                                bold: Some(bold),
                                italic: Some(italic),
                                color,
                                font,
                                ..Default::default()
                            },
                        });
                    }
                }
                if !runs.is_empty() {
                    paragraphs.push(Paragraph {
                        runs,
                        align,
                        ..Default::default()
                    });
                }
            }
        }

        if paragraphs.is_empty() {
            if txbx.is_some() || tx_body.is_some() {
                paragraphs.push(Paragraph::default());
            } else {
                return None;
            }
        }

        let anchor = match body_pr.and_then(|b| attr(b, "anchor")) {
            Some("ctr") | Some("center") => VerticalAnchor::Middle,
            Some("b") | Some("bottom") => VerticalAnchor::Bottom,
            _ => VerticalAnchor::Top,
        };
        let insets = Insets {
            left: body_pr.and_then(|b| crate::xml::attr_i64(b, "lIns")).unwrap_or(91440),
            top: body_pr.and_then(|b| crate::xml::attr_i64(b, "tIns")).unwrap_or(45720),
            right: body_pr.and_then(|b| crate::xml::attr_i64(b, "rIns")).unwrap_or(91440),
            bottom: body_pr.and_then(|b| crate::xml::attr_i64(b, "bIns")).unwrap_or(45720),
        };
        let wrap = body_pr.and_then(|b| attr(b, "wrap")).is_none_or(|w| w != "none");
        let mut body = TextBody {
            paragraphs,
            style: TextStyle::default(),
            insets,
            anchor,
            wrap,
            clip: true,
            ..Default::default()
        };
        // The direction of the text (`vert`), its turn, upright and columns, as a presentation's body has them.
        if let Some(pr) = body_pr {
            if let Some(vert) = attr(pr, "vert") {
                let kind = ibis_doc_model::VerticalKind::parse(vert);
                body.vertical = kind.is_some();
                body.vertical_kind = kind.unwrap_or_default();
                body.sideways = match vert {
                    "vert" => 5_400_000,
                    "vert270" => 16_200_000,
                    _ => 0,
                };
            }
            if let Some(rot) = crate::xml::attr_i64(pr, "rot") {
                body.rotation = (rot as i32).rem_euclid(21_600_000);
            }
            if let Some(upright) = attr(pr, "upright") {
                body.upright = upright == "1" || upright == "true";
            }
            if child(pr, "spAutoFit").is_some() {
                body.autofit = ibis_doc_model::TextAutofit::ResizeShape;
            }
            if let Some(cols) = crate::xml::attr_i64(pr, "numCol") {
                body.columns = (cols as u32).clamp(1, 16);
            }
            if let Some(spc) = crate::xml::attr_i64(pr, "spcCol") {
                body.column_spacing = spc.max(0);
            }
        }
        Some(body)
    }

    fn table(&mut self, tbl: Node, path: &[u32], rels: &Rels) -> Option<FlowTable> {
        let id = self.new_id();
        self.sources.tables.insert(id, TableSource { part: self.part.clone(), path: path.to_vec() });
        let tbl_pr = child(tbl, "tblPr");
        // No style of its own: Word uses its built-in margins (108 twips), not
        // the default table style's (measured on the coverage tests, whose
        // default style sets them to 0).
        let style_id = tbl_pr.and_then(|p| wval(p, "tblStyle")).map(str::to_string);
        // Table properties: the style's, then the table's own.
        let mut chain: Vec<Node> = style_id.as_deref().map(|id| self.chain_nodes(id)).unwrap_or_default().into_iter().filter_map(|s| child(s, "tblPr")).collect();
        chain.extend(tbl_pr);
        // Word keeps its own cell margins (108 twips) for its 標準の表
        // (Normal Table) whatever the file's says (DTST: tables of styles
        // based on one with no margins have Word's).
        let normal_table: Vec<Node> = style_id.as_deref().map(|id| self.chain_nodes(id)).unwrap_or_default().into_iter().filter(|s| wval(*s, "name") == Some("Normal Table")).filter_map(|s| child(s, "tblPr")).collect();
        let mut borders: [Option<Option<FlowBorder>>; 6] = Default::default(); // top right bottom left insideH insideV
        let mut margins = [0i64, 108 * EMU_PER_TWIP, 0, 108 * EMU_PER_TWIP];
        let mut indent = 0;
        let mut align = FlowAlign::Left;
        let mut cell_spacing = None;
        for pr in &chain {
            if let Some(b) = child(*pr, "tblBorders") {
                for (i, name) in ["top", "right", "bottom", "left", "insideH", "insideV"].into_iter().enumerate() {
                    if let Some(e) = child(b, name).or_else(|| child(b, ["top", "end", "bottom", "start", "insideH", "insideV"][i])) {
                        borders[i] = Some(border(e, Some(self.theme)));
                    }
                }
            }
            if let Some(m) = child(*pr, "tblCellMar")
                && !normal_table.contains(pr)
            {
                cell_margins(m, &mut margins);
            }
            if let Some(sp) = child(*pr, "tblCellSpacing") {
                if wattr(sp, "type") == Some("nil") {
                    cell_spacing = None;
                } else if let Some(w) = twips(sp, "w") {
                    cell_spacing = Some(w);
                }
            }
            if let Some(ind) = child(*pr, "tblInd").and_then(|i| twips(i, "w")) {
                indent = ind;
            }
            if let Some(jc) = wval(*pr, "jc") {
                align = match jc {
                    "center" => FlowAlign::Center,
                    "right" | "end" => FlowAlign::Right,
                    _ => FlowAlign::Left,
                };
            }
        }
        let row_nodes: Vec<Node> = children(tbl, "tr").collect();
        let columns = grid_columns(tbl, tbl_pr, &row_nodes);
        // The style's conditional formatting (w:tblStylePr): which parts
        // of the table each cell is in, by the table's options (w:tblLook;
        // Word's new tables' 04A0 without it) and the bands' sizes.
        let style_nodes: Vec<Node> = style_id.as_deref().map(|id| self.chain_nodes(id)).unwrap_or_default();
        let look = table_look(tbl_pr.and_then(|p| child(p, "tblLook")));
        let band = |name: &str| chain.iter().rev().find_map(|pr| wval(*pr, name)).and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1);
        let (row_band, col_band) = (band("tblStyleRowBandSize"), band("tblStyleColBandSize"));
        let defines = |region: ibis_doc_model::TableRegion| style_nodes.iter().any(|st| children(*st, "tblStylePr").any(|c| wattr(c, "type").and_then(ibis_doc_model::TableRegion::from_name) == Some(region)));
        let bands = ibis_doc_model::TableLook {
            first_row: look.first_row && defines(ibis_doc_model::TableRegion::FirstRow),
            last_row: look.last_row && defines(ibis_doc_model::TableRegion::LastRow),
            first_column: look.first_column && defines(ibis_doc_model::TableRegion::FirstCol),
            last_column: look.last_column && defines(ibis_doc_model::TableRegion::LastCol),
            ..look
        };
        let row_count = row_nodes.len();
        let column_count = columns.len();
        let conditional_tc = |regions: &[ibis_doc_model::TableRegion]| -> Vec<Node> {
            let mut out = Vec::new();
            for region in regions {
                for style in &style_nodes {
                    for c in children(*style, "tblStylePr") {
                        if wattr(c, "type").and_then(ibis_doc_model::TableRegion::from_name) == Some(*region)
                            && let Some(tc) = child(c, "tcPr")
                        {
                            out.push(tc);
                        }
                    }
                }
            }
            out
        };
        let mut rows = Vec::new();
        let mut preferred: Vec<Vec<PreferredWidth>> = Vec::new();
        let mut shifted = false;
        for (row_index, tr) in row_nodes.into_iter().enumerate() {
            let row_id = self.new_id();
            let mut grid_at = child(tr, "trPr").and_then(|p| wval(p, "gridBefore")).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            preferred.push(Vec::new());
            shifted |= child(tr, "trPr").is_some_and(|p| child(p, "gridBefore").is_some() || child(p, "gridAfter").is_some());
            let tr_pr = child(tr, "trPr");
            let height = tr_pr.and_then(|p| child(p, "trHeight")).map_or(RowHeight::Auto, |h| {
                let v = twips(h, "val").unwrap_or(0);
                match wattr(h, "hRule") {
                    Some("exact") => RowHeight::Exact(v),
                    Some("auto") => RowHeight::Auto,
                    _ => RowHeight::AtLeast(v),
                }
            });
            let header = tr_pr.and_then(|p| toggle(p, "tblHeader")).unwrap_or(false);
            let cant_split = tr_pr.and_then(|p| toggle(p, "cantSplit")).unwrap_or(false);
            let cell_nodes: Vec<Node> = elements(tr).filter(|n| n.has_tag_name("tc") || n.has_tag_name("sdt")).flat_map(|n| {
                if n.has_tag_name("sdt") { child(n, "sdtContent").map(|c| children(c, "tc").collect::<Vec<_>>()).unwrap_or_default() } else { vec![n] }
            }).collect();
            let mut cells = Vec::new();
            for tc in cell_nodes {
                let tc_pr = child(tc, "tcPr");
                let span = tc_pr.and_then(|p| wval(p, "gridSpan")).and_then(|v| v.parse().ok()).unwrap_or(1u32).max(1);
                // Only a style with conditional formatting puts its cells
                // in parts.
                let regions = if !style_nodes.iter().any(|st| children(*st, "tblStylePr").next().is_some()) {
                    Vec::new()
                } else {
                    // A cell spanning to the last column is in it.
                    let last = (grid_at + span as usize).min(column_count).max(1) - 1;
                    let mut r = ibis_doc_model::TableRegion::of_cell(row_index, grid_at, row_count, column_count, look, bands, row_band, col_band);
                    if last != grid_at && look.last_column && last + 1 == column_count && !r.contains(&ibis_doc_model::TableRegion::LastCol) {
                        r = ibis_doc_model::TableRegion::of_cell(row_index, last, row_count, column_count, look, bands, row_band, col_band);
                    }
                    r
                };
                grid_at += span as usize;
                let cond = conditional_tc(&regions);
                let merge = match tc_pr.and_then(|p| child(p, "vMerge")) {
                    Some(m) if wattr(m, "val") == Some("restart") => VerticalMerge::Start,
                    Some(_) => VerticalMerge::Continue,
                    None => VerticalMerge::None,
                };
                // The cell's own borders (the table's at its edges are
                // resolved with them below).
                let mut own_borders: [Option<Option<FlowBorder>>; 4] = [None; 4];
                for b in cond.iter().filter_map(|tc| child(*tc, "tcBorders")).chain(tc_pr.and_then(|p| child(p, "tcBorders"))) {
                    for (i, (name, alt)) in [("top", "top"), ("right", "end"), ("bottom", "bottom"), ("left", "start")].into_iter().enumerate() {
                        if let Some(e) = child(b, name).or_else(|| child(b, alt)) {
                            own_borders[i] = Some(border(e, Some(self.theme)));
                        }
                    }
                }
                if let Some(row) = preferred.last_mut() {
                    row.push(tc_pr.and_then(|p| child(p, "tcW")).map_or(PreferredWidth::Auto, preferred_width));
                }
                let cell_id = self.new_id();
                let mut cell_margin = margins;
                if let Some(m) = tc_pr.and_then(|p| child(p, "tcMar")) {
                    cell_margins(m, &mut cell_margin);
                }
                let shd_node = tc_pr.and_then(|p| child(p, "shd"));
                let shading = shd_node.and_then(|s| shading(s, self.theme));
                let (shading_fill, shading_pattern, shading_color) = shd_node.and_then(|s| {
                    let (f, p, c) = shading_detail(s, self.theme);
                    p.map(|pat| (f, Some(pat), c))
                }).unwrap_or((None, None, None));
                let diagonal = |name: &str| tc_pr.and_then(|p| child(p, "tcBorders")).and_then(|b| child(b, name)).and_then(|e| border(e, Some(self.theme)));
                let diagonals = [diagonal("tl2br"), diagonal("tr2bl")];
                let anchor = match tc_pr.and_then(|p| wval(p, "vAlign")) {
                    Some("center") => CellAnchor::Center,
                    Some("bottom") => CellAnchor::Bottom,
                    _ => CellAnchor::Top,
                };
                let mut blocks = Vec::new();
                let tc_path = relative_path(tc, tbl).into_iter().fold(path.to_vec(), |mut p, i| {
                    p.push(i);
                    p
                });
                self.blocks(tc, &tc_path, rels, style_id.as_deref(), &mut blocks);
                cells.push(FlowCell { id: cell_id, own_borders, span, merge, blocks, borders: [None; 4], shading, shading_fill, shading_pattern, shading_color, anchor, margins: cell_margin, diagonals, style_fill: None, direction: tc_pr.and_then(|p| wval(p, "textDirection")).map_or_default(ibis_doc_model::CellDirection::from_name), no_wrap: tc_pr.and_then(|p| toggle(p, "noWrap")).unwrap_or(false) });
            }
            rows.push(FlowRow { id: row_id, height, header, cant_split, cells });
        }
        // The model's indent is to the table's left edge, as Word 2013 and
        // later read `w:tblInd`; before, it is to the first cell's text.
        let indent = if self.sources.table_indent_to_edge { indent } else { indent - rows.first().and_then(|r: &FlowRow| r.cells.first()).map_or(0, |c| c.margins[3]) };
        // 自動調整 (unless fixed): from the content and preferred widths
        // when the grid does not say the widths.
        let fixed = chain.iter().rev().find_map(|pr| child(*pr, "tblLayout")).is_some_and(|l| wattr(l, "type") == Some("fixed"));
        let width = chain.iter().rev().find_map(|pr| child(*pr, "tblW")).map_or(PreferredWidth::Auto, preferred_width);
        let autofit = (!fixed && !shifted && fits_content(&columns, width, &rows, &preferred)).then(|| AutoFit { width, cells: preferred });
        // The style's parts (its cells' fills, their paragraphs'
        // formatting) are found from the options (FlowTable::apply_style).
        let style = style_id.map(|id| ibis_doc_model::TableStyleRef { id, look, row_band: row_band as u32, col_band: col_band as u32 });
        // 浮動の表 (w:tblpPr).
        let floating = tbl_pr.and_then(|p| child(p, "tblpPr")).map(|f| {
            let anchor = |name: &str| match wattr(f, name) {
                Some("margin") => ibis_doc_model::TableAnchor::Margin,
                Some("page") => ibis_doc_model::TableAnchor::Page,
                _ => ibis_doc_model::TableAnchor::Text,
            };
            let spec = |name: &str| match wattr(f, name) {
                Some("left") | Some("top") => Some(ibis_doc_model::TableAlign::Start),
                Some("center") => Some(ibis_doc_model::TableAlign::Center),
                Some("right") | Some("bottom") => Some(ibis_doc_model::TableAlign::End),
                Some("inside") => Some(ibis_doc_model::TableAlign::Inside),
                Some("outside") => Some(ibis_doc_model::TableAlign::Outside),
                _ => None,
            };
            let tw = |name: &str| twips(f, name).unwrap_or(0);
            ibis_doc_model::TableFloat {
                horz_anchor: anchor("horzAnchor"),
                vert_anchor: anchor("vertAnchor"),
                x: tw("tblpX"),
                x_align: spec("tblpXSpec"),
                y: tw("tblpY"),
                y_align: spec("tblpYSpec"),
                distance: [tw("leftFromText"), tw("rightFromText"), tw("topFromText"), tw("bottomFromText")],
            }
        });
        let fit_to_page = !fixed && matches!(width, PreferredWidth::Auto);
        let mut table = FlowTable { id, borders, columns, indent, align, rows, autofit, style, floating, cell_spacing, fit_to_page };
        table.apply_style(self.styles);
        table.resolve_borders();
        Some(table)
    }
}

/// A table's style options (`w:tblLook`): its attributes, or the bits of
/// its `w:val` (Word 2007's); Word's new tables' (04A0: the heading row,
/// the first column and the rows' bands) without it.
fn table_look(look: Option<Node>) -> ibis_doc_model::TableLook {
    let bits = look.and_then(|l| wattr(l, "val")).and_then(|v| u32::from_str_radix(v, 16).ok()).unwrap_or(if look.is_some() { 0 } else { 0x04A0 });
    let flag = |name: &str, bit: u32| match look.and_then(|l| wattr(l, name)) {
        Some(v) => matches!(v, "1" | "true" | "on"),
        None => bits & bit != 0,
    };
    ibis_doc_model::TableLook {
        first_row: flag("firstRow", 0x20),
        last_row: flag("lastRow", 0x40),
        first_column: flag("firstColumn", 0x80),
        last_column: flag("lastColumn", 0x100),
        no_hband: flag("noHBand", 0x200),
        no_vband: flag("noVBand", 0x400),
    }
}

/// A preferred width: `w:tblW` or `w:tcW` (`w` in twips, or in fiftieths
/// of a percent, or as `50%`).
fn preferred_width(w: Node) -> PreferredWidth {
    match wattr(w, "type") {
        Some("pct") => {
            let v = wattr(w, "w").unwrap_or("0");
            let fiftieths = match v.strip_suffix('%') {
                Some(p) => p.parse::<f64>().map_or(0, |p| (p * 50.0) as u32),
                None => v.parse::<u32>().unwrap_or(0),
            };
            if fiftieths > 0 { PreferredWidth::Percent(fiftieths) } else { PreferredWidth::Auto }
        }
        Some("nil") | Some("auto") => PreferredWidth::Auto,
        _ => twips(w, "w").filter(|&v| v > 0).map_or(PreferredWidth::Auto, PreferredWidth::Fixed),
    }
}

/// Whether an automatically fitted table is laid out from its content
/// rather than its grid: a cell without a width (or in percent), a table
/// width in percent, or widths that disagree with the grid (by more than
/// a point). Word writes the widths it laid out into the grid and the
/// cells alike, so its own files keep their grid.
fn fits_content(columns: &[i64], width: PreferredWidth, rows: &[FlowRow], preferred: &[Vec<PreferredWidth>]) -> bool {
    const POINT: i64 = 20 * EMU_PER_TWIP;
    let grid: i64 = columns.iter().sum();
    match width {
        PreferredWidth::Percent(_) => return true,
        PreferredWidth::Fixed(w) if (w - grid).abs() > POINT => return true,
        _ => {}
    }
    for (row, widths) in rows.iter().zip(preferred) {
        let mut at = 0usize;
        for (cell, w) in row.cells.iter().zip(widths) {
            let span = cell.span as usize;
            let spanned: i64 = columns.iter().skip(at).take(span).sum();
            match *w {
                PreferredWidth::Fixed(w) if w >= spanned - POINT => {}
                // A cell merged down keeps the width of its start.
                _ if cell.merge == VerticalMerge::Continue => {}
                _ => return true,
            }
            at += span;
        }
    }
    false
}

/// A table's column widths (EMU): its `w:tblGrid`, with the columns it
/// lacks or leaves without a width taken from the cells' widths
/// (`w:tcW`, in twentieths of a point or fiftieths of a percent of the
/// table's `w:tblW`), as Word lays out a table written without a grid (or
/// with a short one) by other programs. A column still without a width
/// gets an equal share of what `w:tblW` leaves, or an inch.
fn grid_columns(tbl: Node, tbl_pr: Option<Node>, rows: &[Node]) -> Vec<i64> {
    let mut columns: Vec<i64> = child(tbl, "tblGrid").into_iter().flat_map(|g| children(g, "gridCol")).map(|c| twips(c, "w").unwrap_or(0)).collect();
    let table_width = tbl_pr.and_then(|p| child(p, "tblW")).filter(|w| matches!(wattr(*w, "type"), None | Some("dxa"))).and_then(|w| twips(w, "w")).filter(|&w| w > 0);
    let cells = |tr: Node| -> Vec<(u32, Option<i64>)> {
        elements(tr)
            .filter(|n| n.has_tag_name("tc") || n.has_tag_name("sdt"))
            .flat_map(|n| if n.has_tag_name("sdt") { child(n, "sdtContent").map(|c| children(c, "tc").collect::<Vec<_>>()).unwrap_or_default() } else { vec![n] })
            .map(|tc| {
                let pr = child(tc, "tcPr");
                let span = pr.and_then(|p| wval(p, "gridSpan")).and_then(|v| v.parse().ok()).unwrap_or(1u32).max(1);
                let width = pr.and_then(|p| child(p, "tcW")).and_then(|w| match wattr(w, "type") {
                    None | Some("dxa") => twips(w, "w"),
                    Some("pct") => {
                        let v = wattr(w, "w")?;
                        let fraction = match v.strip_suffix('%') {
                            Some(p) => p.parse::<f64>().ok()? / 100.0,
                            None => v.parse::<f64>().ok()? / 5000.0,
                        };
                        table_width.map(|t| (t as f64 * fraction) as i64)
                    }
                    _ => None,
                });
                (span, width.filter(|&w| w > 0))
            })
            .collect()
    };
    let before = |tr: Node| child(tr, "trPr").and_then(|p| wval(p, "gridBefore")).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
    let count = rows.iter().map(|&tr| before(tr) + cells(tr).iter().map(|c| c.0 as usize).sum::<usize>()).max().unwrap_or(0);
    if count <= columns.len() && columns.iter().all(|&w| w > 0) {
        return columns;
    }
    columns.resize(columns.len().max(count), 0);
    for &tr in rows {
        let mut at = before(tr);
        for (span, width) in cells(tr) {
            let range = at..(at + span as usize).min(columns.len());
            if let Some(w) = width
                && columns[range.clone()].iter().all(|&c| c == 0)
            {
                let n = range.len().max(1) as i64;
                for c in &mut columns[range.clone()] {
                    *c = w / n;
                }
            }
            at += span as usize;
        }
    }
    let unknown = columns.iter().filter(|&&w| w == 0).count() as i64;
    if unknown > 0 {
        let known: i64 = columns.iter().sum();
        let share = table_width.map(|t| (t - known) / unknown).filter(|&w| w > 0).unwrap_or(ibis_doc_model::EMU_PER_INCH);
        for c in columns.iter_mut().filter(|w| **w == 0) {
            *c = share;
        }
    }
    columns
}

/// The `w:compatibilityMode` of a settings part (Word 2007's 12 when unset;
/// 15 is Word 2013 and later).
fn compatibility_mode(settings: &str) -> u32 {
    let Some(at) = settings.find("compatibilityMode") else { return 12 };
    let tag_end = settings[at..].find('>').map_or(settings.len(), |e| at + e);
    let tag = &settings[at..tag_end];
    tag.find("val=\"").and_then(|v| tag[v + 5..].split('"').next()).and_then(|v| v.parse().ok()).unwrap_or(12)
}

fn cell_margins(m: Node, out: &mut [i64; 4]) {
    for (i, (name, alt)) in [("top", "top"), ("right", "end"), ("bottom", "bottom"), ("left", "start")].into_iter().enumerate() {
        if let Some(v) = child(m, name).or_else(|| child(m, alt)).and_then(|e| twips(e, "w")) {
            out[i] = v;
        }
    }
}

/// A complex field (`w:fldChar`) being read.
/// 囲い文字's parts in an `EQ \o` field's code (`eq \o\ac(○,字)`): the
/// enclosure and the character (any `\s\up n( )` around them taken off).
pub fn enclosed_field(instruction: &str) -> Option<(String, String)> {
    ibis_doc_model::field_format::enclosed(instruction)
}

/// 囲い文字 (`EQ \o\ac`): the sizes (hundredths of a point) the runs of the field's code give
/// the enclosure and the character in it, from the code's runs' text and sizes.
fn enclosure_sizes(instruction: &str, runs: &[(String, u32)]) -> Option<(u32, u32)> {
    let (enclosure, inner) = enclosed_field(instruction)?;
    let size_of = |text: &str| text.chars().find(|c| !c.is_whitespace()).and_then(|c| runs.iter().find(|(t, _)| t.contains(c)).map(|(_, s)| *s));
    Some((size_of(&enclosure)?, size_of(&inner)?))
}

/// Whether a field is one the layout computes (keeping the result saved
/// as what it shows until then): the page counts, the section, captions'
/// numbers and references, dates, formulas, and `PAGE` with a format
/// switch (plain `PAGE` is a page number of its own).
/// A field's code with its name (the first word) in capitals.
fn upcase_field_name(instruction: &str) -> String {
    let t = instruction.trim_start();
    let end = t.find(char::is_whitespace).unwrap_or(t.len());
    if !t[..end].chars().all(|c| c.is_ascii_alphabetic()) {
        return instruction.to_string();
    }
    format!("{}{}", t[..end].to_ascii_uppercase(), &t[end..])
}

fn computed_field(instruction: &str) -> bool {
    let trimmed = instruction.trim_start();
    if trimmed.starts_with('=') || enclosed_field(trimmed).is_some() {
        return true;
    }
    match trimmed.split_whitespace().next() {
        Some("PAGE") => ibis_doc_model::field_format::format_field_number(1, trimmed).is_some(),
        Some(
            "SEQ" | "LISTNUM" | "AUTONUM" | "AUTONUMLGL" | "AUTONUMOUT" | "FILESIZE" | "DOCVARIABLE" | "USERNAME" | "USERINITIALS" | "USERADDRESS" | "REF" | "NOTEREF" | "STYLEREF" | "NUMPAGES" | "SECTIONPAGES" | "SECTION" | "DATE" | "TIME" | "CREATEDATE" | "SAVEDATE"
            | "PRINTDATE" | "AUTHOR" | "TITLE" | "SUBJECT" | "COMMENTS" | "DOCPROPERTY" | "QUOTE" | "SYMBOL" | "SET" | "IF" | "COMPARE"
            | "HYPERLINK" | "TEMPLATE" | "ADVANCE" | "INFO" | "ASK" | "FILLIN" | "FILL-IN" | "MERGEFIELD" | "MERGEREC" | "MERGESEQ" | "NEXT" | "NEXTIF" | "SKIPIF" | "ADDRESSBLOCK" | "GREETINGLINE" | "XE" | "TC" | "TA" | "EQ" | "INCLUDETEXT"
            | "AUTOTEXT" | "AUTOTEXTLIST" | "CITATION",
        ) => true,
        _ => false,
    }
}

#[derive(Default)]
struct Frame {
    instruction: String,
    /// The text of each run of the field's code and the size of the run (hundredths of a point).
    instr_sizes: Vec<(String, u32)>,
    /// Between `separate` and `end`: the field's result.
    showing: bool,
    /// The result is replaced by what the field computes (a page number).
    replaced: bool,
    /// In a `FILENAME` field's result, or a `PAGEREF` field's.
    file_name: bool,
    page_ref: bool,
    /// A table of contents (spanning paragraphs).
    toc: bool,
    /// Whether the field is locked (w:fldLock="1" / "true").
    locked: bool,
    /// The runs it takes in the paragraph being read, and its position.
    runs: Vec<Vec<u32>>,
    atom: Option<usize>,
    form_field: Option<ibis_doc_model::FormFieldData>,
    form_field_text: bool,
    form_field_emitted: bool,
    result_text: String,
}

fn parse_ff_data(ff_data: Node) -> Option<ibis_doc_model::FormFieldData> {
    let name = child(ff_data, "name").and_then(|n| wattr(n, "val")).unwrap_or("").to_string();
    let enabled = child(ff_data, "enabled").map_or(true, |n| wattr(n, "val").map_or(true, |v| v != "0" && v != "false"));
    let calc_on_exit = child(ff_data, "calcOnExit").map_or(false, |n| wattr(n, "val").map_or(true, |v| v == "1" || v == "true"));
    if let Some(text_input) = child(ff_data, "textInput") {
        let default = child(text_input, "default").and_then(|n| wattr(n, "val")).unwrap_or("").to_string();
        let max_length = child(text_input, "maxLength").and_then(|n| wattr(n, "val")).and_then(|v| v.parse().ok()).unwrap_or(0);
        return Some(ibis_doc_model::FormFieldData {
            name,
            enabled,
            calc_on_exit,
            kind: ibis_doc_model::FormFieldKind::TextInput {
                value: default.clone(),
                default,
                max_length,
            },
        });
    }
    if let Some(check_box) = child(ff_data, "checkBox") {
        let checked = child(check_box, "checked").map_or(false, |n| wattr(n, "val").map_or(true, |v| v == "1" || v == "true"));
        let default = child(check_box, "default").map_or(false, |n| wattr(n, "val").map_or(true, |v| v == "1" || v == "true"));
        let size_auto = child(check_box, "sizeAuto").is_some();
        return Some(ibis_doc_model::FormFieldData {
            name,
            enabled,
            calc_on_exit,
            kind: ibis_doc_model::FormFieldKind::CheckBox {
                checked,
                default,
                size_auto,
            },
        });
    }
    if let Some(dd_list) = child(ff_data, "ddList") {
        let result = child(dd_list, "result").and_then(|n| wattr(n, "val")).and_then(|v| v.parse().ok()).unwrap_or(0);
        let default = child(dd_list, "default").and_then(|n| wattr(n, "val")).and_then(|v| v.parse().ok()).unwrap_or(0);
        let items: Vec<String> = children(dd_list, "listEntry").filter_map(|n| wattr(n, "val").map(|s| s.to_string())).collect();
        return Some(ibis_doc_model::FormFieldData {
            name,
            enabled,
            calc_on_exit,
            kind: ibis_doc_model::FormFieldKind::DropDown {
                items,
                selected: result,
                default,
            },
        });
    }
    None
}

/// The complex fields open (nested), across paragraphs.
#[derive(Default)]
struct FieldState {
    frames: Vec<Frame>,
}

/// Whether the package's main part is a Word document.
pub(crate) fn is_document(package: &Package) -> Result<bool, Error> {
    Ok(package.related("", REL_OFFICE_DOCUMENT)?.is_some_and(|main| main.ends_with("document.xml")))
}

fn parse<'i>(name: &str, text: &'i str) -> Result<Document<'i>, Error> {
    let options = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    Document::parse_with_options(text, options).map_err(|e| Error::xml(name, e))
}

/// Read a `.docx` document: one slide holding its first page; the viewer
/// lays out the rest (`Presentation::flow`).
pub(crate) fn read(data: &[u8]) -> Result<Presentation, Error> {
    read_with_sources(data).map(|(p, _)| p)
}

/// Read a `.docx` document, and where each paragraph and table (and each
/// position of a paragraph's runs) came from, for saving.
pub(crate) fn read_with_sources(data: &[u8]) -> Result<(Presentation, SourceMap), Error> {
    let package = Package::new(data)?;
    let main = package.related("", REL_OFFICE_DOCUMENT)?.ok_or_else(|| Error::new("文書の本体が見つかりません"))?;
    let rels = package.relationships(&main)?;
    let xml = package.read_xml(&main)?;
    let doc = parse(&main, &xml)?;
    let root = doc.root_element();
    let body = child(root, "body").ok_or_else(|| Error::new("文書に本文がありません"))?;
    let body_path = crate::xml::element_path(body);

    let part_of = |suffix: &str| rels.values().find(|r| !r.external && r.kind.ends_with(suffix)).map(|r| r.target.clone());
    let theme_xml = part_of("/theme").and_then(|p| package.read_xml(&p).ok().map(|t| (p, t)));
    let theme_doc = theme_xml.as_ref().map(|(p, t)| parse(p, t)).transpose()?;
    let theme = theme_doc.as_ref().map(|d| Theme::read(d.root_element())).unwrap_or_default();
    let styles_part = part_of("/styles");
    let styles_xml = styles_part.as_ref().and_then(|p| package.read_xml(p).ok().map(|t| (p.clone(), t)));
    let styles_doc = styles_xml.as_ref().map(|(p, t)| parse(p, t)).transpose()?;

    let mut styles = FlowStyles::default();
    let mut pic_bullets = Vec::new();
    let mut pic_bullet_rels = HashMap::new();
    if let Some(part) = part_of("/numbering")
        && let Ok(xml) = package.read_xml(&part)
    {
        let doc = parse(&part, &xml)?;
        styles.numbering = numbering(doc.root_element(), &theme);
        pic_bullets = pic_bullet_sources(doc.root_element());
        pic_bullet_rels = package.relationships(&part).unwrap_or_default();
    }
    let mut style_nodes = HashMap::new();
    if let Some(s) = &styles_doc {
        let root = s.root_element();
        if let Some(defaults) = child(root, "docDefaults") {
            if let Some(rpr) = child(defaults, "rPrDefault").and_then(|d| child(d, "rPr")) {
                styles.defaults_run = run_format(rpr, &theme);
            }
            if let Some(ppr) = child(defaults, "pPrDefault").and_then(|d| child(d, "pPr")) {
                styles.defaults_para = para_format(ppr, &theme);
            }
        }
        for style in children(root, "style") {
            let Some(id) = wattr(style, "styleId") else { continue };
            let default = wattr(style, "default").is_some_and(|v| matches!(v, "1" | "true"));
            let kind = match wattr(style, "type") {
                Some("character") => StyleKind::Character,
                Some("table") => StyleKind::Table,
                Some("numbering") => StyleKind::Numbering,
                _ => StyleKind::Paragraph,
            };
            match kind {
                StyleKind::Paragraph if default => styles.default_paragraph = Some(id.to_string()),
                StyleKind::Table if default => styles.default_table = Some(id.to_string()),
                _ => {}
            }
            styles.styles.insert(
                id.to_string(),
                FlowStyle {
                    kind,
                    name: wval(style, "name").unwrap_or(id).to_string(),
                    based_on: wval(style, "basedOn").map(str::to_string),
                    para: child(style, "pPr").map(|p| para_format(p, &theme)).unwrap_or_default(),
                    run: child(style, "rPr").map(|r| run_format(r, &theme)).unwrap_or_default(),
                    next: wval(style, "next").map(str::to_string),
                    auto_update: child(style, "autoRedefine").is_some_and(|e| !matches!(wattr(e, "val"), Some("0" | "false" | "off"))),
                    ui_priority: wval(style, "uiPriority").and_then(|v| v.parse().ok()),
                    quick: child(style, "qFormat").is_some_and(|e| !matches!(wattr(e, "val"), Some("0" | "false" | "off"))),
                    conditional: children(style, "tblStylePr")
                        .filter_map(|c| {
                            Some(ibis_doc_model::TableCondition {
                                region: ibis_doc_model::TableRegion::from_name(wattr(c, "type")?)?,
                                para: child(c, "pPr").map(|p| para_format(p, &theme)).unwrap_or_default(),
                                run: child(c, "rPr").map(|r| run_format(r, &theme)).unwrap_or_default(),
                                fill: child(c, "tcPr").and_then(|t| child(t, "shd")).and_then(|s| shading(s, &theme)),
                            })
                        })
                        .collect(),
                    num_id: child(style, "pPr").and_then(|p| child(p, "numPr")).and_then(|n| wval(n, "numId")).and_then(|v| v.parse().ok()),
                },
            );
            style_nodes.insert(id.to_string(), style);
        }
    }

    // The page setup of the body's section (the last one).
    let sect = child(body, "sectPr");
    let page = sect.map(page_setup).unwrap_or_default();

    let mut presentation = Presentation::empty(page.size);
    let table_indent_to_edge = part_of("/settings").and_then(|p| package.read_xml(&p).ok()).is_some_and(|xml| compatibility_mode(&xml) >= 15);
    let sources = SourceMap { body_part: main.clone(), body_path: body_path.clone(), table_indent_to_edge, ..SourceMap::default() };
    // The picture bullets' images, into the media.
    for (id, rid, size) in pic_bullets {
        let Some(rel) = pic_bullet_rels.get(&rid).filter(|r| !r.external) else { continue };
        let Ok(data) = package.read(&rel.target) else { continue };
        let media = presentation.add_media(MediaFormat::from_path(&rel.target), data);
        styles.numbering.pic_bullets.insert(id, (media, size));
    }
    let mut reader = Reader {
        package: &package,
        theme: &theme,
        styles: &styles,
        style_nodes,
        presentation: &mut presentation,
        media: HashMap::new(),
        last_id: 0,
        part: main.clone(),
        sources,
        fields: FieldState::default(),
        sections: Vec::new(),
        revision: None,
        shape_groups: Vec::new(),
    };
    let mut blocks = Vec::new();
    reader.blocks(body, &body_path, &rels, None, &mut blocks);
    // The break after each section is how the next one starts (its
    // w:type; the last one's in the body's w:sectPr).
    {
        let kind = |sect: Option<Node>| match sect.and_then(|s| wval(s, "type")) {
            Some("continuous") => SectionBreak::Continuous,
            Some("oddPage") => SectionBreak::OddPage,
            Some("evenPage") => SectionBreak::EvenPage,
            Some("nextColumn") => SectionBreak::NextColumn,
            _ => SectionBreak::NextPage,
        };
        let starts: Vec<SectionBreak> = body.descendants().filter(|n| n.is_element() && n.tag_name().name() == "sectPr" && n.parent().is_some_and(|p| p.tag_name().name() == "pPr")).map(|s| kind(Some(s))).skip(1).chain([kind(child(body, "sectPr"))]).collect();
        let mut at = 0;
        for b in &mut blocks {
            if let Block::Paragraph(p) = b
                && p.section.is_some()
            {
                if let Some(k) = starts.get(at) {
                    p.section_break = *k;
                }
                at += 1;
            }
        }
    }
    // The sections' headers and footers, in order: a section has its own
    // when it refers to a part the section before does not (else it is
    // the previous one's: 前と同じ).
    // The paragraphs' `w:sectPr`, in the order they were read.
    let section_nodes: Vec<Node> = body.descendants().filter(|n| n.is_element() && n.tag_name().name() == "sectPr" && n.parent().is_some_and(|p| p.tag_name().name() == "pPr")).collect();
    let mut chain: Vec<(u32, Option<Node>)> = std::mem::take(&mut reader.sections).into_iter().zip(section_nodes).map(|(id, n)| (id, Some(n))).collect();
    chain.push((0, sect));
    let mut previous: [Option<String>; 6] = Default::default();
    let mut last_parts: [Option<Vec<Block>>; 6] = Default::default();
    for (id, node) in chain {
        for kind in PartKind::ALL {
            let Some(part) = node.and_then(|n| part_reference(n, kind, &rels)) else { continue };
            if previous[kind.index()].as_ref() == Some(&part) {
                continue;
            }
            let Ok(xml) = package.read_xml(&part) else { continue };
            let doc = parse(&part, &xml)?;
            let part_rels = package.relationships(&part)?;
            reader.part = part.clone();
            let mut content = Vec::new();
            reader.blocks(doc.root_element(), &[], &part_rels, None, &mut content);
            reader.sources.section_parts.insert((id, kind.index()), part.clone());
            previous[kind.index()] = Some(part);
            if id == 0 {
                last_parts[kind.index()] = Some(content);
            } else {
                set_section_part(&mut blocks, id, kind, content);
            }
        }
    }
    // The footnotes and endnotes (not their separators).
    let mut notes: [Vec<Note>; 2] = Default::default();
    let mut note_separators = [ibis_doc_model::NoteSeparator::Standard; 2];
    for (i, (suffix, element)) in [("/footnotes", "footnote"), ("/endnotes", "endnote")].into_iter().enumerate() {
        let Some(part) = rels.values().find(|r| !r.external && r.kind.ends_with(suffix)).map(|r| r.target.clone()) else { continue };
        let Ok(xml) = package.read_xml(&part) else { continue };
        let doc = parse(&part, &xml)?;
        let part_rels = package.relationships(&part)?;
        reader.part = part.clone();
        for (index, node) in elements(doc.root_element()).enumerate() {
            if node.tag_name().name() == element && wattr(node, "type") == Some("separator") {
                // The line above the notes: the short one, one across the text, or none (no mark in it).
                let has = |name: &str| node.descendants().any(|d| d.is_element() && d.tag_name().name() == name);
                let text = node.descendants().any(|d| d.is_element() && d.tag_name().name() == "t" && d.text().is_some_and(|t| !t.is_empty()));
                note_separators[i] = if has("continuationSeparator") {
                    ibis_doc_model::NoteSeparator::Full
                } else if has("separator") || text {
                    ibis_doc_model::NoteSeparator::Standard
                } else {
                    ibis_doc_model::NoteSeparator::Hidden
                };
            }
            if node.tag_name().name() != element || wattr(node, "type").is_some_and(|t| t != "normal") {
                continue;
            }
            let Some(id) = wattr(node, "id").and_then(|v| v.parse::<u32>().ok()) else { continue };
            let mut content = Vec::new();
            reader.blocks(node, &[index as u32], &part_rels, None, &mut content);
            notes[i].push(Note { id, blocks: content });
        }
        reader.sources.note_parts[i] = Some(part);
    }
    let [footnotes, endnotes] = notes;
    // The comments, and from commentsExtended.xml which answers which and
    // which are resolved (by their last paragraph's w14:paraId).
    let mut comments: Vec<Comment> = Vec::new();
    if let Some(part) = rels.values().find(|r| !r.external && r.kind.ends_with("/comments")).map(|r| r.target.clone())
        && let Ok(xml) = package.read_xml(&part)
    {
        let doc = parse(&part, &xml)?;
        let part_rels = package.relationships(&part)?;
        reader.part = part.clone();
        for (index, node) in elements(doc.root_element()).enumerate() {
            if node.tag_name().name() != "comment" {
                continue;
            }
            let Some(id) = wattr(node, "id").and_then(|v| v.parse::<u32>().ok()) else { continue };
            let mut content = Vec::new();
            reader.blocks(node, &[index as u32], &part_rels, None, &mut content);
            let para_id = elements(node).filter(|n| n.tag_name().name() == "p").last().and_then(|p| p.attributes().find(|a| a.name() == "paraId").map(|a| a.value().to_string()));
            comments.push(Comment {
                id,
                author: wattr(node, "author").unwrap_or("").to_string(),
                initials: wattr(node, "initials").unwrap_or("").to_string(),
                date: wattr(node, "date").unwrap_or("").to_string(),
                blocks: content,
                parent: None,
                done: false,
                para_id,
            });
        }
        reader.sources.comments_part = Some(part);
    }
    if let Some(part) = rels.values().find(|r| !r.external && r.kind.ends_with("/commentsExtended")).map(|r| r.target.clone())
        && let Ok(xml) = package.read_xml(&part)
    {
        let doc = parse(&part, &xml)?;
        let by_para: HashMap<String, u32> = comments.iter().filter_map(|c| c.para_id.clone().map(|p| (p, c.id))).collect();
        for e in doc.root_element().descendants().filter(|n| n.is_element() && n.tag_name().name() == "commentEx") {
            let attr = |name: &str| e.attributes().find(|a| a.name() == name).map(|a| a.value().to_string());
            let Some(id) = attr("paraId").and_then(|p| by_para.get(&p).copied()) else { continue };
            let parent = attr("paraIdParent").and_then(|p| by_para.get(&p).copied());
            let done = attr("done").is_some_and(|d| d == "1" || d == "true");
            if let Some(c) = comments.iter_mut().find(|c| c.id == id) {
                c.parent = parent;
                c.done = done;
            }
        }
        reader.sources.comments_extended_part = Some(part);
    }
    // The watermarks: each section's default header's (else the previous
    // section's); the document's the first one found.
    let mut section_watermarks: Vec<(u32, Option<ibis_doc_model::Watermark>)> = Vec::new();
    {
        let mut ids: Vec<u32> = blocks.iter().filter_map(|b| if let Block::Paragraph(p) = b && p.section.is_some() { Some(p.id) } else { None }).collect();
        ids.push(0);
        let mut current: Option<ibis_doc_model::Watermark> = None;
        let mut read: HashMap<String, Option<ibis_doc_model::Watermark>> = HashMap::new();
        for id in ids {
            if let Some(part) = reader.sources.section_parts.get(&(id, PartKind::Header.index())) {
                current = read
                    .entry(part.clone())
                    .or_insert_with(|| package.read_xml(part).ok().and_then(|xml| parse(part, &xml).ok().and_then(|d| watermark_in(d.root_element()))))
                    .clone();
            }
            section_watermarks.push((id, current.clone()));
        }
    }
    let mut watermark = section_watermarks.iter().find_map(|(_, w)| w.clone());
    if watermark.is_none() {
        // Only in a first page's or even pages' header.
        let mut parts: Vec<&String> = reader.sources.section_parts.iter().filter(|((_, k), _)| !PartKind::ALL[*k].is_footer()).map(|(_, p)| p).collect();
        parts.sort();
        watermark = parts.into_iter().find_map(|part| package.read_xml(part).ok().and_then(|xml| parse(part, &xml).ok().and_then(|d| watermark_in(d.root_element()))));
    }
    if section_watermarks.iter().all(|(_, w)| *w == watermark) {
        section_watermarks.clear();
    }
    let linked: [bool; 6] = std::array::from_fn(|k| last_parts[k].is_none());
    reader.sources.header_part = reader.sources.section_parts.get(&(0, PartKind::Header.index())).cloned();
    reader.sources.footer_part = reader.sources.section_parts.get(&(0, PartKind::Footer.index())).cloned();
    let [header, footer, header_first, footer_first, header_even, footer_even] = last_parts.map(Option::unwrap_or_default);
    let id_floor = reader.last_id;
    let sources = std::mem::take(&mut reader.sources);
    let shape_groups = std::mem::take(&mut reader.shape_groups);
    let title_page = sect.and_then(|s| toggle(s, "titlePg")).unwrap_or(false);
    let even_and_odd = part_of("/settings").and_then(|p| package.read_xml(&p).ok()).is_some_and(|xml| xml.contains("evenAndOddHeaders") && !xml.contains("evenAndOddHeaders w:val=\"0\"") && !xml.contains("evenAndOddHeaders w:val=\"false\""));
    let default_tab_stop = part_of("/settings").and_then(|p| package.read_xml(&p).ok()).and_then(|xml| {
        let at = xml.find("defaultTabStop")?;
        let rest = &xml[at..xml[at..].find('>').map_or(xml.len(), |e| at + e)];
        let value = rest.split("w:val=\"").nth(1)?.split('"').next()?;
        value.parse::<i64>().ok().filter(|v| *v > 0).map(|twips| twips * EMU_PER_TWIP)
    });
    let page_number_format = sect.and_then(|s| child(s, "pgNumType")).and_then(|n| wattr(n, "fmt")).map(str::to_string);
    let page_chapter = page_chapter(sect.and_then(|s| child(s, "pgNumType")));
    let lock_tracking = part_of("/settings").and_then(|p| package.read_xml(&p).ok()).is_some_and(|xml| {
        xml.contains("documentProtection")
            && xml.contains("trackedChanges")
            && !xml.contains("enforcement=\"0\"")
            && !xml.contains("enforcement=\"false\"")
    });
    let track_changes = (part_of("/settings").and_then(|p| package.read_xml(&p).ok()).is_some_and(|xml| xml.contains("<w:trackRevisions") && !xml.contains("trackRevisions w:val=\"0\"") && !xml.contains("trackRevisions w:val=\"false\""))) || lock_tracking;
    let restart_numbering = sect.and_then(|s| child(s, "pgNumType")).and_then(|n| wattr(n, "start")).is_some();
    // 見開きページ and とじしろの位置 (the settings: the document's), kept
    // with each section's page setup.
    let settings_on = |name: &str| {
        part_of("/settings").and_then(|p| package.read_xml(&p).ok()).is_some_and(|xml| xml.contains(&format!("<w:{name}")) && !xml.contains(&format!("{name} w:val=\"0\"")) && !xml.contains(&format!("{name} w:val=\"false\"")))
    };
    let (mirror_margins, gutter_at_top) = (settings_on("mirrorMargins"), settings_on("gutterAtTop"));
    let book_fold = settings_on("bookFoldPrinting");
    let reverse_book_fold = settings_on("bookFoldRevPrinting");
    let two_on_one = settings_on("printTwoOnOne");
    let book_fold_sheets = part_of("/settings").and_then(|p| package.read_xml(&p).ok()).and_then(|xml| {
        let at = xml.find("<w:bookFoldPrintingSheets")?;
        let rest = &xml[at..xml[at..].find('>').map(|e| at + e).unwrap_or(xml.len())];
        let v = rest.find("w:val=\"")? + "w:val=\"".len();
        rest[v..v + rest[v..].find('"')?].parse::<u32>().ok()
    });
    let multiple_pages = if reverse_book_fold {
        ibis_doc_model::MultiplePages::ReverseBookFold
    } else if book_fold {
        ibis_doc_model::MultiplePages::BookFold
    } else if two_on_one {
        ibis_doc_model::MultiplePages::TwoOnOne
    } else if mirror_margins {
        ibis_doc_model::MultiplePages::MirrorMargins
    } else {
        ibis_doc_model::MultiplePages::Normal
    };
    let read_only_recommended = settings_on("readOnlyRecommended");
    let update_fields = settings_on("updateFields");
    // 差し込み印刷の文書の種類 (`w:mailMerge/w:mainDocumentType`).
    let mail_merge_type = part_of("/settings").and_then(|p| package.read_xml(&p).ok()).and_then(|xml| {
        let at = xml.find("<w:mainDocumentType")?;
        let rest = &xml[at..xml[at..].find('>').map(|e| at + e)?];
        let v = rest.find("w:val=\"")? + "w:val=\"".len();
        Some(rest[v..v + rest[v..].find('"')?].to_string())
    });
    // 差し込み印刷のフィールドと列の対応 (`w:mailMerge/w:odso/w:fieldMapData`, DOCX-MERGE-009).
    let settings_xml_for_merge = part_of("/settings").and_then(|p| package.read_xml(&p).ok());
    let mail_merge_field_map = settings_xml_for_merge
        .as_deref()
        .and_then(|xml| roxmltree::Document::parse(xml).ok())
        .map(|doc| {
            let mut out = Vec::new();
            for fmd in doc.descendants().filter(|n| n.is_element() && n.tag_name().name() == "fieldMapData") {
                let name = child(fmd, "name").and_then(|n| wattr(n, "val")).unwrap_or_default().to_string();
                let mapped_name = child(fmd, "mappedName").and_then(|n| wattr(n, "val")).unwrap_or_default().to_string();
                let column = child(fmd, "column").and_then(|n| wattr(n, "val")).and_then(|v| v.parse().ok()).unwrap_or(0);
                if !name.is_empty() || !mapped_name.is_empty() {
                    out.push(ibis_doc_model::merge::FieldMapData { name, mapped_name, column });
                }
            }
            out
        })
        .unwrap_or_default();
    // 禁則文字の設定: the lists for Japanese (`w:lang` ja…).
    let kinsoku_list = |name: &str| -> Option<String> {
        let xml = part_of("/settings").and_then(|p| package.read_xml(&p).ok())?;
        let doc = roxmltree::Document::parse(&xml).ok()?;
        doc.descendants().find(|n| n.is_element() && n.tag_name().name() == name && wattr(*n, "lang").is_some_and(|l| l.starts_with("ja"))).and_then(|n| wattr(n, "val")).map(str::to_string)
    };
    let (no_line_start, no_line_end) = (kinsoku_list("noLineBreaksBefore"), kinsoku_list("noLineBreaksAfter"));
    // 禁則処理の方式 (Word options / w:strictFirstAndLastChars, DOCX-JA-010, ECMA-376 Part 1 §17.15.1.84).
    let strict_first_and_last_chars = settings_on("strictFirstAndLastChars");
    let balance_single_double_byte_width = settings_on("balanceSingleByteDoubleByteWidth");
    let adjust_line_height_in_table = settings_on("adjustLineHeightInTable");
    let do_not_use_html_paragraph_auto_spacing = settings_on("doNotUseHTMLParagraphAutoSpacing");
    let compat_mode = part_of("/settings").and_then(|p| package.read_xml(&p).ok()).and_then(|xml| {
        let at = xml.find("compatibilityMode")?;
        let rest = &xml[at..];
        let v = rest.find("w:val=\"")? + 7;
        rest[v..].split('"').next()?.parse::<u32>().ok()
    });
    // The font table's typefaces and their families (`w:font/w:family`).
    let font_families: std::collections::BTreeMap<String, String> = part_of("/fontTable").and_then(|p| package.read_xml(&p).ok()).and_then(|xml| {
        let doc = roxmltree::Document::parse(&xml).ok()?;
        Some(doc.descendants().filter(|n| n.is_element() && n.tag_name().name() == "font").filter_map(|f| {
            let name = f.attributes().find(|a| a.name() == "name")?.value().to_string();
            let family = f.children().find(|c| c.is_element() && c.tag_name().name() == "family").and_then(|c| c.attributes().find(|a| a.name() == "val").map(|a| a.value().to_string())).unwrap_or_default();
            Some((name, family))
        }).collect())
    }).unwrap_or_default();
    // A document with no settings is read as Word does: with the punctuation squeezed.
    let compress_punctuation = part_of("/settings").and_then(|p| package.read_xml(&p).ok()).is_none_or(|xml| xml.contains("characterSpacingControl w:val=\"compressPunctuation"));
    // 文書の既定テーマフォント言語 (w:themeFontLang, DOCX-JA-019, ECMA-376 Part 1 §17.15.1.88).
    let theme_font_lang = part_of("/settings").and_then(|p| package.read_xml(&p).ok()).and_then(|xml| {
        let doc = roxmltree::Document::parse(&xml).ok()?;
        let node = doc.descendants().find(|n| n.is_element() && n.tag_name().name() == "themeFontLang")?;
        Some(ibis_doc_model::ThemeFontLang {
            val: wattr(node, "val").map(str::to_string),
            east_asia: wattr(node, "eastAsia").map(str::to_string),
            bidi: wattr(node, "bidi").map(str::to_string),
        })
    });
    // 画像の品質と圧縮設定 (Word options / w14:defaultImageDpi, w14:discardImageEditingData, doNotAutoCompressPictures, DOCX-OPTIONS-011).
    let do_not_compress_images = settings_on("doNotAutoCompressPictures");
    let default_image_dpi = part_of("/settings").and_then(|p| package.read_xml(&p).ok()).and_then(|xml| {
        let at = xml.find("defaultImageDpi")?;
        let rest = &xml[at..xml[at..].find('>').map(|e| at + e)?];
        let v = rest.find("val=\"")? + "val=\"".len();
        rest[v..v + rest[v..].find('"')?].parse::<u32>().ok()
    });
    let discard_image_editing_data = part_of("/settings").and_then(|p| package.read_xml(&p).ok()).is_some_and(|xml| {
        xml.contains("discardImageEditingData") && !xml.contains("discardImageEditingData w14:val=\"0\"") && !xml.contains("discardImageEditingData val=\"0\"")
    });
    // 自動回復用データの保存間隔 (Word options / w:saveFrequency, DOCX-OPTIONS-012, ECMA-376 Part 1 §17.15.1.72).
    let save_frequency = part_of("/settings").and_then(|p| package.read_xml(&p).ok()).and_then(|xml| {
        let at = xml.find("saveFrequency")?;
        let rest = &xml[at..xml[at..].find('>').map(|e| at + e)?];
        let v = rest.find("val=\"")? + "val=\"".len();
        rest[v..v + rest[v..].find('"')?].parse::<u32>().ok()
    });
    // フォントの埋め込み設定 (Word options / w:embedTrueTypeFonts, w:embedSystemFonts, w:saveSubsetFonts, DOCX-OPTIONS-013, ECMA-376 Part 1 §17.15.1.34, 35, 75).
    let embed_true_type_fonts = settings_on("embedTrueTypeFonts");
    let embed_system_fonts = settings_on("embedSystemFonts");
    let save_subset_fonts = settings_on("saveSubsetFonts");
    // 互換性オプション (Word options / w:compat, w:compatSetting, w:useFELayout, w:doNotExpandShiftReturn, DOCX-OPTIONS-013).
    let (compatibility_mode, use_fe_layout, do_not_expand_shift_return) = match part_of("/settings").and_then(|p| package.read_xml(&p).ok()) {
        Some(xml) => {
            let mode = if xml.contains("compatibilityMode") {
                let at = xml.find("compatibilityMode").unwrap();
                let rest = &xml[at..xml[at..].find('>').map(|e| at + e).unwrap_or(xml.len())];
                rest.find("val=\"").and_then(|v| rest[v + 5..].split('"').next()).and_then(|v| v.parse::<u32>().ok())
            } else {
                None
            };
            let fe = xml.contains("<w:useFELayout") && !xml.contains("useFELayout w:val=\"0\"") && !xml.contains("useFELayout w:val=\"false\"");
            let shift_return = xml.contains("<w:doNotExpandShiftReturn") && !xml.contains("doNotExpandShiftReturn w:val=\"0\"") && !xml.contains("doNotExpandShiftReturn w:val=\"false\"");
            (mode, fe, shift_return)
        }
        None => (None, false, false),
    };
    // ページの色: Word shows it only with displayBackgroundShape.
    let page_color = child(root, "background")
        .filter(|_| settings_on("displayBackgroundShape"))
        .and_then(|b| match wattr(b, "themeColor") {
            Some(t) => theme_color(&theme, t, wattr(b, "themeTint"), wattr(b, "themeShade")).or_else(|| wattr(b, "color").and_then(hex)),
            None => wattr(b, "color").and_then(hex),
        });
    let mut page = page;
    // The settings' note settings (the document's), where the last
    // section names none of its own.
    let mut doc_variables = std::collections::BTreeMap::new();
    let mut attached_template = None;
    let mut link_styles = false;
    let mut view_mode = ibis_doc_model::DocumentViewMode::Print;
    let mut protection = ibis_doc_model::DocumentProtection::None;
    let mut formatting_protection = false;
    if let Some(xml) = part_of("/settings").and_then(|p| package.read_xml(&p).ok())
        && let Ok(settings) = roxmltree::Document::parse(&xml)
    {
        if let Some(prot) = child(settings.root_element(), "documentProtection") {
            let enforced = wattr(prot, "enforcement").map_or(true, |v| v != "0" && v != "false");
            if enforced {
                if let Some(edit) = wattr(prot, "edit") {
                    protection = ibis_doc_model::DocumentProtection::from_wedit(edit);
                }
                formatting_protection = wattr(prot, "formatting").is_some_and(|v| v != "0" && v != "false" && v != "off");
            }
        }
        if let Some(v) = child(settings.root_element(), "view").and_then(|v| wattr(v, "val")) {
            view_mode = ibis_doc_model::DocumentViewMode::from_wval(v);
        }
        let own = |name: &str| sect.is_some_and(|s| child(s, name).is_some());
        if let Some(n) = child(settings.root_element(), "footnotePr").filter(|_| !own("footnotePr")) {
            page.footnotes = note_settings(n, page.footnotes);
        }
        if let Some(n) = child(settings.root_element(), "endnotePr").filter(|_| !own("endnotePr")) {
            page.endnotes = note_settings(n, page.endnotes);
        }
        if let Some(vars) = child(settings.root_element(), "docVars") {
            for v in children(vars, "docVar") {
                if let (Some(name), Some(val)) = (wattr(v, "name"), wattr(v, "val")) {
                    doc_variables.insert(name.to_string(), val.to_string());
                }
            }
        }
        if let Some(tmpl) = child(settings.root_element(), "attachedTemplate") {
            let rid = rattr(tmpl, "id").or_else(|| tmpl.attribute("r:id"));
            if let Some(rid) = rid {
                if let Some(settings_part) = part_of("/settings") {
                    if let Ok(settings_rels) = package.relationships(&settings_part) {
                        if let Some(rel) = settings_rels.get(rid) {
                            attached_template = Some(rel.target.clone());
                        }
                    }
                }
            }
        }
        if let Some(link) = child(settings.root_element(), "linkStyles") {
            link_styles = wattr(link, "val").map_or(true, |v| v != "0" && v != "false" && v != "off");
        }
    }
    page.mirror_margins = mirror_margins || multiple_pages.is_mirrored();
    page.multiple_pages = multiple_pages;
    page.book_fold_sheets = book_fold_sheets;
    page.gutter_at_top = gutter_at_top;
    for block in blocks.iter_mut() {
        if let Block::Paragraph(p) = block
            && let Some(section) = p.section.as_mut()
        {
            let section = Arc::make_mut(section);
            section.page.mirror_margins = page.mirror_margins;
            section.page.multiple_pages = multiple_pages;
            section.page.book_fold_sheets = book_fold_sheets;
            section.page.gutter_at_top = gutter_at_top;
        }
    }

    let properties = crate::package::read_doc_properties(&package);
    let mark_as_final = properties.custom.iter().any(|p| p.name == "_MarkAsFinal" && matches!(p.value, ibis_doc_model::CustomPropertyValue::Bool(true)));

    let document = Arc::new(FlowDocument {
        file_name: None,
        file_size: Some(data.len() as u64),
        page,
        header,
        footer,
        body: blocks,
        header_first,
        footer_first,
        header_even,
        footer_even,
        title_page,
        even_and_odd,
        default_tab_stop,
        page_number_format,
        page_chapter,
        restart_numbering,
        linked,
        footnotes,
        endnotes,
        comments,
        track_changes,
        styles: Arc::new(styles),
        id_floor,
        watermark,
        section_watermarks,
        page_color,
        paragraph_spacing_preset: None,
        style_set_preset: None,
        show_field_codes: false,
        form_design_mode: false,
        properties,
        mark_as_final,
        read_only_recommended,
        no_line_start,
        no_line_end,
        strict_first_and_last_chars,
        compress_punctuation,
        font_families,
        balance_single_double_byte_width,
        adjust_line_height_in_table,
        do_not_use_html_paragraph_auto_spacing,
        compat_mode,
        theme_font_lang,
        update_fields,
        note_separators,
        mail_merge_type,
        mail_merge_field_map,
        lock_tracking,
        doc_variables,
        attached_template,
        link_styles,
        view_mode,
        protection,
        formatting_protection,
        interaction_mode: if view_mode == ibis_doc_model::DocumentViewMode::Reading || protection == ibis_doc_model::DocumentProtection::ReadOnly {
            ibis_doc_model::DocumentInteractionMode::Viewing
        } else if track_changes {
            ibis_doc_model::DocumentInteractionMode::Reviewing
        } else {
            ibis_doc_model::DocumentInteractionMode::Editing
        },
        do_not_compress_images,
        default_image_dpi,
        discard_image_editing_data,
        save_frequency,
        embed_true_type_fonts,
        embed_system_fonts,
        save_subset_fonts,
        compatibility_mode,
        use_fe_layout,
        do_not_expand_shift_return,
        shape_groups,
    });
    let id = presentation.new_slide_id();
    let mut slide = Slide::new(id);
    let shape = presentation.new_shape_id();
    slide.shapes.push(Shape {
        id: shape,
        frame: ibis_doc_model::EmuRect::new(0, 0, page.size.width, page.size.height),
        rotation: 0,
        flip_h: false,
        flip_v: false,
        kind: ShapeKind::Page { document: document.clone(), index: 0 },
        floating: None,
    });
    presentation.slides.push(slide);
    presentation.theme_colors = theme.colors();
    presentation.theme_fonts = Some(theme.fonts());
    presentation.theme_effects = theme.fmt_scheme_name.clone();
    presentation.flow = Some(document);
    Ok((presentation, sources))
}

#[cfg(test)]
mod namespace_tests {
    use super::*;

    #[test]
    fn story_references_require_relationship_namespace() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let source = std::fs::read(root.join("test/editing/docx-report.docx")).unwrap();
        let reference = crate::read_docx(&source).unwrap();
        for variant in 0..3 {
            let data = crate::test_files::docx_with_story_relationship_namespace(&source, variant);
            let input = root.join(format!(
                "target/scenario-input/docx-story-namespace-{variant}.docx"
            ));
            std::fs::create_dir_all(input.parent().unwrap()).unwrap();
            std::fs::write(input, &data).unwrap();
            let actual = crate::read_docx(&data).unwrap();
            let mut expected = reference.flow.clone();
            if let Some(doc) = expected.as_mut() {
                let doc = Arc::make_mut(doc);
                doc.file_size = actual.flow.as_ref().and_then(|f| f.file_size);
            }
            assert!(
                actual.flow == expected,
                "correct header/footer relationship URI, variant {variant}"
            );
        }
    }

    #[test]
    fn inline_picture_extent_is_unqualified() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../test/editing/docx-paste-source.docx");
        let data = std::fs::read(path).unwrap();
        let xml = crate::test_files::part_text(&data, "word/document.xml").unwrap();
        let source = Document::parse(&xml).unwrap();
        let expected: Vec<EmuSize> = source.descendants().filter(|n| n.is_element() && n.tag_name().namespace() == Some("http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing") && n.tag_name().name() == "extent")
            .map(|n| EmuSize::new(n.attribute("cx").unwrap().parse().unwrap(), n.attribute("cy").unwrap().parse().unwrap())).collect();
        assert!(!expected.is_empty());
        let presentation = crate::read_docx(&data).unwrap();
        let flow = presentation.flow.unwrap();
        let actual: Vec<EmuSize> = flow
            .body
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph(p) => Some(p),
                _ => None,
            })
            .flat_map(|p| &p.runs)
            .filter_map(|r| match r.kind {
                FlowRunKind::Picture { size, .. } => Some(size),
                _ => None,
            })
            .collect();
        assert_eq!(
            actual, expected,
            "wp:extent cx/cy have no attribute namespace"
        );
    }

    #[test]
    fn relationship_attributes_require_namespace() {
        let ns = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
        let cases = [
            (
                "standard-first",
                r#"r:id="Good" f:id="Wrong""#,
                Some("Good"),
                ns,
            ),
            (
                "foreign-first",
                r#"f:id="Wrong" r:id="Good""#,
                Some("Good"),
                ns,
            ),
            ("foreign-only", r#"f:id="Wrong""#, None, ns),
            ("unqualified", r#"id="Wrong""#, None, ns),
            (
                "alternate-prefix",
                r#"q:id="Good" f:id="Wrong""#,
                Some("Good"),
                ns,
            ),
            ("forged-prefix", r#"r:id="Wrong""#, None, "urn:foreign"),
        ];
        for (name, attrs, expected, r_namespace) in cases {
            for attribute in ["id", "embed"] {
                let attrs = attrs.replace("id=", &format!("{attribute}="));
                let xml = format!(
                    r#"<root xmlns:r="{r_namespace}" xmlns:q="{ns}" xmlns:f="urn:foreign" {attrs}/>"#
                );
                let doc = Document::parse(&xml).unwrap();
                assert_eq!(
                    rattr(doc.root_element(), attribute),
                    expected,
                    "{name}/{attribute}"
                );
            }
        }
    }

    // XML accessor negatives are not adopted as valid Office fixtures.
    #[test]
    fn word_attributes_require_namespace() {
        let ns = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
        let standard = r#"w:author="Word" w:date="2026-10-01T00:00:00Z" w:id="941""#;
        let foreign = r#"f:author="Wrong" f:date="Wrong" f:id="1""#;
        let cases = [
            (
                "standard-first",
                "w",
                format!("{standard} {foreign}"),
                true,
                ns,
            ),
            (
                "foreign-first",
                "w",
                format!("{foreign} {standard}"),
                true,
                ns,
            ),
            ("foreign-only", "w", foreign.to_string(), false, ns),
            (
                "unqualified",
                "w",
                r#"author="Wrong" date="Wrong" id="1""#.to_string(),
                false,
                ns,
            ),
            (
                "alternate-prefix",
                "w",
                standard.replace("w:", "x:"),
                true,
                ns,
            ),
            (
                "forged-word-prefix",
                "x",
                standard.to_string(),
                false,
                "urn:ibis:test:foreign",
            ),
        ];
        let mut failures = Vec::new();
        for (name, prefix, attrs, has_standard, word_binding) in cases {
            let xml = format!(
                r#"<{prefix}:ins xmlns:w="{word_binding}" xmlns:x="{ns}" xmlns:f="urn:ibis:test:foreign" {attrs}/>"#
            );
            let doc = Document::parse(&xml).unwrap();
            let node = doc.root_element();
            let expected = if has_standard {
                (Some("Word"), Some("2026-10-01T00:00:00Z"), Some("941"))
            } else {
                (None, None, None)
            };
            let actual = (
                wattr(node, "author"),
                wattr(node, "date"),
                wattr(node, "id"),
            );
            if actual != expected {
                failures.push(name);
            }
        }
        assert!(
            failures.is_empty(),
            "namespace attribute counterexamples: {failures:?}"
        );
    }

    #[test]
    fn word_theme_color_hybrid_modulation() {
        let theme_xml = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme">
            <a:themeElements>
                <a:clrScheme name="Office">
                    <a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
                    <a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
                    <a:dk2><a:srgbClr val="44546A"/></a:dk2>
                    <a:lt2><a:srgbClr val="E7E6E6"/></a:lt2>
                    <a:accent1><a:srgbClr val="4472C4"/></a:accent1>
                    <a:accent2><a:srgbClr val="ED7D31"/></a:accent2>
                    <a:accent3><a:srgbClr val="A5A5A5"/></a:accent3>
                    <a:accent4><a:srgbClr val="FFC000"/></a:accent4>
                    <a:accent5><a:srgbClr val="5B9BD5"/></a:accent5>
                    <a:accent6><a:srgbClr val="70AD47"/></a:accent6>
                    <a:hlink><a:srgbClr val="0563C1"/></a:hlink>
                    <a:folHlink><a:srgbClr val="954F72"/></a:folHlink>
                </a:clrScheme>
            </a:themeElements>
        </a:theme>"#;
        let doc = Document::parse(theme_xml).unwrap();
        let theme = Theme::read(doc.root_element());

        // 1. DCLR-00004: dark1 (000000) with themeFillTint="99" (0.6)
        // Word uses sRGB gamma direct interpolation:
        // c = 0 + (255 - 0) * (1 - 0.6) = 102 (0x66)
        let tint_dark1 = theme_color(&theme, "dark1", Some("99"), None).expect("tint dark1");
        assert_eq!(tint_dark1, Rgb(0x666666), "Word themeTint dark1 99 must match sRGB gamma direct interpolation (102, 102, 102)");

        // 2. Shades scale RGB channels
        assert_eq!(theme_color(&theme, "accent1", None, Some("BF")), Some(Rgb(0x335593)));
        assert_eq!(theme_color(&theme, "accent4", None, Some("BF")), Some(Rgb(0xbf9000)));
        assert_eq!(theme_color(&theme, "accent6", None, Some("BF")), Some(Rgb(0x548235)));
        assert_eq!(theme_color(&theme, "light1", None, Some("BF")), Some(Rgb(0xbfbfbf)));
    }

    #[test]
    fn word_shading_two_color_patterns() {
        let theme_xml = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme"><a:themeElements><a:clrScheme name="Office"/></a:themeElements></a:theme>"#;
        let tdoc = Document::parse(theme_xml).unwrap();
        let theme = Theme::read(tdoc.root_element());

        // Two-color stripe: horzStripe with red pattern and yellow fill
        let xml = r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:pPr><w:shd w:val="horzStripe" w:color="FF0000" w:fill="FFFF00"/></w:pPr></w:p>"#;
        let doc = Document::parse(xml).unwrap();
        let ppr = child(doc.root_element(), "pPr").unwrap();
        let f = para_format(ppr, &theme);
        assert_eq!(f.shading_pattern, Some(Some(ShadingPattern::HorzStripe)));
        assert_eq!(f.shading_color, Some(Some(Rgb(0xff0000))));
        assert_eq!(f.shading_fill, Some(Some(Rgb(0xffff00))));
        // Overall mixed shading color is 50% between red (FF0000) and yellow (FFFF00)
        assert_eq!(f.shading, Some(Some(Rgb(0xff8000))));

        // Standard clear shading should NOT set shading_pattern, shading_color, or shading_fill
        let xml_clear = r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:pPr><w:shd w:val="clear" w:color="auto" w:fill="00FF00"/></w:pPr></w:p>"#;
        let doc_clear = Document::parse(xml_clear).unwrap();
        let ppr_clear = child(doc_clear.root_element(), "pPr").unwrap();
        let f_clear = para_format(ppr_clear, &theme);
        assert_eq!(f_clear.shading_pattern, None);
        assert_eq!(f_clear.shading_color, None);
        assert_eq!(f_clear.shading_fill, None);
        assert_eq!(f_clear.shading, Some(Some(Rgb(0x00ff00))));
    }

    #[test]
    fn word_underline_theme_color() {
        let theme_xml = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme"><a:themeElements><a:clrScheme name="Office">
            <a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>
            <a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
            <a:dk2><a:srgbClr val="44546A"/></a:dk2>
            <a:lt2><a:srgbClr val="E7E6E6"/></a:lt2>
            <a:accent1><a:srgbClr val="4472C4"/></a:accent1>
            <a:accent2><a:srgbClr val="ED7D31"/></a:accent2>
            <a:accent3><a:srgbClr val="A5A5A5"/></a:accent3>
            <a:accent4><a:srgbClr val="FFC000"/></a:accent4>
            <a:accent5><a:srgbClr val="5B9BD5"/></a:accent5>
            <a:accent6><a:srgbClr val="70AD47"/></a:accent6>
        </a:clrScheme></a:themeElements></a:theme>"#;
        let tdoc = Document::parse(theme_xml).unwrap();
        let theme = Theme::read(tdoc.root_element());

        // Underline with themeColor dark1
        let xml_dark1 = r#"<w:r xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:rPr><w:u w:val="dash" w:themeColor="dark1"/></w:rPr></w:r>"#;
        let doc_dark1 = Document::parse(xml_dark1).unwrap();
        let rpr_dark1 = child(doc_dark1.root_element(), "rPr").unwrap();
        let f_dark1 = run_format(rpr_dark1, &theme);
        assert_eq!(f_dark1.underline, Some(true));
        assert_eq!(f_dark1.underline_style, Some(UnderlineStyle::Dash));
        assert_eq!(f_dark1.underline_color, Some(Some(Rgb(0x000000))));

        // Underline with themeColor accent1 and themeShade BF
        let xml_accent1 = r#"<w:r xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:rPr><w:u w:val="single" w:themeColor="accent1" w:themeShade="BF"/></w:rPr></w:r>"#;
        let doc_accent1 = Document::parse(xml_accent1).unwrap();
        let rpr_accent1 = child(doc_accent1.root_element(), "rPr").unwrap();
        let f_accent1 = run_format(rpr_accent1, &theme);
        assert_eq!(f_accent1.underline, Some(true));
        assert_eq!(f_accent1.underline_color, Some(Some(Rgb(0x335593))));

        // Underline with explicit color
        let xml_color = r#"<w:r xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:rPr><w:u w:val="single" w:color="FF0000"/></w:rPr></w:r>"#;
        let doc_color = Document::parse(xml_color).unwrap();
        let rpr_color = child(doc_color.root_element(), "rPr").unwrap();
        let f_color = run_format(rpr_color, &theme);
        assert_eq!(f_color.underline_color, Some(Some(Rgb(0xff0000))));
    }

    #[test]
    fn word_run_border() {
        let theme_xml = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme"><a:themeElements><a:clrScheme name="Office">
            <a:accent1><a:srgbClr val="4472C4"/></a:accent1>
        </a:clrScheme></a:themeElements></a:theme>"#;
        let tdoc = Document::parse(theme_xml).unwrap();
        let theme = Theme::read(tdoc.root_element());

        // Single border with explicit color
        let xml_single = r#"<w:r xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:rPr><w:bdr w:val="single" w:sz="4" w:space="0" w:color="C00000"/></w:rPr></w:r>"#;
        let doc_single = Document::parse(xml_single).unwrap();
        let rpr_single = child(doc_single.root_element(), "rPr").unwrap();
        let f_single = run_format(rpr_single, &theme);
        let b = f_single.border.expect("has border").expect("some border");
        assert_eq!(b.style, BorderStyle::Single);
        assert_eq!(b.width, 4 * 12_700 / 8);
        assert_eq!(b.color, Rgb(0xc00000));
        assert_eq!(b.space, 0);

        // Nil border
        let xml_nil = r#"<w:r xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:rPr><w:bdr w:val="nil"/></w:rPr></w:r>"#;
        let doc_nil = Document::parse(xml_nil).unwrap();
        let rpr_nil = child(doc_nil.root_element(), "rPr").unwrap();
        let f_nil = run_format(rpr_nil, &theme);
        assert_eq!(f_nil.border, Some(None));

        // Themed double border
        let xml_theme = r#"<w:r xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:rPr><w:bdr w:val="double" w:sz="12" w:space="2" w:themeColor="accent1"/></w:rPr></w:r>"#;
        let doc_theme = Document::parse(xml_theme).unwrap();
        let rpr_theme = child(doc_theme.root_element(), "rPr").unwrap();
        let f_theme = run_format(rpr_theme, &theme);
        let bt = f_theme.border.expect("has border").expect("some border");
        assert_eq!(bt.style, BorderStyle::Double);
        assert_eq!(bt.width, 12 * 12_700 / 8);
        assert_eq!(bt.space, 2 * 12_700);
        assert_eq!(bt.color, Rgb(0x4472c4));
    }

    #[test]
    fn word_jc_alignments() {
        let theme_xml = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme"><a:themeElements><a:clrScheme name="Office"/></a:themeElements></a:theme>"#;
        let tdoc = Document::parse(theme_xml).unwrap();
        let theme = Theme::read(tdoc.root_element());

        let check = |val: &str| {
            let xml = format!(r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:pPr><w:jc w:val="{val}"/></w:pPr></w:p>"#);
            let doc = Document::parse(&xml).unwrap();
            let ppr = child(doc.root_element(), "pPr").unwrap();
            para_format(ppr, &theme).align
        };

        assert_eq!(check("both"), Some(FlowAlign::Justify));
        assert_eq!(check("mediumKashida"), Some(FlowAlign::Justify));
        assert_eq!(check("highKashida"), Some(FlowAlign::Justify));
        assert_eq!(check("lowKashida"), Some(FlowAlign::Justify));
        assert_eq!(check("distribute"), Some(FlowAlign::Distribute));
        assert_eq!(check("thaiDistribute"), Some(FlowAlign::Distribute));
        assert_eq!(check("numTab"), Some(FlowAlign::Left));
        assert_eq!(check("left"), Some(FlowAlign::Left));
        assert_eq!(check("start"), Some(FlowAlign::Left));
        assert_eq!(check("center"), Some(FlowAlign::Center));
        assert_eq!(check("right"), Some(FlowAlign::Right));
        assert_eq!(check("end"), Some(FlowAlign::Right));
    }

    #[test]
    fn word_text_alignment() {
        let theme_xml = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme"><a:themeElements><a:clrScheme name="Office"/></a:themeElements></a:theme>"#;
        let tdoc = Document::parse(theme_xml).unwrap();
        let theme = Theme::read(tdoc.root_element());

        let check = |val: &str| {
            let xml = format!(r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:pPr><w:textAlignment w:val="{val}"/></w:pPr></w:p>"#);
            let doc = Document::parse(&xml).unwrap();
            let ppr = child(doc.root_element(), "pPr").unwrap();
            para_format(ppr, &theme).text_alignment
        };

        assert_eq!(check("top"), Some(FontAlign::Top));
        assert_eq!(check("center"), Some(FontAlign::Center));
        assert_eq!(check("bottom"), Some(FontAlign::Bottom));
        assert_eq!(check("baseline"), Some(FontAlign::Baseline));
        assert_eq!(check("auto"), Some(FontAlign::Auto));
    }

    #[test]
    fn word_highlight_palette() {
        assert_eq!(highlight("darkGreen"), Some(Rgb(0x006400)));
        assert_eq!(highlight("darkBlue"), Some(Rgb(0x00008b)));
        assert_eq!(highlight("darkCyan"), Some(Rgb(0x008b8b)));
        assert_eq!(highlight("darkRed"), Some(Rgb(0x8b0000)));
        assert_eq!(highlight("darkMagenta"), Some(Rgb(0x8b008b)));
        assert_eq!(highlight("darkGray"), Some(Rgb(0xa9a9a9)));
        assert_eq!(highlight("lightGray"), Some(Rgb(0xd3d3d3)));
        assert_eq!(highlight("yellow"), Some(Rgb(0xffff00)));
        assert_eq!(highlight("none"), None);
    }

    #[test]
    fn word_paragraph_properties_and_char_indents() {
        let theme_xml = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme"><a:themeElements><a:clrScheme name="Office"/></a:themeElements></a:theme>"#;
        let tdoc = Document::parse(theme_xml).unwrap();
        let theme = Theme::read(tdoc.root_element());

        let xml = r#"<w:p xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
            <w:pPr>
                <w:rPr><w:sz w:val="20"/></w:rPr>
                <w:ind w:firstLineChars="200" w:leftChars="150" w:rightChars="100"/>
                <w:kinsoku w:val="0"/>
                <w:wordWrap w:val="0"/>
                <w:overflowPunct w:val="0"/>
                <w:bidi/>
            </w:pPr>
        </w:p>"#;
        let doc = Document::parse(xml).unwrap();
        let ppr = child(doc.root_element(), "pPr").unwrap();
        let fmt = para_format(ppr, &theme);

        assert_eq!(fmt.kinsoku, Some(false));
        assert_eq!(fmt.word_wrap, Some(false));
        assert_eq!(fmt.overflow_punct, Some(false));
        assert_eq!(fmt.bidi, Some(true));
        // sz=20 (10 pt = 200 twips). 200 chars = 2 chars = 400 twips * EMU_PER_TWIP = 400 * 635 = 254,000 EMUs
        assert_eq!(fmt.first_line, Some(400 * 635));
        assert_eq!(fmt.left, Some(300 * 635));
        assert_eq!(fmt.right, Some(200 * 635));
    }

    #[test]
    fn word_floating_anchor_parsing() {
        let xml = r#"<wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"
            distT="0" distB="0" distL="114300" distR="114300" behindDoc="1">
            <wp:positionH relativeFrom="column">
                <wp:align>center</wp:align>
            </wp:positionH>
            <wp:positionV relativeFrom="paragraph">
                <wp:posOffset>127000</wp:posOffset>
            </wp:positionV>
            <wp:extent cx="1371600" cy="914400"/>
            <wp:wrapSquare wrapText="bothSides"/>
        </wp:anchor>"#;
        let doc = Document::parse(xml).unwrap();
        let anchor = Reader::parse_floating_anchor(doc.root_element()).expect("floating anchor");
        assert_eq!(anchor.behind_doc, true);
        assert_eq!(anchor.relative_h, "column");
        assert_eq!(anchor.align_h.as_deref(), Some("center"));
        assert_eq!(anchor.relative_v, "paragraph");
        assert_eq!(anchor.offset_y, 127000);
        assert_eq!(anchor.wrap, "square");
    }
}





/// The preset shape name (`ST_ShapeType`) of a VML shape type number (`o:spt`, MS-ODRAW).
fn vml_spt_preset(spt: u32) -> Option<&'static str> {
    Some(match spt {
        1 | 75 | 202 => "rect",
        2 => "roundRect",
        3 => "ellipse",
        4 => "diamond",
        5 => "triangle",
        6 => "rtTriangle",
        7 => "parallelogram",
        8 => "trapezoid",
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
        33 => "bentConnector2",
        34 => "bentConnector3",
        35 => "bentConnector4",
        36 => "bentConnector5",
        37 => "curvedConnector2",
        38 => "curvedConnector3",
        39 => "curvedConnector4",
        40 => "curvedConnector5",
        55 => "chevron",
        56 => "pentagon",
        57 => "noSmoking",
        58 => "star8",
        59 => "star16",
        60 => "star32",
        61 => "wedgeRectCallout",
        62 => "wedgeRoundRectCallout",
        63 => "wedgeEllipseCallout",
        64 => "wave",
        65 => "foldedCorner",
        66 => "leftArrow",
        67 => "downArrow",
        68 => "upArrow",
        69 => "leftRightArrow",
        70 => "upDownArrow",
        73 => "lightningBolt",
        74 => "heart",
        76 => "quadArrow",
        84 => "bevel",
        85 => "leftBracket",
        86 => "rightBracket",
        87 => "leftBrace",
        88 => "rightBrace",
        95 => "blockArc",
        96 => "smileyFace",
        97 => "verticalScroll",
        98 => "horizontalScroll",
        99 => "circularArrow",
        109 => "flowChartProcess",
        110 => "flowChartDecision",
        111 => "flowChartInputOutput",
        112 => "flowChartPredefinedProcess",
        113 => "flowChartInternalStorage",
        114 => "flowChartDocument",
        115 => "flowChartMultidocument",
        116 => "flowChartTerminator",
        117 => "flowChartPreparation",
        118 => "flowChartManualInput",
        119 => "flowChartManualOperation",
        120 => "flowChartConnector",
        176 => "flowChartAlternateProcess",
        183 => "sun",
        184 => "moon",
        185 => "bracketPair",
        186 => "bracePair",
        187 => "star4",
        188 => "doubleWave",
        _ => return None,
    })
}

/// A VML `style` attribute's properties (`width:54pt;height:30pt`), the names as they are written.
pub(crate) fn vml_style(value: &str) -> std::collections::HashMap<String, String> {
    value.split(';').filter_map(|kv| kv.split_once(':')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
}

/// A VML length (`54pt`, `1.5in`, `2cm`, `10mm`, `20px`; no unit is pixels) in EMU.
pub(crate) fn vml_length(value: &str) -> Option<i64> {
    let v = value.trim();
    let end = v.find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | '-' | '+'))).unwrap_or(v.len());
    let n: f64 = v[..end].parse().ok()?;
    let k = match v[end..].trim() {
        "pt" => 12_700.0,
        "in" => 914_400.0,
        "cm" => 360_000.0,
        "mm" => 36_000.0,
        "pc" => 152_400.0,
        "px" | "" => 9_525.0,
        _ => return None,
    };
    Some((n * k).round() as i64)
}

/// A VML opacity (`.5` or `32768f`, the fraction of 65536) as an alpha, 0 to 255.
pub(crate) fn vml_opacity(value: &str) -> u8 {
    let v = value.trim();
    let fraction = match v.strip_suffix('f') {
        Some(n) => n.parse::<f64>().map(|x| x / 65536.0),
        None => v.parse::<f64>(),
    };
    (fraction.unwrap_or(1.0).clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A step of a VML path.
pub(crate) enum VmlPath {
    Move(i64, i64),
    Line(i64, i64),
    Curve([i64; 6]),
    Close,
}

/// The steps of a VML `path` (`m0,0l21600,0,21600,21600xe`): `m` `l` `c` and `x`; `e` ends it.
pub(crate) fn vml_path(value: &str) -> Vec<VmlPath> {
    let mut out = Vec::new();
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if !c.is_ascii_alphabetic() {
            continue;
        }
        let mut numbers = String::new();
        while let Some(&n) = chars.peek() {
            if n.is_ascii_alphabetic() {
                break;
            }
            numbers.push(n);
            chars.next();
        }
        let n: Vec<i64> = numbers.split(|ch: char| ch == ',' || ch.is_whitespace()).filter(|t| !t.is_empty()).filter_map(|t| t.parse().ok()).collect();
        match c {
            'm' => {
                for (i, pair) in n.chunks_exact(2).enumerate() {
                    out.push(if i == 0 { VmlPath::Move(pair[0], pair[1]) } else { VmlPath::Line(pair[0], pair[1]) });
                }
            }
            'l' => out.extend(n.chunks_exact(2).map(|pair| VmlPath::Line(pair[0], pair[1]))),
            'c' => out.extend(n.chunks_exact(6).map(|six| VmlPath::Curve([six[0], six[1], six[2], six[3], six[4], six[5]]))),
            'x' => out.push(VmlPath::Close),
            'e' => break,
            _ => {}
        }
    }
    out
}

/// A geometry of one path of `steps` in a box `width` × `height` of its own units; not filled when `filled` is off.
pub(crate) fn vml_custom_geometry(width: i64, height: i64, steps: &[VmlPath], filled: bool) -> ibis_doc_model::Geometry {
    let pt = |x: i64, y: i64| format!("<a:pt x=\"{x}\" y=\"{y}\"/>");
    let mut path = String::new();
    for step in steps {
        match step {
            VmlPath::Move(x, y) => path.push_str(&format!("<a:moveTo>{}</a:moveTo>", pt(*x, *y))),
            VmlPath::Line(x, y) => path.push_str(&format!("<a:lnTo>{}</a:lnTo>", pt(*x, *y))),
            VmlPath::Curve(c) => path.push_str(&format!("<a:cubicBezTo>{}{}{}</a:cubicBezTo>", pt(c[0], c[1]), pt(c[2], c[3]), pt(c[4], c[5]))),
            VmlPath::Close => path.push_str("<a:close/>"),
        }
    }
    let fill = if filled { "" } else { " fill=\"none\"" };
    let xml = format!("<a:custGeom xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:avLst/><a:gdLst/><a:ahLst/><a:cxnLst/><a:rect l=\"0\" t=\"0\" r=\"r\" b=\"b\"/><a:pathLst><a:path w=\"{}\" h=\"{}\"{fill}>{path}</a:path></a:pathLst></a:custGeom>", width.max(1), height.max(1));
    match roxmltree::Document::parse(&xml) {
        Ok(doc) => crate::geometry::geometry(Some(doc.root_element())),
        Err(_) => ibis_doc_model::Geometry::Rect,
    }
}

/// The colour a VML colour (`#rrggbb`, `#rgb` or a name) is.
pub(crate) fn vml_color(value: &str) -> Option<ibis_doc_model::Rgb> {
    let v = value.trim().split_whitespace().next()?;
    if let Some(hex) = v.strip_prefix('#') {
        let hex: String = if hex.len() == 3 { hex.chars().flat_map(|c| [c, c]).collect() } else { hex.to_string() };
        let n = u32::from_str_radix(hex.get(..6)?, 16).ok()?;
        return Some(ibis_doc_model::Rgb(n));
    }
    let n = match v.to_ascii_lowercase().as_str() {
        "black" => 0x000000, "silver" => 0xC0C0C0, "gray" | "grey" => 0x808080, "white" => 0xFFFFFF,
        "maroon" => 0x800000, "red" => 0xFF0000, "purple" => 0x800080, "fuchsia" => 0xFF00FF,
        "green" => 0x008000, "lime" => 0x00FF00, "olive" => 0x808000, "yellow" => 0xFFFF00,
        "navy" => 0x000080, "blue" => 0x0000FF, "teal" => 0x008080, "aqua" => 0x00FFFF,
        _ => return None,
    };
    Some(ibis_doc_model::Rgb(n))
}

/// Word's text watermark in a header (`v:shape` `PowerPlusWaterMarkObject…`
/// with a `v:textpath`).
pub(crate) fn watermark_in(root: Node) -> Option<ibis_doc_model::Watermark> {
    let shape = root.descendants().find(|n| n.is_element() && n.tag_name().name() == "shape" && n.attribute("id").is_some_and(|id| id.starts_with("PowerPlusWaterMarkObject")))?;
    let path = shape.children().find(|n| n.is_element() && n.tag_name().name() == "textpath")?;
    let text = path.attribute("string")?.to_string();
    let style = |s: &str, key: &str| -> Option<String> {
        s.split(';').find_map(|kv| {
            let (k, v) = kv.split_once(':')?;
            (k.trim() == key).then(|| v.trim().trim_matches('"').trim_matches('\'').to_string())
        })
    };
    let font = path.attribute("style").and_then(|s| style(s, "font-family")).unwrap_or_default();
    let color = shape.attribute("fillcolor").and_then(vml_color).unwrap_or(ibis_doc_model::Rgb(0xC0C0C0));
    let opacity = shape.children().find(|n| n.is_element() && n.tag_name().name() == "fill").and_then(|f| f.attribute("opacity"));
    let semitransparent = opacity.is_some_and(|o| {
        let o = o.trim();
        let v = if let Some(f) = o.strip_suffix('f') { f.parse::<f32>().map(|x| x / 65536.0).unwrap_or(1.0) } else { o.parse::<f32>().unwrap_or(1.0) };
        v < 0.99
    });
    let rotation = shape.attribute("style").and_then(|s| style(s, "rotation")).and_then(|r| r.parse::<f32>().ok()).unwrap_or(0.0);
    let rotation = rotation.round() as i32;
    let shape_style = shape.attribute("style").unwrap_or("");
    let length = |key: &str| -> Option<u32> {
        let v = style(shape_style, key)?;
        let (n, k) = if let Some(n) = v.strip_suffix("pt") { (n, 100.0) } else if let Some(n) = v.strip_suffix("in") { (n, 7200.0) } else if let Some(n) = v.strip_suffix("cm") { (n, 7200.0 / 2.54) } else if let Some(n) = v.strip_suffix("mm") { (n, 720.0 / 2.54) } else { (v.as_str(), 100.0 * 0.75) };
        n.trim().parse::<f32>().ok().filter(|x| *x > 0.0).map(|x| (x * k).round() as u32)
    };
    let size = length("width").zip(length("height"));
    Some(ibis_doc_model::Watermark { text, font, color, semitransparent, rotation, size })
}

#[cfg(test)]
mod vml_tests {
    use super::*;

    #[test]
    fn lengths_opacities_and_paths() {
        assert_eq!(vml_length("54pt"), Some(54 * 12_700));
        assert_eq!(vml_length("1in"), Some(914_400));
        assert_eq!(vml_length("2cm"), Some(720_000));
        assert_eq!(vml_length("10px"), Some(95_250));
        assert_eq!(vml_length("0"), Some(0));
        assert_eq!(vml_length("12em"), None);
        assert_eq!(vml_opacity(".5"), 128);
        assert_eq!(vml_opacity("32768f"), 128);
        assert_eq!(vml_opacity("1"), 255);
        let path = vml_path("m0,21600l10800,0,21600,21600xe");
        assert_eq!(path.len(), 4);
        assert!(matches!(path[0], VmlPath::Move(0, 21600)) && matches!(path[2], VmlPath::Line(21600, 21600)) && matches!(path[3], VmlPath::Close));
        let curve = vml_path("m0,10800c5400,0,16200,21600,21600,10800e");
        assert!(matches!(curve[1], VmlPath::Curve([5400, 0, 16200, 21600, 21600, 10800])));
        assert!(matches!(vml_custom_geometry(21600, 21600, &path, true), ibis_doc_model::Geometry::Formula(_)));
    }

    #[test]
    fn a_vml_style_is_read_by_property() {
        let style = vml_style("width:54pt;height:30pt;rotation:30");
        assert_eq!(style.get("rotation").map(String::as_str), Some("30"));
        assert_eq!(style.get("width").and_then(|w| vml_length(w)), Some(54 * 12_700));
    }
}
