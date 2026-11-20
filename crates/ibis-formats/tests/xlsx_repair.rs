// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use ibis_doc_model::CellValue;
use ibis_ooxml::zip::{EntryMeta, Writer};
use ibis_formats::repair::{repair_workbook, RepairMode};

fn make_sample_workbook_zip(sheet_xml: &str) -> Vec<u8> {
    let mut w = Writer::new();
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#;
    let wb = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets>
    <sheet name="Sheet1" sheetId="1" r:id="rId1"/>
  </sheets>
</workbook>"#;
    let wb_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>"#;

    w.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    w.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    w.add("xl/workbook.xml", EntryMeta::new(), wb.as_bytes()).unwrap();
    w.add("xl/_rels/workbook.xml.rels", EntryMeta::new(), wb_rels.as_bytes()).unwrap();
    w.add("xl/worksheets/sheet1.xml", EntryMeta::new(), sheet_xml.as_bytes()).unwrap();
    w.finish().unwrap()
}

#[test]
fn test_repair_intact_workbook() {
    let sheet = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1"><v>100</v></c>
      <c r="B1"><v>200</v></c>
    </row>
  </sheetData>
</worksheet>"#;
    let data = make_sample_workbook_zip(sheet);
    let repaired = repair_workbook(&data, RepairMode::Repair).expect("repair succeeds");
    assert_eq!(repaired.report.sheets_recovered, 1);
    assert_eq!(repaired.report.cells_recovered, 2);
    assert!(repaired.report.success);
}

#[test]
fn test_repair_workbook_with_invalid_xml_control_chars() {
    // Inject invalid ASCII control character \x01 and \x1F inside sheet XML
    let sheet_with_bad_chars = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1" t="inlineStr"><is><t>Corrupt{}Data{}</t></is></c>
      <c r="B1"><v>42</v></c>
    </row>
  </sheetData>
</worksheet>"#,
        '\x01', '\x1F'
    );
    let data = make_sample_workbook_zip(&sheet_with_bad_chars);

    // Standard open should fail due to invalid XML char
    assert!(ibis_formats::open(&*data).is_err());

    // Repair must sanitize and recover the workbook cleanly
    let repaired = repair_workbook(&data, RepairMode::Repair).expect("repair must recover invalid XML");
    assert_eq!(repaired.report.sheets_recovered, 1);
    assert_eq!(repaired.report.cells_recovered, 2);
    assert!(repaired.report.logs.iter().any(|l| l.message.contains("サニタイズ")));
}

#[test]
fn test_repair_workbook_with_unclosed_tags() {
    // Truncated XML without closing </row>, </sheetData>, </worksheet>
    let truncated_sheet = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1"><v>123</v></c>
      <c r="B1"><v>456</v></c>"#;
    let data = make_sample_workbook_zip(truncated_sheet);

    // Standard open fails due to EOF in XML
    assert!(ibis_formats::open(&*data).is_err());

    // Repair must balance unclosed tags and recover data
    let repaired = repair_workbook(&data, RepairMode::Repair).expect("repair must balance unclosed tags");
    assert_eq!(repaired.report.sheets_recovered, 1);
    assert_eq!(repaired.report.cells_recovered, 2);
    assert!(repaired.report.logs.iter().any(|l| l.message.contains("補完修復")));
}

#[test]
fn test_repair_workbook_with_missing_workbook_xml_and_rels() {
    // Create a zip with ONLY xl/worksheets/sheet1.xml, missing workbook.xml, rels, and Content_Types
    let sheet = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1"><v>999</v></c>
    </row>
  </sheetData>
</worksheet>"#;
    let mut w = Writer::new();
    w.add("xl/worksheets/sheet1.xml", EntryMeta::new(), sheet.as_bytes()).unwrap();
    let data = w.finish().unwrap();

    // Standard open fails because [Content_Types].xml is missing
    assert!(ibis_formats::open(&*data).is_err());

    // Repair must reconstruct workbook.xml, [Content_Types].xml, and _rels
    let repaired = repair_workbook(&data, RepairMode::Repair).expect("repair must synthesize missing parts");
    assert_eq!(repaired.report.sheets_recovered, 1);
    assert_eq!(repaired.report.cells_recovered, 1);
    assert!(repaired.report.parts_reconstructed >= 3);
}

#[test]
fn test_extract_values_mode() {
    let sheet = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1"><v>10</v></c>
      <c r="A2"><v>20</v></c>
      <c r="A3"><f>SUM(A1:A2)</f><v>30</v></c>
    </row>
  </sheetData>
</worksheet>"#;
    let data = make_sample_workbook_zip(sheet);

    // In ExtractValues mode, formula <f> is converted/stripped, preserving cached value 30
    let repaired = repair_workbook(&data, RepairMode::ExtractValues).expect("extract values succeeds");
    assert_eq!(repaired.report.formulas_converted_to_values, 1);
    assert_eq!(repaired.report.formulas_recovered, 0);

    let slide = &repaired.presentation.slides[0];
    let sheet = slide.shapes.iter().find_map(|s| {
        if let ibis_doc_model::ShapeKind::Sheet(sheet) = &s.kind {
            Some(sheet)
        } else {
            None
        }
    }).unwrap();
    let cell_a3 = sheet.cell(2, 0).unwrap();
    assert_eq!(cell_a3.value, CellValue::Number(30.0));
    assert!(cell_a3.formula.is_none());
}

#[test]
fn test_extract_formulas_mode() {
    let sheet = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1"><v>10</v></c>
      <c r="A2"><v>20</v></c>
      <c r="A3"><f>SUM(A1:A2)</f><v>30</v></c>
    </row>
  </sheetData>
</worksheet>"#;
    let data = make_sample_workbook_zip(sheet);

    // In ExtractFormulas mode, formula <f> is retained
    let repaired = repair_workbook(&data, RepairMode::ExtractFormulas).expect("extract formulas succeeds");
    assert_eq!(repaired.report.formulas_recovered, 1);
    assert_eq!(repaired.report.formulas_converted_to_values, 0);

    let slide = &repaired.presentation.slides[0];
    let sheet = slide.shapes.iter().find_map(|s| {
        if let ibis_doc_model::ShapeKind::Sheet(sheet) = &s.kind {
            Some(sheet)
        } else {
            None
        }
    }).unwrap();
    let cell_a3 = sheet.cell(2, 0).unwrap();
    assert!(cell_a3.formula.is_some());
}
