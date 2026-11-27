// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0082 (the slide show's button turns round when the toolbar does not fit) and K0083 (the slide list is left out when the content beside it
//! would be narrower than twice the list), both decided from the window's width every frame (`narrow_layout.rs`). Test design:
//! `tasks/doc/test-design/K0082-K0083.md` (T1 to T12).
use crate::shell::narrow_layout::{SLIDESHOW_GAP, SLIDESHOW_ROUND_WIDTH, slide_list_fits, slideshow_is_round};
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::k0083_t1_to_t3_the_deciding_functions_switch_exactly_at_their_edge", deciding_functions),
        ("pptx::k0082_the_slide_show_button_is_round_when_the_row_is_short", round_button),
        ("pptx::k0083_the_slide_list_goes_when_the_content_would_be_under_twice_the_list", slide_list_goes),
        ("pptx::k0083_the_right_pane_and_the_master_view", right_pane_and_master),
        ("pptx::k0083_the_master_view_keeps_its_list", master_view_keeps_the_list),
    ]
}

fn presentation(width: f32) -> Scenario {
    let mut s = Scenario::new_document(ibis_formats::Kind::Presentation);
    s.size(width, 800.);
    s.settle();
    s
}

fn deciding_functions() {
    // T1
    assert!(slide_list_fits(3. * 200. + 4., 200., 4.));
    assert!(!slide_list_fits(3. * 200. + 3., 200., 4.));
    // T2: the toolbar is in the row.
    let (pill, toolbar, name) = (120., 500., 96.);
    assert!(!slideshow_is_round(pill + SLIDESHOW_GAP + toolbar, pill, Some(toolbar), name));
    assert!(slideshow_is_round(pill + SLIDESHOW_GAP + toolbar - 1., pill, Some(toolbar), name));
    // T3: the toolbar is in its own row.
    assert!(!slideshow_is_round(pill + SLIDESHOW_GAP + name, pill, None, name));
    assert!(slideshow_is_round(pill + SLIDESHOW_GAP + name - 1., pill, None, name));
}

/// The widths from `from` down to `to` in steps of 10.
fn widths(from: i32, to: i32) -> Vec<f32> {
    (to / 10..=from / 10).rev().map(|w| (w * 10) as f32).collect()
}

fn round_button() {
    let mut s = presentation(1400.);
    // T4: wide, the pill.
    assert!(!s.with(|e, _| e.slideshow_pill_width.get() == 0.0), "the pill was drawn with a width");
    assert!(!s.update(|e, w, _| e.slideshow_round(w)), "1400: the pill");
    // T5: narrowing: the pill, then round, never back and forth.
    let mut states = Vec::new();
    for w in widths(1400, 300) {
        s.size(w, 800.);
        s.settle();
        states.push((w, s.update(|e, w, _| e.slideshow_round(w))));
    }
    let first_round = states.iter().position(|(_, r)| *r).expect("a width where the button is round");
    assert!(states[..first_round].iter().all(|(_, r)| !r) && states[first_round..].iter().all(|(_, r)| *r), "pill above one width, round below: {states:?}");
    assert!(first_round > 0, "1400 is the pill");
    eprintln!("K0082: round from {} (pill above)", states[first_round].0);
    // T6: widened again: the pill comes back (nothing is kept between frames).
    s.size(1400., 800.);
    s.settle();
    assert!(!s.update(|e, w, _| e.slideshow_round(w)));
    s.screenshot("k0082-wide");
    // The round button starts the show when pressed: scan the title row's y for it.
    s.size(first_round_width(&states), 800.);
    s.settle();
    s.screenshot("k0082-round");
    let mut started = false;
    let _ = SLIDESHOW_ROUND_WIDTH;
    for x in (40..260).step_by(4) {
        s.click((x as f32, 18.));
        s.settle();
        if s.with(|e, _| e.pptx().is_some_and(|p| p.slideshow.is_some())) {
            started = true;
            break;
        }
    }
    assert!(started, "a press on the round button starts the slide show");
}

fn first_round_width(states: &[(f32, bool)]) -> f32 {
    states.iter().find(|(_, r)| *r).unwrap().0
}

fn shown(s: &mut Scenario) -> bool {
    s.with(|e, _| e.slide_list_shown())
}

fn slide_list_goes() {
    let mut s = presentation(1400.);
    // T4
    assert!(shown(&mut s), "1400: the slide list is there");
    let pane = s.with(|e, _| e.pane_width_now());
    let left_wide = s.with(|e, _| f32::from(e.workspace_bounds.unwrap().left()));
    assert!(left_wide >= pane, "the slide is right of the list ({left_wide} {pane})");
    s.screenshot("k0083-wide");
    // T7: the width where it goes is body = 3 panes + the divider (no right pane).
    let edge: f32 = 3. * pane + 4.;
    for (w, expect) in [(edge.ceil(), true), (edge.ceil() - 1., false)] {
        s.size(w, 800.);
        s.settle();
        assert_eq!(shown(&mut s), expect, "{w}: the slide list's presence (edge {edge})");
    }
    s.size(edge.ceil() - 1., 800.);
    s.settle();
    // T11: gone, the slide has the whole width.
    let left = s.with(|e, _| f32::from(e.workspace_bounds.unwrap().left()));
    assert!(left < pane / 2., "no list: the slide's area starts at the window's left ({left})");
    s.screenshot("k0083-narrow");
    // T8: widened, it comes back.
    s.size(1400., 800.);
    s.settle();
    assert!(shown(&mut s));
    // T8: a list dragged to 120 goes at a narrower window.
    s.update(|e, _, _| e.pptx_mut().unwrap().pane_width = Some(120.));
    let edge: f32 = 3. * 120. + 4.;
    for (w, expect) in [(edge.ceil(), true), (edge.ceil() - 1., false)] {
        s.size(w, 800.);
        s.settle();
        assert_eq!(shown(&mut s), expect, "list 120, {w}");
    }
}

fn right_pane_and_master() {
    let mut s = presentation(1400.);
    let pane = s.with(|e, _| e.pane_width_now());
    // T9: the right pane takes its width off the content.
    s.update(|e, _, cx| {
        e.sidebar_open = true;
        cx.notify();
    });
    s.settle();
    let right = s.with(|e, _| e.body_width);
    let sidebar = s.with(|e, _| e.sidebar_width.max(crate::shell::chrome::SIDEBAR_WIDTH));
    if s.with(|e, _| e.right_pane.shown) {
        assert!((1400. - right - sidebar).abs() < 1., "body {right} = window less the right pane {sidebar}");
        let edge: f32 = (3. * pane + 4. + sidebar).ceil();
        for (w, expect) in [(edge, true), (edge - 1., false)] {
            s.size(w, 800.);
            s.settle();
            assert_eq!(shown(&mut s), expect, "right pane open, {w} (edge {edge})");
        }
    }
}

/// T10: the master view keeps its list of masters at a width where the slide list is left out (a file with a source, a new one has no master to open).
fn master_view_keeps_the_list() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(400., 800.);
    s.settle();
    assert!(!shown(&mut s), "400: the slide list is left out");
    s.update(|e, w, cx| e.open_master_view(w, cx));
    s.settle();
    assert!(s.with(|e, _| e.is_master_view()), "the master view is open");
    assert!(shown(&mut s), "the master view keeps its list of masters");
}
