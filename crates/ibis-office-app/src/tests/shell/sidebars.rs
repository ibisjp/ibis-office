// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the sidebar table, the menu search made from the command table, and the doors of what left the menus (menu bar
//! redesign stages 5, 6 and 7, `tasks/doc/menu-redesign`): every resident pane of a screen's pane chain is in the screen's table, the
//! Window menu's the Sidebar submenu has exactly the table's panes for the screen, a pane that cannot be used is grey with its reason, the
//! sidebars and every command that left the menus are found by the search, and each of them has a door.
use std::collections::HashSet;

use ibis_pptx::Kind;

use crate::shell::chrome::{MenuEntry, MenuId};
use crate::shell::menu_context::{ContextState, ObjectKind};
use crate::shell::scenario::Scenario;
use crate::shell::sidebars::{self, Availability, SidebarDef, SidebarKind, SidebarSlot};

pub fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::sidebars_no_duplicates", no_duplicates),
        ("shell::sidebars_cover_pane_chain", cover_pane_chain),
        ("shell::sidebars_commands_exist", commands_exist),
        ("shell::sidebars_in_menu_for_every_screen", in_menu_for_every_screen),
        ("shell::sidebars_disabled_when_unavailable", disabled_when_unavailable),
        ("shell::sidebars_checked_state", checked_state),
        ("shell::sidebars_search_index", search_index),
        ("shell::sidebars_i18n_complete", i18n_complete),
        ("shell::sidebars_submenu_rows_within_limit", submenu_rows_within_limit),
        ("shell::search_finds_every_removed_command", search_finds_every_removed_command),
        ("shell::removed_from_menu_have_entry", removed_from_menu_have_entry),
    ]
}

const FILES: [(Kind, &str, &str); 3] = [
    (Kind::Document, "docx", "test/editing/docx-report.docx"),
    (Kind::Workbook, "xlsx", "test/editing/xlsx-compatibility-v1.xlsx"),
    (Kind::Presentation, "pptx", "test/benchmark/earnings-presentation.pptx"),
];

fn flat(entries: &[MenuEntry], out: &mut Vec<crate::shell::chrome::MenuItem>) {
    for entry in entries {
        match entry {
            MenuEntry::Item(item) => out.push(item.clone()),
            MenuEntry::Submenu(sub) => flat(&sub.items, out),
            _ => {}
        }
    }
}

/// The lines of the Sidebar submenu in the bar of the open document.
fn sidebar_items(s: &mut Scenario) -> Vec<crate::shell::chrome::MenuItem> {
    s.with(|e, _| {
        let menus = e.menu_bar_menus();
        let window = menus.menus.iter().find(|(id, _)| *id == MenuId::Window).map(|(_, entries)| entries.clone()).unwrap_or_default();
        let mut out = Vec::new();
        for entry in &window {
            if let MenuEntry::Submenu(sub) = entry {
                if sub.id == "menu.window.sidebar" {
                    flat(&sub.items, &mut out);
                }
            }
        }
        out
    })
}

/// The rows of the three screens' tables.
fn all_defs() -> impl Iterator<Item = &'static SidebarDef> {
    [Kind::Document, Kind::Workbook, Kind::Presentation].into_iter().flat_map(|kind| sidebars::of(kind).iter())
}

fn no_duplicates() {
    let mut labels = HashSet::new();
    for kind in [Kind::Document, Kind::Workbook, Kind::Presentation] {
        let mut seen = HashSet::new();
        for def in sidebars::of(kind) {
            assert!(seen.insert(def.id), "{kind:?}: the sidebar {} is in the table twice", def.id);
        }
        // A sidebar two screens have is the same sidebar: its name, its place and its group.
        for def in sidebars::of(kind) {
            for other in all_defs().filter(|o| o.id == def.id) {
                assert!(
                    (def.label_key, def.slot, def.group, def.kind) == (other.label_key, other.slot, other.group, other.kind),
                    "the sidebar {} is not the same in two screens' tables",
                    def.id
                );
            }
        }
        labels.clear();
        for def in sidebars::of(kind).iter().filter(|d| d.listed()) {
            let label = crate::shell::i18n::text(def.label_key).to_string();
            assert!(labels.insert(label.clone()), "{kind:?}: two sidebars are named {label}");
        }
    }
}

/// The names of the resident panels of a pane chain (`fn sidebar` of a screen): the panel functions the chain asks for that are not
/// wrapped as a dialog (`modal_named`, `modeless_named`).
fn chain_panels(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in source.lines().filter(|l| l.matches("or_else(").count() > 5) {
        let mut rest = line;
        while let Some(at) = rest.find("editor.") {
            rest = &rest[at + "editor.".len()..];
            let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if !rest[name.len()..].starts_with("(cx)") {
                continue;
            }
            let after = &rest[name.len() + 4..];
            // `.map(|panel| panel.modal_named(` right after the call wraps it as a dialog.
            let wrapped = after.starts_with(".map(") && after[..after.len().min(60)].contains("_named(");
            if !wrapped && !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

/// A resident panel added to a pane chain with no sidebar in the table fails here; so does a table entry that names a panel the
/// chain does not have.
fn cover_pane_chain() {
    let sources = [
        ("docx", include_str!("../../../../ibis-office-docx/src/lib.rs")),
        ("xlsx", include_str!("../../../../ibis-office-xlsx/src/lib.rs")),
        ("pptx", include_str!("../../../../ibis-office-pptx/src/lib.rs")),
    ];
    for ((name, source), kind) in sources.into_iter().zip([Kind::Document, Kind::Workbook, Kind::Presentation]) {
        let named: HashSet<&str> = sidebars::of(kind).iter().flat_map(|d| d.panels.iter().copied()).collect();
        let panels = chain_panels(source);
        assert!(panels.len() >= 6, "{name}: the chain's panels were not found in the source ({panels:?})");
        let missing: Vec<_> = panels.iter().filter(|p| !named.contains(p.as_str())).collect();
        assert!(missing.is_empty(), "{name}: resident panels of the pane chain with no sidebar in {name}/sidebars.rs (add a SidebarDef that names them): {missing:?}");
        let unknown: Vec<_> = named.iter().filter(|p| !panels.iter().any(|q| q == **p)).collect();
        assert!(unknown.is_empty(), "{name}: the sidebars name panels that the pane chain does not have: {unknown:?}");
    }
}

/// The commands the table names exist in the open screen's table (a typo would make a row that does nothing).
fn commands_exist() {
    for (kind, name, file) in FILES {
        let mut s = Scenario::open(file);
        s.with(|e, _| {
            let (_, reg) = e.menu_bar_menus_and_registry();
            for def in sidebars::of(kind) {
                if let Some(id) = def.cmd {
                    assert!(matches!(reg.entry(id), Some(MenuEntry::Item(_))), "{name}: the sidebar {} names the command {id}, which is not an item of the screen", def.id);
                }
            }
        });
    }
}

fn in_menu_for_every_screen() {
    for (kind, name, file) in FILES {
        let mut s = Scenario::open(file);
        let shown: Vec<&'static str> = sidebar_items(&mut s).iter().map(|i| i.id).collect();
        let want: Vec<&'static str> = sidebars::of(kind).iter().filter(|d| d.listed()).map(|d| d.id).collect();
        for id in &want {
            assert!(shown.contains(id), "{name}: the sidebar {id} is not under the Sidebar submenu ({shown:?})");
        }
        let extra: Vec<_> = shown.iter().filter(|i| !want.contains(i) && **i != "view.sidebar" && !i.starts_with("sidebar.heading.")).collect();
        assert!(extra.is_empty(), "{name}: under the Sidebar submenu but not in the table: {extra:?}");
        // The panes that only open a sidebar are not in another menu.
        let others: Vec<&str> = s.with(|e, _| {
            let menus = e.menu_bar_menus();
            let mut out = Vec::new();
            for (menu, entries) in &menus.menus {
                if *menu == MenuId::Window {
                    continue;
                }
                let mut items = Vec::new();
                flat(entries, &mut items);
                out.extend(items.into_iter().map(|i| i.id).filter(|id| crate::shell::sidebars::pane_only_commands(kind).contains(id)));
            }
            out
        });
        assert!(others.is_empty(), "{name}: commands that only open a pane are left in other menus: {others:?}");
    }
}

fn disabled_when_unavailable() {
    // Pure: what each availability says for a state (nothing selected, a picture, a chart, a pivot…).
    for kind in [Kind::Document, Kind::Workbook, Kind::Presentation] {
        let none = ContextState::none(kind);
        let mut picture = ContextState::none(kind);
        picture.objects = vec![ObjectKind::Picture];
        let mut chart = ContextState::none(kind);
        chart.objects = vec![ObjectKind::Chart];
        for def in sidebars::of(kind).iter().filter(|d| d.listed()) {
            let a = (def.available)(&none);
            if let Availability::Disabled(reason) = a {
                assert!(!crate::shell::i18n::text(reason).starts_with("sidebar."), "{}: the reason {reason} is not in the tables", def.id);
            }
        }
        let picture_def = all_defs().find(|d| d.id == "sidebar.picture").unwrap();
        assert_eq!((picture_def.available)(&none), Availability::Disabled("sidebar.reason.picture"));
        assert_eq!((picture_def.available)(&picture), Availability::Enabled);
        let chart_def = all_defs().find(|d| d.id == "sidebar.chart").unwrap();
        assert_eq!((chart_def.available)(&chart), Availability::Enabled);
        assert_eq!((chart_def.available)(&picture), Availability::Disabled("sidebar.reason.chart"));
    }
    // In the window: with nothing selected the rows that need a selection are grey with their reason; they are listed, not hidden.
    for (kind, name, file) in FILES {
        let mut s = Scenario::open(file);
        let items = sidebar_items(&mut s);
        for def in sidebars::of(kind).iter().filter(|d| d.listed()) {
            let item = items.iter().find(|i| i.id == def.id).unwrap_or_else(|| panic!("{name}: {} is hidden", def.id));
            if let Availability::Disabled(reason) = (def.available)(&ContextState::none(kind)) {
                assert!(item.disabled, "{name}: {} needs a selection and must be grey", def.id);
                assert_eq!(item.hint.as_deref(), Some(crate::shell::i18n::text(reason).as_ref()), "{name}: {} shows its reason", def.id);
            }
        }
    }
}

/// A pane that has a command is checked as the command is, and choosing it again closes it (the navigation pane of a document).
fn checked_state() {
    let mut s = Scenario::open(FILES[0].2);
    let checked = |s: &mut Scenario| sidebar_items(s).iter().find(|i| i.id == "sidebar.navigation").map(|i| i.checked).unwrap();
    assert!(!checked(&mut s), "the navigation pane starts closed");
    let item = sidebar_items(&mut s).into_iter().find(|i| i.id == "sidebar.navigation").unwrap();
    assert!(item.enabled());
    let handler = item.handler.clone().unwrap();
    s.update(move |e, w, cx| handler(e, w, cx));
    s.settle();
    assert!(checked(&mut s), "choosing the row opens the pane and it is checked");
    let item = sidebar_items(&mut s).into_iter().find(|i| i.id == "sidebar.navigation").unwrap();
    let handler = item.handler.clone().unwrap();
    s.update(move |e, w, cx| handler(e, w, cx));
    s.settle();
    assert!(!checked(&mut s), "choosing it again closes it");
}

/// Every sidebar that can be opened from the menu is found by the menu search under its name, with where it is.
fn search_index() {
    for (kind, name, file) in FILES {
        let mut s = Scenario::open(file);
        let index = s.with(|e, _| e.command_search_items());
        let items = sidebar_items(&mut s);
        for def in sidebars::of(kind).iter().filter(|d| d.listed()) {
            let row = items.iter().find(|i| i.id == def.id).unwrap();
            if !row.enabled() {
                continue;
            }
            assert!(index.iter().any(|i| i.label == row.label), "{name}: the search does not find the sidebar {} ({})", def.id, row.label);
        }
        assert!(index.iter().all(|i| i.note.is_some()), "{name}: every search row says where the command is");
    }
}

/// The words of the sidebars (names, headings, reasons) are in the Japanese and English tables.
fn i18n_complete() {
    let ja = include_str!("../../../../ibis-office-base/i18n/ja/shell.tsv");
    let en = include_str!("../../../../ibis-office-base/i18n/en/shell.tsv");
    let has = |table: &str, key: &str| table.lines().any(|l| l.split('\t').next() == Some(key));
    let mut keys: Vec<&str> = all_defs().map(|d| d.label_key).collect();
    keys.extend(["sidebar.heading.left", "sidebar.heading.bottom", "sidebar.heading.right", "sidebar.reason.automatic", "sidebar.reason.unavailable"]);
    for def in all_defs() {
        for kind in [Kind::Document, Kind::Workbook, Kind::Presentation] {
            if let Availability::Disabled(reason) = (def.available)(&ContextState::none(kind)) {
                keys.push(reason);
            }
        }
    }
    for key in keys {
        assert!(has(ja, key), "{key} is not in i18n/ja/shell.tsv");
        assert!(has(en, key), "{key} is not in i18n/en/shell.tsv");
    }
}

fn submenu_rows_within_limit() {
    for (kind, name, file) in FILES {
        let mut s = Scenario::open(file);
        let items = sidebar_items(&mut s);
        let rows = items.iter().filter(|i| !i.id.starts_with("sidebar.heading.")).count();
        assert!(rows <= 25, "{name}: the Sidebar submenu has {rows} rows (at most 25)");
        assert!(sidebars::of(kind).iter().any(|d| d.listed() && d.slot == SidebarSlot::Right && d.kind == SidebarKind::Pane));
    }
}

/// Every command the bar does not show (they are `menu_hidden`: the Home commands of the toolbar and the rest) that can run now is
/// found by the search under its own name.
fn search_finds_every_removed_command() {
    for (_, name, file) in FILES {
        let mut s = Scenario::open(file);
        s.with(|e, _| {
            let (_, reg) = e.menu_bar_menus_and_registry();
            let index = e.command_search_items();
            let lost: Vec<_> = reg
                .hidden()
                .filter(|c| c.enabled && c.run.is_some() && !c.label.is_empty() && c.id != "help.search" && !c.id.starts_with("font.section."))
                .filter(|c| !index.iter().any(|i| i.id == c.id))
                .map(|c| c.id)
                .collect();
            assert!(lost.is_empty(), "{name}: commands that left the menus and are not found by the search: {lost:?}");
            // Searching by a typed name finds it (bold, a Home command).
            if name == "docx" {
                let bold = crate::shell::i18n::text("_shell.bold");
                let hit = crate::shell::chrome::filter_menu_items(&index, &bold);
                assert!(!hit.is_empty(), "{name}: the search does not find {bold}");
            }
        });
    }
}

/// The ids of the old menus' top-level items that the redesign takes out of the menu bar (`moved-and-removed.md`, "removed from the menus").
const REMOVED_FROM_MENU: [(Kind, &[&str]); 3] = [
    (Kind::Document, &["format.text", "format.align", "format.spacing", "format.lists", "format.borders", "format.options", "format.clear", "docx.format-brush.lock", "docx.heading-numbers"]),
    (Kind::Workbook, &["edit.duplicate", "format.text", "format.align", "format.borders", "format.clear", "xlsx.comma-style", "xlsx.phonetic-toggle", "xlsx.phonetic-edit", "xlsx.view-code", "xlsx.xml-source", "xlsx.xml-map-properties"]),
    (Kind::Presentation, &["pptx.motion", "format.text", "pptx.text.shadow", "format.align", "format.spacing", "format.lists", "format.borders", "format.clear", "pptx.paragraph.settings"]),
];

/// Each of them still has a door: the command is in the table, it is not in the bar, and what it is (or, for a group of the old Format
/// menu, each of its commands that can run) is found by the search and has the toolbar button, the shortcut or the sidebar it
/// names. A group that the old menus did not fill any more has nothing left to find and is not asked for.
fn removed_from_menu_have_entry() {
    for (kind, name, file) in FILES {
        let mut s = Scenario::open(file);
        s.with(|e, _| {
            let (menus, reg) = e.menu_bar_menus_and_registry();
            let index = e.command_search_items();
            let mut shown = Vec::new();
            for (_, entries) in &menus.menus {
                fn ids(entries: &[MenuEntry], out: &mut Vec<&'static str>) {
                    for entry in entries {
                        match entry {
                            MenuEntry::Item(i) => out.push(i.id),
                            MenuEntry::Submenu(s) => {
                                out.push(s.id);
                                ids(&s.items, out);
                            }
                            _ => {}
                        }
                    }
                }
                ids(entries, &mut shown);
            }
            let toolbar: HashSet<&'static str> = e
                .toolbar_items()
                .into_iter()
                .filter_map(|t| match t {
                    crate::shell::chrome::ToolItem::Button(b) => Some(b.id),
                    _ => None,
                })
                .collect();
            let ids = REMOVED_FROM_MENU.iter().find(|(k, _)| *k == kind).unwrap().1;
            for id in ids {
                let Some(entry) = reg.entry(id) else { panic!("{name}: {id} left the menus but is not in the command table") };
                let mut leaves = Vec::new();
                flat(std::slice::from_ref(entry), &mut leaves);
                let runnable: Vec<_> = leaves.iter().filter(|l| l.handler.is_some() && !l.disabled).collect();
                if runnable.is_empty() {
                    // Not made, or needs a state this document is not in (a command that cannot run now has no door to test here).
                    continue;
                }
                let door = |leaf: &crate::shell::chrome::MenuItem| {
                    index.iter().any(|i| i.id == leaf.id) || toolbar.contains(leaf.id) || leaf.shortcut.is_some()
                };
                let lost: Vec<_> = runnable.iter().filter(|l| !door(l)).map(|l| l.id).collect();
                assert!(lost.is_empty(), "{name}: {id}: commands with no door (not found by the search, no toolbar button, no shortcut): {lost:?}");
                // The group itself is not in the bar; a command that is one item is either hidden or deliberately kept (a shown id is a decision).
                if matches!(entry, MenuEntry::Submenu(_)) {
                    assert!(!shown.contains(id), "{name}: {id} is a group of the old Format menu and must not be in the bar");
                }
            }
        });
    }
}
