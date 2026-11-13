// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Copying and pasting cells as Excel's own embedded object (Mac: the pasteboard type `com.microsoft.Embed-Source`).
//!
//! What Excel puts there is an OLE compound file (MS-CFB) whose `Package` stream is a one-sheet `.xlsx` of the copied cells
//! (`tasks/doc/excel-native-clipboard-investigation.md`). Reading it gives the formulas, number formats, theme colours,
//! borders and merged cells as the file format says them, which the HTML table only approximates. Copied cells are written the
//! same way: a one-sheet workbook made with the ordinary saving, wrapped in a compound file of the class `Excel.Sheet.12`
//! (the streams `Package` and `\u{1}CompObj`).

use ibis_doc_model::{CellInput, CellRange, CellValue, Command, Document, SheetCommand, SlideId, sheet_command::PutCell};

use super::clipboard::{Clip, ClipCell};

/// The class of an Excel worksheet object (`Excel.Sheet.12`, {00020830-0000-0000-C000-000000000046}), as a directory entry stores it.
const EXCEL_SHEET_12: [u8; 16] = [0x30, 0x08, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46];

/// More cells than this are not written as an object (building the workbook costs time for every copy, and the HTML table and
/// the text still carry them).
const MAX_CELLS: usize = 50_000;

/// The `\u{1}CompObj` stream of an Excel worksheet object (MS-OLEDS 2.3.7): the header with the class, the user type, no
/// clipboard format, the ProgID, and the marker that no Unicode copies of the strings follow.
fn comp_obj() -> Vec<u8> {
    let mut out = vec![0x01, 0x00, 0xFE, 0xFF, 0x03, 0x0A, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF];
    out.extend_from_slice(&EXCEL_SHEET_12);
    for text in ["Microsoft Excel Worksheet"] {
        out.extend_from_slice(&(text.len() as u32 + 1).to_le_bytes());
        out.extend_from_slice(text.as_bytes());
        out.push(0);
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    let prog_id = "Excel.Sheet.12";
    out.extend_from_slice(&(prog_id.len() as u32 + 1).to_le_bytes());
    out.extend_from_slice(prog_id.as_bytes());
    out.push(0);
    out.extend_from_slice(&0x71B2_39F4u32.to_le_bytes());
    out.extend_from_slice(&[0; 12]);
    out
}

/// The compound file that holds the workbook `xlsx` as an embedded Excel worksheet.
pub(crate) fn embed_source(xlsx: &[u8]) -> Vec<u8> {
    use ibis_pptx::ibis_ooxml::cfb::{Item, write_with_clsid};
    write_with_clsid(&[Item::stream("Package", xlsx.to_vec()), Item::stream("\u{1}CompObj", comp_obj())], EXCEL_SHEET_12)
}

/// The workbook (`.xlsx`) inside an embedded object, if it is a compound file with a `Package` stream.
pub(crate) fn package_of(bytes: &[u8]) -> Option<Vec<u8>> {
    let package = ibis_pptx::ibis_ooxml::cfb::stream(bytes, "Package").ok()?;
    package.starts_with(b"PK").then_some(package)
}

/// A clip made of the first sheet of the object's workbook: the cells keep the places they have in that sheet (`orig_row` and
/// `orig_column`, so that the relative references of a formula move as they do when Excel pastes), and the clip's own rows and
/// columns count from the first cell. The merged ranges come back as ranges of the clip. `None` when it is not a workbook with
/// cells.
pub(crate) fn read(bytes: &[u8], slide: SlideId, text: &str) -> Option<(Clip, Vec<CellRange>)> {
    let presentation = ibis_pptx::read_xlsx(&package_of(bytes)?).ok()?;
    let sheet = presentation.slides.iter().find_map(|s| crate::shell::editor::sheet_of(s).map(|(_, sheet)| sheet.clone()))?;
    if sheet.cells.is_empty() {
        return None;
    }
    let top = sheet.cells.iter().map(|c| c.row).min()?;
    let left = sheet.cells.iter().map(|c| c.column).min()?;
    let bottom = sheet.cells.iter().map(|c| c.row).max()?;
    let right = sheet.cells.iter().map(|c| c.column).max()?;
    // A merged range reaches past its first cell, which is the only one that holds anything.
    let (mut bottom, mut right) = (bottom, right);
    let merges: Vec<CellRange> = sheet
        .merges
        .iter()
        .filter(|m| m.first_row >= top && m.first_column >= left && m.first_row <= bottom && m.first_column <= right)
        .map(|m| CellRange { first_row: m.first_row - top, first_column: m.first_column - left, last_row: m.last_row - top, last_column: m.last_column - left })
        .collect();
    for m in &merges {
        bottom = bottom.max(m.last_row + top);
        right = right.max(m.last_column + left);
    }
    let cells: Vec<ClipCell> = sheet
        .cells
        .iter()
        .map(|c| ClipCell {
            row: c.row - top,
            column: c.column - left,
            cell: c.clone(),
            style: sheet.styles.get(c.style as usize).cloned().unwrap_or_default(),
            orig_row: c.row,
            orig_column: c.column,
        })
        .collect();
    let col_widths = (left..=right).map(|col| (col - left, sheet.columns.get(col as usize).copied().filter(|&w| w > 0).unwrap_or(sheet.default_column))).collect();
    // Comments and validation rules inside the cells, as places of the clip.
    let comments = sheet
        .comments
        .iter()
        .filter(|c| c.row >= top && c.row <= bottom && c.column >= left && c.column <= right)
        .map(|c| (c.row - top, c.column - left, c.clone()))
        .collect();
    let mut validations = Vec::new();
    for v in &sheet.validations {
        for vr in &v.ranges {
            let (r0, r1, c0, c1) = (vr.first_row.max(top), vr.last_row.min(bottom), vr.first_column.max(left), vr.last_column.min(right));
            if r0 <= r1 && c0 <= c1 {
                let rel = CellRange { first_row: r0 - top, first_column: c0 - left, last_row: r1 - top, last_column: c1 - left };
                let mut rule = v.clone();
                rule.ranges = vec![rel];
                validations.push((rel, rule));
            }
        }
    }
    let clip = Clip {
        slide,
        range: CellRange { first_row: 0, first_column: 0, last_row: bottom - top, last_column: right - left },
        cells,
        cut: false,
        text: text.to_string(),
        validations,
        cell_images: Vec::new(),
        col_widths,
        comments,
        merges: merges.clone(),
        source: None,
        foreign: false,
        origin: None,
        cut_elsewhere: false,
        spill_ghosts: Vec::new(),
    };
    Some((clip, merges))
}

/// Whether the clip shows the text the clipboard holds beside it (the same non-empty fields in the same order): an object left
/// on the clipboard by an earlier copy, beside text that something else put there, is not what was copied.
pub(crate) fn matches_text(clip: &Clip, text: &str) -> bool {
    let fields = |s: &str| -> Vec<String> { s.split(['\t', '\n', '\r']).map(|f| f.trim().trim_matches('"').trim().to_string()).filter(|f| !f.is_empty()).collect() };
    let mut ordered: Vec<&ClipCell> = clip.cells.iter().collect();
    ordered.sort_by_key(|c| (c.row, c.column));
    let shown: Vec<String> = ordered.iter().flat_map(|c| fields(&c.cell.text)).collect();
    shown == fields(text)
}

/// The embedded object of copied cells: a one-sheet workbook, saved by the ordinary saving, in a compound file. The cells keep
/// their own places (`orig_row`, `orig_column`) so that their formulas still mean what they did. `None` when there are no cells,
/// too many, or the workbook could not be made.
pub(crate) fn write(clip: &Clip, merges: &[CellRange]) -> Option<Vec<u8>> {
    if clip.cells.is_empty() || clip.cells.len() > MAX_CELLS {
        return None;
    }
    let opened = ibis_pptx::open(ibis_pptx::blank_file(ibis_pptx::Kind::Workbook).ok()?).ok()?;
    let slide = opened.presentation.slides.iter().find(|s| crate::shell::editor::sheet_of(s).is_some())?.id;
    let default_column = crate::shell::editor::sheet_of(opened.presentation.slides.iter().find(|s| s.id == slide)?)?.1.default_column;
    // Where the clip's first row and column were.
    let first = clip.cells.first()?;
    let (top, left) = (first.orig_row - first.row, first.orig_column - first.column);
    let mut cells = Vec::new();
    for c in &clip.cells {
        let input = match &c.cell.formula {
            Some(f) => CellInput::Formula(f.clone()),
            None if matches!(c.cell.value, CellValue::Empty) => CellInput::Clear,
            None => CellInput::Value(c.cell.value.clone()),
        };
        cells.push(PutCell { row: c.orig_row, column: c.orig_column, input, style: Some(c.style.clone()) });
    }
    let mut commands = vec![Command::Sheet(SheetCommand::PutCells { slide, cells })];
    for (rel, width) in &clip.col_widths {
        if *width != default_column {
            let column = left + rel;
            commands.push(Command::Sheet(SheetCommand::SetSize { slide, axis: ibis_doc_model::sheet_command::Axis::Columns, first: column, last: column, size: *width }));
        }
    }
    for (rel_r, rel_c, comment) in &clip.comments {
        let (row, column) = (top + rel_r, left + rel_c);
        let mut comment = comment.clone();
        comment.row = row;
        comment.column = column;
        commands.push(Command::Sheet(SheetCommand::SetComment { slide, row, column, comment: Some(comment) }));
    }
    for (rel, rule) in &clip.validations {
        let target = CellRange { first_row: top + rel.first_row, first_column: left + rel.first_column, last_row: top + rel.last_row, last_column: left + rel.last_column };
        let mut rule = rule.clone();
        rule.ranges = vec![target];
        commands.push(Command::Sheet(SheetCommand::SetValidation { slide, ranges: vec![target], rule: Some(rule) }));
    }
    for m in merges {
        let range = CellRange { first_row: top + m.first_row, first_column: left + m.first_column, last_row: top + m.last_row, last_column: left + m.last_column };
        commands.push(Command::Sheet(SheetCommand::PasteMerge { slide, range }));
    }
    let mut document = Document::new(opened.presentation.clone());
    document.execute(commands).ok()?;
    let xlsx = opened.source.save(document.presentation(), ibis_pptx::SaveOptions::default()).ok()?;
    Some(embed_source(&xlsx))
}
