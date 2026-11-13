// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The workspace's worksheet parts (`FormatEditor`'s `workspace_*` hooks): the split panes' scrollbars and the frozen panes, drawn
//! over the page the window's workspace (`shell/workspace.rs`) lays out. They were in `shell/workspace.rs`.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::shell::editor::{SlideEditor, SlideLayout, WorkspaceContent};
use crate::shell::palette::rgb;
use crate::ext::*;

#[ibis_office_macros::editor_ext(WorkspaceHooks)]
impl SlideEditor {
    /// Independent horizontal and vertical scrollbars for each split pane (#135).
    pub(crate) fn render_split_scrollbars(&self, content: WorkspaceContent, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (Some(ws), Some(panes)) = (self.workspace_bounds, self.split_panes()) else { return Vec::new() };
        let (split_x, split_y) = self.frozen_split(content).unwrap_or((None, None));
        let ws_w = f32::from(ws.size.width);
        let ws_h = f32::from(ws.size.height);
        let content_w = f32::from(content.size.width).max(ws_w + 1.0);
        let content_h = f32::from(content.size.height).max(ws_h + 1.0);
        let offset = self.workspace_scroll.offset();
        let bar_t = 12.0_f32;
        let track_bg = crate::shell::palette::shade(0.95, 0.92);
        let border_col = rgb(0xdadce0);
        let thumb_bg = crate::shell::palette::shade(0.38, 0.65);

        let mut bars: Vec<AnyElement> = Vec::new();
        let h_seg = |id: &'static str, left_px: f32, width_px: f32, scroll_x: f32, max_scroll: f32, axis: crate::shell::split_axis::SplitAxis, cx: &mut Context<Self>| -> AnyElement {
            let w = width_px.max(24.0);
            let ratio_vis = (w / content_w).clamp(0.08, 0.90);
            let thumb_w = (w * ratio_vis).max(20.0).min((w - 4.0).max(4.0));
            let scroll_ratio = if max_scroll > 0.0 { (-scroll_x / max_scroll).clamp(0.0, 1.0) } else { 0.0 };
            let avail = (w - thumb_w - 4.0).max(0.0);
            let thumb_x = 2.0 + avail * scroll_ratio;
            div()
                .id(id)
                .absolute()
                .bottom_0()
                .left(px(left_px))
                .w(px(w))
                .h(px(bar_t))
                .bg(track_bg)
                .border_t_1()
                .border_r_1()
                .border_color(border_col)
                .cursor_pointer()
                .block_mouse_except_scroll()
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.on_split_scrollbar_down(axis, e.position, cx);
                }))
                .on_mouse_move(cx.listener(Self::on_pointer_move))
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_pointer_up))
                .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_pointer_up))
                .child(
                    div()
                        .absolute()
                        .top(px(2.0))
                        .left(px(thumb_x))
                        .w(px(thumb_w))
                        .h(px(bar_t - 4.0))
                        .rounded(px(4.0))
                        .bg(thumb_bg),
                )
                .into_any_element()
        };
        let v_seg = |id: &'static str, top_px: f32, height_px: f32, scroll_y: f32, max_scroll: f32, axis: crate::shell::split_axis::SplitAxis, cx: &mut Context<Self>| -> AnyElement {
            let h = height_px.max(24.0);
            let ratio_vis = (h / content_h).clamp(0.08, 0.90);
            let thumb_h = (h * ratio_vis).max(20.0).min((h - 4.0).max(4.0));
            let scroll_ratio = if max_scroll > 0.0 { (-scroll_y / max_scroll).clamp(0.0, 1.0) } else { 0.0 };
            let avail = (h - thumb_h - 4.0).max(0.0);
            let thumb_y = 2.0 + avail * scroll_ratio;
            div()
                .id(id)
                .absolute()
                .right_0()
                .top(px(top_px))
                .w(px(bar_t))
                .h(px(h))
                .bg(track_bg)
                .border_l_1()
                .border_b_1()
                .border_color(border_col)
                .cursor_pointer()
                .block_mouse_except_scroll()
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.on_split_scrollbar_down(axis, e.position, cx);
                }))
                .on_mouse_move(cx.listener(Self::on_pointer_move))
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_pointer_up))
                .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_pointer_up))
                .child(
                    div()
                        .absolute()
                        .left(px(2.0))
                        .top(px(thumb_y))
                        .w(px(bar_t - 4.0))
                        .h(px(thumb_h))
                        .rounded(px(4.0))
                        .bg(thumb_bg),
                )
                .into_any_element()
        };

        let head_w = self.current_sheet().map_or(0.0, |(_, s)| if s.headings { crate::shell::sheet_metrics::row_header_width(s) } else { 0.0 });
        let head_h = self.current_sheet().map_or(0.0, |(_, s)| if s.headings { crate::shell::sheet_metrics::COLUMN_HEADER_HEIGHT } else { 0.0 });
        if let Some(sx) = split_x {
            let left_w = sx.clamp(24.0, (ws_w - bar_t - 24.0).max(24.0));
            let right_w = (ws_w - bar_t - left_w).max(24.0);
            let max_lead_x = (sx - head_w).max(0.0);
            let max_right = (content_w - ws_w).max(1.0);
            let right_scrolled = -(max_lead_x - (f32::from(offset.x) + f32::from(panes.main_lead.x)));
            bars.push(h_seg("split-scrollbar-h-left", 0.0, left_w, f32::from(panes.shift.x), (content_w - sx).max(1.0), crate::shell::split_axis::SplitAxis::ScrollHLeft, cx));
            bars.push(h_seg("split-scrollbar-h-right", left_w, right_w, right_scrolled, (max_lead_x + max_right).max(1.0), crate::shell::split_axis::SplitAxis::ScrollHRight, cx));
        } else {
            bars.push(h_seg("split-scrollbar-h-right", 0.0, (ws_w - bar_t).max(24.0), f32::from(offset.x), (content_w - ws_w).max(1.0), crate::shell::split_axis::SplitAxis::ScrollHRight, cx));
        }

        if let Some(sy) = split_y {
            let top_h = sy.clamp(24.0, (ws_h - bar_t - 24.0).max(24.0));
            let bottom_h = (ws_h - bar_t - top_h).max(24.0);
            let max_lead_y = (sy - head_h).max(0.0);
            let max_down = (content_h - ws_h).max(1.0);
            let bottom_scrolled = -(max_lead_y - (f32::from(offset.y) + f32::from(panes.main_lead.y)));
            bars.push(v_seg("split-scrollbar-v-top", 0.0, top_h, f32::from(panes.shift.y), (content_h - sy).max(1.0), crate::shell::split_axis::SplitAxis::ScrollVTop, cx));
            bars.push(v_seg("split-scrollbar-v-bottom", top_h, bottom_h, bottom_scrolled, (max_lead_y + max_down).max(1.0), crate::shell::split_axis::SplitAxis::ScrollVBottom, cx));
        } else {
            bars.push(v_seg("split-scrollbar-v-bottom", 0.0, (ws_h - bar_t).max(24.0), f32::from(offset.y), (content_h - ws_h).max(1.0), crate::shell::split_axis::SplitAxis::ScrollVBottom, cx));
        }
        bars
    }


    /// The frozen rows (which scroll only sideways), the frozen columns
    /// (only up and down) and their corner (not at all), each a copy of the
    /// page clipped to its part of the workspace, with a line along their
    /// edge, as Excel draws ウィンドウ枠の固定.
    pub(crate) fn render_frozen_panes(&self, layout: SlideLayout, content: WorkspaceContent, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some((split_x, split_y)) = self.frozen_split(content) else { return Vec::new() };
        let offset = self.workspace_scroll.offset();
        let mut panes = Vec::new();
        // Split panes (表示 > 分割) are scrolled on their own (`pane_shift`),
        // by the wheel over them; frozen ones not at all.
        let pane_shift = self.pane_shift();
        let split = self.split_panes().is_some();
        let pane = |this: &Self, id: &'static str, width: Option<f32>, height: Option<f32>, shift: Point<Pixels>, cx: &mut Context<Self>| {
            div()
                .id(id)
                .absolute()
                .top_0()
                .left_0()
                .map(|d| match width {
                    Some(w) => d.w(px(w)),
                    None => d.w_full(),
                })
                .map(|d| match height {
                    Some(h) => d.h(px(h)),
                    None => d.h_full(),
                })
                .overflow_hidden()
                // The sheet scrolled under it gets no presses, but the wheel
                // still scrolls it.
                .block_mouse_except_scroll()
                .on_mouse_move(cx.listener(Self::on_pointer_move))
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_pointer_up))
                .when(split, |d| {
                    let (across, down) = (width.is_some(), height.is_some());
                    d.on_scroll_wheel(cx.listener(move |this, e: &ScrollWheelEvent, _, cx| {
                        if this.scroll_split_pane(e, across, down, cx) {
                            cx.stop_propagation();
                        }
                    }))
                })
                .child(
                    div()
                        .absolute()
                        .left(shift.x)
                        .top(shift.y)
                        .w(content.size.width)
                        .h(content.size.height)
                        .child(this.render_slide(layout, content, cx)),
                )
                .into_any_element()
        };
        let main_lead = self.main_pane_lead();
        if let Some(h) = split_y {
            panes.push(pane(self, "frozen-rows", None, Some(h), point(offset.x + main_lead.x, pane_shift.y), cx));
        }
        if let Some(w) = split_x {
            panes.push(pane(self, "frozen-columns", Some(w), None, point(pane_shift.x, offset.y + main_lead.y), cx));
        }
        if let (Some(w), Some(h)) = (split_x, split_y) {
            panes.push(pane(self, "frozen-corner", Some(w), Some(h), pane_shift, cx));
        }
        if split {
            let bar_size = 4.0;
            let bar_bg = rgb(0xD4D4D4);
            let border_color = rgb(0x808080);

            // Horizontal split bar
            if let Some(h) = split_y {
                let bar = div()
                    .id("split-bar-h")
                    .absolute()
                    .left_0()
                    .top(px(h - bar_size / 2.0))
                    .w_full()
                    .h(px(bar_size))
                    .bg(bar_bg)
                    .border_y_1()
                    .border_color(border_color)
                    .cursor(CursorStyle::ResizeUpDown)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.on_split_down(crate::shell::split_axis::SplitAxis::Horizontal, e, cx);
                    }))
                    .on_mouse_move(cx.listener(Self::on_pointer_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_pointer_up));
                panes.push(bar.into_any_element());
            }

            // Vertical split bar
            if let Some(w) = split_x {
                let bar = div()
                    .id("split-bar-v")
                    .absolute()
                    .top_0()
                    .left(px(w - bar_size / 2.0))
                    .w(px(bar_size))
                    .h_full()
                    .bg(bar_bg)
                    .border_x_1()
                    .border_color(border_color)
                    .cursor(CursorStyle::ResizeLeftRight)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.on_split_down(crate::shell::split_axis::SplitAxis::Vertical, e, cx);
                    }))
                    .on_mouse_move(cx.listener(Self::on_pointer_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_pointer_up));
                panes.push(bar.into_any_element());
            }

            // Intersection corner
            if let (Some(w), Some(h)) = (split_x, split_y) {
                let corner = div()
                    .id("split-bar-both")
                    .absolute()
                    .left(px(w - bar_size / 2.0))
                    .top(px(h - bar_size / 2.0))
                    .w(px(bar_size))
                    .h(px(bar_size))
                    .bg(bar_bg)
                    .cursor(CursorStyle::Crosshair)
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.on_split_down(crate::shell::split_axis::SplitAxis::Both, e, cx);
                    }))
                    .on_mouse_move(cx.listener(Self::on_pointer_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_pointer_up));
                panes.push(corner.into_any_element());
            }

            // Guide lines while dragging
            if let Some(drag) = self.sheet_state().and_then(|s| s.split_drag) {
                let guide_color = rgb(crate::shell::sheet_metrics::SELECTION_COLOR);
                let ws_origin = self.workspace_bounds.map_or(Point::default(), |b| b.origin);
                let cur_x = drag.current.x - ws_origin.x;
                let cur_y = drag.current.y - ws_origin.y;
                if drag.axis == crate::shell::split_axis::SplitAxis::Vertical || drag.axis == crate::shell::split_axis::SplitAxis::Both {
                    panes.push(div().id("split-guide-v").absolute().top_0().left(cur_x - px(1.)).w(px(2.)).h_full().bg(guide_color).into_any_element());
                }
                if drag.axis == crate::shell::split_axis::SplitAxis::Horizontal || drag.axis == crate::shell::split_axis::SplitAxis::Both {
                    panes.push(div().id("split-guide-h").absolute().left_0().top(cur_y - px(1.)).w_full().h(px(2.)).bg(guide_color).into_any_element());
                }
            }
        } else {
            let line = rgb(crate::shell::sheet_metrics::FROZEN_LINE);
            if let Some(h) = split_y {
                panes.push(div().absolute().left_0().top(px(h - 1.0)).w_full().h(px(1.)).bg(line).into_any_element());
            }
            if let Some(w) = split_x {
                panes.push(div().absolute().top_0().left(px(w - 1.0)).h_full().w(px(1.)).bg(line).into_any_element());
            }
        }
        panes
    }
}
