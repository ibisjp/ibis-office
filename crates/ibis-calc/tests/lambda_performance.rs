// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub #109: a LAMBDA that reads a LET name holding a 2^20-element array, called recursively, was
//! 70 times slower than Excel (116 s against 1.63 s). The time is read through the crate's `clock`.
use ibis_calc::eval::Book;
use ibis_calc::recalc::recalculate;
use ibis_calc::CellValue;

struct Empty;

impl Book for Empty {
    fn sheet_count(&self) -> usize {
        1
    }
    fn sheet_name(&self, _sheet: usize) -> &str {
        "Sheet1"
    }
    fn value(&self, _sheet: usize, _row: u32, _column: u32) -> CellValue {
        CellValue::Empty
    }
    fn formula(&self, _sheet: usize, row: u32, column: u32) -> Option<&str> {
        if (row, column) == (0, 0) { Some(FORMULA) } else { None }
    }
    fn for_each_cell(&self, _sheet: usize, _area: (u32, u32, u32, u32), _f: &mut dyn FnMut(u32, u32, &CellValue)) {}
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

/// The formula of the issue (`SIZE` is 2^20 there).
const FORMULA: &str = "LET(Xtime,LAMBDA(LET(n,NOW(),LAMBDA(ROUND((NOW()-n)*24*3600,2)))),_t,Xtime(),a,SEQUENCE(SIZE),b,10,f,LAMBDA(LET(_1,a+1,_1)),g,LAMBDA(g,n,LET(h,f(),IF(n<1,h,g(g,n-1)))),i,g(g,b),_t())";

fn run(size: usize) -> (f64, f64) {
    run_formula(&FORMULA.replace("SIZE", &size.to_string()))
}

fn run_formula(formula: &str) -> (f64, f64) {
    struct Sized(Empty, String);
    impl Book for Sized {
        fn sheet_count(&self) -> usize {
            1
        }
        fn sheet_name(&self, s: usize) -> &str {
            self.0.sheet_name(s)
        }
        fn value(&self, s: usize, r: u32, c: u32) -> CellValue {
            self.0.value(s, r, c)
        }
        fn formula(&self, _sheet: usize, row: u32, column: u32) -> Option<&str> {
            if (row, column) == (0, 0) { Some(&self.1) } else { None }
        }
        fn for_each_cell(&self, _s: usize, _a: (u32, u32, u32, u32), _f: &mut dyn FnMut(u32, u32, &CellValue)) {}
        fn extent(&self, s: usize) -> (u32, u32) {
            self.0.extent(s)
        }
        fn defined_name(&self, _n: &str, _s: Option<usize>) -> Option<String> {
            None
        }
        fn dynamic(&self, _s: usize, _r: u32, _c: u32) -> bool {
            true
        }
    }
    let book = Sized(Empty, formula.to_string());
    let cell = (0usize, 0u32, 0u32);
    let t0 = ibis_calc::clock::monotonic_nanos().unwrap();
    let res = recalculate(&book, &[cell], &[cell]);
    let seconds = (ibis_calc::clock::monotonic_nanos().unwrap() - t0) as f64 / 1e9;
    let r = res.iter().find(|r| r.cell == cell).expect("result");
    match &r.value {
        CellValue::Number(n) => (*n, seconds),
        other => panic!("the formula gave {other:?}"),
    }
}

/// IF evaluated the branch it took twice (once to see whether it was a reference, once as a value), so a recursive
/// LAMBDA doing its recursion in IF did 2^depth calls: the issue's `g(g,b)` with b = 10 made 1024 calls of `f()`, each
/// building the array again. With depth 40 this does not finish unless each level is evaluated once.
#[test]
fn recursion_inside_if_is_evaluated_once_per_level() {
    let f = FORMULA.replace("SIZE", "4").replace("b,10", "b,40");
    let (_, seconds) = run_formula(&f);
    eprintln!("depth 40: {seconds:.3} s");
    assert!(seconds < 10.0, "{seconds} s");
}

/// The issue's formula as reported: 2^20 elements (Excel 1.63 s, ibis 116 s before the fix).
#[test]
fn issue_109_formula_at_two_to_the_twenty() {
    let (_, seconds) = run(1 << 20);
    eprintln!("2^20: {seconds:.3} s");
    assert!(seconds < 10.0, "{seconds} s");
}

/// Stage times (printed with --nocapture): where the time of the 2^18-element case goes.
#[test]
#[ignore = "profiling aid"]
fn stages() {
    for depth in [0, 1, 2, 4, 8] {
        let f = FORMULA.replace("SIZE", "16384").replace("b,10", &format!("b,{depth}"));
        let (v, t) = run_formula(&f);
        eprintln!("{t:.3} s  depth {depth} -> {v}");
    }
    for f in [
        "LET(a,SEQUENCE(2^18),1)",
        "LET(a,SEQUENCE(2^18),b,a+1,1)",
        "LET(a,SEQUENCE(2^18),b,a+1,c,a+1,d,a+1,e,a+1,1)",
        "LET(a,SEQUENCE(2^18),f,LAMBDA(a+1),b,f(),1)",
        "LET(a,SEQUENCE(2^18),f,LAMBDA(a+1),b,f(),c,f(),d,f(),e,f(),1)",
        "LET(a,SEQUENCE(2^18),f,LAMBDA(LET(_1,a+1,_1)),g,LAMBDA(n,LET(h,f(),IF(n<1,h,1))),i,g(0),1)",
    ] {
        let (v, t) = run_formula(f);
        eprintln!("{t:.3} s  {f}  -> {v}");
    }
}
