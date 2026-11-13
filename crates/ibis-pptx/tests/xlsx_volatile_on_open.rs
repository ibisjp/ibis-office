// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #177: a book saved by Excel months ago has TODAY() cached as that day. Excel calculates the volatile cells (and what
//! depends on them) again when it opens the book, unless the book is in manual calculation mode.
//! One test (the calculator's "today" is global to the process).

use ibis_doc_model::sheet_calc::sheet_of_slide;
use ibis_doc_model::CellValue;

fn number(name: &str, cell: (u32, u32)) -> (f64, bool) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name);
    let opened = ibis_pptx::open(std::fs::read(path).unwrap()).unwrap();
    let sheet = sheet_of_slide(&opened.presentation.slides[0]).unwrap();
    let CellValue::Number(n) = sheet.cell(cell.0, cell.1).unwrap().value.clone() else { panic!("not a number") };
    (n, false)
}

#[test]
fn volatile_cells_are_calculated_again_on_open() {
    // 2026-11-12 (the file was saved on 2026-02-11, serial 46064).
    ibis_calc::eval::convert::set_today(46338 - 25569);
    // The volatile cell and the cell that depends on it are today's; a plain value stays.
    assert_eq!(number("volatile-today.xlsx", (1, 1)).0, 46338.0);
    assert_eq!(number("volatile-today.xlsx", (1, 2)).0, 46339.0);
    assert_eq!(number("volatile-today.xlsx", (1, 3)).0, 5.0);
    // Manual calculation: the values the file has are kept.
    assert_eq!(number("volatile-today-manual.xlsx", (1, 1)).0, 46064.0);
    assert_eq!(number("volatile-today-manual.xlsx", (1, 2)).0, 46065.0);
    ibis_calc::eval::convert::set_today(i64::MIN);
}
