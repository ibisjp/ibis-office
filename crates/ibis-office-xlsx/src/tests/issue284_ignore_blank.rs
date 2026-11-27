// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #284 (2): the data validation's 「空白を無視する」 (`allow_blank`). Design: `tasks/doc/test-design/issue-284-ignore-blank.md`; the values are Excel's
//! (tasks/doc/requests/2026-11-24-284-ignore-blank-result.md, Microsoft 365 Excel 16.0 build 20625).
//!
//! The book: A1 り, A2 ん, A3 empty, A4 ご; B1 and B2 are lists of `=$A$1:$A$4`, C1 and C2 lists of `り,ん,ご`; B1 and C1 ignore blanks (ON), B2 and C2 do not
//! (OFF). Every cell holds り at the start. What the flag decides is only whether an empty cell is valid; the dropdown's empty item does not depend on it.
use super::SheetSteps;
use crate::ext::*;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{CellEdit, CellInput, CellRange, CellValue, Command, SheetCommand, Validation};

/// (cell, (column, row) 0-based, ignores blanks, the source is a range).
const CELLS: [(&str, (u32, u32), bool, bool); 4] = [("B1", (1, 0), true, true), ("B2", (1, 1), false, true), ("C1", (2, 0), true, false), ("C2", (2, 1), false, false)];

/// All pass on Mac (N1, N2, N6 on 2026-11-26; N3, N5 after the Windows fix 2c9bdbf52).
pub(super) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::issue284_n1_an_empty_entry_is_checked_by_the_flag", n1_empty_entry_follows_the_flag),
        ("xlsx::issue284_n2_the_delete_key_is_never_checked", n2_delete_key),
        ("xlsx::issue284_n6_an_empty_cell_is_circled_only_when_blanks_are_not_ignored", n6_circles),
        ("xlsx::issue284_n3_an_empty_commit_from_the_editor_is_refused_only_when_blanks_are_not_ignored", n3_empty_commit_in_the_editor),
        ("xlsx::issue284_n5_a_value_not_in_the_list_is_refused_either_way", n5_value_not_in_the_list),
    ]
}

pub(super) fn ignored() -> Vec<(&'static str, fn())> {
    vec![
    ]
}

fn book(dropdown: bool) -> Scenario {
    let mut s = super::new_sheet();
    s.size(1200., 800.);
    for (cell, text) in [("A1", "り"), ("A2", "ん"), ("A4", "ご")] {
        s.click_cell(cell);
        s.enter(text, "enter");
    }
    s.update(|e, _, cx| {
        let mut commands = Vec::new();
        for (_, (column, row), ignore_blank, range) in CELLS {
            let rule = Validation {
                ranges: vec![CellRange { first_row: row, last_row: row, first_column: column, last_column: column }],
                kind: "list".into(),
                operator: "between".into(),
                formula1: Some(if range { "=$A$1:$A$4" } else { "\"り,ん,ご\"" }.into()),
                allow_blank: ignore_blank,
                dropdown,
                ..Default::default()
            };
            commands.push(Command::Sheet(SheetCommand::SetValidation { sheet: e.current_slide, ranges: rule.ranges.clone(), rule: Some(rule) }));
        }
        e.run_sheet(commands, cx);
    });
    for (cell, ..) in CELLS {
        s.click_cell(cell);
        s.enter("り", "enter");
        assert_eq!(s.value(cell), CellValue::Text("り".into()), "{cell} starts with り");
    }
    s
}

/// The rule of the cell (`column`, `row`).
fn rule_of(s: &mut Scenario, column: u32, row: u32) -> Validation {
    s.with(move |e, _| e.current_sheet().unwrap().1.validations.iter().find(|v| v.ranges[0].first_row == row && v.ranges[0].first_column == column).unwrap().clone())
}

/// N1: Excel checks an empty entry by the flag alone (the cell's `Validation.Value`: ON true, OFF false), also when the list's source has an empty cell (B2).
fn n1_empty_entry_follows_the_flag() {
    let mut s = book(true);
    for (cell, (column, row), ignore_blank, _) in CELLS {
        let ok = s.with(move |e, _| e.check_entry(row, column, &CellValue::Empty).is_ok());
        assert_eq!(ok, ignore_blank, "{cell}: an empty entry is {} (Excel)", if ignore_blank { "accepted" } else { "refused" });
        let rule = rule_of(&mut s, column, row);
        let keeps = s.with(move |e, _| e.keeps_validation_in_book(&rule, &CellValue::Empty, row, column));
        assert_eq!(keeps, ignore_blank, "{cell}: the empty value keeps the rule only when blanks are ignored");
    }
}

/// N2: the Delete key empties the cell without any check, in all four (Excel: no error, the value becomes empty). So the flag cannot be seen by it.
fn n2_delete_key() {
    let mut s = book(true);
    for (cell, ..) in CELLS {
        s.click_cell(cell);
        s.key("delete");
        assert_eq!(s.value(cell), CellValue::Empty, "{cell}: Delete empties the cell");
        assert!(s.with(|e, _| e.wb().validation_rejected.is_none()), "{cell}: Delete is not refused");
    }
}

/// N3: F2, remove the text, Enter (the dropdown off, so that the list does not pick an item for the Enter): with the flag ON the cell becomes empty, OFF it is refused
/// (the message, the editor stays, the value is the old one). Excel's list opens by itself when the text is emptied and Enter picks an item then: not tested here.
fn n3_empty_commit_in_the_editor() {
    let mut s = book(false);
    for (cell, _, ignore_blank, _) in CELLS {
        s.click_cell(cell);
        s.key("f2");
        s.key("backspace");
        s.key("enter");
        if ignore_blank {
            assert_eq!(s.value(cell), CellValue::Empty, "{cell}: ON, the empty entry is accepted");
            assert!(s.with(|e, _| e.wb().validation_rejected.is_none()), "{cell}: ON, nothing is refused");
        } else {
            assert!(s.with(|e, _| e.wb().validation_rejected.is_some()), "{cell}: OFF, the empty entry is refused");
            assert!(s.with(|e, _| e.panel_message.is_some()), "{cell}: OFF, with the message");
            s.key("escape");
            assert_eq!(s.value(cell), CellValue::Text("り".into()), "{cell}: OFF, the cell keeps its value");
        }
    }
}

/// N4: the dropdown of a source range with an empty cell has the empty item at the cell's place (り, ん, empty, ご) with the flag ON and OFF alike; the comma separated
/// list has no empty item (り, ん, ご).
pub(super) fn n4_dropdown_empty_item() {
    let mut s = book(true);
    for (cell, (column, row), _, range) in CELLS {
        let rule = rule_of(&mut s, column, row);
        let items = s.with(move |e, _| e.validation_list_items_at(&rule, row, column));
        let want: Vec<String> = if range { ["り", "ん", "", "ご"].map(String::from).to_vec() } else { ["り", "ん", "ご"].map(String::from).to_vec() };
        assert_eq!(items, want, "{cell}: the dropdown's items");
    }
}

/// N5: a value that is not in the list (x) is refused with the flag ON and OFF alike: ON ignores the empty, it does not let other values through.
fn n5_value_not_in_the_list() {
    let mut s = book(true);
    for (cell, ..) in CELLS {
        s.click_cell(cell);
        s.type_text("x");
        s.key("enter");
        assert!(s.with(|e, _| e.wb().validation_rejected.is_some()), "{cell}: x is refused");
        assert!(s.with(|e, _| e.panel_message.as_ref().is_some_and(|t| t.contains("リストから選択してください"))), "{cell}: with the message"); // i18n-ok: the Japanese locale's message
        s.key("escape");
        assert_eq!(s.value(cell), CellValue::Text("り".into()), "{cell}: the cell keeps its value");
    }
}

/// N6: 「無効なデータのマーク」: an empty cell is circled only with the flag OFF (B2, C2); the cell with x is circled in all four.
fn n6_circles() {
    let mut s = book(true);
    for (cell, ..) in CELLS {
        s.click_cell(cell);
        s.key("delete");
    }
    let invalid = s.with(|e, _| e.invalid_cells());
    let mut want: Vec<(u32, u32)> = CELLS.iter().filter(|c| !c.2).map(|c| c.1).collect();
    want.sort_unstable();
    assert_eq!(invalid, want, "the empty cells that are circled: the ones that do not ignore blanks");
    s.update(|e, _, cx| {
        let edits = CELLS.iter().map(|c| CellEdit { row: c.1.1, column: c.1.0, input: CellInput::Value(CellValue::Text("x".into())) }).collect();
        e.run_sheet(vec![Command::Sheet(SheetCommand::SetCells { sheet: e.current_slide, edits })], cx);
    });
    let invalid = s.with(|e, _| e.invalid_cells());
    let mut all: Vec<(u32, u32)> = CELLS.iter().map(|c| c.1).collect();
    all.sort_unstable();
    assert_eq!(invalid, all, "x is circled in all four");
}
