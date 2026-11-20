// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #214: a workbook saved while its third sheet is shown opens on the third sheet again. The file is one that Excel
//! wrote (`tabSelected` on the first sheet, no `activeTab`, the second sheet hidden); the sheet is changed, the file saved in place
//! (the Save command's `save_to`), and opened again.
use crate::ext::*;
use crate::shell::scenario::Scenario;

fn part(path: &std::path::Path, name: &str) -> String {
    ibis_formats::test_files::part_text(&std::fs::read(path).unwrap(), name).unwrap()
}

fn ids(s: &mut Scenario) -> Vec<ibis_doc_model::SlideId> {
    s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.iter().filter_map(|sh| sh.slide).collect())
}

pub(crate) fn a_file_saved_on_the_third_sheet_opens_on_it() {
    let folder = crate::shell::scenario::repo_root().join("target/issue214");
    std::fs::create_dir_all(&folder).unwrap();
    let work = folder.join("excel-style.xlsx");
    std::fs::copy(crate::shell::scenario::repo_root().join("test/editing/active-tab-excel-style.xlsx"), &work).unwrap();
    let mut s = Scenario::open(work.to_str().unwrap());
    s.size(1200., 720.);
    let ids = ids(&mut s);
    assert_eq!(ids.len(), 3, "the hidden sheet has no page");
    assert_eq!(s.with(|e, _| e.current_slide), ids[0]);
    // The third sheet (the hidden one counted: index 2, the page after the first), saved in place.
    s.update(|e, window, cx| e.select_slide(ids[1], window, cx));
    let saved = s.save_as(work.to_str().unwrap());
    assert!(part(&saved, "xl/workbook.xml").contains("activeTab=\"2\""), "{}", part(&saved, "xl/workbook.xml"));
    for (i, selected) in [(1, false), (2, false), (3, true), (4, false)] {
        let xml = part(&saved, &format!("xl/worksheets/sheet{i}.xml"));
        assert_eq!(xml.contains("tabSelected=\"1\""), selected, "sheet{i}: {xml}");
    }
    drop(s);
    let mut t = Scenario::open(saved.to_str().unwrap());
    let ids = self::ids(&mut t);
    assert_eq!(t.with(|e, _| e.current_slide), ids[1], "the file opens on the sheet it was left on");
}

/// A workbook made in ibisOffice (no file behind it): sheets added, the third shown, saved, saved again from another sheet, opened.
pub(crate) fn a_new_workbook_saved_on_the_third_sheet_opens_on_it() {
    let folder = crate::shell::scenario::repo_root().join("target/issue214");
    std::fs::create_dir_all(&folder).unwrap();
    // As File > New makes it in the start view: the blank file is the document's source.
    let mut s = Scenario::start_view();
    s.size(1200., 720.);
    s.update(|e, window, cx| e.new_file(ibis_formats::Kind::Workbook, window, cx));
    s.settle();
    s.update(|e, window, cx| e.copy_sheet(window, cx));
    s.update(|e, window, cx| e.copy_sheet(window, cx));
    let ids = ids(&mut s);
    assert_eq!(ids.len(), 3);
    s.update(|e, window, cx| e.select_slide(ids[2], window, cx));
    let saved = s.save_as(folder.join("new-third.xlsx").to_str().unwrap());
    assert!(part(&saved, "xl/workbook.xml").contains("activeTab=\"2\""), "{}", part(&saved, "xl/workbook.xml"));
    // Saved in place a second time, from the second sheet, then the third again.
    s.update(|e, window, cx| e.select_slide(ids[1], window, cx));
    let second = s.save_as(saved.to_str().unwrap());
    assert!(part(&second, "xl/workbook.xml").contains("activeTab=\"1\""), "{}", part(&second, "xl/workbook.xml"));
    s.update(|e, window, cx| e.select_slide(ids[2], window, cx));
    let third = s.save_as(saved.to_str().unwrap());
    assert!(part(&third, "xl/workbook.xml").contains("activeTab=\"2\""), "{}", part(&third, "xl/workbook.xml"));
    drop(s);
    let mut t = Scenario::open(third.to_str().unwrap());
    let ids = self::ids(&mut t);
    assert_eq!(t.with(|e, _| e.current_slide), ids[2], "the file opens on the sheet it was left on");
}
