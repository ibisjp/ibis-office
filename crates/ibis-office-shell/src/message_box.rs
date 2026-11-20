// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The message box: a title, a message and the action bar, over the window's modal backdrop.
//!
//! A screen gives the strings and the `ActionBar` (its buttons are `ActionDefinition`s, so a
//! dangerous answer is `ActionButtonKind::Destructive`, red as everywhere); this part builds
//! the box, its padding and the separator of the bar.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
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
    /// A format's own message box (`render_message_box`: the slide show's questions) is the open one: the window's dialog stack, which
    /// clears the keys when no layer box shows, leaves it.
    format_owned: bool,
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
        if k.id.is_some() && !k.format_owned { *k = Keys::default(); }
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
        k.format_owned = false;
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
    message_box_at(id, title, message, actions, keys, backdrop, None, cx)
}

/// Where a message box that belongs beside something on the page is put (a rejected cell entry: the cell stays in view),
/// and who is told the size it was drawn at (to decide whether the whole box fits there).
pub(crate) struct MessageBoxPlace {
    pub origin: Option<Point<Pixels>>,
    pub measured: Box<dyn Fn(Bounds<Pixels>, &mut App) + 'static>,
}

/// `message_box`, put at `place` (centred when `None` or without an origin).
pub(crate) fn message_box_at(id: &'static str, title: SharedString, message: SharedString, actions: ActionBar, keys: &MessageBoxKeys, backdrop: bool, place: Option<MessageBoxPlace>, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let actions = keys.register(id, actions);
    let (origin, measured) = match place { Some(p) => (p.origin, Some(p.measured)), None => (None, None) };
    confirmation_surface(id, backdrop).child(
        v_flex()
            .id((id, 1usize))
            .relative()
            .when_some(origin, |d, p| d.absolute().left(p.x).top(p.y))
            .when_some(measured, |d, measured| d.child(canvas(|_, _, _| (), move |bounds, _, _, cx| measured(bounds, cx)).absolute().inset_0()))
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

impl crate::editor::SlideEditor {
    /// A message box a format draws over its own full-window view (the slide show: it is not a window dialog layer): the shared
    /// box and action bar, with this editor's keyboard state. The format sends the keys to `message_box_keystroke`.
    pub fn render_message_box(&self, id: &'static str, title: SharedString, message: SharedString, actions: ActionBar, backdrop: bool, cx: &App) -> Stateful<Div> {
        let bar = message_box(id, title, message, actions, &self.msgbox, backdrop, cx);
        self.msgbox.0.borrow_mut().format_owned = true;
        bar
    }

    /// The keys of the message box `render_message_box` draws: Alt+letter (Windows and Linux), Tab, Enter, Space and Esc.
    /// `true`: the key was the box's (the caller stops it).
    pub fn message_box_keystroke(&mut self, keystroke: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        use crate::access_key::{Pressed, Scope, dispatch};
        match dispatch(window, keystroke, Scope::MessageBox) {
            Pressed::No => {}
            Pressed::Disabled => return true,
            Pressed::Click(handler) => { window.defer(cx, move |w, cx| handler(&ClickEvent::default(), w, cx)); return true; }
        }
        let m = &keystroke.modifiers;
        if m.platform || m.control || m.alt { return false; }
        match self.msgbox.key(keystroke.key.as_str(), m.shift) {
            KeyResult::Ignored => false,
            KeyResult::Moved => { cx.notify(); true }
            KeyResult::Nothing => true,
            KeyResult::Press(handler) => { window.defer(cx, move |w, cx| handler(&ClickEvent::default(), w, cx)); true }
        }
    }

    /// A window of the slide show on another display starts a draw: the access keys of its message box register afresh.
    pub fn begin_message_box_frame(window: &Window) { crate::access_key::begin_frame(window); }
}
