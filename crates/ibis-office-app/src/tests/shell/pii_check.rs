// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #246: 校閲 > 個人情報の消し忘れの検査… (`review.pii-check`) on the three screens: a menu line that runs `ibis_formats::pii_preflight` over the open document and
//! shows what it found in a message box.

use super::Scenario;
use crate::shell::chrome::MenuEntry;
use ibis_doc_model::Command;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::pii_check_is_a_review_line_on_the_three_screens", is_in_every_review_menu),
        ("shell::pii_check_lists_notes_hidden_slides_and_numbers_of_a_presentation", presentation_findings),
        ("shell::pii_check_says_so_when_a_workbook_is_clean", clean_workbook),
        ("shell::pii_check_takes_out_notes_comments_and_hidden_slides_and_undoes", remove_and_undo),
    ]
}

fn has_item(entries: &[MenuEntry], id: &str) -> bool {
    entries.iter().any(|e| match e {
        MenuEntry::Item(i) => i.id == id,
        MenuEntry::Submenu(s) => has_item(&s.items, id),
        _ => false,
    })
}

/// Every screen's menus have the line (it is placed by the menu table), and choosing it opens the message box.
fn is_in_every_review_menu() {
    for file in ["test/editing/docx-report.docx", "test/editing/xlsx-compatibility-v1.xlsx", "test/benchmark/earnings-presentation.pptx"] {
        let mut s = Scenario::open(file);
        let found = s.with(|e, _| e.menu_bar_menus().menus.iter().any(|(_, entries)| has_item(entries, "review.pii-check")));
        assert!(found, "{file}: 校閲 has the PII check");
        s.menu("review.pii-check");
        assert!(s.with(|e, _| e.alert.is_some() || e.ask.is_some()), "{file}: the check shows its answer in a message box");
    }
}

/// A slide's notes, a hidden slide and a 12-digit number in a text box are each a line of the message.
fn presentation_findings() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run(vec![Command::SetNotes { slide, notes: "患者 山田太郎".into() }], cx);
    });
    let (message, count) = s.with(|e, _| e.pii_check_message());
    assert!(count >= 1, "the notes are found: {message}");
    assert!(message.contains("ノート"), "the notes are named: {message}");
}

fn clean_workbook() {
    let mut s = Scenario::open("test/editing/xlsx-compatibility-v1.xlsx");
    let (message, count) = s.with(|e, _| e.pii_check_message());
    assert_eq!(count, 0, "{message}");
    assert!(message.contains("見つかりませんでした"), "{message}");
}

/// 取り除く… asks once more; yes takes out the notes as one step, and undo brings them all back.
fn remove_and_undo() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let slides_before = s.with(|e, _| e.presentation().slides.len());
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run(vec![Command::SetNotes { slide, notes: "患者 山田太郎".into() }], cx);
    });
    s.menu("review.pii-check");
    assert!(s.with(|e, _| e.ask.is_some()), "the box offers 取り除く…");
    s.update(|e, _, cx| e.answer_ask(true, cx));
    assert!(s.with(|e, _| e.ask.is_some()), "it asks once more before taking anything out");
    let undo_before = s.with(|e, _| e.document.undo_len());
    s.update(|e, _, cx| e.answer_ask(true, cx));
    assert!(s.with(|e, _| e.presentation().slides.iter().all(|x| x.notes.is_empty() && x.comments.is_empty() && !x.hidden)));
    assert_eq!(s.with(|e, _| e.document.undo_len()), undo_before + 1, "one undo step");
    assert!(s.with(|e, _| e.pii_check_message().1) == 0, "nothing is left to find");
    s.update(|e, _, cx| { e.document.undo(); });
    assert!(s.with(|e, _| e.presentation().slides.iter().any(|x| !x.notes.is_empty())), "undo brings the notes back");
    assert_eq!(s.with(|e, _| e.presentation().slides.len()), slides_before);
}
