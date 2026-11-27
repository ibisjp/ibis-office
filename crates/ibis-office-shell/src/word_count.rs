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

/// What Word's 文字カウント counts in one paragraph (GitHub issue 295; Word's numbers: `tasks/doc/requests/2026-11-25-295-word-count-result.md`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParagraphCounts {
    /// 単語数: a run of characters between spaces, tabs and line breaks is one word (`well-known,` and `3.14` are one each); each full-width character and each
    /// half-width katakana is one word of its own.
    pub words: usize,
    /// 全角文字 + 半角カタカナの数 (the characters that count as a word each); 半角英数の単語数 is `words - far_east`.
    pub far_east: usize,
    /// 文字数 (スペースを含めない).
    pub no_spaces: usize,
    /// 文字数 (スペースを含める): the above and the spaces, tabs, full-width spaces and line breaks (a page break and a column break are not characters here).
    pub with_spaces: usize,
}

impl ParagraphCounts {
    /// Word counts a paragraph (数 段落数) only if it has a character that is not a space, a tab or a line break: an empty paragraph, one of a page break only and
    /// one of spaces only are not counted.
    pub fn counts_as_paragraph(&self) -> bool {
        self.no_spaces > 0
    }

    pub fn add(&mut self, other: ParagraphCounts) {
        self.words += other.words;
        self.far_east += other.far_east;
        self.no_spaces += other.no_spaces;
        self.with_spaces += other.with_spaces;
    }
}

/// Count one paragraph's `text` as Word's 文字カウント does. In `text` a note's mark is U+0002 (one character, never a word of its own: `test` and the mark
/// after it are one word), a picture or object is U+FFFC (no character), a line break U+000B (a character with spaces, ends a word), a page break U+000C and a
/// column break U+000E (no character, end a word).
pub fn count_paragraph(text: &str) -> ParagraphCounts {
    let mut c = ParagraphCounts::default();
    let mut in_word = false;
    for ch in text.chars() {
        match ch {
            '\u{2}' => {
                c.no_spaces += 1;
                c.with_spaces += 1;
            }
            '\u{b}' => {
                c.with_spaces += 1;
                in_word = false;
            }
            '\u{fffc}' | '\u{c}' | '\u{e}' | '\r' | '\n' => in_word = false,
            _ if ch.is_whitespace() => {
                c.with_spaces += 1;
                in_word = false;
            }
            _ if is_cjk_char(ch) => {
                c.no_spaces += 1;
                c.with_spaces += 1;
                c.words += 1;
                c.far_east += 1;
                in_word = false;
            }
            _ => {
                c.no_spaces += 1;
                c.with_spaces += 1;
                if !in_word {
                    c.words += 1;
                    in_word = true;
                }
            }
        }
    }
    c
}
