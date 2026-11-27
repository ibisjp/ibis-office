// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #188, footnotes of vertical (`tbRl`) writing, checked without a window (so that they run on Windows). The documents are Word's own
//! samples of `test/editing/vertical-footnote-*.docx`, and the values are Word for Windows' (Word's PDFs beside them, and
//! `tasks/doc/requests/2026-11-25-188-vertical-footnote-word-result.md`, which has the measurement).

use std::sync::Arc;

use crate::docx::flow_layout::{Item, Page, Pagination};

/// A sample of `test/editing/` laid out by a headless app as the export does it.
fn laid_out(file: &str) -> Arc<Pagination> {
    // (An absolute path is read as it is: the tool below reads the samples Windows' Word was measured with.)
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(file);
    let path = if std::path::Path::new(file).is_absolute() { std::path::PathBuf::from(file) } else { path };
    let mut presentation = ibis_formats::read(&std::fs::read(path).expect("the file")).expect("a document");
    let (_platform, mut cx) = crate::export::headless_app();
    cx.allow_parking();
    let resources = cx.update(|app| {
        let mut resources = crate::render::shape_view::Resources::new(&presentation, app);
        resources.print = true;
        resources.exact = true;
        resources.preload.wait_all();
        resources
    });
    crate::docx::flow_layout::update_fields_on_open(&mut presentation, &resources);
    crate::docx::flow_layout::pages(presentation.flow.as_ref().expect("a flowing document"), &resources)
}

/// A drawn character: its text, origin, size and turns (0 upright, 1 turned clockwise).
#[derive(Debug, Clone)]
struct Glyph {
    c: char,
    x: f32,
    y: f32,
    size: f32,
    turns: i8,
}

fn glyphs(page: &Page) -> Vec<Glyph> {
    let mut out = Vec::new();
    for item in &page.items {
        match item {
            Item::Text { x, y, text, offsets, size, .. } => out.extend(text.chars().zip(offsets.iter()).map(|(c, dx)| Glyph { c, x: x + dx, y: *y, size: *size, turns: 0 })),
            Item::Turned { x, y, text, size, turns, .. } => out.extend(text.chars().map(|c| Glyph { c, x: *x, y: *y, size: *size, turns: *turns })),
            Item::Scaled { x, y, text, size, .. } => out.extend(text.chars().map(|c| Glyph { c, x: *x, y: *y, size: *size, turns: 0 })),
            _ => {}
        }
    }
    out
}

/// A tool, not a check: prints the page's glyphs left of x = 200 (the notes) and the rectangles (`IBIS_VFN_FILE`, `IBIS_VFN_PAGE`).
#[test]
#[ignore = "a tool: needs IBIS_VFN_FILE"]
fn dump_vertical_footnotes() {
    let Ok(file) = std::env::var("IBIS_VFN_FILE") else { return };
    let page: usize = std::env::var("IBIS_VFN_PAGE").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let pages = laid_out(&file);
    let page = &pages.pages[page];
    for g in glyphs(page) {
        println!("G {} x={:.2} y={:.2} size={:.2} turns={}", g.c, g.x, g.y, g.size, g.turns);
    }
    for item in &page.items {
        if let Item::Rect { x, y, w, h, .. } = item {
            println!("R x={x:.2} y={y:.2} w={w:.2} h={h:.2}");
        }
        if let Item::Line { x, y, h, base, vertical, .. } = item {
            println!("L x={x:.2} y={y:.2} h={h:.2} base={base:.2} vertical={vertical}");
        }
    }
}

fn near(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

/// The rectangles of a page: (x, y, w, h).
fn rects(page: &Page) -> Vec<(f32, f32, f32, f32)> {
    page.items.iter().filter_map(|it| if let Item::Rect { x, y, w, h, .. } = it { Some((*x, *y, *w, *h)) } else { None }).collect()
}

/// The x of the first glyph of each line of the notes (left of `limit`, upright, 10 pt), from the right: the lines' origins.
fn note_columns(gs: &[Glyph], limit: f32) -> Vec<f32> {
    let mut xs: Vec<f32> = Vec::new();
    for g in gs.iter().filter(|g| g.x < limit && g.turns == 0 && (g.size - 10.0).abs() < 0.01 && g.c != ' ') {
        if !xs.iter().any(|x| near(*x, g.x, 0.3)) {
            xs.push(g.x);
        }
    }
    xs.sort_by(|a, b| b.total_cmp(a));
    xs
}

/// Word's PDF of `vertical-footnote-base.docx` (ＭＳ 明朝 10.5 pt text, 10 pt notes, A4, 72 pt margins, `tbRl`): the notes are on the page's left from the left margin,
/// the first note's first line the rightmost, 13.44 pt a line; the number (6.0 pt, turned) is at the line's start, 5.40 pt right of the line's origin; the separator is a
/// vertical rule 144.02 pt long from the top margin, in the column right of the first line (the line's origin + 18.36 pt).
#[test]
fn the_notes_of_a_vertical_page_are_at_its_left_and_the_separator_is_a_vertical_rule() {
    let pages = laid_out("vertical-footnote-base.docx");
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let columns = note_columns(&gs, 200.0);
    // Note 1: one line; note 2: two lines; note 3: two lines.
    assert_eq!(columns.len(), 5, "the lines of the three notes: {columns:?}");
    assert!(near(columns[0], 127.46, 0.3), "note 1's line: {}", columns[0]);
    assert!(columns.windows(2).all(|w| near(w[0] - w[1], 13.44, 0.2)), "13.44 pt a line (the vertical pitch of 10 pt): {columns:?}");
    let number = gs.iter().find(|g| g.c == '1' && g.size < 6.1 && g.x < 200.0).expect("note 1's number");
    assert_eq!(number.turns, 1, "a Latin number is turned");
    assert!(near(number.x - columns[0], 5.40, 0.15) && near(number.y, 72.0, 0.05), "the number: x {} y {}", number.x - columns[0], number.y);
    let rules = rects(page);
    let rule = rules.iter().find(|r| r.3 > 100.0).expect("the separator");
    assert!(near(rule.0, columns[0] + 18.36, 0.4), "the separator is {} right of the first line", rule.0 - columns[0]);
    assert!(near(rule.1, 72.0, 0.05) && near(rule.3, 144.02, 0.1) && near(rule.2, 0.48, 0.05), "the separator: {rule:?}");
}

/// The reference mark in the text (Word's PDF of `vertical-footnote-base.docx`, 10.5 pt text): 6.5 pt (0.6 of the text's size, to 0.5 pt) turned, its baseline 5.52 pt right
/// of the column's origin, and it takes half its size (3.24 pt): the kana after it starts where it ends, with no quarter em between Japanese and a digit.
#[test]
fn the_reference_mark_of_a_vertical_line_is_turned_and_takes_half_its_size() {
    let pages = laid_out("vertical-footnote-base.docx");
    let gs = glyphs(&pages.pages[0]);
    let marks: Vec<&Glyph> = gs.iter().filter(|g| (g.size - 6.5).abs() < 0.01 && g.x > 200.0).collect();
    assert_eq!(marks.len(), 3, "the three references");
    let first = gs.iter().find(|g| g.c == '吾').expect("the first character");
    for mark in marks {
        assert_eq!(mark.turns, 1);
        let before = gs.iter().filter(|g| g.size > 10.0 && near(g.x, mark.x - 5.52, 0.3) && g.y < mark.y).last().expect("the character before");
        let after = gs.iter().filter(|g| g.size > 10.0 && near(g.x, mark.x - 5.52, 0.3) && g.y > mark.y).next().expect("the character after");
        assert!(near(mark.x - before.x, 5.52, 0.1), "the baseline {} right of the column", mark.x - before.x);
        // Where a character's origin is in its cell (吾 starts the page's first column at the margin, 72 pt).
        let ascent = first.y - 72.0;
        assert!(near(after.y - mark.y, ascent + 0.5 * 6.5, 0.15), "the next character is {} below the mark's start", after.y - mark.y);
    }
}

/// Word's PDFs of `vertical-footnote-continue.docx` (a note of 2900 characters): the first page keeps the text to the line with the mark (6 columns) and gives the rest of
/// its width to the notes; the rest go on at the left of the next page (17 lines, 14 columns of text) with the separator again, the same length, from the top margin.
#[test]
fn a_note_longer_than_the_page_goes_on_at_the_next_pages_left_with_its_separator() {
    let pages = laid_out("vertical-footnote-continue.docx");
    assert_eq!(pages.pages.len(), 3, "three pages");
    let body_columns = |page: &Page| -> usize {
        let mut xs: Vec<f32> = Vec::new();
        for g in glyphs(page).iter().filter(|g| g.size > 10.0 && g.turns == 0) {
            if !xs.iter().any(|x| near(*x, g.x, 0.3)) {
                xs.push(g.x);
            }
        }
        xs.len()
    };
    let separator = |page: &Page| *rects(page).iter().find(|r| r.3 > 100.0).expect("a separator");
    assert_eq!(body_columns(&pages.pages[0]), 6);
    assert_eq!(body_columns(&pages.pages[1]), 14);
    let (first, second) = (separator(&pages.pages[0]), separator(&pages.pages[1]));
    assert!(near(first.0, 427.99, 0.4) && near(second.0, 307.13, 0.4), "the separators: {} and {}", first.0, second.0);
    assert!(near(first.3, 144.02, 0.1) && near(second.3, 144.02, 0.1) && near(second.1, 72.0, 0.05), "{first:?} {second:?}");
    assert!(rects(&pages.pages[2]).iter().all(|r| r.3 < 100.0), "no notes left, no separator on the last page");
}

/// Word's PDF of `vertical-footnote-cols2.docx` (two columns): the notes run in the section's two columns, the same lines down both (4 lines each of the 8), so the area is
/// 4 lines thick: the separator is right of the first line as with one column, and the text's last column is at the area's edge (Word: 142.4 pt, past 139.76).
#[test]
fn the_notes_of_a_two_column_vertical_section_run_in_its_two_columns() {
    let pages = laid_out("vertical-footnote-cols2.docx");
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let lines = note_columns(&gs, 140.0);
    assert_eq!(lines.len(), 4, "four lines down each column: {lines:?}");
    let rule = *rects(page).iter().find(|r| r.3 > 100.0).expect("the separator");
    assert!(near(rule.0, lines[0] + 18.36, 0.4), "the separator is {} right of the first line", rule.0 - lines[0]);
    let text_left = gs.iter().filter(|g| g.size > 10.0 && g.turns == 0).map(|g| g.x).fold(f32::MAX, f32::min);
    // The origin of a 10.5 pt character in its column of 14.1 pt is (14.1 - 10.56) / 2 right of the column's edge.
    assert!(near(text_left - 1.77, 142.4, 0.4) && text_left - 1.77 > 139.76, "the text's last column starts at {}", text_left - 1.77);
}

/// Word's PDFs of `vertical-footnote-beneath.docx` (`w:pos` `beneathText` in the section's `w:footnotePr`): the notes do not go to the page's left but follow the text's last column
/// with nothing between: the separator's column is against it (the separator 290.09 pt, the first line's origin 271.73, its number 277.13), the same as the end notes of
/// `vertical-footnote-endnote.docx` (`lowerRoman`, turned).
#[test]
fn notes_beneath_the_text_and_end_notes_follow_the_texts_last_column() {
    for file in ["vertical-footnote-beneath.docx", "vertical-footnote-endnote.docx"] {
        let pages = laid_out(file);
        let page = &pages.pages[0];
        let gs = glyphs(page);
        let lines = note_columns(&gs, 290.0);
        assert!(near(lines[0], 271.73, 0.3), "{file}: the first line {}", lines[0]);
        assert!(lines.windows(2).all(|w| near(w[0] - w[1], 13.44, 0.2)), "{file}: {lines:?}");
        let rule = *rects(page).iter().find(|r| r.3 > 100.0).expect("the separator");
        assert!(near(rule.0, 290.09, 0.4) && near(rule.1, 72.0, 0.05), "{file}: the separator {rule:?}");
        let number = gs.iter().find(|g| g.size < 6.1 && g.x < 290.0 && g.turns == 1).expect("the first note's number");
        assert!(near(number.x, 277.13, 0.2) && near(number.y, 72.0, 0.05), "{file}: the number {} {}", number.x, number.y);
    }
}

/// Word's PDFs of `vertical-footnote-kanji.docx` and `-circ.docx` (`w:numFmt` `japaneseCounting`, `decimalEnclosedCircle`): 一 二 三 and ① ② ③ stand upright, their boxes' left edge
/// (the character's origin) 4.56 pt right of the column's origin in the text (10.5 pt) and in the notes (10 pt) alike; the first note's number's origin is at the top margin
/// plus the ascent of its size (77.16 = 72 + 0.859 × 6.0), and the note's text starts after the number's em and a space (91.58 for its first character's origin).
#[test]
fn kanji_and_circled_numbers_of_notes_stand_upright() {
    for (file, first) in [("vertical-footnote-kanji.docx", '一'), ("vertical-footnote-circ.docx", '①')] {
        let pages = laid_out(file);
        let gs = glyphs(&pages.pages[0]);
        let numbers: Vec<&Glyph> = gs.iter().filter(|g| g.size < 6.1 && g.x < 200.0).collect();
        assert_eq!(numbers.len(), 3, "{file}: three numbers");
        let lines = note_columns(&gs, 200.0);
        let n = numbers.iter().find(|g| g.c == first).expect("the first number");
        assert_eq!(n.turns, 0, "{file}: upright");
        assert!(near(n.x - lines[0], 4.56, 0.2) && near(n.y, 77.16, 0.2), "{file}: the number's origin {} right of the line, y {}", n.x - lines[0], n.y);
        let marks: Vec<&Glyph> = gs.iter().filter(|g| (g.size - 6.5).abs() < 0.01 && g.x > 200.0).collect();
        assert_eq!(marks.len(), 3);
        assert!(marks.iter().all(|m| m.turns == 0), "{file}: the marks in the text stand too");
        let text = gs.iter().find(|g| g.c == '短').expect("the first note's text");
        assert!(near(text.y, 91.58, 0.3), "{file}: the text starts after the number: {}", text.y);
    }
}

/// Word's PDF of `vertical-footnote-grid.docx` (`w:docGrid` `lines`, 18 pt): the notes' lines are on the grid, 18.00 pt apart (their characters (18 - 10) / 2 in), not 13.44;
/// the separator's column is not (14.1 pt): the separator 168.62 pt, the first line's origin 147.98.
#[test]
fn the_notes_of_a_grid_page_are_on_the_grid_and_the_separator_is_not() {
    let pages = laid_out("vertical-footnote-grid.docx");
    let page = &pages.pages[0];
    let lines = note_columns(&glyphs(page), 200.0);
    assert_eq!(lines.len(), 5);
    assert!(near(lines[0], 147.98, 0.3), "the first line {}", lines[0]);
    assert!(lines.windows(2).all(|w| near(w[0] - w[1], 18.0, 0.2)), "18 pt apart: {lines:?}");
    let rule = *rects(page).iter().find(|r| r.3 > 100.0).expect("the separator");
    assert!(near(rule.0, 168.62, 0.4), "the separator {}", rule.0);
}

/// Word's PDFs of `vertical-footnote-kanji-in-settings.docx` and `-beneath-in-settings.docx` (`w:numFmt` and `w:pos` in settings.xml's `w:footnotePr`, none in the section's): Word
/// ignores them there: Arabic numbers (turned), and the notes at the page's left (the separator 145.82 pt).
#[test]
fn the_notes_format_and_place_of_settings_xml_are_not_words() {
    let pages = laid_out("vertical-footnote-kanji-in-settings.docx");
    let gs = glyphs(&pages.pages[0]);
    let number = gs.iter().find(|g| g.size < 6.1 && g.x < 200.0).expect("a number");
    assert!(number.c == '1' && number.turns == 1, "{number:?}");
    let pages = laid_out("vertical-footnote-beneath-in-settings.docx");
    let rule = *rects(&pages.pages[0]).iter().find(|r| r.3 > 100.0).expect("the separator");
    assert!(near(rule.0, 145.82, 0.4), "the separator {}", rule.0);
}

/// #188 (Word's PDF of `vertical-footnote-mixed-size.docx`: ＭＳ 明朝, A4, `tbRl`, a reference mark of 10.5 pt text in a note of 9 pt text; its separator 92.78 pt, the note's first
/// character 79.69 pt centre = 82.92 origin, the mark at the top margin): a line takes the room of the run of the largest size, and a superscript counts at the size of its run, not
/// at the 0.6 it is drawn in. So the note's line is 14.1 pt thick (10.5 pt), not 12.1 (9 pt), and the separator is 2 pt further from the left margin. The mark takes its 3.25 pt
/// and the text starts there: no quarter em between it and a Japanese character (the note's mark, as the mark in the text).
#[test]
fn a_note_of_smaller_text_after_a_mark_of_larger_text_takes_the_marks_room() {
    let pages = laid_out("vertical-footnote-mixed-size.docx");
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let mark = gs.iter().find(|g| g.c == '1' && g.x < 200.0).expect("the note's mark");
    let first = gs.iter().find(|g| g.c == 'さ').expect("the note's first character");
    assert!(near(mark.y, 72.0, 0.05), "the mark is at the top margin: {}", mark.y);
    assert!(near(first.y, 82.92, 0.3), "the note's first character starts after the mark's 3.25 pt: {}", first.y);
    let rule = *rects(page).iter().find(|r| r.3 > 100.0).expect("the separator");
    assert!(near(rule.0, 92.78, 0.25) && near(rule.1, 72.0, 0.05) && near(rule.3, 144.02, 0.1), "the separator: {rule:?}");
}

/// #188 (Word's PDF of `n5-vertical.docx` D6c, the same note: a 9 pt さ in a line with a 10.5 pt mark; the さ's centre 79.04): an upright character stands by the largest size
/// of its line, the centre of the box 0.35 of that size from the baseline, not 0.35 of its own size (which put the 9 pt character 0.61 pt left, 78.43).
#[test]
fn an_upright_character_of_a_line_stands_by_the_largest_size_of_the_line() {
    let pages = laid_out("vertical-footnote-mixed-size.docx");
    let gs = glyphs(&pages.pages[0]);
    let first = gs.iter().find(|g| g.c == 'さ').expect("the note's first character");
    assert!(near(first.x + first.size / 2.0, 79.04, 0.15), "Word's さ is at 79.04: {}", first.x + first.size / 2.0);
}
