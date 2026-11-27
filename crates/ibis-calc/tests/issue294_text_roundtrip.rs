// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue 294 (miz9): `TEXT` of a date as text, and the text turned back into a serial number, give Excel's values (design:
//! `tasks/doc/test-design/issue-294.md`; the numbers E1... are that document's).
//!
//! `data/issue294_text_roundtrip.tsv` is the 272 formats of the poster's TEXTTEST.xlsx with what Excel (Windows, ja-JP, measured in 2026)
//! gave for each: `TEXT(source, format)`, `--text` (the text turned into a serial), and `TEXT(that serial, format)` again. The source is
//! 2032-09-28 20:03:01 (serial 48485.835429999999), and 44:03:01 elapsed (1.8354282407407407) for the formats that start with `[`.
//! "This year" in the file is 2026, so the tests give the thread a clock in 2026 (a year that is not 2026 would change the expected
//! serials of the formats that leave the year out).
//!
//! Each test reports all the rows that differ, not only the first.
use ibis_calc::eval::convert::set_thread_clock;
use ibis_calc::eval::Book;
use ibis_calc::recalc::recalculate;
use ibis_calc::CellValue;

const DATE_SOURCE: &str = "48485.835429999999";
const ELAPSED_SOURCE: &str = "1.8354282407407407";

/// 2026-11-25 12:00:00 UTC, in a zone 9 hours east.
fn clock() -> (i64, i64) {
    (1_795_608_000_000, 9 * 3600)
}

struct Formulas(Vec<String>);

impl Book for Formulas {
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
        if column == 0 { self.0.get(row as usize).map(String::as_str) } else { None }
    }
    fn for_each_cell(&self, _sheet: usize, _area: (u32, u32, u32, u32), _f: &mut dyn FnMut(u32, u32, &CellValue)) {}
    fn extent(&self, _sheet: usize) -> (u32, u32) {
        (self.0.len() as u32 + 10, 10)
    }
    fn defined_name(&self, _name: &str, _sheet: Option<usize>) -> Option<String> {
        None
    }
    fn dynamic(&self, _sheet: usize, _row: u32, _column: u32) -> bool {
        true
    }
}

/// The values of the formulas, in order, with the thread's clock in 2026.
fn evaluate(formulas: Vec<String>) -> Vec<CellValue> {
    set_thread_clock(Some(clock));
    let cells: Vec<(usize, u32, u32)> = (0..formulas.len() as u32).map(|r| (0usize, r, 0u32)).collect();
    let res = recalculate(&Formulas(formulas), &cells, &cells);
    set_thread_clock(None);
    cells.iter().map(|c| res.iter().find(|r| r.cell == *c).expect("a result").value.clone()).collect()
}

struct Row {
    row: u32,
    format: String,
    text: String,
    serial: f64,
    text2: String,
}

fn rows() -> Vec<Row> {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/issue294_text_roundtrip.tsv")).unwrap();
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("row\t"))
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            Row { row: f[0].parse().unwrap(), format: f[1].to_string(), text: f[2].to_string(), serial: f[3].parse().unwrap(), text2: f[4].to_string() }
        })
        .collect()
}

fn source_of(format: &str) -> &'static str {
    if format.starts_with('[') { ELAPSED_SOURCE } else { DATE_SOURCE }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn show(v: &CellValue) -> String {
    match v {
        CellValue::Number(n) => format!("{n}"),
        CellValue::Text(t) => format!("{t:?}"),
        other => format!("{other:?}"),
    }
}

fn report(what: &str, diffs: Vec<String>, total: usize) {
    assert!(diffs.is_empty(), "{what}: {} of {total} rows differ from Excel:\n{}", diffs.len(), diffs.join("\n"));
}

/// E1: `TEXT(source, format)` is Excel's text, for every one of the 272 formats.
#[test]
fn e1_text_of_the_source_is_excels_text() {
    let rows = rows();
    let got = evaluate(rows.iter().map(|r| format!("TEXT({},{})", source_of(&r.format), quote(&r.format))).collect());
    let diffs = rows.iter().zip(&got).filter(|(r, g)| **g != CellValue::Text(r.text.clone())).map(|(r, g)| format!("row {} {:?}: Excel {:?}, ours {}", r.row, r.format, r.text, show(g))).collect();
    report("E1", diffs, rows.len());
}

/// E2: Excel's text turned into a serial by `--` is the serial Excel gave (the d/m, two-digit-year and left-out year or day cases among them).
#[test]
fn e2_text_back_to_a_serial_is_excels_serial() {
    let rows = rows();
    let got = evaluate(rows.iter().map(|r| format!("--{}", quote(&r.text))).collect());
    let diffs = rows
        .iter()
        .zip(&got)
        .filter(|(r, g)| !matches!(g, CellValue::Number(n) if (n - r.serial).abs() < 1e-9))
        .map(|(r, g)| format!("row {} {:?} (text {:?}): Excel {}, ours {}", r.row, r.format, r.text, r.serial, show(g)))
        .collect();
    report("E2", diffs, rows.len());
}

/// E3: Excel's serial written as text again with the format is Excel's second text (`TEXT(serial, format)`).
#[test]
fn e3_serial_back_to_text_is_excels_second_text() {
    let rows = rows();
    let got = evaluate(rows.iter().map(|r| format!("TEXT({},{})", r.serial, quote(&r.format))).collect());
    let diffs = rows.iter().zip(&got).filter(|(r, g)| **g != CellValue::Text(r.text2.clone())).map(|(r, g)| format!("row {} {:?}: Excel {:?}, ours {}", r.row, r.format, r.text2, show(g))).collect();
    report("E3", diffs, rows.len());
}

/// E4: the poster's judgment, all in our own engine: the serial the text turns into is written with the format as the same text (all 272 are `1` in Excel's column AU).
#[test]
fn e4_our_own_round_trip_gives_the_text_back() {
    let rows = rows();
    let got = evaluate(rows.iter().map(|r| format!("TEXT(--{},{})", quote(&r.text), quote(&r.format))).collect());
    let diffs = rows.iter().zip(&got).filter(|(r, g)| **g != CellValue::Text(r.text.clone())).map(|(r, g)| format!("row {} {:?}: text {:?} comes back as {}", r.row, r.format, r.text, show(g))).collect();
    report("E4", diffs, rows.len());
}

/// E5: the cases the issue names. `d/m` of 5/4 is "4/5", which reads back as April 5 (m/d is preferred); of May 28 it is "28/5", which cannot be
/// m/d, so it reads back as May 28. A two-digit year 32 is 1932 (the file's row for `yy/m/d`). A text with no year is this year (2026), with no day the 1st.
/// (The first two are the poster's statement; measure them on Windows, request W1.)
#[test]
fn e5_the_named_cases() {
    let cases: [(&str, f64); 8] = [
        ("--\"4/5\"", 46117.0),   // 2026-04-05
        ("--\"28/5\"", 46170.0),  // 2026-05-28
        ("--\"32/9/28\"", 11960.0), // 1932-09-28
        ("--\"9/28\"", 46293.0),  // 2026-09-28
        ("--\"2032/9\"", 48458.0), // 2032-09-01
        ("--\"9月28日\"", 46293.0),
        ("--\"32年9月\"", 11933.0), // 1932-09-01
        ("TEXT(DATE(2026,5,4),\"d/m\")=\"4/5\"", 1.0),
    ];
    let got = evaluate(cases.iter().map(|c| c.0.to_string()).collect());
    let diffs: Vec<String> = cases
        .iter()
        .zip(&got)
        .filter(|(c, g)| !match g {
            CellValue::Number(n) => (n - c.1).abs() < 1e-9,
            CellValue::Bool(b) => (*b as u8 as f64) == c.1,
            _ => false,
        })
        .map(|(c, g)| format!("{}: expected {}, ours {}", c.0, c.1, show(g)))
        .collect();
    report("E5", diffs, cases.len());
}

// ---- E6...: Excel's measurement of 2026-11-25 (`tasks/doc/requests/2026-11-25-294-excel-date-text-result.md`, Windows, ja-JP) ----

struct Measured {
    id: String,
    input: String,
    value: String,
    kind: String,
}

fn measured(prefix: &str) -> Vec<Measured> {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/issue294_excel_measured.tsv")).unwrap();
    text.lines()
        .skip(1)
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(|f| f[0] == prefix)
        .map(|f| Measured { id: f[0].to_string(), input: f[1].to_string(), value: f[2].to_string(), kind: f[3].to_string() })
        .collect()
}

fn matches_excel(v: &CellValue, expected: &str) -> bool {
    match v {
        CellValue::Number(n) => expected.parse::<f64>().is_ok_and(|e| (n - e).abs() < 1e-9),
        CellValue::Text(t) => t == expected,
        CellValue::Error(e) => format!("{e:?}").to_ascii_uppercase().contains("VALUE") && expected == "#VALUE!",
        _ => false,
    }
}

/// E6: W1, the ambiguous d/m (90 days of the file: `TEXT` as `d/m`, `d-m` and `m/d`, and the text turned back with `--`).
#[test]
fn e6_w1_ambiguous_day_month() {
    let rows = measured("W1");
    let mut formulas = Vec::new();
    for r in &rows {
        // "2026-01-02 TEXT(d/m)" or "2026-01-02 --TEXT(d/m) [2/1]"
        let (date, rest) = r.input.split_once(' ').unwrap();
        let mut ymd = date.split('-').map(|p| p.parse::<i32>().unwrap());
        let (y, m, d) = (ymd.next().unwrap(), ymd.next().unwrap(), ymd.next().unwrap());
        if r.kind == "text" {
            let fmt = rest.strip_prefix("TEXT(").unwrap().strip_suffix(')').unwrap();
            formulas.push(format!("TEXT(DATE({y},{m},{d}),{})", quote(fmt)));
        } else {
            let text = rest.split_once('[').unwrap().1.strip_suffix(']').unwrap();
            formulas.push(format!("--{}", quote(text)));
        }
    }
    let got = evaluate(formulas);
    let diffs = rows.iter().zip(&got).filter(|(r, g)| !matches_excel(g, &r.value)).map(|(r, g)| format!("{:?}: Excel {}, ours {}", r.input, r.value, show(g))).collect();
    report("E6", diffs, rows.len());
}

/// E7: W2 and W3, `--"text"` of two-digit years (29 is 2029, 30 is 1930), the left-out year and day, `9月` and a `2/29` of a year with no 29th.
#[test]
fn e7_w2_w3_text_to_serial() {
    let rows: Vec<Measured> = measured("W2").into_iter().chain(measured("W3")).collect();
    let got = evaluate(rows.iter().map(|r| r.input.clone()).collect());
    let diffs = rows.iter().zip(&got).filter(|(r, g)| !matches_excel(g, &r.value)).map(|(r, g)| format!("{}: Excel {}, ours {}", r.input, r.value, show(g))).collect();
    report("E7", diffs, rows.len());
}

/// E8: W4, what typing the text into a cell makes (`Range.Formula`): stricter than `--` (`29/1/1`, `32/9`, `2032/9`, `28/5`, `9月` stay text; `2/29` is February 2029).
#[test]
fn e8_w4_typed_into_a_cell() {
    use ibis_calc::input::{parse_input, Typed};
    let rows = measured("W4");
    set_thread_clock(Some(clock));
    let diffs: Vec<String> = rows
        .iter()
        .filter_map(|r| {
            let typed = r.input.strip_prefix("typed \"").unwrap().strip_suffix('"').unwrap();
            let got = match parse_input(typed, false) {
                Typed::Value(v, _) => v,
                Typed::Formula(f) => CellValue::Text(format!("={f}")),
            };
            // Excel's value equals the typed text exactly when the cell stayed text.
            let ok = if r.value == typed { got == CellValue::Text(typed.to_string()) } else { matches_excel(&got, &r.value) };
            (!ok).then(|| format!("typed {typed:?}: Excel {}, ours {}", r.value, show(&got)))
        })
        .collect();
    set_thread_clock(None);
    report("E8", diffs, rows.len());
}

/// E9: W5, the 1904 date system reads the same order (`4/5` is April 5, `28/5` May 28, `29/1/1` 2029) with serials 1462 lower.
#[test]
fn e9_w5_date_system_1904() {
    set_thread_clock(Some(clock));
    let diffs: Vec<String> = measured("W5-1904")
        .iter()
        .filter(|r| r.kind == "dashdash")
        .filter_map(|r| {
            let text = r.input.strip_prefix("--\"").unwrap().strip_suffix('"').unwrap();
            let got = ibis_calc::eval::convert::parse_date_time(text, true);
            (!got.is_some_and(|g| (g - r.value.parse::<f64>().unwrap()).abs() < 1e-9)).then(|| format!("{}: Excel {}, ours {got:?}", r.input, r.value))
        })
        .collect();
    set_thread_clock(None);
    report("E9", diffs, 5);
}

/// E10: W6, `G/標準` (and `g/標準`) is General in `TEXT`. (`TEXT(x,"General")` is `#VALUE!` in the Japanese Excel; ours reads it as General, kept: not a regression
/// of this issue and other locales use that word.)
#[test]
fn e10_w6_general_name() {
    let rows: Vec<Measured> = measured("W6").into_iter().filter(|r| r.input.contains("標準")).collect();
    let got = evaluate(rows.iter().map(|r| r.input.trim_start_matches('=').to_string()).collect());
    let diffs = rows.iter().zip(&got).filter(|(r, g)| !matches_excel(g, &r.value)).map(|(r, g)| format!("{}: Excel {}, ours {}", r.input, r.value, show(g))).collect();
    report("E10", diffs, rows.len());
}

// ---- E11: the second measurement of 2026-11-25 (`tasks/doc/requests/2026-11-25-evening-batch-294-result.md`, Windows, ja-JP) ----

struct Evening {
    input: String,
    how: String,
    value: String,
    kind: String,
}

fn evening() -> Vec<Evening> {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/issue294_evening.tsv")).unwrap();
    text.lines()
        .skip(1)
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .map(|f| Evening { input: f[1].to_string(), how: f[2].to_string(), value: f[3].to_string(), kind: f[4].to_string() })
        .collect()
}

/// E11: three numbers (`--` reads year/month/day only; the cell also month/day/year, and leaves a month over 12 as text), month and a two- or four-digit
/// year (cell only), `20時` (neither), `20時3分` and `8 PM` (both), the 29th of February.
#[test]
fn e11_three_numbers_month_year_hours() {
    use ibis_calc::input::{parse_input, Typed};
    let rows = evening();
    let dashes: Vec<&Evening> = rows.iter().filter(|r| r.how == "--").collect();
    let got = evaluate(dashes.iter().map(|r| format!("--{}", quote(&r.input))).collect());
    let mut diffs: Vec<String> = dashes.iter().zip(&got).filter(|(r, g)| !matches_excel(g, &r.value)).map(|(r, g)| format!("-- {:?}: Excel {}, ours {}", r.input, r.value, show(g))).collect();
    set_thread_clock(Some(clock));
    for r in rows.iter().filter(|r| r.how != "--") {
        let got = match parse_input(&r.input, false) {
            Typed::Value(v, _) => v,
            Typed::Formula(f) => CellValue::Text(format!("={f}")),
        };
        let ok = if r.kind == "text" { got == CellValue::Text(r.input.clone()) } else { matches_excel(&got, &r.value) };
        if !ok {
            diffs.push(format!("cell {:?}: Excel {}, ours {}", r.input, r.value, show(&got)));
        }
    }
    set_thread_clock(None);
    report("E11", diffs, rows.len());
}
