// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pairwise comparison of explicitly typed fields.

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use super::{array_value, err, number};
use crate::eval::convert::{date_from_serial, serial_from_date};
use crate::eval::{Ctx, Value};
use crate::formula::Expr;
use crate::japanese_date::{DateRange, parse_date};
use crate::value::CellError;

// The Unicode code charts cross-reference REPLACEMENT CHARACTER and SUB.
const ENCODING_SUBSTITUTES: &[char] = &['\u{fffd}', '\u{001a}'];

// Geta and black circle are used by some legacy Japanese conversions,
// including exports of Juki data. JPKI carries substitution positions separately.
const LEGACY_JAPANESE_SUBSTITUTES: &[char] = &['\u{3013}', '\u{25cf}'];

// Other conventional placeholders accepted by field matching.
const FIELD_PLACEHOLDERS: &[char] = &['\u{25a0}', '\u{003f}', '\u{ff1f}'];

pub(super) fn field_match(ctx: &mut Ctx, args: &[Expr]) -> Value {
    let args: Vec<_> = args.iter().map(|arg| ctx.eval_scalar_or_array(arg)).collect();
    // A single address is broadcast unchanged across the other argument.
    // Keep even a failed normalization, so it is not retried for each row.
    let fixed_addresses = if stable_text(&args[2]).is_some_and(|mode| mode.eq_ignore_ascii_case("address"))
        && args.iter().any(|value| matches!(value, Value::Array(_)))
    {
        [stable_text(&args[0]).map(address_key), stable_text(&args[1]).map(address_key)]
    } else {
        [None, None]
    };
    ctx.map_values(args, &mut |ctx, values| {
        if let Some(error) = values.iter().find_map(|value| match value {
            Value::Error(error) => Some(*error),
            _ => None,
        }) {
            return err(error);
        }
        let Value::Text(mode) = &values[2] else {
            return err(CellError::Value);
        };
        let (left, right) = (&values[0], &values[1]);
        let result = if mode.eq_ignore_ascii_case("name-with-kana") {
            let [_, _, _, left_kana, right_kana] = values else {
                return err(CellError::Value);
            };
            let name = super::name_match::compare(left, right, is_default_wildcard, |c| ctx.book.name_variant_cluster(c));
            let kana = super::kana_match::compare_optional(left_kana, right_kana);
            name.zip(kana).map(|(name, kana)| name.min(kana))
        } else if values.len() > 4 {
            return err(CellError::Value);
        } else if mode.eq_ignore_ascii_case("address") || mode.eq_ignore_ascii_case("name") {
            let extra = match values.get(3) {
                None | Some(Value::Empty) => "",
                Some(Value::Text(text)) => text,
                _ => return err(CellError::Value),
            };
            let extra: HashSet<char> = extra.chars().collect();
            let wildcard = |c| is_wildcard(c, &extra);
            if mode.eq_ignore_ascii_case("name") {
                super::name_match::compare(left, right, wildcard, |c| ctx.book.name_variant_cluster(c))
            } else {
                let left = normalized_address(left, &fixed_addresses[0]);
                let right = normalized_address(right, &fixed_addresses[1]);
                left.as_deref().zip(right.as_deref()).and_then(|(left, right)| compare_addresses(left, right, wildcard))
            }
        } else if values.len() > 3 {
            return err(CellError::Value);
        } else if mode.eq_ignore_ascii_case("date") {
            compare_dates(left, right, ctx.date1904())
        } else if mode.eq_ignore_ascii_case("phone") {
            super::contact_match::compare_phones(left, right)
        } else if mode.eq_ignore_ascii_case("postal") {
            super::contact_match::compare_postal_codes(left, right)
        } else if mode.eq_ignore_ascii_case("email") {
            compare_keys(left, right, |v| super::identifier_match::email_key(text_value(v)?))
        } else if mode.eq_ignore_ascii_case("corporate-number") {
            compare_keys(left, right, super::identifier_match::corporate_number)
        } else if mode.eq_ignore_ascii_case("mynumber") {
            compare_keys(left, right, super::identifier_match::mynumber)
        } else if mode.eq_ignore_ascii_case("drivers-license") {
            super::identifier_match::compare_drivers_licenses(left, right)
        } else if mode.eq_ignore_ascii_case("passport-jp") {
            compare_keys(left, right, super::identifier_match::japanese_passport_number)
        } else if mode.eq_ignore_ascii_case("kana") {
            text_value(left).zip(text_value(right)).and_then(|(a, b)| super::kana_match::compare(a, b))
        } else if mode.eq_ignore_ascii_case("gender") {
            compare_keys(left, right, |v| gender_key(text_value(v)?))
        } else {
            return err(CellError::Value);
        };
        result.map_or_else(|| err(CellError::NA), |rank| number(f64::from(rank)))
    })
}

/// Combine explicit field grades, retaining conflicts and missing evidence.
pub(super) fn record_match(ctx: &mut Ctx, args: &[Expr]) -> Value {
    if let [range] = args {
        return match ctx.eval_array(range) {
            Value::Array(a) if a.rows == 1 => record_grade(&a.data),
            Value::Array(a) if a.columns != 0 => {
                let grades = a.data.chunks_exact(a.columns).map(record_grade).collect();
                array_value(a.rows, 1, grades)
            }
            Value::Array(_) => err(CellError::Calc),
            value => record_grade(std::slice::from_ref(&value)),
        };
    }
    ctx.map_args(args, |_, values| record_grade(values))
}

fn record_grade(values: &[Value]) -> Value {
    let mut rank = 2.0_f64;
    let mut compared = false;
    for value in values {
        match value {
            Value::Number(n @ (0.0 | 1.0 | 2.0)) => {
                compared = true;
                rank = rank.min(*n);
            }
            Value::Empty | Value::Error(CellError::NA) => rank = rank.min(1.0),
            Value::Text(text) if text.trim().is_empty() => rank = rank.min(1.0),
            Value::Error(error) => return err(*error),
            _ => return err(CellError::Value),
        }
    }
    if compared { number(rank) } else { err(CellError::NA) }
}

fn compare_keys<T: Eq>(left: &Value, right: &Value, key: impl Fn(&Value) -> Option<T>) -> Option<u8> {
    Some(if key(left)? == key(right)? { 2 } else { 0 })
}

fn text_value(value: &Value) -> Option<&str> {
    match value {
        Value::Text(text) => Some(text),
        _ => None,
    }
}

fn compare_dates(left: &Value, right: &Value, date1904: bool) -> Option<u8> {
    let left = date_range(left, date1904)?;
    let right = date_range(right, date1904)?;
    Some(if left.last < right.first || right.last < left.first {
        0
    } else if left.complete && right.complete && !left.inferred && !right.inferred {
        2
    } else {
        1
    })
}

fn date_range(value: &Value, date1904: bool) -> Option<DateRange> {
    match value {
        Value::Text(text) => parse_date(text, true).ok(),
        Value::Number(serial) => {
            let last_serial = serial_from_date(9999, 12, 31, date1904);
            if !(0.0..=last_serial as f64).contains(serial) || serial.fract() != 0.0 {
                return None;
            }
            if !date1904 && matches!(*serial as i64, 0 | 60) {
                return None;
            }
            let day = date_from_serial(*serial as i64, date1904);
            Some(DateRange {
                first: day,
                last: day,
                inferred: false,
                complete: true,
            })
        }
        _ => None,
    }
}

pub(super) fn fold_ascii_width(c: char) -> char {
    match c {
        '！'..='～' => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
        _ => c,
    }
}

fn gender_key(text: &str) -> Option<bool> {
    let text: String = text.trim().chars().map(|c| fold_ascii_width(c).to_ascii_lowercase()).collect();
    match text.as_str() {
        "男" | "男性" | "m" | "male" => Some(true),
        "女" | "女性" | "f" | "female" => Some(false),
        _ => None,
    }
}

fn is_default_wildcard(c: char) -> bool {
    ENCODING_SUBSTITUTES.contains(&c) || LEGACY_JAPANESE_SUBSTITUTES.contains(&c) || FIELD_PLACEHOLDERS.contains(&c)
}

fn is_wildcard(c: char, extra: &HashSet<char>) -> bool {
    is_default_wildcard(c)
        || (c.is_alphabetic() || matches!(c, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' | '\u{100000}'..='\u{10fffd}')) && extra.contains(&c)
}

fn stable_text(value: &Value) -> Option<&str> {
    match value {
        Value::Array(array) if array.rows == 1 && array.columns == 1 => text_value(array.get(0, 0)),
        value => text_value(value),
    }
}

fn normalized_address<'a>(value: &Value, fixed: &'a Option<Option<String>>) -> Option<Cow<'a, str>> {
    match fixed {
        Some(address) => address.as_deref().map(Cow::Borrowed),
        None => address_key(text_value(value)?).map(Cow::Owned),
    }
}

fn compare_addresses(left: &str, right: &str, wildcard: impl Fn(char) -> bool + Copy) -> Option<u8> {
    let (left_place, left_tail) = address_parts(left);
    let (right_place, right_tail) = address_parts(right);
    if left == right && !left_place.chars().any(wildcard) {
        return Some(2);
    }
    if left_tail != right_tail {
        return Some(0);
    }

    let left = fuzzy_place(left_place, wildcard);
    let right = fuzzy_place(right_place, wildcard);
    if left.chars().count() != right.chars().count() {
        return Some(0);
    }
    let mut shared_letter = false;
    for (a, b) in left.chars().zip(right.chars()) {
        match (wildcard(a), wildcard(b)) {
            (false, false) if a == b => shared_letter |= a.is_alphabetic(),
            (true, true) => (),
            (true, false) if b.is_alphabetic() => (),
            (false, true) if a.is_alphabetic() => (),
            _ => return Some(0),
        }
    }
    Some(u8::from(shared_letter))
}

fn address_parts(key: &str) -> (&str, &str) {
    // Protect the numeric suffix and any building/room text that follows it.
    static NUMBER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[0-9]|[〇零一二三四五六七八九十百千]+(?:丁目|番地|番|号)").expect("valid address number pattern"));
    let boundary = NUMBER.find(key).map_or(key.len(), |m| m.start());
    let (place, tail) = key.split_at(boundary);
    (place.trim_end(), tail)
}

fn fuzzy_place(place: &str, wildcard: impl Fn(char) -> bool) -> String {
    // Keep the municipality itself in the comparison.
    let islands = [
        ("三宅島", "三宅村"),
        ("八丈島", "八丈町"),
        ("種子島", "西之表市"),
        ("種子島", "熊毛郡中種子町"),
        ("種子島", "熊毛郡南種子町"),
        ("種子島", "中種子町"),
        ("種子島", "南種子町"),
        ("奄美大島", "奄美市"),
        ("伊豆大島", "大島町"),
    ];
    let mut place = place.to_owned();
    for (island, municipality) in islands {
        if let Some(at) = place.find(island)
            && (at == 0 || place[..at].ends_with(['都', '県', '郡']))
            && place[at + island.len()..].starts_with(municipality)
            && !island.chars().any(&wildcard)
        {
            place.replace_range(at..at + island.len(), "");
            break;
        }
    }
    let mut after_oaza = false;
    let mut result = String::new();
    let mut rest = place.as_str();
    while let Some(c) = rest.chars().next() {
        if rest.starts_with("大字") && !wildcard('大') && !wildcard('字') {
            after_oaza = true;
            rest = &rest["大字".len()..];
            continue;
        }
        rest = &rest[c.len_utf8()..];
        if c == '字' && !wildcard(c) && (after_oaza || result.ends_with(['市', '区', '町', '村']) || result.is_empty()) {
            after_oaza = false;
            continue;
        }
        result.push(match c {
            'ヶ' | 'ケ' | 'ヵ' | 'が' if !wildcard(c) => 'が',
            _ => c,
        });
    }
    result
}

fn address_key(text: &str) -> Option<String> {
    // Anchor the pattern to the whole address. Only an explicit, complete
    // 丁目/番/号 sequence is interchangeable with the three-number form.
    // The prefix must end outside the numeral run and contain a Han or kana
    // character, so bare numeric identifiers do not take this path.
    static LABELED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\A(.*?[^0-9〇零一二三四五六七八九十百千\-‐‑−\s])\s*([0-9〇零一二三四五六七八九十百千]+)丁目\s*([0-9〇零一二三四五六七八九十百千]+)番\s*([0-9〇零一二三四五六七八九十百千]+)号\z")
            .expect("valid labeled address pattern")
    });
    static HYPHENATED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\A(.*?[^0-9〇零一二三四五六七八九十百千\-‐‑−\s])\s*([0-9]+)\s*[-‐‑−]\s*([0-9]+)\s*[-‐‑−]\s*([0-9]+)\z")
            .expect("valid hyphenated address pattern")
    });
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let folded: String = text
        .chars()
        .map(|c| match c {
            '０'..='９' | '－' => fold_ascii_width(c),
            _ => c,
        })
        .collect();
    if let Some(fields) = LABELED.captures(&folded).or_else(|| HYPHENATED.captures(&folded)) {
        let prefix = fields[1].trim_end();
        if prefix.chars().any(|c| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}'))
            && let (Some(chome), Some(block), Some(lot)) = (address_number(&fields[2]), address_number(&fields[3]), address_number(&fields[4]))
        {
            return Some(format!("{prefix}{chome}-{block}-{lot}"));
        }
    }
    Some(folded)
}

fn address_number(text: &str) -> Option<String> {
    if text.bytes().all(|b| b.is_ascii_digit()) {
        return Some(text.to_owned());
    }
    // Japanese positional units, in strictly descending order. Limit the
    // conversion to the small numerals used in these explicit address fields.
    let mut total = 0;
    let mut pending = None;
    let mut previous_unit = 10_000;
    for c in text.chars() {
        let c = if c == '零' { '〇' } else { c };
        if let Some(digit) = "〇一二三四五六七八九".chars().position(|d| d == c) {
            if pending.replace(digit).is_some() {
                return None;
            }
        } else {
            let unit = match c {
                '十' => 10,
                '百' => 100,
                '千' => 1000,
                _ => return None,
            };
            if unit >= previous_unit || pending == Some(0) {
                return None;
            }
            total += pending.take().unwrap_or(1) * unit;
            previous_unit = unit;
        }
    }
    Some((total + pending.unwrap_or(0)).to_string())
}
