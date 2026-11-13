// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The height of the rows Microsoft Excel measures by their fonts.
//!
//! A row with no `customHeight` of its own is not as high as the file says (the height stored is what the program that saved
//! it measured): Excel for Windows measures it again by the fonts in it, never below the Normal font's row. Microsoft Excel 365's
//! rows of a Normal font set to each of 43 typefaces at 6 to 72 pt (`font-rows`, 11,438 rows; ppem is the size in whole pixels
//! at 96 dpi, `round(pt × 4 / 3)`; a row is `S` pixels, 0.75 pt each) follow
//!
//! `S = tmAscent + tmDescent + pad + extra`
//!
//! - `tmAscent`, `tmDescent` (GDI's TEXTMETRIC at 96 dpi): `yMax` and `−yMin` of the font's `VDMX` row for the ppem, when the font
//!   has a `VDMX` group for the DEFAULT charset at 1:1 (Arial, Times New Roman, Segoe UI, Verdana, Georgia ...); otherwise
//!   `round(usWinAscent × ppem / unitsPerEm)` and `round(usWinDescent × ppem / unitsPerEm)`. Measured: GDI's values equal these for
//!   ppem 11 to 99 in 14 typefaces (the VDMX group is read as the table says; hinting needs not be run).
//! - `pad = floor(|ppem − tmAscent| + 1.5)`. Known: it equals `|ppem − tmAscent| + 1`, and with the rounded `tmAscent` (not the
//!   exact `usWinAscent × ppem / unitsPerEm`) it fits ＭＳ Ｐゴシック at 72 pt as well. Unknown: why Excel adds it, and why it is
//!   the distance from the ascent to the em that is added.
//! - `extra = 2 × max(0, floor((round(EL) − 1) / 2))`, with the external leading
//!   `EL = max(0, hhea.lineGap + hhea.ascender − hhea.descender − usWinAscent − usWinDescent) / unitsPerEm × ppem`
//!   (GDI's tmExternalLeading is built so). Known: it is 0 for typefaces without it (Calibri, Segoe UI ...), and gives the steps of
//!   2 pixels at EL = 2.5, 4.5, 6.5 ... in Arial (ppem 77 on), Times New Roman (ppem 59 on) and SimSun (ppem 18, 33, 47, 61, 75).
//!   Unknown: why Excel adds twice the external leading in steps (the parity of the steps).
//!
//! It holds for ppem 11 to 100 in each of the 86 sizes of Calibri, Cambria, Candara, Consolas, Constantia, Corbel, Tahoma,
//! Gabriola, Cascadia Code, Bahnschrift, メイリオ, Meiryo UI, Segoe UI, Verdana, Georgia, Courier New, Palatino Linotype, Arial,
//! Times New Roman, SimSun, ＭＳ Ｐゴシック, ＭＳ ゴシック and ＭＳ 明朝 (checked against the font files of this PC's Windows;
//! the others of the 43 were checked with the earlier form of `pad` with the exact ascent, and not with this one).
//!
//! 游ゴシック and 游明朝: the formula gives 游ゴシック 11 pt (ppem 15, usWinAscent 2017, usWinDescent 619, hhea lineGap 1024:
//! 15 + 5 + 1 + 4) 25 pixels = 18.75 pt, which is Excel 365's standard height of a workbook in that font (COM measurement, 2026-11-09).
//! Known: at other sizes the `extra` steps of these two come one pixel of EL later than the formula's in places (earlier measurement:
//! 游明朝 fits `extra = 2 × floor(round(EL) / 2)` at 86 of 86 sizes, 游ゴシック the formula at 74 of 86 and that at 64 of 86), the cause of
//! the one-pixel phase is not known; those rows can be one pixel off. A typeface that is not installed (a stand-in's file would give the
//! stand-in's rows) and sizes under 11 ppem (the bitmap fonts) keep the height the file stores.

use std::sync::Arc;

use ibis_doc_model::{Presentation, ShapeKind};

use crate::render::font_metrics::WinRowMetrics;
use crate::render::shape_view::Resources;

/// The height of a row of `name` at `size` (hundredths of a point), in 96 dpi pixels.
fn row_pixels(res: &Resources, name: &str, size: u32) -> Option<f64> {
    let name = name.trim();
    // 游ゴシック, 游明朝 (游 UI ...): the font file decides the row, and macOS's own YuGothic is not Microsoft's (other metrics: its
    // rows come out 16.5 pt and 24 pt where Excel's are 18.75), so they are measured where the file is Microsoft's, on Windows.
    if !cfg!(windows) && (name.starts_with("游") || name.starts_with("Yu ")) { // i18n-ok（書体の名前）
        return None;
    }
    pixels_from_metrics(&*res.win_row_metrics(name)?, size)
}

/// The formula in the module's comment, from the font file's values.
fn pixels_from_metrics(m: &WinRowMetrics, size: u32) -> Option<f64> {
    let ppem = (f64::from(size) / 100.0 * 4.0 / 3.0 + 0.5).floor();
    if ppem < 11.0 {
        return None;
    }
    Some(line_pixels(m, size))
}

/// The same formula without the 11 ppem floor: the height of one line of text of the font at a size under it (the furigana's line, whose
/// size is often 6 to 9 pt; `pixels_from_metrics` does not give a row there, the bitmap fonts decide those rows).
fn line_pixels(m: &WinRowMetrics, size: u32) -> f64 {
    let ppem = (f64::from(size) / 100.0 * 4.0 / 3.0 + 0.5).floor();
    let round = |x: f64| (x + 0.5).floor();
    let em = m.units_per_em;
    let (ascent, descent) = match m.vdmx.as_ref().and_then(|rows| rows.iter().find(|r| f64::from(r.0) == ppem)) {
        Some(&(_, y_max, y_min)) => (f64::from(y_max), -f64::from(y_min)),
        None => (round(m.win_ascent * ppem / em), round(m.win_descent * ppem / em)),
    };
    let pad = ((ppem - ascent).abs() + 1.5).floor();
    let leading = (m.hhea_line_gap + m.hhea_ascent - m.hhea_descent - m.win_ascent - m.win_descent).max(0.0) / em * ppem;
    let extra = 2.0 * ((round(leading) - 1.0) / 2.0).floor().max(0.0);
    ascent + descent + pad + extra
}

/// The height of a row with no cell of its own in a sheet: the tallest of the columns' fonts (see `Sheet::column_fonts`), and of the
/// Normal font where `<cols>` leaves a column out. `None` when a column's typeface is not known.
fn sheet_floor(res: &Resources, sheet: &ibis_doc_model::Sheet, normal_name: &str, normal_size: u32, normal: f64) -> Option<f64> {
    floor_from(&sheet.column_fonts, normal, |font, size| row_pixels(res, font.as_deref().unwrap_or(normal_name), if size > 0 { size } else { normal_size }))
}

fn floor_from(column_fonts: &[(u32, u32, Option<String>, u32)], normal: f64, pixels: impl Fn(&Option<String>, u32) -> Option<f64>) -> Option<f64> {
    let mut floor: f64 = 0.0;
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for (first, last, font, size) in column_fonts {
        let px = if *size == 0 && font.is_none() { normal } else { pixels(font, *size)? };
        floor = floor.max(px);
        ranges.push((*first, *last));
    }
    ranges.sort_unstable();
    let mut next = 0u32;
    for (first, last) in ranges {
        if first > next {
            break;
        }
        next = next.max(last.saturating_add(1));
    }
    if next < 16_384 {
        floor = floor.max(normal);
    }
    Some(floor)
}

/// The Normal font of a workbook (name, size in hundredths of a point): what a cell, a column or a row with no font of its own is
/// measured in. Excel's is the first font of `styles.xml` (a COM measurement: a file whose `cellXfs[0]` and first font are
/// Yu Gothic 11 and whose Normal cell style names Calibri 12 has the standard height 18.75 pt, Yu Gothic 11's row, not Calibri 12's 15.75).
pub(crate) fn normal_font(presentation: &Presentation) -> (String, u32) {
    let default = presentation.workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
    (default.font.clone().unwrap_or_else(|| "Calibri".into()), if default.font_size > 0 { default.font_size } else { 1100 })
}

/// One line of text in `font` at `size` (hundredths of a point; 0 is the Normal size), in points: Excel's row of that font
/// (`row_pixels`). `None` where the typeface cannot be measured.
/// One line of the furigana in `font` at `size`, in points: the formula of `line_pixels` at any size (Microsoft Excel 365, Arial 6 pt: 11 px
/// by the formula, 11 measured; ＭＳ ゴシック 6 pt: 10 and 10), `None` where the typeface cannot be measured.
pub(crate) fn ruby_line_pt(res: &Resources, normal: &(String, u32), font: Option<&str>, size: u32) -> Option<f32> {
    let size = if size > 0 { size } else { normal.1 };
    let name = font.unwrap_or(&normal.0).trim();
    if !cfg!(windows) && (name.starts_with("游") || name.starts_with("Yu ")) { // i18n-ok（書体の名前）
        return None;
    }
    res.win_row_metrics(name).map(|m| (line_pixels(&m, size) * 0.75) as f32)
}

pub(crate) fn line_pt(res: &Resources, normal: &(String, u32), font: Option<&str>, size: u32) -> Option<f32> {
    let size = if size > 0 { size } else { normal.1 };
    row_pixels(res, font.unwrap_or(&normal.0), size).map(|px| (px * 0.75) as f32)
}

/// The height of a row with nothing in it, in points, by the sheet's rule, the file's default row (`declared_pt`: the sheet's
/// `defaultRowHeight`, or `APP_DEFAULT_ROW_PT` where there is no `<sheetFormatPr>`) and the Normal font's row (`normal_row_pt`; `None` where
/// the font cannot be measured: the file's value stays). Microsoft Excel 365 for Windows' COM measurement, 27 workbooks
/// (`tasks/doc/row-height-rowprobe.txt`).
pub(crate) fn empty_row_pt(rule: ibis_doc_model::DefaultRowRule, declared_pt: f32, normal_row_pt: Option<f32>) -> f32 {
    match rule {
        ibis_doc_model::DefaultRowRule::ByFont => normal_row_pt.unwrap_or(declared_pt),
        ibis_doc_model::DefaultRowRule::AsSaved | ibis_doc_model::DefaultRowRule::NoFormat => declared_pt,
    }
}

/// Whether the row has a cell of its own: written (`written_fonts`), a style of its own (`row_fonts`), or, with `text`, a cell.
fn row_has_data(sheet: &ibis_doc_model::Sheet, row: u32, text: bool) -> bool {
    sheet.row_fonts.contains_key(&row) || sheet.written_fonts.contains_key(&row) || (text && sheet.cells.get(sheet.cells.partition_point(|c| c.row < row)).is_some_and(|c| c.row == row))
}

/// The height Excel fits row `row` to, in points: the tallest line of any cell in it (the written ones, empty too, and the row's own
/// style), a cell of wrapped text as many lines as the drawing breaks it into (`drawn`: `sheet_view::drawn_line_counts`) and a cell of
/// text with line breaks as many as it has, never below the sheet's default row. A merged cell does not set the height of its rows.
///
/// This is the one place that says how high a row is for its content: opening a file (`fit_measured_rows`), a row's
/// 行の高さの自動調整, and the refit after an edit, a paste, a column's width or wrapping being changed all call it. `None` where a
/// typeface cannot be measured and `fallback` is false (the row then keeps the height it has); with `fallback` the default row scaled
/// by the size stands for the line. Without `text` only the fonts count (a row of a big file at opening: no cell is visited).
pub(crate) fn fitted_height_pt(sheet: &ibis_doc_model::Sheet, res: &Resources, normal: &(String, u32), row: u32, drawn: &std::collections::HashMap<(u32, u32), usize>, text: bool, fallback: bool, data_floor_pt: Option<f32>) -> Option<f32> {
    let default_pt = sheet.default_row as f32 / ibis_doc_model::EMU_PER_POINT as f32;
    let line = |font: Option<&str>, size: u32| -> Option<f32> {
        line_pt(res, normal, font, size).or_else(|| fallback.then(|| default_pt * if size > 0 { size } else { normal.1 } as f32 / normal.1.max(1) as f32))
    };
    // A row with nothing in it is the sheet's default row (Excel's: `APP_DEFAULT_ROW_PT`); one with cells is at least the row of the fonts
    // of its unwritten cells (the Normal font, the columns' fonts: `data_floor_pt`).
    let has_data = row_has_data(sheet, row, text);
    let mut height = match sheet.row_fonts.get(&row) {
        Some((font, size)) => line(font.as_deref(), *size)?,
        None if has_data => data_floor_pt?,
        None => default_pt,
    };
    for (font, size) in sheet.written_fonts.get(&row).into_iter().flatten() {
        height = height.max(line(font.as_deref(), *size)?);
    }
    if thick_bottom(sheet, row) {
        height += THICK_BOTTOM_PT;
    }
    // A row that takes a line for furigana (a cell shows it, or the row has `row/@ph`) is that much taller: the text's line plus P, which
    // depends on the furigana's font and size alone (Microsoft Excel 365: 游ゴシック 6, 9, 12, 14, 18 pt: 12, 19, 24, 29, 36 px whatever
    // the text, `tasks/doc/m121-xlsx-phonetic.md`). The formula for P is not known; the row of that font stands for it (it is 1 px over at 6,
    // 12 and 14 pt and 2 px at 18 pt for 游ゴシック), and the reader's estimate where the typeface cannot be measured. A row with
    // `customHeight` is not fitted, so it never gets it.
    let ruby = sheet.phonetic_info.rows.get(&row).map_or(0.0, |(font, size, estimate)| ruby_line_pt(res, normal, font.as_deref(), *size).unwrap_or(*estimate as f32 / 100.0));
    if !text {
        return Some(height + ruby);
    }
    // The cells are in row-major order: the row's are found by search.
    let start = sheet.cells.partition_point(|c| c.row < row);
    for cell in sheet.cells[start..].iter().take_while(|c| c.row == row) {
        let Some(style) = sheet.styles.get(cell.style as usize) else { continue };
        let merged = sheet.merge_at(cell.row, cell.column).is_some_and(|m| m.first_row != m.last_row || m.first_column != m.last_column);
        if merged {
            continue;
        }
        // A part of the text in a larger size sets the line.
        let size = cell.runs.iter().flat_map(|r| r.iter()).filter_map(|r| r.size).max().map_or(style.font_size, |s| s.max(style.font_size));
        let one = line(style.font.as_deref(), size)?;
        height = height.max(one);
        if cell.text.is_empty() {
            continue;
        }
        let lines = if style.wrap { drawn.get(&(cell.row, cell.column)).copied().unwrap_or(1) } else { cell.text.split('\n').count() };
        height = height.max(one * lines.max(1) as f32);
    }
    Some(height + ruby)
}

/// A medium border under a row makes its fitted height one pixel (0.75 pt) taller (Microsoft Excel 365 COM, xlsx-sales.xlsx: row 9, whose cells
/// have a medium bottom border, is 20.25 pt against the 19.5 pt of the rows beside it; row 2, which has no cell, is 20.25 pt too, its
/// bottom edge being row 3's top border; without that border it is 19.5 pt again; a medium border on the top of a row does not change that row:
/// `tasks/doc/m72-xlsx-sales-row-heights.md`). #M73 (Calibri 11, 游ゴシック 11/14, ＭＳ Ｐゴシック 11; under A1, over A2, under A1:B1): hair and thin
/// lines (dashed and dotted ones too) add nothing, medium (its dashed forms too), thick and double add one pixel, in every font.
const THICK_BOTTOM_PT: f32 = 0.75;

/// The lines that make a row's bottom edge add a pixel: medium and its dashed forms, thick, double (#M73). `SlantDashDot` is a medium line
/// in Excel's table of styles (its weight is medium); hair, thin, dotted, dashed, dash-dot and dash-dot-dot are thin.
fn adds_a_pixel(line: ibis_doc_model::BorderLine) -> bool {
    use ibis_doc_model::BorderLine::*;
    matches!(line, Medium | Thick | Double | MediumDashed | MediumDashDot | MediumDashDotDot | SlantDashDot)
}

/// Whether the bottom edge of `row` is a line of medium weight or more (`adds_a_pixel`): a cell of the row has one, or a cell of the row below has one as its top (the edge
/// is shared; the row itself may have no cell).
fn thick_bottom(sheet: &ibis_doc_model::Sheet, row: u32) -> bool {
    let medium = |b: &Option<ibis_doc_model::Border>| b.as_ref().is_some_and(|b| adds_a_pixel(b.line));
    let in_row = |r: u32, pick: &dyn Fn(&ibis_doc_model::CellStyle) -> bool| {
        let start = sheet.cells.partition_point(|c| c.row < r);
        sheet.cells[start..].iter().take_while(|c| c.row == r).any(|c| sheet.styles.get(c.style as usize).is_some_and(pick))
    };
    in_row(row, &|s| medium(&s.bottom)) || in_row(row + 1, &|s| medium(&s.top))
}

/// The row of a sheet's cells that nobody wrote, in points, with its font's rows: the tallest of the Normal font's and the columns'.
pub(crate) fn data_floor_pt(res: &Resources, sheet: &ibis_doc_model::Sheet, normal: &(String, u32)) -> Option<f32> {
    let px = row_pixels(res, &normal.0, normal.1)?;
    sheet_floor(res, sheet, &normal.0, normal.1, px).map(|f| (f * 0.75) as f32)
}

/// The least a row with cells is fitted to when edited or refitted: `data_floor_pt`, and where the Normal font (or a column's) cannot be
/// measured, no more than the height the row has now. The sheet's default row stands for the rows with nothing in them (what the file
/// declares); a row of cells was given its height by the reader's estimate or by the file, and a refit that cannot measure the font
/// has no reason to lift it to the default row (F2 and Enter in a cell with no change moved a row of 游ゴシック 12 from 19.5 pt to the
/// file's 20 pt where the typeface is not installed, #fix51); it still shrinks a taller row to the default row, and grows it by what
/// is in the row.
pub(crate) fn data_floor_or_kept_pt(res: &Resources, sheet: &ibis_doc_model::Sheet, normal: &(String, u32), row: u32) -> f32 {
    let default_pt = sheet.default_row as f32 / ibis_doc_model::EMU_PER_POINT as f32;
    data_floor_pt(res, sheet, normal).unwrap_or_else(|| {
        let current = sheet.rows.get(row as usize).copied().filter(|&h| h > 0).unwrap_or(sheet.default_row);
        default_pt.min(current as f32 / ibis_doc_model::EMU_PER_POINT as f32)
    })
}

/// Set the height of each row Excel measures (`Sheet::measured_rows`) to the one `fitted_height_pt` gives, for the sheets whose fonts are
/// known. A row with nothing in it is the sheet's default row (`ibis_pptx::APP_DEFAULT_ROW_PT` unless the file asks for one with
/// `customHeight`: set by the reader), a row with cells the Normal font's row at least.
pub fn fit_measured_rows(presentation: &mut Presentation, res: &Resources) {
    let normal_font = normal_font(presentation);
    // Where the Normal font cannot be measured only the rows with nothing in them are set (to the default row); the others keep their height.
    let normal = row_pixels(res, &normal_font.0, normal_font.1);
    let emu_pt = |pt: f32| (pt as f64 * ibis_doc_model::EMU_PER_POINT as f64).round() as i64;
    for slide in &mut presentation.slides {
        // The row tops before and after the rows are measured: the objects anchored to cells follow their rows (`follow_rows`).
        let mut moved: Option<(Vec<i64>, Vec<i64>)> = None;
        for shape in &mut slide.shapes {
            let ShapeKind::Sheet(sheet) = &mut shape.kind else { continue };
            // A sheet whose every row has a wrapping cell has no `measured_rows` (the reader leaves those rows to the window below),
            // so it is skipped only when no style wraps either.
            if sheet.measured_rows.is_empty() && !sheet.styles.iter().any(|style| style.wrap) {
                continue;
            }
            let before = sheet.row_offsets();
            // What a row measures at least: the cells no one wrote take their column's style (the Normal one where the column has none,
            // so where `<cols>` does not reach all 16,384 columns), or the row's own style if it has one (`<row s customFormat>`);
            // Excel measures all the cells of a row, the written ones (empty too) and these. A sheet whose every column has a style of
            // a small font has no Normal row as its floor.
            let floor = normal.and_then(|normal| sheet_floor(res, sheet, &normal_font.0, normal_font.1, normal));
            let sheet = Arc::make_mut(sheet);
            let data_floor = floor.map(|f| (f * 0.75) as f32);
            // `<sheetFormatPr>` with no `customHeight`: the default row is the Normal font's row (the file's `defaultRowHeight` is ignored).
            if let (Some(data_floor), ibis_doc_model::DefaultRowRule::ByFont) = (data_floor, *sheet.default_row_rule) {
                sheet.default_row = emu_pt(empty_row_pt(ibis_doc_model::DefaultRowRule::ByFont, sheet.default_row as f32 / ibis_doc_model::EMU_PER_POINT as f32, Some(data_floor)));
            }
            // A row of wrapped text is measured by the lines of its text (a layout of the text): only the rows of the first screens, so
            // that opening a file of a million cells does not lay out a million cells; the others keep the height the file stores.
            let window: Vec<u32> = (0..OPEN_WINDOW_ROWS.min(sheet.rows.len() as u32))
                .filter(|&r| {
                    sheet.rows[r as usize] > 0
                        && !sheet.custom_rows.contains(&r)
                        && (!*sheet.rows_as_saved || sheet.measured_rows.contains(&r))
                        && {
                            let start = sheet.cells.partition_point(|c| c.row < r);
                            sheet.cells[start..].iter().take_while(|c| c.row == r).any(|c| !c.text.is_empty() && sheet.styles.get(c.style as usize).is_some_and(|s| s.wrap))
                        }
                })
                .collect();
            let drawn = open_line_counts(sheet, res, &window);
            let as_saved = *sheet.default_row_rule == ibis_doc_model::DefaultRowRule::AsSaved;
            let mut rows: Vec<(u32, f32)> = sheet.measured_rows.iter().filter(|&&r| !as_saved || row_has_data(sheet, r, false)).filter_map(|&r| fitted_height_pt(sheet, res, &normal_font, r, &drawn, false, false, data_floor).map(|h| (r, h))).collect();
            rows.extend(window.iter().filter_map(|&r| fitted_height_pt(sheet, res, &normal_font, r, &drawn, true, false, data_floor).map(|h| (r, h))));
            // A row with cells but no wrapping text (formatted empty cells of a wrapping style, text on one line) is not one of the
            // `measured_rows` where the sheet has a wrapping style anywhere (the reader leaves every row with a wrapping cell out) and
            // is not in the window either; it is measured by its fonts and lines like the others (a sheet converted from an .xls keeps its rows).
            if !*sheet.rows_as_saved && sheet.styles.iter().any(|style| style.wrap) {
                let mut wrapped_text = std::collections::HashSet::new();
                for cell in sheet.cells.iter() {
                    if !cell.text.is_empty() && sheet.styles.get(cell.style as usize).is_some_and(|s| s.wrap) {
                        wrapped_text.insert(cell.row);
                    }
                }
                let measured: std::collections::HashSet<u32> = sheet.measured_rows.iter().copied().chain(window.iter().copied()).collect();
                let rest: Vec<u32> = (0..sheet.rows.len() as u32)
                    .filter(|r| sheet.rows[*r as usize] > 0 && !sheet.custom_rows.contains(r) && !measured.contains(r) && !wrapped_text.contains(r) && row_has_data(sheet, *r, false))
                    .collect();
                rows.extend(rest.into_iter().filter_map(|r| fitted_height_pt(sheet, res, &normal_font, r, &drawn, true, false, data_floor).map(|h| (r, h))));
            }
            for (row, pt) in rows {
                if let Some(h) = sheet.rows.get_mut(row as usize)
                    && *h > 0
                {
                    *h = emu_pt(pt);
                    sheet.fitted_rows.insert(row, *h);
                }
            }
            let after = sheet.row_offsets();
            if before != after {
                // The sheet's shape is its grid's size (as `fit_frame` keeps it after every command).
                shape.frame.height = sheet.rows.iter().sum();
                moved = Some((before, after));
            }
        }
        if let Some((before, after)) = moved {
            follow_rows(slide, &before, &after);
        }
    }
}

/// The objects of a sheet's slide follow the rows when the rows are measured and change height. The reader placed them from the heights
/// the file stores (`xdr:from`/`to` row and `rowOff` against the stored rows); Excel anchors them to the cells, so each corner keeps
/// its row and its offset in the row against the new row tops. A `Free` object (`absoluteAnchor`, `editAs="absolute"`) stays where it
/// is, a `MoveOnly` one keeps its height, and a `MoveAndSize` one has both corners follow. Without it a saved file would anchor the
/// object to other cells than the file's (`save::update_two_cell_corners` places the corners against the rows as they are now).
fn follow_rows(slide: &mut ibis_doc_model::Slide, before: &[i64], after: &[i64]) {
    use ibis_doc_model::ObjectPlacement;
    let remap = |y: i64| -> i64 {
        let i = before.partition_point(|&o| o <= y).saturating_sub(1).min(before.len().saturating_sub(2));
        match (before.get(i), after.get(i)) {
            (Some(&b), Some(&a)) => a + (y - b),
            _ => y,
        }
    };
    for shape in &mut slide.shapes {
        if matches!(shape.kind, ShapeKind::Sheet(_)) {
            continue;
        }
        let placement = slide.shape_placements.get(&shape.id).copied().unwrap_or_default();
        if placement == ObjectPlacement::Free {
            continue;
        }
        let (top, bottom) = (remap(shape.frame.y), remap(shape.frame.y + shape.frame.height));
        shape.frame.y = top;
        if placement == ObjectPlacement::MoveAndSize {
            shape.frame.height = (bottom - top).max(0);
        }
    }
}

/// How many rows from the top of a sheet are measured by their text when it opens (about two screens).
const OPEN_WINDOW_ROWS: u32 = 100;

/// The lines the drawing breaks the wrapped text of `rows` (ascending) into, one layout for each run of consecutive rows.
fn open_line_counts(sheet: &ibis_doc_model::Sheet, res: &Resources, rows: &[u32]) -> std::collections::HashMap<(u32, u32), usize> {
    let mut drawn = std::collections::HashMap::new();
    let mut at = 0;
    while at < rows.len() {
        let first = rows[at];
        let mut last = first;
        while at + 1 < rows.len() && rows[at + 1] == last + 1 {
            at += 1;
            last = rows[at];
        }
        at += 1;
        drawn.extend(super::sheet_view::drawn_line_counts(sheet, res, first, last));
    }
    drawn
}

#[cfg(test)]
mod tests {
    use super::{line_pixels, pixels_from_metrics};
    use crate::render::font_metrics::WinRowMetrics;

    fn metrics(win: (f64, f64), hhea: (f64, f64, f64), em: f64, vdmx: Option<Vec<(u16, i16, i16)>>) -> WinRowMetrics {
        WinRowMetrics { win_ascent: win.0, win_descent: win.1, hhea_ascent: hhea.0, hhea_descent: hhea.1, hhea_line_gap: hhea.2, units_per_em: em, vdmx }
    }

    /// Rows of Microsoft Excel 365 (font-rows: the Normal font set to the typeface, the standard row in points / 0.75): (pt × 100,
    /// pixels).
    #[test]
    fn the_formula_gives_the_rows_microsoft_excel_measured_without_a_vdmx_group() {
        let fonts: [(WinRowMetrics, [(u32, f64); 6]); 4] = [
            // Calibri
            (metrics((1950.0, 550.0), (1536.0, -512.0, 452.0), 2048.0, None), [(900, 16.0), (1100, 20.0), (1200, 21.0), (2000, 35.0), (3600, 62.0), (7200, 123.0)]),
            // ＭＳ Ｐゴシック (72 pt is ppem 96)
            (metrics((220.0, 36.0), (220.0, -36.0, 0.0), 256.0, None), [(900, 15.0), (1100, 18.0), (1200, 19.0), (2000, 32.0), (3600, 56.0), (7200, 111.0)]),
            // Tahoma
            (metrics((2049.0, 423.0), (2049.0, -423.0, 0.0), 2048.0, None), [(900, 15.0), (1100, 19.0), (1200, 20.0), (2000, 34.0), (3600, 59.0), (7200, 117.0)]),
            // SimSun (lineGap 36: the external leading adds 2 pixels from ppem 18, 4 from 33, 6 from 47 ...; ppem 18, 25, 32, 47, 60, 75)
            (metrics((220.0, 36.0), (220.0, -36.0, 36.0), 256.0, None), [(1350, 24.0), (1875, 32.0), (2400, 42.0), (3525, 61.0), (4500, 75.0), (5625, 97.0)]),
        ];
        for (m, rows) in fonts {
            for (size, px) in rows {
                assert_eq!(pixels_from_metrics(&m, size), Some(px), "winA {} size {size}", m.win_ascent);
            }
        }
    }

    /// The furigana's line under 11 ppem (Microsoft Excel 365, #M121: the row grows by the line of the furigana's font): Arial 6 pt is 11
    /// pixels and 9 pt 16 (measured 11 and 16), where a row of that font is not given (`pixels_from_metrics`). 游ゴシック 6 pt comes out 13
    /// against 12 measured: the one pixel of its known phase (module comment). The estimate of a whole row (18 pt, 24 px) it replaced was
    /// 12 px over for the 6 pt lines.
    #[test]
    fn the_furigana_line_is_the_formula_below_11_ppem_too() {
        let arial = metrics((1854.0, 434.0), (1854.0, -434.0, 67.0), 2048.0, None);
        assert_eq!(pixels_from_metrics(&arial, 600), None);
        assert_eq!(line_pixels(&arial, 600), 11.0);
        assert_eq!(line_pixels(&arial, 900), 16.0);
        let yu_gothic = metrics((2017.0, 619.0), (1802.0, -455.0, 1024.0), 2048.0, None);
        assert_eq!(line_pixels(&yu_gothic, 600), 13.0);
        assert_eq!(line_pixels(&yu_gothic, 900), 19.0);
    }

    /// Yu Gothic 11 pt (ppem 15; the values of YuGothR.ttc: usWinAscent 2017, usWinDescent 619, hhea ascender 1802, descender -455,
    /// lineGap 1024, 2048 to the em): 15 + 5 + 1 + 4 = 25 pixels = 18.75 pt, the standard height Microsoft Excel 365 for Windows gives a
    /// workbook whose first font is Yu Gothic 11 (COM measurement 2026-11-09: `StandardHeight` 18.75, a row of 6 lines 112.5).
    #[test]
    fn yu_gothic_11_is_25_pixels() {
        let yu_gothic = metrics((2017.0, 619.0), (1802.0, -455.0, 1024.0), 2048.0, None);
        assert_eq!(pixels_from_metrics(&yu_gothic, 1100), Some(25.0));
    }

    /// Typefaces with a VDMX group: GDI's ascent and descent are its yMax and -yMin (the rows at ppem 77, 80, 96 of Arial and 59,
    /// 80 of Times New Roman, where the external leading adds 2 pixels).
    #[test]
    fn the_formula_takes_the_ascent_and_descent_of_a_vdmx_group() {
        let arial = metrics((1854.0, 434.0), (1854.0, -434.0, 67.0), 2048.0, Some(vec![(77, 70, -17), (80, 72, -17), (96, 86, -21)]));
        for (ppem, px) in [(77u32, 97.0), (80, 100.0), (96, 120.0)] {
            assert_eq!(pixels_from_metrics(&arial, ppem * 75), Some(px), "Arial ppem {ppem}");
        }
        let times = metrics((1825.0, 443.0), (1825.0, -443.0, 87.0), 2048.0, Some(vec![(59, 53, -14), (80, 71, -19)]));
        for (ppem, px) in [(59u32, 76.0), (80, 102.0)] {
            assert_eq!(pixels_from_metrics(&times, ppem * 75), Some(px), "Times New Roman ppem {ppem}");
        }
    }

    /// Excel measures every cell of a row, the unwritten ones with their column's style: a sheet whose `<cols>` styles all 16,384
    /// columns in a small font has that font's row as its floor, not the Normal font's (the Normal font's row where a column is left out).
    #[test]
    fn the_floor_of_a_row_is_the_tallest_column_style() {
        let small = |_: &Option<String>, _: u32| Some(20.0);
        let all = vec![(0, 8, Some("a".to_string()), 900), (9, 16_383, Some("a".to_string()), 900)];
        assert_eq!(super::floor_from(&all, 25.0, small), Some(20.0));
        let gap = vec![(0, 8, Some("a".to_string()), 900), (10, 16_383, Some("a".to_string()), 900)];
        assert_eq!(super::floor_from(&gap, 25.0, small), Some(25.0));
        assert_eq!(super::floor_from(&[], 25.0, small), Some(25.0));
        let unstyled = vec![(0, 16_383, None, 0)];
        assert_eq!(super::floor_from(&unstyled, 25.0, small), Some(25.0));
        assert_eq!(super::floor_from(&all, 25.0, |_, _| None), None);
    }

    /// Excel's empty rows (COM, 27 workbooks: Normal ＭＳ Ｐゴシック 11, 游ゴシック 11, Calibri 11, with and without `<sheetFormatPr>`,
    /// `defaultRowHeight` 13.5, 14.25, 15, 18.75, `customHeight` or not).
    #[test]
    fn empty_rows_follow_the_rule_excel_measured() {
        use ibis_doc_model::DefaultRowRule::{AsSaved, ByFont, NoFormat};
        let normals = [("MS PGothic", 13.5f32), ("Yu Gothic", 18.75), ("Calibri", 15.0)];
        for (name, row) in normals {
            for declared in [13.5f32, 14.25, 15.0, 18.75] {
                assert_eq!(super::empty_row_pt(AsSaved, declared, Some(row)), declared, "{name}: customHeight keeps defaultRowHeight {declared}");
                assert_eq!(super::empty_row_pt(ByFont, declared, Some(row)), row, "{name}: no customHeight, defaultRowHeight {declared} is ignored");
                assert_eq!(super::empty_row_pt(ByFont, declared, None), declared, "{name}: a font that cannot be measured leaves the file's value");
            }
            // No <sheetFormatPr>: the rows without a <row> are 18.75 whatever the Normal font.
            assert_eq!(super::empty_row_pt(NoFormat, 18.75, Some(row)), 18.75, "{name}: no sheetFormatPr");
        }
    }

    #[test]
    fn bitmap_sizes_are_not_measured() {
        let calibri = metrics((1950.0, 550.0), (1536.0, -512.0, 452.0), 2048.0, None);
        assert_eq!(pixels_from_metrics(&calibri, 700), None);
    }

    /// xlsx-sales.xlsx (Mac Excel's file): the rows Microsoft Excel 365 measured 20.25 pt against 19.5 pt (COM, `tasks/doc/m72-xlsx-sales-row-heights.md`)
    /// are the ones whose bottom edge is a medium line: row 9 by its own cells, row 2 (no cell) by row 3's top border.
    #[test]
    fn a_medium_line_under_a_row_is_found_in_its_cells_or_the_next_rows_top() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx");
        let presentation = ibis_pptx::read(&std::fs::read(path).unwrap()).unwrap();
        let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(&presentation.slides[0]).unwrap();
        let thick: Vec<u32> = (0..12).filter(|&r| super::thick_bottom(sheet, r)).collect();
        assert_eq!(thick, vec![1, 8], "rows 2 and 9");
        // A merged cell of one row (A1:E1, 14 pt) is a written font of its row: Excel's row 1 is the 14 pt row, merged or not.
        assert!(sheet.written_fonts.get(&0).is_some_and(|f| f.iter().any(|(_, size)| *size == 1400)), "{:?}", sheet.written_fonts.get(&0));
    }

    /// #M73: which lines under a row add a pixel to its height.
    #[test]
    fn lines_of_medium_weight_and_more_add_a_pixel() {
        use ibis_doc_model::BorderLine::*;
        for line in [Medium, Thick, Double, MediumDashed, MediumDashDot, MediumDashDotDot, SlantDashDot] {
            assert!(super::adds_a_pixel(line), "{line:?}");
        }
        for line in [Thin, Hair, Dotted, Dashed, DashDot, DashDotDot] {
            assert!(!super::adds_a_pixel(line), "{line:?}");
        }
    }
}
