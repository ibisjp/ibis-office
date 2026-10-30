// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What typing into a cell means, as Excel reads it: a formula, a number
//! (with the number format Excel gives it: `1,234` → `#,##0`, `50%` →
//! `0%`, `2026/9/30` → `yyyy/m/d`…), a logical value, an error, or text.

use crate::eval::convert::{parse_date_time, parse_number_text};
use crate::value::{CellError, CellValue};

/// Typed text read.
#[derive(Clone, Debug, PartialEq)]
pub enum Typed {
    /// A formula, without the leading `=`.
    Formula(String),
    /// A value, and the number format Excel gives a cell of General that
    /// it is typed into.
    Value(CellValue, Option<&'static str>),
}

/// Read what was typed into a cell.
pub fn parse_input(text: &str, date1904: bool) -> Typed {
    if let Some(f) = text.strip_prefix('=')
        && !f.is_empty()
    {
        return Typed::Formula(f.to_string());
    }
    if let Some(t) = text.strip_prefix('\'') {
        return Typed::Value(CellValue::Text(t.to_string()), None);
    }
    let t = text.trim();
    if t.is_empty() {
        return Typed::Value(CellValue::Text(text.to_string()), None);
    }
    if t.eq_ignore_ascii_case("TRUE") {
        return Typed::Value(CellValue::Bool(true), None);
    }
    if t.eq_ignore_ascii_case("FALSE") {
        return Typed::Value(CellValue::Bool(false), None);
    }
    if let Some(e) = CellError::parse(t) {
        return Typed::Value(CellValue::Error(e), None);
    }
    // Plain numbers and their decorations.
    let plain = t.trim_start_matches(['+', '-', '(']);
    if plain.starts_with(|c: char| c.is_ascii_digit() || c == '.' || c == '¥' || c == '￥' || c == '$')
        && !t.contains(['/', ':', '年', '月', '日'])
        && let Some(n) = parse_number_text(t, date1904)
    {
        let format = if t.ends_with('%') {
            Some(if t.contains('.') { "0.00%" } else { "0%" })
        } else if t.contains(['¥', '￥']) {
            Some(if t.contains('.') { "\"¥\"#,##0.00;\"¥\"\\-#,##0.00" } else { "\"¥\"#,##0;\"¥\"\\-#,##0" })
        } else if t.contains('$') {
            Some(if t.contains('.') { "\"$\"#,##0.00_);\\(\"$\"#,##0.00\\)" } else { "\"$\"#,##0_);\\(\"$\"#,##0\\)" })
        } else if t.contains(['e', 'E']) {
            Some("0.00E+00")
        } else if t.contains(',') {
            Some(if t.contains('.') { "#,##0.00" } else { "#,##0" })
        } else {
            None
        };
        return Typed::Value(CellValue::Number(n), format);
    }
    // Dates and times.
    if let Some(n) = parse_date_time(t, date1904) {
        let has_time = t.contains(':') || t.to_ascii_uppercase().ends_with('M');
        let has_date = n.abs() >= 1.0 || t.contains(['/', '年', '月']);
        let parts = t.split(['/', '-']).filter(|p| !p.is_empty()).count();
        let format = match (has_date, has_time) {
            (true, true) => "yyyy/m/d h:mm",
            (false, true) if t.to_ascii_uppercase().contains("AM") || t.to_ascii_uppercase().contains("PM") => {
                if t.matches(':').count() >= 2 { "h:mm:ss AM/PM" } else { "h:mm AM/PM" }
            }
            (false, true) => {
                if n < 0.0 {
                    "[h]:mm"
                } else if t.matches(':').count() >= 2 {
                    "h:mm:ss"
                } else {
                    "h:mm"
                }
            }
            _ if t.contains('年') && t.contains('日') => "yyyy\"年\"m\"月\"d\"日\"",
            _ if t.contains('年') => "yyyy\"年\"m\"月\"",
            _ if t.contains('月') => "m\"月\"d\"日\"",
            // 9/30: this year's; the year is not shown.
            _ if parts == 2 && t.split(['/', '-']).next().is_some_and(|p| p.len() <= 2) => "m\"月\"d\"日\"",
            _ if parts == 2 => "yyyy/m",
            _ => "yyyy/m/d",
        };
        return Typed::Value(CellValue::Number(n), Some(format));
    }
    // Signed literals have already been read as numbers/dates above. Remaining
    // leading signs introduce an expression, while apostrophe-escaped text was
    // handled before numeric parsing. Preserve the sign as a unary operator.
    if text.starts_with(['+', '-']) && text.len()>1 {
        return Typed::Formula(text.to_string());
    }
    Typed::Value(CellValue::Text(text.to_string()), None)
}

/// The order of the year, month and day in a date column of a text file
/// (the wizard's 日付: 年月日 / 月日年 / 日月年 …).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateOrder {
    Ymd,
    Mdy,
    Dmy,
    Dym,
    Myd,
    Ydm,
}

impl DateOrder {
    pub const ALL: [DateOrder; 6] = [Self::Ymd, Self::Mdy, Self::Dmy, Self::Dym, Self::Myd, Self::Ydm];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ymd => "YMD",
            Self::Mdy => "MDY",
            Self::Dmy => "DMY",
            Self::Dym => "DYM",
            Self::Myd => "MYD",
            Self::Ydm => "YDM",
        }
    }
}

/// A date written in a fixed order (`3/4/2026`, `26-04-03`, `20260403`),
/// as a serial number; `None` when it is not a date in that order. A year
/// of two digits is 2000-2029 or 1930-1999.
pub fn parse_date_in_order(text: &str, order: DateOrder, date1904: bool) -> Option<f64> {
    let t = text.trim();
    let digits: Vec<i64>;
    let mut widths = Vec::new();
    if t.len() >= 6 && t.chars().all(|c| c.is_ascii_digit()) {
        // No separators: the year is four digits (8 in all) or two (6 in all).
        let year_width = match t.len() {
            8 => 4,
            6 => 2,
            _ => return None,
        };
        let cut = |start: usize, w: usize| t[start..start + w].parse::<i64>().ok();
        let (a, b, c);
        let (wa, wb, wc) = match order {
            DateOrder::Ymd | DateOrder::Ydm => (year_width, 2, 2),
            DateOrder::Mdy | DateOrder::Dmy => (2, 2, year_width),
            DateOrder::Dym | DateOrder::Myd => (2, year_width, 2),
        };
        a = cut(0, wa)?;
        b = cut(wa, wb)?;
        c = cut(wa + wb, wc)?;
        digits = vec![a, b, c];
        widths.extend([wa, wb, wc]);
    } else {
        let parts: Vec<&str> = t.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).collect();
        if parts.len() != 3 || t.chars().any(|c| !(c.is_ascii_digit() || matches!(c, '/' | '-' | '.' | ' '))) {
            return None;
        }
        digits = parts.iter().map(|p| p.parse::<i64>().ok()).collect::<Option<_>>()?;
        widths.extend(parts.iter().map(|p| p.len()));
    }
    let (y, m, d, year_width) = match order {
        DateOrder::Ymd => (digits[0], digits[1], digits[2], widths[0]),
        DateOrder::Ydm => (digits[0], digits[2], digits[1], widths[0]),
        DateOrder::Mdy => (digits[2], digits[0], digits[1], widths[2]),
        DateOrder::Dmy => (digits[2], digits[1], digits[0], widths[2]),
        DateOrder::Myd => (digits[1], digits[0], digits[2], widths[1]),
        DateOrder::Dym => (digits[1], digits[2], digits[0], widths[1]),
    };
    let y = if year_width <= 2 { if y < 30 { 2000 + y } else { 1900 + y } } else { y };
    if !(1..=12).contains(&m) || d < 1 || y < 1900 || y > 9999 {
        return None;
    }
    let days = [31, if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if d > days[(m - 1) as usize] {
        return None;
    }
    Some(crate::eval::convert::serial_from_date(y, m, d, date1904) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(t: &str) -> (CellValue, Option<&'static str>) {
        match parse_input(t, false) {
            Typed::Value(v, f) => (v, f),
            Typed::Formula(f) => panic!("formula {f}"),
        }
    }

    #[test]
    fn typing() {
        assert_eq!(parse_input("=SUM(A1:A3)", false), Typed::Formula("SUM(A1:A3)".into()));
        assert_eq!(value("123"), (CellValue::Number(123.0), None));
        assert_eq!(value("1,234"), (CellValue::Number(1234.0), Some("#,##0")));
        assert_eq!(value("50%"), (CellValue::Number(0.5), Some("0%")));
        assert_eq!(value("¥1,000"), (CellValue::Number(1000.0), Some("\"¥\"#,##0;\"¥\"\\-#,##0")));
        assert_eq!(value("2026/9/30"), (CellValue::Number(46295.0), Some("yyyy/m/d")));
        assert_eq!(value("12:30"), (CellValue::Number(0.5208333333333334), Some("h:mm")));
        assert_eq!(value("true"), (CellValue::Bool(true), None));
        assert_eq!(value("'0123"), (CellValue::Text("0123".into()), None));
        assert_eq!(value("abc"), (CellValue::Text("abc".into()), None));
        assert_eq!(value("#N/A"), (CellValue::Error(CellError::NA), None));
        assert_eq!(value("1e3"), (CellValue::Number(1000.0), Some("0.00E+00")));
        assert_eq!(value("1234567890123456"), (CellValue::Number(1234567890123450.0), None));
        assert_eq!(value("'1234567890123456"), (CellValue::Text("1234567890123456".into()), None));
        assert_eq!(value("1234567890123456789"), (CellValue::Number(1234567890123450000.0), None));
        assert_eq!(parse_input("-12:00", true), Typed::Value(CellValue::Number(-0.5), Some("[h]:mm")));
        assert_eq!(parse_input("-1904/01/02", true), Typed::Value(CellValue::Number(-1.0), Some("yyyy/m/d")));
    }

    #[test]
    fn signed_formula_entry_preserves_typed_literals() {
        for text in ["+A1", "-A1", "+SUM(A1:A3)", "-2^2", "+1+2"] {
            assert_eq!(parse_input(text,false),Typed::Formula(text.to_string()));
        }
        assert_eq!(value("-123"),(CellValue::Number(-123.),None));
        assert_eq!(value("+123"),(CellValue::Number(123.),None));
        assert_eq!(value("-50%"),(CellValue::Number(-0.5),Some("0%")));
        assert_eq!(value("'+A1"),(CellValue::Text("+A1".into()),None));
    }

    #[test]
    fn dates_in_a_chosen_order() {
        let d = |t: &str, o| parse_date_in_order(t, o, false);
        assert_eq!(d("2026/4/3", DateOrder::Ymd), Some(46115.0));
        assert_eq!(d("4/3/2026", DateOrder::Mdy), Some(46115.0));
        assert_eq!(d("3/4/2026", DateOrder::Dmy), Some(46115.0));
        assert_eq!(d("03.04.26", DateOrder::Dmy), Some(46115.0));
        assert_eq!(d("20260403", DateOrder::Ymd), Some(46115.0));
        assert_eq!(d("040326", DateOrder::Mdy), Some(46115.0));
        assert_eq!(d("26-04-03", DateOrder::Ymd), Some(46115.0));
        assert_eq!(d("3/2026/4", DateOrder::Dym), Some(46115.0));
        assert_eq!(d("13/1/2026", DateOrder::Mdy), None);
        assert_eq!(d("31/2/2026", DateOrder::Dmy), None);
        assert_eq!(d("abc", DateOrder::Ymd), None);
        assert_eq!(d("2026/4", DateOrder::Ymd), None);
        assert_eq!(d("1/2/99", DateOrder::Mdy), Some(36162.0));
    }
}
