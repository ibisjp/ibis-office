// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The bullets of the symbol typefaces (Symbol, Wingdings, Wingdings 2 and 3) read from a file are characters of Unicode that stand in
//! for the glyphs, never the private use character the file writes (GitHub issue 142: ✧, Wingdings 0xB2, was drawn as a box).
//! The file is made by `scripts/gen-docx-bullet-symbols.py`: one bullet for each glyph of `data/symbol-fonts.tsv`.

#[test]
fn every_bullet_of_the_symbol_typefaces_is_a_character_with_a_glyph() {
    let data = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/docx-bullet-symbols.docx")).unwrap();
    let presentation = ibis_pptx::read(&data).unwrap();
    let flow = presentation.flow.as_deref().unwrap();
    let mut texts = Vec::new();
    for abs in flow.styles.numbering.abstracts.values() {
        for level in &abs.levels {
            for c in level.text.chars() {
                let u = c as u32;
                assert!(!(0xe000..=0xf8ff).contains(&u), "a private use character {c:?} (U+{u:04X}) stays as the bullet");
                assert!(!c.is_control(), "{c:?}");
            }
            texts.push(level.text.clone());
        }
    }
    assert!(texts.len() >= 230, "{}", texts.len());
    // The four pointed star of issue 142, and the ones the standard bullet library has.
    for want in ["✧", "●", "■", "◆", "➢", "✓", "▪", "❖", "•", "o"] {
        assert!(texts.iter().any(|t| t == want), "{want}");
    }
}
