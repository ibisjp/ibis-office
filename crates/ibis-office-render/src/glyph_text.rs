// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The character each drawn glyph stands for, kept while a PDF is being written.
//!
//! A glyph in a PDF is copied and searched as the character the font's `cmap` (and its single substitutions) gives its glyph id. Several characters
//! share one glyph when the font draws a character with another's glyph: a font standing in for a proportional typeface is asked for `palt` and
//! `pwid`, which draw the full-width オ, （ and １ with the glyphs of ｵ, ( and 1, so the PDF's text came out as the half-width forms. The glyph id alone cannot
//! tell them apart, so the painting code that knows the text (`rich_text`) notes the character it draws at each glyph's origin, and the PDF writer
//! looks it up. Noting is on only while a PDF writer exists (`set_recording`), so that painting the screen keeps nothing.

use gpui_kit::{FontId, GlyphId, Pixels, Point};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

static RECORDING: AtomicBool = AtomicBool::new(false);
type Key = (usize, u32, u32, u32);
static NOTED: Mutex<Option<HashMap<Key, char>>> = Mutex::new(None);

fn key(font: FontId, glyph: GlyphId, origin: Point<Pixels>) -> Key {
    (font.0, glyph.0, f32::from(origin.x).to_bits(), f32::from(origin.y).to_bits())
}

/// Start (`true`) or stop (`false`) keeping the characters; either way what was kept is forgotten.
pub fn set_recording(on: bool) {
    RECORDING.store(on, Ordering::Relaxed);
    if let Ok(mut noted) = NOTED.lock() {
        *noted = None;
    }
}

/// The glyph `glyph` of `font` painted with its origin at `origin` is the character `c`.
pub fn note(font: FontId, glyph: GlyphId, origin: Point<Pixels>, c: char) {
    if RECORDING.load(Ordering::Relaxed)
        && let Ok(mut noted) = NOTED.lock()
    {
        noted.get_or_insert_with(HashMap::new).insert(key(font, glyph, origin), c);
    }
}

/// The character noted for that glyph painted at `origin`, if one was.
pub fn noted(font: FontId, glyph: GlyphId, origin: Point<Pixels>) -> Option<char> {
    NOTED.lock().ok()?.as_ref()?.get(&key(font, glyph, origin)).copied()
}

/// Whether the characters are being kept (a PDF is being written).
pub fn recording() -> bool {
    RECORDING.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A character noted while recording is found by its glyph and origin, and only then; stopping forgets it.
    #[test]
    fn a_noted_character_is_found_only_while_recording() {
        let (font, glyph, at) = (FontId(7), GlyphId(42), gpui_kit::point(gpui_kit::px(12.5), gpui_kit::px(30.0)));
        set_recording(false);
        note(font, glyph, at, 'オ');
        assert_eq!(noted(font, glyph, at), None, "nothing is kept when no PDF is being written");
        set_recording(true);
        note(font, glyph, at, 'オ');
        assert_eq!(noted(font, glyph, at), Some('オ'));
        assert_eq!(noted(font, glyph, gpui_kit::point(gpui_kit::px(13.0), gpui_kit::px(30.0))), None);
        set_recording(false);
        assert_eq!(noted(font, glyph, at), None);
    }
}
