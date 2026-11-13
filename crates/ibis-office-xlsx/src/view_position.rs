// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Where each worksheet was left, kept in the file as Excel keeps it (#145): the `sheetView`'s `topLeftCell` and `selection`.
//! Opening a file puts each sheet's scroll and selected range where the file says (`restore_sheet_views`); saving hands the present
//! scroll (`State::sheet_scrolls`, the current sheet's own) and selection back to the model the file is written from
//! (`workbook_for_save`, the workbook's `FormatEditor::model_for_save`). The scroll is in pixels, the file's position is a cell: the cell at the top-left edge of the grid.
//! Frozen panes: the scroll of the part that scrolls is `pane/@topLeftCell` (the top-left pane's `sheetView/@topLeftCell` is only on
//! a side nothing is frozen on). A sheet with split panes scrolls its panes apart: its scroll is left as the file has it.

use std::borrow::Cow;

use gpui_kit::*;
use ibis_doc_model::{CellRange, Presentation, Sheet, SheetViewPosition, SlideId};

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The scale in pixels per EMU, and the sheet's frame offset.
struct Geometry {
    scale: f32,
    frame: (i64, i64),
}

#[ibis_office_macros::editor_ext(ViewPosition)]
impl SlideEditor {
    fn view_geometry(&self, sheet: &Sheet, frame: (i64, i64)) -> Geometry {
        let zoom = sheet.zoom.map_or(1.0, |z| z as f32 / 100.0);
        Geometry { scale: zoom * self.px_per_inch_at_100() / ibis_doc_model::EMU_PER_INCH as f32, frame }
    }

    /// The sheets' positions as the file gives them are put in force: the selected cell and range of each, and the scroll of each
    /// (the current sheet's is the workspace's now). Run when a file is opened.
    pub(crate) fn restore_sheet_views(&mut self) {
        if !self.is_workbook() {
            return;
        }
        let current = self.current_slide;
        let mut scrolls = Vec::new();
        let mut cells = Vec::new();
        for slide in &self.presentation().slides {
            let Some((shape, sheet)) = crate::shell::editor::sheet_of(slide) else { continue };
            let view = &sheet.view;
            if let Some(active) = view.active_cell {
                let end = view.selection.and_then(|r| {
                    let corner = if (r.first_column, r.first_row) == active { (r.last_column, r.last_row) } else { (r.first_column, r.first_row) };
                    (corner != active).then_some(corner)
                });
                cells.push((slide.id, active, end));
            }
            // Split panes scroll each on their own: not mapped. Frozen panes: the scrolling pane's first cell is `pane/@topLeftCell`.
            if sheet.split.is_some() {
                continue;
            }
            let (frozen_columns, frozen_rows) = sheet.frozen.unwrap_or((0, 0));
            let target = if sheet.frozen.is_some() { view.pane_top_left } else { view.top_left };
            if let Some((column, row)) = target {
                let g = self.view_geometry(sheet, (shape.frame.x, shape.frame.y));
                let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
                let (default_column, default_row) = defaults(sheet);
                let x = (offset_at(&xs, default_column, column) - axis_line(frozen_columns, &xs, default_column, g.frame.0)).max(0) as f32;
                let y = (offset_at(&ys, default_row, row) - axis_line(frozen_rows, &ys, default_row, g.frame.1)).max(0) as f32;
                scrolls.push((slide.id, point(px(-x * g.scale), px(-y * g.scale))));
            }
        }
        let Some(state) = self.sheet_state_mut() else { return };
        for (slide, active, end) in cells {
            state.cells.insert(slide, active);
            match end {
                Some(end) => state.cell_ends.insert(slide, end),
                None => state.cell_ends.remove(&slide),
            };
        }
        let mut now = None;
        for (slide, offset) in scrolls {
            if slide == current {
                now = Some(offset);
            } else {
                state.sheet_scrolls.insert(slide, offset);
            }
        }
        if let Some(offset) = now {
            if let Some(state) = self.sheet_state_mut() {
                state.pending_view_scroll = Some(offset);
            }
            self.workspace_scroll.set_offset(offset);
        }
    }

    /// Once the workspace is laid out, the scroll the file left the sheet at is put in force (the content is as big as the scroll
    /// needs only then). Called as the formula bar is drawn, before the grid.
    pub(crate) fn apply_pending_view_scroll(&mut self) {
        if self.workspace_bounds.is_none() {
            return;
        }
        let Some(offset) = self.sheet_state_mut().and_then(|s| s.pending_view_scroll.take()) else { return };
        self.workspace_scroll.set_offset(offset);
    }

    /// The position each sheet is in now: its selection from the sheet state (when the sheet was ever selected in) and its scroll
    /// (the workspace's for the current sheet, the remembered one for the others), over what the model already has.
    fn current_sheet_views(&self) -> Vec<(SlideId, SheetViewPosition)> {
        let Some(state) = self.sheet_state() else { return Vec::new() };
        let mut out = Vec::new();
        for slide in &self.presentation().slides {
            let Some((shape, sheet)) = crate::shell::editor::sheet_of(slide) else { continue };
            let mut view = sheet.view.clone();
            if let Some(&(column, row)) = state.cells.get(&slide.id) {
                view.active_cell = Some((column, row));
                view.selection = Some(match state.cell_ends.get(&slide.id) {
                    Some(&(c2, r2)) => CellRange { first_row: row.min(r2), last_row: row.max(r2), first_column: column.min(c2), last_column: column.max(c2) },
                    None => CellRange { first_row: row, last_row: row, first_column: column, last_column: column },
                });
            }
            if sheet.split.is_none() {
                let scroll = if slide.id == self.current_slide { Some(self.workspace_scroll.offset()) } else { state.sheet_scrolls.get(&slide.id).copied() };
                if let Some(offset) = scroll {
                    let g = self.view_geometry(sheet, (shape.frame.x, shape.frame.y));
                    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
                    let (default_column, default_row) = defaults(sheet);
                    let (frozen_columns, frozen_rows) = sheet.frozen.unwrap_or((0, 0));
                    let line_x = axis_line(frozen_columns, &xs, default_column, g.frame.0);
                    let line_y = axis_line(frozen_rows, &ys, default_row, g.frame.1);
                    let column = index_at(&xs, default_column, line_x + (-f32::from(offset.x) / g.scale) as i64, g.scale);
                    let row = index_at(&ys, default_row, line_y + (-f32::from(offset.y) / g.scale) as i64, g.scale);
                    if sheet.frozen.is_some() {
                        // The scrolling pane's first cell, and the top-left pane's (it scrolls only on a side nothing is frozen on).
                        view.pane_top_left = Some((column.max(frozen_columns), row.max(frozen_rows)));
                        view.top_left = Some((if frozen_columns > 0 { 0 } else { column }, if frozen_rows > 0 { 0 } else { row })).filter(|c| *c != (0, 0));
                    } else {
                        view.top_left = ((column, row) != (0, 0)).then_some((column, row));
                    }
                }
            }
            out.push((slide.id, view));
        }
        out
    }

    /// The workbook as the file is written from it: the sheets' positions put in (a copy, only when one differs from the model).
    pub(crate) fn workbook_for_save(&self) -> Cow<'_, Presentation> {
        let views = self.current_sheet_views();
        let differs = views.iter().any(|(slide, view)| self.presentation().slide(*slide).and_then(crate::shell::editor::sheet_of).is_some_and(|(_, s)| s.view != *view));
        if !differs {
            return Cow::Borrowed(self.presentation());
        }
        let mut copy = self.presentation().clone();
        for (slide, view) in views {
            if let Ok(sheet) = ibis_doc_model::sheet_command::sheet_mut(&mut copy, slide) {
                sheet.view = view;
            }
        }
        Cow::Owned(copy)
    }
}

/// Where the scrolling part of the grid starts on an axis, in EMU from the sheet's frame: after the frozen columns or rows, else at
/// the frame's start (the headers' edge).
fn axis_line(frozen: u32, offsets: &[i64], default: i64, frame: i64) -> i64 {
    if frozen > 0 { offset_at(offsets, default, frozen) } else { -frame }
}

/// The sizes of a column and a row the sheet has no entry for, as the grid is laid out with (`workspace_content`).
fn defaults(sheet: &Sheet) -> (i64, i64) {
    (
        if sheet.default_column > 0 { sheet.default_column } else { 54 * ibis_doc_model::EMU_PER_POINT },
        if sheet.default_row > 0 { sheet.default_row } else { 20 * ibis_doc_model::EMU_PER_POINT },
    )
}

/// The offset (EMU) of the start of index `i` (a column or a row), past the end by the default size.
fn offset_at(offsets: &[i64], default: i64, i: u32) -> i64 {
    let i = i as usize;
    match offsets.get(i) {
        Some(v) => *v,
        None => offsets.last().copied().unwrap_or(0) + (i + 1 - offsets.len().max(1)) as i64 * default,
    }
}

/// The index whose span holds `emu` (the edge of the view; a pixel's half of tolerance for the rounding of the scroll).
fn index_at(offsets: &[i64], default: i64, emu: i64, scale: f32) -> u32 {
    let emu = emu + (0.5 / scale.max(1e-9)) as i64;
    if emu <= 0 {
        return 0;
    }
    let last = offsets.last().copied().unwrap_or(0);
    if emu < last {
        return (offsets.partition_point(|o| *o <= emu).saturating_sub(1)) as u32;
    }
    (offsets.len().saturating_sub(1) as i64 + (emu - last) / default.max(1)) as u32
}
