// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #299: the lines of the laid-out pages are kept between frames (`flow_lines_arc`) and are made again when the document is edited.

use crate::ext::*;
use crate::shell::scenario::Scenario;
use std::sync::Arc;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("docx::issue299_flow_lines_are_kept_and_follow_edits", flow_lines_cache), ("docx::issue299_timing", timing)]
}

const REPORT: &str = "test/editing/docx-report.docx";

fn flow_lines_cache() {
    let mut s = Scenario::open(REPORT);
    s.size(1200.0, 900.0);
    s.settle();
    let first = s.with(|e, _| e.flow_lines_arc());
    assert!(!first.is_empty(), "the report has lines");
    // No edit between: the same lines (not made again), and the copy `flow_lines` hands out is equal.
    let again = s.with(|e, _| e.flow_lines_arc());
    assert!(Arc::ptr_eq(&first, &again), "an unchanged document keeps its lines");
    assert_eq!(s.with(|e, _| e.flow_lines()).len(), first.len());
    // Typing at the start: the first line grows longer, a paragraph that is full wraps; the lines are made again.
    let before_pages = first.iter().map(|l| l.page).max().unwrap();
    s.key("ctrl-home");
    let long = "あいうえおかきくけこ".repeat(20);
    s.type_text(&long);
    for _ in 0..3 {
        s.key("enter");
    }
    s.settle();
    let after = s.with(|e, _| e.flow_lines_arc());
    assert!(!Arc::ptr_eq(&first, &after), "editing the document makes its lines again");
    assert!(after.len() > first.len(), "the typed text has lines of its own: {} -> {}", first.len(), after.len());
    let after_pages = after.iter().map(|l| l.page).max().unwrap();
    assert!(after_pages >= before_pages, "pages never fewer after adding text");
    // The lines now are what a fresh layout gives: a second read is the cached one, and it is the new one.
    let after2 = s.with(|e, _| e.flow_lines_arc());
    assert!(Arc::ptr_eq(&after, &after2));
    assert!(same_as_fresh(&mut s), "after the edit: the kept lines are the fresh ones");
    // Undo, step by step: whatever the document is after each step, the kept lines are what a layout made from nothing gives.
    for step in 0..4 {
        s.key("ctrl-z");
        s.settle();
        assert!(same_as_fresh(&mut s), "after undo {step}: the kept lines are the fresh ones");
    }
}

type Key = (usize, u32, u32, u32, u32);

fn keys(lines: &[crate::editing::LineGeom]) -> Vec<Key> {
    lines.iter().map(|l| (l.page, l.para, l.x.to_bits(), l.y.to_bits(), l.h.to_bits())).collect()
}

/// The kept lines against the lines of a layout made again from the document (the layout caches emptied first).
fn same_as_fresh(s: &mut Scenario) -> bool {
    let kept = keys(&s.with(|e, _| e.flow_lines_arc()));
    crate::flow_layout::clear_layout_caches();
    let fresh = keys(&s.with(|e, _| e.flow_lines_arc()));
    kept == fresh
}

/// The time of one frame's worth of the hot paths of #299 on a long document (it prints; it fails on nothing): `settings::get` 500 times,
/// `flow_lines` 50 times, and a typed character with the lines asked for after it, 20 times.
fn timing() {
    let mut s = Scenario::open("test/benchmark/securities-registration-statement.docx");
    s.size(1200.0, 900.0);
    s.settle();
    let pages = s.with(|e, _| e.flow_lines().iter().map(|l| l.page).max().unwrap_or(0) + 1);
    let median = |mut v: Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    let time = |f: &mut dyn FnMut()| {
        let t = crate::clock::Instant::now();
        f();
        t.elapsed().as_secs_f64() * 1000.0
    };
    let settings = median((0..9).map(|_| time(&mut || (0..500).for_each(|_| drop(ibis_office_base::settings::get("app.language"))))).collect());
    let lines = median((0..9).map(|_| time(&mut || s.with(|e, _| (0..50).for_each(|_| drop(e.flow_lines()))))).collect());
    let typing = median(
        (0..9)
            .map(|_| {
                let t = crate::clock::Instant::now();
                for _ in 0..20 {
                    s.type_text("a");
                    s.with(|e, _| drop(e.flow_lines()));
                }
                t.elapsed().as_secs_f64() * 1000.0
            })
            .collect(),
    );
    eprintln!("ISSUE299-TIMING pages={pages} settings_get_x500={settings:.3}ms flow_lines_x50={lines:.3}ms type_and_lines_x20={typing:.3}ms");
}
