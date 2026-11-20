// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #179 (a line's height differs from Word's), the document (DOCX) side. The issue's document (`test/github-issue/179/n2-line-height.docx`,
//! downloaded from the issue; not in the repository) has a case a page, marked at its top by an 8 pt label; each test takes the baselines of the kana of a
//! case (the issue's "1 to 3 line feed" is the third line's baseline less the first's) and compares them with the issue's measurements of Word's PDFs.
//! Tolerance 0.25 pt for a position or a feed (Word's PDF is in steps of 0.12 pt), where a test says no more: it says what is left.

use super::flow_layout::{Item, Pagination};
use crate::ext::*;
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    let mut tests: Vec<(&'static str, fn())> = Vec::new();
    tests.extend([
        ("docx::issue179_under_dot_takes_room_below", under_dot_takes_room_below as fn()),
        ("docx::issue179_at_least_line_puts_extra_room_above_the_text", at_least_line_puts_extra_room_above_the_text),
        ("docx::issue179_inline_picture_sits_on_the_baseline_with_the_descent_below", inline_picture_sits_on_the_baseline_with_the_descent_below),
        ("docx::issue179_inline_picture_effect_extent_adds_to_the_line", inline_picture_effect_extent_adds_to_the_line),
        ("docx::issue179_exact_line_keeps_its_baseline_with_a_picture", exact_line_keeps_its_baseline_with_a_picture),
        ("docx::issue179_multiple_line_with_a_picture_adds_the_texts_lines_below", multiple_line_with_a_picture_adds_the_texts_lines_below),
        ("docx::issue54_a_line_of_a_picture_alone_ends_at_the_picture", a_line_of_a_picture_alone_ends_at_the_picture),
    ]);
    tests
}

const FILE: &str = "test/github-issue/179/n2-line-height.docx";

/// Whether the issue's document has been downloaded (`test/github-issue/` is not in the repository): the tests do nothing without it.
fn have_file() -> bool {
    crate::shell::scenario::repo_root().join(FILE).exists()
}

fn pages_of(file: &str) -> std::sync::Arc<Pagination> {
    let mut s = Scenario::open(file);
    s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources))
}

/// The baselines of the case `label`'s lines (the distinct baselines of its kana, in order) and its label's baseline, on the page the label heads.
fn case(pages: &Pagination, label: &str) -> (Vec<f32>, f32) {
    let page = pages
        .pages
        .iter()
        .find(|pg| pg.items.iter().any(|it| matches!(it, Item::Text { text, size, .. } if *size < 9.0 && text.as_ref() == label)))
        .unwrap_or_else(|| panic!("no page of {label}"));
    let mark = page.items.iter().find_map(|it| if let Item::Text { y, text, size, .. } = it { (*size < 9.0 && text.as_ref() == label).then_some(*y) } else { None }).unwrap();
    let mut ys: Vec<f32> = page
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Text { y, text, size, .. } if *size > 9.0 && text.chars().next().is_some_and(|c| ('\u{30a1}'..='\u{30f6}').contains(&c) || ('\u{3041}'..='\u{3093}').contains(&c)) => Some(*y),
            _ => None,
        })
        .collect();
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.05);
    (ys, mark)
}

fn near(a: f32, b: f32, tol: f32, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} against {b}");
}

/// The feed from the first to the third line of a three line case.
fn feed(pages: &Pagination, label: &str) -> f32 {
    let (ys, _) = case(pages, label);
    assert!(ys.len() >= 3, "{label}: {ys:?}");
    ys[ys.len() - 1] - ys[0]
}

/// Difference 2: a dot under the character (`underDot`) takes room below the line as a dot over it does (Word: 30.6 pt against 27.2 pt, in both): the room is a
/// quarter of the line, 0.25 x 13.62 = 3.40 pt (Windows Word's XPS, `tasks/doc/requests/2026-11-14-54-rest-many-patterns-result.md`).
fn under_dot_takes_room_below() {
    if !have_file() {
        return;
    }
    let p = pages_of(FILE);
    near(feed(&p, "H2a"), 30.6, 0.25, "underDot");
    near(feed(&p, "H2b"), 30.6, 0.25, "dot over");
}

/// Difference 3: an `atLeast` line of 36 pt puts the room it adds above the text: the first baseline 34.7 pt under the label's.
fn at_least_line_puts_extra_room_above_the_text() {
    if !have_file() {
        return;
    }
    let p = pages_of(FILE);
    let (ys, mark) = case(&p, "H3a");
    near(ys[0] - mark, 34.7, 0.25, "first baseline of an atLeast 36 pt line");
    near(feed(&p, "H3a"), 72.0, 0.25, "36 pt a line");
}

/// Difference 4: a picture sits on the baseline and the line keeps the text's part below it (14 / 40 / 20 pt: Word 30.6 / 56.6 / 36.6 pt in three lines).
fn inline_picture_sits_on_the_baseline_with_the_descent_below() {
    if !have_file() {
        return;
    }
    let p = pages_of(FILE);
    for (label, want) in [("H4a", 30.6), ("H4b", 56.6), ("H4c", 36.6)] {
        near(feed(&p, label), want, 0.25, label);
    }
}

/// Difference 4: the room of a picture's effects (`wp:effectExtent`, 5 pt above and below) adds to the line (Word 46.6 pt).
fn inline_picture_effect_extent_adds_to_the_line() {
    if !have_file() {
        return;
    }
    let p = pages_of(FILE);
    near(feed(&p, "H4d"), 46.6, 0.25, "H4d");
}

/// Difference 5: in an exact line a picture does not move the baseline from 0.8 of the line (36 pt line: 36.0 / 36.0 pt feeds; 12 pt: 12.0 / 12.0 pt).
fn exact_line_keeps_its_baseline_with_a_picture() {
    if !have_file() {
        return;
    }
    let p = pages_of(FILE);
    let (ys, _) = case(&p, "H5b");
    near(ys[1] - ys[0], 36.0, 0.25, "H5b 1 to 2");
    near(ys[2] - ys[1], 36.0, 0.25, "H5b 2 to 3");
    let (ys, _) = case(&p, "H5c");
    near(ys[1] - ys[0], 12.0, 0.25, "H5c 1 to 2");
    near(ys[2] - ys[1], 12.0, 0.25, "H5c 2 to 3");
}

/// Difference 4: a picture of 40 pt in a line of three lines' spacing: the two lines the multiple adds stay below (Word 111.1 pt; this layout's lines of ＭＳ 明朝
/// are 13.5 pt where Word's are 13.6 pt, `ms_room`, which three lines of this multiple make 0.8 pt).
fn multiple_line_with_a_picture_adds_the_texts_lines_below() {
    if !have_file() {
        return;
    }
    let p = pages_of(FILE);
    near(feed(&p, "H4e"), 111.1, 1.0, "H4e");
}

/// A paragraph that is a 100 pt picture and nothing else (its mark 14 pt ＭＳ ゴシック, no grid; `test/editing/picture-only-line.docx`) is a 100 pt line: the picture sits on
/// the baseline and the line ends there, so the next line's baseline is its own 10.56 pt below the picture's bottom (Windows Word's PDFs of corpus 023: 10.6 pt after a 722.5 pt
/// picture, 10.3 pt after a 710 pt one; the room below the baseline of the paragraph mark's typeface, 5.8 pt here, is only in a line with text in it).
fn a_line_of_a_picture_alone_ends_at_the_picture() {
    let mut s = Scenario::open("test/editing/picture-only-line.docx");
    let (bottom, baseline) = s.with(|e, _| {
        let pages = super::flow_layout::pages(e.flow().expect("a document"), &e.resources);
        let items = &pages.pages[0].items;
        let bottom = items.iter().find_map(|i| if let Item::Picture { y, h, .. } = i { Some(y + h) } else { None }).expect("the picture");
        let baseline = items.iter().find_map(|i| if let Item::Text { y, text, .. } = i && text.contains('あ') { Some(*y) } else { None }).expect("the text");
        (bottom, baseline)
    });
    let below = baseline - bottom;
    assert!((below - 10.56).abs() < 0.8, "the line after the picture has its baseline {below:.2} pt below the picture");
}
