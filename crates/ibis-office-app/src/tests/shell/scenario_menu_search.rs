// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the menu search's history ("履歴"/"コマンド", 2026-11-03), the scroll bar of the menus and
//! submenus, and the grid of pictures of the spreadsheet's 挿入 > グラフ submenu.
use ibis_doc_model::ShapeKind;
use ibis_pptx::Kind;

use crate::shell::chrome::{MenuEntry, MenuId, MENU_SEARCH_COMMANDS_HEADING, MENU_SEARCH_HISTORY_HEADING, MENU_SEARCH_HISTORY_ITEM, menu_search_rows};
use crate::shell::menu_search_history as history;
use crate::shell::scenario::{Scenario, button_probe as buttons};
use gpui_kit::*;
use crate::xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::menu_search_history_lru_per_kind_and_restart", menu_search_history_lru_per_kind_and_restart),
        ("shell::menu_search_history_rows_and_choice", menu_search_history_rows_and_choice),
        ("shell::menu_scrollbar_same_in_menu_and_submenu", menu_scrollbar_same_in_menu_and_submenu),
        ("shell::menu_scrollbar_over_highlighted_row", menu_scrollbar_over_highlighted_row),
        ("shell::menu_chart_submenu_grid", menu_chart_submenu_grid),
        ("shell::menu_search_lists_each_command_once", menu_search_lists_each_command_once),
    ]
}

/// A temporary settings directory (the user's own settings are never written).
fn temp_settings(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ibisOffice-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temporary directory");
    unsafe { std::env::set_var("IBIS_SETTINGS_DIR", &dir) };
    dir
}

/// Type `text` in the menu search and move down with the arrow key.
fn search_and_arrow_down(s: &mut Scenario, text: &str) {
    s.update(|e, window, cx| e.open_toolbar_menu_search(window, cx));
    s.settle();
    let text = text.to_string();
    s.update(move |e, window, cx| {
        let input = e.ensure_toolbar_search_input("menu-search", window, cx);
        input.update(cx, |state, cx| {
            state.focus(window, cx);
            state.replace_all(text, window, cx);
        });
    });
    s.settle();
    s.key("down");
}

fn settings_file(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join("settings.txt")).unwrap_or_default()
}

fn menu_search_history_lru_per_kind_and_restart() {
    let dir = temp_settings("menu-search-history");
    history::invalidate();
    let files = [
        (Kind::Document, "test/editing/docx-report.docx"),
        (Kind::Workbook, "test/editing/xlsx-sales.xlsx"),
        (Kind::Presentation, "test/editing/pptx-plan.pptx"),
    ];
    for (kind, path) in files {
        let mut s = Scenario::open(path);
        assert_eq!(s.with(|e, _| e.kind()), kind);
        // The arrow down key notes the typed string; a string already there moves to the front; five are kept.
        // "ファイル" and "挿入" are in the names of commands of all three screens.
        for word in ["ファイル", "挿入", "表", "ファイル", "保存", "印刷", "開く"] {
            search_and_arrow_down(&mut s, word);
            s.key("escape");
        }
        history::invalidate();
        assert_eq!(history::load(kind), ["開く", "印刷", "保存", "ファイル", "表"], "{kind:?}: the newest first, five at most, a repeat moved to the front");
        // An empty string is never noted.
        search_and_arrow_down(&mut s, "  ");
        s.key("escape");
        history::invalidate();
        assert_eq!(history::load(kind).len(), 5, "{kind:?}: empty is not noted");
        // The file holds it, apart for each kind.
        let file = settings_file(&dir);
        assert!(file.contains(history::key(kind)), "{kind:?}: {file}");
    }
    // Each kind keeps its own.
    let mut docx = Scenario::open("test/editing/docx-report.docx");
    search_and_arrow_down(&mut docx, "段落");
    docx.key("escape");
    history::invalidate();
    assert_eq!(history::load(Kind::Document)[0], "段落");
    assert_eq!(history::load(Kind::Workbook)[0], "開く", "the spreadsheet's list did not change");
    assert_eq!(history::load(Kind::Presentation)[0], "開く", "the presentation's list did not change");
    // A string with nothing found is not noted by the arrow key.
    search_and_arrow_down(&mut docx, "存在しないコマンド999");
    docx.key("escape");
    history::invalidate();
    assert_eq!(history::load(Kind::Document)[0], "段落");
    drop(docx);

    // Running a command from the search (Enter) notes the string too, and the lists survive a restart: they are
    // read from the settings file again.
    let mut xlsx = Scenario::open("test/editing/xlsx-sales.xlsx");
    let grid = crate::shell::i18n::text("xlsx.gridlines").to_string();
    xlsx.update(|e, window, cx| e.open_toolbar_menu_search(window, cx));
    xlsx.settle();
    let typed = grid.clone();
    xlsx.update(move |e, window, cx| {
        let input = e.ensure_toolbar_search_input("menu-search", window, cx);
        input.update(cx, |state, cx| {
            state.focus(window, cx);
            state.replace_all(typed, window, cx);
        });
    });
    xlsx.settle();
    xlsx.key("enter");
    history::invalidate();
    assert_eq!(history::load(Kind::Workbook)[0], grid, "a command run from the search is noted");
    assert_eq!(history::load(Kind::Workbook).len(), 5);
    let before = settings_file(&dir);
    history::invalidate();
    let restored: Vec<Vec<String>> = [Kind::Document, Kind::Workbook, Kind::Presentation].into_iter().map(history::load).collect();
    assert_eq!(settings_file(&dir), before, "reading the lists does not write the file");
    assert_eq!(restored[1][0], grid);
    assert_eq!(restored[0][0], "段落");
    drop(xlsx);
    let _ = std::fs::remove_dir_all(&dir);
    unsafe { std::env::remove_var("IBIS_SETTINGS_DIR") };
    history::invalidate();
}

fn menu_search_history_rows_and_choice() {
    let _probe = buttons::Probe::new();
    let dir = temp_settings("menu-search-rows");
    history::invalidate();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    // With no history: the "コマンド" heading and the commands.
    let items = s.with(|e, _| e.searchable_menu_items_for("menu-search"));
    let rows = menu_search_rows(&items, "", &[]);
    assert_eq!(rows[0].id, MENU_SEARCH_COMMANDS_HEADING);
    assert_eq!(rows.len(), items.len() + 1);
    // With history: 履歴 heading, the strings (newest first), コマンド heading, the commands. While typing: only commands.
    let list: Vec<String> = ["c", "b", "a"].map(String::from).to_vec();
    let rows = menu_search_rows(&items, "", &list);
    let ids: Vec<&str> = rows.iter().take(6).map(|r| r.id).collect();
    assert_eq!(ids, [MENU_SEARCH_HISTORY_HEADING, MENU_SEARCH_HISTORY_ITEM, MENU_SEARCH_HISTORY_ITEM, MENU_SEARCH_HISTORY_ITEM, MENU_SEARCH_COMMANDS_HEADING, rows[5].id]);
    assert_eq!(rows[1].label, "c");
    assert!(!rows[0].enabled() && !rows[4].enabled(), "headings are not chosen");
    assert!(menu_search_rows(&items, "ファイル", &list).iter().all(|r| r.id != MENU_SEARCH_HISTORY_ITEM && r.id != MENU_SEARCH_HISTORY_HEADING));

    // On screen: open the search with history, choose a history row by the keyboard, the string is in the field.
    for word in ["書式", "挿入", "ファイル"] {
        search_and_arrow_down(&mut s, word);
        s.key("escape");
    }
    s.update(|e, window, cx| e.open_toolbar_menu_search(window, cx));
    s.settle();
    s.update(|e, window, cx| e.ensure_toolbar_search_input("menu-search", window, cx).update(cx, |state, cx| state.focus(window, cx)));
    s.screenshot("menu-search-history");
    // Down once: the first history row ("ファイル", the newest) is the one chosen (headings are skipped); Enter puts it in the field.
    s.key("down");
    s.key("enter");
    let typed = s.update(|e, window, cx| e.ensure_toolbar_search_input("menu-search", window, cx).read(cx).value().to_string());
    assert_eq!(typed, "ファイル", "a history row puts its string in the field");
    assert!(s.with(|e, _| e.toolbar_search_open.is_some()), "and the list stays open");
    s.screenshot("menu-search-history-chosen");
    s.key("escape");
    drop(s);
    let _ = std::fs::remove_dir_all(&dir);
    unsafe { std::env::remove_var("IBIS_SETTINGS_DIR") };
    history::invalidate();
}

/// The geometry of the scroll bar of the popup that is open, as a (track width, thumb width, thumb's distance from
/// the right edge of the track).
fn bar_geometry(track: Bounds<Pixels>, thumb: Bounds<Pixels>) -> (f32, f32, f32) {
    (f32::from(track.size.width), f32::from(thumb.size.width), f32::from(track.right() - thumb.right()))
}

fn open_insert_menu(s: &mut Scenario) -> usize {
    let index = s.with(|e, _| e.menu_bar_menus().menus.iter().position(|(id, _)| *id == MenuId::Insert).unwrap());
    s.hover((0., 0.));
    s.update(move |e, w, cx| e.open_menu_at(index, w, cx));
    index
}

/// Down to the submenu `id` of the open Insert menu and open it with the right arrow.
fn open_submenu(s: &mut Scenario, id: &str) {
    let down = s.with(|e, _| {
        let menus = e.menu_bar_menus();
        let (_, (_, entries)) = menus.menus.iter().enumerate().find(|(_, (m, _))| *m == MenuId::Insert).unwrap();
        let target = entries.iter().position(|v| matches!(v, MenuEntry::Submenu(sub) if sub.id == id)).unwrap();
        entries[..=target].iter().filter(|v| match v { MenuEntry::Item(item) => item.enabled(), MenuEntry::Submenu(_) => true, _ => false }).count()
    });
    for _ in 0..down {
        s.key("down");
    }
    s.key("right");
}

fn menu_scrollbar_same_in_menu_and_submenu() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 420.);
    open_insert_menu(&mut s);
    s.settle();
    let root = (buttons::bounds("menu-scrollbar-track"), buttons::bounds("menu-scrollbar-thumb"));
    let (Some(track), Some(thumb)) = root else { panic!("the Insert menu is too short to scroll in this window: {:?}", buttons::all()) };
    let root_bar = bar_geometry(track, thumb);
    s.screenshot("menu-scrollbar-root");
    // A long submenu of the same menu.
    buttons::clear();
    open_submenu(&mut s, "insert.chart");
    s.settle();
    let (Some(track), Some(thumb), Some(popup)) = (buttons::bounds("submenu-scroll-track"), buttons::bounds("submenu-scroll-thumb"), buttons::bounds("submenu-popup")) else {
        panic!("the chart submenu has no scroll bar: {:?}", buttons::all())
    };
    s.screenshot("menu-scrollbar-submenu");
    let sub_bar = bar_geometry(track, thumb);
    assert_eq!(sub_bar, root_bar, "the submenu's scroll bar is the menu's: (track width, thumb width, gap to the track's edge)");
    assert_eq!(track.right(), popup.right(), "the track is at the submenu's right edge, as the menu's is at its own");
    assert_eq!(f32::from(thumb.right() - popup.right()).abs(), root_bar.2, "the thumb is {} px from the edge", root_bar.2);
}

/// GitHub issue #175: the scroll bar is drawn over the highlighted row, also when the menu is scrolled (the row's
/// background used to cover the thumb once the menu was scrolled off the top).
fn menu_scrollbar_over_highlighted_row() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 420.);
    open_insert_menu(&mut s);
    s.settle();
    s.update(|e, w, cx| { e.scroll_menu_to(3, w, cx); });
    s.settle();
    let thumb = buttons::bounds("menu-scrollbar-thumb").expect("a thumb");
    let track = buttons::bounds("menu-scrollbar-track").expect("a track");
    assert!(thumb.origin.y > track.origin.y + px(1.), "the thumb is off the top of the track: {thumb:?} {track:?}");
    // The pointer on the row under the thumb's middle (left of the track), so that row is highlighted.
    let y = f32::from(thumb.center().y);
    s.hover((f32::from(track.left()) - 60., y));
    let shot = s.screenshot("menu-scrollbar-highlighted-row");
    let image = image::open(shot).unwrap().to_rgba8();
    let viewport = s.update(|_, w, _| f32::from(w.viewport_size().width));
    let scale = image.width() as f32 / viewport;
    let at = |x: f32, y: f32| image.get_pixel((x * scale) as u32, (y * scale) as u32).0;
    let (on_thumb, on_row) = (at(f32::from(thumb.center().x), y), at(f32::from(track.left()) - 6., y));
    assert_ne!(on_thumb, on_row, "the thumb shows over the highlighted row: {on_thumb:?} vs the row {on_row:?}");
}

fn menu_chart_submenu_grid() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 900.);
    // The submenu's entries: 「おすすめグラフ...」 first, then charts only.
    let first = s.with(|e, _| {
        let menus = e.menu_bar_menus();
        let (_, entries) = menus.menus.iter().find(|(id, _)| *id == MenuId::Insert).unwrap();
        let Some(MenuEntry::Submenu(sub)) = entries.iter().find(|v| matches!(v, MenuEntry::Submenu(sub) if sub.id == "insert.chart")) else { panic!("no chart submenu") };
        match &sub.items[0] {
            MenuEntry::Item(item) => (item.id, item.label.to_string(), sub.items.iter().filter(|v| matches!(v, MenuEntry::Item(i) if i.thumb.is_some())).count()),
            _ => panic!("the first entry is not an item"),
        }
    });
    assert_eq!(first.0, "xlsx.recommended-charts");
    assert_eq!(first.1, crate::shell::i18n::text("xlsx.recommended-charts.menu"));
    assert!(first.1.ends_with("..."), "{}", first.1);
    assert!(first.2 >= 40, "every kind of chart has a picture: {}", first.2);

    let before = s.with(|e, _| e.slide().shapes.iter().filter(|v| matches!(v.kind, ShapeKind::Chart(_))).count());
    open_insert_menu(&mut s);
    open_submenu(&mut s, "insert.chart");
    s.settle();
    s.screenshot("chart-grid-top");
    let b = |id: &str| buttons::bounds(&format!("menu-item-{id}")).unwrap_or_else(|| panic!("{id} not drawn: {:?}", buttons::all()));
    let (c0, c1, c2) = (b("pptx.insert-chart.column"), b("pptx.insert-chart.stacked-column"), b("pptx.insert-chart.percent-stacked-column"));
    assert_eq!(c0.origin.y, c1.origin.y, "two tiles in a row");
    assert!(c1.origin.x > c0.origin.x + c0.size.width - px(1.), "the second is right of the first: {c0:?} {c1:?}");
    assert!(c2.origin.y > c0.origin.y + c0.size.height - px(1.) && c2.origin.x == c0.origin.x, "the third starts the next row: {c2:?}");
    let recommended = b("xlsx.recommended-charts");
    assert!(recommended.size.width > c0.size.width * 1.8, "the first item is a full-width line, the tiles are narrower");

    // The keyboard: down to the first row, right to its second tile, Enter inserts that one (stacked).
    s.key("down");
    s.key("right");
    s.screenshot("chart-grid-keyboard-second-column");
    s.key("enter");
    let (count, grouping) = s.with(|e, _| {
        let charts: Vec<_> = e.slide().shapes.iter().filter_map(|v| if let ShapeKind::Chart(c) = &v.kind { Some(c.clone()) } else { None }).collect();
        (charts.len(), charts.last().and_then(|c| c.groups.first().map(|g| g.grouping)))
    });
    assert_eq!(count, before + 1, "Enter inserted a chart");
    assert_eq!(grouping, Some(ibis_doc_model::Grouping::Stacked), "the right arrow moved to 積み上げ縦棒 (the second tile)");
    assert!(s.with(|e, _| e.open_menu.is_none()), "and the menu closed");

    // The pointer: the tile under it is the one inserted (the first column: clustered).
    open_insert_menu(&mut s);
    open_submenu(&mut s, "insert.chart");
    s.settle();
    let tile = b("pptx.insert-chart.column");
    s.hover((f32::from(tile.center().x), f32::from(tile.center().y)));
    s.screenshot("chart-grid-hover");
    s.click((f32::from(tile.center().x), f32::from(tile.center().y)));
    let (count2, grouping2) = s.with(|e, _| {
        let charts: Vec<_> = e.slide().shapes.iter().filter_map(|v| if let ShapeKind::Chart(c) = &v.kind { Some(c.clone()) } else { None }).collect();
        (charts.len(), charts.last().and_then(|c| c.groups.first().map(|g| g.grouping)))
    });
    assert_eq!(count2, count + 1, "a click inserted a chart");
    assert_eq!(grouping2, Some(ibis_doc_model::Grouping::Standard));

    // The second column by the pointer.
    open_insert_menu(&mut s);
    open_submenu(&mut s, "insert.chart");
    s.settle();
    let tile = b("pptx.insert-chart.stacked-column");
    s.click((f32::from(tile.center().x), f32::from(tile.center().y)));
    let grouping3 = s.with(|e, _| e.slide().shapes.iter().rev().find_map(|v| if let ShapeKind::Chart(c) = &v.kind { c.groups.first().map(|g| g.grouping) } else { None }));
    assert_eq!(grouping3, Some(ibis_doc_model::Grouping::Stacked), "a click on the second column inserted that one");

    // 「おすすめグラフ...」 still opens the panel by the keyboard.
    open_insert_menu(&mut s);
    open_submenu(&mut s, "insert.chart");
    s.key("enter");
    assert!(s.with(|e, _| e.sidebar_open && e.sheet_state().is_some_and(|st| st.panels.recommended_charts.is_some())), "the first line opens the recommended charts");
}

/// The menu search lists a command once: for a few words, on each screen, no two rows share a command id, or the same words and place.
fn menu_search_lists_each_command_once() {
    let queries = ["\u{56f3}\u{5f62}", "\u{30b0}\u{30e9}\u{30d5}", "\u{8868}", "\u{30c6}\u{30ad}\u{30b9}\u{30c8}\u{30dc}\u{30c3}\u{30af}\u{30b9}", "\u{30d8}\u{30c3}\u{30c0}\u{30fc}", "shape", "chart", "table", "text box", "header"];
    for path in ["test/editing/docx-report.docx", "test/editing/xlsx-sales.xlsx", "test/editing/pptx-plan.pptx"] {
        let mut s = Scenario::open(path);
        let all = s.with(|e, _| e.searchable_menu_items_for("menu-search"));
        for query in queries {
            let rows = menu_search_rows(&all, query, &[]);
            let mut seen_id = std::collections::HashMap::new();
            let mut seen_text = std::collections::HashSet::new();
            for row in rows.iter().filter(|r| !r.id.starts_with("menu-search.section.")) {
                let place = row.note.as_ref().map(|n| n.to_string()).unwrap_or_default();
                let text = (row.label.to_string(), place.clone());
                println!("{path} {query:?}: {} | {} | {place}", row.id, row.label);
                if let Some(prev) = seen_id.insert(row.id, text.clone()) {
                    panic!("{path} {query:?}: command {} listed twice: {prev:?} and {text:?}", row.id);
                }
                assert!(seen_text.insert(text.clone()), "{path} {query:?}: {text:?} listed twice");
                // (A menu and its submenu may share a name: "A \u{25b8} A". The row's text itself must not be the whole place twice.)
                let chars: Vec<char> = place.chars().collect();
                let half = chars.len() / 2;
                let doubled = chars.len() >= 2 && chars[..half].iter().collect::<String>().trim() == chars[half..].iter().collect::<String>().trim();
                assert!(!doubled, "{path} {query:?}: place {place:?} repeats itself");
            }
        }
        // A row that shows the menu's words is also found by the command's own words, still as one row.
        let aliased: Vec<_> = all.iter().filter_map(|r| r.search_alt.as_ref().map(|alt| (r.id, r.label.to_string(), alt.to_string()))).collect();
        for (id, shown, own) in aliased.iter().take(40) {
            for query in [shown, own] {
                let rows: Vec<_> = menu_search_rows(&all, query, &[]).into_iter().filter(|r| r.id == *id).collect();
                assert_eq!(rows.len(), 1, "{path}: {id} found by {query:?} is one row");
                assert_eq!(rows[0].label.as_ref(), shown.as_str(), "{path}: {id} shows the menu's words");
            }
        }
        if path.ends_with(".xlsx") {
            assert!(aliased.iter().any(|(id, _, _)| *id == "xlsx.hide-columns"), "xlsx.hide-columns has its own words as the alternative text");
        }
    }
}
