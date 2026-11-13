// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! フォントの置換 on a workbook (the panel is the shell's, `shell/replace_fonts.rs`): the fonts of the cells' formats
//! and the commands that replace one of them.

use ibis_doc_model::{CellRange, Command, ShapeKind, SheetCommand, StyleChange};

use crate::shell::replace_fonts::same_font;

/// The typefaces the cells' formats of the workbook use (a cell on the workbook's default font names none).
pub(crate) fn workbook_used_fonts(presentation: &ibis_doc_model::Presentation) -> Vec<String> {
    let mut fonts = std::collections::BTreeSet::new();
    for slide in &presentation.slides {
        for shape in &slide.shapes {
            if let ShapeKind::Sheet(sheet) = &shape.kind {
                for cell in sheet.cells.iter() {
                    if let Some(f) = sheet.styles.get(cell.style as usize).and_then(|s| s.font.as_deref()) {
                        let f = f.trim();
                        if !f.is_empty() {
                            fonts.insert(f.to_string());
                        }
                    }
                }
            }
        }
    }
    fonts.into_iter().collect()
}

/// The commands that set `replacement` as the font of the cells formatted in `target` (adjacent cells of a row in one range),
/// and how many cells they change.
pub(crate) fn workbook_replace_commands(presentation: &ibis_doc_model::Presentation, target: &str, replacement: &str) -> (Vec<Command>, usize) {
    let mut commands = Vec::new();
    let mut count = 0;
    if target.trim().is_empty() || replacement.trim().is_empty() || same_font(target, replacement) {
        return (commands, count);
    }
    for slide in &presentation.slides {
        for shape in &slide.shapes {
            let ShapeKind::Sheet(sheet) = &shape.kind else { continue };
            let mut ranges: Vec<CellRange> = Vec::new();
            for cell in sheet.cells.iter() {
                if !sheet.styles.get(cell.style as usize).and_then(|s| s.font.as_deref()).is_some_and(|f| same_font(f, target)) {
                    continue;
                }
                count += 1;
                match ranges.last_mut() {
                    Some(last) if last.first_row == cell.row && last.last_row == cell.row && last.last_column + 1 == cell.column => last.last_column = cell.column,
                    _ => ranges.push(CellRange { first_row: cell.row, first_column: cell.column, last_row: cell.row, last_column: cell.column }),
                }
            }
            if !ranges.is_empty() {
                commands.push(Command::Sheet(SheetCommand::SetFormat { slide: slide.id, ranges, change: StyleChange::Font(Some(replacement.trim().to_string())) }));
            }
        }
    }
    (commands, count)
}
