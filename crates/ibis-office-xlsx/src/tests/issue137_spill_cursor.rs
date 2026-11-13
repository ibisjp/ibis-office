// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #137: with `=SEQUENCE(2^20,5)` in A1 the arrow keys answered late, and after the formula was deleted they
//! stayed late. Measures what one key press costs on a sheet with a million-row spill, and after the formula is gone.
use super::SheetSteps;
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn spill_cursor_cost() {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Workbook);
    s.size(1200., 720.);
    let keys: usize = std::env::var("IBIS_I137_KEYS").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
    let t = crate::clock::Instant::now();
    for _ in 0..keys {
        s.key("right");
    }
    eprintln!("issue137: empty sheet: {keys} x right took {:?}", t.elapsed());
    for _ in 0..keys {
        s.key("left");
    }
    let t = crate::clock::Instant::now();
    s.click_cell("A1");
    s.type_text("=SEQUENCE(2^20,5)");
    s.key("enter");
    eprintln!("issue137: entering the formula took {:?}", t.elapsed());
    s.click_cell("A1");
    let cells = s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).map_or(0, |sh| sh.cells.len()));
    eprintln!("issue137: cells in the sheet: {cells}");
    assert_eq!(cells, 5 << 20, "the spill is in the sheet");
    // What a frame visits for the selection is a handful of cells (the row of the cursor), not the 5 million of the spill; and
    // the spill adds no rows to the list of row heights (the list is walked by many frames' steps).
    let (visited, rows) = s.with(|e, _| {
        let sh = ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).unwrap();
        (crate::sheet_ui::cells_of_selection(sh, e.current_cell(), &e.selected_ranges()).count(), sh.rows.len())
    });
    eprintln!("issue137: cells visited for the selection: {visited}, rows listed: {rows}");
    assert!(visited <= 5 * 2, "visited {visited} cells for one selected cell");
    assert!(rows < 1000, "the spill grew the row list to {rows}");
    for round in ["with the spill", "after delete"] {
        if round == "after delete" {
            s.click_cell("A1");
            s.key("delete");
            let cells = s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).map_or(0, |sh| sh.cells.len()));
            eprintln!("issue137: cells after delete: {cells}");
            assert_eq!(cells, 0, "deleting the formula takes the spilled values away");
            let rows = s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide()).map_or(0, |sh| sh.rows.len()));
            assert!(rows < 1000, "after the delete the row list is {rows} long");
        }
        let t = crate::clock::Instant::now();
        for _ in 0..keys {
            s.key("right");
        }
        eprintln!("issue137: {round}: {keys} x right took {:?}", t.elapsed());
        for _ in 0..keys {
            s.key("left");
        }
    }
}
