// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Writing changes to a workbook (`.xlsx`) — the spreadsheet担当's file.
//!
//! The reader (`reader.rs` `read_xlsx_with_sources`) records the
//! workbook's parts: the workbook, styles, shared strings and calculation
//! chain, and each sheet's part in the workbook's order. A cell is found in
//! its sheet's `sheetData` by its reference.
//!
//! What can be saved so far:
//!
//! - Cells' values and formulas: written into their `c` elements (made
//!   where there are none, in order), the rest of the element kept. Text
//!   goes into the shared strings (an existing plain string is used
//!   again), formulas get the prefixes a file needs (`_xlfn.`) and their
//!   calculated value, and the calculation chain gains and loses the
//!   formula cells. A shared formula whose group changes is written out
//!   cell by cell.
//! - Cells' formats: a format still as read is written as its `cellXfs`
//!   entry; a changed one as a new entry made from the one it came from
//!   (keeping what the model does not hold), with new fonts, fills,
//!   borders and number formats as needed. Nothing already in `styles.xml`
//!   is changed or removed, and a format made once is used again.
//!
//! Anything else that changed is reported as not saved yet.

use std::collections::{BTreeMap, HashMap, HashSet};

use ibis_calc::formula::to_file;
use ibis_calc::number_to_xml;
use ibis_doc_model::{
    Border, BorderLine, CellAlign, CellGradient, CellRange, CellStyle, CellUnderline, CellValue, CellVerticalAlign, Presentation, Rgb, ShapeKind, Sheet, SheetCell, VerticalPosition, cell_name,
};
use ibis_ooxml::{NodeId, Package, XmlDocument};

use super::unsupported;
use crate::Error;

#[path = "xlsx_structure.rs"]
mod structure;
#[path = "xlsx_sizes.rs"]
pub(crate) mod sizes;
#[path = "xlsx_pivot.rs"]
mod pivots;
#[path = "xlsx_slicer.rs"]
mod slicers;
#[path = "xlsx_external.rs"]
mod externals;
#[path = "xlsx_shapes.rs"]
mod shape_edits;

/// Where the things of a workbook's model came from.
#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    /// The workbook part (`xl/workbook.xml`).
    pub workbook: String,
    pub styles: Option<String>,
    pub shared_strings: Option<String>,
    pub calc_chain: Option<String>,
    /// Each sheet's part, in the workbook's order (`None` for a sheet that
    /// is not a worksheet); the same order as the model's
    /// `Workbook::sheets`.
    pub sheet_parts: Vec<Option<String>>,
    /// Each sheet's `sheetId`, in the same order (for the calculation chain).
    pub sheet_ids: Vec<u32>,
    /// The default font's widest digit, in points: column widths are
    /// counted in it.
    pub digit: f64,
    /// The sheets' charts as read, for saving changes to them.
    pub charts: Vec<ChartSource>,
    /// The sheets' shapes in drawings as read, for saving changes to them (e.g. alt text).
    pub shapes: Vec<XlsxShapeSource>,
    /// Each external workbook link's part (`xl/externalLinks/externalLink1.xml`),
    /// in the order `[1]`… number them.
    pub external_parts: Vec<Option<String>>,
}

/// Where a chart on a sheet came from: its shape, its chart part, and its
/// drawing part with the graphic frame's `cNvPr` id there.
#[derive(Clone, Debug)]
pub struct ChartSource {
    pub shape: ibis_doc_model::ShapeId,
    pub chart: String,
    pub drawing: String,
    pub frame_id: Option<String>,
}

/// Where a shape, picture, or graphic frame on a sheet came from in drawings:
/// its shape id, its drawing part, and its `cNvPr` id there.
#[derive(Clone, Debug)]
pub struct XlsxShapeSource {
    pub shape: ibis_doc_model::ShapeId,
    pub drawing: String,
    pub c_nv_pr_id: String,
}

const REL_SHARED_STRINGS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings";
const CT_SHARED_STRINGS: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";
const REL_COMMENTS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
const CT_COMMENTS: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml";
const REL_DRAWING: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const CT_DRAWING: &str = "application/vnd.openxmlformats-officedocument.drawing+xml";
const REL_CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

fn sheet_of(slide: &ibis_doc_model::Slide) -> Option<&Sheet> {
    slide.shapes.iter().find_map(|s| match &s.kind {
        ShapeKind::Sheet(sheet) => Some(&**sheet),
        _ => None,
    })
}

/// A new element, not yet in the tree.
fn detached(doc: &mut XmlDocument, name: &str) -> NodeId {
    let e = doc.insert_element(XmlDocument::ROOT, 0, name);
    doc.detach(e);
    e
}

/// The prefix (with `:`) the element `id` is written with, for new
/// elements beside it.
fn prefix_of(doc: &XmlDocument, id: NodeId) -> String {
    doc.prefix(id).map(|p| format!("{p}:")).unwrap_or_default()
}

fn conditional_source_ids(p: &Presentation) -> Vec<Option<Vec<Option<usize>>>> {
    p.workbook.as_deref().map(|w|w.sheets.iter().map(|entry| {
        let sheet=entry.slide.and_then(|id|p.slide(id)).and_then(sheet_of).or(entry.hidden.as_deref());
        sheet.map(|s|s.conditional.iter().map(|f|f.source_index).collect())
    }).collect()).unwrap_or_default()
}

pub(crate) fn write_changes(package: &mut Package, map: &SourceMap, baseline: &Presentation, current: &Presentation, chart_data: &super::pptx_chart_data::ChartDataEdits, new_charts: &HashMap<ibis_doc_model::ShapeId, super::NewChartEntry>) -> Result<(), Error> {
    let has_svg = current.media.iter().any(|m| m.format == ibis_doc_model::MediaFormat::Svg)
        || package.names().any(|n| n.ends_with(".svg"));
    if has_svg && package.content_type("dummy.svg")?.is_none() {
        super::pptx_shapes::add_default_content_type(package, "svg", "image/svg+xml")?;
    }
    if super::pptx::same_content(baseline, current) && baseline.workbook == current.workbook && conditional_source_ids(baseline)==conditional_source_ids(current) {
        return Ok(());
    }
    let ids = |p: &Presentation| p.slides.iter().map(|s| s.id).collect::<Vec<_>>();
    let mut map = map.clone();
    let map = &mut map;
    // Structural changes first: made in the file, and in the model as read,
    // which the cells are then compared with.
    let transformed;
    let baseline = match current.workbook.as_deref().map(|w| w.structure.as_slice()) {
        Some(edits) if !edits.is_empty() => {
            let mut b = baseline.clone();
            for edit in edits {
                structure::replay(package, map, &b, edit)?;
                // A sheet added with cells (a copy): its part is written
                // empty, so the model as read gets it empty too, and the
                // cells, sizes and merged ranges are written as changes.
                let edit = match edit {
                    ibis_doc_model::sheet_ops::StructureEdit::AddSheet { sheet, name, slide, shape, content } if !content.cells.is_empty() || !content.merges.is_empty() => {
                        let blank = Sheet {
                            cells: Default::default(),
                            styles: Vec::new(),
                            merges: Vec::new(),
                            columns: vec![content.default_column; content.columns.len()],
                            rows: vec![content.default_row; content.rows.len()],
                            hidden_rows: Default::default(),
            custom_rows: Default::default(),
            custom_columns: Default::default(),
                            hidden_columns: Default::default(),
                            spills: Default::default(),
                            validations: Vec::new(),
                            hyperlinks: Vec::new(),
                            outline_rows: Default::default(),
                            outline_columns: Default::default(),
                            frozen: None,
                            split: None,
                            ..(**content).clone()
                        };
                        ibis_doc_model::sheet_ops::StructureEdit::AddSheet { sheet: *sheet, name: name.clone(), slide: *slide, shape: *shape, content: std::sync::Arc::new(blank) }
                    }
                    other => other.clone(),
                };
                ibis_doc_model::sheet_ops::apply_structure(&mut b, &edit);
            }
            if let Some(w) = b.workbook.as_mut() {
                std::sync::Arc::make_mut(w).structure = edits.to_vec();
            }
            transformed = b;
            &transformed
        }
        _ => baseline,
    };
    if ids(baseline) != ids(current) {
        return Err(unsupported("シートの並べ替え"));
    }
    let map: &mut SourceMap = map;
    // Defined names changed: written into the workbook part.
    let names = |p: &Presentation| p.workbook.as_ref().map(|w| w.names.clone()).unwrap_or_default();
    if names(baseline) != names(current) {
        write_names(package, &map.workbook, &names(baseline), &names(current), &external_book_names(current))?;
    }
    let custom_styles = |p: &Presentation| p.workbook.as_ref().map(|w| w.custom_cell_styles.clone()).unwrap_or_default();
    if custom_styles(baseline) != custom_styles(current) {
        write_custom_cell_styles(package, map.styles.as_deref(), &custom_styles(current))?;
    }
    let compatibility = |p: &Presentation| p.workbook.as_deref().and_then(|w| w.compatibility_version);
    let compatibility_changed = compatibility(baseline) != compatibility(current);
    if compatibility_changed {
        let version = compatibility(current).filter(|&v| matches!(v, 1 | 2)).ok_or_else(|| unsupported("計算の互換バージョン"))?;
        write_compatibility_version(package, &map.workbook, version)?;
    }
    let calc_mode = |p: &Presentation| p.workbook.as_deref().map(|w| (w.calc_mode, w.calc_on_save, w.full_precision, w.iterate, w.max_iterations, w.max_change));
    if calc_mode(baseline) != calc_mode(current) {
        if let Some((mode, on_save, full_precision, iterate, max_iterations, max_change)) = calc_mode(current) {
            write_calc_mode(package, &map.workbook, mode, on_save, full_precision, iterate, max_iterations, max_change.0)?;
        }
    }
    let protection = |p: &Presentation| p.workbook.as_deref().and_then(|w| w.protection.clone());
    if protection(baseline) != protection(current) {
        write_workbook_protection(package, &map.workbook, protection(current).as_ref())?;
    }
    let custom_views = |p: &Presentation| p.workbook.as_ref().map(|w| w.custom_views.clone()).unwrap_or_default();
    if custom_views(baseline) != custom_views(current) {
        write_custom_workbook_views(package, &map.workbook, &custom_views(current))?;
    }
    let properties = |p: &Presentation| p.workbook.as_ref().map(|w| w.properties.clone());
    if properties(baseline) != properties(current) {
        if let Some(props) = properties(current) {
            super::docx::write_core_properties(package, &props)?;
            super::docx::write_app_properties(package, &props)?;
            super::docx::write_custom_properties(package, &props.custom, false)?;
        }
    }
    let theme_colors = |p: &Presentation| p.workbook.as_ref().map(|w| w.theme_colors.clone()).unwrap_or_default();
    let theme_fonts = |p: &Presentation| p.workbook.as_ref().and_then(|w| w.theme_fonts.clone());
    if theme_colors(baseline) != theme_colors(current) || theme_fonts(baseline) != theme_fonts(current) {
        write_theme(package, &map.workbook, &theme_colors(current), theme_fonts(current).as_ref())?;
    }
    let window_hidden = |p: &Presentation| p.workbook.as_ref().map_or(false, |w| w.window_hidden);
    if window_hidden(baseline) != window_hidden(current) {
        write_workbook_window_hidden(package, &map.workbook, window_hidden(current))?;
    }
    let queries = |p: &Presentation| p.workbook.as_deref().map(|w| (w.queries.clone(), w.query_parameters.clone())).unwrap_or_default();
    if queries(baseline) != queries(current) {
        write_queries(package, &map.workbook, &queries(current))?;
    }
    let without_names = |p: &Presentation| p.workbook.as_deref().map(|w| ibis_doc_model::Workbook { names: Vec::new(), compatibility_version: None, custom_cell_styles: Vec::new(), calc_mode: ibis_doc_model::CalcMode::Auto, calc_on_save: true, full_precision: true, iterate: false, max_iterations: 100, max_change: ibis_doc_model::SheetFloat(0.001), protection: None, properties: Default::default(), theme_colors: Vec::new(), theme_fonts: None, custom_views: Vec::new(), window_hidden: false, external_books: Vec::new(), queries: Vec::new(), query_parameters: Vec::new(), ..w.clone() });
    if without_names(baseline) != without_names(current) {
        return Err(unsupported("ブック（シート名・名前の定義など）の変更"));
    }
    if let Some(old_wb)=baseline.workbook.as_deref() && let Some(new_wb)=current.workbook.as_deref() {
        for (a,b) in old_wb.sheets.iter().zip(&new_wb.sheets) {
            if a.slide.is_none() && b.slide.is_none() && let Some((a,b))=a.hidden.as_deref().zip(b.hidden.as_deref())
                && !sizes::conditional_sources_eq(&a.conditional,&b.conditional) {return Err(unsupported("非表示シートの条件付き書式の元XML対応変更"));}
        }
    }
    let workbook = current.workbook.as_deref().ok_or_else(|| unsupported("このブックの変更"))?;
    let multi_names: HashSet<String> = workbook.names.iter().filter(|n| n.formula.contains(':') || n.formula.contains(',')).map(|n| n.name.to_lowercase()).collect();
    let mut moved_chart_ids = move_chart_drawings(package, map, baseline, current, chart_data)?;
    moved_chart_ids.extend(shape_edits::write_shape_edits(package, map, baseline, current, &moved_chart_ids.clone(), new_charts, chart_data)?);
    let mut writer = Writer { package, map, baseline, styles: None, strings: None, calc: CalcChanges::default(), multi_names, books: external_book_names(current) };
    for (old_slide, new_slide) in baseline.slides.iter().zip(&current.slides) {
        if old_slide == new_slide && sheet_of(old_slide).zip(sheet_of(new_slide)).is_none_or(|(a,b)| sizes::conditional_sources_eq(&a.conditional,&b.conditional)) {
            continue;
        }
        if old_slide.name != new_slide.name {
            return Err(unsupported("シート名の変更"));
        }
        let others = |s: &ibis_doc_model::Slide| s.shapes.iter().filter(|sh| !matches!(sh.kind, ShapeKind::Sheet(_)) && !moved_chart_ids.contains(&sh.id)).cloned().collect::<Vec<_>>();
        let (Some(old), Some(new)) = (sheet_of(old_slide), sheet_of(new_slide)) else {
            return Err(unsupported("このシートの変更"));
        };
        let (old_shapes, new_shapes) = (others(old_slide), others(new_slide));
        // Slicers, timelines, and form controls deleted: their parts go (below); the rest is compared.
        let removed: Vec<ibis_doc_model::Shape> = old_shapes.iter().filter(|s| matches!(s.kind, ShapeKind::Slicer(_) | ShapeKind::Timeline(_) | ShapeKind::Control(_)) && !new_shapes.iter().any(|n| n.id == s.id)).cloned().collect();
        let old_shapes: Vec<ibis_doc_model::Shape> = old_shapes.into_iter().filter(|s| !removed.iter().any(|r| r.id == s.id)).collect();
        if old_shapes != new_shapes {
            write_charts(writer.package, &writer.map.charts, new, &old_shapes, &new_shapes, chart_data)?;
        }
        write_sheet_shape_labels(writer.package, &writer.map.shapes, &writer.map.charts, old_slide, new_slide)?;
        let index = workbook.sheets.iter().position(|s| s.slide == Some(new_slide.id)).ok_or_else(|| unsupported("このシートの変更"))?;
        let part = map.sheet_parts.get(index).cloned().flatten().ok_or_else(|| unsupported("このシートの変更"))?;
        let sheet_id = map.sheet_ids.get(index).copied().unwrap_or(index as u32 + 1);
        let sheet_name = &workbook.sheets[index].name;
        slicers::write_slicer_frames(writer.package, &part, new, &old_shapes, &new_shapes)?;
        slicers::write_timeline_frames(writer.package, &part, new, &old_shapes, &new_shapes)?;
        write_control_changes(writer.package, &part, new, &old_shapes, &new_shapes)?;
        if !removed.is_empty() {
            let slicer_of = |s: &ibis_doc_model::Shape| match &s.kind { ShapeKind::Slicer(x) => Some((**x).clone()), _ => None };
            let gone: Vec<_> = removed.iter().filter_map(slicer_of).collect();
            let left: Vec<_> = current.slides.iter().flat_map(|s| s.shapes.iter()).filter_map(slicer_of).collect();
            slicers::write_slicer_removals(writer.package, &part, &map.workbook, &gone, &left)?;
            let timeline_of = |s: &ibis_doc_model::Shape| match &s.kind { ShapeKind::Timeline(x) => Some((**x).clone()), _ => None };
            let gone_tl: Vec<_> = removed.iter().filter_map(timeline_of).collect();
            let left_tl: Vec<_> = current.slides.iter().flat_map(|s| s.shapes.iter()).filter_map(timeline_of).collect();
            slicers::write_timeline_removals(writer.package, &part, &map.workbook, &gone_tl, &left_tl)?;
            let control_of = |s: &ibis_doc_model::Shape| match &s.kind { ShapeKind::Control(x) => Some((**x).clone()), _ => None };
            let gone_ctrl: Vec<_> = removed.iter().filter_map(control_of).collect();
            write_control_removals(writer.package, &part, &gone_ctrl)?;
        }
        writer.write_sheet(&part, sheet_id, sheet_name, old, new)?;
    }
    writer.finish()?;
    slicers::write_slicer_selection(package, &map.workbook, baseline, current)?;
    slicers::write_slicer_settings(package, map, baseline, current)?;
    slicers::write_timeline_settings_and_state(package, map, baseline, current)?;
    externals::write_external_links(package, &map.workbook, &map.external_parts, baseline, current)?;
    if compatibility_changed || stale_formulas(baseline, current) {
        full_calculation_on_load(package, &map.workbook)?;
    }
    if (current.media.iter().any(|m| m.format == ibis_doc_model::MediaFormat::Svg)
        || package.names().any(|n| n.ends_with(".svg")))
        && package.content_type("dummy.svg")?.is_none()
    {
        super::pptx_shapes::add_default_content_type(package, "svg", "image/svg+xml")?;
    }
    Ok(())
}

/// Whether a formula the engine cannot calculate refers to a changed cell:
/// its value in the file is then out of date, and Excel is to calculate
/// the workbook again when it opens it.
fn stale_formulas(baseline: &Presentation, current: &Presentation) -> bool {
    let view = ibis_doc_model::sheet_calc::BookView::new(current);
    let mut changed = Vec::new();
    let Some(wb) = current.workbook.as_deref() else { return false };
    for (index, ws) in wb.sheets.iter().enumerate() {
        let Some(id) = ws.slide else { continue };
        let new = current.slide(id).and_then(sheet_of);
        let old = baseline.slide(id).and_then(sheet_of);
        let Some(new) = new else { continue };
        match old {
            Some(old) => changed.extend(changed_cells(old, new).into_iter().filter(|c| c.2.map(|x| (&x.value, &x.formula)) != c.3.map(|x| (&x.value, &x.formula))).map(|c| (index, c.0, c.1))),
            None => changed.extend(new.cells.iter().map(|c| (index, c.row, c.column))),
        }
    }
    !changed.is_empty() && view.recalculate(&changed).iter().any(|r| r.unsupported)
}

/// Write theme colors and fonts to the workbook's theme part (`xl/theme/theme1.xml`).
fn write_theme(package: &mut Package, workbook: &str, colors: &[(String, Rgb)], fonts: Option<&ibis_doc_model::ThemeFonts>) -> Result<(), Error> {
    let mut parts = Vec::new();
    if let Ok(rels) = package.relationships(workbook) {
        for rel in rels.into_iter().filter(|r| !r.external && (r.kind == crate::package::REL_THEME || r.kind.ends_with("/theme"))) {
            if let Some(target) = rel.target_part(workbook) {
                if package.contains(&target) && !parts.contains(&target) {
                    parts.push(target);
                }
            }
        }
    }
    if parts.is_empty() && package.contains("xl/theme/theme1.xml") {
        parts.push("xl/theme/theme1.xml".to_string());
    }
    for part in parts {
        let doc = package.xml_mut(&part)?;
        super::pptx_theme::write_theme_elements(doc, colors, fonts);
    }
    Ok(())
}

/// The external workbooks' names, in the order `[1]`… number them.
pub(super) fn external_book_names(p: &Presentation) -> Vec<String> {
    p.workbook.as_deref().map(|w| w.external_books.iter().map(|b| b.name.clone()).collect()).unwrap_or_default()
}

/// The name of the table of `sheet` that covers the cell.
fn table_at(sheet: &Sheet, row: u32, column: u32) -> Option<&str> {
    sheet.tables.iter().find(|t| t.range.first_row <= row && row <= t.range.last_row && t.range.first_column <= column && column <= t.range.last_column).map(|t| t.name.as_str())
}

/// A formula as the file stores it: the external workbooks by number (a
/// workbook the file does not link cannot be saved).
pub(super) fn file_formula(books: &[String], formula: &str) -> Result<String, Error> {
    file_formula_in(books, formula, None)
}

/// [`file_formula`] of a formula in table `table` (a bare `[@Qty]` is of it).
pub(super) fn file_formula_in(books: &[String], formula: &str, table: Option<&str>) -> Result<String, Error> {
    let formula = &ibis_calc::formula::this_row_to_file(formula, table);
    if let Some(book) = ibis_calc::formula::unknown_books(formula, books).first() {
        return Err(unsupported(&format!("新しい外部ブック（{book}）への参照")));
    }
    Ok(to_file(&ibis_calc::formula::books_by_index(formula, books)))
}

/// The defined names as the model has them, in `workbook/definedNames`:
/// changed ones get their new formula, deleted ones go, new ones are added
/// (others are left as written).
fn write_names(package: &mut Package, workbook: &str, old: &[ibis_doc_model::DefinedName], new: &[ibis_doc_model::DefinedName], books: &[String]) -> Result<(), Error> {
    let doc = package.xml_mut(workbook)?;
    let root = doc.root_element().ok_or_else(|| Error::new("ブックの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    let list = match doc.child(root, "definedNames") {
        Some(l) => l,
        None => {
            let l = detached(doc, &format!("{prefix}definedNames"));
            // After sheets (and functionGroups, externalReferences).
            let before = ["calcPr", "oleSize", "customWorkbookViews", "pivotCaches", "smartTagPr", "smartTagTypes", "webPublishing", "fileRecoveryPr", "webPublishObjects", "extLst"];
            let anchor = doc.elements(root).find(|&c| before.contains(&doc.local_name(c)));
            match anchor {
                Some(a) => doc.insert_before(a, l),
                None => {
                    let n = doc.children(root).len();
                    doc.insert(root, n, l);
                }
            }
            l
        }
    };
    let key = |n: &ibis_doc_model::DefinedName| (n.name.to_lowercase(), n.sheet);
    let elements: Vec<(NodeId, (String, Option<usize>))> = doc
        .children_named(list, "definedName")
        .filter_map(|e| Some((e, (doc.attr(e, "name")?.to_lowercase(), doc.attr(e, "localSheetId").and_then(|v| v.parse::<usize>().ok())))))
        .collect();
    let sheet_names: Vec<String> = doc
        .child(root, "sheets")
        .map(|s| doc.children_named(s, "sheet").filter_map(|sh| doc.attr(sh, "name").map(str::to_string)).collect())
        .unwrap_or_default();
    let default_sheet = sheet_names.first().cloned().unwrap_or_else(|| "Sheet1".to_string());
    let format_formula = |n: &ibis_doc_model::DefinedName| -> String {
        let sheet_name = n.sheet.and_then(|idx| sheet_names.get(idx)).unwrap_or(&default_sheet);
        let qualified = ibis_calc::formula::rewrite_references(&n.formula, |area| {
            if area.sheet.is_none() {
                let mut a = area.clone();
                a.sheet = Some(ibis_calc::formula::SheetRef { first: sheet_name.clone(), last: None, book: None, path: None });
                a.start.row_fixed = true;
                a.start.column_fixed = true;
                a.end.row_fixed = true;
                a.end.column_fixed = true;
                Some(a)
            } else {
                Some(area.clone())
            }
        });
        let no_spaces = ibis_calc::formula::strip_formula_spaces(&qualified);
        to_file(&ibis_calc::formula::books_by_index(&no_spaces, books))
    };
    for (e, k) in &elements {
        match new.iter().find(|n| key(n) == *k) {
            None => doc.detach(*e),
            Some(n) => {
                if old.iter().find(|o| key(o) == *k).is_none_or(|o| o.formula != n.formula) {
                    doc.set_text(*e, &format_formula(n));
                }
            }
        }
    }
    for n in new.iter().filter(|n| !elements.iter().any(|(_, k)| *k == key(n))) {
        let e = detached(doc, &format!("{prefix}definedName"));
        doc.set_attr(e, "name", &n.name);
        if let Some(i) = n.sheet {
            doc.set_attr(e, "localSheetId", &i.to_string());
        }
        if n.hidden {
            doc.set_attr(e, "hidden", "1");
        }
        doc.set_text(e, &format_formula(n));
        let count = doc.children(list).len();
        doc.insert(list, count, e);
    }
    if doc.elements(list).next().is_none() {
        doc.detach(list);
    }
    Ok(())
}

/// Change only the selected compatibility version, retaining extension metadata.
fn write_compatibility_version(package: &mut Package, workbook: &str, version: u32) -> Result<(), Error> {
    const NS: &str = "http://schemas.microsoft.com/office/spreadsheetml/2024/workbookCompatibilityVersion";
    let doc = package.xml_mut(workbook)?;
    let root = doc.root_element().ok_or_else(|| Error::new("ブックの XML が空です"))?;
    if doc.local_name(root) != "workbook" || doc.namespace(root) != Some(MAIN_NS) { return Err(unsupported("計算の互換バージョンの名前空間")); }
    let lists: Vec<_> = doc.elements(root).filter(|&n| doc.local_name(n) == "extLst" && doc.namespace(n) == Some(MAIN_NS)).collect();
    if lists.len() > 1 { return Err(unsupported("重複したブック拡張")); }
    let extensions: Vec<_> = lists.iter().copied().flat_map(|n| doc.elements(n))
        .filter(|&n| doc.local_name(n) == "ext" && doc.namespace(n) == Some(MAIN_NS) && doc.attr(n, "uri") == Some("{D14903EA-33C4-47F7-8F05-3474C54BE107}")).collect();
    let nodes: Vec<_> = extensions.iter().copied().flat_map(|n| doc.elements(n))
        .filter(|&n| doc.local_name(n) == "version" && doc.namespace(n) == Some(NS)).collect();
    if extensions.len() > 1 || nodes.len() > 1 { return Err(unsupported("重複した計算の互換バージョン")); }
    if !extensions.is_empty() && nodes.is_empty() { return Err(unsupported("不明な計算の互換バージョン拡張")); }
    let node = match nodes.first().copied() {
        Some(node) => node,
        None => {
            let list = match lists.first().copied() {
                Some(list) => list,
                None => doc.append_element(root, &format!("{}extLst", prefix_of(doc, root))),
            };
            let ext = doc.append_element(list, &format!("{}ext", prefix_of(doc, list)));
            doc.set_attr(ext, "uri", "{D14903EA-33C4-47F7-8F05-3474C54BE107}");
            let node = doc.append_element(ext, "ibiswcv:version");
            doc.set_attr(node, "xmlns:ibiswcv", NS);
            node
        }
    };
    if doc.attr(node, "warnBelowVersion").is_some_and(|v| crate::xlsx::metadata_index(v).is_none_or(|minimum| minimum == 0)) {
        return Err(unsupported("不正な計算互換版の警告下限"));
    }
    doc.set_attr(node, "setVersion", &version.to_string());
    // An explicit downgrade must not leave a minimum above the selected version.
    if doc.attr(node, "warnBelowVersion").and_then(crate::xlsx::metadata_index).is_some_and(|v| v > version) {
        doc.set_attr(node, "warnBelowVersion", &version.to_string());
    }
    Ok(())
}

fn get_or_create_calc_pr(doc: &mut XmlDocument, root: NodeId) -> NodeId {
    match doc.child(root, "calcPr") {
        Some(c) => c,
        None => {
            let c = detached(doc, &format!("{}calcPr", prefix_of(doc, root)));
            // After sheets, functionGroups, externalReferences and definedNames.
            let before = ["oleSize", "customWorkbookViews", "pivotCaches", "smartTagPr", "smartTagTypes", "webPublishing", "fileRecoveryPr", "webPublishObjects", "extLst"];
            let anchor = doc.elements(root).find(|&e| before.contains(&doc.local_name(e)));
            match anchor {
                Some(a) => doc.insert_before(a, c),
                None => {
                    let n = doc.children(root).len();
                    doc.insert(root, n, c);
                }
            }
            c
        }
    }
}

/// Set `calcPr/@fullCalcOnLoad` in the workbook part, and remove obsolete `calcChain.xml`.
fn full_calculation_on_load(package: &mut Package, workbook: &str) -> Result<(), Error> {
    let doc = package.xml_mut(workbook)?;
    let root = doc.root_element().ok_or_else(|| Error::new("ブックの XML が空です"))?;
    let calc = get_or_create_calc_pr(doc, root);
    doc.set_attr(calc, "fullCalcOnLoad", "1");

    // When full recalculation on load is requested, an existing static calculation chain
    // conflicts with Excel's runtime dependency tree calculation and causes repair prompts.
    // Excel regenerates calcChain on open if absent, so remove the conflicting part.
    let rels_to_remove: Vec<String> = package.relationships(workbook)?
        .into_iter()
        .filter(|rel| rel.kind.ends_with("/calcChain"))
        .map(|rel| rel.id)
        .collect();
    for id in rels_to_remove {
        package.remove_relationship(workbook, &id)?;
    }
    let chain_part = "xl/calcChain.xml";
    if package.read(chain_part).is_ok() {
        package.remove(chain_part);
        let _ = package.remove_override(chain_part);
    }
    Ok(())
}

/// Set or clear calculation mode attributes in `calcPr`.
fn write_calc_mode(
    package: &mut Package,
    workbook: &str,
    mode: ibis_doc_model::CalcMode,
    calc_on_save: bool,
    full_precision: bool,
    iterate: bool,
    max_iterations: u32,
    max_change: f64,
) -> Result<(), Error> {
    let doc = package.xml_mut(workbook)?;
    let root = doc.root_element().ok_or_else(|| Error::new("ブックの XML が空です"))?;
    let calc = get_or_create_calc_pr(doc, root);
    if doc.attr(calc, "calcId").is_none() {
        doc.set_attr(calc, "calcId", "191029");
    }
    match mode {
        ibis_doc_model::CalcMode::Auto => {
            doc.remove_attr(calc, "calcMode");
        }
        ibis_doc_model::CalcMode::AutoNoTable => {
            doc.set_attr(calc, "calcMode", "autoNoTable");
        }
        ibis_doc_model::CalcMode::Manual => {
            doc.set_attr(calc, "calcMode", "manual");
        }
    }
    if !calc_on_save {
        doc.set_attr(calc, "calcOnSave", "0");
    } else {
        doc.remove_attr(calc, "calcOnSave");
    }
    if !full_precision {
        doc.set_attr(calc, "fullPrecision", "0");
    } else {
        doc.remove_attr(calc, "fullPrecision");
    }
    if iterate {
        doc.set_attr(calc, "iterate", "1");
        doc.set_attr(calc, "maxIterations", &max_iterations.to_string());
        doc.set_attr(calc, "maxChange", &max_change.to_string());
    } else {
        doc.remove_attr(calc, "iterate");
        doc.remove_attr(calc, "maxIterations");
        doc.remove_attr(calc, "maxChange");
    }
    Ok(())
}

/// ブックの保護: `workbookProtection` as the model has it, or none.
fn write_workbook_protection(package: &mut Package, workbook: &str, protection: Option<&ibis_doc_model::WorkbookProtection>) -> Result<(), Error> {
    let doc = package.xml_mut(workbook)?;
    let root = doc.root_element().ok_or_else(|| Error::new("ブックの XML が空です"))?;
    if let Some(old) = doc.child(root, "workbookProtection") {
        doc.detach(old);
    }
    let Some(p) = protection else { return Ok(()) };
    let prefix = prefix_of(doc, root);
    let e = detached(doc, &format!("{prefix}workbookProtection"));
    if let Some(h) = &p.hash {
        doc.set_attr(e, "workbookAlgorithmName", &h.algorithm);
        doc.set_attr(e, "workbookHashValue", &h.hash);
        doc.set_attr(e, "workbookSaltValue", &h.salt);
        doc.set_attr(e, "workbookSpinCount", &h.spin_count.to_string());
    } else if let Some(legacy) = &p.legacy {
        doc.set_attr(e, "workbookPassword", legacy);
    }
    if p.lock_structure {
        doc.set_attr(e, "lockStructure", "1");
    }
    if p.lock_windows {
        doc.set_attr(e, "lockWindows", "1");
    }
    if p.lock_revision {
        doc.set_attr(e, "lockRevision", "1");
    }
    // Schema sequence: after fileVersion, fileSharing, workbookPr; before bookViews, sheets, etc.
    let before = ["bookViews", "sheets", "functionGroups", "externalReferences", "definedNames", "calcPr", "oleSize", "customWorkbookViews", "pivotCaches", "smartTagPr", "smartTagTypes", "webPublishing", "fileRecoveryPr", "webPublishObjects", "extLst"];
    let anchor = doc.elements(root).find(|&el| before.contains(&doc.local_name(el)));
    match anchor {
        Some(a) => doc.insert_before(a, e),
        None => {
            let n = doc.children(root).len();
            doc.insert(root, n, e);
        }
    }
    Ok(())
}

/// ユーザー設定のブック ビュー: `customWorkbookViews`.
fn write_custom_workbook_views(package: &mut Package, workbook: &str, views: &[ibis_doc_model::CustomWorkbookView]) -> Result<(), Error> {
    let doc = package.xml_mut(workbook)?;
    let root = doc.root_element().ok_or_else(|| Error::new("ブックの XML が空です"))?;
    if let Some(old) = doc.child(root, "customWorkbookViews") {
        doc.detach(old);
    }
    if views.is_empty() {
        return Ok(());
    }
    let prefix = prefix_of(doc, root);
    let list = detached(doc, &format!("{prefix}customWorkbookViews"));
    let before = ["pivotCaches", "smartTagPr", "smartTagTypes", "webPublishing", "fileRecoveryPr", "webPublishObjects", "extLst"];
    let anchor = doc.elements(root).find(|&el| before.contains(&doc.local_name(el)));
    match anchor {
        Some(a) => doc.insert_before(a, list),
        None => {
            let n = doc.children(root).len();
            doc.insert(root, n, list);
        }
    }
    let (xw, yw, ww, wh, tr) = if let Some(wv) = doc.child(root, "bookViews").and_then(|bv| doc.child(bv, "workbookView")) {
        (
            doc.attr(wv, "xWindow").unwrap_or("5040").to_string(),
            doc.attr(wv, "yWindow").unwrap_or("1800").to_string(),
            doc.attr(wv, "windowWidth").unwrap_or("28300").to_string(),
            doc.attr(wv, "windowHeight").unwrap_or("17360").to_string(),
            doc.attr(wv, "tabRatio").unwrap_or("600").to_string(),
        )
    } else {
        ("5040".to_string(), "1800".to_string(), "28300".to_string(), "17360".to_string(), "600".to_string())
    };
    for v in views {
        let node = doc.append_element(list, &format!("{prefix}customWorkbookView"));
        doc.set_attr(node, "name", &v.name);
        doc.set_attr(node, "guid", &v.guid);
        doc.set_attr(node, "xWindow", &xw);
        doc.set_attr(node, "yWindow", &yw);
        doc.set_attr(node, "windowWidth", &ww);
        doc.set_attr(node, "windowHeight", &wh);
        doc.set_attr(node, "tabRatio", &tr);
        if !v.include_print_settings {
            doc.set_attr(node, "includePrintSettings", "0");
        } else {
            doc.set_attr(node, "includePrintSettings", "1");
        }
        if !v.include_hidden {
            doc.set_attr(node, "includeHiddenRowCol", "0");
        } else {
            doc.set_attr(node, "includeHiddenRowCol", "1");
        }
        doc.set_attr(node, "activeSheetId", &v.active_sheet_index.to_string());
    }
    Ok(())
}

/// ウィンドウの非表示・再表示: `<bookViews><workbookView visibility="hidden"/>`.
/// A relationship's target as a part name, from the folder of its source (`xl/` and `../customXml/item1.xml`).
fn resolve_target(dir: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_string();
    }
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            other => parts.push(other),
        }
    }
    parts.join("/")
}

const QUERIES_NS: &str = "https://ibis.ne.jp/ibisoffice/queries";
const REL_CUSTOM_XML: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml";
const REL_CUSTOM_XML_PROPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps";

/// The part (`customXml/itemN.xml`) of the クエリ エディター's queries, if the workbook has one: a Custom XML
/// Data part (ECMA-376 Part 1 §15.2.5) whose root is `queries` in our namespace.
fn queries_part(package: &Package, workbook: &str) -> Option<String> {
    let dir = workbook.rsplit_once('/').map_or(String::new(), |(d, _)| format!("{d}/"));
    package.relationships(workbook).ok()?.into_iter().filter(|r| !r.external && r.kind == REL_CUSTOM_XML).find_map(|r| {
        let part = resolve_target(&dir, &r.target);
        let text = String::from_utf8(package.read(&part).ok()?).ok()?;
        text.contains(QUERIES_NS).then_some(part)
    })
}

/// The queries of the クエリ エディター in a Custom XML Data part (`customXml/itemN.xml` with its
/// `itemPropsN.xml`), which Excel keeps as it keeps any other custom XML; made when there is none.
fn write_queries(package: &mut Package, workbook: &str, (queries, parameters): &(Vec<ibis_doc_model::query::Query>, Vec<ibis_doc_model::query::Parameter>)) -> Result<(), Error> {
    let text = ibis_doc_model::query::encode_queries(queries, parameters);
    let escaped = text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<queries xmlns=\"{QUERIES_NS}\" version=\"1\">{escaped}</queries>");
    if let Some(part) = queries_part(package, workbook) {
        package.set_bytes(&part, xml.into_bytes());
        return Ok(());
    }
    if queries.is_empty() && parameters.is_empty() {
        return Ok(());
    }
    let n = (1..).find(|n| !package.contains(&format!("customXml/item{n}.xml"))).unwrap_or(1);
    let (part, props) = (format!("customXml/item{n}.xml"), format!("customXml/itemProps{n}.xml"));
    package.set_bytes(&part, xml.into_bytes());
    let id = format!("{{{:08X}-{:04X}-4{:03X}-8{:03X}-{:012X}}}", 0x1B15_0000u32 + n as u32, 0x0A5E, 0x123, 0xABC, 0x1B15_0000_0000u64 + n as u64);
    let item_props = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<ds:datastoreItem ds:itemID=\"{id}\" xmlns:ds=\"http://schemas.openxmlformats.org/officeDocument/2006/customXml\"><ds:schemaRefs><ds:schemaRef ds:uri=\"{QUERIES_NS}\"/></ds:schemaRefs></ds:datastoreItem>"
    );
    package.set_bytes(&props, item_props.into_bytes());
    package.set_override(&props, "application/vnd.openxmlformats-officedocument.customXmlProperties+xml")?;
    package.add_relationship(&part, REL_CUSTOM_XML_PROPS, &format!("itemProps{n}.xml"), false)?;
    package.add_relationship(workbook, REL_CUSTOM_XML, &format!("../customXml/item{n}.xml"), false)?;
    Ok(())
}

fn write_workbook_window_hidden(package: &mut Package, workbook: &str, hidden: bool) -> Result<(), Error> {
    let doc = package.xml_mut(workbook)?;
    let root = doc.root_element().ok_or_else(|| Error::new("ブックの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    let views = match doc.child(root, "bookViews") {
        Some(v) => v,
        None => {
            let v = detached(doc, &format!("{prefix}bookViews"));
            let before = ["sheets", "functionGroups", "externalReferences", "definedNames", "calcPr", "oleSize", "customWorkbookViews", "pivotCaches", "smartTagPr", "smartTagTypes", "webPublishing", "fileRecoveryPr", "webPublishObjects", "extLst"];
            let anchor = doc.elements(root).find(|&el| before.contains(&doc.local_name(el)));
            match anchor {
                Some(a) => doc.insert_before(a, v),
                None => {
                    let n = doc.children(root).len();
                    doc.insert(root, n, v);
                }
            }
            v
        }
    };
    let view = match doc.child(views, "workbookView") {
        Some(w) => w,
        None => doc.append_element(views, &format!("{prefix}workbookView")),
    };
    if hidden {
        doc.set_attr(view, "visibility", "hidden");
    } else {
        doc.remove_attr(view, "visibility");
    }
    Ok(())
}

struct Writer<'a> {
    package: &'a mut Package,
    map: &'a SourceMap,
    baseline: &'a Presentation,
    styles: Option<StylesWriter>,
    strings: Option<StringsWriter>,
    calc: CalcChanges,
    /// The defined names that stand for more than one cell (lower case),
    /// for telling a dynamic array formula from a plain one.
    multi_names: HashSet<String>,
    /// The external workbooks' names as the saved file numbers them (the links it had, then the new ones).
    books: Vec<String>,
}

/// Formula cells to add to and remove from the calculation chain, by sheetId.
#[derive(Default)]
struct CalcChanges {
    added: Vec<(u32, String)>,
    removed: Vec<(u32, String)>,
}

/// A changed cell: row, column, before and after.
type Change<'s> = (u32, u32, Option<&'s SheetCell>, Option<&'s SheetCell>);

fn changed_cells<'s>(old: &'s Sheet, new: &'s Sheet) -> Vec<Change<'s>> {
    let mut changes = Vec::new();
    let (mut i, mut j) = (0, 0);
    loop {
        let (a, b) = (old.cells.get(i), new.cells.get(j));
        let (ka, kb) = (a.map(|c| (c.row, c.column)), b.map(|c| (c.row, c.column)));
        match (ka, kb) {
            (None, None) => break,
            (Some(x), Some(y)) if x == y => {
                let (a, b) = (a.unwrap(), b.unwrap());
                let same = old.styles[a.style as usize] == new.styles[b.style as usize] && a.value == b.value && a.formula == b.formula && a.runs == b.runs && a.phonetic == b.phonetic;
                if !same {
                    changes.push((x.0, x.1, Some(a), Some(b)));
                }
                i += 1;
                j += 1;
            }
            (Some(x), Some(y)) if x < y => {
                changes.push((x.0, x.1, a, None));
                i += 1;
            }
            (Some(x), None) => {
                changes.push((x.0, x.1, a, None));
                i += 1;
            }
            (_, Some(y)) => {
                changes.push((y.0, y.1, None, b));
                j += 1;
            }
        }
    }
    changes
}

impl Writer<'_> {
    fn write_sheet(&mut self, part: &str, sheet_id: u32, sheet_name: &str, old: &Sheet, new: &Sheet) -> Result<(), Error> {
        // Cells, formats, sizes (hidden rows and columns too) and merged ranges can be saved so far.
        let bare = |s: &Sheet| Sheet {
            cells: Default::default(),
            styles: Vec::new(),
            columns: Vec::new(),
            rows: Vec::new(),
            // What the app measured when it opened the sheet, not saved (`Sheet::measured_rows`).
            default_row: 0,
            measured_rows: Default::default(),
            fitted_rows: Default::default(),
            printed_rows: Default::default(),
            column_fonts: Default::default(),
            row_fonts: Default::default(),
            written_fonts: Default::default(),
            merges: Vec::new(),
            hidden_rows: Default::default(),
            custom_rows: Default::default(),
            custom_columns: Default::default(),
            hidden_columns: Default::default(),
            spills: Default::default(),
            frozen: None,
            split: None,
            protection: None,
            protected_ranges: Vec::new(),
            validations: Vec::new(),
            hyperlinks: Vec::new(),
            comments: Vec::new(),
            outline_rows: Default::default(),
            outline_columns: Default::default(),
            print: Default::default(),
            print_area: None,
            grid_lines: true,
            headings: true,
            zoom: None,
            page_break_preview: false,
            page_layout: false,
            show_ruler: true,
            tab_color: None,
            auto_filter: None,
            filtered_rows: Default::default(),
            conditional: Vec::new(),
            tables: Vec::new(),
            pivots: Vec::new(),
            data_tables: Vec::new(),
            data_consolidate: None,
            scenarios: None,
            sparklines: Vec::new(),
            threads: Vec::new(),
            custom_views: Vec::new(),
            show_phonetic: false,
            phonetic_pr: None,
            ignored_errors: Vec::new(),
            cell_images: Vec::new(),
            web_images: Default::default(),
            show_formulas: false,
            show_zeros: true,
            hide_ink: false,
            ..s.clone()
        };
        if bare(old) != bare(new) {
            return Err(unsupported("表示・印刷などシートの設定の変更"));
        }
        if old.outline_rows != new.outline_rows || old.outline_columns != new.outline_columns {
            let digit = self.map.digit;
            let doc = self.package.xml_mut(part)?;
            sizes::write_outline(doc, old, new, digit)?;
        }
        if old.print != new.print {
            let doc = self.package.xml_mut(part)?;
            sizes::write_print(doc, &new.print)?;
        }
        if old.ignored_errors != new.ignored_errors {
            let doc = self.package.xml_mut(part)?;
            sizes::write_ignored_errors(doc, &new.ignored_errors)?;
        }
        if old.protection != new.protection {
            let doc = self.package.xml_mut(part)?;
            sizes::write_protection(doc, new.protection.as_ref())?;
        }
        if old.protected_ranges != new.protected_ranges {
            let doc = self.package.xml_mut(part)?;
            sizes::write_protected_ranges(doc, &new.protected_ranges)?;
        }
        if old.hyperlinks != new.hyperlinks {
            write_hyperlinks(self.package, part, &new.hyperlinks)?;
        }
        if old.comments != new.comments {
            write_comments(self.package, part, &new.comments)?;
        }
        if old.threads != new.threads {
            write_threaded_comments(self.package, &self.map.workbook, part, &new.threads)?;
        }
        if old.tables != new.tables || !table_calculated_formulas_eq(old, new) {
            write_tables(self.package, self.map.styles.as_deref(), part, old, new)?;
        }
        if old.pivots != new.pivots {
            let workbook = self.map.workbook.clone();
            pivots::write_pivots(self.package, part, &workbook, &old.pivots, &new.pivots)?;
        }
        if old.validations != new.validations {
            let doc = self.package.xml_mut(part)?;
            sizes::write_validations(doc, &new.validations)?;
        }
        if old.conditional != new.conditional || !sizes::conditional_sources_eq(&old.conditional,&new.conditional) {
            let styles = self.map.styles.clone().ok_or_else(|| unsupported("書式のないブックの条件付き書式"))?;
            let ids = write_dxfs(self.package, &styles, part, &old.conditional, &new.conditional)?;
            let doc = self.package.xml_mut(part)?;
            sizes::write_conditional(doc, &old.conditional, &new.conditional, &ids)?;
        }
        if old.auto_filter != new.auto_filter {
            let colors = match new.auto_filter.as_ref().filter(|f| f.columns.iter().any(|(_, x)| matches!(x, ibis_doc_model::ColumnFilter::Color { .. }))) {
                Some(f) => {
                    let styles = self.map.styles.clone().ok_or_else(|| unsupported("書式のないブックの色フィルター"))?;
                    write_filter_dxfs(self.package, &styles, part, old.auto_filter.as_ref(), f)?
                }
                None => HashMap::new(),
            };
            let doc = self.package.xml_mut(part)?;
            sizes::write_auto_filter(doc, new.auto_filter.as_ref(), &colors)?;
        }
        if old.data_consolidate != new.data_consolidate {
            let doc = self.package.xml_mut(part)?;
            sizes::write_data_consolidate(doc, new.data_consolidate.as_ref())?;
        }
        if old.scenarios != new.scenarios {
            let doc = self.package.xml_mut(part)?;
            sizes::write_scenarios(doc, new.scenarios.as_ref())?;
        }
        if old.sparklines != new.sparklines {
            let doc = self.package.xml_mut(part)?;
            sizes::write_sparklines(doc, sheet_name, &new.sparklines)?;
        }
        if old.tab_color != new.tab_color {
            let doc = self.package.xml_mut(part)?;
            sizes::write_tab_color(doc, new.tab_color)?;
        }
        if old.grid_lines != new.grid_lines {
            let doc = self.package.xml_mut(part)?;
            sizes::write_grid_lines(doc, new.grid_lines)?;
        }
        if old.headings != new.headings {
            let doc = self.package.xml_mut(part)?;
            sizes::write_headings(doc, new.headings)?;
        }
        if old.show_ruler != new.show_ruler {
            let doc = self.package.xml_mut(part)?;
            sizes::write_show_ruler(doc, new.show_ruler)?;
        }
        if old.show_formulas != new.show_formulas {
            let doc = self.package.xml_mut(part)?;
            sizes::write_show_formulas(doc, new.show_formulas)?;
        }
        if old.show_zeros != new.show_zeros {
            let doc = self.package.xml_mut(part)?;
            sizes::write_show_zeros(doc, new.show_zeros)?;
        }
        if old.zoom != new.zoom {
            let doc = self.package.xml_mut(part)?;
            sizes::write_zoom(doc, new.zoom, if new.page_break_preview { Some("zoomScaleSheetLayoutView") } else if new.page_layout { Some("zoomScalePageLayoutView") } else { None })?;
        }
        if old.page_break_preview != new.page_break_preview || old.page_layout != new.page_layout {
            let doc = self.package.xml_mut(part)?;
            let mode = if new.page_break_preview { Some("pageBreakPreview") } else if new.page_layout { Some("pageLayout") } else { None };
            sizes::write_page_break_preview(doc, mode)?;
        }
        if old.frozen != new.frozen || old.split != new.split {
            let doc = self.package.xml_mut(part)?;
            sizes::write_panes(doc, new.frozen, new.split)?;
        }
        if old.custom_views != new.custom_views {
            let doc = self.package.xml_mut(part)?;
            sizes::write_custom_sheet_views(doc, &new.custom_views)?;
        }
        if old.show_phonetic != new.show_phonetic || old.phonetic_pr != new.phonetic_pr {
            let doc = self.package.xml_mut(part)?;
            sizes::write_phonetic(doc, new.show_phonetic, new.phonetic_pr.as_ref())?;
        } else {
            let doc = self.package.xml_mut(part)?;
            if let Some(root) = doc.root_element() && let Some(p) = doc.child(root, "phoneticPr") {
                if doc.attr(p, "fontId").is_none() {
                    doc.set_attr(p, "fontId", "0");
                }
            }
        }
        if old.columns != new.columns || old.rows != new.rows || old.merges != new.merges {
            let digit = self.map.digit;
            let doc = self.package.xml_mut(part)?;
            sizes::write_sizes(doc, old, new, digit)?;
            sizes::write_merges(doc, &new.merges);
        }
        let mut changes = changed_cells(old, new);
        // A table taken off (範囲に変換) leaves its look in its cells: they are
        // written with it as their own formats.
        let gone: Vec<CellRange> = old.tables.iter().filter(|o| !new.tables.iter().any(|n| n.name.eq_ignore_ascii_case(&o.name))).map(|t| t.range).collect();
        let mut table_looks: HashSet<u32> = HashSet::new();
        for r in &gone {
            for b in new.cells.iter().filter(|c| (r.first_row..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column)) {
                table_looks.insert(b.style);
                if !changes.iter().any(|c| (c.0, c.1) == (b.row, b.column)) {
                    changes.push((b.row, b.column, old.cell(b.row, b.column), Some(b)));
                }
            }
        }
        changes.sort_by_key(|c| (c.0, c.1));
        // Dynamic arrays whose area changed (their cells may not have).
        let respilled: Vec<(u32, u32)> = new.spills.iter().filter(|(a, r)| old.spills.get(a) != Some(r)).map(|(a, _)| *a).filter(|a| !changes.iter().any(|c| (c.0, c.1) == *a)).collect();
        if changes.is_empty() && respilled.is_empty() && old.data_tables == new.data_tables {
            return Ok(());
        }
        // Single-cell dynamic results must keep their calculation mode: a
        // later edit (or compatibility change) can make them grow again.
        let spilling = !new.spills.is_empty();
        let mut cm = if spilling { Some(self.dynamic_array_metadata(new.spills.keys().any(|anchor| !old.spills.contains_key(anchor)))?) } else { None };
        // A value can still carry unrelated cell metadata. When it becomes
        // a dynamic formula, combine only that cell's records with XLDAPR.
        let mut activated_metadata = HashMap::new();
        if let Some(indices) = &mut cm {
            for &(row, column, _, after) in &changes {
                if !after.is_some_and(|c| c.formula.is_some()) || !new.spills.contains_key(&(row,column)) { continue; }
                let doc = self.package.xml(part)?;
                let existing = doc.root_element().and_then(|root| doc.descendants(root).into_iter().find(|&c|
                    doc.local_name(c)=="c" && doc.namespace(c)==Some(MAIN_NS) && doc.attr(c,"r")==Some(cell_name(column,row).as_str())))
                    .and_then(|c| doc.attr(c,"cm").map(str::to_string));
                if let Some(index) = existing
                    && !indices.iter().any(|v| crate::xlsx::metadata_index(v)==crate::xlsx::metadata_index(&index)) {
                    let merged = self.metadata_with_dynamic_record(&index, &indices[0])?;
                    indices.push(merged.clone()); activated_metadata.insert((row,column),merged);
                }
            }
        }
        // Replacing a formula with a value removes its dynamic record, but
        // must retain unrelated records attached to the same cell.
        let mut literal_metadata = HashMap::new();
        for &(row, column, before, after) in &changes {
            if before.is_some_and(|c| c.formula.is_some()) && after.is_some_and(|c| c.formula.is_none()) {
                let doc = self.package.xml(part)?;
                let existing = doc.root_element().and_then(|root| doc.descendants(root).into_iter().find(|&c|
                    doc.local_name(c)=="c" && doc.namespace(c)==Some(MAIN_NS) && doc.attr(c,"r")==Some(cell_name(column,row).as_str())))
                    .and_then(|c| doc.attr(c,"cm").map(str::to_string));
                if let Some(index) = existing { literal_metadata.insert((row,column), self.metadata_without_dynamic_record(&index)?); }
            }
        }
        // The formats the changed cells need, as cellXfs entries.
        let mut xfs: HashMap<u32, u32> = HashMap::new();
        for (_, _, _, b) in &changes {
            if let Some(b) = b
                && !xfs.contains_key(&b.style)
            {
                let style = &new.styles[b.style as usize];
                let xf = if table_looks.contains(&b.style) { self.xf_for(&CellStyle { xf: None, ..style.clone() })? } else { self.xf_for(style)? };
                xfs.insert(b.style, xf);
            }
        }
        // Text that goes into the shared strings.
        let mut string_ids: HashMap<(u32, u32), u32> = HashMap::new();
        for (r, c, _, b) in &changes {
            if let Some(b) = b
                && b.formula.is_none()
                && let CellValue::Text(t) = &b.value
            {
                let id = match (&b.runs, &b.phonetic) {
                    (Some(runs), _) if runs.iter().map(|r| r.text.as_str()).collect::<String>() == *t => self.strings()?.index_rich(runs),
                    (_, Some(ph)) => self.strings()?.index_phonetic(t, ph),
                    _ => self.strings()?.index(t),
                };
                string_ids.insert((*r, *c), id);
            }
        }
        let mut string_delta: i64 = 0;
        let doc = self.package.xml_mut(part)?;
        let mut sheet = SheetXml::new(doc)?;
        for &(row, column, before, after) in &changes {
            let formula_changed = before.and_then(|c| c.formula.as_ref()) != after.and_then(|c| c.formula.as_ref());
            if formula_changed {
                sheet.unshare_around(row, column, old, &self.books)?;
            }
            match after {
                None => {
                    if let Some(c) = sheet.find_cell(row, column) {
                        if sheet.doc.attr(c, "t") == Some("s") {
                            string_delta -= 1;
                        }
                        sheet.remove_cell(row, column, c);
                    }
                }
                Some(cell) => {
                    let c = sheet.cell(row, column);
                    if sheet.doc.attr(c, "t") == Some("s") {
                        string_delta -= 1;
                    }
                    match xfs[&cell.style] {
                        0 => sheet.doc.remove_attr(c, "s"),
                        xf => sheet.doc.set_attr(c, "s", &xf.to_string()),
                    }
                    let spill_changed = old.spills.get(&(row, column)) != new.spills.get(&(row, column));
                    if formula_changed || spill_changed {
                        if let Some(formula) = &cell.formula {
                            // Keep the existing formula's unmodeled attributes and
                            // the cell's recognized composite metadata reference.
                            let f = sheet.formula(c).unwrap_or_else(|| sheet.insert_child(c, "f", 0));
                            for attr in ["t", "ref", "si"] { sheet.doc.remove_attr(f, attr); }
                            sheet.doc.set_text(f, &file_formula_in(&self.books, formula, table_at(new, row, column))?);
                            if !new.spills.contains_key(&(row, column)) { sheet.doc.remove_attr(c, "cm"); }
                            if let Some(index) = activated_metadata.get(&(row,column)) { sheet.doc.set_attr(c,"cm",index); }
                            write_dynamic(&mut sheet, c, f, new, row, column, cm.as_deref(), &self.multi_names);
                        } else {
                            if let Some(f) = sheet.formula(c) { sheet.doc.detach(f); }
                            if let Some(Some(index)) = literal_metadata.get(&(row,column)) { sheet.doc.set_attr(c,"cm",index); }
                            else { sheet.doc.remove_attr(c, "cm"); }
                        }
                    }
                    sheet.write_value(c, cell, string_ids.get(&(row, column)).copied());
                    if sheet.doc.attr(c, "t") == Some("s") {
                        string_delta += 1;
                    }
                }
            }
            let had = before.is_some_and(|c| c.formula.is_some());
            let has = after.is_some_and(|c| c.formula.is_some());
            if had && !has {
                self.calc.removed.push((sheet_id, cell_name(column, row)));
            } else if has && !had {
                self.calc.added.push((sheet_id, cell_name(column, row)));
            }
        }
        for (row, column) in respilled {
            let Some(c) = sheet.find_cell(row, column) else { continue };
            let Some(f) = sheet.formula(c) else { continue };
            write_dynamic(&mut sheet, c, f, new, row, column, cm.as_deref(), &self.multi_names);
        }
        // Data tables made or taken out: the formula of their first cell.
        for t in old.data_tables.iter().filter(|t| !new.data_tables.contains(t)) {
            if let Some(c) = sheet.find_cell(t.range.first_row, t.range.first_column)
                && let Some(f) = sheet.formula(c)
                && sheet.doc.attr(f, "t") == Some("dataTable")
            {
                sheet.doc.detach(f);
            }
        }
        for t in new.data_tables.iter().filter(|t| !old.data_tables.contains(t)) {
            let c = sheet.cell(t.range.first_row, t.range.first_column);
            let f = sheet.formula(c).unwrap_or_else(|| sheet.insert_child(c, "f", 0));
            for attr in ["t", "ref", "dt2D", "dtr", "r1", "r2", "si"] {
                sheet.doc.remove_attr(f, attr);
            }
            sheet.doc.set_text(f, "");
            sheet.doc.set_attr(f, "t", "dataTable");
            sheet.doc.set_attr(f, "ref", &format!("{}:{}", cell_name(t.range.first_column, t.range.first_row), cell_name(t.range.last_column, t.range.last_row)));
            let at = |(row, column): (u32, u32)| cell_name(column, row);
            match (t.row_input, t.column_input) {
                (Some(r), Some(c)) => {
                    sheet.doc.set_attr(f, "dt2D", "1");
                    sheet.doc.set_attr(f, "dtr", "1");
                    sheet.doc.set_attr(f, "r1", &at(r));
                    sheet.doc.set_attr(f, "r2", &at(c));
                }
                (Some(r), None) => {
                    sheet.doc.set_attr(f, "dtr", "1");
                    sheet.doc.set_attr(f, "r1", &at(r));
                }
                (None, Some(c)) => sheet.doc.set_attr(f, "r1", &at(c)),
                (None, None) => {}
            }
        }
        let written: Vec<(u32, u32)> = changes.iter().filter(|c| c.3.is_some()).map(|c| (c.0, c.1)).collect();
        sheet.grow_dimension(&written);
        if string_delta != 0 {
            self.strings()?.count_delta += string_delta;
        }
        Ok(())
    }

    /// Reusable `cm` indices containing dynamic array metadata (XLDAPR),
    /// with the preferred default for newly typed cells first. The
    /// metadata part is made (with Excel's standard content) if the workbook
    /// has none.
    /// Excel does not open a file whose cell metadata block has the dynamic
    /// array record (XLDAPR) after another record: puts it first in each
    /// block that has it elsewhere (a block already right is left alone).
    fn put_dynamic_records_first(&mut self, part: &str) -> Result<(), Error> {
        let doc = self.package.xml(part)?;
        let Some(root) = doc.root_element() else { return Ok(()) };
        let lists: Vec<_> = doc.children_named(root, "cellMetadata").filter(|&n| doc.namespace(n) == Some(MAIN_NS)).collect();
        let [cells] = lists.as_slice() else { return Ok(()) };
        let text = String::from_utf8(doc.to_bytes()).map_err(|e| Error::new(format!("metadata XML: {e}")))?;
        let parsed = roxmltree::Document::parse(&text).map_err(|e| Error::new(format!("metadata XML: {e}")))?;
        let dynamic = crate::xlsx::dynamic_metadata_records(parsed.root_element());
        let mut moves = Vec::new();
        for bk in doc.children_named(*cells, "bk").filter(|&n| doc.namespace(n) == Some(MAIN_NS)) {
            let elements: Vec<_> = doc.elements(bk).collect();
            let is_dynamic = |n: ibis_ooxml::xml::NodeId| doc.local_name(n) == "rc" && doc.namespace(n) == Some(MAIN_NS)
                && doc.attr(n, "t").and_then(crate::xlsx::metadata_index).zip(doc.attr(n, "v").and_then(crate::xlsx::metadata_index)).is_some_and(|r| dynamic.contains(&r));
            if let Some(i) = elements.iter().position(|&n| is_dynamic(n)) && i > 0 {
                moves.push((bk, elements[i]));
            }
        }
        if moves.is_empty() { return Ok(()); }
        let doc = self.package.xml_mut(part)?;
        for (bk, rc) in moves {
            doc.detach(rc);
            doc.insert(bk, 0, rc);
        }
        Ok(())
    }

    fn dynamic_array_metadata(&mut self, needs_default: bool) -> Result<Vec<String>, Error> {
        const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata";
        let workbook = self.map.workbook.clone();
        let dir = workbook.rsplit_once('/').map_or("", |(d, _)| d).to_string();
        let existing = self.package.relationships(&workbook)?.into_iter().find(|r| r.kind == REL).map(|r| ibis_ooxml::package::resolve(&dir, &r.target));
        if let Some(part) = existing {
            self.put_dynamic_records_first(&part)?;
            let doc = self.package.xml(&part)?;
            let root = doc.root_element().ok_or_else(|| Error::new("metadata.xml が空です"))?;
            if doc.local_name(root) != "metadata" || doc.namespace(root) != Some(MAIN_NS) {
                return Err(unsupported("標準namespaceではない動的配列metadataの編集"));
            }
            let standard_lists = |name: &str| doc.children_named(root, name)
                .filter(|&n| doc.namespace(n) == Some(MAIN_NS)).collect::<Vec<_>>();
            let types = standard_lists("metadataTypes");
            let cells = standard_lists("cellMetadata");
            let ([types], [cells]) = (types.as_slice(), cells.as_slice()) else {
                return Err(unsupported("欠落または重複した動的配列metadata listの編集"));
            };
            let (types, cells) = (*types, *cells);
            let types: Vec<_> = doc.children_named(types, "metadataType")
                .filter(|&n| doc.namespace(n) == Some(MAIN_NS)).map(|n| doc.attr(n, "name")).collect();
            if types.iter().any(|name| name.is_none()) { return Err(unsupported("nameのない動的配列metadata typeの編集")); }
            let Some(t) = types.iter().position(|name| *name == Some("XLDAPR")).map(|i| i + 1) else { return Err(unsupported("動的配列の情報のないブックへのスピルの保存")) };
            let t = t.to_string();
            let text = String::from_utf8(doc.to_bytes()).map_err(|e| Error::new(format!("metadata XML: {e}")))?;
            let parsed = roxmltree::Document::parse(&text).map_err(|e| Error::new(format!("metadata XML: {e}")))?;
            let dynamic_records = crate::xlsx::dynamic_metadata_records(parsed.root_element());
            let record_is_dynamic = |rc| doc.attr(rc,"t").and_then(crate::xlsx::metadata_index)
                .zip(doc.attr(rc,"v").and_then(crate::xlsx::metadata_index)).is_some_and(|record| dynamic_records.contains(&record));
            let standard_blocks: Vec<_> = doc.children_named(cells,"bk").filter(|&bk| doc.namespace(bk)==Some(MAIN_NS)).collect();
            if standard_blocks.is_empty() || standard_blocks.iter().any(|&bk| !doc.children_named(bk,"rc").any(|rc| doc.namespace(rc)==Some(MAIN_NS))) {
                return Err(unsupported("標準recordのない動的配列metadata blockの編集"));
            }
            let blocks: Vec<(String, bool)> = doc.children_named(cells, "bk")
                .filter(|&bk| doc.namespace(bk) == Some(MAIN_NS)).enumerate().filter_map(|(i, bk)| {
                    let records: Vec<_> = doc.children_named(bk, "rc").filter(|&rc| doc.namespace(rc) == Some(MAIN_NS)).collect();
                    records.iter().any(|&rc| record_is_dynamic(rc))
                        .then(|| ((i + 1).to_string(), records.len() == 1 && doc.elements(bk).count() == 1))
                }).collect();
            if blocks.is_empty() && !needs_default { return Err(unsupported("動的配列の情報のないブックへのスピルの保存")); }
            // Prefer a plain dynamic record for newly typed cells, while
            // retaining every existing composite record as a reusable index.
            let preferred = blocks.iter().position(|(_, plain)| *plain);
            let mut indices: Vec<String> = blocks.iter().map(|(index, _)| index.clone()).collect();
            if let Some(preferred) = preferred { indices.swap(0, preferred); }
            else if needs_default {
                // A new cell must not inherit unrelated metadata from a
                // composite block. Append only its existing XLDAPR record.
                let count = doc.children_named(cells, "bk").filter(|&bk| doc.namespace(bk) == Some(MAIN_NS)).count();
                self.ensure_cell_metadata_index_unused(count + 1)?;
                let template = doc.children_named(cells, "bk").filter(|&bk| doc.namespace(bk) == Some(MAIN_NS))
                    .flat_map(|bk| doc.children_named(bk, "rc")).find(|&rc| doc.namespace(rc) == Some(MAIN_NS) && record_is_dynamic(rc));
                let futures: Vec<_> = doc.children_named(root,"futureMetadata").filter(|&n| doc.namespace(n)==Some(MAIN_NS) && doc.attr(n,"name")==Some("XLDAPR")).collect();
                let future = if template.is_none() {
                    let [future] = futures.as_slice() else { return Err(unsupported("一意でないXLDAPR future recordの編集")); };
                    let next = doc.children_named(*future,"bk").filter(|&bk| doc.namespace(bk)==Some(MAIN_NS)).count();
                    // Appending a block must not resolve an existing out-of-range
                    // reference and silently promote an old formula's mode.
                    if doc.descendants(root).into_iter().any(|rc| doc.local_name(rc)=="rc" && doc.namespace(rc)==Some(MAIN_NS)
                        && doc.attr(rc,"t").and_then(crate::xlsx::metadata_index)==crate::xlsx::metadata_index(&t)
                        && doc.attr(rc,"v").and_then(crate::xlsx::metadata_index).map(|v| v as usize)==Some(next)) {
                        return Err(unsupported("既存の範囲外future record参照と衝突する動的配列metadataの追加"));
                    }
                    Some((*future, next))
                } else { None };
                let prefix = prefix_of(&doc, root);
                let doc = self.package.xml_mut(&part)?;
                let record = if let Some(template) = template { doc.deep_clone(template) } else {
                    let (future, v) = future.unwrap();
                    let bk = doc.append_element(future,&format!("{prefix}bk"));
                    let list = doc.append_element(bk,&format!("{prefix}extLst"));
                    let ext = doc.append_element(list,&format!("{prefix}ext"));
                    doc.set_attr(ext,"uri","{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}");
                    let prop = doc.append_element(ext,"xda:dynamicArrayProperties");
                    doc.set_attr(prop,"xmlns:xda","http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray");
                    doc.set_attr(prop,"fDynamic","1");doc.set_attr(prop,"fCollapsed","0");
                    doc.set_attr(future,"count",&(v+1).to_string());
                    let rc = detached(doc,&format!("{prefix}rc"));doc.set_attr(rc,"t",&t);doc.set_attr(rc,"v",&v.to_string());rc
                };
                let bk = doc.append_element(cells, &format!("{prefix}bk"));
                doc.insert(bk, 0, record);
                doc.set_attr(cells, "count", &(count + 1).to_string());
                indices.insert(0, (count + 1).to_string());
            }
            return Ok(indices);
        }
        self.ensure_cell_metadata_index_unused(1)?;
        let part = format!("{}metadata.xml", if dir.is_empty() { String::new() } else { format!("{dir}/") });
        let xml = concat!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
            r#"<metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:xda="http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray">"#,
            r#"<metadataTypes count="1"><metadataType name="XLDAPR" minSupportedVersion="120000" copy="1" pasteAll="1" pasteValues="1" merge="1" splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" coerce="1" cellMeta="1"/></metadataTypes>"#,
            r#"<futureMetadata name="XLDAPR" count="1"><bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/></ext></extLst></bk></futureMetadata>"#,
            r#"<cellMetadata count="1"><bk><rc t="1" v="0"/></bk></cellMetadata></metadata>"#
        );
        let doc = ibis_ooxml::XmlDocument::parse(xml.as_bytes()).map_err(|e| Error::new(format!("metadata.xml: {e}")))?;
        self.package.set_xml(&part, doc);
        self.package.set_override(&part, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml")?;
        let target = part.strip_prefix(&format!("{dir}/")).unwrap_or(&part).to_string();
        self.package.add_relationship(&workbook, REL, &target, false)?;
        Ok(vec!["1".into()])
    }

    fn metadata_with_dynamic_record(&mut self, index: &str, default: &str) -> Result<String, Error> {
        const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata";
        let relations: Vec<_> = self.package.relationships(&self.map.workbook)?.into_iter().filter(|r| r.kind==REL && !r.external).collect();
        let [relation] = relations.as_slice() else { return Err(unsupported("一意でないmetadataへ動的recordを追加する編集")); };
        let dir=self.map.workbook.rsplit_once('/').map_or("",|(d,_)|d); let part=ibis_ooxml::package::resolve(dir,&relation.target);
        let doc=self.package.xml(&part)?;let root=doc.root_element().ok_or_else(||unsupported("空metadataへの動的record追加"))?;
        let lists:Vec<_>=doc.children_named(root,"cellMetadata").filter(|&n|doc.namespace(n)==Some(MAIN_NS)).collect();
        let [cells]=lists.as_slice() else {return Err(unsupported("一意でないcellMetadataへの動的record追加"));};let cells=*cells;
        let blocks:Vec<_>=doc.children_named(cells,"bk").filter(|&n|doc.namespace(n)==Some(MAIN_NS)).collect();
        let position=crate::xlsx::metadata_index(index).and_then(|n|n.checked_sub(1)).and_then(|n|blocks.get(n as usize)).copied().ok_or_else(||unsupported("不正cmを持つセルへの動的record追加"))?;
        let template=crate::xlsx::metadata_index(default).and_then(|n|n.checked_sub(1)).and_then(|n|blocks.get(n as usize)).copied().ok_or_else(||unsupported("動的metadata templateがありません"))?;
        let text=String::from_utf8(doc.to_bytes()).map_err(|e|Error::new(format!("metadata XML: {e}")))?;
        let parsed=roxmltree::Document::parse(&text).map_err(|e|Error::new(format!("metadata XML: {e}")))?;
        let dynamic=crate::xlsx::dynamic_metadata_records(parsed.root_element());
        let record=|n|doc.attr(n,"t").and_then(crate::xlsx::metadata_index).zip(doc.attr(n,"v").and_then(crate::xlsx::metadata_index));
        let (t,v)=doc.children_named(template,"rc").filter(|&n|doc.namespace(n)==Some(MAIN_NS)).filter_map(record).find(|r|dynamic.contains(r)).ok_or_else(||unsupported("真の動的recordがありません"))?;
        let old:Vec<_>=doc.elements(position).collect();
        if old.is_empty() || old.iter().any(|&n|doc.local_name(n)!="rc" || doc.namespace(n)!=Some(MAIN_NS) || record(n).is_none_or(|r|r.0==t)) {
            return Err(unsupported("未知子・不正recordまたは既存XLDAPRを持つセルへの動的record追加"));
        }
        let next=blocks.len()+1;self.ensure_cell_metadata_index_unused(next)?;let prefix=prefix_of(&doc,root);
        let doc=self.package.xml_mut(&part)?;let copy=doc.deep_clone(position);let end=doc.children(cells).len();doc.insert(cells,end,copy);
        // The dynamic array record first: Excel does not open a file whose
        // block has it after another record (xsc55_composite_formula_literal).
        let rc=doc.insert_element(copy,0,&format!("{prefix}rc"));doc.set_attr(rc,"t",&t.to_string());doc.set_attr(rc,"v",&v.to_string());doc.set_attr(cells,"count",&next.to_string());
        Ok(next.to_string())
    }

    fn metadata_without_dynamic_record(&mut self, index: &str) -> Result<Option<String>, Error> {
        const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata";
        let relations: Vec<_> = self.package.relationships(&self.map.workbook)?.into_iter().filter(|r| r.kind==REL && !r.external).collect();
        let [relation] = relations.as_slice() else { return Ok(Some(index.to_string())); };
        let dir = self.map.workbook.rsplit_once('/').map_or("", |(d,_)| d);
        let part = ibis_ooxml::package::resolve(dir,&relation.target);
        let doc = self.package.xml(&part)?;
        let Some(root) = doc.root_element() else { return Ok(Some(index.to_string())); };
        let text = String::from_utf8(doc.to_bytes()).map_err(|e| Error::new(format!("metadata XML: {e}")))?;
        let parsed = roxmltree::Document::parse(&text).map_err(|e| Error::new(format!("metadata XML: {e}")))?;
        let dynamic = crate::xlsx::dynamic_metadata_records(parsed.root_element());
        if dynamic.is_empty() { return Ok(Some(index.to_string())); }
        let cells: Vec<_> = doc.children_named(root,"cellMetadata").filter(|&n|doc.namespace(n)==Some(MAIN_NS)).collect();
        let [cells] = cells.as_slice() else { return Ok(Some(index.to_string())); }; let cells=*cells;
        let blocks: Vec<_> = doc.children_named(cells,"bk").filter(|&n|doc.namespace(n)==Some(MAIN_NS)).collect();
        let Some(position) = crate::xlsx::metadata_index(index).and_then(|n|n.checked_sub(1)).map(|n|n as usize) else { return Ok(Some(index.to_string())); };
        let Some(&block) = blocks.get(position) else { return Ok(Some(index.to_string())); };
        let is_dynamic = |n| doc.local_name(n)=="rc" && doc.namespace(n)==Some(MAIN_NS)
            && doc.attr(n,"t").and_then(crate::xlsx::metadata_index).zip(doc.attr(n,"v").and_then(crate::xlsx::metadata_index)).is_some_and(|rc|dynamic.contains(&rc));
        let removed: Vec<_> = doc.elements(block).filter(|&n|is_dynamic(n)).collect();
        if removed.is_empty() { return Ok(Some(index.to_string())); }
        let kept: Vec<_> = doc.elements(block).filter(|n|!removed.contains(n)).collect();
        if kept.is_empty() && doc.attributes(block).is_empty() { return Ok(None); }
        if kept.iter().any(|&n| doc.local_name(n)!="rc" || doc.namespace(n)!=Some(MAIN_NS)
            || doc.attr(n,"t").and_then(crate::xlsx::metadata_index).is_none() || doc.attr(n,"v").and_then(crate::xlsx::metadata_index).is_none()) || kept.is_empty() {
            return Err(unsupported("未知子または不正recordを含むmetadataから動的情報を除く編集"));
        }
        let next = blocks.len()+1;
        self.ensure_cell_metadata_index_unused(next)?;
        let doc = self.package.xml_mut(&part)?;
        let copy = doc.deep_clone(block);
        // Attach before resolving inherited namespaces on the cloned records.
        let end=doc.children(cells).len(); doc.insert(cells,end,copy);
        let records: Vec<_> = doc.elements(copy).collect();
        for n in records {
            if doc.local_name(n)=="rc" && doc.namespace(n)==Some(MAIN_NS)
                && doc.attr(n,"t").and_then(crate::xlsx::metadata_index).zip(doc.attr(n,"v").and_then(crate::xlsx::metadata_index)).is_some_and(|rc|dynamic.contains(&rc)) { doc.detach(n); }
        }
        doc.set_attr(cells,"count",&next.to_string());
        Ok(Some(next.to_string()))
    }

    /// Do not make an existing orphan cm valid, including in worksheets
    /// hidden from the editor, when appending or creating metadata.
    fn ensure_cell_metadata_index_unused(&self, index: usize) -> Result<(), Error> {
        for sheet_part in self.map.sheet_parts.iter().flatten() {
            let sheet = self.package.xml(sheet_part)?;
            if let Some(root) = sheet.root_element()
                && sheet.descendants(root).into_iter().any(|c| sheet.local_name(c)=="c" && sheet.namespace(c)==Some(MAIN_NS)
                    && sheet.attr(c,"cm").and_then(crate::xlsx::metadata_index).map(|v| v as usize)==Some(index)) {
                return Err(unsupported("既存の範囲外cm参照と衝突する動的配列metadataの追加"));
            }
        }
        Ok(())
    }

    fn strings(&mut self) -> Result<&mut StringsWriter, Error> {
        if self.strings.is_none() {
            self.strings = Some(StringsWriter::open(self.package, self.map)?);
        }
        Ok(self.strings.as_mut().unwrap())
    }

    /// The cellXfs entry for a style of the current model.
    fn xf_for(&mut self, style: &CellStyle) -> Result<u32, Error> {
        let styles = self.styles.get_or_insert_with(|| StylesWriter::new(self.baseline));
        styles.xf_for(self.package, self.map.styles.as_deref(), style)
    }

    fn finish(mut self) -> Result<(), Error> {
        if let Some(strings) = self.strings.take() {
            strings.finish(self.package, &self.map.workbook)?;
        }
        if !self.calc.added.is_empty() || !self.calc.removed.is_empty() {
            self.write_calc_chain()?;
        }
        Ok(())
    }

    /// Take removed formulas out of the calculation chain and add new ones
    /// at its end, as Excel does.
    fn write_calc_chain(&mut self) -> Result<(), Error> {
        let Some(part) = self.map.calc_chain.clone() else {
            // No chain: Excel makes one when it next calculates.
            return Ok(());
        };
        let doc = self.package.xml_mut(&part)?;
        let root = doc.root_element().ok_or_else(|| Error::new("calcChain.xml が空です"))?;
        // Each entry's sheet (`i` is left out when it is the previous one's).
        let mut sheet = 0u32;
        let mut keyed: Vec<(NodeId, u32, String)> = Vec::new();
        for e in doc.children_named(root, "c").collect::<Vec<_>>() {
            if let Some(i) = doc.attr(e, "i").and_then(|v| v.parse().ok()) {
                sheet = i;
            }
            keyed.push((e, sheet, doc.attr(e, "r").unwrap_or("").to_string()));
        }
        let removed: HashSet<(u32, &str)> = self.calc.removed.iter().map(|(s, r)| (*s, r.as_str())).collect();
        let gone = |k: usize| removed.contains(&(keyed[k].1, keyed[k].2.as_str()));
        for k in 0..keyed.len() {
            if !gone(k) {
                continue;
            }
            // The next entry inherits this one's sheet: write it there.
            if k + 1 < keyed.len() && !gone(k + 1) && doc.attr(keyed[k + 1].0, "i").is_none() {
                doc.set_attr(keyed[k + 1].0, "i", &keyed[k + 1].1.to_string());
            }
            doc.detach(keyed[k].0);
        }
        let prefix = prefix_of(doc, root);
        for (sheet, r) in &self.calc.added {
            let c = doc.append_element(root, &format!("{prefix}c"));
            doc.set_attr(c, "r", r);
            doc.set_attr(c, "i", &sheet.to_string());
        }
        if doc.children_named(root, "c").next().is_none() {
            // An empty chain is not allowed: the part goes, as Excel does.
            let workbook = self.map.workbook.clone();
            for rel in self.package.relationships(&workbook)? {
                if rel.target_part(&workbook).as_deref() == Some(part.as_str()) {
                    self.package.remove_relationship(&workbook, &rel.id)?;
                }
            }
            self.package.remove(&part);
            self.package.remove_override(&part)?;
        }
        Ok(())
    }
}

/// A sheet part's `sheetData`, with its rows and cells found by number.
struct SheetXml<'d> {
    doc: &'d mut XmlDocument,
    data: NodeId,
    rows: BTreeMap<u32, NodeId>,
    /// The cells of each row, by column (filled when a row is first used).
    cells: HashMap<u32, BTreeMap<u32, NodeId>>,
    prefix: String,
}

impl<'d> SheetXml<'d> {
    fn new(doc: &'d mut XmlDocument) -> Result<Self, Error> {
        let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
        let data = doc.child(root, "sheetData").ok_or_else(|| Error::new("シートに sheetData がありません"))?;
        let prefix = prefix_of(doc, root);
        let mut rows = BTreeMap::new();
        let mut next = 0u32;
        for row in doc.children_named(data, "row") {
            let r = doc.attr(row, "r").and_then(|v| v.parse::<u32>().ok()).map_or(next, |v| v.saturating_sub(1));
            rows.insert(r, row);
            next = r + 1;
        }
        Ok(Self { doc, data, rows, cells: HashMap::new(), prefix })
    }

    fn formula(&self, cell: NodeId) -> Option<NodeId> {
        self.doc.children_named(cell, "f").find(|&n| self.doc.namespace(n) == Some(MAIN_NS))
    }

    fn row_cells(&mut self, row: u32) -> Option<&mut BTreeMap<u32, NodeId>> {
        let node = *self.rows.get(&row)?;
        if !self.cells.contains_key(&row) {
            let mut map = BTreeMap::new();
            let mut next = 0u32;
            for c in self.doc.children_named(node, "c") {
                let column = self.doc.attr(c, "r").and_then(crate::xlsx::cell_ref).map_or(next, |(col, _)| col);
                map.insert(column, c);
                next = column + 1;
            }
            self.cells.insert(row, map);
        }
        self.cells.get_mut(&row)
    }

    fn find_cell(&mut self, row: u32, column: u32) -> Option<NodeId> {
        self.row_cells(row)?.get(&column).copied()
    }

    fn remove_cell(&mut self, row: u32, column: u32, c: NodeId) {
        self.doc.detach(c);
        if let Some(cells) = self.cells.get_mut(&row) {
            cells.remove(&column);
        }
    }

    /// Insert detached `e` among `parent`'s children, before `before` or at
    /// the end.
    fn place(&mut self, parent: NodeId, e: NodeId, before: Option<NodeId>) {
        match before {
            Some(b) => self.doc.insert_before(b, e),
            None => {
                let n = self.doc.children(parent).len();
                self.doc.insert(parent, n, e);
            }
        }
    }

    /// The `c` element of a cell, made (in order) if there is none.
    fn cell(&mut self, row: u32, column: u32) -> NodeId {
        if let Some(c) = self.find_cell(row, column) {
            return c;
        }
        let row_node = match self.rows.get(&row) {
            Some(&r) => r,
            None => {
                let r = detached(self.doc, &format!("{}row", self.prefix));
                self.doc.set_attr(r, "r", &(row + 1).to_string());
                let after = self.rows.range(row + 1..).next().map(|(_, &n)| n);
                let data = self.data;
                self.place(data, r, after);
                self.rows.insert(row, r);
                self.cells.insert(row, BTreeMap::new());
                r
            }
        };
        let _ = self.row_cells(row);
        let c = detached(self.doc, &format!("{}c", self.prefix));
        self.doc.set_attr(c, "r", &cell_name(column, row));
        let cells = &self.cells[&row];
        let next = cells.range(column + 1..).next().map(|(_, &n)| n);
        let last = cells.values().last().copied();
        match (next, last) {
            (Some(n), _) => self.doc.insert_before(n, c),
            // After the row's last cell (so before an extLst, if any).
            (None, Some(l)) => self.doc.insert_after(l, c),
            (None, None) => self.place(row_node, c, None),
        }
        self.cells.get_mut(&row).unwrap().insert(column, c);
        c
    }

    /// A new child element of `parent` at element position `index`.
    fn insert_child(&mut self, parent: NodeId, local: &str, index: usize) -> NodeId {
        let before = self.doc.elements(parent).nth(index);
        let e = detached(self.doc, &format!("{}{local}", self.prefix));
        self.place(parent, e, before);
        e
    }

    /// Write a cell's value (`t`, `v`, `is`); its formula is already there.
    fn write_value(&mut self, c: NodeId, cell: &SheetCell, string: Option<u32>) {
        let original_position = self.doc.elements(c).position(|n| matches!(self.doc.local_name(n), "v" | "is") && self.doc.namespace(n) == Some(MAIN_NS));
        for name in ["v", "is"] {
            while let Some(n) = self.doc.child(c, name) {
                self.doc.detach(n);
            }
        }
        let has_formula = cell.formula.is_some();
        let is_ecma376_error = |e: ibis_doc_model::CellError| matches!(e, ibis_doc_model::CellError::Null | ibis_doc_model::CellError::Div0 | ibis_doc_model::CellError::Value | ibis_doc_model::CellError::Ref | ibis_doc_model::CellError::Name | ibis_doc_model::CellError::Num | ibis_doc_model::CellError::NA);
        let (t, v): (Option<&str>, Option<String>) = match &cell.value {
            CellValue::Empty => (None, None),
            CellValue::Number(n) => (None, Some(number_to_xml(*n))),
            CellValue::Text(s) if has_formula => (Some("str"), Some(crate::xlsx::encode_xstring(s))),
            CellValue::Text(_) => (Some("s"), string.map(|i| i.to_string())),
            CellValue::Bool(b) => (Some("b"), Some(if *b { "1" } else { "0" }.to_string())),
            CellValue::Error(e) if !is_ecma376_error(*e) => (Some("str"), Some(e.as_str().to_string())),
            CellValue::Error(e) => (Some("e"), Some(e.as_str().to_string())),
        };
        match t {
            Some(t) => self.doc.set_attr(c, "t", t),
            None => self.doc.remove_attr(c, "t"),
        }
        if let Some(v) = v {
            let after_formula = self.formula(c).and_then(|f| self.doc.elements(c).position(|n| n == f)).map_or(0, |i| i + 1);
            let index = original_position.unwrap_or(after_formula).max(after_formula);
            let node = self.insert_child(c, "v", index);
            self.doc.set_text(node, &v);
        }
    }

    /// Before a cell's formula changes: if it belongs to a shared formula,
    /// write every cell of that group with its own formula (from the model
    /// as read), so that no cell depends on the one that changes.
    fn unshare_around(&mut self, row: u32, column: u32, old: &Sheet, books: &[String]) -> Result<(), Error> {
        let Some(c) = self.find_cell(row, column) else { return Ok(()) };
        let Some(f) = self.formula(c) else { return Ok(()) };
        match self.doc.attr(f, "t") {
            Some("shared") => {}
            // A dynamic array formula is a formula of its own cell.
            Some("array") if self.doc.attr(c, "cm").is_some() => return Ok(()),
            Some("array") => return Err(unsupported("配列数式の変更")),
            _ => return Ok(()),
        }
        let Some(si) = self.doc.attr(f, "si").map(str::to_string) else { return Ok(()) };
        let members: Vec<(NodeId, NodeId)> = self
            .doc
            .descendants(self.data)
            .into_iter()
            .filter(|&n| self.doc.local_name(n) == "f" && self.doc.namespace(n) == Some(MAIN_NS) && self.doc.attr(n, "t") == Some("shared") && self.doc.attr(n, "si") == Some(si.as_str()))
            .filter_map(|f| Some((self.doc.parent(f)?, f)))
            .collect();
        for (cell, f) in members {
            let Some((col, r)) = self.doc.attr(cell, "r").and_then(crate::xlsx::cell_ref) else {
                return Err(unsupported("位置の書かれていない共有数式の変更"));
            };
            let formula = old.cell(r, col).and_then(|c| c.formula.clone()).ok_or_else(|| unsupported("この共有数式の変更"))?;
            for attr in ["t", "si", "ref"] {
                self.doc.remove_attr(f, attr);
            }
            self.doc.set_text(f, &file_formula_in(books, &formula, table_at(old, r, col))?);
        }
        Ok(())
    }

    /// Widen `dimension/@ref` to take in cells written (it is never narrowed).
    fn grow_dimension(&mut self, cells: &[(u32, u32)]) {
        let Some(&(row0, column0)) = cells.first() else { return };
        let Some(root) = self.doc.root_element() else { return };
        let Some(dim) = self.doc.child(root, "dimension") else { return };
        let current = self.doc.attr(dim, "ref").unwrap_or("A1").to_string();
        // "A1" for a sheet with no cells before.
        let was_empty = current == "A1" && self.rows.values().all(|&r| self.doc.children_named(r, "c").count() <= cells.len());
        let mut r = match crate::xlsx::parse_range(&current) {
            Some(r) if !was_empty => r,
            _ => CellRange { first_row: row0, first_column: column0, last_row: row0, last_column: column0 },
        };
        for &(row, column) in cells {
            r.first_row = r.first_row.min(row);
            r.last_row = r.last_row.max(row);
            r.first_column = r.first_column.min(column);
            r.last_column = r.last_column.max(column);
        }
        let text = if r.first_row == r.last_row && r.first_column == r.last_column {
            cell_name(r.first_column, r.first_row)
        } else {
            format!("{}:{}", cell_name(r.first_column, r.first_row), cell_name(r.last_column, r.last_row))
        };
        if text != current {
            self.doc.set_attr(dim, "ref", &text);
        }
    }
}

/// A `dxf` for each styled rule of `formats`, added at the end of the
/// styles part's `dxfs` (the ones there stay); their ids, rule by rule
/// (`None` for a rule with no format).
fn write_dxfs(package: &mut Package, part: &str, sheet_part: &str, old_formats: &[ibis_doc_model::ConditionalFormat], formats: &[ibis_doc_model::ConditionalFormat]) -> Result<Vec<Vec<Option<u32>>>, Error> {
    let matches = sizes::conditional_matches(old_formats, formats);
    let origins = sizes::conditional_origins(old_formats, formats)?;
    if !formats.iter().zip(&matches).zip(&origins).any(|((f, matched), origin)| matched.is_none() && f.rules.iter().enumerate().any(|(j,r)| r.style.is_some() && origin.is_none_or(|i| old_formats[i].rules.get(j).is_none_or(|old| old.style != r.style)))) {
        return Ok(formats.iter().map(|f| vec![None; f.rules.len()]).collect());
    }
    // Keep the referenced source dxf for a fill-only edit. A fresh modeled
    // dxf would disconnect unknown font/alignment/extension information.
    let source_ids = {
        let sheet = package.xml(sheet_part)?;
        let root = sheet.root_element().ok_or_else(|| unsupported("条件付き書式の元シート"))?;
        sheet.children_named(root, "conditionalFormatting").filter(|&n| sheet.namespace(n)==Some(MAIN_NS))
            .map(|cf| sheet.children_named(cf,"cfRule").filter(|&r|sheet.namespace(r)==Some(MAIN_NS))
                .map(|r|sheet.attr(r,"dxfId").and_then(|id|id.parse::<usize>().ok())).collect::<Vec<_>>()).collect::<Vec<_>>()
    };
    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| Error::new("styles.xml が空です"))?;
    let prefix = prefix_of(doc, root);
    let list = match doc.child(root, "dxfs") {
        Some(l) => l,
        None => {
            let l = detached(doc, &format!("{prefix}dxfs"));
            let before = ["tableStyles", "colors", "extLst"];
            let anchor = doc.elements(root).find(|&c| before.contains(&doc.local_name(c)));
            match anchor {
                Some(a) => doc.insert_before(a, l),
                None => {
                    let n = doc.children(root).len();
                    doc.insert(root, n, l);
                }
            }
            l
        }
    };
    let mut next = doc.children_named(list, "dxf").count() as u32;
    let rgb = |c: ibis_doc_model::Rgb| format!("FF{:06X}", c.0 & 0xFFFFFF);
    let mut out = Vec::new();
    for ((f, matched), origin) in formats.iter().zip(matches).zip(origins) {
        if matched.is_some() {
            out.push(vec![None; f.rules.len()]);
            continue;
        }
        let mut ids = Vec::new();
        for (j,r) in f.rules.iter().enumerate() {
            if origin.is_some_and(|i|old_formats[i].rules.get(j).is_some_and(|old|old.style==r.style)) {ids.push(None);continue;}
            let Some(st) = &r.style else {
                ids.push(None);
                continue;
            };
            let old_style = origin.and_then(|i|old_formats[i].rules.get(j)).and_then(|r|r.style.as_ref());
            if let Some(old) = old_style {
                let mut except_fill = st.clone();
                except_fill.fill = old.fill;
                if &except_fill == old && st.fill.is_some() {
                    let id = origin.and_then(|i|source_ids.get(i)).and_then(|rs|rs.get(j)).copied().flatten()
                        .ok_or_else(|| unsupported("条件付き書式の元dxf参照欠落"))?;
                    let entries: Vec<_> = doc.children_named(list,"dxf").collect();
                    if entries.iter().any(|&n|doc.namespace(n)!=Some(MAIN_NS)) {return Err(unsupported("dxf名前空間"));}
                    let original = *entries.get(id).ok_or_else(||unsupported("条件付き書式の元dxf参照"))?;
                    let clone = doc.deep_clone(original);
                    let n = doc.children(list).len();
                    doc.insert(list,n,clone); // Resolve inherited prefixes before inspecting.
                    patch_conditional_fill(doc,clone,st.fill.unwrap(),&prefix)?;
                    ids.push(Some(next));
                    next += 1;
                    continue;
                }
            }
            let dxf = detached(doc, &format!("{prefix}dxf"));
            if st.color.is_some() || st.bold.is_some() || st.italic.is_some() || st.underline.is_some() || st.strike.is_some() || st.vert_align.is_some() {
                let font = detached(doc, &format!("{prefix}font"));
                for (name, on) in [("b", st.bold), ("i", st.italic), ("strike", st.strike)] {
                    if let Some(on) = on {
                        let e = detached(doc, &format!("{prefix}{name}"));
                        if !on {
                            doc.set_attr(e, "val", "0");
                        }
                        let n = doc.children(font).len();
                        doc.insert(font, n, e);
                    }
                }
                if let Some(u) = st.underline {
                    let e = detached(doc, &format!("{prefix}u"));
                    if u != CellUnderline::Single {
                        doc.set_attr(e, "val", u.ooxml());
                    }
                    let n = doc.children(font).len();
                    doc.insert(font, n, e);
                }
                if let Some(va) = st.vert_align {
                    let e = detached(doc, &format!("{prefix}vertAlign"));
                    doc.set_attr(e, "val", match va {
                        VerticalPosition::Superscript => "superscript",
                        VerticalPosition::Subscript => "subscript",
                        VerticalPosition::Baseline => "baseline",
                    });
                    let n = doc.children(font).len();
                    doc.insert(font, n, e);
                }
                if let Some(c) = st.color {
                    let e = detached(doc, &format!("{prefix}color"));
                    doc.set_attr(e, "rgb", &rgb(c));
                    let n = doc.children(font).len();
                    doc.insert(font, n, e);
                }
                let n = doc.children(dxf).len();
                doc.insert(dxf, n, font);
            }
            if let Some(code) = &st.number_format {
                let id = number_format_id(doc, root, code, &prefix);
                let e = detached(doc, &format!("{prefix}numFmt"));
                doc.set_attr(e, "numFmtId", &id.to_string());
                doc.set_attr(e, "formatCode", code);
                let n = doc.children(dxf).len();
                doc.insert(dxf, n, e);
            }
            if let Some((preset, fg, bg)) = &st.pattern {
                let f = detached(doc, &format!("{prefix}fill"));
                let p = detached(doc, &format!("{prefix}patternFill"));
                doc.set_attr(p, "patternType", preset);
                let fg_elem = detached(doc, &format!("{prefix}fgColor"));
                doc.set_attr(fg_elem, "rgb", &rgb(*fg));
                doc.insert(p, 0, fg_elem);
                let bg_elem = detached(doc, &format!("{prefix}bgColor"));
                doc.set_attr(bg_elem, "rgb", &rgb(*bg));
                let n = doc.children(p).len();
                doc.insert(p, n, bg_elem);
                doc.insert(f, 0, p);
                let n = doc.children(dxf).len();
                doc.insert(dxf, n, f);
            } else if let Some(gradient) = &st.gradient {
                let f = detached(doc, &format!("{prefix}fill"));
                let g = detached(doc, &format!("{prefix}gradientFill"));
                let stops = match gradient {
                    CellGradient::Linear { degree, stops } => {
                        doc.set_attr(g, "degree", &degree.to_string());
                        stops
                    }
                    CellGradient::Path { focus, stops, .. } => {
                        doc.set_attr(g, "type", "path");
                        let l = focus[0] as f64 / 100_000.0;
                        let t = focus[1] as f64 / 100_000.0;
                        let r = (100_000 - focus[2]) as f64 / 100_000.0;
                        let b = (100_000 - focus[3]) as f64 / 100_000.0;
                        doc.set_attr(g, "left", &format!("{:.4}", l));
                        doc.set_attr(g, "top", &format!("{:.4}", t));
                        doc.set_attr(g, "right", &format!("{:.4}", r));
                        doc.set_attr(g, "bottom", &format!("{:.4}", b));
                        stops
                    }
                };
                for (pos, col) in stops {
                    let s = detached(doc, &format!("{prefix}stop"));
                    let frac = *pos as f64 / 1000.0;
                    doc.set_attr(s, "position", &format!("{:.2}", frac));
                    let c = detached(doc, &format!("{prefix}color"));
                    doc.set_attr(c, "rgb", &rgb(*col));
                    doc.insert(s, 0, c);
                    let n = doc.children(g).len();
                    doc.insert(g, n, s);
                }
                doc.insert(f, 0, g);
                let n = doc.children(dxf).len();
                doc.insert(dxf, n, f);
            } else if let Some(fill) = st.fill {
                let f = detached(doc, &format!("{prefix}fill"));
                let p = detached(doc, &format!("{prefix}patternFill"));
                let bg = detached(doc, &format!("{prefix}bgColor"));
                doc.set_attr(bg, "rgb", &rgb(fill));
                doc.insert(p, 0, bg);
                doc.insert(f, 0, p);
                let n = doc.children(dxf).len();
                doc.insert(dxf, n, f);
            }
            if st.borders.iter().any(Option::is_some) {
                let b = detached(doc, &format!("{prefix}border"));
                let style = CellStyle { left: st.borders[0], right: st.borders[1], top: st.borders[2], bottom: st.borders[3], ..Default::default() };
                write_border(doc, b, &CellStyle::default(), &style, &prefix);
                let n = doc.children(dxf).len();
                doc.insert(dxf, n, b);
            }
            let n = doc.children(list).len();
            doc.insert(list, n, dxf);
            ids.push(Some(next));
            next += 1;
        }
        out.push(ids);
    }
    let count = doc.children_named(list, "dxf").count();
    doc.set_attr(list, "count", &count.to_string());
    Ok(out)
}

/// The `dxf` of each colour filter of `filter`, by column: the one the
/// sheet has for a column whose filter is unchanged, else a new one at the
/// end of the styles' `dxfs` (a fill, or a font's colour).
fn write_filter_dxfs(package: &mut Package, part: &str, sheet_part: &str, old: Option<&ibis_doc_model::AutoFilter>, filter: &ibis_doc_model::AutoFilter) -> Result<HashMap<u32, u32>, Error> {
    use ibis_doc_model::ColumnFilter;
    // The dxfIds of the colour filters there, by column.
    let mut there: HashMap<u32, u32> = HashMap::new();
    if let Some(old) = old {
        let sheet = package.xml(sheet_part)?;
        let root = sheet.root_element().ok_or_else(|| unsupported("色フィルターの元シート"))?;
        if let Some(af) = sheet.child(root, "autoFilter") {
            for fc in sheet.children_named(af, "filterColumn") {
                let column = sheet.attr(fc, "colId").and_then(|v| v.parse::<u32>().ok()).map(|c| c + old.range.first_column);
                let id = sheet.child(fc, "colorFilter").and_then(|c| sheet.attr(c, "dxfId")).and_then(|v| v.parse::<u32>().ok());
                if let (Some(column), Some(id)) = (column, id) {
                    there.insert(column, id);
                }
            }
        }
    }
    let mut out = HashMap::new();
    let wanted: Vec<(u32, bool, Option<Rgb>)> = filter.columns.iter().filter_map(|(c, x)| match x {
        ColumnFilter::Color { cell, color } => Some((*c, *cell, *color)),
        _ => None,
    }).collect();
    let mut new = Vec::new();
    for (column, cell, color) in wanted {
        let same = old.is_some_and(|o| o.range.first_column == filter.range.first_column && o.columns.iter().any(|(c, x)| *c == column && *x == ColumnFilter::Color { cell, color }));
        match there.get(&column) {
            Some(&id) if same => {
                out.insert(column, id);
            }
            _ => new.push((column, cell, color)),
        }
    }
    if new.is_empty() {
        return Ok(out);
    }
    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| Error::new("styles.xml が空です"))?;
    let prefix = prefix_of(doc, root);
    let list = match doc.child(root, "dxfs") {
        Some(l) => l,
        None => {
            let l = detached(doc, &format!("{prefix}dxfs"));
            let before = ["tableStyles", "colors", "extLst"];
            let anchor = doc.elements(root).find(|&c| before.contains(&doc.local_name(c)));
            match anchor {
                Some(a) => doc.insert_before(a, l),
                None => {
                    let n = doc.children(root).len();
                    doc.insert(root, n, l);
                }
            }
            l
        }
    };
    let mut next = doc.children_named(list, "dxf").count() as u32;
    for (column, cell, color) in new {
        let dxf = detached(doc, &format!("{prefix}dxf"));
        if cell {
            let f = detached(doc, &format!("{prefix}fill"));
            let p = detached(doc, &format!("{prefix}patternFill"));
            if let Some(c) = color {
                doc.set_attr(p, "patternType", "solid");
                for name in ["fgColor", "bgColor"] {
                    let e = detached(doc, &format!("{prefix}{name}"));
                    doc.set_attr(e, "rgb", &format!("FF{:06X}", c.0 & 0xFF_FFFF));
                    let n = doc.children(p).len();
                    doc.insert(p, n, e);
                }
            }
            doc.insert(f, 0, p);
            doc.insert(dxf, 0, f);
        } else {
            let f = detached(doc, &format!("{prefix}font"));
            let e = detached(doc, &format!("{prefix}color"));
            match color {
                Some(c) => doc.set_attr(e, "rgb", &format!("FF{:06X}", c.0 & 0xFF_FFFF)),
                None => doc.set_attr(e, "auto", "1"),
            }
            doc.insert(f, 0, e);
            doc.insert(dxf, 0, f);
        }
        let n = doc.children(list).len();
        doc.insert(list, n, dxf);
        out.insert(column, next);
        next += 1;
    }
    let count = doc.children_named(list, "dxf").count();
    doc.set_attr(list, "count", &count.to_string());
    Ok(out)
}

/// Patch one modeled color in a cloned dxf while retaining opaque XML.
/// Pattern/gradient semantics that the model does not represent are refused.
fn patch_conditional_fill(doc: &mut XmlDocument, dxf: NodeId, color: Rgb, prefix: &str) -> Result<(), Error> {
    let children = |doc: &XmlDocument, parent, name| doc.children_named(parent,name).filter(|&n|doc.namespace(n)==Some(MAIN_NS)).collect::<Vec<_>>();
    let fills = children(doc,dxf,"fill");
    if fills.len()>1 {return Err(unsupported("複数dxf fill"));}
    let fill = match fills.first() {
        Some(&n) => n,
        None => {
            let f = detached(doc, &format!("{prefix}fill"));
            let after = ["alignment", "border", "protection", "extLst"];
            let anchor = doc.elements(dxf).find(|&c| doc.namespace(c) == Some(MAIN_NS) && after.contains(&doc.local_name(c)));
            match anchor {
                Some(a) => doc.insert_before(a, f),
                None => {
                    let n = doc.children(dxf).len();
                    doc.insert(dxf, n, f);
                }
            }
            f
        }
    };
    if doc.elements(fill).any(|n|doc.namespace(n)==Some(MAIN_NS)&&doc.local_name(n)!="patternFill") {return Err(unsupported("dxf gradient/未知fillの色編集"));}
    let patterns = children(doc,fill,"patternFill");
    if patterns.len()>1 {return Err(unsupported("複数dxf patternFill"));}
    let pattern = match patterns.first() {Some(&n)=>n,None=>doc.append_element(fill,&format!("{prefix}patternFill"))};
    if doc.attr(pattern,"patternType").is_some_and(|s|s!="solid"&&s!="none") {return Err(unsupported("dxf模様塗りの色編集"));}
    let mut colors = children(doc,pattern,"bgColor");
    colors.extend(children(doc,pattern,"fgColor"));
    if colors.len()>1 {return Err(unsupported("複数dxf fill色の編集"));}
    let node = match colors.first() {Some(&n)=>n,None=>doc.append_element(pattern,&format!("{prefix}bgColor"))};
    for attr in ["rgb","theme","tint","indexed","auto"] {doc.remove_attr(node,attr);}
    doc.set_attr(node,"rgb",&format!("FF{:06X}",color.0&0xFFFFFF));
    Ok(())
}

/// The sheet's hyperlinks as the model has them: `hyperlinks` written
/// again, a web address through a relationship of the sheet (one with the
/// same target used again).
fn write_hyperlinks(package: &mut Package, part: &str, links: &[ibis_doc_model::Hyperlink]) -> Result<(), Error> {
    const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
    let existing = package.relationships(part)?;
    let mut ids: Vec<Option<String>> = Vec::new();
    for l in links {
        ids.push(match &l.target {
            Some(t) => Some(match existing.iter().find(|r| r.kind == REL && &r.target == t) {
                Some(r) => r.id.clone(),
                None => package.add_relationship(part, REL, t, true)?,
            }),
            None => None,
        });
    }
    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    if let Some(old) = doc.child(root, "hyperlinks") {
        doc.detach(old);
    }
    if links.is_empty() {
        return Ok(());
    }
    let r = doc.lookup_prefix(root, "http://schemas.openxmlformats.org/officeDocument/2006/relationships").unwrap_or_else(|| {
        doc.set_attr(root, "xmlns:r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships");
        "r".into()
    });
    let list = detached(doc, &format!("{prefix}hyperlinks"));
    for (l, id) in links.iter().zip(ids) {
        let e = detached(doc, &format!("{prefix}hyperlink"));
        let m = l.range;
        let reference = if m.first_row == m.last_row && m.first_column == m.last_column { cell_name(m.first_column, m.first_row) } else { format!("{}:{}", cell_name(m.first_column, m.first_row), cell_name(m.last_column, m.last_row)) };
        doc.set_attr(e, "ref", &reference);
        if let Some(id) = id {
            doc.set_attr(e, &format!("{r}:id"), &id);
        }
        if let Some(loc) = &l.location {
            doc.set_attr(e, "location", loc);
        }
        if let Some(t) = &l.tooltip {
            doc.set_attr(e, "tooltip", t);
        }
        let n = doc.children(list).len();
        doc.insert(list, n, e);
    }
    let before = ["printOptions", "pageMargins", "pageSetup", "headerFooter", "rowBreaks", "colBreaks", "customProperties", "cellWatches", "ignoredErrors", "smartTags", "drawing", "legacyDrawing", "legacyDrawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"];
    let anchor = doc.elements(root).find(|&c| before.contains(&doc.local_name(c)));
    match anchor {
        Some(a) => doc.insert_before(a, list),
        None => {
            let n = doc.children(root).len();
            doc.insert(root, n, list);
        }
    }
    Ok(())
}

const CHART_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const DRAWING_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

/// Write alternative text (`cNvPr/@descr`, `@title`) and shape names (`cNvPr/@name`)
/// of shapes / pictures / charts on the sheet if changed.
fn write_sheet_shape_labels(
    package: &mut Package,
    shape_sources: &[XlsxShapeSource],
    chart_sources: &[ChartSource],
    old_slide: &ibis_doc_model::Slide,
    new_slide: &ibis_doc_model::Slide,
) -> Result<(), Error> {
    let has_shape_geometry_change = old_slide.shapes.iter().zip(&new_slide.shapes).any(|(o, n)| {
        match (&o.kind, &n.kind) {
            (ShapeKind::Auto(oa), ShapeKind::Auto(na)) => oa.geometry != na.geometry,
            _ => false,
        }
    });
    let has_formula_change = old_slide.shape_formulas != new_slide.shape_formulas;
    let has_warp_change = old_slide.shapes.iter().zip(&new_slide.shapes).any(|(o, n)| {
        o.text_body().and_then(|b| b.warp.as_deref()) != n.text_body().and_then(|b| b.warp.as_deref())
    });
    let has_picture_effects_change = new_slide.shapes.iter().any(|shape| {
        match (
            old_slide.shapes.iter().find(|s| s.id == shape.id).map(|s| &s.kind),
            &shape.kind,
        ) {
            (Some(ShapeKind::Picture(op)), ShapeKind::Picture(np)) => {
                op.clear != np.clear || op.duotone != np.duotone || op.luminance != np.luminance
            }
            _ => false,
        }
    });

    if old_slide.shape_alt_texts == new_slide.shape_alt_texts
        && old_slide.shape_names == new_slide.shape_names
        && old_slide.shape_placements == new_slide.shape_placements
        && old_slide.shape_prints == new_slide.shape_prints
        && !has_shape_geometry_change
        && !has_formula_change
        && !has_warp_change
        && !has_picture_effects_change
    {
        return Ok(());
    }

    struct ShapeMetaChange {
        c_nv_pr_id: String,
        name: Option<String>,
        alt: Option<Option<ibis_doc_model::AltText>>,
        placement: Option<ibis_doc_model::ObjectPlacement>,
        print: Option<bool>,
        geometry_preset: Option<String>,
        formula: Option<Option<String>>,
        warp: Option<Option<String>>,
        picture_effects: Option<(Option<Rgb>, Option<(Rgb, Rgb)>, Option<(i32, i32)>)>,
    }

    let mut changes_by_drawing: HashMap<String, Vec<ShapeMetaChange>> = HashMap::new();

    for shape in &new_slide.shapes {
        if matches!(shape.kind, ShapeKind::Sheet(_)) {
            continue;
        }
        let name = new_slide.shape_names.get(&shape.id);
        let old_name = old_slide.shape_names.get(&shape.id);
        let name_change = (name != old_name).then(|| name.cloned().unwrap_or_default());

        let alt = new_slide.shape_alt_texts.get(&shape.id);
        let old_alt = old_slide.shape_alt_texts.get(&shape.id);
        let alt_change = (alt != old_alt).then(|| alt.cloned());

        let placement = new_slide.shape_placements.get(&shape.id).copied().unwrap_or_default();
        let old_placement = old_slide.shape_placements.get(&shape.id).copied().unwrap_or_default();
        let placement_change = (placement != old_placement).then_some(placement);

        let print = new_slide.shape_prints.get(&shape.id).copied().unwrap_or(true);
        let old_print = old_slide.shape_prints.get(&shape.id).copied().unwrap_or(true);
        let print_change = (print != old_print).then_some(print);

        let geometry_preset_change = match (
            old_slide.shapes.iter().find(|s| s.id == shape.id).map(|s| &s.kind),
            &shape.kind,
        ) {
            (Some(ShapeKind::Auto(old_a)), ShapeKind::Auto(new_a)) if old_a.geometry != new_a.geometry => {
                new_a.geometry.preset_name().map(|p| p.to_string())
            }
            _ => None,
        };

        let formula = new_slide.shape_formulas.get(&shape.id);
        let old_formula = old_slide.shape_formulas.get(&shape.id);
        let formula_change = (formula != old_formula).then(|| formula.cloned());

        let warp_change = match (
            old_slide.shapes.iter().find(|s| s.id == shape.id).and_then(|s| s.text_body()),
            shape.text_body(),
        ) {
            (Some(old_b), Some(new_b)) if old_b.warp != new_b.warp => Some(new_b.warp.clone()),
            (None, Some(new_b)) if new_b.warp.is_some() => Some(new_b.warp.clone()),
            _ => None,
        };

        let picture_effects_change = match (
            old_slide.shapes.iter().find(|s| s.id == shape.id).map(|s| &s.kind),
            &shape.kind,
        ) {
            (Some(ShapeKind::Picture(op)), ShapeKind::Picture(np))
                if op.clear != np.clear || op.duotone != np.duotone || op.luminance != np.luminance =>
            {
                Some((np.clear, np.duotone, np.luminance))
            }
            _ => None,
        };

        if name_change.is_none()
            && alt_change.is_none()
            && placement_change.is_none()
            && print_change.is_none()
            && geometry_preset_change.is_none()
            && formula_change.is_none()
            && warp_change.is_none()
            && picture_effects_change.is_none()
        {
            continue;
        }

        let source_info = shape_sources
            .iter()
            .find(|s| s.shape == shape.id)
            .map(|s| (s.drawing.clone(), s.c_nv_pr_id.clone()))
            .or_else(|| {
                chart_sources.iter().find(|c| c.shape == shape.id).and_then(|c| {
                    c.frame_id.as_ref().map(|fid| (c.drawing.clone(), fid.clone()))
                })
            });

        if let Some((drawing, c_nv_pr_id)) = source_info {
            changes_by_drawing.entry(drawing).or_default().push(ShapeMetaChange {
                c_nv_pr_id,
                name: name_change,
                alt: alt_change,
                placement: placement_change,
                print: print_change,
                geometry_preset: geometry_preset_change,
                formula: formula_change,
                warp: warp_change,
                picture_effects: picture_effects_change,
            });
        }
    }

    for (drawing, changes) in changes_by_drawing {
        let doc = package.xml_mut(&drawing)?;
        let Some(root) = doc.root_element() else { continue };
        let found: HashMap<String, NodeId> = doc
            .descendants(root)
            .into_iter()
            .filter(|&n| doc.local_name(n) == "cNvPr")
            .filter_map(|n| doc.attr(n, "id").map(|id| (id.to_string(), n)))
            .collect();

        for item in changes {
            let Some(&node) = found.get(&item.c_nv_pr_id) else { continue };
            if let Some(name) = item.name {
                doc.set_attr(node, "name", &name);
            }
            if let Some(alt_val) = item.alt {
                match alt_val {
                    Some(a) => {
                        if a.description.is_empty() {
                            doc.remove_attr(node, "descr");
                        } else {
                            doc.set_attr(node, "descr", &a.description);
                        }
                        if a.title.is_empty() {
                            doc.remove_attr(node, "title");
                        } else {
                            doc.set_attr(node, "title", &a.title);
                        }
                    }
                    None => {
                        doc.remove_attr(node, "descr");
                        doc.remove_attr(node, "title");
                    }
                }
            }
            if item.placement.is_some() || item.print.is_some() {
                let mut curr = doc.parent(node);
                let mut anchor = None;
                while let Some(p) = curr {
                    if matches!(doc.local_name(p), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor") {
                        anchor = Some(p);
                        break;
                    }
                    curr = doc.parent(p);
                }
                if let Some(a) = anchor {
                    if let Some(placement) = item.placement {
                        let edit_as_val = match placement {
                            ibis_doc_model::ObjectPlacement::MoveAndSize => "twoCell",
                            ibis_doc_model::ObjectPlacement::MoveOnly => "oneCell",
                            ibis_doc_model::ObjectPlacement::Free => "absolute",
                        };
                        doc.set_attr(a, "editAs", edit_as_val);
                    }
                    if let Some(print) = item.print {
                        let client_data = if let Some(cd) = doc.children(a).into_iter().copied().find(|&c| doc.local_name(c) == "clientData") {
                            cd
                        } else {
                            let pfx = prefix_of(doc, a);
                            doc.append_element(a, &format!("{pfx}clientData"))
                        };
                        doc.set_attr(client_data, "fPrintsWithSheet", if print { "1" } else { "0" });
                    }
                }
            }
            if item.geometry_preset.is_some() || item.formula.is_some() || item.warp.is_some() || item.picture_effects.is_some() {
                let mut curr = doc.parent(node);
                let mut sp_node = None;
                while let Some(p) = curr {
                    if matches!(doc.local_name(p), "sp" | "pic") {
                        sp_node = Some(p);
                        break;
                    }
                    curr = doc.parent(p);
                }
                if let Some(sp) = sp_node {
                    if let Some(ref preset) = item.geometry_preset {
                        let sp_pr = doc.children(sp).into_iter().copied().find(|&c| doc.local_name(c) == "spPr");
                        if let Some(sp_pr) = sp_pr {
                            let prst_geom = doc.children(sp_pr).into_iter().copied().find(|&c| doc.local_name(c) == "prstGeom");
                            if let Some(pg) = prst_geom {
                                doc.set_attr(pg, "prst", preset);
                            } else {
                                let pfx = prefix_of(doc, sp_pr);
                                let pg = doc.append_element(sp_pr, &format!("{pfx}prstGeom"));
                                doc.set_attr(pg, "prst", preset);
                            }
                        }
                    }
                    if let Some(ref formula_opt) = item.formula {
                        let f_node = doc.children(sp).into_iter().copied().find(|&c| doc.local_name(c) == "f");
                        match formula_opt {
                            Some(f_text) => {
                                if let Some(f_node) = f_node {
                                    doc.set_text(f_node, f_text);
                                } else {
                                    let pfx = prefix_of(doc, sp);
                                    let new_f = doc.append_element(sp, &format!("{pfx}f"));
                                    doc.set_text(new_f, f_text);
                                }
                            }
                            None => {
                                if let Some(f_node) = f_node {
                                    doc.detach(f_node);
                                }
                            }
                        }
                    }
                    if let Some(ref warp_opt) = item.warp {
                        let a_pfx = match doc.lookup_prefix(root, DRAWING_NS) {
                            Some(p) if !p.is_empty() => format!("{p}:"),
                            _ => "a:".into(),
                        };
                        let tx_body = doc.children(sp).into_iter().copied().find(|&c| doc.local_name(c) == "txBody");
                        let tx_body = match tx_body {
                            Some(tb) => tb,
                            None => {
                                let pfx = prefix_of(doc, sp);
                                doc.append_element(sp, &format!("{pfx}txBody"))
                            }
                        };
                        let body_pr = doc.children(tx_body).into_iter().copied().find(|&c| doc.local_name(c) == "bodyPr");
                        let body_pr = match body_pr {
                            Some(bp) => bp,
                            None => doc.append_element(tx_body, &format!("{a_pfx}bodyPr")),
                        };
                        let warps: Vec<NodeId> = doc.children(body_pr).into_iter().copied().filter(|&c| doc.local_name(c) == "prstTxWarp").collect();
                        match warp_opt {
                            Some(warp_val) => {
                                if warps.is_empty() {
                                    let w_node = doc.append_element(body_pr, &format!("{a_pfx}prstTxWarp"));
                                    doc.set_attr(w_node, "prst", warp_val);
                                    let _ = doc.append_element(w_node, &format!("{a_pfx}avLst"));
                                } else {
                                    for w in warps {
                                        doc.set_attr(w, "prst", warp_val);
                                    }
                                }
                            }
                            None => {
                                for w in warps {
                                    doc.detach(w);
                                }
                            }
                        }
                    }
                    if let Some((clear, duotone, luminance)) = item.picture_effects {
                        let a_pfx = match doc.lookup_prefix(root, DRAWING_NS) {
                            Some(p) if !p.is_empty() => format!("{p}:"),
                            _ => "a:".into(),
                        };
                        let blip_node = doc.descendants(sp).into_iter().find(|&d| doc.local_name(d) == "blip");
                        if let Some(blip) = blip_node {
                            let ext_lst = doc.children(blip).into_iter().copied().find(|&c| doc.local_name(c) == "extLst");
                            if let Some(ext) = ext_lst {
                                doc.detach(ext);
                            }

                            let to_remove: Vec<NodeId> = doc
                                .children(blip)
                                .into_iter()
                                .copied()
                                .filter(|&c| matches!(doc.local_name(c), "clrChange" | "duotone" | "lum"))
                                .collect();
                            for child in to_remove {
                                doc.detach(child);
                            }

                            if let Some((dark, light)) = duotone {
                                let d_node = doc.append_element(blip, &format!("{a_pfx}duotone"));
                                let c1 = doc.append_element(d_node, &format!("{a_pfx}srgbClr"));
                                doc.set_attr(c1, "val", &format!("{:06X}", dark.0 & 0xff_ffff));
                                let c2 = doc.append_element(d_node, &format!("{a_pfx}srgbClr"));
                                doc.set_attr(c2, "val", &format!("{:06X}", light.0 & 0xff_ffff));
                            }

                            if let Some(color) = clear {
                                let clr_hex = format!("{:06X}", color.0 & 0xff_ffff);
                                let change_node = doc.append_element(blip, &format!("{a_pfx}clrChange"));
                                let from_node = doc.append_element(change_node, &format!("{a_pfx}clrFrom"));
                                let from_srgb = doc.append_element(from_node, &format!("{a_pfx}srgbClr"));
                                doc.set_attr(from_srgb, "val", &clr_hex);

                                let to_node = doc.append_element(change_node, &format!("{a_pfx}clrTo"));
                                let to_srgb = doc.append_element(to_node, &format!("{a_pfx}srgbClr"));
                                doc.set_attr(to_srgb, "val", &clr_hex);
                                let alpha = doc.append_element(to_srgb, &format!("{a_pfx}alpha"));
                                doc.set_attr(alpha, "val", "0");
                            }

                            if let Some((bright, contrast)) = luminance {
                                if bright != 0 || contrast != 0 {
                                    let lum_node = doc.append_element(blip, &format!("{a_pfx}lum"));
                                    if bright != 0 {
                                        doc.set_attr(lum_node, "bright", &bright.to_string());
                                    }
                                    if contrast != 0 {
                                        doc.set_attr(lum_node, "contrast", &contrast.to_string());
                                    }
                                }
                            }

                            if let Some(ext) = ext_lst {
                                let len = doc.children(blip).len();
                                doc.insert(blip, len, ext);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// A new, empty drawing part for the sheet part `sheet_part`, related to it
/// and named in its `drawing` element.
fn create_drawing(package: &mut Package, sheet_part: &str) -> Result<String, Error> {
    let mut n = 1;
    let part = loop {
        let candidate = format!("xl/drawings/drawing{n}.xml");
        if !package.contains(&candidate) {
            break candidate;
        }
        n += 1;
    };
    let mut doc = XmlDocument::new_with_declaration();
    let root = doc.append_element(XmlDocument::ROOT, "xdr:wsDr");
    doc.set_attr(root, "xmlns:xdr", "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing");
    doc.set_attr(root, "xmlns:a", "http://schemas.openxmlformats.org/drawingml/2006/main");
    package.set_xml(&part, doc);
    package.set_override(&part, CT_DRAWING)?;

    let rel_target = super::pptx::relative(sheet_part, &part);
    let rid = package.add_relationship(sheet_part, REL_DRAWING, &rel_target, false)?;

    let sheet_doc = package.xml_mut(sheet_part)?;
    let root = sheet_doc.root_element().ok_or_else(|| Error::new("シート XML が空です"))?;
    if sheet_doc.lookup_prefix(root, REL_NS).is_none() {
        sheet_doc.set_attr(root, "xmlns:r", REL_NS);
    }
    let r = sheet_doc.lookup_prefix(root, REL_NS).unwrap_or_else(|| "r".into());
    let before_drawing = ["drawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"];
    let at = sheet_doc.elements(root).find(|&c| before_drawing.contains(&sheet_doc.local_name(c)));
    let drawing_el = detached(sheet_doc, "drawing");
    match at {
        Some(next) => sheet_doc.insert_before(next, drawing_el),
        None => {
            let count = sheet_doc.children(root).len();
            sheet_doc.insert(root, count, drawing_el);
        }
    }
    sheet_doc.set_attr(drawing_el, &format!("{r}:id"), &rid);
    Ok(part)
}

/// Transfer chart drawings between sheets when charts have moved to another sheet.
/// Returns the set of shape IDs for the moved charts.
fn move_chart_drawings(
    package: &mut Package,
    map: &mut SourceMap,
    baseline: &Presentation,
    current: &Presentation,
    chart_data: &super::pptx_chart_data::ChartDataEdits,
) -> Result<HashSet<ibis_doc_model::ShapeId>, Error> {
    let mut moved_chart_ids = HashSet::new();

    let mut moves = Vec::new();
    for old_slide in &baseline.slides {
        for old_shape in &old_slide.shapes {
            if !matches!(old_shape.kind, ShapeKind::Chart(_)) {
                continue;
            }
            if let Some(new_slide) = current.slides.iter().find(|s| s.shapes.iter().any(|sh| sh.id == old_shape.id)) {
                if new_slide.id != old_slide.id {
                    let new_shape = new_slide.shapes.iter().find(|sh| sh.id == old_shape.id).unwrap().clone();
                    moves.push((old_shape.id, old_slide.id, new_slide.id, old_shape.clone(), new_shape));
                }
            }
        }
    }

    if moves.is_empty() {
        return Ok(moved_chart_ids);
    }

    let wb = current.workbook.as_deref().ok_or_else(|| unsupported("ブックの変更"))?;

    for (shape_id, _old_slide_id, new_slide_id, old_shape, new_shape) in moves {
        moved_chart_ids.insert(shape_id);

        let Some(source_idx) = map.charts.iter().position(|s| s.shape == shape_id) else {
            return Err(unsupported("このグラフの移動元情報が見つかりません"));
        };
        let old_drawing_part = map.charts[source_idx].drawing.clone();
        let chart_part = map.charts[source_idx].chart.clone();
        let frame_id = map.charts[source_idx].frame_id.clone();

        let dest_sheet_idx = wb.sheets.iter().position(|s| s.slide == Some(new_slide_id)).ok_or_else(|| unsupported("移動先シート"))?;
        let dest_sheet_part = map.sheet_parts.get(dest_sheet_idx).cloned().flatten().ok_or_else(|| unsupported("移動先シートのXMLパート"))?;
        let dest_sheet = current.slide(new_slide_id).and_then(sheet_of).ok_or_else(|| unsupported("移動先シート"))?;

        let dest_rels = package.relationships(&dest_sheet_part)?;
        let drawing_rel = dest_rels.into_iter().find(|r| r.kind == REL_DRAWING && !r.external);
        let dest_drawing_part = match drawing_rel {
            Some(rel) => rel.target_part(&dest_sheet_part).ok_or_else(|| unsupported("移動先シートの描画パート"))?,
            None => create_drawing(package, &dest_sheet_part)?,
        };

        if old_drawing_part != dest_drawing_part {
            let old_rels = package.relationships(&old_drawing_part)?;
            let old_rel = old_rels.iter().find(|r| {
                r.kind == REL_CHART && r.target_part(&old_drawing_part).as_deref() == Some(&chart_part)
            }).cloned();

            let old_drawing_doc = package.xml_mut(&old_drawing_part)?;
            let old_root = old_drawing_doc.root_element().ok_or_else(|| Error::new("旧描画の XML が空です"))?;
            let frame_node = old_drawing_doc.descendants(old_root).into_iter().find(|&n| {
                if old_drawing_doc.local_name(n) != "graphicFrame" {
                    return false;
                }
                if let Some(fid) = frame_id.as_deref() {
                    if old_drawing_doc.path(n, &["nvGraphicFramePr", "cNvPr"]).and_then(|c| old_drawing_doc.attr(c, "id")) == Some(fid) {
                        return true;
                    }
                }
                if let Some(r) = &old_rel {
                    if old_drawing_doc.descendants(n).into_iter().any(|c| {
                        old_drawing_doc.local_name(c) == "chart" && old_drawing_doc.attr_local(c, "id") == Some(&r.id)
                    }) {
                        return true;
                    }
                }
                false
            });
            let Some(frame_node) = frame_node else {
                return Err(unsupported("移動元描画のグラフフレームが見つかりません"));
            };
            let anchor = old_drawing_doc.parent(frame_node).filter(|&a| {
                matches!(old_drawing_doc.local_name(a), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor")
            }).ok_or_else(|| unsupported("移動元描画のアンカーが見つかりません"))?;

            let old_doc_snapshot = old_drawing_doc.clone();
            old_drawing_doc.detach(anchor);

            let dest_drawing_doc = package.xml_mut(&dest_drawing_part)?;
            let dest_root = dest_drawing_doc.root_element().ok_or_else(|| Error::new("新描画の XML が空です"))?;
            let imported_anchor = dest_drawing_doc.import(&old_doc_snapshot, anchor);
            let n = dest_drawing_doc.children(dest_root).len();
            dest_drawing_doc.insert(dest_root, n, imported_anchor);

            let chart_target = super::pptx::relative(&dest_drawing_part, &chart_part);
            let new_rid = package.add_relationship(&dest_drawing_part, REL_CHART, &chart_target, false)?;

            let dest_drawing_doc = package.xml_mut(&dest_drawing_part)?;
            let chart_node = dest_drawing_doc.descendants(imported_anchor).into_iter().find(|&c| {
                dest_drawing_doc.local_name(c) == "chart"
            });
            if let Some(chart_node) = chart_node {
                let attr_name = dest_drawing_doc.attributes(chart_node).iter()
                    .find(|a| a.name.split_once(':').is_some_and(|(_, l)| l == "id"))
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| {
                        let r = dest_drawing_doc.lookup_prefix(chart_node, REL_NS).unwrap_or_else(|| "r".into());
                        format!("{r}:id")
                    });
                dest_drawing_doc.set_attr(chart_node, &attr_name, &new_rid);
            }

            if let Some(r) = old_rel {
                package.remove_relationship(&old_drawing_part, &r.id)?;
            }
        }

        map.charts[source_idx].drawing = dest_drawing_part.clone();
        for shape_source in &mut map.shapes {
            if shape_source.shape == shape_id {
                shape_source.drawing = dest_drawing_part.clone();
            }
        }

        write_anchor(package, &map.charts[source_idx], dest_sheet, new_shape.frame)?;

        if let (ShapeKind::Chart(ca), ShapeKind::Chart(cb)) = (&old_shape.kind, &new_shape.kind) {
            if ca != cb {
                let edit = chart_data.get(&shape_id).and_then(|edits| edits.iter().rev().find(|(c, _)| c.groups == cb.groups)).map(|(_, d)| d);
                // A chartEx chart: made from its cells; the settings in its XML are written when they changed.
                if ca.ex.is_some() && cb.ex.is_some() {
                    write_chart_ex(package, &map.charts[source_idx].chart, ca, cb)?;
                } else if cb.is_pivot_chart() && ca.groups != cb.groups {
                    super::pptx_chart_data::write_pivot_chart_series(package, &map.charts[source_idx].chart, cb)?;
                    let base = pivot_chart_base(ca, cb);
                    if base != **cb {
                        write_chart_settings(package, &map.charts[source_idx].chart, &base, cb)?;
                    }
                } else {
                match edit {
                    Some(data) if ca.groups != cb.groups || ca.cell_refs != cb.cell_refs => {
                        super::pptx_chart_data::write_sheet_chart_series(package, &map.charts[source_idx].chart, cb, &data.series)?;
                        let base = ibis_doc_model::Chart { groups: cb.groups.clone(), cell_refs: cb.cell_refs.clone(), ..(**ca).clone() };
                        if base != **cb {
                            write_chart_settings(package, &map.charts[source_idx].chart, &base, cb)?;
                        }
                    }
                    // The cells it reads changed (or it reads others): its series' caches written from them.
                    _ if super::pptx_chart_data::data_differs(ca, cb) || ca.cell_refs != cb.cell_refs => {
                        let refs = chart_refs_of(package, &map.charts[source_idx].chart, cb)?;
                        super::pptx_chart_data::write_sheet_chart_series(package, &map.charts[source_idx].chart, cb, &refs)?;
                        let base = ibis_doc_model::Chart { groups: cb.groups.clone(), cell_refs: cb.cell_refs.clone(), ..(**ca).clone() };
                        if base != **cb {
                            write_chart_settings(package, &map.charts[source_idx].chart, &base, cb)?;
                        }
                    }
                    _ => write_chart_settings(package, &map.charts[source_idx].chart, ca, cb)?,
                }
                }
            }
        }
    }

    Ok(moved_chart_ids)
}

/// Baseline chart model after `write_pivot_chart_series` has updated the `<c:ser>` elements' names, categories, and values.
fn pivot_chart_base(ca: &ibis_doc_model::Chart, cb: &ibis_doc_model::Chart) -> ibis_doc_model::Chart {
    let groups: Vec<_> = ca
        .groups
        .iter()
        .zip(&cb.groups)
        .map(|(ca_g, cb_g)| ibis_doc_model::ChartGroup {
            kind: ca_g.kind,
            three_d: ca_g.three_d,
            grouping: ca_g.grouping,
            of_pie: ca_g.of_pie.clone(),
            axes: cb_g.axes.clone(),
            series: cb_g
                .series
                .iter()
                .enumerate()
                .map(|(si, s)| {
                    let old_s = ca_g.series.get(si);
                    ibis_doc_model::Series {
                        fill: if matches!(s.fill, ibis_doc_model::Fill::None) {
                            old_s.map(|o| o.fill.clone()).unwrap_or(ibis_doc_model::Fill::None)
                        } else {
                            ibis_doc_model::Fill::None
                        },
                        stroke: old_s.and_then(|o| o.stroke.clone()),
                        labels: old_s.and_then(|o| o.labels.clone()),
                        marker: old_s.and_then(|o| o.marker.clone()),
                        trendline: old_s.and_then(|o| o.trendline.clone()),
                        error_bars: old_s.and_then(|o| o.error_bars.clone()),
                        ..s.clone()
                    }
                })
                .collect(),
        })
        .collect();
    ibis_doc_model::Chart {
        groups: if groups.len() == cb.groups.len() { groups } else { cb.groups.clone() },
        cell_refs: cb.cell_refs.clone(),
        ..ca.clone()
    }
}

/// A sheet's charts changed in the app: one moved or resized gets its
/// anchor in the drawing set anew; one whose title, legend or axes'
/// scale and gridlines changed gets them in its chart part. Any other
/// change to the sheet's shapes is not saved yet.
fn write_charts(package: &mut Package, sources: &[ChartSource], sheet: &Sheet, old: &[ibis_doc_model::Shape], new: &[ibis_doc_model::Shape], chart_data: &super::pptx_chart_data::ChartDataEdits) -> Result<(), Error> {
    if old.len() != new.len() || old.iter().zip(new).any(|(a, b)| a.id != b.id) {
        return Err(unsupported("シートの図形・グラフの追加・削除"));
    }
    for (a, b) in old.iter().zip(new) {
        if a == b {
            continue;
        }
        if let (ShapeKind::Auto(aa), ShapeKind::Auto(ab)) = (&a.kind, &b.kind) {
            let mut expected_a = aa.clone();
            expected_a.geometry = ab.geometry.clone();
            expected_a.text = ab.text.clone();
            if &expected_a == ab && (a.rotation, a.flip_h, a.flip_v, &a.floating, a.frame) == (b.rotation, b.flip_h, b.flip_v, &b.floating, b.frame) {
                continue;
            }
        }
        if let (ShapeKind::TextBox(_), ShapeKind::TextBox(_)) = (&a.kind, &b.kind) {
            if (a.rotation, a.flip_h, a.flip_v, &a.floating, a.frame) == (b.rotation, b.flip_h, b.flip_v, &b.floating, b.frame) {
                continue;
            }
        }
        if let (ShapeKind::TextBox(_), ShapeKind::Auto(_)) = (&a.kind, &b.kind) {
            if (a.rotation, a.flip_h, a.flip_v, &a.floating, a.frame) == (b.rotation, b.flip_h, b.flip_v, &b.floating, b.frame) {
                continue;
            }
        }
        if let (ShapeKind::Picture(pa), ShapeKind::Picture(pb)) = (&a.kind, &b.kind) {
            let mut expected_a = pa.clone();
            expected_a.clear = pb.clear;
            expected_a.duotone = pb.duotone;
            expected_a.luminance = pb.luminance;
            if &expected_a == pb && (a.rotation, a.flip_h, a.flip_v, &a.floating, a.frame) == (b.rotation, b.flip_h, b.flip_v, &b.floating, b.frame) {
                continue;
            }
        }
        // A form control's frame, state, properties, or text formatting changed: written apart in `write_control_changes`.
        if let (ShapeKind::Control(ca), ShapeKind::Control(cb)) = (&a.kind, &b.kind) {
            if (a.id, a.rotation, a.flip_h, a.flip_v, &a.floating) == (b.id, b.rotation, b.flip_h, b.flip_v, &b.floating) && ca.kind == cb.kind {
                continue;
            }
        }
        // A timeline's frame, selection, settings, or name changed: written apart in `slicers`.
        if let (ShapeKind::Timeline(ta), ShapeKind::Timeline(tb)) = (&a.kind, &b.kind) {
            if (a.id, a.rotation, a.flip_h, a.flip_v, &a.floating) == (b.id, b.rotation, b.flip_h, b.flip_v, &b.floating) && ta.cache == tb.cache && ta.source == tb.source {
                continue;
            }
        }
        // A slicer's selection, settings, or name changed: written apart in `slicers`.
        if let (ShapeKind::Slicer(sa), ShapeKind::Slicer(sb)) = (&a.kind, &b.kind) {
            let same_source_field = match (&sa.source, &sb.source) {
                (ibis_doc_model::slicer::SlicerSource::Table { table: ta, field: fa }, ibis_doc_model::slicer::SlicerSource::Table { table: tb, field: fb }) => ta == tb && fa == fb,
                (ibis_doc_model::slicer::SlicerSource::Pivot { field: fa, .. }, ibis_doc_model::slicer::SlicerSource::Pivot { field: fb, .. }) => fa == fb,
                _ => false,
            };
            if (a.id, a.rotation, a.flip_h, a.flip_v, &a.floating) == (b.id, b.rotation, b.flip_h, b.flip_v, &b.floating) && sa.cache == sb.cache && same_source_field {
                continue;
            }
        }
        let (ShapeKind::Chart(ca), ShapeKind::Chart(cb)) = (&a.kind, &b.kind) else {
            return Err(unsupported("シートの図形の変更"));
        };
        let Some(source) = sources.iter().find(|s| s.shape == b.id) else {
            return Err(unsupported("このグラフの変更"));
        };
        if (a.rotation, a.flip_h, a.flip_v, &a.floating) != (b.rotation, b.flip_h, b.flip_v, &b.floating) {
            return Err(unsupported("グラフの回転・反転"));
        }
        if a.frame != b.frame {
            write_anchor(package, source, sheet, b.frame)?;
        }
        if ca != cb {
            // A chart whose data range was changed (データ範囲・行列の切り替え): its series written from the references recorded with the edit, then the rest.
            let edit = chart_data.get(&b.id).and_then(|edits| edits.iter().rev().find(|(c, _)| c.groups == cb.groups)).map(|(_, d)| d);
            if ca.ex.is_some() && cb.ex.is_some() {
                write_chart_ex(package, &source.chart, ca, cb)?;
            } else if cb.is_pivot_chart() && ca.groups != cb.groups {
                super::pptx_chart_data::write_pivot_chart_series(package, &source.chart, cb)?;
                let base = pivot_chart_base(ca, cb);
                if base != **cb {
                    write_chart_settings(package, &source.chart, &base, cb)?;
                }
            } else {
            match edit {
                Some(data) if ca.groups != cb.groups || ca.cell_refs != cb.cell_refs => {
                    super::pptx_chart_data::write_sheet_chart_series(package, &source.chart, cb, &data.series)?;
                    let base = ibis_doc_model::Chart { groups: cb.groups.clone(), cell_refs: cb.cell_refs.clone(), ..(**ca).clone() };
                    if base != **cb {
                        write_chart_settings(package, &source.chart, &base, cb)?;
                    }
                }
                // The cells it reads changed (or it reads others): its series' caches written from them.
                _ if super::pptx_chart_data::data_differs(ca, cb) || ca.cell_refs != cb.cell_refs => {
                    let refs = chart_refs_of(package, &source.chart, cb)?;
                    super::pptx_chart_data::write_sheet_chart_series(package, &source.chart, cb, &refs)?;
                    let base = ibis_doc_model::Chart { groups: cb.groups.clone(), cell_refs: cb.cell_refs.clone(), ..(**ca).clone() };
                    if base != **cb {
                        write_chart_settings(package, &source.chart, &base, cb)?;
                    }
                }
                _ => write_chart_settings(package, &source.chart, ca, cb)?,
            }
            }
        }
    }
    Ok(())
}

const REL_CTRL_PROP: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/ctrlProp";
const CT_CTRL_PROP: &str = "application/vnd.ms-excel.controlproperties+xml";

fn control_object_type(kind: ibis_doc_model::ControlKind) -> &'static str {
    use ibis_doc_model::ControlKind as K;
    match kind {
        K::Button => "Button",
        K::CheckBox => "CheckBox",
        K::Radio => "Radio",
        K::Scroll => "Scroll",
        K::Spin => "Spin",
        K::Drop => "Drop",
        K::List => "List",
        K::GroupBox => "GBox",
        K::Label => "Label",
    }
}

/// Write a form control's properties and state into its `ctrlProp` part.
fn write_control_props(package: &mut Package, part: &str, c: &ibis_doc_model::FormControl) -> Result<(), Error> {
    use ibis_doc_model::ControlKind as K;
    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| Error::new("コントロールの XML が空です"))?;
    doc.set_attr(root, "objectType", control_object_type(c.kind));
    if matches!(c.kind, K::Button | K::CheckBox | K::Radio | K::Label) {
        doc.set_attr(root, "lockText", "1");
    }
    match c.link.as_deref().filter(|s| !s.is_empty()) {
        Some(link) => doc.set_attr(root, "fmlaLink", link),
        None => doc.remove_attr(root, "fmlaLink"),
    }
    match c.range.as_deref().filter(|s| !s.is_empty()) {
        Some(range) => doc.set_attr(root, "fmlaRange", range),
        None => doc.remove_attr(root, "fmlaRange"),
    }
    match c.kind {
        K::CheckBox | K::Radio => match c.checked {
            0 => doc.remove_attr(root, "checked"),
            2 => doc.set_attr(root, "checked", "Mixed"),
            _ => doc.set_attr(root, "checked", "Checked"),
        },
        K::Scroll | K::Spin => {
            doc.set_attr(root, "dx", "15");
            if c.min != 0 {
                doc.set_attr(root, "min", &c.min.to_string());
            } else {
                doc.remove_attr(root, "min");
            }
            doc.set_attr(root, "max", &c.max.to_string());
            if c.step != 1 {
                doc.set_attr(root, "inc", &c.step.to_string());
            } else {
                doc.remove_attr(root, "inc");
            }
            doc.set_attr(root, "page", &c.page.to_string());
            doc.set_attr(root, "val", &c.value.to_string());
            if c.kind == K::Scroll {
                if c.horizontal {
                    doc.set_attr(root, "horiz", "1");
                } else {
                    doc.remove_attr(root, "horiz");
                }
            }
        }
        K::Drop | K::List => {
            doc.set_attr(root, "dx", "15");
            if c.kind == K::Drop {
                doc.set_attr(root, "dropStyle", "comboedit");
            }
            doc.set_attr(root, "sel", &c.value.to_string());
            doc.set_attr(root, "val", &c.value.to_string());
        }
        _ => {}
    }
    if c.kind == K::Radio {
        if c.first_button {
            doc.set_attr(root, "firstButton", "1");
        } else {
            doc.remove_attr(root, "firstButton");
        }
    }
    if matches!(c.kind, K::CheckBox | K::Radio | K::Drop | K::List | K::GroupBox) {
        if c.three_d {
            doc.remove_attr(root, "noThreeD");
        } else {
            doc.set_attr(root, "noThreeD", "1");
        }
    }
    Ok(())
}

fn set_control_tx_body(doc: &mut XmlDocument, sp: NodeId, c: &ibis_doc_model::FormControl) {
    use ibis_doc_model::ControlKind as K;
    if let Some(old_tx) = doc.child(sp, "txBody") {
        doc.detach(old_tx);
    }
    if c.text.is_empty() && !matches!(c.kind, K::Button | K::CheckBox | K::Radio | K::GroupBox | K::Label) {
        return;
    }
    let sp_prefix = doc.prefix(sp).map_or(String::new(), |p| format!("{p}:"));
    let tx_body = doc.append_element(sp, &format!("{sp_prefix}txBody"));
    let body_pr = doc.append_element(tx_body, "a:bodyPr");
    doc.set_attr(body_pr, "vertOverflow", "clip");
    doc.set_attr(body_pr, "wrap", if c.kind == K::GroupBox { "none" } else { "square" });
    doc.set_attr(body_pr, "lIns", "27432");
    doc.set_attr(body_pr, "tIns", "22860");
    doc.set_attr(body_pr, "rIns", if c.kind == K::Button { "27432" } else { "0" });
    doc.set_attr(body_pr, "bIns", if matches!(c.kind, K::GroupBox | K::Label) { "0" } else { "22860" });
    doc.set_attr(body_pr, "anchor", if matches!(c.kind, K::GroupBox | K::Label) { "t" } else { "ctr" });
    doc.set_attr(body_pr, "upright", "1");
    doc.append_element(tx_body, "a:lstStyle");
    let p = doc.append_element(tx_body, "a:p");
    let p_pr = doc.append_element(p, "a:pPr");
    doc.set_attr(p_pr, "algn", if c.kind == K::Button { "ctr" } else { "l" });
    doc.set_attr(p_pr, "rtl", "0");
    let def_rpr = doc.append_element(p_pr, "a:defRPr");
    doc.set_attr(def_rpr, "sz", "1000");
    let r = doc.append_element(p, "a:r");
    let r_pr = doc.append_element(r, "a:rPr");
    doc.set_attr(r_pr, "lang", "ja-JP");
    doc.set_attr(r_pr, "altLang", "en-US");
    doc.set_attr(r_pr, "sz", &c.font_size.unwrap_or(1100).to_string());
    doc.set_attr(r_pr, "b", if c.bold { "1" } else { "0" });
    doc.set_attr(r_pr, "i", if c.italic { "1" } else { "0" });
    doc.set_attr(r_pr, "u", "none");
    doc.set_attr(r_pr, "strike", "noStrike");
    doc.set_attr(r_pr, "baseline", "0");
    let sf = doc.append_element(r_pr, "a:solidFill");
    let srgb = doc.append_element(sf, "a:srgbClr");
    doc.set_attr(srgb, "val", &format!("{:06X}", c.text_color.map_or(0x000000, |col| col.0)));
    if let Some(font) = c.font.as_deref().filter(|s| !s.is_empty()) {
        let latin = doc.append_element(r_pr, "a:latin");
        doc.set_attr(latin, "typeface", font);
        let ea = doc.append_element(r_pr, "a:ea");
        doc.set_attr(ea, "typeface", font);
    }
    let t = doc.append_element(r, "a:t");
    doc.set_text(t, &c.text);
}

fn update_two_cell_corners(doc: &mut XmlDocument, anchor: NodeId, sheet: &Sheet, frame: ibis_doc_model::EmuRect) {
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let cell = |offsets: &[i64], v: i64| -> (usize, i64) {
        let i = offsets.partition_point(|&o| o <= v).saturating_sub(1).min(offsets.len().saturating_sub(2));
        (i, v - offsets.get(i).copied().unwrap_or(0))
    };
    for (corner_name, x, y) in [("from", frame.x, frame.y), ("to", frame.x + frame.width, frame.y + frame.height)] {
        let Some(corner) = doc.child(anchor, corner_name) else { continue };
        let ((c, co), (r, ro)) = (cell(&xs, x), cell(&ys, y));
        for (tag, val) in [("col", c as i64), ("colOff", co), ("row", r as i64), ("rowOff", ro)] {
            if let Some(el) = doc.child(corner, tag) {
                doc.set_text(el, &val.to_string());
            } else {
                let el = doc.append_element(corner, &format!("xdr:{tag}"));
                doc.set_text(el, &val.to_string());
            }
        }
    }
}

/// Write changes to existing form controls on `sheet_part` (state, properties, frame, and text formatting).
fn write_control_changes(
    package: &mut Package,
    sheet_part: &str,
    sheet: &Sheet,
    old: &[ibis_doc_model::Shape],
    new: &[ibis_doc_model::Shape],
) -> Result<(), Error> {
    let rels = package.relationships(sheet_part)?;
    let drawing_part = rels
        .iter()
        .find(|r| r.kind == REL_DRAWING && !r.external)
        .and_then(|r| r.target_part(sheet_part));
    for (a, b) in old.iter().zip(new) {
        let (ShapeKind::Control(ca), ShapeKind::Control(cb)) = (&a.kind, &b.kind) else { continue };
        if a.frame == b.frame && ca == cb {
            continue;
        }
        if !cb.part.is_empty() && ca != cb {
            write_control_props(package, &cb.part, cb)?;
        }
        let rid = rels
            .iter()
            .find(|r| !r.external && r.target_part(sheet_part).as_deref() == Some(cb.part.as_str()))
            .map(|r| r.id.clone());
        let mut shape_id_found: Option<String> = None;
        {
            let sheet_doc = package.xml_mut(sheet_part)?;
            if let Some(root) = sheet_doc.root_element() {
                let ctrl_node = sheet_doc.descendants(root).into_iter().find(|&n| {
                    sheet_doc.local_name(n) == "control"
                        && ((rid.is_some() && sheet_doc.attr_local(n, "id") == rid.as_deref())
                            || sheet_doc.attr(n, "name") == Some(ca.name.as_str()))
                });
                if let Some(ctrl_node) = ctrl_node {
                    shape_id_found = sheet_doc.attr(ctrl_node, "shapeId").map(str::to_string);
                    if ca.name != cb.name {
                        sheet_doc.set_attr(ctrl_node, "name", &cb.name);
                    }
                    if a.frame != b.frame {
                        if let Some(anchor) = sheet_doc.descendants(ctrl_node).into_iter().find(|&n| sheet_doc.local_name(n) == "anchor") {
                            update_two_cell_corners(sheet_doc, anchor, sheet, b.frame);
                        }
                    }
                }
            }
        }
        let text_diff = (&ca.name, &ca.text, &ca.font, ca.font_size, ca.bold, ca.italic, ca.text_color)
            != (&cb.name, &cb.text, &cb.font, cb.font_size, cb.bold, cb.italic, cb.text_color);
        if (a.frame != b.frame || text_diff)
            && let (Some(drawing), Some(shape_id)) = (drawing_part.as_deref(), shape_id_found.as_deref())
            && package.contains(drawing)
        {
            let draw_doc = package.xml_mut(drawing)?;
            if let Some(root) = draw_doc.root_element() {
                let cnv = draw_doc.descendants(root).into_iter().find(|&n| {
                    draw_doc.local_name(n) == "cNvPr" && draw_doc.attr(n, "id") == Some(shape_id)
                });
                if let Some(cnv) = cnv {
                    if ca.name != cb.name {
                        draw_doc.set_attr(cnv, "name", &cb.name);
                    }
                    let mut p = draw_doc.parent(cnv);
                    let mut sp_node = None;
                    let mut anchor_node = None;
                    while let Some(curr) = p {
                        match draw_doc.local_name(curr) {
                            "sp" if sp_node.is_none() => sp_node = Some(curr),
                            "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor" => {
                                anchor_node = Some(curr);
                                break;
                            }
                            _ => {}
                        }
                        p = draw_doc.parent(curr);
                    }
                    if a.frame != b.frame
                        && let Some(anchor) = anchor_node
                    {
                        update_two_cell_corners(draw_doc, anchor, sheet, b.frame);
                    }
                    if text_diff
                        && let Some(sp) = sp_node
                    {
                        set_control_tx_body(draw_doc, sp, cb);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Add a newly inserted form control to `sheet_part`, its drawing, and a new `xl/ctrlProps/ctrlPropN.xml` part.
pub(super) fn add_control(
    package: &mut Package,
    sheet_part: &str,
    sheet: &Sheet,
    shape: &ibis_doc_model::Shape,
    c: &ibis_doc_model::FormControl,
) -> Result<(), Error> {
    let drawing = match package.relationships(sheet_part)?.into_iter().find(|r| r.kind == REL_DRAWING && !r.external) {
        Some(rel) => rel.target_part(sheet_part).ok_or_else(|| unsupported("シートの描画パート"))?,
        None => create_drawing(package, sheet_part)?,
    };
    let mut max_id = 1024u32;
    if let Ok(sheet_doc) = package.xml(sheet_part)
        && let Some(root) = sheet_doc.root_element()
    {
        for n in sheet_doc.descendants(root) {
            if sheet_doc.local_name(n) == "control"
                && let Some(id) = sheet_doc.attr(n, "shapeId").and_then(|v| v.parse::<u32>().ok())
            {
                max_id = max_id.max(id);
            }
        }
    }
    if let Ok(draw_doc) = package.xml(&drawing)
        && let Some(root) = draw_doc.root_element()
    {
        for n in draw_doc.descendants(root) {
            if draw_doc.local_name(n) == "cNvPr"
                && let Some(id) = draw_doc.attr(n, "id").and_then(|v| v.parse::<u32>().ok())
            {
                max_id = max_id.max(id);
            }
        }
    }
    let shape_id = (max_id + 1).to_string();

    // 1. Append hidden shape with txBody to the sheet's drawing.
    {
        let draw_doc = package.xml_mut(&drawing)?;
        let root = draw_doc.root_element().ok_or_else(|| Error::new("描画の XML が空です"))?;
        if draw_doc.lookup_prefix(root, "http://schemas.openxmlformats.org/drawingml/2006/main").is_none() {
            draw_doc.set_attr(root, "xmlns:a", "http://schemas.openxmlformats.org/drawingml/2006/main");
        }
        let anchor = draw_doc.append_element(root, "xdr:twoCellAnchor");
        draw_doc.set_attr(anchor, "editAs", "oneCell");
        draw_doc.append_element(anchor, "xdr:from");
        draw_doc.append_element(anchor, "xdr:to");
        update_two_cell_corners(draw_doc, anchor, sheet, shape.frame);
        let sp = draw_doc.append_element(anchor, "xdr:sp");
        draw_doc.set_attr(sp, "macro", "");
        draw_doc.set_attr(sp, "textlink", "");
        let nv_sp_pr = draw_doc.append_element(sp, "xdr:nvSpPr");
        let c_nv_pr = draw_doc.append_element(nv_sp_pr, "xdr:cNvPr");
        draw_doc.set_attr(c_nv_pr, "id", &shape_id);
        draw_doc.set_attr(c_nv_pr, "name", &c.name);
        draw_doc.set_attr(c_nv_pr, "hidden", "1");
        draw_doc.append_element(nv_sp_pr, "xdr:cNvSpPr");
        let sp_pr = draw_doc.append_element(sp, "xdr:spPr");
        draw_doc.set_attr(sp_pr, "bwMode", "auto");
        let xfrm = draw_doc.append_element(sp_pr, "a:xfrm");
        let off = draw_doc.append_element(xfrm, "a:off");
        draw_doc.set_attr(off, "x", "0");
        draw_doc.set_attr(off, "y", "0");
        let ext = draw_doc.append_element(xfrm, "a:ext");
        draw_doc.set_attr(ext, "cx", "0");
        draw_doc.set_attr(ext, "cy", "0");
        let prst = draw_doc.append_element(sp_pr, "a:prstGeom");
        draw_doc.set_attr(prst, "prst", "rect");
        draw_doc.append_element(prst, "a:avLst");
        draw_doc.append_element(sp_pr, "a:noFill");
        set_control_tx_body(draw_doc, sp, c);
        draw_doc.append_element(anchor, "xdr:clientData");
    }

    // 2. Create `xl/ctrlProps/ctrlPropN.xml` and relationship on `sheet_part`.
    let ctrl_part = if !c.part.is_empty() && !package.contains(&c.part) {
        c.part.clone()
    } else {
        (1..)
            .map(|n| format!("xl/ctrlProps/ctrlProp{n}.xml"))
            .find(|p| !package.contains(p))
            .unwrap_or_else(|| "xl/ctrlProps/ctrlProp1.xml".to_string())
    };
    let mut ctrl_doc = XmlDocument::new_with_declaration();
    let ctrl_root = ctrl_doc.append_element(XmlDocument::ROOT, "formControlPr");
    ctrl_doc.set_attr(ctrl_root, "xmlns", "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main");
    package.set_xml(&ctrl_part, ctrl_doc);
    write_control_props(package, &ctrl_part, c)?;
    package.set_override(&ctrl_part, CT_CTRL_PROP)?;
    let rel_target = super::pptx::relative(sheet_part, &ctrl_part);
    let rid = package.add_relationship(sheet_part, REL_CTRL_PROP, &rel_target, false)?;

    // 3. Append `<control>` to `<controls>` in `sheet_part`.
    let sheet_doc = package.xml_mut(sheet_part)?;
    let root = sheet_doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    if sheet_doc.lookup_prefix(root, REL_NS).is_none() {
        sheet_doc.set_attr(root, "xmlns:r", REL_NS);
    }
    if sheet_doc.lookup_prefix(root, "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing").is_none() {
        sheet_doc.set_attr(root, "xmlns:xdr", "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing");
    }
    let prefix = prefix_of(sheet_doc, root);
    let controls_el = match sheet_doc.descendants(root).into_iter().find(|&n| sheet_doc.local_name(n) == "controls") {
        Some(el) => el,
        None => {
            let el = detached(sheet_doc, &format!("{prefix}controls"));
            let before = ["webPublishItems", "tableParts", "extLst"];
            let anchor = sheet_doc.elements(root).find(|&c| before.contains(&sheet_doc.local_name(c)));
            match anchor {
                Some(a) => sheet_doc.insert_before(a, el),
                None => {
                    let count = sheet_doc.children(root).len();
                    sheet_doc.insert(root, count, el);
                }
            }
            el
        }
    };
    let ctrl = sheet_doc.append_element(controls_el, &format!("{prefix}control"));
    sheet_doc.set_attr(ctrl, "shapeId", &shape_id);
    sheet_doc.set_attr(ctrl, "r:id", &rid);
    sheet_doc.set_attr(ctrl, "name", &c.name);
    let ctrl_pr = sheet_doc.append_element(ctrl, &format!("{prefix}controlPr"));
    sheet_doc.set_attr(ctrl_pr, "defaultSize", "0");
    sheet_doc.set_attr(ctrl_pr, "autoFill", "0");
    sheet_doc.set_attr(ctrl_pr, "autoPict", "0");
    let anchor = sheet_doc.append_element(ctrl_pr, &format!("{prefix}anchor"));
    sheet_doc.set_attr(anchor, "moveWithCells", "1");
    sheet_doc.set_attr(anchor, "sizeWithCells", "1");
    sheet_doc.append_element(anchor, &format!("{prefix}from"));
    sheet_doc.append_element(anchor, &format!("{prefix}to"));
    update_two_cell_corners(sheet_doc, anchor, sheet, shape.frame);
    Ok(())
}

/// Remove deleted form controls from `sheet_part`, its drawing, relationships, and `ctrlProps`.
fn write_control_removals(
    package: &mut Package,
    sheet_part: &str,
    gone: &[ibis_doc_model::FormControl],
) -> Result<(), Error> {
    if gone.is_empty() {
        return Ok(());
    }
    let rels = package.relationships(sheet_part)?;
    let drawing_part = rels
        .iter()
        .find(|r| r.kind == REL_DRAWING && !r.external)
        .and_then(|r| r.target_part(sheet_part));
    for c in gone {
        let rid = rels
            .iter()
            .find(|r| !r.external && !c.part.is_empty() && r.target_part(sheet_part).as_deref() == Some(c.part.as_str()))
            .map(|r| r.id.clone());
        let mut shape_id_found: Option<String> = None;
        {
            let sheet_doc = package.xml_mut(sheet_part)?;
            if let Some(root) = sheet_doc.root_element() {
                let ctrl_node = sheet_doc.descendants(root).into_iter().find(|&n| {
                    sheet_doc.local_name(n) == "control"
                        && ((rid.is_some() && sheet_doc.attr_local(n, "id") == rid.as_deref())
                            || sheet_doc.attr(n, "name") == Some(c.name.as_str()))
                });
                if let Some(ctrl_node) = ctrl_node {
                    shape_id_found = sheet_doc.attr(ctrl_node, "shapeId").map(str::to_string);
                    let parent = sheet_doc.parent(ctrl_node);
                    sheet_doc.detach(ctrl_node);
                    if let Some(choice) = parent
                        && sheet_doc.local_name(choice) == "Choice"
                        && sheet_doc.elements(choice).next().is_none()
                        && let Some(ac) = sheet_doc.parent(choice)
                        && sheet_doc.local_name(ac) == "AlternateContent"
                    {
                        sheet_doc.detach(ac);
                    }
                }
                if let Some(controls_el) = sheet_doc.descendants(root).into_iter().find(|&n| sheet_doc.local_name(n) == "controls") {
                    let has_remaining = sheet_doc.descendants(controls_el).into_iter().any(|n| sheet_doc.local_name(n) == "control");
                    if !has_remaining {
                        let parent = sheet_doc.parent(controls_el);
                        sheet_doc.detach(controls_el);
                        if let Some(choice) = parent
                            && sheet_doc.local_name(choice) == "Choice"
                            && sheet_doc.elements(choice).next().is_none()
                            && let Some(ac) = sheet_doc.parent(choice)
                            && sheet_doc.local_name(ac) == "AlternateContent"
                        {
                            sheet_doc.detach(ac);
                        }
                    }
                }
            }
        }
        if let (Some(drawing), Some(shape_id)) = (drawing_part.as_deref(), shape_id_found.as_deref())
            && package.contains(drawing)
        {
            let draw_doc = package.xml_mut(drawing)?;
            if let Some(root) = draw_doc.root_element() {
                let cnv = draw_doc.descendants(root).into_iter().find(|&n| {
                    draw_doc.local_name(n) == "cNvPr" && draw_doc.attr(n, "id") == Some(shape_id)
                });
                let mut at = cnv;
                while let Some(curr) = at {
                    if matches!(draw_doc.local_name(curr), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor") {
                        let parent = draw_doc.parent(curr);
                        draw_doc.detach(curr);
                        if let Some(choice) = parent
                            && draw_doc.local_name(choice) == "Choice"
                            && draw_doc.elements(choice).next().is_none()
                            && let Some(ac) = draw_doc.parent(choice)
                            && draw_doc.local_name(ac) == "AlternateContent"
                        {
                            draw_doc.detach(ac);
                        }
                        break;
                    }
                    at = draw_doc.parent(curr);
                }
            }
        }
        if let Some(rid) = rid {
            package.remove_relationship(sheet_part, &rid)?;
        }
        if !c.part.is_empty() && package.contains(&c.part) {
            package.remove_override(&c.part)?;
            package.remove(&c.part);
        }
    }
    Ok(())
}

/// The graphic frame's anchor in the drawing, to cover `frame`: its
/// corners' cells and offsets (`xdr:from`, `xdr:to`), or its extent.
fn write_anchor(package: &mut Package, source: &ChartSource, sheet: &Sheet, frame: ibis_doc_model::EmuRect) -> Result<(), Error> {
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let cell = |offsets: &[i64], v: i64| -> (usize, i64) {
        let i = offsets.partition_point(|&o| o <= v).saturating_sub(1).min(offsets.len().saturating_sub(2));
        (i, v - offsets.get(i).copied().unwrap_or(0))
    };
    let doc = package.xml_mut(&source.drawing)?;
    let root = doc.root_element().ok_or_else(|| Error::new("描画の XML が空です"))?;
    let frame_node = doc.descendants(root).into_iter().find(|&n| {
        doc.local_name(n) == "graphicFrame" && doc.path(n, &["nvGraphicFramePr", "cNvPr"]).and_then(|c| doc.attr(c, "id")) == source.frame_id.as_deref()
    });
    let anchor = frame_node.and_then(|n| doc.parent(n)).filter(|&a| matches!(doc.local_name(a), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor"));
    let Some(anchor) = anchor else { return Err(unsupported("このグラフの位置")) };
    let set = |doc: &mut XmlDocument, corner: NodeId, name: &str, value: i64| {
        if let Some(e) = doc.child(corner, name) {
            doc.set_text(e, &value.to_string());
        }
    };
    let ext_of = |doc: &mut XmlDocument, a: NodeId| {
        if let Some(e) = doc.child(a, "ext") {
            doc.set_attr(e, "cx", &frame.width.to_string());
            doc.set_attr(e, "cy", &frame.height.to_string());
        }
    };
    match doc.local_name(anchor).to_string().as_str() {
        "absoluteAnchor" => {
            if let Some(p) = doc.child(anchor, "pos") {
                doc.set_attr(p, "x", &frame.x.to_string());
                doc.set_attr(p, "y", &frame.y.to_string());
            }
            ext_of(doc, anchor);
        }
        kind => {
            let corners: Vec<(&str, i64, i64)> = if kind == "twoCellAnchor" { vec![("from", frame.x, frame.y), ("to", frame.x + frame.width, frame.y + frame.height)] } else { vec![("from", frame.x, frame.y)] };
            for (name, x, y) in corners {
                let Some(corner) = doc.child(anchor, name) else { continue };
                let ((c, co), (r, ro)) = (cell(&xs, x), cell(&ys, y));
                set(doc, corner, "col", c as i64);
                set(doc, corner, "colOff", co);
                set(doc, corner, "row", r as i64);
                set(doc, corner, "rowOff", ro);
            }
            if kind == "oneCellAnchor" {
                ext_of(doc, anchor);
            }
        }
    }
    // The frame's own size (`xfrm`), which Excel keeps as the anchor's.
    if let Some(x) = frame_node.and_then(|n| doc.child(n, "xfrm"))
        && let Some(e) = doc.child(x, "ext")
    {
        doc.set_attr(e, "cx", &frame.width.to_string());
        doc.set_attr(e, "cy", &frame.height.to_string());
    }
    Ok(())
}

fn write_chart_title_text(doc: &mut XmlDocument, title: NodeId, text: &str, a: &str, c: &str) {
    let rich = match doc.path(title, &["tx", "rich"]) {
        Some(r) => r,
        None => {
            if let Some(tx) = doc.child(title, "tx") {
                doc.detach(tx);
            }
            let tx = detached(doc, &format!("{c}tx"));
            let rich = detached(doc, &format!("{c}rich"));
            for (i, name) in ["bodyPr", "lstStyle"].iter().enumerate() {
                let e = detached(doc, &format!("{a}:{name}"));
                doc.insert(rich, i, e);
            }
            doc.insert(tx, 0, rich);
            doc.insert(title, 0, tx);
            rich
        }
    };
    let paragraphs: Vec<NodeId> = doc.children_named(rich, "p").collect();
    let model = match paragraphs.first() {
        Some(&p) => {
            let copy = doc.deep_clone(p);
            let runs: Vec<NodeId> = doc.elements(copy).filter(|&x| matches!(doc.local_name(x), "r" | "fld" | "br")).collect();
            for (i, r) in runs.into_iter().enumerate() {
                if i > 0 || doc.local_name(r) != "r" {
                    doc.detach(r);
                }
            }
            copy
        }
        None => detached(doc, &format!("{a}:p")),
    };
    for p in paragraphs {
        doc.detach(p);
    }
    for line in text.split('\n') {
        let p = doc.deep_clone(model);
        let run = match doc.child(p, "r") {
            Some(r) => r,
            None => {
                let r = detached(doc, &format!("{a}:r"));
                let n = doc.children(p).len();
                doc.insert(p, n, r);
                r
            }
        };
        let t_el = match doc.child(run, "t") {
            Some(x) => x,
            None => {
                let x = detached(doc, &format!("{a}:t"));
                let n = doc.children(run).len();
                doc.insert(run, n, x);
                x
            }
        };
        doc.set_text(t_el, line);
        let n = doc.children(rich).len();
        doc.insert(rich, n, p);
    }
}

fn built_in_unit_name(unit: f64) -> Option<&'static str> {
    match unit.round() as i64 {
        100 => Some("hundreds"),
        1_000 => Some("thousands"),
        10_000 => Some("tenThousands"),
        100_000 => Some("hundredThousands"),
        1_000_000 => Some("millions"),
        10_000_000 => Some("tenMillions"),
        100_000_000 => Some("hundredMillions"),
        1_000_000_000 => Some("billions"),
        1_000_000_000_000 => Some("trillions"),
        _ => None,
    }
}

/// What the chart panel changes in a chart (`ibis-office-app`'s
/// `xlsx/chart_edit.rs`) written into its part: the title, the legend, and
/// the axes' minimum, maximum, major unit and major gridlines. Any other
/// change is refused.
/// A chartEx chart changed: its part's XML written again when its source has other settings; what it shows follows its
/// cells and is not in the file. Title, legend, and ex_format changes are written via `pptx_chart_ex::write_chart_format`.
fn write_chart_ex(package: &mut Package, part: &str, old: &ibis_doc_model::Chart, new: &ibis_doc_model::Chart) -> Result<(), Error> {
    if old.ex_source != new.ex_source
        || old.ex_source.as_ref().map(|s| s.0.xml()) != new.ex_source.as_ref().map(|s| s.0.xml())
    {
        if let Some(source) = &new.ex_source {
            let doc = ibis_ooxml::XmlDocument::parse(source.0.xml().as_bytes()).map_err(|_| unsupported("グラフの設定"))?;
            *package.xml_mut(part)? = doc;
        }
    }
    let base = ibis_doc_model::Chart {
        groups: new.groups.clone(),
        axes: new.axes.clone(),
        ex: new.ex.clone(),
        ex_source: new.ex_source.clone(),
        cell_refs: new.cell_refs.clone(),
        ..old.clone()
    };
    super::pptx_chart_ex::write_chart_format(package, part, &base, new, false)
}

/// The references the series of `chart` read: its own, or (when it no longer follows the cells) those of its part.
fn chart_refs_of(package: &Package, part: &str, chart: &ibis_doc_model::Chart) -> Result<Vec<super::pptx_chart_data::SeriesRefs>, Error> {
    if !chart.cell_refs.is_empty() {
        return Ok(super::pptx_chart_data::refs_of(chart));
    }
    Ok(super::pptx_chart_data::chart_refs(&package.xml(part)?))
}

pub(super) fn write_chart_settings(package: &mut Package, part: &str, old: &ibis_doc_model::Chart, new: &ibis_doc_model::Chart) -> Result<(), Error> {
    if super::chart_topology::differs(old, new) {
        super::chart_topology::sync(package.xml_mut(part)?, new)?;
        let base = super::chart_topology::baseline(old, new);
        return write_chart_settings(package, part, &base, new);
    }

    let mut expected = old.clone();
    expected.title = new.title.clone();
    expected.background=new.background.clone();expected.border=new.border.clone();expected.plot_border=new.plot_border.clone();
    expected.pivot_source = new.pivot_source.clone();
    expected.legend = match (&old.legend, &new.legend) {
        (Some(o), Some(n)) => Some(ibis_doc_model::Legend {
            source_position: n.source_position,
            side: n.side,
            layout: n.layout.filter(|_| n.layout == o.layout),
            overlay: o.overlay,
            text: n.text.clone(),
            hidden: n.hidden.clone(),
        }),
        (None, Some(n)) if n.overlay == Some(false) && n.layout.is_none() => Some(n.clone()),
        // A new legend otherwise, or none: what is written.
        (None, Some(_)) | (_, None) => None,
    };
    if old.axes.len() != new.axes.len() {
        return Err(unsupported("グラフの軸の追加・削除"));
    }
    for (e, n) in expected.axes.iter_mut().zip(&new.axes) {
        e.text=n.text.clone();e.line=n.line.clone();
        e.min = n.min;
        e.max = n.max;
        e.major_unit = n.major_unit;
        e.minor_unit = n.minor_unit;
        e.display_unit = n.display_unit;
        e.log_base = n.log_base;
        e.gridlines = n.gridlines.clone();
        e.minor_gridlines = n.minor_gridlines.clone();
        e.title = n.title.clone();
        e.reversed = n.reversed;
        e.crosses = n.crosses;
        e.crosses_at = n.crosses_at;
        e.date = n.date;
        e.source_kind = n.source_kind;
    }
    if old.groups.len() != new.groups.len() {
        return Err(unsupported("グラフの系列グループの変更"));
    }
    for (eg, ng) in expected.groups.iter_mut().zip(&new.groups) {
        if old.groups.len() == 1 && (eg.kind != ng.kind || eg.grouping != ng.grouping || eg.three_d != ng.three_d) {
            eg.kind = ng.kind;
            eg.grouping = ng.grouping;
        }
        eg.three_d = ng.three_d;
        if let (ibis_doc_model::ChartType::Bar { horizontal: h1, .. }, ibis_doc_model::ChartType::Bar { horizontal: h2, gap_width: g2, overlap: o2 }) = (&eg.kind, &ng.kind) {
            if h1 == h2 {
                eg.kind = ibis_doc_model::ChartType::Bar { horizontal: *h1, gap_width: *g2, overlap: *o2 };
            }
        }
        if let (ibis_doc_model::ChartType::Radar { .. }, ibis_doc_model::ChartType::Radar { style: n_style }) = (&eg.kind, &ng.kind) {
            eg.kind = ibis_doc_model::ChartType::Radar { style: *n_style };
        }
        if let (ibis_doc_model::ChartType::OfPie { .. }, ibis_doc_model::ChartType::OfPie { kind: n_kind }) = (&eg.kind, &ng.kind) {
            eg.kind = ibis_doc_model::ChartType::OfPie { kind: *n_kind };
            if let (Some(eo), Some(no)) = (eg.of_pie.as_mut(), ng.of_pie.as_ref()) {
                eo.split = no.split;
                eo.split_position = no.split_position;
                eo.second_size = no.second_size;
                eo.gap_width = no.gap_width;
            } else if eg.of_pie.is_none() {
                eg.of_pie = ng.of_pie.clone();
            }
        }
        if let (ibis_doc_model::ChartType::Stock { .. }, ibis_doc_model::ChartType::Stock { up_down_bars, hi_low_lines, drop_lines }) = (&eg.kind, &ng.kind) {
            eg.kind = ibis_doc_model::ChartType::Stock { up_down_bars: *up_down_bars, hi_low_lines: *hi_low_lines, drop_lines: *drop_lines };
        }
        if let (ibis_doc_model::ChartType::Surface { .. }, ibis_doc_model::ChartType::Surface { is_3d, wireframe }) = (&eg.kind, &ng.kind) {
            eg.kind = ibis_doc_model::ChartType::Surface { is_3d: *is_3d, wireframe: *wireframe };
        }
        if eg.series.len() != ng.series.len() {
            return Err(unsupported("グラフの系列数の変更"));
        }
        for (es, ns) in eg.series.iter_mut().zip(&ng.series) {
            es.fill = ns.fill.clone();
            es.labels = ns.labels.clone();
            es.marker = ns.marker.clone();
            es.stroke = ns.stroke.clone();
            es.trendline = ns.trendline.clone();
            es.error_bars = ns.error_bars.clone();
        }
    }
    expected.cell_refs = new.cell_refs.clone();
    expected.disp_blanks_as = new.disp_blanks_as;
    expected.plot_vis_only = new.plot_vis_only;
    expected.data_table = new.data_table;
    expected.style = new.style;
    expected.plot_background = new.plot_background.clone();
    if &expected != new {
        return Err(unsupported("このグラフの変更"));
    }
    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| Error::new("グラフの XML が空です"))?;
    let c = doc.lookup_prefix(root, CHART_NS).map(|p| if p.is_empty() { String::new() } else { format!("{p}:") }).unwrap_or_else(|| "c:".into());
    let a = match doc.lookup_prefix(root, DRAWING_NS) {
        Some(p) => p,
        None => {
            doc.set_attr(root, "xmlns:a", DRAWING_NS);
            "a".into()
        }
    };
    let chart = doc.child(root, "chart").ok_or_else(|| unsupported("グラフの XML"))?;
    if old.background!=new.background || old.border!=new.border {super::chart_format::area(doc,root,Some(&new.background),Some(new.border.as_ref()),&c,&a,&["txPr","externalData","printSettings","userShapes","extLst"])?;}
    let new_el = |doc: &mut XmlDocument, name: &str| detached(doc, &format!("{c}{name}"));
    let valued = |doc: &mut XmlDocument, name: &str, value: &str| {
        let e = detached(doc, &format!("{c}{name}"));
        doc.set_attr(e, "val", value);
        e
    };
    // Put `e` among `parent`'s children before the first of `before`.
    let place = |doc: &mut XmlDocument, parent: NodeId, e: NodeId, before: &[&str]| {
        let at = doc.elements(parent).find(|&x| before.contains(&doc.local_name(x)));
        match at {
            Some(x) => doc.insert_before(x, e),
            None => {
                let n = doc.children(parent).len();
                doc.insert(parent, n, e);
            }
        }
    };
    if old.style != new.style {
        let existing = doc.child(root, "style");
        let alt_content = doc.elements(root).find(|&x| doc.local_name(x) == "AlternateContent");
        match new.style {
            Some(style_id) => {
                if let Some(alt) = alt_content {
                    doc.detach(alt);
                }
                match existing {
                    Some(node) => doc.set_attr(node, "val", &style_id.to_string()),
                    None => {
                        let node = valued(doc, "style", &style_id.to_string());
                        place(doc, root, node, &["clrMapOvr", "pivotSource", "protection", "chart", "spPr", "txPr", "extLst"]);
                    }
                }
            }
            None => {
                if let Some(node) = existing {
                    doc.detach(node);
                }
                if let Some(alt) = alt_content {
                    doc.detach(alt);
                }
            }
        }
    }
    if old.pivot_source != new.pivot_source {
        let existing = doc.child(root, "pivotSource").or_else(|| doc.child(chart, "pivotSource"));
        match &new.pivot_source {
            Some(name) => {
                let ps = match existing {
                    Some(node) => node,
                    None => {
                        let node = new_el(doc, "pivotSource");
                        place(doc, root, node, &["protection", "chart", "spPr", "txPr", "extLst"]);
                        node
                    }
                };
                let name_el = match doc.child(ps, "name") {
                    Some(n) => n,
                    None => {
                        let n = new_el(doc, "name");
                        doc.insert(ps, 0, n);
                        n
                    }
                };
                doc.set_text(name_el, name);
                if doc.child(ps, "fmtId").is_none() {
                    let fmt = valued(doc, "fmtId", "0");
                    let n = doc.children(ps).len();
                    doc.insert(ps, n, fmt);
                }
            }
            None => {
                if let Some(node) = existing {
                    doc.detach(node);
                }
            }
        }
    }
    let old_v3d = old.groups.iter().find_map(|g| g.three_d);
    let new_v3d = new.groups.iter().find_map(|g| g.three_d);
    if old_v3d != new_v3d {
        let existing = doc.child(chart, "view3D");
        match new_v3d {
            Some(v3d) => {
                let v = match existing {
                    Some(node) => node,
                    None => {
                        let node = new_el(doc, "view3D");
                        place(doc, chart, node, &["floor", "sideWall", "backWall", "plotArea", "legend", "plotVisOnly", "dispBlanksAs", "showDLblsOverMax", "extLst"]);
                        node
                    }
                };
                let set_or_insert = |doc: &mut XmlDocument, parent: NodeId, name: &str, val: &str, order: &[&str]| {
                    if let Some(child) = doc.child(parent, name) {
                        doc.set_attr(child, "val", val);
                    } else {
                        let child = valued(doc, name, val);
                        place(doc, parent, child, order);
                    }
                };
                const VIEW3D_ORDER: &[&str] = &["rotX", "hPercent", "rotY", "depthPercent", "rAngAx", "perspective", "extLst"];
                set_or_insert(doc, v, "rotX", &v3d.rot_x.to_string(), VIEW3D_ORDER);
                set_or_insert(doc, v, "rotY", &v3d.rot_y.to_string(), VIEW3D_ORDER);
                set_or_insert(doc, v, "rAngAx", if v3d.right_angle_axes { "1" } else { "0" }, VIEW3D_ORDER);
                set_or_insert(doc, v, "depthPercent", &v3d.depth_percent.to_string(), VIEW3D_ORDER);
            }
            None => {
                if let Some(node) = existing {
                    doc.detach(node);
                }
            }
        }
    }
    if old.title != new.title {
        let existing = doc.child(chart, "title");
        let deleted = doc.child(chart, "autoTitleDeleted");
        match &new.title {
            None => {
                if let Some(t) = existing {
                    doc.detach(t);
                }
                let d = match deleted {
                    Some(d) => d,
                    None => {
                        let d = new_el(doc, "autoTitleDeleted");
                        place(doc, chart, d, &["pivotFmts", "view3D", "floor", "sideWall", "backWall", "plotArea"]);
                        d
                    }
                };
                doc.set_attr(d, "val", "1");
            }
            Some(t) => {
                if let Some(d) = deleted {
                    doc.set_attr(d, "val", "0");
                }
                let title = match existing {
                    Some(x) => x,
                    None => {
                        let x = new_el(doc, "title");
                        let overlay = valued(doc, "overlay", "0");
                        doc.insert(x, 0, overlay);
                        doc.insert(chart, 0, x);
                        x
                    }
                };
                write_chart_title_text(doc, title, &t.text, &a, &c);
                super::chart_format::text(doc,title,&t.style,&c,&a);
            }
        }
    }
    if old.legend != new.legend {
        let existing = doc.child(chart, "legend");
        match &new.legend {
            None => {
                if let Some(l) = existing {
                    doc.detach(l);
                }
            }
            Some(l) => {
                let legend = match existing {
                    Some(x) => x,
                    None => {
                        let x = new_el(doc, "legend");
                        let overlay = valued(doc, "overlay", "0");
                        doc.insert(x, 0, overlay);
                        place(doc, chart, x, &["plotVisOnly", "dispBlanksAs", "showDLblsOverMax", "extLst"]);
                        x
                    }
                };
                super::chart_format::text(doc,legend,&l.text,&c,&a);
                let code = match l.source_position {
                    Some(ibis_doc_model::LegendPosition::Bottom) => "b",
                    Some(ibis_doc_model::LegendPosition::Left) => "l",
                    Some(ibis_doc_model::LegendPosition::Top) => "t",
                    Some(ibis_doc_model::LegendPosition::TopRight) => "tr",
                    _ => "r",
                };
                match doc.child(legend, "legendPos") {
                    Some(p) => doc.set_attr(p, "val", code),
                    None => {
                        let p = valued(doc, "legendPos", code);
                        doc.insert(legend, 0, p);
                    }
                }
                // A position chosen: the legend's manual place goes.
                if l.layout.is_none()
                    && let Some(layout) = doc.child(legend, "layout")
                {
                    doc.detach(layout);
                }
            }
        }
    }
    if old.plot_vis_only != new.plot_vis_only {
        let existing = doc.child(chart, "plotVisOnly");
        let val_str = if new.plot_vis_only { "1" } else { "0" };
        match existing {
            Some(node) => doc.set_attr(node, "val", val_str),
            None => {
                let node = valued(doc, "plotVisOnly", val_str);
                place(doc, chart, node, &["dispBlanksAs", "showDLblsOverMax", "extLst"]);
            }
        }
    }
    if old.disp_blanks_as != new.disp_blanks_as {
        let existing = doc.child(chart, "dispBlanksAs");
        match new.disp_blanks_as {
            Some(disp) => {
                let code = match disp {
                    ibis_doc_model::DispBlanksAs::Span => "span",
                    ibis_doc_model::DispBlanksAs::Zero => "zero",
                    ibis_doc_model::DispBlanksAs::Gap => "gap",
                };
                match existing {
                    Some(node) => doc.set_attr(node, "val", code),
                    None => {
                        let node = valued(doc, "dispBlanksAs", code);
                        place(doc, chart, node, &["showDLblsOverMax", "extLst"]);
                    }
                }
            }
            None => {
                if let Some(node) = existing {
                    doc.detach(node);
                }
            }
        }
    }
    let plot = doc.child(chart, "plotArea").ok_or_else(|| unsupported("グラフの XML"))?;
    if old.plot_border!=new.plot_border {super::chart_format::area(doc,plot,None,Some(new.plot_border.as_ref()),&c,&a,&["txPr","extLst"])?;}
    for (o, n) in old.axes.iter().zip(&new.axes) {
        if (o.min, o.max, o.major_unit, o.minor_unit, o.display_unit, o.log_base, &o.gridlines, &o.minor_gridlines, &o.title, o.reversed, o.crosses, o.crosses_at) == (n.min, n.max, n.major_unit, n.minor_unit, n.display_unit, n.log_base, &n.gridlines, &n.minor_gridlines, &n.title, n.reversed, n.crosses, n.crosses_at)
            && o.date == n.date && o.text==n.text && o.line==n.line
        {
            continue;
        }
        let axis = doc.elements(plot).find(|&x| {
            matches!(doc.local_name(x), "valAx" | "catAx" | "dateAx" | "serAx") && doc.child(x, "axId").and_then(|i| doc.attr(i, "val")).and_then(|v| v.parse::<u32>().ok()) == n.source_id
        });
        let Some(axis) = axis else { return Err(unsupported("このグラフの軸")) };
        if o.text!=n.text {super::chart_format::text(doc,axis,&n.text,&c,&a);}
        if o.line!=n.line {super::chart_format::area(doc,axis,None,Some(n.line.as_ref()),&c,&a,&["txPr","crossAx","crosses","crossesAt","extLst"])?;}
        if o.date != n.date {
            let tag = if n.date { "dateAx" } else { "catAx" };
            doc.rename(axis, &format!("{c}{tag}"));
            if n.date {
                for name in ["lblAlgn", "tickLblSkip", "tickMarkSkip", "noMultiLvlLbl"] {
                    if let Some(child) = doc.child(axis, name) {
                        doc.detach(child);
                    }
                }
            } else {
                for name in ["baseTimeUnit", "majorTimeUnit", "minorTimeUnit"] {
                    if let Some(child) = doc.child(axis, name) {
                        doc.detach(child);
                    }
                }
            }
        }
        let scaling = match doc.child(axis, "scaling") {
            Some(x) => x,
            None => {
                let x = new_el(doc, "scaling");
                place(doc, axis, x, &["delete", "axPos"]);
                x
            }
        };
        if o.log_base != n.log_base {
            if let Some(e) = doc.child(scaling, "logBase") {
                doc.detach(e);
            }
            if let Some(base) = n.log_base {
                let e = valued(doc, "logBase", &crate::save::xlsx::number_text(base.0));
                place(doc, scaling, e, &["orientation", "max", "min", "extLst"]);
            }
        }
        if o.reversed != n.reversed {
            if let Some(e) = doc.child(scaling, "orientation") {
                doc.detach(e);
            }
            let val = if n.reversed { "maxMin" } else { "minMax" };
            let e = valued(doc, "orientation", val);
            place(doc, scaling, e, &["max", "min", "extLst"]);
        }
        for (name, value, before) in [("max", n.max, &["min", "extLst"][..]), ("min", n.min, &["extLst"][..])] {
            if let Some(e) = doc.child(scaling, name) {
                doc.detach(e);
            }
            if let Some(v) = value {
                let e = valued(doc, name, &crate::save::xlsx::number_text(v.0));
                place(doc, scaling, e, before);
            }
        }
        if o.major_unit != n.major_unit {
            if let Some(e) = doc.child(axis, "majorUnit") {
                doc.detach(e);
            }
            if let Some(v) = n.major_unit {
                let e = valued(doc, "majorUnit", &crate::save::xlsx::number_text(v.0));
                place(doc, axis, e, &["minorUnit", "dispUnits", "extLst"]);
            }
        }
        if o.minor_unit != n.minor_unit {
            if let Some(e) = doc.child(axis, "minorUnit") {
                doc.detach(e);
            }
            if let Some(v) = n.minor_unit {
                let e = valued(doc, "minorUnit", &crate::save::xlsx::number_text(v.0));
                place(doc, axis, e, &["dispUnits", "extLst"]);
            }
        }
        if o.display_unit != n.display_unit {
            if let Some(e) = doc.child(axis, "dispUnits") {
                doc.detach(e);
            }
            if let Some(v) = n.display_unit {
                let du = new_el(doc, "dispUnits");
                if let Some(built_in) = built_in_unit_name(v.0) {
                    let b = valued(doc, "builtInUnit", built_in);
                    doc.insert(du, 0, b);
                } else {
                    let c = valued(doc, "custUnit", &crate::save::xlsx::number_text(v.0));
                    doc.insert(du, 0, c);
                }
                place(doc, axis, du, &["extLst"]);
            }
        }
        if o.gridlines.is_some() != n.gridlines.is_some() {
            match (doc.child(axis, "majorGridlines"), n.gridlines.is_some()) {
                (Some(g), false) => doc.detach(g),
                (None, true) => {
                    let g = new_el(doc, "majorGridlines");
                    place(doc, axis, g, &["minorGridlines", "title", "numFmt", "majorTickMark", "minorTickMark", "tickLblPos", "spPr", "txPr", "crossAx"]);
                }
                _ => {}
            }
        }
        if o.gridlines!=n.gridlines {if let (Some(node),Some(stroke))=(doc.child(axis,"majorGridlines"),n.gridlines.as_ref()){super::chart_format::area(doc,node,None,Some(Some(stroke)),&c,&a,&[])?;}}
        if o.minor_gridlines.is_some() != n.minor_gridlines.is_some() {
            match (doc.child(axis, "minorGridlines"), n.minor_gridlines.is_some()) {
                (Some(g), false) => doc.detach(g),
                (None, true) => {
                    let g = new_el(doc, "minorGridlines");
                    place(doc, axis, g, &["title", "numFmt", "majorTickMark", "minorTickMark", "tickLblPos", "spPr", "txPr", "crossAx", "crosses", "crossesAt", "extLst"]);
                }
                _ => {}
            }
        }
        if o.title != n.title {
            let existing = doc.child(axis, "title");
            match &n.title {
                None => {
                    if let Some(t) = existing {
                        doc.detach(t);
                    }
                }
                Some(t) => {
                    let title = match existing {
                        Some(x) => x,
                        None => {
                            let x = new_el(doc, "title");
                            let overlay = valued(doc, "overlay", "0");
                            doc.insert(x, 0, overlay);
                            place(doc, axis, x, &["numFmt", "majorTickMark", "minorTickMark", "tickLblPos", "spPr", "txPr", "crossAx", "crosses", "crossesAt", "extLst"]);
                            x
                        }
                    };
                    write_chart_title_text(doc, title, &t.text, &a, &c);
                super::chart_format::text(doc,title,&t.style,&c,&a);
                }
            }
        }
        if o.crosses != n.crosses || o.crosses_at != n.crosses_at {
            if let Some(e) = doc.child(axis, "crosses") {
                doc.detach(e);
            }
            if let Some(e) = doc.child(axis, "crossesAt") {
                doc.detach(e);
            }
            if let Some(at) = n.crosses_at {
                let e = valued(doc, "crossesAt", &crate::save::xlsx::number_text(at.0));
                place(doc, axis, e, &["auto", "lblAlgn", "lblOffset", "tickLblSkip", "crossBetween", "majorUnit", "minorUnit", "dispUnits", "extLst"]);
            } else if let Some(crosses) = n.crosses {
                let val = match crosses {
                    ibis_doc_model::AxisCrosses::AutoZero => "autoZero",
                    ibis_doc_model::AxisCrosses::Maximum => "max",
                    ibis_doc_model::AxisCrosses::Minimum => "min",
                };
                let e = valued(doc, "crosses", val);
                place(doc, axis, e, &["auto", "lblAlgn", "lblOffset", "tickLblSkip", "crossBetween", "majorUnit", "minorUnit", "dispUnits", "extLst"]);
            }
        }
    }
    // Update gapWidth, overlap, and data labels for each group / series if changed.
    let mut old_series = old.groups.iter().flat_map(|g| &g.series);
    let mut new_series = new.groups.iter().flat_map(|g| &g.series);
    let group_elements: Vec<NodeId> = doc.elements(plot).filter(|&x| {
        let name = doc.local_name(x);
        name.ends_with("Chart") && name != "plotArea"
    }).collect();
    for (gi, mut g_el) in group_elements.into_iter().enumerate() {
        if let (Some(old_g), Some(new_g)) = (old.groups.get(gi), new.groups.get(gi)) {
            if old.groups.len() == 1 && (old_g.kind != new_g.kind || old_g.grouping != new_g.grouping || old_g.three_d.is_some() != new_g.three_d.is_some()) && super::pptx_chart_type::is_conversion(old_g.kind, new_g.kind) {
                super::pptx_chart_type::convert_group(doc, plot, g_el, old_g, new_g)?;
                if let Some(new_el) = doc.elements(plot).find(|&x| {
                    let name = doc.local_name(x);
                    name.ends_with("Chart") && name != "plotArea"
                }) {
                    g_el = new_el;
                }
            }
            if let (ibis_doc_model::ChartType::Bar { gap_width: o_gap, overlap: o_ov, .. }, ibis_doc_model::ChartType::Bar { gap_width: n_gap, overlap: n_ov, .. }) = (&old_g.kind, &new_g.kind) {
                if o_gap != n_gap {
                    if let Some(e) = doc.child(g_el, "gapWidth") {
                        doc.set_attr(e, "val", &n_gap.to_string());
                    } else {
                        let e = valued(doc, "gapWidth", &n_gap.to_string());
                        place(doc, g_el, e, &["overlap", "serLines", "axId", "extLst"]);
                    }
                }
                if o_ov != n_ov {
                    if let Some(e) = doc.child(g_el, "overlap") {
                        doc.set_attr(e, "val", &n_ov.to_string());
                    } else {
                        let e = valued(doc, "overlap", &n_ov.to_string());
                        place(doc, g_el, e, &["serLines", "axId", "extLst"]);
                    }
                }
            }
            if let (ibis_doc_model::ChartType::Radar { style: o_style }, ibis_doc_model::ChartType::Radar { style: n_style }) = (&old_g.kind, &new_g.kind) {
                if o_style != n_style {
                    let val_str = match n_style {
                        Some(ibis_doc_model::RadarStyle::Standard) => "standard",
                        Some(ibis_doc_model::RadarStyle::Marker) => "marker",
                        Some(ibis_doc_model::RadarStyle::Filled) => "filled",
                        None => "standard",
                    };
                    if let Some(e) = doc.child(g_el, "radarStyle") {
                        doc.set_attr(e, "val", val_str);
                    } else {
                        let e = valued(doc, "radarStyle", val_str);
                        place(doc, g_el, e, &["varyColors", "ser", "axId", "extLst"]);
                    }
                }
            }
            if let (ibis_doc_model::ChartType::OfPie { kind: o_kind }, ibis_doc_model::ChartType::OfPie { kind: n_kind }) = (&old_g.kind, &new_g.kind) {
                if o_kind != n_kind {
                    let val_str = match n_kind {
                        Some(ibis_doc_model::OfPieKind::Bar) => "bar",
                        _ => "pie",
                    };
                    if let Some(e) = doc.child(g_el, "ofPieType") {
                        doc.set_attr(e, "val", val_str);
                    } else {
                        let e = valued(doc, "ofPieType", val_str);
                        place(doc, g_el, e, &["varyColors", "ser", "dLbls", "gapWidth", "splitType", "splitPos", "custSplit", "secondPieSize", "serLines", "extLst"]);
                    }
                }
                if let (Some(o_op), Some(n_op)) = (old_g.of_pie.as_ref(), new_g.of_pie.as_ref()) {
                    if o_op.split != n_op.split {
                        let val_str = match n_op.split {
                            Some(ibis_doc_model::OfPieSplit::Custom) => "cust",
                            Some(ibis_doc_model::OfPieSplit::Percent) => "percent",
                            Some(ibis_doc_model::OfPieSplit::Position) => "pos",
                            Some(ibis_doc_model::OfPieSplit::Value) => "val",
                            None => "auto",
                        };
                        if let Some(e) = doc.child(g_el, "splitType") {
                            doc.set_attr(e, "val", val_str);
                        } else {
                            let e = valued(doc, "splitType", val_str);
                            place(doc, g_el, e, &["splitPos", "custSplit", "secondPieSize", "serLines", "extLst"]);
                        }
                    }
                    if o_op.split_position != n_op.split_position {
                        if let Some(sp) = n_op.split_position {
                            let val_str = crate::save::xlsx::number_text(sp.0);
                            if let Some(e) = doc.child(g_el, "splitPos") {
                                doc.set_attr(e, "val", &val_str);
                            } else {
                                let e = valued(doc, "splitPos", &val_str);
                                place(doc, g_el, e, &["custSplit", "secondPieSize", "serLines", "extLst"]);
                            }
                        } else if let Some(e) = doc.child(g_el, "splitPos") {
                            doc.detach(e);
                        }
                    }
                    if o_op.second_size != n_op.second_size {
                        if let Some(ss) = n_op.second_size {
                            if let Some(e) = doc.child(g_el, "secondPieSize") {
                                doc.set_attr(e, "val", &ss.to_string());
                            } else {
                                let e = valued(doc, "secondPieSize", &ss.to_string());
                                place(doc, g_el, e, &["serLines", "extLst"]);
                            }
                        } else if let Some(e) = doc.child(g_el, "secondPieSize") {
                            doc.detach(e);
                        }
                    }
                    if o_op.gap_width != n_op.gap_width {
                        if let Some(gw) = n_op.gap_width {
                            if let Some(e) = doc.child(g_el, "gapWidth") {
                                doc.set_attr(e, "val", &gw.to_string());
                            } else {
                                let e = valued(doc, "gapWidth", &gw.to_string());
                                place(doc, g_el, e, &["splitType", "splitPos", "custSplit", "secondPieSize", "serLines", "extLst"]);
                            }
                        } else if let Some(e) = doc.child(g_el, "gapWidth") {
                            doc.detach(e);
                        }
                    }
                }
            }
            if let (ibis_doc_model::ChartType::Stock { up_down_bars: o_ud, hi_low_lines: o_hl, drop_lines: o_dl },
                    ibis_doc_model::ChartType::Stock { up_down_bars: n_ud, hi_low_lines: n_hl, drop_lines: n_dl }) = (&old_g.kind, &new_g.kind) {
                if o_dl != n_dl {
                    if *n_dl {
                        if doc.child(g_el, "dropLines").is_none() {
                            let e = new_el(doc, "dropLines");
                            place(doc, g_el, e, &["hiLowLines", "upDownBars", "axId", "extLst"]);
                        }
                    } else if let Some(e) = doc.child(g_el, "dropLines") {
                        doc.detach(e);
                    }
                }
                if o_hl != n_hl {
                    if *n_hl {
                        if doc.child(g_el, "hiLowLines").is_none() {
                            let e = new_el(doc, "hiLowLines");
                            place(doc, g_el, e, &["upDownBars", "axId", "extLst"]);
                        }
                    } else if let Some(e) = doc.child(g_el, "hiLowLines") {
                        doc.detach(e);
                    }
                }
                if o_ud != n_ud {
                    if *n_ud {
                        if doc.child(g_el, "upDownBars").is_none() {
                            let e = new_el(doc, "upDownBars");
                            let gw = valued(doc, "gapWidth", "150");
                            let up = new_el(doc, "upBars");
                            let down = new_el(doc, "downBars");
                            let n = doc.children(e).len();
                            doc.insert(e, n, gw);
                            let n = doc.children(e).len();
                            doc.insert(e, n, up);
                            let n = doc.children(e).len();
                            doc.insert(e, n, down);
                            place(doc, g_el, e, &["axId", "extLst"]);
                        }
                    } else if let Some(e) = doc.child(g_el, "upDownBars") {
                        doc.detach(e);
                    }
                }
            }
            if let (ibis_doc_model::ChartType::Surface { is_3d: o_3d, wireframe: o_wf },
                    ibis_doc_model::ChartType::Surface { is_3d: n_3d, wireframe: n_wf }) = (&old_g.kind, &new_g.kind) {
                if o_3d != n_3d {
                    let tag = if *n_3d { "surface3DChart" } else { "surfaceChart" };
                    let prefix = doc.name(g_el).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();
                    doc.rename(g_el, &format!("{prefix}{tag}"));
                }
                if o_wf != n_wf || doc.child(g_el, "wireframe").is_none() {
                    let wf_val = if *n_wf { "1" } else { "0" };
                    if let Some(wf_node) = doc.child(g_el, "wireframe") {
                        doc.set_attr(wf_node, "val", wf_val);
                    } else {
                        let wf_node = valued(doc, "wireframe", wf_val);
                        place(doc, g_el, wf_node, &["ser", "bandFmts", "axId", "extLst"]);
                    }
                }
            }
        }
        let series_elements: Vec<NodeId> = doc.children_named(g_el, "ser").collect();
        for ser in series_elements {
            let o_ser = old_series.next();
            let n_ser = new_series.next();
            if let (Some(o), Some(n)) = (o_ser, n_ser) {
                if o.fill != n.fill {
                    crate::save::pptx_chart::write_series_fill(doc, ser, &n.fill, &c, &a)?;
                }
                if o.labels != n.labels {
                    crate::save::pptx_chart::write_labels(doc, ser, o.labels.as_ref(), n.labels.as_ref())?;
                    if let (Some(node),Some(labels))=(doc.child(ser,"dLbls"),n.labels.as_ref()){super::chart_format::text(doc,node,&labels.style,&c,&a);}
                }
                if o.marker != n.marker {
                    crate::save::pptx_chart::write_series_marker(doc, ser, n.marker.as_ref(), &c, &a)?;
                }
                if o.stroke != n.stroke {
                    crate::save::pptx_chart::write_series_stroke(doc, ser, n.stroke.as_ref(), &c, &a)?;
                }
                if o.trendline != n.trendline {
                    crate::save::pptx_chart::write_trendline(doc, ser, n.trendline.as_ref())?;
                }
                if o.error_bars != n.error_bars {
                    crate::save::pptx_chart::write_error_bars(doc, ser, n.error_bars.as_ref())?;
                }
            }
        }
    }
    if old.data_table != new.data_table {
        crate::save::pptx_chart::write_data_table(doc, plot, new.data_table.as_ref())?;
    }
    if old.axes != new.axes {
        crate::save::pptx_chart::write_axes(doc, plot, &old.axes, &new.axes)?;
    }
    if old.plot_background != new.plot_background {
        crate::save::pptx_chart::write_plot_background(doc, plot, &new.plot_background, &c, &a)?;
    }
    Ok(())
}

/// A number as a chart's XML writes it (`2500000`, `0.5`).
pub(crate) fn number_text(v: f64) -> String {
    ibis_calc::number_to_xml(v)
}

/// The sheet's tables as the model has them: a new one written as its own
/// part (`xl/tables/tableN.xml`, with `tableParts` in the sheet), one whose
/// range or columns changed rewritten there. Taking a table off
/// (範囲に変換) is not saved yet.
/// The calculated column formula for column `col_index` of `table` in `sheet`,
/// when all data rows of that column have formulas that are vertical shifts of
/// the first data row's formula (as Excel's `calculatedColumnFormula`).
fn calculated_column_formula(sheet: &Sheet, table: &ibis_doc_model::SheetTable, col_index: usize) -> Option<String> {
    let col = table.range.first_column + col_index as u32;
    let first_row = table.range.first_row + table.header_rows;
    let last_row = table.range.last_row.checked_sub(table.totals_rows)?;
    if first_row > last_row {
        return None;
    }
    let first = sheet.cell(first_row, col)?.formula.as_deref()?.trim_start_matches('=');
    if first.is_empty() {
        return None;
    }
    for row in (first_row + 1)..=last_row {
        let f = sheet.cell(row, col)?.formula.as_deref()?.trim_start_matches('=');
        if ibis_calc::formula::shift(first, (row - first_row) as i64, 0) != f {
            return None;
        }
    }
    Some(first.to_string())
}

fn table_calculated_formulas(sheet: &Sheet, table: &ibis_doc_model::SheetTable) -> Vec<Option<String>> {
    (0..table.columns.len()).map(|i| calculated_column_formula(sheet, table, i)).collect()
}

fn table_calculated_formulas_eq(old: &Sheet, new: &Sheet) -> bool {
    old.tables.len() == new.tables.len()
        && old.tables.iter().zip(&new.tables).all(|(o, n)| table_calculated_formulas(old, o) == table_calculated_formulas(new, n))
}

fn write_tables(package: &mut Package, styles_part: Option<&str>, part: &str, old_sheet: &Sheet, new_sheet: &Sheet) -> Result<(), Error> {
    let old = &old_sheet.tables;
    let new = &new_sheet.tables;
    const REL_TABLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table";
    const CT_TABLE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml";
    // Tables taken off (範囲に変換): their parts, relationships and
    // `tablePart`s go; their formats stay in the cells.
    // A table renamed keeps its place (and its first cell) in the list.
    let renamed = |i: usize, o: &ibis_doc_model::SheetTable| new.get(i).is_some_and(|n| n.range.first_row == o.range.first_row && n.range.first_column == o.range.first_column && !old.iter().any(|x| x.name.eq_ignore_ascii_case(&n.name)));
    for gone in old.iter().enumerate().filter(|(i, o)| !new.iter().any(|n| n.name.eq_ignore_ascii_case(&o.name)) && !renamed(*i, o)).map(|(_, o)| o) {
        let rels = package.relationships(part)?;
        let mut found = None;
        for rel in rels.iter().filter(|r| r.kind == REL_TABLE && !r.external) {
            let Some(target) = rel.target_part(part) else { continue };
            let doc = package.xml(&target)?;
            let Some(root) = doc.root_element() else { continue };
            if doc.attr(root, "displayName").or_else(|| doc.attr(root, "name")).is_some_and(|n| n.eq_ignore_ascii_case(&gone.name)) {
                found = Some((rel.id.clone(), target));
                break;
            }
        }
        let Some((id, target)) = found else { return Err(unsupported("このテーブルの範囲への変換")) };
        package.remove(&target);
        let _ = package.remove_override(&target);
        package.remove_relationship(part, &id)?;
        let doc = package.xml_mut(part)?;
        let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
        if let Some(list) = doc.child(root, "tableParts") {
            let r = doc.lookup_prefix(root, "http://schemas.openxmlformats.org/officeDocument/2006/relationships").unwrap_or_else(|| "r".into());
            for p in doc.children_named(list, "tablePart").collect::<Vec<_>>() {
                if doc.attr(p, &format!("{r}:id")) == Some(id.as_str()) {
                    doc.detach(p);
                }
            }
            let count = doc.children_named(list, "tablePart").count();
            if count == 0 {
                doc.detach(list);
            } else {
                doc.set_attr(list, "count", &count.to_string());
            }
        }
    }
    // The sheet's table parts, by the table's name.
    let mut parts: Vec<(String, String)> = Vec::new();
    for rel in package.relationships(part)?.iter().filter(|r| r.kind == REL_TABLE && !r.external) {
        let Some(target) = rel.target_part(part) else { continue };
        let doc = package.xml(&target)?;
        let Some(root) = doc.root_element() else { continue };
        let name = doc.attr(root, "displayName").or_else(|| doc.attr(root, "name")).unwrap_or("").to_string();
        parts.push((name, target));
    }
    let area = |r: CellRange| format!("{}:{}", cell_name(r.first_column, r.first_row), cell_name(r.last_column, r.last_row));
    for (ti, t) in new.iter().enumerate() {
        // A renamed table: the one in its place before, under its old name.
        let renamed_from = old.get(ti).filter(|o| !o.name.eq_ignore_ascii_case(&t.name) && o.range.first_row == t.range.first_row && o.range.first_column == t.range.first_column && !new.iter().any(|n| n.name.eq_ignore_ascii_case(&o.name)));
        let before = old.iter().find(|o| o.name.eq_ignore_ascii_case(&t.name)).or(renamed_from);
        let calc_before = before.map(|b| table_calculated_formulas(old_sheet, b)).unwrap_or_default();
        let calc_after = table_calculated_formulas(new_sheet, t);
        if before == Some(t) && calc_before == calc_after {
            continue;
        }
        let part_name = before.map_or(t.name.as_str(), |b| b.name.as_str());
        if let Some((_, target)) = parts.iter().find(|(n, _)| n.eq_ignore_ascii_case(part_name)) {
            let filter_colors = match t.filter.as_ref().filter(|f| f.columns.iter().any(|(_, x)| matches!(x, ibis_doc_model::ColumnFilter::Color { .. }))) {
                Some(f) => {
                    let styles = styles_part.ok_or_else(|| unsupported("書式のないブックの色フィルター"))?;
                    write_filter_dxfs(package, styles, target, before.and_then(|b| b.filter.as_ref()), f)?
                }
                None => HashMap::new(),
            };
            // A table there: grown, with columns added, renamed, its rows
            // and style changed.
            let doc = package.xml_mut(target)?;
            let root = doc.root_element().ok_or_else(|| Error::new("テーブルの XML が空です"))?;
            let prefix = prefix_of(doc, root);
            doc.set_attr(root, "ref", &area(t.range));
            if before.is_some_and(|b| b.name != t.name) {
                doc.set_attr(root, "name", &t.name);
                doc.set_attr(root, "displayName", &t.name);
            }
            if t.header_rows == 0 {
                doc.set_attr(root, "headerRowCount", "0");
            } else {
                doc.remove_attr(root, "headerRowCount");
            }
            if t.totals_rows > 0 {
                doc.set_attr(root, "totalsRowCount", &t.totals_rows.to_string());
                doc.remove_attr(root, "totalsRowShown");
            } else {
                doc.remove_attr(root, "totalsRowCount");
                if before.is_some_and(|b| b.totals_rows > 0) {
                    doc.set_attr(root, "totalsRowShown", "0");
                }
            }
            if before.is_some_and(|b| b.style != t.style) {
                match (&t.style, doc.child(root, "tableStyleInfo")) {
                    (Some(st), e) => {
                        let e = match e {
                            Some(e) => e,
                            None => {
                                let e = detached(doc, &format!("{prefix}tableStyleInfo"));
                                let at = doc.elements(root).find(|&c| doc.local_name(c) == "extLst");
                                match at {
                                    Some(a) => doc.insert_before(a, e),
                                    None => {
                                        let n = doc.children(root).len();
                                        doc.insert(root, n, e);
                                    }
                                }
                                e
                            }
                        };
                        let b = |v: bool| if v { "1" } else { "0" };
                        doc.set_attr(e, "name", &st.name);
                        doc.set_attr(e, "showFirstColumn", b(st.first_column));
                        doc.set_attr(e, "showLastColumn", b(st.last_column));
                        doc.set_attr(e, "showRowStripes", b(st.row_stripes));
                        doc.set_attr(e, "showColumnStripes", b(st.column_stripes));
                    }
                    (None, Some(e)) => doc.detach(e),
                    (None, None) => {}
                }
            }
            // The filter: its ▼ and what it keeps.
            match (&t.filter, doc.child(root, "autoFilter")) {
                (Some(f), Some(e)) => sizes::fill_auto_filter(doc, e, f, &filter_colors, &prefix),
                (Some(f), None) => {
                    let e = detached(doc, &format!("{prefix}autoFilter"));
                    sizes::fill_auto_filter(doc, e, f, &filter_colors, &prefix);
                    let at = doc.elements(root).find(|&c| matches!(doc.local_name(c), "sortState" | "tableColumns"));
                    match at {
                        Some(a) => doc.insert_before(a, e),
                        None => doc.insert(root, 0, e),
                    }
                }
                (None, Some(e)) => doc.detach(e),
                (None, None) => {}
            }
            if let Some(list) = doc.child(root, "tableColumns") {
                let existing: Vec<NodeId> = doc.children_named(list, "tableColumn").collect();
                let existing_len = existing.len();
                let mut next = existing.iter().filter_map(|&c| doc.attr(c, "id")?.parse::<u32>().ok()).max().unwrap_or(0) + 1;
                for name in t.columns.iter().skip(existing_len) {
                    let c = detached(doc, &format!("{prefix}tableColumn"));
                    doc.set_attr(c, "id", &next.to_string());
                    doc.set_attr(c, "name", name);
                    next += 1;
                    let n = doc.children(list).len();
                    doc.insert(list, n, c);
                }
                let count = doc.children_named(list, "tableColumn").count();
                doc.set_attr(list, "count", &count.to_string());
                let cols: Vec<NodeId> = doc.children_named(list, "tableColumn").collect();
                // The 集計行's function or label of each column.
                if before.is_some_and(|b| b.totals != t.totals) {
                    for (i, &c) in cols.iter().enumerate() {
                        let total = t.totals.get(i).cloned().unwrap_or_default();
                        match &total.function {
                            Some(f) => doc.set_attr(c, "totalsRowFunction", f),
                            None => doc.remove_attr(c, "totalsRowFunction"),
                        }
                        match &total.label {
                            Some(l) => doc.set_attr(c, "totalsRowLabel", l),
                            None => doc.remove_attr(c, "totalsRowLabel"),
                        }
                    }
                }
                // Calculated column formulas (`<calculatedColumnFormula>`).
                for (i, &c) in cols.iter().enumerate() {
                    if i < existing_len && calc_before.get(i) == calc_after.get(i) {
                        continue;
                    }
                    match calc_after.get(i).and_then(Option::as_deref) {
                        Some(formula) => {
                            let f_node = match doc.child(c, "calculatedColumnFormula") {
                                Some(n) => n,
                                None => {
                                    let n = detached(doc, &format!("{prefix}calculatedColumnFormula"));
                                    let at = doc.elements(c).find(|&ch| matches!(doc.local_name(ch), "totalsRowFormula" | "xmlColumnPr" | "extLst"));
                                    match at {
                                        Some(a) => doc.insert_before(a, n),
                                        None => {
                                            let len = doc.children(c).len();
                                            doc.insert(c, len, n);
                                        }
                                    }
                                    n
                                }
                            };
                            doc.set_text(f_node, formula);
                        }
                        None => {
                            if let Some(n) = doc.child(c, "calculatedColumnFormula") {
                                doc.detach(n);
                            }
                        }
                    }
                }
            }
            continue;
        }
        // A new table: the next id in the book and a new part.
        let mut id = 0u32;
        let names: Vec<String> = package.names().filter(|n| n.starts_with("xl/tables/") && n.ends_with(".xml")).map(str::to_string).collect();
        for n in &names {
            if let Ok(doc) = package.xml(n)
                && let Some(root) = doc.root_element()
            {
                id = id.max(doc.attr(root, "id").and_then(|v| v.parse().ok()).unwrap_or(0));
            }
        }
        id += 1;
        let mut k = 1;
        let target = loop {
            let candidate = format!("xl/tables/table{k}.xml");
            if !package.contains(&candidate) {
                break candidate;
            }
            k += 1;
        };
        let filter_colors = match t.filter.as_ref().filter(|f| f.columns.iter().any(|(_, x)| matches!(x, ibis_doc_model::ColumnFilter::Color { .. }))) {
            Some(f) => {
                let styles = styles_part.ok_or_else(|| unsupported("書式のないブックの色フィルター"))?;
                write_filter_dxfs(package, styles, &target, None, f)?
            }
            None => HashMap::new(),
        };
        let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
        let filter_area = CellRange { last_row: t.range.last_row - t.totals_rows, ..t.range };
        let mut xml = format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<table xmlns="{MAIN_NS}" id="{id}" name="{name}" displayName="{name}" ref="{area}"{header}{totals}>"#,
            name = esc(&t.name),
            area = area(t.range),
            header = if t.header_rows == 0 { r#" headerRowCount="0""# } else { "" },
            totals = if t.totals_rows > 0 { r#" totalsRowCount="1""# } else { r#" totalsRowShown="0""# },
        );
        if t.header_rows > 0 && t.filter.is_some() {
            xml.push_str(&format!(r#"<autoFilter ref="{}"/>"#, area(filter_area)));
        }
        xml.push_str(&format!(r#"<tableColumns count="{}">"#, t.columns.len()));
        for (i, c) in t.columns.iter().enumerate() {
            let total = t.totals.get(i).cloned().unwrap_or_default();
            let mut extra = String::new();
            if let Some(l) = &total.label {
                extra += &format!(r#" totalsRowLabel="{}""#, esc(l));
            }
            if let Some(f) = &total.function {
                extra += &format!(r#" totalsRowFunction="{}""#, esc(f));
            }
            if let Some(formula) = calc_after.get(i).and_then(Option::as_deref) {
                xml.push_str(&format!(
                    r#"<tableColumn id="{}" name="{}"{extra}><calculatedColumnFormula>{}</calculatedColumnFormula></tableColumn>"#,
                    i + 1,
                    esc(c),
                    esc(formula)
                ));
            } else {
                xml.push_str(&format!(r#"<tableColumn id="{}" name="{}"{extra}/>"#, i + 1, esc(c)));
            }
        }
        xml.push_str("</tableColumns>");
        if let Some(st) = &t.style {
            let b = |v: bool| if v { "1" } else { "0" };
            xml.push_str(&format!(
                r#"<tableStyleInfo name="{}" showFirstColumn="{}" showLastColumn="{}" showRowStripes="{}" showColumnStripes="{}"/>"#,
                esc(&st.name),
                b(st.first_column),
                b(st.last_column),
                b(st.row_stripes),
                b(st.column_stripes)
            ));
        }
        xml.push_str("</table>");
        let mut doc = XmlDocument::parse(xml.as_bytes()).map_err(|e| Error::new(format!("テーブルの XML：{e:?}")))?;
        if let (Some(f), Some(root)) = (&t.filter, doc.root_element())
            && let Some(e) = doc.child(root, "autoFilter")
        {
            sizes::fill_auto_filter(&mut doc, e, f, &filter_colors, "");
        }
        package.set_xml(&target, doc);
        package.set_override(&target, CT_TABLE)?;
        let rid = package.add_relationship(part, REL_TABLE, &relative_target(part, &target), false)?;
        // The sheet's `tableParts`.
        let doc = package.xml_mut(part)?;
        let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
        let prefix = prefix_of(doc, root);
        let r = doc.lookup_prefix(root, "http://schemas.openxmlformats.org/officeDocument/2006/relationships").unwrap_or_else(|| {
            doc.set_attr(root, "xmlns:r", "http://schemas.openxmlformats.org/officeDocument/2006/relationships");
            "r".into()
        });
        let list = match doc.child(root, "tableParts") {
            Some(l) => l,
            None => {
                let l = detached(doc, &format!("{prefix}tableParts"));
                let anchor = doc.elements(root).find(|&c| doc.local_name(c) == "extLst");
                match anchor {
                    Some(a) => doc.insert_before(a, l),
                    None => {
                        let n = doc.children(root).len();
                        doc.insert(root, n, l);
                    }
                }
                l
            }
        };
        let e = detached(doc, &format!("{prefix}tablePart"));
        doc.set_attr(e, &format!("{r}:id"), &rid);
        let n = doc.children(list).len();
        doc.insert(list, n, e);
        let count = doc.children_named(list, "tablePart").count();
        doc.set_attr(list, "count", &count.to_string());
    }
    Ok(())
}

fn write_comments(package: &mut Package, part: &str, comments: &[ibis_doc_model::SheetComment]) -> Result<(), Error> {
    let existing_rels = package.relationships(part)?;
    let existing_comment_rel = existing_rels.iter().find(|r| r.kind == REL_COMMENTS && !r.external).cloned();

    if comments.is_empty() {
        if let Some(rel) = existing_comment_rel {
            if let Some(target_part) = rel.target_part(part) {
                package.remove(&target_part);
                let _ = package.remove_override(&target_part);
            }
            package.remove_relationship(part, &rel.id)?;
        }
        return write_note_shapes(package, part, comments);
    }

    let comments_part = if let Some(rel) = &existing_comment_rel {
        rel.target_part(part).unwrap_or_else(|| "xl/comments1.xml".to_string())
    } else {
        let mut idx = 1;
        loop {
            let candidate = format!("xl/comments{idx}.xml");
            if !package.contains(&candidate) {
                break candidate;
            }
            idx += 1;
        }
    };

    if existing_comment_rel.is_none() {
        let rel_target = relative_target(part, &comments_part);
        package.add_relationship(part, REL_COMMENTS, &rel_target, false)?;
        package.set_override(&comments_part, CT_COMMENTS)?;
    }

    let mut doc = XmlDocument::new_with_declaration();
    let root = doc.append_element(XmlDocument::ROOT, "comments");
    doc.set_attr(root, "xmlns", MAIN_NS);

    let mut authors: Vec<String> = Vec::new();
    for c in comments {
        let a = if c.author.is_empty() { "Author".to_string() } else { c.author.clone() };
        if !authors.contains(&a) {
            authors.push(a);
        }
    }
    if authors.is_empty() {
        authors.push("Author".to_string());
    }

    let authors_el = doc.append_element(root, "authors");
    for a in &authors {
        let author_el = doc.append_element(authors_el, "author");
        doc.set_text(author_el, a);
    }

    let list_el = doc.append_element(root, "commentList");
    for c in comments {
        let comment_el = doc.append_element(list_el, "comment");
        let ref_str = cell_name(c.column, c.row);
        doc.set_attr(comment_el, "ref", &ref_str);
        let a_str = if c.author.is_empty() { "Author" } else { c.author.as_str() };
        let author_id = authors.iter().position(|a| a == a_str).unwrap_or(0);
        doc.set_attr(comment_el, "authorId", &author_id.to_string());

        let text_el = doc.append_element(comment_el, "text");
        let r_el = doc.append_element(text_el, "r");
        let t_el = doc.append_element(r_el, "t");
        if c.text.starts_with(char::is_whitespace) || c.text.ends_with(char::is_whitespace) || c.text.contains('\n') {
            doc.set_attr(t_el, "xml:space", "preserve");
        }
        doc.set_text(t_el, &c.text);
    }

    package.set_xml(&comments_part, doc);
    write_note_shapes(package, part, comments)?;
    Ok(())
}

const REL_VML: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing";
const CT_VML: &str = "application/vnd.openxmlformats-officedocument.vmlDrawing";

/// A note shape of a VML drawing: its cell, if it is one (`x:ClientData
/// ObjectType="Note"` with `x:Row` and `x:Column`).
fn note_cell(shape: &str) -> Option<(u32, u32)> {
    if !shape.contains("ObjectType=\"Note\"") {
        return None;
    }
    let number = |tag: &str| -> Option<u32> {
        let open = format!("<x:{tag}>");
        let start = shape.find(&open)? + open.len();
        let end = shape[start..].find('<')? + start;
        shape[start..end].trim().parse().ok()
    };
    Some((number("Row")?, number("Column")?))
}

/// The shapes (`<v:shape …>…</v:shape>`) of a VML drawing, with where they are.
fn vml_shapes(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(start) = text[at..].find("<v:shape ").map(|i| i + at) {
        let Some(end) = text[start..].find("</v:shape>").map(|i| i + start + "</v:shape>".len()) else { break };
        out.push((start, end));
        at = end;
    }
    out
}

/// The note's shape Excel writes (hidden until the cell is pointed at),
/// beside its cell.
fn note_shape(id: u32, row: u32, column: u32) -> String {
    format!(
        "<v:shape id=\"_x0000_s{id}\" type=\"#_x0000_t202\" style='position:absolute;margin-left:59.25pt;margin-top:1.5pt;width:108pt;height:59.25pt;z-index:{z};visibility:hidden' fillcolor=\"#ffffe1\" o:insetmode=\"auto\">\r\n <v:fill color2=\"#ffffe1\"/>\r\n <v:shadow on=\"t\" color=\"black\" obscured=\"t\"/>\r\n <v:path o:connecttype=\"none\"/>\r\n <v:textbox style='mso-direction-alt:auto'>\r\n  <div style='text-align:left'></div>\r\n </v:textbox>\r\n <x:ClientData ObjectType=\"Note\">\r\n  <x:MoveWithCells/>\r\n  <x:SizeWithCells/>\r\n  <x:Anchor>\r\n   {}, 15, {}, 10, {}, 15, {}, 4</x:Anchor>\r\n  <x:AutoFill>False</x:AutoFill>\r\n  <x:Row>{row}</x:Row>\r\n  <x:Column>{column}</x:Column>\r\n </x:ClientData>\r\n</v:shape>",
        column + 1,
        row.saturating_sub(1),
        column + 3,
        row + 3,
        z = id % 1024,
    )
}

/// The VML drawing Excel needs to show the sheet's notes (it reads no note
/// without its shape): the notes' shapes kept, added and taken off to match
/// `comments`, the drawing's other shapes (form controls) kept; with the
/// sheet's `legacyDrawing`, its relationship and its content type. No
/// shapes left: the drawing goes.
fn write_note_shapes(package: &mut Package, part: &str, comments: &[ibis_doc_model::SheetComment]) -> Result<(), Error> {
    let rels = package.relationships(part)?;
    let rel = rels.iter().find(|r| r.kind == REL_VML && !r.external).cloned();
    let vml_part = rel.as_ref().and_then(|r| r.target_part(part));
    let existing = vml_part.as_ref().and_then(|p| package.read(p).ok()).map(|b| String::from_utf8_lossy(&b).into_owned());
    let wanted: Vec<(u32, u32)> = comments.iter().map(|c| (c.row, c.column)).collect();
    let mut kept: Vec<String> = Vec::new();
    let mut have: Vec<(u32, u32)> = Vec::new();
    let mut max_id = 0u32;
    let (mut head, mut tail) = (String::new(), String::new());
    if let Some(text) = &existing {
        let shapes = vml_shapes(text);
        for &(a, b) in &shapes {
            let shape = &text[a..b];
            if let Some(n) = shape.find("_x0000_s").and_then(|i| shape[i + 8..].split(|c: char| !c.is_ascii_digit()).next().and_then(|d| d.parse::<u32>().ok())) {
                max_id = max_id.max(n);
            }
            match note_cell(shape) {
                Some(cell) if !wanted.contains(&cell) => {}
                Some(cell) => {
                    have.push(cell);
                    kept.push(shape.to_string());
                }
                None => kept.push(shape.to_string()),
            }
        }
        match (shapes.first(), shapes.last()) {
            (Some(&(first, _)), Some(&(_, last))) => {
                head = text[..first].to_string();
                tail = text[last..].to_string();
            }
            _ => {
                let end = text.rfind("</xml>").unwrap_or(text.len());
                head = text[..end].to_string();
                tail = text[end..].to_string();
            }
        }
        if !head.contains("_x0000_t202") {
            let at = head.find("</o:shapelayout>").map(|i| i + "</o:shapelayout>".len()).unwrap_or(head.len());
            head.insert_str(at, "<v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\"\r\n  path=\"m,l,21600r21600,l21600,xe\">\r\n  <v:stroke joinstyle=\"miter\"/>\r\n  <v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/>\r\n </v:shapetype>");
        }
    }
    // The drawing's number (its idmap), for new shapes' ids.
    let number = vml_part.as_ref().and_then(|p| p.trim_end_matches(".vml").rsplit(|c: char| !c.is_ascii_digit()).next().and_then(|d| d.parse::<u32>().ok())).unwrap_or(0);
    let new_part = match &vml_part {
        Some(p) => p.clone(),
        None => (1..).map(|i| format!("xl/drawings/vmlDrawing{i}.vml")).find(|c| !package.contains(c)).expect("a free name"),
    };
    let idmap = if existing.is_some() { number.max(1) } else { new_part.trim_end_matches(".vml").rsplit(|c: char| !c.is_ascii_digit()).next().and_then(|d| d.parse().ok()).unwrap_or(1) };
    if existing.is_none() {
        head = format!("<xml xmlns:v=\"urn:schemas-microsoft-com:vml\"\r\n xmlns:o=\"urn:schemas-microsoft-com:office:office\"\r\n xmlns:x=\"urn:schemas-microsoft-com:office:excel\">\r\n <o:shapelayout v:ext=\"edit\">\r\n  <o:idmap v:ext=\"edit\" data=\"{idmap}\"/>\r\n </o:shapelayout><v:shapetype id=\"_x0000_t202\" coordsize=\"21600,21600\" o:spt=\"202\"\r\n  path=\"m,l,21600r21600,l21600,xe\">\r\n  <v:stroke joinstyle=\"miter\"/>\r\n  <v:path gradientshapeok=\"t\" o:connecttype=\"rect\"/>\r\n </v:shapetype>");
        tail = "</xml>".to_string();
    }
    let mut next = max_id.max(idmap * 1024);
    for c in comments {
        if !have.contains(&(c.row, c.column)) {
            next += 1;
            kept.push(note_shape(next, c.row, c.column));
        }
    }
    let sheet_has = |package: &Package| -> Result<Option<NodeId>, Error> {
        let doc = package.xml(part)?;
        Ok(doc.root_element().and_then(|r| doc.child(r, "legacyDrawing")))
    };
    if kept.is_empty() {
        // Nothing left to draw: the drawing, its relationship and the element go.
        if let (Some(r), Some(p)) = (&rel, &vml_part) {
            package.remove(p);
            let _ = package.remove_override(p);
            package.remove_relationship(part, &r.id)?;
        }
        if sheet_has(package)?.is_some() {
            let doc = package.xml_mut(part)?;
            if let Some(e) = doc.root_element().and_then(|r| doc.child(r, "legacyDrawing")) {
                doc.detach(e);
            }
        }
        return Ok(());
    }
    let mut text = head;
    for shape in &kept {
        text.push_str(shape);
    }
    text.push_str(&tail);
    package.set_bytes(&new_part, text.into_bytes());
    let id = match &rel {
        Some(r) => r.id.clone(),
        None => {
            if package.content_type(&new_part)?.is_none() {
                package.set_override(&new_part, CT_VML)?;
            }
            package.add_relationship(part, REL_VML, &relative_target(part, &new_part), false)?
        }
    };
    if sheet_has(package)?.is_none() {
        let doc = package.xml_mut(part)?;
        let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
        let r = match doc.lookup_prefix(root, REL_NS) {
            Some(p) => p,
            None => {
                doc.set_attr(root, "xmlns:r", REL_NS);
                "r".into()
            }
        };
        let prefix = prefix_of(doc, root);
        let e = detached(doc, &format!("{prefix}legacyDrawing"));
        doc.set_attr(e, &format!("{r}:id"), &id);
        let before = ["legacyDrawingHF", "drawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"];
        let anchor = doc.elements(root).find(|&c| before.contains(&doc.local_name(c)));
        match anchor {
            Some(a) => doc.insert_before(a, e),
            None => {
                let n = doc.children(root).len();
                doc.insert(root, n, e);
            }
        }
    }
    Ok(())
}

const REL_THREADED_COMMENTS: &str = "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment";
const REL_PERSONS: &str = "http://schemas.microsoft.com/office/2017/10/relationships/person";
const CT_THREADED_COMMENTS: &str = "application/vnd.ms-excel.threadedcomments+xml";
const CT_PERSONS: &str = "application/vnd.ms-excel.person+xml";
const THREADED_NS: &str = "http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments";

/// A GUID made from `seed`, deterministic for repeatability.
fn threaded_guid(seed: &str) -> String {
    let hash = |offset: u64| seed.bytes().fold(offset, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    let (a, b) = (hash(0xcbf29ce484222325), hash(0x84222325cbf29ce4));
    format!(
        "{{{:08X}-{:04X}-4{:03X}-{:04X}-{:012X}}}",
        a >> 32,
        (a >> 16) & 0xffff,
        a & 0xfff,
        0x8000 | (b >> 48) & 0x3fff,
        b & 0xffff_ffff_ffff
    )
}

/// Ensure `xl/persons/person.xml` has an entry for each author in `authors`, returning displayName -> id map.
fn ensure_persons(package: &mut Package, workbook: &str, authors: &[String]) -> Result<HashMap<String, String>, Error> {
    let existing_rels = package.relationships(workbook)?;
    let person_rel = existing_rels.into_iter().find(|r| r.kind == REL_PERSONS && !r.external);
    let person_part = if let Some(rel) = person_rel {
        rel.target_part(workbook).unwrap_or_else(|| "xl/persons/person.xml".to_string())
    } else {
        let part = "xl/persons/person.xml".to_string();
        let rel_target = relative_target(workbook, &part);
        package.add_relationship(workbook, REL_PERSONS, &rel_target, false)?;
        package.set_override(&part, CT_PERSONS)?;
        part
    };

    let mut person_map: HashMap<String, String> = HashMap::new();
    let mut doc = if let Ok(d) = package.xml(&person_part) {
        if let Some(root) = d.root_element() {
            for &child in d.children(root) {
                if d.name(child) == "person" {
                    if let (Some(name), Some(id)) = (d.attr(child, "displayName"), d.attr(child, "id")) {
                        person_map.insert(name.to_string(), id.to_string());
                    }
                }
            }
        }
        package.xml_mut(&person_part)?.clone()
    } else {
        let mut d = XmlDocument::new_with_declaration();
        let root = d.append_element(XmlDocument::ROOT, "personList");
        d.set_attr(root, "xmlns", THREADED_NS);
        d.set_attr(root, "xmlns:x", MAIN_NS);
        d
    };

    let root = doc.root_element().ok_or_else(|| Error::new("personList root missing"))?;
    let mut changed = false;
    for author in authors {
        if !person_map.contains_key(author) {
            let id = threaded_guid(&format!("person:{author}"));
            person_map.insert(author.clone(), id.clone());
            let p_el = doc.append_element(root, "person");
            doc.set_attr(p_el, "displayName", author);
            doc.set_attr(p_el, "id", &id);
            doc.set_attr(p_el, "userId", author);
            doc.set_attr(p_el, "providerId", "None");
            changed = true;
        }
    }
    if changed || !package.contains(&person_part) {
        package.set_xml(&person_part, doc);
    }
    Ok(person_map)
}

/// Write threaded comments of a worksheet.
fn write_threaded_comments(
    package: &mut Package,
    workbook: &str,
    part: &str,
    threads: &[ibis_doc_model::ThreadedComment],
) -> Result<(), Error> {
    let existing_rels = package.relationships(part)?;
    let existing_rel = existing_rels.iter().find(|r| r.kind == REL_THREADED_COMMENTS && !r.external).cloned();

    if threads.is_empty() {
        if let Some(rel) = existing_rel {
            if let Some(target_part) = rel.target_part(part) {
                package.remove(&target_part);
                let _ = package.remove_override(&target_part);
            }
            package.remove_relationship(part, &rel.id)?;
        }
        return Ok(());
    }

    let mut authors = Vec::new();
    for t in threads {
        for p in &t.posts {
            let name = if p.author.is_empty() { "Author".to_string() } else { p.author.clone() };
            if !authors.contains(&name) {
                authors.push(name);
            }
        }
    }
    if authors.is_empty() {
        authors.push("Author".to_string());
    }
    let person_map = ensure_persons(package, workbook, &authors)?;

    let threads_part = if let Some(rel) = &existing_rel {
        rel.target_part(part).unwrap_or_else(|| "xl/threadedComments/threadedComment1.xml".to_string())
    } else {
        let mut idx = 1;
        loop {
            let candidate = format!("xl/threadedComments/threadedComment{idx}.xml");
            if !package.contains(&candidate) {
                break candidate;
            }
            idx += 1;
        }
    };

    if existing_rel.is_none() {
        let rel_target = relative_target(part, &threads_part);
        package.add_relationship(part, REL_THREADED_COMMENTS, &rel_target, false)?;
        package.set_override(&threads_part, CT_THREADED_COMMENTS)?;
    }

    let mut doc = XmlDocument::new_with_declaration();
    let root = doc.append_element(XmlDocument::ROOT, "ThreadedComments");
    doc.set_attr(root, "xmlns", THREADED_NS);
    doc.set_attr(root, "xmlns:x", MAIN_NS);

    for (t_idx, t) in threads.iter().enumerate() {
        let t_id = if t.id.is_empty() {
            threaded_guid(&format!("{part}:{}:{}:{}", t.row, t.column, t_idx))
        } else {
            t.id.clone()
        };
        let ref_str = cell_name(t.column, t.row);

        for (p_idx, p) in t.posts.iter().enumerate() {
            let c_el = doc.append_element(root, "threadedComment");
            doc.set_attr(c_el, "ref", &ref_str);
            let dt = if p.date.is_empty() {
                "2026-10-09T00:00:00.00"
            } else {
                &p.date
            };
            doc.set_attr(c_el, "dT", dt);
            let author_name = if p.author.is_empty() { "Author" } else { &p.author };
            let person_id = person_map.get(author_name).cloned().unwrap_or_else(|| threaded_guid(&format!("person:{author_name}")));
            doc.set_attr(c_el, "personId", &person_id);

            if p_idx == 0 {
                doc.set_attr(c_el, "id", &t_id);
                if t.done {
                    doc.set_attr(c_el, "done", "1");
                }
            } else {
                let reply_id = threaded_guid(&format!("{t_id}:{p_idx}:{}", p.date));
                doc.set_attr(c_el, "id", &reply_id);
                doc.set_attr(c_el, "parentId", &t_id);
            }

            let text_el = doc.append_element(c_el, "text");
            doc.set_text(text_el, &p.text);
        }
    }

    package.set_xml(&threads_part, doc);
    Ok(())
}

/// A dynamic array formula's `f` as Excel writes one that spills (or shows
/// `#SPILL!`): an array formula over the area it fills, its cell marked
/// with the dynamic array metadata (`cm`), including a one-cell result so
/// its dynamic calculation mode survives reopening.
///
/// A formula that gives one value and cannot give more (`SUM(A1:A3)`) is
/// written as a plain formula, as Excel writes it; one in a table always
/// (Excel does not open a table with a dynamic array formula in it).
fn write_dynamic(sheet: &mut SheetXml, c: NodeId, f: NodeId, new: &Sheet, row: u32, column: u32, cm: Option<&[String]>, multi_names: &HashSet<String>) {
    let Some(range) = new.spills.get(&(row, column)) else { return };
    let blocked = new.cell(row, column).is_some_and(|x| x.value == CellValue::Error(ibis_doc_model::CellError::Spill));
    let single = range.first_row == range.last_row && range.first_column == range.last_column;
    for a in ["t", "ref", "si"] {
        sheet.doc.remove_attr(f, a);
    }
    let existing_cm = sheet.doc.attr(c, "cm").map(str::to_string);
    sheet.doc.remove_attr(c, "cm");
    let in_table = new.tables.iter().any(|t| (t.range.first_row..=t.range.last_row).contains(&row) && (t.range.first_column..=t.range.last_column).contains(&column));
    let one_value = single && !blocked && new.cell(row, column).and_then(|x| x.formula.as_deref()).is_some_and(|formula| !ibis_calc::formula::arrays::can_return_array(formula, &|n| multi_names.contains(&n.to_lowercase())));
    if in_table || one_value {
        return;
    }
    let Some(indices) = cm else { return };
    let Some(default_cm) = indices.first() else { return };
    let cm = existing_cm.as_ref().filter(|index| crate::xlsx::metadata_index(index).is_some_and(|n| indices.iter().any(|v| crate::xlsx::metadata_index(v)==Some(n)))).unwrap_or(default_cm);
    let area = if blocked || single { cell_name(column, row) } else { format!("{}:{}", cell_name(range.first_column, range.first_row), cell_name(range.last_column, range.last_row)) };
    sheet.doc.set_attr(f, "t", "array");
    sheet.doc.set_attr(f, "ref", &area);
    sheet.doc.set_attr(c, "cm", cm);
}

/// Adding to the shared strings.
/// A shared string to add.
enum Added {
    Plain(String),
    Rich(Vec<ibis_doc_model::CellRun>),
    Phonetic { text: String, phonetic: String },
}

/// `t` under `parent` holding `text` (spaces kept where they matter).
fn write_t(doc: &mut XmlDocument, parent: NodeId, prefix: &str, text: &str) {
    let encoded = crate::xlsx::encode_xstring(text);
    let t = doc.append_element(parent, &format!("{prefix}t"));
    if encoded.starts_with(char::is_whitespace) || encoded.ends_with(char::is_whitespace) || encoded.contains('\n') {
        doc.set_attr(t, "xml:space", "preserve");
    }
    doc.set_text(t, &encoded);
}

struct StringsWriter {
    part: String,
    /// Plain strings (an `si` with just a `t`) by text.
    index: HashMap<String, u32>,
    count: u32,
    added: Vec<Added>,
    count_delta: i64,
    /// The part is new (the workbook had no shared strings).
    new_part: bool,
}

impl StringsWriter {
    fn open(package: &Package, map: &SourceMap) -> Result<Self, Error> {
        let Some(part) = map.shared_strings.clone() else {
            return Ok(Self { part: "xl/sharedStrings.xml".into(), index: HashMap::new(), count: 0, added: Vec::new(), count_delta: 0, new_part: true });
        };
        let doc = package.xml(&part)?;
        let root = doc.root_element().ok_or_else(|| Error::new("sharedStrings.xml が空です"))?;
        let mut index = HashMap::new();
        let mut count = 0u32;
        for si in doc.children_named(root, "si") {
            let elements: Vec<NodeId> = doc.elements(si).collect();
            if let [t] = elements[..]
                && doc.local_name(t) == "t"
            {
                index.entry(doc.text(t)).or_insert(count);
            }
            count += 1;
        }
        Ok(Self { part, index, count, added: Vec::new(), count_delta: 0, new_part: false })
    }

    fn index(&mut self, text: &str) -> u32 {
        if let Some(&i) = self.index.get(text) {
            return i;
        }
        let i = self.count + self.added.len() as u32;
        self.added.push(Added::Plain(text.to_string()));
        self.index.insert(text.to_string(), i);
        i
    }

    /// A string with a phonetic reading (`rPh`).
    fn index_phonetic(&mut self, text: &str, phonetic: &str) -> u32 {
        let key = format!("\u{2}{phonetic}\u{0}{text}");
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        let i = self.count + self.added.len() as u32;
        self.added.push(Added::Phonetic {
            text: text.to_string(),
            phonetic: phonetic.to_string(),
        });
        self.index.insert(key, i);
        i
    }

    /// A string of parts in their own fonts (found again by its parts, under a key no plain text has).
    fn index_rich(&mut self, runs: &[ibis_doc_model::CellRun]) -> u32 {
        let key = format!("\u{1}{runs:?}");
        if let Some(&i) = self.index.get(&key) {
            return i;
        }
        let i = self.count + self.added.len() as u32;
        self.added.push(Added::Rich(runs.to_vec()));
        self.index.insert(key, i);
        i
    }

    fn finish(self, package: &mut Package, workbook: &str) -> Result<(), Error> {
        if self.added.is_empty() && self.count_delta == 0 {
            return Ok(());
        }
        if self.new_part {
            let mut doc = XmlDocument::new_with_declaration();
            let root = doc.append_element(XmlDocument::ROOT, "sst");
            doc.set_attr(root, "xmlns", MAIN_NS);
            package.set_xml(&self.part, doc);
            package.set_override(&self.part, CT_SHARED_STRINGS)?;
            package.add_relationship(workbook, REL_SHARED_STRINGS, &relative_target(workbook, &self.part), false)?;
        }
        let doc = package.xml_mut(&self.part)?;
        let root = doc.root_element().ok_or_else(|| Error::new("sharedStrings.xml が空です"))?;
        let prefix = prefix_of(doc, root);
        let ext = doc.child(root, "extLst");
        for added in &self.added {
            let si = detached(doc, &format!("{prefix}si"));
            match ext {
                Some(ext) => doc.insert_before(ext, si),
                None => {
                    let n = doc.children(root).len();
                    doc.insert(root, n, si);
                }
            }
            match added {
                Added::Plain(text) => write_t(doc, si, &prefix, text),
                Added::Phonetic { text, phonetic } => {
                    write_t(doc, si, &prefix, text);
                    let r_ph = doc.append_element(si, &format!("{prefix}rPh"));
                    let len = text.encode_utf16().count();
                    doc.set_attr(r_ph, "sb", "0");
                    doc.set_attr(r_ph, "eb", &len.to_string());
                    write_t(doc, r_ph, &prefix, phonetic);
                }
                Added::Rich(runs) => {
                    for run in runs {
                        let r = doc.append_element(si, &format!("{prefix}r"));
                        let props = [run.font.is_some(), run.size.is_some(), run.bold.is_some(), run.italic.is_some(), run.underline.is_some(), run.strike.is_some(), run.color.is_some()];
                        if props.contains(&true) {
                            let p = doc.append_element(r, &format!("{prefix}rPr"));
                            if let Some(f) = &run.font {
                                let e = doc.append_element(p, &format!("{prefix}rFont"));
                                doc.set_attr(e, "val", f);
                            }
                            for (name, on) in [("b", run.bold), ("i", run.italic), ("strike", run.strike)] {
                                if on == Some(true) {
                                    doc.append_element(p, &format!("{prefix}{name}"));
                                }
                            }
                            if let Some(c) = run.color {
                                let e = doc.append_element(p, &format!("{prefix}color"));
                                doc.set_attr(e, "rgb", &argb(c));
                            }
                            if let Some(s) = run.size {
                                let e = doc.append_element(p, &format!("{prefix}sz"));
                                doc.set_attr(e, "val", &number_to_xml(s as f64 / 100.0));
                            }
                            if let Some(u) = run.underline.filter(|u| u.enabled()) {
                                let e = doc.append_element(p, &format!("{prefix}u"));
                                if u != ibis_doc_model::CellUnderline::Single {
                                    doc.set_attr(e, "val", u.ooxml());
                                }
                            }
                        }
                        write_t(doc, r, &prefix, &run.text);
                    }
                }
            }
        }
        let unique = doc.children_named(root, "si").count() as i64;
        let count = match doc.attr(root, "count").and_then(|v| v.parse::<i64>().ok()) {
            Some(c) => (c + self.count_delta).max(0),
            None => self.count_delta.max(0),
        };
        doc.set_attr(root, "count", &count.to_string());
        doc.set_attr(root, "uniqueCount", &unique.to_string());
        Ok(())
    }
}

/// `part` relative to the folder of `from` (both part names).
fn relative_target(from: &str, part: &str) -> String {
    let dir = from.rsplit_once('/').map_or("", |(d, _)| d);
    if !dir.is_empty()
        && let Some(rest) = part.strip_prefix(&format!("{dir}/"))
    {
        return rest.to_string();
    }
    format!("/{part}")
}

/// Making `cellXfs` entries for changed formats.
struct StylesWriter {
    /// Each read format (cellXfs index) as the model holds it.
    read: HashMap<u32, CellStyle>,
    /// Formats made in this save, by the model's style.
    made: HashMap<CellStyle, u32>,
}

impl StylesWriter {
    fn new(baseline: &Presentation) -> Self {
        let mut read = HashMap::new();
        let mut add = |s: &CellStyle| {
            if let Some(xf) = s.xf {
                read.entry(xf).or_insert_with(|| s.clone());
            }
        };
        if let Some(wb) = baseline.workbook.as_deref() {
            if let Some(d) = &wb.default_style {
                add(d);
            }
            for s in wb.sheets.iter().filter_map(|s| s.hidden.as_deref()) {
                s.styles.iter().for_each(&mut add);
            }
        }
        for slide in &baseline.slides {
            if let Some(sheet) = sheet_of(slide) {
                sheet.styles.iter().for_each(&mut add);
            }
        }
        Self { read, made: HashMap::new() }
    }

    fn xf_for(&mut self, package: &mut Package, part: Option<&str>, style: &CellStyle) -> Result<u32, Error> {
        let base_xf = style.xf.unwrap_or(0);
        let base = self.read.get(&base_xf).cloned();
        if base.as_ref() == Some(style) {
            return Ok(base_xf);
        }
        if let Some(&xf) = self.made.get(style) {
            return Ok(xf);
        }
        let part = part.ok_or_else(|| unsupported("書式のないブックの書式の変更"))?;
        let base = base.unwrap_or_else(|| CellStyle { xf: Some(base_xf), ..CellStyle::default() });
        let doc = package.xml_mut(part)?;
        let xf = new_xf(doc, base_xf, &base, style)?;
        self.made.insert(style.clone(), xf);
        Ok(xf)
    }
}

/// A list under the styles part's root (`fonts`, `cellXfs`…), made if
/// missing, before the first of `before` that is there (the schema's order).
fn styles_list(doc: &mut XmlDocument, root: NodeId, local: &str, before: &[&str]) -> NodeId {
    if let Some(n) = doc.child(root, local) {
        return n;
    }
    let n = detached(doc, &format!("{}{local}", prefix_of(doc, root)));
    match before.iter().find_map(|b| doc.child(root, b)) {
        Some(a) => doc.insert_before(a, n),
        None => {
            let len = doc.children(root).len();
            doc.insert(root, len, n);
        }
    }
    doc.set_attr(n, "count", "0");
    n
}

/// Append `item` to a list and fix its count: the new item's index.
fn append_item(doc: &mut XmlDocument, list: NodeId, item: NodeId, local: &str) -> u32 {
    let n = doc.children(list).len();
    doc.insert(list, n, item);
    let count = doc.children_named(list, local).count();
    doc.set_attr(list, "count", &count.to_string());
    (count - 1) as u32
}

/// A copy of item `index` of `list` (a new empty `local` if there is none).
fn copy_item(doc: &mut XmlDocument, list: NodeId, local: &str, index: usize, prefix: &str) -> NodeId {
    let found = doc.children_named(list, local).nth(index);
    match found {
        Some(x) => doc.deep_clone(x),
        None => detached(doc, &format!("{prefix}{local}")),
    }
}

/// A colour element naming a theme colour (`theme`, and `tint` unless 0).
fn set_theme_color(doc: &mut XmlDocument, e: NodeId, t: ibis_doc_model::ThemeColor) {
    doc.set_attr(e, "theme", &t.index.to_string());
    if t.tint != 0 {
        // Excel writes the palette's nominal tint (0.8, -0.25) as its value times 32767, cut, over 32767.
        let tint = if t.tint % 10_000 == 0 {
            let nominal = t.tint as f64 / 1_000_000.0;
            ((nominal * 32767.0).trunc()) / 32767.0
        } else {
            t.tint as f64 / 1_000_000.0
        };
        doc.set_attr(e, "tint", &number_to_xml(tint));
    }
}

fn argb(c: Rgb) -> String {
    format!("FF{:06X}", c.0 & 0xFF_FFFF)
}

/// A pattern's SpreadsheetML name (`patternType`) for the DrawingML name the
/// model keeps (`xlsx.rs` reads one as the other).
fn xlsx_pattern(preset: &str) -> &str {
    match preset {
        "pct5" => "gray0625",
        "pct10" => "gray125",
        "pct25" => "lightGray",
        "pct50" => "mediumGray",
        "pct75" => "darkGray",
        "ltHorz" => "lightHorizontal",
        "dkHorz" => "darkHorizontal",
        "ltVert" => "lightVertical",
        "dkVert" => "darkVertical",
        "ltDnDiag" => "lightDown",
        "dkDnDiag" => "darkDown",
        "ltUpDiag" => "lightUp",
        "dkUpDiag" => "darkUp",
        "smGrid" => "lightGrid",
        "cross" => "darkGrid",
        "diagCross" => "lightTrellis",
        "trellis" => "darkTrellis",
        other => other,
    }
}

fn write_gradient_fill(doc: &mut XmlDocument, fills: NodeId, gradient: &CellGradient, prefix: &str) -> u32 {
    let fill = detached(doc, &format!("{prefix}fill"));
    let g = doc.append_element(fill, &format!("{prefix}gradientFill"));
    let stops = match gradient {
        CellGradient::Linear { degree, stops } => {
            doc.set_attr(g, "degree", &degree.to_string());
            stops
        }
        CellGradient::Path { focus, stops, .. } => {
            doc.set_attr(g, "type", "path");
            let l = focus[0] as f64 / 100_000.0;
            let t = focus[1] as f64 / 100_000.0;
            let r = (100_000 - focus[2]) as f64 / 100_000.0;
            let b = (100_000 - focus[3]) as f64 / 100_000.0;
            doc.set_attr(g, "left", &format!("{:.4}", l));
            doc.set_attr(g, "top", &format!("{:.4}", t));
            doc.set_attr(g, "right", &format!("{:.4}", r));
            doc.set_attr(g, "bottom", &format!("{:.4}", b));
            stops
        }
    };
    for (pos, col) in stops {
        let s = doc.append_element(g, &format!("{prefix}stop"));
        let frac = *pos as f64 / 1000.0;
        doc.set_attr(s, "position", &format!("{:.2}", frac));
        let c = doc.append_element(s, &format!("{prefix}color"));
        doc.set_attr(c, "rgb", &argb(*col));
    }
    append_item(doc, fills, fill, "fill")
}

/// Make a cellXfs entry for `style` from entry `base_xf` (read as `base`).
fn new_xf(doc: &mut XmlDocument, base_xf: u32, base: &CellStyle, style: &CellStyle) -> Result<u32, Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("styles.xml が空です"))?;
    let prefix = prefix_of(doc, root);
    let cell_xfs = styles_list(doc, root, "cellXfs", &["cellStyles", "dxfs", "tableStyles", "colors", "extLst"]);
    let xf = copy_item(doc, cell_xfs, "xf", base_xf as usize, &prefix);
    let index_of = |doc: &XmlDocument, attr: &str| doc.attr(xf, attr).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);

    let font_of = |s: &CellStyle| (s.font.clone(), s.font_size, s.bold, s.italic, s.underline, s.strike, s.color, s.color_theme, s.vert_align);
    if font_of(style) != font_of(base) {
        let fonts = styles_list(doc, root, "fonts", &["fills", "borders", "cellStyleXfs", "cellXfs"]);
        let at = index_of(doc, "fontId");
        let font = copy_item(doc, fonts, "font", at, &prefix);
        write_font(doc, font, base, style, &prefix);
        let id = append_item(doc, fonts, font, "font");
        doc.set_attr(xf, "fontId", &id.to_string());
        doc.set_attr(xf, "applyFont", "1");
    }

    if style.fill != base.fill || style.pattern != base.pattern || style.fill_theme != base.fill_theme || style.gradient != base.gradient {
        let fills = styles_list(doc, root, "fills", &["borders", "cellStyleXfs", "cellXfs"]);
        let id = match (&style.gradient, &style.pattern, style.fill) {
            (Some(gradient), _, _) => write_gradient_fill(doc, fills, gradient, &prefix),
            (None, Some((preset, fg, bg)), _) => {
                let fill = detached(doc, &format!("{prefix}fill"));
                let p = doc.append_element(fill, &format!("{prefix}patternFill"));
                // The model names it as DrawingML does (`ltUpDiag`); the file as SpreadsheetML (`lightUp`).
                doc.set_attr(p, "patternType", xlsx_pattern(preset));
                let f = doc.append_element(p, &format!("{prefix}fgColor"));
                doc.set_attr(f, "rgb", &argb(*fg));
                let b = doc.append_element(p, &format!("{prefix}bgColor"));
                doc.set_attr(b, "rgb", &argb(*bg));
                append_item(doc, fills, fill, "fill")
            }
            // Fill 0 is "none" by Excel's rule.
            (None, None, None) => 0,
            (None, None, Some(color)) => {
                let fill = detached(doc, &format!("{prefix}fill"));
                let p = doc.append_element(fill, &format!("{prefix}patternFill"));
                doc.set_attr(p, "patternType", "solid");
                let fg = doc.append_element(p, &format!("{prefix}fgColor"));
                match style.fill_theme {
                    Some(t) => set_theme_color(doc, fg, t),
                    None => doc.set_attr(fg, "rgb", &argb(color)),
                }
                let bg = doc.append_element(p, &format!("{prefix}bgColor"));
                doc.set_attr(bg, "indexed", "64");
                append_item(doc, fills, fill, "fill")
            }
        };
        doc.set_attr(xf, "fillId", &id.to_string());
        doc.set_attr(xf, "applyFill", "1");
    }

    let edges = |s: &CellStyle| (s.left, s.right, s.top, s.bottom, s.diagonal_up, s.diagonal_down);
    if edges(style) != edges(base) {
        let borders = styles_list(doc, root, "borders", &["cellStyleXfs", "cellXfs"]);
        let at = index_of(doc, "borderId");
        let border = copy_item(doc, borders, "border", at, &prefix);
        write_border(doc, border, base, style, &prefix);
        let id = append_item(doc, borders, border, "border");
        doc.set_attr(xf, "borderId", &id.to_string());
        doc.set_attr(xf, "applyBorder", "1");
    }

    let alignment = |s: &CellStyle| (s.align, s.vertical, s.wrap, s.indent, s.shrink, s.rotation);
    if alignment(style) != alignment(base) {
        let a = match doc.child(xf, "alignment") {
            Some(a) => a,
            None => doc.insert_element(xf, 0, &format!("{prefix}alignment")),
        };
        let set = |doc: &mut XmlDocument, attr: &str, value: Option<&str>| match value {
            Some(v) => doc.set_attr(a, attr, v),
            None => doc.remove_attr(a, attr),
        };
        if style.align != base.align {
            let v = match style.align {
                CellAlign::General => None,
                CellAlign::Left => Some("left"),
                CellAlign::Center => Some("center"),
                CellAlign::Right => Some("right"),
                CellAlign::Fill => Some("fill"),
                CellAlign::Justify => Some("justify"),
                CellAlign::CenterAcross => Some("centerContinuous"),
                CellAlign::Distributed => Some("distributed"),
            };
            set(doc, "horizontal", v);
        }
        if style.vertical != base.vertical {
            let v = match style.vertical {
                CellVerticalAlign::Top => Some("top"),
                CellVerticalAlign::Center => Some("center"),
                CellVerticalAlign::Bottom => None,
                CellVerticalAlign::Justify => Some("justify"),
                CellVerticalAlign::Distributed => Some("distributed"),
            };
            set(doc, "vertical", v);
        }
        if style.wrap != base.wrap {
            set(doc, "wrapText", style.wrap.then_some("1"));
        }
        if style.indent != base.indent {
            let v = style.indent.to_string();
            set(doc, "indent", (style.indent > 0).then_some(v.as_str()));
        }
        if style.shrink != base.shrink {
            set(doc, "shrinkToFit", style.shrink.then_some("1"));
        }
        if style.rotation != base.rotation {
            let v = style.rotation.to_string();
            set(doc, "textRotation", (style.rotation > 0).then_some(v.as_str()));
        }
        doc.set_attr(xf, "applyAlignment", "1");
    }

    if (style.unlocked, style.formula_hidden) != (base.unlocked, base.formula_hidden) {
        // After alignment, before extLst.
        let p = match doc.child(xf, "protection") {
            Some(p) => p,
            None => {
                let p = detached(doc, &format!("{prefix}protection"));
                match doc.child(xf, "extLst") {
                    Some(ext) => doc.insert_before(ext, p),
                    None => {
                        let n = doc.children(xf).len();
                        doc.insert(xf, n, p);
                    }
                }
                p
            }
        };
        if style.unlocked {
            doc.set_attr(p, "locked", "0");
        } else {
            doc.remove_attr(p, "locked");
        }
        if style.formula_hidden {
            doc.set_attr(p, "hidden", "1");
        } else {
            doc.remove_attr(p, "hidden");
        }
        doc.set_attr(xf, "applyProtection", "1");
    }

    if style.number_format != base.number_format {
        let id = number_format_id(doc, root, &style.number_format, &prefix);
        doc.set_attr(xf, "numFmtId", &id.to_string());
        doc.set_attr(xf, "applyNumberFormat", "1");
    }

    Ok(append_item(doc, cell_xfs, xf, "xf"))
}

fn write_font(doc: &mut XmlDocument, font: NodeId, base: &CellStyle, style: &CellStyle, prefix: &str) {
    // The flags first, in the schema's order (b, i, strike, u).
    if style.underline != base.underline {
        while let Some(n) = doc.child(font, "u") { doc.detach(n); }
        if style.underline.enabled() {
            let u = doc.insert_element(font, 0, &format!("{prefix}u"));
            doc.set_attr(u, "val", style.underline.ooxml());
        }
    }
    for (local, on, was) in [("strike", style.strike, base.strike), ("i", style.italic, base.italic), ("b", style.bold, base.bold)] {
        if on == was {
            continue;
        }
        while let Some(n) = doc.child(font, local) {
            doc.detach(n);
        }
        if on {
            doc.insert_element(font, 0, &format!("{prefix}{local}"));
        }
    }
    let element = |doc: &mut XmlDocument, local: &str| match doc.child(font, local) {
        Some(e) => e,
        None => doc.append_element(font, &format!("{prefix}{local}")),
    };
    if style.vert_align != base.vert_align {
        while let Some(n) = doc.child(font, "vertAlign") {
            doc.detach(n);
        }
        let v = match style.vert_align {
            ibis_doc_model::VerticalPosition::Superscript => Some("superscript"),
            ibis_doc_model::VerticalPosition::Subscript => Some("subscript"),
            _ => None,
        };
        if let Some(v) = v {
            let e = detached(doc, &format!("{prefix}vertAlign"));
            doc.set_attr(e, "val", v);
            // After u, before sz and the rest (the schema's order).
            let after = ["sz", "color", "name", "family", "charset", "scheme", "extLst"];
            let at = doc.elements(font).find(|&c| after.contains(&doc.local_name(c)));
            match at {
                Some(a) => doc.insert_before(a, e),
                None => {
                    let n = doc.children(font).len();
                    doc.insert(font, n, e);
                }
            }
        }
    }
    if style.font_size != base.font_size {
        let e = element(doc, "sz");
        doc.set_attr(e, "val", &number_to_xml(style.font_size as f64 / 100.0));
    }
    if style.color != base.color || style.color_theme != base.color_theme {
        let e = element(doc, "color");
        for a in ["theme", "tint", "indexed", "auto", "rgb"] {
            doc.remove_attr(e, a);
        }
        match style.color_theme {
            Some(t) => set_theme_color(doc, e, t),
            None => doc.set_attr(e, "rgb", &argb(style.color)),
        }
    }
    if style.font != base.font
        && let Some(name) = &style.font
    {
        let e = element(doc, "name");
        doc.set_attr(e, "val", name);
        // The theme's font no longer applies.
        if let Some(s) = doc.child(font, "scheme") {
            doc.detach(s);
        }
    }
}

fn write_border(doc: &mut XmlDocument, border: NodeId, base: &CellStyle, style: &CellStyle, prefix: &str) {
    const ORDER: [&str; 9] = ["start", "left", "end", "right", "top", "bottom", "diagonal", "vertical", "horizontal"];
    let set_edge = |doc: &mut XmlDocument, local: &str, new: Option<Border>, old: Option<Border>| {
        if new == old {
            return;
        }
        let e = match doc.child(border, local) {
            Some(e) => e,
            None => {
                let e = detached(doc, &format!("{prefix}{local}"));
                let pos = ORDER.iter().position(|o| *o == local).unwrap_or(0);
                match ORDER[pos + 1..].iter().find_map(|o| doc.child(border, o)) {
                    Some(a) => doc.insert_before(a, e),
                    None => {
                        let n = doc.children(border).len();
                        doc.insert(border, n, e);
                    }
                }
                e
            }
        };
        for c in doc.children(e).to_vec() {
            doc.detach(c);
        }
        match new {
            None => doc.remove_attr(e, "style"),
            Some(b) => {
                doc.set_attr(e, "style", border_style(b.line));
                let c = doc.append_element(e, &format!("{prefix}color"));
                doc.set_attr(c, "rgb", &argb(b.color));
            }
        }
    };
    set_edge(doc, "left", style.left, base.left);
    set_edge(doc, "right", style.right, base.right);
    set_edge(doc, "top", style.top, base.top);
    set_edge(doc, "bottom", style.bottom, base.bottom);
    if (style.diagonal_up, style.diagonal_down) != (base.diagonal_up, base.diagonal_down) {
        set_edge(doc, "diagonal", style.diagonal_up.or(style.diagonal_down), base.diagonal_up.or(base.diagonal_down));
        for (attr, on) in [("diagonalUp", style.diagonal_up.is_some()), ("diagonalDown", style.diagonal_down.is_some())] {
            if on {
                doc.set_attr(border, attr, "1");
            } else {
                doc.remove_attr(border, attr);
            }
        }
    }
}

/// The id of a number format: a built-in one, one already defined, or a
/// new one (164 and up).
fn number_format_id(doc: &mut XmlDocument, root: NodeId, code: &str, prefix: &str) -> u32 {
    if let Some(id) = crate::xlsx::builtin_format_id(code) {
        return id;
    }
    let num_fmts = doc.child(root, "numFmts");
    if let Some(list) = num_fmts {
        for n in doc.children_named(list, "numFmt") {
            if doc.attr(n, "formatCode") == Some(code)
                && let Some(id) = doc.attr(n, "numFmtId").and_then(|v| v.parse().ok())
            {
                return id;
            }
        }
    }
    let list = match num_fmts {
        Some(l) => l,
        None => {
            // numFmts comes first in styleSheet.
            let l = detached(doc, &format!("{prefix}numFmts"));
            let first = doc.elements(root).next();
            match first {
                Some(f) => doc.insert_before(f, l),
                None => doc.insert(root, 0, l),
            }
            l
        }
    };
    let next = doc.children_named(list, "numFmt").filter_map(|n| doc.attr(n, "numFmtId").and_then(|v| v.parse::<u32>().ok())).max().map_or(164, |m| (m + 1).max(164));
    let n = doc.append_element(list, &format!("{prefix}numFmt"));
    doc.set_attr(n, "numFmtId", &next.to_string());
    doc.set_attr(n, "formatCode", code);
    let count = doc.children_named(list, "numFmt").count();
    doc.set_attr(list, "count", &count.to_string());
    next
}

fn border_style(line: BorderLine) -> &'static str {
    line.ooxml()
}

fn write_custom_cell_styles(package: &mut Package, styles_part: Option<&str>, styles: &[ibis_doc_model::CustomCellStyle]) -> Result<(), Error> {
    let Some(part) = styles_part else {
        return Ok(());
    };
    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| Error::new("styles.xml が空です"))?;
    let prefix = prefix_of(doc, root);

    let cell_style_xfs = styles_list(doc, root, "cellStyleXfs", &["cellXfs", "cellStyles", "dxfs", "tableStyles", "colors", "extLst"]);
    let cell_styles = styles_list(doc, root, "cellStyles", &["dxfs", "tableStyles", "colors", "extLst"]);

    // Remove previously saved custom styles from cellStyles:
    let existing_cs: Vec<_> = doc.children_named(cell_styles, "cellStyle").collect();
    for cs in existing_cs {
        let name = doc.attr(cs, "name").unwrap_or("");
        let builtin = doc.attr(cs, "builtinId");
        if builtin.is_none() && !name.eq_ignore_ascii_case("Normal") && name != "標準" {
            doc.detach(cs);
        }
    }

    for s in styles {
        let xf = detached(doc, &format!("{prefix}xf"));
        let mut num_fmt_id = 0u32;
        let mut font_id = 0u32;
        let mut fill_id = 0u32;
        let mut border_id = 0u32;

        if s.include_number_format {
            num_fmt_id = number_format_id(doc, root, &s.format.number_format, &prefix);
            doc.set_attr(xf, "applyNumberFormat", "1");
        } else {
            doc.set_attr(xf, "applyNumberFormat", "0");
        }
        doc.set_attr(xf, "numFmtId", &num_fmt_id.to_string());

        if s.include_font {
            let fonts = styles_list(doc, root, "fonts", &["fills", "borders", "cellStyleXfs", "cellXfs"]);
            let font = copy_item(doc, fonts, "font", 0, &prefix);
            write_font(doc, font, &CellStyle::default(), &s.format, &prefix);
            font_id = append_item(doc, fonts, font, "font");
            doc.set_attr(xf, "applyFont", "1");
        } else {
            doc.set_attr(xf, "applyFont", "0");
        }
        doc.set_attr(xf, "fontId", &font_id.to_string());

        if s.include_fill {
            let fills = styles_list(doc, root, "fills", &["borders", "cellStyleXfs", "cellXfs"]);
            fill_id = match (&s.format.gradient, &s.format.pattern, s.format.fill) {
                (Some(gradient), _, _) => write_gradient_fill(doc, fills, gradient, &prefix),
                (None, Some((preset, fg, bg)), _) => {
                    let fill = detached(doc, &format!("{prefix}fill"));
                    let p = doc.append_element(fill, &format!("{prefix}patternFill"));
                    doc.set_attr(p, "patternType", xlsx_pattern(preset));
                    let f = doc.append_element(p, &format!("{prefix}fgColor"));
                    doc.set_attr(f, "rgb", &argb(*fg));
                    let b = doc.append_element(p, &format!("{prefix}bgColor"));
                    doc.set_attr(b, "rgb", &argb(*bg));
                    append_item(doc, fills, fill, "fill")
                }
                (None, None, Some(color)) => {
                    let fill = detached(doc, &format!("{prefix}fill"));
                    let p = doc.append_element(fill, &format!("{prefix}patternFill"));
                    doc.set_attr(p, "patternType", "solid");
                    let fg = doc.append_element(p, &format!("{prefix}fgColor"));
                    match s.format.fill_theme {
                        Some(t) => set_theme_color(doc, fg, t),
                        None => doc.set_attr(fg, "rgb", &argb(color)),
                    }
                    let bg = doc.append_element(p, &format!("{prefix}bgColor"));
                    doc.set_attr(bg, "indexed", "64");
                    append_item(doc, fills, fill, "fill")
                }
                (None, None, None) => 0,
            };
            doc.set_attr(xf, "applyFill", "1");
        } else {
            doc.set_attr(xf, "applyFill", "0");
        }
        doc.set_attr(xf, "fillId", &fill_id.to_string());

        if s.include_border {
            let borders = styles_list(doc, root, "borders", &["cellStyleXfs", "cellXfs"]);
            let border = copy_item(doc, borders, "border", 0, &prefix);
            write_border(doc, border, &CellStyle::default(), &s.format, &prefix);
            border_id = append_item(doc, borders, border, "border");
            doc.set_attr(xf, "applyBorder", "1");
        } else {
            doc.set_attr(xf, "applyBorder", "0");
        }
        doc.set_attr(xf, "borderId", &border_id.to_string());

        if s.include_alignment {
            let a = doc.append_element(xf, &format!("{prefix}alignment"));
            let h = match s.format.align {
                CellAlign::Left => Some("left"),
                CellAlign::Center => Some("center"),
                CellAlign::Right => Some("right"),
                CellAlign::Fill => Some("fill"),
                CellAlign::Justify => Some("justify"),
                CellAlign::CenterAcross => Some("centerContinuous"),
                CellAlign::Distributed => Some("distributed"),
                _ => None,
            };
            if let Some(v) = h {
                doc.set_attr(a, "horizontal", v);
            }
            let v = match s.format.vertical {
                CellVerticalAlign::Top => Some("top"),
                CellVerticalAlign::Center => Some("center"),
                CellVerticalAlign::Justify => Some("justify"),
                CellVerticalAlign::Distributed => Some("distributed"),
                _ => None,
            };
            if let Some(v) = v {
                doc.set_attr(a, "vertical", v);
            }
            if s.format.wrap {
                doc.set_attr(a, "wrapText", "1");
            }
            if s.format.indent > 0 {
                doc.set_attr(a, "indent", &s.format.indent.to_string());
            }
            if s.format.shrink {
                doc.set_attr(a, "shrinkToFit", "1");
            }
            if s.format.rotation > 0 {
                doc.set_attr(a, "textRotation", &s.format.rotation.to_string());
            }
            doc.set_attr(xf, "applyAlignment", "1");
        } else {
            doc.set_attr(xf, "applyAlignment", "0");
        }

        if s.include_protection {
            let p = doc.append_element(xf, &format!("{prefix}protection"));
            if s.format.unlocked {
                doc.set_attr(p, "locked", "0");
            }
            if s.format.formula_hidden {
                doc.set_attr(p, "hidden", "1");
            }
            doc.set_attr(xf, "applyProtection", "1");
        } else {
            doc.set_attr(xf, "applyProtection", "0");
        }

        let xf_id = append_item(doc, cell_style_xfs, xf, "xf");

        let cs = detached(doc, &format!("{prefix}cellStyle"));
        doc.set_attr(cs, "name", &s.name);
        doc.set_attr(cs, "xfId", &xf_id.to_string());
        append_item(doc, cell_styles, cs, "cellStyle");
    }

    Ok(())
}

