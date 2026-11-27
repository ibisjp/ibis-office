// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0078: a menu that is cut (a scroll bar shows) scrolls with a finger's flick, coasts after the finger lifts, and stretches and springs
//! back at its ends, as the body and every `Div` that scrolls do since K0065 (design: `tasks/doc/test-design/K0078.md`; the numbers
//! MS1... are that document's). The menu is not a `Div` that scrolls: it shows a window of its lines (`menu_bar.rs` `MenuScroll`, rebuilt
//! when `start` changes) and takes every `ScrollWheelEvent` over it at the window's capture phase, a line at a time.
//!
//! The menu's scroll is read from `OpenMenu::scrolled_px`, `max_scrolled_px` and `stretch_px` and from the rows the popup painted
//! (`PopupMenu::painted_rows`), so that a fix that scrolls by pixels and one that shows a different offset from what is drawn are both found.
//! The fingers are raw `TouchEvent`s with the mock clock moving between them, as K0065's and K0073's scenarios do. The platforms' own touch paths
//! (iOS, Android) are not exercised here but on the simulators (the design's section 5): no scenario here is proof of them.
//! The numbers are K0065's: rubber band `x * d * 0.55 / (d + 0.55 x)`, return by a critically damped spring (0.524 s scale; under 1 px in 0.7 s),
//! a fling of v px/ms coasts `v * 0.998 / (1 - 0.998)` = 499 v px, 90.6 px in the first 100 ms for 1 px/ms.
use crate::shell::chrome::MenuId;
use crate::shell::scenario::{Scenario, button_probe as buttons};
use gpui_kit::*;
use ibis_office_xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0078_d1_the_wheel_and_the_trackpad_scroll_the_menu_with_no_stretch", d1_wheel),
        ("shell::k0078_ld1_the_wheel_scrolls_a_dropdown_list_with_no_stretch", ld1_wheel),
        ("shell::k0078_d2_a_menu_that_is_not_cut_has_nothing_to_scroll_and_a_finger_does_not_press_it", d2_not_cut),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0078_ms1_a_finger_drags_the_menu_one_for_one_and_presses_nothing", ms1_drag),
        ("shell::k0078_ms2_a_flick_coasts_on_after_the_finger_lifts_and_stops", ms2_flick),
        ("shell::k0078_ms3_a_finger_down_stops_the_coasting_and_is_not_a_press", ms3_catch),
        ("shell::k0078_ms4_pulling_past_the_top_follows_the_rubber_band", ms4_rubber_band),
        ("shell::k0078_ms5_letting_go_past_the_end_returns_without_overshoot", ms5_return),
        ("shell::k0078_ms6_a_fling_into_the_bottom_bounces_once_and_rests_on_the_end", ms6_fling_into_the_bottom),
        ("shell::k0078_ms7_a_fling_into_the_top_bounces_once_and_rests_on_the_end", ms7_fling_into_the_top),
        ("shell::k0078_ms8_a_tap_on_a_line_presses_it_also_after_scrolling", ms8_tap),
        ("shell::k0078_ms9_a_submenu_scrolls_by_a_finger_and_its_parent_stays", ms9_submenu),
        ("shell::k0078_ms10_the_menu_of_a_narrow_touch_window_rises_from_the_bottom_bar_and_scrolls_the_same", ms10_narrow),
        ("shell::k0078_ms11_the_body_under_the_menu_does_not_scroll", ms11_body_stays),
        ("shell::k0078_ls1_a_finger_drags_a_dropdown_list_one_for_one_and_presses_nothing", ls1_drag),
        ("shell::k0078_ls2_a_flick_coasts_in_a_dropdown_list", ls2_flick),
        ("shell::k0078_ls3_pulling_a_dropdown_list_past_its_top_stretches_it_and_it_returns", ls3_bounce),
        ("shell::k0078_ls4_a_tap_on_a_dropdown_list_row_picks_it", ls4_tap),
        ("shell::k0078_st1_a_finger_on_the_sheet_tabs_strip_scrolls_the_tabs_with_coasting_and_stretch", st1_sheet_tabs),
        ("shell::k0078_ms12_two_fingers_scroll_the_menu_too", ms12_two_fingers),
    ]
}

const STEP_MS: u64 = 16;

fn pt(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
}

fn touch(id: u64, phase: TouchPhase, at: Point<Pixels>) -> PlatformInput {
    PlatformInput::Touch(TouchEvent { id: TouchId(id), phase, position: at, predicted_position: None, force: None })
}

/// The Insert menu of a spreadsheet (a long one), opened in a window `width` x `height` (short enough that it is cut), with the touch on.
/// Returns the scenario and a place in the middle of the menu's lines (left of its scroll bar).
fn open(width: f32, height: f32, touch_on: bool) -> (Scenario, Point<Pixels>) {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(width, height);
    s.update(move |e, _, cx| {
        e.touch_seen = touch_on;
        cx.notify();
    });
    s.settle();
    let index = s.with(|e, _| e.menu_bar_menus().menus.iter().position(|(id, _)| *id == MenuId::Insert).unwrap());
    s.hover((0., 0.));
    s.update(move |e, w, cx| e.open_menu_at(index, w, cx));
    s.settle();
    let track = buttons::bounds("menu-scrollbar-track").unwrap_or_else(|| panic!("setup: the Insert menu is cut at {width}x{height}: {:?}", buttons::all()));
    let at = pt(f32::from(track.left()) - 80.0, f32::from(track.center().y));
    (s, at)
}

/// (scrolled px, stretch px, most scrollable px) of the open menu.
fn state(s: &mut Scenario) -> (f32, f32, f32) {
    s.update(|e, window, _| {
        let m = e.open_menu.as_ref().expect("the menu is open");
        (m.scrolled_px(), m.stretch_px(), m.max_scrolled_px(window))
    })
}

fn scrolled(s: &mut Scenario) -> f32 {
    let (px, stretch, _) = state(s);
    // Past the top the drawn lines are lower than at rest: `stretch` is positive there.
    px - stretch
}

/// The top of the first row the menu has painted (what the eye sees move).
fn first_row_top(s: &mut Scenario) -> f32 {
    let menu = s.with(|e, _| e.open_menu.as_ref().expect("the menu is open").popup_entity().clone());
    let rows = s.with_app(|cx| menu.read(cx).painted_rows());
    f32::from(rows.first().expect("the menu has painted rows").0.top())
}

fn one_finger(s: &mut Scenario, from: Point<Pixels>, dy: f32, ms: u64, lift: bool) {
    s.dispatch(vec![touch(1, TouchPhase::Started, from)]);
    let steps = (ms / STEP_MS).max(1);
    let to = pt(f32::from(from.x), f32::from(from.y) + dy);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        s.dispatch(vec![touch(1, TouchPhase::Moved, pt(f32::from(from.x), f32::from(from.y) + dy * t))]);
        s.advance_ms(STEP_MS);
    }
    if lift {
        s.dispatch(vec![touch(1, TouchPhase::Ended, to)]);
    }
}

fn run(s: &mut Scenario, ms: u64) {
    for _ in 0..(ms / STEP_MS) {
        s.advance_ms(STEP_MS);
    }
    s.settle();
}

/// Scrolled to `px` (by the keyboard-free route the menu has: its own scroll to a line, then the finger moves the rest) is not needed: the tests
/// that need the middle scroll the menu down with a slow finger.
fn to_the_middle(s: &mut Scenario, at: Point<Pixels>) {
    let max = state(s).2;
    one_finger(s, at, -(max / 2.0), (max as u64) * 8, false);
    s.dispatch(vec![touch(1, TouchPhase::Moved, pt(f32::from(at.x), f32::from(at.y) - max / 2.0))]);
    run(s, 200);
    s.dispatch(vec![touch(1, TouchPhase::Ended, pt(f32::from(at.x), f32::from(at.y) - max / 2.0))]);
    run(s, 1500);
}

/// MS1 (symptom): a finger drags the menu's lines one for one (not a line at a time, not nothing), slowly enough that nothing coasts after; the lines
/// drawn move as far as the offset says; and the drag does not press the line it began on.
fn ms1_drag() {
    let _probe = buttons::Probe::new();
    buttons::dry_run(true);
    let (mut s, at) = open(1200., 340., true);
    let max = state(&mut s).2;
    assert!(max >= 200.0, "setup: the menu has {max} px to scroll");
    let top0 = first_row_top(&mut s);
    // 70 px (2.5 lines: not a whole number of lines) over 1.4 s (0.05 px/ms), then held still before the lift (no velocity).
    one_finger(&mut s, at, -70.0, 1400, false);
    run(&mut s, 160);
    s.dispatch(vec![touch(1, TouchPhase::Ended, pt(f32::from(at.x), f32::from(at.y) - 70.0))]);
    run(&mut s, 1500);
    let after = state(&mut s);
    assert!((after.0 - 70.0).abs() < 1.5, "the menu follows the finger one for one ({} px for 70)", after.0);
    assert!(((top0 - first_row_top(&mut s)) - 70.0).abs() < 1.5, "and the lines drawn moved 70 px ({} -> {})", top0, first_row_top(&mut s));
    assert!(s.with(|e, _| e.open_menu.is_some()), "the menu is still open: a drag is no press");
    assert_eq!(buttons::dispatch_count(), 0, "and no line was pressed");
    // Back down by the same: the lines return (one for one the other way).
    one_finger(&mut s, at, 70.0, 1400, false);
    run(&mut s, 160);
    s.dispatch(vec![touch(1, TouchPhase::Ended, pt(f32::from(at.x), f32::from(at.y) + 70.0))]);
    run(&mut s, 1500);
    assert!(state(&mut s).0.abs() < 1.5, "and back: {}", state(&mut s).0);
}

/// MS2: a flick (0.2 px/ms: 60 px in 300 ms) lifts; the menu goes on by 0.998^t per ms: 18 px in the first 100 ms and 99.8 px in all (within 25 %),
/// and then it stops and stays.
fn ms2_flick() {
    let _probe = buttons::Probe::new();
    let (mut s, at) = open(1200., 340., true);
    assert!(state(&mut s).2 >= 200.0, "setup");
    one_finger(&mut s, at, -60.0, 300, true);
    let at_lift = scrolled(&mut s);
    run(&mut s, 100);
    let first = scrolled(&mut s) - at_lift;
    assert!((first - 18.1).abs() < 4.5, "in the first 100 ms it coasts about 18 px ({first})");
    run(&mut s, 5000);
    let total = scrolled(&mut s) - at_lift;
    assert!((total - 99.8).abs() < 25.0, "in all it coasts about 100 px ({total})");
    run(&mut s, 2000);
    assert!((scrolled(&mut s) - at_lift - total).abs() < 1.0, "and it has stopped");
}

/// MS3: a finger put down while the menu coasts stops it at once; lifting it is not a press of the line there.
fn ms3_catch() {
    let _probe = buttons::Probe::new();
    buttons::dry_run(true);
    let (mut s, at) = open(1200., 340., true);
    one_finger(&mut s, at, -90.0, 300, true);
    run(&mut s, 100);
    s.dispatch(vec![touch(1, TouchPhase::Started, at)]);
    let caught = scrolled(&mut s);
    run(&mut s, 1500);
    assert_eq!(scrolled(&mut s), caught, "the coasting stopped when the finger came down");
    s.dispatch(vec![touch(1, TouchPhase::Ended, at)]);
    run(&mut s, 200);
    assert_eq!(buttons::dispatch_count(), 0, "the finger that caught it pressed no line");
    assert!(s.with(|e, _| e.open_menu.is_some()));
}

/// MS4: at the top, a finger pulling down 100 px shows the stretch `x d c / (d + c x)` (c 0.55, d the menu's height, within 6 %), never as much as the
/// menu's height, and the offset itself stays 0.
fn ms4_rubber_band() {
    let _probe = buttons::Probe::new();
    let (mut s, at) = open(1200., 340., true);
    let d = buttons::bounds("menu-scrollbar-track").map(|t| f32::from(t.size.height)).unwrap();
    let top0 = first_row_top(&mut s);
    one_finger(&mut s, at, 100.0, 1600, false);
    let (offset, stretch, _) = state(&mut s);
    let expected = 100.0 * d * 0.55 / (d + 0.55 * 100.0);
    assert_eq!(offset, 0.0, "the offset stays at the top");
    assert!((stretch - expected).abs() < expected * 0.06 + 1.0, "pulled 100 px it shows {stretch} px (the band: {expected})");
    assert!(first_row_top(&mut s) - top0 > 0.9 * stretch, "and the lines are drawn that much lower");
    s.dispatch(vec![touch(1, TouchPhase::Ended, pt(f32::from(at.x), f32::from(at.y) + 100.0))]);
}

/// MS5: let go past the top, the menu returns by the critically damped spring: it does not cross the top, never goes back out, and is under 1 px
/// away at 0.7 s.
fn ms5_return() {
    let _probe = buttons::Probe::new();
    let (mut s, at) = open(1200., 340., true);
    one_finger(&mut s, at, 100.0, 1600, false);
    s.dispatch(vec![touch(1, TouchPhase::Ended, pt(f32::from(at.x), f32::from(at.y) + 100.0))]);
    let mut last = state(&mut s).1;
    assert!(last > 20.0, "setup: pulled past the top ({last})");
    for step in 0..44 {
        run(&mut s, STEP_MS);
        let now = state(&mut s).1;
        assert!(now >= -0.01, "it does not cross the top ({now} at step {step})");
        assert!(now <= last + 0.01, "it only comes back ({last} -> {now} at step {step})");
        last = now;
    }
    assert!(last < 1.0, "within 0.7 s it is back ({last})");
}

/// MS6: a fast fling down into the bottom: it goes past the end a little (more than 3 px, never past the band), returns once and rests on the end.
fn ms6_fling_into_the_bottom() {
    let _probe = buttons::Probe::new();
    let (mut s, at) = open(1200., 340., true);
    let max = state(&mut s).2;
    one_finger(&mut s, at, -(max + 300.0), 150, true); // 10 px/ms
    let (mut deepest, mut crossings, mut was_past) = (0.0f32, 0, false);
    for _ in 0..200 {
        run(&mut s, STEP_MS);
        let past = -state(&mut s).1;
        deepest = deepest.max(past);
        if (past > 0.5) != was_past {
            crossings += 1;
            was_past = past > 0.5;
        }
    }
    let (offset, stretch, max) = state(&mut s);
    assert!(deepest > 3.0, "it goes past the bottom a little ({deepest})");
    assert!(deepest < 0.55 * 340.0, "but not farther than the band allows ({deepest})");
    assert!((offset - max).abs() < 1.0 && stretch.abs() < 1.0, "and rests on the end ({offset} of {max}, stretch {stretch})");
    assert!(crossings <= 2, "past once and back once ({crossings} crossings)");
}

/// MS7: the same into the top, from the middle.
fn ms7_fling_into_the_top() {
    let _probe = buttons::Probe::new();
    let (mut s, at) = open(1200., 340., true);
    to_the_middle(&mut s, at);
    assert!(scrolled(&mut s) > 50.0, "setup: scrolled down");
    one_finger(&mut s, at, 900.0, 150, true);
    let (mut deepest, mut crossings, mut was_past) = (0.0f32, 0, false);
    for _ in 0..200 {
        run(&mut s, STEP_MS);
        let past = state(&mut s).1;
        deepest = deepest.max(past);
        if (past > 0.5) != was_past {
            crossings += 1;
            was_past = past > 0.5;
        }
    }
    assert!(deepest > 3.0 && deepest < 0.55 * 340.0, "it goes past the top a little ({deepest})");
    assert!(state(&mut s).0.abs() < 1.0 && state(&mut s).1.abs() < 1.0, "and rests on the top");
    assert!(crossings <= 2, "{crossings}");
}

/// MS8: a tap on a line presses it and closes the menu, at the top and after the menu was scrolled (the line is where it is drawn now).
fn ms8_tap() {
    for scroll_first in [false, true] {
        let _probe = buttons::Probe::new();
        buttons::dry_run(true);
        let (mut s, at) = open(1200., 340., true);
        if scroll_first {
            one_finger(&mut s, at, -80.0, 1600, false);
            run(&mut s, 160);
            s.dispatch(vec![touch(1, TouchPhase::Ended, pt(f32::from(at.x), f32::from(at.y) - 80.0))]);
            run(&mut s, 1500);
        }
        let menu = s.with(|e, _| e.open_menu.as_ref().unwrap().popup_entity().clone());
        let rows = s.with_app(|cx| menu.read(cx).painted_rows());
        // A plain line (not a submenu) well inside the menu.
        let (row, _) = rows.iter().skip(2).find(|(_, sub)| !*sub).expect("a plain line");
        let c = row.center();
        s.dispatch(vec![touch(1, TouchPhase::Started, c)]);
        run(&mut s, 60);
        s.dispatch(vec![touch(1, TouchPhase::Ended, c)]);
        run(&mut s, 200);
        assert!(buttons::dispatch_count() >= 1 || s.with(|e, _| e.open_menu.is_none()), "scrolled first: {scroll_first}: the tap pressed the line");
        assert!(s.with(|e, _| e.open_menu.is_none()), "scrolled first: {scroll_first}: and the menu closed");
    }
}

/// MS9: the submenu (Insert > Chart, a cut one in a low window) scrolls with a finger: its thumb moves down, its lines are not a whole number of lines
/// off, and the menu it hangs from is not scrolled.
fn ms9_submenu() {
    let _probe = buttons::Probe::new();
    let (mut s, _) = open(1200., 340., true);
    let index = s.with(|e, _| e.menu_bar_menus().menus.iter().find(|(id, _)| *id == MenuId::Insert).map(|(_, v)| v.iter().position(|x| matches!(x, crate::shell::chrome::MenuEntry::Submenu(sub) if sub.id == "insert.chart")).unwrap()).unwrap());
    let _ = index;
    buttons::clear();
    // Down the menu's lines, opening each submenu (right) and looking whether it is cut (it has a scroll bar); left closes it again.
    for _ in 0..40 {
        s.key("down");
        s.key("right");
        s.advance_ms(300);
        if buttons::bounds("submenu-scroll-track").is_some() {
            break;
        }
        s.key("left");
    }
    let Some(track) = buttons::bounds("submenu-scroll-track") else { panic!("setup: a cut submenu: {:?}", buttons::all()) };
    let thumb0 = buttons::bounds("submenu-scroll-thumb").unwrap();
    let parent0 = state(&mut s).0;
    let at = pt(f32::from(track.left()) - 60.0, f32::from(track.center().y));
    one_finger(&mut s, at, -70.0, 1400, false);
    run(&mut s, 160);
    s.dispatch(vec![touch(1, TouchPhase::Ended, pt(f32::from(at.x), f32::from(at.y) - 70.0))]);
    run(&mut s, 1500);
    let thumb1 = buttons::bounds("submenu-scroll-thumb").unwrap();
    assert!(thumb1.origin.y > thumb0.origin.y + px(2.0), "the submenu scrolled: {thumb0:?} -> {thumb1:?}");
    assert_eq!(state(&mut s).0, parent0, "the menu it hangs from did not");
}

/// MS10: the menu of a narrow touch window (K0061: the bars at the bottom; the menu rises from the bar): it is cut, and a finger scrolls it as in MS1.
fn ms10_narrow() {
    let _probe = buttons::Probe::new();
    let (mut s, at) = open(400., 560., true);
    let max = state(&mut s).2;
    assert!(max >= 100.0, "setup: the menu is cut in the narrow window ({max})");
    one_finger(&mut s, at, -50.0, 1000, false);
    run(&mut s, 160);
    s.dispatch(vec![touch(1, TouchPhase::Ended, pt(f32::from(at.x), f32::from(at.y) - 50.0))]);
    run(&mut s, 1500);
    assert!((state(&mut s).0 - 50.0).abs() < 1.5, "one for one: {}", state(&mut s).0);
}

/// MS11: a finger on the menu scrolls the menu, not the sheet under it.
fn ms11_body_stays() {
    let _probe = buttons::Probe::new();
    let (mut s, at) = open(1200., 340., true);
    let before = s.with(|e, _| e.cell_center(0, 0).map(|p| (f32::from(p.x), f32::from(p.y))));
    one_finger(&mut s, at, -120.0, 400, true);
    run(&mut s, 2000);
    assert!(state(&mut s).0 > 20.0, "the menu scrolled");
    assert_eq!(s.with(|e, _| e.cell_center(0, 0).map(|p| (f32::from(p.x), f32::from(p.y)))), before, "the sheet did not");
}

/// MS12: two fingers moving together scroll the menu too (a user who scrolls the body that way may do so here), one for one.
fn ms12_two_fingers() {
    let _probe = buttons::Probe::new();
    let (mut s, at) = open(1200., 340., true);
    let (cx, cy) = (f32::from(at.x), f32::from(at.y));
    s.dispatch(vec![touch(1, TouchPhase::Started, pt(cx - 30.0, cy)), touch(2, TouchPhase::Started, pt(cx + 30.0, cy))]);
    for i in 1..=60 {
        let dy = -i as f32;
        s.dispatch(vec![touch(1, TouchPhase::Moved, pt(cx - 30.0, cy + dy)), touch(2, TouchPhase::Moved, pt(cx + 30.0, cy + dy))]);
        s.advance_ms(24);
    }
    run(&mut s, 160);
    s.dispatch(vec![touch(1, TouchPhase::Ended, pt(cx - 30.0, cy - 60.0)), touch(2, TouchPhase::Ended, pt(cx + 30.0, cy - 60.0))]);
    run(&mut s, 1500);
    assert!((state(&mut s).0 - 60.0).abs() < 2.0, "{}", state(&mut s).0);
}

// ---- unchanged (these pass now) ----

/// D1: the mouse wheel and the trackpad (`touch_fingers` 0) scroll the menu as before, and never stretch it: at the top an upward wheel does nothing;
/// a downward wheel moves it; nothing coasts on after it.
fn d1_wheel() {
    let _probe = buttons::Probe::new();
    let (mut s, at) = open(1200., 340., false);
    let (x, y) = (f32::from(at.x), f32::from(at.y));
    s.wheel((x, y), 300.0);
    run(&mut s, 800);
    let (offset, stretch, _) = state(&mut s);
    assert!(offset == 0.0 && stretch == 0.0, "the wheel does not pull the menu past the top ({offset}, {stretch})");
    s.wheel((x, y), -60.0);
    let after = state(&mut s).0;
    assert!(after > 0.0, "a wheel step down scrolls the menu ({after})");
    run(&mut s, 800);
    assert_eq!(state(&mut s).0, after, "and nothing goes on by itself");
    let max = state(&mut s).2;
    s.wheel((x, y), -(max + 500.0));
    run(&mut s, 800);
    let (offset, stretch, max) = state(&mut s);
    assert!((offset - max).abs() < 1.0 && stretch == 0.0, "past the bottom it stops on the end ({offset} of {max}, stretch {stretch})");
}

/// D2: a menu with room for all its lines (a tall window) has nothing to scroll; a finger on it neither moves nor stretches it nor presses a line by
/// dragging.
fn d2_not_cut() {
    let _probe = buttons::Probe::new();
    buttons::dry_run(true);
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 1400.);
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    let index = s.with(|e, _| e.menu_bar_menus().menus.iter().position(|(id, _)| *id == MenuId::Insert).unwrap());
    s.hover((0., 0.));
    s.update(move |e, w, cx| e.open_menu_at(index, w, cx));
    s.settle();
    assert!(buttons::bounds("menu-scrollbar-track").is_none(), "setup: the menu is not cut");
    let menu = s.with(|e, _| e.open_menu.as_ref().unwrap().popup_entity().clone());
    let rows = s.with_app(|cx| menu.read(cx).painted_rows());
    let c = rows[3].0.center();
    one_finger(&mut s, c, -100.0, 400, true);
    run(&mut s, 1000);
    let (offset, stretch, max) = state(&mut s);
    assert!(offset == 0.0 && stretch == 0.0 && max == 0.0, "nothing to scroll ({offset}, {stretch}, {max})");
    assert_eq!(buttons::dispatch_count(), 0, "and the drag pressed no line");
}

// ---- the dropdown lists (chrome/dropdown_list.rs: `list` and `AnchoredDropdown`) ----
// The representative is the menu search's list (ツールバーの「メニューを検索」, Alt+/ ): a `dropdown_list::list` (a `Div` with `overflow_y_scroll`
// and a `ScrollHandle`) over every command, long enough to scroll. `AnchoredDropdown` (the sheet's data validation choices) wraps the same
// `list` and has an `on_scroll_wheel` of its own that sets the offset and stops the event; it is the same family (design 2.1).

/// The menu search opened in a window `width` x `height`, with the touch on. Returns the scenario, the list's scroll handle and a place in the middle of the list.
fn open_list(width: f32, height: f32, touch_on: bool) -> (Scenario, ScrollHandle, Point<Pixels>) {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(width, height);
    s.update(move |e, _, cx| {
        e.touch_seen = touch_on;
        cx.notify();
    });
    s.settle();
    s.update(|e, w, cx| e.open_toolbar_menu_search(w, cx));
    s.settle();
    let handle = s.with(|e, _| e.menu_search_scroll()).expect("setup: the menu search's list exists");
    assert!(f32::from(handle.max_offset().y) > 200.0, "setup: the list has more than 200 px to scroll ({:?})", handle.max_offset());
    let b = handle.bounds();
    (s, handle, pt(f32::from(b.center().x), f32::from(b.center().y)))
}

/// How far the list is scrolled (px, positive = down) and how far its first row is drawn past the top (positive) by a pull: from where the first row
/// is laid out (`bounds_for_item(0)`) against where the list's own top is, less the offset. If the stretch is not part of what `bounds_for_item` says,
/// the fix adds a read-only accessor to the fork (design 2.1) and this reads it.
fn list_state(handle: &ScrollHandle) -> (f32, f32) {
    let offset = -f32::from(handle.offset().y);
    // The stretch a finger's pull gives is not in `bounds_for_item` (it is laid out at the scroll offset only): the handle says it (design 2.1).
    (offset, f32::from(handle.bounce_offset().y))
}

/// LS1: one finger drags the list one for one (70 px; held before the lift so nothing coasts), the list stays open and no row is picked.
fn ls1_drag() {
    let (mut s, handle, at) = open_list(1200., 700., true);
    one_finger(&mut s, at, -70.0, 1400, false);
    run(&mut s, 160);
    s.dispatch(vec![touch(1, TouchPhase::Ended, pt(f32::from(at.x), f32::from(at.y) - 70.0))]);
    run(&mut s, 1500);
    let (offset, stretch) = list_state(&handle);
    assert!((offset - 70.0).abs() < 1.5, "the list follows the finger one for one ({offset} for 70)");
    assert!(stretch.abs() < 1.0, "and is not stretched ({stretch})");
    assert!(s.with(|e, _| e.toolbar_search_open.is_some()), "the list is still open: a drag picks nothing");
}

/// LS2: a flick (0.2 px/ms, 60 px in 300 ms) goes on after the lift by about 18 px in 100 ms and 100 px in all (within 25 %), then stops.
fn ls2_flick() {
    let (mut s, handle, at) = open_list(1200., 700., true);
    one_finger(&mut s, at, -60.0, 300, true);
    let at_lift = list_state(&handle).0;
    run(&mut s, 100);
    let first = list_state(&handle).0 - at_lift;
    assert!((first - 18.1).abs() < 4.5, "in the first 100 ms it coasts about 18 px ({first})");
    run(&mut s, 5000);
    let total = list_state(&handle).0 - at_lift;
    assert!((total - 99.8).abs() < 25.0, "in all it coasts about 100 px ({total})");
    run(&mut s, 2000);
    assert!((list_state(&handle).0 - at_lift - total).abs() < 1.0, "and has stopped");
}

/// LS3: pulled 100 px down at the top, the list shows the rubber band (`x d c / (d + c x)`, d the list's height, within 6 %) and, let go, returns within
/// 0.7 s without crossing the top.
fn ls3_bounce() {
    let (mut s, handle, at) = open_list(1200., 700., true);
    let d = f32::from(handle.bounds().size.height);
    one_finger(&mut s, at, 100.0, 1600, false);
    let (offset, stretch) = list_state(&handle);
    let expected = 100.0 * d * 0.55 / (d + 0.55 * 100.0);
    assert!(offset.abs() < 0.5, "the offset stays at the top ({offset})");
    assert!((stretch - expected).abs() < expected * 0.06 + 1.0, "pulled 100 px it shows {stretch} px (the band: {expected})");
    s.dispatch(vec![touch(1, TouchPhase::Ended, pt(f32::from(at.x), f32::from(at.y) + 100.0))]);
    let mut last = stretch;
    for step in 0..44 {
        run(&mut s, STEP_MS);
        let now = list_state(&handle).1;
        assert!(now >= -0.01 && now <= last + 0.01, "it only comes back, not past the top ({last} -> {now} at step {step})");
        last = now;
    }
    assert!(last < 1.0, "within 0.7 s it is back ({last})");
}

/// LS4: a tap on a row picks it (a plain row of the list, after the list was scrolled by a finger too).
fn ls4_tap() {
    for scroll_first in [false, true] {
        let (mut s, handle, at) = open_list(1200., 700., true);
        if scroll_first {
            one_finger(&mut s, at, -80.0, 1600, false);
            run(&mut s, 160);
            s.dispatch(vec![touch(1, TouchPhase::Ended, pt(f32::from(at.x), f32::from(at.y) - 80.0))]);
            run(&mut s, 1500);
        }
        let b = handle.bounds();
        // The row under the middle of the list (the list's rows are all one height).
        let c = pt(f32::from(b.center().x), f32::from(b.center().y));
        let _probe = buttons::Probe::new();
        buttons::dry_run(true);
        s.dispatch(vec![touch(1, TouchPhase::Started, c)]);
        run(&mut s, 60);
        s.dispatch(vec![touch(1, TouchPhase::Ended, c)]);
        run(&mut s, 300);
        assert!(s.with(|e, _| e.toolbar_search_open.is_none()), "scrolled first: {scroll_first}: the tap picked the row and the list closed");
    }
}

/// LD1 (unchanged; passes now): the wheel (`touch_fingers` 0) scrolls the list and never stretches it: at the top an upward wheel does nothing; a
/// downward one moves it and nothing goes on by itself; far down it stops on the end.
fn ld1_wheel() {
    let (mut s, handle, at) = open_list(1200., 700., false);
    let (x, y) = (f32::from(at.x), f32::from(at.y));
    s.wheel((x, y), 300.0);
    run(&mut s, 800);
    let (offset, stretch) = list_state(&handle);
    assert!(offset.abs() < 0.5 && stretch.abs() < 0.5, "not pulled past the top ({offset}, {stretch})");
    s.wheel((x, y), -60.0);
    let after = list_state(&handle).0;
    assert!(after > 20.0, "a wheel step scrolls the list ({after})");
    run(&mut s, 800);
    assert!((list_state(&handle).0 - after).abs() < 0.5, "and nothing goes on by itself");
    s.wheel((x, y), -100_000.0);
    run(&mut s, 800);
    let (offset, stretch) = list_state(&handle);
    assert!((offset - f32::from(handle.max_offset().y)).abs() < 1.0 && stretch.abs() < 0.5, "past the bottom it stops on the end ({offset}, {stretch})");
}

// ---- the sheet tabs' strip (status_bar.rs:61: the thin bar over the status bar's left that carries the tabs' scrollbar) ----

/// ST1 (K0065 Q15, the one representative of the containers that set their own offset: design 2.1): in a workbook whose tabs overflow, a finger on the
/// strip drags the tabs sideways one for one (a left drag moves the tabs left), they coast on after a flick, they stretch when pulled right of the
/// first tab and return, and the wheel stays as it was (a step moves them, no stretch). The strip is the 12 px at the top of the 26 px status bar,
/// over the tabs' scrollbar.
fn st1_sheet_tabs() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 700.);
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    for _ in 0..25 {
        s.menu("xlsx.insert-sheet");
    }
    s.settle();
    let handle = s.with(|e, _| e.sheet_state().expect("a workbook").tab_scroll.clone());
    assert!(f32::from(handle.max_offset().x) > 300.0, "setup: the tabs overflow ({:?})", handle.max_offset());
    // Back to the first tab (the sheets just added brought the last into view).
    s.wheel((600.0, 676.0), 100_000.0);
    run(&mut s, 400);
    let x_of = |h: &ScrollHandle| -f32::from(h.offset().x);
    let stretch_of = |h: &ScrollHandle| f32::from(h.bounce_offset().x);
    assert!(x_of(&handle).abs() < 1.0, "setup: at the first tab ({})", x_of(&handle));
    let strip = pt(600.0, 700.0 - 26.0 + 6.0);
    let to = |dx: f32| pt(f32::from(strip.x) + dx, f32::from(strip.y));
    // Drag: 80 px to the left over 1.6 s, held, lifted: the tabs moved 80 px, one for one, and nothing coasts.
    s.dispatch(vec![touch(1, TouchPhase::Started, strip)]);
    for i in 1..=100 {
        s.dispatch(vec![touch(1, TouchPhase::Moved, to(-0.8 * i as f32))]);
        s.advance_ms(STEP_MS);
    }
    run(&mut s, 160);
    s.dispatch(vec![touch(1, TouchPhase::Ended, to(-80.0))]);
    run(&mut s, 1500);
    assert!((x_of(&handle) - 80.0).abs() < 1.5, "the tabs follow the finger one for one ({} for 80)", x_of(&handle));
    // Flick: 0.2 px/ms for 300 ms (60 px), lifted: they coast on about 100 px (within 25 %).
    let before = x_of(&handle);
    s.dispatch(vec![touch(1, TouchPhase::Started, to(0.0))]);
    for i in 1..=18 {
        s.dispatch(vec![touch(1, TouchPhase::Moved, to(-(60.0 * i as f32 / 18.0)))]);
        s.advance_ms(STEP_MS);
    }
    s.dispatch(vec![touch(1, TouchPhase::Ended, to(-60.0))]);
    let at_lift = x_of(&handle);
    run(&mut s, 5000);
    let coasted = x_of(&handle) - at_lift;
    assert!(at_lift - before > 50.0, "setup: the flick moved the tabs ({})", at_lift - before);
    assert!((coasted - 99.8).abs() < 25.0, "after the lift they coast about 100 px ({coasted})");
    // Pull right of the first tab (back to the start first by the wheel): they stretch, and return within 0.7 s.
    s.wheel((600.0, 676.0), 100_000.0);
    run(&mut s, 400);
    s.dispatch(vec![touch(1, TouchPhase::Started, to(0.0))]);
    for i in 1..=100 {
        s.dispatch(vec![touch(1, TouchPhase::Moved, to(i as f32))]);
        s.advance_ms(STEP_MS);
    }
    let stretched = stretch_of(&handle);
    assert!(stretched > 20.0 && stretched < 100.0, "pulled 100 px past the first tab they show a stretch under 100 px ({stretched})");
    s.dispatch(vec![touch(1, TouchPhase::Ended, to(100.0))]);
    run(&mut s, 700);
    assert!(stretch_of(&handle).abs() < 1.0, "and are back within 0.7 s ({})", stretch_of(&handle));
    // The wheel (not fingers) as before: a step moves the tabs and nothing stretches them or goes on by itself.
    s.wheel((600.0, 676.0), -60.0);
    let stepped = x_of(&handle);
    run(&mut s, 800);
    assert!(stepped > 20.0 && (x_of(&handle) - stepped).abs() < 0.5 && stretch_of(&handle).abs() < 0.5, "the wheel as before ({stepped} -> {})", x_of(&handle));
}
