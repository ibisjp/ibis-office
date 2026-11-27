// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0085: the sheet tabs do not flicker. On an iPhone (402 pt wide) a new workbook with the right pane open flickered fiercely around the tabs: the +
//! seemed to change sides every frame (design: `tasks/doc/test-design/K0085.md`; the numbers TF1... are that document's). `render_sheet_tabs` decides
//! where the + goes (inline after the last tab, or fixed at the strip's right) from the strip's *last frame's* bounds, which the decision itself changes
//! (the fixed + and its divider take 29 points from the strip): where the strip is left with no width the decision has no ground and swings.
//! The tests draw frame by frame (one frame at a time, the clock moving 16 ms) and read where the + and the strip are drawn.
use crate::shell::scenario::{Scenario, button_probe as buttons};
use crate::ext::*;
use ibis_formats::Kind;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::k0085_tf4_a_wide_window_is_steady_with_few_tabs_and_with_many", tf4_wide_steady),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::k0085_tf1_an_iphone_width_with_the_right_pane_open_draws_the_same_every_frame", tf1_iphone),
        ("xlsx::k0085_tf2_every_width_draws_the_same_every_frame", tf2_sweep),
        ("xlsx::k0085_tf3_across_the_boundary_the_plus_changes_side_once_and_is_always_reachable", tf3_boundary),
        ("xlsx::k0085_tf5_adding_a_sheet_in_a_narrow_window_settles_and_stays", tf5_add_in_narrow),
    ]
}

/// What a frame drew: where the + is (None: not drawn), the strip's width, and whether the + is outside the strip (fixed) or inside it (inline).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Frame {
    plus: Option<(f32, f32)>,
    strip_width: f32,
    fixed: Option<bool>,
}

/// One frame is drawn (the clock moves 16 ms) and read.
fn frame(s: &mut Scenario) -> Frame {
    buttons::clear();
    s.update(|_, _, cx| cx.notify());
    s.advance_ms(16);
    let strip = s.with(|e, _| e.wb().tab_scroll.bounds());
    let plus = buttons::bounds("sheet-add");
    let r = |x: f32| (x * 2.0).round() / 2.0;
    Frame {
        plus: plus.map(|b| (r(f32::from(b.left())), r(f32::from(b.right())))),
        strip_width: r(f32::from(strip.size.width)),
        fixed: plus.map(|b| f32::from(b.left()) >= f32::from(strip.right()) - 0.5),
    }
}

/// A workbook of `sheets` sheets in a window `width` wide with the right pane open (`pane`), a finger on it (`touch`); settled.
fn book(sheets: usize, width: f32, pane: bool, touch: bool) -> Scenario {
    let mut s = Scenario::new_document(Kind::Workbook);
    s.size(width, 874.);
    s.update(move |e, w, cx| {
        e.touch_seen = touch;
        while e.presentation().slides.len() < sheets {
            e.insert_sheet(w, cx);
        }
        cx.notify();
    });
    s.settle();
    if pane {
        s.update(|e, window, cx| e.open_sheet_find(window, cx));
        s.settle();
    }
    s
}

/// `n` frames in a row.
fn frames(s: &mut Scenario, n: usize) -> Vec<Frame> {
    (0..n).map(|_| frame(s)).collect()
}

fn steady(series: &[Frame]) -> bool {
    series.windows(2).all(|w| w[0] == w[1])
}

/// TF1 (the request): an iPhone's width (402 pt, 874 pt high), a new workbook (1 sheet), the right pane open (it keeps its 300 pt, which leaves the
/// strip almost nothing): 12 frames in a row draw the same: the + on the same side at the same place, the strip the same width. Closed again, the
/// same 12 frames of the first layout. Also with 3 sheets, and with a finger and without.
fn tf1_iphone() {
    let _probe = buttons::Probe::new();
    for sheets in [1, 3] {
        for touch in [true, false] {
            let mut s = book(sheets, 402.0, true, touch);
            let series = frames(&mut s, 12);
            assert!(steady(&series), "{sheets} sheet(s), touch {touch}: the frames differ: {series:?}");
            s.update(|e, _, cx| {
                e.sidebar_open = false;
                cx.notify();
            });
            s.settle();
            let closed = frames(&mut s, 12);
            assert!(steady(&closed), "{sheets} sheet(s), touch {touch}, pane closed: the frames differ: {closed:?}");
        }
    }
}

/// TF2: every width from 200 to 700 points in steps of 7, with the pane open and without, 1, 3 and 8 sheets: 8 frames in a row are the same.
fn tf2_sweep() {
    let _probe = buttons::Probe::new();
    let mut bad: Vec<String> = Vec::new();
    for sheets in [1, 3, 8] {
        for pane in [true, false] {
            let mut s = book(sheets, 200.0, pane, true);
            let mut width = 200.0;
            while width <= 700.0 {
                s.size(width, 874.);
                s.settle();
                let series = frames(&mut s, 8);
                if !steady(&series) {
                    bad.push(format!("{sheets} sheets, pane {pane}, {width}: {:?}", &series[..3]));
                }
                width += 7.0;
            }
        }
    }
    assert!(bad.is_empty(), "{} widths draw differently from frame to frame:\n{}", bad.len(), bad.iter().take(8).cloned().collect::<Vec<_>>().join("\n"));
}

/// TF3: a workbook of 5 sheets, the window widened from 250 to 700 in steps of 1 and narrowed back (each width settled and read in 4 frames): the + changes
/// between inline and fixed at most once on the way up and once on the way down (it does not swing near the boundary: the two directions may change
/// at different widths, as the rule of a boundary with hysteresis does), and it is always drawn and its place is inside the window.
fn tf3_boundary() {
    let _probe = buttons::Probe::new();
    let mut s = book(5, 250.0, false, true);
    for (label, widths) in [("up", (250..=700).collect::<Vec<i32>>()), ("down", (250..=700).rev().collect::<Vec<i32>>())] {
        let mut flips = 0;
        let mut last: Option<bool> = None;
        for width in widths {
            s.size(width as f32, 874.);
            s.settle();
            let series = frames(&mut s, 4);
            assert!(steady(&series), "{label} {width}: the frames differ: {series:?}");
            let f = series[0];
            let (left, right) = f.plus.unwrap_or_else(|| panic!("{label} {width}: the + is not drawn"));
            assert!(left >= 0.0 && right <= width as f32 + 0.5, "{label} {width}: the + is inside the window ({left}..{right})");
            if let (Some(before), Some(now)) = (last, f.fixed) {
                if before != now {
                    flips += 1;
                }
            }
            last = f.fixed;
        }
        assert!(flips <= 1, "{label}: the + changed side {flips} times");
    }
}

/// TF4 (unchanged; passes now): a wide window (1200) is the same every frame with 3 sheets (the + inline) and with 30 (the tabs overflow: the + fixed).
fn tf4_wide_steady() {
    let _probe = buttons::Probe::new();
    for (sheets, fixed) in [(3, false), (30, true)] {
        let mut s = book(sheets, 1200.0, false, false);
        let series = frames(&mut s, 10);
        assert!(steady(&series), "{sheets} sheets: the frames differ: {series:?}");
        assert_eq!(series[0].fixed, Some(fixed), "{sheets} sheets: the + is {}", if fixed { "fixed" } else { "inline" });
    }
}

/// TF5: pressing + in a narrow window (402 with the pane, then 402 without) adds a sheet, the new tab is revealed, and the 12 frames after are the same.
fn tf5_add_in_narrow() {
    let _probe = buttons::Probe::new();
    for pane in [true, false] {
        let mut s = book(1, 402.0, pane, true);
        let _ = frame(&mut s);
        let plus = buttons::bounds("sheet-add").expect("setup: the + is drawn");
        s.click((f32::from(plus.center().x), f32::from(plus.center().y)));
        s.settle();
        assert_eq!(s.with(|e, _| e.presentation().slides.len()), 2, "setup: a sheet was added");
        let series = frames(&mut s, 12);
        assert!(steady(&series), "pane {pane}: after adding a sheet the frames differ: {series:?}");
    }
}
