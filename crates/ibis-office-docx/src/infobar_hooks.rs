// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The information bar (`shell/infobar.rs`) as far as the document is concerned: which of its bars the open file shows, and what their
//! buttons do (`FormatEditor::infobar_kind` and `infobar_action`). The rules were in the shell's `infobar.rs`.

use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::shell::infobar::InfoBarKind;
use crate::ext::*;

#[ibis_office_macros::editor_ext(InfobarHooks)]
impl SlideEditor {
    /// The bar the document shows, if any.
    pub(crate) fn docx_infobar_kind(&self) -> Option<InfoBarKind> {
        if self.is_protection_read_only() {
            Some(InfoBarKind::ProtectReadOnly)
        } else if self.is_protection_comments() {
            Some(InfoBarKind::ProtectComments)
        } else if self.is_protection_forms() {
            Some(InfoBarKind::ProtectForms)
        } else if self.is_formatting_protected() {
            Some(InfoBarKind::ProtectFormatting)
        } else if self.is_mark_as_final() {
            Some(InfoBarKind::MarkAsFinal)
        } else if self.is_read_only_recommended() && !self.is_read_only_dismissed() {
            Some(InfoBarKind::ReadOnlyRecommended)
        } else if self.is_read_mode() {
            Some(InfoBarKind::ReadMode)
        } else if self.is_viewing_mode() && !self.is_read_only() {
            // `?view=1` / `--read-only` is the viewer's own choice, not the file's setting, so no
            // "switch to editing" bar (#72). The rule is here for every screen.
            Some(InfoBarKind::Viewing)
        } else if self.is_docx_compatibility_mode() && !self.read_only {
            // Same as the viewing bar: `?view=1` is an embedded read-only view, so no "convert" bar (#139).
            Some(InfoBarKind::Compatibility)
        } else {
            None
        }
    }

    /// What the button of one of the document's bars does.
    pub(crate) fn docx_infobar_action(&mut self, kind: InfoBarKind, window: &mut Window, cx: &mut Context<Self>) {
        let _ = &window;
        match kind {
            InfoBarKind::ProtectReadOnly | InfoBarKind::ProtectComments | InfoBarKind::ProtectForms | InfoBarKind::ProtectFormatting => self.stop_protection(cx),
            InfoBarKind::MarkAsFinal => self.edit_anyway(cx),
            InfoBarKind::ReadOnlyRecommended => self.dismiss_read_only(cx),
            InfoBarKind::ReadMode => self.set_read_mode(false, window, cx),
            InfoBarKind::Viewing => self.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Editing, window, cx),
            InfoBarKind::Compatibility => self.convert_docx_to_current_format(cx),
            _ => {}
        }
    }
}
