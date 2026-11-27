// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The datetimezone: a datetime with its offset from UTC, as Microsoft 365 Excel's Power Query gave it (K0039, the second measurement).
//! Two datetimezones are equal and ordered by the instant (`04:05:09+00:00` = `13:05:09+09:00`); the difference is a duration of the
//! instants; the text is the datetime's with ` +09:00`; `DateTime.From`/`Date.From`/`Time.From` of one give the clock time as written
//! (the offset is not taken off); a text ending in `Z` is UTC and a datetime made of it is the local time; a text with an offset is
//! not a datetime. The offset of `SwitchZone(z, -5, 30)` is `-5 * 60 + 30` minutes (-04:30): the minutes do not take the hours' sign.
use crate::error::MError;
use crate::builtin::days_in_month;
use crate::table::reg;
use crate::value::*;
use std::collections::HashMap;

const DAY: f64 = 86400.0;

/// The instant as seconds since 0001-01-01T00:00:00Z: what two datetimezones are compared by.
pub fn instant(day: i64, secs: f64, offset: i32) -> f64 {
    day as f64 * DAY + secs - f64::from(offset) * 60.0
}

/// `+09:00` (or `+09` without the colon form).
pub fn offset_text(offset: i32, long: bool) -> String {
    let sign = if offset < 0 { '-' } else { '+' };
    let (h, m) = (offset.abs() / 60, offset.abs() % 60);
    if long { format!("{sign}{h:02}:{m:02}") } else { format!("{sign}{h:02}") }
}

pub fn ends_in_z(s: &str) -> bool {
    let t = s.trim();
    t.len() > 1 && (t.ends_with('Z') || t.ends_with('z'))
}

/// Splits a text into its date and time and its offset: a trailing `Z` is 0, `+09:00`, `-0530` and `+09` are the offset in minutes.
fn split_zone(s: &str) -> (&str, Option<i32>) {
    let t = s.trim();
    if ends_in_z(t) {
        return (&t[..t.len() - 1], Some(0));
    }
    // The offset sits after the time: a sign after the last ':' or after a digit of the time part.
    if let Some(at) = t.rfind(['+', '-']).filter(|&at| at > 0 && t[..at].contains(':')) {
        let (body, z) = (&t[..at], &t[at + 1..]);
        let sign = if t.as_bytes()[at] == b'-' { -1 } else { 1 };
        let digits: String = z.chars().filter(|c| *c != ':').collect();
        if digits.chars().all(|c| c.is_ascii_digit()) && matches!(digits.len(), 2 | 4) {
            let h: i32 = digits[..2].parse().unwrap_or(0);
            let m: i32 = if digits.len() == 4 { digits[2..].parse().unwrap_or(0) } else { 0 };
            if h < 24 && m < 60 {
                return (body, Some(sign * (h * 60 + m)));
            }
        }
    }
    (t, None)
}

/// A value made a datetimezone: a datetimezone as it is; a datetime or date or serial number takes the local offset; a text takes the offset
/// it carries (`Z` is +00:00), else the local one.
pub fn to_zone(v: &Value, local: i32) -> Option<Value> {
    Some(match v {
        Value::DateTimeZone(..) => v.clone(),
        Value::DateTime(d, s) => Value::DateTimeZone(*d, *s, local),
        Value::Date(d) => Value::DateTimeZone(*d, 0.0, local),
        Value::Number(n) => Value::DateTimeZone(crate::table::serial_epoch() + n.floor() as i64, ((n - n.floor()) * 86400.0 * 1000.0).round() / 1000.0, local),
        Value::Text(s) => {
            let (body, off) = split_zone(s);
            let (d, secs) = crate::culture::parse_datetime(body)?;
            Value::DateTimeZone(d, secs, off.unwrap_or(local))
        }
        _ => return None,
    })
}

/// The local clock time of a datetimezone, as a datetime (the instant moved to the local offset).
pub fn local_of(z: &Value, local: i32) -> Value {
    match z {
        Value::DateTimeZone(d, s, o) => {
            let total = instant(*d, *s, *o) + f64::from(local) * 60.0;
            Value::DateTime(total.div_euclid(DAY) as i64, total.rem_euclid(DAY))
        }
        v => v.clone(),
    }
}

fn with_offset(z: &Value, offset: i32) -> Value {
    match z {
        Value::DateTimeZone(d, s, o) => {
            let total = instant(*d, *s, *o) + f64::from(offset) * 60.0;
            Value::DateTimeZone(total.div_euclid(DAY) as i64, total.rem_euclid(DAY), offset)
        }
        v => v.clone(),
    }
}

fn zone_arg(v: &Value) -> Result<Option<(i64, f64, i32)>, MError> {
    match v {
        Value::Null => Ok(None),
        Value::DateTimeZone(d, s, o) => Ok(Some((*d, *s, *o))),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type DateTimeZone.", v.type_name()))),
    }
}

fn num(v: &Value) -> Result<f64, MError> {
    match v {
        Value::Number(n) => Ok(*n),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Number.", v.type_name()))),
    }
}

/// A DateTimeZone format: `zz` is `+09` and `zzz` is `+09:00` (the text is put in quotes for the datetime's formatter); a single `z` or a `K`
/// is not a format Excel accepts for a datetimezone.
pub fn prepare_format(f: &str, offset: i32) -> Result<String, MError> {
    let chars: Vec<char> = f.chars().collect();
    let mut out = String::new();
    let (mut i, mut quoted) = (0, false);
    while i < chars.len() {
        let ch = chars[i];
        if ch == '\'' {
            quoted = !quoted;
        }
        if !quoted && ch == '\\' && i + 1 < chars.len() {
            out.push(ch);
            out.push(chars[i + 1]);
            i += 2;
            continue;
        }
        if !quoted && (ch == 'z' || ch == 'K') {
            let mut run = 1;
            while i + run < chars.len() && chars[i + run] == ch {
                run += 1;
            }
            if ch == 'K' || run == 1 {
                return Err(MError::expr("The specified output DateTimeZone format is not supported."));
            }
            out.push('\'');
            out.push_str(&offset_text(offset, run >= 3));
            out.push('\'');
            i += run;
            continue;
        }
        out.push(ch);
        i += 1;
    }
    Ok(out)
}

pub fn register(l: &mut HashMap<&'static str, Value>) {
    reg(l, "#datetimezone", 8, 8, |_, a| {
        let (y, m, d) = (num(&a[0])? as i64, num(&a[1])? as i64, num(&a[2])? as i64);
        if !(1..=9999).contains(&y) || !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
            return Err(MError::expr("The datetimezone is not valid."));
        }
        let (oh, om) = (num(&a[6])?, num(&a[7])?);
        if !(-14.0..=14.0).contains(&oh) || !(-59.0..=59.0).contains(&om) {
            return Err(MError::expr("The datetimezone is not valid."));
        }
        Ok(Value::DateTimeZone(days_from_civil(y, m, d), num(&a[3])? * 3600.0 + num(&a[4])? * 60.0 + num(&a[5])?, (oh * 60.0 + om) as i32))
    });
    for name in ["DateTimeZone.LocalNow", "DateTimeZone.FixedLocalNow"] {
        reg(l, name, 0, 0, |it, _| it.now().map(|(d, s)| Value::DateTimeZone(d, s, it.utc_offset())));
    }
    for name in ["DateTimeZone.UtcNow", "DateTimeZone.FixedUtcNow"] {
        reg(l, name, 0, 0, |it, _| {
            let (d, s) = it.now()?;
            Ok(with_offset(&Value::DateTimeZone(d, s, it.utc_offset()), 0))
        });
    }
    reg(l, "DateTimeZone.From", 1, 2, |it, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        v => to_zone(v, it.utc_offset()).ok_or_else(|| MError::format("We couldn't convert to DateTimeZone.")),
    });
    reg(l, "DateTimeZone.FromText", 1, 2, |it, a| match &a[0] {
        Value::Null => Ok(Value::Null),
        Value::Text(s) if s.trim().is_empty() => Ok(Value::Null),
        v @ Value::Text(_) => to_zone(v, it.utc_offset()).ok_or_else(|| MError::format("The input could not be parsed as a DateTimeZone.")),
        v => Err(MError::expr(format!("We cannot convert a value of type {} to type Text.", v.type_name()))),
    });
    reg(l, "DateTimeZone.ToLocal", 1, 1, |it, a| Ok(zone_arg(&a[0])?.map_or(Value::Null, |_| with_offset(&a[0], it.utc_offset()))));
    reg(l, "DateTimeZone.ToUtc", 1, 1, |_, a| Ok(zone_arg(&a[0])?.map_or(Value::Null, |_| with_offset(&a[0], 0))));
    reg(l, "DateTimeZone.SwitchZone", 2, 3, |_, a| {
        if zone_arg(&a[0])?.is_none() {
            return Ok(Value::Null);
        }
        let (h, m) = (num(&a[1])?, a.get(2).map(num).transpose()?.unwrap_or(0.0));
        Ok(with_offset(&a[0], (h * 60.0 + m) as i32))
    });
    reg(l, "DateTimeZone.RemoveZone", 1, 1, |_, a| Ok(zone_arg(&a[0])?.map_or(Value::Null, |(d, s, _)| Value::DateTime(d, s))));
    reg(l, "DateTimeZone.ZoneHours", 1, 1, |_, a| Ok(zone_arg(&a[0])?.map_or(Value::Null, |(.., o)| Value::Number(f64::from(o / 60)))));
    reg(l, "DateTimeZone.ZoneMinutes", 1, 1, |_, a| Ok(zone_arg(&a[0])?.map_or(Value::Null, |(.., o)| Value::Number(f64::from(o % 60)))));
    reg(l, "DateTimeZone.ToText", 1, 3, |it, a| crate::lib_date::to_text_pub(it, a));
}
