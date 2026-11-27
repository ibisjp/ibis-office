// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A workbook with no `styles.xml` (Excel opens and saves one): editing it and saving adds the smallest styles part, with its content
//! type and the workbook's relationship, and the values and the format come back when the saved file is opened again.
use crate::shell::actions::Save;
use crate::shell::scenario::Scenario;
use ibis_doc_model::ShapeKind;
use ibis_formats::ibis_ooxml::Package;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("shell::xlsx_no_styles_workbooks_save_with_a_new_styles_part", xlsx_no_styles_workbooks_save_with_a_new_styles_part)]
}

fn xlsx_no_styles_workbooks_save_with_a_new_styles_part() {
    for (path, first, second) in [("test/editing/dup-row-control.xlsx", "head", "only-A2"), ("test/editing/active-tab-excel-style.xlsx", "S1", "")] {
        let mut s = Scenario::open(path);
        let file = s.with(|e, _| e.path.clone()).expect("the copy's path");
        assert!(!Package::open(std::fs::read(&file).unwrap()).unwrap().names().any(|n| n == "xl/styles.xml"), "{path}: the file has no styles part");
        // A1 gets a value, and is made bold.
        s.type_text("changed");
        s.key("enter");
        s.key("up");
        s.key("cmd-b");
        s.settle();
        s.update(|e, w, cx| e.save(&Save, w, cx));
        s.settle();
        let (asking, dirty, status) = s.with(|e, _| (e.ask.is_some(), e.is_dirty(), e.status.clone()));
        assert!(!asking && !dirty, "{path}: saved: dirty {dirty}, status {status:?}");
        let bytes = std::fs::read(&file).unwrap();
        let package = Package::open(bytes.clone()).unwrap();
        // The part is there, with its content type and the workbook's relationship; every relationship target and content type holds.
        assert!(package.contains("xl/styles.xml"), "{path}: styles part added");
        assert_eq!(package.content_type("xl/styles.xml").unwrap().as_deref(), Some("application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"), "{path}");
        let rels = package.relationships("xl/workbook.xml").unwrap();
        assert!(rels.iter().any(|r| r.kind.ends_with("/styles") && r.target.trim_start_matches('/').ends_with("styles.xml")), "{path}: workbook relationship");
        for source in ["", "xl/workbook.xml"] {
            for r in package.relationships(source).unwrap().iter().filter(|r| !r.external) {
                let target = if source.is_empty() { r.target.trim_start_matches('/').to_string() } else { format!("xl/{}", r.target.trim_start_matches('/').trim_start_matches("xl/")) };
                assert!(package.contains(&target), "{path}: {} has no part", r.target);
            }
        }
        for name in package.names().filter(|n| !n.ends_with('/')) {
            assert!(package.content_type(name).unwrap().is_some(), "{path}: {name} has no content type");
        }
        // Opened again: the value and the format stayed.
        let opened = ibis_formats::open(bytes).unwrap_or_else(|e| panic!("{path}: opens again: {e}"));
        let sheet = opened.presentation.slides.iter().find_map(|slide| {
            slide.shapes.iter().find_map(|sh| match &sh.kind {
                ShapeKind::Sheet(sheet) => Some(sheet.clone()),
                _ => None,
            })
        });
        let sheet = sheet.expect("a sheet");
        let a1 = sheet.cells.iter().find(|c| c.row == 0 && c.column == 0).expect("A1");
        assert_eq!(a1.text, "changed", "{path}");
        assert!(sheet.styles[a1.style as usize].bold, "{path}: A1 is bold");
        let _ = (first, second);
    }
}
