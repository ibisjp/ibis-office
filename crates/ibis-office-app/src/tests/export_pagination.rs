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
