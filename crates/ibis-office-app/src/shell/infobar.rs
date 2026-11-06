// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The information bar under the ribbon (Office's yellow bar: protection, marked as final, read-only
//! recommended, viewing mode, ...). Which bar a screen shows is decided in one place, `infobar_kind`, for the
//! document (DOCX), spreadsheet (XLSX) and presentation (PPTX) screens alike; the bar is drawn by one component,
//! `render_infobar`. A bar that only one screen has (Office shows it only there) is a kind that only that screen's
//! branch returns.

use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::shell::palette::rgb;

/// The bars the screens can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InfoBarKind {
    /// DOCX: restrict editing, no changes allowed.
    ProtectReadOnly,
    /// DOCX: restrict editing, comments only.
    ProtectComments,
    /// DOCX: restrict editing, forms only.
    ProtectForms,
    /// DOCX: formatting restrictions.
    ProtectFormatting,
    /// DOCX / PPTX / XLSX: marked as final.
    MarkAsFinal,
    /// DOCX / PPTX / XLSX: a password to modify, not yet given.
    ModifyPassword,
    /// DOCX / PPTX: the author recommends opening read-only.
    ReadOnlyRecommended,
    /// DOCX: Read Mode.
    ReadMode,
    /// DOCX: Viewing mode chosen in the document (the user can switch to editing).
    Viewing,
    /// DOCX: an older compatibility mode.
    Compatibility,
}

impl InfoBarKind {
    /// (bar id, button id, title key, message key, button key). The ids are written out so the i18n check sees them.
    fn spec(self, is_presentation: bool, is_spreadsheet: bool) -> (&'static str, &'static str, &'static str, &'static str, &'static str) {
        if self == InfoBarKind::MarkAsFinal && is_spreadsheet {
            return ("xlsx-infobar-final", "infobar-btn-edit-anyway", "xlsx.infobar.final.title", "xlsx.infobar.final.message", "xlsx.infobar.final.edit-anyway");
        }
        match (self, is_presentation) {
            (InfoBarKind::ProtectReadOnly, _) => ("docx-infobar-protected-readonly", "infobar-btn-stop-protection", "docx.infobar.protect.readonly.title", "docx.infobar.protect.readonly.message", "docx.infobar.protect.stop"),
            (InfoBarKind::ProtectComments, _) => ("docx-infobar-protected-comments", "infobar-btn-stop-protection", "docx.infobar.protect.comments.title", "docx.infobar.protect.comments.message", "docx.infobar.protect.stop"),
            (InfoBarKind::ProtectForms, _) => ("docx-infobar-protected-forms", "infobar-btn-stop-protection", "docx.infobar.protect.forms.title", "docx.infobar.protect.forms.message", "docx.infobar.protect.stop"),
            (InfoBarKind::ProtectFormatting, _) => ("docx-infobar-protected-formatting", "infobar-btn-stop-protection", "docx.infobar.protect.formatting.title", "docx.infobar.protect.formatting.message", "docx.infobar.protect.stop"),
            (InfoBarKind::MarkAsFinal, false) => ("docx-infobar-final", "infobar-btn-edit-anyway", "docx.infobar.final.title", "docx.infobar.final.message", "docx.infobar.final.edit-anyway"),
            (InfoBarKind::MarkAsFinal, true) => ("pptx-infobar-final", "infobar-btn-edit-anyway", "pptx.infobar.final.title", "pptx.infobar.final.message", "pptx.infobar.final.edit-anyway"),
            (InfoBarKind::ModifyPassword, _) => ("pptx-infobar-modify", "infobar-btn-modify-password", "pptx.infobar.modify.title", "pptx.infobar.modify.message", "pptx.infobar.modify.unlock"),
            (InfoBarKind::ReadOnlyRecommended, false) => ("docx-infobar-readonly", "infobar-btn-enable-editing", "docx.infobar.readonly.title", "docx.infobar.readonly.message", "docx.infobar.readonly.enable-editing"),
            (InfoBarKind::ReadOnlyRecommended, true) => ("pptx-infobar-readonly", "infobar-btn-enable-editing", "pptx.infobar.readonly.title", "pptx.infobar.readonly.message", "pptx.infobar.readonly.enable-editing"),
            (InfoBarKind::ReadMode, _) => ("docx-infobar-readmode", "infobar-btn-exit-readmode", "docx.infobar.readmode.title", "docx.infobar.readmode.message", "docx.infobar.readmode.exit"),
            (InfoBarKind::Viewing, _) => ("docx-infobar-viewing", "infobar-btn-switch-to-editing", "docx.infobar.viewing.title", "docx.infobar.viewing.message", "docx.infobar.viewing.edit"),
            (InfoBarKind::Compatibility, _) => ("docx-infobar-compat", "infobar-btn-convert-compat", "docx.infobar.compat.title", "docx.infobar.compat.message", "docx.infobar.compat.convert"),
        }
    }
}

impl SlideEditor {
    /// XLSX: the workbook is marked as final (`_MarkAsFinal` in docProps/custom.xml, Excel's 最終版にする).
    pub(crate) fn is_xlsx_mark_as_final(&self) -> bool {
        use ibis_doc_model::CustomPropertyValue;
        self.presentation().workbook.as_deref().is_some_and(|w| w.properties.custom.iter().any(|p| p.name == "_MarkAsFinal" && matches!(p.value, CustomPropertyValue::Bool(true))))
    }

    /// XLSX: the bar's button. Excel removes the mark and the sheet can be edited (undo puts the mark back).
    pub(crate) fn xlsx_edit_anyway(&mut self, cx: &mut Context<Self>) {
        let Some(wb) = self.presentation().workbook.as_deref() else { return };
        let mut properties = wb.properties.clone();
        properties.custom.retain(|p| p.name != "_MarkAsFinal");
        if let Err(err) = self.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetDocProperties(properties))]) {
            log::warn!("edit anyway rejected: {err}");
        }
        cx.notify();
    }

    /// The one place that decides which information bar the open file's screen shows (None for none).
    pub(crate) fn infobar_kind(&self) -> Option<InfoBarKind> {
        // A password to modify, not yet given: the same bar on every screen.
        if self.is_modify_locked() {
            return Some(InfoBarKind::ModifyPassword);
        }
        if self.is_document() {
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
            } else if self.is_docx_compatibility_mode() {
                Some(InfoBarKind::Compatibility)
            } else {
                None
            }
        } else if self.is_presentation() {
            if self.is_pptx_mark_as_final() {
                Some(InfoBarKind::MarkAsFinal)
            } else if self.is_pptx_read_only_recommended() && !self.is_pptx_read_only_dismissed() {
                Some(InfoBarKind::ReadOnlyRecommended)
            } else {
                None
            }
        } else {
            // The spreadsheet screen: marked as final (`_MarkAsFinal` of docProps/custom.xml).
            self.is_xlsx_mark_as_final().then_some(InfoBarKind::MarkAsFinal)
        }
    }

    /// The information bar to put under the ribbon, or None.
    pub(crate) fn render_infobar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let kind = self.infobar_kind()?;
        let (bar_id, btn_id, title_key, message_key, button_key) = kind.spec(self.is_presentation(), !self.is_document() && !self.is_presentation());
        let mut title: SharedString = crate::shell::i18n::text(title_key);
        if kind == InfoBarKind::Compatibility {
            title = format!("{} ({})", title, self.docx_compatibility_mode_name()).into();
        }
        let message = crate::shell::i18n::text(message_key);
        let button = crate::shell::i18n::text(button_key);
        Some(
            div()
                .id(bar_id)
                .w_full()
                .bg(rgb(0xfff4ce))
                .border_b_1()
                .border_color(rgb(0xedebe9))
                .px(px(16.))
                .py(px(6.))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .child(div().text_size(px(12.)).font_weight(FontWeight::BOLD).text_color(rgb(0x323130)).child(title))
                        .child(div().text_size(px(12.)).text_color(rgb(0x605e5c)).child(message)),
                )
                .child(
                    div()
                        .id(btn_id)
                        .px(px(12.))
                        .py(px(4.))
                        .rounded_sm()
                        .border_1()
                        .border_color(rgb(0x8a8886))
                        .bg(rgb(0xffffff))
                        .text_size(px(12.))
                        .text_color(rgb(0x323130))
                        .cursor_pointer()
                        .hover(|h| h.bg(rgb(0xf3f2f1)))
                        .child(button)
                        .on_click(cx.listener(move |this, _, window, cx| this.run_infobar_action(kind, window, cx))),
                )
                .into_any_element(),
        )
    }

    /// What the bar's button does.
    fn run_infobar_action(&mut self, kind: InfoBarKind, window: &mut Window, cx: &mut Context<Self>) {
        match kind {
            InfoBarKind::ProtectReadOnly | InfoBarKind::ProtectComments | InfoBarKind::ProtectForms | InfoBarKind::ProtectFormatting => self.stop_protection(cx),
            InfoBarKind::MarkAsFinal => {
                if self.is_presentation() {
                    self.pptx_edit_anyway(cx)
                } else if self.is_document() {
                    self.edit_anyway(cx)
                } else {
                    self.xlsx_edit_anyway(cx)
                }
            }
            InfoBarKind::ModifyPassword => self.ask_modify_password_to_unlock(window, cx),
            InfoBarKind::ReadOnlyRecommended => {
                if self.is_presentation() {
                    self.dismiss_pptx_read_only(cx)
                } else {
                    self.dismiss_read_only(cx)
                }
            }
            InfoBarKind::ReadMode => self.set_read_mode(false, window, cx),
            InfoBarKind::Viewing => self.set_interaction_mode(ibis_doc_model::DocumentInteractionMode::Editing, window, cx),
            InfoBarKind::Compatibility => self.convert_docx_to_current_format(cx),
        }
    }
}
