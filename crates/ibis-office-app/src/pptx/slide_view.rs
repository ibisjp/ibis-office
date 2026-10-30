// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The slide being edited, as the mock's ① works it (`tasks/doc/design/
//! toolbar.html`): a press selects the shape under it (Shift adds or takes
//! it away), a drag from nothing draws a rubber band that selects the shapes
//! wholly inside it, a drag moves the selection, the handles of one selected
//! shape resize and turn it, and the text box, shape and line tools place
//! what they make and hand back to the select tool (Esc does too). The
//! geometry is `interaction.rs`; the edits are `editing.rs`.

use gpui_kit::component::scroll::{ScrollableElement as _, ScrollbarAxis};
use gpui_kit::component::{ActiveTheme as _, Icon, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, EmuPoint, EmuRect, ResizeHandle, Shape, ShapeId, TextBody};

use super::editing::{self, Tool};
use super::interaction::{self, P};
use crate::render::shape_view::{place, polygon, shape_element_with_positions, slide_backdrop};
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;

/// Key context of the slide while a presentation is edited: the arrow keys
/// move the selection (or turn slides when nothing is selected).
pub(crate) const CONTEXT: &str = "PresentationWorkspace";

/// Side of a resize handle, in pixels.
const HANDLE_SIZE: f32 = 6.0;
/// A press that moves less than this is a click, in pixels.
const DRAG_THRESHOLD: f32 = 3.0;
/// A click with the line tool draws a line this long, in EMU (2 in).
const CLICK_LINE: i64 = 1_828_800;
/// A click with the shape tool draws a shape this big, in EMU (1 in).
const CLICK_SHAPE: i64 = 914_400;

actions!(
    pptx,
    [
        GroupShapes,
        UngroupShapes,
        /// 最前面・前面へ・背面へ・最背面へ（⌥⇧⌘↑・⌥⌘↑・⌥⌘↓・⌥⇧⌘↓）.
        BringToFront,
        /// 上付き・下付き（⇧⌘+・⌘=）.
        ToggleSuperscript,
        ToggleSubscript,
        BringForward,
        SendBackward,
        SendToBack,
        DeleteForward,
        ClearFormatting,
        OpenFind,
        ToggleGridlines,
        ToggleGuides,
        ToggleSnapToGrid,
        ToggleSnapToShape,
        /// The next cell of the table whose cell is edited (Tab).
        NextTableCell,
        /// フォント サイズを大きく・小さく（⌘⇧> ・⌘⇧<）.
        IncreaseFontSize,
        DecreaseFontSize,
        /// 左揃え・中央揃え・右揃え・両端揃え（⌘L・⌘E・⌘R・⌘J）.
        AlignTextLeft,
        AlignTextCenter,
        AlignTextRight,
        AlignTextJustify,
        /// 書式のコピー・貼り付け（⌥⌘C・⌥⌘V）.
        CopyTextFormat,
        PasteTextFormat,
        /// The previous cell (Shift+Tab).
        PreviousTableCell,
        /// 高コントラスト表示の切り替え（⌥⌘H）.
        ToggleHighContrast,
    ]
);

/// A pointer drag on the slide, previewed while it goes and turned into one
/// edit when it ends.
#[derive(Clone, Debug)]
pub(crate) enum Drag {
    Move { start: Point<Pixels>, current: Point<Pixels>, origins: Vec<(ShapeId, EmuRect)> },
    Resize { shape: ShapeId, handle: ResizeHandle, start: Point<Pixels>, current: Point<Pixels>, origin: EmuRect, rotation: i32, keep_aspect_on_corners: bool },
    Rotate { shape: ShapeId, start: Point<Pixels>, current: Point<Pixels>, origin: i32, center: Point<Pixels> },
    Band { start: Point<Pixels>, current: Point<Pixels>, before: Vec<ShapeId> },
    Place { tool: Tool, start: Point<Pixels>, current: Point<Pixels>, points: Vec<Point<Pixels>> },
    /// Selecting the edited text.
    Text { start: Point<Pixels>, current: Point<Pixels> },
    /// A table's border between columns (`column`: the one left of it) or
    /// rows (`row`: the one above it) moved: 列の幅・行の高さ.
    TableBorder { shape: ShapeId, column: Option<usize>, row: Option<usize>, start: Point<Pixels>, current: Point<Pixels> },
    /// 頂点の編集: vertex `index` of outline `part` dragged.
    Vertex { part: usize, index: usize, handle: u8, start: Point<Pixels>, current: Point<Pixels> },
}

impl Drag {
    fn current_mut(&mut self) -> &mut Point<Pixels> {
        match self {
            Drag::Move { current, .. } | Drag::Resize { current, .. } | Drag::Rotate { current, .. } | Drag::Band { current, .. } | Drag::Place { current, .. } | Drag::Text { current, .. } | Drag::TableBorder { current, .. } | Drag::Vertex { current, .. } => current,
        }
    }

    fn start_current(&self) -> (Point<Pixels>, Point<Pixels>) {
        match self {
            Drag::Move { start, current, .. } | Drag::Resize { start, current, .. } | Drag::Rotate { start, current, .. } | Drag::Band { start, current, .. } | Drag::Place { start, current, .. } | Drag::Text { start, current } | Drag::TableBorder { start, current, .. } | Drag::Vertex { start, current, .. } => (*start, *current),
        }
    }
}

fn moved(a: Point<Pixels>, b: Point<Pixels>) -> bool {
    (f32::from(a.x - b.x)).abs() >= DRAG_THRESHOLD || (f32::from(a.y - b.y)).abs() >= DRAG_THRESHOLD
}


impl SlideEditor {
    fn pptx_drag(&self) -> Option<&Drag> {
        self.pptx().and_then(|s| s.drag.as_ref())
    }

    fn shift_held(&self) -> bool {
        self.pptx().is_some_and(|s| s.shift)
    }

    /// A shape as drawn now: its frame and rotation with a drag in progress.
    pub(crate) fn preview(&self, shape: &Shape) -> (EmuRect, i32) {
        let (Some(drag), Some(layout)) = (self.pptx_drag(), self.slide_layout()) else { return (shape.frame, shape.rotation) };
        let shift = self.shift_held();
        let emu = |d: Pixels| layout.to_emu_len(d);
        match drag {
            Drag::Move { start, current, origins } if moved(*start, *current) => {
                let slide = self.slide();
                // Shapes over a chart move with it.
                let owner = slide.attached.iter().find(|(_, ids)| ids.contains(&shape.id)).map(|(c, _)| *c);
                let id = owner.unwrap_or(shape.id);
                if origins.iter().any(|(o, _)| *o == id) {
                    let (mut dx, mut dy) = (emu(current.x - start.x), emu(current.y - start.y));
                    if shift {
                        // Only along the axis moved most.
                        if dx.abs() > dy.abs() { dy = 0 } else { dx = 0 }
                    }
                    if self.is_pptx_snap_to_grid_enabled() {
                        let origin_p = origins.iter().find(|(o, _)| *o == id).map(|(_, f)| EmuPoint::new(f.x, f.y)).unwrap_or_else(|| EmuPoint::new(shape.frame.x, shape.frame.y));
                        let (sdx, sdy) = interaction::snap_delta_to_grid(origin_p, dx, dy, interaction::GRID_INTERVAL);
                        dx = sdx;
                        dy = sdy;
                    } else if self.is_pptx_snap_to_shape_enabled() {
                        let origin_f = origins.iter().find(|(o, _)| *o == id).map(|(_, f)| *f).unwrap_or(shape.frame);
                        let candidate_rect = origin_f.translated(dx, dy);
                        let others: Vec<EmuRect> = slide.shapes.iter().filter(|s| !origins.iter().any(|(o, _)| *o == s.id)).map(|s| s.frame).collect();
                        let size = self.presentation().size_of(slide);
                        let slop = emu(px(8.0));
                        let (snap_dx, snap_dy, _) = interaction::snap_to_shapes(candidate_rect, &others, size.width, size.height, slop);
                        dx += snap_dx;
                        dy += snap_dy;
                    }
                    return (shape.frame.translated(dx, dy), shape.rotation);
                }
                (shape.frame, shape.rotation)
            }
            Drag::Resize { shape: id, handle, start, current, origin, rotation, keep_aspect_on_corners } if *id == shape.id => {
                let mut d_x = emu(current.x - start.x) as f64;
                let mut d_y = emu(current.y - start.y) as f64;
                if self.is_pptx_snap_to_grid_enabled() {
                    d_x = interaction::snap_coord(d_x as i64, interaction::GRID_INTERVAL) as f64;
                    d_y = interaction::snap_coord(d_y as i64, interaction::GRID_INTERVAL) as f64;
                }
                let frame = interaction::resize(*origin, *rotation, *handle, d_x, d_y, shift || *keep_aspect_on_corners);
                (frame, shape.rotation)
            }
            Drag::Rotate { shape: id, start, current, origin, center } if *id == shape.id => {
                let p = |q: Point<Pixels>| P::new(f32::from(q.x) as f64, f32::from(q.y) as f64);
                (shape.frame, interaction::rotated(*origin, p(*center), p(*start), p(*current), shift))
            }
            _ => (shape.frame, shape.rotation),
        }
    }

    pub(crate) fn render_slide_workspace(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let theme = cx.theme();
        let layout = self.slide_layout();
        let content = self.workspace_content();
        let placing = self.pptx_tool() != Tool::Select;

        div()
            .id("workspace")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_scroll_wheel(cx.listener(Self::on_workspace_scroll))
            .on_key_down(cx.listener(Self::on_slide_key))
            .capture_key_down(cx.listener(Self::on_text_key))
            // In a table, Tab and Shift+Tab go to the next and previous cell
            // (bound here: GPUI gives Tab to the focus order otherwise).
            .on_action(cx.listener(|this, _: &NextTableCell, window, cx| {
                if this.is_outline_editing() {
                    this.outline_indent(true, window, cx);
                } else if this.text_editing().is_some_and(|e| e.cell.is_some()) {
                    this.next_table_cell(true, window, cx);
                } else if this.focus_handle.is_focused(window) && this.text_editing().is_none() && this.select_next_shape(true, cx) {
                    // Tab with no text edited: the next shape (as PowerPoint).
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &PreviousTableCell, window, cx| {
                if this.is_outline_editing() {
                    this.outline_indent(false, window, cx);
                } else if this.text_editing().is_some_and(|e| e.cell.is_some()) {
                    this.next_table_cell(false, window, cx);
                } else if this.focus_handle.is_focused(window) && this.text_editing().is_none() && this.select_next_shape(false, cx) {
                } else {
                    cx.propagate();
                }
            }))
            // Picture files dropped from the Finder: put on the slide where they are dropped.
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.drop_picture_files(paths.paths(), window.mouse_position(), window, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &IncreaseFontSize, window, cx| {
                cx.stop_propagation();
                this.step_font_size_of_text(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &DecreaseFontSize, window, cx| {
                cx.stop_propagation();
                this.step_font_size_of_text(false, window, cx);
            }))
            .on_action(cx.listener(|this, _: &AlignTextLeft, window, cx| {
                cx.stop_propagation();
                this.format_paragraphs(window, cx, ibis_doc_model::ParagraphFormat { align: Some(ibis_doc_model::TextAlign::Left), ..Default::default() });
            }))
            .on_action(cx.listener(|this, _: &AlignTextCenter, window, cx| {
                cx.stop_propagation();
                this.format_paragraphs(window, cx, ibis_doc_model::ParagraphFormat { align: Some(ibis_doc_model::TextAlign::Center), ..Default::default() });
            }))
            .on_action(cx.listener(|this, _: &AlignTextRight, window, cx| {
                cx.stop_propagation();
                this.format_paragraphs(window, cx, ibis_doc_model::ParagraphFormat { align: Some(ibis_doc_model::TextAlign::Right), ..Default::default() });
            }))
            .on_action(cx.listener(|this, _: &AlignTextJustify, window, cx| {
                cx.stop_propagation();
                this.format_paragraphs(window, cx, ibis_doc_model::ParagraphFormat { align: Some(ibis_doc_model::TextAlign::Justify), ..Default::default() });
            }))
            .on_action(cx.listener(|this, _: &CopyTextFormat, window, cx| {
                cx.stop_propagation();
                this.copy_character_format(window, cx);
            }))
            .on_action(cx.listener(|this, _: &PasteTextFormat, window, cx| {
                cx.stop_propagation();
                this.paste_character_format(window, cx);
            }))
            .on_action(cx.listener(|this, _: &GroupShapes, _, cx| {
                cx.stop_propagation();
                this.group_selected(cx);
            }))
            .on_action(cx.listener(|this, _: &UngroupShapes, _, cx| {
                cx.stop_propagation();
                this.ungroup_selected(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSuperscript, window, cx| {
                // While text is edited: 上付き; else ⌘+ is the zoom's.
                if this.text_editing().is_some() {
                    cx.stop_propagation();
                    this.toggle_baseline(30_000, window, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleSubscript, window, cx| {
                if this.text_editing().is_some() {
                    cx.stop_propagation();
                    this.toggle_baseline(-25_000, window, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &BringToFront, _, cx| {
                cx.stop_propagation();
                if this.text_editing().is_none() {
                    this.order_selected(editing::Order::Front, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &BringForward, _, cx| {
                cx.stop_propagation();
                if this.text_editing().is_none() {
                    this.order_selected(editing::Order::Forward, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SendBackward, _, cx| {
                cx.stop_propagation();
                if this.text_editing().is_none() {
                    this.order_selected(editing::Order::Backward, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SendToBack, _, cx| {
                cx.stop_propagation();
                if this.text_editing().is_none() {
                    this.order_selected(editing::Order::Back, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &OpenFind, window, cx| {
                cx.stop_propagation();
                this.open_pptx_find(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleGridlines, _, cx| {
                cx.stop_propagation();
                this.toggle_pptx_gridlines(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleGuides, _, cx| {
                cx.stop_propagation();
                this.toggle_pptx_guides(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSnapToGrid, _, cx| {
                cx.stop_propagation();
                this.toggle_pptx_snap_to_grid(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleSnapToShape, _, cx| {
                cx.stop_propagation();
                this.toggle_pptx_snap_to_shape(cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleHighContrast, _, cx| {
                cx.stop_propagation();
                this.toggle_high_contrast(cx);
            }))
            .on_action(cx.listener(|this, _: &ClearFormatting, window, cx| {
                cx.stop_propagation();
                this.clear_formats(window, cx);
            }))
            .on_action(cx.listener(|this, _: &DeleteForward, _, cx| {
                cx.stop_propagation();
                if this.is_outline_editing() {
                    this.outline_delete(true, cx);
                } else if this.text_editing().is_some() {
                    this.delete_text(true, cx);
                } else {
                    this.delete_selected(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::DeleteSelection, _, cx| {
                cx.stop_propagation();
                if this.is_outline_editing() {
                    this.outline_delete(false, cx);
                } else if this.text_editing().is_some() {
                    this.delete_text(false, cx);
                } else {
                    this.delete_selected(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::CancelTool, window, cx| {
                cx.stop_propagation();
                if this.is_outline_editing() {
                    this.end_outline_edit(cx);
                } else if this.is_drawing_path() {
                    this.cancel_path_draw(cx);
                } else if this.is_editing_points() {
                    this.end_edit_points(cx);
                } else if this.pptx().is_some_and(|p| p.slide_zoom_dialog.is_some()) {
                    this.close_slide_zoom_dialog(cx);
                } else if this.pptx().is_some_and(|p| p.section_zoom_dialog.is_some()) {
                    this.close_section_zoom_dialog(cx);
                } else if this.pptx().is_some_and(|p| p.summary_zoom_dialog.is_some()) {
                    this.close_summary_zoom_dialog(cx);
                } else if this.text_editing().is_some() {
                    this.end_text_edit(window, cx);
                } else if this.pptx_tool() != Tool::Select {
                    this.set_pptx_tool(Tool::Select, window, cx);
                } else {
                    this.set_selection(Vec::new());
                }
                cx.notify();
            }))
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .bg(theme.muted)
            .when(self.link_hover,|d|d.cursor_pointer()).on_mouse_move(cx.listener(|e,event:&MouseMoveEvent,_,cx|{e.update_link_hover(event,cx);e.update_clip_hover(event,cx)})).when(placing, |d| d.cursor_crosshair())
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_slide_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_slide_right_down))
            // The pointer left the workspace: no clip has its control bar for the pointer.
            .on_hover(cx.listener(|e, hovered: &bool, _, cx| {
                if !*hovered && e.clear_clip_hover() {
                    cx.notify();
                }
            }))
            .child(
                div()
                    .id("workspace-scroll")
                    .size_full()
                    .overflow_scroll()
                    .track_scroll(&self.workspace_scroll)
                    .when_some(layout.zip(content), |d, (layout, content)| {
                        let slide = self.render_edited_slide(layout.scale, content.slide_origin, cx);
                        d.child(div().relative().flex_none().w(content.size.width).h(content.size.height).child(slide))
                    }),
            )
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
            .child(drag_tracker(cx))
            .children(self.render_slide_zoom_dialog(cx))
            .children(self.render_section_zoom_dialog(cx))
            .children(self.render_summary_zoom_dialog(cx))
            .when(self.panel_dialog_is_modal(), |d| d.child(crate::shell::panel_dialog::viewport_shield()))
            .scrollbar(&self.workspace_scroll, ScrollbarAxis::Both)
    }

    /// The slide's comments' marks (コメント), here only: their author's
    /// initials, yellow (grey once resolved); a click shows the comments.
    fn render_comment_marks(&self, scale: f32, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let slide = self.slide();
        slide
            .comments
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let at = super::comments::mark_place(slide, c);
                div()
                    .id(("comment-mark", i))
                    .absolute()
                    .left(px(at.x as f32 * scale))
                    .top(px(at.y as f32 * scale))
                    .min_w(px(24.))
                    .h(px(24.))
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(rgb(0x9aa0a6))
                    .bg(if c.resolved { rgb(0xe8eaed) } else { rgb(0xfde293) })
                    .text_size(px(11.))
                    .text_color(rgb(0x202124))
                    .cursor_pointer()
                    .child(if c.author.initials.is_empty() { "…".to_string() } else { c.author.initials.clone() })
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.open_slide_comments(window, cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_comment_context_menu(i, event.position, window, cx);
                    }))
                    .into_any_element()
            })
            .collect()
    }

    fn render_edited_slide(&self, scale: f32, origin: Point<Pixels>, cx: &mut Context<Self>) -> Div {
        let slide = self.slide();
        let size = self.presentation().size_of(slide);
        let positions = self.pptx().map(|s| &s.positions);
        let layout = slide.layout.and_then(|l| self.presentation().layout(l));
        self.link_text_positions.borrow_mut().clear();
        let shapes: Vec<AnyElement> = slide
            .shown_shapes()
            .map(|shape| {
                let (frame, rotation) = self.preview(shape);
                let editing = self.text_editing().is_some_and(|e| e.shape == shape.id);
                let empty_text_box = matches!(shape.kind, ibis_doc_model::ShapeKind::TextBox(_)) && shape.text_body().is_some_and(TextBody::is_empty) && !editing;
                let mut drawn = if rotation != shape.rotation { Shape { rotation, ..shape.clone() } } else { shape.clone() };
                // What is being composed shows in the shape.
                if let Some(body) = self.composing_body(shape.id)
                    && let Some(b) = drawn.text_body_mut()
                {
                    *b = body;
                }
                // An empty placeholder (プレースホルダー) shows its prompt
                // (案内文字) in its own text style, here only: not in the
                // slide show, the thumbnails, printing or exports, and not
                // while it is typed into.
                let placeholder = slide.placeholders.get(&shape.id).filter(|_| {
                    !editing && drawn.text_body().is_none_or(TextBody::is_empty) && !matches!(shape.kind, ibis_doc_model::ShapeKind::Picture(_) | ibis_doc_model::ShapeKind::Table(_) | ibis_doc_model::ShapeKind::Chart(_))
                });
                if let Some(prompt) = placeholder.and_then(|p| layout?.prompt_for(&p.placeholder))
                    && let Some(b) = drawn.text_body_mut()
                {
                    b.set_text(&crate::shell::i18n::text(prompt));
                }
                // A table cell being edited: its text with the caret's positions.
                let cell = self.text_editing().filter(|e| e.shape == shape.id).and_then(|e| e.cell);
                if let ibis_doc_model::ShapeKind::Table(table) = &drawn.kind {
                    let mut table = (**table).clone();
                    if let Some(body) = self.composing_body(shape.id)
                        && let Some(c) = cell.and_then(|cell|table.rows.get_mut(cell.0).and_then(|r| r.cells.get_mut(cell.1)))
                    {
                        c.text = body;
                    }
                    let mut recorded=std::collections::HashMap::new();
                    for(r,row)in table.rows.iter().enumerate(){for(c,entry)in row.cells.iter().enumerate(){
                        if entry.text.paragraphs.iter().any(|p|p.runs.iter().any(|r|r.style.link.is_some())){
                            let slot=positions.filter(|_|cell==Some((r,c))).cloned().unwrap_or_default();
                            self.link_text_positions.borrow_mut().insert((shape.id,Some((r,c))),slot.clone());recorded.insert((r,c),slot);
                        }
                    }}
                    let local = EmuRect { x: 0, y: 0, ..frame };
                    return place(div(), frame, scale)
                        .child(place(div(), local, scale).child(crate::render::table_view::table_element_recording(&table, local, scale, &self.resources, cell.zip(positions),Some(&recorded))))
                        .into_any_element();
                }
                let linked_positions=drawn.text_body().filter(|b|b.paragraphs.iter().any(|p|p.runs.iter().any(|r|r.style.link.is_some()))).map(|_|{
                    let slot=positions.filter(|_|editing).cloned().unwrap_or_default();
                    self.link_text_positions.borrow_mut().insert((shape.id,None),slot.clone());slot
                });
                let mut shape_div = place(div(), frame, scale)
                    .when(empty_text_box || placeholder.is_some(), |d| d.border_1().border_dashed().border_color(cx.theme().border))
                    .child(shape_element_with_positions(&drawn, EmuRect { x: 0, y: 0, ..frame }, scale, &self.resources, None, positions.filter(|_| editing).or(linked_positions.as_ref())));

                if let Some(ph) = placeholder {
                    let w_px = frame.width as f32 * scale;
                    let h_px = frame.height as f32 * scale;
                    if w_px >= 40.0 && h_px >= 40.0 {
                        if let Some(actions) = Self::placeholder_quick_actions(&ph.placeholder) {
                            shape_div = shape_div.child(self.render_placeholder_icons(actions, scale, cx));
                        }
                    }
                }

                shape_div.into_any_element()
            })
            .collect();
        let mut slide_div = div()
            .absolute()
            .left(origin.x)
            .top(origin.y)
            .w(px(size.width as f32 * scale))
            .h(px(size.height as f32 * scale))
            .shadow_md()
            .when(self.is_high_contrast(), |d| d.border_2().border_color(rgb(0x000000)))
            .child(slide_backdrop(slide, scale, &self.resources));
        if self.is_pptx_gridlines_enabled() {
            slide_div = slide_div.child(self.render_gridlines(size, scale));
        }
        if self.is_pptx_guides_enabled() {
            slide_div = slide_div.child(self.render_guides(size, scale));
        }
        slide_div
            .children(shapes)
            .children(self.render_comment_marks(scale, cx))
            .child(self.render_selection_overlay(scale, cx))
            .children(self.render_clip_bars(cx))
    }

    fn render_gridlines(&self, size: ibis_doc_model::EmuSize, scale: f32) -> impl IntoElement {
        let w = size.width as f32 * scale;
        let h = size.height as f32 * scale;
        let step = interaction::GRID_INTERVAL as f32 * scale;
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let color = hsla(0.0, 0.0, 0.5, 0.18);
                let mut x = step;
                while x < w {
                    let p1 = bounds.origin + point(px(x), px(0.));
                    let p2 = bounds.origin + point(px(x), px(h));
                    if let Some(path) = polygon(&[p1, p2], Some(px(0.8)), false) {
                        window.paint_path(path, color);
                    }
                    x += step;
                }
                let mut y = step;
                while y < h {
                    let p1 = bounds.origin + point(px(0.), px(y));
                    let p2 = bounds.origin + point(px(w), px(y));
                    if let Some(path) = polygon(&[p1, p2], Some(px(0.8)), false) {
                        window.paint_path(path, color);
                    }
                    y += step;
                }
            },
        )
        .absolute()
        .size_full()
    }

    fn render_guides(&self, size: ibis_doc_model::EmuSize, scale: f32) -> impl IntoElement {
        let w = size.width as f32 * scale;
        let h = size.height as f32 * scale;
        let mid_x = (size.width / 2) as f32 * scale;
        let mid_y = (size.height / 2) as f32 * scale;
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let color = crate::shell::palette::doc(0xe05638);
                let p1 = bounds.origin + point(px(mid_x), px(0.));
                let p2 = bounds.origin + point(px(mid_x), px(h));
                if let Some(path) = polygon(&[p1, p2], Some(px(1.0)), false) {
                    window.paint_path(path, color);
                }
                let p3 = bounds.origin + point(px(0.), px(mid_y));
                let p4 = bounds.origin + point(px(w), px(mid_y));
                if let Some(path) = polygon(&[p3, p4], Some(px(1.0)), false) {
                    window.paint_path(path, color);
                }
            },
        )
        .absolute()
        .size_full()
    }

    fn placeholder_quick_actions(ph: &ibis_doc_model::PlaceholderRef) -> Option<&'static [&'static str]> {
        let kind = ph.kind.as_deref().unwrap_or("");
        let norm = ph.normalized();
        if matches!(kind, "title" | "ctrTitle" | "subTitle" | "dt" | "ftr" | "sldNum") {
            return None;
        }
        match kind {
            "tbl" => Some(&["table"]),
            "chart" => Some(&["chart"]),
            "pic" => Some(&["pic"]),
            "media" => Some(&["media"]),
            _ if norm == "body" => Some(&["table", "chart", "pic", "media"]),
            _ => None,
        }
    }

    fn render_placeholder_icons(&self, actions: &[&'static str], scale: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = h_flex().items_center().justify_center().gap(px(8.));
        let btn_size = (32.0 * scale.clamp(0.6, 1.2)).round();
        let icon_size = (18.0 * scale.clamp(0.6, 1.2)).round();

        for &action in actions {
            let (id_name, icon_name, tooltip_text) = match action {
                "table" => ("btn-ph-table", "table_chart", "_pptx.insert-table"),
                "chart" => ("btn-ph-chart", "bar_chart", "_pptx.insert-chart"),
                "pic" => ("btn-ph-picture", "image", "_pptx.insert-picture"),
                "media" => ("btn-ph-media", "movie", "_pptx.insert-video"),
                _ => continue,
            };

            let mut btn = div()
                .id(id_name)
                .w(px(btn_size))
                .h(px(btn_size))
                .rounded(px(4.))
                .border_1()
                .border_color(rgb(0xd0d5dd))
                .bg(rgb(0xf8f9fa))
                .hover(|h| h.bg(rgb(0xe8eaed)).border_color(rgb(0x9aa0a6)))
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .child(Icon::new(Material(icon_name)).size(px(icon_size)).text_color(rgb(0x5f6368)))
                .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::shell::i18n::text(tooltip_text)).build(window, cx));

            btn = match action {
                "table" => btn.on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.insert_slide_table(2, 5, window, cx);
                })),
                "chart" => btn.on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.insert_slide_chart(window, cx);
                })),
                "pic" => btn.on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.pick_picture(window, cx);
                })),
                "media" => btn.on_mouse_down(MouseButton::Left, cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.pick_clip(false, window, cx);
                })),
                _ => btn,
            };

            row = row.child(btn);
        }

        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(row)
    }

    pub(crate) fn active_alignment_guides(&self) -> Vec<interaction::AlignmentGuide> {
        let (Some(drag), Some(layout)) = (self.pptx_drag(), self.slide_layout()) else { return Vec::new() };
        if !self.is_pptx_snap_to_shape_enabled() || self.is_pptx_snap_to_grid_enabled() {
            return Vec::new();
        }
        let emu = |d: Pixels| layout.to_emu_len(d);
        match drag {
            Drag::Move { start, current, origins } if moved(*start, *current) => {
                let slide = self.slide();
                let (mut dx, mut dy) = (emu(current.x - start.x), emu(current.y - start.y));
                if self.shift_held() {
                    if dx.abs() > dy.abs() { dy = 0 } else { dx = 0 }
                }
                let Some((_, first_frame)) = origins.first() else { return Vec::new() };
                let rect = first_frame.translated(dx, dy);
                let others: Vec<EmuRect> = slide.shapes.iter().filter(|s| !origins.iter().any(|(o, _)| *o == s.id)).map(|s| s.frame).collect();
                let size = self.presentation().size_of(slide);
                let slop = emu(px(8.0));
                let (_, _, guides) = interaction::snap_to_shapes(rect, &others, size.width, size.height, slop);
                guides
            }
            _ => Vec::new(),
        }
    }

    /// The outlines of the selected shapes, the handles of a single one,
    /// the rubber band and the outline of what is being placed.
    fn render_selection_overlay(&self, scale: f32, cx: &mut Context<Self>) -> Div {
        self.ensure_caret_blink(cx);
        let theme = cx.theme();
        let (primary, background) = (theme.primary, theme.background);
        let slide = self.slide();
        let selected = self.selected_shapes();
        let editing = self.text_editing().map(|e| e.shape);
        // Outlines as drawn (rotated), in slide pixels.
        let mut outlines: Vec<(Vec<Point<Pixels>>, bool)> = Vec::new();
        for shape in &selected {
            let (frame, rotation) = self.preview(shape);
            let pts = interaction::corners(frame, rotation).map(|p| point(px(p.x as f32 * scale), px(p.y as f32 * scale)));
            outlines.push((pts.to_vec(), editing == Some(shape.id)));
        }
        let mut dashed: Vec<Vec<Point<Pixels>>> = Vec::new();
        // 頂点の編集: the outline (the vertex dragged where the pointer is) in place of the frame.
        let editing_points = self.is_editing_points();
        if editing_points {
            outlines.clear();
            let dragged = match (self.pptx_drag(), self.slide_layout()) {
                (Some(Drag::Vertex { part, index, handle, start, current }), Some(layout)) if moved(*start, *current) => {
                    let to = layout.to_emu(*current);
                    Some((*part, *index, *handle, (to.x as f64, to.y as f64)))
                }
                _ => None,
            };
            let nodes = self.edit_point_nodes_dragged(dragged);
            for polygon in ibis_doc_model::shape_merge::nodes_polylines(&nodes) {
                let pts: Vec<Point<Pixels>> = polygon.iter().map(|v| point(px(v.0 as f32 * scale), px(v.1 as f32 * scale))).collect();
                outlines.push((pts, false));
            }
            // The handles of the vertex selected, with a line to each.
            if let Some((l, i)) = self.edit_point_selected()
                && let Some(n) = nodes.get(l).and_then(|f| f.get(i))
            {
                for h in [n.cin, n.cout].into_iter().flatten() {
                    let line = [n.p, h].map(|v| point(px(v.0 as f32 * scale), px(v.1 as f32 * scale)));
                    outlines.push((line.to_vec(), false));
                }
            }
        }
        // カスタム軌跡: the lines between the points clicked, from the shape's middle.
        if let Some(from) = self.path_draw_start() {
            let pts: Vec<Point<Pixels>> = std::iter::once(from).chain(self.path_draw_points()).map(|v| point(px(v.0 as f32 * scale), px(v.1 as f32 * scale))).collect();
            if pts.len() >= 2 {
                dashed.push(pts);
            }
        }
        if let (Some(drag), Some(layout)) = (self.pptx_drag(), self.slide_layout()) {
            let (start, current) = drag.start_current();
            let to = |q: Point<Pixels>| layout.to_emu(q);
            let rect_pts = |r: EmuRect| {
                vec![
                    point(px(r.x as f32 * scale), px(r.y as f32 * scale)),
                    point(px(r.right() as f32 * scale), px(r.y as f32 * scale)),
                    point(px(r.right() as f32 * scale), px(r.bottom() as f32 * scale)),
                    point(px(r.x as f32 * scale), px(r.bottom() as f32 * scale)),
                ]
            };
            match drag {
                Drag::Band { .. } if moved(start, current) => dashed.push(rect_pts(EmuRect::from_corners(to(start), to(current)))),
                // Where the table's border goes.
                Drag::TableBorder { shape, column, .. } => {
                    if let Some(sh) = slide.shape(*shape) {
                        let c = to(current);
                        let f = sh.frame;
                        let line = if column.is_some() { [(c.x, f.y), (c.x, f.bottom())] } else { [(f.x, c.y), (f.right(), c.y)] };
                        dashed.push(line.iter().map(|&(x, y)| point(px(x as f32 * scale), px(y as f32 * scale))).collect());
                    }
                }
                Drag::Place { tool: Tool::Line { kind, .. }, start, current, points } => {
                    let (mut a, mut b) = (to(*start), to(*current));
                    if self.is_pptx_snap_to_grid_enabled() {
                        a = interaction::snap_point(a, interaction::GRID_INTERVAL);
                        b = interaction::snap_point(b, interaction::GRID_INTERVAL);
                    }
                    match kind {
                        editing::LineKind::Freehand => {
                            if points.len() >= 2 {
                                let pts: Vec<Point<Pixels>> = points.iter().map(|&p| {
                                    let emu = to(p);
                                    point(px(emu.x as f32 * scale), px(emu.y as f32 * scale))
                                }).collect();
                                dashed.push(pts);
                            }
                        }
                        editing::LineKind::Straight => {
                            dashed.push(vec![point(px(a.x as f32 * scale), px(a.y as f32 * scale)), point(px(b.x as f32 * scale), px(b.y as f32 * scale))]);
                        }
                        editing::LineKind::Bent => {
                            let mid_x = (a.x + b.x) / 2;
                            dashed.push(vec![
                                point(px(a.x as f32 * scale), px(a.y as f32 * scale)),
                                point(px(mid_x as f32 * scale), px(a.y as f32 * scale)),
                                point(px(mid_x as f32 * scale), px(b.y as f32 * scale)),
                                point(px(b.x as f32 * scale), px(b.y as f32 * scale)),
                            ]);
                        }
                        editing::LineKind::Curved => {
                            let mut pts = Vec::with_capacity(17);
                            let mid_x = (a.x + b.x) as f64 / 2.0;
                            let p0 = (a.x as f64, a.y as f64);
                            let p1 = (mid_x, a.y as f64);
                            let p2 = (mid_x, b.y as f64);
                            let p3 = (b.x as f64, b.y as f64);
                            for i in 0..=16 {
                                let t = i as f64 / 16.0;
                                let mt = 1.0 - t;
                                let x = mt * mt * mt * p0.0 + 3.0 * mt * mt * t * p1.0 + 3.0 * mt * t * t * p2.0 + t * t * t * p3.0;
                                let y = mt * mt * mt * p0.1 + 3.0 * mt * mt * t * p1.1 + 3.0 * mt * t * t * p2.1 + t * t * t * p3.1;
                                pts.push(point(px(x as f32 * scale), px(y as f32 * scale)));
                            }
                            dashed.push(pts);
                        }
                    }
                }
                Drag::Place { .. } if moved(start, current) => {
                    let (mut a, mut b) = (to(start), to(current));
                    if self.is_pptx_snap_to_grid_enabled() {
                        a = interaction::snap_point(a, interaction::GRID_INTERVAL);
                        b = interaction::snap_point(b, interaction::GRID_INTERVAL);
                    }
                    dashed.push(rect_pts(interaction::drawn_frame(a, b, self.shift_held())));
                }
                _ => {}
            }
        }
        let band_color = primary;
        let alignment_guides = self.active_alignment_guides();
        let paths = canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let at = |p: &Point<Pixels>| bounds.origin + *p;
                for (pts, edit) in &outlines {
                    let pts: Vec<Point<Pixels>> = pts.iter().map(at).collect();
                    if let Some(path) = polygon(&pts, Some(px(crate::shell::selection_handles::OUTLINE_WIDTH)), true) {
                        window.paint_path(path, primary);
                    }
                }
                for pts in &dashed {
                    let pts: Vec<Point<Pixels>> = pts.iter().map(at).collect();
                    let closed = pts.len() > 2;
                    if closed {
                        let mut fill = band_color;
                        fill.a = 0.08;
                        if let Some(path) = polygon(&pts, None, true) {
                            window.paint_path(path, fill);
                        }
                    }
                    if let Some(path) = polygon(&pts, Some(px(1.0)), closed) {
                        window.paint_path(path, band_color);
                    }
                }
                for guide in &alignment_guides {
                    let path = match guide {
                        interaction::AlignmentGuide::Vertical { x, y1, y2 } => {
                            let p1 = bounds.origin + point(px(*x as f32 * scale), px(*y1 as f32 * scale));
                            let p2 = bounds.origin + point(px(*x as f32 * scale), px(*y2 as f32 * scale));
                            polygon(&[p1, p2], Some(px(1.5)), false)
                        }
                        interaction::AlignmentGuide::Horizontal { y, x1, x2 } => {
                            let p1 = bounds.origin + point(px(*x1 as f32 * scale), px(*y as f32 * scale));
                            let p2 = bounds.origin + point(px(*x2 as f32 * scale), px(*y as f32 * scale));
                            polygon(&[p1, p2], Some(px(1.5)), false)
                        }
                    };
                    if let Some(path) = path {
                        window.paint_path(path, crate::shell::palette::doc(0xe05638));
                    }
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();

        // Handles: one selected shape, not while its text is edited or it is
        // part of another.
        let mut handles: Vec<AnyElement> = Vec::new();
        if editing_points {
            let selected_vertex = self.edit_point_selected();
            let dragged = match (self.pptx_drag(), self.slide_layout()) {
                (Some(Drag::Vertex { part, index, handle, start, current }), Some(layout)) if moved(*start, *current) => {
                    let to = layout.to_emu(*current);
                    Some((*part, *index, *handle, (to.x as f64, to.y as f64)))
                }
                _ => None,
            };
            let nodes = self.edit_point_nodes_dragged(dragged);
            for (l, figure) in nodes.iter().enumerate() {
                for (i, n) in figure.iter().enumerate() {
                    let v = n.p;
                    let chosen = selected_vertex == Some((l, i));
                    handles.push(
                        div()
                            .absolute()
                            .left(px(v.0 as f32 * scale - HANDLE_SIZE / 2.0))
                            .top(px(v.1 as f32 * scale - HANDLE_SIZE / 2.0))
                            .size(px(HANDLE_SIZE))
                            .border_1()
                            .border_color(primary)
                            .bg(if chosen { primary } else { background })
                            .into_any_element(),
                    );
                }
            }
            // The curve's handles of the vertex selected: round.
            for (_, h) in self.edit_point_handles(dragged) {
                handles.push(
                    div()
                        .absolute()
                        .left(px(h.0 as f32 * scale - HANDLE_SIZE / 2.0))
                        .top(px(h.1 as f32 * scale - HANDLE_SIZE / 2.0))
                        .size(px(HANDLE_SIZE))
                        .rounded_full()
                        .border_1()
                        .border_color(primary)
                        .bg(background)
                        .into_any_element(),
                );
            }
        } else {
          for shape in &selected {
            let (frame, rotation) = self.preview(shape);
            handles.push(crate::shell::selection_handles::border_region(format!("pptx-frame-{}",shape.id.0),Bounds::new(point(px(frame.x as f32*scale),px(frame.y as f32*scale)),size(px(frame.width as f32*scale),px(frame.height as f32*scale))),rotation,cx.listener(|e,event:&MouseDownEvent,w,cx|{if let Some(edit)=e.text_editing_mut(){edit.is_new=false;}e.end_text_edit(w,cx);let mut event=event.clone();event.click_count=1;e.on_slide_down(&event,w,cx);})).into_any_element());
            if !slide.is_attached(shape.id) {
                let is_line = matches!(shape.kind, ibis_doc_model::ShapeKind::Line(_))
                    || matches!(shape.kind, ibis_doc_model::ShapeKind::Auto(ref a) if editing::is_connector_geometry(&a.geometry));
                let points=ResizeHandle::ALL.into_iter().filter(|h|!is_line||line_end_handle(shape,*h)).map(|handle|{let p=interaction::handle_point(frame,rotation,handle);(handle,point(px(p.x as f32*scale),px(p.y as f32*scale)))});
                let rotate=(!is_line).then(||{let p=interaction::rotation_handle_point(frame,rotation,interaction::ROTATE_HANDLE_PX as f64/scale as f64);point(px(p.x as f32*scale),px(p.y as f32*scale))});
                handles.extend(crate::shell::selection_handles::group(format!("pptx-selection-handles-{}",shape.id.0).into(),points,rotate,crate::shell::selection_handles::Options{rotation:true,angle:rotation},primary,if editing==Some(shape.id){rgb(0x80d4ff).into()}else{background}).into_iter().map(|(_,element)|element.into_any_element()));
            }
        }
        }
        // The caret (blinking is left out) and the selected text, painted
        // after the text so that they are where it was just painted.
        let mut selected = primary;
        selected.a = 0.25;
        let caret_color = theme.foreground;
        let marks = self.caret_marks();
        let fallback = self.caret_geometry(scale).map(|g| {
            let shows_caret = self.text_editing().is_some_and(|e| e.anchor == e.caret || e.marked.is_some());
            (shows_caret.then_some(g.caret), g.selection)
        });
        let pending = self.pptx().is_some_and(|s| s.pending_click.is_some());
        // The words the spell checker does not know: a red wavy line under them (while the text is edited).
        let misspelled = self.misspelled_ranges();
        let entity = cx.entity();
        let text_marks = (marks.is_some() || fallback.is_some()).then(|| {
            canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| {
                    if pending {
                        let entity = entity.clone();
                        cx.defer(move |cx| entity.update(cx, |this, cx| this.place_caret_at_pending_click(cx)));
                    }
                    let corners = |b: Bounds<Pixels>| {
                        let (x, y, w, h) = (f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height));
                        [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
                    };
                    let (caret, selection) = match marks.as_ref().and_then(|m| m.polygons(false)) {
                        Some(r) => r,
                        None => {
                            let Some((caret, selection)) = fallback.clone() else { return };
                            let shift = |b: Bounds<Pixels>| corners(Bounds::new(bounds.origin + b.origin, b.size));
                            (caret.map(shift), selection.into_iter().map(shift).collect())
                        }
                    };
                    // Turned with the text when it is turned.
                    let mut paint = |q: [(f32, f32); 4], color: Hsla| {
                        let points: Vec<Point<Pixels>> = q.iter().map(|&(x, y)| point(px(x), px(y))).collect();
                        if let Some(p) = polygon(&points, None, true) {
                            window.paint_path(p, color);
                        }
                    };
                    for q in selection {
                        paint(q, selected);
                    }
                    if let Some(c) = caret.filter(|_| crate::pptx::text_edit::caret_blink_visible()) {
                        paint(c, caret_color);
                    }
                    // 赤い波線: under each word (a word on two lines has a line under each part).
                    if let Some(marks) = marks.as_ref() {
                        for range in &misspelled {
                            let Some((_, quads)) = marks.with_selection(range.clone()).polygons(false) else { continue };
                            for q in quads {
                                // The underside of the rectangle (turned with the text when it is).
                                let (a, b) = (q[3], q[2]);
                                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                                let length = dx.hypot(dy);
                                if length < 1.0 {
                                    continue;
                                }
                                let (ux, uy) = (dx / length, dy / length);
                                let (nx, ny) = (-uy, ux);
                                let steps = (length / 2.5).ceil() as usize;
                                let wave = |i: usize, lift: f32| {
                                    let t = (i as f32 * 2.5).min(length);
                                    let up = if i % 2 == 0 { 0.0 } else { 1.6 };
                                    point(px(a.0 + ux * t - nx * (up + lift)), px(a.1 + uy * t - ny * (up + lift)))
                                };
                                let mut ribbon: Vec<Point<Pixels>> = (0..=steps).map(|i| wave(i, 1.0)).collect();
                                ribbon.extend((0..=steps).rev().map(|i| wave(i, 0.0)));
                                if let Some(p) = polygon(&ribbon, None, true) {
                                    window.paint_path(p, crate::shell::palette::doc(0xd93025));
                                }
                            }
                        }
                    }
                },
            )
            .absolute()
            .size_full()
        });
        // Cells selected for merging: shaded.
        let cell_range = self.text_editing().and_then(|e| Some((e.shape, e.cell?, e.cell_anchor?))).and_then(|(id, a, b)| {
            let sh = slide.shape(id)?;
            let ibis_doc_model::ShapeKind::Table(t) = &sh.kind else { return None };
            let heights = t.row_heights(sh.frame.height);
            let (r0, r1, c0, c1) = (a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1));
            let x0 = sh.frame.x + t.columns[..c0].iter().sum::<i64>();
            let x1 = sh.frame.x + t.columns[..=c1.min(t.columns.len() - 1)].iter().sum::<i64>();
            let y0 = sh.frame.y + heights[..r0].iter().sum::<i64>();
            let y1 = sh.frame.y + heights[..=r1.min(heights.len() - 1)].iter().sum::<i64>();
            let mut shade = primary;
            shade.a = 0.2;
            Some(div().absolute().left(px(x0 as f32 * scale)).top(px(y0 as f32 * scale)).w(px((x1 - x0) as f32 * scale)).h(px((y1 - y0) as f32 * scale)).bg(shade))
        });
        div().absolute().top_0().left_0().size_full().child(paths).children(handles).children(text_marks).children(cell_range).children(self.render_pptx_ink_preview(scale))
    }

    /// The border between two columns or two rows of the one selected
    /// table under `p` (window pixels), within a few pixels: (table, the
    /// column left of it, the row above it). Not for a turned table.
    fn table_border_under(&self, p: Point<Pixels>) -> Option<(ShapeId, Option<usize>, Option<usize>)> {
        let layout = self.slide_layout()?;
        let [shape] = self.selected_shapes()[..] else { return None };
        let ibis_doc_model::ShapeKind::Table(table) = &shape.kind else { return None };
        if shape.rotation.rem_euclid(21_600_000) != 0 {
            return None;
        }
        let q = layout.to_emu(p);
        let f = shape.frame;
        let near = 4.0 / layout.scale as f64;
        if (q.x as f64) < f.x as f64 - near || (q.x as f64) > f.right() as f64 + near || (q.y as f64) < f.y as f64 - near || (q.y as f64) > f.bottom() as f64 + near {
            return None;
        }
        let mut x = f.x;
        for (i, w) in table.columns.iter().enumerate().take(table.columns.len().saturating_sub(1)) {
            x += w;
            if ((q.x - x) as f64).abs() <= near {
                return Some((shape.id, Some(i), None));
            }
        }
        let mut y = f.y;
        let heights = table.row_heights(f.height);
        for (i, h) in heights.iter().enumerate() {
            y += h;
            if ((q.y - y) as f64).abs() <= near {
                return Some((shape.id, None, Some(i)));
            }
        }
        None
    }

    /// The handle under `p` (window pixels) of the one selected shape.
    fn handle_under(&self, p: Point<Pixels>) -> Option<(ShapeId, Option<ResizeHandle>)> {
        let layout = self.slide_layout()?;
        let [shape] = self.selected_shapes()[..] else { return None };
        if self.slide().is_attached(shape.id) {
            return None;
        }
        let q = P::new((f32::from(p.x - layout.origin.x) / layout.scale) as f64, (f32::from(p.y - layout.origin.y) / layout.scale) as f64);
        let near = |h: P| ((h.x - q.x).powi(2) + (h.y - q.y).powi(2)).sqrt() * layout.scale as f64 <= interaction::HANDLE_HIT_PX as f64;
        let is_line = matches!(shape.kind, ibis_doc_model::ShapeKind::Line(_))
            || matches!(shape.kind, ibis_doc_model::ShapeKind::Auto(ref a) if editing::is_connector_geometry(&a.geometry));
        if !is_line {
            let above = interaction::ROTATE_HANDLE_PX as f64 / layout.scale as f64;
            if near(interaction::rotation_handle_point(shape.frame, shape.rotation, above)) {
                return Some((shape.id, None));
            }
        }
        ResizeHandle::ALL
            .into_iter()
            .filter(|h| !is_line || line_end_handle(shape, *h))
            .find(|h| near(interaction::handle_point(shape.frame, shape.rotation, *h)))
            .map(|h| (shape.id, Some(h)))
    }

    pub(crate) fn set_drag(&mut self, drag: Option<Drag>) {
        if let Some(state) = self.pptx_mut() {
            state.drag = drag;
        }
    }

    fn on_slide_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel_dialog_is_modal() { cx.stop_propagation(); return; }
        // The control bar of a video or audio clip (動画・音声): its own press.
        if self.clip_bar_down(event, window, cx) {
            return;
        }
        let Some(layout) = self.slide_layout() else { return };
        let position = event.position;
        let tool = self.pptx_tool();
        if let Some(state) = self.pptx_mut() {
            state.shift = event.modifiers.shift;
        }
        // 描画: the pen, the highlighter and the eraser take the press.
        if self.pptx_ink_press(position, cx) {
            self.end_text_edit(window, cx);
            self.focus_handle.focus(window, cx);
            return;
        }
        // カスタム軌跡: a click is a point, two end the path.
        if self.is_drawing_path() {
            self.end_text_edit(window, cx);
            self.focus_handle.focus(window, cx);
            let emu = layout.to_emu(position);
            self.path_draw_click((emu.x as f64, emu.y as f64), event.click_count >= 2, cx);
            return;
        }
        if tool != Tool::Select {
            self.end_text_edit(window, cx);
            self.focus_handle.focus(window, cx);
            self.set_drag(Some(Drag::Place { tool, start: position, current: position, points: vec![position] }));
            return cx.notify();
        }
        // 頂点の編集: a vertex is dragged, an edge gets one, elsewhere ends it.
        if self.is_editing_points() {
            self.end_text_edit(window, cx);
            self.focus_handle.focus(window, cx);
            if let Some((part, index, handle)) = self.edit_points_down(position, cx) {
                self.set_drag(Some(Drag::Vertex { part, index, handle, start: position, current: position }));
                return cx.notify();
            }
            if self.is_editing_points() {
                return cx.notify();
            }
        }
        let p = P::of(layout.to_emu(position));
        let slop = interaction::LINE_SLOP_PX as f64 / layout.scale as f64;
        // ⌘ and a click on a link follows it (as PowerPoint's Ctrl+click).
        if event.click_count==1 && !event.modifiers.shift
            && let Some(link) = self.slide_link_at(layout.to_emu(position))
        {
            self.end_text_edit(window, cx);
            return self.slide_follow_link(&link, window, cx);
        }
        // A press in the text being edited places the caret (or extends
        // the selection with shift); two select a word, three a paragraph.
        // In a table being edited, Shift and a press in another cell select
        // the cells between (セルの結合); a press alone edits that cell.
        if let Some((shape, Some(current))) = self.text_editing().map(|e| (e.shape, e.cell))
            && let Some(cell) = self.slide().shape(shape).and_then(|s| interaction::table_cell_at(s, p))
            && cell != current
            && event.modifiers.shift
        {
            if let Some(e) = self.pptx_mut().and_then(|s| s.edit.as_mut()) {
                e.cell_anchor = Some(cell);
            }
            return cx.notify();
        }
        if let Some((shape, Some(current))) = self.text_editing().map(|e| (e.shape, e.cell))
            && let Some(cell) = self.slide().shape(shape).and_then(|s| interaction::table_cell_at(s, p))
            && cell != current
        {
            self.begin_table_cell_edit(shape, cell, window, cx);
            if let Some(state) = self.pptx_mut() {
                state.pending_click = Some(position);
            }
            return cx.notify();
        }
        if let Some((shape, handle)) = self.handle_under(position) {
            if self.text_editing().is_none_or(|e| e.shape != shape || handle.is_none()) {
                self.end_text_edit(window, cx);
                self.focus_handle.focus(window, cx);
            }
            let Some(s) = self.slide().shape(shape) else { return };
            let keep_aspect_on_corners = matches!(s.kind, ibis_doc_model::ShapeKind::Picture(_)) || self.slide().aspect_locked.contains(&shape);
            let drag = match handle {
                Some(handle) => Drag::Resize { shape, handle, start: position, current: position, origin: s.frame, rotation: s.rotation, keep_aspect_on_corners },
                None => {
                    let c = P::new(s.frame.x as f64 + s.frame.width as f64 / 2.0, s.frame.y as f64 + s.frame.height as f64 / 2.0);
                    let center = layout.origin + point(px(c.x as f32 * layout.scale), px(c.y as f32 * layout.scale));
                    Drag::Rotate { shape, start: position, current: position, origin: s.rotation, center }
                }
            };
            self.set_drag(Some(drag));
            return cx.notify();
        }
        if let Some(edit) = self.text_editing().map(|e| e.shape)
            && let Some(s) = self.slide().shape(edit)
        {
            let f = s.frame;
            let band = (6.0 / layout.scale).max(1.0) as i64;
            let px_emu = p.x as i64;
            let py_emu = p.y as i64;
            let in_outer = px_emu >= f.x - band && px_emu <= f.right() + band && py_emu >= f.y - band && py_emu <= f.bottom() + band;
            let in_inner = px_emu > f.x + band && px_emu < f.right() - band && py_emu > f.y + band && py_emu < f.bottom() - band;
            if in_outer && !in_inner {
                self.set_drag(Some(Drag::Move { start: position, current: position, origins: vec![(edit, f)] }));
                return cx.notify();
            }
        }
        if let Some(edit) = self.text_editing().map(|e| e.shape)
            && self.slide().shape(edit).is_some_and(|s| interaction::hits(s, p, slop))
        {
            if !event.modifiers.shift {
                if let Some(e) = self.pptx_mut().and_then(|s| s.edit.as_mut()) {
                    e.cell_anchor = None;
                }
            }
            if let Some(i) = self.text_index_at(position) {
                match event.click_count {
                    1 => {
                        self.shape_move_caret(i, event.modifiers.shift, cx);
                        self.set_drag(Some(Drag::Text { start: position, current: position }));
                    }
                    n => {
                        let (a, b) = self.text_unit_around(i, n >= 3);
                        self.shape_move_caret(a, false, cx);
                        self.shape_move_caret(b, true, cx);
                    }
                }
            }
            return cx.notify();
        }
        self.end_text_edit(window, cx);
        self.focus_handle.focus(window, cx);
        // A border inside the selected table: its columns' widths or rows' heights.
        if let Some((shape, column, row)) = self.table_border_under(position) {
            self.set_drag(Some(Drag::TableBorder { shape, column, row, start: position, current: position }));
            return cx.notify();
        }
        let hit = interaction::hit_test(self.slide(), p, slop);
        let shift = crate::shell::selection_handles::additive(event.modifiers);
        match hit {
            Some(id) => {
                // Two clicks on a table: its cell's text (セルの文字の編集).
                if event.click_count >= 2
                    && let Some(cell) = self.slide().shape(id).and_then(|s| interaction::table_cell_at(s, p))
                {
                    self.set_selection(vec![id]);
                    self.begin_table_cell_edit(id, cell, window, cx);
                    if let Some(state) = self.pptx_mut() {
                        state.pending_click = Some(position);
                    }
                    return;
                }
                if event.click_count >= 2 && self.slide().shape(id).is_some_and(|s| s.text_body().is_some()) {
                    self.set_selection(vec![id]);
                    self.begin_text_edit(id, false, false, window, cx);
                    // The caret goes where it was clicked, once the text is
                    // painted with its positions.
                    if let Some(state) = self.pptx_mut() {
                        state.pending_click = Some(position);
                    }
                    return;
                }
                let mut target_ids = vec![id];
                for g in &self.slide().groups {
                    let mut leaves = Vec::new();
                    Self::collect_group_leaves(g, &mut leaves);
                    if leaves.contains(&id) {
                        target_ids = leaves;
                        break;
                    }
                }
                let mut selection = self.selected_ids();
                if shift {
                    let all_in = target_ids.iter().all(|x| selection.contains(x));
                    if all_in {
                        selection.retain(|x| !target_ids.contains(x));
                    } else {
                        for &tid in &target_ids {
                            if !selection.contains(&tid) {
                                selection.push(tid);
                            }
                        }
                    }
                    self.set_selection(selection);
                    return cx.notify();
                }
                let already_selected = target_ids.iter().all(|x| selection.contains(x));
                if !already_selected {
                    self.set_selection(target_ids.clone());
                    selection = target_ids;
                }
                // Option held: the copies are what is dragged (as PowerPoint's Ctrl+drag).
                if event.modifiers.alt && self.duplicate_selected_in_place(cx) {
                    selection = self.selected_ids();
                }
                let origins = self.slide().shapes.iter().filter(|s| selection.contains(&s.id)).map(|s| (s.id, s.frame)).collect();
                self.set_drag(Some(Drag::Move { start: position, current: position, origins }));
            }
            None => {
                let before = if shift { self.selected_ids() } else { Vec::new() };
                if !shift {
                    self.set_selection(Vec::new());
                }
                self.set_drag(Some(Drag::Band { start: position, current: position, before }));
            }
        }
        cx.notify();
    }

    pub(crate) fn on_drag_move(&mut self, position: Point<Pixels>, shift: bool, cx: &mut Context<Self>) {
        if self.clip_seek_move(position, cx) {
            return;
        }
        if self.pptx_ink_move(position, cx) {
            return;
        }
        let Some(state) = self.pptx_mut() else { return };
        if state.drag.is_none() {
            return;
        }
        state.shift = shift;
        let text = matches!(state.drag, Some(Drag::Text { .. }));
        if let Some(drag) = state.drag.as_mut() {
            *drag.current_mut() = position;
            if let Drag::Place { points, .. } = drag {
                if points.last().map_or(true, |last| {
                    let dx = f32::from(last.x - position.x);
                    let dy = f32::from(last.y - position.y);
                    (dx * dx + dy * dy).sqrt() >= 2.0
                }) {
                    points.push(position);
                }
            }
            cx.notify();
        }
        // Keep the preview selection in sync during a rubber-band drag, using
        // the original selection so shrinking the band also removes targets.
        let band = match (self.pptx_drag(), self.slide_layout()) {
            (Some(Drag::Band { start, current, before }), Some(layout)) if moved(*start, *current) => {
                Some((EmuRect::from_corners(layout.to_emu(*start), layout.to_emu(*current)), before.clone()))
            }
            _ => None,
        };
        if let Some((band, mut selection)) = band {
            for id in interaction::in_band(self.slide(), band) {
                if !selection.contains(&id) { selection.push(id); }
            }
            self.set_selection(selection);
        }
        if text && let Some(i) = self.text_index_at(position) {
            self.shape_move_caret(i, true, cx);
        }
    }

    pub(crate) fn on_drag_end(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.clip_seek_end() {
            return cx.notify();
        }
        if self.pptx_ink_release(cx) {
            return;
        }
        let Some(drag) = self.pptx_mut().and_then(|s| s.drag.clone()) else { return };
        let Some(layout) = self.slide_layout() else { return self.set_drag(None) };
        let slide = self.current_slide;
        let shift = self.shift_held();
        let (start, current) = drag.start_current();
        match &drag {
            Drag::Text { .. } => self.set_drag(None),
            Drag::Vertex { part, index, handle, .. } => {
                self.set_drag(None);
                if moved(start, current) {
                    let to = layout.to_emu(current);
                    self.move_vertex(*part, *index, *handle, (to.x as f64, to.y as f64), cx);
                }
            }
            Drag::TableBorder { shape, column, row, .. } => {
                self.set_drag(None);
                let delta = layout.to_emu(current);
                let origin = layout.to_emu(start);
                let (dx, dy) = (delta.x - origin.x, delta.y - origin.y);
                self.move_table_border(*shape, *column, *row, dx, dy, cx);
            }
            Drag::Move { origins, .. } => {
                if moved(start, current) {
                    let mut commands: Vec<Command> = origins
                        .iter()
                        .filter_map(|(id, _)| self.slide().shape(*id).map(|s| (s, self.preview(s).0)))
                        .filter(|(s, frame)| *frame != s.frame)
                        .map(|(s, frame)| Command::SetFrame { slide, shape: s.id, frame })
                        .collect();
                    let current_slide = self.slide();
                    let updated_shapes: Vec<Shape> = current_slide.shapes.iter().map(|s| {
                        if let Some((_, frame)) = origins.iter().filter_map(|(id, _)| self.slide().shape(*id).map(|orig| (orig, self.preview(orig).0))).find(|(orig, _)| orig.id == s.id) {
                            Shape { frame, ..s.clone() }
                        } else {
                            s.clone()
                        }
                    }).collect();
                    let rerouted = ibis_doc_model::connector::rerouted_on_slide(&updated_shapes, &current_slide.shape_glues);
                    for (cid, frame, flip_h, flip_v) in rerouted {
                        commands.push(Command::SetConnectorFrame { slide, shape: cid, frame, flip_h, flip_v });
                    }
                    self.set_drag(None);
                    self.run(commands, cx);
                } else {
                    self.set_drag(None);
                    let at=layout.to_emu(current);
                    let clicked=interaction::hit_test(self.slide(),P::new(at.x as f64,at.y as f64),0.);
                    // A click on a member of a group selects the whole group (not its text), as the press does.
                    let members = clicked.map(|id| {
                        let mut leaves = vec![id];
                        for g in &self.slide().groups {
                            let mut all = Vec::new();
                            Self::collect_group_leaves(g, &mut all);
                            if all.contains(&id) {
                                leaves = all;
                                break;
                            }
                        }
                        leaves
                    });
                    if let Some((id, members)) = clicked.zip(members) {
                        let in_group = members.len() > 1;
                        self.set_selection(members);
                        let point=layout.to_emu(start);
                        let border=self.slide().shape(id).is_some_and(|s|crate::shell::selection_handles::on_border((s.frame.x as f32,s.frame.y as f32,s.frame.width as f32,s.frame.height as f32),s.rotation,(point.x as f32,point.y as f32),3./layout.scale));
                        if !border && !in_group && self.slide().shape(id).is_some_and(|s|matches!(s.kind,ibis_doc_model::ShapeKind::TextBox(_)|ibis_doc_model::ShapeKind::Auto(_))){
                            self.begin_text_edit(id,false,false,window,cx);
                            if let Some(state)=self.pptx_mut(){state.pending_click=Some(current);}
                        }
                    }

                }
            }
            Drag::Resize { shape, .. } => {
                let frame = self.slide().shape(*shape).map(|s| (s.frame, self.preview(s).0));
                self.set_drag(None);
                if let Some((old, new)) = frame
                    && old != new
                    && moved(start, current)
                {
                    let mut commands = vec![Command::SetFrame { slide, shape: *shape, frame: new }];
                    let current_slide = self.slide();
                    let updated_shapes: Vec<Shape> = current_slide.shapes.iter().map(|s| {
                        if s.id == *shape {
                            Shape { frame: new, ..s.clone() }
                        } else {
                            s.clone()
                        }
                    }).collect();
                    let rerouted = ibis_doc_model::connector::rerouted_on_slide(&updated_shapes, &current_slide.shape_glues);
                    for (cid, frame, flip_h, flip_v) in rerouted {
                        commands.push(Command::SetConnectorFrame { slide, shape: cid, frame, flip_h, flip_v });
                    }
                    self.run(commands, cx);
                } else if !moved(start, current) && self.text_editing().is_some_and(|e| e.shape == *shape) {
                    if let Some(e) = self.text_editing_mut() {
                        e.is_new = false;
                    }
                    self.end_text_edit(window, cx);
                    self.focus_handle.focus(window, cx);
                }
            }
            Drag::Rotate { shape, .. } => {
                let turn = self.slide().shape(*shape).map(|s| (s.clone(), self.preview(s).1));
                self.set_drag(None);
                if let Some((s, rotation)) = turn
                    && rotation != s.rotation
                {
                    self.run(vec![Command::SetTransform { slide, shape: *shape, rotation, flip_h: s.flip_h, flip_v: s.flip_v }], cx);
                }
            }
            Drag::Band { before, .. } => {
                let mut selection = before.clone();
                if moved(start, current) {
                    let band = EmuRect::from_corners(layout.to_emu(start), layout.to_emu(current));
                    for id in interaction::in_band(self.slide(), band) {
                        if !selection.contains(&id) {
                            selection.push(id);
                        }
                    }
                }
                self.set_selection(selection);
            }
            Drag::Place { tool, points, .. } => {
                let (mut a, mut b) = (layout.to_emu(start), layout.to_emu(current));
                if self.is_pptx_snap_to_grid_enabled() {
                    a = interaction::snap_point(a, interaction::GRID_INTERVAL);
                    b = interaction::snap_point(b, interaction::GRID_INTERVAL);
                }
                let dragged = moved(start, current);
                let id = self.document.ids().new_shape_id();
                let shape = match *tool {
                    Tool::TextBox => {
                        let frame = if dragged { interaction::drawn_frame(a, b, false) } else { self.presentation().default_text_box_frame(a) };
                        Shape::text_box(id, frame, "")
                    }
                    Tool::Shape(preset) => {
                        let frame = if dragged { interaction::drawn_frame(a, b, shift) } else { EmuRect::new(a.x, a.y, CLICK_SHAPE, CLICK_SHAPE) };
                        self.with_defaults(editing::new_shape(id, preset, frame))
                    }
                    Tool::Line { kind: editing::LineKind::Freehand, .. } => {
                        let emu_points: Vec<EmuPoint> = points.iter().map(|&p| layout.to_emu(p)).collect();
                        let (frame, local_pts) = if emu_points.len() >= 2 && dragged {
                            let mut min_x = i64::MAX;
                            let mut min_y = i64::MAX;
                            let mut max_x = i64::MIN;
                            let mut max_y = i64::MIN;
                            for p in &emu_points {
                                min_x = min_x.min(p.x);
                                min_y = min_y.min(p.y);
                                max_x = max_x.max(p.x);
                                max_y = max_y.max(p.y);
                            }
                            let w = (max_x - min_x).max(100);
                            let h = (max_y - min_y).max(100);
                            let frame = EmuRect::new(min_x, min_y, w, h);
                            let local = emu_points.iter().map(|p| (p.x - min_x, p.y - min_y)).collect();
                            (frame, local)
                        } else {
                            let w = CLICK_LINE;
                            let h = CLICK_LINE;
                            let frame = EmuRect::new(a.x, a.y, w, h);
                            let local = vec![(0, 0), (w, h)];
                            (frame, local)
                        };
                        self.with_defaults(editing::new_freehand_shape(id, frame, &local_pts, None))
                    }
                    Tool::Line { kind, arrow } => {
                        let end = if dragged { b } else { EmuPoint::new(a.x + CLICK_LINE, a.y + if kind == editing::LineKind::Straight { 0 } else { CLICK_LINE }) };
                        let snap_radius = 127_000;
                        let slide_ref = self.slide();
                        let (start_pt, start_cxn) = if let Some((target_id, site_idx, pt)) = ibis_doc_model::connector::nearest_site_on_slide(&slide_ref.shapes, a.x, a.y, snap_radius) {
                            (EmuPoint::new(pt.0, pt.1), Some((target_id, site_idx)))
                        } else {
                            (a, None)
                        };
                        let (end_pt, end_cxn) = if let Some((target_id, site_idx, pt)) = ibis_doc_model::connector::nearest_site_on_slide(&slide_ref.shapes, end.x, end.y, snap_radius) {
                            (EmuPoint::new(pt.0, pt.1), Some((target_id, site_idx)))
                        } else {
                            (end, None)
                        };
                        let (frame, flip_h, flip_v) = interaction::line_frame(start_pt, end_pt, shift && kind == editing::LineKind::Straight);
                        let shape = self.with_defaults(editing::new_connector(id, frame, flip_h, flip_v, kind, arrow));
                        if start_cxn.is_some() || end_cxn.is_some() {
                            let glue = ibis_doc_model::Glue { start: start_cxn, end: end_cxn };
                            let slide = self.current_slide;
                            self.set_drag(None);
                            if let Some(state) = self.pptx_mut() {
                                state.tool = Tool::Select;
                            }
                            self.insert_shape(shape, cx);
                            self.run(vec![ibis_doc_model::Command::SetShapeGlue { slide, shape: id, glue: Some(glue) }], cx);
                            cx.notify();
                            return;
                        }
                        shape
                    }
                    Tool::Select => return self.set_drag(None),
                };
                self.set_drag(None);
                if let Some(state) = self.pptx_mut() {
                    state.tool = Tool::Select;
                }
                self.insert_shape(shape, cx);
                // An action button that goes somewhere is a link to it.
                if let Tool::Shape(preset) = *tool
                    && let Some(link) = editing::action_link(preset)
                {
                    let slide = self.current_slide;
                    self.run(vec![ibis_doc_model::Command::SetShapeLink { slide, shape: id, link: Some(link) }], cx);
                }
                if *tool == Tool::TextBox {
                    self.begin_text_edit(id, true, false, window, cx);
                }
            }
        }
        self.set_drag(None);
        cx.notify();
    }

    /// Keys while a text is edited: the caret, selection, deleting,
    /// Enter, and the format shortcuts. Characters come through the text
    /// input (`text_edit.rs`), not here.
    pub(crate) fn on_text_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_drawing_path() {
            match event.keystroke.key.as_str() {
                "enter" => {
                    cx.stop_propagation();
                    self.finish_path_draw(cx);
                }
                "backspace" | "delete" => {
                    cx.stop_propagation();
                    self.path_draw_undo_point(cx);
                }
                _ => {}
            }
            return;
        }
        // The outline view's line being edited has the keys.
        if self.is_outline_editing() {
            if self.outline_key(event.keystroke.key.as_str(), &event.keystroke.modifiers, window, cx) {
                cx.stop_propagation();
            }
            return;
        }
        if self.text_editing().is_none() {
            return;
        }
        let k = &event.keystroke;
        let command = k.modifiers.platform || (k.modifiers.control && !cfg!(target_os = "macos"));
        let key = k.key.as_str();
        match key {
            "left" | "right" | "up" | "down" | "home" | "end" => {
                cx.stop_propagation();
                if matches!(key, "up" | "down") && !command {
                    self.move_caret_vertically(key, k.modifiers.shift, cx);
                } else if k.modifiers.alt && matches!(key, "left" | "right") {
                    // ⌥← ⌥→: by words.
                    if let Some(to) = self.word_target(key == "right") {
                        self.shape_move_caret(to, k.modifiers.shift, cx);
                    }
                } else {
                    // ⌘← ⌘→ go to the line's start and end (⌘↑ ⌘↓ to the text's), as in any Mac text.
                    let (key, whole) = match (key, command) {
                        ("left", true) => ("home", false),
                        ("right", true) => ("end", false),
                        (key, command) => (key, command),
                    };
                    if let Some(to) = self.caret_target(key, whole, k.modifiers.shift) {
                        self.shape_move_caret(to, k.modifiers.shift, cx);
                    }
                }
            }
            // The Mac's own text keys: ⌃A ⌃E line start and end, ⌃B ⌃F left and right, ⌃P ⌃N up and down,
            // ⌃D forward delete, ⌃H backspace, ⌃K delete to the line's end.
            "a" | "e" | "b" | "f" | "p" | "n" | "d" | "h" | "k" if crate::is_mac() && k.modifiers.control && !k.modifiers.platform && !k.modifiers.alt => {
                cx.stop_propagation();
                match key {
                    "d" => self.delete_text(true, cx),
                    "h" => self.delete_text(false, cx),
                    "k" => self.delete_to(true, true, cx),
                    _ => {
                        let target = match key {
                            "a" => "home",
                            "e" => "end",
                            "b" => "left",
                            "f" => "right",
                            "p" => "up",
                            _ => "down",
                        };
                        if matches!(target, "up" | "down") {
                            self.move_caret_vertically(target, k.modifiers.shift, cx);
                        } else if let Some(to) = self.caret_target(target, false, k.modifiers.shift) {
                            self.shape_move_caret(to, k.modifiers.shift, cx);
                        }
                    }
                }
            }
            "backspace" | "delete" => {
                cx.stop_propagation();
                if k.modifiers.alt || command {
                    // ⌥⌫: a word; ⌘⌫: to the start of the line.
                    self.delete_to(key == "delete", command, cx);
                } else {
                    self.delete_text(key == "delete", cx);
                }
            }
            "enter" if k.modifiers.shift => {
                cx.stop_propagation();
                self.insert_line_break(cx);
            }
            "enter" => {
                cx.stop_propagation();
                self.shape_type_text("\n", cx);
            }
            "escape" => {
                cx.stop_propagation();
                self.end_text_edit(window, cx);
            }
            "c" | "x" if command && !k.modifiers.alt => {
                cx.stop_propagation();
                self.copy_edited_text(key == "x", cx);
            }
            "v" if command && !k.modifiers.alt => {
                cx.stop_propagation();
                self.paste_edited_text(cx);
            }
            "a" if command => {
                cx.stop_propagation();
                let len = self.edited_text().map_or(0, |t| t.chars().count());
                self.shape_move_caret(0, false, cx);
                self.shape_move_caret(len, true, cx);
            }
            "b" | "i" | "u" if command => {
                cx.stop_propagation();
                if self.is_document() {
                    let change = match key {
                        "b" => ibis_doc_model::RunChange::Bold(true),
                        "i" => ibis_doc_model::RunChange::Italic(true),
                        _ => {
                            let cur_u = self.box_edit_current_format().and_then(|(f, _)| f.underline).unwrap_or(false);
                            ibis_doc_model::RunChange::Underline(!cur_u)
                        }
                    };
                    self.apply_box_edit_run_change(&change, cx);
                } else {
                    use ibis_doc_model::CharacterFormat;
                    match key {
                        "b" => self.toggle_characters(window, cx, |b, r| b.with_run(r).bold, |on| CharacterFormat { bold: Some(on), ..Default::default() }),
                        "i" => self.toggle_characters(window, cx, |b, r| b.with_run(r).italic, |on| CharacterFormat { italic: Some(on), ..Default::default() }),
                        _ => self.toggle_characters(window, cx, |_, r| r.underline, |on| CharacterFormat { underline: Some(on), ..Default::default() }),
                    }
                }
            }
            _ => {
                // A key the menus name as their shortcut does what the menu item does.
                if self.run_menu_shortcut(&event.keystroke, window, cx) {
                    cx.stop_propagation();
                }
            }
        }
    }

    /// The menu item whose shortcut is the key pressed (one with ⌘, ⌃ or ⌥, or a function key), run if it is
    /// enabled: every shortcut the menus show works, not only the ones bound one by one. `true` if one ran.
    pub(crate) fn run_menu_shortcut(&mut self, k: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let m = &k.modifiers;
        let function = k.key.len() >= 2 && k.key.starts_with('f') && k.key[1..].chars().all(|c| c.is_ascii_digit());
        if !(m.platform || m.control || m.alt || function) {
            return false;
        }
        let Some(want) = shortcut_of(k) else { return false };
        let menus = self.menus();
        let mut found = None;
        fn find(entries: &[crate::shell::chrome::MenuEntry], want: &(u8, String), found: &mut Option<crate::shell::chrome::Handler>) {
            use crate::shell::chrome::MenuEntry;
            for entry in entries {
                match entry {
                    MenuEntry::Item(item) if found.is_none() && item.enabled() && item.shortcut.and_then(parse_shortcut).as_ref() == Some(want) => *found = item.handler.clone(),
                    MenuEntry::Submenu(sub) => find(&sub.items, want, found),
                    _ => {}
                }
            }
        }
        for (_, entries) in &menus.menus {
            find(entries, &want, &mut found);
        }
        match found {
            Some(handler) => {
                handler(self, window, cx);
                true
            }
            None => false,
        }
    }

    fn on_slide_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.text_editing().is_some() {
            return;
        }
        let k = &event.keystroke;
        let command = k.modifiers.platform || (k.modifiers.control && !cfg!(target_os = "macos"));
        let has_selection = !self.selected_ids().is_empty();
        let arrow = match k.key.as_str() {
            "left" => Some((-1, 0)),
            "right" => Some((1, 0)),
            "up" => Some((0, -1)),
            "down" => Some((0, 1)),
            _ => None,
        };
        if let Some((dx, dy)) = arrow
            && !command
        {
            cx.stop_propagation();
            if has_selection {
                self.nudge_selected(dx, dy, k.modifiers.alt, cx);
            } else if dx + dy > 0 {
                self.next_slide(&crate::NextSlide, window, cx);
            } else {
                self.previous_slide(&crate::PreviousSlide, window, cx);
            }
            return;
        }
        match (command, k.modifiers.shift, k.key.as_str()) {
            (false, false, "enter") => {
                if let [shape] = self.selected_ids()[..]
                    && self.slide().shape(shape).is_some_and(|s| s.text_body().is_some())
                {
                    cx.stop_propagation();
                    self.begin_text_edit(shape, false, true, window, cx);
                }
            }
            (true, false, "a") => {
                cx.stop_propagation();
                self.select_all_shapes(window, cx);
            }
            (true, false, "d") => {
                cx.stop_propagation();
                self.duplicate_selected(cx);
            }
            (true, false, "c") if has_selection => {
                cx.stop_propagation();
                self.copy_selected();
                self.write_shapes_to_system_clipboard(cx);
                self.stamp_shape_clipboard(cx);
            }
            (true, false, "x") if has_selection => {
                cx.stop_propagation();
                self.cut_selected(cx);
            }
            (true, false, "v") => {
                if self.paste_from_clipboards(window, cx) {
                    cx.stop_propagation();
                }
            }
            _ => {
                if self.run_menu_shortcut(k, window, cx) {
                    cx.stop_propagation();
                }
            }
        }
    }
}

/// A shortcut as the menus write it (`⌥⇧⌘↑`): its modifiers (bits: ⌘ 1, ⌃ 2, ⌥ 4, ⇧ 8) and key.
pub(crate) fn parse_shortcut(label: &str) -> Option<(u8, String)> {
    let mut bits = 0u8;
    let mut key = String::new();
    for c in label.chars() {
        match c {
            '⌘' => bits |= 1,
            '⌃' => bits |= 2,
            '⌥' => bits |= 4,
            '⇧' => bits |= 8,
            c => key.extend(c.to_uppercase()),
        }
    }
    (!key.is_empty()).then_some((bits, key))
}

/// The key pressed in the same form as [`parse_shortcut`].
pub(crate) fn shortcut_of(k: &Keystroke) -> Option<(u8, String)> {
    let m = &k.modifiers;
    let bits = u8::from(m.platform) | u8::from(m.control) << 1 | u8::from(m.alt) << 2 | u8::from(m.shift) << 3;
    let key = match k.key.as_str() {
        "up" => "↑".to_string(),
        "down" => "↓".to_string(),
        "left" => "←".to_string(),
        "right" => "→".to_string(),
        "enter" => "↩".to_string(),
        "backspace" => "⌫".to_string(),
        "delete" => "⌦".to_string(),
        other if !other.is_empty() => other.to_uppercase(),
        _ => return None,
    };
    Some((bits, key))
}

/// Whether `handle` is at an end of a line (a line has only those two).
fn line_end_handle(shape: &Shape, handle: ResizeHandle) -> bool {
    let ends = match (shape.flip_h, shape.flip_v) {
        (false, false) | (true, true) => [ResizeHandle::TopLeft, ResizeHandle::BottomRight],
        _ => [ResizeHandle::TopRight, ResizeHandle::BottomLeft],
    };
    ends.contains(&handle)
}

/// The pointer's moves and release anywhere in the window reach the editor
/// while a drag goes on (the drag continues past the slide). Always in the
/// tree, so a drag begun since the last frame is followed too.
fn drag_tracker(cx: &Context<SlideEditor>) -> impl IntoElement {
    let editor = cx.entity().downgrade();
    canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            let moving = editor.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble {
                    let _ = moving.update(cx, |this, cx| this.on_drag_move(event.position, event.modifiers.shift, cx));
                }
            });
            let ending = editor.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                    let _ = ending.update(cx, |this, cx| this.on_drag_end(window, cx));
                }
            });
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_0()
}
