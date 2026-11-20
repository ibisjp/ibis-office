// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The blocks of a cut font (GitHub issue #157, stage 2), with IBM Plex Sans (OFL, in the repository for the editor's own UI): the blocks have all the
//! font's characters once, none passes the limit, the cut is the same each time, no block has the font's name, and the text system takes the blocks as a
//! chain (a block without the letter `m` too) and draws each character from the block that has it.

use std::borrow::Cow;
use std::collections::BTreeSet;

use gpui::{FontFallbacks, FontRun, PlatformTextSystem, font, px};
use gpui_wgpu::CosmicTextSystem;
use ibis_font_subsetter::{Hinting, Options, cut, font_code_points};
use skrifa::MetadataProvider as _;
use write_fonts::read::{FontRef, TableProvider};

fn plex() -> Vec<u8> {
    std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/ibis-office-app/fonts/IBMPlexSans-Regular.ttf")).expect("the UI font of the editor")
}

const LIMIT: usize = 24 * 1024;

fn options() -> Options {
    Options { id: "UiFont".into(), name: "Ui Font".into(), chunk: LIMIT, hinting: Hinting::Keep }
}

#[test]
fn the_blocks_have_every_character_once_and_none_passes_the_limit() {
    let bytes = plex();
    let all = font_code_points(&FontRef::new(&bytes).unwrap());
    let cut = cut(&bytes, &options()).unwrap();
    assert!(cut.blocks.len() > 2, "{} blocks: the limit cuts the rest", cut.blocks.len());
    assert_eq!(cut.blocks[0].key, "base");
    assert!(cut.left.is_empty());
    let mut seen = BTreeSet::new();
    for (i, block) in cut.blocks.iter().enumerate() {
        let own: BTreeSet<u32> = block.ranges.iter().flat_map(|r| r[0]..=r[1]).collect();
        assert!(seen.is_disjoint(&own), "block {i} repeats a character of an earlier block");
        seen.extend(own);
        if i > 0 {
            assert!(block.deflated_size <= LIMIT, "block {} is {} bytes deflated", block.key, block.deflated_size);
        }
        assert!(block.file.starts_with(&format!("{}.", block.key)) && block.file.ends_with(".ttf"));
    }
    assert_eq!(seen, all, "every character of the font is in a block");
    // The base block has ASCII.
    let base: BTreeSet<u32> = cut.blocks[0].ranges.iter().flat_map(|r| r[0]..=r[1]).collect();
    assert!((0x20..0x7f).all(|c| base.contains(&c)));
}

#[test]
fn the_same_input_gives_the_same_blocks() {
    let bytes = plex();
    let (a, b) = (cut(&bytes, &options()).unwrap(), cut(&bytes, &options()).unwrap());
    assert_eq!(a.blocks.len(), b.blocks.len());
    for (x, y) in a.blocks.iter().zip(&b.blocks) {
        assert_eq!((&x.file, &x.sha256), (&y.file, &y.sha256));
        assert_eq!(x.bytes, y.bytes);
    }
}

#[test]
fn a_block_names_itself_and_not_the_font_it_was_cut_from() {
    let bytes = plex();
    let cut = cut(&bytes, &options()).unwrap();
    for block in &cut.blocks {
        let font = FontRef::new(&block.bytes).unwrap();
        let names = font.name().unwrap();
        let mut found_family = false;
        for record in names.name_record() {
            let text = record.string(names.string_data()).map(|s| s.chars().collect::<String>()).unwrap_or_default();
            match record.name_id().to_u16() {
                // The copyright and the licence stay: they name the font's authors.
                0 | 13 | 14 => {}
                id => {
                    assert!(!text.contains("Plex") && !text.contains("IBM"), "name {id} of block {} says {text:?}", block.key);
                    if id == 1 {
                        assert_eq!(text, block.family);
                        found_family = true;
                    }
                    if id == 6 {
                        assert!(text.starts_with("ibis-UiFont-"), "{text}");
                    }
                }
            }
        }
        assert!(found_family, "the family name of {}", block.key);
        assert!(block.family.starts_with("ibis Ui Font "));
    }
}

#[test]
fn the_text_system_takes_the_blocks_as_a_chain_and_draws_each_character_from_its_block() {
    let bytes = plex();
    let cut = cut(&bytes, &options()).unwrap();
    // A character (not the letter m's) of each of two later blocks, and one of the base block.
    let char_of = |block: usize| char::from_u32(cut.blocks[block].ranges[0][0]).unwrap();
    let (a, b) = (char_of(1), char_of(cut.blocks.len() - 1));
    // At least one block has no `m` (the text system of the fork keeps it all the same, as it keeps what the application added).
    let has_m = |bytes: &[u8]| FontRef::new(bytes).unwrap().charmap().map('m').is_some();
    assert!(cut.blocks[1..].iter().any(|b| !has_m(&b.bytes)), "a block without the letter m: the test would not show the fork's change");
    let system = CosmicTextSystem::new_without_system_fonts(&cut.blocks[0].family);
    system.add_fonts(cut.blocks.iter().map(|b| Cow::Owned(b.bytes.clone())).collect()).unwrap();
    let mut chain = font(cut.blocks[0].family.clone());
    chain.fallbacks = Some(FontFallbacks::from_fonts(cut.blocks[1..].iter().map(|b| b.family.to_string()).collect()));
    // The blocks after the base one may have no `m` (the text system keeps what the application added all the same).
    let ids: Vec<_> = cut.blocks.iter().map(|b| system.font_id(&font(b.family.clone())).expect("the block is a font of the system")).collect();
    let id = system.font_id(&chain).unwrap();
    let text = format!("A{a}{b}");
    let layout = system.layout_line(&text, px(16.0), &[FontRun { len: text.len(), font_id: id }]);
    let glyph_fonts: Vec<(char, usize)> = layout
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(move |g| (g.id.0, run.font_id.0)))
        .map(|(glyph, font)| (char::from_u32(glyph).unwrap_or('?'), font))
        .collect();
    let used: BTreeSet<usize> = layout.runs.iter().map(|r| r.font_id.0).collect();
    assert!(used.len() >= 2, "the characters came from more than one block: {glyph_fonts:?}");
    assert!(layout.runs.iter().flat_map(|r| &r.glyphs).all(|g| g.id.0 != 0), "no character is drawn as .notdef");
    assert!(used.contains(&ids[0].0) || used.contains(&id.0));
}
