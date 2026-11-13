// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Where a line of text may break, for the documents (Word), the slides (PowerPoint) and the sheets (Excel): the characters a line may not
//! start or end with (禁則), and where Latin text may break (a subset of Unicode's line breaking, UAX #14). The rule is one; each format has
//! its own lists, as each Microsoft Office application has (GitHub issue #68; `scripts/corpus/make_kinsoku_probe.py` makes the cases and
//! `scripts/corpus/kinsoku_compare.py` compares the lines with Office's PDFs).

/// Word's characters a line may not start with: ECMA-376 Part 1, Annex D (Japanese, the default), which Word's PDFs follow.
pub const WORD_NO_LINE_START: &str = "!%),.:;?]}¢°’”‰′″℃、。々〉》」』】〕゛゜ゝゞ・ヽヾ！％），．：；？］｝｡｣､･ﾞﾟ￠"; // i18n-ok（禁則の文字の集合）
/// The same with `w:strictFirstAndLastChars` (高): Annex H, the small kana, ー and the half-width ｧ to ｯ and ｰ added.
pub const WORD_STRICT_NO_LINE_START: &str = "!%),.:;?]}¢°’”‰′″℃、。々〉》」』】〕ぁぃぅぇぉっゃゅょゎ゛゜ゝゞァィゥェォッャュョヮヵヶ・ーヽヾ！％），．：；？］｝｡｣､･ｧｨｩｪｫｬｭｮｯｰﾞﾟ￠"; // i18n-ok（禁則の文字の集合）
/// Word's characters a line may not end with: Annexes E and I (the same list).
pub const WORD_NO_LINE_END: &str = "$([\\{£¥‘“〈《「『【〔＄（［｛｢￡￥"; // i18n-ok（禁則の文字の集合）

/// PowerPoint's characters a line may not start with (the default, `p:kinsoku` absent): Windows PowerPoint's PDFs of
/// `kinsoku-*-probe.pptx`. Word's strict list without ¢ (a line may start with it).
pub(crate) const POWERPOINT_NO_LINE_START: &str = "!%),.:;?]}°’”‰′″℃、。々〉》」』】〕ぁぃぅぇぉっゃゅょゎ゛゜ゝゞァィゥェォッャュョヮヵヶ・ーヽヾ！％），．：；？］｝｡｣､･ｧｨｩｪｫｬｭｮｯｰﾞﾟ￠"; // i18n-ok（禁則の文字の集合）
/// The characters a line may not start with in a run of Chinese (zh-CN, zh-TW) text: PowerPoint's list without ー (measured, PowerPoint 365,
/// test/editing/pptx-text-turn-matrix5.pptx slide 2: a line of zh-CN or zh-TW text may start with ー). Not measured: every other character of the
/// list (it is the Japanese list, kana and marks of it included; PowerPoint keeps its lists per language, `p:kinsoku@lang`, and the default
/// Chinese list may differ in more characters), and the line-end list (the Japanese one is used).
pub(crate) const POWERPOINT_CHINESE_NO_LINE_START: &str = "!%),.:;?]}°’”‰′″℃、。々〉》」』】〕ぁぃぅぇぉっゃゅょゎ゛゜ゝゞァィゥェォッャュョヮヵヶ・ヽヾ！％），．：；？］｝｡｣､･ｧｨｩｪｫｬｭｮｯｰﾞﾟ￠"; // i18n-ok（禁則の文字の集合）
/// PowerPoint's characters a line may not end with (the same PDFs): Word's list without £ ¥ (the backslash, a yen sign in a Japanese font, is in it).
pub(crate) const POWERPOINT_NO_LINE_END: &str = "$([\\{‘“〈《「『【〔＄（［｛｢￡￥"; // i18n-ok（禁則の文字の集合）

/// Excel's characters a line may not start with (Excel for Windows' PDFs of `kinsoku-*-probe.xlsx`): PowerPoint's list without ‰.
pub(crate) const EXCEL_NO_LINE_START: &str = "!%),.:;?]}°’”′″℃、。々〉》」』】〕ぁぃぅぇぉっゃゅょゎ゛゜ゝゞァィゥェォッャュョヮヵヶ・ーヽヾ！％），．：；？］｝｡｣､･ｧｨｩｪｫｬｭｮｯｰﾞﾟ￠"; // i18n-ok（禁則の文字の集合）
/// Excel's characters a line may not end with (the same PDFs): PowerPoint's list without ｢ ￡.
pub(crate) const EXCEL_NO_LINE_END: &str = "$([\\{‘“〈《「『【〔＄（［｛￥"; // i18n-ok（禁則の文字の集合）

/// The characters a line may not start or end with.
#[derive(Clone, Copy, Debug)]
pub struct Kinsoku<'a> {
    pub no_start: &'a str,
    pub no_end: &'a str,
}

impl Kinsoku<'static> {
    pub const POWERPOINT: Self = Kinsoku { no_start: POWERPOINT_NO_LINE_START, no_end: POWERPOINT_NO_LINE_END };
    pub(crate) const POWERPOINT_CHINESE: Self = Kinsoku { no_start: POWERPOINT_CHINESE_NO_LINE_START, no_end: POWERPOINT_NO_LINE_END };
    pub const EXCEL: Self = Kinsoku { no_start: EXCEL_NO_LINE_START, no_end: EXCEL_NO_LINE_END };
}

/// A character text breaks next to as it does next to a kanji (UAX #14's ID, and the marks of Japanese text). Not ※ (U+203B): in Word's PDF of
/// `issue54-symbols.docx` the symbols ‘’“”♂♀℃§☆★○●◎◇◆□■△▲▽▼※⇒⇔①…⑳ (none is above U+2E80) are one run that no line breaks inside until it
/// is longer than the line: 34 of them fill the second line to the edge (a break next to ※ would have left 175 pt empty and sent ⇒⇔①…⑳ down).
pub fn is_ideographic(c: char) -> bool {
    (c as u32 >= 0x2e80 && !matches!(c as u32, 0xfe00..=0xfe0f | 0xfeff | 0xfffc | 0xfffd)) || matches!(c, '―' | '…' | '‥')
}

/// A character of Unicode's East Asian Width "A" (ambiguous) below U+2E80: drawn wide in a Japanese font, and Excel breaks next to it as next
/// to a kanji (EastAsianWidth.txt of Unicode 15.1, by Python's `unicodedata`).
fn east_asian_ambiguous(c: char) -> bool {
    matches!(c as u32,
        0x00a1 | 0x00a4 | 0x00a7..=0x00a8 | 0x00aa | 0x00ad..=0x00ae | 0x00b0..=0x00b4 | 0x00b6..=0x00ba | 0x00bc..=0x00bf | 0x00c6 | 0x00d0 |
        0x00d7..=0x00d8 | 0x00de..=0x00e1 | 0x00e6 | 0x00e8..=0x00ea | 0x00ec..=0x00ed | 0x00f0 | 0x00f2..=0x00f3 | 0x00f7..=0x00fa | 0x00fc |
        0x00fe | 0x0101 | 0x0111 | 0x0113 | 0x011b | 0x0126..=0x0127 | 0x012b | 0x0131..=0x0133 | 0x0138 | 0x013f..=0x0142 | 0x0144 |
        0x0148..=0x014b | 0x014d | 0x0152..=0x0153 | 0x0166..=0x0167 | 0x016b | 0x01ce | 0x01d0 | 0x01d2 | 0x01d4 | 0x01d6 | 0x01d8 | 0x01da |
        0x01dc | 0x0251 | 0x0261 | 0x02c4 | 0x02c7 | 0x02c9..=0x02cb | 0x02cd | 0x02d0 | 0x02d8..=0x02db | 0x02dd | 0x02df | 0x0300..=0x036f |
        0x0391..=0x03a1 | 0x03a3..=0x03a9 | 0x03b1..=0x03c1 | 0x03c3..=0x03c9 | 0x0401 | 0x0410..=0x044f | 0x0451 | 0x2010 | 0x2013..=0x2016 |
        0x2018..=0x2019 | 0x201c..=0x201d | 0x2020..=0x2022 | 0x2024..=0x2027 | 0x2030 | 0x2032..=0x2033 | 0x2035 | 0x203b | 0x203e | 0x2074 |
        0x207f | 0x2081..=0x2084 | 0x20ac | 0x2103 | 0x2105 | 0x2109 | 0x2113 | 0x2116 | 0x2121..=0x2122 | 0x2126 | 0x212b | 0x2153..=0x2154 |
        0x215b..=0x215e | 0x2160..=0x216b | 0x2170..=0x2179 | 0x2189 | 0x2190..=0x2199 | 0x21b8..=0x21b9 | 0x21d2 | 0x21d4 | 0x21e7 | 0x2200 |
        0x2202..=0x2203 | 0x2207..=0x2208 | 0x220b | 0x220f | 0x2211 | 0x2215 | 0x221a | 0x221d..=0x2220 | 0x2223 | 0x2225 | 0x2227..=0x222c |
        0x222e | 0x2234..=0x2237 | 0x223c..=0x223d | 0x2248 | 0x224c | 0x2252 | 0x2260..=0x2261 | 0x2264..=0x2267 | 0x226a..=0x226b |
        0x226e..=0x226f | 0x2282..=0x2283 | 0x2286..=0x2287 | 0x2295 | 0x2299 | 0x22a5 | 0x22bf | 0x2312 | 0x2460..=0x24e9 | 0x24eb..=0x254b |
        0x2550..=0x2573 | 0x2580..=0x258f | 0x2592..=0x2595 | 0x25a0..=0x25a1 | 0x25a3..=0x25a9 | 0x25b2..=0x25b3 | 0x25b6..=0x25b7 |
        0x25bc..=0x25bd | 0x25c0..=0x25c1 | 0x25c6..=0x25c8 | 0x25cb | 0x25ce..=0x25d1 | 0x25e2..=0x25e5 | 0x25ef | 0x2605..=0x2606 | 0x2609 |
        0x260e..=0x260f | 0x261c | 0x261e | 0x2640 | 0x2642 | 0x2660..=0x2661 | 0x2663..=0x2665 | 0x2667..=0x266a | 0x266c..=0x266d | 0x266f |
        0x269e..=0x269f | 0x26bf | 0x26c6..=0x26cd | 0x26cf..=0x26d3 | 0x26d5..=0x26e1 | 0x26e3 | 0x26e8..=0x26e9 | 0x26eb..=0x26f1 | 0x26f4 |
        0x26f6..=0x26f9 | 0x26fb..=0x26fc | 0x26fe..=0x26ff | 0x273d | 0x2776..=0x277f | 0x2b56..=0x2b59
    )
}

/// The marks that stay together when two of them follow each other (JLReq 3.1.10, 分離禁止): ―― …… ‥‥.
fn inseparable(before: char, after: char) -> bool {
    before == after && matches!(before, '―' | '…' | '‥' | '—')
}

/// No-break spaces: a line breaks neither before nor after them (UAX #14 GL: U+00A0, U+202F, U+2007, U+2060).
fn glue(c: char) -> bool {
    matches!(c, '\u{a0}' | '\u{202f}' | '\u{2007}' | '\u{2060}' | '\u{feff}')
}

/// Options of the break between Latin characters. None of Word, PowerPoint and Excel breaks after a slash between Latin letters
/// ("a/b/c/d" and a URL go to the next line whole: their PDFs of `kinsoku-*-probe`).
#[derive(Clone, Copy, Debug, Default)]
pub struct LatinBreaks {
    /// A line may break anywhere in a Latin word (`latinLnBrk` on).
    pub anywhere: bool,
    /// A line breaks next to a character of East Asian Width "A" as next to a kanji, and inside ―― …… ‥‥ (Excel for Windows' PDFs of
    /// `kinsoku-*-probe.xlsx`: "1990‖–2000", "x²‖+y²", "―‖―", "l’é‖cole").
    pub ambiguous_wide: bool,
}

/// Whether a line may break between `before` and `after`, `kinsoku` being the characters a line may not start or end with (`None`: kinsoku
/// off). Next to a kanji or kana a line may break; between Latin characters, after a space, a hyphen, an en dash, an em dash and a soft
/// hyphen (UAX #14 BA, HY, B2), not next to a no-break space.
pub fn may_break(before: char, after: char, kinsoku: Option<Kinsoku>, latin: LatinBreaks) -> bool {
    if let Some(k) = kinsoku
        && (k.no_start.contains(after) || k.no_end.contains(before))
    {
        return false;
    }
    if glue(before) || glue(after) || (!latin.ambiguous_wide && inseparable(before, after)) {
        return false;
    }
    // Not before a space, but before the ideographic space as before a kanji (Word's PDF of corpus 010: a line breaks before 「　」).
    if (after.is_whitespace() && after != '\u{3000}') || after == '\u{ad}' || after == ibis_doc_model::OPTIONAL_HYPHEN {
        return false;
    }
    if before.is_whitespace() {
        return true;
    }
    let wide = |c: char| is_ideographic(c) || (latin.ambiguous_wide && east_asian_ambiguous(c));
    if wide(before) || wide(after) {
        return true;
    }
    match before {
        // After a hyphen before a digit too (Word's PDF of corpus 124: a URL breaks at "ffca-‖4b2a"; not UAX #14's HY × NU).
        '-' | '\u{ad}' | ibis_doc_model::OPTIONAL_HYPHEN | '–' | '—' | '‐' => true,
        _ => latin.anywhere,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ※ (U+203B) between Latin letters: Word and PowerPoint do not break next to it, Excel does (it is East Asian Width "A", `ambiguous_wide`). Their PDFs of
    /// `star-break-probe` (scripts/corpus/make_star_break_probe.py: 'abcdefghij' x7 and 0 to 15 letters, ※, 32 letters, 20 cases in each; docx, pptx and xlsx): the
    /// Word and PowerPoint lines run on to the edge and cut inside the letters (the word has no place to break), and Excel ends the line after ※ every time.
    #[test]
    fn a_reference_mark_between_latin_letters_breaks_in_excel_only() {
        let word_and_powerpoint = LatinBreaks::default();
        let excel = LatinBreaks { ambiguous_wide: true, ..LatinBreaks::default() };
        assert!(!may_break('j', '※', Some(Kinsoku::POWERPOINT), word_and_powerpoint) && !may_break('※', 'k', Some(Kinsoku::POWERPOINT), word_and_powerpoint));
        assert!(may_break('j', '※', Some(Kinsoku::EXCEL), excel) && may_break('※', 'k', Some(Kinsoku::EXCEL), excel));
        // Next to Japanese text it breaks as any mark does.
        assert!(may_break('あ', '※', Some(Kinsoku::POWERPOINT), word_and_powerpoint) && may_break('※', 'ア', Some(Kinsoku::POWERPOINT), word_and_powerpoint));
    }

    #[test]
    fn breaks_of_latin_text() {
        let l = LatinBreaks::default();
        assert!(may_break(' ', 'a', None, l));
        assert!(!may_break('a', 'b', None, l));
        assert!(may_break('-', 'k', None, l));
        assert!(may_break('-', '4', None, l));
        assert!(may_break('—', 'd', None, l));
        assert!(may_break('–', '2', None, l));
        assert!(may_break('\u{ad}', 'f', None, l));
        assert!(!may_break('1', '\u{202f}', None, l));
        assert!(!may_break('\u{a0}', 'k', None, l));
        assert!(!may_break('a', '/', None, l));
        assert!(!may_break('/', 'b', None, l));
    }

    #[test]
    fn breaks_of_japanese_text() {
        let l = LatinBreaks::default();
        let k = Some(Kinsoku::POWERPOINT);
        assert!(may_break('あ', 'い', k, l));
        assert!(may_break('ん', 'A', k, l));
        assert!(!may_break('ん', '。', k, l));
        assert!(!may_break('「', 'あ', k, l));
        assert!(may_break('ん', '―', k, l));
        assert!(!may_break('―', '―', k, l));
        assert!(may_break('ん', '。', None, l));
    }
}
