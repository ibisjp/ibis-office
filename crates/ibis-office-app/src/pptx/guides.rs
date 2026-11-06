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

/// The colour of a guide the file gives none (PowerPoint's own grey for the guides drawn by hand).
const DEFAULT_COLOR: u32 = 0xa4a3a4;
/// How near (screen pixels) a press has to be to a guide to take it.
const GRAB_PX: f32 = 4.0;

impl SlideEditor {
    /// The guides shown: those of the page in the master view, else the normal view's.
    pub(crate) fn guide_list(&self) -> Vec<SlideGuide> {
        if self.is_master_view() { self.slide().guides.clone() } else { self.presentation().guides.clone() }
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
        if let Some(s) = self.pptx_mut() {
            s.guides = true;
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
        let size = self.presentation().size_of(self.slide());
        self.guide_list()
            .iter()
            .enumerate()
            .filter(|(_, g)| {
                let (along, limit) = if g.horizontal { (at.x, size.width) } else { (at.y, size.height) };
                along >= -slop && along <= limit + slop
            })
            .map(|(i, g)| (i, (if g.horizontal { at.y } else { at.x } - g.emu()).abs()))
            .filter(|(_, d)| *d <= slop)
            .min_by_key(|(_, d)| *d)
            .map(|(i, _)| i)
    }

    /// Guide `index` put where the pointer is (`to`): moved, or removed when it is off the slide.
    pub(crate) fn drop_guide(&mut self, index: usize, to: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(layout) = self.slide_layout() else { return };
        let mut guides = self.guide_list();
        let Some(guide) = guides.get(index).cloned() else { return };
        let size = self.presentation().size_of(self.slide());
        let at = layout.to_emu(to);
        let (coord, limit) = if guide.horizontal { (at.y, size.height) } else { (at.x, size.width) };
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
        let guides = self.guide_list();
        (guides.iter().filter(|g| !g.horizontal).map(SlideGuide::emu).collect(), guides.iter().filter(|g| g.horizontal).map(SlideGuide::emu).collect())
    }

    /// Where guide `index` is drawn (EMU along its axis), with a drag of it in progress.
    pub(crate) fn guide_drawn_at(&self, index: usize, guide: &SlideGuide) -> Option<i64> {
        if let (Some(super::slide_view::Drag::Guide { index: dragged, current, .. }), Some(layout)) = (self.pptx().and_then(|s| s.drag.as_ref()), self.slide_layout())
            && *dragged == index
        {
            let at = layout.to_emu(*current);
            let size = self.presentation().size_of(self.slide());
            let (coord, limit) = if guide.horizontal { (at.y, size.height) } else { (at.x, size.width) };
            return (0..=limit).contains(&coord).then_some(coord);
        }
        Some(guide.emu())
    }
}
