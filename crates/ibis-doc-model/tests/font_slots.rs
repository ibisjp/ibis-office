// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The font slot of a character: [MS-OI29500] 2.1.88 (Part 1 17.3.2.26, rFonts), checked against Word 365's PDFs of tasks/doc/m76-word-font-slots.md and
//! m78-word-fallback-fonts.md (ascii Arial, hAnsi Courier New, eastAsia ＭＳ 明朝; the slot of each character is the typeface Word drew it in, where
//! that typeface has the character).

use ibis_doc_model::{FlowRunStyle, FlowStyles, RunFormat, flow::{FontSlot, font_slot}};

fn style(hint: bool) -> FlowRunStyle {
    FlowRunStyle {
        font: Some("Arial".into()),
        h_ansi_font: Some("Courier New".into()),
        east_asian_font: Some("MS Mincho".into()),
        east_asia_hint: hint,
        ..Default::default()
    }
}

fn slot_fonts(hint: bool, text: &str) -> Vec<&'static str> {
    let s = style(hint);
    text.chars()
        .map(|c| match s.font_for(c) {
            Some("Arial") => "ascii",
            Some("Courier New") => "hAnsi",
            Some("MS Mincho") => "eastAsia",
            other => panic!("{c:?}: {other:?}"),
        })
        .collect()
}

#[test]
fn without_the_hint() {
    assert_eq!(slot_fonts(false, "abc"), ["ascii"; 3]);
    // é ■ □ … ‘ ’ → × ÷ ° ± α Я “ ” — ‰ § † ♪ ☆ ○ ※ ① (hAnsi's frame; ※ and ① are drawn in other typefaces by Word because Courier New lacks them, M78).
    assert_eq!(slot_fonts(false, "é■□…‘’→×÷°±αЯ“”—‰§†♪☆○※①ⅠⅡ℃"), ["hAnsi"; 27]);
    // 、。漢あｱＡ ㈱ 〒 ㎡: eastAsia's, hAnsi's typeface having them or not (M78).
    assert_eq!(slot_fonts(false, "、。漢あｱＡ㈱〒㎡"), ["eastAsia"; 9]);
}

#[test]
fn with_the_hint() {
    assert_eq!(slot_fonts(true, "abc"), ["ascii"; 3]);
    assert_eq!(slot_fonts(true, "é"), ["hAnsi"]);
    // Everything else, Latin-1 punctuation and the symbols too (M76, M78).
    assert_eq!(slot_fonts(true, "■□…‘’※①→×÷°±αЯ“”—‰§†♪☆○ⅠⅡ℃"), ["eastAsia"; 26]);
}

#[test]
fn the_slot_does_not_depend_on_the_neighbours() {
    // Q1 – Q6 of M76: the quotes are hAnsi's among Japanese and among Latin text alike.
    let s = style(false);
    for text in ["漢‘字’漢", "abc‘def’abc", "‘’“”"] {
        for c in text.chars().filter(|c| "‘’“”".contains(*c)) {
            assert_eq!(s.font_for(c), Some("Courier New"), "{text}");
        }
    }
}

#[test]
fn ranges_of_the_table() {
    assert_eq!(font_slot('\u{7f}', false), FontSlot::Ascii);
    assert_eq!(font_slot('\u{a0}', false), FontSlot::HAnsi);
    assert_eq!(font_slot('\u{d7}', true), FontSlot::EastAsia);
    assert_eq!(font_slot('\u{e9}', true), FontSlot::HAnsi);
    assert_eq!(font_slot('\u{100}', true), FontSlot::HAnsi);
    assert_eq!(font_slot('\u{2b0}', true), FontSlot::EastAsia);
    assert_eq!(font_slot('\u{5d0}', false), FontSlot::Ascii);
    assert_eq!(font_slot('\u{1100}', false), FontSlot::EastAsia);
    assert_eq!(font_slot('\u{27bf}', false), FontSlot::HAnsi);
    assert_eq!(font_slot('\u{27bf}', true), FontSlot::EastAsia);
    assert_eq!(font_slot('\u{27c0}', true), FontSlot::HAnsi);
    assert_eq!(font_slot('\u{2f00}', false), FontSlot::EastAsia);
    assert_eq!(font_slot('\u{3000}', false), FontSlot::EastAsia);
    assert_eq!(font_slot('\u{33ff}', false), FontSlot::EastAsia);
    assert_eq!(font_slot('\u{ac00}', false), FontSlot::EastAsia);
    assert_eq!(font_slot('\u{e000}', false), FontSlot::HAnsi);
    assert_eq!(font_slot('\u{e000}', true), FontSlot::EastAsia);
    assert_eq!(font_slot('\u{fb1d}', true), FontSlot::Ascii);
    assert_eq!(font_slot('\u{ff21}', false), FontSlot::EastAsia);
    assert_eq!(font_slot('\u{20000}', false), FontSlot::EastAsia);
}

#[test]
fn eastasia_times_new_roman_with_equal_ascii_and_hansi_uses_ascii() {
    let s = FlowRunStyle { font: Some("Arial".into()), east_asian_font: Some("Times New Roman".into()), ..Default::default() };
    assert_eq!(s.font_for('漢'), Some("Arial"));
}

#[test]
fn the_hint_of_a_direct_format_reaches_the_style() {
    let direct = RunFormat { east_asia_hint: Some(true), ..RunFormat::default() };
    let base = RunFormat { font: Some("Arial".into()), h_ansi_font: Some("Courier New".into()), east_asian_font: Some("MS Mincho".into()), ..RunFormat::default() };
    let s = FlowStyles::default().run_format(&base, None, &direct).style();
    assert_eq!(s.font_for('■'), Some("MS Mincho"));
    assert_eq!(s.font_for('é'), Some("Courier New"));
}

#[test]
fn fallback_typefaces_of_m85() {
    use ibis_doc_model::flow::fallback_typeface;
    // The characters of #M78, as #M85 gives them (one table for every hAnsi typeface).
    for c in "※ⅠⅡⅰ".chars() {
        assert_eq!(fallback_typeface(c), Some("ＭＳ 明朝"), "{c:?}");
    }
    for c in "①◎△▽◆◇⇒≦∴℃‐‑‖".chars() {
        assert_eq!(fallback_typeface(c), Some("Cambria Math"), "{c:?}");
    }
    for c in "☆★♪".chars() {
        assert_eq!(fallback_typeface(c), Some("Segoe UI Symbol"), "{c:?}");
    }
    assert_eq!(fallback_typeface('■'), Some("Courier New"));
    // #M98 (every character set apart from its neighbours): ▲ ▼ ‒ in Courier New, which #M85 (characters side by side) had in Cambria Math, carried over.
    for c in "▲▼‒⁞►◄◘◙".chars() {
        assert_eq!(fallback_typeface(c), Some("Courier New"), "{c:?}");
    }
    // The pictographs that are emoji are drawn in Segoe UI Emoji, the circled letters ⓐ in MS Mincho, Ⓐ and ⓵ in Segoe UI Symbol, ⓪ and ❶ in Cambria Math.
    for c in "☔♈♓✅❓➕◽".chars() {
        assert_eq!(fallback_typeface(c), Some("Segoe UI Emoji"), "{c:?}");
    }
    assert_eq!(fallback_typeface('ⓐ'), Some("ＭＳ 明朝"));
    assert_eq!(fallback_typeface('Ⓐ'), Some("Segoe UI Symbol"));
    assert_eq!(fallback_typeface('⓵'), Some("Segoe UI Symbol"));
    assert_eq!(fallback_typeface('⓪'), Some("Cambria Math"));
    assert_eq!(fallback_typeface('❶'), Some("Cambria Math"));
    // A block's ranges: arrows and mathematical operators in Cambria Math, miscellaneous symbols in Segoe UI Symbol, ①–⑳ in Cambria Math.
    assert_eq!(fallback_typeface('\u{2200}'), Some("Cambria Math"));
    assert_eq!(fallback_typeface('\u{2600}'), Some("Segoe UI Symbol"));
    assert_eq!(fallback_typeface('\u{2473}'), Some("Cambria Math"));
    // Not measured: none.
    for c in "a漢é㈱〒㎡".chars() {
        assert_eq!(fallback_typeface(c), None, "{c:?}");
    }
}
