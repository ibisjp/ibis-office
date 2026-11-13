// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #185: a formula entered with its cell references (and function names) in lower case is stored and shown in upper
//! case, as Excel does when the entry is committed. Text in quotes and defined names keep what was typed.
use super::SheetSteps;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{Command, SheetCommand};

fn entered(s: &mut Scenario, cell: &str, text: &str) -> String {
    s.click_cell(cell);
    s.enter(text, "enter");
    s.formula(cell).expect("a formula")
}

pub(crate) fn typed_formulas_are_stored_in_upper_case() {
    let mut s = super::new_sheet();
    assert_eq!(entered(&mut s, "D1", "=a1"), "A1");
    assert_eq!(entered(&mut s, "D2", "=sum(a1:b2)+\"abc\""), "SUM(A1:B2)+\"abc\"");
    assert_eq!(entered(&mut s, "D3", "=$a$1+b$2"), "$A$1+B$2");
    assert_eq!(entered(&mut s, "D4", "=sum(a:a)"), "SUM(A:A)");
    // A sheet written in another case takes the sheet's own name.
    let name = s.with(|e, _| e.slide().name.clone().unwrap());
    let typed = format!("={}!c3", name.to_lowercase());
    assert_eq!(entered(&mut s, "D5", &typed), format!("{}!C3", ibis_calc::formula::quote_sheet(&name)));
}

pub(crate) fn defined_names_keep_their_case() {
    let mut s = super::new_sheet();
    s.update(|e, _, _| {
        e.document.execute(vec![Command::Sheet(SheetCommand::SetName { name: "myRate".into(), sheet: None, formula: Some("2".into()) })]).unwrap();
    });
    assert_eq!(entered(&mut s, "D1", "=myRate*a1"), "myRate*A1");
}
