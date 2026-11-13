// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Every access key the menus have, checked as a whole (shell/access_key.rs; `tasks/doc/design/alt-access-keys.md`; menu bar redesign
//! section 5), for Windows and Linux (`supported()` true): the three screens x the two languages x the menu bar (the static menus,
//! also the ones folded into the ▾), every level of every menu (the menus' lines, the submenus, the Draw button's menu, the coloured
//! menus that show in a state of the selection) and the pop-up menus that can be built without a pointer.
//!
//! What it holds, each failing with the screen, the language and the menu path of the line:
//! * in one level (one menu or submenu) no two lines have the same access letter;
//! * a mark written in a label (`保存(&S)`, `&Save`) is the letter the label gets, underlined at that letter, and a marked letter is
//!   not taken from it by an automatic one (marks win over automatic assignment);
//! * the bar's letters are different on a screen, not Alt+D (the Draw button) or Alt+J (the coloured menus), and a label shows its
//!   letter: in the English name, or `(X)` after a Japanese one.
use ibis_doc_model::ShapeKind;

use super::Scenario;
use crate::docx::scenarios::DocSteps;
use crate::shell::access_key::{self, explicit_letter, marked_letter, resolve};
use crate::shell::chrome::{assign_access_keys, MenuEntry};
use crate::shell::i18n::{self, Language};
use crate::shell::menu_bar::BarMenu;
use crate::shell::menu_layout::{layout, TitleMenu};
use crate::pptx::ext::*;
use crate::docx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::access_keys_bar_unique_all_languages", bar_unique),
        ("shell::access_keys_every_menu_level", every_menu_level),
        // The same on what is distributed (the planned lines hidden, `menu_layout::show_planned`): the keys of a level are assigned
        // among the lines it shows, so hiding lines may move them.
        ("shell::access_keys_bar_unique_planned_hidden", || crate::shell::menu_layout::with_planned(false, bar_unique)),
        ("shell::access_keys_every_menu_level_planned_hidden", || crate::shell::menu_layout::with_planned(false, every_menu_level)),
        ("shell::access_keys_resolution_rules", resolution_rules),
        ("shell::access_keys_menu_item_keys_run", item_keys_run),
        ("shell::access_keys_menu_items_off_on_mac", items_off_on_mac),
    ]
}

const FILES: [(&str, &str); 3] = [
    ("docx", "test/editing/docx-report.docx"),
    ("xlsx", "test/editing/xlsx-compatibility-v1.xlsx"),
    ("pptx", "test/benchmark/earnings-presentation.pptx"),
];

const LANGUAGES: [(&str, Language); 2] = [("ja", Language::Japanese), ("en", Language::English)];

/// One level of a menu, as `fill_menu` shows it: the keys the lines get (`assign_access_keys`), and the checks above.
fn walk(place: &str, entries: &[MenuEntry]) -> usize {
    let mut level = entries.to_vec();
    let before = access_key::duplicate_marks();
    assign_access_keys(&mut level);
    assert_eq!(access_key::duplicate_marks(), before, "{place}: a letter is named twice by the tables in this level");
    let mut seen: Vec<(char, String)> = Vec::new();
    let mut check = |id: &str, label: String, access: &Option<access_key::Resolved>| {
        let Some(r) = access else { return };
        let Some(key) = r.key else { return };
        if let Some((_, other)) = seen.iter().find(|(k, _)| *k == key) {
            panic!("{place}: the access key {key:?} is on both {other:?} and {label:?}");
        }
        seen.push((key, label.clone()));
        let range = r.underline.clone().unwrap_or_else(|| panic!("{place}: {label:?} has the key {key:?} but no underline"));
        let shown = r.text.get(range).unwrap_or_else(|| panic!("{place}: the underline of {label:?} is not on a letter of {:?}", r.text));
        assert_eq!(shown.to_ascii_lowercase(), key.to_string(), "{place}: the underline of {label:?} is not on its key {key:?} (text {:?})", r.text);
        if let Some(named) = explicit_letter(id) {
            assert_eq!(key, named, "{place}: {label:?} ({id}) is named {named:?} by the tables but got {key:?}: a named letter wins over an automatic one");
        }
    };
    for entry in &level {
        match entry {
            MenuEntry::Item(i) => check(i.id, i.label.to_string(), &i.access),
            MenuEntry::Submenu(s) => check(s.id, s.label.to_string(), &s.access),
            _ => {}
        }
    }
    let mut levels = 1;
    for entry in entries {
        if let MenuEntry::Submenu(sub) = entry {
            levels += walk(&format!("{place} > {}", sub.label), &sub.items);
        }
    }
    levels
}

/// The bar's static menus of each screen in each language: a letter each (Format, Slide, Arrange and Tools have none: they are not
/// Alt menus), no two alike, not D or J, and the label shows it.
fn bar_unique() {
    let _on = access_key::override_supported(true);
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        for (lang_name, lang) in LANGUAGES {
            i18n::show(lang);
            let ids: Vec<_> = s.with(|e, _| layout(e.kind()).iter().map(|spec| spec.id).collect());
            let mut letters: Vec<(char, String)> = Vec::new();
            for id in ids {
                let Some(letter) = id.access_letter() else { continue };
                let shown = BarMenu::Static(id).display();
                let place = format!("{name} {lang_name} bar menu {}", id.key());
                assert!(letter != 'd' && letter != 'j', "{place}: Alt+{letter} is the Draw button's / the coloured menus' key");
                assert!(letter.is_ascii_lowercase(), "{place}: the letter {letter:?} is not a lower-case ASCII letter");
                assert_eq!(shown.key, Some(letter), "{place}: the label {:?} does not carry the letter", shown.text);
                let range = shown.underline.clone().unwrap_or_else(|| panic!("{place}: no underline"));
                assert_eq!(shown.text[range].to_ascii_lowercase(), letter.to_string(), "{place}: the underline is not on {letter:?} in {:?}", shown.text);
                if let Some((_, other)) = letters.iter().find(|(l, _)| *l == letter) {
                    panic!("{name} {lang_name}: the bar's menus {other} and {} both have Alt+{letter}", id.key());
                }
                letters.push((letter, id.key().to_string()));
            }
            assert!(letters.len() >= 5, "{name} {lang_name}: the bar has its Alt menus: {letters:?}");
        }
        i18n::show(Language::Japanese);
    }
}

/// Bring a screen to the selections that show coloured menus; the names of the states reached.
fn states(s: &mut Scenario, file: &str) -> Vec<&'static str> {
    match file {
        f if f.ends_with("docx") => {
            let (id, _) = s.paragraph(2);
            s.click_text(ibis_doc_model::FlowPos::new(id, 0));
            s.update(|e, _, cx| e.insert_table(2, 2, cx));
            s.settle();
            vec!["caret in a table"]
        }
        f if f.ends_with("xlsx") => {
            s.key("cmd-a");
            vec!["all cells selected"]
        }
        _ => {
            let mut names = Vec::new();
            for (name, is_kind) in [("a shape selected", (|k: &ShapeKind| matches!(k, ShapeKind::Auto(a) if a.text.is_some())) as fn(&ShapeKind) -> bool), ("a table selected", |k| matches!(k, ShapeKind::Table(_)))] {
                let found = s.with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| is_kind(&sh.kind)).map(|j| (i + 1, j))));
                if let Some((page, shape)) = found {
                    s.page(page);
                    s.click_shape(shape);
                    names.push(name);
                }
            }
            names
        }
    }
}

/// Every level of every menu, in every language, in the states: nothing selected and the selections that bring the coloured menus.
fn every_menu_level() {
    let _on = access_key::override_supported(true);
    let mut levels = 0;
    let check_state = |s: &mut Scenario, name: &str, state: &str, levels: &mut usize| {
        for (lang_name, lang) in LANGUAGES {
            i18n::show(lang);
            let place = format!("{name} {lang_name} [{state}]");
            let (n, draw_lines) = s.with(|e, _| {
                let bar = e.menu_bar_all();
                let mut n = 0;
                // The ▾ holds the same menus the bar shows, so these levels are the ▾'s too.
                for (id, entries) in &bar.statics.menus {
                    n += walk(&format!("{place} menu {}", id.key()), entries);
                }
                for (context, entries) in &bar.contexts {
                    n += walk(&format!("{place} coloured menu {}", context.key()), entries);
                }
                n += walk(&format!("{place} Draw button's menu"), &bar.draw);
                (n, e.title_menu_entries(TitleMenu::Draw).len())
            });
            assert!(draw_lines > 0, "{place}: the Draw button's menu has lines");
            *levels += n;
        }
    };
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        check_state(&mut s, name, "nothing selected", &mut levels);
        for state in states(&mut s, file) {
            check_state(&mut s, name, state, &mut levels);
        }
        // The pop-up menus the presentation screen builds without a pointer (the panels' right-click menus).
        if file.ends_with("pptx") {
            for (lang_name, lang) in LANGUAGES {
                i18n::show(lang);
                levels += s.with(|e, cx| {
                    let place = format!("{name} {lang_name} pop-up");
                    walk(&format!("{place} notes"), &e.notes_context_menu_entries(cx))
                        + walk(&format!("{place} notes format"), &e.notes_format_context_menu_entries())
                        + walk(&format!("{place} properties"), &e.properties_panel_context_menu_entries(None))
                        + walk(&format!("{place} animation pane"), &e.animation_pane_context_menu_entries(0))
                });
            }
        }
        i18n::show(Language::Japanese);
    }
    assert!(levels > 100, "the walk reached the menus' levels: {levels}");
}

/// The rules the checker relies on: marks win over automatic letters, in both languages' forms, and off (macOS) means no key.
fn resolution_rules() {
    let r = resolve(&["Save", "Sa&ve", "Save &As"], true);
    assert_eq!(r.iter().map(|r| r.key).collect::<Vec<_>>(), vec![Some('s'), Some('v'), Some('a')]);
    // Japanese: `(&X)` shows `(X)` with X underlined; a mark takes its letter before the table's.
    let r = resolve(&["保存(&A)", "追加"], true); // i18n-ok (a label to resolve, in a test of the markup)
    assert_eq!((r[0].key, r[0].text.as_str()), (Some('a'), "保存(A)")); // i18n-ok (a label to resolve, in a test of the markup)
    assert_ne!(r[1].key, Some('a'), "追加 would take A, which the mark has"); // i18n-ok (a label to resolve, in a test of the markup)
    assert_eq!(marked_letter("保存(&S)"), Some('s')); // i18n-ok (a label to resolve, in a test of the markup)
    assert_eq!(marked_letter("&Save"), Some('s'));
    assert_eq!(marked_letter("R&D"), None);
    assert_eq!(marked_letter("A && B"), None);
    let r = resolve(&["A&b", "&B"], false);
    assert!(r.iter().all(|r| r.key.is_none() && r.underline.is_none()), "off (macOS): no key, no underline");
}

fn open_index(s: &mut Scenario) -> Option<usize> {
    s.with(|e, _| e.open_menu.as_ref().map(|o| o.index()))
}

/// Alt+E opens Edit; with the menu open, a letter key runs the line that has it (A is Select All on the three screens: the menu closes),
/// a letter no line has does nothing, and Alt+letter is not a line's key.
fn item_keys_run() {
    let _on = access_key::override_supported(true);
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        s.size(1600., 900.);
        s.key("alt-e");
        assert!(open_index(&mut s).is_some(), "{name}: Alt+E opens Edit");
        s.key("1");
        assert!(open_index(&mut s).is_some(), "{name}: a key no line has leaves the menu open");
        s.key("a");
        assert!(open_index(&mut s).is_none(), "{name}: A runs Select All and closes the menu");
        // A submenu opens: Insert > T (Table on the document and the sheet screens) is a submenu or a line; the menu stays or closes,
        // but the key is taken (the document does not get the letter).
        s.key("alt-n");
        assert!(open_index(&mut s).is_some(), "{name}: Alt+N opens Insert");
        s.key("escape");
    }
}

/// Where the OS has no access keys (macOS, the Web) no line gets a key or a changed label.
fn items_off_on_mac() {
    let _off = access_key::override_supported(false);
    let mut s = Scenario::open(FILES[0].1);
    let menus = s.with(|e, _| e.menu_bar_all());
    for (_, entries) in &menus.statics.menus {
        let mut level = entries.clone();
        assign_access_keys(&mut level);
        for entry in &level {
            match entry {
                MenuEntry::Item(i) => assert!(i.access.is_none(), "{}: no key off", i.id),
                MenuEntry::Submenu(sub) => assert!(sub.access.is_none(), "{}: no key off", sub.id),
                _ => {}
            }
        }
    }
}
