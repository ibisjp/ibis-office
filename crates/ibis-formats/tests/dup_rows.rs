// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0043: two `<row>`s of one row number are one row, as Excel reads them (tasks/doc/reports/2026-11-16-k0043-duplicate-row.md): the
//! cells of both are the row's, the later one's where both have the cell, and the row's attributes are the later row's.
use ibis_doc_model::CellValue;

fn read(name: &str) -> ibis_doc_model::Presentation {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name);
    ibis_formats::read(&std::fs::read(path).unwrap()).unwrap()
}

/// The texts of row 2 (index 1) by column: what Excel shows there.
fn row_two(name: &str) -> Vec<(u32, String)> {
    let presentation = read(name);
    let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&presentation.slides[0]).unwrap();
    let mut cells: Vec<(u32, String)> = sheet.cells.iter().filter(|c| c.row == 1).map(|c| (c.column, match &c.value { CellValue::Text(t) => t.clone(), other => format!("{other:?}") })).collect();
    cells.sort();
    cells
}

fn expect(pairs: &[(u32, &str)]) -> Vec<(u32, String)> {
    pairs.iter().map(|(c, t)| (*c, t.to_string())).collect()
}

#[test]
fn rows_of_one_number_are_one_row_as_excel_reads_them() {
    assert_eq!(row_two("dup-row-control.xlsx"), expect(&[(0, "only-A2"), (1, "only-B2")]));
    assert_eq!(row_two("dup-row-same-cells.xlsx"), expect(&[(0, "second-A2"), (1, "second-B2")]));
    assert_eq!(row_two("dup-row-other-cells.xlsx"), expect(&[(0, "first-A2"), (1, "first-B2"), (2, "second-C2"), (3, "second-D2")]));
    assert_eq!(row_two("dup-row-overlap.xlsx"), expect(&[(0, "first-A2"), (1, "second-B2"), (2, "second-C2")]));
    assert_eq!(row_two("dup-row-triple.xlsx"), expect(&[(0, "three")]));
    assert_eq!(row_two("dup-row-height.xlsx"), expect(&[(0, "second-A2")]));
}

/// The row's attributes are the later row's: the first row says `ht="40"`, the second says nothing, so the row is not 40 pt high; the
/// rows after it keep their numbers.
#[test]
fn the_later_rows_attributes_are_the_rows() {
    let presentation = read("dup-row-height.xlsx");
    let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&presentation.slides[0]).unwrap();
    let height = |row: usize| sheet.rows[row] as f64 / 12_700.0;
    assert!(height(1) < 30.0, "row 2 is not the first row's 40 pt: {}", height(1));
    assert!(sheet.cells.iter().any(|c| c.row == 2 && c.column == 0 && c.value == CellValue::Text("after".into())), "the row after it is row 3");
}

/// Saved after an edit, the sheet has one row of each number (no `<row>` twice), with the cells Excel shows: a real workbook (xlsx-sales)
/// whose row 4 is written twice (the second has B4 and a new G4).
#[test]
fn a_saved_sheet_has_each_row_once() {
    use ibis_doc_model::{CellEdit, CellInput, Command, Document, SheetCommand};
    use ibis_formats::ibis_ooxml::package::{Package, SaveOptions as PackageSave};
    use ibis_formats::SaveOptions;
    let original = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap();
    let mut package = Package::open(original).unwrap();
    let xml = String::from_utf8(package.read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    let at = xml.find("<row r=\"5\"").expect("row 5");
    let second = r#"<row r="4"><c r="B4"><v>7</v></c><c r="G4" t="inlineStr"><is><t>extra</t></is></c></row>"#;
    package.set_bytes("xl/worksheets/sheet1.xml", format!("{}{second}{}", &xml[..at], &xml[at..]).into_bytes());
    let data = package.save(PackageSave::default()).unwrap();
    let opened = ibis_formats::open(data).unwrap();
    let cell = |presentation: &ibis_doc_model::Presentation, row: u32, column: u32| {
        ibis_doc_model::sheet_calc::sheet_of_slide(&presentation.slides[0]).unwrap().cell(row, column).map(|c| c.text.clone())
    };
    assert_eq!(cell(&opened.presentation, 3, 1).as_deref(), Some("7"), "the later B4");
    assert_eq!(cell(&opened.presentation, 3, 6).as_deref(), Some("extra"), "G4 is kept");
    assert_eq!(cell(&opened.presentation, 3, 0).as_deref(), Some("北町店"), "A4 of the first row is kept");
    let mut doc = Document::new(opened.presentation.clone());
    let slide = doc.presentation().slides[0].id;
    doc.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet: slide, edits: vec![CellEdit { row: 40, column: 20, input: CellInput::Value(CellValue::Number(1.0)) }] })]).unwrap();
    let saved = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    let sheet = String::from_utf8(Package::open(saved.clone()).unwrap().read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert_eq!(sheet.matches("<row r=\"4\"").count(), 1, "row 4 once");
    let back = ibis_formats::read(&saved).unwrap();
    assert_eq!(cell(&back, 3, 1).as_deref(), Some("7"));
    assert_eq!(cell(&back, 3, 6).as_deref(), Some("extra"));
}

/// xlsx-sales with its `sheetData` replaced by `rows` (the `<row>` elements as written), read.
fn read_rows(rows: &str) -> ibis_doc_model::Presentation {
    use ibis_formats::ibis_ooxml::package::{Package, SaveOptions as PackageSave};
    let original = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap();
    let mut package = Package::open(original).unwrap();
    let xml = String::from_utf8(package.read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    let (a, b) = (xml.find("<sheetData>").unwrap() + "<sheetData>".len(), xml.find("</sheetData>").unwrap());
    package.set_bytes("xl/worksheets/sheet1.xml", format!("{}{rows}{}", &xml[..a], &xml[b..]).into_bytes());
    ibis_formats::read(&package.save(PackageSave::default()).unwrap()).unwrap()
}

fn texts(presentation: &ibis_doc_model::Presentation) -> Vec<(u32, u32, String)> {
    let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&presentation.slides[0]).unwrap();
    let mut cells: Vec<(u32, u32, String)> = sheet.cells.iter().filter_map(|c| match &c.value { CellValue::Text(t) => Some((c.row, c.column, t.clone())), _ => None }).collect();
    cells.sort();
    cells
}

fn cell(r: &str, text: &str) -> String {
    format!(r#"<c r="{r}" t="inlineStr"><is><t>{text}</t></is></c>"#)
}

/// A `<row>` whose number is lower than the row before it is dropped whole, as Excel does when it repairs the file (K0043,
/// tasks/doc/requests/2026-11-16-k0043-row-order-result.md): the earlier row wins, and the rows after it that go on up are kept.
#[test]
fn a_row_numbered_below_the_row_before_it_is_dropped_as_excel_repairs() {
    // Descending: r3, then r2 (dropped), then r4.
    let desc = format!(r#"<row r="1">{}</row><row r="3">{}</row><row r="2">{}</row><row r="4">{}</row>"#, cell("A1", "r1"), cell("A3", "r3-first"), cell("A2", "r2-first"), cell("A4", "r4"));
    assert_eq!(texts(&read_rows(&desc)), vec![(0, 0, "r1".into()), (2, 0, "r3-first".into()), (3, 0, "r4".into())]);
    // A number twice, far apart: the second is dropped (A2 and B2 of the first stay, C2 of the second does not).
    let gap = format!(r#"<row r="1">{}</row><row r="2">{}{}</row><row r="3">{}</row><row r="2">{}{}</row><row r="4">{}</row>"#,
        cell("A1", "r1"), cell("A2", "r2-first"), cell("B2", "r2-B-first"), cell("A3", "r3"), cell("A2", "r2-second"), cell("C2", "r2-C-second"), cell("A4", "r4"));
    assert_eq!(texts(&read_rows(&gap)), vec![(0, 0, "r1".into()), (1, 0, "r2-first".into()), (1, 1, "r2-B-first".into()), (2, 0, "r3".into()), (3, 0, "r4".into())]);
}

/// A `<row>` without a number is the one after the row before it (the first one is row 1); one that then meets a numbered row of the same
/// number is one row with it, the later one's cells winning.
#[test]
fn a_row_without_a_number_is_the_one_after_the_row_before_it() {
    let nor = format!(r#"<row r="1">{}</row><row>{}</row><row>{}</row><row r="5">{}</row><row>{}</row>"#, cell("A1", "r1"), cell("A2", "nor-after1"), cell("A3", "nor-after1-b"), cell("A5", "r5"), cell("A6", "nor-after5"));
    assert_eq!(texts(&read_rows(&nor)), vec![(0, 0, "r1".into()), (1, 0, "nor-after1".into()), (2, 0, "nor-after1-b".into()), (4, 0, "r5".into()), (5, 0, "nor-after5".into())]);
    let nor2 = format!(r#"<row r="2">{}</row><row>{}</row><row r="3">{}</row><row>{}</row>"#, cell("A2", "r2"), cell("A3", "nor"), cell("A3", "r3-explicit"), cell("A4", "nor-last"));
    assert_eq!(texts(&read_rows(&nor2)), vec![(1, 0, "r2".into()), (2, 0, "r3-explicit".into()), (3, 0, "nor-last".into())]);
    let first = format!(r#"<row>{}</row><row>{}</row><row r="2">{}</row>"#, cell("A1", "nor-first"), cell("A2", "nor-second"), cell("A2", "r2-explicit"));
    assert_eq!(texts(&read_rows(&first)), vec![(0, 0, "nor-first".into()), (1, 0, "r2-explicit".into())]);
}

/// GitHub issue #260: the validations Excel writes in the sheet's `extLst` (`x14:dataValidations`: a list whose source is on another sheet or is
/// a formula) are read, and saved again with the sheet's other rules (once).
#[test]
fn x14_data_validations_are_read_and_saved_once_issue_260() {
    use ibis_formats::ibis_ooxml::package::{Package, SaveOptions as PackageSave};
    use ibis_doc_model::{CellEdit, CellInput, Command, Document, SheetCommand};
    let original = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap();
    let mut package = Package::open(original).unwrap();
    let xml = String::from_utf8(package.read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    assert!(!xml.contains("<extLst"), "the sheet has no extLst of its own");
    let ext = concat!(
        r#"<extLst><ext uri="{CCE6A557-97BC-4b89-ADB6-D9C93CAAB3DF}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main">"#,
        r#"<x14:dataValidations xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main" count="2">"#,
        r#"<x14:dataValidation type="list" allowBlank="1" showInputMessage="1" showErrorMessage="1"><x14:formula1><xm:f>OFFSET(Sheet2!$A$1,0,0,3,1)</xm:f></x14:formula1><xm:sqref>H2</xm:sqref></x14:dataValidation>"#,
        r#"<x14:dataValidation type="list" allowBlank="1"><x14:formula1><xm:f>INDIRECT("Sheet2!A1:A3")</xm:f></x14:formula1><xm:sqref>H3:H4</xm:sqref></x14:dataValidation>"#,
        r#"</x14:dataValidations></ext></extLst>"#);
    let at = xml.rfind("</worksheet>").unwrap();
    package.set_bytes("xl/worksheets/sheet1.xml", format!("{}{ext}{}", &xml[..at], &xml[at..]).into_bytes());
    let data = package.save(PackageSave::default()).unwrap();
    let opened = ibis_formats::open(data).unwrap();
    let rules = |p: &ibis_doc_model::Presentation| ibis_doc_model::sheet_calc::sheet_of_slide(&p.slides[0]).unwrap().validations.iter().map(|v| (v.kind.clone(), v.formula1.clone().unwrap_or_default(), v.ranges.len())).collect::<Vec<_>>();
    let read = rules(&opened.presentation);
    assert!(read.contains(&("list".into(), "OFFSET(Sheet2!$A$1,0,0,3,1)".into(), 1)), "{read:?}");
    assert!(read.contains(&("list".into(), "INDIRECT(\"Sheet2!A1:A3\")".into(), 1)), "{read:?}");
    // An edit of a cell leaves the rules as they are; a change of the rules writes them in the sheet's list and the x14 ones are not kept twice.
    let mut d = Document::new(opened.presentation.clone());
    let slide = d.presentation().slides[0].id;
    d.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet: slide, edits: vec![CellEdit { row: 0, column: 20, input: CellInput::Value(CellValue::Number(1.0)) }] })]).unwrap();
    let saved = opened.source.save(d.presentation(), ibis_formats::SaveOptions::default()).unwrap();
    assert_eq!(rules(&ibis_formats::read(&saved).unwrap()).len(), read.len());
}

/// A file that has `_xlnm.Print_Titles` before `_xlnm.Print_Area` in its `definedNames` keeps both (Windows ODF team's finding, 2026-11-20).
#[test]
fn print_titles_before_print_area_are_both_read() {
    use ibis_formats::ibis_ooxml::package::{Package, SaveOptions as PackageSave};
    let original = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap();
    let mut package = Package::open(original).unwrap();
    let xml = String::from_utf8(package.read("xl/workbook.xml").unwrap()).unwrap();
    let names = r#"<definedNames><definedName name="_xlnm.Print_Titles" localSheetId="0">月次売上!$1:$1</definedName><definedName name="_xlnm.Print_Area" localSheetId="0">月次売上!$A$1:$E$10</definedName></definedNames>"#;
    let at = xml.find("<calcPr").unwrap();
    package.set_bytes("xl/workbook.xml", format!("{}{names}{}", &xml[..at], &xml[at..]).into_bytes());
    let p = ibis_formats::read(&package.save(PackageSave::default()).unwrap()).unwrap();
    let wb = p.workbook.as_deref().unwrap();
    let found: Vec<&str> = wb.names.iter().map(|n| n.name.as_str()).collect();
    assert!(found.contains(&"_xlnm.Print_Titles") && found.contains(&"_xlnm.Print_Area"), "{found:?}");
    let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&p.slides[0]).unwrap();
    assert!(sheet.print_area.is_some(), "the print area is read");
}
