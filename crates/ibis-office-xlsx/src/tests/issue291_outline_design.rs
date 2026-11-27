// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #291: grouping rows and columns (the outline) on the sheet, as Excel does it. Designed from the requirement and Excel's behaviour, not from the
//! implementation (`tasks/doc/test-design/issue-291.md`; the numbers X1... are that document's). Each is `#[ignore]`d (listed in `ignored()` only)
//! until the build shows whether it passes; the ones that pass move to `all()` as regression tests, the others are fixed and then moved.
//!
//! Where a test clicks a button of the outline bar it uses the places Excel's measured bar puts them (`tasks/doc/requests/2026-11-24-291-outline-measure-result.md`,
//! 100 %): the buttons are 23 px squares, the row bar's level `n` button starts 2 px + 23 x (n - 1) from the sheet area's left, the − button of a group
//! is on the row after the group (the summary row). They are the spec: a bar that puts them elsewhere has to say why.
use ibis_doc_model::sheet_calc::sheet_of_slide;

use crate::shell::scenario::Scenario;
use super::SheetSteps;
use crate::ext::*;

/// X12 (the keys) passes on Windows (W8) but fails in the macOS scenarios (2026-11-26, Mac W5/W12): it waits here until the Windows xlsx fixes it for both.
pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    all_designed().into_iter().filter(|t| t.0.starts_with("xlsx::issue291_x12_")).collect()
}

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    all_designed().into_iter().filter(|t| !t.0.starts_with("xlsx::issue291_x12_")).collect()
}

fn all_designed() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::issue291_x1_grouped_rows_get_the_outline_bar_and_a_minus_button", x1_rows_bar_and_minus),
        ("xlsx::issue291_x2_grouped_columns_get_the_outline_bar_and_a_minus_button", x2_columns_bar_and_minus),
        ("xlsx::issue291_x3_nested_levels_and_the_eight_level_limit", x3_nested_levels),
        ("xlsx::issue291_x4_ungroup_takes_one_level_off_the_selected_rows", x4_ungroup),
        ("xlsx::issue291_x5_inner_group_stays_collapsed_when_the_outer_one_is_opened_again", x5_inner_stays_collapsed),
        ("xlsx::issue291_x6_level_buttons_show_down_to_the_level", x6_level_buttons),
        ("xlsx::issue291_x7_hide_detail_and_show_detail_work_on_the_group_of_the_selection", x7_menu_detail),
        ("xlsx::issue291_x8_undo_and_redo_of_group_ungroup_collapse_expand", x8_undo_redo),
        ("xlsx::issue291_x9_inserting_and_deleting_rows_inside_a_group", x9_insert_delete_rows),
        ("xlsx::issue291_x10_save_and_reopen_keeps_levels_and_collapsed_state_and_the_bar", x10_save_reopen),
        ("xlsx::issue291_x11_the_bar_is_there_in_a_narrow_window", x11_narrow_window),
        ("xlsx::issue291_x12_alt_shift_arrows_group_and_ungroup", x12_alt_shift_arrows),
    ]
}

fn rows_levels(s: &mut Scenario) -> Vec<(u32, u8)> {
    s.with(|e, _| sheet_of_slide(e.slide()).unwrap().outline_rows.iter().map(|(r, l)| (*r, *l)).collect())
}

fn columns_levels(s: &mut Scenario) -> Vec<(u32, u8)> {
    s.with(|e, _| sheet_of_slide(e.slide()).unwrap().outline_columns.iter().map(|(c, l)| (*c, *l)).collect())
}

fn new_sheet() -> Scenario {
    let mut s = Scenario::new_document(ibis_formats::Kind::Workbook);
    s.size(1200., 800.);
    s.settle();
    for row in 1..=20 {
        s.click_cell(&format!("A{row}"));
        s.enter(&row.to_string(), "enter");
    }
    s
}

/// Select rows `first..=last` (1-based) by their cells in column A and group them.
fn group_rows(s: &mut Scenario, first: u32, last: u32) {
    s.click_cell(&format!("A{first}"));
    for _ in first..last {
        s.key("shift-down");
    }
    s.menu("xlsx.group");
}

fn is_hidden(s: &mut Scenario, row: u32) -> bool {
    s.size_pt(&row.to_string()) == 0.0
}

/// Where the cells' grid starts on the screen (the x of column A's middle, the y of row 1's middle): the bar pushes it right and down.
fn grid_origin(s: &mut Scenario) -> (f32, f32) {
    s.with(|e, _| {
        let p = e.cell_center(0, 0).expect("A1 on screen");
        (f32::from(p.x), f32::from(p.y))
    })
}

/// The middle of the row bar's level `level` (1-based) button in the line of row `row` (1-based; the summary row of a group is the row after it).
fn row_button(s: &mut Scenario, level: u32, row: u32) -> (f32, f32) {
    let ws = s.with(|e, _| e.workspace_bounds.expect("the workspace"));
    let y = s.with(|e, _| e.cell_center(0, row - 1).expect("the row on screen").y);
    (f32::from(ws.left()) + 2.0 + 23.0 * (level - 1) as f32 + 11.5, f32::from(y))
}

/// X1: rows 3-6 grouped on a new sheet: the outline bar is there (the grid moved right), the group's − is on the summary row (row 7), a click
/// on it hides rows 3-6 and shows a + there, a click on the + shows them. (The report of #291: the bar and the − were not shown.)
fn x1_rows_bar_and_minus() {
    let mut s = new_sheet();
    let before = grid_origin(&mut s);
    group_rows(&mut s, 3, 6);
    let after = grid_origin(&mut s);
    assert!(after.0 - before.0 >= 20.0, "the outline bar takes room at the left: {before:?} -> {after:?}");
    assert_eq!(rows_levels(&mut s).iter().map(|(_, l)| *l).collect::<Vec<_>>(), [1, 1, 1, 1]);
    let minus = row_button(&mut s, 1, 7);
    s.click(minus);
    for row in 3..=6 {
        assert!(is_hidden(&mut s, row), "row {row} is hidden after the − is clicked");
    }
    assert!(!is_hidden(&mut s, 2) && !is_hidden(&mut s, 7), "the rows around stay");
    // The summary row (7) has moved up to where row 3 was, and the + is on it.
    let plus = row_button(&mut s, 1, 7);
    s.click(plus);
    for row in 3..=6 {
        assert!(!is_hidden(&mut s, row), "row {row} is shown after the + is clicked");
    }
}

/// X2: the same for columns C-F: the bar above the headers (the grid moved down), the − on the column after the group (G).
fn x2_columns_bar_and_minus() {
    let mut s = new_sheet();
    let before = grid_origin(&mut s);
    let c = s.header("C");
    s.click(c);
    let f = s.header("F");
    s.shift_click(f);
    s.header_menu("C", "xlsx.header-group-columns");
    let after = grid_origin(&mut s);
    assert!(after.1 - before.1 >= 20.0, "the outline bar takes room above the headers: {before:?} -> {after:?}");
    assert_eq!(columns_levels(&mut s).iter().map(|(_, l)| *l).collect::<Vec<_>>(), [1, 1, 1, 1]);
    let ws = s.with(|e, _| e.workspace_bounds.expect("the workspace"));
    let g = s.with(|e, _| e.cell_center(6, 0).expect("G on screen"));
    let minus = (f32::from(g.x), f32::from(ws.top()) + 11.5);
    s.click(minus);
    for column in ["C", "D", "E", "F"] {
        assert_eq!(s.size_pt(column), 0.0, "column {column} is hidden after the − is clicked");
    }
    // The summary column (G) has moved left to where C was, and the + is on it.
    let g = s.with(|e, _| e.cell_center(6, 0).expect("G on screen"));
    s.click((f32::from(g.x), f32::from(ws.top()) + 11.5));
    assert!(s.size_pt("D") > 0.0, "the + shows them again");
}

/// X3: a group inside a group makes level 2; nesting goes to 7 groups (8 buttons) and no further.
fn x3_nested_levels() {
    let mut s = new_sheet();
    group_rows(&mut s, 3, 6);
    group_rows(&mut s, 4, 5);
    assert_eq!(rows_levels(&mut s), [(2, 1), (3, 2), (4, 2), (5, 1)], "3-6 is level 1, 4-5 inside it is level 2");
    for _ in 0..10 {
        group_rows(&mut s, 4, 5);
    }
    let deepest = rows_levels(&mut s).iter().map(|(_, l)| *l).max().unwrap();
    assert_eq!(deepest, 7, "the eighth group is not made: Excel's deepest `outlineLevel` is 7 (its buttons number 8), and a file with 8 does not open in Excel (W9)");
    let before = rows_levels(&mut s);
    group_rows(&mut s, 4, 5);
    assert_eq!(rows_levels(&mut s), before, "grouping at the deepest level does nothing");
}

/// X4: Ungroup takes one level off the selected rows only; a part of a group split off leaves the rest grouped.
fn x4_ungroup() {
    let mut s = new_sheet();
    group_rows(&mut s, 3, 6);
    group_rows(&mut s, 4, 5);
    s.click_cell("A4");
    s.key("shift-down");
    s.menu("xlsx.ungroup");
    assert_eq!(rows_levels(&mut s), [(2, 1), (3, 1), (4, 1), (5, 1)], "4-5 are level 1 again, 3-6 stay");
    s.click_cell("A3");
    s.key("shift-down");
    s.menu("xlsx.ungroup");
    assert_eq!(rows_levels(&mut s), [(4, 1), (5, 1)], "3-4 left the group: 5-6 remain");
    s.click_cell("A5");
    s.key("shift-down");
    s.menu("xlsx.ungroup");
    assert!(rows_levels(&mut s).is_empty(), "nothing is grouped");
}

/// X5 (Excel on Windows, measured: the click sequence in the result file): with 3-6 and 4-5 grouped, the inner − (on row 6) hides 4-5; the outer −
/// (on row 7) hides 3-6; the outer + shows 3 and 6 but **4-5 stay hidden**.
fn x5_inner_stays_collapsed() {
    let mut s = new_sheet();
    group_rows(&mut s, 3, 6);
    group_rows(&mut s, 4, 5);
    let inner = row_button(&mut s, 2, 6);
    s.click(inner);
    assert!(is_hidden(&mut s, 4) && is_hidden(&mut s, 5) && !is_hidden(&mut s, 3) && !is_hidden(&mut s, 6), "the inner − hides 4-5 only");
    let outer = row_button(&mut s, 1, 7);
    s.click(outer);
    assert!((3..=6).all(|r| is_hidden(&mut s, r)), "the outer − hides 3-6");
    let outer = row_button(&mut s, 1, 7); // the summary row has moved up
    s.click(outer);
    assert!(!is_hidden(&mut s, 3) && !is_hidden(&mut s, 6), "the outer + shows 3 and 6");
    assert!(is_hidden(&mut s, 4) && is_hidden(&mut s, 5), "4-5 are still collapsed");
}

/// X6: the level buttons at the bar's top-left: level 1 hides every group's detail, level 3 shows all, level 2 hides the inner groups only. The
/// buttons are in the line of the column headers (a sheet with rows grouped only: at the top of the bar).
fn x6_level_buttons() {
    let mut s = new_sheet();
    group_rows(&mut s, 3, 6);
    group_rows(&mut s, 4, 5);
    group_rows(&mut s, 9, 12);
    let ws = s.with(|e, _| e.workspace_bounds.expect("the workspace"));
    let level = |n: u32| (f32::from(ws.left()) + 2.0 + 23.0 * (n - 1) as f32 + 11.5, f32::from(ws.top()) + 11.5);
    s.click(level(1));
    assert!([3, 4, 5, 6, 9, 10, 11, 12].iter().all(|r| is_hidden(&mut s, *r)), "level 1 hides all the detail");
    s.click(level(3));
    assert!([3, 4, 5, 6, 9, 10, 11, 12].iter().all(|r| !is_hidden(&mut s, *r)), "level 3 shows all");
    s.click(level(2));
    assert!(is_hidden(&mut s, 4) && is_hidden(&mut s, 5), "level 2 hides the inner group");
    assert!(!is_hidden(&mut s, 3) && !is_hidden(&mut s, 6) && !is_hidden(&mut s, 9), "level 2 keeps the outer groups open");
}

/// X7: Data > Show/Hide Detail (詳細を表示しない／詳細を表示) acts on the group of the selected cell: in the inner group only that one, in the outer
/// one all of it.
fn x7_menu_detail() {
    let mut s = new_sheet();
    group_rows(&mut s, 3, 6);
    group_rows(&mut s, 4, 5);
    s.click_cell("A4");
    s.menu("xlsx.collapse");
    assert!(is_hidden(&mut s, 4) && is_hidden(&mut s, 5) && !is_hidden(&mut s, 3), "the inner group only");
    s.click_cell("A3");
    s.menu("xlsx.collapse");
    assert!((3..=6).all(|r| is_hidden(&mut s, r)), "the outer group");
    s.click_cell("A2");
    s.menu("xlsx.expand");
    assert!((3..=6).all(|r| is_hidden(&mut s, r)), "a cell outside any group: nothing changes");
}

/// X8: Undo and Redo of grouping, ungrouping, collapsing and expanding.
fn x8_undo_redo() {
    let mut s = new_sheet();
    group_rows(&mut s, 3, 6);
    s.key("secondary-z");
    assert!(rows_levels(&mut s).is_empty(), "Undo takes the group away");
    s.key("secondary-shift-z");
    assert_eq!(rows_levels(&mut s).len(), 4, "Redo makes it again");
    s.click_cell("A4");
    s.menu("xlsx.collapse");
    assert!(is_hidden(&mut s, 4));
    s.key("secondary-z");
    assert!(!is_hidden(&mut s, 4), "Undo shows the detail again");
    s.key("secondary-shift-z");
    assert!(is_hidden(&mut s, 4), "Redo hides it");
    s.menu("xlsx.expand");
    s.key("secondary-z");
    assert!(is_hidden(&mut s, 4), "Undo of the expansion collapses again");
    s.click_cell("A3");
    for _ in 0..3 {
        s.key("shift-down");
    }
    s.menu("xlsx.ungroup");
    s.key("secondary-z");
    assert_eq!(rows_levels(&mut s).len(), 4, "Undo of Ungroup brings the group back");
}

/// X9: a row inserted inside a group joins it, a row inserted above its first row does not; a row deleted inside it shrinks it; deleting every row
/// of it takes the group away.
fn x9_insert_delete_rows() {
    let mut s = new_sheet();
    group_rows(&mut s, 3, 6);
    s.click_cell("A4");
    s.menu("xlsx.insert-row-above");
    assert_eq!(rows_levels(&mut s).iter().map(|(r, _)| *r).collect::<Vec<_>>(), [2, 3, 4, 5, 6], "the row inserted inside joins the group (rows 3-7)");
    s.click_cell("A3");
    s.menu("xlsx.insert-row-above");
    assert_eq!(rows_levels(&mut s).iter().map(|(r, _)| *r).collect::<Vec<_>>(), [3, 4, 5, 6, 7], "the row inserted above the first row is outside (the group is rows 4-8)");
    // A row inserted right below the group (the summary row's place) joins it too: it takes the level of the row above (Excel, W6 A and J).
    s.click_cell("A9");
    s.menu("xlsx.insert-row-above");
    assert_eq!(rows_levels(&mut s).iter().map(|(r, _)| *r).collect::<Vec<_>>(), [3, 4, 5, 6, 7, 8], "the row inserted right below the group joins it (rows 4-9)");

    s.click_cell("A5");
    s.menu("xlsx.delete-row");
    assert_eq!(rows_levels(&mut s).len(), 5, "a row deleted inside shrinks the group (6 rows to 5)");
    s.click_cell("A4");
    for _ in 0..4 {
        s.key("shift-down");
    }
    s.menu("xlsx.delete-row");
    assert!(rows_levels(&mut s).is_empty(), "no row of the group is left");
}

/// X10: saved and opened again: the levels, the collapsed state (hidden rows), the bar and the + on the summary row come back.
fn x10_save_reopen() {
    // (A new document cannot be saved yet: an opened file.)
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    group_rows(&mut s, 4, 7);
    group_rows(&mut s, 5, 6);
    s.click_cell("A5");
    s.menu("xlsx.collapse");
    s.click_cell("A4");
    s.menu("xlsx.collapse");
    let path = s.save_as("issue291-x10.xlsx");
    let saved = s.reopen();
    let sheet = sheet_of_slide(&saved.slides[0]).unwrap().clone();
    assert_eq!(sheet.outline_rows.iter().map(|(r, l)| (*r, *l)).collect::<Vec<_>>(), [(3, 1), (4, 2), (5, 2), (6, 1)]);
    assert!((3..=6).all(|r| sheet.rows[r as usize] == 0), "the collapsed rows are hidden in the file");
    drop(s);
    let mut s = Scenario::open(path.to_str().unwrap());
    s.size(1200., 800.);
    s.settle();
    let origin_after_reopen = grid_origin(&mut s);
    assert!(origin_after_reopen.0 > 40.0, "the bar is there in the opened file: {origin_after_reopen:?}");
}

/// X11: a window narrower than 720 px (K0061): the outline bar is there and its buttons work (a touch screen needs the same bar; the size of the hit
/// area on touch is a decision for the user, D1 in the design).
fn x11_narrow_window() {
    for width in [719., 500.] {
        let mut s = new_sheet();
        s.size(width, 800.);
        s.settle();
        let before = grid_origin(&mut s);
        group_rows(&mut s, 3, 6);
        let after = grid_origin(&mut s);
        assert!(after.0 - before.0 >= 20.0, "width {width}: the outline bar takes room at the left");
        let minus = row_button(&mut s, 1, 7);
        s.click(minus);
        assert!(is_hidden(&mut s, 4), "width {width}: the − works");
    }
}

/// X12: Alt+Shift+→ groups the selected rows and Alt+Shift+← ungroups them (Excel for Windows; the source is third-party, a Windows check is in
/// the request, W8: this test is switched on when Windows confirms it).
fn x12_alt_shift_arrows() {
    let mut s = new_sheet();
    s.click_cell("A3");
    for _ in 0..3 {
        s.key("shift-down");
    }
    // Windows: Alt+Shift+→ / ←. Mac: Cmd+Shift+K / J (lib.rs key_bindings; the keys differ per platform).
    let (group, ungroup) = if crate::shell::app::is_mac() { ("cmd-shift-k", "cmd-shift-j") } else { ("alt-shift-right", "alt-shift-left") };
    s.key(group);
    assert_eq!(rows_levels(&mut s).len(), 4, "Alt+Shift+→ (Mac: Cmd+Shift+K) groups rows 3-6");
    s.key(ungroup);
    assert!(rows_levels(&mut s).is_empty(), "Alt+Shift+← ungroups them");
}
