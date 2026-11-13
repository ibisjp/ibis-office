// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #154: a list source Excel does not accept (`TRANSPOSE`) works in ibisOffice, but the first time one is set in a file the panel
//! asks (once per file; the answer is kept in the file); キャンセル keeps the rule as it was; sources Excel accepts are not asked.
use super::SheetSteps;
use crate::ext::*;
use crate::validation::list_source_needs_ibis_extension;

fn set_list(s: &mut super::Scenario, cell: &str, source: &str) {
    s.click_cell(cell);
    let source = source.to_string();
    s.update(move |e, window, cx| {
        e.open_validation_panel(window, cx);
        if let Some(p) = e.wb_mut().validation.as_mut() {
            p.kind = "list";
            p.items.update(cx, |i, cx| i.set_value(source.clone(), window, cx));
        }
        e.apply_validation_panel(cx);
    });
}

/// The list source of the rule at the selected cell.
fn formula_here(s: &mut super::Scenario) -> Option<String> {
    s.with(|e, _| e.current_rule().and_then(|r| r.formula1))
}

pub(super) fn judge() {
    for yes in ["TRANSPOSE(A1:A3)", "transpose (A1:A3)", "INDEX(TRANSPOSE(A1:A3),1)", "IF(1,TRANSPOSE(A1:A3),B1)", "_xlfn.TRANSPOSE(A1:A3)"] {
        assert!(list_source_needs_ibis_extension(yes), "{yes}");
    }
    for no in ["$A$1:$A$3", "A1:A3", "OFFSET(A1,0,0,3,1)", "INDEX(A1:A3,0)", "\"TRANSPOSE(A1)\"", "\"a,b\"", "MYTRANSPOSE(A1)", "TRANSPOSE_LIST", "Sheet1!A1:A3"] {
        assert!(!list_source_needs_ibis_extension(no), "{no}");
    }
}

pub(super) fn run() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    for (cell, v) in [("D1", "a"), ("D2", "b"), ("D3", "c")] {
        s.click_cell(cell);
        s.enter(v, "enter");
    }
    assert!(!s.with(|e, _| e.presentation().ibis_extensions));
    // (d) What Excel accepts is not asked.
    for (cell, source) in [("F1", "=$D$1:$D$3"), ("F2", "=OFFSET(D1,0,0,3,1)")] {
        set_list(&mut s, cell, source);
        assert!(s.with(|e, _| e.ask.is_none()), "{source} is not asked");
        assert!(s.with(|e, _| e.current_rule().is_some()), "{source} is set");
    }
    assert!(!s.with(|e, _| e.presentation().ibis_extensions));
    // (a) TRANSPOSE asks; (c) キャンセル keeps the panel and sets nothing.
    set_list(&mut s, "G1", "=TRANSPOSE(D1:D3)");
    let message = s.with(|e, _| e.ask.as_ref().map(|a| a.message.to_string()));
    assert_eq!(message, Some(crate::shell::i18n::text("xlsx.validation.list-needs-extension").to_string()));
    s.update(|e, _, cx| e.answer_ask(false, cx));
    assert!(s.with(|e, _| e.ask.is_none() && e.current_rule().is_none() && !e.presentation().ibis_extensions));
    assert!(s.with(|e, _| e.wb().validation.is_some()), "the panel stays for a change");
    // (b) はい sets the flag and the rule.
    s.update(|e, _, cx| e.apply_validation_panel(cx));
    s.update(|e, _, cx| e.answer_ask(true, cx));
    assert!(s.with(|e, _| e.presentation().ibis_extensions));
    assert_eq!(formula_here(&mut s).as_deref(), Some("TRANSPOSE(D1:D3)"));
    // Another rule with TRANSPOSE: not asked again.
    set_list(&mut s, "G2", "=TRANSPOSE(D1:D2)");
    assert!(s.with(|e, _| e.ask.is_none()));
    assert_eq!(formula_here(&mut s).as_deref(), Some("TRANSPOSE(D1:D2)"));
    // Kept in the saved file.
    s.save_as("issue154-compat-notice.xlsx");
    let saved = s.reopen();
    assert!(saved.ibis_extensions, "the flag is in the file");
    let found = saved.slides.iter().filter_map(|sl| ibis_doc_model::sheet_calc::sheet_of_slide(sl)).flat_map(|sh| sh.validations.iter()).any(|v| v.formula1.as_deref() == Some("TRANSPOSE(D1:D3)"));
    assert!(found, "the rule is in the file");
}
