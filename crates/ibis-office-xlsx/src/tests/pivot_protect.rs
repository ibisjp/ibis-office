// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A new pivot table's sheet (after the current one, opened, its first cell selected), the fields pane that follows the
//! current cell into a table and back out (pushed and popped), and what a pivot table's cells allow (measured on Excel
//! 16.0 build 20527: tasks/doc/reports/2026-11-03-pivot-insert-panel-protect.md).

use super::SheetSteps;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{CellValue, PivotAxis};
use crate::ext::*;

const BOOK: &str = "test/editing/xlsx-pivot-not-refreshed.xlsx";

fn new_pivot() -> Scenario {
    let mut s = Scenario::open(BOOK);
    s.size(1400., 800.);
    s.click_cell("K5");
    s.menu("xlsx.pivot-insert");
    s.update(|e, window, cx| e.create_pivot(window, cx));
    s.settle();
    s
}

fn sheet_names(s: &mut Scenario) -> Vec<String> {
    s.with(|e, _| {
        let wb = e.presentation().workbook.as_ref().unwrap();
        wb.sheets.iter().map(|sh| sh.slide.and_then(|id| e.presentation().slide(id)).and_then(|sl| sl.name.clone()).unwrap_or_default()).collect()
    })
}

fn pivot(s: &mut Scenario) -> ibis_doc_model::PivotTable {
    s.with(|e, _| e.current_sheet().and_then(|(_, sh)| sh.pivots.first().cloned())).expect("the pivot table")
}

fn field(s: &mut Scenario, name: &str) -> usize {
    pivot(s).fields.iter().position(|f| f.name == name).unwrap_or_else(|| panic!("no field {name}"))
}

/// Rows 地域, columns 四半期, values 金額.
fn fill_pivot(s: &mut Scenario) {
    let (region, quarter, amount) = (field(s, "地域"), field(s, "四半期"), field(s, "金額")); // i18n-ok（ユーザー文書の項目名）
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.change_pivot((slide, 0), |p| {
            p.place(region, Some(PivotAxis::Row), usize::MAX);
            p.place(quarter, Some(PivotAxis::Column), usize::MAX);
            p.place(amount, Some(PivotAxis::Values), usize::MAX);
        }, cx);
    });
    s.settle();
}

fn pane_open(s: &mut Scenario) -> bool {
    s.with(|e, _| e.sidebar_open && e.wb().panels.pivot.as_ref().is_some_and(|p| p.target.is_some()))
}

fn status(s: &mut Scenario) -> String {
    s.with(|e, _| e.status.as_ref().map(|t| t.to_string()).unwrap_or_default())
}

/// The reference of the first cell showing `text`.
fn find(s: &mut Scenario, text: &str) -> String {
    s.with(|e, _| {
        let (_, sh) = e.current_sheet().unwrap();
        for row in 0..40 {
            for column in 0..12 {
                if sh.cell(row, column).is_some_and(|c| c.text == text) {
                    return ibis_doc_model::cell_name(column, row);
                }
            }
        }
        panic!("no cell shows {text}")
    })
}

/// The sheet comes after the current one, opens, and its first cell is selected; Undo takes the sheet with it.
pub(super) fn new_sheet_after_current() {
    let mut s = Scenario::open(BOOK);
    s.size(1400., 800.);
    s.click_cell("K5");
    let before = sheet_names(&mut s);
    let current = s.with(|e, _| e.presentation().slide(e.current_slide).and_then(|sl| sl.name.clone()).unwrap());
    let at = before.iter().position(|n| *n == current).unwrap();
    s.menu("xlsx.pivot-insert");
    s.update(|e, window, cx| e.create_pivot(window, cx));
    s.settle();
    let after = sheet_names(&mut s);
    assert_eq!(after.len(), before.len() + 1);
    let now = s.with(|e, _| e.presentation().slide(e.current_slide).and_then(|sl| sl.name.clone()).unwrap());
    assert_eq!(after.iter().position(|n| *n == now), Some(at + 1), "after the current sheet: {after:?} from {before:?}");
    assert_eq!(s.selected(), "A3");
    assert!(pane_open(&mut s), "the fields are shown");
    s.screenshot("pivot-new-sheet");
    s.key("cmd-z");
    s.settle();
    assert_eq!(sheet_names(&mut s), before, "one Undo takes the sheet away");
}

/// The fields pane is pushed over the pane that was on show when the cell enters the table, and popped when it leaves.
pub(super) fn pane_push_and_pop() {
    let mut s = new_pivot();
    fill_pivot(&mut s);
    // Created: the fields are the cell's own; out of the table they go (nothing was before them: the pane closes).
    assert!(pane_open(&mut s));
    s.click_cell("H20");
    s.settle();
    assert!(!pane_open(&mut s) && !s.with(|e, _| e.sidebar_open), "popped: closed");
    // Another pane on show is kept and comes back.
    s.update(|e, window, cx| e.open_names(window, cx));
    s.settle();
    assert!(s.with(|e, _| e.wb().names.is_some()));
    s.click_cell("A3");
    s.settle();
    assert!(pane_open(&mut s), "pushed: the fields");
    assert!(s.with(|e, _| e.wb().names.is_none()), "the names are kept in memory, not shown");
    s.screenshot("pivot-pane-pushed");
    // Table to table (the same one: another cell of it): nothing is pushed again.
    s.click_cell("B4");
    s.settle();
    assert!(pane_open(&mut s));
    s.click_cell("H20");
    s.settle();
    assert!(!pane_open(&mut s), "popped");
    assert!(s.with(|e, _| e.wb().names.is_some() && e.sidebar_open), "the names are back");
    s.screenshot("pivot-pane-popped");
    // The user opens another pane while the cell is in the table: the user's stands, nothing is put back.
    s.click_cell("A3");
    s.settle();
    assert!(pane_open(&mut s));
    s.update(|e, window, cx| e.open_names(window, cx));
    s.settle();
    assert!(!pane_open(&mut s) && s.with(|e, _| e.wb().names.is_some()), "manual wins");
    s.click_cell("B4");
    s.settle();
    assert!(!pane_open(&mut s), "not opened again while the user's pane stands in the table");
    s.click_cell("H20");
    s.settle();
    assert!(s.with(|e, _| e.wb().names.is_some()), "still the user's pane");
    // The user closes the fields: not opened again until the cell has left the table.
    s.click_cell("A3");
    s.settle();
    assert!(pane_open(&mut s));
    s.update(|e, _, cx| e.close_pivot_panel(cx));
    s.settle();
    s.click_cell("B4");
    s.settle();
    assert!(!pane_open(&mut s), "closed by the user");
    s.click_cell("H20");
    s.settle();
    s.click_cell("A3");
    s.settle();
    assert!(pane_open(&mut s), "in again after being out");
}

/// A value refuses, a label is renamed, formats are free, and what changes the table is refused.
pub(super) fn cells_allow_and_refuse() {
    let mut s = new_pivot();
    fill_pivot(&mut s);
    let built = pivot(&mut s);
    let value = s.with(|e, _| {
        let (_, sh) = e.current_sheet().unwrap();
        (0..30).flat_map(|r| (0..10).map(move |c| (r, c))).find(|&(r, c)| matches!(sh.cell(r, c).map(|x| &x.value), Some(CellValue::Number(_))) && c > 0).unwrap()
    });
    let value_ref = ibis_doc_model::cell_name(value.1, value.0);
    let before = s.shown(&value_ref);
    let region = find(&mut s, &built.fields.iter().find(|f| f.name == "地域").unwrap().shared.iter().find_map(|v| if let CellValue::Text(t) = v { Some(t.clone()) } else { None }).unwrap()); // i18n-ok（ユーザー文書の項目名）

    // A value: F2 does not open the editor, typing and Delete change nothing, the message is Excel's.
    s.click_cell(&value_ref);
    s.key("f2");
    assert!(s.with(|e, _| e.cell_edit().is_none()), "F2 does not edit a value");
    assert!(status(&mut s).contains("変更できません") || status(&mut s).contains("cannot change"), "{}", status(&mut s)); // i18n-ok（Excel の日本語のメッセージ・小計の見出しとの照合）
    s.enter("999", "enter");
    assert_eq!(s.shown(&value_ref), before);
    s.key("delete");
    assert_eq!(s.shown(&value_ref), before);
    assert!(status(&mut s).contains("影響") || status(&mut s).contains("affect"), "{}", status(&mut s)); // i18n-ok（Excel の日本語のメッセージ・小計の見出しとの照合）
    // Pasting and filling into the table are refused too.
    s.click_cell(&value_ref);
    s.key("cmd-c");
    s.key("cmd-v");
    assert_eq!(s.shown(&value_ref), before);

    // A format is free.
    s.click_cell(&value_ref);
    s.toolbar("bold");
    let bold = s.with(|e, _| {
        let (_, sh) = e.current_sheet().unwrap();
        sh.cell(value.0, value.1).map(|c| sh.styles[c.style as usize].bold)
    });
    assert_eq!(bold, Some(true));
    assert_eq!(s.shown(&value_ref), before);

    // A label is renamed: the item (kept in the table, so it stays when built again).
    let old = s.shown(&region);
    s.click_cell(&region);
    s.enter("北日本", "enter"); // i18n-ok（Excel の日本語のメッセージ・小計の見出しとの照合）
    assert_eq!(s.shown(&region), "北日本"); // i18n-ok（ユーザーが入力する名前）
    let p = pivot(&mut s);
    assert!(p.aliases.items.iter().any(|(_, o, n)| *o == old && n == "北日本"), "{:?}", p.aliases); // i18n-ok（ユーザーが入力する名前）
    s.key("cmd-z");
    s.settle();
    assert_eq!(s.shown(&region), old, "Undo");
    // An empty name and a formula are refused with Excel's messages.
    s.click_cell(&region);
    s.enter("=1+1", "enter");
    assert_eq!(s.shown(&region), old);
    assert!(status(&mut s).contains("数式") || status(&mut s).contains("formula"), "{}", status(&mut s)); // i18n-ok（Excel の日本語のメッセージ・小計の見出しとの照合）

    // The value field's name, and the same name as a field: Excel's warning.
    let caption = find(&mut s, &built.data_fields.first().map(|d| d.name.clone()).unwrap_or_else(|| "合計 / 金額".into())); // i18n-ok（ユーザー文書の名前）
    s.click_cell(&caption);
    s.enter("地域", "enter"); // i18n-ok（ユーザー文書の項目名）
    assert!(status(&mut s).contains("既に使用") || status(&mut s).contains("already exists"), "{}", status(&mut s)); // i18n-ok（Excel の日本語のメッセージ・小計の見出しとの照合）
    s.click_cell(&caption);
    s.enter("売上高", "enter"); // i18n-ok（ユーザーが入力する名前）
    assert_eq!(pivot(&mut s).data_fields[0].name, "売上高"); // i18n-ok（ユーザーが入力する名前）
    // Rows through the table cannot be inserted or deleted; the row under it can.
    s.click_cell(&region);
    s.update(|e, _, cx| e.delete_lines(ibis_doc_model::sheet_command::Axis::Rows, cx));
    assert_eq!(s.shown(&region), old, "{}", status(&mut s));
    s.screenshot("pivot-protect");
}
