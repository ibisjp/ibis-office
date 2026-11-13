// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The presentation's part of a window's view (`shell/view_state.rs`), taken before the file is read again and
//! put back after: the shown slide (the one at the same position, or the last if there are fewer slides) and
//! the selected shapes that are still on it.

use gpui_kit::*;
use ibis_doc_model::ShapeId;

use crate::shell::editor::SlideEditor;

/// What a presentation's window keeps.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KeptView {
    pub slide_index: usize,
    pub selection: Vec<ShapeId>,
}

#[ibis_office_macros::editor_ext(ViewState)]
impl SlideEditor {
    /// The shown slide and the selected shapes now.
    pub(crate) fn keep_slide_view(&self) -> KeptView {
        KeptView { slide_index: self.presentation().slide_index(self.current_slide).unwrap_or(0), selection: self.selected_ids() }
    }

    /// Put `view` back on the presentation that was read.
    pub(crate) fn put_slide_view_back(&mut self, view: &KeptView, window: &mut Window, cx: &mut Context<Self>) {
        let slides = &self.presentation().slides;
        if slides.is_empty() {
            return;
        }
        let id = slides[view.slide_index.min(slides.len() - 1)].id;
        self.select_slide(id, window, cx);
        let on_slide: Vec<ShapeId> = self.slide().shapes.iter().map(|s| s.id).collect();
        self.set_selection(view.selection.iter().copied().filter(|id| on_slide.contains(id)).collect());
    }
}
