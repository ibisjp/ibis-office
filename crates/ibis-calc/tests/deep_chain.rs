// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Review 0.8.5 (再計算の Tarjan 法が再帰): a running-total column of a million rows is one dependency chain a million
//! cells long. The calculation walks it with an explicit stack, so it runs on a thread with the small stack of the web version.
use ibis_calc::eval::Book;
use ibis_calc::recalc::recalculate;
use ibis_calc::CellValue;

const ROWS: u32 = 1_000_000;

/// Column A: A1 is 1; A(n) is `=A(n-1)+1`.
struct Running {
    formulas: Vec<String>,
}

impl Book for Running {
    fn sheet_count(&self) -> usize {
        1
    }
    fn sheet_name(&self, _sheet: usize) -> &str {
        "Sheet1"
    }
    fn value(&self, _sheet: usize, row: u32, column: u32) -> CellValue {
        if (row, column) == (0, 0) { CellValue::Number(1.0) } else { CellValue::Empty }
    }
    fn formula(&self, _sheet: usize, row: u32, column: u32) -> Option<&str> {
        if column == 0 && row >= 1 { self.formulas.get(row as usize - 1).map(|s| s.as_str()) } else { None }
    }
    fn for_each_cell(&self, _s: usize, _a: (u32, u32, u32, u32), _f: &mut dyn FnMut(u32, u32, &CellValue)) {}
    fn extent(&self, _sheet: usize) -> (u32, u32) {
        (ROWS, 1)
    }
    fn defined_name(&self, _name: &str, _sheet: Option<usize>) -> Option<String> {
        None
    }
    fn dynamic(&self, _sheet: usize, _row: u32, _column: u32) -> bool {
        false
    }
}

#[test]
fn a_chain_of_a_million_cells_is_calculated_on_a_small_stack() {
    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(|| {
            let formulas: Vec<String> = (1..ROWS).map(|r| format!("A{r}+1")).collect();
            let book = Running { formulas };
            let cells: Vec<(usize, u32, u32)> = (1..ROWS).map(|r| (0usize, r, 0u32)).collect();
            let t0 = ibis_calc::clock::monotonic_nanos().unwrap();
            let results = recalculate(&book, &cells, &cells);
            let seconds = (ibis_calc::clock::monotonic_nanos().unwrap() - t0) as f64 / 1e9;
            eprintln!("recalculation of a {ROWS}-row running total: {seconds:.2} s");
            let last = results.iter().find(|r| r.cell == (0, ROWS - 1, 0)).expect("the last cell");
            assert_eq!(last.value, CellValue::Number(ROWS as f64));
        })
        .unwrap();
    handle.join().unwrap();
}
