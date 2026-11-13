// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A deleted paragraph mark (`w:pPr/w:rPr/w:del`, GitHub issue #123): where the markup mode hides deleted text the paragraph joins the next
//! one, accepting joins them for good, rejecting keeps the paragraph, and a deletion of a paragraph mark with the changes recorded marks it
//! (and is saved so). `test/editing/docx-deleted-paragraph-marks.docx` has the paragraphs A, x1, x2 (their text and marks deleted), B, an
//! empty one, E, C (its mark deleted, its text kept) and D.

use ibis_doc_model::{Block, FlowDocument, FlowParagraph};

use super::scenarios::DocSteps as _;
use crate::shell::scenario::Scenario;
use crate::ext::*;

const FILE: &str = "test/editing/docx-deleted-paragraph-marks.docx";

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::deleted_marks_join_where_deleted_text_is_hidden", deleted_marks_join_where_deleted_text_is_hidden),
        ("docx::deleted_marks_accept_joins_and_reject_keeps", deleted_marks_accept_joins_and_reject_keeps),
        ("docx::deleted_marks_backspace_with_tracking_marks_the_paragraph", deleted_marks_backspace_with_tracking_marks_the_paragraph),
        ("docx::review_menu_has_reject_all_changes_and_one_undo_brings_them_back", review_menu_has_reject_all_changes_and_one_undo_brings_them_back),
    ]
}

fn paragraphs(doc: &FlowDocument) -> Vec<&FlowParagraph> {
    doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).collect()
}

fn visible(p: &FlowParagraph) -> String {
    p.runs
        .iter()
        .filter(|r| !r.revision.as_ref().is_some_and(|v| v.kind.is_delete()))
        .map(|r| match &r.kind {
            ibis_doc_model::FlowRunKind::Text(t) => t.clone(),
            _ => String::new(),
        })
        .collect()
}

fn texts(s: &mut Scenario) -> Vec<String> {
    let doc = s.document();
    paragraphs(&doc).into_iter().map(visible).collect()
}

/// The paragraphs that have a line on the pages.
fn lined(s: &mut Scenario) -> Vec<u32> {
    s.with(|e, _| {
        let mut ids: Vec<u32> = e.flow_lines().iter().map(|l| l.para).collect();
        ids.dedup();
        ids
    })
}

fn ids(s: &mut Scenario) -> Vec<u32> {
    let doc = s.document();
    paragraphs(&doc).into_iter().map(|p| p.id).collect()
}

/// All markup keeps every paragraph (the text struck out); where the deleted text is not shown the paragraphs whose marks are deleted
/// join the next one: x1, x2 go into B, C into D (Word's simple and no markup).
fn deleted_marks_join_where_deleted_text_is_hidden() {
    let mut s = Scenario::open(FILE);
    let all = ids(&mut s);
    assert_eq!(all.len(), 8);
    if !s.with(|e, _| e.showing_markup()) {
        s.menu("docx.track.show");
    }
    assert_eq!(lined(&mut s), all, "all markup: a line for each paragraph");
    s.menu("docx.track.show");
    assert!(!s.with(|e, _| e.showing_markup()));
    let shown = lined(&mut s);
    // A, B, the empty one, E, D (C's text on D's line).
    assert_eq!(shown, vec![all[0], all[3], all[4], all[5], all[7]], "no markup: the paragraphs with their marks deleted are joined to the next");
}

/// Accepting joins the paragraphs for good (the formatting of the paragraph whose mark stays), rejecting keeps them and the marks.
fn deleted_marks_accept_joins_and_reject_keeps() {
    let mut s = Scenario::open(FILE);
    s.menu("docx.track.reject-all");
    let kept = texts(&mut s);
    assert_eq!(kept, vec!["A", "x1", "x2", "B", "", "E", "C", "D"]);
    assert!(paragraphs(&s.document()).iter().all(|p| p.mark_revision.is_none() && p.runs.iter().all(|r| r.revision.is_none())));
    drop(s);
    let mut s = Scenario::open(FILE);
    s.menu("docx.track.accept-all");
    assert_eq!(texts(&mut s), vec!["A", "B", "", "E", "CD"]);
    assert!(paragraphs(&s.document()).iter().all(|p| p.mark_revision.is_none()));
    s.save_as("deleted-marks-accepted.docx");
    let saved = s.reopen();
    let doc = saved.flow.as_ref().expect("a document");
    let texts: Vec<String> = paragraphs(doc).into_iter().map(visible).collect();
    assert_eq!(texts, vec!["A", "B", "", "E", "CD"], "saved and read back");
}

/// With the changes recorded, Backspace at the start of a paragraph marks the mark of the one before deleted (nothing joins yet), and the
/// file saved has it as `w:pPr/w:rPr/w:del`.
fn deleted_marks_backspace_with_tracking_marks_the_paragraph() {
    let mut s = Scenario::open(FILE);
    if !s.with(|e, _| e.showing_markup()) {
        s.menu("docx.track.show");
    }
    assert!(s.document().track_changes, "the file records changes");
    let all = ids(&mut s);
    // C's start: the mark of E goes.
    s.click_text(ibis_doc_model::FlowPos::new(all[6], 0));
    s.key("backspace");
    let doc = s.document();
    assert!(paragraphs(&doc)[5].mark_revision.as_ref().is_some_and(|r| r.kind.is_delete()), "E's mark is marked deleted");
    assert_eq!(paragraphs(&doc).len(), 8, "nothing joined yet");
    s.save_as("deleted-marks-tracked.docx");
    let saved = s.reopen();
    let doc = saved.flow.as_ref().expect("a document");
    assert!(paragraphs(doc)[5].mark_revision.as_ref().is_some_and(|r| r.kind.is_delete()), "read back");
    // Accepted, the paragraphs join (E, C and D).
    s.menu("docx.track.accept-all");
    assert_eq!(texts(&mut s), vec!["A", "B", "", "ECD"]);
}

/// 校閲 > 変更箇所 > すべての変更を元に戻す (Word's 校閲 > 元に戻す > すべての変更を元に戻す): the line is in the review menu, in the
/// words of Word, and running it rejects every change in one step that one undo brings back.
fn review_menu_has_reject_all_changes_and_one_undo_brings_them_back() {
    use crate::shell::chrome::{MenuEntry, MenuId};
    fn find<'a>(entries: &'a [MenuEntry], id: &str, path: &mut Vec<String>) -> Option<&'a crate::shell::chrome::MenuItem> {
        for entry in entries {
            match entry {
                MenuEntry::Item(i) if i.id == id => return Some(i),
                MenuEntry::Submenu(sub) => {
                    path.push(sub.label.to_string());
                    if let Some(found) = find(&sub.items, id, path) {
                        return Some(found);
                    }
                    path.pop();
                }
                _ => {}
            }
        }
        None
    }
    let mut s = Scenario::open(FILE);
    let (label, path, enabled) = s.with(|e, _| {
        let menus = e.menu_bar_menus();
        let review = menus.menus.iter().find(|(id, _)| *id == MenuId::Review).expect("the review menu");
        let mut path = Vec::new();
        let item = find(&review.1, "docx.track.reject-all", &mut path).expect("the review menu has Reject All Changes");
        (item.label.to_string(), path, !item.disabled)
    });
    assert_eq!(label, "すべての変更を元に戻す"); // i18n-ok (Japanese text under test)
    assert_eq!(path, vec!["変更箇所".to_string()], "it is in the group beside Accept All Changes"); // i18n-ok (Japanese text under test)
    assert!(enabled, "the file has changes to reject");
    let before = s.document();
    s.menu("docx.track.reject-all");
    let rejected = s.document();
    assert_ne!(rejected.body, before.body, "the changes are rejected");
    assert!(paragraphs(&rejected).iter().all(|p| p.mark_revision.is_none() && p.runs.iter().all(|r| r.revision.is_none())), "no change is left");
    s.key("cmd-z");
    assert_eq!(s.document().body, before.body, "one undo brings every change back");
    // The English table has Word's English words.
    crate::shell::i18n::show(crate::shell::i18n::Language::English);
    let english = crate::shell::i18n::text("docx.track.reject-all").to_string();
    crate::shell::i18n::show(crate::shell::i18n::Language::Japanese);
    assert_eq!(english, "Reject All Changes");
}
