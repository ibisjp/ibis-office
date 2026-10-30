// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Cutting, copying and pasting cells. Copied cells go on the system
//! clipboard as text (tab-separated, as Excel puts them), and are kept
//! here with their formulas and formats: pasting what was copied here puts
//! the formulas (their relative references moved) and the formats; a cut
//! pasted on its sheet moves the cells, and formulas referring to them
//! follow (as in Excel); pasting other text reads it as typed.

use gpui_kit::*;
use ibis_calc::input::{Typed, parse_input};
use ibis_doc_model::sheet_command::Axis;
use ibis_doc_model::{
    CellEdit, CellInput, CellRange, CellStyle, Command, EmuRect, PutCell, Shape, ShapeKind, SheetCell, SheetCommand, SlideId, Validation,
};

use crate::shell::editor::SlideEditor;

/// A single cell inside a Clip.
#[derive(Clone, Debug)]
pub(crate) struct ClipCell {
    /// Row index within the clip (0-based virtual row).
    pub(crate) row: u32,
    /// Column index within the clip (0-based virtual column).
    pub(crate) column: u32,
    pub(crate) cell: SheetCell,
    pub(crate) style: CellStyle,
    /// Original row index in the source sheet (for formula shifting).
    pub(crate) orig_row: u32,
    /// Original column index in the source sheet (for formula shifting).
    pub(crate) orig_column: u32,
}

/// Cells copied or cut.
#[derive(Clone, Debug)]
pub(crate) struct Clip {
    pub(crate) slide: SlideId,
    pub(crate) range: CellRange,
    /// Each cell: its place in the range, the cell, its format, and original coordinates.
    pub(crate) cells: Vec<ClipCell>,
    pub(crate) cut: bool,
    /// The text put on the system clipboard (to know it is still ours).
    pub(crate) text: String,
    /// Validations in the copied range: (relative range within clip, rule).
    pub(crate) validations: Vec<(CellRange, Validation)>,
    /// In-cell images in the copied range: (relative row, relative column, Picture).
    pub(crate) cell_images: Vec<(u32, u32, ibis_doc_model::Picture)>,
    /// Column widths in EMU of the copied columns: (relative column, width).
    pub(crate) col_widths: Vec<(u32, ibis_doc_model::Emu)>,
    /// Comments/notes in the copied range: (relative row, relative column, SheetComment).
    pub(crate) comments: Vec<(u32, u32, ibis_doc_model::SheetComment)>,
}

impl Clip {
    pub(crate) fn cell_at(&self, row: u32, column: u32) -> Option<&ClipCell> {
        self.cells.iter().find(|c| c.row == row && c.column == column)
    }
}

impl SlideEditor {
    pub(crate) fn copy_cells(&mut self, cut: bool, cx: &mut Context<Self>) {
        let shapes:Vec<_>=self.selected_shapes().into_iter().cloned().collect();
        if !shapes.is_empty() && self.copy_office_shapes(&shapes,cx){if cut{self.delete_selected(cx);}return;}
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let ranges = self.selected_ranges();
        if ranges.is_empty() {
            return;
        }

        let mut unique_ranges: Vec<CellRange> = Vec::new();
        for r in ranges {
            if !unique_ranges.contains(&r) {
                unique_ranges.push(r);
            }
        }

        if cut && unique_ranges.len() > 1 {
            self.status = Some(crate::shell::i18n::text("_xlsx.cannot-use-on-multiple-selections").into());
            return cx.notify();
        }

        enum Alignment {
            Single(CellRange),
            Vertical { col_start: u32, col_end: u32, rows: Vec<u32> },
            Horizontal { row_start: u32, row_end: u32, cols: Vec<u32> },
        }

        let alignment = if unique_ranges.len() == 1 {
            Alignment::Single(unique_ranges[0])
        } else {
            let first = &unique_ranges[0];
            let same_cols = unique_ranges.iter().all(|r| r.first_column == first.first_column && r.last_column == first.last_column);
            let same_rows = unique_ranges.iter().all(|r| r.first_row == first.first_row && r.last_row == first.last_row);

            if same_cols {
                let mut rows = Vec::new();
                for r in &unique_ranges {
                    for row in r.first_row..=r.last_row {
                        if !rows.contains(&row) {
                            rows.push(row);
                        }
                    }
                }
                rows.sort_unstable();
                Alignment::Vertical {
                    col_start: first.first_column,
                    col_end: first.last_column,
                    rows,
                }
            } else if same_rows {
                let mut cols = Vec::new();
                for r in &unique_ranges {
                    for col in r.first_column..=r.last_column {
                        if !cols.contains(&col) {
                            cols.push(col);
                        }
                    }
                }
                cols.sort_unstable();
                Alignment::Horizontal {
                    row_start: first.first_row,
                    row_end: first.last_row,
                    cols,
                }
            } else {
                self.status = Some(crate::shell::i18n::text("_xlsx.cannot-use-on-multiple-selections").into());
                return cx.notify();
            }
        };

        let mut cells = Vec::new();
        let mut text = String::new();
        let mut validations = Vec::new();
        let mut cell_images = Vec::new();
        let mut col_widths = Vec::new();
        let mut comments = Vec::new();
        let xs = sheet.column_offsets();
        let ys = sheet.row_offsets();

        let clip_range = match alignment {
            Alignment::Single(range) => {
                for row in range.first_row..=range.last_row {
                    for column in range.first_column..=range.last_column {
                        let cell = sheet.cell(row, column);
                        if column > range.first_column {
                            text.push('\t');
                        }
                        if let Some(c) = cell {
                            text.push_str(&c.text);
                            cells.push(ClipCell {
                                row: row - range.first_row,
                                column: column - range.first_column,
                                cell: c.clone(),
                                style: sheet.styles[c.style as usize].clone(),
                                orig_row: row,
                                orig_column: column,
                            });
                        }
                    }
                    text.push('\n');
                }
                for v in &sheet.validations {
                    for vr in &v.ranges {
                        let r0 = vr.first_row.max(range.first_row);
                        let r1 = vr.last_row.min(range.last_row);
                        let c0 = vr.first_column.max(range.first_column);
                        let c1 = vr.last_column.min(range.last_column);
                        if r0 <= r1 && c0 <= c1 {
                            let rel = CellRange {
                                first_row: r0 - range.first_row,
                                first_column: c0 - range.first_column,
                                last_row: r1 - range.first_row,
                                last_column: c1 - range.first_column,
                            };
                            let mut rule = v.clone();
                            rule.ranges = vec![rel];
                            validations.push((rel, rule));
                        }
                    }
                }
                for img in &sheet.cell_images {
                    if (range.first_row..=range.last_row).contains(&img.row)
                        && (range.first_column..=range.last_column).contains(&img.column)
                    {
                        let col = img.column as usize;
                        let r = img.row as usize;
                        if col + 1 < xs.len() && r + 1 < ys.len() {
                            let (x0, y0) = (xs[col], ys[r]);
                            let (x1, y1) = (xs[col + 1], ys[r + 1]);
                            if let Some(shape) = self.slide().shapes.iter().find(|s| {
                                if let ShapeKind::Picture(_) = &s.kind {
                                    let cx_pos = s.frame.x + s.frame.width / 2;
                                    let cy_pos = s.frame.y + s.frame.height / 2;
                                    (x0..=x1).contains(&cx_pos) && (y0..=y1).contains(&cy_pos)
                                } else {
                                    false
                                }
                            }) {
                                if let ShapeKind::Picture(pic) = &shape.kind {
                                    cell_images.push((img.row - range.first_row, img.column - range.first_column, pic.clone()));
                                }
                            }
                        }
                    }
                }
                for col in range.first_column..=range.last_column {
                    let w = sheet.columns.get(col as usize).copied().filter(|&w| w > 0).unwrap_or(sheet.default_column);
                    col_widths.push((col - range.first_column, w));
                }
                for c in &sheet.comments {
                    if (range.first_row..=range.last_row).contains(&c.row) && (range.first_column..=range.last_column).contains(&c.column) {
                        comments.push((c.row - range.first_row, c.column - range.first_column, c.clone()));
                    }
                }
                range
            }
            Alignment::Vertical { col_start, col_end, rows } => {
                let h = rows.len() as u32;
                let w = col_end - col_start + 1;
                for (v_c, col) in (col_start..=col_end).enumerate() {
                    let w = sheet.columns.get(col as usize).copied().filter(|&w| w > 0).unwrap_or(sheet.default_column);
                    col_widths.push((v_c as u32, w));
                }
                for (v_r, &orig_r) in rows.iter().enumerate() {
                    let v_r = v_r as u32;
                    for (v_c, orig_c) in (col_start..=col_end).enumerate() {
                        let v_c = v_c as u32;
                        let cell = sheet.cell(orig_r, orig_c);
                        if v_c > 0 {
                            text.push('\t');
                        }
                        if let Some(c) = cell {
                            text.push_str(&c.text);
                            cells.push(ClipCell {
                                row: v_r,
                                column: v_c,
                                cell: c.clone(),
                                style: sheet.styles[c.style as usize].clone(),
                                orig_row: orig_r,
                                orig_column: orig_c,
                            });
                        }
                        if let Some(c) = sheet.comment_at(orig_r, orig_c) {
                            comments.push((v_r, v_c, c.clone()));
                        }
                        for v in &sheet.validations {
                            if v.ranges.iter().any(|vr| (vr.first_row..=vr.last_row).contains(&orig_r) && (vr.first_column..=vr.last_column).contains(&orig_c)) {
                                let rel = CellRange { first_row: v_r, last_row: v_r, first_column: v_c, last_column: v_c };
                                let mut rule = v.clone();
                                rule.ranges = vec![rel];
                                validations.push((rel, rule));
                            }
                        }
                        if let Some(img) = sheet.cell_images.iter().find(|img| img.row == orig_r && img.column == orig_c) {
                            let col = img.column as usize;
                            let r = img.row as usize;
                            if col + 1 < xs.len() && r + 1 < ys.len() {
                                let (x0, y0) = (xs[col], ys[r]);
                                let (x1, y1) = (xs[col + 1], ys[r + 1]);
                                if let Some(shape) = self.slide().shapes.iter().find(|s| {
                                    if let ShapeKind::Picture(_) = &s.kind {
                                        let cx_pos = s.frame.x + s.frame.width / 2;
                                        let cy_pos = s.frame.y + s.frame.height / 2;
                                        (x0..=x1).contains(&cx_pos) && (y0..=y1).contains(&cy_pos)
                                    } else {
                                        false
                                    }
                                }) {
                                    if let ShapeKind::Picture(pic) = &shape.kind {
                                        cell_images.push((v_r, v_c, pic.clone()));
                                    }
                                }
                            }
                        }
                    }
                    text.push('\n');
                }
                CellRange { first_row: 0, first_column: 0, last_row: h.saturating_sub(1), last_column: w.saturating_sub(1) }
            }
            Alignment::Horizontal { row_start, row_end, cols } => {
                let h = row_end - row_start + 1;
                let w = cols.len() as u32;
                for (v_c, &col) in cols.iter().enumerate() {
                    let w = sheet.columns.get(col as usize).copied().filter(|&w| w > 0).unwrap_or(sheet.default_column);
                    col_widths.push((v_c as u32, w));
                }
                for (v_r, orig_r) in (row_start..=row_end).enumerate() {
                    let v_r = v_r as u32;
                    for (v_c, &orig_c) in cols.iter().enumerate() {
                        let v_c = v_c as u32;
                        let cell = sheet.cell(orig_r, orig_c);
                        if v_c > 0 {
                            text.push('\t');
                        }
                        if let Some(c) = cell {
                            text.push_str(&c.text);
                            cells.push(ClipCell {
                                row: v_r,
                                column: v_c,
                                cell: c.clone(),
                                style: sheet.styles[c.style as usize].clone(),
                                orig_row: orig_r,
                                orig_column: orig_c,
                            });
                        }
                        if let Some(c) = sheet.comment_at(orig_r, orig_c) {
                            comments.push((v_r, v_c, c.clone()));
                        }
                        for v in &sheet.validations {
                            if v.ranges.iter().any(|vr| (vr.first_row..=vr.last_row).contains(&orig_r) && (vr.first_column..=vr.last_column).contains(&orig_c)) {
                                let rel = CellRange { first_row: v_r, last_row: v_r, first_column: v_c, last_column: v_c };
                                let mut rule = v.clone();
                                rule.ranges = vec![rel];
                                validations.push((rel, rule));
                            }
                        }
                        if let Some(img) = sheet.cell_images.iter().find(|img| img.row == orig_r && img.column == orig_c) {
                            let col = img.column as usize;
                            let r = img.row as usize;
                            if col + 1 < xs.len() && r + 1 < ys.len() {
                                let (x0, y0) = (xs[col], ys[r]);
                                let (x1, y1) = (xs[col + 1], ys[r + 1]);
                                if let Some(shape) = self.slide().shapes.iter().find(|s| {
                                    if let ShapeKind::Picture(_) = &s.kind {
                                        let cx_pos = s.frame.x + s.frame.width / 2;
                                        let cy_pos = s.frame.y + s.frame.height / 2;
                                        (x0..=x1).contains(&cx_pos) && (y0..=y1).contains(&cy_pos)
                                    } else {
                                        false
                                    }
                                }) {
                                    if let ShapeKind::Picture(pic) = &shape.kind {
                                        cell_images.push((v_r, v_c, pic.clone()));
                                    }
                                }
                            }
                        }
                    }
                    text.push('\n');
                }
                CellRange { first_row: 0, first_column: 0, last_row: h.saturating_sub(1), last_column: w.saturating_sub(1) }
            }
        };

        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        let slide = self.current_slide;
        if let Some(state) = self.sheet_state_mut() {
            state.clip = Some(Clip { slide, range: clip_range, cells, cut, text, validations, cell_images, col_widths, comments });
            cx.notify();
        }
    }

    /// Paste at the selected cell (`plain`: values only, no formulas or formats).
    pub(crate) fn paste_cells(&mut self, plain: bool, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        if !plain && self.paste_office_shapes(cx) { return; }
        let slide = self.current_slide;
        let item = cx.read_from_clipboard();
        let text = item.as_ref().and_then(|c| c.text()).unwrap_or_default();
        let clip = self.sheet_state().and_then(|s| s.clip.clone()).filter(|c| c.text == text);
        if clip.is_none() && text.is_empty() {
            if let Some(item) = &item {
                if let Some(ClipboardEntry::Image(image)) = item.entries().iter().find(|e| matches!(e, ClipboardEntry::Image(_))) {
                    if self.paste_sheet_picture("clipboard.png", image.bytes.clone(), cx).is_some() {
                        self.status = Some(crate::shell::i18n::text("_xlsx.pasted-the-image").into());
                        return cx.notify();
                    }
                }
            }
        }

        let Some((column,row))=self.current_cell() else{return};
        let selected_ranges = self.selected_ranges();
        let dest_ranges = if selected_ranges.is_empty() {
            vec![CellRange { first_row: row, last_row: row, first_column: column, last_column: column }]
        } else {
            let mut out = Vec::new();
            for r in selected_ranges {
                if !out.contains(&r) {
                    out.push(r);
                }
            }
            out
        };

        let mut commands = Vec::new();
        // Where copied cells land (貼り付けのオプション is offered there).
        let mut pasted: Option<CellRange> = None;
        match clip {
            // A cut pasted on its own sheet moves the cells: formulas that
            // refer to them follow (Excel).
            Some(clip) if clip.cut && clip.slide == slide && !plain => {
                if dest_ranges.len() > 1 {
                    self.status = Some(crate::shell::i18n::text("_xlsx.cannot-use-on-multiple-selections").into());
                    return cx.notify();
                }
                let dest = dest_ranges[0];
                commands.push(Command::Sheet(SheetCommand::MoveCells { slide, from: clip.range, row: dest.first_row, column: dest.first_column }));
                if let Some(state) = self.sheet_state_mut() {
                    state.clip = None;
                }
            }
            Some(clip) => {
                let (h, w) = (clip.range.last_row - clip.range.first_row + 1, clip.range.last_column - clip.range.first_column + 1);
                if !clip.cut && !plain && dest_ranges.len() == 1 {
                    let dest = dest_ranges[0];
                    let (sh, sw) = (dest.last_row - dest.first_row + 1, dest.last_column - dest.first_column + 1);
                    pasted = Some(if sh % h == 0 && sw % w == 0 {
                        dest
                    } else {
                        CellRange { first_row: dest.first_row, first_column: dest.first_column, last_row: dest.first_row + h - 1, last_column: dest.first_column + w - 1 }
                    });
                }
                let mut cells = Vec::new();
                for dest in &dest_ranges {
                    if h == 1 && w == 1 {
                        let source = clip.cell_at(0, 0);
                        for tr in dest.first_row..=dest.last_row {
                            for tc in dest.first_column..=dest.last_column {
                                let input = match source {
                                    Some(s) => match &s.cell.formula {
                                        Some(_) if plain => CellInput::Value(s.cell.value.clone()),
                                        Some(f) if clip.cut => CellInput::Formula(f.clone()),
                                        Some(f) => {
                                            let dr = tr as i64 - s.orig_row as i64;
                                            let dc = tc as i64 - s.orig_column as i64;
                                            CellInput::Formula(ibis_calc::formula::shift(f, dr, dc))
                                        }
                                        None if s.cell.value.is_empty() => CellInput::Clear,
                                        None => CellInput::Value(s.cell.value.clone()),
                                    },
                                    None => CellInput::Clear,
                                };
                                let style = if plain { None } else { source.map(|s| s.style.clone()) };
                                cells.push(PutCell { row: tr, column: tc, input, style });
                            }
                        }
                    } else {
                        let (sh, sw) = (dest.last_row - dest.first_row + 1, dest.last_column - dest.first_column + 1);
                        let (times_down, times_across) = if !clip.cut && sh % h == 0 && sw % w == 0 {
                            (sh / h, sw / w)
                        } else {
                            (1, 1)
                        };
                        for td in 0..times_down {
                            for ta in 0..times_across {
                                let base_row = dest.first_row + td * h;
                                let base_col = dest.first_column + ta * w;
                                for r in 0..h {
                                    for c in 0..w {
                                        let target_r = base_row + r;
                                        let target_c = base_col + c;
                                        let source = clip.cell_at(r, c);
                                        let input = match source {
                                            Some(s) => match &s.cell.formula {
                                                Some(_) if plain => CellInput::Value(s.cell.value.clone()),
                                                Some(f) if clip.cut => CellInput::Formula(f.clone()),
                                                Some(f) => {
                                                    let dr = target_r as i64 - s.orig_row as i64;
                                                    let dc = target_c as i64 - s.orig_column as i64;
                                                    CellInput::Formula(ibis_calc::formula::shift(f, dr, dc))
                                                }
                                                None if s.cell.value.is_empty() => CellInput::Clear,
                                                None => CellInput::Value(s.cell.value.clone()),
                                            },
                                            None => CellInput::Clear,
                                        };
                                        let style = if plain { None } else { source.map(|s| s.style.clone()) };
                                        cells.push(PutCell { row: target_r, column: target_c, input, style });
                                    }
                                }
                            }
                        }
                    }
                }
                if clip.cut {
                    let dest = dest_ranges[0];
                    let target = CellRange {
                        first_row: dest.first_row,
                        first_column: dest.first_column,
                        last_row: dest.first_row + clip.range.last_row - clip.range.first_row,
                        last_column: dest.first_column + clip.range.last_column - clip.range.first_column,
                    };
                    let inside = |r: u32, c: u32| clip.slide == slide && (target.first_row..=target.last_row).contains(&r) && (target.first_column..=target.last_column).contains(&c);
                    let edits: Vec<CellEdit> = clip
                        .cells
                        .iter()
                        .map(|c| (c.orig_row, c.orig_column))
                        .filter(|&(r, c)| !inside(r, c))
                        .map(|(r, c)| CellEdit { row: r, column: c, input: CellInput::Clear })
                        .collect();
                    if !edits.is_empty() {
                        commands.push(Command::Sheet(SheetCommand::SetCells { slide: clip.slide, edits }));
                        let ranges = vec![clip.range];
                        commands.push(Command::Sheet(SheetCommand::ClearFormats { slide: clip.slide, ranges }));
                        commands.push(Command::Sheet(SheetCommand::SetValidation { slide: clip.slide, ranges: vec![clip.range], rule: None }));
                    }
                    if !clip.cell_images.is_empty() {
                        if let Some(src_slide) = self.document.presentation().slide(clip.slide) {
                            if let Some(sh) = src_slide.shapes.iter().find(|s| matches!(s.kind, ShapeKind::Sheet(_))) {
                                if let ShapeKind::Sheet(sheet) = &sh.kind {
                                    let xs = sheet.column_offsets();
                                    let ys = sheet.row_offsets();
                                    for (rel_r, rel_c, _) in &clip.cell_images {
                                        let cut_r = clip.range.first_row + rel_r;
                                        let cut_c = clip.range.first_column + rel_c;
                                        if !inside(cut_r, cut_c) {
                                            let c_idx = cut_c as usize;
                                            let r_idx = cut_r as usize;
                                            if c_idx + 1 < xs.len() && r_idx + 1 < ys.len() {
                                                let (x0, y0) = (xs[c_idx], ys[r_idx]);
                                                let (x1, y1) = (xs[c_idx + 1], ys[r_idx + 1]);
                                                if let Some(shape) = src_slide.shapes.iter().find(|s| {
                                                    if matches!(s.kind, ShapeKind::Picture(_)) {
                                                        let cx_pos = s.frame.x + s.frame.width / 2;
                                                        let cy_pos = s.frame.y + s.frame.height / 2;
                                                        (x0..=x1).contains(&cx_pos) && (y0..=y1).contains(&cy_pos)
                                                    } else {
                                                        false
                                                    }
                                                }) {
                                                    commands.push(Command::RemoveShape { slide: clip.slide, shape: shape.id });
                                                    commands.push(Command::Sheet(SheetCommand::PlacePictureOverCells { slide: clip.slide, shape: shape.id, row: cut_r, column: cut_c }));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(state) = self.sheet_state_mut() {
                        state.clip = None;
                    }
                }
                if !plain {
                    for dest in &dest_ranges {
                        if h == 1 && w == 1 {
                            for tr in dest.first_row..=dest.last_row {
                                for tc in dest.first_column..=dest.last_column {
                                    for (_, rule) in &clip.validations {
                                        let target_range = CellRange { first_row: tr, first_column: tc, last_row: tr, last_column: tc };
                                        let mut new_rule = rule.clone();
                                        new_rule.ranges = vec![target_range];
                                        commands.push(Command::Sheet(SheetCommand::SetValidation {
                                            slide,
                                            ranges: vec![target_range],
                                            rule: Some(new_rule),
                                        }));
                                    }
                                    for (_, _, pic) in &clip.cell_images {
                                        let shape_id = self.document.ids().new_shape_id();
                                        let shape = Shape::new(shape_id, EmuRect::new(0, 0, 100, 100), ShapeKind::Picture(pic.clone()));
                                        let shape_idx = self.slide().shapes.len() + commands.len();
                                        commands.push(Command::InsertShape { slide, index: shape_idx, shape });
                                        commands.push(Command::Sheet(SheetCommand::PlacePictureInCell { slide, shape: shape_id, row: tr, column: tc }));
                                    }
                                }
                            }
                        } else {
                            let (sh, sw) = (dest.last_row - dest.first_row + 1, dest.last_column - dest.first_column + 1);
                            let (times_down, times_across) = if !clip.cut && sh % h == 0 && sw % w == 0 {
                                (sh / h, sw / w)
                            } else {
                                (1, 1)
                            };
                            for td in 0..times_down {
                                for ta in 0..times_across {
                                    let (tile_row, tile_col) = (dest.first_row + td * h, dest.first_column + ta * w);
                                    for (rel, rule) in &clip.validations {
                                        let target_range = CellRange {
                                            first_row: tile_row + rel.first_row,
                                            first_column: tile_col + rel.first_column,
                                            last_row: tile_row + rel.last_row,
                                            last_column: tile_col + rel.last_column,
                                        };
                                        let mut new_rule = rule.clone();
                                        new_rule.ranges = vec![target_range];
                                        commands.push(Command::Sheet(SheetCommand::SetValidation {
                                            slide,
                                            ranges: vec![target_range],
                                            rule: Some(new_rule),
                                        }));
                                    }
                                    for (rel_r, rel_c, pic) in &clip.cell_images {
                                        let target_row = tile_row + rel_r;
                                        let target_col = tile_col + rel_c;
                                        let shape_id = self.document.ids().new_shape_id();
                                        let shape = Shape::new(shape_id, EmuRect::new(0, 0, 100, 100), ShapeKind::Picture(pic.clone()));
                                        let shape_idx = self.slide().shapes.len() + commands.len();
                                        commands.push(Command::InsertShape { slide, index: shape_idx, shape });
                                        commands.push(Command::Sheet(SheetCommand::PlacePictureInCell { slide, shape: shape_id, row: target_row, column: target_col }));
                                    }
                                }
                            }
                        }
                    }
                }
                commands.insert(0, Command::Sheet(SheetCommand::PutCells { slide, cells }));
            }
            None => {
                let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
                // A cell formatted as Text (@) keeps pasted text as text: a date or a formula too (#13).
                let sheet_here = self.current_sheet().map(|(_, sheet)| sheet);
                let as_text = |r: u32, c: u32| sheet_here.is_some_and(|sheet| sheet.cell(r, c).is_some_and(|cell| sheet.styles[cell.style as usize].number_format == "@"));
                let mut grid: Vec<Vec<(CellInput, Option<String>, String)>> = Vec::new();
                for line in text.trim_end_matches(['\n', '\r']).split('\n') {
                    let mut row_items = Vec::new();
                    for field in line.trim_end_matches('\r').split('\t') {
                        let item = if field.is_empty() {
                            (CellInput::Clear, None, String::new())
                        } else {
                            match parse_input(field, date1904) {
                                Typed::Formula(f) => (CellInput::Formula(f), None, field.to_string()),
                                Typed::Value(v, format) => (CellInput::Value(v), format.map(|s| s.to_string()), field.to_string()),
                            }
                        };
                        row_items.push(item);
                    }
                    grid.push(row_items);
                }
                if grid.is_empty() {
                    return;
                }
                let th = grid.len() as u32;
                let tw = grid.first().map_or(0, |r| r.len()) as u32;
                let mut edits = Vec::new();
                let mut formats = Vec::new();

                for dest in &dest_ranges {
                    if th == 1 && tw == 1 {
                        let (input, format, raw) = &grid[0][0];
                        for r in dest.first_row..=dest.last_row {
                            for c in dest.first_column..=dest.last_column {
                                if as_text(r, c) {
                                    edits.push(CellEdit { row: r, column: c, input: CellInput::Value(ibis_doc_model::CellValue::Text(raw.clone())) });
                                    continue;
                                }
                                edits.push(CellEdit { row: r, column: c, input: input.clone() });
                                if let Some(code) = format {
                                    formats.push(((r, c), code.clone()));
                                }
                            }
                        }
                    } else {
                        for (r, row_items) in grid.iter().enumerate() {
                            for (c, (input, format, raw)) in row_items.iter().enumerate() {
                                let at = (dest.first_row + r as u32, dest.first_column + c as u32);
                                if !raw.is_empty() && as_text(at.0, at.1) {
                                    edits.push(CellEdit { row: at.0, column: at.1, input: CellInput::Value(ibis_doc_model::CellValue::Text(raw.clone())) });
                                    continue;
                                }
                                edits.push(CellEdit { row: at.0, column: at.1, input: input.clone() });
                                if let Some(code) = format {
                                    formats.push((at, code.clone()));
                                }
                            }
                        }
                    }
                }
                if edits.is_empty() {
                    return;
                }
                // Workbooks the pasted formulas name that the file does not link yet are linked in the same step.
                let external: Vec<String> = edits.iter().filter_map(|e| if let CellInput::Formula(f) = &e.input { Some(f.clone()) } else { None }).filter(|f| ibis_calc::formula::has_external_book(f)).collect();
                if !external.is_empty() {
                    let links = self.link_new_books_commands(&format!("CHOOSE(1,{})", external.join(",")));
                    commands.splice(0..0, links);
                }
                commands.push(Command::Sheet(SheetCommand::SetCells { slide, edits }));
                for ((r, c), code) in formats {
                    let range = CellRange { first_row: r, last_row: r, first_column: c, last_column: c };
                    commands.push(Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: ibis_doc_model::StyleChange::NumberFormat(code) }));
                }
            }
        }
        if let Err(err) = self.document.execute(commands) {
            log::warn!("paste rejected: {err}");
            self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
        } else if let Some(range) = pasted {
            self.set_selected_ranges(&[range]);
            let undo_len = self.document.undo_len();
            self.wb_mut().last_paste = Some(LastPaste { slide, range, undo_len });
        }
        cx.notify();
    }

    /// Take the formats off the selected cells (or selected/editing worksheet shape text).
    pub(crate) fn clear_cell_formats(&mut self, cx: &mut Context<Self>) {
        if self.clear_sheet_shape_formats(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some(range) = self.selected_range() else { return };
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::ClearFormats { slide, ranges: vec![range] })]) {
            log::warn!("clearing formats rejected: {err}");
        }
        cx.notify();
    }
}

/// 形式を選択して貼り付けの種類（Paste options）.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PasteType {
    /// Everything: formulas (or values), formats, and validation.
    All,
    /// Formulas (or values for non-formula cells), formats unchanged.
    Formulas,
    /// Values only (formulas converted to values), formats unchanged.
    Values,
    /// Formats only (styles), cell contents unchanged.
    Formats,
    /// Comments and notes only.
    Comments,
    /// Data validation rules only.
    Validation,
    /// All except borders (values/formulas and styles, keeping destination borders).
    AllExceptBorders,
    /// Column widths only (source column widths applied to destination columns).
    ColumnWidths,
    /// Formulas and number formats only.
    FormulasAndNumberFormats,
    /// Values and number formats only.
    ValuesAndNumberFormats,
}

/// 形式を選択して貼り付けの演算（Operation）.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum PasteOp {
    #[default]
    None,
    Add,
    Subtract,
    Multiply,
    Divide,
}

/// 形式を選択して貼り付けの実行オプション.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PasteSpecialOptions {
    pub(crate) paste_type: PasteType,
    pub(crate) operation: PasteOp,
    pub(crate) skip_blanks: bool,
    pub(crate) transpose: bool,
}

impl Default for PasteSpecialOptions {
    fn default() -> Self {
        Self {
            paste_type: PasteType::All,
            operation: PasteOp::None,
            skip_blanks: false,
            transpose: false,
        }
    }
}

/// 形式を選択して貼り付け: what of the copied cells goes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PasteSpecial {
    /// Everything: formulas (or values) and formats.
    All,
    Values,
    Formats,
    Formulas,
    /// データの入力規則.
    Validation,
    /// Rows and columns swapped (values, formulas and formats).
    Transpose,
    /// The copied numbers added to (subtracted from, …) the cells pasted on.
    Add,
    Subtract,
    Multiply,
    Divide,
    /// Formulas referring to the copied cells.
    Link,
    /// コメントとメモ.
    Comments,
    /// 罫線を除くすべて.
    AllExceptBorders,
    /// 列幅.
    ColumnWidths,
    /// 数式と数値の書式.
    FormulasAndNumberFormats,
    /// 値と数値の書式.
    ValuesAndNumberFormats,
}

/// The last paste of copied cells: where it went, and how many undo steps there were just after (the options button
/// is offered while nothing was done since).
#[derive(Clone, Copy, Debug)]
pub(crate) struct LastPaste {
    pub(crate) slide: ibis_doc_model::SlideId,
    pub(crate) range: CellRange,
    pub(crate) undo_len: usize,
}

impl SlideEditor {
    /// The range pasted last, while it is still what is selected and nothing was done after (where 貼り付けのオプション shows).
    pub(crate) fn paste_options_range(&self) -> Option<CellRange> {
        let last = self.sheet_state()?.last_paste?;
        (last.slide == self.current_slide && last.undo_len == self.document.undo_len() && self.selected_range() == Some(last.range)).then_some(last.range)
    }

    /// 貼り付けのオプション: the last paste done again as `kind` (in place of it).
    pub(crate) fn repaste_as(&mut self, kind: PasteSpecial, cx: &mut Context<Self>) {
        let Some(range) = self.paste_options_range() else { return };
        self.document.undo();
        self.set_selected_ranges(&[CellRange { last_row: range.first_row, last_column: range.first_column, ..range }]);
        self.paste_special(kind, cx);
        // The range where it landed this time (a transposed one has its sides swapped).
        let (h, w) = (range.last_row - range.first_row + 1, range.last_column - range.first_column + 1);
        let landed = if kind == PasteSpecial::Transpose {
            CellRange { last_row: range.first_row + w - 1, last_column: range.first_column + h - 1, ..range }
        } else {
            range
        };
        let (slide, undo_len) = (self.current_slide, self.document.undo_len());
        self.set_selected_ranges(&[landed]);
        self.wb_mut().last_paste = Some(LastPaste { slide, range: landed, undo_len });
        cx.notify();
    }

    /// Open 貼り付けのオプション at `position`.
    pub(crate) fn open_paste_options(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        use crate::shell::chrome::{MenuEntry, MenuItem, fill_menu};
        let item = |id: &'static str, label: &'static str, kind: PasteSpecial| -> MenuEntry {
            MenuItem::new(id, crate::shell::i18n::text(label))
                .on(move |this, _, cx| {
                    if let Some(s) = this.sheet_state_mut() {
                        s.list_popup = None;
                    }
                    this.repaste_as(kind, cx);
                })
                .into()
        };
        let entries = std::rc::Rc::new(vec![
            item("xlsx.paste-opt-all", "_xlsx.paste", PasteSpecial::All),
            item("xlsx.paste-opt-formulas", "_xlsx.formulas", PasteSpecial::Formulas),
            item("xlsx.paste-opt-formulas-numfmt", "xlsx.paste-formulas-and-number-formats", PasteSpecial::FormulasAndNumberFormats),
            item("xlsx.paste-opt-no-borders", "xlsx.paste-all-except-borders", PasteSpecial::AllExceptBorders),
            item("xlsx.paste-opt-widths", "xlsx.paste-column-widths", PasteSpecial::ColumnWidths),
            item("xlsx.paste-opt-transpose", "_xlsx.transpose", PasteSpecial::Transpose),
            MenuEntry::Separator,
            item("xlsx.paste-opt-values", "_xlsx.values", PasteSpecial::Values),
            item("xlsx.paste-opt-values-numfmt", "xlsx.paste-values-and-number-formats", PasteSpecial::ValuesAndNumberFormats),
            MenuEntry::Separator,
            item("xlsx.paste-opt-formats", "_xlsx.formats", PasteSpecial::Formats),
            item("xlsx.paste-opt-link", "_xlsx.paste-link", PasteSpecial::Link),
        ]);
        let editor = cx.entity().downgrade();
        let menu = gpui_kit::component::menu::PopupMenu::build(window, cx, move |menu, window, cx| fill_menu(menu.min_w(px(240.)), &entries, &editor, window, cx));
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &gpui::DismissEvent, window, cx| {
            if let Some(s) = this.sheet_state_mut() {
                s.list_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.wb_mut().list_popup = Some(super::validation::ListPopup { menu, position, _dismiss: dismiss });
        cx.notify();
    }

    /// Paste what was copied here, as `kind` says, at the selected cell.
    pub(crate) fn paste_special(&mut self, kind: PasteSpecial, cx: &mut Context<Self>) {
        if kind == PasteSpecial::Link {
            return self.paste_link_special(cx);
        }
        let skip_blanks = self.sheet_state().is_some_and(|s| s.panels.skip_blanks);
        let options = match kind {
            PasteSpecial::All => PasteSpecialOptions { paste_type: PasteType::All, operation: PasteOp::None, skip_blanks, transpose: false },
            PasteSpecial::Values => PasteSpecialOptions { paste_type: PasteType::Values, operation: PasteOp::None, skip_blanks, transpose: false },
            PasteSpecial::Formats => PasteSpecialOptions { paste_type: PasteType::Formats, operation: PasteOp::None, skip_blanks, transpose: false },
            PasteSpecial::Formulas => PasteSpecialOptions { paste_type: PasteType::Formulas, operation: PasteOp::None, skip_blanks, transpose: false },
            PasteSpecial::Validation => PasteSpecialOptions { paste_type: PasteType::Validation, operation: PasteOp::None, skip_blanks, transpose: false },
            PasteSpecial::Transpose => PasteSpecialOptions { paste_type: PasteType::All, operation: PasteOp::None, skip_blanks, transpose: true },
            PasteSpecial::Add => PasteSpecialOptions { paste_type: PasteType::Values, operation: PasteOp::Add, skip_blanks, transpose: false },
            PasteSpecial::Subtract => PasteSpecialOptions { paste_type: PasteType::Values, operation: PasteOp::Subtract, skip_blanks, transpose: false },
            PasteSpecial::Multiply => PasteSpecialOptions { paste_type: PasteType::Values, operation: PasteOp::Multiply, skip_blanks, transpose: false },
            PasteSpecial::Divide => PasteSpecialOptions { paste_type: PasteType::Values, operation: PasteOp::Divide, skip_blanks, transpose: false },
            PasteSpecial::Comments => PasteSpecialOptions { paste_type: PasteType::Comments, operation: PasteOp::None, skip_blanks, transpose: false },
            PasteSpecial::AllExceptBorders => PasteSpecialOptions { paste_type: PasteType::AllExceptBorders, operation: PasteOp::None, skip_blanks, transpose: false },
            PasteSpecial::ColumnWidths => PasteSpecialOptions { paste_type: PasteType::ColumnWidths, operation: PasteOp::None, skip_blanks, transpose: false },
            PasteSpecial::FormulasAndNumberFormats => PasteSpecialOptions { paste_type: PasteType::FormulasAndNumberFormats, operation: PasteOp::None, skip_blanks, transpose: false },
            PasteSpecial::ValuesAndNumberFormats => PasteSpecialOptions { paste_type: PasteType::ValuesAndNumberFormats, operation: PasteOp::None, skip_blanks, transpose: false },
            PasteSpecial::Link => unreachable!(),
        };
        self.apply_paste_special_options(options, cx);
    }

    /// Paste formulas referring to the copied cells (リンク貼り付け).
    pub(crate) fn paste_link_special(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((column, row)) = self.current_cell() else { return };
        let slide = self.current_slide;
        let Some(clip) = self.sheet_state().and_then(|s| s.clip.clone()) else {
            self.status = Some(crate::shell::i18n::text("_xlsx.to-use-paste-special-copy-cells-first").into());
            return cx.notify();
        };
        let source_sheet = self.presentation().workbook.as_ref().and_then(|w| w.sheets.iter().find(|s| s.slide == Some(clip.slide)).map(|s| s.name.clone()));
        let (h, w) = (clip.range.last_row - clip.range.first_row + 1, clip.range.last_column - clip.range.first_column + 1);

        let selected_ranges = self.selected_ranges();
        let dest_ranges = if selected_ranges.is_empty() {
            vec![CellRange { first_row: row, last_row: row, first_column: column, last_column: column }]
        } else {
            let mut out = Vec::new();
            for r in selected_ranges {
                if !out.contains(&r) {
                    out.push(r);
                }
            }
            out
        };

        let mut cells = Vec::new();
        for dest in &dest_ranges {
            let (target_h, target_w) = if h == 1 && w == 1 {
                (dest.last_row - dest.first_row + 1, dest.last_column - dest.first_column + 1)
            } else {
                (h, w)
            };
            for r_idx in 0..target_h {
                for c_idx in 0..target_w {
                    let (clip_r, clip_c) = if h == 1 && w == 1 { (0, 0) } else { (r_idx, c_idx) };
                    let source = clip.cell_at(clip_r, clip_c);
                    let (tr, tc) = (dest.first_row + r_idx, dest.first_column + c_idx);
                    let (orig_r, orig_c) = source.map_or((clip.range.first_row + clip_r, clip.range.first_column + clip_c), |s| (s.orig_row, s.orig_column));
                    let reference = ibis_doc_model::cell_name(orig_c, orig_r);
                    let formula = match (&source_sheet, clip.slide == slide) {
                        (Some(name), false) => format!("{}!{reference}", ibis_calc::formula::quote_sheet(name)),
                        _ => reference,
                    };
                    cells.push(PutCell { row: tr, column: tc, input: CellInput::Formula(formula), style: None });
                }
            }
        }
        if !cells.is_empty() {
            if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::PutCells { slide, cells })]) {
                log::warn!("paste link rejected: {err}");
                self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
            }
        }
        cx.notify();
    }

    /// Paste what was copied here with custom options (PasteSpecialOptions).
    pub(crate) fn apply_paste_special_options(&mut self, options: PasteSpecialOptions, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((column, row)) = self.current_cell() else { return };
        let slide = self.current_slide;
        let Some(clip) = self.sheet_state().and_then(|s| s.clip.clone()) else {
            self.status = Some(crate::shell::i18n::text("_xlsx.to-use-paste-special-copy-cells-first").into());
            return cx.notify();
        };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let (h, w) = (clip.range.last_row - clip.range.first_row + 1, clip.range.last_column - clip.range.first_column + 1);

        let selected_ranges = self.selected_ranges();
        let dest_ranges = if selected_ranges.is_empty() {
            vec![CellRange { first_row: row, last_row: row, first_column: column, last_column: column }]
        } else {
            let mut out = Vec::new();
            for r in selected_ranges {
                if !out.contains(&r) {
                    out.push(r);
                }
            }
            out
        };

        // 1. Column Widths
        if options.paste_type == PasteType::ColumnWidths {
            let mut commands = Vec::new();
            for dest in &dest_ranges {
                if h == 1 && w == 1 {
                    let col_width = clip.col_widths.first().map(|(_, w)| *w).unwrap_or(sheet.default_column);
                    for tc in dest.first_column..=dest.last_column {
                        commands.push(Command::Sheet(SheetCommand::SetSize {
                            slide,
                            axis: Axis::Columns,
                            first: tc,
                            last: tc,
                            size: col_width,
                        }));
                    }
                } else {
                    for &(rel_c, width) in &clip.col_widths {
                        let tc = dest.first_column + rel_c;
                        commands.push(Command::Sheet(SheetCommand::SetSize {
                            slide,
                            axis: Axis::Columns,
                            first: tc,
                            last: tc,
                            size: width,
                        }));
                    }
                }
            }
            if !commands.is_empty() {
                if let Err(err) = self.document.execute(commands) {
                    log::warn!("paste column widths rejected: {err}");
                    self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
                }
                cx.notify();
            }
            return;
        }

        // 2. Comments and Notes
        if options.paste_type == PasteType::Comments {
            let mut commands = Vec::new();
            for dest in &dest_ranges {
                for (rel_r, rel_c, comment) in &clip.comments {
                    let (tr, tc) = if options.transpose {
                        (dest.first_row + rel_c, dest.first_column + rel_r)
                    } else {
                        (dest.first_row + rel_r, dest.first_column + rel_c)
                    };
                    let mut new_c = comment.clone();
                    new_c.row = tr;
                    new_c.column = tc;
                    commands.push(Command::Sheet(SheetCommand::SetComment {
                        slide,
                        row: tr,
                        column: tc,
                        comment: Some(new_c),
                    }));
                }
            }
            if !commands.is_empty() {
                if let Err(err) = self.document.execute(commands) {
                    log::warn!("paste comments rejected: {err}");
                    self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
                }
                cx.notify();
            }
            return;
        }

        // 3. Validation Only
        if options.paste_type == PasteType::Validation {
            let mut commands = Vec::new();
            for dest in &dest_ranges {
                if h == 1 && w == 1 {
                    for tr in dest.first_row..=dest.last_row {
                        for tc in dest.first_column..=dest.last_column {
                            for (_, rule) in &clip.validations {
                                let target_range = CellRange { first_row: tr, first_column: tc, last_row: tr, last_column: tc };
                                let mut new_rule = rule.clone();
                                new_rule.ranges = vec![target_range];
                                commands.push(Command::Sheet(SheetCommand::SetValidation {
                                    slide,
                                    ranges: vec![target_range],
                                    rule: Some(new_rule),
                                }));
                            }
                        }
                    }
                } else {
                    for (rel, rule) in &clip.validations {
                        let target_range = if options.transpose {
                            CellRange {
                                first_row: dest.first_row + rel.first_column,
                                first_column: dest.first_column + rel.first_row,
                                last_row: dest.first_row + rel.last_column,
                                last_column: dest.first_column + rel.last_row,
                            }
                        } else {
                            CellRange {
                                first_row: dest.first_row + rel.first_row,
                                first_column: dest.first_column + rel.first_column,
                                last_row: dest.first_row + rel.last_row,
                                last_column: dest.first_column + rel.last_column,
                            }
                        };
                        let mut new_rule = rule.clone();
                        new_rule.ranges = vec![target_range];
                        commands.push(Command::Sheet(SheetCommand::SetValidation {
                            slide,
                            ranges: vec![target_range],
                            rule: Some(new_rule),
                        }));
                    }
                }
            }
            if !commands.is_empty() {
                if let Err(err) = self.document.execute(commands) {
                    log::warn!("paste validation rejected: {err}");
                    self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
                }
                cx.notify();
            }
            return;
        }

        // 4. Cells (Values, Formulas, Formats, All, AllExceptBorders, NumberFormats, and Operations)
        let mut cells = Vec::new();

        for dest in &dest_ranges {
            let (target_h, target_w) = if h == 1 && w == 1 {
                (dest.last_row - dest.first_row + 1, dest.last_column - dest.first_column + 1)
            } else {
                (h, w)
            };

            for r_idx in 0..target_h {
                for c_idx in 0..target_w {
                    let (clip_r, clip_c) = if h == 1 && w == 1 { (0, 0) } else { (r_idx, c_idx) };
                    let source = clip.cell_at(clip_r, clip_c);
                    // 空白セルを無視する: an empty copied cell leaves its target.
                    if options.skip_blanks && source.is_none_or(|s| s.cell.value.is_empty() && s.cell.formula.is_none()) {
                        continue;
                    }
                    let (tr, tc) = if options.transpose {
                        (dest.first_row + c_idx, dest.first_column + r_idx)
                    } else {
                        (dest.first_row + r_idx, dest.first_column + c_idx)
                    };
                    let target = sheet.cell(tr, tc);
                    let keep = || match target {
                        Some(t) => match &t.formula {
                            Some(f) => CellInput::Formula(f.clone()),
                            None if t.value.is_empty() => CellInput::Clear,
                            None => CellInput::Value(t.value.clone()),
                        },
                        None => CellInput::Clear,
                    };
                    let shifted = |f: &str| {
                        let (orig_r, orig_c) = source.map_or((clip.range.first_row + clip_r, clip.range.first_column + clip_c), |s| (s.orig_row, s.orig_column));
                        ibis_calc::formula::shift(f, tr as i64 - orig_r as i64, tc as i64 - orig_c as i64)
                    };

                    let (input, style) = if options.operation != PasteOp::None {
                        let Some(ibis_doc_model::CellValue::Number(b)) = source.map(|s| s.cell.value.clone()) else { continue };
                        let op_char = match options.operation {
                            PasteOp::Add => '+',
                            PasteOp::Subtract => '-',
                            PasteOp::Multiply => '*',
                            PasteOp::Divide => '/',
                            PasteOp::None => unreachable!(),
                        };
                        let number = ibis_calc::eval::number_to_text(b);
                        let input = match target {
                            Some(t) if t.formula.is_some() => CellInput::Formula(format!("({}){op_char}{number}", t.formula.as_deref().unwrap_or_default())),
                            Some(t) => match t.value {
                                ibis_doc_model::CellValue::Number(a) => {
                                    let v = match op_char {
                                        '+' => a + b,
                                        '-' => a - b,
                                        '*' => a * b,
                                        _ if b == 0.0 => {
                                            cells.push(PutCell { row: tr, column: tc, input: CellInput::Value(ibis_doc_model::CellValue::Error(ibis_doc_model::CellError::Div0)), style: None });
                                            continue;
                                        }
                                        _ => a / b,
                                    };
                                    CellInput::Value(ibis_doc_model::CellValue::Number(v))
                                }
                                ibis_doc_model::CellValue::Empty => CellInput::Value(ibis_doc_model::CellValue::Number(match op_char {
                                    '+' => b,
                                    '-' => -b,
                                    _ => 0.0,
                                })),
                                _ => continue,
                            },
                            None => CellInput::Value(ibis_doc_model::CellValue::Number(match op_char {
                                '+' => b,
                                '-' => -b,
                                _ => 0.0,
                            })),
                        };
                        (input, None)
                    } else {
                        match options.paste_type {
                            PasteType::Values => (
                                source.map_or(CellInput::Clear, |s| if s.cell.value.is_empty() { CellInput::Clear } else { CellInput::Value(s.cell.value.clone()) }),
                                None,
                            ),
                            PasteType::Formats => (keep(), Some(source.map_or_else(CellStyle::default, |s| s.style.clone()))),
                            PasteType::Formulas => {
                                let input = match source {
                                    Some(s) => match &s.cell.formula {
                                        Some(f) => CellInput::Formula(shifted(f)),
                                        None if s.cell.value.is_empty() => CellInput::Clear,
                                        None => CellInput::Value(s.cell.value.clone()),
                                    },
                                    None => CellInput::Clear,
                                };
                                (input, None)
                            }
                            PasteType::All => {
                                let input = match source {
                                    Some(s) => match &s.cell.formula {
                                        Some(f) => CellInput::Formula(shifted(f)),
                                        None if s.cell.value.is_empty() => CellInput::Clear,
                                        None => CellInput::Value(s.cell.value.clone()),
                                    },
                                    None => CellInput::Clear,
                                };
                                (input, Some(source.map_or_else(CellStyle::default, |s| s.style.clone())))
                            }
                            PasteType::AllExceptBorders => {
                                let input = match source {
                                    Some(s) => match &s.cell.formula {
                                        Some(f) => CellInput::Formula(shifted(f)),
                                        None if s.cell.value.is_empty() => CellInput::Clear,
                                        None => CellInput::Value(s.cell.value.clone()),
                                    },
                                    None => CellInput::Clear,
                                };
                                let style = source.map(|s| {
                                    let mut st = s.style.clone();
                                    if let Some(ts) = target.and_then(|t| sheet.styles.get(t.style as usize)) {
                                        st.left = ts.left.clone();
                                        st.right = ts.right.clone();
                                        st.top = ts.top.clone();
                                        st.bottom = ts.bottom.clone();
                                        st.diagonal_up = ts.diagonal_up.clone();
                                        st.diagonal_down = ts.diagonal_down.clone();
                                    } else {
                                        st.left = None;
                                        st.right = None;
                                        st.top = None;
                                        st.bottom = None;
                                        st.diagonal_up = None;
                                        st.diagonal_down = None;
                                    }
                                    st
                                });
                                (input, style)
                            }
                            PasteType::FormulasAndNumberFormats => {
                                let input = match source {
                                    Some(s) => match &s.cell.formula {
                                        Some(f) => CellInput::Formula(shifted(f)),
                                        None if s.cell.value.is_empty() => CellInput::Clear,
                                        None => CellInput::Value(s.cell.value.clone()),
                                    },
                                    None => CellInput::Clear,
                                };
                                let style = source.map(|s| {
                                    let mut st = target.and_then(|t| sheet.styles.get(t.style as usize)).cloned().unwrap_or_default();
                                    st.number_format = s.style.number_format.clone();
                                    st
                                });
                                (input, style)
                            }
                            PasteType::ValuesAndNumberFormats => {
                                let input = match source {
                                    Some(s) if !s.cell.value.is_empty() => CellInput::Value(s.cell.value.clone()),
                                    _ => CellInput::Clear,
                                };
                                let style = source.map(|s| {
                                    let mut st = target.and_then(|t| sheet.styles.get(t.style as usize)).cloned().unwrap_or_default();
                                    st.number_format = s.style.number_format.clone();
                                    st
                                });
                                (input, style)
                            }
                            PasteType::ColumnWidths | PasteType::Comments | PasteType::Validation => unreachable!(),
                        }
                    };
                    cells.push(PutCell { row: tr, column: tc, input, style });
                }
            }
        }
        if cells.is_empty() {
            return;
        }
        let mut commands = vec![Command::Sheet(SheetCommand::PutCells { slide, cells })];
        if options.paste_type == PasteType::All {
            for dest in &dest_ranges {
                if h == 1 && w == 1 {
                    for tr in dest.first_row..=dest.last_row {
                        for tc in dest.first_column..=dest.last_column {
                            for (_, rule) in &clip.validations {
                                let target_range = CellRange { first_row: tr, first_column: tc, last_row: tr, last_column: tc };
                                let mut new_rule = rule.clone();
                                new_rule.ranges = vec![target_range];
                                commands.push(Command::Sheet(SheetCommand::SetValidation {
                                    slide,
                                    ranges: vec![target_range],
                                    rule: Some(new_rule),
                                }));
                            }
                        }
                    }
                } else {
                    for (rel, rule) in &clip.validations {
                        let target_range = if options.transpose {
                            CellRange {
                                first_row: dest.first_row + rel.first_column,
                                first_column: dest.first_column + rel.first_row,
                                last_row: dest.first_row + rel.last_column,
                                last_column: dest.first_column + rel.last_row,
                            }
                        } else {
                            CellRange {
                                first_row: dest.first_row + rel.first_row,
                                first_column: dest.first_column + rel.first_column,
                                last_row: dest.first_row + rel.last_row,
                                last_column: dest.first_column + rel.last_column,
                            }
                        };
                        let mut new_rule = rule.clone();
                        new_rule.ranges = vec![target_range];
                        commands.push(Command::Sheet(SheetCommand::SetValidation {
                            slide,
                            ranges: vec![target_range],
                            rule: Some(new_rule),
                        }));
                    }
                }
            }
        }
        if let Err(err) = self.document.execute(commands) {
            log::warn!("paste special rejected: {err}");
            self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
        }
        cx.notify();
    }
}

impl SlideEditor {
    /// 書式のコピー/貼り付け: pick up the selected cells' formats (or, on
    /// again, give it up); the next range selected takes them.
    pub(crate) fn toggle_format_paint(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        if self.sheet_state_mut().and_then(|s| s.painter.take()).is_some() {
            return cx.notify();
        }
        let (Some(range), Some((_, sheet))) = (self.selected_range(), self.current_sheet()) else { return };
        let default = self.presentation().workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
        let mut styles = Vec::new();
        for row in range.first_row..=range.last_row {
            let line: Vec<CellStyle> = (range.first_column..=range.last_column).map(|column| sheet.cell(row, column).map_or(default.clone(), |c| sheet.styles[c.style as usize].clone())).collect();
            styles.push(line);
        }
        if let Some(state) = self.sheet_state_mut() {
            state.painter = Some(styles);
        }
        cx.notify();
    }

    /// The range selected after 書式のコピー: it takes the formats picked up
    /// (repeated over it), its values staying.
    pub(crate) fn finish_format_paint(&mut self, cx: &mut Context<Self>) {
        let Some(styles) = self.sheet_state_mut().and_then(|s| s.painter.take()) else { return };
        let (Some(range), Some((_, sheet))) = (self.selected_range(), self.current_sheet()) else { return };
        let (h, w) = (styles.len() as u32, styles.first().map_or(0, |l| l.len()) as u32);
        if h == 0 || w == 0 {
            return;
        }
        // A single cell clicked takes the whole block from there.
        let (last_row, last_column) = if range.first_row == range.last_row && range.first_column == range.last_column { (range.first_row + h - 1, range.first_column + w - 1) } else { (range.last_row, range.last_column) };
        let mut cells = Vec::new();
        for row in range.first_row..=last_row {
            for column in range.first_column..=last_column {
                let style = styles[((row - range.first_row) % h) as usize][((column - range.first_column) % w) as usize].clone();
                let input = match sheet.cell(row, column) {
                    Some(t) => match &t.formula {
                        Some(f) => CellInput::Formula(f.clone()),
                        None if t.value.is_empty() => CellInput::Clear,
                        None => CellInput::Value(t.value.clone()),
                    },
                    None => CellInput::Clear,
                };
                cells.push(PutCell { row, column, input, style: Some(style) });
            }
        }
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::PutCells { slide, cells })]) {
            log::warn!("format paint rejected: {err}");
        }
        cx.notify();
    }
}
