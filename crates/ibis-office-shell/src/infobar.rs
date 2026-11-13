// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The information bar under the ribbon (Office's yellow bar: protection, marked as final, read-only
//! recommended, viewing mode, ...). Which bar a screen shows is decided in one place, `infobar_kind`, for the
//! document (DOCX), spreadsheet (XLSX) and presentation (PPTX) screens alike; the bar is drawn by one component,
//! `render_infobar`. A bar that only one screen has (Office shows it only there) is a kind that only that screen's
//! branch returns.

use gpui_kit::*;
use crate::format::with_format;

use crate::editor::SlideEditor;
use crate::palette::rgb;

/// The bars the screens can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfoBarKind {
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
    /// XLSX / PPTX: the file was read from an old binary format (.xls, .ppt): Compatibility Mode, until it is saved in a current format.
    LegacyFormat,
    /// Every screen: the CMYK mode is on and no ICC profile is chosen yet (the colours are shown by the simple conversion).
    CmykProfileHint,
    /// Every screen: the file was made with an ICC profile this PC does not have.
    CmykProfileMissing,
}

/// What one button of a bar does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InfoBarAction {
    /// The bar's own action (`run_infobar_action`).
    Main,
    /// Choose an ICC profile (shell/cmyk_profile.rs).
    ChooseProfile,
    /// Open the page where profiles are given out, in the browser.
    OpenProfileSource,
    /// Close the bar (it is not shown again for this).
    Dismiss,
    /// Go on with the simple conversion.
    UseSimpleConversion,
}

/// A bar's (bar id, button id, title key, message key, button key).
pub type InfoBarSpec = (&'static str, &'static str, &'static str, &'static str, &'static str);

impl InfoBarKind {
    /// The bar's buttons from the left: (button id, label key, what it does). A bar with one button has the one `spec` gives.
    fn buttons(self, first: (&'static str, &'static str)) -> Vec<(&'static str, &'static str, InfoBarAction)> {
        match self {
            InfoBarKind::CmykProfileHint => vec![
                (first.0, first.1, InfoBarAction::ChooseProfile),
                ("infobar-btn-cmyk-source", "shell.infobar.cmyk.source", InfoBarAction::OpenProfileSource),
                ("infobar-btn-cmyk-later", "shell.infobar.cmyk.later", InfoBarAction::Dismiss),
            ],
            InfoBarKind::CmykProfileMissing => vec![
                (first.0, first.1, InfoBarAction::ChooseProfile),
                ("infobar-btn-cmyk-simple", "shell.infobar.cmyk.simple", InfoBarAction::UseSimpleConversion),
            ],
            _ => vec![(first.0, first.1, InfoBarAction::Main)],
        }
    }

    /// (bar id, button id, title key, message key, button key) of the bars the formats share. The ids are written out so the i18n
    /// check sees them. A format that words a bar in its own way gives its own (`FormatEditor::infobar_spec`).
    fn spec(self) -> InfoBarSpec {
        match self {
            InfoBarKind::LegacyFormat => ("xlsx-infobar-legacy", "infobar-btn-convert-legacy", "xlsx.infobar.legacy.title", "xlsx.infobar.legacy.message", "xlsx.infobar.legacy.convert"),
            InfoBarKind::CmykProfileHint => ("infobar-cmyk-hint", "infobar-btn-cmyk-choose", "shell.infobar.cmyk.title", "shell.infobar.cmyk.hint.message", "shell.infobar.cmyk.choose"),
            InfoBarKind::CmykProfileMissing => ("infobar-cmyk-missing", "infobar-btn-cmyk-choose", "shell.infobar.cmyk.title", "shell.infobar.cmyk.missing.message", "shell.infobar.cmyk.choose-short"),
            InfoBarKind::ProtectReadOnly => ("docx-infobar-protected-readonly", "infobar-btn-stop-protection", "docx.infobar.protect.readonly.title", "docx.infobar.protect.readonly.message", "docx.infobar.protect.stop"),
            InfoBarKind::ProtectComments => ("docx-infobar-protected-comments", "infobar-btn-stop-protection", "docx.infobar.protect.comments.title", "docx.infobar.protect.comments.message", "docx.infobar.protect.stop"),
            InfoBarKind::ProtectForms => ("docx-infobar-protected-forms", "infobar-btn-stop-protection", "docx.infobar.protect.forms.title", "docx.infobar.protect.forms.message", "docx.infobar.protect.stop"),
            InfoBarKind::ProtectFormatting => ("docx-infobar-protected-formatting", "infobar-btn-stop-protection", "docx.infobar.protect.formatting.title", "docx.infobar.protect.formatting.message", "docx.infobar.protect.stop"),
            InfoBarKind::MarkAsFinal => ("docx-infobar-final", "infobar-btn-edit-anyway", "docx.infobar.final.title", "docx.infobar.final.message", "docx.infobar.final.edit-anyway"),
            InfoBarKind::ModifyPassword => ("pptx-infobar-modify", "infobar-btn-modify-password", "pptx.infobar.modify.title", "pptx.infobar.modify.message", "pptx.infobar.modify.unlock"),
            InfoBarKind::ReadOnlyRecommended => ("docx-infobar-readonly", "infobar-btn-enable-editing", "docx.infobar.readonly.title", "docx.infobar.readonly.message", "docx.infobar.readonly.enable-editing"),
            InfoBarKind::ReadMode => ("docx-infobar-readmode", "infobar-btn-exit-readmode", "docx.infobar.readmode.title", "docx.infobar.readmode.message", "docx.infobar.readmode.exit"),
            InfoBarKind::Viewing => ("docx-infobar-viewing", "infobar-btn-switch-to-editing", "docx.infobar.viewing.title", "docx.infobar.viewing.message", "docx.infobar.viewing.edit"),
            InfoBarKind::Compatibility => ("docx-infobar-compat", "infobar-btn-convert-compat", "docx.infobar.compat.title", "docx.infobar.compat.message", "docx.infobar.compat.convert"),
        }
    }
}

impl SlideEditor {
    /// XLSX: the workbook is marked as final (`_MarkAsFinal` in docProps/custom.xml, Excel's 最終版にする).
    pub fn is_xlsx_mark_as_final(&self) -> bool {
        use ibis_doc_model::CustomPropertyValue;
        self.presentation().workbook.as_deref().is_some_and(|w| w.properties.custom.iter().any(|p| p.name == "_MarkAsFinal" && matches!(p.value, CustomPropertyValue::Bool(true))))
    }

    /// XLSX: the bar's button. Excel removes the mark and the sheet can be edited (undo puts the mark back).
    pub fn xlsx_edit_anyway(&mut self, cx: &mut Context<Self>) {
        let Some(wb) = self.presentation().workbook.as_deref() else { return };
        let mut properties = wb.properties.clone();
        properties.custom.retain(|p| p.name != "_MarkAsFinal");
        if let Err(err) = self.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetDocProperties(properties))]) {
            log::warn!("edit anyway rejected: {err}");
        }
        cx.notify();
    }

    /// The one place that decides which information bar the open file's screen shows (None for none).
    pub fn infobar_kind(&self) -> Option<InfoBarKind> {
        self.base_infobar_kind().or_else(|| self.cmyk_notice.map(|n| match n {
            crate::cmyk_profile::CmykNotice::Hint => InfoBarKind::CmykProfileHint,
            crate::cmyk_profile::CmykNotice::Missing => InfoBarKind::CmykProfileMissing,
        }))
    }

    fn base_infobar_kind(&self) -> Option<InfoBarKind> {
        // A password to modify, not yet given: the same bar on every screen.
        if self.is_modify_locked() {
            return Some(InfoBarKind::ModifyPassword);
        }
        with_format!(self, infobar_kind(self))
    }

    /// The information bar to put under the ribbon, or None.
    pub fn render_infobar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let kind = self.infobar_kind()?;
        let (bar_id, btn_id, title_key, message_key, button_key) = with_format!(self, infobar_spec(kind)).unwrap_or_else(|| kind.spec());
        let mut title: SharedString = crate::i18n::text(title_key);
        if let Some(suffix) = with_format!(self, infobar_title_suffix(self, kind)) {
            title = format!("{} ({})", title, suffix).into();
        }
        let message: SharedString = if kind == InfoBarKind::CmykProfileMissing {
            crate::i18n::format(message_key, &[("name", &self.presentation().cmyk.profile.clone().unwrap_or_default())]).into()
        } else {
            crate::i18n::text(message_key)
        };
        let buttons = kind.buttons((btn_id, button_key));
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
                .child(div().flex().flex_row().items_center().gap_2().flex_none().children(buttons.into_iter().map(|(id, key, action)| {
                    div()
                        .id(id)
                        .relative()
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
                        .child(crate::i18n::text(key))
                        // (Where the button is drawn, for the scenarios' clicks.)
                        .child(canvas(|_, _, _| (), move |bounds, _, _, _| {
                            #[cfg(not(all(feature = "scenarios", any(target_os = "macos", target_os = "linux"))))]
                            let _ = bounds;
                            #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
                            crate::scenario::button_probe::record_bounds(id, bounds);
                        }).absolute().inset_0())
                        .on_click(cx.listener(move |this, _, window, cx| this.run_infobar_button(kind, action, window, cx)))
                })))
                .into_any_element(),
        )
    }

    /// What a button of the bar does.
    fn run_infobar_button(&mut self, kind: InfoBarKind, action: InfoBarAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            InfoBarAction::Main => self.run_infobar_action(kind, window, cx),
            InfoBarAction::ChooseProfile => self.open_cmyk_profile_chooser(window, cx),
            InfoBarAction::OpenProfileSource => crate::external::open_url(crate::cmyk_profile::PROFILE_SOURCE_URL, cx),
            InfoBarAction::Dismiss => self.dismiss_cmyk_notice(cx),
            InfoBarAction::UseSimpleConversion => self.use_simple_cmyk_conversion(cx),
        }
    }

    /// What the bar's button does: the shell's own bars here, the format's by its `infobar_action`.
    fn run_infobar_action(&mut self, kind: InfoBarKind, window: &mut Window, cx: &mut Context<Self>) {
        match kind {
            InfoBarKind::ModifyPassword => self.ask_modify_password_to_unlock(window, cx),
            // The model is already the current format; saving it under a new name (.xlsx / .pptx) makes it so.
            InfoBarKind::LegacyFormat => self.save_as_then(window, cx, |_, _, _| {}),
            InfoBarKind::CmykProfileHint | InfoBarKind::CmykProfileMissing => {}
            _ => with_format!(self, infobar_action(self, kind, window, cx)),
        }
    }
}
