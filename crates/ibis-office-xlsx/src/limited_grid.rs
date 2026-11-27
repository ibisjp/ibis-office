// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 限られたグリッド表示 (#273): with the setting `xlsx.grid.limited` on, the sheet shows the used range and a margin of rows and
//! columns, not Excel's whole grid, and the grid grows when the selection comes near its edge (arrow keys, Go To, a click, typing
//! into the last cell). Off (the default), nothing here is used.

use ibis_doc_model::{CellRange, Emu, Sheet};

/// The rows kept free below the used range, and below the selection's last row.
pub(crate) const ROW_MARGIN: u32 = 20;
/// The columns kept free right of the used range, and right of the selection's last column.
pub(crate) const COLUMN_MARGIN: u32 = 5;
/// The grid never shows fewer rows and columns than these (a new, empty sheet is not a blank page).
pub(crate) const MIN_ROWS: u32 = 40;
pub(crate) const MIN_COLUMNS: u32 = 12;

/// The columns and rows of the grid to show (counts), for the used range of `sheet` and the selection.
pub(crate) fn extent(sheet: &Sheet, selection: Option<CellRange>) -> (u32, u32) {
    let (mut columns, mut rows) = (0u32, 0u32);
    for cell in sheet.cells.iter() {
        columns = columns.max(cell.column + 1);
        rows = rows.max(cell.row + 1);
    }
    for m in &sheet.merges {
        columns = columns.max(m.last_column + 1);
        rows = rows.max(m.last_row + 1);
    }
    let mut columns = columns + COLUMN_MARGIN;
    let mut rows = rows + ROW_MARGIN;
    if let Some(r) = selection {
        columns = columns.max(r.last_column + 1 + COLUMN_MARGIN);
        rows = rows.max(r.last_row + 1 + ROW_MARGIN);
    }
    (columns.clamp(MIN_COLUMNS, 16_384), rows.clamp(MIN_ROWS, 1_048_576))
}

/// The offset of the edge before line `n` (the sum of the first `n` sizes, then the default size for each one past the list).
pub(crate) fn offset_at(sizes: &[Emu], default: Emu, n: u32) -> Emu {
    let n = n as usize;
    let listed: Emu = sizes.iter().take(n).sum();
    listed + n.saturating_sub(sizes.len()) as Emu * default
}

/// The size in pixels of the grid to show for `sheet` at `scale` (pixels per EMU), if this window limits it (`Resources::grid_limit`).
pub(crate) fn limit_px(sheet: &Sheet, scale: f32, limit: Option<(u32, u32)>) -> Option<(f32, f32)> {
    let (columns, rows) = limit?;
    Some((offset_at(&sheet.columns, sheet.default_column, columns) as f32 * scale, offset_at(&sheet.rows, sheet.default_row, rows) as f32 * scale))
}
