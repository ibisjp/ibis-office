// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #195: the cell the file was left at is the cell the file opens at. The selected cell and range and the scroll of each
//! sheet (`sheetView`: `selection/@activeCell`, `@sqref`, `topLeftCell`) were kept already (#145, `issue145_sheet_view.rs`); the
//! sheet the file was left on (`workbookView/@activeTab`, `sheetView/@tabSelected`) is read when the file is opened and written when
//! it is saved (ECMA-376 18.2.1 workbookView, 18.3.1.88 sheetView). The sheets keep their own positions when switched.
use super::SheetSteps;
use crate::ext::*;
use crate::shell::scenario::Scenario;

fn ids(s: &mut Scenario) -> Vec<ibis_doc_model::SlideId> {
    s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.iter().filter_map(|sh| sh.slide).collect())
}

fn part(path: &std::path::Path, name: &str) -> String {
    ibis_formats::test_files::part_text(&std::fs::read(path).unwrap(), name).unwrap()
}

pub(crate) fn active_sheet_and_cell_round_trip_through_the_file() {
    let mut s = Scenario::open("test/editing/ibis-src.xlsx");
    s.size(1200., 720.);
    s.update(|e, window, cx| e.copy_sheet(window, cx));
    s.update(|e, window, cx| e.copy_sheet(window, cx));
    let ids = ids(&mut s);
    assert!(ids.len() >= 3);
    // Each sheet has its own cell; the file is left on the third.
    s.update(|e, window, cx| e.select_slide(ids[0], window, cx));
    s.click_cell("B2");
    s.update(|e, window, cx| e.select_slide(ids[1], window, cx));
    s.click_cell("D4");
    s.update(|e, window, cx| e.select_slide(ids[2], window, cx));
    s.click_cell("C7");
    let folder = crate::shell::scenario::repo_root().join("target/issue195");
    std::fs::create_dir_all(&folder).unwrap();
    let path = s.save_as(folder.join("active-tab.xlsx").to_str().unwrap());
    let book = part(&path, "xl/workbook.xml");
    assert!(book.contains("activeTab=\"2\""), "{book}");
    for (i, selected) in [(1, false), (2, false), (3, true)] {
        let xml = part(&path, &format!("xl/worksheets/sheet{i}.xml"));
        assert_eq!(xml.contains("tabSelected=\"1\""), selected, "sheet{i}: {xml}");
    }
    drop(s);

    // Opened again: on the third sheet at C7, and the others where they were left.
    let mut t = Scenario::open(path.to_str().unwrap());
    let ids = self::ids(&mut t);
    assert_eq!(t.with(|e, _| e.current_slide), ids[2], "the file opens on the sheet it was left on");
    assert_eq!(t.with(|e, _| e.current_cell()), Some((2, 6)));
    t.update(|e, window, cx| e.select_slide(ids[0], window, cx));
    assert_eq!(t.with(|e, _| e.current_cell()), Some((1, 1)), "the first sheet keeps its own cell");
    t.update(|e, window, cx| e.select_slide(ids[1], window, cx));
    assert_eq!(t.with(|e, _| e.current_cell()), Some((3, 3)), "and so does the second");

    // Back on the first sheet and saved: `activeTab` goes (the first sheet), `tabSelected` moves to it alone.
    t.update(|e, window, cx| e.select_slide(ids[0], window, cx));
    let again = t.save_as(folder.join("active-tab-first.xlsx").to_str().unwrap());
    assert!(!part(&again, "xl/workbook.xml").contains("activeTab"), "{}", part(&again, "xl/workbook.xml"));
    for (i, selected) in [(1, true), (2, false), (3, false)] {
        let xml = part(&again, &format!("xl/worksheets/sheet{i}.xml"));
        assert_eq!(xml.contains("tabSelected=\"1\""), selected, "sheet{i}: {xml}");
    }
    drop(t);
    let mut u = Scenario::open(again.to_str().unwrap());
    let ids = self::ids(&mut u);
    assert_eq!(u.with(|e, _| e.current_slide), ids[0]);
    assert_eq!(u.with(|e, _| e.current_cell()), Some((1, 1)));
}
