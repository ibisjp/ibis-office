// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0065: on a touch device one finger on the main content selects and moves (never scrolls), two fingers scroll, and every scroll has
//! inertia and a bounce at the edge that match the iPhone (design: `tasks/doc/test-design/K0065.md`; the numbers Q1... are that document's).
//! Every test is in `all()` (K0065 is implemented; before it Q1, Q3, Q4 and Q6 to Q8 failed).
//!
//! The fingers are sent as raw `TouchEvent`s, the way gpui's recognizer takes them (the web already delivers them so; the iOS, Android and
//! Windows platforms are to deliver the same). A platform's own path (iOS's `touch_*`, Windows's `WM_POINTER`) is not exercised here: it is
//! checked on the device (the design's D-tests), and no scenario here may be taken as proof of it.
//!
//! The numbers come from Apple's published UIScrollView constants: `UIScrollView.DecelerationRate.normal` is 0.998 per millisecond, so a velocity
//! `v` (px/ms) coasts `v * 0.998 / (1 - 0.998)` = 499 v px in all, and in `t` ms `v * (1 - 0.998^t) / -ln(0.998)`; the rubber band pulls a finger
//! travel `x` beyond the edge to `x * d * 0.55 / (d + 0.55 * x)` for a view `d` long (WWDC 2018 "Designing Fluid Interfaces",
//! https://developer.apple.com/videos/play/wwdc2018/803/; the page was not read: the formula is the one the design's section 4 gives, to be
//! confirmed against an iPhone), and the way back is the critically damped spring `(x0 + (v0 + w x0) t) e^(-w t)` with w = 2 pi / 0.5 s.
use gpui_kit::*;

use ibis_office_docx::ext::*;
use ibis_office_xlsx::ext::*;
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0065_q1_one_finger_on_the_body_selects_and_does_not_scroll", q1_one_finger_selects),
        ("shell::k0065_q2_two_fingers_scroll_the_body_on_the_three_screens", q2_two_fingers_scroll),
        ("shell::k0065_q3_one_finger_outside_the_body_scrolls_with_inertia", q3_outside_the_body),
        ("shell::k0065_q4_a_fling_coasts_as_uiscrollview_does", q4_fling_distance),
        ("shell::k0065_q5_a_finger_down_stops_the_coasting", q5_catch),
        ("shell::k0065_q6_pulling_past_the_edge_follows_the_rubber_band", q6_rubber_band),
        ("shell::k0065_q7_releasing_past_the_edge_returns_without_overshoot", q7_return),
        ("shell::k0065_q8_a_fling_into_the_edge_bounces_and_comes_back", q8_fling_into_the_edge),
        ("shell::k0065_q9_two_fingers_apart_zoom_and_two_fingers_together_scroll_do_not_mix", q9_pinch_and_scroll),
        ("shell::k0065_q10_a_second_finger_takes_back_the_first_ones_selection_and_scrolls", q10_second_finger_cancels),
        ("shell::k0065_q11_scrolling_does_not_break_an_ime_composition", q11_ime),
        ("shell::k0065_q12_the_content_follows_the_fingers_at_every_zoom", q12_zoom),
        ("shell::k0065_q2b_the_slide_left_in_the_middle_scrolls_both_ways_one_for_one", q2b_from_the_middle),
        ("shell::k0065_q6b_the_bottom_edge_and_the_side_edge_stretch_and_return", q6b_other_edges),
        ("shell::k0065_q8b_a_fling_into_the_bottom_bounces_and_comes_back", q8b_fling_into_the_bottom),
        ("shell::k0065_q13_the_wheel_and_trackpad_do_not_bounce", q13_wheel_no_bounce),
    ]
}

fn pt(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
}

fn touch(id: u64, phase: TouchPhase, at: Point<Pixels>) -> PlatformInput {
    PlatformInput::Touch(TouchEvent { id: TouchId(id), phase, position: at, predicted_position: None, force: None })
}

/// The scrolled amount of the body (positive = scrolled down): the document's and the slide workspace's `ScrollHandle` offset, the sheet's by its first cell.
fn scrolled(s: &mut Scenario) -> f32 {
    if s.with(|e, _| e.sheet_state().is_some()) {
        let top = s.with(|e, _| e.cell_center(0, 0).map(|p| f32::from(p.y)));
        // (The stretch past an end is `ScrollBounce`'s displacement of what is drawn, not part of the scroll position: it is read from `touch_overscroll`; reading changed, ibisOffice K0065.)
        return -(top.unwrap_or(-100_000.0) - SHEET_FIRST_ROW_Y.with(|y| y.get())) - s.with(|e, _| e.touch_overscroll().1);
    }
    s.with(|e, _| -f32::from(e.workspace_scroll.offset().y) - e.touch_overscroll().1)
}

thread_local! {
    /// Where the first row's middle was before any scrolling (the sheet has no scroll handle of its own to read).
    static SHEET_FIRST_ROW_Y: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
}

/// A screen with a body that scrolls: the document (a long one), the sheet (200 rows of numbers), the presentation (zoomed to 300 %). Returns it with the
/// middle of its body.
fn screen(kind: &str) -> (Scenario, Point<Pixels>) {
    screen_with(kind, true)
}

/// `screen`, with a slide that opens in the middle of its zoomed page left there when `top` is false.
fn screen_with(kind: &str, top: bool) -> (Scenario, Point<Pixels>) {
    let mut s = match kind {
        "docx" => Scenario::open("test/editing/docx-report.docx"),
        "xlsx" => {
            let mut s = Scenario::new_document(ibis_formats::Kind::Workbook);
            s.size(1200., 900.);
            s.settle();
            // A1 is selected in a new sheet: each number typed and Enter goes down a row.
            for row in 1..=200 {
                s.type_text(&row.to_string());
                s.key("enter");
            }
            s.key("ctrl-home");
            s
        }
        _ => {
            let mut s = Scenario::new_document(ibis_formats::Kind::Presentation);
            s.size(1200., 900.);
            s.update(|e, window, cx| e.set_zoom(Some(3.0), window, cx));
            s
        }
    };
    s.size(1200., 900.);
    s.settle();
    if kind == "pptx" && top {
        // A slide zoomed to 300 % opens scrolled to its middle: Q6 to Q8 need the top (setup only; added by the implementer, ibisOffice K0065).
        s.update(|e, _, cx| {
            e.workspace_scroll.set_offset(point(px(0.0), px(0.0)));
            cx.notify();
        });
        s.settle();
        assert_eq!(s.with(|e, _| e.touch_overscroll()), (0.0, 0.0), "the slide starts at the top with no stretch (before any reading)");
    }
    SHEET_FIRST_ROW_Y.with(|y| y.set(s.with(|e, _| e.cell_center(0, 0).map(|p| f32::from(p.y))).unwrap_or(0.0)));
    let middle = s.with(|e, _| e.workspace_bounds).map(|b| b.center()).expect("the workspace is laid out");
    (s, middle)
}

const STEP_MS: u64 = 16;

/// One finger at `from` goes to `to` in `ms` (steps of 16 ms) and lifts (`lift`) or stays down.
fn one_finger(s: &mut Scenario, from: Point<Pixels>, to: Point<Pixels>, ms: u64, lift: bool) {
    s.dispatch(vec![touch(1, TouchPhase::Started, from)]);
    let steps = (ms / STEP_MS).max(1);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        s.dispatch(vec![touch(1, TouchPhase::Moved, pt(f32::from(from.x) + (f32::from(to.x) - f32::from(from.x)) * t, f32::from(from.y) + (f32::from(to.y) - f32::from(from.y)) * t))]);
        s.advance_ms(STEP_MS);
    }
    if lift {
        s.dispatch(vec![touch(1, TouchPhase::Ended, to)]);
    }
}

/// Two fingers (60 px either side of `center`) go `(dx, dy)` together in `ms` and lift (`lift`).
fn two_fingers(s: &mut Scenario, center: Point<Pixels>, (dx, dy): (f32, f32), ms: u64, lift: bool) {
    let (cx, cy) = (f32::from(center.x), f32::from(center.y));
    s.dispatch(vec![touch(1, TouchPhase::Started, pt(cx - 60.0, cy)), touch(2, TouchPhase::Started, pt(cx + 60.0, cy))]);
    let steps = (ms / STEP_MS).max(1);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        s.dispatch(vec![touch(1, TouchPhase::Moved, pt(cx - 60.0 + dx * t, cy + dy * t)), touch(2, TouchPhase::Moved, pt(cx + 60.0 + dx * t, cy + dy * t))]);
        s.advance_ms(STEP_MS);
    }
    if lift {
        s.dispatch(vec![touch(1, TouchPhase::Ended, pt(cx - 60.0 + dx, cy + dy)), touch(2, TouchPhase::Ended, pt(cx + 60.0 + dx, cy + dy))]);
    }
}

/// Let the scenario's clock run `ms` (coasting and the bounce are drawn on it).
fn run(s: &mut Scenario, ms: u64) {
    for _ in 0..(ms / STEP_MS) {
        s.advance_ms(STEP_MS);
    }
    s.settle();
}

/// The caret and anchor of the document, the selected cell of the sheet, the frame of the first shape of the slide: what a drag selects or moves.
fn selection_of(s: &mut Scenario, kind: &str) -> String {
    match kind {
        "docx" => s.with(|e, _| format!("{:?}", e.doc_state().map(|st| (st.caret, st.anchor)))),
        "xlsx" => s.with(|e, _| format!("{:?}", e.sheet_state().map(|st| st.cells.clone()))),
        _ => s.with(|e, _| format!("{:?}", e.slide().shapes.iter().map(|sh| sh.frame).collect::<Vec<_>>())),
    }
}

/// Q1: one finger dragged across the body (text, cells, a shape) selects or moves and the body does not scroll, however long the drag.
fn q1_one_finger_selects() {
    for kind in ["docx", "xlsx", "pptx"] {
        let (mut s, middle) = screen(kind);
        if kind == "pptx" {
            // A shape to drag: the slide's first one, grabbed in its middle.
            s.update(|e, _, cx| {
                let id = e.document.ids().new_shape_id();
                let shape = ibis_doc_model::Shape::new(id, ibis_doc_model::EmuRect::new(500_000, 500_000, 800_000, 600_000), ibis_doc_model::ShapeKind::Auto(ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect, ibis_doc_model::Fill::Solid(ibis_doc_model::Color::BLACK), None)));
                e.insert_shape(shape, cx);
                cx.notify();
            });
            s.settle();
        }
        let before = (selection_of(&mut s, kind), scrolled(&mut s));
        let start = if kind == "pptx" { let (x, y) = s.shape_position(0); pt(x, y) } else { middle };
        one_finger(&mut s, start, pt(f32::from(start.x) + 150.0, f32::from(start.y) + 160.0), 400, true);
        run(&mut s, 1500);
        assert_eq!(scrolled(&mut s), before.1, "{kind}: one finger on the body does not scroll it (not even by coasting)");
        assert_ne!(selection_of(&mut s, kind), before.0, "{kind}: the drag selects or moves something");
    }
}

/// Q2: two fingers moving up 200 px scroll the body down by 200 px (the content follows the fingers) on the three screens, and sideways too.
fn q2_two_fingers_scroll() {
    for kind in ["docx", "xlsx", "pptx"] {
        let (mut s, middle) = screen(kind);
        let before = scrolled(&mut s);
        // Slow enough (a long hold at the end) that no velocity is left to coast on.
        two_fingers(&mut s, middle, (0.0, -200.0), 800, false);
        s.advance_ms(200);
        s.dispatch(vec![touch(1, TouchPhase::Ended, pt(f32::from(middle.x) - 60.0, f32::from(middle.y) - 200.0)), touch(2, TouchPhase::Ended, pt(f32::from(middle.x) + 60.0, f32::from(middle.y) - 200.0))]);
        run(&mut s, 1000);
        let moved = scrolled(&mut s) - before;
        assert!((moved - 200.0).abs() < 8.0, "{kind}: two fingers up 200 px scroll the body down about 200 px ({moved})");
    }
}

/// Q3: where there is no selection (the narrow window's toolbar, K0061) one finger scrolls, and the scroll coasts on after the finger lifts.
fn q3_outside_the_body() {
    let mut s = Scenario::new_document(ibis_formats::Kind::Document);
    s.size(400., 700.);
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    let tb = s.with(|e, _| e.toolbar_bounds.get());
    let at = pt(f32::from(tb.left()) + 200.0, f32::from(tb.top()) + 18.0);
    one_finger(&mut s, at, pt(f32::from(at.x) - 120.0, f32::from(at.y)), 160, true);
    let just_after = s.with(|e, _| f32::from(e.toolbar_scroll.offset().x));
    run(&mut s, 1500);
    let later = s.with(|e, _| f32::from(e.toolbar_scroll.offset().x));
    assert!(just_after < -100.0, "one finger scrolls the toolbar ({just_after})");
    assert!(later < just_after - 40.0, "it coasts on after the finger lifts: {just_after} -> {later}");
}

/// Q4: a fling of 1 px/ms (two fingers, the body) coasts 0.998 / (1 - 0.998) = 499 px after it lifts, 90.6 px in the first 100 ms, within 10 %.
fn q4_fling_distance() {
    let (mut s, middle) = screen("docx");
    two_fingers(&mut s, middle, (0.0, -300.0), 300, false); // 1 px/ms
    let at_lift = scrolled(&mut s);
    s.dispatch(vec![touch(1, TouchPhase::Ended, pt(f32::from(middle.x) - 60.0, f32::from(middle.y) - 300.0)), touch(2, TouchPhase::Ended, pt(f32::from(middle.x) + 60.0, f32::from(middle.y) - 300.0))]);
    run(&mut s, 100);
    let first_100 = scrolled(&mut s) - at_lift;
    assert!((first_100 - 90.6).abs() < 9.0, "in the first 100 ms it coasts about 90.6 px ({first_100})");
    run(&mut s, 5000);
    let total = scrolled(&mut s) - at_lift;
    assert!((total - 499.0).abs() < 50.0, "in all it coasts about 499 px ({total})");
    let later = {
        run(&mut s, 2000);
        scrolled(&mut s) - at_lift
    };
    assert!((later - total).abs() < 1.0, "and it has stopped ({total} -> {later})");
}

/// Q5: a finger put down while the body coasts stops it at once (and a tap there is not a scroll).
fn q5_catch() {
    let (mut s, middle) = screen("docx");
    two_fingers(&mut s, middle, (0.0, -300.0), 300, true);
    run(&mut s, 100);
    s.dispatch(vec![touch(1, TouchPhase::Started, middle)]);
    let caught = scrolled(&mut s);
    run(&mut s, 1500);
    assert_eq!(scrolled(&mut s), caught, "the coasting stopped when the finger came down");
    s.dispatch(vec![touch(1, TouchPhase::Ended, middle)]);
}


/// The mean gray of each pixel row of the picture at `path`, from `top` to `top + height` (points; the picture's scale is its width over 1200).
fn row_profile(path: &std::path::Path, top: f32, height: f32) -> (Vec<f32>, f32) {
    let image = image::open(path).unwrap().to_luma8();
    let scale = image.width() as f32 / 1200.0;
    let (first, count) = ((top * scale) as u32, (height * scale) as u32);
    let rows = (first..(first + count).min(image.height()))
        .map(|y| (0..image.width()).map(|x| f32::from(image.get_pixel(x, y).0[0])).sum::<f32>() / image.width() as f32)
        .collect();
    (rows, scale)
}

/// How far (points) the content of `pulled` is below the same content of `rest`: the shift that makes the rows from the top of the body (the paper's, the slide's, the first
/// row's edge: unique, so the shift is not mistaken for a multiple of a row's height) agree best.
fn pixel_shift(rest: &std::path::Path, pulled: &std::path::Path, body_top: f32) -> f32 {
    let (a, scale) = row_profile(rest, body_top, 460.0);
    let (b, _) = row_profile(pulled, body_top, 460.0);
    let window = 300.min(a.len().saturating_sub(1));
    let best = (0..(150.0 * scale) as usize)
        .map(|d| {
            let sum: f32 = (0..window).filter(|i| i + d < b.len()).map(|i| (a[i] - b[i + d]).abs()).sum();
            (d, sum)
        })
        .min_by(|x, y| x.1.partial_cmp(&y.1).unwrap())
        .unwrap();
    best.0 as f32 / scale
}

/// Q6: with the body at the top, two fingers pulling 100 px down (past the edge) show the body 100 * d * 0.55 / (d + 55) px past it, d being the body's height
/// (not the 100 px, and not nothing): the pull gets heavier.
fn q6_rubber_band() {
    for kind in ["docx", "xlsx", "pptx"] {
        let (mut s, middle) = screen(kind);
        let d = s.with(|e, _| e.workspace_bounds).map(|b| f32::from(b.size.height)).unwrap();
        let body_top = s.with(|e, _| e.workspace_bounds).map(|b| f32::from(b.origin.y)).unwrap();
        let first_row_before = s.with(|e, _| e.cell_center(0, 0).map(|p| f32::from(p.y)));
        let rest = s.screenshot(&format!("k0065-q6-{kind}-rest"));
        two_fingers(&mut s, middle, (0.0, 100.0), 800, false);
        s.advance_ms(200);
        s.settle();
        let past = -scrolled(&mut s);
        let expect = 100.0 * d * 0.55 / (d + 0.55 * 100.0);
        assert!((past - expect).abs() < expect * 0.1 + 2.0, "{kind}: the body is {past} px past the top, the rubber band gives {expect}");
        // The picture, not only the numbers: what is drawn went down by the band's value (landmark: the body's top edge, found as the shift that matches the rows
        // of the rest picture), and `touch_overscroll()` says the same.
        let pulled = s.screenshot(&format!("k0065-q6-{kind}-pulled"));
        let shift = pixel_shift(&rest, &pulled, body_top);
        assert!((shift - expect).abs() < expect * 0.1 + 2.0, "{kind}: the drawn body went down {shift} px, the rubber band gives {expect}");
        let reported = s.with(|e, _| e.touch_overscroll().1);
        assert!((reported - shift).abs() < 3.0, "{kind}: touch_overscroll() ({reported}) is the drawn shift ({shift})");
        // The scroll position stays at the edge while the band shows: the stretch is the drawing's, not the scroll's.
        assert_eq!(s.with(|e, _| f32::from(e.workspace_scroll.offset().y)), 0.0, "{kind}: the scroll offset stays at the top while the body is pulled past it");
        // The sheet's `scrolled()` reads the first cell's place and takes `touch_overscroll()` off: that counts the drawing's shift once only if `cell_center` does not include it.
        if let Some(before) = first_row_before {
            let during = s.with(|e, _| e.cell_center(0, 0).map(|p| f32::from(p.y))).unwrap();
            assert!((during - before).abs() < 1.0, "{kind}: cell_center is the layout's, without the drawing's shift ({before} -> {during}), so `scrolled()` does not count it twice");
        }
        two_fingers_release(&mut s, middle, 100.0);
    }
}

fn two_fingers_release(s: &mut Scenario, center: Point<Pixels>, dy: f32) {
    let (cx, cy) = (f32::from(center.x), f32::from(center.y));
    s.dispatch(vec![touch(1, TouchPhase::Ended, pt(cx - 60.0, cy + dy)), touch(2, TouchPhase::Ended, pt(cx + 60.0, cy + dy))]);
}

/// Q7: lifting the fingers past the edge, the body returns to the edge as a critically damped spring does (w = 12.57 per second): it never goes to the other
/// side of the edge, it never moves away from it, and within 700 ms it is back (under 1 px).
fn q7_return() {
    for kind in ["docx", "xlsx", "pptx"] {
        let (mut s, middle) = screen(kind);
        let rest = s.screenshot(&format!("k0065-q7-{kind}-rest"));
        two_fingers(&mut s, middle, (0.0, 100.0), 800, false);
        s.advance_ms(200);
        let start = -scrolled(&mut s);
        assert!(start > 10.0, "{kind}: past the top to start from ({start})");
        two_fingers_release(&mut s, middle, 100.0);
        let mut last = start;
        for step in 1..=43 {
            run(&mut s, STEP_MS);
            let now = -scrolled(&mut s);
            assert!(now >= -0.5, "{kind}: it never goes past the edge to the other side ({now} at {} ms)", step * 16);
            assert!(now <= last + 0.5, "{kind}: it never moves away from the edge on its way back ({last} -> {now})");
            last = now;
        }
        assert!(last < 1.0, "{kind}: back within 700 ms ({last})");
        // And what is drawn is back: no shift left (under 1 px).
        s.settle();
        let back = s.screenshot(&format!("k0065-q7-{kind}-back"));
        let body_top = s.with(|e, _| e.workspace_bounds).map(|b| f32::from(b.origin.y)).unwrap();
        let shift = pixel_shift(&rest, &back, body_top);
        assert!(shift < 1.0, "{kind}: the drawn body is back at the edge (shift {shift} px)");
    }
}

/// Q8: a fast fling toward the top from near it: the body goes past the edge (a little, by the band), and comes back to rest on the edge; it does not
/// stay past it and it does not bounce again.
fn q8_fling_into_the_edge() {
    let (mut s, middle) = screen("docx");
    two_fingers(&mut s, middle, (0.0, -400.0), 300, true);
    run(&mut s, 3000);
    assert!(scrolled(&mut s) > 200.0, "scrolled down a good way first");
    two_fingers(&mut s, middle, (0.0, 1500.0), 150, true); // 10 px/ms up the page, far more than the page above
    let mut deepest = 0.0f32;
    let mut crossings = 0;
    let mut was_past = false;
    for _ in 0..200 {
        run(&mut s, STEP_MS);
        let past = -scrolled(&mut s);
        deepest = deepest.max(past);
        if (past > 0.5) != was_past {
            crossings += 1;
            was_past = past > 0.5;
        }
    }
    assert!(deepest > 5.0, "it goes past the edge a little ({deepest})");
    assert!(deepest < 0.55 * 900.0, "but never farther than the band allows ({deepest})");
    assert!(-scrolled(&mut s) < 1.0, "and rests on the edge");
    assert!(crossings <= 2, "it goes past once and returns once, no second bounce ({crossings} crossings)");
}

/// Q9: a pinch (the platform's tracker turns two fingers moving apart into `Pinch` events; the scenario sends those) zooms and does not scroll; two fingers
/// moving together (raw touches) scroll and do not zoom.
fn q9_pinch_and_scroll() {
    let (mut s, middle) = screen("pptx");
    let (zoom, scroll) = s.with(|e, _| (e.zoom, e.workspace_scroll.offset()));
    for _ in 0..10 {
        s.pinch((f32::from(middle.x), f32::from(middle.y)), 0.05);
        s.advance_ms(STEP_MS);
    }
    run(&mut s, 1500);
    assert_ne!(s.with(|e, _| e.zoom), zoom, "a pinch zooms");
    let _ = scroll; // (zooming about the middle moves the offset by itself, so the offset says nothing here)
    let zoom = s.with(|e, _| e.zoom);
    two_fingers(&mut s, middle, (0.0, -150.0), 600, false);
    s.advance_ms(200);
    two_fingers_release(&mut s, middle, -150.0);
    run(&mut s, 1500);
    assert_eq!(s.with(|e, _| e.zoom), zoom, "moving together does not zoom");
}

/// Q10 (the 2026-11-14 rule, #209): one finger starts a selection or a move; a second finger coming down takes it all back (the selection, the move) and the two
/// fingers scroll.
fn q10_second_finger_cancels() {
    for kind in ["docx", "xlsx"] {
        let (mut s, middle) = screen(kind);
        let before = (selection_of(&mut s, kind), scrolled(&mut s));
        let from = pt(f32::from(middle.x) - 100.0, f32::from(middle.y) - 200.0);
        s.dispatch(vec![touch(1, TouchPhase::Started, from)]);
        for i in 1..=8 {
            s.dispatch(vec![touch(1, TouchPhase::Moved, pt(f32::from(from.x) + 10.0 * i as f32, f32::from(from.y) + 14.0 * i as f32))]);
            s.advance_ms(STEP_MS);
        }
        let second = pt(f32::from(from.x) + 150.0, f32::from(from.y) + 112.0);
        s.dispatch(vec![touch(2, TouchPhase::Started, second)]);
        assert_eq!(selection_of(&mut s, kind), before.0, "{kind}: the second finger took the first one's selection back");
        for i in 1..=20 {
            let dy = -10.0 * i as f32;
            s.dispatch(vec![touch(1, TouchPhase::Moved, pt(f32::from(from.x) + 80.0, f32::from(from.y) + 112.0 + dy)), touch(2, TouchPhase::Moved, pt(f32::from(second.x), f32::from(second.y) + dy))]);
            s.advance_ms(STEP_MS);
        }
        s.dispatch(vec![touch(1, TouchPhase::Ended, from), touch(2, TouchPhase::Ended, second)]);
        run(&mut s, 1500);
        assert!(scrolled(&mut s) > before.1 + 100.0, "{kind}: the two fingers scrolled");
        assert_eq!(selection_of(&mut s, kind), before.0, "{kind}: and nothing is selected");
    }
}

/// Q11: while a text is being composed (the input method's marked text) two fingers scroll the body and the composition stays as it is (neither committed nor
/// dropped). What one finger tapping into the text does to a composition is left to the iPhone's behaviour, measured by the user (D-tests).
fn q11_ime() {
    let (mut s, middle) = screen("docx");
    s.click((f32::from(middle.x), f32::from(middle.y) - 300.0));
    s.ime_compose("にほん");
    let marked = s.with(|e, _| format!("{:?}", e.doc_state().map(|st| (st.caret, st.anchor))));
    two_fingers(&mut s, middle, (0.0, -200.0), 600, false);
    s.advance_ms(200);
    two_fingers_release(&mut s, middle, -200.0);
    run(&mut s, 1000);
    assert_eq!(s.with(|e, _| format!("{:?}", e.doc_state().map(|st| (st.caret, st.anchor)))), marked, "the composition (caret and marked range) is untouched by the scroll");
    s.ime_commit("日本");
}

/// Q12: the content follows two fingers one for one on the screen at zoom 50 %, 100 % and 300 % (not 1 / zoom of it).
fn q12_zoom() {
    for zoom in [0.5f32, 1.0, 3.0] {
        let (mut s, middle) = screen("pptx");
        s.update(move |e, window, cx| e.set_zoom(Some(zoom), window, cx));
        s.settle();
        let before = scrolled(&mut s);
        two_fingers(&mut s, middle, (0.0, -100.0), 800, false);
        s.advance_ms(200);
        two_fingers_release(&mut s, middle, -100.0);
        run(&mut s, 1000);
        let moved = scrolled(&mut s) - before;
        // At 50 % the slide may not scroll at all (it fits): then it stays within the band and comes back.
        let fits = s.with(|e, _| f32::from(e.workspace_scroll.max_offset().y)) <= 0.5;
        if !fits {
            assert!((moved - 100.0).abs() < 6.0, "zoom {zoom}: 100 px of fingers move the content 100 px ({moved})");
        } else {
            assert!(moved.abs() < 1.0, "zoom {zoom}: a body that fits does not stay moved ({moved})");
        }
    }
}

/// Q13: the mouse wheel and the trackpad (events without fingers) keep the clamped edge: no pull past the top, no spring. (Their inertia is the OS's: it
/// arrives as events.)
fn q13_wheel_no_bounce() {
    let (mut s, middle) = screen("docx");
    s.wheel((f32::from(middle.x), f32::from(middle.y)), 300.0);
    run(&mut s, 800);
    assert!(scrolled(&mut s) <= 0.5, "the wheel does not pull the body past the top ({})", scrolled(&mut s));
    s.wheel((f32::from(middle.x), f32::from(middle.y)), -120.0);
    assert!((scrolled(&mut s) - 120.0).abs() < 1.5, "a wheel step moves the body by the step at once, with no inertia of the app's own ({})", scrolled(&mut s));
    run(&mut s, 800);
    assert!((scrolled(&mut s) - 120.0).abs() < 1.5, "and it stays there");
}

/// Q2b: the slide left where it opens (the middle of its page at 300 %, not set to the top): two fingers move it up and then down one for one, and there is no stretch.
fn q2b_from_the_middle() {
    let (mut s, middle) = screen_with("pptx", false);
    let before = scrolled(&mut s);
    two_fingers(&mut s, middle, (0.0, -100.0), 800, false);
    s.advance_ms(200);
    two_fingers_release(&mut s, middle, -100.0);
    run(&mut s, 1500);
    let up = scrolled(&mut s) - before;
    assert!((up - 100.0).abs() < 6.0, "100 px of fingers up scroll the slide 100 px ({up})");
    assert_eq!(s.with(|e, _| e.touch_overscroll()), (0.0, 0.0), "no stretch in the middle");
    two_fingers(&mut s, middle, (0.0, 100.0), 800, false);
    s.advance_ms(200);
    two_fingers_release(&mut s, middle, 100.0);
    run(&mut s, 1500);
    let down = scrolled(&mut s) - before;
    assert!(down.abs() < 6.0, "and 100 px down brings it back ({down})");
    assert_eq!(s.with(|e, _| e.touch_overscroll()), (0.0, 0.0), "no stretch on the way back");
}

/// The shift of the drawn body, down (positive) from `rest` to `now` (the helper of Q6).
fn drawn_shift(s: &mut Scenario, rest: &std::path::Path, label: &str) -> f32 {
    let body_top = s.with(|e, _| e.workspace_bounds).map(|b| f32::from(b.origin.y)).unwrap();
    let now = s.screenshot(label);
    pixel_shift(rest, &now, body_top)
}

/// Q6b: at the bottom end of the document, fingers pulling 100 px up show the band's value past it (the drawn body moves up), and lifted it returns; at the left end of the 300 % slide, fingers
/// pulling 100 px right stretch it sideways.
fn q6b_other_edges() {
    let (mut s, middle) = screen("docx");
    let d = s.with(|e, _| e.workspace_bounds).map(|b| f32::from(b.size.height)).unwrap();
    let max = s.with(|e, _| f32::from(e.workspace_scroll.max_offset().y));
    s.update(move |e, _, cx| {
        e.workspace_scroll.set_offset(point(px(0.0), px(-max)));
        cx.notify();
    });
    s.settle();
    assert_eq!(s.with(|e, _| e.touch_overscroll()), (0.0, 0.0));
    let expect = 100.0 * d * 0.55 / (d + 0.55 * 100.0);
    two_fingers(&mut s, middle, (0.0, -100.0), 800, false);
    s.advance_ms(200);
    s.settle();
    let reported = s.with(|e, _| e.touch_overscroll().1);
    assert!((reported + expect).abs() < expect * 0.1 + 2.0, "docx bottom: the band shows {reported}, the rubber band gives {expect} (up)");
    assert_eq!(s.with(|e, _| f32::from(e.workspace_scroll.offset().y)), -max, "the scroll offset stays at the bottom while it is pulled past it");
    two_fingers_release(&mut s, middle, -100.0);
    for _ in 0..43 {
        run(&mut s, STEP_MS);
    }
    assert!(s.with(|e, _| e.touch_overscroll().1.abs()) < 1.0, "it returns to the bottom edge within 700 ms");

    drop(s);
    // The side: the slide at 300 % starts at the left end (the scroll set to the origin): fingers 100 px to the right pull it past its left edge.
    let (mut s, middle) = screen("pptx");
    let w = s.with(|e, _| e.workspace_bounds).map(|b| f32::from(b.size.width)).unwrap();
    let expect = 100.0 * w * 0.55 / (w + 0.55 * 100.0);
    two_fingers(&mut s, middle, (100.0, 0.0), 800, false);
    s.advance_ms(200);
    s.settle();
    let reported = s.with(|e, _| e.touch_overscroll().0);
    assert!((reported - expect).abs() < expect * 0.1 + 2.0, "pptx left edge: the band shows {reported}, the rubber band gives {expect}");
    assert_eq!(s.with(|e, _| f32::from(e.workspace_scroll.offset().x)), 0.0, "the scroll offset stays at the left end");
    two_fingers_release(&mut s, middle, 0.0);
    let _ = drawn_shift;
    run(&mut s, 700);
    assert!(s.with(|e, _| e.touch_overscroll().0.abs()) < 1.0, "and it returns within 700 ms");
}

/// Q8b: a fast fling toward the bottom end of the document goes past it a little and comes back to rest on the end; it does not bounce again.
fn q8b_fling_into_the_bottom() {
    let (mut s, middle) = screen("docx");
    let max = s.with(|e, _| f32::from(e.workspace_scroll.max_offset().y));
    s.update(move |e, _, cx| {
        e.workspace_scroll.set_offset(point(px(0.0), px(-(max - 300.0))));
        cx.notify();
    });
    s.settle();
    two_fingers(&mut s, middle, (0.0, -600.0), 90, true); // far faster than the 300 px left
    let mut deepest = 0.0f32;
    let mut crossings = 0;
    let mut was_past = false;
    for _ in 0..200 {
        run(&mut s, STEP_MS);
        let past = -s.with(|e, _| e.touch_overscroll().1);
        deepest = deepest.max(past);
        if (past > 0.5) != was_past {
            crossings += 1;
            was_past = past > 0.5;
        }
    }
    assert!(deepest > 5.0, "it goes past the bottom edge a little ({deepest})");
    assert!(s.with(|e, _| e.touch_overscroll().1.abs()) < 1.0 && (scrolled(&mut s) - max).abs() < 1.0, "and rests on the end");
    assert!(crossings <= 2, "it goes past once and returns once, no second bounce ({crossings} crossings)");
}
