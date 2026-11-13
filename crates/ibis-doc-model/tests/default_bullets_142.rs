// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

//! A new bullet list has Japanese Word 365's default bullets (#142, #M140: tasks/doc/m140/numbering.xml):
//! Wingdings F06C, F0D8 (the arrowhead of the second level), F0B2 in turn, 440 twips a level.

use ibis_doc_model::Numbering;

#[test]
fn a_new_bullet_list_has_words_wingdings_bullets() {
    let (numbering, id) = Numbering::default().with_new_list(true);
    let codes = ['\u{f06c}', '\u{f0d8}', '\u{f0b2}'];
    let stand_ins = ["\u{25cf}", "\u{27a2}", "\u{2727}"];
    for level in 0..9u32 {
        let lvl = numbering.level(id, level).expect("level");
        let i = level as usize % 3;
        assert_eq!(lvl.format, "bullet");
        assert_eq!(lvl.symbol, Some(("Wingdings".to_string(), codes[i])), "level {level}");
        assert_eq!(lvl.text, stand_ins[i], "level {level}");
        assert_eq!(lvl.para.left, Some(440 * 635 * (level as i64 + 1)));
        assert_eq!(lvl.para.first_line, Some(-440 * 635));
    }
}

#[test]
fn a_new_numbered_list_has_no_symbol_font() {
    let (numbering, id) = Numbering::default().with_new_list(false);
    assert!((0..9).all(|l| numbering.level(id, l).unwrap().symbol.is_none()));
}
