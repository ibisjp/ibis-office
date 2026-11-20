// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Excel's built-in cell styles (ホーム > セルのスタイル: 標準, 良い, 悪い,
//! 見出し, アクセント, …) read from ECMA-376's presetCellStyles.xml, with
//! the workbook's theme colours and fonts: each style as the changes of
//! format it makes to a cell (only the parts the style includes: its
//! `apply…="0"` parts are left as they are).
//!
//! presetCellStyles.xml (`crates/ibis-formats/data/`) is the file of ECMA-376
//! 5th edition Part 1, OfficeOpenXML-SpreadsheetMLStyles.zip, unmodified
//! (see data/ECMA-NOTICE.txt). Nothing of it is copied into this source: it
//! is parsed when the styles are asked for. Each style there is a small
//! style sheet; its second `cellStyleXfs/xf` is the style.

use ibis_doc_model::sheet_command::{Edge, StyleChange};
use ibis_doc_model::{Border, BorderLine, CellUnderline};
use roxmltree::Node;

use crate::theme::Theme;
use crate::xlsx::SheetColors;
use crate::xml::{attr, attr_bool, attr_i64, child, children};

const PRESET_XML: &str = include_str!("../data/presetCellStyles.xml");

/// A built-in cell style.
#[derive(Clone, Debug, PartialEq)]
pub struct PresetCellStyle {
    /// Its element's name in presetCellStyles.xml (`good`, `heading1`, …).
    pub key: &'static str,
    /// The name Excel shows in Japanese.
    pub name: String,
    /// The group Excel's gallery puts it in.
    pub group: &'static str,
    pub changes: Vec<StyleChange>,
}

/// The Japanese names and groups, by the element's name: as Excel's
/// gallery lists them (the outline levels are left out).
const NAMES: &[(&str, &str, &str)] = &[
    ("normal", "標準", "良い、悪い、どちらでもない"),
    ("bad", "悪い", "良い、悪い、どちらでもない"),
    ("good", "良い", "良い、悪い、どちらでもない"),
    ("neutral", "どちらでもない", "良い、悪い、どちらでもない"),
    ("calculation", "計算", "データとモデル"),
    ("checkCell", "チェック セル", "データとモデル"),
    ("explanatoryText", "説明文", "データとモデル"),
    ("input", "入力", "データとモデル"),
    ("linkedCell", "リンク セル", "データとモデル"),
    ("note", "メモ", "データとモデル"),
    ("output", "出力", "データとモデル"),
    ("warningText", "警告文", "データとモデル"),
    ("title", "タイトル", "タイトルと見出し"),
    ("heading1", "見出し 1", "タイトルと見出し"),
    ("heading2", "見出し 2", "タイトルと見出し"),
    ("heading3", "見出し 3", "タイトルと見出し"),
    ("heading4", "見出し 4", "タイトルと見出し"),
    ("total", "集計", "タイトルと見出し"),
    ("accent1pct20", "20% - アクセント 1", "テーマのセル スタイル"),
    ("accent2pct20", "20% - アクセント 2", "テーマのセル スタイル"),
    ("accent3pct20", "20% - アクセント 3", "テーマのセル スタイル"),
    ("accent4pct20", "20% - アクセント 4", "テーマのセル スタイル"),
    ("accent5pct20", "20% - アクセント 5", "テーマのセル スタイル"),
    ("accent6pct20", "20% - アクセント 6", "テーマのセル スタイル"),
    ("accent1pct40", "40% - アクセント 1", "テーマのセル スタイル"),
    ("accent2pct40", "40% - アクセント 2", "テーマのセル スタイル"),
    ("accent3pct40", "40% - アクセント 3", "テーマのセル スタイル"),
    ("accent4pct40", "40% - アクセント 4", "テーマのセル スタイル"),
    ("accent5pct40", "40% - アクセント 5", "テーマのセル スタイル"),
    ("accent6pct40", "40% - アクセント 6", "テーマのセル スタイル"),
    ("accent1pct60", "60% - アクセント 1", "テーマのセル スタイル"),
    ("accent2pct60", "60% - アクセント 2", "テーマのセル スタイル"),
    ("accent3pct60", "60% - アクセント 3", "テーマのセル スタイル"),
    ("accent4pct60", "60% - アクセント 4", "テーマのセル スタイル"),
    ("accent5pct60", "60% - アクセント 5", "テーマのセル スタイル"),
    ("accent6pct60", "60% - アクセント 6", "テーマのセル スタイル"),
    ("accent1", "アクセント 1", "テーマのセル スタイル"),
    ("accent2", "アクセント 2", "テーマのセル スタイル"),
    ("accent3", "アクセント 3", "テーマのセル スタイル"),
    ("accent4", "アクセント 4", "テーマのセル スタイル"),
    ("accent5", "アクセント 5", "テーマのセル スタイル"),
    ("accent6", "アクセント 6", "テーマのセル スタイル"),
    ("comma", "桁区切り", "数値の書式"),
    ("comma0", "桁区切り [0]", "数値の書式"),
    ("currency", "通貨", "数値の書式"),
    ("currency0", "通貨 [0]", "数値の書式"),
    ("percent", "パーセント", "数値の書式"),
];

/// The built-in cell styles, in the order Excel's gallery lists them, with
/// the colours and fonts of the theme `theme_xml` (a theme part's XML; the
/// Office theme when there is none).
pub fn preset_cell_styles(theme_xml: Option<&str>) -> Vec<PresetCellStyle> {
    let default_theme = crate::new_file::theme();
    let theme_doc = theme_xml.and_then(|t| roxmltree::Document::parse(t).ok()).or_else(|| roxmltree::Document::parse(&default_theme).ok());
    let theme = theme_doc.as_ref().map(|d| Theme::read(d.root_element())).unwrap_or_default();
    let colors = SheetColors::new(&theme, None);
    let Ok(doc) = roxmltree::Document::parse(PRESET_XML.trim_start_matches('\u{feff}')) else { return Vec::new() };
    let root = doc.root_element();
    NAMES
        .iter()
        .filter_map(|&(key, name, group)| {
            let element = root.children().find(|n| n.is_element() && n.tag_name().name() == key)?;
            let sheet = child(element, "styleSheet")?;
            let mut changes = style_changes(sheet, &colors, key == "normal");
            // 標準 puts the number format back to General too.
            if key == "normal" && !changes.iter().any(|c| matches!(c, StyleChange::NumberFormat(_))) {
                changes.insert(0, StyleChange::NumberFormat("General".into()));
            }
            Some(PresetCellStyle { key, name: name.into(), group, changes })
        })
        .collect()
}

/// The built-in cell styles with the given theme colours and fonts.
pub fn preset_cell_styles_with_theme(colors: &[(String, ibis_doc_model::Rgb)], fonts: Option<&ibis_doc_model::ThemeFonts>) -> Vec<PresetCellStyle> {
    let theme = Theme::from_colors_and_fonts(colors, fonts);
    let sheet_colors = SheetColors::new(&theme, None);
    let Ok(doc) = roxmltree::Document::parse(PRESET_XML.trim_start_matches('\u{feff}')) else { return Vec::new() };
    let root = doc.root_element();
    NAMES
        .iter()
        .filter_map(|&(key, name, group)| {
            let element = root.children().find(|n| n.is_element() && n.tag_name().name() == key)?;
            let sheet = child(element, "styleSheet")?;
            let mut changes = style_changes(sheet, &sheet_colors, key == "normal");
            if key == "normal" && !changes.iter().any(|c| matches!(c, StyleChange::NumberFormat(_))) {
                changes.insert(0, StyleChange::NumberFormat("General".into()));
            }
            Some(PresetCellStyle { key, name: name.into(), group, changes })
        })
        .collect()
}

/// The built-in cell styles with the theme of a workbook read (`source`).
pub fn preset_cell_styles_of(source: Option<&crate::Source>) -> Vec<PresetCellStyle> {
    let theme = source.and_then(|s| {
        let package = s.package();
        ["xl/theme/theme1.xml"].iter().find_map(|part| package.read(part).ok()).and_then(|bytes| String::from_utf8(bytes).ok())
    });
    preset_cell_styles(theme.as_deref())
}

/// The colours of the theme in SpreadsheetML's order (lt1, dk1, lt2, dk2, accent1–6).
pub fn workbook_theme_colors_from_slice(colors: &[(String, ibis_doc_model::Rgb)]) -> [ibis_doc_model::Rgb; 10] {
    use ibis_doc_model::Rgb;
    let office = [0xFFFFFF, 0x000000, 0xE7E6E6, 0x44546A, 0x4472C4, 0xED7D31, 0xA5A5A5, 0xFFC000, 0x5B9BD5, 0x70AD47];
    let names = ["lt1", "dk1", "lt2", "dk2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6"];
    std::array::from_fn(|i| colors.iter().find(|(n, _)| n == names[i]).map(|(_, c)| *c).unwrap_or(Rgb(office[i])))
}

/// The colours of the workbook's theme in SpreadsheetML's order (lt1, dk1,
/// lt2, dk2, accent1–6): the colour palette's ten columns. Office's own
/// where the workbook has no theme.
pub fn workbook_theme_colors(source: Option<&crate::Source>) -> [ibis_doc_model::Rgb; 10] {
    use ibis_doc_model::Rgb;
    let text = source.and_then(|s| s.package().read("xl/theme/theme1.xml").ok()).and_then(|b| String::from_utf8(b).ok());
    let doc = text.as_ref().and_then(|t| roxmltree::Document::parse(t).ok());
    let theme = doc.as_ref().map(|d| Theme::read(d.root_element())).unwrap_or_default();
    let office = [0xFFFFFF, 0x000000, 0xE7E6E6, 0x44546A, 0x4472C4, 0xED7D31, 0xA5A5A5, 0xFFC000, 0x5B9BD5, 0x70AD47];
    let names = ["lt1", "dk1", "lt2", "dk2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6"];
    std::array::from_fn(|i| theme.color(names[i]).unwrap_or(Rgb(office[i])))
}

/// A colour lighter (tint > 0) or darker (< 0), as Excel's tint does; the
/// tint in millionths.
pub fn tinted(rgb: ibis_doc_model::Rgb, tint: i32) -> ibis_doc_model::Rgb {
    if tint == 0 { rgb } else { crate::xlsx::apply_tint(rgb, tint as f64 / 1_000_000.0) }
}

/// The changes the style of a preset's style sheet makes.
fn style_changes(sheet: Node, colors: &SheetColors, normal: bool) -> Vec<StyleChange> {
    // The style: the sheet's cell style other than Normal (Normal's own
    // sheet lists it with another), or Normal for Normal.
    let styles: Vec<Node> = child(sheet, "cellStyles").map(|c| children(c, "cellStyle").collect()).unwrap_or_default();
    let named = |n: &Node| attr(*n, "name") == Some("Normal");
    let entry = if normal { styles.iter().find(|n| named(n)) } else { styles.iter().find(|n| !named(n)) };
    let index = entry.and_then(|n| attr_i64(*n, "xfId")).unwrap_or(if normal { 0 } else { 1 }).max(0) as usize;
    let Some(xf) = child(sheet, "cellStyleXfs").and_then(|x| children(x, "xf").nth(index)) else { return Vec::new() };
    let applies = |name: &str| attr_bool(xf, name).unwrap_or(true);
    let nth = |list: &str, item: &'static str, id: &str| -> Option<Node> {
        let index = attr_i64(xf, id)?.max(0) as usize;
        child(sheet, list).and_then(|l| children(l, item).nth(index))
    };
    let mut out = Vec::new();
    // A number format of its own (the others' General is not applied: a
    // date given 良い stays a date).
    let number_format = attr_i64(xf, "numFmtId").unwrap_or(0);
    if applies("applyNumberFormat") && (number_format != 0 || attr_bool(xf, "applyNumberFormat") == Some(true)) {
        let id = number_format;
        let code = child(sheet, "numFmts")
            .and_then(|n| children(n, "numFmt").find(|f| attr_i64(*f, "numFmtId") == Some(id)))
            .and_then(|f| attr(f, "formatCode"))
            .map(str::to_string)
            .or_else(|| u32::try_from(id).ok().and_then(ibis_calc::numfmt::builtin).map(str::to_string))
            .unwrap_or_else(|| "General".into());
        out.push(StyleChange::NumberFormat(code));
    }
    if applies("applyFont")
        && let Some(font) = nth("fonts", "font", "fontId")
    {
        let size = child(font, "sz").and_then(|s| attr(s, "val")).and_then(|v| v.parse::<f64>().ok()).unwrap_or(11.0);
        out.push(StyleChange::FontSize((size * 100.0).round() as u32));
        out.push(StyleChange::Bold(child(font, "b").is_some_and(|b| attr_bool(b, "val").unwrap_or(true))));
        out.push(StyleChange::Italic(child(font, "i").is_some_and(|i| attr_bool(i, "val").unwrap_or(true))));
        let underline = match child(font, "u").map(|u| attr(u, "val").unwrap_or("single")) {
            None | Some("none") => CellUnderline::None,
            Some("double") => CellUnderline::Double,
            Some(_) => CellUnderline::Single,
        };
        out.push(StyleChange::UnderlineKind(underline));
        out.push(StyleChange::Color(child(font, "color").and_then(|c| colors.color(c)).unwrap_or(ibis_doc_model::Rgb::BLACK)));
        // The headings' typeface (the theme's major font); the body font
        // is the workbook's own.
        let scheme = child(font, "scheme").and_then(|s| attr(s, "val"));
        out.push(StyleChange::Font(match scheme {
            Some("major") => colors.scheme_font("major").map(str::to_string),
            _ => None,
        }));
    }
    if applies("applyFill") {
        let pattern = nth("fills", "fill", "fillId").and_then(|f| child(f, "patternFill"));
        let fill = pattern.filter(|p| attr(*p, "patternType") == Some("solid")).and_then(|p| child(p, "fgColor")).and_then(|c| colors.color(c));
        out.push(StyleChange::Fill(fill));
    }
    if applies("applyBorder") {
        let border = nth("borders", "border", "borderId");
        for (name, edge) in [("left", Edge::Left), ("right", Edge::Right), ("top", Edge::Top), ("bottom", Edge::Bottom)] {
            let side = border.and_then(|b| child(b, name)).and_then(|e| {
                let line = BorderLine::from_ooxml(attr(e, "style")?)?;
                Some(Border { line, color: child(e, "color").and_then(|c| colors.color(c)).unwrap_or(ibis_doc_model::Rgb::BLACK) })
            });
            out.push(StyleChange::Border(edge, side));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::preset_cell_styles;
    use ibis_doc_model::Rgb;
    use ibis_doc_model::sheet_command::{Edge, StyleChange};

    #[test]
    fn presets_from_the_ecma_file_with_the_office_theme() {
        let styles = preset_cell_styles(None);
        assert_eq!(styles.len(), 47);
        let get = |key: &str| styles.iter().find(|s| s.key == key).unwrap_or_else(|| panic!("{key}")).changes.clone();
        // 良い: dark green text on light green (no borders, no number format).
        let good = get("good");
        assert!(good.contains(&StyleChange::Fill(Some(Rgb(0xC6EFCE)))), "{good:?}");
        assert!(good.contains(&StyleChange::Color(Rgb(0x006100))), "{good:?}");
        assert!(!good.iter().any(|c| matches!(c, StyleChange::Border(..) | StyleChange::NumberFormat(_))));
        // 見出し 1: bold, 15 pt, a thick accent 1 line under it, no fill change.
        let heading = get("heading1");
        assert!(heading.contains(&StyleChange::Bold(true)) && heading.contains(&StyleChange::FontSize(1500)), "{heading:?}");
        assert!(heading.iter().any(|c| matches!(c, StyleChange::Border(Edge::Bottom, Some(b)) if b.line == ibis_doc_model::BorderLine::Thick)), "{heading:?}");
        // 20% - アクセント 1: accent 1 (4472C4) tinted 80% towards white.
        assert!(get("accent1pct20").iter().any(|c| matches!(c, StyleChange::Fill(Some(_)))));
        // 桁区切り: a number format only.
        assert_eq!(get("comma").len(), 1, "{:?}", get("comma"));
    }
}
