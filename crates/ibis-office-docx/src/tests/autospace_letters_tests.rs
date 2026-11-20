// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The quarter em between Japanese text and Latin letters (`autoSpaceDE`) is put where Word puts it (GitHub issue #166, Word's PDF of
//! `autospace-letters.docx`): beside é, α and а (Latin, Greek and Cyrillic letters) and the half-width katakana ｱ, not beside ー (U+30FC).

use super::flow_layout::Item;
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::autospace_is_between_japanese_and_the_letters_of_word", autospace_is_between_japanese_and_the_letters_of_word as fn()),
        ("docx::autospace_is_a_quarter_of_the_scaled_em", autospace_is_a_quarter_of_the_scaled_em),
    ]
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

/// 「あ1あ」 of ＭＳ 明朝 11 pt at 100% and at 80% (`test/editing/autospace-scaled.docx`): the quarter em after the digit is of the size the character is set at,
/// 2.75 pt, and 2.2 pt at 80% (Word's XPS of corpus 016, Meiryo 11 pt at 80%: 2.16 pt between 「1」 and the kanji, 8.8 pt the character's width).
fn autospace_is_a_quarter_of_the_scaled_em() {
    let mut s = Scenario::open("test/editing/autospace-scaled.docx");
    // Each paragraph's line: the distance from the digit to the kana after it (the digit's own width and the quarter em).
    let widths: Vec<f32> = s.with(|e, _| {
        let pages = super::flow_layout::pages(e.flow().expect("a document"), &e.resources);
        let mut out = Vec::new();
        for it in pages.pages.iter().flat_map(|p| p.items.iter()) {
            if let Item::Line { stops, .. } = it {
                if stops.len() >= 3 {
                    out.push(stops[2].1 - stops[1].1);
                }
            }
        }
        out
    });
    assert_eq!(widths.len(), 2, "two lines: {widths:?}");
    // The digit's width is the same share of the em at 100% and at 80%: the difference of the two is 0.2 of 5.5 plus the quarters' difference 0.55 pt.
    let (full, scaled) = (widths[0], widths[1]);
    let quarter_full = 11.0 / 4.0;
    let digit = full - quarter_full;
    assert!(((scaled - digit * 0.8) - 2.2).abs() < 0.05, "the quarter em at 80% is 2.2 pt: {full:.2} {scaled:.2}");
}
