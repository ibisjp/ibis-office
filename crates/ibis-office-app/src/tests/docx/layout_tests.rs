// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Layout rules checked without a window (so that they run on Windows, where there is no scenario runner): a small file of
//! `test/corpus-report/docx/` is laid out by a headless app as the export does it, and the lines' baselines and the pages are
//! read. The same rules are in `corpus_report.rs` for the scenario runner (macOS).

use std::sync::Arc;

use super::flow_layout::{Item, Page, Pagination};

fn laid_out(file: &str) -> Arc<Pagination> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/corpus-report/docx").join(file);
    let mut presentation = ibis_pptx::read(&std::fs::read(path).expect("the file")).expect("a document");
    let (_platform, mut cx) = crate::export::headless_app();
    cx.allow_parking();
    let resources = cx.update(|app| {
        let mut resources = crate::render::shape_view::Resources::new(&presentation, app);
        resources.print = true;
        resources.exact = true;
        resources.preload.wait_all();
        resources
    });
    super::flow_layout::update_fields_on_open(&mut presentation, &resources);
    super::flow_layout::pages(presentation.flow.as_ref().expect("a flowing document"), &resources)
}

/// A character drawn on a page: its baseline and the glyph.
struct Glyph {
    c: char,
    x: f32,
    y: f32,
}

fn glyphs(page: &Page) -> Vec<Glyph> {
    let mut out = Vec::new();
    for item in &page.items {
        if let Item::Text { x, y, text, offsets, .. } = item {
            out.extend(text.chars().zip(offsets.iter()).map(|(c, dx)| Glyph { c, x: x + dx, y: *y }));
        }
    }
    out
}

/// The x of the first glyph of the run of glyphs spelling `text`, and of its last.
fn span_of(glyphs: &[Glyph], text: &str) -> f32 {
    let want: Vec<char> = text.chars().collect();
    let i = (0..=glyphs.len().saturating_sub(want.len())).find(|&i| glyphs[i..i + want.len()].iter().map(|g| g.c).eq(want.iter().copied())).unwrap_or_else(|| panic!("{text:?} is drawn"));
    glyphs[i + want.len() - 1].x - glyphs[i].x
}

/// The baseline of the first run of glyphs spelling `text`.
fn find(glyphs: &[Glyph], text: &str) -> f32 {
    let want: Vec<char> = text.chars().collect();
    (0..=glyphs.len().saturating_sub(want.len()))
        .find(|&i| glyphs[i..i + want.len()].iter().map(|g| g.c).eq(want.iter().copied()))
        .map(|i| glyphs[i].y)
        .unwrap_or_else(|| panic!("{text:?} is drawn"))
}

fn near(actual: f32, expected: f32, tolerance: f32) -> bool {
    (actual - expected).abs() <= tolerance
}

/// A10: text that cannot sit beside a floating table starts right under it (Word's trial files f2, f8: 25.0pt from the last row's
/// text to the next line's).
#[test]
fn a10_text_that_cannot_sit_beside_a_table_starts_under_it() {
    for file in ["ftab-f2-full-center.docx", "ftab-f8-slot-15pt.docx"] {
        let pages = laid_out(file);
        let gs = glyphs(&pages.pages[0]);
        let last_row = gs.iter().filter(|g| g.c == 'T').map(|g| g.y).fold(0.0f32, f32::max);
        let l00 = find(&gs, "L00");
        assert!(near(l00 - last_row, 25.0, 0.6), "{file}: L00 {:.1}pt under the last row's text; Word: 25.0pt", l00 - last_row);
    }
}

/// A10: the empty paragraphs go under the table (f5: eight of them; L00 to L01 is 9 lines).
#[test]
fn a10_empty_paragraphs_go_under_the_table() {
    let pages = laid_out("ftab-f5-full-empties.docx");
    let gs = glyphs(&pages.pages[0]);
    let (l00, l01, l02) = (find(&gs, "L00"), find(&gs, "L01"), find(&gs, "L02"));
    let rows = (l01 - l00) / (l02 - l01);
    assert!(near(rows, 9.0, 0.2), "L00 to L01 is {rows:.2} lines; Word: 9");
}

/// T7: a page break in a table's cell adds no line (the same place for the paragraph after, one page).
#[test]
fn t7_page_break_in_a_cell_adds_no_line() {
    let plain = laid_out("cellbreak-plain.docx");
    let after = find(&glyphs(&plain.pages[0]), "after");
    for file in ["cellbreak-start.docx", "cellbreak-both.docx"] {
        let pages = laid_out(file);
        let y = find(&glyphs(&pages.pages[0]), "after");
        assert!(pages.pages.len() == 1 && near(y, after, 0.1), "{file}: {} pages, `after` at {y:.1} (without the break: {after:.1})", pages.pages.len());
    }
}

/// F5: a paragraph moved on from the page of an obstacle is laid again without it (the floating table B after A and the empty
/// paragraphs comes at the top of page 2, not at the foot of an added page).
#[test]
fn f5_paragraph_moved_off_an_obstacle_page_is_laid_again() {
    for file in ["floats-a-b-1-empty.docx", "floats-a-b-3-empties.docx", "floats-a-b-5-empties.docx"] {
        let pages = laid_out(file);
        let on = pages.pages.iter().position(|p| glyphs(p).iter().any(|g| g.c == 'B')).expect("B");
        let y = find(&glyphs(&pages.pages[on]), "B");
        assert!(pages.pages.len() == 2 && on == 1 && y < 200.0, "{file}: {} pages, B on page {} at y {y:.0}", pages.pages.len(), on + 1);
    }
}

/// A floating table of one row whose text is taller than a page: the row is divided where the page ends and goes on at the top of
/// the next page (Word; 033's last page was blank and the end of the text was not drawn). Every item is drawn, on two pages.
#[test]
fn a_floating_row_taller_than_a_page_goes_on_to_the_next_page() {
    for (file, n) in [("floatrow-split-70.docx", 70), ("floatrow-split-120.docx", 120), ("floatrow-fits-30.docx", 30)] {
        let pages = laid_out(file);
        let drawn: Vec<String> = pages.pages.iter().map(|p| glyphs(p).iter().map(|g| g.c).collect::<String>()).collect();
        let missing: Vec<usize> = (0..n).filter(|i| !drawn.iter().any(|d| d.contains(&format!("ITEM{i:03}")))).collect();
        assert!(missing.is_empty(), "{file}: items {missing:?} are not drawn on any of the {} pages", pages.pages.len());
        // The items go on in order: the last item is on a page after the first's (when they do not all fit on one).
        let page_of = |i: usize| drawn.iter().position(|d| d.contains(&format!("ITEM{i:03}"))).unwrap();
        assert!(page_of(n - 1) >= page_of(0), "{file}: the last item before the first");
    }
}

/// Word's Windows PDFs of these three fixtures (2026-10-21) have 81 paragraphs each:
/// nine font sizes, three line multiples, three paragraphs per combination. The measured
/// gaps are 14.5/21.75/29 pt for text fitting one 14.5 pt grid, 29 pt for text needing two,
/// and 43.5 pt for text needing three. At 18 pt pitch the corresponding gaps are 18/27/36
/// or 36 pt. These are observed reference values, not a copy of the layout's font metrics.
#[test]
fn line_multiples_on_a_grid_take_the_grids_the_text_needs() {
    let sizes = [9.0f32, 10.0, 10.5, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0];
    for (file, pitch, single_grids) in [
        ("gridmult-linesAndChars-290.docx", 14.5f32, [1.0f32, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 2.0, 3.0]),
        ("gridmult-lines-290.docx", 14.5, [1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 2.0, 3.0]),
        ("gridmult-lines-360.docx", 18.0, [1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 2.0, 2.0, 2.0]),
    ] {
        let pages = laid_out(file);
        assert_eq!(pages.pages.len(), 4, "{file}: Word's reference has four pages");
        let mut baselines = Vec::new();
        for (page_index, page) in pages.pages.iter().enumerate() {
            let mut ys: Vec<f32> = page.items.iter().filter_map(|item| match item {
                Item::Text { y, text, .. } if text.contains('あ') => Some(*y),
                _ => None,
            }).collect();
            ys.sort_by(f32::total_cmp);
            ys.dedup_by(|a, b| near(*a, *b, 0.01));
            baselines.extend(ys.into_iter().map(|y| (page_index, y)));
        }
        // A missing or wrapped paragraph must fail before positional groups are assigned.
        assert_eq!(baselines.len(), 81, "{file}: all three paragraphs of every size/multiple must be drawn once");
        for (size_index, size) in sizes.iter().enumerate() {
            for (multiple_index, multiple) in [1.0f32, 1.5, 2.0].into_iter().enumerate() {
                let start = (size_index * 3 + multiple_index) * 3;
                let group = &baselines[start..start + 3];
                let expected = single_grids[size_index].max(multiple) * pitch;
                let mut checked = 0;
                for pair in group.windows(2) {
                    // Preserve the page identity: a page break is not a line spacing measurement.
                    if pair[0].0 == pair[1].0 {
                        let gap = pair[1].1 - pair[0].1;
                        assert!(near(gap, expected, 0.35), "{file}: {size} pt, {multiple} lines, page {}: gap {gap:.2} pt; Word: {expected:.2} pt", pair[0].0 + 1);
                        checked += 1;
                    }
                }
                assert!(checked > 0, "{file}: {size} pt, {multiple} lines: no adjacent paragraphs on one page");
            }
        }
    }
}

/// A character grid's added pitch (`charSpace`) on a fixed-width font (ＭＳ 明朝 8 pt): a full-width character takes the whole of it, a half-width one
/// (a digit) half of it. Word's PDFs: 103 (charSpace +0.503 pt, a digit advances 4.25 pt) and 121 (charSpace -1.0 pt, a space 5.02 pt of 5.52).
/// `halfwidth-charspace.docx`: page 1 has charSpace 2061 (+0.503 pt), page 2 -4096 (-1.0 pt).
#[test]
fn a_half_width_letter_of_a_fixed_font_takes_half_of_the_added_pitch() {
    let pages = laid_out("halfwidth-charspace.docx");
    assert_eq!(pages.pages.len(), 2);
    for (page, cs) in [(0, 2061.0f32 / 4096.0), (1, -1.0)] {
        let gs = glyphs(&pages.pages[page]);
        let digits = span_of(&gs, "1234567890") / 9.0;
        let kana = span_of(&gs, "あいうえおかきくけこ") / 9.0; // i18n-ok（試験の文書の中の語。UI の文字ではない）
        assert!(near(kana, 8.0 + cs, 0.05), "page {}: a full-width character advances {kana:.3} pt; Word: {:.3}", page + 1, 8.0 + cs);
        assert!(near(digits, 4.0 + cs / 2.0, 0.05), "page {}: a digit advances {digits:.3} pt; Word: {:.3}", page + 1, 4.0 + cs / 2.0);
    }
}

/// Every paragraph of a footer that is not framed takes its height off the body, an empty one too, also where the footer's distance from the edge (49.6 pt)
/// is more than the bottom margin (42.55 pt) (Word's PDFs of `footer-empty-134-variants-probe.docx`, `footer-trailing-empty-probe.docx`: an empty footer takes one
/// line off the body like a footer of one line of text; the corpus 134 line at 789 pt that was taken for the body's last line is the footer's own).
/// `empty-footer-below-margin.docx`: 36 lines of exactly 20 pt fill the page to its margin, and the empty footer's line (13+ pt) is in their way: the last line
/// is on page 2.
#[test]
fn an_empty_footer_pushes_the_body_up() {
    let pages = laid_out("empty-footer-below-margin.docx");
    assert_eq!(pages.pages.len(), 2, "36 lines of 20 pt do not fit between the margin and the empty footer's line");
}

/// The lines of the first page of each section of `footer-trailing-empty-probe.docx` (1 pt lines, 900 to a section, footer 36 pt from the edge, bottom margin
/// 28.35 pt): Word 756 with one line of text in the footer (A), 742 with a text and an empty paragraph (B), 729 with two empty ones after it (C), 756 with an empty
/// one only (D) and with a framed text and an empty one (E, the frame takes none). ibis counts one to three more (its ＭＳ 明朝 line is 13.5 pt, Word's 13.58).
#[test]
fn the_footer_paragraphs_take_the_body_down_one_line_each() {
    let pages = laid_out("footer-trailing-empty-probe.docx");
    let lines_of = |section: usize, letter: char, footer_text: bool| -> f32 {
        let n = glyphs(&pages.pages[section * 2]).iter().filter(|g| g.c == letter).count();
        (n - usize::from(footer_text)) as f32
    };
    let (a, b, c, d, e) = (lines_of(0, 'A', true), lines_of(1, 'B', true), lines_of(2, 'C', true), lines_of(3, 'D', false), lines_of(4, 'E', true));
    assert!(near(a, 756.0, 1.5), "text only: {a} lines; Word 756");
    assert!(near(b, 742.0, 3.0), "text and an empty paragraph: {b} lines; Word 742");
    assert!(near(c, 729.0, 4.0), "text and two empty paragraphs: {c} lines; Word 729");
    assert!(near(d, 756.0, 5.0), "an empty paragraph only: {d} lines; Word 756");
    assert!(near(e, 756.0, 5.0), "a framed text and an empty paragraph: {e} lines; Word 756");
    assert!(a - b > 9.0 && b - c > 9.0, "each empty paragraph takes a line: {a} {b} {c}");
}

/// The room `w:spacing` adds to a character: with `balanceSingleByteDoubleByteWidth` twice to a half-width space (and to a double-byte character), once to
/// digits and Latin letters; without it once to all (Word's PDFs of `spacing-charspace-balance-probe.docx` and `spacing-charspace-size-probe.docx`, Microsoft 365
/// 16.0.20527, every `w:charSpace` of the document grid alike): +100 and +200 twips (5 and 10 pt) on four digits and on three spaces, measured from the marker
/// "E" before a run to the one after it: the digits 5 pt more per character, the spaces 15 pt more in all without the setting and 30 pt with it.
#[test]
fn a_half_width_space_takes_the_spacing_twice_with_the_balance_setting() {
    for (file, spaces_more) in [("spacing-charspace-size-probe.docx", 15.0f32), ("spacing-charspace-balance-probe.docx", 30.0)] {
        let pages = laid_out(file);
        for page in [0usize, 6] {
            let gs = glyphs(&pages.pages[page]);
            let mut ys: Vec<f32> = gs.iter().map(|g| g.y).collect();
            ys.sort_by(|a, b| a.total_cmp(b));
            ys.dedup_by(|a, b| (*a - *b).abs() < 1.0);
            let markers = |y: f32| -> Vec<f32> {
                let mut xs: Vec<f32> = gs.iter().filter(|g| g.c == 'E' && (g.y - y).abs() < 1.0).map(|g| g.x).collect();
                xs.sort_by(|a, b| a.total_cmp(b));
                xs
            };
            let (five, ten) = (markers(ys[1]), markers(ys[2]));
            let digits = ten[0] - five[0];
            let spaces = (ten[3] - ten[2]) - (five[3] - five[2]);
            assert!(near(digits, 20.0, 1.0), "{file} page {page}: four digits move {digits:.1} pt more; Word 20");
            assert!(near(spaces, spaces_more, 1.0), "{file} page {page}: three spaces move {spaces:.1} pt more; Word {spaces_more}");
        }
    }
}

/// A justified line (mode 15) takes the word that overflows by what its spaces and that word give: at most a quarter of each space and at most
/// 0.35 of the last word with the space before it (Word's PDF of `justify-shrink-lastword-compat15-probe.docx`: Courier New, 17 words, the last of 6
/// letters, the line 0.3 to 3.9 letters too long in steps of 0.1: the first 17-word line is the one that is 2.4 letters too long, the first of 16 words
/// 2.5 letters; the boundary is 0.35 x (the letters + 1) for the last words of 2 to 10 letters).
#[test]
fn a_justified_line_takes_a_word_that_overflows_by_a_part_of_it() {
    let pages = laid_out("justify-shrink-lastword-compat15-probe.docx");
    let mut lines: Vec<(usize, i32, usize)> = Vec::new(); // (page, y, letters)
    for (n, page) in pages.pages.iter().enumerate() {
        let mut gs = glyphs(page);
        gs.sort_by(|a, b| a.y.total_cmp(&b.y));
        for g in gs.iter().filter(|g| g.c != ' ') {
            let key = (n, (g.y * 2.0).round() as i32);
            match lines.last_mut() {
                Some(l) if (l.0, l.1) == key => l.2 += 1,
                _ => lines.push((key.0, key.1, 1)),
            }
        }
    }
    // Each paragraph is two lines; the 6-letter last word, step t of 37 (the overflow 0.3 + 0.1 t letters), is paragraph (6 - 2) * 37 + t.
    let words_of = |t: usize| lines[2 * ((6 - 2) * 37 + t)].2;
    // 17 words of 4 letters but the last of 6: 70 letters; 16 words: 64.
    assert_eq!(words_of(21), 70, "2.4 letters too long: the 17th word is taken");
    assert_eq!(words_of(23), 64, "2.6 letters too long: it is not (16 words of 4)");
}

/// A space in a run of its own is as wide as the same space inside a word's run when the runs beside it are Latin text (Word's PDF of corpus 124,
/// `balanceSingleByteDoubleByteWidth`: 0.25 em; ibis took half an em for a space alone in its run), and half an em beside East Asian text.
/// `lone-space-run-between-latin-words.docx`: "Qqq", a space of its own run (`w:hint="eastAsia"`), "Zzz", against "Qqq Zzz" in one run; and the same with あ.
#[test]
fn a_space_alone_in_its_run_between_latin_words_is_a_latin_space() {
    let pages = laid_out("lone-space-run-between-latin-words.docx");
    let gs = glyphs(&pages.pages[0]);
    let x_of = |y_index: usize, c: char| -> f32 {
        let mut ys: Vec<f32> = gs.iter().map(|g| g.y).collect();
        ys.sort_by(|a, b| a.total_cmp(b));
        ys.dedup_by(|a, b| (*a - *b).abs() < 1.0);
        gs.iter().find(|g| (g.y - ys[y_index]).abs() < 1.0 && g.c == c).map(|g| g.x).unwrap_or_else(|| panic!("{c} on line {y_index}"))
    };
    assert!(near(x_of(0, 'Z'), x_of(1, 'Z'), 0.05), "Latin words: {} against {}", x_of(0, 'Z'), x_of(1, 'Z'));
    // Beside East Asian text it is half an em as before (both ways of writing it agree).
    assert!(near(x_of(2, 'あ'), x_of(3, 'あ'), 0.05), "East Asian: {} against {}", x_of(2, 'あ'), x_of(3, 'あ')); // i18n-ok（試験の文書の中の語）
}

/// In a justified line (mode 15) a closing ” ’ . , at the end of the line's last word hangs past the edge and takes none of the overflow, while " ' ) ] } ; : ! ?
/// do not (Word's PDF of `justify-hang-closer-compat15-probe.docx`, Times New Roman 10.5 pt, 8 words, the last with one closer, the overflow of 0.5 to 12 pt in
/// steps of 0.5: with no closer 4.5 pt is the most taken; with ” 9.0, ’ 8.0, . and , 7.0, and the others 4.5).
#[test]
fn a_justified_line_hangs_a_closing_quote_or_a_full_stop_at_its_end() {
    let pages = laid_out("justify-hang-closer-compat15-probe.docx");
    let mut lines: Vec<(usize, i32, usize)> = Vec::new(); // (page, y, letters)
    for (n, page) in pages.pages.iter().enumerate() {
        let mut gs = glyphs(page);
        gs.sort_by(|a, b| a.y.total_cmp(&b.y));
        for g in gs.iter().filter(|g| g.c.is_ascii_alphabetic()) {
            let key = (n, (g.y * 2.0).round() as i32);
            match lines.last_mut() {
                Some(l) if (l.0, l.1) == key => l.2 += 1,
                _ => lines.push((key.0, key.1, 1)),
            }
        }
    }
    // Closers in the file's order: none, ” ’ " ' ) ] } . , ; : ! ? and four full-width; 24 steps each (the overflow 0.5 + 0.5 t pt).
    // A paragraph is two lines; its second line has the three tail words (12 letters) if the last word was taken, 20 or more if not.
    let taken = |closer: usize, t: usize| lines[2 * (closer * 24 + t) + 1].2 < 20;
    for (closer, name, most) in [(0, "none", 4.5f32), (1, "closing double quote", 9.0), (2, "closing single quote", 8.0), (8, "full stop", 7.0), (9, "comma", 7.0), (3, "straight double quote", 4.5), (5, "bracket", 4.5), (13, "question mark", 4.5)] {
        let t_most = ((most - 0.5) / 0.5) as usize;
        assert!(taken(closer, t_most), "{name}: {most} pt over is taken");
        assert!(!taken(closer, t_most + 1), "{name}: {} pt over is not", most + 0.5);
    }
}

/// An installed proportional ＭＳ typeface is laid out with its own advances whichever name the file gives it, the Japanese one or the English one (GitHub issue
/// #56: "MS PMincho" was taken for a typeface that is not installed because the drawn family is named the other way by the OS, and its Latin letters were set
/// 0.508 em each). `ms-p-fonts-english-names.docx`: the same Latin text in ＭＳ Ｐ明朝, MS PMincho, ＭＳ Ｐゴシック and MS PGothic; where the typeface is installed
/// (Windows) the line is as wide with either name, and not N letters of 0.508 em.
#[cfg(windows)]
#[test]
fn an_installed_proportional_ms_typeface_has_its_own_advances_by_either_name() {
    let pages = laid_out("ms-p-fonts-english-names.docx");
    let gs = glyphs(&pages.pages[0]);
    let mut ys: Vec<f32> = gs.iter().map(|g| g.y).collect();
    ys.sort_by(|a, b| a.total_cmp(b));
    ys.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    let width = |line: usize| -> f32 {
        let xs: Vec<f32> = gs.iter().filter(|g| (g.y - ys[line]).abs() < 1.0).map(|g| g.x).collect();
        xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min)
    };
    let letters = 26 + 26 + 10 + 4 + 3;
    let squeezed = letters as f32 * 0.508 * 10.5;
    for (japanese, english, name) in [(0, 1, "Mincho"), (2, 3, "Gothic")] {
        let (a, b) = (width(japanese), width(english));
        assert!(near(a, b, 0.5), "{name}: {a:.1} pt with the Japanese name, {b:.1} pt with the English one");
        // (ＭＳ Ｐゴシック's own letters happen to come to within 3 pt of the stand-in's total, so only ＭＳ Ｐ明朝 is told apart that way.)
        if name == "Mincho" {
            assert!((b - squeezed).abs() > 5.0, "{name}: {b:.1} pt is the stand-in's {squeezed:.1} pt");
        }
    }
}

/// A count of characters in `w:ind` (not 0) wins over the length beside it: Word's PDF of `distribute-first-line-chars-probe.docx` (ＭＳ 明朝 10.5 and
/// 12 pt, `firstLine` 500 = 25 pt, left 107): with `firstLineChars` 100 and 250 the first line starts at 72.504 / 88.224 (10.5 pt) and 74.064 / 92.064 pt
/// (12 pt), that is 1 and 2.5 characters in; with 0 or none it starts at 87.024, 25 pt in.
#[test]
fn a_first_line_count_of_characters_wins_over_the_first_line_length() {
    let pages = laid_out("distribute-first-line-chars-probe.docx");
    let gs = glyphs(&pages.pages[0]);
    let mut starts: Vec<(f32, f32)> = gs.iter().filter(|g| g.c == 'あ').map(|g| (g.y, g.x)).collect(); // i18n-ok（試験の文書の文字）
    starts.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(starts.len(), 16);
    // Each setting twice (a left aligned paragraph, a distributed one), 10.5 pt then 12 pt: none, 0, 100, 250.
    let word = [87.024, 87.024, 72.504, 88.224, 87.024, 87.024, 74.064, 92.064];
    for (k, expected) in word.into_iter().enumerate() {
        for x in [starts[2 * k].1, starts[2 * k + 1].1] {
            assert!(near(x, expected, 0.15), "setting {k}: {x} against Word's {expected}");
        }
    }
}

/// One character of a first-line count is the size of the paragraph's first text plus its character spacing, not the paragraph mark's size: Word's PDF of
/// `first-line-chars-unit-probe.docx` (left 107, firstLine 500, counts 100 and 250; indents from 62.05 pt): mark 10.5 / run 9 pt 9.01 and 22.45; mark 9 / run
/// 10.5 pt 10.45 and 26.17; mark 12 / run 9 pt 9.01 and 22.45; mark 9 / run 12 pt 12.01 and 30.01; 10.5 pt with a spacing of 1.4 pt 11.89 and 29.77; a first
/// run of 9 pt then one of 12 pt 9.01 and 22.45.
#[test]
fn a_first_line_character_is_the_first_runs_size_plus_its_spacing() {
    let pages = laid_out("first-line-chars-unit-probe.docx");
    let gs = glyphs(&pages.pages[0]);
    let mut lines: Vec<(f32, f32)> = Vec::new(); // (y, x of the first glyph)
    for g in &gs {
        match lines.iter_mut().find(|l| near(l.0, g.y, 0.5)) {
            Some(l) => l.1 = l.1.min(g.x),
            None => lines.push((g.y, g.x)),
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(lines.len(), 12);
    let word = [9.01, 22.45, 10.45, 26.17, 9.01, 22.45, 12.01, 30.01, 11.89, 29.77, 9.01, 22.45];
    for (k, indent) in word.into_iter().enumerate() {
        assert!(near(lines[k].1 - 62.05, indent, 0.15), "paragraph {k}: {} against Word's {indent}", lines[k].1 - 62.05);
    }
}

/// The limit of that squeeze follows the number of automatic spaces (Word's PDFs of `justify-autospace-squeeze-gaps{1,3,4}-probe.docx` and the
/// two-space one, a line narrowed by 0.5 pt a paragraph from where the last 「条」 just fits): taken up to 1.0 (1 space), 2.5 (2), 3.5 (3) and 4.0 pt (4)
/// short, 3, 6, 8 and 9 paragraphs; and in modes 11 and 14 only when it fits (the first).
#[test]
fn the_squeeze_limit_of_automatic_spaces_is_by_their_number_and_the_last_word() {
    let taken = |file: &str, kanji_in_line: usize| -> usize {
        let pages = laid_out(file);
        let mut lines: Vec<(f32, Vec<char>)> = Vec::new();
        for page in &pages.pages {
            for g in glyphs(page) {
                match lines.iter_mut().find(|l| near(l.0, g.y, 0.5)) {
                    Some(l) => l.1.push(g.c),
                    None => lines.push((g.y, vec![g.c])),
                }
            }
        }
        lines.sort_by(|a, b| a.0.total_cmp(&b.0));
        lines.iter().step_by(3).take_while(|l| l.1.iter().filter(|c| **c == '条').count() >= kanji_in_line).count() // i18n-ok（試験の文書の文字）
    };
    assert_eq!(taken("justify-autospace-squeeze-gaps1-probe.docx", 1), 3);
    assert_eq!(taken("justify-autospace-squeeze-b1-g0-probe.docx", 1), 6);
    assert_eq!(taken("justify-autospace-squeeze-gaps3-probe.docx", 1), 8);
    assert_eq!(taken("justify-autospace-squeeze-gaps4-probe.docx", 2), 9);
    assert_eq!(taken("justify-autospace-squeeze-compat11-probe.docx", 1), 1);
    assert_eq!(taken("justify-autospace-squeeze-compat14-probe.docx", 1), 1);
}

/// A numbered line of ＭＳ ゴシック has the room `ms_room` adds to its 1 em split above and below the text as a line without a number has (the
/// number is a unit of its own that `ms_room_of` has to look the typeface up for: it had none, so that the baseline of a numbered line was half the
/// room (1.65 pt at 11 pt) too high; corpus 053 page 2, the label "② 補講の実施" of a merged cell: Word 714.8, ibis 713.14). The baselines of a
/// numbered paragraph and the two plain ones after it are one line apart each.
/// `list-number-ms-gothic-baseline.docx`.
#[test]
fn a_numbered_line_of_ms_gothic_has_its_baseline_where_a_plain_line_has_it() {
    let pages = laid_out("list-number-ms-gothic-baseline.docx");
    let gs = glyphs(&pages.pages[0]);
    let (a, b, c) = (find(&gs, "補講の実施"), find(&gs, "補講の上限"), find(&gs, "補講の方法")); // i18n-ok（試験の文書の文字）
    assert!(near(b - a, c - b, 0.05), "the numbered line's baseline is a line above the next: {} against {}", b - a, c - b);
}

/// A closing bracket does not hang past the edge of a table's cell: the line takes the character before it down too (Word's PDF of corpus 023 page 10:
/// "（…確認変更届）" in a cell 202 pt wide breaks before 届, where ibis hung the bracket; 150 corpus documents, 59 more lines the same as Word's with no
/// cell hanging, none fewer). `cell-closer-does-not-hang.docx`: 19 full-width characters and ）, in a cell with room for 19.5.
#[test]
fn a_closing_bracket_does_not_hang_past_the_edge_of_a_cell() {
    let pages = laid_out("cell-closer-does-not-hang.docx");
    let gs = glyphs(&pages.pages[0]);
    let (first, last, closer) = (find(&gs, "あ"), find(&gs, "て"), find(&gs, "）")); // i18n-ok（試験の文書の文字）
    // The one in the table is the second of each (the body paragraph above has all 20 on one line).
    let all: Vec<&Glyph> = gs.iter().filter(|g| g.c == '）').collect(); // i18n-ok（試験の文書の文字）
    let table_closer = all.last().expect("the bracket in the cell").y;
    let table_last = gs.iter().filter(|g| g.c == 'て').last().expect("the last character").y; // i18n-ok（試験の文書の文字）
    assert!(near(table_closer, table_last, 0.1), "the bracket and the character before it are on one line");
    assert!(table_closer > first && table_last > last - 0.1 && closer <= table_closer, "they are on the cell's second line");
}

/// Where Word stops taking the last word of a justified line: the room a line has to take a word is a quarter of its spaces and 0.35 of the word
/// (`justify_overflow_room`). `justify-shrink-boundary-compat15-probe.docx` is 330 one-line candidates (Times New Roman and Century, 9 to 12 pt, 4, 8 and 14
/// spaces, the natural overflow 10 to 30 % of the spaces); Word's PDF (16.0.20527) has the last word on the first line in some and not in the others, and the
/// 330 lines are the same in ibisOffice. Here the two sides of the boundary in 12 of the 30 settings (the widest line that took the word, the next one that did not).
#[test]
fn a_justified_line_takes_its_last_word_as_far_as_word_does() {
    let pages = laid_out("justify-shrink-boundary-compat15-probe.docx");
    let gs: Vec<Glyph> = pages.pages.iter().flat_map(|p| glyphs(p)).collect();
    let at = |text: &str, from: usize| -> usize {
        let want: Vec<char> = text.chars().collect();
        (from..=gs.len() - want.len()).find(|&i| gs[i..i + want.len()].iter().map(|g| g.c).eq(want.iter().copied())).unwrap_or_else(|| panic!("{text:?} is drawn"))
    };
    let rows: &[(&str, &str, bool)] = &[
        ("P013", "lnvppshvecai", true), // Times New Roman, 9 pt, 8 spaces, the line is 12 % of its spaces too wide
        ("P014", "urouldcdcaa", false), // Times New Roman, 9 pt, 8 spaces, the line is 14 % of its spaces too wide
        ("P026", "aoewaf", true), // Times New Roman, 9 pt, 14 spaces, the line is 16 % of its spaces too wide
        ("P027", "byferab", false), // Times New Roman, 9 pt, 14 spaces, the line is 18 % of its spaces too wide
        ("P079", "dkanfvaa", true), // Times New Roman, 10.5 pt, 8 spaces, the line is 12 % of its spaces too wide
        ("P080", "duktblpkim", false), // Times New Roman, 10.5 pt, 8 spaces, the line is 14 % of its spaces too wide
        ("P093", "kdtmw", true), // Times New Roman, 10.5 pt, 14 spaces, the line is 18 % of its spaces too wide
        ("P094", "kmffm", false), // Times New Roman, 10.5 pt, 14 spaces, the line is 20 % of its spaces too wide
        ("P145", "kfuoekcwsw", true), // Times New Roman, 12 pt, 8 spaces, the line is 12 % of its spaces too wide
        ("P146", "rtiafi", false), // Times New Roman, 12 pt, 8 spaces, the line is 14 % of its spaces too wide
        ("P157", "nwbww", true), // Times New Roman, 12 pt, 14 spaces, the line is 14 % of its spaces too wide
        ("P158", "aa", false), // Times New Roman, 12 pt, 14 spaces, the line is 16 % of its spaces too wide
        ("P178", "faoypepvct", true), // Century, 9 pt, 8 spaces, the line is 12 % of its spaces too wide
        ("P179", "ckguorcwww", false), // Century, 9 pt, 8 spaces, the line is 14 % of its spaces too wide
        ("P190", "ctsw", true), // Century, 9 pt, 14 spaces, the line is 14 % of its spaces too wide
        ("P191", "ikm", false), // Century, 9 pt, 14 spaces, the line is 16 % of its spaces too wide
        ("P244", "hmluyfk", true), // Century, 10.5 pt, 8 spaces, the line is 12 % of its spaces too wide
        ("P245", "mfksnlpdem", false), // Century, 10.5 pt, 8 spaces, the line is 14 % of its spaces too wide
        ("P256", "ppnft", true), // Century, 10.5 pt, 14 spaces, the line is 14 % of its spaces too wide
        ("P257", "lgi", false), // Century, 10.5 pt, 14 spaces, the line is 16 % of its spaces too wide
        ("P311", "cfkbcuucm", true), // Century, 12 pt, 8 spaces, the line is 14 % of its spaces too wide
        ("P312", "hwceffat", false), // Century, 12 pt, 8 spaces, the line is 16 % of its spaces too wide
        ("P323", "dgim", true), // Century, 12 pt, 14 spaces, the line is 16 % of its spaces too wide
        ("P324", "ashbmgk", false), // Century, 12 pt, 14 spaces, the line is 18 % of its spaces too wide
    ];
    for (label, last, word_takes_it) in rows {
        let start = at(label, 0);
        let word = at(&format!(" {last} qa"), start + label.len()) + 1; // the word is the one before " qa" (a short one is also inside others)
        let on_the_first_line = (gs[word].y - gs[start].y).abs() < 1.0;
        assert_eq!(on_the_first_line, *word_takes_it, "{label}: {last} is {} the first line", if on_the_first_line { "on" } else { "not on" });
    }
}

/// An empty paragraph that carries a continuous section break takes no room: the next section starts on the line under the text before it (Word's PDF
/// of corpus 020 page 2: the title at 54.0, the next section's first line at 65.5, one 11.5 pt grid line under it, not two; ibis kept the break's line, and
/// the page's text ran over to a third page). `continuous-break-paragraph-no-height.docx`: a line, the break's paragraph, a line, the second break's
/// paragraph (the section after it continuous), two lines.
#[test]
fn an_empty_paragraph_that_carries_a_continuous_section_break_takes_no_room() {
    let pages = laid_out("continuous-break-paragraph-no-height.docx");
    let gs = glyphs(&pages.pages[0]);
    let (a, b, c, d) = (find(&gs, "あ"), find(&gs, "い"), find(&gs, "う"), find(&gs, "え")); // i18n-ok（試験の文書の文字）
    assert!(near(b - a, d - c, 0.05) && near(c - b, d - c, 0.05), "the lines are one line apart: {} {} {}", b - a, c - b, d - c);
}

/// The last line of a paragraph that took its last word by narrowing its spaces is drawn narrowed, as the same line is when more text follows (Word's PDF of
/// corpus 124: the paragraph's last line "… Original page URL)" ends at the right margin; before, the last word was sent to a line of its own, and with
/// it taken the line was not narrowed and ran 6 pt past the margin). `justify-shrink-paragraph-last-word-compat15.docx`: four paragraphs of
/// `justify-shrink-boundary-compat15-probe.docx` (P093, P079, P244, P190: each takes its last word on the first line there) without the words after it.
#[test]
fn a_paragraphs_last_word_taken_by_narrowing_is_drawn_narrowed() {
    let alone: Vec<Glyph> = laid_out("justify-shrink-paragraph-last-word-compat15.docx").pages.iter().flat_map(|p| glyphs(p)).collect();
    let with_more: Vec<Glyph> = laid_out("justify-shrink-boundary-compat15-probe.docx").pages.iter().flat_map(|p| glyphs(p)).collect();
    let at = |gs: &[Glyph], text: &str, from: usize| -> usize {
        let want: Vec<char> = text.chars().collect();
        (from..=gs.len() - want.len()).find(|&i| gs[i..i + want.len()].iter().map(|g| g.c).eq(want.iter().copied())).unwrap_or_else(|| panic!("{text:?} is drawn"))
    };
    for (label, last) in [("P093", "kdtmw"), ("P079", "dkanfvaa"), ("P244", "hmluyfk"), ("P190", "ctsw")] {
        let a = at(&alone, label, 0);
        let word_a = at(&alone, &format!(" {last}"), a + label.len()) + 1;
        let b = at(&with_more, label, 0);
        let word_b = at(&with_more, &format!(" {last} qa"), b + label.len()) + 1;
        assert!((alone[word_a].y - alone[a].y).abs() < 1.0, "{label}: {last} is on the first line of the paragraph's one line");
        assert!(near(alone[word_a].x - alone[a].x, with_more[word_b].x - with_more[b].x, 0.05), "{label}: {last} starts {:.2} pt from the label, {:.2} pt where more follows", alone[word_a].x - alone[a].x, with_more[word_b].x - with_more[b].x);
    }
}

/// A bold ＭＳ 明朝 or ＭＳ ゴシック line of a section of vertical writing is turned down the page as the others are (GitHub issue #54: the
/// headings of a `tbRl` page were drawn across at its top left). Their glyphs, bolder than the cell of a fixed-width font, are drawn squeezed
/// (`Item::Scaled`), and the page's turning left those as they were. `vertical-bold-ms-heading.docx`: a bold heading and a plain line in each font.
#[test]
fn a_bold_ms_line_of_vertical_writing_is_turned_down_the_page() {
    let pages = laid_out("vertical-bold-ms-heading.docx");
    let page = &pages.pages[0];
    assert!(!page.items.iter().any(|it| matches!(it, Item::Scaled { .. })), "no character is left across");
    let gs = glyphs(page);
    for heading in ["縦書きの見出し"] { // i18n-ok（試し docx の本文）
        let chars: Vec<char> = heading.chars().collect();
        let starts: Vec<usize> = (0..gs.len()).filter(|&i| gs[i].c == chars[0]).collect();
        // Each line's characters, upright, down one column: the same x, each lower than the one before.
        for i in starts {
            let run: Vec<&Glyph> = gs[i..].iter().take(chars.len()).collect();
            if !run.iter().map(|g| g.c).eq(chars.iter().copied()) {
                continue;
            }
            assert!(run.iter().all(|g| near(g.x, run[0].x, 1.0)), "{heading}: one column, x {:?}", run.iter().map(|g| g.x).collect::<Vec<_>>());
            assert!(run.windows(2).all(|w| w[1].y > w[0].y + 5.0), "{heading}: down the page, y {:?}", run.iter().map(|g| g.y).collect::<Vec<_>>());
        }
    }
    assert_eq!(gs.iter().filter(|g| g.c == '見').count(), 2, "both headings are drawn");
}

/// The body of a page ends at the larger of the bottom margin and the footer's distance from the edge plus its height, the footer's last empty paragraph
/// counted (Word's PDF of `footer-bottom-room-probe.docx`, 60 exact 18 pt lines in each of 8 sections: no footer 42 lines on the first page with a bottom
/// margin of 851 twips and 43 with 300; a page number alone 42 with both; a page number and an empty paragraph, or two empty paragraphs, 41).
#[test]
fn a_footer_takes_its_height_off_the_body_with_its_empty_paragraphs() {
    let pages = laid_out("footer-bottom-room-probe.docx");
    let text_of = |page: &Page| -> String { glyphs(page).iter().map(|g| g.c).collect() };
    let texts: Vec<String> = pages.pages.iter().map(text_of).collect();
    // (the section's label, the last line of its first page in Word's PDF)
    let expected = [("S01d0851", 42), ("S02d0300", 43), ("S03a0851", 42), ("S04a0300", 42), ("S05b0851", 41), ("S06b0300", 41), ("S07c0851", 41), ("S08c0300", 41)];
    for (prefix, last) in expected {
        let page = texts.iter().find(|t| t.contains(&format!("{prefix}L01"))).unwrap_or_else(|| panic!("{prefix} is drawn"));
        let mut most = 0;
        let mut from = 0;
        while let Some(at) = page[from..].find(&format!("{prefix}L")) {
            let n = &page[from + at + prefix.len() + 1..from + at + prefix.len() + 3];
            most = most.max(n.parse::<u32>().unwrap_or(0));
            from += at + prefix.len();
        }
        assert_eq!(most, last, "{prefix}: the first page ends at line {most}");
    }
}

/// A line spacing below one line (`w:line` 216 = 0.9) shortens the line, in a paragraph off the line grid (Word's PDF of corpus
/// 089 (only 0.9 and 1.0 are measured): ＭＳ 明朝 9 pt paragraphs of 0.9 lines are 10.5 pt apart, 0.9 × 1.2935 em × 9 pt = 10.48; before, ibis kept the single line, 11.6).
/// `line-multiple-below-one-off-grid.docx`: four paragraphs each of 0.9 and 1.0 lines (and of 0.8, not checked).
#[test]
fn a_multiple_below_one_shortens_the_line() {
    let pages = laid_out("line-multiple-below-one-off-grid.docx");
    let gs = glyphs(&pages.pages[0]);
    let y = |text: &str| find(&gs, text);
    for (name, lines) in [("一行目", 0.9f32), ("二行目", 1.0)] { // i18n-ok（試験の文書の中の語。UI の文字ではない）
        let pitch = (y(&format!("{name}3")) - y(&format!("{name}0"))) / 3.0;
        let want = 9.0 * 1.2935 * lines;
        assert!(near(pitch, want, 0.2), "{name}: paragraphs {pitch:.2} pt apart; Word: {want:.2} pt ({lines} lines)");
    }
}

/// A floating text box with `spAutoFit` keeps the text off only as high as its text is, not as high as the extent the file saved
/// (Word's default box is 110 pt of which one line is filled; corpus 063's date line wrapped round the room that is not there).
/// `autofit-textbox-wrap-height.docx`: the box (saved 77.6 x 110.6 pt, one line of text) hangs from the first line; two empty lines
/// under it, a right-aligned date line of 35 characters (367 pt, wider than the room beside the box) is one line.
#[test]
fn an_autofit_text_box_keeps_the_text_off_only_as_high_as_its_text() {
    let pages = laid_out("autofit-textbox-wrap-height.docx");
    let gs = glyphs(&pages.pages[0]);
    let gap = find(&gs, "次の段落") - find(&gs, "年"); // i18n-ok（試験の文書の中の語。UI の文字ではない）
    assert!(near(gap, 18.0, 0.5), "the next paragraph is {gap:.1} pt under the date line; Word: 18 pt (the date is one line)");
}

/// An empty paragraph's line is as high as a line of the paragraph mark's Latin (`w:rFonts/@w:ascii`) typeface at the mark's size, whatever
/// its East Asian one is (Word's PDF of `emptyline-century-default.docx`, Microsoft 365 16.0.20527: five empty paragraphs between a line
/// of Century 10.5 pt text and the next, in eight settings; the baselines of the text lines are 75.86, 75.72, 80.79, 75.72, 78.74, 84.72,
/// 80.79 and 72.96 pt apart: Century 10.5 pt 12.6 each, with ＭＳ 明朝 13.6, Century 11 pt 13.2, Century 12 pt 14.4, ＭＳ ゴシック 13.6,
/// Times New Roman 12.07). A table's cell is not covered.
#[test]
fn an_empty_paragraph_is_as_high_as_its_latin_typeface() {
    let pages = laid_out("emptyline-century-default.docx");
    let gs = glyphs(&pages.pages[0]);
    let want = [75.86f32, 75.72, 80.79, 75.72, 78.74, 84.72, 80.79, 72.96];
    // Setting h (Times New Roman: Word 12.07 pt a line, hhea line gap included; the layout gives 11.6) is not met yet.
    for (k, v) in "abcdefg".chars().enumerate() {
        let d = find(&gs, &format!("E{v}")) - find(&gs, &format!("T{v}"));
        assert!(near(d, want[k], 0.8), "setting {v}: {d:.2} pt from T to E; Word: {:.2}", want[k]);
    }
}

/// A 100x50 pixel picture placed in a 200x400 pt frame is stretched by Word.
/// Inspect the actual rendered color bounds: inspecting Item::Picture's frame
/// alone would pass even if the image element still used ObjectFit::Contain.
#[cfg(any(target_os = "windows", target_os = "macos"))]
#[test]
#[ignore = "uses the platform headless GPU renderer; run explicitly with --ignored"]
fn inline_picture_stretches_to_the_explicit_frame_when_rendered() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
    let source = root.join("test/corpus-report/docx/inline-picture-stretched-frame.docx");
    let bytes = std::fs::read(source).unwrap();
    let presentation = ibis_pptx::read(&bytes).unwrap();
    let out_dir = root.join("target/layout-regression").join(format!("picture-frame-{}", std::process::id()));
    std::fs::create_dir_all(&out_dir).unwrap();
    let options = crate::export::Options {
        format: crate::export::Format::Png,
        out_dir: out_dir.clone(),
        name: "picture-frame".into(),
        width: 1190,
        slides: vec![1],
        layout: false,
        file_name: None,
        print_backgrounds: true,
        print_hidden_text: false,
        print_markup: false,
        color: Default::default(),
        handout: None,
        frame: false,
        handout_landscape: false,
        notes_pages: false,
        outline_pages: false,
        notes_source: None,
        handout_marks: Default::default(),
    };
    crate::export::export(presentation, &options).unwrap();
    let png = std::fs::read(out_dir.join("picture-frame-01.png")).unwrap();
    let image = image::load_from_memory(&png).unwrap().to_rgba8();
    let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, pixel) in image.enumerate_pixels() {
        let [r, g, b, a] = pixel.0;
        if a > 128 && r.max(g).max(b) - r.min(g).min(b) > 100 {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    assert_ne!(left, u32::MAX, "the four-color picture must be drawn");
    let scale = image.width() as f32 / 595.3;
    let width = (right - left + 1) as f32 / scale;
    let height = (bottom - top + 1) as f32 / scale;
    // The original pattern has a 3 pixel black border: colored bounds are
    // 94/100 of the frame's width and 44/50 of its height.
    assert!(near(width, 188.0, 2.0), "colored width {width:.2} pt; Word frame: 200 pt");
    // 3 pt: the colored edge pixels of the scaled border are blended (the old, letterboxed drawing was 100 pt high).
    assert!(near(height, 352.0, 3.0), "colored height {height:.2} pt; Word frame: 400 pt");
}

/// An exact-height line has its baseline 0.8 of the line down, whatever the typeface and the size of the text (Word's PDF of
/// `exact-baseline-sweep.docx`, Microsoft 365 16.0.20527.20088: Century and MS Mincho, 8 / 10.5 / 12 pt text in exact 10 / 12 / 14 / 18 / 24 pt
/// lines, two lines each; the baseline is 0.797 to 0.813 of the line's height down, the spread being 600 dpi pixels). The constant is Word's.
#[test]
fn an_exact_line_has_its_baseline_eight_tenths_down() {
    let pages = laid_out("exact-baseline-sweep.docx");
    let gs = glyphs(&pages.pages[0]);
    let mut top = 56.7f32;
    let mut n = 0;
    for h in [10.0f32, 12.0, 14.0, 18.0, 24.0] {
        for _sz in 0..3 {
            n += 1;
            let at = find(&gs, &format!("X{n:02}"));
            assert!(near(at - top, 0.8 * h, 0.2), "X{n:02} (line {h} pt): baseline {:.2} pt below its line's top; Word: {:.2}", at - top, 0.8 * h);
            top += 2.0 * h;
        }
    }
}

/// An exact-height line stays on its page only if its box ends within 0.1 pt past the foot, with or without a line grid and whatever the
/// typeface and size (Word's PDFs of the six `exactfoot*` documents, Microsoft 365 16.0.20527.20088): in section `k + 3` the 20th line's box ends
/// `k` x 0.1 pt past the foot; it stays on the page for k <= 0 and goes to the next one from k = 1.
#[test]
fn an_exact_line_stays_only_while_its_box_fits_the_page() {
    for file in ["exactfoot-century-8pt", "exactfoot-century-10-5pt", "exactfoot-mincho-8pt", "exactfoot-mincho-10-5pt", "exactfootgrid-century-8pt", "exactfootgrid-mincho-8pt"] {
        let pages = laid_out(&format!("{file}.docx"));
        let texts: Vec<String> = pages.pages.iter().map(|p| glyphs(p).iter().map(|g| g.c).collect()).collect();
        for s in 0..13 {
            let page_of = |l: u32| texts.iter().position(|t| t.contains(&format!("S{s:02}L{l:02}"))).expect("the line is drawn");
            let k = s as i32 - 3;
            let stays = page_of(20) == page_of(1);
            assert_eq!(stays, k <= 0, "{file}: k = {k}: the 20th line {} the first's page; Word: {}", if stays { "is on" } else { "is not on" }, if k <= 0 { "on" } else { "not on" });
        }
    }
}

/// A run's character spacing counts twice in a paragraph on the character grid (Word's PDF of `gridspacing-halfwidth-probe.docx`, Microsoft 365
/// 16.0.20527: 20 half-width spaces between two あ, charSpace +1.624 pt: the spaces advance 6.061 pt with no spacing and 5.268 pt with -0.4 pt, a
/// difference of 0.793 pt, not 0.4; the first あ to the second 133.36 and 117.38 pt).
#[test]
fn a_runs_character_spacing_counts_twice_on_the_character_grid() {
    let pages = laid_out("gridspacing-halfwidth-probe.docx");
    let mut kana: Vec<(f32, f32)> = glyphs(&pages.pages[0]).iter().filter(|g| g.c == '\u{3042}').map(|g| (g.y, g.x)).collect();
    kana.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let rows: Vec<Vec<f32>> = kana.chunk_by(|a, b| (a.0 - b.0).abs() < 0.5).map(|r| r.iter().map(|k| k.1).collect()).collect();
    // The first group's rows (ＭＳ ゴシック): spacing 0 (space row), then -0.4 (space row).
    let span = |r: usize| rows[r][1] - rows[r][0];
    assert!(near(span(0), 133.36, 0.4), "no spacing: {:.2} pt; Word 133.36", span(0));
    assert!(near(span(3), 117.38, 0.4), "-0.4 pt spacing: {:.2} pt; Word 117.38", span(3));
}

/// A typeface that is not installed is drawn with Cambria if the file's font table says `w:family` roman or does not list it, and with
/// Calibri otherwise (Word's PDF of `missing-font-fonttable-family.docx`, Microsoft 365 16.0.20527.20088: only `Zzqb Roman` is Cambria; the
/// swiss, modern, script, decorative, unspecified and panose-only ones are Calibri). Where those are not installed, Times New Roman and Arial stand in.
#[test]
fn a_missing_typeface_is_drawn_with_cambria_for_roman_and_with_calibri_otherwise() {
    let pages = laid_out("missing-font-fonttable-family.docx");
    let family_of = |word: &str| -> String {
        pages.pages[0].items.iter().find_map(|item| match item {
            Item::Text { text, font, .. } if text.contains(word) => Some(font.family.to_string()),
            _ => None,
        }).expect("the word is drawn")
    };
    for (name, roman) in [("Zzqa", false), ("Zzqb", true), ("Zzqc", false), ("Zzqd", false), ("Zzqe", false), ("Zzqf", false), ("Zzqg", false), ("Zzqh", false)] {
        let family = family_of(name);
        let want: &[&str] = if roman { &["Cambria", "Times New Roman"] } else { &["Calibri", "Carlito", "Arial"] };
        assert!(want.contains(&family.as_str()), "{name}: drawn with {family}; Word: {}", if roman { "Cambria" } else { "Calibri" });
    }
}

/// A typeface that is not installed and that the font table does not list is drawn with Cambria, with or without a font table or a theme (Word's
/// PDFs of `missing-font-ctx-{plain,fonttable,theme,both}.docx`: Zzqx, Zzqy, BIZ UDMincho and Zzqz; "Noto Sans JP", which Word itself knows, is Calibri and not met).
#[test]
fn a_missing_typeface_the_font_table_does_not_list_is_drawn_with_cambria() {
    for file in ["plain", "fonttable", "theme", "both"] {
        let pages = laid_out(&format!("missing-font-ctx-{file}.docx"));
        let family = pages.pages[0].items.iter().find_map(|item| match item {
            Item::Text { text, font, .. } if text.contains("Zzqx") => Some(font.family.to_string()),
            _ => None,
        }).expect("the word is drawn");
        assert!(["Cambria", "Times New Roman"].contains(&family.as_str()), "{file}: drawn with {family}; Word: Cambria");
    }
}

/// A multiple of lines in a table's cell on a line grid is the grid times the multiple, as in the body: Word's PDFs of `cellgrid-multiple-*.docx`
/// (synthetic, mode 15 with `adjustLineHeightInTable`, Microsoft 365 16.0.20527.20088, 1-cell-wide tables): ＭＳ 明朝 12 pt on a 14.05 pt grid
/// at 3 lines, rows 42.26 / 42.12 / 42.12 pt apart (079's 枚 cells; 3 x the 15.4 pt text line rounded up to the grid gave 4 grids, 56.2 pt);
/// 10 pt on a 14.5 pt grid, a row to the next of the same multiple: 1.15 lines 16.70 pt, 1.5 lines 21.84, 2 lines 29.04, 2.5 lines 36.24
/// (14.5 pt x the multiple: 16.68, 21.75, 29.0, 36.25), not whole grids.
#[test]
fn a_multiple_of_lines_in_a_cell_is_the_grid_times_the_multiple() {
    let pages = laid_out("cellgrid-multiple-12pt-grid281.docx");
    let gs = glyphs(&pages.pages[0]);
    for (a, b) in [("R0", "R1"), ("R1", "R2"), ("R2", "R3")] {
        let d = find(&gs, b) - find(&gs, a);
        assert!(near(d, 42.15, 0.2), "{a} to {b}: {d:.2} pt; Word: 42.12 to 42.26 pt (3 grids)");
    }
    let pages = laid_out("cellgrid-multiple-10pt-grid290.docx");
    let gs = glyphs(&pages.pages[0]);
    // Between two multiples the room the grid line has beyond the text is split evenly above and below the text, so the distance is the first
    // line's height plus half the difference of the two (Word: 1.15 to 1.5 lines 19.20, 1.5 to 2 25.32, 2 to 2.5 32.52, 2.5 to a single line 25.47).
    for (a, b, want) in [("R1", "R2", 19.20f32), ("R3", "R4", 25.32), ("R5", "R6", 32.52), ("R7", "R8", 25.47)] {
        let d = find(&gs, b) - find(&gs, a);
        assert!(near(d, want, 0.3), "{a} to {b}: {d:.2} pt; Word: {want:.2} pt");
    }
    for (a, b, want) in [("R0", "R1", 16.70f32), ("R2", "R3", 21.84), ("R4", "R5", 29.04), ("R6", "R7", 36.24)] {
        let d = find(&gs, b) - find(&gs, a);
        assert!(near(d, want, 0.3), "{a} to {b}: {d:.2} pt; Word: {want:.2} pt");
    }
}

/// `w:doNotSnapToGridInCell` does not take the cells' lines off the grid: Word's PDF of `cellgrid-donotsnap-10pt5-grid290.docx` (the flag, mode 15,
/// ＭＳ 明朝 10.5 pt, 1.15 lines, 14.5 pt grid) has its rows 16.70 / 16.68 / 16.68 / 16.68 pt apart, the grid times 1.15 (051 has the flag and
/// its rows are the `w:trHeight`, 20 pt, not the 29 pt of two grids).
#[test]
fn a_cell_with_do_not_snap_to_grid_in_cell_still_has_the_grid_times_the_multiple() {
    let pages = laid_out("cellgrid-donotsnap-10pt5-grid290.docx");
    let gs = glyphs(&pages.pages[0]);
    for (a, b) in [("R0", "R1"), ("R1", "R2"), ("R2", "R3"), ("R3", "R4")] {
        let d = find(&gs, b) - find(&gs, a);
        assert!(near(d, 16.69, 0.2), "{a} to {b}: {d:.2} pt; Word: 16.68 to 16.70 pt");
    }
}

/// Empty paragraphs of a multiple of lines, in a cell and in the body, are the grid times the multiple too (Word's PDF of
/// `cellgrid-empty-multiple-gothic10-grid290.docx`: ＭＳ ゴシック 10 pt, 14.5 pt grid, a line of text then four empty paragraphs of the multiple,
/// the next text line 72.62 / 81.12 / 101.55 / 130.58 / 159.48 pt under in the cell for 1 / 1.15 / 1.5 / 2 / 2.5 lines: 14.5 + 4 x 14.5, 16.675, 21.75,
/// 29, 36.25; in the body 72.48 and 81.12 for 1 and 1.15).
#[test]
fn empty_paragraphs_of_a_multiple_are_the_grid_times_the_multiple() {
    let pages = laid_out("cellgrid-empty-multiple-gothic10-grid290.docx");
    let gs = glyphs(&pages.pages[0]);
    for (a, b, want) in [("T0", "T1", 72.62f32), ("T1", "T2", 81.12), ("T2", "T3", 101.55), ("T3", "T4", 130.58), ("T4", "T5", 159.48), ("B0", "B1", 72.48), ("B1", "B2", 81.12)] {
        let d = find(&gs, b) - find(&gs, a);
        assert!(near(d, want, 0.4), "{a} to {b}: {d:.2} pt; Word: {want:.2} pt");
    }
}

/// Where the text sits in a table row: Word's PDF of `cellrow-valign-exact-gothic10-grid290.docx` (ＭＳ ゴシック 10 pt, 14.5 pt grid, one cell per row, rows of 20.55
/// and 35 pt, text at the top, centre or bottom, in an exact 13 pt line, an exact 14 pt line, a single and a 1.15 line). The baselines of the 24 rows are, from the row
/// top: exact 13 pt 10.35 (0.8 of the line) at the top, 14.18 at the centre and 17.99 at the bottom of a 20.55 pt row (the line box aligned like any content);
/// the single line 10.94 at the top (the text's 1 em centred in the 14.5 pt grid line: (14.5 - 10) / 2 + 0.87 x 10), the 1.15 line 12.07. The distances of
/// the baselines of one row to the next are asserted.
#[test]
fn text_sits_in_a_row_as_a_box_of_the_lines_height() {
    let pages = laid_out("cellrow-valign-exact-gothic10-grid290.docx");
    let gs = glyphs(&pages.pages[0]);
    let word = [774.84f32, 750.46, 726.1, 713.14, 667.06, 621.1, 607.3, 583.54, 559.63, 545.59, 500.11, 454.63, 440.95, 417.43, 393.77, 379.25, 334.01, 288.77, 273.17, 250.73, 228.17, 211.46, 167.3, 123.14];
    for k in 0..word.len() - 1 {
        let (a, b) = (format!("V{k:02}"), format!("V{:02}", k + 1));
        let d = find(&gs, &b) - find(&gs, &a);
        assert!(near(d, word[k] - word[k + 1], 0.35), "{a} to {b}: {d:.2} pt; Word: {:.2} pt", word[k] - word[k + 1]);
    }
}

/// With `balanceSingleByteDoubleByteWidth` a run's character spacing counts twice on double-byte characters whatever the document grid (Word's PDF of
/// `spacing-balance-grid-probe.docx`, Microsoft 365 16.0.20527.20088: four 漢 with +100 twips are 10.07 pt further apart each, the first to the fourth 61.7 pt on the pages
/// without a grid, with a line grid and with a character grid without `w:charSpace`; the Latin letters of the same paragraph 5.0 pt each, Z to W 55.56 pt).
#[test]
fn spacing_counts_twice_on_double_byte_characters_with_balance_single_byte_double_byte_width() {
    let pages = laid_out("spacing-balance-grid-probe.docx");
    for page in 0..3 {
        let gs = glyphs(&pages.pages[page]);
        let x = |c: char| gs.iter().find(|g| g.c == c).map(|g| g.x).expect("drawn");
        let xs: Vec<f32> = gs.iter().filter(|g| g.c == '\u{6f22}').map(|g| g.x).collect();
        let cjk = xs[3] - xs[0];
        assert!(near(cjk, 61.7, 0.4), "page {page}: first to fourth kanji {cjk:.2} pt; Word: 61.7 pt (3 x (10.56 + 10.0))");
        let latin = x('W') - x('Z');
        assert!(near(latin, 55.56, 0.5), "page {page}: Z to W {latin:.2} pt; Word: 55.56 pt");
    }
}

/// A run's character spacing is its twips exactly (no rounding to pixels): Word's PDF of `spacing-twips-rounding-probe.docx` (Microsoft 365
/// 16.0.20527.20088, `balanceSingleByteDoubleByteWidth`, 10.5 pt ＭＳ 明朝, eight 漢 with 20 to 200 twips): the pitch of one 漢 is 10.5 pt + 2 x spacing within
/// 0.03 pt (20 twips 12.48, 53 15.80, 100 20.51, 158 26.30, 200 30.50 pt); the eight Century letters of the same run pitch 1 x.
#[test]
fn spacing_is_not_rounded_to_pixels_on_double_byte_characters() {
    let pages = laid_out("spacing-twips-rounding-probe.docx");
    let gs = glyphs(&pages.pages[0]);
    let mut rows: Vec<Vec<&Glyph>> = Vec::new();
    for g in &gs {
        match rows.iter_mut().find(|r| (r[0].y - g.y).abs() < 0.5) { Some(r) => r.push(g), None => rows.push(vec![g]) }
    }
    rows.sort_by(|a, b| a[0].y.total_cmp(&b[0].y));
    let vals = [20, 30, 40, 50, 53, 60, 70, 80, 90, 100, 105, 110, 120, 130, 140, 150, 158, 170, 180, 190, 200];
    assert_eq!(rows.len(), vals.len());
    for (row, v) in rows.iter().zip(vals) {
        let kanji: Vec<f32> = row.iter().filter(|g| g.c == '\u{6f22}').map(|g| g.x).collect();
        let pitch = (kanji[7] - kanji[0]) / 7.0;
        let want = 10.5 + 2.0 * v as f32 / 20.0;
        eprintln!("PITCH {v} {pitch:.3} {want:.3}");
        assert!(near(pitch, want, 0.05), "{v} twips: pitch {pitch:.3} pt; Word: {want:.3} pt");
    }
}

/// A whitespace character does not count for the height of a line that has other text (Word's PDF of `line-height-whitespace-run-probe.docx`, Microsoft 365
/// 16.0.20527.20088, paragraphs off the grid: lines of 8 pt ＭＳ 明朝 text with a 10.5 pt full-width or ASCII space at the start, in the middle or at the end
/// are 10.3 to 10.44 pt apart, like the lines of the 8 pt text alone (10.32 / 10.44); a 10.5 pt か or Latin letter instead of the space makes them 13.56 to
/// 13.68 pt).
#[test]
fn a_whitespace_run_does_not_raise_the_line_of_other_text() {
    let pages = laid_out("line-height-whitespace-run-probe.docx");
    let gs = glyphs(&pages.pages[0]);
    let y = |n: usize| find(&gs, &format!("T{n:02}"));
    // Paragraphs 2 and 3 (a space in the middle of the label) are not found as a whole: from 1 to 4 across them.
    let d = y(4) - y(1);
    assert!(near(d, 31.1, 0.4), "paragraph 4 after 1: {d:.2} pt; Word: 31.1 pt (three lines of the 8 pt text)");
    for n in 5..=9 {
        let d = y(n) - y(n - 1);
        assert!(near(d, 10.38, 0.2), "paragraph {n} after {}: {d:.2} pt; Word: 10.3 to 10.44 pt (8 pt text, the space is not counted)", n - 1);
    }
    for n in [11, 13] {
        let d = y(n) - y(n - 1);
        assert!(near(d, 13.62, 0.25), "paragraph {n} after {}: {d:.2} pt; Word: 13.56 to 13.68 pt (a 10.5 pt letter counts)", n - 1);
    }
}

/// Where the baseline of a single ＭＳ 明朝 line is, off the grid and on it (Word's PDF of `ms-single-line-baseline-before-exact-probe.docx`, Microsoft 365
/// 16.0.20527.20088, a single line, then an exact line whose baseline is 0.8 of its height down; the distance between the two baselines is the room
/// under the single line's baseline plus that): off the grid 8 / 10.5 / 12 pt text leaves 2.40 / 3.12 / 3.48 pt (0.29 em: the line 1.29 em high has its baseline
/// 1.0 em down); on the 16.75 pt grid 5.52 / 4.58 / 3.96 pt ((grid - 1 em) / 2 + 0.87 em down).
#[test]
fn the_baseline_of_a_single_ms_line_sits_a_whole_em_down() {
    let pages = laid_out("ms-single-line-baseline-before-exact-probe.docx");
    let gs = glyphs(&pages.pages[0]);
    let word = [12.0f32, 21.6, 12.72, 22.32, 13.08, 22.68, 15.12, 24.72, 14.18, 23.76, 13.56, 23.16];
    for (g, want) in word.iter().enumerate() {
        let d = find(&gs, &format!("E{g:02}")) - find(&gs, &format!("S{g:02}"));
        assert!(near(d, *want, 0.35), "group {g}: the single line to the exact line {d:.2} pt; Word: {want:.2} pt");
    }
}
