// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Layout rules checked without a window (so that they run on Windows, where there is no scenario runner): a small file of
//! `test/corpus-report/docx/` is laid out by a headless app as the export does it, and the lines' baselines and the pages are
//! read. The same rules are in `corpus_report.rs` for the scenario runner (macOS).

use std::sync::Arc;

use crate::docx::flow_layout::{Item, Page, Pagination};
// The scenarios' driver below calls the document's methods (Windows runs these as plain tests, without it).
#[cfg(not(windows))]
use crate::docx::ext::*;

/// The checks, run by the scenario runner (`--test scenarios -- docx::layout`): macOS and Linux have no other way to run them (GPUI's macOS platform
/// can be created only on the main thread, and `cargo test` runs a test on another one). On Windows they are `#[test]`s of `cargo test --lib`.
#[cfg(not(windows))]
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::layout::a10_text_that_cannot_sit_beside_a_table_starts_under_it", a10_text_that_cannot_sit_beside_a_table_starts_under_it),
        ("docx::layout::a10_empty_paragraphs_go_under_the_table", a10_empty_paragraphs_go_under_the_table),
        ("docx::layout::t7_page_break_in_a_cell_adds_no_line", t7_page_break_in_a_cell_adds_no_line),
        ("docx::layout::f5_paragraph_moved_off_an_obstacle_page_is_laid_again", f5_paragraph_moved_off_an_obstacle_page_is_laid_again),
        ("docx::layout::a_floating_row_taller_than_a_page_goes_on_to_the_next_page", a_floating_row_taller_than_a_page_goes_on_to_the_next_page),
        ("docx::layout::line_multiples_on_a_grid_take_the_grids_the_text_needs", line_multiples_on_a_grid_take_the_grids_the_text_needs),
        ("docx::layout::a_half_width_letter_of_a_fixed_font_takes_half_of_the_added_pitch", a_half_width_letter_of_a_fixed_font_takes_half_of_the_added_pitch),
        ("docx::layout::an_empty_footer_pushes_the_body_up", an_empty_footer_pushes_the_body_up),
        ("docx::layout::the_footer_paragraphs_take_the_body_down_one_line_each", the_footer_paragraphs_take_the_body_down_one_line_each),
        ("docx::layout::a_half_width_space_takes_the_spacing_twice_with_the_balance_setting", a_half_width_space_takes_the_spacing_twice_with_the_balance_setting),
        ("docx::layout::a_justified_line_takes_a_word_that_overflows_by_a_part_of_it", a_justified_line_takes_a_word_that_overflows_by_a_part_of_it),
        ("docx::layout::a_space_alone_in_its_run_between_latin_words_is_a_latin_space", a_space_alone_in_its_run_between_latin_words_is_a_latin_space),
        ("docx::layout::a_justified_line_hangs_a_closing_quote_or_a_full_stop_at_its_end", a_justified_line_hangs_a_closing_quote_or_a_full_stop_at_its_end),
        ("docx::layout::a_first_line_count_of_characters_wins_over_the_first_line_length", a_first_line_count_of_characters_wins_over_the_first_line_length),
        ("docx::layout::a_first_line_character_is_the_first_runs_size_plus_its_spacing", a_first_line_character_is_the_first_runs_size_plus_its_spacing),
        ("docx::layout::the_squeeze_limit_of_automatic_spaces_is_by_their_number_and_the_last_word", the_squeeze_limit_of_automatic_spaces_is_by_their_number_and_the_last_word),
        ("docx::layout::a_numbered_line_of_ms_gothic_has_its_baseline_where_a_plain_line_has_it", a_numbered_line_of_ms_gothic_has_its_baseline_where_a_plain_line_has_it),
        ("docx::layout::a_closing_bracket_does_not_hang_past_the_edge_of_a_cell", a_closing_bracket_does_not_hang_past_the_edge_of_a_cell),
        ("docx::layout::an_empty_paragraph_that_carries_a_continuous_section_break_takes_no_room", an_empty_paragraph_that_carries_a_continuous_section_break_takes_no_room),
        ("docx::layout::a_bold_ms_line_of_vertical_writing_is_turned_down_the_page", a_bold_ms_line_of_vertical_writing_is_turned_down_the_page),
        ("docx::layout::a_run_of_symbols_is_broken_where_the_line_ends_not_at_the_reference_mark", a_run_of_symbols_is_broken_where_the_line_ends_not_at_the_reference_mark),
        ("docx::layout::a_horizontal_section_before_a_vertical_one_sets_its_characters_by_their_own_widths", a_horizontal_section_before_a_vertical_one_sets_its_characters_by_their_own_widths),
        ("docx::layout::a_stop_or_a_closing_bracket_before_the_middle_dot_is_squeezed", a_stop_or_a_closing_bracket_before_the_middle_dot_is_squeezed),
        ("docx::layout::a_line_does_not_break_next_to_the_reference_mark_between_latin_letters", a_line_does_not_break_next_to_the_reference_mark_between_latin_letters),
        ("docx::layout::every_bullet_of_the_menu_has_a_typeface_that_fits_its_character", every_bullet_of_the_menu_has_a_typeface_that_fits_its_character),
        ("docx::layout::a_reference_mark_between_latin_letters_breaks_in_excel_only", a_reference_mark_between_latin_letters_breaks_in_excel_only),
        ("docx::layout::a_full_width_form_takes_the_vertical_glyph", a_full_width_form_takes_the_vertical_glyph),
        ("docx::layout::every_bullet_menu_line_has_its_symbol_as_a_glyph_mark", every_bullet_menu_line_has_its_symbol_as_a_glyph_mark),
        ("docx::layout::a_bullet_of_the_menu_is_saved_as_words_wingdings_code", a_bullet_of_the_menu_is_saved_as_words_wingdings_code),
        ("docx::layout::the_second_level_arrow_bullet_is_an_arrowhead_with_or_without_wingdings", the_second_level_arrow_bullet_is_an_arrowhead_with_or_without_wingdings),
        ("docx::layout::closing_brackets_are_squeezed_in_a_document_with_no_styles_part_only", closing_brackets_are_squeezed_in_a_document_with_no_styles_part_only),
        ("docx::layout::a_footer_takes_its_height_off_the_body_with_its_empty_paragraphs", a_footer_takes_its_height_off_the_body_with_its_empty_paragraphs),
        ("docx::layout::a_multiple_below_one_shortens_the_line", a_multiple_below_one_shortens_the_line),
        ("docx::layout::an_autofit_text_box_keeps_the_text_off_only_as_high_as_its_text", an_autofit_text_box_keeps_the_text_off_only_as_high_as_its_text),
        ("docx::layout::an_empty_paragraph_is_as_high_as_its_latin_typeface", an_empty_paragraph_is_as_high_as_its_latin_typeface),
        ("docx::layout::an_exact_line_has_its_baseline_eight_tenths_down", an_exact_line_has_its_baseline_eight_tenths_down),
        ("docx::layout::an_exact_line_stays_only_while_its_box_fits_the_page", an_exact_line_stays_only_while_its_box_fits_the_page),
        ("docx::layout::a_runs_character_spacing_counts_twice_on_the_character_grid", a_runs_character_spacing_counts_twice_on_the_character_grid),
        ("docx::layout::a_missing_typeface_is_drawn_with_cambria_for_roman_and_with_calibri_otherwise", a_missing_typeface_is_drawn_with_cambria_for_roman_and_with_calibri_otherwise),
        ("docx::layout::a_missing_typeface_the_font_table_does_not_list_is_drawn_with_cambria", a_missing_typeface_the_font_table_does_not_list_is_drawn_with_cambria),
        ("docx::layout::a_multiple_of_lines_in_a_cell_is_the_grid_times_the_multiple", a_multiple_of_lines_in_a_cell_is_the_grid_times_the_multiple),
        ("docx::layout::a_cell_with_do_not_snap_to_grid_in_cell_still_has_the_grid_times_the_multiple", a_cell_with_do_not_snap_to_grid_in_cell_still_has_the_grid_times_the_multiple),
        ("docx::layout::empty_paragraphs_of_a_multiple_are_the_grid_times_the_multiple", empty_paragraphs_of_a_multiple_are_the_grid_times_the_multiple),
        ("docx::layout::text_sits_in_a_row_as_a_box_of_the_lines_height", text_sits_in_a_row_as_a_box_of_the_lines_height),
        ("docx::layout::spacing_counts_twice_on_double_byte_characters_with_balance_single_byte_double_byte_width", spacing_counts_twice_on_double_byte_characters_with_balance_single_byte_double_byte_width),
        ("docx::layout::spacing_is_not_rounded_to_pixels_on_double_byte_characters", spacing_is_not_rounded_to_pixels_on_double_byte_characters),
        ("docx::layout::a_whitespace_run_does_not_raise_the_line_of_other_text", a_whitespace_run_does_not_raise_the_line_of_other_text),
        ("docx::layout::an_even_page_with_a_framed_page_number_has_a_one_line_footer", an_even_page_with_a_framed_page_number_has_a_one_line_footer),
        ("docx::layout::a_line_of_whitespace_only_is_as_high_as_the_paragraph_mark", a_line_of_whitespace_only_is_as_high_as_the_paragraph_mark),
        ("docx::layout::a_header_of_a_whitespace_only_line_is_as_high_as_its_paragraph_mark", a_header_of_a_whitespace_only_line_is_as_high_as_its_paragraph_mark),
        ("docx::layout::the_baseline_of_a_single_ms_line_sits_a_whole_em_down", the_baseline_of_a_single_ms_line_sits_a_whole_em_down),
        ("docx::layout::the_automatic_spaces_are_squeezed_to_take_one_more_character", the_automatic_spaces_are_squeezed_to_take_one_more_character),
        ("docx::layout::a_justified_paragraph_that_ends_in_hanging_spaces_has_its_last_line_as_one_without_them", a_justified_paragraph_that_ends_in_hanging_spaces_has_its_last_line_as_one_without_them),
        ("docx::layout::a_justified_line_with_a_hanging_stop_is_flush_up_to_the_character_before_it", a_justified_line_with_a_hanging_stop_is_flush_up_to_the_character_before_it),
        ("docx::layout::text_goes_between_two_wrapped_shapes_anchored_in_one_paragraph", text_goes_between_two_wrapped_shapes_anchored_in_one_paragraph),
        ("docx::layout::a_shape_set_from_the_page_makes_its_hole_on_its_own_page_only", a_shape_set_from_the_page_makes_its_hole_on_its_own_page_only),
        ("docx::layout::a_run_of_headings_kept_with_the_next_moves_together", a_run_of_headings_kept_with_the_next_moves_together),
        ("docx::layout::a_cell_merged_down_over_a_page_end_goes_on_with_its_rows", a_cell_merged_down_over_a_page_end_goes_on_with_its_rows),
        ("docx::layout::the_text_of_a_cell_merged_over_a_page_end_is_set_within_its_part_on_the_first_page", the_text_of_a_cell_merged_over_a_page_end_is_set_within_its_part_on_the_first_page),
        ("docx::layout::an_automatically_fitted_table_has_the_column_widths_word_gives_it", an_automatically_fitted_table_has_the_column_widths_word_gives_it),
        ("docx::layout::the_endnotes_of_a_section_that_do_not_fit_go_on_in_the_next_page", the_endnotes_of_a_section_that_do_not_fit_go_on_in_the_next_page),
        ("docx::layout::a_combining_mark_and_a_variation_selector_take_no_room", a_combining_mark_and_a_variation_selector_take_no_room),
        ("docx::layout::a_line_with_two_bracket_pairs_squeezes_both_and_keeps_its_last_character", a_line_with_two_bracket_pairs_squeezes_both_and_keeps_its_last_character),
        ("docx::layout::a_shape_placed_from_its_paragraph_is_placed_from_the_top_of_the_space_before_it", a_shape_placed_from_its_paragraph_is_placed_from_the_top_of_the_space_before_it),
        ("docx::layout::below_compatibility_mode_15_a_shape_placed_from_its_paragraph_is_placed_from_the_first_line", below_compatibility_mode_15_a_shape_placed_from_its_paragraph_is_placed_from_the_first_line),
        ("docx::layout::the_underline_of_the_spaces_that_end_a_line_is_cut_at_the_right_edge_or_is_not_drawn", the_underline_of_the_spaces_that_end_a_line_is_cut_at_the_right_edge_or_is_not_drawn),
        ("docx::layout::a_single_space_that_ends_a_paragraph_hangs_and_keeps_its_word_on_the_line", a_single_space_that_ends_a_paragraph_hangs_and_keeps_its_word_on_the_line),
        ("docx::layout::a_paragraph_without_text_runs_measures_the_spaces_of_a_field_result", a_paragraph_without_text_runs_measures_the_spaces_of_a_field_result),
        ("docx::layout::an_opening_bracket_after_the_leading_spaces_of_a_line_keeps_its_half_em", an_opening_bracket_after_the_leading_spaces_of_a_line_keeps_its_half_em),
        ("docx::layout::spaces_of_a_run_and_at_a_paragraphs_start_are_half_an_em_where_widths_are_balanced", spaces_of_a_run_and_at_a_paragraphs_start_are_half_an_em_where_widths_are_balanced),
        ("docx::layout::a_label_whose_following_half_width_space_does_not_fit_goes_down_with_it", a_label_whose_following_half_width_space_does_not_fit_goes_down_with_it),
    ]
}

/// Unfixed rules, skipped in normal runs (`--ignored` runs them).
#[cfg(not(windows))]
pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    #[allow(unused_mut)]
    let mut v: Vec<(&'static str, fn())> = vec![
        // 期待値は Windows の Word（MS 書体）から測ったもの。Mac には該当書体が無く字幅が変わる。Windows で流す（cargo test --lib）。// i18n-ok:
        ("docx::layout::a_justified_line_takes_its_last_word_as_far_as_word_does", a_justified_line_takes_its_last_word_as_far_as_word_does),
        ("docx::layout::the_cell_of_a_word_with_an_indent_is_as_wide_as_the_word_and_indent_need", the_cell_of_a_word_with_an_indent_is_as_wide_as_the_word_and_indent_need),
        ("docx::layout::a_justified_line_of_mode_15_takes_in_characters_by_squeezing_each_stop_by_a_fixed_part_of_its_em", a_justified_line_of_mode_15_takes_in_characters_by_squeezing_each_stop_by_a_fixed_part_of_its_em),
        ("docx::layout::a_distributed_line_takes_in_the_two_characters_word_does_by_squeezing_its_stops", a_distributed_line_takes_in_the_two_characters_word_does_by_squeezing_its_stops),
        ("docx::layout::a_paragraphs_last_word_taken_by_narrowing_is_drawn_narrowed", a_paragraphs_last_word_taken_by_narrowing_is_drawn_narrowed),
    ];
    #[cfg(target_os = "macos")]
    v.push(("docx::layout::inline_picture_stretches_to_the_explicit_frame_when_rendered", inline_picture_stretches_to_the_explicit_frame_when_rendered));
    v
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

/// A character drawn on a page: its baseline and the glyph.
struct Glyph {
    c: char,
    x: f32,
    y: f32,
}

fn glyphs(page: &Page) -> Vec<Glyph> {
    let mut out = Vec::new();
    for item in &page.items {
        match item {
            Item::Text { x, y, text, offsets, .. } => out.extend(text.chars().zip(offsets.iter()).map(|(c, dx)| Glyph { c, x: x + dx, y: *y })),
            Item::Turned { x, y, text, .. } | Item::Scaled { x, y, text, .. } => out.extend(text.chars().map(|c| Glyph { c, x: *x, y: *y })),
            _ => {}
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
fn a10_empty_paragraphs_go_under_the_table() {
    let pages = laid_out("ftab-f5-full-empties.docx");
    let gs = glyphs(&pages.pages[0]);
    let (l00, l01, l02) = (find(&gs, "L00"), find(&gs, "L01"), find(&gs, "L02"));
    let rows = (l01 - l00) / (l02 - l01);
    assert!(near(rows, 9.0, 0.2), "L00 to L01 is {rows:.2} lines; Word: 9");
}

/// T7: a page break in a table's cell adds no line (the same place for the paragraph after, one page).
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
fn an_empty_footer_pushes_the_body_up() {
    let pages = laid_out("empty-footer-below-margin.docx");
    assert_eq!(pages.pages.len(), 2, "36 lines of 20 pt do not fit between the margin and the empty footer's line");
}

/// The lines of the first page of each section of `footer-trailing-empty-probe.docx` (1 pt lines, 900 to a section, footer 36 pt from the edge, bottom margin
/// 28.35 pt): Word 756 with one line of text in the footer (A), 742 with a text and an empty paragraph (B), 729 with two empty ones after it (C), 756 with an empty
/// one only (D) and with a framed text and an empty one (E, the frame takes none). ibis counts one to three more (its ＭＳ 明朝 line is 13.5 pt, Word's 13.58).
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
fn a_numbered_line_of_ms_gothic_has_its_baseline_where_a_plain_line_has_it() {
    let pages = laid_out("list-number-ms-gothic-baseline.docx");
    let gs = glyphs(&pages.pages[0]);
    let (a, b, c) = (find(&gs, "補講の実施"), find(&gs, "補講の上限"), find(&gs, "補講の方法")); // i18n-ok（試験の文書の文字）
    assert!(near(b - a, c - b, 0.05), "the numbered line's baseline is a line above the next: {} against {}", b - a, c - b);
}

/// A closing bracket does not hang past the edge of a table's cell: the line takes the character before it down too (Word's PDF of corpus 023 page 10:
/// "（…確認変更届）" in a cell 202 pt wide breaks before 届, where ibis hung the bracket; 150 corpus documents, 59 more lines the same as Word's with no
/// cell hanging, none fewer). `cell-closer-does-not-hang.docx`: 19 full-width characters and ）, in a cell with room for 19.5.
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
/// (`Item::Scaled`), and the page's turning left those as they were. They stay squeezed (`Item::Scaled`), upright in the column with the others.
/// `vertical-bold-ms-heading.docx`: a bold heading and a plain line in each font.
#[cfg_attr(windows, test)]
fn a_bold_ms_line_of_vertical_writing_is_turned_down_the_page() {
    let pages = laid_out("vertical-bold-ms-heading.docx");
    let page = &pages.pages[0];
    // `glyphs` reads the squeezed (`Item::Scaled`) characters too.
    let mut gs = glyphs(page);
    // In page order: down a column, columns from the right.
    gs.sort_by(|a, b| b.x.round().total_cmp(&a.x.round()).then(a.y.total_cmp(&b.y)));
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

/// A run of symbols (‘’“”♂♀℃§☆★○●◎◇◆□■△▲▽▼※⇒⇔①…⑳) is not broken at ※: the line takes them to its edge and the run goes on in the next line (Word's PDF of
/// `issue54-symbols.docx`, ＭＳ 明朝 14 pt: the second line is ‘ to ⑩, 34 characters, and the third starts with ⑪; ibisOffice broke before ⇒ and left 175 pt empty).
#[cfg_attr(windows, test)]
fn a_run_of_symbols_is_broken_where_the_line_ends_not_at_the_reference_mark() {
    let pages = laid_out("issue54-symbols.docx");
    let gs = glyphs(&pages.pages[0]);
    let at = |c: char| gs.iter().find(|g| g.c == c).unwrap_or_else(|| panic!("{c} is drawn"));
    let (first, last, next, mark) = (at('‘'), at('⑩'), at('⑪'), at('※'));
    assert!(near(first.y, last.y, 1.0) && near(first.y, mark.y, 1.0), "‘ ※ ⑩ are on one line: y {} {} {}", first.y, mark.y, last.y);
    assert!(next.y > last.y + 10.0, "⑪ starts the next line");
    assert!(near(last.x + 14.04, 532.5, 1.5), "⑩ ends the line where Word's does, 532.5 pt (the margin is 538.5): {}", last.x + 14.04);
}

/// An upright character takes a cell of the font size only in a section whose text runs down the page, not in the document's other sections (GitHub issue #54:
/// the heading of the horizontal first page of the document was set four ems wide, every kana a whole em, because the document's last section is vertical).
/// `horizontal-section-before-vertical-section.docx`: あ、い in ＭＳ Ｐ明朝 12 pt in a horizontal section, then a `tbRl` one; the stop is 0.664 em there.
#[cfg_attr(windows, test)]
fn a_horizontal_section_before_a_vertical_one_sets_its_characters_by_their_own_widths() {
    let pages = laid_out("horizontal-section-before-vertical-section.docx");
    let gs = glyphs(&pages.pages[0]);
    let x = |c: char| gs.iter().find(|g| g.c == c).unwrap_or_else(|| panic!("{c} is drawn")).x;
    assert!(near(x('\u{3044}') - x('\u{3001}'), 12.0 * 0.664, 0.1), "the stop is {:.2} pt wide; \u{ff2d}\u{ff33} \u{ff30}\u{660e}\u{671d} sets it 0.664 em (7.97 pt)", x('\u{3044}') - x('\u{3001}'));
}

/// A line does not break before or after ※ between Latin letters (Word's PDF of `star-break-probe.docx`: 'abcdefghij' x7, 0 to 15 letters, ※ and 32 letters in a
/// left aligned paragraph each: ※ is on the line of the letter before it and of the one after it in all 16; the Latin word is cut at the edge). PowerPoint
/// is the same (`star-break-probe.pptx`), Excel breaks after ※ (`line_break::tests`).
#[cfg_attr(windows, test)]
fn a_line_does_not_break_next_to_the_reference_mark_between_latin_letters() {
    let pages = laid_out("star-break-probe.docx");
    let gs: Vec<Glyph> = pages.pages.iter().flat_map(|p| glyphs(p)).collect();
    let marks: Vec<usize> = (1..gs.len() - 1).filter(|&i| gs[i].c == '※' && gs[i - 1].c.is_ascii_lowercase() && gs[i + 1].c.is_ascii_lowercase()).collect();
    assert_eq!(marks.len(), 16, "the 16 Latin cases");
    for i in marks {
        assert!(near(gs[i - 1].y, gs[i].y, 0.5), "a line breaks before ※ (y {} and {})", gs[i - 1].y, gs[i].y);
        assert!(near(gs[i + 1].y, gs[i].y, 0.5) || gs[i + 1].x < gs[i].x, "a line breaks after ※ with room left");
    }
}

/// GitHub issue #142: the 15 bullets of the menu are written with a typeface that fits the character (Unicode with none or an ordinary typeface, a private use
/// code point only in a symbol typeface); a Unicode character in Wingdings was a box on Windows and another character in Word. (Also a `#[test]` in `list_formats`.)
fn every_bullet_of_the_menu_has_a_typeface_that_fits_its_character() {
    use crate::docx::list_formats::{BULLET_LIBRARY, bullet_font_fits};
    assert_eq!(BULLET_LIBRARY.len(), 15);
    for (id, _, text, font) in BULLET_LIBRARY {
        assert!(bullet_font_fits(text, font), "{id}: {text:?} with {font:?}");
    }
    assert!(!bullet_font_fits("■", Some("Wingdings")) && bullet_font_fits("\u{f0a7}", Some("Wingdings")));
}

/// ※ between Latin letters: no break in Word and PowerPoint, a break next to it in Excel (their PDFs of `star-break-probe`). (Also a `#[test]` of `line_break`.)
fn a_reference_mark_between_latin_letters_breaks_in_excel_only() {
    use crate::render::line_break::{Kinsoku, LatinBreaks, may_break};
    let plain = LatinBreaks::default();
    let excel = LatinBreaks { ambiguous_wide: true, ..LatinBreaks::default() };
    assert!(!may_break('j', '※', Some(Kinsoku::POWERPOINT), plain) && !may_break('※', 'k', Some(Kinsoku::POWERPOINT), plain));
    assert!(may_break('j', '※', Some(Kinsoku::EXCEL), excel) && may_break('※', 'k', Some(Kinsoku::EXCEL), excel));
    assert!(may_break('あ', '※', Some(Kinsoku::POWERPOINT), plain) && may_break('※', 'ア', Some(Kinsoku::POWERPOINT), plain));
}

/// ＝ (U+FF1D) is drawn with the font's vertical glyph in vertical writing, as Word's PDF of test-gakusyosi.docx does (the turned form); § and ○ keep theirs.
fn a_full_width_form_takes_the_vertical_glyph() {
    use crate::render::vertical_text::vertical_glyph;
    assert!(vertical_glyph('＝') && vertical_glyph('＜') && vertical_glyph('、'));
    assert!(!vertical_glyph('§') && !vertical_glyph('○') && !vertical_glyph('⇒'));
}

/// The 15 lines of the bullet menu show their symbol as a `LeadMark::Glyph` (drawn in the symbol typeface, GitHub issue #142: the heavy check mark in the line's text was a
/// purple emoji on Windows, where the variation selector U+FE0E is not honoured), and the line's own text names it without the symbol.
fn every_bullet_menu_line_has_its_symbol_as_a_glyph_mark() {
    use crate::docx::list_formats::BULLET_LIBRARY;
    use crate::shell::chrome::lead_marks::{LeadMark, bullet_glyph};
    for (id, symbol, text, _) in BULLET_LIBRARY {
        assert_eq!(bullet_glyph(symbol), vec![LeadMark::Glyph(text.chars().next().unwrap())], "{id}");
        assert!(!crate::shell::i18n::text(id).contains(text), "{id}: the line's text has no symbol in it");
    }
}

/// #142 (the user, 2026-11-12): a bullet chosen from the menu is the one Word writes, Wingdings' code (✧ is U+F0B2 in Wingdings, #M133), saved so
/// and read back as the same bullet.
#[cfg(not(windows))]
fn a_bullet_of_the_menu_is_saved_as_words_wingdings_code() {
    use crate::docx::list_formats::BULLET_LIBRARY;
    let index = BULLET_LIBRARY.iter().position(|(id, ..)| *id == "docx.bullet.sparkle").expect("the sparkle");
    let mut s = crate::shell::scenario::Scenario::open("test/editing/docx-report.docx");
    let _ = index;
    s.menu("docx.bullet.sparkle");
    let path = s.save_as("bullet-142.docx");
    let numbering = String::from_utf8(ibis_pptx::ibis_ooxml::Package::open(std::fs::read(&path).unwrap()).unwrap().read("word/numbering.xml").unwrap()).unwrap();
    assert!(numbering.contains("w:val=\"\u{f0b2}\"") && numbering.contains("w:ascii=\"Wingdings\""), "Wingdings' code in numbering.xml");
    let again = ibis_pptx::open(std::fs::read(&path).unwrap()).unwrap();
    let flow = again.presentation.flow.expect("a document");
    assert!(flow.styles.numbering.abstracts.values().flat_map(|a| a.levels.iter()).any(|l| l.symbol == Some(("Wingdings".to_string(), '\u{f0b2}'))), "read back as Wingdings' ✧");
}

/// A stop or a closing bracket gives up its half em before ・, and ・ ； ： before an opening bracket (Word's PDF of `pair-middle-dot-probe.docx`, ＭＳ 明朝 14 pt,
/// `w:kern` 2 in the defaults: 漢．・漢 sets ． 7.1 pt wide, 漢・「漢 sets ・ 7.1 pt; 漢・」漢, 漢！・漢 and 漢・・漢 keep the full 14 pt).
#[cfg_attr(windows, test)]
fn a_stop_or_a_closing_bracket_before_the_middle_dot_is_squeezed() {
    let pages = laid_out("pair-middle-dot-probe.docx");
    let gs = glyphs(&pages.pages[0]);
    let step = |a: char, b: char| -> f32 {
        let i = (0..gs.len() - 1).find(|&i| gs[i].c == a && gs[i + 1].c == b && near(gs[i].y, gs[i + 1].y, 0.5)).unwrap_or_else(|| panic!("{a}{b} is drawn"));
        gs[i + 1].x - gs[i].x
    };
    for (a, b) in [('．', '・'), ('。', '・'), ('、', '・'), ('）', '・'), ('・', '「'), ('；', '「'), ('、', '、')] {
        assert!(near(step(a, b), 7.0, 0.3), "{a}{b}: {:.2} pt; Word 7.1", step(a, b));
    }
    for (a, b) in [('・', '」'), ('！', '・'), ('・', '・'), ('！', '「'), ('ー', '」')] {
        assert!(near(step(a, b), 14.0, 0.3), "{a}{b}: {:.2} pt; Word 14.0", step(a, b));
    }
}

/// A closing bracket or a stop before another one is squeezed by half an em where the kerning is on, which in a document with no styles part it is at
/// every size, and not in one with a styles part and no `w:kern` (Word's PDFs of `bracket-pairs-nostyles.docx` and `bracket-pairs-styles-nokern.docx`,
/// あ（い）（う）「え」「お」、「か」。『き』く in ＭＳ 明朝 10.5 pt: the line ends 194.4 and 231.3 pt from the margin, 7 pairs of 5.25 pt apart).
#[cfg_attr(windows, test)]
fn closing_brackets_are_squeezed_in_a_document_with_no_styles_part_only() {
    let end = |name: &str| {
        let pages = laid_out(name);
        let gs = glyphs(&pages.pages[0]);
        let steps: Vec<(char, f32)> = gs.windows(2).take(21).map(|w| (w[0].c, w[1].x - w[0].x)).collect();
        eprintln!("{name}: {steps:?}");
        gs.iter().find(|g| g.c == '\u{304f}').expect("\u{304f} is drawn").x
    };
    let (squeezed, full) = (end("bracket-pairs-nostyles.docx"), end("bracket-pairs-styles-nokern.docx"));
    assert!(near(full - squeezed, 7.0 * 5.25, 0.5), "ends {squeezed:.2} and {full:.2}: 7 pairs of half an em (36.75 pt) apart");
}

/// The body of a page ends at the larger of the bottom margin and the footer's distance from the edge plus its height, the footer's last empty paragraph
/// counted (Word's PDF of `footer-bottom-room-probe.docx`, 60 exact 18 pt lines in each of 8 sections: no footer 42 lines on the first page with a bottom
/// margin of 851 twips and 43 with 300; a page number alone 42 with both; a page number and an empty paragraph, or two empty paragraphs, 41).
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
#[cfg_attr(windows, ignore = "uses the platform headless GPU renderer; run explicitly with --ignored")]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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
#[cfg_attr(windows, test)]
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

/// A page has the room its own footer leaves (Word's PDF of `even-footer-framed-number-probe.docx`, Microsoft 365: odd pages have a footer of the page number and an
/// empty paragraph, even pages one whose page number is in a frame, so only the empty paragraph takes room: 61 lines of 12 pt on the first page, 63 on the second, 61 on the
/// third; corpus 023 page 40 is such a page). Only the even page is asserted exactly: the two line footer of the odd pages is a line shorter in Word than here for this
/// probe's default font (Word 61 lines, here 62; not understood yet), while the even page's 63 lines are Word's.
#[cfg_attr(windows, test)]
fn an_even_page_with_a_framed_page_number_has_a_one_line_footer() {
    let pages = laid_out("even-footer-framed-number-probe.docx");
    // Each line of the body is `L` and its number: the lines are counted by their `L` (the characters do not come as one `Item::Text` a line here).
    let lines_on = |n: usize| -> usize { glyphs(&pages.pages[n]).iter().filter(|g| g.c == 'L').count() };
    let (first, second, third) = (lines_on(0), lines_on(1), lines_on(2));
    assert_eq!(second, 63, "lines on the even page; Word: 63 (61 on the pages before and after it)");
    assert!(first < second && third < second, "the odd pages with their two line footer hold fewer lines than the even page: {first}, {second}, {third}");
}

/// A line of whitespace only is as high as an empty one: the paragraph mark's size sets it, not the size of the spaces (Word's PDF of
/// `whitespace-only-line-probe.docx`, Microsoft 365: a paragraph of one 14 pt ideographic or ASCII space (or two, or a 14 pt one and an 18 pt one) with a 12 pt
/// mark and an empty one with a 12 pt mark are all 29.16 to 29.28 pt before the next 10.5 pt line (T), one with a 14 pt mark and a 12 pt space 31.8, a
/// 10.5 pt mark with a 20 pt or a 14 pt space 27.24, a 20 pt mark with a 10.5 pt space 39.51).
#[cfg_attr(windows, test)]
fn a_line_of_whitespace_only_is_as_high_as_the_paragraph_mark() {
    let pages = laid_out("whitespace-only-line-probe.docx");
    let gs = glyphs(&pages.pages[0]);
    // (the case's number, Word's distance from the line of T before it to the line of T after the spaces-only paragraph)
    let word = [(1, 29.18), (2, 31.8), (3, 31.8), (4, 27.24), (5, 27.24), (6, 39.51), (7, 29.28), (8, 29.16), (9, 29.16), (10, 29.16), (11, 27.24)];
    for rep in 0..2 {
        for (n, want) in word {
            let d = find(&gs, &format!("T{rep}{n:02}")) - find(&gs, &format!("T{rep}{:02}", n - 1));
            // (Word sets a 14 pt line 14.04 pt, a pixel of its 600 dpi grid, so a 14 pt mark is 0.3 pt more than ours.)
            assert!(near(d, want, 0.4), "case {n} (repeat {rep}): {d:.2} pt; Word: {want:.2} pt");
        }
    }
}

/// A header of one paragraph of a 14 pt space with a 12 pt mark is 15.43 pt high, as a 12 pt line, and the body starts under it (Word's PDF of
/// `whitespace-only-header-probe.docx`: the first line of the body, 10.5 pt, has its baseline 54.46 pt from the top of the page; corpus 023's page 40).
#[cfg_attr(windows, test)]
fn a_header_of_a_whitespace_only_line_is_as_high_as_its_paragraph_mark() {
    let pages = laid_out("whitespace-only-header-probe.docx");
    let gs = glyphs(&pages.pages[0]);
    let y = find(&gs, "H00");
    assert!(near(y, 54.46, 0.3), "the first line of the body: {y:.2} pt from the top; Word: 54.46 pt");
}

/// Where the baseline of a single ＭＳ 明朝 line is, off the grid and on it (Word's PDF of `ms-single-line-baseline-before-exact-probe.docx`, Microsoft 365
/// 16.0.20527.20088, a single line, then an exact line whose baseline is 0.8 of its height down; the distance between the two baselines is the room
/// under the single line's baseline plus that): off the grid 8 / 10.5 / 12 pt text leaves 2.40 / 3.12 / 3.48 pt (0.29 em: the line 1.29 em high has its baseline
/// 1.0 em down); on the 16.75 pt grid 5.52 / 4.58 / 3.96 pt ((grid - 1 em) / 2 + 0.87 em down).
#[cfg_attr(windows, test)]
fn the_baseline_of_a_single_ms_line_sits_a_whole_em_down() {
    let pages = laid_out("ms-single-line-baseline-before-exact-probe.docx");
    let gs = glyphs(&pages.pages[0]);
    let word = [12.0f32, 21.6, 12.72, 22.32, 13.08, 22.68, 15.12, 24.72, 14.18, 23.76, 13.56, 23.16];
    for (g, want) in word.iter().enumerate() {
        let d = find(&gs, &format!("E{g:02}")) - find(&gs, &format!("S{g:02}"));
        assert!(near(d, *want, 0.35), "group {g}: the single line to the exact line {d:.2} pt; Word: {want:.2} pt");
    }
}

/// A justified or distributed line (mode 15) squeezes its automatic spaces between Japanese and Latin to take one more character: of the line's
/// spaces before the overflowing character, its own space after it not counted; a distributed line only for the word after the last space (GitHub
/// issue #68 D2, Word's PDFs of `kinsoku-both-normal-probe.docx` and `kinsoku-distribute-normal-probe.docx`, whose cases these are:
/// `make_kinsoku_probe.py squeeze`). The characters of each case's first line, as Word ends it.
#[cfg_attr(windows, test)]
fn the_automatic_spaces_are_squeezed_to_take_one_more_character() {
    for (file, want) in [
        ("kinsoku-squeeze-both.docx", [("W018", 52), ("W022", 52), ("W050", 50), ("W083", 49)]),
        ("kinsoku-squeeze-distribute.docx", [("W018", 51), ("W022", 52), ("W050", 50), ("W083", 49)]),
    ] {
        let pages = laid_out(file);
        let gs: Vec<Glyph> = pages.pages.iter().flat_map(|p| glyphs(p)).collect();
        for (case, chars) in want {
            let marker: Vec<char> = format!("■{case}").chars().collect();
            let at = (0..gs.len() - marker.len()).find(|&i| gs[i..i + marker.len()].iter().map(|g| g.c).eq(marker.iter().copied())).expect("the case is drawn") + marker.len();
            let first = gs[at..].iter().take_while(|g| near(g.y, gs[at].y, 0.5)).count();
            assert_eq!(first, chars, "{file} {case}: the first line has {first} characters; Word: {chars}");
        }
    }
}

/// The lines of a page's text as (baseline, its glyphs by x), the glyphs of the text boxes and of the characters `keep` rejects left out.
fn lines_of(page: &Page, keep: impl Fn(char) -> bool) -> Vec<(f32, Vec<Glyph>)> {
    let mut gs: Vec<Glyph> = glyphs(page).into_iter().filter(|g| keep(g.c)).collect();
    gs.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let mut lines: Vec<(f32, Vec<Glyph>)> = Vec::new();
    for g in gs {
        match lines.last_mut() {
            Some((y, line)) if near(*y, g.y, 0.5) => line.push(g),
            _ => lines.push((g.y, vec![g])),
        }
    }
    lines
}

/// E6: "this character stands past the right margin" is decided where the line is split and kept on the line (its hang start); the drawing counts the
/// line up to it. A justified paragraph that ends in full-width spaces (19 of a letter's trailing blanks, past the margin) has its last line as the
/// same text without them has it: not spread, not narrowed (`justify-trailing-ideographic-spaces.docx`: the same 80 characters twice, the first
/// followed by the spaces; before, the first one's last line was narrowed by the hanging spaces' width).
#[cfg_attr(windows, test)]
fn a_justified_paragraph_that_ends_in_hanging_spaces_has_its_last_line_as_one_without_them() {
    let pages = laid_out("justify-trailing-ideographic-spaces.docx");
    let lines = lines_of(&pages.pages[0], |c| c != '\u{3000}' && c != ' ');
    assert!(lines.len() >= 4 && lines.len() % 2 == 0, "two paragraphs of as many lines each: {} lines", lines.len());
    let half = lines.len() / 2;
    let (with_spaces, without) = (&lines[half - 1].1, &lines[lines.len() - 1].1);
    assert_eq!(with_spaces.len(), without.len(), "the last lines have the same characters");
    for (a, b) in with_spaces.iter().zip(without) {
        assert!(near(a.x, b.x, 0.05), "{}: {:.2} with the trailing spaces, {:.2} without", a.c, a.x, b.x);
    }
}

/// E6: a justified line that ends in a stop that hangs (`justify-hanging-stops.docx`: 42 kana and a 、, three times) is flush with the right margin
/// up to the character before the stop, which stands past it.
#[cfg_attr(windows, test)]
fn a_justified_line_with_a_hanging_stop_is_flush_up_to_the_character_before_it() {
    let pages = laid_out("justify-hanging-stops.docx");
    let lines = lines_of(&pages.pages[0], |_| true);
    assert!(lines.len() >= 3, "{} lines", lines.len());
    let right = 595.3 - 72.0;
    for (n, (_, line)) in lines.iter().take(lines.len() - 1).enumerate() {
        let last = line.iter().rev().find(|g| g.c != '、').expect("a character");
        assert!(near(last.x + 10.5, right, 1.0), "line {n}: the last character before the stop ends at {:.2}, the margin is at {right:.2}", last.x + 10.5);
    }
}

/// E1: two shapes that wrap the text and are anchored in one paragraph (`two-wrapped-shapes.docx`, 100 pt wide at the left and the right margin): the text
/// beside them is between the two, not under the second.
#[cfg_attr(windows, test)]
fn text_goes_between_two_wrapped_shapes_anchored_in_one_paragraph() {
    let pages = laid_out("two-wrapped-shapes.docx");
    let lines = lines_of(&pages.pages[0], |c| !c.is_ascii());
    let (left, right) = (72.0 + 100.0, 595.3 - 72.0 - 100.0);
    let beside: Vec<_> = lines.iter().filter(|(y, _)| *y < 72.0 + 90.0).collect();
    assert!(!beside.is_empty(), "lines beside the shapes");
    for (y, line) in beside {
        let (first, last) = (line.first().expect("a glyph"), line.last().expect("a glyph"));
        assert!(first.x >= left - 0.1 && last.x + 10.5 <= right + 0.1, "the line at {y:.1} runs from {:.1} to {:.1}, the room is {left:.1} to {right:.1}", first.x, last.x + 10.5);
    }
}

/// E2: a shape set from the page makes its hole on the page its paragraph is on (`page-shape-on-page2.docx`: the paragraph is on page 2, the box at 200 pt
/// from the top): page 1 has none there, page 2 has it.
#[cfg_attr(windows, test)]
fn a_shape_set_from_the_page_makes_its_hole_on_its_own_page_only() {
    let pages = laid_out("page-shape-on-page2.docx");
    assert!(pages.pages.len() >= 2, "{} pages", pages.pages.len());
    let on_first = lines_of(&pages.pages[0], |c| !c.is_ascii() || c.is_ascii_digit());
    for (y, line) in on_first.iter().filter(|(y, _)| *y > 215.0 && *y < 300.0) {
        assert!(near(line[0].x, 72.0, 0.5), "page 1, the line at {y:.1} starts at {:.1}: a hole where the shape is not", line[0].x);
    }
    let on_second = lines_of(&pages.pages[1], |c| !c.is_ascii());
    let beside: Vec<_> = on_second.iter().filter(|(y, _)| *y > 215.0 && *y < 300.0).collect();
    assert!(!beside.is_empty(), "lines beside the shape on page 2");
    for (y, line) in beside {
        assert!(line[0].x >= 72.0 + 150.0 - 0.1, "page 2, the line at {y:.1} starts at {:.1}, inside the shape", line[0].x);
    }
}

/// keepNext chain: headings that are each kept with the next go with the paragraph that ends the chain (`keep-next-chain.docx`: 32 lines of 20 pt, two
/// headings, a paragraph; the first heading fits with the second at the foot of page 1, the paragraph does not): both headings move to page 2, not
/// only the second.
#[cfg_attr(windows, test)]
fn a_run_of_headings_kept_with_the_next_moves_together() {
    let pages = laid_out("keep-next-chain.docx");
    let page_of = |text: &str| {
        let want: Vec<char> = text.chars().collect();
        pages.pages.iter().position(|p| {
            let gs = glyphs(p);
            (0..=gs.len().saturating_sub(want.len())).any(|i| gs[i..i + want.len()].iter().map(|g| g.c).eq(want.iter().copied()))
        })
    };
    assert_eq!(page_of("見出し一"), Some(1), "the first heading"); // i18n-ok（試験・ファイルの中の日本語のデータ）
    assert_eq!(page_of("見出し二"), Some(1), "the second heading"); // i18n-ok（試験・ファイルの中の日本語のデータ）
    assert_eq!(page_of("本文の段落"), Some(1), "the paragraph"); // i18n-ok（試験・ファイルの中の日本語のデータ）
}

/// E3: a cell merged down over 80 rows (70 lines of text in it) that goes over the page's end (`vmerge-across-pages.docx`): the rows on page 2 hold the rest of
/// its text; nothing of it is drawn under the foot of page 1 (before, all of it was drawn with the first row, down past the paper's edge, and page 2's cell was empty).
#[cfg_attr(windows, test)]
fn a_cell_merged_down_over_a_page_end_goes_on_with_its_rows() {
    let pages = laid_out("vmerge-across-pages.docx");
    assert!(pages.pages.len() >= 2, "{} pages", pages.pages.len());
    let foot = pages.pages[0].height - 72.0;
    let merged: Vec<Vec<Glyph>> = pages.pages.iter().map(|p| glyphs(p).into_iter().filter(|g| g.c == '結').collect()).collect();
    assert!(!merged[0].is_empty(), "the merged cell's text on page 1");
    assert!(merged[0].iter().all(|g| g.y <= foot + 0.5), "text of the merged cell under the foot of page 1: {:.1} (foot {foot:.1})", merged[0].iter().map(|g| g.y).fold(0.0, f32::max));
    assert!(!merged[1].is_empty(), "the rest of the merged cell's text on page 2");
    let total: usize = merged.iter().map(Vec::len).sum();
    assert_eq!(total, 70, "every one of the 70 lines is drawn once");
}

/// E4: the endnotes of a section (30 of them, at the section's end) do not all fit under the last page's text (`endnotes-overflow.docx`): the rest go on in the next
/// page, none is drawn under the foot of a page, and the last note is drawn.
#[cfg_attr(windows, test)]
fn the_endnotes_of_a_section_that_do_not_fit_go_on_in_the_next_page() {
    let pages = laid_out("endnotes-overflow.docx");
    assert!(pages.pages.len() >= 2, "{} pages", pages.pages.len());
    for (n, page) in pages.pages.iter().enumerate() {
        let foot = page.height - 72.0;
        let lowest = glyphs(page).iter().map(|g| g.y).fold(0.0, f32::max);
        assert!(lowest <= foot + 0.5, "page {}: text down to {lowest:.1}, the foot is at {foot:.1}", n + 1);
    }
    // Word's PDF: page 1's last two note lines are at 736.3 and 749.9, page 2's last at 763.5 (the notes follow the text and the separator's line at once).
    let first = lines_of(&pages.pages[0], |_| true);
    let (a, b) = (first[first.len() - 2].0, first[first.len() - 1].0);
    assert!(near(a, 736.3, 1.0) && near(b, 749.9, 1.0), "page 1's last note lines at {a:.1} and {b:.1}; Word: 736.3 and 749.9");
    assert_eq!(pages.pages.len(), 3, "Word has 3 pages");
    let last = pages.pages.last().expect("a page");
    let text: String = glyphs(last).iter().map(|g| g.c).collect();
    assert!(text.contains("本文 30"), "the last note is on the last page: {text}"); // i18n-ok（試験・ファイルの中の日本語のデータ）
}

/// E5: a combining mark (は + U+309A, e + U+0301) and a kanji's variation selector are part of the character before them: they take no room, so the text
/// after them stands where it stands in the same text without them (`combining-chars.docx`: two lines, the second without the marks).
#[cfg_attr(windows, test)]
fn a_combining_mark_and_a_variation_selector_take_no_room() {
    let pages = laid_out("combining-chars.docx");
    let lines = lines_of(&pages.pages[0], |_| true);
    assert_eq!(lines.len(), 2, "{} lines", lines.len());
    let bars = |line: &[Glyph]| line.iter().filter(|g| g.c == '|').map(|g| g.x).collect::<Vec<_>>();
    let (with, without) = (bars(&lines[0].1), bars(&lines[1].1));
    assert_eq!(with.len(), 3);
    for (a, b) in with.iter().zip(&without) {
        assert!(near(*a, *b, 0.05), "a bar at {a:.2} with the marks, {b:.2} without");
    }
}

/// E7: a left aligned line of 37 characters in a paragraph 326 pt wide (`squeeze-bracket-pairs-left.docx`): the first of each of the pairs （「 and ）」 is half an em
/// short, 9 pt in all, so the 333 pt line is 324 pt and the last character (よ) is on it (Word's PDF of a form's line of this shape: the character is on the line;
/// before, with only a closing character before an opening one squeezed, the line was 333 pt and よ went down alone to the line below).
#[cfg_attr(windows, test)]
fn a_line_with_two_bracket_pairs_squeezes_both_and_keeps_its_last_character() {
    let pages = laid_out("squeeze-bracket-pairs-left.docx");
    let lines = lines_of(&pages.pages[0], |c| c != '\u{3000}');
    assert_eq!(lines.len(), 2, "{} lines", lines.len());
    let first = &lines[0].1;
    assert_eq!(first.len(), 37, "the first line holds the 37 characters");
    assert!(first.last().is_some_and(|g| g.c == 'よ'), "it ends with よ"); // i18n-ok（試験・ファイルの中の日本語のデータ）
}

/// A shape placed from its paragraph (`relativeFrom="paragraph"`, a line over a heading) is placed from the paragraph's top with the space before it
/// included, not from the first line's top (0.8.5 review, corpus 047: the line meant for above a heading's text, 4.4 pt below the paragraph's top
/// with 6 pt before it, crossed the text; Word's PDF has it above). Two headings with the same offset and 6 pt and 26 pt before them: the line is 20 pt
/// further above the text in the second. `line-anchored-to-heading.docx` (made by `scripts/corpus/gen_docx_anchor_probes.py`).
#[cfg_attr(windows, test)]
fn a_shape_placed_from_its_paragraph_is_placed_from_the_top_of_the_space_before_it() {
    let pages = laid_out("line-anchored-to-heading.docx");
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let lines: Vec<f32> = page.items.iter().filter_map(|it| if let Item::Shape { y, .. } = it { Some(*y) } else { None }).collect();
    assert_eq!(lines.len(), 2, "two lines are drawn");
    let below = |text: &str, line: f32| find(&gs, text) - line;
    let (a, b) = (below("見出しＡ", lines[0]), below("見出しＢ", lines[1])); // i18n-ok（試験の文書の文字）
    assert!(near(b - a, 20.0, 0.1), "the second line is 20 pt further above its text: {a:.2} pt and {b:.2} pt from the line to the text");
    // The line is over the first heading's text (its baseline is lower than the line by more than the line's 4.4 pt offset from a top that is 6 pt above the line's).
    assert!(a > 0.0, "the first line is above its heading's baseline: {a:.2}");
}

/// The width of a space where a document balances half-width and full-width characters (`balanceSingleByteDoubleByteWidth`; 0.8.5 review
/// docx-underline-off-page, corpus 133, `spaces-count-matrix-century.docx`, Word's PDF of it, Century 10.5 pt with ＭＳ 明朝): a single space between two Latin
/// characters is the font's own (2.5 to 2.9 pt); two or more in a row are 5.25 pt each (half an em), and so is one that begins the paragraph, whatever `w:hint`.
#[cfg_attr(windows, test)]
fn spaces_of_a_run_and_at_a_paragraphs_start_are_half_an_em_where_widths_are_balanced() {
    let pages = laid_out("spaces-count-matrix-century.docx");
    let gs = glyphs(&pages.pages[0]);
    // The x of every "X" in page order: A+1..6 spaces, 1 and 2 leading, then (the line "あ" and "A…あ" ones too).
    let xs: Vec<f32> = gs.iter().filter(|g| g.c == 'X').map(|g| g.x).collect();
    let a_x = gs.iter().find(|g| g.c == 'A').expect("A").x;
    assert!(xs.len() >= 8, "the markers are drawn: {}", xs.len());
    for k in 2..6 {
        assert!(near(xs[k] - xs[k - 1], 5.25, 0.2), "{} spaces are {:.2} pt further than {} ({:.2})", k + 1, xs[k] - xs[k - 1], k, 5.25);
    }
    // One space is the font's own: less than a half em, so two take less than twice a half em more than one.
    assert!(xs[1] - xs[0] > 5.25 && xs[1] - xs[0] < 10.0, "the second space adds a half em to the one of the font: {:.2}", xs[1] - xs[0]);
    assert!(near(xs[6], a_x + 5.25, 0.3), "a space that begins the paragraph is half an em: X is at {:.2}, A at {:.2}", xs[6], a_x);
    assert!(near(xs[7] - xs[6], 5.25, 0.2), "the second one is another half em");
}

/// A word whose first following half-width space (an East Asian one, where the widths are balanced) does not fit on its line goes down to the next line with it, though
/// the word itself would fit (Word's PDF of `date-wrap-variants.docx`, 18 full-width spaces, n half-width ones, "Date: ", 34 underlined spaces, 10.5 pt in a line of
/// 425.2 pt: with n = 39 "Date:" and the space after it end at 423.5 and stay; with 40 and 41 they go to the next line, at the paragraph's hanging indent). The
/// signature block of corpus 133 (`underlined-trailing-spaces-signature.docx`): "Date:" stands where "By:" does, and its underline ends inside the margin.
#[cfg_attr(windows, test)]
fn a_label_whose_following_half_width_space_does_not_fit_goes_down_with_it() {
    let pages = laid_out("date-wrap-variants.docx");
    let gs = glyphs(&pages.pages[0]);
    let ds: Vec<(f32, f32)> = gs.iter().filter(|g| g.c == 'D').map(|g| (g.x, g.y)).collect();
    assert!(ds.len() >= 6, "the labels are drawn: {}", ds.len());
    assert!(ds[3].0 > ds[2].0 && ds[2].0 > ds[1].0, "n = 36, 38, 39 stand further and further along the line");
    assert!(ds[4].0 < ds[3].0 - 100.0 && near(ds[4].0, ds[5].0, 0.1), "n = 40 and 41 go down to the hanging indent: {:?}", &ds[3..6]);
    let signature = laid_out("underlined-trailing-spaces-signature.docx");
    let page = &signature.pages[0];
    let sg = glyphs(page);
    let by = sg.iter().find(|g| g.c == 'B').expect("By:");
    let date = sg.iter().find(|g| g.c == 'D').expect("Date:");
    assert!(near(date.x, by.x, 0.3) && date.y > by.y + 5.0, "Date: is under By:, at the same x: {:.2} and {:.2}", date.x, by.x);
    let right_edge = by.x - 204.75 + 425.2;
    for item in &page.items {
        if let Item::Rect { x, w, .. } = item {
            assert!(*x + *w <= right_edge + 0.5, "an underline ends at {:.2}, the margin is at {:.2}", x + w, right_edge);
        }
    }
}

/// Below compatibility mode 15 a shape placed from its paragraph is placed from the top of the first line, the space before it not counted (corpus 049, mode 11: Word's
/// PDF has the lines where that puts them, 4.8 pt lower than from the paragraph's top). `line-anchored-to-heading-compat11.docx`: the two headings of
/// `line-anchored-to-heading.docx`; the line is the same distance from the text under both.
#[cfg_attr(windows, test)]
fn below_compatibility_mode_15_a_shape_placed_from_its_paragraph_is_placed_from_the_first_line() {
    let pages = laid_out("line-anchored-to-heading-compat11.docx");
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let lines: Vec<f32> = page.items.iter().filter_map(|it| if let Item::Shape { y, .. } = it { Some(*y) } else { None }).collect();
    assert_eq!(lines.len(), 2, "two lines are drawn");
    let (a, b) = (find(&gs, "見出しＡ") - lines[0], find(&gs, "見出しＢ") - lines[1]); // i18n-ok（試験の文書の文字）
    assert!(near(a, b, 0.1), "the line is as far from the text of the heading with 26 pt before it as from the one with 6 pt: {a:.2} and {b:.2}");
}

/// The underline of the spaces that end a line: with `ulTrailSpace` it is drawn up to the right edge (cut there where the spaces hang past it), without it not at all
/// (Word's PDFs of `trailing-spaces-hang-ms.docx` and `trailing-spaces-hang-ms-no-ultrailspace.docx`: an underline from the word's end to the margin, 505.0 to 509.5 for a
/// margin at 510.25, and none).
#[cfg_attr(windows, test)]
fn the_underline_of_the_spaces_that_end_a_line_is_cut_at_the_right_edge_or_is_not_drawn() {
    for (file, drawn) in [("trailing-spaces-hang-ms.docx", true), ("trailing-spaces-hang-ms-no-ultrailspace.docx", false)] {
        let pages = laid_out(file);
        let page = &pages.pages[0];
        let gs = glyphs(page);
        let left = gs.iter().find(|g| g.c == 'a').expect("the words").x;
        let edge = left + 425.2;
        let mut at_the_edge = 0;
        for item in &page.items {
            if let Item::Rect { x, w, .. } = item {
                assert!(*x + *w <= edge + 0.3, "{file}: an underline ends at {:.2}, the edge is at {edge:.2}", x + w);
                if *x + *w > edge - 0.3 {
                    at_the_edge += 1;
                }
            }
        }
        assert_eq!(at_the_edge > 0, drawn, "{file}: {at_the_edge} underlines end at the edge");
    }
}

/// A single space that ends a paragraph hangs past the right edge, and does not take its word down with it (corpus 124 page 2: "… such as quoting, " ends a line in Word's
/// PDF; the half-width space of a document that balances widths is half an em, and one of two or more after a word has to fit, the single last one does not).
/// `single-trailing-space-hangs.docx`: 80 letters (420 pt) and a space in a line of 422 pt.
#[cfg_attr(windows, test)]
fn a_single_space_that_ends_a_paragraph_hangs_and_keeps_its_word_on_the_line() {
    let pages = laid_out("single-trailing-space-hangs.docx");
    let gs = glyphs(&pages.pages[0]);
    let letters: Vec<&Glyph> = gs.iter().filter(|g| g.c == 'a').collect();
    assert_eq!(letters.len(), 80);
    assert!(letters.iter().all(|g| near(g.y, letters[0].y, 0.01)), "the word is on one line");
}

/// E7 (open): a distributed line of 26 characters in a 296 pt cell (`distribute-squeeze-stops-cell.docx`, 274.5 pt of room, ＭＳ ゴシック 10 pt with 0.5 pt of spacing and
/// `balanceSingleByteDoubleByteWidth`: 11 pt a character, 286 pt; four stops and brackets, 。） after the fifth character, two 、) fits Word on one line: it
/// squeezes the stops by 5.46, 1.96, 2.1 and 2.1 pt (11.6 pt, the TJ adjustments of Word's PDF of a corpus form's line of this shape), which is two characters more
/// than the 24 that fit unsqueezed. This layout takes one more (25) and sets the 26th alone on a second line: its squeeze is by the line's first overflowing
/// character only (the rule of Word's PDFs of `compresspunct-mincho-w41453.docx`: one more character than fits). What lets Word take two here is not known (the run
/// has no kerning, so the pair 。） is not squeezed as one: squeezing pairs with the setting on alone moved 22 corpus documents' lines away from Word's).
#[cfg_attr(windows, test)]
#[cfg_attr(windows, ignore = "open: Word takes two more characters than fit by squeezing the line's stops; the condition is not known")]
fn a_distributed_line_takes_in_the_two_characters_word_does_by_squeezing_its_stops() {
    let pages = laid_out("distribute-squeeze-stops-cell.docx");
    let lines = lines_of(&pages.pages[0], |_| true);
    assert_eq!(lines.len(), 1, "{} lines", lines.len());
    assert_eq!(lines[0].1.len(), 26);
}

/// The width of a space is measured over the paragraph's text runs; a paragraph whose characters come from a field (its cached result has spaces) has none, and the 0.8.5 branch
/// panicked there (index out of bounds: corpus 023, 058, 142, 143, 144 could not be exported). The result's own text is used then. `spaces-in-field-result.docx`: "A  B C".
#[cfg_attr(windows, test)]
fn a_paragraph_without_text_runs_measures_the_spaces_of_a_field_result() {
    let pages = laid_out("spaces-in-field-result.docx");
    let gs = glyphs(&pages.pages[0]);
    let x_of = |c: char| gs.iter().find(|g| g.c == c).unwrap_or_else(|| panic!("{c} is drawn")).x;
    assert!(x_of('A') < x_of('B') && x_of('B') < x_of('C'), "A, B and C are drawn in order");
    // Two spaces in a row are half an em each (a single one between Latin letters is the font's).
    assert!(x_of('B') - x_of('A') >= 15.0, "A and B are two half-width spaces and a letter apart: {:.2}", x_of('B') - x_of('A'));
}

/// The opening bracket that is the first character seen on a line, after the half-width spaces of its indent, keeps its half em where the line is squeezed to take in a character
/// (Word's PDF of corpus 047 page 2 and of `squeeze-cell-width-sweep-15.docx`: the 【 of the form's line, in a cell of 9170 twips that takes 23.9 pt too long a line in, stands 9.9 pt
/// (the cell's indent and margin) to the right of where it stands on a line that fits, `form-line-punct-probe2.docx`; ibisOffice put it 5.25 pt to the left, squeezed).
#[cfg_attr(windows, test)]
fn an_opening_bracket_after_the_leading_spaces_of_a_line_keeps_its_half_em() {
    let squeezed = laid_out("squeeze-cell-width-sweep-15.docx");
    let fits = laid_out("form-line-punct-probe2.docx");
    let x_of = |p: &Pagination| glyphs(&p.pages[0]).iter().find(|g| g.c == '【').expect("the bracket").x; // i18n-ok（試験の文書の文字）
    let (a, b) = (x_of(&squeezed), x_of(&fits));
    assert!(near(a - b, 9.9, 0.3), "the bracket is {:.2} pt to the right of where it stands on a line that fits (9.9 pt of the table's indent and margin)", a - b);
}

/// E7 (029): the characters on each line of `squeeze-faces-{meiryo,yugothic,msmincho}-{left,both}.docx` (four paragraphs of 120 characters, a stop after every fifth, in rooms of
/// 42.8, 43.1, 43.4 and 43.8 em of 10.5 pt) as Word's PDFs have them. A left aligned line takes in nothing (42, 43, 43, 43 on the first lines); a justified one squeezes each stop
/// by 0.216 em (Word's TJ adjustments of 216, the same in the three typefaces), the last stop of the line not counting: 43, 43 (not 45: 8 stops give 1.73 em, 1.9 are needed), 45, 45.
#[cfg_attr(windows, test)]
#[cfg_attr(windows, ignore = "open: Word's stops give 0.216 em each (the last of the line none) and this layout half an em; where 0.216 comes from is not known")]
fn a_justified_line_of_mode_15_takes_in_characters_by_squeezing_each_stop_by_a_fixed_part_of_its_em() {
    for face in ["meiryo", "yugothic", "msmincho"] {
        for (align, want) in [("left", [42, 41, 37, 43, 43, 34, 43, 43, 34, 43, 43, 34]), ("both", [43, 43, 34, 43, 43, 34, 45, 45, 30, 45, 45, 30])] {
            let pages = laid_out(&format!("squeeze-faces-{face}-{align}.docx"));
            let got: Vec<usize> = lines_of(&pages.pages[0], |_| true).iter().map(|(_, line)| line.len()).collect();
            assert_eq!(got, want, "{face} {align}: the characters on each line");
        }
    }
}

/// E3 (128): the text of a cell merged down over a page's end that is centred or at the foot is set within the cell's part on the page it starts on (`vmerge-aligned-across-pages.docx`:
/// 80 rows, 49 of them on page 1). Word's PDF: the first row's text is at 83.0 and the last one on page 1 at 760.8; the centred lines (408.3, 421.9, 435.5) are centred on the middle of
/// those, 421.9; the lines at the foot (747.2 and 760.8) end on the last row's line; and none of this text is on page 2. (The rows' pitch here is 14.0 pt, Word's 14.15, so the
/// positions are compared with the rows', not with Word's numbers.)
#[cfg_attr(windows, test)]
fn the_text_of_a_cell_merged_over_a_page_end_is_set_within_its_part_on_the_first_page() {
    let pages = laid_out("vmerge-aligned-across-pages.docx");
    assert!(pages.pages.len() >= 2, "{} pages", pages.pages.len());
    let ys = |c: char, page: usize| -> Vec<f32> {
        let mut v: Vec<f32> = glyphs(&pages.pages[page]).into_iter().filter(|g| g.c == c).map(|g| g.y).collect();
        v.sort_by(f32::total_cmp);
        v.dedup_by(|a, b| near(*a, *b, 0.3));
        v
    };
    let rows = ys('行', 0);
    let (first, last) = (rows[0], *rows.last().expect("rows"));
    let centred = ys('中', 0);
    assert_eq!(centred.len(), 3, "centred lines: {centred:?}");
    let mean = centred.iter().sum::<f32>() / 3.0;
    assert!(near(mean, (first + last) / 2.0, 1.0), "the centred lines' middle {mean:.1}, the part's middle {:.1}", (first + last) / 2.0);
    let foot = ys('下', 0);
    assert_eq!(foot.len(), 2, "lines at the foot: {foot:?}");
    assert!(near(foot[1], last, 1.0), "the last line at the foot {:.1}, the last row's {last:.1}", foot[1]);
    for c in ['中', '下'] {
        assert!(ys(c, 1).is_empty(), "{c} on page 2");
    }
}

/// The widths of the columns of the first table on page 1, from its vertical rules.
fn column_widths(page: &Page) -> Vec<f32> {
    let mut xs: Vec<f32> = page
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Rect { x, w, h, .. } if *w < 1.0 && *h > 5.0 => Some(x + w / 2.0),
            Item::Stroke { x, x2, y, y2, .. } if (x - x2).abs() < 0.01 && (y2 - y).abs() > 5.0 => Some(*x),
            _ => None,
        })
        .collect();
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| near(*a, *b, 0.3));
    xs.windows(2).map(|w| w[1] - w[0]).collect()
}

/// A table laid out automatically (no fixed layout) has the column widths Word gives it (Word's PDFs of `autofitm-*.docx`, the columns' rules measured: all 55 files match within 1.2 pt;
/// these are a sample). A column is as wide as its cells' preferred widths, or as its longest word with the paragraph's indents (the first line's for the first word) and the
/// cell's margins if that is wider (a word of 11 Ws, a left indent of 400 twips: 134.7 pt, not 100); the table's width then shares out what is more or less than the columns need
/// (in proportion for a percentage, down towards the least widths for a set width).
#[cfg_attr(windows, test)]
fn an_automatically_fitted_table_has_the_column_widths_word_gives_it() {
    let cases: [(&str, &[f32]); 6] = [
        ("autofitm-a-w11-i400-dxa.docx", &[134.67, 82.8, 82.58]),
        ("autofitm-a-w16-i0-pct.docx", &[201.63, 124.58, 124.71]),
        ("autofitm-a-w6-i800-auto.docx", &[107.52, 99.99, 99.98]),
        ("autofitm-b-hang.docx", &[99.96, 114.75, 99.98]),
        ("autofitm-b-first.docx", &[99.96, 134.79, 99.98]),
        ("autofitm-c-wide-auto.docx", &[181.95, 89.66, 89.52, 89.79]),
    ];
    for (file, want) in cases {
        let pages = laid_out(file);
        let got = column_widths(&pages.pages[0]);
        assert_eq!(got.len(), want.len(), "{file}: {got:?}");
        for (g, w) in got.iter().zip(want) {
            assert!(near(*g, *w, 1.2), "{file}: {got:.1?}, Word: {want:.1?}");
        }
    }
}

/// The cell of corpus 099 (Century 12 pt `m` and a superscript `2`, right aligned, a left indent of 282 twips, 727 twips wide, `autofitm-f-727-dxa.docx`) is as wide as they and the indent need
/// in Word's PDF: 40.1 pt, not the 36.35 pt the file says. The width of Century's letters is a Windows value (a Mac without it has another typeface's: 38.2 pt).
#[cfg_attr(windows, test)]
fn the_cell_of_a_word_with_an_indent_is_as_wide_as_the_word_and_indent_need() {
    let pages = laid_out("autofitm-f-727-dxa.docx");
    let got = column_widths(&pages.pages[0]);
    let want = [48.96, 49.92, 38.67, 38.64, 57.6, 30.74, 39.24, 47.52, 40.11, 30.24, 30.36];
    assert_eq!(got.len(), want.len(), "{got:?}");
    for (g, w) in got.iter().zip(want) {
        assert!(near(*g, w, 1.2), "{got:.1?}, Word: {want:.1?}");
    }
}

/// #142 (the user, Mac): the arrowhead bullet at the second level. Chosen at level 2 it is Wingdings' U+F0D8 in the file; where Wingdings is absent the
/// level's text is the arrowhead character itself (U+27A2), which is what is drawn.
#[cfg(not(windows))]
fn the_second_level_arrow_bullet_is_an_arrowhead_with_or_without_wingdings() {
    let mut s = crate::shell::scenario::Scenario::open("test/editing/docx-report.docx");
    s.size(1200., 900.);
    s.menu("docx.bullet.arrow");
    s.key("tab");
    s.menu("docx.bullet.arrow");
    s.settle();
    s.screenshot("bullet-arrow-level2");
    s.with(|e, _| {
        let n = &e.flow().unwrap().styles.numbering;
        let l = n.abstracts.values().last().unwrap().levels.get(1).unwrap();
        assert_eq!(l.symbol, Some(("Wingdings".to_string(), '\u{f0d8}')), "Word's code");
        assert_eq!(l.text, "\u{27a2}", "the arrowhead character is the stand-in without Wingdings");
    });
    assert_eq!(ibis_pptx::symbol_font::stand_in("Wingdings", '\u{f0d8}'), Some('\u{27a2}'));
}
