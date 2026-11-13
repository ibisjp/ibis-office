// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! トリミング on the page of a document: the document's side of the crop mode the screens share
//! (`shell/crop_mode.rs`). The picture at the caret is the target (`FormatEditor::crop_target`), the pointer is read on
//! the page the picture is on (`crop_pointer`), and the crop kept goes to the run of the picture (`crop_keep`). The
//! target is the document's own (`FlowTarget`, kept by the shared mode as `CropTarget::Own`).

use gpui_kit::*;
use ibis_doc_model::{Crop, EmuRect, EmuSize, FlowCommand, FlowPos, Shape, ShapeId, ShapeKind};

use crate::shell::crop_mode::{CropTarget, full_extent, kept_part};
use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// EMU in a point (a page of a document is laid out in points).
const EMU_PER_POINT: f64 = 12_700.;

/// A picture of a flowing document in crop mode: the run at `at`, drawn on page `page`.
struct FlowTarget {
    at: FlowPos,
    page: usize,
    /// Whether the page is the one of the upper pane of a split window.
    top_pane: bool,
    /// An inline picture takes its size from the run; an anchored one has a frame and an offset from its anchor (EMU).
    inline: bool,
    offset: (i64, i64),
    /// The frame it showed through when crop mode began (EMU of the page).
    frame: EmuRect,
    /// The picture as it is drawn (its effects and outline), to be drawn whole.
    drawn: Shape,
}

#[ibis_office_macros::editor_ext(PictureCrop)]
impl SlideEditor {
    /// The picture of a flowing document crop mode would work on: the one at the caret that is not turned or flipped,
    /// the place it has on its page, and its crop.
    pub(crate) fn flow_crop_target(&self) -> Option<(CropTarget, EmuRect, Crop)> {
        let (at, ..) = self.picture_at_caret()?;
        let look = self.picture_look_of(at)?;
        if look.rotation != 0 || look.flip_h || look.flip_v {
            return None;
        }
        let (page, (x, y, w, h), offset) = self.floating_text_box_frame_of(at)?;
        let emu = |points: f32| (f64::from(points) * EMU_PER_POINT).round() as i64;
        let frame = EmuRect { x: emu(x), y: emu(y), width: emu(w), height: emu(h) };
        let full = full_extent(frame, look.crop)?;
        let (_, inline) = self.is_picture_at(at);
        let drawn = self.flow_picture_shape(at)?;
        let target = FlowTarget { at, page, top_pane: self.doc_split_active(), inline, offset, frame, drawn };
        Some((CropTarget::Own(Box::new(target)), full, look.crop))
    }

    /// The crop and the kept part's size go to the picture of the document; an anchored picture whose left or top edge
    /// moved is put where the kept part is (as an offset from its anchor), one that keeps its left and top keeps its
    /// alignment. The picture stays selected.
    pub(crate) fn keep_flow_crop(&mut self, target: &CropTarget, full: EmuRect, crop: Crop, cx: &mut Context<Self>) {
        let Some(target) = target.own::<FlowTarget>() else { return };
        let Some(mut look) = self.picture_look_of(target.at) else { return };
        let kept = kept_part(full, crop);
        let mut commands = Vec::new();
        if look.crop != crop {
            look.crop = crop;
            commands.push(FlowCommand::SetPictureLook { at: target.at, look });
        }
        if kept.width != target.frame.width || kept.height != target.frame.height || kept.x != target.frame.x || kept.y != target.frame.y {
            let size = EmuSize::new(kept.width, kept.height);
            if target.inline {
                commands.push(FlowCommand::SetPictureSize { at: target.at, size });
            } else {
                let (dx, dy) = (kept.x - target.frame.x, kept.y - target.frame.y);
                let offset = (dx != 0 || dy != 0).then(|| (target.offset.0 + dx, target.offset.1 + dy));
                commands.push(FlowCommand::SetShapeGeometry { at: target.at, size: Some(size), offset });
            }
        }
        if commands.is_empty() {
            return;
        }
        let keep = self.text_selection();
        let keep_multi = self.doc_state().map(|s| s.selected_shapes.clone()).unwrap_or_default();
        if self.run_flow(commands, cx) && let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
            state.anchor = Some(a);
            state.caret = Some(c);
            state.selected_shapes = keep_multi;
            state.shape_selected = true;
        }
    }

    /// Where the pointer is, in EMU of the page the picture is on.
    pub(crate) fn flow_crop_pointer(&self, target: &CropTarget, position: Point<Pixels>) -> Option<(f64, f64)> {
        let target = target.own::<FlowTarget>()?;
        let (x, y) = self.page_point_pane(target.page, position, target.top_pane)?;
        Some((f64::from(x) * EMU_PER_POINT, f64::from(y) * EMU_PER_POINT))
    }

    /// The whole picture dimmed, the kept part in colour, the frame and its handles over page `page` of the document
    /// (`k` pixels to the point), when the picture being cropped is on that page.
    pub(crate) fn render_flow_crop_mode(&self, page: usize, k: f32, cx: &mut Context<Self>) -> Option<Div> {
        let target = self.own_crop_target::<FlowTarget>()?;
        if target.page != page {
            return None;
        }
        self.render_own_crop_mode(&target.drawn, k / EMU_PER_POINT as f32, cx)
    }

    /// The picture of the run at `at` as a shape (an inline picture's look is the shape's), to draw it whole.
    fn flow_picture_shape(&self, at: FlowPos) -> Option<Shape> {
        let paragraph = self.paragraph_of(at.para)?;
        let mut offset = 0;
        for run in &paragraph.runs {
            if offset == at.offset {
                use ibis_doc_model::FlowRunKind;
                return match &run.kind {
                    FlowRunKind::Shape(shape) if matches!(shape.kind, ShapeKind::Picture(_)) => Some((**shape).clone()),
                    FlowRunKind::Picture { media, size, look } | FlowRunKind::PictureControl { media, size, look, .. } => {
                        let mut picture = ibis_doc_model::Picture::new(*media);
                        picture.stroke = look.stroke.clone();
                        picture.duotone = look.duotone;
                        picture.luminance = look.luminance;
                        picture.soft_edge = look.soft_edge;
                        picture.geometry = look.geometry.clone();
                        picture.effects = look.effects.clone();
                        Some(Shape { id: ShapeId(0), frame: EmuRect { x: 0, y: 0, width: size.width, height: size.height }, rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Picture(picture), floating: None })
                    }
                    _ => None,
                };
            }
            offset += run.len();
        }
        None
    }
}
