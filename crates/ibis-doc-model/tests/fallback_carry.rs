// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Word's carrying over of a fallback typeface from the character before (#M98, tasks/doc/m98-word-fallback-carry.md): the cases of the Windows measurement.

use ibis_doc_model::flow::{CarryInput, FallbackCarry, FontSlot};

/// The glyphs the typefaces have, as far as the cases need (the cmaps of the font files: tasks/doc/m85-font-coverage.tsv, m98).
fn has(typeface: &str, c: char) -> Option<bool> {
    let chars = match typeface {
        "Cambria Math" => "‐‑‒⁞△▲▼■□⅓☀☆",
        "Courier New" => "‒▲▼■□◘⅓",
        "Segoe UI Symbol" => "☀☆☔ⓐⒶ",
        "ＭＳ 明朝" => "ⓐ☀Ⅰ‒▲",
        "Segoe UI Emoji" => "☔☀",
        _ => return None,
    };
    Some(chars.contains(c))
}

/// Run `text` on a line of Century (it has none of the characters of the cases but the ones in `century`): the typeface of each character, `-` for Century's own.
fn run(text: &str, century: &str) -> Vec<String> {
    run_with(text, century, |_| false, |_| false)
}

fn run_with(text: &str, century: &str, line_start: impl Fn(usize) -> bool, cut_before: impl Fn(usize) -> bool) -> Vec<String> {
    let mut carry = FallbackCarry::default();
    text.chars()
        .enumerate()
        .map(|(i, c)| {
            if cut_before(i) {
                carry.reset();
            }
            let slot = if "漢あ".contains(c) { FontSlot::EastAsia } else { FontSlot::HAnsi };
            let input = CarryInput { c, slot, typeface: "Century", bold: false, italic: false, line_start: line_start(i), has_glyph: century.contains(c) || c.is_ascii() };
            carry.next(input, has).fallback.map_or("-".to_string(), str::to_string)
        })
        .collect()
}

#[test]
fn a_character_set_apart_takes_the_table() {
    assert_eq!(run("‒", ""), ["Courier New"]);
    assert_eq!(run("‐", ""), ["Cambria Math"]);
    assert_eq!(run("ⓐ", ""), ["ＭＳ 明朝"]);
}

#[test]
fn the_typeface_of_the_character_before_is_carried_if_it_has_the_glyph() {
    // ‒ is Courier New set apart; after ‐ (Cambria Math, which has it) it is Cambria Math. #M98 (c) 1.
    assert_eq!(run("‐‒", ""), ["Cambria Math", "Cambria Math"]);
    assert_eq!(run("‑‒", ""), ["Cambria Math", "Cambria Math"]);
    assert_eq!(run("‒⅓", ""), ["Courier New", "Courier New"]);
    assert_eq!(run("☀ⓐ", ""), ["Segoe UI Symbol", "Segoe UI Symbol"]);
    assert_eq!(run("ⓐ☀", ""), ["ＭＳ 明朝", "ＭＳ 明朝"]);
    assert_eq!(run("☔☀", ""), ["Segoe UI Emoji", "Segoe UI Emoji"]);
    // The typeface before has no glyph: the table. ‐ is Cambria Math, which lacks ⓐ.
    assert_eq!(run("‐ⓐ", ""), ["Cambria Math", "ＭＳ 明朝"]);
    // A typeface not known (no cmap) is not carried.
    assert_eq!(run("⁇ⓐ", ""), ["Malgun Gothic", "ＭＳ 明朝"]);
}

#[test]
fn the_carrying_over_goes_on_and_does_not_act_backwards() {
    assert_eq!(run("‐‒‒‒‒‒‒‒‒‒", ""), vec!["Cambria Math"; 10]);
    // The carried typeface changes where it lacks the glyph: Cambria Math has no ⓐ, which takes the table (ＭＳ 明朝), and the ‒ after it is carried from that.
    assert_eq!(run("‐‒ⓐ‒", ""), ["Cambria Math", "Cambria Math", "ＭＳ 明朝", "ＭＳ 明朝"]);
    // Backwards: ‒ before ‐ stays Courier New.
    assert_eq!(run("‒‐", ""), ["Courier New", "Cambria Math"]);
    assert_eq!(run("‒‒‐", ""), ["Courier New", "Courier New", "Cambria Math"]);
}

#[test]
fn what_cuts_the_carrying_over() {
    // A space, a no-break space, a tab, a character Century has, a character of the East Asian slot.
    for cut in [" ", "\u{a0}", "\t", ",", "a", "漢", "あ"] {
        let text = format!("‐{cut}‒");
        assert_eq!(run(&text, "")[2], "Courier New", "{cut:?}");
    }
    // A character that the typeface has itself (a Century glyph, here ▲) cuts too.
    assert_eq!(run("‐▲‒", "▲"), ["Cambria Math", "-", "Courier New"]);
    // A line break or a paragraph's end before the character (`reset`).
    assert_eq!(run_with("‐‒", "", |_| false, |i| i == 1), ["Cambria Math", "Courier New"]);
    // The first character of a wrapped line: Word draws it as set apart; the next one carries from it.
    assert_eq!(run_with("‐‒‒", "", |i| i == 1, |_| false), ["Cambria Math", "Courier New", "Courier New"]);
}

#[test]
fn what_does_not_cut_the_carrying_over() {
    // A zero width space is seen through.
    assert_eq!(run("‐\u{200b}‒", "")[2], "Cambria Math");
}

#[test]
fn a_change_of_the_runs_typeface_bold_or_italic_cuts_it() {
    let draw = |first: (&str, bool, bool), second: (&str, bool, bool)| {
        let mut carry = FallbackCarry::default();
        let mut one = |c: char, (typeface, bold, italic): (&str, bool, bool)| {
            carry.next(CarryInput { c, slot: FontSlot::HAnsi, typeface, bold, italic, line_start: false, has_glyph: false }, has).fallback
        };
        one('‐', first);
        one('‒', second)
    };
    let plain = ("Century", false, false);
    assert_eq!(draw(plain, plain), Some("Cambria Math"));
    assert_eq!(draw(plain, ("Century", true, false)), Some("Courier New"));
    assert_eq!(draw(("Century", true, false), plain), Some("Courier New"));
    assert_eq!(draw(plain, ("Century", false, true)), Some("Courier New"));
    assert_eq!(draw(("Century", true, false), ("Century", true, false)), Some("Cambria Math"));
    assert_eq!(draw(plain, ("Calibri", false, false)), Some("Courier New"));
}

#[test]
fn the_choice_says_when_the_carrying_over_changed_the_typeface() {
    let mut carry = FallbackCarry::default();
    let input = |c| CarryInput { c, slot: FontSlot::HAnsi, typeface: "Century", bold: false, italic: false, line_start: false, has_glyph: false };
    assert!(!carry.next(input('‐'), has).carried);
    assert!(carry.next(input('‒'), has).carried);
    // Same typeface as set apart: nothing to redo at a line's start.
    carry.reset();
    carry.next(input('‒'), has);
    assert!(!carry.next(input('▲'), has).carried);
}
