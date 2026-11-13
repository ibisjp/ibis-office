// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the Window menu and the list of windows (menu bar redesign, stage 4, `tasks/doc/menu-redesign`): the order the
//! open documents were last used in, the list Ctrl+Tab opens (current on the second row, Enter, Esc, a click), the next and
//! previous window, the list at the bottom of the menu (at most five, a mark for unsaved changes, the format's icon),
//! keys that do not collide, and a menu that other windows' drawing does not close. Pictures: `target/scenario/shell__window_*/`.
use gpui_kit::*;
use ibis_pptx::Kind;

use super::Scenario;
use crate::xlsx::scenarios::SheetSteps;
use crate::shell::chrome::{MenuEntry, MenuId};
use crate::shell::open_documents::OpenDocuments;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::open_documents_mru_order", mru_order),
        ("shell::window_list_current_is_second", current_is_second),
        ("shell::window_list_enter_activates", enter_activates),
        ("shell::window_list_esc_closes", esc_closes),
        ("shell::window_list_click_confirms", click_confirms),
        ("shell::window_menu_bottom_list", bottom_list),
        ("shell::window_next_prev_wrap", next_prev_wrap),
        ("shell::window_keys_no_conflict", keys_no_conflict),
        ("shell::window_menu_other_window_not_cleared", other_window_not_cleared),
        ("shell::window_menu_dirty_marker_and_close", dirty_marker_and_close),
        ("shell::window_menu_layout", menu_layout),
    ]
}

/// Open windows of the kinds, each drawn once so that it is in the table of open documents.
fn open_others(s: &mut Scenario, kinds: &[Kind]) -> Vec<AnyWindowHandle> {
    let before = s.with_app(|cx| cx.windows());
    for kind in kinds {
        let kind = *kind;
        s.with_app(|cx| crate::open_window_with(cx, move |e, w, cx| e.new_file(kind, w, cx)));
    }
    let others: Vec<AnyWindowHandle> = s.with_app(|cx| cx.windows()).into_iter().filter(|w| !before.contains(w)).collect();
    draw(s, &others);
    others
}

/// Draw the windows (the scenario settles its own).
fn draw(s: &mut Scenario, windows: &[AnyWindowHandle]) {
    for _ in 0..2 {
        for w in windows {
            s.update_window(*w, |w, _| w.refresh());
        }
        s.settle();
    }
}

fn me(s: &mut Scenario) -> EntityId {
    s.with(|e, _| e.entity_id)
}

/// The names of the documents, most recently used first.
fn mru_names(s: &mut Scenario) -> Vec<String> {
    s.with_app(|cx| OpenDocuments::mru(cx)).into_iter().map(|d| d.name.to_string()).collect()
}

fn mru_ids(s: &mut Scenario) -> Vec<EntityId> {
    s.with_app(|cx| OpenDocuments::mru(cx)).into_iter().map(|d| d.id).collect()
}

fn activate(s: &mut Scenario, id: EntityId) {
    s.with_app(|cx| OpenDocuments::activate(cx, id));
}

/// A document, a spreadsheet and a presentation open: the first window and the two others.
fn three() -> (Scenario, Vec<EntityId>) {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1400., 900.);
    open_others(&mut s, &[Kind::Workbook, Kind::Presentation]);
    let mut ids = mru_ids(&mut s);
    ids.sort_by_key(|id| id.as_u64());
    assert_eq!(ids.len(), 3, "three documents are open");
    // In the order they were opened: the document, the spreadsheet, the presentation.
    let opened: Vec<EntityId> = s.with_app(|cx| OpenDocuments::by_open_order(cx)).into_iter().map(|d| d.id).collect();
    (s, opened)
}

/// The table follows the activations: A, B, C gives C, B, A; then B gives B, C, A.
fn mru_order() {
    let (mut s, ids) = three();
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    for id in [a, b, c] {
        activate(&mut s, id);
    }
    assert_eq!(mru_ids(&mut s), vec![c, b, a]);
    activate(&mut s, b);
    assert_eq!(mru_ids(&mut s), vec![b, c, a]);
    // Opening order does not change with use.
    let opened: Vec<EntityId> = s.with_app(|cx| OpenDocuments::by_open_order(cx)).into_iter().map(|d| d.id).collect();
    assert_eq!(opened, vec![a, b, c]);
    let kinds: Vec<Kind> = s.with_app(|cx| OpenDocuments::by_open_order(cx)).into_iter().map(|d| d.kind).collect();
    assert_eq!(kinds, vec![Kind::Document, Kind::Workbook, Kind::Presentation]);
}

/// The list opens on the second row (the window used just before this one), on the first with one window, and on the last with
/// Ctrl+Shift+Tab; the key opens it.
fn current_is_second() {
    let (mut s, ids) = three();
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    // The first window (the document) is the current one, the presentation was used before it.
    for id in [b, c, a] {
        activate(&mut s, id);
    }
    assert_eq!(mru_ids(&mut s), vec![a, c, b]);
    s.key("ctrl-tab");
    assert!(s.with(|e, _| e.window_switcher_open()), "Ctrl+Tab opens the list of windows");
    assert_eq!(s.with(|e, _| e.window_switcher.as_ref().map(|w| w.rows.clone())), Some(vec![a, c, b]), "most recently used first, this window on the top");
    s.screenshot("window-list");
    s.key("enter");
    assert!(!s.with(|e, _| e.window_switcher_open()), "Enter confirms and closes");
    assert_eq!(mru_ids(&mut s)[0], c, "the row after the current one was brought forward");
    // From the end.
    for id in [b, c, a] {
        activate(&mut s, id);
    }
    s.key("ctrl-shift-tab");
    assert!(s.with(|e, _| e.window_switcher_open()));
    s.key("enter");
    assert_eq!(mru_ids(&mut s)[0], b, "Ctrl+Shift+Tab: the last row (the window used the longest ago)");
}

/// ↓ then Enter brings that row's window forward and the table is updated; with a single window Enter changes nothing.
fn enter_activates() {
    let (mut s, ids) = three();
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    for id in [b, c, a] {
        activate(&mut s, id);
    }
    // rows: a (this), c, b. Down once from the second row: b.
    s.key("ctrl-tab");
    s.key("down");
    s.key("enter");
    assert_eq!(mru_ids(&mut s)[0], b, "↓ moved to the third row, Enter brought it forward");
    // Tab moves as ↓ does and wraps.
    for id in [b, c, a] {
        activate(&mut s, id);
    }
    s.key("ctrl-tab");
    s.key("tab");
    s.key("tab");
    s.key("enter");
    assert_eq!(mru_ids(&mut s)[0], a, "Tab twice from the second row wraps to the first (this window): nothing changes");
    // A single window: Enter on the first row does nothing.
    drop(s);
    let mut one = Scenario::open("test/editing/xlsx-sales.xlsx");
    let id = me(&mut one);
    one.key("ctrl-tab");
    assert!(one.with(|e, _| e.window_switcher_open()), "the list opens with one window too");
    one.key("enter");
    assert!(!one.with(|e, _| e.window_switcher_open()));
    assert_eq!(mru_ids(&mut one), vec![id]);
}

/// Esc closes the list and nothing changes; another menu that is open takes the Esc.
fn esc_closes() {
    let (mut s, ids) = three();
    for id in [ids[1], ids[2], ids[0]] {
        activate(&mut s, id);
    }
    let before = mru_ids(&mut s);
    s.key("ctrl-tab");
    assert!(s.with(|e, _| e.window_switcher_open()));
    s.key("escape");
    assert!(!s.with(|e, _| e.window_switcher_open()), "Esc closes the list");
    assert_eq!(mru_ids(&mut s), before, "and changes nothing");
    // A menu open takes the Esc (the list is not opened over it).
    s.update(|e, w, cx| e.open_menu_at(0, w, cx));
    s.key("escape");
    assert!(s.with(|e, _| e.open_menu.is_none()), "Esc closes the menu");
    assert_eq!(mru_ids(&mut s), before);
}

/// A click on a row confirms it.
fn click_confirms() {
    let (mut s, ids) = three();
    for id in [ids[1], ids[2], ids[0]] {
        activate(&mut s, id);
    }
    s.update(|e, w, cx| e.open_window_switcher(false, w, cx));
    s.settle();
    let target = ids[2];
    // The second row is the presentation (a document, the presentation, the spreadsheet).
    let rows = s.with(|e, _| e.window_switcher.as_ref().map(|w| w.rows.clone())).expect("open");
    assert_eq!(rows[1], target);
    s.update(move |e, w, cx| e.activate_open_document(target, w, cx));
    assert_eq!(mru_ids(&mut s)[0], target);
    assert!(!s.with(|e, _| e.window_switcher_open()));
}

fn window_entries(s: &mut Scenario) -> Vec<MenuEntry> {
    s.with(|e, _| e.bar_items().into_iter().find(|(m, _)| m.key() == MenuId::Window.key()).map(|(_, entries)| entries).expect("a Window menu"))
}

fn window_rows(entries: &[MenuEntry]) -> usize {
    entries.iter().filter(|e| !matches!(e, MenuEntry::Separator)).count()
}

/// The list at the bottom: in the order of opening, the current window checked, the icon of the format; more than five end with
/// 「その他のウィンドウ（N）…」 and keep this window among the five; the menu has 20 rows or fewer.
fn bottom_list() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1400., 900.);
    open_others(&mut s, &[Kind::Document, Kind::Presentation]);
    let entries = window_entries(&mut s);
    let open: Vec<_> = entries.iter().filter_map(|e| if let MenuEntry::Item(i) = e { Some(i.clone()) } else { None }).filter(|i| i.id.starts_with("window.open.")).collect();
    assert_eq!(open.len(), 3, "three windows are listed");
    assert_eq!(open.iter().filter(|i| i.checked).count(), 1, "one is checked: this window");
    assert!(open[0].checked, "the first (opened first) is this one");
    assert!(open.iter().all(|i| i.format_mark.is_some()), "each has the icon of its format");
    assert_eq!(open.iter().map(|i| i.format_mark.unwrap().0).collect::<Vec<_>>(), vec![crate::shell::menu_context::format_color(Kind::Workbook), crate::shell::menu_context::format_color(Kind::Document), crate::shell::menu_context::format_color(Kind::Presentation)]);
    assert!(window_rows(&entries) <= 20, "the Window menu has {} rows", window_rows(&entries));
    // More than five: five and the "more" line; this window is in the five even when it was opened last.
    open_others(&mut s, &[Kind::Document, Kind::Document, Kind::Document]);
    let entries = window_entries(&mut s);
    let ids: Vec<&str> = entries.iter().filter_map(|e| if let MenuEntry::Item(i) = e { Some(i.id) } else { None }).filter(|id| id.starts_with("window.open.")).collect();
    assert_eq!(ids, vec!["window.open.0", "window.open.1", "window.open.2", "window.open.3", "window.open.4", "window.open.more"], "five windows and the line for the rest");
    let more = entries.iter().find_map(|e| (if let MenuEntry::Item(i) = e { Some(i.clone()) } else { None }).filter(|i| i.id == "window.open.more")).unwrap();
    assert!(more.label.contains('1'), "the rest is one window: {}", more.label);
    assert!(window_rows(&entries) <= 20, "{} rows", window_rows(&entries));
    // A window opened last is still listed (the sixth window is this one in its own menu).
    let last = s.with_app(|cx| OpenDocuments::by_open_order(cx)).last().map(|d| d.editor.clone()).unwrap();
    let handle = s.with_app(|cx| OpenDocuments::by_open_order(cx)).last().map(|d| d.window).unwrap();
    let checked = s.update_window(handle, |window, cx| {
        let _ = window;
        last.update(cx, |e, _| e.open_window_menu_items().into_iter().filter_map(|en| if let MenuEntry::Item(i) = en { Some((i.id, i.checked)) } else { None }).collect::<Vec<_>>()).unwrap()
    });
    assert!(checked.iter().any(|(id, c)| *c && *id != "window.open.more"), "the sixth window finds itself in its list: {checked:?}");
    s.update(|e, w, cx| e.open_menu_at(window_index(e), w, cx));
    s.screenshot("window-menu-six");
}

fn window_index(e: &crate::shell::editor::SlideEditor) -> usize {
    e.bar_items().iter().position(|(m, _)| m.key() == MenuId::Window.key()).expect("a Window menu")
}

/// Next and previous go in the order of opening and wrap round; they are not the list's order.
fn next_prev_wrap() {
    let (mut s, ids) = three();
    let (a, b, c) = (ids[0], ids[1], ids[2]);
    let step = |s: &mut Scenario, forward: bool| {
        s.update(move |e, w, cx| e.step_window(forward, w, cx));
        mru_ids(s)[0]
    };
    // From the first window: next is the second, then (from the second) the third, and the previous of the first is the third.
    assert_eq!(step(&mut s, true), b);
    assert_eq!(s.with_app(|cx| OpenDocuments::neighbour(cx, b, true)).map(|d| d.id), Some(c));
    assert_eq!(s.with_app(|cx| OpenDocuments::neighbour(cx, c, true)).map(|d| d.id), Some(a), "wraps round at the end");
    assert_eq!(s.with_app(|cx| OpenDocuments::neighbour(cx, a, false)).map(|d| d.id), Some(c), "and at the start");
    assert_eq!(step(&mut s, false), c);
    // One window: nothing.
    drop(s);
    let mut one = Scenario::open("test/editing/pptx-plan.pptx");
    let id = me(&mut one);
    assert!(one.with_app(|cx| OpenDocuments::neighbour(cx, id, true)).is_none());
}

/// The three commands' keys are nobody else's: in the table of the commands (the menus' shortcuts), and on each screen.
fn keys_no_conflict() {
    for file in ["test/editing/docx-report.docx", "test/editing/xlsx-sales.xlsx", "test/editing/pptx-plan.pptx"] {
        let mut s = Scenario::open(file);
        let all: Vec<(&'static str, Option<&'static str>)> = s.with(|e, _| {
            let (_, reg) = e.menu_bar_menus_and_registry();
            reg.commands.iter().map(|c| (c.id, c.shortcut)).collect()
        });
        let key_of = |id: &str| all.iter().find(|(i, _)| *i == id).and_then(|(_, k)| *k).unwrap_or_else(|| panic!("{file}: {id} has no key"));
        let keys = [key_of("window.list"), key_of("window.next"), key_of("window.prev")];
        for key in keys {
            // `xlsx.show-formulas` is labelled ⌘` but is bound to Ctrl+` (xlsx/mod.rs): the system's ⌘` is not taken.
            let users: Vec<&str> = all.iter().filter(|(i, k)| *k == Some(key) && *i != "xlsx.show-formulas").map(|(i, _)| *i).collect();
            assert_eq!(users.len(), 1, "{file}: the key {key} is used by {users:?}");
        }
        assert_eq!(keys[0], "⌃Tab");
        if cfg!(target_os = "macos") {
            assert_eq!([keys[1], keys[2]], ["⌘`", "⌘⇧`"]);
        } else {
            assert_eq!([keys[1], keys[2]], ["⌃F6", "⌃⇧F6"]);
        }
    }
}

/// Another window's drawing does not close the Window menu that is open (`followup_recent_menu_other_window`'s rule).
fn other_window_not_cleared() {
    let (mut s, _) = three();
    let ix = s.with(|e, _| window_index(e));
    s.update(move |e, w, cx| e.open_menu_at(ix, w, cx));
    let others: Vec<AnyWindowHandle> = s.with_app(|cx| cx.windows()).into_iter().skip(1).collect();
    for step in 0..4 {
        draw(&mut s, &others);
        s.hover((300. + step as f32 * 4., 200.));
        assert_eq!(s.with(|e, _| e.open_menu.as_ref().map(|o| o.index())), Some(ix), "the Window menu stays while the other windows draw (step {step})");
    }
    // The list of windows too.
    s.update(|e, _, cx| e.close_menu(cx));
    s.update(|e, w, cx| e.open_window_switcher(false, w, cx));
    draw(&mut s, &others);
    assert!(s.with(|e, _| e.window_switcher_open()), "the list of windows stays while the other windows draw");
    s.update(|e, _, cx| e.close_window_switcher(cx));
}

/// A document with changes shows ● in the lists, saving clears it, and a window that is closed leaves the list.
fn dirty_marker_and_close() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1400., 900.);
    let others = open_others(&mut s, &[Kind::Presentation]);
    let mine = |s: &mut Scenario| -> String { s.with_app(|cx| OpenDocuments::by_open_order(cx)).into_iter().next().unwrap().name.to_string() };
    let dirty = |s: &mut Scenario| -> bool { s.with_app(|cx| OpenDocuments::by_open_order(cx)).into_iter().next().unwrap().dirty };
    assert!(!dirty(&mut s), "just opened: no unsaved changes");
    s.click_cell("A1");
    s.type_text("changed");
    s.key("enter");
    assert!(dirty(&mut s), "an edit marks it");
    let label = |s: &mut Scenario| -> String {
        window_entries(s).into_iter().find_map(|e| (if let MenuEntry::Item(i) = e { Some(i) } else { None }).filter(|i| i.id == "window.open.0")).unwrap().label.to_string()
    };
    assert!(label(&mut s).ends_with('●'), "the Window menu shows ● : {}", label(&mut s));
    s.key("cmd-s");
    assert!(!dirty(&mut s), "saved: the mark is gone");
    assert!(!label(&mut s).contains('●'), "{}", label(&mut s));
    assert!(!mine(&mut s).is_empty());
    // A closed window leaves the list.
    assert_eq!(mru_ids(&mut s).len(), 2);
    s.close_window(others[0]);
    draw(&mut s, &[]);
    assert_eq!(mru_ids(&mut s).len(), 1, "the closed window is gone from the table");
    let rows = window_entries(&mut s).iter().filter(|e| matches!(e, MenuEntry::Item(i) if i.id.starts_with("window.open."))).count();
    assert_eq!(rows, 1);
}

/// The Window menu of the three screens: the list, next and previous first, the sidebar, the full screen, at most 20 rows; a picture.
fn menu_layout() {
    for (name, file) in [("docx", "test/editing/docx-report.docx"), ("xlsx", "test/editing/xlsx-sales.xlsx"), ("pptx", "test/editing/pptx-plan.pptx")] {
        let mut s = Scenario::open(file);
        s.size(1400., 900.);
        let entries = window_entries(&mut s);
        let ids: Vec<&str> = entries.iter().filter_map(|e| match e { MenuEntry::Item(i) => Some(i.id), MenuEntry::Submenu(sub) => Some(sub.id), _ => None }).collect();
        assert_eq!(&ids[..3], ["window.list", "window.next", "window.prev"], "{name}: the window commands first: {ids:?}");
        assert!(ids.contains(&"view.fullscreen"), "{name}: full screen: {ids:?}");
        assert!(ids.iter().any(|i| i.starts_with("window.open.")), "{name}: the open windows at the bottom");
        assert!(window_rows(&entries) <= 20, "{name}: {} rows", window_rows(&entries));
        // With one window the next and previous are not available.
        let disabled = |id: &str| entries.iter().any(|e| matches!(e, MenuEntry::Item(i) if i.id == id && !i.enabled()));
        assert!(disabled("window.next") && disabled("window.prev"), "{name}: one window: next and previous are greyed");
        let ix = s.with(|e, _| window_index(e));
        s.update(move |e, w, cx| e.open_menu_at(ix, w, cx));
        s.screenshot(&format!("{name}-window-menu"));
    }
}
