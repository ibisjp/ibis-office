// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0073: on a touch screen a long press opens the context menu a right click opens (design: `tasks/doc/test-design/K0073.md`; the numbers
//! P1... are that document's). The fingers are sent as raw `TouchEvent`s with the mock clock moving between them (as K0065's scenarios do):
//! the platforms' own touch paths (iOS, Android, Windows `WM_POINTER`, macOS touch) are not exercised here but on the devices
//! (`tasks/doc/requests/2026-11-25-k0073-long-press-devices.md`), and no scenario here may be taken as proof of them.
//!
//! The numbers: the press time is 500 ms (Android `ViewConfiguration.getLongPressTimeout()` default 500 ms; iOS
//! `UILongPressGestureRecognizer.minimumPressDuration` default 0.5 s); the finger may move 8 px (gpui's `touch_slop`, Android's touch slop
//! is 8 dp) and a move of 14 px is beyond both Android's 8 dp and iOS's `allowableMovement` of 10 pt. The tests use 450 ms (not yet) and
//! 600 ms (long enough), 4 px (within) and 14 px (beyond), so that they hold for either platform's value.
use super::Scenario;
use crate::shell::scenario::button_probe as buttons;
use gpui_kit::*;
use crate::docx::scenarios::DocSteps;
use ibis_office_docx::ext::*;
use ibis_office_xlsx::ext::*;

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0073_p1_a_long_press_on_the_body_opens_the_menu_on_the_three_screens", p1_body),
        ("shell::k0073_p2_the_press_time_is_500_ms", p2_time),
        ("shell::k0073_p3_a_finger_that_moves_far_does_not_open_it_and_one_that_stays_does", p3_movement),
        ("shell::k0073_p4_one_finger_selection_and_drag_are_not_taken_for_a_long_press", p4_selection_and_drag),
        ("shell::k0073_p5_lifting_the_finger_leaves_the_menu_open_and_runs_nothing", p5_release),
        ("shell::k0073_p6_a_mouse_button_held_does_not_open_it", p6_mouse_hold),
        ("shell::k0073_p7_a_second_finger_cancels_it", p7_second_finger),
        ("shell::k0073_p8_the_caret_goes_to_the_press_as_for_a_right_click", p8_caret),
        ("shell::k0073_p9_mouse_and_touch_both_work_in_any_order", p9_both_devices),
        ("shell::k0073_p10_a_press_outside_closes_it_and_goes_nowhere", p10_outside),
        ("shell::k0073_p11_the_start_view_list_opens_its_menu", p11_start_view),
        ("shell::k0073_lx1_a_long_press_on_a_column_header_opens_its_menu", lx1_column_header),
        ("shell::k0073_lp1_a_long_press_on_a_thumbnail_opens_its_menu", lp1_thumbnail),
    ]
}

fn pt(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
}

fn touch(id: u64, phase: TouchPhase, at: Point<Pixels>) -> PlatformInput {
    PlatformInput::Touch(TouchEvent { id: TouchId(id), phase, position: at, predicted_position: None, force: None })
}

const STEP_MS: u64 = 10;

/// The finger goes down at `at` and the clock runs `ms` with the finger still.
fn press(s: &mut Scenario, at: (f32, f32), ms: u64) {
    s.dispatch(vec![touch(1, TouchPhase::Started, pt(at.0, at.1))]);
    wait_still(s, ms);
}

fn wait_still(s: &mut Scenario, ms: u64) {
    for _ in 0..(ms / STEP_MS) {
        s.advance_ms(STEP_MS);
    }
}

fn lift(s: &mut Scenario, at: (f32, f32)) {
    s.dispatch(vec![touch(1, TouchPhase::Ended, pt(at.0, at.1))]);
}

fn menu_open(s: &mut Scenario) -> bool {
    // Each screen keeps its context menu in its own place: the document's `link_popup`, the sheet's `header_popup`, the slide's `shape_edit.popup`.
    s.with(|e, _| e.link_popup.is_some() || e.sheet_popup_open() || e.shape_edit.popup.is_some())
}

fn close_menu(s: &mut Scenario) {
    s.update(|e, _, cx| {
        e.link_popup = None;
        e.shape_edit.popup = None;
        e.close_sheet_popup();
        cx.notify();
    });
}

fn text_screen_pos(s: &mut Scenario, pos: ibis_doc_model::FlowPos) -> (f32, f32) {
    let (page, x, y, h) = s.update(|e, _, cx| {
        let lines = e.flow_lines();
        let (page, x, y, h) = e.caret_box(&lines, pos).expect("the position is laid out");
        e.scroll_into_view(page, y, h, cx);
        (page, x, y, h)
    });
    let (origin, k) = s.with(|e, _| e.page_origin(page)).expect("the page is placed");
    (f32::from(origin.x) + x * k + 0.5, f32::from(origin.y) + (y + h / 2.0) * k)
}

/// A screen of the kind (one at a time: the driver allows one open scenario) and a point on its body where a right click opens the menu.
fn screen(kind: &str) -> (Scenario, (f32, f32)) {
    match kind {
        "docx" => {
            let mut d = Scenario::open("test/editing/docx-report.docx");
            d.size(1200., 900.);
            let (para, _) = d.paragraph(2);
            let at = text_screen_pos(&mut d, ibis_doc_model::FlowPos::new(para, 2));
            (d, at)
        }
        "xlsx" => {
            let mut x = Scenario::open("test/editing/xlsx-sales.xlsx");
            x.size(1200., 900.);
            let c = x.with(|e, _| e.cell_center(2, 3)).expect("a cell is on screen");
            (x, (c.x.into(), c.y.into()))
        }
        _ => {
            let mut p = Scenario::open("test/benchmark/earnings-presentation.pptx");
            p.size(1200., 900.);
            let at = p.shape_position(0);
            (p, at)
        }
    }
}

const KINDS: [&str; 3] = ["docx", "xlsx", "pptx"];

/// P1: a finger held still 600 ms opens the menu while it is still down (as iOS and Android do), on the three screens.
fn p1_body() {
    for name in KINDS {
        let (mut s, at) = screen(name);
        assert!(!menu_open(&mut s), "{name}: no menu before");
        press(&mut s, at, 600);
        assert!(menu_open(&mut s), "{name}: the menu is open after 600 ms with the finger still down");
        lift(&mut s, at);
        assert!(menu_open(&mut s), "{name}: it stays open after the finger lifts");
    }
}

/// P2: 450 ms is not a long press (a lift then is a tap: no menu), 600 ms is.
fn p2_time() {
    for name in KINDS {
        let (mut s, at) = screen(name);
        press(&mut s, at, 450);
        assert!(!menu_open(&mut s), "{name}: no menu at 450 ms");
        lift(&mut s, at);
        assert!(!menu_open(&mut s), "{name}: a tap that ends before 500 ms opens no menu");
        press(&mut s, at, 600);
        assert!(menu_open(&mut s), "{name}: the menu opens at 600 ms");
        lift(&mut s, at);
    }
}

/// P3: moving 14 px before the time is up is a drag (no menu); moving 4 px is still a long press.
fn p3_movement() {
    for name in KINDS {
        let (mut s, at) = screen(name);
        s.dispatch(vec![touch(1, TouchPhase::Started, pt(at.0, at.1))]);
        wait_still(&mut s, 200);
        s.dispatch(vec![touch(1, TouchPhase::Moved, pt(at.0 + 14., at.1))]);
        wait_still(&mut s, 600);
        assert!(!menu_open(&mut s), "{name}: a finger that moved 14 px opens no menu");
        lift(&mut s, (at.0 + 14., at.1));
        close_menu(&mut s);
        s.dispatch(vec![touch(1, TouchPhase::Started, pt(at.0, at.1))]);
        wait_still(&mut s, 200);
        s.dispatch(vec![touch(1, TouchPhase::Moved, pt(at.0 + 4., at.1))]);
        wait_still(&mut s, 400);
        assert!(menu_open(&mut s), "{name}: a finger that wandered 4 px still opens it");
        lift(&mut s, (at.0 + 4., at.1));
    }
}

/// P4 (K0065): a one finger drag on the body still selects (docx: the text between the points) and a long press afterwards is separate.
fn p4_selection_and_drag() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1200., 900.);
    let (para, _) = s.paragraph(2);
    let a = text_screen_pos(&mut s, ibis_doc_model::FlowPos::new(para, 1));
    let b = text_screen_pos(&mut s, ibis_doc_model::FlowPos::new(para, 6));
    s.dispatch(vec![touch(1, TouchPhase::Started, pt(a.0, a.1))]);
    for i in 1..=10 {
        let x = a.0 + (b.0 - a.0) * i as f32 / 10.;
        s.dispatch(vec![touch(1, TouchPhase::Moved, pt(x, a.1))]);
        s.advance_ms(30);
    }
    lift(&mut s, b);
    assert!(!menu_open(&mut s), "a drag that takes 300 ms and moves far opens no menu");
    assert_ne!(s.with(|e, _| e.ordered_selection().map(|(x, y)| x == y)), Some(true), "the drag selected text (K0065)");
    // A tap that is not held opens none either.
    s.dispatch(vec![touch(1, TouchPhase::Started, pt(a.0, a.1))]);
    s.advance_ms(60);
    lift(&mut s, a);
    s.wait(700);
    assert!(!menu_open(&mut s), "a tap opens no menu, even when time goes by after it");
}

/// P5: lifting after the menu opened leaves it open; the line under the finger does not run.
fn p5_release() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1200., 900.);
    let (para, text) = s.with(|e, _| e.flow().unwrap().body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some((p.id, p.text())).filter(|(_, t)| t.chars().count() > 6) } else { None }).unwrap());
    let at = text_screen_pos(&mut s, ibis_doc_model::FlowPos::new(para, 2));
    press(&mut s, at, 600);
    lift(&mut s, at);
    assert!(menu_open(&mut s), "the menu stays open after the lift");
    assert_eq!(s.with(|e, _| e.flow().unwrap().body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { (p.id == para).then(|| p.text()) } else { None })), Some(text), "no line ran (the text is the same)");
}

/// P6: a mouse button held down for 700 ms opens no menu (the mouse has the right button).
fn p6_mouse_hold() {
    for name in KINDS {
        let (mut s, at) = screen(name);
        let position = pt(at.0, at.1);
        s.dispatch(vec![PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }), PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() })]);
        wait_still(&mut s, 700);
        assert!(!menu_open(&mut s), "{name}: a held mouse button opens no menu");
        s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() })]);
        assert!(!menu_open(&mut s), "{name}: nor does its release");
    }
}

/// P7: a second finger down before the time is up is a scroll or a pinch, never the menu (K0065).
fn p7_second_finger() {
    for name in KINDS {
        let (mut s, at) = screen(name);
        s.dispatch(vec![touch(1, TouchPhase::Started, pt(at.0, at.1))]);
        wait_still(&mut s, 300);
        s.dispatch(vec![touch(2, TouchPhase::Started, pt(at.0 + 80., at.1))]);
        wait_still(&mut s, 600);
        assert!(!menu_open(&mut s), "{name}: two fingers down open no menu");
        s.dispatch(vec![touch(1, TouchPhase::Ended, pt(at.0, at.1)), touch(2, TouchPhase::Ended, pt(at.0 + 80., at.1))]);
    }
}

/// P8: as for a right click (#140), the caret goes to the press unless it is inside the selection, which stays.
fn p8_caret() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1200., 900.);
    let (para, _) = s.paragraph(2);
    s.update(|e, _, cx| e.set_caret(ibis_doc_model::FlowPos::new(para, 0), false, cx));
    let at = text_screen_pos(&mut s, ibis_doc_model::FlowPos::new(para, 3));
    press(&mut s, at, 600);
    lift(&mut s, at);
    assert!(menu_open(&mut s));
    let sel = s.with(|e, _| e.text_selection()).expect("a caret");
    assert_eq!(sel.0, sel.1, "no selection made");
    assert_eq!(sel.0.offset, 3, "the caret is where the finger was");
    close_menu(&mut s);
    s.update(|e, _, cx| {
        e.set_caret(ibis_doc_model::FlowPos::new(para, 0), false, cx);
        e.set_caret(ibis_doc_model::FlowPos::new(para, 6), true, cx);
    });
    let inside = text_screen_pos(&mut s, ibis_doc_model::FlowPos::new(para, 3));
    press(&mut s, inside, 600);
    lift(&mut s, inside);
    assert_eq!(s.with(|e, _| e.ordered_selection()), Some((ibis_doc_model::FlowPos::new(para, 0), ibis_doc_model::FlowPos::new(para, 6))), "a long press inside a selection keeps it");
}

/// P9: a device with both: right click, long press, right click, long press, each opens it (the last input kind changes between them).
fn p9_both_devices() {
    for name in KINDS {
        let (mut s, at) = screen(name);
        for round in 0..2 {
            s.right_click(at);
            assert!(menu_open(&mut s), "{name}: right click opens it (round {round})");
            close_menu(&mut s);
            s.hover((at.0 + 3., at.1));
            press(&mut s, at, 600);
            assert!(menu_open(&mut s), "{name}: the long press opens it after the mouse was used (round {round})");
            lift(&mut s, at);
            close_menu(&mut s);
        }
    }
}

/// P10 (with K0074): with the menu open from a long press, a tap outside closes it and the body does not get that tap.
fn p10_outside() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 900.);
    let c = s.with(|e, _| e.cell_center(2, 3)).expect("a cell is on screen");
    press(&mut s, (c.x.into(), c.y.into()), 600);
    lift(&mut s, (c.x.into(), c.y.into()));
    assert!(menu_open(&mut s));
    let before = crate::xlsx::scenarios::SheetSteps::selected(&mut s);
    // A cell to the left of the menu (the menu opens at the finger and is 350 px wide, so a cell to the right of the finger would be under it).
    let other = s.with(|e, _| e.cell_center(0, 8)).expect("a cell is on screen");
    // A tap: the finger is down for a moment (60 ms) and lifts.
    s.dispatch(vec![touch(1, TouchPhase::Started, other)]);
    s.advance_ms(60);
    s.dispatch(vec![touch(1, TouchPhase::Ended, other)]);
    s.advance_ms(60);
    assert!(!menu_open(&mut s), "the tap closes the menu");
    assert_eq!(crate::xlsx::scenarios::SheetSteps::selected(&mut s), before, "the tap did not select the cell under it");
}

/// P11: the start view's list of recent files (its row's context menu).
fn p11_start_view() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::start_view();
    let file = s.screenshot_path("k0073-recent").with_extension("docx");
    std::fs::write(&file, ibis_formats::blank_file(ibis_formats::Kind::Document).unwrap()).unwrap();
    let file = file.canonicalize().unwrap();
    s.update(|e, _, cx| {
        e.recent_files = crate::shell::recent_files::add(&file);
        cx.notify();
    });
    let b = buttons::bounds("row-actions-0").expect("a recent row");
    let at = (f32::from(b.origin.x) - 80., f32::from(b.center().y));
    press(&mut s, at, 600);
    assert!(menu_open(&mut s), "a long press on a recent file opens its menu");
    lift(&mut s, at);
    assert!(!s.with(|e, _| e.has_document), "the file was not opened by the lift");
    let _ = std::fs::remove_file(&file);
}

/// L-X1: the places outside the sheet body (the page's touch drag does not claim them; the window's long press does): a column header opens its menu.
fn lx1_column_header() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 900.);
    let c = s.with(|e, _| e.cell_center(2, 0)).expect("a cell is on screen");
    // The column headers lie above the first row (the setup is checked with the mouse's right click).
    let at = (f32::from(c.x), f32::from(c.y) - 25.);
    s.right_click(at);
    assert!(menu_open(&mut s), "setup: a right click on the column header opens its menu");
    close_menu(&mut s);
    press(&mut s, at, 600);
    assert!(menu_open(&mut s), "a long press on the column header opens its menu");
    lift(&mut s, at);
    assert!(menu_open(&mut s), "it stays open after the lift");
}

/// L-P1: a slide's thumbnail (the pane at the left) opens the slide menu.
fn lp1_thumbnail() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1200., 900.);
    let mut found = None;
    for y in [180., 220., 260., 300., 340.] {
        s.right_click((90., y));
        if menu_open(&mut s) {
            found = Some((90., y));
            close_menu(&mut s);
            break;
        }
    }
    let at = found.expect("setup: a right click on a thumbnail opens its menu");
    press(&mut s, at, 600);
    assert!(menu_open(&mut s), "a long press on a thumbnail opens its menu");
    lift(&mut s, at);
    assert!(menu_open(&mut s), "it stays open after the lift");
}
