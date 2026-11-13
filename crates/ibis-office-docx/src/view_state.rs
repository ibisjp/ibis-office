// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The document's part of a window's view (`shell/view_state.rs`), taken before the file is read again and put
//! back after: the caret and the other end of the selection, if the paragraph is still there and the offset is
//! within it.

use gpui_kit::*;
use ibis_doc_model::FlowPos;

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// What a document's window keeps.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct KeptView {
    pub caret: Option<FlowPos>,
    pub anchor: Option<FlowPos>,
}

#[ibis_office_macros::editor_ext(ViewState)]
impl SlideEditor {
    /// The caret and the other end of the selection now.
    pub(crate) fn keep_doc_view(&self) -> KeptView {
        let state = self.doc_state();
        KeptView { caret: state.and_then(|s| s.caret), anchor: state.and_then(|s| s.anchor) }
    }

    /// Put `view` back on the document that was read.
    pub(crate) fn put_doc_view_back(&mut self, view: &KeptView, cx: &mut Context<Self>) {
        let KeptView { caret, anchor } = view;
        let valid = |pos: &FlowPos| {
            self.flow()
                .and_then(|doc| ibis_doc_model::flow_command::paragraph(doc, pos.para))
                .is_some_and(|p| pos.offset <= p.len())
        };
        if let Some(caret) = caret.filter(valid) {
            match anchor.filter(valid) {
                Some(anchor) => {
                    self.set_caret(anchor, false, cx);
                    self.set_caret(caret, true, cx);
                }
                None => self.set_caret(caret, false, cx),
            }
        }
    }
}
