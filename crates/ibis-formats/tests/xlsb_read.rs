// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A binary workbook (`.xlsb`) is read as the workbook made from it. The
//! sample is written by `scripts/gen-xlsb-fixture.py` from MS-XLSB (LibreOffice
//! reads it with the same values).
use ibis_doc_model::{CellValue, Rgb, sheet_calc::sheet_of_slide};

#[test]
fn a_binary_workbook_is_read() {
    let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/generated.xlsb")).unwrap();
    assert!(ibis_formats::xlsb_to_xlsx(&data).is_ok());
    let p = ibis_formats::read(&data).unwrap();
    assert_eq!(p.slides.len(), 2);
    assert_eq!(p.slides[0].name.as_deref(), Some("売上"));
    let sheet = sheet_of_slide(&p.slides[0]).unwrap();
    let cell = |r, c| sheet.cell(r, c).unwrap_or_else(|| panic!("no cell {r}:{c}"));
    // Strings (shared and inline), the heading's style.
    assert_eq!(cell(0, 0).value, CellValue::Text("商品".into()));
    let head = &sheet.styles[cell(0, 0).style as usize];
    assert!(head.bold);
    assert_eq!(head.fill, Some(Rgb(0xFFFF00)));
    assert_eq!(cell(7, 0).value, CellValue::Text("Hello \"xlsb\" & <co>".into()));
    // Numbers: a real with a format, an integer RK, a percentage RK.
    assert_eq!(cell(1, 1).value, CellValue::Number(1234.5));
    assert_eq!(cell(1, 1).text, "1,234.50円");
    assert_eq!(cell(2, 1).value, CellValue::Number(250.0));
    assert_eq!(cell(2, 2).text, "75%");
    // Formulas: a function, IF with its jumps, another sheet, a shared one, an array one.
    assert_eq!(cell(3, 1).formula.as_deref(), Some("SUM(B2:B3)"));
    assert_eq!(cell(3, 2).formula.as_deref(), Some("IF(B4>1000,\"多い\",\"少ない\")"));
    assert_eq!(cell(3, 3).formula.as_deref(), Some("Data!B1*2"));
    assert_eq!(cell(4, 1).formula.as_deref(), Some("B2+B3"));
    assert_eq!(cell(4, 2).formula.as_deref(), Some("C2+C3"));
    assert_eq!(cell(5, 3).formula.as_deref(), Some("B2:B3*2"));
    // A boolean, an error.
    assert_eq!(cell(7, 1).value, CellValue::Bool(true));
    assert_eq!(cell(7, 2).text, "#DIV/0!");
    // Merged cells, the frozen row and column, the column widths.
    assert_eq!(sheet.merges.len(), 1);
    assert_eq!(sheet.frozen, Some((1, 1)));
    // Page breaks (row and col).
    assert_eq!(sheet.print.row_breaks, vec![4]);
    assert_eq!(sheet.print.column_breaks, vec![2]);
    // The second sheet: protected with a legacy password hash.
    let other = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(other.cell(0, 1).unwrap().value, CellValue::Number(3.25));
    assert_eq!(other.cell(0, 2).unwrap().text, "2023/3/15");
    let sheet_prot = other.protection.as_ref().expect("sheet2 should be protected");
    assert_eq!(sheet_prot.legacy.as_deref(), Some("CB14"));
    // Workbook protection.
    let wb = p.workbook.as_ref().expect("workbook metadata");
    let wb_prot = wb.protection.as_ref().expect("workbook protection");
    assert!(wb_prot.lock_structure);
    assert_eq!(wb_prot.legacy.as_deref(), Some("1234"));
}

/// A workbook Excel saved as .xlsb (`test/editing/xlsb-features.xlsb`): data validation, a conditional format, a
/// note, an AutoFilter, and formulas that name another sheet.
#[test]
fn an_excel_binary_workbook_keeps_its_features() {
    let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/xlsb-features.xlsb")).unwrap();
    let p = ibis_formats::read(&data).unwrap();
    let sheet = sheet_of_slide(&p.slides[0]).unwrap();
    assert_eq!(sheet.validations.len(), 1);
    assert_eq!(sheet.conditional.len(), 1);
    assert_eq!(sheet.comments.len(), 1);
    assert!(sheet.auto_filter.is_some());
    let other = sheet_of_slide(&p.slides[1]).unwrap();
    assert_eq!(other.cell(0, 0).unwrap().formula.as_deref(), Some("Data!C3*2"));
    assert_eq!(other.cell(1, 0).unwrap().formula.as_deref(), Some("SUM(Data!C2:C9)"));
    assert_eq!(other.cell(2, 0).unwrap().formula.as_deref(), Some("IF(Data!C3=0,\"なし\",\"あり\")"));
    // The sales book: four sheets, a chart and the formulas across sheets.
    let data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/xlsb-sales.xlsb")).unwrap();
    let p = ibis_formats::read(&data).unwrap();
    assert_eq!(p.slides.len(), 4);
    let refs = sheet_of_slide(&p.slides[3]).unwrap();
    assert_eq!(refs.cell(1, 0).unwrap().formula.as_deref(), Some("月次売上!E9*2"));
    assert_eq!(refs.cell(1, 0).unwrap().value, CellValue::Number(41_030_000.0));
}
