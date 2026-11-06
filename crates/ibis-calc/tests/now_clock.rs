// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `NOW` reads the clock at each call, in 10-millisecond steps (Excel 16.0 build 20527, measured over COM:
//! `LET(a,NOW(),s,SEQUENCE(2^20),b,NOW(),(b-a)*86400)` gave 0.03 to 0.04 s; two calls with no work between gave 0;
//! `LET(f,LAMBDA(RAND()),a,f(),b,f(),a=b)` is FALSE). The tests give the thread a clock that moves 1 ms per read,
//! so they do not depend on the system time.
use ibis_calc::eval::convert::set_thread_clock;
use ibis_calc::eval::Book;
use ibis_calc::recalc::recalculate;
use ibis_calc::CellValue;
use std::cell::Cell;

thread_local! {
    static MILLIS: Cell<i64> = const { Cell::new(0) };
}

/// 2026-11-03 12:00:00 UTC plus one millisecond per read, in a zone 9 hours east.
fn clock() -> (i64, i64) {
    (1_793_707_200_000 + MILLIS.with(|m| m.replace(m.get() + 1)), 9 * 3600)
}

struct One(String);

impl Book for One {
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
        if (row, column) == (0, 0) { Some(&self.0) } else { None }
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

fn number(formula: &str) -> f64 {
    MILLIS.with(|m| m.set(0));
    set_thread_clock(Some(clock));
    let cell = (0usize, 0u32, 0u32);
    let res = recalculate(&One(formula.to_string()), &[cell], &[cell]);
    set_thread_clock(None);
    match &res.iter().find(|r| r.cell == cell).expect("result").value {
        CellValue::Number(n) => *n,
        other => panic!("{formula} gave {other:?}"),
    }
}

/// The time of day, in seconds, of a serial number.
fn seconds_of_day(serial: f64) -> f64 {
    ((serial - serial.floor()) * 86400.0 * 100.0).round() / 100.0
}

#[test]
fn now_is_the_local_time_to_10_ms() {
    // 12:00:00 UTC + 9 h = 21:00:00; the first read is at 0 ms.
    assert_eq!(seconds_of_day(number("NOW()")), 21.0 * 3600.0);
}

#[test]
fn later_calls_in_one_formula_see_later_time() {
    // Reads at 0 and 1 ms (the second read is also the first NOW's neighbour): both in the same 10 ms step.
    assert_eq!(number("LET(a,NOW(),b,NOW(),(b-a)*86400)"), 0.0);
    // Thirty reads of the clock (each NOW() reads once, so burn 29 with a MAP) put the next NOW in a later step.
    let d = number("LET(a,NOW(),x,MAP(SEQUENCE(29),LAMBDA(i,NOW())),b,NOW(),ROUND((b-a)*86400,2))");
    assert_eq!(d, 0.03);
}

#[test]
fn a_lambda_reads_the_clock_at_each_call() {
    let d = number("LET(f,LAMBDA(NOW()),a,f(),x,MAP(SEQUENCE(19),LAMBDA(i,f())),b,f(),ROUND((b-a)*86400,2))");
    assert_eq!(d, 0.02, "21 calls of f read the clock 21 times");
}

#[test]
fn the_expression_of_the_issue_gives_elapsed_time() {
    let d = number("LET(Xtime,LAMBDA(LET(n,NOW(),LAMBDA(ROUND((NOW()-n)*24*3600,2)))),_t,Xtime(),x,MAP(SEQUENCE(49),LAMBDA(i,NOW())),_t())");
    assert!(d > 0.0, "{d}");
}

#[test]
fn rand_differs_at_each_call_of_a_lambda() {
    assert_eq!(number("LET(f,LAMBDA(RAND()),a,f(),b,f(),IF(a=b,1,0))"), 0.0);
}

/// FORECAST.ETS evaluated its date argument twice (once to see whether it is a range, once as a value).
#[test]
fn forecast_ets_reads_its_date_argument_once() {
    // The clock moves 1 ms per read: one evaluation of NOW() reads once; the target 46000.5 is a few reads from now.
    let reads_of = |formula: &str| {
        let _ = number(formula);
        MILLIS.with(|m| m.get())
    };
    let rest = "{1;2;3;4;5;6},{46000;46001;46002;46003;46004;46005})";
    let now = reads_of("NOW()");
    let fixed = reads_of(&format!("FORECAST.ETS(46006,{rest}"));
    let with_now = reads_of(&format!("FORECAST.ETS(NOW(),{rest}"));
    assert!(with_now <= now, "NOW() read {now}, base {fixed}, with NOW {with_now}");
}
