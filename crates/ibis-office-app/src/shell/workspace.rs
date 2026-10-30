// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The workspace in the middle of the window: the slide, its shapes, the
//! selection frame with resize handles, and the in-place text field.

use gpui_kit::component::scroll::{ScrollableElement as _, ScrollbarAxis};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{EmuRect, ResizeHandle, Shape, ShapeKind, TextBody};

use crate::shell::editor::{SlideEditor, SlideLayout, Tool, WorkspaceContent};
use crate::render::shape_view::{place, polygon, shape_element, shape_element_with_positions, slide_backdrop};

/// Side of a resize handle, in pixels.
const HANDLE_SIZE: f32 = 6.0;

impl SlideEditor {
    pub(crate) fn render_workspace(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        // The scrolling in bounds before anything reads it: the wheel adds
        // its turn unchecked, and GPUI holds the scrolled view in bounds
        // only as it lays it out, after the frozen panes and the headers
        // were placed by the turn (past the top, the left pane went down
        // and back while the sheet beside it stayed: #5).
        self.clamp_workspace_scroll();
        if (self.is_workbook() || self.is_document()) && self.sync_scrolling {
            self.sync_scroll_to_peer(cx);
        }
        let theme = cx.theme();
        let layout = self.slide_layout();
        let content = self.workspace_content();
        // The outer element is the workspace (focus, pointer, its bounds for
        // the layout); the inner one scrolls a slide larger than it.
        div()
            .id("workspace")
            .when(self.editing.is_none(), |d| {
                d.key_context(if self.is_workbook() { crate::shell::editor::SHEET_CONTEXT } else { crate::shell::editor::WORKSPACE_CONTEXT })
            })
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(Self::on_text_key))
            .on_scroll_wheel(cx.listener(Self::on_workspace_scroll))
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .bg(theme.muted)
            .when(self.tool == Tool::TextBox, |d| d.cursor_crosshair())
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_workspace_down))
            .child(
                div()
                    .id("workspace-scroll")
                    .size_full()
                    .overflow_scroll()
                    .track_scroll(&self.workspace_scroll)
                    .when_some(layout.zip(content), |d, (layout, content)| {
                        // The page layout view: the papers (page_layout.rs); else the slide.
                        match self.render_page_layout(content, cx) {
                            Some(papers) => d.child(papers),
                            None => {
                                let lead = self.main_pane_lead();
                                d.child(
                                    div()
                                        .relative()
                                        .flex_none()
                                        .left(lead.x)
                                        .top(lead.y)
                                        .w(content.size.width)
                                        .h(content.size.height)
                                        .child(self.render_slide(layout, content, cx)),
                                )
                            }
                        }
                    }),
            )
            // Frozen panes: the sheet again, over the scrolled one, held
            // still on their axis.
            .when_some(layout.zip(content).filter(|_| !self.page_layout_shown()), |d, (layout, content)| d.children(self.render_frozen_panes(layout, content, cx)))
            // Under the scrollbars, which stay usable at the headers' ends.
            .when_some(content.filter(|_| !self.page_layout_shown()).and_then(|content| self.render_sheet_headers(content, cx)), |d, headers| {
                d.child(headers)
            })
            .children(self.render_loading(cx))
            .child(crate::shell::editor::wheel_zoom_listener(cx))
            .child(crate::shell::editor::bounds_observer(move |bounds, window, cx| {
                let changed = entity.update(cx, |this, cx| {
                    if this.workspace_bounds != Some(bounds) {
                        this.workspace_bounds = Some(bounds);
                        cx.notify();
                        true
                    } else {
                        false
                    }
                });
                if changed {
                    window.on_next_frame(move |window, _| window.refresh());
                }
            }))
            .when(self.panel_dialog_is_modal(), |d| d.child(crate::shell::panel_dialog::viewport_shield()))
            .map(|d| {
                if self.split_panes().is_some() && !self.page_layout_shown()
                    && let Some(content) = content
                {
                    d.children(self.render_split_scrollbars(content, cx))
                } else {
                    d.scrollbar(&self.workspace_scroll, ScrollbarAxis::Both)
                }
            })
    }

    /// Independent horizontal and vertical scrollbars for each split pane (#135).
    fn render_split_scrollbars(&self, content: WorkspaceContent, cx: &mut Context<Self>) -> Vec<AnyElement> {
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
        let h_seg = |id: &'static str, left_px: f32, width_px: f32, scroll_x: f32, max_scroll: f32, axis: crate::xlsx::split_view::SplitAxis, cx: &mut Context<Self>| -> AnyElement {
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
        let v_seg = |id: &'static str, top_px: f32, height_px: f32, scroll_y: f32, max_scroll: f32, axis: crate::xlsx::split_view::SplitAxis, cx: &mut Context<Self>| -> AnyElement {
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

        let head_w = self.current_sheet().map_or(0.0, |(_, s)| if s.headings { crate::xlsx::sheet_ui::row_header_width(s) } else { 0.0 });
        let head_h = self.current_sheet().map_or(0.0, |(_, s)| if s.headings { crate::xlsx::sheet_ui::COLUMN_HEADER_HEIGHT } else { 0.0 });
        if let Some(sx) = split_x {
            let left_w = sx.clamp(24.0, (ws_w - bar_t - 24.0).max(24.0));
            let right_w = (ws_w - bar_t - left_w).max(24.0);
            let max_lead_x = (sx - head_w).max(0.0);
            let max_right = (content_w - ws_w).max(1.0);
            let right_scrolled = -(max_lead_x - (f32::from(offset.x) + f32::from(panes.main_lead.x)));
            bars.push(h_seg("split-scrollbar-h-left", 0.0, left_w, f32::from(panes.shift.x), (content_w - sx).max(1.0), crate::xlsx::split_view::SplitAxis::ScrollHLeft, cx));
            bars.push(h_seg("split-scrollbar-h-right", left_w, right_w, right_scrolled, (max_lead_x + max_right).max(1.0), crate::xlsx::split_view::SplitAxis::ScrollHRight, cx));
        } else {
            bars.push(h_seg("split-scrollbar-h-right", 0.0, (ws_w - bar_t).max(24.0), f32::from(offset.x), (content_w - ws_w).max(1.0), crate::xlsx::split_view::SplitAxis::ScrollHRight, cx));
        }

        if let Some(sy) = split_y {
            let top_h = sy.clamp(24.0, (ws_h - bar_t - 24.0).max(24.0));
            let bottom_h = (ws_h - bar_t - top_h).max(24.0);
            let max_lead_y = (sy - head_h).max(0.0);
            let max_down = (content_h - ws_h).max(1.0);
            let bottom_scrolled = -(max_lead_y - (f32::from(offset.y) + f32::from(panes.main_lead.y)));
            bars.push(v_seg("split-scrollbar-v-top", 0.0, top_h, f32::from(panes.shift.y), (content_h - sy).max(1.0), crate::xlsx::split_view::SplitAxis::ScrollVTop, cx));
            bars.push(v_seg("split-scrollbar-v-bottom", top_h, bottom_h, bottom_scrolled, (max_lead_y + max_down).max(1.0), crate::xlsx::split_view::SplitAxis::ScrollVBottom, cx));
        } else {
            bars.push(v_seg("split-scrollbar-v-bottom", 0.0, (ws_h - bar_t).max(24.0), f32::from(offset.y), (content_h - ws_h).max(1.0), crate::xlsx::split_view::SplitAxis::ScrollVBottom, cx));
        }
        bars
    }

    /// The frozen rows (which scroll only sideways), the frozen columns
    /// (only up and down) and their corner (not at all), each a copy of the
    /// page clipped to its part of the workspace, with a line along their
    /// edge, as Excel draws ウィンドウ枠の固定.
    fn render_frozen_panes(&self, layout: SlideLayout, content: WorkspaceContent, cx: &mut Context<Self>) -> Vec<AnyElement> {
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
                        this.on_split_down(crate::xlsx::split_view::SplitAxis::Horizontal, e, cx);
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
                        this.on_split_down(crate::xlsx::split_view::SplitAxis::Vertical, e, cx);
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
                        this.on_split_down(crate::xlsx::split_view::SplitAxis::Both, e, cx);
                    }))
                    .on_mouse_move(cx.listener(Self::on_pointer_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_pointer_up));
                panes.push(corner.into_any_element());
            }

            // Guide lines while dragging
            if let Some(drag) = self.sheet_state().and_then(|s| s.split_drag) {
                let guide_color = rgb(crate::xlsx::sheet_ui::SELECTION_COLOR);
                let ws_origin = self.workspace_bounds.map_or(Point::default(), |b| b.origin);
                let cur_x = drag.current.x - ws_origin.x;
                let cur_y = drag.current.y - ws_origin.y;
                if drag.axis == crate::xlsx::split_view::SplitAxis::Vertical || drag.axis == crate::xlsx::split_view::SplitAxis::Both {
                    panes.push(div().id("split-guide-v").absolute().top_0().left(cur_x - px(1.)).w(px(2.)).h_full().bg(guide_color).into_any_element());
                }
                if drag.axis == crate::xlsx::split_view::SplitAxis::Horizontal || drag.axis == crate::xlsx::split_view::SplitAxis::Both {
                    panes.push(div().id("split-guide-h").absolute().left_0().top(cur_y - px(1.)).w_full().h(px(2.)).bg(guide_color).into_any_element());
                }
            }
        } else {
            let line = rgb(crate::xlsx::sheet_ui::FROZEN_LINE);
            if let Some(h) = split_y {
                panes.push(div().absolute().left_0().top(px(h - 1.0)).w_full().h(px(1.)).bg(line).into_any_element());
            }
            if let Some(w) = split_x {
                panes.push(div().absolute().top_0().left(px(w - 1.0)).h_full().w(px(1.)).bg(line).into_any_element());
            }
        }
        panes
    }

    /// The slide drawn for a page of the page layout view (its own scale, the cursor and the rest in it).
    pub(crate) fn render_slide_for_pages(&self, layout: SlideLayout, content: WorkspaceContent, cx: &mut Context<Self>) -> Div {
        self.render_slide(layout, content, cx)
    }

    fn render_slide(&self, layout: SlideLayout, content: WorkspaceContent, cx: &mut Context<Self>) -> Div {
        let slide = self.slide();
        let mut size = self.presentation().size_of(slide);
        let scale = layout.scale;
        // A worksheet's page is its whole grid, past the cells it uses.
        if let Some((shape, _)) = self.current_sheet() {
            let def_scale = scale.max(1e-9);
            let content_emu_w = (f32::from(content.size.width) / def_scale) as i64;
            let content_emu_h = (f32::from(content.size.height) / def_scale) as i64;
            size.width = size.width.max(shape.frame.right()).max(content_emu_w);
            size.height = size.height.max(shape.frame.bottom()).max(content_emu_h);
        }

        let hide_ink = self.is_workbook() && self.current_sheet().map(|(_, s)| s.hide_ink).unwrap_or(false);
        let mut shapes: Vec<AnyElement> = slide
            .shown_shapes()
            .filter(|shape| !(hide_ink && crate::xlsx::ink::stroke_points(shape).is_some()))
            .map(|shape| self.render_shape(shape, scale, cx))
            .collect();
        // The selected cell is drawn over the worksheet and under the charts
        // and pictures on it, as in Excel.
        if let Some(index) = slide.shown_shapes().position(|s| matches!(s.kind, ShapeKind::Sheet(_))) {
            // Each overlay goes right over the sheet (so the one put in last is lowest);
            // `overlays` counts them, to put the page lines over all of them.
            let mut overlays = 0;
            if let Some(cursor) = self.render_cell_cursor(scale, cx) {
                shapes.insert(index + 1, cursor.into_any_element());
                overlays += 1;
            }
            // Page break preview greys the sheet outside the print area,
            // under the selected cell.
            if let Some(preview) = self.render_page_break_preview(scale, false) {
                shapes.insert(index + 1, preview.into_any_element());
                overlays += 1;
            }
            // Validation circles over invalid data.
            if let Some(circles) = self.render_validation_circles(scale) {
                shapes.insert(index + 1, circles.into_any_element());
                overlays += 1;
            }
            // Trace arrows over cells (参照元・参照先のトレース).
            if let Some(arrows) = self.render_trace_arrows(scale) {
                shapes.insert(index + 1, arrows.into_any_element());
                overlays += 1;
            }
            // The stroke being drawn with the pen (描画).
            if let Some(stroke) = self.render_ink_preview(scale) {
                shapes.insert(index + 1, stroke.into_any_element());
                overlays += 1;
            }
            // A chart's data ranges (データの編集), over the cells.
            if let Some(frames) = self.render_chart_ranges(scale) {
                shapes.insert(index + 1, frames.into_any_element());
                overlays += 1;
            }
            // The page lines and numbers over the cells' borders and the selected cell
            // (a manual break must show).
            if let Some(lines) = self.render_page_break_preview(scale, true) {
                shapes.insert(index + 1 + overlays, lines.into_any_element());
            }
        }

        div()
            .absolute()
            .left(content.slide_origin.x)
            .top(content.slide_origin.y)
            .w(layout.to_px(size.width))
            .h(layout.to_px(size.height))
            .when(!self.is_workbook(), |d| d.shadow_md())
            .child(slide_backdrop(slide, scale, &self.resources))
            .children(shapes)
            .when_some(self.render_selection(scale, cx), |d, sel| d.child(sel))
            .when_some(self.creation_rect(), |d, rect| {
                d.child(
                    place(div(), rect, scale)
                        .border_1()
                        .border_color(cx.theme().primary),
                )
            })
    }

    fn render_shape(&self, shape: &Shape, scale: f32, cx: &mut Context<Self>) -> AnyElement {
        let id = shape.id;
        if let ShapeKind::Sheet(_) = &shape.kind {
            // The grid is the page itself: a press selects a cell, and the
            // shape is never moved, resized or removed.
            let mut frame = shape.frame;
            if self.is_workbook() {
                if let Some(content) = self.workspace_content() {
                    let def_scale = scale.max(1e-9);
                    let content_emu_w = (f32::from(content.size.width) / def_scale) as i64;
                    let content_emu_h = (f32::from(content.size.height) / def_scale) as i64;
                    frame.width = frame.width.max(content_emu_w.saturating_sub(frame.x));
                    frame.height = frame.height.max(content_emu_h.saturating_sub(frame.y));
                }
            }
            return place(div().id(("shape", id.0)), frame, scale)
                .when(self.tool == Tool::Select, |d| d.cursor(CursorStyle::Arrow))
                .on_mouse_down(MouseButton::Left, cx.listener(Self::on_sheet_down))
                .on_mouse_down(MouseButton::Right, cx.listener(Self::on_sheet_right_down))
                .child(shape_element(shape, EmuRect { x: 0, y: 0, ..frame }, scale, &self.resources, None))
                .into_any_element();
        }
        let frame = self.display_frame(shape);
        let editing = self.editing.as_ref().filter(|e| e.shape == id);
        let positions = editing.map(|e| e.positions.clone());
        let mut drawn = shape.clone();
        if let Some(composing) = self.composing_body(id)
            && let Some(b) = drawn.text_body_mut()
        {
            *b = composing;
        }
        let body = drawn.text_body();
        let empty_text_box = matches!(shape.kind, ibis_doc_model::ShapeKind::TextBox(_))
            && body.is_some_and(TextBody::is_empty)
            && editing.is_none();
        let field = editing
            .map(|edit| {
                self.render_text_field(
                    edit.pending_click.is_some(),
                    cx,
                )
                .into_any_element()
            });
        // Drawn at the wrapper's origin; the wrapper takes the pointer.
        let local = EmuRect { x: 0, y: 0, ..frame };
        let w_px = frame.width as f32 * scale;
        let h_px = frame.height as f32 * scale;
        let border_strips = editing.is_some().then(|| {
            let strip = |left: Pixels, top: Pixels, width: Option<Pixels>, height: Option<Pixels>, cx: &mut Context<Self>| {
                let mut d = div()
                    .absolute()
                    .left(left)
                    .top(top)
                    .cursor(CursorStyle::OpenHand)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                            cx.stop_propagation();
                            if let Some(origin) = this.slide().shape(id).map(|s| s.frame) {
                                this.drag = Some(crate::shell::editor::Drag::Move {
                                    shape: id,
                                    start: event.position,
                                    current: event.position,
                                    origin,
                                });
                            }
                            cx.notify();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                            if let Some(crate::shell::editor::Drag::Move { shape, start, .. }) = this.drag
                                && shape == id
                                && (event.position.x - start.x).abs() < px(2.0)
                                && (event.position.y - start.y).abs() < px(2.0)
                            {
                                cx.stop_propagation();
                                this.drag = None;
                                if let Some(edit) = this.editing.as_mut()
                                    && edit.shape == id
                                {
                                    edit.edit.is_new = false;
                                }
                                this.commit_edit(window, cx);
                                this.focus_handle.focus(window, cx);
                            }
                        }),
                    );
                if let Some(w) = width {
                    d = d.w(w);
                } else {
                    d = d.right_0();
                }
                if let Some(h) = height {
                    d = d.h(h);
                } else {
                    d = d.bottom_0();
                }
                d.into_any_element()
            };
            let bw = px(6.0);
            vec![
                strip(px(0.0), px(-3.0), None, Some(bw), cx),
                strip(px(0.0), px(h_px - 3.0), None, Some(bw), cx),
                strip(px(-3.0), px(0.0), Some(bw), None, cx),
                strip(px(w_px - 3.0), px(0.0), Some(bw), None, cx),
            ]
        });

        place(div().id(("shape", id.0)), frame, scale)
            .when(self.tool == Tool::Select && editing.is_none(), |d| d.cursor_move())
            .when(empty_text_box, |d| d.border_1().border_dashed().border_color(cx.theme().border))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event, window, cx| this.on_shape_down(id, event, window, cx)),
            )
            .when(self.is_workbook(), |d| d.on_mouse_down(MouseButton::Right, cx.listener(move |this, event, window, cx| this.on_sheet_shape_right_down(id, event, window, cx))))
            .child(shape_element_with_positions(&drawn, local, scale, &self.resources, None, positions.as_ref()))
            .children(field)
            .children(border_strips.into_iter().flatten())
            .into_any_element()
    }

    /// The caret and selection overlay while a shape's text is edited.
    fn render_text_field(
        &self,
        pending: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.ensure_caret_blink(cx);
        let marks = self.caret_marks();
        let theme = cx.theme();
        let mut selected_color = theme.primary;
        selected_color.a = 0.25;
        let caret_color = theme.foreground;
        let entity = cx.entity();

        div()
            .absolute()
            .inset_0()
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.focus_handle.focus(window, cx);
                    if let Some(char_idx) = this.text_index_at(event.position) {
                        match event.click_count {
                            1 => {
                                this.shape_move_caret(char_idx, event.modifiers.shift, cx);
                                if let Some(edit) = this.editing.as_mut() {
                                    edit.drag_anchor = Some(edit.edit.anchor);
                                }
                            }
                            n => {
                                let (a, b) = this.text_unit_around(char_idx, n >= 3);
                                this.shape_move_caret(a, false, cx);
                                this.shape_move_caret(b, true, cx);
                                if let Some(edit) = this.editing.as_mut() {
                                    edit.drag_anchor = None;
                                }
                            }
                        }
                    }
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _window, cx| {
                if event.pressed_button != Some(MouseButton::Left) {
                    return;
                }
                if this.editing.as_ref().is_some_and(|e| e.drag_anchor.is_some())
                    && let Some(char_idx) = this.text_index_at(event.position)
                {
                    this.shape_move_caret(char_idx, true, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if let Some(edit) = this.editing.as_mut() {
                        edit.drag_anchor = None;
                    }
                    cx.notify();
                }),
            )
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, window, cx| {
                        if pending {
                            let entity = entity.clone();
                            cx.defer(move |cx| entity.update(cx, |this, cx| this.place_xlsx_edit_caret_at_pending_click(cx)));
                        }
                        let Some(marks) = marks.as_ref() else { return };
                        let Some((caret, selection)) = marks.polygons(false) else { return };
                        let mut paint = |q: [(f32, f32); 4], color: Hsla| {
                            let points: Vec<Point<Pixels>> = q.iter().map(|&(x, y)| point(px(x), px(y))).collect();
                            if let Some(p) = polygon(&points, None, true) {
                                window.paint_path(p, color);
                            }
                        };
                        for q in selection {
                            paint(q, selected_color);
                        }
                        if let Some(c) = caret.filter(|_| crate::pptx::text_edit::caret_blink_visible()) {
                            paint(c, caret_color);
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
    }

    /// Selection frame and the eight resize handles.
    fn render_selection(&self, scale: f32, cx: &mut Context<Self>) -> Option<Div> {
        let shape = self.selected_shape()?;
        if self.is_workbook() && self.current_sheet().map(|(_, s)| s.hide_ink).unwrap_or(false) {
            if crate::xlsx::ink::stroke_points(shape).is_some() {
                return None;
            }
        }
        let id = shape.id;
        let frame = self.display_frame(shape);
        let editing = self.editing.as_ref().is_some_and(|e| e.shape == id);
        let theme = cx.theme();
        let (primary, background) = (theme.primary, theme.background);

        let outline=super::selection_handles::outline(Bounds::new(point(px(frame.x as f32*scale),px(frame.y as f32*scale)),size(px(frame.width as f32*scale),px(frame.height as f32*scale))),primary).when(editing,|d|d.border_dashed());
        let others=self.selected_shapes().into_iter().filter(|s|s.id!=id).map(|shape|{let f=self.display_frame(shape);super::selection_handles::outline(Bounds::new(point(px(f.x as f32*scale),px(f.y as f32*scale)),size(px(f.width as f32*scale),px(f.height as f32*scale))),primary)}).collect::<Vec<_>>();

        let points=ResizeHandle::ALL.map(|handle|{let (ax,ay)=handle.anchor();(handle,point(px((frame.x as f32+frame.width as f32*ax as f32/2.)*scale),px((frame.y as f32+frame.height as f32*ay as f32/2.)*scale)))});
        let handles=super::selection_handles::group("workspace-selection-handles".into(),points,None,Default::default(),primary,if editing{rgb(0x80d4ff).into()}else{background}).into_iter().map(|(handle,element)|{
            let handle=handle.unwrap();
            element                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event, window, cx| {
                        this.on_handle_down(id, handle, event, window, cx)
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                        if let Some(crate::shell::editor::Drag::Resize { shape, start, .. }) = this.drag
                            && shape == id
                            && (event.position.x - start.x).abs() < px(2.0)
                            && (event.position.y - start.y).abs() < px(2.0)
                        {
                            cx.stop_propagation();
                            this.drag = None;
                            if let Some(edit) = this.editing.as_mut()
                                && edit.shape == id
                            {
                                edit.edit.is_new = false;
                            }
                            this.commit_edit(window, cx);
                            this.focus_handle.focus(window, cx);
                        }
                    }),
                )
        });

        // The outline is a sibling of the handles, so it never takes clicks
        // meant for the shape beneath it.
        let border=super::selection_handles::border_region("workspace-frame",Bounds::new(point(px(frame.x as f32*scale),px(frame.y as f32*scale)),size(px(frame.width as f32*scale),px(frame.height as f32*scale))),shape.rotation,cx.listener(move|e,event:&MouseDownEvent,w,cx|{if let Some(edit)=e.editing.as_mut(){edit.edit.is_new=false;}e.commit_edit(w,cx);let mut event=event.clone();event.click_count=1;e.on_shape_down(id,&event,w,cx);}));
        Some(div().absolute().size_full().top_0().left_0().child(outline).children(others).child(border).children(handles))
    }
}
