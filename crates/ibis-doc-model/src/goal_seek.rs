// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > What-If 分析 > ゴールシーク: the value of one cell (a constant)
//! that makes a formula's result a given value.
//!
//! Tried on a copy of the book: the cell set to a value, the book
//! calculated, the formula's result read. A secant search from the cell's
//! own value; where it does not converge, the values away from it widened
//! by tens until the result crosses the target, and halved from there.

use crate::command::{Command, Document};
use crate::model::{Presentation, SlideId};
use crate::sheet::CellValue;
use crate::sheet_command::{CellEdit, CellInput, SheetCommand};

/// Why no value was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GoalSeekError {
    /// The cell to change holds a formula (or is not on the sheet).
    ChangingCell,
    /// The formula's cell has no formula, or its result is not a number.
    NotANumber,
    /// No value of the cell makes the formula's result the target.
    NotFound,
}

/// The value of cell `changing` (row, column) of `slide`'s sheet that makes
/// the number cell `formula` shows `target`.
pub fn goal_seek(presentation: &Presentation, slide: SlideId, formula: (u32, u32), target: f64, changing: (u32, u32)) -> Result<f64, GoalSeekError> {
    let sheet = presentation.slide(slide).and_then(crate::sheet_calc::sheet_of_slide).ok_or(GoalSeekError::ChangingCell)?;
    if sheet.cell(formula.0, formula.1).is_none_or(|c| c.formula.is_none()) {
        return Err(GoalSeekError::NotANumber);
    }
    let start = match sheet.cell(changing.0, changing.1) {
        Some(c) if c.formula.is_some() => return Err(GoalSeekError::ChangingCell),
        Some(c) => match &c.value {
            CellValue::Number(n) => *n,
            CellValue::Empty => 0.0,
            _ => return Err(GoalSeekError::ChangingCell),
        },
        None => 0.0,
    };
    let mut doc = Document::new(presentation.clone());
    // The formula's result minus the target at `x`.
    let mut miss = |x: f64| -> Option<f64> {
        let edit = CellEdit { row: changing.0, column: changing.1, input: CellInput::Value(CellValue::Number(x)) };
        doc.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet: slide, edits: vec![edit] })]).ok()?;
        let sheet = doc.presentation().slide(slide).and_then(crate::sheet_calc::sheet_of_slide)?;
        match sheet.cell(formula.0, formula.1).map(|c| &c.value) {
            Some(CellValue::Number(v)) if v.is_finite() => Some(v - target),
            _ => None,
        }
    };
    let tolerance = 1e-9 * target.abs().max(1.0);
    let found = |f: f64| f.abs() <= tolerance;
    let f0 = miss(start);
    if f0.is_some_and(found) {
        return Ok(start);
    }
    // Secant.
    if let Some(mut f0) = f0 {
        let (mut x0, mut x1) = (start, start + (start.abs() * 1e-3).max(1e-3));
        for _ in 0..100 {
            let Some(f1) = miss(x1) else { break };
            if found(f1) {
                return Ok(x1);
            }
            let slope = (f1 - f0) / (x1 - x0);
            if slope == 0.0 || !slope.is_finite() {
                break;
            }
            let x2 = x1 - f1 / slope;
            if !x2.is_finite() {
                break;
            }
            (x0, f0, x1) = (x1, f1, x2);
        }
    }
    // Widening steps either side, then halving between two that cross.
    let mut previous: Option<(f64, f64)> = miss(start).map(|f| (start, f));
    for sign in [1.0, -1.0] {
        let mut last = previous;
        let mut step = 1e-3;
        while step < 1e12 {
            let x = start + sign * step;
            if let Some(f) = miss(x) {
                if found(f) {
                    return Ok(x);
                }
                if let Some((px, pf)) = last
                    && pf.signum() != f.signum()
                {
                    let (mut lo, mut hi, mut flo) = (px, x, pf);
                    for _ in 0..200 {
                        let mid = (lo + hi) / 2.0;
                        let Some(fm) = miss(mid) else { break };
                        if found(fm) || (hi - lo).abs() <= 1e-15 * mid.abs().max(1.0) {
                            return Ok(mid);
                        }
                        if fm.signum() == flo.signum() {
                            (lo, flo) = (mid, fm);
                        } else {
                            hi = mid;
                        }
                    }
                }
                last = Some((x, f));
            }
            step *= 10.0;
        }
        previous = None;
    }
    Err(GoalSeekError::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{EmuRect, EmuSize};
    use crate::model::{Shape, ShapeKind, Slide};
    use crate::sheet::{Sheet, Workbook};
    use std::sync::Arc;

    fn book(cells: &[(u32, u32, CellInput)]) -> (Presentation, SlideId) {
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
        let mut d = Document::new(p);
        let edits = cells.iter().map(|(row, column, input)| CellEdit { row: *row, column: *column, input: input.clone() }).collect();
        d.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet: id, edits })]).unwrap();
        (d.presentation().clone(), id)
    }

    fn n(v: f64) -> CellInput {
        CellInput::Value(CellValue::Number(v))
    }

    #[test]
    fn a_square_is_solved_from_the_cells_own_value() {
        let (p, s) = book(&[(0, 0, n(2.0)), (0, 1, CellInput::Formula("A1*A1+3".into()))]);
        let x = goal_seek(&p, s, (0, 1), 28.0, (0, 0)).unwrap();
        assert!((x - 5.0).abs() < 1e-6, "{x}");
    }

    #[test]
    fn a_cube_with_a_flat_start_is_found_by_widening() {
        let (p, s) = book(&[(0, 0, n(0.0)), (0, 1, CellInput::Formula("A1^3-2".into()))]);
        let x = goal_seek(&p, s, (0, 1), 0.0, (0, 0)).unwrap();
        assert!((x - 2f64.cbrt()).abs() < 1e-6, "{x}");
    }

    #[test]
    fn what_cannot_be_reached_or_changed_is_refused() {
        let (p, s) = book(&[(0, 0, n(1.0)), (0, 1, CellInput::Formula("A1*0+1".into())), (0, 2, CellInput::Formula("A1".into()))]);
        assert_eq!(goal_seek(&p, s, (0, 1), 5.0, (0, 0)), Err(GoalSeekError::NotFound));
        assert_eq!(goal_seek(&p, s, (0, 1), 5.0, (0, 2)), Err(GoalSeekError::ChangingCell));
        assert_eq!(goal_seek(&p, s, (0, 0), 5.0, (0, 0)), Err(GoalSeekError::NotANumber));
    }
}
