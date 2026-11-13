// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scrolling the sheet while a drag holds the pointer at or past the edge of the cells (GitHub issue #186), as Excel does:
//! selecting cells, selecting whole columns or rows along the headers, dragging the fill handle and moving a selection
//! by its border. The view scrolls on a timer for as long as the button is held there, even if the pointer does not move,
//! and the farther past the edge the pointer is the more cells it passes at each beat. Where the pointer is inside the
//! cells on an axis, that axis is not scrolled.

use std::time::Duration;

use gpui_kit::*;
use ibis_doc_model::{CellRange, sheet_command::Axis};

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The time between two beats of the scrolling.
const BEAT: Duration = Duration::from_millis(50);
/// How far past the edge (in pixels) the pointer goes for each further cell per beat.
const PIXELS_PER_STEP: f32 = 24.0;
/// The most cells a beat passes.
const MAX_STEP: i32 = 40;

/// The pointer held past the edge of the cells during a drag.
pub(crate) struct DragScroll {
    /// Where the pointer was last (window pixels).
    pub(crate) pointer: Point<Pixels>,
    /// Option was held (a move copies).
    pub(crate) alt: bool,
    /// The timer; dropping it stops the scrolling.
    pub(crate) _task: Task<()>,
}

/// How many cells a beat passes for a pointer `past` pixels beyond the edge (`past` is 0 on the edge): 1 on the edge,
/// one more for each `PIXELS_PER_STEP`. Negative `past` (inside the cells) is no step.
pub(crate) fn step_for(past: f32) -> i32 {
    if past < 0.0 { 0 } else { (1 + (past / PIXELS_PER_STEP) as i32).min(MAX_STEP) }
}

/// The scrolling of one axis: -/+ steps for a pointer at `at` between the cells' `low` and `high` edges.
/// `low_scrolls` is false when the cells before `low` are frozen (they do not scroll).
fn axis_step(at: f32, low: f32, high: f32, low_scrolls: bool) -> i32 {
    if at >= high {
        step_for(at - high)
    } else if low_scrolls && at <= low {
        -step_for(low - at)
    } else {
        0
    }
}

#[ibis_office_macros::editor_ext(DragAutoscroll)]
impl SlideEditor {
    /// The part of the window the cells are shown in (left, top, right, bottom), past the headers, and the left and top
    /// edges' ends of the frozen panes; `None` away from a sheet in the normal view.
    fn drag_viewport(&self) -> Option<(f32, f32, f32, f32)> {
        if self.page_layout_shown() {
            return None;
        }
        let ws = self.workspace_bounds?;
        let (_, sheet) = self.current_sheet()?;
        let content = self.workspace_content()?;
        let (split_x, split_y) = self.frozen_split(content).unwrap_or((None, None));
        let head_w = if sheet.headings { crate::shell::sheet_metrics::row_header_width(sheet) } else { 0.0 };
        let head_h = if sheet.headings { crate::shell::sheet_metrics::COLUMN_HEADER_HEIGHT } else { 0.0 };
        Some((
            f32::from(ws.origin.x) + split_x.unwrap_or(head_w),
            f32::from(ws.origin.y) + split_y.unwrap_or(head_h),
            f32::from(ws.origin.x + ws.size.width) - super::sheet_ui::SCROLLBAR_ROOM,
            f32::from(ws.origin.y + ws.size.height) - super::sheet_ui::SCROLLBAR_ROOM,
        ))
    }

    /// Whether the cells before the frozen panes scroll for a drag that started in `anchor` (a drag from inside the frozen
    /// cells scrolls the other way only once it has left them).
    fn drag_low_scrolls(&self) -> (bool, bool) {
        let frozen = self.current_sheet().and_then(|(_, s)| s.frozen).or_else(|| self.split_panes().map(|p| p.at)).unwrap_or((0, 0));
        let (column, row) = self.current_cell().unwrap_or((0, 0));
        (column >= frozen.0, row >= frozen.1)
    }

    /// The cells to pass at each beat on each axis (columns, rows; negative: left, up) for the pointer at `pointer`.
    pub(crate) fn drag_scroll_steps(&self, pointer: Point<Pixels>) -> (i32, i32) {
        let Some((left, top, right, bottom)) = self.drag_viewport() else { return (0, 0) };
        let (low_x, low_y) = self.drag_low_scrolls();
        (
            axis_step(f32::from(pointer.x), left, right, low_x),
            axis_step(f32::from(pointer.y), top, bottom, low_y),
        )
    }

    /// The pointer clamped inside the cells (a pixel in from each edge): the drag selects the cell at the edge while the
    /// scrolling goes on.
    pub(crate) fn drag_clamped(&self, pointer: Point<Pixels>) -> Point<Pixels> {
        let Some((left, top, right, bottom)) = self.drag_viewport() else { return pointer };
        let (low_x, low_y) = self.drag_low_scrolls();
        let x = f32::from(pointer.x).min(right - 1.0);
        let y = f32::from(pointer.y).min(bottom - 1.0);
        point(px(if low_x { x.max(left + 1.0) } else { x }), px(if low_y { y.max(top + 1.0) } else { y }))
    }

    fn drag_unit_px(&self) -> (f32, f32) {
        let scale = self.workspace_content().map_or(1.0, |c| c.scale);
        self.current_sheet().map_or((64.0, 20.0), |(_, s)| (s.default_column as f32 * scale, s.default_row as f32 * scale))
    }

    /// Which drag, if any, the scrolling serves: cells (or the fill handle) being selected, a selection being moved,
    /// or columns or rows being selected along a header.
    fn drag_active(&self) -> bool {
        self.sheet_state().is_some_and(|s| s.cell_drag || s.cell_move.is_some() || s.header_select.is_some())
    }

    /// The pointer moved outside the window during a drag (the button is held, so the window keeps getting the moves, but
    /// no element is under the pointer and the window's own move listener does not run): the drag takes the move as one
    /// inside the window, so the view goes on scrolling past any edge, right and left as well as below and above.
    pub(crate) fn drag_moved_outside_window(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if event.pressed_button != Some(MouseButton::Left) || !self.drag_active() {
            return false;
        }
        let (size, at) = (window.viewport_size(), event.position);
        if at.x >= px(0.) && at.y >= px(0.) && at.x < size.width && at.y < size.height {
            return false;
        }
        self.xlsx_pointer_move(event, window, cx)
    }

    /// The pointer moved during a drag: where it is past the edge of the cells, the scrolling is started (or kept, with the
    /// new place) and the drag goes only as far as the edge; returns that. In the cells it stops the scrolling.
    pub(crate) fn drag_pointer_moved(&mut self, pointer: Point<Pixels>, alt: bool, cx: &mut Context<Self>) -> bool {
        let steps = self.drag_scroll_steps(pointer);
        let header_axis = self.sheet_state().and_then(|s| s.header_select);
        let steps = match header_axis {
            Some(Axis::Columns) => (steps.0, 0),
            Some(Axis::Rows) => (0, steps.1),
            None => steps,
        };
        if steps == (0, 0) || !self.drag_active() {
            if let Some(state) = self.sheet_state_mut() {
                state.drag_scroll = None;
            }
            return false;
        }
        let keep = self.sheet_state_mut().and_then(|s| s.drag_scroll.as_mut()).map(|d| {
            d.pointer = pointer;
            d.alt = alt;
        });
        if keep.is_none() {
            let task = cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(BEAT).await;
                    let more = this.update(cx, |this, cx| this.drag_autoscroll_beat(cx)).unwrap_or(false);
                    if !more {
                        break;
                    }
                }
            });
            if let Some(state) = self.sheet_state_mut() {
                state.drag_scroll = Some(DragScroll { pointer, alt, _task: task });
            }
        }
        true
    }

    /// One beat: scroll and carry the drag to the cells that come into view. Returns false when the scrolling is over.
    pub(crate) fn drag_autoscroll_beat(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((pointer, alt)) = self.sheet_state().and_then(|s| s.drag_scroll.as_ref()).map(|d| (d.pointer, d.alt)) else { return false };
        if !self.drag_active() {
            if let Some(state) = self.sheet_state_mut() {
                state.drag_scroll = None;
            }
            return false;
        }
        let (dx, dy) = self.drag_scroll_steps(pointer);
        if let Some(axis) = self.sheet_state().and_then(|s| s.header_select) {
            self.header_autoscroll(axis, if axis == Axis::Columns { dx } else { dy }, cx);
        } else {
            let clamped = self.drag_clamped(pointer);
            if self.sheet_state().is_some_and(|s| s.cell_move.is_some() || s.fill_source.is_some()) {
                // A selection being moved, or the fill handle: the target is found from the pointer, put past the edge by the step.
                let (unit_w, unit_h) = self.drag_unit_px();
                let beyond = point(clamped.x + px(dx as f32 * unit_w), clamped.y + px(dy as f32 * unit_h));
                if self.sheet_state().is_some_and(|s| s.cell_move.is_some()) {
                    self.update_cell_move(beyond, alt, cx);
                    if let Some(target) = self.sheet_state().and_then(|s| s.cell_move.map(|m| m.target)) {
                        self.scroll_given_range_into_view(target, false);
                    }
                } else {
                    self.extend_cells_to(beyond);
                    self.scroll_selection_end_into_view();
                }
            } else {
                // Cells being selected: the cell at the edge (or under the pointer on an axis it is inside), and the far end
                // of the selection `step` cells on past it where the pointer is past the edge.
                self.extend_cells_to(clamped);
                self.step_selection_end(dx, dy);
                self.scroll_selection_end_into_view();
            }
        }
        cx.notify();
        true
    }

    /// The selection's far corner goes `dx` columns and `dy` rows on (a drag of cells held past the edge).
    fn step_selection_end(&mut self, dx: i32, dy: i32) {
        if (dx, dy) == (0, 0) || self.sheet_state().is_some_and(|s| s.deselect_drag.is_some()) {
            return;
        }
        let Some((anchor, slide)) = self.current_cell().map(|c| (c, self.current_slide)) else { return };
        let end = self.sheet_state().and_then(|s| s.cell_ends.get(&slide).copied()).unwrap_or(anchor);
        let Some((_, sheet)) = self.current_sheet() else { return };
        // To Excel's last column and row, not only the cells the sheet holds (the view grows its grid as the drag goes on).
        let (columns, rows) = (super::sheet_ui::MAX_SHEET_COLUMNS as i64, super::sheet_ui::MAX_SHEET_ROWS as i64);
        let to = ((end.0 as i64 + dx as i64).clamp(0, columns - 1) as u32, (end.1 as i64 + dy as i64).clamp(0, rows - 1) as u32);
        let was_kept = self.selected_ranges().len() > 1;
        if to == anchor {
            self.wb_mut().cell_ends.remove(&slide);
        } else {
            self.wb_mut().cell_ends.insert(slide, to);
        }
        self.extra_ranges_follow(was_kept);
    }

    /// A beat of a drag along the headers of `axis`: the selection's far line goes `step` on.
    fn header_autoscroll(&mut self, axis: Axis, step: i32, cx: &mut Context<Self>) {
        if step == 0 {
            return;
        }
        let Some((column, row)) = self.current_cell() else { return };
        let (anchor, limit) = match axis {
            Axis::Columns => (column, super::sheet_ui::MAX_SHEET_COLUMNS),
            Axis::Rows => (row, super::sheet_ui::MAX_SHEET_ROWS),
        };
        // From the line at the edge the pointer is past (the last one shown, or the first of those that scroll), `step` lines on.
        let geometry = self.sheet_state().map(|s| s.header_geometry.borrow().clone()).unwrap_or_default();
        let Some((left, top, right, bottom)) = self.drag_viewport() else { return };
        let (spans, low, high) = match axis {
            Axis::Columns => (&geometry.columns, left, right),
            Axis::Rows => (&geometry.rows, top, bottom),
        };
        let edge = if step > 0 { spans.iter().rev().find(|s| s.1 < high) } else { spans.iter().find(|s| s.2 > low) };
        let Some(&(base, _, _)) = edge else { return };
        let to = (base as i64 + step as i64).clamp(0, limit.saturating_sub(1) as i64) as u32;
        let was_kept = self.selected_ranges().len() > 1;
        self.select_lines(axis, anchor, to, cx);
        self.extra_ranges_follow(was_kept);
        // The line comes into view; the other axis stays where it is (a cell of the first line shown on it).
        let range = match axis {
            Axis::Columns => CellRange { first_column: to, last_column: to, first_row: geometry.rows.first().map_or(0, |r| r.0), last_row: geometry.rows.first().map_or(0, |r| r.0) },
            Axis::Rows => CellRange { first_row: to, last_row: to, first_column: geometry.columns.first().map_or(0, |c| c.0), last_column: geometry.columns.first().map_or(0, |c| c.0) },
        };
        self.scroll_given_range_into_view(range, false);
    }
}
