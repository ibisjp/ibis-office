// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The parts of the steps that Power Query's longer forms need: a row condition of several comparisons, windows of dates, what to
//! do with a column that is not there (`MissingField`), the functions `Table.TransformColumns` applies, and the splitting of a text
//! by the splitters of M.
use ibis_calc::eval::convert::{civil_from_days, days_from_civil};
use ibis_calc::CellValue;

use super::{matches_op, Op, SplitBy, Step};

/// A part of a date a condition compares (`Date.Year([d]) = 2024`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatePart {
    Year,
    Quarter,
    Month,
    Day,
}

impl DatePart {
    pub const ALL: [DatePart; 4] = [Self::Year, Self::Quarter, Self::Month, Self::Day];

    pub fn m_name(self) -> &'static str {
        match self {
            Self::Year => "Date.Year",
            Self::Quarter => "Date.QuarterOfYear",
            Self::Month => "Date.Month",
            Self::Day => "Date.Day",
        }
    }
}

/// Where a window of dates lies from today.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateWhen {
    /// The unit that holds today (`Date.IsInCurrentMonth`).
    Current,
    /// The unit before, or `count` units before (`Date.IsInPreviousMonth`, `Date.IsInPreviousNMonths`); today's unit is not in it.
    Previous,
    /// The unit after, or `count` units after.
    Next,
    /// From the start of the year to today (`Date.IsInYearToDate`).
    YearToDate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateUnit {
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

impl DateUnit {
    pub const ALL: [DateUnit; 5] = [Self::Day, Self::Week, Self::Month, Self::Quarter, Self::Year];

    /// The name in M's function names (`Month` in `Date.IsInCurrentMonth`).
    pub fn m_name(self) -> &'static str {
        match self {
            Self::Day => "Day",
            Self::Week => "Week",
            Self::Month => "Month",
            Self::Quarter => "Quarter",
            Self::Year => "Year",
        }
    }

    /// The number of this unit a date (serial number) is in: the days, the weeks (from Sunday), the months, the quarters, the years.
    fn index(self, serial: i64) -> i64 {
        let (y, m, _) = civil_from_days(serial - 25569);
        match self {
            Self::Day => serial,
            // Serial 1 is a Sunday in the 1900 system.
            Self::Week => (serial - 1).div_euclid(7),
            Self::Month => y * 12 + i64::from(m) - 1,
            Self::Quarter => y * 4 + (i64::from(m) - 1) / 3,
            Self::Year => y,
        }
    }
}

/// A window of dates counted from today (`Date.IsIn…`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateFilter {
    pub when: DateWhen,
    pub unit: DateUnit,
    /// How many units: `None` is the one unit (the current, the previous, the next).
    pub count: Option<u32>,
}

impl DateFilter {
    /// Whether the date (a serial number, the time of day dropped) is in the window when today is `today` (a serial number).
    pub fn contains(&self, date: f64, today: i64) -> bool {
        let d = date.floor() as i64;
        if self.when == DateWhen::YearToDate {
            return DateUnit::Year.index(d) == DateUnit::Year.index(today) && d <= today;
        }
        let (u, t) = (self.unit.index(d), self.unit.index(today));
        let n = i64::from(self.count.unwrap_or(1));
        match self.when {
            DateWhen::Current => u == t,
            DateWhen::Previous => u >= t - n && u < t,
            DateWhen::Next => u > t && u <= t + n,
            DateWhen::YearToDate => unreachable!(),
        }
    }
}

/// A condition on a row: a comparison of a column, or comparisons joined by `and` / `or`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cond {
    Leaf { column: String, op: Op, value: String },
    /// A part of a date compared with a number (`Date.Year([d]) >= 2024`).
    Part { column: String, part: DatePart, op: Op, value: String },
    Date { column: String, filter: DateFilter },
    And(Vec<Cond>),
    Or(Vec<Cond>),
    Not(Box<Cond>),
    /// A condition written in brackets (Excel writes what the checkboxes of a column pick so, and puts the next column's in brackets after an
    /// `and`); it means what is inside.
    Group(Box<Cond>),
}

impl Cond {
    /// The names of the columns the condition reads.
    pub fn columns(&self) -> Vec<&str> {
        match self {
            Cond::Leaf { column, .. } | Cond::Part { column, .. } | Cond::Date { column, .. } => vec![column],
            Cond::And(v) | Cond::Or(v) => v.iter().flat_map(Cond::columns).collect(),
            Cond::Not(c) | Cond::Group(c) => c.columns(),
        }
    }

    /// Whether the row (its columns named `names`) meets the condition; `today` is a serial number, `p` writes the parameters into a text.
    pub fn matches(&self, names: &[String], row: &[CellValue], today: i64, p: &dyn Fn(&str) -> String) -> bool {
        let at = |column: &str| names.iter().position(|n| n == column).and_then(|i| row.get(i));
        match self {
            Cond::Leaf { column, op, value } => at(column).is_some_and(|v| matches_op(v, *op, &p(value))),
            Cond::Part { column, part, op, value } => match at(column) {
                Some(CellValue::Number(serial)) => {
                    let (y, m, d) = civil_from_days(serial.floor() as i64 - 25569);
                    let n = match part {
                        DatePart::Year => y as f64,
                        DatePart::Quarter => f64::from((m - 1) / 3 + 1),
                        DatePart::Month => f64::from(m),
                        DatePart::Day => f64::from(d),
                    };
                    matches_op(&CellValue::Number(n), *op, &p(value))
                }
                _ => false,
            },
            Cond::Date { column, filter } => matches!(at(column), Some(CellValue::Number(serial)) if filter.contains(*serial, today)),
            Cond::And(v) => v.iter().all(|c| c.matches(names, row, today, p)),
            Cond::Or(v) => v.iter().any(|c| c.matches(names, row, today, p)),
            Cond::Not(c) => !c.matches(names, row, today, p),
            Cond::Group(c) => c.matches(names, row, today, p),
        }
    }
}

/// What a step does with a column it names that is not in the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Missing {
    /// An error (the default).
    Error,
    /// The column is left out of the step (`MissingField.Ignore`).
    Ignore,
    /// A column of nulls is made (`MissingField.UseNull`).
    UseNull,
}

impl Missing {
    pub fn m_name(self) -> &'static str {
        match self {
            Self::Error => "MissingField.Error",
            Self::Ignore => "MissingField.Ignore",
            Self::UseNull => "MissingField.UseNull",
        }
    }

    pub fn from_m(text: &str) -> Option<Missing> {
        [Self::Error, Self::Ignore, Self::UseNull].into_iter().find(|m| m.m_name() == text.trim())
    }
}

impl Step {
    /// The columns a step that `WithMissing` may hold names.
    pub(super) fn named_columns(&self) -> Vec<String> {
        match self {
            Step::SelectColumns(c) | Step::RemoveColumns(c) | Step::ReorderColumns(c) => c.clone(),
            Step::RenameColumns(r) => r.iter().map(|(from, _)| from.clone()).collect(),
            Step::ChangeType { column, .. } => vec![column.clone()],
            _ => Vec::new(),
        }
    }

    /// The step without the columns in `gone`; `None` when nothing is left of it to do.
    pub(super) fn without_columns(&self, gone: &[String]) -> Option<Step> {
        let keep = |c: &Vec<String>| c.iter().filter(|n| !gone.contains(n)).cloned().collect::<Vec<_>>();
        Some(match self {
            Step::SelectColumns(c) => Step::SelectColumns(keep(c)),
            Step::RemoveColumns(c) => Step::RemoveColumns(keep(c)),
            Step::ReorderColumns(c) => Step::ReorderColumns(keep(c)),
            Step::RenameColumns(r) => {
                let r: Vec<_> = r.iter().filter(|(from, _)| !gone.contains(from)).cloned().collect();
                if r.is_empty() {
                    return None;
                }
                Step::RenameColumns(r)
            }
            Step::ChangeType { column, .. } if gone.contains(column) => return None,
            other => other.clone(),
        })
    }
}

/// A function `Table.TransformColumns` applies to each value of a column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Transform {
    Trim,
    Upper,
    Lower,
    /// The first letter of each word a capital (`Text.Proper`).
    Proper,
    /// Control characters taken out (`Text.Clean`).
    Clean,
    /// The number of characters (`Text.Length`).
    Length,
    /// Rounded to the number of decimals (`Number.Round(_, n)`).
    Round(i32),
    RoundUp,
    RoundDown,
    Abs,
    Sqrt,
    Year,
    Month,
    Day,
    /// The text put before (`each "x" & _`).
    Prefix(String),
    /// The text put after (`each _ & "x"`).
    Suffix(String),
}

impl Transform {
    /// The function written in M (what follows the column's name in `{"a", …}`).
    pub fn to_m(&self) -> String {
        let q = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        match self {
            Self::Trim => "Text.Trim".into(),
            Self::Upper => "Text.Upper".into(),
            Self::Lower => "Text.Lower".into(),
            Self::Proper => "Text.Proper".into(),
            Self::Clean => "Text.Clean".into(),
            Self::Length => "Text.Length".into(),
            Self::Round(n) => format!("each Number.Round(_, {n})"),
            Self::RoundUp => "Number.RoundUp".into(),
            Self::RoundDown => "Number.RoundDown".into(),
            Self::Abs => "Number.Abs".into(),
            Self::Sqrt => "Number.Sqrt".into(),
            Self::Year => "Date.Year".into(),
            Self::Month => "Date.Month".into(),
            Self::Day => "Date.Day".into(),
            Self::Prefix(s) => format!("each {} & _", q(s)),
            Self::Suffix(s) => format!("each _ & {}", q(s)),
        }
    }

    /// The type Power Query gives the column after it (the third part of the pair).
    pub fn result_type_m(&self) -> &'static str {
        match self {
            // `Number.RoundUp` and `Number.RoundDown` give whole numbers.
            Self::Length | Self::Year | Self::Month | Self::Day | Self::RoundUp | Self::RoundDown => "Int64.Type",
            Self::Round(_) | Self::Abs | Self::Sqrt => "type number",
            _ => "type text",
        }
    }

    /// The function an M text names (the inverse of [`to_m`](Self::to_m)); `Number.Round` alone rounds to whole numbers.
    pub fn from_m(text: &str) -> Option<Transform> {
        let t = text.trim();
        Some(match t {
            "Text.Trim" => Self::Trim,
            "Text.Upper" => Self::Upper,
            "Text.Lower" => Self::Lower,
            "Text.Proper" => Self::Proper,
            "Text.Clean" => Self::Clean,
            "Text.Length" => Self::Length,
            "Number.Round" => Self::Round(0),
            "Number.RoundUp" => Self::RoundUp,
            "Number.RoundDown" => Self::RoundDown,
            "Number.Abs" => Self::Abs,
            "Number.Sqrt" => Self::Sqrt,
            "Date.Year" => Self::Year,
            "Date.Month" => Self::Month,
            "Date.Day" => Self::Day,
            _ => {
                let body = t.strip_prefix("each")?.trim_start();
                let compact: String = body.split_whitespace().collect();
                if let Some(n) = compact.strip_prefix("Number.Round(_,").and_then(|x| x.strip_suffix(')')) {
                    return n.parse().ok().map(Self::Round);
                }
                if let Some(rest) = body.strip_suffix('_').map(str::trim_end).and_then(|x| x.strip_suffix('&')) {
                    return super::m_parse::unquote(rest).map(Self::Prefix);
                }
                if let Some(rest) = body.strip_prefix('_').map(str::trim_start).and_then(|x| x.strip_prefix('&')) {
                    return super::m_parse::unquote(rest).map(Self::Suffix);
                }
                return None;
            }
        })
    }

    /// The function with the parameters (`{Name}`) in its texts replaced.
    pub(super) fn with_parameters(&self, p: &dyn Fn(&str) -> String) -> Transform {
        match self {
            Self::Prefix(s) => Self::Prefix(p(s)),
            Self::Suffix(s) => Self::Suffix(p(s)),
            other => other.clone(),
        }
    }

    pub(super) fn apply(&self, v: &CellValue) -> CellValue {
        use super::expr::call;
        if matches!(v, CellValue::Empty | CellValue::Error(_)) {
            return v.clone();
        }
        let text = |f: &dyn Fn(&str) -> String| match v {
            CellValue::Text(s) => CellValue::Text(f(s)),
            other => CellValue::Text(f(&super::value_text(other))),
        };
        match self {
            Self::Trim => text(&|s| s.trim().to_string()),
            Self::Upper => text(&|s| s.to_uppercase()),
            Self::Lower => text(&|s| s.to_lowercase()),
            Self::Proper => call("Text.Proper", std::slice::from_ref(v)),
            Self::Clean => text(&|s| s.chars().filter(|c| !c.is_control()).collect()),
            Self::Length => call("Text.Length", std::slice::from_ref(v)),
            Self::Round(n) => call("Number.Round", &[v.clone(), CellValue::Number(f64::from(*n))]),
            Self::RoundUp => call("Number.RoundUp", std::slice::from_ref(v)),
            Self::RoundDown => call("Number.RoundDown", std::slice::from_ref(v)),
            Self::Abs => call("Number.Abs", std::slice::from_ref(v)),
            Self::Sqrt => match v {
                CellValue::Number(n) if *n >= 0.0 => CellValue::Number(n.sqrt()),
                _ => CellValue::Error(ibis_calc::CellError::Num),
            },
            Self::Year => call("Date.Year", std::slice::from_ref(v)),
            Self::Month => call("Date.Month", std::slice::from_ref(v)),
            Self::Day => call("Date.Day", std::slice::from_ref(v)),
            Self::Prefix(s) => text(&|t| format!("{s}{t}")),
            Self::Suffix(s) => text(&|t| format!("{t}{s}")),
        }
    }
}

/// The serial number of `#date(y, m, d)` or `#datetime(y, m, d, h, mi, s)` as written in a step.
pub(crate) fn date_literal(text: &str) -> Option<f64> {
    let t = text.trim();
    let (args, with_time) = if let Some(a) = t.strip_prefix("#datetime(") {
        (a, true)
    } else {
        (t.strip_prefix("#date(")?, false)
    };
    let nums: Vec<f64> = args.strip_suffix(')')?.split(',').map(|x| x.trim().parse().ok()).collect::<Option<_>>()?;
    if nums.len() != if with_time { 6 } else { 3 } {
        return None;
    }
    let serial = days_from_civil(nums[0] as i64, nums[1] as i64, nums[2] as i64) + 25569;
    let time = if with_time { (nums[3] * 3600.0 + nums[4] * 60.0 + nums[5]) / 86400.0 } else { 0.0 };
    Some(serial as f64 + time)
}

/// A date and time text without the zone written after it (`2024-03-05T10:00:00+09:00` → `2024-03-05T10:00:00`).
pub(crate) fn without_zone(text: &str) -> &str {
    let t = text.trim();
    if let Some(x) = t.strip_suffix('Z') {
        return x;
    }
    let b = t.as_bytes();
    // `+hh:mm` or `-hh:mm` at the end, after a time (a `-` of a date is further left).
    if t.len() > 6 && t.contains(':') {
        let tail = &b[t.len() - 6..];
        if matches!(tail[0], b'+' | b'-') && tail[3] == b':' && tail[1..3].iter().chain(&tail[4..]).all(u8::is_ascii_digit) && t[..t.len() - 6].contains(':') {
            return &t[..t.len() - 6];
        }
    }
    t
}

/// A duration text `[-][d.]hh:mm:ss[.fff]` as days.
pub(crate) fn parse_duration(text: &str) -> Option<f64> {
    let t = text.trim();
    let (neg, t) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t),
    };
    let (days, time) = match t.split_once('.') {
        // `d.hh:mm:ss`: the dot before the first colon separates the days.
        Some((d, rest)) if rest.contains(':') && !d.contains(':') => (d.parse::<f64>().ok()?, rest),
        _ => (0.0, t),
    };
    let mut parts = time.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let s: f64 = parts.next().map_or(Some(0.0), |x| x.parse().ok())?;
    let total = days + (h * 3600.0 + m * 60.0 + s) / 86400.0;
    Some(if neg { -total } else { total })
}

/// The pieces a text is split into.
pub(crate) fn split_text(text: &str, by: &SplitBy) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let take = |a: usize, b: usize| -> String { chars[a.min(chars.len())..b.min(chars.len())].iter().collect() };
    match by {
        SplitBy::Delimiter(d) if !d.is_empty() => text.split(d.as_str()).map(str::to_string).collect(),
        SplitBy::Delimiter(_) => vec![text.to_string()],
        SplitBy::Length(n) if *n > 0 => chars.chunks(*n).map(|c| c.iter().collect()).collect(),
        SplitBy::Length(_) => vec![text.to_string()],
        SplitBy::AnyDelimiter(ds) => {
            let ds: Vec<&String> = ds.iter().filter(|d| !d.is_empty()).collect();
            let (mut out, mut cur, mut i) = (Vec::new(), String::new(), 0);
            while i < chars.len() {
                let rest: String = chars[i..].iter().collect();
                match ds.iter().find(|d| rest.starts_with(d.as_str())) {
                    Some(d) => {
                        out.push(std::mem::take(&mut cur));
                        i += d.chars().count();
                    }
                    None => {
                        cur.push(chars[i]);
                        i += 1;
                    }
                }
            }
            out.push(cur);
            out
        }
        SplitBy::EachDelimiter { delimiters, from_end } => {
            let mut pieces = Vec::new();
            let mut rest = text.to_string();
            for d in delimiters.iter().filter(|d| !d.is_empty()) {
                let at = if *from_end { rest.rfind(d.as_str()) } else { rest.find(d.as_str()) };
                let Some(at) = at else { break };
                if *from_end {
                    pieces.push(rest[at + d.len()..].to_string());
                    rest.truncate(at);
                } else {
                    pieces.push(rest[..at].to_string());
                    rest = rest[at + d.len()..].to_string();
                }
            }
            pieces.push(rest);
            if *from_end {
                pieces.reverse();
            }
            pieces
        }
        SplitBy::Positions { positions, from_end } => {
            let n = chars.len();
            // Offsets from the end are turned into offsets from the start; the pieces are in the text's order.
            let mut at: Vec<usize> = positions.iter().map(|&p| if *from_end { n.saturating_sub(p) } else { p.min(n) }).collect();
            at.sort_unstable();
            at.dedup();
            let mut out: Vec<String> = if *from_end {
                // The positions count back from the end: the pieces run from the start to the farthest position, and so on.
                let mut bounds = vec![0];
                bounds.extend(&at);
                bounds.windows(2).map(|w| take(w[0], w[1])).collect()
            } else {
                at.iter().enumerate().map(|(k, &a)| take(a, at.get(k + 1).copied().unwrap_or(n))).collect()
            };
            if out.is_empty() {
                out.push(text.to_string());
            }
            out
        }
        SplitBy::Once { length, from_end } => split_text(text, &SplitBy::Positions { positions: vec![0, *length], from_end: *from_end }),
        SplitBy::Transition { from, to } => {
            let mut out = Vec::new();
            let mut cur = String::new();
            for (k, &c) in chars.iter().enumerate() {
                if k > 0 && from.contains(chars[k - 1]) && to.contains(c) {
                    out.push(std::mem::take(&mut cur));
                }
                cur.push(c);
            }
            out.push(cur);
            out
        }
        SplitBy::Lengths { lengths, from_end } => {
            let n = chars.len();
            let mut out = Vec::new();
            if *from_end {
                let mut end = n;
                for &l in lengths {
                    let start = end.saturating_sub(l);
                    out.push(take(start, end));
                    end = start;
                }
                out.reverse();
            } else {
                let mut start = 0;
                for &l in lengths {
                    out.push(take(start, start + l));
                    start += l;
                }
            }
            if out.is_empty() {
                out.push(text.to_string());
            }
            out
        }
    }
}
