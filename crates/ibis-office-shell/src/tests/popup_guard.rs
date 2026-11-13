// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The press that closes a menu or a dropdown list only closes it (nothing under the pointer gets it), and Escape in a modal
//! dialog: it closes the dialog unless a drag, an open dropdown list or the attribute `esc_closes(false)` takes it.
use super::{Scenario, button_probe as buttons};
use crate::{chrome, dialog::DialogMode, editor::SlideEditor};
use gpui_kit::*;
use gpui_kit::component::v_flex;

thread_local! {
    static COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static CHOSEN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static ESC_CLOSES: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

pub fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::popup_outside_press_only_closes", outside_press_only_closes),
        ("shell::popup_menu_bar_outside_press_only_closes", menu_bar_outside_press_only_closes),
        ("shell::dialog_escape_closes_modal", escape_closes_modal),
        ("shell::dialog_escape_attribute_off", escape_attribute_off),
        ("shell::dialog_escape_dropdown_first", escape_dropdown_first),
    ]
}

/// A count button ABOVE a dropdown, so the list (below the dropdown) never covers it.
fn panel(_: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>) -> Option<chrome::SidebarPanel> {
    let items = ["Alpha", "Beta", "Gamma"].into_iter().map(|label| chrome::DropdownItem::new(label).on_select(std::rc::Rc::new(|_, _, cx| { CHOSEN.with(|c| c.set(c.get() + 1)); cx.notify(); }))).collect();
    let body = v_flex().gap_2()
        .child(chrome::action_button("guard-count").label("Count").on_click(cx.listener(|_, _, _, cx| { COUNT.with(|c| c.set(c.get() + 1)); cx.notify(); })))
        .child(chrome::Dropdown::new("guard-dropdown", "Choose", items, &cx.weak_entity()))
        .into_any_element();
    let panel = chrome::SidebarPanel { title: "Popup guard".into(), body };
    Some(if ESC_CLOSES.with(|e| e.get()) { panel } else { panel.esc_closes(false) })
}
struct Cleanup;
impl Drop for Cleanup { fn drop(&mut self) { chrome::FORCED_SIDEBAR.with(|s| s.set(None)); ESC_CLOSES.with(|e| e.set(true)); } }

fn open(mode: DialogMode) -> (Scenario, Cleanup) {
    COUNT.with(|c| c.set(0));
    CHOSEN.with(|c| c.set(0));
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1200., 800.);
    chrome::FORCED_SIDEBAR.with(|c| c.set(Some((panel, "popup-guard", mode))));
    s.update(|e, _, cx| { e.sidebar_open = true; cx.notify(); });
    s.settle();
    (s, Cleanup)
}
fn center(id: &str) -> (f32, f32) {
    let b = buttons::bounds(id).unwrap_or_else(|| panic!("{id} not rendered: {:?}", buttons::all()));
    (f32::from(b.center().x), f32::from(b.center().y))
}
fn count() -> usize { COUNT.with(|c| c.get()) }

/// A press outside the open dropdown list closes the list and reaches nothing under the pointer, whatever the button or the count.
fn outside_press_only_closes() {
    let _probe = buttons::Probe::new();
    let (mut s, _cleanup) = open(DialogMode::Sidebar);
    // Without a list open, the count button works.
    s.click(center("guard-count"));
    assert_eq!(count(), 1);
    for how in ["click", "double click", "right click"] {
        buttons::click(&mut s, "guard-dropdown");
        assert!(s.with(|e, _| e.toolbar_search_open.is_some()), "{how}: the list is open");
        let at = center("guard-count");
        let before = count();
        match how { "click" => s.click(at), "double click" => s.double_click(at), _ => s.right_click(at) }
        s.settle();
        assert!(s.with(|e, _| e.toolbar_search_open.is_none()), "{how}: the press outside closes the list");
        assert_eq!(count(), before, "{how}: the press that closes the list is not delivered to the button under it");
    }
    // The next press is an ordinary one again.
    let before = count();
    s.click(center("guard-count"));
    assert_eq!(count(), before + 1, "a press with no list open reaches the button");
    // A press inside the list chooses an item (and closes the list) as before.
    buttons::click(&mut s, "guard-dropdown");
    let row = buttons::all().into_iter().find(|id| id.contains("searchable-item-guard-dropdown") && id.ends_with(", 1)")).unwrap();
    s.click(center(&row));
    assert_eq!(CHOSEN.with(|c| c.get()), 1, "a press inside the list reaches the item");
    assert!(s.with(|e, _| e.toolbar_search_open.is_none()), "a press on an item closes the list");
}

/// The same for the menu bar's menu.
fn menu_bar_outside_press_only_closes() {
    let _probe = buttons::Probe::new();
    let (mut s, _cleanup) = open(DialogMode::Sidebar);
    s.update(|e, w, cx| e.open_menu_at(0, w, cx));
    s.settle();
    assert!(s.with(|e, _| e.open_menu.is_some()));
    let before = count();
    s.click(center("guard-count"));
    s.settle();
    assert!(s.with(|e, _| e.open_menu.is_none()), "the press outside closes the menu");
    assert_eq!(count(), before, "the press that closes the menu is not delivered to the button under it");
    s.click(center("guard-count"));
    assert_eq!(count(), before + 1);
}

/// Escape closes a modal dialog by default.
fn escape_closes_modal() {
    let _probe = buttons::Probe::new();
    let (mut s, _cleanup) = open(DialogMode::Modal);
    assert!(s.with(|e, _| e.dialog_is_modal()));
    s.key("escape");
    assert!(!s.with(|e, _| e.dialog_is_modal() || e.dialog.active), "Escape closes the modal dialog");
}

/// The attribute `esc_closes(false)` keeps the dialog open.
fn escape_attribute_off() {
    let _probe = buttons::Probe::new();
    ESC_CLOSES.with(|e| e.set(false));
    let (mut s, _cleanup) = open(DialogMode::Modal);
    assert!(s.with(|e, _| e.dialog_is_modal()));
    s.key("escape");
    assert!(s.with(|e, _| e.dialog_is_modal()), "esc_closes(false): Escape does not close the dialog");
    ESC_CLOSES.with(|e| e.set(true));
    s.update(|_, _, cx| cx.notify());
    s.settle();
    s.key("escape");
    assert!(!s.with(|e, _| e.dialog_is_modal()), "the default closes it again");
}

/// An open dropdown list takes Escape; the second Escape closes the dialog. A drag in progress takes it too.
fn escape_dropdown_first() {
    let _probe = buttons::Probe::new();
    let (mut s, _cleanup) = open(DialogMode::Modal);
    buttons::click(&mut s, "guard-dropdown");
    assert!(s.with(|e, _| e.toolbar_search_open.is_some()));
    s.key("escape");
    assert!(s.with(|e, _| e.toolbar_search_open.is_none() && e.dialog_is_modal()), "the first Escape closes only the list");
    s.key("escape");
    assert!(!s.with(|e, _| e.dialog_is_modal()), "the second Escape closes the dialog");
}
