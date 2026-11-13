// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #32: the check box in a cell (Insert > Check Box). It is drawn in place of TRUE/FALSE, a press on the box and the space bar turn it
//! (an undo step each), and it is saved and read back.

use super::SheetSteps;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{CellRange, CellValue, Command, SheetCommand, StyleChange, sheet_calc::sheet_of_slide};
use crate::ext::*;

fn is_box(s: &mut Scenario, row: u32, column: u32) -> bool {
    s.with(move |e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.cell(row, column).is_some_and(|c| sheet.styles[c.style as usize].checkbox)
    })
}

pub(super) fn run() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    s.click_cell("A1");
    s.enter("TRUE", "enter");
    // Insert > Check Box on A1:D3: empty cells get FALSE, TRUE stays, one undo step.
    s.click_cell("A1");
    for k in ["shift-down", "shift-down", "shift-right", "shift-right", "shift-right"] {
        s.key(k);
    }
    s.menu("xlsx.insert-cell-checkbox");
    assert_eq!(s.value("A1"), CellValue::Bool(true));
    assert_eq!(s.value("B2"), CellValue::Bool(false));
    assert!(is_box(&mut s, 0, 0) && is_box(&mut s, 2, 3));
    // Alignments and a bigger font, as in the probe compared with Excel.
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        let col = |c: u32| CellRange { first_row: 0, last_row: 2, first_column: c, last_column: c };
        e.run_sheet(
            vec![
                Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![col(1)], change: StyleChange::Align(ibis_doc_model::CellAlign::Left) }),
                Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![col(2)], change: StyleChange::Align(ibis_doc_model::CellAlign::Right) }),
                Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![CellRange { first_row: 1, last_row: 1, first_column: 0, last_column: 0 }], change: StyleChange::FontSize(1800) }),
                Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![CellRange { first_row: 1, last_row: 1, first_column: 1, last_column: 1 }], change: StyleChange::Fill(Some(ibis_doc_model::Rgb(0xFFFF00))) }),
            ],
            cx,
        );
    });
    s.click_cell("E5");
    s.settle();
    s.screenshot("issue32-checkboxes");
    // Where Excel (Microsoft 365, Windows) puts the 12 pt box of 游ゴシック in a 20 pt row, measured in its PDF of the same cells: 12 pt
    // square, its top 5.5 pt below the row's top (the box stands on the line's bottom, a descent below the baseline).
    for (row, column) in [(0u32, 0u32), (2, 3)] {
        let (top, size) = s.with(move |e, _| {
            let (shape, sheet) = e.current_sheet().unwrap();
            let _ = shape;
            let (l, t, r, b) = crate::sheet_view::checkbox_box(sheet, row, column).expect("the box was drawn");
            let row_top = sheet.row_offsets()[row as usize] as f64;
            ((t - row_top) / 12_700.0, ((r - l) / 12_700.0, (b - t) / 12_700.0))
        });
        assert!((size.0 - 12.0).abs() < 0.1 && (size.1 - 12.0).abs() < 0.1, "the box is the font size wide: {size:?}");
        assert!((top - 5.5).abs() < 0.6, "the box's top is {top} pt below the row's top, Excel's 5.5");
    }

    // A press on the box turns it, and is one undo step.
    s.click_cell("E5");
    let at = s.with(|e, _| e.checkbox_window_center(0, 1)).expect("A2's box was drawn");
    s.click(at);
    assert_eq!(s.value("A2"), CellValue::Bool(true));
    assert_eq!(s.selected(), "A2");
    s.key("cmd-z");
    assert_eq!(s.value("A2"), CellValue::Bool(false));
    // A press in the cell beside the box only selects it.
    s.click_cell("C2");
    assert_eq!(s.value("C2"), CellValue::Bool(false));
    // The space bar: the selected cell, then all selected ones take the opposite of the selected cell's.
    // (A press in A1 would be on its box.)
    s.update(|e, _, cx| {
        e.set_cell(0, 0);
        cx.notify();
    });
    s.type_text(" ");
    assert_eq!(s.value("A1"), CellValue::Bool(false));
    // (A press in A1 would be on its box.)
    s.update(|e, _, cx| {
        e.set_cell(0, 0);
        cx.notify();
    });
    s.key("shift-right");
    s.type_text(" ");
    assert_eq!((s.value("A1"), s.value("B1")), (CellValue::Bool(true), CellValue::Bool(true)));
    // A cell that is not a check box takes the space as typed text.
    s.click_cell("F1");
    s.type_text(" ");
    assert!(s.with(|e, _| e.cell_edit().is_some()));
    s.key("escape");

    // Saved and read back.
    s.save_as("issue32-checkbox.xlsx");
    let reopened = s.reopen();
    let slide = reopened.slides.iter().find(|sl| sheet_of_slide(sl).is_some_and(|sh| sh.cell(1, 1).is_some_and(|c| sh.styles[c.style as usize].checkbox))).expect("a sheet with the check boxes");
    let sheet = sheet_of_slide(slide).unwrap();
    assert_eq!(sheet.cell(0, 0).unwrap().value, CellValue::Bool(true));
}
