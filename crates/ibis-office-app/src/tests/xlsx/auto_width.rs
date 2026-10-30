// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A number entered widens a column nobody sized by hand (issue #64); a hand-sized column and text stay as they are.
//! Excel's widths (`test/github-issue/64/auto-width/ui/FINDINGS.md`: 10 digits 11.0, 11 digits 12.0, 12 and more 12.29) are a
//! guide only: the widths here come from the same measure as "fit to selection".

use super::SheetSteps;
use ibis_doc_model::{Command, EMU_PER_POINT, SheetCommand, sheet_calc::sheet_of_slide, sheet_command::Axis};

fn custom(s: &mut crate::shell::scenario::Scenario, column: u32) -> bool {
    s.with(move |e, _| sheet_of_slide(e.slide()).unwrap().custom_columns.contains(&column))
}

pub(super) fn run() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    // 1. Digits added one by one widen column D (never narrow it); the widths are recorded.
    let mut widths = Vec::new();
    let mut last = s.size_pt("D");
    let first = last;
    for digits in 1..=16usize {
        s.click_cell("D5");
        s.enter(&"1234567890123456"[..digits], "enter");
        let w = s.size_pt("D");
        assert!(w >= last, "{digits} digits: {w} narrower than {last}");
        last = w;
        widths.push((digits, w));
    }
    eprintln!("auto-width widths (pt) from {first}: {widths:?}");
    assert!(last > first, "wide numbers widen the column");
    assert!(!custom(&mut s, 3), "a width the sheet chose is not a fixed one");
    // Past the General format's 11 characters (the exponent form) it stops growing.
    let at = |n: usize| widths[n - 1].1;
    assert_eq!(at(12), at(16));
    assert_eq!(at(11), at(12));
    // 2. Undo takes the widening back with the entry.
    s.click_cell("D6");
    let before = s.size_pt("D");
    s.enter("-1234567890123456", "enter");
    assert!(s.size_pt("D") >= before);
    s.click_cell("E6");
    let e_before = s.size_pt("E");
    s.enter("1234567890123456", "enter");
    let e_after = s.size_pt("E");
    assert!(e_after > e_before);
    s.key("cmd-z");
    assert_eq!(s.size_pt("E"), e_before, "undo gives the width back");
    s.key("cmd-shift-z");
    assert_eq!(s.size_pt("E"), e_after, "redo widens again");
    // 3. Text does not widen.
    s.click_cell("F5");
    let f_before = s.size_pt("F");
    s.enter("abcdefghijklmnopqrstuvwxyz0123456789", "enter");
    assert_eq!(s.size_pt("F"), f_before);
    // A formula's number does.
    s.click_cell("G5");
    let g_before = s.size_pt("G");
    s.enter("=123456789012*10", "enter");
    assert!(s.size_pt("G") > g_before);
    // 4. A column sized by hand does not widen.
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run_sheet(vec![Command::Sheet(SheetCommand::SetSize { slide, axis: Axis::Columns, first: 7, last: 7, size: 40 * EMU_PER_POINT })], cx);
    });
    assert!(custom(&mut s, 7));
    s.click_cell("H5");
    s.enter("1234567890123456", "enter");
    assert_eq!(s.size_pt("H"), 40.0);
    // 5. "Fit to selection" gives it back to the sheet: it widens again; empty cells change nothing.
    s.header_menu("H", "xlsx.header-fit-columns");
    assert!(!custom(&mut s, 7));
    let fitted = s.size_pt("H");
    s.click_cell("H6");
    s.enter("9", "enter");
    s.click_cell("H5");
    s.enter("1234567890", "enter");
    assert!(s.size_pt("H") >= fitted);
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run_sheet(vec![Command::Sheet(SheetCommand::SetSize { slide, axis: Axis::Columns, first: 9, last: 9, size: 33 * EMU_PER_POINT })], cx);
    });
    s.click_cell("J5");
    s.header_menu("J", "xlsx.header-fit-columns");
    assert!(custom(&mut s, 9), "fitting empty cells changes nothing");
    assert_eq!(s.size_pt("J"), 33.0);
}
