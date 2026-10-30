// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The xlsx defects of the corpus report (Excel for the web's PDFs against
//! ibisOffice's) that need drawing or printing, each as Excel shows it, on
//! the small files of `test/corpus-report/xlsx` (made up, no corpus text).
//! The ones the model shows are `ibis-pptx/tests/corpus_report_xlsx.rs`.
//!
//! TODO: このファイルは、実文書の比較（コーパス）から見つかった崩れの試験の
//! 仮置き場で、ずっと残すものではない。試験が通るようになったら（崩れを直した
//! ら）、その試験を本番の試験に移すか、試験として要らなければ消し、最小文書
//! （`test/corpus-report/xlsx/`）も一緒に移すか消す。
//! 本番の移し先は `crates/ibis-office-app/src/xlsx/scenarios.rs`。
//! 崩れが残っている試験（赤）はここに置いたままにする。

use std::path::{Path, PathBuf};
use std::process::Command;

use ibis_doc_model::{Color, EMU_PER_POINT, Rgb, ShapeKind, sheet_calc::sheet_of_slide};

use crate::export::{Format, Options, export};
use crate::render::chart_view::{LAID_OUT, LaidOut};
use crate::shell::scenario::Scenario;

const DIR: &str = "test/corpus-report/xlsx";

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::corpus_report_xlsx_x1b_cf_anchor_crash", corpus_report_xlsx_x1b_cf_anchor_crash),
        ("xlsx::corpus_report_xlsx_x1b_cf_anchor_color", corpus_report_xlsx_x1b_cf_anchor_color),
        ("xlsx::corpus_report_xlsx_x2b_header_footer_line_breaks", corpus_report_xlsx_x2b_header_footer_line_breaks),
        ("xlsx::corpus_report_xlsx_x2c_page_numbers_across_sheets", corpus_report_xlsx_x2c_page_numbers_across_sheets),
        ("xlsx::corpus_report_xlsx_x2d_center_across_date", corpus_report_xlsx_x2d_center_across_date),
        ("xlsx::corpus_report_xlsx_x3a_fill_spaces_by_font", corpus_report_xlsx_x3a_fill_spaces_by_font),
        ("xlsx::corpus_report_xlsx_x3b_column_wider_than_page", corpus_report_xlsx_x3b_column_wider_than_page),
        ("xlsx::corpus_report_xlsx_x4a_pie_label_value_format", corpus_report_xlsx_x4a_pie_label_value_format),
        ("xlsx::corpus_report_xlsx_x4b_pie_label_separator", corpus_report_xlsx_x4b_pie_label_separator),
        ("xlsx::corpus_report_xlsx_x4d_multilevel_category_outer_level", corpus_report_xlsx_x4d_multilevel_category_outer_level),
        ("xlsx::corpus_report_xlsx_x5a_bar3d_walls", corpus_report_xlsx_x5a_bar3d_walls),
        ("xlsx::corpus_report_xlsx_x5c_bar_scatter_combo", corpus_report_xlsx_x5c_bar_scatter_combo),
        ("xlsx::corpus_report_xlsx_x5d_axis_title_rotation", corpus_report_xlsx_x5d_axis_title_rotation),
        ("xlsx::corpus_report_xlsx_x6b_overflow_text_print_area", corpus_report_xlsx_x6b_overflow_text_print_area),
        ("xlsx::corpus_report_xlsx_x7a_narrow_columns_fit_one_page", corpus_report_xlsx_x7a_narrow_columns_fit_one_page),
        ("xlsx::corpus_report_xlsx_x8c_narrow_boolean_and_wrapped_number", corpus_report_xlsx_x8c_narrow_boolean_and_wrapped_number),
        ("xlsx::corpus_report_xlsx_guard_italic_ja_font_is_slanted", corpus_report_xlsx_guard_italic_ja_font_is_slanted),
        ("xlsx::corpus_report_xlsx_guard_text_box_without_outline_keeps_its_text_position", corpus_report_xlsx_guard_text_box_without_outline_keeps_its_text_position),
        ("xlsx::corpus_report_xlsx_guard_merged_rich_text_fits_the_cell", corpus_report_xlsx_guard_merged_rich_text_fits_the_cell),
        ("xlsx::corpus_report_xlsx_guard_default_row_height_follows_row_font", corpus_report_xlsx_guard_default_row_height_follows_row_font),
        ("xlsx::corpus_report_xlsx_guard_wrap_follows_column_width_chars", corpus_report_xlsx_guard_wrap_follows_column_width_chars),
        ("xlsx::corpus_report_xlsx_guard_center_across_text_is_centred_on_the_range", corpus_report_xlsx_guard_center_across_text_is_centred_on_the_range),
        ("xlsx::corpus_report_xlsx_custom_row_height_floors_to_whole_points", corpus_report_xlsx_custom_row_height_floors_to_whole_points),
        ("xlsx::corpus_report_xlsx_zero_size_text_box_draws_nothing", corpus_report_xlsx_zero_size_text_box_draws_nothing),
        ("xlsx::corpus_report_xlsx_empty_formula_result_stops_overflow", corpus_report_xlsx_empty_formula_result_stops_overflow),
        ("xlsx::corpus_report_xlsx_print_area_formula_is_evaluated", corpus_report_xlsx_print_area_formula_is_evaluated),
        ("xlsx::corpus_report_xlsx_hidden_group_box_not_drawn", corpus_report_xlsx_hidden_group_box_not_drawn),
        ("xlsx::corpus_report_xlsx_merged_edge_keeps_each_cells_colour", corpus_report_xlsx_merged_edge_keeps_each_cells_colour),
        ("xlsx::corpus_report_xlsx_grid_lines_set_zero_prints_gridlines", corpus_report_xlsx_grid_lines_set_zero_prints_gridlines),
        ("xlsx::corpus_report_xlsx_no_print_shape_not_printed", corpus_report_xlsx_no_print_shape_not_printed),
        ("xlsx::corpus_report_xlsx_zero_height_chart_draws_nothing", corpus_report_xlsx_zero_height_chart_draws_nothing),
        ("xlsx::corpus_report_xlsx_list_box_items_from_hidden_sheet", corpus_report_xlsx_list_box_items_from_hidden_sheet),
        ("xlsx::corpus_report_xlsx_manual_break_top_border_not_on_previous_page", corpus_report_xlsx_manual_break_top_border_not_on_previous_page),
        ("xlsx::corpus_report_xlsx_merged_double_border_stays_double", corpus_report_xlsx_merged_double_border_stays_double),
        ("xlsx::corpus_report_xlsx_slant_dash_dot_keeps_gaps", corpus_report_xlsx_slant_dash_dot_keeps_gaps),
        ("xlsx::corpus_report_xlsx_hair_border_is_thinner_than_thin", corpus_report_xlsx_hair_border_is_thinner_than_thin),
        ("xlsx::corpus_report_xlsx_center_across_inner_border_not_drawn", corpus_report_xlsx_center_across_inner_border_not_drawn),
        ("xlsx::corpus_report_xlsx_merged_top_border_not_extended_from_the_anchor", corpus_report_xlsx_merged_top_border_not_extended_from_the_anchor),
        ("xlsx::corpus_report_xlsx_x3c_substitute_font_digits", corpus_report_xlsx_x3c_substitute_font_digits),
        ("xlsx::corpus_report_xlsx_x4c_pie_size", corpus_report_xlsx_x4c_pie_size),
        ("xlsx::corpus_report_xlsx_x5b_all_zero_value_axis", corpus_report_xlsx_x5b_all_zero_value_axis),
        ("xlsx::corpus_report_xlsx_x5e_explicit_rot0_categories", corpus_report_xlsx_x5e_explicit_rot0_categories),
        ("xlsx::corpus_report_xlsx_overflow_text_past_page_width_adds_no_page", corpus_report_xlsx_overflow_text_past_page_width_adds_no_page),
        ("xlsx::corpus_report_xlsx_ms_font_wrapped_line_pitch", corpus_report_xlsx_ms_font_wrapped_line_pitch),
        ("xlsx::corpus_report_xlsx_wrap_at_ascii_space", corpus_report_xlsx_wrap_at_ascii_space),
        ("xlsx::corpus_report_xlsx_shape_text_clipped_at_shape_bottom", corpus_report_xlsx_shape_text_clipped_at_shape_bottom),
        ("xlsx::corpus_report_xlsx_center_across_wrapped_header_uses_the_whole_range", corpus_report_xlsx_center_across_wrapped_header_uses_the_whole_range),
        ("xlsx::corpus_report_xlsx_distributed_header_not_spread", corpus_report_xlsx_distributed_header_not_spread),
        ("xlsx::corpus_report_xlsx_rich_text_small_run_sets_line_pitch", corpus_report_xlsx_rich_text_small_run_sets_line_pitch),
        ("xlsx::corpus_report_xlsx_yugothic_fit_to_page_scale", corpus_report_xlsx_yugothic_fit_to_page_scale),
        ("xlsx::corpus_report_xlsx_overflow_text_from_outside_print_area_has_white_ground", corpus_report_xlsx_overflow_text_from_outside_print_area_has_white_ground),
        ("xlsx::corpus_report_xlsx_tab_in_cell_text_takes_no_width", corpus_report_xlsx_tab_in_cell_text_takes_no_width),
        ("xlsx::corpus_report_xlsx_vertical_text_wraps_into_columns", corpus_report_xlsx_vertical_text_wraps_into_columns),
        ("xlsx::corpus_report_xlsx_row_style_fills_the_whole_row", corpus_report_xlsx_row_style_fills_the_whole_row),
        ("xlsx::corpus_report_xlsx_column_width_digit_is_fractional", corpus_report_xlsx_column_width_digit_is_fractional),
        ("xlsx::corpus_report_xlsx_row_number_column_does_not_overflow_into_hashes", corpus_report_xlsx_row_number_column_does_not_overflow_into_hashes),
    ]
}

/// Unfixed corpus_report bug scenarios (skipped in normal `cargo test`, run with `--ignored` or `--include-ignored`).
pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    vec![
        // Expectations measured in Excel for Mac (rows at 100%); the print now takes a row at 0.9067 of its height as Excel for Windows'
        // PDFs do (`windows_print_geometry` in export.rs). Both cannot hold: the values are to be measured again in Excel for Windows
        // (after 0.8.5).
        ("xlsx::corpus_report_xlsx_bottom_aligned_line_sits_at_the_row_bottom", corpus_report_xlsx_bottom_aligned_line_sits_at_the_row_bottom),
        ("xlsx::corpus_report_xlsx_centered_text_leaves_room_above_bottom_rule", corpus_report_xlsx_centered_text_leaves_room_above_bottom_rule),
        ("xlsx::corpus_report_xlsx_custom_row_heights_fill_the_page_like_excel", corpus_report_xlsx_custom_row_heights_fill_the_page_like_excel),
    ]
}

fn open(name: &str) -> Scenario {
    Scenario::open(&format!("{DIR}/{name}"))
}

fn read(name: &str) -> ibis_doc_model::Presentation {
    let path = crate::shell::scenario::repo_root().join(DIR).join(name);
    ibis_pptx::read(&std::fs::read(&path).unwrap_or_else(|e| panic!("{name}: {e}"))).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// The text each cell of the current sheet shows, by (row, column).
fn shown(s: &mut Scenario) -> std::collections::HashMap<(u32, u32), String> {
    s.update(|e, window, _| {
        let sheet = sheet_of_slide(e.slide()).expect("a worksheet").clone();
        super::sheet_view::shown_texts(&sheet, &e.resources, window).into_iter().map(|(r, c, t)| ((r, c), t)).collect()
    })
}

fn all_hashes(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c == '#')
}

/// What each chart of the current sheet lays out, in shape order, at a
/// pixel a point.
fn charts(s: &mut Scenario) -> Vec<LaidOut> {
    s.with(|e, _| {
        LAID_OUT.with(|l| *l.borrow_mut() = Some(Vec::new()));
        for shape in &e.slide().shapes {
            if let ShapeKind::Chart(chart) = &shape.kind {
                let _ = crate::render::chart_view::chart_element(chart, shape.frame, 1.0 / EMU_PER_POINT as f32, &e.resources);
            }
        }
        LAID_OUT.with(|l| l.borrow_mut().take().unwrap_or_default())
    })
}

fn solid(rgb: u32) -> gpui_kit::Background {
    crate::render::shape_view::hsla(Color::opaque(Rgb(rgb))).into()
}

fn texts(chart: &LaidOut) -> Vec<&str> {
    chart.texts.iter().map(|t| t.0.as_str()).collect()
}

/// X1b 条件付き書式の範囲が複数の区画（`sqref="CP7:GA7 B7:BJ7"`）で最初の区画が左上でないと、相対参照の基準を取り違える。
/// 範囲外にずれた参照が `#REF!` になって X1a の panic に入る。範囲 C1:D1 A1:B1・数式 A1="土"（ファイルに # は無い）。
/// Excel は全区画を囲む矩形の左上（A1）を基準にするので、「土」の A1 と C1 に色が付く。
pub(crate) fn corpus_report_xlsx_x1b_cf_anchor_crash() {
    let p = read("cf-anchor-crash.xlsx");
    let sheet = sheet_of_slide(&p.slides[0]).unwrap();
    let (applied, _) = super::conditional::apply(sheet).expect("the rule applies");
    let colored: Vec<bool> = (0..4).map(|c| applied.styles[applied.cell(0, c).unwrap().style as usize].fill == Some(Rgb(0xFF6666))).collect();
    assert_eq!(colored, vec![true, false, true, false], "A1:D1（土・月・土・火）のうち色が付くのは A1 と C1");
}

/// X1b （落ちない形）範囲 C1:D1 A1:B1・数式 A1=1・値 1,0,0,1。ibisOffice は C1（値 0）だけに色を付ける。
/// Excel は A1 を基準にするので A1 と D1（値 1）に色が付く。
pub(crate) fn corpus_report_xlsx_x1b_cf_anchor_color() {
    let p = read("cf-anchor-color.xlsx");
    let sheet = sheet_of_slide(&p.slides[0]).unwrap();
    let (applied, _) = super::conditional::apply(sheet).expect("the rule applies");
    let colored: Vec<bool> = (0..4).map(|c| applied.styles[applied.cell(0, c).unwrap().style as usize].fill == Some(Rgb(0xFF6666))).collect();
    assert_eq!(colored, vec![true, false, false, true], "A1:D1（1,0,0,1）のうち色が付くのは A1 と D1");
}

/// X2b ヘッダー・フッターの改行が無視され、2行の見出しが1行につながって印刷される。
/// 中央と右のヘッダー、中央のフッターに2行ずつ。Excel はどれも2行に積む（154 p.2 の正解）。
pub(crate) fn corpus_report_xlsx_x2b_header_footer_line_breaks() {
    let mut s = open("multiline.xlsx");
    let marks = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        let default_style = e.presentation().workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
        super::header_footer::page_marks(&sheet.print, &default_style, 1.0, 1, 1, "Sheet1", "multiline.xlsx", &e.resources)
    });
    let runs: Vec<(String, f32)> = marks.iter().map(|m| (m.run.text.clone(), m.baseline)).collect();
    let baseline = |text: &str| {
        let found: Vec<&(String, f32)> = runs.iter().filter(|(t, _)| t.contains(text)).collect();
        assert_eq!(found.len(), 1, "「{text}」の行が1つ：{runs:?}");
        assert!(!found[0].0.contains('\n') && !found[0].0.contains('\r'), "改行が文字のまま並ぶ：{runs:?}");
        found[0].1
    };
    for (upper, lower) in [("First line of title", "Second line of title"), ("Right top", "Right bottom"), ("Footer upper", "Footer lower")] {
        assert!(baseline(lower) > baseline(upper) + 1.0, "「{lower}」は「{upper}」の下の行：{runs:?}");
    }
}

/// X2c ブック全体を印刷すると、`&P`・`&N` がシートごとに1から数え直しになる（Excel はブック全体の通し番号と総ページ数）。
/// 2シート × 2ページ、フッター「Page &P of &N」、useFirstPageNumber なし。Excel は「Page 1 of 4」〜「Page 4 of 4」。
/// 番号は書き出し（export.rs）の中で決まるので、ibisOffice の実行ファイルで PDF に書き出し、pdftotext で文字を読む。
pub(crate) fn corpus_report_xlsx_x2c_page_numbers_across_sheets() {
    let exe = std::env::current_exe().unwrap();
    let app = exe.parent().and_then(|d| d.parent()).map(|d| d.join("ibisOffice")).filter(|p| p.exists()).expect("target/<profile>/ibisOffice（cargo test がテストと一緒に作る）");
    let out = crate::shell::scenario::repo_root().join("target/scenario/xlsx::corpus_report_xlsx_x2c_page_numbers_across_sheets");
    std::fs::create_dir_all(&out).unwrap();
    let file = crate::shell::scenario::repo_root().join(DIR).join("page-numbers-across-sheets.xlsx");
    let status = std::process::Command::new(&app).arg("--export").arg("pdf").arg("--out-dir").arg(&out).arg(&file).status().expect("ibisOffice を起動できる");
    assert!(status.success(), "PDF に書き出せる");
    let pdf = out.join("page-numbers-across-sheets.pdf");
    let text = std::process::Command::new("pdftotext").arg(&pdf).arg("-").output().expect("pdftotext（poppler）が要る");
    let text = String::from_utf8_lossy(&text.stdout).into_owned();
    let footers: Vec<&str> = text.lines().map(str::trim).filter(|l| l.starts_with("Page ")).collect();
    assert_eq!(footers, vec!["Page 1 of 4", "Page 2 of 4", "Page 3 of 4", "Page 4 of 4"]);
}

/// X2d 「選択範囲内で中央」（`centerContinuous`）の数値（日付）が自分の列にしか入れず、右の空いたセルにまたがれないので `#####` になる。
/// 幅6の列で、右に空の centerContinuous のセルが2つ続く日付。Excel はその3列を1つの枠にして中央に「March 2012」を出す
/// （152 の正解）。同じ日付を1セルだけに置いた F3 は Excel でも #####（対照）。
pub(crate) fn corpus_report_xlsx_x2d_center_across_date() {
    let mut s = open("center-across-number.xlsx");
    let texts = shown(&mut s);
    assert!(all_hashes(&texts[&(2, 5)]), "F3（1セルだけ）は Excel でも #：{:?}", texts[&(2, 5)]);
    let number = texts[&(0, 5)].clone();
    drop(s);
    let mut s = open("center-across-date.xlsx");
    let date = shown(&mut s)[&(0, 1)].clone();
    assert_eq!((number.as_str(), date.as_str()), ("March 2012", "March 2023"), "F1:H1 にまたがる日付と、B1:D1 にまたがる日付（Times 斜体 12pt）");
}

/// X3a 表示形式の `*`（繰り返し文字）を、書体を見ずに文字数で決め打ちした空白で埋めるため、11pt を超える書体
/// （ＭＳ Ｐ明朝 12pt など）の会計形式の数値が全部 `####` になる。
/// 値 2552 を7つの書体・大きさ × 2つの書式（41 と #,##0）× 3つの列の幅（10・16.25・30）で並べた表。
/// Excel は `*` のせいでは # にしないので、どれも「2,552」（141 の正解は列の幅の 31%）。
pub(crate) fn corpus_report_xlsx_x3a_fill_spaces_by_font() {
    let mut s = open("fill-vs-font.xlsx");
    let texts = shown(&mut s);
    let labels = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        (1..15).map(|r| sheet.cell(r, 0).map(|c| c.text.clone()).unwrap_or_default()).collect::<Vec<_>>()
    });
    let mut wrong = Vec::new();
    for row in 1..15u32 {
        for column in 1..4u32 {
            let text = texts.get(&(row, column)).cloned().unwrap_or_default();
            if text.trim() != "2,552" {
                wrong.push(format!("{} 列{}: {text:?}", labels[row as usize - 1], column + 1));
            }
        }
    }
    assert!(wrong.is_empty(), "「2,552」と出ないセル {} 個：{wrong:#?}", wrong.len());
}

/// X3b 印刷範囲より幅の広い1列（94.29 文字）を、ページの途中で切って次のページに続けるため、右寄せの数値がページから消えて
/// 余分なページに出る。Excel は列を印刷範囲に収めて1ページに印刷する（158 p.3 の正解）。
pub(crate) fn corpus_report_xlsx_x3b_column_wider_than_page() {
    let p = read("wide-column-right-align.xlsx");
    let slide = &p.slides[0];
    let area = crate::export::export_area(&p, slide);
    let titles = crate::export::print_titles(&p, slide);
    let (_, _, pages) = crate::export::sheet_pages(slide, area, titles, (0.0, 0.0)).expect("a worksheet");
    assert_eq!(pages.len(), 1, "ページ数（印刷範囲の幅 {} pt）", area.width as f64 / EMU_PER_POINT as f64);
}

/// X3c 入っていない書体（Noto Sans JP）を字幅の広い Hiragino Sans で描き、縮めもしないため、列に収まる数値が `####` になる。
/// 9文字の列に 654,321 を Noto Sans JP 12pt（A2）と Arial 12pt（C2）で。Excel はどちらも数値を出す
/// （104 の正解は列の 84%）。
pub(crate) fn corpus_report_xlsx_x3c_substitute_font_digits() {
    let mut s = open("substitute-font-digits.xlsx");
    let texts = shown(&mut s);
    assert_eq!(texts[&(1, 2)].trim(), "654,321", "C2（Arial 12pt、対照）");
    assert_eq!(texts[&(1, 0)].trim(), "654,321", "A2（Noto Sans JP 12pt）");
}

/// X4a 円グラフのラベルで値と割合を両方出すと、値までラベルの割合の書式（0.0%）で表示され、「4,622人」が「462200.0%」になる。
/// ラベルの書式は 0.0%、系列の書式は #,##0"pcs"。Excel は割合だけにラベルの書式を使い、値は系列の書式
/// （「Alpha, 4,622pcs, 88.0%」）。
pub(crate) fn corpus_report_xlsx_x4a_pie_label_value_format() {
    let mut s = open("pie-label-numfmt.xlsx");
    let laid = charts(&mut s);
    let texts = texts(&laid[0]);
    let alpha = texts.iter().find(|t| t.contains("Alpha")).unwrap_or_else(|| panic!("Alpha のラベル：{texts:?}"));
    assert!(alpha.contains("4,622pcs") && !alpha.contains("462200"), "Alpha のラベル {alpha:?}（Excel は「Alpha, 4,622pcs, 88.0%」）");
}

/// X4b データラベルの区切り文字（`c:separator`）を読まず、改行区切りのラベルも「, 」でつないで1行にする。
/// 2つ目の円グラフは区切りが改行（`&#10;`）。Excel は分類名・値・割合を3行に積む（103 p.4 の正解）。
pub(crate) fn corpus_report_xlsx_x4b_pie_label_separator() {
    let mut s = open("pie-label-numfmt.xlsx");
    let laid = charts(&mut s);
    let texts = texts(&laid[1]);
    let alpha = texts.iter().find(|t| t.contains("Alpha")).unwrap_or_else(|| panic!("Alpha のラベル：{texts:?}"));
    assert_eq!(alpha.lines().count(), 3, "Alpha のラベル {alpha:?} は3行");
    assert!(!alpha.contains(", "), "Alpha のラベル {alpha:?} を「, 」でつながない");
}

/// X4c 自動レイアウトの円グラフで円が大きすぎる（外側のラベルの分として1行分しか空けないため）。
/// 行の高さを 20pt に固定した 300pt の枠（原因1の行の高さを除いたもの）。ibisOffice は円の直径が枠の高さの約84%、
/// Excel は 102 の正解で約63%。許容は両者の間の 75% まで（ラベルの長さで Excel の値は動くので、きっちりは決めない）。
pub(crate) fn corpus_report_xlsx_x4c_pie_size() {
    let mut s = open("pie-label-numfmt.xlsx");
    let laid = charts(&mut s);
    let chart = &laid[0];
    let slices = [0x5B9BD5, 0xA5D8F3, 0xED7D31, 0x70AD47].map(solid);
    let (x0, y0, x1, y1) = chart.fills.iter().filter(|(_, bg)| slices.contains(bg)).fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |(a, b, c, d), ((l, t, r, w), _)| (a.min(*l), b.min(*t), c.max(*r), d.max(*w)));
    assert!(x1 > x0, "円の扇形が描かれる");
    let ratio = (x1 - x0).max(y1 - y0) / chart.size.1;
    assert!(ratio <= 0.75, "円の直径は枠の高さの {:.0}%（Excel は約63%）", ratio * 100.0);
}

/// X4d 2段の項目軸（`multiLvlStrRef`）の外側の段を描かず、「2025年／2026年」の区分が出ない。
/// 6か月の棒グラフで、外側の段は「Year A」「Year B」（noMultiLvlLbl=0）。Excel は月の下に年の段を出す（109 p.1 の正解）。
pub(crate) fn corpus_report_xlsx_x4d_multilevel_category_outer_level() {
    let mut s = open("multilevel-category.xlsx");
    let laid = charts(&mut s);
    let texts = texts(&laid[0]);
    assert!(texts.contains(&"Jan"), "内側の段：{texts:?}");
    assert!(texts.contains(&"Year A") && texts.contains(&"Year B"), "外側の段「Year A」「Year B」：{texts:?}");
}

/// X5a 立体の棒グラフ（`bar3DChart`）を平面の棒グラフとして描き、灰色の壁と床を描かないため、白い枠だけになる。
/// 壁・床は灰色（C0C0C0）、plotArea は noFill。Excel は灰色の奥の壁・横の壁・床を描く（161 p.1 の正解）。
pub(crate) fn corpus_report_xlsx_x5a_bar3d_walls() {
    let mut s = open("x5a-bar3d-walls.xlsx");
    let laid = charts(&mut s);
    let gray = solid(0xC0C0C0);
    let walls: Vec<_> = laid[0].fills.iter().filter(|(_, bg)| *bg == gray).map(|(r, _)| *r).collect();
    assert!(!walls.is_empty(), "灰色の壁・床が描かれる（プロットエリア {:?}）", laid[0].plot);
}

/// X5b 値がすべて0の系列で、縦軸が 0〜0.1（目盛り2本）になり、Excel の 0〜1（0.2 刻み、6本）にならない。
/// 2つ目のグラフは軸の書式 0.0。Excel の目盛りは 0.0・0.2・…・1.0（161 p.1 の正解）。
pub(crate) fn corpus_report_xlsx_x5b_all_zero_value_axis() {
    let mut s = open("x5b-all-zero-axis.xlsx");
    let laid = charts(&mut s);
    let texts = texts(&laid[1]);
    let mut ticks: Vec<&str> = texts.iter().copied().filter(|t| t.parse::<f64>().is_ok()).collect();
    ticks.sort_by(|a, b| a.parse::<f64>().unwrap().total_cmp(&b.parse::<f64>().unwrap()));
    assert_eq!(ticks, vec!["0.0", "0.2", "0.4", "0.6", "0.8", "1.0"], "縦軸の目盛り（全部の文字：{texts:?}）");
}

/// X5c 横棒と散布図を重ねたグラフ（ドットプロット）で、散布図の点の数を項目数に数えて棒が下に押し込まれる。
/// 横棒8本と散布図の点24個。Excel は8項目の横棒を高さいっぱいに並べる（145 p.1 の正解）：
/// いちばん上の棒はプロットエリアの上から 1/8 の帯に入る。
pub(crate) fn corpus_report_xlsx_x5c_bar_scatter_combo() {
    let mut s = open("x5c-bar-scatter-combo.xlsx");
    let laid = charts(&mut s);
    let chart = &laid[0];
    let bar = solid(0xD9D9D9);
    let bars: Vec<(f32, f32, f32, f32)> = chart.fills.iter().filter(|(_, bg)| *bg == bar).map(|(r, _)| *r).collect();
    assert_eq!(bars.len(), 8, "横棒8本");
    let (_, top, _, height) = chart.plot;
    let highest = bars.iter().map(|b| b.1).fold(f32::MAX, f32::min);
    assert!(highest - top < height / 8.0, "いちばん上の棒の上端はプロットエリアの上から {:.0}%（Excel は 12.5% 以内）", (highest - top) / height * 100.0);
}

/// X5d 軸のタイトルを回さず、横書きで描くため、縦軸の見出しが折れ線や表に重なる。
/// 縦軸のタイトルは rot="-5400000"（1つ目）と bodyPr だけ（2つ目、縦の軸の既定は -90°）。Excel はどちらも縦書き
/// （149・151 の正解）。
pub(crate) fn corpus_report_xlsx_x5d_axis_title_rotation() {
    let mut s = open("x5d-axis-title-rotation.xlsx");
    let laid = charts(&mut s);
    for (i, chart) in laid.iter().enumerate() {
        let title = chart.texts.iter().find(|t| t.0 == "Cost per unit distance").unwrap_or_else(|| panic!("グラフ {} の縦軸のタイトル", i + 1));
        assert!((title.2.rem_euclid(360.0) - 270.0).abs() < 0.5, "グラフ {} の縦軸のタイトルの向き {}°（Excel は -90°）", i + 1, title.2);
    }
}

/// X5e 項目軸に `rot="0"` が指定されていても、項目名が帯の幅を超えると 45° に回すため、項目名が斜めに並ぶ。
/// 2段の項目名（4年 × 4四半期）、項目軸は rot="0"。Excel は横書きのまま帯の幅で折り返す（107 p.1 の正解）。
pub(crate) fn corpus_report_xlsx_x5e_explicit_rot0_categories() {
    let mut s = open("x5e-multilevel-categories.xlsx");
    let laid = charts(&mut s);
    let quarters: Vec<&(String, (f32, f32), f32)> = laid[0].texts.iter().filter(|t| ["Apr-Jun", "Jul-Sep", "Oct-Dec", "Jan-Mar"].contains(&t.0.replace('\n', "").as_str())).collect();
    assert!(!quarters.is_empty(), "四半期の項目名：{:?}", laid[0].texts);
    let turned: Vec<_> = quarters.iter().filter(|t| t.2 != 0.0).map(|t| (t.0.as_str(), t.2)).collect();
    assert!(turned.is_empty(), "rot=\"0\" の項目名が回る：{turned:?}");
}

/// X6b 印刷範囲の無いシートで、使われている最後の列からはみ出して右へ続く文字が、使用範囲の右端で切れる。
/// A 列（幅12）だけに長い英文の題名を15行。Excel は文字が届く列まで印刷する（172 p.1 の正解は約 450pt の幅まで読める）：
/// 印刷の幅は、いちばん長い題名の幅以上。
pub(crate) fn corpus_report_xlsx_x6b_overflow_text_print_area() {
    let mut s = open("X6b-overflow-text-past-used-range.xlsx");
    let (area, widest) = s.with(|e, app| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        let widest = sheet.cells.iter().map(|c| super::sheet_view::text_width_pt(&c.text, &sheet.styles[c.style as usize], &e.resources, app.text_system())).fold(0.0, f32::max);
        (crate::export::export_area(e.presentation(), e.slide()), widest)
    });
    let width = area.width as f32 / EMU_PER_POINT as f32;
    assert!(widest > 200.0, "題名の幅 {widest} pt");
    assert!(width >= widest, "印刷の幅 {width} pt（いちばん長い題名は {widest} pt）");
}

/// X7a 列の幅を Excel for Mac の「1画素＝1pt、数字の幅 6pt」で数えるため、どの列も Excel（Windows・web）より約9%広くなり、
/// ページの右端の列が次のページへ押し出される。ＭＳ Ｐゴシック 11、幅 2.43 文字の列39列、印刷範囲 A1:AM30、scale=93。
/// Excel は1ページ（ibisOffice は右端の列だけのページができて2ページ）。
pub(crate) fn corpus_report_xlsx_x7a_narrow_columns_fit_one_page() {
    let p = read("X6d-narrow-columns-point-rounding.xlsx");
    let slide = &p.slides[0];
    let area = crate::export::export_area(&p, slide);
    let titles = crate::export::print_titles(&p, slide);
    let (_, scale, pages) = crate::export::sheet_pages(slide, area, titles, (0.0, 0.0)).expect("a worksheet");
    assert_eq!(pages.len(), 1, "ページ数（印刷範囲の幅 {} pt、{}%）", area.width as f64 / EMU_PER_POINT as f64, scale * 100.0);
}

/// X8c 幅の足りない列の論理値（FALSE）が「##」にならず「FAL／SE」と折り返して出る（折り返し指定の数値も同じ）。
/// 幅 5.5 の C 列に、文字列の書式・折り返しの FALSE（C3）と、折り返しの数値 123456789（C7）。Excel は論理値と数値を
/// 折り返さず、入らなければ # で埋める（135 p.2 の正解は「##」）。
pub(crate) fn corpus_report_xlsx_x8c_narrow_boolean_and_wrapped_number() {
    let mut s = open("x8c-narrow-boolean.xlsx");
    let texts = shown(&mut s);
    assert!(all_hashes(&texts[&(7, 2)]), "C8（折り返しなしの数値、対照）：{:?}", texts[&(7, 2)]);
    assert!(all_hashes(&texts[&(2, 2)]), "C3（FALSE・文字列の書式・折り返し）：{:?}", texts[&(2, 2)]);
    assert!(all_hashes(&texts[&(6, 2)]), "C7（123456789・折り返し）：{:?}", texts[&(6, 2)]);
}

/// The file exported as PDF as `--export pdf` writes it, into `target/scenario/corpus_report_xlsx/<label>/`.
fn export_pdf(file: &str, label: &str) -> PathBuf {
    let root = crate::shell::scenario::repo_root();
    let path = root.join(DIR).join(file);
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("{file}: {e}"));
    let presentation = ibis_pptx::read(&data).unwrap_or_else(|e| panic!("{file}: {e}"));
    let out_dir = root.join("target/scenario/corpus_report_xlsx").join(label);
    std::fs::create_dir_all(&out_dir).unwrap();
    let name = Path::new(file).file_stem().unwrap().to_string_lossy().into_owned();
    // The command line's defaults (`--export pdf` without options).
    let options = Options {
        format: Format::Pdf,
        out_dir: out_dir.clone(),
        name: name.clone(),
        width: 1920,
        slides: Vec::new(),
        layout: false,
        file_name: Some(file.into()),
        print_backgrounds: false,
        print_hidden_text: false,
        print_markup: true,
        color: Default::default(),
        handout: None,
        frame: false,
        handout_landscape: false,
        notes_pages: false,
        outline_pages: false,
        notes_source: None,
        handout_marks: Default::default(),
    };
    let result = export(presentation, &options);
    // The export draws without font smoothing; the window's scenarios after this one draw with it.
    gpui_macos::set_font_smoothing(true);
    result.unwrap_or_else(|e| panic!("{file}: {e}"));
    out_dir.join(format!("{name}.pdf"))
}

/// A line of text on a PDF page (`pdftotext -bbox-layout`), in points from the page's top left.
struct TextLine {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    text: String,
    /// Where each word starts and ends on the x axis.
    words: Vec<(f32, f32)>,
}

impl TextLine {
    /// The text without spaces (pdftotext puts a space where a gap is wide).
    fn compact(&self) -> String {
        // The substitute font (Hiragino Sans for ＭＳ Ｐゴシック) shares glyphs between the full-width and the half-width
        // forms, and pdftotext reports the half-width ones: read ﾐ and ｰ as ミ and ー.
        self.text.chars().filter(|c| !c.is_whitespace()).map(|c| match c { 'ﾐ' => 'ミ', 'ｰ' => 'ー', c => c }).collect()
    }
}

/// The lines of text of each page of the PDF, by page.
fn pdf_pages(pdf: &Path) -> Vec<Vec<TextLine>> {
    let out = Command::new("pdftotext").arg("-bbox-layout").arg(pdf).arg("-").output().expect("pdftotext（poppler）が要る");
    let xml = String::from_utf8_lossy(&out.stdout).into_owned();
    let number = |tag: &str, name: &str| -> f32 {
        let start = tag.find(&format!("{name}=\"")).unwrap_or_else(|| panic!("{name} in {tag}")) + name.len() + 2;
        tag[start..].split('"').next().unwrap().parse().unwrap()
    };
    let mut pages: Vec<Vec<TextLine>> = Vec::new();
    let mut open: Option<TextLine> = None;
    for line in xml.lines().map(str::trim) {
        if line.starts_with("<page ") {
            pages.push(Vec::new());
        } else if line.starts_with("<line ") {
            open = Some(TextLine { x0: number(line, "xMin"), y0: number(line, "yMin"), x1: number(line, "xMax"), y1: number(line, "yMax"), text: String::new(), words: Vec::new() });
        } else if line.starts_with("<word ") {
            if let Some(l) = open.as_mut() {
                l.words.push((number(line, "xMin"), number(line, "xMax")));
                let word = line.split_once('>').unwrap().1.trim_end_matches("</word>");
                let word = word.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&");
                if !l.text.is_empty() {
                    l.text.push(' ');
                }
                l.text.push_str(&word);
            }
        } else if line == "</line>" {
            if let (Some(l), Some(page)) = (open.take(), pages.last_mut()) {
                page.push(l);
            }
        }
    }
    pages
}

/// A page of the PDF drawn in grey levels.
struct Raster {
    width: usize,
    height: usize,
    gray: Vec<u8>,
    px_per_pt: f32,
}

impl Raster {
    fn px(&self, pt: f32) -> usize {
        (pt * self.px_per_pt).round().max(0.0) as usize
    }

    fn gray_at(&self, x_pt: f32, y_pt: f32) -> u8 {
        let (x, y) = (self.px(x_pt).min(self.width - 1), self.px(y_pt).min(self.height - 1));
        self.gray[y * self.width + x]
    }

    /// How many pixels darker than `limit` there are in the rectangle (points).
    fn ink(&self, (x0, y0, x1, y1): (f32, f32, f32, f32), limit: u8) -> usize {
        let (x0, x1, y0, y1) = (self.px(x0), self.px(x1).min(self.width), self.px(y0), self.px(y1).min(self.height));
        (y0..y1).map(|y| self.gray[y * self.width + x0..y * self.width + x1].iter().filter(|v| **v < limit).count()).sum()
    }

    /// The runs of pixels darker than `limit` going down the pixel column at `x_pt` between `y0_pt` and `y1_pt`: (start, length) in pixels.
    fn dark_runs_down(&self, x_pt: f32, y0_pt: f32, y1_pt: f32, limit: u8) -> Vec<(usize, usize)> {
        let x = self.px(x_pt).min(self.width - 1);
        let mut runs: Vec<(usize, usize)> = Vec::new();
        let mut inside = false;
        for y in self.px(y0_pt)..self.px(y1_pt).min(self.height) {
            let dark = self.gray[y * self.width + x] < limit;
            match (dark, inside) {
                (true, false) => runs.push((y, 1)),
                (true, true) => runs.last_mut().unwrap().1 += 1,
                _ => {}
            }
            inside = dark;
        }
        runs
    }

    /// The pixel rows whose dark pixels (darker than `limit`) number at least `length`, as (first row, last row) groups.
    fn rows_with_ink(&self, length: usize, limit: u8) -> Vec<(usize, usize)> {
        let mut groups: Vec<(usize, usize)> = Vec::new();
        for y in 0..self.height {
            if self.gray[y * self.width..(y + 1) * self.width].iter().filter(|v| **v < limit).count() >= length {
                match groups.last_mut() {
                    Some((_, last)) if *last + 1 == y => *last = y,
                    _ => groups.push((y, y)),
                }
            }
        }
        groups
    }
}

impl Raster {
    /// The horizontal rules: groups of pixel rows with at least `length` dark pixels (darker than `limit`), as (y of the first row, leftmost x, rightmost x) in points.
    fn horizontal_rules(&self, length: usize, limit: u8) -> Vec<(f32, f32, f32)> {
        let mut rules: Vec<(f32, f32, f32)> = Vec::new();
        let mut last_row = None;
        for y in 0..self.height {
            let row = &self.gray[y * self.width..(y + 1) * self.width];
            let dark: Vec<usize> = (0..self.width).filter(|x| row[*x] < limit).collect();
            if dark.len() < length {
                continue;
            }
            let (first, last) = (dark[0] as f32 / self.px_per_pt, *dark.last().unwrap() as f32 / self.px_per_pt);
            match rules.last_mut() {
                Some(rule) if last_row == Some(y - 1) => {
                    rule.1 = rule.1.min(first);
                    rule.2 = rule.2.max(last);
                }
                _ => rules.push((y as f32 / self.px_per_pt, first, last)),
            }
            last_row = Some(y);
        }
        rules
    }

    /// The vertical rules: pixel columns with an unbroken run of at least `length_pt` of dark pixels (darker than `limit`), as the x of each group's middle in points.
    fn vertical_rules(&self, length_pt: f32, limit: u8) -> Vec<f32> {
        let length = self.px(length_pt);
        let mut columns: Vec<usize> = Vec::new();
        for x in 0..self.width {
            let mut run = 0;
            let mut longest = 0;
            for y in 0..self.height {
                if self.gray[y * self.width + x] < limit {
                    run += 1;
                    longest = longest.max(run);
                } else {
                    run = 0;
                }
            }
            if longest >= length {
                columns.push(x);
            }
        }
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for x in columns {
            match groups.last_mut() {
                Some(group) if *group.last().unwrap() + 1 == x => group.push(x),
                _ => groups.push(vec![x]),
            }
        }
        groups.iter().map(|g| (g[0] + g[g.len() - 1]) as f32 / 2.0 / self.px_per_pt).collect()
    }

    /// The x (points) of the ink's centre of mass between the rows `y0_pt` and `y1_pt` and the columns `x0_pt` and `x1_pt`.
    fn centroid_x(&self, (x0_pt, y0_pt, x1_pt, y1_pt): (f32, f32, f32, f32)) -> f32 {
        let (x0, x1, y0, y1) = (self.px(x0_pt), self.px(x1_pt).min(self.width), self.px(y0_pt), self.px(y1_pt).min(self.height));
        let (mut sum, mut mass) = (0.0f64, 0.0f64);
        for y in y0..y1 {
            for x in x0..x1 {
                let ink = 255.0 - self.gray[y * self.width + x] as f64;
                sum += ink * x as f64;
                mass += ink;
            }
        }
        (sum / mass.max(1.0)) as f32 / self.px_per_pt
    }
}

/// Page `page` (1-based) of the PDF drawn at `dpi` by `pdftoppm`.
fn raster(pdf: &Path, page: usize, dpi: u32) -> Raster {
    let base = pdf.with_file_name(format!("{}-p{page}-{dpi}", pdf.file_stem().unwrap().to_string_lossy()));
    let status = Command::new("pdftoppm").args(["-r", &dpi.to_string(), "-f", &page.to_string(), "-l", &page.to_string(), "-singlefile", "-gray"]).arg(pdf).arg(&base).status().expect("pdftoppm（poppler）が要る");
    assert!(status.success(), "pdftoppm");
    let data = std::fs::read(format!("{}.pgm", base.display())).unwrap();
    let mut at = 0;
    let mut token = || {
        while data[at].is_ascii_whitespace() {
            at += 1;
        }
        let start = at;
        while !data[at].is_ascii_whitespace() {
            at += 1;
        }
        String::from_utf8_lossy(&data[start..at]).into_owned()
    };
    assert_eq!(token(), "P5");
    let (width, height): (usize, usize) = (token().parse().unwrap(), token().parse().unwrap());
    let _ = token();
    let gray = data[at + 1..].to_vec();
    assert_eq!(gray.len(), width * height, "pgm");
    Raster { width, height, gray, px_per_pt: dpi as f32 / 72.0 }
}

/// The pages the sheet prints on, as `export` makes them.
fn printed_pages(name: &str) -> (usize, f64) {
    let p = read(name);
    // The sheet as `export` lays it out for printing.
    let mut printed = p.slides[0].clone();
    crate::export::print_layout(&p, &mut printed);
    let slide = &printed;
    let area = crate::export::export_area(&p, slide);
    let titles = crate::export::print_titles(&p, slide);
    let (_, scale, pages) = crate::export::sheet_pages(slide, area, titles, (0.0, 0.0)).expect("a worksheet");
    (pages.len(), scale)
}

/// xlsx-C014-b1（X6b の直しの副作用。todo-xlsx の X6b は直した）印刷範囲の無いシートで、折り返しなしの長い文字が
/// ページの幅を越えて右へはみ出すと、はみ出す文字の届く列まで範囲を広げるため、文の続きだけの白紙に近いページが右に増える。
/// 後退（92341f7c1 では 1 ページ、原因の見当 0501642d2 の X6b の直し）。
/// A〜K 列（幅 6、Arial 10）× 40 行の表は Letter 縦の 1 ページの幅に収まり、A5 に約 150 文字の英文が右へはみ出す。
/// Excel は文字をページの幅で切り、右へページを足さない（実文書の正解 PDF は行の帯ごとに 1 ページ）：1 ページ。ibisOffice は 2 ページ。
pub(crate) fn corpus_report_xlsx_overflow_text_past_page_width_adds_no_page() {
    let (pages, _) = printed_pages("overflow-text-wider-than-page.xlsx");
    assert_eq!(pages, 1, "ページ数（文がページの幅を越えると右にページが増える）");
}

/// xlsx-C014-b2 手動の改ページ（`rowBreaks`）の直後の行の上罫線が、前のページの下端に引かれる。
/// 58 行 × A〜H 列（Arial 10、既定の行の高さ、拡大縮小 82%、Letter 縦）、改ページは 57 行目の下、58 行目だけ上罫線（thin）。
/// 実文書の正解 PDF では、ページの境に当たる罫線は前のページの下端には引かれない：1 ページ目に水平線は無い。
/// ibisOffice は 1 ページ目の最後の行（Row 57）の下に A〜H 列の幅の水平線を引く。
pub(crate) fn corpus_report_xlsx_manual_break_top_border_not_on_previous_page() {
    let pdf = export_pdf("break-border-top-of-next-page.xlsx", "break_border");
    let page = raster(&pdf, 1, 100);
    let rules: Vec<f32> = page.rows_with_ink(200, 128).iter().map(|(first, _)| *first as f32 / page.px_per_pt).collect();
    assert!(rules.is_empty(), "1 ページ目の下に幅 200 画素以上の水平線がある（y = {rules:?} pt）");
}

/// xlsx-C002-b1 `customHeight` の行の高さが小数（24.75pt）のとき、Excel は整数ポイントに切り捨てた高さ（24pt）で行を並べるが、
/// ibisOffice は小数のまま並べるので、同じ用紙に載る行が少なく、表の最下行が次のページに回る。
/// 30 行（各 24.75pt、`customHeight`）、A4 縦、拡大縮小 100%、余白は既定。Excel は 30 × 24 = 720pt で 1 ページ（Total の行まで）。
/// ibisOffice は 742.5pt で 2 ページ（Total の行だけが 2 ページ目）。切り捨ては拡大縮小の前に行われる。
pub(crate) fn corpus_report_xlsx_custom_row_height_floors_to_whole_points() {
    let (pages, _) = printed_pages("custom-row-height-fraction.xlsx");
    assert_eq!(pages, 1, "ページ数（行の高さ 24.75pt を 24pt に切り捨てれば 30 行が 1 ページに入る）");
}

/// xlsx-C003-b1 ＭＳ 系の書体（ＭＳ 明朝・ＭＳ ゴシック・ＭＳ Ｐゴシック）の折り返す複数行のセルで、行送りが正解より大きい。
/// 行の高さが「行送り × 行数」ちょうどに作られた表では、最後の行が下の罫線に接して切れる。
/// 正解の行送りは文字の大きさの約 1.23 倍（ＭＳ 明朝 11 で 13.5pt、9 で 11.05pt、ＭＳ ゴシック 12 で 14.7pt）。
/// ibisOffice は約 1.44 倍（それぞれ 16.0pt・13.0pt・17.0pt）。許容は正解より 0.7pt まで。
/// 2 行ずつ 3 つのセル（ＭＳ 明朝 11、行の高さ 27）、4 行（ＭＳ 明朝 9、48.6）、2 行（ＭＳ ゴシック 12、30）。
pub(crate) fn corpus_report_xlsx_ms_font_wrapped_line_pitch() {
    let pdf = export_pdf("ms-font-line-pitch.xlsx", "ms_line_pitch");
    let pages = pdf_pages(&pdf);
    let lines = &pages[0];
    let top = |text: &str| lines.iter().find(|l| l.compact() == text).unwrap_or_else(|| panic!("「{text}」の行")).y0;
    let mut pitches: Vec<(String, f32, f32)> = Vec::new();
    for n in 1..=3 {
        pitches.push((format!("ＭＳ 明朝 11 の {n} つ目"), top(&format!("さしすせそたちつてと{n}")) - top(&format!("あいうえおかきくけこ{n}")), 11.0));
    }
    let mut dummy: Vec<f32> = lines.iter().filter(|l| l.compact().ends_with("ダミー文面です。")).map(|l| l.y0).collect();
    dummy.sort_by(f32::total_cmp);
    assert_eq!(dummy.len(), 6, "ダミー文面の行（4 行と 2 行）：{dummy:?}");
    pitches.push(("ＭＳ 明朝 9 の 4 行".into(), (dummy[3] - dummy[0]) / 3.0, 9.0));
    pitches.push(("ＭＳ ゴシック 12 の 2 行".into(), dummy[5] - dummy[4], 12.0));
    let wrong: Vec<String> = pitches.iter().filter(|(_, pitch, size)| *pitch > size * 1.2273 + 0.7).map(|(what, pitch, size)| format!("{what}：行送り {pitch:.1}pt（正解は {:.1}pt）", size * 1.2273)).collect();
    assert!(wrong.is_empty(), "行送りが大きい：{wrong:#?}");
}

/// xlsx-C003-b2 折り返すセルの半角スペースを折り返しの位置にしない。Excel は、行があふれるとき、その行にある直近の半角スペースの
/// 後ろで折り返す（先頭が半角スペースなら 1 行目は空行になり、本文は 2 行目から。文中の半角スペースの後ろで行が終わる）。
/// ＭＳ 明朝 9、幅 52 の列、折り返し・上揃え。B1 は先頭が半角スペースの文、B2 は文中に「１ ダミー甲の者」を含む文（全角スペースで始まる）。
/// Excel は B1 の 1 行目を空行にして本文を 2 行目（セルの上端から約 13pt 下）から、B2 は「１」で行を終えて次の行を「ダミー甲」から始める。
/// ibisOffice は半角スペースを見ず、B1 の本文を 1 行目（セルの上端から 3pt 下）から、B2 を文字の間で折る。
pub(crate) fn corpus_report_xlsx_wrap_at_ascii_space() {
    let pdf = export_pdf("ascii-space-wrap.xlsx", "ascii_space_wrap");
    let pages = pdf_pages(&pdf);
    let lines = &pages[0];
    let first = lines.iter().filter(|l| l.compact().starts_with("注意事項の説明文")).map(|l| l.y0).reduce(f32::min).expect("B1 の本文");
    assert!(first >= 49.0, "B1（先頭が半角スペース）の本文の 1 行目の位置 {first}pt（セルの上端は 36pt。Excel は 1 行目が空行で 52pt 付近）");
    let after_space = lines.iter().any(|l| l.compact().starts_with("ダミー甲"));
    assert!(after_space, "B2 の「１ ダミー甲」の半角スペースの後ろで行が変わる（「ダミー甲」で始まる行が無い）：{:?}", lines.iter().map(|l| l.compact()).collect::<Vec<_>>());
}

/// xlsx-C003-b4 縦位置の指定が無い（下揃え）一行の文字が、低い行で上の罫線に接する。
/// ＭＳ ゴシック 10、行の高さ 13.5pt（`customHeight` なし）の 2 行目で、上の行（18pt）は罫線で囲んだ見出し帯。
/// 正解の文字は行の下端に付き、下端は行の下端の約 0.4pt 上（上端は行の上端の約 1.9pt 下）。ibisOffice は文字の上端が行の上端に付き
/// （y=53.7pt、下に 3.8pt の余白）、見出し帯の下線に接する。許容は行の下端まで 1.5pt。行は 54〜67.5pt。
pub(crate) fn corpus_report_xlsx_bottom_aligned_line_sits_at_the_row_bottom() {
    let pdf = export_pdf("bottom-align-first-line.xlsx", "bottom_align");
    let pages = pdf_pages(&pdf);
    let line = pages[0].iter().find(|l| l.compact().starts_with("１見出し帯の下の")).expect("2 行目の文字");
    assert!(67.5 - line.y1 <= 1.5, "文字の下端 {}pt（行の下端 67.5pt との差 {:.1}pt）", line.y1, 67.5 - line.y1);
}

/// xlsx-C003-b5 図形（`xdr:sp`、角丸四角形）の `a:bodyPr vertOverflow="clip" horzOverflow="clip"` を見ず、図形の高さを越える文字を
/// 図形の外まで描く。正解の文字は図形の下端で切れ、図形の外に文字は出ない。
/// 図形は行 2〜13（約 231pt まで）、中に 36pt の見出しと 16pt の本文 4 段落（折り返して 10 行）。
/// ibisOffice は本文の最後の行（236〜265pt）が図形の下に描かれる。図形の下端の下（236〜258pt、本文の文字が出る幅）に字の画素が無い。
pub(crate) fn corpus_report_xlsx_shape_text_clipped_at_shape_bottom() {
    let pdf = export_pdf("shape-vert-overflow-clip.xlsx", "shape_clip");
    let page = raster(&pdf, 1, 100);
    let ink = page.ink((140.0, 236.0, 420.0, 258.0), 110);
    assert_eq!(ink, 0, "図形の下端の外に字の画素が {ink} 個ある（図形は clip）");
}

/// xlsx-C004-b1・xlsx-C007-b3（同じきっかけ）「選択範囲内で中央」（`centerContinuous`）＋折り返し（`wrapText`）のセルを、
/// 選択範囲（字のあるセルと、右の空の同じ書式のセル）ではなく 1 つの列の幅で折り返し、行の高さも測り直して伸ばす。
/// 正解は 1 行で範囲の中央に出て、行の高さはファイルの `ht` のまま（実文書の正解 PDF）。
/// center-across-wrap-header.xlsx：8 列、見出し 4 組（それぞれ 2 列にまたがる 10 字、ＭＳ 明朝 11、`ht=26.4` で `customHeight` なし）。
/// ibisOffice は見出しを 3 行に折り返して行を伸ばし、1 行目の下の数値の先頭が 80.1pt から 94.2pt に下がる。
/// center-across-wrap-two-cells.xlsx：B2 に 8 字（B 幅 56pt、B+C 112pt）、C2 は空で同じ書式。ibisOffice は「ダミー項目」「名称一」の 2 行。
pub(crate) fn corpus_report_xlsx_center_across_wrapped_header_uses_the_whole_range() {
    let pdf = export_pdf("center-across-wrap-header.xlsx", "center_across_header");
    let pages = pdf_pages(&pdf);
    let lines = &pages[0];
    let mut wrong = Vec::new();
    for heading in ["ダミー項目名テスト欄", "ダミー指標区分名称欄", "ダミー分類名称区分欄", "ダミー合計金額表示欄"] {
        if !lines.iter().any(|l| l.compact().contains(heading)) {
            wrong.push(format!("見出し「{heading}」が 1 行に出ない"));
        }
    }
    // 見出しの行の高さを、罫線で測る（表の左の縦線の上端から、見出しの下の横線まで）。字の位置で測ると、本物の ＭＳ 明朝 が無い PC では
    // 行の高さでなく代わりの書体の字の上端の差が混ざる。1 ページに収める縮小（fitToPage）があるので、次の数値の行（`ht=14.4`）の間隔との比で見る。
    // 正解は `ht=26.4` のままで 26.4 / 14.4 = 1.83 倍。折り返して伸びると 3 行分（約 14pt）増えて 2.8 倍を越える。画素（200 dpi）の丸めは 1 画素で約 0.03 倍。
    let page = raster(&pdf, 1, 200);
    let rules = page.horizontal_rules(page.px(300.0), 200);
    let left = *page.vertical_rules(100.0, 200).first().expect("表の左の縦線");
    let table_top = page.dark_runs_down(left, 0.0, 400.0, 200).first().expect("表の左の縦線の上端").0 as f32 / page.px_per_pt;
    let header_height = rules.first().expect("見出しの下の横線").0 - table_top;
    let row_pitch = rules[1].0 - rules[0].0;
    let ratio = header_height / row_pitch;
    if (ratio - 26.4 / 14.4).abs() > 0.1 {
        wrong.push(format!("見出しの行の高さは次の行の {ratio:.2} 倍（行の高さが `ht` のままなら 26.4 / 14.4 = 1.83 倍。見出し {header_height}pt、次の行 {row_pitch}pt）"));
    }
    let pdf = export_pdf("center-across-wrap-two-cells.xlsx", "center_across_two");
    let pages = pdf_pages(&pdf);
    if !pages[0].iter().any(|l| l.compact().contains("ダミー項目名称一")) {
        wrong.push("B2:C2 の見出し「ダミー項目名称一」が 1 行に出ない".into());
    }
    assert!(wrong.is_empty(), "選択範囲内で中央＋折り返し：{wrong:#?}");
}

/// xlsx-C007-b2 幅・高さが 0 のテキスト ボックス（`<a:ext cx="0" cy="0"/>`、`bodyPr vertOverflow="clip"`）の文字を描く。
/// 正解は大きさ 0 で内側を切るので何も出ない。from と to が同じセルの `twoCellAnchor` のテキスト ボックスが 2 つ（「ダミー見出し甲」「Dummy heading A」など）。
/// ibisOffice は文字が重なって出る（x≈300〜370pt、y≈52〜83pt）。
pub(crate) fn corpus_report_xlsx_zero_size_text_box_draws_nothing() {
    let pdf = export_pdf("zero-size-text-box.xlsx", "zero_size_text_box");
    let pages = pdf_pages(&pdf);
    let texts: Vec<String> = pages[0].iter().map(TextLine::compact).collect();
    assert!(texts.iter().any(|t| t == "Dummytitle"), "セルの文字は出る：{texts:?}");
    let drawn: Vec<&String> = texts.iter().filter(|t| t.contains("ダミー見出し") || t.contains("Dummyheading")).collect();
    assert!(drawn.is_empty(), "大きさ 0 のテキスト ボックスの文字が出る：{drawn:?}");
}

/// xlsx-C007-b5（todo-xlsx の H012）`horizontal="distributed"`（均等割り付け）の見出しの文字を、セルの幅いっぱいに散らして並べる。
/// 実文書の正解 PDF では、見出し（ＭＳ 明朝 9・10pt）は散らさず中央に並び、長い見出しは折り返して中央に並ぶ。
/// 見出し「ダミー甲」（4 字）と「ダミー丙丁」（5 字）、9pt。ibisOffice はどちらもセルの幅いっぱいの 54pt に散らす（「ダ ミ ー 甲」）。散らさなければ 36pt と 45pt。許容は字数 × 9pt + 4pt まで。
/// ※ 正解は Excel for the web 由来の見え方の可能性がある（確度は低い。Excel のデスクトップ版が均等割り付けを散らすかは未確認）。
pub(crate) fn corpus_report_xlsx_distributed_header_not_spread() {
    let pdf = export_pdf("distributed-header.xlsx", "distributed_header");
    let pages = pdf_pages(&pdf);
    let spread: Vec<String> = pages[0]
        .iter()
        .filter(|l| ["ダミー甲", "ダミー丙丁"].contains(&l.compact().as_str()) && l.x1 - l.x0 > l.compact().chars().count() as f32 * 9.0 + 4.0)
        .map(|l| format!("「{}」の幅 {:.1}pt", l.text, l.x1 - l.x0))
        .collect();
    assert!(spread.is_empty(), "見出しが散らされる：{spread:?}");
}

/// xlsx-C007-b6 リッチテキストで文字の大きさが混ざる折り返しセルの行間が広く（9pt の run の行も 11pt の行と同じ行送り）、
/// `shrinkToFit` を付けると run の大きさ（9pt）が無視されて 11pt で描かれる。
/// 結合セル（2 行分）の文字列は 1 行目が 11pt、2〜3 行目（「（ダミー注記の一行目⏎ダミー注記の二行目）」）が `sz=9` の run、書式は ＭＳ 明朝・`wrapText`。
/// 上のセルは wrap のみ、下のセルは wrap + `shrinkToFit`。Excel の行送りは ＭＳ 明朝 9pt で 11.05pt（1.2273 倍）、注記の 1 行は 10 字 × 9pt = 90pt。
/// ibisOffice は上のセルの 9pt の行を 15.0pt 間隔で並べ、下のセルの注記は 11pt の幅（112pt）。許容は行送り 11.75pt まで、幅 100pt まで。
pub(crate) fn corpus_report_xlsx_rich_text_small_run_sets_line_pitch() {
    let pdf = export_pdf("rich-text-small-run-pitch.xlsx", "rich_text_small_run");
    let pages = pdf_pages(&pdf);
    let mut notes: Vec<&TextLine> = pages[0].iter().filter(|l| l.compact().starts_with("（ダミー注記") || l.compact().starts_with("ダミー注記")).collect();
    assert_eq!(notes.len(), 4, "注記の行（上のセルに 2 行、下のセルに 2 行）：{:?}", pages[0].iter().map(|l| l.compact()).collect::<Vec<_>>());
    notes.sort_by(|a, b| a.y0.total_cmp(&b.y0));
    let upper_pitch = notes[1].y0 - notes[0].y0;
    let lower_width = notes[3].x1 - notes[3].x0;
    assert!(upper_pitch <= 11.75, "上のセル（wrap）の 9pt の行送り {upper_pitch:.1}pt（Excel は 11.05pt）");
    assert!(lower_width <= 100.0, "下のセル（wrap + shrinkToFit）の注記の幅 {lower_width:.1}pt（9pt の run なら約 90pt）");
}

/// xlsx-C008-b1 結果が空文字列の数式セル（`<c t="str"><f>IF(…,…,"")</f><v></v></c>`）が、左隣の文字のはみ出しを止めない。
/// Excel は数式の入ったセルを空とみなさないので、左隣の折り返しなし・左揃えの長い文字は自分のセルの右端で切れる（定数の文字のセルと同じ）。
/// A 列・B 列は幅 14。1 行目は A1 に長い英文、B1 は結果が空文字列の数式。3 行目は B3 が定数の文字「x」（対照：A3 は A 列の右端で切れる）。
/// ibisOffice は 3 行目だけ切れ、1 行目は全文を右へはみ出させる（x が 525pt まで）。許容は A 列の右端（3 行目の描画の右端）から 7pt。
pub(crate) fn corpus_report_xlsx_empty_formula_result_stops_overflow() {
    let pdf = export_pdf("empty-formula-blocks-overflow.xlsx", "empty_formula_overflow");
    let pages = pdf_pages(&pdf);
    let right = |key: &str| pages[0].iter().find(|l| l.compact().starts_with(key)).unwrap_or_else(|| panic!("{key} の行")).x1;
    let (blocked_by_constant, blocked_by_formula) = (right("Row3"), right("Row1"));
    // A 列の右端は今の描画から求める：右隣に何も無い 2 行目は右へはみ出し、B3 が定数の 3 行目は A 列の右端で切れる。
    let overflowing = right("Row2");
    assert!(blocked_by_constant + 50.0 < overflowing, "対照：B3 が定数の A3 は A 列の右端で切れる（右端 {blocked_by_constant}pt、右隣の無い A2 の右端 {overflowing}pt）");
    assert!(blocked_by_formula <= blocked_by_constant + 7.0, "B1 が空文字列の数式の A1 は A 列の右端（A3 の右端 {blocked_by_constant}pt）で切れる（右端 {blocked_by_formula}pt）");
}

/// xlsx-C009-b2 印刷範囲（`_xlnm.Print_Area`）が `IF(…,OFFSET(…),OFFSET(…))` の式のとき、式を評価せず、使用範囲の全体を印刷する。
/// 100 行 × 3 列、E1=0、印刷範囲は `IF(Sheet1!$E$1=0,OFFSET(Sheet1!$A$1,0,0,10,3),OFFSET(Sheet1!$A$1,0,0,Sheet1!$E$1,3))`。
/// Excel は A1:C10 を 1 ページに印刷する（実文書の正解 PDF は式の評価結果の 20 行だけ）。ibisOffice は 100 行を複数ページに印刷する。
pub(crate) fn corpus_report_xlsx_print_area_formula_is_evaluated() {
    let p = read("print-area-formula.xlsx");
    let slide = &p.slides[0];
    let rows = sheet_of_slide(slide).unwrap().row_offsets();
    let area = crate::export::export_area(&p, slide);
    let titles = crate::export::print_titles(&p, slide);
    let (_, _, pages) = crate::export::sheet_pages(slide, area, titles, (0.0, 0.0)).expect("a worksheet");
    let ten_rows = (rows[10] - rows[0]) as f64 / EMU_PER_POINT as f64;
    assert!(area.height as f64 / EMU_PER_POINT as f64 <= ten_rows + 1.0, "印刷範囲の高さ {} pt（A1:C10 なら {ten_rows} pt）", area.height as f64 / EMU_PER_POINT as f64);
    assert_eq!(pages.len(), 1, "ページ数");
}

/// xlsx-C010-b1 非表示（VML の `style="…visibility:hidden…"`、`x:ClientData ObjectType="GBox"`）のフォーム コントロールのグループ ボックスを
/// 枠と文字つきで描き、同じ範囲のオプション ボタンを覆う。正解 PDF（実文書）ではグループ ボックスの枠も「グループ」の文字も出ない。
/// C3:F3 にオプション ボタン 4 つ（○1〜○4）、同じ範囲に `visibility:hidden` のグループ ボックス「Dummy group」。
/// ibisOffice は C3:F3 に枠と「Dummy group」を描く（`visibility:hidden` を外した文書と同じ描画）。
pub(crate) fn corpus_report_xlsx_hidden_group_box_not_drawn() {
    let pdf = export_pdf("hidden-group-box.xlsx", "hidden_group_box");
    let pages = pdf_pages(&pdf);
    let texts: Vec<String> = pages[0].iter().map(TextLine::compact).collect();
    assert!(texts.iter().any(|t| t == "ItemA"), "セルの文字は出る：{texts:?}");
    let drawn: Vec<&String> = texts.iter().filter(|t| t.contains("group")).collect();
    assert!(drawn.is_empty(), "非表示のグループ ボックスの文字が出る：{drawn:?}");
}

/// xlsx-C012-b1 既定の書体が 游ゴシック 11 のシートで、数字の幅を 7px 固定で数えるため、列が正解より約 7% 狭く、fitToPage の縮小率が約 7% 大きくなる。
/// 実文書の正解（游ゴシック 11、fitToPage）は ibisOffice の字が 1.07 倍大きい（7 文書のうち 5 文書で 1.07〜1.12）。列の幅を 1.0765 倍にすると字の大きさと列の位置が合う。
/// 列 25 本（狭い列と広い列が交互）、A4 縦、fitToPage の表。ibisOffice の縮小率は 0.776。正解の寸法なら約 0.72（0.776 ÷ 1.0765）。
/// 許容は 0.67〜0.74。
pub(crate) fn corpus_report_xlsx_yugothic_fit_to_page_scale() {
    let (pages, scale) = printed_pages("yugothic-fit-to-page.xlsx");
    assert_eq!(pages, 1, "ページ数");
    assert!((0.67..=0.74).contains(&scale), "縮小率 {scale:.3}（Excel は約 0.72）");
}

/// xlsx-C015-b1① 結合セルの境の二重線（`double`）を、細い二重線にせず太い一本線で描く。
/// 左は結合なしの上下 2 つのセル（境は二重線）、右は同じものを上下とも A:B の結合セル（Merged upper / Merged lower）にしたもの。
/// 正解は結合の有無によらず細い二重線（実文書 127 の正解）。400dpi で 2 つの文字の間の境を縦に見ると、
/// 結合なしは約 6 画素の線が 2 本（間に白）、ibisOffice の結合ありは 17 画素の 1 本。
pub(crate) fn corpus_report_xlsx_merged_double_border_stays_double() {
    let pdf = export_pdf("double-border-merged.xlsx", "double_border");
    let pages = pdf_pages(&pdf);
    let line = |key: &str| pages[0].iter().find(|l| l.compact() == key).unwrap_or_else(|| panic!("{key} の行"));
    let page = raster(&pdf, 1, 400);
    let bands = |upper: &str, lower: &str| {
        let (u, l) = (line(upper), line(lower));
        page.dark_runs_down((u.x0 + u.x1) / 2.0, u.y1 + 0.2, l.y0 - 0.2, 160).len()
    };
    assert_eq!(bands("Unmergedupper", "Unmergedlower"), 2, "結合なしの境は二重線（対照）");
    assert_eq!(bands("Mergedupper", "Mergedlower"), 2, "結合ありの境も二重線（太い一本線にならない）");
}

/// xlsx-C015-b1② 結合セルの辺の罫線の色を、結合範囲の途中から別の色（灰色）で描く。
/// A16:A29 を結合し、その範囲の一部のセルの左辺・下辺は灰色（C0C0C0）、右辺は自動（黒）。右辺は結合範囲の全体で黒のはずだが、
/// ibisOffice は途中から灰色（192）に変える。結合しない同じ書式（対照）は全部黒。
pub(crate) fn corpus_report_xlsx_merged_edge_keeps_each_cells_colour() {
    let pdf = export_pdf("merged-edge-colour.xlsx", "merged_edge_colour");
    let pages = pdf_pages(&pdf);
    let labels: Vec<&TextLine> = pages[0].iter().filter(|l| l.compact() == "Label").collect();
    assert_eq!(labels.len(), 3, "A2:A15・A16:A29・A30:A31 の文字");
    let page = raster(&pdf, 1, 200);
    let y = |l: &TextLine| (l.y0 + l.y1) / 2.0;
    let right_edge = (0..page.width).rev().find(|x| page.gray[page.px(y(labels[0])) * page.width + x] < 100).expect("右辺の縦線") as f32 / page.px_per_pt;
    let darkest = |l: &TextLine| (-1..=1).map(|d| page.gray_at(right_edge + d as f32 / page.px_per_pt, y(l))).min().unwrap();
    assert!(darkest(labels[0]) < 100, "A2:A15 の右辺は黒（対照）");
    assert!(darkest(labels[1]) < 100, "A16:A29 の右辺は黒（灰色 {} にならない）", darkest(labels[1]));
}

/// xlsx-C015-b2 小さい字（Arial 8）の垂直中央の文字が Excel より約 1pt 低く、行の下の罫線（medium）が文字の下端に付く。
/// 行の高さ 12pt（`customHeight` なし）の表で、「Row three」の行の下に medium の罫線。
/// 正解は字の下端（基線）から罫線の上端まで 2.0pt（Mac 版 Excel と実文書の正解 PDF が一致）、ibisOffice は 0.84pt。許容は 1.6pt 以上。
pub(crate) fn corpus_report_xlsx_centered_text_leaves_room_above_bottom_rule() {
    let pdf = export_pdf("wrap-center-baseline.xlsx", "wrap_center_baseline");
    let pages = pdf_pages(&pdf);
    let word = pages[0].iter().find(|l| l.compact() == "Rowthree").expect("Row three");
    let page = raster(&pdf, 1, 600);
    let (x0, x1) = (page.px(word.x0), page.px(word.x1));
    let full = |y: usize| page.gray[y * page.width + x0..y * page.width + x1].iter().filter(|v| **v < 160).count() * 100 / (x1 - x0) > 95;
    let any = |y: usize| page.gray[y * page.width + x0..y * page.width + x1].iter().any(|v| *v < 160);
    let rule_top = (page.px(word.y0)..page.px(word.y1 + 8.0)).find(|y| full(*y)).expect("下の罫線");
    let text_bottom = (page.px(word.y0)..rule_top).rev().find(|y| any(*y)).expect("字");
    let gap = (rule_top - text_bottom - 1) as f32 / page.px_per_pt;
    assert!(gap >= 1.6, "字の下端から罫線の上端まで {gap:.2}pt（Excel は 2.0pt）");
}

/// xlsx-C016-b3（todo-xlsx の H011 の続き）`slantDashDot` の罫線がほぼ実線に見える。
/// 正解は一周期 16.6pt に、線（約 8.4pt）・空き（約 2.6pt）・点（約 2.9pt）・空き（約 2.6pt）が並び、線の帯の中央の行で背景の部分が約 3 割。
/// 色 4472C4 の slantDashDot の下罫線の行（B〜N 列）。ibisOffice は斜めの細い隙間（約 1 画素）しか無く、背景の部分が 1% 未満（mediumDashDot の対照は約 3 割）。
/// 許容は線の部分が帯の長さの 85% まで（300dpi の帯の中央の 1 行）。
pub(crate) fn corpus_report_xlsx_slant_dash_dot_keeps_gaps() {
    let pdf = export_pdf("slant-dash-dot-border.xlsx", "slant_dash_dot");
    let page = raster(&pdf, 1, 300);
    let bands = page.rows_with_ink(200, 200);
    let (first, last) = *bands.first().expect("罫線の帯");
    let y = (first + last) / 2;
    let row = &page.gray[y * page.width..(y + 1) * page.width];
    let dark: Vec<usize> = row.iter().enumerate().filter(|(_, v)| **v < 200).map(|(x, _)| x).collect();
    let covered = dark.len() as f64 / (dark.last().unwrap() - dark.first().unwrap() + 1) as f64;
    assert!(covered <= 0.85, "帯の中央の行で線の部分が {:.0}%（正解は約 70%）", covered * 100.0);
}

/// xlsx-C016-b1（todo-xlsx の H011）`hair` の罫線を、`thin` と同じ太さの線で描く。
/// 正解（Windows 版 Excel の PDF、400dpi）の hair は 1 画素（約 0.18pt）の連続した実線で、同じ表の `thin`（5 画素、約 0.9pt）よりはっきり細い。
/// hair-border-thickness.xlsx：B〜N 列の下罫線がすべて hair の行が 6 本。ibisOffice は hair を thin に置き換えて 6 画素（約 1.1pt）の線で描く。
/// 許容は線の太さが 3 画素（約 0.54pt）まで。
pub(crate) fn corpus_report_xlsx_hair_border_is_thinner_than_thin() {
    let pdf = export_pdf("hair-border-thickness.xlsx", "hair_thickness");
    let page = raster(&pdf, 1, 400);
    let bands = page.rows_with_ink(1500, 200);
    assert_eq!(bands.len(), 6, "B〜N 列の hair の罫線は 6 本：{bands:?}");
    let thickest = bands.iter().map(|(a, b)| b - a + 1).max().unwrap();
    assert!(thickest <= 3, "hair の線の太さが {thickest} 画素（{:.2}pt）。正解は 1 画素（約 0.18pt）", thickest as f32 / page.px_per_pt);
}

/// How long (points) the longest unbroken dark vertical run is within a few pixels of the middle of the text `label`, over the height of its row (22pt around the text).
fn rule_through_text(pdf: &Path, label: &str) -> f32 {
    let pages = pdf_pages(pdf);
    let line = pages[0].iter().find(|l| l.compact() == label).unwrap_or_else(|| panic!("「{label}」の行"));
    let page = raster(pdf, 1, 300);
    let (cx, cy) = ((line.x0 + line.x1) / 2.0, (line.y0 + line.y1) / 2.0);
    let longest = (-4..=4).flat_map(|d| page.dark_runs_down(cx + d as f32 / page.px_per_pt, cy - 11.0, cy + 11.0, 128)).map(|(_, length)| length).max().unwrap_or(0);
    longest as f32 / page.px_per_pt
}

fn rules_through_headings(file: &str, label: &str) -> Vec<String> {
    let pdf = export_pdf(file, label);
    ["GroupA", "GroupB"].iter().map(|heading| (heading, rule_through_text(&pdf, heading))).filter(|(_, length)| *length > 15.0).map(|(heading, length)| format!("「{heading}」の中央を {length:.1}pt の縦線が通る")).collect()
}

/// xlsx-C017-b1 「選択範囲内で中央」（`centerContinuous`）の連なり（先頭の文字のセルと、右に続く空の同じ書式のセル）の内側の縦罫線を描き、見出しの文字を縦線が貫く。
/// 正解の PDF（実文書の表の見出し）は連なりを 1 つの枠にし、内側の線は無い（外側の左右の辺と上下の辺だけ）。
/// center-continuous-inner-border.xlsx：B2:C2 と D2:E2 が全辺 thin の centerContinuous（「Group A」「Group B」）。ibisOffice は B|C と D|E の境の縦線を引く（22pt の高さの行の全体）。
/// 許容は縦線が文字の高さ（行の約 2/3）まで。内側の辺を外した対照（-ctrl）は縦線が 5.5pt 以下。
pub(crate) fn corpus_report_xlsx_center_across_inner_border_not_drawn() {
    let wrong = rules_through_headings("center-continuous-inner-border.xlsx", "center_across_inner_border");
    assert!(wrong.is_empty(), "centerContinuous の連なりの内側の辺：{wrong:#?}");
}

/// How many horizontal rules (72pt or longer, 100dpi) page 1 of the file prints.
fn printed_rules(file: &str, label: &str) -> usize {
    let pdf = export_pdf(file, label);
    raster(&pdf, 1, 100).horizontal_rules(100, 200).len()
}

/// xlsx-C017-b2 `<printOptions gridLinesSet="0"/>`（`gridLines` の指定なし）で `showGridLines` が既定（表示）のシートで、正解は画面の枠線と同じ格子を印刷する。
/// ibisOffice は `gridLines` だけを読み、`gridLinesSet` を見ないので、枠線を印刷しない（文字だけのページになる）。
/// grid-lines-set-zero.xlsx：罫線の無いセルに文字（B2・D2 など 4 行）。実文書 の正解は、セルに罫線が 1 本も無い表に列 A〜K の格子を印刷する。
/// 許容は幅 72pt 以上の水平線が 3 本以上（`gridLines="1"` の対照は 11 本）。
pub(crate) fn corpus_report_xlsx_grid_lines_set_zero_prints_gridlines() {
    let rules = printed_rules("grid-lines-set-zero.xlsx", "grid_lines_set_zero");
    assert!(rules >= 3, "枠線の水平線が {rules} 本（`gridLinesSet=\"0\"` のシートは枠線を印刷する。`gridLines=\"1\"` の対照は 11 本）");
}

/// The right end (points) of the table's top rule and of the top rule of the merged cell B5:H5, as the two rules page 1 prints (the merged cell's is the first rule below the table's third rule).
fn merged_top_rule_ends(file: &str, label: &str) -> (f32, f32) {
    let pdf = export_pdf(file, label);
    let rules = raster(&pdf, 1, 100).horizontal_rules(100, 200);
    assert!(rules.len() >= 4, "表の罫線 3 本と結合セルの上辺：{rules:?}");
    let merged = rules.iter().find(|r| r.0 > rules[2].0 + 5.0).unwrap_or_else(|| panic!("結合セルの上辺：{rules:?}"));
    (rules[0].2, merged.2)
}

/// xlsx-C017-b3 結合セルの辺を描くとき、周りのセルに罫線が無いと、左上のセルの罫線で補って線が結合範囲の全幅に伸びる（表の下に余分な横線が出る）。
/// 正解は各セルの辺で決まり、罫線のあるセルの分しか引かない（実文書 の正解は表の幅だけ）。
/// merged-anchor-border.xlsx：B2:D3 の表（全辺 thin）の下に B5:H5 の結合セル。上辺 thin は B5・C5・D5 にだけ（E5〜H5 には無い）。
/// ibisOffice は上辺を B〜H（右端 507.6pt）に引く。正しくは B〜D（表の右端 255.6pt）まで。許容は表の右端から 5pt。
pub(crate) fn corpus_report_xlsx_merged_top_border_not_extended_from_the_anchor() {
    let (table, merged) = merged_top_rule_ends("merged-anchor-border.xlsx", "merged_anchor_border");
    assert!(merged <= table + 5.0, "結合セルの上辺が右端 {merged:.1}pt まで伸びる（表の右端 {table:.1}pt。罫線のあるセルは B〜D だけ）");
}

/// How far each "ItalicAbc…" line's first word leans: the x of the centre of its ink in the upper half minus that of the lower half (points), by line.
fn italic_leans(pdf: &Path) -> Vec<f32> {
    let pages = pdf_pages(pdf);
    let page = raster(pdf, 1, 400);
    pages[0]
        .iter()
        .filter(|l| l.compact().starts_with("ItalicAbc"))
        .map(|l| {
            let (x0, x1) = (l.words[0].0 - 0.5, l.words[0].1 + 0.5);
            let mid = (l.y0 + l.y1) / 2.0;
            page.centroid_x((x0, l.y0, x1, mid)) - page.centroid_x((x0, mid, x1, l.y1))
        })
        .collect()
}

/// 悪化の検知（緑）xlsx-C018-b1（最新の main で直った。以前の症状：）`<i/>`（斜体）を、ＭＳ 系・Calibri＋scheme(minor)（テーマの和文書体は ＭＳ Ｐゴシック）の文字に効かせない（ヒラギノなどで描く和文系の書体には斜体の字面が無く、疑似斜体もかけないので立った字になる）。
/// 正解は ＭＳ 系を `Tm` のせん断 0.3333 で斜めにし、Calibri は Calibri-Italic で描く。
/// italic-ja-font.xlsx：4 行（ＭＳ Ｐゴシック・ＭＳ 明朝・Calibri＋scheme・Arial）すべて `<i/>`、-ctrl は `<i/>` を全部外したもの。
/// 先頭の語（Italic）の字の上半分の重心を下半分の重心と比べた右への傾きが、`<i/>` を外した対照より 0.4pt 以上大きい（実の斜体の Arial の行は約 1pt）。
pub(crate) fn corpus_report_xlsx_guard_italic_ja_font_is_slanted() {
    let italic = italic_leans(&export_pdf("italic-ja-font.xlsx", "italic_ja_font"));
    let upright = italic_leans(&export_pdf("italic-ja-font-ctrl.xlsx", "italic_ja_font_ctrl"));
    assert_eq!((italic.len(), upright.len()), (4, 4), "4 行");
    let rows = ["ＭＳ Ｐゴシック", "ＭＳ 明朝", "Calibri＋scheme(minor)", "Arial（実の斜体の書体、対照）"];
    let flat: Vec<String> = (0..4).filter(|i| italic[*i] - upright[*i] < 0.4).map(|i| format!("{}：傾き {:.2}pt（`<i/>` を外した対照 {:.2}pt）", rows[i], italic[i], upright[i])).collect();
    assert!(flat.is_empty(), "斜体が効かない行：{flat:#?}");
}

/// The compact text of each line of page 1 of the PDF.
fn first_page_texts(pdf: &Path) -> Vec<String> {
    pdf_pages(pdf)[0].iter().map(TextLine::compact).collect()
}

/// xlsx-C019-b1 「オブジェクトを印刷する」を外した図形（`xdr:clientData fPrintsWithSheet="0"`）を描く。正解の PDF（実文書）にはその図形が 1 つも出ない。
/// no-print-shape.xlsx：セルの右に薄黄色の四角形（赤い文字「Dummy label: not printed」）、`fPrintsWithSheet="0"`。ibisOffice は四角形の文字を描く（`shape_prints` を読むのは編集画面と保存だけ）。
/// 対照（-ctrl）は `fPrintsWithSheet="0"` を外しただけで、同じ描画（要素を無視している）。
pub(crate) fn corpus_report_xlsx_no_print_shape_not_printed() {
    let texts = first_page_texts(&export_pdf("no-print-shape.xlsx", "no_print_shape"));
    assert!(texts.iter().any(|t| t == "Row1"), "セルの文字は出る：{texts:?}");
    let drawn: Vec<&String> = texts.iter().filter(|t| t.contains("notprinted")).collect();
    assert!(drawn.is_empty(), "印刷しない図形の文字が出る：{drawn:?}");
}

/// xlsx-C019-b2 高さ 0 の位置に置かれたグラフ（`twoCellAnchor` の from と to が同じ行・同じオフセット）を、ページ幅の水平の細い黒線（潰れたグラフの枠）として描く。
/// 正解の PDF（実文書）は何も描かない。zero-height-chart.xlsx：C3:G3 の位置（from row 2、to row 2）の棒グラフ。罫線・枠線の無いシートなので、水平線が 1 本でもあればグラフの枠。
/// ibisOffice は C〜G の幅（177.8〜491.8pt）の水平線を 1 本描く。
pub(crate) fn corpus_report_xlsx_zero_height_chart_draws_nothing() {
    let pdf = export_pdf("zero-height-chart.xlsx", "zero_height_chart");
    let rules = raster(&pdf, 1, 100).horizontal_rules(100, 200);
    assert!(rules.is_empty(), "高さ 0 のグラフの水平線：{rules:?}（y・左端・右端 pt）");
}

/// How many lines of page 1 start with `prefix`.
fn lines_starting(pdf: &Path, prefix: &str) -> usize {
    first_page_texts(pdf).iter().filter(|t| t.starts_with(prefix)).count()
}

/// xlsx-C019-b3 フォーム コントロールのリスト ボックス（`objectType="List"`、`fmlaRange="Data!$B$5:$B$20"`）の項目が、項目のあるシートが非表示（`state="veryHidden"`）だと空になる。
/// 正解の PDF（実文書）は枠の中に項目を並べる（非表示のシートのセルも参照できる）。
/// list-box-hidden-sheet.xlsx：Sheet1 に「Item 01〜20」のセルとリスト ボックス、項目は veryHidden の Data シートの「Name 05〜20」。ibisOffice は空の枠だけ。
/// 許容は枠の中に「Name」の行が 4 つ以上（Data を表示にした対照は 8 行）。
pub(crate) fn corpus_report_xlsx_list_box_items_from_hidden_sheet() {
    let names = lines_starting(&export_pdf("list-box-hidden-sheet.xlsx", "list_box_hidden"), "Name");
    assert!(names >= 4, "リスト ボックスの項目の行が {names} 本（Data シートが非表示でも項目が出る。対照は 8 行）");
}

fn text_box_lines(file: &str, label: &str) -> Vec<String> {
    first_page_texts(&export_pdf(file, label))
}

/// 悪化の検知（緑）xlsx-C020-b1（最新の main で直った。以前の症状：）輪郭線の無い文字枠（`xdr:sp` の `txBox="1"`、`spPr/a:ln` が `<a:noFill/>`）の文字が、PDF の書き出しで用紙の左余白（と水平中央寄せの分）だけ左にずれ、印刷範囲の左端で切れる。
/// 正解は枠の中に収まる（実文書）。輪郭線を塗りと同じ色の実線にしただけの対照は、文字が枠の中に 4 行で収まる。
/// no-outline-text-box-shift.xlsx：左余白 0.2in・水平中央寄せのシートに、大かっこと塗りのある文字枠（「（留意事項）」の見出しと 3 つの文）。
/// ibisOffice は各行の先頭十数字が切れ、見出し「（留意事項）」と行頭の「・」が出ない（先頭の行は「い一文がここに入ります…」から）。
pub(crate) fn corpus_report_xlsx_guard_text_box_without_outline_keeps_its_text_position() {
    let texts = text_box_lines("no-outline-text-box-shift.xlsx", "no_outline_text_box");
    assert!(texts.iter().any(|t| t == "（留意事項）"), "枠の見出し「（留意事項）」が出る（左へずれて切れる）：{texts:?}");
    assert!(texts.iter().any(|t| t.starts_with("・ダミーの長い一文")), "行頭の「・」から出る：{texts:?}");
}

/// The share of the ground under the text line `key` that is white (lighter than 250), 0 to 1.
fn white_under_text(file: &str, label: &str, key: &str) -> f32 {
    let pdf = export_pdf(file, label);
    let pages = pdf_pages(&pdf);
    let line = pages[0].iter().find(|l| l.compact().starts_with(key)).unwrap_or_else(|| panic!("{key} の行"));
    let page = raster(&pdf, 1, 200);
    let (w, h) = (page.px(line.x1) - page.px(line.x0), page.px(line.y1) - page.px(line.y0));
    let dark = page.ink((line.x0, line.y0, line.x1, line.y1), 250);
    1.0 - dark as f32 / (w * h) as f32
}

/// xlsx-C020-b2（確度は中。根拠は 1 文書の正解 PDF だけ）印刷範囲の外の列のセルから、灰色の塗りのある空のセルへはみ出す薄い灰色の文字が、塗りに埋もれて見えない。
/// 正解の PDF（実文書）では、はみ出した文字の幅の分は白い地になり、文字が見える（Excel for the web の見え方の可能性がある）。
/// overflow-from-outside-print-area.xlsx：A1 が灰色の文字（A1〜H1 は文字と同じ灰色の塗り）、印刷範囲 B1:H10。文字の行の背景は、ibisOffice は塗りの灰色のまま（白は 5%）。
/// 塗りを外した対照は 74% が白。許容は文字の行の背景の 30% 以上が白。
pub(crate) fn corpus_report_xlsx_overflow_text_from_outside_print_area_has_white_ground() {
    let white = white_under_text("overflow-from-outside-print-area.xlsx", "overflow_outside_area", "Dummyoverflow");
    assert!(white >= 0.3, "はみ出した文字の行の背景の白 {:.0}%（塗りに埋もれて文字が見えない。塗りの無い対照は 74%）", white * 100.0);
}

/// xlsx-C020-b3 セルの文字の箇条書き記号（`•`）の直後のタブ文字を、タブ位置まで広げる（約 1cm 空く）。正解の PDF（実文書）は詰めて「•Dummy …」と続ける。
/// tab-in-cell-text.xlsx：折り返し指定のセルに「•⇥Dummy second item.」などの 4 段落。ibisOffice は「•」と本文が別の行（28pt 離れる）に出る。
/// タブを外した対照は「•Dummy second item.」が 1 つの行。許容は記号と本文が 1 つの行にあること（pdftotext は広い隙間で行を分ける）。
pub(crate) fn corpus_report_xlsx_tab_in_cell_text_takes_no_width() {
    let texts = first_page_texts(&export_pdf("tab-in-cell-text.xlsx", "tab_in_cell"));
    assert!(texts.iter().any(|t| t == "•Dummyseconditem."), "「•」と本文が詰まって 1 つの行になる（タブの幅が空く）：{texts:?}");
}

/// The characters of 「あいうえおかきくけこ」 that page 1 of the file prints.
fn printed_vertical_chars(file: &str, label: &str) -> String {
    let all = first_page_texts(&export_pdf(file, label)).concat();
    "あいうえおかきくけこ".chars().filter(|c| all.contains(*c)).collect()
}

/// xlsx-C021-b2 縦書き（`textRotation="255"`）＋折り返し（`wrapText="1"`）のセルで、セルの高さに入らない文字を次の列に回さず、1 列のまま上下に切る。
/// 正解の PDF（実文書）は高さに入らない分を左隣の列に回し、2 列に並べる（文字は切れない）。
/// vertical-text-wrap-columns.xlsx：結合セル A2:A4（54pt）に 10 字「あいうえおかきくけこ」。ibisOffice は 1 列に入る 5 字（「うえおかき」）だけを出し、残りを描かない。
/// 許容は 10 字が全部出ること（行の高さを 60pt にして 1 列に収めた対照は 10 字が出る）。
pub(crate) fn corpus_report_xlsx_vertical_text_wraps_into_columns() {
    let printed = printed_vertical_chars("vertical-text-wrap-columns.xlsx", "vertical_text_wrap");
    assert_eq!(printed.chars().count(), 10, "縦書きの 10 字のうち出る字：「{printed}」（残りは切れて出ない）");
}

/// The shade (0 black to 255 white) at the point (`x_pt`, the middle of the text line `key`) of page 1 of the file.
fn shade_right_of(file: &str, label: &str, key: &str, x_pt: f32) -> u8 {
    let pdf = export_pdf(file, label);
    let pages = pdf_pages(&pdf);
    let line = pages[0].iter().find(|l| l.compact().starts_with(key)).unwrap_or_else(|| panic!("{key} の行"));
    raster(&pdf, 1, 100).gray_at(x_pt, line.y0 - 3.0)
}

/// xlsx-C022-b1 行の書式（`<row s="1" customFormat="1">`）を読まず、行の中でセルの無い列に塗りと罫線を引かない。見出しの帯が先頭のセルの幅で止まる。
/// 正解の PDF（実文書）は、行の書式の塗り・下線を表の右端まで通しで引く。
/// row-default-style.xlsx：1 行目（`ht=26.5`）に灰色の塗りの行の書式、A1 だけに文字。A〜E 列（右端 450pt）の帯が正しい。
/// ibisOffice は A 列（約 145pt）だけ塗る（400pt の位置は白、255）。対照（B1〜E1 に同じ塗りの空のセルを足したもの）は 207 の灰色。許容は 240 以下。
pub(crate) fn corpus_report_xlsx_row_style_fills_the_whole_row() {
    let shade = shade_right_of("row-default-style.xlsx", "row_style", "Dummy", 400.0);
    assert!(shade <= 240, "1 行目の B〜E 列（x=400pt）の塗り {shade}（行の書式の灰色 207 にならず白）");
}

/// xlsx-C023-b1（X7a の残り。多くの束がこのキーに寄せた）列の幅（`<col width>` の文字数）の 1 文字（既定の書体の数字の幅）を、実の数字の幅（約 7.4〜7.5px）より狭く数えるので、列が正解より狭い。
/// 正解の PDF（実文書、Calibri 11）は 1 文字約 7.5px で、表の幅が 499.7pt。以前の ibisOffice は 7px 固定で 466.9pt（比 1.070）、最新の main でも 487.8pt（比 1.024）。
/// fractional-digit-width.xlsx：Calibri 11、`<cols>` の width が 10.78・27.44・9・8.44・9.33・8・15.66 の 7 列、24pt 固定の 6 行、拡大縮小なし（右端の位置は用紙で決まるので、列の幅だけを見る）。
/// A〜C 列の幅（A の左端から C の右端の縦罫線まで）は、正解で 265.9pt（実文書の A 10.78・B 27.44・C 9 文字の列の縦罫線 50.76・111.24・265.86・316.62pt）。最新の main は 259.2pt。許容は 263pt 以上（正解から 1% まで）。
pub(crate) fn corpus_report_xlsx_column_width_digit_is_fractional() {
    let pdf = export_pdf("fractional-digit-width.xlsx", "fractional_digit_width");
    let rules = raster(&pdf, 1, 200).vertical_rules(100.0, 160);
    assert!(rules.len() >= 4, "表の縦罫線（左右の端と列の境）：{rules:?}");
    let span = rules[3] - rules[0];
    assert!(span >= 263.0, "A〜C 列の幅 {span:.1}pt（正解は 265.9pt。縦罫線 {rules:?}）");
}

/// The numbers among 1〜130 that the file's PDF does not print as a number (a line that is the number, or the number and the dummy text of the next column, without spaces).
fn missing_row_numbers(file: &str, label: &str) -> Vec<u32> {
    let pdf = export_pdf(file, label);
    let lines: Vec<String> = pdf_pages(&pdf).iter().flatten().map(TextLine::compact).collect();
    (1..=130u32).filter(|n| !lines.iter().any(|t| *t == n.to_string() || *t == format!("{n}ダミー"))).collect()
}

/// 統合の記録の「新ビルドの後退」（実文書の行番号の列）行番号の列が狭く、3 桁の番号（100 以上）が「##」になる。
/// 後退の疑い（92341f7c1 の描画では ## にならなかった。原因の見当は列の数字の幅を GDI で測り直した 3b99a30ec・0e8e5546e・2350b92c0 で、数字が入る幅の余白の判定）。
/// 正解の PDF（実文書、ＭＳ Ｐゴシック 11 の既定の書体）は幅 3.625 の A 列に 3 桁の番号を右揃えで出す（100〜122 が読める）。
/// row-number-column-hash.xlsx：既定の書体 ＭＳ Ｐゴシック 11、A 列（幅 3.625、右揃え、罫線つき）に 1〜130 の番号、B 列（幅 20.625）に「ダミー」、行の高さ 18pt。
/// ibisOffice は 100〜130 を「##」にする（実文書 の ibisOffice の PDF は 31 行が「##」）。許容は 1〜130 の番号が全部数字で出ること。
pub(crate) fn corpus_report_xlsx_row_number_column_does_not_overflow_into_hashes() {
    let missing = missing_row_numbers("row-number-column-hash.xlsx", "row_number_hash");
    assert!(missing.is_empty(), "数字で出ない番号 {} 個（先頭 {:?}）。3 桁の番号が「##」になる", missing.len(), &missing[..missing.len().min(8)]);
}

/// xlsx-C004-b2（最新の main で直ったとして外した試験を戻した。直っていなかったので赤のまま）`customHeight` の行の高さを、正解と違う比で並べるので、1 ページに載る行が正解と合わない。
/// custom-row-heights-page-capacity.xlsx：49 行（全行 `customHeight`、5.25〜21.75pt）、A4 縦、`scale=95`、上余白 0.32in・下余白 0.77in。
/// 実文書の正解 PDF では 1 ページ目に 46 行、2 ページ目に 3 行（正解の行は `ht` × 拡大縮小の約 0.96 倍。xlsx-C026 の実測：全行 customHeight の実文書で 0.96）。
/// 623815f40 より前は 44 行目で打ち切り（行が正解より高い）。最新の main は 49 行全部が 1 ページ目に載る（行が `ht` × 拡大縮小の 0.907 倍と低く、実文書 の y の傾き
/// 「正解 = 1.062 × ibisOffice」＝行が正解より 6% 低い）。許容は 1 ページ目の最後の行が 45〜47 行目（正解は 46 行目）。
pub(crate) fn corpus_report_xlsx_custom_row_heights_fill_the_page_like_excel() {
    let pdf = export_pdf("custom-row-heights-page-capacity.xlsx", "row_heights_capacity");
    let pages = pdf_pages(&pdf);
    let has = |page: usize, row: u32| pages.get(page).is_some_and(|p| p.iter().any(|l| l.compact().contains(&format!("ダミー行{row:02}"))));
    let last = (1..=49u32).filter(|r| has(0, *r)).max().expect("1 ページ目の行");
    assert!((45..=47).contains(&last), "1 ページ目の最後の行は {last} 行目（正解は 46 行目。2 ページ目の行：{:?}）", (1..=49u32).filter(|r| has(1, *r)).collect::<Vec<_>>());
}

/// 悪化の検知（緑）xlsx-C003-b3（最新の main で直って外した試験を戻した）一部の run だけ文字が小さいリッチテキストを下揃えの結合セルに入れると、
/// 先頭の行が結合範囲の上端の外にはみ出して切れる。正解の 4 行は約 39.7pt で範囲に収まる。
/// 結合セル B1:F2（行の高さ 18 + 24.75 = 42.75pt）に BIZ UDゴシック 10pt の 2 行と 8pt の run（2 行に折り返す）。先頭の行の上端が範囲の上端（36pt）以上。
pub(crate) fn corpus_report_xlsx_guard_merged_rich_text_fits_the_cell() {
    let pages = pdf_pages(&export_pdf("merged-rich-text-run-sizes.xlsx", "merged_rich_text"));
    assert!(!pages[0].is_empty(), "結合セルの文字が出る");
    let top = pages[0].iter().map(|l| l.y0).fold(f32::MAX, f32::min);
    assert!(top >= 35.5, "先頭の行の上端 {top}pt（結合範囲の上端は 36pt）");
}

/// 悪化の検知（緑）xlsx-C014-b3（X5f の延長。最新の main で直って外した試験を戻した）`customHeight` の無い `defaultRowHeight`（Mac の Excel が既定の書体 游ゴシック 11 から書いた 19.8）を、
/// 行の中の書体（ここは ＭＳ 明朝 12）を見ずに高さの指定が無い行にそのまま使う。正解の行は約 15.2pt（60 行 × 15.2 × 0.7 ≒ 638pt で 1 ページ）。
/// default-row-height-no-custom-height.xlsx：既定の書体 游ゴシック 11、`defaultRowHeight="19.8"`、ＭＳ 明朝 12 の 1 行の文字を `ht` 無しで 60 行、A4 縦、拡大縮小 70%。
pub(crate) fn corpus_report_xlsx_guard_default_row_height_follows_row_font() {
    let (pages, scale) = printed_pages("default-row-height-no-custom-height.xlsx");
    assert_eq!(pages, 1, "ページ数（拡大縮小 {}%）", scale * 100.0);
}

/// 悪化の検知（緑）xlsx-C007-b4（X7a の現れ。最小文書は直った）列の幅の差で折り返しの位置・行数が正解と変わる。
/// wrap-column-width-chars.xlsx：A1:F1 を結合（幅 10.7109375 × 6）、ＭＳ 明朝 10pt の全角 102 字。Excel の式（1 列 56.25pt）では 6 列 337.5pt で 1 行 33 字、4 行（x 53.5〜385pt）。
/// 以前の ibisOffice は 1 行 37〜38 字で 3 行（x 〜423pt）。許容は 4 行以上、どの行の右端も 395pt まで。
pub(crate) fn corpus_report_xlsx_guard_wrap_follows_column_width_chars() {
    let pages = pdf_pages(&export_pdf("wrap-column-width-chars.xlsx", "wrap_column_width"));
    let lines = &pages[0];
    let right = lines.iter().map(|l| l.x1).fold(0.0, f32::max);
    assert!(lines.len() >= 4 && right <= 395.0, "折り返した行 {} 本、右端 {right:.1}pt（正解は 4 行、右端 385pt まで）", lines.len());
}

/// 悪化の検知（緑）xlsx-C009-b1（X2d と同根。最小文書は直った）「選択範囲内で中央」の文字列が、選択範囲（H〜J）ではなく先頭の 1 セルの中央に置かれる。
/// center-continuous-text-across-range.xlsx：H6 に「ダミー見出」、I6・J6 は空の同じ書式（centerContinuous）、H7〜J7 は「甲」「乙」「丙」。
/// 正しくは H〜J の中央（「乙」の中心 198.5pt）。以前の ibisOffice は先頭の列の中央（「甲」の中心 167.5pt）。許容は「乙」の中心から 4pt。
pub(crate) fn corpus_report_xlsx_guard_center_across_text_is_centred_on_the_range() {
    let pages = pdf_pages(&export_pdf("center-continuous-text-across-range.xlsx", "center_across_text"));
    let centre = |key: &str| {
        let l = pages[0].iter().find(|l| l.compact() == key).unwrap_or_else(|| panic!("「{key}」の行"));
        (l.x0 + l.x1) / 2.0
    };
    let (heading, middle) = (centre("ダミー見出"), centre("乙"));
    assert!((heading - middle).abs() <= 4.0, "見出しの中心 {heading:.1}pt（H〜J の中央は「乙」の中心 {middle:.1}pt）");
}
