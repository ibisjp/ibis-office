// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The menus' hierarchy, as a file (menu bar redesign, stage 8; `tasks/doc/menu-redesign/implementation-spec.md` section 8, 「ダンプ」):
//! `tasks/doc/<screen>-menu-tree.txt` has, for nothing selected, a shape selected and the caret in a table, the old menus' tree
//! (`menus()`: every command the screen defines, which the menu search and the shortcuts use) and the menu bar's tree of the redesign
//! (`menu_bar_all()`), each line with its depth, id, label, shortcut and state. The old tree is what the redesign's tables of moved
//! and removed commands are checked against (the ids files have no parents). It is written only when `IBIS_DUMP_MENU_IDS` is set.
//! `<screen>-menu-tree-release.txt` is the same with the planned lines hidden (what is distributed).
use ibis_doc_model::ShapeKind;

use super::Scenario;
use crate::docx::scenarios::DocSteps;
use crate::shell::chrome::{MenuEntry, Menus};
use crate::docx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("shell::menu_dump_tree", dump_tree)]
}

fn walk(entries: &[MenuEntry], depth: usize, out: &mut String) {
    for entry in entries {
        match entry {
            MenuEntry::Item(i) => {
                let state = format!("{}{}", if i.enabled() { "" } else { " [off]" }, if i.checked { " [on]" } else { "" });
                out.push_str(&format!("{}{}\t{}\t{}{state}\n", "  ".repeat(depth), i.id, i.label, i.shortcut.unwrap_or("")));
            }
            MenuEntry::Submenu(s) => {
                out.push_str(&format!("{}{}\t{} >\n", "  ".repeat(depth), s.id, s.label));
                walk(&s.items, depth + 1, out);
            }
            MenuEntry::Separator => out.push_str(&format!("{}--\n", "  ".repeat(depth))),
            _ => {}
        }
    }
}

fn tree(menus: &Menus, out: &mut String) {
    for (id, entries) in &menus.menus {
        out.push_str(&format!("[{}]\n", id.key()));
        walk(entries, 1, out);
    }
}

fn state(s: &mut Scenario, name: &str, out: &mut String) {
    out.push_str(&format!("\n## {name}\n\n# menus() (what the screen defines)\n"));
    s.with(|e, _| tree(&e.menus(), out));
    out.push_str("\n# the menu bar\n");
    s.with(|e, _| {
        let bar = e.menu_bar_all();
        tree(&bar.statics, out);
        for (context, entries) in &bar.contexts {
            out.push_str(&format!("[{}]\n", context.key()));
            walk(entries, 1, out);
        }
    });
}

fn write(file: &str, text: &str) {
    assert!(text.lines().count() > 200, "{file}: the tree has lines: {}", text.lines().count());
    if std::env::var_os("IBIS_DUMP_MENU_IDS").is_some() {
        std::fs::write(crate::shell::scenario::repo_root().join("tasks/doc").join(file), text).unwrap();
    }
}

/// The trees as a development build shows them (`<screen>-menu-tree.txt`: the planned lines are grey lines), then as what is distributed
/// shows them (`<screen>-menu-tree-release.txt`: the planned lines are hidden, `menu_layout::show_planned`).
fn dump_tree() {
    crate::shell::menu_layout::with_planned(true, || dump_trees(""));
    crate::shell::menu_layout::with_planned(false, || dump_trees("-release"));
}

fn dump_trees(suffix: &str) {
    // The document: nothing selected, then the caret in a table.
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let mut text = String::new();
    state(&mut s, "nothing selected", &mut text);
    let (id, _) = s.paragraph(2);
    s.click_text(ibis_doc_model::FlowPos::new(id, 0));
    s.update(|e, _, cx| e.insert_table(2, 2, cx));
    s.settle();
    state(&mut s, "the caret in a table", &mut text);
    write(&format!("docx-menu-tree{suffix}.txt"), &text);
    drop(s);
    // The sheet: nothing selected, then everything.
    let mut s = Scenario::open("test/editing/xlsx-compatibility-v1.xlsx");
    let mut text = String::new();
    state(&mut s, "nothing selected", &mut text);
    s.key("cmd-a");
    state(&mut s, "all cells selected", &mut text);
    write(&format!("xlsx-menu-tree{suffix}.txt"), &text);
    drop(s);
    // The presentation: nothing selected, a text shape, a table.
    let mut s = Scenario::open("test/benchmark/earnings-presentation.pptx");
    let mut text = String::new();
    state(&mut s, "nothing selected", &mut text);
    for (name, is_kind) in [("a shape selected", (|k: &ShapeKind| matches!(k, ShapeKind::Auto(a) if a.text.is_some())) as fn(&ShapeKind) -> bool), ("a table selected", |k| matches!(k, ShapeKind::Table(_)))] {
        let found = s.with(|e, _| e.presentation().slides.iter().enumerate().find_map(|(i, sl)| sl.shapes.iter().position(|sh| is_kind(&sh.kind)).map(|j| (i + 1, j))));
        if let Some((page, shape)) = found {
            s.page(page);
            s.click_shape(shape);
            state(&mut s, name, &mut text);
        }
    }
    write(&format!("pptx-menu-tree{suffix}.txt"), &text);
}
