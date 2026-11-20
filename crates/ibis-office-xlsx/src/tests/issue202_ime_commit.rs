// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #202: what the input method is still composing in a cell goes into that cell when the pointer leaves it for something else (a click on a sheet
//! tab, on the name box), as Excel does, and is not carried on to the other sheet or the name box. (A click on another cell did already.)
use super::SheetSteps;
use crate::shell::scenario::Scenario;
use crate::ext::*;
use ibis_doc_model::sheet::CellValue;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::issue202_a_sheet_tab_takes_the_composing_text_into_its_cell", a_sheet_tab_takes_the_composing_text_into_its_cell),
        ("xlsx::issue202_a_click_on_another_cell_takes_the_composing_text_into_its_cell", a_click_on_another_cell_takes_the_composing_text_into_its_cell),
    ]
}

fn composing() -> Scenario {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("xlsx.insert-sheet");
    let first = s.with(|e, _| e.presentation().slides[0].id);
    s.update(move |e, window, cx| e.select_slide(first, window, cx));
    s.click_cell("K20");
    s.ime_compose("\u{3042}\u{3044}\u{3046}");
    assert!(s.with(|e, _| e.cell_edit().is_some_and(|c| c.marked.is_some())), "the input method is composing in the cell");
    s
}

fn a_sheet_tab_takes_the_composing_text_into_its_cell() {
    let mut s = composing();
    let first = s.with(|e, _| e.current_slide);
    let other = s.with(|e, _| e.presentation().slides.last().unwrap().id);
    // What a click on the other sheet's tab does.
    s.update(move |e, window, cx| e.select_slide(other, window, cx));
    assert!(s.with(|e, _| e.cell_edit().is_none()), "the typing ended");
    assert_eq!(s.with(move |e, _| e.current_slide), other);
    // The text is in the cell of the sheet it was typed on, and nothing was typed on the other.
    s.update(move |e, window, cx| e.select_slide(first, window, cx));
    assert_eq!(s.value("K20"), CellValue::Text("\u{3042}\u{3044}\u{3046}".into()));
    s.update(move |e, window, cx| e.select_slide(other, window, cx));
    assert_eq!(s.value("K20"), CellValue::Empty);
}

fn a_click_on_another_cell_takes_the_composing_text_into_its_cell() {
    let mut s = composing();
    s.click_cell("K22");
    assert!(s.with(|e, _| e.cell_edit().is_none()));
    assert_eq!(s.value("K20"), CellValue::Text("\u{3042}\u{3044}\u{3046}".into()));
}
