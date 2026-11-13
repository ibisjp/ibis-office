// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A row whose height nobody fixed (no `customHeight`) follows its wrapping cells as Excel's does: when a long text is entered,
//! wrapping is turned on or off, cells are pasted or a column's width changes. A row sized by hand stays. One undo step each.
//! And the headers' line is green and 2 px wide where rows or columns are hidden.

use ibis_doc_model::{Command, EMU_PER_POINT, SheetCommand, StyleChange, sheet_calc::sheet_of_slide, sheet_command::Axis};

use super::SheetSteps;
use crate::shell::scenario::Scenario;
use crate::ext::*;

const LONG: &str = "The quick brown fox jumps over the lazy dog and keeps running through the long long field until the night comes";

fn fixed(s: &mut Scenario, row: u32) -> bool {
    s.with(move |e, _| sheet_of_slide(e.slide()).unwrap().custom_rows.contains(&row))
}

pub(super) fn long_text_grows_the_row() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    let normal = s.size_pt("2");
    // Wrapping on, then a long text: the row grows by itself, and is not a fixed height.
    s.click_cell("B2");
    s.update(|e, _, cx| e.format_cells(StyleChange::Wrap(true), cx));
    assert_eq!(s.size_pt("2"), normal, "an empty wrapping cell: the row stays");
    s.enter(LONG, "enter");
    let grown = s.size_pt("2");
    assert!(grown > normal * 2.0, "the row grew to the lines: {grown} pt from {normal}");
    assert!(!fixed(&mut s, 1));
    s.screenshot("wrapped-grown");
    // One undo takes the text and the height back.
    s.key("cmd-z");
    assert_eq!(s.size_pt("2"), normal, "undo gives the height back with the entry");
    s.key("cmd-shift-z");
    assert_eq!(s.size_pt("2"), grown, "redo");
    // Text cleared: the row shrinks back (Excel fits it again).
    s.click_cell("B2");
    s.key("delete");
    s.update(|e, _, cx| e.fit_wrapped_rows(ibis_doc_model::CellRange { first_row: 1, last_row: 1, first_column: 0, last_column: 5 }, cx));
    assert_eq!(s.size_pt("2"), normal, "no text: the row is at the default again");
}

pub(super) fn a_row_sized_by_hand_stays() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run_sheet(vec![Command::Sheet(SheetCommand::SetSize { slide, axis: Axis::Rows, first: 2, last: 2, size: 30 * EMU_PER_POINT })], cx);
    });
    assert!(fixed(&mut s, 2));
    s.click_cell("B3");
    s.update(|e, _, cx| e.format_cells(StyleChange::Wrap(true), cx));
    s.enter(LONG, "enter");
    assert_eq!(s.size_pt("3"), 30.0, "a height set by hand is not changed by the text");
    // Excel's 行の高さの自動調整 gives it back to the sheet: it fits, and the mark is gone.
    let (x, y) = s.header_line("3");
    s.double_click((x, y));
    assert!(s.size_pt("3") > 30.0);
    assert!(!fixed(&mut s, 2), "fitted: not a fixed height");
}

pub(super) fn wrap_on_off_and_column_width() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    let normal = s.size_pt("5");
    s.click_cell("C5");
    s.enter(LONG, "enter");
    assert_eq!(s.size_pt("5"), normal, "not wrapping: one line");
    s.click_cell("C5");
    s.update(|e, _, cx| e.format_cells(StyleChange::Wrap(true), cx));
    let wrapped = s.size_pt("5");
    assert!(wrapped > normal * 2.0, "wrapping turned on: {wrapped}");
    s.key("cmd-z");
    assert_eq!(s.size_pt("5"), normal, "one undo: wrapping and height");
    s.key("cmd-shift-z");
    // A wider column: fewer lines (Excel fits the row at once).
    s.update(|e, _, cx| {
        e.select_lines(Axis::Columns, 2, 2, cx);
        e.set_selected_column_width(60.0, cx);
    });
    let wide = s.size_pt("5");
    assert!(wide < wrapped, "a wider column needs fewer lines: {wide} < {wrapped}");
    s.screenshot("wider-column");
    s.key("cmd-z");
    assert_eq!(s.size_pt("5"), wrapped, "undo of the width gives the height back");
    // Wrapping off: the row shrinks back.
    s.click_cell("C5");
    s.update(|e, _, cx| e.format_cells(StyleChange::Wrap(false), cx));
    assert_eq!(s.size_pt("5"), normal, "wrapping off: the default height");
}

pub(super) fn hidden_lines_are_marked_in_the_headers() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        let hide = |axis, first, last| Command::Sheet(SheetCommand::Hide { slide, axis, first, last, hidden: true });
        e.run_sheet(vec![hide(Axis::Rows, 2, 3), hide(Axis::Rows, 6, 6), hide(Axis::Columns, 1, 2), hide(Axis::Columns, 5, 5)], cx);
    });
    s.screenshot("hidden-middle");
    // Hidden from the first row and column.
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        let hide = |axis, first, last| Command::Sheet(SheetCommand::Hide { slide, axis, first, last, hidden: true });
        e.run_sheet(vec![hide(Axis::Rows, 0, 0), hide(Axis::Columns, 0, 0)], cx);
    });
    s.screenshot("hidden-first");
    // The line of a hidden run still sizes the column before it and shows the hidden ones (the hit test is unchanged).
    let (x, y) = s.header_line("D");
    s.double_click((x, y));
}
