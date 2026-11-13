// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-03: the scope of a name is changed in the edit dialog (and the formulas that use the name follow), the
//! conditional formatting manager as a table (stop check boxes with a heading check box, rows dragged by their grip,
//! the rule editor as a child dialog over it, no up/down buttons), and the grip of the sort levels' table.

use super::SheetSteps;
use crate::shell::scenario::Scenario;
use crate::shell::scenario::button_probe as buttons;
use gpui_kit::px;
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand};
use crate::ext::*;

const BOOK: &str = "test/editing/xlsx-sales.xlsx";
/// The name used in the user's workbook.
const TAX: &str = "税率"; // i18n-ok（ユーザー文書の名前）

fn names(s: &mut Scenario) -> Vec<(String, Option<usize>, String)> {
    s.with(|e, _| e.presentation().workbook.as_ref().unwrap().names.iter().filter(|n| !n.name.starts_with("_xl")).map(|n| (n.name.clone(), n.sheet, n.formula.clone())).collect())
}

/// A formula in `row`, column A of the workbook's sheet number `sheet`.
fn put_formula(s: &mut Scenario, sheet: usize, row: u32, formula: &str) {
    s.update(|e, _, _| {
        let slide = e.presentation().workbook.as_ref().unwrap().sheets[sheet].slide.unwrap();
        e.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![CellEdit { row, column: 0, input: CellInput::Formula(formula.into()) }] })]).unwrap();
    });
}

fn formula_of(s: &mut Scenario, sheet: usize, row: u32) -> (Option<String>, CellValue) {
    s.with(|e, _| {
        let slide = e.presentation().workbook.as_ref().unwrap().sheets[sheet].slide.unwrap();
        let sh = crate::shell::editor::sheet_of(e.presentation().slides.iter().find(|x| x.id == slide).unwrap()).unwrap().1;
        let c = sh.cell(row, 0).unwrap();
        (c.formula.clone(), c.value.clone())
    })
}

fn set_scope(s: &mut Scenario, scope: Option<usize>) {
    s.update(|e, _, cx| {
        e.sheet_state_mut().unwrap().names.as_mut().unwrap().dialog.as_mut().unwrap().scope = scope;
        cx.notify();
    });
    s.settle();
}

fn dialog_scope(s: &mut Scenario) -> Option<usize> {
    s.with(|e, _| e.sheet_state().unwrap().names.as_ref().unwrap().dialog.as_ref().unwrap().scope)
}

fn dialog_open(s: &mut Scenario) -> bool {
    s.with(|e, _| e.sheet_state().unwrap().names.as_ref().unwrap().dialog.is_some())
}

fn confirm(s: &mut Scenario) {
    s.update(|e, w, cx| e.names_confirm_dialog(w, cx));
    s.settle();
}

fn open_edit(s: &mut Scenario, name: &str, scope: Option<usize>) {
    s.update(|e, w, cx| e.names_open_dialog(Some((name.to_lowercase(), scope)), w, cx));
    s.settle();
}

/// The edit dialog's scope is a drop-down: from the workbook to a sheet and back, the formulas that use the name keep
/// reaching it, a scope that has the name already is refused, and each change is one undo step.
pub(super) fn names_scope_is_changed_in_the_edit_dialog() {
    let mut s = Scenario::open(BOOK);
    s.size(1400., 800.);
    let sheet0 = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets[0].name.clone());
    let q0 = ibis_calc::formula::quote_sheet(&sheet0);
    s.update(|e, _, _| {
        e.document.execute(vec![Command::Sheet(SheetCommand::SetName { name: TAX.into(), sheet: None, formula: Some("0.1".into()) })]).unwrap();
    });
    put_formula(&mut s, 0, 20, &format!("{TAX}*2"));
    put_formula(&mut s, 1, 20, &format!("{TAX}*3"));
    assert_eq!(formula_of(&mut s, 0, 20).1, CellValue::Number(0.2));
    assert_eq!(formula_of(&mut s, 1, 20).1, CellValue::Number(0.30000000000000004));
    s.update(|e, w, cx| e.open_names(w, cx));
    s.settle();

    // The dialog of an existing name shows the scope as a drop-down, changeable.
    open_edit(&mut s, TAX, None);
    assert_eq!(dialog_scope(&mut s), None);
    s.screenshot("names-scope-edit-dialog");
    set_scope(&mut s, Some(0));
    s.screenshot("names-scope-edit-dialog-sheet");

    // Workbook -> the first sheet: the name is the sheet's; the other sheet's use is written with the sheet.
    confirm(&mut s);
    assert!(!dialog_open(&mut s));
    assert_eq!(names(&mut s), vec![(TAX.to_string(), Some(0), "0.1".to_string())]);
    assert_eq!(formula_of(&mut s, 0, 20), (Some(format!("{TAX}*2")), CellValue::Number(0.2)));
    assert_eq!(formula_of(&mut s, 1, 20).0, Some(format!("{q0}!{TAX}*3")));
    assert_eq!(formula_of(&mut s, 1, 20).1, CellValue::Number(0.30000000000000004), "still reaches the name");
    s.screenshot("names-scope-after-move");

    // One undo step takes both back.
    s.key("cmd-z");
    assert_eq!(names(&mut s), vec![(TAX.to_string(), None, "0.1".to_string())]);
    assert_eq!(formula_of(&mut s, 1, 20).0, Some(format!("{TAX}*3")));
    s.key("cmd-shift-z");
    assert_eq!(names(&mut s)[0].1, Some(0));

    // The first sheet -> the workbook: the sheet-qualified use becomes the plain name.
    open_edit(&mut s, TAX, Some(0));
    assert_eq!(dialog_scope(&mut s), Some(0));
    set_scope(&mut s, None);
    confirm(&mut s);
    assert_eq!(names(&mut s), vec![(TAX.to_string(), None, "0.1".to_string())]);
    assert_eq!(formula_of(&mut s, 1, 20).0, Some(format!("{TAX}*3")));
    assert_eq!(formula_of(&mut s, 0, 20).1, CellValue::Number(0.2));

    // The scope that has the name already: refused, nothing changes.
    s.update(|e, _, _| {
        e.document.execute(vec![Command::Sheet(SheetCommand::SetName { name: TAX.into(), sheet: Some(2), formula: Some("0.5".into()) })]).unwrap();
    });
    let before = names(&mut s);
    open_edit(&mut s, TAX, None);
    set_scope(&mut s, Some(2));
    confirm(&mut s);
    assert!(dialog_open(&mut s), "refused: the sheet has the name");
    assert_eq!(names(&mut s), before);
    s.screenshot("names-scope-duplicate");
    // Back to its own scope, then another sheet: fine.
    set_scope(&mut s, Some(1));
    confirm(&mut s);
    assert!(!dialog_open(&mut s));
    assert!(names(&mut s).iter().any(|n| n.0 == TAX && n.1 == Some(1)));
    assert!(!names(&mut s).iter().any(|n| n.0 == TAX && n.1.is_none()));

    // 名前の定義…: the list and the new-name dialog (with the selected range as its reference); 名前の管理: the list only.
    s.click_cell("B4");
    s.menu("xlsx.name-manager");
    assert!(!dialog_open(&mut s));
    s.menu("xlsx.define-name");
    assert!(dialog_open(&mut s));
    assert!(s.with(|e, _| e.sheet_state().unwrap().names.as_ref().unwrap().dialog.as_ref().unwrap().editing.is_none()));
    let reference = s.with(|e, cx| e.sheet_state().unwrap().names.as_ref().unwrap().dialog.as_ref().unwrap().formula.read(cx).value().to_string());
    assert!(reference.contains("$B$4"), "{reference}");
    s.screenshot("define-name-dialog");
}

fn rule(s: &mut Scenario, formula: &str, fill: &str) {
    s.menu("xlsx.conditional");
    super::cf_field(s, "formula", formula);
    super::cf_field(s, "fill", fill);
    super::cf_click(s, "apply");
}

fn model(s: &mut Scenario) -> Vec<ibis_doc_model::ConditionalFormat> {
    s.with(|e, _| crate::shell::editor::sheet_of(e.slide()).unwrap().1.conditional.clone())
}

/// The formulas of the rules in the order of evaluation.
fn order(s: &mut Scenario) -> Vec<String> {
    let mut all: Vec<(u32, String)> = model(s)
        .iter()
        .flat_map(|f| f.rules.iter().map(|r| (r.priority, match &r.kind { ibis_doc_model::CfKind::Expression(t) => t.clone(), other => format!("{other:?}") })))
        .collect();
    all.sort();
    all.into_iter().map(|a| a.1).collect()
}

fn stops(s: &mut Scenario) -> Vec<bool> {
    let mut all: Vec<(u32, bool)> = model(s).iter().flat_map(|f| f.rules.iter().map(|r| (r.priority, r.stop))).collect();
    all.sort();
    all.into_iter().map(|a| a.1).collect()
}

fn manager_open(s: &mut Scenario) -> bool {
    s.with(|e, _| e.wb().conditional_edit.as_ref().is_some_and(|p| p.manager))
}

fn click_key(s: &mut Scenario, key: &str) {
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    buttons::click(s, key);
}

/// 条件付き書式ルールの管理: the table (the stop mark at the end of the row, display only), rows dragged by their
/// grips (no up/down buttons), the rule editor as a child dialog with the manager open behind it.
pub(super) fn conditional_manager_table_dnd_stop_all_and_child_dialog() {
    let mut s = super::new_sheet();
    s.size(1400., 1000.);
    s.enter("15", "enter");
    s.click_cell("A1");
    rule(&mut s, "A1>10", "FFC7CE");
    rule(&mut s, "A1>0", "C6EFCE");
    rule(&mut s, "A1<100", "FFEB9C");
    assert_eq!(order(&mut s), vec!["A1>10", "A1>0", "A1<100"]);

    s.menu("xlsx.conditional");
    assert!(manager_open(&mut s));
    s.size(1400., 1000.);
    s.settle();
    let table = crate::conditional_edit::manager::RULES_TABLE;
    {
        let _probe = buttons::Probe::new();
        s.update(|_, _, cx| cx.notify());
        let keys = buttons::all();
        // No up/down buttons any more; the heading check box, a stop check box and the edit button per row exist.
        assert!(!keys.iter().any(|k| k.contains("conditional-manager-") && !k.ends_with("-close")), "{keys:?}");
        assert!(!keys.iter().any(|k| k.contains("stop-all")), "no heading check box: {keys:?}");
        assert_eq!(keys.iter().filter(|k| k.starts_with(&format!("{table}-conditional-edit-"))).count(), 3, "{keys:?}");
        // The radio buttons and 追加 are one line: the add button is at the right, level with the radio buttons.
        let radio = buttons::bounds("conditional-filter-sheet").or_else(|| s.with(|e, _| e.wb().conditional_edit.as_ref().unwrap().ui.borrow().get("filter-sheet").copied())).unwrap();
        let add = buttons::bounds("conditional-add").expect("the add button");
        assert!((f32::from(radio.center().y) - f32::from(add.center().y)).abs() < 4., "{radio:?} {add:?}");
        assert!(add.origin.x > radio.right(), "the add button is to the right");
        let dialog = s.with(|e, _| e.dialog.bounds.unwrap());
        assert!(add.right() > dialog.right() - px(40.), "right aligned: {add:?} in {dialog:?}");
    }
    s.screenshot("conditional-manager-table");

    // The stop mark at the end of the row is only shown: a click on it changes nothing.
    let mark_click = |s: &mut Scenario, row: usize| {
        let dnd = s.with(|e, _| e.wb().conditional_edit.as_ref().unwrap().manager_table.dnd.clone());
        let b = dnd.row_bounds(table, row).expect("the row");
        // Left of the three row buttons and the grip: 4 (padding) + 24 (grip) + 3 * 28 (buttons) + half the 32 of the column.
        s.click((f32::from(b.right() - px(128.)), f32::from(b.center().y)));
    };
    mark_click(&mut s, 1);
    assert_eq!(stops(&mut s), vec![false, false, false]);
    assert!(manager_open(&mut s));
    // It is set in the rule editor (the check box there), and the manager then shows it.
    super::cf_click(&mut s, "rule-stop-1-0");
    assert_eq!(stops(&mut s), vec![false, true, false]);
    s.menu("xlsx.conditional");
    s.size(1400., 1000.);
    s.settle();
    s.screenshot("conditional-manager-stop-shown");
    mark_click(&mut s, 1);
    assert_eq!(stops(&mut s), vec![false, true, false], "a click does not switch it off");

    // Drag the first row to below the last: one move, one undo step.
    super::cf_drag_rule(&mut s, 0, 2, true);
    assert_eq!(order(&mut s), vec!["A1>0", "A1<100", "A1>10"]);
    s.screenshot("conditional-manager-dragged");
    // Back up to the top (onto the upper half of the first row).
    super::cf_drag_rule(&mut s, 2, 0, false);
    assert_eq!(order(&mut s), vec!["A1>10", "A1>0", "A1<100"]);
    super::cf_drag_rule(&mut s, 1, 1, true);
    assert_eq!(order(&mut s), vec!["A1>10", "A1>0", "A1<100"], "dropped on itself: nothing moves");

    // 編集: the rule editor is a child dialog; the manager stays open behind it.
    super::cf_click(&mut s, "edit-1");
    assert!(s.with(|e, _| e.dialog_is_modal()));
    assert!(s.with(|e, _| e.dialog.parent.is_some() && e.wb().conditional_edit.as_ref().is_some_and(|p| !p.manager && p.parent.is_some())));
    assert_eq!(s.with(|e, _| e.dialog.key), Some("conditional_rule_editor"));
    s.settle();
    {
        let _probe = buttons::Probe::new();
        s.update(|_, _, cx| cx.notify());
        let keys = buttons::all();
        assert!(keys.iter().any(|k| k.contains("conditional-delete-0")), "the manager is drawn behind the editor: {keys:?}");
    }
    s.screenshot("conditional-manager-child-dialog");
    // The parent takes no clicks while the child is open.
    let parent_close = {
        let _probe = buttons::Probe::new();
        s.update(|_, _, cx| cx.notify());
        buttons::bounds("conditional-manager-close")
    };
    if let Some(b) = parent_close {
        let editor_bounds = s.with(|e, _| e.dialog.bounds.unwrap());
        if !editor_bounds.contains(&b.center()) {
            s.click((f32::from(b.center().x), f32::from(b.center().y)));
            assert!(s.with(|e, _| e.dialog.key) == Some("conditional_rule_editor"), "the manager's close button does not reach through the editor");
        }
    }
    // Cancel: back to the manager, the same table.
    click_key(&mut s, "dialog-cancel");
    assert!(manager_open(&mut s), "the manager is back");
    assert!(s.with(|e, _| e.dialog.parent.is_none()));
    assert_eq!(s.with(|e, _| e.dialog.key), Some("conditional_panel"));
    // Edit and apply: back to the manager too, with the change.
    super::cf_row_click(&mut s, "conditional-edit", 1, 0);
    assert!(!manager_open(&mut s));
    super::cf_field(&mut s, "formula", "A1>5");
    super::cf_click_editor(&mut s, "apply");
    assert!(manager_open(&mut s), "after OK the manager is open again");
    assert_eq!(order(&mut s), vec!["A1>10", "A1>5", "A1<100"]);
    // ルールを追加 opens the editor over the manager too; Escape cancels it.
    click_key(&mut s, "conditional-add");
    assert_eq!(s.with(|e, _| e.dialog.key), Some("conditional_rule_editor"));
    s.key("escape");
    assert!(manager_open(&mut s), "Escape closes the child only");
    assert_eq!(order(&mut s).len(), 3);
    // Delete a row and duplicate one with the row buttons.
    super::cf_click(&mut s, "delete-2-0");
    assert_eq!(order(&mut s), vec!["A1>10", "A1>5"]);
    super::cf_click(&mut s, "duplicate-0-0");
    assert_eq!(order(&mut s).len(), 3);
    s.screenshot("conditional-manager-after");
    click_key(&mut s, "conditional-manager-close");
    assert!(!manager_open(&mut s));
}

/// The sort levels' table ends each row in the three-line grip (`drag_handle`), and the grip is a bundled icon.
pub(super) fn sort_levels_grip_is_drawn() {
    use gpui_kit::AssetSource;
    let assets = crate::shell::icons::Assets::new("");
    let svg = assets.load("icons/material/drag_handle.svg").expect("loading").expect("the drag_handle icon is bundled");
    assert!(svg.len() > 20);
    let mut s = super::new_sheet();
    s.size(1400., 900.);
    for (row, values) in [["name", "score"], ["a", "1"], ["b", "5"], ["c", "3"]].into_iter().enumerate() {
        for (col, value) in values.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(col as u32, row as u32));
            s.enter(value, "enter");
        }
    }
    s.click_cell("A1");
    s.menu("xlsx.sort-manage");
    super::sort_field(&mut s, "range", "A1:B4");
    super::sort_click(&mut s, "pick-0");
    super::sort_click(&mut s, "pick-0-B");
    super::sort_click(&mut s, "add");
    super::sort_field(&mut s, "column-1", "A");
    super::sort_click(&mut s, "range");
    s.settle();
    s.screenshot("sort-levels-grip");
    assert_eq!(s.with(|e, _| e.wb().sort_edit.as_ref().unwrap().levels.len()), 2);
}
