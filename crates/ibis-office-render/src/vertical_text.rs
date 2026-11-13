// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Unicode UAX #50 orientation; OpenType's vertical glyphs belong to the font.
use gpui_kit::{Font, FontFeatures};
pub fn upright(c: char) -> bool {
    if word_turns(c) {
        return false;
    }
    match orientation(c) {
        unicode_vo::Orientation::Rotated => word_keeps_upright(c),
        _ => true,
    }
}

/// Whether the base-less combining marks at the start of a vertical line (paragraph start, after a line break, after a tab) share one em cell of the column, the
/// character `next` that follows them being what Word looks at. Word's PDFs of `m138-nobase-probe*.docx` (#M138, ＭＳ 明朝 10.5 pt, 1 to 4 marks of U+0300..033B):
/// before ❶ ① ○ ★ ■ ※ ✓ the whole run of marks is ONE 1 em cell whatever its length, the marks across the column on the cell's top; before 亜 あ ア ㈱ 「 ０ Ａ
/// (full width) and before a, 1, α, ― (turned) each mark takes its own half em (n marks, n/2 em), as they do anywhere else in a line (after a character, horizontally).
/// What the first group has that the second lacks: they are upright (UAX #50 U or Tu, Word's orientation: `upright`) and not full width (East Asian Width A or N,
/// not W or F). Word looks to cluster the marks with such a symbol as one unit set in the half-width way (the symbol's own half-width form is the unit), while a full width
/// character is a unit of its own (the marks stand beside it), and a turned character is laid along the column, where the marks too are laid along it. After a
/// character (亜, the same ❶) the marks are a run of their own again: the rule is for a line's start only. This is one reading that fits all 18 symbols and
/// letters measured; a symbol the probes did not have is by this rule, not measured.
pub fn baseless_marks_share_a_cell(next: char) -> bool {
    use unicode_width::UnicodeWidthChar;
    upright(next) && next.width() == Some(1)
}

/// UAX #50 orientation with Word's one difference in the Tr set: ； (U+FF1B) is Tu there. Word's PDF of GitHub issue 120's test.docx (游明朝 of Windows,
/// whose GSUB `vert` has ： but no ；, ？ or ！) draws ； upright with its horizontal glyph in its own cell, under ： (which takes its `vert` glyph),
/// where UAX #50 Tr turned it. Of the 27 Tr characters of the 455 of that document ； is the only one without a vertical glyph in the font, so the
/// only one this rule changes.
fn orientation(c: char) -> unicode_vo::Orientation {
    match c {
        '\u{FF1B}' => unicode_vo::Orientation::TransformedOrUpright,
        _ => unicode_vo::char_orientation(c),
    }
}

/// Whether Word turns `c` in vertical text though UAX #50 sets it upright. Word's PDF of `uniq.docx` (GitHub issue 54: the 455 different characters of
/// gakusyosi's test.docx, one to a line between two あ, in 游明朝 with the East Asian hint; Microsoft 365 16.0.20527) turns ± × ÷ ∞ ∴ ∵, the six
/// characters where UAX #50 revision 17 (Unicode 15) says upright and the Word of the PDF has the older, rotated, orientation; every other
/// character measured, 453 of the 455, agrees with UAX #50 plus `word_keeps_upright`.
fn word_turns(c: char) -> bool {
    matches!(c, '\u{00B1}' | '\u{00D7}' | '\u{00F7}' | '\u{221E}' | '\u{2234}' | '\u{2235}') || word_turns_symbol(c)
}

/// More characters UAX #50 sets upright and Word turns (the same older orientation as ± × ÷): ¼ ½ ¾ (U+00BC..00BE), ⅓ ⅔ ⅕ (U+2153..2155) and
/// the Miscellaneous Technical characters ⎾ ⎿ (U+23BE, U+23BF) and ⏀ .. ⏌ (U+23C0..23CC). Word's PDF of `fullwidth-symbols-all.docx` (#M125, GitHub issue
/// 54's F4, ＭＳ 明朝 and 游明朝 in vertical text; Microsoft 365) lays each of the 22 down, in both typefaces; the half-width ones of them take the half
/// of an em along the column as the turned Latin letters do (their advance in the font, which the layout takes for a character that is not upright).
/// © and ® (U+00A9, U+00AE) are laid down too, the half-width ones of ＭＳ 明朝 (Word's PDF of the marked copy, #M136: turned, 5.28 pt of the 10.56 pt text each; the
/// first reading took them for upright). The other characters of those blocks the document has (§ 〓, ⌅ ⌘, ♩ ◁ ‼ ...) stay upright in Word, as UAX #50 has them. Characters of the same
/// blocks the document does not have (⅐ ⅛ ⏍ ...) are not measured, and stay as UAX #50.
fn word_turns_symbol(c: char) -> bool {
    matches!(c as u32, 0x00A9 | 0x00AE | 0x00BC..=0x00BE | 0x2150..=0x2155 | 0x215B..=0x215E | 0x23BE..=0x23CD)
}

/// Whether Word keeps `c` upright in vertical text though UAX #50 turns it (same PDF): the quotation marks ‘ ’ “ ” (U+2018, U+2019, U+201C,
/// U+201D), ⇒ ⇔ (U+21D2, U+21D4) and Å (U+212B), which take the East Asian font of a run with the East Asian hint, and the fullwidth forms
/// (＋ is upright already; － ＜ ＝ ＞ are the ones UAX #50 turns). The other East Asian Ambiguous characters UAX #50 turns (Greek and Cyrillic
/// letters, the mathematical operators, the arrows → ← ↑ ↓, the box-drawing characters ─ ┌ ╂ ...) are turned by Word too: 192 of the 199
/// measured; an earlier rule that kept all East Asian Ambiguous characters upright was wrong for them.
fn word_keeps_upright(c: char) -> bool {
    use unicode_width::UnicodeWidthChar;
    matches!(c, '\u{2018}' | '\u{2019}' | '\u{201C}' | '\u{201D}' | '\u{21D2}' | '\u{21D4}' | '\u{212B}') || c.width() == Some(2)
}

/// Whether the font's vertical glyph (`vert`/`vrt2`) is used for `c`: UAX #50 Tu and Tr only (U, such as §, keeps its glyph). A character UAX #50 turns that
/// stays upright only because it took the East Asian font keeps its horizontal glyph: Word's PDF
/// draws the plain ‘ ’ “ ” ⇒ ⇔ in all four typefaces, where Hiragino and Yu fonts have vertical
/// forms (small marks, ⇓ ⇕) for them.
pub fn vertical_glyph(c: char) -> bool {
    matches!(
        orientation(c),
        unicode_vo::Orientation::TransformedOrUpright | unicode_vo::Orientation::TransformedOrRotated
    ) || {
        // A full-width character UAX #50 turns that Word keeps upright (the fullwidth forms, ＝ U+FF1D) is drawn with the font's vertical glyph too:
        // Word's PDF of GitHub issue 54's test-gakusyosi.docx (游明朝) draws ＝ as the turned form (‖), where ibisOffice drew the horizontal one.
        use unicode_width::UnicodeWidthChar;
        matches!(orientation(c), unicode_vo::Orientation::Rotated) && c.width() == Some(2) && !word_turns(c)
    }
}

pub fn font(base: &Font) -> Font {
    let mut font = base.clone();
    let mut features = base.features.tag_value_list().to_vec();
    features.retain(|(tag, _)| tag != "vert" && tag != "vrt2");
    features.extend([("vert".into(), 1), ("vrt2".into(), 1)]);
    font.features = FontFeatures(std::sync::Arc::new(features));
    font
}

/// CoreText's horizontal shaping can ignore vertical-only substitutions even
/// when vert is enabled. Select the font's GSUB vertical glyph explicitly; the
/// same glyph ID is used by the screen and by PDF paint records.
fn glyph_pair(
    ts: &gpui_kit::TextSystem,
    font_id: gpui_kit::FontId,
    c: char,
) -> Option<(gpui_kit::GlyphId, gpui_kit::GlyphId)> {
    use std::{collections::HashMap, rc::Rc, sync::Arc};
    type FontData = Rc<(Arc<Vec<u8>>, harfrust::ShaperData)>;
    thread_local! {
        static FONTS:std::cell::RefCell<HashMap<(usize,u32),FontData>>=std::cell::RefCell::new(HashMap::new());
        static GLYPHS:std::cell::RefCell<HashMap<(usize,u32,char),Option<(gpui_kit::GlyphId,gpui_kit::GlyphId)>>>=std::cell::RefCell::new(HashMap::new());
    }
    let (data, index) = ts.font_data(font_id)?;
    let key = (Arc::as_ptr(&data) as usize, index, c);
    if let Some(found) = GLYPHS.with(|cache| cache.borrow().get(&key).copied()) {
        return found;
    }
    let font = harfrust::FontRef::from_index(&data, index).ok()?;
    let cached = FONTS.with(|cache| {
        cache
            .borrow_mut()
            .entry((key.0, key.1))
            .or_insert_with(|| Rc::new((data.clone(), harfrust::ShaperData::new(&font))))
            .clone()
    });
    // Keep the data alive in FONTS: pointer keys cannot be reused by another
    // text system, including a headless clipboard-image renderer.
    let shaper = cached.1.shaper(&font).build();
    let mut buffer = harfrust::UnicodeBuffer::new();
    buffer.push_str(&c.to_string());
    buffer.set_direction(harfrust::Direction::TopToBottom);
    buffer.set_script(harfrust::script::HAN);
    buffer.guess_segment_properties();
    let features = [
        harfrust::Feature::new(harfrust::Tag::new(b"vert"), 1, ..),
        harfrust::Feature::new(harfrust::Tag::new(b"vrt2"), 1, ..),
    ];
    let shaped = shaper.shape(buffer, &features);
    let found = shaped
        .glyph_infos()
        .first()
        .filter(|g| g.glyph_id != 0)
        .map(|g| gpui_kit::GlyphId(g.glyph_id));
    let mut regular = harfrust::UnicodeBuffer::new();
    regular.push_str(&c.to_string());
    regular.set_direction(harfrust::Direction::LeftToRight);
    regular.set_script(harfrust::script::HAN);
    regular.guess_segment_properties();
    let regular = shaper
        .shape(regular, &[])
        .glyph_infos()
        .first()
        .filter(|g| g.glyph_id != 0)
        .map(|g| gpui_kit::GlyphId(g.glyph_id));
    let found = found.zip(regular);
    GLYPHS.with(|cache| cache.borrow_mut().insert(key, found));
    found
}

/// The font's vertical glyph (`vert`/`vrt2`) of `c` when it has one other than the horizontal glyph (for 、。「」ー…).
pub(crate) fn vertical_glyph_id(ts: &gpui_kit::TextSystem, font_id: gpui_kit::FontId, c: char) -> Option<gpui_kit::GlyphId> {
    glyph_pair(ts, font_id, c).filter(|(vertical, normal)| vertical != normal).map(|(vertical, _)| vertical)
}

/// UAX #50 Tr uses the horizontal glyph rotated when the selected face has
/// no vert/vrt2 substitution. Use the same path for committed and IME text.
pub fn paint_glyph(
    window: &mut gpui_kit::Window,
    origin: gpui_kit::Point<gpui_kit::Pixels>,
    font: gpui_kit::FontId,
    c: char,
    size: f32,
    color: gpui_kit::Hsla,
) -> bool {
    use gpui_kit::{Radians, ScaledPixels, TransformationMatrix, point, px};
    let Some((vertical, normal)) = glyph_pair(window.text_system(), font, c) else {
        return false;
    };
    let rotate = match orientation(c) {
        unicode_vo::Orientation::Rotated | unicode_vo::Orientation::Upright => !upright(c),
        unicode_vo::Orientation::TransformedOrRotated => vertical == normal,
        _ => false,
    };
    if !rotate {
        return window
            .paint_glyph(origin, font, vertical, px(size), color)
            .is_ok();
    }
    let Ok(bounds) = window.text_system().typographic_bounds(font, px(size), c) else {
        return false;
    };
    let center = origin + bounds.origin + point(bounds.size.width / 2., bounds.size.height / 2.);
    let scale = window.scale_factor();
    let pivot = point(
        ScaledPixels(f32::from(center.x) * scale),
        ScaledPixels(f32::from(center.y) * scale),
    );
    let transform = TransformationMatrix::unit()
        .translate(pivot)
        .rotate(Radians(std::f32::consts::FRAC_PI_2))
        .translate(point(ScaledPixels(-pivot.x.0), ScaledPixels(-pivot.y.0)));
    window
        .paint_glyph_transformed(origin, font, normal, px(size), color, transform)
        .is_ok()
}

/// How many characters of text being composed go in each column (or line), where a column holds `first` of them and the next ones `rest`
/// (both at least one): the cell of character `i`, as (column, place in it).
fn wrapped_cell(i: usize, first: usize, rest: usize) -> (usize, usize) {
    if i < first { (0, i) } else { ((i - first) / rest + 1, (i - first) % rest) }
}

/// Text being composed in a vertical line: a character to a cell of `size_px` down the column, from the caret, and on to the column left of it
/// (`column_px` over) where the column's room (`first_px` from the caret, `room_px` from its start) is used, as Word wraps it.
pub fn marked(text: String, spans: Vec<crate::ime_text::ImeSpan>, k: f32, face: Font, size_px: f32, first_px: f32, room_px: f32, column_px: f32) -> gpui_kit::AnyElement {
    use gpui_kit::*;
    let count = text.chars().count();
    let first = ((first_px / size_px).floor() as usize).max(1);
    let rest = ((room_px / size_px).floor() as usize).max(1);
    canvas(
        |_, _, _| (),
        move |bounds, _, window, cx| {
            for (i, c) in text.chars().enumerate() {
                let (column, row) = wrapped_cell(i, first, rest);
                let value = c.to_string();
                let run = TextRun {
                    len: value.len(),
                    font: face.clone(),
                    color: black(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped =
                    window
                        .text_system()
                        .shape_line(value.into(), px(size_px), &[run], None);
                // A column after the first starts at the top of the line, not at the caret.
                let start = if column == 0 { 0.0 } else { (room_px - first_px).max(0.0) * -1.0 };
                let at = bounds.origin + point(px(-(column as f32) * column_px), px(start + row as f32 * size_px));
                if column > 0 && row == 0 {
                    window.paint_quad(fill(Bounds::new(at, gpui_kit::size(px(column_px), px((count - first - (column - 1) * rest).min(rest) as f32 * size_px))), white()));
                }
                if let Some(run) = shaped.runs.first()
                    && paint_glyph(
                        window,
                        at + point(px(0.), shaped.ascent),
                        run.font_id,
                        c,
                        size_px,
                        black(),
                    )
                {
                    continue;
                }
                shaped
                    .paint(at, px(size_px), TextAlign::Left, None, window, cx)
                    .ok();
            }
            // The underline of each clause, a column at a time, on the right of the characters (see `ime_text::paint_column`). The turned
            // line box has its descent on the left of the baseline, so the mirrored baseline is a descent in from the column's right edge.
            let run = TextRun { len: 3, font: face.clone(), color: black(), background_color: None, underline: None, strikethrough: None };
            let descent = f32::from(window.text_system().shape_line("\u{3042}".into(), px(size_px), &[run], None).descent);
            let columns = if count == 0 { 0 } else { wrapped_cell(count - 1, first, rest).0 + 1 };
            for column in 0..columns {
                let start = if column == 0 { 0.0 } else { (room_px - first_px).max(0.0) * -1.0 };
                let cells: Vec<(f32, f32, usize)> = (0..count)
                    .filter_map(|i| {
                        let (c, row) = wrapped_cell(i, first, rest);
                        (c == column).then(|| (row as f32 * size_px, size_px, i))
                    })
                    .collect();
                let origin = bounds.origin + point(px(-(column as f32) * column_px), px(start));
                crate::ime_text::paint_column(window, origin, &cells, &spans, size_px / k, k, size_px - descent, black());
            }
        },
    )
    .w(px(size_px))
    .h(px(size_px * count.min(first) as f32))
    .into_any_element()
}

/// Where text being composed breaks: characters of the given `widths` go in lines, the first `first_px` wide and the next ones `room_px`
/// (a character wider than the line still stands alone on one). Each character comes back as (index, distance from its line's start).
fn break_lines(widths: &[f32], first_px: f32, room_px: f32) -> Vec<Vec<(usize, f32)>> {
    let mut lines: Vec<Vec<(usize, f32)>> = vec![Vec::new()];
    let mut used = 0.0f32;
    let mut limit = first_px.max(0.0);
    for (i, &w) in widths.iter().enumerate() {
        if used + w > limit + 0.01 && !lines.last().is_some_and(|l| l.is_empty()) {
            lines.push(Vec::new());
            used = 0.0;
            limit = room_px;
        }
        lines.last_mut().expect("a line").push((i, used));
        used += w;
    }
    lines
}

/// Text being composed in a horizontal line: `base_px` below the top of the line is its baseline, as the page's text has it. It breaks at
/// `first_px` from the caret and then at `room_px` (from `indent_px` left of the caret), a line (`line_px`) down each time, with an underline.
pub fn marked_horizontal(text: String, spans: Vec<crate::ime_text::ImeSpan>, k: f32, face: Font, size_px: f32, base_px: f32, line_px: f32, first_px: f32, room_px: f32, indent_px: f32) -> gpui_kit::AnyElement {
    use gpui_kit::*;
    canvas(
        |_, _, _| (),
        move |bounds, _, window, cx| {
            // The text in lines: its characters shaped one by one (a composition string is short, and the advance is the same).
            let shapes: Vec<ShapedLine> = text
                .chars()
                .map(|c| {
                    let value = c.to_string();
                    let run = TextRun { len: value.len(), font: face.clone(), color: black(), background_color: None, underline: None, strikethrough: None };
                    window.text_system().shape_line(value.into(), px(size_px), &[run], None)
                })
                .collect();
            let widths: Vec<f32> = shapes.iter().map(|s| f32::from(s.width)).collect();
            let mut shapes = shapes.into_iter();
            let lines: Vec<Vec<(ShapedLine, f32, usize)>> = break_lines(&widths, first_px, room_px.max(size_px))
                .into_iter()
                .map(|line| line.into_iter().map(|(index, at)| (shapes.next().expect("a shape for each character"), at, index)).collect())
                .collect();
            for (n, line) in lines.iter().enumerate() {
                let x0 = if n == 0 { indent_px } else { 0.0 };
                let top = bounds.origin.y + px(n as f32 * line_px);
                let end = line.last().map_or(0.0, |(s, at, _)| at + f32::from(s.width));
                let left = bounds.origin.x + px(x0);
                window.paint_quad(fill(Bounds::new(point(left, top), gpui_kit::size(px(end), px(line_px))), white()));
                for (shaped, at, _) in line {
                    let origin = point(left + px(*at), top + px(base_px) - shaped.ascent);
                    shaped.paint(origin, shaped.ascent + shaped.descent, TextAlign::Left, None, window, cx).ok();
                }
                // Each clause as the input method underlines it (the one being converted thick, the part being typed dotted, as Word does).
                let cells: Vec<(f32, f32, usize)> = line.iter().map(|(s, at, i)| (*at, f32::from(s.width), *i)).collect();
                crate::ime_text::paint_line(window, point(left, top), &cells, &spans, size_px / k, k, base_px, black());
            }
        },
    )
    .w(px(room_px.max(size_px)))
    .h(px(line_px))
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{baseless_marks_share_a_cell, upright, vertical_glyph};

    // Word's PDF of issue 54 (vertical page, four typefaces): these stay upright.
    #[test]
    fn east_asian_ambiguous_symbols_stay_upright_in_vertical_text() {
        for c in "\u{2018}\u{2019}\u{201c}\u{201d}\u{21d2}\u{21d4}\u{212b}\u{ff0d}\u{ff1c}\u{ff1d}\u{ff1e}".chars() {
            assert!(upright(c), "U+{:04X}", c as u32);
        }
    }

    // Word's PDF of issue 54 (uniq.docx): these are turned, the East Asian Ambiguous ones UAX #50 turns and ± × ÷ ∞ ∴ ∵.
    #[test]
    fn word_turns_mathematical_signs_arrows_and_box_drawing() {
        for c in "\u{00b1}\u{00d7}\u{00f7}\u{221e}\u{2234}\u{2235}\u{2208}\u{2200}\u{222b}\u{2192}\u{2190}\u{2191}\u{2500}\u{254b}\u{0391}\u{0430}\u{00b0}".chars() {
            assert!(!upright(c), "U+{:04X}", c as u32);
        }
        for c in "§○■①Ⅰ※℃".chars() {
            assert!(upright(c), "U+{:04X}", c as u32);
        }
    }

    // Word's PDF of fullwidth-symbols-all.docx (#M125, GitHub issue 54's F4): the vulgar fractions of Latin-1 and the Number Forms it has, and the
    // Miscellaneous Technical characters U+23BE..23CC, are laid down in vertical text though UAX #50 has them upright; the symbols beside them stay.
    #[test]
    fn word_turns_fractions_and_the_technical_symbols_it_has() {
        let turned: Vec<char> = [0xA9, 0xAE, 0xBC, 0xBD, 0xBE, 0x2153, 0x2154, 0x2155, 0x23BE, 0x23BF, 0x23C0, 0x23C6, 0x23CC].iter().filter_map(|&u| char::from_u32(u)).collect();
        for c in turned {
            assert!(!upright(c), "U+{:04X}", c as u32);
            assert!(!vertical_glyph(c), "U+{:04X}", c as u32);
        }
        for u in [0xA7u32, 0x3013, 0x2305, 0x2318, 0x2669, 0x25C1, 0x203C, 0x2156, 0x215A] {
            let c = char::from_u32(u).unwrap();
            assert!(upright(c), "U+{:04X}", u);
        }
    }

    // Word's PDFs of m130-vertical-probe.docx (#M130): the rest of the Number Forms fractions it was given and ⏍ are laid down too.
    #[test]
    fn word_turns_the_other_fractions_and_the_technical_symbol_of_the_probe() {
        for u in [0x2150u32, 0x2151, 0x2152, 0x215B, 0x215C, 0x215D, 0x215E, 0x23CD] {
            let c = char::from_u32(u).unwrap();
            assert!(!upright(c), "U+{:04X}", u);
            assert!(!vertical_glyph(c), "U+{:04X}", u);
        }
    }

    #[test]
    fn ambiguous_symbols_keep_their_horizontal_glyph() {
        assert!(!vertical_glyph('\u{21d2}') && !vertical_glyph('\u{2018}'));
        assert!(vertical_glyph('、') && vertical_glyph('（') && !vertical_glyph('§') && !vertical_glyph('○'));
        // ＝ (U+FF1D) is drawn with the font's vertical glyph, as Word does; § and ○ (not full-width forms) keep theirs.
        assert!(vertical_glyph('＝') && vertical_glyph('＜'));
    }

    // Latin letters and digits (narrow in East Asian text too) are still turned.
    #[test]
    fn latin_letters_are_still_turned() {
        for c in "Ab1".chars() {
            assert!(!upright(c));
        }
        assert!(upright('あ'));
    }

    // Text being composed (GitHub issues #121, #122): it goes on to the next column or line where the line's room ends.
    #[test]
    fn composition_breaks_at_the_lines_end() {
        use super::{break_lines, wrapped_cell};
        // 10.5pt characters, 3 fit the first line (room left of the caret), 5 each after.
        let w = [10.5f32; 12];
        let lines = break_lines(&w, 31.6, 52.6);
        assert_eq!(lines.iter().map(Vec::len).collect::<Vec<_>>(), vec![3, 5, 4]);
        assert_eq!(lines[1][0], (3, 0.0));
        assert_eq!(lines[1][4].1, 42.0);
        // Vertical: the same counts by cell.
        assert_eq!((0..12).map(|i| wrapped_cell(i, 3, 5)).collect::<Vec<_>>()[2..9], [(0, 2), (1, 0), (1, 1), (1, 2), (1, 3), (1, 4), (2, 0)]);
    }

    #[test]
    fn composition_with_no_room_still_puts_a_character_on_each_line() {
        use super::{break_lines, wrapped_cell};
        let lines = break_lines(&[12.0; 3], 0.0, 5.0);
        assert_eq!(lines.iter().map(Vec::len).collect::<Vec<_>>(), vec![1, 1, 1]);
        assert_eq!(wrapped_cell(2, 1, 1), (2, 0));
        // Fits exactly: no break; mixed widths (full and half width).
        assert_eq!(break_lines(&[10.0, 10.0], 20.0, 20.0).len(), 1);
        let mixed = break_lines(&[10.0, 5.0, 10.0, 5.0], 16.0, 16.0);
        assert_eq!(mixed.iter().map(Vec::len).collect::<Vec<_>>(), vec![2, 2]);
    }

    // #M138: the characters after base-less marks that make the marks share one em cell at a vertical line's start are the upright ones that are not full width (EAW A or N);
    // the full width ones and the turned ones (Word's PDFs: a, 1, α, ―) leave each mark its half em.
    #[test]
    fn baseless_marks_share_a_cell_before_upright_symbols_that_are_not_full_width() {
        for u in [0x2776u32, 0x2460, 0x25CB, 0x2605, 0x25A0, 0x203B, 0x2713] {
            assert!(baseless_marks_share_a_cell(char::from_u32(u).unwrap()), "U+{u:04X}");
        }
        for u in [0x4E9Cu32, 0x3042, 0x30A2, 0x3231, 0x300C, 0xFF10, 0xFF21, 0x61, 0x31, 0x3B1, 0x2015] {
            assert!(!baseless_marks_share_a_cell(char::from_u32(u).unwrap()), "U+{u:04X}");
        }
    }

    // Word's PDF of the marked test document (#M136, GitHub issue 54's F4: pages 3 and 4, vertical ＭＳ 明朝): every character the TSV has takes the orientation Word
    // gave it (U upright, R turned), the font-independent half of what the Mac draws. The invisible and image-drawn U+3029..302F, the blanks and the Latin
    // characters (turned in a run of the Latin font, which `upright` does not tell) are not asked.
    #[test]
    fn orientation_of_every_character_of_words_vertical_page() {
        let tsv = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tasks/doc/m54-marked/word-p3-4-all-chars.tsv")).expect("the TSV of Word's characters");
        let mut wrong = Vec::new();
        for line in tsv.lines().skip(1) {
            let f: Vec<&str> = line.split('\t').collect();
            let c = f[3].chars().next().unwrap();
            if f[3].chars().count() != 1 || c.is_whitespace() || c.is_ascii() || f[4] != "MS-Mincho" || (0x3029..=0x302f).contains(&(c as u32)) {
                continue;
            }
            if upright(c) != (f[7] == "U") {
                wrong.push(format!("U+{:04X} word {}", c as u32, f[7]));
            }
        }
        assert!(wrong.is_empty(), "{wrong:?}");
    }
}
