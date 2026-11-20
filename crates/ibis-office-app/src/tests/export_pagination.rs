// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use ibis_doc_model::{CellKind, CellRange, CellStyle, CellValue, EmuRect, Shape, ShapeId, ShapeKind, Sheet, SheetCell, Slide, SlideId};

const U: i64 = 12_700;
fn rect(x: i64, y: i64, w: i64, h: i64) -> EmuRect { EmuRect::new(x * U, y * U, w * U, h * U) }

fn cell(row: u32, column: u32, value: CellValue, formula: Option<&str>, style: u32) -> SheetCell {
    SheetCell { row, column, value, text: String::new(), kind: CellKind::Text, formula: formula.map(str::to_owned), style, phonetic: None, runs: None }
}

fn probe(extra: Option<SheetCell>, style: CellStyle) -> Slide {
    let mut sheet = Sheet::default();
    sheet.columns = vec![60 * U; 3];
    sheet.rows = vec![60 * U; 2];
    sheet.default_column = 60 * U;
    sheet.default_row = 60 * U;
    sheet.styles = vec![style];
    let mut cells = vec![cell(0, 0, CellValue::Text("left".into()), None, 0), cell(0, 2, CellValue::Text("right".into()), None, 0), cell(1, 1, CellValue::Text("bottom".into()), None, 0)];
    cells.extend(extra);
    sheet.cells = cells.into();
    sheet.print.paper = (100 * U, 100 * U);
    sheet.print.margins = [0; 4];
    let mut slide = Slide::new(SlideId(1));
    slide.shapes.push(Shape::new(ShapeId(1), rect(0, 0, 180, 120), ShapeKind::Sheet(std::sync::Arc::new(sheet))));
    slide
}

fn pages(slide: &Slide, titles: super::Titles) -> Vec<EmuRect> {
    super::sheet_pages(slide, rect(0, 0, 180, 120), titles, (0.0, 0.0)).unwrap().2
}

#[test]
fn print_pagination_keeps_internal_empty_pages_and_trims_only_the_tail() {
    let slide = probe(None, CellStyle::default());
    let out = pages(&slide, (None, None));
    assert_eq!(out.len(), 5);
    assert_eq!(out[1], rect(0, 60, 60, 60), "internal empty tile is retained");
    assert_eq!(out.last().unwrap(), &rect(120, 0, 60, 60));
}

#[test]
fn print_pagination_empty_formula_literal_and_hidden_values_still_occupy_pages() {
    let mut displayed_empty = cell(1, 2, CellValue::Empty, None, 0);
    displayed_empty.text = "displayed text".into();
    for extra in [cell(1, 2, CellValue::Empty, Some("\"\""), 0), cell(1, 2, CellValue::Text(String::new()), None, 0), cell(1, 2, CellValue::Number(42.0), None, 0), displayed_empty] {
        let slide = probe(Some(extra), CellStyle { number_format: ";;;".into(), ..CellStyle::default() });
        assert_eq!(pages(&slide, (None, None)).len(), 6);
    }
    let slide = probe(Some(cell(1, 2, CellValue::Empty, None, 0)), CellStyle { number_format: "0.00".into(), ..CellStyle::default() });
    assert_eq!(pages(&slide, (None, None)).len(), 5);
}

#[test]
fn print_pagination_uses_print_area_fills_merges_and_repeated_titles() {
    let mut slide = probe(None, CellStyle::default());
    if let ShapeKind::Sheet(sheet) = &mut slide.shapes[0].kind {
        std::sync::Arc::make_mut(sheet).print_area = Some(CellRange { first_row: 0, last_row: 1, first_column: 0, last_column: 2 });
    }
    assert_eq!(pages(&slide, (None, None)).len(), 6);
    let slide = probe(Some(cell(1, 2, CellValue::Empty, None, 0)), CellStyle { fill: Some(ibis_doc_model::Rgb(0xFFFFFF)), ..CellStyle::default() });
    assert_eq!(pages(&slide, (None, None)).len(), 6);
    let mut slide = probe(None, CellStyle::default());
    if let ShapeKind::Sheet(sheet) = &mut slide.shapes[0].kind {
        std::sync::Arc::make_mut(sheet).merges.push(CellRange { first_row: 1, last_row: 1, first_column: 1, last_column: 2 });
    }
    assert_eq!(pages(&slide, (None, None)).len(), 6);
    let slide = probe(None, CellStyle::default());
    assert_eq!(pages(&slide, (Some((0, 60 * U)), None)).len(), 6);
}

#[test]
fn print_pagination_shapes_must_be_visible_and_printable() {
    let mut slide = probe(None, CellStyle::default());
    slide.shapes.push(Shape::text_box(ShapeId(2), rect(120, 60, 20, 20), "shape"));
    assert_eq!(pages(&slide, (None, None)).len(), 6);
    slide.shape_prints.insert(ShapeId(2), false);
    assert_eq!(pages(&slide, (None, None)).len(), 5);
    slide.shape_prints.clear();
    slide.hidden_shapes.insert(ShapeId(2));
    assert_eq!(pages(&slide, (None, None)).len(), 5);
}

/// A column prints at whole pixels of the printer (screen pixels x printed digit / screen digit, 600 dpi) and is rounded again at a
/// scale; fitting a sheet one page wide takes the largest percent at which those columns fit the page less the 7 pixel box. Excel 365's
/// PDFs: ＭＳ Ｐゴシック 11 columns of 2.625, 8.43 and 13 characters are 121, 385 and 598 px (21, 67, 104 screen px x 46/8); at 96 %
/// 116, 370 and 574 px; 132's 50 columns of 4722 px at 96 % do not fit 4724.4 px of room less 7 px, 076's 24 of 4008 px at 100 %
/// do (4015.7 px).
#[test]
fn printed_columns_are_whole_printer_pixels_and_fit_by_them() {
    let px = ibis_doc_model::EMU_PER_INCH / 600;
    let factor = 46.0 * 0.12 / 6.0; // the printed digit (46 px of 0.12 pt) over the screen one (8 px of 0.75 pt)
    for (screen, printed) in [(21, 121), (67, 385), (104, 598)] {
        assert_eq!(super::printed_column(screen * 9525, factor), printed * px);
    }
    for (printed, at_96) in [(121, 116), (385, 370), (598, 574)] {
        assert_eq!(super::zoomed_row(printed * px, 0.96), (at_96 as f64 * px as f64 / 0.96).round() as i64);
    }
    let room = |total_px: i64| ((total_px - 7) * px) as f64;
    let edges = |widths: &[i64]| widths.iter().scan(0, |at, w| { *at += w * px; Some(*at) }).collect::<Vec<_>>();
    let mut xs = vec![0];
    xs.extend(edges(&[121, 121, 385, 598]));
    let area = EmuRect::new(0, 0, *xs.last().unwrap(), 100);
    // 1225 px: whole at 100 % on a page of 1225 + 7 px, one percent less on one pixel short.
    assert_eq!(super::fit_columns(&xs, area, room(1225 + 7)), 1.0);
    assert!(super::fit_columns(&xs, area, room(1225 + 6)) < 1.0);
}

/// Excel 365's page holds the columns' printed pixels plus 7 in a width of the paper in whole pixels (rounded), less the left margin
/// (whole pixels, down, at least 8) and the right margin (whole pixels, up). 218 probes (A4, Letter, B5, A5, A3, Legal, both
/// orientations, margins in fractions of a pixel, 10 to 200 %, 7 Normal fonts): the 7 and the roundings do not change with them.
#[test]
fn a_page_holds_the_columns_and_7_pixels_in_the_paper_less_the_rounded_margins() {
    let px = ibis_doc_model::EMU_PER_INCH / 600;
    // (paper in EMU, left, right, columns' px that fill a page less 7)
    let fits_on = |paper: i64, left: i64, right: i64, total_px: i64| -> usize {
        let mut sheet = Sheet::default();
        let half = total_px / 2;
        sheet.columns = vec![half * px, (total_px - half) * px];
        sheet.rows = vec![60 * U];
        sheet.default_column = 60 * U;
        sheet.default_row = 60 * U;
        sheet.styles = vec![CellStyle::default()];
        sheet.cells = vec![cell(0, 0, CellValue::Text("a".into()), None, 0), cell(0, 1, CellValue::Text("b".into()), None, 0)].into();
        sheet.print.paper = (paper, 100 * U * 100);
        sheet.print.margins = [left, right, 0, 0];
        *sheet.printed = true;
        let width = total_px * px;
        let mut slide = Slide::new(SlideId(1));
        slide.shapes.push(Shape::new(ShapeId(1), EmuRect::new(0, 0, width, 60 * U), ShapeKind::Sheet(std::sync::Arc::new(sheet))));
        super::sheet_pages(&slide, EmuRect::new(0, 0, width, 60 * U), (None, None), (0.0, 0.0)).unwrap().2.len()
    };
    let inch = ibis_doc_model::EMU_PER_INCH;
    // A4 (210 mm = 4960.6 px, a page of 4961), 0.7 inch (420 px) margins: 4961 - 840 - 7 = 4114 px fit.
    let a4 = 7_560_000 + 310;
    assert_eq!(fits_on(a4, inch * 7 / 10, inch * 7 / 10, 4114), 1);
    assert_eq!(fits_on(a4, inch * 7 / 10, inch * 7 / 10, 4115), 2);
    // Letter (5100 px): the right margin of 0.7 inch + 0.5 px rounds up, 5100 - 420 - 421 - 7.
    assert_eq!(fits_on(inch * 17 / 2, inch * 7 / 10, inch * 7 / 10 + px / 2, 4252), 1);
    assert_eq!(fits_on(inch * 17 / 2, inch * 7 / 10, inch * 7 / 10 + px / 2, 4253), 2);
    // A left margin of 0 counts as 8 px (5100 - 8 - 420 - 7).
    assert_eq!(fits_on(inch * 17 / 2, 0, inch * 7 / 10, 4665), 1);
    assert_eq!(fits_on(inch * 17 / 2, 0, inch * 7 / 10, 4666), 2);
}
