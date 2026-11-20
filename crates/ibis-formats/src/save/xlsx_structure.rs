// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Structural changes (rows and columns inserted and deleted) made in a
//! workbook's XML, as Excel makes them: cells, rows and column ranges
//! move; merged ranges, conditional formats, validations, links, the
//! filter, the view, page breaks and drawings' anchors move, grow or
//! shrink; every formula of every sheet, the defined names, the
//! calculation chain and the charts' ranges follow. A shared formula of a
//! changed workbook is written out cell by cell first (its cells would not
//! all move alike).
//!
//! What cannot follow yet (tables, pivot tables, an array formula cut by
//! the change) is an error, and nothing is written.

use ibis_calc::formula::{Axis, adjust_references};
use ibis_doc_model::sheet_ops::{StructureEdit, move_index, move_range};
use ibis_doc_model::{CellRange, Presentation, Sheet, cell_name};
use ibis_ooxml::{NodeId, Package, XmlDocument};

use super::SourceMap;
use crate::save::unsupported;
use crate::Error;

const REL_DRAWING: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const REL_TABLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table";
const REL_WORKSHEET: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const CT_WORKSHEET: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml";

/// The worksheet at `index` of the model (a slide's, or a hidden one).
fn model_sheet(p: &Presentation, index: usize) -> Option<&Sheet> {
    let ws = p.workbook.as_ref()?.sheets.get(index)?;
    match ws.slide {
        Some(id) => p.slide(id).and_then(ibis_doc_model::sheet_calc::sheet_of_slide).map(|a| &**a),
        None => ws.hidden.as_deref(),
    }
}

/// A range written as Excel writes it (`A1`, `A1:B2`).
/// Where the cell at `row`, `column` is after rows or columns are inserted or deleted (`None`: it was deleted).
fn move_cell(row: u32, column: u32, axis: Axis, at: u32, count: i64) -> Option<(u32, u32)> {
    match axis {
        Axis::Rows => Some((move_index(row, at, count)?, column)),
        Axis::Columns => Some((row, move_index(column, at, count)?)),
    }
}

fn range_text(r: CellRange) -> String {
    if r.first_row == r.last_row && r.first_column == r.last_column {
        cell_name(r.first_column, r.first_row)
    } else {
        format!("{}:{}", cell_name(r.first_column, r.first_row), cell_name(r.last_column, r.last_row))
    }
}

/// A space-separated list of ranges (`sqref`) moved; `None` if all go.
fn move_sqref(sqref: &str, axis: Axis, at: u32, count: i64) -> Option<String> {
    let parts: Vec<String> = sqref.split_whitespace().filter_map(|p| crate::xlsx::parse_range(p)).filter_map(|r| move_range(r, axis, at, count)).map(range_text).collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

fn move_ref_attr(doc: &mut XmlDocument, node: NodeId, attr: &str, axis: Axis, at: u32, count: i64) -> bool {
    let Some(v) = doc.attr(node, attr).map(str::to_string) else { return true };
    match move_sqref(&v, axis, at, count) {
        Some(n) => {
            if n != v {
                doc.set_attr(node, attr, &n);
            }
            true
        }
        None => false,
    }
}

/// Whether the XML `bytes` may name the sheet `name` (in a formula): `false`
/// only when it certainly does not. A name that is not plain ASCII or has a
/// character XML writes as an entity is taken as possibly there.
fn mentions_sheet(bytes: &[u8], name: &str) -> bool {
    if name.is_empty() || !name.is_ascii() || name.contains(['&', '<', '>', '\'', '"']) {
        return true;
    }
    let t = name.as_bytes();
    let first = t[0].to_ascii_lowercase();
    bytes.windows(t.len()).enumerate().any(|(i, w)| w[0].to_ascii_lowercase() == first && w.eq_ignore_ascii_case(t) && i < bytes.len())
}

fn detached(doc: &mut XmlDocument, name: &str) -> NodeId {
    let e = doc.insert_element(XmlDocument::ROOT, 0, name);
    doc.detach(e);
    e
}

fn unused_table_col_name(existing: &[String], start: usize) -> String {
    let mut k = start;
    loop {
        let name = format!("列{k}");
        if !existing.iter().any(|c| c.eq_ignore_ascii_case(&name)) {
            return name;
        }
        k += 1;
    }
}

/// Make one structural change in the package (and in `map`, which follows
/// the sheets). `before` is the model as it stands before the change (for
/// writing out shared formulas).
pub(crate) fn replay(package: &mut Package, map: &mut SourceMap, before: &Presentation, edit: &StructureEdit) -> Result<(), Error> {
    match edit {
        StructureEdit::AddSheet { sheet, name, content, .. } => return add_sheet(package, map, *sheet, name, content),
        StructureEdit::DeleteSheet { sheet } => return delete_sheet(package, map, before, *sheet),
        StructureEdit::RenameSheet { sheet, name } => return rename_sheet(package, map, before, *sheet, name),
        StructureEdit::HideSheet { sheet, hidden, .. } => return hide_sheet(package, map, *sheet, *hidden),
        StructureEdit::MoveSheet { sheet, to } => return move_sheet(package, map, *sheet, *to),
        _ => {}
    }
    let workbook = before.workbook.as_deref().ok_or_else(|| unsupported("このブックの行・列の挿入と削除"))?;
    let target = edit.sheet();
    let target_name = workbook.sheets.get(target).map(|s| s.name.clone()).ok_or_else(|| unsupported("このシートの行・列の挿入と削除"))?;
    let (axis, at, count) = edit.shift();
    for (index, part) in map.sheet_parts.iter().enumerate() {
        let Some(part) = part else { continue };
        let own = workbook.sheets[index].name.clone();
        // A sheet that never names the changed sheet has no reference to move:
        // left unread (parsing every sheet of a big workbook took seconds).
        if index != target && !package.is_changed(part) && !mentions_sheet(&package.read(part)?, &target_name) {
            continue;
        }
        let model = model_sheet(before, index);
        let doc = package.xml_mut(part)?;
        let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
        // Formulas: shared ones written out, then every one adjusted.
        let formulas: Vec<NodeId> = doc.descendants(root).into_iter().filter(|&n| doc.local_name(n) == "f").collect();
        for f in formulas {
            let cell = doc.parent(f).filter(|&c| doc.local_name(c) == "c");
            if doc.attr(f, "t") == Some("shared") {
                let pos = cell.and_then(|c| doc.attr(c, "r")).and_then(crate::xlsx::cell_ref);
                let text = pos.and_then(|(col, row)| model.and_then(|m| m.cell(row, col)).and_then(|c| c.formula.clone())).ok_or_else(|| unsupported("この共有数式のあるシートの行・列の挿入と削除"))?;
                for a in ["t", "si", "ref"] {
                    doc.remove_attr(f, a);
                }
                doc.set_text(f, &super::file_formula(&super::external_book_names(before), &text)?);
            }
            let text = doc.text(f);
            if !text.is_empty() {
                let new = adjust_references(&text, &own, &target_name, axis, at, count);
                if new != text {
                    doc.set_text(f, &new);
                }
            }
            if index == target && doc.attr(f, "t") == Some("array") && !move_ref_attr(doc, f, "ref", axis, at, count) {
                return Err(unsupported("配列数式を消す行・列の削除"));
            }
            if index == target && doc.attr(f, "t") == Some("dataTable") {
                if !move_ref_attr(doc, f, "ref", axis, at, count)
                    || !move_ref_attr(doc, f, "r1", axis, at, count)
                    || !move_ref_attr(doc, f, "r2", axis, at, count)
                {
                    doc.detach(f);
                }
            }
        }
        // Formulas of conditional formats and validations.
        let texts: Vec<NodeId> = doc.descendants(root).into_iter().filter(|&n| matches!(doc.local_name(n), "formula" | "formula1" | "formula2")).collect();
        for n in texts {
            let text = doc.text(n);
            let new = adjust_references(&text, &own, &target_name, axis, at, count);
            if new != text {
                doc.set_text(n, &new);
            }
        }
        let points: Vec<_> = doc.descendants(root).into_iter().filter(|&n| doc.local_name(n) == "cfvo" && doc.attr(n, "type") == Some("formula")).collect();
        for n in points {
            if let Some(text) = doc.attr(n, "val").map(str::to_string) {
                let new = adjust_references(&text, &own, &target_name, axis, at, count);
                if new != text { doc.set_attr(n, "val", &new); }
            }
        }
        if index == target {
            change_sheet_xml(doc, root, axis, at, count)?;
        }
    }
    // The sheet's drawings: anchors move.
    if let Some(Some(part)) = map.sheet_parts.get(target) {
        let drawings: Vec<String> = package.relationships(part)?.into_iter().filter(|r| r.kind == REL_DRAWING).filter_map(|r| r.target_part(part)).collect();
        for d in drawings {
            let doc = package.xml_mut(&d)?;
            let Some(root) = doc.root_element() else { continue };
            let markers: Vec<NodeId> = doc.descendants(root).into_iter().filter(|&n| matches!(doc.local_name(n), "from" | "to")).collect();
            for m in markers {
                let local = if axis == Axis::Rows { "row" } else { "col" };
                let Some(e) = doc.child(m, local) else { continue };
                let Ok(v) = doc.text(e).trim().parse::<u32>() else { continue };
                // A marker in deleted rows goes to where they were.
                let n = move_index(v, at, count).unwrap_or(at);
                if n != v {
                    doc.set_text(e, &n.to_string());
                }
            }
        }
    }
    // The sheet's threaded comments: each thread's cell moves; one in the
    // deleted rows or columns goes with its replies.
    if let Some(Some(part)) = map.sheet_parts.get(target) {
        const REL_THREADED: &str = "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment";
        let parts: Vec<String> = package.relationships(part)?.into_iter().filter(|r| r.kind == REL_THREADED && !r.external).filter_map(|r| r.target_part(part)).collect();
        for t in parts {
            let doc = package.xml_mut(&t)?;
            let Some(root) = doc.root_element() else { continue };
            let posts: Vec<NodeId> = doc.children_named(root, "threadedComment").collect();
            let mut gone: Vec<String> = Vec::new();
            for &c in &posts {
                let Some((col, row)) = doc.attr(c, "ref").and_then(crate::xlsx::cell_ref) else { continue };
                let (r, k) = if axis == Axis::Rows { (move_index(row, at, count), Some(col)) } else { (Some(row), move_index(col, at, count)) };
                match (r, k) {
                    (Some(r), Some(k)) => doc.set_attr(c, "ref", &cell_name(k, r)),
                    _ => {
                        if let Some(id) = doc.attr(c, "id") {
                            gone.push(id.to_string());
                        }
                        doc.detach(c);
                    }
                }
            }
            for &c in &posts {
                if doc.attr(c, "parentId").is_some_and(|p| gone.iter().any(|g| g == p)) {
                    doc.detach(c);
                }
            }
        }
    }
    // The sheet's notes (the old kind of comment): each `comment`'s cell in `comments*.xml` and each note's shape in the VML drawing
    // (`x:Row`, `x:Column` and the anchor) move together; one in the deleted rows or columns goes.
    if let Some(Some(part)) = map.sheet_parts.get(target) {
        const REL_COMMENTS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
        const REL_VML: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing";
        let rels = package.relationships(part)?;
        let comment_parts: Vec<String> = rels.iter().filter(|r| r.kind == REL_COMMENTS && !r.external).filter_map(|r| r.target_part(part)).collect();
        let vml_parts: Vec<String> = rels.iter().filter(|r| r.kind == REL_VML && !r.external).filter_map(|r| r.target_part(part)).collect();
        for c in comment_parts {
            let doc = package.xml_mut(&c)?;
            let Some(root) = doc.root_element() else { continue };
            let notes: Vec<NodeId> = doc.descendants(root).into_iter().filter(|&n| doc.local_name(n) == "comment").collect();
            for note in notes {
                let Some((col, row)) = doc.attr(note, "ref").and_then(crate::xlsx::cell_ref) else { continue };
                match move_cell(row, col, axis, at, count) {
                    Some((row, col)) => doc.set_attr(note, "ref", &cell_name(col, row)),
                    None => doc.detach(note),
                }
            }
        }
        for v in vml_parts {
            // The drawing is read as XML; one that is not is left as it is.
            let Ok(doc) = package.xml_mut(&v) else { continue };
            let Some(root) = doc.root_element() else { continue };
            let shapes: Vec<NodeId> = doc.descendants(root).into_iter().filter(|&n| doc.local_name(n) == "shape").collect();
            for shape in shapes {
                let Some(data) = doc.descendants(shape).into_iter().find(|&n| doc.local_name(n) == "ClientData" && doc.attr(n, "ObjectType") == Some("Note")) else { continue };
                let number = |doc: &XmlDocument, name: &str| doc.child(data, name).and_then(|e| doc.text(e).trim().parse::<u32>().ok());
                let (Some(row), Some(col)) = (number(doc, "Row"), number(doc, "Column")) else { continue };
                let Some((new_row, new_col)) = move_cell(row, col, axis, at, count) else {
                    doc.detach(shape);
                    continue;
                };
                if let Some(e) = doc.child(data, "Row") {
                    doc.set_text(e, &new_row.to_string());
                }
                if let Some(e) = doc.child(data, "Column") {
                    doc.set_text(e, &new_col.to_string());
                }
                // The box is drawn at `x:Anchor`: left column, offset, top row, offset, right column, offset, bottom row, offset.
                if let Some(anchor) = doc.child(data, "Anchor") {
                    let mut parts: Vec<String> = doc.text(anchor).split(',').map(|p| p.trim().to_string()).collect();
                    if parts.len() == 8 {
                        let indexes = if axis == Axis::Rows { [2, 6] } else { [0, 4] };
                        for i in indexes {
                            if let Ok(v) = parts[i].parse::<u32>() {
                                parts[i] = move_index(v, at, count).unwrap_or(at).to_string();
                            }
                        }
                        doc.set_text(anchor, &parts.join(", "));
                    }
                }
            }
        }
    }
    // The sheet's tables: ref and formulas move, columns follow.
    if let Some(Some(part)) = map.sheet_parts.get(target) {
        let table_rels: Vec<(String, String)> = package.relationships(part)?
            .into_iter()
            .filter(|r| r.kind == REL_TABLE && !r.external)
            .filter_map(|r| Some((r.id.clone(), r.target_part(part)?)))
            .collect();
        let mut gone_tables: Vec<String> = Vec::new();
        let mut new_table_headers: Vec<(u32, u32, String)> = Vec::new();
        for (rel_id, table_part) in table_rels {
            let doc = package.xml_mut(&table_part)?;
            let Some(root) = doc.root_element() else { continue };
            let Some(r) = doc.attr(root, "ref").and_then(crate::xlsx::parse_range) else { continue };
            let has_header = doc.attr(root, "headerRowCount")
                .map(|v| v != "0")
                .unwrap_or_else(|| doc.attr(root, "headerRowShown") != Some("0"));
            match move_range(r, axis, at, count) {
                Some(new_r) => {
                    doc.set_attr(root, "ref", &range_text(new_r));
                    if let Some(af) = doc.child(root, "autoFilter") {
                        if !move_ref_attr(doc, af, "ref", axis, at, count) {
                            doc.detach(af);
                        }
                    }
                    if axis == Axis::Columns {
                        if let Some(cols_node) = doc.child(root, "tableColumns") {
                            let prefix = doc.prefix(root).map(|p| format!("{p}:")).unwrap_or_default();
                            let col_nodes: Vec<NodeId> = doc.children_named(cols_node, "tableColumn").collect();
                            if count > 0 {
                                let n = count as u32;
                                if at > r.first_column && at <= r.last_column {
                                    let rel_at = (at - r.first_column) as usize;
                                    let mut existing_names: Vec<String> = col_nodes.iter().filter_map(|&c| doc.attr(c, "name").map(str::to_string)).collect();
                                    let mut max_id = col_nodes.iter().filter_map(|&c| doc.attr(c, "id")?.parse::<u32>().ok()).max().unwrap_or(0);
                                    let insert_before_node = col_nodes.get(rel_at).copied();
                                    for i in 0..n {
                                        max_id += 1;
                                        let name = unused_table_col_name(&existing_names, (i + 1) as usize);
                                        existing_names.push(name.clone());
                                        let new_col = detached(doc, &format!("{prefix}tableColumn"));
                                        doc.set_attr(new_col, "id", &max_id.to_string());
                                        doc.set_attr(new_col, "name", &name);
                                        match insert_before_node {
                                            Some(before) => doc.insert_before(before, new_col),
                                            None => {
                                                let count = doc.children(cols_node).len();
                                                doc.insert(cols_node, count, new_col);
                                            }
                                        }
                                        if has_header {
                                            new_table_headers.push((at + i, r.first_row, name));
                                        }
                                    }
                                }
                            } else {
                                let n = (-count) as u32;
                                let end = at + n - 1;
                                if at <= r.last_column && end >= r.first_column {
                                    let del_start = at.max(r.first_column);
                                    let del_end = end.min(r.last_column);
                                    let rel_start = (del_start - r.first_column) as usize;
                                    let del_len = (del_end - del_start + 1) as usize;
                                    for i in (rel_start..(rel_start + del_len).min(col_nodes.len())).rev() {
                                        doc.detach(col_nodes[i]);
                                    }
                                }
                            }
                            let count = doc.children_named(cols_node, "tableColumn").count();
                            doc.set_attr(cols_node, "count", &count.to_string());
                        }
                    }
                    // Formulas inside table (calculatedColumnFormula, totalsRowFormula)
                    let formulas: Vec<NodeId> = doc.descendants(root).into_iter()
                        .filter(|&n| matches!(doc.local_name(n), "calculatedColumnFormula" | "totalsRowFormula"))
                        .collect();
                    for f in formulas {
                        let text = doc.text(f);
                        if !text.is_empty() {
                            let new = adjust_references(&text, &target_name, &target_name, axis, at, count);
                            if new != text {
                                doc.set_text(f, &new);
                            }
                        }
                    }
                }
                None => {
                    gone_tables.push(rel_id);
                    package.remove(&table_part);
                    let _ = package.remove_override(&table_part);
                }
            }
        }
        if !new_table_headers.is_empty() {
            let sheet_doc = package.xml_mut(part)?;
            insert_header_cells(sheet_doc, &new_table_headers)?;
        }
        if !gone_tables.is_empty() {
            for gone_id in &gone_tables {
                package.remove_relationship(part, gone_id)?;
            }
            let sheet_doc = package.xml_mut(part)?;
            let sheet_root = sheet_doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
            let r_prefix = sheet_doc.lookup_prefix(sheet_root, REL_NS).unwrap_or_else(|| "r".into());
            for gone_id in &gone_tables {
                if let Some(list) = sheet_doc.child(sheet_root, "tableParts") {
                    for p in sheet_doc.children_named(list, "tablePart").collect::<Vec<_>>() {
                        if sheet_doc.attr(p, &format!("{r_prefix}:id")) == Some(gone_id.as_str()) {
                            sheet_doc.detach(p);
                        }
                    }
                    let count = sheet_doc.children_named(list, "tablePart").count();
                    if count == 0 {
                        sheet_doc.detach(list);
                    } else {
                        sheet_doc.set_attr(list, "count", &count.to_string());
                    }
                }
            }
        }
    }
    // Defined names.
    let doc = package.xml_mut(&map.workbook)?;
    if let Some(root) = doc.root_element() {
        let names: Vec<NodeId> = doc.descendants(root).into_iter().filter(|&n| doc.local_name(n) == "definedName").collect();
        for n in names {
            let scope = doc.attr(n, "localSheetId").and_then(|v| v.parse::<usize>().ok()).and_then(|i| workbook.sheets.get(i)).map_or(target_name.clone(), |s| s.name.clone());
            let text = doc.text(n);
            let new = adjust_references(&text, &scope, &target_name, axis, at, count);
            if new != text {
                doc.set_text(n, &new);
            }
        }
    }
    // The calculation chain: the sheet's entries move or go.
    if let Some(chain) = &map.calc_chain
        && package.contains(chain)
    {
        let sheet_id = map.sheet_ids.get(target).copied().unwrap_or(target as u32 + 1);
        let doc = package.xml_mut(chain)?;
        if let Some(root) = doc.root_element() {
            let mut sheet = 0u32;
            let entries: Vec<NodeId> = doc.children_named(root, "c").collect();
            let mut pending_i: Option<u32> = None;
            for e in entries {
                if let Some(i) = doc.attr(e, "i").and_then(|v| v.parse().ok()) {
                    sheet = i;
                }
                if let Some(i) = pending_i.take()
                    && doc.attr(e, "i").is_none()
                {
                    doc.set_attr(e, "i", &i.to_string());
                }
                if sheet != sheet_id {
                    continue;
                }
                let Some((col, row)) = doc.attr(e, "r").and_then(crate::xlsx::cell_ref) else { continue };
                let (r, c) = if axis == Axis::Rows { (move_index(row, at, count), Some(col)) } else { (Some(row), move_index(col, at, count)) };
                match (r, c) {
                    (Some(r), Some(c)) => doc.set_attr(e, "r", &cell_name(c, r)),
                    _ => {
                        if doc.attr(e, "i").is_some() {
                            pending_i = Some(sheet);
                        }
                        doc.detach(e);
                    }
                }
            }
        }
    }
    // Charts' ranges.
    let charts: Vec<String> = package.names().filter(|n| n.starts_with("xl/charts/chart") && n.ends_with(".xml")).map(str::to_string).collect();
    for c in charts {
        let original = package.xml(&c)?;
        let Some(root) = original.root_element() else { continue };
        let refs: Vec<(NodeId, String)> = original.descendants(root).into_iter().filter(|&n| original.local_name(n) == "f").map(|n| (n, original.text(n))).collect();
        let changes: Vec<(NodeId, String)> = refs.into_iter().filter_map(|(n, t)| {
            let new = adjust_references(&t, &target_name, &target_name, axis, at, count);
            (new != t).then_some((n, new))
        }).collect();
        if changes.is_empty() {
            continue;
        }
        let doc = package.xml_mut(&c)?;
        for (n, t) in changes {
            doc.set_text(n, &t);
        }
    }
    Ok(())
}

/// The changed sheet's own XML: rows and cells, column ranges, merged
/// ranges, dimension, links, filter, formats' ranges, view and breaks.
fn change_sheet_xml(doc: &mut XmlDocument, root: NodeId, axis: Axis, at: u32, count: i64) -> Result<(), Error> {
    let data = doc.child(root, "sheetData").ok_or_else(|| Error::new("シートに sheetData がありません"))?;
    let rows: Vec<NodeId> = doc.children_named(data, "row").collect();
    let mut next_row = 0u32;
    for row in rows {
        let r = doc.attr(row, "r").and_then(|v| v.parse::<u32>().ok()).map_or(next_row, |v| v.saturating_sub(1));
        next_row = r + 1;
        if axis == Axis::Rows {
            match move_index(r, at, count) {
                None => {
                    doc.detach(row);
                    continue;
                }
                Some(n) => {
                    if n != r {
                        doc.set_attr(row, "r", &(n + 1).to_string());
                    }
                }
            }
        } else {
            // Column spans are only a hint: dropped.
            doc.remove_attr(row, "spans");
        }
        let new_row = if axis == Axis::Rows { move_index(r, at, count).unwrap_or(r) } else { r };
        let cells: Vec<NodeId> = doc.children_named(row, "c").collect();
        let mut next_col = 0u32;
        for c in cells {
            let col = doc.attr(c, "r").and_then(crate::xlsx::cell_ref).map_or(next_col, |(col, _)| col);
            next_col = col + 1;
            let new_col = if axis == Axis::Columns {
                match move_index(col, at, count) {
                    Some(n) => n,
                    None => {
                        doc.detach(c);
                        continue;
                    }
                }
            } else {
                col
            };
            let name = cell_name(new_col, new_row);
            if doc.attr(c, "r") != Some(name.as_str()) {
                doc.set_attr(c, "r", &name);
            }
        }
    }
    // Column ranges (1-based min–max).
    if axis == Axis::Columns
        && let Some(cols) = doc.child(root, "cols")
    {
        for col in doc.children_named(cols, "col").collect::<Vec<_>>() {
            let (Some(min), Some(max)) = (doc.attr(col, "min").and_then(|v| v.parse::<u32>().ok()), doc.attr(col, "max").and_then(|v| v.parse::<u32>().ok())) else { continue };
            let r = CellRange { first_row: 0, last_row: 0, first_column: min.saturating_sub(1), last_column: max.saturating_sub(1) };
            // A range reaching the last column stays there.
            match move_range(r, axis, at, count) {
                Some(n) => {
                    let (nmin, nmax) = (n.first_column + 1, (n.last_column + 1).min(16_384));
                    if nmin > 16_384 {
                        doc.detach(col);
                        continue;
                    }
                    doc.set_attr(col, "min", &nmin.to_string());
                    doc.set_attr(col, "max", &nmax.to_string());
                }
                None => doc.detach(col),
            }
        }
        if doc.children_named(cols, "col").next().is_none() {
            doc.detach(cols);
        }
    }
    if let Some(dim) = doc.child(root, "dimension") {
        move_ref_attr(doc, dim, "ref", axis, at, count);
    }
    if let Some(merges) = doc.child(root, "mergeCells") {
        for m in doc.children_named(merges, "mergeCell").collect::<Vec<_>>() {
            let v = doc.attr(m, "ref").unwrap_or("").to_string();
            match crate::xlsx::parse_range(&v).and_then(|r| move_range(r, axis, at, count)).filter(|r| r.first_row != r.last_row || r.first_column != r.last_column) {
                Some(r) => doc.set_attr(m, "ref", &range_text(r)),
                None => doc.detach(m),
            }
        }
        let n = doc.children_named(merges, "mergeCell").count();
        if n == 0 {
            doc.detach(merges);
        } else {
            doc.set_attr(merges, "count", &n.to_string());
        }
    }
    for (list, item, attr) in [("conditionalFormatting", "", "sqref"), ("dataValidations", "dataValidation", "sqref"), ("hyperlinks", "hyperlink", "ref")] {
        let containers: Vec<NodeId> = doc.children_named(root, list).collect();
        for container in containers {
            let targets: Vec<NodeId> = if item.is_empty() { vec![container] } else { doc.children_named(container, item).collect() };
            for t in targets {
                if !move_ref_attr(doc, t, attr, axis, at, count) {
                    doc.detach(t);
                }
            }
            if !item.is_empty() {
                let n = doc.children_named(container, item).count();
                if n == 0 {
                    doc.detach(container);
                } else if doc.attr(container, "count").is_some() {
                    doc.set_attr(container, "count", &n.to_string());
                }
            }
        }
    }
    if let Some(filter) = doc.child(root, "autoFilter")
        && !move_ref_attr(doc, filter, "ref", axis, at, count)
    {
        doc.detach(filter);
    }
    // The view: the selection and the pane's top-left cell.
    if let Some(views) = doc.child(root, "sheetViews") {
        for n in doc.descendants(views) {
            match doc.local_name(n) {
                "selection" => {
                    if !move_ref_attr(doc, n, "sqref", axis, at, count) || !move_ref_attr(doc, n, "activeCell", axis, at, count) {
                        doc.set_attr(n, "activeCell", "A1");
                        doc.set_attr(n, "sqref", "A1");
                    }
                }
                "pane" | "sheetView" => {
                    move_ref_attr(doc, n, "topLeftCell", axis, at, count);
                }
                _ => {}
            }
        }
    }
    let breaks = if axis == Axis::Rows { "rowBreaks" } else { "colBreaks" };
    if let Some(b) = doc.child(root, breaks) {
        for brk in doc.children_named(b, "brk").collect::<Vec<_>>() {
            let Some(id) = doc.attr(brk, "id").and_then(|v| v.parse::<u32>().ok()) else { continue };
            match move_index(id, at, count) {
                Some(n) => doc.set_attr(brk, "id", &n.to_string()),
                None => doc.detach(brk),
            }
        }
        let n = doc.children_named(b, "brk").count();
        doc.set_attr(b, "count", &n.to_string());
        if doc.attr(b, "manualBreakCount").is_some() {
            doc.set_attr(b, "manualBreakCount", &n.to_string());
        }
    }
    Ok(())
}

// ----- Sheets added, deleted, renamed --------------------------------------

/// Every formula-bearing text of the workbook passed through `f`: cells'
/// formulas, conditional formats' and validations' formulas of every
/// sheet, the defined names and the charts' ranges.
fn rewrite_all(package: &mut Package, map: &SourceMap, f: &dyn Fn(&str) -> String) -> Result<(), Error> {
    let mut parts: Vec<String> = map.sheet_parts.iter().flatten().cloned().collect();
    parts.push(map.workbook.clone());
    parts.extend(package.names().filter(|n| n.starts_with("xl/charts/chart") && n.ends_with(".xml")).map(str::to_string));
    for part in parts {
        if !package.contains(&part) {
            continue;
        }
        let original = package.xml(&part)?;
        let Some(root) = original.root_element() else { continue };
        let changes: Vec<(NodeId, String)> = original
            .descendants(root)
            .into_iter()
            .filter(|&n| matches!(original.local_name(n), "f" | "formula" | "formula1" | "formula2" | "definedName"))
            .filter_map(|n| {
                let t = original.text(n);
                let new = f(&t);
                (new != t).then_some((n, new))
            })
            .collect();
        // Conditional thresholds store formulas in attributes, not text.
        // Patch them in place so unmodelled rule attributes remain intact.
        let thresholds: Vec<(NodeId, String)> = original.descendants(root).into_iter()
            .filter(|&n| original.local_name(n) == "cfvo" && original.attr(n, "type") == Some("formula"))
            .filter_map(|n| {
                let old = original.attr(n, "val")?;
                let new = f(old);
                (new != old).then_some((n, new))
            }).collect();
        if changes.is_empty() && thresholds.is_empty() {
            continue;
        }
        let doc = package.xml_mut(&part)?;
        for (n, t) in changes {
            doc.set_text(n, &t);
        }
        for (n, value) in thresholds {
            doc.set_attr(n, "val", &value);
        }
    }
    Ok(())
}

/// The workbook's `sheets` element and its `sheet` children.
fn sheet_elements(doc: &XmlDocument) -> Option<(NodeId, Vec<NodeId>)> {
    let root = doc.root_element()?;
    let sheets = doc.child(root, "sheets")?;
    Some((sheets, doc.children_named(sheets, "sheet").collect()))
}

/// Shift `localSheetId`s and the active and first tabs for a sheet added
/// (`delta` 1) or deleted (-1) at `index`; defined names local to a
/// deleted sheet go.
fn shift_sheet_indexes(doc: &mut XmlDocument, index: usize, delta: i64) {
    let Some(root) = doc.root_element() else { return };
    if let Some(names) = doc.child(root, "definedNames") {
        for n in doc.children_named(names, "definedName").collect::<Vec<_>>() {
            let Some(i) = doc.attr(n, "localSheetId").and_then(|v| v.parse::<i64>().ok()) else { continue };
            if delta < 0 && i == index as i64 {
                doc.detach(n);
            } else if i >= index as i64 {
                doc.set_attr(n, "localSheetId", &(i + delta).max(0).to_string());
            }
        }
        if doc.elements(names).next().is_none() {
            doc.detach(names);
        }
    }
    if let Some(views) = doc.child(root, "bookViews") {
        for v in doc.children_named(views, "workbookView").collect::<Vec<_>>() {
            for attr in ["activeTab", "firstSheet"] {
                if let Some(i) = doc.attr(v, attr).and_then(|x| x.parse::<i64>().ok())
                    && (i > index as i64 || (delta > 0 && i == index as i64))
                {
                    doc.set_attr(v, attr, &(i + delta).max(0).to_string());
                }
            }
        }
    }
}

/// docProps/app.xml: the worksheet count and the sheet titles.
fn update_app(package: &mut Package, change: impl Fn(&mut Vec<String>)) -> Result<(), Error> {
    let part = "docProps/app.xml";
    if !package.contains(part) {
        return Ok(());
    }
    let doc = package.xml_mut(part)?;
    let Some(root) = doc.root_element() else { return Ok(()) };
    let (Some(pairs), Some(titles)) = (doc.child(root, "HeadingPairs").and_then(|h| doc.elements(h).next()), doc.child(root, "TitlesOfParts").and_then(|t| doc.elements(t).next())) else { return Ok(()) };
    // The first heading pair counts the worksheets, which come first in the titles.
    let variants: Vec<NodeId> = doc.elements(pairs).collect();
    let Some(count_node) = variants.get(1).and_then(|v| doc.elements(*v).next()) else { return Ok(()) };
    let Ok(count) = doc.text(count_node).trim().parse::<usize>() else { return Ok(()) };
    let items: Vec<NodeId> = doc.elements(titles).collect();
    if items.len() < count {
        return Ok(());
    }
    let mut names: Vec<String> = items[..count].iter().map(|&n| doc.text(n)).collect();
    change(&mut names);
    let prefix = doc.prefix(items.first().copied().unwrap_or(titles)).map(|p| format!("{p}:")).unwrap_or_default();
    for n in &items[..count] {
        doc.detach(*n);
    }
    for (i, name) in names.iter().enumerate() {
        let e = doc.insert_element(titles, i, &format!("{prefix}lpstr"));
        doc.set_text(e, name);
    }
    doc.set_text(count_node, &names.len().to_string());
    let total = doc.elements(titles).count();
    doc.set_attr(titles, "size", &total.to_string());
    Ok(())
}

fn add_sheet(package: &mut Package, map: &mut SourceMap, index: usize, name: &str, content: &Sheet) -> Result<(), Error> {
    let mut n = 1;
    while package.contains(&format!("xl/worksheets/sheet{n}.xml")) {
        n += 1;
    }
    let part = format!("xl/worksheets/sheet{n}.xml");
    let mut doc = XmlDocument::new_with_declaration();
    let root = doc.append_element(XmlDocument::ROOT, "worksheet");
    doc.set_attr(root, "xmlns", MAIN_NS);
    doc.set_attr(root, "xmlns:r", REL_NS);
    let dim = doc.append_element(root, "dimension");
    doc.set_attr(dim, "ref", "A1");
    let views = doc.append_element(root, "sheetViews");
    let view = doc.append_element(views, "sheetView");
    doc.set_attr(view, "workbookViewId", "0");
    let format = doc.append_element(root, "sheetFormatPr");
    let row_pt = content.default_row as f64 / ibis_doc_model::EMU_PER_POINT as f64;
    doc.set_attr(format, "defaultRowHeight", &format!("{}", (row_pt * 100.0).round() / 100.0));
    // The columns' width as the model has it (the first sheet's), which a
    // new sheet in Excel would not have otherwise.
    if content.default_column > 0 && map.digit > 0.0 {
        let chars = content.default_column as f64 / ibis_doc_model::EMU_PER_POINT as f64 / map.digit;
        doc.set_attr(format, "defaultColWidth", &format!("{}", (chars * 256.0).round() / 256.0));
    }
    doc.append_element(root, "sheetData");
    let margins = doc.append_element(root, "pageMargins");
    for (a, v) in [("left", "0.7"), ("right", "0.7"), ("top", "0.75"), ("bottom", "0.75"), ("header", "0.3"), ("footer", "0.3")] {
        doc.set_attr(margins, a, v);
    }
    package.set_xml(&part, doc);
    package.set_override(&part, CT_WORKSHEET)?;
    let dir = map.workbook.rsplit_once('/').map_or("", |(d, _)| d);
    let target = part.strip_prefix(&format!("{dir}/")).map_or(format!("/{part}"), str::to_string);
    let rid = package.add_relationship(&map.workbook, REL_WORKSHEET, &target, false)?;
    let sheet_id = map.sheet_ids.iter().copied().max().unwrap_or(0) + 1;
    let doc = package.xml_mut(&map.workbook)?;
    let (sheets, list) = sheet_elements(doc).ok_or_else(|| Error::new("ブックに sheets がありません"))?;
    let prefix = doc.prefix(sheets).map(|p| format!("{p}:")).unwrap_or_default();
    let r = doc.lookup_prefix(sheets, REL_NS).unwrap_or_else(|| "r".into());
    let e = doc.insert_element(XmlDocument::ROOT, 0, &format!("{prefix}sheet"));
    doc.detach(e);
    doc.set_attr(e, "name", name);
    doc.set_attr(e, "sheetId", &sheet_id.to_string());
    doc.set_attr(e, &format!("{r}:id"), &rid);
    match list.get(index) {
        Some(&next) => doc.insert_before(next, e),
        None => {
            let n = doc.children(sheets).len();
            doc.insert(sheets, n, e);
        }
    }
    shift_sheet_indexes(doc, index, 1);
    map.sheet_parts.insert(index, Some(part));
    map.sheet_ids.insert(index, sheet_id);
    let name = name.to_string();
    update_app(package, |names| names.insert(index.min(names.len()), name.clone()))
}

fn delete_sheet(package: &mut Package, map: &mut SourceMap, before: &Presentation, index: usize) -> Result<(), Error> {
    let workbook = before.workbook.as_deref().ok_or_else(|| unsupported("このシートの削除"))?;
    let name = workbook.sheets.get(index).map(|s| s.name.clone()).ok_or_else(|| unsupported("このシートの削除"))?;
    let part = map.sheet_parts.get(index).cloned().flatten();
    let sheet_id = map.sheet_ids.get(index).copied();
    // The sheet's element and relationship.
    let doc = package.xml_mut(&map.workbook)?;
    let (_, list) = sheet_elements(doc).ok_or_else(|| Error::new("ブックに sheets がありません"))?;
    let element = *list.get(index).ok_or_else(|| Error::new("シートが見つかりません"))?;
    let rid = doc.attr_local(element, "id").map(str::to_string);
    doc.detach(element);
    shift_sheet_indexes(doc, index, -1);
    if let Some(rid) = rid {
        package.remove_relationship(&map.workbook, &rid)?;
    }
    map.sheet_parts.remove(index);
    map.sheet_ids.remove(index);
    if let Some(part) = &part {
        let rels = ibis_ooxml::package::relationships_part(part);
        package.remove(part);
        package.remove_override(part)?;
        if package.contains(&rels) {
            package.remove(&rels);
        }
    }
    // References to it become #REF!.
    rewrite_all(package, map, &|f| ibis_calc::formula::remove_sheet(f, &name))?;
    // Its entries of the calculation chain.
    if let (Some(chain), Some(sid)) = (&map.calc_chain, sheet_id)
        && package.contains(chain)
    {
        let doc = package.xml_mut(chain)?;
        if let Some(root) = doc.root_element() {
            // Each entry's sheet (inherited when `i` is left out).
            let mut sheet = 0u32;
            let entries: Vec<(NodeId, u32)> = doc
                .children_named(root, "c")
                .collect::<Vec<_>>()
                .into_iter()
                .map(|e| {
                    if let Some(i) = doc.attr(e, "i").and_then(|v| v.parse().ok()) {
                        sheet = i;
                    }
                    (e, sheet)
                })
                .collect();
            let mut previous: Option<u32> = None;
            for (e, sheet) in entries {
                if sheet == sid {
                    doc.detach(e);
                    continue;
                }
                // Write the sheet where it can no longer be inherited.
                if previous != Some(sheet) && doc.attr(e, "i").is_none() {
                    doc.set_attr(e, "i", &sheet.to_string());
                }
                previous = Some(sheet);
            }
            if doc.children_named(root, "c").next().is_none() {
                let workbook = map.workbook.clone();
                let chain = chain.clone();
                for rel in package.relationships(&workbook)? {
                    if rel.target_part(&workbook).as_deref() == Some(chain.as_str()) {
                        package.remove_relationship(&workbook, &rel.id)?;
                    }
                }
                package.remove(&chain);
                package.remove_override(&chain)?;
                map.calc_chain = None;
            }
        }
    }
    update_app(package, |names| {
        if index < names.len() {
            names.remove(index);
        }
    })
}

/// A sheet's element moved in `sheets` (and its part with it in the map);
/// defined names' `localSheetId`, the active and first tabs and the
/// titles in docProps/app.xml follow.
fn move_sheet(package: &mut Package, map: &mut SourceMap, from: usize, to: usize) -> Result<(), Error> {
    let doc = package.xml_mut(&map.workbook)?;
    let (sheets, list) = sheet_elements(doc).ok_or_else(|| Error::new("ブックに sheets がありません"))?;
    if from >= list.len() || to >= list.len() || from == to {
        return Ok(());
    }
    let element = list[from];
    doc.detach(element);
    let rest: Vec<NodeId> = doc.children_named(sheets, "sheet").collect();
    match rest.get(to) {
        Some(&next) => doc.insert_before(next, element),
        None => {
            let n = doc.children(sheets).len();
            doc.insert(sheets, n, element);
        }
    }
    let new_index = |i: usize| -> usize {
        if i == from {
            to
        } else if from < to && i > from && i <= to {
            i - 1
        } else if to < from && i >= to && i < from {
            i + 1
        } else {
            i
        }
    };
    let Some(root) = doc.root_element() else { return Ok(()) };
    if let Some(names) = doc.child(root, "definedNames") {
        for n in doc.children_named(names, "definedName").collect::<Vec<_>>() {
            if let Some(i) = doc.attr(n, "localSheetId").and_then(|v| v.parse::<usize>().ok()) {
                doc.set_attr(n, "localSheetId", &new_index(i).to_string());
            }
        }
    }
    if let Some(views) = doc.child(root, "bookViews") {
        for v in doc.children_named(views, "workbookView").collect::<Vec<_>>() {
            for attr in ["activeTab", "firstSheet"] {
                let i = doc.attr(v, attr).and_then(|x| x.parse::<usize>().ok()).unwrap_or(0);
                let n = new_index(i);
                if n == 0 {
                    doc.remove_attr(v, attr);
                } else {
                    doc.set_attr(v, attr, &n.to_string());
                }
            }
        }
    }
    let part = map.sheet_parts.remove(from);
    map.sheet_parts.insert(to, part);
    let id = map.sheet_ids.remove(from);
    map.sheet_ids.insert(to, id);
    update_app(package, |names| {
        if from < names.len() && to < names.len() {
            let n = names.remove(from);
            names.insert(to, n);
        }
    })
}

/// `sheet/@state` hidden or not; the book's active tab (and first tab
/// shown) moved off a sheet hidden.
fn hide_sheet(package: &mut Package, map: &mut SourceMap, index: usize, hidden: bool) -> Result<(), Error> {
    let doc = package.xml_mut(&map.workbook)?;
    let (_, list) = sheet_elements(doc).ok_or_else(|| Error::new("ブックに sheets がありません"))?;
    let element = *list.get(index).ok_or_else(|| Error::new("シートが見つかりません"))?;
    if hidden {
        doc.set_attr(element, "state", "hidden");
    } else {
        doc.remove_attr(element, "state");
    }
    let visible: Vec<usize> = list.iter().enumerate().filter(|(_, e)| doc.attr(**e, "state").is_none_or(|s| s == "visible")).map(|(i, _)| i).collect();
    let Some(&first) = visible.first() else { return Err(unsupported("すべてのシートを非表示にすること")) };
    let Some(root) = doc.root_element() else { return Ok(()) };
    if let Some(views) = doc.child(root, "bookViews") {
        for view in doc.children_named(views, "workbookView").collect::<Vec<_>>() {
            for attr in ["activeTab", "firstSheet"] {
                let current = doc.attr(view, attr).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
                if !visible.contains(&current) {
                    if first == 0 {
                        doc.remove_attr(view, attr);
                    } else {
                        doc.set_attr(view, attr, &first.to_string());
                    }
                }
            }
        }
    }
    // A hidden sheet's tab is not selected.
    if hidden
        && let Some(Some(part)) = map.sheet_parts.get(index).cloned()
    {
        let doc = package.xml_mut(&part)?;
        let views: Vec<_> = doc.descendants(doc.root_element().unwrap_or(ibis_ooxml::XmlDocument::ROOT)).into_iter().filter(|&n| doc.local_name(n) == "sheetView").collect();
        for v in views {
            doc.remove_attr(v, "tabSelected");
        }
    }
    Ok(())
}

fn rename_sheet(package: &mut Package, map: &mut SourceMap, before: &Presentation, index: usize, name: &str) -> Result<(), Error> {
    let workbook = before.workbook.as_deref().ok_or_else(|| unsupported("このシートの名前の変更"))?;
    let old = workbook.sheets.get(index).map(|s| s.name.clone()).ok_or_else(|| unsupported("このシートの名前の変更"))?;
    let doc = package.xml_mut(&map.workbook)?;
    let (_, list) = sheet_elements(doc).ok_or_else(|| Error::new("ブックに sheets がありません"))?;
    let element = *list.get(index).ok_or_else(|| Error::new("シートが見つかりません"))?;
    doc.set_attr(element, "name", name);
    let new = name.to_string();
    rewrite_all(package, map, &|f| ibis_calc::formula::rename_sheet(f, &old, &new))?;
    update_app(package, |names| {
        if let Some(n) = names.get_mut(index) {
            *n = new.clone();
        }
    })
}

fn insert_header_cells(doc: &mut XmlDocument, headers: &[(u32, u32, String)]) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let data = doc.child(root, "sheetData").ok_or_else(|| Error::new("シートに sheetData がありません"))?;
    let prefix = doc.prefix(root).map(|p| format!("{p}:")).unwrap_or_default();
    for &(col, row, ref name) in headers {
        let row_1based = row + 1;
        let rows: Vec<NodeId> = doc.children_named(data, "row").collect();
        let mut row_node = None;
        let mut after_row = None;
        for r_node in rows {
            let r_idx = doc.attr(r_node, "r").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
            if r_idx == row_1based {
                row_node = Some(r_node);
                break;
            } else if r_idx > row_1based {
                after_row = Some(r_node);
                break;
            }
        }
        let row_node = match row_node {
            Some(r) => r,
            None => {
                let r = detached(doc, &format!("{prefix}row"));
                doc.set_attr(r, "r", &row_1based.to_string());
                match after_row {
                    Some(after) => doc.insert_before(after, r),
                    None => {
                        let count = doc.children(data).len();
                        doc.insert(data, count, r);
                    }
                }
                r
            }
        };

        let cells: Vec<NodeId> = doc.children_named(row_node, "c").collect();
        let mut existing_cell = None;
        let mut before_cell = None;
        let mut style_to_copy = None;
        for c_node in cells {
            let c_col = doc.attr(c_node, "r").and_then(crate::xlsx::cell_ref).map(|(c, _)| c).unwrap_or(0);
            if c_col == col {
                existing_cell = Some(c_node);
                break;
            } else if c_col > col {
                if before_cell.is_none() {
                    before_cell = Some(c_node);
                }
            } else if let Some(s) = doc.attr(c_node, "s") {
                style_to_copy = Some(s.to_string());
            }
        }
        if existing_cell.is_some() {
            continue;
        }
        if style_to_copy.is_none() {
            if let Some(b) = before_cell {
                style_to_copy = doc.attr(b, "s").map(str::to_string);
            }
        }

        let c_name = cell_name(col, row);
        let new_c = detached(doc, &format!("{prefix}c"));
        doc.set_attr(new_c, "r", &c_name);
        doc.set_attr(new_c, "t", "inlineStr");
        if let Some(s) = style_to_copy {
            doc.set_attr(new_c, "s", &s);
        }
        match before_cell {
            Some(before) => doc.insert_before(before, new_c),
            None => {
                let count = doc.children(row_node).len();
                doc.insert(row_node, count, new_c);
            }
        }
        let new_is = doc.insert_element(new_c, 0, &format!("{prefix}is"));
        let new_t = doc.insert_element(new_is, 0, &format!("{prefix}t"));
        doc.append_text(new_t, name);
    }
    Ok(())
}
