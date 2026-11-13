// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Conversions between numbers, text and dates, as Excel makes them.

/// A number as text, as Excel converts it in a formula (`=1/3&""`): 15
/// significant digits, no trailing zeros, and the exponent form outside
/// the range where plain digits are shown.
pub fn number_to_text(n: f64) -> String {
    if n == 0.0 || !n.is_finite() {
        return "0".into();
    }
    let rounded = round_significant(n, 15);
    let exponent = rounded.abs().log10().floor() as i32;
    // Plain digits while they take no more than 16 places (such as
    // 1000000000000000 or 0.000000000000001), the exponent form beyond.
    let plain = {
        let decimals = (14 - exponent).clamp(0, 340) as usize;
        let s = format!("{:.*}", decimals, rounded);
        if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s }
    };
    let digits = plain.bytes().filter(|b| b.is_ascii_digit()).count();
    if digits <= 17 || (exponent >= -5 && exponent <= 10) {
        return if plain == "-0" { "0".into() } else { plain };
    }
    let mantissa = rounded / 10f64.powi(exponent);
    let mut m = format!("{:.14}", mantissa);
    if m.contains('.') {
        m = m.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    // The mantissa may round up to 10.
    if m.trim_start_matches('-') == "10" {
        return format!("{}1E{}{:02}", if n < 0.0 { "-" } else { "" }, if exponent + 1 < 0 { "-" } else { "+" }, (exponent + 1).abs());
    }
    format!("{m}E{}{:02}", if exponent < 0 { "-" } else { "+" }, exponent.abs())
}

/// `n` rounded to `digits` significant digits.
pub fn round_significant(n: f64, digits: i32) -> f64 {
    if n == 0.0 || !n.is_finite() {
        return n;
    }
    let exponent = n.abs().log10().floor() as i32;
    let scale = digits - 1 - exponent;
    // Through text, which rounds the decimal digits exactly.
    let s = format!("{:.*e}", (digits - 1).max(0) as usize, n);
    let _ = scale;
    s.parse().unwrap_or(n)
}

/// Days from 1899-12-30 (serial 0 in Excel's 1900 date system, with its
/// 1900-02-29) to a date.
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    // Howard Hinnant's algorithm (days since 1970-01-01).
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 1970-01-01 as a 1900-system serial.
const UNIX_EPOCH_SERIAL: i64 = 25569;

/// A date as a serial number in the 1900 date system (1900-01-01 is 1,
/// and 1900-02-29, which did not exist, is 60) or the 1904 system.
pub fn serial_from_date(y: i64, m: i64, d: i64, date1904: bool) -> i64 {
    if !date1904 && y == 1900 && m == 2 && d == 29 {
        return 60;
    }
    let days = days_from_civil(y, m, d) + UNIX_EPOCH_SERIAL;
    if date1904 {
        return days - 1462;
    }
    // Before 1900-03-01 the serials are one less (the false leap day).
    if days < 61 { days - 1 } else { days }
}

/// A serial's date: (year, month, day). Serial 60 in the 1900 system is
/// 1900-02-29, and serial 0 is 1900-01-00, as Excel shows them.
pub fn date_from_serial(serial: i64, date1904: bool) -> (i64, u32, u32) {
    if date1904 {
        return civil_from_days(serial + 1462 - UNIX_EPOCH_SERIAL);
    }
    match serial {
        0 => (1900, 1, 0),
        60 => (1900, 2, 29),
        s if s < 60 => civil_from_days(s + 1 - UNIX_EPOCH_SERIAL),
        s => civil_from_days(s - UNIX_EPOCH_SERIAL),
    }
}

/// Day of the week, 0 = Sunday, of a 1900-system serial (Excel counts
/// serial 1, 1900-01-01, as a Sunday).
pub fn weekday(serial: i64) -> u32 {
    ((serial + 6).rem_euclid(7)) as u32
}

pub fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

static TODAY: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(i64::MIN);

/// Set today (days since 1970-01-01), for date filters (`今月`, …): the
/// app's local date, or a fixed day in tests. Without it, the system
/// clock's date in UTC is used where there is one.
pub fn set_today(days: i64) {
    TODAY.store(days, std::sync::atomic::Ordering::Relaxed);
}

/// The clock the app gives: milliseconds since 1970 (UTC) and the computer's
/// time zone's offset east of UTC in seconds. Without it (tests, tools),
/// today's date in UTC is used and the time of day is not known.
static CLOCK: std::sync::OnceLock<fn() -> (i64, i64)> = std::sync::OnceLock::new();

thread_local! {
    /// A clock for one thread, ahead of the app's: tests give a time they control.
    static THREAD_CLOCK: std::cell::Cell<Option<fn() -> (i64, i64)>> = const { std::cell::Cell::new(None) };
}

/// Give the calculator the local clock (once; the first one stays).
pub fn set_clock(now: fn() -> (i64, i64)) {
    let _ = CLOCK.set(now);
}

/// Give this thread its own clock (`None` takes it away), for tests.
pub fn set_thread_clock(now: Option<fn() -> (i64, i64)>) {
    THREAD_CLOCK.with(|c| c.set(now));
}

fn clock() -> Option<(i64, i64)> {
    THREAD_CLOCK.with(|c| c.get()).or_else(|| CLOCK.get().copied()).map(|f| f())
}

/// The file of the workbook being calculated (its whole path), for `CELL("filename")` and the sheet
/// names `CELL("address")` gives for another sheet's cell. The app sets it when it opens or saves a
/// workbook (the one it shows); `None` before it is saved.
static WORKBOOK_FILE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Tell the calculator which file the workbook is (`None`: not saved yet).
pub fn set_workbook_file(path: Option<String>) {
    if let Ok(mut file) = WORKBOOK_FILE.lock() {
        *file = path;
    }
}

thread_local! {
    /// The file being opened on this thread (see [`with_opening_file`]).
    static OPENING_FILE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Run `read` (which opens a workbook) with `path` as the file `CELL("filename")` sees on this thread, so that the
/// volatile cells calculated again on open (as Excel does, #177) know which file they are in. Other windows' files are
/// not touched (the setting is per thread).
pub fn with_opening_file<T>(path: Option<String>, read: impl FnOnce() -> T) -> T {
    let before = OPENING_FILE.with(|f| f.replace(path));
    let out = read();
    OPENING_FILE.with(|f| *f.borrow_mut() = before);
    out
}

/// The workbook's file path, if it has one.
pub fn workbook_file() -> Option<String> {
    if let Some(path) = OPENING_FILE.with(|f| f.borrow().clone()) {
        return Some(path);
    }
    WORKBOOK_FILE.lock().ok().and_then(|f| f.clone())
}

/// The serial number of a day counted from 1970-01-01.
fn serial_of_days(days: i64, date1904: bool) -> i64 {
    let (y, m, d) = civil_from_days(days);
    serial_from_date(y, m as i64, d as i64, date1904)
}

/// Today as a serial number, if known.
pub fn today_serial(date1904: bool) -> Option<i64> {
    let set = TODAY.load(std::sync::atomic::Ordering::Relaxed);
    let days = if set != i64::MIN {
        set
    } else if let Some((millis, offset)) = clock() {
        (millis.div_euclid(1000) + offset).div_euclid(86_400)
    } else {
        crate::clock::unix_days()?
    };
    Some(serial_of_days(days, date1904))
}

/// Excel's `NOW` ticks every 10 milliseconds (measured with Excel 16.0 build 20527: the values of
/// 200,000 calls in 0.1 s fell on 9 distinct values, all multiples of 0.01 s apart).
const NOW_TICK_MILLIS: i64 = 10;

/// Now as a serial number with the time of day (local time), for `NOW`: the clock's time at this call
/// (Excel reads the clock at each call, not once per calculation), in 10-millisecond steps.
/// Without a clock that knows the time, it is today's date alone.
pub fn now_serial(date1904: bool) -> Option<f64> {
    let set = TODAY.load(std::sync::atomic::Ordering::Relaxed);
    if set == i64::MIN {
        // One read of the clock gives both the day and the time of day.
        if let Some((millis, offset)) = clock() {
            let local = millis + offset * 1000;
            let in_day = local.rem_euclid(86_400_000) / NOW_TICK_MILLIS * NOW_TICK_MILLIS;
            return Some(serial_of_days(local.div_euclid(86_400_000), date1904) as f64 + in_day as f64 / 86_400_000.0);
        }
    }
    Some(today_serial(date1904)? as f64)
}

static CURRENT_YEAR: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// Set the year dates typed without one take (`9/30`). Without it, the
/// system clock's year (UTC) is used where there is one.
pub fn set_current_year(year: i64) {
    CURRENT_YEAR.store(year, std::sync::atomic::Ordering::Relaxed);
}

/// The year a date typed without one takes.
pub fn current_year() -> i64 {
    let set = CURRENT_YEAR.load(std::sync::atomic::Ordering::Relaxed);
    if set != 0 {
        return set;
    }
    if let Some((millis, offset)) = clock() {
        return civil_from_days((millis.div_euclid(1000) + offset).div_euclid(86_400)).0;
    }
    if let Some(days) = crate::clock::unix_days() {
        return civil_from_days(days).0;
    }
    2026
}

/// Text Excel reads as a number where a number is wanted: `1,234`,
/// `-1.5E3`, `50%`, `¥1,000`, `$5`, `(12)`, a date (`2026/9/30`,
/// `2026-09-30`, `9/30`), a time (`13:45`, `1:30:15 PM`), or a date and
/// time. `None` if it is not a number.
pub fn parse_number_text(text: &str, date1904: bool) -> Option<f64> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if let Some(n) = parse_plain_number(t) {
        return Some(n);
    }
    parse_date_time(t, date1904)
}

/// Truncate digits after the 15th significant digit, matching Excel's 15-digit precision rule.
/// Digits beyond 15 in the integer part become '0'; digits beyond 15 in the fraction are dropped.
pub fn truncate_to_15_digits(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut sig_count = 0;
    let mut seen_sig = false;
    let mut in_exponent = false;
    let mut seen_dot = false;

    for c in s.chars() {
        if c == 'e' || c == 'E' {
            in_exponent = true;
            result.push(c);
            continue;
        }
        if in_exponent {
            result.push(c);
            continue;
        }
        if c == '.' {
            seen_dot = true;
            result.push(c);
            continue;
        }
        if c.is_ascii_digit() {
            if !seen_sig && c != '0' {
                seen_sig = true;
            }
            if seen_sig {
                sig_count += 1;
                if sig_count > 15 {
                    if !seen_dot {
                        result.push('0');
                    }
                    continue;
                }
            }
            result.push(c);
        } else {
            result.push(c);
        }
    }
    result
}

fn parse_plain_number(t: &str) -> Option<f64> {
    let mut s = t.trim().to_string();
    let mut negative = false;
    if s.starts_with('(') && s.ends_with(')') {
        negative = true;
        s = s[1..s.len() - 1].trim().to_string();
    }
    let mut percent = 0;
    while s.ends_with('%') {
        percent += 1;
        s.pop();
        s = s.trim_end().to_string();
    }
    let mut sign = |s: &mut String| {
        if let Some(rest) = s.strip_prefix('-') {
            negative = !negative;
            *s = rest.trim_start().to_string();
        } else if let Some(rest) = s.strip_prefix('+') {
            *s = rest.trim_start().to_string();
        }
    };
    sign(&mut s);
    for currency in ["¥", "￥", "$", "\\"] {
        if let Some(rest) = s.strip_prefix(currency) {
            s = rest.trim_start().to_string();
            sign(&mut s);
            break;
        }
    }
    if s.is_empty() {
        return None;
    }
    // Thousands separators: groups of three digits before the point.
    let (int_part, rest) = match s.find(['.', 'e', 'E']) {
        Some(i) => (&s[..i], &s[i..]),
        None => (s.as_str(), ""),
    };
    if int_part.contains(',') {
        let groups: Vec<&str> = int_part.split(',').collect();
        if groups[0].is_empty() || groups[0].len() > 3 || groups[1..].iter().any(|g| g.len() != 3) {
            return None;
        }
    }
    let cleaned: String = format!("{}{}", int_part.replace(',', ""), rest);
    if !cleaned.bytes().all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-')) {
        return None;
    }
    if !cleaned.bytes().next().is_some_and(|b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    let truncated = truncate_to_15_digits(&cleaned);
    let mut n: f64 = truncated.parse().ok()?;
    if negative {
        n = -n;
    }
    for _ in 0..percent {
        n /= 100.0;
    }
    n.is_finite().then_some(n)
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

fn month_name(s: &str) -> Option<u32> {
    let l = s.to_ascii_lowercase();
    if l.len() < 3 {
        return None;
    }
    let full = ["january", "february", "march", "april", "may", "june", "july", "august", "september", "october", "november", "december"];
    for (i, m) in MONTHS.iter().enumerate() {
        if l == *m || l == full[i] || (l.len() >= 3 && full[i].starts_with(&l) && l.starts_with(m)) {
            return Some(i as u32 + 1);
        }
    }
    None
}

/// A date, a time, or both, in the Japanese locale's forms.
pub fn parse_date_time(t: &str, date1904: bool) -> Option<f64> {
    let t = t.trim();
    if date1904 && let Some(rest) = t.strip_prefix('-') {
        let v = parse_date_time(rest, date1904)?;
        return Some(-v);
    }
    // Split off a time: the part with ':' or AM/PM.
    let (date_part, time_part) = split_date_time(t);
    let time = match time_part {
        Some(tp) => Some(parse_time(tp)?),
        None => None,
    };
    let date = match date_part {
        Some(dp) if !dp.is_empty() => Some(parse_date(dp, date1904)?),
        _ => None,
    };
    match (date, time) {
        (None, None) => None,
        (d, t) => Some(d.unwrap_or(0.0) + t.unwrap_or(0.0)),
    }
}

fn split_date_time(t: &str) -> (Option<&str>, Option<&str>) {
    let has_time = |s: &str| s.contains(':') || s.to_ascii_uppercase().ends_with("AM") || s.to_ascii_uppercase().ends_with("PM") || s.contains('時');
    if !has_time(t) {
        return (Some(t), None);
    }
    // The time starts at the first token with ':' (or a number before AM/PM).
    let parts: Vec<&str> = t.split(' ').filter(|p| !p.is_empty()).collect();
    if let Some(i) = parts.iter().position(|p| p.contains(':') || p.contains('時')) {
        let date = parts[..i].join(" ");
        let start = t.find(parts[i]).unwrap_or(0);
        return (if date.is_empty() { None } else { Some(&t[..t[..start].trim_end().len()]) }, Some(&t[start..]));
    }
    // "3 PM"
    if parts.len() >= 2 {
        let last = parts[parts.len() - 1].to_ascii_uppercase();
        if last == "AM" || last == "PM" {
            let i = parts.len() - 2;
            let start = t.rfind(parts[i]).unwrap_or(0);
            let date = t[..start].trim_end();
            return (if date.is_empty() { None } else { Some(date) }, Some(&t[start..]));
        }
    }
    (Some(t), None)
}

fn parse_time(t: &str) -> Option<f64> {
    let mut s = t.trim().to_string();
    let upper = s.to_ascii_uppercase();
    let mut pm = None;
    if upper.ends_with("AM") || upper.ends_with("PM") {
        pm = Some(upper.ends_with("PM"));
        s.truncate(s.len() - 2);
        s = s.trim_end().to_string();
    } else if upper.ends_with('A') || upper.ends_with('P') {
        pm = Some(upper.ends_with('P'));
        s.truncate(s.len() - 1);
        s = s.trim_end().to_string();
    }
    let s = s.replace('時', ":").replace('分', ":").replace('秒', "");
    let s = s.trim_end_matches(':');
    let parts: Vec<&str> = s.split(':').collect();
    if parts.is_empty() || parts.len() > 3 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    let h: f64 = parts[0].parse().ok()?;
    let m: f64 = parts.get(1).map_or(Some(0.0), |p| p.parse().ok())?;
    let sec: f64 = parts.get(2).map_or(Some(0.0), |p| p.parse().ok())?;
    if !parts[0].bytes().all(|b| b.is_ascii_digit()) || parts.get(1).is_some_and(|p| !p.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    if m >= 60.0 || sec >= 60.0 || h < 0.0 {
        return None;
    }
    let mut h = h;
    if let Some(pm) = pm {
        if h > 12.0 || h == 0.0 && parts.len() < 2 {
            return None;
        }
        if h == 12.0 {
            h = 0.0;
        }
        if pm {
            h += 12.0;
        }
    } else if parts.len() == 1 {
        return None;
    }
    if h >= 10000.0 {
        return None;
    }
    Some((h * 3600.0 + m * 60.0 + sec) / 86400.0)
}

fn normalize_era(t: &str) -> Option<String> {
    const ERAS: &[(&str, i64)] = &[
        ("令和", 2018), ("平成", 1988), ("昭和", 1925), ("大正", 1911), ("明治", 1867),
        ("R", 2018), ("r", 2018), ("H", 1988), ("h", 1988),
        ("S", 1925), ("s", 1925), ("T", 1911), ("t", 1911),
        ("M", 1867), ("m", 1867),
    ];
    for &(prefix, base) in ERAS {
        if let Some(rest) = t.strip_prefix(prefix) {
            let num_len = rest.chars().take_while(|c| c.is_ascii_digit()).count();
            if num_len > 0 {
                let era_year: i64 = rest[..num_len].parse().ok()?;
                let full_year = base + era_year;
                return Some(format!("{}{}", full_year, &rest[num_len..]));
            }
        }
    }
    None
}

fn parse_date(t: &str, date1904: bool) -> Option<f64> {
    let t = t.trim();
    if let Some(normalized) = normalize_era(t) {
        return parse_date(&normalized, date1904);
    }
    // 2026年9月30日, 9月30日
    if t.contains('年') || t.contains('月') {
        let s = t.replace('年', "/").replace('月', "/").replace('日', "");
        return parse_date(s.trim_end_matches('/'), date1904);
    }
    let s_clean;
    let t = if t.contains(',') {
        s_clean = t.replace(',', " ");
        if s_clean.split_whitespace().any(|p| month_name(p).is_some()) {
            &s_clean
        } else {
            t
        }
    } else {
        t
    };
    let seps: &[char] = &['/', '-', ' ', '.'];
    let parts: Vec<&str> = t.split(|c| seps.contains(&c)).filter(|p| !p.is_empty()).collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let num = |s: &str| -> Option<i64> { if s.bytes().all(|b| b.is_ascii_digit()) && s.len() <= 4 { s.parse().ok() } else { None } };
    let (y, m, d): (i64, i64, i64);
    let current_year = current_year();
    if parts.len() == 3 {
        if let (Some(a), Some(b), Some(c)) = (num(parts[0]), num(parts[1]), num(parts[2])) {
            if parts[0].len() >= 3 || a > 31 {
                (y, m, d) = (a, b, c);
            } else {
                // m/d/y in the English form? Japanese locale: y/m/d first.
                (y, m, d) = (a, b, c);
            }
        } else if let (Some(a), Some(mo), Some(c)) = (num(parts[0]), month_name(parts[1]), num(parts[2])) {
            (y, m, d) = (c, mo as i64, a); // 30-Sep-2026
        } else if let (Some(mo), Some(b), Some(c)) = (month_name(parts[0]), num(parts[1]), num(parts[2])) {
            (y, m, d) = (c, mo as i64, b); // Sep 30 2026
        } else {
            return None;
        }
    } else if let (Some(a), Some(b)) = (num(parts[0]), num(parts[1])) {
        if parts[0].len() >= 3 || a > 12 {
            // 2026/9: year and month, the first day.
            if b > 12 || b < 1 {
                return None;
            }
            (y, m, d) = (a, b, 1);
        } else {
            (y, m, d) = (current_year, a, b);
        }
    } else if let (Some(a), Some(mo)) = (num(parts[0]), month_name(parts[1])) {
        if a > 31 {
            (y, m, d) = (a, mo as i64, 1);
        } else {
            (y, m, d) = (current_year, mo as i64, a);
        }
    } else if let (Some(mo), Some(b)) = (month_name(parts[0]), num(parts[1])) {
        if b > 31 {
            (y, m, d) = (b, mo as i64, 1);
        } else {
            (y, m, d) = (current_year, mo as i64, b);
        }
    } else {
        return None;
    }
    let y = if y < 30 && parts[0].len() <= 2 && parts.len() == 3 { 2000 + y } else if y < 100 && parts.len() == 3 && parts[0].len() <= 2 { 1900 + y } else { y };
    let max_d = if !date1904 && y == 1900 && m == 2 {
        29
    } else {
        days_in_month(y, m as u32) as i64
    };
    if !(1..=12).contains(&m) || d < 1 || d > max_d || !(1900..=9999).contains(&y) {
        return None;
    }
    Some(serial_from_date(y, m, d, date1904) as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_as_text() {
        assert_eq!(number_to_text(1.0 / 3.0), "0.333333333333333");
        assert_eq!(number_to_text(0.1 + 0.2), "0.3");
        assert_eq!(number_to_text(123456789012345.0), "123456789012345");
        assert_eq!(number_to_text(1e15), "1000000000000000");
        assert_eq!(number_to_text(5.95987596448829e-6), "5.95987596448829E-06");
        assert_eq!(number_to_text(-2.5), "-2.5");
        assert_eq!(number_to_text(1e-15), "0.000000000000001");
        assert_eq!(number_to_text(0.0000001), "0.0000001");
        assert_eq!(number_to_text(12345678901234567890.0), "1.23456789012346E+19");
    }

    #[test]
    fn serials() {
        assert_eq!(serial_from_date(1900, 1, 1, false), 1);
        assert_eq!(serial_from_date(1900, 3, 1, false), 61);
        assert_eq!(serial_from_date(2026, 9, 30, false), 46295);
        assert_eq!(date_from_serial(60, false), (1900, 2, 29));
        assert_eq!(date_from_serial(59, false), (1900, 2, 28));
        assert_eq!(date_from_serial(46295, false), (2026, 9, 30));
        assert_eq!(date_from_serial(0, true), (1904, 1, 1));
        assert_eq!(weekday(1), 0);
        assert_eq!(weekday(46295), 3); // Wednesday
    }

    #[test]
    fn text_as_numbers() {
        assert_eq!(parse_number_text("1,234.5", false), Some(1234.5));
        assert_eq!(parse_number_text("50%", false), Some(0.5));
        assert_eq!(parse_number_text("¥1,000", false), Some(1000.0));
        assert_eq!(parse_number_text("(12)", false), Some(-12.0));
        assert_eq!(parse_number_text("1e3", false), Some(1000.0));
        assert_eq!(parse_number_text("abc", false), None);
        assert_eq!(parse_number_text("1,23", false), None);
        assert_eq!(parse_number_text("2026/9/30", false), Some(46295.0));
        assert_eq!(parse_number_text("12:00", false), Some(0.5));
        assert_eq!(parse_number_text("2026/9/30 18:00", false), Some(46295.75));
        assert_eq!(parse_number_text("1:30 PM", false), Some((13.5) / 24.0));
        assert_eq!(parse_number_text("-12:00", true), Some(-0.5));
        assert_eq!(parse_number_text("-1904/01/02", true), Some(-1.0));
        assert_eq!(parse_number_text("-12:00", false), None);
    }

    #[test]
    fn long_numbers_15_digits() {
        // 15 digits intact
        assert_eq!(parse_number_text("123456789012345", false), Some(123456789012345.0));
        // 16 digits: 16th digit truncated to 0
        assert_eq!(parse_number_text("1234567890123456", false), Some(1234567890123450.0));
        // 19 digits: digits 16..19 truncated to 0
        assert_eq!(parse_number_text("1234567890123456789", false), Some(1234567890123450000.0));
        // 16 digits in fraction: 16th digit dropped
        assert_eq!(parse_number_text("1.234567890123456", false), Some(1.23456789012345));
        // Leading zeros don't count towards significant digits
        assert_eq!(parse_number_text("0.001234567890123456", false), Some(0.00123456789012345));
        // Scientific notation with 16 digits mantissa
        assert_eq!(parse_number_text("1.234567890123456E+10", false), Some(1.23456789012345E+10));
    }
}
