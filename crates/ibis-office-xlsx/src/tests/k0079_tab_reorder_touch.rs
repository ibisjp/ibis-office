// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0079 (iOS and Android's sheet tabs; design: `tasks/doc/test-design/K0079.md`; the numbers TR1... are that document's):
//! a finger on a sheet tab is a scroll of the strip when it moves at once, a held tab that lifts (soft shadow) at 500 ms, a tab to carry along
//! the strip when it moves between 500 and 1000 ms, and the tab's menu when it has not moved by 1000 ms. This is the sheet tabs' exception to
//! K0073's rule (the right click's menu at 500 ms within 8 px). The mouse is as it was: a right click opens the menu, a drag reorders.
//! The fingers are raw `TouchEvent`s with the mock clock moving between them (as K0065's and K0073's scenarios do); the platforms' own touch
//! paths are for the simulators (the design's section 5).
use super::SheetSteps;
use crate::ext::*;
use crate::shell::scenario::Scenario;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput, TouchEvent, TouchId, TouchPhase, point, px};
use ibis_doc_model::{Command, SheetCommand, SlideId};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::k0079_g1_a_right_click_on_a_tab_opens_its_menu_at_once", g1_right_click),
        ("xlsx::k0079_g2_a_mouse_held_on_a_tab_neither_lifts_it_nor_opens_a_menu", g2_mouse_hold),
        ("xlsx::k0079_g3_the_mouse_drag_still_reorders_the_tabs", g3_mouse_drag),
        ("xlsx::k0079_tr1_a_tab_held_500_ms_is_lifted_and_no_menu_yet", tr1_lift_at_500),
        ("xlsx::k0079_tr2_a_tab_held_still_1000_ms_opens_its_menu", tr2_menu_at_1000),
        ("xlsx::k0079_tr3_a_tab_moved_between_500_and_1000_ms_is_carried_and_dropped_where_it_is_let_go", tr3_carry),
        ("xlsx::k0079_tr4_a_finger_that_moves_before_500_ms_scrolls_the_strip", tr4_scroll_first),
        ("xlsx::k0079_tr5_a_slip_within_the_slop_keeps_the_hold_going", tr5_slip),
        ("xlsx::k0079_tr6_a_tab_let_go_where_it_was_picked_up_changes_nothing", tr6_let_go_in_place),
        ("xlsx::k0079_tr11_after_the_lift_a_move_within_8_px_is_no_move_and_the_menu_comes_at_1000_ms", tr11_small_move_after_lift),
        ("xlsx::k0079_tr7_a_second_finger_takes_the_hold_back", tr7_second_finger),
        ("xlsx::k0079_tr8_a_carried_tab_scrolls_the_strip_at_its_ends", tr8_edge),
        ("xlsx::k0079_tr9_the_carry_is_one_step_to_undo_and_is_refused_in_a_protected_workbook", tr9_undo_and_protection),
        ("xlsx::k0079_tr10_a_cell_still_opens_its_menu_at_500_ms", tr10_cell_unchanged),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}

const STEP_MS: u64 = 16;

fn at(x: f32, y: f32) -> gpui_kit::Point<gpui_kit::Pixels> {
    point(px(x), px(y))
}

fn touch(id: u64, phase: TouchPhase, x: f32, y: f32) -> PlatformInput {
    PlatformInput::Touch(TouchEvent { id: TouchId(id), phase, position: at(x, y), predicted_position: None, force: None })
}

fn order(s: &mut Scenario) -> Vec<SlideId> {
    s.with(|e, _| e.presentation().slides.iter().map(|s| s.id).collect())
}

/// A workbook of 7 sheets with names that fill a part of the strip, `width` wide, the touch on.
fn book(width: f32) -> (Scenario, Vec<SlideId>) {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(width, 720.);
    s.update(|e, w, cx| {
        e.touch_seen = true;
        while e.presentation().slides.len() < 7 {
            e.insert_sheet(w, cx);
        }
        let commands = e.presentation().slides.iter().enumerate().map(|(i, sl)| Command::Sheet(SheetCommand::RenameSheet { sheet: sl.id, name: format!("Tab_{i}_data") })).collect();
        e.run_sheet(commands, cx);
        let first = e.presentation().slides[0].id;
        e.select_slide(first, w, cx);
    });
    s.settle();
    let ids = order(&mut s);
    (s, ids)
}

/// The tab's box where it is drawn now: (left, top, width, height).
fn tab_box(s: &mut Scenario, index: usize) -> (f32, f32, f32, f32) {
    s.with(|e, _| {
        let b = e.wb().tab_scroll.bounds_for_item(index).expect("tab laid out");
        (f32::from(b.left()), f32::from(b.top()), f32::from(b.size.width), f32::from(b.size.height))
    })
}

fn run(s: &mut Scenario, ms: u64) {
    for _ in 0..(ms / STEP_MS) {
        s.advance_ms(STEP_MS);
    }
    s.settle();
}

fn menu_open(s: &mut Scenario) -> bool {
    s.with(|e, _| e.sheet_popup_open())
}

fn lifted(s: &mut Scenario) -> Option<SlideId> {
    s.with(|e, _| e.sheet_tab_lifted())
}

fn strip_offset(s: &mut Scenario) -> f32 {
    s.with(|e, _| f32::from(e.wb().tab_scroll.offset().x))
}

/// The finger goes down on the middle of tab `index`; returns the place.
fn down_on(s: &mut Scenario, index: usize) -> (f32, f32) {
    let (x, y, w, h) = tab_box(s, index);
    let p = (x + w / 2.0, y + h / 2.0);
    s.dispatch(vec![touch(1, TouchPhase::Started, p.0, p.1)]);
    p
}

/// TR1 (A: 500 ms): a finger held still on a tab shows nothing at 450 ms; at 550 ms the tab is lifted (soft shadow) and no menu is open; the order is
/// unchanged and the strip has not scrolled.
fn tr1_lift_at_500() {
    let (mut s, ids) = book(1100.);
    let p = down_on(&mut s, 2);
    run(&mut s, 450);
    assert_eq!(lifted(&mut s), None, "not lifted at 450 ms");
    run(&mut s, 100);
    assert_eq!(lifted(&mut s), Some(ids[2]), "lifted at 550 ms: the tab the finger is on");
    assert!(!menu_open(&mut s), "and the menu is not open yet (K0073's 500 ms does not apply to the tabs)");
    assert_eq!(order(&mut s), ids);
    s.dispatch(vec![touch(1, TouchPhase::Ended, p.0, p.1)]);
    run(&mut s, 200);
}

/// TR2 (A: 1000 ms): not moved by 950 ms: no menu; by 1050 ms: the tab's menu is open (as a right click's, the tab is made the current sheet), and it
/// stays open after the finger lifts; the order is unchanged.
fn tr2_menu_at_1000() {
    let (mut s, ids) = book(1100.);
    let p = down_on(&mut s, 2);
    run(&mut s, 950);
    assert!(!menu_open(&mut s), "no menu at 950 ms");
    run(&mut s, 100);
    assert!(menu_open(&mut s), "the tab's menu at 1050 ms");
    assert_eq!(s.with(|e, _| e.current_slide), ids[2], "the sheet of the tab is the current one, as a right click makes it");
    s.dispatch(vec![touch(1, TouchPhase::Ended, p.0, p.1)]);
    run(&mut s, 200);
    assert!(menu_open(&mut s), "it stays open after the lift");
    assert_eq!(order(&mut s), ids, "nothing was moved");
}

/// TR3 (A: carry): held 600 ms (lifted), moved to the right half of tab 4 in 300 ms, lifted: the menu never opened, the tab followed the finger on its
/// row (it does not go up or down), the others made room, and where it was let go it is dropped (the same order as the mouse's drag makes).
fn tr3_carry() {
    let (mut s, ids) = book(1100.);
    let (x1, y, w1, h) = tab_box(&mut s, 1);
    let (x4, _, w4, _) = tab_box(&mut s, 4);
    let start = (x1 + w1 / 2.0, y + h / 2.0);
    s.dispatch(vec![touch(1, TouchPhase::Started, start.0, start.1)]);
    run(&mut s, 600);
    assert_eq!(lifted(&mut s), Some(ids[1]), "setup: lifted");
    let goal = x4 + w4 * 0.75;
    for i in 1..=18 {
        let t = i as f32 / 18.0;
        // The finger also drifts down 30 px: the tab keeps to its row.
        s.dispatch(vec![touch(1, TouchPhase::Moved, start.0 + (goal - start.0) * t, start.1 + 30.0 * t)]);
        s.advance_ms(STEP_MS);
    }
    assert!(!menu_open(&mut s), "no menu while it is carried");
    let shown = s.with(|e, _| e.wb().tab_drag.as_ref().map(|d| d.order.clone())).expect("a carry");
    assert_eq!(shown[..5], [ids[0], ids[2], ids[3], ids[4], ids[1]], "the tabs made room: tab 1 would come after tab 4");
    assert_eq!(order(&mut s), ids, "nothing is changed before the finger lifts");
    s.dispatch(vec![touch(1, TouchPhase::Ended, goal, start.1 + 30.0)]);
    run(&mut s, 1500);
    let mut expected = ids.clone();
    let tab = expected.remove(1);
    expected.insert(4, tab);
    assert_eq!(order(&mut s), expected, "dropped where it was let go");
    assert!(!menu_open(&mut s), "and no menu opened at 1000 ms or after it");
    assert_eq!(lifted(&mut s), None, "the tab is put down");
    assert_eq!(s.with(|e, _| e.current_slide), ids[1], "the sheet that was carried is the current one, as the mouse's drag makes it");
}

/// TR4: a finger that moves before 500 ms scrolls the strip (a narrow window, the strip overflows) as it did, and neither lifts a tab nor opens a menu
/// then or later, even if it stops and stays down to 1500 ms.
fn tr4_scroll_first() {
    let (mut s, ids) = book(520.);
    let max = s.with(|e, _| f32::from(e.wb().tab_scroll.max_offset().x));
    assert!(max > 50.0, "setup: the strip overflows ({max})");
    s.update(|e, _, cx| {
        e.wb().tab_scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    });
    s.settle();
    let (x, y, w, h) = tab_box(&mut s, 2);
    let start = (x + w / 2.0, y + h / 2.0);
    s.dispatch(vec![touch(1, TouchPhase::Started, start.0, start.1)]);
    run(&mut s, 150);
    for i in 1..=10 {
        s.dispatch(vec![touch(1, TouchPhase::Moved, start.0 - 6.0 * i as f32, start.1)]);
        s.advance_ms(STEP_MS);
    }
    assert!(strip_offset(&mut s) < -40.0, "the strip scrolls with the finger ({})", strip_offset(&mut s));
    run(&mut s, 1500);
    assert_eq!(lifted(&mut s), None, "no tab is lifted by a finger that moved before 500 ms");
    assert!(!menu_open(&mut s), "and no menu opens at 1000 ms");
    s.dispatch(vec![touch(1, TouchPhase::Ended, start.0 - 60.0, start.1)]);
    run(&mut s, 300);
    assert_eq!(order(&mut s), ids, "the order is unchanged");
}

/// TR5: a slip of 4 px (within K0073's 8 px) during the hold does not cancel it: lifted at 550 ms, the menu at 1050 ms. A move of 14 px before 500 ms is
/// a scroll (TR4), and one of 14 px between 500 and 1000 ms is a carry (TR3), not a cancel.
fn tr5_slip() {
    let (mut s, ids) = book(1100.);
    let p = down_on(&mut s, 3);
    run(&mut s, 200);
    s.dispatch(vec![touch(1, TouchPhase::Moved, p.0 + 4.0, p.1)]);
    run(&mut s, 350);
    assert_eq!(lifted(&mut s), Some(ids[3]), "lifted at 550 ms after a 4 px slip");
    run(&mut s, 500);
    assert!(menu_open(&mut s), "the menu at 1050 ms");
    s.dispatch(vec![touch(1, TouchPhase::Ended, p.0 + 4.0, p.1)]);
    run(&mut s, 200);
}

/// TR6: held 700 ms (lifted) and let go without moving: the tab is put down, no menu opens, the order is unchanged, and the sheet is selected as a tap
/// selects it (the commander's decision).
fn tr6_let_go_in_place() {
    let (mut s, ids) = book(1100.);
    let p = down_on(&mut s, 3);
    run(&mut s, 700);
    assert_eq!(lifted(&mut s), Some(ids[3]), "setup");
    s.dispatch(vec![touch(1, TouchPhase::Ended, p.0, p.1)]);
    run(&mut s, 1500);
    assert_eq!(lifted(&mut s), None, "put down");
    assert!(!menu_open(&mut s), "no menu (it came before 1000 ms)");
    assert_eq!(order(&mut s), ids, "the order is unchanged");
    assert_eq!(s.with(|e, _| e.current_slide), ids[3], "the sheet of the tab is selected, as a tap selects it");
}

/// TR11 (the commander's decision): after the lift (at 600 ms) a move of 6 px or less is no move: the tab stays lifted, and at 1050 ms the menu opens
/// (a move over 8 px is a carry: TR3). The order is unchanged.
fn tr11_small_move_after_lift() {
    let (mut s, ids) = book(1100.);
    let p = down_on(&mut s, 3);
    run(&mut s, 600);
    assert_eq!(lifted(&mut s), Some(ids[3]), "setup: lifted");
    for i in 1..=6 {
        s.dispatch(vec![touch(1, TouchPhase::Moved, p.0 + i as f32, p.1)]);
        s.advance_ms(STEP_MS);
    }
    run(&mut s, 300);
    assert!(!menu_open(&mut s), "not yet at 900 ms");
    assert_eq!(order(&mut s), ids, "the tab was not carried");
    run(&mut s, 200);
    assert!(menu_open(&mut s), "the menu at 1100 ms: a move within 8 px is no move");
    s.dispatch(vec![touch(1, TouchPhase::Ended, p.0 + 6.0, p.1)]);
    run(&mut s, 200);
    assert_eq!(order(&mut s), ids, "nothing was moved");
}

/// TR7: a second finger down during the hold takes it back: nothing is lifted or carried at 700 ms, and no menu opens at 1050 ms (K0065/K0073: two
/// fingers are a scroll or a pinch); the order is unchanged.
fn tr7_second_finger() {
    let (mut s, ids) = book(1100.);
    let p = down_on(&mut s, 3);
    run(&mut s, 300);
    s.dispatch(vec![touch(2, TouchPhase::Started, p.0 + 120.0, p.1)]);
    run(&mut s, 400);
    assert_eq!(lifted(&mut s), None, "not lifted");
    run(&mut s, 500);
    assert!(!menu_open(&mut s), "no menu");
    s.dispatch(vec![touch(1, TouchPhase::Ended, p.0, p.1), touch(2, TouchPhase::Ended, p.0 + 120.0, p.1)]);
    run(&mut s, 200);
    assert_eq!(order(&mut s), ids);
}

/// TR8: a tab carried to within 16 px of an end of the strip scrolls the strip, so a tab beyond can be reached (as the mouse's drag does; a narrow
/// window).
fn tr8_edge() {
    let (mut s, _) = book(520.);
    s.update(|e, _, cx| {
        e.wb().tab_scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    });
    s.settle();
    let (x0, y, w0, h) = tab_box(&mut s, 0);
    let (view_right, max) = s.with(|e, _| (f32::from(e.wb().tab_scroll.bounds().right()), f32::from(e.wb().tab_scroll.max_offset().x)));
    assert!(max > 50.0, "setup: the strip overflows ({max})");
    let start = (x0 + w0 / 2.0, y + h / 2.0);
    s.dispatch(vec![touch(1, TouchPhase::Started, start.0, start.1)]);
    run(&mut s, 600);
    for i in 1..=10 {
        let t = i as f32 / 10.0;
        s.dispatch(vec![touch(1, TouchPhase::Moved, start.0 + (view_right - 4.0 - start.0) * t, start.1)]);
        s.advance_ms(STEP_MS);
    }
    for _ in 0..30 {
        s.dispatch(vec![touch(1, TouchPhase::Moved, view_right - 4.0, start.1)]);
        s.advance_ms(STEP_MS);
    }
    assert!(strip_offset(&mut s) < -30.0, "the strip scrolled toward the end ({})", strip_offset(&mut s));
    s.dispatch(vec![touch(1, TouchPhase::Ended, view_right - 4.0, start.1)]);
    run(&mut s, 800);
}

/// TR9: the carry is one step to undo (⌘Z puts the order back, ⇧⌘Z does it again), and in a workbook whose structure is protected the tab can be
/// lifted but not carried (the same refusal the mouse's drag has).
fn tr9_undo_and_protection() {
    let (mut s, ids) = book(1100.);
    let (x1, y, w1, h) = tab_box(&mut s, 1);
    let (x4, _, w4, _) = tab_box(&mut s, 4);
    let start = (x1 + w1 / 2.0, y + h / 2.0);
    s.dispatch(vec![touch(1, TouchPhase::Started, start.0, start.1)]);
    run(&mut s, 600);
    for i in 1..=10 {
        s.dispatch(vec![touch(1, TouchPhase::Moved, start.0 + (x4 + w4 * 0.75 - start.0) * i as f32 / 10.0, start.1)]);
        s.advance_ms(STEP_MS);
    }
    s.dispatch(vec![touch(1, TouchPhase::Ended, x4 + w4 * 0.75, start.1)]);
    run(&mut s, 500);
    let carried = order(&mut s);
    assert_ne!(carried, ids, "setup: it was carried");
    s.key("cmd-z");
    assert_eq!(order(&mut s), ids, "one step to undo");
    s.key("cmd-shift-z");
    assert_eq!(order(&mut s), carried, "and to redo");
    s.key("cmd-z");
    // Protected structure: the mouse's drag is refused (`workbook_protection_refuses`); the finger's carry is as well, and the order stays.
    s.update(|e, _, cx| {
        e.run_sheet(vec![Command::Sheet(SheetCommand::SetWorkbookProtection {
            protection: Some(ibis_doc_model::sheet::WorkbookProtection { lock_structure: true, lock_windows: false, lock_revision: false, hash: None, legacy: None }),
        })], cx);
    });
    s.settle();
    let (x1, y, w1, h) = tab_box(&mut s, 1);
    let start = (x1 + w1 / 2.0, y + h / 2.0);
    s.dispatch(vec![touch(1, TouchPhase::Started, start.0, start.1)]);
    run(&mut s, 600);
    for i in 1..=10 {
        s.dispatch(vec![touch(1, TouchPhase::Moved, start.0 + 200.0 * i as f32 / 10.0, start.1)]);
        s.advance_ms(STEP_MS);
    }
    s.dispatch(vec![touch(1, TouchPhase::Ended, start.0 + 200.0, start.1)]);
    run(&mut s, 500);
    assert_eq!(order(&mut s), ids, "a protected workbook's tabs do not move");
}

/// TR10 (K0073 unchanged outside the tabs; passes where K0073 is merged): a finger held still on a cell opens its menu at 500 ms (not 1000 ms): the
/// sheet tabs are the only exception.
fn tr10_cell_unchanged() {
    let (mut s, _) = book(1100.);
    let c = s.with(|e, _| e.cell_center(2, 2)).expect("a cell is on screen");
    let (x, y) = (f32::from(c.x), f32::from(c.y));
    s.dispatch(vec![touch(1, TouchPhase::Started, x, y)]);
    run(&mut s, 450);
    assert!(!menu_open(&mut s), "not yet at 450 ms");
    run(&mut s, 150);
    assert!(menu_open(&mut s), "at 600 ms the cell's menu is open");
    s.dispatch(vec![touch(1, TouchPhase::Ended, x, y)]);
}

// ---- the mouse, as it was (these pass now) ----

fn mouse(button: MouseButton, x: f32, y: f32, down: bool) -> PlatformInput {
    if down {
        PlatformInput::MouseDown(MouseDownEvent { button, position: at(x, y), click_count: 1, ..Default::default() })
    } else {
        PlatformInput::MouseUp(MouseUpEvent { button, position: at(x, y), click_count: 1, ..Default::default() })
    }
}

/// G1: a right click on a tab opens its menu at once (no wait of 1000 ms).
fn g1_right_click() {
    let (mut s, ids) = book(1100.);
    let (x, y, w, h) = tab_box(&mut s, 2);
    s.right_click((x + w / 2.0, y + h / 2.0));
    assert!(menu_open(&mut s), "the menu opens at the right click");
    assert_eq!(s.with(|e, _| e.current_slide), ids[2]);
}

/// G2: the mouse's left button held on a tab for 2 s lifts nothing and opens nothing (the finger's hold is for fingers only).
fn g2_mouse_hold() {
    let (mut s, ids) = book(1100.);
    let (x, y, w, h) = tab_box(&mut s, 2);
    let p = (x + w / 2.0, y + h / 2.0);
    s.dispatch(vec![mouse(MouseButton::Left, p.0, p.1, true)]);
    run(&mut s, 2000);
    assert_eq!(lifted(&mut s), None, "the tab is not lifted by a held mouse button");
    assert!(!menu_open(&mut s), "and no menu opens");
    s.dispatch(vec![mouse(MouseButton::Left, p.0, p.1, false)]);
    assert_eq!(order(&mut s), ids);
}

/// G3: the mouse's drag reorders at once (no wait of 500 ms): pressed, moved 30 px to the right half of tab 4 and released, the tab is dropped there.
fn g3_mouse_drag() {
    let (mut s, ids) = book(1100.);
    let (x1, y, w1, h) = tab_box(&mut s, 1);
    let (x4, _, w4, _) = tab_box(&mut s, 4);
    let from = (x1 + w1 / 2.0, y + h / 2.0);
    s.dispatch(vec![
        mouse(MouseButton::Left, from.0, from.1, true),
        PlatformInput::MouseMove(MouseMoveEvent { position: at(from.0 + 30.0, from.1), pressed_button: Some(MouseButton::Left), ..Default::default() }),
        PlatformInput::MouseMove(MouseMoveEvent { position: at(x4 + w4 * 0.75, from.1), pressed_button: Some(MouseButton::Left), ..Default::default() }),
        mouse(MouseButton::Left, x4 + w4 * 0.75, from.1, false),
    ]);
    let mut expected = ids.clone();
    let tab = expected.remove(1);
    expected.insert(4, tab);
    assert_eq!(order(&mut s), expected, "the mouse reorders at once");
}
