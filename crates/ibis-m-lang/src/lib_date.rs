// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `Date.*`, `DateTime.*`, `Time.*` and `Duration.*` that read or move a date by the calendar. Left for the Excel answers (the request,
//! items 7-9): the week functions (`DayOfWeek`, `WeekOfYear`, `WeekOfMonth`, `StartOfWeek`, `EndOfWeek`), the text forms (`ToText`,
//! `FromText`, `Text.From` of a date), `IsIn...` windows (they need the current time, which the host gives).
use crate::builtin::{as_number, culture_arg, days_in_month};
use crate::culture::{self, Culture};
use crate::ast::{Ty, TypeSpec};
use crate::error::MError;
use crate::eval::Interp;
use crate::table::reg;
use crate::value::*;
use std::collections::HashMap;

const DAY: f64 = 86400.0;

/// Year, month and day of a date or a datetime; null is None; anything else is an error.
fn ymd(v: &Value) -> Result<Option<(i64, i64, i64)>, MError> {
    match v {
        Value::Null => Ok(None),
        Value::Date(d) | Value::DateTime(d, _) => Ok(Some(civil_from_days(*d))),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Date.", v.type_name()))),
    }
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// The same kind of value (date or datetime) with a new day number, keeping the clock time of a datetime.
fn with_day(v: &Value, day: i64) -> Result<Value, MError> {
    if !(0..=days_from_civil(9999, 12, 31)).contains(&day) {
        return Err(MError::expr("The date is outside the range 0001-01-01 to 9999-12-31."));
    }
    Ok(match v {
        Value::DateTime(_, s) => Value::DateTime(day, *s),
        _ => Value::Date(day),
    })
}

fn day_of(v: &Value) -> i64 {
    match v {
        Value::Date(d) | Value::DateTime(d, _) => *d,
        _ => 0,
    }
}

fn add_months(v: &Value, n: i64) -> Result<Value, MError> {
    let Some((y, m, d)) = ymd(v)? else { return Ok(Value::Null) };
    let t = y * 12 + (m - 1) + n;
    let (ny, nm) = (t.div_euclid(12), t.rem_euclid(12) + 1);
    if !(1..=9999).contains(&ny) {
        return Err(MError::expr("The date is outside the range 0001-01-01 to 9999-12-31."));
    }
    with_day(v, days_from_civil(ny, nm, d.min(days_in_month(ny, nm))))
}

fn start_end(v: &Value, f: impl Fn(i64, i64, i64) -> (i64, bool)) -> Result<Value, MError> {
    let Some((y, m, d)) = ymd(v)? else { return Ok(Value::Null) };
    let (day, end_of_day) = f(y, m, d);
    Ok(match v {
        Value::DateTime(..) => Value::DateTime(day, if end_of_day { 86399.9999999 } else { 0.0 }),
        _ => Value::Date(day),
    })
}

fn get(a: &[Value], f: impl Fn(i64, i64, i64, i64) -> f64) -> Result<Value, MError> {
    Ok(match ymd(&a[0])? {
        None => Value::Null,
        Some((y, m, d)) => Value::Number(f(y, m, d, day_of(&a[0]))),
    })
}

fn secs(v: &Value) -> Result<Option<f64>, MError> {
    match v {
        Value::Null => Ok(None),
        Value::Time(s) | Value::DateTime(_, s) => Ok(Some(*s)),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Time.", v.type_name()))),
    }
}

fn dur(v: &Value) -> Result<Option<f64>, MError> {
    match v {
        Value::Null => Ok(None),
        Value::Duration(s) => Ok(Some(*s)),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Duration.", v.type_name()))),
    }
}

pub fn register(l: &mut HashMap<&'static str, Value>) {
    reg(l, "Date.Year", 1, 1, |_, a| get(a, |y, _, _, _| y as f64));
    reg(l, "Date.Month", 1, 1, |_, a| get(a, |_, m, _, _| m as f64));
    reg(l, "Date.Day", 1, 1, |_, a| get(a, |_, _, d, _| d as f64));
    reg(l, "Date.QuarterOfYear", 1, 1, |_, a| get(a, |_, m, _, _| ((m - 1) / 3 + 1) as f64));
    reg(l, "Date.DayOfYear", 1, 1, |_, a| get(a, |y, _, _, day| (day - days_from_civil(y, 1, 1) + 1) as f64));
    reg(l, "Date.DaysInMonth", 1, 1, |_, a| get(a, |y, m, _, _| days_in_month(y, m) as f64));
    reg(l, "Date.IsLeapYear", 1, 1, |_, a| Ok(match ymd(&a[0])? {
        None => Value::Null,
        Some((y, ..)) => Value::Logical(is_leap(y)),
    }));
    reg(l, "Date.AddDays", 2, 2, |_, a| match ymd(&a[0])? {
        None => Ok(Value::Null),
        Some(_) => with_day(&a[0], day_of(&a[0]) + as_number(&a[1])?.floor() as i64),
    });
    reg(l, "Date.AddWeeks", 2, 2, |_, a| match ymd(&a[0])? {
        None => Ok(Value::Null),
        Some(_) => with_day(&a[0], day_of(&a[0]) + 7 * as_number(&a[1])?.floor() as i64),
    });
    reg(l, "Date.AddMonths", 2, 2, |_, a| add_months(&a[0], as_number(&a[1])?.floor() as i64));
    reg(l, "Date.AddQuarters", 2, 2, |_, a| add_months(&a[0], 3 * as_number(&a[1])?.floor() as i64));
    reg(l, "Date.AddYears", 2, 2, |_, a| add_months(&a[0], 12 * as_number(&a[1])?.floor() as i64));
    reg(l, "Date.StartOfDay", 1, 1, |_, a| start_end(&a[0], |y, m, d| (days_from_civil(y, m, d), false)));
    reg(l, "Date.EndOfDay", 1, 1, |_, a| start_end(&a[0], |y, m, d| (days_from_civil(y, m, d), true)));
    reg(l, "Date.StartOfMonth", 1, 1, |_, a| start_end(&a[0], |y, m, _| (days_from_civil(y, m, 1), false)));
    reg(l, "Date.EndOfMonth", 1, 1, |_, a| start_end(&a[0], |y, m, _| (days_from_civil(y, m, days_in_month(y, m)), true)));
    reg(l, "Date.StartOfQuarter", 1, 1, |_, a| start_end(&a[0], |y, m, _| (days_from_civil(y, (m - 1) / 3 * 3 + 1, 1), false)));
    reg(l, "Date.EndOfQuarter", 1, 1, |_, a| {
        start_end(&a[0], |y, m, _| {
            let qm = (m - 1) / 3 * 3 + 3;
            (days_from_civil(y, qm, days_in_month(y, qm)), true)
        })
    });
    reg(l, "Date.StartOfYear", 1, 1, |_, a| start_end(&a[0], |y, _, _| (days_from_civil(y, 1, 1), false)));
    reg(l, "Date.EndOfYear", 1, 1, |_, a| start_end(&a[0], |y, _, _| (days_from_civil(y, 12, 31), true)));
    // A date from a date, a datetime, a serial number or a text of the ISO form; the rest is the culture's (the request, item 8).
    reg(l, "DateTime.Date", 1, 1, |_, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        Value::DateTime(d, _) | Value::Date(d) => Ok(Value::Date(*d)),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type DateTime.", v.type_name()))),
    });
    reg(l, "DateTime.Time", 1, 1, |_, a| Ok(secs(&a[0])?.map_or(Value::Null, Value::Time)));
    reg(l, "Time.Hour", 1, 1, |_, a| Ok(secs(&a[0])?.map_or(Value::Null, |s| Value::Number((s / 3600.0).floor()))));
    reg(l, "Time.Minute", 1, 1, |_, a| Ok(secs(&a[0])?.map_or(Value::Null, |s| Value::Number((s % 3600.0 / 60.0).floor()))));
    reg(l, "Time.Second", 1, 1, |_, a| Ok(secs(&a[0])?.map_or(Value::Null, |s| Value::Number(s % 60.0))));
    reg(l, "Duration.Days", 1, 1, |_, a| Ok(dur(&a[0])?.map_or(Value::Null, |s| Value::Number((s / DAY).trunc()))));
    reg(l, "Duration.Hours", 1, 1, |_, a| Ok(dur(&a[0])?.map_or(Value::Null, |s| Value::Number((s % DAY / 3600.0).trunc()))));
    reg(l, "Duration.Minutes", 1, 1, |_, a| Ok(dur(&a[0])?.map_or(Value::Null, |s| Value::Number((s % 3600.0 / 60.0).trunc()))));
    reg(l, "Duration.Seconds", 1, 1, |_, a| Ok(dur(&a[0])?.map_or(Value::Null, |s| Value::Number(s % 60.0))));
    reg(l, "Duration.TotalDays", 1, 1, |_, a| Ok(dur(&a[0])?.map_or(Value::Null, |s| Value::Number(s / DAY))));
    reg(l, "Duration.TotalHours", 1, 1, |_, a| Ok(dur(&a[0])?.map_or(Value::Null, |s| Value::Number(s / 3600.0))));
    reg(l, "Duration.TotalMinutes", 1, 1, |_, a| Ok(dur(&a[0])?.map_or(Value::Null, |s| Value::Number(s / 60.0))));
    reg(l, "Duration.TotalSeconds", 1, 1, |_, a| Ok(dur(&a[0])?.map_or(Value::Null, Value::Number)));
    reg(l, "Duration.From", 1, 1, |it, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        v => typed(v, Ty::Duration, it.culture()),
    });
    reg(l, "Number.Power", 2, 2, |_, a| match (&a[0], &a[1]) {
        (Value::Null, _) | (_, Value::Null) => Ok(Value::Null),
        (x, y) => {
            let (x, y) = (as_number(x)?, as_number(y)?);
            // Excel: 0 to the power 0 and infinity to the power 0 are NaN (not 1), and 1 to the power NaN is NaN.
            Ok(Value::Number(if (x == 0.0 && y == 0.0) || (x.is_infinite() && y == 0.0) || (x == 1.0 && y.is_nan()) { f64::NAN } else { x.powf(y) }))
        }
    });
    reg(l, "Number.Sign", 1, 1, |_, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        v => {
            let n = as_number(v)?;
            Ok(Value::Number(if n > 0.0 { 1.0 } else if n < 0.0 { -1.0 } else { 0.0 }))
        }
    });
    reg(l, "Number.IsEven", 1, 1, |_, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        v => Ok(Value::Logical(as_number(v)?.rem_euclid(2.0) == 0.0)),
    });
    reg(l, "Number.IsOdd", 1, 1, |_, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        v => Ok(Value::Logical(as_number(v)?.rem_euclid(2.0) == 1.0)),
    });
    reg(l, "Number.Exp", 1, 1, |_, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        v => Ok(Value::Number(as_number(v)?.exp())),
    });
    reg(l, "Number.Log", 1, 2, |_, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        v => {
            let n = as_number(v)?;
            Ok(Value::Number(match a.get(1) {
                Some(b) => n.ln() / as_number(b)?.ln(),
                None => n.ln(),
            }))
        }
    });
}


fn first_day(v: Option<&Value>) -> Result<i64, MError> {
    match v {
        None | Some(Value::Null) => Ok(0),
        Some(v) => {
            let n = as_number(v)?;
            if n.fract() != 0.0 || !(0.0..=6.0).contains(&n) {
                return Err(MError::expr("The first day of the week must be Day.Sunday to Day.Saturday."));
            }
            Ok(n as i64)
        }
    }
}

/// 0 = Sunday .. 6 = Saturday (0001-01-01 was a Monday).
fn weekday(day: i64) -> i64 {
    (day + 1).rem_euclid(7)
}

/// Days from the start of the week (`first` is the first day) to `day`.
fn since_week_start(day: i64, first: i64) -> i64 {
    (weekday(day) - first).rem_euclid(7)
}

fn typed(v: &Value, ty: Ty, c: Culture) -> Result<Value, MError> {
    match crate::table::convert_value(v, &TypeSpec { ty, nullable: true }, c) {
        Value::Failed(e) => Err((*e).clone()),
        v => Ok(v),
    }
}

fn from_text(it: &Interp, a: &[Value], ty: Ty) -> Result<Value, MError> {
    let c = culture_arg(it, a.get(1))?;
    match &a[0] {
        Value::Null => Ok(Value::Null),
        v @ Value::Text(_) => typed(v, ty, c),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Text.", v.type_name()))),
    }
}

/// `Date.ToText(d, format?, culture?)` and the same of datetime and time: no format is the culture's own form; a format string is read by
/// `format.rs`; the format and the culture may be given as a record `[Format=…, Culture=…]` (a date then takes no time part in its format).
fn to_text(it: &Interp, a: &[Value]) -> Result<Value, MError> {
    let (mut format, mut culture_arg_value) = (a.get(1).cloned(), a.get(2).cloned());
    if let Some(r @ Value::Record(_)) = &format {
        let f = it.field(r, "Format", true)?;
        culture_arg_value = Some(it.field(r, "Culture", true)?);
        format = Some(f);
    }
    let c = culture_arg(it, culture_arg_value.as_ref())?;
    let format = match format {
        Some(Value::Text(f)) if !f.is_empty() => Some(f),
        _ => None,
    };
    let moment = |day: i64, secs: f64| crate::format::Moment { day, secs };
    match (&a[0], format) {
        (Value::Null, _) => Ok(Value::Null),
        (Value::Duration(_), Some(_)) => Err(MError::expr("The \"format\" parameter of Duration.ToText has no effect and is not supported.")),
        (Value::Date(d), Some(f)) => crate::format::format_moment(moment(*d, 0.0), &f, c, "DateTime").map(Value::text),
        (Value::DateTime(d, s), Some(f)) => crate::format::format_moment(moment(*d, *s), &f, c, "DateTime").map(Value::text),
        // A time has no day: the format is read on the first of January of year 1.
        (Value::Time(s), Some(f)) => crate::format::format_moment(moment(0, *s), &f, c, "Time").map(Value::text),
        (v, _) => crate::table::scalar_text_in(v, c).map(Value::text).ok_or_else(|| MError::expr(format!("We cannot convert a value of type {} to type Text.", v.type_name()))),
    }
}

/// The calendar-reading functions that depend on the week and the culture, and the number rounding (the request, sections 4, 6, 7).
pub fn register_more(l: &mut HashMap<&'static str, Value>) {
    for (i, n) in ["Day.Sunday", "Day.Monday", "Day.Tuesday", "Day.Wednesday", "Day.Thursday", "Day.Friday", "Day.Saturday"].into_iter().enumerate() {
        l.insert(n, Value::Number(i as f64));
    }
    for (n, v) in [("RoundingMode.Up", 0.0), ("RoundingMode.Down", 1.0), ("RoundingMode.AwayFromZero", 2.0), ("RoundingMode.TowardZero", 3.0), ("RoundingMode.ToEven", 4.0)] {
        l.insert(n, Value::Number(v));
    }
    reg(l, "Number.Round", 1, 3, |_, a| round(a, None));
    reg(l, "Number.RoundUp", 1, 2, |_, a| round(a, Some(0)));
    reg(l, "Number.RoundDown", 1, 2, |_, a| round(a, Some(1)));
    reg(l, "Date.DayOfWeek", 1, 2, |_, a| {
        let first = first_day(a.get(1))?;
        match ymd(&a[0])? {
            None => Ok(Value::Null),
            Some(_) => Ok(Value::Number(since_week_start(day_of(&a[0]), first) as f64)),
        }
    });
    reg(l, "Date.DayOfWeekName", 1, 2, |it, a| {
        let c = culture_arg(it, a.get(1))?;
        match ymd(&a[0])? {
            None => Ok(Value::Null),
            Some(_) => Ok(Value::text(culture::day_name(weekday(day_of(&a[0])) as usize, c))),
        }
    });
    reg(l, "Date.MonthName", 1, 2, |it, a| {
        let c = culture_arg(it, a.get(1))?;
        match ymd(&a[0])? {
            None => Ok(Value::Null),
            Some((_, m, _)) => Ok(Value::text(culture::month_name(m, c))),
        }
    });
    reg(l, "Date.StartOfWeek", 1, 2, |_, a| {
        let first = first_day(a.get(1))?;
        start_end(&a[0], |y, m, d| (days_from_civil(y, m, d) - since_week_start(days_from_civil(y, m, d), first), false))
    });
    reg(l, "Date.EndOfWeek", 1, 2, |_, a| {
        let first = first_day(a.get(1))?;
        start_end(&a[0], |y, m, d| (days_from_civil(y, m, d) - since_week_start(days_from_civil(y, m, d), first) + 6, true))
    });
    // The week of the year counts from the week that holds 1 January; the week of the month from the one that holds the 1st.
    reg(l, "Date.WeekOfYear", 1, 2, |_, a| {
        let first = first_day(a.get(1))?;
        get(a, |y, _, _, day| {
            let jan1 = days_from_civil(y, 1, 1);
            ((day - jan1 + since_week_start(jan1, first)) / 7 + 1) as f64
        })
    });
    reg(l, "Date.WeekOfMonth", 1, 2, |_, a| {
        let first = first_day(a.get(1))?;
        get(a, |y, m, d, _| {
            let first_of_month = days_from_civil(y, m, 1);
            ((d - 1 + since_week_start(first_of_month, first)) / 7 + 1) as f64
        })
    });
    reg(l, "Date.From", 1, 2, |it, a| {
        let c = culture_arg(it, a.get(1))?;
        typed(&a[0], Ty::Date, c)
    });
    reg(l, "DateTime.From", 1, 2, |it, a| {
        let c = culture_arg(it, a.get(1))?;
        typed(&a[0], Ty::DateTime, c)
    });
    reg(l, "Time.From", 1, 2, |it, a| {
        let c = culture_arg(it, a.get(1))?;
        match &a[0] {
            Value::Null => Ok(Value::Null),
            v => typed(v, Ty::Time, c),
        }
    });
    reg(l, "Date.FromText", 1, 2, |it, a| from_text(it, a, Ty::Date));
    reg(l, "Time.FromText", 1, 2, |it, a| from_text(it, a, Ty::Time));
    reg(l, "Duration.FromText", 1, 1, |it, a| from_text(it, a, Ty::Duration));
    reg(l, "Number.Mod", 2, 3, |_, a| arithmetic(a, |x, y| x % y));
    reg(l, "Number.IntegerDivide", 2, 3, |_, a| arithmetic(a, |x, y| (x / y).trunc()));
    for (name, ty) in [("Int64.From", Ty::Int64), ("Int32.From", Ty::Int32), ("Currency.From", Ty::Currency), ("Percentage.From", Ty::Percentage), ("Decimal.From", Ty::Number), ("Double.From", Ty::Number), ("Single.From", Ty::Number)] {
        // The static name is the function's; one closure each cannot name its type, so the type goes by the name.
        let f: crate::value::Builtin = match ty {
            Ty::Int64 => |it, a| number_type(it, a, Ty::Int64),
            Ty::Int32 => |it, a| number_type(it, a, Ty::Int32),
            Ty::Currency => |it, a| number_type(it, a, Ty::Currency),
            Ty::Percentage => |it, a| number_type(it, a, Ty::Percentage),
            _ => |it, a| number_type(it, a, Ty::Number),
        };
        l.insert(name, Value::Function(std::rc::Rc::new(crate::value::Func::Builtin { name, min: 1, max: 2, f })));
    }
    reg(l, "DateTime.FromText", 1, 2, |it, a| from_text(it, a, Ty::DateTime));
    reg(l, "Date.ToText", 1, 3, |it, a| to_text(it, a));
    reg(l, "DateTime.ToText", 1, 3, |it, a| to_text(it, a));
    reg(l, "Time.ToText", 1, 3, |it, a| to_text(it, a));
    reg(l, "Duration.ToText", 1, 2, |it, a| to_text(it, &[a[0].clone(), Value::Null, a.get(1).cloned().unwrap_or(Value::Null)]));
}

/// The windows of `Date.IsIn…`: the unit of the date and of today are numbered (days; weeks from Sunday unless a first day is given; months;
/// quarters; years) and the date is in the window when its number is the one of today, the `n` before (not today's), or the `n` after.
/// The same windows as the screen's date filter (`DateFilter`). Whether Excel counts the same is the open item 20 of the second request.
fn unit_index(unit: &str, day: i64, first: i64) -> i64 {
    let (y, m, _) = civil_from_days(day);
    match unit {
        "Day" => day,
        "Week" => (day + 1 - first).div_euclid(7),
        "Month" => y * 12 + m - 1,
        "Quarter" => y * 4 + (m - 1) / 3,
        _ => y,
    }
}

fn in_window(it: &Interp, a: &[Value], unit: &'static str, when: i32, with_count: bool) -> Result<Value, MError> {
    let Some(_) = ymd(&a[0])? else { return Ok(Value::Null) };
    let (today, _) = it.now()?;
    let n = if with_count { as_number(&a[1])? as i64 } else { 1 };
    let first = if unit == "Week" { first_day(a.get(if with_count { 2 } else { 1 }))? } else { 0 };
    let (u, t) = (unit_index(unit, day_of(&a[0]), first), unit_index(unit, today, first));
    Ok(Value::Logical(match when {
        0 => u == t,
        1 => u >= t - n && u < t,
        _ => u > t && u <= t + n,
    }))
}

pub fn register_clock(l: &mut HashMap<&'static str, Value>) {
    reg(l, "DateTime.LocalNow", 0, 0, |it, _| it.now().map(|(d, s)| Value::DateTime(d, s)));
    reg(l, "DateTime.FixedLocalNow", 0, 0, |it, _| it.now().map(|(d, s)| Value::DateTime(d, s)));
    reg(l, "Date.IsInYearToDate", 1, 1, |it, a| {
        let Some((y, ..)) = ymd(&a[0])? else { return Ok(Value::Null) };
        let (today, _) = it.now()?;
        Ok(Value::Logical(y == civil_from_days(today).0 && day_of(&a[0]) <= today))
    });
    reg(l, "DateTime.IsInYearToDate", 1, 1, |it, a| {
        let Some((y, ..)) = ymd(&a[0])? else { return Ok(Value::Null) };
        let (today, _) = it.now()?;
        Ok(Value::Logical(y == civil_from_days(today).0 && day_of(&a[0]) <= today))
    });
    macro_rules! windows {
        ($($prefix:literal),*) => {$(
            windows!(@unit $prefix, "Day"); windows!(@unit $prefix, "Week"); windows!(@unit $prefix, "Month"); windows!(@unit $prefix, "Quarter"); windows!(@unit $prefix, "Year");
        )*};
        (@unit $prefix:literal, $unit:literal) => {
            reg(l, concat!($prefix, ".IsInCurrent", $unit), 1, 2, |it, a| in_window(it, a, $unit, 0, false));
            reg(l, concat!($prefix, ".IsInPrevious", $unit), 1, 2, |it, a| in_window(it, a, $unit, 1, false));
            reg(l, concat!($prefix, ".IsInNext", $unit), 1, 2, |it, a| in_window(it, a, $unit, 2, false));
            reg(l, concat!($prefix, ".IsInPreviousN", $unit, "s"), 2, 3, |it, a| in_window(it, a, $unit, 1, true));
            reg(l, concat!($prefix, ".IsInNextN", $unit, "s"), 2, 3, |it, a| in_window(it, a, $unit, 2, true));
        };
    }
    windows!("Date", "DateTime");
}

/// `Number.Round(x, digits, mode)`: half to even by default (`2.5` is 2); `fixed` is the mode of `RoundUp` (0: toward + infinity) and
/// `RoundDown` (1: toward - infinity).
fn round(a: &[Value], fixed: Option<i32>) -> Result<Value, MError> {
    if matches!(a[0], Value::Null) {
        return Ok(Value::Null);
    }
    let x = as_number(&a[0])?;
    let digits = match a.get(1) {
        None | Some(Value::Null) => 0,
        Some(d) => as_number(d)? as i32,
    };
    let mode = match (fixed, a.get(2)) {
        (Some(m), _) => m,
        (None, Some(Value::Number(m))) => *m as i32,
        _ => 4,
    };
    Ok(Value::Number(culture::round_scaled(x, digits, mode)))
}

/// `Number.Mod` and `Number.IntegerDivide`: null gives null, a text is an error, a divisor of 0 gives NaN (not an error); the third
/// argument, the precision (0 double, 1 decimal), is checked and does not change the value.
fn arithmetic(a: &[Value], f: impl Fn(f64, f64) -> f64) -> Result<Value, MError> {
    if let Some(p) = a.get(2) {
        match p {
            Value::Null => {}
            p => {
                let n = as_number(p)?;
                if n != 0.0 && n != 1.0 {
                    return Err(MError::expr("The value of 'Precision.Type' is not one of Precision.Double, Precision.Decimal."));
                }
            }
        }
    }
    if matches!(a[0], Value::Null) || matches!(a[1], Value::Null) {
        return Ok(Value::Null);
    }
    let (x, y) = (as_number(&a[0])?, as_number(&a[1])?);
    if y == 0.0 {
        return Ok(Value::Number(f64::NAN));
    }
    Ok(Value::Number(f(x, y)))
}

/// `Currency.From`, `Int64.From`...: the conversion of a column of that type (a value that cannot be is an error here).
fn number_type(it: &Interp, a: &[Value], ty: Ty) -> Result<Value, MError> {
    if matches!(a[0], Value::Null) {
        return Ok(Value::Null);
    }
    typed(&a[0], ty, culture_arg(it, a.get(1))?)
}
