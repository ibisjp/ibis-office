// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! XLSX-MODEL-020: the parts of a data model, the connections and the
//! queries (which ibisOffice does not run) stay in the file when the
//! workbook is edited and saved, and the sheets show their cell values as
//! they are.
//!
//! The file is made from `xlsx-sales.xlsx` with synthetic parts (their
//! content is arbitrary bytes: only that they are kept is checked).
use ibis_doc_model::sheet_calc::sheet_of_slide;
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, Document, SheetCommand};
use ibis_formats::SaveOptions;

const PARTS: [(&str, &str, &str, &[u8]); 4] = [
    ("xl/model/item.data", "application/vnd.ms-excel.model", "http://schemas.microsoft.com/office/2007/relationships/model", b"\x00\x01data model bytes\xff\xfe"),
    ("xl/connections.xml", "application/vnd.openxmlformats-officedocument.spreadsheetml.connections+xml", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/connections", b"<connections xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><connection id=\"1\" name=\"Query - Sales\" type=\"5\"/></connections>"),
    ("xl/queryTables/queryTable1.xml", "application/vnd.openxmlformats-officedocument.spreadsheetml.queryTable+xml", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/queryTable", b"<queryTable xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" name=\"Q1\" connectionId=\"1\"/>"),
    ("customXml/item1.xml", "application/xml", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXml", b"<DataMashup xmlns=\"http://schemas.microsoft.com/DataMashup\">AAAA</DataMashup>"),
];

#[test]
fn model_connection_and_query_parts_survive_an_edit_and_save() {
    let stock = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap();
    let mut package = ibis_ooxml::Package::open(stock).unwrap();
    for (name, content_type, relationship, bytes) in PARTS {
        package.set_bytes(name, bytes.to_vec());
        if content_type != "application/xml" {
            package.set_override(name, content_type).unwrap();
        }
        let target = name.strip_prefix("xl/").map_or_else(|| format!("../{name}"), str::to_string);
        package.add_relationship("xl/workbook.xml", relationship, &target, false).unwrap();
    }
    let original = package.save(SaveOptions::default()).unwrap();

    let opened = ibis_formats::open(original.clone()).unwrap();
    // The sheets read as usual.
    let sheet = sheet_of_slide(&opened.presentation.slides[0]).unwrap();
    assert!(!sheet.cells.is_empty());
    let mut d = Document::new(opened.presentation.clone());
    let slide = d.presentation().slides[0].id;
    d.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet: slide, edits: vec![CellEdit { row: 30, column: 3, input: CellInput::Value(CellValue::Number(42.0)) }] })]).unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();

    let after = ibis_ooxml::Package::open(saved.clone()).unwrap();
    for (name, content_type, relationship, bytes) in PARTS {
        assert_eq!(after.read(name).unwrap(), bytes, "{name} is kept as it was");
        if content_type != "application/xml" {
            assert_eq!(after.content_type(name).unwrap().as_deref(), Some(content_type), "{name}'s content type");
        }
        let target = name.strip_prefix("xl/").map_or_else(|| format!("../{name}"), str::to_string);
        assert!(after.relationships("xl/workbook.xml").unwrap().iter().any(|r| r.kind == relationship && r.target == target), "{name}'s relationship");
    }
    // The edit is in, and the file reads again.
    let again = ibis_formats::read(&saved).unwrap();
    assert_eq!(sheet_of_slide(&again.slides[0]).unwrap().cell(30, 3).map(|c| c.value.clone()), Some(CellValue::Number(42.0)));
}
