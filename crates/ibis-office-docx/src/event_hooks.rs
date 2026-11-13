// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What the document does with the window's events (`FormatEditor`'s event hooks): the code was in `shell/editor.rs`.

use gpui_kit::*;
use ibis_doc_model::Command;

use crate::shell::editor::SlideEditor;
use crate::ext::*;

#[ibis_office_macros::editor_ext(EventHooks)]
impl SlideEditor {

    /// The font size (points) chosen in the toolbar, for the text selected.
    pub(crate) fn docx_set_font_size(&mut self, pt: f32, cx: &mut Context<Self>) {
        let hundredths = (pt * 100.0).round().clamp(100.0, 400_000.0) as u32;
        if self.apply_box_edit_run_change(&ibis_doc_model::RunChange::Size(hundredths), cx) {
            return;
        }
        self.apply_text_properties(
            ibis_doc_model::RunFormat {
                size: Some(hundredths),
                ..Default::default()
            },
            cx,
        );
    }

    /// A change of a floating text box's text being edited: `Some(whether it was done)` when a box is being edited.
    pub(crate) fn docx_box_edit_splice(&mut self, start: usize, remove: usize, insert: &str, typing: bool, cx: &mut Context<Self>) -> Option<bool> {
        let (at, edit) = self.doc_state().and_then(|s| s.box_edit.as_ref()).map(|b| (b.at, b.edit.clone()))?;
        let join = edit.is_new || (typing && edit.typing);
        let cmd = ibis_doc_model::FlowCommand::SpliceShapeText { at, start, remove, insert: insert.to_string() };
        let keep = self.text_selection();
        let done = if join {
            let res = self.document.amend_last(vec![Command::Flow(cmd)]);
            if res.is_ok() {
                self.sync_peers(cx);
            }
            res.is_ok()
        } else {
            self.run_flow(vec![cmd], cx)
        };
        if done {
            if let (Some((a, c)), Some(st)) = (keep, self.doc_state_mut()) {
                st.anchor = Some(a);
                st.caret = Some(c);
            }
            if let Some(b) = self.doc_state_mut().and_then(|s| s.box_edit.as_mut()) {
                b.edit.typing = typing;
                b.edit.is_new = false;
            }
            self.sync_docx_edited_box_resource();
            cx.notify();
        }
        Some(done)
    }
}
