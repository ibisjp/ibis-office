// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0020: the rectangle a cell gives the input method for the text it is composing (where the candidate window opens) is that of the last
//! character of the text, on the line it is on, and not the whole box of the cell.
use super::SheetSteps;
use crate::shell::format::with_format;
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::k0020_the_candidate_rect_is_the_last_character_of_the_composing_text", the_candidate_rect_is_the_last_character),
        ("xlsx::k0020_the_candidate_rect_typed_straight_into_the_cell_is_at_the_cell_before_the_first_paint", || in_the_cell(Entry::Direct)),
        ("xlsx::k0020_the_candidate_rect_after_f2_is_at_the_cell", || in_the_cell(Entry::F2)),
        ("xlsx::k0020_the_candidate_rect_after_a_double_click_is_at_the_cell", || in_the_cell(Entry::DoubleClick)),
        ("xlsx::k0020_the_candidate_rect_after_a_press_on_the_formula_bar_is_at_the_bar", in_the_bar),
    ]
}

fn the_candidate_rect_is_the_last_character() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("K20");
    let text = "\u{3042}\u{3044}\u{3046}\u{3048}\u{304a}";
    s.ime_compose(text);
    s.settle();
    let n = text.encode_utf16().count();
    let (whole, last, via_platform) = s.update(|e, w, cx| {
        let whole = e.cell_edit().and_then(|c| c.rect.get()).expect("the box was painted");
        let last = with_format!(e, bounds_for_range(e, n - 1..n, Default::default(), w, cx)).expect("a rectangle");
        let sel = with_format!(e, selected_text_range(e, true, w, cx)).unwrap();
        let platform = gpui_kit::PlatformInputHandler::compute_ime_candidate_bounds(Some(0..n), &sel, |r| with_format!(e, bounds_for_range(e, r, Default::default(), w, cx)));
        (whole, last, platform)
    });
    assert!(last.origin.x > whole.origin.x + gpui_kit::px(20.0), "its x is where the last character starts, not the box's left: {:?} in {:?}", last.origin, whole.origin);
    assert!(last.size.width < whole.size.width, "narrower than the box: {:?} in {:?}", last.size, whole.size);
    assert_eq!(via_platform.map(|b| b.origin), Some(last.origin), "the platform layer asks for it");
}

#[derive(Clone, Copy)]
enum Entry {
    Direct,
    F2,
    DoubleClick,
}

/// The candidate window opens at the cell being typed into however the entry began, even for the first character (asked before anything is painted).
fn in_the_cell(entry: Entry) {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("C6");
    let centre = s.with(|e, _| e.cell_center(2, 5)).expect("the cell is on screen");
    match entry {
        Entry::Direct => {}
        Entry::F2 => s.key("f2"),
        Entry::DoubleClick => s.double_click((f32::from(centre.x), f32::from(centre.y))),
    }
    // Asked at once, as the platform does for the first character: no paint has laid the box out yet.
    // (in the same step as the first character: the platform asks while it is still handling that key)
    let rect = s
        .update(|e, w, cx| {
            with_format!(e, replace_and_mark_text_in_range(e, None, "\u{3042}", Some(1..1), w, cx));
            with_format!(e, bounds_for_range(e, 0..1, Default::default(), w, cx))
        })
        .expect("a rectangle");
    let near = |r: gpui_kit::Bounds<gpui_kit::Pixels>| (f32::from(r.origin.x) - f32::from(centre.x)).abs() < 150.0 && (f32::from(r.origin.y) - f32::from(centre.y)).abs() < 40.0;
    assert!(near(rect), "at the cell {:?}, not the editor's corner: {:?}", centre, rect);
    s.settle();
    s.ime_compose("\u{3042}\u{3044}\u{3046}");
    s.settle();
    let rect = s.update(|e, w, cx| with_format!(e, bounds_for_range(e, 2..3, Default::default(), w, cx))).expect("a rectangle");
    assert!(near(rect), "after the paint, at the cell {:?}: {:?}", centre, rect);
}

fn in_the_bar() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("C6");
    let bar = s.with(|e, _| e.sheet_state().and_then(|st| st.bar_bounds.get())).expect("the bar's text is painted");
    s.click((f32::from(bar.origin.x) + 8.0, f32::from(bar.origin.y + bar.size.height / 2.0)));
    s.ime_compose("\u{3042}\u{3044}\u{3046}");
    s.settle();
    let rect = s.update(|e, w, cx| with_format!(e, bounds_for_range(e, 2..3, Default::default(), w, cx))).expect("a rectangle");
    assert!(rect.origin.y >= bar.origin.y && rect.origin.y < bar.origin.y + bar.size.height, "inside the bar's height: {:?} in {:?}", rect, bar);
    assert!(rect.origin.x > bar.origin.x + gpui_kit::px(12.0), "under the last character, not the bar's left: {:?} in {:?}", rect, bar);
    assert!(rect.size.width < bar.size.width / 2.0, "a character, not the whole bar: {:?}", rect);
}
