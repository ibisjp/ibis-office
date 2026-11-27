// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the toolbar over the soft keyboard (K0064): shown only with a soft keyboard on a touch device, the window shortened by the
//! keyboard's height, the buttons by the place, the hide button (it commits nothing), none on an iPad's attached keyboard, none with a
//! hardware keyboard, sideways scroll in a narrow window, and the screenshots in the light and the dark appearance.
use crate::shell::keyboard_toolbar::{KeyboardKind, SoftKeyboard, Target, buttons_for};
use crate::shell::narrow_layout::more_to_scroll;
use crate::shell::scenario::{Scenario, button_probe as buttons};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0064_toolbar_follows_the_keyboard", toolbar_follows_the_keyboard),
        ("shell::k0064_window_is_shortened_by_the_keyboard", window_is_shortened),
        ("shell::k0064_hide_button_keeps_the_edit", hide_button_keeps_the_edit),
        ("shell::k0064_ipad_attached_has_no_hide_button", ipad_attached),
        ("shell::k0064_buttons_by_place", buttons_by_place),
        ("shell::k0064_composing_disables_the_buttons", composing_disables),
        ("shell::k0064_narrow_toolbar_scrolls_sideways", narrow_scrolls),
        ("shell::k0064_screenshots", screenshots),
    ]
}

const PHONE_KEYBOARD: SoftKeyboard = SoftKeyboard { height: 300., kind: KeyboardKind::Docked, duration: 0.25 };

fn document(kind: ibis_formats::Kind, width: f32, keyboard: SoftKeyboard) -> Scenario {
    let mut s = Scenario::new_document(kind);
    s.size(width, 700.);
    s.update(|e, _, cx| {
        e.touch_seen = true;
        e.soft_keyboard = keyboard;
        cx.notify();
    });
    s.settle();
    s
}

fn ids(s: &mut Scenario) -> Vec<String> {
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    buttons::all().into_iter().filter(|id| id.starts_with("kbd-")).collect()
}

fn toolbar_follows_the_keyboard() {
    let mut s = document(ibis_formats::Kind::Document, 390., PHONE_KEYBOARD);
    let shown = ids(&mut s);
    assert!(shown.iter().any(|id| id == "kbd-undo") && shown.iter().any(|id| id == "kbd-hide"), "the toolbar with its hide button: {shown:?}");
    // Gone with the keyboard.
    s.update(|e, _, cx| {
        e.soft_keyboard = SoftKeyboard::HIDDEN;
        cx.notify();
    });
    s.settle();
    assert!(ids(&mut s).is_empty(), "no toolbar without a keyboard");
    // A hardware keyboard shows none; neither does a mouse (no touch).
    s.update(|e, _, cx| {
        e.soft_keyboard = SoftKeyboard { height: 55., kind: KeyboardKind::Hardware, duration: 0. };
        cx.notify();
    });
    s.settle();
    assert!(ids(&mut s).is_empty(), "none with a hardware keyboard");
    s.update(|e, _, cx| {
        e.soft_keyboard = PHONE_KEYBOARD;
        e.touch_seen = false;
        cx.notify();
    });
    s.settle();
    assert!(ids(&mut s).is_empty(), "none on a mouse's computer");
}

fn window_is_shortened() {
    let mut s = document(ibis_formats::Kind::Document, 390., SoftKeyboard::HIDDEN);
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let before = s.with(|e, _| e.workspace_bounds.unwrap());
    s.update(|e, _, cx| {
        e.soft_keyboard = PHONE_KEYBOARD;
        cx.notify();
    });
    s.settle();
    let after = s.with(|e, _| e.workspace_bounds.unwrap());
    // The document ends over the toolbar, which is over the keyboard: it leaves the keyboard's height and the toolbar's.
    let limit = 700. - 300. - crate::shell::keyboard_toolbar::KEYBOARD_TOOLBAR_HEIGHT;
    assert!(f32::from(after.bottom()) <= limit + 0.5, "the document ends at {} (limit {limit}; it was {})", f32::from(after.bottom()), f32::from(before.bottom()));
    assert!(after.bottom() < before.bottom());
}

fn hide_button_keeps_the_edit() {
    let mut s = document(ibis_formats::Kind::Workbook, 390., PHONE_KEYBOARD);
    s.type_text("12");
    let editing = s.with(|e, _| e.keyboard_target());
    assert_eq!(editing, Target::SheetValue);
    {
        let _probe = buttons::Probe::new();
        s.update(|_, _, cx| cx.notify());
        s.settle();
        buttons::click(&mut s, "kbd-hide");
        s.settle();
    }
    assert!(!s.with(|e, _| e.soft_keyboard.visible()), "the keyboard is hidden");
    assert!(ids(&mut s).is_empty(), "so is the toolbar");
    // The cell is still being typed into: nothing was committed.
    assert_eq!(s.with(|e, _| e.keyboard_target()), Target::SheetValue);
}

fn ipad_attached() {
    let mut s = document(ibis_formats::Kind::Document, 820., SoftKeyboard { height: 300., kind: KeyboardKind::DockedWithHideKey, duration: 0.25 });
    let shown = ids(&mut s);
    assert!(shown.iter().any(|id| id == "kbd-undo"), "the buttons are shown: {shown:?}");
    assert!(!shown.iter().any(|id| id == "kbd-hide"), "the keyboard has a key of its own for that: {shown:?}");
    // A floating keyboard has none of its own: the toolbar has the button, and the window is not shortened.
    s.update(|e, _, cx| {
        e.soft_keyboard = SoftKeyboard { height: 0., kind: KeyboardKind::Floating, duration: 0.25 };
        cx.notify();
    });
    s.settle();
    assert!(ids(&mut s).iter().any(|id| id == "kbd-hide"), "floating: with the hide button");
}

fn buttons_by_place() {
    for target in [Target::Text, Target::Table, Target::List, Target::Notes, Target::SheetValue, Target::SheetFormula, Target::DialogField, Target::Find, Target::ReadOnly] {
        let list = buttons_for(target);
        assert!(list.len() <= 5, "{target:?}: at most five buttons");
        let mut seen = std::collections::HashSet::new();
        assert!(list.iter().all(|b| seen.insert(b.id)), "{target:?}: no button twice");
    }
    assert_eq!(buttons_for(Target::ReadOnly).len(), 0, "a read-only place has only the hide button");
    assert_eq!(buttons_for(Target::SheetFormula).iter().map(|b| b.id).collect::<Vec<_>>(), ["open-paren", "close-paren", "range-colon", "insert-function", "toggle-reference"]);
}

fn composing_disables() {
    let mut s = document(ibis_formats::Kind::Workbook, 390., PHONE_KEYBOARD);
    s.type_text("a");
    s.ime_compose("にほん");
    let (idle, composing) = s.with(|e, _| (e.key_button_enabled_for_scenario(false), e.key_button_enabled_for_scenario(true)));
    assert!(idle, "the text buttons work when nothing is being composed");
    assert!(!composing, "and not while it is");
    // The hide button is not one of them.
    assert!(ids(&mut s).iter().any(|id| id == "kbd-hide"));
}

fn narrow_scrolls() {
    let mut s = document(ibis_formats::Kind::Workbook, 250., PHONE_KEYBOARD);
    s.type_text("=");
    assert_eq!(s.with(|e, _| e.keyboard_target()), Target::SheetFormula);
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let more = s.with(|e, _| more_to_scroll(&e.keyboard_toolbar_scroll));
    assert_eq!(more, (false, true), "five buttons do not fit 250 pt: more to the right");
}

fn screenshots() {
    for (kind, name) in [(ibis_formats::Kind::Document, "docx"), (ibis_formats::Kind::Workbook, "xlsx"), (ibis_formats::Kind::Presentation, "pptx")] {
        for dark in [false, true] {
            let mut s = document(kind, 390., PHONE_KEYBOARD);
            s.update(move |_, _, cx| crate::shell::palette::force(dark, cx));
            s.settle();
            s.screenshot(&format!("k0064-{name}-{}", if dark { "dark" } else { "light" }));
            // The forced appearance is process-wide: give the system's back, or a later scenario runs dark (xlsx::k0020 failed after this one).
            s.update(|_, _, cx| crate::shell::palette::release(cx));
        }
    }
}
