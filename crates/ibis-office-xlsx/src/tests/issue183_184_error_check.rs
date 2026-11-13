// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issues #183 and #184.
//! #183: the error button (!) sits at the left of the selected cell; in the first visible column it was under the row header
//! (or the frozen columns), so its message was out of reach. Excel puts it at the right of the cell then.
//! #184: the values a dynamic array formula spills are formula results: no "number stored as text" on them (and no
//! "convert to number", which would write over the spill).

use super::SheetSteps;
use crate::ext::*;
use crate::shell::scenario::Scenario;
use ibis_doc_model::error_check::{ErrorKind, errors_at};
use ibis_doc_model::sheet_calc::sheet_of_slide;

fn on_right(s: &mut Scenario) -> bool {
    s.with(|e, _| e.error_button_on_right())
}

/// The button is at the left of the cell where there is room, at the right in the first visible column.
pub fn error_button_stays_reachable() {
    let mut s = super::new_sheet();
    s.click_cell("A1");
    s.enter("=1/0", "enter");
    s.click_cell("A1");
    assert_eq!(s.with(|e, _| e.current_error()), Some(ErrorKind::EvalError));
    assert!(on_right(&mut s), "A1 is under the row header at the left: the button goes to the right");
    s.click_cell("B1");
    s.enter("=1/0", "enter");
    s.click_cell("B1");
    assert_eq!(s.with(|e, _| e.current_error()), Some(ErrorKind::EvalError));
    assert!(!on_right(&mut s), "B1 has the room of column A at its left");
}

/// With the first columns frozen, the first column after them has no room either.
pub fn error_button_stays_reachable_beside_frozen_columns() {
    let mut s = super::new_sheet();
    s.click_cell("C1");
    s.enter("=1/0", "enter");
    s.click_cell("C1");
    s.menu("xlsx.freeze");
    assert_eq!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().frozen.map(|f| f.0)), Some(2));
    s.click_cell("C1");
    assert_eq!(s.with(|e, _| e.current_error()), Some(ErrorKind::EvalError));
    assert!(on_right(&mut s), "C1 is at the edge of the frozen columns");
}

/// `=TEXT(SEQUENCE(5),"000")`: the spilled texts look like numbers, but are formula results.
pub fn spilled_texts_have_no_number_as_text() {
    let mut s = super::new_sheet();
    s.click_cell("A1");
    s.enter("=TEXT(SEQUENCE(5),\"000\")", "enter");
    assert_eq!(s.value("A5"), ibis_doc_model::CellValue::Text("005".into()));
    for (row, name) in ["A1", "A2", "A3", "A4", "A5"].iter().enumerate() {
        s.click_cell(name);
        let errors = s.with(|e, _| {
            let sheet = sheet_of_slide(e.slide()).unwrap();
            errors_at(sheet, row as u32, 0)
        });
        assert!(errors.is_empty(), "{name} has no error mark: {errors:?}");
        assert_eq!(s.with(|e, _| e.current_error()), None, "{name}: no button, no menu");
    }
    // A text constant typed beside it is still marked.
    s.click_cell("C1");
    s.enter("'007", "enter");
    s.click_cell("C1");
    assert_eq!(s.with(|e, _| e.current_error()), Some(ErrorKind::NumberAsText));
}
