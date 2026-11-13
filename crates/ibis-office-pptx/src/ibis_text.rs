// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibisOffice's own text features PowerPoint does not have (`ibis_doc_model::ibis_text`): text wrap round pictures and
//! videos (GitHub issue #132) and linked text boxes (#131).
//!
//! A file uses them only once the user said yes to the question "この機能は PowerPoint にない機能です。有効にしますか？"
//! (`SlideEditor::require_ibis_extensions`); the answer is kept in the file.

use ibis_doc_model::{Command, Emu, ObjectWrap, ObjectWrapKind, ShapeId, ShapeKind};

use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// Space kept round an object, millimetres (the choices of 余白).
const MARGINS_MM: [(&str, Emu); 4] = [("pptx.wrap.margin-0", 0), ("pptx.wrap.margin-2", 2 * 36_000), ("pptx.wrap.margin-5", 5 * 36_000), ("pptx.wrap.margin-10", 10 * 36_000)];

#[ibis_office_macros::editor_ext(IbisText)]
impl SlideEditor {
    /// The pictures and videos selected.
    fn wrap_targets(&self) -> Vec<ShapeId> {
        self.selected_shapes().iter().filter(|s| matches!(s.kind, ShapeKind::Picture(_))).map(|s| s.id).collect()
    }

    /// The wrap of the first selected picture or video (what the menu shows as chosen).
    pub(crate) fn selected_wrap(&self) -> ObjectWrap {
        let slide = self.slide();
        self.wrap_targets().first().and_then(|id| slide.ibis_wrap.get(id).copied()).unwrap_or(ObjectWrap::none())
    }

    /// Wrap text round the selected pictures and videos as `kind` says (`None` keeps what they have) with `margin` round them
    /// (`None` keeps theirs). Asks first, in a file that has not turned the features on.
    pub(crate) fn set_selected_wrap(&mut self, kind: Option<ObjectWrapKind>, margin: Option<Emu>, cx: &mut gpui_kit::Context<Self>) {
        let targets = self.wrap_targets();
        if targets.is_empty() || (kind == Some(ObjectWrapKind::None) && !self.ibis_extensions_on()) {
            return;
        }
        self.require_ibis_extensions(
            move |this, cx| {
                let slide = this.slide().id;
                let commands: Vec<Command> = targets
                    .iter()
                    .map(|&shape| {
                        let now = this.slide().ibis_wrap.get(&shape).copied().unwrap_or(ObjectWrap::none());
                        Command::SetObjectWrap { slide, shape, wrap: ObjectWrap { kind: kind.unwrap_or(now.kind), margins: margin.map_or(now.margins, |m| [m; 4]) } }
                    })
                    .collect();
                this.run(commands, cx);
            },
            cx,
        );
    }

    /// The text boxes among the selected shapes (those with text).
    fn text_box_targets(&self) -> Vec<ShapeId> {
        self.selected_shapes().iter().filter(|s| s.text_body().is_some()).map(|s| s.id).collect()
    }

    /// Whether the selected text boxes ignore the wrap round pictures (all of them do).
    pub(crate) fn selected_ignore_wrap(&self) -> bool {
        let targets = self.text_box_targets();
        !targets.is_empty() && targets.iter().all(|id| self.slide().ibis_wrap_ignore.contains(id))
    }

    /// 回り込みを無視する on the selected text boxes: on when not all of them have it, else off.
    pub(crate) fn toggle_selected_ignore_wrap(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let targets = self.text_box_targets();
        if targets.is_empty() {
            return;
        }
        let ignore = !self.selected_ignore_wrap();
        if ignore && !self.ibis_extensions_on() {
            self.require_ibis_extensions(move |this, cx| this.set_ignore_wrap(&targets, true, cx), cx);
        } else {
            self.set_ignore_wrap(&targets, ignore, cx);
        }
    }

    fn set_ignore_wrap(&mut self, targets: &[ShapeId], ignore: bool, cx: &mut gpui_kit::Context<Self>) {
        let slide = self.slide().id;
        self.run(targets.iter().map(|&shape| Command::SetWrapIgnore { slide, shape, ignore }).collect(), cx);
    }
}

/// テキストの回り込み ▾ (なし／四角形／上下, 余白) of the figure format menu and the right-click menu; disabled with no picture
/// or video selected.
pub(crate) fn wrap_menu(editor: &SlideEditor, enabled: bool) -> MenuEntry {
    let now = editor.selected_wrap();
    let mut items: Vec<MenuEntry> = [("pptx.wrap.none", ObjectWrapKind::None), ("pptx.wrap.square", ObjectWrapKind::Square), ("pptx.wrap.top-bottom", ObjectWrapKind::TopBottom)]
        .into_iter()
        .map(|(id, kind)| MenuItem::tr(id).disabled(!enabled).checked(enabled && now.kind == kind).on(move |e, _, cx| e.set_selected_wrap(Some(kind), None, cx)).into())
        .collect();
    items.push(MenuEntry::Separator);
    items.extend(MARGINS_MM.into_iter().map(|(id, margin)| -> MenuEntry {
        MenuItem::tr(id).disabled(!enabled || now.kind == ObjectWrapKind::None).checked(enabled && now.kind != ObjectWrapKind::None && now.margins == [margin; 4]).on(move |e, _, cx| e.set_selected_wrap(None, Some(margin), cx)).into()
    }));
    Submenu::tr("pptx.wrap.menu", "wrap_text", items).into()
}

/// 回り込みを無視する: the text box's own switch, checked when its text goes under the pictures.
pub(crate) fn wrap_ignore_item(editor: &SlideEditor) -> MenuEntry {
    MenuItem::tr("pptx.wrap.ignore").disabled(editor.text_box_targets().is_empty()).checked(editor.selected_ignore_wrap()).on(|e, _, cx| e.toggle_selected_ignore_wrap(cx)).into()
}

// ----- Linked text boxes (#131) -------------------------------------------------------------------------------------

#[ibis_office_macros::editor_ext(IbisText2)]
impl SlideEditor {
    /// 連結を作成: the text of `from` (and its chain) goes on in `to` (a text box not in a chain; what it holds is added at the end
    /// of the text). Asks first in a file that has not turned the features on.
    pub(crate) fn link_text_boxes(&mut self, from: ShapeId, to: ShapeId, cx: &mut gpui_kit::Context<Self>) {
        if from == to || !self.is_chainable(from) || !self.is_chainable(to) || self.slide().chain_of(to).is_some() {
            return;
        }
        // The link goes after the last box of the chain.
        if let Some(boxes) = self.chain_boxes_of(from)
            && boxes.last() != Some(&from)
        {
            return;
        }
        self.require_ibis_extensions(
            move |this, cx| {
                let (mut boxes, mut full) = match (this.chain_boxes_of(from), this.slide().chain_of(from).cloned()) {
                    (Some(boxes), chain) => {
                        let full = this.chain_text(&boxes, chain.as_ref());
                        (boxes, full)
                    }
                    _ => (vec![from], this.chain_text(&[from], None)),
                };
                boxes.push(to);
                if let Some(body) = this.slide().shape(to).and_then(|s| s.text_body()).filter(|b| !b.is_empty()) {
                    full.extend(body.paragraphs.iter().cloned());
                }
                if let Some(command) = this.flow_command(&boxes, full) {
                    this.run(vec![command], cx);
                }
            },
            cx,
        );
    }

    /// 連結を作成 with the mouse: the next click on a text box is the box the text goes on in.
    pub(crate) fn begin_link_mode(&mut self, from: ShapeId, cx: &mut gpui_kit::Context<Self>) {
        if !self.is_chainable(from) {
            return;
        }
        self.require_ibis_extensions(
            move |this, cx| {
                if this.pptx().is_some() {
                    this.shape_edit.link_mode = Some(from);
                }
                cx.notify();
            },
            cx,
        );
    }

    /// 連結を解除: the link between `shape` and the box after it is broken. Each part keeps the text its boxes hold (a chain of one box
    /// is no chain).
    pub(crate) fn unlink_text_box(&mut self, shape: ShapeId, cx: &mut gpui_kit::Context<Self>) {
        let Some(chain) = self.slide().chain_of(shape).cloned() else { return };
        let Some(at) = chain.position(shape).filter(|&i| i + 1 < chain.boxes.len()) else { return };
        let mut chains: Vec<_> = self.slide().text_chains.iter().filter(|c| **c != chain).cloned().collect();
        let part = |range: std::ops::Range<usize>, joined_first: bool| {
            let mut joined = chain.joined[range.clone()].to_vec();
            if !joined_first && let Some(j) = joined.first_mut() {
                *j = false;
            }
            ibis_doc_model::TextChain { boxes: chain.boxes[range.clone()].to_vec(), joined, padding: chain.padding[range].to_vec() }
        };
        for c in [part(0..at + 1, true), part(at + 1..chain.boxes.len(), false)] {
            if c.boxes.len() >= 2 {
                chains.push(c);
            }
        }
        let slide = self.slide().id;
        self.run(vec![Command::SetTextChains { slide, chains, bodies: Vec::new() }], cx);
    }

    /// 次のボックスへ／前のボックスへ: selects the box after (or before) `shape` in its chain.
    pub(crate) fn select_chain_neighbor(&mut self, shape: ShapeId, next: bool, cx: &mut gpui_kit::Context<Self>) {
        let Some(chain) = self.slide().chain_of(shape) else { return };
        let Some(at) = chain.position(shape) else { return };
        let target = if next { chain.boxes.get(at + 1) } else { at.checked_sub(1).and_then(|i| chain.boxes.get(i)) };
        if let Some(&id) = target {
            self.set_selection(vec![id]);
            cx.notify();
        }
    }

    /// Whether the text of `shape` does not all fit its box (the last box of a chain, or a box of no chain).
    pub(crate) fn box_overflows(&self, shape: &ibis_doc_model::Shape) -> bool {
        let slide = self.slide();
        if slide.chain_of(shape.id).is_some_and(|c| c.boxes.last() != Some(&shape.id)) {
            return false;
        }
        if !matches!(shape.kind, ShapeKind::TextBox(_) | ShapeKind::Auto(_)) {
            return false;
        }
        let (Some(body), Some((width, capacity))) = (shape.text_body(), crate::shell::text_chains::chain_space(shape)) else { return false };
        if body.is_empty() {
            return false;
        }
        let text = self.resources.window_text_system();
        let mut used = 0.0f32;
        for (i, p) in body.paragraphs.iter().enumerate() {
            let m = self.resources.measure_paragraph(body, p, width, i == 0, false, 1.0, None, &text);
            used += if i == 0 { 0.0 } else { m.before } + m.lines.iter().map(|l| l.2).sum::<f32>() + m.after;
        }
        used > capacity + 0.5
    }

    /// What the canvas shows of linked text boxes: for the selected text boxes, the red mark at the bottom edge of one whose text does not
    /// all show (click: 連結を作成 begins), and thin lines between the boxes of a chain that has a box selected.
    pub(crate) fn render_chain_marks(&self, scale: f32, cx: &mut gpui_kit::Context<Self>) -> Vec<gpui_kit::AnyElement> {
        use gpui_kit::*;
        let slide = self.slide();
        let selected = self.selected_shapes();
        let mut out: Vec<AnyElement> = Vec::new();
        let mut segments: Vec<((f32, f32), (f32, f32))> = Vec::new();
        for shape in &selected {
            if let Some(chain) = slide.chain_of(shape.id) {
                let boxes: Vec<&ibis_doc_model::Shape> = chain.boxes.iter().filter_map(|id| slide.shape(*id)).collect();
                for pair in boxes.windows(2) {
                    let seg = chain_link_segment(pair[0], pair[1], |s| self.preview(s), scale);
                    if !segments.contains(&seg) {
                        segments.push(seg);
                    }
                }
            }
            if self.is_chainable(shape.id) && self.box_overflows(shape) {
                let id = shape.id;
                // (Below the bottom edge near its right end: the middle of the edge is the handle's.)
                let (x, y) = ((shape.frame.x + shape.frame.width) as f32 * scale - 24.0, (shape.frame.y + shape.frame.height) as f32 * scale + 12.0);
                out.push(
                    div()
                        .id(("pptx-chain-overflow", id.0 as usize))
                        .absolute()
                        .left(px(x - 7.0))
                        .top(px(y - 7.0))
                        .w(px(14.0))
                        .h(px(14.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(white())
                        .border_1()
                        .border_color(rgb(0xd93025))
                        .text_color(rgb(0xd93025))
                        .text_size(px(11.0))
                        .cursor_pointer()
                        .child("+")
                        .on_mouse_down(MouseButton::Left, cx.listener(move |e, _, _, cx| {
                            e.begin_link_mode(id, cx);
                            cx.stop_propagation();
                        }))
                        .into_any_element(),
                );
            }
        }
        if !segments.is_empty() {
            out.push(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        for ((x0, y0), (x1, y1)) in &segments {
                            paint_dashed_line(window, bounds.origin, (*x0, *y0), (*x1, *y1), rgb(0x1a73e8).into());
                        }
                    },
                )
                .absolute()
                .size_full()
                .into_any_element(),
            );
        }
        out
    }
}

/// A dashed line of the least width (one pixel) from `a` to `b` (pixels from `origin`), as the link line of a chain and the frames of
/// the boxes of a chain that do not hold the caret are drawn.
pub(crate) fn paint_dashed_line(window: &mut gpui_kit::Window, origin: gpui_kit::Point<gpui_kit::Pixels>, a: (f32, f32), b: (f32, f32), color: gpui_kit::Hsla) {
    use gpui_kit::*;
    const WIDTH: f32 = 1.0;
    const DASH: f32 = 4.0;
    const GAP: f32 = 3.0;
    let ((x0, y0), (x1, y1)) = (a, b);
    let (dx, dy) = (x1 - x0, y1 - y0);
    let len = dx.hypot(dy).max(1.0);
    let (ux, uy) = (dx / len, dy / len);
    let (nx, ny) = (-uy * WIDTH / 2.0, ux * WIDTH / 2.0);
    let at = |x: f32, y: f32| point(origin.x + px(x), origin.y + px(y));
    let mut from = 0.0f32;
    while from < len {
        let to = (from + DASH).min(len);
        let (ax, ay, bx, by) = (x0 + ux * from, y0 + uy * from, x0 + ux * to, y0 + uy * to);
        let quad = [at(ax + nx, ay + ny), at(bx + nx, by + ny), at(bx - nx, by - ny), at(ax - nx, ay - ny)];
        if let Some(path) = crate::render::shape_view::polygon(&quad, None, true) {
            window.paint_path(path, color);
        }
        from += DASH + GAP;
    }
}

#[ibis_office_macros::editor_ext(ChainFrames)]
impl SlideEditor {
    /// The frames of a chain while the caret is in one of its boxes: each box of the chain with whether its frame is solid (the box
    /// holding the caret) or dashed (the others), blue both. Empty when no box of a chain is being edited.
    pub(crate) fn chain_frame_styles(&self) -> Vec<(ShapeId, bool)> {
        let Some(edit) = self.text_editing() else { return Vec::new() };
        let Some(boxes) = self.chain_boxes_of(edit.shape) else { return Vec::new() };
        boxes.into_iter().map(|id| (id, id == edit.shape)).collect()
    }
}

/// 連結 submenu of the format and right-click menus: 連結を作成 / 連結を解除 / 次のボックスへ / 前のボックスへ.
pub(crate) fn chain_menu(editor: &SlideEditor) -> MenuEntry {
    let one = match editor.selected_shapes().as_slice() {
        [s] if s.text_body().is_some() => Some(s.id),
        _ => None,
    };
    let chain = one.and_then(|id| editor.slide().chain_of(id));
    let position = one.zip(chain).and_then(|(id, c)| c.position(id));
    let has_next = position.zip(chain).is_some_and(|(i, c)| i + 1 < c.boxes.len());
    let has_prev = position.is_some_and(|i| i > 0);
    let can_link = one.is_some_and(|id| chain.is_none_or(|c| c.boxes.last() == Some(&id)));
    let items: Vec<MenuEntry> = vec![
        MenuItem::tr("pptx.chain.link").icon("link").disabled(!can_link).on(move |e, _, cx| { if let Some(id) = one { e.begin_link_mode(id, cx); } }).into(),
        MenuItem::tr("pptx.chain.unlink").icon("close").disabled(!has_next).on(move |e, _, cx| { if let Some(id) = one { e.unlink_text_box(id, cx); } }).into(),
        MenuItem::tr("pptx.chain.next").icon("arrow_forward").disabled(!has_next).on(move |e, _, cx| { if let Some(id) = one { e.select_chain_neighbor(id, true, cx); } }).into(),
        MenuItem::tr("pptx.chain.previous").icon("arrow_back").disabled(!has_prev).on(move |e, _, cx| { if let Some(id) = one { e.select_chain_neighbor(id, false, cx); } }).into(),
    ];
    Submenu::tr("pptx.chain.menu", "link", items).into()
}

/// The line that shows a link of a chain, in pixels at `scale`: from the bottom right corner of the box the text leaves to the top left
/// corner of the box it goes on in; in vertical text (the lines go from the right to the left), from the bottom left corner to the top right
/// one. `frame_of` gives a box's frame and rotation as drawn now (so the line follows a box that is being dragged); a turned box has its
/// corner turned with it.
pub(crate) fn chain_link_segment(from: &ibis_doc_model::Shape, to: &ibis_doc_model::Shape, frame_of: impl Fn(&ibis_doc_model::Shape) -> (ibis_doc_model::EmuRect, i32), scale: f32) -> ((f32, f32), (f32, f32)) {
    let vertical = from.text_body().is_some_and(|b| b.vertical);
    let corner = |s: &ibis_doc_model::Shape, right: bool, bottom: bool| {
        let (f, rotation) = frame_of(s);
        let (cx, cy) = (f.x as f64 + f.width as f64 / 2.0, f.y as f64 + f.height as f64 / 2.0);
        let (px_, py_) = (f.x as f64 + if right { f.width as f64 } else { 0.0 } - cx, f.y as f64 + if bottom { f.height as f64 } else { 0.0 } - cy);
        let turn = (rotation as f64 / 60_000.0).to_radians();
        let (sin, cos) = turn.sin_cos();
        (((cx + px_ * cos - py_ * sin) as f32) * scale, ((cy + px_ * sin + py_ * cos) as f32) * scale)
    };
    if vertical { (corner(from, false, true), corner(to, true, false)) } else { (corner(from, true, true), corner(to, false, false)) }
}
