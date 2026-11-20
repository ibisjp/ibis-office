// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The values Microsoft 365 Excel's Power Query gave (Japanese install, ja-JP), from
//! `tasks/doc/requests/2026-11-17-k0039-m-semantics-result.md`; each assert names the section of that file. Nothing here is a coefficient: the
//! rules the values show (round half to even after scaling, UTF-16 text, the week count, the order of types) are written once, in the code.
use crate::*;

fn show(it: &Interp, v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Logical(b) => b.to_string(),
        Value::Number(n) => crate::culture::number_text(*n),
        Value::Text(s) => format!("\"{s}\""),
        Value::Failed(e) => format!("!{}", e.reason),
        Value::Gen(_) => show(it, &Value::list(crate::builtin::list_items(it, v).unwrap_or_default())),
        Value::List(items) => format!("{{{}}}", items.iter().map(|l| l.force(it).map(|v| show(it, &v)).unwrap_or_else(|e| format!("!{}", e.reason))).collect::<Vec<_>>().join(", ")),
        Value::Date(d) => {
            let (y, m, dd) = value::civil_from_days(*d);
            format!("#date({y}, {m}, {dd})")
        }
        Value::Table(t) => {
            let cell = |v: &Value| show(it, v);
            let mut out = vec![t.columns.join(",")];
            for r in &t.rows {
                out.push(r.iter().map(cell).collect::<Vec<_>>().join(","));
            }
            out.join(" | ")
        }
        v => format!("{v:?}"),
    }
}

fn ev(src: &str) -> String {
    let it = Interp::new();
    match it.run(src) {
        Ok(v) => show(&it, &v),
        Err(e) => format!("ERR {}", e.reason),
    }
}

#[test]
fn rounding_section_4() {
    for (src, want) in [
        ("Number.Round(2.5)", "2"),
        ("Number.Round(3.5)", "4"),
        ("Number.Round(-2.5)", "-2"),
        ("Number.Round(0.5)", "0"),
        ("Number.Round(1.5)", "2"),
        ("Number.Round(1.2345, 2)", "1.23"),
        ("Number.Round(2.675, 2)", "2.68"),
        ("Number.Round(1234.5, -2)", "1200"),
        ("Number.Round(2.5, 0, RoundingMode.AwayFromZero)", "3"),
        ("Number.Round(2.5, 0, RoundingMode.ToEven)", "2"),
        ("Number.Round(-0.5, 0, RoundingMode.Down)", "-1"),
        ("Number.Round(-0.5, 0, RoundingMode.Up)", "0"),
        ("Number.Round(-0.5, 0, RoundingMode.TowardZero)", "0"),
        ("Number.Round(2.5, 0, RoundingMode.Up)", "3"),
        ("Number.RoundUp(-1.2)", "-1"),
        ("Number.RoundDown(-1.2)", "-2"),
        ("Number.RoundUp(1.0000000001)", "2"),
        ("Number.Round(1.005, 2)", "1"),
        ("Number.Round(0.285, 2)", "0.28"),
        ("Number.Round(-1.5)", "-2"),
        ("Number.Round(2.5, 0, RoundingMode.Down)", "2"),
        ("Number.Round(2.5, 1, RoundingMode.Up)", "2.5"),
        ("Number.Round(0.125, 2)", "0.12"),
        ("Number.Round(0.135, 2)", "0.14"),
    ] {
        assert_eq!(ev(src), want, "{src}");
    }
}

#[test]
fn division_and_records_sections_2_and_3() {
    assert_eq!(ev("1 / 0"), "\u{221e}");
    assert_eq!(ev("-1 / 0"), "-\u{221e}");
    assert_eq!(ev("#nan = #nan"), "false");
    assert_eq!(ev("Number.Power(2, 3)"), "8");
    assert_eq!(ev("[a=1][b]?"), "null");
    assert_eq!(ev("[a=1,a=2]"), "ERR Expression.Error");
    assert_eq!(ev("{1,2}{5}?"), "null");
    assert_eq!(ev("{1,2}{5}"), "ERR Expression.Error");
    // Section 1: none of these is M.
    for src in ["1 ?? error \"x\"", "true or error \"x\"", "1 + if true then 1 else 2", "not if true then false else true", "1 + each _", "1 + let x = 1 in x", "1 + try 1", "2 ^ 3"] {
        assert!(ev(src).starts_with("ERR"), "{src}");
    }
    assert_eq!(ev("1 + (error \"x\")"), "ERR Expression.Error");
}

#[test]
fn types_section_5() {
    let conv = |v: &str, ty: &str| ev(&format!("Table.TransformColumnTypes(#table({{\"v\"}}, {{{{{v}}}}}), {{{{\"v\", {ty}}}}})"));
    for (v, ty, want) in [
        ("2.5", "Int64.Type", "v | 2"),
        ("3.5", "Int64.Type", "v | 4"),
        ("-2.5", "Int64.Type", "v | -2"),
        ("0.5", "Int64.Type", "v | 0"),
        ("\"2.5\"", "Int64.Type", "v | 2"),
        ("\" 12 \"", "type number", "v | 12"),
        ("\"1,234\"", "type number", "v | 1234"),
        ("\"1.234,5\"", "type number", "v | !DataFormat.Error"),
        ("\"1e3\"", "type number", "v | 1000"),
        ("\"0x10\"", "type number", "v | !DataFormat.Error"),
        ("\"+5\"", "type number", "v | 5"),
        ("\"-\"", "type number", "v | !DataFormat.Error"),
        ("\"\"", "type number", "v | null"),
        ("\"true\"", "type number", "v | !DataFormat.Error"),
        ("true", "type number", "v | 1"),
        ("12345.67891", "Currency.Type", "v | 12345.6789"),
        ("12345.67891", "Percentage.Type", "v | 12345.67891"),
        ("2147483648", "Int32.Type", "v | !Expression.Error"),
        ("2147483648", "Int64.Type", "v | 2147483648"),
        ("1e21", "Int64.Type", "v | !Expression.Error"),
        ("\"true\"", "type logical", "v | true"),
        ("\"TRUE\"", "type logical", "v | true"),
        ("\"yes\"", "type logical", "v | !Expression.Error"),
        ("2", "type logical", "v | true"),
        ("0", "type logical", "v | false"),
        ("\"\"", "type logical", "v | null"),
        ("1e21", "type text", "v | \"1000000000000000000000\""),
        ("2.5", "type text", "v | \"2.5\""),
        ("true", "type text", "v | \"true\""),
    ] {
        assert_eq!(conv(v, ty), want, "{v} as {ty}");
    }
}

#[test]
fn number_and_date_as_text_section_6() {
    for (src, want) in [
        ("Text.From(0.1 + 0.2)", "\"0.30000000000000004\""),
        ("Text.From(1 / 3)", "\"0.33333333333333331\""),
        ("Text.From(-0)", "\"0\""),
        ("Text.From(1e-7)", "\"1E-07\""),
        ("Text.From(-1.5e-5)", "\"-1.5E-05\""),
        ("Text.From(1e15)", "\"1000000000000000\""),
        ("Text.From(1e21)", "\"1000000000000000000000\""),
        ("Text.From(1234567.891)", "\"1234567.891\""),
        ("Text.From(Number.Power(2, 53))", "\"9007199254740992\""),
        ("Text.From(#nan)", "\"NaN\""),
        ("Text.From(true)", "\"true\""),
        ("Text.From(#date(2026, 11, 17))", "\"2026/11/17\""),
        ("Text.From(#date(2026, 11, 17), \"en-US\")", "\"11/17/2026\""),
        ("Text.From(#datetime(2026, 11, 17, 13, 5, 9))", "\"2026/11/17 13:05:09\""),
        ("Text.From(#datetime(2026, 11, 17, 13, 5, 9), \"en-US\")", "\"11/17/2026 1:05:09 PM\""),
        ("Text.From(#datetime(2026, 11, 17, 0, 0, 0))", "\"2026/11/17 0:00:00\""),
        ("Text.From(#datetime(2026, 11, 17, 0, 0, 0), \"en-US\")", "\"11/17/2026 12:00:00 AM\""),
        ("Text.From(#time(13, 5, 9))", "\"13:05\""),
        ("Text.From(#time(13, 5, 9), \"en-US\")", "\"1:05 PM\""),
        ("Text.From(#duration(1, 2, 3, 4.5))", "\"1.02:03:04.5000000\""),
        ("Text.From(null)", "null"),
        ("Number.ToText(0.1 + 0.2, \"G\")", "\"0.3\""),
        ("Number.ToText(1 / 3, \"G\")", "\"0.333333333333333\""),
    ] {
        assert_eq!(ev(src), want, "{src}");
    }
}

#[test]
fn calendar_section_7() {
    for (src, want) in [
        ("Date.DayOfWeek(#date(2026, 11, 15))", "0"),
        ("Date.DayOfWeek(#date(2026, 11, 15), Day.Monday)", "6"),
        ("Date.DayOfWeekName(#date(2026, 11, 17))", "\"火曜日\""),
        ("Date.DayOfWeekName(#date(2026, 11, 17), \"en-US\")", "\"Tuesday\""),
        ("Date.MonthName(#date(2026, 11, 17))", "\"11月\""),
        ("Date.MonthName(#date(2026, 11, 17), \"en-US\")", "\"November\""),
        ("Date.StartOfWeek(#date(2026, 11, 17))", "#date(2026, 11, 15)"),
        ("Date.StartOfWeek(#date(2026, 11, 17), Day.Monday)", "#date(2026, 11, 16)"),
        ("Date.EndOfWeek(#date(2026, 11, 17))", "#date(2026, 11, 21)"),
        ("Date.EndOfWeek(#date(2026, 11, 17), Day.Monday)", "#date(2026, 11, 22)"),
        ("Date.WeekOfYear(#date(2026, 1, 1))", "1"),
        ("Date.WeekOfYear(#date(2026, 1, 1), Day.Monday)", "1"),
        ("Date.WeekOfYear(#date(2026, 1, 4))", "2"),
        ("Date.WeekOfYear(#date(2026, 1, 4), Day.Monday)", "1"),
        ("Date.WeekOfYear(#date(2025, 12, 29))", "53"),
        ("Date.WeekOfYear(#date(2025, 12, 29), Day.Monday)", "53"),
        ("Date.WeekOfYear(#date(2026, 12, 31))", "53"),
        ("Date.WeekOfYear(#date(2026, 12, 31), Day.Monday)", "53"),
        ("Date.WeekOfMonth(#date(2026, 11, 1))", "1"),
        ("Date.WeekOfMonth(#date(2026, 11, 8))", "2"),
        ("Date.WeekOfMonth(#date(2026, 11, 30))", "5"),
        ("Date.WeekOfMonth(#date(2026, 11, 1), Day.Monday)", "1"),
        ("Date.WeekOfMonth(#date(2026, 11, 8), Day.Monday)", "2"),
        ("Date.WeekOfMonth(#date(2026, 11, 30), Day.Monday)", "6"),
        ("Date.AddMonths(#date(2026, 3, 31), -1)", "#date(2026, 2, 28)"),
        ("Date.IsLeapYear(#date(1900, 1, 1))", "false"),
    ] {
        assert_eq!(ev(src), want, "{src}");
    }
}

#[test]
fn date_from_text_section_8() {
    for (text, want) in [
        ("2026-11-17", "#date(2026, 11, 17)"),
        ("2026/11/17", "#date(2026, 11, 17)"),
        ("11/17/2026", "#date(2026, 11, 17)"),
        ("2026年11月17日", "#date(2026, 11, 17)"),
        ("20261117", "#date(2026, 11, 17)"),
        ("Nov 17, 2026", "#date(2026, 11, 17)"),
        ("17-Nov-2026", "#date(2026, 11, 17)"),
        ("1/2/2026", "#date(2026, 1, 2)"),
        ("17/11/2026", "ERR DataFormat.Error"),
        ("2026-11-17T13:05:09", "ERR DataFormat.Error"),
        ("2026-11-17 13:05:09", "ERR DataFormat.Error"),
        ("2026-02-30", "ERR DataFormat.Error"),
        ("", "null"),
    ] {
        for culture in ["", ", \"en-US\"", ", \"ja-JP\""] {
            let src = format!("Date.FromText(\"{text}\"{culture})");
            // The empty text is null for a date (`Date.FromText("")`).
            assert_eq!(ev(&src), want, "{src}");
        }
    }
    assert_eq!(ev("Date.From(46343)"), "#date(2026, 11, 17)");
    assert_eq!(ev("Date.From(46343.75)"), "#date(2026, 11, 17)");
    assert_eq!(ev("Date.From(60)"), "#date(1900, 2, 28)");
    assert_eq!(ev("Date.From(-1)"), "#date(1899, 12, 29)");
    assert_eq!(ev("Date.From(\"46343\")"), "ERR DataFormat.Error");
    assert_eq!(ev("Number.From(#date(2026, 11, 17))"), "46343");
    assert_eq!(ev("Number.From(#datetime(2026, 11, 17, 12, 0, 0))"), "46343.5");
    assert_eq!(ev("#time(13, 5, 9) + #duration(0, 12, 0, 0)"), "time(3909)");
}

#[test]
fn text_comparison_and_sort_section_10() {
    for (src, want) in [
        ("\"a\" < \"B\"", "false"),
        ("\"a\" = \"A\"", "false"),
        ("\"a\" < \"あ\"", "true"),
        ("\"ア\" = \"ｱ\"", "false"),
        ("\"B\" < \"a\"", "true"),
        ("\"é\" < \"f\"", "false"),
        ("\"\" < \"a\"", "true"),
        ("\"ア\" < \"あ\"", "false"),
        ("\"é\" = \"e#(0301)\"", "false"),
        ("Comparer.OrdinalIgnoreCase(\"a\", \"A\")", "0"),
        ("Comparer.Ordinal(\"a\", \"A\")", "1"),
        ("Comparer.OrdinalIgnoreCase(\"ａ\", \"A\")", "1"),
        ("List.Sort({\"b\",\"A\",\"a\",\"B\"}, Comparer.OrdinalIgnoreCase)", "{\"A\", \"a\", \"b\", \"B\"}"),
        ("List.Sort({\"b\",\"a\",\"_\",\"B\",\"A\",\"2\",\"10\",\"1\",\"-\"})", "{\"-\", \"1\", \"10\", \"2\", \"A\", \"B\", \"_\", \"a\", \"b\"}"),
        ("List.Distinct({\"a\",\"A\",\"ａ\"})", "{\"a\", \"A\", \"ａ\"}"),
        ("Text.Contains(\"abc\", \"B\")", "false"),
        ("Text.Contains(\"abc\", \"B\", Comparer.OrdinalIgnoreCase)", "true"),
        ("Text.Replace(\"aAa\", \"a\", \"x\")", "\"xAx\""),
        ("Text.PositionOf(\"aAa\", \"A\")", "1"),
        ("List.Max({\"a\", \"B\"})", "\"a\""),
        ("List.Min({\"a\", \"B\"})", "\"B\""),
        ("List.Sort({2, null, 1})", "{null, 1, 2}"),
        ("List.Sort({2, null, 1}, Order.Descending)", "{2, 1, null}"),
        ("List.Sort({\"b\", 2, null, \"a\", 1, true, #date(2026, 1, 1)})", "{null, #date(2026, 1, 1), 1, 2, true, \"a\", \"b\"}"),
        ("List.Sort({true, false})", "{false, true}"),
    ] {
        assert_eq!(ev(src), want, "{src}");
    }
}

#[test]
fn case_trim_and_utf16_sections_11_and_12() {
    for (src, want) in [
        ("Text.Upper(\"straße\")", "\"STRAßE\""),
        ("Text.Upper(\"ǆ\")", "\"Ǆ\""),
        ("Text.Upper(\"ａｂｃ\")", "\"ＡＢＣ\""),
        ("Text.Lower(\"İ\")", "\"i\""),
        ("Text.Lower(\"ΑΣ\")", "\"ασ\""),
        ("Text.Lower(\"ǅ\")", "\"ǆ\""),
        ("Text.Proper(\"o'neil mc-donald 1st ａｂｃ ǆ the.end\")", "\"O'Neil Mc-Donald 1St Ａｂｃ Ǆ The.End\""),
        ("Text.Proper(\"ABC dEF ghi_jkl 9ab\")", "\"Abc Def Ghi_Jkl 9Ab\""),
        ("Text.Proper(\"ß straße\")", "\"ß Straße\""),
        ("Text.Trim(\" #(tab)#(00A0)#(3000)a#(2003) \")", "\"a\""),
        ("Text.Trim(\"#(lf)#(cr)a#(000B)#(000C)\")", "\"a\""),
        ("Text.Trim(\"#(0085)a#(2028)#(2029)\")", "\"a\""),
        ("Text.Trim(\"xxaxx\", \"x\")", "\"a\""),
        ("Text.Length(\"ａ😀é\")", "4"),
        ("Text.Length(\"😀\")", "2"),
        ("Text.Length(\"ａ😀e#(0301)\")", "5"),
        ("Text.Range(\"abc\", 3)", "\"\""),
        ("Text.Range(\"abc\", 4)", "ERR Expression.Error"),
        ("Text.Range(\"abc\", 1, 5)", "ERR Expression.Error"),
        ("Text.Range(\"abc\", -1)", "ERR Expression.Error"),
        ("Text.PadStart(\"5\", 3, \"ab\")", "ERR Expression.Error"),
        ("Text.PadStart(\"5\", 3, \"\")", "ERR Expression.Error"),
        ("Text.AfterDelimiter(\"a-b-c\", \"-\", 5)", "\"\""),
        ("Text.AfterDelimiter(\"a-b-c\", \"\")", "ERR Expression.Error"),
        ("Text.BetweenDelimiters(\"a(b(c)d)e\", \"(\", \")\", 0, 1)", "\"b(c)d\""),
        ("Text.Split(\"みかん\", \"ん\")", "{\"みか\", \"\"}"),
        ("Text.Split(\"ん\", \"ん\")", "{\"\", \"\"}"),
        ("Text.Split(\"abc\", \"\")", "{\"abc\"}"),
        ("Text.Split(null, \",\")", "ERR Expression.Error"),
        ("Text.Combine({1,2}, \"-\")", "ERR Expression.Error"),
        ("Text.Remove(\"a😀b\", {\"😀\"})", "ERR Expression.Error"),
        ("Text.Select(\"a😀b1\", {\"a\"..\"z\"})", "\"ab\""),
    ] {
        assert_eq!(ev(src), want, "{src}");
    }
    // A surrogate cut in two is shown as U+FFFD (Excel shows the same).
    assert_eq!(ev("Text.Range(\"a😀b\", 1, 1)"), "\"\u{fffd}\"");
    // Text.Clean takes the control characters (category Cc) only.
    assert_eq!(ev("Text.Clean(\"a#(tab)b#(lf)c#(00A0)d#(200B)e\")"), "\"abc\u{a0}d\u{200b}e\"");
    assert_eq!(ev("Text.Clean(\"a#(0085)b#(2028)c#(007F)d#(0080)e#(001F)f\")"), "\"ab\u{2028}cdef\"");
}

#[test]
fn aggregates_section_13() {
    for (src, want) in [
        ("List.Sum({})", "null"),
        ("List.Sum({null})", "null"),
        ("List.Sum({1,null,2})", "3"),
        ("List.Sum({\"a\"})", "ERR Expression.Error"),
        ("List.Sum({true})", "ERR Expression.Error"),
        ("List.Sum({0.1, 0.2, 0.3})", "0.60000000000000009"),
        ("List.Average({})", "null"),
        ("List.Average({1,2,null})", "1.5"),
        ("List.Max({})", "null"),
        ("List.Max({}, 0)", "0"),
        ("List.Max({1, null})", "1"),
        ("List.Max({1, \"a\"})", "\"a\""),
        ("List.Max({#nan, 1})", "1"),
        ("List.Median({1,2,3,4})", "2.5"),
        ("List.Median({})", "null"),
        ("List.Mode({1,1,2,2})", "2"),
        ("List.Mode({2,2,1,1})", "1"),
        ("List.Mode({})", "null"),
        ("List.Modes({1,1,2,2})", "{1, 2}"),
        ("List.StandardDeviation({1,2,3,4})", "1.2909944487358056"),
        ("List.StandardDeviation({1})", "0"),
        ("List.StandardDeviation({})", "null"),
        ("List.Product({})", "null"),
        ("List.Product({2, null, 3})", "6"),
        ("List.Count({null})", "1"),
        ("List.Distinct({1, 1.0, \"1\"})", "{1, \"1\"}"),
        ("List.Numbers(0, 3, 0.1)", "{0, 0.1, 0.2}"),
        ("List.Numbers(5, 3, -2)", "{5, 3, 1}"),
    ] {
        assert_eq!(ev(src), want, "{src}");
    }
}

const T14: &str = "#table({\"k\",\"v\"}, {{\"b\",1},{\"a\",2},{\"b\",3},{\"c\",4},{\"a\",5}})";
const T14N: &str = "#table({\"k\",\"v\"}, {{null,1},{\"a\",2},{null,3},{\"A\",4},{\"a\",5}})";

#[test]
fn group_and_reorder_sections_14_and_15() {
    let s = "{{\"s\", each List.Sum([v]), type number}}";
    assert_eq!(ev(&format!("Table.Group({T14}, {{\"k\"}}, {s})")), "k,s | \"b\",4 | \"a\",7 | \"c\",4");
    assert_eq!(ev(&format!("Table.Group({T14}, {{\"k\"}}, {s}, GroupKind.Local)")), "k,s | \"b\",1 | \"a\",2 | \"b\",3 | \"c\",4 | \"a\",5");
    assert_eq!(ev(&format!("Table.Group({T14N}, {{\"k\"}}, {s})")), "k,s | null,4 | \"a\",7 | \"A\",4");
    assert_eq!(ev(&format!("Table.Group({T14N}, {{\"k\"}}, {s}, GroupKind.Global, Comparer.OrdinalIgnoreCase)")), "k,s | null,4 | \"a\",11");
    assert_eq!(ev(&format!("Table.Group({T14}, {{}}, {{{{\"n\", each Table.RowCount(_)}}}})")), "n | 5");
    // The type of an aggregated column: the one given, else any; the key keeps its own.
    let it = Interp::new();
    let Ok(Value::Table(t)) = it.run(&format!("Table.Group({T14}, {{\"k\"}}, {s})")) else { panic!() };
    assert_eq!(t.types[1].ty, ast::Ty::Number);
    let Ok(Value::Table(t)) = it.run(&format!("Table.Group({T14}, {{\"k\"}}, {{{{\"s\", each List.Sum([v])}}}})")) else { panic!() };
    assert_eq!(t.types[1].ty, ast::Ty::Any);
    // An aggregation that fails is the error of its cell.
    assert_eq!(ev(&format!("Table.Group({T14}, {{\"k\"}}, {{{{\"s\", each error \"boom\"}}}})")), "k,s | \"b\",!Expression.Error | \"a\",!Expression.Error | \"c\",!Expression.Error");
    let r = "#table({\"a\",\"b\",\"c\",\"d\"}, {{1,2,3,4}})";
    assert_eq!(ev(&format!("Table.ReorderColumns({r}, {{\"c\"}})")), "a,b,c,d | 1,2,3,4");
    assert_eq!(ev(&format!("Table.ReorderColumns({r}, {{\"c\",\"a\"}})")), "c,b,a,d | 3,2,1,4");
    assert_eq!(ev(&format!("Table.ReorderColumns({r}, {{\"d\",\"b\"}})")), "a,d,c,b | 1,4,3,2");
    assert_eq!(ev(&format!("Table.ReorderColumns({r}, {{\"c\",\"x\"}})")), "ERR Expression.Error");
    assert_eq!(ev(&format!("Table.ReorderColumns({r}, {{\"c\",\"x\"}}, MissingField.Ignore)")), "a,b,c,d | 1,2,3,4");
    assert_eq!(ev(&format!("Table.ReorderColumns({r}, {{\"c\",\"x\"}}, MissingField.UseNull)")), "a,b,c,d,x | 1,2,3,4,null");
    assert_eq!(ev(&format!("Table.ReorderColumns({r}, {{\"c\",\"c\"}})")), "ERR Expression.Error");
    assert_eq!(ev(&format!("Table.ReorderColumns({r}, {{}})")), "a,b,c,d | 1,2,3,4");
}

#[test]
fn headers_pivot_unpivot_sections_16_to_18() {
    assert_eq!(ev("Table.PromoteHeaders(#table({\"x\",\"y\",\"z\",\"w\"}, {{\"a\",\"a\",\"\",\"x\"},{1,2,3,4}}))"), "a,a_1,,x | 1,2,3,4");
    assert_eq!(ev("Table.PromoteHeaders(#table({\"x\",\"y\",\"z\"}, {{null,\"b\",#date(2026,1,1)},{1,2,3}}))"), "Column1,b,Column3 | 1,2,3");
    assert_eq!(ev("Table.PromoteHeaders(#table({\"x\",\"y\",\"z\"}, {{null,\"b\",#date(2026,1,1)},{1,2,3}}), [PromoteAllScalars=true])"), "Column1,b,2026/01/01 | 1,2,3");
    assert_eq!(ev("Table.PromoteHeaders(#table({\"x\",\"y\",\"z\"}, {{true,1.5,#datetime(2026,1,1,0,0,0)},{1,2,3}}))"), "Column1,1.5,Column3 | 1,2,3");
    assert_eq!(ev("Table.PromoteHeaders(#table({\"x\",\"y\",\"z\"}, {{true,1.5,#datetime(2026,1,1,0,0,0)},{1,2,3}}), [PromoteAllScalars=true])"), "true,1.5,2026/01/01 0:00:00 | 1,2,3");
    assert_eq!(ev("Table.PromoteHeaders(#table({\"x\",\"y\",\"z\"}, {{\"a\",\"A\",\"a\"},{1,2,3}}))"), "a,A,a_1 | 1,2,3");
    assert_eq!(ev("Table.PromoteHeaders(#table({\"c1\",\"c2\"}, {}))"), "c1,c2");
    let p = "#table({\"k\",\"a\",\"v\"}, {{1,\"x\",10},{1,\"y\",20},{2,\"x\",30},{3,null,50}})";
    assert_eq!(ev(&format!("Table.Pivot({p}, {{\"x\",\"y\"}}, \"a\", \"v\")")), "k,x,y | 1,10,20 | 2,30,null | 3,null,null");
    assert_eq!(ev(&format!("Table.Pivot({p}, {{\"y\",\"x\"}}, \"a\", \"v\", List.Sum)")), "k,y,x | 1,20,10 | 2,null,30 | 3,null,null");
    assert_eq!(ev(&format!("Table.Pivot({p}, {{\"x\",\"y\",\"z\"}}, \"a\", \"v\", List.Sum)")), "k,x,y,z | 1,10,20,null | 2,30,null,null | 3,null,null,null");
    assert_eq!(ev(&format!("Table.Pivot({p}, {{\"x\",\"y\"}}, \"a\", \"v\", List.Count)")), "k,x,y | 1,1,1 | 2,1,0 | 3,0,0");
    assert_eq!(ev(&format!("Table.Pivot({p}, List.Distinct({p}[a]), \"a\", \"v\", List.Sum)")), "ERR Expression.Error");
    assert_eq!(ev("Table.Pivot(#table({\"k\",\"a\",\"v\"}, {{1,\"x\",10},{1,\"X\",20}}), {\"x\"}, \"a\", \"v\", List.Sum)"), "k,x | 1,10");
    assert_eq!(ev("Table.Pivot(#table({\"k\",\"a\",\"v\"}, {{1,1,10}}), {1}, \"a\", \"v\", List.Sum)"), "ERR Expression.Error");
    assert_eq!(ev("Table.Pivot(#table({\"k\",\"a\",\"v\"}, {{1,\"x\",10}}), {\"k\"}, \"a\", \"v\", List.Sum)"), "ERR Expression.Error");
    let u = "#table({\"k\",\"a\",\"b\"}, {{1,10,11},{2,null,21},{3,30,31}})";
    assert_eq!(ev(&format!("Table.Unpivot({u}, {{\"a\",\"b\"}}, \"attr\", \"val\")")), "k,attr,val | 1,\"a\",10 | 1,\"b\",11 | 2,\"b\",21 | 3,\"a\",30 | 3,\"b\",31");
    assert_eq!(ev(&format!("Table.Unpivot({u}, {{\"b\",\"a\"}}, \"attr\", \"val\")")), "k,attr,val | 1,\"a\",10 | 1,\"b\",11 | 2,\"b\",21 | 3,\"a\",30 | 3,\"b\",31");
    assert_eq!(ev(&format!("Table.UnpivotOtherColumns({u}, {{\"a\",\"b\"}}, \"attr\", \"val\")")), "a,b,attr,val | 10,11,\"k\",1 | null,21,\"k\",2 | 30,31,\"k\",3");
    assert_eq!(ev(&format!("Table.Unpivot({u}, {{}}, \"attr\", \"val\")")), "ERR Expression.Error");
    assert_eq!(ev(&format!("Table.Unpivot({u}, {{\"a\",\"b\"}}, \"k\", \"val\")")), "ERR Expression.Error");
    assert_eq!(ev(&format!("Table.Unpivot({u}, {{\"a\",\"x\"}}, \"attr\", \"val\")")), "ERR Expression.Error");
    let it = Interp::new();
    let Ok(Value::Table(t)) = it.run("Table.Unpivot(Table.TransformColumnTypes(#table({\"k\",\"a\",\"b\"}, {{1,10,11}}), {{\"k\", Int64.Type},{\"a\", Int64.Type},{\"b\", type number}}), {\"a\",\"b\"}, \"attr\", \"val\")") else { panic!() };
    assert_eq!(t.types.iter().map(|x| x.ty).collect::<Vec<_>>(), [ast::Ty::Int64, ast::Ty::Text, ast::Ty::Number]);
}

#[test]
fn joins_section_19() {
    let l = "#table({\"k\",\"v\"}, {{1,\"a\"},{2,\"b\"},{2,\"c\"}})";
    let r2 = "#table({\"k2\",\"w\"}, {{2,\"x\"},{3,\"y\"},{2,\"z\"}})";
    let j = |kind: &str| ev(&format!("Table.Join({l}, {{\"k\"}}, {r2}, {{\"k2\"}}, {kind})"));
    assert_eq!(j("JoinKind.Inner"), "k,v,k2,w | 2,\"b\",2,\"x\" | 2,\"b\",2,\"z\" | 2,\"c\",2,\"x\" | 2,\"c\",2,\"z\"");
    assert_eq!(j("JoinKind.LeftOuter"), "k,v,k2,w | 2,\"b\",2,\"x\" | 2,\"b\",2,\"z\" | 2,\"c\",2,\"x\" | 2,\"c\",2,\"z\" | 1,\"a\",null,null");
    assert_eq!(j("JoinKind.RightOuter"), "k,v,k2,w | 2,\"b\",2,\"x\" | 2,\"b\",2,\"z\" | 2,\"c\",2,\"x\" | 2,\"c\",2,\"z\" | null,null,3,\"y\"");
    assert_eq!(j("JoinKind.FullOuter"), "k,v,k2,w | 2,\"b\",2,\"x\" | 2,\"b\",2,\"z\" | 2,\"c\",2,\"x\" | 2,\"c\",2,\"z\" | null,null,3,\"y\" | 1,\"a\",null,null");
    assert_eq!(j("JoinKind.LeftAnti"), "k,v,k2,w | 1,\"a\",null,null");
    assert_eq!(j("JoinKind.RightAnti"), "k,v,k2,w | null,null,3,\"y\"");
    assert_eq!(ev(&format!("Table.Join({l}, {{\"k\"}}, {r2}, {{\"k2\"}})")), j("JoinKind.Inner"));
    // The same key name on both sides is one column; another repeated name is an error.
    let r = "#table({\"k\",\"w\"}, {{2,\"x\"},{3,\"y\"},{2,\"z\"}})";
    assert_eq!(ev(&format!("Table.Join({l}, {{\"k\"}}, {r}, {{\"k\"}}, JoinKind.Inner)")), "k,v,w | 2,\"b\",\"x\" | 2,\"b\",\"z\" | 2,\"c\",\"x\" | 2,\"c\",\"z\"");
    assert_eq!(ev(&format!("Table.Join({l}, {{\"k\"}}, #table({{\"k2\",\"v\"}}, {{{{2,\"q\"}}}}), {{\"k2\"}}, JoinKind.Inner)")), "ERR Expression.Error");
    // Nested: null matches null, the case of a text matters.
    assert_eq!(ev("Table.NestedJoin(#table({\"k\",\"v\"}, {{null,\"a\"},{1,\"b\"}}), {\"k\"}, #table({\"k\",\"w\"}, {{null,\"x\"}}), {\"k\"}, \"r\", JoinKind.Inner)"), "k,v,r | null,\"a\",k,w | null,\"x\"");
    assert_eq!(ev(&format!("Table.NestedJoin({l}, {{\"k\"}}, {r}, {{\"k\"}}, \"k\", JoinKind.Inner)")), "ERR Expression.Error");
    assert_eq!(ev(&format!("Table.NestedJoin({l}, {{\"k\"}}, {r}, {{\"k\",\"w\"}}, \"r\", JoinKind.Inner)")), "ERR Expression.Error");
    assert_eq!(ev(&format!("Table.ExpandTableColumn(Table.NestedJoin({l}, {{\"k\"}}, {r}, {{\"k\"}}, \"r\", JoinKind.LeftOuter), \"r\", {{\"k\",\"w\"}})")), "ERR Expression.Error");
    assert_eq!(ev(&format!("Table.ExpandTableColumn(Table.NestedJoin({l}, {{\"k\"}}, {r}, {{\"k\"}}, \"r\", JoinKind.LeftOuter), \"r\", {{\"k\",\"w\"}}, {{\"rk\",\"rw\"}})")), "k,v,rk,rw | 1,\"a\",null,null | 2,\"b\",2,\"x\" | 2,\"b\",2,\"z\" | 2,\"c\",2,\"x\" | 2,\"c\",2,\"z\"");
}

#[test]
fn fill_distinct_split_error_cells_sections_20_to_26() {
    let f = "#table({\"c\",\"d\"}, {{\"a\",1},{null,null},{\"\",2},{\"b\",null}})";
    assert_eq!(ev(&format!("Table.FillDown({f}, {{\"c\"}})")), "c,d | \"a\",1 | \"a\",null | \"\",2 | \"b\",null");
    assert_eq!(ev(&format!("Table.FillUp({f}, {{\"c\"}})")), "c,d | \"a\",1 | \"\",null | \"\",2 | \"b\",null");
    assert_eq!(ev(&format!("Table.FillDown({f}, {{}})")), "c,d | \"a\",1 | null,null | \"\",2 | \"b\",null");
    assert_eq!(ev("Table.Distinct(#table({\"v\",\"i\"}, {{\"a\",1},{\"A\",2},{\"a \",3},{\"a\",4}}), {\"v\"})"), "v,i | \"a\",1 | \"A\",2 | \"a \",3");
    assert_eq!(ev("Table.Distinct(#table({\"v\",\"i\"}, {{\"a\",1},{\"A\",2},{\"a\",3}}), {{\"v\", Comparer.OrdinalIgnoreCase}})"), "v,i | \"a\",1");
    assert_eq!(ev("Table.Distinct(#table({\"v\"}, {{1},{1.0}}))"), "v | 1");
    assert_eq!(ev("Table.Distinct(#table({\"v\"}, {{#date(2026,1,1)},{#datetime(2026,1,1,0,0,0)}}))").matches('|').count(), 2);
    assert_eq!(ev("Table.Distinct(#table({\"v\",\"w\"}, {{\"a\",1}}), {\"x\"})"), "ERR Expression.Error");
    let sp = "Splitter.SplitTextByDelimiter(\",\")";
    let t = "#table({\"a\",\"b\"}, {{\"x,y,z\",1},{\"p\",2},{null,3},{\"q,r\",4}})";
    assert_eq!(ev(&format!("Table.SplitColumn({t}, \"a\", {sp})")), "a.1,a.2,a.3,b | \"x\",\"y\",\"z\",1 | \"p\",null,null,2 | null,null,null,3 | \"q\",\"r\",null,4");
    assert_eq!(ev(&format!("Table.SplitColumn({t}, \"a\", {sp}, 3)")), "a.1,a.2,a.3,b | \"x\",\"y\",\"z\",1 | \"p\",null,null,2 | null,null,null,3 | \"q\",\"r\",null,4");
    assert_eq!(ev(&format!("Table.SplitColumn({t}, \"a\", {sp}, {{\"a1\",\"a2\"}})")), "a1,a2,b | \"x\",\"y\",1 | \"p\",null,2 | null,null,3 | \"q\",\"r\",4");
    assert_eq!(ev(&format!("Table.SplitColumn({t}, \"a\", {sp}, {{\"b\",\"a2\"}})")), "ERR Expression.Error");
    assert_eq!(ev(&format!("Table.SplitColumn({t}, \"x\", {sp})")), "ERR Expression.Error");
    assert_eq!(ev(&format!("Table.SplitColumn(#table({{\"a\"}}, {{{{1}}}}), \"a\", {sp})")), "ERR Expression.Error");
    assert_eq!(ev(&format!("Table.SplitColumn(#table({{\"a\"}}, {{{{\"\"}}}}), \"a\", {sp})")), "a.1 | \"\"");
    // Error cells (section 23): made lazily, one cell each.
    let e = "Table.TransformColumns(#table({\"v\",\"w\"}, {{10,\"a\"},{20,\"b\"},{30,\"c\"}}), {{\"v\", each if _ = 20 then error \"boom\" else _}})";
    assert_eq!(ev(&format!("Table.RowCount({e})")), "3");
    assert_eq!(ev(&format!("Table.ToRows({e})")).contains('!'), true);
    assert_eq!(ev(&format!("Table.RemoveRowsWithErrors({e})")), "v,w | 10,\"a\" | 30,\"c\"");
    assert_eq!(ev(&format!("Table.SelectRowsWithErrors({e})")), "v,w | !Expression.Error,\"b\"");
    assert_eq!(ev(&format!("Table.RemoveRowsWithErrors({e}, {{\"w\"}})")), "v,w | 10,\"a\" | !Expression.Error,\"b\" | 30,\"c\"");
    assert_eq!(ev(&format!("Table.ReplaceErrorValues({e}, {{{{\"v\", 0}}}})")), "v,w | 10,\"a\" | 0,\"b\" | 30,\"c\"");
    assert_eq!(ev(&format!("Table.SelectRows({e}, each [v] > 0)")), "ERR Expression.Error");
    assert_eq!(ev(&format!("Table.RowCount(Table.SelectRows({e}, each [w] <> \"a\"))")), "2");
    // ReplaceValue (section 26).
    let tn = "#table({\"v\"}, {{1},{2},{12},{null}})";
    assert_eq!(ev(&format!("Table.ReplaceValue({tn}, \"1\", \"x\", Replacer.ReplaceText, {{\"v\"}})")), "v | !Expression.Error | !Expression.Error | !Expression.Error | null");
    assert_eq!(ev(&format!("Table.ReplaceValue({tn}, 1, 9, Replacer.ReplaceValue, {{\"v\"}})")), "v | 9 | 2 | 12 | null");
    assert_eq!(ev(&format!("Table.ReplaceValue({tn}, null, 0, Replacer.ReplaceValue, {{\"v\"}})")), "v | 1 | 2 | 12 | 0");
    let ts = "#table({\"v\"}, {{\"a\"},{\"\"},{null},{\"aa\"}})";
    assert_eq!(ev(&format!("Table.ReplaceValue({ts}, \"\", \"E\", Replacer.ReplaceValue, {{\"v\"}})")), "v | \"a\" | \"E\" | null | \"aa\"");
    assert_eq!(ev(&format!("Table.ReplaceValue({ts}, \"a\", \"b\", Replacer.ReplaceText, {{\"v\"}})")), "v | \"b\" | \"\" | null | \"bb\"");
    assert_eq!(ev(&format!("Table.ReplaceValue({ts}, null, \"N\", Replacer.ReplaceText, {{\"v\"}})")), "v | !Expression.Error | !Expression.Error | !Expression.Error | !Expression.Error");
    assert_eq!(ev("Replacer.ReplaceText(null, \"b\", \"x\")"), "null");
    assert_eq!(ev("Replacer.ReplaceText(\"abc\", \"\", \"x\")"), "\"abc\"");
}

#[test]
fn transpose_combine_sort_types_sections_24_to_29() {
    assert_eq!(ev("Table.Transpose(#table({\"a\",\"b\"}, {{1,2}}))"), "Column1 | 1 | 2");
    assert_eq!(ev("Table.Transpose(#table({\"a\"}, {}))"), "");
    assert_eq!(ev("Table.Transpose(#table({\"a\",\"b\"}, {{1,2}}), {\"p\",\"q\"})"), "ERR Expression.Error");
    assert_eq!(ev("Table.Transpose(#table({\"a\",\"b\"}, {{1,2},{3,4}}), {\"p\",\"q\"})"), "p,q | 1,3 | 2,4");
    assert_eq!(ev("Table.PromoteHeaders(Table.Transpose(#table({\"a\",\"b\"}, {{1,2}})))"), "1 | 2".replace("1 | 2", "1 | 2"));
    assert_eq!(ev("Table.Combine({#table({\"a\",\"b\"}, {{1,2}}), #table({\"b\",\"c\"}, {{3,4}})})"), "a,b,c | 1,2,null | null,3,4");
    assert_eq!(ev("Table.Combine({#table({\"a\"}, {{1}}), #table({\"A\"}, {{2}})})"), "a,A | 1,null | null,2");
    assert_eq!(ev("Table.Combine({#table({\"a\",\"b\"}, {{1,2}}), #table({\"b\",\"a\"}, {{3,4}})})"), "a,b | 1,2 | 4,3");
    assert_eq!(ev("Table.Combine({#table({\"a\",\"b\"}, {{1,2}}), #table({\"b\",\"c\"}, {{3,4}})}, {\"c\",\"a\"})"), "c,a | null,1 | 4,null");
    assert_eq!(ev("Table.Combine({})"), "");
    assert_eq!(ev("Table.Combine({#table({\"a\"}, {{1}}), null})"), "ERR Expression.Error");
    let m = "#table({\"v\"}, {{2},{\"b\"},{null},{#date(2026,1,1)},{true},{1},{1.5},{\"a\"}})";
    assert_eq!(ev(&format!("Table.Sort({m}, {{{{\"v\", Order.Ascending}}}})")), "v | null | #date(2026, 1, 1) | 1 | 1.5 | 2 | true | \"a\" | \"b\"");
    assert_eq!(ev(&format!("Table.Sort({m}, {{{{\"v\", Order.Descending}}}})")), "v | \"b\" | \"a\" | true | 2 | 1.5 | 1 | #date(2026, 1, 1) | null");
    assert_eq!(ev("Table.Sort(#table({\"v\"}, {{1}}), {{\"x\", Order.Ascending}})"), "ERR Expression.Error");
    // Types (sections 28 and 29).
    let ty = |src: &str| -> Vec<ast::Ty> {
        let it = Interp::new();
        match it.run(src) {
            Ok(Value::Table(t)) => t.types.iter().map(|x| x.ty).collect(),
            other => panic!("{src}: {other:?}"),
        }
    };
    let tg = "Table.TransformColumnTypes(#table({\"k\",\"v\"}, {{\"x\",1},{\"y\",2}}), {{\"k\", type text},{\"v\", Int64.Type}})";
    assert_eq!(ty(&format!("Table.AddIndexColumn({tg}, \"i\")")), [ast::Ty::Text, ast::Ty::Int64, ast::Ty::Number]);
    assert_eq!(ty(&format!("Table.AddIndexColumn({tg}, \"i\", 0, 1, Int64.Type)")), [ast::Ty::Text, ast::Ty::Int64, ast::Ty::Int64]);
    assert_eq!(ty(&format!("Table.AddColumn({tg}, \"n\", each [v] * 2)")), [ast::Ty::Text, ast::Ty::Int64, ast::Ty::Any]);
    assert_eq!(ty(&format!("Table.AddColumn({tg}, \"n\", each [v] * 2, type number)")), [ast::Ty::Text, ast::Ty::Int64, ast::Ty::Number]);
    assert_eq!(ty(&format!("Table.Group({tg}, {{\"k\"}}, {{{{\"s\", each List.Sum([v]), Int64.Type}}}})")), [ast::Ty::Text, ast::Ty::Int64]);
    assert_eq!(ty("Table.CombineColumns(#table({\"a\",\"b\"}, {{1,2}}), {\"a\",\"b\"}, Combiner.CombineTextByDelimiter(\"-\", QuoteStyle.None), \"ab\")"), [ast::Ty::Text]);
    assert_eq!(ty("Table.SplitColumn(#table({\"a\"}, {{\"x,y\"}}), \"a\", Splitter.SplitTextByDelimiter(\",\"), {\"a1\",\"a2\"})"), [ast::Ty::Text, ast::Ty::Text]);
    let pv = "Table.TransformColumnTypes(#table({\"k\",\"a\",\"v\"}, {{1,\"x\",10},{1,\"y\",20}}), {{\"k\", Int64.Type},{\"a\", type text},{\"v\", Int64.Type}})";
    assert_eq!(ty(&format!("Table.Pivot({pv}, {{\"x\",\"y\"}}, \"a\", \"v\", List.Sum)")), [ast::Ty::Int64, ast::Ty::Int64, ast::Ty::Int64]);
    assert_eq!(ty(&format!("Table.Pivot({pv}, {{\"x\",\"y\"}}, \"a\", \"v\", List.Count)")), [ast::Ty::Int64, ast::Ty::Number, ast::Ty::Number]);
}
