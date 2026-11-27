// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-15 (#219), as Microsoft PowerPoint's Next / Previous went in a show of 8 slides with 6 and 7 hidden (measured through COM):
//! on from a shown slide: the next shown one; on from a hidden one: the slide next to it, hidden or not; back: through the slides seen,
//! and before any seen, the shown slide before. 最初から starts with the first shown slide.
use crate::shell::scenario::Scenario;
use crate::ext::*;
use ibis_doc_model::Command;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue219_slideshow_next_and_previous_with_hidden_slides", next_and_previous),
        ("pptx::issue219_slideshow_back_goes_through_the_slides_seen", back_through_seen),
        ("pptx::issue219_slideshow_from_the_first_skips_a_hidden_first_slide", from_first),
    ]
}

/// 8 slides, the 6th and 7th hidden.
fn eight(s: &mut Scenario) -> Vec<ibis_doc_model::SlideId> {
    s.update(|e, w, cx| {
        while e.presentation().slides.len() < 8 {
            e.add_slide(w, cx);
        }
    });
    let ids: Vec<_> = s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.id).collect());
    assert_eq!(ids.len(), 8);
    for i in [5, 6] {
        s.update(|e, _, cx| { let mut sl = e.presentation().slide(ids[i]).unwrap().clone(); sl.hidden = true; e.run(vec![Command::SetSlide { slide: sl }], cx) });
    }
    ids
}

fn start_at(s: &mut Scenario, n: usize) {
    s.page(n);
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    s.settle();
}

fn at(s: &mut Scenario, ids: &[ibis_doc_model::SlideId]) -> usize {
    let cur = s.with(|e, _| e.current_slide);
    ids.iter().position(|&i| i == cur).unwrap() + 1
}

fn run(start: usize, keys: &[&str], expect: &[usize]) {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let ids = eight(&mut s);
    start_at(&mut s, start);
    assert_eq!(at(&mut s, &ids), start, "starts at {start}");
    let mut got = Vec::new();
    for k in keys {
        s.key(k);
        got.push(at(&mut s, &ids));
    }
    assert_eq!(got, expect, "from {start}: {keys:?}");
}

fn next_and_previous() {
    // From a shown slide: the hidden 6 and 7 are passed over (5 -> 8); back (no slide seen before): the shown slide before.
    run(5, &["space"], &[8]);
    run(5, &["left"], &[4]);
    // From the hidden 6: the hidden 7 next, then 8; back: the shown 5.
    run(6, &["space", "space"], &[7, 8]);
    run(6, &["left"], &[5]);
    // From the hidden 7: 8; back: 5 (the hidden 6 is passed over).
    run(7, &["space"], &[8]);
    run(7, &["left"], &[5]);
}

fn back_through_seen() {
    // 6 -> 7 -> 8, and back through the hidden slides seen: 7, 6, then (nothing seen before 6) the shown 5.
    run(6, &["space", "space", "left", "left", "left"], &[7, 8, 7, 6, 5]);
    // 5 -> 8 and back is to 5 (the slide left), not into the hidden ones.
    run(5, &["space", "left"], &[8, 5]);
}

fn from_first() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let ids = eight(&mut s);
    s.update(|e, _, cx| { let mut sl = e.presentation().slide(ids[0]).unwrap().clone(); sl.hidden = true; e.run(vec![Command::SetSlide { slide: sl }], cx) });
    s.page(3);
    s.update(|e, w, cx| {
        e.select_first_show_slide(w, cx);
        e.start_slideshow(w, cx)
    });
    s.settle();
    assert_eq!(at(&mut s, &ids), 2);
}
