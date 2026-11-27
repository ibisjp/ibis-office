// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The document's editing marks (the blue ↵ of a paragraph's end and a line break, ↓, the tab's →, ¶) are boxes on iOS (the report: the marks look right on a Mac).
//! A Mac's font database finds a font with the character by itself (Apple Symbols, Menlo, STIX); on iOS the database holds only the fonts the app adds
//! (`IOS_SYSTEM_FONTS`: Hiragino Sans, PingFang, SF, Apple Color Emoji) and the bundled UI font, and none of them has ↵ (U+21B5: it is in Apple Symbols, Menlo, STIX, Symbol
//! of the iOS system, checked in the simulator's runtime). The iOS fonts cannot be had on a Mac, so the test is on what can: the list the app adds names a font
//! that has every mark's character (Apple Symbols), and that font's file, where it is (a Mac has it), really has them.
use crate::IOS_SYSTEM_FONTS;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("shell::ios_edit_marks_the_added_fonts_have_every_editing_mark", marks_covered)]
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}

/// The characters of the editing marks: ↵ and ↓ (a Japanese document's paragraph end and line break), ¶ (the others' paragraph end), → (a tab), the dot of a space.
const MARKS: [char; 5] = ['\u{21b5}', '\u{2193}', '\u{b6}', '\u{2192}', '\u{b7}'];

fn marks_covered() {
    let names: Vec<String> = IOS_SYSTEM_FONTS.iter().map(|n| n.to_string_lossy().into_owned()).collect();
    assert!(names.iter().any(|n| n == "AppleSymbols"), "iOS adds Apple Symbols, the font that has ↵ (U+21B5): {names:?}");
    // Apple Symbols' own file, where this machine has it: it has every mark.
    let path = std::path::Path::new("/System/Library/Fonts/Apple Symbols.ttf");
    if let Ok(data) = std::fs::read(path) {
        let face = ttf_parser::Face::parse(&data, 0).expect("Apple Symbols is a font");
        for c in MARKS {
            assert!(face.glyph_index(c).is_some(), "Apple Symbols has {c:?} (U+{:04X})", c as u32);
        }
    }
}
