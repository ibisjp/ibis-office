// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The worksheet's fixed measures and colours that the window's workspace draws too (the row and column headers, the selection's green,
//! the frozen panes' line): they are the workbook's, shared here so the window need not name the workbook.

/// Height of the column headers (A, B, C…), in pixels.
pub const COLUMN_HEADER_HEIGHT: f32 = 20.0;

/// Width of the row headers (1, 2, 3…), in pixels (base width for up to 4 digits).
pub const ROW_HEADER_WIDTH: f32 = 44.0;

/// Width of the row headers dynamically sized based on sheet row count digits (Excel specification).
pub fn row_header_width(sheet: &ibis_doc_model::Sheet) -> f32 {
    // The rows with a size of their own, or with a cell (the cells are in row order: the last one is the lowest). Not
    // `row_offsets()`, which builds a million entries just to count them (issue #137).
    let max_row = sheet.rows.len().max(sheet.cells.last().map_or(0, |c| c.row as usize + 1));
    row_header_width_for_rows(max_row)
}

pub fn row_header_width_for_rows(max_row: usize) -> f32 {
    let digits = max_row.max(1).to_string().len().max(4);
    // 4 digits (up to 9,999): base 44.0 px
    // Each additional digit adds 8.0 px (character width ~7.0px at 11pt + margin)
    // 5 digits (10k+): 52.0px, 6 digits (100k+): 60.0px, 7 digits (1M+, e.g. 1,048,576): 68.0px
    ROW_HEADER_WIDTH + (digits.saturating_sub(4) as f32) * 8.0
}

/// Excel's green, for the selected cell and sheet tab.
pub const SELECTION_COLOR: u32 = 0x107c41;

/// The line along the edge of frozen panes.
pub const FROZEN_LINE: u32 = 0x9b9b9b;
