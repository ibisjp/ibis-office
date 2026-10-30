// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Number formats: what a value looks like in a cell.
//!
//! A format code (`#,##0.00;[Red]-#,##0.00`, `yyyy/m/d`, `[DBNum1]General`)
//! is parsed once into sections of tokens ([`parse`], cached) and applied
//! to a value ([`format`]). The result is the text Excel shows, with what
//! the display needs beside it: the colour, where a fill character repeats
//! to the cell's width (`*`), and whether the value cannot be shown (a
//! negative date: Excel fills the cell with `#`).
//!
//! Numbers are formatted from their decimal digits as Excel sees them (15
//! significant digits), so that 0.995 shows as 1.00 with `0.00`, as in
//! Excel, and not 0.99 as the binary value would round.

pub(crate) mod dbnum;
mod locale;
mod parse;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::eval::convert::{date_from_serial, days_in_month};
use crate::value::CellValue;
pub use parse::{Condition, DatePart, Section, Token, parse};

/// A colour a format names (`[Red]`, `[Color10]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatColor {
    Black,
    Blue,
    Cyan,
    Green,
    Magenta,
    Red,
    White,
    Yellow,
    /// `[ColorN]`, 1–56: the workbook's palette entry.
    Indexed(u8),
}

/// Excel's default palette of `[Color1]`–`[Color56]`.
const PALETTE: [u32; 56] = [
    0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x800000, 0x008000, 0x000080, 0x808000, 0x800080, 0x008080,
    0xC0C0C0, 0x808080, 0x9999FF, 0x993366, 0xFFFFCC, 0xCCFFFF, 0x660066, 0xFF8080, 0x0066CC, 0xCCCCFF, 0x000080, 0xFF00FF, 0xFFFF00, 0x00FFFF,
    0x800080, 0x800000, 0x008080, 0x0000FF, 0x00CCFF, 0xCCFFFF, 0xCCFFCC, 0xFFFF99, 0x99CCFF, 0xFF99CC, 0xCC99FF, 0xFFCC99, 0x3366FF, 0x33CCCC,
    0x99CC00, 0xFFCC00, 0xFF9900, 0xFF6600, 0x666699, 0x969696, 0x003366, 0x339966, 0x003300, 0x333300, 0x993300, 0x993366, 0x333399, 0x333333,
];

impl FormatColor {
    /// The colour as `0xRRGGBB`.
    pub fn rgb(self) -> u32 {
        match self {
            Self::Black => 0x000000,
            Self::Blue => 0x0000FF,
            Self::Cyan => 0x00FFFF,
            Self::Green => 0x00FF00,
            Self::Magenta => 0xFF00FF,
            Self::Red => 0xFF0000,
            Self::White => 0xFFFFFF,
            Self::Yellow => 0xFFFF00,
            Self::Indexed(n) => PALETTE[(n.clamp(1, 56) - 1) as usize],
        }
    }
}

/// A value formatted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Formatted {
    pub text: String,
    pub color: Option<FormatColor>,
    /// A character to repeat so the text fills the cell (`*x`), and the
    /// byte offset in `text` where the repeats go.
    pub fill: Option<(usize, char)>,
    /// The value cannot be shown in this format (a date before 1900 or
    /// after 9999): Excel fills the cell with `#`.
    pub overflow: bool,
}

/// A parsed format code, shared.
pub type Parsed = Rc<Vec<Section>>;

thread_local! {
    static CACHE: RefCell<HashMap<String, Parsed>> = RefCell::new(HashMap::new());
}

/// The parsed sections of `code` (cached).
pub fn parsed(code: &str) -> Parsed {
    if let Some(p) = CACHE.with(|c| c.borrow().get(code).cloned()) {
        return p;
    }
    let p = Rc::new(parse(code));
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 4096 {
            c.clear();
        }
        c.insert(code.to_string(), p.clone());
    });
    p
}

/// Whether `code` shows a number as a date or time (its first section has
/// date or time parts).
pub fn is_date_format(code: &str) -> bool {
    parsed(code).first().is_some_and(Section::is_date)
}

/// Format a cell's value with `code`.
pub fn format(value: &CellValue, code: &str, date1904: bool) -> Formatted {
    let sections = parsed(code);
    if sections.iter().any(|s| s.invalid) {
        return Formatted { overflow: true, ..Default::default() };
    }
    match value {
        CellValue::Empty => Formatted::default(),
        // Denormal numbers are zero to Excel.
        CellValue::Number(n) => format_number(if n.abs() < f64::MIN_POSITIVE { 0.0 } else { *n }, &sections, date1904),
        CellValue::Text(t) => format_text(t, &sections),
        // Logical values take the text section, as text.
        CellValue::Bool(b) => format_text(if *b { "TRUE" } else { "FALSE" }, &sections),
        CellValue::Error(e) => Formatted { text: e.as_str().into(), ..Default::default() },
    }
}

/// Just the text (for TEXT and the like).
pub fn format_text_only(value: &CellValue, code: &str, date1904: bool) -> String {
    format(value, code, date1904).text
}

fn format_text(t: &str, sections: &[Section]) -> Formatted {
    // The fourth section, or a section with @ (a single section with @).
    let section = if sections.len() >= 4 {
        Some(&sections[3])
    } else {
        sections.iter().find(|s| s.tokens.iter().any(|t| matches!(t, Token::Text)))
    };
    let Some(section) = section else {
        return Formatted { text: t.to_string(), ..Default::default() };
    };
    let mut out = Formatted { color: section.color, ..Default::default() };
    for token in &section.tokens {
        match token {
            Token::Text => out.text.push_str(t),
            Token::Literal(s) => out.text.push_str(s),
            Token::Space(_) => out.text.push(' '),
            Token::Fill(c) => out.fill = Some((out.text.len(), *c)),
            _ => {}
        }
    }
    out
}

/// Which section a number takes, and whether it shows as its magnitude
/// (the sign then comes from the section's own text, if any).
fn choose(n: f64, sections: &[Section]) -> Option<(usize, bool)> {
    let conditional = sections.iter().take(2).any(|s| s.condition.is_some());
    if conditional {
        // Conditions of the first two sections; the next one for the rest.
        for (i, s) in sections.iter().enumerate().take(2) {
            if let Some(c) = &s.condition
                && c.holds(n)
            {
                // A section meant for negative numbers shows the magnitude.
                return Some((i, c.for_negatives()));
            }
        }
        let rest = sections.iter().enumerate().take(3).find(|(_, s)| s.condition.is_none()).map(|(i, _)| i);
        // After a section for negatives, the rest show without a sign.
        let after_negatives = sections[0].condition.is_some_and(|c| c.for_negatives());
        return match rest {
            Some(i) => Some((i, after_negatives)),
            // No section for the value: Excel shows ###.
            None => None,
        };
    }
    let numeric: Vec<usize> = (0..sections.len().min(3)).collect();
    Some(match numeric.len() {
        0 => return None,
        1 => (0, false),
        2 => {
            if n < 0.0 {
                (1, true)
            } else {
                (0, false)
            }
        }
        _ => {
            if n > 0.0 {
                (0, false)
            } else if n < 0.0 {
                (1, true)
            } else {
                (2, false)
            }
        }
    })
}

fn format_number(n: f64, sections: &[Section], date1904: bool) -> Formatted {
    let Some((index, magnitude)) = choose(n, sections) else {
        return Formatted { overflow: true, ..Default::default() };
    };
    let section = &sections[index];
    // A text-only section shows a number as General.
    let value = if magnitude { n.abs() } else { n };
    let mut out = Formatted { color: section.color, ..Default::default() };
    if section.is_date() {
        return date_section(value, section, date1904, out);
    }
    let has_digits = section.tokens.iter().any(|t| matches!(t, Token::Digit(_) | Token::General));
    // A section for text shows a number as General, without its literals.
    if !has_digits && section.tokens.iter().any(|t| matches!(t, Token::Text)) {
        out.text = general(value, 11);
        return out;
    }
    if !has_digits {
        // Literals only (a sign still shows for a negative value).
        if value < 0.0 && !magnitude {
            out.text.push('-');
        }
        for token in &section.tokens {
            match token {
                Token::Literal(s) => out.text.push_str(s),
                Token::Space(_) => out.text.push(' '),
                Token::Fill(c) => out.fill = Some((out.text.len(), *c)),
                Token::Text => out.text.push_str(&general(value, 11)),
                _ => {}
            }
        }
        return out;
    }
    let text = number_section(value, section, &mut out);
    // Excel shows no more than 255 characters: longer is ###.
    if out.overflow || text.chars().count() > 255 {
        return Formatted { overflow: true, color: section.color, ..Default::default() };
    }
    let general = section.tokens.iter().any(|t| matches!(t, Token::General));
    out.text = match section.dbnum {
        Some(level) if !general => dbnum::digits(&text, level),
        _ => text,
    };
    out
}

// ----- Numbers -------------------------------------------------------------

/// A number's decimal digits (at most 15 significant, as Excel keeps
/// them) and the power of ten of the first: `digits × 10^(exponent + 1 - len)`.
#[derive(Clone, Debug)]
struct Decimal {
    digits: Vec<u8>,
    /// The position of the decimal point: the number of integer digits
    /// (may be ≤ 0 or more than the digits).
    point: i32,
}

const POWERS_OF_TEN: [u64; 20] = [1, 10, 100, 1_000, 10_000, 100_000, 1_000_000, 10_000_000, 100_000_000, 1_000_000_000, 10_000_000_000, 100_000_000_000, 1_000_000_000_000, 10_000_000_000_000, 100_000_000_000_000, 1_000_000_000_000_000, 10_000_000_000_000_000, 100_000_000_000_000_000, 1_000_000_000_000_000_000, 10_000_000_000_000_000_000];

impl Decimal {
    fn of(v: f64) -> Decimal {
        let v = v.abs();
        if v == 0.0 || !v.is_finite() {
            return Decimal { digits: Vec::new(), point: 1 };
        }
        // Denormal numbers show as zero.
        if v < 1e-307 {
            return Decimal { digits: Vec::new(), point: 1 };
        }
        // A whole number of at most 15 digits is its own digits (the usual
        // cell: counts and amounts; formatting every one with 15 significant
        // digits took the most time of reading a large workbook).
        if v < 1e15 && v.fract() == 0.0 {
            let mut n = v as u64;
            let mut digits: Vec<u8> = Vec::with_capacity(15);
            while n > 0 {
                digits.push((n % 10) as u8);
                n /= 10;
            }
            digits.reverse();
            let point = digits.len() as i32;
            while digits.last() == Some(&0) {
                digits.pop();
            }
            return Decimal { digits, point };
        }
        if let Some(d) = Decimal::of_exact(v) {
            return d;
        }
        let s = format!("{:.14e}", v);
        let (mantissa, exponent) = s.split_once('e').unwrap();
        let exponent: i32 = exponent.parse().unwrap();
        let mut digits: Vec<u8> = mantissa.bytes().filter(|b| b.is_ascii_digit()).map(|b| b - b'0').collect();
        while digits.last() == Some(&0) {
            digits.pop();
        }
        Decimal { digits, point: exponent + 1 }
    }

    /// The 15 significant digits of a number between 1e-5 and 1e15 that is
    /// not whole, as `format!("{:.14e}")` gives them, found with integer
    /// arithmetic (the value is exactly `m × 2^e2`; scaling it by a power of
    /// ten and rounding the exact quotient, ties to even). `None` outside that
    /// range. Formatting with the standard library took most of the time of
    /// reading a workbook of amounts with decimals.
    fn of_exact(v: f64) -> Option<Decimal> {
        if !(1e-5..1e15).contains(&v) {
            return None;
        }
        let bits = v.to_bits();
        let exp = ((bits >> 52) & 0x7ff) as i32;
        let m = ((bits & ((1u64 << 52) - 1)) | (1u64 << 52)) as u128;
        let e2 = exp - 1075;
        if e2 >= 0 || e2 < -120 {
            return None;
        }
        let shift = (-e2) as u32;
        let mut e10 = v.log10().floor() as i32;
        for _ in 0..4 {
            let k = 14 - e10;
            if !(0..=19).contains(&k) {
                return None;
            }
            let scaled = m * POWERS_OF_TEN[k as usize] as u128;
            let floor = scaled >> shift;
            // The power of ten the number is in is decided by the number, not by its rounding.
            if floor < 100_000_000_000_000 {
                e10 -= 1;
                continue;
            }
            if floor >= 1_000_000_000_000_000 {
                e10 += 1;
                continue;
            }
            let mut n = floor;
            let rem = scaled & ((1u128 << shift) - 1);
            let half = 1u128 << (shift - 1);
            if rem > half || (rem == half && n & 1 == 1) {
                n += 1;
            }
            // Rounded up to the next power of ten.
            if n == 1_000_000_000_000_000 {
                return Some(Decimal { digits: vec![1], point: e10 + 2 });
            }
            // The 15 digits, least significant last; trailing zeros left off.
            let mut t = n as u64;
            let mut buf = [0u8; 15];
            for slot in buf.iter_mut().rev() {
                *slot = (t % 10) as u8;
                t /= 10;
            }
            let mut len = 15;
            while len > 0 && buf[len - 1] == 0 {
                len -= 1;
            }
            return Some(Decimal { digits: buf[..len].to_vec(), point: e10 + 1 });
        }
        None
    }

    fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }

    /// Rounded to `decimals` places after the point, halves up.
    fn round(&self, decimals: i32) -> Decimal {
        let keep = self.point + decimals;
        if keep >= self.digits.len() as i32 {
            return self.clone();
        }
        if keep < 0 {
            return Decimal { digits: Vec::new(), point: 1 };
        }
        let mut digits = self.digits[..keep as usize].to_vec();
        let mut point = self.point;
        if self.digits[keep as usize] >= 5 {
            // Carry.
            let mut i = digits.len();
            loop {
                if i == 0 {
                    digits.insert(0, 1);
                    point += 1;
                    break;
                }
                i -= 1;
                if digits[i] == 9 {
                    digits[i] = 0;
                } else {
                    digits[i] += 1;
                    break;
                }
            }
        }
        while digits.last() == Some(&0) {
            digits.pop();
        }
        if digits.is_empty() {
            point = 1;
        }
        Decimal { digits, point }
    }

    /// The integer digits (none for a number below one).
    fn integer(&self) -> Vec<u8> {
        if self.point <= 0 {
            return Vec::new();
        }
        (0..self.point as usize).map(|i| self.digits.get(i).copied().unwrap_or(0)).collect()
    }

    /// `count` digits after the point.
    fn fraction(&self, count: usize) -> Vec<u8> {
        (0..count as i32).map(|i| {
            let at = self.point + i;
            if at < 0 { 0 } else { self.digits.get(at as usize).copied().unwrap_or(0) }
        })
        .collect()
    }

    /// Plain decimal representation (e.g. "123.45" or "0.05").
    fn to_plain_string(&self) -> String {
        if self.is_zero() {
            return "0".into();
        }
        let int: String = if self.point <= 0 { "0".into() } else { self.integer().iter().map(|x| (b'0' + x) as char).collect() };
        let frac_len = (self.digits.len() as i32 - self.point).max(0) as usize;
        let frac: String = self.fraction(frac_len).iter().map(|x| (b'0' + x) as char).collect();
        if frac.is_empty() { int } else { format!("{int}.{frac}") }
    }
}

/// General, in at most `width` characters excluding the minus sign (11 in a cell), as Excel shows
/// it: plain digits, rounded to fit, or the exponent form for numbers too
/// large or small.
pub fn general(v: f64, width: usize) -> String {
    let d = Decimal::of(v);
    if d.is_zero() {
        return "0".into();
    }
    let negative = v < 0.0;
    // ECMA-376 numFmt / General floating-point rule excludes the minus sign.
    // https://learn.microsoft.com/dotnet/api/documentformat.openxml.spreadsheet.numberingformat
    let room = width;
    let sign = if negative { "-" } else { "" };
    // Plain digits, rounded to fit: the text and the significant digits it shows.
    let plain = (d.point <= room as i32).then(|| {
        let int_len = d.point.max(1) as usize;
        let decimals = room.saturating_sub(int_len + 1) as i32;
        let r = d.round(decimals.max(0));
        if r.is_zero() || r.point > room as i32 {
            return None;
        }
        let int: String = if r.point <= 0 { "0".into() } else { r.integer().iter().map(|x| (b'0' + x) as char).collect() };
        let frac_len = (r.digits.len() as i32 - r.point).max(0) as usize;
        let frac: String = r.fraction(frac_len).iter().map(|x| (b'0' + x) as char).collect();
        let text = if frac.is_empty() { format!("{sign}{int}") } else { format!("{sign}{int}.{frac}") };
        Some((text, r.digits.len()))
    }).flatten();
    // The exponent form: d.ddddE+XX within the width.
    let exponent = d.point - 1;
    let exp_text = |e: i32| format!("E{}{:02}", if e < 0 { "-" } else { "+" }, e.abs());
    let mantissa_room = room.saturating_sub(exp_text(exponent).len());
    let decimals = mantissa_room.saturating_sub(2) as i32;
    let m = Decimal { digits: d.digits.clone(), point: 1 }.round(decimals.max(0));
    let (m, e) = if m.point > 1 { (Decimal { digits: vec![1], point: 1 }, exponent + 1) } else { (m, exponent) };
    let first = m.digits.first().copied().unwrap_or(0);
    let rest: String = m.digits.iter().skip(1).map(|x| (b'0' + x) as char).collect();
    let sci = if rest.is_empty() { format!("{sign}{first}{}", exp_text(e)) } else { format!("{sign}{first}.{rest}{}", exp_text(e)) };
    match plain {
        // Plain unless the exponent form shows more of the number.
        Some((text, shown)) if shown >= m.digits.len() => text,
        _ => sci,
    }
}

/// The layout of a number section's placeholders.
struct Layout {
    /// Integer placeholders, in order.
    int: Vec<char>,
    /// Fraction placeholders.
    frac: Vec<char>,
    grouping: bool,
    /// Divisions by 1000 (commas after the last placeholder).
    scale: i32,
    percent: i32,
    point: bool,
    exponent: Option<(bool, Vec<char>)>,
    fraction: Option<FractionLayout>,
}

struct FractionLayout {
    /// Placeholders of the whole part (empty for a plain fraction).
    whole: Vec<char>,
    numerator: Vec<char>,
    denominator: Denominator,
}

enum Denominator {
    Digits(Vec<char>),
    Fixed(u64),
}

fn layout(section: &Section) -> Layout {
    let mut l = Layout { int: Vec::new(), frac: Vec::new(), grouping: false, scale: 0, percent: 0, point: false, exponent: None, fraction: None };
    let mut in_exp = false;
    let mut exp_digits = Vec::new();
    let mut exp_plus = false;
    let tokens = &section.tokens;
    for (i, t) in tokens.iter().enumerate() {
        match t {
            Token::Digit(c) if in_exp => exp_digits.push(*c),
            Token::Digit(c) if l.point => l.frac.push(*c),
            Token::Digit(c) => l.int.push(*c),
            Token::Point => l.point = true,
            Token::Comma => {
                // Between placeholders: grouping; after the last: scaling.
                let digit_after = tokens[i + 1..].iter().take_while(|t| !matches!(t, Token::Point | Token::Exponent(..))).any(|t| matches!(t, Token::Digit(_)));
                let digit_before = tokens[..i].iter().any(|t| matches!(t, Token::Digit(_)));
                if digit_before && digit_after && !l.point {
                    l.grouping = true;
                } else if digit_before && !digit_after {
                    l.scale += 1;
                }
            }
            Token::Percent => l.percent += 1,
            Token::Exponent(plus, _) => {
                in_exp = true;
                exp_plus = *plus;
            }
            _ => {}
        }
    }
    if in_exp {
        l.exponent = Some((exp_plus, exp_digits));
    }
    if let Some(slash) = tokens.iter().position(|t| matches!(t, Token::Slash)) {
        // The numerator: the digit run just before the slash; the whole
        // part: digits before it, separated by a literal.
        let before = &tokens[..slash];
        let mut num_start = before.len();
        while num_start > 0 && matches!(before[num_start - 1], Token::Digit(_)) {
            num_start -= 1;
        }
        let numerator: Vec<char> = before[num_start..].iter().filter_map(|t| if let Token::Digit(c) = t { Some(*c) } else { None }).collect();
        let whole: Vec<char> = before[..num_start].iter().filter_map(|t| if let Token::Digit(c) = t { Some(*c) } else { None }).collect();
        let after = &tokens[slash + 1..];
        let denominator = match after.iter().find_map(|t| if let Token::FixedDenominator(d) = t { Some(*d) } else { None }) {
            Some(d) => Denominator::Fixed(d),
            None => Denominator::Digits(after.iter().take_while(|t| matches!(t, Token::Digit(_))).filter_map(|t| if let Token::Digit(c) = t { Some(*c) } else { None }).collect()),
        };
        l.fraction = Some(FractionLayout { whole, numerator, denominator });
    }
    l
}

fn number_section(value: f64, section: &Section, out: &mut Formatted) -> String {
    let l = layout(section);
    if section.tokens.iter().any(|t| matches!(t, Token::General)) {
        return general_section(value, section, out);
    }
    if l.fraction.is_some() {
        return fraction_section(value, section, &l, out);
    }
    if let Some(text) = simple_number(value, section, &l) {
        return text;
    }
    number_section_with(value, section, l, out)
}

/// A plain number format (placeholders, a point, grouping, percent signs and
/// literals): the same text as `number_section_with` gives, without its
/// vectors of digits and strings (reading a large workbook formats every
/// cell). `None` for any other format.
fn simple_number(value: f64, section: &Section, l: &Layout) -> Option<String> {
    if l.exponent.is_some() || l.fraction.is_some() || l.scale != 0 {
        return None;
    }
    if !section.tokens.iter().all(|t| matches!(t, Token::Digit(_) | Token::Point | Token::Comma | Token::Percent | Token::Literal(_) | Token::Space(_))) {
        return None;
    }
    let v = value.abs() * 100f64.powi(l.percent);
    if !v.is_finite() {
        return None;
    }
    let n_frac = l.frac.len();
    let d = Decimal::of(v).round(n_frac as i32);
    let int_len = if d.is_zero() { 0 } else { d.point.max(0) as usize };
    let int_digit = |i: usize| d.digits.get(i).copied().unwrap_or(0);
    let frac_digit = |i: usize| {
        let at = d.point + i as i32;
        if at < 0 { 0 } else { d.digits.get(at as usize).copied().unwrap_or(0) }
    };
    let shows_zero = int_len == 0 && (0..n_frac).all(|i| frac_digit(i) == 0);
    let mut text = String::new();
    if value < 0.0 && !shows_zero {
        text.push('-');
    }
    let n_int = l.int.len();
    // The digits the integer placeholders show: those of the number, and a 0 for each
    // placeholder of 0 beyond them.
    let filled = if int_len >= n_int { int_len } else { int_len + l.int[..n_int - int_len].iter().filter(|c| **c == '0').count() };
    let total_int_digits = if n_int == 0 { 0 } else { filled };
    let mut emitted = 0usize;
    let mut last_significant = n_frac;
    while last_significant > 0 && frac_digit(last_significant - 1) == 0 && l.frac[last_significant - 1] != '0' {
        last_significant -= 1;
    }
    let (mut int_i, mut frac_i) = (0usize, 0usize);
    let mut point_seen = false;
    let mut int_written = false;
    for t in &section.tokens {
        match t {
            Token::Digit(_) if point_seen => {
                let place = l.frac[frac_i];
                if frac_i < last_significant || place == '0' {
                    text.push((b'0' + frac_digit(frac_i)) as char);
                } else if place == '?' {
                    text.push(' ');
                }
                frac_i += 1;
            }
            Token::Digit(_) => {
                let p = int_i;
                let from_right = n_int - p;
                let mut push_digit = |text: &mut String, digit: u8| {
                    if l.grouping && emitted > 0 && (total_int_digits - emitted) % 3 == 0 {
                        text.push(',');
                    }
                    emitted += 1;
                    text.push((b'0' + digit) as char);
                };
                if from_right <= int_len {
                    let last = int_len - from_right;
                    let first = if p == 0 { 0 } else { last };
                    for i in first..=last {
                        push_digit(&mut text, int_digit(i));
                    }
                } else {
                    match l.int[p] {
                        '0' => push_digit(&mut text, 0),
                        '?' => text.push(' '),
                        _ => {}
                    }
                }
                int_written = true;
                int_i += 1;
            }
            Token::Point => {
                if !int_written {
                    if int_len > 0 && n_int == 0 {
                        for i in 0..int_len {
                            text.push((b'0' + int_digit(i)) as char);
                        }
                    }
                    int_written = true;
                }
                point_seen = true;
                text.push('.');
            }
            Token::Literal(s) => text.push_str(s),
            Token::Space(_) => text.push(' '),
            Token::Percent => text.push('%'),
            _ => {}
        }
    }
    Some(text.replace('\0', ""))
}

fn number_section_with(value: f64, section: &Section, l: Layout, out: &mut Formatted) -> String {
    let negative = value < 0.0;
    let mut v = value.abs() * 100f64.powi(l.percent);
    v /= 1000f64.powi(l.scale);
    if !v.is_finite() {
        out.overflow = true;
        return String::new();
    }
    // The digits, rounded to the placeholders.
    let (d, exponent) = match &l.exponent {
        Some(_) => {
            let d = Decimal::of(v);
            if d.is_zero() {
                (d, 0)
            } else {
                // With # among several integer placeholders the exponent
                // is a multiple of their count (engineering notation);
                // otherwise one integer digit, zero-padded.
                let int_places = l.int.len().max(1) as i32;
                let group = int_places;
                let first = d.point - 1;
                let mut e = first.div_euclid(group) * group;
                let r = Decimal { digits: d.digits.clone(), point: first - e + 1 }.round(l.frac.len() as i32);
                // Rounded up past the group (9.995 → 10.00 with one digit):
                // the next exponent.
                let r = if r.point > group {
                    e += group;
                    Decimal { digits: r.digits.clone(), point: r.point - group }
                } else {
                    r
                };
                (r, e)
            }
        }
        None => (Decimal::of(v).round(l.frac.len() as i32), 0),
    };
    let int_digits: Vec<u8> = d.integer().into_iter().skip_while(|x| *x == 0).collect();
    // Zero in the exponent form fills every integer placeholder with 0.
    let int_digits = if l.exponent.is_some() && d.is_zero() { vec![0; l.int.len()] } else { int_digits };
    let frac_digits = d.fraction(l.frac.len());
    // The minus sign, unless the number shows as zero.
    let shows_zero = int_digits.is_empty() && frac_digits.iter().all(|x| *x == 0);
    let mut text = String::new();
    if negative && !shows_zero {
        text.push('-');
    }
    // Integer part: digits right-aligned to the placeholders; more digits
    // than placeholders go at the first.
    let n_int = l.int.len();
    let mut int_out: Vec<String> = vec![String::new(); n_int];
    if n_int > 0 {
        let extra = int_digits.len().saturating_sub(n_int);
        for p in 0..n_int {
            // Placeholder p (from the left) takes digit at int_digits.len() - (n_int - p).
            let from_right = n_int - p;
            if from_right <= int_digits.len() {
                let idx = int_digits.len() - from_right;
                let s: String = if p == 0 { int_digits[..=idx].iter().map(|x| (b'0' + x) as char).collect() } else { ((b'0' + int_digits[idx]) as char).to_string() };
                int_out[p] = s;
            } else {
                int_out[p] = match l.int[p] {
                    '0' => "0".into(),
                    '?' => " ".into(),
                    _ => String::new(),
                };
            }
        }
        let _ = extra;
    }
    // Grouping: a comma before a digit with a multiple of three digits
    // after it in the whole integer part.
    let total_int_digits: usize = int_out.iter().map(|p| p.chars().filter(|c| c.is_ascii_digit()).count()).sum();
    let mut emitted_int_digits = 0usize;
    // Walk the tokens, writing placeholders from the digits.
    let mut int_i = 0;
    let mut frac_i = 0;
    let mut in_exp = false;
    let mut exp_i = 0;
    let mut int_written = false;
    // Fraction digits with trailing zeros dropped for # and ?.
    let mut frac_shown = frac_digits.clone();
    let mut last_significant = frac_shown.len();
    while last_significant > 0 && frac_shown[last_significant - 1] == 0 && l.frac[last_significant - 1] != '0' {
        last_significant -= 1;
    }
    frac_shown.truncate(frac_shown.len());
    let (exp_plus, exp_places) = l.exponent.clone().unwrap_or((false, Vec::new()));
    let exp_digits: Vec<u8> = exponent.unsigned_abs().to_string().bytes().map(|b| b - b'0').collect();
    let mut point_seen = false;
    for t in &section.tokens {
        match t {
            Token::Digit(_) if in_exp => {
                // Exponent digits: at least the placeholders' count.
                let n = exp_places.len();
                let shown = exp_digits.len().max(n);
                if exp_i == 0 {
                    let pad = shown - exp_digits.len();
                    for k in 0..shown.saturating_sub(n - 1) {
                        if k < pad {
                            text.push(if exp_places[0] == '#' && k + 1 < shown { '\0' } else { '0' });
                        } else {
                            text.push((b'0' + exp_digits[k - pad]) as char);
                        }
                    }
                } else {
                    let k = shown - n + exp_i;
                    let pad = shown - exp_digits.len();
                    text.push(if k < pad { '0' } else { (b'0' + exp_digits[k - pad]) as char });
                }
                exp_i += 1;
            }
            Token::Digit(_) if point_seen => {
                let d = frac_shown[frac_i];
                let place = l.frac[frac_i];
                if frac_i < last_significant || place == '0' {
                    text.push((b'0' + d) as char);
                } else if place == '?' {
                    text.push(' ');
                }
                frac_i += 1;
            }
            Token::Digit(_) => {
                // This placeholder's share of the integer digits.
                for ch in int_out[int_i].chars() {
                    if ch.is_ascii_digit() {
                        if l.grouping && emitted_int_digits > 0 && (total_int_digits - emitted_int_digits) % 3 == 0 {
                            text.push(',');
                        }
                        emitted_int_digits += 1;
                    }
                    text.push(ch);
                }
                int_written = true;
                int_i += 1;
            }
            Token::Point => {
                if !int_written {
                    // No integer placeholders: the integer digits still show.
                    if !int_digits.is_empty() && n_int == 0 {
                        text.push_str(&int_digits.iter().map(|x| (b'0' + x) as char).collect::<String>());
                    }
                    int_written = true;
                }
                point_seen = true;
                text.push('.');
            }
            Token::Exponent(_, lower) => {
                in_exp = true;
                text.push(if *lower { 'e' } else { 'E' });
                if exponent < 0 {
                    text.push('-');
                } else if exp_plus {
                    text.push('+');
                }
            }
            Token::Literal(s) => text.push_str(s),
            Token::Space(_) => text.push(' '),
            Token::Fill(c) => out.fill = Some((text.len(), *c)),
            Token::Percent => text.push('%'),
            Token::Text => text.push_str(&general(value, 11)),
            Token::Comma => {}
            _ => {}
        }
    }
    let _ = int_written;
    text.replace('\0', "")
}

/// A section with General and literals around it (the minus sign, if
/// any, first). With `[DBNum]`, General shows 15 digits, in Japanese
/// numerals for levels 1–3.
fn general_section(value: f64, section: &Section, out: &mut Formatted) -> String {
    let mut text = String::new();
    let digits = match section.dbnum {
        Some(level) => {
            let plain = plain_digits(value.abs());
            // Too long to show in digits: the exponent form, as it is.
            if plain.len() > 255 { general(value.abs(), 11) } else { dbnum::general(&plain, level) }
        }
        None => general(value.abs(), 11),
    };
    if value < 0.0 && digits.chars().any(|c| c != '0' && c != '〇' && c != '０' && c != '.') {
        text.push('-');
    }
    for t in &section.tokens {
        match t {
            Token::General => text.push_str(&digits),
            Token::Literal(s) => text.push_str(s),
            Token::Space(_) => text.push(' '),
            Token::Fill(c) => out.fill = Some((text.len(), *c)),
            Token::Percent => text.push('%'),
            Token::Digit(c) => {
                if *c == '0' {
                    text.push('0')
                }
            }
            _ => {}
        }
    }
    text
}

/// The fraction p/q for x with q at most `max_q`, as Excel finds it: the
/// last convergent of the continued fraction within the bound.
fn best_fraction(x: f64, max_q: u64) -> (u64, u64) {
    if x == 0.0 {
        return (0, 1);
    }
    let (mut p0, mut q0, mut p1, mut q1) = (1u64, 0u64, x.floor() as u64, 1u64);
    let mut v = x;
    loop {
        let frac = v - v.floor();
        if frac.abs() < 1e-12 || (x - p1 as f64 / q1 as f64).abs() < 1e-15 {
            return (p1, q1);
        }
        v = 1.0 / frac;
        if !v.is_finite() || v > 1e15 {
            return (p1, q1);
        }
        let a = v.floor() as u64;
        let p2 = a.saturating_mul(p1).saturating_add(p0);
        let q2 = a.saturating_mul(q1).saturating_add(q0);
        if q2 > max_q {
            return (p1, q1);
        }
        (p0, q0, p1, q1) = (p1, q1, p2, q2);
    }
}

fn fraction_section(value: f64, section: &Section, l: &Layout, out: &mut Formatted) -> String {
    let f = l.fraction.as_ref().unwrap();
    let x = value.abs() * 100f64.powi(l.percent);
    let has_whole = !f.whole.is_empty();
    let (mut whole, rest) = if has_whole { (x.trunc(), x - x.trunc()) } else { (0.0, x) };
    let (mut num, den) = match &f.denominator {
        Denominator::Fixed(q) => (((rest * *q as f64) + 0.5).floor() as u64, *q),
        Denominator::Digits(d) => {
            let max_q = 10u64.pow(d.len().clamp(1, 9) as u32) - 1;
            best_fraction(rest, max_q)
        }
    };
    if has_whole && num == den && den != 0 {
        whole += 1.0;
        num = 0;
    }
    // Excel shows ### for a numerator it cannot hold: over 32767 with no whole part and one numerator place beside a fixed
    // denominator (TEXT(45000.75,"?/4") is #VALUE!, where "?/?" goes on to 1999999/2 for 999999.5).
    let fixed_den = matches!(f.denominator, Denominator::Fixed(_));
    if num > i32::MAX as u64 || (!has_whole && x >= 2_147_483_648.0) || (!has_whole && fixed_den && num > 32767 && f.numerator.len() == 1) {
        out.overflow = true;
        return String::new();
    }
    let zero_fraction = num == 0;
    let whole_text: String = if whole > 0.0 { Decimal::of(whole).integer().iter().map(|d| (b'0' + d) as char).collect() } else { String::new() };
    // Placeholders pad on the left: 0 with zeros, ? with spaces, # not at all.
    let pad_left = |s: String, places: &[char]| -> String {
        if s.len() >= places.len() {
            return s;
        }
        let missing = places.len() - s.len();
        let filler: String = places[..missing].iter().filter_map(|c| match c {
            '0' => Some('0'),
            '?' => Some(' '),
            _ => None,
        }).collect();
        format!("{filler}{s}")
    };
    let pad_right = |s: String, places: &[char]| -> String {
        if s.len() >= places.len() {
            return s;
        }
        let filler: String = places[s.len()..].iter().filter_map(|c| match c {
            '0' => Some('0'),
            '?' => Some(' '),
            _ => None,
        }).collect();
        format!("{s}{filler}")
    };
    let slash = section.tokens.iter().position(|t| matches!(t, Token::Slash)).unwrap();
    let mut num_start = slash;
    while num_start > 0 && matches!(section.tokens[num_start - 1], Token::Digit(_)) {
        num_start -= 1;
    }
    let whole_start = section.tokens[..num_start].iter().position(|t| matches!(t, Token::Digit(_)));
    let mut text = String::new();
    // The sign stays on a whole part of zero, but not on a lone 0/1.
    if value < 0.0 && (has_whole || !zero_fraction) {
        text.push('-');
    }
    let mut whole_done = false;
    // A zero fraction of # placeholders leaves nothing, not even the space before it.
    let blank_fraction = has_whole && zero_fraction && !f.numerator.iter().chain(match &f.denominator {
        Denominator::Digits(d) => d.iter(),
        Denominator::Fixed(_) => [].iter(),
    }).any(|c| *c == '?' || *c == '0');
    let mut i = 0;
    while i < section.tokens.len() {
        let t = &section.tokens[i];
        if Some(i) >= whole_start && i < num_start && matches!(t, Token::Digit(_)) {
            if !whole_done {
                let w = if !whole_text.is_empty() {
                    whole_text.clone()
                } else if zero_fraction {
                    // A value of zero shows a 0.
                    "0".into()
                } else {
                    String::new()
                };
                text.push_str(&pad_left(w, &f.whole));
                whole_done = true;
            }
            i += 1;
            continue;
        }
        if i == num_start {
            let den_places: Vec<char> = match &f.denominator {
                Denominator::Digits(d) => d.clone(),
                Denominator::Fixed(q) => q.to_string().chars().map(|_| '0').collect(),
            };
            if blank_fraction {
                while text.ends_with(' ') {
                    text.pop();
                }
            } else if has_whole && zero_fraction {
                // No fraction: spaces as wide as it.
                text.push_str(&" ".repeat(f.numerator.len() + 1 + den_places.len()));
            } else {
                text.push_str(&pad_left(num.to_string(), &f.numerator));
                text.push('/');
                match &f.denominator {
                    Denominator::Fixed(q) => text.push_str(&q.to_string()),
                    Denominator::Digits(_) => text.push_str(&pad_right(den.to_string(), &den_places)),
                }
            }
            i = slash + 1;
            while i < section.tokens.len() && matches!(section.tokens[i], Token::Digit(_) | Token::FixedDenominator(_)) {
                i += 1;
            }
            continue;
        }
        // A blank whole part before a # numerator takes its space with it.
        let skip_separator = i < num_start && whole_done && whole_text.is_empty() && !zero_fraction && f.numerator.iter().all(|c| *c == '#');
        match t {
            Token::Literal(s) if skip_separator && s.trim().is_empty() => {}
            Token::Literal(s) => text.push_str(s),
            Token::Space(_) => text.push(' '),
            Token::Fill(c) => out.fill = Some((text.len(), *c)),
            Token::Percent => text.push('%'),
            _ => {}
        }
        i += 1;
    }
    text
}

// ----- Dates and times -----------------------------------------------------

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const DAYS_JA: [&str; 7] = ["日", "月", "火", "水", "木", "金", "土"];

/// Japanese eras: the first day (y, m, d), the name, the initial.
const ERAS: [((i64, u32, u32), &str, &str); 5] = [((1868, 9, 8), "明治", "M"), ((1912, 7, 30), "大正", "T"), ((1926, 12, 25), "昭和", "S"), ((1989, 1, 8), "平成", "H"), ((2019, 5, 1), "令和", "R")];

fn era_of(y: i64, m: u32, d: u32) -> Option<(usize, i64)> {
    let mut found = None;
    for (i, (start, _, _)) in ERAS.iter().enumerate() {
        if (y, m, d) >= (start.0, start.1, start.2) {
            found = Some((i, y - start.0 + 1));
        }
    }
    found
}

fn date_section(value: f64, section: &Section, date1904: bool, mut out: Formatted) -> Formatted {
    // A lone elapsed-time part ([h], [mm], [ss]) takes negative values too.
    let parts: Vec<&DatePart> = section.tokens.iter().filter_map(|t| if let Token::Date(p) = t { Some(p) } else { None }).collect();
    if let [p @ (DatePart::ElapsedHours(_) | DatePart::ElapsedMinutes(_) | DatePart::ElapsedSeconds(_))] = parts[..]
        && value < 0.0
    {
        let mut positive = date_section(-value, section, date1904, out.clone());
        let _ = p;
        positive.text.insert(0, '-');
        return positive;
    }
    let max = if date1904 { 2_957_003.0 } else { 2_958_465.999_999_99 };
    // The 1904 date system shows a negative serial as a minus sign before the
    // date of its size (-1 is "-1904/01/02"), as Excel does.
    if date1904 && value < 0.0 && -value <= max + 1.0 {
        let mut positive = date_section(-value, section, date1904, out);
        if !positive.overflow {
            if let Some((pos, char)) = positive.fill {
                positive.fill = Some((pos + 1, char));
            }
            positive.text.insert(0, '-');
        }
        return positive;
    }
    if value < 0.0 || value > max + 1.0 {
        out.overflow = true;
        return out;
    }
    // Round to the smallest unit shown: fractions of a second, or seconds.
    let decimals = section.tokens.iter().find_map(|t| if let Token::Date(DatePart::SubSecond(n)) = t { Some(*n) } else { None }).unwrap_or(0);
    let unit = 86_400.0 * 10f64.powi(decimals as i32);
    let ticks = (value * unit).round();
    let day = (ticks / unit).floor();
    let in_day = ticks - day * unit; // ticks within the day
    let total_seconds_scaled = in_day as i64;
    let scale = 10i64.pow(decimals as u32);
    let secs = total_seconds_scaled / scale;
    let sub = total_seconds_scaled % scale;
    let (h, mi, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    let day_i = day as i64;
    let (y, mo, d) = if !date1904 && day_i == 0 { (1900, 1, 0) } else { date_from_serial(day_i, date1904) };
    let dow = {
        let s = if date1904 { day_i + 1462 } else { day_i };
        ((s + 6).rem_euclid(7)) as usize
    };
    let twelve = section.tokens.iter().any(|t| matches!(t, Token::Date(DatePart::AmPm(_))));
    let hour12 = |h: i64| {
        let h = h % 12;
        if h == 0 { 12 } else { h }
    };
    // The names of the section's locale (`[$-407]`), when it is not English or Japanese.
    let names = section.locale.and_then(locale::names);
    // Elapsed totals.
    let total_seconds_all = day_i * 86_400 + secs;
    let mut text = String::new();
    // Numbers in the section's numerals, if [DBNum].
    let num = |s: String, year: bool| -> String {
        match section.dbnum {
            Some(level @ 1..=3) if year => dbnum::digits(&s, level),
            Some(level @ 1..=3) => dbnum::date_number(&s, level),
            _ => s,
        }
    };
    for t in &section.tokens {
        match t {
            Token::Date(p) => match p {
                DatePart::Year(n) => {
                    if *n <= 2 {
                        text.push_str(&num(format!("{:02}", y.rem_euclid(100)), true));
                    } else {
                        text.push_str(&num(format!("{:04}", y), true));
                    }
                }
                DatePart::Month(n) => match n {
                    1 => text.push_str(&num(mo.to_string(), false)),
                    2 => text.push_str(&num(format!("{:02}", mo), false)),
                    3 => match names {
                        Some(n) => text.push_str(n.months_short[mo as usize - 1]),
                        None => text.push_str(&MONTHS[mo as usize - 1][..3]),
                    },
                    4 => match names {
                        Some(n) => text.push_str(n.months[mo as usize - 1]),
                        None => text.push_str(MONTHS[mo as usize - 1]),
                    },
                    _ => match names {
                        Some(n) => text.extend(n.months[mo as usize - 1].chars().next()),
                        None => text.push_str(&MONTHS[mo as usize - 1][..1]),
                    },
                },
                DatePart::Day(n) => match n {
                    1 => text.push_str(&num(d.to_string(), false)),
                    2 => text.push_str(&num(format!("{:02}", d), false)),
                    3 => match names {
                        Some(n) => text.push_str(n.days_short[dow]),
                        None => text.push_str(&DAYS[dow][..3]),
                    },
                    _ => match names {
                        Some(n) => text.push_str(n.days[dow]),
                        None => text.push_str(DAYS[dow]),
                    },
                },
                DatePart::JaDay(n) => {
                    text.push_str(DAYS_JA[dow]);
                    if *n >= 4 {
                        text.push_str("曜日");
                    }
                }
                DatePart::Hour(n) => {
                    let v = if twelve { hour12(h) } else { h };
                    if *n >= 2 { text.push_str(&num(format!("{:02}", v), false)) } else { text.push_str(&num(v.to_string(), false)) }
                }
                DatePart::Minute(n) => {
                    if *n >= 2 { text.push_str(&num(format!("{:02}", mi), false)) } else { text.push_str(&num(mi.to_string(), false)) }
                }
                DatePart::Second(n) => {
                    if *n >= 2 { text.push_str(&num(format!("{:02}", s), false)) } else { text.push_str(&num(s.to_string(), false)) }
                }
                DatePart::SubSecond(n) => {
                    text.push('.');
                    text.push_str(&format!("{:0width$}", sub, width = *n as usize));
                }
                DatePart::ElapsedHours(n) => text.push_str(&format!("{:0width$}", total_seconds_all / 3600, width = *n as usize)),
                DatePart::ElapsedMinutes(n) => text.push_str(&format!("{:0width$}", total_seconds_all / 60, width = *n as usize)),
                DatePart::ElapsedSeconds(n) => text.push_str(&format!("{:0width$}", total_seconds_all, width = *n as usize)),
                DatePart::AmPm(style) => {
                    let pm = h >= 12;
                    let own = names.and_then(|n| n.am_pm).filter(|_| matches!(style, parse::AmPm::Upper | parse::AmPm::Lower));
                    let s = match style {
                        _ if own.is_some() => if pm { own.map_or("PM", |o| o.1) } else { own.map_or("AM", |o| o.0) },
                        parse::AmPm::Upper => if pm { "PM" } else { "AM" },
                        parse::AmPm::Lower => if pm { "pm" } else { "am" },
                        parse::AmPm::A => if pm { "P" } else { "A" },
                        parse::AmPm::LowerA => if pm { "p" } else { "a" },
                        parse::AmPm::Ja => if pm { "午後" } else { "午前" },
                        parse::AmPm::Zh => if pm { "下午" } else { "上午" },
                    };
                    text.push_str(s);
                }
                DatePart::EraYear(n) => {
                    let (_, ey) = era_of(y, mo, d.max(1)).unwrap_or((0, y - 1867));
                    if *n >= 2 { text.push_str(&num(format!("{:02}", ey), false)) } else { text.push_str(&num(ey.to_string(), false)) }
                }
                DatePart::Era(n) => {
                    let (i, _) = era_of(y, mo, d.max(1)).unwrap_or((0, 0));
                    match n {
                        1 => text.push_str(ERAS[i].2),
                        2 => text.push_str(&ERAS[i].1[..3]),
                        _ => text.push_str(ERAS[i].1),
                    }
                }
                DatePart::Buddhist(n) => {
                    let by = y + 543;
                    if *n <= 2 {
                        let s = format!("{:04}", by);
                        if s.len() >= 4 {
                            text.push_str(&s[2..4]);
                        } else {
                            text.push_str(&format!("{:02}", by.rem_euclid(100)));
                        }
                    } else {
                        text.push_str(&by.to_string());
                    }
                }
            },
            Token::Literal(l) => text.push_str(l),
            Token::Space(_) => text.push(' '),
            Token::Fill(c) => out.fill = Some((text.len(), *c)),
            Token::Digit(c) => {
                if *c == '0' {
                    text.push('0');
                }
            }
            Token::Point => text.push('.'),
            _ => {}
        }
    }
    let _ = days_in_month;
    out.text = text;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The digits by the standard library's formatting, to check `Decimal::of_exact`.
    fn digits_by_format(v: f64) -> Decimal {
        let s = format!("{:.14e}", v);
        let (mantissa, exponent) = s.split_once('e').unwrap();
        let exponent: i32 = exponent.parse().unwrap();
        let mut digits: Vec<u8> = mantissa.bytes().filter(|b| b.is_ascii_digit()).map(|b| b - b'0').collect();
        while digits.last() == Some(&0) {
            digits.pop();
        }
        Decimal { digits, point: exponent + 1 }
    }

    #[test]
    fn a_locale_gives_its_names_to_months_days_and_am_pm() {
        // 2026-10-13 is a Tuesday; 14:05.
        let v = CellValue::Number(46308.0 + 14.0 / 24.0 + 5.0 / 1440.0);
        let show = |code: &str| format(&v, code, false).text;
        assert_eq!(show("[$-409]dddd, mmmm d, yyyy"), "Tuesday, October 13, 2026");
        assert_eq!(show("[$-407]dddd, d. mmmm yyyy"), "Dienstag, 13. Oktober 2026");
        assert_eq!(show("[$-407]ddd d mmm"), "Di 13 Okt");
        assert_eq!(show("[$-40C]dddd d mmmm yyyy"), "mardi 13 octobre 2026");
        assert_eq!(show("[$-40A]dddd, d \\d\\e mmmm"), "martes, 13 de octubre");
        assert_eq!(show("[$-412]yyyy mmmm d dddd"), "2026 10월 13 화요일");
        assert_eq!(show("[$-804]mmmm dddd"), "十月 星期二");
        assert_eq!(show("[$-412]h:mm AM/PM"), "2:05 오후");
        assert_eq!(show("[$-407]h:mm AM/PM"), "2:05 PM");
        assert_eq!(show("[$-de-DE]mmmm"), "Oktober");
        // Russian (LCID 0x0419 and ru-RU tag)
        assert_eq!(show("[$-419]dddd, d mmmm yyyy"), "вторник, 13 октябрь 2026");
        assert_eq!(show("[$-ru-RU]ddd d mmm"), "вт 13 окт");
        // Dutch (LCID 0x0413 and nl-NL tag)
        assert_eq!(show("[$-413]dddd d mmmm yyyy"), "dinsdag 13 oktober 2026");
        assert_eq!(show("[$-nl-NL]ddd d mmm"), "di 13 okt");
        // Polish (LCID 0x0415 and pl-PL tag)
        assert_eq!(show("[$-415]dddd, d mmmm yyyy"), "wtorek, 13 październik 2026");
        assert_eq!(show("[$-pl-PL]ddd d mmm"), "wt 13 paź");
        // Swedish (LCID 0x041D and sv-SE tag)
        assert_eq!(show("[$-41D]dddd d mmmm yyyy"), "tisdag 13 oktober 2026");
        assert_eq!(show("[$-sv-SE]ddd d mmm"), "tis 13 okt");
        // Turkish (LCID 0x041F and tr-TR tag)
        assert_eq!(show("[$-41F]d mmmm yyyy dddd"), "13 Ekim 2026 Salı");
        assert_eq!(show("[$-tr-TR]d mmm ddd"), "13 Eki Sal");
        // Arabic (LCID 0x0401)
        assert_eq!(show("[$-401]dddd d mmmm yyyy"), "الثلاثاء 13 أكتوبر 2026");
        assert_eq!(show("[$-401]h:mm AM/PM"), "2:05 م");
        // Hindi (LCID 0x0439)
        assert_eq!(show("[$-439]dddd, d mmmm yyyy"), "मंगलवार, 13 अक्टूबर 2026");
        assert_eq!(show("[$-439]h:mm AM/PM"), "2:05 अपराह्न");
        // Currency symbols with locales
        let cur_val = CellValue::Number(1234.5);
        let show_cur = |code: &str| format(&cur_val, code, false).text;
        assert_eq!(show_cur("[$₽-419] #,##0.00"), "₽ 1,234.50");
        assert_eq!(show_cur("#,##0.00\\ [$₽-419]"), "1,234.50 ₽");
        assert_eq!(show_cur("[$€-40C] #,##0.00"), "€ 1,234.50");
        assert_eq!(show_cur("[$£-809] #,##0.00"), "£ 1,234.50");
        // English and Japanese stay as they are.
        assert_eq!(show("[$-411]yyyy年m月d日 aaaa"), "2026年10月13日 火曜日");
    }

    #[test]
    fn simple_formats_give_what_the_general_way_gives() {
        let codes = [
            "0", "0.00", "0.0", "#,##0", "#,##0.00", "#,##0.0", "00000", "000.00", "#", "#.##", "#.00", ".00", ".##", "?.??", "0.0%", "0%", "0.00%", "#,##0%", "\"$\"#,##0.00", "¥#,##0", "#,##0 \"円\"", "0.0_);(0.0)", "0\"年\"", "#,###", "#,##0.000", "00.00", "0.0#", "#0.0?", "# ??/", "#,##0;-#,##0", "0,0", "#,#0", "##,##0.0", "0.00 ", "\\¥#,##0",
        ];
        let mut x: u64 = 0x1234_5678_9ABC_DEF1;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let mut values: Vec<f64> = vec![0.0, -0.0, 0.5, -0.5, 1.5, 2.5, -2.5, 0.005, 0.015, 0.995, 9.995, 99.5, 999.5, 999.999, 1234567.891, -1234567.891, 0.0001, -0.0001, 1e-9, 1e15, 123456789012345.0, 1e20, 0.1 + 0.2, 2.675, 1.005, 100.0, 1000.0, 12.0, -12.0, 0.999999999999999];
        for _ in 0..4000 {
            let r = next();
            let size = 10f64.powi((r % 14) as i32 - 4);
            values.push(((next() % 2_000_000) as f64 / 1000.0 - 1000.0) * size);
            values.push((next() % 100_000_000) as f64 / 100.0);
        }
        let mut compared = 0;
        for code in codes {
            let sections = parsed(code);
            for section in sections.iter().take(1) {
                if section.tokens.iter().any(|t| matches!(t, Token::General)) {
                    continue;
                }
                for &v in &values {
                    let l = layout(section);
                    if let Some(fast) = simple_number(v, section, &l) {
                        let mut out = Formatted::default();
                        let slow = number_section_with(v, section, layout(section), &mut out);
                        assert_eq!(fast, slow, "{code} {v:e}");
                        compared += 1;
                    }
                }
            }
        }
        assert!(compared > 100_000, "{compared}");
    }

    #[test]
    fn exact_digits_are_the_same_as_the_standard_formats() {
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let mut checked = 0;
        let mut check = |v: f64| {
            if let Some(d) = Decimal::of_exact(v) {
                let want = digits_by_format(v);
                assert_eq!((d.digits.clone(), d.point), (want.digits.clone(), want.point), "{v:e}");
                checked += 1;
            }
        };
        // Random numbers of every size in range, amounts with decimals, halves, and neighbours of 15-digit ties.
        for _ in 0..300_000 {
            let r = next();
            let size = 10f64.powi((r % 20) as i32 - 5);
            check(f64::from_bits(0x3FF0_0000_0000_0000 | (next() >> 12)) * size / 2.0);
            check(((next() % 10_000_000) as f64) / 100.0);
            check(((next() % 1_000_000) as f64) / 8.0 + 0.5);
            check(((next() % 100_000_000_000_000) as f64) / 10.0 + 0.5);
        }
        for v in [0.1 + 0.2, 2.675, 1.005, 0.5, 1.5, 2.5, 999_999_999_999_999.9, 99_999.999_999_999_9, 1e-5, 0.000_012_345, 123_456_789_012_345.6, 0.999_999_999_999_999_9] {
            check(v);
            check(f64::from_bits(v.to_bits() + 1));
            check(f64::from_bits(v.to_bits() - 1));
        }
        assert!(checked > 1_000_000, "{checked}");
    }

    fn f(v: f64, code: &str) -> String {
        format(&CellValue::Number(v), code, false).text
    }

    #[test]
    fn negative_dates_in_the_1904_system() {
        // Excel for Mac's display (coverage XSYS-1904-00031..39).
        for (v, code, shown) in [
            (-1.0, "yyyy/mm/dd", "-1904/01/02"),
            (-1.0, "yyyy/m/d h:mm", "-1904/1/2 0:00"),
            (-1.0, "ggge\"年\"m\"月\"d\"日\"", "-明治37年1月2日"),
            (-1.0, "[h]:mm", "-24:00"),
            (-1.0, "General", "-1"),
            (-0.5, "yyyy/mm/dd", "-1904/01/01"),
            (-0.5, "yyyy/m/d h:mm", "-1904/1/1 12:00"),
            (-0.5, "ggge\"年\"m\"月\"d\"日\"", "-明治37年1月1日"),
            (-0.5, "[h]:mm", "-12:00"),
        ] {
            assert_eq!(format(&CellValue::Number(v), code, true).text, shown, "{v} {code}");
        }
        // The 1900 system still cannot show them.
        assert!(format(&CellValue::Number(-1.0), "yyyy/mm/dd", false).overflow);
    }

    #[test]
    fn numbers() {
        assert_eq!(f(0.995, "0.00"), "1.00");
        assert_eq!(f(1234.5678, "#,##0.00"), "1,234.57");
        assert_eq!(f(-1234.5678, "#,##0"), "-1,235");
        assert_eq!(f(0.5, "000-0000"), "000-0001");
        assert_eq!(f(-0.001, "#.00"), ".00");
        assert_eq!(f(-1.0, ".00"), "-1.00");
        assert_eq!(f(1e20, "0"), "100000000000000000000");
        assert_eq!(f(0.1, "%0"), "%10");
        assert_eq!(f(100.0, "#,,"), "");
        assert_eq!(f(1e20, "0,0"), "100,000,000,000,000,000,000");
        assert_eq!(f(-2.5, "#,##0,,\"M\""), "0M");
        assert_eq!(f(-1234.5678, "0.00E+00\" 円\""), "-1.23E+03 円");
        assert_eq!(f(-12345678.9, "0.00E-00"), "-1.23E07");
        assert_eq!(f(-1234.5678, "00.00E+00"), "-12.35E+02");
        assert_eq!(f(1.0, "##0.0E+0"), "1.0E+0");
    }

    #[test]
    fn generals() {
        assert_eq!(general(0.3333333333333333, 11), "0.333333333");
        assert_eq!(general(1234567890.0,11),"1234567890");
        assert_eq!(general(12345678901.0,11),"12345678901");
        assert_eq!(general(-12345678901.0,11),"-12345678901");
        assert_eq!(general(123456789012.0, 11), "1.23457E+11");
        assert_eq!(general(1230000000000000.0, 11), "1.23E+15");
        assert_eq!(general(1e-10, 11), "1E-10");
        assert_eq!(general(-12345678.9, 11), "-12345678.9");
        assert_eq!(general(5e-324, 11), "0");
        assert_eq!(general(3.141592653589793, 11), "3.141592654");
    }

    #[test]
    fn sections_and_dates() {
        assert_eq!(f(-1.0, "[Red][<0]0;[Blue][>0]0;0"), "1");
        assert_eq!(f(-0.05, "[>=100]00000;\"small\""), "-small");
        assert_eq!(f(60.0, "yyyy/m/d"), "1900/2/29");
        assert_eq!(f(0.0, "aaaa"), "土曜日");
        assert_eq!(f(0.0, "ggge\"年\"m\"月\"d\"日\""), "明治33年1月0日");
        assert_eq!(f(1.0, "[h]:mm:ss"), "24:00:00");
        assert_eq!(f(0.0, "h:mm AM/PM"), "12:00 AM");
        assert_eq!(f(45351.0000115741, "yyyy-mm-dd hh:mm:ss"), "2024-02-29 00:00:01");
        assert_eq!(f(43585.0, "bb"), "62");
        assert!(format(&CellValue::Number(-1.0), "yyyy", false).overflow);
    }

    #[test]
    fn fractions() {
        assert_eq!(f(-1.0, "0 ?/3"), "-1    ");
        assert_eq!(f(5e-324, "0/0"), "0/1");
        assert_eq!(f(1.25, "# ?/?"), "1 1/4");
        assert_eq!(f(0.5, "?/4"), "2/4");
        assert_eq!(format(&CellValue::Number(1234.5678), "* # ?/?", false).text, "1234 4/7");
        assert_eq!(format(&CellValue::Number(0.5), "* # ?/?", false).text, " 1/2");
        assert_eq!(f(999999.5, "?/?"), "1999999/2");
        assert_eq!(f(12345678.0, "?/?"), "12345678/1");
        // A fixed denominator with one numerator place and no whole part holds no more than 32767 (### in a cell, #VALUE! in TEXT).
        assert!(format(&CellValue::Number(45000.75), "?/4", false).overflow);
        assert_eq!(f(8191.75, "?/4"), "32767/4");
    }

    #[test]
    fn test_round_to_display_precision() {
        // Standard decimals
        assert_eq!(round_to_display_precision(10.456, "0.00", false), 10.46);
        assert_eq!(round_to_display_precision(10.454, "0.00", false), 10.45);
        assert_eq!(round_to_display_precision(10.456, "0", false), 10.0);
        assert_eq!(round_to_display_precision(10.5, "0", false), 11.0);
        assert_eq!(round_to_display_precision(10.456, "#,##0.0", false), 10.5);

        // Negative numbers
        assert_eq!(round_to_display_precision(-10.456, "0.00", false), -10.46);
        assert_eq!(round_to_display_precision(-10.456, "#,##0.00;(#,##0.00)", false), -10.46);

        // Percentage
        assert_eq!(round_to_display_precision(0.12345, "0.0%", false), 0.123);
        assert_eq!(round_to_display_precision(0.1238, "0.0%", false), 0.124);

        // General returns unchanged
        assert_eq!(round_to_display_precision(10.456789, "General", false), 10.456789);
    }
}

/// The built-in number formats (`numFmtId` below 164) as Excel with the
/// Japanese locale shows them.
pub fn builtin(id: u32) -> Option<&'static str> {
    Some(match id {
        0 => "General",
        1 | 59 => "0",
        2 | 60 => "0.00",
        3 | 61 => "#,##0",
        4 | 62 => "#,##0.00",
        5 | 63 => "\"¥\"#,##0;\"¥\"\\-#,##0",
        6 | 64 => "\"¥\"#,##0;[Red]\"¥\"\\-#,##0",
        7 | 65 => "\"¥\"#,##0.00;\"¥\"\\-#,##0.00",
        8 | 66 => "\"¥\"#,##0.00;[Red]\"¥\"\\-#,##0.00",
        9 | 67 => "0%",
        10 | 68 => "0.00%",
        11 => "0.00E+00",
        12 | 69 => "# ?/?",
        13 | 70 => "# ??/??",
        14 | 71 | 72 => "yyyy/m/d",
        15 | 73 => "d-mmm-yy",
        16 | 74 => "d-mmm",
        17 | 75 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 | 76 => "h:mm",
        21 | 77 => "h:mm:ss",
        22 | 78 => "yyyy/m/d h:mm",
        23 => "\"$\"#,##0_);\\(\"$\"#,##0\\)",
        24 => "\"$\"#,##0_);[Red]\\(\"$\"#,##0\\)",
        25 => "\"$\"#,##0.00_);\\(\"$\"#,##0.00\\)",
        26 => "\"$\"#,##0.00_);[Red]\\(\"$\"#,##0.00\\)",
        27 | 36 | 50 | 57 => "[$-411]ge.m.d",
        28 | 29 | 51 | 54 | 58 => "[$-411]ggge\"年\"m\"月\"d\"日\"",
        30 => "m/d/yy",
        31 => "yyyy\"年\"m\"月\"d\"日\"",
        32 => "h\"時\"mm\"分\"",
        33 => "h\"時\"mm\"分\"ss\"秒\"",
        34 | 52 | 55 => "yyyy\"年\"m\"月\"",
        35 | 53 | 56 => "m\"月\"d\"日\"",
        37 => "#,##0;-#,##0",
        38 => "#,##0;[Red]-#,##0",
        39 => "#,##0.00;-#,##0.00",
        40 => "#,##0.00;[Red]-#,##0.00",
        41 => "_ * #,##0_ ;_ * \\-#,##0_ ;_ * \"-\"_ ;_ @_ ",
        42 => "_ \"¥\"* #,##0_ ;_ \"¥\"* \\-#,##0_ ;_ \"¥\"* \"-\"_ ;_ @_ ",
        43 => "_ * #,##0.00_ ;_ * \\-#,##0.00_ ;_ * \"-\"??_ ;_ @_ ",
        44 => "_ \"¥\"* #,##0.00_ ;_ \"¥\"* \\-#,##0.00_ ;_ \"¥\"* \"-\"??_ ;_ @_ ",
        45 | 79 => "mm:ss",
        46 | 80 => "[h]:mm:ss",
        47 | 81 => "mm:ss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => return None,
    })
}

/// A number in plain digits with 15 significant digits (zeros beyond),
/// never in the exponent form.
fn plain_digits(v: f64) -> String {
    Decimal::of(v).to_plain_string()
}

/// Rounds `value` to the precision that `code` displays, matching Excel's
/// "precision as displayed" (`calcPr/@fullPrecision="0"`) behavior.
///
/// If `code` is General, text, date/time, or unhandled format, the original
/// `value` is returned.
pub fn round_to_display_precision(value: f64, code: &str, _date1904: bool) -> f64 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    let sections = parsed(code);
    let Some((index, _)) = choose(value, &sections) else {
        return value;
    };
    let section = &sections[index];
    if section.is_date() {
        return value;
    }
    if section.tokens.iter().any(|t| matches!(t, Token::General)) {
        return value;
    }
    let l = layout(section);
    if l.fraction.is_some() {
        return value;
    }
    let sign = if value < 0.0 { "-" } else { "" };
    if let Some((_, _)) = &l.exponent {
        let percent_factor = 100f64.powi(l.percent);
        let scale_factor = 1000f64.powi(l.scale);
        let v = value.abs() * percent_factor / scale_factor;
        let d = Decimal::of(v);
        if d.is_zero() {
            return 0.0;
        }
        let int_places = l.int.len().max(1) as i32;
        let group = int_places;
        let first = d.point - 1;
        let mut e = first.div_euclid(group) * group;
        let r = Decimal { digits: d.digits.clone(), point: first - e + 1 }.round(l.frac.len() as i32);
        let r = if r.point > group {
            e += group;
            Decimal { digits: r.digits.clone(), point: r.point - group }
        } else {
            r
        };
        let s = r.to_plain_string();
        let exp_factor = 10f64.powi(e);
        let rounded: f64 = s.parse().unwrap_or(0.0) * exp_factor;
        let s_full = format!("{sign}{}", rounded * scale_factor / percent_factor);
        return s_full.parse().unwrap_or(value);
    }

    let decimals = l.frac.len() as i32 + 2 * l.percent - 3 * l.scale;
    let d = Decimal::of(value).round(decimals);
    let s = format!("{sign}{}", d.to_plain_string());
    s.parse().unwrap_or(value)
}

