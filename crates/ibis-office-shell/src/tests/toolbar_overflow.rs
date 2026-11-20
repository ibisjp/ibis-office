// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0047: what the toolbar's buttons do is the same whether they are drawn in the toolbar or in the row ︙ opens when the window is too
//! narrow for them. The scenarios press the buttons where they are drawn (never through their handlers), in both places.
use super::{Scenario, button_probe as probe};

pub fn all() -> Vec<(&'static str, fn())> {
    vec![("shell::toolbar_overflow_split_button_menu_works", split_button_menu), ("shell::toolbar_overflow_every_button_works", every_button_works), ("shell::button_probe_dry_run_ends_with_its_probe", dry_run_ends_with_its_probe)]
}

/// A dry run belongs to the probe that asked for it: the next probe presses for real (the menu grid scenarios that followed `toolbar_overflow_every_button_works` in the full run pressed Enter
/// in a dry run and inserted nothing, while each passed alone).
fn dry_run_ends_with_its_probe() {
    {
        let _probe = probe::Probe::new();
        assert!(!probe::dispatched(), "a probe starts with its presses run");
        probe::dry_run(true);
        assert!(probe::dispatched(), "the dry run asked for is in force");
    }
    let _next = probe::Probe::new();
    assert!(!probe::dispatched(), "the next probe is not a dry run");
}

/// The id of a drawn toolbar part `id`: `<id>-<index>` with the index its place gives (the main bar's, or 1000 and more in the row of ︙).
fn drawn(id: &str, suffix: &str) -> Option<String> {
    probe::all().into_iter().find(|k| k.starts_with(&format!("{id}-")) && k.ends_with(suffix) && k[id.len() + 1..k.len() - suffix.len()].chars().all(|c| c.is_ascii_digit()))
}

/// 罫線's ▼ then 格子 (the borders of all the cells' sides): done from the main bar, and from the row ︙ opens in a narrow window.
fn split_button_menu() {
    for (width, narrow) in [(1400., false), (520., true)] {
        let _probe = probe::Probe::new();
        let mut s = Scenario::open_sized("test/editing/xlsx-sales.xlsx", width);
        s.settle();
        if narrow {
            probe::click(&mut s, "toolbar-more-button");
            s.settle();
        }
        let caret_dbg = |s: &mut Scenario| format!("window {width}, caret {:?}, more {:?}", probe::bounds("borders-1012-menu"), probe::bounds("toolbar-more-button"));
        let _ = &s;
        let caret = drawn("borders", "-menu").unwrap_or_else(|| panic!("the borders ▼ is not drawn (narrow: {narrow}): {:?}", probe::all()));
        let done = s.with(|e, _| e.document.can_undo());
        probe::click(&mut s, &caret);
        s.settle();
        let items: Vec<String> = probe::all().into_iter().filter(|k| k.starts_with("menu-item-")).collect();
        assert!(!items.is_empty(), "the ▼ opened no menu (narrow: {narrow}); {}", caret_dbg(&mut s));
        probe::click(&mut s, "menu-item-xlsx.border-all");
        s.settle();
        assert!(s.with(|e, _| e.document.can_undo()) != done, "the all-borders choice did nothing (narrow: {narrow})");
    }
}

/// The drawn id of a toolbar part of the id `id`: in the main bar (a small index), or in the row of ︙ (1000 and more).
fn drawn_in(id: &str, suffix: &str, overflow: bool) -> Option<String> {
    probe::all().into_iter().find(|k| {
        k.starts_with(&format!("{id}-"))
            && k.ends_with(suffix)
            && k[id.len() + 1..k.len() - suffix.len()].parse::<usize>().is_ok_and(|n| (n >= 1000) == overflow)
    })
}

/// A command of every part of the toolbar, pressed where it is drawn: the press must reach the one place commands are run from
/// (`chrome::dispatch`). Every button with a command, the ▼ of every split button and of every button with a menu, and the first
/// choice of each menu, in a window wide enough for the whole toolbar and in one too narrow for it (the row ︙ opens). The commands are
/// counted and left undone (some open the system's dialogs). What this finds is a button that is drawn where no press reaches it
/// (outside the window, under another thing), or that a press does not run. A button added to the toolbar is in this at once.
fn every_button_works() {
    use crate::chrome::{MenuEntry, ToolItem};
    // (the file, the shape picked first (a presentation), the format's name)
    let states: [(&str, Option<usize>); 6] = [
        ("test/editing/xlsx-sales.xlsx", None),
        ("test/editing/docx-report.docx", None),
        ("test/editing/88-review-order.pptx", None),
        ("test/editing/88-review-order.pptx", Some(0)),
        ("test/editing/88-review-order.pptx", Some(1)),
        ("test/editing/88-review-order.pptx", Some(2)),
    ];
    let mut failures: Vec<String> = Vec::new();
    let mut pressed = 0;
    for (path, pick) in states {
        for (width, overflow) in [(3200., false), (520., true)] {
            let _probe = probe::Probe::new();
            probe::dry_run(true);
            let mut s = Scenario::open_sized(path, width);
            s.settle();
            if let Some(i) = pick {
                if s.with(|e, _| e.slide().shapes.len()) <= i {
                    continue;
                }
                s.select_shape(i);
                s.settle();
            }
            let items: Vec<ToolItem> = s.update(|e, _, _| e.toolbar_items());
            let label = format!("{path} pick {pick:?} {}", if overflow { "narrow (︙)" } else { "wide" });
            for item in &items {
                let (id, menu, split, enabled) = match item {
                    ToolItem::Button(b) if !b.hidden => (b.id, b.menu.clone().unwrap_or_default(), b.menu.is_some() && b.left_action().is_some(), b.enabled()),
                    ToolItem::Select { id, menu, disabled, .. } => (*id, menu.clone(), false, !disabled && !menu.is_empty()),
                    _ => continue,
                };
                if !enabled || id == "toggle-menus" || id == "menu-search" {
                    continue;
                }
                // Where it is drawn: open the row of ︙ if it is not in the bar.
                let mut place = drawn_in(id, "", overflow).or_else(|| drawn_in(id, "", !overflow));
                if place.is_none() && probe::bounds("toolbar-more-button").is_some() {
                    probe::click(&mut s, "toolbar-more-button");
                    s.settle();
                    place = drawn_in(id, "", true);
                }
                let Some(place) = place else {
                    failures.push(format!("{label}: {id} is not drawn"));
                    continue;
                };
                let has_menu = !menu.is_empty();
                let before = probe::dispatch_count();
                // The button itself: its command (a split button's left half), or its menu opening.
                let ids_before: Vec<String> = probe::all();
                probe::click(&mut s, &place);
                s.settle();
                pressed += 1;
                if !has_menu || split {
                    if probe::dispatch_count() == before && !has_menu {
                        failures.push(format!("{label}: pressing {id} ({place}) ran nothing; at {:?}, dry {}", probe::bounds(&place), probe::dispatch_count()));
                    }
                    if split {
                        if probe::dispatch_count() == before {
                            failures.push(format!("{label}: pressing the icon of {id} ({place}) ran nothing"));
                        }
                    } else {
                        continue;
                    }
                }
                // The menu: the ▼ of a split button, then the first choice that is a plain item.
                if split {
                    let Some(caret) = drawn_in(id, "-menu", overflow).or_else(|| drawn_in(id, "-menu", !overflow)) else {
                        failures.push(format!("{label}: the ▼ of {id} is not drawn"));
                        continue;
                    };
                    probe::click(&mut s, &caret);
                    s.settle();
                }
                let first = menu.iter().find_map(|e| if let MenuEntry::Item(i) = e { i.enabled().then_some(i.id) } else { None });
                let Some(first) = first else {
                    s.key("escape");
                    continue;
                };
                // The rows the press opened: a menu's (`menu-item-<id>`), or a list's (`searchable-item-…`, numbered). The choice is the item the
                // menu starts with when it is drawn, else the first row that runs something.
                let mut rows: Vec<String> = probe::all().into_iter().filter(|k| !ids_before.contains(k) && (k.starts_with("menu-item-") || k.contains("searchable-item-"))).collect();
                if rows.is_empty() {
                    failures.push(format!("{label}: pressing {id} ({place}) opened no menu or list"));
                    s.key("escape");
                    continue;
                }
                // Only the rows of choices that are available can be pressed to run something.
                let available: Vec<&str> = crate::chrome::flatten_menu_entries(&menu).iter().filter(|i| i.enabled()).map(|i| i.id).collect::<Vec<_>>().into_iter().map(|id| &*Box::leak(id.to_string().into_boxed_str())).collect();
                rows.retain(|k| !k.starts_with("menu-item-") || available.contains(&&k["menu-item-".len()..]));
                rows.sort_by_key(|k| (k.as_str() != format!("menu-item-{first}"), k.len(), k.clone()));
                if rows.is_empty() {
                    // Nothing in it is available (or only below the fold of a long menu): nothing to press.
                    s.key("escape");
                    continue;
                }
                let before_item = probe::dispatch_count();
                for row in rows.iter().take(6) {
                    if probe::bounds(row).is_none() {
                        break; // The menu closed (the press ran something, or only closed it).
                    }
                    probe::click(&mut s, row);
                    s.settle();
                    if probe::dispatch_count() != before_item {
                        break;
                    }
                    // Not a row that runs something (a heading): the list is still open, the next row.
                }
                // A menu whose rows are the items of a submenu (opened by its own row) is not walked further here: the rows seen are the
                // submenu's, which may all be unavailable for what is selected.
                let only_submenus = menu.iter().any(|e| matches!(e, MenuEntry::Submenu(_))) && rows.iter().all(|k| k.starts_with("menu-item-") && !menu.iter().any(|e| matches!(e, MenuEntry::Item(i) if format!("menu-item-{}", i.id) == *k)));
                if probe::dispatch_count() == before_item && !only_submenus {
                    failures.push(format!("{label}: choosing a row of the menu of {id} ({place}) ran nothing; tried {:?}; entries {}", &rows[..rows.len().min(6)], menu.iter().map(|e| match e { MenuEntry::Item(i) => format!("item:{}:{}:{}", i.id, i.enabled(), i.handler.is_some()), MenuEntry::Submenu(_) => "sub".into(), MenuEntry::Separator => "sep".into(), MenuEntry::Palette(_) => "palette".into(), MenuEntry::Tiles(_) => "tiles".into() }).collect::<Vec<_>>().join(",")));
                    s.key("escape");
                }
            }
        }
    }
    assert!(pressed > 50, "the buttons were pressed: {pressed}");
    assert!(failures.is_empty(), "{} of {pressed} presses failed:\n{}", failures.len(), failures.join("\n"));
}
