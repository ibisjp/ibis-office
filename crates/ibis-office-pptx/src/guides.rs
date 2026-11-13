// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ガイド線 (表示 > ガイド): the guides of the normal view (`presentation.xml`) or, in the
//! slide master view, those of the master or layout shown (`p15:sldGuideLst`). They are
//! drawn over the slide, dragged to move (out of the slide to remove), added from the
//! グリッドとガイド menu, and the shapes dragged snap to them.
use ibis_doc_model::{Command, SlideGuide};
use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The colour of a guide the file gives none (PowerPoint's own grey for the guides drawn by hand).
const DEFAULT_COLOR: u32 = 0xa4a3a4;
/// How near (screen pixels) a shape's edge has to come to a guide to snap to it: PowerPoint's is 2 pixels of the screen
/// (measured at about 2.4 pt, 2026-11-09 on Windows at 100%).
pub(crate) const GUIDE_SNAP_PX: f32 = 2.0;
/// How near (screen pixels) a press has to be to a guide to take it.
const GRAB_PX: f32 = 4.0;

/// The step a guide moves by while dragged, from the slide's middle (EMU): 0.1 cm, or 0.01 cm with Alt held (and
/// the snap off), as PowerPoint's. The other units of the ruler keep the same proportions of their own unit.
pub(crate) fn guide_step_emu(unit: crate::shell::editor::RulerUnit, alt: bool) -> f64 {
    use crate::shell::editor::RulerUnit::*;
    let coarse = match unit {
        Centimeters | Millimeters => 36_000.0,
        Inches => 91_440.0,
        Points | Picas => 12_700.0,
    };
    if alt { coarse / 10.0 } else { coarse }
}

/// `coord` (EMU along the axis, from the slide's start) moved to the step grid measured from the middle `half`.
pub(crate) fn quantize_from_middle(coord: i64, half: i64, step: f64) -> i64 {
    half + (((coord - half) as f64 / step).round() * step).round() as i64
}

/// The drag (`d_x`, `d_y`, EMU) of the handle `handle` of a shape at `origin` (not turned), moved so that the edge the handle
/// moves lies on the guide (`xs` the vertical ones, `ys` the horizontal ones) within `slop` of it.
pub(crate) fn snap_resize_to_guides(origin: ibis_doc_model::EmuRect, handle: ibis_doc_model::ResizeHandle, d_x: f64, d_y: f64, xs: &[i64], ys: &[i64], slop: i64) -> (f64, f64) {
    use ibis_doc_model::ResizeHandle::*;
    let edge_x = match handle {
        TopLeft | Left | BottomLeft => Some(origin.x),
        TopRight | Right | BottomRight => Some(origin.right()),
        Top | Bottom => None,
    };
    let edge_y = match handle {
        TopLeft | Top | TopRight => Some(origin.y),
        BottomLeft | Bottom | BottomRight => Some(origin.bottom()),
        Left | Right => None,
    };
    let near = |edge: Option<i64>, d: f64, targets: &[i64]| -> f64 {
        let Some(edge) = edge else { return d };
        let at = edge as f64 + d;
        targets.iter().map(|t| *t as f64 - at).filter(|diff| diff.abs() <= slop as f64).min_by(|a, b| a.abs().total_cmp(&b.abs())).map_or(d, |diff| d + diff)
    };
    (near(edge_x, d_x, xs), near(edge_y, d_y, ys))
}

#[ibis_office_macros::editor_ext(Guides)]
impl SlideEditor {
    /// The guides shown: those of the page in the master view, else the normal view's.
    pub(crate) fn guide_list(&self) -> Vec<SlideGuide> {
        if self.is_master_view() { self.slide().guides.clone() } else { self.presentation().guides.clone() }
    }

    /// The guides the page shows without owning them: the layout's and its master's on a slide (so the same
    /// master guides show on every layout), the master's on a layout page of the master view.
    pub(crate) fn inherited_guides(&self) -> Vec<SlideGuide> {
        let presentation = self.presentation();
        let layout = self.slide().layout.and_then(|id| presentation.layouts.iter().find(|l| l.id == id));
        let Some(layout) = layout else { return Vec::new() };
        if self.is_master_view() {
            return if self.master_page_is_layout() { layout.master_guides.clone() } else { Vec::new() };
        }
        layout.master_guides.iter().chain(layout.guides.iter()).cloned().collect()
    }

    fn guide_owner(&self) -> Option<ibis_doc_model::SlideId> {
        self.is_master_view().then_some(self.current_slide)
    }

    fn set_guide_list(&mut self, guides: Vec<SlideGuide>, cx: &mut Context<Self>) {
        let slide = self.guide_owner();
        self.run(vec![Command::SetGuides { slide, guides }], cx);
    }

    pub(crate) fn guide_color(guide: &SlideGuide) -> u32 {
        guide.color.unwrap_or(DEFAULT_COLOR)
    }

    /// The guides are shown: with none in the file, PowerPoint's two through the middle of the slide are made.
    pub(crate) fn seed_guides(&mut self, cx: &mut Context<Self>) {
        if !self.guide_list().is_empty() {
            return;
        }
        let size = self.presentation().size_of(self.slide());
        let mid = |len: i64| SlideGuide::pos_from_emu(len / 2);
        self.set_guide_list(
            vec![
                SlideGuide { id: 1, horizontal: true, pos: mid(size.height), color: Some(0xfbae40), user_drawn: false },
                SlideGuide { id: 2, horizontal: false, pos: mid(size.width), color: Some(0xfbae40), user_drawn: false },
            ],
            cx,
        );
    }

    /// ガイドを追加: a guide (`horizontal`, or vertical) a half inch from the middle, where none is yet.
    pub(crate) fn add_guide(&mut self, horizontal: bool, cx: &mut Context<Self>) {
        let mut guides = self.guide_list();
        let size = self.presentation().size_of(self.slide());
        let mid = SlideGuide::pos_from_emu(if horizontal { size.height } else { size.width } / 2);
        // Half an inch (36 pt = 288 units) steps off the middle until the place is free.
        let pos = (0..).map(|n: i32| mid + n * 288).find(|p| !guides.iter().any(|g| g.horizontal == horizontal && g.pos == *p)).unwrap_or(mid);
        let id = guides.iter().map(|g| g.id).max().unwrap_or(0) + 1;
        guides.push(SlideGuide { id, horizontal, pos, color: None, user_drawn: true });
        if self.pptx().is_some() {
            self.shape_edit.guides = true;
        }
        self.set_guide_list(guides, cx);
    }

    /// すべてのガイドを削除.
    pub(crate) fn remove_all_guides(&mut self, cx: &mut Context<Self>) {
        if !self.guide_list().is_empty() {
            self.set_guide_list(Vec::new(), cx);
        }
    }

    /// The guide under `position` (a point of the window), if guides are shown.
    pub(crate) fn guide_at(&self, position: Point<Pixels>) -> Option<usize> {
        if !self.is_pptx_guides_enabled() {
            return None;
        }
        let layout = self.slide_layout()?;
        let at = layout.to_emu(position);
        let slop = layout.to_emu_len(px(GRAB_PX));
        // The guides run past the slide's edges, so a press anywhere along them takes one.
        self.guide_list()
            .iter()
            .enumerate()
            .map(|(i, g)| (i, (if g.horizontal { at.y } else { at.x } - g.emu()).abs()))
            .filter(|(_, d)| *d <= slop)
            .min_by_key(|(_, d)| *d)
            .map(|(i, _)| i)
    }

    /// Where a guide dragged to the pointer (`at`) lies (EMU along its axis), on the step grid from the middle of the slide.
    fn guide_coord(&self, horizontal: bool, at: ibis_doc_model::EmuPoint, alt: bool) -> (i64, i64) {
        let size = self.presentation().size_of(self.slide());
        let (coord, limit) = if horizontal { (at.y, size.height) } else { (at.x, size.width) };
        (quantize_from_middle(coord, limit / 2, guide_step_emu(self.ruler_unit, alt)), limit)
    }

    /// Guide `index` put where the pointer is (`to`): moved, or removed when it is off the slide.
    pub(crate) fn drop_guide(&mut self, index: usize, to: Point<Pixels>, alt: bool, cx: &mut Context<Self>) {
        let Some(layout) = self.slide_layout() else { return };
        let mut guides = self.guide_list();
        let Some(guide) = guides.get(index).cloned() else { return };
        let (coord, limit) = self.guide_coord(guide.horizontal, layout.to_emu(to), alt);
        if coord < 0 || coord > limit {
            guides.remove(index);
        } else {
            let pos = SlideGuide::pos_from_emu(coord);
            if pos == guide.pos {
                return;
            }
            guides[index].pos = pos;
            guides[index].user_drawn = true;
        }
        self.set_guide_list(guides, cx);
    }

    /// The positions (EMU) of the guides to snap to, by direction: the x of the vertical ones, the y of the horizontal ones.
    pub(crate) fn guide_snap_targets(&self) -> (Vec<i64>, Vec<i64>) {
        if !self.is_pptx_guides_enabled() {
            return (Vec::new(), Vec::new());
        }
        // The page's own guides, and those it shows from its layout and master.
        let guides: Vec<SlideGuide> = self.guide_list().into_iter().chain(self.inherited_guides()).collect();
        (guides.iter().filter(|g| !g.horizontal).map(SlideGuide::emu).collect(), guides.iter().filter(|g| g.horizontal).map(SlideGuide::emu).collect())
    }

    /// Where guide `index` is drawn (EMU along its axis), with a drag of it in progress.
    pub(crate) fn guide_drawn_at(&self, index: usize, guide: &SlideGuide) -> Option<i64> {
        if let (Some(super::slide_view::Drag::Guide { index: dragged, current, alt, .. }), Some(layout)) = (self.shape_edit.drag.as_ref(), self.slide_layout())
            && *dragged == index
        {
            let (coord, limit) = self.guide_coord(guide.horizontal, layout.to_emu(*current), *alt);
            return (0..=limit).contains(&coord).then_some(coord);
        }
        Some(guide.emu())
    }

    /// The number shown by the guide being dragged: its distance from the middle of the slide in the ruler's unit
    /// (two decimals, no sign), and the arrow of the side it is on: left or above the middle `arrow_left`, `arrow_drop_up`
    /// (drawn before the number, above it), right or below `arrow_right`, `arrow_drop_down` (after it, below it), as PowerPoint's.
    pub(crate) fn guide_drag_label(&self) -> Option<(String, &'static str)> {
        let Some(super::slide_view::Drag::Guide { index, .. }) = self.shape_edit.drag.as_ref() else { return None };
        let guide = self.guide_list().get(*index)?.clone();
        let coord = self.guide_drawn_at(*index, &guide)?;
        Some(guide_label(coord, self.presentation().size_of(self.slide()), guide.horizontal, self.ruler_unit))
    }
}

/// The label of a guide at `coord` (EMU from the slide's start): see [`SlideEditor::guide_drag_label`].
pub(crate) fn guide_label(coord: i64, size: ibis_doc_model::EmuSize, horizontal: bool, unit: crate::shell::editor::RulerUnit) -> (String, &'static str) {
    let half = (if horizontal { size.height } else { size.width }) / 2;
    let d = (coord - half) as f64 / unit.emu_per_unit() as f64;
    let text = format!("{:.2}", d.abs());
    let icon = match (horizontal, d < 0.0) {
        (false, false) => "arrow_right",
        (false, true) => "arrow_left",
        (true, false) => "arrow_drop_down",
        (true, true) => "arrow_drop_up",
    };
    (text, icon)
}


