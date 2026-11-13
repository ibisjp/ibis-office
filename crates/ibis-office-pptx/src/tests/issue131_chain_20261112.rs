// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-12: linked text boxes (GitHub issue #131), the user's list F: the line of a link joins the corners of the boxes (and follows a
//! box being dragged), composing text flows on into the next box, the arrow keys go over from one box to the next, and Select All takes
//! the text of the whole chain.
use crate::shell::dialog::InteractionLayer;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{EmuRect, Shape, ShapeId};
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue131_chain_the_link_line_joins_the_corners_and_follows_a_dragged_box", link_line_corners_and_drag as fn()),
        ("pptx::issue131_chain_the_link_line_of_vertical_text_joins_the_bottom_left_and_top_right_corners", link_line_vertical),
        ("pptx::issue131_chain_arrow_keys_go_over_from_one_box_to_the_next_and_back", arrows_cross_boxes),
        ("pptx::issue131_chain_select_all_takes_the_text_of_the_whole_chain", select_all_chain),
        ("pptx::issue131_chain_the_box_with_the_caret_has_a_solid_frame_the_others_of_the_chain_dashed", frames_solid_and_dashed),
        ("pptx::issue131_chain_composing_text_flows_on_into_the_next_box_and_typing_goes_on_there", composing_flows_on),
    ]
}

const SENTENCE: &str = "The quick brown fox jumps over the lazy dog while the text flows round the picture in the middle of the box. ";

/// Two boxes side by side (the first low, the second tall), linked, the first one selected.
fn linked(s: &mut Scenario, text: &str, vertical: bool) -> (ShapeId, ShapeId) {
    let (a, b) = s.update(|e, _, cx| {
        let (a, b) = (e.document.ids().new_shape_id(), e.document.ids().new_shape_id());
        let mut first = Shape::text_box(a, EmuRect::new(500_000, 500_000, 3_000_000, 900_000), text);
        let mut second = Shape::text_box(b, EmuRect::new(4_500_000, 1_500_000, 3_000_000, 1_800_000), "");
        for shape in [&mut first, &mut second] {
            if let Some(body) = shape.text_body_mut() {
                body.wrap = true;
                body.vertical = vertical;
            }
        }
        e.insert_shape(first, cx);
        e.insert_shape(second, cx);
        (a, b)
    });
    s.update(|e, _, cx| e.link_text_boxes(a, b, cx));
    assert_eq!(s.with(|e, _| e.interaction_layer()), InteractionLayer::Ask);
    s.update(|e, _, cx| e.answer_ask(true, cx));
    assert!(s.with(|e, _| e.slide().chain_of(a).is_some()), "the boxes are linked");
    (a, b)
}

fn link_line_corners_and_drag() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (a, b) = linked(&mut s, &SENTENCE.repeat(6), false);
    s.update(|e, _, _| e.set_selection(vec![a]));
    let (ra, rb) = s.with(|e, _| (e.slide().shape(a).unwrap().frame, e.slide().shape(b).unwrap().frame));
    let seg = |s: &mut Scenario| {
        s.with(|e, _| {
            let sl = e.slide();
            crate::ibis_text::chain_link_segment(sl.shape(a).unwrap(), sl.shape(b).unwrap(), |sh| e.preview(sh), 1.0)
        })
    };
    // Horizontal text: the bottom right corner of the first box to the top left corner of the second.
    let ((x0, y0), (x1, y1)) = seg(&mut s);
    assert_eq!(((x0 as i64, y0 as i64), (x1 as i64, y1 as i64)), ((ra.x + ra.width, ra.y + ra.height), (rb.x, rb.y)));
    s.screenshot("issue131-link-line-horizontal");
    // Dragged: the line follows the box (the button still down).
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| sh.id == a).unwrap());
    let from = s.shape_position(index);
    let to = (from.0 + 60.0, from.1 + 90.0);
    let layout = s.with(|e, _| e.slide_layout().unwrap());
    s.update(|e, _, _| {
        let start = gpui_kit::point(gpui_kit::px(from.0), gpui_kit::px(from.1));
        let current = gpui_kit::point(gpui_kit::px(to.0), gpui_kit::px(to.1));
        e.set_drag(Some(crate::slide_view::Drag::Move { start, current, origins: vec![(a, ra)] }));
    });
    let moved = seg(&mut s);
    let dx = (60.0 / layout.scale) as i64;
    assert!(((moved.0.0 as i64) - (x0 as i64) - dx).abs() < 2000, "the line's start moved with the box: {moved:?}");
    assert_eq!(moved.1.0 as i64, x1 as i64, "the second box did not move");
    s.screenshot("issue131-link-line-dragging");
    s.update(|e, _, _| e.set_drag(None));
}

fn link_line_vertical() {
    // Vertical text: the bottom left corner of the first box to the top right corner of the second.
    let mut v = Scenario::open("test/editing/pptx-plan.pptx");
    let (a, b) = linked(&mut v, &SENTENCE.repeat(6), true);
    v.update(|e, _, _| e.set_selection(vec![a]));
    let (ra, rb) = v.with(|e, _| (e.slide().shape(a).unwrap().frame, e.slide().shape(b).unwrap().frame));
    let ((x0, y0), (x1, y1)) = v.with(|e, _| {
        let sl = e.slide();
        crate::ibis_text::chain_link_segment(sl.shape(a).unwrap(), sl.shape(b).unwrap(), |sh| e.preview(sh), 1.0)
    });
    assert_eq!(((x0 as i64, y0 as i64), (x1 as i64, y1 as i64)), ((ra.x, ra.y + ra.height), (rb.x + rb.width, rb.y)));
    v.screenshot("issue131-link-line-vertical");
}

/// The text of a box.
fn text_of(s: &mut Scenario, id: ShapeId) -> String {
    s.with(|e, _| e.slide().shape(id).unwrap().text_body().unwrap().text())
}

/// The whole text of the chain `id` is in (the boxes' texts as one).
fn whole(s: &mut Scenario, id: ShapeId) -> String {
    s.with(|e, _| {
        let boxes = e.chain_boxes_of(id).unwrap();
        let chain = e.slide().chain_of(id).cloned();
        e.chain_text(&boxes, chain.as_ref()).iter().map(|p| p.text()).collect::<Vec<_>>().join("\n")
    })
}

/// Box `a` of a chain, edited with the caret at the end of its text.
fn edit_at_end(s: &mut Scenario, a: ShapeId) {
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| sh.id == a).unwrap());
    s.double_click_shape(index);
    s.update(|e, _, cx| {
        let len = e.edited_text().map_or(0, |t| t.chars().count());
        e.shape_move_caret(len, false, cx);
    });
}

const COMPOSING: &str = "\u{3042}\u{3044}\u{3046}\u{3048}\u{304a}\u{304b}\u{304d}\u{304f}\u{3051}\u{3053}"; // i18n-ok: Japanese IME composition fixture.

fn composing_flows_on() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (a, b) = linked(&mut s, &SENTENCE.repeat(6), false);
    s.key("escape");
    let base = whole(&mut s, a);
    edit_at_end(&mut s, a);
    assert_eq!(s.with(|e, _| e.text_editing().map(|t| t.shape)), Some(a));
    // The caret is at the end of the first box: where the text goes on in the second.
    let at = s.with(|e, _| e.chain_span(a).unwrap().ends[0]);
    let with_at = |text: &str, inserted: &str| -> String {
        let head: String = base.chars().take(at).collect();
        let tail: String = base.chars().skip(at).collect();
        format!("{head}{inserted}{tail}{text}")
    };
    let (before_a, before_b) = (text_of(&mut s, a), text_of(&mut s, b));
    let long = COMPOSING.repeat(8);
    s.ime_compose(&long);
    s.screenshot("issue131-composing-flows-on");
    // The box is full: the composing text is not drawn over its lower edge, the next box shows it (and its own text after it).
    let (shown_a, shown_b) = s.with(|e, _| (e.composing_body(a).map(|b| b.text()), e.composing_body(b).map(|b| b.text())));
    let (shown_a, shown_b) = (shown_a.expect("the first box shows its part"), shown_b.expect("the second box shows its part"));
    assert!(shown_b.contains(&long[long.len() - 12..]), "the end of the composing text is in the second box: {shown_b:?}");
    assert!(shown_a.starts_with(&before_a) && shown_a.chars().count() < before_a.chars().count() + long.chars().count() / 4, "the first box keeps its lines and shows only what fits its last line: {shown_a:?}");
    let caret_box = s.with(|e, _| e.editing_display_shape());
    assert_eq!(caret_box, Some(b), "the caret is where the composing text ends");
    // Nothing of it is in the document yet.
    assert_eq!((text_of(&mut s, a), text_of(&mut s, b)), (before_a.clone(), before_b.clone()));
    // Committed, the text is in the chain (typed text goes on in the second box at once) and the caret is in the box it reached.
    s.ime_commit(&long);
    assert_eq!(whole(&mut s, a), with_at("", &long));
    assert_eq!(s.with(|e, _| e.text_editing().map(|t| t.shape)), Some(b), "the caret went on to the second box");
    // Typing on goes on there; a deletion takes the characters back, from the text of the chain.
    s.type_text("xyz");
    assert_eq!(whole(&mut s, a), with_at("", &format!("{long}xyz")));
    for _ in 0..3 {
        s.key("backspace");
    }
    assert_eq!(whole(&mut s, a), with_at("", &long));
}

fn edit_state(s: &mut Scenario) -> (ShapeId, usize) {
    s.with(|e, _| {
        let t = e.text_editing().expect("a text is edited");
        (t.shape, t.caret)
    })
}

fn arrows_cross_boxes() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (a, b) = linked(&mut s, &SENTENCE.repeat(6), false);
    s.key("escape");
    edit_at_end(&mut s, a);
    let len_a = text_of(&mut s, a).chars().count();
    assert_eq!(edit_state(&mut s), (a, len_a));
    // At the end of the first box, → goes to the next character, which is in the second box (the text goes on in the middle of a paragraph).
    s.key("right");
    assert_eq!(edit_state(&mut s), (b, 1));
    s.key("left");
    assert_eq!(edit_state(&mut s), (b, 0));
    s.key("left");
    assert_eq!(edit_state(&mut s), (a, len_a - 1));
    s.key("right");
    s.key("right");
    assert_eq!(edit_state(&mut s), (b, 1));
    // ↑ from the first line of the second box goes to the first box, ↓ on the last line of the first goes to the second.
    s.key("up");
    assert_eq!(edit_state(&mut s).0, a);
    s.key("down");
    assert_eq!(edit_state(&mut s), (b, 0));
    s.screenshot("issue131-arrows-second-box");
}

fn select_all_chain() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (a, b) = linked(&mut s, &SENTENCE.repeat(6), false);
    s.key("escape");
    let base = whole(&mut s, a);
    edit_at_end(&mut s, a);
    s.key("cmd-a");
    s.screenshot("issue131-select-all-chain");
    assert!(s.with(|e, _| e.chain_select_all_active()));
    // Copying takes the text of both boxes.
    s.key("cmd-c");
    let copied = s.with_app(|app| app.read_from_clipboard().and_then(|c| c.text()));
    assert_eq!(copied, Some(base.clone()));
    // Deleting takes it from both, in one step of undo.
    s.key("backspace");
    assert_eq!((text_of(&mut s, a), text_of(&mut s, b)), (String::new(), String::new()));
    s.key("cmd-z");
    assert_eq!(whole(&mut s, a), base);
    // Typing replaces all of it.
    s.key("cmd-a");
    s.type_text("Q");
    assert_eq!(whole(&mut s, a), "Q");
}

/// The box holding the caret has the solid frame, the other boxes of its chain the dashed one; with no caret in the chain, none is dashed.
fn frames_solid_and_dashed() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (a, b) = linked(&mut s, "text", false);
    s.update(|e, _, _| e.set_selection(vec![a]));
    assert!(s.with(|e, _| e.chain_frame_styles().is_empty()), "no caret in the chain: no dashed frame");
    s.update(|e, window, cx| e.begin_edit(a, false, window, cx));
    assert_eq!(s.with(|e, _| e.chain_frame_styles()), vec![(a, true), (b, false)]);
    s.update(|e, window, cx| e.end_text_edit(window, cx));
    s.update(|e, window, cx| e.begin_edit(b, false, window, cx));
    assert_eq!(s.with(|e, _| e.chain_frame_styles()), vec![(a, false), (b, true)]);
}
