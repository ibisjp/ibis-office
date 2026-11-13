// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The Unicode character that shows what a glyph of a symbol typeface (`Symbol`, `Wingdings`, `Wingdings 2`, `Wingdings 3`) shows.
//!
//! A bullet or a character in these typefaces is written as a code (0x20..=0xFF), or as U+F000 plus the code (the private use area, where
//! Word and PowerPoint write it). Where the typeface is not installed (and where it is, for the room measure of a bullet), the character
//! below stands in for the glyph. The data are files, not code: `data/adobe-symbol.txt` (Adobe's Symbol encoding, Unicode, Inc.) and
//! `data/symbol-fonts.tsv` (the Wingdings typefaces).

use std::collections::HashMap;
use std::sync::OnceLock;

const ADOBE_SYMBOL: &str = include_str!("../data/adobe-symbol.txt");
const WINGDINGS: &str = include_str!("../data/symbol-fonts.tsv");

type Table = HashMap<(u8, u8), char>;

/// The number of a typeface this module knows.
fn font_id(font: &str) -> Option<u8> {
    match font {
        "Symbol" => Some(0),
        "Wingdings" => Some(1),
        "Wingdings 2" => Some(2),
        "Wingdings 3" => Some(3),
        _ => None,
    }
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut out = Table::new();
        // Adobe's file: `Unicode<TAB>code<TAB># name`. The first character of a code stands (U+00A0 follows U+0020 for 0x20); the
        // private use ones (Adobe's corporate use area) are no characters.
        for line in ADOBE_SYMBOL.lines().filter(|l| !l.starts_with('#')) {
            let mut cols = line.split('\t');
            let (Some(u), Some(code)) = (cols.next(), cols.next()) else { continue };
            let (Ok(u), Ok(code)) = (u32::from_str_radix(u.trim(), 16), u8::from_str_radix(code.trim(), 16)) else { continue };
            if let Some(c) = char::from_u32(u).filter(|_| !(0xe000..=0xf8ff).contains(&u)) {
                out.entry((0, code)).or_insert(c);
            }
        }
        for line in WINGDINGS.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
            let mut cols = line.split('\t');
            let (Some(font), Some(code), Some(u)) = (cols.next(), cols.next(), cols.next()) else { continue };
            let (Some(font), Ok(code), Some(c)) = (font_id(font), u8::from_str_radix(code, 16), u32::from_str_radix(u, 16).ok().and_then(char::from_u32)) else { continue };
            out.insert((font, code), c);
        }
        out
    })
}

/// Whether `font` is one of the typefaces this module knows.
pub fn is_known(font: &str) -> bool {
    font_id(font).is_some()
}

/// The code of the typeface `c` is: itself (0x20..=0xFF) or U+F000 plus the code.
pub fn code_of(c: char) -> Option<u8> {
    let u = c as u32;
    match u {
        0x20..=0xff => Some(u as u8),
        0xf020..=0xf0ff => Some((u - 0xf000) as u8),
        _ => None,
    }
}

/// Whether Unicode draws `c` as a colour emoji unless it has the text presentation selector (U+FE0E) after it (`Emoji_Presentation` in
/// emoji-data.txt): of the stand-ins of the tables, the hourglass and the signs of the zodiac. A glyph of these typefaces is a plain
/// black shape.
pub fn emoji_by_default(c: char) -> bool {
    matches!(c as u32, 0x231B | 0x2648..=0x2653)
}

/// The Unicode character that stands in for the glyph `c` (a code or U+F000 plus it) of `font`, if there is one.
pub fn stand_in(font: &str, c: char) -> Option<char> {
    let code = code_of(c)?;
    table().get(&(font_id(font)?, code)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue 142: the bullet of Word's library, ✧ (Wingdings 0xB2, written U+F0B2), was left as the private use character: no typeface
    /// has a glyph for it, and it was drawn as a box.
    #[test]
    fn the_four_pointed_star_bullet_has_a_character() {
        assert_eq!(stand_in("Wingdings", '\u{f0b2}'), Some('\u{2727}'));
        assert_eq!(stand_in("Wingdings", '\u{b2}'), Some('\u{2727}'));
    }

    #[test]
    fn the_bullets_of_the_library_have_characters() {
        for (c, want) in [('\u{f06c}', '●'), ('\u{f0d8}', '➢'), ('\u{f06e}', '■'), ('\u{f075}', '◆'), ('\u{f0fc}', '✓'), ('\u{f0a7}', '▪'), ('\u{f076}', '❖')] {
            assert_eq!(stand_in("Wingdings", c), Some(want), "{c:?}");
        }
        assert_eq!(stand_in("Symbol", '\u{f0b7}'), Some('•'));
        assert_eq!(stand_in("Symbol", 'a'), Some('α'));
        assert_eq!(stand_in("Wingdings 3", '\u{f075}'), Some('▶'));
        assert_eq!(stand_in("Arial", '\u{f0b7}'), None);
    }

    /// No character of the tables is in the private use area (it would be a box again) or a control character.
    #[test]
    fn no_stand_in_is_a_private_use_character() {
        for ((_, code), c) in table() {
            assert!(!(0xe000..=0xf8ff).contains(&(*c as u32)) && !c.is_control(), "code {code:#x} -> {c:?}");
        }
        for id in 0..4u8 {
            assert!(table().keys().filter(|(f, _)| *f == id).count() > 40, "typeface {id}");
        }
    }
}
