// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データの編集 in the workbook editor: a chart's data as its embedded
//! workbook and the references (`c:f`) its series take their names,
//! categories and values from; the ranges grown or shrunk with the data
//! (the table they are in, else the cells around them); the chart's
//! series and caches rewritten from the workbook, and the workbook put in
//! the chart's file.
//!
//! The series are counted in the chart's order: the plot area's chart
//! elements (`pptx_chart::GROUP_ELEMENTS`) and their `c:ser`, as the model's
//! groups and series are.

use ibis_doc_model::{CellRange, Chart, Number, Presentation, ShapeKind, Sheet, SheetCell};
use ibis_ooxml::{NodeId, Package, XmlDocument};

use super::pptx::{next_part_name, relative};
use super::pptx_chart::{GROUP_ELEMENTS, REL_PACKAGE, cell_address, number_text};
use super::pptx_new_chart::NewChartKind;
use crate::Error;
use crate::save::unsupported;

const CT_XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

/// Where one series takes its name, categories and values from in the
/// workbook (`Sheet1!$B$1`, `Sheet1!$A$2:$A$5`, `Sheet1!$B$2:$B$5`); `None`
/// where the chart holds them itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeriesRefs {
    pub name: Option<String>,
    pub categories: Option<String>,
    pub values: Option<String>,
    /// A bubble chart's sizes (`bubbleSize`).
    pub sizes: Option<String>,
}

/// A chart's data: its workbook (`.xlsx`) and its series' references.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartData {
    pub workbook: Vec<u8>,
    pub series: Vec<SeriesRefs>,
}

/// A reference's sheet (as written, quotes and all) and its range.
pub fn parse_ref(formula: &str) -> Option<(String, CellRange)> {
    let (sheet, range) = formula.trim_start_matches('=').rsplit_once('!')?;
    let (first, last) = range.split_once(':').unwrap_or((range, range));
    let ((c0, r0), (c1, r1)) = (cell_address(first)?, cell_address(last)?);
    Some((sheet.to_string(), CellRange { first_row: r0.min(r1), first_column: c0.min(c1), last_row: r0.max(r1), last_column: c0.max(c1) }))
}

/// A reference to `range` of `sheet` (as written), absolute.
pub fn format_ref(sheet: &str, range: CellRange) -> String {
    let cell = |c: u32, r: u32| format!("${}${}", crate::csv::column_name(c), r + 1);
    if range.first_row == range.last_row && range.first_column == range.last_column {
        format!("{sheet}!{}", cell(range.first_column, range.first_row))
    } else {
        format!("{sheet}!{}:{}", cell(range.first_column, range.first_row), cell(range.last_column, range.last_row))
    }
}

/// The sheet named `sheet` (as a reference writes it) of `workbook`.
pub fn sheet_of<'a>(workbook: &'a Presentation, sheet: &str) -> Option<&'a Sheet> {
    let name = sheet.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')).map_or_else(|| sheet.to_string(), |s| s.replace("''", "'"));
    workbook.slides.iter().find(|s| s.name.as_deref() == Some(name.as_str())).and_then(|s| {
        s.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Sheet(sheet) => Some(&**sheet),
            _ => None,
        })
    })
}

/// The cells of reference `formula`, in order (down a column, then across).
pub(super) fn cells<'a>(workbook: &'a Presentation, formula: &str) -> Option<Vec<Option<&'a SheetCell>>> {
    let (sheet, range) = parse_ref(formula)?;
    let sheet = sheet_of(workbook, &sheet)?;
    let mut out = Vec::new();
    for column in range.first_column..=range.last_column {
        for row in range.first_row..=range.last_row {
            out.push(sheet.cell(row, column));
        }
    }
    Some(out)
}

/// What a cell shows (empty for none).
pub(super) fn shown(cell: Option<&SheetCell>) -> String {
    cell.map(|c| c.text.clone()).unwrap_or_default()
}

/// A cell's number, if it holds one.
pub(super) fn number(cell: Option<&SheetCell>) -> Option<f64> {
    match cell.map(|c| &c.value) {
        Some(ibis_doc_model::CellValue::Number(v)) => Some(*v),
        _ => None,
    }
}

/// The references of the chart part's series, in the chart's order.
pub(crate) fn chart_refs(doc: &XmlDocument) -> Vec<SeriesRefs> {
    let Some(root) = doc.root_element() else { return Vec::new() };
    let Some(plot) = doc.path(root, &["chart", "plotArea"]) else { return Vec::new() };
    let formula = |node: Option<NodeId>| node.and_then(|n| doc.descendants(n).into_iter().find(|&f| doc.local_name(f) == "f")).map(|f| doc.text(f)).filter(|f| !f.is_empty());
    doc.elements(plot)
        .filter(|&g| GROUP_ELEMENTS.contains(&doc.local_name(g)))
        .flat_map(|g| doc.elements(g).filter(|&s| doc.local_name(s) == "ser").collect::<Vec<_>>())
        .map(|ser| SeriesRefs {
            name: formula(doc.child(ser, "tx")),
            categories: formula(doc.child(ser, "cat").or_else(|| doc.child(ser, "xVal"))),
            values: formula(doc.child(ser, "val").or_else(|| doc.child(ser, "yVal"))),
            sizes: formula(doc.child(ser, "bubbleSize")),
        })
        .collect()
}

/// A workbook holding `chart`'s data as PowerPoint lays it out (Sheet1:
/// names in row 1 from B, categories in column A from row 2, each series'
/// values in its column, all a table), and the references to it.
pub(crate) fn workbook_from_chart(chart: &Chart) -> Result<ChartData, Error> {
    let series: Vec<&ibis_doc_model::Series> = chart.groups.iter().flat_map(|g| g.series.iter()).collect();
    let quote = |t: &str| format!("\"{}\"", t.replace('"', "\"\""));
    let categories: Vec<String> = series.iter().find(|s| !s.categories.is_empty()).map(|s| s.categories.clone()).unwrap_or_default();
    let rows = series.iter().map(|s| s.values.len()).max().unwrap_or(0).max(categories.len());
    let mut csv = String::from("\" \"");
    for s in &series {
        csv.push(',');
        csv.push_str(&quote(&s.name));
    }
    csv.push('\n');
    for i in 0..rows {
        csv.push_str(&quote(categories.get(i).map_or("", String::as_str)));
        for s in &series {
            csv.push(',');
            if let Some(Some(Number(v))) = s.values.get(i) {
                csv.push_str(&number_text(*v));
            }
        }
        csv.push('\n');
    }
    let last = rows.max(1) as u32;
    // The data made a table (テーブル1), as PowerPoint keeps it, so that
    // Excel grows the ranges with it. Its headings are unique and not empty.
    let mut headings: Vec<String> = vec![" ".into()];
    for (i, s) in series.iter().enumerate() {
        let mut h = if s.name.trim().is_empty() { format!("系列 {}", i + 1) } else { s.name.clone() };
        let mut n = 2;
        while headings.iter().any(|x| x.eq_ignore_ascii_case(&h)) {
            h = format!("{}{n}", if s.name.trim().is_empty() { format!("系列 {}", i + 1) } else { s.name.clone() });
            n += 1;
        }
        headings.push(h);
    }
    let mut csv = csv;
    if let Some(end) = csv.find('\n') {
        let first: Vec<String> = headings.iter().map(|h| quote(h)).collect();
        csv.replace_range(..end, &first.join(","));
    }
    let workbook = {
        use ibis_doc_model::{Command, SheetCommand, SheetTable};
        let opened = crate::open(crate::csv::workbook_from(&csv, "Sheet1")?)?;
        let mut doc = ibis_doc_model::Document::new(opened.presentation.clone());
        let slide = doc.presentation().slides.first().map(|s| s.id).ok_or_else(|| Error::new("ブックを作れません"))?;
        let range = CellRange { first_row: 0, first_column: 0, last_row: last, last_column: series.len() as u32 };
        let table = SheetTable {
            name: "テーブル1".into(),
            range,
            header_rows: 1,
            totals_rows: 0,
            columns: headings,
            style: None,
            filter: Some(ibis_doc_model::AutoFilter { range, columns: Vec::new() }),
            totals: Vec::new(),
        };
        doc.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index: 0, table: Some(table), cells: Vec::new() })]).map_err(|e| Error::new(format!("ブックに表を作れません: {e:?}")))?;
        opened.source.save(doc.presentation(), crate::SaveOptions::default())?
    };
    let refs = (0..series.len() as u32)
        .map(|i| SeriesRefs {
            name: Some(format_ref("Sheet1", CellRange { first_row: 0, first_column: i + 1, last_row: 0, last_column: i + 1 })),
            categories: (!categories.is_empty()).then(|| format_ref("Sheet1", CellRange { first_row: 1, first_column: 0, last_row: last, last_column: 0 })),
            values: Some(format_ref("Sheet1", CellRange { first_row: 1, first_column: i + 1, last_row: last, last_column: i + 1 })),
            sizes: None,
        })
        .collect();
    Ok(ChartData { workbook, series: refs })
}

/// The chart part's data: its embedded workbook and references, or (with
/// no workbook, or no references to one) a workbook made from `chart`.
pub(crate) fn read_chart_data(package: &Package, chart_part: &str, chart: &Chart) -> Result<ChartData, Error> {
    if super::pptx_chart_ex::is_chart_ex(package, chart_part) {
        return super::pptx_chart_ex::read_chart_data(package, chart_part);
    }
    let refs = chart_refs(&package.xml(chart_part)?);
    let book = package.relationships(chart_part)?.into_iter().find(|r| r.kind == REL_PACKAGE && !r.external).and_then(|r| r.target_part(chart_part)).filter(|p| package.contains(p));
    match book {
        Some(book) if refs.iter().any(|r| r.values.is_some()) => Ok(ChartData { workbook: package.read(&book)?.to_vec(), series: refs }),
        _ => workbook_from_chart(chart),
    }
}

/// Excel's current region: the range around `cell` bounded by empty rows
/// and columns.
fn current_region(sheet: &Sheet, cell: (u32, u32)) -> CellRange {
    let filled = |r: u32, c: u32| sheet.cell(r, c).is_some_and(|x| !matches!(x.value, ibis_doc_model::CellValue::Empty) || !x.text.is_empty());
    let mut range = CellRange { first_row: cell.0, first_column: cell.1, last_row: cell.0, last_column: cell.1 };
    loop {
        let (r0, c0) = (range.first_row.saturating_sub(1), range.first_column.saturating_sub(1));
        let (r1, c1) = (range.last_row + 1, range.last_column + 1);
        let mut grown = range;
        for r in r0..=r1 {
            for c in c0..=c1 {
                let inside = (range.first_row..=range.last_row).contains(&r) && (range.first_column..=range.last_column).contains(&c);
                if !inside && filled(r, c) {
                    grown.first_row = grown.first_row.min(r);
                    grown.last_row = grown.last_row.max(r);
                    grown.first_column = grown.first_column.min(c);
                    grown.last_column = grown.last_column.max(c);
                }
            }
        }
        if grown == range {
            return range;
        }
        range = grown;
    }
}

/// The references grown or shrunk to the data: when the series are laid
/// out as PowerPoint lays them (names in a row over their values' columns,
/// the categories in the column before, or the same across), they cover
/// the table the names' corner is in, else its current region. Other
/// layouts keep their references.
pub fn grown_refs(workbook: &Presentation, refs: &[SeriesRefs]) -> Vec<SeriesRefs> {
    grown(workbook, refs).unwrap_or_else(|| refs.to_vec())
}

fn grown(workbook: &Presentation, refs: &[SeriesRefs]) -> Option<Vec<SeriesRefs>> {
    let parsed: Vec<_> = refs.iter().map(|r| Some((parse_ref(r.name.as_deref()?)?, parse_ref(r.categories.as_deref()?)?, parse_ref(r.values.as_deref()?)?))).collect::<Option<_>>()?;
    let ((sheet, first_name), (_, cats), (_, first_values)) = parsed.first()?.clone();
    if parsed.iter().any(|((s, _), (t, _), (u, _))| *s != sheet || *t != sheet || *u != sheet) {
        return None;
    }
    // Series in columns (true) or in rows.
    let columns = first_values.first_column == first_values.last_column && cats.first_column == cats.last_column;
    // In columns: (along, across) = (row, column); in rows, the other way.
    let flip = |r: CellRange| if columns { r } else { CellRange { first_row: r.first_column, first_column: r.first_row, last_row: r.last_column, last_column: r.last_row } };
    let (cats, first_values) = (flip(cats), flip(first_values));
    let corner = (first_name.first_row, first_name.first_column);
    let corner = if columns { (corner.0, cats.first_column) } else { (cats.first_column, corner.1) };
    for (i, ((_, name), (_, c), (_, v))) in parsed.iter().enumerate() {
        let (name, c, v) = (flip(*name), flip(*c), flip(*v));
        let column = first_values.first_column + i as u32;
        let ok = name.first_row == name.last_row
            && name.first_column == column
            && name.last_column == column
            && v.first_column == column
            && v.last_column == column
            && c == cats
            && v.first_row == cats.first_row
            && v.last_row == cats.last_row
            && name.first_row + 1 == cats.first_row
            && cats.first_column + 1 == first_values.first_column;
        if !ok {
            return None;
        }
    }
    let sheet_model = sheet_of(workbook, &sheet)?;
    let (r, c) = corner;
    let region = sheet_model
        .tables
        .iter()
        .find(|t| (t.range.first_row..=t.range.last_row).contains(&r) && (t.range.first_column..=t.range.last_column).contains(&c))
        .map(|t| CellRange { last_row: t.range.last_row - t.totals_rows.min(t.range.last_row - t.range.first_row), ..t.range })
        .unwrap_or_else(|| current_region(sheet_model, corner));
    if (region.first_row, region.first_column) != corner {
        return None;
    }
    let region = flip(region);
    if region.last_row <= region.first_row || region.last_column <= region.first_column {
        return None;
    }
    let range = |r0: u32, c0: u32, r1: u32, c1: u32| format_ref(&sheet, flip(CellRange { first_row: r0, first_column: c0, last_row: r1, last_column: c1 }));
    let (top, left, bottom, right) = (region.first_row, region.first_column, region.last_row, region.last_column);
    Some(
        (left + 1..=right)
            .map(|col| SeriesRefs { name: Some(range(top, col, top, col)), categories: Some(range(top + 1, left, bottom, left)), values: Some(range(top + 1, col, bottom, col)), ..Default::default() })
            .collect(),
    )
}

/// `chart` with its series as the workbook gives them: one per reference
/// (a new one like the last of its group, the last group taking those
/// added; those beyond the references taken away), named, categorised and
/// valued from the cells. Approximately as the file reads it once written
/// (a new series' colour, say, is the file's).
pub fn chart_from_data(chart: &Chart, workbook: &Presentation, refs: &[SeriesRefs]) -> Chart {
    chart_from_cells(chart, workbook, refs, false)
}

/// `chart_from_data` for a chart of a worksheet: it goes on reading the cells (`Chart::cell_refs`) and follows them as they change.
pub fn sheet_chart_from_data(chart: &Chart, workbook: &Presentation, refs: &[SeriesRefs]) -> Chart {
    chart_from_cells(chart, workbook, refs, true)
}

fn chart_from_cells(chart: &Chart, workbook: &Presentation, refs: &[SeriesRefs], follow: bool) -> Chart {
    // A chartEx chart is drawn from its part as written: the model is read again after saving.
    if chart.ex.is_some() {
        return chart.clone();
    }
    let mut out = chart.clone();
    // The series each group keeps, ensuring multi-group charts preserve at least one series per group when possible.
    let mut left = refs.len();
    let groups = out.groups.len();
    for (i, group) in out.groups.iter_mut().enumerate() {
        let remaining_groups = groups.saturating_sub(1 + i);
        let max_for_this = if left > remaining_groups { left - remaining_groups } else { 1 };
        let want = if i + 1 == groups {
            left
        } else {
            group.series.len().min(max_for_this).max(1)
        };
        left = left.saturating_sub(want);
        if group.series.is_empty() {
            continue;
        }
        while group.series.len() < want {
            let mut extra = group.series.last().cloned().expect("a series");
            extra.point_fills.clear();
            extra.point_strokes.clear();
            extra.point_explosions.clear();
            if let Some(labels) = extra.labels.as_mut() {
                labels.points.clear();
            }
            group.series.push(extra);
        }
        group.series.truncate(want);
    }
    let scatter: Vec<bool> = out.groups.iter().flat_map(|g| g.series.iter().map(|_| matches!(g.kind, ibis_doc_model::ChartType::Scatter { .. } | ibis_doc_model::ChartType::Bubble { .. }))).collect();
    for (i, ((series, r), is_scatter)) in out.groups.iter_mut().flat_map(|g| g.series.iter_mut()).zip(refs).zip(scatter).enumerate() {
        if let Some(cells) = r.name.as_deref().and_then(|f| cells(workbook, f)) {
            let n = cells.into_iter().map(shown).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
            series.name = if n.is_empty() { format!("系列{}", i + 1) } else { n };
        } else if series.name.is_empty() {
            // A series with no name formula owns a literal chart name. Keep it
            // when refreshing cell values, including names edited in the UI.
            series.name = if crate::english_names() { format!("Series{}", i + 1) } else { format!("系列{}", i + 1) }; // i18n-ok（文書に保存する名前）
        }
        if let Some(cells) = r.categories.as_deref().and_then(|f| cells(workbook, f)) {
            if is_scatter {
                series.x_values = cells.into_iter().map(|c| number(c).map(Number)).collect();
            } else {
                series.categories = cells.into_iter().map(shown).collect();
            }
        } else if is_scatter {
            series.x_values.clear();
        } else {
            series.categories.clear();
        }
        if let Some(cells) = r.values.as_deref().and_then(|f| cells(workbook, f)) {
            series.values = cells.into_iter().map(|c| number(c).map(Number)).collect();
        }
        if let Some(cells) = r.sizes.as_deref().and_then(|f| cells(workbook, f)) {
            series.bubble_sizes = cells.into_iter().map(|c| number(c).map(Number)).collect();
        }
    }
    // The chart goes on reading these cells: it follows them as they change.
    if follow {
        out.cell_refs = refs.iter().map(|r| ibis_doc_model::SeriesCells { name: r.name.clone(), categories: r.categories.clone(), values: r.values.clone(), sizes: r.sizes.clone() }).collect();
    }
    out
}

/// A chart's series references, as the model keeps them.
pub fn refs_of(chart: &Chart) -> Vec<SeriesRefs> {
    chart.cell_refs.iter().map(|r| SeriesRefs { name: r.name.clone(), categories: r.categories.clone(), values: r.values.clone(), sizes: r.sizes.clone() }).collect()
}

/// Whether two charts read the same cells but show different data (the cells changed), or read different cells.
pub(crate) fn data_differs(a: &Chart, b: &Chart) -> bool {
    if a.cell_refs != b.cell_refs {
        return true;
    }
    let (sa, sb): (Vec<_>, Vec<_>) = (a.groups.iter().flat_map(|g| &g.series).collect(), b.groups.iter().flat_map(|g| &g.series).collect());
    sa.len() == sb.len() && sa.iter().zip(&sb).any(|(x, y)| x.name != y.name || x.categories != y.categories || x.values != y.values || x.x_values != y.x_values || x.bubble_sizes != y.bubble_sizes)
}

/// The `prefix:` of `node`'s name.
fn prefix_of(doc: &XmlDocument, node: NodeId) -> String {
    doc.name(node).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default()
}

/// `node`'s children taken away.
fn clear(doc: &mut XmlDocument, node: NodeId) {
    for c in doc.children(node).to_vec() {
        doc.detach(c);
    }
}

/// A string reference (`c:strRef`) under `parent`: `formula` and its cache.
fn write_str_ref(doc: &mut XmlDocument, parent: NodeId, p: &str, formula: &str, texts: &[String]) {
    let reference = doc.append_element(parent, &format!("{p}strRef"));
    let f = doc.append_element(reference, &format!("{p}f"));
    doc.set_text(f, formula);
    let cache = doc.append_element(reference, &format!("{p}strCache"));
    let count = doc.append_element(cache, &format!("{p}ptCount"));
    doc.set_attr(count, "val", &texts.len().to_string());
    for (i, t) in texts.iter().enumerate().filter(|(_, t)| !t.is_empty()) {
        let pt = doc.append_element(cache, &format!("{p}pt"));
        doc.set_attr(pt, "idx", &i.to_string());
        let v = doc.append_element(pt, &format!("{p}v"));
        doc.set_text(v, t);
    }
}

/// A number reference (`c:numRef`) under `parent`: `formula` and its cache.
fn write_num_ref(doc: &mut XmlDocument, parent: NodeId, p: &str, formula: &str, format: &str, values: &[Option<f64>]) {
    let reference = doc.append_element(parent, &format!("{p}numRef"));
    let f = doc.append_element(reference, &format!("{p}f"));
    doc.set_text(f, formula);
    let cache = doc.append_element(reference, &format!("{p}numCache"));
    let code = doc.append_element(cache, &format!("{p}formatCode"));
    doc.set_text(code, format);
    let count = doc.append_element(cache, &format!("{p}ptCount"));
    doc.set_attr(count, "val", &values.len().to_string());
    for (i, v) in values.iter().enumerate() {
        if let Some(v) = v {
            let pt = doc.append_element(cache, &format!("{p}pt"));
            doc.set_attr(pt, "idx", &i.to_string());
            let node = doc.append_element(pt, &format!("{p}v"));
            doc.set_text(node, &number_text(*v));
        }
    }
}

/// The format code of the number cache under `node`, if any.
fn format_code(doc: &XmlDocument, node: NodeId) -> Option<String> {
    doc.descendants(node).into_iter().find(|&n| doc.local_name(n) == "formatCode").map(|n| doc.text(n))
}

/// The element of a series where `name` goes (CT_BarSer and the like:
/// `idx`, `order`, `tx`, `spPr`, …, `cat`, `val`, …), made if missing.
fn series_child(doc: &mut XmlDocument, ser: NodeId, name: &str, before: &[&str]) -> NodeId {
    if let Some(n) = doc.child(ser, name) {
        return n;
    }
    let p = prefix_of(doc, ser);
    let at = doc.children(ser).iter().position(|&c| doc.is_element(c) && before.contains(&doc.local_name(c))).unwrap_or(doc.children(ser).len());
    doc.insert_element(ser, at, &format!("{p}{name}"))
}

/// The chart part's series made as many as `counts` says in each group (new
/// ones made like the group's last, numbered after the others, in the next
/// accent colour), in the chart's order.
fn resize_series(doc: &mut XmlDocument, chart_part: &str, counts: &[usize], scatter_ok: bool) -> Result<Vec<NodeId>, Error> {
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{chart_part} が空です")))?;
    let plot = doc.path(root, &["chart", "plotArea"]).ok_or_else(|| unsupported("このグラフのデータ"))?;
    let groups: Vec<NodeId> = doc.elements(plot).filter(|&g| GROUP_ELEMENTS.contains(&doc.local_name(g))).collect();
    if groups.len() != counts.len() {
        return Err(unsupported("このグラフのデータ"));
    }
    let mut next_idx = doc
        .descendants(plot)
        .into_iter()
        .filter(|&n| matches!(doc.local_name(n), "idx" | "order") && doc.parent(n).is_some_and(|s| doc.local_name(s) == "ser"))
        .filter_map(|n| doc.attr(n, "val").and_then(|v| v.parse::<u32>().ok()))
        .max()
        .map_or(0, |m| m + 1);
    let mut all = Vec::new();
    for (&group, &want) in groups.iter().zip(counts) {
        if !scatter_ok && matches!(doc.local_name(group), "scatterChart" | "bubbleChart") {
            return Err(unsupported("散布図・バブル チャートのデータの編集"));
        }
        let mut sers: Vec<NodeId> = doc.elements(group).filter(|&s| doc.local_name(s) == "ser").collect();
        if sers.is_empty() || want == 0 {
            return Err(unsupported("系列の無くなるグラフのデータ"));
        }
        while sers.len() > want {
            doc.detach(sers.pop().expect("a series"));
        }
        while sers.len() < want {
            let last = *sers.last().expect("a series");
            let copy = doc.deep_clone(last);
            doc.insert_after(last, copy);
            for name in ["idx", "order"] {
                if let Some(n) = doc.child(copy, name) {
                    doc.set_attr(n, "val", &next_idx.to_string());
                }
            }
            // Its own points' looks and labels are the series copied's.
            for n in doc.elements(copy).filter(|&n| doc.local_name(n) == "dPt").collect::<Vec<_>>() {
                doc.detach(n);
            }
            if let Some(labels) = doc.child(copy, "dLbls") {
                for n in doc.elements(labels).filter(|&n| doc.local_name(n) == "dLbl").collect::<Vec<_>>() {
                    doc.detach(n);
                }
            }
            // Office's id of the series (c16:uniqueId) is the copied one's: not kept.
            if let Some(ext_list) = doc.child(copy, "extLst") {
                for ext in doc.elements(ext_list).filter(|&e| doc.elements(e).any(|c| doc.local_name(c) == "uniqueId")).collect::<Vec<_>>() {
                    doc.detach(ext);
                }
                if doc.elements(ext_list).next().is_none() {
                    doc.detach(ext_list);
                }
            }
            if let Some(sp) = doc.child(copy, "spPr") {
                for n in doc.descendants(sp).into_iter().filter(|&n| doc.local_name(n) == "schemeClr" && doc.attr(n, "val").is_some_and(|v| v.starts_with("accent"))).collect::<Vec<_>>() {
                    doc.set_attr(n, "val", &format!("accent{}", next_idx % 6 + 1));
                }
            }
            next_idx += 1;
            sers.push(copy);
        }
        all.extend(sers);
    }
    Ok(all)
}

/// The chart part's series rewritten from `data`: as many as it has
/// references in each group as `after` has (new ones made like the group's
/// last, numbered after the others, in the next accent colour), each
/// one's references and caches from the workbook's cells; the workbook put
/// in as the chart's (related anew if it had none).
pub(crate) fn write_chart_data(package: &mut Package, chart_part: &str, after: &Chart, data: &ChartData) -> Result<(), Error> {
    let opened = crate::open(data.workbook.clone()).map_err(|e| Error::new(format!("グラフのデータのブックを開けません: {e}")))?;
    let workbook = opened.presentation;
    let counts: Vec<usize> = after.groups.iter().map(|g| g.series.len()).collect();
    if counts.iter().sum::<usize>() != data.series.len() {
        return Err(unsupported("系列の数の合わないグラフのデータ"));
    }
    let doc = package.xml_mut(chart_part)?;
    let all = resize_series(doc, chart_part, &counts, false)?;
    const AFTER_TX: &[&str] = &["spPr", "invertIfNegative", "pictureOptions", "marker", "dPt", "dLbls", "trendline", "errBars", "explosion", "cat", "val", "smooth", "shape", "extLst"];
    const AFTER_CAT: &[&str] = &["val", "smooth", "shape", "bubble3D", "extLst"];
    const AFTER_VAL: &[&str] = &["smooth", "shape", "bubble3D", "extLst"];
    for (ser, r) in all.into_iter().zip(&data.series) {
        let p = prefix_of(doc, ser);
        if let Some(f) = &r.name {
            let texts = cells(&workbook, f).ok_or_else(|| unsupported("グラフのデータの範囲"))?;
            let name = texts.into_iter().map(shown).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ");
            let tx = series_child(doc, ser, "tx", AFTER_TX);
            clear(doc, tx);
            write_str_ref(doc, tx, &p, f, &[name]);
        }
        if let Some(f) = &r.categories {
            let list = cells(&workbook, f).ok_or_else(|| unsupported("グラフのデータの範囲"))?;
            let cat = series_child(doc, ser, "cat", AFTER_CAT);
            if doc.descendants(cat).into_iter().any(|n| doc.local_name(n) == "multiLvlStrRef") {
                return Err(unsupported("複数レベルの項目のデータの編集"));
            }
            let numeric = doc.descendants(cat).into_iter().any(|n| matches!(doc.local_name(n), "numRef" | "numLit")) && list.iter().all(|c| c.is_none() || number(*c).is_some());
            let format = format_code(doc, cat).unwrap_or_else(|| "General".into());
            clear(doc, cat);
            if numeric {
                write_num_ref(doc, cat, &p, f, &format, &list.into_iter().map(number).collect::<Vec<_>>());
            } else {
                write_str_ref(doc, cat, &p, f, &list.into_iter().map(shown).collect::<Vec<_>>());
            }
        }
        if let Some(f) = &r.values {
            let list = cells(&workbook, f).ok_or_else(|| unsupported("グラフのデータの範囲"))?;
            let val = series_child(doc, ser, "val", AFTER_VAL);
            let format = format_code(doc, val).unwrap_or_else(|| "General".into());
            clear(doc, val);
            write_num_ref(doc, val, &p, f, &format, &list.into_iter().map(number).collect::<Vec<_>>());
        }
    }
    // The workbook.
    let existing = package.relationships(chart_part)?.into_iter().find(|r| r.kind == REL_PACKAGE && !r.external).and_then(|r| r.target_part(chart_part));
    match existing {
        Some(book) => package.set_bytes(&book, data.workbook.clone()),
        None => {
            let book = next_part_name(package, "ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx");
            package.set_bytes(&book, data.workbook.clone());
            package.set_override(&book, CT_XLSX)?;
            let rel = package.add_relationship(chart_part, REL_PACKAGE, &relative(chart_part, &book), false)?;
            let doc = package.xml_mut(chart_part)?;
            let root = doc.root_element().ok_or_else(|| Error::new(format!("{chart_part} が空です")))?;
            if let Some(old) = doc.child(root, "externalData") {
                doc.detach(old);
            }
            let p = prefix_of(doc, root);
            let r = doc.lookup_prefix(root, "http://schemas.openxmlformats.org/officeDocument/2006/relationships").filter(|r| !r.is_empty());
            let at = doc.children(root).iter().position(|&c| doc.is_element(c) && matches!(doc.local_name(c), "printSettings" | "userShapes" | "extLst")).unwrap_or(doc.children(root).len());
            let external = doc.insert_element(root, at, &format!("{p}externalData"));
            match r {
                Some(r) => doc.set_attr(external, &format!("{r}:id"), &rel),
                None => {
                    doc.set_attr(external, "xmlns:r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships");
                    doc.set_attr(external, "r:id", &rel);
                }
            }
            let update = doc.append_element(external, &format!("{p}autoUpdate"));
            doc.set_attr(update, "val", "0");
        }
    }
    Ok(())
}

/// A sheet name as a reference writes it: in quotes when it has a space or
/// a mark, or begins with a digit.
pub fn quote_sheet(name: &str) -> String {
    let plain = name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') && !name.chars().next().is_some_and(|c| c.is_ascii_digit());
    if plain { name.to_string() } else { format!("'{}'", name.replace('\'', "''")) }
}

/// The range a chart's series take their data from (names, categories
/// and values together), its sheet as written, and whether the series are
/// in columns.
pub fn refs_range(refs: &[SeriesRefs]) -> Option<(String, CellRange, bool)> {
    let mut sheet: Option<String> = None;
    let mut all: Option<CellRange> = None;
    for f in refs.iter().flat_map(|r| [r.name.as_deref(), r.categories.as_deref(), r.values.as_deref(), r.sizes.as_deref()]).flatten() {
        let (s, r) = parse_ref(f)?;
        if sheet.get_or_insert_with(|| s.clone()) != &s {
            return None;
        }
        all = Some(match all {
            None => r,
            Some(a) => CellRange { first_row: a.first_row.min(r.first_row), first_column: a.first_column.min(r.first_column), last_row: a.last_row.max(r.last_row), last_column: a.last_column.max(r.last_column) },
        });
    }
    let by_columns = if let Some(first_ref) = refs.first().and_then(|r| r.values.as_deref()).and_then(parse_ref) {
        let first = first_ref.1;
        if first.first_column == first.last_column && first.first_row < first.last_row {
            true
        } else if first.first_row == first.last_row && first.first_column < first.last_column {
            false
        } else if refs.len() > 1 {
            if let Some(second_ref) = refs.get(1).and_then(|r| r.values.as_deref()).and_then(parse_ref) {
                second_ref.1.first_column != first.first_column
            } else {
                first.first_column == first.last_column
            }
        } else if let Some(cat_ref) = refs.first().and_then(|r| r.categories.as_deref()).and_then(parse_ref) {
            let cat = cat_ref.1;
            cat.first_column == cat.last_column && cat.first_row < cat.last_row
        } else {
            first.first_column == first.last_column
        }
    } else {
        true
    };
    Some((sheet?, all?, by_columns))
}

/// The series of a chart over `range` of `sheet` (as written), the first row
/// naming them and the first column the categories: in columns, or in rows.
/// `None` when the range has no such heading row and column of text.
pub fn refs_for_range(workbook: &Presentation, sheet: &str, range: CellRange, by_columns: bool) -> Option<Vec<SeriesRefs>> {
    use ibis_doc_model::CellValue;
    let model=sheet_of(workbook,sheet)?;
    let (first,last,count,series_first,series_last)=if by_columns{(range.first_row,range.last_row,range.last_row-range.first_row+1,range.first_column,range.last_column)}else{(range.first_column,range.last_column,range.last_column-range.first_column+1,range.first_row,range.last_row)};
    let cell=|point:u32,series:u32|{let(r,c)=if by_columns{(point,series)}else{(series,point)};model.cell(r,c).map(|c|&c.value)};
    let number=|point,series|matches!(cell(point,series),Some(CellValue::Number(_)));
    let header=|series|count>1 && matches!(cell(first,series),Some(CellValue::Text(t))if !t.is_empty()) && (first+1..=last).all(|point|number(point,series));
    let category=series_first<series_last && (first+u32::from(count>1)..=last).all(|point|matches!(cell(point,series_first),Some(CellValue::Text(t))if !t.is_empty()));
    let values_first=series_first+u32::from(category);
    let one=|point0,series,point1|{let(r0,c0,r1,c1)=if by_columns{(point0,series,point1,series)}else{(series,point0,series,point1)};format_ref(sheet,CellRange{first_row:r0,first_column:c0,last_row:r1,last_column:c1})};
    let mut out=Vec::new();
    for series in values_first..=series_last{
        let has_header=header(series);let data_first=first+u32::from(has_header);
        if !(data_first..=last).any(|point|number(point,series)){continue;}
        out.push(SeriesRefs{name:has_header.then(||one(first,series,first)),categories:category.then(||one(data_first,series_first,last)),values:Some(one(data_first,series,last)),..Default::default()});
    }
    (!out.is_empty()).then_some(out)
}

/// Choose between complete numeric series layouts. Equal point counts favour columns.
pub fn preferred_series_columns(workbook:&Presentation,sheet:&str,range:CellRange)->bool{
    let score=|by_columns|->Option<u32>{
        let refs=refs_for_range(workbook,sheet,range,by_columns)?;
        let model=sheet_of(workbook,sheet)?;
        let mut points=0;
        // A layout that leaves a line of the range out (a row of headings, read as series by rows) is not complete.
        let lines=if by_columns{range.last_column-range.first_column+1}else{range.last_row-range.first_row+1};
        let category=refs.first().is_some_and(|s|s.categories.is_some());
        if refs.len() as u32+u32::from(category)<lines{return None;}
        for series in refs{
            let (_,r)=parse_ref(series.values.as_deref()?)?;
            if !(r.first_row..=r.last_row).all(|row|(r.first_column..=r.last_column).all(|col|matches!(model.cell(row,col).map(|c|&c.value),Some(ibis_doc_model::CellValue::Number(_))))){return None;}
            points=points.max((r.last_row-r.first_row+1)*(r.last_column-r.first_column+1));
        }Some(points)
    };
    match(score(true),score(false)){(Some(a),Some(b))=>a>=b,(Some(_),None)=>true,(None,Some(_))=>false,(None,None)=>range.last_row-range.first_row>=range.last_column-range.first_column}
}

/// The series of a scatter chart over `range` of `sheet`:
/// When `by_columns` is true: the first column the X values, each other column
/// a series of Y values (named by the first row if headings, or unnamed
/// if pure numeric data points).
/// When `by_columns` is false: the first row the X values, each other row
/// a series of Y values (named by the first column if headings, or unnamed
/// if pure numeric data points).
/// `None` when the range has no numbers.
pub fn scatter_refs_for_range(workbook: &Presentation, sheet: &str, range: CellRange, by_columns: bool) -> Option<Vec<SeriesRefs>> {
    let model = sheet_of(workbook, sheet)?;
    let is_number = |r: u32, c: u32| matches!(model.cell(r, c).map(|x| &x.value), Some(ibis_doc_model::CellValue::Number(_)));
    let (r0, c0, r1, c1) = (range.first_row, range.first_column, range.last_row, range.last_column);
    let one = |r0: u32, c0: u32, r1: u32, c1: u32| format_ref(sheet, CellRange { first_row: r0, first_column: c0, last_row: r1, last_column: c1 });
    if by_columns {
        if c1 <= c0 {
            return None;
        }
        let has_header = r1>r0 && (c0..=c1).all(|c| matches!(model.cell(r0,c).map(|x|&x.value),Some(ibis_doc_model::CellValue::Text(t)) if !t.is_empty())) && (r0+1..=r1).all(|r|(c0..=c1).all(|c|is_number(r,c)));
        let data_r0 = if has_header { r0 + 1 } else { r0 };
        if r1 < data_r0 {
            return None;
        }
        if !(data_r0..=r1).any(|r| (c0..=c1).any(|c| is_number(r, c))) {
            return None;
        }
        Some((c0 + 1..=c1).map(|c| SeriesRefs {
            name: if has_header { Some(one(r0, c, r0, c)) } else { None },
            categories: Some(one(data_r0, c0, r1, c0)),
            values: Some(one(data_r0, c, r1, c)),
            ..Default::default()
        }).collect())
    } else {
        if r1 <= r0 {
            return None;
        }
        let has_header = c1>c0 && (r0..=r1).all(|r| matches!(model.cell(r,c0).map(|x|&x.value),Some(ibis_doc_model::CellValue::Text(t)) if !t.is_empty())) && (c0+1..=c1).all(|c|(r0..=r1).all(|r|is_number(r,c)));
        let data_c0 = if has_header { c0 + 1 } else { c0 };
        if c1 < data_c0 {
            return None;
        }
        if !(data_c0..=c1).any(|c| (r0..=r1).any(|r| is_number(r, c))) {
            return None;
        }
        Some((r0 + 1..=r1).map(|r| SeriesRefs {
            name: if has_header { Some(one(r, c0, r, c0)) } else { None },
            categories: Some(one(r0, data_c0, r0, c1)),
            values: Some(one(r, data_c0, r, c1)),
            ..Default::default()
        }).collect())
    }
}

/// The series of a bubble chart over `range` of `sheet`:
/// When `by_columns` is true: the first column the X values, pairs of columns
/// after that the Y values (named by the first row if headings, or unnamed
/// if pure numeric data points) and bubble sizes.
/// When `by_columns` is false: the first row the X values, pairs of rows
/// after that the Y values (named by the first column if headings, or unnamed
/// if pure numeric data points) and bubble sizes.
/// `None` when the range does not have pairs of number columns/rows.
pub fn bubble_refs_for_range(workbook: &Presentation, sheet: &str, range: CellRange, by_columns: bool) -> Option<Vec<SeriesRefs>> {
    let model = sheet_of(workbook, sheet)?;
    let is_number = |r: u32, c: u32| matches!(model.cell(r, c).map(|x| &x.value), Some(ibis_doc_model::CellValue::Number(_)));
    let (r0, c0, r1, c1) = (range.first_row, range.first_column, range.last_row, range.last_column);
    let one = |r0: u32, c0: u32, r1: u32, c1: u32| format_ref(sheet, CellRange { first_row: r0, first_column: c0, last_row: r1, last_column: c1 });
    if by_columns {
        if c1 < c0 + 2 || (c1 - c0) % 2 != 0 {
            return None;
        }
        let has_header = (c0..=c1).any(|c| !is_number(r0, c));
        let data_r0 = if has_header { r0 + 1 } else { r0 };
        if r1 < data_r0 {
            return None;
        }
        if !(data_r0..=r1).all(|r| (c0..=c1).all(|c| is_number(r, c))) {
            return None;
        }
        let pairs = (c1 - c0) / 2;
        Some((0..pairs).map(|k| {
            let y_col = c0 + 1 + 2 * k;
            let size_col = c0 + 2 + 2 * k;
            SeriesRefs {
                name: if has_header { Some(one(r0, y_col, r0, y_col)) } else { None },
                categories: Some(one(data_r0, c0, r1, c0)),
                values: Some(one(data_r0, y_col, r1, y_col)),
                sizes: Some(one(data_r0, size_col, r1, size_col)),
            }
        }).collect())
    } else {
        if r1 < r0 + 2 || (r1 - r0) % 2 != 0 {
            return None;
        }
        let has_header = (r0..=r1).any(|r| !is_number(r, c0));
        let data_c0 = if has_header { c0 + 1 } else { c0 };
        if c1 < data_c0 {
            return None;
        }
        if !(data_c0..=c1).all(|c| (r0..=r1).all(|r| is_number(r, c))) {
            return None;
        }
        let pairs = (r1 - r0) / 2;
        Some((0..pairs).map(|k| {
            let y_row = r0 + 1 + 2 * k;
            let size_row = r0 + 2 + 2 * k;
            SeriesRefs {
                name: if has_header { Some(one(y_row, c0, y_row, c0)) } else { None },
                categories: Some(one(r0, data_c0, r0, c1)),
                values: Some(one(y_row, data_c0, y_row, c1)),
                sizes: Some(one(size_row, data_c0, size_row, c1)),
            }
        }).collect())
    }
}

/// The series of a stock chart over `range` of `sheet`:
/// When `by_columns` is true:
/// - If `cols >= val_count + 1`: column 0 is categories (e.g. date), followed by `val_count` value columns.
/// - If `cols == val_count`: no categories column (categories: None), all columns are value columns.
/// - If the first row of value columns contains non-numeric cells, it is treated as a heading row
///   (`name: Some(...)`), and values start at `first_row + 1`. Otherwise (pure numeric / headerless),
///   `name: None`, and values start at `first_row`.
/// When `by_columns` is false:
/// - Same logic across rows.
/// `None` when the range does not have enough columns/rows or required number cells.
pub fn stock_refs_for_range(
    workbook: &Presentation,
    sheet: &str,
    range: CellRange,
    kind: NewChartKind,
    by_columns: bool,
) -> Option<Vec<SeriesRefs>> {
    let val_count = match kind {
        NewChartKind::StockHLC => 3,
        NewChartKind::StockOHLC => 4,
        NewChartKind::StockVHLC => 4,
        NewChartKind::StockVOHLC => 5,
        _ => return None,
    };
    let model = sheet_of(workbook, sheet)?;
    let is_number = |r: u32, c: u32| matches!(model.cell(r, c).map(|x| &x.value), Some(ibis_doc_model::CellValue::Number(_)));
    let (r0, c0, r1, c1) = (range.first_row, range.first_column, range.last_row, range.last_column);
    let one = |r0: u32, c0: u32, r1: u32, c1: u32| format_ref(sheet, CellRange { first_row: r0, first_column: c0, last_row: r1, last_column: c1 });

    if by_columns {
        let cols = c1.checked_sub(c0)? + 1;
        if cols < val_count {
            return None;
        }
        let (has_cat, val_c0) = if cols >= val_count + 1 {
            (true, c0 + 1)
        } else {
            (false, c0)
        };
        let val_c1 = val_c0 + val_count - 1;
        let has_header = (val_c0..=val_c1).any(|c| !is_number(r0, c));
        let data_r0 = if has_header { r0 + 1 } else { r0 };
        if r1 < data_r0 {
            return None;
        }
        if !(data_r0..=r1).all(|r| (val_c0..=val_c1).all(|c| is_number(r, c))) {
            return None;
        }
        Some((0..val_count).map(|k| {
            let col = val_c0 + k;
            SeriesRefs {
                name: if has_header { Some(one(r0, col, r0, col)) } else { None },
                categories: if has_cat { Some(one(data_r0, c0, r1, c0)) } else { None },
                values: Some(one(data_r0, col, r1, col)),
                sizes: None,
            }
        }).collect())
    } else {
        let rows = r1.checked_sub(r0)? + 1;
        if rows < val_count {
            return None;
        }
        let (has_cat, val_r0) = if rows >= val_count + 1 {
            (true, r0 + 1)
        } else {
            (false, r0)
        };
        let val_r1 = val_r0 + val_count - 1;
        let has_header = (val_r0..=val_r1).any(|r| !is_number(r, c0));
        let data_c0 = if has_header { c0 + 1 } else { c0 };
        if c1 < data_c0 {
            return None;
        }
        if !(val_r0..=val_r1).all(|r| (data_c0..=c1).all(|c| is_number(r, c))) {
            return None;
        }
        Some((0..val_count).map(|k| {
            let row = val_r0 + k;
            SeriesRefs {
                name: if has_header { Some(one(row, c0, row, c0)) } else { None },
                categories: if has_cat { Some(one(r0, data_c0, r0, c1)) } else { None },
                values: Some(one(row, data_c0, row, c1)),
                sizes: None,
            }
        }).collect())
    }
}

/// A sheet chart's series (xlsx) set to `refs`, as many in each group as
/// `after` has: each one's references written, and its caches from the
/// series of `after` (which holds what the cells give).
pub(crate) fn write_sheet_chart_series(package: &mut Package, chart_part: &str, after: &Chart, refs: &[SeriesRefs]) -> Result<(), Error> {
    let counts: Vec<usize> = after.groups.iter().map(|g| g.series.len()).collect();
    if counts.iter().sum::<usize>() != refs.len() {
        return Err(unsupported("系列の数の合わないグラフのデータ"));
    }
    let doc = package.xml_mut(chart_part)?;
    super::chart_topology::sync_series(doc,after,refs)?;
    let all = resize_series(doc, chart_part, &counts, true)?;
    const AFTER_TX: &[&str] = &["spPr", "invertIfNegative", "pictureOptions", "marker", "dPt", "dLbls", "trendline", "errBars", "explosion", "cat", "val", "smooth", "shape", "extLst"];
    const AFTER_CAT: &[&str] = &["val", "smooth", "shape", "bubble3D", "extLst"];
    const AFTER_VAL: &[&str] = &["smooth", "shape", "bubble3D", "extLst"];
    let series: Vec<&ibis_doc_model::Series> = after.groups.iter().flat_map(|g| g.series.iter()).collect();
    let root=doc.root_element().ok_or_else(||unsupported("グラフのルート要素"))?;
    let drawing_prefix=doc.lookup_prefix(root,"http://schemas.openxmlformats.org/drawingml/2006/main").filter(|p|!p.is_empty()).unwrap_or_else(||{doc.set_attr(root,"xmlns:a","http://schemas.openxmlformats.org/drawingml/2006/main");"a".into()});
    for ((ser, r), model) in all.into_iter().zip(refs).zip(series) {
        let p = prefix_of(doc, ser);
        // Rebuilding references and caches must also retain each series' model appearance.
        // The caller's settings baseline already has these groups, so no later delta writes them.
        super::pptx_chart::write_series_fill(doc,ser,&model.fill,&p,&drawing_prefix)?;
        super::pptx_chart::write_series_stroke(doc,ser,model.stroke.as_ref(),&p,&drawing_prefix)?;
        super::pptx_chart::write_series_marker(doc,ser,model.marker.as_ref(),&p,&drawing_prefix)?;
        super::pptx_chart::write_labels(doc,ser,None,model.labels.as_ref())?;
        super::pptx_chart::write_trendline(doc,ser,model.trendline.as_ref())?;
        super::pptx_chart::write_error_bars(doc,ser,model.error_bars.as_ref())?;
        if let Some(f) = &r.name {
            let tx = series_child(doc, ser, "tx", AFTER_TX);
            clear(doc, tx);
            write_str_ref(doc, tx, &p, f, &[model.name.clone()]);
        } else if !model.name.is_empty() {
            let tx=series_child(doc,ser,"tx",AFTER_TX);clear(doc,tx);
            let value=doc.append_element(tx,&format!("{p}v"));doc.set_text(value,&model.name);
        } else if let Some(tx) = doc.child(ser, "tx") {
            doc.detach(tx);
        }
        // A scatter chart's X values (`xVal`) and Y values (`yVal`).
        if doc.parent(ser).is_some_and(|g| matches!(doc.local_name(g), "scatterChart" | "bubbleChart")) {
            if let Some(f) = &r.categories {
                let x = series_child(doc, ser, "xVal", &["yVal", "bubbleSize", "smooth", "extLst"]);
                let format = format_code(doc, x).unwrap_or_else(|| "General".into());
                clear(doc, x);
                write_num_ref(doc, x, &p, f, &format, &model.x_values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>());
            }
            if let Some(f) = &r.values {
                let y = series_child(doc, ser, "yVal", &["bubbleSize", "smooth", "bubble3D", "extLst"]);
                let format = format_code(doc, y).unwrap_or_else(|| if model.format.is_empty() { "General".into() } else { model.format.clone() });
                clear(doc, y);
                write_num_ref(doc, y, &p, f, &format, &model.values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>());
            }
            if let Some(f) = &r.sizes {
                let z = series_child(doc, ser, "bubbleSize", &["bubble3D", "extLst"]);
                let format = format_code(doc, z).unwrap_or_else(|| "General".into());
                clear(doc, z);
                write_num_ref(doc, z, &p, f, &format, &model.bubble_sizes.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>());
            }
            continue;
        }
        if let Some(f) = &r.categories {
            let cat = series_child(doc, ser, "cat", AFTER_CAT);
            if doc.descendants(cat).into_iter().any(|n| doc.local_name(n) == "multiLvlStrRef") {
                return Err(unsupported("複数レベルの項目のデータの編集"));
            }
            let numbers: Vec<Option<f64>> = model.categories.iter().map(|c| c.trim().parse::<f64>().ok()).collect();
            let numeric = doc.descendants(cat).into_iter().any(|n| matches!(doc.local_name(n), "numRef" | "numLit")) && model.categories.iter().zip(&numbers).all(|(c, n)| c.is_empty() || n.is_some());
            let format = format_code(doc, cat).unwrap_or_else(|| "General".into());
            clear(doc, cat);
            if numeric {
                write_num_ref(doc, cat, &p, f, &format, &numbers);
            } else {
                write_str_ref(doc, cat, &p, f, &model.categories);
            }
        } else if let Some(cat) = doc.child(ser, "cat") {
            doc.detach(cat);
        }
        if let Some(f) = &r.values {
            let val = series_child(doc, ser, "val", AFTER_VAL);
            let format = format_code(doc, val).unwrap_or_else(|| if model.format.is_empty() { "General".into() } else { model.format.clone() });
            clear(doc, val);
            write_num_ref(doc, val, &p, f, &format, &model.values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>());
        }
    }
    Ok(())
}

/// Write a PivotChart's series (`c:ser`) into `chart_part` from `after.groups`,
/// resizing `<c:ser>` elements as needed and writing each series's name, categories, and values.
pub(crate) fn write_pivot_chart_series(package: &mut Package, chart_part: &str, after: &Chart) -> Result<(), Error> {
    let counts: Vec<usize> = after.groups.iter().map(|g| g.series.len()).collect();
    let doc = package.xml_mut(chart_part)?;
    let all = resize_series(doc, chart_part, &counts, true)?;
    const AFTER_TX: &[&str] = &["spPr", "invertIfNegative", "pictureOptions", "marker", "dPt", "dLbls", "trendline", "errBars", "explosion", "cat", "val", "smooth", "shape", "extLst"];
    const AFTER_CAT: &[&str] = &["val", "smooth", "shape", "bubble3D", "extLst"];
    const AFTER_VAL: &[&str] = &["smooth", "shape", "bubble3D", "extLst"];
    let series: Vec<&ibis_doc_model::Series> = after.groups.iter().flat_map(|g| g.series.iter()).collect();
    for (ser, model) in all.into_iter().zip(series) {
        let p = prefix_of(doc, ser);
        if !model.name.is_empty() {
            let tx = series_child(doc, ser, "tx", AFTER_TX);
            let existing_f = doc.descendants(tx).into_iter().find(|&n| doc.local_name(n) == "f").map(|n| doc.text(n));
            clear(doc, tx);
            if let Some(f) = existing_f {
                write_str_ref(doc, tx, &p, &f, &[model.name.clone()]);
            } else {
                let v = doc.append_element(tx, &format!("{p}v"));
                doc.set_text(v, &model.name);
            }
        } else if let Some(tx) = doc.child(ser, "tx") {
            doc.detach(tx);
        }
        if !model.categories.is_empty() {
            let cat = series_child(doc, ser, "cat", AFTER_CAT);
            let existing_f = doc.descendants(cat).into_iter().find(|&n| doc.local_name(n) == "f").map(|n| doc.text(n));
            clear(doc, cat);
            if let Some(f) = existing_f {
                write_str_ref(doc, cat, &p, &f, &model.categories);
            } else {
                let lit = doc.append_element(cat, &format!("{p}strLit"));
                let count = doc.append_element(lit, &format!("{p}ptCount"));
                doc.set_attr(count, "val", &model.categories.len().to_string());
                for (i, t) in model.categories.iter().enumerate().filter(|(_, t)| !t.is_empty()) {
                    let pt = doc.append_element(lit, &format!("{p}pt"));
                    doc.set_attr(pt, "idx", &i.to_string());
                    let v = doc.append_element(pt, &format!("{p}v"));
                    doc.set_text(v, t);
                }
            }
        } else if let Some(cat) = doc.child(ser, "cat") {
            doc.detach(cat);
        }
        let val = series_child(doc, ser, "val", AFTER_VAL);
        let format = format_code(doc, val).unwrap_or_else(|| if model.format.is_empty() { "General".into() } else { model.format.clone() });
        let existing_f = doc.descendants(val).into_iter().find(|&n| doc.local_name(n) == "f").map(|n| doc.text(n));
        clear(doc, val);
        let nums: Vec<Option<f64>> = model.values.iter().map(|v| v.map(|n| n.0)).collect();
        if let Some(f) = existing_f {
            write_num_ref(doc, val, &p, &f, &format, &nums);
        } else {
            let lit = doc.append_element(val, &format!("{p}numLit"));
            let code = doc.append_element(lit, &format!("{p}formatCode"));
            doc.set_text(code, &format);
            let count = doc.append_element(lit, &format!("{p}ptCount"));
            doc.set_attr(count, "val", &nums.len().to_string());
            for (i, v) in nums.iter().enumerate() {
                if let Some(v) = v {
                    let pt = doc.append_element(lit, &format!("{p}pt"));
                    doc.set_attr(pt, "idx", &i.to_string());
                    let node = doc.append_element(pt, &format!("{p}v"));
                    doc.set_text(node, &number_text(*v));
                }
            }
        }
    }
    Ok(())
}

/// The chart data edits by chart: each model of the chart and the data it
/// was made from ([`crate::Source::set_chart_data`]).
pub(crate) type ChartDataEdits = std::collections::HashMap<ibis_doc_model::ShapeId, Vec<(Chart, ChartData)>>;

/// The data `chart` (of shape `shape`) was made from, if it was made from
/// one (the latest such).
pub(crate) fn matching<'a>(edits: &'a ChartDataEdits, shape: ibis_doc_model::ShapeId, chart: &Chart) -> Option<&'a ChartData> {
    edits.get(&shape)?.iter().rev().find(|(c, _)| c == chart).map(|(_, d)| d)
}

/// `after` with the data labels of `before`'s series (a new series its
/// own: those of the series it was copied from, as written) and the rest of
/// `before`: what the chart part holds once its data is rewritten.
pub(crate) fn as_before(before: &Chart, after: &Chart) -> Chart {
    let groups = after
        .groups
        .iter()
        .enumerate()
        .map(|(gi, g)| ibis_doc_model::ChartGroup {
            series: g
                .series
                .iter()
                .enumerate()
                .map(|(si, s)| ibis_doc_model::Series {
                    labels: before.groups.get(gi).and_then(|b| b.series.get(si)).map_or_else(|| s.labels.clone(), |b| b.labels.clone()),
                    ..s.clone()
                })
                .collect(),
            ..g.clone()
        })
        .collect();
    Chart { groups, cell_refs: after.cell_refs.clone(), ..before.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_parse_and_format() {
        let (sheet, range) = parse_ref("'売上 2026'!$B$2:$B$5").unwrap();
        assert_eq!(sheet, "'売上 2026'");
        assert_eq!(format_ref(&sheet, range), "'売上 2026'!$B$2:$B$5");
        assert_eq!(format_ref("Sheet1", CellRange { first_row: 0, first_column: 27, last_row: 0, last_column: 27 }), "Sheet1!$AB$1");
    }

    #[test]
    fn ranges_follow_the_data() {
        // PowerPoint's layout: 3 series over 4 categories, then a row and a column more.
        let csv = " ,a,b,c,d\nx,1,2,3,4\ny,1,2,3,4\nz,1,2,3,4\nw,1,2,3,4\nv,1,2,3,4\n";
        let book = crate::read(&crate::csv::workbook_from(csv, "Sheet1").unwrap()).unwrap();
        let refs: Vec<SeriesRefs> = ["B", "C", "D"]
            .iter()
            .map(|c| SeriesRefs { name: Some(format!("Sheet1!${c}$1")), categories: Some("Sheet1!$A$2:$A$5".into()), values: Some(format!("Sheet1!${c}$2:${c}$5")), ..Default::default() })
            .collect();
        let grown = grown_refs(&book, &refs);
        assert_eq!(grown.len(), 4);
        assert_eq!(grown[3], SeriesRefs { name: Some("Sheet1!$E$1".into()), categories: Some("Sheet1!$A$2:$A$6".into()), values: Some("Sheet1!$E$2:$E$6".into()), ..Default::default() });
        // Another layout keeps its references.
        let odd = vec![SeriesRefs { name: None, categories: None, values: Some("Sheet1!$B$2:$B$3".into()), ..Default::default() }];
        assert_eq!(grown_refs(&book, &odd), odd);
    }
}
