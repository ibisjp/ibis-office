// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #145: with row 1000 of the first sheet in view, showing the second sheet and coming back showed row 1 again. Each worksheet
//! keeps how far it was scrolled (and its selected cell) until the workbook closes, by the sheet's id.
use super::SheetSteps;
use crate::shell::scenario::Scenario;
use gpui_kit::{point, px};
use crate::ext::*;

fn scroll(s: &mut Scenario) -> f32 {
    s.with(|e, _| f32::from(e.workspace_scroll.offset().y))
}

pub(crate) fn sheets_keep_their_scroll() {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Workbook);
    s.size(1200., 720.);
    s.update(|e, window, cx| e.copy_sheet(window, cx));
    let ids: Vec<_> = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.iter().filter_map(|sh| sh.slide).collect());
    assert_eq!(ids.len(), 2);
    let (one, two) = (ids[0], ids[1]);
    s.update(|e, window, cx| e.select_slide(one, window, cx));
    s.click_cell("C7");
    // Row 1000 in view on the first sheet.
    s.update(|e, _, cx| {
        e.workspace_scroll.set_offset(point(px(0.0), px(-19000.0)));
        cx.notify();
    });
    let first = scroll(&mut s);
    assert!(first < -1000.0, "the first sheet is scrolled down: {first}");
    // The second sheet opens at its top, and is scrolled to a place of its own.
    s.update(|e, window, cx| e.select_slide(two, window, cx));
    assert_eq!(scroll(&mut s), 0.0, "a sheet not shown before opens at the top");
    s.update(|e, _, cx| {
        e.workspace_scroll.set_offset(point(px(0.0), px(-4000.0)));
        cx.notify();
    });
    let second = scroll(&mut s);
    s.update(|e, window, cx| e.select_slide(one, window, cx));
    assert_eq!(scroll(&mut s), first, "back on the first sheet it is where it was left");
    assert_eq!(s.with(|e, _| e.current_cell()), Some((2, 6)), "and so is its selected cell");
    s.update(|e, window, cx| e.select_slide(two, window, cx));
    assert_eq!(scroll(&mut s), second, "the second sheet keeps its own");
    s.update(|e, window, cx| e.select_slide(one, window, cx));
    // The sheets are told apart by id: moving the first one after the second changes nothing.
    assert_eq!(scroll(&mut s), first);
}

/// The positions go into the file as Excel writes them (`sheetView/@topLeftCell`, `selection`) and come back when it is opened: each
/// sheet's scroll to the row at the top, and the selected cell and range.
pub(crate) fn sheet_views_round_trip_through_the_file() {
    let mut s = Scenario::open("test/editing/ibis-src.xlsx");
    s.size(1200., 720.);
    s.update(|e, window, cx| e.copy_sheet(window, cx));
    let ids: Vec<_> = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.iter().filter_map(|sh| sh.slide).collect());
    let (one, two) = (ids[0], ids[1]);
    s.update(|e, window, cx| e.select_slide(one, window, cx));
    s.click_cell("C7");
    s.update(|e, _, cx| {
        e.workspace_scroll.set_offset(point(px(0.0), px(-19000.0)));
        cx.notify();
    });
    let first = scroll(&mut s);
    s.update(|e, window, cx| e.select_slide(two, window, cx));
    s.click_cell("B2");
    s.update(|e, _, cx| {
        e.workspace_scroll.set_offset(point(px(0.0), px(-4000.0)));
        cx.notify();
    });
    let second = scroll(&mut s);
    // Saved outside the scenario's folder: opening it again copies it there (a file onto itself would be emptied).
    let folder = crate::shell::scenario::repo_root().join("target/issue145-sheet-views");
    std::fs::create_dir_all(&folder).unwrap();
    let path = s.save_as(folder.join("sheet-views.xlsx").to_str().unwrap());
    // The model read back from the file.
    let saved = s.reopen();
    let view = |slide| crate::shell::editor::sheet_of(saved.slide(slide).unwrap()).unwrap().1.view.clone();
    let (a, b) = (view(one), view(two));
    assert_eq!(a.active_cell, Some((2, 6)), "the first sheet's selected cell");
    assert!(a.top_left.is_some_and(|(c, r)| c == 0 && r > 800), "the first sheet's top row: {:?}", a.top_left);
    assert_eq!(b.active_cell, Some((1, 1)));
    assert!(b.top_left.is_some_and(|(_, r)| r > 150 && r < 250), "the second sheet's top row: {:?}", b.top_left);
    // And the XML: Excel's own attributes.
    let xml = ibis_pptx::test_files::part_text(&std::fs::read(&path).unwrap(), "xl/worksheets/sheet1.xml").unwrap();
    assert!(xml.contains("topLeftCell=\"A") && xml.contains("activeCell=\"C7\"") && xml.contains("sqref=\"C7\""), "{xml}");
    // Opened again: where each sheet was left.
    drop(s);
    let mut t = Scenario::open(path.to_str().unwrap());
    let ids: Vec<_> = t.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.iter().filter_map(|sh| sh.slide).collect());
    assert!((scroll(&mut t) - first).abs() < 25.0, "the first sheet opens where it was left: {} for {first}", scroll(&mut t));
    assert_eq!(t.with(|e, _| e.current_cell()), Some((2, 6)));
    t.update(|e, window, cx| e.select_slide(ids[1], window, cx));
    assert!((scroll(&mut t) - second).abs() < 25.0, "the second sheet too: {} for {second}", scroll(&mut t));
    assert_eq!(t.with(|e, _| e.current_cell()), Some((1, 1)));
}

/// Frozen panes: the part that scrolls is `pane/@topLeftCell`. Saved from the scroll, put back as the scroll when opened, for the top
/// row frozen, the first column frozen and both (at the selected cell); and `topLeftCell` comes before `workbookViewId` as Excel writes it.
pub(crate) fn frozen_panes_keep_the_scroll_in_the_file() {
    // (the menu that freezes, the frozen columns and rows, the scroll to give (x, y))
    let cases = [("xlsx.freeze-top-row", (0u32, 1u32), (0.0f32, -4000.0f32)), ("xlsx.freeze-first-column", (1, 0), (-1500.0, 0.0)), ("xlsx.freeze-at-selection", (2, 3), (-1500.0, -4000.0))];
    for (n, (menu, frozen, (x, y))) in cases.into_iter().enumerate() {
        let mut s = Scenario::open("test/editing/ibis-src.xlsx");
        s.size(1200., 720.);
        let slide = s.with(|e, _| e.current_slide);
        if menu == "xlsx.freeze-at-selection" {
            s.click_cell("C4");
        }
        s.menu(menu);
        assert_eq!(s.with(|e, _| e.current_sheet().unwrap().1.frozen), Some(frozen), "{menu}");
        s.update(|e, _, cx| {
            e.sheet_state_mut().unwrap().cells.insert(slide, (9, 74));
            e.workspace_scroll.set_offset(point(px(x), px(y)));
            cx.notify();
        });
        let scrolled = s.with(|e, _| e.workspace_scroll.offset());
        let folder = crate::shell::scenario::repo_root().join("target/issue145-sheet-views");
        std::fs::create_dir_all(&folder).unwrap();
        let path = s.save_as(folder.join(format!("frozen-{n}.xlsx")).to_str().unwrap());
        let saved = s.reopen();
        let view = crate::shell::editor::sheet_of(&saved.slides[0]).unwrap().1.view.clone();
        let (column, row) = view.pane_top_left.expect("the scrolling pane's first cell");
        assert_eq!(view.active_cell, Some((9, 74)), "{menu}: J75");
        // The scroll is in the pane's top-left cell on the axes that scrolled; the cell after the frozen ones where it did not.
        assert_eq!(column > frozen.0, x != 0.0, "{menu}: column {column}");
        assert_eq!(row > frozen.1, y != 0.0, "{menu}: row {row}");
        assert_eq!(column.max(frozen.0), column);
        // `topLeftCell` before `workbookViewId` (as Excel writes it) when the top-left pane scrolls on a side.
        let xml = ibis_pptx::test_files::part_text(&std::fs::read(&path).unwrap(), "xl/worksheets/sheet1.xml").unwrap();
        let tag = &xml[xml.find("<sheetView ").unwrap()..];
        let tag = &tag[..tag.find('>').unwrap()];
        if let (Some(a), Some(b)) = (tag.find("topLeftCell"), tag.find("workbookViewId")) {
            assert!(a < b, "{tag}");
        }
        assert!(xml.contains("<pane ") && xml.contains("state=\"frozen\""), "{menu}");
        drop(s);
        let mut t = Scenario::open(path.to_str().unwrap());
        let now = t.with(|e, _| e.workspace_scroll.offset());
        assert!((f32::from(now.x) - f32::from(scrolled.x)).abs() < 110.0 && (f32::from(now.y) - f32::from(scrolled.y)).abs() < 25.0, "{menu}: opens at {now:?}, was {scrolled:?}");
        assert_eq!(t.with(|e, _| e.current_cell()), Some((9, 74)), "{menu}");
    }
}

/// K0025: scrolled down on the first sheet, showing the second and deleting it (or undoing its creation) shows the first sheet where it was
/// left: every change of the active sheet goes through `SlideEditor::set_current_slide` (the format's `page_changed`).
pub(crate) fn deleting_the_other_sheet_keeps_the_scroll() {
    use ibis_doc_model::sheet_command::SheetCommand;
    use ibis_doc_model::Command;
    let mut s = Scenario::new_document(ibis_pptx::Kind::Workbook);
    s.size(1200., 720.);
    s.update(|e, window, cx| e.copy_sheet(window, cx));
    let ids: Vec<_> = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.iter().filter_map(|sh| sh.slide).collect());
    let (one, two) = (ids[0], ids[1]);
    s.update(|e, window, cx| e.select_slide(one, window, cx));
    s.update(|e, _, cx| {
        e.workspace_scroll.set_offset(point(px(0.0), px(-19000.0)));
        cx.notify();
    });
    let first = scroll(&mut s);
    assert!(first < -1000.0, "{first}");
    s.update(|e, window, cx| e.select_slide(two, window, cx));
    s.update(|e, _, cx| {
        e.workspace_scroll.set_offset(point(px(0.0), px(-3000.0)));
        cx.notify();
    });
    // Deleting the shown sheet: the first one comes back where it was.
    s.update(|e, _, cx| e.run_sheet(vec![Command::Sheet(SheetCommand::DeleteSheet { slide: two })], cx));
    assert_eq!(s.with(|e, _| e.current_slide), one);
    assert_eq!(scroll(&mut s), first, "after the other sheet is deleted the first keeps its scroll");
    // Deleting a sheet that is not shown changes nothing of the view.
    s.update(|e, window, cx| e.copy_sheet(window, cx));
    s.update(|e, window, cx| e.select_slide(one, window, cx));
    assert_eq!(scroll(&mut s), first);
}

/// The rule behind K0025: the active sheet (`current_slide`) is never assigned outside `SlideEditor::set_current_slide`; the places
/// that may (loading a file or a window, `select_slide`'s fallback for formats with no `show_page`) are listed here.
pub(crate) fn current_slide_is_assigned_only_by_the_setter() {
    fn scan(dir: &std::path::Path, allowed: &[&str], bad: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "tests") {
                    continue;
                }
                scan(&path, allowed, bad);
            } else if path.extension().is_some_and(|e| e == "rs") {
                for (n, line) in std::fs::read_to_string(&path).unwrap().lines().enumerate() {
                    let t = line.trim();
                    if t.starts_with("//") || t.starts_with("let ") {
                        continue;
                    }
                    let mut rest = t;
                    while let Some(at) = rest.find("current_slide") {
                        let after = rest[at + "current_slide".len()..].trim_start();
                        if after.starts_with('=') && !after.starts_with("==") && !allowed.contains(&t) {
                            bad.push(format!("{}:{}: {t}", path.display(), n + 1));
                        }
                        rest = &rest[at + 1..];
                    }
                }
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut bad = Vec::new();
    scan(&root.join("ibis-office-xlsx/src"), &[], &mut bad);
    // The shell: only these lines of editor.rs (the setter, loading, a window made from this one, the fallback of `select_slide`).
    let allowed = [
        "self.current_slide = to;",
        "self.current_slide = presentation.slides[0].id;",
        "editor.current_slide = current;",
        "self.current_slide = slide;",
    ];
    scan(&root.join("ibis-office-shell/src"), &allowed, &mut bad);
    assert!(bad.is_empty(), "assign the active sheet with SlideEditor::set_current_slide (it saves and restores each sheet's scroll):\n{}", bad.join("\n"));
}
