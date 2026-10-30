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
//! Left out: 游ゴシック and 游明朝. Their `extra` steps come one pixel of EL later than the formula's: 游明朝 fits with
//! `extra = 2 × floor(round(EL) / 2)` (86 of 86), but 游ゴシック does not fit that (64 of 86) nor the formula (74 of 86), and the
//! cause of the one-pixel phase is not known. Their rows, those of a typeface that is not installed (a stand-in's file would give
//! the stand-in's rows) and of sizes under 11 ppem (the bitmap fonts) keep the height the file stores.

use std::sync::Arc;

use ibis_doc_model::{Presentation, ShapeKind};

use crate::render::font_metrics::WinRowMetrics;
use crate::render::shape_view::Resources;

/// The height of a row of `name` at `size` (hundredths of a point), in 96 dpi pixels.
fn row_pixels(res: &Resources, name: &str, size: u32) -> Option<f64> {
    let name = name.trim();
    // 游ゴシック, 游明朝 (游 UI ...): the phase of `extra` is not worked out (see above).
    if name.starts_with("游") || name.starts_with("Yu ") { // i18n-ok（書体の名前）
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
    let round = |x: f64| (x + 0.5).floor();
    let em = m.units_per_em;
    let (ascent, descent) = match m.vdmx.as_ref().and_then(|rows| rows.iter().find(|r| f64::from(r.0) == ppem)) {
        Some(&(_, y_max, y_min)) => (f64::from(y_max), -f64::from(y_min)),
        None => (round(m.win_ascent * ppem / em), round(m.win_descent * ppem / em)),
    };
    let pad = ((ppem - ascent).abs() + 1.5).floor();
    let leading = (m.hhea_line_gap + m.hhea_ascent - m.hhea_descent - m.win_ascent - m.win_descent).max(0.0) / em * ppem;
    let extra = 2.0 * ((round(leading) - 1.0) / 2.0).floor().max(0.0);
    Some(ascent + descent + pad + extra)
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

/// Set the height of each row Excel measures (`Sheet::measured_rows`) to the one its fonts give, and the default row to the
/// Normal font's row, for the sheets whose fonts are known.
pub(crate) fn fit_measured_rows(presentation: &mut Presentation, res: &Resources) {
    let default = presentation.workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
    let normal_name = default.font.clone().unwrap_or_else(|| "Calibri".into());
    let normal_size = if default.font_size > 0 { default.font_size } else { 1100 };
    let Some(normal) = row_pixels(res, &normal_name, normal_size) else { return };
    let emu = |px: f64| (px * 0.75 * ibis_doc_model::EMU_PER_POINT as f64).round() as i64;
    for slide in &mut presentation.slides {
        for shape in &mut slide.shapes {
            let ShapeKind::Sheet(sheet) = &mut shape.kind else { continue };
            if sheet.measured_rows.is_empty() {
                continue;
            }
            // The row of a font: None when it is not known (the row keeps its height).
            let px = |font: &Option<String>, size: u32| {
                if size == 0 && font.is_none() { Some(normal) } else { row_pixels(res, font.as_deref().unwrap_or(&normal_name), if size > 0 { size } else { normal_size }) }
            };
            // What a row measures at least: the cells no one wrote take their column's style (the Normal one where the column has none,
            // so where `<cols>` does not reach all 16,384 columns), or the row's own style if it has one (`<row s customFormat>`);
            // Excel measures all the cells of a row, the written ones (empty too) and these. A sheet whose every column has a style of
            // a small font has no Normal row as its floor, and a row that is not in the file is as high as that floor.
            let Some(floor) = sheet_floor(res, sheet, &normal_name, normal_size, normal) else { continue };
            let rows: std::collections::HashMap<u32, Option<f64>> = sheet
                .measured_rows
                .iter()
                .map(|&r| {
                    let mut height = match sheet.row_fonts.get(&r) {
                        Some((font, size)) => px(font, *size),
                        None => Some(floor),
                    };
                    for (font, size) in sheet.written_fonts.get(&r).into_iter().flatten() {
                        height = match (height, px(font, *size)) {
                            (Some(a), Some(b)) => Some(a.max(b)),
                            _ => None,
                        };
                    }
                    (r, height)
                })
                .collect();
            let sheet = Arc::make_mut(sheet);
            for (row, px) in rows {
                if let (Some(px), Some(h)) = (px, sheet.rows.get_mut(row as usize))
                    && *h > 0
                {
                    *h = emu(px);
                    sheet.fitted_rows.insert(row, *h);
                }
            }
            sheet.default_row = emu(floor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::pixels_from_metrics;
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

    #[test]
    fn bitmap_sizes_are_not_measured() {
        let calibri = metrics((1950.0, 550.0), (1536.0, -512.0, 452.0), 2048.0, None);
        assert_eq!(pixels_from_metrics(&calibri, 700), None);
    }
}
