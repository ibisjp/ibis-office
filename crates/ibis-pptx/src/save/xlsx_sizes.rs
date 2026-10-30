// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Column widths, row heights and merged ranges written into a sheet's XML.

use ibis_doc_model::{CellRange, EMU_PER_POINT, Emu, Sheet, cell_name};
use ibis_ooxml::{NodeId, XmlDocument};

use crate::Error;

fn prefix_of(doc: &XmlDocument, id: NodeId) -> String {
    doc.prefix(id).map(|p| format!("{p}:")).unwrap_or_default()
}

fn detached(doc: &mut XmlDocument, name: &str) -> NodeId {
    let e = doc.insert_element(XmlDocument::ROOT, 0, name);
    doc.detach(e);
    e
}

/// Put `e` among `parent`'s children before the first of `before` present
/// (the schema's order), or at the end.
fn place(doc: &mut XmlDocument, parent: NodeId, e: NodeId, before: &[&str]) {
    let anchor = doc.elements(parent).find(|&c| before.contains(&doc.local_name(c)));
    match anchor {
        Some(a) => doc.insert_before(a, e),
        None => {
            let n = doc.children(parent).len();
            doc.insert(parent, n, e);
        }
    }
}

/// Points as a number for XML (no trailing zeros).
fn number(v: f64) -> String {
    let s = format!("{v:.8}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

/// A column width in points as Excel stores it: characters of the widest
/// digit, to 1/256.
fn width_chars(size: Emu, digit: f64) -> f64 {
    let pt = size as f64 / EMU_PER_POINT as f64;
    (pt / digit * 256.0).round() / 256.0
}

pub(crate) fn write_sizes(doc: &mut XmlDocument, old: &Sheet, new: &Sheet, digit: f64) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    // Columns: runs of changed columns with the same new width. Only widths the user fixed are saved (`customWidth`); a width the
    // sheet chose by itself (a number entered wider than the column) is not, so the file opens with the column as it was (as Excel's).
    // A column given back to the sheet ("fit to selection") keeps its new width, without `customWidth`.
    let in_range = |c: u32| (c as usize) < new.columns.len() && (c as usize) < old.columns.len();
    let mut runs: Vec<(u32, u32, Emu, bool)> = Vec::new();
    for c in 0..new.columns.len().max(old.columns.len()) as u32 {
        if !in_range(c) {
            continue;
        }
        let w = new.columns[c as usize];
        let (fixed, was_fixed) = (new.custom_columns.contains(&c), old.custom_columns.contains(&c));
        let changed = old.columns[c as usize] != w || fixed != was_fixed;
        if !changed || (w != 0 && !fixed && !was_fixed) {
            continue;
        }
        match runs.last_mut() {
            Some(r) if r.1 + 1 == c && r.2 == w && r.3 == fixed => r.1 = c,
            _ => runs.push((c, c, w, fixed)),
        }
    }
    if !runs.is_empty() {
        let cols = match doc.child(root, "cols") {
            Some(c) => c,
            None => {
                let c = detached(doc, &format!("{prefix}cols"));
                place(doc, root, c, &["sheetData"]);
                c
            }
        };
        for (a, b, w, fixed) in runs {
            // Split the col elements that overlap [a, b] (1-based in XML).
            let (a1, b1) = (a + 1, b + 1);
            for col in doc.children_named(cols, "col").collect::<Vec<_>>() {
                let (Some(min), Some(max)) = (doc.attr(col, "min").and_then(|v| v.parse::<u32>().ok()), doc.attr(col, "max").and_then(|v| v.parse::<u32>().ok())) else { continue };
                if max < a1 || min > b1 {
                    continue;
                }
                if min < a1 {
                    let left = doc.deep_clone(col);
                    doc.set_attr(left, "max", &(a1 - 1).to_string());
                    doc.insert_before(col, left);
                    doc.set_attr(col, "min", &a1.to_string());
                }
                if max > b1 {
                    let right = doc.deep_clone(col);
                    doc.set_attr(right, "min", &(b1 + 1).to_string());
                    doc.insert_after(col, right);
                    doc.set_attr(col, "max", &b1.to_string());
                }
            }
            // Every column of the run now has its own element, or none.
            let mut covered: Vec<(u32, u32, NodeId)> = doc
                .children_named(cols, "col")
                .filter_map(|c| Some((doc.attr(c, "min")?.parse().ok()?, doc.attr(c, "max")?.parse().ok()?, c)))
                .filter(|(min, max, _)| *max >= a1 && *min <= b1)
                .collect();
            // Gaps get new elements.
            let mut at = a1;
            let mut gaps = Vec::new();
            covered.sort_by_key(|c| c.0);
            for (min, max, _) in &covered {
                if *min > at {
                    gaps.push((at, min - 1));
                }
                at = max + 1;
            }
            if at <= b1 {
                gaps.push((at, b1));
            }
            for (g0, g1) in gaps {
                let e = detached(doc, &format!("{prefix}col"));
                doc.set_attr(e, "min", &g0.to_string());
                doc.set_attr(e, "max", &g1.to_string());
                // In order among the others.
                let next = doc.children_named(cols, "col").find(|&c| doc.attr(c, "min").and_then(|v| v.parse::<u32>().ok()).is_some_and(|m| m > g1));
                match next {
                    Some(n) => doc.insert_before(n, e),
                    None => {
                        let n = doc.children(cols).len();
                        doc.insert(cols, n, e);
                    }
                }
                covered.push((g0, g1, e));
            }
            for (_, _, e) in covered {
                if w == 0 {
                    doc.set_attr(e, "hidden", "1");
                    if doc.attr(e, "width").is_none() {
                        doc.set_attr(e, "width", &number(width_chars(old.columns[a as usize].max(1), digit)));
                    }
                } else {
                    doc.remove_attr(e, "hidden");
                    doc.set_attr(e, "width", &number(width_chars(w, digit)));
                    if fixed {
                        doc.set_attr(e, "customWidth", "1");
                    } else {
                        doc.remove_attr(e, "customWidth");
                    }
                }
            }
        }
    }
    // Rows.
    let data = doc.child(root, "sheetData").ok_or_else(|| Error::new("シートに sheetData がありません"))?;
    let changed_rows: Vec<u32> = (0..new.rows.len().min(old.rows.len()) as u32).filter(|&r| old.rows[r as usize] != new.rows[r as usize] && new.fitted_rows.get(&r) != Some(&new.rows[r as usize])).collect();
    for r in changed_rows {
        let h = new.rows[r as usize];
        let found = doc.children_named(data, "row").find(|&n| doc.attr(n, "r").and_then(|v| v.parse::<u32>().ok()) == Some(r + 1));
        let row = match found {
            Some(n) => n,
            None => {
                let e = detached(doc, &format!("{prefix}row"));
                doc.set_attr(e, "r", &(r + 1).to_string());
                let next = doc.children_named(data, "row").find(|&n| doc.attr(n, "r").and_then(|v| v.parse::<u32>().ok()).is_some_and(|x| x > r + 1));
                match next {
                    Some(n) => doc.insert_before(n, e),
                    None => {
                        let n = doc.children(data).len();
                        doc.insert(data, n, e);
                    }
                }
                e
            }
        };
        if h == 0 {
            doc.set_attr(row, "hidden", "1");
        } else {
            doc.remove_attr(row, "hidden");
            doc.set_attr(row, "ht", &number(h as f64 / EMU_PER_POINT as f64));
            doc.set_attr(row, "customHeight", "1");
        }
    }
    Ok(())
}

pub(crate) fn write_merges(doc: &mut XmlDocument, merges: &[CellRange]) {
    let Some(root) = doc.root_element() else { return };
    let prefix = prefix_of(doc, root);
    let text = |m: &CellRange| format!("{}:{}", cell_name(m.first_column, m.first_row), cell_name(m.last_column, m.last_row));
    let wanted: Vec<String> = merges.iter().map(text).collect();
    let list = doc.child(root, "mergeCells");
    let existing: Vec<(NodeId, String)> = list.map(|l| doc.children_named(l, "mergeCell").filter_map(|m| Some((m, doc.attr(m, "ref")?.to_string()))).collect()).unwrap_or_default();
    if existing.iter().map(|e| &e.1).eq(wanted.iter()) {
        return;
    }
    if wanted.is_empty() {
        if let Some(l) = list {
            doc.detach(l);
        }
        return;
    }
    let list = match list {
        Some(l) => l,
        None => {
            let l = detached(doc, &format!("{prefix}mergeCells"));
            // After sheetData (and the elements that follow it before mergeCells).
            place(doc, root, l, &["phoneticPr", "conditionalFormatting", "dataValidations", "hyperlinks", "printOptions", "pageMargins", "pageSetup", "headerFooter", "rowBreaks", "colBreaks", "customProperties", "cellWatches", "ignoredErrors", "smartTags", "drawing", "legacyDrawing", "legacyDrawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"]);
            l
        }
    };
    // Keep the ones that stay, drop the others, add the new ones at the end.
    for (node, r) in &existing {
        if !wanted.contains(r) {
            doc.detach(*node);
        }
    }
    for w in &wanted {
        if !existing.iter().any(|e| &e.1 == w) {
            let m = doc.append_element(list, &format!("{prefix}mergeCell"));
            doc.set_attr(m, "ref", w);
        }
    }
    let n = doc.children_named(list, "mergeCell").count();
    doc.set_attr(list, "count", &n.to_string());
}

/// Frozen or split panes in the first `sheetView`: `pane` with the split, the
/// top-left cell of the part that scrolls and `state="frozen"` or `state="split"`, and a
/// `selection` for that part (as Excel writes them); none when un-paned.
pub(crate) fn write_panes(doc: &mut XmlDocument, frozen: Option<(u32, u32)>, split: Option<(u32, u32)>) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    let views = match doc.child(root, "sheetViews") {
        Some(v) => v,
        None => {
            let v = detached(doc, &format!("{prefix}sheetViews"));
            place(doc, root, v, &["sheetFormatPr", "cols", "sheetData"]);
            v
        }
    };
    let view = match doc.child(views, "sheetView") {
        Some(v) => v,
        None => {
            let v = doc.insert_element(views, 0, &format!("{prefix}sheetView"));
            doc.set_attr(v, "workbookViewId", "0");
            v
        }
    };
    if let Some(p) = doc.child(view, "pane") {
        doc.detach(p);
    }
    // Selections of the frozen or split parts go with the pane.
    for sel in doc.children_named(view, "selection").collect::<Vec<_>>() {
        if doc.attr(sel, "pane").is_some() {
            doc.detach(sel);
        }
    }
    let (columns, rows, state) = if let Some((cols, r)) = frozen {
        (cols, r, "frozen")
    } else if let Some((cols, r)) = split {
        (cols, r, "split")
    } else {
        return Ok(());
    };
    let pane = detached(doc, &format!("{prefix}pane"));
    if columns > 0 {
        doc.set_attr(pane, "xSplit", &columns.to_string());
    }
    if rows > 0 {
        doc.set_attr(pane, "ySplit", &rows.to_string());
    }
    doc.set_attr(pane, "topLeftCell", &cell_name(columns, rows));
    let active = match (columns > 0, rows > 0) {
        (true, true) => "bottomRight",
        (true, false) => "topRight",
        _ => "bottomLeft",
    };
    doc.set_attr(pane, "activePane", active);
    doc.set_attr(pane, "state", state);
    let n = doc.children(view).len();
    let first = doc.elements(view).next();
    match first {
        Some(f) => doc.insert_before(f, pane),
        None => doc.insert(view, n, pane),
    }
    let sel = detached(doc, &format!("{prefix}selection"));
    doc.set_attr(sel, "pane", active);
    doc.insert_after(pane, sel);
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn write_frozen(doc: &mut XmlDocument, frozen: Option<(u32, u32)>) -> Result<(), Error> {
    write_panes(doc, frozen, None)
}

/// The sheet's data validation as the model has it: `dataValidations`
/// written again (after `conditionalFormatting`), or taken out if none.
pub(crate) fn write_validations(doc: &mut XmlDocument, rules: &[ibis_doc_model::Validation]) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    if let Some(old) = doc.child(root, "dataValidations") {
        doc.detach(old);
    }
    if rules.is_empty() {
        return Ok(());
    }
    let list = detached(doc, &format!("{prefix}dataValidations"));
    doc.set_attr(list, "count", &rules.len().to_string());
    for r in rules {
        let v = detached(doc, &format!("{prefix}dataValidation"));
        if r.kind != "none" {
            doc.set_attr(v, "type", &r.kind);
        }
        if let Some((style, title, text)) = &r.error {
            if style != "stop" {
                doc.set_attr(v, "errorStyle", style);
            }
            let _ = (title, text);
        }
        if r.kind != "list" && r.kind != "custom" && r.kind != "none" && r.operator != "between" {
            doc.set_attr(v, "operator", &r.operator);
        }
        if r.allow_blank {
            doc.set_attr(v, "allowBlank", "1");
        }
        if r.kind == "list" && !r.dropdown {
            doc.set_attr(v, "showDropDown", "1");
        }
        if r.prompt.is_some() {
            doc.set_attr(v, "showInputMessage", "1");
        }
        if r.error.is_some() {
            doc.set_attr(v, "showErrorMessage", "1");
        }
        if let Some((_, title, text)) = &r.error {
            if !title.is_empty() {
                doc.set_attr(v, "errorTitle", title);
            }
            if !text.is_empty() {
                doc.set_attr(v, "error", text);
            }
        }
        if let Some((title, text)) = &r.prompt {
            if !title.is_empty() {
                doc.set_attr(v, "promptTitle", title);
            }
            if !text.is_empty() {
                doc.set_attr(v, "prompt", text);
            }
        }
        let sqref: Vec<String> = r.ranges.iter().map(|m| if m.first_row == m.last_row && m.first_column == m.last_column { cell_name(m.first_column, m.first_row) } else { format!("{}:{}", cell_name(m.first_column, m.first_row), cell_name(m.last_column, m.last_row)) }).collect();
        doc.set_attr(v, "sqref", &sqref.join(" "));
        for (name, f) in [("formula1", &r.formula1), ("formula2", &r.formula2)] {
            if let Some(f) = f {
                let e = detached(doc, &format!("{prefix}{name}"));
                doc.set_text(e, &ibis_calc::formula::to_file(f));
                let n = doc.children(v).len();
                doc.insert(v, n, e);
            }
        }
        let n = doc.children(list).len();
        doc.insert(list, n, v);
    }
    place(doc, root, list, &["hyperlinks", "printOptions", "pageMargins", "pageSetup", "headerFooter", "rowBreaks", "colBreaks", "customProperties", "cellWatches", "ignoredErrors", "smartTags", "drawing", "legacyDrawing", "legacyDrawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"]);
    Ok(())
}

/// Outline levels (グループ化): `row/@outlineLevel` (rows made if need be),
/// `col/@outlineLevel` (col elements split as for widths), and
/// `sheetFormatPr/@outlineLevelRow`・`@outlineLevelCol`.
pub(crate) fn write_outline(doc: &mut XmlDocument, old: &Sheet, new: &Sheet, digit: f64) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    let data = doc.child(root, "sheetData").ok_or_else(|| Error::new("シートに sheetData がありません"))?;
    let rows: std::collections::BTreeSet<u32> = old.outline_rows.keys().chain(new.outline_rows.keys()).copied().collect();
    for r in rows {
        let level = new.outline_rows.get(&r).copied().unwrap_or(0);
        if old.outline_rows.get(&r).copied().unwrap_or(0) == level {
            continue;
        }
        let found = doc.children_named(data, "row").find(|&n| doc.attr(n, "r").and_then(|v| v.parse::<u32>().ok()) == Some(r + 1));
        let row = match found {
            Some(n) => n,
            None => {
                let e = detached(doc, &format!("{prefix}row"));
                doc.set_attr(e, "r", &(r + 1).to_string());
                let next = doc.children_named(data, "row").find(|&n| doc.attr(n, "r").and_then(|v| v.parse::<u32>().ok()).is_some_and(|x| x > r + 1));
                match next {
                    Some(n) => doc.insert_before(n, e),
                    None => {
                        let n = doc.children(data).len();
                        doc.insert(data, n, e);
                    }
                }
                e
            }
        };
        if level == 0 {
            doc.remove_attr(row, "outlineLevel");
        } else {
            doc.set_attr(row, "outlineLevel", &level.to_string());
        }
    }
    // Columns: one col element per changed column (split from runs).
    let columns: std::collections::BTreeSet<u32> = old.outline_columns.keys().chain(new.outline_columns.keys()).copied().collect();
    for c in columns {
        let level = new.outline_columns.get(&c).copied().unwrap_or(0);
        if old.outline_columns.get(&c).copied().unwrap_or(0) == level {
            continue;
        }
        let cols = match doc.child(root, "cols") {
            Some(x) => x,
            None => {
                let x = detached(doc, &format!("{prefix}cols"));
                place(doc, root, x, &["sheetData"]);
                x
            }
        };
        let c1 = c + 1;
        // Split a col element covering c into before / c / after.
        let covering = doc.children_named(cols, "col").find(|&e| {
            let (a, b) = (doc.attr(e, "min").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0), doc.attr(e, "max").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0));
            a <= c1 && c1 <= b
        });
        let col = match covering {
            Some(e) => {
                let (a, b) = (doc.attr(e, "min").and_then(|v| v.parse::<u32>().ok()).unwrap_or(c1), doc.attr(e, "max").and_then(|v| v.parse::<u32>().ok()).unwrap_or(c1));
                if a < c1 {
                    let left = doc.deep_clone(e);
                    doc.set_attr(left, "max", &(c1 - 1).to_string());
                    doc.insert_before(e, left);
                }
                if b > c1 {
                    let right = doc.deep_clone(e);
                    doc.set_attr(right, "min", &(c1 + 1).to_string());
                    doc.insert_after(e, right);
                }
                doc.set_attr(e, "min", &c1.to_string());
                doc.set_attr(e, "max", &c1.to_string());
                e
            }
            None => {
                let e = detached(doc, &format!("{prefix}col"));
                doc.set_attr(e, "min", &c1.to_string());
                doc.set_attr(e, "max", &c1.to_string());
                let w = new.columns.get(c as usize).copied().unwrap_or(0);
                doc.set_attr(e, "width", &number(width_chars(w.max(1), digit)));
                let next = doc.children_named(cols, "col").find(|&x| doc.attr(x, "min").and_then(|v| v.parse::<u32>().ok()).is_some_and(|m| m > c1));
                match next {
                    Some(n) => doc.insert_before(n, e),
                    None => {
                        let n = doc.children(cols).len();
                        doc.insert(cols, n, e);
                    }
                }
                e
            }
        };
        if level == 0 {
            doc.remove_attr(col, "outlineLevel");
        } else {
            doc.set_attr(col, "outlineLevel", &level.to_string());
        }
    }
    // The deepest levels on sheetFormatPr.
    let format = match doc.child(root, "sheetFormatPr") {
        Some(f) => f,
        None => {
            let f = detached(doc, &format!("{prefix}sheetFormatPr"));
            doc.set_attr(f, "defaultRowHeight", "15");
            place(doc, root, f, &["cols", "sheetData"]);
            f
        }
    };
    for (attr, levels) in [("outlineLevelRow", &new.outline_rows), ("outlineLevelCol", &new.outline_columns)] {
        match levels.values().max() {
            Some(m) => doc.set_attr(format, attr, &m.to_string()),
            None => doc.remove_attr(format, attr),
        }
    }
    Ok(())
}

/// Print settings: `pageMargins`, `pageSetup` (paper, orientation, scale
/// or fit to pages), `sheetPr/pageSetUpPr/@fitToPage`, `printOptions`
/// (centred) and `rowBreaks`/`colBreaks`, as the model has them.
pub(crate) fn write_print(doc: &mut XmlDocument, print: &ibis_doc_model::SheetPrint) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    let inches = |e: Emu| number(e as f64 / ibis_doc_model::EMU_PER_INCH as f64);
    // The order of a worksheet's elements after sheetData.
    let order = ["sheetCalcPr", "sheetProtection", "protectedRanges", "scenarios", "autoFilter", "sortState", "dataConsolidate", "customSheetViews", "mergeCells", "phoneticPr", "conditionalFormatting", "dataValidations", "hyperlinks", "printOptions", "pageMargins", "pageSetup", "headerFooter", "rowBreaks", "colBreaks", "customProperties", "cellWatches", "ignoredErrors", "smartTags", "drawing", "legacyDrawing", "legacyDrawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"];
    let element = |doc: &mut XmlDocument, name: &str| -> NodeId {
        if let Some(e) = doc.child(root, name) {
            return e;
        }
        let e = detached(doc, &format!("{prefix}{name}"));
        let after = order.iter().position(|n| *n == name).unwrap_or(0);
        place(doc, root, e, &order[after + 1..]);
        e
    };
    // printOptions
    let has_options = print.centered.0 || print.centered.1 || print.grid_lines || print.headings;
    if has_options {
        let o = element(doc, "printOptions");
        if print.centered.0 {
            doc.set_attr(o, "horizontalCentered", "1");
        } else {
            doc.remove_attr(o, "horizontalCentered");
        }
        if print.centered.1 {
            doc.set_attr(o, "verticalCentered", "1");
        } else {
            doc.remove_attr(o, "verticalCentered");
        }
        if print.grid_lines {
            doc.set_attr(o, "gridLines", "1");
            doc.set_attr(o, "gridLinesSet", "1");
        } else {
            doc.remove_attr(o, "gridLines");
            doc.remove_attr(o, "gridLinesSet");
        }
        if print.headings {
            doc.set_attr(o, "headings", "1");
        } else {
            doc.remove_attr(o, "headings");
        }
    } else if let Some(o) = doc.child(root, "printOptions") {
        doc.detach(o);
    }
    let m = element(doc, "pageMargins");
    for (i, name) in ["left", "right", "top", "bottom"].into_iter().enumerate() {
        doc.set_attr(m, name, &inches(print.margins[i]));
    }
    doc.set_attr(m, "header", &inches(print.header_margin));
    doc.set_attr(m, "footer", &inches(print.footer_margin));
    let setup = element(doc, "pageSetup");
    let mm = |e: Emu| (e as f64 / 36_000.0).round() as i64;
    let paper = match (mm(print.paper.0), mm(print.paper.1)) {
        (216, 279) | (279, 216) => 1,
        (216, 356) | (356, 216) => 5,
        (297, 420) | (420, 297) => 8,
        (148, 210) | (210, 148) => 11,
        (257, 364) | (364, 257) => 12,
        (182, 257) | (257, 182) => 13,
        (100, 148) | (148, 100) => 43,
        (200, 148) | (148, 200) => 44,
        _ => 9,
    };
    doc.set_attr(setup, "paperSize", &paper.to_string());
    doc.set_attr(setup, "orientation", if print.landscape { "landscape" } else { "portrait" });
    if print.scale != 100 {
        doc.set_attr(setup, "scale", &print.scale.to_string());
    } else {
        doc.remove_attr(setup, "scale");
    }
    match print.fit_to {
        Some((w, h)) => {
            doc.set_attr(setup, "fitToWidth", &w.to_string());
            doc.set_attr(setup, "fitToHeight", &h.to_string());
        }
        None => {
            doc.remove_attr(setup, "fitToWidth");
            doc.remove_attr(setup, "fitToHeight");
        }
    }
    if print.over_then_down {
        doc.set_attr(setup, "pageOrder", "overThenDown");
    } else {
        doc.remove_attr(setup, "pageOrder");
    }
    if print.black_and_white {
        doc.set_attr(setup, "blackAndWhite", "1");
    } else {
        doc.remove_attr(setup, "blackAndWhite");
    }
    if print.draft {
        doc.set_attr(setup, "draft", "1");
    } else {
        doc.remove_attr(setup, "draft");
    }
    if let Some(errors) = &print.cell_errors {
        doc.set_attr(setup, "errors", errors);
    } else {
        doc.remove_attr(setup, "errors");
    }
    if let Some(first_page) = print.first_page_number {
        doc.set_attr(setup, "firstPageNumber", &first_page.to_string());
    } else {
        doc.remove_attr(setup, "firstPageNumber");
    }
    if print.use_first_page_number {
        doc.set_attr(setup, "useFirstPageNumber", "1");
    } else {
        doc.remove_attr(setup, "useFirstPageNumber");
    }
    // A pageSetup's r:id (printer settings) stays as it was.
    // fitToPage on sheetPr.
    let sheet_pr = match doc.child(root, "sheetPr") {
        Some(p) => p,
        None => {
            let p = detached(doc, &format!("{prefix}sheetPr"));
            doc.insert(root, 0, p);
            p
        }
    };
    let setup_pr = doc.child(sheet_pr, "pageSetUpPr");
    match (print.fit_to.is_some(), setup_pr) {
        (true, Some(p)) => doc.set_attr(p, "fitToPage", "1"),
        (true, None) => {
            let p = detached(doc, &format!("{prefix}pageSetUpPr"));
            doc.set_attr(p, "fitToPage", "1");
            // After tabColor and outlinePr.
            let n = doc.children(sheet_pr).len();
            doc.insert(sheet_pr, n, p);
        }
        (false, Some(p)) => doc.remove_attr(p, "fitToPage"),
        (false, None) => {}
    }
    // headerFooter
    if !print.header_footer.is_empty() {
        let hf = element(doc, "headerFooter");
        if print.header_footer.different_first {
            doc.set_attr(hf, "differentFirst", "1");
        } else {
            doc.remove_attr(hf, "differentFirst");
        }
        if print.header_footer.different_odd_even {
            doc.set_attr(hf, "differentOddEven", "1");
        } else {
            doc.remove_attr(hf, "differentOddEven");
        }
        if !print.header_footer.scale_with_doc {
            doc.set_attr(hf, "scaleWithDoc", "0");
        } else {
            doc.remove_attr(hf, "scaleWithDoc");
        }
        if !print.header_footer.align_with_margins {
            doc.set_attr(hf, "alignWithMargins", "0");
        } else {
            doc.remove_attr(hf, "alignWithMargins");
        }
        for child in doc.children(hf).to_vec() {
            doc.detach(child);
        }
        let children = [
            ("oddHeader", &print.header_footer.odd_header),
            ("oddFooter", &print.header_footer.odd_footer),
            ("evenHeader", &print.header_footer.even_header),
            ("evenFooter", &print.header_footer.even_footer),
            ("firstHeader", &print.header_footer.first_header),
            ("firstFooter", &print.header_footer.first_footer),
        ];
        for (name, val) in children {
            if let Some(text) = val {
                let c = detached(doc, &format!("{prefix}{name}"));
                doc.set_text(c, text);
                let n = doc.children(hf).len();
                doc.insert(hf, n, c);
            }
        }
    } else if let Some(hf) = doc.child(root, "headerFooter") {
        doc.detach(hf);
    }
    // Manual page breaks.
    for (name, list, max) in [("rowBreaks", &print.row_breaks, 16383u32), ("colBreaks", &print.column_breaks, 1048575u32)] {
        if let Some(old) = doc.child(root, name) {
            doc.detach(old);
        }
        if list.is_empty() {
            continue;
        }
        let e = element(doc, name);
        doc.set_attr(e, "count", &list.len().to_string());
        doc.set_attr(e, "manualBreakCount", &list.len().to_string());
        for id in list {
            let b = detached(doc, &format!("{prefix}brk"));
            doc.set_attr(b, "id", &id.to_string());
            doc.set_attr(b, "max", &max.to_string());
            doc.set_attr(b, "man", "1");
            let n = doc.children(e).len();
            doc.insert(e, n, b);
        }
    }
    Ok(())
}

/// `sheetView/@showGridLines` (absent: shown).
pub(crate) fn write_grid_lines(doc: &mut XmlDocument, shown: bool) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    let views = match doc.child(root, "sheetViews") {
        Some(v) => v,
        None => {
            let v = detached(doc, &format!("{prefix}sheetViews"));
            place(doc, root, v, &["sheetFormatPr", "cols", "sheetData"]);
            v
        }
    };
    let view = match doc.child(views, "sheetView") {
        Some(v) => v,
        None => {
            let v = doc.insert_element(views, 0, &format!("{prefix}sheetView"));
            doc.set_attr(v, "workbookViewId", "0");
            v
        }
    };
    if shown {
        doc.remove_attr(view, "showGridLines");
    } else {
        doc.set_attr(view, "showGridLines", "0");
    }
    Ok(())
}

/// `sheetView/@showRowColHeaders` (absent: shown).
pub(crate) fn write_headings(doc: &mut XmlDocument, shown: bool) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    let views = match doc.child(root, "sheetViews") {
        Some(v) => v,
        None => {
            let v = detached(doc, &format!("{prefix}sheetViews"));
            place(doc, root, v, &["sheetFormatPr", "cols", "sheetData"]);
            v
        }
    };
    let view = match doc.child(views, "sheetView") {
        Some(v) => v,
        None => {
            let v = doc.insert_element(views, 0, &format!("{prefix}sheetView"));
            doc.set_attr(v, "workbookViewId", "0");
            v
        }
    };
    if shown {
        doc.remove_attr(view, "showRowColHeaders");
    } else {
        doc.set_attr(view, "showRowColHeaders", "0");
    }
    Ok(())
}

/// `sheetView/@showRuler` (absent: shown).
pub(crate) fn write_show_ruler(doc: &mut XmlDocument, shown: bool) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    let views = match doc.child(root, "sheetViews") {
        Some(v) => v,
        None => {
            let v = detached(doc, &format!("{prefix}sheetViews"));
            place(doc, root, v, &["sheetFormatPr", "cols", "sheetData"]);
            v
        }
    };
    let view = match doc.child(views, "sheetView") {
        Some(v) => v,
        None => {
            let v = doc.insert_element(views, 0, &format!("{prefix}sheetView"));
            doc.set_attr(v, "workbookViewId", "0");
            v
        }
    };
    if shown {
        doc.remove_attr(view, "showRuler");
    } else {
        doc.set_attr(view, "showRuler", "0");
    }
    Ok(())
}

/// `sheetView/@showFormulas` (absent: false).
pub(crate) fn write_show_formulas(doc: &mut XmlDocument, shown: bool) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    let views = match doc.child(root, "sheetViews") {
        Some(v) => v,
        None => {
            let v = detached(doc, &format!("{prefix}sheetViews"));
            place(doc, root, v, &["sheetFormatPr", "cols", "sheetData"]);
            v
        }
    };
    let view = match doc.child(views, "sheetView") {
        Some(v) => v,
        None => {
            let v = doc.insert_element(views, 0, &format!("{prefix}sheetView"));
            doc.set_attr(v, "workbookViewId", "0");
            v
        }
    };
    if shown {
        doc.set_attr(view, "showFormulas", "1");
    } else {
        doc.remove_attr(view, "showFormulas");
    }
    Ok(())
}

/// `sheetView/@showZeros` (absent: true).
pub(crate) fn write_show_zeros(doc: &mut XmlDocument, shown: bool) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    let views = match doc.child(root, "sheetViews") {
        Some(v) => v,
        None => {
            let v = detached(doc, &format!("{prefix}sheetViews"));
            place(doc, root, v, &["sheetFormatPr", "cols", "sheetData"]);
            v
        }
    };
    let view = match doc.child(views, "sheetView") {
        Some(v) => v,
        None => {
            let v = doc.insert_element(views, 0, &format!("{prefix}sheetView"));
            doc.set_attr(v, "workbookViewId", "0");
            v
        }
    };
    if shown {
        doc.remove_attr(view, "showZeros");
    } else {
        doc.set_attr(view, "showZeros", "0");
    }
    Ok(())
}

/// `sheetView/@zoomScale` (absent: 100%).
/// エラー チェックで無視するエラー: `ignoredErrors` written from the model.
pub(crate) fn write_ignored_errors(doc: &mut XmlDocument, errors: &[ibis_doc_model::error_check::IgnoredError]) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    if let Some(old) = doc.child(root, "ignoredErrors") {
        doc.detach(old);
    }
    if errors.is_empty() {
        return Ok(());
    }
    let list = detached(doc, &format!("{prefix}ignoredErrors"));
    for e in errors {
        let n = detached(doc, &format!("{prefix}ignoredError"));
        let sqref: Vec<String> = e.ranges.iter().map(|m| if m.first_row == m.last_row && m.first_column == m.last_column { cell_name(m.first_column, m.first_row) } else { format!("{}:{}", cell_name(m.first_column, m.first_row), cell_name(m.last_column, m.last_row)) }).collect();
        doc.set_attr(n, "sqref", &sqref.join(" "));
        for a in &e.attributes {
            doc.set_attr(n, a, "1");
        }
        let at = doc.children(list).len();
        doc.insert(list, at, n);
    }
    place(doc, root, list, &["smartTags", "drawing", "legacyDrawing", "legacyDrawingHF", "drawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"]);
    Ok(())
}

pub(crate) fn write_zoom(doc: &mut XmlDocument, zoom: Option<u16>, view_zoom_attr: Option<&str>) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    let views = match doc.child(root, "sheetViews") {
        Some(v) => v,
        None => {
            let v = detached(doc, &format!("{prefix}sheetViews"));
            place(doc, root, v, &["sheetFormatPr", "cols", "sheetData"]);
            v
        }
    };
    let view = match doc.child(views, "sheetView") {
        Some(v) => v,
        None => {
            let v = doc.insert_element(views, 0, &format!("{prefix}sheetView"));
            doc.set_attr(v, "workbookViewId", "0");
            v
        }
    };
    let attr_name = view_zoom_attr.unwrap_or("zoomScale");
    match zoom.filter(|&z| z > 0 && z != 100) {
        Some(z) => {
            doc.set_attr(view, attr_name, &z.to_string());
            if view_zoom_attr.is_some() {
                doc.set_attr(view, "zoomScale", &z.to_string());
            }
        }
        None => {
            doc.remove_attr(view, attr_name);
            if view_zoom_attr.is_some() {
                doc.remove_attr(view, "zoomScale");
            }
        }
    }
    Ok(())
}

/// `sheetView/@view` ("pageBreakPreview", "pageLayout" or normal).
pub(crate) fn write_page_break_preview(doc: &mut XmlDocument, mode: Option<&str>) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    let views = match doc.child(root, "sheetViews") {
        Some(v) => v,
        None => {
            let v = detached(doc, &format!("{prefix}sheetViews"));
            place(doc, root, v, &["sheetFormatPr", "cols", "sheetData"]);
            v
        }
    };
    let view = match doc.child(views, "sheetView") {
        Some(v) => v,
        None => {
            let v = doc.insert_element(views, 0, &format!("{prefix}sheetView"));
            doc.set_attr(v, "workbookViewId", "0");
            v
        }
    };
    if let Some(mode) = mode {
        doc.set_attr(view, "view", mode);
    } else {
        doc.remove_attr(view, "view");
    }
    Ok(())
}

/// `sheetPr/tabColor` (first in sheetPr), or none.
pub(crate) fn write_tab_color(doc: &mut XmlDocument, color: Option<ibis_doc_model::Rgb>) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    let sheet_pr = match doc.child(root, "sheetPr") {
        Some(p) => p,
        None => {
            if color.is_none() {
                return Ok(());
            }
            let p = detached(doc, &format!("{prefix}sheetPr"));
            doc.insert(root, 0, p);
            p
        }
    };
    if let Some(old) = doc.child(sheet_pr, "tabColor") {
        doc.detach(old);
    }
    if let Some(c) = color {
        let e = detached(doc, &format!("{prefix}tabColor"));
        doc.set_attr(e, "rgb", &format!("FF{:06X}", c.0 & 0xFFFFFF));
        doc.insert(sheet_pr, 0, e);
    }
    Ok(())
}

/// The sheet's `dataConsolidate`, or none.
pub(crate) fn write_data_consolidate(doc: &mut XmlDocument, consolidate: Option<&ibis_doc_model::DataConsolidate>) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    if let Some(old) = doc.child(root, "dataConsolidate") {
        doc.detach(old);
    }
    let Some(c) = consolidate else { return Ok(()) };
    let e = detached(doc, &format!("{prefix}dataConsolidate"));
    doc.set_attr(e, "function", c.function.ooxml());
    if c.left_labels {
        doc.set_attr(e, "leftLabels", "1");
    }
    if c.top_labels {
        doc.set_attr(e, "topLabels", "1");
    }
    if c.link {
        doc.set_attr(e, "link", "1");
    }
    if !c.data_refs.is_empty() {
        let refs = detached(doc, &format!("{prefix}dataRefs"));
        doc.set_attr(refs, "count", &c.data_refs.len().to_string());
        for r in &c.data_refs {
            let item = detached(doc, &format!("{prefix}dataRef"));
            doc.set_attr(item, "ref", r);
            let n = doc.children(refs).len();
            doc.insert(refs, n, item);
        }
        let n = doc.children(e).len();
        doc.insert(e, n, refs);
    }
    place(doc, root, e, &["customSheetViews", "mergeCells", "phoneticPr", "conditionalFormatting", "dataValidations", "hyperlinks", "printOptions", "pageMargins", "pageSetup", "headerFooter", "rowBreaks", "colBreaks", "customProperties", "cellWatches", "ignoredErrors", "smartTags", "drawing", "legacyDrawing", "legacyDrawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"]);
    Ok(())
}

/// The sheet's `autoFilter` as the model has it (after sheetData's
/// neighbours, before mergeCells), or none.
pub(crate) fn write_auto_filter(doc: &mut XmlDocument, filter: Option<&ibis_doc_model::AutoFilter>, colors: &std::collections::HashMap<u32, u32>) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    if let Some(old) = doc.child(root, "autoFilter") {
        doc.detach(old);
    }
    let Some(f) = filter else { return Ok(()) };
    let e = detached(doc, &format!("{prefix}autoFilter"));
    fill_auto_filter(doc, e, f, colors, &prefix);
    place(doc, root, e, &["sortState", "dataConsolidate", "customSheetViews", "mergeCells", "phoneticPr", "conditionalFormatting", "dataValidations", "hyperlinks", "printOptions", "pageMargins", "pageSetup", "headerFooter", "rowBreaks", "colBreaks", "customProperties", "cellWatches", "ignoredErrors", "smartTags", "drawing", "legacyDrawing", "legacyDrawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"]);
    Ok(())
}

/// An `autoFilter` element `e` (its children replaced) as `f` has it: its
/// `ref` and a `filterColumn` for each column filtered (a table's too).
pub(crate) fn fill_auto_filter(doc: &mut XmlDocument, e: NodeId, f: &ibis_doc_model::AutoFilter, colors: &std::collections::HashMap<u32, u32>, prefix: &str) {
    use ibis_doc_model::ColumnFilter;
    for c in doc.children(e).to_vec() {
        doc.detach(c);
    }
    let r = f.range;
    doc.set_attr(e, "ref", &format!("{}:{}", cell_name(r.first_column, r.first_row), cell_name(r.last_column, r.last_row)));
    // In column order, as Excel writes them.
    let mut columns: Vec<&(u32, ColumnFilter)> = f.columns.iter().collect();
    columns.sort_by_key(|(c, _)| *c);
    let element = |doc: &mut XmlDocument, name: &str| detached(doc, &format!("{prefix}{name}"));
    let append = |doc: &mut XmlDocument, parent: NodeId, child: NodeId| {
        let n = doc.children(parent).len();
        doc.insert(parent, n, child);
    };
    for (column, filter) in columns {
        let fc = element(doc, "filterColumn");
        doc.set_attr(fc, "colId", &(column - r.first_column).to_string());
        let inner = match filter {
            ColumnFilter::Values(values) | ColumnFilter::Dates { values, .. } => {
                let list = element(doc, "filters");
                if values.iter().any(String::is_empty) {
                    doc.set_attr(list, "blank", "1");
                }
                for v in values.iter().filter(|v| !v.is_empty()) {
                    let x = element(doc, "filter");
                    doc.set_attr(x, "val", v);
                    append(doc, list, x);
                }
                if let ColumnFilter::Dates { groups, .. } = filter {
                    for g in groups {
                        let x = element(doc, "dateGroupItem");
                        let depth = ["year", "month", "day", "hour", "minute", "second"].iter().position(|k| *k == g.grouping).unwrap_or(0);
                        for (i, (name, v)) in [("year", g.year), ("month", g.month), ("day", g.day), ("hour", g.hour), ("minute", g.minute), ("second", g.second)].into_iter().enumerate() {
                            if i <= depth {
                                doc.set_attr(x, name, &v.to_string());
                            }
                        }
                        doc.set_attr(x, "dateTimeGrouping", &g.grouping);
                        append(doc, list, x);
                    }
                }
                list
            }
            ColumnFilter::AboveAverage(above) => {
                let d = element(doc, "dynamicFilter");
                doc.set_attr(d, "type", if *above { "aboveAverage" } else { "belowAverage" });
                d
            }
            ColumnFilter::Period(kind) => {
                let d = element(doc, "dynamicFilter");
                doc.set_attr(d, "type", kind);
                d
            }
            ColumnFilter::Top { top, percent, value } => {
                let t = element(doc, "top10");
                if !*top {
                    doc.set_attr(t, "top", "0");
                }
                if *percent {
                    doc.set_attr(t, "percent", "1");
                }
                doc.set_attr(t, "val", value);
                t
            }
            ColumnFilter::Custom { and, conditions } => {
                let list = element(doc, "customFilters");
                if *and {
                    doc.set_attr(list, "and", "1");
                }
                for (op, value) in conditions {
                    let x = element(doc, "customFilter");
                    if op != "equal" {
                        doc.set_attr(x, "operator", op);
                    }
                    doc.set_attr(x, "val", value);
                    append(doc, list, x);
                }
                list
            }
            ColumnFilter::Color { cell, .. } => {
                let c = element(doc, "colorFilter");
                if let Some(id) = colors.get(column) {
                    doc.set_attr(c, "dxfId", &id.to_string());
                }
                if !*cell {
                    doc.set_attr(c, "cellColor", "0");
                }
                c
            }
            ColumnFilter::Icon { icon_set, icon_id } => {
                let c = element(doc, "iconFilter");
                doc.set_attr(c, "iconSet", icon_set);
                doc.set_attr(c, "iconId", &icon_id.to_string());
                c
            }
        };
        doc.insert(fc, 0, inner);
        append(doc, e, fc);
    }
}

/// The sheet's conditional formats as the model has them: the
/// Whether a rule tree can be reused while only management flags change.
pub(crate) fn conditional_content_eq(a: &ibis_doc_model::ConditionalFormat, b: &ibis_doc_model::ConditionalFormat) -> bool {
    a.ranges == b.ranges && a.rules.len() == b.rules.len()
        && a.rules.iter().zip(&b.rules).all(|(a, b)| a.kind == b.kind && a.style == b.style)
}

pub(crate) fn conditional_sources_eq(a: &[ibis_doc_model::ConditionalFormat], b: &[ibis_doc_model::ConditionalFormat]) -> bool {
    a.len()==b.len() && a.iter().zip(b).all(|(a,b)|a.source_index==b.source_index)
}

/// Match each reusable subtree once; adding an identical rule still needs
/// a new subtree and dxf after the original has been consumed.
pub(crate) fn conditional_matches(old: &[ibis_doc_model::ConditionalFormat], new: &[ibis_doc_model::ConditionalFormat]) -> Vec<Option<usize>> {
    let same_source = |a: &ibis_doc_model::ConditionalFormat, b: &ibis_doc_model::ConditionalFormat| a.source_index == b.source_index;
    // Management keeps model storage order. Prefer that identity even if
    // two unmodelled rules have identical model content and swapped flags.
    if old.len() == new.len() && old.iter().zip(new).all(|(a, b)| same_source(a,b) && conditional_content_eq(a, b)) {
        return (0..new.len()).map(Some).collect();
    }
    let mut used = vec![false; old.len()];
    new.iter().map(|f| {
        let index = old.iter().enumerate().position(|(i, a)| !used[i] && same_source(a,f) && a == f)
            .or_else(|| old.iter().enumerate().position(|(i, a)| !used[i] && same_source(a,f) && conditional_content_eq(a, f)));
        if let Some(i) = index { used[i] = true; }
        index
    }).collect()
}

/// Locate edited imports by explicit provenance, never by similar content.
pub(crate) fn conditional_origins(old: &[ibis_doc_model::ConditionalFormat], new: &[ibis_doc_model::ConditionalFormat]) -> Result<Vec<Option<usize>>, Error> {
    let mut used = std::collections::HashSet::new();
    new.iter().map(|f| {
        let Some(source) = f.source_index else { return Ok(None); };
        let found: Vec<_> = old.iter().enumerate().filter(|(_,a)| a.source_index == Some(source)).map(|(i,_)|i).collect();
        let [index] = found.as_slice() else { return Err(Error::new("条件付き書式の元XML対応が一意ではありません")); };
        if !used.insert(*index) { return Err(Error::new("条件付き書式の元XML対応が重複しています")); }
        Ok(Some(*index))
    }).collect()
}

/// `conditionalFormatting` elements written again (after `mergeCells`),
/// each rule with its `dxfId` from `ids`.
pub(crate) fn write_conditional(doc: &mut XmlDocument, old_formats: &[ibis_doc_model::ConditionalFormat], formats: &[ibis_doc_model::ConditionalFormat], ids: &[Vec<Option<u32>>]) -> Result<(), Error> {
    use ibis_doc_model::{CfKind, CfValue};
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    let originals: Vec<_> = doc.children_named(root, "conditionalFormatting").collect();
    if originals.len() != old_formats.len() {
        return Err(Error::new("条件付き書式の元の XML とモデルの数が一致しません"));
    }
    for &old in &originals {
        doc.detach(old);
    }
    let matches = conditional_matches(old_formats, formats);
    let origins = conditional_origins(old_formats, formats)?;
    let rgb = |c: ibis_doc_model::Rgb| format!("FF{:06X}", c.0 & 0xFFFFFF);
    let text = |m: &CellRange| if m.first_row == m.last_row && m.first_column == m.last_column { cell_name(m.first_column, m.first_row) } else { format!("{}:{}", cell_name(m.first_column, m.first_row), cell_name(m.last_column, m.last_row)) };
    let after = ["dataValidations", "hyperlinks", "printOptions", "pageMargins", "pageSetup", "headerFooter", "rowBreaks", "colBreaks", "customProperties", "cellWatches", "ignoredErrors", "smartTags", "drawing", "legacyDrawing", "legacyDrawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"];
    for (((f, rule_ids), matched), origin) in formats.iter().zip(ids).zip(matches).zip(origins) {
        // Keep the entire original subtree of an unchanged format,
        // including unmodelled rules, attributes, extensions and dxf ids,
        // while patching priority and stopIfTrue when management changes.
        if let Some(i) = matched {
            let rules: Vec<_> = doc.children_named(originals[i], "cfRule").collect();
            if rules.len() != f.rules.len() {
                return Err(Error::new("条件付き書式の規則数が元のXMLと一致しません"));
            }
            for ((node, old), new) in rules.into_iter().zip(&old_formats[i].rules).zip(&f.rules) {
                if old.priority != new.priority { doc.set_attr(node, "priority", &new.priority.to_string()); }
                if old.stop != new.stop {
                    if new.stop { doc.set_attr(node, "stopIfTrue", "1"); }
                    else { doc.remove_attr(node, "stopIfTrue"); }
                }
            }
            place(doc, root, originals[i], &after);
            continue;
        }
        if let Some(i) = origin {
            let cf = originals[i];
            // Attach before resolving inherited namespaces on the original tree.
            place(doc, root, cf, &after);
            if doc.namespace(cf) != Some("http://schemas.openxmlformats.org/spreadsheetml/2006/main") { return Err(Error::new("未知namespaceの条件付き書式更新")); }
            let rules:Vec<_>=doc.children_named(cf,"cfRule").filter(|&n|doc.namespace(n)==Some("http://schemas.openxmlformats.org/spreadsheetml/2006/main")).collect();
            if rules.len()!=f.rules.len() || rules.len()!=old_formats[i].rules.len() { return Err(Error::new("条件付き書式の元規則数が異なります")); }
            doc.set_attr(cf,"sqref",&f.ranges.iter().map(text).collect::<Vec<_>>().join(" "));
            for (((node,old),new),id) in rules.into_iter().zip(&old_formats[i].rules).zip(&f.rules).zip(rule_ids) {
                let expected=match old.kind {CfKind::Expression(_)=>"expression",CfKind::CellIs {..}=>"cellIs",_=>return Err(Error::new("この種類の条件付き書式更新は未対応です"))};
                if doc.attr(node,"type")!=Some(expected) {return Err(Error::new("元の種類を変換する条件付き書式更新は未対応です"));}
                let (kind,operator,formulas)=match &new.kind {CfKind::Expression(s)=>("expression",None,vec![s.as_str()]),CfKind::CellIs {operator,formulas}=>("cellIs",Some(operator.as_str()),formulas.iter().map(String::as_str).collect()),_=>return Err(Error::new("この種類の条件付き書式更新は未対応です"))};
                if let Some(op)=operator && formulas.len()!=if matches!(op,"between"|"notBetween") {2}else{1} {return Err(Error::new("セル値比較の式数が不正です"));}
                doc.set_attr(node,"type",kind);if let Some(op)=operator {doc.set_attr(node,"operator",op);}else{doc.remove_attr(node,"operator");}
                doc.set_attr(node,"priority",&new.priority.max(1).to_string());if new.stop {doc.set_attr(node,"stopIfTrue","1");}else{doc.remove_attr(node,"stopIfTrue");}
                if old.style!=new.style {if let Some(id)=id {doc.set_attr(node,"dxfId",&id.to_string());}else{doc.remove_attr(node,"dxfId");}}
                let original:Vec<_>=doc.children_named(node,"formula").filter(|&n|doc.namespace(n)==Some("http://schemas.openxmlformats.org/spreadsheetml/2006/main")).collect();
                for (at,formula) in formulas.iter().enumerate() {
                    let n=if let Some(&n)=original.get(at) {n}else {let n=detached(doc,&format!("{prefix}formula"));if let Some(&last)=original.last(){doc.insert_after(last,n);}else{doc.insert(node,0,n);}n};
                    doc.set_text(n,&ibis_calc::formula::to_file(formula));
                }
                for &n in original.iter().skip(formulas.len()) {doc.detach(n);}
            }
            continue;
        }
        let cf = detached(doc, &format!("{prefix}conditionalFormatting"));
        doc.set_attr(cf, "sqref", &f.ranges.iter().map(text).collect::<Vec<_>>().join(" "));
        for (r, id) in f.rules.iter().zip(rule_ids) {
            let e = detached(doc, &format!("{prefix}cfRule"));
            let add = |doc: &mut XmlDocument, parent: NodeId, name: &str| -> NodeId {
                let x = detached(doc, &format!("{prefix}{name}"));
                let n = doc.children(parent).len();
                doc.insert(parent, n, x);
                x
            };
            let formula = |doc: &mut XmlDocument, f: &str| {
                let x = add(doc, e, "formula");
                doc.set_text(x, &ibis_calc::formula::to_file(f));
            };
            let cfvo = |doc: &mut XmlDocument, parent: NodeId, v: &CfValue| {
                let x = add(doc, parent, "cfvo");
                doc.set_attr(x, "type", &v.kind);
                if let Some(val) = &v.value {
                    doc.set_attr(x, "val", val);
                }
                if !v.gte {
                    doc.set_attr(x, "gte", "0");
                }
            };
            let kind = match &r.kind {
                CfKind::CellIs { .. } => "cellIs",
                CfKind::Expression(_) => "expression",
                CfKind::TimePeriod { .. } => "timePeriod",
                CfKind::Top10 { .. } => "top10",
                CfKind::AboveAverage { .. } => "aboveAverage",
                CfKind::Unique => "uniqueValues",
                CfKind::Duplicate => "duplicateValues",
                CfKind::ColorScale(_) => "colorScale",
                CfKind::DataBar { .. } => "dataBar",
                CfKind::IconSet { .. } => "iconSet",
                CfKind::Other => return Err(Error::new("未対応の条件付き書式の変更は保存できません")),
            };
            doc.set_attr(e, "type", kind);
            if let Some(id) = id {
                doc.set_attr(e, "dxfId", &id.to_string());
            }
            doc.set_attr(e, "priority", &r.priority.max(1).to_string());
            if r.stop {
                doc.set_attr(e, "stopIfTrue", "1");
            }
            match &r.kind {
                CfKind::CellIs { operator, formulas } => {
                    doc.set_attr(e, "operator", operator);
                    for f in formulas {
                        formula(doc, f);
                    }
                }
                CfKind::Expression(f) => formula(doc, f),
                CfKind::TimePeriod { period, formula: f } => {
                    doc.set_attr(e, "timePeriod", period);
                    formula(doc, f);
                }
                CfKind::Top10 { rank, percent, bottom } => {
                    if *percent {
                        doc.set_attr(e, "percent", "1");
                    }
                    if *bottom {
                        doc.set_attr(e, "bottom", "1");
                    }
                    doc.set_attr(e, "rank", &rank.to_string());
                }
                CfKind::AboveAverage { above, equal, std_dev } => {
                    if !above {
                        doc.set_attr(e, "aboveAverage", "0");
                    }
                    if *equal {
                        doc.set_attr(e, "equalAverage", "1");
                    }
                    if *std_dev > 0 {
                        doc.set_attr(e, "stdDev", &std_dev.to_string());
                    }
                }
                CfKind::ColorScale(points) => {
                    let cs = add(doc, e, "colorScale");
                    for (v, _) in points {
                        cfvo(doc, cs, v);
                    }
                    for (_, c) in points {
                        let x = add(doc, cs, "color");
                        doc.set_attr(x, "rgb", &rgb(*c));
                    }
                }
                CfKind::DataBar { min, max, color, gradient, negative_color, axis_color, axis_position, min_length, max_length, border_color, negative_border_color, direction } => {
                    let db = add(doc, e, "dataBar");
                    if let Some(min_l) = min_length {
                        doc.set_attr(db, "minLength", &min_l.to_string());
                    }
                    if let Some(max_l) = max_length {
                        doc.set_attr(db, "maxLength", &max_l.to_string());
                    }
                    cfvo(doc, db, min);
                    cfvo(doc, db, max);
                    let x = add(doc, db, "color");
                    doc.set_attr(x, "rgb", &rgb(*color));

                    if !gradient || negative_color.is_some() || axis_color.is_some() || axis_position.is_some() || border_color.is_some() || *direction != ibis_doc_model::DataBarDirection::Context {
                        let mut x14_attrs = String::new();
                        if !gradient {
                            x14_attrs.push_str(r#" gradient="0""#);
                        }
                        if let Some(min_l) = min_length {
                            x14_attrs.push_str(&format!(r#" minLength="{min_l}""#));
                        }
                        if let Some(max_l) = max_length {
                            x14_attrs.push_str(&format!(r#" maxLength="{max_l}""#));
                        }
                        if border_color.is_some() {
                            x14_attrs.push_str(r#" border="1""#);
                        }
                        match direction {
                            ibis_doc_model::DataBarDirection::LeftToRight => x14_attrs.push_str(r#" direction="leftToRight""#),
                            ibis_doc_model::DataBarDirection::RightToLeft => x14_attrs.push_str(r#" direction="rightToLeft""#),
                            ibis_doc_model::DataBarDirection::Context => {}
                        }
                        if let Some(pos) = axis_position {
                            let pos_str = match pos {
                                ibis_doc_model::DataBarAxisPosition::Automatic => "automatic",
                                ibis_doc_model::DataBarAxisPosition::Midpoint => "midpoint",
                                ibis_doc_model::DataBarAxisPosition::None => "none",
                            };
                            x14_attrs.push_str(&format!(r#" axisPosition="{pos_str}""#));
                        }
                        let mut x14_children = String::new();
                        if let Some(c) = border_color {
                            x14_children.push_str(&format!(r#"<x14:borderColor rgb="{}"/>"#, rgb(*c)));
                        }
                        if let Some(neg_c) = negative_color {
                            x14_children.push_str(&format!(r#"<x14:negativeFillColor rgb="{}"/>"#, rgb(*neg_c)));
                        }
                        if let Some(c) = negative_border_color {
                            x14_children.push_str(&format!(r#"<x14:negativeBorderColor rgb="{}"/>"#, rgb(*c)));
                        }
                        if let Some(ax_c) = axis_color {
                            x14_children.push_str(&format!(r#"<x14:axisColor rgb="{}"/>"#, rgb(*ax_c)));
                        }
                        let ext_xml = format!(
                            r#"<ext uri="{{B025F937-C7B1-47D3-B67F-A62EFF666E3E}}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:dataBar{x14_attrs}>{x14_children}</x14:dataBar></ext>"#
                        );
                        if let Ok(ext_doc) = XmlDocument::parse(ext_xml.as_bytes()) {
                            if let Some(ext_root) = ext_doc.root_element() {
                                let imported = doc.import(&ext_doc, ext_root);
                                let ext_lst = add(doc, e, "extLst");
                                let n = doc.children(ext_lst).len();
                                doc.insert(ext_lst, n, imported);
                            }
                        }
                    }
                }
                CfKind::IconSet { set, values, reverse, show_value, custom, icons } => {
                    let is = add(doc, e, "iconSet");
                    doc.set_attr(is, "iconSet", set);
                    if *reverse {
                        doc.set_attr(is, "reverse", "1");
                    }
                    if !show_value {
                        doc.set_attr(is, "showValue", "0");
                    }
                    if *custom {
                        doc.set_attr(is, "custom", "1");
                    }
                    for v in values {
                        cfvo(doc, is, v);
                    }

                    if *custom || !icons.is_empty() {
                        let ext_lst = add(doc, e, "extLst");
                        let ext = add(doc, ext_lst, "ext");
                        doc.set_attr(ext, "uri", "{B025F937-C7B1-47D3-B67F-A62EFF666E3E}");
                        let x14_is = add(doc, ext, "x14:iconSet");
                        doc.set_attr(x14_is, "xmlns:x14", "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main");
                        doc.set_attr(x14_is, "xmlns:xm", "http://schemas.microsoft.com/office/excel/2006/main");
                        doc.set_attr(x14_is, "iconSet", set);
                        if *custom {
                            doc.set_attr(x14_is, "custom", "1");
                        }
                        if *reverse {
                            doc.set_attr(x14_is, "reverse", "1");
                        }
                        if !show_value {
                            doc.set_attr(x14_is, "showValue", "0");
                        }
                        for v in values {
                            let x14_cfvo = add(doc, x14_is, "x14:cfvo");
                            let k = if v.kind == "min" { "autoMin" } else if v.kind == "max" { "autoMax" } else { &v.kind };
                            doc.set_attr(x14_cfvo, "type", k);
                            if let Some(val) = &v.value {
                                let xm_f = add(doc, x14_cfvo, "xm:f");
                                doc.set_text(xm_f, val);
                            }
                        }
                        for opt_icon in icons {
                            let cf_icon = add(doc, x14_is, "x14:cfIcon");
                            if let Some(icon) = opt_icon {
                                if icon.is_no_icon() {
                                    doc.set_attr(cf_icon, "iconSet", "NoIcon");
                                    doc.set_attr(cf_icon, "iconId", "-1");
                                } else {
                                    doc.set_attr(cf_icon, "iconSet", &icon.set);
                                    doc.set_attr(cf_icon, "iconId", &icon.id.to_string());
                                }
                            } else {
                                doc.set_attr(cf_icon, "iconSet", "NoIcon");
                                doc.set_attr(cf_icon, "iconId", "-1");
                            }
                        }
                    }
                }
                _ => {}
            }
            let n = doc.children(cf).len();
            doc.insert(cf, n, e);
        }
        place(doc, root, cf, &after);
    }
    Ok(())
}

/// シートの保護: `sheetProtection` as the model has it (the password's
/// hash, never the password; each operation not allowed as Excel's
/// attribute), or none.
pub(crate) fn write_protection(doc: &mut XmlDocument, protection: Option<&ibis_doc_model::SheetProtection>) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    if let Some(old) = doc.child(root, "sheetProtection") {
        doc.detach(old);
    }
    let Some(p) = protection else { return Ok(()) };
    let prefix = prefix_of(doc, root);
    let e = detached(doc, &format!("{prefix}sheetProtection"));
    if let Some(h) = &p.hash {
        doc.set_attr(e, "algorithmName", &h.algorithm);
        doc.set_attr(e, "hashValue", &h.hash);
        doc.set_attr(e, "saltValue", &h.salt);
        doc.set_attr(e, "spinCount", &h.spin_count.to_string());
    } else if let Some(legacy) = &p.legacy {
        doc.set_attr(e, "password", legacy);
    }
    doc.set_attr(e, "sheet", "1");
    let a = &p.allowed;
    // Written when it differs from the attribute's default.
    let flags = [
        ("objects", !a.objects, false),
        ("scenarios", !a.scenarios, false),
        ("formatCells", !a.format_cells, true),
        ("formatColumns", !a.format_columns, true),
        ("formatRows", !a.format_rows, true),
        ("insertColumns", !a.insert_columns, true),
        ("insertRows", !a.insert_rows, true),
        ("insertHyperlinks", !a.insert_hyperlinks, true),
        ("deleteColumns", !a.delete_columns, true),
        ("deleteRows", !a.delete_rows, true),
        ("selectLockedCells", !a.select_locked, false),
        ("sort", !a.sort, true),
        ("autoFilter", !a.auto_filter, true),
        ("pivotTables", !a.pivot_tables, true),
        ("selectUnlockedCells", !a.select_unlocked, false),
    ];
    for (name, value, default) in flags {
        if value != default {
            doc.set_attr(e, name, if value { "1" } else { "0" });
        }
    }
    // After sheetData (and sheetCalcPr), before the rest.
    let after = ["protectedRanges", "scenarios", "autoFilter", "sortState", "dataConsolidate", "customSheetViews", "mergeCells", "phoneticPr", "conditionalFormatting", "dataValidations", "hyperlinks", "printOptions", "pageMargins", "pageSetup", "headerFooter", "rowBreaks", "colBreaks", "customProperties", "cellWatches", "ignoredErrors", "smartTags", "drawing", "legacyDrawing", "legacyDrawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"];
    place(doc, root, e, &after);
    Ok(())
}

/// 範囲の編集の許可: `protectedRanges` as the model has it, or none.
pub(crate) fn write_protected_ranges(doc: &mut XmlDocument, protected_ranges: &[ibis_doc_model::ProtectedRange]) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    if let Some(old) = doc.child(root, "protectedRanges") {
        doc.detach(old);
    }
    if protected_ranges.is_empty() { return Ok(()); }
    let prefix = prefix_of(doc, root);
    let e = detached(doc, &format!("{prefix}protectedRanges"));
    for pr in protected_ranges {
        let node = detached(doc, &format!("{prefix}protectedRange"));
        doc.set_attr(node, "name", &pr.name);
        let sqref: Vec<String> = pr.ranges.iter().map(|r| {
            if r.first_row == r.last_row && r.first_column == r.last_column {
                ibis_doc_model::cell_name(r.first_column, r.first_row)
            } else {
                format!("{}:{}", ibis_doc_model::cell_name(r.first_column, r.first_row), ibis_doc_model::cell_name(r.last_column, r.last_row))
            }
        }).collect();
        doc.set_attr(node, "sqref", &sqref.join(" "));
        if let Some(h) = &pr.hash {
            doc.set_attr(node, "algorithmName", &h.algorithm);
            doc.set_attr(node, "hashValue", &h.hash);
            doc.set_attr(node, "saltValue", &h.salt);
            doc.set_attr(node, "spinCount", &h.spin_count.to_string());
        } else if let Some(legacy) = &pr.legacy {
            doc.set_attr(node, "password", legacy);
        }
        let n = doc.children(e).len();
        doc.insert(e, n, node);
    }
    let after = ["scenarios", "autoFilter", "sortState", "dataConsolidate", "customSheetViews", "mergeCells", "phoneticPr", "conditionalFormatting", "dataValidations", "hyperlinks", "printOptions", "pageMargins", "pageSetup", "headerFooter", "rowBreaks", "colBreaks", "customProperties", "cellWatches", "ignoredErrors", "smartTags", "drawing", "legacyDrawing", "legacyDrawingHF", "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst"];
    place(doc, root, e, &after);
    Ok(())
}

/// シナリオの登録と管理: `scenarios` as the model has it, or none.
pub(crate) fn write_scenarios(doc: &mut XmlDocument, scenarios: Option<&ibis_doc_model::SheetScenarios>) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    if let Some(old) = doc.child(root, "scenarios") {
        doc.detach(old);
    }
    let Some(sc) = scenarios else { return Ok(()) };
    if sc.scenarios.is_empty() { return Ok(()) };

    let prefix = prefix_of(doc, root);
    let add = |doc: &mut XmlDocument, parent: NodeId, name: &str| {
        let e = detached(doc, &format!("{prefix}{name}"));
        let n = doc.children(parent).len();
        doc.insert(parent, n, e);
        e
    };

    let sc_node = detached(doc, &format!("{prefix}scenarios"));
    if let Some(current) = sc.current {
        doc.set_attr(sc_node, "current", &current.to_string());
    }
    if let Some(show) = sc.show {
        doc.set_attr(sc_node, "show", &show.to_string());
    }
    if let Some(sqref) = &sc.sqref {
        if !sqref.is_empty() {
            doc.set_attr(sc_node, "sqref", sqref);
        }
    }

    for scenario in &sc.scenarios {
        let s_node = add(doc, sc_node, "scenario");
        doc.set_attr(s_node, "name", &scenario.name);
        if let Some(comment) = &scenario.comment {
            if !comment.is_empty() {
                doc.set_attr(s_node, "comment", comment);
            }
        }
        if let Some(user) = &scenario.user {
            if !user.is_empty() {
                doc.set_attr(s_node, "user", user);
            }
        }
        if scenario.locked {
            doc.set_attr(s_node, "locked", "1");
        }
        if scenario.hidden {
            doc.set_attr(s_node, "hidden", "1");
        }
        doc.set_attr(s_node, "count", &scenario.input_cells.len().to_string());

        for ic in &scenario.input_cells {
            let ic_node = add(doc, s_node, "inputCells");
            doc.set_attr(ic_node, "r", &ic.r);
            doc.set_attr(ic_node, "val", &ic.val);
            if let Some(num_fmt_id) = ic.num_fmt_id {
                doc.set_attr(ic_node, "numFmtId", &num_fmt_id.to_string());
            }
            if ic.deleted {
                doc.set_attr(ic_node, "deleted", "1");
            }
        }
    }

    let after = [
        "autoFilter", "sortState", "dataConsolidate", "customSheetViews", "mergeCells",
        "phoneticPr", "conditionalFormatting", "dataValidations", "hyperlinks", "printOptions",
        "pageMargins", "pageSetup", "headerFooter", "rowBreaks", "colBreaks", "customProperties",
        "cellWatches", "ignoredErrors", "smartTags", "drawing", "legacyDrawing", "legacyDrawingHF",
        "picture", "oleObjects", "controls", "webPublishItems", "tableParts", "extLst",
    ];
    place(doc, root, sc_node, &after);
    Ok(())
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// スパークライン（`extLst/ext/x14:sparklineGroups`）の書き込み。
pub(crate) fn write_sparklines(doc: &mut XmlDocument, sheet_name: &str, sparklines: &[ibis_doc_model::SparklineGroup]) -> Result<(), Error> {
    const SPARKLINE_URI: &str = "{05C60535-1F16-4fd2-B633-F4F36F0B64E0}";
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);

    let ext_lst = match doc.child(root, "extLst") {
        Some(e) => e,
        None => {
            if sparklines.is_empty() {
                return Ok(());
            }
            let e = detached(doc, &format!("{prefix}extLst"));
            let n = doc.children(root).len();
            doc.insert(root, n, e);
            e
        }
    };

    let existing: Vec<NodeId> = doc.elements(ext_lst)
        .filter(|&c| doc.local_name(c) == "ext" && doc.attr(c, "uri") == Some(SPARKLINE_URI))
        .collect();
    for c in existing {
        doc.detach(c);
    }

    if sparklines.is_empty() {
        if doc.elements(ext_lst).next().is_none() {
            doc.detach(ext_lst);
        }
        return Ok(());
    }

    let mut groups_xml = String::new();
    for g in sparklines {
        let type_attr = match g.kind {
            ibis_doc_model::SparklineKind::Line => r#"type="line""#,
            ibis_doc_model::SparklineKind::Column => r#"type="column""#,
            ibis_doc_model::SparklineKind::WinLoss => r#"type="stacked""#,
        };
        let mut attrs = format!(" {type_attr}");
        if g.line_weight != 75 {
            attrs.push_str(&format!(r#" lineWeight="{}""#, number(g.line_weight as f64 / 100.0)));
        }
        if g.markers { attrs.push_str(r#" markers="1""#); }
        if g.high { attrs.push_str(r#" high="1""#); }
        if g.low { attrs.push_str(r#" low="1""#); }
        if g.first { attrs.push_str(r#" first="1""#); }
        if g.last { attrs.push_str(r#" last="1""#); }
        if g.negative_points { attrs.push_str(r#" negative="1""#); }
        if g.display_x_axis { attrs.push_str(r#" displayXAxis="1""#); }
        if g.right_to_left { attrs.push_str(r#" rightToLeft="1""#); }
        if g.display_hidden { attrs.push_str(r#" displayHidden="1""#); }
        if g.date_axis { attrs.push_str(r#" dateAxis="1""#); }

        match g.empty {
            ibis_doc_model::SparklineEmpty::Gap => attrs.push_str(r#" displayEmptyCellsAs="gap""#),
            ibis_doc_model::SparklineEmpty::Zero => attrs.push_str(r#" displayEmptyCellsAs="zero""#),
            ibis_doc_model::SparklineEmpty::Span => attrs.push_str(r#" displayEmptyCellsAs="span""#),
        }
        match &g.min_axis {
            ibis_doc_model::SparklineAxis::Individual => {},
            ibis_doc_model::SparklineAxis::Group => attrs.push_str(r#" minAxisType="group""#),
            ibis_doc_model::SparklineAxis::Custom(v) => {
                attrs.push_str(r#" minAxisType="custom""#);
                attrs.push_str(&format!(r#" manualMin="{}""#, esc(v)));
            }
        }
        match &g.max_axis {
            ibis_doc_model::SparklineAxis::Individual => {},
            ibis_doc_model::SparklineAxis::Group => attrs.push_str(r#" maxAxisType="group""#),
            ibis_doc_model::SparklineAxis::Custom(v) => {
                attrs.push_str(r#" maxAxisType="custom""#);
                attrs.push_str(&format!(r#" manualMax="{}""#, esc(v)));
            }
        }

        let mut children_xml = String::new();
        let color_tag = |tag: &str, c: Option<ibis_doc_model::Rgb>| -> String {
            match c {
                Some(rgb) => format!(r#"<x14:{} rgb="FF{:06X}"/>"#, tag, rgb.0 & 0xFFFFFF),
                None => String::new(),
            }
        };
        children_xml.push_str(&color_tag("colorSeries", g.series));
        children_xml.push_str(&color_tag("colorNegative", g.negative));
        children_xml.push_str(&color_tag("colorAxis", g.axis));
        children_xml.push_str(&color_tag("colorMarkers", g.markers_color));
        children_xml.push_str(&color_tag("colorFirst", g.first_color));
        children_xml.push_str(&color_tag("colorLast", g.last_color));
        children_xml.push_str(&color_tag("colorHigh", g.high_color));
        children_xml.push_str(&color_tag("colorLow", g.low_color));

        if let Some(ref f) = g.date_range {
            let f_qual = if f.contains('!') {
                f.clone()
            } else if sheet_name.contains(|c: char| !c.is_alphanumeric() && c != '_') {
                format!("'{}'!{f}", sheet_name.replace('\'', "''"))
            } else {
                format!("{sheet_name}!{f}")
            };
            children_xml.push_str(&format!(r#"<xm:f>{}</xm:f>"#, esc(&f_qual)));
        }

        children_xml.push_str("<x14:sparklines>");
        for s in &g.sparklines {
            let range_str = if s.data.first_row == s.data.last_row && s.data.first_column == s.data.last_column {
                cell_name(s.data.first_column, s.data.first_row)
            } else {
                format!("{}:{}", cell_name(s.data.first_column, s.data.first_row), cell_name(s.data.last_column, s.data.last_row))
            };
            let sh_target = s.sheet.as_deref().unwrap_or(sheet_name);
            let f = if sh_target.contains(|c: char| !c.is_alphanumeric() && c != '_') {
                format!("'{}'!{}", sh_target.replace('\'', "''"), range_str)
            } else {
                format!("{sh_target}!{range_str}")
            };
            let sqref = cell_name(s.column, s.row);
            children_xml.push_str(&format!(r#"<x14:sparkline><xm:f>{}</xm:f><xm:sqref>{}</xm:sqref></x14:sparkline>"#, esc(&f), sqref));
        }
        children_xml.push_str("</x14:sparklines>");

        groups_xml.push_str(&format!(r#"<x14:sparklineGroup{attrs}>{children_xml}</x14:sparklineGroup>"#));
    }

    let ext_xml = format!(r#"<ext uri="{SPARKLINE_URI}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:sparklineGroups xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main">{groups_xml}</x14:sparklineGroups></ext>"#);
    let ext_doc = XmlDocument::parse(ext_xml.as_bytes()).map_err(|e| Error::new(format!("スパークライン XML の生成に失敗しました: {e}")))?;
    let ext_root = ext_doc.root_element().ok_or_else(|| Error::new("スパークライン XML が空です"))?;
    let imported = doc.import(&ext_doc, ext_root);
    if !prefix.is_empty() {
        doc.rename(imported, &format!("{prefix}ext"));
    }
    let n = doc.children(ext_lst).len();
    doc.insert(ext_lst, n, imported);
    Ok(())
}

pub(crate) fn write_custom_sheet_views(doc: &mut XmlDocument, views: &[ibis_doc_model::CustomSheetView]) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);
    if let Some(csv) = doc.child(root, "customSheetViews") {
        doc.detach(csv);
    }
    if views.is_empty() {
        return Ok(());
    }
    let list = detached(doc, &format!("{prefix}customSheetViews"));
    let before = [
        "mergeCells", "phoneticPr", "conditionalFormatting", "dataValidations", "hyperlinks",
        "printOptions", "pageMargins", "pageSetup", "headerFooter", "rowBreaks", "colBreaks",
        "customProperties", "drawing", "tableParts", "extLst",
    ];
    place(doc, root, list, &before);

    for v in views {
        let node = doc.append_element(list, &format!("{prefix}customSheetView"));
        doc.set_attr(node, "guid", &v.guid);
        if let Some(z) = v.zoom {
            doc.set_attr(node, "scale", &z.to_string());
        }
        doc.set_attr(node, "showGridLines", if v.grid_lines { "1" } else { "0" });
        doc.set_attr(node, "showRowCol", if v.headings { "1" } else { "0" });
        doc.set_attr(node, "showRuler", if v.show_ruler { "1" } else { "0" });
        if v.show_formulas {
            doc.set_attr(node, "showFormulas", "1");
        }
        if !v.show_zeros {
            doc.set_attr(node, "zeroValues", "0");
        }
        if v.page_break_preview {
            doc.set_attr(node, "view", "pageBreakPreview");
        }
        if let Some((x, y)) = v.frozen {
            let p = doc.append_element(node, &format!("{prefix}pane"));
            if x > 0 { doc.set_attr(p, "xSplit", &x.to_string()); }
            if y > 0 { doc.set_attr(p, "ySplit", &y.to_string()); }
            let top_left = cell_name(x, y);
            doc.set_attr(p, "topLeftCell", &top_left);
            let active = match (x > 0, y > 0) {
                (true, true) => "bottomRight",
                (true, false) => "topRight",
                (false, true) => "bottomLeft",
                (false, false) => "topLeft",
            };
            doc.set_attr(p, "activePane", active);
            doc.set_attr(p, "state", "frozen");
        } else if let Some((x, y)) = v.split {
            let p = doc.append_element(node, &format!("{prefix}pane"));
            if x > 0 { doc.set_attr(p, "xSplit", &x.to_string()); }
            if y > 0 { doc.set_attr(p, "ySplit", &y.to_string()); }
            let top_left = cell_name(x, y);
            doc.set_attr(p, "topLeftCell", &top_left);
            let active = match (x > 0, y > 0) {
                (true, true) => "bottomRight",
                (true, false) => "topRight",
                (false, true) => "bottomLeft",
                (false, false) => "topLeft",
            };
            doc.set_attr(p, "activePane", active);
            doc.set_attr(p, "state", "split");
        }
        if let Some((r, c)) = v.active_cell {
            let sel = doc.append_element(node, &format!("{prefix}selection"));
            let cell = cell_name(c, r);
            doc.set_attr(sel, "activeCell", &cell);
            doc.set_attr(sel, "sqref", &cell);
        }
        if let Some(print) = &v.print {
            let margins = doc.append_element(node, &format!("{prefix}pageMargins"));
            doc.set_attr(margins, "left", &number(print.margins[0] as f64 / ibis_doc_model::EMU_PER_INCH as f64));
            doc.set_attr(margins, "right", &number(print.margins[1] as f64 / ibis_doc_model::EMU_PER_INCH as f64));
            doc.set_attr(margins, "top", &number(print.margins[2] as f64 / ibis_doc_model::EMU_PER_INCH as f64));
            doc.set_attr(margins, "bottom", &number(print.margins[3] as f64 / ibis_doc_model::EMU_PER_INCH as f64));
            doc.set_attr(margins, "header", &number(print.header_margin as f64 / ibis_doc_model::EMU_PER_INCH as f64));
            doc.set_attr(margins, "footer", &number(print.footer_margin as f64 / ibis_doc_model::EMU_PER_INCH as f64));
            let opts = doc.append_element(node, &format!("{prefix}printOptions"));
            if print.grid_lines { doc.set_attr(opts, "gridLines", "1"); }
            if print.headings { doc.set_attr(opts, "headings", "1"); }
            let setup = doc.append_element(node, &format!("{prefix}pageSetup"));
            doc.set_attr(setup, "orientation", if print.landscape { "landscape" } else { "portrait" });
            doc.set_attr(setup, "scale", &print.scale.to_string());
        }
        if let Some(Some(af)) = &v.auto_filter {
            let filter_node = doc.append_element(node, &format!("{prefix}autoFilter"));
            doc.set_attr(filter_node, "ref", &format!("{}:{}", cell_name(af.range.first_column, af.range.first_row), cell_name(af.range.last_column, af.range.last_row)));
        }
    }
    Ok(())
}

/// Write sheet phonetic display and properties (`showPhonetic` on sheetView, `<phoneticPr>`).
pub(crate) fn write_phonetic(
    doc: &mut XmlDocument,
    show: bool,
    pr: Option<&ibis_doc_model::PhoneticProperties>,
) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("シートの XML が空です"))?;
    let prefix = prefix_of(doc, root);

    // sheetView showPhonetic
    let views = match doc.child(root, "sheetViews") {
        Some(v) => v,
        None => {
            let v = detached(doc, &format!("{prefix}sheetViews"));
            place(doc, root, v, &["sheetFormatPr", "cols", "sheetData"]);
            v
        }
    };
    let view = match doc.child(views, "sheetView") {
        Some(v) => v,
        None => {
            let v = doc.insert_element(views, 0, &format!("{prefix}sheetView"));
            doc.set_attr(v, "workbookViewId", "0");
            v
        }
    };
    if show {
        doc.set_attr(view, "showPhonetic", "1");
    } else {
        doc.remove_attr(view, "showPhonetic");
    }

    // phoneticPr (ECMA-376 Part 1 §18.3.1.65: fontId is required).
    if let Some(old) = doc.child(root, "phoneticPr") {
        doc.detach(old);
    }
    if let Some(p) = pr {
        let elem = detached(doc, &format!("{prefix}phoneticPr"));
        doc.set_attr(elem, "fontId", &p.font_id.to_string());
        doc.set_attr(elem, "type", p.phonetic_type.to_ooxml());
        doc.set_attr(elem, "alignment", p.alignment.to_ooxml());
        place(doc, root, elem, &[
            "conditionalFormatting",
            "dataValidations",
            "hyperlinks",
            "printOptions",
            "pageMargins",
            "pageSetup",
            "headerFooter",
            "rowBreaks",
            "colBreaks",
            "drawing",
            "legacyDrawing",
            "tableParts",
            "extLst",
        ]);
    }
    Ok(())
}

