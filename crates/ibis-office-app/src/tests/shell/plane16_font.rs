// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #158: the Digital Agency's Add Character_GJMincho (acgjm.ttf; its cmap has a format 12 subtable with the characters of Plane 16,
//! U+100000..) is drawn by the platform's text system as its own glyphs, not by another typeface. The font file is not in the repository (the setup's
//! download, kept under `test/`); without it the scenario is skipped. A scenario, not a `#[test]`: the Mac text system needs the main thread.
use crate::shell::scenario::Scenario;
use gpui_kit::*;

const FAMILY: &str = "Add Character_GJMincho";

fn font_file() -> Option<Vec<u8>> {
    let root = crate::shell::scenario::repo_root();
    // This worktree's `test/`, the main checkout's (a sibling folder `ibisOffice`), and where the setup puts the download.
    let mut places = vec![root.join("test/fonts-digital-agency/x/acgjm.ttf"), root.join("../ibisOffice/test/fonts-digital-agency/x/acgjm.ttf")];
    if let Some(home) = std::env::var_os("HOME") {
        places.push(std::path::Path::new(&home).join("Library/Application Support/ibisOffice/fonts/ACGJMincho/acgjm.ttf"));
    }
    places.iter().find_map(|p| std::fs::read(p).ok())
}

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("shell::issue158_a_plane_16_character_is_drawn_in_the_typeface_that_has_it", a_plane_16_character_is_drawn_in_the_typeface_that_has_it as fn())]
}

fn a_plane_16_character_is_drawn_in_the_typeface_that_has_it() {
    let Some(bytes) = font_file() else {
        eprintln!("skipped: acgjm.ttf is not here");
        return;
    };
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(move |_, window, cx| {
        cx.text_system().add_fonts(vec![std::borrow::Cow::Owned(bytes)]).expect("the font is added");
        let system = window.text_system();
        let font = gpui_kit::font(FAMILY);
        let id = system.resolve_font(&font);
        // Characters of Plane 16 the font's cmap maps (the first, one in the middle, the last) are drawn as glyphs of the font.
        for c in ['\u{100000}', '\u{109705}', '\u{10fffd}'] {
            // (The font has no A or B: they may be drawn in another typeface; the Plane 16 character is the point.)
            let text = format!("A{c}B");
            let run = TextRun { len: text.len(), font: font.clone(), color: black(), background_color: None, underline: None, strikethrough: None };
            let line = system.shape_line(SharedString::from(text), px(20.0), &[run], None);
            let glyphs: Vec<_> = line.runs.iter().flat_map(|r| r.glyphs.iter()).collect();
            assert_eq!(glyphs.len(), 3, "A, U+{:X}, B are three glyphs", c as u32);
            let own = line.runs.iter().find(|r| r.glyphs.iter().any(|g| g.index == 1)).expect("the run of the character");
            assert_eq!(own.font_id, id, "U+{:X} is drawn in {FAMILY}, with no fallback typeface", c as u32);
            assert_eq!(glyphs[1].index, 1, "the glyph of the character is at its byte offset");
            assert!(glyphs[1].id.0 != 0, "U+{:X} has a glyph (not .notdef)", c as u32);
            let alone = system.advance(id, px(20.0), c).expect("an advance").width;
            assert!(alone > px(1.0), "U+{:X} has an advance", c as u32);
        }
    });
}
