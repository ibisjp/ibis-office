// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > What-If 分析 > データテーブル: a block of results of one or two
//! formulas as input cells take the values across the top and down the
//! left.
//!
//! Calculated on a copy of the book with its data tables taken out: the
//! input cells set to the values, the book calculated, the formula's result
//! read, for each result cell.

use crate::command::{Command, Document};
use crate::model::{Presentation, ShapeKind, SlideId};
use crate::sheet::{CellValue, DataTable};
use crate::sheet_command::{CellEdit, CellInput, SheetCommand};
use std::sync::Arc;

/// The results of `table`, as (row, column, value) for each cell of its range.
pub fn values(presentation: &Presentation, slide: SlideId, table: &DataTable) -> Vec<(u32, u32, CellValue)> {
    let mut copy = presentation.clone();
    if let Some(s) = copy.slides.iter_mut().find(|s| s.id == slide) {
        for shape in &mut s.shapes {
            if let ShapeKind::Sheet(sheet) = &mut shape.kind {
                Arc::make_mut(sheet).data_tables.clear();
            }
        }
    }
    let header = |p: &Presentation, row: u32, column: u32| -> CellValue {
        p.slide(slide).and_then(crate::sheet_calc::sheet_of_slide).and_then(|s| s.cell(row, column)).map_or(CellValue::Empty, |c| c.value.clone())
    };
    let r = table.range;
    if r.first_row == 0 || r.first_column == 0 {
        return Vec::new();
    }
    let (top, left) = (r.first_row - 1, r.first_column - 1);
    // The headers are read before any input is set (an input may be one).
    let across: Vec<CellValue> = (r.first_column..=r.last_column).map(|c| header(&copy, top, c)).collect();
    let down: Vec<CellValue> = (r.first_row..=r.last_row).map(|row| header(&copy, row, left)).collect();
    let mut doc = Document::new(copy);
    let mut out = Vec::new();
    for (i, row) in (r.first_row..=r.last_row).enumerate() {
        for (j, column) in (r.first_column..=r.last_column).enumerate() {
            let (formula, sets): ((u32, u32), Vec<((u32, u32), &CellValue)>) = match (table.row_input, table.column_input) {
                (Some(row_input), Some(column_input)) => ((top, left), vec![(row_input, &across[j]), (column_input, &down[i])]),
                (Some(row_input), None) => ((row, left), vec![(row_input, &across[j])]),
                (None, Some(column_input)) => ((top, column), vec![(column_input, &down[i])]),
                (None, None) => return Vec::new(),
            };
            let edits = sets.into_iter().map(|((row, column), v)| CellEdit { row, column, input: CellInput::Value(v.clone()) }).collect();
            let value = match doc.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet: slide, edits })]) {
                Ok(()) => doc.presentation().slide(slide).and_then(crate::sheet_calc::sheet_of_slide).and_then(|s| s.cell(formula.0, formula.1)).map_or(CellValue::Empty, |c| c.value.clone()),
                Err(_) => CellValue::Error(crate::sheet::CellError::Value),
            };
            out.push((row, column, value));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Document;
    use crate::geometry::{EmuRect, EmuSize};
    use crate::model::{Shape, Slide};
    use crate::sheet::{CellRange, Sheet, Workbook};

    fn book() -> (Document, SlideId) {
        let mut p = Presentation::empty(EmuSize::WIDESCREEN);
        let id = p.new_slide_id();
        let mut slide = Slide::new(id);
        slide.name = Some("Sheet1".into());
        let shape = p.new_shape_id();
        let sheet = Sheet { columns: vec![100; 10], rows: vec![100; 20], grid_lines: true, ..Sheet::default() };
        slide.shapes.push(Shape { id: shape, frame: EmuRect::new(0, 0, 1000, 2000), rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Sheet(Arc::new(sheet)), floating: None });
        p.slides.push(slide);
        p.workbook = Some(Arc::new(Workbook {
            sheets: vec![crate::sheet::WorkbookSheet { name: "Sheet1".into(), state: Default::default(), slide: Some(id), hidden: None, other: false }],
            ..Default::default()
        }));
        (Document::new(p), id)
    }

    fn set(d: &mut Document, slide: SlideId, cells: &[(u32, u32, CellInput)]) {
        let edits = cells.iter().map(|(row, column, input)| CellEdit { row: *row, column: *column, input: input.clone() }).collect();
        d.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet: slide, edits })]).unwrap();
    }

    fn n(v: f64) -> CellInput {
        CellInput::Value(CellValue::Number(v))
    }

    fn at(d: &Document, slide: SlideId, row: u32, column: u32) -> CellValue {
        d.presentation().slide(slide).and_then(crate::sheet_calc::sheet_of_slide).and_then(|s| s.cell(row, column)).map_or(CellValue::Empty, |c| c.value.clone())
    }

    #[test]
    fn a_two_input_table_is_filled_and_follows_its_inputs() {
        let (mut d, s) = book();
        // Inputs F1 (row) and F2 (column); the formula in the corner A3; values across B3:D3 and down A4:A5.
        set(&mut d, s, &[(0, 5, n(0.0)), (1, 5, n(0.0)), (2, 0, CellInput::Formula("F1*10+F2".into())), (2, 1, n(1.0)), (2, 2, n(2.0)), (2, 3, n(3.0)), (3, 0, n(1.0)), (4, 0, n(2.0))]);
        let table = DataTable { range: CellRange { first_row: 3, last_row: 4, first_column: 1, last_column: 3 }, row_input: Some((0, 5)), column_input: Some((1, 5)) };
        d.execute(vec![Command::Sheet(SheetCommand::SetDataTable { sheet: s, table: table.clone() })]).unwrap();
        for (row, column, v) in [(3, 1, 11.0), (3, 2, 21.0), (3, 3, 31.0), (4, 1, 12.0), (4, 2, 22.0), (4, 3, 32.0)] {
            assert_eq!(at(&d, s, row, column), CellValue::Number(v), "{row},{column}");
        }
        // The inputs themselves are as they were.
        assert_eq!(at(&d, s, 0, 5), CellValue::Number(0.0));
        // A header changed: the results follow.
        set(&mut d, s, &[(2, 1, n(7.0))]);
        assert_eq!(at(&d, s, 3, 1), CellValue::Number(71.0));
        // A result is not changed on its own.
        assert!(d.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet: s, edits: vec![CellEdit { row: 3, column: 1, input: n(1.0) }] })]).is_err());
        assert!(d.undo());
        assert_eq!(at(&d, s, 3, 1), CellValue::Number(11.0));
        assert!(d.undo());
        assert_eq!(at(&d, s, 3, 1), CellValue::Empty);
    }

    #[test]
    fn one_input_tables_run_down_a_column_or_along_a_row() {
        let (mut d, s) = book();
        // A column input: the values down A4:A6, the formulas across B3:C3.
        set(&mut d, s, &[(0, 5, n(0.0)), (2, 1, CellInput::Formula("F1*2".into())), (2, 2, CellInput::Formula("F1+100".into())), (3, 0, n(1.0)), (4, 0, n(2.0)), (5, 0, n(3.0))]);
        let by_column = DataTable { range: CellRange { first_row: 3, last_row: 5, first_column: 1, last_column: 2 }, row_input: None, column_input: Some((0, 5)) };
        d.execute(vec![Command::Sheet(SheetCommand::SetDataTable { sheet: s, table: by_column })]).unwrap();
        assert_eq!((at(&d, s, 3, 1), at(&d, s, 5, 1), at(&d, s, 4, 2)), (CellValue::Number(2.0), CellValue::Number(6.0), CellValue::Number(102.0)));
        // A row input: the values along E9:G9, the formulas down D10:D11.
        set(&mut d, s, &[(8, 4, n(1.0)), (8, 5, n(2.0)), (8, 6, n(3.0)), (9, 3, CellInput::Formula("F1*F1".into())), (10, 3, CellInput::Formula("-F1".into()))]);
        let by_row = DataTable { range: CellRange { first_row: 9, last_row: 10, first_column: 4, last_column: 6 }, row_input: Some((0, 5)), column_input: None };
        d.execute(vec![Command::Sheet(SheetCommand::SetDataTable { sheet: s, table: by_row })]).unwrap();
        assert_eq!((at(&d, s, 9, 6), at(&d, s, 10, 5)), (CellValue::Number(9.0), CellValue::Number(-2.0)));
    }

    #[test]
    fn data_table_tracks_row_column_insert_delete_and_move() {
        let (mut d, s) = book();
        set(&mut d, s, &[(0, 5, n(0.0)), (1, 5, n(0.0)), (2, 0, CellInput::Formula("F1*10+F2".into())), (2, 1, n(1.0)), (2, 2, n(2.0)), (2, 3, n(3.0)), (3, 0, n(1.0)), (4, 0, n(2.0))]);
        let table = DataTable { range: CellRange { first_row: 3, last_row: 4, first_column: 1, last_column: 3 }, row_input: Some((0, 5)), column_input: Some((1, 5)) };
        d.execute(vec![Command::Sheet(SheetCommand::SetDataTable { sheet: s, table })]).unwrap();

        // Insert 1 row at row 0 and 1 column at column 0 -> table shifts to (4..=5, 2..=4), inputs to (1, 6) and (2, 6).
        d.execute(vec![
            Command::Sheet(SheetCommand::Insert { sheet: s, axis: crate::sheet_command::Axis::Rows, at: 0, count: 1 }),
            Command::Sheet(SheetCommand::Insert { sheet: s, axis: crate::sheet_command::Axis::Columns, at: 0, count: 1 }),
        ])
        .unwrap();
        let sheet = crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
        assert_eq!(sheet.data_tables[0].range, CellRange { first_row: 4, last_row: 5, first_column: 2, last_column: 4 });
        assert_eq!(sheet.data_tables[0].row_input, Some((1, 6)));
        assert_eq!(sheet.data_tables[0].column_input, Some((2, 6)));
        // Change a moved header cell and verify the moved data table recalculates.
        set(&mut d, s, &[(3, 2, n(9.0))]);
        assert_eq!(at(&d, s, 4, 2), CellValue::Number(91.0));
        assert_eq!(at(&d, s, 5, 4), CellValue::Number(32.0));

        // Move the input cells from G2:G3 ((1,6)..=(2,6)) to H2:H3 ((1,7)..=(2,7)).
        d.execute(vec![Command::Sheet(SheetCommand::MoveCells {
            sheet: s,
            from: CellRange { first_row: 1, last_row: 2, first_column: 6, last_column: 6 },
            row: 1,
            column: 7,
        })])
        .unwrap();
        let sheet = crate::sheet_calc::sheet_of_slide(d.presentation().slide(s).unwrap()).unwrap();
        assert_eq!(sheet.data_tables[0].row_input, Some((1, 7)));
        assert_eq!(sheet.data_tables[0].column_input, Some((2, 7)));
        set(&mut d, s, &[(3, 2, n(5.0))]);
        assert_eq!(at(&d, s, 4, 2), CellValue::Number(51.0));
    }
}

