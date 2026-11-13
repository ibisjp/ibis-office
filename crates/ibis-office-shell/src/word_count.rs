// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Counting words and characters of a text (Word's 文字カウント): what the document's count panel and the workbook's
//! word-count panel both use, so it lives with what every format shares.

/// Word counts each CJK character as one word.
pub(crate) fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{3000}'..='\u{303F}' // CJK Symbols and Punctuation
        | '\u{3040}'..='\u{309F}' // Hiragana
        | '\u{30A0}'..='\u{30FF}' // Katakana
        | '\u{3400}'..='\u{4DBF}' // CJK Unified Ideographs Extension A
        | '\u{4E00}'..='\u{9FFF}' // CJK Unified Ideographs
        | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
        | '\u{AC00}'..='\u{D7AF}' // Hangul Syllables
        | '\u{FF01}'..='\u{FF60}' // Fullwidth Forms
        | '\u{FF61}'..='\u{FF9F}' // Halfwidth Katakana
    )
}

/// Count (words, characters without whitespace, characters with spaces) in `text`.
pub fn count_words_and_chars(text: &str) -> (usize, usize, usize) {
    let mut words = 0;
    let mut no_spaces = 0;
    let mut with_spaces = 0;
    let mut in_word = false;

    for c in text.chars() {
        if c == '\u{fffc}' {
            if in_word {
                words += 1;
                in_word = false;
            }
            continue;
        }
        if matches!(c, '\r' | '\n' | '\u{b}' | '\u{c}' | '\u{e}') {
            if in_word {
                words += 1;
                in_word = false;
            }
            continue;
        }

        with_spaces += 1;

        if c.is_whitespace() {
            if in_word {
                words += 1;
                in_word = false;
            }
        } else {
            no_spaces += 1;
            if is_cjk_char(c) {
                if in_word {
                    words += 1;
                    in_word = false;
                }
                words += 1;
            } else if c.is_alphanumeric() || c == '_' {
                in_word = true;
            } else {
                if in_word {
                    words += 1;
                    in_word = false;
                }
            }
        }
    }

    if in_word {
        words += 1;
    }

    (words, no_spaces, with_spaces)
}
