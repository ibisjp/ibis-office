// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use std::sync::LazyLock;

use regex::Regex;

use super::field_match::fold_ascii_width;
use crate::eval::Value;

#[derive(PartialEq, Eq)]
struct Phone {
    number: String,
    extension: Option<String>,
}

fn phone(value: &Value) -> Option<Phone> {
    static EXTENSION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(?:内線|ext\.?|#|x)[\t 　]*([0-9]+)\z").expect("valid extension pattern"));
    static JP_TRUNK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\A\+81[\t 　]*(?:[-‐‑−][\t 　]*)?\(0\)").expect("valid Japanese trunk pattern"));
    static MAIN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\A\+?(?:[0-9]+|\([0-9]+\))(?:[\t 　]*(?:[-‐‑−][\t 　]*)?(?:[0-9]+|\([0-9]+\)))*\z").expect("valid phone pattern"));

    let Value::Text(text) = value else { return None };
    let folded: String = text.trim().chars().map(fold_ascii_width).collect();
    let (main, extension) = match EXTENSION.captures(&folded) {
        Some(c) => (&folded[..c.get(0)?.start()], Some(c[1].to_owned())),
        None => (folded.as_str(), None),
    };
    let main = JP_TRUNK.replace(main.trim_end(), "+81");
    if !MAIN.is_match(&main) {
        return None;
    }
    let digits: String = main.chars().filter(char::is_ascii_digit).collect();
    let number = if main.starts_with('+') {
        if let Some(jp) = digits.strip_prefix("81") {
            japanese_national_number(jp)?;
        } else if digits.starts_with('0') || !(3..=15).contains(&digits.len()) {
            return None;
        }
        digits
    } else {
        let jp = digits.strip_prefix('0')?;
        japanese_national_number(jp)?;
        format!("81{jp}")
    };
    Some(Phone { number, extension })
}

fn japanese_national_number(digits: &str) -> Option<()> {
    ((9..=10).contains(&digits.len()) && !digits.starts_with('0')).then_some(())
}

pub(super) fn compare_phones(left: &Value, right: &Value) -> Option<u8> {
    let (left, right) = (phone(left)?, phone(right)?);
    Some(if left.number != right.number {
        0
    } else {
        match (left.extension, right.extension) {
            (Some(a), Some(b)) => {
                if a == b {
                    2
                } else {
                    0
                }
            }
            (None, None) => 2,
            _ => 1,
        }
    })
}

enum PostalCode {
    Complete(u32),
    NeedsLeadingZero(u32),
}

fn postal_code(value: &Value) -> Option<PostalCode> {
    static POSTAL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\A(?:〒[\t 　]*)?([0-9]{3})[\t 　]*[-‐‑−]?[\t 　]*([0-9]{4})\z").expect("valid postal code pattern"));
    match value {
        Value::Text(text) => {
            let folded: String = text.trim().chars().map(fold_ascii_width).collect();
            let c = POSTAL.captures(&folded)?;
            let prefix: u32 = c[1].parse().ok()?;
            let suffix: u32 = c[2].parse().ok()?;
            Some(PostalCode::Complete(prefix * 10_000 + suffix))
        }
        Value::Number(number) if number.fract() == 0.0 => match *number {
            n if (1_000_000.0..=9_999_999.0).contains(&n) => Some(PostalCode::Complete(n as u32)),
            n if (10_000.0..=999_999.0).contains(&n) => Some(PostalCode::NeedsLeadingZero(n as u32)),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn compare_postal_codes(left: &Value, right: &Value) -> Option<u8> {
    use PostalCode::{Complete, NeedsLeadingZero};
    match (postal_code(left)?, postal_code(right)?) {
        (Complete(a), Complete(b)) => Some(if a == b { 2 } else { 0 }),
        (Complete(a), NeedsLeadingZero(b)) | (NeedsLeadingZero(b), Complete(a)) => Some(u8::from(a == b)),
        (NeedsLeadingZero(_), NeedsLeadingZero(_)) => None,
    }
}
