// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! XLSX-FILE-008・009: a workbook with macros (`.xlsm`) edited and saved
//! keeps them; saved as a macro-enabled template (`.xltm`) it keeps them in
//! the template content type; saved without macros (`.xlsx`) the macros'
//! parts and the relationships to them are gone and the content type is the
//! plain one. Nothing runs the macros.
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, Document, SheetCommand};
use ibis_formats::SaveOptions;

const VBA: &[u8] = b"not a real VBA project: the bytes must come back as they are";
const MAIN: &str = "xl/workbook.xml";
const XLSM: &str = "application/vnd.ms-excel.sheet.macroEnabled.main+xml";

fn xlsm() -> Vec<u8> {
    let stock = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap();
    let mut package = ibis_ooxml::Package::open(stock).unwrap();
    package.set_override(MAIN, XLSM).unwrap();
    package.set_bytes("xl/vbaProject.bin", VBA);
    package.set_override("xl/vbaProject.bin", "application/vnd.ms-office.vbaProject").unwrap();
    package.add_relationship(MAIN, "http://schemas.microsoft.com/office/2006/relationships/vbaProject", "vbaProject.bin", false).unwrap();
    package.save(SaveOptions::default()).unwrap()
}

fn edited(opened: &ibis_formats::Opened) -> ibis_doc_model::Presentation {
    let mut d = Document::new(opened.presentation.clone());
    let slide = d.presentation().slides[0].id;
    d.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet: slide, edits: vec![CellEdit { row: 40, column: 0, input: CellInput::Value(CellValue::Number(7.0)) }] })]).unwrap();
    d.presentation().clone()
}

fn types(data: &[u8]) -> String {
    String::from_utf8(ibis_ooxml::Package::open(data.to_vec()).unwrap().read("[Content_Types].xml").unwrap()).unwrap()
}

#[test]
fn an_edited_xlsm_keeps_its_macros() {
    let data = xlsm();
    let opened = ibis_formats::open(data.clone()).unwrap();
    let saved = opened.source.save(&edited(&opened), SaveOptions::default()).unwrap();
    let after = ibis_ooxml::Package::open(saved.clone()).unwrap();
    assert_eq!(after.read("xl/vbaProject.bin").unwrap(), VBA);
    assert_eq!(after.content_type(MAIN).unwrap().as_deref(), Some(XLSM));
    assert!(after.relationships(MAIN).unwrap().iter().any(|r| r.kind.ends_with("/vbaProject")));
    let again = ibis_formats::read(&saved).unwrap();
    assert_eq!(ibis_doc_model::sheet_calc::sheet_of_slide(&again.slides[0]).unwrap().cell(40, 0).map(|c| c.value.clone()), Some(CellValue::Number(7.0)));
}

#[test]
fn an_xlsm_saved_as_a_template_is_the_macro_enabled_template() {
    let opened = ibis_formats::open(xlsm()).unwrap();
    let saved = opened.source.save_template(&edited(&opened), SaveOptions::default()).unwrap();
    let after = ibis_ooxml::Package::open(saved).unwrap();
    assert_eq!(after.content_type(MAIN).unwrap().as_deref(), Some("application/vnd.ms-excel.template.macroEnabled.main+xml"));
    assert_eq!(after.read("xl/vbaProject.bin").unwrap(), VBA);
}

#[test]
fn an_xlsm_saved_without_macros_loses_them_and_nothing_else() {
    let opened = ibis_formats::open(xlsm()).unwrap();
    let saved = opened.source.save_without_macros(&edited(&opened), SaveOptions::default()).unwrap();
    let after = ibis_ooxml::Package::open(saved.clone()).unwrap();
    assert!(!after.contains("xl/vbaProject.bin"));
    assert!(!after.relationships(MAIN).unwrap().iter().any(|r| r.kind.ends_with("/vbaProject")));
    assert_eq!(after.content_type(MAIN).unwrap().as_deref(), Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"));
    let t = types(&saved);
    assert!(!t.contains("vbaProject") && !t.contains("macroEnabled"), "{t}");
    // It opens as a plain workbook with the edit in.
    let again = ibis_formats::open(saved).unwrap();
    assert_eq!(ibis_doc_model::sheet_calc::sheet_of_slide(&again.presentation.slides[0]).unwrap().cell(40, 0).map(|c| c.value.clone()), Some(CellValue::Number(7.0)));
    // A file with no macros is saved as it is.
    let plain = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap();
    let opened = ibis_formats::open(plain.clone()).unwrap();
    assert_eq!(opened.source.save_without_macros(&opened.presentation, SaveOptions::default()).unwrap(), plain);
}
