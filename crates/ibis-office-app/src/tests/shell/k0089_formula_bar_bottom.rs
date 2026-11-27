// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0089: in a narrow window of a touch device (`narrow_layout::bars_at_bottom`) the spreadsheet screen's formula bar is at the window's bottom too: below
//! the main toolbar, the last row (design: `tasks/doc/test-design/K0089.md`; the numbers FB1... are that document's). K0061's bottom order is the
//! document, the status bar, the menu bar, the toolbar; the formula bar was the top row of the document. A wide window and a narrow one with a mouse
//! only are as they were (the formula bar above the grid, under the toolbar). With the soft keyboard up (K0064) the bars at the bottom give way to the
//! keyboard's toolbar; the design's proposal is that the formula bar stays, just above the keyboard's toolbar (question 1 of the design).
use crate::shell::keyboard_toolbar::{KeyboardKind, SoftKeyboard};
use crate::shell::scenario::{Scenario, button_probe as buttons};
use crate::xlsx::scenarios::SheetSteps;
use gpui_kit::*;
use ibis_office_xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0089_fb1_the_formula_bar_is_the_last_row_below_the_toolbar", fb1_order),
        ("shell::k0089_fb2_the_formula_bar_still_edits_the_cell", fb2_edits),
        ("shell::k0089_fb3_a_wide_window_and_a_mouse_keep_the_formula_bar_above_the_grid", fb3_unchanged),
        ("shell::k0089_fb4_the_expanded_formula_bar_grows_upward_and_overlaps_nothing", fb4_expanded),
        ("shell::k0089_fb5_with_the_soft_keyboard_the_formula_bar_stays_above_the_keyboards_toolbar", fb5_keyboard),
        ("shell::k0089_fb6_the_layout_follows_the_window_and_the_finger", fb6_follows),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    vec![]
}

fn book(width: f32, height: f32, touch: bool) -> Scenario {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(width, height);
    s.update(move |e, _, cx| {
        e.touch_seen = touch;
        cx.notify();
    });
    s.settle();
    s
}

/// The rows of a spreadsheet window, as drawn (tops and bottoms in points): the formula bar, the toolbar, the menu bar's first button, the first cell's
/// middle.
#[derive(Debug, Clone, Copy)]
struct Rows {
    formula: (f32, f32),
    toolbar: (f32, f32),
    menu_top: f32,
    first_cell_y: f32,
}

fn rows(s: &mut Scenario) -> Rows {
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let formula = s.with(|e, _| e.sheet_state().and_then(|st| st.bar_bounds.get())).expect("the formula bar is drawn");
    let toolbar = s.with(|e, _| e.toolbar_bounds.get());
    let menu = buttons::bounds("menu-button-menu.file").or_else(|| buttons::bounds("menu-button-menu.edit")).unwrap_or_else(|| panic!("a menu of the bar: {:?}", buttons::all()));
    let cell = s.with(|e, _| e.cell_center(0, 0)).expect("a cell is on screen");
    Rows { formula: (f32::from(formula.top()), f32::from(formula.bottom())), toolbar: (f32::from(toolbar.top()), f32::from(toolbar.bottom())), menu_top: f32::from(menu.top()), first_cell_y: f32::from(cell.y) }
}

/// FB1 (the request): under 720 px on a touch device (719 and 400, 874 high) the formula bar is below the toolbar and is the last row: from the top the
/// grid, the menu bar, the toolbar, the formula bar; it is inside the window and the grid is not under it.
fn fb1_order() {
    for width in [719.0, 400.0] {
        let mut s = book(width, 874.0, true);
        let r = rows(&mut s);
        assert!(r.first_cell_y < r.menu_top, "{width}: the grid is above the menu bar ({r:?})");
        assert!(r.menu_top <= r.toolbar.0 + 1.0, "{width}: the menu bar is above the toolbar ({r:?})");
        assert!(r.toolbar.1 <= r.formula.0 + 1.0, "{width}: the formula bar is below the toolbar ({r:?})");
        assert!(r.formula.1 <= 874.5, "{width}: the formula bar is inside the window ({r:?})");
        assert!(r.formula.1 >= 874.0 - 1.0 - 8.0, "{width}: and is the last row, at the window's bottom ({r:?})");
        assert!(r.formula.0 > r.first_cell_y + 100.0, "{width}: the formula bar is not above the grid ({r:?})");
    }
}

/// FB2: the bar at the bottom works: a tap on its text, typing and Enter put the text in the cell (A1) the bar belongs to.
fn fb2_edits() {
    let mut s = book(719.0, 874.0, true);
    let r = rows(&mut s);
    s.click_cell("A1");
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let text = buttons::bounds("formula-bar-text").map(|b| b.center()).unwrap_or_else(|| point(px(200.), px((r.formula.0 + r.formula.1) / 2.0)));
    let t = |phase| PlatformInput::Touch(TouchEvent { id: TouchId(1), phase, position: text, predicted_position: None, force: None });
    s.dispatch(vec![t(TouchPhase::Started)]);
    s.advance_ms(60);
    s.dispatch(vec![t(TouchPhase::Ended)]);
    s.settle();
    s.key("cmd-a");
    s.type_text("k0089");
    s.key("enter");
    s.settle();
    assert_eq!(s.shown("A1"), "k0089", "the text typed in the bar at the bottom is the cell's");
}

/// FB3 (unchanged; passes now): at 720 and wider with a finger, and under 720 with a mouse only (K0077), the formula bar is under the toolbar and above
/// the grid, as it was.
fn fb3_unchanged() {
    for (width, touch) in [(720.0, true), (1200.0, true), (719.0, false), (400.0, false)] {
        let mut s = book(width, 874.0, touch);
        let r = rows(&mut s);
        assert!(r.menu_top <= r.toolbar.0 + 1.0, "{width} touch {touch}: the menu bar then the toolbar ({r:?})");
        assert!(r.toolbar.1 <= r.formula.0 + 1.0, "{width} touch {touch}: the formula bar is under the toolbar ({r:?})");
        assert!(r.formula.1 <= r.first_cell_y, "{width} touch {touch}: and above the grid ({r:?})");
    }
}

/// FB4: the formula bar expanded (its ▼, `toggle_formula_bar_expanded`) grows upward from the window's bottom: its bottom stays at the window's bottom, it
/// is taller, and the toolbar is above it with no overlap.
fn fb4_expanded() {
    let mut s = book(719.0, 874.0, true);
    let before = rows(&mut s);
    s.update(|e, _, cx| e.toggle_formula_bar_expanded(cx));
    s.settle();
    let after = rows(&mut s);
    assert!(after.formula.1 - after.formula.0 > before.formula.1 - before.formula.0 + 20.0, "it is taller ({before:?} -> {after:?})");
    assert!((after.formula.1 - before.formula.1).abs() < 1.5, "its bottom is where it was ({before:?} -> {after:?})");
    assert!(after.toolbar.1 <= after.formula.0 + 1.0, "the toolbar is above it, no overlap ({after:?})");
}

/// FB5 (the design's proposal; question 1): with the soft keyboard up (a phone's, 300 pt) the menu bar and the toolbar give way to the keyboard's toolbar as
/// K0064 has it, and the formula bar stays, directly above the keyboard's toolbar (its bottom at the toolbar's top, within 1 pt), below the grid, and
/// is not under the keyboard.
fn fb5_keyboard() {
    let mut s = book(719.0, 874.0, true);
    s.update(|e, _, cx| {
        e.soft_keyboard = SoftKeyboard { height: 300., kind: KeyboardKind::Docked, duration: 0.25 };
        cx.notify();
    });
    s.settle();
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let kbd = buttons::bounds("kbd-undo").or_else(|| buttons::bounds("kbd-hide")).unwrap_or_else(|| panic!("the keyboard's toolbar: {:?}", buttons::all()));
    let formula = s.with(|e, _| e.sheet_state().and_then(|st| st.bar_bounds.get())).expect("the formula bar is drawn above the keyboard's toolbar");
    assert!((f32::from(formula.bottom()) - f32::from(kbd.top())).abs() < 8.0 || f32::from(formula.bottom()) <= f32::from(kbd.top()) + 1.0, "the formula bar is just above the keyboard's toolbar ({formula:?} {kbd:?})");
    assert!(f32::from(formula.bottom()) <= 874.0 - 300.0 + 1.0, "and above the keyboard ({formula:?})");
    let cell = s.with(|e, _| e.cell_center(0, 0)).expect("the grid is on screen");
    assert!(f32::from(cell.y) < f32::from(formula.top()), "the grid is above the formula bar");
}

/// FB6: the layout follows the window and the finger: a touch window 1200 → 719 → 1200 has the formula bar above the grid, at the bottom, above the grid
/// again; a mouse-only 719 keeps it above the grid, and the first touch moves it down.
fn fb6_follows() {
    {
        let mut s = book(1200.0, 874.0, true);
        let r = rows(&mut s);
        assert!(r.formula.1 <= r.first_cell_y, "1200: above the grid ({r:?})");
        s.size(719.0, 874.0);
        s.settle();
        let r = rows(&mut s);
        assert!(r.formula.0 > r.toolbar.0, "719: below the toolbar ({r:?})");
        s.size(1200.0, 874.0);
        s.settle();
        let r = rows(&mut s);
        assert!(r.formula.1 <= r.first_cell_y, "back at 1200: above the grid ({r:?})");
    }
    let mut s = book(719.0, 874.0, false);
    let r = rows(&mut s);
    assert!(r.formula.1 <= r.first_cell_y, "719 with a mouse: above the grid ({r:?})");
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    let r = rows(&mut s);
    assert!(r.formula.0 > r.toolbar.0, "the first touch: below the toolbar ({r:?})");
}
