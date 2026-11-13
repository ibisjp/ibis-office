// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #81: the items of a row header's right-click menu are chosen by a real pointer click
//! (not by calling their handlers), and each one acts on the row.
use super::SheetSteps;
use crate::shell::scenario::button_probe as buttons;
use crate::ext::*;

/// Right-click row header `row` (e.g. "3"), then click the menu item `id` with the pointer.
fn choose(s: &mut crate::shell::scenario::Scenario, row: &str, id: &str) {
    let at = s.header(row);
    s.right_click(at);
    assert!(s.with(|e, _| e.sheet_state().unwrap().header_popup.is_some()), "the menu opens");
    let bounds = buttons::bounds(&format!("menu-item-{id}")).unwrap_or_else(|| panic!("{id} is not in the opened menu: {:?}", buttons::all()));
    s.click((f32::from(bounds.center().x), f32::from(bounds.center().y)));
}

pub(crate) fn row_menu_items_act() {
    // A tall window shows the whole menu; a short one cuts it with the scroll rows (▲ ▼).
    for height in [900., 520.] {
        row_menu_items_act_in(height);
    }
}

fn row_menu_items_act_in(height: f32) {
    let _probe = buttons::Probe::new();
    let mut s = super::new_sheet();
    s.size(1200., height);
    for (cell, text) in [("A1", "one"), ("A2", "two"), ("A3", "three"), ("A4", "four")] {
        s.click_cell(cell);
        s.enter(text, "enter");
    }
    let a = |s: &mut crate::shell::scenario::Scenario, cell: &str| s.shown(cell);

    choose(&mut s, "2", "xlsx.header-insert-above");
    assert_eq!((a(&mut s, "A1"), a(&mut s, "A2"), a(&mut s, "A3")), ("one".into(), "".into(), "two".into()), "insert row above");
    choose(&mut s, "3", "xlsx.header-insert-below");
    assert_eq!((a(&mut s, "A3"), a(&mut s, "A4"), a(&mut s, "A5")), ("two".into(), "".into(), "three".into()), "insert row below");
    choose(&mut s, "5", "xlsx.header-clear");
    assert_eq!(a(&mut s, "A5"), "", "clear contents");
    choose(&mut s, "3", "xlsx.header-delete-rows");
    assert_eq!((a(&mut s, "A3"), a(&mut s, "A4")), ("".into(), "".into()), "delete row");
}
