// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The check box in a cell (Excel's Insert > Check Box): a cell whose format is a check box shows its logical value as a box,
//! ticked for TRUE. A press on the box, or the space bar on the selected cells, turns the value (the cell's value changes, so it is an
//! undo step of its own); Insert > Check Box gives the selected cells the format and FALSE where they are empty.

use gpui_kit::*;
use ibis_doc_model::{CellEdit, CellInput, CellRange, CellValue, Command, SheetCommand, StyleChange};

use crate::shell::editor::SlideEditor;
use crate::ext::*;

#[ibis_office_macros::editor_ext(CheckboxExt)]
impl SlideEditor {
    /// The check box cells (row, column, ticked) among the selected cells that a press of the space bar turns: those with a check
    /// box format and a logical value that no formula gives.
    fn selected_checkboxes(&self) -> Vec<(u32, u32, bool)> {
        let Some((_, sheet)) = self.current_sheet() else { return Vec::new() };
        let ranges = self.selected_ranges();
        sheet
            .cells
            .iter()
            .filter(|c| c.formula.is_none() && sheet.styles.get(c.style as usize).is_some_and(|s| s.checkbox))
            .filter(|c| ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column)))
            .filter_map(|c| match c.value {
                CellValue::Bool(on) => Some((c.row, c.column, on)),
                _ => None,
            })
            .collect()
    }

    /// The space bar on selected check boxes: all take the opposite of the selected cell's (the active cell's) value, as one step.
    /// False when the selected cell is not a check box (the key is then a typed space).
    pub(crate) fn space_turns_checkboxes(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((column, row)) = self.current_cell() else { return false };
        let boxes = self.selected_checkboxes();
        let Some(&(_, _, on)) = boxes.iter().find(|&&(r, c, _)| (r, c) == (row, column)) else { return false };
        self.set_checkboxes(boxes.iter().map(|&(r, c, _)| (r, c)).collect(), !on, cx);
        true
    }

    /// The check box under a window position (its square, not the rest of its cell), if the cell's value is one a press turns.
    pub(crate) fn checkbox_under(&self, position: Point<Pixels>) -> Option<(u32, u32)> {
        let (layout, (shape, sheet)) = self.sheet_layout_at(position).zip(self.current_sheet())?;
        let p = layout.to_emu(position);
        let (column, row) = super::sheet_view::checkbox_at(sheet, (p.x - shape.frame.x) as f64, (p.y - shape.frame.y) as f64)?;
        let cell = sheet.cell(row, column)?;
        (cell.formula.is_none() && matches!(cell.value, CellValue::Bool(_)) && sheet.styles.get(cell.style as usize).is_some_and(|s| s.checkbox)).then_some((column, row))
    }

    /// A press on a check box: selects its cell and turns the box.
    pub(crate) fn press_checkbox(&mut self, column: u32, row: u32, cx: &mut Context<Self>) {
        let on = self.current_sheet().and_then(|(_, s)| s.cell(row, column)).is_some_and(|c| matches!(c.value, CellValue::Bool(true)));
        self.set_cell(column, row);
        self.set_checkboxes(vec![(row, column)], !on, cx);
    }

    fn set_checkboxes(&mut self, cells: Vec<(u32, u32)>, on: bool, cx: &mut Context<Self>) {
        if cells.is_empty() || self.protection_refuses_ranges(super::protection::Guarded::Cells, &cells.iter().map(|&(r, c)| CellRange { first_row: r, last_row: r, first_column: c, last_column: c }).collect::<Vec<_>>(), cx) {
            return;
        }
        let edits = cells.into_iter().map(|(row, column)| CellEdit { row, column, input: CellInput::Value(CellValue::Bool(on)) }).collect();
        self.run_sheet(vec![Command::Sheet(SheetCommand::SetCells { slide: self.current_slide, edits })], cx);
        self.sync_shape_formulas(cx);
    }

    /// Insert > Check Box: the selected cells become check boxes. Empty cells get FALSE; a logical value stays; other values are kept
    /// (shown as they are, as Excel's check box format does for what is not a logical value).
    pub(crate) fn insert_cell_checkboxes(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::FormatCells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let slide = self.current_slide;
        let mut commands = vec![Command::Sheet(SheetCommand::SetFormat { slide, ranges: ranges.clone(), change: StyleChange::Checkbox(true) })];
        if let Some((_, sheet)) = self.current_sheet() {
            let mut edits = Vec::new();
            for r in &ranges {
                for row in r.first_row..=r.last_row {
                    for column in r.first_column..=r.last_column {
                        if sheet.cell(row, column).is_none_or(|c| matches!(c.value, CellValue::Empty)) {
                            edits.push(CellEdit { row, column, input: CellInput::Value(CellValue::Bool(false)) });
                        }
                    }
                }
            }
            if !edits.is_empty() {
                commands.push(Command::Sheet(SheetCommand::SetCells { slide, edits }));
            }
        }
        self.run_sheet(commands, cx);
    }
}
