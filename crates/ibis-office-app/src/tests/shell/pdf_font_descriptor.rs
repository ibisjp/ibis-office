// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #188: the `/FontDescriptor` of a font in the PDF carries the numbers Word writes (`/Ascent` = hhea ascender, `/Descent` = typo
//! descender, `/CapHeight` = typo ascender, in 1000 per em), read back from a PDF that krilla wrote.

use krilla::Document;
use krilla::geom::Point;
use krilla::page::PageSettings;
use krilla::text::{Font, GlyphId, KrillaGlyph};

/// Writes one glyph in the face of `data` the way the exporter registers it, and returns what the PDF's `/FontDescriptor` says.
fn descriptor(data: &[u8], index: u32) -> (f32, f32, f32) {
    let font = match crate::pdf_vector::descriptor_metrics_font(data, index) {
        Some(patched) => Font::new(std::sync::Arc::new(patched).into(), index).unwrap(),
        None => Font::new(std::sync::Arc::new(data.to_vec()).into(), index).unwrap(),
    };
    let mut document = Document::new();
    let mut page = document.start_page_with(PageSettings::from_wh(100.0, 100.0).unwrap());
    let mut surface = page.surface();
    // Glyph 3 is some glyph; the descriptor does not depend on which.
    surface.draw_glyphs(Point::from_xy(10.0, 50.0), &[KrillaGlyph::new(GlyphId::new(3), 0.5, 0.0, 0.0, 0.0, 0..1, None)], font, "a", 12.0, false);
    surface.finish();
    page.finish();
    let pdf = document.finish().unwrap();
    let text = String::from_utf8_lossy(&pdf);
    let number = |key: &str| -> f32 {
        let at = text.find(&format!("/{key} ")).unwrap_or_else(|| panic!("no /{key}"));
        text[at + key.len() + 2..].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap().parse().unwrap()
    };
    (number("Ascent"), number("Descent"), number("CapHeight"))
}

fn near(got: (f32, f32, f32), want: (f32, f32, f32)) {
    assert!((got.0 - want.0).abs() < 1.0 && (got.1 - want.1).abs() < 1.0 && (got.2 - want.2).abs() < 1.0, "{got:?} wanted {want:?}");
}

/// Office's own fonts, where this Mac has them (the Office for Mac bundle): the values are Word's PDF's (Meiryo 1060 / -122 / 878, MS Mincho
/// 859 / -141 / 859, Arial 905 / -210 / 728). Does nothing when the files are not there.
#[test]
fn the_font_descriptor_has_the_numbers_word_writes() {
    let dir = std::path::Path::new("/Applications/Microsoft Excel.app/Contents/Resources/DFonts");
    let cases: [(&str, u32, (f32, f32, f32)); 4] = [
        ("meiryo.ttc", 0, (1060.0, -122.0, 878.0)),
        ("msmincho.ttc", 0, (859.0, -141.0, 859.0)),
        ("msgothic.ttc", 0, (859.0, -141.0, 859.0)),
        ("arial.ttf", 0, (905.0, -210.0, 728.0)),
    ];
    let mut checked = 0;
    for (name, index, want) in cases {
        let Ok(data) = std::fs::read(dir.join(name)) else { continue };
        near(descriptor(&data, index), want);
        checked += 1;
    }
    eprintln!("checked {checked} of {} fonts", cases.len());
}

/// A font that ships with the repository (IBM Plex Sans / Carlito): the descriptor follows the same rule from its own tables.
#[test]
fn the_font_descriptor_follows_hhea_and_typo_of_the_bundled_font() {
    use skrifa::raw::TableProvider as _;
    let path = ["fonts/carlito/Carlito-Regular.ttf", "../ibis-office-render/fonts/carlito/Carlito-Regular.ttf"].into_iter().map(|p| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(p)).find(|p| p.exists()).expect("Carlito");
    let data = std::fs::read(path).unwrap();
    let font = skrifa::FontRef::from_index(&data, 0).unwrap();
    let em = f32::from(font.head().unwrap().units_per_em());
    let (hhea, os2) = (font.hhea().unwrap(), font.os2().unwrap());
    let per_1000 = |v: f32| v * 1000.0 / em;
    let want = (per_1000(f32::from(hhea.ascender().to_i16())), per_1000(f32::from(os2.s_typo_descender())), per_1000(f32::from(os2.s_typo_ascender())));
    near(descriptor(&data, 0), want);
}
