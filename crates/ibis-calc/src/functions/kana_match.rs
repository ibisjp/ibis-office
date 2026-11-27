// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use super::text::KANA;
use crate::eval::Value;

pub(super) fn compare(left: &str, right: &str) -> Option<u8> {
    Some(compare_keys(&kana_key(left)?, &kana_key(right)?))
}

pub(super) fn compare_optional(left: &Value, right: &Value) -> Option<u8> {
    fn key(value: &Value) -> Option<Option<Vec<char>>> {
        match value {
            Value::Empty => Some(None),
            Value::Text(text) if text.trim().is_empty() => Some(None),
            Value::Text(text) => kana_key(text).map(Some),
            _ => None,
        }
    }
    Some(match (key(left)?, key(right)?) {
        (Some(a), Some(b)) => compare_keys(&a, &b),
        _ => 1,
    })
}

fn compare_keys(left: &[char], right: &[char]) -> u8 {
    let letters = |c: &&char| **c != ' ';
    if left.iter().filter(letters).eq(right.iter().filter(letters)) {
        2
    } else if long_vowel_key(left) == long_vowel_key(right) {
        1
    } else {
        0
    }
}

fn kana_key(input: &str) -> Option<Vec<char>> {
    let mut kana = Vec::new();
    let mut letters = false;
    let mut can_voice = false;
    for c in input.chars() {
        if c.is_whitespace() {
            if kana.last().is_some_and(|&last| last != ' ') {
                kana.push(' ');
            }
            can_voice = false;
            continue;
        }
        let c = match c {
            'ぁ'..='ゖ' | 'ゝ' | 'ゞ' => char::from_u32(c as u32 + 0x60)?,
            '\u{3099}' | '゛' => 'ﾞ',
            '\u{309a}' | '゜' => 'ﾟ',
            'ｦ'..='ﾝ' | '･' => KANA.iter().find(|(_, half)| half.chars().count() == 1 && half.starts_with(c))?.0,
            _ => c,
        };
        match c {
            'ﾞ' | 'ﾟ' if can_voice => {
                let base = kana.last_mut()?;
                let composed = match (*base, c) {
                    ('ワ', 'ﾞ') => Some('ヷ'),
                    ('ヰ', 'ﾞ') => Some('ヸ'),
                    ('ヱ', 'ﾞ') => Some('ヹ'),
                    ('ヲ', 'ﾞ') => Some('ヺ'),
                    ('ヽ', 'ﾞ') => Some('ヾ'),
                    _ => None,
                };
                if let Some(composed) = composed {
                    *base = composed;
                    can_voice = false;
                    continue;
                }
                let half = KANA.iter().find(|(full, half)| full == base && half.chars().count() == 1)?.1;
                *base = KANA
                    .iter()
                    .find(|(_, marked)| {
                        marked
                            .strip_prefix(half)
                            .is_some_and(|suffix| suffix.starts_with(c) && suffix.chars().count() == 1)
                    })?
                    .0;
                can_voice = false;
            }
            'ァ'..='ヺ' | 'ヽ' | 'ヾ' => {
                kana.push(c);
                letters = true;
                can_voice = true;
            }
            'ー' | '・' => {
                kana.push(c);
                can_voice = false;
            }
            _ => return None,
        }
    }
    letters.then_some(kana)
}

fn long_vowel_key(kana: &[char]) -> Vec<char> {
    let mut previous = None;
    kana.iter()
        .filter_map(|&c| {
            if c == ' ' {
                previous = None;
                return None;
            }
            let result = match (previous, c) {
                (Some('a'), 'ア') | (Some('i'), 'イ') | (Some('u'), 'ウ') | (Some('e'), 'エ' | 'イ') | (Some('o'), 'オ' | 'ウ') => 'ー',
                _ => c,
            };
            previous = vowel(c);
            Some(result)
        })
        .collect()
}

fn vowel(c: char) -> Option<char> {
    // Small kana have the vowel of the contracted syllable.
    for (vowel, row) in [
        ('a', "ァアカガサザタダナハバパマャヤラヮワヷ"),
        ('i', "ィイキギシジチヂニヒビピミリヰヸ"),
        ('u', "ゥウクグスズツヅヌフブプムュユルヴ"),
        ('e', "ェエケゲセゼテデネヘベペメレヱヹ"),
        ('o', "ォオコゴソゾトドノホボポモョヨロヲヺ"),
    ] {
        if row.contains(c) {
            return Some(vowel);
        }
    }
    None
}
