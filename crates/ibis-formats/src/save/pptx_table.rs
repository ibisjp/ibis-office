// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Tables: the text of a table's cells written into its `a:tc` elements
//! (`a:graphicFrame/a:graphic/a:graphicData/a:tbl/a:tr/a:tc`), keeping each
//! cell's paragraphs' and runs' own formatting as a shape's text does
//! (`pptx_text::rewrite_text`); rows and columns inserted, removed and
//! resized (a new one a copy of the one it was made like, its text
//! emptied); a cell's fill.

use ibis_doc_model::{Fill, Table, TableCell, TextBody};
use ibis_ooxml::{NodeId, XmlDocument};

use super::pptx::{ParagraphSource, ShapeSource};
use crate::Error;
use crate::save::unsupported;

/// Whether `a` and `b` differ only in their cells' text, fills and merges
/// (spans and covered cells).
fn only_text_and_fill_differ(a: &Table, b: &Table) -> bool {
    let shape = |t: &Table| Table {
        rows: t
            .rows
            .iter()
            .map(|r| ibis_doc_model::TableRow {
                cells: r
                    .cells
                    .iter()
                    .map(|c| TableCell { text: TextBody::default(), fill: Fill::None, column_span: 1, row_span: 1, merged: false, borders: Default::default(), ..c.clone() })
                    .collect(),
                ..r.clone()
            })
            .collect(),
        style_options: Default::default(),
        style_id: None,
        ..t.clone()
    };
    shape(a) == shape(b)
}

/// Write cell `tc`'s span and whether it is covered (`gridSpan`, `rowSpan`,
/// `hMerge`, `vMerge`): `origin` is the cell covering it, if any.
fn write_cell_merge(doc: &mut XmlDocument, tc: NodeId, cell: &TableCell, covered_from: Option<(bool, bool)>) {
    let set = |doc: &mut XmlDocument, name: &str, value: Option<String>| match value {
        Some(v) => doc.set_attr(tc, name, &v),
        None => doc.remove_attr(tc, name),
    };
    set(doc, "gridSpan", (cell.column_span > 1).then(|| cell.column_span.to_string()));
    set(doc, "rowSpan", (cell.row_span > 1).then(|| cell.row_span.to_string()));
    let (h, v) = covered_from.unwrap_or((false, false));
    set(doc, "hMerge", h.then(|| "1".to_string()));
    set(doc, "vMerge", v.then(|| "1".to_string()));
}

/// Write `fill` as cell `tc`'s own (`a:tcPr`), in place of the fill it had.
fn write_cell_fill(doc: &mut XmlDocument, tc: NodeId, previous: &Fill, fill: &Fill) -> Result<(), Error> {
    let pr = match doc.child(tc, "tcPr") {
        Some(pr) => pr,
        None => {
            // `a:tcPr` after `a:txBody`, before `a:extLst`.
            let a = super::pptx_new::Prefixes::at(doc, tc).a;
            let index = doc.children(tc).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "extLst").unwrap_or(doc.children(tc).len());
            doc.insert_element(tc, index, &format!("{a}tcPr"))
        }
    };
    super::pptx_new::check_fill(fill)?;
    if super::pptx_shapes::write_fill_properties(doc, pr, previous, fill)? { return Ok(()); }
    let old: Vec<NodeId> = doc.children(pr).iter().copied().filter(|&c| doc.is_element(c) && doc.namespace(c)==Some(super::pptx_new::NS_A) && FILLS.contains(&doc.local_name(c))).collect();
    for n in old {
        doc.detach(n);
    }
    let x = super::pptx_new::Prefixes::at(doc, pr);
    let element = x.import(doc, &super::pptx_new::fill_xml(&x, fill)?)?;
    // After the borders and `a:cell3D`, before `a:headers` and `a:extLst`.
    let index = doc.children(pr).iter().position(|&c| doc.is_element(c) && matches!(doc.local_name(c), "headers" | "extLst")).unwrap_or(doc.children(pr).len());
    doc.insert(pr, index, element);
    Ok(())
}

/// Write the edges of cell `tc` that changed from `old` to `new`
/// (`a:lnL`, `a:lnR`, `a:lnT`, `a:lnB` of its `a:tcPr`; none is `a:noFill`).
fn write_cell_borders(doc: &mut XmlDocument, tc: NodeId, old: &ibis_doc_model::CellBorders, new: &ibis_doc_model::CellBorders) -> Result<(), Error> {
    let pr = match doc.child(tc, "tcPr") {
        Some(pr) => pr,
        None => {
            let a = super::pptx_new::Prefixes::at(doc, tc).a;
            let index = doc.children(tc).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "extLst").unwrap_or(doc.children(tc).len());
            doc.insert_element(tc, index, &format!("{a}tcPr"))
        }
    };
    let x = super::pptx_new::Prefixes::at(doc, pr);
    let a = x.a.clone();
    const ORDER: [&str; 6] = ["lnL", "lnR", "lnT", "lnB", "lnTlToBr", "lnBlToTr"];
    for (i, (name, was, is)) in [
        ("lnL", &old.left, &new.left),
        ("lnR", &old.right, &new.right),
        ("lnT", &old.top, &new.top),
        ("lnB", &old.bottom, &new.bottom),
        ("lnTlToBr", &old.tl_to_br, &new.tl_to_br),
        ("lnBlToTr", &old.bl_to_tr, &new.bl_to_tr),
    ].into_iter().enumerate() {
        if was == is {
            continue;
        }
        if let Some(n) = doc.child(pr, name) {
            doc.detach(n);
        }
        let inner = super::pptx_new::line_xml(&x, is.as_ref());
        // `a:ln` written under the edge's name.
        let xml = inner.replacen(&format!("<{a}ln"), &format!("<{a}{name}"), 1).replace(&format!("</{a}ln>"), &format!("</{a}{name}>"));
        let element = x.import(doc, &xml)?;
        // In the edges' order, before everything else in `a:tcPr`.
        let index = doc
            .children(pr)
            .iter()
            .position(|&c| doc.is_element(c) && ORDER.iter().position(|o| *o == doc.local_name(c)).is_none_or(|j| j > i))
            .unwrap_or(doc.children(pr).len());
        doc.insert(pr, index, element);
    }
    Ok(())
}

const FILLS: [&str; 6] = ["noFill", "solidFill", "gradFill", "blipFill", "pattFill", "grpFill"];

/// Whether `a` and `b` differ only in their cells' text.
pub(crate) fn only_text_differs(a: &Table, b: &Table) -> bool {
    let shape = |t: &Table| Table {
        rows: t
            .rows
            .iter()
            .map(|r| ibis_doc_model::TableRow { cells: r.cells.iter().map(|c| TableCell { text: TextBody::default(), ..c.clone() }).collect(), ..r.clone() })
            .collect(),
        style_options: Default::default(),
        style_id: None,
        ..t.clone()
    };
    shape(a) == shape(b)
}

/// Whether the change from `a` to `b` can be written: cells' text, rows
/// and columns inserted, removed, moved and resized (in tables without
/// merged cells; new cells as made by `Table::with_row_inserted` and the like).
pub(crate) fn writable(a: &Table, b: &Table) -> bool {
    if only_text_differs(a, b) || only_text_and_fill_differ(a, b) {
        return true;
    }
    if a.has_merges() || b.has_merges() {
        return false;
    }
    // Each kept cell as it was but for its text and fill.
    let column_sources: Vec<Option<usize>> = if b.column_sources.len() == b.columns.len() { b.column_sources.clone() } else { (0..b.columns.len()).map(Some).collect() };
    b.rows.iter().all(|row| {
        let Some(old_row) = row.source.and_then(|r| a.rows.get(r)) else { return true };
        row.cells.iter().zip(&column_sources).all(|(cell, source)| match source.and_then(|c| old_row.cells.get(c)) {
            Some(old) => TableCell { text: TextBody::default(), fill: Fill::None, ..cell.clone() } == TableCell { text: TextBody::default(), fill: Fill::None, ..old.clone() },
            None => true,
        })
    })
}

/// The paragraphs of cell `tc` as the reader made them (one a paragraph,
/// and one more after each line break), with where they and their runs are.
fn cell_source(doc: &XmlDocument, tc: NodeId) -> ShapeSource {
    let mut paragraphs = Vec::new();
    if let Some(body) = doc.child(tc, "txBody") {
        for p in doc.children_named(body, "p") {
            let mut line = 0;
            let mut runs = Vec::new();
            for &n in doc.children(p) {
                if !doc.is_element(n) {
                    continue;
                }
                match doc.local_name(n) {
                    "r" | "fld" => runs.push(doc.element_path(n)),
                    "br" => {
                        paragraphs.push(ParagraphSource { p: doc.element_path(p), line, runs: std::mem::take(&mut runs) });
                        line += 1;
                    }
                    _ => {}
                }
            }
            paragraphs.push(ParagraphSource { p: doc.element_path(p), line, runs });
        }
    }
    ShapeSource { part: String::new(), path: Vec::new(), runs: None, paragraphs, turns: Vec::new(), group: None }
}

/// Cell frame properties live in tcPr and override txBody's defaults.
fn write_cell_frame(doc: &mut XmlDocument, tc: NodeId, old: &TextBody, new: &TextBody) {
    if old.insets == new.insets && old.anchor == new.anchor && (old.vertical, old.vertical_kind, old.sideways) == (new.vertical, new.vertical_kind, new.sideways) { return; }
    let pr = match doc.child(tc, "tcPr") {
        Some(pr) => pr,
        None => {
            let a = super::pptx_new::Prefixes::at(doc, tc).a;
            let at = doc.children(tc).iter().position(|&n| doc.is_element(n) && doc.local_name(n) == "extLst").unwrap_or_else(|| doc.children(tc).len());
            doc.insert_element(tc, at, &format!("{a}tcPr"))
        }
    };
    for (name, a, b) in [("marL", old.insets.left, new.insets.left), ("marT", old.insets.top, new.insets.top), ("marR", old.insets.right, new.insets.right), ("marB", old.insets.bottom, new.insets.bottom)] {
        if a != b { doc.set_attr(pr, name, &b.to_string()); }
    }
    if old.anchor != new.anchor {
        let value = match new.anchor { ibis_doc_model::VerticalAnchor::Top => "t", ibis_doc_model::VerticalAnchor::Middle => "ctr", ibis_doc_model::VerticalAnchor::Bottom => "b" };
        doc.set_attr(pr, "anchor", value);
    }
    if (old.vertical, old.vertical_kind, old.sideways) != (new.vertical, new.vertical_kind, new.sideways) {
        let value = if new.vertical { new.vertical_kind.ooxml() } else { match new.sideways { 5_400_000 => "vert", 16_200_000 => "vert270", _ => "horz" } };
        doc.set_attr(pr, "vert", value);
    }
}

/// Write the cells of `new` whose text differs from `old` into table
/// frame `node`.
pub(crate) fn write_cell_text(doc: &mut XmlDocument, node: NodeId, old: &Table, new: &Table) -> Result<(), Error> {
    let tbl = doc.path(node, &["graphic", "graphicData", "tbl"]).ok_or_else(|| unsupported("この表の変更"))?;
    // The cells, found before any is rewritten (paths shift).
    let rows: Vec<Vec<NodeId>> = doc.children_named(tbl, "tr").map(|tr| doc.children_named(tr, "tc").collect()).collect();
    let mut changed = Vec::new();
    for (r, (a, b)) in old.rows.iter().zip(&new.rows).enumerate() {
        for (c, (x, y)) in a.cells.iter().zip(&b.cells).enumerate() {
            if x.text != y.text {
                let tc = *rows.get(r).and_then(|row| row.get(c)).ok_or_else(|| unsupported("この表の変更"))?;
                changed.push((tc, &x.text, &y.text));
            }
        }
    }
    // Each cell's paths are taken just before it is written: writing a
    // cell changes nothing outside it.
    for (tc, old_body, new_body) in changed {
        let source = cell_source(doc, tc);
        super::pptx_text::rewrite_text(doc, tc, &source, old_body, new_body)?;
        write_cell_frame(doc, tc, old_body, new_body);
    }
    Ok(())
}

/// Write table style options (`firstRow`, `bandRow`, etc.) into `a:tblPr`.
fn write_table_style_options(doc: &mut XmlDocument, tbl: NodeId, options: &ibis_doc_model::TableStyleOptions) {
    let pr = match doc.child(tbl, "tblPr") {
        Some(pr) => pr,
        None => {
            let a = super::pptx_new::Prefixes::at(doc, tbl).a;
            doc.insert_element(tbl, 0, &format!("{a}tblPr"))
        }
    };
    let set = |doc: &mut XmlDocument, name: &str, on: bool| {
        doc.set_attr(pr, name, if on { "1" } else { "0" });
    };
    set(doc, "firstRow", options.first_row);
    set(doc, "lastRow", options.last_row);
    set(doc, "firstCol", options.first_col);
    set(doc, "lastCol", options.last_col);
    set(doc, "bandRow", options.band_row);
    set(doc, "bandCol", options.band_col);
}

/// Write table style ID (`tableStyleId`) into `a:tblPr`.
fn write_table_style_id(doc: &mut XmlDocument, tbl: NodeId, style_id: Option<&str>) {
    let pr = match doc.child(tbl, "tblPr") {
        Some(pr) => pr,
        None => {
            let a = super::pptx_new::Prefixes::at(doc, tbl).a;
            doc.insert_element(tbl, 0, &format!("{a}tblPr"))
        }
    };
    if let Some(existing) = doc.child(pr, "tableStyleId") {
        doc.detach(existing);
    }
    if let Some(id) = style_id {
        let a = super::pptx_new::Prefixes::at(doc, pr).a;
        let index = doc.children(pr).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "extLst").unwrap_or(doc.children(pr).len());
        let elem = doc.insert_element(pr, index, &format!("{a}tableStyleId"));
        doc.set_text(elem, id);
    }
}

/// Write table `new` into table frame `node`, which holds `old`.
pub(crate) fn write_table(doc: &mut XmlDocument, node: NodeId, old: &Table, new: &Table) -> Result<(), Error> {
    let tbl = doc.path(node, &["graphic", "graphicData", "tbl"]).ok_or_else(|| unsupported("この表の変更"))?;
    if old.style_options != new.style_options {
        write_table_style_options(doc, tbl, &new.style_options);
    }
    if old.style_id != new.style_id {
        write_table_style_id(doc, tbl, new.style_id.as_deref());
    }
    if only_text_differs(old, new) {
        return write_cell_text(doc, node, old, new);
    }
    if only_text_and_fill_differ(old, new) {
        let rows: Vec<Vec<NodeId>> = doc.children_named(tbl, "tr").map(|tr| doc.children_named(tr, "tc").collect()).collect();
        // Which cell covers each covered cell: one to its left in its row
        // (hMerge) and/or above in its column (vMerge).
        let mut covered: std::collections::HashMap<(usize, usize), (bool, bool)> = Default::default();
        for (r, row) in new.rows.iter().enumerate() {
            for (c, cell) in row.cells.iter().enumerate() {
                if cell.merged {
                    continue;
                }
                for rr in r..r + cell.row_span.max(1) as usize {
                    for cc in c..c + cell.column_span.max(1) as usize {
                        if (rr, cc) != (r, c) {
                            covered.insert((rr, cc), (cc > c, rr > r));
                        }
                    }
                }
            }
        }
        for (r, (a, b)) in old.rows.iter().zip(&new.rows).enumerate() {
            for (c, (x, y)) in a.cells.iter().zip(&b.cells).enumerate() {
                let tc = *rows.get(r).and_then(|row| row.get(c)).ok_or_else(|| unsupported("この表の変更"))?;
                if x.fill != y.fill {
                    write_cell_fill(doc, tc, &x.fill, &y.fill)?;
                }
                if x.borders != y.borders {
                    write_cell_borders(doc, tc, &x.borders, &y.borders)?;
                }
                if (x.column_span, x.row_span, x.merged) != (y.column_span, y.row_span, y.merged) {
                    write_cell_merge(doc, tc, y, if y.merged { covered.get(&(r, c)).copied() } else { None });
                }
            }
        }
        return write_cell_text(doc, node, old, new);
    }
    if old.has_merges() || new.has_merges() {
        return Err(unsupported("結合したセルのある表の行・列の変更"));
    }
    let tbl = doc.path(node, &["graphic", "graphicData", "tbl"]).ok_or_else(|| unsupported("この表の変更"))?;
    let grid = doc.child(tbl, "tblGrid").ok_or_else(|| unsupported("この表の変更"))?;
    let old_columns: Vec<NodeId> = doc.children_named(grid, "gridCol").collect();
    let old_rows: Vec<NodeId> = doc.children_named(tbl, "tr").collect();
    if old_columns.len() != old.columns.len() || old_rows.len() != old.rows.len() || old_columns.is_empty() || old_rows.is_empty() {
        return Err(unsupported("この表の変更"));
    }
    // Each new column's and row's original: its source, else the one
    // before it (as `Table::with_row_inserted` makes it), else the first.
    let origins = |sources: Vec<Option<usize>>, count: usize| -> Vec<(usize, bool)> {
        let mut last = 0;
        sources
            .into_iter()
            .map(|s| match s.filter(|&s| s < count) {
                Some(s) => {
                    last = s;
                    (s, true)
                }
                None => (last, false),
            })
            .collect()
    };
    let column_sources = if new.column_sources.len() == new.columns.len() { new.column_sources.clone() } else { (0..new.columns.len()).map(Some).collect() };
    let columns = origins(column_sources, old_columns.len());
    let rows = origins(new.rows.iter().map(|r| r.source).collect(), old_rows.len());

    // The grid.
    for &c in &old_columns {
        doc.detach(c);
    }
    for (j, &(from, _)) in columns.iter().enumerate() {
        let col = doc.deep_clone(old_columns[from]);
        doc.set_attr(col, "w", &new.columns[j].to_string());
        let end = doc.children(grid).len();
        doc.insert(grid, end, col);
    }
    // The rows, each with its cells in the new columns' order.
    let mut written: Vec<(NodeId, &TextBody, &TextBody)> = Vec::new();
    let first_row = old_rows[0];
    let at = doc.children(tbl).iter().position(|&c| c == first_row).unwrap_or(doc.children(tbl).len());
    for &tr in &old_rows {
        doc.detach(tr);
    }
    for (i, &(from_row, _)) in rows.iter().enumerate() {
        let tr = doc.deep_clone(old_rows[from_row]);
        doc.set_attr(tr, "h", &new.rows[i].height.to_string());
        let cloned: Vec<NodeId> = doc.children_named(tr, "tc").collect();
        for &tc in &cloned {
            doc.detach(tc);
        }
        for (j, &(from_column, column_kept)) in columns.iter().enumerate() {
            let source_tc = *cloned.get(from_column).or(cloned.last()).ok_or_else(|| unsupported("この表の変更"))?;
            let tc = if column_kept && !cloned.is_empty() { source_tc } else { doc.deep_clone(source_tc) };
            let end = doc.children(tr).len();
            // `a:tc` come before the row's `a:extLst`.
            let index = doc.children(tr).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "extLst").unwrap_or(end);
            doc.insert(tr, index, tc);
            let template = &old.rows[from_row].cells[from_column.min(old.rows[from_row].cells.len() - 1)].text;
            let target = &new.rows[i].cells[j].text;
            if template != target {
                written.push((tc, template, target));
            }
        }
        doc.insert(tbl, at + i, tr);
    }
    // The cells whose text is not their original's.
    for (tc, old_body, new_body) in written {
        let source = cell_source(doc, tc);
        super::pptx_text::rewrite_text(doc, tc, &source, old_body, new_body)?;
        write_cell_frame(doc, tc, old_body, new_body);
    }
    Ok(())
}
