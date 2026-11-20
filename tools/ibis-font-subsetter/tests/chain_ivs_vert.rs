// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The blocks of a cut font as a chain, with the variation sequences (IVS) and the vertical glyphs (GitHub issue #157, masanork's question on the fork's comparison: "IVS,
//! vertical glyphs, redrawing after a block is added"). `tests/fonts/chain.ttf` (made by `generate_chain.py`, synthetic): U+3400, U+FF5F, U+20B9F and U+FE35 are outside the
//! base block (not in JIS X 0208), U+FF08 is in it; U+3400 has the variants VS1 and VS17, U+20B9F has VS17; `vert`/`vrt2` turn U+FF5F into a glyph of its own and U+FF08 into
//! the glyph of U+FE35 (a character of another block).

use std::borrow::Cow;

use gpui::{FontFallbacks, FontRun, PlatformTextSystem, font, px};
use gpui_wgpu::CosmicTextSystem;
use std::collections::BTreeSet;

use ibis_font_subsetter::{Hinting, ranges_of, subset};
use ttf_parser::Face;

const CHAIN: &[u8] = include_bytes!("fonts/chain.ttf");

struct Block {
    key: String,
    family: String,
    ranges: Vec<[u32; 2]>,
    bytes: Vec<u8>,
}

/// The base block (ASCII and U+FF08, which is in JIS X 0208) and a block for each of the characters outside it, made with `subset` as `cut` makes its blocks (`cut`'s smallest
/// block limit, 4 KiB, is more than this font): the characters outside the base block each in a block of its own, as a limit that falls short of two of them would put them.
fn blocks() -> Vec<Block> {
    let sets: [(&str, Vec<u32>); 6] = [
        ("base", vec![0x20, 0x41, 0xFF08]),
        ("0001", vec![0x3400]),
        ("0002", vec![0xFF5F]),
        ("0003", vec![0xFE35]),
        ("0004", vec![0x20B9F]),
        ("0005", vec![]),
    ];
    sets.into_iter()
        .filter(|(_, set)| !set.is_empty())
        .map(|(key, set)| {
            let set: BTreeSet<u32> = set.into_iter().collect();
            let family = format!("ibis Chain {key}");
            let bytes = subset(CHAIN, &set, &family, &format!("ibis-Chain-{key}"), &format!("ibis-Chain-{key}-0"), Hinting::Keep).expect("the fixture is cut");
            Block { key: key.into(), family, ranges: ranges_of(&set), bytes }
        })
        .collect()
}

fn block_of<'a>(blocks: &'a [Block], c: char) -> &'a Block {
    blocks.iter().find(|b| b.ranges.iter().any(|r| r[0] <= c as u32 && c as u32 <= r[1])).unwrap_or_else(|| panic!("a block with {c:?}"))
}

fn bbox(face: &Face, id: ttf_parser::GlyphId) -> Option<(i16, i16, i16, i16)> {
    face.glyph_bounding_box(id).map(|r| (r.x_min, r.y_min, r.x_max, r.y_max))
}

/// The base character and its variant glyph go to the block of the base character, whatever block the selector is in (the selectors are in every block): the cut has the
/// variant's outline, and the base block (the first, which lacks the character) has none of it.
#[test]
fn a_variant_glyph_is_in_the_block_of_its_base_character() {
    let blocks = blocks();
    let original = Face::parse(CHAIN, 0).unwrap();
    assert!(blocks.len() >= 5, "{} blocks", blocks.len());
    for (base, selector) in [('\u{3400}', '\u{FE00}'), ('\u{3400}', '\u{E0100}'), ('\u{20B9F}', '\u{E0100}')] {
        let block = block_of(&blocks, base);
        assert_ne!(block.key, "base", "{base:?} is not in the base block");
        let face = Face::parse(&block.bytes, 0).unwrap();
        let new = face.glyph_variation_index(base, selector).unwrap_or_else(|| panic!("{base:?} {selector:?} in block {}", block.key));
        let old = original.glyph_variation_index(base, selector).unwrap();
        assert_eq!(bbox(&face, new), bbox(&original, old), "the variant's outline of {base:?} {selector:?}");
        assert_ne!(Some(new), face.glyph_index(base), "the variant is not the base glyph");
        // The base block has no such character and no variant of it.
        let base_face = Face::parse(&blocks[0].bytes, 0).unwrap();
        assert!(base_face.glyph_variation_index(base, selector).is_none(), "{base:?} {selector:?} not in the base block");
    }
}

/// The text system takes the blocks as a chain: the variant is drawn from the block of the base character, as one glyph for the cluster (base + selector), where the first font
/// of the chain (the base block) has the selector in its `cmap` and not the base character.
#[test]
fn a_variation_sequence_is_drawn_with_its_variant_glyph_from_a_later_block() {
    let blocks = blocks();
    let system = CosmicTextSystem::new_without_system_fonts(&blocks[0].family);
    system.add_fonts(blocks.iter().map(|b| Cow::Owned(b.bytes.clone())).collect()).unwrap();
    let mut chain = font(blocks[0].family.clone());
    chain.fallbacks = Some(FontFallbacks::from_fonts(blocks[1..].iter().map(|b| b.family.to_string()).collect()));
    let chain_id = system.font_id(&chain).unwrap();
    let original = Face::parse(CHAIN, 0).unwrap();
    for (text, base, selector) in [("\u{3400}\u{E0100}", '\u{3400}', Some('\u{E0100}')), ("\u{3400}\u{FE00}", '\u{3400}', Some('\u{FE00}')), ("\u{20B9F}\u{E0100}", '\u{20B9F}', Some('\u{E0100}')), ("\u{3400}", '\u{3400}', None)] {
        let layout = system.layout_line(text, px(16.0), &[FontRun { len: text.len(), font_id: chain_id }]);
        let glyphs: Vec<_> = layout.runs.iter().flat_map(|r| r.glyphs.iter().map(move |g| (r.font_id, g.id.0))).collect();
        assert_eq!(glyphs.len(), 1, "one glyph for {text:?}: {glyphs:?}");
        let block = block_of(&blocks, base);
        let own_id = system.font_id(&font(block.family.clone())).unwrap();
        let (font_id, glyph) = glyphs[0];
        assert_eq!(font_id, own_id, "{text:?} is drawn from the block of {base:?}");
        let face = Face::parse(&block.bytes, 0).unwrap();
        let wanted_old = match selector {
            Some(s) => original.glyph_variation_index(base, s).unwrap(),
            None => original.glyph_index(base).unwrap(),
        };
        assert_eq!(bbox(&face, ttf_parser::GlyphId(glyph as u16)), bbox(&original, wanted_old), "the glyph of {text:?}");
    }
}

fn vertical(bytes: &[u8], c: char) -> Option<ttf_parser::GlyphId> {
    use harfrust::{Direction, Feature, FontRef, ShaperData, Tag, UnicodeBuffer};
    let font = FontRef::from_index(bytes, 0).ok()?;
    let data = ShaperData::new(&font);
    let shaper = data.shaper(&font).build();
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(&c.to_string());
    buffer.set_direction(Direction::TopToBottom);
    buffer.guess_segment_properties();
    let features = [Feature::new(Tag::new(b"vert"), 1, ..), Feature::new(Tag::new(b"vrt2"), 1, ..)];
    let result = shaper.shape(buffer, &features);
    result.glyph_infos().first().filter(|g| g.glyph_id != 0).map(|g| ttf_parser::GlyphId(g.glyph_id as u16))
}

/// `vert` and `vrt2` stay in every block and turn the characters of the block into the vertical glyphs: where the block has the character (U+FF5F in a block of its own) and
/// where the vertical glyph is the glyph of a character of another block (U+FF08, in the base block, to the glyph of U+FE35, in a block of its own).
#[test]
fn the_vertical_glyphs_come_with_the_block_of_the_character_even_when_the_glyph_is_a_character_of_another_block() {
    let blocks = blocks();
    let original = Face::parse(CHAIN, 0).unwrap();
    for (c, vertical_of) in [('\u{FF5F}', original.glyph_index('\u{FF5F}').map(|_| ())), ('\u{FF08}', Some(()))] {
        let _ = vertical_of;
        let block = block_of(&blocks, c);
        let face = Face::parse(&block.bytes, 0).unwrap();
        assert!(face.tables().gsub.is_some(), "GSUB stays in block {}", block.key);
        let new = vertical(&block.bytes, c).unwrap_or_else(|| panic!("a vertical glyph of {c:?} in block {}", block.key));
        let old = vertical(CHAIN, c).unwrap();
        assert_ne!(Some(new), face.glyph_index(c), "{c:?}: the vertical glyph is not the horizontal one");
        assert_eq!(bbox(&face, new), bbox(&original, old), "the vertical glyph of {c:?} in block {}", block.key);
        assert_eq!(face.glyph_ver_advance(new), original.glyph_ver_advance(old), "its vertical advance");
    }
    // U+FE35 is a character of another block than U+FF08's, and the base block does not have it as a character: the glyph is there without the character.
    let base = Face::parse(&blocks[0].bytes, 0).unwrap();
    assert!(base.glyph_index('\u{FE35}').is_none());
    assert_ne!(block_of(&blocks, '\u{FE35}').key, block_of(&blocks, '\u{FF08}').key);
}

/// Drawn through the chain, the vertical glyph is asked of the font that has the character (the run's own font: `vertical_text::vertical_glyph_id(.., run.font_id, c)`): the block
/// of U+FF5F gives its vertical glyph, though the first font of the chain (the base block) does not have the character.
#[test]
fn a_character_of_a_later_block_has_its_vertical_glyph_from_the_font_the_run_is_drawn_with() {
    let blocks = blocks();
    let system = CosmicTextSystem::new_without_system_fonts(&blocks[0].family);
    system.add_fonts(blocks.iter().map(|b| Cow::Owned(b.bytes.clone())).collect()).unwrap();
    let mut chain = font(blocks[0].family.clone());
    chain.fallbacks = Some(FontFallbacks::from_fonts(blocks[1..].iter().map(|b| b.family.to_string()).collect()));
    let chain_id = system.font_id(&chain).unwrap();
    let text = "\u{FF08}\u{FF5F}";
    let layout = system.layout_line(text, px(16.0), &[FontRun { len: text.len(), font_id: chain_id }]);
    let used: Vec<_> = layout.runs.iter().flat_map(|r| r.glyphs.iter().map(move |g| (r.font_id, g.id.0))).collect();
    assert_eq!(used.len(), 2, "{used:?}");
    for ((font_id, _), c) in used.iter().zip(['\u{FF08}', '\u{FF5F}']) {
        let block = block_of(&blocks, c);
        // (The characters of the base block, the chain's first font, are drawn with the chain's own font id.)
        let own = if block.key == "base" { chain_id } else { system.font_id(&font(block.family.clone())).unwrap() };
        assert_eq!(*font_id, own, "{c:?} is drawn from its block");
        let (data, index) = system_font_data(&system, *font_id);
        assert_eq!(index, 0);
        assert!(vertical(&data[..], c).is_some_and(|v| Face::parse(&data, 0).unwrap().glyph_index(c) != Some(v)), "the font the run is drawn with has the vertical glyph of {c:?}");
    }
}

fn system_font_data(system: &CosmicTextSystem, id: gpui::FontId) -> (Vec<u8>, u32) {
    let (data, index) = system.font_data(id).expect("the font's file");
    (data.to_vec(), index)
}
