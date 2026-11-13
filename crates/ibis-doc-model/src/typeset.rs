// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Where a line of text may end (禁則処理), shared by the document (DOCX), spreadsheet (XLSX) and presentation (PPTX) screens.
//!
//! The mechanism is one: which characters may not start or end a line (a `Kinsoku`), characters that are not separated
//! (no-break spaces, the pairs `――` `……` `‥‥`), and the places a line may end inside Latin text (after a space, a hyphen, a
//! dash or a soft hyphen: Unicode's line breaking rules, UAX #14). The lists of characters are each application's own.
//!
//! * DOCX: Word's default lists, ECMA-376 Part 1 (the lists of the 禁則文字の設定: Annex D and E for the standard level,
//!   H and I for the high level, `w:strictFirstAndLastChars`).
//! * PPTX and XLSX: the lists PowerPoint and Excel (Windows) use, which differ from Word's: they were read off their output
//!   (the placement of every character of the lists in a line of exactly 46 full-width characters).

/// The characters a line may not start with and the characters it may not end with.
#[derive(Clone, Copy, Debug)]
pub struct Kinsoku<'a> {
    pub no_start: &'a str,
    pub no_end: &'a str,
}

impl Kinsoku<'_> {
    pub fn cannot_start(&self, c: char) -> bool {
        self.no_start.contains(c)
    }
    pub fn cannot_end(&self, c: char) -> bool {
        self.no_end.contains(c)
    }
}

/// Word, standard level (ECMA-376 Part 1, Annex D): characters a line does not start with.
pub const WORD_NO_START: &str = "!%),.:;?]}¢°’”‰′″℃、。々〉》」』】〕゛゜ゝゞ・ヽヾ！％），．：；？］｝￠｡｣､･ﾞﾟ"; // i18n-ok（行頭禁則の文字の集合。文字の並び）
/// Word, high level (`w:strictFirstAndLastChars`, Annex H): the standard characters and the small kana, the long vowel mark and
/// the half-width small kana and long vowel mark.
pub const WORD_NO_START_STRICT_EXTRA: &str = "ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮヵヶーｧｨｩｪｫｬｭｮｯｰ"; // i18n-ok（行頭禁則の文字の集合。文字の並び）
/// Word, high level: the standard characters and the extra ones, in one string.
pub const WORD_NO_START_STRICT: &str = "!%),.:;?]}¢°’”‰′″℃、。々〉》」』】〕゛゜ゝゞ・ヽヾ！％），．：；？］｝￠｡｣､･ﾞﾟぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮヵヶーｧｨｩｪｫｬｭｮｯｰ"; // i18n-ok（行頭禁則の文字の集合。文字の並び）
/// Word (Annex E): characters a line does not end with.
pub const WORD_NO_END: &str = "$([\\{£¥‘“〈《「『【〔＄（［｛￡￥｢"; // i18n-ok（行末禁則の文字の集合。文字の並び）

/// PowerPoint (Windows): characters a line does not start with. Not `― 〜 ‐`, which it lets start a line; with `゛ ゜ ℃ ° ′ ″` and the
/// half-width `｡ ｣ ､ ･ ﾞ ｯ ｰ` (issue 68, P1).
pub const PPTX_NO_START: &str = "、。，．・：；？！ー゛゜℃°′″）」』】〕〉》］｝〙〗’”ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮヵヶ々ゝゞヽヾ%％),.:;?!]}｡｣､･ﾞｯｰ"; // i18n-ok（禁則処理の文字集合（表示文字ではない））
/// PowerPoint (Windows): characters a line does not end with (the opening brackets and `＄ ￥ $`, issue 68, P2).
pub const PPTX_NO_END: &str = "（「『【〔〈《［｛〘〖‘“([{＄￥$"; // i18n-ok（禁則処理の文字集合（表示文字ではない））

/// Excel (Windows): characters a line does not start with (issue 68, X2).
pub const XLSX_NO_START: &str = "、。，．・：；？！ー゛゜ゝゞヽヾ々ぁぃぅぇぉっゃゅょゎァィゥェォッャュョヮヵヶ％℃°′″%）］｝」』】〕〉》’”)]}.,:;!?｡｣､･ﾞｯｰ"; // i18n-ok（禁則処理の文字集合（表示文字ではない））
/// Excel (Windows): characters a line does not end with (issue 68, X1).
pub const XLSX_NO_END: &str = "（「『【〔〈《［｛＄￥$¥£#‘“([{"; // i18n-ok（禁則処理の文字集合（表示文字ではない））

pub const PPTX_KINSOKU: Kinsoku<'static> = Kinsoku { no_start: PPTX_NO_START, no_end: PPTX_NO_END };
pub const XLSX_KINSOKU: Kinsoku<'static> = Kinsoku { no_start: XLSX_NO_START, no_end: XLSX_NO_END };

/// A space that does not let the line end after it (UAX #14, class GL): NO-BREAK SPACE, NARROW NO-BREAK SPACE, FIGURE SPACE,
/// WORD JOINER and ZERO WIDTH NO-BREAK SPACE.
pub fn is_no_break_space(c: char) -> bool {
    matches!(c, '\u{00a0}' | '\u{202f}' | '\u{2007}' | '\u{2060}' | '\u{feff}')
}

/// Two characters that are not separated: `――` `……` `‥‥` (JLReq 分離禁止). The line may end before the pair, not between.
pub fn is_inseparable_pair(before: char, after: char) -> bool {
    before == after && matches!(before, '―' | '…' | '‥')
}

/// Whether a line may end after `c` inside Latin text: after a space (not a no-break one), a hyphen-minus (when `hyphen`), a
/// hyphen, an en dash, an em dash (UAX #14 classes HY, BA, B2) and a soft hyphen.
pub fn breaks_after_latin(c: char, hyphen: bool) -> bool {
    match c {
        '-' => hyphen,
        ' ' | '\u{2010}' | '\u{2013}' | '\u{2014}' | '\u{00ad}' | crate::flow::OPTIONAL_HYPHEN => true,
        _ => false,
    }
}

/// Whether a line may end between `before` and `after`. `kinsoku` is the lists to follow (none: no 禁則処理, only the places
/// Latin text allows). `wide` says whether a character is Japanese (a line may end on either side of one). `hyphen` says whether a
/// hyphen-minus ends a line.
pub fn may_break(before: char, after: char, kinsoku: Option<&Kinsoku>, wide: impl Fn(char) -> bool, hyphen: bool) -> bool {
    if is_no_break_space(before) || is_no_break_space(after) {
        return false;
    }
    if let Some(k) = kinsoku
        && (k.cannot_start(after) || k.cannot_end(before))
    {
        return false;
    }
    if is_inseparable_pair(before, after) {
        return false;
    }
    breaks_after_latin(before, hyphen) || wide(before) || wide(after)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wide(c: char) -> bool {
        c as u32 >= 0x2e80
    }

    #[test]
    fn word_lists_follow_the_standard() {
        let k = Kinsoku { no_start: WORD_NO_START, no_end: WORD_NO_END };
        // Not at the start of a line: ℃ ° ′ ″ ｣ ･ ﾞ ‰ ¢ ﾟ; may start one: ― … ‥ (issue 68, D6).
        for c in "℃°′″｣･ﾞ‰¢ﾟ".chars() {
            assert!(k.cannot_start(c), "{c}");
        }
        for c in "―…‥ーっ".chars() {
            assert!(!k.cannot_start(c), "{c}");
        }
        // Not at the end of a line: ＄ ￥ $ ¥ £ ｢ (D7).
        for c in "＄￥$¥£｢".chars() {
            assert!(k.cannot_end(c), "{c}");
        }
    }

    #[test]
    fn strict_is_standard_and_extra() {
        assert_eq!(WORD_NO_START_STRICT, format!("{WORD_NO_START}{WORD_NO_START_STRICT_EXTRA}"));
    }

    #[test]
    fn pairs_and_dashes() {
        let k = Kinsoku { no_start: WORD_NO_START, no_end: WORD_NO_END };
        assert!(!may_break('―', '―', Some(&k), wide, true));
        assert!(may_break('を', '―', Some(&k), wide, true));
        assert!(may_break('c', 'd', Some(&k), wide, true) == false);
        assert!(may_break('—', 'd', Some(&k), wide, true));
        assert!(may_break('–', '2', Some(&k), wide, true));
        assert!(!may_break('\u{a0}', 'M', Some(&k), wide, true));
        assert!(!may_break('1', '\u{202f}', Some(&k), wide, true));
        assert!(!may_break('/', 'b', Some(&k), wide, true));
    }
}
