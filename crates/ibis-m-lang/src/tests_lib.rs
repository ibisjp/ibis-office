// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Stage 3 (the part that does not wait for Excel): `Text.*`, `List.*`, the calendar functions of `Date.*`, the splitters. The splitter
//! values are what Excel gave (`tasks/doc/requests/2026-11-16-k0039-split-semantics-result.md`, section A); the others are what the
//! Microsoft Learn reference says for each function (the example in its page).
use crate::*;

fn show(it: &Interp, v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Logical(b) => b.to_string(),
        Value::Number(n) => crate::builtin::number_to_text(*n),
        Value::Text(s) => format!("\"{s}\""),
        Value::Gen(_) => show(it, &Value::list(crate::builtin::list_items(it, v).unwrap_or_default())),
        Value::List(items) => format!("{{{}}}", items.iter().map(|l| l.force(it).map(|v| show(it, &v)).unwrap_or_else(|e| format!("!{}", e.message))).collect::<Vec<_>>().join(", ")),
        Value::Date(d) => {
            let (y, m, dd) = value::civil_from_days(*d);
            format!("#date({y}, {m}, {dd})")
        }
        Value::DateTime(d, s) => {
            let (y, m, dd) = value::civil_from_days(*d);
            format!("#datetime({y}, {m}, {dd}, {}, {}, {})", (s / 3600.0).floor(), (s % 3600.0 / 60.0).floor(), s % 60.0)
        }
        v => format!("{v:?}"),
    }
}

fn ev(src: &str) -> String {
    let it = Interp::new();
    match it.run(src) {
        Ok(v) => show(&it, &v),
        Err(e) => format!("ERR {}", e.message),
    }
}

#[test]
fn splitters_give_what_excel_gives() {
    assert_eq!(ev("Splitter.SplitTextByLengths({2, 3})(\"abcdefgh\")"), "{\"ab\", \"cde\"}");
    assert_eq!(ev("Splitter.SplitTextByLengths({2, 3}, true)(\"abcdefgh\")"), "{\"def\", \"gh\"}");
    assert_eq!(ev("Splitter.SplitTextByPositions({2, 5})(\"abcdefgh\")"), "{\"cde\", \"fgh\"}");
    assert_eq!(ev("Splitter.SplitTextByPositions({0, 5}, true)(\"AppleRed-L-1\")"), "{\"AppleRe\", \"d-L-1\"}");
    assert_eq!(ev("Splitter.SplitTextByPositions({0, 5}, false)(\"AppleRed-L-1\")"), "{\"Apple\", \"Red-L-1\"}");
    assert_eq!(ev("Splitter.SplitTextByCharacterTransition({\"a\"..\"z\"}, {\"A\"..\"Z\"})(\"AppleRed-L-1\")"), "{\"Apple\", \"Red-L-1\"}");
    assert_eq!(ev("Splitter.SplitTextByCharacterTransition({\"A\"..\"Z\"}, {\"a\"..\"z\"})(\"AppleRed-L-1\")"), "{\"A\", \"ppleR\", \"ed-L-1\"}");
    assert_eq!(ev("Splitter.SplitTextByCharacterTransition({\"0\"..\"9\"}, (c) => not List.Contains({\"0\"..\"9\"}, c))(\"AppleRed-L-1\")"), "{\"AppleRed-L-1\"}");
    assert_eq!(ev("Splitter.SplitTextByCharacterTransition((c) => not List.Contains({\"0\"..\"9\"}, c), {\"0\"..\"9\"})(\"AppleRed-L-1\")"), "{\"AppleRed-L-\", \"1\"}");
    assert_eq!(ev("Splitter.SplitTextByRepeatedLengths(3)(\"abcdefgh\")"), "{\"abc\", \"def\", \"gh\"}");
    assert_eq!(ev("Splitter.SplitTextByEachDelimiter({\"-\", \"-\"})(\"a-b-c-d\")"), "{\"a\", \"b\", \"c-d\"}");
    assert_eq!(ev("Splitter.SplitTextByEachDelimiter({\"-\", \"-\"}, QuoteStyle.Csv, true)(\"a-b-c-d\")"), "{\"a-b\", \"c\", \"d\"}");
    assert_eq!(ev("Splitter.SplitTextByAnyDelimiter({\",\", \";\"})(\"a,b;c\")"), "{\"a\", \"b\", \"c\"}");
}

#[test]
fn text_functions() {
    assert_eq!(ev("Text.Contains(\"Hello World\", \"World\")"), "true");
    assert_eq!(ev("Text.Contains(\"Hello World\", \"world\")"), "false");
    assert_eq!(ev("Text.Contains(\"Hello World\", \"world\", Comparer.OrdinalIgnoreCase)"), "true");
    assert_eq!(ev("Text.StartsWith(\"Hello\", \"He\")"), "true");
    assert_eq!(ev("Text.EndsWith(\"Hello\", \"lo\")"), "true");
    assert_eq!(ev("Text.Replace(\"the quick brown fox\", \"the\", \"a\")"), "\"a quick brown fox\"");
    assert_eq!(ev("Text.Remove(\"a,b;c\", {\",\", \";\"})"), "\"abc\"");
    assert_eq!(ev("Text.Remove(\"a,b;c\", \",;\")"), "\"abc\"");
    assert_eq!(ev("Text.Select(\"a1b2c3\", {\"0\"..\"9\"})"), "\"123\"");
    assert_eq!(ev("Text.Combine({\"a\", null, \"b\"}, \"-\")"), "\"a-b\"");
    assert_eq!(ev("Text.Split(\"a,b,,c\", \",\")"), "{\"a\", \"b\", \"\", \"c\"}");
    assert_eq!(ev("Text.SplitAny(\"a,b;c\", \",;\")"), "{\"a\", \"b\", \"c\"}");
    assert_eq!(ev("Text.Range(\"Hello World\", 6)"), "\"World\"");
    assert_eq!(ev("Text.Range(\"Hello World\", 6, 3)"), "\"Wor\"");
    assert!(ev("Text.Range(\"abc\", 1, 5)").starts_with("ERR"));
    assert_eq!(ev("Text.Start(\"Hello\", 2)"), "\"He\"");
    assert_eq!(ev("Text.End(\"Hello\", 2)"), "\"lo\"");
    assert_eq!(ev("Text.Middle(\"Hello World\", 6, 5)"), "\"World\"");
    assert_eq!(ev("Text.Middle(\"abc\", 1, 10)"), "\"bc\"");
    assert_eq!(ev("Text.PadStart(\"5\", 3, \"0\")"), "\"005\"");
    assert_eq!(ev("Text.PadEnd(\"5\", 3, \"0\")"), "\"500\"");
    assert_eq!(ev("Text.PadStart(\"abcd\", 3)"), "\"abcd\"");
    assert_eq!(ev("Text.Reverse(\"abc\")"), "\"cba\"");
    assert_eq!(ev("Text.ToList(\"ab\")"), "{\"a\", \"b\"}");
    assert_eq!(ev("Text.Repeat(\"ab\", 3)"), "\"ababab\"");
    assert_eq!(ev("Text.Insert(\"abc\", 1, \"X\")"), "\"aXbc\"");
    assert_eq!(ev("Text.At(\"abc\", 2)"), "\"c\"");
    assert_eq!(ev("Text.Trim(\"  a b  \")"), "\"a b\"");
    assert_eq!(ev("Text.Trim(\"xxaxx\", \"x\")"), "\"a\"");
    assert_eq!(ev("Text.TrimStart(\"  a \")"), "\"a \"");
    assert_eq!(ev("Text.TrimEnd(\"  a \")"), "\"  a\"");
    assert_eq!(ev("Text.PositionOf(\"Hello World\", \"o\")"), "4");
    assert_eq!(ev("Text.PositionOf(\"Hello World\", \"o\", Occurrence.Last)"), "7");
    assert_eq!(ev("Text.PositionOf(\"Hello World\", \"o\", Occurrence.All)"), "{4, 7}");
    assert_eq!(ev("Text.PositionOf(\"Hello\", \"z\")"), "-1");
    assert_eq!(ev("Text.AfterDelimiter(\"a-b-c\", \"-\")"), "\"b-c\"");
    assert_eq!(ev("Text.AfterDelimiter(\"a-b-c\", \"-\", 1)"), "\"c\"");
    assert_eq!(ev("Text.AfterDelimiter(\"a-b-c\", \"-\", {0, RelativePosition.FromEnd})"), "\"c\"");
    assert_eq!(ev("Text.BeforeDelimiter(\"a-b-c\", \"-\")"), "\"a\"");
    assert_eq!(ev("Text.BeforeDelimiter(\"a-b-c\", \"-\", {0, RelativePosition.FromEnd})"), "\"a-b\"");
    assert_eq!(ev("Text.BetweenDelimiters(\"a(b)c\", \"(\", \")\")"), "\"b\"");
    assert_eq!(ev("Text.Upper(null)"), "null");
    assert_eq!(ev("Text.Length(\"日本語\")"), "3");
}

#[test]
fn list_functions() {
    assert_eq!(ev("List.Sort({3, 1, 2})"), "{1, 2, 3}");
    assert_eq!(ev("List.Sort({3, 1, 2}, Order.Descending)"), "{3, 2, 1}");
    assert_eq!(ev("List.Median({1, 2, 3, 4})"), "2.5");
    assert_eq!(ev("List.Median({5, 3, 1})"), "3");
    assert_eq!(ev("List.StandardDeviation({1, 2, 3, 4})"), "1.2909944487358056");
    assert_eq!(ev("List.Product({2, 3, 4})"), "24");
    assert_eq!(ev("List.NonNullCount({null, 1, 2})"), "2");
    assert_eq!(ev("List.FirstN({1, 2, 3, 4}, 2)"), "{1, 2}");
    assert_eq!(ev("List.FirstN({1, 2, 3, 1}, each _ < 3)"), "{1, 2}");
    assert_eq!(ev("List.LastN({1, 2, 3, 4}, 2)"), "{3, 4}");
    assert_eq!(ev("List.Skip({1, 2, 3, 4}, 3)"), "{4}");
    assert_eq!(ev("List.RemoveFirstN({1, 2, 3})"), "{2, 3}");
    assert_eq!(ev("List.RemoveLastN({1, 2, 3}, 2)"), "{1}");
    assert_eq!(ev("List.Range({1, 2, 3, 4, 5}, 1, 3)"), "{2, 3, 4}");
    assert!(ev("List.Range({1, 2}, 1, 5)").starts_with("ERR"));
    assert_eq!(ev("List.Union({{1, 2}, {2, 3}})"), "{1, 2, 3}");
    assert_eq!(ev("List.Intersect({{1, 2, 3}, {2, 3, 4}})"), "{2, 3}");
    assert_eq!(ev("List.Zip({{1, 2}, {\"a\", \"b\"}})"), "{{1, \"a\"}, {2, \"b\"}}");
    assert_eq!(ev("List.PositionOf({\"a\", \"b\", \"a\"}, \"a\")"), "0");
    assert_eq!(ev("List.PositionOf({\"a\", \"b\", \"a\"}, \"a\", Occurrence.All)"), "{0, 2}");
    assert_eq!(ev("List.PositionOf({1}, 5)"), "-1");
    assert_eq!(ev("List.Positions({\"a\", \"b\"})"), "{0, 1}");
    assert_eq!(ev("List.ReplaceValue({1, 2, 1}, 1, 9, Replacer.ReplaceValue)"), "{9, 2, 9}");
    assert_eq!(ev("List.IsEmpty({})"), "true");
    assert_eq!(ev("List.AllTrue({true, false})"), "false");
    assert_eq!(ev("List.AnyTrue({true, false})"), "true");
    assert_eq!(ev("List.MatchesAll({2, 4}, each Number.IsEven(_))"), "true");
    assert_eq!(ev("List.MatchesAny({1, 4}, each Number.IsEven(_))"), "true");
    assert_eq!(ev("List.Single({7})"), "7");
    assert!(ev("List.Single({1, 2})").starts_with("ERR"));
    assert_eq!(ev("List.SingleOrDefault({}, 5)"), "5");
    assert_eq!(ev("List.Repeat({1, 2}, 2)"), "{1, 2, 1, 2}");
    assert_eq!(ev("List.ContainsAny({1, 2}, {5, 2})"), "true");
    assert_eq!(ev("List.ContainsAll({1, 2}, {1, 5})"), "false");
    assert_eq!(ev("List.RemoveItems({1, 2, 3}, {2})"), "{1, 3}");
    assert_eq!(ev("List.Dates(#date(2026, 1, 30), 3, #duration(1, 0, 0, 0))"), "{#date(2026, 1, 30), #date(2026, 1, 31), #date(2026, 2, 1)}");
    assert_eq!(ev("List.Reverse({1, 2})"), "{2, 1}");
    assert_eq!(ev("List.Average({1, 2, null})"), "1.5");
}

#[test]
fn calendar_functions() {
    assert_eq!(ev("Date.Year(#date(2026, 11, 17))"), "2026");
    assert_eq!(ev("Date.Month(#date(2026, 11, 17))"), "11");
    assert_eq!(ev("Date.Day(#date(2026, 11, 17))"), "17");
    assert_eq!(ev("Date.Year(#datetime(2026, 11, 17, 13, 5, 9))"), "2026");
    assert_eq!(ev("Date.Day(null)"), "null");
    assert!(ev("Date.Year(\"2026-11-17\")").starts_with("ERR"));
    assert_eq!(ev("Date.QuarterOfYear(#date(2026, 11, 17))"), "4");
    assert_eq!(ev("Date.DayOfYear(#date(2024, 12, 31))"), "366");
    assert_eq!(ev("Date.DaysInMonth(#date(2100, 2, 1))"), "28");
    assert_eq!(ev("Date.DaysInMonth(#date(2024, 2, 1))"), "29");
    assert_eq!(ev("Date.IsLeapYear(#date(2000, 1, 1))"), "true");
    assert_eq!(ev("Date.IsLeapYear(#date(1900, 1, 1))"), "false");
    assert_eq!(ev("Date.AddDays(#date(2026, 12, 30), 3)"), "#date(2027, 1, 2)");
    assert_eq!(ev("Date.AddWeeks(#date(2026, 1, 1), 2)"), "#date(2026, 1, 15)");
    assert_eq!(ev("Date.AddMonths(#date(2026, 1, 31), 1)"), "#date(2026, 2, 28)");
    assert_eq!(ev("Date.AddMonths(#date(2026, 11, 17), -11)"), "#date(2025, 12, 17)");
    assert_eq!(ev("Date.AddQuarters(#date(2026, 11, 17), 1)"), "#date(2027, 2, 17)");
    assert_eq!(ev("Date.AddYears(#date(2024, 2, 29), 1)"), "#date(2025, 2, 28)");
    assert_eq!(ev("Date.AddDays(#datetime(2026, 1, 1, 13, 5, 9), 1)"), "#datetime(2026, 1, 2, 13, 5, 9)");
    assert_eq!(ev("Date.StartOfMonth(#date(2026, 11, 17))"), "#date(2026, 11, 1)");
    assert_eq!(ev("Date.EndOfMonth(#date(2024, 2, 10))"), "#date(2024, 2, 29)");
    assert_eq!(ev("Date.StartOfQuarter(#date(2026, 11, 17))"), "#date(2026, 10, 1)");
    assert_eq!(ev("Date.EndOfQuarter(#date(2026, 11, 17))"), "#date(2026, 12, 31)");
    assert_eq!(ev("Date.StartOfYear(#date(2026, 11, 17))"), "#date(2026, 1, 1)");
    assert_eq!(ev("Date.EndOfYear(#date(2026, 11, 17))"), "#date(2026, 12, 31)");
    assert_eq!(ev("Date.StartOfDay(#datetime(2026, 11, 17, 13, 5, 9))"), "#datetime(2026, 11, 17, 0, 0, 0)");
    assert_eq!(ev("DateTime.Date(#datetime(2026, 11, 17, 13, 5, 9))"), "#date(2026, 11, 17)");
    assert_eq!(ev("Time.Hour(#time(13, 5, 9))"), "13");
    assert_eq!(ev("Time.Minute(#datetime(2026, 1, 1, 13, 5, 9))"), "5");
    assert_eq!(ev("Time.Second(#time(13, 5, 9))"), "9");
    assert_eq!(ev("Duration.Days(#duration(1, 12, 0, 0))"), "1");
    assert_eq!(ev("Duration.Hours(#duration(1, 12, 0, 0))"), "12");
    assert_eq!(ev("Duration.TotalDays(#duration(1, 12, 0, 0))"), "1.5");
    assert_eq!(ev("Duration.TotalHours(#duration(0, 2, 30, 0))"), "2.5");
    assert_eq!(ev("Date.From(46343)"), "#date(2026, 11, 17)".replace("2026, 11, 17", "2026, 11, 17"));
    assert_eq!(ev("Number.Power(2, 10)"), "1024");
    assert_eq!(ev("Number.Sign(-3)"), "-1");
    assert_eq!(ev("Number.IsEven(4)"), "true");
    assert_eq!(ev("Number.IsOdd(4)"), "false");
}
