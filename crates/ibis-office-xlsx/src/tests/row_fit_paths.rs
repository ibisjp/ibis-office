// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `xlsx::row_fit_paths_agree`: how high a row is for its content (行の高さの自動調整) is said in one place,
//! `xlsx::row_fit::fitted_height_pt` (the lines are the drawing's own: `sheet_view::drawn_line_counts`, one line is Excel's row of
//! the font), and every way a row gets its height agrees with it and with what is drawn: opening a file, the autofit command
//! (the header's menu, the double click on the row's line), an entry, a paste, a column's width, wrapping and the font's size.
//! In each, the row is `lines x one line`, the drawing shows every one of those lines (none cut off), and the cell's editing box
//! (F2) is not higher than the row. A row sized by hand (`customHeight`) is left alone by all but the autofit command.
//!
//! Microsoft Excel 365 for Windows (COM, 2026-11-09, `tasks/doc/issue-triage/unlabeled-issues-20261107.xlsx`): a row with no
//! `customHeight` is measured again each time the file opens, wrapped text too, whatever `ht` says (`ht="90"` and `ht="20"` both
//! open at 112.5 pt = 6 lines x 18.75 pt); the standard height is the first font's row (Yu Gothic 11: 18.75 pt, not the file's
//! `defaultRowHeight="15"`); a `customHeight` row stays until the autofit command, which gives it the measured height.

use super::SheetSteps;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{EMU_PER_POINT, StyleChange, sheet_calc::sheet_of_slide};
use crate::ext::*;

const ISSUES: &str = "test/editing/xlsx-rowfit-issue-triage.xlsx";

/// Rows 1 to 40 of `ISSUES` as Excel opens them (points).
const EXCEL_ROWS: [f32; 40] = [
    24.0, 37.5, 112.5, 94.25, 90.0, 36.0, 72.0, 93.75, 75.0, 18.75, 90.0, 150.0, 18.75, 18.0, 18.75, 75.0, 56.25, 56.25, 56.25, 18.75, 37.5, 37.5, 37.5, 18.75, 37.5, 56.25, 37.5, 56.25,
    93.75, 37.5, 112.5, 54.0, 56.25, 72.0, 18.0, 18.0, 18.0, 90.0, 60.0, 60.0,
];

fn row_pt(s: &mut Scenario, row: u32) -> f32 {
    s.with(move |e, _| sheet_of_slide(e.slide()).unwrap().rows[row as usize] as f32 / EMU_PER_POINT as f32)
}

fn custom(s: &mut Scenario, row: u32) -> bool {
    s.with(move |e, _| sheet_of_slide(e.slide()).unwrap().custom_rows.contains(&row))
}

/// What the row (0-based) of the cell at `column` has, drawn: (the row's height, the lines the drawing breaks the text into, one
/// line, the lines the drawing shows on the sheet as it is).
fn state(s: &mut Scenario, row: u32, column: u32) -> (f32, usize, f32, usize) {
    let out = std::rc::Rc::new(std::cell::RefCell::new((0.0, 0, 0.0, 0)));
    let sink = out.clone();
    s.update(move |e, w, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        let normal = crate::row_fit::normal_font(e.presentation());
        let lines = crate::sheet_view::drawn_line_counts(&sheet, &e.resources, row, row).get(&(row, column)).copied().unwrap_or(0);
        let style = &sheet.styles[sheet.cell(row, column).unwrap().style as usize];
        let line = crate::row_fit::line_pt(&e.resources, &normal, style.font.as_deref(), style.font_size).expect("the typeface can be measured");
        let shown = crate::sheet_view::shown_texts(&sheet, &e.resources, w).into_iter().find(|(r, c, _)| *r == row && *c == column).map_or(0, |(_, _, t)| t.split('\n').count());
        *sink.borrow_mut() = (sheet.rows[row as usize] as f32 / EMU_PER_POINT as f32, lines, line, shown);
    });
    let v = *out.borrow();
    v
}

/// The row is as high as its lines, the drawing shows all of them, and the editing box (F2) is not higher than the row.
fn agree(s: &mut Scenario, cell: &str, row: u32, column: u32, what: &str) {
    let (height, lines, line, shown) = state(s, row, column);
    eprintln!("{what}: {cell} row {height} pt, {lines} drawn lines x {line} pt, {shown} shown");
    assert!((height - lines as f32 * line).abs() < 0.3, "{what}: {cell}: the row is {height} pt, the drawing breaks the text into {lines} lines of {line} pt");
    assert_eq!(shown, lines, "{what}: {cell}: the drawing shows {shown} of {lines} lines (the row is {height} pt)");
    s.click_cell(cell);
    s.key("f2");
    s.screenshot(&format!("edit-{what}").replace(|c: char| !c.is_ascii_alphanumeric() && c != '-', "-"));
    let (boxed, edit_lines) = s.with(|e, _| {
        let c = e.cell_edit().expect("editing");
        let l = c.layout.borrow().clone().expect("the box is laid out");
        (l.height / c.pixels_per_point.get().max(0.01), l.lines.len())
    });
    s.key("escape");
    assert_eq!(edit_lines, lines, "{what}: {cell}: the editing box has {edit_lines} lines, the drawing {lines}");
    // The box is the drawing's block (lines at the drawing's pitch) and the cell's padding, 4 pt: where the pitch is the row's line (Yu Gothic,
    // ＭＳ ゴシック) it is the row's height; the drawing's pitch for Latin Calibri (15.66 pt at 11 pt, measured on Excel for Mac) is not Excel for
    // Windows' row line (15 pt), which is the 2 pt over three lines kept here as an open difference (`line_pitch`).
    let pitch_over = s.with(move |e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        let cell = sheet.cell(row, column).unwrap();
        let style = &sheet.styles[cell.style as usize];
        let japanese = cell.text.chars().any(|c| !c.is_ascii());
        let pitch = crate::sheet_view::line_pitch(style.font.as_deref(), japanese).unwrap_or(1.2) * style.font_size as f32 / 100.0;
        (pitch - line).max(0.0) * edit_lines as f32
    });
    assert!(boxed <= height + 4.0 + pitch_over + 1.0, "{what}: {cell}: the editing box is {boxed} pt high, the row {height} pt (the padding 4 pt and the pitch over the row's line {pitch_over} pt allowed)");
}

/// Open `file`, then every other way of giving the row of its B2 a height.
fn paths(file: &str, one: f32) {
    let mut s = Scenario::open(file);
    s.size(1400., 900.);
    let default_row = s.with(|e, _| sheet_of_slide(e.slide()).unwrap().default_row as f32 / EMU_PER_POINT as f32);
    // Excel 365 (COM, 27 workbooks): with a `<sheetFormatPr>` and no `customHeight` the default row is the Normal font's row (the file says
    // 15 here), which is also a row of data's (`one`).
    assert!((default_row - one).abs() < 0.01, "{file}: the default row is {default_row} pt, Excel's is the Normal font's {one} (the file says 15)");
    // Opening: a row with no customHeight is measured by its text, whatever `ht` says; one with customHeight is not.
    agree(&mut s, "B2", 1, 1, &format!("{file} open B2"));
    agree(&mut s, "B3", 2, 1, &format!("{file} open B3 (ht 90)"));
    agree(&mut s, "B5", 4, 1, &format!("{file} open B5 (ht 20)"));
    assert_eq!(row_pt(&mut s, 3), 145.5, "{file}: a row with customHeight is as the file says");
    assert!(custom(&mut s, 3));
    assert_eq!(row_pt(&mut s, 5), one, "{file}: a line of text is one row of the font");
    // The autofit command (the double click on the row's line) agrees with opening, and gives a row sized by hand back.
    let opened = row_pt(&mut s, 1);
    s.update(|e, w, cx| e.fit_rows(1, w, cx));
    assert_eq!(row_pt(&mut s, 1), opened, "{file}: autofit gives the height opening gave");
    s.update(|e, w, cx| e.fit_rows(3, w, cx));
    assert!(!custom(&mut s, 3), "{file}: autofit gives the row back to the sheet");
    agree(&mut s, "B4", 3, 1, &format!("{file} autofit B4"));
    // An entry: wrapping text typed in a cell that did not wrap.
    s.click_cell("B9");
    s.update(|e, _, cx| e.format_cells(StyleChange::Wrap(true), cx));
    s.enter("A line of text that is long enough to be broken into several lines in a column that is sixty characters wide: it goes on, and on, and on, until it is certainly more than one hundred and twenty characters long.", "enter");
    agree(&mut s, "B9", 8, 1, &format!("{file} entry"));
    // A column's width: fewer lines, the row follows.
    let before = row_pt(&mut s, 1);
    s.update(|e, _, cx| {
        e.select_lines(ibis_doc_model::sheet_command::Axis::Columns, 1, 1, cx);
        e.set_selected_column_width(100.0, cx);
    });
    assert!(row_pt(&mut s, 1) < before, "{file}: a wider column needs fewer lines: {} < {before}", row_pt(&mut s, 1));
    agree(&mut s, "B2", 1, 1, &format!("{file} wider column"));
    agree(&mut s, "B9", 8, 1, &format!("{file} wider column B9"));
    // Wrapping off and on again.
    s.click_cell("B2");
    s.update(|e, _, cx| e.format_cells(StyleChange::Wrap(false), cx));
    assert_eq!(row_pt(&mut s, 1), one, "{file}: wrapping off: one line");
    s.update(|e, _, cx| e.format_cells(StyleChange::Wrap(true), cx));
    agree(&mut s, "B2", 1, 1, &format!("{file} wrapping on"));
    // The font's size.
    s.update(|e, _, cx| e.format_cells(StyleChange::FontSize(1600), cx));
    agree(&mut s, "B2", 1, 1, &format!("{file} 16 pt"));
    // A paste.
    s.click_cell("B2");
    s.key("cmd-c");
    s.click_cell("B12");
    s.key("cmd-v");
    agree(&mut s, "B12", 11, 1, &format!("{file} paste"));
}

pub(super) fn agree_scenario() {
    // Yu Gothic is measured on Windows, where the font file is Microsoft's (`row_fit::row_pixels`): the workbooks in it run there only.
    if !cfg!(windows) {
        eprintln!("row_fit_paths_agree: the Yu Gothic workbooks are checked on Windows only");
        paths("test/editing/xlsx-rowfit-calibri.xlsx", 15.0);
        // ＭＳ ゴシック is not installed on a Mac without Microsoft Office's fonts (a stand-in's file would give the stand-in's rows): the rows
        // of that workbook keep what the file says there, and are checked where the typeface is.
        let installed = Scenario::open("test/editing/xlsx-rowfit-msgothic.xlsx").with(|e, _| e.resources.has_typeface("ＭＳ ゴシック")); // i18n-ok (typeface name)
        if installed {
            paths("test/editing/xlsx-rowfit-msgothic.xlsx", 13.5);
        } else {
            eprintln!("row_fit_paths_agree: ＭＳ ゴシック is not installed, its workbook is checked where it is"); // i18n-ok (typeface name)
        }
        return;
    }
    // The issue list of 2026-11-07 against Excel's rows (a workbook of openpyxl: `ht` is a reference value, the first font is Yu Gothic 11).
    let mut s = Scenario::open(ISSUES);
    s.size(1400., 900.);
    let default_row = s.with(|e, _| sheet_of_slide(e.slide()).unwrap().default_row as f32 / EMU_PER_POINT as f32);
    assert!((default_row - 18.75).abs() < 0.01, "the default row is {default_row} pt, Excel's 18.75 (the file says 15)");
    let rows: Vec<f32> = (0..40).map(|r| row_pt(&mut s, r)).collect();
    let wrong: Vec<String> = (0..40).filter(|&r| (rows[r] - EXCEL_ROWS[r]).abs() > 0.3).map(|r| format!("row {}: {} pt, Excel {}", r + 1, rows[r], EXCEL_ROWS[r])).collect();
    assert!(wrong.is_empty(), "rows that open at another height than in Excel: {wrong:?}");
    // H31 (the long completion comment): Excel's 6 lines of 18.75 pt.
    let (height, lines, line, shown) = state(&mut s, 30, 7);
    assert_eq!((lines, shown), (6, 6), "H31 breaks into 6 lines");
    assert!((height - 6.0 * line).abs() < 0.01 && (height - 112.5).abs() < 0.01, "H31 is {height} pt");
    agree(&mut s, "H31", 30, 7, "issues open H31");
    // The autofit command on H31 after it was made larger by hand: back to the measured height; the column's autofit leaves it.
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run_sheet(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetSize { slide, axis: ibis_doc_model::sheet_command::Axis::Rows, first: 30, last: 30, size: 200 * EMU_PER_POINT })], cx);
    });
    assert!(custom(&mut s, 30));
    s.update(|e, w, cx| e.fit_rows(30, w, cx));
    assert!(!custom(&mut s, 30));
    agree(&mut s, "H31", 30, 7, "issues autofit H31");
    // The row of customHeight stays (row 32 is 54 pt).
    assert_eq!(row_pt(&mut s, 31), 54.0);
    paths("test/editing/xlsx-rowfit-yugothic.xlsx", 18.75);
    paths("test/editing/xlsx-rowfit-calibri.xlsx", 15.0);
    paths("test/editing/xlsx-rowfit-msgothic.xlsx", 13.5);
}

/// Opening lays out the text of the first screens' rows, not of every cell: the cells laid out for their lines (`LAID_OUT_FOR_LINES`)
/// are the same for a sheet of 2,000 wrapping rows as for one of 8,000 (no time is asserted: the count is).
pub(super) fn open_cost_scenario() {
    use std::sync::atomic::Ordering;
    let mut s = Scenario::open("test/editing/xlsx-rowfit-calibri.xlsx");
    s.size(1400., 900.);
    let mut counts = Vec::new();
    for rows in [2000u32, 8000] {
        let counted = s.with(move |e, _| {
            let mut p = e.presentation().clone();
            let text = "A line of text that is long enough to be broken into several lines in a column that is sixty characters wide, and then some more text.";
            for shape in &mut p.slides[0].shapes {
                if let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind {
                    let sheet = std::sync::Arc::make_mut(sheet);
                    let style = sheet.cell(1, 1).unwrap().style;
                    sheet.rows = vec![sheet.default_row; rows as usize];
                    sheet.custom_rows.clear();
                    sheet.cells.clear();
                    for r in 0..rows {
                        sheet.cells.push(ibis_doc_model::SheetCell { row: r, column: 1, value: ibis_doc_model::CellValue::Text(text.into()), text: text.into(), kind: ibis_doc_model::CellKind::Text, formula: None, style, phonetic: None, runs: None });
                    }
                    sheet.measured_rows.0 = (0..rows).collect();
                }
            }
            let before = crate::sheet_view::LAID_OUT_FOR_LINES.load(Ordering::Relaxed);
            crate::row_fit::fit_measured_rows(&mut p, &e.resources);
            crate::sheet_view::LAID_OUT_FOR_LINES.load(Ordering::Relaxed) - before
        });
        eprintln!("{rows} wrapping rows: {counted} cells laid out when opening");
        counts.push(counted);
    }
    assert!(counts[0] > 0, "the rows of the first screens are measured by their lines");
    assert_eq!(counts[0], counts[1], "the cells laid out on opening do not grow with the sheet: {counts:?}");
    assert!(counts[0] <= 100, "{counts:?}");
}

/// A sheet converted from an .xls keeps the heights of the rows with nothing in them (a ROW record's, DEFAULTROWHEIGHT's where there is
/// none): Microsoft Excel for Windows measured no font for them (corpus 018's second sheet: rows 1 and 2 are 13.5 and 14.25 pt; 108's
/// empty rows under the chart 14.25 pt, which an 18.75 pt row made 63 pt too tall and the table below it jump a page). Opening
/// measures the rows with cells only: the empty rows and the default row are as the reader made them.
pub(super) fn xls_empty_rows_as_saved_scenario() {
    let path = "test/editing/legacy-compat.xls";
    let read = crate::read_file(&crate::shell::scenario::repo_root().join(path)).expect("the xls reads");
    let raw: Vec<(Vec<ibis_doc_model::Emu>, ibis_doc_model::Emu, Vec<u32>)> = read
        .presentation
        .slides
        .iter()
        .filter_map(|slide| slide.shapes.iter().find_map(|sh| if let ibis_doc_model::ShapeKind::Sheet(sheet) = &sh.kind { Some(sheet) } else { None }))
        .map(|sheet| (sheet.rows.clone(), sheet.default_row, (0..sheet.rows.len() as u32).filter(|r| sheet.written_fonts.contains_key(r) || sheet.row_fonts.contains_key(r)).collect()))
        .collect();
    assert!(!raw.is_empty());
    let mut s = Scenario::open(path);
    s.size(1200., 800.);
    let opened: Vec<(Vec<ibis_doc_model::Emu>, ibis_doc_model::Emu)> = s.with(|e, _| {
        e.presentation()
            .slides
            .iter()
            .filter_map(|slide| slide.shapes.iter().find_map(|sh| if let ibis_doc_model::ShapeKind::Sheet(sheet) = &sh.kind { Some((sheet.rows.clone(), sheet.default_row)) } else { None }))
            .collect()
    });
    for (i, ((rows, default_row, with_data), (after, after_default))) in raw.iter().zip(&opened).enumerate() {
        assert_eq!(default_row, after_default, "sheet {i}: the default row is the file's");
        for (r, (&before, &now)) in rows.iter().zip(after).enumerate() {
            if !with_data.contains(&(r as u32)) {
                assert_eq!(before, now, "sheet {i}: row {} has nothing in it and keeps the saved height", r + 1);
            }
        }
    }
}

/// A row that takes a line for furigana (a cell shows it, or the row has `row/@ph`) is taller than a row of the same cells without one
/// (Microsoft Excel 365: the row is the text's line plus P, which the furigana's font and size decide: `tasks/doc/m121-xlsx-phonetic.md`).
/// In `xlsx-phonetic.xlsx`, row 1 shows two readings, row 2 shows one and has `row/@ph`, row 3 has no reading shown (D3's column has
/// `col/@phonetic`, which shows nothing).
pub(super) fn phonetic_row_scenario() {
    let mut s = Scenario::open("test/editing/xlsx-phonetic.xlsx");
    let (r1, r2, r3) = (row_pt(&mut s, 0), row_pt(&mut s, 1), row_pt(&mut s, 2));
    eprintln!("phonetic rows: {r1} {r2} {r3} pt");
    assert!(r1 > r3 + 3.0, "row 1 takes a furigana line: {r1} against {r3}");
    assert!(r2 > r3 + 3.0, "row 2 takes a furigana line: {r2} against {r3}");
}

/// A row with a furigana line and the `ht` Excel saved (no `customHeight`) stays at that `ht` after the app has fitted the rows (Microsoft
/// Excel 365 uses it as it is, #M122), so the drawing and the page cut (made from `ht`) have the same height. `xlsx-phonetic-ht.xlsx` is
/// `xlsx-phonetic.xlsx` with `ht="27.75"` on rows 1 and 2 and `ht="30"` on row 3 (a row with no furigana).
pub(super) fn phonetic_row_with_saved_ht_scenario() {
    let mut s = Scenario::open("test/editing/xlsx-phonetic-ht.xlsx");
    let (r1, r2) = (row_pt(&mut s, 0), row_pt(&mut s, 1));
    eprintln!("phonetic rows with ht: {r1} {r2} pt");
    assert_eq!(r1, 27.75);
    assert_eq!(r2, 27.75);
}
