// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub #98 / #101: which values a LAMBDA helper (MAP, BYROW, BYCOL, REDUCE,
//! SCAN, MAKEARRAY, LET, LAMBDA called directly, GROUPBY) hands to its LAMBDA,
//! and which functions return a reference (TAKE, DROP, INDEX, OFFSET, XLOOKUP,
//! IF, CHOOSE, ...) or only values (CHOOSECOLS, SORT, FILTER, HSTACK, ...).
//! The expected grids are what Excel for Mac 16.114.3 (dynamic arrays) showed
//! in H1:L5 for `=<formula>` entered in H1 (measured 2026-11-02 with
//! `target/issue98/measure.applescript`; the table is
//! `tests/data/lambda_refs_excel.tsv`, `formula<TAB>rows~joined~by~tilde`, cells
//! joined by `|`). Data: A1:A3 = 10,20,30; B1:B3 = a,b,c; C1:C3 = 1,2,3;
//! A5 `=1+1`; A6 `=3*2`.
use ibis_calc::eval::Book;
use ibis_calc::recalc::recalculate;
use ibis_calc::CellValue;
use std::collections::HashMap;

struct Sheet {
    values: HashMap<(u32, u32), CellValue>,
    formulas: HashMap<(u32, u32), String>,
}

impl Book for Sheet {
    fn sheet_count(&self) -> usize {
        1
    }
    fn sheet_name(&self, _sheet: usize) -> &str {
        "Sheet1"
    }
    fn value(&self, _sheet: usize, row: u32, column: u32) -> CellValue {
        self.values.get(&(row, column)).cloned().unwrap_or(CellValue::Empty)
    }
    fn formula(&self, _sheet: usize, row: u32, column: u32) -> Option<&str> {
        self.formulas.get(&(row, column)).map(String::as_str)
    }
    fn for_each_cell(&self, _sheet: usize, (r0, c0, r1, c1): (u32, u32, u32, u32), f: &mut dyn FnMut(u32, u32, &CellValue)) {
        for r in r0..=r1 {
            for c in c0..=c1 {
                let v = self.value(0, r, c);
                if !v.is_empty() {
                    f(r, c, &v);
                }
            }
        }
    }
    fn extent(&self, _sheet: usize) -> (u32, u32) {
        (100, 100)
    }
    fn defined_name(&self, _name: &str, _sheet: Option<usize>) -> Option<String> {
        None
    }
    fn dynamic(&self, _sheet: usize, _row: u32, _column: u32) -> bool {
        true
    }
}

fn show(v: &CellValue) -> String {
    match v {
        CellValue::Empty => String::new(),
        // Excel's text of a cell in a narrow column: 8 decimals at most.
        CellValue::Number(n) if n.fract() != 0.0 => format!("{n:.8}").trim_end_matches('0').to_string(),
        CellValue::Number(n) => ibis_calc::number_to_xml(*n),
        CellValue::Text(t) => t.clone(),
        CellValue::Bool(b) => if *b { "TRUE" } else { "FALSE" }.into(),
        CellValue::Error(e) => e.as_str().to_string(),
    }
}

fn book(formula: &str) -> Sheet {
    let mut values = HashMap::new();
    for (i, n) in [10.0, 20.0, 30.0].into_iter().enumerate() {
        values.insert((i as u32, 0), CellValue::Number(n));
        values.insert((i as u32, 2), CellValue::Number(i as f64 + 1.0));
    }
    for (i, t) in ["a", "b", "c"].into_iter().enumerate() {
        values.insert((i as u32, 1), CellValue::Text(t.into()));
    }
    // F1:F3 hold errors (`=NA()`, `=1/0`, `=VALUE("x")`).
    values.insert((0, 5), CellValue::Error(ibis_calc::CellError::NA));
    values.insert((1, 5), CellValue::Error(ibis_calc::CellError::Div0));
    values.insert((2, 5), CellValue::Error(ibis_calc::CellError::Value));
    values.insert((4, 0), CellValue::Number(2.0));
    values.insert((5, 0), CellValue::Number(6.0));
    let mut formulas = HashMap::new();
    formulas.insert((4, 0), "1+1".to_string());
    formulas.insert((5, 0), "3*2".to_string());
    formulas.insert((0, 7), formula.to_string());
    Sheet { values, formulas }
}

/// The grid H1:L5 as the table writes it.
fn grid_of(formula: &str, size: (usize, usize)) -> String {
    let b = book(formula);
    let cell = (0usize, 0u32, 7u32);
    let res = recalculate(&b, &[cell], &[cell]);
    let r = res.iter().find(|r| r.cell == cell).expect("result");
    let mut rows: Vec<Vec<String>> = vec![vec![String::new(); size.1]; size.0];
    match &r.spill {
        Some((nr, nc, vals)) => {
            for i in 0..*nr {
                for j in 0..*nc {
                    if i < size.0 && j < size.1 {
                        rows[i][j] = show(&vals[i * nc + j]);
                    }
                }
            }
        }
        None => rows[0][0] = show(&r.value),
    }
    while rows.last().is_some_and(|r| r.iter().all(String::is_empty)) {
        rows.pop();
    }
    let ncol = rows.iter().map(|r| r.iter().rposition(|c| !c.is_empty()).map_or(0, |p| p + 1)).max().unwrap_or(0);
    rows.iter().map(|r| r[..ncol].join("|")).collect::<Vec<_>>().join("~")
}

#[test]
fn lambda_helpers_and_reference_functions_match_excel() {
    check("tests/data/lambda_refs_excel.tsv");
}

/// #99: function names as values (HSTACK(SUM,MAX), CHOOSE, IF, LET) given to
/// GROUPBY, PIVOTBY, MAP, REDUCE and the like, and the arguments of GROUPBY
/// (measured in Excel for Mac 16.114.3, 2026-11-02, `target/issue99/`).
#[test]
fn function_names_as_values_and_groupby_arguments_match_excel() {
    check("tests/data/groupby_funcs_excel.tsv");
}

/// The second round (2026-11-02): PIVOTBY and GROUPBY with a column of functions, PERCENTOF's relative_to, ARRAYTOTEXT of an error cell,
/// INDEX and CHOOSE with arrays, CELL of a union, TAKE with an array of counts, calling an array of functions, LET names as `x:x`.
#[test]
fn second_round_matches_excel() {
    check("tests/data/excel_round2.tsv");
}

/// #98 (third round, 2026-11-02, `target/issue98c/`): calling an array that holds a function value
/// (`HSTACK(SUM,MAX)`, `CHOOSE({1,2},SUM,MAX)`, `TAKE(f,,1)`, BYCOL's column of functions) makes Excel give up on the
/// whole formula, whatever wraps the call (ISERROR, ROWS, IFERROR, MAP, LET, ...): the cell shows 0. Calling an array of
/// other values gives #VALUE! for each element (the array's shape); a single function value (INDEX, MAP's element,
/// REDUCE's element, CHOOSE(1,...)) is called as usual.
#[test]
fn calling_an_array_of_functions_matches_excel() {
    check("tests/data/lambda_array_call_excel.tsv");
}

/// What Excel did with the cells that read a cell it gave up on (measured with a cell K2 `=H1+1` that held 6 while H1
/// was 5, then H1 became `=LAMBDA(f,f(A1))(HSTACK(SUM,MAX))`): H1 shows 0; K2 kept 6 (stale, also after a full
/// calculation and after A1 changed). A cell entered afterwards (`=H1+1`, `=ISERROR(H1)`, `=H1`) showed 0. The engine
/// reports the dependents as `unsupported`, so the caller keeps the value the cell had.
#[test]
fn cells_reading_a_given_up_cell_keep_their_value() {
    let mut b = book("LAMBDA(f,f(A1))(HSTACK(SUM,MAX))");
    // K1 `=H1+1` (6 before), K2 `=K1+1` (7 before), K3 `=A1+1`
    b.formulas.insert((0, 10), "H1+1".to_string());
    b.formulas.insert((1, 10), "K1+1".to_string());
    b.formulas.insert((2, 10), "A1+1".to_string());
    b.values.insert((0, 10), CellValue::Number(6.0));
    b.values.insert((1, 10), CellValue::Number(7.0));
    let all = [(0usize, 0u32, 7u32), (0, 0, 10), (0, 1, 10), (0, 2, 10)];
    let res = recalculate(&b, &[(0, 0, 7)], &all);
    let get = |r: u32, c: u32| res.iter().find(|x| x.cell == (0, r, c)).unwrap();
    assert_eq!(get(0, 7).value, CellValue::Number(0.0));
    assert!(!get(0, 7).unsupported);
    assert!(get(0, 10).unsupported);
    assert!(get(1, 10).unsupported);
    // K3 does not depend on H1: it is not calculated again.
    assert!(res.iter().all(|x| x.cell != (0, 2, 10)));
}

fn check(file: &str) {
    check_sized(file, (5, 5));
}

fn check_sized(file: &str, size: (usize, usize)) {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(file)).unwrap();
    let mut bad = Vec::new();
    let mut n = 0;
    for line in text.lines().filter(|l| !l.is_empty()) {
        let (f, want) = line.split_once('\t').unwrap();
        n += 1;
        let got = grid_of(f, size);
        if got != want {
            bad.push(format!("{f}\n    excel: {want}\n    ibis : {got}"));
        }
    }
    assert!(bad.is_empty(), "{} of {n} differ:\n{}", bad.len(), bad.join("\n"));
}

/// #98 (2026-11-02, `target/issue98c/forms4.txt`): PERCENTOF's `relative_to` (0 to 4) in PIVOTBY with two or more levels of
/// row and column fields and subtotals in both directions (`row_total_depth` and `col_total_depth` of 2 and -2, 1 and 0),
/// and a three-level row field; measured in Excel for Mac 16.114.3 as the H1:U16 grid.
#[test]
fn percentof_relative_to_with_nested_fields_matches_excel() {
    check_sized("tests/data/percentof_relative_excel.tsv", (16, 14));
}

/// #98: PIVOTBY's row_sort_order and col_sort_order (by the keys, then the total of each group) with SUM, with a row of
/// functions (HSTACK) and a column of functions (VSTACK), with and without subtotals (Excel for Mac 16.114.3).
#[test]
fn pivotby_sort_orders_with_function_arrays_match_excel() {
    check_sized("tests/data/pivotby_sort_excel.tsv", (16, 14));
}

/// #106 (2026-11-04, Excel on Windows via COM, `target/issue106/`): a function value (`SUM`, `IF({1},SUM)`, `INDEX(IF({1},SUM),1,1)`,
/// a LAMBDA, `HSTACK(SUM,MAX)`) in the array places and the function places of MAP, SCAN, REDUCE, BYROW, BYCOL, DROP, EXPAND,
/// TAKE, VSTACK, HSTACK and the rest, through `LET(fx,<value>,<body>)`.
#[test]
fn function_values_as_arguments_match_excel() {
    check("tests/data/function_args_excel.tsv");
}

/// #106: an array of function values (`HSTACK(SUM,MAX)`, `VSTACK`, `CHOOSE({1,2},SUM,MAX)`, an array of LAMBDAs, a function
/// next to a number) where REDUCE, SCAN, BYROW and BYCOL want their LAMBDA runs the helper once for each element (the first
/// value of each result); MAP and MAKEARRAY give #VALUE!. Same measurement.
#[test]
fn arrays_of_functions_given_to_helpers_match_excel() {
    check("tests/data/function_array_args_excel.tsv");
}

/// #106 (2026-11-04, Excel on Windows via COM): arrays (`{1,2}`, `{1}`, `{1,2;3,4}`, `{1,"a"}`), function values and arrays of them
/// (`HSTACK(SUM,MAX)`, `IF({1},SUM)`, `LAMBDA(x,x)`) given as the numeric argument of SEQUENCE, OFFSET, INDEX, REPT, LEFT, MID, ROUND, IF,
/// CHOOSE, TEXT, ABS, SQRT, MOD, POWER, DATE, LEN, UPPER, the operators and the rest. SEQUENCE, MAKEARRAY and EXPAND run once for each
/// element and keep its first value; OFFSET gives an array of #VALUE! of the shape; SUMPRODUCT of a function value is #VALUE!.
#[test]
fn arrays_as_numeric_arguments_match_excel() {
    check("tests/data/array_args_excel.tsv");
}

/// #141 (2026-11-09, Excel 16.0.20527 on Windows via COM, `Formula2` and the spill range): N and T of an array give an array, each element
/// made a number or text (`=N({TRUE,FALSE,"a",5})` spills 1, 0, 0, 5; errors stay in their place); a range (`N(A1:A6)`) is its first cell.
#[test]
fn n_and_t_of_arrays_spill_like_excel() {
    check("tests/data/n_t_arrays_excel.tsv");
}
