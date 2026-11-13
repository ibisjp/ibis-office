// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The find panel's keys on the document, spreadsheet and presentation screens (2026-11-02):
//! Ctrl+F (⌘F on a Mac) opens the panel in the right pane, Enter is Next and Shift+Enter Previous in its field,
//! and on a Mac ⌘G / ⇧⌘G find the next / previous match even with the panel closed.
//! The keys are written with `secondary-`: ⌘ on a Mac, Ctrl elsewhere.
use super::Scenario;
use crate::docx::scenarios::DocSteps;
#[cfg(target_os = "macos")]
use crate::pptx::ext::*;
use crate::xlsx::scenarios::SheetSteps;
use ibis_doc_model::FlowPos;
use crate::docx::ext::*;
use crate::xlsx::ext::*;

/// The cells A4 and A5 of xlsx-sales.xlsx hold this.
const WORD: &str = "町店"; // i18n-ok（シートの中の文字）

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::find_shortcuts_open_next_previous", docx_keys),
        ("xlsx::find_shortcuts_open_next_previous", xlsx_keys),
        ("pptx::find_shortcuts_open_next_previous", pptx_keys),
    ]
}

fn docx_current(s: &mut Scenario) -> Option<usize> {
    s.with(|e, _| e.doc_state().and_then(|d| d.find.as_ref()).and_then(|f| f.current))
}

fn docx_query(s: &mut Scenario) -> Option<String> {
    s.with(|e, cx| e.doc_state().and_then(|d| d.find.as_ref()).map(|f| f.query.read(cx).value().to_string()))
}

/// 文書画面: ⌘F / Ctrl+F opens the panel (with the selected text), Enter and Shift+Enter walk the matches.
fn docx_keys() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1200., 800.);
    // The character the report uses most: it has many matches.
    let doc = s.document();
    let mut counts = std::collections::BTreeMap::<char, usize>::new();
    for p in doc.body.iter().filter_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some(p) } else { None }) {
        for c in p.text().chars().filter(|c| c.is_alphanumeric()) {
            *counts.entry(c).or_default() += 1;
        }
    }
    let common = counts.iter().max_by_key(|(_, n)| **n).map(|(c, _)| *c).unwrap();
    // A selection goes into the field.
    let (id, text) = doc.body.iter().find_map(|b| if let ibis_doc_model::Block::Paragraph(p) = b { Some((p.id, p.text())).filter(|(_, t)| t.chars().count() >= 10 && !t.contains('\u{fffc}')) } else { None }).unwrap();
    s.click_text(FlowPos::new(id, 0));
    s.key("shift-right");
    s.key("shift-right");
    s.key("secondary-f");
    assert_eq!(docx_query(&mut s), Some(text.chars().take(2).collect()), "the selected text is in the find field");
    assert!(s.with(|e, _| e.sidebar_open));
    // Typing replaces it (the field is focused and selected text is replaced when typing over it).
    s.key("secondary-a");
    s.type_text(&common.to_string());
    assert_eq!(docx_query(&mut s), Some(common.to_string()));
    let n = s.with(|e, _| e.doc_state().and_then(|d| d.find.as_ref()).map_or(0, |f| f.matches.len()));
    assert!(n >= 3, "{n} matches of {common}");
    s.key("enter");
    let first = docx_current(&mut s).expect("Enter shows a match");
    s.key("enter");
    let second = docx_current(&mut s).unwrap();
    assert_eq!(second, (first + 1) % n, "Enter is Next");
    s.key("shift-enter");
    assert_eq!(docx_current(&mut s), Some(first), "Shift+Enter is Previous");
    s.key("shift-enter");
    assert_eq!(docx_current(&mut s), Some((first + n - 1) % n));
    let before_f3 = docx_current(&mut s).unwrap();
    s.key("f3");
    assert_eq!(docx_current(&mut s), Some((before_f3 + 1) % n), "F3 is Next");
    s.key("shift-f3");
    assert_eq!(docx_current(&mut s), Some(before_f3), "Shift+F3 is Previous");
    // The field keeps the keys: nothing was typed into the document.
    assert_eq!(s.document().body, doc.body, "Enter did not break a paragraph");
    // Ctrl+F again with the panel open keeps the text.
    s.key("secondary-f");
    assert_eq!(docx_query(&mut s), Some(common.to_string()));
    #[cfg(target_os = "macos")]
    docx_cmd_g(&mut s, common);
}

/// With the panel closed, ⌘G opens it and finds with the text last typed there (Mac only).
#[cfg(target_os = "macos")]
fn docx_cmd_g(s: &mut Scenario, common: char) {
    s.update(|e, _, cx| { if let Some(d) = e.doc_state_mut() { d.find = None; } cx.notify(); });
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("cmd-g");
    assert_eq!(docx_query(s), Some(common.to_string()));
    assert!(docx_current(s).is_some(), "⌘G showed a match");
    let a = docx_current(s);
    s.key("cmd-g");
    assert_ne!(docx_current(s), a);
    s.key("cmd-shift-g");
    assert_eq!(docx_current(s), a);
}

fn xlsx_selected_after(s: &mut Scenario) -> String {
    s.selected()
}

/// 表計算画面.
fn xlsx_keys() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    s.click_cell("A1");
    s.key("secondary-f");
    assert!(s.with(|e, _| e.sheet_state().is_some_and(|st| st.find.is_some())), "the find panel is open");
    assert!(s.with(|e, _| e.sidebar_open));
    s.type_text(WORD); // i18n-ok（シートの中の文字）
    s.key("enter");
    assert_eq!(xlsx_selected_after(&mut s), "A4", "Enter is Next");
    s.key("enter");
    assert_eq!(xlsx_selected_after(&mut s), "A5");
    s.key("shift-enter");
    assert_eq!(xlsx_selected_after(&mut s), "A4", "Shift+Enter is Previous");
    s.key("enter");
    assert_eq!(xlsx_selected_after(&mut s), "A5", "the field still has the keys");
    // F3 is Next as well; Shift+F3 is Previous while the panel is open (Insert Function otherwise).
    let from = xlsx_selected_after(&mut s);
    s.key("enter");
    let by_enter = xlsx_selected_after(&mut s);
    s.key("shift-enter");
    assert_eq!(xlsx_selected_after(&mut s), from);
    s.key("f3");
    assert_eq!(xlsx_selected_after(&mut s), by_enter, "F3 does what Enter does");
    s.key("shift-f3");
    assert_eq!(xlsx_selected_after(&mut s), from, "Shift+F3 is Previous while the find panel is open");
    assert!(s.with(|e, _| e.sheet_state().is_some_and(|st| st.find.is_some())));
    // Ctrl+F with the panel open keeps its text.
    s.key("secondary-f");
    let text = s.with(|e, cx| e.sheet_state().and_then(|st| st.find.as_ref()).map(|f| f.query.read(cx).value().to_string()));
    assert_eq!(text.as_deref(), Some(WORD));
    #[cfg(target_os = "macos")]
    {
        s.update(|e, _, cx| { if let Some(st) = e.sheet_state_mut() { st.find = None; } cx.notify(); });
        s.click_cell("A1");
        s.key("cmd-g");
        assert_eq!(xlsx_selected_after(&mut s), "A4", "⌘G opens the panel and finds with the last text");
        s.key("cmd-g");
        assert_eq!(xlsx_selected_after(&mut s), "A5");
        s.key("cmd-shift-g");
        assert_eq!(xlsx_selected_after(&mut s), "A4");
    }
}

fn pptx_range(s: &mut Scenario) -> Option<std::ops::Range<usize>> {
    s.with(|e, _| e.text_editing().map(|t| t.range()))
}

fn pptx_query(s: &mut Scenario) -> Option<String> {
    s.with(|e, cx| e.shape_edit.find.as_ref().map(|f| f.query.read(cx).value().to_string()))
}

/// プレゼンテーション画面.
fn pptx_keys() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1200., 800.);
    s.page(1);
    s.double_click_shape(1);
    s.key("secondary-a");
    s.type_text("foo bar foo bar foo");
    // The selected text goes into the field.
    for _ in 0..3 { s.key("shift-left"); }
    s.key("secondary-f");
    assert_eq!(pptx_query(&mut s).as_deref(), Some("foo"), "the selected text is in the find field");
    s.key("secondary-a");
    s.type_text("bar");
    assert_eq!(s.with(|e, cx| e.pptx_find_matches(cx).len()), 2);
    s.key("enter");
    assert_eq!(pptx_range(&mut s), Some(4..7), "Enter is Next");
    s.key("enter");
    assert_eq!(pptx_range(&mut s), Some(12..15), "the field still has the keys");
    s.key("shift-enter");
    assert_eq!(pptx_range(&mut s), Some(4..7), "Shift+Enter is Previous");
    s.key("f3");
    assert_eq!(pptx_range(&mut s), Some(12..15), "F3 is Next");
    s.key("shift-f3");
    assert_eq!(pptx_range(&mut s), Some(4..7), "Shift+F3 is Previous");
    assert_eq!(s.with(|e, _| e.presentation().slides[0].shapes[1].text_body().unwrap().text()), "foo bar foo bar foo", "Enter did not break a paragraph");
    #[cfg(target_os = "macos")]
    {
        s.update(|e, w, cx| { if e.pptx().is_some() { e.shape_edit.find = None; } e.focus_handle.focus(w, cx); cx.notify(); });
        s.key("cmd-g");
        assert_eq!(pptx_query(&mut s).as_deref(), Some("bar"), "⌘G opens the panel with the last text");
        assert!(pptx_range(&mut s).is_some());
        let a = pptx_range(&mut s);
        s.key("cmd-g");
        assert_ne!(pptx_range(&mut s), a);
        s.key("cmd-shift-g");
        assert_eq!(pptx_range(&mut s), a);
    }
}
