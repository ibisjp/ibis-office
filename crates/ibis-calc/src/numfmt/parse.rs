// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Parsing a number format code into sections and tokens.

use super::FormatColor;

/// AM/PM styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmPm {
    /// `AM/PM`
    Upper,
    /// `am/pm`
    Lower,
    /// `A/P`
    A,
    /// `a/p`
    LowerA,
    /// `午前/午後`
    Ja,
    /// `上午/下午`
    Zh,
}

/// A part of a date or time, with the number of letters written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatePart {
    Year(u8),
    Month(u8),
    Day(u8),
    /// `aaa` / `aaaa`: the day of the week in Japanese.
    JaDay(u8),
    Hour(u8),
    Minute(u8),
    Second(u8),
    /// Fractions of a second: the digits.
    SubSecond(u8),
    ElapsedHours(u8),
    ElapsedMinutes(u8),
    ElapsedSeconds(u8),
    AmPm(AmPm),
    /// `e` / `ee`: the year of the Japanese era.
    EraYear(u8),
    /// `g`, `gg`, `ggg`: the era's initial, first character, name.
    Era(u8),
    /// `bb` / `bbbb`: the Buddhist year.
    Buddhist(u8),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    /// `0`, `#` or `?`.
    Digit(char),
    Point,
    Comma,
    Percent,
    /// `E+` (true) or `E-`, and whether written in lower case.
    Exponent(bool, bool),
    Slash,
    FixedDenominator(u64),
    Literal(String),
    /// `_x`: a space the width of x.
    Space(char),
    /// `*x`: x repeated to fill the cell.
    Fill(char),
    /// `@`: the text.
    Text,
    General,
    Date(DatePart),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Condition {
    pub op: &'static str,
    pub value: f64,
}

impl Condition {
    pub fn holds(&self, n: f64) -> bool {
        match self.op {
            "<" => n < self.value,
            "<=" => n <= self.value,
            ">" => n > self.value,
            ">=" => n >= self.value,
            "=" => n == self.value,
            _ => n != self.value,
        }
    }

    /// A condition for negative numbers only (`[<0]`, `[<=-1]`): the
    /// section shows the number without its sign.
    pub fn for_negatives(&self) -> bool {
        matches!(self.op, "<" | "<=") && self.value <= 0.0
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    pub tokens: Vec<Token>,
    pub color: Option<FormatColor>,
    pub condition: Option<Condition>,
    /// `[DBNum1]`–`[DBNum3]`: Japanese numerals.
    pub dbnum: Option<u8>,
    /// `[$-411]`: the Japanese locale.
    pub japanese: bool,
    /// `[$-407]`, `[$-409]`, `[$-411]` and the like: the locale (LCID) whose month and day names a date shows (and whose
    /// language `aaa`/`aaaa` follow). `None` without a locale tag: English `ddd`/`dddd`, Japanese `aaa`/`aaaa`.
    pub locale: Option<u16>,
    /// Whether the format code contains invalid tokens (e.g. unquoted 'n' or single 'B').
    pub invalid: bool,
}

impl Section {
    pub fn is_date(&self) -> bool {
        self.tokens.iter().any(|t| matches!(t, Token::Date(_)))
    }
}

fn color_named(name: &str) -> Option<FormatColor> {
    Some(match name.to_ascii_lowercase().as_str() {
        "black" => FormatColor::Black,
        "blue" => FormatColor::Blue,
        "cyan" => FormatColor::Cyan,
        "green" => FormatColor::Green,
        "magenta" => FormatColor::Magenta,
        "red" => FormatColor::Red,
        "white" => FormatColor::White,
        "yellow" => FormatColor::Yellow,
        n => {
            let k: u8 = n.strip_prefix("color")?.parse().ok()?;
            if !(1..=56).contains(&k) {
                return None;
            }
            FormatColor::Indexed(k)
        }
    })
}

/// Split a code into its sections (at `;` outside quotes and brackets).
fn split_sections(code: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut start, mut quoted, mut bracket, mut escaped) = (0, false, false, false);
    for (i, c) in code.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' | '!' | '_' | '*' if !quoted && !bracket => escaped = true,
            '"' if !bracket => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            ';' if !quoted && !bracket => {
                out.push(&code[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&code[start..]);
    out
}

pub fn parse(code: &str) -> Vec<Section> {
    let code = if code.is_empty() { "General" } else { code };
    split_sections(code).into_iter().map(parse_section).collect()
}

fn push_literal(tokens: &mut Vec<Token>, s: &str) {
    if let Some(Token::Literal(last)) = tokens.last_mut() {
        last.push_str(s);
    } else {
        tokens.push(Token::Literal(s.to_string()));
    }
}

fn parse_section(s: &str) -> Section {
    let mut section = Section::default();
    let mut system: Option<&str> = None;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let lower = |c: char| c.to_ascii_lowercase();
    let starts_with_ci = |i: usize, pat: &str| -> bool {
        let p: Vec<char> = pat.chars().collect();
        i + p.len() <= chars.len() && chars[i..i + p.len()].iter().zip(&p).all(|(a, b)| lower(*a) == lower(*b))
    };
    let run = |i: usize, c: char| -> usize { chars[i..].iter().take_while(|x| lower(**x) == c).count() };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                let mut j = i + 1;
                let mut lit = String::new();
                while j < chars.len() && chars[j] != '"' {
                    lit.push(chars[j]);
                    j += 1;
                }
                push_literal(&mut section.tokens, &lit);
                i = j + 1;
            }
            // `!` shows the next character as is, exactly like a backslash (#181).
            '\\' | '!' => {
                if let Some(n) = chars.get(i + 1) {
                    push_literal(&mut section.tokens, &n.to_string());
                }
                i += 2;
            }
            '_' => {
                section.tokens.push(Token::Space(chars.get(i + 1).copied().unwrap_or(' ')));
                i += 2;
            }
            '*' => {
                section.tokens.push(Token::Fill(chars.get(i + 1).copied().unwrap_or(' ')));
                i += 2;
            }
            '[' => {
                let end = chars[i..].iter().position(|&x| x == ']').map_or(chars.len(), |p| i + p);
                let inner: String = chars[i + 1..end.min(chars.len())].iter().collect();
                i = end + 1;
                let l = inner.to_ascii_lowercase();
                if let Some(color) = color_named(&inner) {
                    section.color = Some(color);
                } else if let Some(rest) = inner.strip_prefix('$') {
                    // [$USD], [$¥-411], [$-411]: the currency text before the '-'.
                    let text = rest.split('-').next().unwrap_or("");
                    if !text.is_empty() {
                        push_literal(&mut section.tokens, text);
                    }
                    let locale = rest.rsplit('-').next().unwrap_or("").to_ascii_lowercase();
                    // A tag such as `de-DE`: all of it after the first `-`; else the last part.
                    let tag = rest.split_once('-').map_or("", |(_, t)| t);
                    let lcid = super::locale::lcid_of(tag).or_else(|| super::locale::lcid_of(&locale));
                    match locale.as_str() {
                        // The system's long date and time.
                        "f800" | "sysdate" => system = Some("yyyy\"年\"m\"月\"d\"日\" aaaa"),
                        "f400" | "systime" => system = Some("h:mm:ss"),
                        "411" | "ja" => {
                            section.japanese = true;
                            section.locale = Some(0x0411);
                        }
                        _ if lcid.is_some_and(|l| l & 0x3FF == 0x11) => {
                            section.japanese = true;
                            section.locale = Some(0x0411);
                        }
                        // An explicit locale (`[$-409]`) also gives `aaa`/`aaaa` its weekday names: Excel shows `[$-409]aaaa`
                        // as Wednesday. Only a tag after a `-` is a locale; `[$USD]` is a currency.
                        _ if rest.contains('-') && lcid.is_some() => section.locale = lcid,
                        _ if lcid.is_some_and(|l| super::locale::names(l).is_some()) => section.locale = lcid,
                        _ => {}
                    }
                } else if l.starts_with("dbnum") {
                    section.dbnum = l[5..].parse().ok();
                } else if !l.is_empty() && l.chars().all(|x| x == 'h') {
                    section.tokens.push(Token::Date(DatePart::ElapsedHours(l.len() as u8)));
                } else if !l.is_empty() && l.chars().all(|x| x == 'm') {
                    section.tokens.push(Token::Date(DatePart::ElapsedMinutes(l.len() as u8)));
                } else if !l.is_empty() && l.chars().all(|x| x == 's') {
                    section.tokens.push(Token::Date(DatePart::ElapsedSeconds(l.len() as u8)));
                } else if let Some(cond) = parse_condition(&inner) {
                    section.condition = Some(cond);
                }
                // Other brackets (calendars, locales) are left out.
            }
            '0' | '#' | '?' => {
                section.tokens.push(Token::Digit(c));
                i += 1;
            }
            '.' => {
                // Fractions of a second after seconds.
                let zeros = chars[i + 1..].iter().take_while(|x| **x == '0').count();
                let after_seconds = section.tokens.iter().rev().find(|t| !matches!(t, Token::Literal(_))).is_some_and(|t| matches!(t, Token::Date(DatePart::Second(_) | DatePart::ElapsedSeconds(_))));
                if after_seconds && zeros > 0 {
                    section.tokens.push(Token::Date(DatePart::SubSecond(zeros.min(3) as u8)));
                    i += 1 + zeros;
                } else {
                    section.tokens.push(Token::Point);
                    i += 1;
                }
            }
            ',' => {
                section.tokens.push(Token::Comma);
                i += 1;
            }
            '%' => {
                section.tokens.push(Token::Percent);
                i += 1;
            }
            '@' => {
                section.tokens.push(Token::Text);
                i += 1;
            }
            'E' | 'e' if matches!(chars.get(i + 1), Some('+' | '-')) && section.tokens.iter().any(|t| matches!(t, Token::Digit(_))) => {
                section.tokens.push(Token::Exponent(chars[i + 1] == '+', c == 'e'));
                i += 2;
            }
            '/' => {
                // A fixed denominator: digits starting with 1–9.
                if chars.get(i + 1).is_some_and(|d| ('1'..='9').contains(d)) {
                    let digits: String = chars[i + 1..].iter().take_while(|d| d.is_ascii_digit()).collect();
                    section.tokens.push(Token::Slash);
                    section.tokens.push(Token::FixedDenominator(digits.parse().unwrap_or(1)));
                    i += 1 + digits.len();
                } else {
                    section.tokens.push(Token::Slash);
                    i += 1;
                }
            }
            _ if starts_with_ci(i, "general") => {
                section.tokens.push(Token::General);
                i += 7;
            }
            _ if starts_with_ci(i, "am/pm") => {
                let style = if chars[i] == 'a' { AmPm::Lower } else { AmPm::Upper };
                section.tokens.push(Token::Date(DatePart::AmPm(style)));
                i += 5;
            }
            _ if starts_with_ci(i, "a/p") => {
                let style = if chars[i] == 'a' { AmPm::LowerA } else { AmPm::A };
                section.tokens.push(Token::Date(DatePart::AmPm(style)));
                i += 3;
            }
            '午' if chars[i..].iter().collect::<String>().starts_with("午前/午後") => {
                section.tokens.push(Token::Date(DatePart::AmPm(AmPm::Ja)));
                i += 5;
            }
            '上' if chars[i..].iter().collect::<String>().starts_with("上午/下午") => {
                section.tokens.push(Token::Date(DatePart::AmPm(AmPm::Zh)));
                i += 5;
            }
            'y' | 'Y' => {
                let n = run(i, 'y');
                section.tokens.push(Token::Date(DatePart::Year(if n <= 2 { 2 } else { 4 })));
                i += n;
            }
            'm' | 'M' => {
                let n = run(i, 'm');
                section.tokens.push(Token::Date(DatePart::Month(n.min(5) as u8)));
                i += n;
            }
            'd' | 'D' => {
                let n = run(i, 'd');
                section.tokens.push(Token::Date(DatePart::Day(n.min(4) as u8)));
                i += n;
            }
            'h' | 'H' => {
                let n = run(i, 'h');
                section.tokens.push(Token::Date(DatePart::Hour(n.min(2) as u8)));
                i += n;
            }
            's' | 'S' => {
                let n = run(i, 's');
                section.tokens.push(Token::Date(DatePart::Second(n.min(2) as u8)));
                i += n;
            }
            'r' | 'R' => {
                // The era's year, as e (Japanese locale).
                let n = run(i, 'r');
                section.tokens.push(Token::Date(DatePart::EraYear(n.min(2) as u8)));
                i += n;
            }
            'e' | 'E' => {
                let n = run(i, 'e');
                section.tokens.push(Token::Date(DatePart::EraYear(n.min(2) as u8)));
                i += n;
            }
            'g' | 'G' => {
                let n = run(i, 'g');
                section.tokens.push(Token::Date(DatePart::Era(n.min(3) as u8)));
                i += n;
            }
            'a' | 'A' if run(i, 'a') >= 3 => {
                let n = run(i, 'a');
                section.tokens.push(Token::Date(DatePart::JaDay(n.min(4) as u8)));
                i += n;
            }
            'n' | 'N' => {
                section.invalid = true;
                push_literal(&mut section.tokens, &c.to_string());
                i += 1;
            }
            'b' | 'B' => {
                let n = run(i, 'b');
                section.tokens.push(Token::Date(DatePart::Buddhist(if n >= 3 { 4 } else { 2 })));
                i += n;
            }
            _ => {
                push_literal(&mut section.tokens, &c.to_string());
                i += 1;
            }
        }
    }
    if let Some(code) = system {
        let mut replaced = parse_section(code);
        replaced.color = section.color;
        replaced.condition = section.condition;
        replaced.invalid = section.invalid;
        return replaced;
    }
    // AM/PM in Japanese for the Japanese locale.
    if section.japanese {
        for t in section.tokens.iter_mut() {
            if let Token::Date(DatePart::AmPm(style @ (AmPm::Upper | AmPm::Lower))) = t {
                *style = AmPm::Ja;
            }
        }
    }
    // Minutes: an m right after hours or right before seconds.
    let positions: Vec<usize> = section.tokens.iter().enumerate().filter(|(_, t)| matches!(t, Token::Date(_))).map(|(i, _)| i).collect();
    for (k, &p) in positions.iter().enumerate() {
        if let Token::Date(DatePart::Month(n)) = section.tokens[p]
            && n <= 2
        {
            let prev = k.checked_sub(1).map(|j| &section.tokens[positions[j]]);
            let next = positions.get(k + 1).map(|&j| &section.tokens[j]);
            let after_hour = matches!(prev, Some(Token::Date(DatePart::Hour(_) | DatePart::ElapsedHours(_))));
            let before_second = matches!(next, Some(Token::Date(DatePart::Second(_) | DatePart::ElapsedSeconds(_))));
            if after_hour || before_second {
                section.tokens[p] = Token::Date(DatePart::Minute(n));
            }
        }
    }
    // In a date, a slash and digits are literal.
    if section.is_date() {
        let tokens = std::mem::take(&mut section.tokens);
        for t in tokens {
            match t {
                Token::Slash => push_literal(&mut section.tokens, "/"),
                Token::FixedDenominator(d) => push_literal(&mut section.tokens, &d.to_string()),
                Token::Comma => push_literal(&mut section.tokens, ","),
                Token::Percent => push_literal(&mut section.tokens, "%"),
                other => section.tokens.push(other),
            }
        }
    }
    section
}

fn parse_condition(s: &str) -> Option<Condition> {
    let s = s.trim();
    let (op, rest) = ["<=", ">=", "<>", "<", ">", "="].iter().find_map(|op| s.strip_prefix(op).map(|r| (*op, r)))?;
    Some(Condition { op, value: rest.trim().parse().ok()? })
}
