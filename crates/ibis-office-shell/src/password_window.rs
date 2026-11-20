// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! パスワード (a modal dialog of the shared dialog mechanism, `shell/dialog.rs`): asked for when a protected file is
//! opened (パスワードを使用して暗号化), and for the password a file is to be protected with
//! (ファイル > パスワードで保護…), twice, or taken off. The buttons are the shared action bar (Alt access keys on
//! Windows and Linux, the primary button first, Cancel last); Enter / Ctrl+Enter and Escape are the dialog's own.
use gpui_kit::component::input::InputState;
use gpui_kit::*;

use crate::chrome::{ActionButtonKind, ActionDefinition, SidebarPanel};
use crate::editor::SlideEditor;
use crate::i18n;
use crate::palette::rgb;

pub enum Mode {
    /// A protected file's password, for `path`.
    Open { path: std::path::PathBuf, slide: Option<usize> },
    /// The password the open file is to be protected with; `had`: it has one.
    Set { had: bool },
    /// 書き込みパスワード: asked for to edit the open file.
    Modify,
    /// 書き込みパスワード: the one to set; `had`: the file has one.
    SetModify { had: bool },
}

/// The dialog's state (`SlideEditor::password_dialog`).
pub struct PasswordState {
    mode: Mode,
    first: Entity<InputState>,
    second: Entity<InputState>,
    error: Option<SharedString>,
}

impl PasswordState {
    /// Whether an error is shown (tests).
    pub fn has_error(&self) -> bool { self.error.is_some() }
}

impl SlideEditor {
    fn open_password_dialog(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        if self.dialog.active {
            self.close_dialog(window, cx);
        }
        let masked = |placeholder: &str, window: &mut Window, cx: &mut Context<Self>| {
            let placeholder = i18n::text(placeholder);
            cx.new(|cx| InputState::new(window, cx).masked(true).placeholder(placeholder))
        };
        let first = masked("_shell.password", window, cx);
        let second = masked("_shell.confirm-password", window, cx);
        first.update(cx, |input, cx| input.focus(window, cx));
        self.password_dialog = Some(PasswordState { mode, first, second, error: None });
        self.sidebar_open = true;
        cx.notify();
    }

    /// The password of protected file `path` asked for (`wrong`: after one that was not it).
    pub fn ask_password_to_open(&mut self, path: std::path::PathBuf, slide: Option<usize>, wrong: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.open_password_dialog(Mode::Open { path, slide }, window, cx);
        if let Some(state) = self.password_dialog.as_mut() {
            state.error = wrong.then(|| i18n::text("_shell.the-password-is-not-correct"));
        }
    }

    /// ファイル > パスワードで保護…: the password for the open file.
    pub fn ask_password_to_set(&mut self, had: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.open_password_dialog(Mode::Set { had }, window, cx);
    }

    /// 書き込みパスワード asked for (the file is read-only until it is given).
    pub fn ask_password_to_modify(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_password_dialog(Mode::Modify, window, cx);
    }

    /// ファイル > 書き込みパスワード…: the password to modify the open presentation.
    pub fn ask_password_to_set_modify(&mut self, had: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.open_password_dialog(Mode::SetModify { had }, window, cx);
    }

    /// Whether the password dialog is open (tests).
    pub fn password_dialog_open(&self) -> bool {
        self.password_dialog.is_some() && self.dialog.active && self.dialog.key == Some("password_dialog")
    }

    fn password_error(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(state) = self.password_dialog.as_mut() {
            state.error = Some(message);
        }
        cx.notify();
    }

    /// OK: open the file, or set the password.
    pub(crate) fn accept_password_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.password_dialog.as_ref() else { return };
        let password = state.first.read(cx).value().to_string();
        let confirmed = state.second.read(cx).value() == password.as_str();
        if password.is_empty() {
            return self.password_error(i18n::text("_shell.enter-a-password"), cx);
        }
        match &state.mode {
            Mode::Open { path, slide } => {
                let (path, slide) = (path.clone(), *slide);
                self.close_dialog(window, cx);
                self.open_path_with_password(path, password, slide, window, cx);
            }
            Mode::Modify => match self.unlock_modify(&password, cx) {
                Ok(()) => self.close_dialog(window, cx),
                Err(message) => self.password_error(message.into(), cx),
            },
            Mode::SetModify { .. } => {
                if !confirmed {
                    return self.password_error(i18n::text("_shell.the-passwords-do-not-match"), cx);
                }
                match self.set_modify_password(Some(password), cx) {
                    Ok(()) => self.close_dialog(window, cx),
                    Err(message) => self.password_error(message.into(), cx),
                }
            }
            Mode::Set { .. } => {
                if !confirmed {
                    return self.password_error(i18n::text("_shell.the-passwords-do-not-match"), cx);
                }
                self.close_dialog(window, cx);
                self.set_protection(Some(password), window, cx);
            }
        }
    }

    /// The password taken off.
    fn remove_password_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let modify = self.password_dialog.as_ref().is_some_and(|s| matches!(s.mode, Mode::SetModify { .. }));
        self.close_dialog(window, cx);
        if modify {
            let _ = self.set_modify_password(None, cx);
        } else {
            self.set_protection(None, window, cx);
        }
    }

    /// The dialog's panel.
    pub(crate) fn password_panel_if_open(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.password_dialog.as_ref()?;
        let setting = matches!(state.mode, Mode::Set { .. } | Mode::SetModify { .. });
        let had = matches!(state.mode, Mode::Set { had: true } | Mode::SetModify { had: true });
        let modify = matches!(state.mode, Mode::Modify | Mode::SetModify { .. });
        let read_only_choice = matches!(state.mode, Mode::Modify);
        let (first, second) = (state.first.clone(), state.second.clone());
        let error = state.error.clone();
        let (title, message) = match (setting, modify) {
            (true, true) => ("pptx.password-window.set-password-modify", "pptx.password-window.password-needed-edit-file"),
            (true, false) => ("_shell.protect-with-password", "_shell.password-required-to-open-the-file-2"),
            (false, true) => ("pptx.password-window.enter-password-modify", "pptx.password-window.password-needed-edit-file-2"),
            (false, false) => ("_shell.enter-password", "_shell.this-file-is-protected-by-a-password"),
        };
        let mut body = crate::chrome::panel_form().child(div().child(i18n::text(message))).child(crate::chrome::transient_text_field(&first).full_width());
        if setting {
            body = body.child(crate::chrome::transient_text_field(&second).full_width());
            body = body.child(div().text_xs().text_color(rgb(crate::chrome::TEXT_SECONDARY)).child(i18n::text("_shell.a-forgotten-password-cannot-be-recovered")));
        }
        body = body.children(crate::chrome::panel_message(error.as_deref().unwrap_or("")));
        // The primary button first, then Cancel, then the deleting one (docs/rule/implimentationRule.md, the action bar).
        let mut actions = crate::chrome::action_bar()
            .action(ActionDefinition::new("password-ok", i18n::text("OK"), ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.accept_password_dialog(w, cx))));
        // 書き込みパスワード asked for as the file is opened: 読み取り専用 goes on without it (the file stays locked, saved only under another name).
        if read_only_choice {
            actions = actions.action(ActionDefinition::new("password-read-only", i18n::text("pptx.password-window.open-read-only"), ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_dialog(w, cx))));
        }
        actions = actions.action(crate::chrome::dialog_cancel_definition(cx));
        if had {
            actions = actions.action(ActionDefinition::new("password-remove", i18n::text(if modify { "pptx.password-window.remove-password-modify" } else { "_shell.remove-password" }), ActionButtonKind::Destructive).on_click(cx.listener(|e, _, w, cx| e.remove_password_dialog(w, cx))));
        }
        Some(SidebarPanel::with_footer(i18n::text(title), body.into_any_element(), actions))
    }
}
