// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #296 (elric109k-lab): (1) page numbers with the chapter number show the number only (Word: 1-4, not 第1章-4); (2) the navigation
//! pane's heading list; (3) a sample of each heading style where a style is chosen. Test design: `tasks/doc/test-design/296.md`.
//!
//! Registered: T1-T4, the page numbers' chapter numbers (1). T5-T8 ((2) and (3)) are kept below under `cfg(any())` until the user decides; they call
//! accessors (`outline_rows`, `style_sample`, `style_menu_samples`) the implementation adds.
use crate::ext::*;
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue296_t1_word_chapter_page_numbers_of_every_label_form", t1_word_cases),
        ("docx::issue296_t2_chapter_before_the_first_heading_of_its_level_is_the_first_one_after", t2_before_first_heading),
        ("docx::issue296_t3_saving_does_not_change_the_label_or_the_pgnumtype", t3_save_keeps_the_settings),
        ("docx::issue296_t4_changing_the_label_changes_the_page_numbers", t4_label_edit_follows),
    ]
}

/// The footer text of each page: the text items below the page's bottom margin line, as `header011_chapter_in_page_numbers` reads them.
fn footers(s: &mut Scenario) -> Vec<String> {
    s.with(|e, _| {
        let doc = e.flow().unwrap().clone();
        let laid = super::flow_layout::pages(&doc, &e.resources);
        laid.pages
            .iter()
            .map(|pg| {
                pg.items
                    .iter()
                    .filter_map(|i| match i {
                        super::flow_layout::Item::Text { text, y, .. } if *y > pg.height - 100.0 => Some(text.to_string()),
                        _ => None,
                    })
                    .collect::<String>()
            })
            .collect()
    })
}

/// What Word 365 (this PC, scripts/corpus/office_truth_win.ps1, PDF text of each file, 2026-11-26) puts in the footer `PAGE=<PAGE field>` of the
/// four pages of each probe made by scripts/corpus/gen_chapter_label_probes_296.py. Chapter 1 is pages 1-2 and chapter 2 is pages 3-4.
/// The label of the heading is the number part only: 第1章 -> 1, Chapter 1 -> 1, "1." -> 1, 第一章 (japaneseCounting) -> 一, upperRoman -> I.
const WORD: &[(&str, [&str; 4])] = &[
    ("plain", ["PAGE=1-1", "PAGE=1-2", "PAGE=2-3", "PAGE=2-4"]),
    ("dai-sho", ["PAGE=1-1", "PAGE=1-2", "PAGE=2-3", "PAGE=2-4"]),
    ("chapter-word", ["PAGE=1-1", "PAGE=1-2", "PAGE=2-3", "PAGE=2-4"]),
    ("dot", ["PAGE=1-1", "PAGE=1-2", "PAGE=2-3", "PAGE=2-4"]),
    ("kanji", ["PAGE=一-1", "PAGE=一-2", "PAGE=二-3", "PAGE=二-4"]),
    ("roman", ["PAGE=I-1", "PAGE=I-2", "PAGE=II-3", "PAGE=II-4"]),
    ("colon", ["PAGE=1:1", "PAGE=1:2", "PAGE=2:3", "PAGE=2:4"]),
    ("period", ["PAGE=1.1", "PAGE=1.2", "PAGE=2.3", "PAGE=2.4"]),
    // Word draws em dash as the full-width U+FF0D (the document's East Asian font) and en dash as a plain hyphen U+002D.
    ("emdash", ["PAGE=1\u{ff0d}1", "PAGE=1\u{ff0d}2", "PAGE=2\u{ff0d}3", "PAGE=2\u{ff0d}4"]),
    ("endash", ["PAGE=1-1", "PAGE=1-2", "PAGE=2-3", "PAGE=2-4"]),
    // A chapter without a numbering label has no chapter number: the page number alone.
    ("unnumbered", ["PAGE=1", "PAGE=2", "PAGE=3", "PAGE=4"]),
];

/// T1: each probe's footers equal Word's. Fails today for dai-sho, chapter-word, dot, kanji, colon, period and emdash/endash variants that carry words.
fn t1_word_cases() {
    for (name, expected) in WORD {
        let mut s = Scenario::open(&format!("test/editing/docx-chapter-label-{name}.docx"));
        let got = footers(&mut s);
        assert_eq!(got.len(), 4, "{name}: {got:?}");
        for (g, e) in got.iter().zip(expected) {
            assert!(g.ends_with(e), "{name}: ibisOffice {got:?}, Word {expected:?}");
        }
    }
}

/// T2: chapStyle 2 (heading 2 starts the chapter): Word shows 1.1-1 on page 1 although the first heading 2 is on page 2, and 2.1-4 on the last page.
/// Word's PDF: `PAGE=1.1-1 | 1.1-2 | 2.1-3 | 2.1-4`. The label of the level-2 heading is a multilevel `%1.%2` here.
fn t2_before_first_heading() {
    let mut s = Scenario::open("test/editing/docx-chapter-label-level2.docx");
    let got = footers(&mut s);
    let want = ["PAGE=1.1-1", "PAGE=1.1-2", "PAGE=2.1-3", "PAGE=2.1-4"];
    assert_eq!(got.len(), 4, "{got:?}");
    for (g, w) in got.iter().zip(want) {
        assert!(g.ends_with(w), "ibisOffice {got:?}, Word {want:?}");
    }
}

/// T3: opening and saving keep the heading numbering (`w:lvlText` 第%1章 is not rewritten) and `w:pgNumType`.
fn t3_save_keeps_the_settings() {
    let mut s = Scenario::open("test/editing/docx-chapter-label-dai-sho.docx");
    let saved = s.save_as("test-issue296-dai-sho.docx");
    let bytes = std::fs::read(saved).unwrap();
    let doc = ibis_formats::test_files::part_text(&bytes, "word/document.xml").unwrap();
    let numbering = ibis_formats::test_files::part_text(&bytes, "word/numbering.xml").unwrap();
    assert!(doc.contains("w:chapStyle=\"1\"") && doc.contains("w:chapSep=\"hyphen\""));
    assert!(numbering.contains("第%1章"));
    s.reload_saved();
    assert_eq!(s.with(|e, _| e.flow().unwrap().page_chapter.clone()), Some((1, "hyphen".to_string())));
}

/// T4: the page numbers follow the label (Word recalculates the fields): after the heading's first chapter is moved behind another one (cut the
/// second chapter's heading and paste it before the first), the footers' chapter numbers come in the new order: 1-1 1-2 ... as Word numbers them.
/// Done with menu commands only: move heading (outline) down once, as the navigation pane's down button does.
fn t4_label_edit_follows() {
    let mut s = Scenario::open("test/editing/docx-chapter-label-dai-sho.docx");
    let first = s.with(|e, _| e.outline()[0].2);
    s.update(|e, _, cx| e.jump_to(first, cx));
    assert!(s.update(|e, _, cx| e.move_heading(false, cx)), "the heading moved behind the next chapter");
    let got = footers(&mut s);
    // Word 365 (this PC, the same document with the two sections swapped, 2026-11-26): 3 pages, `PAGE=1-1 | 2-2 | 2-3` (第1章 Beta, then 第2章 Alpha).
    assert_eq!(got.len(), 3, "{got:?}");
    for (g, w) in got.iter().zip(["PAGE=1-1", "PAGE=2-2", "PAGE=2-3"]) {
        assert!(g.ends_with(w), "ibisOffice {got:?}, Word 1-1 2-2 2-3");
    }
}

/// T5: the navigation pane's heading rows: (level, label, text). Word's pane shows the numbering label with the text (第1章 Alpha); a heading of
/// level 2 is indented under its level 1. PENDING: needs `outline_rows()` -> Vec<(u32 level, String label, String text, u32 id)`; without a
/// numbering the label is empty.
#[allow(dead_code)]
#[cfg(any())] // (2)/(3) wait for the user's decision; not compiled until their accessors exist
fn t5_pane_rows() {
    let mut s = Scenario::open("test/editing/docx-chapter-label-level2.docx");
    let rows = s.with(|e, _| e.outline_rows());
    let shown: Vec<(u32, String, String)> = rows.into_iter().map(|(l, label, text, _)| (l, label, text)).collect();
    assert_eq!(
        shown,
        vec![
            (1, "第1章".to_string(), "Alpha".to_string()),
            (2, "1.1".to_string(), "Alpha one".to_string()),
            (1, "第2章".to_string(), "Beta".to_string()),
            (2, "2.1".to_string(), "Beta one".to_string()),
        ]
    );
    let mut s = Scenario::open("test/editing/docx-chapter-label-unnumbered.docx");
    assert!(s.with(|e, _| e.outline_rows()).iter().all(|r| r.1.is_empty()));
    // The pane is in the 表示 menu, opens, and its default tab is the headings.
    s.menu("docx.navigation-pane");
    assert!(s.with(|e, _| e.outline_open));
    assert_eq!(s.with(|e, _| e.navigation_tab()), crate::NavigationTab::Headings);
}

/// T6: the list follows the document: a body paragraph made 見出し 1 appears in the list at its place; a heading made 標準 disappears; typing in a
/// heading changes its row's text; undo restores the list.
#[allow(dead_code)]
#[cfg(any())] // (2)/(3) wait for the user's decision; not compiled until their accessors exist
fn t6_pane_follows_edits() {
    let mut s = Scenario::open("test/editing/docx-chapter-label-plain.docx");
    let before = s.with(|e, _| e.outline_rows()).len();
    s.menu("docx.navigation-pane");
    // the second paragraph (body text, "text") becomes a heading 1
    let para = s.with(|e, _| match &e.flow().unwrap().body[1] {
        ibis_doc_model::Block::Paragraph(p) => p.id,
        _ => unreachable!(),
    });
    s.update(|e, _, cx| e.jump_to(para, cx));
    assert!(s.update(|e, _, cx| e.body_to_heading(cx)));
    assert_eq!(s.with(|e, _| e.outline_rows()).len(), before + 1);
    s.menu("edit.undo");
    assert_eq!(s.with(|e, _| e.outline_rows()).len(), before);
}

/// T7: the sample of a style (font, size in pt, bold, italic, colour) that the style menu and the style panel draw each name with. 見出し 1 is bold
/// 16 pt in the probe (sz 32); 見出し 2 is bold 14 pt; 標準 is 11 pt. PENDING: needs `style_sample(&FlowDocument, style_id) -> StyleSample`.
#[allow(dead_code)]
#[cfg(any())] // (2)/(3) wait for the user's decision; not compiled until their accessors exist
fn t7_style_sample() {
    let mut s = Scenario::open("test/editing/docx-chapter-label-level2.docx");
    let doc = s.document();
    let h1 = super::toolbar::style_sample(&doc, "Heading1");
    let h2 = super::toolbar::style_sample(&doc, "Heading2");
    let normal = super::toolbar::style_sample(&doc, "Normal");
    assert!((h1.size_pt - 16.0).abs() < 0.01 && h1.bold, "{h1:?}");
    assert!((h2.size_pt - 14.0).abs() < 0.01 && h2.bold, "{h2:?}");
    assert!((normal.size_pt - 11.0).abs() < 0.01 && !normal.bold, "{normal:?}");
    // the sample is of the style as the document defines it, not a fixed list: a document whose heading 1 is italic shows it italic
    assert!(!h1.italic);
}

/// T8: the style list shows each entry with its sample (the entry carries its sample; the same data is used by the style panel's heading-style
/// choice). PENDING: `MenuItem` entries of the style menu have a `sample` that is `Some(style_sample(..))` for every paragraph style in the list.
#[allow(dead_code)]
#[cfg(any())] // (2)/(3) wait for the user's decision; not compiled until their accessors exist
fn t8_style_menu_samples() {
    let mut s = Scenario::open("test/editing/docx-chapter-label-level2.docx");
    let entries = s.with(|e, _| e.style_menu_samples());
    assert!(entries.iter().any(|(id, _)| id == "Heading1") && entries.iter().any(|(id, _)| id == "Heading2"));
    assert!(entries.iter().all(|(_, sample)| sample.size_pt > 0.0));
}
