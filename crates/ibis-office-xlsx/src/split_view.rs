// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示 > 分割 (ウィンドウの分割): the window split at the selected cell
//! into panes that scroll on their own, as Excel's: the top pane up and
//! down by itself (sideways with the bottom one), the left pane sideways
//! by itself (up and down with the right one). Drawn as the frozen panes
//! are (`render_frozen_panes`), each pane with its own scrolling.
//! Saved in OOXML as sheetView/pane state="split", with Undo/Redo support.
use crate::shell::editor::SlideEditor;
use gpui_kit::*;
use ibis_doc_model::{Command, SheetCommand};

// Shared with the window: shell/split_axis.rs.
pub(crate) use crate::shell::split_axis::SplitAxis;
use crate::ext::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitPanes {
    /// Where it splits: the columns left of it and the rows above it.
    pub at: (u32, u32),
    /// How far the top and left panes are scrolled (as a scroll offset:
    /// zero or less).
    pub shift: Point<Pixels>,
    /// Upward/leftward lead of the bottom/right pane (`0..=(split - head)`) so the bottom/right pane
    /// can independently scroll all the way up to row 1 / column A (#135).
    pub main_lead: Point<Pixels>,
    /// Which pane is active on each axis: `(right_pane, bottom_pane)`.
    pub active_pane: (bool, bool),
}

#[ibis_office_macros::editor_ext(SplitView)]
impl SlideEditor {
    /// The current sheet's split panes, if it is split.
    pub fn split_panes(&self) -> Option<SplitPanes> {
        let (_, sheet) = self.current_sheet()?;
        let at = sheet.split?;
        let (shift, main_lead, active_pane) = self
            .sheet_state()
            .and_then(|s| s.panels.splits.get(&self.current_slide).map(|p| (p.shift, p.main_lead, p.active_pane)))
            .unwrap_or((Point::default(), Point::default(), (true, true)));
        Some(SplitPanes { at, shift, main_lead, active_pane })
    }

    /// How far the split panes are scrolled (nothing for frozen ones).
    pub(crate) fn pane_shift(&self) -> Point<Pixels> {
        self.split_panes().map_or(Point::default(), |p| p.shift)
    }

    /// Extra positive lead of the bottom/right split pane when scrolled into rows `1..at.1` / columns `A..at.0` (#135).
    pub(crate) fn main_pane_lead(&self) -> Point<Pixels> {
        self.split_panes().map_or(Point::default(), |p| p.main_lead)
    }

    /// Split the window at the selected cell (in the middle of the view if
    /// it is A1), or take the split away. Frozen panes are unfrozen first,
    /// as Excel does.
    pub(crate) fn toggle_split(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        if self.split_panes().is_some() {
            self.wb_mut().panels.splits.remove(&slide);
            if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::Split { slide, split: None })]) {
                log::warn!("un-splitting rejected: {err}");
            }
            return cx.notify();
        }
        let (Some((_, sheet)), Some((column, row))) = (self.current_sheet(), self.current_cell()) else { return };
        let at = if (column, row) == (0, 0) {
            // The middle of the view, in whole rows and columns.
            let (Some(ws), Some(content)) = (self.workspace_bounds, self.workspace_content()) else { return };
            let scale = content.scale;
            let offset = self.workspace_scroll.offset();
            let middle = |offsets: Vec<i64>, view: Pixels, scroll: Pixels| {
                let target = (f32::from(view) / 2.0 - f32::from(scroll)) / scale;
                offsets.iter().position(|&o| o as f32 > target).map_or(1, |i| i.saturating_sub(1).max(1)) as u32
            };
            (middle(sheet.column_offsets(), ws.size.width, offset.x), middle(sheet.row_offsets(), ws.size.height, offset.y))
        } else {
            (column, row)
        };
        if sheet.frozen.is_some() {
            if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::Freeze { slide, frozen: None })]) {
                log::warn!("unfreezing rejected: {err}");
            }
        }
        // The top/left pane starts at the top-left of the sheet so row 1 / column A are immediately reachable (#135).
        let shift = Point::default();
        let main_lead = Point::default();
        let active_pane = (column >= at.0, row >= at.1);
        self.wb_mut().panels.splits.insert(slide, SplitPanes { at, shift, main_lead, active_pane });
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::Split { slide, split: Some(at) })]) {
            log::warn!("splitting rejected: {err}");
        }
        cx.notify();
    }

    /// The wheel over a split pane: its own scrolling, on the axes it has
    /// (`across` sideways for left pane, `down` up and down for top pane;
    /// when `across == false` or `down == false`, scrolls the right/bottom
    /// pane via `main_lead` + `workspace_scroll`).
    pub(crate) fn scroll_split_pane(&mut self, event: &ScrollWheelEvent, across: bool, down: bool, cx: &mut Context<Self>) -> bool {
        let (Some(ws), Some(panes), Some(content)) = (self.workspace_bounds, self.split_panes(), self.workspace_content()) else { return false };
        let delta = event.delta.pixel_delta(px(20.));
        if delta.x.is_zero() && delta.y.is_zero() {
            return false;
        }
        let (split_x, split_y) = self.frozen_split(content).unwrap_or((None, None));
        let (head_w, head_h) = self.current_sheet().map_or((0.0, 0.0), |(_, sheet)| {
            (
                if sheet.headings { super::sheet_ui::row_header_width(sheet) } else { 0.0 },
                if sheet.headings { super::sheet_ui::COLUMN_HEADER_HEIGHT } else { 0.0 },
            )
        });
        let ws_w = f32::from(ws.size.width);
        let ws_h = f32::from(ws.size.height);
        let content_w = f32::from(content.size.width);
        let content_h = f32::from(content.size.height);

        let mut shift = panes.shift;
        let mut main_lead = panes.main_lead;
        let mut ws_offset = self.workspace_scroll.offset();
        let mut moved = false;

        if !delta.y.is_zero() && delta.y.abs() >= delta.x.abs() {
            if down {
                let sy = split_y.unwrap_or(ws_h * 0.5).max(1.0);
                let max_scroll = (content_h - sy).max(0.0);
                shift.y = (shift.y + delta.y).clamp(px(-max_scroll), px(0.));
                moved = true;
            } else {
                let sy = split_y.unwrap_or(0.0);
                let max_lead = (sy - head_h).max(0.0);
                let max_down = (content_h - ws_h).max(0.0);
                let current_combined = f32::from(main_lead.y) + f32::from(ws_offset.y);
                let next_combined = (current_combined + f32::from(delta.y)).clamp(-max_down, max_lead);
                if next_combined > 0.0 {
                    main_lead.y = px(next_combined);
                    ws_offset.y = px(0.0);
                } else {
                    main_lead.y = px(0.0);
                    ws_offset.y = px(next_combined);
                }
                self.workspace_scroll.set_offset(ws_offset);
                moved = true;
            }
        } else if !delta.x.is_zero() {
            if across {
                let sx = split_x.unwrap_or(ws_w * 0.5).max(1.0);
                let max_scroll = (content_w - sx).max(0.0);
                shift.x = (shift.x + delta.x).clamp(px(-max_scroll), px(0.));
                moved = true;
            } else {
                let sx = split_x.unwrap_or(0.0);
                let max_lead = (sx - head_w).max(0.0);
                let max_down = (content_w - ws_w).max(0.0);
                let current_combined = f32::from(main_lead.x) + f32::from(ws_offset.x);
                let next_combined = (current_combined + f32::from(delta.x)).clamp(-max_down, max_lead);
                if next_combined > 0.0 {
                    main_lead.x = px(next_combined);
                    ws_offset.x = px(0.0);
                } else {
                    main_lead.x = px(0.0);
                    ws_offset.x = px(next_combined);
                }
                self.workspace_scroll.set_offset(ws_offset);
                moved = true;
            }
        }
        if moved {
            let slide = self.current_slide;
            if let Some(p) = self.wb_mut().panels.splits.get_mut(&slide) {
                p.shift = shift;
                p.main_lead = main_lead;
            }
            cx.notify();
        }
        moved
    }

    /// Press on a split bar: double-click removes the split on that axis (or both);
    /// single click starts dragging the split bar.
    pub(crate) fn on_split_down(&mut self, axis: SplitAxis, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let Some(panes) = self.split_panes() else { return };
        if event.click_count >= 2 {
            // Excel: double-click on a split bar removes the split.
            let (col, row) = panes.at;
            let new_split = match axis {
                SplitAxis::Horizontal => if col > 0 { Some((col, 0)) } else { None },
                SplitAxis::Vertical => if row > 0 { Some((0, row)) } else { None },
                SplitAxis::Both => None,
                _ => return,
            };
            if new_split.is_none() {
                self.wb_mut().panels.splits.remove(&slide);
            } else if let Some(at) = new_split {
                if let Some(p) = self.wb_mut().panels.splits.get_mut(&slide) {
                    p.at = at;
                }
            }
            if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::Split { slide, split: new_split })]) {
                log::warn!("split removal rejected: {err}");
            }
            return cx.notify();
        }

        // Start split drag
        self.wb_mut().split_drag = Some(SplitDrag {
            axis,
            start: event.position,
            current: event.position,
            start_at: panes.at,
        });
        cx.notify();
    }

    /// Start or update scrolling from one of the split scrollbar tracks (#135).
    pub(crate) fn on_split_scrollbar_down(&mut self, axis: SplitAxis, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(panes) = self.split_panes() else { return };
        self.wb_mut().split_drag = Some(SplitDrag {
            axis,
            start: position,
            current: position,
            start_at: panes.at,
        });
        self.apply_split_scrollbar_pos(axis, position);
        cx.notify();
    }

    fn apply_split_scrollbar_pos(&mut self, axis: SplitAxis, position: Point<Pixels>) {
        let (Some(ws), Some(content)) = (self.workspace_bounds, self.workspace_content()) else { return };
        let Some(panes) = self.split_panes() else { return };
        let (split_x, split_y) = self.frozen_split(content).unwrap_or((None, None));
        let ws_w = f32::from(ws.size.width);
        let ws_h = f32::from(ws.size.height);
        let content_w = f32::from(content.size.width);
        let content_h = f32::from(content.size.height);
        let local_x = f32::from(position.x - ws.origin.x);
        let local_y = f32::from(position.y - ws.origin.y);
        let slide = self.current_slide;

        let (head_w, head_h) = self.current_sheet().map_or((0.0, 0.0), |(_, sheet)| {
            (
                if sheet.headings { super::sheet_ui::row_header_width(sheet) } else { 0.0 },
                if sheet.headings { super::sheet_ui::COLUMN_HEADER_HEIGHT } else { 0.0 },
            )
        });

        match axis {
            SplitAxis::ScrollHLeft => {
                let sx = split_x.unwrap_or(ws_w * 0.5).max(1.0);
                let ratio = (local_x / sx).clamp(0.0, 1.0);
                let max_scroll = (content_w - sx).max(0.0);
                let mut shift = panes.shift;
                shift.x = px(-ratio * max_scroll);
                let mut active = panes.active_pane;
                active.0 = false;
                self.wb_mut().panels.splits.insert(slide, SplitPanes { at: panes.at, shift, main_lead: panes.main_lead, active_pane: active });
            }
            SplitAxis::ScrollHRight => {
                let sx = split_x.unwrap_or(0.0);
                let track_w = (ws_w - sx - 12.0).max(1.0);
                let ratio = ((local_x - sx - 8.0) / (track_w - 16.0).max(1.0)).clamp(0.0, 1.0);
                let max_lead = (sx - head_w).max(0.0);
                let max_down = (content_w - ws_w).max(0.0);
                let total_range = max_lead + max_down;
                let combined = max_lead - ratio * total_range;
                let offset = self.workspace_scroll.offset();
                let mut main_lead = panes.main_lead;
                if combined > 0.0 {
                    main_lead.x = px(combined);
                    self.workspace_scroll.set_offset(point(px(0.0), offset.y));
                } else {
                    main_lead.x = px(0.0);
                    self.workspace_scroll.set_offset(point(px(combined), offset.y));
                }
                let mut active = panes.active_pane;
                active.0 = true;
                self.wb_mut().panels.splits.insert(slide, SplitPanes { at: panes.at, shift: panes.shift, main_lead, active_pane: active });
            }
            SplitAxis::ScrollVTop => {
                let sy = split_y.unwrap_or(ws_h * 0.5).max(1.0);
                let ratio = (local_y / sy).clamp(0.0, 1.0);
                let max_scroll = (content_h - sy).max(0.0);
                let mut shift = panes.shift;
                shift.y = px(-ratio * max_scroll);
                let mut active = panes.active_pane;
                active.1 = false;
                self.wb_mut().panels.splits.insert(slide, SplitPanes { at: panes.at, shift, main_lead: panes.main_lead, active_pane: active });
            }
            SplitAxis::ScrollVBottom => {
                let sy = split_y.unwrap_or(0.0);
                let track_h = (ws_h - sy - 12.0).max(1.0);
                let ratio = ((local_y - sy - 8.0) / (track_h - 16.0).max(1.0)).clamp(0.0, 1.0);
                let max_lead = (sy - head_h).max(0.0);
                let max_down = (content_h - ws_h).max(0.0);
                let total_range = max_lead + max_down;
                let combined = max_lead - ratio * total_range;
                let offset = self.workspace_scroll.offset();
                let mut main_lead = panes.main_lead;
                if combined > 0.0 {
                    main_lead.y = px(combined);
                    self.workspace_scroll.set_offset(point(offset.x, px(0.0)));
                } else {
                    main_lead.y = px(0.0);
                    self.workspace_scroll.set_offset(point(offset.x, px(combined)));
                }
                let mut active = panes.active_pane;
                active.1 = true;
                self.wb_mut().panels.splits.insert(slide, SplitPanes { at: panes.at, shift: panes.shift, main_lead, active_pane: active });
            }
            _ => {}
        }
    }

    /// Moving while dragging a split bar: update current position and notify.
    pub(crate) fn on_split_move(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(axis) = self.sheet_state().and_then(|s| s.split_drag.map(|d| d.axis)) else { return };
        if matches!(axis, SplitAxis::ScrollHLeft | SplitAxis::ScrollHRight | SplitAxis::ScrollVTop | SplitAxis::ScrollVBottom) {
            if let Some(drag) = self.wb_mut().split_drag.as_mut() {
                drag.current = position;
            }
            self.apply_split_scrollbar_pos(axis, position);
            cx.notify();
            return;
        }
        if let Some(drag) = self.wb_mut().split_drag.as_mut() {
            drag.current = position;
            cx.notify();
        }
    }

    /// Release while dragging a split bar: calculate new row/col split position and apply command.
    pub fn on_split_up(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        // The release may come after the workbook has closed in the same click (保存しない in the save question): the
        // sheet's listeners of the frame still get it.
        let Some(drag) = self.sheet_state_mut().and_then(|s| s.split_drag.take()) else { return };
        if matches!(drag.axis, SplitAxis::ScrollHLeft | SplitAxis::ScrollHRight | SplitAxis::ScrollVTop | SplitAxis::ScrollVBottom) {
            self.apply_split_scrollbar_pos(drag.axis, position);
            cx.notify();
            return;
        }
        let (Some(content), Some((shape, sheet))) = (self.workspace_content(), self.current_sheet()) else { return };
        let ws_origin = self.workspace_bounds.map_or(Point::default(), |b| b.origin);
        let local_x = f32::from(position.x - ws_origin.x);
        let local_y = f32::from(position.y - ws_origin.y);

        let emu_x = ((local_x - f32::from(content.slide_origin.x)) / content.scale) as i64 - shape.frame.x;
        let emu_y = ((local_y - f32::from(content.slide_origin.y)) / content.scale) as i64 - shape.frame.y;
        let head_w = if sheet.headings { super::sheet_ui::row_header_width(sheet) } else { 0.0 };
        let head_h = if sheet.headings { super::sheet_ui::COLUMN_HEADER_HEIGHT } else { 0.0 };

        let find_closest = |offsets: &[i64], target_emu: i64, min_head: f32, pos: f32| -> u32 {
            if pos <= min_head + 8.0 {
                return 0;
            }
            let mut best_i = 1;
            let mut min_diff = i64::MAX;
            for (i, &offset) in offsets.iter().enumerate().skip(1) {
                let diff = (offset - target_emu).abs();
                if diff < min_diff {
                    min_diff = diff;
                    best_i = i as u32;
                }
            }
            best_i
        };

        let mut new_at = drag.start_at;
        if drag.axis == SplitAxis::Vertical || drag.axis == SplitAxis::Both {
            new_at.0 = find_closest(&sheet.column_offsets(), emu_x, head_w, local_x);
        }
        if drag.axis == SplitAxis::Horizontal || drag.axis == SplitAxis::Both {
            new_at.1 = find_closest(&sheet.row_offsets(), emu_y, head_h, local_y);
        }

        let new_split = if new_at == (0, 0) {
            None
        } else {
            Some(new_at)
        };

        let slide = self.current_slide;
        if let Some(at) = new_split {
            let (shift, main_lead, active_pane) = self
                .sheet_state()
                .and_then(|s| s.panels.splits.get(&slide).map(|p| (p.shift, p.main_lead, p.active_pane)))
                .unwrap_or((Point::default(), Point::default(), (true, true)));
            self.wb_mut().panels.splits.insert(slide, SplitPanes { at, shift, main_lead, active_pane });
        } else {
            self.wb_mut().panels.splits.remove(&slide);
        }

        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::Split { slide, split: new_split })]) {
            log::warn!("split move rejected: {err}");
        }
        cx.notify();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplitDrag {
    pub(crate) axis: SplitAxis,
    pub(crate) start: Point<Pixels>,
    pub(crate) current: Point<Pixels>,
    pub(crate) start_at: (u32, u32),
}
