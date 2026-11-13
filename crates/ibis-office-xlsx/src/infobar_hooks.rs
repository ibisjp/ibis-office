// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The information bar (`shell/infobar.rs`) as far as the workbook is concerned: which of its bars the open file shows, and what their
//! buttons do (`FormatEditor::infobar_kind` and `infobar_action`). The rules were in the shell's `infobar.rs`.

use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::shell::infobar::InfoBarKind;

#[ibis_office_macros::editor_ext(InfobarHooks)]
impl SlideEditor {
    /// The bar the workbook shows, if any.
    pub(crate) fn xlsx_infobar_kind(&self) -> Option<InfoBarKind> {
        // The spreadsheet screen: marked as final (`_MarkAsFinal` of docProps/custom.xml).
        if self.is_xlsx_mark_as_final() {
            Some(InfoBarKind::MarkAsFinal)
        } else if self.opened_from_legacy && !self.read_only {
            // An old .xls: same rule as the document screen's compatibility bar (no bar under `?view=1`).
            Some(InfoBarKind::LegacyFormat)
        } else {
            None
        }
    }

    /// What the button of one of the workbook's bars does.
    pub(crate) fn xlsx_infobar_action(&mut self, kind: InfoBarKind, window: &mut Window, cx: &mut Context<Self>) {
        let _ = &window;
        match kind {
            InfoBarKind::MarkAsFinal => self.xlsx_edit_anyway(cx),
            _ => {}
        }
    }
}
