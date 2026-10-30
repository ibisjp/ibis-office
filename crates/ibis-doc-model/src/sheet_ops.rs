// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Changes to a workbook's structure: rows and columns inserted and
//! deleted. They move cells, merged ranges and row heights or column
//! widths, and every formula of the workbook (and every defined name) that
//! refers to the sheet, as Excel does; references to deleted cells become
//! `#REF!`.
//!
//! The same functions change the model when the user edits it and, when
//! saving, the model as it was read, so that the saved file's cells can be
//! compared with the edited ones after the file's XML has had the same
//! changes (see `ibis-pptx/src/save/xlsx.rs`). The workbook keeps the
//! list of structural changes in the order they were made
//! ([`crate::Workbook::structure`]).

use std::sync::Arc;

use ibis_calc::formula::{Axis, adjust_references};

use crate::geometry::{Emu, EmuRect};
use crate::model::{Presentation, Shape, ShapeId, ShapeKind, Slide, SlideId};
use crate::sheet::{CellKind, CellRange, CellValue, Sheet, SheetCell, WorkbookSheet};

/// A structural change, on the sheet at `sheet` in the workbook's order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructureEdit {
    /// `count` rows (or columns) inserted before `at` (0-based).
    Insert { sheet: usize, axis: Axis, at: u32, count: u32 },
    /// `count` rows (or columns) deleted from `at`.
    Delete { sheet: usize, axis: Axis, at: u32, count: u32 },
    /// A new worksheet at `sheet`, on a new slide (with the ids given).
    AddSheet { sheet: usize, name: String, slide: SlideId, shape: ShapeId, content: Arc<Sheet> },
    /// The worksheet at `sheet` deleted: references to it become `#REF!`.
    DeleteSheet { sheet: usize },
    /// The worksheet at `sheet` renamed: references follow.
    RenameSheet { sheet: usize, name: String },
    /// The worksheet at `sheet` moved to position `to` (in the workbook's order).
    MoveSheet { sheet: usize, to: usize },
    /// The worksheet at `sheet` hidden (its slide goes), or shown again
    /// (on a new slide, with the ids given).
    HideSheet { sheet: usize, hidden: bool, slide: SlideId, shape: ShapeId },
}

impl StructureEdit {
    pub fn sheet(&self) -> usize {
        match self {
            Self::Insert { sheet, .. } | Self::Delete { sheet, .. } | Self::AddSheet { sheet, .. } | Self::DeleteSheet { sheet } | Self::RenameSheet { sheet, .. } | Self::HideSheet { sheet, .. } | Self::MoveSheet { sheet, .. } => *sheet,
        }
    }

    /// A row or column change as (axis, at, signed count: positive to insert).
    pub fn shift(&self) -> (Axis, u32, i64) {
        match self {
            Self::Insert { axis, at, count, .. } => (*axis, *at, *count as i64),
            Self::Delete { axis, at, count, .. } => (*axis, *at, -(*count as i64)),
            _ => (Axis::Rows, 0, 0),
        }
    }
}

/// A position moved by the change: `None` if it is deleted.
pub fn move_index(i: u32, at: u32, count: i64) -> Option<u32> {
    if count > 0 {
        Some(if i >= at { i + count as u32 } else { i })
    } else {
        let n = (-count) as u32;
        if i < at {
            Some(i)
        } else if i < at + n {
            None
        } else {
            Some(i - n)
        }
    }
}

/// A range moved, grown or shrunk by the change: `None` if it is all deleted.
pub fn move_range(r: CellRange, axis: Axis, at: u32, count: i64) -> Option<CellRange> {
    let (s, e) = match axis {
        Axis::Rows => (r.first_row, r.last_row),
        Axis::Columns => (r.first_column, r.last_column),
    };
    let (ns, ne) = if count > 0 {
        let n = count as u32;
        (if s >= at { s + n } else { s }, if e >= at { e + n } else { e })
    } else {
        let n = (-count) as u32;
        let end = at + n - 1;
        if s >= at && e <= end {
            return None;
        }
        (if s < at { s } else if s <= end { at } else { s - n }, if e < at { e } else if e <= end { at - 1 } else { e - n })
    };
    Some(match axis {
        Axis::Rows => CellRange { first_row: ns, last_row: ne, ..r },
        Axis::Columns => CellRange { first_column: ns, last_column: ne, ..r },
    })
}

/// The sheet's own changes: cells, merged ranges, sizes, print settings.
fn change_sheet(sheet: &mut Sheet, axis: Axis, at: u32, count: i64) {
    let mut cells = std::mem::take(&mut sheet.cells);
    cells.retain_mut(|c| {
        let i = if axis == Axis::Rows { c.row } else { c.column };
        match move_index(i, at, count) {
            Some(n) => {
                if axis == Axis::Rows {
                    c.row = n
                } else {
                    c.column = n
                }
                true
            }
            None => false,
        }
    });
    cells.sort_by_key(|c| (c.row, c.column));
    sheet.cells = cells;
    sheet.merges = sheet.merges.iter().filter_map(|m| move_range(*m, axis, at, count)).filter(|m| m.first_row != m.last_row || m.first_column != m.last_column).collect();
    sheet.print_area = sheet.print_area.and_then(|r| move_range(r, axis, at, count));
    // Sizes: new rows and columns at the default size; the grid keeps its length when rows go.
    let (sizes, default): (&mut Vec<Emu>, Emu) = match axis {
        Axis::Rows => (&mut sheet.rows, sheet.default_row),
        Axis::Columns => (&mut sheet.columns, sheet.default_column),
    };
    let default = if default > 0 { default } else { sizes.first().copied().unwrap_or(0) };
    let at_i = (at as usize).min(sizes.len());
    if count > 0 {
        for _ in 0..count {
            sizes.insert(at_i, default);
        }
    } else {
        let n = ((-count) as usize).min(sizes.len() - at_i);
        sizes.drain(at_i..at_i + n);
        sizes.extend(std::iter::repeat_n(default, n));
    }
    if axis == Axis::Rows {
        // The rows the app measured follow their rows (the ones deleted are gone).
        sheet.measured_rows.0 = sheet.measured_rows.iter().filter_map(|&r| move_index(r, at, count)).collect();
        sheet.fitted_rows.0 = std::mem::take(&mut sheet.fitted_rows.0).into_iter().filter_map(|(r, h)| Some((move_index(r, at, count)?, h))).collect();
    }
    for cf in sheet.conditional.iter_mut() {
        cf.ranges = cf.ranges.iter().filter_map(|r| move_range(*r, axis, at, count)).collect();
    }
    sheet.conditional.retain(|cf| !cf.ranges.is_empty());
    for v in sheet.validations.iter_mut() {
        v.ranges = v.ranges.iter().filter_map(|r| move_range(*r, axis, at, count)).collect();
    }
    sheet.validations.retain(|v| !v.ranges.is_empty());
    sheet.auto_filter = sheet.auto_filter.take().and_then(|mut f| {
        f.range = move_range(f.range, axis, at, count)?;
        if axis == Axis::Columns {
            f.columns = f.columns.into_iter().filter_map(|(c, x)| Some((move_index(c, at, count)?, x))).collect();
        }
        Some(f)
    });
    if sheet.auto_filter.is_none() {
        sheet.filtered_rows.clear();
    } else if axis == Axis::Rows {
        sheet.filtered_rows = std::mem::take(&mut sheet.filtered_rows).into_iter().filter_map(|row| move_index(row, at, count)).collect();
    }
    sheet.hyperlinks = std::mem::take(&mut sheet.hyperlinks).into_iter().filter_map(|mut h| {
        h.range = move_range(h.range, axis, at, count)?;
        Some(h)
    }).collect();
    sheet.comments = std::mem::take(&mut sheet.comments).into_iter().filter_map(|mut c| {
        match axis {
            Axis::Rows => c.row = move_index(c.row, at, count)?,
            Axis::Columns => c.column = move_index(c.column, at, count)?,
        }
        Some(c)
    }).collect();
    sheet.threads = std::mem::take(&mut sheet.threads).into_iter().filter_map(|mut c| {
        match axis {
            Axis::Rows => c.row = move_index(c.row, at, count)?,
            Axis::Columns => c.column = move_index(c.column, at, count)?,
        }
        Some(c)
    }).collect();
    sheet.cell_images = std::mem::take(&mut sheet.cell_images).into_iter().filter_map(|mut img| {
        match axis {
            Axis::Rows => img.row = move_index(img.row, at, count)?,
            Axis::Columns => img.column = move_index(img.column, at, count)?,
        }
        Some(img)
    }).collect();
    sheet.web_images = std::mem::take(&mut sheet.web_images).into_iter().filter_map(|((r, c), spec)| {
        let (r, c) = match axis {
            Axis::Rows => (move_index(r, at, count)?, c),
            Axis::Columns => (r, move_index(c, at, count)?),
        };
        Some(((r, c), spec))
    }).collect();
    sheet.spills = std::mem::take(&mut sheet.spills)
        .into_iter()
        .filter_map(|(a, r)| {
            let (row, column) = match axis {
                Axis::Rows => (move_index(a.0, at, count)?, a.1),
                Axis::Columns => (a.0, move_index(a.1, at, count)?),
            };
            let r = move_range(r, axis, at, count).unwrap_or(CellRange { first_row: row, first_column: column, last_row: row, last_column: column });
            Some(((row, column), r))
        })
        .collect();
    let outline = match axis {
        Axis::Rows => &mut sheet.outline_rows,
        Axis::Columns => &mut sheet.outline_columns,
    };
    *outline = std::mem::take(outline).into_iter().filter_map(|(i, l)| Some((move_index(i, at, count)?, l))).collect();
    if axis == Axis::Columns {
        sheet.custom_columns = std::mem::take(&mut sheet.custom_columns).into_iter().filter_map(|i| move_index(i, at, count)).collect();
    }
    let hidden = match axis {
        Axis::Rows => &mut sheet.hidden_rows,
        Axis::Columns => &mut sheet.hidden_columns,
    };
    *hidden = std::mem::take(hidden).into_iter().filter_map(|(i, size)| Some((move_index(i, at, count)?, size))).collect();
    let breaks = match axis {
        Axis::Rows => &mut sheet.print.row_breaks,
        Axis::Columns => &mut sheet.print.column_breaks,
    };
    *breaks = breaks.iter().filter_map(|b| move_index(*b, at, count)).collect();
    let mut new_table_headers = Vec::new();
    sheet.tables = std::mem::take(&mut sheet.tables)
        .into_iter()
        .filter_map(|mut t| {
            let new_range = move_range(t.range, axis, at, count)?;
            if axis == Axis::Columns {
                let first = t.range.first_column;
                let last = t.range.last_column;
                if count > 0 {
                    let n = count as u32;
                    if at > first && at <= last {
                        let rel_at = (at - first) as usize;
                        for i in 0..n {
                            let name = unused_table_column_name(&t.columns, (i + 1) as usize);
                            t.columns.insert(rel_at + i as usize, name.clone());
                            if !t.totals.is_empty() {
                                t.totals.insert(rel_at + i as usize, Default::default());
                            }
                            if t.header_rows > 0 {
                                let row = t.range.first_row;
                                let col = at + i;
                                let style = sheet.cells.iter()
                                    .find(|c| c.row == row && (c.column == first || c.column == last + n))
                                    .map(|c| c.style)
                                    .unwrap_or(0);
                                new_table_headers.push(SheetCell {
                                    row,
                                    column: col,
                                    value: CellValue::Text(name.clone()),
                                    formula: None,
                                    text: name,
                                    kind: CellKind::Text,
                                    style,
                                    runs: None,
                                    phonetic: None,
                                });
                            }
                        }
                    }
                } else {
                    let n = (-count) as u32;
                    let end = at + n - 1;
                    if at <= last && end >= first {
                        let del_start = at.max(first);
                        let del_end = end.min(last);
                        let rel_start = (del_start - first) as usize;
                        let del_len = (del_end - del_start + 1) as usize;
                        if rel_start < t.columns.len() {
                            let end_idx = (rel_start + del_len).min(t.columns.len());
                            t.columns.drain(rel_start..end_idx);
                        }
                        if !t.totals.is_empty() && rel_start < t.totals.len() {
                            let end_idx = (rel_start + del_len).min(t.totals.len());
                            t.totals.drain(rel_start..end_idx);
                        }
                        if t.columns.is_empty() {
                            return None;
                        }
                    }
                }
            }
            t.range = new_range;
            if let Some(f) = &mut t.filter {
                if let Some(fr) = move_range(f.range, axis, at, count) {
                    f.range = fr;
                    if axis == Axis::Columns {
                        f.columns = std::mem::take(&mut f.columns)
                            .into_iter()
                            .filter_map(|(c, x)| Some((move_index(c, at, count)?, x)))
                            .collect();
                    }
                } else {
                    t.filter = None;
                }
            }
            Some(t)
        })
        .collect();
    if !new_table_headers.is_empty() {
        sheet.cells.extend(new_table_headers);
        sheet.cells.sort_by_key(|c| (c.row, c.column));
    }
    let move_pos = |(r, c): (u32, u32)| -> Option<(u32, u32)> {
        match axis {
            Axis::Rows => Some((move_index(r, at, count)?, c)),
            Axis::Columns => Some((r, move_index(c, at, count)?)),
        }
    };
    sheet.data_tables = std::mem::take(&mut sheet.data_tables)
        .into_iter()
        .filter_map(|mut t| {
            t.range = move_range(t.range, axis, at, count)?;
            if let Some(p) = t.row_input {
                t.row_input = Some(move_pos(p)?);
            }
            if let Some(p) = t.column_input {
                t.column_input = Some(move_pos(p)?);
            }
            Some(t)
        })
        .collect();
}

/// Finds an unused table column name such as "列1", "列2", etc.
fn unused_table_column_name(existing: &[String], start: usize) -> String {
    let mut k = start;
    loop {
        let name = format!("列{k}");
        if !existing.iter().any(|c| c.eq_ignore_ascii_case(&name)) {
            return name;
        }
        k += 1;
    }
}

/// Apply a structural change to the model (without calculating anything
/// again). Returns the formula cells whose formula changed, as (sheet
/// index, row, column) after the change.
pub fn apply_structure(presentation: &mut Presentation, edit: &StructureEdit) -> Vec<(usize, u32, u32)> {
    match edit {
        StructureEdit::AddSheet { .. } | StructureEdit::DeleteSheet { .. } | StructureEdit::RenameSheet { .. } | StructureEdit::HideSheet { .. } | StructureEdit::MoveSheet { .. } => return apply_sheet_edit(presentation, edit),
        _ => {}
    }
    let Some(workbook) = presentation.workbook.clone() else { return Vec::new() };
    let target = edit.sheet();
    let Some(target_name) = workbook.sheets.get(target).map(|s| s.name.clone()) else { return Vec::new() };
    let (axis, at, count) = edit.shift();
    let mut changed = Vec::new();
    for (index, ws) in workbook.sheets.iter().enumerate() {
        let own = ws.name.clone();
        let mut apply = |sheet: &mut Sheet| {
            if index == target {
                change_sheet(sheet, axis, at, count);
            }
            rewrite_conditional(sheet, &|f| adjust_references(f, &own, &target_name, axis, at, count));
            for cell in sheet.cells.iter_mut() {
                if let Some(f) = &cell.formula {
                    let new = adjust_references(f, &own, &target_name, axis, at, count);
                    if new != *f {
                        cell.formula = Some(new);
                        changed.push((index, cell.row, cell.column));
                    }
                }
            }
        };
        match ws.slide {
            Some(id) => {
                let Some(slide) = presentation.slide_mut(id) else { continue };
                let (old_cell_images, old_xs, old_ys) = if index == target {
                    if let Some(sh) = slide.shapes.iter().find(|s| matches!(s.kind, ShapeKind::Sheet(_))) {
                        if let ShapeKind::Sheet(sheet) = &sh.kind {
                            (sheet.cell_images.clone(), sheet.column_offsets(), sheet.row_offsets())
                        } else {
                            (Vec::new(), Vec::new(), Vec::new())
                        }
                    } else {
                        (Vec::new(), Vec::new(), Vec::new())
                    }
                } else {
                    (Vec::new(), Vec::new(), Vec::new())
                };

                for shape in slide.shapes.iter_mut() {
                    if let ShapeKind::Sheet(sheet) = &mut shape.kind {
                        let s = Arc::make_mut(sheet);
                        apply(s);
                        if index == target {
                            // The grid's size.
                            shape.frame.width = s.columns.iter().sum();
                            shape.frame.height = s.rows.iter().sum();
                        }
                    }
                }

                if index == target && !old_cell_images.is_empty() {
                    let (new_xs, new_ys, new_cell_images) = if let Some(sh) = slide.shapes.iter().find(|s| matches!(s.kind, ShapeKind::Sheet(_))) {
                        if let ShapeKind::Sheet(sheet) = &sh.kind {
                            (sheet.column_offsets(), sheet.row_offsets(), sheet.cell_images.clone())
                        } else {
                            (Vec::new(), Vec::new(), Vec::new())
                        }
                    } else {
                        (Vec::new(), Vec::new(), Vec::new())
                    };

                    let mut shapes_to_remove = std::collections::HashSet::new();
                    for old_img in &old_cell_images {
                        let old_c = old_img.column as usize;
                        let old_r = old_img.row as usize;
                        if old_c + 1 >= old_xs.len() || old_r + 1 >= old_ys.len() {
                            continue;
                        }
                        let (ox0, oy0) = (old_xs[old_c], old_ys[old_r]);
                        let (ox1, oy1) = (old_xs[old_c + 1], old_ys[old_r + 1]);

                        let new_img = new_cell_images.iter().find(|img| {
                            match axis {
                                Axis::Rows => move_index(old_img.row, at, count) == Some(img.row) && img.column == old_img.column,
                                Axis::Columns => move_index(old_img.column, at, count) == Some(img.column) && img.row == old_img.row,
                            }
                        });

                        for shape in slide.shapes.iter_mut() {
                            if matches!(shape.kind, ShapeKind::Picture(_)) {
                                let cx = shape.frame.x + shape.frame.width / 2;
                                let cy = shape.frame.y + shape.frame.height / 2;
                                if (ox0..=ox1).contains(&cx) && (oy0..=oy1).contains(&cy) {
                                    if let Some(target_img) = new_img {
                                        let tc = target_img.column as usize;
                                        let tr = target_img.row as usize;
                                        if tc + 1 < new_xs.len() && tr + 1 < new_ys.len() {
                                            let (nx0, ny0) = (new_xs[tc], new_ys[tr]);
                                            let (nx1, ny1) = (new_xs[tc + 1], new_ys[tr + 1]);
                                            let (cw, ch) = ((nx1 - nx0).max(1), (ny1 - ny0).max(1));
                                            let (w, h) = (shape.frame.width.max(1), shape.frame.height.max(1));
                                            let scale = (cw as f64 / w as f64).min(ch as f64 / h as f64);
                                            let (fw, fh) = ((w as f64 * scale) as i64, (h as f64 * scale) as i64);
                                            shape.frame = EmuRect::new(nx0 + (cw - fw) / 2, ny0 + (ch - fh) / 2, fw.max(1), fh.max(1));
                                        }
                                    } else {
                                        shapes_to_remove.insert(shape.id);
                                    }
                                    break;
                                }
                            }
                        }
                    }
                    if !shapes_to_remove.is_empty() {
                        slide.shapes.retain(|s| !shapes_to_remove.contains(&s.id));
                    }
                }
            }
            None => {
                if let Some(hidden) = presentation.workbook.as_mut().and_then(|w| Arc::make_mut(w).sheets[index].hidden.as_mut()) {
                    apply(Arc::make_mut(hidden));
                }
            }
        }
    }
    // Defined names and chart references refer to sheets by name.
    if let Some(wb) = presentation.workbook.as_mut() {
        let wb = Arc::make_mut(wb);
        for name in wb.names.iter_mut() {
            let own = name.sheet.and_then(|i| workbook.sheets.get(i)).map_or(target_name.as_str(), |s| s.name.as_str()).to_string();
            name.formula = adjust_references(&name.formula, &own, &target_name, axis, at, count);
        }
    }
    crate::chart_refresh::rewrite_chart_references(presentation, &|f| adjust_references(f, "", &target_name, axis, at, count));
    changed
}

/// Every formula of the workbook (and every defined name) passed through
/// `f` (given the sheet's index; a defined name's own sheet, or `usize::MAX`
/// for a name of the whole book); the cells whose formula changed.
pub(crate) fn rewrite_formulas(presentation: &mut Presentation, f: &dyn Fn(usize, &str) -> String) -> Vec<(usize, u32, u32)> {
    let Some(workbook) = presentation.workbook.clone() else { return Vec::new() };
    let mut changed = Vec::new();
    for (index, ws) in workbook.sheets.iter().enumerate() {
        let mut apply = |sheet: &mut Sheet| {
            rewrite_conditional(sheet, &|text| f(index, text));
            for cell in sheet.cells.iter_mut() {
                if let Some(text) = &cell.formula {
                    let new = f(index, text);
                    if new != *text {
                        cell.formula = Some(new);
                        changed.push((index, cell.row, cell.column));
                    }
                }
            }
        };
        match ws.slide {
            Some(id) => {
                if let Some(slide) = presentation.slide_mut(id) {
                    for shape in slide.shapes.iter_mut() {
                        if let ShapeKind::Sheet(sheet) = &mut shape.kind {
                            apply(Arc::make_mut(sheet));
                        }
                    }
                }
            }
            None => {
                if let Some(h) = presentation.workbook.as_mut().and_then(|w| Arc::make_mut(w).sheets[index].hidden.as_mut()) {
                    apply(Arc::make_mut(h));
                }
            }
        }
    }
    if let Some(wb) = presentation.workbook.as_mut() {
        for name in Arc::make_mut(wb).names.iter_mut() {
            name.formula = f(name.sheet.unwrap_or(usize::MAX), &name.formula);
        }
    }
    crate::chart_refresh::rewrite_chart_references(presentation, &|r| f(usize::MAX, r));
    changed
}

fn apply_sheet_edit(presentation: &mut Presentation, edit: &StructureEdit) -> Vec<(usize, u32, u32)> {
    let Some(workbook) = presentation.workbook.clone() else { return Vec::new() };
    match edit {
        StructureEdit::AddSheet { sheet, name, slide, shape, content } => {
            // The slide goes after the slides of the sheets before it.
            let at = workbook.sheets[..(*sheet).min(workbook.sheets.len())].iter().filter_map(|s| s.slide).filter_map(|id| presentation.slide_index(id)).max().map_or(0, |i| i + 1);
            let mut new = Slide::new(*slide);
            new.name = Some(name.clone());
            let frame = EmuRect::new(0, 0, content.columns.iter().sum(), content.rows.iter().sum());
            new.size = Some(crate::geometry::EmuSize::new(frame.width.max(1), frame.height.max(1)));
            new.shapes.push(Shape { id: *shape, frame, rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Sheet(content.clone()), floating: None });
            presentation.slides.insert(at.min(presentation.slides.len()), new);
            let wb = Arc::make_mut(presentation.workbook.as_mut().unwrap());
            wb.sheets.insert(*sheet, WorkbookSheet { name: name.clone(), state: Default::default(), slide: Some(*slide), hidden: None, other: false });
            for n in wb.names.iter_mut() {
                if let Some(i) = n.sheet.as_mut()
                    && *i >= *sheet
                {
                    *i += 1;
                }
            }
            Vec::new()
        }
        StructureEdit::DeleteSheet { sheet } => {
            let Some(ws) = workbook.sheets.get(*sheet) else { return Vec::new() };
            let name = ws.name.clone();
            if let Some(id) = ws.slide
                && let Some(i) = presentation.slide_index(id)
            {
                presentation.slides.remove(i);
            }
            {
                let wb = Arc::make_mut(presentation.workbook.as_mut().unwrap());
                wb.sheets.remove(*sheet);
                wb.names.retain(|n| n.sheet != Some(*sheet));
                for n in wb.names.iter_mut() {
                    if let Some(i) = n.sheet.as_mut()
                        && *i > *sheet
                    {
                        *i -= 1;
                    }
                }
            }
            rewrite_formulas(presentation, &|_, f| ibis_calc::formula::remove_sheet(f, &name))
        }
        StructureEdit::MoveSheet { sheet, to } => {
            let (from, to) = (*sheet, *to);
            let count = workbook.sheets.len();
            if from >= count || to >= count || from == to {
                return Vec::new();
            }
            let wb = Arc::make_mut(presentation.workbook.as_mut().unwrap());
            let moved = wb.sheets.remove(from);
            wb.sheets.insert(to, moved);
            // Sheet indexes of names: the permutation.
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
            for n in wb.names.iter_mut() {
                if let Some(i) = n.sheet.as_mut() {
                    *i = new_index(*i);
                }
            }
            // The slides follow the workbook's order.
            let order: Vec<SlideId> = wb.sheets.iter().filter_map(|s| s.slide).collect();
            presentation.slides.sort_by_key(|s| order.iter().position(|id| *id == s.id).unwrap_or(usize::MAX));
            Vec::new()
        }
        StructureEdit::HideSheet { sheet, hidden, slide, shape } => {
            let Some(ws) = workbook.sheets.get(*sheet) else { return Vec::new() };
            if *hidden {
                let Some(id) = ws.slide else { return Vec::new() };
                let Some(i) = presentation.slide_index(id) else { return Vec::new() };
                let removed = presentation.slides.remove(i);
                let content = removed.shapes.iter().find_map(|s| match &s.kind {
                    ShapeKind::Sheet(sheet) => Some(sheet.clone()),
                    _ => None,
                });
                let wb = Arc::make_mut(presentation.workbook.as_mut().unwrap());
                let entry = &mut wb.sheets[*sheet];
                entry.slide = None;
                entry.hidden = content;
                entry.state = crate::sheet::SheetState::Hidden;
            } else {
                let Some(content) = ws.hidden.clone() else { return Vec::new() };
                let at = workbook.sheets[..*sheet].iter().filter_map(|s| s.slide).filter_map(|id| presentation.slide_index(id)).max().map_or(0, |i| i + 1);
                let mut new = Slide::new(*slide);
                new.name = Some(ws.name.clone());
                let frame = EmuRect::new(0, 0, content.columns.iter().sum(), content.rows.iter().sum());
                new.size = Some(crate::geometry::EmuSize::new(frame.width.max(1), frame.height.max(1)));
                new.shapes.push(Shape { id: *shape, frame, rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Sheet(content), floating: None });
                presentation.slides.insert(at.min(presentation.slides.len()), new);
                let wb = Arc::make_mut(presentation.workbook.as_mut().unwrap());
                let entry = &mut wb.sheets[*sheet];
                entry.slide = Some(*slide);
                entry.hidden = None;
                entry.state = crate::sheet::SheetState::Visible;
            }
            Vec::new()
        }
        StructureEdit::RenameSheet { sheet, name } => {
            let Some(ws) = workbook.sheets.get(*sheet) else { return Vec::new() };
            let old = ws.name.clone();
            if let Some(id) = ws.slide
                && let Some(s) = presentation.slide_mut(id)
            {
                s.name = Some(name.clone());
            }
            Arc::make_mut(presentation.workbook.as_mut().unwrap()).sheets[*sheet].name = name.clone();
            rewrite_formulas(presentation, &|_, f| ibis_calc::formula::rename_sheet(f, &old, name))
        }
        _ => Vec::new(),
    }
}

/// Whether `name` can name a new sheet of the workbook (Excel's rules:
/// 1–31 characters, none of `[]:*?/\`, not starting or ending with `'`,
/// not "History", unique without regard to case).
pub fn valid_sheet_name(presentation: &Presentation, name: &str, except: Option<usize>) -> bool {
    let n = name.chars().count();
    if n == 0 || n > 31 || name.contains(['[', ']', ':', '*', '?', '/', '\\']) || name.starts_with('\'') || name.ends_with('\'') || name.eq_ignore_ascii_case("History") {
        return false;
    }
    presentation.workbook.as_ref().is_none_or(|w| !w.sheets.iter().enumerate().any(|(i, s)| Some(i) != except && s.name.to_lowercase() == name.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet::Unsaved;

    #[test]
    fn measured_rows_follow_inserted_and_deleted_rows() {
        let mut sheet = Sheet { measured_rows: Unsaved(vec![1, 4, 6]), fitted_rows: Unsaved([(1, 10), (4, 40), (6, 60)].into()), rows: vec![5; 10], ..Sheet::default() };
        change_sheet(&mut sheet, Axis::Rows, 3, 2);
        assert_eq!(sheet.measured_rows.0, vec![1, 6, 8]);
        assert_eq!(sheet.fitted_rows.0, [(1, 10), (6, 40), (8, 60)].into());
        change_sheet(&mut sheet, Axis::Rows, 0, -2);
        assert_eq!(sheet.measured_rows.0, vec![4, 6]);
        assert_eq!(sheet.fitted_rows.0, [(4, 40), (6, 60)].into());
    }

    #[test]
    fn moving() {
        assert_eq!(move_index(5, 3, 2), Some(7));
        assert_eq!(move_index(2, 3, 2), Some(2));
        assert_eq!(move_index(3, 3, -2), None);
        assert_eq!(move_index(6, 3, -2), Some(4));
        let r = CellRange { first_row: 1, last_row: 5, first_column: 0, last_column: 0 };
        assert_eq!(move_range(r, Axis::Rows, 2, 3).unwrap().last_row, 8);
        assert_eq!(move_range(r, Axis::Rows, 0, -2).unwrap(), CellRange { first_row: 0, last_row: 3, first_column: 0, last_column: 0 });
        assert!(move_range(r, Axis::Rows, 1, -5).is_none());
    }
}

/// Conditional formulas follow the same structural reference changes as cells.
fn rewrite_conditional(sheet: &mut Sheet, f: &dyn Fn(&str) -> String) {
    use crate::sheet::{CfKind, CfValue};
    let point = |v: &mut CfValue| { if v.kind == "formula" { if let Some(text) = &mut v.value { *text = f(text); } } };
    for cf in &mut sheet.conditional {
        for r in &mut cf.rules {
            match &mut r.kind {
                CfKind::Expression(text) => *text = f(text),
                CfKind::CellIs { formulas, .. } => { for text in formulas { *text = f(text); } },
                CfKind::ColorScale(points) => { for (v, _) in points { point(v); } },
                CfKind::DataBar { min, max, .. } => { point(min); point(max); },
                CfKind::IconSet { values, .. } => { for v in values { point(v); } },
                _ => {},
            }
        }
    }
}
