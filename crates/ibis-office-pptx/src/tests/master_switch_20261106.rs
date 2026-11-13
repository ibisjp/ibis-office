// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-06: clicking a page in the slide master view's list shows it fast. Times (the click to the drawn window) are
//! printed when `IBIS_DEBUG_TIMING` is set. The user's non-public file is read from `IBIS_PRIVATE_PPTX` when it is set and
//! exists; without it that scenario passes. Pictures are in `target/scenario/<name>/`.
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::master_switch_20261106_plan", || switch_times("test/editing/pptx-plan.pptx", "plan")),
        ("pptx::master_switch_20261106_private", private),
        ("pptx::master_switch_20261106_unedited_pages_keep_their_pictures", pictures_kept),
    ]
}

fn private() {
    let Some(path) = std::env::var("IBIS_PRIVATE_PPTX").ok().filter(|p| std::path::Path::new(p).exists()) else { return };
    switch_times(&path, "private");
}

/// The time of clicking each page in turn (the click, the drawing after it).
fn switch_times(path: &str, label: &str) {
    let mut s = Scenario::open(path);
    s.menu("pptx.master-view");
    s.settle();
    let pages = s.with(|e, _| e.master_view_page_count());
    let mut total = std::time::Duration::ZERO;
    let mut worst = std::time::Duration::ZERO;
    for round in 0..2 {
        for i in (0..pages).rev() {
            let start = crate::clock::Instant::now();
            s.update(|e, w, cx| e.switch_master_page(i, w, cx));
            s.settle();
            let took = start.elapsed();
            if round == 1 {
                total += took;
                worst = worst.max(took);
            }
        }
    }
    if std::env::var_os("IBIS_DEBUG_TIMING").is_some() {
        eprintln!("master switch {label}: {pages} pages, mean {:?}, worst {:?}", total / pages.max(1) as u32, worst);
    }
    s.screenshot(&format!("master-switch-{label}"));
}

/// Choosing a page without editing it keeps the pictures of the other pages (they are not made again).
fn pictures_kept() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("pptx.master-view");
    s.settle();
    let made = s.with(|e, _| e.master_view_thumb_count());
    assert!(made > 0, "the list's pictures are made");
    s.update(|e, w, cx| e.switch_master_page(1, w, cx));
    s.settle();
    let first = s.with(|e, _| e.master_view_thumb_ids());
    s.update(|e, w, cx| e.switch_master_page(2, w, cx));
    s.settle();
    let second = s.with(|e, _| e.master_view_thumb_ids());
    for (i, (a, b)) in first.iter().zip(&second).enumerate() {
        if i != 1 && i != 2 {
            assert_eq!(a, b, "the picture of page {i} is the same one (not made again)");
        }
    }
}
