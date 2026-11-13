// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #176: copying the whole spill range of `=SEQUENCE(5)` and pasting it elsewhere pasted the spilled (ghost)
//! cells as values too, and the pasted formula met them and showed #SPILL!. Excel pastes the formula into the first cell only
//! and the paste spills again; ghost cells copied without their formula cell are pasted as values.

use super::SheetSteps;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{CellError, CellValue, sheet_calc::sheet_of_slide};
use crate::ext::*;

fn num(n: f64) -> CellValue {
    CellValue::Number(n)
}

fn has_formula(s: &mut Scenario, name: &str) -> bool {
    s.formula(name).is_some()
}

/// The spill of A1 (`=SEQUENCE(5)`) is in place; A1:A5 is selected.
fn start() -> Scenario {
    let mut s = super::new_sheet();
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
    s.click_cell("A1");
    s.enter("=SEQUENCE(5)", "enter");
    assert_eq!(s.value("A5"), num(5.0));
    s
}

fn select(s: &mut Scenario, from: &str, rows: usize) {
    s.click_cell(from);
    for _ in 1..rows {
        s.key("shift-down");
    }
}

fn assert_spilled_at(s: &mut Scenario, top: u32, column: &str, n: u32) {
    for i in 0..n {
        let name = format!("{column}{}", top + i);
        assert_eq!(s.value(&name), num((i + 1) as f64), "{name} holds the spilled value");
        assert!(matches!(s.value(&name), CellValue::Number(_)), "{name} is no error");
    }
    assert_eq!(s.formula(&format!("{column}{top}")).as_deref(), Some("SEQUENCE(5)"));
    for i in 1..n {
        let name = format!("{column}{}", top + i);
        assert!(!has_formula(s, &name), "{name} is a spilled cell, no formula");
    }
}

fn no_spill_error(s: &mut Scenario) {
    let any = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.cells.iter().any(|c| c.value == CellValue::Error(CellError::Spill))
    });
    assert!(!any, "no cell shows #SPILL!");
}

/// The whole spill range copied and pasted: the formula only, and it spills again.
pub(super) fn whole_range_pastes_the_formula() {
    let mut s = start();
    select(&mut s, "A1", 5);
    s.key("cmd-c");
    s.click_cell("C1");
    s.key("cmd-v");
    no_spill_error(&mut s);
    assert_spilled_at(&mut s, 1, "C", 5);
    assert!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().spills.contains_key(&(0, 2))), "C1 spills");
    // The source is as it was.
    assert_spilled_at(&mut s, 1, "A", 5);
    // Pasting again beside it (on the cells of the first paste's neighbour) works too.
    s.click_cell("E3");
    s.key("cmd-v");
    no_spill_error(&mut s);
    assert_spilled_at(&mut s, 3, "E", 5);
}

/// Pasted over a destination that is bigger (the copy is tiled): each tile spills.
pub(super) fn whole_range_tiled() {
    let mut s = start();
    select(&mut s, "A1", 5);
    s.key("cmd-c");
    select(&mut s, "C1", 10);
    s.key("cmd-v");
    no_spill_error(&mut s);
    assert_spilled_at(&mut s, 1, "C", 5);
    assert_spilled_at(&mut s, 6, "C", 5);
}

/// A copy of spilled cells without their formula cell: an ordinary paste and the formulas paste bring nothing (Excel), the
/// values paste brings the values.
pub(super) fn ghost_cells_alone_paste_nothing_but_values() {
    let mut s = start();
    select(&mut s, "A2", 3);
    s.key("cmd-c");
    s.click_cell("C1");
    s.key("cmd-v");
    no_spill_error(&mut s);
    s.update(|e, _, cx| e.paste_special(crate::clipboard::PasteSpecial::Formulas, cx));
    for i in 1..=3 {
        assert!(s.value(&format!("C{i}")).is_empty(), "C{i} stays empty");
    }
    s.click_cell("E1");
    s.key("cmd-shift-v");
    for (i, n) in [2.0, 3.0, 4.0].into_iter().enumerate() {
        let name = format!("E{}", i + 1);
        assert_eq!(s.value(&name), num(n));
        assert!(!has_formula(&mut s, &name), "{name} has no formula");
    }
}

/// 形式を選択して貼り付け: values give the values of the whole range; formulas give the formula that spills.
pub(super) fn paste_special_values_and_formulas() {
    let mut s = start();
    select(&mut s, "A1", 5);
    s.key("cmd-c");
    s.click_cell("C1");
    s.key("cmd-shift-v");
    no_spill_error(&mut s);
    for i in 1..=5 {
        let name = format!("C{i}");
        assert_eq!(s.value(&name), num(i as f64));
        assert!(!has_formula(&mut s, &name), "{name}: values only");
    }
    s.click_cell("E1");
    s.update(|e, _, cx| e.paste_special(crate::clipboard::PasteSpecial::Formulas, cx));
    no_spill_error(&mut s);
    assert_spilled_at(&mut s, 1, "E", 5);
}

/// Cut and paste: the formula and its spill move.
pub(super) fn cut_moves_the_spill() {
    let mut s = start();
    select(&mut s, "A1", 5);
    s.key("cmd-x");
    s.click_cell("C1");
    s.key("cmd-v");
    no_spill_error(&mut s);
    assert_spilled_at(&mut s, 1, "C", 5);
    for i in 1..=5 {
        assert!(s.value(&format!("A{i}")).is_empty(), "A{i} is empty after the cut");
    }
}

/// A relative reference in the formula moves with the paste.
pub(super) fn relative_reference_moves() {
    let mut s = super::new_sheet();
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
    s.click_cell("D1");
    s.enter("10", "enter");
    s.click_cell("E1");
    s.enter("20", "enter");
    s.click_cell("A1");
    s.enter("=SEQUENCE(3)*D1", "enter");
    select(&mut s, "A1", 3);
    s.key("cmd-c");
    s.click_cell("B5");
    s.key("cmd-v");
    no_spill_error(&mut s);
    assert_eq!(s.formula("B5").as_deref(), Some("SEQUENCE(3)*E5"));
}

/// The fill handle dragged right over the whole spill range: the formula is filled and spills again.
pub(super) fn fill_right_spills_again() {
    let mut s = start();
    select(&mut s, "A1", 5);
    s.drag_fill("A5", "B5");
    no_spill_error(&mut s);
    assert_eq!(s.formula("B1").as_deref(), Some("SEQUENCE(5)"));
    for i in 1..=5 {
        assert_eq!(s.value(&format!("B{i}")), num(i as f64));
    }
    assert!(!has_formula(&mut s, "B2"), "B2 is a spilled cell");
}

fn cut_to(s: &mut Scenario, from: &str, rows: usize, to: &str) {
    select(s, from, rows);
    s.key("cmd-x");
    s.click_cell(to);
    s.key("cmd-v");
}

/// A cut that holds the formula cell takes the whole array (A1 alone, A1:A3): column A is empty afterwards.
pub(super) fn cut_with_the_formula_cell_moves_all() {
    for rows in [1, 3] {
        let mut s = start();
        cut_to(&mut s, "A1", rows, "C1");
        no_spill_error(&mut s);
        assert_spilled_at(&mut s, 1, "C", 5);
        for i in 1..=5 {
            assert!(s.value(&format!("A{i}")).is_empty(), "rows {rows}: A{i} is empty after the cut");
        }
    }
}

/// A cut of spilled cells alone does nothing (Excel refuses): the array stays, nothing is pasted.
pub(super) fn cut_of_ghost_cells_alone_is_refused() {
    let mut s = start();
    cut_to(&mut s, "A2", 3, "C1");
    no_spill_error(&mut s);
    assert_spilled_at(&mut s, 1, "A", 5);
    for i in 1..=3 {
        assert!(s.value(&format!("C{i}")).is_empty(), "C{i} stays empty");
    }
}
