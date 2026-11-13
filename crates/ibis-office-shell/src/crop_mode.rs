// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! トリミング on the slide (WYSIWYG): the selected picture shown whole, the part kept in full colour inside a
//! frame with black corner and edge handles, the rest dimmed. Dragging a handle moves a side of the frame; dragging
//! the picture moves it under the frame. Enter or a press outside keeps the crop (the frame becomes the kept part, the
//! picture keeps its scale, as in Office), Escape gives it up. Shared by the presentation and workbook screens, whose
//! pictures are shapes of the same model (`ShapeKind::Picture`); a picture that is turned or flipped is cropped with
//! the crop panel's numbers only.
//!
//! The document screen shares it: a picture of a flowing document (an inline picture, or an anchored one) is cropped on
//! its page the same way. The model of the drag is in EMU in the coordinates of the place the picture is on (a slide, or
//! a page), so only three things differ by the target (`CropTarget`): where the pointer is, how the picture is drawn
//! whole, and the commands that keep the crop. Which picture is the target, where the pointer is on it and how the crop
//! is kept are the format's to say (`FormatEditor::crop_target`, `crop_pointer`, `crop_keep`): a slide's and a sheet's
//! are here (the trait's defaults), a document's in `docx/picture_crop.rs`. What a document's target is (the run of the
//! picture, its page, its pane) is not known here: it is kept as the screen's own (`CropTarget::Own`) and handed back.

use gpui_kit::*;
use crate::palette::rgb;
use ibis_doc_model::{Command, Crop, EmuRect, ResizeHandle, Shape, ShapeId, ShapeKind, SlideId};

use crate::editor::SlideEditor;
use crate::format::with_format;

/// `Crop` is in 100 000ths of the picture's side.
const WHOLE: f64 = 100_000.;
/// At least this much of a side stays (1 %), so the frame never closes up.
const MIN_KEPT: f64 = WHOLE / 100.;
/// The side of a corner or edge handle, in pixels.
const HANDLE: f32 = 10.;

#[derive(Clone, Copy)]
struct CropDrag {
    /// The handle held; `None` when the picture itself is dragged under the frame.
    handle: Option<ResizeHandle>,
    /// Where the pointer went down, in EMU of the place the picture is on.
    start: (f64, f64),
    from: Crop,
}

/// What the crop mode works on.
pub enum CropTarget {
    /// A picture shape of a slide or a sheet.
    Slide { shape: ShapeId, slide: SlideId },
    /// A picture the screen keeps in its own way (a document's: the run of the picture, `docx/picture_crop.rs`). What
    /// the screen made in `FormatEditor::crop_target` comes back to it in `crop_pointer` and `crop_keep`; it draws the
    /// overlay where the picture is with `render_own_crop_mode`.
    Own(Box<dyn std::any::Any>),
}

impl CropTarget {
    /// The screen's own target, when this is one of type `T`.
    pub fn own<T: 'static>(&self) -> Option<&T> {
        match self {
            CropTarget::Own(target) => target.downcast_ref(),
            CropTarget::Slide { .. } => None,
        }
    }
}

/// The crop mode of a picture, while it is on.
pub(crate) struct CropMode {
    target: CropTarget,
    /// The whole picture's place on the slide: it does not move while the crop changes.
    full: EmuRect,
    crop: Crop,
    drag: Option<CropDrag>,
}

/// The place the whole picture takes on the slide, given the frame it shows through (`frame`) and the crop it has.
pub fn full_extent(frame: EmuRect, crop: Crop) -> Option<EmuRect> {
    let kept_w = 1. - (f64::from(crop.left) + f64::from(crop.right)) / WHOLE;
    let kept_h = 1. - (f64::from(crop.top) + f64::from(crop.bottom)) / WHOLE;
    if kept_w <= 0. || kept_h <= 0. {
        return None;
    }
    let (w, h) = (frame.width as f64 / kept_w, frame.height as f64 / kept_h);
    let x = frame.x as f64 - f64::from(crop.left) / WHOLE * w;
    let y = frame.y as f64 - f64::from(crop.top) / WHOLE * h;
    Some(EmuRect { x: x.round() as i64, y: y.round() as i64, width: w.round() as i64, height: h.round() as i64 })
}

/// The part of the picture kept (the frame it shows through), given the whole picture's place and the crop.
pub fn kept_part(full: EmuRect, crop: Crop) -> EmuRect {
    let (w, h) = (full.width as f64, full.height as f64);
    let x = full.x as f64 + f64::from(crop.left) / WHOLE * w;
    let y = full.y as f64 + f64::from(crop.top) / WHOLE * h;
    let kept_w = w * (1. - (f64::from(crop.left) + f64::from(crop.right)) / WHOLE);
    let kept_h = h * (1. - (f64::from(crop.top) + f64::from(crop.bottom)) / WHOLE);
    EmuRect { x: x.round() as i64, y: y.round() as i64, width: kept_w.round() as i64, height: kept_h.round() as i64 }
}

/// The frame a picture takes when its crop changes from `old` to `new` (the menu's and the panel's cropping): the part of
/// the picture that stays shows where it was, at the size it was, so the frame shrinks (or grows back) by what is cut (or
/// given back) and the picture is not stretched. A turned picture keeps its place on the slide (the frame's centre moves
/// along the turned axes); a flipped one cuts the opposite side on the slide. `None` when a crop leaves nothing.
pub(crate) fn recropped_frame(frame: EmuRect, rotation: i32, flip_h: bool, flip_v: bool, old: Crop, new: Crop) -> Option<EmuRect> {
    let seen = |c: Crop| Crop {
        left: if flip_h { c.right } else { c.left },
        right: if flip_h { c.left } else { c.right },
        top: if flip_v { c.bottom } else { c.top },
        bottom: if flip_v { c.top } else { c.bottom },
    };
    let (old, new) = (seen(old), seen(new));
    let full = full_extent(frame, old)?;
    if f64::from(new.left) + f64::from(new.right) >= WHOLE || f64::from(new.top) + f64::from(new.bottom) >= WHOLE {
        return None;
    }
    let kept = kept_part(full, new);
    // The centre of the kept part, from the old frame's centre, in the picture's own (unturned) axes.
    let (dx, dy) = ((kept.x as f64 + kept.width as f64 / 2.) - (frame.x as f64 + frame.width as f64 / 2.), (kept.y as f64 + kept.height as f64 / 2.) - (frame.y as f64 + frame.height as f64 / 2.));
    let angle = f64::from(rotation) / 60_000. * std::f64::consts::PI / 180.;
    let (sx, sy) = (dx * angle.cos() - dy * angle.sin(), dx * angle.sin() + dy * angle.cos());
    let (cx, cy) = (frame.x as f64 + frame.width as f64 / 2. + sx, frame.y as f64 + frame.height as f64 / 2. + sy);
    Some(EmuRect { x: (cx - kept.width as f64 / 2.).round() as i64, y: (cy - kept.height as f64 / 2.).round() as i64, width: kept.width, height: kept.height })
}

/// The crop after a drag of `(dx, dy)` EMU from `from`: a handle moves the sides it is on (never to cut a side away or
/// to add space); no handle moves the picture under the frame (the crop shifts from one side to the opposite one).
pub(crate) fn dragged(full: EmuRect, from: Crop, handle: Option<ResizeHandle>, dx: f64, dy: f64) -> Crop {
    let (fx, fy) = (dx / full.width.max(1) as f64 * WHOLE, dy / full.height.max(1) as f64 * WHOLE);
    let (l, t, r, b) = (f64::from(from.left), f64::from(from.top), f64::from(from.right), f64::from(from.bottom));
    let clamp_side = |value: f64, other: f64| value.clamp(0., (WHOLE - MIN_KEPT - other).max(0.));
    let round = |v: f64| v.round() as i32;
    match handle {
        Some(h) => {
            let left = if h.moves_left() { clamp_side(l + fx, r) } else { l };
            let right = if h.moves_right() { clamp_side(r - fx, left) } else { r };
            let top = if h.moves_top() { clamp_side(t + fy, b) } else { t };
            let bottom = if h.moves_bottom() { clamp_side(b - fy, top) } else { b };
            Crop { left: round(left), top: round(top), right: round(right), bottom: round(bottom) }
        }
        None => {
            // Moving the picture right by `fx` cuts less of its left and more of its right.
            let (sx, sy) = (fx.clamp(-r, l), fy.clamp(-b, t));
            Crop { left: round(l - sx), top: round(t - sy), right: round(r + sx), bottom: round(b + sy) }
        }
    }
}

impl SlideEditor {
    /// Whether the selection is one picture that crop mode can work on (not turned, not flipped).
    pub fn can_crop_mode(&self) -> bool {
        with_format!(self, crop_target(self)).is_some()
    }

    /// The picture of a slide or a sheet crop mode would work on: the one shape selected, when it is a picture that is
    /// not turned or flipped; the place the whole picture has, and its crop.
    pub(crate) fn slide_crop_target(&self) -> Option<(CropTarget, EmuRect, Crop)> {
        let shapes = self.selected_shapes();
        let [shape] = shapes.as_slice() else { return None };
        if shape.rotation != 0 || shape.flip_h || shape.flip_v {
            return None;
        }
        let ShapeKind::Picture(picture) = &shape.kind else { return None };
        let full = full_extent(shape.frame, picture.crop)?;
        Some((CropTarget::Slide { shape: shape.id, slide: self.current_slide }, full, picture.crop))
    }

    pub fn is_crop_mode(&self) -> bool {
        self.crop_mode.is_some()
    }

    /// トリミング: the selected picture shown for cropping on the slide (or the page).
    pub fn begin_crop_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_crop_mode() {
            return;
        }
        self.end_text_edit(window, cx);
        let Some((target, full, crop)) = with_format!(self, crop_target(self)) else { return };
        self.crop_mode = Some(CropMode { target, full, crop, drag: None });
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Leave crop mode: with `keep`, the picture takes the crop and its frame the kept part (one change to undo).
    pub fn finish_crop_mode(&mut self, keep: bool, cx: &mut Context<Self>) {
        let Some(mode) = self.crop_mode.take() else { return };
        if keep {
            with_format!(self, crop_keep(self, &mode.target, mode.full, mode.crop, cx));
        }
        cx.notify();
    }

    /// The crop and the kept part's frame go to the picture shape of the slide or the sheet.
    pub(crate) fn keep_slide_crop(&mut self, target: &CropTarget, full: EmuRect, crop: Crop, cx: &mut Context<Self>) {
        let CropTarget::Slide { shape, slide } = target else { return };
        let (slide, shape_id): (SlideId, ShapeId) = (*slide, *shape);
        let target = self.presentation().slide(slide).and_then(|s| s.shape(shape_id)).cloned();
        if let Some(shape) = target && let ShapeKind::Picture(picture) = &shape.kind {
            let frame = kept_part(full, crop);
            let mut commands = Vec::new();
            if picture.crop != crop {
                let mut picture = picture.clone();
                picture.crop = crop;
                commands.push(Command::SetKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Picture(picture)) });
            }
            if shape.frame != frame {
                commands.push(Command::SetFrame { slide, shape: shape_id, frame });
            }
            if !commands.is_empty() {
                self.run(commands, cx);
            }
        }
    }

    /// The key of crop mode: Enter keeps, Escape gives up. True when taken.
    pub(crate) fn crop_mode_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        if self.crop_mode.is_none() || self.dialog.active {
            return false;
        }
        match key.key.as_str() {
            "enter" => self.finish_crop_mode(true, cx),
            "escape" => self.finish_crop_mode(false, cx),
            _ => return false,
        }
        true
    }

    /// Where the pointer is, in EMU of the slide or of the page the picture is on.
    fn pointer_emu(&self, position: Point<Pixels>) -> Option<(f64, f64)> {
        let mode = self.crop_mode.as_ref()?;
        with_format!(self, crop_pointer(self, &mode.target, position))
    }

    /// Where the pointer is, in EMU of the slide or the sheet.
    pub(crate) fn slide_crop_pointer(&self, position: Point<Pixels>) -> Option<(f64, f64)> {
        let layout = self.slide_layout()?;
        Some((layout.to_emu_len(position.x) as f64, layout.to_emu_len(position.y) as f64))
    }

    /// A press on a handle (`Some`) or on the picture (`None`) in crop mode.
    pub(crate) fn crop_down(&mut self, handle: Option<ResizeHandle>, event: &MouseDownEvent, cx: &mut Context<Self>) {
        cx.stop_propagation();
        let start = self.pointer_emu(event.position);
        if let (Some(mode), Some(start)) = (self.crop_mode.as_mut(), start) {
            mode.drag = Some(CropDrag { handle, start, from: mode.crop });
        }
        cx.notify();
    }

    /// The pointer moved with a drag of crop mode going on. True when taken.
    pub fn crop_pointer_move(&mut self, position: Point<Pixels>, pressed: bool, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.crop_mode.as_ref().and_then(|m| m.drag) else { return false };
        if !pressed {
            return self.crop_pointer_up(cx);
        }
        let Some(now) = self.pointer_emu(position) else { return true };
        let (dx, dy) = (now.0 - drag.start.0, now.1 - drag.start.1);
        if let Some(mode) = self.crop_mode.as_mut() {
            mode.crop = dragged(mode.full, drag.from, drag.handle, dx, dy);
        }
        cx.notify();
        true
    }

    /// The button released: the drag ends. True when there was one.
    pub fn crop_pointer_up(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(mode) = self.crop_mode.as_mut() else { return false };
        let had = mode.drag.take().is_some();
        if had {
            cx.notify();
        }
        had
    }

    /// The crop now (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub fn crop_mode_crop(&self) -> Option<Crop> {
        self.crop_mode.as_ref().map(|m| m.crop)
    }

    /// Whether a drag is going on, and by which handle (`None` inside: the picture itself) (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub fn crop_mode_drag_handle(&self) -> Option<Option<ResizeHandle>> {
        self.crop_mode.as_ref().and_then(|m| m.drag).map(|d| d.handle)
    }

    /// The place of the kept part on the slide or page (for the scenarios).
    #[cfg(feature = "scenarios")]
    pub fn crop_mode_kept(&self) -> Option<EmuRect> {
        self.crop_mode.as_ref().map(|m| kept_part(m.full, m.crop))
    }

    /// The whole picture dimmed, the kept part in colour, the frame and its black handles, over the slide.
    pub fn render_crop_mode(&self, scale: f32, cx: &mut Context<Self>) -> Option<Div> {
        let mode = self.crop_mode.as_ref()?;
        let CropTarget::Slide { shape, slide } = &mode.target else { return None };
        let shape = self.presentation().slide(*slide)?.shape(*shape)?;
        let ShapeKind::Picture(_) = &shape.kind else { return None };
        let mut whole = shape.clone();
        whole.frame = mode.full;
        if let ShapeKind::Picture(p) = &mut whole.kind {
            p.crop = Crop::default();
        }
        let full = mode.full;
        Some(crop_overlay(full, mode.crop, scale, &|| crate::render::shape_view::shape_element(&whole, full, scale, &self.resources, None), cx))
    }

    /// The target of crop mode, when it is one the screen keeps in its own way and of type `T`.
    pub fn own_crop_target<T: 'static>(&self) -> Option<&T> {
        self.crop_mode.as_ref()?.target.own()
    }

    /// The same over a picture the screen keeps in its own way (`CropTarget::Own`): `drawn` is the picture as a shape
    /// (its effects and outline), shown whole at the place crop mode has for it; `scale` is pixels to the EMU of the
    /// place the overlay fills (a page of a document).
    pub fn render_own_crop_mode(&self, drawn: &Shape, scale: f32, cx: &mut Context<Self>) -> Option<Div> {
        let mode = self.crop_mode.as_ref()?;
        let mut whole = drawn.clone();
        whole.frame = mode.full;
        if let ShapeKind::Picture(p) = &mut whole.kind {
            p.crop = Crop::default();
        }
        let full = mode.full;
        Some(crop_overlay(full, mode.crop, scale, &|| crate::render::shape_view::shape_element(&whole, full, scale, &self.resources, None), cx))
    }
}

/// The whole picture dimmed (`image` draws it at its place), the kept part over it in colour, the frame and its black
/// handles. `scale` is pixels to the EMU; the places are in EMU of the container the overlay fills. Presses on the
/// handles and on the picture go to `crop_down`.
fn crop_overlay(full: EmuRect, crop: Crop, scale: f32, image: &dyn Fn() -> AnyElement, cx: &mut Context<SlideEditor>) -> Div {
    let at = |emu: i64| px(emu as f32 * scale);
    let kept = kept_part(full, crop);
    // The whole picture on white, dimmed; the kept part over it in full colour.
    let dimmed = div().absolute().left(at(full.x)).top(at(full.y)).w(at(full.width)).h(at(full.height)).bg(rgb(0xffffff))
        .child(div().absolute().size_full().opacity(0.4).child(div().absolute().left(at(-full.x)).top(at(-full.y)).child(image())));
    let window = div().id("crop-mode-picture").absolute().left(at(kept.x)).top(at(kept.y)).w(at(kept.width)).h(at(kept.height)).overflow_hidden().bg(rgb(0xffffff))
        .cursor(CursorStyle::OpenHand)
        .on_mouse_down(MouseButton::Left, cx.listener(|e, event: &MouseDownEvent, _, cx| e.crop_down(None, event, cx)))
        .child(div().absolute().left(at(-kept.x)).top(at(-kept.y)).child(image()));
    let frame = div().absolute().left(at(kept.x)).top(at(kept.y)).w(at(kept.width)).h(at(kept.height)).border_1().border_color(rgb(0x000000));
    let handles = ResizeHandle::ALL.map(|handle| {
        let (ax, ay) = handle.anchor();
        let (cx_, cy_) = (kept.x as f32 + kept.width as f32 * ax as f32 / 2., kept.y as f32 + kept.height as f32 * ay as f32 / 2.);
        let cursor = match handle {
            ResizeHandle::TopLeft | ResizeHandle::BottomRight => CursorStyle::ResizeUpLeftDownRight,
            ResizeHandle::TopRight | ResizeHandle::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
            ResizeHandle::Top | ResizeHandle::Bottom => CursorStyle::ResizeUpDown,
            _ => CursorStyle::ResizeLeftRight,
        };
        // Black, thick on the corners (an L) and on the edges (a bar), as in Office's crop handles.
        let (w, h) = match handle {
            ResizeHandle::Top | ResizeHandle::Bottom => (HANDLE * 2., HANDLE / 2.),
            ResizeHandle::Left | ResizeHandle::Right => (HANDLE / 2., HANDLE * 2.),
            _ => (HANDLE, HANDLE),
        };
        div().id(SharedString::from(format!("crop-handle-{ax}-{ay}"))).absolute().left(px(cx_ * scale - w / 2.)).top(px(cy_ * scale - h / 2.)).w(px(w)).h(px(h))
            .bg(rgb(0x000000)).border_1().border_color(rgb(0xffffff)).cursor(cursor)
            .on_mouse_down(MouseButton::Left, cx.listener(move |e, event: &MouseDownEvent, _, cx| e.crop_down(Some(handle), event, cx)))
    });
    div().absolute().size_full().top_0().left_0().child(dimmed).child(window).child(frame).children(handles)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i64, y: i64, width: i64, height: i64) -> EmuRect {
        EmuRect { x, y, width, height }
    }

    #[core::prelude::v1::test]
    fn the_whole_picture_is_recovered_from_its_frame_and_crop() {
        // A picture cropped 10 % left and 30 % right shows 60 % of its width through a 600 000 EMU frame at x = 1 000 000.
        let crop = Crop { left: 10_000, top: 0, right: 30_000, bottom: 20_000 };
        let frame = rect(1_000_000, 500_000, 600_000, 400_000);
        let full = full_extent(frame, crop).unwrap();
        assert_eq!(full, rect(900_000, 500_000, 1_000_000, 500_000));
        assert_eq!(kept_part(full, crop), frame);
        assert!(full_extent(frame, Crop { left: 60_000, right: 40_000, ..Default::default() }).is_none());
    }

    #[core::prelude::v1::test]
    fn a_handle_moves_its_sides_and_keeps_some_of_the_picture() {
        let full = rect(0, 0, 1_000_000, 500_000);
        // The left edge 100 000 EMU right: 10 % of the width cut.
        assert_eq!(dragged(full, Crop::default(), Some(ResizeHandle::Left), 100_000., 0.), Crop { left: 10_000, ..Default::default() });
        // The bottom-right corner 250 000 left and 50 000 up: 25 % off the right and 10 % off the bottom.
        assert_eq!(dragged(full, Crop::default(), Some(ResizeHandle::BottomRight), -250_000., -50_000.), Crop { right: 25_000, bottom: 10_000, ..Default::default() });
        // Beyond the edge: nothing added; past the opposite side: 1 % stays.
        assert_eq!(dragged(full, Crop::default(), Some(ResizeHandle::Left), -90_000., 0.), Crop::default());
        let far = dragged(full, Crop { right: 50_000, ..Default::default() }, Some(ResizeHandle::Left), 9_000_000., 0.);
        assert_eq!(far, Crop { left: 49_000, right: 50_000, ..Default::default() });
    }

    #[core::prelude::v1::test]
    fn dragging_the_picture_shifts_the_crop_between_opposite_sides() {
        let full = rect(0, 0, 1_000_000, 500_000);
        let from = Crop { left: 20_000, top: 10_000, right: 30_000, bottom: 10_000 };
        // The picture 100 000 EMU right and 50 000 down: 10 % less cut on the left, 10 % more on the right; the same on the height.
        assert_eq!(dragged(full, from, None, 100_000., 50_000.), Crop { left: 10_000, top: 0, right: 40_000, bottom: 20_000 });
        // It cannot be pulled past the cut that is left on a side.
        assert_eq!(dragged(full, from, None, 900_000., 0.), Crop { left: 0, top: 10_000, right: 50_000, bottom: 10_000 });
    }
}

#[cfg(test)]
mod recrop_tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn what_stays_is_not_moved_or_stretched() {
        let frame = EmuRect { x: 1_000_000, y: 2_000_000, width: 4_000_000, height: 2_000_000 };
        for (rotation, flip_h) in [(0, false), (0, true), (5_400_000, false), (2_000_000, true)] {
            let old = Crop { left: 10_000, top: 5_000, right: 20_000, bottom: 0 };
            let new = Crop { left: 30_000, top: 0, right: 5_000, bottom: 25_000 };
            let before = full_extent(frame, if flip_h { Crop { left: old.right, right: old.left, ..old } } else { old }).unwrap();
            let f = recropped_frame(frame, rotation, flip_h, false, old, new).unwrap();
            let seen = if flip_h { Crop { left: new.right, right: new.left, ..new } } else { new };
            let after = full_extent(f, seen).unwrap();
            // The whole picture has the same size; unturned, the same place.
            assert!((after.width - before.width).abs() <= 2 && (after.height - before.height).abs() <= 2, "{rotation} {flip_h}: {before:?} {after:?}");
            if rotation == 0 {
                assert!((after.x - before.x).abs() <= 2 && (after.y - before.y).abs() <= 2, "{before:?} {after:?}");
            }
        }
    }
}
