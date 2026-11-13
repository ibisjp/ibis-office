// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Layout rules of the line grid checked without a window (so that they run on Windows, where there is no scenario runner), laid
//! out as `layout_tests` does: a small file of `test/corpus-report/docx/`, by a headless app as the export does it.

use std::sync::Arc;

use crate::docx::flow_layout::{Item, Pagination};
// The scenarios' driver below calls the document's methods (Windows runs these as plain tests, without it).
#[cfg(not(windows))]
use crate::docx::ext::*;

/// The checks, run by the scenario runner (`--test scenarios -- docx::layout_grid`): macOS and Linux have no other way to run them (GPUI's macOS platform
/// can be created only on the main thread, and `cargo test` runs a test on another one). On Windows they are `#[test]`s of `cargo test --lib`.
#[cfg(not(windows))]
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::layout_grid::p1_grid_line_stays_when_its_text_fits", p1_grid_line_stays_when_its_text_fits),
        ("docx::layout_grid::hidden_mark_empty_paragraph_takes_no_room", hidden_mark_empty_paragraph_takes_no_room),
        ("docx::layout_grid::style_without_based_on_takes_the_default_size_not_normals", style_without_based_on_takes_the_default_size_not_normals),
        ("docx::layout_grid::first_page_header_pushes_only_the_first_page_body", first_page_header_pushes_only_the_first_page_body),
        ("docx::layout_grid::a_page_break_s_mark_goes_to_the_next_page_with_split_pg_break_and_para_mark_below_mode_15", a_page_break_s_mark_goes_to_the_next_page_with_split_pg_break_and_para_mark_below_mode_15),
        ("docx::layout_grid::east_asian_typefaces_lines_are_1_3_times_their_windows_height", east_asian_typefaces_lines_are_1_3_times_their_windows_height),
        ("docx::layout_grid::a_line_takes_in_one_character_by_squeezing_its_stops_and_a_second_only_if_it_cannot_start_a_line", a_line_takes_in_one_character_by_squeezing_its_stops_and_a_second_only_if_it_cannot_start_a_line),
        ("docx::layout_grid::a_left_aligned_line_does_not_squeeze_its_stops_to_take_in_a_character", a_left_aligned_line_does_not_squeeze_its_stops_to_take_in_a_character),
        ("docx::layout_grid::only_a_justified_or_distributed_line_squeezes_in_mode_15", only_a_justified_or_distributed_line_squeezes_in_mode_15),
        ("docx::layout_grid::closing_characters_hang_by_word_s_rules", closing_characters_hang_by_word_s_rules),
        ("docx::layout_grid::spaces_before_a_closing_character_squeeze_to_a_quarter_of_an_em_each", spaces_before_a_closing_character_squeeze_to_a_quarter_of_an_em_each),
        ("docx::layout_grid::lines_in_a_cell_off_the_grid_are_1_3_times_the_size_apart", lines_in_a_cell_off_the_grid_are_1_3_times_the_size_apart),
        ("docx::layout_grid::ivs_selector_takes_no_room", ivs_selector_takes_no_room),
        ("docx::layout_grid::a_yu_line_on_a_grid_is_1_673_em_high_and_takes_the_grids_that_hold_it", a_yu_line_on_a_grid_is_1_673_em_high_and_takes_the_grids_that_hold_it),
        ("docx::layout_grid::a_multiple_on_a_grid_is_the_larger_of_the_grids_the_line_takes_and_the_grid_times_it", a_multiple_on_a_grid_is_the_larger_of_the_grids_the_line_takes_and_the_grid_times_it),
    ]
}

/// Unfixed rules, skipped in normal runs (`--ignored` runs them).
#[cfg(not(windows))]
pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    vec![
        // 期待値は Windows の Word（MS 書体）から測ったもの。Mac には該当書体が無く字幅が変わる。Windows で流す（cargo test --lib）。// i18n-ok:
        ("docx::layout_grid::ms_gothic_lines_in_a_cell_are_as_high_as_in_the_body", ms_gothic_lines_in_a_cell_are_as_high_as_in_the_body),
        ("docx::layout_grid::a_combined_run_wider_than_its_cell_is_not_broken", a_combined_run_wider_than_its_cell_is_not_broken),
        ("docx::layout_grid::emoji_selector_and_joiner_take_no_room", emoji_selector_and_joiner_take_no_room),
        ("docx::layout_grid::a_justified_line_squeezes_its_full_width_spaces", a_justified_line_squeezes_its_full_width_spaces),
        ("docx::layout_grid::below_mode_15_a_justified_line_squeezes_only_a_closing_bracket", below_mode_15_a_justified_line_squeezes_only_a_closing_bracket),
        ("docx::layout_grid::below_mode_15_a_line_squeezes_by_one_stop_s_half_at_most", below_mode_15_a_line_squeezes_by_one_stop_s_half_at_most),
        ("docx::layout_grid::a_proportional_face_squeezes_no_stop", a_proportional_face_squeezes_no_stop),
        ("docx::layout_grid::an_opening_bracket_that_starts_a_line_keeps_its_half_em", an_opening_bracket_that_starts_a_line_keeps_its_half_em),
    ]
}

#[cfg(not(windows))]
fn laid_out(file: &str) -> Arc<Pagination> {
    let mut s = crate::shell::scenario::Scenario::open(&format!("test/corpus-report/docx/{file}"));
    s.with(|e, _| crate::docx::flow_layout::pages(e.flow().expect("a document"), &e.resources))
}

#[cfg(windows)]
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
    crate::docx::flow_layout::update_fields_on_open(&mut presentation, &resources);
    crate::docx::flow_layout::pages(presentation.flow.as_ref().expect("a flowing document"), &resources)
}

/// P1: a line of the line grid at the foot of the page stays on it when its text fits, though the room the grid adds under the text
/// hangs past the foot (half of what the grid adds). Word's PDFs of 21 lines whose last one hangs 2 pt or 3 pt past the foot:
/// Century 10.5 pt stays with 2 pt and goes on with 3 pt, ＭＳ 明朝 10.5 pt stays with 1 pt and goes on with 2 pt (the faces differ
/// in the height of their lines: 12.1 pt and 13.58 pt in a 17.15 pt grid).
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
fn hidden_mark_empty_paragraph_takes_no_room() {
    let none = second_gap("hidden-mark-none.docx");
    let hidden = second_gap("hidden-mark-empty.docx");
    let shown = second_gap("hidden-mark-shown-empty.docx");
    assert!((hidden - none).abs() < 0.01, "hidden {hidden} against none {none}");
    assert!(shown > none + 5.0, "an empty paragraph takes a line: {shown} against {none}");
}

/// 144 (Word's PDF): a paragraph style that is not based on Normal gets the document defaults' size, 10 pt when they have none (Word's
/// PDF shows the cells of style `a3`, which has no `w:basedOn`, at 9.96 pt, and the body at Normal's 10.5 pt).
#[cfg_attr(windows, test)]
fn style_without_based_on_takes_the_default_size_not_normals() {
    let pages = laid_out("style-without-based-on-size.docx");
    let size_of = |word: &str| pages.pages[0].items.iter().find_map(|item| if let Item::Text { text, size, .. } = item && text.contains(word) { Some(*size) } else { None }).unwrap_or_else(|| panic!("no text {word}"));
    assert!((size_of("normal") - 10.5).abs() < 0.01, "Normal text {}", size_of("normal"));
    assert!((size_of("plain") - 10.0).abs() < 0.01, "text of a style not based on Normal {}", size_of("plain"));
}

/// 023 (Word's PDF): the body of the first page of a section with its own first page header starts where that header ends; the later pages
/// have the default header's (one line, in the margin), so their body starts at the top margin (42.55 pt), not under the first page's taller one.
/// `first-page-header-taller.docx`: a first page header of three lines, a default header of one, exact 12 pt body lines.
/// A proportional face (its 、 narrower than an em) squeezes no stop or bracket (Word's PDFs, Microsoft 365 16.0.20527.20088, of the squeeze
/// file in 8 pt Meiryo UI, mode 15, and ＭＳ Ｐ明朝, mode 11: 14 left aligned then 7 justified paragraphs, the justified lines as long as the
/// left aligned ones; in 游明朝 the justified lines of mode 15 take in one more).
#[cfg_attr(windows, test)]
fn a_proportional_face_squeezes_no_stop() {
    let meiryo: Vec<usize> = (0..21).flat_map(|k| [52, 51, 15 + k % 7]).collect();
    let msp: Vec<usize> = (0..21).flat_map(|k| [53, 51, 14 + k % 7]).collect();
    assert_eq!(chars_per_line_all("squeeze-proportional-meiryoui-mode15-probe.docx"), meiryo, "Meiryo UI 8 pt, mode 15");
    assert_eq!(chars_per_line_all("squeeze-proportional-msp-mode11-probe.docx"), msp, "ＭＳ Ｐ明朝 8 pt, mode 11"); // i18n-ok（試験・ファイルの中の日本語のデータ）
    // 游明朝 sets 、 at an em: its justified lines take in one more character.
    let yu: Vec<usize> = (0..21).flat_map(|k| if k < 14 { [51, 51, 16 + k % 7] } else { [52, 51, 15 + k % 7] }).collect();
    assert_eq!(chars_per_line_all("squeeze-proportional-yumincho-mode15-probe.docx"), yu, "游明朝 8 pt, mode 15"); // i18n-ok（試験・ファイルの中の日本語のデータ）
}

/// An opening bracket that starts a line keeps its half em (Word's PDFs, Microsoft 365 16.0.20527.20088, of
/// `squeeze-line-start-bracket-mode*-probe.docx`: ＭＳ 明朝 10.5 pt on a line of 40.55 em, so a 41st character is 0.45 em over; left aligned
/// then justified paragraphs of 70 characters: kanji, （ then kanji, （ and 、 among them, 、 among them, 「 then kanji, （（ then kanji).
#[cfg_attr(windows, test)]
fn an_opening_bracket_that_starts_a_line_keeps_its_half_em() {
    let firsts = |file: &str| first_lines(file, |_| 70);
    let m11 = firsts("squeeze-line-start-bracket-mode11-probe.docx");
    assert_eq!(m11, [40, 40, 41, 41, 40, 41, 40, 40, 41, 41, 40, 41], "mode 11");
    let m15 = firsts("squeeze-line-start-bracket-mode15-probe.docx");
    // The lines that start with one bracket (not yet the others of mode 15: Word keeps 40 in the justified line with 、 and takes in a 41st
    // in the left aligned one that starts with （（, why is not known).
    assert_eq!([m15[1], m15[4], m15[7], m15[10]], [40, 40, 40, 40], "mode 15: {m15:?}");
}

#[cfg_attr(windows, test)]
fn a_page_break_s_mark_goes_to_the_next_page_with_split_pg_break_and_para_mark_below_mode_15() {
    // Word's PDFs (Microsoft 365 16.0.20527.20088): "PAGE-A", a paragraph of a page break (two in the second file), an empty paragraph that
    // ends a section (next page), "PAGE-B". The page "PAGE-B" is on, and the number of pages.
    let page_of_b = |file: &str| -> (usize, usize) {
        let pages = laid_out(file);
        let at = pages.pages.iter().position(|page| page.items.iter().any(|item| matches!(item, Item::Text { text, .. } if text.contains("PAGE-B"))));
        (at.unwrap_or_else(|| panic!("{file}: PAGE-B is drawn")) + 1, pages.pages.len())
    };
    assert_eq!(page_of_b("split-page-break-mark-mode11-probe.docx"), (3, 3), "mode 11 with the flag: the mark and the section's end make page 2");
    assert_eq!(page_of_b("split-page-break-mark-mode11-two-breaks-probe.docx"), (4, 4), "mode 11 with the flag, two page breaks");
    assert_eq!(page_of_b("split-page-break-mark-mode11-off-probe.docx"), (2, 2), "mode 11 without the flag");
    assert_eq!(page_of_b("split-page-break-mark-mode15-probe.docx"), (2, 2), "mode 15 leaves the flag alone");
}

#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
#[cfg_attr(windows, ignore = "a justified line's full-width spaces are not squeezed yet (the condition under which Word squeezes them is unknown)")]
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
#[cfg_attr(windows, test)]
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
/// justified paragraph squeeze nothing for a kanji (39: the 40th needs 0.53 em, more than one stop's half), and a ） that would be the 40th
/// character is pulled in (40).
#[cfg_attr(windows, test)]
fn below_mode_15_a_justified_line_squeezes_only_a_closing_bracket() {
    for file in ["squeezecompat14-mincho-w41453.docx", "squeezecompat11-mincho-w41453.docx", "squeezenocompat-mincho-w41453.docx"] {
        assert_eq!(series_first_lines(file, 0), [39, 39, 40, 39, 39, 39, 39], "{file}, left aligned, closing bracket");
        assert_eq!(series_first_lines(file, 1), [39; 7], "{file}, left aligned, kanji");
        assert_eq!(series_first_lines(file, 2), [39; 7], "{file}, justified, kanji");
    }
}

/// Below mode 15 a line gives up no more than one stop's half em to take in one more character (Word's PDFs of mode 11, Microsoft 365
/// 16.0.20527.20088: `squeeze-compat11-mincho105-probe.docx` and `…-mincho12-probe.docx`, ＭＳ 明朝 in 414.45 pt, 14 left aligned then 7
/// justified paragraphs of kanji with 、 （ ） here and there). At 10.5 pt the 40th character needs 0.53 em and no line takes it in (only a ）
/// that cannot start a line is); at 12 pt the 35th needs 0.5 em and the lines with a stop take it in, left aligned or justified. The same
/// lines come out of Word with a line grid, a grid of lines and characters, in a table's cell, with `useFELayout` and distributed.
#[cfg_attr(windows, test)]
fn below_mode_15_a_line_squeezes_by_one_stop_s_half_at_most() {
    let mincho105 = [39, 39, 39, 1, 39, 39, 39, 2, 40, 39, 39, 2, 39, 39, 39, 4, 39, 39, 39, 5, 39, 39, 39, 6, 39, 39, 39, 7, 39, 39, 39, 1, 39, 39, 39, 2, 39, 39, 39, 3, 39, 39, 39, 4, 39, 39, 39, 5, 39, 39, 39, 6, 39, 39, 39, 7, 39, 39, 39, 1, 39, 39, 39, 2, 39, 39, 39, 3, 39, 39, 39, 4, 39, 39, 39, 5, 39, 39, 39, 6, 39, 39, 39, 7];
    let mincho12 = [35, 35, 34, 14, 35, 35, 34, 15, 35, 35, 34, 16, 35, 35, 34, 17, 35, 35, 34, 18, 35, 35, 34, 19, 35, 35, 34, 20, 35, 34, 34, 15, 35, 34, 34, 16, 35, 34, 34, 17, 35, 34, 34, 18, 35, 34, 34, 19, 35, 34, 34, 20, 35, 34, 34, 21, 35, 34, 34, 15, 35, 34, 34, 16, 35, 34, 34, 17, 35, 34, 34, 18, 35, 34, 34, 19, 35, 34, 34, 20, 35, 34, 34, 21];
    assert_eq!(chars_per_line_all("squeeze-compat11-mincho105-probe.docx"), mincho105, "10.5 pt");
    assert_eq!(chars_per_line_all("squeeze-compat11-mincho12-probe.docx"), mincho12, "12 pt");
    // On a character grid (`linesAndChars`) the body's lines take in none, a table's cell squeezes as without the grid.
    let chargrid: Vec<usize> = (0..21).flat_map(|k| [34, 34, 34, 16 + k % 7]).collect();
    assert_eq!(chars_per_line_all("squeeze-compat11-mincho12-chargrid-probe.docx"), chargrid, "12 pt, character grid");
    assert_eq!(chars_per_line_all("squeeze-compat11-mincho12-cell-chargrid-probe.docx"), mincho12, "12 pt, character grid, in a cell");
}

/// Which closing characters hang past the edge (Word's PDFs of `hang-line-end-closers-compat14-probe.docx` and `…compat15-probe.docx`: 20
/// full-width characters fill a 210 pt line, then one of ） 」 』 】 〕 〉 》 ］ ｝ 、 。 ， ． ’ ” ー ぁ っ, 18 paragraphs justified and 18 left
/// aligned). The first line has 21 characters when it hangs, 19 when the character goes down with the one before it, 20 when the character may
/// start a line (ー ぁ っ). 〕 〉 》 do not hang. Below mode 15 a left aligned line hangs them as a justified one does; in mode 15 it hangs nothing.
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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

/// #79: an ideographic variation sequence (a kanji and U+E0100..E01EF) is one character: the selector takes no room, so the text
/// after it is where it is without the selector (it took a full em, a blank, before). `ivs-sequence.docx`: 奈良県葛 + U+E0100 +
/// 城市, and 東京都葛飾区 without a selector, in the same size.
#[cfg_attr(windows, test)]
fn ivs_selector_takes_no_room() {
    let pages = laid_out("ivs-sequence.docx");
    let x_of = |text_has: &str, target: char| -> f32 {
        pages.pages[0].items.iter().find_map(|item| {
            if let Item::Text { x, text, offsets, .. } = item && text.contains(text_has) {
                let at = text.chars().position(|c| c == target)?;
                Some(x + offsets[at])
            } else {
                None
            }
        }).unwrap_or_else(|| panic!("no text {text_has}"))
    };
    // 飾 follows four characters; 城 follows four characters and a selector.
    let (a, b) = (x_of("東京都", '飾'), x_of("奈良県", '城')); // i18n-ok: test data
    assert!((a - b).abs() < 0.01, "飾 at {a}, 城 at {b}"); // i18n-ok: test data
}

/// #66: the emoji presentation selector and a zero width joiner start where the character before them ends, and so does the next character:
/// `emoji-sequence.docx` has ⛓, ⛓ + U+FE0F, 🧑 and 🧑 + ZWJ + 💻, one paragraph each, each followed by あ. The offsets of a text are where each
/// character starts (not where the last one ends), so the あ tells where the sequence ends. The selector may change the face (⛓ alone is
/// drawn in the text face, with the selector in the colour emoji face, wider), so it is the selector's own room that is measured.
#[cfg_attr(windows, test)]
fn emoji_selector_and_joiner_take_no_room() {
    let pages = laid_out("emoji-sequence.docx");
    // Every character's start, with its baseline.
    let mut at: Vec<(char, f32, f32)> = Vec::new();
    for item in &pages.pages[0].items {
        if let Item::Text { x, y, text, offsets, .. } = item {
            at.extend(text.chars().zip(offsets.iter()).map(|(c, dx)| (c, *y, x + dx)));
        }
    }
    let x_of = |c: char, nth: usize| -> (f32, f32) {
        at.iter().filter(|g| g.0 == c).nth(nth).map(|g| (g.1, g.2)).unwrap_or_else(|| panic!("{c:?} number {nth} is drawn: {at:?}"))
    };
    // The paragraphs' あ, top down.
    let mut a: Vec<(f32, f32)> = at.iter().filter(|g| g.0 == 'あ').map(|g| (g.1, g.2)).collect(); // i18n-ok: test data
    a.sort_by(|p, q| p.0.total_cmp(&q.0));
    assert_eq!(a.len(), 4, "one あ for each paragraph: {at:?}"); // i18n-ok: test data
    let (chain_y, chain) = x_of('\u{26D3}', 1);
    let (_, selector) = x_of('\u{FE0F}', 0);
    assert!((chain_y - a[1].0).abs() < 0.01, "the second ⛓ is in the second paragraph");
    assert!(selector - chain > 1.0, "⛓ has a width: {chain} to {selector}");
    assert!((a[1].1 - selector).abs() < 0.01, "the selector takes room: it starts at {selector}, あ at {}", a[1].1); // i18n-ok: test data
    let (person_y, person) = x_of('\u{1F9D1}', 1);
    let (_, joiner) = x_of('\u{200D}', 0);
    let (_, laptop) = x_of('\u{1F4BB}', 0);
    assert!((person_y - a[3].0).abs() < 0.01, "the second 🧑 is in the fourth paragraph");
    assert!(joiner - person > 1.0, "🧑 has a width: {person} to {joiner}");
    assert!((laptop - joiner).abs() < 0.01 && (a[3].1 - joiner).abs() < 0.01, "the joiner or the joined character takes room: ZWJ at {joiner}, 💻 at {laptop}, あ at {}", a[3].1); // i18n-ok: test data
}

/// The distance of each `label` line to the line under it (the paragraph's second line), by the label's text, in the lines of the pages.
fn label_pitches(file: &str) -> Vec<(String, f32)> {
    let pages = laid_out(file);
    let mut out = Vec::new();
    for page in &pages.pages {
        let mut lines: Vec<(f32, String)> = page
            .items
            .iter()
            .filter_map(|item| if let Item::Text { y, text, .. } = item { Some((*y, text.to_string())) } else { None })
            .collect();
        lines.sort_by(|a, b| a.0.total_cmp(&b.0));
        for pair in lines.windows(2) {
            if pair[0].1.chars().next().is_some_and(|c| c.is_ascii_digit()) && pair[1].1.starts_with("行間") { // i18n-ok (Japanese text under test or a name table that matches labels)
                out.push((pair[0].1.clone(), pair[1].0 - pair[0].0));
            }
        }
    }
    out
}

/// GitHub issue 54 (gakusyosi's test.docx: 24 pt 游明朝 at `w:line="259"` on an 18 pt grid, 54 pt in Word, 36 pt here): a line of 游明朝 or 游ゴシック
/// on the line grid is 1.673 em high (1.3 times its Windows ascent plus descent, 1.2871 em, as off the grid), and takes the grids that hold that
/// (a grid tied with the line's height, to the 0.05 pt of a `w:linePitch`, takes one more). Word's PDFs of `grid-ratio-yumincho-326-340.docx`,
/// `grid-ratio-yugothic-326-340.docx` and `grid-ratio-yumincho-398-402.docx` (Microsoft 365 16.0.20527; 游明朝 and 游ゴシック alike): the lines of
/// 20, 40 and 60 pt on grids of 16.3 to 17.0 pt take one grid fewer from 16.75 pt up (20 pt: 3 to 2 grids, 40 pt: 5 to 4, 60 pt: 7 to 6); 12 pt
/// takes 2 grids and 36 pt 4 on grids of 19.9 to 20.05 pt, and 1 and 3 on 20.1 pt; 54 pt takes 6 on all of them.
#[cfg_attr(windows, test)]
fn a_yu_line_on_a_grid_is_1_673_em_high_and_takes_the_grids_that_hold_it() {
    // (size, pitch in twips) -> grids, section by section: the labels are `<twips>_<size>`.
    let grids = |file: &str| -> Vec<(u32, String, f32)> {
        label_pitches(file)
            .into_iter()
            .map(|(label, pitch)| {
                let (twips, size) = label.split_once('_').expect("a label");
                let twips: u32 = twips.parse().expect("twips");
                (twips, size.to_string(), pitch / (twips as f32 / 20.0))
            })
            .collect()
    };
    for file in ["grid-ratio-yumincho-326-340.docx", "grid-ratio-yugothic-326-340.docx"] {
        let found = grids(file);
        assert_eq!(found.len(), 15 * 6, "{file}: {} paragraphs", found.len());
        for (twips, size, n) in found {
            let want = match (size.as_str(), twips <= 334) {
                ("12", _) => 2.0,
                ("20", true) => 3.0,
                ("20", false) => 2.0,
                ("36", _) => 4.0,
                ("40", true) => 5.0,
                ("40", false) => 4.0,
                ("54", _) => 6.0,
                ("60", true) => 7.0,
                ("60", false) => 6.0,
                _ => unreachable!("{size}"),
            };
            assert!((n - want).abs() < 0.05, "{file}: {size} pt on {twips} twips takes {n} grids; Word {want}");
        }
    }
    for (twips, size, n) in grids("grid-ratio-yumincho-398-402.docx") {
        let want = match (size.as_str(), twips <= 401) {
            ("12", true) => 2.0,
            ("12", false) => 1.0,
            ("36", true) => 4.0,
            ("36", false) => 3.0,
            _ => unreachable!("{size}"),
        };
        assert!((n - want).abs() < 0.05, "{size} pt on {twips} twips takes {n} grids; Word {want}");
    }
}

/// A multiple of lines (`w:lineRule="auto"`) on a line grid: the larger of the grids the single line takes and the grid times the multiple (Word's
/// PDFs of `line-multiple-grid-sweep-*.docx`, 8 to 60 pt, multiples 1 / 259 / 360 / 480 twentieths; the same in 游明朝 and 游ゴシック). On a grid
/// of 18 pt: up to 10.5 pt the line is 18 / 19.44 / 27 / 36 pt, 11 to 20 pt 36 pt, 22 to 32 pt 54, 36 and 40 pt 72, 44 and 48 pt 90, 54 and 60 pt
/// 108, whatever the multiple (up to 2); on 14.5 pt: 8 pt 14.52 / 15.72 / 21.72 / 29.04, 9 to 16 pt 29, 18 to 26 pt 43.5, 28 to 32 pt 58.
#[cfg_attr(windows, test)]
fn a_multiple_on_a_grid_is_the_larger_of_the_grids_the_line_takes_and_the_grid_times_it() {
    let want_360 = |size: f32, m: u32| -> f32 {
        let grids = match size {
            s if s <= 10.5 => return 18.0 * m as f32 / 240.0,
            s if s <= 20.0 => 2.0,
            s if s <= 32.0 => 3.0,
            s if s <= 40.0 => 4.0,
            s if s <= 48.0 => 5.0,
            _ => 6.0,
        };
        grids * 18.0
    };
    let want_290 = |size: f32, m: u32| -> f32 {
        let grids = match size {
            s if s <= 8.0 => return 14.5 * m as f32 / 240.0,
            s if s <= 16.0 => 2.0,
            s if s <= 26.0 => 3.0,
            s if s <= 32.0 => 4.0,
            s if s <= 40.0 => 5.0,
            s if s <= 48.0 => 6.0,
            _ => 7.0,
        };
        grids * 14.5
    };
    for (file, want) in [
        ("line-multiple-grid-sweep-yumincho-g360.docx", &want_360 as &dyn Fn(f32, u32) -> f32),
        ("line-multiple-grid-sweep-yugothic-g360.docx", &want_360),
        ("line-multiple-grid-sweep-yumincho-g290.docx", &want_290),
    ] {
        let found = label_pitches(file);
        assert!(found.len() >= 80, "{file}: {} of 88 paragraphs found", found.len());
        for (label, pitch) in found {
            let (size, m) = label.split_once("pt ").expect("a label");
            let (size, m): (f32, u32) = (size.parse().expect("size"), m.parse().expect("multiple"));
            let expected = want(size, m);
            assert!((pitch - expected).abs() < 0.15, "{file}: {size} pt at {m}/240 lines is {pitch} pt apart; Word {expected}");
        }
    }
}
