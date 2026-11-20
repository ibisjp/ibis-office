// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #234: Ctrl+Enter (改ページ) as Word does it. (1) The screen follows the caret to the new page. (2) The caret can stand before and
//! after the break's mark, so Backspace and Delete act on what is next to it. (3) A break on an empty paragraph (or at the start of one) takes
//! that paragraph: no empty paragraph is left before it.
use crate::shell::scenario::Scenario;
use ibis_doc_model::{Block, FlowPos};
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue234_a_page_break_on_an_empty_paragraph_takes_it_and_the_screen_follows", empty_paragraph),
        ("docx::issue234_a_page_break_at_the_start_of_text_leaves_no_empty_paragraph_before_it", start_of_text),
        ("docx::issue234_word365_paragraphs_caret_stops_and_keys_of_the_measured_cases", word_cases),
        ("docx::issue234_the_caret_stands_before_and_after_the_break_and_backspace_and_delete_follow_it", caret_and_keys),
    ]
}

/// The body's paragraphs: (id, text).
fn paragraphs(s: &mut Scenario) -> Vec<(u32, String)> {
    s.with(|e, _| e.flow().expect("a document").body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some((p.id, p.text())) } else { None }).collect())
}

fn texts(s: &mut Scenario) -> Vec<String> {
    paragraphs(s).into_iter().map(|p| p.1).collect()
}

fn caret(s: &mut Scenario) -> FlowPos {
    s.with(|e, _| e.doc_state().and_then(|d| d.caret)).expect("a caret")
}

fn new_document() -> Scenario {
    let mut s = Scenario::start_view();
    s.menu("file.new.document");
    s.key("cmd-8");
    s
}

fn caret_at(s: &mut Scenario, pos: FlowPos) {
    s.update(|e, _, cx| e.set_caret(pos, false, cx));
}

/// The caret's (page, x) and its top and bottom in the window.
fn caret_in_window(s: &mut Scenario) -> (usize, f32, f32, f32) {
    let pos = caret(s);
    s.with(|e, _| {
        let (page, x, y, h) = e.caret_box(&e.flow_lines(), pos).expect("the caret is laid out");
        let (origin, k) = e.page_origin(page).expect("the page is placed");
        (page, x, f32::from(origin.y) + y * k, f32::from(origin.y) + (y + h) * k)
    })
}

/// Item 1 and 3: in a new document Ctrl+Enter sets the break on the empty paragraph, a paragraph follows, and the window shows the caret on page 2.
fn empty_paragraph() {
    let mut s = new_document();
    let first = paragraphs(&mut s)[0].0;
    caret_at(&mut s, FlowPos::new(first, 0));
    s.key("cmd-enter");
    s.settle();
    assert_eq!(texts(&mut s), vec!["\u{c}".to_string(), String::new()], "the break is on the empty paragraph, not after an empty one");
    assert_eq!(paragraphs(&mut s)[0].0, first, "it is the same paragraph");
    assert_eq!(caret(&mut s), FlowPos::new(paragraphs(&mut s)[1].0, 0));
    let (page, _, top, bottom) = caret_in_window(&mut s);
    assert_eq!(page, 1, "the caret is on the next page");
    let (view_top, view_h) = s.with(|e, _| { let b = e.workspace_bounds.expect("the workspace"); (f32::from(b.origin.y), f32::from(b.size.height)) });
    assert!(top >= view_top - 0.5 && bottom <= view_top + view_h + 0.5, "the window follows the caret to page 2: caret {top}..{bottom} in a window {view_top}..{}", view_top + view_h);
    s.screenshot("docx-issue234-empty-paragraph");
    // Undo is one step.
    s.key("cmd-z");
    assert_eq!(texts(&mut s), vec![String::new()]);
}

/// Item 3: at the start of a paragraph with text the break takes the paragraph's place and the text follows it.
fn start_of_text() {
    let mut s = new_document();
    let first = paragraphs(&mut s)[0].0;
    caret_at(&mut s, FlowPos::new(first, 0));
    s.type_text("Mac");
    caret_at(&mut s, FlowPos::new(first, 0));
    s.key("cmd-enter");
    s.settle();
    assert_eq!(texts(&mut s), vec!["\u{c}".to_string(), "Mac".to_string()]);
    assert_eq!(caret(&mut s).offset, 0);
    // At the end of the text the text, the break and the rest are three paragraphs (#116).
    s.key("cmd-z");
    caret_at(&mut s, FlowPos::new(first, 3));
    s.key("cmd-enter");
    assert_eq!(texts(&mut s), vec!["Mac".to_string(), "\u{c}".to_string(), String::new()]);
}

/// Item 2: the caret stands after the break (at the end of its dashes, where the paragraph's mark is) and before it; Backspace there takes the
/// paragraph's mark first (the next paragraph joins), then the break; Delete before the break takes the break.
fn caret_and_keys() {
    let mut s = new_document();
    let first = paragraphs(&mut s)[0].0;
    caret_at(&mut s, FlowPos::new(first, 0));
    s.type_text("A");
    s.key("cmd-enter");
    s.settle();
    let paras = paragraphs(&mut s);
    assert_eq!(texts(&mut s), vec!["A".to_string(), "\u{c}".to_string(), String::new()]);
    let (hold, after) = (paras[1].0, paras[2].0);
    let (_, x_before, ..) = { caret_at(&mut s, FlowPos::new(hold, 0)); caret_in_window(&mut s) };
    let (_, x_after, ..) = { caret_at(&mut s, FlowPos::new(hold, 1)); caret_in_window(&mut s) };
    assert!(x_after > x_before + 50.0, "the caret after the break is at the end of its dashes: {x_before} vs {x_after}");
    // Backspace on the new page's first line: the mark of the break's paragraph goes, the caret is after the break.
    caret_at(&mut s, FlowPos::new(after, 0));
    s.key("backspace");
    assert_eq!(texts(&mut s), vec!["A".to_string(), "\u{c}".to_string()]);
    assert_eq!(caret(&mut s), FlowPos::new(hold, 1));
    let (_, x, ..) = caret_in_window(&mut s);
    assert!((x - x_after).abs() < 0.01, "the caret is drawn after the break: {x} vs {x_after}");
    // Delete there does nothing (nothing follows), Backspace takes the break.
    s.key("delete");
    assert_eq!(texts(&mut s), vec!["A".to_string(), "\u{c}".to_string()]);
    s.key("backspace");
    assert_eq!(texts(&mut s), vec!["A".to_string(), String::new()]);
    assert_eq!(caret(&mut s), FlowPos::new(hold, 0));
    // Delete before the break takes it.
    s.key("cmd-z");
    caret_at(&mut s, FlowPos::new(hold, 0));
    s.key("delete");
    assert_eq!(texts(&mut s), vec!["A".to_string(), String::new()]);
    s.screenshot("docx-issue234-caret-after-break");
}

fn t(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn offset_of(s: &mut Scenario) -> (u32, usize) {
    let c = caret(s);
    (c.para, c.offset)
}

/// Word 365's Ctrl+Enter, Backspace, Delete and arrows, as Windows measured them (`tasks/doc/requests/2026-11-17-234-page-break-result.md`, cases A, B, C, E, F;
/// compatibility modes 14 and 15 alike): `\u{c}` is a paragraph of the break alone.
fn word_cases() {
    // A: twice in an empty document, three paragraphs, the caret in the last.
    let mut s = new_document();
    let first = paragraphs(&mut s)[0].0;
    caret_at(&mut s, FlowPos::new(first, 0));
    s.key("cmd-enter");
    s.key("cmd-enter");
    assert_eq!(texts(&mut s), t(&["\u{c}", "\u{c}", ""]));
    let last = paragraphs(&mut s)[2].0;
    assert_eq!(offset_of(&mut s), (last, 0));
    // A: Backspace once joins the last empty paragraph (the caret between the break and its paragraph mark, on page 1 again), twice takes the break.
    s.key("cmd-z");
    s.key("cmd-z");
    s.key("cmd-enter");
    assert_eq!(texts(&mut s), t(&["\u{c}", ""]));
    s.key("backspace");
    assert_eq!(texts(&mut s), t(&["\u{c}"]));
    assert_eq!(offset_of(&mut s), (first, 1));
    assert_eq!(caret_in_window(&mut s).0, 0, "page 1 again");
    s.key("backspace");
    assert_eq!(texts(&mut s), t(&[""]));
    // A: Delete before the break takes it (the paragraphs stay), once more joins the two paragraphs.
    s.key("cmd-z");
    s.key("cmd-z");
    assert_eq!(texts(&mut s), t(&["\u{c}", ""]));
    caret_at(&mut s, FlowPos::new(first, 0));
    s.key("delete");
    assert_eq!(texts(&mut s), t(&["", ""]));
    s.key("delete");
    assert_eq!(texts(&mut s), t(&[""]));
    // C: an empty paragraph between two of text takes the break itself and an empty one follows it: A, [break], (empty), B.
    drop(s);
    let mut s = new_document();
    let first = paragraphs(&mut s)[0].0;
    caret_at(&mut s, FlowPos::new(first, 0));
    s.type_text("A");
    s.key("enter");
    s.key("enter");
    s.type_text("B");
    let empty = paragraphs(&mut s)[1].0;
    caret_at(&mut s, FlowPos::new(empty, 0));
    s.key("cmd-enter");
    assert_eq!(texts(&mut s), t(&["A", "\u{c}", "", "B"]));
    drop(s);
    // B: "Mac" at its end, in the middle, at its start.
    for (offset, want) in [(3, vec!["Mac", "\u{c}", ""]), (1, vec!["M", "\u{c}", "ac"]), (0, vec!["\u{c}", "Mac"])] {
        let mut s = new_document();
        let first = paragraphs(&mut s)[0].0;
        caret_at(&mut s, FlowPos::new(first, 0));
        s.type_text("Mac");
        caret_at(&mut s, FlowPos::new(first, offset));
        s.key("cmd-enter");
        assert_eq!(texts(&mut s), t(&want), "Mac at {offset}");
    }
    // Mac, the break, the empty paragraph: the right arrow stops before the break, after it (before its paragraph mark, the dashes' end) and on the next page.
    let mut s = new_document();
    let first = paragraphs(&mut s)[0].0;
    caret_at(&mut s, FlowPos::new(first, 0));
    s.type_text("Mac");
    s.key("cmd-enter");
    let paras = paragraphs(&mut s);
    let (hold, after) = (paras[1].0, paras[2].0);
    caret_at(&mut s, FlowPos::new(first, 3));
    let mut stops = Vec::new();
    for _ in 0..3 {
        s.key("right");
        stops.push(offset_of(&mut s));
    }
    assert_eq!(stops, vec![(hold, 0), (hold, 1), (after, 0)], "stops: before the break, after it, the next page");
    // Backspace from the empty paragraph on the next page: it goes, then the break, then the paragraph mark (the text is back).
    s.key("backspace");
    assert_eq!((texts(&mut s), offset_of(&mut s)), (t(&["Mac", "\u{c}"]), (hold, 1)));
    s.key("backspace");
    assert_eq!((texts(&mut s), offset_of(&mut s)), (t(&["Mac", ""]), (hold, 0)));
    s.key("backspace");
    assert_eq!((texts(&mut s), offset_of(&mut s)), (t(&["Mac"]), (first, 3)));
    // Delete at the end of "Mac" takes the paragraph mark: the break is then in the paragraph of the text (Mac, the break), then it goes, then the empty paragraph.
    s.key("cmd-z");
    s.key("cmd-z");
    s.key("cmd-z");
    assert_eq!(texts(&mut s), t(&["Mac", "\u{c}", ""]));
    caret_at(&mut s, FlowPos::new(first, 3));
    s.key("delete");
    assert_eq!(texts(&mut s), t(&["Mac\u{c}", ""]));
    assert!(s.with(|e, _| e.flow().unwrap().body.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.runs.iter().any(|r| matches!(r.kind, ibis_doc_model::FlowRunKind::PageBreak))) } else { None }).unwrap()), "the break is a run of the paragraph of Mac");
    // The Insert menu's page break is Ctrl+Enter.
    drop(s);
    let mut s = new_document();
    let first = paragraphs(&mut s)[0].0;
    caret_at(&mut s, FlowPos::new(first, 0));
    s.menu("docx.page-break");
    assert_eq!(texts(&mut s), t(&["\u{c}", ""]));
}
