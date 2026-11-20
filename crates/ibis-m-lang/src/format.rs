// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The format strings of `Date.ToText`, `DateTime.ToText`, `Time.ToText`, `Number.ToText` and `Text.Format`, as Excel's Power Query reads them
//! (measured: `tasks/doc/requests/2026-11-17-k0039-m-semantics-2-result.md`, sections 12-15). A format it does not take is an error, as
//! Excel's is (`h`, `H`, `z`, `K`, `e`, `a` alone are not formats of a date or a datetime).
use crate::culture::{self, Culture};
use crate::error::MError;
use crate::value::civil_from_days;

fn unsupported_format(what: &str) -> MError {
    MError::expr(format!("The specified output {what} format is not supported."))
}

/// A date and time to be written: the day (days since 0001-01-01) and the seconds of the day.
#[derive(Clone, Copy)]
pub struct Moment {
    pub day: i64,
    pub secs: f64,
}

fn weekday_index(day: i64) -> usize {
    (day + 1).rem_euclid(7) as usize
}

/// The pattern a one-character standard format stands for in the culture.
fn standard(ch: char, c: Culture) -> Option<&'static str> {
    Some(match (ch, c) {
        ('d', Culture::Ja) => "yyyy/MM/dd",
        ('d', Culture::En) => "M/d/yyyy",
        ('D', Culture::Ja) => "yyyy年M月d日",
        ('D', Culture::En) => "dddd, MMMM d, yyyy",
        ('M' | 'm', Culture::Ja) => "M月d日",
        ('M' | 'm', Culture::En) => "MMMM d",
        ('y' | 'Y', Culture::Ja) => "yyyy年M月",
        ('y' | 'Y', Culture::En) => "MMMM yyyy",
        ('t', Culture::Ja) => "H:mm",
        ('t', Culture::En) => "h:mm tt",
        ('T', Culture::Ja) => "H:mm:ss",
        ('T', Culture::En) => "h:mm:ss tt",
        ('G', Culture::Ja) => "yyyy/MM/dd H:mm:ss",
        ('G', Culture::En) => "M/d/yyyy h:mm:ss tt",
        ('g', Culture::Ja) => "yyyy/MM/dd H:mm",
        ('g', Culture::En) => "M/d/yyyy h:mm tt",
        ('s', _) => "yyyy-MM-dd'T'HH:mm:ss",
        ('o' | 'O', _) => "yyyy-MM-dd'T'HH:mm:ss.fffffff",
        _ => return None,
    })
}

/// A moment written by a format: a one-character standard format, or a custom pattern (`yyyy-MM-dd HH:mm:ss`, quotes and `\` for text).
pub fn format_moment(m: Moment, format: &str, c: Culture, what: &str) -> Result<String, MError> {
    let chars: Vec<char> = format.chars().collect();
    if chars.len() == 1 {
        return match standard(chars[0], c) {
            Some(p) => custom(m, p, c, what, false),
            None => Err(unsupported_format(what)),
        };
    }
    custom(m, format, c, what, true)
}

/// `strict`: a user's pattern (a single `h` or `H` is not a format of Excel's); a standard format's own pattern may have one.
fn custom(m: Moment, pattern: &str, c: Culture, what: &str, strict: bool) -> Result<String, MError> {
    let (y, mo, d) = civil_from_days(m.day);
    let total = m.secs.floor() as i64;
    let (h, mi, s) = (total / 3600, total % 3600 / 60, total % 60);
    let frac = ((m.secs - m.secs.floor()) * 1e7).round() as i64;
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let mut run = 1;
        while i + run < chars.len() && chars[i + run] == ch {
            run += 1;
        }
        match ch {
            '\'' => {
                // A quoted text up to the closing quote.
                let mut j = i + 1;
                while j < chars.len() && chars[j] != '\'' {
                    out.push(chars[j]);
                    j += 1;
                }
                i = j + 1;
                continue;
            }
            '"' => {
                // M's own quote: a doubled one inside text; here `""text""` is the text.
                let mut j = i + run;
                while j < chars.len() && chars[j] != '"' {
                    out.push(chars[j]);
                    j += 1;
                }
                i = j + run.min(chars.len() - j).max(1);
                continue;
            }
            '\\' => {
                if let Some(&next) = chars.get(i + 1) {
                    out.push(next);
                }
                i += 2;
                continue;
            }
            'y' => out.push_str(&match run {
                1 | 2 => format!("{:02}", y % 100),
                n => format!("{y:0width$}", width = n.max(4)),
            }),
            'M' => match run {
                1 => out.push_str(&mo.to_string()),
                2 => out.push_str(&format!("{mo:02}")),
                3 => out.push_str(&match c {
                    Culture::Ja => mo.to_string(),
                    Culture::En => culture::month_name(mo, c)[..3].to_string(),
                }),
                _ => out.push_str(&culture::month_name(mo, c)),
            },
            'd' => match run {
                1 => out.push_str(&d.to_string()),
                2 => out.push_str(&format!("{d:02}")),
                3 => out.push_str(&match c {
                    Culture::Ja => culture::day_name(weekday_index(m.day), c).chars().next().map(String::from).unwrap_or_default(),
                    Culture::En => culture::day_name(weekday_index(m.day), c)[..3].to_string(),
                }),
                _ => out.push_str(&culture::day_name(weekday_index(m.day), c)),
            },
            'H' | 'h' if run == 1 && strict => return Err(unsupported_format(what)),
            'H' => out.push_str(&if run == 1 { h.to_string() } else { format!("{h:02}") }),
            'h' => {
                let h12 = if h % 12 == 0 { 12 } else { h % 12 };
                out.push_str(&if run == 1 { h12.to_string() } else { format!("{h12:02}") })
            }
            'm' => out.push_str(&if run == 1 { mi.to_string() } else { format!("{mi:02}") }),
            's' => out.push_str(&if run == 1 { s.to_string() } else { format!("{s:02}") }),
            'f' | 'F' => {
                let digits = format!("{frac:07}");
                let n = run.min(7);
                let text = &digits[..n];
                out.push_str(if ch == 'F' { text.trim_end_matches('0') } else { text });
            }
            't' if run >= 2 => out.push_str(match (c, h < 12) {
                (Culture::Ja, true) => "午前",
                (Culture::Ja, false) => "午後",
                (Culture::En, true) => "AM",
                (Culture::En, false) => "PM",
            }),
            'g' if run >= 2 => out.push_str(match c {
                Culture::Ja => "西暦",
                Culture::En => "A.D.",
            }),
            'z' | 'K' | 'e' | 'a' | 't' | 'g' => return Err(unsupported_format(what)),
            other => {
                for _ in 0..run {
                    out.push(other);
                }
            }
        }
        i += run;
    }
    Ok(out)
}

/// The decimal digits of a number as `Text.From` knows them (the shortest that read back the same) and the decimal exponent of the first:
/// the value is `0.d1d2d3… × 10^(point)`.
fn digits_of(a: f64) -> (Vec<u8>, i32) {
    if a == 0.0 {
        return (vec![0], 1);
    }
    let sci = {
        let s15 = format!("{:.14e}", a);
        if s15.parse::<f64>().ok() == Some(a) { s15 } else { format!("{:.16e}", a) }
    };
    let (mant, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let mut digits: Vec<u8> = mant.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect();
    while digits.len() > 1 && digits.last() == Some(&0) {
        digits.pop();
    }
    (digits, exp.parse::<i32>().unwrap_or(0) + 1)
}

/// `digits` rounded to `decimals` places after the point (half away from zero, on the digits: 0.125 is 0.13, 2.675 is 2.68, 1.005 is 1.01);
/// the integer part and the fraction part as text.
fn fixed(a: f64, decimals: usize) -> (String, String) {
    let (digits, point) = digits_of(a);
    // Digits kept: `point` before the point and `decimals` after it.
    let keep = point + decimals as i32;
    let mut d: Vec<u8> = digits.clone();
    if keep < 0 {
        d = vec![0];
    } else if (keep as usize) < d.len() {
        let round_up = d[keep as usize] >= 5;
        d.truncate(keep as usize);
        if round_up {
            let mut i = d.len();
            loop {
                if i == 0 {
                    d.insert(0, 1);
                    break;
                }
                i -= 1;
                if d[i] == 9 {
                    d[i] = 0;
                } else {
                    d[i] += 1;
                    break;
                }
            }
        }
    }
    let carried = if keep >= 0 { d.len() as i32 - keep.max(0) } else { 0 };
    let point = if keep >= 0 { point + carried.max(0) } else { point };
    // Lay out: integer digits then `decimals` fraction digits.
    let total_int = point.max(0) as usize;
    let mut all: Vec<u8> = Vec::new();
    if point <= 0 {
        all.extend(std::iter::repeat_n(0, (-point) as usize));
        all.extend(&d);
    } else {
        all.extend(&d);
    }
    let int_digits: Vec<u8> = if total_int == 0 { vec![0] } else { (0..total_int).map(|i| all.get(i).copied().unwrap_or(0)).collect() };
    let start = if total_int == 0 { 0 } else { total_int };
    let frac: String = (0..decimals).map(|i| char::from(b'0' + all.get(start + i).copied().unwrap_or(0))).collect();
    let int_text: String = int_digits.iter().map(|x| char::from(b'0' + x)).collect();
    (int_text, frac)
}

fn group(int_text: &str) -> String {
    let mut out = String::new();
    for (i, ch) in int_text.chars().enumerate() {
        if i > 0 && (int_text.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn is_zero(int_text: &str, frac: &str) -> bool {
    int_text.bytes().all(|b| b == b'0') && frac.bytes().all(|b| b == b'0')
}

/// A number written by a format string: `N`, `F`, `P`, `C`, `E`, `G` with a number of places, or a custom pattern (`0.00`, `#,##0.00`,
/// `0.0%`, `00000`, `0.00;(0.00)`, quotes and `\` for text).
pub fn format_number(n: f64, format: &str, c: Culture) -> Result<String, MError> {
    if n.is_nan() {
        return Ok("NaN".into());
    }
    if n.is_infinite() {
        return Ok(if n > 0.0 { "\u{221e}".into() } else { "-\u{221e}".into() });
    }
    let chars: Vec<char> = format.chars().collect();
    let letter = chars.first().copied().filter(|ch| ch.is_ascii_alphabetic());
    let precision: Option<usize> = chars.get(1..).filter(|r| !r.is_empty() && r.iter().all(char::is_ascii_digit)).map(|r| r.iter().collect::<String>().parse().unwrap_or(0));
    let standard_form = letter.is_some() && (chars.len() == 1 || precision.is_some());
    if !standard_form {
        return custom_number(n, format);
    }
    let a = n.abs();
    let neg = n < 0.0;
    match letter.expect("a letter").to_ascii_uppercase() {
        'N' | 'F' => {
            let (int_text, frac) = fixed(a, precision.unwrap_or(2));
            let int_text = if letter == Some('N') || letter == Some('n') { group(&int_text) } else { int_text };
            let body = if frac.is_empty() { int_text.clone() } else { format!("{int_text}.{frac}") };
            Ok(if neg && !is_zero(&int_text.replace(',', ""), &frac) { format!("-{body}") } else { body })
        }
        'P' => {
            let (int_text, frac) = fixed(a * 100.0, precision.unwrap_or(2));
            // The product is made on the digits' value (0.5 is 50): the double product is exact enough for the shortest form.
            let body = if frac.is_empty() { group(&int_text) } else { format!("{}.{frac}", group(&int_text)) };
            Ok(format!("{}{body}%", if neg && !is_zero(&int_text, &frac) { "-" } else { "" }))
        }
        'C' => {
            let default = match c {
                Culture::Ja => 0,
                Culture::En => 2,
            };
            let (int_text, frac) = fixed(a, precision.unwrap_or(default));
            let body = if frac.is_empty() { group(&int_text) } else { format!("{}.{frac}", group(&int_text)) };
            let sign = match c {
                Culture::Ja => "\u{a5}",
                Culture::En => "$",
            };
            let negative = neg && !is_zero(&int_text, &frac);
            Ok(match (c, negative) {
                (Culture::Ja, true) => format!("-{sign}{body}"),
                (Culture::En, true) => format!("({sign}{body})"),
                _ => format!("{sign}{body}"),
            })
        }
        'E' => {
            let places = precision.unwrap_or(6);
            let (digits, point) = digits_of(a);
            let exp = if a == 0.0 { 0 } else { point - 1 };
            // Round the digits to places + 1 significant digits.
            let scaled = if a == 0.0 { 0.0 } else { a / 10f64.powi(exp) };
            let (int_text, frac) = fixed(scaled, places);
            let (int_text, frac, exp) = if int_text.len() > 1 { ("1".to_string(), "0".repeat(places), exp + 1) } else { (int_text, frac, exp) };
            let _ = digits;
            let mantissa = if frac.is_empty() { int_text } else { format!("{int_text}.{frac}") };
            let e = if letter == Some('e') { 'e' } else { 'E' };
            Ok(format!("{}{mantissa}{e}{}{:03}", if neg { "-" } else { "" }, if exp < 0 { '-' } else { '+' }, exp.abs()))
        }
        'G' => match precision {
            None | Some(0) => Ok(culture::number_text(format!("{:.14e}", n).parse::<f64>().unwrap_or(n))),
            Some(p) => {
                let (digits, point) = digits_of(a);
                let exp = if a == 0.0 { 0 } else { point - 1 };
                let _ = digits;
                // p significant digits: fixed form when -5 < exponent < p, else with an exponent.
                let scaled_places = (p as i32 - 1 - exp).max(0) as usize;
                if exp >= p as i32 || exp < -4 {
                    let scaled = if a == 0.0 { 0.0 } else { a / 10f64.powi(exp) };
                    let (int_text, frac) = fixed(scaled, p - 1);
                    let (int_text, frac, exp) = if int_text.len() > 1 { ("1".to_string(), "0".repeat(p - 1), exp + 1) } else { (int_text, frac, exp) };
                    let frac = frac.trim_end_matches('0').to_string();
                    let mantissa = if frac.is_empty() { int_text } else { format!("{int_text}.{frac}") };
                    Ok(format!("{}{mantissa}E{}{:02}", if neg { "-" } else { "" }, if exp < 0 { '-' } else { '+' }, exp.abs()))
                } else {
                    let (int_text, frac) = fixed(a, scaled_places);
                    let frac = frac.trim_end_matches('0').to_string();
                    let body = if frac.is_empty() { int_text } else { format!("{int_text}.{frac}") };
                    Ok(format!("{}{body}", if neg { "-" } else { "" }))
                }
            }
        },
        _ => Err(MError::expr("The specified output Number format is not supported.")),
    }
}

/// A custom pattern: the sections (positive; negative; zero) split at `;` outside quotes; in one, `0` and `#` are digits, `,` between them
/// groups the thousands, `.` is the point, `%` multiplies by 100 and is written; anything else is text, `'…'`, `"…"` and `\` quote it.
fn custom_number(n: f64, pattern: &str) -> Result<String, MError> {
    let mut sections: Vec<Vec<char>> = vec![Vec::new()];
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for ch in pattern.chars() {
        if escaped {
            sections.last_mut().expect("a section").push(ch);
            escaped = false;
        } else if ch == '\\' {
            sections.last_mut().expect("a section").push(ch);
            escaped = true;
        } else if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            sections.last_mut().expect("a section").push(ch);
        } else if ch == '\'' || ch == '"' {
            quote = Some(ch);
            sections.last_mut().expect("a section").push(ch);
        } else if ch == ';' {
            sections.push(Vec::new());
        } else {
            sections.last_mut().expect("a section").push(ch);
        }
    }
    let (section, minus) = match (sections.len(), n) {
        (1, _) => (&sections[0], n < 0.0),
        (_, x) if x < 0.0 => (&sections[1], false),
        (3, x) if x == 0.0 => (&sections[2], false),
        _ => (&sections[0], false),
    };
    let a = n.abs();
    let (mut before, mut after, mut min_int, mut min_frac, mut max_frac) = (String::new(), String::new(), 0usize, 0usize, 0usize);
    let (mut seen_digit, mut in_frac, mut group_it, mut percent, mut last_digit_at) = (false, false, false, false, None::<usize>);
    let mut prefix = String::new();
    let mut suffix_chars: Vec<(usize, char)> = Vec::new();
    let mut i = 0;
    let mut literals: Vec<(usize, String)> = Vec::new();
    while i < section.len() {
        let ch = section[i];
        match ch {
            '0' | '#' => {
                seen_digit = true;
                if in_frac {
                    max_frac += 1;
                    if ch == '0' {
                        min_frac = max_frac;
                    }
                } else if ch == '0' {
                    min_int += 1;
                }
                last_digit_at = Some(i);
            }
            ',' if seen_digit && !in_frac => group_it = true,
            '.' if !in_frac => {
                in_frac = true;
                last_digit_at = Some(i);
            }
            '\'' | '"' => {
                let q = ch;
                let mut j = i + 1;
                let mut text = String::new();
                while j < section.len() && section[j] != q {
                    text.push(section[j]);
                    j += 1;
                }
                literals.push((i, text));
                i = j;
            }
            '\\' => {
                if let Some(&next) = section.get(i + 1) {
                    literals.push((i, next.to_string()));
                }
                i += 1;
            }
            '%' => {
                percent = true;
                literals.push((i, "%".to_string()));
            }
            other => literals.push((i, other.to_string())),
        }
        i += 1;
    }
    let _ = (&mut before, &mut after, &mut prefix, &mut suffix_chars);
    let scaled = if percent { a * 100.0 } else { a };
    let (int_text, frac) = fixed(scaled, max_frac);
    let int_text = if int_text.bytes().all(|b| b == b'0') { String::new() } else { int_text };
    let int_text = format!("{}{}", "0".repeat(min_int.saturating_sub(int_text.len())), int_text);
    let int_text = if group_it { group(&int_text) } else { int_text };
    let mut frac = frac;
    while frac.len() > min_frac && frac.ends_with('0') {
        frac.pop();
    }
    let number = if frac.is_empty() { int_text.clone() } else { format!("{int_text}.{frac}") };
    let number = if number.is_empty() { "0".to_string() } else { number };
    // The text before the first digit placeholder, the number, the text after the last.
    let first_digit = section.iter().position(|ch| matches!(ch, '0' | '#' | '.' ));
    let split_at = first_digit.unwrap_or(0);
    let end_at = last_digit_at.unwrap_or(0);
    let mut out = String::new();
    if minus && !is_zero(&int_text.replace(',', ""), &frac) && !(int_text.is_empty() && frac.is_empty()) {
        out.push('-');
    }
    for (at, text) in &literals {
        if *at < split_at {
            out.push_str(text);
        }
    }
    out.push_str(&number);
    for (at, text) in &literals {
        if *at > end_at || first_digit.is_none() && *at >= split_at {
            out.push_str(text);
        }
    }
    Ok(out)
}

/// `Text.Format("#{0} and #[name]", …)`: `#{n}` the n-th of a list, `#[name]` the field of a record, `##` a `#`.
pub fn text_format(template: &str, positional: &[String], named: &[(String, String)]) -> Result<String, MError> {
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '#' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        match chars.get(i + 1) {
            Some('#') => {
                out.push('#');
                i += 2;
            }
            Some(&open @ ('{' | '[')) => {
                let close = if open == '{' { '}' } else { ']' };
                let Some(end) = chars[i + 2..].iter().position(|&ch| ch == close) else {
                    return Err(MError::format("Close the placeholder of the text format."));
                };
                let name: String = chars[i + 2..i + 2 + end].iter().collect::<String>().trim().to_string();
                let value = if open == '{' {
                    name.parse::<usize>().ok().and_then(|k| positional.get(k).cloned())
                } else {
                    named.iter().find(|(n, _)| *n == name).map(|(_, v)| v.clone())
                };
                match value {
                    Some(v) => out.push_str(&v),
                    None => return Err(MError::expr("The format string has an invalid placeholder or value: there weren't enough elements in the enumeration to complete the operation.")),
                }
                i += end + 3;
            }
            _ => {
                out.push('#');
                i += 1;
            }
        }
    }
    Ok(out)
}
