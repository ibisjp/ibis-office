// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データの入力規則（リスト）: a full-width entry that matches a half-width
//! list item once converted to half-width is accepted (the user's decision of
//! 2026-11-03; Excel itself refuses it, measured by COM `Validation.Value`),
//! and the cell keeps what was typed.
use super::SheetSteps;
use ibis_doc_model::{CellRange, CellValue, Command, SheetCommand, Validation};
use crate::ext::*;

fn list(formula: &str, column: u32) -> Validation {
    Validation {
        ranges: vec![CellRange { first_row: 0, last_row: 9, first_column: column, last_column: column }],
        kind: "list".into(),
        operator: "between".into(),
        formula1: Some(formula.into()),
        formula2: None,
        allow_blank: true,
        dropdown: true,
        prompt: None,
        error: None,
    }
}

pub(super) fn list_accepts_full_width() {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    s.update(|e, _, cx| {
        let typed = list("\"ABC,def,123,ｱｲｳ,a b\"", 0); // i18n-ok: Japanese test fixture, not a UI label.
        let ranged = list("$D$1:$D$3", 1);
        for rule in [typed, ranged] {
            e.run_sheet(vec![Command::Sheet(SheetCommand::SetValidation { slide: e.current_slide, ranges: rule.ranges.clone(), rule: Some(rule) })], cx);
        }
    });
    s.click_cell("D1");
    s.type_text("ABC");
    s.key("enter");
    s.click_cell("D2");
    s.type_text("def");
    s.key("enter");
    s.click_cell("D3");
    s.type_text("123");
    s.key("enter");
    let ok = |s: &mut crate::shell::scenario::Scenario, column: u32, text: &str| s.with(|e, _| e.check_entry(0, column, &CellValue::Text(text.into())).is_ok());
    for column in [0, 1] {
        assert!(ok(&mut s, column, "ABC"), "exact, column {column}");
        assert!(ok(&mut s, column, "ＡＢＣ"), "full-width letters, column {column}"); // i18n-ok: Japanese test fixture, not a UI label.
        assert!(ok(&mut s, column, "ｄｅｆ"), "full-width lower case letters, column {column}"); // i18n-ok: Japanese test fixture, not a UI label.
        assert!(ok(&mut s, column, "１２３"), "full-width digits, column {column}"); // i18n-ok: Japanese test fixture, not a UI label.
        assert!(!ok(&mut s, column, "ＡＢＤ"), "a full-width text that matches nothing is still refused, column {column}"); // i18n-ok: Japanese test fixture, not a UI label.
        assert!(!ok(&mut s, column, "xyz"), "column {column}");
    }
    // Half-width kana and the space of the typed list.
    assert!(ok(&mut s, 0, "アイウ"), "full-width katakana to half-width kana"); // i18n-ok: Japanese test fixture, not a UI label.
    assert!(ok(&mut s, 0, "ａ　ｂ"), "full-width space to space"); // i18n-ok: Japanese test fixture, not a UI label.
    // Entering it through the cell: the cell keeps the typed text.
    s.click_cell("A1");
    s.type_text("ＡＢＣ"); // i18n-ok: Japanese test fixture, not a UI label.
    s.key("enter");
    assert_eq!(s.value("A1"), CellValue::Text("ＡＢＣ".into())); // i18n-ok: Japanese test fixture, not a UI label.
    assert!(s.with(|e, _| e.panel_message.is_none()));
    s.click_cell("A2");
    s.type_text("ＡＢＤ"); // i18n-ok: Japanese test fixture, not a UI label.
    s.key("enter");
    assert!(s.with(|e, _| e.panel_message.is_some()), "a non-matching entry is refused with the message");
}
