// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #291, X13 of `tasks/doc/test-design/issue-291.md`: the outline of columns (and the file's own items for it) as the outline of rows has it: nesting to
//! Excel's seven nested groups (its outline level 8) and no further, one level off when ungrouping, lines inserted inside a group join it and one above its first line does not,
//! a deleted line shrinks it, and the worksheet's XML says `outlineLevel`, `collapsed` and `outlineLevelRow` / `outlineLevelCol` (ECMA-376 18.3.1).

use ibis_doc_model::sheet_command::Axis;
use ibis_doc_model::{Command, Document, SheetCommand, ShapeKind};
use ibis_formats::{Kind, SaveOptions};

fn blank() -> (Document, ibis_doc_model::SlideId, ibis_formats::Source) {
    let opened = ibis_formats::open(ibis_formats::blank_file(Kind::Workbook).unwrap()).unwrap();
    let document = Document::new(opened.presentation);
    let slide = document.presentation().slides[0].id;
    (document, slide, opened.source)
}

fn sheet(document: &Document) -> std::sync::Arc<ibis_doc_model::sheet::Sheet> {
    document.presentation().slides[0].shapes.iter().find_map(|s| if let ShapeKind::Sheet(sheet) = &s.kind { Some(sheet.clone()) } else { None }).unwrap()
}

fn run(document: &mut Document, command: SheetCommand) -> Result<(), ibis_doc_model::EditError> {
    document.execute(vec![Command::Sheet(command)])
}

fn levels(document: &Document, axis: Axis) -> Vec<(u32, u8)> {
    let sheet = sheet(document);
    let map = if axis == Axis::Rows { &sheet.outline_rows } else { &sheet.outline_columns };
    map.iter().map(|(i, l)| (*i, *l)).collect()
}

#[test]
fn both_axes_nest_to_seven_group_levels_and_no_further() {
    for axis in [Axis::Rows, Axis::Columns] {
        let (mut d, s, _) = blank();
        for level in 1..=7 {
            run(&mut d, SheetCommand::Group { sheet: s, axis, first: 2, last: 4 }).unwrap_or_else(|e| panic!("{axis:?} level {level}: {e:?}"));
        }
        assert_eq!(levels(&d, axis), vec![(2, 7), (3, 7), (4, 7)], "{axis:?}");
        assert!(run(&mut d, SheetCommand::Group { sheet: s, axis, first: 2, last: 4 }).is_err(), "{axis:?}: an eighth group (Excel's outline level 9) is refused");
        assert_eq!(levels(&d, axis), vec![(2, 7), (3, 7), (4, 7)], "{axis:?}: nothing changed");
    }
}

#[test]
fn ungrouping_columns_takes_one_level_off_and_a_part_leaves_the_rest_grouped() {
    let (mut d, s, _) = blank();
    run(&mut d, SheetCommand::Group { sheet: s, axis: Axis::Columns, first: 1, last: 5 }).unwrap();
    run(&mut d, SheetCommand::Group { sheet: s, axis: Axis::Columns, first: 2, last: 3 }).unwrap();
    run(&mut d, SheetCommand::Ungroup { sheet: s, axis: Axis::Columns, first: 2, last: 3 }).unwrap();
    assert_eq!(levels(&d, Axis::Columns), vec![(1, 1), (2, 1), (3, 1), (4, 1), (5, 1)]);
    run(&mut d, SheetCommand::Ungroup { sheet: s, axis: Axis::Columns, first: 3, last: 3 }).unwrap();
    assert_eq!(levels(&d, Axis::Columns), vec![(1, 1), (2, 1), (4, 1), (5, 1)], "the rest stays grouped");
}

#[test]
fn lines_inserted_inside_a_group_join_it_and_one_above_it_does_not() {
    for axis in [Axis::Rows, Axis::Columns] {
        let (mut d, s, _) = blank();
        run(&mut d, SheetCommand::Group { sheet: s, axis, first: 2, last: 5 }).unwrap();
        run(&mut d, SheetCommand::Group { sheet: s, axis, first: 3, last: 4 }).unwrap();
        // Between two lines of the inner group: level 2.
        run(&mut d, SheetCommand::Insert { sheet: s, axis, at: 4, count: 1 }).unwrap();
        assert_eq!(levels(&d, axis), vec![(2, 1), (3, 2), (4, 2), (5, 2), (6, 1)], "{axis:?}: inside the inner group");
        // Above the first line of the group: not in it.
        run(&mut d, SheetCommand::Insert { sheet: s, axis, at: 2, count: 1 }).unwrap();
        assert_eq!(levels(&d, axis), vec![(3, 1), (4, 2), (5, 2), (6, 2), (7, 1)], "{axis:?}: above the group");
        // A line deleted from inside shrinks it.
        run(&mut d, SheetCommand::Delete { sheet: s, axis, at: 5, count: 1 }).unwrap();
        assert_eq!(levels(&d, axis), vec![(3, 1), (4, 2), (5, 2), (6, 1)], "{axis:?}: a deleted line");
        // Every line of the group deleted: the group is gone.
        run(&mut d, SheetCommand::Delete { sheet: s, axis, at: 3, count: 4 }).unwrap();
        assert!(levels(&d, axis).is_empty(), "{axis:?}: no group is left");
    }
}

#[test]
fn the_worksheet_says_outline_levels_and_collapsed_for_rows_and_columns() {
    let (mut d, s, source) = blank();
    run(&mut d, SheetCommand::Group { sheet: s, axis: Axis::Rows, first: 2, last: 5 }).unwrap();
    run(&mut d, SheetCommand::Group { sheet: s, axis: Axis::Columns, first: 1, last: 3 }).unwrap();
    run(&mut d, SheetCommand::Group { sheet: s, axis: Axis::Columns, first: 2, last: 3 }).unwrap();
    run(&mut d, SheetCommand::CollapseGroup { sheet: s, axis: Axis::Columns, first: 2, last: 3, collapse: true }).unwrap();
    run(&mut d, SheetCommand::CollapseGroup { sheet: s, axis: Axis::Rows, first: 2, last: 5, collapse: true }).unwrap();
    let bytes = source.save(d.presentation(), SaveOptions::default()).unwrap();
    let package = ibis_ooxml::package::Package::open(bytes).unwrap();
    let xml = String::from_utf8(package.read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(xml.contains(r#"outlineLevelRow="1""#), "{xml}");
    assert!(xml.contains(r#"outlineLevelCol="2""#), "{xml}");
    // The rows of the group carry the level and are hidden; the row after the group (the summary row) says collapsed.
    assert!(xml.contains(r#"r="3""#) && xml.matches(r#"outlineLevel="1""#).count() >= 4 + 1, "{xml}");
    assert!(xml.contains(r#"collapsed="1""#), "{xml}");
    assert!(xml.contains("hidden=\"1\""), "{xml}");
}

/// #291 W2: the totals above (left) of the groups, `sheetPr/outlinePr/@summaryBelow` / `@summaryRight` = 0 (ECMA-376 18.3.1.31): read, written, and the collapsed
/// mark of a group sits on the line before it (the line after it when the totals are below).
#[test]
fn summary_above_and_left_are_kept_in_the_file_and_move_the_collapsed_mark() {
    let (mut d, s, source) = blank();
    run(&mut d, SheetCommand::Group { sheet: s, axis: Axis::Rows, first: 2, last: 5 }).unwrap();
    run(&mut d, SheetCommand::CollapseGroup { sheet: s, axis: Axis::Rows, first: 2, last: 5, collapse: true }).unwrap();
    assert!(sheet(&d).collapsed_rows.contains(&6), "below: the mark is on the line after the group");
    run(&mut d, SheetCommand::SetOutlineSummary { sheet: s, above: true, left: true }).unwrap();
    let now = sheet(&d);
    assert!(now.outline_summary_above && now.outline_summary_left);
    assert_eq!(now.collapsed_rows.iter().copied().collect::<Vec<_>>(), vec![1], "above: the mark moved to the line before the group");
    run(&mut d, SheetCommand::CollapseGroup { sheet: s, axis: Axis::Rows, first: 2, last: 5, collapse: false }).unwrap();
    assert!(sheet(&d).collapsed_rows.is_empty());
    let bytes = source.save(d.presentation(), SaveOptions::default()).unwrap();
    let package = ibis_ooxml::package::Package::open(bytes.clone()).unwrap();
    let xml = String::from_utf8(package.read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(xml.contains(r#"summaryBelow="0""#) && xml.contains(r#"summaryRight="0""#), "{xml}");
    // Read back.
    let reopened = ibis_formats::open(bytes).unwrap();
    let back = Document::new(reopened.presentation);
    let back = sheet(&back);
    assert!(back.outline_summary_above && back.outline_summary_left);
    // Setting them back leaves the attributes out (their default is 1).
    run(&mut d, SheetCommand::SetOutlineSummary { sheet: s, above: false, left: false }).unwrap();
    let bytes = reopened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let xml = String::from_utf8(ibis_ooxml::package::Package::open(bytes).unwrap().read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(!xml.contains("summaryBelow") && !xml.contains("summaryRight"), "{xml}");
}
