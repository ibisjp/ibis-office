// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! オート SUM (the toolbar's Σ menu), as Excel: on one cell, the numbers
//! above it (or else to its left) go into the function and the entry is
//! left open to confirm with Enter; on a range, each column's total goes
//! under it (into the range's last row when that row is empty).

use ibis_doc_model::{CellEdit, CellInput, CellRange, CellValue, Command, SheetCell, SheetCommand, StyleChange, cell_name};

use gpui_kit::*;

use super::cell_edit::EditMode;
use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The functions of the menu: (name, label).
pub(crate) const FUNCTIONS: &[(&str, &str)] = &[("SUM", "_xlsx.sum"), ("AVERAGE", "_xlsx.average"), ("COUNT", "_xlsx.count-2"), ("MAX", "_xlsx.max"), ("MIN", "_xlsx.min-2")];

fn is_number(cell: Option<&SheetCell>) -> bool {
    cell.is_some_and(|c| matches!(c.value, CellValue::Number(_)))
}

fn range_text(first_row: u32, first_column: u32, last_row: u32, last_column: u32) -> String {
    let a = cell_name(first_column, first_row);
    if (first_row, first_column) == (last_row, last_column) { a } else { format!("{a}:{}", cell_name(last_column, last_row)) }
}

#[ibis_office_macros::editor_ext(Autosum)]
impl SlideEditor {
    pub(crate) fn auto_function(&mut self, function: &str, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let (Some(range), Some((_, sheet))) = (self.selected_range(), self.current_sheet()) else { return };
        if range.first_row == range.last_row && range.first_column == range.last_column {
            let (row, column) = (range.first_row, range.first_column);
            // The numbers right above, else right to the left.
            let mut top = row;
            while top > 0 && is_number(sheet.cell(top - 1, column)) {
                top -= 1;
            }
            let argument = if top < row {
                range_text(top, column, row - 1, column)
            } else {
                let mut left = column;
                while left > 0 && is_number(sheet.cell(row, left - 1)) {
                    left -= 1;
                }
                if left < column { range_text(row, left, row, column - 1) } else { String::new() }
            };
            self.begin_cell_edit(EditMode::Edit, false, cx);
            let text = format!("={function}({argument})");
            self.cell_replace(None, &text, None, cx);
            return;
        }
        // A row of empty cells: in each, the numbers above it (as one cell does).
        let empty = |r: u32, c: u32| sheet.cell(r, c).is_none_or(|x| x.value.is_empty() && x.formula.is_none());
        if range.first_row == range.last_row && (range.first_column..=range.last_column).all(|c| empty(range.first_row, c)) {
            let row = range.first_row;
            let mut edits = Vec::new();
            let mut formats = Vec::new();
            for column in range.first_column..=range.last_column {
                let mut top = row;
                while top > 0 && is_number(sheet.cell(top - 1, column)) {
                    top -= 1;
                }
                if top == row {
                    continue;
                }
                edits.push(CellEdit { row, column, input: CellInput::Formula(format!("{function}({})", range_text(top, column, row - 1, column))) });
                let code = sheet.cell(top, column).map(|c| sheet.styles[c.style as usize].number_format.clone());
                if let (Some(code), false) = (code, function == "COUNT")
                    && code != "General"
                    && code != "@"
                {
                    formats.push((column, code));
                }
            }
            return self.put_totals(row, edits, formats, cx);
        }
        // A range: a total under each column that has numbers.
        let last_empty = range.last_row > range.first_row
            && (range.first_column..=range.last_column).all(|c| sheet.cell(range.last_row, c).is_none_or(|x| x.value.is_empty() && x.formula.is_none()));
        let (data_last, target) = if last_empty { (range.last_row - 1, range.last_row) } else { (range.last_row, range.last_row + 1) };
        let mut edits = Vec::new();
        let mut formats = Vec::new();
        for column in range.first_column..=range.last_column {
            if !(range.first_row..=data_last).any(|r| is_number(sheet.cell(r, column))) {
                continue;
            }
            let formula = format!("{function}({})", range_text(range.first_row, column, data_last, column));
            edits.push(CellEdit { row: target, column, input: CellInput::Formula(formula) });
            // The total shows as the numbers do (a General cell).
            let general = sheet.cell(target, column).is_none_or(|c| sheet.styles[c.style as usize].number_format == "General");
            let code = sheet.cell(range.first_row, column).map(|c| sheet.styles[c.style as usize].number_format.clone());
            if let (true, Some(code), false) = (general, code, function == "COUNT") {
                if code != "General" && code != "@" {
                    formats.push((column, code));
                }
            }
        }
        self.put_totals(target, edits, formats, cx);
    }

    /// Put the totals into row `row` (one step), with the numbers' formats.
    fn put_totals(&mut self, row: u32, edits: Vec<CellEdit>, formats: Vec<(u32, String)>, cx: &mut Context<Self>) {
        if edits.is_empty() {
            return;
        }
        let slide = self.current_slide;
        let mut commands = vec![Command::Sheet(SheetCommand::SetCells { slide, edits })];
        for (column, code) in formats {
            let range = CellRange { first_row: row, first_column: column, last_row: row, last_column: column };
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: StyleChange::NumberFormat(code) }));
        }
        if let Err(err) = self.document.execute(commands) {
            log::warn!("auto function rejected: {err}");
        }
        cx.notify();
    }
}
