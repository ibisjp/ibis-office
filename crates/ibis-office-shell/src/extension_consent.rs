// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! CMYK as a feature PowerPoint does not have. The question before the first use ("この機能は PowerPoint にない機能です。
//! 有効にしますか？", OK / キャンセル) is the one every ibisOffice feature of that kind shares: `SlideEditor::require_ibis_extensions`
//! (its answer is the file's one flag `Presentation::ibis_extensions`, in the message box of `SlideEditor::ask`). This file adds
//! what CMYK needs on top: its own mode flag (`CmykMode::enabled`, in the file), turned on with the consent.

use std::rc::Rc;

use gpui_kit::*;

use super::editor::SlideEditor;

/// What to do once CMYK is on (at once when it already is).
pub(crate) type AfterConsent = Rc<dyn Fn(&mut SlideEditor, &mut Context<SlideEditor>)>;

impl SlideEditor {
    /// Runs `then` for a use of CMYK: at once when the file has the CMYK mode on, else after the shared question (OK turns the
    /// file's ibisOffice features and its CMYK mode on first; キャンセル does nothing).
    pub fn require_cmyk(&mut self, then: AfterConsent, cx: &mut Context<Self>) {
        if self.presentation().cmyk.enabled {
            then(self, cx);
            return;
        }
        self.require_ibis_extensions(
            move |this, cx| {
                this.enable_cmyk_mode(cx);
                if this.presentation().cmyk.enabled {
                    then(this, cx);
                }
            },
            cx,
        );
    }

    /// Turns the CMYK mode on for the file (one undo step), and puts the user's profile in use or shows the bar about it.
    fn enable_cmyk_mode(&mut self, cx: &mut Context<Self>) {
        let mut mode = self.presentation().cmyk.clone();
        mode.enabled = true;
        self.run(vec![ibis_doc_model::Command::SetCmykMode { cmyk: mode }], cx);
        self.after_cmyk_enabled(cx);
    }
}
