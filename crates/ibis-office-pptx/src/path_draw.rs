// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! カスタム軌跡 (アニメーション > 軌跡 > カスタム軌跡): the points of a path
//! clicked on the slide, from the selected shape's middle; a double click or
//! Enter ends it and adds the 軌跡 effect (a line between the points, as the
//! slide show plays it), Backspace takes the last point away, Esc cancels.
use gpui_kit::*;
use ibis_doc_model::{AnimationClass, AnimationLook, Command, ShapeId};

use crate::animation_ui::effect;

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The shape that will move and the points clicked, in slide EMU.
#[derive(Clone, Debug)]
pub(crate) struct PathDraw {
    pub(crate) shape: ShapeId,
    pub(crate) points: Vec<(f64, f64)>,
}

#[ibis_office_macros::editor_ext(PathDrawExt)]
impl SlideEditor {
    pub(crate) fn is_drawing_path(&self) -> bool {
        self.pptx().is_some_and(|p| p.path_draw.is_some())
    }

    /// Whether the selection is one shape a path can be drawn for.
    pub(crate) fn can_draw_path(&self) -> bool {
        self.selected_ids().len() == 1
    }

    /// The middle of the shape the path starts from (slide EMU).
    pub(crate) fn path_draw_start(&self) -> Option<(f64, f64)> {
        let id = self.pptx()?.path_draw.as_ref()?.shape;
        let f = self.slide().shape(id)?.frame;
        Some((f.x as f64 + f.width as f64 / 2.0, f.y as f64 + f.height as f64 / 2.0))
    }

    /// The points clicked so far (slide EMU).
    pub(crate) fn path_draw_points(&self) -> Vec<(f64, f64)> {
        self.pptx().and_then(|p| p.path_draw.as_ref()).map(|d| d.points.clone()).unwrap_or_default()
    }

    /// カスタム軌跡: begin, for the selected shape.
    pub(crate) fn begin_path_draw(&mut self, cx: &mut Context<Self>) {
        if !self.can_draw_path() {
            return;
        }
        let Some(&shape) = self.selected_ids().first() else { return };
        if let Some(s) = self.pptx_mut() {
            s.path_draw = Some(PathDraw { shape, points: Vec::new() });
        }
        self.set_status(crate::shell::i18n::text("pptx.path-draw.custom-path-click-slide-place").to_string(), cx);
        cx.notify();
    }

    pub(crate) fn cancel_path_draw(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.path_draw = None;
        }
        cx.notify();
    }

    /// A click at `to` (slide EMU) while the path is drawn: a point; `done` (a double click) ends it.
    pub(crate) fn path_draw_click(&mut self, to: (f64, f64), done: bool, cx: &mut Context<Self>) {
        if let Some(d) = self.pptx_mut().and_then(|s| s.path_draw.as_mut()) {
            // The second click of a double click is on the point the first made.
            if !(done && d.points.last().is_some_and(|l| (l.0 - to.0).hypot(l.1 - to.1) < 1.0e5)) {
                d.points.push(to);
            }
        }
        if done {
            self.finish_path_draw(cx);
        } else {
            cx.notify();
        }
    }

    /// Backspace: the last point taken away.
    pub(crate) fn path_draw_undo_point(&mut self, cx: &mut Context<Self>) {
        if let Some(d) = self.pptx_mut().and_then(|s| s.path_draw.as_mut()) {
            d.points.pop();
        }
        cx.notify();
    }

    /// The path ended: a 軌跡 effect with these points, after the slide's other effects.
    pub(crate) fn finish_path_draw(&mut self, cx: &mut Context<Self>) {
        let Some(draw) = self.pptx_mut().and_then(|s| s.path_draw.take()) else { return };
        cx.notify();
        let Some(start) = (|| {
            let f = self.slide().shape(draw.shape)?.frame;
            Some((f.x as f64 + f.width as f64 / 2.0, f.y as f64 + f.height as f64 / 2.0))
        })() else {
            return;
        };
        if draw.points.is_empty() {
            return;
        }
        let size = self.presentation().slide_size;
        let (w, h) = (size.width.max(1) as f64, size.height.max(1) as f64);
        let path: Vec<(i32, i32)> = draw.points.iter().map(|p| (((p.0 - start.0) / w * 100_000.0).round() as i32, ((p.1 - start.1) / h * 100_000.0).round() as i32)).collect();
        let Some(&(x, y)) = path.last() else { return };
        let slide = self.current_slide;
        let mut animations = self.slide().animations.clone();
        let mut effect = effect(draw.shape, AnimationClass::Path, AnimationLook::Motion(x, y));
        effect.path = if path.len() == 1 { Vec::new() } else { path };
        animations.push(effect);
        self.run(vec![Command::SetAnimations { slide, animations }], cx);
    }
}
