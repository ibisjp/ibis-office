// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Unicode UAX #50 orientation; OpenType's vertical glyphs belong to the font.
use gpui_kit::{Font, FontFeatures};
pub(crate) fn upright(c: char) -> bool {
    match unicode_vo::char_orientation(c) {
        unicode_vo::Orientation::Rotated => east_asian_ambiguous(c),
        _ => true,
    }
}

/// Whether the font's vertical glyph (`vert`/`vrt2`) is used for `c`: UAX #50 Tu and Tr only (U, such as §, keeps its glyph). A character UAX #50 turns that
/// stays upright only because it took the East Asian font keeps its horizontal glyph: Word's PDF
/// draws the plain ‘ ’ “ ” ⇒ ⇔ in all four typefaces, where Hiragino and Yu fonts have vertical
/// forms (small marks, ⇓ ⇕) for them.
pub(crate) fn vertical_glyph(c: char) -> bool {
    matches!(
        unicode_vo::char_orientation(c),
        unicode_vo::Orientation::TransformedOrUpright | unicode_vo::Orientation::TransformedOrRotated
    )
}

/// East Asian Width "Ambiguous" (UAX #11) at or above U+2000: the quotes, arrows, math and
/// geometric symbols that Word sets in the run's East Asian font (ECMA-376 17.18.41 `w:hint`
/// "eastAsia"). Word keeps a character that took that font upright in vertical text and gives it
/// the em cell, though UAX #50 turns it (Word's PDF of issue 54: ‘ ’ “ ” ⇒ ⇔ upright in ＭＳ 明朝,
/// ＭＳ ゴシック, 游明朝 and 游ゴシック). The Latin-1 ambiguous characters are left to UAX #50: no
/// Word PDF here shows them.
fn east_asian_ambiguous(c: char) -> bool {
    use unicode_width::UnicodeWidthChar;
    c >= '\u{2000}' && c.width() == Some(1) && c.width_cjk() == Some(2)
}
pub(crate) fn font(base: &Font) -> Font {
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

/// UAX #50 Tr uses the horizontal glyph rotated when the selected face has
/// no vert/vrt2 substitution. Use the same path for committed and IME text.
pub(crate) fn paint_glyph(
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
    let rotate = match unicode_vo::char_orientation(c) {
        unicode_vo::Orientation::Rotated => !upright(c),
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

pub(crate) fn marked(text: String, face: Font, size_px: f32) -> gpui_kit::AnyElement {
    use gpui_kit::*;
    let count = text.chars().count();
    canvas(
        |_, _, _| (),
        move |bounds, _, window, cx| {
            for (i, c) in text.chars().enumerate() {
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
                let at = bounds.origin + point(px(0.), px(i as f32 * size_px));
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
        },
    )
    .w(px(size_px))
    .h(px(size_px * count as f32))
    .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{upright, vertical_glyph};

    // Word's PDF of issue 54 (vertical page, four typefaces): these stay upright.
    #[test]
    fn east_asian_ambiguous_symbols_stay_upright_in_vertical_text() {
        for c in "\u{2018}\u{2019}\u{201c}\u{201d}\u{21d2}\u{21d4}".chars() {
            assert!(upright(c), "U+{:04X}", c as u32);
        }
    }

    #[test]
    fn ambiguous_symbols_keep_their_horizontal_glyph() {
        assert!(!vertical_glyph('\u{21d2}') && !vertical_glyph('\u{2018}'));
        assert!(vertical_glyph('、') && vertical_glyph('（') && !vertical_glyph('§') && !vertical_glyph('○'));
    }

    // Latin letters and digits (narrow in East Asian text too) are still turned.
    #[test]
    fn latin_letters_are_still_turned() {
        for c in "Ab1".chars() {
            assert!(!upright(c));
        }
        assert!(upright('あ'));
    }
}
