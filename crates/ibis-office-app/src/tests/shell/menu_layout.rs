// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the redesigned menu bar's layout (menu bar redesign stages 1 and 2, `tasks/doc/menu-redesign`): the rows of a
//! menu, no command shown twice, no empty submenu, every command of the open document reached by the bar or the
//! menu search, the access letters, the words of the menus, and a picture of every menu of the three screens.
use std::collections::HashSet;

use crate::shell::chrome::{MenuEntry, MenuId};
use crate::shell::menu_layout::with_planned;
use crate::shell::scenario::Scenario;
use crate::docx::ext::*;
use crate::xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::menu_layout_rows_within_limit", rows_within_limit),
        ("shell::menu_layout_docx_edit_starts_with_asian_layout", docx_edit_starts_with_asian_layout),
        ("shell::menu_layout_no_duplicate_ids", no_duplicate_ids),
        ("shell::menu_layout_no_empty_submenu", no_empty_submenu),
        ("shell::menu_layout_commands_reachable", commands_reachable),
        ("shell::menu_layout_words_and_letters", words_and_letters),
        ("shell::menu_layout_no_unreplaced_placeholders", no_unreplaced_placeholders),
        ("shell::menu_layout_paste_special_submenu_is_the_dialog", paste_special_submenu_is_the_dialog),
        ("shell::menu_layout_sheet_menus", sheet_menus),
        ("shell::menu_layout_order", order),
        ("shell::menu_layout_items_match_old_menus", items_match_old_menus),
        ("shell::menu_layout_review_comment_lines", review_comment_lines),
        ("shell::menu_layout_menu_words_searchable", menu_words_searchable),
        ("shell::menu_layout_tied_lines", tied_lines),
        ("shell::menu_layout_selection_entries", selection_entries),
        ("shell::menu_layout_screenshots", screenshots),
        // The same checks on what is distributed (the planned lines hidden, `menu_layout::show_planned`).
        ("shell::menu_planned_hidden_has_no_planned_line", planned_hidden_has_no_planned_line),
        ("shell::menu_planned_hidden_rows_within_limit", || with_planned(false, rows_within_limit)),
        ("shell::menu_planned_hidden_no_duplicate_ids", || with_planned(false, no_duplicate_ids)),
        ("shell::menu_planned_hidden_no_empty_submenu", || with_planned(false, no_empty_submenu)),
        ("shell::menu_planned_hidden_commands_reachable", || with_planned(false, commands_reachable)),
        ("shell::menu_planned_hidden_words_and_letters", || with_planned(false, words_and_letters)),
        ("shell::menu_planned_hidden_order", || with_planned(false, order)),
        ("shell::menu_planned_hidden_tied_lines", || with_planned(false, tied_lines)),
        ("shell::menu_planned_hidden_selection_entries", || with_planned(false, selection_entries)),
    ]
}

const FILES: [(&str, &str); 3] = [
    ("docx", "test/editing/docx-report.docx"),
    ("xlsx", "test/editing/xlsx-compatibility-v1.xlsx"),
    ("pptx", "test/benchmark/earnings-presentation.pptx"),
];

/// The most rows a menu may have at its top level (separators and the lines inside submenus not counted).
const MAX_ROWS: usize = 25;

/// The redesign allows these to be in two menus (comment, cross-reference, page break).
const ALLOWED_TWICE: &[&str] = &["pptx.anim.preview", "docx.page-break", "docx.xref", "docx.quick-parts.prop.comments", "insert.comment", "xlsx.protect-sheet"];

fn rows(entries: &[MenuEntry]) -> usize {
    entries.iter().filter(|e| !matches!(e, MenuEntry::Separator)).count()
}

fn ids(entries: &[MenuEntry], out: &mut Vec<&'static str>) {
    for entry in entries {
        match entry {
            MenuEntry::Item(item) => out.push(item.id),
            MenuEntry::Submenu(sub) => {
                out.push(sub.id);
                ids(&sub.items, out);
            }
            _ => {}
        }
    }
}

fn rows_within_limit() {
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        let counts = s.with(|e, _| e.menu_bar_menus().menus.iter().map(|(id, entries)| (id.key(), rows(entries))).collect::<Vec<_>>());
        for (menu, n) in &counts {
            assert!(*n <= MAX_ROWS, "{name}: the menu {menu} has {n} rows (at most {MAX_ROWS}): {counts:?}");
        }
    }
}

fn no_duplicate_ids() {
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        let (all_ids, mut labels) = s.with(|e, _| {
            let menus = e.menu_bar_menus();
            let mut all_ids = Vec::new();
            let mut labels = Vec::new();
            for (id, entries) in &menus.menus {
                ids(entries, &mut all_ids);
                let mut seen = HashSet::new();
                for entry in entries {
                    let label = match entry {
                        MenuEntry::Item(item) => item.label.to_string(),
                        MenuEntry::Submenu(sub) => sub.label.to_string(),
                        _ => continue,
                    };
                    if !seen.insert(label.clone()) {
                        labels.push(format!("{}: {label}", id.key()));
                    }
                }
            }
            (all_ids, labels)
        });
        let mut seen = HashSet::new();
        // `pptx.anim.more-effects` is in each of the animation submenus the old menu had (not a layout duplicate).
        let twice: Vec<_> = all_ids.iter().filter(|id| !seen.insert(**id) && !ALLOWED_TWICE.contains(id) && **id != "pptx.anim.more-effects").collect();
        assert!(twice.is_empty(), "{name}: shown more than once in the bar: {twice:?}");
        // The design has a group and a line of the same name: Review's Comments (new, delete, next, previous) and Data's Data Validation.
        labels.retain(|l| l != "menu.review: コメント" && l != "menu.data: データの入力規則"); // i18n-ok（試験：設計の言葉）
        assert!(labels.is_empty(), "{name}: the same label twice in one menu: {labels:?}");
    }
}

fn no_empty_submenu() {
    fn empty(entries: &[MenuEntry], out: &mut Vec<&'static str>) {
        for entry in entries {
            if let MenuEntry::Submenu(sub) = entry {
                if sub.items.iter().all(|e| matches!(e, MenuEntry::Separator)) {
                    out.push(sub.id);
                }
                empty(&sub.items, out);
            }
        }
    }
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        let found = s.with(|e, _| {
            let mut out = Vec::new();
            for (_, entries) in &e.menu_bar_menus().menus {
                empty(entries, &mut out);
            }
            out
        });
        assert!(found.is_empty(), "{name}: empty submenus in the bar: {found:?}");
    }
}

/// Every command of the open document is in the bar or `menu_hidden`; the hidden ones that can run are found by the menu search.
fn commands_reachable() {
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        s.with(|e, _| {
            let (menus, reg) = e.menu_bar_menus_and_registry();
            let mut shown = Vec::new();
            for (_, entries) in &menus.menus {
                ids(entries, &mut shown);
            }
            // The Draw button's menu and the sidebars (which show their own commands under `sidebar.*` ids) are menus too.
            ids(&e.title_menu_entries(crate::shell::menu_layout::TitleMenu::Draw), &mut shown);
            shown.extend(crate::shell::sidebars::pane_only_commands(e.kind()));
            let shown: HashSet<_> = shown.into_iter().collect();
            for c in &reg.commands {
                assert!(shown.contains(c.id) != c.menu_hidden, "{name}: {} is both shown and hidden, or neither (shown {}, hidden {})", c.id, shown.contains(c.id), c.menu_hidden);
            }
            let searchable: HashSet<_> = e.command_search_items().into_iter().map(|i| i.id).collect();
            let lost: Vec<_> = reg.hidden().filter(|c| c.enabled && c.run.is_some() && !searchable.contains(c.id)).map(|c| c.id).collect();
            assert!(lost.is_empty(), "{name}: not shown in the bar and not found by the menu search: {lost:?}");
            assert!(!reg.commands.is_empty(), "{name}: no commands");
        });
    }
}

fn words_and_letters() {
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        s.with(|e, _| {
            let menus = e.menu_bar_menus();
            let mut letters = HashSet::new();
            for (id, entries) in &menus.menus {
                let label = id.label().to_string();
                assert!(!label.is_empty() && !label.starts_with("menu."), "{name}: the name of {id:?} is not in the tables: {label}");
                let letter = id.access_letter().unwrap_or_else(|| panic!("{name}: {id:?} has no access letter"));
                assert!(letters.insert(letter), "{name}: the access letter {letter} of {id:?} is used twice in the bar");
                fn check(name: &str, entries: &[MenuEntry]) {
                    for entry in entries {
                        match entry {
                            // A line may carry the key of its words (`LayoutItem::CmdAs`, a submenu's own `label_key`, `menu.as.*`): the words are what the key gives.
                            // (Other labels are not looked up: a window's line is a document's name.)
                            MenuEntry::Item(item) => {
                                let words = if item.label.starts_with("menu.") { crate::shell::i18n::text(&item.label) } else { item.label.clone() };
                                assert!(!words.is_empty() && !words.starts_with("menu."), "{name}: {} has no words: {}", item.id, item.label)
                            }
                            MenuEntry::Submenu(sub) => {
                                let words = if sub.label.starts_with("menu.") { crate::shell::i18n::text(&sub.label) } else { sub.label.clone() };
                                assert!(!words.is_empty() && !words.starts_with("menu."), "{name}: submenu {} has no words: {}", sub.id, sub.label);
                                check(name, &sub.items);
                            }
                            _ => {}
                        }
                    }
                }
                check(name, entries);
            }
        });
    }
}

/// No line of any menu (the bar, the selection's context menus, the Draw menu, and every command of the table, shown or hidden) has
/// an unreplaced `{name}` placeholder of the i18n tables in its words (an item made from a message with `{count}` showed it raw).
fn no_unreplaced_placeholders() {
    /// `{count}`: braces around a name of the tables (a label that is the brace symbols themselves, `{ }`, is not one).
    fn has_placeholder(text: &str) -> bool {
        text.split('{').skip(1).any(|rest| rest.split('}').next().is_some_and(|name| rest.contains('}') && !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')))
    }
    fn check(name: &str, place: &str, entries: &[MenuEntry], bad: &mut Vec<String>) {
        for entry in entries {
            match entry {
                MenuEntry::Item(item) => {
                    for text in [crate::shell::chrome::menu_item_display_label(item).to_string(), item.shortcut.unwrap_or_default().to_string()] {
                        if has_placeholder(&text) {
                            bad.push(format!("{name}: {place}: {} = {text}", item.id));
                        }
                    }
                }
                MenuEntry::Submenu(sub) => {
                    let label = crate::shell::i18n::text(&sub.label);
                    if has_placeholder(&label) {
                        bad.push(format!("{name}: {place}: {} = {label}", sub.id));
                    }
                    check(name, place, &sub.items, bad);
                }
                _ => {}
            }
        }
    }
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        let bad = s.with(|e, _| {
            let mut bad = Vec::new();
            let (bar, reg) = e.menu_bar_menus_and_registry();
            for (id, entries) in &bar.menus {
                check(name, id.key(), entries, &mut bad);
            }
            for (ctx, entries) in &e.menu_bar_all().contexts {
                check(name, ctx.key(), entries, &mut bad);
            }
            check(name, "draw", &e.title_menu_entries(crate::shell::menu_layout::TitleMenu::Draw), &mut bad);
            for c in &reg.commands {
                if let Some(entry) = reg.entry(c.id) {
                    check(name, "command table", std::slice::from_ref(entry), &mut bad);
                }
            }
            bad
        });
        assert!(bad.is_empty(), "unreplaced placeholders in menu words: {bad:#?}");
    }
}

/// 編集 > 形式を選択して貼り付け: the kinds of paste in the submenu are the dialog's 貼り付け list (the same ids, words and order), and no
/// command that is not a paste (the 選択 submenu's 数式, a pivot table's 値) is among them.
fn paste_special_submenu_is_the_dialog() {
    let mut s = Scenario::open("test/editing/xlsx-compatibility-v1.xlsx");
    s.with(|e, _| {
        let menus = e.menu_bar_menus();
        let (_, edit) = menus.menus.iter().find(|(id, _)| *id == MenuId::Edit).expect("the Edit menu");
        let Some(MenuEntry::Submenu(sub)) = edit.iter().find(|v| matches!(v, MenuEntry::Submenu(sub) if sub.id == "xlsx.paste-special")) else { panic!("no paste-special submenu") };
        let mut items: Vec<(&'static str, String)> = Vec::new();
        for entry in &sub.items {
            if let MenuEntry::Item(item) = entry {
                items.push((item.id, crate::shell::chrome::menu_item_display_label(item).to_string()));
            }
        }
        let kinds = crate::xlsx::paste_special_dialog::PASTE_TYPE_OPTIONS;
        let in_menu: Vec<_> = items.iter().filter(|(id, _)| kinds.iter().any(|(_, _, k)| k == id)).cloned().collect();
        let in_dialog: Vec<_> = kinds.iter().map(|(_, label, id)| (*id, crate::shell::i18n::text(label).to_string())).collect();
        assert_eq!(in_menu, in_dialog, "the submenu's kinds of paste and the dialog's");
        let ids: Vec<_> = items.iter().map(|(id, _)| *id).collect();
        for id in ["xlsx.select-formulas", "xlsx.pivot-select-values"] {
            assert!(!ids.contains(&id), "{id} is not a paste: {ids:?}");
        }
    });
}

/// 編集 of a spreadsheet: 新しいシート is the first line of the シート submenu (not of セルの挿入), and コピー is the first line of the コピー submenu.
fn sheet_menus() {
    let mut s = Scenario::open("test/editing/xlsx-compatibility-v1.xlsx");
    s.with(|e, _| {
        let menus = e.menu_bar_menus();
        let sub_ids = |menu: MenuId, sub_id: &str| -> Vec<&'static str> {
            let (_, entries) = menus.menus.iter().find(|(id, _)| *id == menu).expect("the menu");
            let Some(MenuEntry::Submenu(sub)) = entries.iter().find(|v| matches!(v, MenuEntry::Submenu(sub) if sub.id == sub_id)) else { panic!("no {sub_id}") };
            let mut out = Vec::new();
            ids(&sub.items, &mut out);
            out
        };
        assert_eq!(sub_ids(MenuId::Edit, "xlsx.sheet-menu").first(), Some(&"xlsx.insert-sheet"), "the new sheet opens the sheet submenu");
        assert!(!sub_ids(MenuId::Edit, "xlsx.insert-cells-menu").contains(&"xlsx.insert-sheet"), "and is not among the cells' insertions");
        assert!(sub_ids(MenuId::Edit, "xlsx.insert-cells-menu").contains(&"xlsx.insert-row-above"), "which are the rows, columns and cells");
        let copy = sub_ids(MenuId::Edit, "menu.group.copy");
        assert_eq!(copy, ["edit.copy", "xlsx.copy-as-picture"], "copy, then copy as picture");
        // 挿入: テキスト ボックス opens the group of 画像・図形・SmartArt; テキスト is in the group of コントロール and メディア.
        let (_, insert) = menus.menus.iter().find(|(id, _)| *id == MenuId::Insert).expect("the Insert menu");
        let at = |id: &str| insert.iter().position(|v| matches!(v, MenuEntry::Item(i) if i.id == id) || matches!(v, MenuEntry::Submenu(s) if s.id == id)).unwrap_or_else(|| panic!("no {id} in the Insert menu"));
        let no_separator = |a: usize, b: usize| !insert[a.min(b)..a.max(b)].iter().any(|v| matches!(v, MenuEntry::Separator));
        assert!(at("insert.text-box") + 1 == at("insert.image") && no_separator(at("insert.text-box"), at("insert.smartart")), "the text box is the first line of the pictures-shapes-SmartArt group");
        assert!(at("menu.group.text") < at("menu.group.controls") && no_separator(at("menu.group.text"), at("menu.group.media")), "the text submenu is in the group of the controls and the media");
        assert!(!no_separator(at("insert.smartart"), at("menu.group.text")), "and that is another group than the pictures");
    });
}

/// The design's Review menu has a line 新しいコメント (the command `insert.comment`, whose words in Insert are コメント) and a submenu コメント ▸ (previous and
/// next comment ...): the line runs the command under its own words, the submenu is a group of its own, so that the two lines of one level do not stand
/// for one id (and one access key). The Insert menu's line keeps the command's words.
fn review_comment_lines() {
    for (name, file) in [FILES[0], FILES[2]] {
        let mut s = Scenario::open(file);
        s.with(|e, _| {
            let menus = e.menu_bar_menus();
            let find = |menu: MenuId| &menus.menus.iter().find(|(id, _)| *id == menu).unwrap_or_else(|| panic!("{name}: no {menu:?} menu")).1;
            let review = find(MenuId::Review);
            let line = review.iter().find_map(|v| match v { MenuEntry::Item(i) if i.id == "insert.comment" => Some(i), _ => None }).unwrap_or_else(|| panic!("{name}: no new-comment line in Review"));
            assert_eq!(crate::shell::i18n::text(&line.label).as_ref(), crate::shell::i18n::text("review.new-comment").as_ref(), "{name}: the Review line has its own words");
            let sub = review.iter().find_map(|v| match v { MenuEntry::Submenu(s) if s.id == "review.comments" => Some(s), _ => None }).unwrap_or_else(|| panic!("{name}: no comments submenu in Review"));
            assert_eq!(crate::shell::i18n::text(&sub.label).as_ref(), crate::shell::i18n::text("review.comments").as_ref());
            assert!(!review.iter().any(|v| matches!(v, MenuEntry::Submenu(s) if s.id == "insert.comment")), "{name}: the submenu does not stand for the command's id");
            let insert = find(MenuId::Insert);
            let there = insert.iter().find_map(|v| match v { MenuEntry::Item(i) if i.id == "insert.comment" => Some(i), _ => None }).unwrap_or_else(|| panic!("{name}: no comment line in Insert"));
            assert_eq!(crate::shell::i18n::text(&there.label).as_ref(), crate::shell::i18n::text("insert.comment").as_ref(), "{name}: Insert keeps the command's words");
        });
    }
}

/// The lines that show a command under the menu's own words (`tasks/doc/menu-redesign/menu-wording-2.tsv`, keys `menu.as.*`): the menu search
/// finds each by those words and by the command's own words, both running the same command.
fn menu_words_searchable() {
    fn walk(entries: &[MenuEntry], out: &mut Vec<(&'static str, String)>) {
        for entry in entries {
            match entry {
                MenuEntry::Item(item) if item.label.starts_with("menu.as.") => out.push((item.id, item.label.to_string())),
                MenuEntry::Submenu(sub) => walk(&sub.items, out),
                _ => {}
            }
        }
    }
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        s.with(|e, _| {
            let mut lines = Vec::new();
            for (_, entries) in &e.menu_bar_menus().menus {
                walk(entries, &mut lines);
            }
            assert!(lines.len() >= 10, "{name}: only {} lines have the menu's own words", lines.len());
            // Every line the layout gives the menu's own words to shows them (a command only inside an old submenu too).
            fn keys(items: &[crate::shell::menu_layout::LayoutItem], out: &mut Vec<&'static str>) {
                use crate::shell::menu_layout::LayoutItem;
                for item in items {
                    match item {
                        LayoutItem::CmdAs { label_key, .. } if label_key.starts_with("menu.as.") => out.push(label_key),
                        LayoutItem::Sub { label_key, items, .. } => {
                            if label_key.starts_with("menu.as.") {
                                out.push(label_key);
                            }
                            keys(items, out);
                        }
                        _ => {}
                    }
                }
            }
            fn shown(entries: &[MenuEntry], out: &mut HashSet<String>) {
                for entry in entries {
                    match entry {
                        MenuEntry::Item(item) => {
                            out.insert(item.label.to_string());
                        }
                        MenuEntry::Submenu(sub) => {
                            out.insert(sub.label.to_string());
                            shown(&sub.items, out);
                        }
                        _ => {}
                    }
                }
            }
            let mut want = Vec::new();
            for spec in crate::shell::menu_layout::layout(e.kind()) {
                keys(spec.items, &mut want);
            }
            let mut have = HashSet::new();
            for (_, entries) in &e.menu_bar_menus().menus {
                shown(entries, &mut have);
            }
            let lost: Vec<_> = want.iter().filter(|k| !have.contains(**k)).collect();
            assert!(lost.is_empty(), "{name}: lines of the layout with the menu's own words that the bar does not show under them: {lost:?}");
            let found = e.command_search_items();
            let mut searched = 0;
            for (id, key) in &lines {
                let words = crate::shell::i18n::text(key);
                let rows: Vec<_> = found.iter().filter(|i| i.id == *id).collect();
                if rows.is_empty() {
                    continue; // grey now (nothing to act on): the search lists no line of it under any words
                }
                searched += 1;
                assert!(rows.iter().any(|i| i.label == words), "{name}: the menu search does not find {id} by the menu's words {words}");
                // A grey line (the command cannot run now) is not listed by the search at all; one that runs is one row (the menu's
                // words and path) that the command's own words find too (its search text), not a second row (2026-11-13, the user).
                assert_eq!(rows.len(), 1, "{name}: {id} is listed once");
                assert!(rows[0].search_alt.as_ref().is_some_and(|alt| alt.as_ref() as &str != words.as_ref() as &str), "{name}: the menu search does not find {id} by the command's own words");
            }
            assert!(searched >= 5, "{name}: only {searched} lines with the menu's own words can run now (the check sees too little)");
        });
    }
}

/// The bar's menus are in the order of the redesign (a menu with nothing to show may be missing).
fn order() {
    use MenuId::*;
    let expected: [(&str, Vec<MenuId>); 3] = [
        ("docx", vec![File, Edit, Insert, Design, Layout, References, Mailings, Review, View, Window, Help]),
        ("xlsx", vec![File, Edit, Insert, PageLayout, Formulas, Data, Review, View, Window, Help]),
        ("pptx", vec![File, Edit, Insert, Design, Transitions, Animations, SlideShow, Review, View, Window, Help]),
    ];
    for ((name, file), (_, want)) in FILES.into_iter().zip(expected) {
        let mut s = Scenario::open(file);
        let got: Vec<MenuId> = s.with(|e, _| e.menu_bar_menus().menus.iter().map(|(id, _)| *id).collect());
        let mut it = want.iter();
        assert!(got.iter().all(|g| it.any(|w| w == g)), "{name}: the bar's menus {got:?} are not in the order {want:?}");
        for must in [File, Edit, Insert, View, Window, Help] {
            assert!(got.contains(&must), "{name}: the bar has no {must:?}");
        }
    }
}

/// A picture of every menu of the three screens, open: `target/scenario/<name>/NN-<screen>-<menu>.png`.
fn screenshots() {
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        s.size(1400., 900.);
        let keys = s.with(|e, _| e.menu_bar_menus().menus.iter().map(|(id, _)| id.key()).collect::<Vec<_>>());
        for (ix, key) in keys.iter().enumerate() {
            s.hover((0., 0.));
            s.update(move |e, w, cx| e.open_menu_at(ix, w, cx));
            s.settle();
            s.screenshot(&format!("{name}-{}", key.trim_start_matches("menu.")));
        }
    }
}

/// Words of the table's lines (the appendix's) and the words of the old definition of the id the layout gives them differ, and the difference
/// was looked at and is right (the appendix says 「エクスポート」 for what the old menu called 「書き出し」, and so on). A new difference is not
/// in this list and fails: the line then points at a command that does something else (the 「メディア ▸ ビデオ」 of the presentation screen
/// pointed at `pptx.action.movie`, the video action button that draws a shape, instead of `insert.video`; the transitions' サウンド at
/// `pptx.action.sound`, the sound action button; a sheet's オート SUM at the status bar's summaries).
const WORDS_REVIEWED: &[&str] = &[
    "file.copy", "file.export", "docx.insert.draw-table", "insert.video", "insert.audio", "docx.caption.figure", "docx.toa", "docx.picture.replace",
    "file.start-window", "xlsx.open-and-repair", "xlsx.insert-sheet", "xlsx.filter", "xlsx.gridlines", "xlsx.picture.reset", "xlsx.chart-format",
    "pptx.autorecover-settings", "pptx.slide.add-hidden-title", "pptx.notes-header-footer", "pptx.background.custom", "pptx.background", "pptx.theme-colors",
    "pptx.transition", "view.slideshow", "pptx.skip-slide", "pptx.check-accessibility", "pptx.inspect-document", "pptx.grid-view", "pptx.fill.custom",
    "pptx.shadow", "pptx.text.warp", "pptx.default-shape", "pptx.smartart.add-shape", "pptx.video.reset-effects", "pptx.video.styles",
    "pptx.clip.caption.add", "file.info", "edit.find-replace", "insert.symbol", "docx.index", "tools.spelling", "tools.dictionary", "docx.picture.ratio",
    "docx.picture.size", "pptx.chart.data-quick", "xlsx.theme-palette", "xlsx.theme-fonts", "xlsx.theme-effects", "xlsx.sort", "xlsx.formula-bar",
    "xlsx.headings", "window.arrange-all", "xlsx.picture.remove-background", "xlsx.chart.save-template", "pptx.insert-object", "pptx.theme",
    "pptx.slide-size", "slideshow.rehearse-timings", "pptx.proofing-language", "pptx.review.compare", "pptx.normal-view", "pptx.frame.settings",
    "pptx.smartart.change-layout", "xlsx.autosum", "xlsx.autosum.average", "xlsx.autosum.count", "xlsx.autosum.max", "xlsx.autosum.min",
    "transition.sound", "transition.on-click", "pptx.anim.duration", "pptx.clip.play", "pptx.layout.master.rename", "pptx.rotate.right",
    "pptx.rotate.left", "pptx.rotate.flip-v", "pptx.rotate.flip-h", "docx.page.direction.horizontal", "insert.comment", "tools.settings", "help.help",
    "insert.link", "insert.table", "transition.none", "transition.fade", "transition.wipe", "transition.split", "xlsx.insert-symbol",
    "docx.gridlines", "xlsx.pivot-blank-rows", "xlsx.pivot-styles", "slideshow.custom-shows", "docx.comments.next", "docx.comments.previous",
    // The update check and log lines: the spec says 更新の確認… and 更新ログを表示, the registry's words are 更新を確認 and 更新のログを表示 (same
    // commands); docx.arrange-all is 並べて表示 in the registry. Reviewed: the registry's words stay. // i18n-ok (Japanese text under test)
    "help.check-update", "help.update-log", "docx.arrange-all",
];

fn norm_words(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && !"　…・（）()「」、。:：／/-.,▸>".contains(*c)).collect() // i18n-ok (Japanese text under test or a name table that matches labels)
}

/// Every line of the menus (`tasks/doc/menu-redesign/menu-items.tsv`, written by `scripts/gen-menu-layout.py` from the appendix and its
/// id fixes) that has an id: the bar of the screen has that command, and the old definition's words are the line's words (or the
/// difference is in `WORDS_REVIEWED`). The old menus' handlers are what a command runs, so an id behind a line is what it runs.
fn items_match_old_menus() {
    let Ok(table) = std::fs::read_to_string(crate::shell::scenario::repo_root().join("tasks/doc/menu-redesign/menu-items.tsv")) else {
        return; // The table is not in a checkout without tasks/.
    };
    fn labels(entries: &[MenuEntry], out: &mut std::collections::HashMap<&'static str, String>) {
        for entry in entries {
            match entry {
                MenuEntry::Item(i) => {
                    out.entry(i.id).or_insert_with(|| i.label.to_string());
                }
                MenuEntry::Submenu(s) => {
                    out.entry(s.id).or_insert_with(|| s.label.to_string());
                    labels(&s.items, out);
                }
                _ => {}
            }
        }
    }
    let mut failures = Vec::new();
    for ((name, file), screen) in FILES.into_iter().zip(["doc", "sheet", "slide"]) {
        let mut s = Scenario::open(file);
        let have = s.with(|e, _| {
            let bar = e.menu_bar_all();
            let mut have = std::collections::HashMap::new();
            labels(&bar.statics.menus.iter().flat_map(|(_, v)| v.clone()).collect::<Vec<_>>(), &mut have);
            for (_, entries) in &bar.contexts {
                labels(entries, &mut have);
            }
            have
        });
        for line in table.lines().skip(1) {
            let c: Vec<&str> = line.split('\t').collect();
            if c.len() < 6 || c[0] != screen || c[4].is_empty() {
                continue;
            }
            let (menu, path, words, id) = (c[1], c[2], c[3], c[4]);
            // The media group is the video and the audio file pickers, never an action button (which only draws a shape).
            if menu == "挿入" && path.starts_with("メディア▸") && !["insert.video", "insert.audio"].contains(&id) { // i18n-ok (Japanese text under test or a name table that matches labels)
                failures.push(format!("{name}: {menu} {path}: {id} is not the video or audio picker"));
            }
            let Some(label) = have.get(id) else { continue };
            let (a, b) = (norm_words(words), norm_words(label));
            if a != b && !a.contains(&b) && !b.contains(&a) && !WORDS_REVIEWED.contains(&id) {
                failures.push(format!("{name}: {menu} {path}: the line says 「{words}」 but {id} is 「{label}」")); // i18n-ok (Japanese text under test or a name table that matches labels)
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// What is distributed hides the planned lines (`menu_layout::show_planned`): in every menu of the bar, every context menu that shows
/// and the Draw button's menu there is no planned line, no submenu with nothing in it, and no separator at the start, at the end or
/// twice in a row; no menu of the bar is empty. A development build shows planned lines (so the hidden form is not hidden by chance).
fn planned_hidden_has_no_planned_line() {
    fn check(name: &str, place: &str, entries: &[MenuEntry], planned: &mut usize, bad: &mut Vec<String>) {
        if entries.is_empty() {
            bad.push(format!("{name}: {place} is empty"));
        }
        if matches!(entries.first(), Some(MenuEntry::Separator)) || matches!(entries.last(), Some(MenuEntry::Separator)) {
            bad.push(format!("{name}: {place} starts or ends with a separator"));
        }
        for pair in entries.windows(2) {
            if matches!(pair, [MenuEntry::Separator, MenuEntry::Separator]) {
                bad.push(format!("{name}: {place} has two separators in a row"));
            }
        }
        for entry in entries {
            match entry {
                MenuEntry::Item(item) if item.id.starts_with("menu.planned.") => *planned += 1,
                MenuEntry::Submenu(sub) => check(name, &format!("{place} > {}", sub.id), &sub.items, planned, bad),
                _ => {}
            }
        }
    }
    fn count(s: &mut Scenario, name: &str) -> (usize, Vec<String>) {
        s.with(|e, _| {
            let (mut planned, mut bad) = (0, Vec::new());
            let bar = e.menu_bar_all();
            for (id, entries) in &bar.statics.menus {
                check(name, id.key(), entries, &mut planned, &mut bad);
            }
            for (context, entries) in &bar.contexts {
                check(name, context.key(), entries, &mut planned, &mut bad);
            }
            check(name, "draw", &e.title_menu_entries(crate::shell::menu_layout::TitleMenu::Draw), &mut planned, &mut bad);
            (planned, bad)
        })
    }
    let mut shown_total = 0;
    for (name, file) in FILES {
        let mut s = Scenario::open(file);
        let menus_shown = with_planned(true, || s.with(|e, _| e.menu_bar_menus().menus.len()));
        shown_total += with_planned(true, || count(&mut s, name)).0;
        let (planned, bad) = with_planned(false, || count(&mut s, name));
        assert_eq!(planned, 0, "{name}: hidden, the bar has no planned line");
        assert!(bad.is_empty(), "{name}: hidden, the menus are tidy: {bad:?}");
        let menus_hidden = with_planned(false, || s.with(|e, _| e.menu_bar_menus().menus.len()));
        assert_eq!(menus_hidden, menus_shown, "{name}: hiding the planned lines takes no menu out of the bar");
    }
    assert!(shown_total > 0, "a development build shows the planned lines");
}

/// The bar entry of `path` (ids of the submenus, then of the line) in menu `menu`.
fn bar_entry(menus: &crate::shell::chrome::Menus, menu: MenuId, path: &[&str]) -> Option<MenuEntry> {
    let (_, mut entries) = menus.menus.iter().find(|(id, _)| *id == menu)?.clone();
    for (i, want) in path.iter().enumerate() {
        let entry = entries.iter().find(|v| matches!(v, MenuEntry::Item(it) if it.id == *want) || matches!(v, MenuEntry::Submenu(s) if s.id == *want))?.clone();
        if i + 1 == path.len() {
            return Some(entry);
        }
        let MenuEntry::Submenu(sub) = entry else { return None };
        entries = sub.items;
    }
    None
}

/// Lines of the design tied to commands that were there already (2026-11-11, `tasks/doc/menu-redesign/menu-gaps.tsv`): the line is at its
/// place in the bar (also with the planned lines hidden, as distributed: they are commands, not planned lines), and the lines this list runs
/// do what the command does. Issues: phonetic guides #170 #40, apply a layout #84 #85 #62, insert an equation #171
/// (distributed text #53 stays on the toolbar's alignment arrow, user decision 2026-11-11: `issue53_fit_text_entries`).
fn tied_lines() {
    use MenuId::*;
    let want: [(usize, Vec<(MenuId, Vec<&str>)>); 3] = [
        (0, vec![
            (Insert, vec!["menu.group.equation-group", "insert.equation"]),
            (Layout, vec!["menu.group.margins", "docx.page.margins-narrow"]),
            (References, vec!["docx.reference.bibliography.update"]),
            (References, vec!["docx.caption.insert"]),
            (References, vec!["menu.group.table-of-authorities", "docx.toa.update"]),
            (Review, vec!["draw.hide-ink"]),
            (Mailings, vec!["docx.merge.envelopes"]),
            (Mailings, vec!["docx.merge.labels"]),
            (Mailings, vec!["docx.merge.select-recipients"]),
            (Mailings, vec!["docx.merge.insert-field"]),
            (Mailings, vec!["docx.merge.preview"]),
        ]),
        (1, vec![
            (View, vec!["xlsx.phonetic-toggle"]),
            (View, vec!["xlsx.phonetic-edit"]),
            (Formulas, vec!["menu.group.function-library", "xlsx.datetime-functions", "xlsx.fn.today"]),
        ]),
        (2, vec![
            (Edit, vec!["menu.group.slides", "pptx.layout"]),
            (Design, vec!["pptx.background"]),
            (Review, vec!["draw.hide-ink"]),
        ]),
    ];
    for (at, lines) in want {
        let (name, file) = FILES[at];
        let mut s = Scenario::open(file);
        let menus = s.with(|e, _| e.menu_bar_menus());
        for (menu, path) in lines {
            assert!(bar_entry(&menus, menu, &path).is_some(), "{name}: no {path:?} in the {menu:?} menu");
        }
    }
    // The lines run their commands from the bar.
    let run = |s: &mut Scenario, menu: MenuId, path: &[&str]| {
        let menus = s.with(|e, _| e.menu_bar_menus());
        let Some(MenuEntry::Item(item)) = bar_entry(&menus, menu, path) else { panic!("no line {path:?}") };
        assert!(item.enabled(), "{path:?} is enabled");
        let handler = item.handler.clone().unwrap_or_else(|| panic!("{path:?} has a handler"));
        s.update(|e, window, cx| handler(e, window, cx));
    };
    let mut s = Scenario::open(FILES[1].1);
    run(&mut s, Formulas, &["menu.group.function-library", "xlsx.datetime-functions", "xlsx.fn.today"]);
    let text = s.with(|e, _| e.cell_edit().map(|c| c.text.clone())).unwrap_or_default();
    assert!(text.starts_with('=') && text.ends_with("TODAY("), "Date & Time > TODAY starts the function in the cell: {text}");
    // #39: ウィンドウ ▸ サイドバー ▸ セルの書式設定 opens the pane (it was grey in every state: format.options had no handler on a sheet).
    drop(s);
    let mut s = Scenario::open(FILES[1].1);
    run(&mut s, Window, &["menu.window.sidebar", "sidebar.cell-format"]);
    drop(s);
    let mut s = Scenario::open(FILES[0].1);
    // No table of authorities in the file: the update leaves the document as it is.
    let before = s.with(|e, _| e.flow().map(|d| d.body.len()));
    run(&mut s, References, &["menu.group.table-of-authorities", "docx.toa.update"]);
    assert_eq!(s.with(|e, _| e.flow().map(|d| d.body.len())), before, "nothing to update");
    // #14: ウィンドウ ▸ サイドバー ▸ テキスト ボックスの設定 runs docx.textbox.edit (it had no command: grey for ever, 「自動で開く」).
    let menus = s.with(|e, _| e.menu_bar_menus());
    let Some(MenuEntry::Item(line)) = bar_entry(&menus, Window, &["menu.window.sidebar", "sidebar.text-box"]) else { panic!("no text box sidebar line") };
    assert!(line.handler.is_some(), "the text box sidebar line runs a command");
    // 差し込み文書 ▸ 差し込みフィールドの挿入 opens the panel (at the first step while there are no recipients); 封筒… opens it at the choice
    // of the kind of document, envelopes chosen (the body is not replaced until the panel's own button; the choice is modal, so it goes last).
    run(&mut s, Mailings, &["docx.merge.insert-field"]);
    assert!(s.with(|e, _| e.doc_state().and_then(|st| st.merge.as_ref()).is_some_and(|m| m.visible)), "the panel is open");
    run(&mut s, Mailings, &["docx.merge.envelopes"]);
    let merge = s.with(|e, _| e.doc_state().and_then(|st| st.merge.as_ref()).map(|m| (m.visible, m.dialog == Some(crate::docx::merge_panel::MergeDialog::Kind), m.kind_draft)));
    assert_eq!(merge, Some((true, true, Some("envelopes"))), "the panel at the kind of document, envelopes chosen");
    assert_eq!(s.with(|e, _| e.flow().map(|d| d.body.len())), before, "the body stays until the panel's button");
    drop(s);
    let mut s = Scenario::open(FILES[0].1);
    let menus = s.with(|e, _| e.menu_bar_menus());
    // 差し込み文書: the way to the panel (差し込み印刷…) is the first line (user decision 2026-11-11: the panel is enough).
    let first = menus.menus.iter().find(|(id, _)| *id == Mailings).and_then(|(_, entries)| entries.first().cloned());
    assert!(matches!(&first, Some(MenuEntry::Item(it)) if it.id == "docx.merge" && it.enabled()), "Mail Merge… is the first line of the Mailings menu");
    // #53 (user decision 2026-11-11): 文字の均等割り付け is on the toolbar's alignment arrow, not in the menu bar; the menu search finds it.
    assert!(menus.menus.iter().all(|(_, entries)| crate::shell::scenario::find_item(entries, "docx.align.fit-text").is_none()), "fit text is not in the menu bar");
    let align = match s.toolbar_part("align") {
        Some(crate::shell::chrome::ToolItem::Button(b)) => b.menu.unwrap_or_default(),
        _ => panic!("no toolbar alignment button"),
    };
    assert!(crate::shell::scenario::find_item(&align, "docx.toolbar.fit-text").is_some(), "the toolbar's alignment arrow has fit text");
    let items = s.with(|e, _| e.searchable_menu_items_for("menu-search"));
    let found = crate::shell::chrome::menu_search_rows(&items, &crate::shell::i18n::text("align.distribute"), &[]);
    assert!(found.iter().any(|r| r.id == "docx.align.fit-text"), "the menu search finds fit text: {:?}", found.iter().map(|r| r.id).collect::<Vec<_>>());
}

/// The submenu `id` among `entries` (at any depth).
fn find_submenu(entries: &[MenuEntry], id: &str) -> Option<crate::shell::chrome::Submenu> {
    entries.iter().find_map(|entry| match entry {
        MenuEntry::Submenu(sub) if sub.id == id => Some(sub.clone()),
        MenuEntry::Submenu(sub) => find_submenu(&sub.items, id),
        _ => None,
    })
}

/// The ways to features that issues asked for and that show only with a selection (2026-11-11): the context menus of the bar (the coloured
/// ones), the right-click menu and the sidebar line, with that selection made.
/// #77: a text box's columns (`pptx.text.columns`) in 図形の書式 (and the right-click menu); #43: the chart's format pane
/// (`xlsx.chart-format`) in グラフの書式, the right-click menu and ウィンドウ ▸ サイドバー; #86: a video's play/pause (`pptx.clip.play`) in
/// ビデオ再生 and the right-click menu.
fn selection_entries() {
    use crate::shell::menu_context::ContextMenu;
    use crate::pptx::ext::*;
    let context = |s: &mut Scenario, menu: ContextMenu| -> Vec<MenuEntry> {
        let bar = s.with(|e, _| e.menu_bar_all());
        bar.contexts.into_iter().find(|(c, _)| *c == menu).map(|(_, entries)| entries).unwrap_or_else(|| panic!("the bar shows {menu:?} with this selection"))
    };
    let run = |s: &mut Scenario, item: &crate::shell::chrome::MenuItem| {
        assert!(item.enabled(), "{} is enabled", item.id);
        let handler = item.handler.clone().unwrap_or_else(|| panic!("{} has a handler", item.id));
        s.update(|e, window, cx| handler(e, window, cx));
    };
    // #77
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);
    s.click_shape(0);
    let format = context(&mut s, ContextMenu::ShapeFormat);
    let columns = find_submenu(&format, "pptx.text.columns").expect("Shape Format has the columns of the text");
    let two = crate::shell::scenario::find_item(&columns.items, "pptx.text.columns.2").expect("2 columns").clone();
    run(&mut s, &two);
    assert_eq!(s.with(|e, _| e.slide().shapes[0].text_body().map(|b| b.columns)), Some(2), "2 columns from the bar's Shape Format");
    assert!(find_submenu(&s.with(|e, _| e.shape_context_menu_entries()), "pptx.text.columns").is_some(), "the right-click menu has the columns");
    // #86
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let video = root.join("clip-test.mp4");
    let bytes = std::fs::read(&video).expect("clip-test.mp4");
    s.update(|e, window, cx| e.end_text_edit(window, cx));
    let clip = s.update(|e, _, cx| e.insert_clip(&video, bytes.clone(), false, cx)).expect("the clip is inserted");
    s.update(|e, _, _| e.set_selection(vec![clip]));
    let playback = context(&mut s, ContextMenu::VideoPlayback);
    let play = crate::shell::scenario::find_item(&playback, "pptx.clip.play").expect("Video Playback has play/pause").clone();
    let right = s.with(|e, _| e.shape_context_menu_entries());
    let tops: Vec<String> = right.iter().filter_map(|entry| match entry {
        MenuEntry::Item(it) => Some(it.id.to_string()),
        MenuEntry::Submenu(sub) => Some(sub.id.to_string()),
        _ => None,
    }).collect();
    let selected = s.with(|e, _| (e.selected_ids(), e.text_editing().is_some()));
    assert!(crate::shell::scenario::find_item(&right, "pptx.clip.play").is_some(), "the right-click menu has play/pause: {tops:?} {selected:?}");
    run(&mut s, &play);
    drop(s);
    // #43
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    let index = s.with(|e, _| e.slide().shapes.iter().position(|shape| matches!(shape.kind, ibis_doc_model::ShapeKind::Chart(_)))).expect("a chart");
    let at = s.shape_position(index);
    s.click(at);
    let chart = context(&mut s, ContextMenu::ChartFormat);
    let line = crate::shell::scenario::find_item(&chart, "xlsx.chart-format").expect("Chart Format has the format pane").clone();
    run(&mut s, &line);
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()), "the chart's format pane is open");
    assert!(crate::shell::scenario::find_item(&s.with(|e, _| e.shape_menu()), "xlsx.chart-format").is_some(), "the right-click menu has it");
    let menus = s.with(|e, _| e.menu_bar_menus());
    let Some(MenuEntry::Item(side)) = bar_entry(&menus, MenuId::Window, &["menu.window.sidebar", "sidebar.chart"]) else { panic!("no chart sidebar line") };
    assert!(side.enabled(), "Window > Sidebar > chart format is enabled with a chart chosen");
}

/// K0013 (2026-11-12): the document's Edit menu starts with 拡張書式 (Asian Layout), and 縦中横 is in it and works.
fn docx_edit_starts_with_asian_layout() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let edit = s.with(|e, _| e.menu_bar_menus().menus.into_iter().find(|(id, _)| *id == MenuId::Edit).map(|(_, entries)| entries)).expect("an Edit menu");
    let first = match edit.first() {
        Some(MenuEntry::Submenu(sub)) => sub.clone(),
        _ => panic!("the Edit menu starts with a submenu"),
    };
    assert_eq!(first.label.as_ref(), crate::shell::i18n::text("menu.group.asian-layout").as_ref(), "Asian Layout first");
    assert!(crate::shell::scenario::find_item(&first.items, "docx.text.tate-chu-yoko").is_some(), "tate-chu-yoko is in Asian Layout: {:?}", first.items.iter().map(|e| match e { MenuEntry::Item(i) => i.id, MenuEntry::Submenu(s) => s.id, _ => "-" }).collect::<Vec<_>>());
}
