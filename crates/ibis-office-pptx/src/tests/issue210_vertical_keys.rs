// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #210: in vertical text of a text box the arrows follow the writing, as in PowerPoint and Word: ← → change the line
//! (← the next line in `eaVert`, whose lines go right to left), ↑ ↓ the character; with Shift they select by the same steps.
use crate::shell::scenario::Scenario;
use ibis_doc_model::{EmuRect, Shape, VerticalKind};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue210_arrows_in_ea_vert_text_move_by_line_and_character", ea_vert as fn()),
        ("pptx::issue210_arrows_in_vert270_text_run_the_other_way", vert270),
        ("pptx::issue210_arrows_in_horizontal_text_are_unchanged", horizontal),
        ("pptx::issue210_the_arrow_turning_table", turning_table),
        ("pptx::issue210_page_keys_leave_the_text_and_turn_the_slide", page_keys),
    ]
}

/// A text box of two paragraphs, `ABC` and `DEF`, written as given, with the caret after the A.
fn editing(vertical: bool, kind: VerticalKind, sideways: i32) -> Scenario {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let id = s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        let mut shape = Shape::text_box(id, EmuRect::new(500_000, 500_000, 3_000_000, 3_000_000), "ABC\nDEF");
        if let Some(body) = shape.text_body_mut() {
            body.vertical = vertical;
            body.vertical_kind = kind;
            body.sideways = sideways;
        }
        e.insert_shape(shape, cx);
        id
    });
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| sh.id == id).unwrap());
    s.double_click_shape(index);
    s.settle();
    s.update(|e, _, cx| e.shape_move_caret(1, false, cx));
    s
}

fn caret(s: &mut Scenario) -> usize {
    s.with(|e, _| e.text_editing().expect("a text is edited").caret)
}

fn range(s: &mut Scenario) -> std::ops::Range<usize> {
    s.with(|e, _| e.text_editing().expect("a text is edited").range())
}

fn ea_vert() {
    let mut s = editing(true, VerticalKind::EastAsian, 0);
    s.key("down");
    assert_eq!(caret(&mut s), 2, "↓ goes to the next character");
    s.key("up");
    assert_eq!(caret(&mut s), 1, "↑ goes to the previous character");
    s.key("left");
    assert_eq!(caret(&mut s), 5, "← goes to the next line (the lines go right to left), the same place in it");
    s.key("right");
    assert_eq!(caret(&mut s), 1, "→ goes back to the previous line");
    s.key("shift-down");
    assert_eq!(range(&mut s), 1..2, "Shift+↓ selects a character");
    s.update(|e, _, cx| e.shape_move_caret(1, false, cx));
    s.key("shift-left");
    assert_eq!(range(&mut s), 1..5, "Shift+← selects to the next line");
}

fn vert270() {
    let mut s = editing(false, VerticalKind::EastAsian, 16_200_000);
    s.key("up");
    assert_eq!(caret(&mut s), 2, "↑ goes to the next character (the text runs up)");
    s.key("down");
    assert_eq!(caret(&mut s), 1);
    s.key("right");
    assert_eq!(caret(&mut s), 5, "→ goes to the next line (the lines go left to right)");
    s.key("left");
    assert_eq!(caret(&mut s), 1);
}

fn horizontal() {
    let mut s = editing(false, VerticalKind::EastAsian, 0);
    s.key("right");
    assert_eq!(caret(&mut s), 2);
    s.key("down");
    assert_eq!(caret(&mut s), 6, "↓ goes to the next line, keeping the column");
}

fn turning_table() {
    use crate::shell::text_keys::text_arrow_key as turn;
    use VerticalKind::*;
    // (vertical, kind, sideways): [left, right, up, down]
    let cases = [
        ((true, EastAsian, 0), ["down", "up", "left", "right"]),
        ((false, EastAsian, 5_400_000), ["down", "up", "left", "right"]),
        ((true, Mongolian, 0), ["up", "down", "left", "right"]),
        ((true, WordArt, 0), ["up", "down", "left", "right"]),
        ((true, WordArtRtl, 0), ["down", "up", "left", "right"]),
        ((false, EastAsian, 16_200_000), ["up", "down", "right", "left"]),
        ((false, EastAsian, 0), ["left", "right", "up", "down"]),
    ];
    for ((vertical, kind, sideways), want) in cases {
        for (key, want) in ["left", "right", "up", "down"].into_iter().zip(want) {
            assert_eq!(turn(vertical, kind, sideways, key), want, "{vertical} {kind:?} {sideways} {key}");
        }
    }
    assert_eq!(turn(true, EastAsian, 0, "home"), "home");
}

/// PowerPoint for Windows (2026-11-18): Page Down and Page Up in a text being edited, vertical or not, leave it (nothing is selected) and
/// show the next or the previous slide.
fn page_keys() {
    for (vertical, sideways) in [(false, 0), (true, 0)] {
        let mut s = editing(vertical, VerticalKind::EastAsian, sideways);
        let here = s.with(|e, _| e.presentation().slide_index(e.current_slide)).unwrap();
        s.update(|e, w, cx| e.step_slide(true, w, cx));
        s.settle();
        assert!(s.with(|e, _| e.text_editing().is_none()), "Page Down leaves the text");
        assert_eq!(s.with(|e, _| e.presentation().slide_index(e.current_slide)), Some(here + 1), "and shows the next slide");
        s.key("pageup");
        assert_eq!(s.with(|e, _| e.presentation().slide_index(e.current_slide)), Some(here), "Page Up goes back");
    }
}
