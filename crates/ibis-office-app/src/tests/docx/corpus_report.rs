// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The docx defects of the corpus report (the corpus compared with Word for
//! the web's PDFs) that show in the layout: each lays out a small file of
//! `test/corpus-report/docx/` and checks the pages against what Word draws.
//!
//! TODO: このファイルは、実文書の比較（コーパス）から見つかった崩れの試験の
//! 仮置き場で、ずっと残すものではない。試験が通るようになったら（崩れを直した
//! ら）、その試験を本番の試験に移すか、試験として要らなければ消し、最小文書
//! （`test/corpus-report/docx/`）も一緒に移すか消す。
//! 本番の移し先は `crates/ibis-office-app/src/docx/scenarios.rs`。
//! 崩れが残っている試験（赤）はここに置いたままにする。

use std::sync::Arc;

use gpui_kit::*;

use super::flow_layout::{Item, Page, Pagination};
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::corpus_report_docx_w1_fittext_span_matches_width", w1_fittext_span_matches_width),
        ("docx::corpus_report_docx_t1_vertical_centered_cell_stays_on_one_page", t1_vertical_centered_cell_stays_on_one_page),
        ("docx::corpus_report_docx_t3_cell_fits_exact_width", t3_cell_fits_exact_width),
        ("docx::corpus_report_docx_t4_autospacing_in_cell", t4_autospacing_in_cell),
        ("docx::corpus_report_docx_t6_split_row_keeps_min_height", t6_split_row_keeps_min_height),
        ("docx::corpus_report_docx_b2_trailing_ideographic_space_hangs", b2_trailing_ideographic_space_hangs),
        ("docx::corpus_report_docx_b3_small_kana_may_start_line", b3_small_kana_may_start_line),
        ("docx::corpus_report_docx_b4_curly_quotes_in_latin_font", b4_curly_quotes_in_latin_font),
        ("docx::corpus_report_docx_b5_no_squeeze_without_kerning", b5_no_squeeze_without_kerning),
        ("docx::corpus_report_docx_b6_right_tab_past_right_indent", b6_right_tab_past_right_indent),
        ("docx::corpus_report_docx_p1_grid_line_stays_when_its_text_fits", p1_grid_line_stays_when_its_text_fits),
        ("docx::corpus_report_docx_f1_page_anchored_table_follows_anchor", f1_page_anchored_table_follows_anchor),
        ("docx::corpus_report_docx_f2_stacked_floating_tables_do_not_overlap", f2_stacked_floating_tables_do_not_overlap),
        ("docx::corpus_report_docx_f3_long_floating_table_splits", f3_long_floating_table_splits),
        ("docx::corpus_report_docx_f4_floating_table_wrap_ends_at_page_break", f4_floating_table_wrap_ends_at_page_break),
        ("docx::corpus_report_docx_g2_eq_overstrike_shows_every_character", g2_eq_overstrike_shows_every_character),
        ("docx::corpus_report_docx_g3_enclosure_uses_its_own_size", g3_enclosure_uses_its_own_size),
        ("docx::corpus_report_docx_h1_continuous_section_first_page_header", h1_continuous_section_first_page_header),
        ("docx::corpus_report_docx_h2_lowercase_page_fields", h2_lowercase_page_fields),
        ("docx::corpus_report_docx_h3_framed_page_number_centered", h3_framed_page_number_centered),
        ("docx::corpus_report_docx_i2_leftchars_with_hangingchars", i2_leftchars_with_hangingchars),
        ("docx::corpus_report_docx_i4_justified_list_label_tab", i4_justified_list_label_tab),
        ("docx::corpus_report_docx_l1_ms_mincho_line_pitch_without_grid", l1_ms_mincho_line_pitch_without_grid),
        ("docx::corpus_report_docx_l2_table_rows_on_line_grid", l2_table_rows_on_line_grid),
        ("docx::corpus_report_docx_l3_doc_grid_without_type_is_no_grid", l3_doc_grid_without_type_is_no_grid),
        ("docx::corpus_report_docx_l4_char_space_applied", l4_char_space_applied),
        ("docx::corpus_report_docx_l5_ms_mincho_14pt_takes_two_grid_lines", l5_ms_mincho_14pt_takes_two_grid_lines),
        ("docx::corpus_report_docx_l6_grid_auto_multiple", l6_grid_auto_multiple),
        ("docx::corpus_report_docx_p1_break_only_line_at_page_bottom", p1_break_only_line_at_page_bottom),
        ("docx::corpus_report_docx_s2_relative_height_order", s2_relative_height_order),
        ("docx::corpus_report_docx_s4_layout_in_cell_page_relative", s4_layout_in_cell_page_relative),
        ("docx::corpus_report_docx_s9_shape_beside_square_wrap_drawn", s9_shape_beside_square_wrap_drawn),
        ("docx::corpus_report_docx_s10_top_and_bottom_offset", s10_top_and_bottom_offset),
        ("docx::corpus_report_docx_s14_autofit_box_fits_text", s14_autofit_box_fits_text),
        ("docx::corpus_report_docx_s11_vertical_text_box_drawn_vertical", s11_vertical_text_box_drawn_vertical),
        ("docx::corpus_report_docx_s13_turned_upright_text_drawn_level", s13_turned_upright_text_drawn_level),
        ("docx::corpus_report_docx_guard_cell_box_negative_column_offset", guard_c007_b1_cell_box_negative_column_offset),
        ("docx::corpus_report_docx_a10_text_that_cannot_sit_beside_a_table_starts_under_it", a10_text_that_cannot_sit_beside_a_table_starts_under_it),
        ("docx::corpus_report_docx_a10_empty_paragraphs_go_under_the_table", a10_empty_paragraphs_go_under_the_table),
        ("docx::corpus_report_docx_f6_floating_row_taller_than_a_page_goes_on", f6_floating_row_taller_than_a_page_goes_on),
        ("docx::corpus_report_docx_t7_page_break_in_a_cell_adds_no_line", t7_page_break_in_a_cell_adds_no_line),
        ("docx::corpus_report_docx_f5_paragraph_moved_off_an_obstacle_page_is_laid_again", f5_paragraph_moved_off_an_obstacle_page_is_laid_again),
        ("docx::corpus_report_docx_c020_b3_vml_line_from_offset_is_used", c020_b3_vml_line_from_offset_is_used),
        ("docx::corpus_report_docx_c021_b1_hint_east_asia_times_sign_is_full_width", c021_b1_hint_east_asia_times_sign_is_full_width),
        ("docx::corpus_report_docx_c022_b1_textbox_trailing_underlined_space_is_underlined", c022_b1_textbox_trailing_underlined_space_is_underlined),
        ("docx::corpus_report_docx_c022_b2_textbox_snap_to_grid_off_is_not_grid_high", c022_b2_textbox_snap_to_grid_off_is_not_grid_high),
        ("docx::corpus_report_docx_c023_b1_latin_hyphen_is_a_break_opportunity", c023_b1_latin_hyphen_is_a_break_opportunity),
        ("docx::corpus_report_docx_c023_b2_justified_line_shrinks_spaces_for_one_more_word", c023_b2_justified_line_shrinks_spaces_for_one_more_word),
        ("docx::corpus_report_docx_c024_b1_vertical_cell_with_fit_text_draws_its_label", c024_b1_vertical_cell_with_fit_text_draws_its_label),
        ("docx::corpus_report_docx_c025_b2_text_box_run_border_is_drawn", c025_b2_text_box_run_border_is_drawn),
        ("docx::corpus_report_docx_c026_b2_field_result_gets_no_auto_space", c026_b2_field_result_gets_no_auto_space),
        ("docx::corpus_report_docx_c026_b3_justify_slack_goes_to_the_auto_space", c026_b3_justify_slack_goes_to_the_auto_space),
        ("docx::corpus_report_docx_c028_b2_private_use_characters_are_mapped", c028_b2_private_use_characters_are_mapped),
        ("docx::corpus_report_docx_c028_b3_trailing_space_run_has_no_border", c028_b3_trailing_space_run_has_no_border),
        ("docx::corpus_report_docx_c028_b4_check_mark_is_as_large_as_the_box", c028_b4_check_mark_is_as_large_as_the_box),
        ("docx::corpus_report_docx_c001_b1_html_autospacing_sums_after_and_before", c001_b1_html_autospacing_sums_after_and_before),
        ("docx::corpus_report_docx_c002_b1_autofit_label_column_keeps_one_line", c002_b1_autofit_label_column_keeps_one_line),
        ("docx::corpus_report_docx_c014_b1_autofit_indent_does_not_widen_column", c014_b1_autofit_indent_does_not_widen_column),
        ("docx::corpus_report_docx_c014_b2_autofit_tcw_grid_mismatch_uses_grid", c014_b2_autofit_tcw_grid_mismatch_uses_grid),
        ("docx::corpus_report_docx_c010_b1_halfwidth_kana_advance_half_em", c010_b1_halfwidth_kana_advance_half_em),
        ("docx::corpus_report_docx_c011_b1_nested_table_rules_stop_at_page_bottom", c011_b1_nested_table_rules_stop_at_page_bottom),
        ("docx::corpus_report_docx_c011_b2_blank_paragraphs_after_wrap_square_take_height", c011_b2_blank_paragraphs_after_wrap_square_take_height),
        ("docx::corpus_report_docx_c016_b3_empty_paragraph_as_tall_as_text_line", c016_b3_empty_paragraph_as_tall_as_text_line),
        ("docx::corpus_report_docx_w2_ms_halfwidth_latin_drawn_half_em", w2_ms_halfwidth_latin_drawn_half_em),
        ("docx::corpus_report_docx_w3_ms_proportional_widths", w3_ms_proportional_widths),
        ("docx::corpus_report_docx_w4_halfwidth_space_is_half_em", w4_halfwidth_space_is_half_em),
        ("docx::corpus_report_docx_w5_mincho_bold_not_heavier_face", w5_mincho_bold_not_heavier_face),
        ("docx::corpus_report_docx_t2_nowrap_fixed_cell_wraps_in_cell", t2_nowrap_fixed_cell_wraps_in_cell),
        ("docx::corpus_report_docx_h4_ptab_right_aligns_header", h4_ptab_right_aligns_header),
        ("docx::corpus_report_docx_i1_leftchars_zero_keeps_numbering_indent", i1_leftchars_zero_keeps_numbering_indent),
        ("docx::corpus_report_docx_m1_downscaled_picture_is_averaged", m1_downscaled_picture_is_averaged),
        ("docx::corpus_report_docx_s8_dash_dot_stroked_border_band", s8_dash_dot_stroked_border_band),
        ("docx::corpus_report_docx_c006_b2_ms_gothic_drawn_heavier_than_default", c006_b2_ms_gothic_drawn_heavier_than_default),
        ("docx::corpus_report_docx_c017_b1_dashed_border_has_word_pattern", c017_b1_dashed_border_has_word_pattern),
        ("docx::corpus_report_docx_c018_b1_borderless_float_table_right_is_inside_the_margin", c018_b1_borderless_float_table_right_is_inside_the_margin),
        ("docx::corpus_report_docx_c023_b3_wordwrap0_does_not_cut_a_word_that_fits", c023_b3_wordwrap0_does_not_cut_a_word_that_fits),
        ("docx::corpus_report_docx_s5_vml_line_drawn", s5_vml_line_drawn),
        ("docx::corpus_report_docx_c013_b1_footer_frame_paragraph_takes_no_height", c013_b1_footer_frame_paragraph_takes_no_height),
        ("docx::corpus_report_docx_c015_b1_right_aligned_trailing_halfwidth_spaces_counted", c015_b1_right_aligned_trailing_halfwidth_spaces_counted),
        ("docx::corpus_report_docx_c015_b2_char_space_applies_in_table_cell", c015_b2_char_space_applies_in_table_cell),
        ("docx::corpus_report_docx_c015_b3_floating_table_keeps_first_line_indent", c015_b3_floating_table_keeps_first_line_indent),
        ("docx::corpus_report_docx_c016_b1_at_least_zero_line_not_snapped_to_grid", c016_b1_at_least_zero_line_not_snapped_to_grid),
        ("docx::corpus_report_docx_c016_b2_multiple_line_spacing_on_two_grid_lines", c016_b2_multiple_line_spacing_on_two_grid_lines),
        ("docx::corpus_report_docx_c006_b1_text_box_east_asian_font_as_body", c006_b1_text_box_east_asian_font_as_body),
        ("docx::corpus_report_docx_c008_b1_single_line_cell_takes_two_grid_lines", c008_b1_single_line_cell_takes_two_grid_lines),
        ("docx::corpus_report_docx_reg_b2_table_heading_row_starts_every_page", reg_b2_table_heading_row_starts_every_page),
    ]
}

/// Unfixed corpus_report bug scenarios (skipped in normal `cargo test`, run with `--ignored` or `--include-ignored`).
pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    vec![
        // Failing since origin/main (Word measurements made on Windows: the compression of punctuation in compatibility 15, an empty
        // paragraph of the footer, the vertical text in a cell) was merged with the Mac side's expectations. Which is right needs the
        // real Word (tasks/doc/release-plan-20261030.md, after 0.8.5).
        ("docx::corpus_report_docx_b1_compress_punctuation_keeps_line", b1_compress_punctuation_keeps_line),
        ("docx::corpus_report_docx_reg_b1_blank_paragraphs_at_page_end_add_no_page", reg_b1_blank_paragraphs_at_page_end_add_no_page),
        ("docx::corpus_report_docx_t5_merged_vertical_text_one_column", t5_merged_vertical_text_one_column),
    ]
}

fn open(file: &str) -> Scenario {
    Scenario::open(&format!("test/corpus-report/docx/{file}"))
}

fn pages_of(s: &mut Scenario) -> Arc<Pagination> {
    s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources))
}

fn laid_out(file: &str) -> Arc<Pagination> {
    pages_of(&mut open(file))
}

/// A character drawn on a page: where its glyph starts and its baseline.
#[derive(Clone, Debug)]
struct Glyph {
    c: char,
    x: f32,
    y: f32,
    size: f32,
    font: Font,
}

/// The page's characters in the order they are laid out.
fn glyphs(page: &Page) -> Vec<Glyph> {
    let mut out = Vec::new();
    for item in &page.items {
        match item {
            Item::Text { x, y, text, offsets, font, size, .. } => {
                for (c, dx) in text.chars().zip(offsets.iter()) {
                    out.push(Glyph { c, x: x + dx, y: *y, size: *size, font: font.clone() });
                }
            }
            Item::Turned { x, y, text, font, size, .. } => {
                for c in text.chars() {
                    out.push(Glyph { c, x: *x, y: *y, size: *size, font: font.clone() });
                }
            }
            Item::Scaled { x, y, text, font, size, .. } => {
                for c in text.chars() {
                    out.push(Glyph { c, x: *x, y: *y, size: *size, font: font.clone() });
                }
            }
            _ => {}
        }
    }
    out
}

/// Every run of glyphs spelling `text`, in layout order.
fn find_all(glyphs: &[Glyph], text: &str) -> Vec<Vec<Glyph>> {
    let want: Vec<char> = text.chars().collect();
    if glyphs.len() < want.len() {
        return Vec::new();
    }
    (0..=glyphs.len() - want.len()).filter(|&i| glyphs[i..i + want.len()].iter().map(|g| g.c).eq(want.iter().copied())).map(|i| glyphs[i..i + want.len()].to_vec()).collect()
}

fn find(glyphs: &[Glyph], text: &str) -> Vec<Glyph> {
    find_all(glyphs, text).into_iter().next().unwrap_or_else(|| panic!("{text:?} is drawn"))
}

/// The page (index) and glyphs where `text` is first drawn.
fn find_in(pages: &Pagination, text: &str) -> Option<(usize, Vec<Glyph>)> {
    pages.pages.iter().enumerate().find_map(|(i, p)| find_all(&glyphs(p), text).into_iter().next().map(|g| (i, g)))
}

/// The baselines of the page's text, top to bottom.
fn baselines(page: &Page) -> Vec<f32> {
    let mut ys: Vec<f32> = glyphs(page).iter().filter(|g| !g.c.is_whitespace()).map(|g| g.y).collect();
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    ys
}

/// The characters drawn between `top` and `bottom`, left to right then top
/// to bottom, without spaces.
fn text_between(page: &Page, top: f32, bottom: f32) -> String {
    let mut gs: Vec<Glyph> = glyphs(page).into_iter().filter(|g| g.y > top && g.y < bottom && !g.c.is_whitespace()).collect();
    gs.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    gs.iter().map(|g| g.c).collect()
}

/// The width the glyph of `c` is drawn with in `font` at `size` (as the
/// page is drawn: each character shaped alone, `page_view.rs`).
fn drawn_width(s: &mut Scenario, c: char, font: &Font, size: f32) -> f32 {
    let font = font.clone();
    s.update(move |_, window, _| {
        let text = c.to_string();
        let run = TextRun { len: text.len(), font, color: black(), background_color: None, underline: None, strikethrough: None };
        f32::from(window.text_system().shape_line(SharedString::from(text), px(size), &[run], None).width)
    })
}

fn near(actual: f32, expected: f32, tolerance: f32) -> bool {
    (actual - expected).abs() <= tolerance
}

fn check(failures: Vec<String>) {
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The window's pixels (RGB) over `(x, y, w, h)` (points) of page `index`,
/// scrolled into view first: columns, rows and the pixels row by row.
fn page_pixels(s: &mut Scenario, index: usize, (x, y, w, h): (f32, f32, f32, f32)) -> (usize, usize, Vec<[u8; 3]>) {
    s.update(|e, _, cx| e.scroll_into_view(index, y, h, cx));
    let (origin, k) = s.with(|e, _| e.page_origin(index)).expect("the page shown");
    let (width, pixels) = s.picture();
    let scale = width as f32 / 1200.0;
    let height = pixels.len() as u32 / 4 / width;
    let left = ((f32::from(origin.x) + x * k) * scale).round() as i64;
    let top = ((f32::from(origin.y) + y * k) * scale).round() as i64;
    let (cols, rows) = ((w * k * scale).round() as i64, (h * k * scale).round() as i64);
    assert!(left >= 0 && top >= 0 && left + cols <= width as i64 && top + rows <= height as i64, "({x}, {y}) {w}×{h}pt of page {} is in the window", index + 1);
    let mut out = Vec::with_capacity((cols * rows) as usize);
    for py in top..top + rows {
        for px in left..left + cols {
            let i = ((py as u32 * width + px as u32) * 4) as usize;
            out.push([pixels[i], pixels[i + 1], pixels[i + 2]]);
        }
    }
    (cols as usize, rows as usize, out)
}

/// The pixels (column, row) that `is` picks out of a crop of [`page_pixels`].
fn pick(crop: &(usize, usize, Vec<[u8; 3]>), is: impl Fn([u8; 3]) -> bool) -> Vec<(usize, usize)> {
    let (cols, _, pixels) = crop;
    pixels.iter().enumerate().filter(|(_, p)| is(**p)).map(|(i, _)| (i % cols, i / cols)).collect()
}

/// How many runs of consecutive rows have a picked pixel (lines of text).
fn bands(picked: &[(usize, usize)], rows: usize) -> usize {
    let mut on = vec![false; rows];
    for &(_, r) in picked {
        on[r] = true;
    }
    on.iter().zip(std::iter::once(&false).chain(on.iter())).filter(|(now, before)| **now && !**before).count()
}

/// The left edge of the page's tables: the leftmost vertical rule.
fn left_rule(page: &Page) -> f32 {
    page.items.iter().filter_map(|i| if let Item::Rect { x, w, h, .. } = i && *w < 1.0 && *h > 5.0 { Some(*x) } else { None }).fold(f32::MAX, f32::min)
}

/// W1「文字の均等割り付け（w:fitText）を読まず、区間が指定の幅にならない」:
/// Word は同じ w:id の run の区間を w:fitText/@w:val（1800tw = 90pt）の幅に
/// 広げ・縮める。区間の幅は先頭の字の左端から最後の字の右端（全角なので
/// 字の大きさ）まで。許容差 1.5pt はレポートの「134 区間すべてで差 1.5pt 以内」。
fn w1_fittext_span_matches_width() {
    let pages = laid_out("fittext-expand-compress.docx");
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let mut failures = Vec::new();
    for (label, chars) in [("A:", 5), ("B:", 5), ("C:", 5), ("D:", 2), ("E:", 9), ("F:", 9)] {
        let y = find(&gs, label)[0].y;
        let cell: Vec<&Glyph> = gs.iter().filter(|g| near(g.y, y, 0.5) && g.x < 160.0).collect();
        let span = cell.last().map_or(0.0, |g| g.x + g.size) - cell.first().map_or(0.0, |g| g.x);
        if cell.len() != chars || !near(span, 90.0, 1.5) {
            failures.push(format!("row {label} {} characters on its line over {span:.2}pt; Word: {chars} characters over 90pt", cell.len()));
        }
    }
    let title = find_all(&gs, "申込案内書").pop().expect("the title");
    let span = title[4].x + title[4].size - title[0].x;
    let centre = title[0].x + span / 2.0;
    let page_centre = page.width / 2.0;
    if !near(span, 90.0, 1.5) || !near(centre, page_centre, 1.5) {
        failures.push(format!("title over {span:.2}pt centred at {centre:.2}; Word: 90pt centred at {page_centre:.2}"));
    }
    check(failures);
}

/// W2「ＭＳ 明朝・ＭＳ ゴシックの半角英字を幅の広い字形で描き、隣の字に重なる」:
/// Word は半角英字を 0.5em の字形で描く（PDF の幅表で W・M・m・O とも 500/1000）。
/// 描く字形の幅（page_view と同じく1字ずつ組んだ幅）が 0.5em を 0.25pt 以上
/// 超えないこと。
fn w2_ms_halfwidth_latin_drawn_half_em() {
    let mut s = open("msfont-halfwidth.docx");
    let pages = pages_of(&mut s);
    let gs = glyphs(&pages.pages[0]);
    let mut failures = Vec::new();
    for (which, line) in find_all(&gs, "WWWWWWWWWW MMMMMMMMMM mmmmm wwwww OOOOO").into_iter().take(2).enumerate() {
        for g in line.iter().filter(|g| "WMmwO".contains(g.c)).step_by(5) {
            let w = drawn_width(&mut s, g.c, &g.font, g.size);
            if w > g.size * 0.5 + 0.25 {
                failures.push(format!("{} {:?} in {}: drawn {w:.2}pt wide; Word: {:.2}pt", ["ＭＳ 明朝", "ＭＳ ゴシック"][which], g.c, g.font.family, g.size * 0.5));
            }
        }
    }
    check(failures);
}

/// W3「ＭＳ Ｐ明朝・ＭＳ Ｐゴシックの字が置き換え先の幅で並び、行が長くなる」:
/// Word の幅（PDF の幅表）は、ＭＳ Ｐ明朝の「（」0.5em・「、」0.664em、
/// ＭＳ Ｐゴシックの o 0.508em。送り幅の許容差は 0.05em。
fn w3_ms_proportional_widths() {
    let pages = laid_out("msfont-halfwidth.docx");
    let gs = glyphs(&pages.pages[0]);
    let below = |label: &str, text: &str| -> Vec<Glyph> {
        let y = find(&gs, label)[0].y;
        find_all(&gs, text).into_iter().filter(|g| g[0].y > y + 1.0).min_by(|a, b| a[0].y.total_cmp(&b[0].y)).unwrap_or_else(|| panic!("{text} below {label}"))
    };
    let advance = |line: &[Glyph], c: char| -> (f32, f32) {
        let i = line.iter().position(|g| g.c == c).unwrap_or_else(|| panic!("{c}"));
        (line[i + 1].x - line[i].x, line[i].size)
    };
    let mut failures = Vec::new();
    let mincho = below("ＭＳ Ｐ明朝（", "「見本」の文章です。（かっこ）、句読点。");
    for (c, em) in [('（', 0.5), ('、', 0.664)] {
        let (adv, size) = advance(&mincho, c);
        if !near(adv / size, em, 0.05) {
            failures.push(format!("ＭＳ Ｐ明朝 {c:?}: {:.3}em; Word: {em}em", adv / size));
        }
    }
    let gothic = below("ＭＳ Ｐゴシック（", "Hello World,");
    let (adv, size) = advance(&gothic, 'o');
    if !near(adv / size, 0.508, 0.05) {
        failures.push(format!("ＭＳ Ｐゴシック 'o': {:.3}em; Word: 0.508em", adv / size));
    }
    check(failures);
}

/// W4「半角空白が欧文書体の幅（約 0.25em）で並び、Word（字の大きさの半分）の
/// 約半分になる（balanceSingleByteDoubleByteWidth 未対応）」: Word は半角空白を
/// 字の大きさの半分（10.5pt なら 5.25pt）にする。Century の半角空白4つの行の
/// 「あ」は全角空白2つの行の「あ」と同じ位置、下線付きの半角空白20個は 105pt。
fn w4_halfwidth_space_is_half_em() {
    let pages = laid_out("balance-spaces.docx");
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let mut failures = Vec::new();
    let lines = find_all(&gs, "あいうえお（");
    let (full, half) = (lines[0][0].x, lines[1][0].x);
    if !near(half, full, 0.3) {
        failures.push(format!("after 4 half-width spaces あ is at x {half:.2}; Word: {full:.2} (as after 2 full-width spaces)"));
    }
    let y = find(&gs, "氏名")[0].y;
    let underline = page.items.iter().find_map(|i| if let Item::Rect { y: ry, w, h, .. } = i && *h < 1.5 && near(*ry, y, 3.0) { Some(*w) } else { None }).expect("the underline");
    if !near(underline, 105.0, 1.0) {
        failures.push(format!("the underline of 20 spaces is {underline:.2}pt; Word: 105pt"));
    }
    check(failures);
}

/// W5「ＭＳ 明朝の太字を、置き換え先の書体の太いウェイト（W6）で描き、
/// Word よりずっと太くなる」: ＭＳ 明朝には太字の書体が無く、Word は普通の
/// 字形を細い線で太らせる（擬似太字）。太字の行も普通の行と同じ書体（face）で
/// 描かれること。
fn w5_mincho_bold_not_heavier_face() {
    let mut s = open("mincho-bold.docx");
    let pages = pages_of(&mut s);
    let gs = glyphs(&pages.pages[0]);
    let regular = find(&gs, "ＭＳ 明朝の通常");
    let bold = find(&gs, "ＭＳ 明朝の太字：");
    let digits = find_all(&gs, "2020");
    let pairs = [("和文", regular[0].font.clone(), bold[0].font.clone()), ("半角数字", digits[0][0].font.clone(), digits[1][0].font.clone())];
    let mut failures = Vec::new();
    for (what, normal, heavy) in pairs {
        let (a, b) = {
            let (normal, heavy) = (normal.clone(), heavy.clone());
            s.update(move |_, window, _| (window.text_system().resolve_font(&normal), window.text_system().resolve_font(&heavy)))
        };
        if a != b {
            failures.push(format!("{what}: bold drawn with another face ({} {:?}) than regular ({} {:?})", heavy.family, heavy.weight, normal.family, normal.weight));
        }
    }
    check(failures);
}

/// T1「縦書きのセルを中央揃え・均等割り付けにすると、行が数千 pt に伸びて
/// 白紙のページが大量にできる」: Word ではどちらの表も1ページに収まり、
/// 5行に結合した縦書きセルの ABCDEFGH は1列に並ぶ。
fn t1_vertical_centered_cell_stays_on_one_page() {
    let mut failures = Vec::new();
    for file in ["vert_center.docx", "vert_distribute_merged.docx"] {
        let pages = laid_out(file);
        if pages.pages.len() != 1 {
            failures.push(format!("{file}: {} pages; Word: 1", pages.pages.len()));
        }
        let page = &pages.pages[0];
        let off_page: Vec<String> = glyphs(page).iter().filter(|g| "縦書見出".contains(g.c) && g.y > page.height).map(|g| format!("{}@{:.0}", g.c, g.y)).collect();
        if !off_page.is_empty() {
            failures.push(format!("{file}: vertical text below the page: {off_page:?}"));
        }
    }
    let pages = laid_out("r5-vertical-merged.docx");
    let xs: Vec<f32> = glyphs(&pages.pages[0]).iter().filter(|g| "ABCDEFGH".contains(g.c) && g.y > 105.0).map(|g| g.x).collect();
    let spread = xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
    if xs.len() != 8 || spread > 0.5 {
        failures.push(format!("r5-vertical-merged.docx: ABCDEFGH at x {xs:?}; Word: one column"));
    }
    check(failures);
}

/// T2「「折り返さない」（w:noWrap）のセルで、固定幅でも折り返さずはみ出し、
/// 中央揃えの文字が消える」: Word は固定幅のセルでは noWrap でもセル幅で
/// 折り返し、中央揃えの CENTER は3列目（表の左端から 200〜300pt）の中に出る。
fn t2_nowrap_fixed_cell_wraps_in_cell() {
    let mut failures = Vec::new();
    let pages = laid_out("r3-nowrap-cells.docx");
    let gs = glyphs(&pages.pages[0]);
    let left = find(&gs, "normal cell")[0].x - 5.4;
    let (col2, col3) = (left + 100.0, left + 200.0);
    let center = find_all(&gs, "CENTER").pop().expect("CENTER");
    if center.iter().any(|g| g.x < col3 || g.x > col3 + 100.0) {
        failures.push(format!("CENTER at x {:.1}..{:.1}; Word: in column 3 ({col3:.1}..{:.1})", center[0].x, center[5].x, col3 + 100.0));
    }
    let wrapped = find(&gs, "noWrap left");
    let line_end = gs.iter().filter(|g| near(g.y, wrapped[0].y, 0.5) && g.x >= col2 && g.x < col3 + 200.0 && !"CENTER".contains(g.c)).map(|g| g.x).fold(0.0f32, f32::max);
    if line_end > col3 {
        failures.push(format!("column 2's text runs to x {line_end:.1}; Word: wraps before {col3:.1}"));
    }
    let pages = laid_out("r3-nowrap-fixed-cell.docx");
    let right = pages.pages[0].width - 56.7;
    let past: Vec<f32> = glyphs(&pages.pages[0]).iter().filter(|g| g.x > right).map(|g| g.x).collect();
    if !past.is_empty() {
        failures.push(format!("r3-nowrap-fixed-cell.docx: {} characters right of the margin ({right:.1}), up to x {:.0}; Word: wrapped in the cell", past.len(), past.iter().cloned().fold(0.0f32, f32::max)));
    }
    check(failures);
}

/// T3「列の境目を 1/300 インチに丸めるため、ちょうど収まる文字がセル内で
/// 折り返す」: 文字の入る幅は (3060 − 99×2)tw ＝ 143.1pt、11pt × 13字 ＝ 143.0pt
/// で、Word は13字を1行に入れる（「痰」まで1行目）。
fn t3_cell_fits_exact_width() {
    let pages = laid_out("repro-E-cell-edge-rounding.docx");
    let gs = glyphs(&pages.pages[0]);
    let mut failures = Vec::new();
    for (i, line) in find_all(&gs, "高齢者").into_iter().enumerate() {
        let tan = gs.iter().find(|g| g.c == '痰' && g.y > line[0].y - 0.5 && g.y < line[0].y + 30.0).expect("痰");
        if !near(tan.y, line[0].y, 0.5) {
            failures.push(format!("table {}: 痰 on the next line (y {:.2}, the first line {:.2}); Word: 13 characters on the first line", i + 1, tan.y, line[0].y));
        }
    }
    check(failures);
}

/// T4「段落の前後の自動間隔（beforeAutospacing／afterAutospacing）を無視する」:
/// Word は afterAutospacing の段落の後を 14pt にし、セルの最後の段落では 0 にする。
/// 行は固定 14pt の行で、1段落のセルは最小の高さ 17pt、2段落のセルは
/// 14 + 14 + 14 = 42pt。許容差 1pt は罫線の太さと 137 の実測（17.5／42.5pt）から。
fn t4_autospacing_in_cell() {
    let pages = laid_out("repro-D-autospacing-in-cell.docx");
    let page = &pages.pages[0];
    let left = left_rule(page);
    let heights: Vec<f32> = page.items.iter().filter_map(|i| if let Item::Rect { x, w, h, .. } = i && *w < 1.0 && *h > 5.0 && near(*x, left, 0.1) { Some(*h) } else { None }).collect();
    let expected = [17.0, 42.0, 17.0];
    assert!(heights.len() == 3 && heights.iter().zip(expected).all(|(h, e)| near(*h, e, 1.0)), "row heights {heights:?}; Word: {expected:?}");
}

/// T5「縦に結合したセルや最小の高さ（atLeast）の行の縦書きを最初の行の高さで
/// 折り返し、逆順の横書きのように左へはみ出す」: Word は結合した欄に1列の
/// 縦書きで描き（表の左の外に出ない）、収まる長さなら結合した欄の中央に置く。
fn t5_merged_vertical_text_one_column() {
    let pages = laid_out("vtext-merged-row-height.docx");
    let gs = glyphs(&pages.pages[0]);
    let mut failures = Vec::new();
    let table_left = 72.0 + 20.0;
    for (case, text, column) in [("1", "架空の品目に係る事項", 21.0), ("2", "上部構造", 36.0)] {
        let label_y = find(&gs, &format!("（{case}）"))[0].y;
        let chars: Vec<&Glyph> = gs.iter().filter(|g| text.contains(g.c) && g.y > label_y + 1.0 && g.y < label_y + 140.0 && g.x < table_left + column).collect();
        let xs: Vec<f32> = chars.iter().map(|g| g.x).collect();
        let spread = xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
        let leftmost = xs.iter().cloned().fold(f32::MAX, f32::min);
        if spread > 0.5 || leftmost < table_left - 0.5 {
            failures.push(format!("({case}): {text} at x {xs:?}; Word: one column inside the table (left {table_left})"));
        }
    }
    let kubun: Vec<f32> = gs.iter().filter(|g| g.c == '区' || g.c == '分').map(|g| g.y).collect();
    let rows: Vec<f32> = find_all(&gs, "架空の科目").iter().map(|g| g[0].y).collect();
    let middle = (rows[0] + rows[rows.len() - 1]) / 2.0;
    let at = kubun.iter().sum::<f32>() / kubun.len() as f32;
    if !near(at, middle, 10.0) {
        failures.push(format!("(3): 区分 around y {at:.1}; Word: the middle of the 5 merged rows ({middle:.1})"));
    }
    check(failures);
}

/// T6「ページで分けた表の行で、最小の高さ（trHeight の atLeast）が失われる」:
/// 最小 600pt（12000tw）の行が2ページに分かれても、Word では合計の高さが
/// 600pt 以上ある（左の罫線の長さの和で測る）。
fn t6_split_row_keeps_min_height() {
    let pages = laid_out("split-row-min-height.docx");
    let mut total = 0.0;
    for page in &pages.pages {
        let left = left_rule(page);
        total += page.items.iter().filter_map(|i| if let Item::Rect { x, w, h, .. } = i && *w < 1.0 && *h > 15.0 && near(*x, left, 0.1) { Some(*h) } else { None }).sum::<f32>();
    }
    assert!(total >= 599.0, "the split row is {total:.1}pt high in all; Word: at least 600pt");
}

/// B1「約物（括弧・句読点）を詰めずに折り返すため、Word より1字早く改行する
/// （compressPunctuation 未対応）」: Word は「、」「（」「）」を詰めて、
/// 本文幅をわずかに超える段落を1行に収める（repro-B は4段落で4行、
/// compress_punct は「、」を含む2段落が1行ずつで計3行）。
fn b1_compress_punctuation_keeps_line() {
    let mut failures = Vec::new();
    for (file, lines) in [("repro-B-compress-punct.docx", 4), ("compress_punct.docx", 3)] {
        let pages = laid_out(file);
        let n = baselines(&pages.pages[0]).len();
        if n != lines {
            failures.push(format!("{file}: {n} lines; Word: {lines}"));
        }
    }
    check(failures);
}

/// B2「行末の全角空白がぶら下がらず、余分な行ができる」: Word は段落末の
/// 全角空白を行末からはみ出させ、6段落は6行になる。
fn b2_trailing_ideographic_space_hangs() {
    let pages = laid_out("repro-C-trailing-ideographic-space.docx");
    let mut ys: Vec<f32> = glyphs(&pages.pages[0]).iter().map(|g| g.y).collect();
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    assert_eq!(ys.len(), 6, "lines at {ys:?}; Word: 6 (the trailing spaces hang)");
}

/// B3「小書きのかな・長音「ー」を行頭に置かない（禁則が Word の標準より強い）」:
/// Word は「…りるチェ／ックボックス」「…エネルギ／ー」と改行する。
fn b3_small_kana_may_start_line() {
    let pages = laid_out("kinsoku_small_kana.docx");
    let gs = glyphs(&pages.pages[0]);
    let mut failures = Vec::new();
    for (word, at) in [("チェック", 2), ("エネルギー", 4)] {
        let g = &find(&gs, word)[at];
        let first = gs.iter().filter(|h| near(h.y, g.y, 0.5)).min_by(|a, b| a.x.total_cmp(&b.x)).unwrap();
        if first.c != g.c {
            failures.push(format!("the line with {:?} of {word} starts with {:?}; Word: starts with {:?}", g.c, first.c, g.c));
        }
    }
    check(failures);
}

/// B4「欧文の中の “ ” を、w:hint を見ずに和文の全角の書体で描く」:
/// hint が無く ascii・hAnsi が Times New Roman の run で、Word は引用符を
/// Times New Roman（欧文の幅、0.444em）で描き、前後が空かない。書体と、引用符の
/// 送り幅（次の字までの距離）が 0.6em 以下であることを確かめる。
fn b4_curly_quotes_in_latin_font() {
    let pages = laid_out("repro-J-curly-quotes-font.docx");
    let gs = glyphs(&pages.pages[0]);
    let mut wrong = Vec::new();
    for (i, g) in gs.iter().enumerate().filter(|(_, g)| g.c == '“' || g.c == '”') {
        let advance = gs.get(i + 1).filter(|n| near(n.y, g.y, 0.5)).map(|n| n.x - g.x);
        if g.font.family.as_ref() != "Times New Roman" || advance.is_some_and(|a| a > g.size * 0.6) {
            wrong.push(format!("{:?} in {} advancing {:?}pt", g.c, g.font.family, advance.map(|a| (a * 100.0).round() / 100.0)));
        }
    }
    assert!(wrong.is_empty(), "{} quotes not drawn as Times New Roman's, e.g. {:?}; Word: Times New Roman, at most 0.6em", wrong.len(), wrong.first());
}

/// B5「カーニングが無効な文字でも、「）（」など約物の連続を半角に詰める」:
/// kern=0 の行では Word は詰めず、2つ目の「（」は1つ目から全角6字（63pt）右。
fn b5_no_squeeze_without_kerning() {
    let pages = laid_out("punct-squeeze-kern.docx");
    let line = find(&glyphs(&pages.pages[0]), "（\u{3000}\u{3000}\u{3000}階）（");
    let gap = line[6].x - line[0].x;
    assert!(near(gap, 63.0, 0.3), "the second （ is {gap:.2}pt after the first; Word: 63pt (no squeeze with kern=0)");
}

/// P1「行のグリッドの行が本文の下端にかかるとき、文字が収まるのに次のページへ送る」: Word は行のグリッドの余白（文字の下に足される
/// 分、グリッドの足す量の半分）が下端を越えても、文字が収まれば同じページに置く。21 行の段落で、21 行目が下端を越える量だけを変えた 4 本
/// （Word の PDF：Century 10.5pt は 2pt まで 1 ページ・3pt から 2 ページ、ＭＳ 明朝 10.5pt は 1pt まで 1 ページ・2pt から 2 ページ）。
fn p1_grid_line_stays_when_its_text_fits() {
    let mut failures = Vec::new();
    for (file, expected) in [("grid-foot-century-2pt.docx", 1), ("grid-foot-century-3pt.docx", 2), ("grid-foot-mincho-1pt.docx", 1), ("grid-foot-mincho-2pt.docx", 2)] {
        let pages = laid_out(file).pages.len();
        if pages != expected {
            failures.push(format!("{file}: {pages} pages; Word: {expected}"));
        }
    }
    check(failures);
}

/// B6「右インデントより少し外にある右揃えタブで、行末の字（「印」）が次の行へ
/// 折り返す」: Word は右タブ 8364tw（左余白から 418.2pt）に「印」を右揃えで置く
/// （右インデント 0 の行と同じ x ＝ 492.75pt）。
fn b6_right_tab_past_right_indent() {
    let pages = laid_out("right-tab-past-indent.docx");
    let gs = glyphs(&pages.pages[0]);
    let label = find(&gs, "右インデント");
    let seal = gs.iter().filter(|g| g.c == '印').min_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
    assert!(near(seal.y, label[0].y, 0.5) && near(seal.x, 492.75, 0.5), "印 at ({:.2}, {:.2}), its line at y {:.2}; Word: on the line at x 492.75", seal.x, seal.y, label[0].y);
}

/// F1「ページ基準の浮動の表が、アンカー段落のページではなく、いま組んでいる
/// ページに置かれて重なる」: Word では表はアンカー段落と一緒に2ページ目に置かれる。
fn f1_page_anchored_table_follows_anchor() {
    let pages = laid_out("B-floating-page-anchor-next-page.docx");
    let (table, _) = find_in(&pages, "浮動表 一行目").expect("the table");
    let (anchor, _) = find_in(&pages, "（次の様式）").expect("the anchor");
    assert_eq!(table, anchor, "the table on page {}, its anchor paragraph on page {}; Word: the same page", table + 1, anchor + 1);
}

/// F2「浮動の表が続くと、後の表や本文が前の表の下に回り込まず重なる」:
/// Word では3つの表が縦に並び、後の本文はその下に続く。
fn f2_stacked_floating_tables_do_not_overlap() {
    let pages = laid_out("C-stacked-floating-tables.docx");
    let at = |t: &str| find_in(&pages, t).map(|(p, g)| (p, g[0].y)).unwrap_or_else(|| panic!("{t}"));
    let mut failures = Vec::new();
    for (upper, lower) in [("表一 2", "表二 1"), ("表二 6", "表三 1"), ("表三 1", "表の後の本文一")] {
        let (a, b) = (at(upper), at(lower));
        if !(b.0 > a.0 || (b.0 == a.0 && b.1 >= a.1 + 10.0)) {
            failures.push(format!("{lower} (page {}, y {:.1}) is not below {upper} (page {}, y {:.1})", b.0 + 1, b.1, a.0 + 1, a.1));
        }
    }
    check(failures);
}

/// F3「1ページに収まらない浮動の表が、次のページに分割されずにはみ出す」:
/// Word は100行の表を2ページに分け、どの行も本文の下端（余白 1134tw の上）より上。
fn f3_long_floating_table_splits() {
    let pages = laid_out("D-floating-table-taller-than-page.docx");
    let mut below = Vec::new();
    for (i, page) in pages.pages.iter().enumerate() {
        let bottom = page.height - 56.7;
        below.extend(find_all(&glyphs(page), "項目").iter().filter(|g| g[0].y > bottom + 1.0).map(|g| format!("page {} y {:.0}", i + 1, g[0].y)));
    }
    assert!(below.is_empty(), "{} rows below the bottom margin, the first at {:?} ({} pages); Word: the table goes on to page 2", below.len(), below.first(), pages.pages.len());
}

/// F4「浮動の表の直後の段落に改ページがあると、次のページの行まで表の回り込みを
/// 受け、文字が左右に散る」: Word は2ページ目の「（見出し）」を1行に詰めて置く。
fn f4_floating_table_wrap_ends_at_page_break() {
    let pages = laid_out("floating-table-pagebreak.docx");
    let page = pages.pages.iter().find(|p| glyphs(p).iter().any(|g| g.c == '見')).expect("（見出し）");
    let gs: Vec<Glyph> = glyphs(page).into_iter().filter(|g| "（見出し）".contains(g.c) && g.y < 130.0).collect();
    let line = gs.iter().all(|g| near(g.y, gs[0].y, 0.5));
    let packed = gs.windows(2).all(|w| near(w[1].x - w[0].x, 10.5, 0.5));
    assert!(gs.len() == 5 && line && packed, "（見出し） drawn at {:?}; Word: one line, side by side", gs.iter().map(|g| (g.c, g.x.round(), g.y.round())).collect::<Vec<_>>());
}

/// A10「全幅（または細い隙間しか残らない）浮動の表の横に置けない行は、表の下端のすぐ下から
/// 始まる」: Word の試し docx（ftab の f2・f8）で、表の最後の行の文字から次の行の文字まで
/// 25.0pt（Word 186.9 - 161.9。字体の違いによる行の高さの差は含まない）。距離の上限は無い。
fn a10_text_that_cannot_sit_beside_a_table_starts_under_it() {
    let mut failures = Vec::new();
    for file in ["ftab-f2-full-center.docx", "ftab-f8-slot-15pt.docx"] {
        let pages = laid_out(file);
        let gs = glyphs(&pages.pages[0]);
        let last_row = gs.iter().filter(|g| g.c == 'T').map(|g| g.y).fold(0.0f32, f32::max);
        let l00 = find(&gs, "L00")[0].y;
        if !near(l00 - last_row, 25.0, 0.6) {
            failures.push(format!("{file}: L00 {:.1}pt under the last row's text; Word: 25.0pt", l00 - last_row));
        }
    }
    check(failures);
}

/// A10「表の下へ押し下げる空の段落に長さの上限は無い」: f5 は表の下に空段落 8 つ（Word:
/// L00 から L01 まで 9 行分。表の横に置こうとして飛ばされず、表の下に積まれる）。
fn a10_empty_paragraphs_go_under_the_table() {
    let pages = laid_out("ftab-f5-full-empties.docx");
    let gs = glyphs(&pages.pages[0]);
    let (l00, l01, l02) = (find(&gs, "L00")[0].y, find(&gs, "L01")[0].y, find(&gs, "L02")[0].y);
    let pitch = l02 - l01;
    let rows = (l01 - l00) / pitch;
    assert!(near(rows, 9.0, 0.2), "L00 to L01 is {rows:.2} lines; Word: 9 (the anchor line and 8 empty ones, all under the table)");
}

/// T7「表のセルの中の改ページは何もしない」: Word は先頭に改ページのあるセルの見出し行を
/// 1 行の高さで描く（144 の p.4 で 24.8pt、ibisOffice は 29.8pt だった）。改ページの無い
/// 同じ表と、続く段落の位置が同じで、ページは 1 つ。
fn t7_page_break_in_a_cell_adds_no_line() {
    let plain = laid_out("cellbreak-plain.docx");
    let after = find(&glyphs(&plain.pages[0]), "after")[0].y;
    let mut failures = Vec::new();
    for file in ["cellbreak-start.docx", "cellbreak-both.docx"] {
        let pages = laid_out(file);
        let y = find(&glyphs(&pages.pages[0]), "after")[0].y;
        if pages.pages.len() != 1 || !near(y, after, 0.1) {
            failures.push(format!("{file}: {} pages, `after` at {y:.1} (without the break: {after:.1}); Word: one page, the same place", pages.pages.len()));
        }
    }
    check(failures);
}

/// F5「障害物のあるページで下へ押し下げた段落が、次のページへ移るとき押し下げた高さを持ち越す」:
/// ページの下端の近くで終わる浮動の表 A の後ろの空段落と、その後ろの浮動の表 B。Word
///（144）は B を次のページの上（空段落の下）に置く（ibisOffice は B の見出し行をページの下端に置き、1 ページ増えた）。
fn f5_paragraph_moved_off_an_obstacle_page_is_laid_again() {
    let mut failures = Vec::new();
    for file in ["floats-a-b-1-empty.docx", "floats-a-b-3-empties.docx", "floats-a-b-5-empties.docx"] {
        let pages = laid_out(file);
        let (page, b) = find_in(&pages, "B").expect("B");
        if pages.pages.len() != 2 || page != 1 || b[0].y > 200.0 {
            failures.push(format!("{file}: {} pages, B on page {} at y {:.0}; Word: 2 pages, B on page 2 under the empty paragraphs (in the top 200pt)", pages.pages.len(), page + 1, b[0].y));
        }
    }
    check(failures);
}

/// F6「ページより高い 1 行の浮動の表の、ページに収まらない分の文字が描かれない」: Word は行を
/// ページの下端で分け、残りを次のページの上に置く（033 の最後のページは空白だった）。
fn f6_floating_row_taller_than_a_page_goes_on() {
    let mut failures = Vec::new();
    for (file, n) in [("floatrow-split-70.docx", 70), ("floatrow-split-120.docx", 120), ("floatrow-fits-30.docx", 30)] {
        let pages = laid_out(file);
        let missing: Vec<usize> = (0..n).filter(|i| find_in(&pages, &format!("ITEM{i:03}")).is_none()).collect();
        if !missing.is_empty() {
            failures.push(format!("{file}: items {missing:?} are not drawn ({} pages)", pages.pages.len()));
        }
    }
    check(failures);
}

/// G2「EQ フィールドの \o\ad（均等割り付けの重ね書き）・\o\al を囲い文字として
/// 描き、先頭の1字だけになる」: Word は \o\ad(申込案内書,全角空白15字) を
/// 157.5pt の幅に5字とも描き、\o\al の二行の小書きも全部の字を描く。
fn g2_eq_overstrike_shows_every_character() {
    let pages = laid_out("eq-overstrike-distribute.docx");
    let gs = glyphs(&pages.pages[0]);
    let mut failures = Vec::new();
    let (top, bottom) = (find(&gs, "題名")[0].y, find(&gs, "二行の小書き")[0].y);
    let title: Vec<&Glyph> = gs.iter().filter(|g| g.y > top + 1.0 && g.y < bottom - 1.0 && "申込案内書".contains(g.c)).collect();
    let span = title.iter().map(|g| g.x + g.size).fold(0.0f32, f32::max) - title.iter().map(|g| g.x).fold(f32::MAX, f32::min);
    if title.len() != 5 || !near(span, 157.5, 2.0) {
        failures.push(format!("\\o\\ad: {:?} over {span:.1}pt; Word: 申込案内書 over 157.5pt", title.iter().map(|g| g.c).collect::<String>()));
    }
    let small: String = gs.iter().filter(|g| g.y > bottom + 1.0).map(|g| g.c).collect();
    let missing: String = "団体のときは名称及び代表者の氏名".chars().filter(|c| !small.contains(*c)).collect();
    if !missing.is_empty() {
        failures.push(format!("\\o\\al: {missing:?} not drawn (drawn: {small:?})"));
    }
    check(failures);
}

/// G3「囲い文字（EQ \o\ac）の囲みの大きさと位置を使わず、小さく細い丸になる」:
/// Word は ○ を自分の run の大きさ 16.5pt（sz=33）で、中の「印」を 11pt で描く。
fn g3_enclosure_uses_its_own_size() {
    let pages = laid_out("shapes-5-enclosure-size.docx");
    let gs = glyphs(&pages.pages[0]);
    let ring = gs.iter().find(|g| g.c == '○').expect("○");
    let seal = gs.iter().find(|g| g.c == '印').expect("印");
    assert!(near(ring.size, 16.5, 0.05) && near(seal.size, 11.0, 0.05), "○ {}pt, 印 {}pt; Word: ○ 16.5pt, 印 11pt", ring.size, seal.size);
}

/// H1「継続（continuous）の節区切りで始まる節が、次に作るページを「最初の
/// ページ」とみなし、titlePg のヘッダーが出る」: 節2 は1ページ目の途中から
/// 始まるので、Word は2ページ目に節2 の既定のヘッダーを出す。
fn h1_continuous_section_first_page_header() {
    let pages = laid_out("hdrftr-continuous-titlepg.docx");
    let header = text_between(&pages.pages[1], 0.0, 85.0);
    assert_eq!(header, "第2節の既定のヘッダー", "page 2's header");
}

/// H2「小文字のフィールド名（page・numpages）を認識せず、ページ番号が全ページで
/// 保存時の「1」になる」: Word はフィールド名の大文字・小文字を区別せず、
/// 3ページの文書のフッターを「- 2 - / 3」のように進める。
fn h2_lowercase_page_fields() {
    let mut failures = Vec::new();
    let pages = laid_out("lowercase-page-field.docx");
    for (i, page) in pages.pages.iter().enumerate() {
        let footer = text_between(page, page.height - 56.7, page.height);
        let want = format!("-{}-/3", i + 1);
        if footer != want {
            failures.push(format!("lowercase-page-field.docx page {}: footer {footer:?}; Word: {want:?}", i + 1));
        }
    }
    let pages = laid_out("hdrftr-lowercase-page.docx");
    for (i, page) in pages.pages.iter().enumerate() {
        let footer = text_between(page, page.height - 70.0, page.height);
        let want = format!("-{n}-／3（大文字）{n}", n = i + 1);
        let header = text_between(page, 0.0, 60.0);
        let header_want = format!("単純フィールド（小文字）：{}", i + 1);
        if footer != want || header != header_want {
            failures.push(format!("hdrftr-lowercase-page.docx page {}: header {header:?} footer {footer:?}; Word: {header_want:?} {want:?}", i + 1));
        }
    }
    check(failures);
}

/// H3「段落の枠（w:framePr）を読まず、フッターのページ番号が中央に寄らず、
/// 位置も上にずれる」: xAlign="center"・hAnchor="margin" の枠の番号を、Word は
/// 文字領域の中央に置く（左右の余白が同じなのでページの中央）。許容差 1.5pt。
fn h3_framed_page_number_centered() {
    let pages = laid_out("hdrftr-framepr-footer.docx");
    let page = &pages.pages[0];
    let number = glyphs(page).into_iter().filter(|g| g.y > page.height - 85.0).find(|g| g.c == '1').expect("the page number");
    let centre = number.x + number.size * 0.25;
    let want = page.width / 2.0;
    assert!(near(centre, want, 1.5), "the page number's centre at x {centre:.2}; Word: {want:.2} (the text area's centre)");
}

/// H4「位置指定のタブ（w:ptab）を読まず、ヘッダーの右寄せの文字が中央に出る」:
/// Word は右揃えの ptab の後の「（様式第９号）」を右余白の端（595.3 − 85.05pt）に
/// 右揃えで置く。
fn h4_ptab_right_aligns_header() {
    let pages = laid_out("hdrftr-ptab-right.docx");
    let gs = glyphs(&pages.pages[0]);
    let label = find(&gs, "（様式第９号）");
    let right = label[6].x + label[6].size;
    let want = pages.pages[0].width - 85.05;
    assert!(near(right, want, 1.0), "（様式第９号） ends at x {right:.2}; Word: {want:.2}");
}

/// I1「番号付き段落の字下げが leftChars="0" だけだと、番号が左の余白へはみ出す」:
/// Word は leftChars="0" を字数指定なしとし、番号定義の left − hanging（0）に
/// 番号を置く（本文の左端 72pt）。
fn i1_leftchars_zero_keeps_numbering_indent() {
    let pages = laid_out("leftchars-zero.docx");
    let label = find(&glyphs(&pages.pages[0]), "（注１）");
    assert!(near(label[0].x, 72.0, 0.5), "（注１） at x {:.2}; Word: 72 (the left margin)", label[0].x);
}

/// I2「leftChars と hangingChars があり w:left が無い段落で、左インデントを
/// leftChars だけで計算する」: Word は左インデントを (leftChars ＋ hangingChars)
/// 字 ＝ 0 とし、1行目は比較用の left=0 hanging=283 の段落と同じ位置、
/// 2行目は本文の左端（72pt）。
fn i2_leftchars_with_hangingchars() {
    let pages = laid_out("leftchars-hangingchars.docx");
    let gs = glyphs(&pages.pages[0]);
    let lines = find_all(&gs, "登録番号（");
    let start = |y: f32| gs.iter().filter(|g| near(g.y, y, 0.5)).map(|g| g.x).fold(f32::MAX, f32::min);
    let (chars, reference) = (start(lines[0][0].y), start(lines[1][0].y));
    let second = gs.iter().filter(|g| g.y > lines[0][0].y + 1.0 && g.y < lines[1][0].y - 1.0).map(|g| g.x).fold(f32::MAX, f32::min);
    assert!(near(chars, reference, 0.5) && near(second, 72.0, 0.5), "first line at x {chars:.2} (left=0 hanging=283: {reference:.2}), second line at {second:.2}; Word: {reference:.2} and 72");
}

/// I4「両端揃えの番号付き段落が折り返すと、番号の後のタブが既定のタブ位置まで
/// 飛び、本文がセルからはみ出す」: Word は「②」の後の本文を左インデント
/// （225tw）から始める（左揃えの行③と同じ位置）。
fn i4_justified_list_label_tab() {
    let pages = laid_out("justify-list-label.docx");
    let rows = find_all(&glyphs(&pages.pages[0]), "折り返す長");
    let (justified, left) = (rows[0][0].x, rows[1][0].x);
    assert!(near(justified, left, 1.0), "after ② the text starts at x {justified:.2}; Word: {left:.2} (as the left-aligned row)");
}

fn pitches(page: &Page) -> Vec<f32> {
    baselines(page).windows(2).map(|w| w[1] - w[0]).collect()
}

/// L1「ＭＳ 明朝・ＭＳ ゴシック（と Meiryo UI）の1行が Word より低く、文書が
/// 詰まってページが減る」: グリッドなしのＭＳ 明朝 10.5pt を Word は 13.5／13.75pt
/// の交互（平均 13.66pt）で並べ、65 行は2ページになる。
fn l1_ms_mincho_line_pitch_without_grid() {
    let pages = laid_out("r1-msmincho-nogrid.docx");
    let p = pitches(&pages.pages[0]);
    let mean = p.iter().sum::<f32>() / p.len() as f32;
    assert!(near(mean, 13.625, 0.25) && pages.pages.len() == 2, "line pitch {mean:.2}pt, {} pages; Word: 13.5/13.75pt, 2 pages", pages.pages.len());
}

/// L2「表のセルの中の行が、文書の行グリッドに合わない（adjustLineHeightInTable
/// を無視）」: Word はセルの中の行も 18pt のグリッドで並べる。
fn l2_table_rows_on_line_grid() {
    let pages = laid_out("A-grid-in-table-adjust.docx");
    let gs = glyphs(&pages.pages[0]);
    let ys: Vec<f32> = (1..=4).map(|i| find(&gs, &format!("行{i}"))[0].y).collect();
    let p: Vec<f32> = ys.windows(2).map(|w| w[1] - w[0]).collect();
    assert!(p.iter().all(|d| near(*d, 18.0, 0.5)), "the table's rows {p:?}pt apart; Word: 18pt (the grid)");
}

/// L3「種類（w:type）の無い docGrid を行グリッドとして扱い、行送りが倍になる」:
/// 省略時は default（グリッドなし）で、Word は最小値 16.1pt の段落を約 16pt
/// 間隔で並べる（049 の実測）。
fn l3_doc_grid_without_type_is_no_grid() {
    let pages = laid_out("docgrid_no_type.docx");
    let p = pitches(&pages.pages[0]);
    assert!(p.iter().all(|d| near(*d, 16.1, 0.6)), "paragraphs {:?}pt apart; Word: about 16pt", &p[..3.min(p.len())]);
}

/// L4「「文字数と行数を指定」の字送り（charSpace）が反映されず、1行の字数が
/// Word と違う」: charSpace="-1193"（1/4096pt 単位で −0.29pt）で字送りが詰まり、
/// Word は43字の段落を1行に収める（5段落で5行）。
fn l4_char_space_applied() {
    let pages = laid_out("repro-A-charspace.docx");
    let n = baselines(&pages.pages[0]).len();
    assert_eq!(n, 5, "lines of 5 paragraphs of 43 characters; Word: 5");
}

/// L5「行グリッドのある本文で、14pt 程度のＭＳ 明朝の行がグリッド1本ぶんしか
/// 取らず、後ろが詰まる」: Word は 14pt の見出しをグリッド（17.1pt）2本ぶんで
/// 組み、3行目は1行目の 3 × 17.1pt 下。
fn l5_ms_mincho_14pt_takes_two_grid_lines() {
    let pages = laid_out("hdrftr-grid-msmincho-14pt.docx");
    let gs = glyphs(&pages.pages[0]);
    let gap = find(&gs, "3行目（")[0].y - find(&gs, "一行目")[0].y;
    assert!(near(gap, 51.3, 1.0), "the third line {gap:.2}pt below the first; Word: 51.3pt (the heading takes 2 grid lines)");
}

/// L6「行グリッドのある節で、「1.5 行」などの倍数の行間がグリッドの整数倍に
/// 丸められ、行が詰まる」: グリッド 18pt で 1.5 行の段落を Word は 27pt 間隔で並べる。
fn l6_grid_auto_multiple() {
    let pages = laid_out("grid-auto-multiple.docx");
    let gs = glyphs(&pages.pages[0]);
    let gap = find(&gs, "行の行 2")[0].y - find(&gs, "行の行 1")[0].y;
    assert!(near(gap, 27.0, 0.5), "1.5-line paragraphs {gap:.2}pt apart; Word: 27pt");
}

/// M1「解像度の高い画像を小さく貼ると、元の解像度のまま間引いて描くため、
/// ガビガビになる」: 1600px 四方の画像（上部は 1px の縦線）を 1 インチで貼る。
/// 正しく縮小（面積平均）すれば縦線の帯は一様な灰色になり、列ごとの平均の
/// 明るさのばらつきは小さい。ウィンドウを描いた画素で、その帯の列ごとの平均の
/// 標準偏差が、元の画像を面積平均で縮小した場合の値を 8（256 階調）より大きく
/// 超えないこと。
fn m1_downscaled_picture_is_averaged() {
    let mut s = open("image-downscale.docx");
    let pages = pages_of(&mut s);
    let (media, x, y, w, h) = pages.pages[0].items.iter().find_map(|i| if let Item::Picture { media, x, y, w, h } = i { Some((*media, *x, *y, *w, *h)) } else { None }).expect("the 1-inch picture");
    let (origin, k) = s.with(|e, _| e.page_origin(0)).expect("page 1 shown");
    let data = s.with(|e, _| e.presentation().media(media).map(|m| m.data.clone())).expect("the picture's data");
    let source = image::load_from_memory(&data).expect("a PNG").to_luma8();
    let (width, pixels) = s.picture();
    let scale = width as f32 / 1200.0;
    let height = pixels.len() as u32 / 4 / width;
    let luma = |px: u32, py: u32| -> f32 {
        let i = ((py * width + px) * 4) as usize;
        pixels[i] as f32 * 0.299 + pixels[i + 1] as f32 * 0.587 + pixels[i + 2] as f32 * 0.114
    };
    let (left, top) = (((f32::from(origin.x) + x * k) * scale) as u32, ((f32::from(origin.y) + y * k) * scale) as u32);
    let (cols, rows) = ((w * k * scale) as u32, (h * k * scale) as u32);
    assert!(left + cols < width && top + rows < height, "the picture ({left}, {top}) {cols}×{rows} is in the window ({width}×{height})");
    let band = (rows / 10).max(4);
    let shown: Vec<f32> = (2..cols - 2).map(|cx| (2..band).map(|cy| luma(left + cx, top + cy)).sum::<f32>() / (band - 2) as f32).collect();
    let (sw, sh) = source.dimensions();
    let averaged: Vec<f32> = (2..cols - 2)
        .map(|cx| {
            let (x0, x1) = (cx * sw / cols, (cx + 1) * sw / cols);
            let (y0, y1) = (2 * sh / rows, band * sh / rows);
            let mut sum = 0.0;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    sum += source.get_pixel(sx, sy).0[0] as f32;
                }
            }
            sum / ((x1 - x0) * (y1 - y0)).max(1) as f32
        })
        .collect();
    let spread = |v: &[f32]| {
        let mean = v.iter().sum::<f32>() / v.len() as f32;
        (v.iter().map(|a| (a - mean) * (a - mean)).sum::<f32>() / v.len() as f32).sqrt()
    };
    let (drawn, reference) = (spread(&shown), spread(&averaged));
    assert!(drawn <= reference + 8.0, "the band of 1px lines drawn {cols}×{rows}px: column brightness spread {drawn:.1}; averaged down: {reference:.1}");
}

/// P1「ページ末尾の「改ページだけの行」「節区切りだけの空段落」が次のページに
/// 送られ、白紙のページができる」: Word はこれらを前のページに残し、
/// 3つの文書はどれも2ページ。
fn p1_break_only_line_at_page_bottom() {
    let mut failures = Vec::new();
    for file in ["repro-F-pagebreak-line-at-bottom.docx", "repro-G-section-mark-after-table.docx", "sectpara_39.docx"] {
        let n = laid_out(file).pages.len();
        if n != 2 {
            failures.push(format!("{file}: {n} pages; Word: 2"));
        }
    }
    check(failures);
}

/// S2「重なった図形の前後が relativeHeight でなく文書の順になり、本文が塗りの
/// 図形に隠れる」: Word は relativeHeight の大きいテキストボックスを橙の四角より
/// 手前に描く（page_view は手前の図形を items の順に描く）。
fn s2_relative_height_order() {
    let pages = laid_out("r2-zorder.docx");
    let order: Vec<bool> = pages.pages[0].items.iter().filter_map(|i| if let Item::Shape { shape, .. } = i { Some(shape.text_body().is_some()) } else { None }).collect();
    assert_eq!(order, vec![false, true], "drawn in this order (true: the text box); Word: the orange rectangle, then the text box on top");
}

/// S4「表のセルの中の図形（layoutInCell）で、page 基準の位置をセル基準にしない」:
/// Word は layoutInCell="1" の図形の page 基準の位置をセルの左上から測り、
/// 大かっこを表のセルの中に描く。
fn s4_layout_in_cell_page_relative() {
    let pages = laid_out("r7-shape-in-cell-page-relative.docx");
    let page = &pages.pages[0];
    let (cx, cy) = page.items.iter().find_map(|i| if let Item::Shape { x, y, w, h, .. } = i { Some((x + w / 2.0, y + h / 2.0)) } else { None }).expect("the bracket");
    let rules: Vec<(f32, f32, f32, f32)> = page.items.iter().filter_map(|i| if let Item::Rect { x, y, w, h, .. } = i { Some((*x, *y, *w, *h)) } else { None }).collect();
    let (l, t) = rules.iter().fold((f32::MAX, f32::MAX), |(l, t), r| (l.min(r.0), t.min(r.1)));
    let (r, b) = rules.iter().fold((0.0f32, 0.0f32), |(r, b), q| (r.max(q.0 + q.2), b.max(q.1 + q.3)));
    assert!(cx >= l && cx <= r && cy >= t && cy <= b + 1.0, "the bracket's centre at ({cx:.1}, {cy:.1}); Word: in the cell ({l:.1}, {t:.1})-({r:.1}, {b:.1})");
}

/// S8「罫線の種類 dashDotStroked を細い実線で描く」: Word は sz によらず幅約 3.5pt
/// の帯（斜めの短い線が並ぶ）を描く。sz=4 の1つ目の表の上の罫線の帯の太さが 3pt 以上。
fn s8_dash_dot_stroked_border_band() {
    let pages = laid_out("shapes-3-dashdotstroked-border.docx");
    let page = &pages.pages[0];
    let top = page
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Rect { y, w, .. } | Item::Shading { y, w, .. } if *w > 100.0 => Some(*y),
            _ => None,
        })
        .fold(f32::MAX, f32::min);
    let band: Vec<(f32, f32)> = page
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Rect { y, w, h, .. } | Item::Shading { y, w, h, .. } if *w > 2.0 && *y < top + 6.0 && *y + *h > top - 6.0 && *h < 8.0 => Some((*y, *y + *h)),
            Item::Stroke { y, y2, w, .. } if *y < top + 6.0 && *y > top - 6.0 => Some((y.min(*y2) - w / 2.0, y.max(*y2) + w / 2.0)),
            _ => None,
        })
        .collect();
    let thickness = band.iter().map(|b| b.1).fold(f32::MIN, f32::max) - band.iter().map(|b| b.0).fold(f32::MAX, f32::min);
    assert!(thickness >= 3.0, "the top border of the dashDotStroked sz=4 cell is {thickness:.2}pt thick; Word: a band about 3.5pt");
}

/// S9「四角などで回り込む図形の横に組まれる段落では、その段落の図形が描かれない」:
/// Word は回り込みの箱の横の段落にある赤い横線も描く（赤い線は2本）。
fn s9_shape_beside_square_wrap_drawn() {
    let pages = laid_out("shapes-4-square-wrap-hides-shapes.docx");
    let red = pages.pages.iter().flat_map(|p| &p.items).filter(|i| matches!(i, Item::Shape { shape, .. } if matches!(&shape.kind, ibis_doc_model::ShapeKind::Line(s) if s.color.rgb == ibis_doc_model::Rgb(0xff0000)))).count();
    assert_eq!(red, 2, "red lines drawn; Word: 2 (the one beside the box too)");
}

/// S10「上下に回り込む（wrapTopAndBottom）図形の縦位置（posOffset）を使わず、
/// アンカー段落の文字より前に置く」: Word は箱を段落の上端から 20pt 下に置き、
/// アンカー段落の1行目の文字はその上に出る。
fn s10_top_and_bottom_offset() {
    let mut failures = Vec::new();
    let pages = laid_out("shapes-6-topandbottom-offset.docx");
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let box_y = page.items.iter().find_map(|i| if let Item::Shape { y, .. } = i { Some(*y) } else { None }).expect("the box");
    let before = find(&gs, "Paragraph before")[0].y;
    let para_top = page.items.iter().find_map(|i| if let Item::Line { y, h, .. } = i && before > *y && before <= y + h { Some(y + h) } else { None }).expect("the first paragraph's line");
    let anchor = find(&gs, "Anchor paragraph")[0].y;
    if !near(box_y, para_top + 20.0, 1.5) || anchor > box_y {
        failures.push(format!("shapes-6: the box at y {box_y:.2}, the anchor paragraph's first line at {anchor:.2}, its top {para_top:.2}; Word: the box at {:.2}, the line above it", para_top + 20.0));
    }
    let pages = laid_out("hdrftr-topandbottom-offset.docx");
    let page = &pages.pages[0];
    let box_y = page.items.iter().find_map(|i| if let Item::Shape { y, .. } = i { Some(*y) } else { None }).expect("the box");
    let text = find(&glyphs(page), "様式第９号の参考")[0].y;
    if text > box_y {
        failures.push(format!("hdrftr: 様式第９号の参考 at y {text:.2}, below the box's top {box_y:.2}; Word: above the box"));
    }
    check(failures);
}

/// S14「「図形を文字に合わせる」（spAutoFit）のテキストボックスを保存時の大きさで
/// 描き、白い塗りが線や罫線を隠す」: Word は箱を1行の文字（10.5pt）と上下の
/// 内余白（3.6pt ずつ）の高さに縮める。行の高さの差を見込んで 30pt 以下。
fn s14_autofit_box_fits_text() {
    let pages = laid_out("spautofit-textbox.docx");
    let h = pages.pages[0].items.iter().find_map(|i| if let Item::Shape { shape, h, .. } = i && shape.text_body().is_some() { Some(*h) } else { None }).expect("the text box");
    assert!(h <= 30.0, "the box is {h:.1}pt high (saved 110.6pt); Word: fitted to its one line");
}

/// S5「VML だけで描かれた線（w:pict の v:line）が表示されない」（描画）:
/// v:line は from="0,9pt" to="450pt,9pt"、mso-position-vertical-relative:text なので、
/// Word は線だけの段落（前の段落の行の箱の下端が上端）から 9pt 下に、幅 450pt・
/// 太さ 1pt の線を描く。組版の図形の位置（±1pt）と、その高さの画素の行に暗い画素が
/// 8割以上並ぶことで確かめる。
fn s5_vml_line_drawn() {
    let mut s = open("vml_line_only.docx");
    let pages = pages_of(&mut s);
    let page = &pages.pages[0];
    let lines: Vec<(f32, f32, f32, f32)> = page.items.iter().filter_map(|i| if let Item::Shape { x, y, w, h, .. } = i { Some((*x, *y, *w, *h)) } else { None }).collect();
    let gs = glyphs(page);
    let line_bottom = |text: &str| {
        let baseline = find(&gs, text)[0].y;
        page.items.iter().find_map(|i| if let Item::Line { x, y, h, .. } = i && baseline > *y && baseline <= y + h { Some((*x, y + h)) } else { None }).expect("the paragraph's line box")
    };
    let mut failures = Vec::new();
    for (i, before) in ["線の上の段落", "線の下の段落"].into_iter().enumerate() {
        let (left, bottom) = line_bottom(before);
        let want = bottom + 9.0;
        let laid = lines.get(i).copied();
        if !laid.is_some_and(|l| near(l.1, want, 1.0) && near(l.2, 450.0, 1.0) && l.3 < 1.0) {
            failures.push(format!("line {}: laid out at {laid:?}; Word: level, 450pt long, at y {want:.2}", i + 1));
        }
        let crop = page_pixels(&mut s, 0, (left, want - 2.0, 450.0, 4.0));
        let mut per_row = vec![0usize; crop.1];
        for (_, r) in pick(&crop, |[r, g, b]| (r as u32 + g as u32 + b as u32) < 300) {
            per_row[r] += 1;
        }
        let longest = per_row.iter().max().copied().unwrap_or(0);
        if longest < crop.0 * 8 / 10 {
            failures.push(format!("line {}: around y {want:.2} the darkest pixel row has {longest} of {} dark; Word: a line across", i + 1, crop.0));
        }
    }
    check(failures);
}

/// S11「テキストボックスの縦書き（bodyPr の vert="eaVert"・"vert"）を読まず、横書きで
/// 描く」（描画）: Word は eaVert と vert の箱の文字「架空の書類の写し」（9pt × 8字）を
/// 縦に並べて描く。箱の内側（枠の線を除く）の文字の画素の範囲が、縦書きなら
/// 縦長（高さが幅の3倍以上）、横書きの horz の箱は横長。
fn s11_vertical_text_box_drawn_vertical() {
    let mut s = open("vtext-textbox-eavert.docx");
    let pages = pages_of(&mut s);
    let page = &pages.pages[0];
    let frames: Vec<(u64, (f32, f32, f32, f32))> = page.items.iter().filter_map(|i| if let Item::Shape { shape, x, y, w, h } = i { Some((shape.id.0, (*x, *y, *w, *h))) } else { None }).collect();
    let mut failures = Vec::new();
    for (id, vertical) in [(101, false), (102, true), (103, true)] {
        let Some((_, (x, y, w, h))) = frames.iter().find(|f| f.0 == id).copied() else {
            failures.push(format!("box {id} not laid out"));
            continue;
        };
        let crop = page_pixels(&mut s, 0, (x + 3.0, y + 3.0, w - 6.0, h - 6.0));
        let ink = pick(&crop, |[r, g, b]| (r as u32 + g as u32 + b as u32) < 384);
        if ink.is_empty() {
            failures.push(format!("box {id}: no text drawn"));
            continue;
        }
        let (cw, ch) = (ink.iter().map(|p| p.0).max().unwrap() - ink.iter().map(|p| p.0).min().unwrap() + 1, ink.iter().map(|p| p.1).max().unwrap() - ink.iter().map(|p| p.1).min().unwrap() + 1);
        let drawn_vertical = ch >= cw * 3;
        let drawn_level = cw >= ch * 3;
        if (vertical && !drawn_vertical) || (!vertical && !drawn_level) {
            failures.push(format!("box {id}: the text drawn over {cw}×{ch}px; Word: {}", if vertical { "a column (tall)" } else { "a line (wide)" }));
        }
    }
    check(failures);
}

/// S13「180 度回した図形の文字が逆さまになり、全行が1行に重なる（upright を読まない）」
/// （描画）: 図形を 180 度回しても upright="1" の文字は正立のまま、Word では4行が
/// 上から並ぶ。回していない同じ吹き出し（shape-rot0-reference.docx、180 度回しても
/// 長方形の部分は同じ場所）と、箱の内側（枠の線を 3pt 除く）の赤い文字の画素を比べる：
/// 行の数（文字のある行の帯）が同じ4本で、行ごと・列ごとの赤い画素の数の分布の差が
/// 全体の 15% 以内（逆さまなら右寄せ・上下反転になり、重なれば帯が1本になる）。
fn s13_turned_upright_text_drawn_level() {
    let red_text = |[r, g, b]: [u8; 3]| r > 150 && g < 110 && b < 110;
    let profile = |file: &str| -> (usize, Vec<usize>, Vec<usize>) {
        let mut s = open(file);
        let pages = pages_of(&mut s);
        let (x, y, w, h) = pages.pages[0].items.iter().find_map(|i| if let Item::Shape { x, y, w, h, .. } = i { Some((*x, *y, *w, *h)) } else { None }).expect("the callout");
        let crop = page_pixels(&mut s, 0, (x + 3.0, y + 3.0, w - 6.0, h - 6.0));
        let red = pick(&crop, red_text);
        let (mut rows, mut cols) = (vec![0usize; crop.1], vec![0usize; crop.0]);
        for &(c, r) in &red {
            rows[r] += 1;
            cols[c] += 1;
        }
        (bands(&red, crop.1), rows, cols)
    };
    let (turned_bands, turned_rows, turned_cols) = profile("shape-rot180-upright.docx");
    let (level_bands, level_rows, level_cols) = profile("shape-rot0-reference.docx");
    let differ = |a: &[usize], b: &[usize]| a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).sum::<usize>() as f32 / b.iter().sum::<usize>().max(1) as f32;
    let (rows, cols) = (differ(&turned_rows, &level_rows), differ(&turned_cols, &level_cols));
    assert!(turned_bands == level_bands && rows <= 0.15 && cols <= 0.15, "turned 180° with upright=1: {turned_bands} bands of text (unturned: {level_bands}), rows differ by {:.0}%, columns by {:.0}%; Word: drawn as the unturned callout", rows * 100.0, cols * 100.0);
}

/// docx-C001-b1「settings の doNotUseHTMLParagraphAutoSpacing があるのに、隣り合う
/// 段落の前の after と次の before を大きい方だけにして足さない」: Word は足す。
/// 固定 15pt の行で before 5pt・after 5pt の段落は 15 + 5 + 5 = 25pt 間隔、
/// 見出し（before 20pt）はその前の段落（after 5pt）の 15 + 5 + 20 = 40pt 下
/// （Mac 版 Word の PDF の実測は 25.0pt と 39.8pt）。許容差 1pt。
/// todo-docx の T4（beforeAutospacing）とは別のきっかけ（settings の doNotUseHTMLParagraphAutoSpacing）。
fn c001_b1_html_autospacing_sums_after_and_before() {
    let pages = laid_out("html-autospacing-sum.docx");
    let gs = glyphs(&pages.pages[0]);
    let y = |text: &str| find(&gs, text)[0].y;
    let mut failures = Vec::new();
    for i in 1..6 {
        let gap = y(&format!("A-{}", i + 1)) - y(&format!("A-{i}"));
        if !near(gap, 25.0, 1.0) {
            failures.push(format!("A-{i} to A-{}: {gap:.2}pt; Word: 25.0pt (15 + after 5 + before 5)", i + 1));
        }
    }
    let gap = y("グループB") - y("A-6");
    if !near(gap, 39.8, 1.0) {
        failures.push(format!("A-6 to the heading: {gap:.2}pt; Word: 39.8pt (15 + after 5 + before 20)"));
    }
    check(failures);
}

/// docx-C002-b1・docx-C014-b2「自動調整の表（tblW auto）で、セルの tcW が tblGrid の
/// 列幅と食い違うと、先頭の見出し列を狭く組み、「ダミー項目一」が折り返す」:
/// Word は tblGrid の列幅のまま組み（食い違いのない T2 と同じ）、見出しは 1 行。
fn c002_b1_autofit_label_column_keeps_one_line() {
    c002_b1_autofit_label_column_keeps_one_line_check("autofit-label-col-narrow.docx");
}

fn c002_b1_autofit_label_column_keeps_one_line_check(file: &str) {
    let pages = laid_out(file);
    let gs = glyphs(&pages.pages[0]);
    let mut failures = Vec::new();
    for label in ["ダミー項目一", "ダミー項目二", "ダミー項目三", "ダミー項目四"] {
        for (table, cell) in ["T1", "T2"].into_iter().zip(find_all(&gs, label)) {
            let rows = cell.iter().map(|g| (g.y * 2.0).round() as i64).collect::<std::collections::BTreeSet<_>>().len();
            if rows != 1 {
                failures.push(format!("{table} {label}: wrapped over {rows} lines; Word: 1 line (as T2 of the same file)"));
            }
        }
    }
    check(failures);
}

/// The x of the page's vertical rules (table borders) whose top is in
/// `top..bottom`, left to right.
fn rule_xs(page: &Page, top: f32, bottom: f32) -> Vec<f32> {
    let mut xs: Vec<f32> = page.items.iter().filter_map(|i| if let Item::Rect { x, y, w, h, .. } = i && *w < 1.5 && *h > 5.0 && *y >= top - 1.0 && *y < bottom { Some(*x) } else { None }).collect();
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 0.6);
    xs
}

/// docx-C014-b1「割合（pct）幅の自動調整の表で、字下げのある段落が狭い先頭の列を
/// 押し広げる」: Word は希望幅（割合）どおりで、字下げのある T1・T2 も字下げの
/// 無い T3 と列の境目が同じ（先頭の列の右端は Word の PDF で 97.2pt）。許容差 1.5pt。
fn c014_b1_autofit_indent_does_not_widen_column() {
    let pages = laid_out("autofit-indent-narrow-col.docx");
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let tops: Vec<f32> = ["T1:", "T2:", "T3:"].iter().map(|l| find(&gs, l)[0].y).collect();
    let tables: Vec<Vec<f32>> = (0..3).map(|i| rule_xs(page, tops[i], tops.get(i + 1).copied().unwrap_or(f32::MAX))).collect();
    let mut failures = Vec::new();
    for (i, name) in ["T1 (numbered)", "T2 (indent only)"].into_iter().enumerate() {
        let differs = tables[i].len() != tables[2].len() || tables[i].iter().zip(&tables[2]).any(|(a, b)| !near(*a, *b, 1.5));
        if differs {
            failures.push(format!("{name}: column edges {:.1?}; Word: as T3 (no indent) {:.1?}", tables[i], tables[2]));
        }
    }
    check(failures);
}

/// docx-C014-b2「自動調整の表で、セルの希望幅（tcW）が tblGrid と食い違う（Word が本文幅
/// より広い希望幅を縮めて tblGrid に書いた形）と、列の境目が Word とずれ、表も狭くなる」:
/// Word は tblGrid の列幅どおりで、T1（食い違い）も T2（tcW が tblGrid と同じ）と
/// 同じ境目に組む（Mac 版 Word の PDF で 1px 以内）。許容差 1.5pt。
fn c014_b2_autofit_tcw_grid_mismatch_uses_grid() {
    let pages = laid_out("autofit-tcw-grid-mismatch.docx");
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let top2 = find(&gs, "T2:")[0].y;
    let t1 = rule_xs(page, 0.0, top2);
    let t2 = rule_xs(page, top2, f32::MAX);
    let differs = t1.len() != t2.len() || t1.iter().zip(&t2).any(|(a, b)| !near(*a, *b, 1.5));
    assert!(!differs, "T1 (tcW over tblGrid): column edges {t1:.1?}; Word: as T2 (tcW = tblGrid) {t2:.1?}");
}

/// docx-C010-b1「半角カナが、欧文の書体（ascii・hAnsi）が ＭＳ 系でない run で
/// 1 字 1em の幅で進む」: Word は半角カナを東アジアの書体（ＭＳ 明朝）の半角、
/// 0.5em（10.5pt の字で 5.25pt）で進める。19 字の run の幅は 99.75pt。
/// 許容差 1.5pt。
/// todo-docx の W2・W4（ＭＳ 系の半角、[x]）は書体がＭＳ 系の run。欧文の書体が別の run の半角カナは残る。
fn c010_b1_halfwidth_kana_advance_half_em() {
    c010_b1_halfwidth_kana_advance_half_em_check("halfwidth-kana-advance.docx");
}

fn c010_b1_halfwidth_kana_advance_half_em_check(file: &str) {
    let pages = laid_out(file);
    let gs = glyphs(&pages.pages[0]);
    let mut failures = Vec::new();
    for (row, bar) in find_all(&gs, "|").into_iter().enumerate() {
        let first = find_all(&gs, "ﾀﾞﾐｰﾓｼﾞﾚﾂｱｲｳｴｵｶｷｸｹｺ")[row][0].x;
        let width = bar[0].x - first;
        if !near(width, 99.75, 1.5) {
            failures.push(format!("row {}: the 19 half-width kana span {width:.2}pt; Word: 99.75pt (0.5em each)", row + 1));
        }
    }
    check(failures);
}

/// docx-C011-b1「入れ子の表がページ下端をまたぐと、はみ出した行の縦罫線が下余白・
/// フッターの位置まで伸びる」: Word は線をページの本文の下端（A4・余白 72pt で 770pt）
/// で止める。
fn c011_b1_nested_table_rules_stop_at_page_bottom() {
    c011_b1_nested_table_rules_stop_at_page_bottom_check("nested-table-page-split.docx");
}

fn c011_b1_nested_table_rules_stop_at_page_bottom_check(file: &str) {
    let pages = laid_out(file);
    let mut failures = Vec::new();
    for (i, page) in pages.pages.iter().enumerate() {
        let bottom = page.height - 72.0;
        for item in &page.items {
            if let Item::Rect { x, y, w, h, .. } = item && y + h > bottom + 1.0 {
                failures.push(format!("p{}: a rule at x {x:.1} ({w:.1} × {h:.1}) from y {y:.1} to {:.1}; Word: nothing below the text area's bottom {bottom:.1}", i + 1, y + h));
            }
        }
    }
    check(failures);
}

/// docx-C011-b2「四角で回り込む箱（wrapSquare）の直後の空の段落が、箱の下へ回らず
/// 高さも取らない」: 箱の幅は本文幅とほぼ同じで横に余地が無く、Word は箱の高さの範囲に
/// 入る行を箱の下へ送る。空の段落 9 個も 1 行ずつ高さを取り、「後続の本文 A」は
/// 箱の下端から 9 行分（9 × 行送り）下に来る。
/// todo-docx の F2（浮動の表の下の空の段落、[x]）と同じ仕組みの、回り込む箱の場合。
fn c011_b2_blank_paragraphs_after_wrap_square_take_height() {
    c011_b2_blank_paragraphs_after_wrap_square_take_height_check("blank-paras-after-wrapsquare.docx");
}

fn c011_b2_blank_paragraphs_after_wrap_square_take_height_check(file: &str) {
    let pages = laid_out(file);
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let (box_y, box_h) = page.items.iter().find_map(|i| if let Item::Shape { y, h, .. } = i { Some((*y, *h)) } else { None }).expect("the box");
    let pitch = find(&gs, "先行段落 2")[0].y - find(&gs, "先行段落 1")[0].y;
    let below = find(&gs, "後続の本文 A")[0].y - (box_y + box_h);
    assert!(below >= 9.0 * pitch - 2.0, "the text after the nine empty paragraphs {below:.1}pt below the box; Word: at least 9 lines ({:.1}pt) below it", 9.0 * pitch);
}

/// docx-C013-b1「フッターのページ番号の枠の段落（w:framePr）の高さをフッターの高さに
/// 数え、本文の下端が上がって、1 ページぶんの高さの表の行が割れる」: Word は枠の段落が
/// 流れの高さを取らず、6 つの表は 1 ページに 1 つずつで 6 ページ。表の見出し行は
/// どのページの上端にもある（行の細片が前のページから続かない）。
/// todo-docx の H3（枠の位置）・P2（フッターが余白を越えたとき）は [x] だが、枠の段落が高さを取る崩れは残る（直しの漏れ）。
fn c013_b1_footer_frame_paragraph_takes_no_height() {
    c013_b1_footer_frame_paragraph_takes_no_height_check("footer-framepr-height.docx");
}

fn c013_b1_footer_frame_paragraph_takes_no_height_check(file: &str) {
    let pages = laid_out(file);
    let mut failures = Vec::new();
    if pages.pages.len() != 6 {
        failures.push(format!("{} pages; Word: 6 (one table to a page)", pages.pages.len()));
    }
    for (i, page) in pages.pages.iter().enumerate() {
        let first = glyphs(page).into_iter().filter(|g| !g.c.is_whitespace()).min_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
        if !first.as_ref().is_some_and(|g| g.c == 'A') {
            failures.push(format!("p{}: starts with {:?}; Word: the table's heading row (A)", i + 1, first.as_ref().map(|g| g.c)));
        }
    }
    check(failures);
}

/// docx-C015-b1「右寄せの段落の末尾の半角空白を、右寄せの幅に数えない」: Word は
/// 段落の最後の行の末尾の半角空白も数え、「日」の右端は末尾の空白 0・1・2・6 個で
/// 右余白から 0・5.25・10.5・31.5pt 左になる（正解の PDF で半角空白 1 個 = 5.25pt）。
/// 許容差 0.6pt。
fn c015_b1_right_aligned_trailing_halfwidth_spaces_counted() {
    let pages = laid_out("right-trailing-space.docx");
    let gs = glyphs(&pages.pages[0]);
    let days = find_all(&gs, "日");
    assert_eq!(days.len(), 4, "four right-aligned lines");
    let shifts: Vec<f32> = days.iter().map(|d| d[0].x - days[0][0].x).collect();
    let want = [0.0, -5.25, -10.5, -31.5];
    assert!(shifts.iter().zip(want).all(|(s, w)| near(*s, w, 0.6)), "『日』 shifted {shifts:.2?}pt for 0, 1, 2, 6 trailing half-width spaces; Word: {want:?}pt");
}

/// docx-C015-b2「文字グリッドの字送り（charSpace）が表のセルの中で効かない」:
/// docGrid linesAndChars・charSpace=6651（1.624pt）で、Word は表のセルの中の全角文字にも
/// 字送りを足す。同じ「あいうえおかきくけこ」の幅が、表の外と表のセルの中で同じ。
/// 許容差 0.6pt。
/// todo-docx の L4（charSpace、[x]）は表の外。表のセルの中は残る。
fn c015_b2_char_space_applies_in_table_cell() {
    c015_b2_char_space_applies_in_table_cell_check("charspace-in-cell.docx");
}

fn c015_b2_char_space_applies_in_table_cell_check(file: &str) {
    let pages = laid_out(file);
    let gs = glyphs(&pages.pages[0]);
    let spans: Vec<f32> = find_all(&gs, "あいうえおかきくけこ").iter().map(|r| r[9].x - r[0].x).collect();
    assert!(spans.len() == 2 && near(spans[1], spans[0], 0.6), "the ten characters span {spans:.2?}pt (outside the table, in its cell); Word: the same in both (the character grid's spacing is added in the cell)");
}

/// docx-C015-b3「右上の浮動の表の真下の桁に始まる行が、字下げごと左端へ戻る」:
/// 段落の字下げ（firstLine 6378tw）で行頭が浮動の表の桁にかかるとき、Word は
/// 字下げを保ち、「令和」を左余白（85.05pt）から 318.9pt 右の 403.95pt に置く。
/// 許容差 1.5pt。
fn c015_b3_floating_table_keeps_first_line_indent() {
    c015_b3_floating_table_keeps_first_line_indent_check("float-table-indent.docx");
}

fn c015_b3_floating_table_keeps_first_line_indent_check(file: &str) {
    let pages = laid_out(file);
    let gs = glyphs(&pages.pages[0]);
    let x = find(&gs, "令和")[0].x;
    assert!(near(x, 403.95, 1.5), "the date line starts at x {x:.2}; Word: 403.95pt (left margin 85.05 + first-line indent 318.9)");
}

/// docx-C016-b1「行間が「最小値 0」（w:line="0" lineRule="atLeast"）の段落を行グリッドに
/// 乗せ、Word より行送りが広くなる」: 行グリッド（lines・18pt）があっても、Word は
/// 11pt の ＭＳ 明朝の行を自然な高さで並べ、行送りは約 14.25pt（正解の PDF の本文）。
/// 許容差 0.7pt。
/// todo-docx の L3（種類の無い docGrid、[x]）の派生。
fn c016_b1_at_least_zero_line_not_snapped_to_grid() {
    c016_b1_at_least_zero_line_not_snapped_to_grid_check("atleast0-snaps-grid.docx");
}

fn c016_b1_at_least_zero_line_not_snapped_to_grid_check(file: &str) {
    let pages = laid_out(file);
    let p = pitches(&pages.pages[0]);
    assert!(!p.is_empty() && p.iter().all(|d| near(*d, 14.25, 0.7)), "paragraphs {:.2?}pt apart; Word: 14.25pt (not the 18pt grid)", &p[..4.min(p.len())]);
}

/// docx-C016-b2「行グリッドで 1 行が 2 本以上のとき、倍数の行間（1.5 行）を掛けすぎる」:
/// グリッド 14.5pt に 12pt の ＭＳ ゴシック（1 行 15.6pt で 2 本ぶん）の 1.5 行の段落を、
/// Word は 2 本ぶん 29pt のまま並べる（1 本に収まる行だけに 1.5 を掛ける）。
/// 許容差 1pt。
/// todo-docx の L6（倍数の行間、[x]）は 1 グリッドに収まる行まで。2 グリッド以上の行は残る。
fn c016_b2_multiple_line_spacing_on_two_grid_lines() {
    c016_b2_multiple_line_spacing_on_two_grid_lines_check("multiple-2grids.docx");
}

fn c016_b2_multiple_line_spacing_on_two_grid_lines_check(file: &str) {
    let pages = laid_out(file);
    let p = pitches(&pages.pages[0]);
    assert!(p.len() >= 6 && p[..6].iter().all(|d| near(*d, 29.0, 1.0)), "paragraphs {:.2?}pt apart; Word: 29pt (2 grid lines, no 1.5 on top)", &p[..4.min(p.len())]);
}

/// docx-C016-b3「グリッドの無い文書で、空の段落の高さが ＭＳ 明朝の約 1.3 倍にならず
/// 1.0 倍のまま（文字のある行より低い）」: 文字のある行 1 つと空の段落 4 つの組は、
/// Word では空の段落も文字のある行と同じ 14.25pt で、5 × 14.25 = 71.25pt。
/// 許容差 2pt。
/// todo-docx の L1（ＭＳ 明朝の行の高さ、[x]）の派生（空の段落の行）。
fn c016_b3_empty_paragraph_as_tall_as_text_line() {
    c016_b3_empty_paragraph_as_tall_as_text_line_check("empty-para-height.docx");
}

fn c016_b3_empty_paragraph_as_tall_as_text_line_check(file: &str) {
    let pages = laid_out(file);
    let gs = glyphs(&pages.pages[0]);
    let mut failures = Vec::new();
    for n in 1..6 {
        let gap = find(&gs, &format!("段落 {}。", n + 1))[0].y - find(&gs, &format!("段落 {n}。"))[0].y;
        if !near(gap, 71.25, 2.0) {
            failures.push(format!("paragraph {n} to {}: {gap:.2}pt; Word: 71.25pt (a text line and four empty ones, 14.25pt each)", n + 1));
        }
    }
    check(failures);
}

/// docx-C006-b1「テキストボックスの run の eastAsia 書体（rFonts の eastAsia だけを指定）が
/// 使われず、日本語が明朝（欧文側の書体）で描かれる」: 同じ書式（eastAsia が
/// ＭＳ ゴシック）の「ダミー部品」を、Word は本文でも枠の中でも同じ書体で描く。
/// 枠の中の run を描く書体が、本文の同じ字の書体と同じであること。
fn c006_b1_text_box_east_asian_font_as_body() {
    let mut s = open("textbox-ea-font.docx");
    let pages = pages_of(&mut s);
    let page = &pages.pages[0];
    let body_font = find(&glyphs(page), "ダミー部品")[0].font.clone();
    let style = page
        .items
        .iter()
        .find_map(|i| if let Item::Shape { shape, .. } = i { shape.text_body() } else { None })
        .and_then(|t| t.paragraphs.iter().flat_map(|p| &p.runs).find(|r| r.text.contains("ダミー部品")).map(|r| r.style.clone()))
        .expect("the text box's run");
    let boxed = s.with(move |e, _| e.resources.font(&ibis_doc_model::TextStyle::default().with_run(&style)));
    assert_eq!(boxed.family, body_font.family, "the text box draws 「ダミー部品」 in {} ({:?}); Word: the same typeface as the body's ({})", boxed.family, boxed.weight, body_font.family);
}

/// docx-C006-b2「置き換え先（ヒラギノ角ゴ W3）が、ＭＳ ゴシック・HGP創英角ｺﾞｼｯｸUB より
/// 細く、字が薄く見える」: 太字でない ＭＳ ゴシックの run を、置き換え先の標準の太さ（W3、
/// 400）で描く。Word の正解 PDF では、ＭＳ ゴシックは W3 より線が太く（暗さが約 12% 多い）、
/// HGP創英角ｺﾞｼｯｸUB は極太。ＭＳ ゴシックは W5（500）以上、HG 創英角ｺﾞｼｯｸUB は
/// W6 以上（700）の太さで描かれること。しきい値は暗さの比（約 1.12）と「極太」から決めた
/// 見立てで、細かい値は直す担当が決めてよい（W5 の指定の行を 500 の対照とする）。
fn c006_b2_ms_gothic_drawn_heavier_than_default() {
    let pages = laid_out("msgothic-light.docx");
    let gs = glyphs(&pages.pages[0]);
    let weight = |text: &str| find(&gs, text)[0].font.weight.0;
    let mut failures = Vec::new();
    let ms = weight("MSゴシック 10.5pt");
    if ms < 500.0 {
        failures.push(format!("ＭＳ ゴシック drawn in weight {ms} (Hiragino Sans W3); Word: heavier, W5 (500) or more"));
    }
    let hg = weight("HGP創英角ｺﾞｼｯｸUB");
    if hg < 700.0 {
        failures.push(format!("HGP創英角ｺﾞｼｯｸUB drawn in weight {hg}; Word: extra bold, W6 (700) or more"));
    }
    check(failures);
}

/// docx-C008-b1「行グリッドのある文書で、ＭＳ 明朝の 1 行が 1 ピッチを超えるとき、Word は
/// 2 ピッチぶんの行にするが、1 ピッチのまま」（枠の中の形は直った。表のセルの中の形が残る）:
/// グリッド 14.05pt・12pt の ＭＳ 明朝（1 行 約 15.6pt）の「枚」を、下寄せ（vAlign=bottom）の
/// セルに 1 行だけ置く。Word の正解 PDF では「枚」の字の中心がセルの下罫線の約 14.8pt 上
/// （2 ピッチ = 28.1pt の行の中央）で、今は約 6.5pt 上（1 ピッチの行の中央）。許容差 3pt。
/// todo-docx の L2（表のセルの行グリッド、[x]）は直っている。この崩れは L2 の後に残る分。
fn c008_b1_single_line_cell_takes_two_grid_lines() {
    let pages = laid_out("msfont-grid-single-line-cell.docx");
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let mut failures = Vec::new();
    for (row, g) in find_all(&gs, "枚").iter().enumerate() {
        let g = &g[0];
        let border = page.items.iter().filter_map(|i| if let Item::Rect { y, w, h, .. } = i && *h < 1.5 && *w > 20.0 && *y > g.y { Some(*y) } else { None }).fold(f32::MAX, f32::min);
        let above = border - (g.y - 0.35 * g.size);
        if !near(above, 14.8, 3.0) {
            failures.push(format!("row {}: the center of 枚 is {above:.1}pt above the cell's bottom rule; Word: 14.8pt (a two-grid-line, 28.1pt line)", row + 1));
        }
    }
    assert!(!failures.is_empty() || !find_all(&gs, "枚").is_empty(), "the cells are drawn");
    check(failures);
}

/// The first border row of a page (a table's top edge): the thin horizontal pieces at the topmost
/// y, left to right, as (x, width).
fn top_edge(page: &Page) -> Vec<(f32, f32)> {
    let thin: Vec<(f32, f32, f32)> = page.items.iter().filter_map(|i| if let Item::Rect { x, y, w, h, .. } = i && *h < 1.0 && *w >= 1.0 { Some((*x, *y, *w)) } else { None }).collect();
    let top = thin.iter().map(|r| r.1).fold(f32::MAX, f32::min);
    let mut pieces: Vec<(f32, f32)> = thin.iter().filter(|r| (r.1 - top).abs() < 0.3).map(|r| (r.0, r.2)).collect();
    pieces.sort_by(|a, b| a.0.total_cmp(&b.0));
    pieces
}

/// docx-C017-b1「罫線の種類 dashed（sz=4、0.5pt）を、線 2pt・間 2pt（太さの 4 倍ずつ）の粗い破線に
/// 描く」: Word は 0.5pt の dashed を、線 約 1.5pt・間 約 0.5pt（周期 2pt、3:1）で描き、縮小すると
/// 実線に近く見える（正解の PDF を 300dpi で測って 6px・2px）。表の上の罫線の最初の 10 本の
/// 線の長さが 1.5pt、周期が 2.0pt であること。許容差 0.3pt。
/// 太い dashed の周期は確かめていない（コーパスの dashed は sz=4 だけ）。
fn c017_b1_dashed_border_has_word_pattern() {
    let pages = laid_out("dashed-border-pattern.docx");
    let pieces = top_edge(&pages.pages[0]);
    assert!(pieces.len() >= 12, "the top border is drawn in {} pieces (a dashed line is many short ones)", pieces.len());
    let mut failures = Vec::new();
    for (a, b) in pieces[..10].iter().zip(&pieces[1..11]) {
        if !near(a.1, 1.5, 0.3) || !near(b.0 - a.0, 2.0, 0.3) {
            failures.push(format!("a dash at x {:.2} is {:.2}pt long and the next starts {:.2}pt on; Word: 1.5pt long, a 2.0pt period", a.0, a.1, b.0 - a.0));
            break;
        }
    }
    check(failures);
}

/// docx-C018-b1「罫線も塗りも無い浮動の表が、`tblpXSpec="right"`（`horzAnchor="margin"`）だと幅 0 で
/// 置かれ、用紙の右端の外に出る」: Word は表（幅 325pt）の右端を右余白（用紙の右から 72pt）に
/// そろえて置き、表の左端は 198pt。表の中の字は全部右余白の内側にあり、列「1-1」は
/// x 250 付近（235〜265）に出る。対照（罫線を single にしただけ）はそのとおりに出る。
fn c018_b1_borderless_float_table_right_check(file: &str) {
    let pages = laid_out(file);
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let right = page.width - 72.0;
    let mut failures = Vec::new();
    let beyond: Vec<String> = gs.iter().filter(|g| g.x + g.size > right + 1.0).map(|g| format!("{:?} at x {:.1}", g.c, g.x)).collect();
    if !beyond.is_empty() {
        failures.push(format!("{} characters are drawn right of the right margin ({right:.1}pt): {}", beyond.len(), beyond[..beyond.len().min(4)].join(", ")));
    }
    let x = find(&gs, "1-1")[0].x;
    if !near(x, 249.7, 15.0) {
        failures.push(format!("the cell 「1-1」 at x {x:.1}; Word: about 250pt (the table's right edge on the right margin)"));
    }
    check(failures);
}

fn c018_b1_borderless_float_table_right_is_inside_the_margin() {
    c018_b1_borderless_float_table_right_check("borderless-float-table-right.docx");
}

/// docx-C020-b3「VML の線（`v:line`）の `from` の始点（x, y）を位置に使わず、段落の上端・左端に描く」:
/// `from="6.2pt,14.5pt"`（style に margin-left・margin-top は無い）の線を、Word は段落の上端から
/// 14.5pt 下、左端から 6.2pt 内側から引く（元文書の正解 PDF で、線は項目の文字の下、左端から
/// 6.2〜6.95pt 内側）。線の左端が文字の左端より 6.2pt 右（許容差 1pt）で、線が文字の
/// 下（その行の基準線より下）にあること。対照（margin で同じ位置を指定した文書）は通る。
fn c020_b3_vml_line_from_offset_check(file: &str) {
    let pages = laid_out(file);
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let heads = find_all(&gs, "【");
    let lines: Vec<(f32, f32)> = page.items.iter().filter_map(|i| if let Item::Shape { x, y, .. } = i { Some((*x, *y)) } else { None }).collect();
    assert!(heads.len() == 6 && lines.len() == 6, "six items and six lines are drawn ({} / {})", heads.len(), lines.len());
    let mut failures = Vec::new();
    for (n, (head, line)) in heads.iter().zip(&lines).enumerate() {
        let (dx, below) = (line.0 - head[0].x, line.1 - head[0].y);
        if !near(dx, 6.2, 1.0) || below <= 0.0 {
            failures.push(format!("item {}: the line starts {dx:.1}pt right of the text's left edge and {below:.1}pt below its baseline; Word: 6.2pt right, under the text", n + 1));
        }
    }
    check(failures);
}

fn c020_b3_vml_line_from_offset_is_used() {
    c020_b3_vml_line_from_offset_check("vml-line-from-offset.docx");
}

/// docx-C021-b1「`w:hint="eastAsia"` の run の U+00D7（×）を、和文の書体で 1em に描かず、欧文書体の
/// 細い字形（Times の約 0.56em）で描く」: Word は和文の書体（ＭＳ 明朝）の全角、1em で描く
/// （正解の PDF で、金額欄「×，×××，×××」が 9 字 × 10.5pt = 94.5pt）。「あ」から「円」まで
/// の距離は、1 行目（あ＋×，×××，×××＋円）が 105pt、3 行目（あ＋× 10 個＋円）が 115.5pt。
/// 2・4 行目の「△」の並び（和文で通る）は 84pt・115.5pt。許容差 1.5pt。
/// hint の無い × を Word が欧文書体で描くことは確かめていない（コーパスの × は全部 hint 付き）。
fn c021_b1_hint_east_asia_times_sign_is_full_width() {
    let pages = laid_out("hint-eastasia-times-sign.docx");
    let gs = glyphs(&pages.pages[0]);
    let (starts, ends) = (find_all(&gs, "あ"), find_all(&gs, "円"));
    assert!(starts.len() == 4 && ends.len() == 4, "four rows");
    let mut failures = Vec::new();
    for (row, want) in [105.0, 84.0, 115.5, 115.5].into_iter().enumerate() {
        let span = ends[row][0].x - starts[row][0].x;
        if !near(span, want, 1.5) {
            failures.push(format!("row {}: 「あ」 to 「円」 is {span:.1}pt; Word: {want}pt (× takes a full em)", row + 1));
        }
    }
    check(failures);
}

/// The window's pixels over a text box's text row: the rightmost dark pixel (points from the page's
/// left) of the rows that hold a long horizontal rule (an underline), if there is one.
fn rightmost_underline(s: &mut Scenario, page: usize, (x, y, w, h): (f32, f32, f32, f32)) -> Option<f32> {
    let crop = page_pixels(s, page, (x, y, w, h));
    let (cols, rows, pixels) = &crop;
    let per_pt = *cols as f32 / w;
    let dark = |p: &[u8; 3]| (p[0] as u32 + p[1] as u32 + p[2] as u32) / 3 < 160;
    let mut right: Option<f32> = None;
    for r in 0..*rows {
        let row = &pixels[r * cols..(r + 1) * cols];
        let count = row.iter().filter(|p| dark(p)).count();
        if count as f32 >= 40.0 * per_pt {
            let last = row.iter().rposition(dark).unwrap();
            right = Some(right.map_or(x + last as f32 / per_pt, |v: f32| v.max(x + last as f32 / per_pt)));
        }
    }
    right
}

/// docx-C022-b1「テキストボックスの行の末尾（段落末）にある、下線つきの空白に下線が出ない」:
/// 箱の 1 行「事業所名［下線つきの空白 14 字］［空白 1 字］代表者職・氏名［下線つきの空白 11 字］」で、
/// Word は末尾の 11 字（115pt）にも下線を引き、下線は箱の左端から約 395pt（x 約 467pt）まで
/// 続く。今は途中の空白までで、x 約 352pt で終わる。下線の右端が x 420pt 以上であること。
/// 対照（末尾に実字「終」を足しただけ）は通る。
fn c022_b1_textbox_trailing_underline_check(file: &str) {
    let mut s = open(file);
    let pages = pages_of(&mut s);
    let (x, y, w, h) = pages.pages[0].items.iter().find_map(|i| if let Item::Shape { x, y, w, h, .. } = i { Some((*x, *y, *w, *h)) } else { None }).expect("the text box");
    let right = rightmost_underline(&mut s, 0, (x, y, w, h));
    assert!(right.is_some_and(|r| r >= 420.0), "the underline in the box ends at x {right:?}pt; Word: about 467pt (it covers the trailing spaces)");
}

fn c022_b1_textbox_trailing_underlined_space_is_underlined() {
    c022_b1_textbox_trailing_underline_check("textbox-trailing-underline-space.docx");
}

/// docx-C022-b2「行グリッド（`w:docGrid` type=lines・linePitch 16.35pt）のある文書で、テキストボックスの
/// `snapToGrid=0` の段落をグリッドの高さに広げる」: Word は箱の snapToGrid=0 の段落を自然な高さ
/// （ＭＳ 明朝 10pt で約 13pt、正解の PDF で段落 1 つぶん 13.0pt）で並べ、16.35pt のグリッドに広げない。
/// 箱の文字のある段落の行間が、グリッドの倍数の固定値（14.5pt を超えるもの）でないこと。
/// 対照（docGrid を外した同じ文書）は固定値にならない。
/// 確度は中〜高（README）。「snapToGrid が無い（既定）の箱の段落」をグリッドに合わせるかは主張しない。
fn c022_b2_textbox_snap_to_grid_off_check(file: &str) {
    let pages = laid_out(file);
    let body = pages.pages[0].items.iter().find_map(|i| if let Item::Shape { shape, .. } = i { shape.text_body().cloned() } else { None }).expect("the text box");
    let mut failures = Vec::new();
    for p in &body.paragraphs {
        let text: String = p.runs.iter().map(|r| r.text.as_str()).collect();
        if text.trim().is_empty() {
            continue;
        }
        if let Some(ibis_doc_model::LineSpacing::Points(v)) = p.line_spacing && v as f32 / 100.0 > 14.5 {
            failures.push(format!("the box's paragraph {text:?} is set {:.2}pt high; Word: its natural height (about 13pt), not the grid's 16.35pt", v as f32 / 100.0));
        }
    }
    check(failures);
}

fn c022_b2_textbox_snap_to_grid_off_is_not_grid_high() {
    c022_b2_textbox_snap_to_grid_off_check("textbox-snaptogrid-off-grid.docx");
}

/// docx-C023-b1「英数字の語（URL・型番）の中の半角ハイフンの後ろで折れない」: 和文 20〜29 字の
/// 直後の URL（約 243pt）が行末に収まらないとき、Word はハイフンの後ろ（…/e-、…/e-news/e-）で
/// 折り、URL の頭が前の行に残る（元文書の正解 PDF）。ibisOffice は URL 全体を次の行へ送る。
/// 10 の段落の URL の頭が、和文のある行（行の左端より右）に来ること。1 行より長い URL
/// （最後の段落）は、Word では「…8d15-」で切れて次の行が「47e2b9c05f31」で始まる（ibisOffice は
/// 文字単位で「…8d15-4」で切る）。
/// 対照（ハイフンを `_` にした文書）は、Word も `_` では折らないので URL 全体が次の行に来る。
fn c023_b1_latin_hyphen_break_check(file: &str, hyphen: bool) {
    let pages = laid_out(file);
    let gs = glyphs(&pages.pages[0]);
    let urls = find_all(&gs, "https");
    assert!(urls.len() >= 11, "eleven URLs ({})", urls.len());
    let mut failures = Vec::new();
    for (n, url) in urls[..10].iter().enumerate() {
        let at_line_start = near(url[0].x, 72.0, 0.5);
        if at_line_start == hyphen {
            failures.push(format!("URL {}: starts at x {:.1} ({}); Word: {}", n + 1, url[0].x, if at_line_start { "moved whole to the next line" } else { "after the Japanese on its line" }, if hyphen { "its head stays on the Japanese's line, cut after a hyphen" } else { "moved whole to the next line (no break at an underscore)" }));
        }
    }
    if hyphen {
        let tail = find_all(&gs, "47e2b9c05f31/6b1e9a07");
        if !tail.first().is_some_and(|t| near(t[0].x, 72.0, 0.5)) {
            failures.push(format!("the long URL's second line starts at x {:?}; Word: at 72pt with 「47e2b9c05f31」 (the first line ends after 「8d15-」)", tail.first().map(|t| t[0].x)));
        }
    }
    check(failures);
}

fn c023_b1_latin_hyphen_is_a_break_opportunity() {
    c023_b1_latin_hyphen_break_check("latin-hyphen-break.docx", true);
}

/// The last word of the first line of each paragraph of a file of two-line paragraphs (the page's
/// text lines, top to bottom, every other one).
fn first_line_last_words(page: &Page) -> Vec<String> {
    let mut lines: Vec<(f32, String)> = Vec::new();
    for item in &page.items {
        if let Item::Text { y, text, .. } = item {
            lines.push((*y, text.to_string()));
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    lines.iter().step_by(2).map(|l| l.1.split_whitespace().last().unwrap_or("").to_string()).collect()
}

/// docx-C023-b2「両端揃えの行で、語間の空白を詰めて 1 語多く入れない」: 英文の 5 つの段落で、
/// 1 語足すと自然幅が行の幅（453.5pt）を 2.5〜4.3pt 超える 1 行目を、Word は語間を約 6〜10%
/// 詰めて 1 語多く入れる（元文書の正解 PDF で、語間が自然幅より 8〜11% 詰まった行、各語の幅は
/// 自然幅のまま）。1 行目の最後の語は follow・region・each・several・compare。今は 8% までの詰め
/// （JUSTIFY_SHRINK）で入る段落 1・3・4 は通り、10% 近く詰める段落 2・5 が 1 語手前で折れる。
/// 対照（左揃え）は詰めない: 最後の語が who・every・and・while・members。
/// 詰められる限度は 11% までしか実測していない（README の確度は中）。
fn c023_b2_justify_space_shrink_check(file: &str, want: [&str; 5]) {
    let pages = laid_out(file);
    let got = first_line_last_words(&pages.pages[0]);
    assert!(got.iter().map(String::as_str).eq(want), "the last word of each paragraph's first line {got:?}; Word: {want:?}");
}

fn c023_b2_justified_line_shrinks_spaces_for_one_more_word() {
    c023_b2_justify_space_shrink_check("justify-space-shrink.docx", ["follow", "region", "each", "several", "compare"]);
}

/// docx-C023-b3「`wordWrap=0` の段落で、行内に収まる英数字の語を途中で割る」: Word は行末に収まらない
/// 英数字の語（「(KRT-07)」「XYZ12345」「(ABCD1234)」、行末の残りは 10〜20pt）を割らず、語ごと
/// 次の行の頭に送る（元文書の正解 PDF）。6 つの段落とも、語が行頭（x=72）から始まること。
/// 4e88734dc では 6 つとも割れたが、今は XYZ12345 の 2 つ（直前の和文 1 字ごと次の行へ送る形）が残る。
/// 対照（`wordWrap` を指定しない文書）は 6 つとも語を割らない。
/// 確度は中（README: ECMA-376 の説明と食い違い、正解 PDF 2 文書からの推定）。
fn c023_b3_wordwrap0_latin_word_check(file: &str) {
    let pages = laid_out(file);
    let gs = glyphs(&pages.pages[0]);
    let mut failures = Vec::new();
    for word in ["(KRT-07)", "XYZ12345", "(ABCD1234)"] {
        for (n, w) in find_all(&gs, word).iter().enumerate() {
            if !near(w[0].x, 72.0, 0.5) {
                failures.push(format!("{word} ({}): starts at x {:.1}; Word: at the line start 72pt (the word is not cut)", n + 1, w[0].x));
            }
        }
    }
    check(failures);
}

fn c023_b3_wordwrap0_does_not_cut_a_word_that_fits() {
    c023_b3_wordwrap0_latin_word_check("wordwrap0-latin-midword.docx");
}

/// docx-C024-b1「縦書きセル（`textDirection tbRlV`）の中の fitText・`w:w` の付いた字が描かれず、
/// ラベルが空欄になる」: Word は縦書きの「その／の／他」を欄の中に 3 字並べる（元文書の正解
/// PDF）。fitText と `w:w` の run、fitText だけ、`w:w` だけの 3 つの欄とも、「そ」「他」が 3 つ
/// ずつ描かれること。対照（fitText と `w:w` を外した文書）は通る。
fn c024_b1_vertical_cell_fit_text_check(file: &str) {
    let pages = laid_out(file);
    let gs = glyphs(&pages.pages[0]);
    let counts: Vec<usize> = ["そ", "他"].iter().map(|c| find_all(&gs, c).len()).collect();
    assert_eq!(counts, vec![3, 3], "「そ」「他」 drawn in the vertical cells; Word: 3 each (each cell shows その／の／他)");
}

fn c024_b1_vertical_cell_with_fit_text_draws_its_label() {
    c024_b1_vertical_cell_fit_text_check("vtext-cell-fittext.docx");
}

/// docx-C025-b2「テキスト ボックスの中の文字の囲み罫線（rPr の `w:bdr`）が描かれない」: 本文の run の
/// `w:bdr` は描かれるが、枠なし・塗りなしのテキスト ボックスの中の run の `w:bdr` は描かれない。
/// Word は箱の中の「Boxed in textbox」に 0.5pt の囲みを描く（元文書の正解 PDF）。箱の範囲の画素に、
/// 文字の幅ほど（45pt 以上）続く横の線（囲みの上辺と下辺）が 2 本以上あること。
fn c025_b2_text_box_run_border_is_drawn() {
    let mut s = open("textbox-run-border.docx");
    let pages = pages_of(&mut s);
    let (x, y, w, h) = pages.pages[0].items.iter().find_map(|i| if let Item::Shape { x, y, w, h, .. } = i { Some((*x, *y, *w, *h)) } else { None }).expect("the text box");
    let (cols, rows, pixels) = page_pixels(&mut s, 0, (x, y, w, h));
    let per_pt = cols as f32 / w;
    let mut rules = 0;
    for r in 0..rows {
        let mut run = 0usize;
        let mut longest = 0usize;
        for p in &pixels[r * cols..(r + 1) * cols] {
            if (p[0] as u32 + p[1] as u32 + p[2] as u32) / 3 < 200 {
                run += 1;
                longest = longest.max(run);
            } else {
                run = 0;
            }
        }
        if longest as f32 >= 45.0 * per_pt {
            rules += 1;
        }
    }
    assert!(rules >= 2, "{rules} rows of the text box's crop hold a long horizontal rule; Word: the run's border (top and bottom edges, 2 rows or more)");
}

/// The gap between the character `c` (the `n`th drawn) and the next one: the next one's x minus
/// this one's right edge (its size).
fn gap_after(gs: &[Glyph], c: char, n: usize) -> f32 {
    let i = gs.iter().enumerate().filter(|(_, g)| g.c == c).nth(n).map(|(i, _)| i).unwrap_or_else(|| panic!("{c:?} is drawn {} times", n + 1));
    gs[i + 1].x - (gs[i].x + gs[i].size)
}

/// docx-C026-b2「日本語の直後のフィールドの結果（PAGE の数字）に、日本語と数字の間の自動の空きを
/// 入れる」: フッターの「附録」の直後の PAGE フィールドの結果の数字との間に、Word は空きを入れず
/// 「附録5」と詰める（元文書の正解 PDF）。同じ文書の普通の本文「附録5」「附録X」には空き
/// （全角の 1/4 = 2.6pt）が入る。1 行目（PAGE）の空きが 0（許容差 0.5pt）。
/// フィールドの結果であることが原因か、前後の run の書式が原因かは分けていない（確度は中）。
fn c026_b2_field_result_gets_no_auto_space() {
    let pages = laid_out("field-autospace.docx");
    let gs = glyphs(&pages.pages[0]);
    let gap = gap_after(&gs, '録', 0);
    assert!(near(gap, 0.0, 0.5), "the gap between 「附録」 and the PAGE field's number is {gap:.2}pt; Word: none (「附録5」 touching)");
}

/// docx-C026-b3「両端揃えの余りを、Word は自動の空き（日本語と数字の間）に配り、全文字には配らない」:
/// ＭＳ 明朝 12pt の両端揃えの行（行の余り 約 3pt）で、Word は日本語の字をどれも 12.00pt のままに
/// 並べ（14 字で 168.0pt）、「10」の前後の空きに余りを加える（3.0pt に余りの半分 1.5pt を足して 4.5pt、
/// 元文書の正解 PDF の 2 行）。ibisOffice は余りを全文字に等分する（14 字で 169.0pt、空き 3.1pt）。
/// 1 行目の「あ」から 14 字目までの幅が 168.0pt（許容差 0.4pt）で、「10」の前の空きが 4.5pt
/// （許容差 0.6pt）。差は 1pt ほどと小さい（確度は中）。
fn c026_b3_justify_slack_goes_to_the_auto_space() {
    let pages = laid_out("justify-autospace.docx");
    let gs = glyphs(&pages.pages[0]);
    let first = find(&gs, "あいうえおかきくけこさしすせ");
    let span = first[13].x + 12.0 - first[0].x;
    let ten = find(&gs, "10");
    let gap = ten[0].x - (first[13].x + 12.0);
    let mut failures = Vec::new();
    if !near(span, 168.0, 0.4) {
        failures.push(format!("the 14 kana span {span:.2}pt; Word: 168.0pt (12.00pt each, the slack is not spread over them)"));
    }
    if !near(gap, 4.5, 0.6) {
        failures.push(format!("the gap before 「10」 is {gap:.2}pt; Word: 4.5pt (3.0pt of auto space and half the slack)"));
    }
    check(failures);
}

/// Rows of the page's text: the baselines with the characters on them, top to bottom.
fn text_rows(page: &Page) -> Vec<(f32, Vec<Glyph>)> {
    let mut rows: Vec<(f32, Vec<Glyph>)> = Vec::new();
    for g in glyphs(page) {
        match rows.iter_mut().find(|r| (r.0 - g.y).abs() < 0.5) {
            Some(r) => r.1.push(g),
            None => rows.push((g.y, vec![g])),
        }
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    rows
}

/// docx-C028-b2「本文の私用領域（U+F0xx）の字が、書体の下位バイトの字に戻されず、「?」入りの四角になる」:
/// Word は U+F0xx を書体の 0xxx の字として引く（元文書の正解 PDF で、一太郎の記号書体
/// JustUnitMarkG の U+F02C は「甲」「乙」の行末の小さな「,」）。描かれる字に私用領域の字（U+F000〜
/// U+F8FF）が無いこと、「甲」「乙」の行末の字が「,」であること。Wingdings の U+F0FE を「☑」で
/// 描くのは推定（コーパスで確かめたのは JustUnitMarkG の「,」のみ）で、私用領域の字でないことだけ確かめる。
fn c028_b2_private_use_characters_are_mapped() {
    let pages = laid_out("pua-text.docx");
    let page = &pages.pages[0];
    let gs = glyphs(page);
    let mut failures = Vec::new();
    let pua: Vec<String> = gs.iter().filter(|g| ('\u{F000}'..='\u{F8FF}').contains(&g.c)).map(|g| format!("U+{:04X}", g.c as u32)).collect();
    if !pua.is_empty() {
        failures.push(format!("private-use characters drawn as they are (tofu): {pua:?}; Word: the font's own character (U+F02C is 「,」)"));
    }
    for row in text_rows(page).iter().filter(|r| r.1.iter().any(|g| g.c == '甲' || g.c == '乙')) {
        let last = row.1.iter().rev().find(|g| !g.c.is_whitespace()).map(|g| g.c);
        if last != Some(',') {
            failures.push(format!("the line of 「甲」/「乙」 ends with {last:?}; Word: 「,」"));
        }
    }
    check(failures);
}

/// docx-C028-b3「行末の空白だけの run に枠（`w:bdr`）があると、空の四角が出る」: Word は行末の空白には
/// 枠を描かない（元文書の正解 PDF で、行の右に何も出ない。1 行目のように字のある run の枠は出る）。
/// 最小文書で、1 行目の枠（左 71〜169pt）のほかに、行末の空白の枠（x 200pt より右の縦の線）が
/// 無いこと。対照（行末の空白の run と段落記号の `w:bdr` を外した文書）は通る。
fn c028_b3_run_border_check(file: &str) {
    let pages = laid_out(file);
    let boxes: Vec<f32> = pages.pages[0].items.iter().filter_map(|i| if let Item::Rect { x, w, h, .. } = i && *w < 1.0 && *h > 10.0 && *x > 200.0 { Some(*x) } else { None }).collect();
    assert!(boxes.is_empty(), "vertical rules of a run border at x {boxes:.1?} right of the text; Word: no border around a line's trailing space");
}

fn c028_b3_trailing_space_run_has_no_border() {
    c028_b3_run_border_check("run-border-trailing-space.docx");
}

/// The width (points) of the glyph `g` drawn on the page: the longest run of pixel columns that
/// hold a dark pixel in its em box (a neighbour's edge, apart from it, does not count), 0 when
/// nothing is drawn there.
fn ink_width(s: &mut Scenario, g: &Glyph) -> f32 {
    let crop = page_pixels(s, 0, (g.x, g.y - g.size * 0.9, g.size, g.size * 1.15));
    let dark = pick(&crop, |p| (p[0] as u32 + p[1] as u32 + p[2] as u32) / 3 < 150);
    let mut on = vec![false; crop.0];
    for (c, _) in dark {
        on[c] = true;
    }
    let (mut longest, mut run) = (0usize, 0usize);
    for c in on {
        run = if c { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    longest as f32 / (crop.0 as f32 / g.size)
}

/// docx-C028-b4「「☑」（U+2611）が、東アジア書体で全角にならず小さく描かれる」: `w:hint="eastAsia"` の
/// run の「☑」を、Word は隣の「□」と同じ大きさの全角で描く（元文書の正解 PDF）。今は半分ほどの
/// 大きさで薄い。「□有　　☑無」の行で、「☑」の字の幅（暗い画素の幅）が「□」の 0.8 倍以上。
/// 対照（「☑」を「□」にした文書）は「□」が全角で出る（□ の幅が 0.7em 以上）。
fn c028_b4_check_mark_is_as_large_as_the_box() {
    let mut s = open("check-mark-u2611-size.docx");
    let pages = pages_of(&mut s);
    let gs = glyphs(&pages.pages[0]);
    let (square, check) = (find(&gs, "□")[0].clone(), find(&gs, "☑")[0].clone());
    let (a, b) = (ink_width(&mut s, &square), ink_width(&mut s, &check));
    assert!(a > 0.0 && b >= 0.8 * a, "「☑」 is {b:.2}pt wide in ink, 「□」 {a:.2}pt; Word: the same size (a full-width box)");
}

/// 悪化の検知（緑）: docx-C007-b1（表のセルの中の箱の、列基準の負のオフセット）。第 2 回で
/// 赤の試験を足したが、623815f40 の main で直ったので外した。文書を戻して緑の試験にする。
/// 3 列目（左端 約 257pt）のセルの箱の posOffset は −200pt で、Word は箱の左端を
/// 257 − 200 = 57pt（左余白）に置く。許容差 2pt。
fn guard_c007_b1_cell_box_negative_column_offset() {
    let pages = laid_out("cellbox-column-negative.docx");
    let x = pages.pages[0].items.iter().find_map(|i| if let Item::Shape { x, .. } = i { Some(*x) } else { None }).expect("the box");
    assert!(near(x, 57.0, 2.0), "the box's left edge at x {x:.1}; Word: 57pt (the third column's left edge 257pt, minus 200pt)");
}

/// 後退（統合メモの「新ビルドの後退の疑い」。92341f7c1 では出なかった。原因の見当は P2＝フッターが
/// 下余白を越えるとき本文の下端を上げる処理、docx-C027-b1・C016-b3 のフッターの空の段落の高さ）
/// docx-reg-b1「節の最後に空の段落（行の高さ固定 18pt）が並ぶページに、ページ番号の入る
/// フッター（PAGE と空の段落の 2 段落）があると、空の段落が次のページへ溢れて、ページ番号だけの
/// 白紙ページが 1 枚増える」: Word は 1 ページに収める（元文書の正解 PDF で、この節は 1 ページで、
/// 次の節がすぐ次のページから始まる）。最小文書は元文書のその 1 ページぶんを、文面だけ
/// ダミーに替えたもの。1 ページであること。対照（フッターを参照しない同じ文書）は 1 ページ。
/// 元文書では表の行の高さの差（Word より約 21pt 高い）も重なっていて、このページがぎりぎりに満ちている。
fn reg_b1_blank_paragraphs_at_page_end_check(file: &str) {
    let pages = laid_out(file);
    assert!(pages.pages.len() == 1, "{} pages; Word: 1 (the empty paragraphs at the section's end fit on the page; no page with only the page number)", pages.pages.len());
}

fn reg_b1_blank_paragraphs_at_page_end_add_no_page() {
    reg_b1_blank_paragraphs_at_page_end_check("trailing-blank-paragraphs-footer.docx");
}

/// 後退（44852d4d2 では正しく描けていた。20e41a89e 以降で出る）
/// docx-reg-b2「ページ番号の枠（`w:framePr`）の無い、空の段落だけのフッターの文書で、2 ページ目が表の
/// 見出し行でなく 2 つ目の表の行から始まる」: docx-C013-b1 の対照（枠の段落だけを外した同じ文書）は、
/// 44852d4d2 では 6 ページで、どのページも表の見出し行（A）が上端にあった。20e41a89e 以降は、枠の段落のある
/// 赤の文書（c013_b1）と同じ崩れ方（2 ページ目の頭が見出し行「A」でなく「a」の行、6 ページ目が空）になる。
/// 枠の段落が引き金でなくなった、または main の別の変化による後退（原因は調べていない）。
/// 確かめは c013_b1 と同じ（6 ページで、どのページも見出し行の「A」から始まる）。
fn reg_b2_table_heading_row_starts_every_page() {
    c013_b1_footer_frame_paragraph_takes_no_height_check("footer-framepr-height-ctrl.docx");
}
