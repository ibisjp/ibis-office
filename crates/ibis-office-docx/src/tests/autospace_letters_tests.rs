// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The quarter em between Japanese text and Latin letters (`autoSpaceDE`) is put where Word puts it (GitHub issue #166, Word's PDF of
//! `autospace-letters.docx`): beside é, α and а (Latin, Greek and Cyrillic letters) and the half-width katakana ｱ, not beside ー (U+30FC).

use super::flow_layout::Item;
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("docx::autospace_is_between_japanese_and_the_letters_of_word", autospace_is_between_japanese_and_the_letters_of_word)]
}

/// The space before and after the character `x` in each of the three places `test/editing/autospace-letters.docx` has it
/// (か x き, n x m, 0 x 0): what is between the previous character's advance and `x`, and between `x`'s advance and the next one.
fn gaps(chars: &[(char, f32, f32, f32)], x: char) -> Vec<(f32, f32, f32)> {
    // The last three (the report the file adds them to may have the character in its own text, ー in a word).
    let all: Vec<usize> = (1..chars.len() - 1).filter(|&i| chars[i].0 == x).collect();
    assert!(all.len() >= 3, "{x}: three places");
    let at = &all[all.len() - 3..];
    at.iter().map(|&i| (chars[i].3, chars[i].1 - chars[i - 1].1 - chars[i - 1].2, chars[i + 1].1 - chars[i].1 - chars[i].2)).collect()
}

fn autospace_is_between_japanese_and_the_letters_of_word() {
    let mut s = Scenario::open("test/editing/autospace-letters.docx");
    // Each character of the pages' text: (character, x, advance of its glyph, the size of its text).
    let chars: Vec<(char, f32, f32, f32)> = s.with(|e, _| {
        let pages = super::flow_layout::pages(e.flow().expect("a document"), &e.resources);
        let mut chars = Vec::new();
        for page in &pages.pages {
            for it in &page.items {
                if let Item::Text { x, text, offsets, font, size, .. } = it {
                    for (k, c) in text.chars().enumerate() {
                        chars.push((c, x + offsets[k], e.resources.text_width(&c.to_string(), font, *size), *size));
                    }
                }
            }
        }
        chars
    });
    let quarter = |size: f32| size / 4.0;
    let near = |a: f32, b: f32, what: &str| assert!((a - b).abs() < 0.3, "{what}: {a:.2} against {b:.2}");
    // é α а: the quarter em beside them from Japanese text (か x き), none between Latin letters or digits.
    for x in ['\u{e9}', '\u{3b1}', '\u{430}'] {
        let g = gaps(&chars, x);
        near(g[0].1, quarter(g[0].0), &format!("{x} after the kana"));
        near(g[0].2, quarter(g[0].0), &format!("{x} before the kana"));
        near(g[1].1, 0.0, &format!("{x} after n"));
        near(g[1].2, 0.0, &format!("{x} before m"));
        near(g[2].1, 0.0, &format!("{x} after 0"));
        near(g[2].2, 0.0, &format!("{x} before 0"));
    }
    // ー: none anywhere (it is of the common script, not Japanese).
    for (i, what) in [(1, "after"), (2, "before")] {
        for place in gaps(&chars, '\u{30fc}') {
            let v = if i == 1 { place.1 } else { place.2 };
            near(v, 0.0, &format!("U+30FC {what}"));
        }
    }
    // ｱ: the space from n and 0 (it is Japanese), none from the kana.
    let g = gaps(&chars, '\u{ff71}');
    near(g[0].1, 0.0, "U+FF71 after the kana");
    near(g[0].2, 0.0, "U+FF71 before the kana");
    for place in &g[1..] {
        near(place.1, quarter(place.0), "U+FF71 after a Latin letter or digit");
        near(place.2, quarter(place.0), "U+FF71 before a Latin letter or digit");
    }
}
