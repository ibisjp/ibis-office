// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What the print panels of the three screens share: the file's CMYK setting (ibisOffice's own; PowerPoint has none) and the
//! buttons at the foot (印刷 / 設定を保存 / キャンセル). CMYK concerns only those who print, so it is set here and nowhere else.
//! A file with the mode on is shown on the screen as the press would print it (a soft proof); the PDF and the printer driver's
//! own conversion are not touched (`shell::cmyk`).

use gpui_kit::component::v_flex;
use gpui_kit::*;

use super::{chrome, editor::SlideEditor, i18n::text};

impl SlideEditor {
    /// The CMYK setting of the file in a print panel: the check box for the mode (turning it on asks first) and the button
    /// for the ICC profile, with a line that says what the mode does. `prefix` makes the element ids (`pptx-print`, ...).
    pub fn print_cmyk_section(&mut self, prefix: &'static str, cx: &mut Context<Self>) -> AnyElement {
        // The mode as the panel was opened, kept for Cancel.
        if self.print_cmyk_origin.is_none() {
            self.print_cmyk_origin = Some(self.presentation().cmyk.clone());
        }
        let on = self.cmyk_on();
        v_flex()
            .gap_1()
            .w_full()
            .child(chrome::checkbox_button(
                SharedString::from(format!("{prefix}-cmyk-mode")),
                text("shell.print.cmyk-mode"),
                on,
                false,
                cx.listener(|e, _, _, cx| e.toggle_cmyk_mode(cx)),
            ))
            .child(div().pl(px(22.)).child(
                chrome::button_group([chrome::ButtonGroupItem::new(SharedString::from(format!("{prefix}-cmyk-profile")), text("shell.cmyk.profile.title"))
                    .disabled(!on)
                    .on_click(cx.listener(|e, _, w, cx| e.open_cmyk_profile_chooser(w, cx)))])
                .compact(),
            ))
            .child(div().pl(px(22.)).text_sm().text_color(rgb(0x5f6368)).child(text("shell.print.cmyk-note")))
            .into_any_element()
    }

    /// 設定を保存 / 印刷: what was set in the panel stays.
    pub fn keep_print_cmyk(&mut self) {
        self.print_cmyk_origin = None;
    }

    /// キャンセル (and the window's close button): the CMYK mode is put back as it was when the panel was opened. (Not while the
    /// window to choose the profile is open in the panel's place: that is a step of the same setting.)
    pub fn cancel_print_cmyk(&mut self, cx: &mut Context<Self>) {
        if self.cmyk_profile_chooser_open() {
            return;
        }
        if let Some(origin) = self.print_cmyk_origin.take() {
            if origin != self.presentation().cmyk {
                self.run(vec![ibis_doc_model::Command::SetCmykMode { cmyk: origin }], cx);
            }
        }
        cx.notify();
    }

    /// The check box: on asks first (a feature PowerPoint does not have) and shows the screen as printed; off shows it as it is.
    pub(crate) fn toggle_cmyk_mode(&mut self, cx: &mut Context<Self>) {
        if self.cmyk_on() {
            let mut mode = self.presentation().cmyk.clone();
            mode.enabled = false;
            self.run(vec![ibis_doc_model::Command::SetCmykMode { cmyk: mode }], cx);
            cx.notify();
        } else {
            self.require_cmyk(std::rc::Rc::new(|_, cx| cx.notify()), cx);
        }
    }
}

/// The foot of a print panel: 印刷 (the primary button), 設定を保存 (keeps the settings, prints nothing) and キャンセル.
/// `prefix` makes the ids (`pptx-print-apply`, `pptx-print-save`, `pptx-print-cancel`).
pub fn print_action_bar(
    prefix: &'static str,
    print_label: SharedString,
    print: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    save: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cancel: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> chrome::ActionBar {
    chrome::action_bar()
        .action(chrome::ActionDefinition::new(SharedString::from(format!("{prefix}-apply")), print_label, chrome::ActionButtonKind::Primary).on_click(print))
        .action(chrome::ActionDefinition::new(SharedString::from(format!("{prefix}-save")), text("shell.print.save-settings"), chrome::ActionButtonKind::Normal).on_click(save))
        .action(chrome::ActionDefinition::new(SharedString::from(format!("{prefix}-cancel")), text("_shell.cancel"), chrome::ActionButtonKind::Normal).on_click(cancel))
}
