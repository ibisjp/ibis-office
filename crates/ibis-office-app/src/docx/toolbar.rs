// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A document's toolbar and menus (the mock's ⑤-1 「ドキュメント：本文を
//! 編集中」): what each button shows and the editing method it calls
//! (`editing.rs`).

use gpui_kit::*;
use ibis_doc_model::{FlowAlign, Rgb, StyleKind};

use crate::shell::chrome::{Handler, MenuEntry, MenuId, MenuItem, Menus, Submenu, ToolButton, ToolItem};
use crate::shell::editor::SlideEditor;

fn sep() -> ToolItem {
    ToolItem::Separator
}

/// The styles offered, as Google's list names them, and the names Word
/// gives them in `styles.xml`.
const STYLES: &[(&str, &str, &str)] = &[
    ("docx.style.normal", "_shell.normal", "Normal"),
    ("docx.style.title", "_shell.title", "Title"),
    ("docx.style.subtitle", "docx.toolbar.subtitle", "Subtitle"),
    ("docx.style.heading1", "docx.toolbar.heading-1", "heading 1"),
    ("docx.style.heading2", "docx.toolbar.heading-2", "heading 2"),
    ("docx.style.heading3", "docx.toolbar.heading-3", "heading 3"),
];

/// Text colours offered (Word's standard colours).
pub(super) const COLORS: &[(&str, &str, u32)] = &[
    ("docx.color.black", "docx.toolbar.automatic-black", 0x000000),
    ("docx.color.dark-red", "_shell.dark-red", 0xc00000),
    ("docx.color.red", "_shell.red", 0xff0000),
    ("docx.color.orange", "_shell.orange", 0xffc000),
    ("docx.color.yellow", "_shell.yellow", 0xffff00),
    ("docx.color.light-green", "_shell.light-green", 0x92d050),
    ("docx.color.green", "_shell.green", 0x00b050),
    ("docx.color.light-blue", "_shell.light-blue", 0x00b0f0),
    ("docx.color.blue", "_shell.blue", 0x0070c0),
    ("docx.color.dark-blue", "_shell.dark-blue", 0x002060),
    ("docx.color.purple", "_shell.purple", 0x7030a0),
];

/// Office theme base 10 colors (background 1, text 1, background 2, text 2, accents 1–6).
pub(super) const THEME_10: [Rgb; 10] = [
    Rgb(0xffffff),
    Rgb(0x000000),
    Rgb(0xe7e6e6),
    Rgb(0x44546a),
    Rgb(0x4472c4),
    Rgb(0xed7d31),
    Rgb(0xa5a5a5),
    Rgb(0xffc000),
    Rgb(0x5b9bd5),
    Rgb(0x70ad47),
];

/// Office theme colors: (id, label, RGB, theme_color_name, theme_tint, theme_shade).
pub(super) const THEME_COLORS: &[(&str, &str, u32, &str, Option<&str>, Option<&str>)] = &[
    ("docx.theme-color.text1", "docx.toolbar.text-1-black", 0x000000, "text1", None, None),
    ("docx.theme-color.bg1", "docx.toolbar.background-1-white", 0xffffff, "background1", None, None),
    ("docx.theme-color.text2", "docx.toolbar.text-2-dark-blue-gray", 0x44546a, "text2", None, None),
    ("docx.theme-color.bg2", "docx.toolbar.background-2-light-gray", 0xe7e6e6, "background2", None, None),
    ("docx.theme-color.accent1", "_docx.blue-accent-1", 0x4472c4, "accent1", None, None),
    ("docx.theme-color.accent2", "docx.toolbar.accent-2-orange", 0xed7d31, "accent2", None, None),
    ("docx.theme-color.accent3", "docx.toolbar.accent-3-gray", 0xa5a5a5, "accent3", None, None),
    ("docx.theme-color.accent4", "docx.toolbar.accent-4-yellow", 0xffc000, "accent4", None, None),
    ("docx.theme-color.accent5", "docx.toolbar.accent-5-light-blue", 0x5b9bd5, "accent5", None, None),
    ("docx.theme-color.accent6", "docx.toolbar.accent-6-green", 0x70ad47, "accent6", None, None),
    ("docx.theme-color.accent1-light", "docx.toolbar.accent-1-lighter-80-percent", 0xd9e1f2, "accent1", Some("33"), None),
    ("docx.theme-color.accent1-dark", "docx.toolbar.accent-1-darker-25-percent", 0x305496, "accent1", None, Some("bf")),
    ("docx.theme-color.accent2-light", "docx.toolbar.accent-2-lighter-80-percent", 0xfce4d6, "accent2", Some("33"), None),
    ("docx.theme-color.accent2-dark", "docx.toolbar.accent-2-darker-25-percent", 0xc65911, "accent2", None, Some("bf")),
    ("docx.theme-color.accent5-light", "docx.toolbar.accent-5-lighter-80-percent", 0xddebf7, "accent5", Some("33"), None),
    ("docx.theme-color.accent6-light", "docx.toolbar.accent-6-lighter-80-percent", 0xe2efda, "accent6", Some("33"), None),
];

/// All 16 modeled highlight colours.
pub(super) const HIGHLIGHTS: &[(&str, &str, u32)] = &[
    ("docx.highlight.yellow", "_shell.yellow", 0xffff00),
    ("docx.highlight.green", "docx.toolbar.bright-green", 0x00ff00),
    ("docx.highlight.cyan", "xlsx.sparkline-color-cyan", 0x00ffff),
    ("docx.highlight.pink", "docx.toolbar.pink", 0xff00ff),
    ("docx.highlight.blue", "_shell.blue", 0x0000ff),
    ("docx.highlight.red", "_shell.red", 0xff0000),
    ("docx.highlight.gray", "docx.toolbar.gray-25-percent", 0xd3d3d3),
    ("docx.highlight.dark-blue", "_shell.dark-blue", 0x00008b),
    ("docx.highlight.dark-cyan", "docx.toolbar.dark-teal", 0x008b8b),
    ("docx.highlight.dark-green", "docx.toolbar.dark-green", 0x006400),
    ("docx.highlight.dark-magenta", "docx.toolbar.dark-pink", 0x8b008b),
    ("docx.highlight.dark-red", "_shell.dark-red", 0x8b0000),
    ("docx.highlight.dark-yellow", "docx.toolbar.dark-yellow", 0x808000),
    ("docx.highlight.dark-gray", "docx.toolbar.gray-50-percent", 0xa9a9a9),
    ("docx.highlight.black", "_shell.black", 0x000000),
    ("docx.highlight.white", "_shell.white", 0xffffff),
];

/// Preset page colours (Word's ページの色).
pub(super) const PAGE_COLORS: &[(&str, &str, u32)] = &[
    ("docx.page.color.light-yellow", "_shell.light-yellow", 0xfff2cc),
    ("docx.page.color.light-blue", "_shell.light-blue", 0xddebf7),
    ("docx.page.color.light-green", "_shell.light-green", 0xe2efda),
    ("docx.page.color.light-gray", "docx.toolbar.light-gray", 0xf2f2f2),
    ("docx.page.color.light-orange", "_xlsx.light-orange", 0xfce4d6),
    ("docx.page.color.yellow", "_shell.yellow", 0xffff00),
    ("docx.page.color.green", "_shell.green", 0x00ff00),
    ("docx.page.color.cyan", "xlsx.sparkline-color-cyan", 0x00ffff),
    ("docx.page.color.blue", "_shell.blue", 0x0000ff),
    ("docx.page.color.red", "_shell.red", 0xff0000),
    ("docx.page.color.black", "_shell.black", 0x000000),
];

/// A style's name as the menus show it (見出し 1 for `heading 1`).
pub(crate) fn style_display_name(name: &str) -> String {
    if let Some((_, label, _)) = STYLES.iter().find(|(_, _, n)| n.eq_ignore_ascii_case(name)) {
        return crate::shell::i18n::text(label).to_string();
    }
    let lower = name.to_ascii_lowercase();
    if let Some(n) = lower.strip_prefix("heading ") {
        return crate::shell::i18n::format("docx.help-panels.heading-n", &[("n", &n)]);
    }
    match lower.as_str() {
        "quote" => crate::shell::i18n::text("docx.toolbar.quote").to_string(),
        "intense quote" => crate::shell::i18n::text("docx.toolbar.intense-quote").to_string(),
        "list paragraph" => crate::shell::i18n::text("docx.toolbar.list-paragraph").to_string(),
        "header" => crate::shell::i18n::text("_shell.header").to_string(),
        "footer" => crate::shell::i18n::text("_shell.footer").to_string(),
        "caption" => crate::shell::i18n::text("docx.help-panels.caption").to_string(),
        "no spacing" => crate::shell::i18n::text("docx.toolbar.no-spacing").to_string(),
        "toc heading" => crate::shell::i18n::text("docx.toolbar.toc-heading").to_string(),
        _ => match lower.strip_prefix("toc ") {
            Some(n) => crate::shell::i18n::format("docx.toolbar.toc-n", &[("n", &n)]),
            None => name.to_string(),
        },
    }
}

/// The menu ids of the document's other paragraph styles.
/// 挿入 > 日付と時刻's pictures (a field's `\@`), as Word's Japanese
/// dialog offers them.
const DATE_PICTURES: [&str; 8] = ["yyyy/MM/dd", "yyyy'年'M'月'd'日'", "yyyy'年'M'月'd'日'(aaa)", "ggge'年'M'月'd'日'", "yyyy-MM-dd", "H:mm", "H'時'mm'分'", "h:mm AM/PM"]; // i18n-ok（日付の書式記号（フィールドの書式になる））
const DATE_IDS: [&str; 8] = ["docx.date.0", "docx.date.1", "docx.date.2", "docx.date.3", "docx.date.4", "docx.date.5", "docx.date.6", "docx.date.7"];
/// Whether 日付と時刻 puts fields (自動的に更新する) rather than text.
static DATE_UPDATES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) const DOC_STYLE_IDS: [&str; 40] = [
    "docx.style.doc.1", "docx.style.doc.2", "docx.style.doc.3", "docx.style.doc.4", "docx.style.doc.5", "docx.style.doc.6", "docx.style.doc.7", "docx.style.doc.8", "docx.style.doc.9", "docx.style.doc.10",
    "docx.style.doc.11", "docx.style.doc.12", "docx.style.doc.13", "docx.style.doc.14", "docx.style.doc.15", "docx.style.doc.16", "docx.style.doc.17", "docx.style.doc.18", "docx.style.doc.19", "docx.style.doc.20",
    "docx.style.doc.21", "docx.style.doc.22", "docx.style.doc.23", "docx.style.doc.24", "docx.style.doc.25", "docx.style.doc.26", "docx.style.doc.27", "docx.style.doc.28", "docx.style.doc.29", "docx.style.doc.30",
    "docx.style.doc.31", "docx.style.doc.32", "docx.style.doc.33", "docx.style.doc.34", "docx.style.doc.35", "docx.style.doc.36", "docx.style.doc.37", "docx.style.doc.38", "docx.style.doc.39", "docx.style.doc.40",
];

/// The document's paragraph styles besides the six always offered, by
/// their shown names (見出し 4 before 見出し 10), at most 40: their ids and
/// labels, for the list's second part (`docx.style.doc.<n>`).
pub(crate) fn document_paragraph_styles(doc: &ibis_doc_model::FlowDocument) -> Vec<(String, String)> {
    let others = || doc.styles.styles.iter().filter(|(_, s)| s.kind == StyleKind::Paragraph && !STYLES.iter().any(|(_, _, n)| n.eq_ignore_ascii_case(&s.name)));
    // スタイル ギャラリーの表示順 (DOCX-STYLE-011): when the document marks styles for the gallery
    // (`w:qFormat`) only those are offered, by their `w:uiPriority` (99 without one).
    let marked = others().any(|(_, s)| s.quick);
    let mut out: Vec<(u32, String, String)> = others()
        .filter(|(_, s)| s.quick || !marked)
        .map(|(id, s)| (if marked { s.ui_priority.unwrap_or(99) } else { 0 }, id.clone(), style_display_name(&s.name)))
        .collect();
    let key = |label: &str| {
        let digits: String = label.chars().rev().take_while(char::is_ascii_digit).collect::<Vec<_>>().into_iter().rev().collect();
        (label[..label.len() - digits.len()].to_string(), digits.parse::<u32>().unwrap_or(0))
    };
    out.sort_by(|a, b| (a.0, key(&a.2)).cmp(&(b.0, key(&b.2))));
    let mut out: Vec<(String, String)> = out.into_iter().map(|(_, id, label)| (id, label)).collect();
    out.truncate(DOC_STYLE_IDS.len());
    out
}

/// The menu ids of the cross-references offered (one per caption).
const XREF_IDS: [&str; 50] = [
    "docx.xref.1", "docx.xref.2", "docx.xref.3", "docx.xref.4", "docx.xref.5", "docx.xref.6", "docx.xref.7", "docx.xref.8", "docx.xref.9", "docx.xref.10",
    "docx.xref.11", "docx.xref.12", "docx.xref.13", "docx.xref.14", "docx.xref.15", "docx.xref.16", "docx.xref.17", "docx.xref.18", "docx.xref.19", "docx.xref.20",
    "docx.xref.21", "docx.xref.22", "docx.xref.23", "docx.xref.24", "docx.xref.25", "docx.xref.26", "docx.xref.27", "docx.xref.28", "docx.xref.29", "docx.xref.30",
    "docx.xref.31", "docx.xref.32", "docx.xref.33", "docx.xref.34", "docx.xref.35", "docx.xref.36", "docx.xref.37", "docx.xref.38", "docx.xref.39", "docx.xref.40",
    "docx.xref.41", "docx.xref.42", "docx.xref.43", "docx.xref.44", "docx.xref.45", "docx.xref.46", "docx.xref.47", "docx.xref.48", "docx.xref.49", "docx.xref.50",
];

fn handler(f: impl Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) -> Handler {
    std::rc::Rc::new(f)
}

/// The paragraph style of the caret's paragraph, as the list names it.
fn style_label(editor: &SlideEditor) -> SharedString {
    let Some((start, _)) = editor.ordered_selection() else { return crate::shell::i18n::text("_shell.normal") };
    let Some(doc) = editor.flow() else { return crate::shell::i18n::text("_shell.normal") };
    let Some(p) = ibis_doc_model::flow_command::paragraph(doc, start.para) else { return crate::shell::i18n::text("_shell.normal") };
    let id = p.style.clone().or_else(|| doc.styles.default_paragraph.clone());
    let name = id.as_ref().and_then(|id| doc.styles.styles.get(id)).map(|s| s.name.clone()).unwrap_or_default();
    if name.is_empty() { crate::shell::i18n::text("_shell.normal") } else { style_display_name(&name).into() }
}

/// The fills offered for table cells.
const FILLS: &[(&str, &str, u32)] = &[
    ("docx.fill.light-blue", "_shell.light-blue", 0xdeeaf6),
    ("docx.fill.light-green", "_shell.light-green", 0xe2efd9),
    ("docx.fill.light-yellow", "_shell.light-yellow", 0xfff2cc),
    ("docx.fill.light-red", "docx.paragraph-decorations.light-red", 0xfbe4d5),
    ("docx.fill.gray", "_shell.gray", 0xd9d9d9),
];

pub(crate) const CELL_MARGIN_NORMAL: [ibis_doc_model::Emu; 4] = [0, 108 * 635, 0, 108 * 635];
pub(crate) const CELL_MARGIN_NONE: [ibis_doc_model::Emu; 4] = [0, 0, 0, 0];
pub(crate) const CELL_MARGIN_NARROW: [ibis_doc_model::Emu; 4] = [2 * 12700, 3 * 12700, 2 * 12700, 3 * 12700];
pub(crate) const CELL_MARGIN_WIDE: [ibis_doc_model::Emu; 4] = [4 * 12700, 8 * 12700, 4 * 12700, 8 * 12700];

pub(crate) const TABLE_INDENTS: &[(&str, &str, ibis_doc_model::Emu)] = &[
    ("docx.table.indent.0", "docx.toolbar.none-0-pt", 0),
    ("docx.table.indent.1", "docx.toolbar.1-character-10-5-pt", 133_350),
    ("docx.table.indent.2", "docx.toolbar.2-characters-21-pt", 266_700),
    ("docx.table.indent.3", "docx.toolbar.3-characters-31-5-pt", 400_050),
    ("docx.table.indent.4", "docx.toolbar.4-characters-42-pt", 533_400),
];

pub(crate) const TABLE_INDENT_NONE: ibis_doc_model::Emu = 0;
pub(crate) const TABLE_INDENT_5MM: ibis_doc_model::Emu = 5 * 36000;
pub(crate) const TABLE_INDENT_10MM: ibis_doc_model::Emu = 10 * 36000;
pub(crate) const TABLE_INDENT_15MM: ibis_doc_model::Emu = 15 * 36000;
pub(crate) const TABLE_INDENT_20MM: ibis_doc_model::Emu = 20 * 36000;

pub(crate) const TABLE_CELL_SPACINGS: &[(&str, &str, Option<ibis_doc_model::Emu>)] = &[
    ("docx.table.cell-spacing.none", "docx.toolbar.none-0-pt", None),
    ("docx.table.cell-spacing.1pt", "1 pt", Some(12_700)),
    ("docx.table.cell-spacing.2pt", "2 pt", Some(25_400)),
    ("docx.table.cell-spacing.3pt", "3 pt", Some(38_100)),
    ("docx.table.cell-spacing.4pt", "4 pt", Some(50_800)),
    ("docx.table.cell-spacing.6pt", "6 pt", Some(76_200)),
];

pub(crate) const TABLE_FLOAT_DISTANCES: &[(&str, &str, [ibis_doc_model::Emu; 4])] = &[
    ("docx.table.float-distance.none", "docx.toolbar.none-0-pt", [0, 0, 0, 0]),
    ("docx.table.float-distance.standard", "docx.toolbar.standard-left-right-9-pt", [180 * 635, 180 * 635, 0, 0]),
    ("docx.table.float-distance.medium", "docx.toolbar.medium-left-right-9-pt", [180 * 635, 180 * 635, 120 * 635, 120 * 635]),
    ("docx.table.float-distance.large", "docx.toolbar.large-left-right-18-pt", [360 * 635, 360 * 635, 240 * 635, 240 * 635]),
];

pub(crate) const PICTURE_WRAPS: &[(&str, &str, ibis_doc_model::PictureWrapType)] = &[
    ("docx.picture.wrap.inline", "docx.picture.wrap.inline", ibis_doc_model::PictureWrapType::Inline),
    ("docx.picture.wrap.square", "docx.picture.wrap.square", ibis_doc_model::PictureWrapType::Square),
    ("docx.picture.wrap.tight", "docx.picture.wrap.tight", ibis_doc_model::PictureWrapType::Tight),
    ("docx.picture.wrap.through", "docx.picture.wrap.through", ibis_doc_model::PictureWrapType::Through),
    ("docx.picture.wrap.top-and-bottom", "docx.picture.wrap.top-and-bottom", ibis_doc_model::PictureWrapType::TopAndBottom),
    ("docx.picture.wrap.behind-text", "docx.picture.wrap.behind-text", ibis_doc_model::PictureWrapType::BehindText),
    ("docx.picture.wrap.in-front-of-text", "docx.picture.wrap.in-front-of-text", ibis_doc_model::PictureWrapType::InFrontOfText),
];

pub(crate) const PICTURE_ALIGN_H: &[(&str, &str, &str)] = &[
    ("docx.picture.align-h.left", "docx.page-number.bottom-left", "left"),
    ("docx.picture.align-h.center", "_shell.center-2", "center"),
    ("docx.picture.align-h.right", "docx.page-number.bottom-right", "right"),
];

pub(crate) const PICTURE_ALIGN_V: &[(&str, &str, &str)] = &[
    ("docx.picture.align-v.top", "docx.picture.align-v.top", "top"),
    ("docx.picture.align-v.center", "xlsx.text-vertical.middle", "center"),
    ("docx.picture.align-v.bottom", "docx.picture.align-v.bottom", "bottom"),
];

pub(crate) const PICTURE_FLOAT_DISTANCES: &[(&str, &str, [ibis_doc_model::Emu; 4])] = &[
    ("docx.picture.distance.none", "docx.toolbar.none-0-pt", [0, 0, 0, 0]),
    ("docx.picture.distance.standard", "docx.toolbar.standard-left-right-9-pt", [114_300, 114_300, 0, 0]),
    ("docx.picture.distance.medium", "docx.toolbar.medium-left-right-9-pt", [114_300, 114_300, 76_200, 76_200]),
    ("docx.picture.distance.large", "docx.toolbar.large-left-right-18-pt", [228_600, 228_600, 152_400, 152_400]),
];

pub(crate) const PICTURE_ZORDERS: &[(&str, &str, super::picture_panel::ZOrderAction)] = &[
    ("docx.picture.zorder.front", "_shell.bring-to-front", super::picture_panel::ZOrderAction::BringToFront),
    ("docx.picture.zorder.forward", "_shell.bring-forward", super::picture_panel::ZOrderAction::BringForward),
    ("docx.picture.zorder.backward", "_shell.send-backward", super::picture_panel::ZOrderAction::SendBackward),
    ("docx.picture.zorder.back", "_shell.send-to-back", super::picture_panel::ZOrderAction::SendToBack),
];

#[allow(dead_code)]
pub(crate) const PICTURE_ALIGN_DISTRIBUTE: &[(&str, &str)] = &[
    ("docx.picture.align.left", "align.left"),
    ("docx.picture.align.center", "_shell.align-center"),
    ("docx.picture.align.right", "align.right"),
    ("docx.picture.align.top", "_shell.align-top"),
    ("docx.picture.align.middle", "docx.picture.align.middle"),
    ("docx.picture.align.bottom", "_shell.align-bottom"),
    ("docx.picture.distribute.horizontally", "docx.picture.distribute.horizontally"),
    ("docx.picture.distribute.vertically", "docx.picture.distribute.vertically"),
    ("docx.picture.align-to.margin", "docx.picture.align-to.margin"),
    ("docx.picture.align-to.page", "docx.picture.align-to.page"),
];

pub(crate) const PICTURE_RECOLORS: &[(&str, Option<super::picture_panel::PictureRecolor>)] = &[
    ("docx.picture.recolor.none", None),
    ("docx.picture.recolor.grayscale", Some(super::picture_panel::PictureRecolor::Grayscale)),
    ("docx.picture.recolor.sepia", Some(super::picture_panel::PictureRecolor::Sepia)),
    ("docx.picture.recolor.washout", Some(super::picture_panel::PictureRecolor::Washout)),
    ("docx.picture.recolor.bw50", Some(super::picture_panel::PictureRecolor::BlackAndWhite50)),
    ("docx.picture.recolor.blue", Some(super::picture_panel::PictureRecolor::Blue)),
    ("docx.picture.recolor.green", Some(super::picture_panel::PictureRecolor::Green)),
    ("docx.picture.recolor.red", Some(super::picture_panel::PictureRecolor::Red)),
    ("docx.picture.recolor.orange", Some(super::picture_panel::PictureRecolor::Orange)),
    ("docx.picture.recolor.purple", Some(super::picture_panel::PictureRecolor::Purple)),
];

pub(crate) const PICTURE_SATURATIONS: &[(&str, Option<i32>)] = &[
    ("docx.picture.saturation.0", Some(0)),
    ("docx.picture.saturation.50", Some(50_000)),
    ("docx.picture.saturation.100", None),
    ("docx.picture.saturation.200", Some(200_000)),
    ("docx.picture.saturation.300", Some(300_000)),
];

pub(crate) const PICTURE_TRANSPARENCIES: &[(&str, Option<i32>)] = &[
    ("docx.picture.transparency.0", None),
    ("docx.picture.transparency.15", Some(85_000)),
    ("docx.picture.transparency.30", Some(70_000)),
    ("docx.picture.transparency.50", Some(50_000)),
    ("docx.picture.transparency.65", Some(35_000)),
    ("docx.picture.transparency.80", Some(20_000)),
];

pub(crate) const PICTURE_SOFT_EDGES: &[(&str, Option<i64>)] = &[
    ("docx.picture.soft-edge.none", None),
    ("docx.picture.soft-edge.1pt", Some(12_700)),
    ("docx.picture.soft-edge.2pt5", Some(31_750)),
    ("docx.picture.soft-edge.5pt", Some(63_500)),
    ("docx.picture.soft-edge.10pt", Some(127_000)),
];

pub(crate) fn toolbar_right(editor: &SlideEditor) -> Vec<ToolItem> {
    let editing = editor.doc_state().is_some_and(|s| s.caret.is_some());
    let in_table = editor.table_at_caret().is_some();
    let current = editor.current_format();
    let style = current.as_ref().map(|(_, s)| s.clone());
    let doc = editor.flow().cloned();
    let caret_paragraph = editor.ordered_selection().and_then(|(s, _)| doc.as_deref().and_then(|d| ibis_doc_model::flow_command::paragraph(d, s.para)));
    let (shape_align, shape_list_kind) = editor
        .active_shape_text_target()
        .and_then(|(at, range, _)| {
            let p = editor.paragraph_of(at.para)?;
            let mut offset = 0;
            for run in &p.runs {
                if offset == at.offset
                    && let ibis_doc_model::FlowRunKind::Shape(shape) = &run.kind
                {
                    let body = match &shape.kind {
                        ibis_doc_model::ShapeKind::Auto(a) => a.text.as_ref(),
                        ibis_doc_model::ShapeKind::TextBox(b) => Some(b),
                        _ => None,
                    }?;
                    let idx = range.map(|(from, _)| *body.paragraphs_in(from, from).start()).unwrap_or(0);
                    let para = body.paragraphs.get(idx);
                    let ta = para.and_then(|pr| pr.align).unwrap_or(body.style.align);
                    let fa = match ta {
                        ibis_doc_model::TextAlign::Left => FlowAlign::Left,
                        ibis_doc_model::TextAlign::Center => FlowAlign::Center,
                        ibis_doc_model::TextAlign::Right => FlowAlign::Right,
                        ibis_doc_model::TextAlign::Justify => FlowAlign::Justify,
                        ibis_doc_model::TextAlign::Distributed => FlowAlign::Distribute,
                    };
                    let lk = para.and_then(|pr| pr.bullet.as_ref()).map(|b| b.auto.is_none());
                    return Some((Some(fa), lk));
                }
                offset += run.len();
            }
            None
        })
        .unwrap_or((None, None));
    let align = shape_align.or_else(|| caret_paragraph.map(|p| p.align));
    let contextual_spacing = caret_paragraph.map_or(false, |p| p.contextual_spacing);
    let list_kind = shape_list_kind.or_else(|| caret_paragraph.and_then(|p| p.list).and_then(|l| doc.as_deref()?.styles.numbering.level(l.num_id, 0).map(|x| x.format == "bullet")));

    let style_menu: Vec<MenuEntry> = STYLES
        .iter()
        .map(|&(item_id, label, name)| {
            let id = doc.as_deref().and_then(|d| d.styles.paragraph_style_named(name).map(str::to_string));
            let id = id.or_else(|| doc.as_deref().and_then(|d| d.styles.styles.iter().find(|(_, s)| s.kind == StyleKind::Paragraph && s.name == name).map(|(i, _)| i.clone())));
            let label = crate::shell::i18n::text(label);
            let item = MenuItem::new(item_id, label.clone()).checked(style_label(editor) == label);
            match (id, name) {
                (Some(id), _) => item.on(move |this, _, cx| this.apply_style(Some(id.clone()), cx)).into(),
                (None, "Normal") => item.on(|this, _, cx| this.apply_style(None, cx)).into(),
                // A style the document does not have is not offered yet.
                _ => item.disabled(true).into(),
            }
        })
        .chain({
            // The document's own paragraph styles (見出し 4, 引用文, …).
            let current = style_label(editor);
            let others = doc.as_deref().map(document_paragraph_styles).unwrap_or_default();
            let mut entries: Vec<MenuEntry> = Vec::new();
            if !others.is_empty() {
                entries.push(MenuEntry::Separator);
            }
            for ((id, label), item_id) in others.into_iter().zip(DOC_STYLE_IDS) {
                let checked = current.as_ref() == label;
                entries.push(MenuItem::new(item_id, label).checked(checked).on(move |this, _, cx| this.apply_style(Some(id.clone()), cx)).into());
            }
            entries
        })
        // The document's character styles (fix: style_new.rs).
        .chain(super::style_new::char_style_entries(editor))
        .chain([
            MenuEntry::Separator,
            {
                let label = match editor.caret_style_name() {
                    Some(name) => crate::shell::i18n::format("docx.toolbar.update-name-match-selection", &[("name", &style_display_name(&name))]),
                    None => crate::shell::i18n::text("docx.toolbar.update-style-match-selection").to_string(),
                };
                MenuItem::new("docx.style.update", label).disabled(!editing).on(|this, _, cx| this.update_style_to_match(cx)).into()
            },
            MenuItem::tr("docx.style.modify").disabled(!editing).on(|this, window, cx| this.open_style_edit(window, cx)).into(),
            MenuItem::tr("docx.style.new").disabled(!editing).on(|this, window, cx| this.open_new_style_panel(window, cx)).into(),
        ])
        .collect();
    let font = editor.selection_font_label();
    let leading = vec![
        MenuItem::tr("docx.font.custom").on(|this, window, cx| this.open_font_edit(window, cx)).into(),
    ];
    let font_menu = crate::shell::font_list::build_font_menu_entries(
        "docx.font",
        leading,
        (!font.is_empty()).then_some(font.as_str()),
        |f, this, _, cx| {
            // A Japanese font is set for both scripts, as Word does;
            // a Latin one for Latin text only.
            let change = if f.is_ascii() && !crate::render::shape_view::is_east_asian_typeface(f) {
                ibis_doc_model::RunChange::Font(f.to_string())
            } else {
                ibis_doc_model::RunChange::AllFonts(f.to_string())
            };
            this.apply_run_change(change, cx);
        },
    );
    let cur_theme_color = current.as_ref().and_then(|(f, _)| f.theme_color.clone());
    let _theme_items: Vec<MenuEntry> = THEME_COLORS.iter().map(|&(item_id, label, rgb_val, theme_name, tint, shade)| {
        let is_checked = cur_theme_color.as_ref().is_some_and(|tc| {
            tc.name == theme_name && tc.tint.as_deref() == tint && tc.shade.as_deref() == shade
        });
        let tc = ibis_doc_model::FlowThemeColor::new(theme_name, tint, shade);
        MenuItem::new(item_id, crate::shell::i18n::text(label))
            .checked(is_checked)
            .on(move |this, _, cx| this.set_theme_color(Rgb(rgb_val), tc.clone(), cx))
            .into()
    }).collect();

    // 文字の色: a palette: the theme's colours (the colours themselves keep their theme name, the tints are plain
    // colours), the standard colours, "自動".
    let color_menu: Vec<MenuEntry> = {
        use crate::shell::color_palette::{Palette, PickColor, Swatch, THEME_TINTS};
        let pick: PickColor = std::rc::Rc::new(|this, c, _, cx| this.set_text_color(c.unwrap_or(Rgb(0)), cx));
        let current = style.as_ref().map(|s| s.color).filter(|_| cur_theme_color.is_none());
        // Word's order: background 1, text 1, background 2, text 2, accents 1 to 6.
        let order = ["docx.theme-color.bg1", "docx.theme-color.text1", "docx.theme-color.bg2", "docx.theme-color.text2", "docx.theme-color.accent1", "docx.theme-color.accent2", "docx.theme-color.accent3", "docx.theme-color.accent4", "docx.theme-color.accent5", "docx.theme-color.accent6"];
        let base: Vec<&(&str, &str, u32, &str, Option<&str>, Option<&str>)> = order.iter().filter_map(|id| THEME_COLORS.iter().find(|t| t.0 == *id)).collect();
        let mut theme: Vec<Vec<Swatch>> = Vec::new();
        theme.push(
            base.iter()
                .map(|&&(item_id, label, rgb_val, theme_name, tint, shade)| {
                    let checked = cur_theme_color.as_ref().is_some_and(|tc| tc.name == theme_name && tc.tint.as_deref() == tint && tc.shade.as_deref() == shade);
                    let tc = ibis_doc_model::FlowThemeColor::new(theme_name, tint, shade);
                    Swatch { rgb: Rgb(rgb_val), item: MenuItem::new(item_id, crate::shell::i18n::text(label)).checked(checked).on(move |this, _, cx| this.set_theme_color(Rgb(rgb_val), tc.clone(), cx)) }
                })
                .collect(),
        );
        if base.len() == 10 {
            for row in 1..6 {
                theme.push((0..10).map(|i| {
                    let rgb = ibis_pptx::tinted(Rgb(base[i].2), THEME_TINTS[i][row]);
                    Swatch::new(rgb, format!("#{:06X}", rgb.0), current == Some(rgb), &pick)
                }).collect());
            }
        }
        let standard: Vec<Swatch> = COLORS[1..].iter().map(|&(item_id, name, c)| Swatch::with_id(item_id, Rgb(c), crate::shell::i18n::text(name), current == Some(Rgb(c)), &pick)).collect();
        let none = MenuItem::new("docx.color.black", crate::shell::i18n::text("docx.toolbar.automatic-black")).icon("format_color_reset").checked(current == Some(Rgb(0))).on(|this, _, cx| this.set_text_color(Rgb(0), cx));
        let more = MenuItem::tr("docx.color.custom").on(|this, window, cx| this.open_color_edit(false, window, cx));
        vec![MenuEntry::Palette(Palette { none: Some(none), theme, standard, pick: Some(pick), more: Some(more), button: None, current:None })]
    };
    // 蛍光ペンの色: Word's sixteen, and "色なし".
    let highlight_menu: Vec<MenuEntry> = {
        let pick: crate::shell::color_palette::PickColor = std::rc::Rc::new(|this, c, _, cx| this.set_highlight(c, cx));
        let current = style.as_ref().and_then(|s| s.highlight);
        let list: Vec<(SharedString, Rgb, &'static str)> = HIGHLIGHTS.iter().map(|&(item_id, name, c)| (SharedString::from(crate::shell::i18n::text(name)), Rgb(c), item_id)).collect();
        let none = MenuItem::tr("docx.highlight-none").icon("format_color_reset").checked(style.as_ref().is_some_and(|s| s.highlight.is_none())).on(|this, _, cx| this.set_highlight(None, cx));
        crate::shell::color_palette::simple_palette(Some(none), None, &list, current, pick)
    };
    let align_item = |id: &'static str, icon: &'static str, label: &'static str, a: FlowAlign| -> MenuEntry {
        MenuItem::new(id, crate::shell::i18n::text(label)).icon(icon).checked(align == Some(a)).on(move |this, _, cx| this.set_alignment(a, cx)).into()
    };
    let spacing = |id: &'static str, label: &'static str, m: f32| -> MenuEntry { MenuItem::new(id, crate::shell::i18n::text(label)).on(move |this, _, cx| this.set_line_spacing(m, cx)).into() };
    let _color = style.as_ref().map_or(0, |s| s.color.0);
    let current_text_align = align.map(|a| match a {
        FlowAlign::Left => ibis_doc_model::TextAlign::Left,
        FlowAlign::Center => ibis_doc_model::TextAlign::Center,
        FlowAlign::Right => ibis_doc_model::TextAlign::Right,
        FlowAlign::Justify => ibis_doc_model::TextAlign::Justify,
        FlowAlign::Distribute => ibis_doc_model::TextAlign::Distributed,
    });
    let [align_l, align_c, align_r, align_drop] = crate::shell::chrome::align_h_toolbar_items(
        current_text_align,
        editing,
        handler(|this, _, cx| this.set_alignment(FlowAlign::Left, cx)),
        handler(|this, _, cx| this.set_alignment(FlowAlign::Center, cx)),
        handler(|this, _, cx| this.set_alignment(FlowAlign::Right, cx)),
        vec![
            align_item("align.left", "format_align_left", "docx.toolbar.align-left-l", FlowAlign::Left),
            align_item("align.center", "format_align_center", "docx.toolbar.center-e", FlowAlign::Center),
            align_item("align.right", "format_align_right", "docx.toolbar.align-right-r", FlowAlign::Right),
            align_item("align.justify", "format_align_justify", "docx.toolbar.justify-j", FlowAlign::Justify),
            align_item("align.distribute", "format_align_justify", "_docx.distribute-j", FlowAlign::Distribute),
        ],
    );
    let shape_anchor = editor.caret_shape_anchor();
    let current_valign = shape_anchor.or_else(|| {
        in_table.then(|| match editor.caret_cell_anchor().unwrap_or(ibis_doc_model::CellAnchor::Top) {
            ibis_doc_model::CellAnchor::Top => ibis_doc_model::VerticalAnchor::Top,
            ibis_doc_model::CellAnchor::Center => ibis_doc_model::VerticalAnchor::Middle,
            ibis_doc_model::CellAnchor::Bottom => ibis_doc_model::VerticalAnchor::Bottom,
        })
    });
    let valign_enabled = editing && (shape_anchor.is_some() || in_table);
    let set_valign = |a: ibis_doc_model::VerticalAnchor| {
        move |this: &mut SlideEditor, _: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<SlideEditor>| {
            if this.caret_shape_anchor().is_some() {
                this.set_shape_anchor(a, cx);
            } else if this.table_at_caret().is_some() {
                let cell_anchor = match a {
                    ibis_doc_model::VerticalAnchor::Top => ibis_doc_model::CellAnchor::Top,
                    ibis_doc_model::VerticalAnchor::Middle => ibis_doc_model::CellAnchor::Center,
                    ibis_doc_model::VerticalAnchor::Bottom => ibis_doc_model::CellAnchor::Bottom,
                };
                if this.caret_cell_anchor() == Some(cell_anchor) {
                    return;
                }
                this.edit_table(move |r, c, _| ibis_doc_model::TableEdit::Anchor { cells: vec![(r, c)], anchor: cell_anchor }, cx);
            }
        }
    };
    let valign_item = |id: &'static str, icon: &'static str, label: &'static str, a: ibis_doc_model::VerticalAnchor| -> MenuEntry {
        MenuItem::new(id, crate::shell::i18n::text(label))
            .icon(icon)
            .checked(current_valign == Some(a))
            .on(set_valign(a))
            .into()
    };
    let [valign_t, valign_m, valign_b, valign_drop] = crate::shell::chrome::align_v_toolbar_items(
        current_valign,
        valign_enabled,
        handler(set_valign(ibis_doc_model::VerticalAnchor::Top)),
        handler(set_valign(ibis_doc_model::VerticalAnchor::Middle)),
        handler(set_valign(ibis_doc_model::VerticalAnchor::Bottom)),
        vec![
            valign_item("valign.top", "vertical_align_top", "docx.cell.valign-top", ibis_doc_model::VerticalAnchor::Top),
            valign_item("valign.middle", "vertical_align_center", "docx.cell.valign-center", ibis_doc_model::VerticalAnchor::Middle),
            valign_item("valign.bottom", "vertical_align_bottom", "docx.cell.valign-bottom", ibis_doc_model::VerticalAnchor::Bottom),
        ],
    );
    // Bullets and numbering are split buttons as Word's: the left half toggles the list in the bullet (the number format)
    // last chosen, the ▼ has the library. `bullet-types` is the same menu for the Format menu (not drawn).
    let bullets = ToolButton::new("bullets", "format_list_bulleted", crate::shell::i18n::text("_shell.bullets"))
        .on(list_kind == Some(true))
        .disabled(!editing)
        .click(|this, _, cx| this.toggle_list(true, cx))
        .menu(
            super::list_formats::BULLET_LIBRARY.iter().enumerate().map(|(i, &(id, label, _, _))| {
                let current = editor.caret_bullet_format();
                let now = current.as_ref().is_some_and(|(t, _, _)| t == label);
                MenuItem::tr(id).checked(now).on(move |this, _, cx| this.apply_bullet_format(i, cx)).into()
            }).collect()
        )
        .last_used(editor, "bullets", |_| true, |_| false, false)
        .quick_off(list_kind == Some(true), |this, _, cx| this.toggle_list(true, cx));
    let bullet_types = ToolButton::text("bullet-types", crate::shell::i18n::text("pptx.index-fields.bullets"), crate::shell::i18n::text("pptx.chrome.bullet-symbol")).disabled(!editing).menu(bullets.menu.clone().unwrap_or_default()).hidden();
    let numbered = ToolButton::new("numbered", "format_list_numbered", crate::shell::i18n::text("_shell.numbering"))
        .on(list_kind == Some(false))
        .disabled(!editing)
        .click(|this, _, cx| this.toggle_list(false, cx))
        .menu({
            let current = editor.caret_list_format();
            super::list_formats::LIBRARY.iter().enumerate().map(|(i, &(id, label, format, text))| {
                let on = current.as_ref().is_some_and(|(f, t, _)| f == format && t == text);
                MenuItem::new(id, label).checked(on).on(move |this, _, cx| this.apply_list_format(i, cx)).into()
            }).collect()
        })
        .last_used(editor, "numbered", |_| true, |_| false, false)
        .quick_off(list_kind == Some(false), |this, _, cx| this.toggle_list(false, cx));
    vec![
        sep(),
        ToolItem::Select { id: "style", value: style_label(editor), width: 112.0, tooltip: crate::shell::i18n::text("_shell.styles"), menu: style_menu, disabled: !editing },
        sep(),
        ToolItem::Select { id: "font", value: font.into(), width: 128.0, tooltip: crate::shell::i18n::text("_shell.font"), menu: font_menu, disabled: !editing },
        sep(),
        ToolItem::FontSize {
            value: editor.selection_font_size_label(),
            smaller: editing.then(|| handler(|this, _, cx| this.step_text_size(false, cx))),
            larger: editing.then(|| handler(|this, _, cx| this.step_text_size(true, cx))),
        },
        sep(),
        ToolButton::new("bold", "format_bold", crate::shell::i18n::text("_shell.bold-b")).on(style.as_ref().is_some_and(|s| s.bold)).disabled(!editing).click(|this, _, cx| this.toggle_bold(cx)).into(),
        ToolButton::new("italic", "format_italic", crate::shell::i18n::text("_shell.italic-i")).on(style.as_ref().is_some_and(|s| s.italic)).disabled(!editing).click(|this, _, cx| this.toggle_italic(cx)).into(),
        // A split button as Word's: the left half takes the underline off, or underlines in the kind last chosen (single at first).
        ToolButton::new("underline", "format_underlined", crate::shell::i18n::text("_shell.underline-u")).on(style.as_ref().is_some_and(|s| s.underline)).disabled(!editing).click(|this, _, cx| this.toggle_underline(cx))
            .menu(underline_type_items(style.as_ref().map(|s| (s.underline, s.underline_style)), editing))
            .last_used(editor, "underline", |i| i.id != "docx.underline.none", |i| i.id == "docx.underline.single", false)
            .quick_off(style.as_ref().is_some_and(|s| s.underline), |this, _, cx| this.toggle_underline(cx))
            .into(),
        ToolButton::new("text-color", "format_color_text", crate::shell::i18n::text("_shell.font-color")).disabled(!editing).palette(editor, "text-color", 0xff0000, color_menu).into(),
        ToolButton::new("highlight", "ink_highlighter", crate::shell::i18n::text("docx.toolbar.highlight-color")).disabled(!editing).palette(editor, "highlight", 0xffff00, highlight_menu).into(),
        sep(),
        ToolButton::new("link", "link", crate::shell::i18n::text("docx.toolbar.insert-link-k")).on(editor.link_at_selection().is_some()).disabled(!editing).click(|this, window, cx| this.open_link(window, cx)).into(),
        ToolButton::new("image", "image", crate::shell::i18n::text("_shell.insert-picture")).disabled(!editing).click(|this, window, cx| this.doc_pick_picture(window, cx)).into(),
        ToolButton::new("equation", "functions", crate::shell::i18n::text("insert.equation")).disabled(!editing).click(|this, window, cx| this.open_docx_math_panel(window, cx)).into(),
        sep(),
        align_l,
        align_c,
        align_r,
        align_drop,
        valign_t,
        valign_m,
        valign_b,
        valign_drop,
        ToolButton::new("line-spacing", "format_line_spacing", crate::shell::i18n::text("_shell.line-paragraph-spacing")).disabled(!editing).menu(vec![spacing("docx.spacing.1", "1", 1.0), spacing("docx.spacing.1.15", "1.15", 1.15), spacing("docx.spacing.1.5", "1.5", 1.5), spacing("docx.spacing.2", "2", 2.0), MenuEntry::Separator,
            MenuItem::tr("docx.paragraph.details").on(|this, window, cx| this.open_paragraph_edit(None, window, cx)).into(),
            MenuItem::tr("docx.spacing.before-add").on(|this, _, cx| this.apply_para_change(ibis_doc_model::ParaChange::SpaceBefore(12 * 12_700), cx)).into(),
            MenuItem::tr("docx.spacing.after-remove").on(|this, _, cx| this.apply_para_change(ibis_doc_model::ParaChange::SpaceAfter(0), cx)).into(),
            MenuItem::new("docx.spacing.contextual", crate::shell::i18n::text("fix.toolbar.don-t-add-space-between")).checked(contextual_spacing).on(move |this, _, cx| this.apply_para_change(ibis_doc_model::ParaChange::ContextualSpacing(!contextual_spacing), cx)).into()]).into(),
        ToolButton::new("checklist", "checklist", crate::shell::i18n::text("docx.toolbar.checklist"))
            .on(editor.shape_has_checklist())
            .disabled(!editing && editor.active_shape_text_target().is_none())
            .click(|this, _, cx| this.toggle_checklist(cx))
            .into(),
        bullets.into(),
        bullet_types.into(),
        numbered.into(),
        ToolButton::new("indent-less", "format_indent_decrease", crate::shell::i18n::text("_shell.decrease-indent")).disabled(!editing).click(|this, _, cx| this.step_indent(false, cx)).into(),
        ToolButton::new("indent-more", "format_indent_increase", crate::shell::i18n::text("_shell.increase-indent")).disabled(!editing).click(|this, _, cx| this.step_indent(true, cx)).into(),
        ToolButton::new("clear-format", "format_clear", crate::shell::i18n::text("pptx.chrome.clear-all-formatting")).disabled(!editing).click(|this, _, cx| this.clear_formatting(cx)).into(),
        ToolButton::text("formatting-marks", "¶", crate::shell::i18n::text("docx.toolbar.show-formatting-marks-8")).on(editor.formatting_marks_open).click(|this, _, cx| this.toggle_formatting_marks(cx)).into(),
    ]
    .into_iter()
    .chain({
        let shape_active = editor.doc_state().is_some_and(|s| s.shape_selected || s.box_edit.is_some());
        shape_active.then(|| editor.text_box_at_caret()).flatten().map(|info| {
            let is_line = info.is_line;
            let cur_fill = (info.fill.len() == 6).then(|| u32::from_str_radix(&info.fill, 16).ok()).flatten().map(Rgb);
            let cur_line = (info.line.len() == 6).then(|| u32::from_str_radix(&info.line, 16).ok()).flatten().map(Rgb);
            let std_colors: Vec<(SharedString, Rgb, &'static str)> = COLORS[1..]
                .iter()
                .map(|&(id, name, c)| (crate::shell::i18n::text(name), Rgb(c), id))
                .collect();
            let fill_pick: crate::shell::color_palette::PickColor = std::rc::Rc::new(move |this, chosen, window, cx| {
                if let Some(c) = chosen {
                    this.note_recent_color(c);
                }
                this.apply_selected_shapes_fill(chosen, window, cx);
            });
            let fill_none = MenuItem::new("shape.fill.none", "_shell.no-fill")
                .checked(cur_fill.is_none())
                .on(move |this, window, cx| {
                    this.apply_selected_shapes_fill(None, window, cx);
                });
            let fill_menu = crate::shell::color_palette::simple_palette(Some(fill_none), Some(&THEME_10), &std_colors, cur_fill, fill_pick);

            let line_pick: crate::shell::color_palette::PickColor = std::rc::Rc::new(move |this, chosen, window, cx| {
                if let Some(c) = chosen {
                    this.note_recent_color(c);
                }
                this.apply_selected_shapes_line_color(chosen, window, cx);
            });
            let line_none = MenuItem::new("shape.line.none", "_shell.no-outline")
                .checked(cur_line.is_none())
                .on(move |this, window, cx| {
                    this.apply_selected_shapes_line_color(None, window, cx);
                });
            let line_menu = crate::shell::color_palette::simple_palette(Some(line_none), Some(&THEME_10), &std_colors, cur_line, line_pick);
            let mut items = vec![sep()];
            if !is_line {
                items.push(
                    ToolButton::new("shape-fill-color", "format_color_fill", crate::shell::i18n::text("docx.text-box-panel.fill-color"))
                        .palette(editor, "shape-fill-color", cur_fill.map_or(0x4472c4, |c| c.0), fill_menu)
                        .into(),
                );
            }
            items.push(
                ToolButton::new("shape-line-color", "border_color", crate::shell::i18n::text("docx.text-box-panel.outline-color"))
                    .palette(editor, "shape-line-color", cur_line.map_or(0x2f528f, |c| c.0), line_menu)
                    .into(),
            );
            items
        }).into_iter().flatten()
    })
    .chain([
        sep(),
        {
            let mode = editor.interaction_mode();
            let (icon, label) = match mode {
                ibis_doc_model::DocumentInteractionMode::Editing => ("edit", "docx.interaction.editing"),
                ibis_doc_model::DocumentInteractionMode::Reviewing => ("rate_review", "docx.interaction.reviewing"),
                ibis_doc_model::DocumentInteractionMode::Viewing => ("visibility", "docx.interaction.viewing"),
            };
            let items: Vec<MenuEntry> = vec![
                MenuItem::tr("docx.interaction.editing")
                    .icon("edit")
                    .checked(mode == ibis_doc_model::DocumentInteractionMode::Editing)
                    .disabled(editor.is_track_changes_locked())
                    .on(|this, window, cx| this.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Editing, window, cx))
                    .into(),
                MenuItem::tr("docx.interaction.reviewing")
                    .icon("rate_review")
                    .checked(mode == ibis_doc_model::DocumentInteractionMode::Reviewing)
                    .on(|this, window, cx| this.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Reviewing, window, cx))
                    .into(),
                MenuItem::tr("docx.interaction.viewing")
                    .icon("visibility")
                    .checked(mode == ibis_doc_model::DocumentInteractionMode::Viewing)
                    .on(|this, window, cx| this.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Viewing, window, cx))
                    .into(),
            ];
            ToolButton::new("interaction-mode", icon, crate::shell::i18n::text(label)).menu(items).into()
        },
    ])
    .chain(in_table.then(|| {
        // In a table (the mock's ⑤-3): the cell's fill and vertical alignment.
        let fill_pick: crate::shell::color_palette::PickColor = std::rc::Rc::new(|this, chosen, _, cx| {
            this.edit_table(move |r, c2, _| ibis_doc_model::TableEdit::Shading { cells: vec![(r, c2)], color: chosen }, cx);
        });
        let fill_swatches: Vec<(SharedString, Rgb, &'static str)> = FILLS
            .iter()
            .chain(COLORS[1..].iter())
            .map(|&(id, name, c)| (crate::shell::i18n::text(name), Rgb(c), id))
            .collect();
        let fill_none = MenuItem::tr("docx.fill.none")
            .icon("format_color_reset")
            .on(|this, _, cx| this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Shading { cells: vec![(r, c)], color: None }, cx));
        let fills = crate::shell::color_palette::simple_palette(Some(fill_none), Some(&THEME_10), &fill_swatches, None, fill_pick);
        let anchor = editor.caret_cell_anchor().unwrap_or(ibis_doc_model::CellAnchor::Top);
        let valigns = vec![
            MenuItem::tr("docx.cell.valign-top")
                .icon("vertical_align_top")
                .checked(anchor == ibis_doc_model::CellAnchor::Top)
                .on(|this, _, cx| {
                    if this.caret_cell_anchor() == Some(ibis_doc_model::CellAnchor::Top) { return; }
                    this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Anchor { cells: vec![(r, c)], anchor: ibis_doc_model::CellAnchor::Top }, cx);
                })
                .into(),
            MenuItem::tr("docx.cell.valign-center")
                .icon("vertical_align_center")
                .checked(anchor == ibis_doc_model::CellAnchor::Center)
                .on(|this, _, cx| {
                    if this.caret_cell_anchor() == Some(ibis_doc_model::CellAnchor::Center) { return; }
                    this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Anchor { cells: vec![(r, c)], anchor: ibis_doc_model::CellAnchor::Center }, cx);
                })
                .into(),
            MenuItem::tr("docx.cell.valign-bottom")
                .icon("vertical_align_bottom")
                .checked(anchor == ibis_doc_model::CellAnchor::Bottom)
                .on(|this, _, cx| {
                    if this.caret_cell_anchor() == Some(ibis_doc_model::CellAnchor::Bottom) { return; }
                    this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Anchor { cells: vec![(r, c)], anchor: ibis_doc_model::CellAnchor::Bottom }, cx);
                })
                .into(),
        ];
        let direction = editor.caret_cell_direction().unwrap_or(ibis_doc_model::CellDirection::LrTb);
        let directions = vec![
            MenuItem::tr("docx.cell.direction.lrtb")
                .checked(direction == ibis_doc_model::CellDirection::LrTb)
                .on(|this, _, cx| {
                    if this.caret_cell_direction() == Some(ibis_doc_model::CellDirection::LrTb) { return; }
                    this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Direction { cells: vec![(r, c)], direction: ibis_doc_model::CellDirection::LrTb }, cx);
                })
                .into(),
            MenuItem::tr("docx.cell.direction.tbrl")
                .checked(direction == ibis_doc_model::CellDirection::TbRl)
                .on(|this, _, cx| {
                    if this.caret_cell_direction() == Some(ibis_doc_model::CellDirection::TbRl) { return; }
                    this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Direction { cells: vec![(r, c)], direction: ibis_doc_model::CellDirection::TbRl }, cx);
                })
                .into(),
            MenuItem::tr("docx.cell.direction.tblrv")
                .checked(direction == ibis_doc_model::CellDirection::TbLrV)
                .on(|this, _, cx| {
                    if this.caret_cell_direction() == Some(ibis_doc_model::CellDirection::TbLrV) { return; }
                    this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Direction { cells: vec![(r, c)], direction: ibis_doc_model::CellDirection::TbLrV }, cx);
                })
                .into(),
            MenuItem::tr("docx.cell.direction.btlr")
                .checked(direction == ibis_doc_model::CellDirection::BtLr)
                .on(|this, _, cx| {
                    if this.caret_cell_direction() == Some(ibis_doc_model::CellDirection::BtLr) { return; }
                    this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Direction { cells: vec![(r, c)], direction: ibis_doc_model::CellDirection::BtLr }, cx);
                })
                .into(),
        ];
        let margins = editor.caret_cell_margins().unwrap_or(CELL_MARGIN_NORMAL);
        let margin_items = vec![
            MenuItem::tr("docx.cell.margins.normal")
                .checked(margins == CELL_MARGIN_NORMAL)
                .on(|this, _, cx| {
                    if this.caret_cell_margins() == Some(CELL_MARGIN_NORMAL) { return; }
                    this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Margins { cells: vec![(r, c)], margins: CELL_MARGIN_NORMAL }, cx);
                })
                .into(),
            MenuItem::tr("docx.cell.margins.none")
                .checked(margins == CELL_MARGIN_NONE)
                .on(|this, _, cx| {
                    if this.caret_cell_margins() == Some(CELL_MARGIN_NONE) { return; }
                    this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Margins { cells: vec![(r, c)], margins: CELL_MARGIN_NONE }, cx);
                })
                .into(),
            MenuItem::tr("docx.cell.margins.narrow")
                .checked(margins == CELL_MARGIN_NARROW)
                .on(|this, _, cx| {
                    if this.caret_cell_margins() == Some(CELL_MARGIN_NARROW) { return; }
                    this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Margins { cells: vec![(r, c)], margins: CELL_MARGIN_NARROW }, cx);
                })
                .into(),
            MenuItem::tr("docx.cell.margins.wide")
                .checked(margins == CELL_MARGIN_WIDE)
                .on(|this, _, cx| {
                    if this.caret_cell_margins() == Some(CELL_MARGIN_WIDE) { return; }
                    this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Margins { cells: vec![(r, c)], margins: CELL_MARGIN_WIDE }, cx);
                })
                .into(),
        ];
        let table_align = editor.caret_table_align().unwrap_or(ibis_doc_model::FlowAlign::Left);
        let table_align_items = vec![
            MenuItem::tr("docx.table.align.left")
                .icon("format_align_left")
                .checked(table_align == ibis_doc_model::FlowAlign::Left)
                .on(|this, _, cx| {
                    if this.caret_table_align() == Some(ibis_doc_model::FlowAlign::Left) { return; }
                    this.edit_table(|_, _, _| ibis_doc_model::TableEdit::Align(ibis_doc_model::FlowAlign::Left), cx);
                })
                .into(),
            MenuItem::tr("docx.table.align.center")
                .icon("format_align_center")
                .checked(table_align == ibis_doc_model::FlowAlign::Center)
                .on(|this, _, cx| {
                    if this.caret_table_align() == Some(ibis_doc_model::FlowAlign::Center) { return; }
                    this.edit_table(|_, _, _| ibis_doc_model::TableEdit::Align(ibis_doc_model::FlowAlign::Center), cx);
                })
                .into(),
            MenuItem::tr("docx.table.align.right")
                .icon("format_align_right")
                .checked(table_align == ibis_doc_model::FlowAlign::Right)
                .on(|this, _, cx| {
                    if this.caret_table_align() == Some(ibis_doc_model::FlowAlign::Right) { return; }
                    this.edit_table(|_, _, _| ibis_doc_model::TableEdit::Align(ibis_doc_model::FlowAlign::Right), cx);
                })
                .into(),
        ];
        let table_indent = editor.caret_table_indent().unwrap_or(0);
        let table_indent_items: Vec<MenuEntry> = TABLE_INDENTS.iter().map(|&(id, label, val)| {
            MenuItem::new(id, crate::shell::i18n::text(label))
                .checked(table_indent == val)
                .on(move |this, _, cx| {
                    if this.caret_table_indent() == Some(val) { return; }
                    this.edit_table(move |_, _, _| ibis_doc_model::TableEdit::Indent(val), cx);
                })
                .into()
        }).collect();
        let table_cell_spacing = editor.caret_table_cell_spacing().flatten();
        let table_spacing_items: Vec<MenuEntry> = TABLE_CELL_SPACINGS.iter().map(|&(id, label, val)| {
            MenuItem::new(id, crate::shell::i18n::text(label))
                .checked(table_cell_spacing == val)
                .on(move |this, _, cx| {
                    if this.caret_table_cell_spacing().flatten() == val { return; }
                    this.edit_table(move |_, _, _| ibis_doc_model::TableEdit::CellSpacing(val), cx);
                })
                .into()
        }).collect();
        let table_float = editor.caret_table_floating().flatten();
        let is_floating = table_float.is_some();
        let float_align = table_float.and_then(|f| f.x_align);
        let table_wrap_items: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.table.wrap.none")
                .checked(!is_floating)
                .on(|this, _, cx| this.set_table_floating(None, cx))
                .into(),
            MenuItem::tr("docx.table.wrap.around-left")
                .checked(is_floating && (float_align == Some(ibis_doc_model::TableAlign::Start) || float_align.is_none()))
                .on(|this, _, cx| this.set_table_float_horz_align(Some(ibis_doc_model::TableAlign::Start), cx))
                .into(),
            MenuItem::tr("docx.table.wrap.around-center")
                .checked(is_floating && float_align == Some(ibis_doc_model::TableAlign::Center))
                .on(|this, _, cx| this.set_table_float_horz_align(Some(ibis_doc_model::TableAlign::Center), cx))
                .into(),
            MenuItem::tr("docx.table.wrap.around-right")
                .checked(is_floating && float_align == Some(ibis_doc_model::TableAlign::End))
                .on(|this, _, cx| this.set_table_float_horz_align(Some(ibis_doc_model::TableAlign::End), cx))
                .into(),
        ];
        let sort_items: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.table.sort.asc").on(|this, _, cx| this.sort_table(true, true, cx)).into(),
            MenuItem::tr("docx.table.sort.desc").on(|this, _, cx| this.sort_table(false, true, cx)).into(),
            MenuItem::tr("docx.table.sort.asc-all").on(|this, _, cx| this.sort_table(true, false, cx)).into(),
            MenuItem::tr("docx.table.sort.desc-all").on(|this, _, cx| this.sort_table(false, false, cx)).into(),
        ];
        let cur_style_id = editor.caret_docx_table_style().flatten();
        let mut table_style_items: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.table.style.none")
                .checked(cur_style_id.is_none())
                .on(|this, _, cx| this.set_docx_table_style(None, cx))
                .into(),
        ];
        for (id, name) in editor.document_table_styles() {
            let id_clone = id.clone();
            let is_checked = cur_style_id.as_deref() == Some(&id);
            let menu_id: &'static str = match id.as_str() {
                "TableGrid" => "docx.table.style.table-grid",
                _ => "docx.table.style.item",
            };
            table_style_items.push(
                MenuItem::new(menu_id, crate::shell::i18n::text(&name))
                    .checked(is_checked)
                    .on(move |this, _, cx| this.set_docx_table_style(Some(id_clone.clone()), cx))
                    .into(),
            );
        }
        vec![
            sep(),
            ToolButton::new("table-style", "table_chart", crate::shell::i18n::text("docx.toolbar.table-style")).menu(table_style_items).into(),
            ToolButton::new("table-align", "format_align_left", crate::shell::i18n::text("docx.toolbar.table-alignment")).menu(table_align_items).into(),
            ToolButton::new("table-indent", "format_indent_increase", crate::shell::i18n::text("docx.toolbar.table-indent")).menu(table_indent_items).into(),
            ToolButton::new("table-spacing", "grid_view", crate::shell::i18n::text("docx.toolbar.cell-spacing")).menu(table_spacing_items).into(),
            ToolButton::new("table-wrap", "wrap_text", crate::shell::i18n::text("docx.toolbar.text-wrapping-table")).menu(table_wrap_items).into(),
            ToolButton::new("table-sort", "sort", crate::shell::i18n::text("_xlsx.sort-2")).menu(sort_items).into(),
            // A split button as Word's shading: the left half shades in the fill last chosen.
            ToolButton::new("cell-fill", "format_color_fill", crate::shell::i18n::text("docx.toolbar.shading-cell")).palette(editor, "cell-fill", 0xdeeaf6, fills).into(),
            ToolButton::new("cell-valign", "vertical_align_center", crate::shell::i18n::text("_shell.vertical-alignment")).menu(valigns).into(),
            ToolButton::new("cell-direction", "text_rotation_none", crate::shell::i18n::text("docx.chrome.text-direction")).menu(directions).into(),
            ToolButton::new("cell-margins", "crop_square", crate::shell::i18n::text("docx.toolbar.cell-margins")).menu(margin_items).into(),
            ToolButton::new("cell-wrap-text", "wrap_text", crate::shell::i18n::text("docx.toolbar.wrap-text-cell"))
                .on(!editor.caret_cell_no_wrap().unwrap_or(false))
                .click(|this, _, cx| this.toggle_cell_wrap_text(cx))
                .into(),
        ]
    }).into_iter().flatten())
    .chain(editor.picture_at_caret().is_some().then(|| {
        let cur_wrap = editor.picture_wrap_at_caret().unwrap_or(ibis_doc_model::PictureWrapType::Inline);
        let cur_anchor = editor.picture_floating_at_caret();
        let cur_dist = cur_anchor.as_ref().map(|a| [a.dist_l, a.dist_r, a.dist_t, a.dist_b]);
        let cur_align_h = cur_anchor.as_ref().and_then(|a| a.align_h.as_deref());
        let cur_align_v = cur_anchor.as_ref().and_then(|a| a.align_v.as_deref());
        let is_floating = cur_wrap != ibis_doc_model::PictureWrapType::Inline;

        let mut picture_wrap_items: Vec<MenuEntry> = PICTURE_WRAPS.iter().map(|&(id, label, wrap)| {
            MenuItem::new(id, crate::shell::i18n::text(label))
                .checked(cur_wrap == wrap)
                .on(move |this, _, cx| this.set_picture_wrap(wrap, cx))
                .into()
        }).collect();
        picture_wrap_items.push(MenuEntry::Separator);
        picture_wrap_items.push(
            MenuItem::tr("docx.picture.edit-wrap-points")
                .checked(editor.is_editing_wrap_points())
                .on(|this, _, cx| this.toggle_edit_wrap_points(cx))
                .into(),
        );
        picture_wrap_items.push(
            MenuItem::tr("docx.picture.move-with-text")
                .disabled(!is_floating)
                .checked(is_floating && cur_anchor.as_ref().is_some_and(|a| a.moves_with_text()))
                .on(|this, _, cx| this.set_picture_move_with_text(true, cx))
                .into(),
        );
        picture_wrap_items.push(
            MenuItem::tr("docx.picture.fix-position-on-page")
                .disabled(!is_floating)
                .checked(is_floating && cur_anchor.as_ref().is_some_and(|a| !a.moves_with_text()))
                .on(|this, _, cx| this.set_picture_move_with_text(false, cx))
                .into(),
        );

        let picture_align_items: Vec<MenuEntry> = PICTURE_ALIGN_H.iter().map(|&(id, label, align)| {
            MenuItem::new(id, crate::shell::i18n::text(label))
                .disabled(!is_floating)
                .checked(is_floating && cur_align_h == Some(align))
                .on(move |this, _, cx| this.set_picture_floating_align_h(Some(align), cx))
                .into()
        }).collect();

        let picture_align_v_items: Vec<MenuEntry> = PICTURE_ALIGN_V.iter().map(|&(id, label, align)| {
            MenuItem::new(id, crate::shell::i18n::text(label))
                .disabled(!is_floating)
                .checked(is_floating && cur_align_v == Some(align))
                .on(move |this, _, cx| this.set_picture_floating_align_v(Some(align), cx))
                .into()
        }).collect();

        let picture_dist_items: Vec<MenuEntry> = PICTURE_FLOAT_DISTANCES.iter().map(|&(id, label, dist)| {
            MenuItem::new(id, crate::shell::i18n::text(label))
                .disabled(!is_floating)
                .checked(is_floating && cur_dist == Some(dist))
                .on(move |this, _, cx| this.set_picture_floating_distance(dist, cx))
                .into()
        }).collect();

        let picture_zorder_items: Vec<MenuEntry> = PICTURE_ZORDERS.iter().map(|&(id, label, action)| {
            MenuItem::new(id, crate::shell::i18n::text(label))
                .disabled(!is_floating)
                .on(move |this, _, cx| this.reorder_picture_zorder(action, cx))
                .into()
        }).collect();

        let picture_align_distribute_items: Vec<MenuEntry> = vec![
            MenuItem::new("docx.picture.align.left", crate::shell::i18n::text("_shell.align-left"))
                .disabled(!is_floating)
                .checked(is_floating && cur_align_h == Some("left"))
                .on(|this, _, cx| this.set_picture_floating_align_h(Some("left"), cx))
                .into(),
            MenuItem::new("docx.picture.align.center", crate::shell::i18n::text("_shell.align-center"))
                .disabled(!is_floating)
                .checked(is_floating && cur_align_h == Some("center"))
                .on(|this, _, cx| this.set_picture_floating_align_h(Some("center"), cx))
                .into(),
            MenuItem::new("docx.picture.align.right", crate::shell::i18n::text("_shell.align-right"))
                .disabled(!is_floating)
                .checked(is_floating && cur_align_h == Some("right"))
                .on(|this, _, cx| this.set_picture_floating_align_h(Some("right"), cx))
                .into(),
            MenuItem::new("docx.picture.align.top", crate::shell::i18n::text("_shell.align-top"))
                .disabled(!is_floating)
                .checked(is_floating && cur_align_v == Some("top"))
                .on(|this, _, cx| this.set_picture_floating_align_v(Some("top"), cx))
                .into(),
            MenuItem::new("docx.picture.align.middle", crate::shell::i18n::text("_shell.align-center-vertically"))
                .disabled(!is_floating)
                .checked(is_floating && cur_align_v == Some("center"))
                .on(|this, _, cx| this.set_picture_floating_align_v(Some("center"), cx))
                .into(),
            MenuItem::new("docx.picture.align.bottom", crate::shell::i18n::text("_shell.align-bottom"))
                .disabled(!is_floating)
                .checked(is_floating && cur_align_v == Some("bottom"))
                .on(|this, _, cx| this.set_picture_floating_align_v(Some("bottom"), cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::new("docx.picture.distribute.horizontally", crate::shell::i18n::text("_docx.distribute-horizontally"))
                .disabled(!is_floating)
                .on(|this, _, cx| this.distribute_pictures_horizontally(cx))
                .into(),
            MenuItem::new("docx.picture.distribute.vertically", crate::shell::i18n::text("_docx.distribute-vertically"))
                .disabled(!is_floating)
                .on(|this, _, cx| this.distribute_pictures_vertically(cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::new("docx.picture.align-to.margin", crate::shell::i18n::text("_docx.align-to-margin"))
                .disabled(!is_floating)
                .checked(is_floating && cur_anchor.as_ref().is_some_and(|a| a.relative_h == "margin"))
                .on(|this, _, cx| this.set_picture_align_relative("margin", cx))
                .into(),
            MenuItem::new("docx.picture.align-to.page", crate::shell::i18n::text("_docx.align-to-page"))
                .disabled(!is_floating)
                .checked(is_floating && cur_anchor.as_ref().is_some_and(|a| a.relative_h == "page"))
                .on(|this, _, cx| this.set_picture_align_relative("page", cx))
                .into(),
        ];

        vec![
            sep(),
            ToolButton::new("picture-wrap", "wrap_text", crate::shell::i18n::text("docx.toolbar.text-wrapping-picture")).menu(picture_wrap_items).into(),
            ToolButton::new("picture-align-distribute", "align_horizontal_center", crate::shell::i18n::text("docx.toolbar.align-distribute-picture")).menu(picture_align_distribute_items).into(),
            ToolButton::new("picture-align-h", "format_align_center", crate::shell::i18n::text("docx.toolbar.horizontal-alignment-picture")).menu(picture_align_items).into(),
            ToolButton::new("picture-align-v", "vertical_align_center", crate::shell::i18n::text("docx.toolbar.vertical-alignment-picture")).menu(picture_align_v_items).into(),
            ToolButton::new("picture-distance", "space_bar", crate::shell::i18n::text("docx.toolbar.distance-text-picture")).menu(picture_dist_items).into(),
            ToolButton::new("picture-zorder", "flip_to_front", crate::shell::i18n::text("docx.toolbar.order-picture")).menu(picture_zorder_items).into(),
            ToolButton::new("picture-size", "aspect_ratio", crate::shell::i18n::text("docx.picture-panel.picture-size")).click(|this, window, cx| this.open_picture_edit(window, cx)).into(),
            ToolButton::new("picture-replace", "swap_horiz", crate::shell::i18n::text("_docx.change-picture")).click(|this, window, cx| this.doc_pick_replacement_picture(window, cx)).into(),
        ]
    }).into_iter().flatten())
    .filter(|item| {
        use super::accessibility::DocxToolbarGroup;
        match item {
            ToolItem::Select { id: "style", .. } => editor.is_docx_toolbar_group_visible(DocxToolbarGroup::Styles),
            ToolItem::Select { id: "font", .. } | ToolItem::FontSize { .. } => editor.is_docx_toolbar_group_visible(DocxToolbarGroup::Font),
            ToolItem::Button(b) => match b.id {
                "bold" | "italic" | "underline" => editor.is_docx_toolbar_group_visible(DocxToolbarGroup::Font),
                "text-color" | "highlight" => editor.is_docx_toolbar_group_visible(DocxToolbarGroup::Color),
                "link" | "image" | "equation" => editor.is_docx_toolbar_group_visible(DocxToolbarGroup::Insert),
                "align-left" | "align-center" | "align-right" | "align"
                | "valign-top" | "valign-center" | "valign-bottom" | "vertical-align"
                | "line-spacing" | "checklist" | "bullets" | "bullet-types" | "numbered"
                | "indent-less" | "indent-more" | "clear-format" | "formatting-marks" => {
                    editor.is_docx_toolbar_group_visible(DocxToolbarGroup::Paragraph)
                }
                "interaction-mode" => editor.is_docx_toolbar_group_visible(DocxToolbarGroup::Review),
                _ => true,
            },
            _ => true,
        }
    })
    .collect()
}

/// Connect the menus' items the document can do.
/// Put `new` at the end of the submenu `id` (in whatever menu it is).
fn push_into(entries: &mut Vec<MenuEntry>, id: &str, new: MenuEntry) -> Result<(), MenuEntry> {
    let mut new = new;
    for e in entries.iter_mut() {
        if let MenuEntry::Submenu(sub) = e {
            if sub.id == id {
                sub.items.push(new);
                return Ok(());
            }
            match push_into(&mut sub.items, id, new) {
                Ok(()) => return Ok(()),
                Err(back) => new = back,
            }
        }
    }
    Err(new)
}

pub(crate) fn menus(_editor: &SlideEditor, menus: &mut Menus) {
    // ファイル > 新規 > テンプレートから新規作成… (DOCX-FILE-002)
    let mut template_entry: MenuEntry = MenuItem::tr("docx.new-from-template").icon("dashboard_customize").on(|this, window, cx| this.new_from_template(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match push_into(entries, "file.new", template_entry) {
            Ok(()) => break,
            Err(back) => template_entry = back,
        }
    }
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.chart.data-quick").icon("bar_chart").disabled(!_editor.can_edit_doc_chart()).on(|e, w, cx| e.open_doc_chart_panel(w, cx)).into());
    menus.items_mut(MenuId::Format).push(_editor.doc_chart_type_entry());
    menus.fill_submenu("insert.chart", crate::xlsx::chart_insert::chart_items_with(SlideEditor::doc_insert_chart));
    menus.fill_submenu("insert.smartart", crate::docx::insert_objects::docx_smartart_insert_items());
    menus.fill_submenu(
        "tools.spelling",
        vec![
            MenuItem::tr("docx.review.spelling-grammar").icon("spellcheck").shortcut("F7").on(|e, w, cx| e.open_doc_spelling(w, cx)).into(),
            MenuItem::tr("pptx.spelling.check").icon("spellcheck").on(|e, w, cx| e.open_doc_spelling(w, cx)).into(),
            MenuItem::tr("docx.review.thesaurus").icon("menu_book").shortcut("⇧F7").on(|e, w, cx| e.open_thesaurus_panel(w, cx)).into(),
        ],
    );
    menus.connect("docx.review.spelling-grammar", |this, window, cx| this.open_doc_spelling(window, cx));
    menus.connect("docx.review.thesaurus", |this, window, cx| this.open_thesaurus_panel(window, cx));
    menus.connect("insert.symbol", |this, _, cx| this.open_symbols(cx));
    menus.connect("edit.cut", |this, _, cx| this.cut_text(cx));
    menus.connect("edit.copy", |this, _, cx| this.copy_text(cx));
    menus.connect("edit.paste", |this, _, cx| this.paste_text(cx));
    menus.connect("edit.paste-plain", |this, _, cx| this.paste_plain_text(cx));
    menus.connect("edit.paste-match", |this, _, cx| this.paste_match_format(cx));
    menus.connect("edit.copy-format", |this, _, cx| this.copy_format(cx));
    menus.connect("edit.paste-format", |this, _, cx| this.paste_format(cx));
    // 編集 > 形式を選択して貼り付け
    let paste_mode = _editor.default_paste_mode();
    let paste_default_submenu = vec![
        MenuItem::tr("docx.options.paste-default.keep-source")
            .checked(paste_mode == crate::shell::editor::DefaultPasteMode::KeepSourceFormatting)
            .on(|this, _, cx| this.set_default_paste_mode(crate::shell::editor::DefaultPasteMode::KeepSourceFormatting, cx))
            .into(),
        MenuItem::tr("docx.options.paste-default.merge-format")
            .checked(paste_mode == crate::shell::editor::DefaultPasteMode::MergeFormatting)
            .on(|this, _, cx| this.set_default_paste_mode(crate::shell::editor::DefaultPasteMode::MergeFormatting, cx))
            .into(),
        MenuItem::tr("docx.options.paste-default.text-only")
            .checked(paste_mode == crate::shell::editor::DefaultPasteMode::KeepTextOnly)
            .on(|this, _, cx| this.set_default_paste_mode(crate::shell::editor::DefaultPasteMode::KeepTextOnly, cx))
            .into(),
    ];
    let paste_special_submenu = vec![
        MenuItem::tr("docx.paste-special.keep-source")
            .icon("content_paste")
            .shortcut("⌘V")
            .on(|this, _, cx| this.paste_keep_source(cx))
            .into(),
        MenuItem::tr("docx.paste-special.match-format")
            .icon("content_paste_search")
            .shortcut("⌥⇧⌘V")
            .on(|this, _, cx| this.paste_match_format(cx))
            .into(),
        MenuItem::tr("docx.paste-special.text-only")
            .icon("content_paste_go")
            .shortcut("⇧⌘V")
            .on(|this, _, cx| this.paste_plain_text(cx))
            .into(),
        MenuItem::tr("docx.paste-special.format-only")
            .icon("format_paint")
            .shortcut("⌥⌘V")
            .on(|this, _, cx| this.paste_format(cx))
            .into(),
        Submenu::tr("docx.options.paste-default", "settings", paste_default_submenu).into(),
    ];
    menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.autocorrect-options").icon("spellcheck").on(|this, window, cx| this.open_doc_autocorrect_panel(window, cx)).into());
    menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.autorecover-settings").icon("history").on(|this, window, cx| this.open_doc_autorecover_settings(window, cx)).into());
    menus.items_mut(MenuId::File).push(MenuItem::tr("docx.recover-unsaved").icon("restore_page").on(|this, _, cx| this.open_doc_recover_unsaved(cx)).into());
    let drawing_table = _editor.doc_state().is_some_and(|s| s.draw_table_mode);
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("docx.insert.draw-table").icon("border_style").checked(drawing_table).on(|this, _, cx| this.toggle_draw_table(cx)).into());
    let edit = menus.items_mut(MenuId::Edit);
    let at = edit.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "edit.paste-format")).map_or(edit.len(), |i| i + 1);
    edit.insert(at, Submenu::tr("docx.paste-special", "content_paste", paste_special_submenu).into());
    let autocorrect_on = !_editor.doc_state().is_some_and(|s| s.autocorrect_off);
    edit.push(MenuItem::tr("docx.edit.autocorrect").checked(autocorrect_on).on(|this, _, cx| this.toggle_autocorrect(cx)).into());
    let overtype = _editor.is_overtype();
    edit.push(
        MenuItem::tr("docx.edit.overtype")
            .checked(overtype)
            .shortcut("Ins")
            .on(|this, _, cx| this.toggle_overtype(cx))
            .into(),
    );
    let drag_select_subitems: Vec<MenuEntry> = vec![
        MenuItem::tr("docx.edit.drag-drop")
            .checked(_editor.is_drag_drop_text_editing_enabled())
            .on(|this, _, cx| this.toggle_drag_drop_text_editing(cx))
            .into(),
        MenuItem::tr("docx.edit.select-word")
            .checked(_editor.is_auto_select_entire_word_enabled())
            .on(|this, _, cx| this.toggle_auto_select_entire_word(cx))
            .into(),
    ];
    edit.push(Submenu::tr("docx.edit.drag-select", "touch_app", drag_select_subitems).into());
    let find_at = edit.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "edit.find-replace")).map_or(edit.len(), |i| i + 1);
    edit.insert(
        find_at,
        MenuItem::tr("docx.find.next")
            .shortcut("⌘G")
            .on(|this, _, cx| this.find_next_in_direction(cx))
            .into(),
    );
    edit.insert(
        find_at + 1,
        MenuItem::tr("docx.find.previous")
            .shortcut("⇧⌘G")
            .on(|this, _, cx| this.find_next(false, cx))
            .into(),
    );
    edit.insert(
        find_at + 2,
        MenuItem::tr("docx.go-to")
            .shortcut("⌥⌘G")
            .on(|this, window, cx| this.open_go_to_panel(window, cx))
            .into(),
    );

    menus.connect("docx.find.next", |this, _, cx| this.find_next_in_direction(cx));
    menus.connect("docx.find.previous", |this, _, cx| this.find_next(false, cx));

    menus.connect("docx.insert.draw-table", |this, _, cx| this.toggle_draw_table(cx));
    menus.connect("docx.backup-always", |this, _, cx| this.toggle_doc_backup(cx));
    menus.connect("docx.autorecover-settings", |this, window, cx| this.open_doc_autorecover_settings(window, cx));
    menus.connect("docx.recover-unsaved", |this, _, cx| this.open_doc_recover_unsaved(cx));
    menus.connect("docx.autocorrect-options", |this, window, cx| this.open_doc_autocorrect_panel(window, cx));
    menus.connect("docx.edit.autocorrect", |this, _, cx| this.toggle_autocorrect(cx));
    menus.connect("docx.edit.overtype", |this, _, cx| this.toggle_overtype(cx));
    menus.connect("docx.edit.drag-drop", |this, _, cx| this.toggle_drag_drop_text_editing(cx));
    menus.connect("docx.edit.select-word", |this, _, cx| this.toggle_auto_select_entire_word(cx));
    menus.connect("docx.paste-special.keep-source", |this, _, cx| this.paste_keep_source(cx));
    menus.connect("docx.paste-special.match-format", |this, _, cx| this.paste_match_format(cx));
    menus.connect("docx.paste-special.text-only", |this, _, cx| this.paste_plain_text(cx));
    menus.connect("docx.paste-special.format-only", |this, _, cx| this.paste_format(cx));
    menus.connect("docx.options.paste-default.keep-source", |this, _, cx| this.set_default_paste_mode(crate::shell::editor::DefaultPasteMode::KeepSourceFormatting, cx));
    menus.connect("docx.options.paste-default.merge-format", |this, _, cx| this.set_default_paste_mode(crate::shell::editor::DefaultPasteMode::MergeFormatting, cx));
    menus.connect("docx.options.paste-default.text-only", |this, _, cx| this.set_default_paste_mode(crate::shell::editor::DefaultPasteMode::KeepTextOnly, cx));
    menus.connect("docx.fields.insert", |this, window, cx| this.open_field_panel(window, cx));
    menus.connect("docx.fields.edit", |this, window, cx| this.open_field_panel(window, cx));
    menus.connect("docx.fields.update", |this, _, cx| this.update_fields(false, cx));
    menus.connect("docx.fields.unlink", |this, _, cx| this.unlink_fields(cx));
    menus.connect("docx.fields.toggle", |this, _, cx| this.toggle_field_codes(cx));
    menus.connect("docx.fields.toggle-all", |this, _, cx| this.toggle_all_field_codes(cx));
    menus.connect("docx.fields.lock", |this, _, cx| this.lock_fields(true, cx));
    menus.connect("docx.fields.unlock", |this, _, cx| this.lock_fields(false, cx));
    menus.connect("docx.caption.figure", |this, _, cx| this.insert_caption(&crate::shell::i18n::text("docx.common.figure"), cx));
    menus.connect("docx.caption.table", |this, _, cx| this.insert_caption(&crate::shell::i18n::text("insert.table"), cx));
    menus.connect("docx.caption.equation", |this, _, cx| this.insert_caption(&crate::shell::i18n::text("insert.equation"), cx));
    menus.connect("docx.caption.insert", |this, window, cx| this.open_caption_panel(window, cx));
    menus.connect("docx.xref.dialog", |this, window, cx| this.open_cross_reference_panel(window, cx));
    menus.connect("align.left", |this, _, cx| this.set_alignment(FlowAlign::Left, cx));
    menus.connect("align.center", |this, _, cx| this.set_alignment(FlowAlign::Center, cx));
    menus.connect("align.right", |this, _, cx| this.set_alignment(FlowAlign::Right, cx));
    menus.connect("align.justify", |this, _, cx| this.set_alignment(FlowAlign::Justify, cx));
    menus.connect("align.distribute", |this, _, cx| this.set_alignment(FlowAlign::Distribute, cx));

    menus.connect("docx.drop-cap", |this, _, cx| this.set_drop_cap_location(ibis_doc_model::DropCapLocation::Drop, cx));
    menus.connect("docx.drop-cap.none", |this, _, cx| this.set_drop_cap_location(ibis_doc_model::DropCapLocation::None, cx));
    menus.connect("docx.drop-cap.drop", |this, _, cx| this.set_drop_cap_location(ibis_doc_model::DropCapLocation::Drop, cx));
    menus.connect("docx.drop-cap.margin", |this, _, cx| this.set_drop_cap_location(ibis_doc_model::DropCapLocation::Margin, cx));
    menus.connect("docx.drop-cap.lines.2", |this, _, cx| this.set_drop_cap_lines(2, cx));
    menus.connect("docx.drop-cap.lines.3", |this, _, cx| this.set_drop_cap_lines(3, cx));
    menus.connect("docx.drop-cap.lines.4", |this, _, cx| this.set_drop_cap_lines(4, cx));
    menus.connect("docx.drop-cap.lines.5", |this, _, cx| this.set_drop_cap_lines(5, cx));
    menus.connect("docx.drop-cap.distance.0", |this, _, cx| this.set_drop_cap_distance(0, cx));
    menus.connect("docx.drop-cap.distance.3", |this, _, cx| this.set_drop_cap_distance(3 * 12700, cx));
    menus.connect("docx.drop-cap.distance.6", |this, _, cx| this.set_drop_cap_distance(6 * 12700, cx));
    menus.connect("docx.drop-cap.distance.12", |this, _, cx| this.set_drop_cap_distance(12 * 12700, cx));

    menus.connect("docx.shape.align-text", |this, _, cx| this.set_shape_anchor(ibis_doc_model::VerticalAnchor::Middle, cx));
    menus.connect("docx.shape.align-text.top", |this, _, cx| this.set_shape_anchor(ibis_doc_model::VerticalAnchor::Top, cx));
    menus.connect("docx.shape.align-text.middle", |this, _, cx| this.set_shape_anchor(ibis_doc_model::VerticalAnchor::Middle, cx));
    menus.connect("docx.shape.align-text.bottom", |this, _, cx| this.set_shape_anchor(ibis_doc_model::VerticalAnchor::Bottom, cx));

    menus.connect("edit.select-all", |this, _, cx| this.select_all(cx));
    menus.connect("format.clear", |this, _, cx| this.clear_formatting(cx));
    menus.connect("file.info", |this, window, cx| this.open_doc_properties_panel(window, cx));
    menus.connect("docx.properties", |this, window, cx| this.open_doc_properties_panel(window, cx));
    menus.connect("docx.mark-as-final", |this, _, cx| this.toggle_mark_as_final(cx));
    menus.connect("docx.read-only-recommended", |this, _, cx| this.toggle_read_only_recommended(cx));
    menus.connect("docx.inspect-document", |this, window, cx| this.open_doc_inspect_panel(window, cx));
    menus.connect("docx.compatibility-check", |this, window, cx| this.open_doc_compatibility_panel(window, cx));
    menus.connect("docx.print-settings", |this, window, cx| this.open_doc_print_panel(window, cx));
    menus.connect("docx.options.print.update-fields", |this, _, cx| this.toggle_update_fields_before_printing(cx));
    menus.connect("docx.options.print.update-links", |this, _, cx| this.toggle_update_links_before_printing(cx));
    menus.connect("docx.options.print.backgrounds", |this, _, cx| this.toggle_print_backgrounds(cx));
    menus.connect("docx.options.print.hidden-text", |this, _, cx| this.toggle_print_hidden_text(cx));
    menus.connect("docx.options.print.markup", |this, _, cx| this.toggle_print_markup(cx));
    menus.connect("docx.picture.compress", |this, window, cx| this.open_docx_compress_panel(window, cx));
    menus.connect("docx.options.image.do-not-compress", |this, _, cx| this.toggle_do_not_compress_images(cx));
    menus.connect("docx.options.image.discard-cropped", |this, _, cx| this.toggle_discard_image_editing_data(cx));
    menus.connect("docx.options.image.res-high-fidelity", |this, _, cx| this.set_default_image_dpi(Some(32767), cx));
    menus.connect("docx.options.image.res-hd", |this, _, cx| this.set_default_image_dpi(Some(330), cx));
    menus.connect("docx.options.image.res-print", |this, _, cx| this.set_default_image_dpi(Some(220), cx));
    menus.connect("docx.options.image.res-web", |this, _, cx| this.set_default_image_dpi(Some(150), cx));
    menus.connect("docx.options.image.res-email", |this, _, cx| this.set_default_image_dpi(Some(96), cx));
    menus.connect("docx.options.save-frequency.default", |this, _, cx| this.set_save_frequency(None, cx));
    menus.connect("docx.options.save-frequency.5min", |this, _, cx| this.set_save_frequency(Some(5), cx));
    menus.connect("docx.options.save-frequency.10min", |this, _, cx| this.set_save_frequency(Some(10), cx));
    menus.connect("docx.options.save-frequency.15min", |this, _, cx| this.set_save_frequency(Some(15), cx));
    menus.connect("docx.options.save-frequency.20min", |this, _, cx| this.set_save_frequency(Some(20), cx));
    menus.connect("docx.options.save-frequency.30min", |this, _, cx| this.set_save_frequency(Some(30), cx));
    menus.connect("docx.options.save-frequency.off", |this, _, cx| this.toggle_autorecover_disabled_for_doc(cx));
    menus.connect("docx.options.autorecover.disable-doc", |this, _, cx| this.toggle_autorecover_disabled_for_doc(cx));
    menus.connect("docx.options.font.embed", |this, _, cx| this.toggle_embed_true_type_fonts(cx));
    menus.connect("docx.options.font.save-subset", |this, _, cx| this.toggle_save_subset_fonts(cx));
    menus.connect("docx.options.font.embed-system", |this, _, cx| this.toggle_embed_system_fonts(cx));
    menus.connect("docx.options.compat.mode-word2013", |this, _, cx| this.set_compatibility_mode(Some(15), cx));
    menus.connect("docx.options.compat.mode-word2010", |this, _, cx| this.set_compatibility_mode(Some(14), cx));
    menus.connect("docx.options.compat.mode-word2007", |this, _, cx| this.set_compatibility_mode(Some(12), cx));
    menus.connect("docx.options.compat.mode-word2003", |this, _, cx| this.set_compatibility_mode(Some(11), cx));
    menus.connect("docx.options.compat.use-fe-layout", |this, _, cx| this.toggle_use_fe_layout(cx));
    menus.connect("docx.options.compat.shift-return", |this, _, cx| this.toggle_do_not_expand_shift_return(cx));
    menus.connect("docx.file.convert", |this, _, cx| this.convert_docx_to_current_format(cx));
    menus.connect("docx.file.repair", |this, window, cx| this.open_docx_repair_panel(window, cx));
    menus.connect("docx.template.attach", |this, window, cx| this.open_docx_template_panel(window, cx));

    let file = menus.items_mut(MenuId::File);
    let at = file.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "file.info")).map_or(file.len(), |i| i + 1);
    file.insert(at, MenuItem::tr("docx.backup-always").icon("backup").checked(super::backup::backup_enabled()).on(|this, _, cx| this.toggle_doc_backup(cx)).into());
    file.insert(
        at,
        MenuItem::tr("docx.mark-as-final")
            .icon("lock")
            .checked(_editor.is_mark_as_final())
            .on(|this, _, cx| this.toggle_mark_as_final(cx))
            .into(),
    );
    file.insert(
        at + 1,
        MenuItem::tr("docx.read-only-recommended")
            .icon("visibility")
            .checked(_editor.is_read_only_recommended())
            .on(|this, _, cx| this.toggle_read_only_recommended(cx))
            .into(),
    );
    file.insert(
        at + 2,
        MenuItem::tr("docx.inspect-document")
            .icon("find_in_page")
            .on(|this, window, cx| this.open_doc_inspect_panel(window, cx))
            .into(),
    );
    file.insert(
        at + 3,
        MenuItem::tr("docx.compatibility-check")
            .icon("rule")
            .on(|this, window, cx| this.open_doc_compatibility_panel(window, cx))
            .into(),
    );
    file.insert(
        at + 4,
        MenuItem::tr("docx.file.convert")
            .icon("upgrade")
            .disabled(!_editor.is_docx_compatibility_mode())
            .on(|this, _, cx| this.convert_docx_to_current_format(cx))
            .into(),
    );
    let print_at = file.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "file.print")).map_or(file.len(), |i| i + 1);
    file.insert(
        print_at,
        MenuItem::tr("docx.print-settings")
            .icon("tune")
            .on(|this, window, cx| this.open_doc_print_panel(window, cx))
            .into(),
    );
    file.insert(
        print_at + 1,
        Submenu::tr(
            "docx.print-options",
            "print",
            vec![
                MenuItem::tr("docx.options.print.update-fields")
                    .checked(_editor.is_update_fields_before_printing())
                    .on(|this, _, cx| this.toggle_update_fields_before_printing(cx))
                    .into(),
                MenuItem::tr("docx.options.print.update-links")
                    .checked(_editor.is_update_links_before_printing())
                    .on(|this, _, cx| this.toggle_update_links_before_printing(cx))
                    .into(),
                MenuItem::tr("docx.options.print.backgrounds")
                    .checked(_editor.is_print_backgrounds())
                    .on(|this, _, cx| this.toggle_print_backgrounds(cx))
                    .into(),
                MenuItem::tr("docx.options.print.hidden-text")
                    .checked(_editor.is_print_hidden_text())
                    .on(|this, _, cx| this.toggle_print_hidden_text(cx))
                    .into(),
                MenuItem::tr("docx.options.print.markup")
                    .checked(_editor.is_print_markup())
                    .on(|this, _, cx| this.toggle_print_markup(cx))
                    .into(),
            ],
        )
        .into(),
    );
    let cur_dpi = _editor.default_image_dpi();
    let res_items = vec![
        MenuItem::tr("docx.options.image.res-high-fidelity")
            .checked(cur_dpi == Some(32767) || cur_dpi == Some(0))
            .on(|this, _, cx| this.set_default_image_dpi(Some(32767), cx))
            .into(),
        MenuItem::tr("docx.options.image.res-hd")
            .checked(cur_dpi == Some(330))
            .on(|this, _, cx| this.set_default_image_dpi(Some(330), cx))
            .into(),
        MenuItem::tr("docx.options.image.res-print")
            .checked(cur_dpi == Some(220) || cur_dpi.is_none())
            .on(|this, _, cx| this.set_default_image_dpi(Some(220), cx))
            .into(),
        MenuItem::tr("docx.options.image.res-web")
            .checked(cur_dpi == Some(150))
            .on(|this, _, cx| this.set_default_image_dpi(Some(150), cx))
            .into(),
        MenuItem::tr("docx.options.image.res-email")
            .checked(cur_dpi == Some(96))
            .on(|this, _, cx| this.set_default_image_dpi(Some(96), cx))
            .into(),
    ];
    file.insert(
        print_at + 1,
        Submenu::tr(
            "docx.options.image-quality",
            "image",
            vec![
                MenuItem::tr("docx.picture.compress")
                    .icon("image")
                    .on(|this, window, cx| this.open_docx_compress_panel(window, cx))
                    .into(),
                MenuItem::tr("docx.options.image.do-not-compress")
                    .checked(_editor.is_do_not_compress_images())
                    .on(|this, _, cx| this.toggle_do_not_compress_images(cx))
                    .into(),
                MenuItem::tr("docx.options.image.discard-cropped")
                    .checked(_editor.is_discard_image_editing_data())
                    .on(|this, _, cx| this.toggle_discard_image_editing_data(cx))
                    .into(),
                Submenu::tr(
                    "docx.options.image.default-resolution",
                    "tune",
                    res_items,
                )
                .into(),
            ],
        )
        .into(),
    );

    let cur_freq = _editor.save_frequency();
    let is_disabled = _editor.is_autorecover_disabled_for_doc();
    let freq_items = vec![
        MenuItem::tr("docx.options.save-frequency.default")
            .checked(cur_freq.is_none() && !is_disabled)
            .on(|this, _, cx| this.set_save_frequency(None, cx))
            .into(),
        MenuItem::tr("docx.options.save-frequency.5min")
            .checked(cur_freq == Some(5) && !is_disabled)
            .on(|this, _, cx| this.set_save_frequency(Some(5), cx))
            .into(),
        MenuItem::tr("docx.options.save-frequency.10min")
            .checked(cur_freq == Some(10) && !is_disabled)
            .on(|this, _, cx| this.set_save_frequency(Some(10), cx))
            .into(),
        MenuItem::tr("docx.options.save-frequency.15min")
            .checked(cur_freq == Some(15) && !is_disabled)
            .on(|this, _, cx| this.set_save_frequency(Some(15), cx))
            .into(),
        MenuItem::tr("docx.options.save-frequency.20min")
            .checked(cur_freq == Some(20) && !is_disabled)
            .on(|this, _, cx| this.set_save_frequency(Some(20), cx))
            .into(),
        MenuItem::tr("docx.options.save-frequency.30min")
            .checked(cur_freq == Some(30) && !is_disabled)
            .on(|this, _, cx| this.set_save_frequency(Some(30), cx))
            .into(),
        MenuItem::tr("docx.options.save-frequency.off")
            .checked(is_disabled)
            .on(|this, _, cx| this.toggle_autorecover_disabled_for_doc(cx))
            .into(),
    ];
    file.insert(
        print_at + 2,
        Submenu::tr(
            "docx.options.autorecover",
            "history",
            vec![
                MenuItem::tr("docx.autorecover-settings")
                    .icon("settings")
                    .on(|this, window, cx| this.open_doc_autorecover_settings(window, cx))
                    .into(),
                Submenu::tr("docx.options.autorecover.interval", "timer", freq_items).into(),
                MenuItem::tr("docx.options.autorecover.disable-doc")
                    .checked(is_disabled)
                    .on(|this, _, cx| this.toggle_autorecover_disabled_for_doc(cx))
                    .into(),
                MenuItem::tr("docx.recover-unsaved")
                    .icon("restore_page")
                    .on(|this, _, cx| this.open_doc_recover_unsaved(cx))
                    .into(),
            ],
        )
        .into(),
    );

    let embed_fonts = _editor.is_embed_true_type_fonts();
    let save_subset = _editor.is_save_subset_fonts();
    let do_not_embed_system = !_editor.is_embed_system_fonts();
    let cur_mode = _editor.compatibility_mode();
    let use_fe = _editor.is_use_fe_layout();
    let shift_ret = _editor.is_do_not_expand_shift_return();

    file.insert(
        print_at + 3,
        Submenu::tr(
            "docx.options.font-embedding",
            "font_download",
            vec![
                MenuItem::tr("docx.options.font.embed")
                    .checked(embed_fonts)
                    .on(|this, _, cx| this.toggle_embed_true_type_fonts(cx))
                    .into(),
                MenuItem::tr("docx.options.font.save-subset")
                    .checked(save_subset)
                    .on(|this, _, cx| this.toggle_save_subset_fonts(cx))
                    .into(),
                MenuItem::tr("docx.options.font.embed-system")
                    .checked(do_not_embed_system)
                    .on(|this, _, cx| this.toggle_embed_system_fonts(cx))
                    .into(),
            ],
        )
        .into(),
    );

    let compat_modes = vec![
        MenuItem::tr("docx.options.compat.mode-word2013")
            .checked(cur_mode == Some(15) || cur_mode.is_none())
            .on(|this, _, cx| this.set_compatibility_mode(Some(15), cx))
            .into(),
        MenuItem::tr("docx.options.compat.mode-word2010")
            .checked(cur_mode == Some(14))
            .on(|this, _, cx| this.set_compatibility_mode(Some(14), cx))
            .into(),
        MenuItem::tr("docx.options.compat.mode-word2007")
            .checked(cur_mode == Some(12))
            .on(|this, _, cx| this.set_compatibility_mode(Some(12), cx))
            .into(),
        MenuItem::tr("docx.options.compat.mode-word2003")
            .checked(cur_mode == Some(11))
            .on(|this, _, cx| this.set_compatibility_mode(Some(11), cx))
            .into(),
    ];

    file.insert(
        print_at + 4,
        Submenu::tr(
            "docx.options.compatibility",
            "tune",
            vec![
                Submenu::tr("docx.options.compat.mode", "history", compat_modes).into(),
                MenuItem::tr("docx.options.compat.use-fe-layout")
                    .checked(use_fe)
                    .on(|this, _, cx| this.toggle_use_fe_layout(cx))
                    .into(),
                MenuItem::tr("docx.options.compat.shift-return")
                    .checked(shift_ret)
                    .on(|this, _, cx| this.toggle_do_not_expand_shift_return(cx))
                    .into(),
            ],
        )
        .into(),
    );

    file.insert(
        print_at + 5,
        MenuItem::tr("docx.file.repair")
            .icon("cleaning_services")
            .disabled(super::repair::damaged_document().is_none())
            .on(|this, window, cx| this.open_docx_repair_damaged(window, cx))
            .into(),
    );
    file.insert(
        print_at + 6,
        MenuItem::tr("docx.template.attach")
            .icon("extension")
            .on(|this, window, cx| this.open_docx_template_panel(window, cx))
            .into(),
    );


    // Format conversion is a File > Export choice, not a separate settings screen.
    for (id, key, format) in [
        ("docx.file.export.doc", "docx.convert.format.doc", ibis_pptx::docx_convert::DocxConvertFormat::Doc),
        ("docx.file.export.rtf", "docx.convert.format.rtf", ibis_pptx::docx_convert::DocxConvertFormat::Rtf),
        ("docx.file.export.txt", "docx.convert.format.txt", ibis_pptx::docx_convert::DocxConvertFormat::Txt),
        ("docx.file.export.html", "docx.convert.format.html", ibis_pptx::docx_convert::DocxConvertFormat::Html),
        ("docx.file.export.odt", "docx.convert.format.odt", ibis_pptx::docx_convert::DocxConvertFormat::Odt),
    ] {
        let entry = MenuItem::new(id, crate::shell::i18n::text(key)).icon("description")
            .on(move |editor, window, cx| editor.export_docx_format_dialog(format, window, cx)).into();
        let _ = push_into(menus.items_mut(MenuId::File), "file.export", entry);
    }

    menus.connect("tools.word-count", |this, window, cx| this.open_word_count_panel(window, cx));
    // The shell's items that every format shows: Word's Page Setup (the panel of 書式 > ページ設定), the dictionary (the thesaurus) and the
    // accessibility check were greyed in the menus.
    menus.connect("file.page-setup", |this, window, cx| this.open_page_setup_panel(window, cx));
    menus.connect("tools.dictionary", |this, window, cx| this.open_thesaurus_panel(window, cx));
    menus.connect("tools.accessibility", |this, _, cx| this.open_accessibility_panel(cx));
    menus.connect("docx.tabs", |this, window, cx| this.open_tab_edit(window, cx));
    for (i, &(id, _, _, _)) in super::list_formats::BULLET_LIBRARY.iter().enumerate() {
        menus.connect(id, move |this, _, cx| this.apply_bullet_format(i, cx));
    }
    menus.connect("docx.bullet.toggle", |this, _, cx| this.toggle_list(true, cx));
    menus.connect("docx.list.toggle", |this, _, cx| this.toggle_list(false, cx));
    menus.connect("docx.list.define-style", |this, window, cx| this.open_define_list_style_panel(window, cx));
    menus.connect("docx.bullet.suffix-tab", |this, _, cx| this.set_bullet_suffix(ibis_doc_model::ListSuffix::Tab, cx));
    menus.connect("docx.bullet.suffix-space", |this, _, cx| this.set_bullet_suffix(ibis_doc_model::ListSuffix::Space, cx));
    menus.connect("docx.bullet.suffix-none", |this, _, cx| this.set_bullet_suffix(ibis_doc_model::ListSuffix::Nothing, cx));
    menus.connect("docx.cell.direction.lrtb", |this, _, cx| {
        this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Direction { cells: vec![(r, c)], direction: ibis_doc_model::CellDirection::LrTb }, cx);
    });
    menus.connect("docx.cell.direction.tbrl", |this, _, cx| {
        this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Direction { cells: vec![(r, c)], direction: ibis_doc_model::CellDirection::TbRl }, cx);
    });
    menus.connect("docx.cell.direction.tblrv", |this, _, cx| {
        this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Direction { cells: vec![(r, c)], direction: ibis_doc_model::CellDirection::TbLrV }, cx);
    });
    menus.connect("docx.cell.direction.btlr", |this, _, cx| {
        this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Direction { cells: vec![(r, c)], direction: ibis_doc_model::CellDirection::BtLr }, cx);
    });
    menus.connect("docx.cell.margins.normal", |this, _, cx| {
        this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Margins { cells: vec![(r, c)], margins: CELL_MARGIN_NORMAL }, cx);
    });
    menus.connect("docx.cell.margins.none", |this, _, cx| {
        this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Margins { cells: vec![(r, c)], margins: CELL_MARGIN_NONE }, cx);
    });
    menus.connect("docx.cell.margins.narrow", |this, _, cx| {
        this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Margins { cells: vec![(r, c)], margins: CELL_MARGIN_NARROW }, cx);
    });
    menus.connect("docx.cell.margins.wide", |this, _, cx| {
        this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Margins { cells: vec![(r, c)], margins: CELL_MARGIN_WIDE }, cx);
    });
    menus.connect("docx.cell.wrap-text", |this, _, cx| this.toggle_cell_wrap_text(cx));
    menus.connect("docx.table.align.left", |this, _, cx| {
        this.edit_table(|_, _, _| ibis_doc_model::TableEdit::Align(ibis_doc_model::FlowAlign::Left), cx);
    });
    menus.connect("docx.table.align.center", |this, _, cx| {
        this.edit_table(|_, _, _| ibis_doc_model::TableEdit::Align(ibis_doc_model::FlowAlign::Center), cx);
    });
    menus.connect("docx.table.align.right", |this, _, cx| {
        this.edit_table(|_, _, _| ibis_doc_model::TableEdit::Align(ibis_doc_model::FlowAlign::Right), cx);
    });
    for &(id, _, val) in TABLE_INDENTS {
        menus.connect(id, move |this, _, cx| {
            this.edit_table(move |_, _, _| ibis_doc_model::TableEdit::Indent(val), cx);
        });
    }
    menus.connect("docx.table.indent.none", |this, _, cx| {
        this.edit_table(|_, _, _| ibis_doc_model::TableEdit::Indent(0), cx);
    });
    menus.connect("docx.table.indent.5mm", |this, _, cx| {
        this.edit_table(|_, _, _| ibis_doc_model::TableEdit::Indent(5 * 36000), cx);
    });
    menus.connect("docx.table.indent.10mm", |this, _, cx| {
        this.edit_table(|_, _, _| ibis_doc_model::TableEdit::Indent(10 * 36000), cx);
    });
    menus.connect("docx.table.indent.15mm", |this, _, cx| {
        this.edit_table(|_, _, _| ibis_doc_model::TableEdit::Indent(15 * 36000), cx);
    });
    menus.connect("docx.table.indent.20mm", |this, _, cx| {
        this.edit_table(|_, _, _| ibis_doc_model::TableEdit::Indent(20 * 36000), cx);
    });
    for &(id, _, val) in TABLE_CELL_SPACINGS {
        menus.connect(id, move |this, _, cx| {
            this.edit_table(move |_, _, _| ibis_doc_model::TableEdit::CellSpacing(val), cx);
        });
    }
    menus.connect("docx.table.split", |this, _, cx| this.split_table(cx));
    menus.connect("docx.table.select-cell", |this, _, cx| this.select_table_cell(cx));
    menus.connect("docx.table.select-row", |this, _, cx| this.select_table_row(cx));
    menus.connect("docx.table.select-table", |this, _, cx| this.select_table(cx));
    menus.connect("docx.table.formula", |this, window, cx| this.open_table_formula(window, cx));
    menus.connect("docx.columns.1", |this, _, cx| this.set_columns(1, cx));
    menus.connect("docx.columns.2", |this, _, cx| this.set_columns(2, cx));
    menus.connect("docx.columns.3", |this, _, cx| this.set_columns(3, cx));
    menus.connect("docx.columns.separator", |this, _, cx| this.toggle_column_separator(cx));
    menus.connect("docx.table.select.cell", |this, _, cx| this.select_table_cell(cx));
    menus.connect("docx.table.select.column", |this, _, cx| this.select_table_column(cx));
    menus.connect("docx.table.select.row", |this, _, cx| this.select_table_row(cx));
    menus.connect("docx.table.select.table", |this, _, cx| this.select_table(cx));
    menus.connect("docx.table.cell-insert.shift-right", |this, _, cx| this.edit_table(|r, c, _| ibis_doc_model::TableEdit::InsertCellShiftRight { row: r, cell: c }, cx));
    menus.connect("docx.table.cell-insert.shift-down", |this, _, cx| this.edit_table(|r, c, _| ibis_doc_model::TableEdit::InsertCellShiftDown { row: r, cell: c }, cx));
    menus.connect("docx.table.cell-delete.shift-left", |this, _, cx| this.edit_table(|r, c, _| ibis_doc_model::TableEdit::DeleteCellShiftLeft { row: r, cell: c }, cx));
    menus.connect("docx.table.cell-delete.shift-up", |this, _, cx| this.edit_table(|r, c, _| ibis_doc_model::TableEdit::DeleteCellShiftUp { row: r, cell: c }, cx));
    menus.connect("docx.table.sort.asc", |this, _, cx| this.sort_table(true, true, cx));
    menus.connect("docx.table.sort.desc", |this, _, cx| this.sort_table(false, true, cx));
    menus.connect("docx.table.sort.asc-all", |this, _, cx| this.sort_table(true, false, cx));
    menus.connect("docx.table.sort.desc-all", |this, _, cx| this.sort_table(false, false, cx));
    menus.connect("docx.table.wrap.none", |this, _, cx| this.set_table_floating(None, cx));
    menus.connect("docx.table.wrap.around-left", |this, _, cx| this.set_table_float_horz_align(Some(ibis_doc_model::TableAlign::Start), cx));
    menus.connect("docx.table.wrap.around-center", |this, _, cx| this.set_table_float_horz_align(Some(ibis_doc_model::TableAlign::Center), cx));
    menus.connect("docx.table.wrap.around-right", |this, _, cx| this.set_table_float_horz_align(Some(ibis_doc_model::TableAlign::End), cx));
    for &(id, _, dist) in TABLE_FLOAT_DISTANCES {
        menus.connect(id, move |this, _, cx| this.set_table_float_distance(dist, cx));
    }
    menus.connect("docx.table.style.none", |this, _, cx| this.set_docx_table_style(None, cx));
    menus.connect("docx.table.style.TableGrid", |this, _, cx| this.set_docx_table_style(Some("TableGrid".to_string()), cx));
    menus.connect("docx.table.style.table-grid", |this, _, cx| this.set_docx_table_style(Some("TableGrid".to_string()), cx));
    menus.connect("docx.formatting-marks", |this, _, cx| this.toggle_formatting_marks(cx));
    menus.connect("docx.text.hidden", |this, _, cx| this.toggle_hidden(cx));
    menus.connect("docx.format.hidden", |this, _, cx| this.toggle_hidden(cx));
    menus.connect("docx.view.marks.spaces", |this, _, cx| this.toggle_show_spaces(cx));
    menus.connect("docx.view.marks.tabs", |this, _, cx| this.toggle_show_tabs(cx));
    menus.connect("docx.view.marks.paragraph", |this, _, cx| this.toggle_show_paragraph_marks(cx));
    menus.connect("docx.view.marks.line-breaks", |this, _, cx| this.toggle_show_line_breaks(cx));
    menus.connect("docx.view.marks.hidden", |this, _, cx| this.toggle_show_hidden_text(cx));
    menus.connect("docx.picture.border.none", |this, _, cx| this.clear_picture_border(cx));
    for &(_label, w) in &super::picture_panel::PICTURE_BORDER_WIDTHS {
        let menu_id = match w {
            6_350 => "docx.picture.border.w050",
            12_700 => "docx.picture.border.w100",
            19_050 => "docx.picture.border.w150",
            28_575 => "docx.picture.border.w225",
            38_100 => "docx.picture.border.w300",
            57_150 => "docx.picture.border.w450",
            _ => "docx.picture.border.w600",
        };
        menus.connect(menu_id, move |this, _, cx| this.set_picture_border_width(w, cx));
    }
    for &(_label, c) in &super::picture_panel::PICTURE_BORDER_COLORS {
        let menu_id = match c {
            0x000000 => "docx.picture.border.color.black",
            0x757575 => "docx.picture.border.color.gray",
            0xD32F2F => "docx.picture.border.color.red",
            0x1B65D4 => "docx.picture.border.color.blue",
            0x2E7D32 => "docx.picture.border.color.green",
            _ => "docx.picture.border.color.orange",
        };
        menus.connect(menu_id, move |this, _, cx| this.set_picture_border_color(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(c)), cx));
    }
    for &(_label, d) in &super::picture_panel::PICTURE_BORDER_DASHES {
        let menu_id = match d {
            ibis_doc_model::LineDash::Solid => "docx.picture.border.dash.solid",
            ibis_doc_model::LineDash::Dot => "docx.picture.border.dash.dot",
            ibis_doc_model::LineDash::Dash => "docx.picture.border.dash.dash",
            ibis_doc_model::LineDash::LgDash => "docx.picture.border.dash.lgdash",
            _ => "docx.picture.border.dash.dashdot",
        };
        menus.connect(menu_id, move |this, _, cx| this.set_picture_border_dash(d, cx));
    }
    for &(id, preset) in &super::picture_panel::PICTURE_CROP_SHAPES {
        menus.connect(id, move |this, _, cx| this.crop_picture_to_shape(preset, cx));
    }
    menus.items_mut(MenuId::Format).push(MenuItem::tr("docx.format-brush.apply")
        .disabled(!_editor.doc_state().is_some_and(|s| s.format_brush.is_some()))
        .on(|this, _, cx| this.finish_doc_format_brush(cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("docx.format-brush.lock")
        .checked(_editor.doc_state().is_some_and(|s| s.format_brush.as_ref().is_some_and(|b| b.persistent)))
        .on(|this, _, cx| this.toggle_doc_format_brush_persistent(cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("docx.format-brush.cancel")
        .disabled(!_editor.doc_state().is_some_and(|s| s.format_brush.is_some()))
        .on(|this, _, cx| this.cancel_doc_format_brush(cx)).into());
    for entry in menus.items_mut(MenuId::Format) {
        if let MenuEntry::Submenu(sub)=entry && sub.id=="format.borders" {
            sub.items=super::paragraph_decorations::menu(_editor);
        }
    }
    menus.connect("docx.picture.replace", |this, window, cx| this.doc_pick_replacement_picture(window, cx));
    for &(id, _, wrap) in PICTURE_WRAPS {
        menus.connect(id, move |this, _, cx| this.set_picture_wrap(wrap, cx));
    }
    for &(id, _, dist) in PICTURE_FLOAT_DISTANCES {
        menus.connect(id, move |this, _, cx| this.set_picture_floating_distance(dist, cx));
    }
    for &(id, _, align) in PICTURE_ALIGN_H {
        menus.connect(id, move |this, _, cx| this.set_picture_floating_align_h(Some(align), cx));
    }
    for &(id, _, align) in PICTURE_ALIGN_V {
        menus.connect(id, move |this, _, cx| this.set_picture_floating_align_v(Some(align), cx));
    }
    menus.connect("docx.picture.move-with-text", |this, _, cx| this.set_picture_move_with_text(true, cx));
    menus.connect("docx.picture.fix-position-on-page", |this, _, cx| this.set_picture_move_with_text(false, cx));
    menus.connect("docx.picture.edit-wrap-points", |this, _, cx| this.toggle_edit_wrap_points(cx));
    menus.connect("docx.picture.wrap-points.reset", |this, _, cx| { this.reset_picture_wrap_points(cx); });
    menus.connect("docx.picture.allow-overlap", |this, _, cx| this.toggle_picture_allow_overlap(cx));
    menus.connect("docx.picture.lock-anchor", |this, _, cx| this.toggle_picture_locked(cx));
    menus.connect("docx.picture.layout-in-cell", |this, _, cx| this.toggle_picture_layout_in_cell(cx));
    for &(id, _, action) in PICTURE_ZORDERS {
        menus.connect(id, move |this, _, cx| this.reorder_picture_zorder(action, cx));
    }
    menus.connect("docx.picture.align.left", |this, _, cx| this.set_picture_floating_align_h(Some("left"), cx));
    menus.connect("docx.picture.align.center", |this, _, cx| this.set_picture_floating_align_h(Some("center"), cx));
    menus.connect("docx.picture.align.right", |this, _, cx| this.set_picture_floating_align_h(Some("right"), cx));
    menus.connect("docx.picture.align.top", |this, _, cx| this.set_picture_floating_align_v(Some("top"), cx));
    menus.connect("docx.picture.align.middle", |this, _, cx| this.set_picture_floating_align_v(Some("center"), cx));
    menus.connect("docx.picture.align.bottom", |this, _, cx| this.set_picture_floating_align_v(Some("bottom"), cx));
    menus.connect("docx.picture.distribute.horizontally", |this, _, cx| this.distribute_pictures_horizontally(cx));
    menus.connect("docx.picture.distribute.vertically", |this, _, cx| this.distribute_pictures_vertically(cx));
    menus.connect("docx.picture.align-to.margin", |this, _, cx| this.set_picture_align_relative("margin", cx));
    menus.connect("docx.picture.alt-text", |this, window, cx| this.open_picture_alt_text(window, cx));
    menus.connect("docx.alt-text", |this, window, cx| this.open_picture_alt_text(window, cx));
    menus.connect("docx.picture.decorative", |this, _, cx| this.toggle_picture_decorative(cx));
    for &(id, recolor) in PICTURE_RECOLORS {
        menus.connect(id, move |this, _, cx| this.recolor_picture(recolor, cx));
    }
    for &(id, sat) in PICTURE_SATURATIONS {
        menus.connect(id, move |this, _, cx| this.set_picture_saturation(sat, cx));
    }
    for &(id, opac) in PICTURE_TRANSPARENCIES {
        menus.connect(id, move |this, _, cx| this.set_picture_opacity(opac, cx));
    }
    for &(id, soft) in PICTURE_SOFT_EDGES {
        menus.connect(id, move |this, _, cx| this.set_picture_soft_edge(soft, cx));
    }
    menus.connect("docx.selection-pane", |this, _, cx| this.open_selection_pane(cx));
    menus.connect("docx.navigation-pane", |this, _, cx| {
        this.outline_open = !this.outline_open;
        cx.notify();
    });
    menus.connect("docx.master-doc.panel", |this, window, cx| this.open_master_doc_panel(window, cx));
    menus.connect("docx.master-doc.create-from-heading", |this, _, cx| { this.create_subdocument_from_heading(cx); });
    menus.connect("docx.master-doc.toggle-expand", |this, _, cx| { this.toggle_expand_subdocuments(cx); });
    menus.connect("docx.master-doc.lock", |this, _, cx| { this.toggle_subdocument_lock_at_caret(cx); });
    menus.connect("docx.master-doc.unlink", |this, _, cx| { this.unlink_subdocument_at_caret(cx); });
    menus.connect("docx.master-doc.merge", |this, _, cx| { this.merge_adjacent_subdocuments(cx); });
    menus.connect("docx.master-doc.split", |this, _, cx| { this.split_subdocument_at_caret(cx); });
    if menus.find_mut("docx.navigation-pane").is_none() {
        menus.items_mut(MenuId::View).push(MenuItem::tr("docx.navigation-pane")
            .icon("toc")
            .checked(_editor.outline_open)
            .on(|this, _, cx| {
                this.outline_open = !this.outline_open;
                cx.notify();
            }).into());
    }
    menus.items_mut(MenuId::View).push(MenuItem::tr("docx.selection-pane")
        .icon("layers")
        .on(|this, _, cx| this.open_selection_pane(cx)).into());
    {
        let subs_count = _editor.subdocuments().len();
        let caret_sub = _editor.subdocument_at_caret();
        let master_doc_items: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.master-doc.panel")
                .icon("account_tree")
                .on(|this, window, cx| this.open_master_doc_panel(window, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("docx.master-doc.create-from-heading")
                .on(|this, _, cx| { this.create_subdocument_from_heading(cx); })
                .into(),
            MenuItem::tr("docx.master-doc.toggle-expand")
                .disabled(subs_count == 0)
                .on(|this, _, cx| { this.toggle_expand_subdocuments(cx); })
                .into(),
            MenuItem::tr("docx.master-doc.lock")
                .disabled(caret_sub.is_none())
                .checked(caret_sub.as_ref().is_some_and(|(_, s)| s.locked))
                .on(|this, _, cx| { this.toggle_subdocument_lock_at_caret(cx); })
                .into(),
            MenuItem::tr("docx.master-doc.unlink")
                .disabled(caret_sub.is_none())
                .on(|this, _, cx| { this.unlink_subdocument_at_caret(cx); })
                .into(),
            MenuItem::tr("docx.master-doc.merge")
                .disabled(subs_count < 2)
                .on(|this, _, cx| { this.merge_adjacent_subdocuments(cx); })
                .into(),
            MenuItem::tr("docx.master-doc.split")
                .disabled(caret_sub.is_none())
                .on(|this, _, cx| { this.split_subdocument_at_caret(cx); })
                .into(),
        ];
        menus.items_mut(MenuId::View).push(
            crate::shell::chrome::Submenu::tr("docx.master-doc", "account_tree", master_doc_items).into(),
        );
    }
    let marks_subitems: Vec<MenuEntry> = vec![
        MenuItem::tr("docx.view.marks.spaces")
            .checked(_editor.formatting_marks_options.show_spaces)
            .on(|this, _, cx| this.toggle_show_spaces(cx))
            .into(),
        MenuItem::tr("docx.view.marks.tabs")
            .checked(_editor.formatting_marks_options.show_tabs)
            .on(|this, _, cx| this.toggle_show_tabs(cx))
            .into(),
        MenuItem::tr("docx.view.marks.paragraph")
            .checked(_editor.formatting_marks_options.show_paragraph_marks)
            .on(|this, _, cx| this.toggle_show_paragraph_marks(cx))
            .into(),
        MenuItem::tr("docx.view.marks.line-breaks")
            .checked(_editor.formatting_marks_options.show_line_breaks)
            .on(|this, _, cx| this.toggle_show_line_breaks(cx))
            .into(),
        MenuItem::tr("docx.view.marks.hidden")
            .checked(_editor.formatting_marks_options.show_hidden_text)
            .on(|this, _, cx| this.toggle_show_hidden_text(cx))
            .into(),
    ];
    menus.items_mut(MenuId::View).push(crate::shell::chrome::Submenu::tr("docx.view.marks-options", "notes", marks_subitems).into());
    menus.items_mut(MenuId::View).push(MenuItem::tr("docx.view.dark-page").icon("dark_mode").checked(_editor.is_dark_page()).on(|this, _, cx| this.toggle_dark_page(cx)).into());
    menus.items_mut(MenuId::View).push(
        MenuItem::tr("docx.view.immersive-reader")
            .icon("menu_book")
            .checked(_editor.is_immersive_reader_enabled())
            .on(|this, window, cx| this.toggle_immersive_reader(window, cx))
            .into(),
    );
    menus.items_mut(MenuId::View).push(
        MenuItem::tr("docx.view.mini-toolbar")
            .checked(_editor.is_mini_toolbar_enabled())
            .on(|this, _, cx| this.toggle_mini_toolbar(cx))
            .into(),
    );
    {
        use crate::docx::screentip::ScreenTipStyle;
        let style = _editor.screentip_style();
        let screentip_subitems: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.view.screentips.show-descriptions")
                .checked(style == ScreenTipStyle::ShowFeatureDescriptions)
                .on(|this, _, cx| this.set_screentip_style(ScreenTipStyle::ShowFeatureDescriptions, cx))
                .into(),
            MenuItem::tr("docx.view.screentips.dont-show-descriptions")
                .checked(style == ScreenTipStyle::DontShowFeatureDescriptions)
                .on(|this, _, cx| this.set_screentip_style(ScreenTipStyle::DontShowFeatureDescriptions, cx))
                .into(),
            MenuItem::tr("docx.view.screentips.dont-show")
                .checked(style == ScreenTipStyle::DontShowScreenTips)
                .on(|this, _, cx| this.set_screentip_style(ScreenTipStyle::DontShowScreenTips, cx))
                .into(),
        ];
        menus.items_mut(MenuId::View).push(
            crate::shell::chrome::Submenu::tr("docx.view.screentips", "help_outline", screentip_subitems).into(),
        );
    }
    menus.items_mut(MenuId::Format).push(MenuItem::tr("docx.selection-pane")
        .icon("layers")
        .on(|this, _, cx| this.open_selection_pane(cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("docx.picture.size")
        .disabled(_editor.picture_at_caret().is_none()).on(|this,window,cx|this.open_picture_edit(window,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("docx.picture.alt-text")
        .disabled(_editor.picture_at_caret().is_none()).on(|this,window,cx|this.open_picture_alt_text(window,cx)).into());
    let has_pic = _editor.picture_at_caret().is_some();
    let pic_dec = _editor.picture_decorative_at_caret().unwrap_or(false);
    menus.items_mut(MenuId::Format).push(MenuItem::tr("docx.picture.decorative")
        .disabled(!has_pic)
        .checked(has_pic && pic_dec)
        .on(|this, _, cx| this.toggle_picture_decorative(cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("docx.picture.replace")
        .icon("swap_horiz")
        .disabled(!has_pic)
        .on(|this,window,cx|this.doc_pick_replacement_picture(window,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("docx.picture.reset")
        .icon("reset_image")
        .disabled(!has_pic)
        .on(|this, _, cx| this.reset_picture(cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("docx.picture.compress")
        .icon("image")
        .on(|this, window, cx| this.open_docx_compress_panel(window, cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("docx.picture.ratio")
        .disabled(!_editor.doc_state().is_some_and(|s|s.picture_edit.is_some()))
        .checked(_editor.doc_state().is_some_and(|s|s.picture_edit.as_ref().is_some_and(|e|e.keep_ratio)))
        .on(|this,_,cx|this.toggle_picture_ratio(cx)).into());
    let pic_stroke = _editor.picture_stroke_at_caret();
    let border_none_item = MenuItem::tr("docx.picture.border.none")
        .disabled(!has_pic)
        .checked(has_pic && pic_stroke.is_none())
        .on(|this, _, cx| this.clear_picture_border(cx));
    let width_items: Vec<MenuEntry> = super::picture_panel::PICTURE_BORDER_WIDTHS.iter().map(|&(_label, w)| {
        let menu_id = match w {
            6_350 => "docx.picture.border.w050",
            12_700 => "docx.picture.border.w100",
            19_050 => "docx.picture.border.w150",
            28_575 => "docx.picture.border.w225",
            38_100 => "docx.picture.border.w300",
            57_150 => "docx.picture.border.w450",
            _ => "docx.picture.border.w600",
        };
        MenuItem::tr(menu_id)
            .disabled(!has_pic)
            .checked(has_pic && pic_stroke.as_ref().is_some_and(|s| s.width == w))
            .on(move |this, _, cx| this.set_picture_border_width(w, cx))
            .into()
    }).collect();
    let color_items: Vec<MenuEntry> = {
        use crate::shell::color_palette::{Palette, PickColor, Swatch, theme_rows};
        let pick: PickColor = std::rc::Rc::new(|this, c, _, cx| {
            if let Some(rgb) = c {
                this.set_picture_border_color(ibis_doc_model::Color::opaque(rgb), cx);
            } else {
                this.clear_picture_border(cx);
            }
        });
        let current = pic_stroke.as_ref().map(|s| s.color.rgb);
        let mut theme = theme_rows(&THEME_10, &pick, current);
        if !has_pic {
            for s in theme.iter_mut().flatten() {
                s.item = s.item.clone().disabled(true);
            }
        }
        let standard: Vec<Swatch> = super::picture_panel::PICTURE_BORDER_COLORS
            .iter()
            .map(|&(label, c)| {
                let menu_id = match c {
                    0x000000 => "docx.picture.border.color.black",
                    0x757575 => "docx.picture.border.color.gray",
                    0xD32F2F => "docx.picture.border.color.red",
                    0x1B65D4 => "docx.picture.border.color.blue",
                    0x2E7D32 => "docx.picture.border.color.green",
                    _ => "docx.picture.border.color.orange",
                };
                Swatch {
                    rgb: Rgb(c),
                    item: MenuItem::new(menu_id, crate::shell::i18n::text(label))
                        .disabled(!has_pic)
                        .checked(has_pic && pic_stroke.as_ref().is_some_and(|s| s.color.rgb.0 == c))
                        .on(move |this, _, cx| this.set_picture_border_color(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(c)), cx)),
                }
            })
            .collect();
        vec![MenuEntry::Palette(Palette {
            none: None,
            theme,
            standard,
            pick: Some(pick),
            more: None,
            button: None, current:None,
        })]
    };
    let dash_items: Vec<MenuEntry> = super::picture_panel::PICTURE_BORDER_DASHES.iter().map(|&(_label, d)| {
        let menu_id = match d {
            ibis_doc_model::LineDash::Solid => "docx.picture.border.dash.solid",
            ibis_doc_model::LineDash::Dot => "docx.picture.border.dash.dot",
            ibis_doc_model::LineDash::Dash => "docx.picture.border.dash.dash",
            ibis_doc_model::LineDash::LgDash => "docx.picture.border.dash.lgdash",
            _ => "docx.picture.border.dash.dashdot",
        };
        MenuItem::tr(menu_id)
            .disabled(!has_pic)
            .checked(has_pic && pic_stroke.as_ref().is_some_and(|s| s.dash == d))
            .on(move |this, _, cx| this.set_picture_border_dash(d, cx))
            .into()
    }).collect();
    let border_menu = vec![
        border_none_item.into(),
        MenuEntry::Separator,
        crate::shell::chrome::Submenu::tr("docx.picture.border.width", "line_weight", width_items).into(),
        crate::shell::chrome::Submenu::tr("docx.picture.border.color", "palette", color_items).into(),
        crate::shell::chrome::Submenu::tr("docx.picture.border.dash", "border_style", dash_items).into(),
    ];
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.picture.border", "border_all", border_menu).into());

    let geom_preset_now = _editor.picture_geometry_preset_at_caret();
    let crop_to_shape_items: Vec<MenuEntry> = super::picture_panel::PICTURE_CROP_SHAPES.iter().map(|&(id, preset)| {
        let is_selected = match preset {
            None => geom_preset_now.is_none(),
            Some(p) => geom_preset_now == Some(p),
        };
        MenuItem::tr(id)
            .disabled(!has_pic)
            .checked(has_pic && is_selected)
            .on(move |this, _, cx| this.crop_picture_to_shape(preset, cx))
            .into()
    }).collect();
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.picture.crop-to-shape", "crop", crop_to_shape_items).into());

    let look_now = _editor.picture_look_at_caret();
    let duotone_now = look_now.as_ref().and_then(|l| l.duotone);
    let lum_now = look_now.as_ref().and_then(|l| l.luminance);
    let bilevel_now = look_now.as_ref().and_then(|l| l.effects.bilevel);
    let sat_now = look_now.as_ref().and_then(|l| l.effects.saturation);
    let opac_now = look_now.as_ref().and_then(|l| l.effects.opacity);
    let soft_now = look_now.as_ref().and_then(|l| l.soft_edge);

    let recolor_items: Vec<MenuEntry> = PICTURE_RECOLORS.iter().map(|&(id, recolor)| {
        let is_selected = match recolor {
            None => duotone_now.is_none() && bilevel_now.is_none() && lum_now != Some((70_000, -70_000)),
            Some(super::picture_panel::PictureRecolor::Grayscale) => duotone_now == Some((Rgb(0x000000), Rgb(0xFFFFFF))),
            Some(super::picture_panel::PictureRecolor::Sepia) => duotone_now == Some((Rgb(0x3F2300), Rgb(0xFFF3D6))),
            Some(super::picture_panel::PictureRecolor::Washout) => lum_now == Some((70_000, -70_000)),
            Some(super::picture_panel::PictureRecolor::BlackAndWhite50) => bilevel_now == Some(50_000),
            Some(super::picture_panel::PictureRecolor::Blue) => duotone_now == Some((Rgb(0x002060), Rgb(0xD9E1F2))),
            Some(super::picture_panel::PictureRecolor::Green) => duotone_now == Some((Rgb(0x004010), Rgb(0xE2EFDA))),
            Some(super::picture_panel::PictureRecolor::Red) => duotone_now == Some((Rgb(0x600000), Rgb(0xFCE4D6))),
            Some(super::picture_panel::PictureRecolor::Orange) => duotone_now == Some((Rgb(0x803000), Rgb(0xFFE6CC))),
            Some(super::picture_panel::PictureRecolor::Purple) => duotone_now == Some((Rgb(0x400060), Rgb(0xF2E6FF))),
        };
        MenuItem::tr(id)
            .disabled(!has_pic)
            .checked(has_pic && is_selected)
            .on(move |this, _, cx| this.recolor_picture(recolor, cx))
            .into()
    }).collect();
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.picture.recolor", "palette", recolor_items).into());

    let sat_items: Vec<MenuEntry> = PICTURE_SATURATIONS.iter().map(|&(id, sat)| {
        let is_selected = match sat {
            None => sat_now.is_none() || sat_now == Some(100_000),
            Some(s) => sat_now == Some(s),
        };
        MenuItem::tr(id)
            .disabled(!has_pic)
            .checked(has_pic && is_selected)
            .on(move |this, _, cx| this.set_picture_saturation(sat, cx))
            .into()
    }).collect();
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.picture.saturation", "palette", sat_items).into());

    let trans_items: Vec<MenuEntry> = PICTURE_TRANSPARENCIES.iter().map(|&(id, opac)| {
        let is_selected = match opac {
            None => opac_now.is_none(),
            Some(o) => opac_now == Some(o),
        };
        MenuItem::tr(id)
            .disabled(!has_pic)
            .checked(has_pic && is_selected)
            .on(move |this, _, cx| this.set_picture_opacity(opac, cx))
            .into()
    }).collect();
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.picture.transparency", "opacity", trans_items).into());

    let soft_items: Vec<MenuEntry> = PICTURE_SOFT_EDGES.iter().map(|&(id, soft)| {
        let is_selected = soft_now == soft;
        MenuItem::tr(id)
            .disabled(!has_pic)
            .checked(has_pic && is_selected)
            .on(move |this, _, cx| this.set_picture_soft_edge(soft, cx))
            .into()
    }).collect();
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.picture.soft-edge", "blur_on", soft_items).into());
    let cur_wrap = _editor.picture_wrap_at_caret();
    let cur_anchor = _editor.picture_floating_at_caret();
    let cur_dist = cur_anchor.as_ref().map(|a| [a.dist_l, a.dist_r, a.dist_t, a.dist_b]);
    let cur_align_h = cur_anchor.as_ref().and_then(|a| a.align_h.as_deref());
    let is_floating_pic = cur_wrap.is_some_and(|w| w != ibis_doc_model::PictureWrapType::Inline);

    let mut wrap_items: Vec<MenuEntry> = PICTURE_WRAPS.iter().map(|&(id, _, wrap)| {
        MenuItem::tr(id)
            .disabled(!has_pic)
            .checked(has_pic && cur_wrap == Some(wrap))
            .on(move |this, _, cx| this.set_picture_wrap(wrap, cx))
            .into()
    }).collect();
    wrap_items.push(MenuEntry::Separator);
    wrap_items.push(
        MenuItem::tr("docx.picture.edit-wrap-points")
            .disabled(!has_pic)
            .checked(has_pic && _editor.is_editing_wrap_points())
            .on(|this, _, cx| this.toggle_edit_wrap_points(cx))
            .into(),
    );
    wrap_items.push(
        MenuItem::tr("docx.picture.move-with-text")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && is_floating_pic && cur_anchor.as_ref().is_some_and(|a| a.moves_with_text()))
            .on(|this, _, cx| this.set_picture_move_with_text(true, cx))
            .into(),
    );
    wrap_items.push(
        MenuItem::tr("docx.picture.fix-position-on-page")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && is_floating_pic && cur_anchor.as_ref().is_some_and(|a| !a.moves_with_text()))
            .on(|this, _, cx| this.set_picture_move_with_text(false, cx))
            .into(),
    );

    let dist_items: Vec<MenuEntry> = PICTURE_FLOAT_DISTANCES.iter().map(|&(id, _, dist)| {
        MenuItem::tr(id)
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && cur_dist == Some(dist))
            .on(move |this, _, cx| this.set_picture_floating_distance(dist, cx))
            .into()
    }).collect();

    let align_items: Vec<MenuEntry> = PICTURE_ALIGN_H.iter().map(|&(id, _, align)| {
        MenuItem::tr(id)
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && cur_align_h == Some(align))
            .on(move |this, _, cx| this.set_picture_floating_align_h(Some(align), cx))
            .into()
    }).collect();

    let cur_align_v = cur_anchor.as_ref().and_then(|a| a.align_v.as_deref());
    let align_v_items: Vec<MenuEntry> = PICTURE_ALIGN_V.iter().map(|&(id, _, align)| {
        MenuItem::tr(id)
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && cur_align_v == Some(align))
            .on(move |this, _, cx| this.set_picture_floating_align_v(Some(align), cx))
            .into()
    }).collect();

    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.picture.wrap", "wrap_text", wrap_items).into());
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.picture.distance", "space_bar", dist_items).into());
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.picture.align-h", "format_align_center", align_items).into());
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.picture.align-v", "vertical_align_center", align_v_items).into());
    menus.items_mut(MenuId::Format).push(
        MenuItem::tr("docx.picture.move-with-text")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && is_floating_pic && cur_anchor.as_ref().is_some_and(|a| a.moves_with_text()))
            .on(|this, _, cx| this.set_picture_move_with_text(true, cx))
            .into(),
    );
    menus.items_mut(MenuId::Format).push(
        MenuItem::tr("docx.picture.fix-position-on-page")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && is_floating_pic && cur_anchor.as_ref().is_some_and(|a| !a.moves_with_text()))
            .on(|this, _, cx| this.set_picture_move_with_text(false, cx))
            .into(),
    );
    menus.items_mut(MenuId::Format).push(
        MenuItem::tr("docx.picture.allow-overlap")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && cur_anchor.as_ref().is_some_and(|a| a.allow_overlap))
            .on(|this, _, cx| this.toggle_picture_allow_overlap(cx))
            .into(),
    );
    menus.items_mut(MenuId::Format).push(
        MenuItem::tr("docx.picture.lock-anchor")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && cur_anchor.as_ref().is_some_and(|a| a.locked))
            .on(|this, _, cx| this.toggle_picture_locked(cx))
            .into(),
    );
    menus.items_mut(MenuId::Format).push(
        MenuItem::tr("docx.picture.layout-in-cell")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && cur_anchor.as_ref().is_some_and(|a| a.layout_in_cell))
            .on(|this, _, cx| this.toggle_picture_layout_in_cell(cx))
            .into(),
    );
    let zorder_items: Vec<MenuEntry> = PICTURE_ZORDERS.iter().map(|&(id, _, action)| {
        MenuItem::tr(id)
            .disabled(!has_pic || !is_floating_pic)
            .on(move |this, _, cx| this.reorder_picture_zorder(action, cx))
            .into()
    }).collect();
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.picture.zorder", "flip_to_front", zorder_items).into());

    let align_distribute_items: Vec<MenuEntry> = vec![
        MenuItem::tr("docx.picture.align.left")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && is_floating_pic && cur_align_h == Some("left"))
            .on(|this, _, cx| this.set_picture_floating_align_h(Some("left"), cx))
            .into(),
        MenuItem::tr("docx.picture.align.center")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && is_floating_pic && cur_align_h == Some("center"))
            .on(|this, _, cx| this.set_picture_floating_align_h(Some("center"), cx))
            .into(),
        MenuItem::tr("docx.picture.align.right")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && is_floating_pic && cur_align_h == Some("right"))
            .on(|this, _, cx| this.set_picture_floating_align_h(Some("right"), cx))
            .into(),
        MenuItem::tr("docx.picture.align.top")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && is_floating_pic && cur_align_v == Some("top"))
            .on(|this, _, cx| this.set_picture_floating_align_v(Some("top"), cx))
            .into(),
        MenuItem::tr("docx.picture.align.middle")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && is_floating_pic && cur_align_v == Some("center"))
            .on(|this, _, cx| this.set_picture_floating_align_v(Some("center"), cx))
            .into(),
        MenuItem::tr("docx.picture.align.bottom")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && is_floating_pic && cur_align_v == Some("bottom"))
            .on(|this, _, cx| this.set_picture_floating_align_v(Some("bottom"), cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("docx.picture.distribute.horizontally")
            .disabled(!has_pic || !is_floating_pic)
            .on(|this, _, cx| this.distribute_pictures_horizontally(cx))
            .into(),
        MenuItem::tr("docx.picture.distribute.vertically")
            .disabled(!has_pic || !is_floating_pic)
            .on(|this, _, cx| this.distribute_pictures_vertically(cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("docx.picture.align-to.margin")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && is_floating_pic && cur_anchor.as_ref().is_some_and(|a| a.relative_h == "margin"))
            .on(|this, _, cx| this.set_picture_align_relative("margin", cx))
            .into(),
        MenuItem::tr("docx.picture.align-to.page")
            .disabled(!has_pic || !is_floating_pic)
            .checked(has_pic && is_floating_pic && cur_anchor.as_ref().is_some_and(|a| a.relative_h == "page"))
            .on(|this, _, cx| this.set_picture_align_relative("page", cx))
            .into(),
    ];
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.picture.align-distribute", "align_horizontal_center", align_distribute_items).into());

    let cur_shape_anchor = _editor.caret_shape_anchor().unwrap_or(ibis_doc_model::VerticalAnchor::Top);
    let shape_has_text = _editor.text_box_at_caret().is_some_and(|i| i.has_text);
    let shape_align_items = vec![
        MenuItem::tr("docx.shape.align-text.top")
            .icon("vertical_align_top")
            .disabled(!shape_has_text)
            .checked(shape_has_text && cur_shape_anchor == ibis_doc_model::VerticalAnchor::Top)
            .on(|this, _, cx| this.set_shape_anchor(ibis_doc_model::VerticalAnchor::Top, cx))
            .into(),
        MenuItem::tr("docx.shape.align-text.middle")
            .icon("vertical_align_center")
            .disabled(!shape_has_text)
            .checked(shape_has_text && cur_shape_anchor == ibis_doc_model::VerticalAnchor::Middle)
            .on(|this, _, cx| this.set_shape_anchor(ibis_doc_model::VerticalAnchor::Middle, cx))
            .into(),
        MenuItem::tr("docx.shape.align-text.bottom")
            .icon("vertical_align_bottom")
            .disabled(!shape_has_text)
            .checked(shape_has_text && cur_shape_anchor == ibis_doc_model::VerticalAnchor::Bottom)
            .on(|this, _, cx| this.set_shape_anchor(ibis_doc_model::VerticalAnchor::Bottom, cx))
            .into(),
    ];
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.shape.align-text", "vertical_align_center", shape_align_items).into());

    super::shape_picture_tools::register_menus(menus, _editor);

    let editing = _editor.ordered_selection().is_some();
    let text_format = _editor.current_format().map(|(f, _)| f);
    let text_style = _editor.current_format().map(|(_, s)| s);
    for entry in menus.items_mut(MenuId::Format) {
        if let MenuEntry::Submenu(sub) = entry && sub.id == "format.text" {
            let types = underline_type_items(text_style.as_ref().map(|s| (s.underline, s.underline_style)), editing);
            sub.items.push(crate::shell::chrome::Submenu::tr("docx.underline.types", "format_underlined", types).into());
            let colors: Vec<MenuEntry> = {
                use crate::shell::color_palette::{Palette, PickColor, Swatch, theme_rows};
                let pick: PickColor = std::rc::Rc::new(|this, c, _, cx| this.set_underline_color(c, cx));
                let cur_u = text_style.as_ref().and_then(|s| s.underline_color);
                let mut theme = theme_rows(&THEME_10, &pick, cur_u);
                if !editing {
                    for s in theme.iter_mut().flatten() {
                        s.item = s.item.clone().disabled(true);
                    }
                }
                let standard: Vec<Swatch> = [
                    ("docx.underline.black", "_shell.black", 0u32),
                    ("docx.underline.red", "_shell.red", 0xff0000),
                    ("docx.underline.blue", "_shell.blue", 0x0000ff),
                    ("docx.underline.green", "_shell.green", 0x008000),
                ]
                .into_iter()
                .map(|(id, label, value)| Swatch {
                    rgb: Rgb(value),
                    item: MenuItem::new(id, crate::shell::i18n::text(label))
                        .disabled(!editing)
                        .checked(text_style.as_ref().is_some_and(|s| s.underline_color == Some(Rgb(value))))
                        .on(move |this, _, cx| this.set_underline_color(Some(Rgb(value)), cx)),
                })
                .collect();
                let none = MenuItem::tr("docx.underline.auto")
                    .disabled(!editing)
                    .checked(text_style.as_ref().is_some_and(|s| s.underline_color.is_none()))
                    .on(|this, _, cx| this.set_underline_color(None, cx));
                let more = MenuItem::tr("docx.underline.custom")
                    .disabled(!editing)
                    .on(|this, window, cx| this.open_color_edit(true, window, cx));
                vec![MenuEntry::Palette(Palette {
                    none: Some(none),
                    theme,
                    standard,
                    pick: Some(pick),
                    more: Some(more),
                    button: None, current:None,
                })]
            };
            sub.items.push(crate::shell::chrome::Submenu::tr("docx.underline.colors", "format_color_text", colors).into());
            let text_colors: Vec<MenuEntry> = {
                use crate::shell::color_palette::{Palette, PickColor, Swatch, THEME_TINTS};
                let pick: PickColor = std::rc::Rc::new(|this, c, _, cx| this.set_text_color(c.unwrap_or(Rgb(0)), cx));
                let cur_theme_color = text_format.as_ref().and_then(|f| f.theme_color.clone());
                let current = text_style.as_ref().map(|s| s.color).filter(|_| cur_theme_color.is_none());
                let order = [
                    "docx.theme-color.bg1",
                    "docx.theme-color.text1",
                    "docx.theme-color.bg2",
                    "docx.theme-color.text2",
                    "docx.theme-color.accent1",
                    "docx.theme-color.accent2",
                    "docx.theme-color.accent3",
                    "docx.theme-color.accent4",
                    "docx.theme-color.accent5",
                    "docx.theme-color.accent6",
                ];
                let base: Vec<&(&str, &str, u32, &str, Option<&str>, Option<&str>)> =
                    order.iter().filter_map(|id| THEME_COLORS.iter().find(|t| t.0 == *id)).collect();
                let mut theme: Vec<Vec<Swatch>> = Vec::new();
                theme.push(
                    base.iter()
                        .map(|&&(item_id, label, rgb_val, theme_name, tint, shade)| {
                            let checked = cur_theme_color
                                .as_ref()
                                .is_some_and(|tc| tc.name == theme_name && tc.tint.as_deref() == tint && tc.shade.as_deref() == shade);
                            let tc = ibis_doc_model::FlowThemeColor::new(theme_name, tint, shade);
                            Swatch {
                                rgb: Rgb(rgb_val),
                                item: MenuItem::new(item_id, crate::shell::i18n::text(label))
                                    .disabled(!editing)
                                    .checked(checked)
                                    .on(move |this, _, cx| this.set_theme_color(Rgb(rgb_val), tc.clone(), cx)),
                            }
                        })
                        .collect(),
                );
                if base.len() == 10 {
                    for row in 1..6 {
                        theme.push(
                            (0..10)
                                .map(|i| {
                                    let rgb = ibis_pptx::tinted(Rgb(base[i].2), THEME_TINTS[i][row]);
                                    let mut sw = Swatch::new(rgb, format!("#{:06X}", rgb.0), current == Some(rgb), &pick);
                                    sw.item = sw.item.disabled(!editing);
                                    sw
                                })
                                .collect(),
                        );
                    }
                }
                let standard: Vec<Swatch> = COLORS[1..]
                    .iter()
                    .map(|&(id, label, val)| {
                        let is_checked = text_style.as_ref().is_some_and(|s| s.color == Rgb(val)) && cur_theme_color.is_none();
                        Swatch {
                            rgb: Rgb(val),
                            item: MenuItem::new(id, crate::shell::i18n::text(label))
                                .checked(is_checked)
                                .disabled(!editing)
                                .on(move |this, _, cx| this.set_text_color(Rgb(val), cx)),
                        }
                    })
                    .collect();
                let none = MenuItem::new("docx.format.text.color.black", crate::shell::i18n::text("docx.toolbar.automatic-black"))
                    .checked(text_style.as_ref().is_some_and(|s| s.color == Rgb(0)) && cur_theme_color.is_none())
                    .disabled(!editing)
                    .on(|this, _, cx| this.set_text_color(Rgb(0), cx));
                let more = MenuItem::tr("docx.format.text.color.custom")
                    .disabled(!editing)
                    .on(|this, window, cx| this.open_color_edit(false, window, cx));
                vec![MenuEntry::Palette(Palette {
                    none: Some(none),
                    theme,
                    standard,
                    pick: Some(pick),
                    more: Some(more),
                    button: None, current:None,
                })]
            };
            sub.items.push(crate::shell::chrome::Submenu::new("docx.format.text.colors", "format_color_text", crate::shell::i18n::text("docx.toolbar.automatic"), text_colors).into());
            sub.items.push(MenuItem::tr("docx.text.strike").checked(text_style.as_ref().is_some_and(|s| s.strike)).disabled(!editing)
                .on(|this, _, cx| this.toggle_strike(cx)).into());
            sub.items.push(MenuItem::tr("docx.text.double-strike").checked(text_style.as_ref().is_some_and(|s| s.double_strike)).disabled(!editing)
                .on(|this, _, cx| this.toggle_double_strike(cx)).into());
            sub.items.push(MenuItem::tr("docx.text.caps").checked(text_style.as_ref().is_some_and(|s| s.caps)).disabled(!editing)
                .on(|this, _, cx| this.toggle_caps(cx)).into());
            sub.items.push(MenuItem::tr("docx.text.small-caps").checked(text_style.as_ref().is_some_and(|s| s.small_caps)).disabled(!editing)
                .on(|this, _, cx| this.toggle_small_caps(cx)).into());
            sub.items.push(MenuItem::tr("docx.text.hidden").checked(text_style.as_ref().is_some_and(|s| s.hidden))
                .shortcut(if cfg!(target_os = "macos") { "⌘⇧X" } else { "Ctrl+Shift+H" })
                .disabled(!editing)
                .on(|this, _, cx| this.toggle_hidden(cx)).into());
            sub.items.push(MenuItem::tr("docx.text.outline").checked(text_style.as_ref().is_some_and(|s| s.outline)).disabled(!editing)
                .on(|this, _, cx| this.toggle_outline(cx)).into());
            sub.items.push(MenuItem::tr("docx.text.shadow").checked(text_style.as_ref().is_some_and(|s| s.shadow)).disabled(!editing)
                .on(|this, _, cx| this.toggle_shadow(cx)).into());
            sub.items.push(MenuItem::tr("docx.text.emboss").checked(text_style.as_ref().is_some_and(|s| s.emboss)).disabled(!editing)
                .on(|this, _, cx| this.toggle_emboss(cx)).into());
            sub.items.push(MenuItem::tr("docx.text.imprint").checked(text_style.as_ref().is_some_and(|s| s.imprint)).disabled(!editing)
                .on(|this, _, cx| this.toggle_imprint(cx)).into());
            for (id, label, position) in [("docx.text.superscript", "docx.chrome.superscript", ibis_doc_model::VerticalPosition::Superscript),
                ("docx.text.subscript", "docx.chrome.subscript", ibis_doc_model::VerticalPosition::Subscript)] {
                sub.items.push(MenuItem::new(id, crate::shell::i18n::text(label)).checked(text_style.as_ref().is_some_and(|s| s.position == position)).disabled(!editing)
                    .on(move |this, _, cx| this.toggle_text_position(position, cx)).into());
            }
            sub.items.push(MenuItem::tr("docx.text.baseline").checked(text_style.as_ref().is_some_and(|s| s.position == ibis_doc_model::VerticalPosition::Baseline)).disabled(!editing)
                .on(|this, _, cx| this.apply_run_change(ibis_doc_model::RunChange::Position(ibis_doc_model::VerticalPosition::Baseline), cx)).into());
            sub.items.push(MenuItem::new("docx.text.ruby", crate::shell::i18n::text("docx.toolbar.phonetic-guide")).disabled(!editing).on(|this, window, cx| this.open_ruby(window, cx)).into());
            sub.items.push(MenuItem::new("docx.text.fit-text", crate::shell::i18n::text("docx.fit-text.title")).disabled(!editing).on(|e,w,cx|e.open_fit_text(w,cx)).into());
            // 文字の囲み線 (Word's ホーム > 文字の囲み線).
            sub.items.push(
                MenuItem::tr("docx.text.border")
                    .checked(text_style.as_ref().is_some_and(|s| s.border.is_some()))
                    .disabled(!editing)
                    .on(|this, _, cx| this.toggle_character_border(cx))
                    .into(),
            );
            // 文字の網かけ (Word's ホーム > 文字の網かけ).
            sub.items.push(
                MenuItem::tr("docx.text.shading")
                    .checked(text_style.as_ref().is_some_and(|s| s.highlight == Some(ibis_doc_model::Rgb(0xd9d9d9))))
                    .disabled(!editing)
                    .on(|this, _, cx| this.toggle_character_shading(cx))
                    .into(),
            );
            // 文字種の変換 (Word's ホーム > 文字種の変換).
            {
                use ibis_doc_model::CaseConversion;
                let cases: Vec<MenuEntry> = vec![
                    MenuItem::tr("docx.text.case.sentence")
                        .disabled(!editing)
                        .on(|this, _, cx| this.convert_case(CaseConversion::Sentence, cx))
                        .into(),
                    MenuItem::tr("docx.text.case.lower")
                        .disabled(!editing)
                        .on(|this, _, cx| this.convert_case(CaseConversion::Lower, cx))
                        .into(),
                    MenuItem::tr("docx.text.case.upper")
                        .disabled(!editing)
                        .on(|this, _, cx| this.convert_case(CaseConversion::Upper, cx))
                        .into(),
                    MenuItem::tr("docx.text.case.title")
                        .disabled(!editing)
                        .on(|this, _, cx| this.convert_case(CaseConversion::Title, cx))
                        .into(),
                    MenuItem::tr("docx.text.case.toggle")
                        .disabled(!editing)
                        .on(|this, _, cx| this.convert_case(CaseConversion::Toggle, cx))
                        .into(),
                    MenuEntry::Separator,
                    MenuItem::tr("docx.text.case.half-width")
                        .disabled(!editing)
                        .on(|this, _, cx| this.convert_case(CaseConversion::HalfWidth, cx))
                        .into(),
                    MenuItem::tr("docx.text.case.full-width")
                        .disabled(!editing)
                        .on(|this, _, cx| this.convert_case(CaseConversion::FullWidth, cx))
                        .into(),
                    MenuEntry::Separator,
                    MenuItem::tr("docx.text.case.katakana")
                        .disabled(!editing)
                        .on(|this, _, cx| this.convert_case(CaseConversion::Katakana, cx))
                        .into(),
                    MenuItem::tr("docx.text.case.hiragana")
                        .disabled(!editing)
                        .on(|this, _, cx| this.convert_case(CaseConversion::Hiragana, cx))
                        .into(),
                ];
                sub.items.push(crate::shell::chrome::Submenu::tr("docx.text.case", "match_case", cases).into());
            }
            // 囲い文字 (Word's ホーム > 囲い文字).
            {
                let mut enclosures: Vec<MenuEntry> = vec![
                    MenuItem::tr("docx.text.enclose.none").disabled(!editing).on(|this, _, cx| this.clear_enclosed_character(cx)).into(),
                ];
                enclosures.extend(
                    [("docx.text.enclose.circle", '○'), ("docx.text.enclose.square", '□'), ("docx.text.enclose.triangle", '△'), ("docx.text.enclose.diamond", '◇')]
                        .into_iter()
                        .map(|(id, c)| MenuItem::new(id, c.to_string()).disabled(!editing).on(move |this, _, cx| this.enclose_character(c, cx)).into())
                );
                sub.items.push(crate::shell::chrome::Submenu::new("docx.text.enclose", "text_fields", crate::shell::i18n::text("docx.toolbar.enclose-characters"), enclosures).into());
            }
            // 組み文字 (Word's 拡張書式 > 組み文字).
            {
                use ibis_doc_model::Combine;
                let combines: Vec<MenuEntry> = [
                    ("docx.text.combine.none", "docx.toolbar.none-2", None),
                    ("docx.text.combine.plain", "docx.toolbar.no-brackets", Some(Combine::Plain)),
                    ("docx.text.combine.round", "( )", Some(Combine::Round)),
                    ("docx.text.combine.square", "[ ]", Some(Combine::Square)),
                    ("docx.text.combine.angle", "< >", Some(Combine::Angle)),
                    ("docx.text.combine.curly", "{ }", Some(Combine::Curly)),
                ]
                .into_iter()
                .map(|(id, label, c)| {
                    MenuItem::new(id, crate::shell::i18n::text(label))
                        .checked(text_style.as_ref().is_some_and(|s| s.combine == c))
                        .disabled(!editing)
                        .on(move |this, _, cx| this.apply_run_change(ibis_doc_model::RunChange::Combine(c), cx))
                        .into()
                })
                .collect();
                sub.items.push(crate::shell::chrome::Submenu::new("docx.text.combine", "text_fields", crate::shell::i18n::text("docx.toolbar.combine-characters"), combines).into());
            }
            // 縦中横 (Word's 拡張書式 > 縦中横).
            {
                let on = text_style.as_ref().is_some_and(|s| s.tate_chu_yoko);
                sub.items.push(
                    MenuItem::new("docx.text.tate-chu-yoko", crate::shell::i18n::text("docx.toolbar.horizontal-vertical"))
                        .checked(on)
                        .disabled(!editing)
                        .on(move |this, _, cx| this.apply_run_change(ibis_doc_model::RunChange::TateChuYoko(!on), cx))
                        .into(),
                );
            }
            // 傍点 (Word's フォント > 傍点).
            {
                use ibis_doc_model::EmphasisMark;
                let marks: Vec<MenuEntry> = [("docx.text.emphasis.none", "docx.toolbar.none", EmphasisMark::None), ("docx.text.emphasis.dot", "docx.toolbar.dot", EmphasisMark::Dot), ("docx.text.emphasis.comma", "docx.toolbar.comma", EmphasisMark::Comma), ("docx.text.emphasis.circle", "docx.toolbar.circle", EmphasisMark::Circle), ("docx.text.emphasis.under-dot", "docx.toolbar.under-dot", EmphasisMark::UnderDot)]
                    .into_iter()
                    .map(|(id, label, mark)| {
                        MenuItem::new(id, crate::shell::i18n::text(label))
                            .checked(text_style.as_ref().is_some_and(|s| s.emphasis == mark))
                            .disabled(!editing)
                            .on(move |this, _, cx| this.apply_run_change(ibis_doc_model::RunChange::Emphasis(mark), cx))
                            .into()
                    })
                    .collect();
                sub.items.push(crate::shell::chrome::Submenu::new("docx.text.emphasis", "text_fields", crate::shell::i18n::text("docx.toolbar.emphasis-mark"), marks).into());
            }
            sub.items.push(MenuEntry::Separator);

            let scales = [
                ("docx.text.scale-200", "200%", 200),
                ("docx.text.scale-150", "150%", 150),
                ("docx.text.scale-100", "docx.toolbar.100-percent-normal", 100),
                ("docx.text.scale-90", "90%", 90),
                ("docx.text.scale-80", "80%", 80),
                ("docx.text.scale-66", "66%", 66),
                ("docx.text.scale-50", "50%", 50),
                ("docx.text.scale-33", "33%", 33),
            ];
            let scale_items: Vec<MenuEntry> = scales.iter().map(|&(id, label, value)| MenuItem::new(id, crate::shell::i18n::text(label))
                .checked(text_style.as_ref().is_some_and(|s| s.scale == value)).disabled(!editing)
                .on(move |this, _, cx| this.set_character_scale(if value == 100 { None } else { Some(value) }, cx)).into()).collect();
            sub.items.push(crate::shell::chrome::Submenu::tr("docx.text.scale_menu", "aspect_ratio", scale_items).into());

            let spacings: &[(&str, &str, ibis_doc_model::Emu)] = &[
                ("docx.text.spacing-normal", "docx.toolbar.normal-0-pt", 0),
                ("docx.text.spacing-expand-05", "docx.toolbar.expanded-plus-0-5-pt", 6_350),
                ("docx.text.spacing-expand-1", "docx.toolbar.expanded-plus-1-pt", 12_700),
                ("docx.text.spacing-expand-15", "docx.toolbar.expanded-plus-1-5-pt", 19_050),
                ("docx.text.spacing-expand-2", "docx.toolbar.expanded-plus-2-pt", 25_400),
                ("docx.text.spacing-expand-3", "docx.toolbar.expanded-plus-3-pt", 38_100),
                ("docx.text.spacing-expand-4", "docx.toolbar.expanded-plus-4-pt", 50_800),
                ("docx.text.spacing-condense-05", "docx.toolbar.condensed-0-5-pt", -6_350),
                ("docx.text.spacing-condense-1", "docx.toolbar.condensed-1-pt", -12_700),
                ("docx.text.spacing-condense-15", "docx.toolbar.condensed-1-5-pt", -19_050),
                ("docx.text.spacing-condense-2", "docx.toolbar.condensed-2-pt", -25_400),
            ];
            let mut spacing_items: Vec<MenuEntry> = Vec::new();
            for &(id, label, value) in spacings {
                if id == "docx.text.spacing-expand-05" || id == "docx.text.spacing-condense-05" {
                    spacing_items.push(MenuEntry::Separator);
                }
                spacing_items.push(MenuItem::new(id, crate::shell::i18n::text(label))
                    .checked(text_style.as_ref().is_some_and(|s| s.spacing == value)).disabled(!editing)
                    .on(move |this, _, cx| this.set_character_spacing(if value == 0 { None } else { Some(value) }, cx)).into());
            }
            sub.items.push(crate::shell::chrome::Submenu::tr("docx.text.spacing_menu", "horizontal_distribute", spacing_items).into());

            let kerns = [
                ("docx.text.kern-none", "fix.toolbar.none-0-pt", Some(0)),
                ("docx.text.kern-1", "fix.toolbar.1-pt-above", Some(2)),
                ("docx.text.kern-10", "fix.toolbar.10-pt-above", Some(20)),
                ("docx.text.kern-12", "fix.toolbar.12-pt-above-default", Some(24)),
                ("docx.text.kern-14", "fix.toolbar.14-pt-above", Some(28)),
                ("docx.text.kern-16", "fix.toolbar.16-pt-above", Some(32)),
                ("docx.text.kern-18", "fix.toolbar.18-pt-above", Some(36)),
            ];
            let mut kern_items: Vec<MenuEntry> = kerns.iter().map(|&(id, label, value)| MenuItem::new(id, crate::shell::i18n::text(label))
                .checked(text_format.as_ref().is_some_and(|f| f.kern == value)).disabled(!editing)
                .on(move |this, _, cx| this.set_character_kerning(value, cx)).into()).collect();
            kern_items.push(MenuEntry::Separator);
            kern_items.push(MenuItem::new("docx.text.kern-clear", crate::shell::i18n::text("fix.toolbar.clear-setting"))
                .checked(text_format.as_ref().is_some_and(|f| f.kern.is_none())).disabled(!editing)
                .on(|this, _, cx| this.set_character_kerning(None, cx)).into());
            sub.items.push(crate::shell::chrome::Submenu::tr("docx.text.kern_menu", "font_download", kern_items).into());

            // OpenType の機能 (Word's フォント > 詳細設定 > OpenType の機能: DOCX-FONT-016).
            {
                use ibis_doc_model::{Ligatures, NumberForm, NumberSpacing, RunChange};

                let lig_items: Vec<MenuEntry> = [
                    ("docx.text.opentype.ligatures.none", Ligatures::None),
                    ("docx.text.opentype.ligatures.standard", Ligatures::Standard),
                    ("docx.text.opentype.ligatures.standard-contextual", Ligatures::StandardContextual),
                    ("docx.text.opentype.ligatures.historical-discretional", Ligatures::HistoricalDiscretional),
                    ("docx.text.opentype.ligatures.all", Ligatures::All),
                ]
                .into_iter()
                .map(|(id, lig)| {
                    MenuItem::tr(id)
                        .checked(text_style.as_ref().is_some_and(|s| s.ligatures == lig))
                        .disabled(!editing)
                        .on(move |this, _, cx| this.apply_run_change(RunChange::Ligatures(lig), cx))
                        .into()
                })
                .collect();

                let spacing_items: Vec<MenuEntry> = [
                    ("docx.text.opentype.num-spacing.default", NumberSpacing::Default),
                    ("docx.text.opentype.num-spacing.proportional", NumberSpacing::Proportional),
                    ("docx.text.opentype.num-spacing.tabular", NumberSpacing::Tabular),
                ]
                .into_iter()
                .map(|(id, spacing)| {
                    MenuItem::tr(id)
                        .checked(text_style.as_ref().is_some_and(|s| s.num_spacing == spacing))
                        .disabled(!editing)
                        .on(move |this, _, cx| this.apply_run_change(RunChange::NumberSpacing(spacing), cx))
                        .into()
                })
                .collect();

                let form_items: Vec<MenuEntry> = [
                    ("docx.text.opentype.num-form.default", NumberForm::Default),
                    ("docx.text.opentype.num-form.lining", NumberForm::Lining),
                    ("docx.text.opentype.num-form.old-style", NumberForm::OldStyle),
                ]
                .into_iter()
                .map(|(id, form)| {
                    MenuItem::tr(id)
                        .checked(text_style.as_ref().is_some_and(|s| s.num_form == form))
                        .disabled(!editing)
                        .on(move |this, _, cx| this.apply_run_change(RunChange::NumberForm(form), cx))
                        .into()
                })
                .collect();

                let cntxt_on = text_style.as_ref().is_some_and(|s| s.contextual_alternates);
                let cntxt_item = MenuItem::tr("docx.text.opentype.cntxt-alts")
                    .checked(cntxt_on)
                    .disabled(!editing)
                    .on(move |this, _, cx| this.apply_run_change(RunChange::ContextualAlternates(!cntxt_on), cx))
                    .into();

                let opentype_entries = vec![
                    crate::shell::chrome::Submenu::tr("docx.text.opentype.ligatures", "font_download", lig_items).into(),
                    crate::shell::chrome::Submenu::tr("docx.text.opentype.num-spacing", "horizontal_distribute", spacing_items).into(),
                    crate::shell::chrome::Submenu::tr("docx.text.opentype.num-form", "pin", form_items).into(),
                    MenuEntry::Separator,
                    cntxt_item,
                ];
                sub.items.push(crate::shell::chrome::Submenu::tr("docx.text.opentype", "text_fields", opentype_entries).into());
            }
        }
    }
    let align = _editor.ordered_selection().and_then(|(s, _)| _editor.flow().and_then(|d| ibis_doc_model::flow_command::paragraph(d, s.para).map(|p| p.align)));
    let contextual_spacing = _editor.ordered_selection().and_then(|(s, _)| _editor.flow().and_then(|d| ibis_doc_model::flow_command::paragraph(d, s.para).map(|p| p.contextual_spacing))).unwrap_or(false);
    for entry in menus.items_mut(MenuId::Format) {
        let MenuEntry::Submenu(sub) = entry else { continue };
        if sub.id == "format.align" {
            for (id, shortcut, value) in [("align.left", "⌘L", FlowAlign::Left), ("align.center", "⌘E", FlowAlign::Center),
                ("align.right", "⌘R", FlowAlign::Right), ("align.justify", "⌘J", FlowAlign::Justify), ("align.distribute", "⇧⌘J", FlowAlign::Distribute)] {
                sub.items.push(MenuItem::tr(id).shortcut(shortcut).checked(align == Some(value)).disabled(!editing).on(move |this, _, cx| this.set_alignment(value, cx)).into());
            }
            sub.items.push(MenuEntry::Separator);
            sub.items.push(MenuItem::new("docx.paragraph.details", crate::shell::i18n::text("docx.paragraph.details#2")).disabled(!editing)
                .on(|this, window, cx| this.open_paragraph_edit(None, window, cx)).into());
            sub.items.push(MenuItem::new("docx.tabs", crate::shell::i18n::text("docx.paragraph-panel.tabs")).disabled(!editing)
                .on(|this, window, cx| this.open_tab_edit(window, cx)).into());
        }
        if sub.id == "format.spacing" {
            for (id, label, rule) in [("docx.line.multiple", "docx.toolbar.multiple", super::paragraph_panel::LineRule::Multiple),
                ("docx.line.atleast", "docx.toolbar.least", super::paragraph_panel::LineRule::AtLeast),
                ("docx.line.exact", "docx.toolbar.exactly", super::paragraph_panel::LineRule::Exact)] {
                sub.items.push(MenuItem::new(id, crate::shell::i18n::text(label)).disabled(!editing).on(move |this, window, cx| this.open_paragraph_edit(Some(rule), window, cx)).into());
            }
            sub.items.push(MenuEntry::Separator);
            sub.items.push(MenuItem::new("docx.spacing.contextual", crate::shell::i18n::text("fix.toolbar.don-t-add-space-between")).checked(contextual_spacing).disabled(!editing).on(move |this, _, cx| this.apply_para_change(ibis_doc_model::ParaChange::ContextualSpacing(!contextual_spacing), cx)).into());
        }
    }
    menus.connect("docx.page-break", |this, _, cx| this.type_text("\u{c}", cx));
    menus.connect("docx.column-break", |this, _, cx| this.type_text("\u{e}", cx));
    menus.connect("docx.toc", |this, _, cx| this.insert_toc(cx));
    menus.connect("docx.toc.update", |this, _, cx| this.update_toc(None, cx));
    menus.connect("docx.toc.update-dialog", |this, _, cx| this.open_update_toc_panel(cx));
    menus.connect("docx.toc.update-page-numbers-only", |this, _, cx| { this.update_toc_page_numbers_only(cx); });
    menus.connect("docx.toc.update-entire-table", |this, _, cx| this.update_toc(None, cx));
    menus.connect("docx.table-of-figures", |this, _, cx| {
        let is_en = crate::shell::i18n::language() == crate::shell::i18n::Language::English;
        this.insert_table_of_figures(&if is_en { "Figure".to_string() } else { crate::shell::i18n::text("docx.common.figure").to_string() }, cx);
    });
    menus.connect("docx.table-of-figures.figure", |this, _, cx| {
        let is_en = crate::shell::i18n::language() == crate::shell::i18n::Language::English;
        this.insert_table_of_figures(&if is_en { "Figure".to_string() } else { crate::shell::i18n::text("docx.common.figure").to_string() }, cx);
    });
    menus.connect("docx.table-of-figures.table", |this, _, cx| {
        let is_en = crate::shell::i18n::language() == crate::shell::i18n::Language::English;
        this.insert_table_of_figures(&if is_en { "Table".to_string() } else { crate::shell::i18n::text("insert.table").to_string() }, cx);
    });
    menus.connect("docx.table-of-figures.equation", |this, _, cx| {
        let is_en = crate::shell::i18n::language() == crate::shell::i18n::Language::English;
        this.insert_table_of_figures(&if is_en { "Equation".to_string() } else { crate::shell::i18n::text("insert.equation").to_string() }, cx);
    });
    // 目次の更新 and its levels, after 目次.
    {
        let entries = menus.items_mut(MenuId::Insert);
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "docx.toc")).map_or(entries.len(), |i| i + 1);
        let levels: Vec<MenuEntry> = [(1u32, "docx.toc.levels-1", "docx.toolbar.up-heading-1"), (2, "docx.toc.levels-2", "docx.toolbar.up-heading-2"), (3, "docx.toc.levels-3", "docx.toolbar.up-heading-3"), (4, "docx.toc.levels-4", "docx.toolbar.up-heading-4")]
            .into_iter()
            .map(|(n, id, label)| MenuItem::new(id, crate::shell::i18n::text(label)).on(move |this, _, cx| this.update_toc(Some((1, n)), cx)).into())
            .collect();
        let update_subitems: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.toc.update-dialog").on(|this, _, cx| this.open_update_toc_panel(cx)).into(),
            MenuItem::tr("docx.toc.update-page-numbers-only").on(|this, _, cx| { this.update_toc_page_numbers_only(cx); }).into(),
            MenuItem::tr("docx.toc.update-entire-table").on(|this, _, cx| this.update_toc(None, cx)).into(),
        ];
        entries.insert(at, MenuItem::tr("docx.toc.manual").on(|this, _, cx| this.insert_manual_toc(cx)).into());
        let at = at + 1;
        entries.insert(at, MenuItem::tr("docx.toc.update").on(|this, _, cx| this.update_toc(None, cx)).into());
        entries.insert(at + 1, crate::shell::chrome::Submenu::tr("docx.toc.update-menu", "refresh", update_subitems).into());
        entries.insert(at + 2, crate::shell::chrome::Submenu::tr("docx.toc.levels", "toc", levels).into());
        let tof_items: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.table-of-figures.figure").on(|this, _, cx| {
                let is_en = crate::shell::i18n::language() == crate::shell::i18n::Language::English;
                this.insert_table_of_figures(&if is_en { "Figure".to_string() } else { crate::shell::i18n::text("docx.common.figure").to_string() }, cx);
            }).into(),
            MenuItem::tr("docx.table-of-figures.table").on(|this, _, cx| {
                let is_en = crate::shell::i18n::language() == crate::shell::i18n::Language::English;
                this.insert_table_of_figures(&if is_en { "Table".to_string() } else { crate::shell::i18n::text("insert.table").to_string() }, cx);
            }).into(),
            MenuItem::tr("docx.table-of-figures.equation").on(|this, _, cx| {
                let is_en = crate::shell::i18n::language() == crate::shell::i18n::Language::English;
                this.insert_table_of_figures(&if is_en { "Equation".to_string() } else { crate::shell::i18n::text("insert.equation").to_string() }, cx);
            }).into(),
        ];
        entries.insert(at + 3, crate::shell::chrome::Submenu::tr("docx.table-of-figures", "table_chart", tof_items).into());
    }
    // 挿入 > 図表番号・相互参照・フィールドの更新 (after 図表目次).
    {
        let captions = _editor.flow().map(|d| ibis_doc_model::flow_command::captions(d)).unwrap_or_default();
        let entries = menus.items_mut(MenuId::Insert);
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Submenu(s) if s.id == "docx.table-of-figures")).map_or(entries.len(), |i| i + 1);
        let mut references: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.xref.dialog").on(|this, window, cx| this.open_cross_reference_panel(window, cx)).into(),
            crate::shell::chrome::MenuEntry::Separator,
        ];
        if captions.is_empty() {
            references.push(MenuItem::tr("docx.xref.none").disabled(true).into());
        } else {
            references.extend(captions.into_iter().enumerate().take(50).map(|(i, (id, _, text))| MenuItem::new(XREF_IDS[i.min(XREF_IDS.len() - 1)], text).on(move |this, _, cx| this.insert_cross_reference(id, cx)).into()));
        };
        entries.insert(at, MenuItem::tr("docx.caption.figure").on(|this, _, cx| this.insert_caption(&crate::shell::i18n::text("docx.common.figure"), cx)).into());
        entries.insert(at + 1, MenuItem::tr("docx.caption.table").on(|this, _, cx| this.insert_caption(&crate::shell::i18n::text("insert.table"), cx)).into());
        entries.insert(at + 2, MenuItem::tr("docx.caption.equation").on(|this, _, cx| this.insert_caption(&crate::shell::i18n::text("insert.equation"), cx)).into());
        entries.insert(at + 3, MenuItem::tr("docx.caption.insert").on(|this, window, cx| this.open_caption_panel(window, cx)).into());
        entries.insert(at + 4, crate::shell::chrome::Submenu::tr("docx.xref", "link", references).into());
        let showing_codes = _editor.flow().is_some_and(|d| d.show_field_codes);
        entries.insert(at + 5, MenuItem::tr("docx.fields.insert").shortcut("⌘F9").on(|this, window, cx| this.open_field_panel(window, cx)).into());
        entries.insert(at + 6, MenuItem::tr("docx.fields.edit").on(|this, window, cx| this.open_field_panel(window, cx)).into());
        entries.insert(at + 7, MenuItem::tr("docx.fields.update").shortcut("F9").on(|this, _, cx| this.update_fields(false, cx)).into());
        entries.insert(at + 8, MenuItem::tr("docx.fields.unlink").shortcut("⇧⌘F9").on(|this, _, cx| this.unlink_fields(cx)).into());
        entries.insert(at + 9, MenuItem::tr("docx.fields.toggle").shortcut("⇧F9").on(|this, _, cx| this.toggle_field_codes(cx)).into());
        entries.insert(at + 10, MenuItem::tr("docx.fields.toggle-all").shortcut("⌥F9").checked(showing_codes).on(|this, _, cx| this.toggle_all_field_codes(cx)).into());
        entries.insert(at + 11, MenuItem::tr("docx.fields.lock").shortcut("⌘F11").on(|this, _, cx| this.lock_fields(true, cx)).into());
        entries.insert(at + 12, MenuItem::tr("docx.fields.unlock").shortcut("⇧⌘F11").on(|this, _, cx| this.lock_fields(false, cx)).into());
    }
    // 挿入 > 脚注・文末脚注 (after the cross-references' items).
    {
        let has_footnotes = _editor.flow().is_some_and(|d| !d.footnotes.is_empty());
        let has_endnotes = _editor.flow().is_some_and(|d| !d.endnotes.is_empty());
        let entries = menus.items_mut(MenuId::Insert);
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "docx.fields.unlock")).map_or(entries.len(), |i| i + 1);
        entries.insert(at, MenuItem::tr("docx.footnote").icon("notes").shortcut("⌥⌘F").on(|this, _, cx| this.insert_note(false, cx)).into());
        entries.insert(at + 1, MenuItem::tr("docx.endnote").shortcut("⌥⌘E").on(|this, _, cx| this.insert_note(true, cx)).into());
        entries.insert(at + 2, MenuItem::tr("docx.footnote.next").on(|this, _, cx| this.go_to_note(false, true, cx)).into());
        entries.insert(at + 3, MenuItem::tr("docx.footnote.previous").on(|this, _, cx| this.go_to_note(false, false, cx)).into());
        entries.insert(at + 4, MenuItem::tr("docx.notes.show").on(|this, _, cx| this.show_note(cx)).into());
        entries.insert(at + 2, MenuItem::tr("docx.notes.to-endnotes").disabled(!has_footnotes).on(|this, _, cx| this.convert_notes(true, cx)).into());
        entries.insert(at + 3, MenuItem::tr("docx.notes.to-footnotes").disabled(!has_endnotes).on(|this, _, cx| this.convert_notes(false, cx)).into());
        entries.insert(at + 7, MenuItem::tr("docx.notes.settings").on(|this, window, cx| this.open_notes_panel(window, cx)).into());
        entries.insert(at + 5, MenuItem::tr("docx.bookmark").on(|this, window, cx| this.open_bookmark_panel(window, cx)).into());
        // 注の区切り線: the line above the footnotes and above the endnotes.
        use ibis_doc_model::NoteSeparator as Sep;
        let separators = _editor.flow().map(|d| d.note_separators).unwrap_or_default();
        let items = |endnote: bool| -> Vec<MenuEntry> {
            let current = separators[usize::from(endnote)];
            [("docx.notes.separator-standard", "docx.notes.separator-standard-end", Sep::Standard), ("docx.notes.separator-full", "docx.notes.separator-full-end", Sep::Full), ("docx.notes.separator-none", "docx.notes.separator-none-end", Sep::Hidden)]
                .into_iter()
                .map(|(id, end_id, kind)| MenuItem::new(if endnote { end_id } else { id }, crate::shell::i18n::text(id)).checked(current == kind).on(move |this, _, cx| this.set_note_separator(endnote, kind, cx)).into())
                .collect()
        };
        entries.insert(at + 6, crate::shell::chrome::Submenu::tr("docx.notes.separator-footnote", "horizontal_rule", items(false)).into());
        entries.insert(at + 7, crate::shell::chrome::Submenu::tr("docx.notes.separator-endnote", "horizontal_rule", items(true)).into());
    }
    // 挿入 > 日付と時刻 (after フィールドの更新): the time now in Word's
    // Japanese pictures, as text or (自動的に更新する) a DATE/TIME field.
    {
        let now = crate::clock::local_now();
        let update = DATE_UPDATES.load(std::sync::atomic::Ordering::Relaxed);
        let mut items: Vec<MenuEntry> = DATE_PICTURES
            .iter()
            .enumerate()
            .map(|(i, picture)| MenuItem::new(DATE_IDS[i], ibis_doc_model::date_field::format_picture(&now, picture)).on(move |this, _, cx| this.insert_date(picture, update, cx)).into())
            .collect();
        items.push(MenuEntry::Separator);
        items.push(
            MenuItem::tr("docx.date.update")
                .checked(update)
                .on(move |_, _, cx| {
                    DATE_UPDATES.store(!update, std::sync::atomic::Ordering::Relaxed);
                    cx.notify();
                })
                .into(),
        );
        let entries = menus.items_mut(MenuId::Insert);
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "docx.fields.unlock")).map_or(entries.len(), |i| i + 1);
        entries.insert(at, crate::shell::chrome::Submenu::tr("docx.date", "schedule", items).into());
    }
    // ツール > 変更履歴 (Word's 校閲): recording, 承諾・却下, how it is shown.
    {
        use crate::docx::flow_layout::MarkupDisplayMode;
        let tracking = _editor.flow().is_some_and(|d| d.track_changes);
        let locked = _editor.is_track_changes_locked();
        let mode = crate::docx::flow_layout::markup_display_mode();
        let display_menu: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.track.display.simple")
                .checked(mode == MarkupDisplayMode::Simple)
                .on(|this, _, cx| this.set_markup_display_mode(MarkupDisplayMode::Simple, cx))
                .into(),
            MenuItem::tr("docx.track.display.all")
                .checked(mode == MarkupDisplayMode::All)
                .on(|this, _, cx| this.set_markup_display_mode(MarkupDisplayMode::All, cx))
                .into(),
            MenuItem::tr("docx.track.display.none")
                .checked(mode == MarkupDisplayMode::None)
                .on(|this, _, cx| this.set_markup_display_mode(MarkupDisplayMode::None, cx))
                .into(),
            MenuItem::tr("docx.track.display.original")
                .checked(mode == MarkupDisplayMode::Original)
                .on(|this, _, cx| this.set_markup_display_mode(MarkupDisplayMode::Original, cx))
                .into(),
        ];
        let authors = _editor.revision_authors();
        let mut reviewer_menu: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.track.reviewers.all")
                .checked(_editor.is_all_reviewers())
                .on(|this, _, cx| {
                    let next = !this.is_all_reviewers();
                    this.set_all_reviewers(next, cx);
                })
                .into(),
        ];
        if !authors.is_empty() {
            reviewer_menu.push(MenuEntry::Separator);
            for author in authors {
                let author_for_id = author.clone();
                let author_for_on = author.clone();
                let menu_id: &'static str = Box::leak(format!("docx.track.reviewers.author:{author_for_id}").into_boxed_str());
                let is_checked = _editor.is_reviewer_shown(&author);
                reviewer_menu.push(
                    MenuItem::new(menu_id, author)
                        .checked(is_checked)
                        .on(move |this, _, cx| this.toggle_reviewer(&author_for_on, cx))
                        .into(),
                );
            }
        }
        let menu: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.track.record").checked(tracking).disabled(locked).on(|this, _, cx| this.toggle_track_changes(cx)).into(),
            MenuItem::tr("docx.track.lock").checked(locked).icon("lock").on(|this, _, cx| this.toggle_lock_tracking(cx)).into(),
            crate::shell::chrome::Submenu::tr("docx.track.display", "visibility", display_menu).into(),
            crate::shell::chrome::Submenu::tr("docx.track.reviewers", "people", reviewer_menu).into(),
            MenuItem::tr("docx.track.show").checked(mode == MarkupDisplayMode::All).on(move |this, _, cx| {
                let next = if mode == MarkupDisplayMode::All { MarkupDisplayMode::None } else { MarkupDisplayMode::All };
                this.set_markup_display_mode(next, cx);
            })
            .into(),
            MenuEntry::Separator,
            MenuItem::tr("docx.track.accept").disabled(locked).on(|this, _, cx| this.resolve_revisions(false, true, cx)).into(),
            MenuItem::tr("docx.track.reject").disabled(locked).on(|this, _, cx| this.resolve_revisions(false, false, cx)).into(),
            MenuItem::tr("docx.track.accept-shown").disabled(locked).on(|this, _, cx| this.resolve_revisions_shown(true, cx)).into(),
            MenuItem::tr("docx.track.reject-shown").disabled(locked).on(|this, _, cx| this.resolve_revisions_shown(false, cx)).into(),
            MenuItem::tr("docx.track.accept-all").disabled(locked).on(|this, _, cx| this.resolve_revisions(true, true, cx)).into(),
            MenuItem::tr("docx.track.reject-all").disabled(locked).on(|this, _, cx| this.resolve_revisions(true, false, cx)).into(),
            MenuItem::tr("docx.track.accept-all-stop").disabled(locked).on(|this, _, cx| this.resolve_revisions_and_stop_tracking(true, cx)).into(),
            MenuItem::tr("docx.track.reject-all-stop").disabled(locked).on(|this, _, cx| this.resolve_revisions_and_stop_tracking(false, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("docx.compare").icon("compare").disabled(locked).on(|this, window, cx| this.pick_compare_file(window, cx)).into(),
            MenuItem::tr("docx.compare.new-doc").icon("add_to_photos").disabled(locked).on(|this, window, cx| this.pick_compare_file_new_doc(window, cx)).into(),
            MenuItem::tr("docx.combine").icon("call_merge").disabled(locked).on(|this, window, cx| this.pick_combine_file(window, cx)).into(),
            MenuItem::tr("docx.compare.format").checked(_editor.doc_state().is_none_or(|s| s.compare_options.format)).on(|this, _, cx| this.toggle_compare_option(0, cx)).into(),
            MenuItem::tr("docx.compare.case").checked(_editor.doc_state().is_none_or(|s| s.compare_options.case)).on(|this, _, cx| this.toggle_compare_option(1, cx)).into(),
            MenuItem::tr("docx.compare.white-space").checked(_editor.doc_state().is_none_or(|s| s.compare_options.white_space)).on(|this, _, cx| this.toggle_compare_option(3, cx)).into(),
            MenuItem::tr("docx.compare.tables").checked(_editor.doc_state().is_none_or(|s| s.compare_options.tables)).on(|this, _, cx| this.toggle_compare_option(2, cx)).into(),
            MenuItem::tr("docx.compare.headers").checked(_editor.doc_state().is_none_or(|s| s.compare_options.headers)).on(|this, _, cx| this.toggle_compare_option(4, cx)).into(),
            MenuItem::tr("docx.compare.footnotes").checked(_editor.doc_state().is_none_or(|s| s.compare_options.footnotes)).on(|this, _, cx| this.toggle_compare_option(5, cx)).into(),
            MenuItem::tr("docx.compare.comments").checked(_editor.doc_state().is_none_or(|s| s.compare_options.comments)).on(|this, _, cx| this.toggle_compare_option(6, cx)).into(),
            MenuItem::tr("docx.compare.fields").checked(_editor.doc_state().is_none_or(|s| s.compare_options.fields)).on(|this, _, cx| this.toggle_compare_option(7, cx)).into(),
            MenuItem::tr("docx.track.change-user-name").icon("person").on(|this, window, cx| this.open_user_info_panel(window, cx)).into(),
        ];
        menus.items_mut(MenuId::Tools).push(crate::shell::chrome::Submenu::tr("docx.track", "rate_review", menu).into());
        {
            let protection = _editor.document_protection();
            let formatting_protected = _editor.is_formatting_protected();
            let is_protected = protection != ibis_doc_model::DocumentProtection::None || formatting_protected;
            let protect_menu: Vec<MenuEntry> = vec![
                MenuItem::tr("docx.protect.formatting")
                    .checked(formatting_protected)
                    .on(|this, _, cx| {
                        let next = !this.is_formatting_protected();
                        this.set_formatting_protection(next, cx);
                    })
                    .into(),
                MenuItem::tr("docx.protect.read-only")
                    .checked(protection == ibis_doc_model::DocumentProtection::ReadOnly)
                    .on(|this, _, cx| {
                        let next = if this.is_protection_read_only() {
                            ibis_doc_model::DocumentProtection::None
                        } else {
                            ibis_doc_model::DocumentProtection::ReadOnly
                        };
                        this.set_document_protection(next, cx);
                    })
                    .into(),
                MenuItem::tr("docx.protect.comments")
                    .checked(protection == ibis_doc_model::DocumentProtection::Comments)
                    .on(|this, _, cx| {
                        let next = if this.is_protection_comments() {
                            ibis_doc_model::DocumentProtection::None
                        } else {
                            ibis_doc_model::DocumentProtection::Comments
                        };
                        this.set_document_protection(next, cx);
                    })
                    .into(),
                MenuItem::tr("docx.protect.forms")
                    .checked(protection == ibis_doc_model::DocumentProtection::Forms)
                    .on(|this, _, cx| {
                        let next = if this.is_protection_forms() {
                            ibis_doc_model::DocumentProtection::None
                        } else {
                            ibis_doc_model::DocumentProtection::Forms
                        };
                        this.set_document_protection(next, cx);
                    })
                    .into(),
                MenuEntry::Separator,
                MenuItem::tr("docx.protect.stop")
                    .disabled(!is_protected)
                    .on(|this, _, cx| this.stop_protection(cx))
                    .into(),
            ];
            menus.items_mut(MenuId::Tools).push(crate::shell::chrome::Submenu::tr("docx.protect", "security", protect_menu).into());
        }
        // ツール > 言語: the language of the selected text (w:lang) and default language (w:themeFontLang), as Word's 校正言語の設定.
        {
            let editing = _editor.ordered_selection().is_some();
            let current_format = _editor.current_format().map(|(f, _)| f);
            let no_proof = current_format.as_ref().is_some_and(|f| f.no_proof == Some(true));
            let current = current_format.as_ref().map(|f| (&f.lang, &f.lang_east_asia, &f.lang_bidi));

            #[derive(Clone, Copy)]
            enum LangKind {
                Western,
                EastAsia,
                Bidi,
            }

            let mut languages: Vec<MenuEntry> = [
                ("docx.language.ja", "ja-JP", LangKind::EastAsia),
                ("docx.language.en-US", "en-US", LangKind::Western),
                ("docx.language.en-GB", "en-GB", LangKind::Western),
                ("docx.language.fr", "fr-FR", LangKind::Western),
                ("docx.language.de", "de-DE", LangKind::Western),
                ("docx.language.zh", "zh-CN", LangKind::EastAsia),
                ("docx.language.ko", "ko-KR", LangKind::EastAsia),
                ("docx.language.ar", "ar-SA", LangKind::Bidi),
                ("docx.language.he", "he-IL", LangKind::Bidi),
            ]
            .into_iter()
            .map(|(id, code, kind)| {
                let on = current.as_ref().is_some_and(|(l, e, b)| match kind {
                    LangKind::Western => l.as_deref() == Some(code),
                    LangKind::EastAsia => e.as_deref() == Some(code),
                    LangKind::Bidi => b.as_deref() == Some(code),
                });
                MenuItem::tr(id)
                    .checked(on)
                    .disabled(!editing)
                    .on(move |this, _, cx| {
                        let code = Some(code.to_string());
                        let change = match kind {
                            LangKind::Western => ibis_doc_model::RunChange::Language { lang: code, east_asia: None, bidi: None },
                            LangKind::EastAsia => ibis_doc_model::RunChange::Language { lang: None, east_asia: code, bidi: None },
                            LangKind::Bidi => ibis_doc_model::RunChange::Language { lang: None, east_asia: None, bidi: code },
                        };
                        this.apply_run_change(change, cx)
                    })
                    .into()
            })
            .collect();
            languages.push(MenuEntry::Separator);
            languages.push(
                MenuItem::tr("docx.language.set-default")
                    .icon("settings_backup_restore")
                    .disabled(!editing)
                    .on(|this, _, cx| this.set_current_as_default_language(cx))
                    .into(),
            );
            languages.push(MenuEntry::Separator);
            languages.push(
                MenuItem::tr("docx.language.no-proof")
                    .checked(no_proof)
                    .disabled(!editing)
                    .on(move |this, _, cx| this.apply_run_change(ibis_doc_model::RunChange::NoProof(!no_proof), cx))
                    .into(),
            );
            menus.items_mut(MenuId::Tools).push(crate::shell::chrome::Submenu::tr("docx.language", "translate", languages).into());
        }
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.merge").icon("mail").on(|this, window, cx| this.open_merge(window, cx)).into());
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.merge.print").icon("print").on(|this, window, cx| this.print_merged_documents(window, cx)).into());
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.merge.export-pdf").icon("picture_as_pdf").on(|this, window, cx| this.export_merged_pdf(window, cx)).into());
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.merge.match-fields").icon("sync_alt").on(|this, _, cx| this.toggle_match_fields(cx)).into());
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.merge.auto-match-fields").icon("auto_fix_high").on(|this, _, cx| this.auto_match_merge_fields(cx)).into());
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.merge.clear-match-fields").icon("clear").on(|this, _, cx| this.clear_merge_field_map(cx)).into());
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.merge.check-errors").icon("fact_check").on(|this, _, cx| { this.check_merge_errors(cx); }).into());
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.accessibility-check").icon("accessibility").on(|this, _, cx| this.open_accessibility_panel(cx)).into());
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.inspect-document").icon("find_in_page").on(|this, window, cx| this.open_doc_inspect_panel(window, cx)).into());
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.compatibility-check").icon("rule").on(|this, window, cx| this.open_doc_compatibility_panel(window, cx)).into());
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.print-settings").icon("tune").on(|this, window, cx| this.open_doc_print_panel(window, cx)).into());
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.reference.source-manager").icon("manage_accounts").on(|this, window, cx| this.open_source_manager_panel(window, cx)).into());
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.options.user-info").icon("person").on(|this, window, cx| this.open_user_info_panel(window, cx)).into());
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.file.repair").icon("cleaning_services").disabled(super::repair::damaged_document().is_none()).on(|this, window, cx| this.open_docx_repair_damaged(window, cx)).into());
        menus.items_mut(MenuId::Tools).push(MenuItem::tr("docx.template.attach").icon("extension").on(|this, window, cx| this.open_docx_template_panel(window, cx)).into());
    }
    menus.connect("docx.protect.formatting", |this, _, cx| {
        let next = !this.is_formatting_protected();
        this.set_formatting_protection(next, cx);
    });
    menus.connect("docx.protect.read-only", |this, _, cx| {
        let next = if this.is_protection_read_only() {
            ibis_doc_model::DocumentProtection::None
        } else {
            ibis_doc_model::DocumentProtection::ReadOnly
        };
        this.set_document_protection(next, cx);
    });
    menus.connect("docx.protect.comments", |this, _, cx| {
        let next = if this.is_protection_comments() {
            ibis_doc_model::DocumentProtection::None
        } else {
            ibis_doc_model::DocumentProtection::Comments
        };
        this.set_document_protection(next, cx);
    });
    menus.connect("docx.protect.forms", |this, _, cx| {
        let next = if this.is_protection_forms() {
            ibis_doc_model::DocumentProtection::None
        } else {
            ibis_doc_model::DocumentProtection::Forms
        };
        this.set_document_protection(next, cx);
    });
    menus.connect("docx.protect.stop", |this, _, cx| this.stop_protection(cx));
    menus.connect("docx.language.set-default", |this, _, cx| this.set_current_as_default_language(cx));
    menus.connect("docx.language.ja", |this, _, cx| this.apply_run_change(ibis_doc_model::RunChange::Language { lang: None, east_asia: Some("ja-JP".into()), bidi: None }, cx));
    menus.connect("docx.language.en-US", |this, _, cx| this.apply_run_change(ibis_doc_model::RunChange::Language { lang: Some("en-US".into()), east_asia: None, bidi: None }, cx));
    menus.connect("docx.language.en-GB", |this, _, cx| this.apply_run_change(ibis_doc_model::RunChange::Language { lang: Some("en-GB".into()), east_asia: None, bidi: None }, cx));
    menus.connect("docx.language.fr", |this, _, cx| this.apply_run_change(ibis_doc_model::RunChange::Language { lang: Some("fr-FR".into()), east_asia: None, bidi: None }, cx));
    menus.connect("docx.language.de", |this, _, cx| this.apply_run_change(ibis_doc_model::RunChange::Language { lang: Some("de-DE".into()), east_asia: None, bidi: None }, cx));
    menus.connect("docx.language.zh", |this, _, cx| this.apply_run_change(ibis_doc_model::RunChange::Language { lang: None, east_asia: Some("zh-CN".into()), bidi: None }, cx));
    menus.connect("docx.language.ko", |this, _, cx| this.apply_run_change(ibis_doc_model::RunChange::Language { lang: None, east_asia: Some("ko-KR".into()), bidi: None }, cx));
    menus.connect("docx.language.ar", |this, _, cx| this.apply_run_change(ibis_doc_model::RunChange::Language { lang: None, east_asia: None, bidi: Some("ar-SA".into()) }, cx));
    menus.connect("docx.language.he", |this, _, cx| this.apply_run_change(ibis_doc_model::RunChange::Language { lang: None, east_asia: None, bidi: Some("he-IL".into()) }, cx));
    menus.connect("docx.track.lock", |this, _, cx| this.toggle_lock_tracking(cx));
    menus.connect("docx.track.accept", |this, _, cx| this.resolve_revisions(false, true, cx));
    menus.connect("docx.track.reject", |this, _, cx| this.resolve_revisions(false, false, cx));
    menus.connect("docx.track.accept-shown", |this, _, cx| this.resolve_revisions_shown(true, cx));
    menus.connect("docx.track.reject-shown", |this, _, cx| this.resolve_revisions_shown(false, cx));
    menus.connect("docx.track.accept-all", |this, _, cx| this.resolve_revisions(true, true, cx));
    menus.connect("docx.track.reject-all", |this, _, cx| this.resolve_revisions(true, false, cx));
    menus.connect("docx.track.accept-all-stop", |this, _, cx| this.resolve_revisions_and_stop_tracking(true, cx));
    menus.connect("docx.track.reject-all-stop", |this, _, cx| this.resolve_revisions_and_stop_tracking(false, cx));
    menus.connect("docx.track.display.simple", |this, _, cx| this.set_markup_display_mode(crate::docx::flow_layout::MarkupDisplayMode::Simple, cx));
    menus.connect("docx.track.display.all", |this, _, cx| this.set_markup_display_mode(crate::docx::flow_layout::MarkupDisplayMode::All, cx));
    menus.connect("docx.track.display.none", |this, _, cx| this.set_markup_display_mode(crate::docx::flow_layout::MarkupDisplayMode::None, cx));
    menus.connect("docx.track.display.original", |this, _, cx| this.set_markup_display_mode(crate::docx::flow_layout::MarkupDisplayMode::Original, cx));
    menus.connect("docx.track.reviewers.all", |this, _, cx| {
        let next = !this.is_all_reviewers();
        this.set_all_reviewers(next, cx);
    });
    menus.connect("docx.track.change-user-name", |this, window, cx| this.open_user_info_panel(window, cx));
    menus.connect("docx.options.user-info", |this, window, cx| this.open_user_info_panel(window, cx));
    menus.connect("docx.accessibility-check", |this, _, cx| this.open_accessibility_panel(cx));
    menus.connect("docx.merge", |this, window, cx| this.open_merge(window, cx));
    menus.connect("docx.merge.print", |this, window, cx| this.print_merged_documents(window, cx));
    menus.connect("docx.merge.export-pdf", |this, window, cx| this.export_merged_pdf(window, cx));
    menus.connect("docx.merge.match-fields", |this, _, cx| this.toggle_match_fields(cx));
    menus.connect("docx.merge.auto-match-fields", |this, _, cx| this.auto_match_merge_fields(cx));
    menus.connect("docx.merge.clear-match-fields", |this, _, cx| this.clear_merge_field_map(cx));
    menus.connect("docx.merge.check-errors", |this, _, cx| { this.check_merge_errors(cx); });
    menus.connect("docx.merge-panel.check-errors", |this, _, cx| { this.check_merge_errors(cx); });
    menus.connect("docx.reference.insert-citation", |this, _, cx| this.insert_citation_at_selection(cx));
    menus.connect("docx.reference.bibliography", |this, _, cx| this.insert_bibliography(cx));
    menus.connect("docx.reference.bibliography.update", |this, _, cx| this.update_bibliography(cx));
    menus.connect("docx.reference.source-manager", |this, window, cx| this.open_source_manager_panel(window, cx));
    menus.connect("docx.compare", |this, window, cx| this.pick_compare_file(window, cx));
    menus.connect("docx.compare.new-doc", |this, window, cx| this.pick_compare_file_new_doc(window, cx));
    menus.connect("docx.combine", |this, window, cx| this.pick_combine_file(window, cx));
    menus.connect("docx.compare.format", |this, _, cx| this.toggle_compare_option(0, cx));
    menus.connect("docx.compare.case", |this, _, cx| this.toggle_compare_option(1, cx));
    menus.connect("docx.compare.white-space", |this, _, cx| this.toggle_compare_option(3, cx));
    menus.connect("docx.compare.tables", |this, _, cx| this.toggle_compare_option(2, cx));
    menus.connect("docx.compare.headers", |this, _, cx| this.toggle_compare_option(4, cx));
    menus.connect("docx.compare.footnotes", |this, _, cx| this.toggle_compare_option(5, cx));
    menus.connect("docx.compare.comments", |this, _, cx| this.toggle_compare_option(6, cx));
    menus.connect("docx.compare.fields", |this, _, cx| this.toggle_compare_option(7, cx));
    menus.connect("docx.ruler", |this, _, cx| this.open_ruler_panel(cx));
    menus.connect("docx.vertical-ruler", |this, _, cx| this.toggle_vertical_ruler(cx));
    menus.connect("docx.ruler-unit.cm", |this, _, cx| this.set_ruler_unit(crate::shell::editor::RulerUnit::Centimeters, cx));
    menus.connect("docx.ruler-unit.mm", |this, _, cx| this.set_ruler_unit(crate::shell::editor::RulerUnit::Millimeters, cx));
    menus.connect("docx.ruler-unit.inch", |this, _, cx| this.set_ruler_unit(crate::shell::editor::RulerUnit::Inches, cx));
    menus.connect("docx.ruler-unit.point", |this, _, cx| this.set_ruler_unit(crate::shell::editor::RulerUnit::Points, cx));
    menus.connect("docx.ruler-unit.pica", |this, _, cx| this.set_ruler_unit(crate::shell::editor::RulerUnit::Picas, cx));
    menus.connect("docx.white-space", |this, _, cx| this.toggle_white_space(cx));
    menus.connect("docx.page-movement.vertical", |this, window, cx| this.set_page_movement_side_to_side(false, window, cx));
    menus.connect("docx.page-movement.side-to-side", |this, window, cx| this.set_page_movement_side_to_side(true, window, cx));
    menus.connect("docx.interaction.editing", |this, window, cx| {
        this.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Editing, window, cx);
    });
    menus.connect("docx.interaction.reviewing", |this, window, cx| {
        this.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Reviewing, window, cx);
    });
    menus.connect("docx.interaction.viewing", |this, window, cx| {
        this.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Viewing, window, cx);
    });
    menus.connect("docx.print-layout", |this, window, cx| this.set_document_view_mode(ibis_doc_model::DocumentViewMode::Print, window, cx));
    menus.connect("docx.view.read-mode", |this, window, cx| this.set_document_view_mode(ibis_doc_model::DocumentViewMode::Reading, window, cx));
    menus.connect("docx.view.web-layout", |this, window, cx| this.set_document_view_mode(ibis_doc_model::DocumentViewMode::Web, window, cx));
    menus.connect("docx.view.outline", |this, window, cx| this.set_document_view_mode(ibis_doc_model::DocumentViewMode::Outline, window, cx));
    menus.connect("docx.view.draft", |this, window, cx| this.set_document_view_mode(ibis_doc_model::DocumentViewMode::Draft, window, cx));
    menus.connect("docx.view.focus", |this, window, cx| this.toggle_focus_mode(window, cx));
    {
        const VIEW_MODE_IDS: [&str; 6] = [
            "docx.view.read-mode",
            "docx.print-layout",
            "docx.view.web-layout",
            "docx.view.outline",
            "docx.view.draft",
            "docx.view.focus",
        ];
        let view_items = menus.items_mut(MenuId::View);
        let already_grouped = view_items.iter().any(|e| {
            matches!(e, MenuEntry::Submenu(sub) if sub.items.iter().any(|i| matches!(i, MenuEntry::Item(it) if VIEW_MODE_IDS.contains(&it.id))))
        });
        if !already_grouped {
            let mut mode_items = Vec::new();
            let mut insert_idx = None;
            let mut idx = 0;
            while idx < view_items.len() {
                let is_mode_item = matches!(&view_items[idx], MenuEntry::Item(it) if VIEW_MODE_IDS.contains(&it.id));
                if is_mode_item {
                    if insert_idx.is_none() {
                        insert_idx = Some(idx);
                    }
                    mode_items.push(view_items.remove(idx));
                } else {
                    idx += 1;
                }
            }
            if !mode_items.is_empty() {
                let pos = insert_idx.unwrap_or(0).min(view_items.len());
                view_items.insert(
                    pos,
                    crate::shell::chrome::Submenu::tr("docx.view.view-mode", "chrome_reader_mode", mode_items).into(),
                );
            }
        }
    }
    menus.connect("docx.view.immersive-reader", |this, window, cx| this.toggle_immersive_reader(window, cx));
    menus.connect("docx.view.mini-toolbar", |this, _, cx| this.toggle_mini_toolbar(cx));
    menus.connect("docx.view.screentips.show-descriptions", |this, _, cx| this.set_screentip_style(crate::docx::screentip::ScreenTipStyle::ShowFeatureDescriptions, cx));
    menus.connect("docx.view.screentips.dont-show-descriptions", |this, _, cx| this.set_screentip_style(crate::docx::screentip::ScreenTipStyle::DontShowFeatureDescriptions, cx));
    menus.connect("docx.view.screentips.dont-show", |this, _, cx| this.set_screentip_style(crate::docx::screentip::ScreenTipStyle::DontShowScreenTips, cx));
    menus.connect("docx.view.one-page", |this, window, cx| this.zoom_one_page(window, cx));
    menus.connect("docx.view.multiple-pages", |this, window, cx| this.zoom_multiple_pages(window, cx));
    menus.connect("docx.view.page-width", |this, window, cx| this.zoom_page_width(window, cx));
    menus.connect("docx.view.zoom-100", |this, window, cx| this.zoom_100_percent(window, cx));
    {
        let win_items = menus.items_mut(MenuId::Window);
        let has_id = |items: &[MenuEntry], id: &str| items.iter().any(|e| matches!(e, MenuEntry::Item(it) if it.id == id));
        if has_id(win_items, "docx.new-window") {
            win_items.retain(|e| !matches!(e, MenuEntry::Item(it) if it.id == "window.new-window"));
        } else if let Some(MenuEntry::Item(it)) = win_items.iter_mut().find(|e| matches!(e, MenuEntry::Item(it) if it.id == "window.new-window")) {
            it.id = "docx.new-window";
        }
        if has_id(win_items, "docx.arrange-all") {
            win_items.retain(|e| !matches!(e, MenuEntry::Item(it) if it.id == "window.arrange-all"));
        } else if let Some(MenuEntry::Item(it)) = win_items.iter_mut().find(|e| matches!(e, MenuEntry::Item(it) if it.id == "window.arrange-all")) {
            it.id = "docx.arrange-all";
        }
        if has_id(win_items, "docx.split") || has_id(win_items, "docx.remove-split") {
            win_items.retain(|e| !matches!(e, MenuEntry::Item(it) if it.id == "window.split"));
        } else if let Some(MenuEntry::Item(it)) = win_items.iter_mut().find(|e| matches!(e, MenuEntry::Item(it) if it.id == "window.split")) {
            it.id = if _editor.is_document_split() { "docx.remove-split" } else { "docx.split" };
        }
    }
    menus.connect("docx.new-window", |this, _window, cx| this.new_window(cx));
    menus.connect("docx.arrange-all", |_this, window, cx| SlideEditor::arrange_all_windows(Some(window), cx));
    menus.connect("docx.split", |this, _, cx| this.toggle_document_split(cx));
    menus.connect("docx.remove-split", |this, _, cx| this.toggle_document_split(cx));
    if let Some(item) = menus.find_mut("docx.view-side-by-side") {
        item.checked = _editor.side_by_side;
    }
    menus.connect("docx.view-side-by-side", |this, window, cx| {
        this.toggle_view_side_by_side(window, cx);
    });
    if let Some(item) = menus.find_mut("docx.sync-scrolling") {
        item.checked = _editor.sync_scrolling;
        item.disabled = !_editor.side_by_side;
    }
    menus.connect("docx.sync-scrolling", |this, _window, cx| {
        this.toggle_sync_scrolling(cx);
    });
    if let Some(item) = menus.find_mut("docx.reset-window-position") {
        item.disabled = !_editor.side_by_side;
    }
    menus.connect("docx.reset-window-position", |this, window, cx| {
        this.reset_side_by_side_positions(window, cx);
    });
    menus.connect("insert.link", |this, window, cx| this.open_link(window, cx));
    menus.connect("insert.comment", |this, window, cx| this.add_new_comment(window, cx));
    menus.connect("insert.rich-text", |this, _, cx| this.insert_rich_text_control(None, cx));
    menus.connect("insert.plain-text", |this, _, cx| this.insert_plain_text_control(None, cx));
    menus.connect("insert.picture-control", |this, _, cx| this.insert_picture_control(cx));
    menus.connect("insert.building-block-gallery", |this, _, cx| this.insert_building_block_gallery(None, None, cx));
    menus.connect("insert.repeating-section", |this, _, cx| this.insert_repeating_section(None, None, cx));
    menus.connect("insert.group", |this, _, cx| this.insert_group_control(None, cx));
    menus.connect("insert.checkbox", |this, _, cx| this.insert_checkbox(cx));
    menus.connect("docx.form.checkbox.toggle", |this, _, cx| this.toggle_checklist(cx));
    menus.connect("insert.dropdown-list", |this, _, cx| {
        this.insert_dropdown_list(vec![
            (crate::shell::i18n::text("_docx.form-choice-1").to_string(), "1".to_string()),
            (crate::shell::i18n::text("_docx.form-choice-2").to_string(), "2".to_string()),
            (crate::shell::i18n::text("_docx.form-choice-3").to_string(), "3".to_string()),
        ], cx);
    });
    menus.connect("insert.combo-box", |this, _, cx| {
        this.insert_combo_box(vec![
            (crate::shell::i18n::text("_docx.form-item-1").to_string(), "1".to_string()),
            (crate::shell::i18n::text("_docx.form-item-2").to_string(), "2".to_string()),
            (crate::shell::i18n::text("_docx.form-item-3").to_string(), "3".to_string()),
        ], cx);
    });
    menus.connect("docx.form.dropdown.cycle", |this, _, cx| {
        this.cycle_dropdown_around_caret(cx);
    });
    menus.connect("insert.date-picker", |this, _, cx| {
        this.insert_date_picker(None, cx);
    });
    menus.connect("docx.form.properties", |this, window, cx| this.open_control_properties(window, cx));
    menus.connect("docx.form.date.step", |this, _, cx| {
        this.step_date_picker_around_caret(1, cx);
    });
    menus.connect("insert.form-text", |this, _, cx| this.insert_legacy_form_text(cx));
    menus.connect("insert.form-checkbox", |this, _, cx| this.insert_legacy_form_checkbox(cx));
    menus.connect("insert.form-dropdown", |this, _, cx| this.insert_legacy_form_dropdown(Vec::new(), cx));
    menus.connect("docx.form.legacy-dropdown.cycle", |this, _, cx| {
        this.cycle_dropdown_around_caret(cx);
    });
    menus.connect("docx.form.picture.replace", |this, window, cx| {
        this.doc_pick_replace_picture_control(window, cx);
    });
    menus.connect("docx.form.picture.reset", |this, _, cx| {
        this.reset_picture_control_around_caret(cx);
    });
    menus.connect("docx.form.gallery.cycle", |this, _, cx| {
        this.cycle_building_block_gallery_around_caret(cx);
    });
    menus.connect("docx.form.gallery.reset", |this, _, cx| {
        this.reset_building_block_gallery_around_caret(cx);
    });
    menus.connect("docx.form.repeating.add", |this, _, cx| {
        this.add_repeating_section_item_around_caret(cx);
    });
    menus.connect("docx.form.repeating.delete", |this, _, cx| {
        this.delete_repeating_section_item_around_caret(cx);
    });
    menus.connect("docx.form.repeating.cycle", |this, _, cx| {
        this.cycle_repeating_section_around_caret(cx);
    });
    menus.connect("docx.form.group", |this, _, cx| {
        this.group_control_at_selection_or_caret(cx);
    });
    menus.connect("docx.form.ungroup", |this, _, cx| {
        this.ungroup_control_around_caret(cx);
    });
    if let Some(item) = menus.find_mut("docx.form.design-mode") {
        item.checked = _editor.form_design_mode();
    }
    menus.connect("docx.form.design-mode", |this, _, cx| {
        this.toggle_form_design_mode(cx);
    });
    {
        let entries = menus.items_mut(MenuId::Insert);
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "insert.comment")).map_or(entries.len(), |i| i + 1);
        entries.insert(at, MenuItem::tr("docx.comments.show").icon("comment").on(|this, window, cx| this.open_comments(window, cx)).into());
        entries.insert(at + 1, MenuItem::tr("docx.comments.next").on(|this, _, cx| this.go_to_comment(true, cx)).into());
        entries.insert(at + 2, MenuItem::tr("docx.comments.previous").on(|this, _, cx| this.go_to_comment(false, cx)).into());
    }
    menus.connect("edit.find-replace", |this, window, cx| this.open_find(window, cx));
    menus.connect("edit.go-to", |this, window, cx| this.open_go_to_panel(window, cx));
    menus.connect("docx.go-to", |this, window, cx| this.open_go_to_panel(window, cx));
    menus.connect("docx.header-footer", |this, _, cx| this.edit_header(false, cx));
    menus.connect("docx.page-number", |this, _, cx| this.insert_page_number(cx));
    menus.connect("docx.page-number.bottom-left", |this, _, cx| this.insert_page_number_at(crate::docx::editing::PageNumberPosition::FooterLeft, cx));
    menus.connect("docx.page-number.bottom-center", |this, _, cx| this.insert_page_number_at(crate::docx::editing::PageNumberPosition::FooterCenter, cx));
    menus.connect("docx.page-number.bottom-right", |this, _, cx| this.insert_page_number_at(crate::docx::editing::PageNumberPosition::FooterRight, cx));
    menus.connect("docx.page-number.top-left", |this, _, cx| this.insert_page_number_at(crate::docx::editing::PageNumberPosition::HeaderLeft, cx));
    menus.connect("docx.page-number.top-center", |this, _, cx| this.insert_page_number_at(crate::docx::editing::PageNumberPosition::HeaderCenter, cx));
    menus.connect("docx.page-number.top-right", |this, _, cx| this.insert_page_number_at(crate::docx::editing::PageNumberPosition::HeaderRight, cx));
    menus.connect("docx.page-number.current", |this, _, cx| this.insert_page_number_at(crate::docx::editing::PageNumberPosition::Current, cx));
    menus.connect("docx.page.borders.none", |this, _, cx| this.set_page_border_preset(crate::docx::editing::PageBorderPreset::None, cx));
    menus.connect("docx.page.borders.box-thin", |this, _, cx| this.set_page_border_preset(crate::docx::editing::PageBorderPreset::BoxThin, cx));
    menus.connect("docx.page.borders.box-medium", |this, _, cx| this.set_page_border_preset(crate::docx::editing::PageBorderPreset::BoxMedium, cx));
    menus.connect("docx.page.borders.box-thick", |this, _, cx| this.set_page_border_preset(crate::docx::editing::PageBorderPreset::BoxThick, cx));
    menus.connect("docx.page.borders.box-double", |this, _, cx| this.set_page_border_preset(crate::docx::editing::PageBorderPreset::BoxDouble, cx));
    menus.connect("docx.page.borders.top-bottom", |this, _, cx| this.set_page_border_preset(crate::docx::editing::PageBorderPreset::TopBottom, cx));
    menus.connect("docx.page.borders.from-page", |this, _, cx| this.set_page_border_offset_from(false, cx));
    menus.connect("docx.page.borders.from-text", |this, _, cx| this.set_page_border_offset_from(true, cx));
    menus.connect("docx.design.paragraph-spacing.default", |this, _, cx| this.set_document_paragraph_spacing(ibis_doc_model::ParagraphSpacingPreset::Default, cx));
    menus.connect("docx.design.paragraph-spacing.none", |this, _, cx| this.set_document_paragraph_spacing(ibis_doc_model::ParagraphSpacingPreset::NoSpace, cx));
    menus.connect("docx.design.paragraph-spacing.compact", |this, _, cx| this.set_document_paragraph_spacing(ibis_doc_model::ParagraphSpacingPreset::Compact, cx));
    menus.connect("docx.design.paragraph-spacing.tight", |this, _, cx| this.set_document_paragraph_spacing(ibis_doc_model::ParagraphSpacingPreset::Tight, cx));
    menus.connect("docx.design.paragraph-spacing.open", |this, _, cx| this.set_document_paragraph_spacing(ibis_doc_model::ParagraphSpacingPreset::Open, cx));
    menus.connect("docx.design.paragraph-spacing.relaxed", |this, _, cx| this.set_document_paragraph_spacing(ibis_doc_model::ParagraphSpacingPreset::Relaxed, cx));
    menus.connect("docx.design.paragraph-spacing.double", |this, _, cx| this.set_document_paragraph_spacing(ibis_doc_model::ParagraphSpacingPreset::Double, cx));
    for preset in &crate::docx::editing::DOCX_DOCUMENT_THEME_PRESETS {
        let tid = preset.id;
        menus.connect(preset.action_id, move |this, _, cx| this.apply_docx_theme(tid, cx));
    }
    menus.connect("docx.design.themes", |this, window, cx| this.open_docx_theme_panel(window, cx));
    menus.connect("docx.design.themes.customize", |this, window, cx| this.open_docx_theme_panel(window, cx));
    for preset in &crate::docx::editing::DOCX_THEME_COLORS_PRESETS {
        let pid = preset.id;
        menus.connect(preset.action_id, move |this, _, cx| this.apply_docx_theme_colors(pid, cx));
    }
    menus.connect("docx.design.theme-colors", |this, window, cx| this.open_docx_theme_panel(window, cx));
    menus.connect("docx.design.theme-colors.customize", |this, window, cx| this.open_docx_theme_panel(window, cx));
    for preset in &crate::docx::editing::DOCX_STYLE_SET_PRESETS {
        let p = preset.preset;
        menus.connect(preset.action_id, move |this, _, cx| this.set_document_style_set(p, cx));
    }
    menus.connect("docx.design.style-set", |this, _, cx| this.set_document_style_set(ibis_doc_model::StyleSetPreset::Default, cx));
    for preset in &crate::docx::editing::DOCX_THEME_FONTS_PRESETS {
        let pid = preset.id;
        menus.connect(preset.action_id, move |this, _, cx| this.apply_docx_theme_fonts(pid, cx));
    }
    menus.connect("docx.design.theme-fonts", |this, _, cx| this.apply_docx_theme_fonts("office", cx));
    for preset in &crate::docx::editing::DOCX_THEME_EFFECTS_PRESETS {
        let pid = preset.id;
        menus.connect(preset.action_id, move |this, _, cx| this.apply_docx_theme_effects(pid, cx));
    }
    menus.connect("docx.design.effects", |this, _, cx| this.apply_docx_theme_effects("office", cx));
    menus.connect("docx.design.set-as-default", |this, _, cx| this.set_docx_design_as_default(cx));
    menus.connect("insert.equation", |this, window, cx| this.open_docx_math_panel(window, cx));
    menus.connect("docx.math.panel", |this, window, cx| this.open_docx_math_panel(window, cx));
    menus.connect("docx.math.builtin", |this, window, cx| this.open_docx_math_panel(window, cx));
    for eq in crate::docx::math_panel::BUILTIN_EQUATIONS {
        let eid = eq.id;
        menus.connect(eq.action_id, move |this, _, cx| { this.insert_docx_math_builtin(eid, cx); });
    }
    menus.connect("docx.math.structures", |this, window, cx| this.open_docx_math_panel(window, cx));
    menus.connect("docx.math.struct.fraction", |this, _, cx| { this.insert_docx_math_structure("fraction", cx); });
    menus.connect("docx.math.struct.fraction-skewed", |this, _, cx| { this.insert_docx_math_structure("fraction-skewed", cx); });
    menus.connect("docx.math.struct.fraction-linear", |this, _, cx| { this.insert_docx_math_structure("fraction-linear", cx); });
    menus.connect("docx.math.struct.fraction-nobar", |this, _, cx| { this.insert_docx_math_structure("fraction-nobar", cx); });
    menus.connect("docx.math.struct.script", |this, _, cx| { this.insert_docx_math_structure("script", cx); });
    menus.connect("docx.math.struct.radical", |this, _, cx| { this.insert_docx_math_structure("radical", cx); });
    menus.connect("docx.math.struct.integral", |this, _, cx| { this.insert_docx_math_structure("integral", cx); });
    menus.connect("docx.math.struct.large-op", |this, _, cx| { this.insert_docx_math_structure("large-op", cx); });
    menus.connect("docx.math.struct.brackets", |this, _, cx| { this.insert_docx_math_structure("brackets", cx); });
    menus.connect("docx.math.struct.function", |this, _, cx| { this.insert_docx_math_structure("function", cx); });
    menus.connect("docx.math.struct.accent", |this, _, cx| { this.insert_docx_math_structure("accent", cx); });
    menus.connect("docx.math.struct.limit", |this, _, cx| { this.insert_docx_math_structure("limit", cx); });
    menus.connect("docx.math.struct.matrix", |this, _, cx| { this.insert_docx_math_structure("matrix", cx); });
    menus.connect("docx.math.convert", |this, _, cx| { this.convert_docx_math_mode(false, cx); });
    menus.connect("docx.math.convert.professional", |this, _, cx| { this.convert_docx_math_mode(false, cx); });
    menus.connect("docx.math.convert.linear", |this, _, cx| { this.convert_docx_math_mode(true, cx); });
    menus.connect("docx.math.convert.display", |this, _, cx| { this.convert_docx_math_mode(false, cx); });
    menus.connect("docx.math.convert.inline", |this, _, cx| { this.convert_docx_math_mode(false, cx); });
    menus.connect("docx.quick-parts", |this, window, cx| this.open_quick_parts_panel(window, cx));
    menus.connect("docx.quick-parts.panel", |this, window, cx| this.open_quick_parts_panel(window, cx));
    menus.connect("docx.quick-parts.save-selection", |this, window, cx| this.open_save_building_block_dialog(window, cx));
    menus.connect("docx.quick-parts.autotext", |this, _, cx| { this.insert_quick_part_field("AUTOTEXT", cx); });
    menus.connect("docx.quick-parts.field", |this, window, cx| this.open_field_panel(window, cx));
    menus.connect("docx.quick-parts.organizer", |this, window, cx| this.open_quick_parts_panel(window, cx));
    menus.connect("docx.quick-parts.prop.title", |this, _, cx| { this.insert_quick_part_field("TITLE", cx); });
    menus.connect("docx.quick-parts.prop.author", |this, _, cx| { this.insert_quick_part_field("AUTHOR", cx); });
    menus.connect("docx.quick-parts.prop.subject", |this, _, cx| { this.insert_quick_part_field("SUBJECT", cx); });
    menus.connect("docx.quick-parts.prop.company", |this, _, cx| { this.insert_quick_part_field(r#"DOCPROPERTY "Company""#, cx); });
    menus.connect("docx.quick-parts.prop.comments", |this, _, cx| { this.insert_quick_part_field("COMMENTS", cx); });
    menus.connect("docx.quick-parts.prop.createdate", |this, _, cx| { this.insert_quick_part_field("CREATEDATE", cx); });
    menus.connect("docx.quick-parts.prop.savedate", |this, _, cx| { this.insert_quick_part_field("SAVEDATE", cx); });
    menus.connect("docx.quick-parts.prop.page", |this, _, cx| { this.insert_quick_part_field("PAGE", cx); });
    menus.connect("docx.quick-parts.prop.numpages", |this, _, cx| { this.insert_quick_part_field("NUMPAGES", cx); });
    menus.connect("docx.quick-parts.prop.filename", |this, _, cx| { this.insert_quick_part_field("FILENAME", cx); });
    // 挿入 > ページ番号の位置.
    {
        use crate::docx::editing::PageNumberPosition;
        let pos_items = vec![
            crate::shell::chrome::Submenu::tr(
                "docx.page-number.bottom",
                "vertical_align_bottom",
                vec![
                    MenuItem::tr("docx.page-number.bottom-left").on(|this, _, cx| this.insert_page_number_at(PageNumberPosition::FooterLeft, cx)).into(),
                    MenuItem::tr("docx.page-number.bottom-center").on(|this, _, cx| this.insert_page_number_at(PageNumberPosition::FooterCenter, cx)).into(),
                    MenuItem::tr("docx.page-number.bottom-right").on(|this, _, cx| this.insert_page_number_at(PageNumberPosition::FooterRight, cx)).into(),
                ],
            )
            .into(),
            crate::shell::chrome::Submenu::tr(
                "docx.page-number.top",
                "vertical_align_top",
                vec![
                    MenuItem::tr("docx.page-number.top-left").on(|this, _, cx| this.insert_page_number_at(PageNumberPosition::HeaderLeft, cx)).into(),
                    MenuItem::tr("docx.page-number.top-center").on(|this, _, cx| this.insert_page_number_at(PageNumberPosition::HeaderCenter, cx)).into(),
                    MenuItem::tr("docx.page-number.top-right").on(|this, _, cx| this.insert_page_number_at(PageNumberPosition::HeaderRight, cx)).into(),
                ],
            )
            .into(),
            MenuEntry::Separator,
            MenuItem::tr("docx.page-number.current").icon("navigation").on(|this, _, cx| this.insert_page_number_at(PageNumberPosition::Current, cx)).into(),
        ];
        let entries = menus.items_mut(MenuId::Insert);
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "docx.page-number")).map_or(entries.len(), |i| i + 1);
        entries.insert(at, crate::shell::chrome::Submenu::tr("docx.page-number.position", "format_shapes", pos_items).into());
    }
    menus.connect("docx.section.page-start-continue", |this, _, cx| this.set_page_number_start(None, cx));
    menus.connect("docx.section.page-start-1", |this, _, cx| this.set_page_number_start(Some(1), cx));
    menus.connect("docx.section.page-start-2", |this, _, cx| this.set_page_number_start(Some(2), cx));
    menus.connect("docx.section.page-start-3", |this, _, cx| this.set_page_number_start(Some(3), cx));
    menus.connect("docx.section.page-start-5", |this, _, cx| this.set_page_number_start(Some(5), cx));
    menus.connect("docx.section.page-start-10", |this, _, cx| this.set_page_number_start(Some(10), cx));
    menus.connect("docx.section.page-start-custom", |this, window, cx| this.open_page_setup_panel(window, cx));
    menus.connect("docx.section.show-document-text", |this, _, cx| this.toggle_show_document_text(cx));
    menus.connect("docx.page.doc-grid", |this, window, cx| this.open_page_setup_panel(window, cx));
    menus.connect("docx.page.multiple-pages.normal", |this, _, cx| this.set_multiple_pages(ibis_doc_model::MultiplePages::Normal, None, cx));
    menus.connect("docx.page.multiple-pages.mirror-margins", |this, _, cx| this.set_multiple_pages(ibis_doc_model::MultiplePages::MirrorMargins, None, cx));
    menus.connect("docx.page.multiple-pages.book-fold", |this, _, cx| {
        let sheets = this.caret_book_fold_sheets();
        this.set_multiple_pages(ibis_doc_model::MultiplePages::BookFold, sheets, cx);
    });
    menus.connect("docx.page.multiple-pages.reverse-book-fold", |this, _, cx| {
        let sheets = this.caret_book_fold_sheets();
        this.set_multiple_pages(ibis_doc_model::MultiplePages::ReverseBookFold, sheets, cx);
    });
    menus.connect("docx.page.multiple-pages.two-on-one", |this, _, cx| this.set_multiple_pages(ibis_doc_model::MultiplePages::TwoOnOne, None, cx));
    // 挿入 > 表紙.
    {
        let editing = _editor.ordered_selection().is_some();
        let covers: Vec<MenuEntry> = ["docx.cover.1", "docx.cover.2", "docx.cover.3"].into_iter().enumerate().map(|(i, id)| MenuItem::tr(id).disabled(!editing).on(move |this, _, cx| this.insert_cover_page(i, cx)).into()).collect();
        let entries = menus.items_mut(MenuId::Insert);
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "docx.page-break")).map_or(0, |i| i + 1);
        entries.insert(at, crate::shell::chrome::Submenu::tr("docx.cover", "description", covers).into());
    }
    // 挿入 > 白紙のページ (after 改ページ).
    {
        let editing = _editor.ordered_selection().is_some();
        let entries = menus.items_mut(MenuId::Insert);
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "docx.page-break")).map_or(0, |i| i + 1);
        entries.insert(at, MenuItem::tr("docx.blank-page").disabled(!editing).on(|this, _, cx| this.type_text("\u{c}\n\u{c}", cx)).into());
    }
    // 挿入 > セクション区切り (after 改ページ).
    {
        let enabled=_editor.can_insert_section_break();
        let entries = menus.items_mut(MenuId::Insert);
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "docx.page-break")).map_or(0, |i| i + 1);
        entries.insert(at, MenuItem::tr("docx.section-next").disabled(!enabled).on(|this, _, cx| this.insert_section_break(ibis_doc_model::SectionBreak::NextPage, cx)).into());
        entries.insert(at + 1, MenuItem::tr("docx.section-continuous").disabled(!enabled).on(|this, _, cx| this.insert_section_break(ibis_doc_model::SectionBreak::Continuous, cx)).into());
        entries.insert(at + 2, MenuItem::tr("docx.section-odd").disabled(!enabled).on(|this, _, cx| this.insert_section_break(ibis_doc_model::SectionBreak::OddPage, cx)).into());
        entries.insert(at + 3, MenuItem::tr("docx.section-even").disabled(!enabled).on(|this, _, cx| this.insert_section_break(ibis_doc_model::SectionBreak::EvenPage, cx)).into());
    }
    // 挿入 > 索引, 索引登録.
    {
        let editing = _editor.ordered_selection().is_some();
        let entries = menus.items_mut(MenuId::Insert);
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "docx.toc")).map_or(entries.len(), |i| i + 1);
        entries.insert(at, MenuItem::tr("docx.index").icon("list_alt").disabled(!editing).on(|this, _, cx| this.insert_index(cx)).into());
        entries.insert(at + 1, MenuItem::tr("docx.index.mark").icon("bookmark_add").disabled(!editing).on(|this, _, cx| this.mark_index_entry(cx)).into());
        let levels: Vec<MenuEntry> = [("docx.toc.mark-level-1", 1u32), ("docx.toc.mark-level-2", 2), ("docx.toc.mark-level-3", 3)]
            .into_iter()
            .map(|(id, n)| MenuItem::tr(id).disabled(!editing).on(move |this, _, cx| this.mark_toc_entry(n, cx)).into())
            .collect();
        entries.insert(at + 2, crate::shell::chrome::Submenu::tr("docx.toc.mark-entry", "bookmark_add", levels).into());
        entries.insert(at + 3, MenuItem::tr("docx.toc.from-entries").icon("toc").disabled(!editing).on(|this, _, cx| this.insert_toc_from_entries(cx)).into());
        let categories: Vec<MenuEntry> = [("docx.toa.mark-category-1", 1u32), ("docx.toa.mark-category-2", 2), ("docx.toa.mark-category-3", 3), ("docx.toa.mark-category-4", 4), ("docx.toa.mark-category-5", 5), ("docx.toa.mark-category-6", 6)]
            .into_iter()
            .map(|(id, n)| MenuItem::tr(id).disabled(!editing).on(move |this, _, cx| this.mark_citation(n, cx)).into())
            .collect();
        entries.insert(at + 4, crate::shell::chrome::Submenu::tr("docx.toa.mark", "bookmark_add", categories).into());
        entries.insert(at + 5, MenuItem::tr("docx.toa").icon("list_alt").disabled(!editing).on(|this, _, cx| this.insert_toa(cx)).into());
        entries.insert(at + 6, MenuItem::tr("docx.reference.insert-citation").icon("format_quote").disabled(!editing).on(|this, _, cx| this.insert_citation_at_selection(cx)).into());
        entries.insert(at + 7, MenuItem::tr("docx.reference.bibliography").icon("auto_stories").disabled(!editing).on(|this, _, cx| this.insert_bibliography(cx)).into());
        entries.insert(at + 8, MenuItem::tr("docx.reference.bibliography.update").icon("refresh").disabled(!editing).on(|this, _, cx| this.update_bibliography(cx)).into());
        entries.insert(at + 9, MenuItem::tr("docx.reference.source-manager").icon("manage_accounts").disabled(!editing).on(|this, window, cx| this.open_source_manager_panel(window, cx)).into());
    }
    // 挿入 > 図形: the preset shapes, at the caret.
    {
        let editing = _editor.ordered_selection().is_some();
        let items: Vec<MenuEntry> = crate::pptx::editing::SHAPES
            .iter()
            .map(|&(preset, icon, id)| MenuItem::tr(id).icon(icon).disabled(!editing).on(move |this, _, cx| this.arm_draw_shape(super::draw_table::DrawKind::Shape(preset), cx)).into())
            .collect();
        for entry in menus.items_mut(MenuId::Insert) {
            if let MenuEntry::Submenu(sub) = entry
                && sub.id == "insert.shape"
            {
                sub.items = vec![
                    MenuItem::tr("docx.shape.line").icon("remove").disabled(!editing).on(|this, _, cx| this.arm_draw_shape(super::draw_table::DrawKind::Line { arrow: false }, cx)).into(),
                    MenuItem::tr("docx.shape.arrow").icon("arrow_right_alt").disabled(!editing).on(|this, _, cx| this.arm_draw_shape(super::draw_table::DrawKind::Line { arrow: true }, cx)).into(),
                    MenuItem::tr("docx.shape.elbow").icon("turn_right").disabled(!editing).on(|this, _, cx| this.arm_draw_shape(super::draw_table::DrawKind::Connector { curved: false, arrow: false }, cx)).into(),
                    MenuItem::tr("docx.shape.elbow-arrow").icon("turn_right").disabled(!editing).on(|this, _, cx| this.arm_draw_shape(super::draw_table::DrawKind::Connector { curved: false, arrow: true }, cx)).into(),
                    MenuItem::tr("docx.shape.curved").icon("gesture").disabled(!editing).on(|this, _, cx| this.arm_draw_shape(super::draw_table::DrawKind::Connector { curved: true, arrow: false }, cx)).into(),
                    MenuItem::tr("docx.shape.curved-arrow").icon("gesture").disabled(!editing).on(|this, _, cx| this.arm_draw_shape(super::draw_table::DrawKind::Connector { curved: true, arrow: true }, cx)).into(),
                    MenuEntry::Separator,
                ];
                sub.items.extend(items.clone());
            }
        }
    }
    // 挿入 > テキスト ボックス (a text box at the caret) and its text.
    {
        let editing = _editor.ordered_selection().is_some();
        let can_edit = _editor.text_box_at_caret().is_some();
        let entries = menus.items_mut(MenuId::Insert);
        if let Some(at) = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "insert.text-box")) {
            entries[at] = MenuItem::tr("insert.text-box").icon("text_fields").disabled(!editing).on(|this, window, cx| this.doc_insert_text_box(window, cx)).into();
            entries.insert(at + 1, MenuItem::new("docx.textbox.edit", crate::shell::i18n::text("docx.toolbar.shape-text-box-format")).disabled(!can_edit).on(|this, window, cx| this.open_text_box(window, cx)).into());
        }
    }
    // 挿入 > ドロップ キャップ
    {
        let editing = _editor.ordered_selection().is_some();
        let dc = _editor.caret_drop_cap();
        let loc = dc.as_ref().map_or(ibis_doc_model::DropCapLocation::None, |d| d.location);
        let lines = dc.as_ref().map_or(3, |d| d.lines);
        let space = dc.as_ref().map_or(0, |d| d.space);

        let lines_items = vec![
            MenuItem::tr("docx.drop-cap.lines.2").checked(loc != ibis_doc_model::DropCapLocation::None && lines == 2).disabled(!editing).on(|this, _, cx| this.set_drop_cap_lines(2, cx)).into(),
            MenuItem::tr("docx.drop-cap.lines.3").checked(loc != ibis_doc_model::DropCapLocation::None && lines == 3).disabled(!editing).on(|this, _, cx| this.set_drop_cap_lines(3, cx)).into(),
            MenuItem::tr("docx.drop-cap.lines.4").checked(loc != ibis_doc_model::DropCapLocation::None && lines == 4).disabled(!editing).on(|this, _, cx| this.set_drop_cap_lines(4, cx)).into(),
            MenuItem::tr("docx.drop-cap.lines.5").checked(loc != ibis_doc_model::DropCapLocation::None && lines == 5).disabled(!editing).on(|this, _, cx| this.set_drop_cap_lines(5, cx)).into(),
        ];
        let dist_items = vec![
            MenuItem::tr("docx.drop-cap.distance.0").checked(loc != ibis_doc_model::DropCapLocation::None && space == 0).disabled(!editing).on(|this, _, cx| this.set_drop_cap_distance(0, cx)).into(),
            MenuItem::tr("docx.drop-cap.distance.3").checked(loc != ibis_doc_model::DropCapLocation::None && space == 3 * 12700).disabled(!editing).on(|this, _, cx| this.set_drop_cap_distance(3 * 12700, cx)).into(),
            MenuItem::tr("docx.drop-cap.distance.6").checked(loc != ibis_doc_model::DropCapLocation::None && space == 6 * 12700).disabled(!editing).on(|this, _, cx| this.set_drop_cap_distance(6 * 12700, cx)).into(),
            MenuItem::tr("docx.drop-cap.distance.12").checked(loc != ibis_doc_model::DropCapLocation::None && space == 12 * 12700).disabled(!editing).on(|this, _, cx| this.set_drop_cap_distance(12 * 12700, cx)).into(),
        ];

        let drop_cap_sub = vec![
            MenuItem::tr("docx.drop-cap.none").checked(loc == ibis_doc_model::DropCapLocation::None).disabled(!editing).on(|this, _, cx| this.set_drop_cap_location(ibis_doc_model::DropCapLocation::None, cx)).into(),
            MenuItem::tr("docx.drop-cap.drop").checked(loc == ibis_doc_model::DropCapLocation::Drop).disabled(!editing).on(|this, _, cx| this.set_drop_cap_location(ibis_doc_model::DropCapLocation::Drop, cx)).into(),
            MenuItem::tr("docx.drop-cap.margin").checked(loc == ibis_doc_model::DropCapLocation::Margin).disabled(!editing).on(|this, _, cx| this.set_drop_cap_location(ibis_doc_model::DropCapLocation::Margin, cx)).into(),
            MenuEntry::Separator,
            crate::shell::chrome::Submenu::tr("docx.drop-cap.lines", "format_line_spacing", lines_items).into(),
            crate::shell::chrome::Submenu::tr("docx.drop-cap.distance", "space_bar", dist_items).into(),
        ];
        let entries = menus.items_mut(MenuId::Insert);
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "docx.textbox.edit" || i.id == "insert.text-box")).map_or(entries.len(), |i| i + 1);
        entries.insert(at, crate::shell::chrome::Submenu::tr("docx.drop-cap", "format_size", drop_cap_sub).into());
    }
    // 挿入 > クイック パーツ
    {
        let editing = _editor.ordered_selection().is_some();
        let prop_items: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.quick-parts.prop.title").disabled(!editing).on(|this, _, cx| { this.insert_quick_part_field("TITLE", cx); }).into(),
            MenuItem::tr("docx.quick-parts.prop.author").disabled(!editing).on(|this, _, cx| { this.insert_quick_part_field("AUTHOR", cx); }).into(),
            MenuItem::tr("docx.quick-parts.prop.subject").disabled(!editing).on(|this, _, cx| { this.insert_quick_part_field("SUBJECT", cx); }).into(),
            MenuItem::tr("docx.quick-parts.prop.company").disabled(!editing).on(|this, _, cx| { this.insert_quick_part_field(r#"DOCPROPERTY "Company""#, cx); }).into(),
            MenuItem::tr("docx.quick-parts.prop.comments").disabled(!editing).on(|this, _, cx| { this.insert_quick_part_field("COMMENTS", cx); }).into(),
            MenuItem::tr("docx.quick-parts.prop.createdate").disabled(!editing).on(|this, _, cx| { this.insert_quick_part_field("CREATEDATE", cx); }).into(),
            MenuItem::tr("docx.quick-parts.prop.savedate").disabled(!editing).on(|this, _, cx| { this.insert_quick_part_field("SAVEDATE", cx); }).into(),
            MenuItem::tr("docx.quick-parts.prop.page").disabled(!editing).on(|this, _, cx| { this.insert_quick_part_field("PAGE", cx); }).into(),
            MenuItem::tr("docx.quick-parts.prop.numpages").disabled(!editing).on(|this, _, cx| { this.insert_quick_part_field("NUMPAGES", cx); }).into(),
            MenuItem::tr("docx.quick-parts.prop.filename").disabled(!editing).on(|this, _, cx| { this.insert_quick_part_field("FILENAME", cx); }).into(),
        ];
        let qp_items: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.quick-parts.save-selection").icon("bookmark_add").disabled(!editing).on(|this, window, cx| this.open_save_building_block_dialog(window, cx)).into(),
            MenuItem::tr("docx.quick-parts.panel").icon("auto_awesome").disabled(!editing).on(|this, window, cx| this.open_quick_parts_panel(window, cx)).into(),
            MenuItem::tr("docx.quick-parts.autotext").disabled(!editing).on(|this, _, cx| { this.insert_quick_part_field("AUTOTEXT", cx); }).into(),
            crate::shell::chrome::Submenu::tr("docx.quick-parts.prop", "badge", prop_items).into(),
            MenuItem::tr("docx.quick-parts.field").icon("code").disabled(!editing).on(|this, window, cx| this.open_field_panel(window, cx)).into(),
            MenuItem::tr("docx.quick-parts.organizer").disabled(!editing).on(|this, window, cx| this.open_quick_parts_panel(window, cx)).into(),
        ];
        let entries = menus.items_mut(MenuId::Insert);
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Submenu(s) if s.id == "docx.drop-cap")).map_or(entries.len(), |i| i + 1);
        entries.insert(at, crate::shell::chrome::Submenu::tr("docx.quick-parts", "auto_awesome", qp_items).into());
    }
    // 挿入 > 数式: 組込数式、数式パネル、構造、変換
    {
        let editing = _editor.ordered_selection().is_some();
        let builtin_items: Vec<MenuEntry> = crate::docx::math_panel::BUILTIN_EQUATIONS
            .iter()
            .map(|eq| {
                let eid = eq.id;
                MenuItem::tr(eq.label_key)
                    .icon("functions")
                    .disabled(!editing)
                    .on(move |this, _, cx| {
                        this.insert_docx_math_builtin(eid, cx);
                    })
                    .into()
            })
            .collect();
        let struct_items: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.math.struct.fraction").icon("functions").disabled(!editing).on(|this, _, cx| { this.insert_docx_math_structure("fraction", cx); }).into(),
            MenuItem::tr("docx.math.struct.script").icon("superscript").disabled(!editing).on(|this, _, cx| { this.insert_docx_math_structure("script", cx); }).into(),
            MenuItem::tr("docx.math.struct.radical").icon("square_root").disabled(!editing).on(|this, _, cx| { this.insert_docx_math_structure("radical", cx); }).into(),
            MenuItem::tr("docx.math.struct.integral").icon("functions").disabled(!editing).on(|this, _, cx| { this.insert_docx_math_structure("integral", cx); }).into(),
            MenuItem::tr("docx.math.struct.large-op").icon("functions").disabled(!editing).on(|this, _, cx| { this.insert_docx_math_structure("large-op", cx); }).into(),
            MenuItem::tr("docx.math.struct.brackets").icon("data_array").disabled(!editing).on(|this, _, cx| { this.insert_docx_math_structure("brackets", cx); }).into(),
            MenuItem::tr("docx.math.struct.function").icon("functions").disabled(!editing).on(|this, _, cx| { this.insert_docx_math_structure("function", cx); }).into(),
            MenuItem::tr("docx.math.struct.accent").icon("functions").disabled(!editing).on(|this, _, cx| { this.insert_docx_math_structure("accent", cx); }).into(),
            MenuItem::tr("docx.math.struct.limit").icon("functions").disabled(!editing).on(|this, _, cx| { this.insert_docx_math_structure("limit", cx); }).into(),
            MenuItem::tr("docx.math.struct.matrix").icon("grid_on").disabled(!editing).on(|this, _, cx| { this.insert_docx_math_structure("matrix", cx); }).into(),
        ];
        let convert_items: Vec<MenuEntry> = vec![
            MenuItem::tr("docx.math.convert.professional").disabled(!editing).on(|this, _, cx| { this.convert_docx_math_mode(false, cx); }).into(),
            MenuItem::tr("docx.math.convert.linear").disabled(!editing).on(|this, _, cx| { this.convert_docx_math_mode(true, cx); }).into(),
            MenuItem::tr("docx.math.convert.display").disabled(!editing).on(|this, _, cx| { this.convert_docx_math_mode(false, cx); }).into(),
            MenuItem::tr("docx.math.convert.inline").disabled(!editing).on(|this, _, cx| { this.convert_docx_math_mode(false, cx); }).into(),
        ];
        let entries = menus.items_mut(MenuId::Insert);
        if let Some(at) = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "insert.equation")) {
            entries[at] = MenuItem::tr("insert.equation").icon("functions").disabled(!editing).on(|this, window, cx| this.open_docx_math_panel(window, cx)).into();
            entries.insert(at + 1, crate::shell::chrome::Submenu::tr("docx.math.builtin", "functions", builtin_items).into());
            entries.insert(at + 2, crate::shell::chrome::Submenu::tr("docx.math.structures", "functions", struct_items).into());
            entries.insert(at + 3, crate::shell::chrome::Submenu::tr("docx.math.convert", "transform", convert_items).into());
        }
    }
    // 挿入 > 画像 > ファイルから…
    for entry in menus.items_mut(MenuId::Insert) {
        if let MenuEntry::Submenu(sub) = entry
            && sub.id == "insert.image"
        {
            sub.items.push(MenuItem::tr("docx.image.file").icon("upload_file").on(|this, window, cx| this.doc_pick_picture(window, cx)).into());
        }
    }
    // 挿入 > 表: tables of a few sizes.
    for entry in menus.items_mut(MenuId::Insert) {
        if let MenuEntry::Submenu(sub) = entry
            && sub.id == "insert.table"
        {
            for (id, label, rows, cols) in [("docx.table.insert-2x2", "2 × 2", 2, 2), ("docx.table.insert-3x3", "3 × 3", 3, 3), ("docx.table.insert-4x4", "4 × 4", 4, 4), ("docx.table.insert-5x3", "docx.toolbar.5-rows-times-3-columns", 5, 3)] {
                sub.items.push(MenuItem::new(id, crate::shell::i18n::text(label)).on(move |this, _, cx| this.insert_table(rows, cols, cx)).into());
            }
        }
    }
    // 挿入 > アイコン, SmartArt, クイック パーツ, ワードアート, 署名欄
    {
        let editing = _editor.ordered_selection().is_some();
        let entries = menus.items_mut(MenuId::Insert);
        entries.push(MenuItem::tr("docx.insert.icon").icon("category").disabled(!editing).on(|this, _, cx| { this.doc_insert_icon("auto_awesome", cx); }).into());
        entries.push(crate::shell::chrome::Submenu::tr("docx.insert.icon-categories", "category", crate::docx::insert_objects::docx_icon_insert_items()).into());
        entries.push(MenuItem::tr("insert.smartart").icon("account_tree").disabled(!editing).on(|this, _, cx| { this.doc_insert_smartart("process", cx); }).into());
        entries.push(crate::shell::chrome::Submenu::tr("docx.insert.smartart-presets", "account_tree", crate::docx::insert_objects::docx_smartart_insert_items()).into());
        entries.push(MenuItem::tr("docx.quick-parts").icon("extension").disabled(!editing).on(|this, window, cx| { this.open_quick_parts_panel(window, cx); }).into());
        entries.push(crate::shell::chrome::Submenu::tr("docx.quick-parts-menu", "extension", crate::docx::insert_objects::docx_quick_parts_items()).into());
        entries.push(MenuItem::tr("docx.insert.wordart").icon("title").disabled(!editing).on(|this, _, cx| { this.doc_insert_wordart(0, None, cx); }).into());
        entries.push(crate::shell::chrome::Submenu::tr("docx.insert.wordart-styles", "title", crate::docx::insert_objects::docx_wordart_insert_items()).into());
        entries.push(MenuItem::tr("docx.signature-line").icon("draw").disabled(!editing).on(|this, _, cx| { this.doc_insert_signature_line("", "", cx); }).into());
    }
    // 表示形式 > 表: rows, columns and the table, where the caret is in one.
    let in_table = _editor.can_select_table();
    let item = |id: &'static str, label: &'static str, f: fn(&mut SlideEditor, &mut Context<SlideEditor>)| -> MenuEntry {
        let i = MenuItem::new(id, crate::shell::i18n::text(label));
        (if in_table { i.on(move |this, _, cx| f(this, cx)) } else { i.disabled(true) }).into()
    };
    let table_menu = vec![
        crate::shell::chrome::Submenu::tr(
            "docx.table.select",
            "select_all",
            vec![
                item("docx.table.select.cell", "docx.toolbar.select-cell", |this, cx| this.select_table_cell(cx)),
                item("docx.table.select.column", "docx.toolbar.select-column", |this, cx| this.select_table_column(cx)),
                item("docx.table.select.row", "docx.toolbar.select-row", |this, cx| this.select_table_row(cx)),
                item("docx.table.select.table", "docx.toolbar.select-table", |this, cx| this.select_table(cx)),
            ],
        ).into(),
        MenuEntry::Separator,
        item("docx.table.row-above", "_shell.insert-row-above", |this, cx| this.edit_table(|r, _, _| ibis_doc_model::TableEdit::InsertRow { row: r, below: false }, cx)),
        item("docx.table.row-below", "_shell.insert-row-below", |this, cx| this.edit_table(|r, _, _| ibis_doc_model::TableEdit::InsertRow { row: r, below: true }, cx)),
        item("docx.table.column-left", "_shell.insert-column-left", |this, cx| this.edit_table(|_, _, g| ibis_doc_model::TableEdit::InsertColumn { column: g, right: false }, cx)),
        item("docx.table.column-right", "_shell.insert-column-right", |this, cx| this.edit_table(|_, _, g| ibis_doc_model::TableEdit::InsertColumn { column: g, right: true }, cx)),
        crate::shell::chrome::Submenu::tr(
            "docx.table.cell-insert",
            "add",
            vec![
                item("docx.table.cell-insert.shift-right", "_shell.insert-cells-shift-cells-right", |this, cx| this.edit_table(|r, c, _| ibis_doc_model::TableEdit::InsertCellShiftRight { row: r, cell: c }, cx)),
                item("docx.table.cell-insert.shift-down", "docx.context-menu.insert-cells-shift-down", |this, cx| this.edit_table(|r, c, _| ibis_doc_model::TableEdit::InsertCellShiftDown { row: r, cell: c }, cx)),
            ],
        ).into(),
        MenuEntry::Separator,
        item("docx.table.delete-row", "docx.context-menu.delete-row", |this, cx| this.edit_table(|r, _, _| ibis_doc_model::TableEdit::DeleteRows { first: r, last: r }, cx)),
        item("docx.table.delete-column", "docx.context-menu.delete-column", |this, cx| this.edit_table(|_, _, g| ibis_doc_model::TableEdit::DeleteColumn { column: g }, cx)),
        crate::shell::chrome::Submenu::tr(
            "docx.table.cell-delete",
            "delete",
            vec![
                item("docx.table.cell-delete.shift-left", "_shell.delete-cells-shift-cells-left", |this, cx| this.edit_table(|r, c, _| ibis_doc_model::TableEdit::DeleteCellShiftLeft { row: r, cell: c }, cx)),
                item("docx.table.cell-delete.shift-up", "docx.context-menu.delete-cells-shift-up", |this, cx| this.edit_table(|r, c, _| ibis_doc_model::TableEdit::DeleteCellShiftUp { row: r, cell: c }, cx)),
            ],
        ).into(),
        item("docx.table.delete", "docx.toolbar.delete-table", |this, cx| this.delete_table(cx)),
        MenuEntry::Separator,
        item("docx.table.equal-columns", "docx.toolbar.distribute-columns-evenly", |this, cx| this.edit_table(|_, _, _| ibis_doc_model::TableEdit::EqualColumns, cx)),
        item("docx.table.equal-rows", "docx.toolbar.distribute-rows-evenly", |this, cx| this.edit_table(|_, _, _| ibis_doc_model::TableEdit::EqualRows, cx)),
        crate::shell::chrome::Submenu::tr(
            "docx.table.autofit",
            "fit_screen",
            vec![
                item("docx.table.autofit-contents", "docx.toolbar.autofit-contents", |this, cx| this.edit_table(|_, _, _| ibis_doc_model::TableEdit::AutoFitContents, cx)),
                item("docx.table.autofit-window", "docx.toolbar.autofit-window", |this, cx| {
                    let avail = this.flow().map_or(150 * 36000, |d| (d.page.size.width - d.page.left - d.page.right).max(100 * 36000));
                    this.edit_table(move |_, _, _| ibis_doc_model::TableEdit::AutoFitWindow(avail), cx);
                }),
                item("docx.table.autofit-fixed", "docx.toolbar.fixed-column-width", |this, cx| this.edit_table(|_, _, _| ibis_doc_model::TableEdit::FixedColumns, cx)),
            ],
        ).into(),
        MenuEntry::Separator,
    ];
    let mut table_menu = table_menu;
    table_menu.extend(super::cell_borders::menu(_editor));
    table_menu.push(MenuItem::tr("docx.cell.merge-right").disabled(!_editor.can_merge_right_cell()).on(|this,_,cx|this.merge_right_cell(cx)).into());
    table_menu.push(MenuItem::tr("docx.cell.merge-below").disabled(!_editor.can_merge_below_cell()).on(|this,_,cx|this.merge_below_cell(cx)).into());
    table_menu.push(MenuItem::tr("docx.cell.unmerge-below").disabled(!_editor.can_unmerge_below_cell()).on(|this,_,cx|this.unmerge_below_cell(cx)).into());
    table_menu.push(MenuItem::tr("docx.table.to-text").disabled(!_editor.can_table_to_text()).on(|this,_,cx|this.table_to_text(cx)).into());
    table_menu.push(MenuItem::tr("docx.table.from-text").disabled(!_editor.can_text_to_table()).on(|this,_,cx|this.text_to_table(cx)).into());
    table_menu.push(MenuItem::tr("docx.cell.split-grid").disabled(!_editor.can_split_cell_grid()).on(|this,_,cx|this.split_cell_grid(cx)).into());
    table_menu.push(MenuItem::tr("docx.table.split").shortcut(if cfg!(target_os = "macos") { "⇧⌘↩" } else { "Ctrl+Shift+Enter" }).disabled(!_editor.can_split_table()).on(|this,_,cx|this.split_table(cx)).into());
    table_menu.push(
        crate::shell::chrome::Submenu::tr(
            "docx.table.sort",
            "sort",
            vec![
                item("docx.table.sort.asc", "docx.toolbar.sort-ascending-header-row", |this, cx| this.sort_table(true, true, cx)),
                item("docx.table.sort.desc", "docx.toolbar.sort-descending-header-row", |this, cx| this.sort_table(false, true, cx)),
                item("docx.table.sort.asc-all", "docx.toolbar.sort-ascending-no-header-row", |this, cx| this.sort_table(true, false, cx)),
                item("docx.table.sort.desc-all", "docx.toolbar.sort-descending-no-header-row", |this, cx| this.sort_table(false, false, cx)),
            ],
        )
        .into(),
    );
    // Word's 表のプロパティ > 行: タイトル行の繰り返し・行の途中で改ページする.
    let (header, cant_split) = _editor.caret_row_flags().unwrap_or_default();
    table_menu.push(
        MenuItem::tr("docx.table.repeat-header")
            .checked(header)
            .disabled(!in_table)
            .on(move |this, _, cx| this.edit_table(|r, _, _| ibis_doc_model::TableEdit::RepeatHeader { row: r, on: !header }, cx))
            .into(),
    );
    table_menu.push(
        MenuItem::tr("docx.table.cant-split")
            .checked(cant_split)
            .disabled(!in_table)
            .on(move |this, _, cx| this.edit_table(|r, _, _| ibis_doc_model::TableEdit::CantSplit { rows: vec![r], on: !cant_split }, cx))
            .into(),
    );
    table_menu.push(MenuItem::tr("docx.table.size").disabled(!in_table).on(|this,window,cx|this.open_table_size(window,cx)).into());
    table_menu.push(MenuItem::tr("docx.table.formula").icon("functions").disabled(!in_table).on(|this,window,cx|this.open_table_formula(window,cx)).into());
    let height_rule=_editor.doc_state().and_then(|s|s.table_size.as_ref()).map(|s|s.rule);
    for (id,label,rule) in [("docx.table.height.auto","docx.toolbar.row-height-auto",super::table_size_panel::HeightRule::Auto),
        ("docx.table.height.minimum","docx.toolbar.row-height-least",super::table_size_panel::HeightRule::AtLeast),
        ("docx.table.height.exact","docx.toolbar.row-height-exactly",super::table_size_panel::HeightRule::Exact)] {
        table_menu.push(MenuItem::new(id,crate::shell::i18n::text(label)).disabled(!in_table).checked(height_rule==Some(rule)).on(move|this,window,cx|{
            if !this.doc_state().is_some_and(|s|s.table_size.is_some()) {this.open_table_size(window,cx);}
            this.choose_table_height(rule,window,cx);
        }).into());
    }
    let anchor = _editor.caret_cell_anchor().unwrap_or(ibis_doc_model::CellAnchor::Top);
    let anchor_item = |id: &'static str, icon: &'static str, label: &'static str, a: ibis_doc_model::CellAnchor| -> MenuEntry {
        let i = MenuItem::new(id, crate::shell::i18n::text(label)).icon(icon).checked(in_table && anchor == a);
        (if in_table {
            i.on(move |this, _, cx| {
                if this.caret_cell_anchor() == Some(a) { return; }
                this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Anchor { cells: vec![(r, c)], anchor: a }, cx);
            })
        } else {
            i.disabled(true)
        }).into()
    };
    table_menu.push(MenuEntry::Separator);
    table_menu.push(anchor_item("docx.table.valign-top", "vertical_align_top", "docx.toolbar.cell-vertical-alignment-top", ibis_doc_model::CellAnchor::Top));
    table_menu.push(anchor_item("docx.table.valign-center", "vertical_align_center", "docx.toolbar.cell-vertical-alignment-center", ibis_doc_model::CellAnchor::Center));
    table_menu.push(anchor_item("docx.table.valign-bottom", "vertical_align_bottom", "docx.toolbar.cell-vertical-alignment-bottom", ibis_doc_model::CellAnchor::Bottom));
    let direction = _editor.caret_cell_direction().unwrap_or(ibis_doc_model::CellDirection::LrTb);
    let direction_item = |id: &'static str, d: ibis_doc_model::CellDirection| -> MenuEntry {
        let i = MenuItem::tr(id).checked(in_table && direction == d);
        (if in_table {
            i.on(move |this, _, cx| {
                if this.caret_cell_direction() == Some(d) { return; }
                this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Direction { cells: vec![(r, c)], direction: d }, cx);
            })
        } else {
            i.disabled(true)
        }).into()
    };
    table_menu.push(
        crate::shell::chrome::Submenu::tr(
            "docx.cell.direction",
            "text_rotation_none",
            vec![
                direction_item("docx.cell.direction.lrtb", ibis_doc_model::CellDirection::LrTb),
                direction_item("docx.cell.direction.tbrl", ibis_doc_model::CellDirection::TbRl),
                direction_item("docx.cell.direction.tblrv", ibis_doc_model::CellDirection::TbLrV),
                direction_item("docx.cell.direction.btlr", ibis_doc_model::CellDirection::BtLr),
            ],
        )
        .into(),
    );
    let margins = _editor.caret_cell_margins().unwrap_or(CELL_MARGIN_NORMAL);
    let margin_item = |id: &'static str, m: [ibis_doc_model::Emu; 4]| -> MenuEntry {
        let i = MenuItem::tr(id).checked(in_table && margins == m);
        (if in_table {
            i.on(move |this, _, cx| {
                if this.caret_cell_margins() == Some(m) { return; }
                this.edit_table(|r, c, _| ibis_doc_model::TableEdit::Margins { cells: vec![(r, c)], margins: m }, cx);
            })
        } else {
            i.disabled(true)
        }).into()
    };
    table_menu.push(
        crate::shell::chrome::Submenu::tr(
            "docx.cell.margins",
            "crop_square",
            vec![
                margin_item("docx.cell.margins.normal", CELL_MARGIN_NORMAL),
                margin_item("docx.cell.margins.none", CELL_MARGIN_NONE),
                margin_item("docx.cell.margins.narrow", CELL_MARGIN_NARROW),
                margin_item("docx.cell.margins.wide", CELL_MARGIN_WIDE),
            ],
        )
        .into(),
    );
    let cell_wrap = !_editor.caret_cell_no_wrap().unwrap_or(false);
    table_menu.push(
        MenuItem::tr("docx.cell.wrap-text")
            .icon("wrap_text")
            .checked(in_table && cell_wrap)
            .disabled(!in_table)
            .on(|this, _, cx| this.toggle_cell_wrap_text(cx))
            .into(),
    );
    let table_align = _editor.caret_table_align().unwrap_or(ibis_doc_model::FlowAlign::Left);
    let align_item = |id: &'static str, icon: &'static str, a: ibis_doc_model::FlowAlign| -> MenuEntry {
        let i = MenuItem::tr(id).icon(icon).checked(in_table && table_align == a);
        (if in_table {
            i.on(move |this, _, cx| {
                if this.caret_table_align() == Some(a) { return; }
                this.edit_table(|_, _, _| ibis_doc_model::TableEdit::Align(a), cx);
            })
        } else {
            i.disabled(true)
        }).into()
    };
    table_menu.push(
        crate::shell::chrome::Submenu::tr(
            "docx.table.align",
            "format_align_left",
            vec![
                align_item("docx.table.align.left", "format_align_left", ibis_doc_model::FlowAlign::Left),
                align_item("docx.table.align.center", "format_align_center", ibis_doc_model::FlowAlign::Center),
                align_item("docx.table.align.right", "format_align_right", ibis_doc_model::FlowAlign::Right),
            ],
        )
        .into(),
    );
    let table_indent = _editor.caret_table_indent().unwrap_or(0);
    let mut indent_items: Vec<MenuEntry> = TABLE_INDENTS.iter().map(|&(id, label, val)| {
        let i = MenuItem::new(id, crate::shell::i18n::text(label)).checked(in_table && table_indent == val);
        (if in_table {
            i.on(move |this, _, cx| {
                if this.caret_table_indent() == Some(val) { return; }
                this.edit_table(move |_, _, _| ibis_doc_model::TableEdit::Indent(val), cx);
            })
        } else {
            i.disabled(true)
        }).into()
    }).collect();
    let fix_indents: &[(&str, ibis_doc_model::Emu)] = &[
        ("docx.table.indent.none", TABLE_INDENT_NONE),
        ("docx.table.indent.5mm", TABLE_INDENT_5MM),
        ("docx.table.indent.10mm", TABLE_INDENT_10MM),
        ("docx.table.indent.15mm", TABLE_INDENT_15MM),
        ("docx.table.indent.20mm", TABLE_INDENT_20MM),
    ];
    for &(id, val) in fix_indents {
        let i = MenuItem::tr(id).checked(in_table && table_indent == val);
        indent_items.push((if in_table {
            i.on(move |this, _, cx| {
                if this.caret_table_indent() == Some(val) { return; }
                this.edit_table(move |_, _, _| ibis_doc_model::TableEdit::Indent(val), cx);
            })
        } else {
            i.disabled(true)
        }).into());
    }
    table_menu.push(
        crate::shell::chrome::Submenu::tr(
            "docx.table.indent",
            "format_indent_increase",
            indent_items,
        )
        .into(),
    );
    let table_cell_spacing = _editor.caret_table_cell_spacing().flatten();
    let spacing_items: Vec<MenuEntry> = TABLE_CELL_SPACINGS.iter().map(|&(id, label, val)| {
        let i = MenuItem::new(id, crate::shell::i18n::text(label)).checked(in_table && table_cell_spacing == val);
        (if in_table {
            i.on(move |this, _, cx| {
                if this.caret_table_cell_spacing().flatten() == val { return; }
                this.edit_table(move |_, _, _| ibis_doc_model::TableEdit::CellSpacing(val), cx);
            })
        } else {
            i.disabled(true)
        }).into()
    }).collect();
    table_menu.push(
        crate::shell::chrome::Submenu::tr(
            "docx.table.cell-spacing",
            "grid_view",
            spacing_items,
        )
        .into(),
    );
    let table_float = _editor.caret_table_floating().flatten();
    let is_floating = table_float.is_some();
    let float_align = table_float.and_then(|f| f.x_align);
    let wrap_menu_items: Vec<MenuEntry> = vec![
        MenuItem::tr("docx.table.wrap.none")
            .checked(in_table && !is_floating)
            .disabled(!in_table)
            .on(|this, _, cx| this.set_table_floating(None, cx))
            .into(),
        MenuItem::tr("docx.table.wrap.around-left")
            .checked(in_table && is_floating && (float_align == Some(ibis_doc_model::TableAlign::Start) || float_align.is_none()))
            .disabled(!in_table)
            .on(|this, _, cx| this.set_table_float_horz_align(Some(ibis_doc_model::TableAlign::Start), cx))
            .into(),
        MenuItem::tr("docx.table.wrap.around-center")
            .checked(in_table && is_floating && float_align == Some(ibis_doc_model::TableAlign::Center))
            .disabled(!in_table)
            .on(|this, _, cx| this.set_table_float_horz_align(Some(ibis_doc_model::TableAlign::Center), cx))
            .into(),
        MenuItem::tr("docx.table.wrap.around-right")
            .checked(in_table && is_floating && float_align == Some(ibis_doc_model::TableAlign::End))
            .disabled(!in_table)
            .on(|this, _, cx| this.set_table_float_horz_align(Some(ibis_doc_model::TableAlign::End), cx))
            .into(),
    ];
    table_menu.push(
        crate::shell::chrome::Submenu::tr(
            "docx.table.wrap",
            "wrap_text",
            wrap_menu_items,
        )
        .into(),
    );
    let current_distance = table_float.map(|f| f.distance).unwrap_or([180 * 635, 180 * 635, 0, 0]);
    let dist_items: Vec<MenuEntry> = TABLE_FLOAT_DISTANCES.iter().map(|&(id, label, dist)| {
        let i = MenuItem::new(id, crate::shell::i18n::text(label)).checked(in_table && is_floating && current_distance == dist);
        (if in_table {
            i.on(move |this, _, cx| {
                this.set_table_float_distance(dist, cx);
            })
        } else {
            i.disabled(true)
        }).into()
    }).collect();
    table_menu.push(
        crate::shell::chrome::Submenu::tr(
            "docx.table.float-distance",
            "space_bar",
            dist_items,
        )
        .into(),
    );
    // 表スタイルのオプション (Word's テーブル デザイン): the parts its style
    // formats, where the caret's table has a style that formats parts.
    {
        let look = _editor.caret_table().and_then(|t| t.style.as_ref()).map(|s| s.look);
        type Flag = fn(&mut ibis_doc_model::TableLook) -> &mut bool;
        let options: [(&'static str, &'static str, Flag); 6] = [
            ("docx.table.look.first-row", "docx.table-design.header-row", |l| &mut l.first_row),
            ("docx.table.look.last-row", "docx.table-design.total-row", |l| &mut l.last_row),
            ("docx.table.look.first-column", "docx.table-design.first-column", |l| &mut l.first_column),
            ("docx.table.look.last-column", "docx.table-design.last-column", |l| &mut l.last_column),
            ("docx.table.look.row-bands", "docx.toolbar.banded-rows", |l| &mut l.no_hband),
            ("docx.table.look.column-bands", "docx.toolbar.banded-columns", |l| &mut l.no_vband),
        ];
        let items: Vec<MenuEntry> = options
            .into_iter()
            .map(|(id, label, flag)| {
                // The bands' options are on when their "no" flag is off.
                let negated = id.ends_with("-bands");
                let checked = look.is_some_and(|mut l| *flag(&mut l) != negated);
                let item = MenuItem::new(id, crate::shell::i18n::text(label)).checked(checked);
                match look {
                    Some(l) => item
                        .on(move |this, _, cx| {
                            let mut next = l;
                            let f = flag(&mut next);
                            *f = !*f;
                            this.edit_table(move |_, _, _| ibis_doc_model::TableEdit::Look(next), cx);
                        })
                        .into(),
                    None => item.disabled(true).into(),
                }
            })
            .collect();
        // 表のスタイル (Word's 表のスタイルギャラリー)
        {
            let cur_style_id = _editor.caret_docx_table_style().flatten();
            let mut style_items: Vec<MenuEntry> = vec![
                MenuItem::tr("docx.table.style.none")
                    .disabled(!in_table)
                    .checked(in_table && cur_style_id.is_none())
                    .on(|this, _, cx| this.set_docx_table_style(None, cx))
                    .into(),
            ];
            for (id, name) in _editor.document_table_styles() {
                let id_clone = id.clone();
                let is_checked = in_table && cur_style_id.as_deref() == Some(&id);
                let menu_id: &'static str = match id.as_str() {
                    "TableGrid" => "docx.table.style.table-grid",
                    _ => "docx.table.style.item",
                };
                style_items.push(
                    MenuItem::new(menu_id, crate::shell::i18n::text(&name))
                        .disabled(!in_table)
                        .checked(is_checked)
                        .on(move |this, _, cx| this.set_docx_table_style(Some(id_clone.clone()), cx))
                        .into(),
                );
            }
            table_menu.push(MenuEntry::Separator);
            table_menu.push(crate::shell::chrome::Submenu::new("docx.table.style", "table_chart", crate::shell::i18n::text("docx.toolbar.table-style"), style_items).into());
        }
        table_menu.push(MenuEntry::Separator);
        table_menu.push(crate::shell::chrome::Submenu::new("docx.table.look", "table", crate::shell::i18n::text("docx.toolbar.table-style-options"), items).into());
    }
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.table", "table", table_menu).into());
    // 表示形式 > 箇条書き / 番号付きリスト
    let caret_para = _editor.ordered_selection().and_then(|(s, _)| _editor.flow().and_then(|d| ibis_doc_model::flow_command::paragraph(d, s.para).cloned()));
    let editing = caret_para.is_some();
    let in_bullet = caret_para.as_ref().and_then(|p| p.list).and_then(|l| _editor.flow().and_then(|d| d.styles.numbering.level(l.num_id, 0))).map(|l| l.format == "bullet");
    let in_list = _editor.caret_list_numbered();
    let renumber = |id: &'static str, label: String, how: ibis_doc_model::flow_command::Renumber| -> MenuEntry {
        let i = MenuItem::new(id, crate::shell::i18n::text(&label));
        (if in_list { i.on(move |this, _, cx| this.renumber_list(how, cx)) } else { i.disabled(true) }).into()
    };
    use ibis_doc_model::flow_command::Renumber;
    const STARTS: [&str; 10] = ["docx.list.start-1", "docx.list.start-2", "docx.list.start-3", "docx.list.start-4", "docx.list.start-5", "docx.list.start-6", "docx.list.start-7", "docx.list.start-8", "docx.list.start-9", "docx.list.start-10"];
    let starts: Vec<MenuEntry> = STARTS.iter().enumerate().map(|(i, id)| renumber(id, format!("{}", i + 1), Renumber::StartAt(i as u32 + 1))).collect();
    let list_menu = vec![
        MenuItem::tr("docx.list.toggle").icon("format_list_numbered").checked(in_bullet == Some(false)).disabled(!editing).on(|this, _, cx| this.toggle_list(false, cx)).into(),
        MenuEntry::Separator,
        renumber("docx.list.restart", "docx.toolbar.restart-1".into(), Renumber::StartAt(1)),
        renumber("docx.list.continue", "docx.toolbar.continue-previous-list".into(), Renumber::Continue),
        crate::shell::chrome::Submenu::tr("docx.list.start", "format_list_numbered", starts).into(),
        MenuEntry::Separator,
        // 番号の書式 (Word's 番号ライブラリ) and 番号の後: list_formats.rs.
        crate::shell::chrome::Submenu::tr("docx.list.format", "format_list_numbered", {
            let current = _editor.caret_list_format();
            super::list_formats::LIBRARY
                .iter()
                .enumerate()
                .map(|(i, &(id, label, format, text))| {
                    let on = current.as_ref().is_some_and(|(f, t, _)| f == format && t == text);
                    MenuItem::new(id, label).checked(on).on(move |this, _, cx| this.apply_list_format(i, cx)).into()
                })
                .collect()
        })
        .into(),
        crate::shell::chrome::Submenu::tr("docx.list.suffix", "format_list_numbered", {
            let current = _editor.caret_list_format().map(|(_, _, s)| s);
            [("docx.list.suffix-tab", ibis_doc_model::ListSuffix::Tab), ("docx.list.suffix-space", ibis_doc_model::ListSuffix::Space), ("docx.list.suffix-none", ibis_doc_model::ListSuffix::Nothing)]
                .into_iter()
                .map(|(id, suffix)| {
                    let item = MenuItem::tr(id).checked(current == Some(suffix));
                    (if current.is_some() { item.on(move |this, _, cx| this.set_list_suffix(suffix, cx)) } else { item.disabled(true) }).into()
                })
                .collect()
        })
        .into(),
        MenuEntry::Separator,
        crate::shell::chrome::Submenu::tr("docx.list.styles", "format_list_numbered", {
            let current = _editor.caret_list_style();
            let styles = _editor.list_styles();
            if styles.is_empty() {
                vec![MenuItem::tr("docx.list.styles.empty").disabled(true).into()]
            } else {
                styles
                    .into_iter()
                    .map(|(style_id, name, num_id)| {
                        let is_on = current.as_ref() == Some(&name) || current.as_ref() == Some(&style_id);
                        let menu_id: &'static str = Box::leak(format!("docx.list.style:{style_id}").into_boxed_str());
                        MenuItem::new(menu_id, name)
                            .checked(is_on)
                            .on(move |this, _, cx| this.apply_list_style(num_id, cx))
                            .into()
                    })
                    .collect()
            }
        })
        .into(),
        MenuItem::tr("docx.list.define-style")
            .icon("format_list_numbered")
            .disabled(!editing)
            .on(|this, window, cx| this.open_define_list_style_panel(window, cx))
            .into(),
    ];
    let bullet_menu = vec![
        MenuItem::tr("docx.bullet.toggle").icon("format_list_bulleted").checked(in_bullet == Some(true)).disabled(!editing).on(|this, _, cx| this.toggle_list(true, cx)).into(),
        MenuItem::tr("docx.bullet.picture").icon("image").disabled(!editing).on(|this, window, cx| this.doc_pick_picture_bullet(window, cx)).into(),
        MenuEntry::Separator,
        crate::shell::chrome::Submenu::tr("docx.bullet.format", "format_list_bulleted", {
            let current = _editor.caret_bullet_format();
            super::list_formats::BULLET_LIBRARY
                .iter()
                .enumerate()
                .map(|(i, &(id, label, _, _))| {
                    let on = current.as_ref().is_some_and(|(t, _, _)| t == label);
                    MenuItem::tr(id).checked(on).on(move |this, _, cx| this.apply_bullet_format(i, cx)).into()
                })
                .collect()
        })
        .into(),
        crate::shell::chrome::Submenu::tr("docx.bullet.suffix", "format_list_bulleted", {
            let current = _editor.caret_bullet_format().map(|(_, _, s)| s);
            [("docx.bullet.suffix-tab", ibis_doc_model::ListSuffix::Tab), ("docx.bullet.suffix-space", ibis_doc_model::ListSuffix::Space), ("docx.bullet.suffix-none", ibis_doc_model::ListSuffix::Nothing)]
                .into_iter()
                .map(|(id, suffix)| {
                    let item = MenuItem::tr(id).checked(current == Some(suffix));
                    (if current.is_some() { item.on(move |this, _, cx| this.set_bullet_suffix(suffix, cx)) } else { item.disabled(true) }).into()
                })
                .collect()
        })
        .into(),
    ];
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.bullet", "format_list_bulleted", bullet_menu).into());
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.list", "format_list_numbered", list_menu).into());
    // 表示形式 > 改ページと改行 (Word's 段落 > 改ページと改行).
    let (keep_next, keep_lines, break_before, contextual_spacing, widow_control, suppress_line_numbers, suppress_auto_hyphens, overflow_punct, auto_space_letters, auto_space_digits, kinsoku, word_wrap, bidi, snap_to_grid) =
        caret_para.as_ref().map_or((false, false, false, false, true, false, false, true, true, true, true, true, false, true), |p| {
            (p.keep_with_next, p.keep_lines, p.page_break_before, p.contextual_spacing, p.widow_control, p.suppress_line_numbers, p.suppress_auto_hyphens, p.overflow_punct, p.auto_space_letters, p.auto_space_digits, p.kinsoku, p.word_wrap, p.bidi, p.snap_to_grid)
        });
    let flag = |id: &'static str, label: &'static str, on: bool, change: fn(bool) -> ibis_doc_model::ParaChange| -> MenuEntry {
        MenuItem::new(id, crate::shell::i18n::text(label)).checked(on).disabled(!editing).on(move |this, _, cx| this.apply_para_change(change(!on), cx)).into()
    };
    let pagination = vec![
        flag("docx.para.widow-control", "docx.paragraph-panel.widow-orphan-control", widow_control, ibis_doc_model::ParaChange::WidowControl),
        flag("docx.para.keep-next", "docx.toolbar.keep-next", keep_next, ibis_doc_model::ParaChange::KeepNext),
        flag("docx.para.keep-lines", "docx.toolbar.keep-lines-together", keep_lines, ibis_doc_model::ParaChange::KeepLines),
        flag("docx.para.page-break-before", "docx.toolbar.page-break-before", break_before, ibis_doc_model::ParaChange::PageBreakBefore),
        flag("docx.para.suppress-line-numbers", "docx.paragraph-panel.suppress-line-numbers", suppress_line_numbers, ibis_doc_model::ParaChange::SuppressLineNumbers),
        flag("docx.para.suppress-auto-hyphens", "docx.paragraph-panel.don-t-hyphenate", suppress_auto_hyphens, ibis_doc_model::ParaChange::SuppressAutoHyphens),
        flag("docx.para.contextual-spacing", "fix.toolbar.don-t-add-space-between", contextual_spacing, ibis_doc_model::ParaChange::ContextualSpacing),
        flag("docx.para.overflow-punct", "docx.toolbar.hanging-punctuation", overflow_punct, ibis_doc_model::ParaChange::OverflowPunct),
        flag("docx.para.auto-space-de", "docx.toolbar.automatically-adjust-space-between", auto_space_letters, ibis_doc_model::ParaChange::AutoSpaceDE),
        flag("docx.para.auto-space-dn", "docx.toolbar.automatically-adjust-space-between-2", auto_space_digits, ibis_doc_model::ParaChange::AutoSpaceDN),
        flag("docx.para.kinsoku", "docx.toolbar.use-asian-rules-controlling-first", kinsoku, ibis_doc_model::ParaChange::Kinsoku),
        flag("docx.para.word-wrap", "docx.toolbar.allow-latin-text-wrap-middle", word_wrap, ibis_doc_model::ParaChange::WordWrap),
        flag("docx.para.bidi", "docx.toolbar.right-left-text-direction", bidi, ibis_doc_model::ParaChange::Bidi),
        flag("docx.para.snap-to-grid", "docx.toolbar.snap-grid", snap_to_grid, ibis_doc_model::ParaChange::SnapToGrid),
    ];
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.para.pagination", "insert_page_break", pagination).into());
    let strict_kinsoku = _editor.strict_kinsoku();
    let kinsoku_mode: Vec<MenuEntry> = vec![
        MenuItem::new("docx.kinsoku.mode.normal", crate::shell::i18n::text("docx.kinsoku.mode.normal")).checked(!strict_kinsoku).on(|this, _, cx| this.set_kinsoku_preset(false, cx)).into(),
        MenuItem::new("docx.kinsoku.mode.strict", crate::shell::i18n::text("docx.kinsoku.mode.strict")).checked(strict_kinsoku).on(|this, _, cx| this.set_kinsoku_preset(true, cx)).into(),
    ];
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.kinsoku.mode", "format_align_left", kinsoku_mode).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("docx.kinsoku.menu").icon("tune").on(|this, window, cx| this.open_kinsoku_panel(window, cx)).into());
    menus.connect("docx.kinsoku.menu", |this, window, cx| this.open_kinsoku_panel(window, cx));
    menus.connect("docx.kinsoku.mode.normal", |this, _, cx| this.set_kinsoku_preset(false, cx));
    menus.connect("docx.kinsoku.mode.strict", |this, _, cx| this.set_kinsoku_preset(true, cx));
    menus.connect("docx.para.suppress-auto-hyphens", |this, _, cx| this.toggle_suppress_auto_hyphens(cx));
    menus.connect("docx.para.overflow-punct", |this, _, cx| this.toggle_overflow_punct(cx));
    menus.connect("docx.para.auto-space-de", |this, _, cx| this.toggle_auto_space_de(cx));
    menus.connect("docx.para.auto-space-dn", |this, _, cx| this.toggle_auto_space_dn(cx));
    menus.connect("docx.para.kinsoku", |this, _, cx| this.toggle_kinsoku(cx));
    menus.connect("docx.para.word-wrap", |this, _, cx| this.toggle_word_wrap(cx));
    menus.connect("docx.para.bidi", |this, _, cx| this.toggle_bidi(cx));
    menus.connect("docx.para.snap-to-grid", |this, _, cx| this.toggle_snap_to_grid(cx));
    menus.items_mut(MenuId::Format).push(MenuItem::tr("docx.heading-numbers").icon("format_list_numbered").on(|this, _, cx| this.number_headings(cx)).into());
    {
        let editing = _editor.ordered_selection().is_some();
        let outline: Vec<MenuEntry> = vec![
            MenuEntry::Separator,
            MenuItem::tr("docx.outline.promote").disabled(!editing).on(|this, _, cx| { this.change_heading_level(-1, cx); }).into(),
            MenuItem::tr("docx.outline.demote").disabled(!editing).on(|this, _, cx| { this.change_heading_level(1, cx); }).into(),
            MenuItem::tr("docx.outline.move-up").disabled(!editing).on(|this, _, cx| { this.move_heading(true, cx); }).into(),
            MenuItem::tr("docx.outline.move-down").disabled(!editing).on(|this, _, cx| { this.move_heading(false, cx); }).into(),
            MenuItem::tr("docx.outline.to-heading").disabled(!editing).on(|this, _, cx| { this.body_to_heading(cx); }).into(),
            MenuItem::tr("docx.outline.to-body").disabled(!editing).on(|this, _, cx| { this.heading_to_body(cx); }).into(),
            MenuItem::tr("docx.outline.collapse").disabled(!editing).on(|this, _, cx| this.toggle_heading_at_caret(cx)).into(),
            MenuItem::tr("docx.outline.collapse-all").disabled(!editing).on(|this, _, cx| this.set_all_headings_collapsed(true, cx)).into(),
            MenuItem::tr("docx.outline.expand-all").disabled(!editing).on(|this, _, cx| this.set_all_headings_collapsed(false, cx)).into(),
        ];
        menus.items_mut(MenuId::Format).extend(outline);
    }
    // 表示形式 > ページ設定: the orientation, the paper and the margins
    // (Word's レイアウト > 印刷の向き・サイズ・余白).
    let is_vertical = _editor.flow().is_some_and(|d| d.page.direction == ibis_doc_model::CellDirection::TbRl);
    let ln_restart = {
        let section = _editor.caret_section();
        let setup = _editor.flow().and_then(|d| ibis_doc_model::flow_command::section_setup(d, section));
        setup.and_then(|s| s.page.line_numbers).map(|n| n.restart)
    };
    let ln_menu: Vec<MenuEntry> = vec![
        MenuItem::tr("docx.line-numbers.none")
            .checked(ln_restart.is_none())
            .on(|this, _, cx| this.set_line_numbers(None, cx))
            .into(),
        MenuItem::tr("docx.line-numbers.continuous")
            .checked(ln_restart == Some(ibis_doc_model::LineNumberRestart::Continuous))
            .on(|this, _, cx| this.set_line_numbers(Some(ibis_doc_model::LineNumberRestart::Continuous), cx))
            .into(),
        MenuItem::tr("docx.line-numbers.restart-page")
            .checked(ln_restart == Some(ibis_doc_model::LineNumberRestart::NewPage))
            .on(|this, _, cx| this.set_line_numbers(Some(ibis_doc_model::LineNumberRestart::NewPage), cx))
            .into(),
        MenuItem::tr("docx.line-numbers.restart-section")
            .checked(ln_restart == Some(ibis_doc_model::LineNumberRestart::NewSection))
            .on(|this, _, cx| this.set_line_numbers(Some(ibis_doc_model::LineNumberRestart::NewSection), cx))
            .into(),
    ];
    let caret_cols = _editor.caret_columns();
    let cols_menu: Vec<MenuEntry> = vec![
        MenuItem::tr("docx.columns.1")
            .checked(caret_cols.count == 1)
            .on(|this, _, cx| this.set_columns(1, cx))
            .into(),
        MenuItem::tr("docx.columns.2")
            .checked(caret_cols.count == 2)
            .on(|this, _, cx| this.set_columns(2, cx))
            .into(),
        MenuItem::tr("docx.columns.3")
            .checked(caret_cols.count == 3)
            .on(|this, _, cx| this.set_columns(3, cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("docx.columns.separator")
            .checked(caret_cols.separator)
            .disabled(caret_cols.count <= 1)
            .on(|this, _, cx| this.toggle_column_separator(cx))
            .into(),
    ];
    let page_menu: Vec<MenuEntry> = vec![
        MenuItem::tr("docx.page.direction.horizontal").checked(!is_vertical).on(|this, _, cx| this.set_text_direction(false, cx)).into(),
        MenuItem::tr("docx.page.direction.vertical").checked(is_vertical).on(|this, _, cx| this.set_text_direction(true, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("docx.page.portrait").on(|this, _, cx| this.set_orientation(false, cx)).into(),
        MenuItem::tr("docx.page.landscape").on(|this, _, cx| this.set_orientation(true, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("docx.page.a4").on(|this, _, cx| this.set_paper(11_906, 16_838, cx)).into(),
        MenuItem::tr("docx.page.b5").on(|this, _, cx| this.set_paper(10_318, 14_570, cx)).into(),
        MenuItem::tr("docx.page.a3").on(|this, _, cx| this.set_paper(16_838, 23_811, cx)).into(),
        MenuItem::tr("docx.page.letter").on(|this, _, cx| this.set_paper(12_240, 15_840, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("docx.page.margins-normal").on(|this, _, cx| this.set_margins([1985, 1701, 1701, 1701], cx)).into(),
        MenuItem::tr("docx.page.margins-narrow").on(|this, _, cx| this.set_margins([720, 720, 720, 720], cx)).into(),
        MenuItem::tr("docx.page.margins-wide").on(|this, _, cx| this.set_margins([1440, 2880, 1440, 2880], cx)).into(),
        MenuEntry::Separator,
        crate::shell::chrome::Submenu::tr("docx.line-numbers", "format_list_numbered", ln_menu).into(),
        crate::shell::chrome::Submenu::tr("docx.columns", "view_column", cols_menu).into(),
        {
            let current_preset = _editor.page_border_preset();
            let borders_from_text = _editor.caret_page().is_some_and(|p| p.borders_from_text);
            let has_borders = current_preset != crate::docx::editing::PageBorderPreset::None;
            let page_borders_menu: Vec<MenuEntry> = vec![
                MenuItem::tr("docx.page.borders.none")
                    .checked(!has_borders)
                    .on(|this, _, cx| this.set_page_border_preset(crate::docx::editing::PageBorderPreset::None, cx))
                    .into(),
                MenuItem::tr("docx.page.borders.box-thin")
                    .checked(current_preset == crate::docx::editing::PageBorderPreset::BoxThin)
                    .on(|this, _, cx| this.set_page_border_preset(crate::docx::editing::PageBorderPreset::BoxThin, cx))
                    .into(),
                MenuItem::tr("docx.page.borders.box-medium")
                    .checked(current_preset == crate::docx::editing::PageBorderPreset::BoxMedium)
                    .on(|this, _, cx| this.set_page_border_preset(crate::docx::editing::PageBorderPreset::BoxMedium, cx))
                    .into(),
                MenuItem::tr("docx.page.borders.box-thick")
                    .checked(current_preset == crate::docx::editing::PageBorderPreset::BoxThick)
                    .on(|this, _, cx| this.set_page_border_preset(crate::docx::editing::PageBorderPreset::BoxThick, cx))
                    .into(),
                MenuItem::tr("docx.page.borders.box-double")
                    .checked(current_preset == crate::docx::editing::PageBorderPreset::BoxDouble)
                    .on(|this, _, cx| this.set_page_border_preset(crate::docx::editing::PageBorderPreset::BoxDouble, cx))
                    .into(),
                MenuItem::tr("docx.page.borders.top-bottom")
                    .checked(current_preset == crate::docx::editing::PageBorderPreset::TopBottom)
                    .on(|this, _, cx| this.set_page_border_preset(crate::docx::editing::PageBorderPreset::TopBottom, cx))
                    .into(),
                MenuEntry::Separator,
                MenuItem::tr("docx.page.borders.from-page")
                    .checked(!borders_from_text)
                    .on(|this, _, cx| this.set_page_border_offset_from(false, cx))
                    .into(),
                MenuItem::tr("docx.page.borders.from-text")
                    .checked(borders_from_text)
                    .on(|this, _, cx| this.set_page_border_offset_from(true, cx))
                    .into(),
            ];
            crate::shell::chrome::Submenu::tr("docx.page.borders", "border_outer", page_borders_menu).into()
        },
        {
            let current_theme = _editor.current_docx_theme_preset();
            let mut themes_menu: Vec<MenuEntry> = crate::docx::editing::DOCX_DOCUMENT_THEME_PRESETS
                .iter()
                .map(|preset| {
                    MenuItem::tr(preset.label_key)
                        .checked(current_theme == Some(preset.id))
                        .on(move |this, _, cx| this.apply_docx_theme(preset.id, cx))
                        .into()
                })
                .collect();
            themes_menu.push(MenuEntry::Separator);
            themes_menu.push(
                MenuItem::tr("docx.design.themes.customize")
                    .icon("tune")
                    .on(|this, window, cx| this.open_docx_theme_panel(window, cx))
                    .into(),
            );
            crate::shell::chrome::Submenu::tr("docx.design.themes", "style", themes_menu).into()
        },
        {
            let current_preset = _editor.document_style_set_preset();
            let style_set_menu: Vec<MenuEntry> = crate::docx::editing::DOCX_STYLE_SET_PRESETS
                .iter()
                .map(|preset| {
                    MenuItem::tr(preset.label_key)
                        .checked(current_preset == Some(preset.preset) || (current_preset.is_none() && preset.preset == ibis_doc_model::StyleSetPreset::Default))
                        .on(move |this, _, cx| this.set_document_style_set(preset.preset, cx))
                        .into()
                })
                .collect();
            crate::shell::chrome::Submenu::tr("docx.design.style-set", "auto_awesome_motion", style_set_menu).into()
        },
        {
            let current_color = _editor.current_docx_theme_colors_preset();
            let mut tc_menu: Vec<MenuEntry> = crate::docx::editing::DOCX_THEME_COLORS_PRESETS
                .iter()
                .map(|preset| {
                    MenuItem::tr(preset.label_key)
                        .checked(current_color == Some(preset.id))
                        .on(move |this, _, cx| this.apply_docx_theme_colors(preset.id, cx))
                        .into()
                })
                .collect();
            tc_menu.push(MenuEntry::Separator);
            tc_menu.push(
                MenuItem::tr("docx.design.theme-colors.customize")
                    .icon("palette")
                    .on(|this, window, cx| this.open_docx_theme_panel(window, cx))
                    .into(),
            );
            crate::shell::chrome::Submenu::tr("docx.design.theme-colors", "palette", tc_menu).into()
        },
        {
            let current_preset = _editor.document_paragraph_spacing_preset();
            let ps_menu: Vec<MenuEntry> = vec![
                MenuItem::tr("docx.design.paragraph-spacing.default")
                    .checked(current_preset == Some(ibis_doc_model::ParagraphSpacingPreset::Default) || current_preset.is_none())
                    .on(|this, _, cx| this.set_document_paragraph_spacing(ibis_doc_model::ParagraphSpacingPreset::Default, cx))
                    .into(),
                MenuItem::tr("docx.design.paragraph-spacing.none")
                    .checked(current_preset == Some(ibis_doc_model::ParagraphSpacingPreset::NoSpace))
                    .on(|this, _, cx| this.set_document_paragraph_spacing(ibis_doc_model::ParagraphSpacingPreset::NoSpace, cx))
                    .into(),
                MenuItem::tr("docx.design.paragraph-spacing.compact")
                    .checked(current_preset == Some(ibis_doc_model::ParagraphSpacingPreset::Compact))
                    .on(|this, _, cx| this.set_document_paragraph_spacing(ibis_doc_model::ParagraphSpacingPreset::Compact, cx))
                    .into(),
                MenuItem::tr("docx.design.paragraph-spacing.tight")
                    .checked(current_preset == Some(ibis_doc_model::ParagraphSpacingPreset::Tight))
                    .on(|this, _, cx| this.set_document_paragraph_spacing(ibis_doc_model::ParagraphSpacingPreset::Tight, cx))
                    .into(),
                MenuItem::tr("docx.design.paragraph-spacing.open")
                    .checked(current_preset == Some(ibis_doc_model::ParagraphSpacingPreset::Open))
                    .on(|this, _, cx| this.set_document_paragraph_spacing(ibis_doc_model::ParagraphSpacingPreset::Open, cx))
                    .into(),
                MenuItem::tr("docx.design.paragraph-spacing.relaxed")
                    .checked(current_preset == Some(ibis_doc_model::ParagraphSpacingPreset::Relaxed))
                    .on(|this, _, cx| this.set_document_paragraph_spacing(ibis_doc_model::ParagraphSpacingPreset::Relaxed, cx))
                    .into(),
                MenuItem::tr("docx.design.paragraph-spacing.double")
                    .checked(current_preset == Some(ibis_doc_model::ParagraphSpacingPreset::Double))
                    .on(|this, _, cx| this.set_document_paragraph_spacing(ibis_doc_model::ParagraphSpacingPreset::Double, cx))
                    .into(),
            ];
            crate::shell::chrome::Submenu::tr("docx.design.paragraph-spacing", "format_line_spacing", ps_menu).into()
        },
        {
            let current_preset = _editor.current_docx_theme_fonts_preset();
            let tf_menu: Vec<MenuEntry> = crate::docx::editing::DOCX_THEME_FONTS_PRESETS
                .iter()
                .map(|preset| {
                    MenuItem::tr(preset.label_key)
                        .checked(current_preset == Some(preset.id))
                        .on(move |this, _, cx| this.apply_docx_theme_fonts(preset.id, cx))
                        .into()
                })
                .collect();
            crate::shell::chrome::Submenu::tr("docx.design.theme-fonts", "font_download", tf_menu).into()
        },
        {
            let ef_menu: Vec<MenuEntry> = crate::docx::editing::DOCX_THEME_EFFECTS_PRESETS
                .iter()
                .map(|preset| {
                    MenuItem::tr(preset.label_key)
                        .checked(_editor.is_docx_theme_effects_active(preset.id))
                        .on(move |this, _, cx| this.apply_docx_theme_effects(preset.id, cx))
                        .into()
                })
                .collect();
            crate::shell::chrome::Submenu::tr("docx.design.effects", "auto_fix_high", ef_menu).into()
        },
        MenuItem::tr("docx.design.set-as-default")
            .icon("check_circle")
            .on(|this, _, cx| this.set_docx_design_as_default(cx))
            .into(),
        {
            let current_mp = _editor.caret_multiple_pages();
            let mp_menu: Vec<MenuEntry> = vec![
                MenuItem::tr("docx.page.multiple-pages.normal")
                    .checked(current_mp == ibis_doc_model::MultiplePages::Normal)
                    .on(|this, _, cx| this.set_multiple_pages(ibis_doc_model::MultiplePages::Normal, None, cx))
                    .into(),
                MenuItem::tr("docx.page.multiple-pages.mirror-margins")
                    .checked(current_mp == ibis_doc_model::MultiplePages::MirrorMargins)
                    .on(|this, _, cx| this.set_multiple_pages(ibis_doc_model::MultiplePages::MirrorMargins, None, cx))
                    .into(),
                MenuItem::tr("docx.page.multiple-pages.book-fold")
                    .checked(current_mp == ibis_doc_model::MultiplePages::BookFold)
                    .on(|this, _, cx| {
                        let sheets = this.caret_book_fold_sheets();
                        this.set_multiple_pages(ibis_doc_model::MultiplePages::BookFold, sheets, cx);
                    })
                    .into(),
                MenuItem::tr("docx.page.multiple-pages.reverse-book-fold")
                    .checked(current_mp == ibis_doc_model::MultiplePages::ReverseBookFold)
                    .on(|this, _, cx| {
                        let sheets = this.caret_book_fold_sheets();
                        this.set_multiple_pages(ibis_doc_model::MultiplePages::ReverseBookFold, sheets, cx);
                    })
                    .into(),
                MenuItem::tr("docx.page.multiple-pages.two-on-one")
                    .checked(current_mp == ibis_doc_model::MultiplePages::TwoOnOne)
                    .on(|this, _, cx| this.set_multiple_pages(ibis_doc_model::MultiplePages::TwoOnOne, None, cx))
                    .into(),
            ];
            crate::shell::chrome::Submenu::tr("docx.page.multiple-pages", "auto_stories", mp_menu).into()
        },
        MenuItem::tr("docx.page.doc-grid").on(|this, window, cx| this.open_page_setup_panel(window, cx)).into(),
        MenuItem::tr("docx.page.setup").on(|this, window, cx| this.open_page_setup_panel(window, cx)).into(),
        MenuItem::tr("docx.watermark").on(|this, window, cx| this.open_watermark_panel(window, cx)).into(),
        {
            let current_page_color = _editor.flow().and_then(|d| d.page_color);
            let page_color_menu: Vec<MenuEntry> = {
                let list: Vec<(SharedString, Rgb, &'static str)> = PAGE_COLORS.iter().map(|&(item_id, name, c)| (SharedString::from(crate::shell::i18n::text(name)), Rgb(c), item_id)).collect();
                let none = MenuItem::tr("docx.page.color.none").icon("format_color_reset").checked(current_page_color.is_none()).on(|this, _, cx| this.set_page_color(None, cx));
                crate::shell::color_palette::simple_palette(Some(none), None, &list, current_page_color, std::rc::Rc::new(|this, c, _, cx| this.set_page_color(c, cx)))
            };
            crate::shell::chrome::Submenu::tr("docx.page.color", "format_color_fill", page_color_menu).into()
        },
    ];
    menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.page", "crop_landscape", page_menu).into());
    // 表示形式 > セクション (Word's ヘッダーとフッター タブ and ページ番号の書式).
    {
        use ibis_doc_model::PartKind;
        let section = _editor.caret_section();
        let setup = _editor.flow().and_then(|d| ibis_doc_model::flow_command::section_setup(d, section));
        let even_odd = _editor.flow().is_some_and(|d| d.even_and_odd);
        let (title, restart, format, first_page) = setup.as_ref().map_or((false, false, None, 1), |s| (s.title_page, s.restart_numbering, s.page_number_format.clone(), s.page.first_page));
        let show_document_text = _editor.is_show_document_text();
        let (header_linked, has_previous) = _editor.part_linked(PartKind::Header);
        let (footer_linked, _) = _editor.part_linked(PartKind::Footer);
        let link = |id: &'static str, label: &'static str, kind: PartKind, on: bool| -> MenuEntry {
            MenuItem::new(id, crate::shell::i18n::text(label)).checked(on).disabled(!has_previous).on(move |this, _, cx| this.toggle_linked(kind, cx)).into()
        };
        let edit = |id: &'static str, label: &'static str, kind: PartKind| -> MenuEntry { MenuItem::new(id, crate::shell::i18n::text(label)).on(move |this, _, cx| this.edit_part(kind, cx)).into() };
        let fmt = |id: &'static str, label: &'static str, f: Option<&'static str>| -> MenuEntry {
            MenuItem::new(id, crate::shell::i18n::text(label)).checked(format.as_deref() == f || (f.is_none() && format.as_deref() == Some("decimal"))).on(move |this, _, cx| this.set_section(move |s| s.page_number_format = f.map(str::to_string), cx)).into()
        };
        let start_preset = |id: &'static str, label: &'static str, n: Option<u32>| -> MenuEntry {
            let is_on = match n {
                None => !restart,
                Some(k) => restart && first_page == k,
            };
            MenuItem::new(id, crate::shell::i18n::text(label))
                .checked(is_on)
                .on(move |this, _, cx| this.set_page_number_start(n, cx))
                .into()
        };
        let mut start_items = vec![
            start_preset("docx.section.page-start-continue", "docx.toolbar.continue-previous-section", None),
            MenuEntry::Separator,
            start_preset("docx.section.page-start-1", "docx.toolbar.start-1", Some(1)),
            start_preset("docx.section.page-start-2", "docx.toolbar.start-2", Some(2)),
            start_preset("docx.section.page-start-3", "docx.toolbar.start-3", Some(3)),
            start_preset("docx.section.page-start-5", "docx.toolbar.start-5", Some(5)),
            start_preset("docx.section.page-start-10", "docx.toolbar.start-10", Some(10)),
        ];
        if restart && ![1, 2, 3, 5, 10].contains(&first_page) {
            let dynamic_id = "docx.section.page-start-current";
            let dynamic_label = crate::shell::i18n::format("_docx.start-number-first-page", &[("first_page", &first_page)]);
            start_items.push(
                MenuItem::new(dynamic_id, dynamic_label)
                    .checked(true)
                    .on(move |this, _, cx| this.set_page_number_start(Some(first_page), cx))
                    .into(),
            );
        }
        start_items.push(MenuEntry::Separator);
        start_items.push(
            MenuItem::new("docx.section.page-start-custom", crate::shell::i18n::text("docx.toolbar.page-setup"))
                .on(|this, window, cx| this.open_page_setup_panel(window, cx))
                .into(),
        );
        // ページ番号に章番号を含める: the heading level (1 to 3) whose number goes before the page number, and the separator.
        let chapter = setup.as_ref().and_then(|s| s.page_chapter.clone());
        let mut chapter_items: Vec<MenuEntry> = vec![MenuItem::tr("docx.section.chapter-none").checked(chapter.is_none()).on(|this, _, cx| this.set_section(|s| s.page_chapter = None, cx)).into(), MenuEntry::Separator];
        for (id, n) in [("docx.section.chapter-level-1", 1u32), ("docx.section.chapter-level-2", 2), ("docx.section.chapter-level-3", 3)] {
            let (on, sep) = (chapter.as_ref().is_some_and(|(l, _)| *l == n), chapter.as_ref().map_or_else(|| "hyphen".to_string(), |(_, s)| s.clone()));
            chapter_items.push(MenuItem::tr(id).checked(on).on(move |this, _, cx| { let sep = sep.clone(); this.set_section(move |s| s.page_chapter = Some((n, sep.clone())), cx) }).into());
        }
        chapter_items.push(MenuEntry::Separator);
        for (id, name) in [("docx.section.chapter-sep-hyphen", "hyphen"), ("docx.section.chapter-sep-period", "period"), ("docx.section.chapter-sep-colon", "colon"), ("docx.section.chapter-sep-emdash", "emDash"), ("docx.section.chapter-sep-endash", "enDash")] {
            let on = chapter.as_ref().is_some_and(|(_, s)| s == name);
            let level = chapter.as_ref().map(|(l, _)| *l);
            chapter_items.push(MenuItem::tr(id).checked(on).disabled(level.is_none()).on(move |this, _, cx| { if let Some(l) = level { this.set_section(move |s| s.page_chapter = Some((l, name.to_string())), cx) } }).into());
        }
        let menu = vec![
            edit("docx.section.edit-header", "docx.toolbar.edit-header-section", PartKind::Header),
            edit("docx.section.edit-footer", "docx.toolbar.edit-footer-section", PartKind::Footer),
            edit("docx.section.edit-first-header", "docx.toolbar.edit-first-page-header", PartKind::HeaderFirst),
            edit("docx.section.edit-first-footer", "docx.toolbar.edit-first-page-footer", PartKind::FooterFirst),
            edit("docx.section.edit-even-header", "docx.toolbar.edit-even-page-header", PartKind::HeaderEven),
            edit("docx.section.edit-even-footer", "docx.toolbar.edit-even-page-footer", PartKind::FooterEven),
            MenuEntry::Separator,
            link("docx.section.link-header", "docx.toolbar.link-header-previous", PartKind::Header, header_linked),
            link("docx.section.link-footer", "docx.toolbar.link-footer-previous", PartKind::Footer, footer_linked),
            MenuItem::tr("docx.section.title-page").checked(title).on(move |this, _, cx| this.set_section(move |s| s.title_page = !title, cx)).into(),
            MenuItem::tr("docx.section.even-odd").checked(even_odd).on(|this, _, cx| this.toggle_even_and_odd(cx)).into(),
            MenuItem::tr("docx.section.show-document-text").checked(show_document_text).on(|this, _, cx| this.toggle_show_document_text(cx)).into(),
            MenuItem::tr("docx.quick-parts").icon("auto_awesome").on(|this, window, cx| this.open_quick_parts_panel(window, cx)).into(),
            MenuEntry::Separator,
            crate::shell::chrome::Submenu::tr("docx.section.start-page", "tag", start_items).into(),
            MenuItem::tr("docx.section.restart").checked(restart).on(move |this, _, cx| {
                this.set_section(
                    move |s| {
                        s.restart_numbering = !restart;
                        s.page.first_page = 1;
                    },
                    cx,
                )
            })
            .into(),
            fmt("docx.section.format-decimal", "docx.toolbar.page-number-1-2-3", None),
            fmt("docx.section.format-lower-roman", "docx.toolbar.page-number-i-ii-iii", Some("lowerRoman")),
            fmt("docx.section.format-upper-roman", "docx.toolbar.page-number-i-ii-iii-2", Some("upperRoman")),
            fmt("docx.section.format-lower-letter", "docx.toolbar.page-number-b-c", Some("lowerLetter")),
            fmt("docx.section.format-upper-letter", "docx.toolbar.page-number-b-c-2", Some("upperLetter")),
            crate::shell::chrome::Submenu::tr("docx.section.chapter", "format_list_numbered", chapter_items).into(),
        ];
        menus.items_mut(MenuId::Format).push(crate::shell::chrome::Submenu::tr("docx.section", "view_agenda", menu).into());
    }

    // 挿入 > アイコン, SmartArt, クイック パーツ, ワードアート, 署名欄の connect
    menus.connect("docx.insert.icon", |this, _, cx| { this.doc_insert_icon("auto_awesome", cx); });
    menus.connect("insert.smartart", |this, _, cx| { this.doc_insert_smartart("process", cx); });
    menus.connect("docx.insert.smartart", |this, _, cx| { this.doc_insert_smartart("process", cx); });
    menus.connect("pptx.insert.smartart.list", |this, _, cx| { this.doc_insert_smartart("list", cx); });
    menus.connect("pptx.insert.smartart.vlist", |this, _, cx| { this.doc_insert_smartart("vlist", cx); });
    menus.connect("pptx.insert.smartart.process", |this, _, cx| { this.doc_insert_smartart("process", cx); });
    menus.connect("pptx.insert.smartart.chevron", |this, _, cx| { this.doc_insert_smartart("chevron", cx); });
    menus.connect("pptx.insert.smartart.cycle", |this, _, cx| { this.doc_insert_smartart("cycle", cx); });
    menus.connect("pptx.insert.smartart.hierarchy", |this, _, cx| { this.doc_insert_smartart("hierarchy", cx); });
    menus.connect("pptx.insert.smartart.relationship", |this, _, cx| { this.doc_insert_smartart("relationship", cx); });
    menus.connect("pptx.insert.smartart.matrix", |this, _, cx| { this.doc_insert_smartart("matrix", cx); });
    menus.connect("pptx.insert.smartart.pyramid", |this, _, cx| { this.doc_insert_smartart("pyramid", cx); });
    menus.connect("pptx.insert.smartart.picture", |this, _, cx| { this.doc_insert_smartart("picture", cx); });
    menus.connect("docx.insert.wordart", |this, _, cx| { this.doc_insert_wordart(0, None, cx); });
    for (idx, &(_, insert_id, _)) in crate::docx::insert_objects::DOCX_WORDART_STYLES.iter().enumerate() {
        menus.connect(insert_id, move |this, _, cx| { this.doc_insert_wordart(idx, None, cx); });
    }
    menus.connect("docx.signature-line", |this, _, cx| { this.doc_insert_signature_line("", "", cx); });
    menus.connect("docx.quick-parts", |this, window, cx| { this.open_quick_parts_panel(window, cx); });
    menus.connect("docx.quick-parts.author", |this, _, cx| { this.doc_insert_quick_part("AUTHOR", "Author", cx); });
    menus.connect("docx.quick-parts.title", |this, _, cx| { this.doc_insert_quick_part("TITLE", "Document Title", cx); });
    menus.connect("docx.quick-parts.subject", |this, _, cx| { this.doc_insert_quick_part("SUBJECT", "Subject", cx); });
    menus.connect("docx.quick-parts.createdate", |this, _, cx| { this.doc_insert_quick_part("CREATEDATE", "2026/10/16", cx); });
    menus.connect("docx.quick-parts.filename", |this, _, cx| { this.doc_insert_quick_part("FILENAME", "Document", cx); });
    menus.connect("docx.quick-parts.numpages", |this, _, cx| { this.doc_insert_quick_part("NUMPAGES", "1", cx); });

    // 描画 / 手描き注釈・インク（DOCX-IMAGE-019, DOCX-INK-001〜007, DOCX-INK-010）
    {
        use crate::docx::ink::{InkEffect, PenKind};
        let ink = _editor.doc_state().and_then(|s| s.docx_ink.clone());
        let pen = ink.as_ref().map(|i| i.pen);
        let mouse = ink.as_ref().is_none_or(|i| i.mouse);
        let to_shape = ink.as_ref().is_some_and(|i| i.ink_to_shape_mode);
        let editor_mode = ink.as_ref().is_some_and(|i| i.ink_editor_mode);
        let ink_items: Vec<MenuEntry> = vec![
            MenuItem::new("docx.ink.panel", crate::shell::i18n::text("docx.ink.panel"))
                .icon("draw")
                .on(|this, _, cx| this.open_docx_ink_panel(cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::new("docx.ink.pen", crate::shell::i18n::text("docx.ink.pen"))
                .checked(pen == Some(PenKind::Pen))
                .on(|this, _, cx| {
                    this.open_docx_ink_panel(cx);
                    this.set_docx_ink_pen(PenKind::Pen, cx);
                })
                .into(),
            MenuItem::new("docx.ink.pencil", crate::shell::i18n::text("docx.ink.pencil"))
                .checked(pen == Some(PenKind::Pencil))
                .on(|this, _, cx| {
                    this.open_docx_ink_panel(cx);
                    this.set_docx_ink_pen(PenKind::Pencil, cx);
                })
                .into(),
            MenuItem::new("docx.ink.highlighter", crate::shell::i18n::text("docx.ink.highlighter"))
                .checked(pen == Some(PenKind::Highlighter))
                .on(|this, _, cx| {
                    this.open_docx_ink_panel(cx);
                    this.set_docx_ink_pen(PenKind::Highlighter, cx);
                })
                .into(),
            MenuItem::new("docx.ink.eraser", crate::shell::i18n::text("docx.ink.eraser"))
                .checked(pen == Some(PenKind::Eraser))
                .on(|this, _, cx| {
                    this.open_docx_ink_panel(cx);
                    this.set_docx_ink_pen(PenKind::Eraser, cx);
                })
                .into(),
            MenuItem::new("docx.ink.lasso", crate::shell::i18n::text("docx.ink.lasso"))
                .checked(pen == Some(PenKind::Select))
                .on(|this, _, cx| {
                    this.open_docx_ink_panel(cx);
                    this.set_docx_ink_pen(PenKind::Select, cx);
                })
                .into(),
            MenuEntry::Separator,
            MenuItem::new("docx.ink.draw-with-mouse", crate::shell::i18n::text("docx.ink.draw-with-mouse"))
                .checked(mouse)
                .on(|this, _, cx| this.toggle_docx_ink_mouse(cx))
                .into(),
            MenuItem::new("docx.ink.ink-to-shape", crate::shell::i18n::text("docx.ink.ink-to-shape"))
                .checked(to_shape)
                .on(|this, _, cx| this.toggle_docx_ink_to_shape_mode(cx))
                .into(),
            MenuItem::new("docx.ink.convert-now", crate::shell::i18n::text("docx.ink.convert-now"))
                .on(|this, _, cx| {
                    this.docx_ink_to_shape(cx);
                })
                .into(),
            MenuItem::new("docx.ink.ink-editor", crate::shell::i18n::text("docx.ink.ink-editor"))
                .checked(editor_mode)
                .on(|this, _, cx| this.toggle_docx_ink_editor_mode(cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::new("docx.ink.effect.rainbow", crate::shell::i18n::text("docx.ink.effect.rainbow"))
                .on(|this, _, cx| this.set_docx_ink_effect(Some(InkEffect::Rainbow), cx))
                .into(),
            MenuItem::new("docx.ink.effect.galaxy", crate::shell::i18n::text("docx.ink.effect.galaxy"))
                .on(|this, _, cx| this.set_docx_ink_effect(Some(InkEffect::Galaxy), cx))
                .into(),
            MenuItem::new("docx.ink.effect.gold", crate::shell::i18n::text("docx.ink.effect.gold"))
                .on(|this, _, cx| this.set_docx_ink_effect(Some(InkEffect::Gold), cx))
                .into(),
            MenuItem::new("docx.ink.erase-all", crate::shell::i18n::text("docx.ink.erase-all"))
                .on(|this, _, cx| {
                    this.erase_all_docx_ink(cx);
                })
                .into(),
        ];
        menus.items_mut(MenuId::Insert).push(
            crate::shell::chrome::Submenu::tr("docx.ink", "draw", ink_items).into(),
        );
    }

    // スクリーンリーダーの操作・読み上げ・リボンとクイックアクセスのカスタマイズ (DOCX-ACCESS-007, DOCX-ACCESS-009, DOCX-CLOUD-020)
    menus.items_mut(MenuId::Tools).push(
        MenuItem::tr("docx.read-aloud")
            .icon("record_voice_over")
            .shortcut("⌥⌘Space")
            .checked(_editor.is_docx_read_aloud_open())
            .on(|this, _, cx| this.open_docx_read_aloud(cx))
            .into(),
    );
    menus.items_mut(MenuId::Tools).push(
        MenuItem::tr("docx.screen-reader")
            .icon("accessibility_new")
            .checked(_editor.is_docx_screen_reader_open())
            .on(|this, _, cx| this.open_docx_screen_reader_panel(cx))
            .into(),
    );
    menus.items_mut(MenuId::Tools).push(
        MenuItem::tr("docx.customize-toolbar")
            .icon("tune")
            .checked(_editor.is_docx_customize_toolbar_open())
            .on(|this, _, cx| this.open_docx_customize_toolbar_panel(cx))
            .into(),
    );
    menus.items_mut(MenuId::View).push(
        MenuItem::tr("docx.read-aloud")
            .icon("record_voice_over")
            .checked(_editor.is_docx_read_aloud_open())
            .on(|this, _, cx| this.open_docx_read_aloud(cx))
            .into(),
    );
    menus.items_mut(MenuId::View).push(
        MenuItem::tr("docx.customize-toolbar")
            .icon("tune")
            .checked(_editor.is_docx_customize_toolbar_open())
            .on(|this, _, cx| this.open_docx_customize_toolbar_panel(cx))
            .into(),
    );
    menus.connect("docx.read-aloud", |this, _, cx| this.open_docx_read_aloud(cx));
    menus.connect("docx.screen-reader", |this, _, cx| this.open_docx_screen_reader_panel(cx));
    menus.connect("docx.customize-toolbar", |this, _, cx| this.open_docx_customize_toolbar_panel(cx));
}

/// The underline kinds of Word (the Format menu's 下線の種類 and the toolbar's underline button's ▼), with the
/// caret's underline (`now`: whether underlined, and how) checked, and 下線なし at the end.
pub(crate) fn underline_type_items(now: Option<(bool, ibis_doc_model::UnderlineStyle)>, editing: bool) -> Vec<MenuEntry> {
    use ibis_doc_model::UnderlineStyle;
    let underlines = [
        ("docx.underline.single", "docx.paragraph-decorations.single", UnderlineStyle::Single), ("docx.underline.words", "docx.toolbar.words-only", UnderlineStyle::Words),
        ("docx.underline.double", "_docx.double-underline", UnderlineStyle::Double), ("docx.underline.thick", "_docx.thick-underline", UnderlineStyle::Thick),
        ("docx.underline.dotted", "_shell.dotted", UnderlineStyle::Dotted), ("docx.underline.dottedHeavy", "docx.toolbar.thick-dotted", UnderlineStyle::DottedHeavy),
        ("docx.underline.dash", "_shell.dashed", UnderlineStyle::Dash), ("docx.underline.dashedHeavy", "docx.toolbar.thick-dashed", UnderlineStyle::DashedHeavy),
        ("docx.underline.dashLong", "docx.toolbar.long-dashed", UnderlineStyle::DashLong), ("docx.underline.dashLongHeavy", "docx.toolbar.thick-long-dashed", UnderlineStyle::DashLongHeavy),
        ("docx.underline.dotDash", "docx.picture.border.dash.dashdot", UnderlineStyle::DotDash), ("docx.underline.dashDotHeavy", "docx.toolbar.thick-dash-dot", UnderlineStyle::DashDotHeavy),
        ("docx.underline.dotDotDash", "_docx.dash-dot-dot", UnderlineStyle::DotDotDash), ("docx.underline.dashDotDotHeavy", "docx.toolbar.thick-dash-dot-dot", UnderlineStyle::DashDotDotHeavy),
        ("docx.underline.wave", "_shell.wave", UnderlineStyle::Wave), ("docx.underline.wavyHeavy", "docx.toolbar.thick-wave", UnderlineStyle::WavyHeavy),
        ("docx.underline.wavyDouble", "docx.paragraph-decorations.double-wave", UnderlineStyle::WavyDouble),
    ];
    let mut types: Vec<MenuEntry> = underlines.iter().map(|&(id, label, value)| MenuItem::new(id, crate::shell::i18n::text(label))
        .checked(now.is_some_and(|(on, kind)| on && kind == value)).disabled(!editing)
        .on(move |this, _, cx| this.set_underline_style(Some(value), cx)).into()).collect();
    types.push(MenuItem::tr("docx.underline.none").checked(now.is_some_and(|(on, _)| !on)).disabled(!editing)
        .on(|this, _, cx| this.set_underline_style(None, cx)).into());
    types
}
