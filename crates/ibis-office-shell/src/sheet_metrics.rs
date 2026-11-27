// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The worksheet's fixed measures and colours that the window's workspace draws too (the row and column headers, the selection's green,
//! the frozen panes' line): they are the workbook's, shared here so the window need not name the workbook.

/// Height of the column headers (A, B, C…), in pixels.
pub const COLUMN_HEADER_HEIGHT: f32 = 20.0;

/// Width of the row headers (1, 2, 3…), in pixels (base width for up to 4 digits).
pub const ROW_HEADER_WIDTH: f32 = 44.0;

/// Excel's size of an outline button (a level button or a - / + button, a square, in pixels) at 100 %, and the largest it gets, as measured on Microsoft 365
/// Excel 16.0 build 20625 at 96 DPI (`tasks/doc/requests/2026-11-26-291-outline-zoom-result.md`): 23 px at 100 %, and 42 px from 175 % up.
const OUTLINE_BUTTON_AT_100: f32 = 23.0;
const OUTLINE_BUTTON_MAX: f32 = 42.0;

/// The button's size by the sheet's zoom: the 100 % size times the zoom, rounded, and not above Excel's largest. No rule that gives every measured size was
/// found (Excel's sizes go 5, 9, 11, 14, 15, 15, 16, 19, 20, 21, 22, 23, 23, 25, 26, 29, 29, 31, 33, 35, 36, 40, 42 px at 25, 40, 50, 60, 65, 70, 75, 80, 85,
/// 90, 95, 100, 105, 110, 115, 120, 125, 130, 140, 150, 160, 170, 175 %, in no pattern of one rounding): this is at most 2 px off, 17 of 30 measured zooms exact.
pub fn outline_button_size(sheet: &ibis_doc_model::Sheet) -> f32 {
    let zoom = f32::from(sheet.zoom.unwrap_or(100));
    (OUTLINE_BUTTON_AT_100 * zoom / 100.0).round().clamp(1.0, OUTLINE_BUTTON_MAX)
}

fn outline_depth(levels: &std::collections::BTreeMap<u32, u8>) -> u32 {
    u32::from(levels.values().copied().max().unwrap_or(0))
}

/// Whether rows or columns are grouped (the outline's strips are there).
pub fn has_outline(sheet: &ibis_doc_model::Sheet) -> bool {
    !(sheet.outline_rows.is_empty() && sheet.outline_columns.is_empty())
}

/// The outline's strip at the left of the row numbers: a level button for each level of the grouped rows and one more, and 5 px
/// (Excel at 100 %: 74 px for two levels, 97 px for three); 0 when nothing is grouped.
pub fn outline_width(sheet: &ibis_doc_model::Sheet) -> f32 {
    // Only columns grouped: Excel has no strip at the left; the columns' level buttons sit above the row numbers (#291 W1).
    if sheet.outline_rows.is_empty() {
        return 0.0;
    }
    outline_button_size(sheet) * (outline_depth(&sheet.outline_rows) + 1) as f32 + 5.0
}

/// The outline's strip above the column headers: a button for each level of the grouped columns and one more, and 2 px (Excel at 100 %: 71 px
/// for two levels). With only rows grouped, the one band of the row level buttons.
pub fn outline_height(sheet: &ibis_doc_model::Sheet) -> f32 {
    if !has_outline(sheet) {
        return 0.0;
    }
    let b = outline_button_size(sheet);
    // Only rows grouped: Excel puts the level buttons in the column headers' own band (no strip above it); the band is as high as a button and its 2 px.
    // Our headers are lower than a button, so the strip is only what the band lacks.
    if sheet.outline_columns.is_empty() {
        return (b + 2.0 - COLUMN_HEADER_HEIGHT).max(0.0);
    }
    b * (outline_depth(&sheet.outline_columns) + 1) as f32 + outline_extra(sheet).1
}

/// What the columns' level buttons are further from the row numbers' start (x), and the strip higher (y), than the buttons themselves, as measured on Excel
/// (#291 W1). y: 3 px at every zoom from 25 to 400 % (the strip is 3 buttons + 3 px). x: 5 px at 100 %; Excel's own x is 1..6 px up to 175 % and then
/// grows with the row numbers' width (9 px at 200 %, 39 px at 400 %), which the window does not follow: this is the 100 % value times the zoom, rounded.
pub fn outline_extra(sheet: &ibis_doc_model::Sheet) -> (f32, f32) {
    ((5.0 * f32::from(sheet.zoom.unwrap_or(100)) / 100.0).round(), 3.0)
}

/// Height of the column headers with the outline's strip above them.
pub fn column_header_height(sheet: &ibis_doc_model::Sheet) -> f32 {
    COLUMN_HEADER_HEIGHT + outline_height(sheet)
}

/// Width of the row headers with the outline's strip at their left.
pub fn row_header_width(sheet: &ibis_doc_model::Sheet) -> f32 {
    row_number_width(sheet) + outline_width(sheet)
}

/// Width of the row headers dynamically sized based on sheet row count digits (Excel specification), without the outline.
pub fn row_number_width(sheet: &ibis_doc_model::Sheet) -> f32 {
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
