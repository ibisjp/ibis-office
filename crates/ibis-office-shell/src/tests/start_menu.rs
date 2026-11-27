// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0052 and K0047: the start window's menus. The scenarios press every line where it is drawn (the menu bar the start window has on
//! Windows and Linux, asked for on this system too), the keys ⌘D, ⌘E and ⌘P make the three kinds of new file in the start window only,
//! and ⌘P is still the print key of a document window.
use ibis_formats::Kind;

use super::{button_probe as probe, Scenario};
use crate::chrome::{MenuEntry, MenuId};

pub fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::start_menu_lines", lines),
        ("shell::start_menu_every_line_works", every_line_works),
        ("shell::start_menu_keys_make_new_files", keys_make_new_files),
        ("shell::start_menu_key_p_is_still_print_in_a_document", key_p_is_print_in_a_document),
    ]
}

/// The key of a Mac's shortcut on this system.
fn key(letter: &str) -> String {
    format!("{}-{letter}", if cfg!(target_os = "macos") { "cmd" } else { "ctrl" })
}

/// The lines the start window's menus have: File (the three new files first, with their keys; Open; the recent files; the window's close),
/// Window (Minimize) and Help (check for updates, source code, About).
fn lines() {
    let mut s = Scenario::start_view();
    let menus = s.with(|e, _| e.start_menus());
    let ids = |id: MenuId| -> Vec<&'static str> {
        menus.iter().find(|(m, _)| *m == id).map(|(_, entries)| entries.iter().filter_map(|e| if let MenuEntry::Item(i) = e { Some(i.id) } else { None }).collect()).unwrap_or_default()
    };
    let file = ids(MenuId::File);
    assert_eq!(&file[..5], ["file.new-document", "file.new-workbook", "file.new-presentation", "file.open", "file.close-window"], "File menu: {file:?}");
    let keys: Vec<Option<&str>> = menus[0].1.iter().filter_map(|e| if let MenuEntry::Item(i) = e { Some(i.shortcut) } else { None }).take(5).collect();
    assert_eq!(keys, [Some("⌘D"), Some("⌘E"), Some("⌘P"), Some("⌘O"), Some("⌘W")]);
    assert!(menus[0].1.iter().any(|e| matches!(e, MenuEntry::Submenu(sub) if sub.id == "file.recent")), "the File menu has the recent files");
    assert!(ids(MenuId::Window).contains(&"window.minimize"), "the Window menu has Minimize");
    assert!(ids(MenuId::Help).contains(&"help.about"), "the Help menu has About");
    // The start window is no document: it is not in the list of windows (#265: "プレゼンテーション1.pptx" was always listed).
    assert!(!ids(MenuId::Window).iter().any(|id| id.starts_with("window.open.")), "the start window lists itself: {:?}", ids(MenuId::Window));
}

/// Every line of the three menus, pressed in the menu bar: the press must reach `chrome::dispatch` (the one place the commands are run from).
fn every_line_works() {
    crate::start_menu::force_bar(true);
    let _probe = probe::Probe::new();
    probe::dry_run(true);
    let mut s = Scenario::start_view();
    s.settle();
    let menus = s.with(|e, _| e.start_menus());
    let mut failures: Vec<String> = Vec::new();
    let mut pressed = 0;
    for (menu, entries) in &menus {
        for entry in entries {
            let MenuEntry::Item(item) = entry else { continue };
            if !item.handler.is_some() || item.disabled {
                continue;
            }
            probe::click(&mut s, &format!("menu-button-{}", menu.key()));
            s.settle();
            let place = format!("menu-item-{}", item.id);
            if probe::bounds(&place).is_none() {
                failures.push(format!("{} {}: not drawn (found {:?})", menu.key(), item.id, probe::all().into_iter().filter(|k| k.starts_with("menu-item-")).collect::<Vec<_>>()));
                s.key("escape");
                continue;
            }
            let before = probe::dispatch_count();
            probe::click(&mut s, &place);
            s.settle();
            pressed += 1;
            if probe::dispatch_count() == before {
                failures.push(format!("{} {}: the press ran nothing", menu.key(), item.id));
            }
        }
    }
    crate::start_menu::force_bar(false);
    assert!(pressed >= 8, "only {pressed} lines were pressed");
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// ⌘D, ⌘E and ⌘P in the start window make a new document, spreadsheet and presentation in it.
fn keys_make_new_files() {
    for (letter, kind) in [("d", Kind::Document), ("e", Kind::Workbook), ("p", Kind::Presentation)] {
        let mut s = Scenario::start_view();
        assert!(!s.with(|e, _| e.has_document), "the start window shows no file");
        s.key(&key(letter));
        s.settle();
        let (has, now) = s.with(|e, _| (e.has_document, e.kind()));
        if !(has && now == kind) {
            let ks = gpui_kit::Keystroke::parse(&key(letter)).unwrap();
            let bound = s.update(|_, w, _| w.possible_bindings_for_input(&[ks]).iter().map(|b| b.action().name().to_string()).collect::<Vec<_>>().join(","));
            s.update(|_, w, cx| w.dispatch_action(Box::new(crate::actions::StartNewDocument), cx));
            s.settle();
            let directly = s.with(|e, _| e.has_document);
            panic!("{} in the start window: has a file {has}, kind {now:?}, wanted {kind:?} (the bindings of the key from the focus: {bound}; the action dispatched directly makes a file: {directly})", key(letter));
        }
    }
}

/// In a document window ⌘P is Print, not a new presentation: the window keeps its file and its kind.
fn key_p_is_print_in_a_document() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.settle();
    s.key(&key("p"));
    s.settle();
    let (has, now) = s.with(|e, _| (e.has_document, e.kind()));
    assert!(has && now == Kind::Document, "⌘P in a document: has a file {has}, kind {now:?}");
}
