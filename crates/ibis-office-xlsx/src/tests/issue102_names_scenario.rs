// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue #102: the name manager as a list (search, filter, sort, add, edit, delete, paste the list into the sheet,
//! LAMBDA, names with errors, keys, putting a name into a formula, the pane's width).

use super::SheetSteps;
use crate::shell::scenario::Scenario;
use crate::names_list::{NameFilter, NameKind, NameRow, ScopeFilter, SortColumn};
use ibis_doc_model::{CellValue, Command, SheetCommand};
use crate::ext::*;

const BOOK: &str = "test/editing/xlsx-sales.xlsx";

fn define(s: &mut Scenario, name: &str, sheet: Option<usize>, formula: &str) {
    s.update(|e, _, _| {
        e.document.execute(vec![Command::Sheet(SheetCommand::SetName { name: name.into(), sheet, formula: Some(formula.into()) })]).unwrap();
    });
}

/// A workbook with names of every kind: ranges, constants, a formula, a LAMBDA, a helper, a name with an error and
/// two names of one name (the workbook's and the first sheet's).
fn with_names() -> Scenario {
    let mut s = Scenario::open(BOOK);
    let sheet = s.with(|e, _| e.slide().name.clone().unwrap());
    let q = ibis_calc::formula::quote_sheet(&sheet);
    define(&mut s, "売上金額", None, &format!("{q}!$B$4:$B$8"));
    define(&mut s, "税率", None, "0.1");
    define(&mut s, "合計", None, &format!("SUM({q}!$B$4:$B$8)"));
    define(&mut s, "税込", None, "LAMBDA(金額, [税率], 金額*(1+税率))");
    define(&mut s, "_Helper", None, "LAMBDA(x, x)");
    define(&mut s, "旧_売上", None, "#REF!");
    define(&mut s, "対象月", Some(0), &format!("{q}!$A$1"));
    define(&mut s, "対象月", None, &format!("{q}!$A$2"));
    s.update(|e, w, cx| e.open_names(w, cx));
    s.settle();
    s
}

fn rows(s: &mut Scenario) -> Vec<NameRow> {
    s.with(|e, cx| e.names_shown(cx))
}

fn names(s: &mut Scenario) -> Vec<String> {
    rows(s).into_iter().map(|r| r.name).collect()
}

fn set_search(s: &mut Scenario, text: &str) {
    s.update(|e, w, cx| {
        let input = e.sheet_state().unwrap().names.as_ref().unwrap().search.clone();
        input.update(cx, |i, cx| i.set_value(text, w, cx));
    });
}

fn set_filter(s: &mut Scenario, f: impl FnOnce(&mut NameFilter)) {
    s.update(|e, _, cx| {
        let filter = &mut e.sheet_state_mut().unwrap().names.as_mut().unwrap().filter;
        *filter = NameFilter::default();
        f(filter);
        cx.notify();
    });
}

fn all_names(s: &mut Scenario) -> Vec<(String, Option<usize>, String)> {
    s.with(|e, _| e.presentation().workbook.as_ref().unwrap().names.iter().filter(|n| !n.name.starts_with("_xl")).map(|n| (n.name.clone(), n.sheet, n.formula.clone())).collect())
}

fn dialog_set(s: &mut Scenario, name: Option<&str>, formula: &str) {
    s.update(|e, w, cx| {
        let d = e.sheet_state().unwrap().names.as_ref().unwrap().dialog.as_ref().unwrap();
        if let Some(name) = name {
            d.name.update(cx, |i, cx| i.set_value(name, w, cx));
        }
        d.formula.update(cx, |i, cx| i.set_value(formula, w, cx));
    });
    s.settle();
}

fn dialog_open(s: &mut Scenario) -> bool {
    s.with(|e, _| e.sheet_state().unwrap().names.as_ref().unwrap().dialog.is_some())
}

/// The list: kinds, scopes, values, the LAMBDA's parameters, the name with an error.
pub(super) fn list_contents() {
    let mut s = with_names();
    s.screenshot("issue102-list");
    let all = rows(&mut s);
    assert_eq!(all.len(), 8, "{all:?}");
    // The default order: the name, ascending.
    assert_eq!(names(&mut s)[0], "_Helper");
    let by = |s: &mut Scenario, n: &str| rows(s).into_iter().find(|r| r.name == n).unwrap();
    assert_eq!(by(&mut s, "売上金額").kind, NameKind::Range);
    assert!(by(&mut s, "売上金額").value.starts_with('{'), "{}", by(&mut s, "売上金額").value);
    assert_eq!(by(&mut s, "税率").kind, NameKind::Constant);
    assert_eq!(by(&mut s, "税率").value, "0.1");
    assert_eq!(by(&mut s, "合計").kind, NameKind::Formula);
    assert!(!by(&mut s, "合計").value.is_empty());
    let lambda = by(&mut s, "税込");
    assert_eq!(lambda.kind, NameKind::Lambda);
    assert_eq!(lambda.summary, "(金額, [税率]) → 金額*(1+税率)");
    assert_eq!(lambda.value, "（関数）");
    let broken = by(&mut s, "旧_売上");
    assert!(broken.error);
    assert_eq!(broken.value, "#REF!");
    // Two names of one name: told apart by scope.
    let same: Vec<_> = rows(&mut s).into_iter().filter(|r| r.name == "対象月").collect();
    assert_eq!(same.len(), 2);
    assert!(same.iter().any(|r| r.sheet.is_none()) && same.iter().any(|r| r.sheet == Some(0) && !r.scope.is_empty()));
}

/// Search, filter, sort.
pub(super) fn search_filter_sort() {
    let mut s = with_names();
    set_search(&mut s, "税");
    assert_eq!(names(&mut s), vec!["税率", "税込"]);
    set_search(&mut s, "sum(");
    assert_eq!(names(&mut s), vec!["合計"]);
    set_search(&mut s, "存在しない名前");
    assert!(names(&mut s).is_empty());
    set_search(&mut s, "");
    set_filter(&mut s, |f| f.errors_only = true);
    assert_eq!(names(&mut s), vec!["旧_売上"]);
    set_filter(&mut s, |f| f.hide_helpers = true);
    assert_eq!(rows(&mut s).len(), 7);
    assert!(!names(&mut s).contains(&"_Helper".to_string()));
    set_filter(&mut s, |f| f.kinds = vec![NameKind::Lambda]);
    assert_eq!(names(&mut s), vec!["_Helper", "税込"]);
    set_filter(&mut s, |f| f.scope = ScopeFilter::Sheet(0));
    assert_eq!(names(&mut s), vec!["対象月"]);
    set_filter(&mut s, |f| f.scope = ScopeFilter::Workbook);
    assert_eq!(rows(&mut s).len(), 7);
    set_filter(&mut s, |_| {});
    // Sort: the default is the name ascending; pressing the heading: descending, then the order of definition.
    s.update(|e, _, cx| {
        let p = e.sheet_state_mut().unwrap().names.as_mut().unwrap();
        p.sort = p.sort.pressed(SortColumn::Name);
        cx.notify();
    });
    assert_eq!(names(&mut s)[0], "税込");
    s.update(|e, _, _| {
        let p = e.sheet_state_mut().unwrap().names.as_mut().unwrap();
        p.sort = p.sort.pressed(SortColumn::Name);
    });
    assert_eq!(names(&mut s)[0], "売上金額");
    s.screenshot("issue102-sorted");
}

/// Add, edit (rename, change the formula; the scope stays), delete; each is one undo step.
pub(super) fn add_edit_delete() {
    let mut s = with_names();
    let before = all_names(&mut s).len();
    s.update(|e, w, cx| e.names_open_dialog(None, w, cx));
    s.settle();
    assert!(dialog_open(&mut s));
    s.screenshot("issue102-dialog-new");
    // A name like a cell reference, an empty one, a duplicate, an empty formula: not confirmable.
    for (name, formula) in [("A1", "=0.5"), ("", "=0.5"), ("税率", "=0.5"), ("新税率", "")] {
        dialog_set(&mut s, Some(name), formula);
        s.update(|e, w, cx| e.names_confirm_dialog(w, cx));
        assert!(dialog_open(&mut s), "{name:?} {formula:?}");
        assert_eq!(all_names(&mut s).len(), before);
    }
    dialog_set(&mut s, Some("税率"), "=0.5");
    s.screenshot("issue102-dialog-invalid");
    dialog_set(&mut s, Some("新税率"), "=0.08");
    s.update(|e, w, cx| e.names_confirm_dialog(w, cx));
    assert!(!dialog_open(&mut s));
    assert_eq!(all_names(&mut s).len(), before + 1);
    assert!(all_names(&mut s).contains(&("新税率".to_string(), None, "0.08".to_string())));
    assert!(names(&mut s).contains(&"新税率".to_string()));
    s.key("cmd-z");
    assert_eq!(all_names(&mut s).len(), before);
    s.key("cmd-shift-z");
    assert_eq!(all_names(&mut s).len(), before + 1);

    // Edit: rename and change the formula in one step; the scope is the name's own.
    s.update(|e, w, cx| e.names_open_dialog(Some(("対象月".to_string(), Some(0))), w, cx));
    s.settle();
    s.screenshot("issue102-dialog-edit");
    let sheet = s.with(|e, _| e.slide().name.clone().unwrap());
    let q = ibis_calc::formula::quote_sheet(&sheet);
    dialog_set(&mut s, Some("対象期"), &format!("={q}!$B$1"));
    s.update(|e, w, cx| e.names_confirm_dialog(w, cx));
    let got = all_names(&mut s);
    assert!(got.iter().any(|n| n.0 == "対象期" && n.1 == Some(0) && n.2 == format!("{q}!$B$1")), "{got:?}");
    assert!(!got.iter().any(|n| n.0 == "対象月" && n.1 == Some(0)), "{got:?}");
    assert!(got.iter().any(|n| n.0 == "対象月" && n.1.is_none()), "the workbook's own stays: {got:?}");
    s.key("cmd-z");
    assert!(all_names(&mut s).iter().any(|n| n.0 == "対象月" && n.1 == Some(0)), "one undo undoes the rename and the formula");
    assert!(!all_names(&mut s).iter().any(|n| n.0 == "対象期"));
    // Renaming to a name the scope has is refused.
    s.update(|e, w, cx| e.names_open_dialog(Some(("税率".to_string(), None)), w, cx));
    dialog_set(&mut s, Some("合計"), "=0.1");
    s.update(|e, w, cx| e.names_confirm_dialog(w, cx));
    assert!(dialog_open(&mut s));
    // Escape leaves everything as it was.
    let count = all_names(&mut s).len();
    s.key("escape");
    assert!(!dialog_open(&mut s));
    assert_eq!(all_names(&mut s).len(), count);

    // Delete from the list; one undo restores it.
    s.update(|e, w, cx| e.names_delete_row(("税率".to_string(), None), w, cx));
    assert!(!all_names(&mut s).iter().any(|n| n.0 == "税率"));
    s.key("cmd-z");
    assert!(all_names(&mut s).iter().any(|n| n.0 == "税率"));
}

/// A new name's sheet scope can be chosen; the same name in another scope is fine.
pub(super) fn add_in_sheet_scope() {
    let mut s = with_names();
    s.update(|e, w, cx| e.names_open_dialog(None, w, cx));
    s.update(|e, _, _| e.sheet_state_mut().unwrap().names.as_mut().unwrap().dialog.as_mut().unwrap().scope = Some(0));
    dialog_set(&mut s, Some("税率"), "=0.2");
    s.update(|e, w, cx| e.names_confirm_dialog(w, cx));
    assert!(all_names(&mut s).contains(&("税率".to_string(), Some(0), "0.2".to_string())));
    assert!(all_names(&mut s).contains(&("税率".to_string(), None, "0.1".to_string())));
}

/// 一覧を貼り付け: four columns from the active cell, the names shown (filtered), one undo.
pub(super) fn paste_list() {
    let mut s = with_names();
    set_filter(&mut s, |f| f.hide_helpers = true);
    let shown = rows(&mut s);
    s.click_cell("H2");
    s.update(|e, _, cx| e.names_paste_list(cx));
    assert_eq!(s.shown("H2"), "名前");
    assert_eq!(s.shown("I2"), "参照範囲または数式");
    assert_eq!(s.shown("J2"), "スコープ");
    assert_eq!(s.shown("K2"), "値");
    assert_eq!(s.shown("H3"), shown[0].name);
    assert_eq!(s.shown("I3"), format!("={}", shown[0].formula));
    assert_eq!(s.shown("J3"), if shown[0].scope.is_empty() { "ブック".to_string() } else { shown[0].scope.clone() });
    let last = shown.len() + 2;
    assert_eq!(s.shown(&format!("H{last}")), shown.last().unwrap().name);
    assert_eq!(s.shown(&format!("H{}", last + 1)), "");
    // A scoped name shows its sheet.
    let at = shown.iter().position(|r| r.sheet == Some(0)).unwrap() + 3;
    assert_eq!(s.shown(&format!("J{at}")), shown[at - 3].scope);
    s.screenshot("issue102-pasted");
    s.key("cmd-z");
    assert_eq!(s.shown("H2"), "");
    assert_eq!(s.value("K3"), CellValue::Empty);
}

/// The names with errors are deleted together, one undo.
pub(super) fn delete_errors() {
    let mut s = with_names();
    define(&mut s, "旧_担当", None, "#REF!");
    let before = all_names(&mut s).len();
    s.update(|e, _, cx| e.names_delete_errors(cx));
    assert_eq!(all_names(&mut s).len(), before - 2);
    assert!(!all_names(&mut s).iter().any(|n| n.2.contains("#REF!")));
    s.key("cmd-z");
    assert_eq!(all_names(&mut s).len(), before);
}

/// The keys of the list, and a row's click.
pub(super) fn keys_and_clicks() {
    let mut s = with_names();
    let sel = |s: &mut Scenario| s.with(|e, _| e.sheet_state().unwrap().names.as_ref().unwrap().selected.clone());
    let shown = rows(&mut s);
    s.update(|e, w, cx| e.names_select(shown[0].key(), false, w, cx));
    assert_eq!(sel(&mut s), Some(shown[0].key()));
    s.key("down");
    assert_eq!(sel(&mut s), Some(shown[1].key()));
    s.key("end");
    assert_eq!(sel(&mut s), Some(shown.last().unwrap().key()));
    s.key("home");
    assert_eq!(sel(&mut s), Some(shown[0].key()));
    s.key("up");
    assert_eq!(sel(&mut s), Some(shown[0].key()));
    // A click on a range's row goes to the range.
    let range = shown.iter().find(|r| r.name == "売上金額").unwrap().clone();
    s.update(|e, w, cx| e.names_row_clicked(range.key(), 1, w, cx));
    assert_eq!(s.with(|e, _| e.selected_range()).map(|r| (r.first_row, r.first_column, r.last_row, r.last_column)), Some((3, 1, 7, 1)));
    // A double click does not open the dialog (no formula is being typed).
    s.update(|e, w, cx| e.names_row_clicked(range.key(), 2, w, cx));
    assert!(!dialog_open(&mut s));
    // F2 opens it; Escape closes it.
    s.update(|e, w, cx| e.names_select(range.key(), false, w, cx));
    s.key("f2");
    assert!(dialog_open(&mut s));
    s.key("escape");
    assert!(!dialog_open(&mut s));
    // Delete deletes the selected one.
    let count = all_names(&mut s).len();
    s.key("delete");
    assert_eq!(all_names(&mut s).len(), count - 1);
    s.key("cmd-z");
    assert_eq!(all_names(&mut s).len(), count);
    s.update(|e, w, cx| e.names_select(shown[0].key(), false, w, cx));
    s.type_text("税");
    assert!(sel(&mut s).is_some_and(|k| k.0.starts_with('税')));
}

/// While a formula is being typed, a double click or Enter puts the name into it.
pub(super) fn insert_into_formula() {
    let mut s = with_names();
    s.click_cell("G10");
    s.type_text("=SUM(");
    let key = ("売上金額".to_string(), None);
    s.update(|e, w, cx| e.names_row_clicked(key.clone(), 2, w, cx));
    let text = s.with(|e, _| e.cell_edit().map(|c| c.text.clone()));
    assert_eq!(text.as_deref(), Some("=SUM(売上金額"), "{text:?}");
    s.type_text(")");
    s.key("enter");
    assert_eq!(s.formula("G10").as_deref(), Some("SUM(売上金額)"));
    // Enter on the selected row does the same.
    s.click_cell("G11");
    s.type_text("=");
    s.update(|e, w, cx| e.names_select(key.clone(), false, w, cx));
    s.key("enter");
    let text = s.with(|e, _| e.cell_edit().map(|c| c.text.clone()));
    assert_eq!(text.as_deref(), Some("=売上金額"), "{text:?}");
}

/// The right pane is dragged wider at its left edge; the list then has the value and scope columns.
pub(super) fn pane_width() {
    let mut s = with_names();
    assert_eq!(s.with(|e, _| e.sidebar_width), 300.);
    s.screenshot("issue102-pane-300");
    let (width, height) = (1400., 800.);
    s.size(width, height);
    s.settle();
    // The handle is the pane's left edge: drag it 200 px to the left.
    let x = width - 300.;
    s.drag((x, 400.), (x - 200., 400.));
    s.settle();
    let w = s.with(|e, _| e.sidebar_width);
    assert_eq!(w, 500., "dragged by 200");
    s.screenshot("issue102-pane-500");
    s.drag((width - 500., 400.), (0., 400.));
    assert_eq!(s.with(|e, _| e.sidebar_width), 720., "no wider than 720");
    s.drag((width - 720., 400.), (width, 400.));
    assert_eq!(s.with(|e, _| e.sidebar_width), 300., "no narrower than 300");
}
