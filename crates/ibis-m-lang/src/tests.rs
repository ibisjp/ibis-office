// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The language tests: the examples of the Power Query M language specification (operators, null, let, records, lists, functions,
//! try/error, laziness) and the limits.
use crate::*;

fn show(it: &Interp, v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Logical(b) => b.to_string(),
        Value::Number(n) => crate::builtin::number_to_text(*n),
        Value::Text(s) => format!("\"{s}\""),
        Value::Gen(_) => show(it, &Value::list(crate::builtin::list_items(it, v).unwrap_or_default())),
        Value::List(items) => format!("{{{}}}", items.iter().map(|l| l.force(it).map(|v| show(it, &v)).unwrap_or_else(|e| format!("!{}", e.message))).collect::<Vec<_>>().join(", ")),
        Value::Record(f) => format!("[{}]", f.iter().map(|(n, l)| format!("{n}={}", l.force(it).map(|v| show(it, &v)).unwrap_or_else(|e| format!("!{}", e.message)))).collect::<Vec<_>>().join(", ")),
        Value::Date(d) => {
            let (y, m, dd) = value::civil_from_days(*d);
            format!("#date({y},{m},{dd})")
        }
        Value::Duration(s) => format!("#duration({s}s)"),
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

fn err(src: &str) -> MError {
    Interp::new().run(src).expect_err(src)
}

#[test]
fn arithmetic_and_precedence() {
    assert_eq!(ev("1 + 2 * 3"), "7");
    assert_eq!(ev("(1 + 2) * 3"), "9");
    assert_eq!(ev("10 / 4"), "2.5");
    assert_eq!(ev("-2 + 5"), "3");
    assert_eq!(ev("1.5e2"), "150");
    assert_eq!(ev("0xff"), "255");
    assert_eq!(ev("1 / 0"), "\u{221e}");
}

#[test]
fn text_and_escapes() {
    assert_eq!(ev(r#""a" & "b""#), "\"ab\"");
    assert_eq!(ev(r#""say ""hi""""#), "\"say \"hi\"\"");
    assert_eq!(ev(r##""a#(lf)b""##), "\"a\nb\"");
    assert_eq!(ev(r##""#(0041)""##), "\"A\"");
    assert_eq!(ev(r#"Text.Upper("abc") & Text.From(12)"#), "\"ABC12\"");
}

#[test]
fn null_semantics() {
    assert_eq!(ev("null + 1"), "null");
    assert_eq!(ev("1 < null"), "null");
    assert_eq!(ev("null = null"), "true");
    assert_eq!(ev("null = 1"), "false");
    assert_eq!(ev("null ?? 2"), "2");
    assert_eq!(ev("1 ?? (error \"never\")"), "1");
    assert_eq!(ev("null ?? null ?? 3"), "3");
    assert_eq!(ev("null and false"), "false");
    assert_eq!(ev("null and true"), "null");
    assert_eq!(ev("null or true"), "true");
    assert_eq!(ev("true or (error \"never\")"), "true");
    assert_eq!(ev("false and (error \"never\")"), "false");
    assert_eq!(ev("not null"), "null");
}

#[test]
fn comparison_and_logic() {
    assert_eq!(ev("1 < 2 and 2 <= 2"), "true");
    assert_eq!(ev("\"a\" < \"b\""), "true");
    assert_eq!(ev("\"abc\" = \"abc\""), "true");
    assert_eq!(ev("1 <> 2"), "true");
    assert_eq!(ev("{1,2} = {1,2}"), "true");
    assert_eq!(ev("[a=1,b=2] = [b=2,a=1]"), "true");
    assert_eq!(ev("true = 1"), "false");
}

#[test]
fn lists() {
    assert_eq!(ev("{1, 2, 3}{1}"), "2");
    assert_eq!(ev("{1..5}"), "{1, 2, 3, 4, 5}");
    assert_eq!(ev("{1, 3..5, 9}"), "{1, 3, 4, 5, 9}");
    assert_eq!(ev("{1,2} & {3}"), "{1, 2, 3}");
    assert_eq!(ev("{1,2}{5}?"), "null");
    assert!(ev("{1,2}{5}").starts_with("ERR"));
    assert_eq!(ev("{}"), "{}");
    assert_eq!(ev("List.Sum({1,2,3})"), "6");
    assert_eq!(ev("List.Sum({})"), "null");
    assert_eq!(ev("List.Transform({1,2,3}, each _ * 2)"), "{2, 4, 6}");
    assert_eq!(ev("List.Select({1,2,3,4}, each Number.From(_) > 2)"), "{3, 4}");
    assert_eq!(ev("List.Max({3,9,2})"), "9");
    assert_eq!(ev("List.Min({3,9,2})"), "2");
    assert_eq!(ev("List.Count(List.Numbers(1, 5, 2))"), "5");
    assert_eq!(ev("List.Accumulate({1,2,3}, 0, (s, x) => s + x)"), "6");
    assert_eq!(ev("List.Generate(() => 1, each _ < 5, each _ + 1)"), "{1, 2, 3, 4}");
}

#[test]
fn records() {
    assert_eq!(ev("[a=1, b=2][b]"), "2");
    assert_eq!(ev("[a=1, b=a+1][b]"), "2");
    assert_eq!(ev("[b=a+1, a=1][b]"), "2");
    assert_eq!(ev("[a=1][c]?"), "null");
    assert!(ev("[a=1][c]").starts_with("ERR"));
    assert_eq!(ev("[a=1] & [b=2]"), "[a=1, b=2]");
    assert_eq!(ev("[a=1, b=2] & [b=3]"), "[a=1, b=3]");
    assert_eq!(ev("[]"), "[]");
    assert_eq!(ev("[#\"my field\"=1][#\"my field\"]"), "1");
    assert_eq!(ev("Record.Field([x=7], \"x\")"), "7");
    assert_eq!(ev("[Sales Amount = 5, Column 1 = 6][Sales Amount] + [Sales Amount = 5, Column 1 = 6][Column 1]"), "11");
    assert_eq!(ev("List.Transform({[a b = 1]}, each [a b])"), "{1}");
    assert_eq!(ev("Record.FieldNames([x=1,y=2])"), "{\"x\", \"y\"}");
    assert_eq!(ev("[a=1,a=2]"), "ERR The field 'a' of the record was defined more than once.");
}

#[test]
fn let_and_functions() {
    assert_eq!(ev("let x = 1, y = x + 1 in y"), "2");
    assert_eq!(ev("let y = x + 1, x = 1 in y"), "2");
    assert_eq!(ev("let f = (x) => x + 1 in f(2)"), "3");
    assert_eq!(ev("let add = (a, b) => a + b, inc = (x) => add(x, 1) in inc(5)"), "6");
    assert_eq!(ev("let f = (x, optional y) => if y = null then x else x + y in {f(1), f(1, 2)}"), "{1, 3}");
    assert_eq!(ev("let a = 10, f = (x) => x + a in f(1)"), "11");
    assert_eq!(ev("let f = (n) => if n <= 1 then 1 else n * @f(n - 1) in f(5)"), "120");
    assert_eq!(ev("let f = (x as number) as number => x * 2 in f(4)"), "8");
    assert!(ev("let f = (x as number) => x in f(\"a\")").starts_with("ERR"));
    assert!(ev("let f = (x) => x in f(1, 2)").starts_with("ERR"));
    assert_eq!(ev("(x) => x + 1"), "function");
    assert_eq!(ev("let r = (1 + 2) in r"), "3");
    assert_eq!(ev("let #\"my name\" = 5 in #\"my name\" + 1"), "6");
    assert_eq!(ev("each _ + 1"), "function");
    assert_eq!(ev("List.Transform({[a=1],[a=2]}, each [a] * 10)"), "{10, 20}");
}

#[test]
fn if_expression() {
    assert_eq!(ev("if 1 < 2 then \"y\" else \"n\""), "\"y\"");
    assert!(ev("if 1 then 1 else 2").starts_with("ERR"));
    assert_eq!(ev("if false then error \"never\" else 3"), "3");
}

#[test]
fn errors_and_try() {
    assert_eq!(ev("try error \"boom\" otherwise 1"), "1");
    assert_eq!(ev("try 5 otherwise 1"), "5");
    assert_eq!(ev("try 5"), "[HasError=false, Value=5]");
    assert_eq!(ev("try error \"boom\""), "[HasError=true, Error=[Reason=\"Expression.Error\", Message=\"boom\", Detail=null]]");
    assert_eq!(ev("try error [Reason=\"R\", Message=\"M\", Detail=3]"), "[HasError=true, Error=[Reason=\"R\", Message=\"M\", Detail=3]]");
    assert_eq!(ev("(try error \"x\")[Error][Message]"), "\"x\"");
    assert_eq!(ev("try 1 + \"a\" otherwise \"bad\""), "\"bad\"");
    assert_eq!(ev("try null[a]? otherwise 9"), "null");
    let e = err("error \"x\"");
    assert_eq!(e.message, "x");
    assert_eq!(e.reason, "Expression.Error");
}

#[test]
fn laziness() {
    // A name that is not used is not evaluated.
    assert_eq!(ev("let a = error \"boom\", b = 2 in b"), "2");
    assert_eq!(ev("[a = error \"boom\", b = 2][b]"), "2");
    assert_eq!(ev("{error \"boom\", 2}{1}"), "2");
    // An unsupported function in an unused branch is not an error.
    assert_eq!(ev("let a = Table.Fuzzy(1), b = 2 in b"), "2");
    // A value is evaluated once.
    assert_eq!(ev("let a = List.Count(List.Numbers(0, 1000)) in a + a"), "2000");
    // Errors surface when used.
    assert!(ev("let a = error \"boom\" in a + 1").starts_with("ERR boom"));
    assert!(ev("let a = a + 1 in a").contains("cyclic"));
}

#[test]
fn is_as_and_types() {
    assert_eq!(ev("1 is number"), "true");
    assert_eq!(ev("\"a\" is number"), "false");
    assert_eq!(ev("null is nullable number"), "true");
    assert_eq!(ev("null is number"), "false");
    assert_eq!(ev("1.5 is number"), "true");
    assert_eq!(ev("{1} is list"), "true");
    assert_eq!(ev("[a=1] is record"), "true");
    assert_eq!(ev("1 as number"), "1");
    assert!(ev("\"a\" as number").starts_with("ERR"));
    assert_eq!(ev("Int64.Type"), "type Int64");
    assert_eq!(ev("type text"), "type Text");
    assert_eq!(ev("type table [a = number, b = text]"), "type Table");
}

#[test]
fn dates_and_durations() {
    assert_eq!(ev("#date(2026, 11, 17)"), "#date(2026,11,17)");
    assert_eq!(ev("#date(2024, 2, 28) + #duration(2, 0, 0, 0)"), "#date(2024,3,1)");
    assert_eq!(ev("#date(2026, 3, 1) - #date(2026, 2, 1)"), "#duration(2419200s)");
    assert_eq!(ev("#date(2026,1,1) < #date(2026,1,2)"), "true");
    assert_eq!(ev("#date(2026,1,1) = #date(2026,1,1)"), "true");
    assert!(ev("#date(2026, 2, 30)").starts_with("ERR"));
    assert_eq!(value::days_from_civil(1, 1, 1), 0);
    assert_eq!(value::civil_from_days(value::days_from_civil(1970, 1, 1)), (1970, 1, 1));
    assert_eq!(value::civil_from_days(value::days_from_civil(2000, 2, 29)), (2000, 2, 29));
}

#[test]
fn tables() {
    let src = "let t = #table({\"a\",\"b\"}, {{1,\"x\"},{2,\"y\"}}) in {t{0}[b], t[a], t{[a=2]}[b], t{[a=9]}?}";
    assert_eq!(ev(src), "{\"x\", {1, 2}, \"y\", null}");
    assert!(ev("#table({\"a\"}, {{1,2}})").starts_with("ERR"));
}

#[test]
fn comments_and_syntax_errors() {
    assert_eq!(ev("// c\nlet /* inner */ x = 1 in x"), "1");
    let e = err("let\n  x = 1,\n  y = (2 +\nin y");
    assert!(matches!(e.kind, ErrKind::Syntax));
    assert_eq!(e.line, 4);
    assert!(matches!(err("1 +").kind, ErrKind::Syntax));
    assert!(matches!(err("\"abc").kind, ErrKind::Syntax));
    assert!(matches!(err("let x = 1 in x x").kind, ErrKind::Syntax));
}

#[test]
fn unsupported_function_names_the_line() {
    let e = err("let\n    a = 1,\n    b = List.Frobnicate(a)\nin\n    b");
    match e.kind {
        ErrKind::Unsupported { name, line } => {
            assert_eq!(name, "List.Frobnicate");
            assert_eq!(line, 3);
        }
        k => panic!("{k:?}"),
    }
    // A name that is not a library name is an ordinary error.
    let e = err("foo + 1");
    assert!(matches!(e.kind, ErrKind::Raised));
    assert!(e.message.contains("foo"));
}

#[test]
fn limits_stop_runaway_evaluation() {
    let e = err("let f = (n) => f(n + 1) in f(0)");
    assert!(matches!(e.kind, ErrKind::Limit));
    let e = err("List.Count(List.Numbers(0, 100000000))");
    assert!(matches!(e.kind, ErrKind::Limit));
    // try does not hide a limit.
    assert!(matches!(err("try (let f = (n) => f(n + 1) in f(0)) otherwise 1").kind, ErrKind::Limit));
    // A deep but finite recursion within the limit works.
    assert_eq!(ev("let f = (n) => if n = 0 then 0 else 1 + f(n - 1) in f(20)"), "20");
}

#[test]
fn implicit_fields_in_conditions() {
    assert_eq!(ev("List.Select({[a=1,b=2],[a=2,b=3]}, each ([a] <> 1) and ([b] <> -6 and [b] <> -4))"), "{[a=2, b=3]}");
    assert_eq!(ev("List.Select({[#\"a-b\"=1]}, each [#\"a-b\"] >= -3 and [#\"a-b\"] <= 3)"), "{[a-b=1]}");
    assert_eq!(ev("List.Transform({[x=1]}, each ([x] <> 140))"), "{true}");
}

#[test]
fn excel_filter_line() {
    let src = "let\n    a = #table({\"Sales Amount\",\"a-b\"},{{1,2}}),\n    b = Table.SelectRows(a, each ([Sales Amount] <> 140) and ([#\"a-b\"] <> -6 and [#\"a-b\"] <> -4)),\n    c = Table.RowCount(b)\nin\n    c";
    assert_eq!(ev(src), "1");
}

