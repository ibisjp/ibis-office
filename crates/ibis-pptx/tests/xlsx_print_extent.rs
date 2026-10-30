// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Used cells for printing, including typed values with empty display text.
use std::sync::Arc;

use ibis_doc_model::{CellValue, ShapeKind, Sheet};
use ibis_ooxml::{Package, SaveOptions};

fn read_probe(cell: &str, dimension: &str) -> (Arc<Sheet>, (usize, usize)) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx");
    let mut package = Package::open(std::fs::read(path).unwrap()).unwrap();
    package.set_bytes("xl/workbook.xml", br#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Probe" sheetId="1" r:id="rId1"/></sheets></workbook>"#.to_vec());
    package.set_bytes("xl/_rels/workbook.xml.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#.to_vec());
    package.set_bytes("xl/styles.xml", br#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><numFmts count="1"><numFmt numFmtId="165" formatCode=";;;"/></numFmts><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="165" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#.to_vec());
    let xml = format!(r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><dimension ref="{dimension}"/><sheetFormatPr defaultRowHeight="15"/><cols><col min="1" max="3" width="60" customWidth="1"/></cols><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>source</t></is></c></row><row r="70">{cell}</row></sheetData></worksheet>"#);
    package.set_bytes("xl/worksheets/sheet1.xml", xml.into_bytes());
    package.remove("xl/worksheets/_rels/sheet1.xml.rels");
    let presentation = ibis_pptx::read_xlsx(&package.save(SaveOptions::default()).unwrap()).unwrap();
    let sheet = presentation.slides[0].shapes.iter().find_map(|shape| match &shape.kind {
        ShapeKind::Sheet(sheet) => Some(Arc::clone(sheet)),
        _ => None,
    }).expect("the first worksheet");
    // What prints is the slide's size (the sheet's used extent); the sheet itself goes on past it as an empty grid.
    let size = presentation.slides[0].size.expect("a sheet size");
    let used = (
        sheet.column_offsets().iter().skip(1).take_while(|x| **x <= size.width).count(),
        sheet.row_offsets().iter().skip(1).take_while(|y| **y <= size.height).count(),
    );
    (sheet, used)
}

#[test]
fn typed_empty_text_extends_the_print_grid_beyond_visible_cells() {
    let (sheet, used) = read_probe(r#"<c r="C70" t="inlineStr"><is><t></t></is></c>"#, "A1:C70");
    assert_eq!(used, (3, 70));
    let cell = sheet.cell(69, 2).unwrap();
    assert_eq!(cell.value, CellValue::Text(String::new()));
    assert!(cell.text.is_empty());
}

#[test]
fn a_number_hidden_by_its_format_still_extends_the_print_grid() {
    let (sheet, used) = read_probe(r#"<c r="C70" s="1"><v>42</v></c>"#, "A1:C70");
    assert_eq!(used, (3, 70));
    let cell = sheet.cell(69, 2).unwrap();
    assert_eq!(cell.value, CellValue::Number(42.0));
    assert!(cell.text.is_empty());
}

#[test]
fn an_empty_results_formula_extends_the_print_grid() {
    let (sheet, used) = read_probe(r#"<c r="C70" t="str"><f>""</f><v></v></c>"#, "A1:C70");
    assert_eq!(used, (3, 70));
    assert!(sheet.cell(69, 2).unwrap().formula.is_some());
}

#[test]
fn an_empty_format_is_retained_without_extending_the_print_grid() {
    let (sheet, used) = read_probe(r#"<c r="C70" s="1"/>"#, "A1:C70");
    assert_eq!(used, (1, 1));
    assert_eq!(sheet.cell(69, 2).unwrap().value, CellValue::Empty);
}

#[test]
fn a_dimension_hint_does_not_materialize_unused_rows_and_columns() {
    let (sheet, used) = read_probe("", "A1:XFD1048576");
    assert_eq!(used, (1, 1));
    assert_eq!(sheet.cells.len(), 1);
}
