// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The information bar (`shell/infobar.rs`) as far as the presentation is concerned: which of its bars the open file shows, and what their
//! buttons do (`FormatEditor::infobar_kind` and `infobar_action`). The rules were in the shell's `infobar.rs`.

use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::shell::infobar::InfoBarKind;
use crate::ext::*;

#[ibis_office_macros::editor_ext(InfobarHooks)]
impl SlideEditor {
    /// The bar the presentation shows, if any.
    pub(crate) fn pptx_infobar_kind(&self) -> Option<InfoBarKind> {
        if self.is_pptx_mark_as_final() {
            Some(InfoBarKind::MarkAsFinal)
        } else if self.is_pptx_read_only_recommended() && !self.is_pptx_read_only_dismissed() {
            Some(InfoBarKind::ReadOnlyRecommended)
        } else if self.opened_from_legacy && !self.read_only {
            // An old .ppt: same rule as the document screen's compatibility bar (no bar under `?view=1`).
            Some(InfoBarKind::LegacyFormat)
        } else {
            None
        }
    }

    /// What the button of one of the presentation's bars does.
    pub(crate) fn pptx_infobar_action(&mut self, kind: InfoBarKind, window: &mut Window, cx: &mut Context<Self>) {
        let _ = &window;
        match kind {
            InfoBarKind::MarkAsFinal => self.pptx_edit_anyway(cx),
            InfoBarKind::ReadOnlyRecommended => self.dismiss_pptx_read_only(cx),
            _ => {}
        }
    }
}
