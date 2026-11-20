// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0039: a query whose source is `Excel.Workbook(File.Contents(path), null, true)` (what Excel's ブックから writes) reads the workbook at the
//! path through the app's file reader: a sheet is its used range from A1 (the columns `Column1`…), a table is the table. The Excel M is
//! read and run; the result is what the file holds.
use crate::shell::scenario::Scenario;
use ibis_doc_model::query::{m_to_query, result_of, Query, Source};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("shell::k0039_workbook_source_is_read_from_its_file", workbook_source), ("shell::k0039_workbook_is_read_by_the_interpreter", workbook_by_the_interpreter)]
}

fn workbook_source() {
    crate::shell::read_file::register_query_workbook_loader();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let path = crate::shell::scenario::repo_root().join("test/editing/xlsx-sales.xlsx");
    let m = format!(
        "let\n    ソース = Excel.Workbook(File.Contents(\"{}\"), null, true),\n    Data_Sheet = ソース{{[Item=\"月次売上\",Kind=\"Sheet\"]}}[Data]\nin\n    Data_Sheet", // i18n-ok（Excel が書く M の試験データ（手順名・シート名）。迷った：試験用の入力）
        path.display()
    );
    let query = m_to_query(&m, &Query::new("Q", Source::Blank)).expect("Excel's M is read");
    assert!(matches!(&query.source, Source::Workbook { sheet: true, item, .. } if item == "月次売上"), "{:?}", query.source); // i18n-ok（試験データのシート名）
    let table = s.with(|e, _| result_of(e.presentation(), &query, &[], &[], 0)).expect("the workbook is read");
    // The sheet's used range from A1: the columns Column1… and a row for each row of it (the blank ones too).
    assert_eq!(table.columns[0].name, "Column1");
    assert!(table.columns.len() >= 5 && table.rows.len() >= 9, "{} columns, {} rows", table.columns.len(), table.rows.len());
    // The same cells as the file's sheet.
    let direct = s.with(|e, _| {
        let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).expect("a sheet").clone();
        (0..table.rows.len() as u32).map(|r| (0..table.columns.len() as u32).map(|c| sheet.cell(r, c).map_or(ibis_calc::CellValue::Empty, |x| x.value.clone())).collect::<Vec<_>>()).collect::<Vec<_>>()
    });
    assert_eq!(table.rows, direct);
    // A file that is not there is named.
    let missing = m.replace(&path.display().to_string(), "/no/such/file.xlsx");
    let query = m_to_query(&missing, &Query::new("Q", Source::Blank)).unwrap();
    assert!(s.with(|e, _| result_of(e.presentation(), &query, &[], &[], 0)).is_err());
}

/// The M interpreter reads the same file through the same reading (`Excel.Workbook(File.Contents(path), …)`): the navigation to the sheet
/// is M's own (`Source{[Item=…,Kind="Sheet"]}[Data]`), the cells are the file's, and with the second argument true the first row is the names.
fn workbook_by_the_interpreter() {
    crate::shell::read_file::register_query_workbook_loader();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let path = crate::shell::scenario::repo_root().join("test/editing/xlsx-sales.xlsx");
    let m = |use_headers: &str| {
        format!(
            "let\n    ソース = Excel.Workbook(File.Contents(\"{}\"), {use_headers}, true),\n    Data_Sheet = ソース{{[Item=\"月次売上\",Kind=\"Sheet\"]}}[Data]\nin\n    Data_Sheet", // i18n-ok（Excel が書く M の試験データ）
            path.display()
        )
    };
    let by_steps = {
        let query = m_to_query(&m("null"), &Query::new("Q", Source::Blank)).expect("Excel's M is read");
        s.with(|e, _| result_of(e.presentation(), &query, &[], &[], 0)).expect("the workbook is read")
    };
    let by_m = ibis_doc_model::query::eval_m(&m("null"), &[]).expect("the interpreter reads the workbook");
    assert_eq!(by_m.names(), by_steps.names());
    assert_eq!(by_m.rows, by_steps.rows);
    // With useHeaders the first row of the sheet is the names of the columns.
    let promoted = ibis_doc_model::query::eval_m(&m("true"), &[]).expect("the headers are promoted");
    assert_eq!(promoted.rows.len(), by_steps.rows.len() - 1);
    assert_eq!(promoted.rows, by_steps.rows[1..].to_vec());
    assert_eq!(promoted.columns.len(), by_steps.columns.len());
    // A file that is not there is told, not a panic.
    assert!(ibis_doc_model::query::eval_m(&m("null").replace(&path.display().to_string(), "/no/such/file.xlsx"), &[]).is_err());
}
