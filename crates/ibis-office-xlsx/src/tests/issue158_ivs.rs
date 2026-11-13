// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #158: in a cell being edited (and in the formula bar, which edits the same text) a kanji and its variation selector (an IVS,
//! U+E0100..E01EF, or a standardized one, U+FE00..FE0F) are one character for ← →, Shift+← →, Backspace, Delete, an input method's range and
//! a double click's word. The caret of the cell editor counts UTF-16 units: the selector of an IVS is two of them.
use super::SheetSteps;
use crate::shell::scenario::Scenario;
use crate::ext::*;

/// A, the kanji 葛 with U+E0100, B (UTF-16 units: 1, 1+2, 1).
const IVS_TEXT: &str = "A\u{845b}\u{e0100}B";
/// A, 葛 with U+FE00, B (1, 1+1, 1).
const STANDARD_TEXT: &str = "A\u{845b}\u{fe00}B";

fn editing(text: &str) -> Scenario {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Workbook);
    s.size(1200., 720.);
    s.click_cell("A1");
    s.ime_commit(text);
    // F2: the arrows move the caret in the text.
    s.key("f2");
    s
}

fn state(s: &mut Scenario) -> (String, usize) {
    s.with(|e, _| {
        let c = e.cell_edit().expect("a cell is edited");
        (c.text.clone(), c.caret)
    })
}

pub(super) fn caret_moves_over_the_pair() {
    for (text, positions) in [(IVS_TEXT, [5usize, 4, 1, 0]), (STANDARD_TEXT, [4, 3, 1, 0])] {
        let mut s = editing(text);
        assert_eq!(state(&mut s).1, positions[0], "{text:?}: the caret is at the end");
        for want in &positions[1..] {
            s.key("left");
            assert_eq!(state(&mut s).1, *want, "{text:?}: ← goes over the pair");
        }
        for want in positions.iter().rev().skip(1) {
            s.key("right");
            assert_eq!(state(&mut s).1, *want, "{text:?}: → goes over the pair");
        }
    }
}

pub(super) fn shift_arrows_select_the_pair() {
    let mut s = editing(IVS_TEXT);
    s.key("left");
    s.key("shift-left");
    let (a, c) = s.with(|e, _| {
        let c = e.cell_edit().unwrap();
        (c.anchor, c.caret)
    });
    assert_eq!((a, c), (Some(4), 1), "Shift+← takes the kanji and its selector");
    s.key("shift-right");
    assert_eq!(state(&mut s).1, 4, "Shift+→ gives it back whole");
}

pub(super) fn backspace_and_delete_take_the_pair() {
    for text in [IVS_TEXT, STANDARD_TEXT] {
        let mut s = editing(text);
        s.key("left");
        s.key("backspace");
        assert_eq!(state(&mut s), ("AB".to_string(), 1), "{text:?}: Backspace takes the pair");
        drop(s);
        let mut s = editing(text);
        s.key("left");
        s.key("left");
        s.key("delete");
        assert_eq!(state(&mut s), ("AB".to_string(), 1), "{text:?}: Delete takes the pair");
    }
}

/// A character outside the BMP (U+100000, two UTF-16 units) is one step too.
pub(super) fn plane16_character_is_one_step() {
    let mut s = editing("A\u{100000}B");
    s.key("left");
    s.key("left");
    assert_eq!(state(&mut s).1, 1);
    s.key("delete");
    assert_eq!(state(&mut s), ("AB".to_string(), 1));
}

pub(super) fn an_ime_range_between_the_pair_takes_the_pair() {
    let mut s = editing(STANDARD_TEXT);
    // The range is the selector alone (UTF-16 units 2..3): the kanji goes with it.
    s.update(|e, _, cx| e.cell_replace(Some(2..3), "X", None, cx));
    assert_eq!(state(&mut s).0, "AXB");
    drop(s);
    let mut s = editing(STANDARD_TEXT);
    // The range ends after the kanji, before its selector.
    s.update(|e, _, cx| e.cell_replace(Some(0..2), "X", None, cx));
    assert_eq!(state(&mut s).0, "XB");
}

pub(super) fn a_word_keeps_the_pair() {
    let text = "ab \u{845b}\u{e0100}\u{57ce} cd";
    // A double click on the kanji selects the kanji run with the selector (bytes of the text).
    let at = text.find('\u{845b}').unwrap();
    let (a, b) = crate::cell_edit::word_range(text, at);
    assert_eq!(&text[a..b], "\u{845b}\u{e0100}\u{57ce}");
    // A double click on the selector itself is the same word.
    let (a, b) = crate::cell_edit::word_range(text, at + '\u{845b}'.len_utf8());
    assert_eq!(&text[a..b], "\u{845b}\u{e0100}\u{57ce}");
}
