// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A note (the old kind of comment) stays on its cell when rows or columns are inserted or deleted and the book is saved (A5 of the
//! 0.8.5 review): `comments*.xml` and the VML drawing move with the cells.

use ibis_doc_model::sheet_command::Axis;
use ibis_doc_model::{Command, Document, SheetCommand};
use ibis_ooxml::{Package, SaveOptions as PackageOptions};
use ibis_pptx::SaveOptions;

const SHEET: &str = "xl/worksheets/sheet1.xml";

/// `xlsx-sales.xlsx` with a note on B5 (a `comments1.xml` and a VML drawing, as Excel writes them).
fn with_note() -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx");
    let mut package = Package::open(std::fs::read(path).unwrap()).unwrap();
    package.set_bytes(
        "xl/comments1.xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><authors><author>a</author></authors><commentList><comment ref="B5" authorId="0"><text><r><t>メモ</t></r></text></comment></commentList></comments>"#.as_bytes().to_vec(),
    );
    package.set_override("xl/comments1.xml", "application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml").unwrap();
    package.set_bytes(
        "xl/drawings/vmlDrawing1.vml",
        r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><v:shape id="_x0000_s1025" type="#_x0000_t202"><x:ClientData ObjectType="Note"><x:MoveWithCells/><x:Anchor>2, 15, 3, 10, 4, 31, 7, 4</x:Anchor><x:Row>4</x:Row><x:Column>1</x:Column></x:ClientData></v:shape></xml>"##.as_bytes().to_vec(),
    );
    package.set_override("xl/drawings/vmlDrawing1.vml", "application/vnd.openxmlformats-officedocument.vmlDrawing").unwrap();
    package.add_relationship(SHEET, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments", "../comments1.xml", false).unwrap();
    package.add_relationship(SHEET, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing", "../drawings/vmlDrawing1.vml", false).unwrap();
    package.save(PackageOptions::default()).unwrap()
}

fn text(bytes: &[u8], part: &str) -> String {
    String::from_utf8(Package::open(bytes.to_vec()).unwrap().read(part).unwrap().to_vec()).unwrap()
}

#[test]
fn a_note_follows_its_cell_when_a_row_is_inserted_or_deleted() {
    let data = with_note();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let slide = opened.presentation.slides.iter().find(|s| s.name.as_deref() == Some("月次売上")).unwrap().id;
    let mut d = Document::new(opened.presentation.clone());
    d.execute(vec![Command::Sheet(SheetCommand::Insert { slide, axis: Axis::Rows, at: 1, count: 1 })]).unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let comments = text(&saved, "xl/comments1.xml");
    assert!(comments.contains(r#"ref="B6""#), "{comments}");
    let vml = text(&saved, "xl/drawings/vmlDrawing1.vml");
    assert!(vml.contains("<x:Row>5</x:Row>") && vml.contains("<x:Column>1</x:Column>"), "{vml}");
    assert!(vml.contains("2, 15, 4, 10, 4, 31, 8, 4"), "the box moves with its cell: {vml}");
    // A column inserted before it.
    let mut d2 = Document::new(opened.presentation.clone());
    d2.execute(vec![Command::Sheet(SheetCommand::Insert { slide, axis: Axis::Columns, at: 0, count: 1 })]).unwrap();
    let saved2 = opened.source.save(d2.presentation(), SaveOptions::default()).unwrap();
    assert!(text(&saved2, "xl/comments1.xml").contains(r#"ref="C5""#));
    let vml2 = text(&saved2, "xl/drawings/vmlDrawing1.vml");
    assert!(vml2.contains("<x:Column>2</x:Column>") && vml2.contains("3, 15, 3, 10, 5, 31, 7, 4"), "{vml2}");
    // The row of the note deleted: the note goes.
    let mut d3 = Document::new(opened.presentation.clone());
    d3.execute(vec![Command::Sheet(SheetCommand::Delete { slide, axis: Axis::Rows, at: 4, count: 1 })]).unwrap();
    let saved3 = opened.source.save(d3.presentation(), SaveOptions::default()).unwrap();
    assert!(!text(&saved3, "xl/comments1.xml").contains("<comment "));
    assert!(!text(&saved3, "xl/drawings/vmlDrawing1.vml").contains("ClientData"));
}
