// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! パスワード (a small window): asked for when a protected file is opened
//! (パスワードを使用して暗号化), and for the password a file is to be
//! protected with (ファイル > パスワードで保護…), twice, or taken off.
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Root, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;

use crate::shell::editor::SlideEditor;
use crate::shell::i18n;

enum Mode {
    /// A protected file's password, for `path` (`wrong`: the last one was not it).
    Open { path: std::path::PathBuf, slide: Option<usize> },
    /// The password the open file is to be protected with; `had`: it has one.
    Set { had: bool },
    /// 書き込みパスワード: asked for to edit the open presentation.
    Modify,
    /// 書き込みパスワード: the one to set; `had`: the file has one.
    SetModify { had: bool },
}

pub(crate) struct PasswordWindow {
    editor: WeakEntity<SlideEditor>,
    /// The window of the editor, to act in.
    parent: AnyWindowHandle,
    mode: Mode,
    first: Entity<InputState>,
    second: Entity<InputState>,
    error: Option<SharedString>,
}

fn open_window(view: impl FnOnce(&mut Window, &mut App) -> PasswordWindow, title: SharedString, height: f32, cx: &mut App) {
    let parent = cx.active_window().and_then(|w| w.update(cx, |_, window, _| window.bounds()).ok());
    let (width, height) = (px(400.), px(height));
    let bounds = match parent {
        Some(p) => Bounds::new(point(p.origin.x + (p.size.width - width) / 2.0, p.origin.y + (p.size.height - height) / 2.0), size(width, height)),
        None => Bounds::centered(None, size(width, height), cx),
    };
    let options = WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), titlebar: Some(TitlebarOptions { title: Some(title), ..Default::default() }), ..Default::default() };
    let _ = cx.open_window(options, |window, cx| {
        crate::shell::palette::watch(window, cx);
        let made = view(window, cx);
        let first = made.first.clone();
        let view = cx.new(|_| made);
        first.update(cx, |input, cx| input.focus(window, cx));
        cx.new(|cx| Root::new(view, window, cx))
    });
}

/// The password of protected file `path` asked for (`wrong`: after one that was not it).
pub(crate) fn ask_to_open(editor: WeakEntity<SlideEditor>, path: std::path::PathBuf, slide: Option<usize>, wrong: bool, window: &mut Window, cx: &mut App) {
    let parent = window.window_handle();
    open_window(
        move |window, cx| PasswordWindow {
            editor,
            parent,
            mode: Mode::Open { path, slide },
            first: cx.new(|cx| InputState::new(window, cx).masked(true).placeholder(i18n::text("_shell.password"))),
            second: cx.new(|cx| InputState::new(window, cx)),
            error: wrong.then(|| i18n::text("_shell.the-password-is-not-correct")),
        },
        i18n::text("_shell.enter-password"),
        190.0,
        cx,
    );
}

/// ファイル > パスワードで保護…: the password for the open file.
pub(crate) fn ask_to_set(editor: WeakEntity<SlideEditor>, had: bool, window: &mut Window, cx: &mut App) {
    let parent = window.window_handle();
    open_window(
        move |window, cx| PasswordWindow {
            editor,
            parent,
            mode: Mode::Set { had },
            first: cx.new(|cx| InputState::new(window, cx).masked(true).placeholder(i18n::text("_shell.password"))),
            second: cx.new(|cx| InputState::new(window, cx).masked(true).placeholder(i18n::text("_shell.confirm-password"))),
            error: None,
        },
        i18n::text("_shell.protect-with-password"),
        260.0,
        cx,
    );
}

/// 書き込みパスワード asked for (the file is read-only until it is given).
pub(crate) fn ask_to_modify(editor: WeakEntity<SlideEditor>, window: &mut Window, cx: &mut App) {
    let parent = window.window_handle();
    open_window(
        move |window, cx| PasswordWindow {
            editor,
            parent,
            mode: Mode::Modify,
            first: cx.new(|cx| InputState::new(window, cx).masked(true).placeholder(i18n::text("_shell.password"))),
            second: cx.new(|cx| InputState::new(window, cx)),
            error: None,
        },
        i18n::text("pptx.password-window.enter-password-modify"),
        190.0,
        cx,
    );
}

/// ファイル > 書き込みパスワード…: the password to modify the open presentation.
pub(crate) fn ask_to_set_modify(editor: WeakEntity<SlideEditor>, had: bool, window: &mut Window, cx: &mut App) {
    let parent = window.window_handle();
    open_window(
        move |window, cx| PasswordWindow {
            editor,
            parent,
            mode: Mode::SetModify { had },
            first: cx.new(|cx| InputState::new(window, cx).masked(true).placeholder(i18n::text("_shell.password"))),
            second: cx.new(|cx| InputState::new(window, cx).masked(true).placeholder(i18n::text("_shell.confirm-password"))),
            error: None,
        },
        i18n::text("pptx.password-window.set-password-modify"),
        260.0,
        cx,
    );
}

impl PasswordWindow {
    fn close(&self, window: &mut Window) {
        window.remove_window();
    }

    /// OK: open the file, or set the password.
    fn accept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let password = self.first.read(cx).value().to_string();
        if password.is_empty() {
            self.error = Some(i18n::text("_shell.enter-a-password"));
            return cx.notify();
        }
        match &self.mode {
            Mode::Open { path, slide } => {
                let (path, slide, editor, parent) = (path.clone(), *slide, self.editor.clone(), self.parent);
                self.close(window);
                cx.defer(move |cx| {
                    let _ = parent.update(cx, |_, window, cx| {
                        let _ = editor.update(cx, |e, cx| e.open_path_with_password(path, password, slide, window, cx));
                    });
                });
            }
            Mode::Modify => {
                // The editor says whether it is the password (the file can be edited then).
                let result = self.editor.update(cx, |e, cx| e.unlock_modify(&password, cx)).unwrap_or_else(|e| Err(e.to_string()));
                match result {
                    Ok(()) => self.close(window),
                    Err(message) => {
                        self.error = Some(message.into());
                        cx.notify();
                    }
                }
            }
            Mode::SetModify { .. } => {
                if self.second.read(cx).value() != password.as_str() {
                    self.error = Some(i18n::text("_shell.the-passwords-do-not-match"));
                    return cx.notify();
                }
                let result = self.editor.update(cx, |e, cx| e.set_modify_password(Some(password), cx)).unwrap_or_else(|e| Err(e.to_string()));
                match result {
                    Ok(()) => self.close(window),
                    Err(message) => {
                        self.error = Some(message.into());
                        cx.notify();
                    }
                }
            }
            Mode::Set { .. } => {
                if self.second.read(cx).value() != password.as_str() {
                    self.error = Some(i18n::text("_shell.the-passwords-do-not-match"));
                    return cx.notify();
                }
                let (editor, parent) = (self.editor.clone(), self.parent);
                self.close(window);
                cx.defer(move |cx| {
                    let _ = parent.update(cx, |_, window, cx| {
                        let _ = editor.update(cx, |e, cx| e.set_protection(Some(password), window, cx));
                    });
                });
            }
        }
    }

    /// The password taken off.
    fn remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.mode, Mode::SetModify { .. }) {
            let _ = self.editor.update(cx, |e, cx| e.set_modify_password(None, cx));
            return self.close(window);
        }
        let (editor, parent) = (self.editor.clone(), self.parent);
        self.close(window);
        cx.defer(move |cx| {
            let _ = parent.update(cx, |_, window, cx| {
                let _ = editor.update(cx, |e, cx| e.set_protection(None, window, cx));
            });
        });
    }
}

impl Render for PasswordWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let setting = matches!(self.mode, Mode::Set { .. } | Mode::SetModify { .. });
        let had = matches!(self.mode, Mode::Set { had: true } | Mode::SetModify { had: true });
        let modify = matches!(self.mode, Mode::Modify | Mode::SetModify { .. });
        let mut body = v_flex().id("password-dialog").size_full().gap_2().p_4().bg(theme.background).text_color(theme.foreground)
            .capture_key_down(cx.listener(move|e,event:&KeyDownEvent,w,cx|{
                let key=&event.keystroke;let modified=if cfg!(target_os="macos"){key.modifiers.platform}else{key.modifiers.control};
                let policy=if setting{super::panel_dialog::SubmitType::CtrlEnter}else{super::panel_dialog::SubmitType::Enter};
                if key.key=="enter" && !key.modifiers.shift && super::panel_dialog::form::shortcut(policy,&[],false,modified) {
                    use gpui_kit::EntityInputHandler as _;
                    for input in [&e.first, &e.second] {
                        if input.read(cx).focus_handle(cx).is_focused(w)
                            && input.update(cx, |state, cx| state.marked_text_range(w, cx).is_some()) { return; }
                    }
                    e.accept(w,cx);w.prevent_default();cx.stop_propagation();
                }
            }));
        body = body.child(div().text_size(px(13.)).child(i18n::text(match (setting, modify) {
            (true, true) => "pptx.password-window.password-needed-edit-file",
            (true, false) => "_shell.password-required-to-open-the-file-2",
            (false, true) => "pptx.password-window.password-needed-edit-file-2",
            (false, false) => "_shell.this-file-is-protected-by-a-password",
        })));
        body = body.child(Input::new(&self.first).small());
        if setting {
            body = body.child(Input::new(&self.second).small());
            body = body.child(div().text_size(px(11.)).text_color(theme.muted_foreground).child(i18n::text("_shell.a-forgotten-password-cannot-be-recovered")));
        }
        if let Some(error) = &self.error {
            body = body.child(div().text_size(px(12.)).text_color(rgb(0xb3261e)).child(error.clone()));
        }
        let mut buttons = h_flex().gap_2().justify_end().mt_2();
        if had {
            buttons = buttons.child(crate::shell::chrome::destructive_button("password-remove",cx).small().label(i18n::text(if modify { "pptx.password-window.remove-password-modify" } else { "_shell.remove-password" })).on_click(cx.listener(|this, _, window, cx| this.remove(window, cx))));
        }
        buttons = buttons
            .child(crate::shell::chrome::action_button("password-cancel").small().label(i18n::text("_shell.cancel")).on_click(cx.listener(|this, _, window, _| this.close(window))))
            .child(crate::shell::chrome::primary_button("password-ok").small().label(i18n::text("OK")).on_click(cx.listener(|this, _, window, cx| this.accept(window, cx))));
        body.child(buttons)
    }
}
