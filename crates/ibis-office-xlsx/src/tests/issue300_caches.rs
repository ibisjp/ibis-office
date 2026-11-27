// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #300: the sheet's shaped cell texts, the headers' geometry and labels are kept between frames. After each change that has to drop them
//! (an edit and its undo, zoom, a column's width, a row's height, a format, the sheet shown, the theme fonts) the window drawn from the kept
//! caches is the window drawn from caches emptied first (`release_layouts`).

use super::SheetSteps;
use crate::ext::*;
use crate::shell::scenario::Scenario;

const BOOK: &str = "test/benchmark/coffee-chain-sales-report.xlsx";

/// The window as drawn now with the caches kept, against the window drawn after the caches were emptied.
fn same_as_fresh(s: &mut Scenario, step: &str) {
    s.settle();
    let kept = s.picture();
    crate::sheet_view::release_layouts();
    s.settle();
    let fresh = s.picture();
    assert_eq!(kept.0, fresh.0, "{step}: width");
    assert!(kept.1 == fresh.1, "{step}: the window drawn from the kept caches differs from the one drawn from nothing");
}

pub(crate) fn caches_follow_the_changes() {
    let mut s = Scenario::open(BOOK);
    s.size(1200., 800.);
    same_as_fresh(&mut s, "as opened");
    // An edit and its undo.
    s.click_cell("C3");
    s.type_text("あいうえお ABC 12345");
    s.key("enter");
    same_as_fresh(&mut s, "typed");
    s.key("ctrl-z");
    same_as_fresh(&mut s, "undone");
    // A format.
    s.click_cell("C3");
    s.key("ctrl-b");
    same_as_fresh(&mut s, "bold");
    s.key("ctrl-z");
    same_as_fresh(&mut s, "bold undone");
    // A column's width and a row's height, dragged at the headers' lines.
    let (x, y) = s.header_line("C");
    s.drag((x, y), (x + 60.0, y));
    same_as_fresh(&mut s, "column widened");
    s.key("ctrl-z");
    same_as_fresh(&mut s, "column width undone");
    let (x, y) = s.header_line("4");
    s.drag((x, y), (x, y + 30.0));
    same_as_fresh(&mut s, "row heightened");
    s.key("ctrl-z");
    same_as_fresh(&mut s, "row height undone");
    // Zoom.
    s.update(|e, window, cx| e.set_zoom(Some(1.5), window, cx));
    same_as_fresh(&mut s, "zoomed in");
    s.update(|e, window, cx| e.set_zoom(Some(0.6), window, cx));
    same_as_fresh(&mut s, "zoomed out");
    s.update(|e, window, cx| e.set_zoom(Some(1.0), window, cx));
    // The theme's fonts.
    s.menu("xlsx.theme-fonts.arial");
    same_as_fresh(&mut s, "theme fonts changed");
    // Another sheet and back.
    let ids: Vec<_> = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.iter().filter_map(|sh| sh.slide).collect());
    if ids.len() > 1 {
        s.update(|e, window, cx| e.select_slide(ids[1], window, cx));
        same_as_fresh(&mut s, "second sheet");
        s.update(|e, window, cx| e.select_slide(ids[0], window, cx));
        same_as_fresh(&mut s, "first sheet again");
    }
}

/// The time of a window drawn with the caches kept and of one drawn after they were emptied (it prints; it fails on nothing).
pub(crate) fn timing() {
    let mut s = Scenario::open(BOOK);
    s.size(1400., 900.);
    s.settle();
    let median = |mut v: Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    let mut frame = |s: &mut Scenario, cold: bool| {
        if cold {
            crate::sheet_view::release_layouts();
        }
        let t = crate::clock::Instant::now();
        s.update(|_, _, cx| cx.notify());
        s.settle();
        t.elapsed().as_secs_f64() * 1000.0
    };
    drop(frame(&mut s, false));
    let warm = median((0..30).map(|_| frame(&mut s, false)).collect());
    let cold = median((0..30).map(|_| frame(&mut s, true)).collect());
    eprintln!("ISSUE300-TIMING xlsx_frame_kept={warm:.2}ms xlsx_frame_after_release={cold:.2}ms");
}
