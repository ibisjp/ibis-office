// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the menus: no menu of the menu bar and no drop-down of the
//! toolbar has a submenu with nothing in it (no items, or only separators).
//! The menus as the shell and the formats build them are listed first
//! (`menus_unpruned`); a submenu that is empty there must be one of the
//! ids in `the hidden lists` (not yet made; the menu leaves it out), and the
//! menus as drawn have none.
use ibis_doc_model::ShapeKind;

use crate::shell::chrome::{MenuEntry, ToolItem};
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::menus_docx_no_empty_submenu", menus_docx_no_empty_submenu),
        ("shell::menus_docx_no_dead_items", menus_docx_no_dead_items),
        ("shell::menus_xlsx_no_dead_items", menus_xlsx_no_dead_items),
        ("shell::menus_pptx_no_dead_items", menus_pptx_no_dead_items),
        ("shell::menus_xlsx_no_empty_submenu", menus_xlsx_no_empty_submenu),
        ("shell::menus_pptx_no_empty_submenu", menus_pptx_no_empty_submenu),
        ("shell::menus_pptx_selected_no_empty_submenu", menus_pptx_selected_no_empty_submenu),
    ]
}

/// Submenus that have no items yet (a feature that is not made): the menu
/// leaves them out. Take an id out of here when its items are made.
/// Excel has no table menu (it is the item 表), nor list and spacing for cells.
const DOCX_HIDDEN: &[&str] = &[];
const XLSX_HIDDEN: &[&str] = &["insert.table", "format.spacing", "format.lists"];
const PPTX_HIDDEN: &[&str] = &[];

fn is_blank(entries: &[MenuEntry]) -> bool {
    entries.iter().all(|e| matches!(e, MenuEntry::Separator))
}

/// The ids of the submenus with nothing in them, in `entries`.
fn empty_ids(entries: &[MenuEntry], out: &mut Vec<&'static str>) {
    for entry in entries {
        if let MenuEntry::Submenu(sub) = entry {
            if is_blank(&sub.items) {
                out.push(sub.id);
            }
            empty_ids(&sub.items, out);
        }
    }
}

fn toolbar_menus(items: Vec<ToolItem>) -> Vec<Vec<MenuEntry>> {
    items
        .into_iter()
        .filter_map(|item| match item {
            ToolItem::Button(b) => b.menu,
            ToolItem::Select { menu, .. } => Some(menu),
            _ => None,
        })
        .collect()
}

/// Check the open document's menus: `what` names the state.
fn check(s: &mut Scenario, what: &str, hidden: &[&str]) {
    let (raw, shown, tools) = s.with(|e, _| {
        let mut raw = Vec::new();
        for (_, entries) in &e.menus_unpruned().menus {
            empty_ids(entries, &mut raw);
        }
        let mut shown = Vec::new();
        for (_, entries) in &e.menus().menus {
            empty_ids(entries, &mut shown);
        }
        let mut tools = Vec::new();
        for menu in toolbar_menus(e.toolbar_items()) {
            empty_ids(&crate::shell::chrome::prune_entries(menu.clone()), &mut tools);
            let mut raw_tool = Vec::new();
            empty_ids(&menu, &mut raw_tool);
            raw.extend(raw_tool);
        }
        (raw, shown, tools)
    });
    assert!(shown.is_empty(), "{what}: the menu bar draws empty submenus {shown:?}");
    assert!(tools.is_empty(), "{what}: a toolbar drop-down draws empty submenus {tools:?}");
    let unknown: Vec<_> = raw.iter().filter(|id| !hidden.contains(id)).collect();
    assert!(unknown.is_empty(), "{what}: submenus with nothing in them (make them, or list them in the hidden list above): {unknown:?}");
}

fn menus_docx_no_empty_submenu() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    check(&mut s, "docx", DOCX_HIDDEN);
}

fn menus_xlsx_no_empty_submenu() {
    let mut s = Scenario::open("test/editing/xlsx-compatibility-v1.xlsx");
    check(&mut s, "xlsx", XLSX_HIDDEN);
}

fn menus_pptx_no_empty_submenu() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    check(&mut s, "pptx, nothing selected", PPTX_HIDDEN);
}

/// Text, a picture, a chart, a table, and two shapes together selected.
fn menus_pptx_selected_no_empty_submenu() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let kinds: [(&str, fn(&ShapeKind) -> bool); 4] = [
        ("text", |k| matches!(k, ShapeKind::Auto(a) if a.text.is_some())),
        ("picture", |k| matches!(k, ShapeKind::Picture(_))),
        ("chart", |k| matches!(k, ShapeKind::Chart(_))),
        ("table", |k| matches!(k, ShapeKind::Table(_))),
    ];
    for (name, is_kind) in kinds {
        let found = s.with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| is_kind(&sh.kind)).map(|j| (i + 1, j))));
        let Some((page, shape)) = found else {
            eprintln!("menus: no {name} in the file");
            continue;
        };
        s.page(page);
        s.click_shape(shape);
        check(&mut s, &format!("pptx, a {name} selected"), PPTX_HIDDEN);
    }
    s.key("cmd-a");
    check(&mut s, "pptx, everything selected", PPTX_HIDDEN);
}

/// Items of the menu bar that are shown but do nothing: no handler (the menu draws them greyed for good), not disabled by
/// the state. A menu item shown with a shortcut that is not bound is the same kind of fault (⌘⌥F of 脚注 was). The ids in the lists are the
/// ones known not to be made yet (take one out when it is); a new one fails the scenario. How to find them in another format: open a
/// file of it, list `e.menus().menus` with `dead_items`, and put what is not made into the list (2026-10-19).
const DOCX_DEAD: &[&str] = &["file.export.svg", "file.language", "edit.duplicate", "insert.video", "insert.audio", "format.options", "help.help", "help.shortcuts"];

fn dead_items(entries: &[MenuEntry], out: &mut Vec<&'static str>) {
    for entry in entries {
        match entry {
            MenuEntry::Item(i) if i.handler.is_none() && !i.disabled => out.push(i.id),
            MenuEntry::Submenu(sub) => dead_items(&sub.items, out),
            _ => {}
        }
    }
}

fn check_dead(s: &mut Scenario, what: &str, known: &[&str]) {
    let dead = s.with(|e, _| {
        let mut out = Vec::new();
        for (_, entries) in &e.menus().menus {
            dead_items(entries, &mut out);
        }
        out
    });
    let new: Vec<_> = dead.iter().filter(|id| !known.contains(id)).collect();
    assert!(new.is_empty(), "{what}: menu items shown but with no handler (make them, or list them as not made): {new:?}");
    let made: Vec<_> = known.iter().filter(|id| !dead.contains(id)).collect();
    assert!(made.is_empty(), "{what}: these items work now, take them out of the list: {made:?}");
}

fn menus_docx_no_dead_items() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    check_dead(&mut s, "docx", DOCX_DEAD);
}

/// xlsx: the items not made yet (2026-10-19; the rest were connected: 書式に合わせて貼り付け, 書式のコピーと貼り付け, 保護, 区切り位置,
/// ユーザー補助). The scenario fails for a new one, and for one that works now (take it out).
const XLSX_DEAD: &[&str] = &["file.export.svg", "file.language", "edit.duplicate", "insert.video", "insert.audio", "insert.equation", "format.options", "help.help", "help.shortcuts"];

/// pptx: not checked yet (the list is for its owner to go through: many are handled by other means, such as a selection's
/// context menu); the list is written to `target/menus-dead-<format>.txt`.
fn menus_xlsx_no_dead_items() {
    let mut s = Scenario::open("test/editing/xlsx-compatibility-v1.xlsx");
    check_dead(&mut s, "xlsx", XLSX_DEAD);
}

/// Items of the pptx menus that are made in no state: the intersection of the dead items of nothing selected, a text shape
/// selected, text being edited, a picture, a chart, a table, and everything selected (an item that only a selection connects is
/// not dead). Not made yet; take an id out of the list when it is.
const PPTX_DEAD: &[&str] = &["file.export.svg", "file.language", "help.help", "help.shortcuts"];
/// Items only a state this check does not reach connects: an animation chosen in the animation pane, a vertex selected while the edit
/// points are on, a character format copied.
const PPTX_STATE_ONLY: &[&str] = &["pptx.anim.options-dialog", "pptx.edit-points.smooth", "pptx.edit-points.straight", "pptx.edit-points.corner", "edit.paste-format"];

fn menus_pptx_no_dead_items() {
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let dead_now = |s: &mut Scenario| {
        s.with(|e, _| {
            let mut out = Vec::new();
            for (_, entries) in &e.menus().menus {
                dead_items(entries, &mut out);
            }
            out
        })
    };
    let mut states: Vec<(String, Vec<&'static str>)> = vec![("nothing selected".into(), dead_now(&mut s))];
    let kinds: [(&str, fn(&ShapeKind) -> bool); 4] = [
        ("text", |k| matches!(k, ShapeKind::Auto(a) if a.text.is_some())),
        ("picture", |k| matches!(k, ShapeKind::Picture(_))),
        ("chart", |k| matches!(k, ShapeKind::Chart(_))),
        ("table", |k| matches!(k, ShapeKind::Table(_))),
    ];
    for (name, is_kind) in kinds {
        let found = s.with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| is_kind(&sh.kind)).map(|j| (i + 1, j))));
        let Some((page, shape)) = found else { continue };
        s.page(page);
        s.click_shape(shape);
        states.push((format!("a {name} selected"), dead_now(&mut s)));
        if name == "text" {
            s.double_click_shape(shape);
            states.push(("text being edited".into(), dead_now(&mut s)));
            s.key("escape");
            s.key("escape");
        }
    }
    // A shape with its edit points on (the smooth, straight and corner items).
    let auto = s.with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Auto(a) if a.text.is_none())).map(|j| (i + 1, j))));
    if let Some((page, shape)) = auto {
        s.page(page);
        s.click_shape(shape);
        let editable = s.with(|e, _| e.menus().find_mut("pptx.edit-points").is_some_and(|i| i.enabled()));
        if editable {
            s.menu("pptx.edit-points");
            states.push(("edit points".into(), dead_now(&mut s)));
            s.key("escape");
        }
    }
    s.key("cmd-a");
    states.push(("everything selected".into(), dead_now(&mut s)));
    // Two shapes grouped, the group selected.
    let grouped = s.with(|e, _| e.menus().find_mut("pptx.group").is_some_and(|i| i.enabled()));
    if grouped {
        s.menu("pptx.group");
        states.push(("a group selected".into(), dead_now(&mut s)));
    }
    // A SmartArt graphic made here (inserted, so selected).
    let insert_item = s.with(|e, _| e.menus().find_mut("pptx.insert.smartart.process").is_some_and(|i| i.enabled()));
    if insert_item {
        s.menu("pptx.insert.smartart.process");
        states.push(("a SmartArt graphic inserted".into(), dead_now(&mut s)));
    }
    // A SmartArt graphic of a file selected (the first scenario goes first: one scenario at a time).
    drop(s);
    let mut smart = Scenario::open("test/editing/pptx-smartart.pptx");
    let graphic = smart.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Placeholder(ibis_doc_model::Placeholder::Diagram))));
    if let Some(graphic) = graphic {
        smart.click_shape(graphic);
        states.push(("a SmartArt graphic selected".into(), dead_now(&mut smart)));
        // One of its shapes (its items), as the SmartArt items of the menus work on.
        let part = smart.with(|e, _| e.slide().attached.get(&e.slide().shapes[graphic].id).and_then(|p| p.first().copied()));
        if let Some(part) = part {
            smart.update(|e, _, _| e.set_selection(vec![part]));
            states.push(("an item of a SmartArt graphic selected".into(), dead_now(&mut smart)));
        }
    }
    // Dead in every state.
    let mut never: Vec<&'static str> = states[0].1.clone();
    never.retain(|id| !PPTX_STATE_ONLY.contains(id));
    for (_, dead) in &states[1..] {
        never.retain(|id| dead.contains(id));
    }
    let mut report = String::new();
    for (what, dead) in &states {
        report.push_str(&format!("{what}: {}\n", dead.len()));
    }
    let _ = std::fs::write(crate::shell::scenario::repo_root().join("target/menus-dead-pptx.txt"), format!("{report}\nnever made: {never:?}\nper state:\n{states:?}\n"));
    let new: Vec<_> = never.iter().filter(|id| !PPTX_DEAD.contains(id)).collect();
    assert!(new.is_empty(), "pptx: menu items shown but with no handler in any state (make them, or list them as not made): {new:?}");
    let made: Vec<_> = PPTX_DEAD.iter().filter(|id| !never.contains(id)).collect();
    assert!(made.is_empty(), "pptx: these items work now, take them out of the list: {made:?}");
}
