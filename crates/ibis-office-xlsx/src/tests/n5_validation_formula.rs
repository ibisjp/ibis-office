// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! N5: a typed entry that reads as a formula (`-x`) is checked against the
//! cell's data validation rule by its result, as a typed value is.
use super::SheetSteps;
use ibis_doc_model::{CellRange, Command, SheetCommand, Validation};
use crate::ext::*;

pub(super) fn run() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    s.update(|e, _, cx| {
        let rule = Validation {
            ranges: vec![CellRange { first_row: 0, last_row: 9, first_column: 0, last_column: 0 }],
            kind: "list".into(),
            operator: "between".into(),
            formula1: Some("\"A,B,C\"".into()),
            formula2: None,
            allow_blank: true,
            dropdown: true,
            prompt: None,
            error: None,
        };
        e.run_sheet(vec![Command::Sheet(SheetCommand::SetValidation { slide: e.current_slide, ranges: rule.ranges.clone(), rule: Some(rule) })], cx);
    });
    for text in ["D", "-x"] {
        s.click_cell("A1");
        s.type_text(text);
        s.key("enter");
        assert!(s.with(|e, _| e.wb().validation_rejected.is_some()), "{text} is refused");
        assert!(s.value("A1").is_empty(), "{text} does not enter the cell");
        s.key("enter");
        s.key("escape");
    }
}
