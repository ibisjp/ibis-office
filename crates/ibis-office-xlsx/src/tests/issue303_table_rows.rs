// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #303: a row more at the bottom of a table. Design: `tasks/doc/test-design/303.md` (T1... are its numbers); the behaviour is Excel 365's
//! (`ListRows.Add` through COM on 2026-11-26): Tab in the bottom-right cell adds a row (above the 集計行, which moves down), the calculated column's formula and
//! the number format of the row above come with it, one Undo; the handle at the table's bottom-right corner is dragged to resize it.
use super::SheetSteps;
use crate::ext::*;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{CellRange, CellValue};

pub(super) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::issue303_t1_tab_in_the_bottom_right_cell_adds_a_row", t1_tab_adds_a_row),
        ("xlsx::issue303_t2_one_undo_takes_the_row_back", t2_undo),
        ("xlsx::issue303_t3_with_a_totals_row_the_row_comes_in_above_it", t3_totals_row),
        ("xlsx::issue303_t4_one_undo_takes_the_row_above_the_totals_back", t4_totals_undo),
        ("xlsx::issue303_t5_tab_elsewhere_does_not_change_the_table", t5_tab_elsewhere),
        ("xlsx::issue303_t6_the_new_row_has_the_table_style_and_the_number_format", t6_style),
        ("xlsx::issue303_t7_the_handle_dragged_down_adds_rows", t7_drag_down),
        ("xlsx::issue303_t8_the_handle_dragged_up_shrinks_and_not_past_the_heading", t8_drag_up),
        ("xlsx::issue303_t9_cells_pasted_under_the_table_take_it_in", t9_paste),
        ("xlsx::issue303_t10_the_grown_table_is_saved", t10_saved),
    ]
}

pub(super) fn ignored() -> Vec<(&'static str, fn())> {
    vec![]
}

/// A new sheet with the table A1:C4 (headings 名・数・倍; 倍 a calculated column `=B2*2`); the numbers in B have the format `0.00`.
fn plain() -> Scenario {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    for (cell, text) in [("A1", "名"), ("B1", "数"), ("C1", "倍"), ("A2", "a"), ("B2", "1"), ("A3", "b"), ("B3", "2"), ("A4", "c"), ("B4", "3")] {
        s.click_cell(cell);
        s.enter(text, "enter");
    }
    s.update(|e, _, cx| {
        e.make_table(CellRange { first_row: 0, last_row: 3, first_column: 0, last_column: 2 }, true, cx).unwrap();
    });
    s.click_cell("C2");
    s.enter("=B2*2", "enter");
    assert_eq!(s.formula("C4").as_deref(), Some("B4*2"), "the calculated column fills");
    s.update(|e, _, cx| {
        e.run_sheet(
            vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetFormat {
                sheet: e.current_slide,
                ranges: vec![CellRange { first_row: 1, last_row: 3, first_column: 1, last_column: 1 }],
                change: ibis_doc_model::sheet_command::StyleChange::NumberFormat("0.00".into()),
            })],
            cx,
        );
    });
    s
}

/// The sheet with a 集計行: Table1 = A1:C6 (headings Item・Qty・Twice, Twice calculated, 集計行 on row 6).
fn totals() -> Scenario {
    let mut s = Scenario::open("test/editing/xlsx-table.xlsx");
    s.size(1200., 800.);
    s
}

fn range(s: &mut Scenario) -> CellRange {
    s.with(|e, _| e.current_sheet().unwrap().1.tables[0].range)
}

fn t1_tab_adds_a_row() {
    let mut s = plain();
    assert_eq!(range(&mut s), CellRange { first_row: 0, last_row: 3, first_column: 0, last_column: 2 });
    s.click_cell("C4");
    s.key("tab");
    assert_eq!(range(&mut s), CellRange { first_row: 0, last_row: 4, first_column: 0, last_column: 2 });
    assert_eq!(s.formula("C5").as_deref(), Some("B5*2"), "the calculated column's formula comes down");
    assert_eq!(s.selected(), "A5", "the cell goes to the first of the new row");
    assert_eq!(s.value("A5"), CellValue::Empty);
}

fn t2_undo() {
    let mut s = plain();
    s.click_cell("C4");
    s.key("tab");
    s.key("cmd-z");
    assert_eq!(range(&mut s), CellRange { first_row: 0, last_row: 3, first_column: 0, last_column: 2 });
    assert_eq!(s.formula("C5"), None);
    s.key("cmd-shift-z");
    assert_eq!(range(&mut s).last_row, 4);
    assert_eq!(s.formula("C5").as_deref(), Some("B5*2"));
}

fn t3_totals_row() {
    let mut s = totals();
    s.click_cell("C5");
    s.key("tab");
    assert_eq!(range(&mut s), CellRange { first_row: 0, last_row: 6, first_column: 0, last_column: 2 });
    let (totals_rows, header) = s.with(|e, _| {
        let t = &e.current_sheet().unwrap().1.tables[0];
        (t.totals_rows, t.header_rows)
    });
    assert_eq!((totals_rows, header), (1, 1));
    assert_eq!(s.shown("A7"), "Total", "the 集計行 moved down");
    assert_eq!(s.value("A6"), CellValue::Empty, "the new row is empty");
    assert!(s.formula("C6").is_some_and(|f| f.contains("Qty")), "the calculated column's formula comes down: {:?}", s.formula("C6"));
    assert_eq!(s.formula("B7").as_deref().map(|f| f.contains("SUBTOTAL")), Some(true));
    assert_eq!(s.selected(), "A6");
}

fn t4_totals_undo() {
    let mut s = totals();
    s.click_cell("C5");
    s.key("tab");
    s.key("cmd-z");
    assert_eq!(range(&mut s), CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 2 });
    assert_eq!(s.shown("A6"), "Total");
    assert_eq!(s.formula("C6"), None);
}

fn t5_tab_elsewhere() {
    let mut s = plain();
    s.click_cell("B4");
    s.key("tab");
    assert_eq!(range(&mut s).last_row, 3);
    assert_eq!(s.selected(), "C4");
    drop(s);
    let mut s = totals();
    s.click_cell("C6");
    s.key("tab");
    assert_eq!(range(&mut s).last_row, 5, "Tab in the 集計行 adds nothing");
}

fn t6_style() {
    let mut s = plain();
    s.click_cell("C4");
    s.key("tab");
    let (fill4, fill5, format5) = s.with(|e, _| {
        let sheet = e.current_sheet().unwrap().1;
        let fill = |r: u32| sheet.cell(r, 0).map(|c| sheet.styles[c.style as usize].fill);
        (fill(3), fill(4), sheet.cell(4, 1).map(|c| sheet.styles[c.style as usize].number_format.clone()))
    });
    assert_ne!(fill4, fill5, "the stripes go on");
    assert_eq!(format5.as_deref(), Some("0.00"), "the number format of the row above carries on");
}

/// The window position of the table's bottom-right corner (its handle is just inside it).
fn corner(s: &mut Scenario, last_column: u32, last_row: u32) -> (f32, f32) {
    s.with(move |e, _| {
        let a = e.cell_center(last_column, last_row).unwrap();
        let b = e.cell_center(last_column + 1, last_row + 1).unwrap();
        (f32::from(a.x + b.x) / 2.0, f32::from(a.y + b.y) / 2.0)
    })
}

fn center(s: &mut Scenario, column: u32, row: u32) -> (f32, f32) {
    s.with(move |e, _| {
        let a = e.cell_center(column, row).unwrap();
        (f32::from(a.x), f32::from(a.y))
    })
}

fn t7_drag_down() {
    let mut s = plain();
    s.click_cell("B2");
    let (cx, cy) = corner(&mut s, 2, 3);
    let to = center(&mut s, 2, 5);
    s.screenshot("issue303-before");
    s.drag((cx - 3.0, cy - 3.0), to);
    assert_eq!(range(&mut s), CellRange { first_row: 0, last_row: 5, first_column: 0, last_column: 2 });
    assert_eq!(s.formula("C6").as_deref(), Some("B6*2"));
    s.screenshot("issue303-after");
    s.key("cmd-z");
    assert_eq!(range(&mut s).last_row, 3, "one Undo");
}

fn t8_drag_up() {
    let mut s = plain();
    s.click_cell("B2");
    let (cx, cy) = corner(&mut s, 2, 3);
    let to = center(&mut s, 2, 2);
    s.drag((cx - 3.0, cy - 3.0), to);
    assert_eq!(range(&mut s).last_row, 2, "shrinks by one row");
    // Not past the headings: the table keeps its one row of data.
    s.click_cell("B2");
    let (cx, cy) = corner(&mut s, 2, 2);
    let to = center(&mut s, 2, 0);
    s.drag((cx - 3.0, cy - 3.0), to);
    assert_eq!(range(&mut s).last_row, 1);
}

fn t9_paste() {
    let mut s = plain();
    s.click_cell("A2");
    s.key("shift-down");
    s.key("shift-right");
    s.key("cmd-c");
    s.click_cell("A5");
    s.key("cmd-v");
    assert_eq!(range(&mut s).last_row, 5, "two rows pasted under the table");
    assert_eq!(s.value("A5"), CellValue::Text("a".into()), "the pasted value stays");
    assert_eq!(s.formula("C5").as_deref(), Some("B5*2"));
}

fn t10_saved() {
    let mut s = plain();
    s.click_cell("C4");
    s.key("tab");
    s.save_as("issue303.xlsx");
    let back = s.reopen();
    let t = &ibis_doc_model::sheet_calc::sheet_of_slide(back.slides.iter().find(|x| ibis_doc_model::sheet_calc::sheet_of_slide(x).is_some_and(|sh| !sh.tables.is_empty())).unwrap()).unwrap().tables[0];
    assert_eq!(t.range, CellRange { first_row: 0, last_row: 4, first_column: 0, last_column: 2 });
    assert_eq!(t.columns.len(), 3);
}
