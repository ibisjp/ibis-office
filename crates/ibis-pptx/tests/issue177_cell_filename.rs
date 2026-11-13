// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #177 regression: the volatile cells calculated again on open (CELL among them) must know the file, so the common
//! sheet-name formula keeps showing the sheet name instead of turning into #VALUE!.
use ibis_doc_model::{CellValue, Presentation, ShapeKind};
use ibis_ooxml::{Package, SaveOptions};

const FORMULA: &str = r#"MID(CELL("filename",A1),FIND("]",CELL("filename",A1))+1,255)"#;

/// test/editing/xlsx-sales.xlsx with the formula in the unused cell H1 of the first sheet, cached as `CACHED`.
fn book() -> (Vec<u8>, String) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let mut package = Package::open(std::fs::read(root.join("xlsx-sales.xlsx")).unwrap()).unwrap();
    let workbook = String::from_utf8(package.read("xl/workbook.xml").unwrap()).unwrap();
    let at = workbook.find("<sheet name=\"").unwrap() + "<sheet name=\"".len();
    let name = workbook[at..workbook[at..].find('"').unwrap() + at].to_string();
    let part = "xl/worksheets/sheet1.xml";
    let xml = String::from_utf8(package.read(part).unwrap()).unwrap();
    let cell = format!(r#"<c r="H1" t="str"><f>{}</f><v>CACHED</v></c></row>"#, FORMULA.replace('"', "&quot;"));
    let xml = xml.replacen("</row>", &cell, 1);
    package.set_bytes(part, xml.into_bytes());
    (package.save(SaveOptions::default()).unwrap(), name)
}

fn h1(p: &Presentation) -> CellValue {
    p.slides[0]
        .shapes
        .iter()
        .find_map(|s| match &s.kind {
            ShapeKind::Sheet(sheet) => sheet.cell(0, 7).map(|c| c.value.clone()),
            _ => None,
        })
        .expect("H1 is a cell")
}

#[test]
fn sheet_name_formula_survives_open_with_a_path() {
    let (data, name) = book();
    let path = "/dir/Book.xlsx".to_string();
    let opened = ibis_calc::eval::convert::with_opening_file(Some(path), || ibis_pptx::open(data)).unwrap();
    assert_eq!(h1(&opened.presentation), CellValue::Text(name));
}

#[test]
fn sheet_name_formula_keeps_its_cache_without_a_path() {
    let (data, _) = book();
    let opened = ibis_calc::eval::convert::with_opening_file(None, || ibis_pptx::open(data)).unwrap();
    assert_eq!(h1(&opened.presentation), CellValue::Text("CACHED".into()));
}
