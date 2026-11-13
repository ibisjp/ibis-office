// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issues #116 (the marks of paragraph ends, line breaks, page breaks and the tab after a bullet; the caret after a forced break or a
//! page break) and #117 (Enter on an empty bullet: the same for an item of the file and for one just made). The file is the reporter's
//! (test/github-issue/116/NewLine.docx, not kept in the repository). Word's marks: the paragraph end ↵ and the line break ↓ (Japanese
//! Word), a bullet's tab →, a page break a dashed line with 改ページ in the middle.
use crate::shell::i18n::{self, Language};
use crate::shell::scenario::Scenario;
use ibis_doc_model::{Block, FlowPos};
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue116_marks_glyphs_follow_word", marks_glyphs),
        ("docx::issue116_line_break_at_the_end_moves_the_caret", line_break_at_end),
        ("docx::issue116_page_break_is_a_paragraph_of_its_own", page_break),
        ("docx::issue116_tab_after_a_bullet_is_marked", bullet_tab),
        ("docx::issue116_marks_in_vertical_writing", vertical_marks),
        ("docx::issue117_enter_on_an_empty_bullet", empty_bullet),
        ("docx::issue117_enter_on_an_empty_second_level_item_goes_up", empty_second_level),
    ]
}

const FILE: &str = "test/github-issue/116/NewLine.docx";

/// The body's paragraphs: (id, text, in a list, left indent, style).
fn paragraphs(s: &mut Scenario) -> Vec<(u32, String, bool, i64, Option<String>)> {
    s.with(|e, _| {
        e.flow().expect("a document").body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some((p.id, p.text(), p.list.is_some(), p.left, p.style.clone())) } else { None }).collect()
    })
}

fn id_of(s: &mut Scenario, text: &str) -> u32 {
    paragraphs(s).into_iter().find(|p| p.1 == text).unwrap_or_else(|| panic!("no paragraph {text:?}")).0
}

fn caret_at(s: &mut Scenario, pos: FlowPos) {
    s.update(|e, _, cx| e.set_caret(pos, false, cx));
}

fn caret(s: &mut Scenario) -> FlowPos {
    s.with(|e, _| e.doc_state().and_then(|d| d.caret)).expect("a caret")
}

fn marks_on(s: &mut Scenario) {
    if !s.with(|e, _| e.doc_prefs().formatting_marks_open) {
        s.key("cmd-8");
    }
}

/// Item 1: Japanese Word shows ↵ for a paragraph's end and ↓ for a line break, the other languages ¶ and ↵.
fn marks_glyphs() {
    assert_eq!(crate::doc_ui::mark_glyphs(true), ("\u{21b5}", "\u{2193}"));
    assert_eq!(crate::doc_ui::mark_glyphs(false), ("\u{b6}", "\u{21b5}"));
    let mut s = Scenario::open(FILE);
    i18n::show(Language::Japanese);
    marks_on(&mut s);
    s.screenshot("docx-issue116-marks-ja");
    i18n::show(Language::English);
    s.update(|_, _, cx| cx.notify());
    s.screenshot("docx-issue116-marks-en");
    i18n::show(Language::Japanese);
}

/// Items 2 and 5: Shift+Enter at the end of a paragraph puts the paragraph's end on a line of its own below (the break's mark and the
/// paragraph's end no longer overlap) and the caret goes there.
fn line_break_at_end() {
    let mut s = Scenario::open(FILE);
    i18n::show(Language::Japanese);
    marks_on(&mut s);
    let mac = id_of(&mut s, "Mac");
    caret_at(&mut s, FlowPos::new(mac, 3));
    let before = s.with(|e, _| e.flow_lines().into_iter().filter(|l| l.para == mac).count());
    assert_eq!(before, 1);
    s.key("shift-enter");
    s.settle();
    let lines: Vec<_> = s.with(|e, _| e.flow_lines().into_iter().filter(|l| l.para == mac).collect());
    assert_eq!(lines.len(), 2, "the paragraph's end is on a line below the break: {lines:?}");
    assert!(lines[1].y > lines[0].y + 1.0);
    let pos = caret(&mut s);
    assert_eq!(pos, FlowPos::new(mac, 4), "the caret is after the break");
    let (_, _, y, _) = s.with(|e, _| e.caret_box(&e.flow_lines(), pos)).expect("the caret is laid out");
    assert!((y - lines[1].y).abs() < 0.01, "the caret is on the new line: {y} vs {}", lines[1].y);
    s.screenshot("docx-issue116-line-break-at-end");
    // What is typed next is on the new line.
    s.type_text("x");
    s.settle();
    let pos = caret(&mut s);
    assert_eq!(pos, FlowPos::new(mac, 5));
    let (_, x, y, _) = s.with(|e, _| e.caret_box(&e.flow_lines(), pos)).expect("the caret is laid out");
    let lines: Vec<_> = s.with(|e, _| e.flow_lines().into_iter().filter(|l| l.para == mac).collect());
    assert!((y - lines[1].y).abs() < 0.01 && x > lines[1].x, "x is typed on the second line");
    // Undo takes both back.
    s.key("cmd-z");
    s.key("cmd-z");
    assert_eq!(paragraphs(&mut s).iter().find(|p| p.0 == mac).unwrap().1, "Mac");
}

/// Items 3 and 5: Ctrl+Enter makes Word's three paragraphs (the text before, the break alone, the text after), and the caret goes to the
/// start of the last, on the next page; the break is drawn as a dashed line with its name.
fn page_break() {
    let mut s = Scenario::open(FILE);
    i18n::show(Language::Japanese);
    marks_on(&mut s);
    let mac = id_of(&mut s, "Mac");
    let before = paragraphs(&mut s).len();
    caret_at(&mut s, FlowPos::new(mac, 3));
    let page_of_mac = s.with(|e, _| e.caret_box(&e.flow_lines(), FlowPos::new(mac, 3))).unwrap().0;
    s.key("cmd-enter");
    s.settle();
    let paras = paragraphs(&mut s);
    assert_eq!(paras.len(), before + 2, "the break is a paragraph of its own and a paragraph follows it: {paras:?}");
    let at = paras.iter().position(|p| p.0 == mac).unwrap();
    assert_eq!(paras[at].1, "Mac");
    assert!(paras[at].2, "the text before keeps its list");
    assert_eq!(paras[at + 1].1, "\u{c}");
    assert!(!paras[at + 1].2 && paras[at + 1].4.is_none(), "the break's paragraph is a standard one: {:?}", paras[at + 1]);
    assert!(paras[at + 2].1.is_empty() && paras[at + 2].2, "the paragraph after has the first one's list: {:?}", paras[at + 2]);
    let pos = caret(&mut s);
    assert_eq!(pos, FlowPos::new(paras[at + 2].0, 0));
    let (page, ..) = s.with(|e, _| e.caret_box(&e.flow_lines(), pos)).expect("the caret is laid out");
    assert_eq!(page, page_of_mac + 1, "the caret is on the next page");
    // In the middle of a word it is the same.
    s.key("cmd-z");
    assert_eq!(paragraphs(&mut s).len(), before, "one undo step");
    caret_at(&mut s, FlowPos::new(mac, 1));
    s.key("cmd-enter");
    let paras = paragraphs(&mut s);
    let at = paras.iter().position(|p| p.0 == mac).unwrap();
    assert_eq!((paras[at].1.as_str(), paras[at + 1].1.as_str(), paras[at + 2].1.as_str()), ("M", "\u{c}", "ac"));
    // The page with the dashes, and the next page.
    s.key("cmd-z");
    caret_at(&mut s, FlowPos::new(mac, 3));
    s.key("cmd-enter");
    s.screenshot("docx-issue116-page-break-page1");
    s.page(2);
    s.screenshot("docx-issue116-page-break-page2");
    // The English UI names it.
    i18n::show(Language::English);
    s.update(|_, _, cx| cx.notify());
    s.page(1);
    s.screenshot("docx-issue116-page-break-en");
    i18n::show(Language::Japanese);
}

/// Item 4: the tab after a bullet is marked (→) on a list item's first line only.
fn bullet_tab() {
    let mut s = Scenario::open(FILE);
    i18n::show(Language::Japanese);
    marks_on(&mut s);
    let windows = id_of(&mut s, "Windows\u{b}Client, Server");
    let mac = id_of(&mut s, "Mac");
    let os = id_of(&mut s, "OS");
    let lines = s.with(|e, _| e.flow_lines());
    let of = |id: u32| -> Vec<_> { lines.iter().filter(|l| l.para == id).collect() };
    let tab = of(mac)[0].list_tab.expect("the bullet's tab is marked");
    assert!(tab.1 > tab.0 + 1.0 && tab.0 > 0.0, "{tab:?}");
    assert_eq!(tab.1, of(mac)[0].stops[0].1, "it ends where the text starts");
    assert!(of(os)[0].list_tab.is_none(), "a paragraph that is no list item has none");
    assert!(of(windows)[0].list_tab.is_some() && of(windows)[1].list_tab.is_none(), "only the first line of an item");
    s.screenshot("docx-issue116-bullet-tab");
}

/// Issue 117: Enter on an empty bullet takes the list off, and the item's indent with it (the list's style, List Paragraph, goes back
/// to the standard one, as Word's RemoveNumbers does): the same for an item of the file as for one just made by Enter.
fn empty_bullet() {
    // A new item: Enter at the end of "Mac", then Enter on the empty item.
    let mut s = Scenario::open(FILE);
    let mac = id_of(&mut s, "Mac");
    caret_at(&mut s, FlowPos::new(mac, 3));
    s.key("enter");
    let new_item = caret(&mut s).para;
    assert_ne!(new_item, mac);
    assert!(paragraphs(&mut s).iter().find(|p| p.0 == new_item).unwrap().2, "an empty list item");
    s.key("enter");
    let made = paragraphs(&mut s).into_iter().find(|p| p.0 == new_item).unwrap();
    assert!(!made.2 && made.3 == 0 && made.4.is_none(), "no list, no indent, the standard style: {made:?}");
    // An item of the file: its text taken away, then Enter.
    drop(s);
    let mut s = Scenario::open(FILE);
    let mac = id_of(&mut s, "Mac");
    caret_at(&mut s, FlowPos::new(mac, 3));
    for _ in 0..3 {
        s.key("backspace");
    }
    let item = paragraphs(&mut s).into_iter().find(|p| p.0 == mac).unwrap();
    assert!(item.2 && item.1.is_empty(), "an empty item of the file: {item:?}");
    s.key("enter");
    let opened = paragraphs(&mut s).into_iter().find(|p| p.0 == mac).unwrap();
    assert_eq!((opened.2, opened.3, &opened.4), (made.2, made.3, &made.4), "the same as the one just made");
    // One undo step puts the bullet back.
    s.key("cmd-z");
    let back = paragraphs(&mut s).into_iter().find(|p| p.0 == mac).unwrap();
    assert!(back.2 && back.3 > 0 && back.4.as_deref() == Some("a9"), "{back:?}");
}

/// The user's F15 of #117 (2026-11-12): in Word, Enter at the end of an item, Tab (the new item goes to the second level), then Enter
/// on it while it is empty: the item goes back to the first level and keeps its mark. A second Enter takes the list off.
fn empty_second_level() {
    let level = |s: &mut Scenario, id: u32| -> Option<u32> {
        s.with(|e, _| e.flow().expect("a document").body.iter().find_map(|b| if let Block::Paragraph(p) = b { (p.id == id).then(|| p.list.map(|l| l.level)) } else { None })).flatten()
    };
    let mut s = Scenario::open(FILE);
    let mac = id_of(&mut s, "Mac");
    caret_at(&mut s, FlowPos::new(mac, 3));
    s.key("enter");
    let item = caret(&mut s).para;
    let first = level(&mut s, item).expect("an item");
    s.key("tab");
    assert_eq!(level(&mut s, item), Some(first + 1), "Tab takes it a level down");
    s.key("enter");
    assert_eq!(level(&mut s, item), Some(first), "Enter on the empty item takes it back up, still an item");
    assert_eq!(caret(&mut s).para, item, "no new paragraph");
    s.key("enter");
    assert_eq!(level(&mut s, item), None, "Enter again takes the list off");
}

/// Vertical writing (`tbRl`) has the marks too: the tab after a bullet, a line break, a paragraph's end and a page break, each turned a
/// quarter turn with the line (the line runs down, so a tab's → is drawn ↓).
fn vertical_marks() {
    let mut s = Scenario::open(FILE);
    i18n::show(Language::Japanese);
    marks_on(&mut s);
    s.menu("docx.page.direction.vertical");
    s.settle();
    let windows = id_of(&mut s, "Windows\u{b}Client, Server");
    let mac = id_of(&mut s, "Mac");
    let lines = s.with(|e, _| e.flow_lines());
    let of = |id: u32| -> Vec<_> { lines.iter().filter(|l| l.para == id).collect() };
    assert!(of(mac).iter().all(|l| l.vertical), "the page is vertical");
    let tab = of(mac)[0].list_tab.expect("the bullet's tab is marked in vertical writing too");
    assert!(tab.1 > tab.0 + 1.0 && tab.0 > 0.0, "{tab:?}");
    assert_eq!(tab.1, of(mac)[0].stops[0].1, "it ends where the text starts");
    assert!(of(windows)[0].list_tab.is_some() && of(windows).len() >= 2, "an item with a line break has two lines");
    s.screenshot("docx-issue116-vertical-marks");
    caret_at(&mut s, FlowPos::new(mac, 3));
    s.key("cmd-enter");
    s.settle();
    s.screenshot("docx-issue116-vertical-page-break");
    i18n::show(Language::English);
    s.update(|_, _, cx| cx.notify());
    s.page(1);
    s.screenshot("docx-issue116-vertical-marks-en");
    i18n::show(Language::Japanese);
}
