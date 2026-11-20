// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What the host gives: file reading (`Csv.Document`, `Excel.Workbook`), the clock (`Date.IsIn…`), and the lazy list (`List.Generate`).
use crate::ast::{Ty, TypeSpec};
use crate::*;
use std::rc::Rc;

struct Mock;

fn text_ty() -> TypeSpec {
    TypeSpec { ty: Ty::Text, nullable: true }
}

impl Host for Mock {
    fn read_csv(&self, path: &str, o: &CsvOptions) -> Result<Value, String> {
        if path != "C:\\work\\T.csv" {
            return Err(format!("no file {path}"));
        }
        // The options reach the host as they were written.
        assert_eq!((o.delimiter.as_str(), o.columns, o.encoding, o.quoted), (";", 3, 65001, false));
        Ok(Value::Table(Rc::new(Table {
            columns: vec!["Column1".into(), "Column2".into(), "Column3".into()],
            types: vec![text_ty(); 3],
            rows: vec![vec![Value::text("a"), Value::text("b"), Value::text("c")], vec![Value::text("1"), Value::text("2"), Value::text("3")]],
        })))
    }

    fn read_workbook(&self, _: &str) -> Result<Value, String> {
        let sheet = Value::Table(Rc::new(Table { columns: vec!["Column1".into(), "Column2".into()], types: vec![text_ty(); 2], rows: vec![vec![Value::text("h1"), Value::text("h2")], vec![Value::text("x"), Value::text("y")]] }));
        Ok(Value::Table(Rc::new(Table {
            columns: vec!["Name".into(), "Data".into(), "Item".into(), "Kind".into(), "Hidden".into()],
            types: vec![text_ty(); 5],
            rows: vec![vec![Value::text("Data"), sheet, Value::text("Data"), Value::text("Sheet"), Value::Logical(false)]],
        })))
    }
}

fn run(it: &Interp, src: &str) -> String {
    match it.run(src) {
        Ok(Value::Table(t)) => {
            let mut out = vec![t.columns.join(",")];
            for r in &t.rows {
                out.push(r.iter().map(|v| format!("{v:?}")).collect::<Vec<_>>().join(","));
            }
            out.join(" | ")
        }
        Ok(Value::Number(n)) => n.to_string(),
        Ok(Value::Logical(b)) => b.to_string(),
        Ok(Value::Null) => "null".into(),
        Ok(v) => format!("{v:?}"),
        Err(e) => format!("ERR {} {:?}", e.reason, e.kind),
    }
}

#[test]
fn csv_and_workbook_are_read_by_the_host() {
    let it = Interp::new();
    it.set_host(Rc::new(Mock));
    let csv = "Csv.Document(File.Contents(\"C:\\work\\T.csv\"),[Delimiter=\";\", Columns=3, Encoding=65001, QuoteStyle=QuoteStyle.None])";
    assert_eq!(run(&it, &format!("let Source = {csv}, P = Table.PromoteHeaders(Source, [PromoteAllScalars=true]) in P")), "a,b,c | \"1\",\"2\",\"3\"");
    // The file is only named by File.Contents.
    assert_eq!(run(&it, "File.Contents(\"x\")"), "binary(x)");
    assert!(run(&it, "Csv.Document(File.Contents(\"nofile\"))").starts_with("ERR DataFormat.Error"));
    let nav = "let Source = Excel.Workbook(File.Contents(\"C:\\work\\B.xlsx\"), null, true), Data_Sheet = Source{[Item=\"Data\",Kind=\"Sheet\"]}[Data] in Data_Sheet";
    assert_eq!(run(&it, nav), "Column1,Column2 | \"h1\",\"h2\" | \"x\",\"y\"");
    // With the first row as the names (the second argument true).
    assert_eq!(run(&it, &nav.replace("null, true", "true")), "h1,h2 | \"x\",\"y\"");
    // Without a host a file cannot be read.
    assert!(run(&Interp::new(), "Csv.Document(File.Contents(\"x\"))").starts_with("ERR Expression.Error"));
}

#[test]
fn the_clock_and_the_windows_of_dates() {
    let it = Interp::new();
    // Tuesday 2026-11-17 13:05:09.
    it.set_now(value::days_from_civil(2026, 11, 17), 13.0 * 3600.0 + 5.0 * 60.0 + 9.0);
    assert_eq!(run(&it, "Date.From(DateTime.LocalNow())"), "date(739937)".replace("739937", &value::days_from_civil(2026, 11, 17).to_string()));
    assert_eq!(run(&it, "DateTime.LocalNow() = DateTime.FixedLocalNow()"), "true");
    for (src, want) in [
        ("Date.IsInCurrentDay(#date(2026, 11, 17))", "true"),
        ("Date.IsInPreviousDay(#date(2026, 11, 16))", "true"),
        ("Date.IsInNextDay(#date(2026, 11, 18))", "true"),
        ("Date.IsInCurrentWeek(#date(2026, 11, 15))", "true"),
        ("Date.IsInCurrentWeek(#date(2026, 11, 21))", "true"),
        ("Date.IsInCurrentWeek(#date(2026, 11, 22))", "false"),
        ("Date.IsInCurrentWeek(#date(2026, 11, 14))", "false"),
        ("Date.IsInPreviousWeek(#date(2026, 11, 14))", "true"),
        ("Date.IsInCurrentMonth(#date(2026, 11, 1))", "true"),
        ("Date.IsInCurrentMonth(#date(2026, 10, 31))", "false"),
        ("Date.IsInPreviousNDays(#date(2026, 11, 15), 2)", "true"),
        ("Date.IsInPreviousNDays(#date(2026, 11, 14), 2)", "false"),
        ("Date.IsInPreviousNDays(#date(2026, 11, 17), 2)", "false"),
        ("Date.IsInNextNMonths(#date(2027, 1, 31), 2)", "true"),
        ("Date.IsInNextNMonths(#date(2027, 2, 1), 2)", "false"),
        ("Date.IsInCurrentQuarter(#date(2026, 10, 1))", "true"),
        ("Date.IsInPreviousQuarter(#date(2026, 9, 30))", "true"),
        ("Date.IsInCurrentYear(#date(2026, 1, 1))", "true"),
        ("Date.IsInPreviousYear(#date(2025, 12, 31))", "true"),
        ("Date.IsInPreviousNYears(#date(2023, 6, 1), 3)", "true"),
        ("Date.IsInYearToDate(#date(2026, 11, 17))", "true"),
        ("Date.IsInYearToDate(#date(2026, 11, 18))", "false"),
        ("DateTime.IsInCurrentDay(#datetime(2026, 11, 17, 23, 59, 59))", "true"),
        ("Date.IsInCurrentMonth(null)", "null"),
    ] {
        assert_eq!(run(&it, src), want, "{src}");
    }
    // The first day of the week can be given.
    assert_eq!(run(&it, "Date.IsInCurrentWeek(#date(2026, 11, 22), Day.Monday)"), "true");
    // Without a clock there is no now.
    assert!(run(&Interp::new(), "Date.IsInCurrentDay(#date(2026, 11, 17))").starts_with("ERR Expression.Error"));
}

#[test]
fn a_generated_list_is_made_as_it_is_asked_for() {
    let it = Interp::new();
    let endless = "List.Generate(() => 0, each true, each _ + 1)";
    assert_eq!(run(&it, &format!("List.Count(List.FirstN({endless}, 3))")), "3");
    assert_eq!(run(&it, &format!("{endless}{{4}}")), "4");
    assert_eq!(run(&it, &format!("List.First({endless})")), "0");
    assert_eq!(run(&it, &format!("List.Count(List.FirstN({endless}, each _ < 5))")), "5");
    assert_eq!(run(&it, &format!("List.Sum(List.Range({endless}, 2, 3))")), "9");
    assert_eq!(run(&it, "List.Count(List.Generate(() => 1, each _ < 5, each _ + 1))"), "4");
    assert_eq!(run(&it, "List.Sum(List.Generate(() => [i = 0, v = 1], each [i] < 4, each [i = [i] + 1, v = [v] * 2], each [v]))"), "15");
    // The whole of an endless list stops with a limit, not a hang.
    assert!(run(&it, &format!("List.Count({endless})")).contains("Limit"));
    // A name that is made once is made once.
    assert_eq!(run(&it, &format!("let g = {endless} in g{{2}} + g{{2}}")), "4");
}
