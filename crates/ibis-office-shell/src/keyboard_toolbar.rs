// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The toolbar over the soft keyboard (K0064; design `tasks/doc/design/2026-11-24-k0064-keyboard-toolbar.md`).
//!
//! * The OS tells the keyboard's height, kind and the time it takes to move (iOS `UIKeyboardWillChangeFrameNotification`, Android
//!   `WindowInsets` ime): [`SoftKeyboard`], kept in the window's editor (`SlideEditor::soft_keyboard`), read from the platform as the
//!   window is laid out ([`os_keyboard`]). The window is shortened by the keyboard's height and the toolbar sits at its bottom.
//! * The toolbar is drawn here (GPUI), not by the OS: what the buttons do depends on where the user is typing, and they run the same
//!   commands the menus and the toolbar run. Which buttons are shown for which place is data ([`buttons_for`]).
//! * The ⌄ at the right end hides the keyboard (the same picture as the formula bar's ▼). It leaves what is being edited as it is: the
//!   caret, the selection and a cell being typed into stay (no commit). An iPad's attached keyboard has a key of its own for that, so
//!   there it is not shown ([`SoftKeyboard::has_hide_button`]).
//! * While text is being composed (an IME's marked text) the buttons that act on text are disabled: a press would drop or double the
//!   marked text. The hide button works (it commits the marked text, as the OS does when the keyboard goes).

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, h_flex};
use gpui_kit::*;

use crate::chrome::icon;
use crate::editor::SlideEditor;
use crate::format::with_format;
use crate::palette::rgb;

/// The toolbar's height: the smallest size a finger presses (iOS 44 pt, Android 48 dp).
pub const KEYBOARD_TOOLBAR_HEIGHT: f32 = if cfg!(target_os = "android") { 48.0 } else { 44.0 };
/// The width of a button (it is as high as the bar).
const BUTTON_WIDTH: f32 = KEYBOARD_TOOLBAR_HEIGHT;
const BUTTON_GAP: f32 = 4.0;
/// The keyboard's own colour as the toolbar shows it: light grey with the light appearance, dark grey with the dark one (the theme's
/// two colours: `palette::rgb` turns the light one into the dark).
const BAR_BG: u32 = 0xd2d5db;
const BAR_EDGE: u32 = 0xb4b8c0;

/// The kind of the soft keyboard (what the OS says of its frame).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardKind {
    /// Across the bottom of the window, with no key of its own to hide it (an iPhone's, Android's).
    Docked,
    /// The same on an iPad: its bottom right key hides it.
    DockedWithHideKey,
    /// A small or split keyboard (an iPad's) that covers no edge of the window.
    Floating,
    /// A hardware keyboard is connected: nothing, or only a bar of suggestions, is shown.
    Hardware,
}

/// The soft keyboard now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoftKeyboard {
    /// How much of the window's bottom it covers (points). 0 for a floating one.
    pub height: f32,
    pub kind: KeyboardKind,
    /// The seconds it takes to move.
    pub duration: f32,
}

impl SoftKeyboard {
    /// No keyboard.
    pub const HIDDEN: SoftKeyboard = SoftKeyboard { height: 0.0, kind: KeyboardKind::Docked, duration: 0.0 };

    /// A soft keyboard is on the screen: across the bottom (it covers some height), or floating. (A hardware keyboard shows none.)
    pub fn visible(&self) -> bool {
        match self.kind {
            KeyboardKind::Hardware => false,
            KeyboardKind::Floating => true,
            KeyboardKind::Docked | KeyboardKind::DockedWithHideKey => self.height > 0.5,
        }
    }

    /// The toolbar is shown with it.
    pub fn shows_toolbar(&self) -> bool {
        self.visible()
    }

    /// The toolbar has the button that hides the keyboard: not with an iPad's attached keyboard, which has a key for that.
    pub fn has_hide_button(&self) -> bool {
        self.kind != KeyboardKind::DockedWithHideKey
    }
}

/// The keyboard the OS shows, as the platform last told it (the other systems have no soft keyboard: hidden).
#[cfg(target_os = "ios")]
pub fn os_keyboard() -> SoftKeyboard {
    let info = gpui_ios::keyboard();
    SoftKeyboard {
        height: info.height,
        duration: info.duration,
        kind: match info.kind {
            gpui_ios::KeyboardKind::Docked => KeyboardKind::Docked,
            gpui_ios::KeyboardKind::DockedWithHideKey => KeyboardKind::DockedWithHideKey,
            gpui_ios::KeyboardKind::Floating => KeyboardKind::Floating,
            gpui_ios::KeyboardKind::Hardware => KeyboardKind::Hardware,
        },
    }
}

#[cfg(target_os = "android")]
pub fn os_keyboard() -> SoftKeyboard {
    let info = gpui_android::keyboard();
    SoftKeyboard {
        height: info.height,
        duration: info.duration,
        kind: match info.kind {
            gpui_android::KeyboardKind::Docked => KeyboardKind::Docked,
            gpui_android::KeyboardKind::DockedWithHideKey => KeyboardKind::DockedWithHideKey,
            gpui_android::KeyboardKind::Floating => KeyboardKind::Floating,
            gpui_android::KeyboardKind::Hardware => KeyboardKind::Hardware,
        },
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
pub fn os_keyboard() -> SoftKeyboard {
    SoftKeyboard::HIDDEN
}

/// Where the user is typing: what decides the buttons (design 2.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// The text of a document, or of a shape.
    Text,
    /// A table's cell (a document's, a slide's).
    Table,
    /// A list (bullets, numbers) in a document or a shape.
    List,
    /// The notes of a slide.
    Notes,
    /// A sheet's cell, a value being typed (or the formula bar with one).
    SheetValue,
    /// A sheet's cell, or the formula bar, with a formula (it begins with `=`).
    SheetFormula,
    /// A text field of a dialog or a panel.
    DialogField,
    /// The find (and replace) field.
    Find,
    /// A file that cannot be edited: only the hide button.
    ReadOnly,
}

/// What a button is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    /// A Material icon.
    Icon(&'static str),
    /// A character of its own (a bracket, a colon): not translated.
    Text(&'static str),
}

/// What a button does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Does {
    /// The menu command of this id (`edit.undo`): enabled when the menu's line is.
    Menu(&'static str),
    /// The toolbar button of this id (`bold`): enabled when the button is.
    Tool(&'static str),
    /// A key the soft keyboard has not (`left`, `tab`): as the hardware key does.
    Key(&'static str),
    /// Characters typed into the text (a bracket of a formula): as the keyboard does.
    Type(&'static str),
}

/// A button of the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyButton {
    /// The button's id (`kbd-undo`) and the end of its i18n id (`shell.keyboard-toolbar.undo`: its name for VoiceOver and TalkBack).
    pub id: &'static str,
    pub glyph: Glyph,
    pub does: Does,
}

const fn key(id: &'static str, glyph: Glyph, does: Does) -> KeyButton {
    KeyButton { id, glyph, does }
}

const UNDO: KeyButton = key("undo", Glyph::Icon("undo"), Does::Menu("edit.undo"));
const LEFT: KeyButton = key("caret-left", Glyph::Icon("arrow_back"), Does::Key("left"));
const RIGHT: KeyButton = key("caret-right", Glyph::Icon("arrow_forward"), Does::Key("right"));
const BOLD: KeyButton = key("bold", Glyph::Icon("format_bold"), Does::Tool("bold"));
const BULLETS: KeyButton = key("bullets", Glyph::Icon("format_list_bulleted"), Does::Tool("bullets"));
const PREVIOUS_CELL: KeyButton = key("previous-cell", Glyph::Icon("keyboard_tab_rtl"), Does::Key("shift-tab"));
const NEXT_CELL: KeyButton = key("next-cell", Glyph::Icon("keyboard_tab"), Does::Key("tab"));
// Tab and Shift+Tab, which the soft keyboard has not: a list's level down (deeper) and up.
const LOWER_LEVEL: KeyButton = key("lower-level", Glyph::Icon("format_indent_increase"), Does::Key("tab"));
const RAISE_LEVEL: KeyButton = key("raise-level", Glyph::Icon("format_indent_decrease"), Does::Key("shift-tab"));
// A sheet: Enter (down) and Tab (right) fill a table in.
const COMMIT_DOWN: KeyButton = key("commit-down", Glyph::Icon("arrow_downward"), Does::Key("enter"));
const COMMIT_RIGHT: KeyButton = key("commit-right", Glyph::Icon("keyboard_tab"), Does::Key("tab"));
const OPEN_PAREN: KeyButton = key("open-paren", Glyph::Text("("), Does::Type("("));
const CLOSE_PAREN: KeyButton = key("close-paren", Glyph::Text(")"), Does::Type(")"));
const RANGE_COLON: KeyButton = key("range-colon", Glyph::Text(":"), Does::Type(":"));
const INSERT_FUNCTION: KeyButton = key("insert-function", Glyph::Icon("function"), Does::Menu("xlsx.insert-function"));
const TOGGLE_REFERENCE: KeyButton = key("toggle-reference", Glyph::Text("F4"), Does::Key("f4"));
const PREVIOUS_FIELD: KeyButton = key("previous-field", Glyph::Icon("arrow_upward"), Does::Key("shift-tab"));
const NEXT_FIELD: KeyButton = key("next-field", Glyph::Icon("arrow_downward"), Does::Key("tab"));
const FIND_PREVIOUS: KeyButton = key("find-previous", Glyph::Icon("arrow_upward"), Does::Key("shift-enter"));
const FIND_NEXT: KeyButton = key("find-next", Glyph::Icon("arrow_downward"), Does::Key("enter"));

/// The buttons for a place, left to right (not the hide button: it is at the right end). At most five (design 2.1).
pub fn buttons_for(target: Target) -> &'static [KeyButton] {
    match target {
        Target::Text => &[UNDO, LEFT, RIGHT, BOLD, BULLETS],
        Target::Table => &[UNDO, LEFT, RIGHT, PREVIOUS_CELL, NEXT_CELL],
        Target::List => &[UNDO, LEFT, RIGHT, LOWER_LEVEL, RAISE_LEVEL],
        Target::Notes => &[UNDO, LEFT, RIGHT],
        Target::SheetValue => &[COMMIT_DOWN, COMMIT_RIGHT, UNDO, LEFT, RIGHT],
        Target::SheetFormula => &[OPEN_PAREN, CLOSE_PAREN, RANGE_COLON, INSERT_FUNCTION, TOGGLE_REFERENCE],
        Target::DialogField => &[PREVIOUS_FIELD, NEXT_FIELD],
        Target::Find => &[FIND_PREVIOUS, FIND_NEXT],
        Target::ReadOnly => &[],
    }
}

/// The i18n id of a button's name (the tooltip is not shown on a finger: this is what VoiceOver and TalkBack read).
pub fn label_id(button: &KeyButton) -> String {
    format!("shell.keyboard-toolbar.{}", button.id)
}

/// The i18n id of the hide button's name.
pub const HIDE_LABEL: &str = "shell.keyboard-toolbar.hide";

impl SlideEditor {
    /// The keyboard the window is laid out with: the OS's (iOS, Android), or the one a scenario put (`soft_keyboard`).
    pub(crate) fn sync_soft_keyboard(&mut self) {
        if cfg!(any(target_os = "ios", target_os = "android")) {
            let now = os_keyboard();
            if now != self.soft_keyboard {
                self.soft_keyboard = now;
            }
        }
    }

    /// The toolbar is shown now: a soft keyboard is on the screen of a touch device.
    pub(crate) fn keyboard_toolbar_shown(&self) -> bool {
        self.has_document && self.is_touch_device() && self.soft_keyboard.shows_toolbar()
    }

    /// Where the user is typing now.
    pub fn keyboard_target(&self) -> Target {
        if self.is_read_only() {
            Target::ReadOnly
        } else if self.dialog_is_modal() {
            Target::DialogField
        } else {
            with_format!(self, soft_keyboard_target(self)).unwrap_or(Target::Text)
        }
    }

    /// Hide the soft keyboard (the toolbar's ⌄). What is being edited stays as it is: nothing is committed, the caret and the selection
    /// are kept. Tapping the text again brings the keyboard back.
    pub fn hide_soft_keyboard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // K0091: closed until the user taps a place to type in (`on_touch_session`): the window says "no text input" meanwhile, so the platform's check
        // after the ⌄'s own tap does not show the keyboard again. A text field (the comment's, the find pane's) loses the focus to the editor, which
        // takes no text now.
        // What is being composed is put in the text first, as the OS's unmarkText does when the keyboard goes: with the keyboard closed the window takes no
        // text input, and the composing text left over would be drawn on, and typed again by the keyboard when it comes back.
        self.commit_composition(window, cx);
        self.keyboard_closed = true;
        self.focus_handle.focus(window, cx);
        #[cfg(target_os = "ios")]
        gpui_ios::hide_keyboard();
        #[cfg(target_os = "android")]
        gpui_android::hide_keyboard();
        // (With no OS keyboard — a scenario — it is gone at once. On a device the OS says so when it has moved, and the window is laid
        // out again.)
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            self.soft_keyboard = SoftKeyboard::HIDDEN;
        }
        cx.notify();
    }

    /// The text an input method is composing becomes the text (it is typed in over the composition, which ends), as the OS's unmarkText makes it; nothing
    /// composing: nothing happens. A screen whose composing text is its own (a cell, a text box) is told the composition is over.
    pub(crate) fn commit_composition(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if with_format!(self, marked_text_range(self, window, cx)).is_none() {
            return;
        }
        match with_format!(self, marked_text(self)) {
            Some(text) => with_format!(self, replace_text_in_range(self, None, &text, window, cx)),
            None => with_format!(self, unmark_text(self, window, cx)),
        }
    }

    /// For the scenarios: whether an input method is composing now (`marked_text_range`).
    #[cfg(feature = "scenarios")]
    pub fn composing_for_scenario(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        with_format!(self, marked_text_range(self, window, cx)).is_some()
    }

    /// For the scenarios: whether the text buttons work, with a composition going on or not.
    #[cfg(feature = "scenarios")]
    pub fn key_button_enabled_for_scenario(&self, composing: bool) -> bool {
        self.key_button_enabled(Does::Key("left"), composing)
    }

    /// Whether a button can be pressed now.
    fn key_button_enabled(&self, does: Does, composing: bool) -> bool {
        if composing {
            return false;
        }
        match does {
            Does::Key(_) | Does::Type(_) => true,
            Does::Menu(id) => self.menus().find_mut(id).is_some_and(|item| item.enabled()),
            Does::Tool(id) => self.toolbar_items().iter().any(|item| matches!(item, crate::chrome::ToolItem::Button(b) if b.id == id && b.enabled())),
        }
    }

    /// Press a button: the same command the menu or the toolbar runs, or the key.
    pub fn press_key_button(&mut self, button: &KeyButton, window: &mut Window, cx: &mut Context<Self>) {
        match button.does {
            Does::Menu(id) => {
                let handler = self.menus().find_mut(id).and_then(|item| item.enabled().then(|| item.handler.clone()).flatten());
                if let Some(handler) = handler {
                    crate::chrome::dispatch(&handler, self, window, cx);
                }
            }
            Does::Tool(id) => {
                let handler = self.toolbar_items().into_iter().find_map(|item| match item {
                    crate::chrome::ToolItem::Button(b) if b.id == id && b.enabled() => b.left_action().cloned().or(b.handler),
                    _ => None,
                });
                if let Some(handler) = handler {
                    crate::chrome::dispatch(&handler, self, window, cx);
                }
            }
            Does::Key(keys) => {
                // Pressed from the toolbar's own click: the key goes to the editor after this update ends (it is a key of the window).
                if let Ok(keystroke) = Keystroke::parse(keys) {
                    window.defer(cx, move |window, cx| {
                        window.dispatch_keystroke(keystroke, cx);
                    });
                }
            }
            Does::Type(text) => {
                with_format!(self, replace_text_in_range(self, None, text, window, cx));
            }
        }
        cx.notify();
    }

    /// The toolbar over the keyboard. `width` is the window's.
    pub(crate) fn render_keyboard_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let target = self.keyboard_target();
        let composing = with_format!(self, marked_text_range(self, window, cx)).is_some();
        let hide_button = self.soft_keyboard.has_hide_button();
        let scroll = self.keyboard_toolbar_scroll.clone();
        let text = |id: &str| crate::i18n::text(id);
        let enabled_color: Hsla = rgb(crate::chrome::ICON).into();
        let disabled_color: Hsla = crate::palette::shade(0.12, 0.38);
        let buttons: Vec<AnyElement> = buttons_for(target)
            .iter()
            .map(|button| {
                let enabled = self.key_button_enabled(button.does, composing);
                let color = if enabled { enabled_color } else { disabled_color };
                let label = text(&label_id(button));
                let id = SharedString::from(format!("kbd-{}", button.id));
                let content = match button.glyph {
                    Glyph::Icon(name) => icon(name, 22.0, color).into_any_element(),
                    Glyph::Text(chars) => div().text_size(px(18.)).text_color(color).child(chars).into_any_element(),
                };
                let button = *button;
                observe(Button::new(id.clone()), id)
                    .ghost()
                    .w(px(BUTTON_WIDTH))
                    .h(px(KEYBOARD_TOOLBAR_HEIGHT))
                    .flex_shrink_0()
                    .rounded(px(6.))
                    .disabled(!enabled)
                    .tooltip(label)
                    .child(content)
                    .on_click(cx.listener(move |this, _, window, cx| this.press_key_button(&button, window, cx)))
                    .into_any_element()
            })
            .collect();
        let hide = hide_button.then(|| {
            let id = SharedString::from("kbd-hide");
            h_flex()
                .flex_none()
                .items_center()
                .child(div().w(px(1.)).h(px(24.)).bg(rgb(crate::chrome::BORDER)))
                .child(
                    observe(Button::new(id.clone()), id)
                        .ghost()
                        .w(px(BUTTON_WIDTH))
                        .h(px(KEYBOARD_TOOLBAR_HEIGHT))
                        .rounded(px(6.))
                        .tooltip(text(HIDE_LABEL))
                        // The same picture as the formula bar's ▼ at its right end (a chevron pointing down).
                        .child(icon("keyboard_arrow_down", 24.0, enabled_color))
                        .on_click(cx.listener(|this, _, window, cx| this.hide_soft_keyboard(window, cx))),
                )
        });
        h_flex()
            .id("keyboard-toolbar")
            .w_full()
            .h(px(KEYBOARD_TOOLBAR_HEIGHT))
            .flex_none()
            .items_center()
            .bg(rgb(BAR_BG))
            .border_t_1()
            .border_color(rgb(BAR_EDGE))
            .child(
                // The buttons scroll sideways when they do not fit (as K0061's bars), with a shadow at the end that has more.
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(div().id("keyboard-toolbar-scroll").flex().items_center().size_full().overflow_x_scroll().track_scroll(&scroll).child(h_flex().flex_none().gap(px(BUTTON_GAP)).px(px(BUTTON_GAP)).children(buttons)))
                    .child(crate::narrow_layout::edge_shadows(&scroll)),
            )
            .children(hide)
            .into_any_element()
    }
}

#[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux", windows)))]
fn observe(button: Button, id: SharedString) -> Button {
    super::scenario::button_probe::observe(button, ElementId::Name(id))
}
#[cfg(not(all(feature = "scenarios", any(target_os = "macos", target_os = "linux", windows))))]
fn observe(button: Button, _id: SharedString) -> Button {
    button
}
