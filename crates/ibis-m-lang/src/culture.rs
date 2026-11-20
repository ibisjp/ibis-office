// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What depends on the culture, as Microsoft 365 Excel's Power Query gave it (Japanese install, `Culture.Current` = ja-JP; the values are
//! those of `tasks/doc/requests/2026-11-17-k0039-m-semantics-result.md`, sections 6 and 8): how numbers, dates, times and durations are
//! written as text, the names of the days and months, and how a text is read as a number or a date. Only `ja-JP` (the default) and
//! `en-US` are known; another culture is an error, not a guess.
use crate::error::MError;
use crate::value::{civil_from_days, days_from_civil};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Culture {
    Ja,
    En,
}

impl Culture {
    pub fn parse(name: &str) -> Result<Culture, MError> {
        match name {
            "ja-JP" | "ja" => Ok(Culture::Ja),
            "en-US" | "en" => Ok(Culture::En),
            other => Err(MError::unsupported(&format!("the culture {other}"), 0)),
        }
    }
}

/// A number as `Text.From` writes it: the shortest of 15 and 17 significant digits that reads back the same (what .NET's round-trip format
/// does: 0.1 + 0.2 is 0.30000000000000004, 1/3 is 0.33333333333333331); no exponent unless the number is below 1E-05 ("1E-07"); `NaN`; `∞`.
pub fn number_text(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "\u{221e}".into() } else { "-\u{221e}".into() };
    }
    if n == 0.0 {
        return "0".into();
    }
    let sign = if n < 0.0 { "-" } else { "" };
    let a = n.abs();
    let sci = {
        let s15 = format!("{:.14e}", a);
        if s15.parse::<f64>().ok() == Some(a) { s15 } else { format!("{:.16e}", a) }
    };
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(|c| c.is_ascii_digit()).collect::<String>().trim_end_matches('0').to_string();
    let digits = if digits.is_empty() { "0".to_string() } else { digits };
    if exp <= -5 {
        let rest = &digits[1..];
        let body = if rest.is_empty() { digits[..1].to_string() } else { format!("{}.{}", &digits[..1], rest) };
        return format!("{sign}{body}E-{:02}", -exp);
    }
    let body = if exp >= 0 {
        let int_len = exp as usize + 1;
        if digits.len() <= int_len {
            format!("{digits}{}", "0".repeat(int_len - digits.len()))
        } else {
            format!("{}.{}", &digits[..int_len], &digits[int_len..])
        }
    } else {
        format!("0.{}{digits}", "0".repeat((-exp - 1) as usize))
    };
    format!("{sign}{body}")
}

/// A number read from text: blanks around are allowed, a sign, digits with `,` as the group mark, `.` as the point and an exponent. `1.234,5`,
/// `0x10`, `-`, full-width digits and the empty text are not numbers (the same in ja-JP and en-US).
pub fn parse_number(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (sign, rest) = match t.as_bytes()[0] {
        b'+' => (1.0, &t[1..]),
        b'-' => (-1.0, &t[1..]),
        _ => (1.0, t),
    };
    if rest.is_empty() || !rest.bytes().all(|c| c.is_ascii_digit() || matches!(c, b',' | b'.' | b'e' | b'E' | b'+' | b'-')) {
        return None;
    }
    // `,` only between digits of the integer part.
    let (int_part, tail) = match rest.find(['.', 'e', 'E']) {
        Some(i) => rest.split_at(i),
        None => (rest, ""),
    };
    if int_part.is_empty() && !tail.starts_with('.') {
        return None;
    }
    if int_part.contains(',') {
        let groups: Vec<&str> = int_part.split(',').collect();
        if groups[0].is_empty() || groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3) {
            return None;
        }
    }
    let clean: String = format!("{}{tail}", int_part.replace(',', ""));
    if clean.contains(',') {
        return None;
    }
    clean.parse::<f64>().ok().map(|v| sign * v)
}

pub const DAY_NAMES_JA: [&str; 7] = ["日曜日", "月曜日", "火曜日", "水曜日", "木曜日", "金曜日", "土曜日"];
pub const DAY_NAMES_EN: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS_EN: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

pub fn day_name(dow: usize, c: Culture) -> String {
    match c {
        Culture::Ja => DAY_NAMES_JA[dow % 7].to_string(),
        Culture::En => DAY_NAMES_EN[dow % 7].to_string(),
    }
}

pub fn month_name(m: i64, c: Culture) -> String {
    match c {
        Culture::Ja => format!("{m}月"),
        Culture::En => MONTHS_EN[((m - 1).rem_euclid(12)) as usize].to_string(),
    }
}

fn hms(secs: f64) -> (i64, i64, i64) {
    let t = secs.floor() as i64;
    (t / 3600, t % 3600 / 60, t % 60)
}

pub fn date_text(day: i64, c: Culture) -> String {
    let (y, m, d) = civil_from_days(day);
    match c {
        Culture::Ja => format!("{y}/{m:02}/{d:02}"),
        Culture::En => format!("{m}/{d}/{y}"),
    }
}

pub fn time_text(secs: f64, c: Culture) -> String {
    let (h, mi, _) = hms(secs);
    match c {
        Culture::Ja => format!("{h}:{mi:02}"),
        Culture::En => format!("{}:{mi:02} {}", if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "AM" } else { "PM" }),
    }
}

pub fn datetime_text(day: i64, secs: f64, c: Culture) -> String {
    let (h, mi, s) = hms(secs);
    match c {
        Culture::Ja => format!("{} {h}:{mi:02}:{s:02}", date_text(day, c)),
        Culture::En => format!("{} {}:{mi:02}:{s:02} {}", date_text(day, c), if h % 12 == 0 { 12 } else { h % 12 }, if h < 12 { "AM" } else { "PM" }),
    }
}

/// `[-][d.]hh:mm:ss[.fffffff]`.
pub fn duration_text(secs: f64) -> String {
    let neg = secs < 0.0;
    let a = secs.abs();
    let whole = a.floor() as i64;
    let frac = ((a - a.floor()) * 1e7).round() as i64;
    let (d, h, m, s) = (whole / 86400, whole % 86400 / 3600, whole % 3600 / 60, whole % 60);
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if d > 0 {
        out.push_str(&format!("{d}."));
    }
    out.push_str(&format!("{h:02}:{m:02}:{s:02}"));
    if frac > 0 {
        out.push_str(&format!(".{frac:07}"));
    }
    out
}

fn month_of(word: &str) -> Option<i64> {
    let w = word.to_lowercase();
    MONTHS_EN.iter().position(|m| {
        let m = m.to_lowercase();
        m == w || (w.len() == 3 && m.starts_with(&w)) || (w == "sept" && m == "september")
    }).map(|i| i as i64 + 1)
}

fn valid(y: i64, m: i64, d: i64) -> Option<i64> {
    ((1..=9999).contains(&y) && (1..=12).contains(&m) && d >= 1 && d <= crate::builtin::days_in_month(y, m)).then(|| days_from_civil(y, m, d))
}

/// Runs of digits, of letters, and single other characters.
fn pieces(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in s.chars() {
        let kind = if c.is_ascii_digit() { 1 } else if c.is_alphabetic() { 2 } else { 0 };
        match out.last_mut() {
            Some(last) if kind != 0 && last.chars().next().is_some_and(|f| (f.is_ascii_digit() && kind == 1) || (f.is_alphabetic() && kind == 2)) => last.push(c),
            _ => out.push(c.to_string()),
        }
    }
    out
}

/// A date from text. Read: `2026-11-17`, `2026/11/17`, `11/17/2026` (month first in both cultures; `17/11/2026` is not a date),
/// `2026年11月17日`, `20261117`, `Nov 17, 2026`, `17-Nov-2026`. A text with a time is not a date (`Date.FromText("2026-11-17 13:05:09")` fails).
pub fn parse_date(s: &str) -> Option<i64> {
    let t = s.trim();
    let p = pieces(t);
    let num = |i: usize| -> Option<i64> { p.get(i)?.parse().ok() };
    let sep = |i: usize, c: &str| p.get(i).is_some_and(|x| x == c);
    match p.len() {
        1 if t.len() == 8 && t.bytes().all(|c| c.is_ascii_digit()) => valid(t[..4].parse().ok()?, t[4..6].parse().ok()?, t[6..].parse().ok()?),
        5 if (sep(1, "-") && sep(3, "-") || sep(1, "/") && sep(3, "/")) && p[0].len() == 4 => valid(num(0)?, num(2)?, num(4)?),
        5 if sep(1, "/") && sep(3, "/") && p[4].len() == 4 => valid(num(4)?, num(0)?, num(2)?),
        6 if sep(1, "年") && sep(3, "月") && sep(5, "日") => valid(num(0)?, num(2)?, num(4)?),
        5 if sep(1, "-") && sep(3, "-") && p[0].len() <= 2 && p[4].len() == 4 => valid(num(4)?, month_of(&p[2])?, num(0)?),
        // `年` `月` `日` are letters, so the Japanese form is 6 pieces (digits 年 digits 月 digits 日).
        _ => {
            // `Nov 17, 2026`: word, blank, digits, comma, blank, digits.
            let q: Vec<&String> = p.iter().filter(|x| x.as_str() != " ").collect();
            if q.len() == 4 && q[2].as_str() == "," {
                return valid(q[3].parse().ok()?, month_of(q[0])?, q[1].parse().ok()?);
            }
            None
        }
    }
}

/// A datetime from text: a date as `parse_date` reads it, alone, or followed by `T` or a blank and `h:mm[:ss[.f]]`.
pub fn parse_datetime(s: &str) -> Option<(i64, f64)> {
    let t = s.trim();
    if let Some(d) = parse_date(t) {
        return Some((d, 0.0));
    }
    let (date, time) = t.split_once(['T', ' '])?;
    let d = parse_date(date)?;
    let mut parts = time.trim().split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next().map(|x| x.parse().ok()).unwrap_or(Some(0.0))?;
    let sec: f64 = parts.next().map(|x| x.parse().ok()).unwrap_or(Some(0.0))?;
    if parts.next().is_some() || !(0.0..24.0).contains(&h) || !(0.0..60.0).contains(&m) || !(0.0..60.0).contains(&sec) {
        return None;
    }
    Some((d, h * 3600.0 + m * 60.0 + sec))
}

/// Rounds half to even after scaling by a power of ten (the way Excel's `Number.Round` was measured: 2.675 → 2.68 and 1.005 → 1.00 come
/// from the product 2.675 * 100 being 267.5 and 1.005 * 100 being 100.49999999999999 in doubles, then rounding to even).
pub fn round_scaled(x: f64, digits: i32, mode: i32) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let scale = 10f64.powi(digits.abs());
    let scaled = if digits >= 0 { x * scale } else { x / scale };
    let r = match mode {
        0 => scaled.ceil(),
        1 => scaled.floor(),
        2 => scaled.signum() * (scaled.abs() + 0.5).floor(),
        3 => scaled.trunc(),
        _ => {
            let f = scaled.floor();
            let diff = scaled - f;
            if diff > 0.5 {
                f + 1.0
            } else if diff < 0.5 {
                f
            } else if f % 2.0 == 0.0 {
                f
            } else {
                f + 1.0
            }
        }
    };
    let back = if digits >= 0 { r / scale } else { r * scale };
    // -0 is 0.
    if back == 0.0 { 0.0 } else { back }
}

/// The simple (one character to one character) upper-case mapping, as .NET's invariant `ToUpper` does it: `ß` stays `ß`, `ǆ` is `Ǆ`.
pub fn simple_upper(c: char) -> char {
    let mut it = c.to_uppercase();
    match (it.next(), it.next()) {
        (Some(u), None) => u,
        _ => c,
    }
}

/// The simple lower-case mapping: `İ` is `i`, `Σ` is `σ` (no final sigma), `ǅ` is `ǆ`.
pub fn simple_lower(c: char) -> char {
    if c == '\u{130}' {
        return 'i';
    }
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

/// A time of day from text (seconds in the day): `13:05:09`, `1:05 PM` (also `13:05:09 PM`: the PM of a 24-hour time changes nothing),
/// `13:05`, `13:05:09.5`, `13:5:9`, `1305`, `13時05分09秒`, blanks around, an offset (`+09:00`, dropped); `午後 1:05`, `1:05:09 午後` only in
/// ja-JP. A time past 23:59:59 and a leading `T` are not times.
pub fn parse_time(s: &str, c: Culture) -> Option<f64> {
    let mut t = s.trim().to_string();
    let mut pm = false;
    let mut am = false;
    for (word, is_pm) in [("PM", true), ("pm", true), ("AM", false), ("am", false), ("午後", true), ("午前", false)] {
        let japanese = word == "午後" || word == "午前";
        if japanese && c != Culture::Ja {
            continue;
        }
        if t.contains(word) {
            t = t.replace(word, " ");
            if is_pm { pm = true } else { am = true }
        }
    }
    let mut t = t.trim().to_string();
    // An offset after the time is read and dropped.
    if let Some(i) = t.rfind(['+']).filter(|&i| i > 0) {
        t.truncate(i);
    } else if let Some(i) = t.rfind('-').filter(|&i| i > 0) {
        t.truncate(i);
    }
    let t = t.trim();
    let (h, m, sec): (f64, f64, f64) = if let (Some(hi), Some(mi)) = (t.find('時'), t.find('分')) {
        let h: f64 = t[..hi].parse().ok()?;
        let m: f64 = t[hi + '時'.len_utf8()..mi].parse().ok()?;
        let rest = &t[mi + '分'.len_utf8()..];
        let sec: f64 = if rest.is_empty() { 0.0 } else { rest.strip_suffix('秒')?.parse().ok()? };
        (h, m, sec)
    } else if t.len() == 4 && t.bytes().all(|b| b.is_ascii_digit()) {
        (t[..2].parse().ok()?, t[2..].parse().ok()?, 0.0)
    } else {
        let mut parts = t.split(':');
        let h: f64 = parts.next()?.trim().parse().ok()?;
        let m: f64 = parts.next()?.trim().parse().ok()?;
        let sec: f64 = match parts.next() {
            Some(x) => x.trim().parse().ok()?,
            None => 0.0,
        };
        if parts.next().is_some() {
            return None;
        }
        (h, m, sec)
    };
    let h = if pm && h < 12.0 { h + 12.0 } else if am && h == 12.0 { 0.0 } else { h };
    if !(0.0..24.0).contains(&h) || !(0.0..60.0).contains(&m) || !(0.0..60.0).contains(&sec) {
        return None;
    }
    Some(h * 3600.0 + m * 60.0 + sec)
}

/// A duration from text (seconds): `1.02:03:04`, `02:03:04`, `-1.00:00:00`, `1:30` (hours and minutes), `12` (days), `P1D` (ISO), with a
/// fraction of a second. `+02:03:04`, `1d`, `1:2:3:4`, `25:00:00` and `1.5` are not durations.
pub fn parse_duration(s: &str) -> Option<f64> {
    let t = s.trim();
    if let Some(iso) = t.strip_prefix('P') {
        // Only days (`P1D`) are read of the ISO form.
        let days: f64 = iso.strip_suffix('D')?.parse().ok()?;
        return Some(days * 86400.0);
    }
    let (neg, body) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t),
    };
    if body.starts_with('+') || body.is_empty() {
        return None;
    }
    let total = if !body.contains(':') {
        // A bare number of days, whole.
        if !body.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        body.parse::<f64>().ok()? * 86400.0
    } else {
        let (days, clock) = match body.split_once('.') {
            Some((d, rest)) if rest.contains(':') && d.bytes().all(|b| b.is_ascii_digit()) => (d.parse::<f64>().ok()?, rest),
            _ => (0.0, body),
        };
        let parts: Vec<&str> = clock.split(':').collect();
        if !(2..=3).contains(&parts.len()) {
            return None;
        }
        let h: f64 = parts[0].parse().ok()?;
        let m: f64 = parts[1].parse().ok()?;
        let sec: f64 = parts.get(2).map_or(Some(0.0), |x| x.parse().ok())?;
        if h >= 24.0 && days == 0.0 && parts.len() == 3 || m >= 60.0 || sec >= 60.0 || h < 0.0 {
            return None;
        }
        days * 86400.0 + h * 3600.0 + m * 60.0 + sec
    };
    Some(if neg { -total } else { total })
}
