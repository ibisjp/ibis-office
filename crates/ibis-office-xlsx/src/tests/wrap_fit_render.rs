// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Fitting a row to its wrapped text (行の高さの自動調整) breaks the text where the cell's drawing breaks it: the lines the
//! drawing shows (`shown_texts`) times the line pitch the drawing uses is the height the row is fitted to.

use super::SheetSteps;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{EMU_PER_POINT, sheet_calc::sheet_of_slide};
use crate::ext::*;

/// The lines the drawing shows for the cell at (row, column), 0-based.
fn drawn_lines(s: &mut Scenario, row: u32, column: u32) -> Vec<String> {
    let out = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = out.clone();
    s.update(move |e, w, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        *sink.borrow_mut() = crate::sheet_view::shown_texts(&sheet, &e.resources, w)
            .into_iter()
            .find(|(r, c, _)| *r == row && *c == column)
            .map(|(_, _, t)| t.split('\n').map(str::to_string).collect())
            .unwrap_or_default();
    });
    let lines = out.borrow().clone();
    lines
}

/// The height the drawing's lines need: its line count times Excel's row of the font (`row_fit::line_pt`: Yu Gothic 11 is 18.75 pt, which is
/// not the pitch the drawing sets its lines at, `line_pitch`: that difference is open), in points.
fn drawn_height(s: &mut Scenario, row: u32, column: u32) -> (usize, f32) {
    let lines = drawn_lines(s, row, column).len();
    let line = s.with(move |e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        let style = &sheet.styles[sheet.cell(row, column).unwrap().style as usize];
        let normal = crate::row_fit::normal_font(e.presentation());
        crate::row_fit::line_pt(&e.resources, &normal, style.font.as_deref(), style.font_size)
    });
    (lines, lines as f32 * line.unwrap_or(f32::NAN))
}

pub(super) fn issue_f11() {
    let mut s = Scenario::open("test/editing/xlsx-rowfit-issue-triage.xlsx");
    s.size(1400., 900.);
    let before = s.size_pt("11");
    let (lines, need) = drawn_height(&mut s, 10, 5);
    eprintln!("F11: row {before} pt, drawn {lines} lines, need {need} pt");
    s.screenshot("f11-before");
    s.update(|e, w, cx| e.fit_rows(10, w, cx));
    let after = s.size_pt("11");
    let (lines2, need2) = drawn_height(&mut s, 10, 5);
    eprintln!("F11 fitted: {after} pt, drawn {lines2} lines, need {need2} pt");
    s.screenshot("f11-after");
    // Yu Gothic is measured on Windows only (`row_fit::row_pixels`): elsewhere the row is the file's reference value scaled, not Excel's.
    if need2.is_nan() {
        eprintln!("issue_f11: the typeface cannot be measured here, checked on Windows");
        return;
    }
    assert!((after - need2).abs() < 0.3, "fitted {after} pt, the drawing needs {need2} pt ({lines2} lines)");
    let _ = EMU_PER_POINT;
}

/// The row's height by the lines the drawing shows, `line` points each (the sheet's row height for the font's size).
fn assert_fitted(s: &mut Scenario, cell: &str, row: u32, column: u32, what: &str) {
    s.update(move |e, w, cx| e.fit_rows(row, w, cx));
    let lines = drawn_lines(s, row, column);
    let (height, line) = s.with(move |e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        let style = &sheet.styles[sheet.cell(row, column).unwrap().style as usize];
        let default = e.default_font_size_pt();
        let base = if sheet.default_row > 0 { sheet.default_row as f32 / EMU_PER_POINT as f32 } else { 18.0 };
        (sheet.rows[row as usize] as f32 / EMU_PER_POINT as f32, base * (style.font_size as f32 / 100.0) / default)
    });
    eprintln!("{what}: {cell} drawn {} lines, row {height} pt (line {line} pt)", lines.len());
    assert!((height - lines.len() as f32 * line).abs() < 0.3, "{what}: {cell} has {} drawn lines of {line} pt, the row is {height} pt: {lines:?}", lines.len());
}

/// Texts of every kind, in columns of many widths, fitted: the row is as tall as the drawing's lines.
pub(super) fn fitted_height_is_the_drawn_lines() {
    let texts: [(&str, &str); 6] = [
        ("japanese", "日本語の長い文章です。句読点、「かぎ括弧」（丸括弧）を含み、禁則処理で行頭に来てはいけない文字が行の終わりに来る場合を確かめます。"), // i18n-ok (Japanese text under test or a name table that matches labels)
        ("latin", "The quick brown fox jumps over the lazy dog and keeps running through the long long field until the night comes"),
        ("mixed", "https://github.com/ibisjp/ibis-office/issues/54#issuecomment-6503496111 test.docxを対応する。また、全角記号をすべて洗い出したテストファイルを作る。"), // i18n-ok (Japanese text under test or a name table that matches labels)
        ("longword", "supercalifragilisticexpialidocious_supercalifragilisticexpialidocious_and then short words"),
        ("newlines", "一行目の少し長めの文章がここに入ります\n\n三行目 short\n四行目はとても長い文章で列の幅を超えて折り返される予定のものです"), // i18n-ok (Japanese text under test or a name table that matches labels)
        ("crlf", "line one is rather long and will wrap\r\nsecond line also is long enough to wrap at some widths\r\nthird"),
    ];
    for file in ["test/editing/xlsx-rowfit-issue-triage.xlsx", "test/editing/xlsx-sales.xlsx"] {
        let mut s = Scenario::open(file);
        s.size(1400., 900.);
        let sales = file.contains("sales");
        if sales {
            s.menu("xlsx.insert-sheet");
        }
        let (cell, column, row) = if sales { ("B2", 1u32, 1u32) } else { ("F11", 5, 10) };
        for (name, text) in texts {
            for width in [12.0f32, 20.0, 33.0, 50.0, 77.0] {
                s.update(move |e, _, cx| {
                    e.select_lines(ibis_doc_model::sheet_command::Axis::Columns, column, column, cx);
                    e.set_selected_column_width(width, cx);
                });
                s.click_cell(cell);
                if sales {
                    s.update(|e, _, cx| e.format_cells(ibis_doc_model::StyleChange::Wrap(true), cx));
                }
                s.enter(text, "enter");
                assert_fitted(&mut s, cell, row, column, &format!("{file} {name} width {width}"));
            }
            s.screenshot(&format!("{}-{name}", if sales { "sales" } else { "f11" }));
        }
    }
}
