// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K-formula-bar-kbd (iOS and Android; design: `tasks/doc/test-design/K-formula-bar-keyboard.md`; the numbers FK1... are that document's): a tap on the
//! worksheet's formula bar (above the grid in a wide window, under the toolbar in a narrow touch one, K0089) brings the soft keyboard up as a tap on a cell
//! does. What the platform asks after a tap is `SlideEditor::accepts_text_now` (iOS `tick`, Android `platform.rs`): it must be true after a tap on the bar's
//! text, whether or not the keyboard was closed by the toolbar's ⌄ (K0091). The platforms' own tick and the real keyboard are for the simulators (the
//! design's section 5): no scenario here is proof of them. Raw `TouchEvent`s with the mock clock moving between them (K0065's way).
use crate::shell::keyboard_toolbar::{KeyboardKind, SoftKeyboard, Target};
use crate::shell::scenario::{Scenario, button_probe as buttons};
use crate::xlsx::scenarios::SheetSteps;
use gpui_kit::*;
use ibis_office_xlsx::ext::*;
use ibis_office_xlsx::sheet_ui::SheetUi;

const PHONE_KEYBOARD: SoftKeyboard = SoftKeyboard { height: 300., kind: KeyboardKind::Docked, duration: 0.25 };

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::kfbk_fk1_a_tap_on_the_formula_bar_brings_the_keyboard_up_wide_and_narrow", fk1_bar_tap_asks_for_the_keyboard),
        ("shell::kfbk_fk2_the_typing_after_the_tap_goes_to_the_cell_through_the_bar", fk2_typing_goes_to_the_cell),
        ("shell::kfbk_fk3_after_the_toolbars_hide_button_a_tap_on_the_bar_brings_it_back", fk3_bar_tap_after_hide),
        ("shell::kfbk_fk4_after_the_hide_button_taps_on_the_bar_that_are_not_text_places_do_not", fk4_expand_button_does_not),
        ("shell::kfbk_fk5_a_tap_on_the_name_box_brings_the_keyboard_up", fk5_name_box),
        ("shell::kfbk_fk6_a_read_only_workbook_never_brings_it_up_by_the_bar", fk6_read_only),
        ("shell::kfbk_fk7_a_tap_on_a_cell_and_the_hide_button_work_as_before", fk7_cells_as_before),
        ("shell::kfbk_fk8_after_a_tap_elsewhere_and_over_the_frames_the_bar_tap_takes_text", fk8_after_other_tap_and_frames),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}

/// A touch window with the workbook, A1 selected, and no soft keyboard (`wide`: 1200 x 800; otherwise a phone's 390 x 700, where the bar is under the toolbar).
fn book(wide: bool) -> Scenario {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    if wide {
        s.size(1200., 800.);
    } else {
        s.size(390., 700.);
    }
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    s.click_cell("A1");
    s
}

fn tap(s: &mut Scenario, at: Point<Pixels>) {
    let t = |phase| PlatformInput::Touch(TouchEvent { id: TouchId(1), phase, position: at, predicted_position: None, force: None });
    s.dispatch(vec![t(TouchPhase::Started)]);
    s.advance_ms(60);
    s.dispatch(vec![t(TouchPhase::Ended)]);
    s.settle();
}

fn accepts(s: &mut Scenario) -> bool {
    s.update(|e, window, cx| e.accepts_text_now(window, cx))
}

/// The formula bar as drawn: its text field's bounds (the bar's whole row is a little taller).
fn bar(s: &mut Scenario) -> Bounds<Pixels> {
    s.update(|_, _, cx| cx.notify());
    s.settle();
    s.with(|e, _| e.sheet_state().and_then(|st| st.bar_bounds.get())).expect("the formula bar is drawn")
}

fn bar_text_point(s: &mut Scenario) -> Point<Pixels> {
    let b = bar(s);
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    buttons::bounds("formula-bar-text").map(|t| t.center()).unwrap_or_else(|| b.center())
}

/// The name box's middle: the left end of the bar's row (a mark `formula-bar-name-box` is used when the bar has one).
fn name_box_point(s: &mut Scenario) -> Point<Pixels> {
    let b = bar(s);
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    buttons::bounds("formula-bar-name-box").map(|t| t.center()).unwrap_or_else(|| point(px(40.), b.center().y))
}

fn close_keyboard(s: &mut Scenario) {
    s.update(|e, _, cx| {
        e.soft_keyboard = PHONE_KEYBOARD;
        cx.notify();
    });
    s.settle();
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    buttons::click(s, "kbd-hide");
    s.settle();
}

/// FK1 (the request): the keyboard is not up, A1 is selected; a finger on the bar's text, in a wide window (the bar above the grid) and in a narrow one (the
/// bar under the toolbar), and the platform's check after the tap is told "text input": the cell is being typed into through the bar.
fn fk1_bar_tap_asks_for_the_keyboard() {
    for wide in [true, false] {
        let mut s = book(wide);
        assert!(!s.with(|e, _| e.soft_keyboard.visible()), "wide {wide}: setup: no keyboard");
        let at = bar_text_point(&mut s);
        tap(&mut s, at);
        assert!(s.with(|e, _| e.cell_edit().is_some()), "wide {wide}: the tap begins the typing into the cell (K0089's FB2)");
        assert!(accepts(&mut s), "wide {wide}: the check after the tap is told the window takes text: the keyboard comes up");
    }
}

/// FK2: with the cell being typed into through the bar, the characters of the keyboard go to the cell: "ab" typed after the tap is in the bar's text and the
/// cell takes it on Enter.
fn fk2_typing_goes_to_the_cell() {
    for wide in [true, false] {
        let mut s = book(wide);
        let at = bar_text_point(&mut s);
        tap(&mut s, at);
        s.key("cmd-a");
        s.type_text("kb");
        assert!(accepts(&mut s), "wide {wide}: the window still takes text while typing");
        s.key("enter");
        assert_eq!(s.shown("A1"), "kb", "wide {wide}: the text typed after the bar's tap is the cell's");
    }
}

/// FK3 (K0091 with the bar): the keyboard was closed by the toolbar's ⌄ (the check says no); a tap on the bar's text is going to type: the check says yes
/// again (as a tap on a cell's text does: K0091's KC5).
fn fk3_bar_tap_after_hide() {
    for wide in [true, false] {
        let mut s = book(wide);
        close_keyboard(&mut s);
        assert!(!accepts(&mut s), "wide {wide}: setup: closed by the ⌄, the check says no");
        let at = bar_text_point(&mut s);
        tap(&mut s, at);
        assert!(accepts(&mut s), "wide {wide}: a tap on the bar's text brings the keyboard back");
    }
}

/// FK4: the bar's ▼ (expand) is a button, not a place to type in: after the ⌄ a tap on it does not bring the keyboard back (K0091's KC6: only a place to type in
/// does); the bar's `fx` (a dialog) likewise.
fn fk4_expand_button_does_not() {
    for wide in [true, false] {
        let mut s = book(wide);
        close_keyboard(&mut s);
        let b = bar(&mut s);
        // The ▼ is at the right end of the bar's row.
        let at = point(b.right() + px(10.), b.center().y);
        tap(&mut s, at);
        assert!(!accepts(&mut s), "wide {wide}: the expand button is not a place to type in: the keyboard stays closed");
    }
}

/// FK5 (decision 1 of the design): a tap on the name box (the cell's name at the bar's left) brings the keyboard up: a reference is typed there. The check is told yes
/// in both layouts, with and without the ⌄ before it.
fn fk5_name_box() {
    for wide in [true, false] {
        for closed in [false, true] {
            let mut s = book(wide);
            if closed {
                close_keyboard(&mut s);
            }
            let at = name_box_point(&mut s);
            tap(&mut s, at);
            assert!(accepts(&mut s), "wide {wide}, closed {closed}: a tap on the name box brings the keyboard up");
        }
    }
}

/// FK6: a workbook that cannot be edited (read-only): a tap on the bar does not bring the keyboard up (nothing is typed there), in both layouts.
fn fk6_read_only() {
    for wide in [true, false] {
        let mut s = book(wide);
        s.update(|e, _, cx| {
            e.read_only = true;
            cx.notify();
        });
        s.settle();
        let at = bar_text_point(&mut s);
        tap(&mut s, at);
        assert!(!accepts(&mut s), "wide {wide}: read-only: no keyboard for the bar");
        assert_eq!(s.with(|e, _| e.keyboard_target()), Target::ReadOnly);
    }
}

/// FK7 (regression): the cell's own tap brings the keyboard up as before (the check says yes after a tap on a cell); the ⌄ then closes it (K0091's KC2) and a tap on a
/// cell brings it back (KC5), in both layouts.
fn fk7_cells_as_before() {
    for wide in [true, false] {
        let mut s = book(wide);
        let cell = s.with(|e, _| e.cell_center(1, 1)).expect("a cell is on screen");
        tap(&mut s, cell);
        assert!(accepts(&mut s), "wide {wide}: a tap on a cell: the keyboard comes up");
        close_keyboard(&mut s);
        assert!(!accepts(&mut s), "wide {wide}: closed by the ⌄");
        let cell = s.with(|e, _| e.cell_center(1, 2)).expect("a cell is on screen");
        tap(&mut s, cell);
        assert!(accepts(&mut s), "wide {wide}: a tap on a cell brings it back");
    }
}

/// FK8 (the user's "the keyboard is not up", the other ways to it): after a tap that is no place to type in (the status bar's row), and the 3 frames the platform asks again over
/// (`KEYBOARD_CHECK_TICKS`), a tap on the bar's text begins the cell's edit and the window takes text at each of the frames after it (the edit is begun by the press, before the
/// first of the platform's questions).
fn fk8_after_other_tap_and_frames() {
    for wide in [true, false] {
        let mut s = book(wide);
        let elsewhere = point(px(195.), px(if wide { 790. } else { 690. }));
        tap(&mut s, elsewhere);
        let at = bar_text_point(&mut s);
        let t = |phase| PlatformInput::Touch(TouchEvent { id: TouchId(1), phase, position: at, predicted_position: None, force: None });
        s.dispatch(vec![t(TouchPhase::Started)]);
        s.advance_ms(60);
        s.dispatch(vec![t(TouchPhase::Ended)]);
        for frame in 0..3 {
            assert!(s.with(|e, _| e.cell_edit().is_some()), "wide {wide}: the cell's edit has begun at frame {frame} after the tap");
            assert!(accepts(&mut s), "wide {wide}: the window takes text at frame {frame} after the tap");
            s.advance_ms(16);
        }
    }
}
