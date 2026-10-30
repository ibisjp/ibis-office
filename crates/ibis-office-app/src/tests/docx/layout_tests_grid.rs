// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Layout rules of the line grid checked without a window (so that they run on Windows, where there is no scenario runner), laid
//! out as `layout_tests` does: a small file of `test/corpus-report/docx/`, by a headless app as the export does it.

use std::sync::Arc;

use super::flow_layout::{Item, Pagination};

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

/// P1: a line of the line grid at the foot of the page stays on it when its text fits, though the room the grid adds under the text
/// hangs past the foot (half of what the grid adds). Word's PDFs of 21 lines whose last one hangs 2 pt or 3 pt past the foot:
/// Century 10.5 pt stays with 2 pt and goes on with 3 pt, ＭＳ 明朝 10.5 pt stays with 1 pt and goes on with 2 pt (the faces differ
/// in the height of their lines: 12.1 pt and 13.58 pt in a 17.15 pt grid).
#[test]
fn p1_grid_line_stays_when_its_text_fits() {
    for (file, expected) in [("grid-foot-century-2pt.docx", 1), ("grid-foot-century-3pt.docx", 2), ("grid-foot-mincho-1pt.docx", 1), ("grid-foot-mincho-2pt.docx", 2)] {
        let pages = laid_out(file).pages.len();
        assert_eq!(pages, expected, "{file}: {pages} pages; Word: {expected}");
    }
}

/// The baseline of the first and the second paragraph of the first page.
fn second_gap(file: &str) -> f32 {
    let pages = laid_out(file);
    let mut ys: Vec<f32> = pages.pages[0].items.iter().filter_map(|item| if let Item::Text { y, text, .. } = item && !text.trim().is_empty() { Some(*y) } else { None }).collect();
    ys.sort_by(|a, b| a.total_cmp(b));
    assert!(ys.len() >= 2, "{file}: {} texts", ys.len());
    ys[1] - ys[0]
}

/// 019 (Word's PDF): an empty paragraph whose mark is hidden (`w:pPr/w:rPr/w:vanish`) takes no room, as no paragraph at all; an
/// ordinary empty one takes a line.
#[test]
fn hidden_mark_empty_paragraph_takes_no_room() {
    let none = second_gap("hidden-mark-none.docx");
    let hidden = second_gap("hidden-mark-empty.docx");
    let shown = second_gap("hidden-mark-shown-empty.docx");
    assert!((hidden - none).abs() < 0.01, "hidden {hidden} against none {none}");
    assert!(shown > none + 5.0, "an empty paragraph takes a line: {shown} against {none}");
}

/// 144 (Word's PDF): a paragraph style that is not based on Normal gets the document defaults' size, 10 pt when they have none (Word's
/// PDF shows the cells of style `a3`, which has no `w:basedOn`, at 9.96 pt, and the body at Normal's 10.5 pt).
#[test]
fn style_without_based_on_takes_the_default_size_not_normals() {
    let pages = laid_out("style-without-based-on-size.docx");
    let size_of = |word: &str| pages.pages[0].items.iter().find_map(|item| if let Item::Text { text, size, .. } = item && text.contains(word) { Some(*size) } else { None }).unwrap_or_else(|| panic!("no text {word}"));
    assert!((size_of("normal") - 10.5).abs() < 0.01, "Normal text {}", size_of("normal"));
    assert!((size_of("plain") - 10.0).abs() < 0.01, "text of a style not based on Normal {}", size_of("plain"));
}

/// 023 (Word's PDF): the body of the first page of a section with its own first page header starts where that header ends; the later pages
/// have the default header's (one line, in the margin), so their body starts at the top margin (42.55 pt), not under the first page's taller one.
/// `first-page-header-taller.docx`: a first page header of three lines, a default header of one, exact 12 pt body lines.
#[test]
fn first_page_header_pushes_only_the_first_page_body() {
    let pages = laid_out("first-page-header-taller.docx");
    assert!(pages.pages.len() >= 2, "{} pages", pages.pages.len());
    let first_y = |page: usize| {
        pages.pages[page].items.iter().filter_map(|item| if let Item::Text { y, text, .. } = item && text.starts_with("line") { Some(*y) } else { None }).fold(f32::MAX, f32::min)
    };
    let (first, second) = (first_y(0), first_y(1));
    assert!(first > second + 10.0, "first page body at {first}, later page at {second}");
    // The later page's first line is in a 12 pt line at the margin: its baseline is within the line (42.55 + 12).
    assert!(second < 42.55 + 12.0 + 0.5, "later page body starts at {second}");
}

/// The mean distance of the baselines of the lines of `size` pt text on the first page (the texts of one line each).
fn line_pitch_of(file: &str, size: f32) -> f32 {
    let pages = laid_out(file);
    let mut ys: Vec<f32> = pages.pages[0]
        .items
        .iter()
        .filter_map(|item| if let Item::Text { y, size: s, text, .. } = item && (s - size).abs() < 0.01 && text.contains("ABC") { Some(*y) } else { None })
        .collect();
    ys.sort_by(|a, b| a.total_cmp(b));
    assert!(ys.len() >= 6, "{file}: {} lines of {size} pt", ys.len());
    (ys[ys.len() - 1] - ys[0]) / (ys.len() - 1) as f32
}

/// 058 (Word's PDFs of `lineheight-msgothic-mode15-*`): a line of ＭＳ ゴシック 10.5 pt is as high in a table's cell as in the body (13.62 pt in
/// Word) in a document without a line grid, with or without `w:docGrid w:linePitch` (no `w:type`).
#[test]
fn ms_gothic_lines_in_a_cell_are_as_high_as_in_the_body() {
    for grid in ["grid360", "nogrid"] {
        let body = line_pitch_of(&format!("lineheight-msgothic-mode15-{grid}-body.docx"), 10.5);
        let cell = line_pitch_of(&format!("lineheight-msgothic-mode15-{grid}-table.docx"), 10.5);
        assert!((cell - body).abs() < 0.05, "{grid}: cell {cell}, body {body}");
        assert!((cell - 13.62).abs() < 0.2, "{grid}: {cell} pt; Word 13.62");
    }
}

/// 072 (Word's PDFs of `lineheight-*-mode15-nogrid-*`, Microsoft 365 16.0.20527.20088): the lines of an East Asian typeface are 1.3 times its
/// Windows ascent plus descent, in the body and in a table's cell, at 8 and 10.5 pt. (Word's pitches: メイリオ 15.60 / 20.46, Meiryo UI 13.2
/// at 8 pt, 游ゴシック and 游明朝 13.38 / 17.55, Yu Gothic UI 13.84 / 18.15.)
#[test]
fn east_asian_typefaces_lines_are_1_3_times_their_windows_height() {
    for (typeface, at_8, at_10_5) in [("meiryo", 15.60, 20.46), ("meiryoui", 13.2, 0.0), ("yugothic", 13.38, 17.55), ("yumincho", 13.38, 17.55), ("yugothicui", 13.84, 18.15)] {
        for place in ["body", "table"] {
            let file = format!("lineheight-{typeface}-mode15-nogrid-{place}.docx");
            let pitch = line_pitch_of(&file, 8.0);
            assert!((pitch - at_8).abs() < 0.1, "{file}: {pitch} pt at 8 pt; Word {at_8}");
            if at_10_5 > 0.0 {
                let pitch = line_pitch_of(&file, 10.5);
                assert!((pitch - at_10_5).abs() < 0.1, "{file}: {pitch} pt at 10.5 pt; Word {at_10_5}");
            }
        }
    }
}

/// The number of characters of each line of the first page, from the top, by the lines' `y` (a line's runs are summed).
fn chars_per_line(file: &str) -> Vec<usize> {
    let pages = laid_out(file);
    let mut lines: Vec<(f32, usize)> = Vec::new();
    for item in &pages.pages[0].items {
        if let Item::Text { y, text, .. } = item {
            match lines.iter_mut().find(|(ly, _)| (ly - y).abs() < 0.5) {
                Some(line) => line.1 += text.chars().count(),
                None => lines.push((*y, text.chars().count())),
            }
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    lines.into_iter().map(|l| l.1).collect()
}

/// 058・010 (Word's PDF of `compresspunct-mincho-w41453.docx`, ＭＳ 明朝 10.5 pt, 414.45 pt wide): 39 characters fit a line of kanji; a line with
/// stops squeezes them to take in one more character (40), and a second only when it cannot start a line (a 、 at the 41st place).
#[test]
fn a_line_takes_in_one_character_by_squeezing_its_stops_and_a_second_only_if_it_cannot_start_a_line() {
    let lines = chars_per_line("compresspunct-mincho-w41453.docx");
    // Word: 6 lines of 39 kanji, the paragraph's last 6 characters, then the paragraph of 、 every third character.
    assert_eq!(&lines[..6], &[39; 6], "{lines:?}");
    assert_eq!(&lines[7..13], &[40, 41, 40, 41, 40, 38], "{lines:?}");
    // The paragraphs with a stop every fourth to twelfth character have 40 characters in each line but the last.
    assert!(lines[13..43].iter().filter(|&&n| n == 40).count() >= 25, "{lines:?}");
}

/// The number of characters of each line of the whole document, in reading order (a line's runs summed by `y`, page by page).
fn chars_per_line_all(file: &str) -> Vec<usize> {
    let pages = laid_out(file);
    let mut all = Vec::new();
    for page in &pages.pages {
        let mut lines: Vec<(f32, usize)> = Vec::new();
        for item in &page.items {
            if let Item::Text { y, text, .. } = item {
                match lines.iter_mut().find(|(ly, _)| (ly - y).abs() < 0.5) {
                    Some(line) => line.1 += text.chars().count(),
                    None => lines.push((*y, text.chars().count())),
                }
            }
        }
        lines.sort_by(|a, b| a.0.total_cmp(&b.0));
        all.extend(lines.into_iter().map(|l| l.1));
    }
    all
}

/// The first line of each paragraph of a document of paragraphs of `total(index)` characters.
fn first_lines(file: &str, total: impl Fn(usize) -> usize) -> Vec<usize> {
    let lines = chars_per_line_all(file);
    let (mut out, mut at, mut index) = (Vec::new(), 0, 0);
    while at < lines.len() {
        let (mut got, wanted) = (0, total(index));
        out.push(lines[at]);
        while got < wanted && at < lines.len() {
            got += lines[at];
            at += 1;
        }
        index += 1;
    }
    out
}

/// Word squeezes a line to take in one more character only in a justified paragraph (Word's PDFs of `sqlimit-mincho-w41453.docx` and
/// `sqlimit2-mincho-w41453.docx`, left aligned, ＭＳ 明朝 9 / 10.5 / 12 pt, the width of the line swept in 0.1 pt steps from 0.45 to 0.8 em
/// short of the extra character, with one and with two stops in the line: 157 paragraphs, none takes it in; the justified paragraphs of
/// `compresspunct-mincho-w41453.docx` do).
#[test]
fn a_left_aligned_line_does_not_squeeze_its_stops_to_take_in_a_character() {
    // A (10.5 pt, one stop), B (two stops): 41 paragraphs each of 129 characters, 38 fit a line; C (12 pt): 43 of 124, 33; D (9 pt): 32 of 135, 44.
    let sizes: Vec<(usize, usize, usize)> = vec![(41, 129, 38), (41, 129, 38), (43, 124, 33), (32, 135, 44)];
    for file in ["sqlimit-mincho-w41453.docx", "sqlimit2-mincho-w41453.docx"] {
        let total = |index: usize| {
            let mut left = index;
            for (count, chars, _) in &sizes {
                if left < *count {
                    return *chars;
                }
                left -= count;
            }
            0
        };
        let firsts = first_lines(file, total);
        let mut index = 0;
        for (count, _, fits) in &sizes {
            for n in 0..*count {
                assert_eq!(firsts.get(index + n), Some(fits), "{file}: paragraph {}: {firsts:?}", index + n);
            }
            index += count;
        }
    }
}

/// A full-width space is squeezed like a stop in a justified paragraph (Word's PDF of `spacelimit-mincho-w41453.docx`: 1 to 8 spaces and 0 to
/// 3 half-width digits in the first 36 characters of 146: the first line has 40 characters, 41 when two or three digits make room).
/// Not done: 150 documents match Word's lines worse with it (002, 004, 142, 143, 144), so what stops Word from squeezing a space is not known.
#[test]
#[ignore = "a justified line's full-width spaces are not squeezed yet (the condition under which Word squeezes them is unknown)"]
fn a_justified_line_squeezes_its_full_width_spaces() {
    let firsts = first_lines("spacelimit-mincho-w41453.docx", |_| 146);
    let expected: Vec<usize> = (0..24).map(|i| if i % 4 >= 2 { 41 } else { 40 }).collect();
    assert_eq!(&firsts[..24], &expected[..], "{firsts:?}");
}

/// The first lines of the 7 paragraphs of series number `series` (of 7 paragraphs each) of a `squeezealign-mincho-w41453.docx`-like document: the
/// character that would be taken in is the 38th to the 44th, and a paragraph has that number plus 80 characters.
fn series_first_lines(file: &str, series: usize) -> Vec<usize> {
    let firsts = first_lines(file, |index| 38 + 80 + index % 7);
    firsts[series * 7..series * 7 + 7].to_vec()
}

/// What Word squeezes (Word's PDFs of `squeezealign-mincho-w41453.docx` and `squeezedist-mincho-w41453.docx`, ＭＳ 明朝 10.5 pt, 39 characters
/// fit): the 40th character, a kanji or a closing bracket that cannot start a line. Compatibility mode 15, no grid, lines, linesAndChars: only
/// a justified paragraph squeezes (40 characters), a left aligned one does not (39, and 38 when a ） is the 40th: the kanji before it goes
/// down with it); a distributed paragraph squeezes as a justified one does.
#[test]
fn only_a_justified_or_distributed_line_squeezes_in_mode_15() {
    // series: close (left aligned), kanji (left aligned), kanji_both (justified); the 7 paragraphs have the extra one at the 38th to 44th place.
    for grid in 0..3 {
        let file = "squeezealign-mincho-w41453.docx";
        assert_eq!(series_first_lines(file, grid * 3), [39, 39, 38, 39, 39, 39, 39], "grid {grid}, left aligned, closing bracket");
        assert_eq!(series_first_lines(file, grid * 3 + 1), [39; 7], "grid {grid}, left aligned, kanji");
        assert_eq!(series_first_lines(file, grid * 3 + 2), [40; 7], "grid {grid}, justified, kanji");
    }
    let file = "squeezedist-mincho-w41453.docx";
    assert_eq!(series_first_lines(file, 0), [40, 40, 40, 41, 40, 40, 40], "{file}, closing bracket");
    assert_eq!(series_first_lines(file, 1), [40; 7], "{file}, kanji");
}

/// Below mode 15 (Word's PDFs of `squeezecompat14-mincho-w41453.docx` and `squeezecompat11-…`, no grid, not in a table): a left aligned and a
/// justified paragraph squeeze nothing for a kanji (39), and a ） that would be the 40th character is pulled in (40). Not done: the documents
/// of the corpus of those modes (026, 152, 153, in a table's cell and with a grid) have their justified lines squeezed, so the squeeze below
/// mode 15 is left as it was until what tells the two apart is known.
#[test]
#[ignore = "below mode 15 the squeeze is left as it was: Word's lines of 026, 152 and 153 (justified, in a cell, with a grid) squeeze, the minimal files do not"]
fn below_mode_15_a_justified_line_squeezes_only_a_closing_bracket() {
    for file in ["squeezecompat14-mincho-w41453.docx", "squeezecompat11-mincho-w41453.docx", "squeezenocompat-mincho-w41453.docx"] {
        assert_eq!(series_first_lines(file, 0), [39, 39, 40, 39, 39, 39, 39], "{file}, left aligned, closing bracket");
        assert_eq!(series_first_lines(file, 1), [39; 7], "{file}, left aligned, kanji");
        assert_eq!(series_first_lines(file, 2), [39; 7], "{file}, justified, kanji");
    }
}

/// Which closing characters hang past the edge (Word's PDFs of `hang-line-end-closers-compat14-probe.docx` and `…compat15-probe.docx`: 20
/// full-width characters fill a 210 pt line, then one of ） 」 』 】 〕 〉 》 ］ ｝ 、 。 ， ． ’ ” ー ぁ っ, 18 paragraphs justified and 18 left
/// aligned). The first line has 21 characters when it hangs, 19 when the character goes down with the one before it, 20 when the character may
/// start a line (ー ぁ っ). 〕 〉 》 do not hang. Below mode 15 a left aligned line hangs them as a justified one does; in mode 15 it hangs nothing.
#[test]
fn closing_characters_hang_by_word_s_rules() {
    let hang_or_not = [21, 21, 21, 21, 19, 19, 19, 21, 21, 21, 21, 21, 21, 21, 21, 20, 20, 20];
    let left_15 = [19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 19, 20, 20, 20];
    for (mode, left) in [("14", hang_or_not), ("15", left_15)] {
        let firsts = first_lines(&format!("hang-line-end-closers-compat{mode}-probe.docx"), |_| 21);
        assert_eq!(&firsts[..18], &hang_or_not, "mode {mode}, justified");
        assert_eq!(&firsts[18..36], &left, "mode {mode}, left aligned");
    }
}

/// The first line of each paragraph of `spacecap-compat*-mincho-w41453.docx` (the "あ"×nA, s full-width spaces, "）", "ああ" paragraphs), counted
/// without the spaces (as the characters of Word's PDF are).
fn spacecap_first_lines(file: &str) -> Vec<usize> {
    let pages = laid_out(file);
    let mut all: Vec<usize> = Vec::new();
    for page in &pages.pages {
        let mut lines: Vec<(f32, usize)> = Vec::new();
        for item in &page.items {
            if let Item::Text { y, text, .. } = item {
                let n = text.chars().filter(|c| *c != '\u{3000}').count();
                match lines.iter_mut().find(|(ly, _)| (ly - y).abs() < 0.5) {
                    Some(line) => line.1 += n,
                    None => lines.push((*y, n)),
                }
            }
        }
        lines.sort_by(|a, b| a.0.total_cmp(&b.0));
        all.extend(lines.into_iter().map(|l| l.1));
    }
    let (mut firsts, mut at) = (Vec::new(), 0);
    for n_a in [19usize, 18, 17, 19, 18, 17] {
        for _s in 1..=8 {
            let wanted = n_a + 1 + 2;
            let mut got = 0;
            firsts.push(all[at]);
            while got < wanted && at < all.len() {
                got += all[at];
                at += 1;
            }
        }
    }
    firsts
}

/// Full-width spaces squeeze so that a closing character that must stay fits (Word's PDFs of `spacecap-compat11-mincho-w41453.docx` and
/// `…compat15…`: a line of 20 ems, "あ"×nA, s full-width spaces, "）"). Each space down to a quarter of an em and the run to a whole number of half
/// ems; beyond that the "）" hangs if it starts within the line, else it goes down with the "あ" before the spaces. Below mode 15 a left aligned
/// line does it as a justified one does; in mode 15 only a justified one does. The first line's characters, without the spaces:
#[test]
fn spaces_before_a_closing_character_squeeze_to_a_quarter_of_an_em_each() {
    let fits_19: [usize; 8] = [20, 20, 20, 20, 18, 18, 18, 18];
    let fits_18: [usize; 8] = [19; 8];
    let fits_17: [usize; 8] = [19, 18, 18, 18, 18, 18, 18, 18];
    for (file, left) in [("spacecap-compat11-mincho-w41453.docx", true), ("spacecap-compat15-mincho-w41453.docx", false)] {
        let firsts = spacecap_first_lines(file);
        assert_eq!(&firsts[24..32], &fits_19, "{file}, justified, 19");
        assert_eq!(&firsts[32..40], &fits_18, "{file}, justified, 18");
        assert_eq!(&firsts[40..48], &fits_17, "{file}, justified, 17");
        if left {
            assert_eq!(&firsts[0..8], &fits_19, "{file}, left aligned, 19");
            assert_eq!(&firsts[8..16], &fits_18, "{file}, left aligned, 18");
            assert_eq!(&firsts[16..24], &fits_17, "{file}, left aligned, 17");
        } else {
            assert_eq!(&firsts[0..8], &[18; 8], "{file}, left aligned, 19");
            assert_eq!(&firsts[8..16], &[19, 17, 17, 17, 17, 17, 17, 17], "{file}, left aligned, 18");
            assert_eq!(&firsts[16..24], &[19, 18, 16, 16, 16, 16, 16, 16], "{file}, left aligned, 17");
        }
    }
}

/// A 組み文字 wider than its cell stays whole, with half-width digits in it too (Word's PDF of corpus document 020: "(満18歳以上の者)" in a 40 pt
/// cell is one combined character with one pair of round brackets, in two rows). Seven characters, two of them digits (another typeface), in
/// a 40 pt cell: the digits do not split the run, nor does the width of the whole.
#[test]
fn a_combined_run_wider_than_its_cell_is_not_broken() {
    // The plain line breaker, and the one beside a floating table or shape.
    for file in ["combine-narrow-cell-mincho-w41453.docx", "combine-narrow-cell-floating-mincho-w41453.docx"] {
        a_combined_run_stays_whole(file);
    }
}

fn a_combined_run_stays_whole(file: &str) {
    let pages = laid_out(file);
    let (mut opens, mut closes, mut rows) = (0, 0, std::collections::BTreeSet::new());
    for item in &pages.pages[0].items {
        if let Item::Text { y, text, size, .. } = item {
            if *size > 10.0 && text.as_ref() == "(" {
                opens += 1;
            }
            if *size > 10.0 && text.as_ref() == ")" {
                closes += 1;
            }
            if *size < 8.0 && text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c) || c.is_ascii_digit()) {
                rows.insert((*y * 10.0).round() as i32);
            }
        }
    }
    assert_eq!((opens, closes), (1, 1), "{file}: one pair of brackets");
    assert_eq!(rows.len(), 2, "{file}: two rows of characters: {rows:?}");
}

/// The lines of ＭＳ 明朝 in a table's cell of a document with a line grid (`linesAndChars`, no `adjustLineHeightInTable`) are 1.3 times the size
/// apart, whatever the size (Word's PDF of `smallsize-grid230-mincho-w41453.docx`: 5.04 / 6.00 / 6.96 / 8.04 / 9.00 / 9.96 pt text at 6.48 /
/// 7.68-7.80 / 9.0-9.12 / 10.32-10.44 / 11.64-11.76 / 12.96 pt, the cell's lines off the 11.5 pt grid).
#[test]
fn lines_in_a_cell_off_the_grid_are_1_3_times_the_size_apart() {
    let pages = laid_out("smallsize-grid230-mincho-w41453.docx");
    for (size, pitch) in [(5.0f32, 6.5f32), (6.0, 7.8), (7.0, 9.1), (8.0, 10.4), (9.0, 11.7), (10.0, 13.0)] {
        let mut ys: Vec<f32> = Vec::new();
        for item in &pages.pages[0].items {
            if let Item::Text { x, y, text, size: s, .. } = item
                && text.as_ref() == "\u{3042}"
                && (*s - size).abs() < 0.01
                && *x > 95.0
                && *x < 96.5
            {
                ys.push(*y);
            }
        }
        ys.sort_by(|a, b| a.total_cmp(b));
        assert_eq!(ys.len(), 4, "{size} pt: {ys:?}");
        for pair in ys.windows(2) {
            assert!((pair[1] - pair[0] - pitch).abs() < 0.2, "{size} pt: lines {} apart, Word's {pitch}: {ys:?}", pair[1] - pair[0]);
        }
    }
}
