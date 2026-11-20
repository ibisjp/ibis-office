// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #188, the presentation (PPTX) side: which characters stand and which lie in a column of vertical text (`vert="eaVert"`).
//! The characters are those of the issue's `n5-orientation-diff.tsv` where PowerPoint (Microsoft 365 for Windows, PDF) and ibisOffice 0.8.6 differed
//! and the UAX #50 orientation sides with PowerPoint.

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue188_half_width_katakana_lie_in_a_column", half_width_katakana_lie_in_a_column),
        ("pptx::issue188_symbols_uax50_upright_stand_in_a_column", symbols_uax50_upright_stand_in_a_column),
        ("pptx::issue188_powerpoint_lies_the_blocks_it_turns_and_stands_the_rest", powerpoint_eavert_orientation),
    ]
}

/// The 63 half-width katakana and marks (U+FF61..FF9F; UAX #50 R) lie in PowerPoint's column.
fn half_width_katakana_lie_in_a_column() {
    let lying = "｡｢｣､･ｦｧｨｩｪｫｬｭｮｯｰｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜﾝﾞﾟ";
    assert_eq!(lying.chars().count(), 63);
    for c in lying.chars() {
        assert!(!crate::render::vertical_text::upright(c), "U+{:04X} lies", c as u32);
    }
}

/// The 47 symbols (☆ ♪ ❶ ➀ ★ ♡ ✓ ... ; UAX #50 U) stand in PowerPoint's column.
fn symbols_uax50_upright_stand_in_a_column() {
    let standing = "☆☖☗♪♭♯❖❶❷❸❹❺❻❼❽❾❿➀➁➂➃➄➅➆➇➈➉➊➋➌➍➎➏➐➑➒➓★♡♢♤♧♩♫♬♮✓";
    assert_eq!(standing.chars().count(), 47);
    for c in standing.chars() {
        assert!(crate::render::vertical_text::upright(c), "U+{:04X} stands", c as u32);
    }
    // Hiragana, kanji and the like stand as before.
    for c in "あアー漢字１Ａ、。「」".chars() {
        assert!(crate::render::vertical_text::upright(c), "U+{:04X} stands", c as u32);
    }
}

/// PowerPoint's `eaVert` (Microsoft 365 for Windows, XPS, ＭＳ 明朝 18 pt): the characters it turns that UAX #50 stands (the private use area, ⌘ ⏰ ⌒,
/// § ° × €, ℃ Ⅰ, 𝒜, 🂡) and the ones it stands (Hangul, the supplementary kanji, 😀, ☆ ♪ ① ❶ ➀ ✓ ■, the brackets including 〖〗).
fn powerpoint_eavert_orientation() {
    use crate::render::vertical_text::powerpoint_upright as stands;
    for c in "\u{E000}\u{E123}\u{F0000}⌘⏰⌒§°×€℃Ⅰ𝒜🂡←∀─⤀—᐀ｱA".chars() {
        assert!(!stands(c), "U+{:04X} lies", c as u32);
    }
    for c in "가각힣ㄱ가𠀋𠀀😀☆♪❶➀①✓■「」〈〉【】『』〔〕（）‘’“”〖〗〘〙〚〛あ漢".chars() {
        assert!(stands(c), "U+{:04X} stands", c as u32);
    }
}
