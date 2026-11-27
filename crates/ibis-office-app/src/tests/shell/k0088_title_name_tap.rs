// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0088 (iOS and Android's edit screens; design: `tasks/doc/test-design/K0088.md`; the numbers FN1... are that document's): the ▾ right of the file name
//! in the title bar stays out, and a tap on the file name opens the menu the ▾ opens (ファイル名をコピー, ファイルパスをコピー, ファイル名を変更…).
//! A computer is as it was: the ▾, and nothing at the name. The mobile OS is `mobile_os::ForcedMobileOs` (K0076); the name is found by where it is drawn
//! (`title-file-name`), the ▾ by `title-file-menu` and the menu's lines by `menu-item-shell.file-name-menu.*` (`title_file_menu.rs` finds them so).
use crate::shell::mobile_os::ForcedMobileOs;
use crate::shell::scenario::{Scenario, button_probe as buttons};
use gpui_kit::*;
use ibis_formats::Kind;

const FILES: [(Kind, &str); 3] = [
    (Kind::Document, "test/editing/docx-report.docx"),
    (Kind::Workbook, "test/editing/xlsx-sales.xlsx"),
    (Kind::Presentation, "test/editing/pptx-plan.pptx"),
];

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0088_d1_a_computer_keeps_the_caret_and_the_mouse_on_the_name_opens_nothing", d1_computer),
        ("shell::k0088_d2_a_finger_on_a_computer_does_not_make_it_a_phone", d2_touch_seen),
        ("shell::k0088_fn1_no_caret_beside_the_name", fn1_no_caret),
        ("shell::k0088_fn2_a_tap_on_the_name_opens_the_files_menu", fn2_tap_opens),
        ("shell::k0088_fn3_the_menus_lines_work_as_the_carets_do", fn3_lines_work),
        ("shell::k0088_fn4_a_document_with_no_file_has_no_menu_at_the_name", fn4_no_file),
        ("shell::k0088_fn5_a_tap_behind_a_dialog_opens_nothing", fn5_modal),
        ("shell::k0088_fn6_a_long_press_and_a_drag_on_the_name_are_not_a_tap", fn6_not_a_tap),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}


fn tap(s: &mut Scenario, at: Point<Pixels>) {
    let t = |phase| PlatformInput::Touch(TouchEvent { id: TouchId(1), phase, position: at, predicted_position: None, force: None });
    s.dispatch(vec![t(TouchPhase::Started)]);
    s.advance_ms(60);
    s.dispatch(vec![t(TouchPhase::Ended)]);
    s.settle();
}

fn open(kind_file: (Kind, &str)) -> Scenario {
    let mut s = Scenario::open(kind_file.1);
    s.size(1000., 700.);
    s.settle();
    s
}

/// Whether the file name's menu is open: its lines are drawn.
fn menu_open(s: &mut Scenario) -> bool {
    buttons::clear();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    buttons::bounds("menu-item-shell.file-name-menu.copy-name").is_some()
}

fn name_at(s: &mut Scenario) -> Point<Pixels> {
    buttons::clear();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    buttons::bounds("title-file-name").unwrap_or_else(|| panic!("setup: the file name is drawn: {:?}", buttons::all())).center()
}

fn caret_drawn(s: &mut Scenario) -> bool {
    buttons::clear();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    buttons::bounds("title-file-menu").is_some()
}

/// FN1 (K0088 1): on a mobile OS the ▾ is not drawn, though the file has a path, on the three screens.
fn fn1_no_caret() {
    let _mobile = ForcedMobileOs::new();
    let _probe = buttons::Probe::new();
    for f in FILES {
        let mut s = open(f);
        assert!(!caret_drawn(&mut s), "{:?}: no ▾", f.0);
    }
}

/// FN2 (K0088 2): a finger's tap on the name opens the menu with the three lines (the same ones the ▾ opens), under the name.
fn fn2_tap_opens() {
    let _mobile = ForcedMobileOs::new();
    let _probe = buttons::Probe::new();
    for f in FILES {
        let mut s = open(f);
        let c = name_at(&mut s);
        tap(&mut s, c);
        assert!(menu_open(&mut s), "{:?}: the tap opened the menu", f.0);
        for line in ["copy-name", "copy-path", "rename"] {
            assert!(buttons::bounds(&format!("menu-item-shell.file-name-menu.{line}")).is_some(), "{:?}: {line} is in the menu", f.0);
        }
        let first = buttons::bounds("menu-item-shell.file-name-menu.copy-name").unwrap();
        assert!(f32::from(first.top()) >= f32::from(c.y) - 2.0, "{:?}: the menu hangs under the name ({first:?})", f.0);
        // Another tap, outside, closes it.
        tap(&mut s, point(px(500.), px(400.)));
        assert!(!menu_open(&mut s), "{:?}: a tap outside closed it", f.0);
    }
}

/// FN3 (K0088 3): the lines work as the ▾'s do: ファイル名をコピー puts the name (with its extension) on the clipboard; ファイル名を変更… opens the
/// rename dialog; ファイルパスをコピー puts the path.
fn fn3_lines_work() {
    let _mobile = ForcedMobileOs::new();
    let _probe = buttons::Probe::new();
    for f in FILES {
        let mut s = open(f);
        let c = name_at(&mut s);
        tap(&mut s, c);
        let b = buttons::bounds("menu-item-shell.file-name-menu.copy-name").expect("setup: the menu");
        tap(&mut s, b.center());
        let copied = s.with(|_, cx| cx.read_from_clipboard().and_then(|i| i.text())).unwrap_or_default();
        let expected = f.1.rsplit('/').next().unwrap();
        assert_eq!(copied, expected, "{:?}: the name was copied", f.0);
        let c = name_at(&mut s);
        tap(&mut s, c);
        let b = buttons::bounds("menu-item-shell.file-name-menu.rename").expect("setup: the menu again");
        tap(&mut s, b.center());
        assert!(s.with(|e, _| e.rename_file.is_some()), "{:?}: the rename dialog opened", f.0);
    }
}

/// FN4 (K0088 4): a new document (no file yet) has nothing to copy or rename: a tap on its name opens no menu (as it has no ▾ today).
fn fn4_no_file() {
    let _mobile = ForcedMobileOs::new();
    let _probe = buttons::Probe::new();
    for kind in [Kind::Document, Kind::Workbook, Kind::Presentation] {
        let mut s = Scenario::new_document(kind);
        s.size(1000., 700.);
        s.settle();
        let c = name_at(&mut s);
        tap(&mut s, c);
        assert!(!menu_open(&mut s), "{kind:?}: no menu for a document with no file");
    }
}

/// FN5 (K0088 5): with a dialog open (the ▾ is `disabled` then) a tap on the name behind it opens nothing.
fn fn5_modal() {
    let _mobile = ForcedMobileOs::new();
    let _probe = buttons::Probe::new();
    let mut s = open(FILES[1]);
    let c = name_at(&mut s);
    s.menu("tools.settings");
    s.settle();
    assert!(s.with(|e, _| e.settings.is_some()), "setup: a dialog is open");
    tap(&mut s, c);
    assert!(!menu_open(&mut s), "nothing opens behind the dialog");
}

/// FN6 (K0088 6): a touch that moves more than 8 points, or is held 500 ms (K0073's right click), is not the tap that opens the menu.
fn fn6_not_a_tap() {
    let _mobile = ForcedMobileOs::new();
    let _probe = buttons::Probe::new();
    let mut s = open(FILES[0]);
    let c = name_at(&mut s);
    let t = |phase, x: f32| PlatformInput::Touch(TouchEvent { id: TouchId(1), phase, position: point(c.x + px(x), c.y), predicted_position: None, force: None });
    s.dispatch(vec![t(TouchPhase::Started, 0.0)]);
    for i in 1..=10 {
        s.dispatch(vec![t(TouchPhase::Moved, 3.0 * i as f32)]);
        s.advance_ms(16);
    }
    s.dispatch(vec![t(TouchPhase::Ended, 30.0)]);
    s.settle();
    assert!(!menu_open(&mut s), "a drag of 30 points is no tap");
    s.dispatch(vec![t(TouchPhase::Started, 0.0)]);
    for _ in 0..40 {
        s.advance_ms(16);
    }
    s.dispatch(vec![t(TouchPhase::Ended, 0.0)]);
    s.settle();
    // (What a long press does there is K0073's: the title row has no right-click menu, so nothing opens.)
    assert!(!menu_open(&mut s), "a held touch is no tap");
}

/// D1 (unchanged; passes now): on a computer the ▾ is drawn when the file has a path, the click on it opens the menu, and a click with the mouse on the
/// name opens nothing (the name is the window's drag area).
fn d1_computer() {
    let _probe = buttons::Probe::new();
    for f in FILES {
        let mut s = open(f);
        assert!(caret_drawn(&mut s), "{:?}: the ▾", f.0);
        let c = name_at(&mut s);
        s.click((f32::from(c.x), f32::from(c.y)));
        assert!(!menu_open(&mut s), "{:?}: a click on the name opens nothing", f.0);
        buttons::click(&mut s, "title-file-menu");
        assert!(menu_open(&mut s), "{:?}: the ▾ opens the menu", f.0);
    }
}

/// D2 (unchanged; passes now): a finger that touched a computer's window (`touch_seen`) is not a phone: the ▾ stays.
fn d2_touch_seen() {
    let _probe = buttons::Probe::new();
    let mut s = open(FILES[0]);
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    assert!(caret_drawn(&mut s), "the ▾ stays on a computer that a finger touched");
}
