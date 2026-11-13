// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The workspace in the middle of the window: the slide, its shapes, the
//! selection frame with resize handles, and the in-place text field.

use gpui_kit::component::scroll::{ScrollableElement as _, ScrollbarAxis};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::format::with_format;
use crate::palette::rgb;
use ibis_doc_model::{EmuRect, ResizeHandle, Shape, ShapeKind, TextBody};

use crate::editor::{SlideEditor, SlideLayout, Tool, WorkspaceContent};
use crate::render::shape_view::{place, polygon, shape_element, shape_element_with_positions, slide_backdrop};

/// The alpha of the white under the scrollbars' thumbs in the page break preview (the user's choice, 2026-11-04).
const PAGE_BREAK_SCROLL_TRACK_ALPHA: f32 = 0.5;

impl SlideEditor {
    pub fn render_workspace(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        // The scrolling in bounds before anything reads it: the wheel adds
        // its turn unchecked, and GPUI holds the scrolled view in bounds
        // only as it lays it out, after the frozen panes and the headers
        // were placed by the turn (past the top, the left pane went down
        // and back while the sheet beside it stayed: #5).
        self.clamp_workspace_scroll();
        // Windows side by side that scroll together (only the screens with 並べて比較 turn it on).
        if self.sync_scrolling {
            self.sync_scroll_to_peer(cx);
        }
        let layout = self.slide_layout();
        let content = self.workspace_content();
        // The cell being typed into, laid out before the sheet is (the pop-ups of the cursor read its layout): one element,
        // over the frozen panes, not a part of the sheet they draw again.
        let cell_editor = content.filter(|_| !with_format!(self, workspace_page_layout_shown(self))).and_then(|content| with_format!(self, workspace_cell_editor(self, content, cx)));
        let theme = cx.theme();
        // The outer element is the workspace (focus, pointer, its bounds for
        // the layout); the inner one scrolls a slide larger than it.
        div()
            .id("workspace")
            .when(self.editing.is_none(), |d| {
                d.key_context(with_format!(self, workspace_key_context()))
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
                        match with_format!(self, workspace_page_layout(self, content, cx)) {
                            Some(papers) => d.child(papers),
                            None => {
                                let lead = with_format!(self, workspace_main_pane_lead(self));
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
            .when_some(layout.zip(content).filter(|_| !with_format!(self, workspace_page_layout_shown(self))), |d, (layout, content)| d.children(with_format!(self, workspace_frozen_panes(self, layout, content, cx))))
            .children(cell_editor)
            // Under the scrollbars, which stay usable at the headers' ends.
            .when_some(content.filter(|_| !with_format!(self, workspace_page_layout_shown(self))).and_then(|content| with_format!(self, workspace_headers(self, content, cx))), |d, headers| {
                d.child(headers)
            })
            .children(self.render_loading(cx))
            .child(crate::editor::wheel_zoom_listener(cx))
            .child(crate::editor::bounds_observer(move |bounds, window, cx| {
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
            .when(self.dialog_is_modal(), |d| d.child(crate::dialog::viewport_shield()))
            .map(|d| {
                if let Some(bars) = content.and_then(|content| with_format!(self, workspace_split_scrollbars(self, content, cx))) {
                    d.children(bars)
                } else if with_format!(self, workspace_page_break_preview_on(self)) {
                    // The page break preview greys the cells outside the pages, the grey of the scrollbar's thumb: the
                    // track gets a half-transparent white under the thumb, so the thumb shows on it.
                    d.child(
                        div().absolute().inset_0().child(
                            gpui_kit::component::scroll::Scrollbar::new(&self.workspace_scroll)
                                .id("workspace-scrollbar-page-break")
                                .axis(ScrollbarAxis::Both)
                                .viewport_from_layout()
                                .styles(|s| s.track(|t| t.bg(hsla(0.0, 0.0, 1.0, PAGE_BREAK_SCROLL_TRACK_ALPHA)))),
                        ),
                    )
                } else {
                    d.scrollbar(&self.workspace_scroll, ScrollbarAxis::Both)
                }
            })
    }

    /// The slide drawn for a page of the page layout view (its own scale, the cursor and the rest in it).
    pub fn render_slide_for_pages(&self, layout: SlideLayout, content: WorkspaceContent, cx: &mut Context<Self>) -> Div {
        self.render_slide(layout, content, cx)
    }

    pub fn render_slide(&self, layout: SlideLayout, content: WorkspaceContent, cx: &mut Context<Self>) -> Div {
        let slide = self.slide();
        let mut size = self.presentation().size_of(slide);
        let scale = layout.scale;
        // A worksheet's page is its whole grid, past the cells it uses.
        if let Some(sheet_frame) = with_format!(self, workspace_sheet_frame(self)) {
            let def_scale = scale.max(1e-9);
            let content_emu_w = (f32::from(content.size.width) / def_scale) as i64;
            let content_emu_h = (f32::from(content.size.height) / def_scale) as i64;
            size.width = size.width.max(sheet_frame.right()).max(content_emu_w);
            size.height = size.height.max(sheet_frame.bottom()).max(content_emu_h);
        }

        let hide_ink = with_format!(self, workspace_hide_ink(self));
        let mut shapes: Vec<AnyElement> = slide
            .shown_shapes()
            .filter(|shape| !(hide_ink && crate::ink::stroke_points(shape).is_some()))
            .map(|shape| self.render_shape(shape, scale, cx))
            .collect();
        // The selected cell is drawn over the worksheet and under the charts
        // and pictures on it, as in Excel.
        if let Some(index) = slide.shown_shapes().position(|s| matches!(s.kind, ShapeKind::Sheet(_))) {
            // Each overlay goes right over the sheet (so the one put in last is lowest);
            // `overlays` counts them, to put the page lines over all of them.
            let parts = with_format!(self, workspace_sheet_overlays(self, scale, cx));
            let overlays = parts.over_sheet.len();
            for overlay in parts.over_sheet {
                shapes.insert(index + 1, overlay);
            }
            // The page lines and numbers over the cells' borders and the selected cell (a manual break must show).
            if let Some(lines) = parts.page_lines {
                shapes.insert(index + 1 + overlays, lines);
            }
        }

        div()
            .absolute()
            .left(content.slide_origin.x)
            .top(content.slide_origin.y)
            .w(layout.to_px(size.width))
            .h(layout.to_px(size.height))
            .when(with_format!(self, workspace_page_shadow()), |d| d.shadow_md())
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
            if with_format!(self, workspace_sheet_frame(self)).is_some() {
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
                .on_mouse_down(MouseButton::Left, cx.listener(|this, event: &MouseDownEvent, window, cx| with_format!(this, workspace_grid_down(this, event, window, cx))))
                .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, window, cx| with_format!(this, workspace_grid_down(this, event, window, cx))))
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
                                this.drag = Some(crate::editor::Drag::Move {
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
                            if let Some(crate::editor::Drag::Move { shape, start, .. }) = this.drag
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

        // A group box only draws a frame round the controls in it: a press goes through it to what is under it
        // (the option buttons above it have their own, a cell below gets the rest), unless the controls are being designed.
        let click_through = matches!(&shape.kind, ShapeKind::Control(c) if c.kind == ibis_doc_model::ControlKind::GroupBox) && !with_format!(self, workspace_shape_design_mode(self));
        place(div().id(("shape", id.0)), frame, scale)
            .when(self.tool == Tool::Select && editing.is_none() && !click_through, |d| d.cursor_move())
            .when(empty_text_box, |d| d.border_1().border_dashed().border_color(cx.theme().border))
            .when(!click_through, |d| d.on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event, window, cx| this.on_shape_down(id, event, window, cx)),
            ))
            .when(with_format!(self, workspace_shape_has_right_click(self)) && !click_through, |d| d.on_mouse_down(MouseButton::Right, cx.listener(move |this, event, window, cx| with_format!(this, workspace_shape_right_down(this, id, event, window, cx)))))
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
                        if let Some(c) = caret.filter(|_| crate::text_edit::caret_blink_visible()) {
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
        if let Some(crop) = self.render_crop_mode(scale, cx) {
            return Some(crop);
        }
        let shape = self.selected_shape()?;
        if with_format!(self, workspace_hide_ink(self)) {
            if crate::ink::stroke_points(shape).is_some() {
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
                        if let Some(crate::editor::Drag::Resize { shape, start, .. }) = this.drag
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
        let border=super::selection_handles::border_region_with_menu("workspace-frame",Bounds::new(point(px(frame.x as f32*scale),px(frame.y as f32*scale)),size(px(frame.width as f32*scale),px(frame.height as f32*scale))),shape.rotation,cx.listener(move|e,event:&MouseDownEvent,w,cx|{if let Some(edit)=e.editing.as_mut(){edit.edit.is_new=false;}e.commit_edit(w,cx);let mut event=event.clone();event.click_count=1;e.on_shape_down(id,&event,w,cx);}),with_format!(self, workspace_shape_has_right_click(self)).then(||cx.listener(move|e,event:&MouseDownEvent,w,cx|with_format!(e, workspace_shape_right_down(e, id, event, w, cx)))));
        Some(div().absolute().size_full().top_0().left_0().child(outline).children(others).child(border).children(handles))
    }
}
