// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0080: in a narrow window of a touch device the status bar has no zoom controls (the slider, the − and + at its sides, the
//! percentage and its ▲ arrow), and the sheet tabs' scroll bar (a workbook's only) reaches right to where the zoom controls were, its left end
//! where it was. The judgement is K0061's/K0077's `narrow_layout::bars_at_bottom`: under 720 px **and** on a touch device. A wide window, and a
//! narrow one with a mouse only, are as they were (design: `tasks/doc/test-design/K0080.md`; the numbers NS1... are that document's).
use crate::shell::scenario::{Scenario, button_probe as buttons};
use ibis_formats::Kind;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0080_ns2_a_wide_window_or_a_mouse_keeps_the_zoom_controls", ns2_unchanged),
        ("shell::k0080_ns3_the_zoom_still_works_without_the_controls", ns3_zoom_still_works),
        ("shell::k0080_ns1_a_narrow_touch_window_has_no_zoom_controls_on_the_three_screens", ns1_hidden),
        ("shell::k0080_ns4_the_controls_go_and_come_back_with_the_window_and_the_finger", ns4_follows_the_window),
        ("shell::k0080_ns5_the_sheet_tabs_scroll_bar_reaches_to_where_the_zoom_was", ns5_scrollbar_extends),
        ("shell::k0080_ns6_the_scroll_bar_is_unchanged_in_a_wide_window_and_with_a_mouse", ns6_scrollbar_unchanged),
    ]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}


const KINDS: [Kind; 3] = [Kind::Document, Kind::Workbook, Kind::Presentation];
/// The probes of the zoom controls' parts and of the sheet tabs' scroll bar (`status_bar.rs`).
const ZOOM_PARTS: [&str; 3] = ["status-zoom-slider", "status-zoom-out", "status-zoom-in"];

/// Whether the icon that fits the page to the window is drawn (`zoom-fit`; the document and the presentation have it, a workbook has none).
fn has_fit(s: &mut Scenario) -> bool {
    buttons::clear();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    buttons::bounds("zoom-fit").is_some()
}

fn screen(kind: Kind, width: f32, touch: bool) -> Scenario {
    let mut s = Scenario::new_document(kind);
    s.size(width, 700.);
    s.update(move |e, _, cx| {
        e.touch_seen = touch;
        cx.notify();
    });
    s.settle();
    s
}

/// The parts of the zoom that are drawn now (probe ids), and whether the percentage's button is (its bounds are set when it is painted).
fn drawn_zoom_parts(s: &mut Scenario) -> Vec<String> {
    buttons::clear();
    s.update(|e, _, cx| {
        e.zoom_trigger_bounds.set(None);
        cx.notify();
    });
    s.settle();
    let mut out: Vec<String> = ZOOM_PARTS.iter().filter(|id| buttons::bounds(id).is_some()).map(|id| id.to_string()).collect();
    // The icon that fits the page is not one of the parts that go (the decision: it stays); `has_fit` says it separately.
    if s.with(|e, _| e.zoom_trigger_bounds.get().is_some()) {
        out.push("percentage".to_string());
    }
    out
}

/// NS1 (the request): under 720 px on a touch device (719 and 400) the slider, the − and + and the percentage's button (with its ▲) are not drawn, on
/// the document, the workbook and the presentation. The status bar still shows its page and hint.
fn ns1_hidden() {
    let _probe = buttons::Probe::new();
    for kind in KINDS {
        for width in [719.0, 400.0] {
            let mut s = screen(kind, width, true);
            let parts = drawn_zoom_parts(&mut s);
            assert!(parts.is_empty(), "{kind:?} at {width}: the slider, − and + and the percentage's button are not drawn: {parts:?}");
            // The decision (the user): the fit icon stays in a narrow window of a touch device; a workbook has none.
            assert_eq!(has_fit(&mut s), kind != Kind::Workbook, "{kind:?} at {width}: the fit icon is drawn (not on a workbook)");
            assert!(!s.status_texts().is_empty(), "{kind:?} at {width}: the status bar still has its texts");
        }
    }
}

/// NS2 (unchanged; passes now): at 720 px and wider with a finger, and under 720 px with a mouse only (K0077), the controls are all drawn.
fn ns2_unchanged() {
    let _probe = buttons::Probe::new();
    for kind in KINDS {
        for (width, touch) in [(720.0, true), (1200.0, true), (719.0, false), (400.0, false), (1200.0, false)] {
            let mut s = screen(kind, width, touch);
            let parts = drawn_zoom_parts(&mut s);
            for wanted in ZOOM_PARTS.iter().map(|s| s.to_string()).chain(["percentage".to_string()]) {
                assert!(parts.contains(&wanted), "{kind:?} at {width} (touch {touch}): {wanted} is drawn: {parts:?}");
            }
            assert_eq!(has_fit(&mut s), kind != Kind::Workbook, "{kind:?} at {width} (touch {touch}): the fit icon is drawn (not on a workbook)");
        }
    }
}

/// NS3 (passes now): with the controls gone (NS1's windows) the zoom is still in the View menu: the zoom-in line zooms, and the status bar's zoom
/// is not what keeps it. Checked where the controls are drawn too (the same lines), so that hiding them takes nothing else away.
fn ns3_zoom_still_works() {
    for kind in KINDS {
        for (width, touch) in [(719.0, true), (1200.0, true)] {
            let mut s = screen(kind, width, touch);
            let before = s.with(|e, _| e.current_zoom());
            s.menu("view.zoom-in");
            s.settle();
            assert!(s.with(|e, _| e.current_zoom()) > before, "{kind:?} at {width}: View > zoom in zooms");
        }
    }
}

/// NS4: the controls follow the window and the finger: a touch window 1200 → 700 → 1200 shows, hides, shows; a mouse-only window 700 keeps them,
/// and the first touch on it (`touch_seen`) takes them away.
fn ns4_follows_the_window() {
    let _probe = buttons::Probe::new();
    let mut s = screen(Kind::Document, 1200.0, true);
    assert!(drawn_zoom_parts(&mut s).contains(&"status-zoom-slider".to_string()), "1200: shown");
    s.size(700.0, 700.0);
    s.settle();
    assert!(drawn_zoom_parts(&mut s).is_empty(), "700 with a finger: hidden");
    s.size(1200.0, 700.0);
    s.settle();
    assert!(drawn_zoom_parts(&mut s).contains(&"status-zoom-slider".to_string()), "back at 1200: shown again");
    drop(s); // two Scenarios cannot be alive together
    let mut s = screen(Kind::Document, 700.0, false);
    assert!(drawn_zoom_parts(&mut s).contains(&"status-zoom-slider".to_string()), "700 with a mouse: shown");
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    assert!(drawn_zoom_parts(&mut s).is_empty(), "the first touch: hidden");
}

/// A workbook of many sheets (the tabs overflow), `width` wide, with a finger (`touch`) or a mouse only. Returns it with the tabs' scroll bar
/// (left, right) and, if the zoom is drawn, the zoom controls' width.
fn workbook_bar(width: f32, touch: bool) -> (f32, f32, Option<f32>) {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(width, 700.);
    s.update(move |e, _, cx| {
        e.touch_seen = touch;
        cx.notify();
    });
    s.settle();
    for _ in 0..30 {
        s.menu("xlsx.insert-sheet");
    }
    buttons::clear();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let bar = buttons::bounds("sheet-tabs-strip").unwrap_or_else(|| panic!("the sheet tabs' scroll bar is drawn (the tabs overflow): {:?}", buttons::all()));
    let zoom = buttons::bounds("status-zoom-controls").map(|b| f32::from(b.size.width));
    (f32::from(bar.left()), f32::from(bar.right()), zoom)
}

/// NS5 (the request: the scroll bar's right end): in a narrow window with a finger the bar's right end is where the zoom controls' room was (it grows
/// by their width, with the gap that was before them, within 40 px), its left end is the one of the same window with a mouse (zoom drawn), and it does
/// not go past the window's right edge.
fn ns5_scrollbar_extends() {
    let (left_mouse, right_mouse, zoom) = workbook_bar(719.0, false);
    let zoom = zoom.expect("setup: the mouse-only window draws the zoom");
    assert!(zoom > 150.0, "setup: the zoom controls are wide ({zoom})");
    let (left_touch, right_touch, none) = workbook_bar(719.0, true);
    assert!(none.is_none_or(|w| w < 1.0), "the finger's window has no zoom controls (a workbook has no fit icon to leave: the whole group goes, {none:?})");
    assert!((left_touch - left_mouse).abs() < 1.0, "the left end is where it was ({left_mouse} -> {left_touch})");
    assert!(right_touch >= right_mouse + zoom - 1.0, "the right end reaches to where the zoom was: {right_mouse} -> {right_touch} (zoom {zoom})");
    assert!(right_touch <= right_mouse + zoom + 40.0, "and not much farther ({right_touch})");
    assert!(right_touch <= 719.0, "and not past the window");
}

/// NS6 (unchanged; passes now): in a wide window (touch or not) and in a narrow one with a mouse only, the bar's two ends are the same with or without
/// the finger (the zoom is drawn in all of them).
fn ns6_scrollbar_unchanged() {
    for width in [719.0, 1200.0] {
        let (l0, r0, z0) = workbook_bar(width, false);
        assert!(z0.is_some(), "setup: the mouse's window draws the zoom at {width}");
        if width >= 720.0 {
            let (l1, r1, z1) = workbook_bar(width, true);
            assert!(z1.is_some(), "a wide window with a finger draws the zoom");
            assert!((l0 - l1).abs() < 1.0 && (r0 - r1).abs() < 1.0, "{width}: the bar is the same with a finger ({l0} {r0}) ({l1} {r1})");
        }
    }
}
