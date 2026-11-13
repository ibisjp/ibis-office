// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The pivot table's field pane as two tables (the fields above, the layout below as group rows) and the value
//! field dialog. Every move is a real pointer event on the rows the tables painted.

use super::SheetSteps;
use crate::shell::scenario::Scenario;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, PlatformInput, point, px};
use ibis_doc_model::{CellValue, PivotAxis};
use crate::ext::*;

const BOOK: &str = "test/editing/xlsx-pivot-not-refreshed.xlsx";
const FIELDS: &str = "pivot-fields-table";
const LAYOUT: &str = "pivot-layout-table";

fn new_pivot() -> Scenario {
    let mut s = Scenario::open(BOOK);
    s.size(1400., 800.);
    s.click_cell("K5");
    s.menu("xlsx.pivot-insert");
    s.update(|e, window, cx| e.create_pivot(window, cx));
    s.settle();
    s
}

fn pivot(s: &mut Scenario) -> ibis_doc_model::PivotTable {
    s.with(|e, _| e.current_sheet().and_then(|(_, sh)| sh.pivots.first().cloned())).expect("the pivot table")
}

fn field(s: &mut Scenario, name: &str) -> usize {
    pivot(s).fields.iter().position(|f| f.name == name).unwrap_or_else(|| panic!("no field {name}"))
}

fn dnd(s: &mut Scenario) -> crate::shell::chrome::row_dnd::RowDnd {
    s.with(|e, _| e.wb().panels.pivot.as_ref().unwrap().ui.as_ref().unwrap().dnd.clone())
}

/// The middle of row `index` of the table as painted (the rows that take drops).
fn row_at(s: &mut Scenario, table: &'static str, index: usize) -> (f32, f32) {
    let b = dnd(s).row_bounds(table, index).unwrap_or_else(|| panic!("row {index} of {table} not painted"));
    (f32::from(b.origin.x + b.size.width / 2.), f32::from(b.origin.y + b.size.height / 2.))
}

fn search(s: &mut Scenario, query: &str) {
    s.update(|e, window, cx| {
        let input = e.wb().panels.pivot.as_ref().unwrap().ui.as_ref().unwrap().search.clone();
        input.update(cx, |i, cx| i.set_value(query.to_string(), window, cx));
    });
    s.settle();
}

fn drag(s: &mut Scenario, from: (f32, f32), to: (f32, f32)) {
    s.drag_through(&[from, (from.0 + 6., from.1 + 6.), ((from.0 + to.0) / 2., (from.1 + to.1) / 2.), to, (to.0, to.1 + 0.5)]);
    s.settle();
}

/// The index of an area's group row in the layout table: each area is a group row and its fields (or one placeholder
/// row when it has none).
fn group_row(s: &mut Scenario, area: usize) -> usize {
    let p = pivot(s);
    let counts = [p.page_fields.len(), p.col_fields.len(), p.row_fields.len(), p.data_fields.len()];
    (0..area).map(|a| 1 + counts[a].max(1)).sum()
}

/// Search, tick a field into the rows, drag a field to the columns, drag a placed field to another area and out.
pub(super) fn search_add_and_drag() {
    let mut s = new_pivot();
    let (region, amount, quarter) = (field(&mut s, "地域"), field(&mut s, "金額"), field(&mut s, "四半期")); // i18n-ok（ユーザー文書の項目名と、表示される名前の照合）
    s.screenshot("pivot-fields-empty");
    let total = pivot(&mut s).fields.len();

    // The search narrows the rows to the ones that match.
    search(&mut s, "金"); // i18n-ok（ユーザー文書の項目名と、表示される名前の照合）
    let built = s.with(|e, _| e.wb().panels.pivot.as_ref().unwrap().ui.as_ref().unwrap().fields_table.last_built());
    assert!(built.end >= 1 && built.end < total, "narrowed: {built:?} of {total}");
    s.screenshot("pivot-fields-search");
    search(&mut s, "");

    // The check box places a field in the rows.
    let y = row_at(&mut s, FIELDS, region).1;
    let b = dnd(&mut s).row_bounds(FIELDS, region).unwrap();
    s.click((f32::from(b.origin.x) + 18., y));
    assert_eq!(pivot(&mut s).row_fields, vec![region as i32], "ticked into the rows");
    assert_eq!(pivot(&mut s).place_of(region), Some((PivotAxis::Row, 0)));
    s.screenshot("pivot-fields-row-added");

    // A field dragged onto the 列 group row goes to the columns.
    let from = row_at(&mut s, FIELDS, quarter);
    let col_group = group_row(&mut s, 1);
    let to = row_at(&mut s, LAYOUT, col_group);
    drag(&mut s, (from.0 + 20., from.1), to);
    assert_eq!(pivot(&mut s).col_fields, vec![quarter as i32], "dropped on the column group");
    s.screenshot("pivot-fields-dragged-to-columns");

    // A placed field dragged from the layout onto the フィルター group row becomes a report filter.
    let row_item = group_row(&mut s, 2) + 1;
    let from = row_at(&mut s, LAYOUT, row_item);
    let to = row_at(&mut s, LAYOUT, 0);
    drag(&mut s, from, to);
    assert_eq!(pivot(&mut s).page_fields.len(), 1, "moved to the filters");
    assert!(pivot(&mut s).row_fields.is_empty());

    // Dragged onto the list of fields, it leaves the table.
    let from = row_at(&mut s, LAYOUT, 1);
    let to = row_at(&mut s, FIELDS, 3);
    drag(&mut s, from, to);
    assert!(pivot(&mut s).page_fields.is_empty(), "taken out of the layout");

    // Escape gives up a drag: nothing changes.
    let before = pivot(&mut s).col_fields.clone();
    let from = row_at(&mut s, FIELDS, amount);
    let to = row_at(&mut s, LAYOUT, 0);
    let at = |p: (f32, f32)| point(px(p.0), px(p.1));
    s.dispatch(vec![
        PlatformInput::MouseMove(MouseMoveEvent { position: at(from), ..Default::default() }),
        PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at(from), click_count: 1, ..Default::default() }),
        PlatformInput::MouseMove(MouseMoveEvent { position: at(to), pressed_button: Some(MouseButton::Left), ..Default::default() }),
    ]);
    s.settle();
    assert!(dnd(&mut s).dragging().is_some(), "a drag in progress");
    s.screenshot("pivot-fields-dragging");
    s.key("escape");
    assert!(dnd(&mut s).dragging().is_none(), "Escape gave it up");
    assert_eq!(pivot(&mut s).col_fields, before);
    assert!(pivot(&mut s).data_fields.is_empty());
}

/// The value field's dialog: the summary changed on the 集計方法 tab, the calculation on the other, OK applies, キャンセル does not.
pub(super) fn value_dialog_changes_summary() {
    let mut s = new_pivot();
    let (region, amount) = (field(&mut s, "地域"), field(&mut s, "金額")); // i18n-ok（ユーザー文書の項目名と、表示される名前の照合）
    s.update(move |e, _, cx| {
        let t = e.wb().panels.pivot.as_ref().unwrap().target.unwrap();
        e.toggle_pivot_field(t, region, cx);
        e.toggle_pivot_field(t, amount, cx);
    });
    s.settle();
    assert_eq!(pivot(&mut s).data_fields.len(), 1);
    assert_eq!(s.value("B3"), CellValue::Text("合計 / 金額".into())); // i18n-ok（ユーザー文書の項目名と、表示される名前の照合）
    s.screenshot("pivot-fields-placed");

    // Double-clicking the value row opens the dialog over the pane.
    let item = group_row(&mut s, 3) + 1;
    let at = row_at(&mut s, LAYOUT, item);
    s.double_click(at);
    s.settle();
    assert_eq!(s.with(|e, _| e.wb().panels.pivot.as_ref().and_then(|p| p.value)), Some(0), "the dialog is open");
    assert!(s.with(|e, _| e.dialog_is_modal() && e.dialog.key == Some("pivot_value_dialog")));
    s.screenshot("pivot-value-dialog-summarize");

    // Cancel changes nothing.
    s.update(|e, _, _| e.set_draft(|d| d.function = "max".into()));
    s.update(|e, w, cx| e.close_dialog(w, cx));
    assert_eq!(pivot(&mut s).data_fields[0].function, "sum");
    assert!(s.with(|e, _| !e.dialog_is_modal()), "the pane stays, the dialog is gone");

    // The dialog again; 平均 and a ratio, then OK.
    s.update(|e, w, cx| e.open_pivot_value_dialog(0, w, cx));
    s.update(|e, _, _| e.set_draft(|d| { d.function = "average".into(); d.number_format = Some("#,##0.00".into()); }));
    s.settle();
    s.screenshot("pivot-value-dialog-summarize-average");
    s.update(|e, _, cx| {
        if let Some(u) = e.wb_mut().panels.pivot.as_mut().and_then(|p| p.ui.as_mut()) { u.value_tab = 1; }
        e.set_draft(|d| d.show_as = Some("percentOfTotal".into()));
        cx.notify();
    });
    s.settle();
    s.screenshot("pivot-value-dialog-show-as");
    s.update(|e, w, cx| e.pivot_value_dialog_ok(w, cx));
    s.settle();
    let df = pivot(&mut s).data_fields[0].clone();
    assert_eq!((df.function.as_str(), df.show_as.as_deref(), df.number_format.as_deref()), ("average", Some("percentOfTotal"), Some("#,##0.00")));
    assert_eq!(s.value("B3"), CellValue::Text("平均 / 金額".into()), "the name followed the summary"); // i18n-ok（ユーザー文書の項目名と、表示される名前の照合）
    s.screenshot("pivot-value-dialog-applied");
    s.key("cmd-z");
    assert_eq!(pivot(&mut s).data_fields[0].function, "sum", "one step to undo");
}
