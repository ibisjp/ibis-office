// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #158: in the text of a text box (placeholders and table cells take the same editing) a kanji and its variation selector
//! (an IVS, U+E0100..E01EF, or a standardized one, U+FE00..FE0F) are one character for ← →, Shift+← →, Backspace, Delete, an input
//! method's range and a double click's word. The caret counts characters (the selector is one).
use crate::shell::scenario::Scenario;
use ibis_doc_model::{EmuRect, Shape, ShapeId};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue158_caret_moves_over_the_pair", caret_moves_over_the_pair as fn()),
        ("pptx::issue158_shift_arrows_select_the_pair", shift_arrows_select_the_pair),
        ("pptx::issue158_backspace_and_delete_take_the_pair", backspace_and_delete_take_the_pair),
        ("pptx::issue158_an_ime_range_between_the_pair_takes_the_pair", an_ime_range_between_the_pair_takes_the_pair),
        ("pptx::issue158_a_double_click_word_keeps_the_pair", a_double_click_word_keeps_the_pair),
    ]
}

/// A, the kanji with U+E0100, B.
const IVS_TEXT: &str = "A\u{845b}\u{e0100}B";
/// A, the kanji with U+FE00, B.
const STANDARD_TEXT: &str = "A\u{845b}\u{fe00}B";

/// A text box with `text` edited, the caret at its end.
fn editing(text: &str) -> (Scenario, ShapeId) {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let id = s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        e.insert_shape(Shape::text_box(id, EmuRect::new(500_000, 500_000, 3_000_000, 900_000), text), cx);
        id
    });
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| sh.id == id).unwrap());
    s.double_click_shape(index);
    s.update(|e, _, cx| {
        let len = e.edited_text().map_or(0, |t| t.chars().count());
        e.shape_move_caret(len, false, cx);
    });
    (s, id)
}

fn caret(s: &mut Scenario) -> usize {
    s.with(|e, _| e.text_editing().expect("a text is edited").caret)
}

fn text_of(s: &mut Scenario, id: ShapeId) -> String {
    s.with(|e, _| e.slide().shape(id).unwrap().text_body().unwrap().text())
}

fn caret_moves_over_the_pair() {
    for (text, positions) in [(IVS_TEXT, [4usize, 3, 1, 0]), (STANDARD_TEXT, [4, 3, 1, 0])] {
        let (mut s, _) = editing(text);
        assert_eq!(caret(&mut s), positions[0], "{text:?}: the caret is at the end");
        for want in &positions[1..] {
            s.key("left");
            assert_eq!(caret(&mut s), *want, "{text:?}: ← goes over the pair");
        }
        for want in positions.iter().rev().skip(1) {
            s.key("right");
            assert_eq!(caret(&mut s), *want, "{text:?}: → goes over the pair");
        }
    }
}

fn shift_arrows_select_the_pair() {
    let (mut s, _) = editing(IVS_TEXT);
    s.key("left");
    s.key("shift-left");
    assert_eq!(s.with(|e, _| e.text_editing().map(|t| t.range())), Some(1..3), "Shift+← takes the kanji and its selector");
    s.key("shift-right");
    assert_eq!(s.with(|e, _| e.text_editing().map(|t| t.range())), Some(3..3));
}

fn backspace_and_delete_take_the_pair() {
    for text in [IVS_TEXT, STANDARD_TEXT] {
        let (mut s, id) = editing(text);
        s.key("left");
        s.key("backspace");
        assert_eq!(text_of(&mut s, id), "AB", "{text:?}: Backspace takes the pair");
        assert_eq!(caret(&mut s), 1);
        drop(s);
        let (mut s, id) = editing(text);
        s.key("left");
        s.key("left");
        s.key("delete");
        assert_eq!(text_of(&mut s, id), "AB", "{text:?}: Delete takes the pair");
        assert_eq!(caret(&mut s), 1);
    }
}

fn an_ime_range_between_the_pair_takes_the_pair() {
    let (mut s, id) = editing(STANDARD_TEXT);
    // The range is the selector alone (UTF-16 units 2..3): the kanji goes with it.
    s.update(|e, _, cx| e.input_replace(Some(2..3), "X", cx));
    assert_eq!(text_of(&mut s, id), "AXB");
    drop(s);
    let (mut s, id) = editing(IVS_TEXT);
    // The range ends after the kanji, before its selector (the selector is two UTF-16 units).
    s.update(|e, _, cx| e.input_replace(Some(0..2), "X", cx));
    assert_eq!(text_of(&mut s, id), "XB");
}

fn a_double_click_word_keeps_the_pair() {
    let text = "ab \u{845b}\u{e0100}\u{57ce} cd";
    let (mut s, _) = editing(text);
    // A word around the kanji (and around its selector) takes the whole kanji run.
    for at in [3, 4] {
        let (a, b) = s.with(|e, _| e.text_unit_around(at, false));
        let word: String = text.chars().skip(a).take(b - a).collect();
        assert_eq!(word.trim_end(), "\u{845b}\u{e0100}\u{57ce}", "at {at}");
    }
}
