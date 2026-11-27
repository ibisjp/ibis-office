// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What depends on the culture, as Microsoft 365 Excel's Power Query on Windows gave it (`Culture.Current` = ja-JP; the values are those of
//! `tasks/doc/requests/2026-11-17-k0039-m-semantics-result.md`, sections 6 and 8, and of the seven-culture table of
//! `2026-11-20-k0039-locales-result.tsv`, copied to `tests/data/` and run by `tests_locales.rs`): how numbers, dates, times and durations are
//! written as text, the names of the days and months, and how a text is read as a number or a date. The data of a culture is in
//! `culture_data.rs` (ja-JP, en-US, zh-CN, de-DE, fr-FR, en-GB, th-TH and the invariant culture); a name that is not one of them is the
//! current culture, as in Excel.
use crate::culture_data::{self as data, Data, DateOrder};
use crate::value::days_from_civil;

/// A culture of the ones this interpreter holds the data of (the seven Excel was measured in, and the invariant culture).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Culture {
    Ja,
    En,
    Zh,
    De,
    Fr,
    EnGb,
    Th,
    Invariant,
}

impl Culture {
    /// The culture of a name, as Excel's Power Query reads it: any case, `-` or `_` between the language and the country (`en_US` is en-US),
    /// the empty name is the invariant culture. A name it does not know is `None`: the call then uses the current culture (Excel does not
    /// refuse `xx-XX` or `zz`; it gives the result of ja-JP, the current culture of the measurement).
    pub fn parse(name: &str) -> Option<Culture> {
        let n = name.replace('_', "-").to_ascii_lowercase();
        Some(match n.as_str() {
            "" => Culture::Invariant,
            "ja-jp" | "ja" => Culture::Ja,
            "en-us" | "en" => Culture::En,
            "zh-cn" => Culture::Zh,
            "de-de" => Culture::De,
            "fr-fr" => Culture::Fr,
            "en-gb" => Culture::EnGb,
            "th-th" => Culture::Th,
            _ => return None,
        })
    }

    pub fn data(self) -> &'static Data {
        match self {
            Culture::Ja => &data::JA,
            Culture::En => &data::EN,
            Culture::Zh => &data::ZH,
            Culture::De => &data::DE,
            Culture::Fr => &data::FR,
            Culture::EnGb => &data::GB,
            Culture::Th => &data::TH,
            Culture::Invariant => &data::INVARIANT,
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

/// `number_text` with the culture's decimal separator (`Text.From(1234.5, "de-DE")` is `1234,5`).
pub fn number_text_in(n: f64, c: Culture) -> String {
    let t = number_text(n);
    match c.data().decimal {
        "." => t,
        d => t.replace('.', d),
    }
}

/// A number read from text in the culture: blanks around are allowed, a sign, digits, the culture's group mark between digits of the integer
/// part (its places are not checked: `1234,5` is 12345 in ja-JP), its decimal mark, an exponent, and a `%` at the end (the number divided by
/// 100: `12,5%` is 1.25 in ja-JP, 0.125 in de-DE). Another group mark (`1 234,5` with U+00A0), full-width digits, `0x10`, `-` and the empty text
/// are not numbers. As Excel read them in the seven cultures (`tests/data/k0039-locales-result.tsv`, sections 3 and 5).
pub fn parse_number_in(s: &str, c: Culture) -> Option<f64> {
    let d = c.data();
    let t = s.trim();
    let (t, percent) = match t.strip_suffix('%') {
        Some(rest) => (rest.trim_end(), true),
        None => (t, false),
    };
    let (sign, rest) = match t.chars().next()? {
        '+' => (1.0, &t[1..]),
        '-' => (-1.0, &t[1..]),
        _ => (1.0, t),
    };
    let (decimal, group) = (d.decimal.chars().next()?, d.group.chars().next()?);
    // The mantissa and the exponent.
    let (mantissa, exp) = match rest.find(['e', 'E']) {
        Some(i) => (&rest[..i], Some(&rest[i + 1..])),
        None => (rest, None),
    };
    let (int_part, frac_part) = match mantissa.find(decimal) {
        Some(i) => (&mantissa[..i], Some(&mantissa[i + decimal.len_utf8()..])),
        None => (mantissa, None),
    };
    let ic: Vec<char> = int_part.chars().collect();
    for (i, &ch) in ic.iter().enumerate() {
        let ok = ch.is_ascii_digit() || (ch == group && i > 0 && i + 1 < ic.len() && ic[i - 1].is_ascii_digit() && ic[i + 1].is_ascii_digit());
        if !ok {
            return None;
        }
    }
    if let Some(f) = frac_part {
        if !f.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
    }
    if int_part.is_empty() && frac_part.is_none_or(str::is_empty) {
        return None;
    }
    let mut clean: String = int_part.chars().filter(|ch| ch.is_ascii_digit()).collect();
    if let Some(f) = frac_part {
        clean.push('.');
        clean.push_str(f);
    }
    if let Some(e) = exp {
        let digits = e.strip_prefix(['+', '-']).unwrap_or(e);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        clean.push('e');
        clean.push_str(e);
    }
    let v = clean.parse::<f64>().ok()?;
    Some(sign * if percent { v / 100.0 } else { v })
}

/// `parse_number_in` in ja-JP.
pub fn parse_number(s: &str) -> Option<f64> {
    parse_number_in(s, Culture::Ja)
}

pub const DAY_NAMES_JA: [&str; 7] = ["日曜日", "月曜日", "火曜日", "水曜日", "木曜日", "金曜日", "土曜日"];
pub const DAY_NAMES_EN: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// The full name of a day (0 = Sunday) in the culture.
pub fn day_name(dow: usize, c: Culture) -> String {
    c.data().days[dow % 7].to_string()
}

/// The full name of a month (1 to 12) in the culture.
pub fn month_name(m: i64, c: Culture) -> String {
    c.data().months[((m - 1).rem_euclid(12)) as usize].to_string()
}

/// The text of a date, a time, a datetime as `Text.From` writes them: the culture's short date, short time and short date with long time.
pub fn date_text(day: i64, c: Culture) -> String {
    crate::format::format_standard(day, 0.0, 'd', c)
}

pub fn time_text(secs: f64, c: Culture) -> String {
    crate::format::format_standard(0, secs, 't', c)
}

pub fn datetime_text(day: i64, secs: f64, c: Culture) -> String {
    crate::format::format_standard(day, secs, 'G', c)
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

const EN_MONTHS: [&str; 12] = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];

/// The month a word names: the culture's own names (full or abbreviated, the abbreviation's `.` left off), and always the English ones
/// (full, three letters, `Sept`): `17 November 2026` is read in every culture, `17 novembre 2026` only in fr-FR (as Excel read them).
fn month_of(word: &str, c: Culture) -> Option<i64> {
    let w = word.to_lowercase();
    let own = c.data();
    let find = |names: &[&str; 12], abbr: bool| names.iter().position(|m| {
        let m = m.to_lowercase();
        let m = if abbr { m.trim_end_matches('.') } else { m.as_str() };
        !m.is_empty() && m == w
    });
    find(&own.months, false)
        .or_else(|| find(&own.months_abbr, true))
        .or_else(|| EN_MONTHS.iter().position(|m| *m == w || (w.len() == 3 && m.starts_with(&w)) || (w == "sept" && *m == "september")))
        .map(|i| i as i64 + 1)
}

fn valid(y: i64, m: i64, d: i64) -> Option<i64> {
    ((1..=9999).contains(&y) && (1..=12).contains(&m) && d >= 1 && d <= crate::builtin::days_in_month(y, m)).then(|| days_from_civil(y, m, d))
}

#[derive(PartialEq, Debug)]
enum Tok {
    Num(String),
    Word(String),
    Other(char),
}

fn is_word_char(c: char) -> bool {
    c.is_alphabetic() || matches!(c, '\u{0300}'..='\u{036F}' | '\u{0E31}'..='\u{0E3A}' | '\u{0E47}'..='\u{0E4E}')
}

/// Runs of digits, runs of letters (with the combining marks of the Thai names), and single other characters.
fn tokens(s: &str) -> Vec<Tok> {
    let mut out: Vec<Tok> = Vec::new();
    for c in s.chars() {
        match out.last_mut() {
            Some(Tok::Num(n)) if c.is_ascii_digit() => n.push(c),
            Some(Tok::Word(w)) if is_word_char(c) => w.push(c),
            _ if c.is_ascii_digit() => out.push(Tok::Num(c.to_string())),
            _ if is_word_char(c) => out.push(Tok::Word(c.to_string())),
            _ => out.push(Tok::Other(c)),
        }
    }
    out
}

/// A year as written: four digits are the year of the culture's calendar as they are; one or two digits are in the 100 years that end at the
/// culture's `TwoDigitYearMax`; the Thai calendar's year is the Gregorian year + 543. `literal`: the year is Gregorian as written.
fn year_of(text: &str, c: Culture, literal: bool) -> Option<i64> {
    let y: i64 = text.parse().ok()?;
    if text.len() > 4 {
        return None;
    }
    let d = c.data();
    let y = if text.len() <= 2 {
        let top = d.two_digit_year_max;
        let full = top / 100 * 100 + y;
        if full > top { full - 100 } else { full }
    } else {
        y
    };
    Some(if d.buddhist && !literal { y - 543 } else { y })
}

/// A date read from text. `date_target`: the target is a date (`Date.FromText`), not a datetime: then `yyyy-MM-dd` is the Gregorian year as
/// written even in th-TH (`2569-11-17` is year 2569 for a date; for a datetime every year is the Thai calendar's, and `2026-11-17` is 1483).
///
/// Read, as Excel read them in the seven cultures (`tests/data/k0039-locales-result.tsv`, section 4): `yyyy-MM-dd` and `yyyy/MM/dd` in every
/// culture; the numbers `a/b/c` (also with `-` or `.`) in the culture's order (`DateOrder`: ja-JP, zh-CN year first, or month first when the
/// year has four digits; en-US month first; de-DE, fr-FR, en-GB, th-TH day first); `2026年11月17日`; a day, a month name and a year
/// (`17 November 2026`, `17-Nov-2026`), a month name, a day and a year (`Nov 17, 2026`) with the names of the culture and the English ones;
/// `20261117`. Full-width digits and a text with a time are not dates.
pub fn parse_date_in(s: &str, c: Culture, date_target: bool) -> Option<i64> {
    let t = s.trim();
    if t.len() == 8 && t.bytes().all(|b| b.is_ascii_digit()) {
        return valid(t[..4].parse().ok()?, t[4..6].parse().ok()?, t[6..].parse().ok()?);
    }
    let p = tokens(t);
    let num = |i: usize| -> Option<&str> {
        match p.get(i)? {
            Tok::Num(n) => Some(n.as_str()),
            _ => None,
        }
    };
    let other = |i: usize| -> Option<char> {
        match p.get(i)? {
            Tok::Other(c) => Some(*c),
            _ => None,
        }
    };
    // 2026年11月17日
    if p.len() == 6 && other(1).is_none() && matches!(&p[1], Tok::Word(w) if w == "年") && matches!(&p[3], Tok::Word(w) if w == "月") && matches!(&p[5], Tok::Word(w) if w == "日") {
        return valid(year_of(num(0)?, c, false)?, num(2)?.parse().ok()?, num(4)?.parse().ok()?);
    }
    // a/b/c
    if let (5, Some(a), Some(b), Some(d), Some(sep)) = (p.len(), num(0), num(2), num(4), other(1)) {
        if !matches!(sep, '/' | '-' | '.') || other(3) != Some(sep) {
            return None;
        }
        let order = c.data().date_order;
        let n = |x: &str| x.parse::<i64>().ok();
        return if a.len() == 4 {
            valid(year_of(a, c, date_target && sep == '-')?, n(b)?, n(d)?)
        } else {
            match order {
                DateOrder::YearFirst if d.len() <= 2 => valid(year_of(a, c, false)?, n(b)?, n(d)?),
                DateOrder::YearFirst if d.len() == 4 => valid(year_of(d, c, false)?, n(a)?, n(b)?),
                DateOrder::YearFirst => None,
                DateOrder::MonthFirst => valid(year_of(d, c, false)?, n(a)?, n(b)?),
                DateOrder::DayFirst => valid(year_of(d, c, false)?, n(b)?, n(a)?),
            }
        };
    }
    // With a month's name: the blanks, commas, dots and dashes between the words and numbers are dropped.
    let q: Vec<&Tok> = p.iter().filter(|x| !matches!(x, Tok::Other(' ' | ',' | '.' | '-'))).collect();
    match q.as_slice() {
        [Tok::Num(d), Tok::Word(m), Tok::Num(y)] => valid(year_of(y, c, false)?, month_of(m, c)?, d.parse().ok()?),
        [Tok::Word(m), Tok::Num(d), Tok::Num(y)] => valid(year_of(y, c, false)?, month_of(m, c)?, d.parse().ok()?),
        _ => None,
    }
}

/// A date from text, for a date (see `parse_date_in`), in ja-JP.
pub fn parse_date(s: &str) -> Option<i64> {
    parse_date_in(s, Culture::Ja, true)
}

/// A datetime from text: a date as `parse_date_in` reads it, alone, or followed by `T` or a blank and a time as `parse_time` reads it with no
/// offset (`2026-11-17 13:05:09`, `17.11.2026 13:05:09` in de-DE, `11/17/2026 1:05:09 PM` in ja-JP and en-US).
pub fn parse_datetime_in(s: &str, c: Culture) -> Option<(i64, f64)> {
    let t = s.trim();
    if let Some(d) = parse_date_in(t, c, false) {
        return Some((d, 0.0));
    }
    for (at, ch) in t.char_indices() {
        if ch != 'T' && ch != ' ' {
            continue;
        }
        let (left, right) = (&t[..at], t[at + 1..].trim());
        if right.contains('+') || right.contains('-') || right.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if let (Some(d), Some(secs)) = (parse_date_in(left, c, false), parse_time(right, c)) {
            return Some((d, secs));
        }
    }
    None
}

/// `parse_datetime_in` in ja-JP.
pub fn parse_datetime(s: &str) -> Option<(i64, f64)> {
    parse_datetime_in(s, Culture::Ja)
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
/// `13:05`, `13:05:09.5`, `13:5:9`, `1305`, `13時05分09秒`, blanks around, an offset (`+09:00`, dropped); `午後 1:05`, `1:05:09 午後` with the culture's own designators (ja-JP 午前 午後, zh-CN 上午 下午). A time past 23:59:59 and a leading `T` are not times.
pub fn parse_time(s: &str, c: Culture) -> Option<f64> {
    let mut t = s.trim().to_string();
    let mut pm = false;
    let mut am = false;
    // The culture's own designators (午前 午後 in ja-JP, 上午 下午 in zh-CN; de-DE and fr-FR have none) and always the English AM and PM in
    // any case (`1:05 pm`).
    let own = c.data();
    for (word, is_pm) in [(own.pm, true), (own.am, false)] {
        if !word.is_empty() && word != "AM" && word != "PM" && t.contains(word) {
            t = t.replace(word, " ");
            if is_pm { pm = true } else { am = true }
        }
    }
    for (word, is_pm) in [("PM", true), ("AM", false)] {
        let upper = t.to_ascii_uppercase();
        if let Some(at) = upper.find(word) {
            t.replace_range(at..at + 2, " ");
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
