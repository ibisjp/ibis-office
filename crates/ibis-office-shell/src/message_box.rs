// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The message box: a title, a message and the action bar, over the window's modal backdrop.
//!
//! A screen gives the strings and the `ActionBar` (its buttons are `ActionDefinition`s, so a
//! dangerous answer is `ActionButtonKind::Destructive`, red as everywhere); this part builds
//! the box, its padding and the separator of the bar.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::*;

use std::cell::RefCell;

use crate::chrome::action_bar::ClickHandler;
use crate::chrome::{ActionBar, ActionButtonKind, action_bar_host};
use crate::dialog::confirmation_surface;

/// The width of a message box. Wide enough for three answers in a row, in either language.
pub(crate) const MESSAGE_BOX_WIDTH: f32 = 580.;

/// The keyboard of the open message box (one for every message box: save confirmation, file changed, ask, alert, ...).
/// The box registers its buttons while it renders; `SlideEditor`'s keystroke interceptor reads them:
/// Tab and Shift+Tab move the blue ring, Enter and Space press the ringed button, Esc presses the cancel (the last
/// normal button; with none, the primary one).
#[derive(Default)]
pub struct MessageBoxKeys(RefCell<Keys>);
#[derive(Default)]
struct Keys {
    id: Option<&'static str>,
    focus: usize,
    buttons: Vec<(ActionButtonKind, bool, Option<ClickHandler>)>,
}
/// What a key did in the open message box.
pub(crate) enum KeyResult {
    /// Not a message box key (or no box registered yet).
    Ignored,
    /// Handled (the ring moved).
    Moved,
    /// Handled: run this (deferred, the click handler updates the editor).
    Press(ClickHandler),
    /// Handled: the button is disabled.
    Nothing,
}
impl MessageBoxKeys {
    /// The first button the keyboard is on: the primary one; else the first that is not destructive.
    fn initial(buttons: &[(ActionButtonKind, bool, Option<ClickHandler>)]) -> usize {
        let enabled = |i: &usize| buttons[*i].1;
        (0..buttons.len()).filter(enabled).find(|&i| matches!(buttons[i].0, ActionButtonKind::Primary))
            .or_else(|| (0..buttons.len()).filter(enabled).find(|&i| !matches!(buttons[i].0, ActionButtonKind::Destructive)))
            .unwrap_or(0)
    }
    /// The box is not showing: the next one starts from its first button.
    pub(crate) fn clear(&self) {
        let mut k = self.0.borrow_mut();
        if k.id.is_some() { *k = Keys::default(); }
    }
    /// The button the ring is on (for tests).
    pub fn focus(&self) -> usize { self.0.borrow().focus }
    /// A box registers (each draw); returns the bar with the ring on the keyboard's button.
    fn register(&self, id: &'static str, actions: ActionBar) -> ActionBar {
        let buttons = actions.key_buttons();
        let mut k = self.0.borrow_mut();
        if k.id != Some(id) { k.id = Some(id); k.focus = Self::initial(&buttons); }
        if k.focus >= buttons.len() { k.focus = 0; }
        let focus = k.focus;
        k.buttons = buttons;
        drop(k);
        actions.focus_button(focus)
    }
    pub(crate) fn key(&self, key: &str, shift: bool) -> KeyResult {
        let mut k = self.0.borrow_mut();
        let n = k.buttons.len();
        if k.id.is_none() || n == 0 { return KeyResult::Ignored; }
        let press = |i: usize, k: &Keys| match &k.buttons[i] {
            (_, true, Some(h)) => KeyResult::Press(h.clone()),
            _ => KeyResult::Nothing,
        };
        match key {
            "tab" => {
                let step = |i: usize| if shift { (i + n - 1) % n } else { (i + 1) % n };
                let mut i = step(k.focus);
                for _ in 0..n { if k.buttons[i].1 { break; } i = step(i); }
                k.focus = i;
                KeyResult::Moved
            }
            "enter" | "space" if !shift => { let f = k.focus; press(f, &k) }
            "escape" => {
                let i = k.buttons.iter().rposition(|b| matches!(b.0, ActionButtonKind::Normal))
                    .or_else(|| k.buttons.iter().position(|b| matches!(b.0, ActionButtonKind::Primary)))
                    .unwrap_or(0);
                press(i, &k)
            }
            _ => KeyResult::Ignored,
        }
    }
}

/// A message box over the whole window: input behind it is blocked (`confirmation_surface`).
/// `backdrop` (default OFF; ON only for a very important question) dims the window behind it.
pub(crate) fn message_box(id: &'static str, title: SharedString, message: SharedString, actions: ActionBar, keys: &MessageBoxKeys, backdrop: bool, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let actions = keys.register(id, actions);
    confirmation_surface(id, backdrop).child(
        v_flex()
            .id((id, 1usize))
            .w(px(MESSAGE_BOX_WIDTH))
            .max_w_full()
            .rounded_lg()
            .overflow_hidden()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .shadow_xl()
            .child(
                v_flex()
                    .p_6()
                    .gap_3()
                    .child(div().text_base().font_weight(FontWeight::SEMIBOLD).text_color(theme.foreground).child(title))
                    .child(div().text_sm().text_color(theme.muted_foreground).child(message)),
            )
            .child(action_bar_host(actions.access_scope(crate::access_key::Scope::MessageBox))),
    )
}
