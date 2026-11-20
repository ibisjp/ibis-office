// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Table styles of SpreadsheetML tables (`tableStyleInfo`): the built-in ones
//! (TableStyleLight1–21, TableStyleMedium1–28, TableStyleDark1–11) read from
//! ECMA-376's presetTableStyles.xml, and a workbook's own (`styles.xml`
//! `tableStyles`). Applied to the table's cells as direct formatting where a
//! cell has none of its own.
//!
//! presetTableStyles.xml (`crates/ibis-formats/data/`) is the file of
//! ECMA-376 5th edition Part 1, OfficeOpenXML-SpreadsheetMLStyles.zip,
//! unmodified (see data/ECMA-NOTICE.txt). Nothing of it is copied into this
//! source: it is parsed when a workbook's table uses a built-in style.

use std::collections::HashMap;

use ibis_doc_model::{Border, BorderLine, CellKind, CellRange, CellStyle, CellValue, Rgb, Sheet, SheetCell};
use roxmltree::Node;

use crate::xlsx::{SheetColors, parse_range};
use crate::xml::{attr, attr_bool, attr_i64, child, children};

const PRESET_XML: &str = include_str!("../data/presetTableStyles.xml");

/// The formats of one table style element (a `dxf`), as far as cells show
/// them: bold, the font colour, a solid fill, and the borders, `horizontal`
/// and `vertical` being the ones between the element's cells.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ElementFormat {
    bold: Option<bool>,
    color: Option<Rgb>,
    fill: Option<Rgb>,
    left: Option<Border>,
    right: Option<Border>,
    top: Option<Border>,
    bottom: Option<Border>,
    horizontal: Option<Border>,
    vertical: Option<Border>,
}

fn element_format(dxf: Node, colors: &SheetColors) -> ElementFormat {
    let font = child(dxf, "font");
    let fill = child(dxf, "fill").and_then(|f| child(f, "patternFill")).and_then(|p| match attr(p, "patternType") {
        Some("none") => None,
        _ => child(p, "fgColor").or_else(|| child(p, "bgColor")).and_then(|c| colors.color(c)),
    });
    let border = child(dxf, "border");
    let side = |name: &str| -> Option<Border> {
        let e = border.and_then(|b| child(b, name))?;
        let style = attr(e, "style")?;
        if style == "none" {
            return None;
        }
        let line = BorderLine::from_ooxml(style).unwrap_or(BorderLine::Thin);
        Some(Border { line, color: child(e, "color").and_then(|c| colors.color(c)).unwrap_or(Rgb::BLACK) })
    };
    ElementFormat {
        bold: font.and_then(|f| child(f, "b")).map(|b| attr_bool(b, "val").unwrap_or(true)),
        color: font.and_then(|f| child(f, "color")).and_then(|c| colors.color(c)),
        fill,
        left: side("left"),
        right: side("right"),
        top: side("top"),
        bottom: side("bottom"),
        horizontal: side("horizontal"),
        vertical: side("vertical"),
    }
}

/// A style's elements: type (`wholeTable`, `headerRow`, …) → (stripe size, format).
type StyleElements = HashMap<String, (u32, ElementFormat)>;

fn style_elements(style: Node, dxfs: &[Node], first_id: i64, colors: &SheetColors) -> StyleElements {
    children(style, "tableStyleElement")
        .filter_map(|e| {
            let kind = attr(e, "type")?.to_string();
            let size = attr_i64(e, "size").unwrap_or(1).max(1) as u32;
            let format = attr_i64(e, "dxfId")
                .and_then(|id| dxfs.get((id - first_id).max(0) as usize))
                .map(|d| element_format(*d, colors))
                .unwrap_or_default();
            Some((kind, (size, format)))
        })
        .collect()
}

/// A built-in style by name, from presetTableStyles.xml. There each style is
/// an element named after it holding its own `dxfs` and `tableStyles`; the 60
/// table styles number their `dxf`s from 1 (their `dxfId`s run 1..=count;
/// the pivot styles' from 0).
fn builtin_style(name: &str, colors: &SheetColors) -> Option<StyleElements> {
    if !name.starts_with("TableStyle") && !name.starts_with("PivotStyle") {
        return None;
    }
    let doc = roxmltree::Document::parse(PRESET_XML.trim_start_matches('\u{feff}')).ok()?;
    let entry = doc.root_element().children().find(|n| n.is_element() && n.tag_name().name() == name)?;
    let dxfs: Vec<Node> = child(entry, "dxfs").into_iter().flat_map(|d| children(d, "dxf")).collect();
    // Each entry holds the one style; its `name` is not always the entry's
    // (the file has "TableStyleLight14 " and "TableStyleDark9 2").
    let style = child(entry, "tableStyles").and_then(|t| children(t, "tableStyle").next())?;
    let max_id = children(style, "tableStyleElement").filter_map(|e| attr_i64(e, "dxfId")).max().unwrap_or(0);
    let first_id = if max_id as usize == dxfs.len() { 1 } else { 0 };
    Some(style_elements(style, &dxfs, first_id, colors))
}

/// A workbook's own table style (`styles.xml` `tableStyles/tableStyle`),
/// whose `dxfId`s index the workbook's `dxfs` from 0.
fn custom_style(name: &str, styles: Node, colors: &SheetColors) -> Option<StyleElements> {
    let style = child(styles, "tableStyles").and_then(|t| children(t, "tableStyle").find(|s| attr(*s, "name") == Some(name)))?;
    let dxfs: Vec<Node> = child(styles, "dxfs").into_iter().flat_map(|d| children(d, "dxf")).collect();
    Some(style_elements(style, &dxfs, 0, colors))
}

/// Formats the cells of the tables (`xl/tables/table*.xml`) on a sheet with
/// their table styles. A cell's own fill, borders and colour stay; the
/// style fills in what it lacks (call after the tables' own `dxf`s).
pub(crate) fn apply_table_styles(sheet: &mut Sheet, table_xmls: &[&str], styles: Option<Node>, colors: &SheetColors) {
    for xml in table_xmls {
        let Ok(doc) = roxmltree::Document::parse(xml) else { continue };
        let root = doc.root_element();
        let Some(range) = attr(root, "ref").and_then(parse_range) else { continue };
        let Some(info) = child(root, "tableStyleInfo") else { continue };
        let Some(name) = attr(info, "name") else { continue };
        let Some(elements) = styles.and_then(|s| custom_style(name, s, colors)).or_else(|| builtin_style(name, colors)) else { continue };
        let flag = |n: &str| attr_bool(info, n).unwrap_or(false);
        let header_rows = attr_i64(root, "headerRowCount")
            .map(|v| v.max(0) as u32)
            .unwrap_or(if attr(root, "headerRowShown") == Some("0") { 0 } else { 1 });
        let total_rows = attr_i64(root, "totalsRowCount").map_or(0, |v| v.max(0) as u32);
        apply(sheet, range, header_rows, total_rows, &elements, Options {
            first_column: flag("showFirstColumn"),
            last_column: flag("showLastColumn"),
            row_stripes: flag("showRowStripes"),
            column_stripes: flag("showColumnStripes"),
        });
    }
}

#[derive(Clone, Copy)]
struct Options {
    first_column: bool,
    last_column: bool,
    row_stripes: bool,
    column_stripes: bool,
}

/// Each cell's format: the elements that cover it, from the lowest
/// precedence to the highest (ECMA-376 Part 1 §18.8.40 lists the element
/// types; the order is Excel's: the whole table, column then row stripes,
/// the last and first columns, the header and total rows, their corner
/// cells), each overriding what it sets. An element's own outer borders go
/// on the edges of the area it covers, its `horizontal`/`vertical` ones
/// between its cells.
fn apply(sheet: &mut Sheet, range: CellRange, header_rows: u32, total_rows: u32, elements: &StyleElements, o: Options) {
    let (r0, r1, c0, c1) = (range.first_row, range.last_row, range.first_column, range.last_column);
    if r1 < r0 || c1 < c0 {
        return;
    }
    let body = (r0 + header_rows, r1.saturating_sub(total_rows));
    let get = |k: &str| elements.get(k);
    let mut formats: HashMap<(u32, u32), ElementFormat> = HashMap::new();
    let mut lay = |area: (u32, u32, u32, u32), f: &ElementFormat| lay_element(&mut formats, area, f);
    if let Some((_, f)) = get("wholeTable") {
        lay((r0, r1, c0, c1), f);
    }
    // Stripes alternate bands of each element's size over the data rows.
    let in_first_band = |first: &str, second: &str, i: u32| {
        let a = get(first).map_or(1, |e| e.0);
        let b = get(second).map_or(1, |e| e.0);
        i % (a + b) < a
    };
    if o.column_stripes {
        for c in c0..=c1 {
            let first = in_first_band("firstColumnStripe", "secondColumnStripe", c - c0);
            if let Some((_, f)) = get(if first { "firstColumnStripe" } else { "secondColumnStripe" }) {
                lay((body.0, body.1, c, c), f);
            }
        }
    }
    if o.row_stripes && body.1 >= body.0 {
        for r in body.0..=body.1 {
            let first = in_first_band("firstRowStripe", "secondRowStripe", r - body.0);
            if let Some((_, f)) = get(if first { "firstRowStripe" } else { "secondRowStripe" }) {
                lay((r, r, c0, c1), f);
            }
        }
    }
    if o.last_column && let Some((_, f)) = get("lastColumn") {
        lay((r0, r1, c1, c1), f);
    }
    if o.first_column && let Some((_, f)) = get("firstColumn") {
        lay((r0, r1, c0, c0), f);
    }
    if header_rows > 0 && let Some((_, f)) = get("headerRow") {
        lay((r0, r0 + header_rows - 1, c0, c1), f);
    }
    if total_rows > 0 && let Some((_, f)) = get("totalRow") {
        lay((r1 + 1 - total_rows, r1, c0, c1), f);
    }
    if header_rows > 0 {
        if o.first_column && let Some((_, f)) = get("firstHeaderCell") {
            lay((r0, r0, c0, c0), f);
        }
        if o.last_column && let Some((_, f)) = get("lastHeaderCell") {
            lay((r0, r0, c1, c1), f);
        }
    }
    if total_rows > 0 {
        if o.first_column && let Some((_, f)) = get("firstTotalCell") {
            lay((r1, r1, c0, c0), f);
        }
        if o.last_column && let Some((_, f)) = get("lastTotalCell") {
            lay((r1, r1, c1, c1), f);
        }
    }
    write_formats(sheet, formats);
}

/// An element's format laid over `area` (rows `.0..=.1`, columns
/// `.2..=.3`): what it sets overrides, its outer borders on the area's
/// edges and its `horizontal`/`vertical` ones between its cells.
fn lay_element(formats: &mut HashMap<(u32, u32), ElementFormat>, area: (u32, u32, u32, u32), f: &ElementFormat) {
    let (ar0, ar1, ac0, ac1) = area;
    if ar1 < ar0 || ac1 < ac0 {
        return;
    }
    for r in ar0..=ar1 {
        for c in ac0..=ac1 {
            let e = formats.entry((r, c)).or_default();
            if f.bold.is_some() {
                e.bold = f.bold;
            }
            if f.color.is_some() {
                e.color = f.color;
            }
            if f.fill.is_some() {
                e.fill = f.fill;
            }
            let pick = |outer: bool, edge: Option<Border>, inner: Option<Border>| if outer { edge } else { inner };
            if let Some(b) = pick(c == ac0, f.left, f.vertical) {
                e.left = Some(b);
            }
            if let Some(b) = pick(c == ac1, f.right, f.vertical) {
                e.right = Some(b);
            }
            if let Some(b) = pick(r == ar0, f.top, f.horizontal) {
                e.top = Some(b);
            }
            if let Some(b) = pick(r == ar1, f.bottom, f.horizontal) {
                e.bottom = Some(b);
            }
        }
    }
}

/// The cells' formats the elements give, filled into what each cell lacks.
fn write_formats(sheet: &mut Sheet, formats: HashMap<(u32, u32), ElementFormat>) {
    let index: HashMap<(u32, u32), usize> = sheet.cells.iter().enumerate().map(|(i, c)| ((c.row, c.column), i)).collect();
    let mut cache: HashMap<CellStyle, u32> = HashMap::new();
    for (i, s) in sheet.styles.iter().enumerate() {
        cache.entry(s.clone()).or_insert(i as u32);
    }
    let mut added = Vec::new();
    let mut keys: Vec<_> = formats.keys().copied().collect();
    keys.sort();
    for key in keys {
        let f = formats[&key];
        let existing = index.get(&key).copied();
        // A cell the table covers but the sheet does not have starts from the
        // workbook's default format (the first).
        let mut s = existing.map_or_else(|| sheet.styles.first().cloned().unwrap_or_default(), |i| sheet.styles[sheet.cells[i].style as usize].clone());
        let before = s.clone();
        if s.fill.is_none() {
            s.fill = f.fill;
        }
        if let Some(true) = f.bold {
            s.bold = true;
        }
        if s.color == Rgb::BLACK && let Some(c) = f.color {
            s.color = c;
            s.color_theme = None;
        }
        s.left = s.left.or(f.left);
        s.right = s.right.or(f.right);
        s.top = s.top.or(f.top);
        s.bottom = s.bottom.or(f.bottom);
        if s == before {
            continue;
        }
        let id = *cache.entry(s.clone()).or_insert_with(|| {
            sheet.styles.push(s);
            (sheet.styles.len() - 1) as u32
        });
        match existing {
            Some(i) => sheet.cells[i].style = id,
            None => added.push(SheetCell { runs: None, phonetic: None, row: key.0, column: key.1, value: CellValue::Empty, text: String::new(), kind: CellKind::Empty, formula: None, style: id }),
        }
    }
    if !added.is_empty() {
        sheet.cells.extend(added);
        sheet.cells.sort_by_key(|c| (c.row, c.column));
    }
}

/// Formats the cells of the pivot table `index` on `sheet` with its style
/// (`pivotTableStyleInfo`), as tables are: `rows` and `columns` say what
/// each body row and data column is (see [`PivotTable::line_kinds`]). The
/// areas (ECMA-376 Part 1 §18.18.84): the header rows (`headerRow`), the
/// row labels (`firstColumn`), the stripes, each level's group headings
/// (`firstRowSubheading`… — the whole row, or in 表形式 the field's label
/// cells over its group), subtotal rows and columns, 総計 (`totalRow`,
/// `lastColumn`), and the report filters; later ones over earlier ones.
///
/// [`PivotTable::line_kinds`]: ibis_doc_model::PivotTable::line_kinds
pub(crate) fn apply_pivot_style(sheet: &mut Sheet, index: usize, rows: &[ibis_doc_model::pivot::PivotLineKind], columns: &[ibis_doc_model::pivot::PivotLineKind], styles: Option<Node>, colors: &SheetColors) {
    use ibis_doc_model::pivot::PivotLineKind as K;
    let Some(pivot) = sheet.pivots.get(index) else { return };
    let Some(info) = pivot.style.clone() else { return };
    let Some(elements) = styles.and_then(|s| custom_style(&info.name, s, colors)).or_else(|| builtin_style(&info.name, colors)) else { return };
    let loc = pivot.location;
    let (r0, r1, c0, c1) = (loc.first_row, loc.last_row, loc.first_column, loc.last_column);
    if r1 < r0 || c1 < c0 {
        return;
    }
    let body = r0 + pivot.first_data_row;
    let data_col = c0 + pivot.first_data_col;
    let (label_col, _) = pivot.row_label_places();
    let pages = pivot.page_fields.len() as u32;
    let get = |k: &str| elements.get(k).map(|e| e.1);
    let size = |k: &str| elements.get(k).map_or(1, |e| e.0);
    let mut formats: HashMap<(u32, u32), ElementFormat> = HashMap::new();
    let mut lay = |area: (u32, u32, u32, u32), k: &str| {
        if let Some(f) = get(k) {
            lay_element(&mut formats, area, &f);
        }
    };
    lay((r0, r1, c0, c1), "wholeTable");
    let row_of = |i: usize| body + i as u32;
    let col_of = |i: usize| data_col + i as u32;
    if info.col_stripes {
        let (a, b) = (size("firstColumnStripe"), size("secondColumnStripe"));
        for (i, _) in columns.iter().enumerate() {
            let k = if (i as u32) % (a + b) < a { "firstColumnStripe" } else { "secondColumnStripe" };
            lay((body, r1, col_of(i), col_of(i)), k);
        }
    }
    if info.row_stripes {
        let (a, b) = (size("firstRowStripe"), size("secondRowStripe"));
        for (i, kind) in rows.iter().enumerate() {
            if *kind == K::Grand {
                continue;
            }
            let k = if (i as u32) % (a + b) < a { "firstRowStripe" } else { "secondRowStripe" };
            lay((row_of(i), row_of(i), c0, c1), k);
        }
    }
    if info.row_headers && data_col > c0 {
        lay((body, r1, c0, data_col - 1), "firstColumn");
    }
    if info.col_headers && body > r0 {
        lay((r0, body - 1, c0, c1), "headerRow");
    }
    if body > r0 && data_col > c0 {
        lay((r0, r0, c0, c0), "firstHeaderCell");
    }
    const NTH: [&str; 3] = ["first", "second", "third"];
    for (i, kind) in columns.iter().enumerate() {
        if let K::Subtotal { depth } = kind {
            lay((body, r1, col_of(i), col_of(i)), &format!("{}SubtotalColumn", NTH[(*depth as usize).min(2)]));
        }
    }
    // Group headings: the row of a compact or outline group; in 表形式, the
    // field's label cells from the group's first row to its subtotal.
    let mut group_start: Vec<Option<usize>> = vec![None; 3];
    for (i, kind) in rows.iter().enumerate() {
        match *kind {
            K::Item { depth, last: false, .. } => lay((row_of(i), row_of(i), c0, c1), &format!("{}RowSubheading", NTH[(depth as usize).min(2)])),
            K::Item { .. } => {
                for d in 0..3 {
                    group_start[d].get_or_insert(i);
                }
            }
            K::Subtotal { depth } => {
                let d = depth as usize;
                if let (Some(start), Some(&lc)) = (group_start.get(d.min(2)).copied().flatten(), label_col.get(d)) {
                    if i > start {
                        lay((row_of(start), row_of(i - 1), c0 + lc, c0 + lc), &format!("{}RowSubheading", NTH[d.min(2)]));
                    }
                }
                for g in group_start.iter_mut().skip(d) {
                    *g = None;
                }
            }
            K::Grand | K::Blank => {}
        }
    }
    for (i, kind) in rows.iter().enumerate() {
        if let K::Subtotal { depth } = kind {
            lay((row_of(i), row_of(i), c0, c1), &format!("{}SubtotalRow", NTH[(*depth as usize).min(2)]));
        }
    }
    if info.last_column {
        for (i, kind) in columns.iter().enumerate() {
            if *kind == K::Grand {
                lay((r0, r1, col_of(i), col_of(i)), "lastColumn");
            }
        }
    }
    for (i, kind) in rows.iter().enumerate() {
        if *kind == K::Grand {
            lay((row_of(i), row_of(i), c0, c1), "totalRow");
        }
    }
    if pages > 0 {
        let first = r0.saturating_sub(pages + 1);
        lay((first, first + pages - 1, c0, c0), "pageFieldLabels");
        lay((first, first + pages - 1, c0 + 1, c0 + 1), "pageFieldValues");
    }
    write_formats(sheet, formats);
}

/// A copy of `sheet` with a table of a built-in style over `range` formatted
/// as Excel formats it (what each cell lacks filled in), for a table made
/// or grown in the app. `theme` is the book's colour scheme
/// ([`ibis_doc_model::Workbook::theme_colors`]).
pub fn styled_with_table(sheet: &Sheet, table: &ibis_doc_model::SheetTable, theme: &[(String, ibis_doc_model::Rgb)]) -> Sheet {
    let mut out = sheet.clone();
    let Some(info) = &table.style else { return out };
    let fallback;
    let theme = if theme.is_empty() {
        fallback = crate::new_file::theme_colours();
        crate::theme::Theme::from_colors(&fallback)
    } else {
        crate::theme::Theme::from_colors(theme)
    };
    let colors = SheetColors::new(&theme, None);
    let Some(elements) = builtin_style(&info.name, &colors) else { return out };
    apply(&mut out, table.range, table.header_rows, table.totals_rows, &elements, Options {
        first_column: info.first_column,
        last_column: info.last_column,
        row_stripes: info.row_stripes,
        column_stripes: info.column_stripes,
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    const THEME: &str = r#"<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:themeElements><a:clrScheme name="t">
        <a:dk1><a:srgbClr val="000000"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1>
        <a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2>
        <a:accent1><a:srgbClr val="4472C4"/></a:accent1></a:clrScheme></a:themeElements></a:theme>"#;

    fn cell(row: u32, column: u32) -> SheetCell {
        SheetCell { runs: None, phonetic: None, row, column, value: CellValue::Text("x".into()), text: "x".into(), kind: CellKind::Text, formula: None, style: 0 }
    }

    // TableStyleMedium2 as presetTableStyles.xml defines it: the header row
    // filled with accent 1 in bold white, every other data row accent 1 80%
    // lighter, thin accent 1 (40% lighter) lines at the edges and between
    // rows but not between columns.
    #[test]
    fn medium2_from_the_ecma_presets() {
        let theme_doc = roxmltree::Document::parse(THEME).unwrap();
        let theme = Theme::read(theme_doc.root_element());
        let colors = SheetColors::new(&theme, None);
        let mut sheet = Sheet { cells: vec![cell(0, 0), cell(0, 1), cell(1, 0), cell(2, 0)].into(), styles: vec![CellStyle::default()], ..Default::default() };
        let table = r#"<table xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" ref="A1:B3">
            <tableStyleInfo name="TableStyleMedium2" showFirstColumn="0" showLastColumn="0" showRowStripes="1" showColumnStripes="0"/></table>"#;
        apply_table_styles(&mut sheet, &[table], None, &colors);
        let style = |r: u32, c: u32| sheet.styles[sheet.cell(r, c).unwrap().style as usize].clone();
        let header = style(0, 0);
        assert_eq!(header.fill, Some(Rgb(0x4472C4)));
        assert!(header.bold);
        assert_eq!(header.color, Rgb(0xFFFFFF));
        let first_row = style(1, 0);
        assert_eq!(first_row.fill.map(|f| f != Rgb(0xFFFFFF) && f != Rgb(0x4472C4)), Some(true));
        assert_eq!(style(2, 0).fill, None);
        let line = first_row.top.unwrap();
        assert_eq!(line.line, BorderLine::Thin);
        assert_eq!(first_row.left.map(|b| b.line), Some(BorderLine::Thin));
        assert_eq!(first_row.right, None, "no line between the columns");
        // The cell the sheet lacked (B2) is made, for its stripe and lines.
        assert!(sheet.cell(1, 1).is_some());
        assert_eq!(style(1, 1).right.map(|b| b.line), Some(BorderLine::Thin));
    }

    #[test]
    fn every_builtin_table_style_is_found() {
        let theme = Theme::default();
        let colors = SheetColors::new(&theme, None);
        let names = (1..=21).map(|i| format!("TableStyleLight{i}"))
            .chain((1..=28).map(|i| format!("TableStyleMedium{i}")))
            .chain((1..=11).map(|i| format!("TableStyleDark{i}")));
        for name in names {
            let elements = builtin_style(&name, &colors).unwrap_or_else(|| panic!("{name}"));
            assert!(elements.contains_key("wholeTable"), "{name}");
        }
        // The pivot styles are in the same file (their dxfs from 0).
        let pivot = builtin_style("PivotStyleLight16", &colors).expect("PivotStyleLight16");
        assert!(pivot.contains_key("headerRow") && pivot.contains_key("firstRowSubheading"));
    }
}
