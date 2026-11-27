// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #291: the groups of rows and which of them are collapsed (`row/@outlineLevel`, `row/@collapsed`) are saved and read again.

use ibis_doc_model::sheet_command::{Axis, SheetCommand};
use ibis_doc_model::{Command, Document, ShapeKind};
use ibis_formats::{Kind, SaveOptions};

fn sheet_of(document: &Document) -> std::sync::Arc<ibis_doc_model::sheet::Sheet> {
    document.presentation().slides[0].shapes.iter().find_map(|s| if let ShapeKind::Sheet(sheet) = &s.kind { Some(sheet.clone()) } else { None }).unwrap()
}

#[test]
fn collapsed_groups_survive_saving() {
    let opened = ibis_formats::open(ibis_formats::blank_file(Kind::Workbook).unwrap()).unwrap();
    let mut document = Document::new(opened.presentation);
    let sheet = document.presentation().slides[0].id;
    let run = |document: &mut Document, c: SheetCommand| document.execute(vec![Command::Sheet(c)]).unwrap();
    for (first, last) in [(2, 5), (3, 4)] {
        run(&mut document, SheetCommand::Group { sheet, axis: Axis::Rows, first, last });
    }
    run(&mut document, SheetCommand::Group { sheet, axis: Axis::Columns, first: 1, last: 2 });
    run(&mut document, SheetCommand::CollapseGroup { sheet, axis: Axis::Rows, first: 3, last: 4, collapse: true });
    run(&mut document, SheetCommand::CollapseGroup { sheet, axis: Axis::Columns, first: 1, last: 2, collapse: true });
    let bytes = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let again = Document::new(ibis_formats::open(bytes).unwrap().presentation);
    let read = sheet_of(&again);
    assert_eq!(read.outline_rows.keys().copied().collect::<Vec<_>>(), vec![2, 3, 4, 5]);
    assert_eq!(read.collapsed_rows.iter().copied().collect::<Vec<_>>(), vec![5]);
    assert_eq!(read.collapsed_columns.iter().copied().collect::<Vec<_>>(), vec![3]);
    assert_eq!((3..=4).map(|r| read.rows[r]).collect::<Vec<_>>(), vec![0, 0]);
    // Shown again from the file: the inner group is collapsed on its own, as it was saved.
    let mut again = again;
    let s = again.presentation().slides[0].id;
    again.execute(vec![Command::Sheet(SheetCommand::CollapseGroup { sheet: s, axis: Axis::Rows, first: 2, last: 5, collapse: true })]).unwrap();
    again.execute(vec![Command::Sheet(SheetCommand::CollapseGroup { sheet: s, axis: Axis::Rows, first: 2, last: 5, collapse: false })]).unwrap();
    let read = sheet_of(&again);
    assert_eq!((2..=5).map(|r| read.rows[r] == 0).collect::<Vec<_>>(), vec![false, true, true, false]);
}
