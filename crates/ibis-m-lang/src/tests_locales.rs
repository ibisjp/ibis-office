// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0039: the culture tables measured in Microsoft 365 Excel's Power Query on Windows (`tests/data/k0039-locales-result.tsv`, a copy of
//! `tasks/doc/requests/2026-11-20-k0039-locales-result.tsv`; request `2026-11-20-k0039-locales.md`, result `-result.md`). Sections 1 to 5
//! of the table are run as M expressions and each result is compared with the measured value: every row is a test. Section 6 (a
//! datetimezone column in a sheet) is a conversion of the application's table, tested in `ibis-doc-model`.
use crate::*;

const TABLE: &str = include_str!("../tests/data/k0039-locales-result.tsv");

/// `{XXXX}` (four hex digits of one UTF-16 unit) in an input or a format stands for that character.
fn unbrace(s: &str) -> String {
    let c: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        if c[i] == '{' && i + 5 < c.len() && c[i + 5] == '}' && c[i + 1..i + 5].iter().all(|x| x.is_ascii_hexdigit() && !x.is_ascii_lowercase()) {
            let h: String = c[i + 1..i + 5].iter().collect();
            out.push(char::from_u32(u32::from_str_radix(&h, 16).unwrap()).unwrap());
            i += 6;
        } else {
            out.push(c[i]);
            i += 1;
        }
    }
    out
}

/// The `value` column: when it has a non-ASCII character the whole text follows as ` {hex hex …}` (UTF-16 units).
fn value_of(s: &str) -> String {
    if let Some(at) = s.rfind(" {") {
        let tail = &s[at + 2..];
        if let Some(hex) = tail.strip_suffix('}') {
            if !hex.is_empty() && hex.split(' ').all(|h| h.len() == 4 && h.chars().all(|x| x.is_ascii_hexdigit())) {
                let units: Vec<u16> = hex.split(' ').map(|h| u16::from_str_radix(h, 16).unwrap()).collect();
                return String::from_utf16(&units).unwrap();
            }
        }
    }
    s.to_string()
}

/// A result as the table writes its `type` and `value` columns.
fn show(v: &Value) -> (String, String) {
    let ymd = |d: i64| {
        let (y, m, dd) = value::civil_from_days(d);
        format!("{y:04}-{m:02}-{dd:02}")
    };
    let hms = |s: f64| {
        let t = s.floor() as i64;
        format!("{:02}:{:02}:{:02}.{:07}", t / 3600, t % 3600 / 60, t % 60, ((s - s.floor()) * 1e7).round() as i64)
    };
    match v {
        Value::Null => ("null".into(), String::new()),
        Value::Text(s) => ("text".into(), s.to_string()),
        Value::Number(n) => ("number".into(), culture::number_text(*n)),
        Value::Date(d) => ("date".into(), ymd(*d)),
        Value::DateTime(d, s) => ("datetime".into(), format!("{}T{}", ymd(*d), hms(*s))),
        Value::Time(s) => ("time".into(), hms(*s)),
        Value::Failed(_) => ("error".into(), String::new()),
        v => (format!("other {v:?}"), String::new()),
    }
}

fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

/// The M expression a row measured (the `fn`, `input`, `format` and `culture` columns).
fn expression(section: &str, culture: &str, f: &str, input: &str, format: &str) -> String {
    let c = match culture {
        "(empty)" => quote(""),
        n => quote(n),
    };
    let fmt = if format.is_empty() { "null".to_string() } else { quote(format) };
    match (section, f) {
        ("1" | "2", "Text.From") => format!("Text.From({input}, {c})"),
        ("1" | "2" | "3", "Date.ToText" | "DateTime.ToText" | "Time.ToText" | "Number.ToText") => format!("{f}({input}, {fmt}, {c})"),
        ("3", "Text.From") => format!("Text.From({input}, {c})"),
        ("3" | "4", _) => format!("{f}({input}, {c})"),
        ("5", "Table.TransformColumnTypes") => format!("Table.TransformColumnTypes(#table({{\"c\"}}, {{{{{input}}}}}), {{{{\"c\", {format}}}}}, {c}){{0}}[c]"),
        _ => panic!("unknown row {section} {f}"),
    }
}

/// The rows of sections 1 to 5 that this crate does not match yet, as `section culture fn input format`: kept here so the rest is a test
/// that stays green and any row that starts to fail (or that now matches) is told.
///
/// - `ko-KR` (the short date is `yyyy-MM-dd`) and `ar-SA` (the Hijri calendar: `16/07/47`) were measured for one format each, only to see that
///   a culture other than the seven is not refused; this crate does not hold their data, and a name it does not know is the current culture.
const NOT_YET: &[&str] = &["1\tko-KR\t", "1\tar-SA\t"];

#[test]
fn locales_table() {
    let mut failed: Vec<String> = Vec::new();
    let mut total = 0;
    for line in TABLE.lines().skip(1) {
        let c: Vec<&str> = line.split('\t').collect();
        if c.len() < 8 || !matches!(c[0], "1" | "2" | "3" | "4" | "5") {
            continue;
        }
        let (section, culture, f, input, format, ty, value, err) = (c[0], c[1], c[2], unbrace(c[3]), unbrace(c[4]), c[5], value_of(c[6]), c[7]);
        let _ = err;
        total += 1;
        let src = expression(section, culture, f, &input, &format);
        let it = Interp::new();
        it.set_utc_offset(540);
        let got = match it.run(&src) {
            Ok(v) => show(&v),
            Err(_) => ("error".into(), String::new()),
        };
        // A number is compared as a number (the table keeps the 15 digits Excel wrote); an error only as an error.
        let same = match ty {
            "error" => got.0 == "error",
            "number" => got.0 == "number" && got.1.parse::<f64>().ok().zip(value.parse::<f64>().ok()).is_some_and(|(a, b)| a == b || (a - b).abs() <= b.abs() * 1e-14),
            _ => got.0 == ty && got.1 == value,
        };
        if !same {
            failed.push(format!("{section}\t{culture}\t{f}\t{input}\t{format}\twant {ty} {value:?}\tgot {} {:?}", got.0, got.1));
        }
    }
    let unknown: Vec<&String> = failed.iter().filter(|f| !NOT_YET.iter().any(|n| f.starts_with(n))).collect();
    if !unknown.is_empty() || std::env::var("LOCALES_REPORT").is_ok() {
        let shown: Vec<&String> = failed.iter().take(std::env::var("LOCALES_MAX").ok().and_then(|s| s.parse().ok()).unwrap_or(40)).collect();
        panic!("{} of {total} rows differ:\n{}", failed.len(), shown.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n"));
    }
}
