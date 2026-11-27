// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #300: the slide sorter grid builds the shapes of the cards in view only (and of the current slide). What is in view is worked out every frame
//! from the scroll, the window and the slides, so each change (adding, deleting, moving a slide, scrolling, the window's size) is followed.

use crate::grid_view::DRAWN_CARDS;
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("pptx::issue300_the_grid_draws_the_cards_in_view_and_follows_changes", grid_follows), ("pptx::issue300_grid_timing", timing)]
}

fn drawn(s: &mut Scenario) -> Vec<usize> {
    s.settle();
    let mut v = DRAWN_CARDS.with(|d| d.borrow().clone());
    v.sort_unstable();
    v.dedup();
    v
}

fn deck(slides: usize, width: f32, height: f32) -> Scenario {
    let mut s = Scenario::new_document(ibis_formats::Kind::Presentation);
    s.size(width, height);
    for _ in 1..slides {
        s.update(|e, window, cx| e.duplicate_current_slide(window, cx));
    }
    s.update(|e, window, cx| e.toggle_grid_view(window, cx));
    s.settle();
    s
}

fn count(s: &mut Scenario) -> usize {
    s.with(|e, _| e.presentation().slides.len())
}

fn grid_follows() {
    let mut s = deck(60, 1200., 800.);
    assert_eq!(count(&mut s), 60);
    let current = s.with(|e, _| e.presentation().slide_index(e.current_slide).unwrap());
    let top = drawn(&mut s);
    assert!(top.contains(&0), "the first row is drawn: {top:?}");
    assert!(top.len() < 60, "cards out of view are left out: {} of 60", top.len());
    assert!(top.contains(&current), "the current slide is always drawn");
    assert!(!top.contains(&40), "a card far below is not drawn: {top:?}");
    // The window's size: a wider window holds more cards in a row, so more in view; a narrower one fewer.
    s.size(2400., 800.);
    let wide = drawn(&mut s);
    s.size(500., 800.);
    let narrow = drawn(&mut s);
    assert!(wide.len() > narrow.len(), "wide {} narrow {}", wide.len(), narrow.len());
    s.size(1200., 800.);
    assert_eq!(drawn(&mut s), top, "the same window draws the same cards again");
    // Scrolling to the end: the last cards are drawn, the first are not.
    let at = (600., 400.);
    for _ in 0..40 {
        s.wheel(at, -2000.);
    }
    let end = drawn(&mut s);
    assert!(end.contains(&59), "the last card is drawn after scrolling down: {end:?}");
    assert!(!end.contains(&1), "the first cards are left out when scrolled away: {end:?}");
    // Adding a slide (the current one is the first or last: it is drawn wherever the scroll is) and deleting it.
    s.update(|e, window, cx| e.duplicate_current_slide(window, cx));
    assert_eq!(count(&mut s), 61);
    let added = drawn(&mut s);
    assert!(added.contains(&60) || added.contains(&1), "the added slide is drawn (it is the current one): {added:?}");
    assert!(added.contains(&60), "the new last card is in view at the end: {added:?}");
    s.update(|e, window, cx| e.delete_slide(window, cx));
    assert_eq!(count(&mut s), 60);
    let removed = drawn(&mut s);
    assert!(removed.iter().all(|&i| i < 60), "no card past the end: {removed:?}");
    assert!(removed.contains(&59), "the last card is still drawn after the delete: {removed:?}");
    // Moving the current slide to the end: it is drawn (current), and the others in view stay.
    s.update(|e, window, cx| e.move_current_slide(1000, cx));
    let moved = drawn(&mut s);
    let index = s.with(|e, _| e.presentation().slide_index(e.current_slide).unwrap());
    assert_eq!(index, 59);
    assert!(moved.contains(&59));
    // Scrolling back up.
    for _ in 0..40 {
        s.wheel(at, 2000.);
    }
    let back = drawn(&mut s);
    assert!(back.contains(&0) && !back.contains(&58), "back at the top: {back:?}");
}

/// The time of settling the grid of 60 slides: with the cards in view only (it prints; it fails on nothing).
fn timing() {
    let mut s = deck(60, 1200., 800.);
    let median = |mut v: Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    let ms = median(
        (0..15)
            .map(|_| {
                let t = crate::clock::Instant::now();
                s.update(|_, _, cx| cx.notify());
                s.settle();
                t.elapsed().as_secs_f64() * 1000.0
            })
            .collect(),
    );
    eprintln!("ISSUE300-TIMING pptx_grid_60_slides_settle={ms:.2}ms");
}
