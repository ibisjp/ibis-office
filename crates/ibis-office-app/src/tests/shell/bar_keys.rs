// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the menu bar's keys (menu bar redesign, stage 8, `tasks/doc/menu-redesign`; Windows and Linux): the access letters are
//! assigned once per screen and the labels carry them, a Mac-like setting shows and does nothing, Alt+letter opens the menu that has
//! the letter (also one folded into the ▾), Alt+J goes round the coloured menus, F10 puts the focus on the bar and the arrows,
//! Enter and Esc work it.
use std::collections::HashSet;

use super::Scenario;
use crate::docx::scenarios::DocSteps;
use crate::shell::access_key;
use crate::shell::chrome::MenuId;
use crate::shell::menu_bar::{BarMenu, OVERFLOW_IX};
use crate::docx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::menu_access_keys_unique_per_screen", unique_per_screen),
        ("shell::menu_access_keys_off_on_mac", off_on_mac),
        ("shell::menu_alt_letter_opens", alt_letter_opens),
        ("shell::menu_alt_j_cycles_contexts", alt_j_cycles),
        ("shell::menu_f10_focuses_bar", f10_focuses_bar),
        ("shell::menu_overflow_alt_key", overflow_alt_key),
    ]
}

const FILES: [(&str, &str); 3] = [
    ("docx", "test/editing/docx-report.docx"),
    ("xlsx", "test/editing/xlsx-compatibility-v1.xlsx"),
    ("pptx", "test/benchmark/earnings-presentation.pptx"),
];

fn open_index(s: &mut Scenario) -> Option<usize> {
    s.with(|e, _| e.open_menu.as_ref().map(|o| o.index()))
}

fn index_of(s: &mut Scenario, id: MenuId) -> usize {
    s.with(|e, _| e.bar_items().iter().position(|(m, _)| *m == BarMenu::Static(id)).unwrap())
}

/// Each static menu of a screen's bar has a letter and no two share one; the label shows it (the letter is in the English name, or
/// `(X)` is put after the name), and the underline is on that letter.
fn unique_per_screen() {
    let _on = access_key::override_supported(true);
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        let displays: Vec<(MenuId, access_key::Resolved)> = s.with(|e, _| {
            e.bar_items().iter().filter_map(|(m, _)| if let BarMenu::Static(id) = m { Some((*id, m.display())) } else { None }).collect()
        });
        let mut letters = HashSet::new();
        for (id, shown) in &displays {
            let key = shown.key.unwrap_or_else(|| panic!("{name}: {id:?} has no access key"));
            assert!(letters.insert(key), "{name}: the letter {key} is used twice: {displays:?}");
            let range = shown.underline.clone().expect("the letter to underline");
            assert_eq!(shown.text[range].to_ascii_lowercase(), key.to_string(), "{name}: the underline is on the key's letter in {}", shown.text);
        }
    }
}

/// A Mac-like setting: no key, no underline, the plain label, and Alt+letter does nothing.
fn off_on_mac() {
    let _off = access_key::override_supported(false);
    let mut s = Scenario::open(FILES[0].1);
    let shown = s.with(|e, _| e.bar_items().iter().map(|(m, _)| m.display()).collect::<Vec<_>>());
    assert!(shown.iter().all(|r| r.key.is_none() && r.underline.is_none()), "no keys and no underline: {shown:?}");
    s.key("alt-e");
    s.key("f10");
    assert!(open_index(&mut s).is_none() && s.with(|e, _| e.bar_keys.focus.is_none()), "no access key opens a menu and F10 does nothing");
}

fn alt_letter_opens() {
    let _on = access_key::override_supported(true);
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        s.size(1600., 900.);
        let edit = index_of(&mut s, MenuId::Edit);
        s.key("alt-e");
        assert_eq!(open_index(&mut s), Some(edit), "{name}: Alt+E opens the Edit menu");
        let insert = index_of(&mut s, MenuId::Insert);
        s.key("alt-n");
        assert_eq!(open_index(&mut s), Some(insert), "{name}: Alt+N opens the Insert menu (another menu that was open gives way)");
        s.key("escape");
        assert!(open_index(&mut s).is_none(), "{name}: Esc closes it");
        s.key("ctrl-alt-e");
        assert!(open_index(&mut s).is_none(), "{name}: Ctrl+Alt+E is not Alt+E");
    }
}

/// With a table in the document, Table Design and Table Layout are coloured menus: Alt+J opens the first, again the next, then round; with
/// none, nothing happens.
fn alt_j_cycles() {
    let _on = access_key::override_supported(true);
    let mut s = Scenario::open(FILES[0].1);
    s.size(1600., 900.);
    s.key("alt-j");
    assert!(open_index(&mut s).is_none(), "no coloured menu: Alt+J does nothing");
    // A table (2 x 2) inserted at the caret makes its menus appear.
    let (id, _) = s.paragraph(2);
    s.click_text(ibis_doc_model::FlowPos::new(id, 0));
    s.update(|e, _, cx| e.insert_table(2, 2, cx));
    s.settle();
    let contexts: Vec<usize> = s.with(|e, _| e.bar_items().iter().enumerate().filter(|(_, (m, _))| matches!(m, BarMenu::Context(_))).map(|(ix, _)| ix).collect());
    assert!(contexts.len() >= 2, "a table's caret shows two coloured menus: {contexts:?}");
    s.key("alt-j");
    assert_eq!(open_index(&mut s), Some(contexts[0]), "the first press opens the first coloured menu");
    s.key("alt-j");
    assert_eq!(open_index(&mut s), Some(contexts[1]), "the next press opens the next");
    s.key("alt-j");
    assert_eq!(open_index(&mut s), Some(contexts[0]), "and round again");
}

/// F10 gives the bar the focus (on the first menu); → moves along the bar in the order it is drawn, ← back, ↓ opens the menu the focus is
/// on, Esc leaves.
fn f10_focuses_bar() {
    let _on = access_key::override_supported(true);
    let mut s = Scenario::open(FILES[0].1);
    s.size(1600., 900.);
    s.key("f10");
    let first = s.with(|e, _| e.bar_keys.focus);
    assert!(first.is_some(), "F10 puts the focus on the bar");
    s.key("right");
    let second = s.with(|e, _| e.bar_keys.focus);
    assert_ne!(first, second, "→ moves on");
    s.key("left");
    assert_eq!(s.with(|e, _| e.bar_keys.focus), first, "← moves back");
    s.key("left");
    let last = s.with(|e, _| e.bar_keys.focus);
    let order = s.with(|e, w| { let _ = w; e.bar_items().len() });
    assert!(last.is_some() && order > 2, "← from the first wraps to the last (Help)");
    s.key("right");
    assert_eq!(s.with(|e, _| e.bar_keys.focus), first);
    s.key("down");
    assert_eq!(open_index(&mut s), first, "↓ opens the menu the focus is on");
    assert!(s.with(|e, _| e.bar_keys.focus.is_none()), "the focus is the menu's now");
    s.key("escape");
    s.key("f10");
    s.key("escape");
    assert!(s.with(|e, _| e.bar_keys.focus.is_none()), "Esc gives the focus back");
}

/// In a narrow window the Review menu is folded into the ▾; Alt+R opens the ▾ (where it is). F10 and → reach the ▾ before Help.
fn overflow_alt_key() {
    let _on = access_key::override_supported(true);
    let mut s = Scenario::open(FILES[0].1);
    s.size(300., 800.);
    s.settle();
    s.key("alt-r");
    assert_eq!(open_index(&mut s), Some(OVERFLOW_IX), "Alt+R opens the ▾ that holds Review");
    s.key("escape");
    s.key("f10");
    let mut reached = false;
    for _ in 0..20 {
        if s.with(|e, _| e.bar_keys.focus) == Some(OVERFLOW_IX) {
            reached = true;
            break;
        }
        s.key("right");
    }
    assert!(reached, "F10 then → reaches the ▾");
}
