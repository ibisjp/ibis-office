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
use ibis_calc::input::{Typed, parse_imported};
use ibis_doc_model::sheet_command::Axis;
use ibis_doc_model::{
    CellEdit, CellInput, CellRange, CellStyle, Command, EmuRect, PutCell, Shape, ShapeKind, SheetCell, SheetCommand, SlideId, Validation,
};

use crate::shell::editor::SlideEditor;
use crate::ext::*;

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
pub struct Clip {
    pub(crate) slide: SlideId,
    pub(crate) range: CellRange,
    /// Each cell: its place in the range, the cell, its format, and original coordinates.
    pub(crate) cells: Vec<ClipCell>,
    pub cut: bool,
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
    /// Merged ranges inside the copied cells (relative to the clip).
    pub(crate) merges: Vec<CellRange>,
    /// The workbook the cells came from (its defined names), so that the names their formulas use go with them.
    pub(crate) source: Option<super::paste_names::NameSource>,
    /// Copied in another window: `slide` is not a sheet of this workbook.
    pub(crate) foreign: bool,
    /// The window the cells were copied or cut in.
    pub(crate) origin: Option<WeakEntity<SlideEditor>>,
    /// Cut in another window: pasting here also clears the cells there (Excel: 切り取り then 貼り付け between workbooks).
    pub(crate) cut_elsewhere: bool,
    /// Places (row, column in the clip) of cells that a dynamic array formula inside the clip spilled into: a paste that carries
    /// formulas leaves them to the pasted formula, which spills again (Excel); a paste of values brings them (#176).
    pub(crate) spill_ghosts: Vec<(u32, u32)>,
}

thread_local! {
    /// The HTML table a cut left on the clipboard once the cut was pasted: pasting again brings nothing (as Excel's spent clipboard).
    static SPENT_HTML: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    /// What was last copied in any workbook window (all windows run on this thread), with the window it came
    /// from: pasting into another window uses the formulas, formats and names, not only the text on the system
    /// clipboard.
    static SHARED_CLIP: std::cell::RefCell<Option<(usize, Clip)>> = const { std::cell::RefCell::new(None) };
}

/// For tests, which drive one window: what was copied counts as copied in another window from now on.
#[cfg(any(test, feature = "scenarios"))]
pub(crate) fn copied_in_another_window() {
    SHARED_CLIP.with(|c| {
        if let Some((owner, _)) = c.borrow_mut().as_mut() {
            *owner = 0;
        }
    });
}

impl Clip {
    /// The format a copied place pastes with: its cell's; a blank place that a merged range covers has the range's first cell's;
    /// a place with no cell has the default format (it clears what it lands on).
    pub(crate) fn style_at(&self, row: u32, column: u32) -> CellStyle {
        let own = self.cell_at(row, column);
        let blank = own.is_none_or(|s| s.cell.value.is_empty() && s.cell.formula.is_none());
        let source = if blank {
            self.merges.iter().find(|m| (m.first_row..=m.last_row).contains(&row) && (m.first_column..=m.last_column).contains(&column)).and_then(|m| self.cell_at(m.first_row, m.first_column)).or(own)
        } else {
            own
        };
        source.map_or_else(CellStyle::default, |s| s.style.clone())
    }

    pub(crate) fn cell_at(&self, row: u32, column: u32) -> Option<&ClipCell> {
        self.cells.iter().find(|c| c.row == row && c.column == column)
    }

    /// The copied cell that a paste puts at a place: with `carries_formulas`, a place that a copied dynamic array spilled into
    /// is left empty (the pasted formula fills it again; pasted as a value too, it would block the spill with #SPILL!).
    pub(crate) fn source_at(&self, row: u32, column: u32, carries_formulas: bool) -> Option<&ClipCell> {
        if carries_formulas && self.spill_ghosts.contains(&(row, column)) {
            return None;
        }
        self.cell_at(row, column)
    }
}

#[ibis_office_macros::editor_ext(Clipboard)]
impl SlideEditor {
    /// Pasting what another window copied: that window's clip (formulas, formats, names), when the system
    /// clipboard still holds its text, becomes this window's clip. The cells stay where they were (a cut
    /// pasted here is a copy).
    pub(crate) fn adopt_shared_clip(&mut self, text: &str, _cx: &mut Context<Self>) {
        if self.sheet_state().is_none_or(|s| s.clip.as_ref().is_some_and(|c| c.text == text)) {
            return;
        }
        let me = self as *const Self as usize;
        let found = SHARED_CLIP.with(|c| c.borrow().as_ref().filter(|(owner, clip)| *owner != me && clip.text == text).map(|(_, clip)| clip.clone()));
        let Some(mut clip) = found else { return };
        clip.foreign = true;
        clip.cut_elsewhere = clip.cut;
        clip.cut = false;
        clip.slide = SlideId(u64::MAX);
        if let Some(state) = self.sheet_state_mut() {
            state.clip = Some(clip);
        }
    }

    /// The commands that define the names `brought` (before the cells are put), as Excel does (`paste_names.rs`).
    pub(crate) fn brought_commands(&mut self, brought: super::paste_names::Brought) -> Vec<Command> {
        let mut commands = Vec::new();
        for (name, sheet, formula) in brought.names {
            // A name that refers to the copied workbook links it first (as typing such a formula does).
            if ibis_calc::formula::has_external_book(&formula) {
                commands.extend(self.link_new_books_commands(&format!("CHOOSE(1,{formula})")));
            }
            commands.push(Command::Sheet(SheetCommand::SetName { name, sheet, formula: Some(formula) }));
        }
        commands
    }

    pub(crate) fn copy_cells(&mut self, cut: bool, cx: &mut Context<Self>) {
        let shapes:Vec<_>=self.selected_shapes().into_iter().cloned().collect();
        if !shapes.is_empty() && self.copy_office_shapes(&shapes,cx){if cut{self.delete_selected(cx);}return;}
        self.commit_cell_edit(cx);
        // Cutting out of a pivot table changes it.
        if cut && !self.selected_ranges().is_empty() {
            let ranges = self.selected_ranges();
            if self.pivot_refuses_ranges(&ranges, cx) {
                return;
            }
        }
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
        // A cut takes a dynamic array whole; the spilled cells alone cannot be cut (Excel does nothing).
        if cut && unique_ranges.len() == 1 {
            match cut_range(sheet, unique_ranges[0]) {
                Some(r) => unique_ranges[0] = r,
                None => return,
            }
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
        // Merged ranges inside the copied cells (of the clip's own rows and columns), for the HTML table.
        let mut merges: Vec<CellRange> = Vec::new();
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
                for m in &sheet.merges {
                    if m.first_row >= range.first_row && m.last_row <= range.last_row && m.first_column >= range.first_column && m.last_column <= range.last_column {
                        merges.push(CellRange { first_row: m.first_row - range.first_row, first_column: m.first_column - range.first_column, last_row: m.last_row - range.first_row, last_column: m.last_column - range.first_column });
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

        let spill_ghosts = spill_ghosts_in(sheet, &cells);
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        let slide = self.current_slide;
        let owner = self as *const Self as usize;
        let source = self.presentation().workbook.as_ref().map(|w| super::paste_names::NameSource {
            names: w.names.clone(),
            sheet_names: w.sheets.iter().map(|s| s.name.clone()).collect(),
            sheet: w.sheets.iter().position(|s| s.slide == Some(slide)),
            book: self.file_path().as_ref().and_then(|p| Some((format!("{}/", p.parent()?.to_string_lossy().replace('\\', "/")), p.file_name()?.to_string_lossy().to_string()))),
        });
        if let Some(state) = self.sheet_state_mut() {
            let clip = Clip { slide, range: clip_range, cells, cut, text, validations, cell_images, col_widths, comments, merges: merges.clone(), source, foreign: false, origin: Some(cx.weak_entity()), cut_elsewhere: false, spill_ghosts };
            // The cells as an HTML table beside the text, as Excel puts them (formats, formulas and merged cells go with it).
            crate::native_shape_clipboard::write_html(&super::clipboard_html::write(&clip, &merges));
            // The clipboard holds something new, whatever a cut pasted before left on it.
            SPENT_HTML.with(|s| *s.borrow_mut() = None);
            // And as an XML Spreadsheet, which Excel for Windows pastes with the formulas and formats.
            if let Some(xml) = super::clipboard_xmlss::write(&clip, &merges) {
                crate::native_shape_clipboard::write_xml_spreadsheet(&xml);
            }
            // And as Excel's own embedded worksheet (the file format's formulas, formats and theme colours).
            if let Some(bytes) = super::clipboard_native::write(&clip, &merges) {
                crate::native_shape_clipboard::write_embed_source(&bytes);
            }
            SHARED_CLIP.with(|c| *c.borrow_mut() = Some((owner, clip.clone())));
            state.clip = Some(clip);
            state.copy_outline = true;
            cx.notify();
        }
    }

    /// Cells another program copied (Excel's embedded worksheet first, else its HTML table) that go with the clipboard's `text`.
    /// A cut that was pasted already (the clipboard is spent), and a table the text beside it contradicts, are not read.
    pub(crate) fn external_clip(&mut self, text: &str, slide: SlideId, dest: CellRange) -> Option<(Clip, Vec<CellRange>)> {
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let spent = SPENT_HTML.with(|s| s.borrow().clone());
        let html = crate::native_shape_clipboard::read_html();
        let html_spent = html.is_some() && html == spent;
        // Excel's embedded worksheet is exact, so it comes first; the HTML table is read when there is none.
        let native = if html_spent {
            None
        } else {
            crate::native_shape_clipboard::read_embed_source()
                .and_then(|bytes| super::clipboard_native::read(&bytes, slide, text))
                .filter(|(clip, _)| super::clipboard_native::matches_text(clip, text))
        };
        let found = match native {
            Some(found) => Some(found),
            None => html
                .filter(|_| !html_spent)
                .and_then(|html| super::clipboard_html::read(&html, date1904))
                .filter(|cells| super::clipboard_html::matches_text(cells, text))
                .map(|cells| super::clipboard_html::clip_of(cells, slide, text, (dest.first_row, dest.first_column))),
        };
        found.map(|(mut clip, merges)| {
            clip.merges = merges.clone();
            (clip, merges)
        })
    }

    /// Paste at the selected cell (`plain`: values only, no formulas or formats).
    pub fn paste_cells(&mut self, plain: bool, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        if !plain && self.paste_office_shapes(cx) { return; }
        let slide = self.current_slide;
        let item = cx.read_from_clipboard();
        let text = item.as_ref().and_then(|c| c.text()).unwrap_or_default();
        self.adopt_shared_clip(&text, cx);
        let clip = self.sheet_state().and_then(|s| s.clip.clone()).filter(|c| c.text == text);
        if clip.is_none() && text.is_empty() {
            // A drawing (SVG, a metafile) or a bitmap, named by what its bytes are.
            if let Some((name, bytes)) = crate::native_shape_clipboard::clipboard_picture(item.as_ref()) {
                if self.paste_sheet_picture(&name, bytes, cx).is_some() {
                    self.status = Some(crate::shell::i18n::text("_xlsx.pasted-the-image").into());
                    return cx.notify();
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

        // Cells another program copied as an HTML table (Excel's) come with their formats and formulas.
        let clip = match clip {
            Some(c) => Some(c),
            None if !plain => match self.external_clip(&text, slide, dest_ranges[0]) {
                Some((clip, _)) => Some(clip),
                None => None,
            },
            None => None,
        };
        // Names the pasted formulas use come with them (a question may be asked first).
        let mut brought = match clip.as_ref().filter(|c| !plain && !(c.cut && c.slide == slide)) {
            Some(c) => match self.paste_names_gate(c, super::paste_names_dialog::PasteAgain::Paste, cx) {
                Some(b) => Some(b),
                None => return,
            },
            None => None,
        };
        let mut commands = Vec::new();
        // Where copied cells land (貼り付けのオプション is offered there).
        let mut pasted: Option<CellRange> = None;
        // Cells cut in another window are cleared there once they are pasted here.
        let cut_elsewhere = clip.as_ref().filter(|c| c.cut_elsewhere && !plain).and_then(|c| Some((c.origin.clone()?, c.text.clone())));
        match clip {
            // A cut pasted on its own sheet moves the cells: formulas that
            // refer to them follow (Excel).
            Some(clip) if clip.cut && clip.slide == slide && !plain => {
                if dest_ranges.len() > 1 {
                    self.status = Some(crate::shell::i18n::text("_xlsx.cannot-use-on-multiple-selections").into());
                    return cx.notify();
                }
                let dest = dest_ranges[0];
                // Excel refuses a cut over merged cells that lie inside the landing place without being all of it or one of the
                // moved merges (a merge only partly under it is dissolved, as the whole one that it is).
                let (dr, dc) = (dest.first_row as i64 - clip.range.first_row as i64, dest.first_column as i64 - clip.range.first_column as i64);
                let landing = CellRange { first_row: dest.first_row, first_column: dest.first_column, last_row: (clip.range.last_row as i64 + dr) as u32, last_column: (clip.range.last_column as i64 + dc) as u32 };
                let within = |m: &CellRange, r: &CellRange| m.first_row >= r.first_row && m.last_row <= r.last_row && m.first_column >= r.first_column && m.last_column <= r.last_column;
                let refused = self.current_sheet().is_some_and(|(_, sheet)| {
                    let moved: Vec<CellRange> = clip.merges.iter().map(|m| CellRange { first_row: dest.first_row + m.first_row, first_column: dest.first_column + m.first_column, last_row: dest.first_row + m.last_row, last_column: dest.first_column + m.last_column }).collect();
                    sheet.merges.iter().any(|m| within(m, &landing) && *m != landing && !moved.contains(m) && !within(m, &clip.range))
                });
                if refused {
                    self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
                    return cx.notify();
                }
                commands.push(Command::Sheet(SheetCommand::MoveCells { slide, from: clip.range, row: dest.first_row, column: dest.first_column }));
                if let Some(state) = self.sheet_state_mut() {
                    SPENT_HTML.with(|s| *s.borrow_mut() = crate::native_shape_clipboard::read_html());
                    state.clip = None;
                }
            }
            Some(clip) => {
                let (h, w) = (clip.range.last_row - clip.range.first_row + 1, clip.range.last_column - clip.range.first_column + 1);
                // The merged ranges, comments and validation of the copied cells go with them (an ordinary paste is 形式を選択して
                // 貼り付け の すべて): where the copies land is worked out as there, and a destination that Excel refuses is refused.
                let mut shape_commands: Vec<Command> = Vec::new();
                let mut comment_commands: Vec<Command> = Vec::new();
                if !clip.cut && !plain {
                    let Some((tiles, rects)) = paste_tiles(&dest_ranges, h, w, false, false) else {
                        self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
                        return cx.notify();
                    };
                    let Some((_, here)) = self.current_sheet() else { return };
                    let Some(inside) = merged_under(here, &rects) else {
                        self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
                        return cx.notify();
                    };
                    shape_commands.extend(inside.into_iter().map(|range| Command::Sheet(SheetCommand::Unmerge { slide, range })));
                    for &Tile { row: tile_row, column: tile_column, .. } in &tiles {
                        for m in &clip.merges {
                            let range = CellRange { first_row: tile_row + m.first_row, first_column: tile_column + m.first_column, last_row: tile_row + m.last_row, last_column: tile_column + m.last_column };
                            shape_commands.push(Command::Sheet(SheetCommand::PasteMerge { slide, range }));
                        }
                        for r in 0..h {
                            for c in 0..w {
                                let (tr, tc) = (tile_row + r, tile_column + c);
                                let copied = clip.comments.iter().find(|(cr, cc, _)| *cr == r && *cc == c).map(|(_, _, comment)| comment);
                                if copied.is_some() || here.comment_at(tr, tc).is_some() {
                                    let comment = copied.map(|comment| ibis_doc_model::SheetComment { row: tr, column: tc, ..comment.clone() });
                                    comment_commands.push(Command::Sheet(SheetCommand::SetComment { slide, row: tr, column: tc, comment }));
                                }
                            }
                        }
                    }
                }
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
                        let source = clip.source_at(0, 0, !plain);
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
                                let style = if plain { None } else { Some(clip.style_at(0, 0)) };
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
                                        let source = clip.source_at(r, c, !plain);
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
                                        let style = if plain { None } else { Some(clip.style_at(r, c)) };
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
                        SPENT_HTML.with(|s| *s.borrow_mut() = crate::native_shape_clipboard::read_html());
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
                let names = match brought.take() {
                    Some(brought) => {
                        // The names an answer renamed are written new in the pasted formulas.
                        for c in cells.iter_mut() {
                            if let CellInput::Formula(f) = &mut c.input {
                                *f = super::paste_names::rename_in_formula(f, &brought.renames);
                            }
                        }
                        self.brought_commands(brought)
                    }
                    None => Vec::new(),
                };
                commands.insert(0, Command::Sheet(SheetCommand::PutCells { slide, cells }));
                // The ranges that go are dissolved and the merged ones made before the cells are put (what lies under a merge stays).
                commands.splice(0..0, shape_commands);
                commands.splice(0..0, names);
                commands.extend(comment_commands);
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
                            match parse_imported(field, date1904) {
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
        } else {
            if let Some(range) = pasted {
                // The rows not fixed by hand fit to the wrapping text pasted (in the paste's undo step).
                self.fit_wrapped_rows(range, cx);
                self.set_selected_ranges(&[range]);
                let undo_len = self.document.undo_len();
                self.wb_mut().last_paste = Some(LastPaste { slide, range, undo_len });
            }
            if let Some((origin, text)) = cut_elsewhere {
                // The cut is done: the clipboard is spent, and the cells are gone from the window they were cut in.
                SPENT_HTML.with(|s| *s.borrow_mut() = crate::native_shape_clipboard::read_html());
                SHARED_CLIP.with(|c| *c.borrow_mut() = None);
                if let Some(state) = self.sheet_state_mut() {
                    state.clip = None;
                }
                if let Some(source) = origin.upgrade() {
                    source.update(cx, |s, cx| s.clear_cut_cells(&text, cx));
                }
            }
        }
        cx.notify();
    }

    /// The cells cut here were pasted in another window: take them out (their formats and validation too), as a cut
    /// pasted on another sheet does, in one step of this workbook's undo.
    pub(crate) fn clear_cut_cells(&mut self, text: &str, cx: &mut Context<Self>) {
        let Some(clip) = self.sheet_state().and_then(|s| s.clip.clone()).filter(|c| c.cut && c.text == text) else { return };
        let slide = clip.slide;
        let edits: Vec<CellEdit> = clip.cells.iter().map(|c| CellEdit { row: c.orig_row, column: c.orig_column, input: CellInput::Clear }).collect();
        let mut commands = vec![Command::Sheet(SheetCommand::SetCells { slide, edits })];
        commands.push(Command::Sheet(SheetCommand::ClearFormats { slide, ranges: vec![clip.range] }));
        commands.push(Command::Sheet(SheetCommand::SetValidation { slide, ranges: vec![clip.range], rule: None }));
        if let Some(state) = self.sheet_state_mut() {
            state.clip = None;
        }
        self.run_sheet(commands, cx);
    }

    /// Take the formats off the selected cells (or selected/editing worksheet shape text).
    pub fn clear_cell_formats(&mut self, cx: &mut Context<Self>) {
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
pub enum PasteType {
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

#[ibis_office_macros::editor_ext(Clipboard2)]
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
        self.wb_mut().list_popup = Some(super::validation::ListPopup { menu, position, anchor: Anchor::TopLeft, _dismiss: dismiss });
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
        let Some(clip) = self.sheet_state().and_then(|s| s.clip.clone()).filter(|c| !c.foreign) else {
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

    /// The cells 形式を選択して貼り付け pastes: what this window copied while the clipboard still holds it, else the cells another
    /// program put there (Excel's worksheet or HTML table, with their formats), else the last copy here.
    pub(crate) fn paste_special_clip(&mut self, dest: CellRange, cx: &mut Context<Self>) -> Option<Clip> {
        let text = cx.read_from_clipboard().and_then(|c| c.text()).unwrap_or_default();
        self.adopt_shared_clip(&text, cx);
        let own = self.sheet_state().and_then(|s| s.clip.clone());
        if let Some(clip) = own.as_ref().filter(|c| c.text == text) {
            return Some(clip.clone());
        }
        let slide = self.current_slide;
        match self.external_clip(&text, slide, dest) {
            Some((clip, _)) => Some(clip),
            None => own,
        }
    }

    /// Paste what was copied here with custom options (PasteSpecialOptions).
    pub(crate) fn apply_paste_special_options(&mut self, options: PasteSpecialOptions, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((column, row)) = self.current_cell() else { return };
        let slide = self.current_slide;
        let first = self.selected_ranges().first().copied().unwrap_or(CellRange { first_row: row, last_row: row, first_column: column, last_column: column });
        let Some(clip) = self.paste_special_clip(first, cx) else {
            self.status = Some(crate::shell::i18n::text("_xlsx.to-use-paste-special-copy-cells-first").into());
            return cx.notify();
        };
        // Excel offers no 形式を選択して貼り付け after a cut (its Paste Special fails): only the plain paste moves the cells.
        if clip.cut {
            self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
            return cx.notify();
        }
        // Names the pasted formulas use come with them, for the kinds of paste that carry formulas (a question may be asked first).
        let brought = if options.operation == PasteOp::None
            && matches!(options.paste_type, PasteType::All | PasteType::Formulas | PasteType::AllExceptBorders | PasteType::FormulasAndNumberFormats)
        {
            match self.paste_names_gate(&clip, super::paste_names_dialog::PasteAgain::Special(options), cx) {
                Some(b) => Some(b),
                None => return,
            }
        } else {
            None
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

        // What each kind of paste carries (Excel's 形式を選択して貼り付け, measured: `test/editing/xlsx-paste-special-excel.tsv`).
        use PasteType as T;
        let carries_contents = matches!(options.paste_type, T::All | T::Formulas | T::Values | T::AllExceptBorders | T::FormulasAndNumberFormats | T::ValuesAndNumberFormats);
        let carries_formulas = matches!(options.paste_type, T::All | T::Formulas | T::AllExceptBorders | T::FormulasAndNumberFormats);
        let carries_formats = matches!(options.paste_type, T::All | T::Formats | T::AllExceptBorders);
        let carries_number_format = matches!(options.paste_type, T::FormulasAndNumberFormats | T::ValuesAndNumberFormats);
        let carries_merges = carries_formats;
        let carries_comments = matches!(options.paste_type, T::All | T::AllExceptBorders | T::Comments);
        let carries_validation = matches!(options.paste_type, T::All | T::AllExceptBorders | T::Validation);
        let operation = if carries_contents { options.operation } else { PasteOp::None };

        let mut commands = Vec::new();

        // Column widths: the copied columns' widths, over as many columns as the copies cover (Excel: the copies are tiled as for the
        // cells, a transposed one is as many columns wide as the copy had rows, and the widths are still those of the copied columns, in
        // turn; of several areas each is a single cell or a whole number of copies wide, else it refuses).
        if options.paste_type == T::ColumnWidths {
            let (th, tw) = if options.transpose { (w, h) } else { (h, w) };
            let mut spans: Vec<(u32, u32)> = Vec::new();
            for dest in &dest_ranges {
                let (sh, sw) = (dest.last_row - dest.first_row + 1, dest.last_column - dest.first_column + 1);
                let count = if h == 1 && w == 1 {
                    sw
                } else if dest_ranges.len() > 1 {
                    if sh == 1 && sw == 1 {
                        tw
                    } else if sw % tw == 0 {
                        sw
                    } else {
                        self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
                        return cx.notify();
                    }
                } else if sh % th == 0 && sw % tw == 0 {
                    sw
                } else {
                    tw
                };
                spans.push((dest.first_column, count));
            }
            for (first, count) in spans {
                for j in 0..count {
                    let width = clip.col_widths.iter().find(|(rel, _)| *rel == j % w).map(|(_, width)| *width).unwrap_or(sheet.default_column);
                    commands.push(Command::Sheet(SheetCommand::SetSize { slide, axis: Axis::Columns, first: first + j, last: first + j, size: width }));
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

        let in_merge = |r: u32, c: u32| clip.merges.iter().any(|m| (m.first_row..=m.last_row).contains(&r) && (m.first_column..=m.last_column).contains(&c));
        let number_text = |n: f64| ibis_calc::eval::number_to_text(n);
        let mut cells = Vec::new();
        let mut comment_commands = Vec::new();
        let mut validation_commands = Vec::new();
        let mut merge_commands = Vec::new();

        // Where the copies land: as Excel tiles them (the destination a whole number of copies wide and high takes that many; one
        // area that is not takes one; a destination of several areas takes one in each, or refuses when one is no copy's size).
        let (th, tw) = if options.transpose { (w, h) } else { (h, w) };
        let Some((mut tiles, mut rects)) = paste_tiles(&dest_ranges, th, tw, false, options.paste_type == T::Formats) else {
            self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
            return cx.notify();
        };
        // Merged ranges of the destination: one that a copy only partly covers refuses the paste; one it covers is dissolved by the
        // kinds of paste that bring merges of their own (a paste of values refuses any).
        let mut dissolve = Vec::new();
        let touches = |m: &CellRange, r: &CellRange| m.first_row <= r.last_row && m.last_row >= r.first_row && m.first_column <= r.last_column && m.last_column >= r.first_column;
        if operation != PasteOp::None {
            // With an operation Excel refuses only a paste of values, and only for a merge under the first copy; every other kind
            // goes on, and the kinds that bring merges dissolve the ranges they touch.
            if let Some(first) = tiles.first() {
                let first_copy = CellRange { first_row: first.row, first_column: first.column, last_row: first.row + th - 1, last_column: first.column + tw - 1 };
                if options.paste_type == T::Values && sheet.merges.iter().any(|m| touches(m, &first_copy)) {
                    self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
                    return cx.notify();
                }
                // A paste of values over merged cells elsewhere in the destination makes the one copy, not the tiles.
                if options.paste_type == T::Values && sheet.merges.iter().any(|m| rects.iter().any(|r| touches(m, r))) {
                    tiles.truncate(1);
                    rects = vec![first_copy];
                }
            }
            if carries_merges {
                dissolve = sheet.merges.iter().filter(|m| rects.iter().any(|r| touches(m, r))).copied().collect();
            }
        } else if options.paste_type != T::Comments && options.paste_type != T::Validation {
            match merged_under(sheet, &rects) {
                None => {
                    self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
                    return cx.notify();
                }
                Some(inside) => {
                    if options.paste_type == T::Values && !inside.is_empty() {
                        self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
                        return cx.notify();
                    }
                    if carries_merges {
                        dissolve = inside;
                    }
                }
            }
        }

        for tile in &tiles {
            let (origin_row, origin_column) = (tile.row, tile.column);
            // The place of the source cell (relative `r`, `c`) in the destination.
            let place = |r: u32, c: u32| if options.transpose { (origin_row + c, origin_column + r) } else { (origin_row + r, origin_column + c) };

            for clip_r in 0..h {
                for clip_c in 0..w {
                    let source = clip.source_at(clip_r, clip_c, carries_formulas);
                    let blank = source.is_none_or(|s| s.cell.value.is_empty() && s.cell.formula.is_none());
                    // 空白セルを無視する: a blank copied cell leaves its target as it is (all of it: contents, format, comment,
                    // validation), except a cell that a merged range covers.
                    if options.skip_blanks && blank && !in_merge(clip_r, clip_c) {
                        continue;
                    }
                    let (tr, tc) = place(clip_r, clip_c);
                    if tile.area.is_some_and(|a| !(a.first_row..=a.last_row).contains(&tr) || !(a.first_column..=a.last_column).contains(&tc)) {
                        continue;
                    }
                    let target = sheet.cell(tr, tc);
                    // A cell that a merged range covers has the format of the range's first cell.
                    let style_source = source.filter(|s| !(s.cell.value.is_empty() && s.cell.formula.is_none())).or_else(|| {
                        clip.merges.iter().find(|m| (m.first_row..=m.last_row).contains(&clip_r) && (m.first_column..=m.last_column).contains(&clip_c)).and_then(|m| clip.cell_at(m.first_row, m.first_column)).or(source)
                    });
                    let (orig_r, orig_c) = source.map_or((clip.range.first_row + clip_r, clip.range.first_column + clip_c), |s| (s.orig_row, s.orig_column));
                    let shifted = |f: &str| {
                        if options.transpose {
                            ibis_calc::formula::transpose(f, orig_r, orig_c, tr, tc)
                        } else {
                            ibis_calc::formula::shift(f, tr as i64 - orig_r as i64, tc as i64 - orig_c as i64)
                        }
                    };
                    let keep = || match target {
                        Some(t) => match &t.formula {
                            Some(f) => CellInput::Formula(f.clone()),
                            None if t.value.is_empty() => CellInput::Clear,
                            None => CellInput::Value(t.value.clone()),
                        },
                        None => CellInput::Clear,
                    };
                    let target_style = target.and_then(|t| sheet.styles.get(t.style as usize));

                    // Contents.
                    let input = if !carries_contents {
                        keep()
                    } else if operation == PasteOp::None {
                        match source {
                            Some(s) => match &s.cell.formula {
                                Some(f) if carries_formulas => CellInput::Formula(shifted(f)),
                                _ if s.cell.value.is_empty() => CellInput::Clear,
                                _ => CellInput::Value(s.cell.value.clone()),
                            },
                            None => CellInput::Clear,
                        }
                    } else {
                        // 演算: the destination is combined with the copied number (a blank copied cell counts as 0; text on either
                        // side leaves the destination as it is).
                        let symbol = match operation {
                            PasteOp::Add => '+',
                            PasteOp::Subtract => '-',
                            PasteOp::Multiply => '*',
                            PasteOp::Divide => '/',
                            PasteOp::None => unreachable!(),
                        };
                        let blank_target = target.is_none_or(|t| t.value.is_empty() && t.formula.is_none());
                        let copied = match source.map(|s| &s.cell.value) {
                            None | Some(ibis_doc_model::CellValue::Empty) => Some(0.0),
                            Some(ibis_doc_model::CellValue::Number(n)) => Some(*n),
                            _ => None,
                        };
                        let here = match target.map(|t| &t.value) {
                            None | Some(ibis_doc_model::CellValue::Empty) => Some(0.0),
                            Some(ibis_doc_model::CellValue::Number(n)) => Some(*n),
                            _ => None,
                        };
                        match (copied, here) {
                            // Nothing combined with nothing stays nothing.
                            _ if blank && blank_target => keep(),
                            (Some(b), Some(a)) => {
                                // A formula on either side is kept as a formula (the destination first, each in parentheses).
                                let copied_formula = source.and_then(|s| s.cell.formula.as_deref()).filter(|_| carries_formulas).map(shifted);
                                let here_formula = target.and_then(|t| t.formula.clone());
                                if copied_formula.is_some() || here_formula.is_some() {
                                    let left = match &here_formula {
                                        Some(f) => format!("({f})"),
                                        None => number_text(a),
                                    };
                                    let right = match &copied_formula {
                                        Some(f) => format!("({f})"),
                                        None => number_text(b),
                                    };
                                    CellInput::Formula(format!("{left}{symbol}{right}"))
                                } else {
                                    match symbol {
                                        '+' => CellInput::Value(ibis_doc_model::CellValue::Number(a + b)),
                                        '-' => CellInput::Value(ibis_doc_model::CellValue::Number(a - b)),
                                        '*' => CellInput::Value(ibis_doc_model::CellValue::Number(a * b)),
                                        _ if b == 0.0 => CellInput::Value(ibis_doc_model::CellValue::Error(ibis_doc_model::CellError::Div0)),
                                        _ => CellInput::Value(ibis_doc_model::CellValue::Number(a / b)),
                                    }
                                }
                            }
                            _ => keep(),
                        }
                    };

                    // Format.
                    let style = if carries_formats {
                        let mut st = style_source.map_or_else(CellStyle::default, |s| s.style.clone());
                        if options.paste_type == T::AllExceptBorders {
                            let (l, r, t, b, du, dd) = target_style.map_or((None, None, None, None, None, None), |ts| (ts.left.clone(), ts.right.clone(), ts.top.clone(), ts.bottom.clone(), ts.diagonal_up.clone(), ts.diagonal_down.clone()));
                            (st.left, st.right, st.top, st.bottom, st.diagonal_up, st.diagonal_down) = (l, r, t, b, du, dd);
                        }
                        Some(st)
                    } else if carries_number_format {
                        let mut st = target_style.cloned().unwrap_or_default();
                        st.number_format = style_source.map_or_else(|| "General".to_string(), |s| s.style.number_format.clone());
                        Some(st)
                    } else {
                        None
                    };
                    cells.push(PutCell { row: tr, column: tc, input, style });

                    // Comment and validation: the target takes the copied cell's (none removes its own).
                    let covered_alive = sheet.merges.iter().any(|m| (m.first_row..=m.last_row).contains(&tr) && (m.first_column..=m.last_column).contains(&tc) && (tr, tc) != (m.first_row, m.first_column) && !dissolve.contains(m));
                    if carries_comments && !(options.skip_blanks && blank) && !covered_alive {
                        let copied = clip.comments.iter().find(|(r, c, _)| *r == clip_r && *c == clip_c).map(|(_, _, c)| c);
                        if copied.is_some() || sheet.comment_at(tr, tc).is_some() {
                            let comment = copied.map(|c| {
                                let mut c = c.clone();
                                c.row = tr;
                                c.column = tc;
                                c
                            });
                            comment_commands.push(Command::Sheet(SheetCommand::SetComment { slide, row: tr, column: tc, comment }));
                        }
                    }
                    if carries_validation && !(options.skip_blanks && blank) && !covered_alive {
                        let cell_range = CellRange { first_row: tr, first_column: tc, last_row: tr, last_column: tc };
                        let covers = |rel: &CellRange| (rel.first_row..=rel.last_row).contains(&clip_r) && (rel.first_column..=rel.last_column).contains(&clip_c);
                        let rule = clip.validations.iter().find(|(rel, _)| covers(rel)).map(|(_, rule)| {
                            let mut rule = rule.clone();
                            rule.ranges = vec![cell_range];
                            rule
                        });
                        let had = sheet.validations.iter().any(|v| v.ranges.iter().any(|vr| (vr.first_row..=vr.last_row).contains(&tr) && (vr.first_column..=vr.last_column).contains(&tc)));
                        if rule.is_some() || had {
                            validation_commands.push(Command::Sheet(SheetCommand::SetValidation { slide, ranges: vec![cell_range], rule }));
                        }
                    }
                }
            }
            if carries_merges {
                for m in &clip.merges {
                    let (r0, c0) = place(m.first_row, m.first_column);
                    let (r1, c1) = place(m.last_row, m.last_column);
                    if tile.area.is_some_and(|a| r0.min(r1) < a.first_row || r0.max(r1) > a.last_row || c0.min(c1) < a.first_column || c0.max(c1) > a.last_column) {
                        continue;
                    }
                    let range = CellRange { first_row: r0.min(r1), first_column: c0.min(c1), last_row: r0.max(r1), last_column: c0.max(c1) };
                    merge_commands.push(Command::Sheet(SheetCommand::PasteMerge { slide, range }));
                }
            }
        }
        if cells.is_empty() && comment_commands.is_empty() && validation_commands.is_empty() && merge_commands.is_empty() {
            return;
        }
        if let Some(brought) = brought {
            for c in cells.iter_mut() {
                if let CellInput::Formula(f) = &mut c.input {
                    *f = super::paste_names::rename_in_formula(f, &brought.renames);
                }
            }
            commands = self.brought_commands(brought);
        }
        // The merged ranges first: what a destination cell under one holds stays there (as in Excel).
        commands.extend(dissolve.into_iter().map(|range| Command::Sheet(SheetCommand::Unmerge { slide, range })));
        commands.extend(merge_commands);
        if carries_contents || carries_formats || carries_number_format {
            commands.push(Command::Sheet(SheetCommand::PutCells { slide, cells }));
        }
        commands.extend(comment_commands);
        commands.extend(validation_commands);
        if let Err(err) = self.document.execute(commands) {
            log::warn!("paste special rejected: {err}");
            self.status = Some(crate::shell::i18n::text("_xlsx.can-t-paste-here").into());
        } else {
            // The cells pasted (the clip's size either way round, for a transposed one) and the selection they fill.
            let side = (clip.range.last_row - clip.range.first_row + 1).max(clip.range.last_column - clip.range.first_column + 1);
            for dest in &dest_ranges {
                let reach = CellRange {
                    last_row: dest.last_row.max(dest.first_row + side - 1),
                    last_column: dest.last_column.max(dest.first_column + side - 1),
                    ..*dest
                };
                self.fit_wrapped_rows(reach, cx);
            }
        }
        cx.notify();
    }
}

/// One copy of the copied cells at (`row`, `column`); when `area` is set, only what falls inside it is written (a format fills the
/// area with the copies cut off at its edge).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tile {
    pub(crate) row: u32,
    pub(crate) column: u32,
    pub(crate) area: Option<CellRange>,
}

/// Where copies of `h` x `w` cells land in the destination `areas`, and the ranges they cover: every cell for one cell copied, a
/// copy for each whole `h` x `w` the area holds, else one copy at the area's first cell (several areas: one at each single cell,
/// and `None`, as Excel refuses, when an area is neither a single cell nor a whole number of copies). `fill` (a paste of formats)
/// repeats the copy over a bigger area to its edge, whole copies or not, and refuses nothing.
/// The places (in the clip) of copied cells that a dynamic array formula spilled into (not those of a blocked formula): a paste
/// that carries formulas brings nothing for them. With the formula's cell in the clip too, the pasted formula spills again; with
/// the ghosts alone there is nothing to paste but values (Excel, #176).
fn spill_ghosts_in(sheet: &ibis_doc_model::Sheet, cells: &[ClipCell]) -> Vec<(u32, u32)> {
    let mut ghosts = Vec::new();
    for (&(ar, ac), &range) in &sheet.spills {
        let live = sheet.cell(ar, ac).is_some_and(|a| a.formula.is_some() && a.value != ibis_doc_model::CellValue::Error(ibis_doc_model::CellError::Spill));
        if !live {
            continue;
        }
        for c in cells {
            let inside = (range.first_row..=range.last_row).contains(&c.orig_row) && (range.first_column..=range.last_column).contains(&c.orig_column);
            if inside && c.cell.formula.is_none() && (c.orig_row, c.orig_column) != (ar, ac) {
                ghosts.push((c.row, c.column));
            }
        }
    }
    ghosts
}

/// What a cut of `range` takes: a dynamic array whose formula cell is in it goes whole (its spill range is added, Excel), and
/// `None` when the range holds only part of a spill whose formula cell is outside it (Excel refuses that cut).
fn cut_range(sheet: &ibis_doc_model::Sheet, mut range: CellRange) -> Option<CellRange> {
    let inside = |r: &CellRange, row: u32, column: u32| (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&column);
    loop {
        let before = range;
        for (&(ar, ac), &spill) in &sheet.spills {
            let live = sheet.cell(ar, ac).is_some_and(|a| a.formula.is_some() && a.value != ibis_doc_model::CellValue::Error(ibis_doc_model::CellError::Spill));
            if !live || spill.first_row == spill.last_row && spill.first_column == spill.last_column {
                continue;
            }
            let meets = spill.first_row <= range.last_row && spill.last_row >= range.first_row && spill.first_column <= range.last_column && spill.last_column >= range.first_column;
            if !meets {
                continue;
            }
            if inside(&range, ar, ac) {
                range = CellRange {
                    first_row: range.first_row.min(spill.first_row),
                    first_column: range.first_column.min(spill.first_column),
                    last_row: range.last_row.max(spill.last_row),
                    last_column: range.last_column.max(spill.last_column),
                };
            } else {
                return None;
            }
        }
        if range == before {
            return Some(range);
        }
    }
}

/// Whether the cell at (`row`, `column`) holds a value that a dynamic array formula spilled into it, the formula's own cell
/// lying inside `within` too (a copy or a fill of the whole array leaves such cells to the formula, which spills again).
pub(crate) fn is_spill_ghost_within(sheet: &ibis_doc_model::Sheet, within: CellRange, row: u32, column: u32) -> bool {
    if sheet.cell(row, column).is_some_and(|c| c.formula.is_some()) {
        return false;
    }
    sheet.spills.iter().any(|(&(ar, ac), range)| {
        (ar, ac) != (row, column)
            && (range.first_row..=range.last_row).contains(&row)
            && (range.first_column..=range.last_column).contains(&column)
            && (within.first_row..=within.last_row).contains(&ar)
            && (within.first_column..=within.last_column).contains(&ac)
            && sheet.cell(ar, ac).is_some_and(|a| a.formula.is_some() && a.value != ibis_doc_model::CellValue::Error(ibis_doc_model::CellError::Spill))
    })
}

pub(crate) fn paste_tiles(areas: &[CellRange], h: u32, w: u32, cut: bool, fill: bool) -> Option<(Vec<Tile>, Vec<CellRange>)> {
    let (mut out, mut regions) = (Vec::new(), Vec::new());
    for area in areas {
        let (sh, sw) = (area.last_row - area.first_row + 1, area.last_column - area.first_column + 1);
        let once = |out: &mut Vec<Tile>, regions: &mut Vec<CellRange>| {
            out.push(Tile { row: area.first_row, column: area.first_column, area: None });
            regions.push(CellRange { first_row: area.first_row, first_column: area.first_column, last_row: area.first_row + h - 1, last_column: area.first_column + w - 1 });
        };
        if h == 1 && w == 1 {
            out.extend((area.first_row..=area.last_row).flat_map(|r| (area.first_column..=area.last_column).map(move |c| Tile { row: r, column: c, area: None })));
            regions.push(*area);
        } else if fill && (sh > 1 || sw > 1) {
            for td in 0..sh.div_ceil(h) {
                for ta in 0..sw.div_ceil(w) {
                    out.push(Tile { row: area.first_row + td * h, column: area.first_column + ta * w, area: Some(*area) });
                }
            }
            regions.push(*area);
        } else if !cut && sh % h == 0 && sw % w == 0 {
            for td in 0..sh / h {
                for ta in 0..sw / w {
                    out.push(Tile { row: area.first_row + td * h, column: area.first_column + ta * w, area: None });
                }
            }
            regions.push(*area);
        } else if areas.len() == 1 || (sh == 1 && sw == 1) {
            once(&mut out, &mut regions);
        } else {
            return None;
        }
    }
    Some((out, regions))
}

/// The merged ranges of `sheet` that lie wholly inside one of `rects` (those a paste over `rects` replaces), or `None` when
/// one of them reaches out of the `rects` it touches (Excel: 結合したセルには行えません).
pub(crate) fn merged_under(sheet: &ibis_doc_model::Sheet, rects: &[CellRange]) -> Option<Vec<CellRange>> {
    let touches = |m: &CellRange, r: &CellRange| m.first_row <= r.last_row && m.last_row >= r.first_row && m.first_column <= r.last_column && m.last_column >= r.first_column;
    let inside = |m: &CellRange, r: &CellRange| m.first_row >= r.first_row && m.last_row <= r.last_row && m.first_column >= r.first_column && m.last_column <= r.last_column;
    let mut out = Vec::new();
    for m in &sheet.merges {
        let hit: Vec<&CellRange> = rects.iter().filter(|r| touches(m, r)).collect();
        if hit.is_empty() {
            continue;
        }
        if !hit.iter().any(|r| inside(m, r)) {
            return None;
        }
        out.push(*m);
    }
    Some(out)
}

#[ibis_office_macros::editor_ext(Clipboard3)]
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
