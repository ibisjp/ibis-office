// SPDX-License-Identifier: AGPL-3.0-only
// Tests taken in from masanork's fork of ibisOffice (GitHub issue #157, option A), run against our own subsetter (skera) instead of the fork's code.
// Origin: https://github.com/masanork/ibis-office, commit 1e281696dc1f77b86b4b229a5234b2bbd42bc67d,
//   crates/ibis-font-subsetter/tests/subset.rs and tests/fonts/{ivs.ttf, ivs-cff.otf, generate.py}.
// Author of the tests and of the fixtures: Masanori Kusunoki (@masanork); he has signed the ibisOffice CLA (see the CLA sheet of issues.xlsx).
// Changed from the original: the calls go to `ibis_font_subsetter::subset` (code points in, font out) and the checks of the fork's own API
// (streaming input, merge aliases, the 50 MiB limit) are left out, as our tool has no such API.
//! The pitfalls the fork guards against: IVS (`cmap` format 14), composite glyphs, supplementary code points, `vert`/`liga` shaping with GSUB/GPOS/GDEF,
//! vertical metrics, and the name records (copyright and licence stay, the font's own names are replaced).

use std::collections::BTreeSet;

use ibis_font_subsetter::{Hinting, subset};
use ttf_parser::Face;

const TTF: &[u8] = include_bytes!("fonts/ivs.ttf");
const CFF: &[u8] = include_bytes!("fonts/ivs-cff.otf");

fn cut(input: &[u8], text: &str) -> Vec<u8> {
    let set: BTreeSet<u32> = text.chars().map(u32::from).collect();
    subset(input, &set, "ibis Fixture", "ibis-Fixture", "ibis-Fixture-0", Hinting::Keep).expect("the fixture is cut")
}

#[test]
fn preserves_bare_base_and_distinct_bmp_and_supplementary_variants() {
    for input in [TTF, CFF] {
        let output = cut(input, "A");
        let original = Face::parse(input, 0).unwrap();
        let subset = Face::parse(&output, 0).unwrap();
        let base = subset.glyph_index('A').unwrap();
        let bmp = subset.glyph_variation_index('A', '\u{fe00}').unwrap();
        let supplementary = subset.glyph_variation_index('A', '\u{e0100}').unwrap();
        assert_ne!(base, bmp);
        assert_ne!(base, supplementary);
        assert_ne!(bmp, supplementary);
        for (new, old) in [
            (base, original.glyph_index('A').unwrap()),
            (bmp, original.glyph_variation_index('A', '\u{fe00}').unwrap()),
            (supplementary, original.glyph_variation_index('A', '\u{e0100}').unwrap()),
        ] {
            assert_eq!(subset.glyph_bounding_box(new), original.glyph_bounding_box(old));
            assert_eq!(subset.glyph_hor_advance(new), original.glyph_hor_advance(old));
        }
    }
}

#[test]
fn retains_composite_dependencies_and_supplementary_codepoints() {
    let output = cut(TTF, "D\u{20000}\u{100001}");
    let original = Face::parse(TTF, 0).unwrap();
    let subset = Face::parse(&output, 0).unwrap();
    for cp in ['D', '\u{20000}', '\u{100001}'] {
        let new = subset.glyph_index(cp).unwrap();
        let old = original.glyph_index(cp).unwrap();
        assert_eq!(subset.glyph_bounding_box(new), original.glyph_bounding_box(old));
        assert_eq!(subset.glyph_hor_advance(new), original.glyph_hor_advance(old));
    }
    assert!(subset.number_of_glyphs() >= 5);
}

fn shape_glyphs(font_data: &[u8], text: &str, vertical: bool, liga: bool) -> Vec<(u32, i32, i32, i32, i32)> {
    use harfrust::{Direction, Feature, FontRef, ShaperData, Tag, UnicodeBuffer};
    let font = FontRef::from_index(font_data, 0).unwrap();
    let shaper = ShaperData::new(&font);
    let shaper = shaper.shaper(&font).build();
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.set_direction(if vertical { Direction::TopToBottom } else { Direction::LeftToRight });
    buffer.guess_segment_properties();
    let features = [Feature::new(Tag::new(b"vert"), u32::from(vertical), ..), Feature::new(Tag::new(b"liga"), u32::from(liga), ..)];
    let result = shaper.shape(buffer, &features);
    result.glyph_infos().iter().zip(result.glyph_positions()).map(|(g, p)| (g.glyph_id, p.x_advance, p.y_advance, p.x_offset, p.y_offset)).collect()
}

#[test]
fn preserves_vertical_and_ligature_shaping_and_metrics_for_ttf_and_cff() {
    for input in [TTF, CFF] {
        let output = cut(input, "AA");
        let original = Face::parse(input, 0).unwrap();
        let subset = Face::parse(&output, 0).unwrap();
        assert!(subset.tables().gsub.is_some(), "GSUB must remain in the subset");
        assert!(subset.tables().gpos.is_some(), "GPOS must remain in the subset");
        for (text, vertical, liga) in [("A", false, false), ("A", true, false), ("AA", false, true), ("AA", false, false)] {
            let before = shape_glyphs(input, text, vertical, liga);
            let after = shape_glyphs(&output, text, vertical, liga);
            assert_eq!(before.len(), after.len(), "{text:?} vertical={vertical} liga={liga}");
            for (old, new) in before.iter().zip(&after) {
                assert_eq!((old.1, old.2, old.3, old.4), (new.1, new.2, new.3, new.4), "position {text:?} vertical={vertical} liga={liga}");
                let (o, n) = (ttf_parser::GlyphId(old.0 as u16), ttf_parser::GlyphId(new.0 as u16));
                assert_eq!(original.glyph_bounding_box(o), subset.glyph_bounding_box(n), "outline {text:?} vertical={vertical} liga={liga}");
                assert_eq!(original.glyph_ver_advance(o), subset.glyph_ver_advance(n));
                assert_eq!(original.glyph_y_origin(o), subset.glyph_y_origin(n));
            }
        }
        assert_ne!(shape_glyphs(&output, "A", true, false)[0].0, shape_glyphs(&output, "A", false, false)[0].0);
        assert_eq!(shape_glyphs(&output, "AA", false, true).len(), 1);
    }
}

#[test]
fn invalid_and_unmatched_fonts_do_not_produce_a_subset() {
    for input in [&[][..], b"not a font", &TTF[..12]] {
        let set: BTreeSet<u32> = [u32::from('A')].into();
        assert!(subset(input, &set, "f", "f", "f", Hinting::Keep).is_err());
    }
}

/// The name records: copyright (0) and licence (13, 14) of the source stay; the family, full, PostScript and unique names are the block's own.
#[test]
fn name_records_are_renamed_and_the_attribution_stays() {
    use write_fonts::read::{FontRef, TableProvider};
    for input in [TTF, CFF] {
        let source = FontRef::new(input).unwrap();
        let output = cut(input, "A");
        let font = FontRef::new(&output).unwrap();
        let read = |f: &FontRef, id: u16| {
            let names = f.name().unwrap();
            names.name_record().iter().filter(|r| r.name_id().to_u16() == id).filter_map(|r| r.string(names.string_data()).ok().map(|s| s.chars().collect::<String>())).collect::<Vec<_>>()
        };
        for id in [0u16, 13, 14] {
            let before = read(&source, id);
            let after = read(&font, id);
            assert!(after.iter().all(|t| before.contains(t)), "name {id} keeps the source's text");
            assert_eq!(before.is_empty(), after.is_empty(), "name {id} is kept when the source has it");
        }
        assert_eq!(read(&font, 1), ["ibis Fixture"]);
        assert_eq!(read(&font, 6), ["ibis-Fixture"]);
    }
}
