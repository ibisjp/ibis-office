// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use std::sync::LazyLock;

use regex::Regex;

use crate::eval::Value;

struct Name<'a> {
    primary: String,
    parts: Option<(&'a str, &'a str)>,
    old: Option<&'a str>,
}

fn parse(text: &str) -> Name<'_> {
    static FORMER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\A([^\s()]+)\s*\(\s*旧姓(?:\s*:\s*|\s+)?([^\s():]+)\s*\)\s*([^\s()]+)\z").expect("valid former surname pattern"));
    static MIDDLE_DOT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\A([^\s()・]+)\s*・\s*旧姓\s*:?\s*([^\s():]+)\s+([^\s()]+)\z").expect("valid middle-dot former surname pattern"));
    if let Some(fields) = FORMER.captures(text).or_else(|| MIDDLE_DOT.captures(text)) {
        let get = |i| fields.get(i).expect("required name group").as_str();
        return Name {
            primary: format!("{}{}", get(1), get(3)),
            parts: Some((get(1), get(3))),
            old: Some(get(2)),
        };
    }
    let mut words = text.split_whitespace();
    let parts = match (words.next(), words.next(), words.next()) {
        (Some(surname), Some(given), None) if !text.contains(['(', ')']) => Some((surname, given)),
        _ => None,
    };
    Name {
        primary: text.chars().filter(|c| !c.is_whitespace()).collect(),
        parts,
        old: None,
    }
}

pub(super) fn compare(left: &Value, right: &Value, wildcard: impl Fn(char) -> bool, cluster: impl Fn(char) -> Option<u32>) -> Option<u8> {
    let (Value::Text(left), Value::Text(right)) = (left, right) else {
        return None;
    };
    let folds = |s: &str| s.trim().replace('（', "(").replace('）', ")").replace('：', ":");
    let left = folds(left);
    let right = folds(right);
    if left.is_empty() || right.is_empty() {
        return None;
    }
    let left = parse(&left);
    let right = parse(&right);
    let compare = |a: &str, b: &str| compare_spelling(a, b, &wildcard, &cluster);
    let align = |a: &Name<'_>, b: &Name<'_>, a_old: bool, b_old: bool| {
        let form = |name: &Name<'_>, old| match (old, name.old, name.parts) {
            (true, Some(surname), Some((_, given))) => (format!("{surname}{given}"), Some(surname.chars().count())),
            _ => (name.primary.clone(), name.parts.map(|(s, _)| s.chars().count())),
        };
        let (a_text, a_split) = form(a, a_old);
        let (b_text, b_split) = form(b, b_old);
        if matches!((a_split, b_split), (Some(a), Some(b)) if a != b) {
            0
        } else {
            compare(&a_text, &b_text)
        }
    };
    let primary = align(&left, &right, false, false);
    if primary > 0 {
        return Some(if left.old.is_some() || right.old.is_some() { 1 } else { primary });
    }
    if (left.old.is_some() && align(&left, &right, true, false) > 0)
        || (right.old.is_some() && align(&left, &right, false, true) > 0)
        || (left.old.is_some() && right.old.is_some() && align(&left, &right, true, true) > 0)
    {
        return Some(1);
    }
    // Shared given name is only evidence of a possible surname change.
    if let (Some((ls, lg)), Some((rs, rg))) = (left.parts, right.parts)
        && ![ls, lg, rs, rg].iter().any(|s| s.chars().any(&wildcard))
        && [ls, lg, rs, rg].iter().all(|s| s.chars().all(char::is_alphabetic))
        && compare(lg, rg) > 0
    {
        return Some(1);
    }
    Some(0)
}

fn compare_spelling(a: &str, b: &str, wildcard: &impl Fn(char) -> bool, cluster: &impl Fn(char) -> Option<u32>) -> u8 {
    if a.chars().count() != b.chars().count() {
        return 0;
    }
    let mut uncertain = false;
    let mut evidence = false;
    for (a, b) in a.chars().zip(b.chars()) {
        match (wildcard(a), wildcard(b)) {
            (true, true) => uncertain = true,
            (true, false) if b.is_alphabetic() => uncertain = true,
            (false, true) if a.is_alphabetic() => uncertain = true,
            (false, false) if a == b => evidence |= a.is_alphabetic(),
            (false, false) if cluster(a).is_some_and(|id| cluster(b) == Some(id)) => {
                evidence = true;
                uncertain = true;
            }
            _ => return 0,
        }
    }
    match (evidence, uncertain) {
        (true, true) => 1,
        (true, false) => 2,
        (false, _) => 0,
    }
}
