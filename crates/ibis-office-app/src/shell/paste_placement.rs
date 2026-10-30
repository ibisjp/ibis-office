// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Sheet paste placement, including repeated clipboard operations.
use super::editor::SlideEditor;
use gpui_kit::*;
use ibis_doc_model::{Shape, SlideId};
#[derive(Clone)]
struct LastPaste {
    stamp: u64,
    slide: SlideId,
    cell: Option<(u32, u32)>,
    undo_after: usize,
    origin: (i64, i64),
}
pub(crate) fn stamp(data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    h.finish()
}
impl SlideEditor {
    pub(crate) fn position_sheet_paste(&mut self, shapes: &mut [Shape], stamp: u64) {
        let Some(bounds) = ibis_pptx::shape_clipboard::drawing_bounds(shapes) else {
            return;
        };
        let cell = self.current_cell();
        let slide = self.current_slide;
        let previous = self
            .document_memory
            .get::<LastPaste>("clipboard.sheet-placement");
        let origin = if let Some(last) = previous.filter(|p| {
            p.stamp == stamp
                && p.slide == slide
                && p.cell == cell
                && p.undo_after == self.document.undo_len()
        }) {
            (
                last.origin.0 + crate::pptx::editing::PASTE_OFFSET,
                last.origin.1 + crate::pptx::editing::PASTE_OFFSET,
            )
        } else if cell.is_some() {
            self.sheet_insertion_origin()
        } else if let Some((view, layout)) = self.workspace_bounds.zip(self.slide_layout()) {
            let center = layout.to_emu(view.center());
            (center.x - bounds.width / 2, center.y - bounds.height / 2)
        } else {
            (0, 0)
        };
        for shape in shapes {
            shape.frame.x += origin.0 - bounds.x;
            shape.frame.y += origin.1 - bounds.y;
        }
        self.document_memory.set(
            "clipboard.sheet-placement",
            LastPaste {
                stamp,
                slide,
                cell,
                undo_after: self.document.undo_len() + 1,
                origin,
            },
        );
    }
}
