// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #234 item 4 (reopened 2026-11-25, 0.8.9 on Windows 11): Backspace at the start of the page a page break added leaves no empty page, as
//! in Word (design: `tasks/doc/test-design/issue-234.md`; the numbers B1... are that document's). The 0.8.9 scenarios (`docx/scenario_issue234.rs`) read the
//! paragraphs and the caret; they never look at how many pages the document is laid out on, so they passed while the empty page stayed. These count the
//! pages: the model's (`presentation().slides`, the pages the layout made) and the last page the laid-out lines are on. Word's page counts after each
//! key are measured on Windows (request `tasks/doc/requests/2026-11-25-234-backspace-pages.md`); until then the expected counts are what the
//! reporter says Word does (the page is gone as soon as the caret is behind the break).
use super::Scenario;
use crate::docx::ext::*;
use ibis_doc_model::{Block, FlowPos};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::issue234_b1_empty_document_page_break_then_backspace", b1_empty_document),
        ("shell::issue234_b2_after_text_page_break_then_backspace", b2_after_text),
        ("shell::issue234_b3_two_page_breaks_at_the_end", b3_two_breaks),
        ("shell::issue234_b4_a_paragraph_follows_the_break", b4_text_follows),
        ("shell::issue234_b5_page_break_in_the_middle_of_text", b5_middle_of_text),
        ("shell::issue234_b6_the_arrow_into_the_break_and_the_pages_after_delete", b6_arrows_and_delete),
        ("shell::issue234_b7_a_new_page_follows_the_caret_after_undo", b7_undo),
        ("shell::issue234_b9_the_break_after_a_full_page", b9_full_page),
    ]
}

fn new_document() -> Scenario {
    let mut s = Scenario::start_view();
    s.menu("file.new.document");
    s.key("cmd-8");
    s.size(1200., 900.);
    s
}

fn paragraphs(s: &mut Scenario) -> Vec<(u32, String)> {
    s.with(|e, _| e.flow().expect("a document").body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some((p.id, p.text())) } else { None }).collect())
}

fn texts(s: &mut Scenario) -> Vec<String> {
    paragraphs(s).into_iter().map(|p| p.1).collect()
}

/// (the document's pages as the layout made them, the page the caret is on, counted from 1).
fn pages(s: &mut Scenario) -> (usize, usize) {
    s.with(|e, _| {
        let at = e.doc_state().and_then(|d| d.caret).and_then(|c| e.caret_box(&e.flow_lines(), c)).map_or(0, |b| b.0 + 1);
        (e.presentation().slides.len(), at)
    })
}

fn start(s: &mut Scenario) -> u32 {
    let first = paragraphs(s)[0].0;
    s.update(|e, _, cx| e.set_caret(FlowPos::new(first, 0), false, cx));
    first
}

/// The state after a key: the paragraphs' texts and the pages, in one line.
fn state(s: &mut Scenario) -> String {
    let p = pages(s);
    format!("{:?}, {} page(s), the caret on page {}", texts(s), p.0, p.1)
}

fn expect(s: &mut Scenario, what: &str, paragraphs_now: &[&str], pages_now: usize, failures: &mut Vec<String>) {
    let (model, lines) = pages(s);
    let t = texts(s);
    let ok_text = t.iter().map(String::as_str).collect::<Vec<_>>() == paragraphs_now;
    let _ = lines;
    if !ok_text || model != pages_now {
        failures.push(format!("{what}: expected {paragraphs_now:?} on {pages_now} page(s); got {}", state(s)));
    }
}

fn report(failures: Vec<String>) {
    assert!(failures.is_empty(), "{} step(s) differ:\n{}", failures.len(), failures.join("\n"));
}

/// B1: an empty document, Ctrl+Enter (2 pages), Backspace (Word: the empty paragraph on page 2 goes, one page), Backspace (the break goes, one page).
fn b1_empty_document() {
    let mut s = new_document();
    start(&mut s);
    let mut f = Vec::new();
    s.key("cmd-enter");
    expect(&mut s, "after Ctrl+Enter", &["\u{c}", ""], 2, &mut f);
    s.key("backspace");
    expect(&mut s, "after the first Backspace (the caret is after the break)", &["\u{c}"], 1, &mut f);
    s.key("backspace");
    expect(&mut s, "after the second Backspace (the break is gone)", &[""], 1, &mut f);
    report(f);
}

/// B2: "Mac" and the break after it (the reporter's case: the break added at the end of the text), then Backspace twice and a third time.
fn b2_after_text() {
    let mut s = new_document();
    start(&mut s);
    s.type_text("Mac");
    let mut f = Vec::new();
    s.key("cmd-enter");
    expect(&mut s, "after Ctrl+Enter", &["Mac", "\u{c}", ""], 2, &mut f);
    s.key("backspace");
    expect(&mut s, "Backspace 1: the empty paragraph goes", &["Mac", "\u{c}"], 1, &mut f);
    s.key("backspace");
    expect(&mut s, "Backspace 2: the break goes", &["Mac", ""], 1, &mut f);
    s.key("backspace");
    expect(&mut s, "Backspace 3: the empty paragraph joins Mac", &["Mac"], 1, &mut f);
    report(f);
}

/// B3: two breaks one after the other at the end (three pages), Backspace one at a time.
fn b3_two_breaks() {
    let mut s = new_document();
    start(&mut s);
    s.type_text("A");
    let mut f = Vec::new();
    s.key("cmd-enter");
    s.key("cmd-enter");
    expect(&mut s, "after two Ctrl+Enter", &["A", "\u{c}", "\u{c}", ""], 3, &mut f);
    s.key("backspace");
    expect(&mut s, "Backspace 1", &["A", "\u{c}", "\u{c}"], 2, &mut f);
    s.key("backspace");
    expect(&mut s, "Backspace 2: the second break goes", &["A", "\u{c}", ""], 2, &mut f);
    s.key("backspace");
    expect(&mut s, "Backspace 3: the empty paragraph goes", &["A", "\u{c}"], 1, &mut f);
    report(f);
}

/// B4: a paragraph follows the break ("A", break, "B"): Backspace at the start of "B" joins it to the break's paragraph; "B" stays on page 2.
fn b4_text_follows() {
    let mut s = new_document();
    start(&mut s);
    s.type_text("A");
    s.key("cmd-enter");
    s.type_text("B");
    let mut f = Vec::new();
    expect(&mut s, "A, break, B", &["A", "\u{c}", "B"], 2, &mut f);
    let b = paragraphs(&mut s)[2].0;
    s.update(|e, _, cx| e.set_caret(FlowPos::new(b, 0), false, cx));
    s.key("backspace");
    expect(&mut s, "Backspace at the start of B (B joins the break's paragraph, still on page 2)", &["A", "\u{c}B"], 2, &mut f);
    report(f);
}

/// B5: the break in the middle of "Mac" (M, break, ac): Backspace at the start of "ac" joins "ac" to the break's paragraph, two pages stay.
fn b5_middle_of_text() {
    let mut s = new_document();
    let first = start(&mut s);
    s.type_text("Mac");
    s.update(|e, _, cx| e.set_caret(FlowPos::new(first, 1), false, cx));
    let mut f = Vec::new();
    s.key("cmd-enter");
    expect(&mut s, "M, break, ac", &["M", "\u{c}", "ac"], 2, &mut f);
    s.key("backspace");
    expect(&mut s, "Backspace at the start of ac", &["M", "\u{c}ac"], 2, &mut f);
    report(f);
}

/// B6: the caret goes into the break with the left arrow from the new page, Delete before the break (the break goes, one page), as the 0.8.9 scenario does
/// for the text; the pages are counted.
fn b6_arrows_and_delete() {
    let mut s = new_document();
    start(&mut s);
    s.type_text("A");
    s.key("cmd-enter");
    let mut f = Vec::new();
    s.key("left");
    expect(&mut s, "the left arrow from the new page changes no text", &["A", "\u{c}", ""], 2, &mut f);
    s.key("left");
    s.key("delete");
    expect(&mut s, "Delete before the break takes the break (its paragraph stays: Word `¶ ¶`)", &["A", "", ""], 1, &mut f);
    report(f);
}

/// B7: Ctrl+Enter, Backspace, then undo: the break and the page come back; redo: they go again.
fn b7_undo() {
    let mut s = new_document();
    start(&mut s);
    s.type_text("A");
    s.key("cmd-enter");
    s.key("backspace");
    let mut f = Vec::new();
    expect(&mut s, "after Backspace", &["A", "\u{c}"], 1, &mut f);
    s.key("cmd-z");
    expect(&mut s, "undo", &["A", "\u{c}", ""], 2, &mut f);
    s.key("cmd-shift-z");
    expect(&mut s, "redo", &["A", "\u{c}"], 1, &mut f);
    report(f);
}

pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    Vec::new()
}

/// B9 (Word, measured on Windows, request `2026-11-25-234-backspace-pages.md` W8, the same in compatibility modes 14 and 15): the last line fills the first page
/// (one more line makes the second), Ctrl+Enter after it: the break's paragraph itself falls on page 2 and the empty paragraph after it is page 3 (3 pages);
/// Backspace once: the empty paragraph goes (2 pages); Backspace twice: the break goes and the empty paragraph that is left does not fit page 1 (still 2 pages).
fn b9_full_page() {
    // The number of lines that fill the page: the most that stay on one page here.
    let lines = {
        let mut s = new_document();
        start(&mut s);
        let mut lines = 0;
        loop {
            lines += 1;
            s.type_text(&format!("Line {lines}"));
            if pages(&mut s).0 > 1 {
                break lines;
            }
            s.key("enter");
            assert!(lines < 200, "the lines never make a second page");
        }
    };
    // `lines` lines and the empty paragraph after the last Enter made the second page: the page holds `lines - 1` lines and that paragraph; take it away.
    let full = lines - 1;
    let mut s = new_document();
    start(&mut s);
    for n in 1..=full {
        s.type_text(&format!("Line {n}"));
        if n < full {
            s.key("enter");
        }
    }
    let mut f = Vec::new();
    let (model, _) = pages(&mut s);
    if model != 1 {
        f.push(format!("setup: {full} lines should fill one page, got {model}"));
    }
    s.key("cmd-enter");
    let mut with_break = vec![String::new(); 0];
    with_break.push(format!("after Ctrl+Enter: {}", state(&mut s)));
    let after_break = pages(&mut s).0;
    if after_break != 3 {
        f.push(format!("after Ctrl+Enter: expected 3 page(s); got {}", with_break[0]));
    }
    s.key("backspace");
    let p1 = pages(&mut s).0;
    if p1 != 2 {
        f.push(format!("Backspace 1: expected 2 page(s); got {p1} ({})", texts(&mut s).last().cloned().unwrap_or_default().escape_debug()));
    }
    s.key("backspace");
    let p2 = pages(&mut s).0;
    if p2 != 2 {
        f.push(format!("Backspace 2: expected 2 page(s); got {p2}"));
    }
    report(f);
}
