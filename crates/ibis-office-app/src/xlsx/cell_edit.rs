// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Typing into a cell, as in Excel: typing on a selected cell replaces
//! its content ("Enter" mode, where the arrows finish the entry and move),
//! F2 or a double-click edits what is there ("Edit" mode, where ← and →
//! move the caret). Enter, Tab and the arrows finish the entry and move;
//! Esc gives it up. Japanese input goes through the shell's input hooks
//! (`shell/input.rs`), which call the functions here.
//!
//! Finishing an entry reads it as Excel does (`ibis_calc::input`) and
//! makes it one editing step: the value or formula, and the number format
//! Excel gives a General cell for what was typed (`1,234`, `50%`,
//! `2026/9/30`).

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::*;
use ibis_calc::input::{Typed, parse_input};
use ibis_doc_model::{CellRun, CellEdit as ModelEdit, CellInput, CellRange, CellValue, Command, SheetCommand, SlideId, StyleChange};

use crate::shell::editor::SlideEditor;

/// How the arrows act while typing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EditMode {
    /// Typing replaced the content: the arrows finish the entry and move.
    Enter,
    /// F2 or a double-click: ← and → move the caret.
    Edit,
}

/// A cell being typed into.
#[derive(Clone, Debug)]
pub(crate) struct CellEdit {
    pub(crate) slide: SlideId,
    pub(crate) column: u32,
    pub(crate) row: u32,
    pub(crate) text: String,
    /// The caret, in UTF-16 units.
    pub(crate) caret: usize,
    /// Text the input method is composing, in UTF-16 units.
    pub(crate) marked: Option<Range<usize>>,
    pub(crate) mode: EditMode,
    /// The other end of the selected text (UTF-16 units), the caret being one.
    pub(crate) anchor: Option<usize>,
    /// The text's parts in their own fonts (rich text), their texts together `text`.
    pub(crate) runs: Option<Vec<CellRun>>,
    /// A reference put in by clicking a cell: its byte span in `text` and
    /// the cell clicked (a drag makes it a range from there).
    pub(crate) pointed: Option<(usize, usize, (u32, u32))>,
    pub(crate) reference_drag:bool,
    /// Where the text is on screen (set when painted), for the input method.
    pub(crate) rect: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Pixels per point of the text on screen (set when painted), to find the
    /// character under the pointer.
    pub(crate) pixels_per_point: Rc<Cell<f32>>,
    /// The text's lines as painted (set when painted): where each glyph is, for the caret and for finding the character under the pointer.
    pub(crate) layout: Rc<std::cell::RefCell<Option<Rc<super::sheet_view::EditLayout>>>>,
    /// Pressed on the text and dragging: the position (UTF-16 units) it began at.
    pub(crate) dragging: Option<usize>,
    /// Whether the press being dragged began in the formula bar (not in the cell).
    pub(crate) dragging_in_bar: bool,
    /// Font changes asked for with the caret alone (⌘B…): they go to the text typed next.
    pub(crate) pending: Option<PendingFormat>,
    /// How far the text is scrolled up in the cell's box (pixels), when its lines go below the highest box (409.5 pt).
    pub(crate) scroll_y: Rc<Cell<f32>>,
    /// The caret the scroll last followed (UTF-16 units): when the caret moves, the box scrolls to show it.
    pub(crate) followed_caret: Rc<Cell<Option<usize>>>,
}

/// Font changes for the next typed text (Excel's ⌘B with the caret alone),
/// with the cell's style they are made against.
#[derive(Clone, Debug)]
pub(crate) struct PendingFormat {
    pub(crate) changes: Vec<StyleChange>,
    pub(crate) style: ibis_doc_model::CellStyle,
}

/// `runs` with the bytes `b0..b1` of their text replaced by `text`, which
/// takes the font of the part it lands in (of the one before it at a seam).
pub(crate) fn runs_replace(runs: &mut Vec<CellRun>, b0: usize, b1: usize, text: &str) {
    let mut out: Vec<CellRun> = Vec::new();
    let mut at = 0usize;
    let mut inserted = false;
    let mut last_format: Option<CellRun> = None;
    for run in runs.iter() {
        let (start, end) = (at, at + run.text.len());
        at = end;
        let format = CellRun { text: String::new(), ..run.clone() };
        let before = &run.text[..b0.clamp(start, end) - start];
        let after = &run.text[b1.clamp(start, end) - start..];
        if !before.is_empty() {
            out.push(CellRun { text: before.to_string(), ..format.clone() });
        }
        // The new text goes where the replaced range starts: in this run, or right after the one it ends.
        if !inserted && b0 >= start && (b0 < end || (b0 == end && std::ptr::eq(run, runs.last().unwrap()))) {
            if !text.is_empty() {
                out.push(CellRun { text: text.to_string(), ..format.clone() });
            }
            inserted = true;
        }
        if !after.is_empty() && b1 < end {
            out.push(CellRun { text: after.to_string(), ..format.clone() });
        }
        last_format = Some(format);
    }
    if !inserted && !text.is_empty() {
        out.push(CellRun { text: text.to_string(), ..last_format.unwrap_or_default() });
    }
    *runs = merge_runs(out);
}

/// `runs` with the bytes `b0..b1` of their text replaced by `inserted` runs,
/// preserving character formatting of both the surrounding text and the inserted runs.
pub(crate) fn runs_splice(runs: &mut Vec<CellRun>, b0: usize, b1: usize, inserted: &[CellRun]) {
    let mut out: Vec<CellRun> = Vec::new();
    let mut at = 0usize;
    let mut did_insert = false;
    for run in runs.iter() {
        let (start, end) = (at, at + run.text.len());
        at = end;
        let format = CellRun { text: String::new(), ..run.clone() };
        let before = &run.text[..b0.clamp(start, end) - start];
        let after = &run.text[b1.clamp(start, end) - start..];
        if !before.is_empty() {
            out.push(CellRun { text: before.to_string(), ..format.clone() });
        }
        if !did_insert && b0 >= start && (b0 < end || (b0 == end && std::ptr::eq(run, runs.last().unwrap()))) {
            for ins in inserted {
                if !ins.text.is_empty() {
                    out.push(ins.clone());
                }
            }
            did_insert = true;
        }
        if !after.is_empty() && b1 < end {
            out.push(CellRun { text: after.to_string(), ..format.clone() });
        }
    }
    if !did_insert {
        for ins in inserted {
            if !ins.text.is_empty() {
                out.push(ins.clone());
            }
        }
    }
    *runs = merge_runs(out);
}

/// Neighbouring parts of the same font as one; empty parts gone.
pub(crate) fn merge_runs(runs: Vec<CellRun>) -> Vec<CellRun> {
    let mut out: Vec<CellRun> = Vec::new();
    for run in runs.into_iter().filter(|r| !r.text.is_empty()) {
        match out.last_mut() {
            Some(last) if CellRun { text: String::new(), ..last.clone() } == (CellRun { text: String::new(), ..run.clone() }) => last.text.push_str(&run.text),
            _ => out.push(run),
        }
    }
    out
}

/// The height of a line of the widened formula bar.
pub(crate) const BAR_LINE_HEIGHT: f32 = 18.0;

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// The byte offset of UTF-16 position `u` in `s`.
pub(crate) fn byte_at(s: &str, u: usize) -> usize {
    let mut count = 0;
    for (i, c) in s.char_indices() {
        if count >= u {
            return i;
        }
        count += c.len_utf16();
    }
    s.len()
}

/// The lines of `text` (cut at line breaks), each with the UTF-16 position it starts at.
pub(crate) fn text_lines(text: &str) -> Vec<(usize, &str)> {
    let mut start = 0;
    text.split('\n')
        .map(|line| {
            let at = start;
            start += utf16_len(line) + 1;
            (at, line)
        })
        .collect()
}

/// A line of the formula bar's text while typing, kept whole so moving the caret never splits text runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BarLine {
    pub(crate) text: String,
    pub(crate) selection_bytes: Option<Range<usize>>,
    pub(crate) caret_byte: Option<usize>,
}

impl CellEdit {
    /// The line the caret is on (an index into `text_lines`).
    pub(crate) fn caret_line(&self) -> usize {
        text_lines(&self.text).iter().rposition(|(start, _)| *start <= self.caret).unwrap_or(0)
    }

    /// Where the caret goes one line up (`dy` < 0) or down, to the same column or the end of a shorter line; none at the first or last line.
    pub(crate) fn caret_after_line_move(&self, dy: i32) -> Option<usize> {
        let lines = text_lines(&self.text);
        let current = self.caret_line();
        let target = current as i64 + dy as i64;
        if target < 0 || target as usize >= lines.len() {
            return None;
        }
        let column = self.caret - lines[current].0;
        let (start, line) = lines[target as usize];
        Some(start + column.min(utf16_len(line)))
    }

    /// One line of the text (starting at UTF-16 position `start`) as the bar shows it while typing: its full text, the selected byte range, and the caret byte offset.
    pub(crate) fn bar_line(&self, start: usize, line: &str) -> BarLine {
        let end = start + utf16_len(line);
        let text = line.trim_end_matches('\r').to_string();
        let text_len = text.len();
        let selection_bytes = self.selection().and_then(|r| {
            let a = r.start.clamp(start, end);
            let b = r.end.clamp(start, end);
            if a < b {
                Some(byte_at(line, a - start).min(text_len)..byte_at(line, b - start).min(text_len))
            } else {
                None
            }
        });
        let caret_byte = (start..=end).contains(&self.caret).then(|| byte_at(line, self.caret - start).min(text_len));
        BarLine { text, selection_bytes, caret_byte }
    }

    /// The caret as a byte offset into `text`.
    pub(crate) fn caret_byte(&self) -> usize {
        byte_at(&self.text, self.caret)
    }

    /// The selected text, in UTF-16 units (none when the caret is alone).
    pub(crate) fn selection(&self) -> Option<Range<usize>> {
        let a = self.anchor?;
        let (s, e) = (a.min(self.caret), a.max(self.caret));
        (s != e).then_some(s..e)
    }

    fn replace(&mut self, range: Range<usize>, text: &str) -> Range<usize> {
        let len = utf16_len(&self.text);
        let (start, end) = (range.start.min(len), range.end.min(len).max(range.start.min(len)));
        let (b0, b1) = (byte_at(&self.text, start), byte_at(&self.text, end));
        let pending = if text.is_empty() { None } else { self.pending.take() };
        if let Some(p) = &pending {
            if self.text.is_empty() {
                self.runs = Some(vec![whole_run("", &p.style)]);
            } else if self.runs.is_none() {
                self.runs = Some(vec![whole_run(&self.text, &p.style)]);
            }
        }
        if let Some(runs) = self.runs.as_mut() {
            if self.text.is_empty() && pending.is_some() {
                // Nothing to take a place in: the first run holds the new text.
                if let Some(first) = runs.first_mut() {
                    first.text = text.to_string();
                }
            } else {
                runs_replace(runs, b0, b1, text);
            }
            if let Some(p) = &pending {
                for change in &p.changes {
                    let taken = std::mem::take(runs);
                    *runs = apply_to_runs(taken, b0, b0 + text.len(), change, &p.style);
                }
            }
        }
        self.anchor = None;
        self.text.replace_range(b0..b1, text);
        let inserted = start..start + utf16_len(text);
        self.caret = inserted.end;
        inserted
    }

    fn replace_runs(&mut self, range: Range<usize>, text: &str, inserted_runs: &[CellRun]) -> Range<usize> {
        let len = utf16_len(&self.text);
        let (start, end) = (range.start.min(len), range.end.min(len).max(range.start.min(len)));
        let (b0, b1) = (byte_at(&self.text, start), byte_at(&self.text, end));
        if self.runs.is_none() {
            self.runs = Some(vec![CellRun { text: self.text.clone(), ..Default::default() }]);
        }
        if let Some(runs) = self.runs.as_mut() {
            runs_splice(runs, b0, b1, inserted_runs);
        }
        self.anchor = None;
        self.text.replace_range(b0..b1, text);
        let inserted = start..start + utf16_len(text);
        self.caret = inserted.end;
        inserted
    }
}

/// The bytes of the word at byte `at` of `text`: a run of letters and
/// digits, or of the same kind of other character (a double-click selects it).
pub(crate) fn word_range(text: &str, at: usize) -> (usize, usize) {
    let kind = |c: char| -> u8 {
        if c.is_whitespace() {
            0
        } else if c.is_alphanumeric() && c.is_ascii() || c == '_' || c == '.' {
            1
        } else if c.is_alphanumeric() {
            // Kana, kanji and so on: a run of the same script.
            match c {
                '\u{3040}'..='\u{309f}' => 2,
                '\u{30a0}'..='\u{30ff}' | '\u{ff66}'..='\u{ff9f}' => 3,
                _ => 4,
            }
        } else {
            5
        }
    };
    let at = at.min(text.len());
    let next = text[at..].chars().next();
    let prev = text[..at].chars().next_back();
    let Some(c) = next.or(prev) else { return (0, 0) };
    let k = kind(c);
    let mut start = if next.is_some() { at } else { at - c.len_utf8() };
    let mut end = if next.is_some() { at + c.len_utf8() } else { at };
    while let Some(p) = text[..start].chars().next_back().filter(|&p| kind(p) == k) {
        start -= p.len_utf8();
    }
    while let Some(n) = text[end..].chars().next().filter(|&n| kind(n) == k) {
        end += n.len_utf8();
    }
    (start, end)
}

impl SlideEditor {
    /// The caret position (UTF-16 units) the pointer is at in the text being
    /// typed: the boundary between characters nearest to it. `None` before
    /// the text has been painted.
    pub(crate) fn cell_text_index_at(&self, position: Point<Pixels>, _cx: &App) -> Option<usize> {
        let edit = self.cell_edit()?;
        let rect = edit.rect.get()?;
        let layout = edit.layout.borrow().clone()?;
        let x = f32::from(position.x - rect.origin.x);
        // The box may be scrolled (text below the highest box): the line under the pointer is that far down.
        let y = f32::from(position.y - rect.origin.y) + edit.scroll_y.get();
        // The line whose middle is nearest to the pointer, then the boundary between characters nearest in x.
        let middle = |l: &super::sheet_view::EditLine| l.baseline + (l.descent - l.ascent) / 2.0;
        let line = layout.lines.iter().min_by(|a, b| (y - middle(a)).abs().total_cmp(&(y - middle(b)).abs()))?;
        let text = &edit.text;
        let (start, end) = (line.start.min(text.len()), line.end.min(text.len()));
        let mut best = (f32::MAX, start);
        for (i, _) in text[start..end].char_indices().map(|(i, c)| (start + i, c)).chain(std::iter::once((end, ' '))) {
            let d = (line.x_at(i) - x).abs();
            if d < best.0 {
                best = (d, i);
            }
        }
        Some(utf16_len(&text[..best.1]))
    }

    /// The caret position (UTF-16 units) the pointer is at in the formula
    /// bar's text (the window's font at the bar's size; the text starts
    /// 8 px in from the field's edge).
    pub(crate) fn bar_text_index_at(&self, position: Point<Pixels>, window: &Window) -> Option<usize> {
        let edit = self.cell_edit()?;
        let state = self.sheet_state()?;
        let rect = state.bar_bounds.get()?;
        let visual=&state.bar_visual_lines;
        let fallback=text_lines(&edit.text);
        let lines=if visual.is_empty(){fallback}else{visual.iter().map(|(s,l)|(*s,l.as_str())).collect()};
        // Closed, the bar shows the line the caret is on; widened, all of them from the scrolled top, each a line high.
        let row = if state.bar_expanded {
            let y = f32::from(position.y - rect.origin.y - state.bar_scroll.offset().y);
            ((y / BAR_LINE_HEIGHT).floor().max(0.0) as usize).min(lines.len() - 1)
        } else {
            state.bar_caret_row.filter(|&i|lines.get(i).is_some_and(|(a,l)|(*a..=*a+utf16_len(l)).contains(&edit.caret))).unwrap_or_else(||lines.iter().rposition(|(s,_)|*s<=edit.caret).unwrap_or(0))
        };
        let (start, line) = lines[row];
        let line = line.trim_end_matches('\r');
        let style = window.text_style();
        let size = window.rem_size() * 0.875;
        let run = TextRun { len: line.len(), font: style.font(), color: style.color, background_color: None, underline: None, strikethrough: None };
        let shaped = window.text_system().shape_line(SharedString::from(line.to_string()), size, &[run], None);
        let byte = shaped.closest_index_for_x(position.x - rect.origin.x - px(8.)).min(line.len());
        let byte = (0..=byte).rev().find(|&b| line.is_char_boundary(b)).unwrap_or(0);
        Some(start + utf16_len(&line[..byte]))
    }

    /// A press on the formula bar's text, as `press_in_cell_text` does on the cell's.
    pub(crate) fn press_in_bar_text(&mut self, position: Point<Pixels>, click_count: usize, shift: bool, window: &Window, cx: &mut Context<Self>) -> bool {
        let Some(index) = self.bar_text_index_at(position, window) else { return false };
        self.wb_mut().bar_caret_row=None;
        self.place_caret(index, click_count, shift, true, cx);
        true
    }

    /// A press on the text being typed: the caret goes there (Shift extends
    /// the selection, a double-click selects the word) and dragging on
    /// selects; typing goes on in Edit mode. False if it could not tell where.
    pub(crate) fn press_in_cell_text(&mut self, position: Point<Pixels>, click_count: usize, shift: bool, cx: &mut Context<Self>) -> bool {
        let Some(index) = self.cell_text_index_at(position, cx) else { return false };
        self.place_caret(index, click_count, shift, false, cx);
        true
    }

    /// F2 while typing: Enter (the arrows point at cells or finish the entry) and Edit (the arrows move the caret) swap.
    pub(crate) fn toggle_edit_mode(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.sheet_state_mut().and_then(|s| s.cell_edit.as_mut()) else { return };
        edit.mode = match edit.mode {
            EditMode::Enter => EditMode::Edit,
            EditMode::Edit => EditMode::Enter,
        };
        cx.notify();
    }

    fn place_caret(&mut self, index: usize, click_count: usize, shift: bool, in_bar: bool, cx: &mut Context<Self>) {
        let Some(edit) = self.sheet_state_mut().and_then(|s| s.cell_edit.as_mut()) else { return };
        edit.dragging_in_bar = in_bar;
        edit.mode = EditMode::Edit;
        edit.pending = None;
        edit.marked = None;
        if click_count >= 2 {
            let b = byte_at(&edit.text, index);
            let (b0, b1) = word_range(&edit.text, b);
            edit.anchor = Some(utf16_len(&edit.text[..b0]));
            edit.caret = utf16_len(&edit.text[..b1]);
            edit.dragging = None;
        } else if shift {
            edit.anchor = Some(edit.anchor.unwrap_or(edit.caret));
            edit.caret = index;
            edit.dragging = edit.anchor;
        } else {
            edit.anchor = None;
            edit.caret = index;
            edit.dragging = Some(index);
        }
        cx.notify();
    }

    /// The pointer moved with the button down on the text being typed: the
    /// selection runs from where it was pressed to the pointer.
    pub(crate) fn drag_in_cell_text(&mut self, position: Point<Pixels>, window: &Window, cx: &mut Context<Self>) {
        let in_bar = self.cell_edit().is_some_and(|e| e.dragging_in_bar);
        let Some(index) = (if in_bar { self.bar_text_index_at(position, window) } else { self.cell_text_index_at(position, cx) }) else { return };
        if let Some(edit) = self.sheet_state_mut().and_then(|s| s.cell_edit.as_mut())
            && let Some(from) = edit.dragging
        {
            edit.anchor = Some(from);
            edit.caret = index;
            cx.notify();
        }
    }

    pub(crate) fn cell_edit(&self) -> Option<&CellEdit> {
        self.sheet_state().and_then(|s| s.cell_edit.as_ref())
    }

    pub(crate) fn cell_edit_mut(&mut self) -> Option<&mut CellEdit> {
        self.sheet_state_mut().and_then(|s| s.cell_edit.as_mut())
    }

    /// Whether the keys go to the cells now (a workbook, a cell selected,
    /// no shape being edited, the name box and ジャンプ not in use).
    pub(crate) fn cells_take_typing(&self, window: &Window, cx: &App) -> bool {
        let Some(state) = self.sheet_state() else { return false };
        self.selection.is_none()
            && self.editing.is_none()
            && self.slideshow.is_none()
            && !state.jump.goto_open
            && !state.jump.name_box.read(cx).focus_handle(cx).is_focused(window)
            && state.jump.renaming.is_none()
            && self.current_cell().is_some()
    }

    /// Start typing into the selected cell: with its content (F2), or empty.
    pub(crate) fn begin_cell_edit(&mut self, mode: EditMode, keep_content: bool, cx: &mut Context<Self>) {
        if self.is_read_only() {
            return;
        }
        // A locked cell of a protected sheet is not typed into.
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some((column, row)) = self.current_cell() else { return };
        let text = if keep_content {
            self.current_sheet().and_then(|(_, sheet)| sheet.cell(row, column).map(|c| (sheet, c))).and_then(|(sheet, c)| match &c.formula {
                Some(f) => Some(format!("={}", sheet.formula_shown(row, column, f))),
                None if super::sheet_ui::spilled_ghost_formula_on(sheet, row, column).is_some() => None,
                None => Some(match &c.value {
                    // A number as typed, not as shown.
                    CellValue::Number(n) => ibis_calc::eval::number_to_text(*n),
                    _ => c.text.clone(),
                }),
            })
        } else {
            None
        }
        .unwrap_or_default();
        let mut caret = utf16_len(&text);
        let slide = self.current_slide;
        // A text constant with parts in their own fonts keeps them while edited.
        let runs = if keep_content {
            self.current_sheet().and_then(|(_, sheet)| sheet.cell(row, column)).filter(|c| c.formula.is_none()).and_then(|c| c.runs.as_ref().map(|r| r.to_vec())).filter(|r| r.iter().map(|x| x.text.as_str()).collect::<String>() == text)
        } else {
            None
        };
        // When editing existing cell content, measure its edit-box layout: if lines overflow the box (reaching the
        // bottom of the sheet view), start with the caret at the beginning; and scroll horizontally so the box (and
        // its scrollbar) fits on screen (or aligns its left edge with the row header's right edge when wider than the window).
        let mut scroll_span: Option<(u32, f32)> = None;
        if keep_content
            && !text.is_empty()
            && let Some((shape, sheet)) = self.current_sheet()
        {
            let scale = self.workspace_content().map_or(1.0 / ibis_doc_model::EMU_PER_POINT as f32, |c| c.scale);
            let px_per_point = (ibis_doc_model::EMU_PER_POINT as f32 * scale).max(0.1);
            let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
            let at = |offsets: &[i64], i: u32, default: i64| {
                if (i as usize) < offsets.len() {
                    offsets[i as usize]
                } else {
                    offsets.last().copied().unwrap_or(0) + (i as usize - offsets.len().saturating_sub(1)) as i64 * default.max(1)
                }
            };
            let range = sheet.merge_at(row, column).unwrap_or(ibis_doc_model::CellRange { first_row: row, first_column: column, last_row: row, last_column: column });
            let (l, r) = (at(&xs, range.first_column, sheet.default_column), at(&xs, range.last_column + 1, sheet.default_column));
            let (t, b) = (at(&ys, range.first_row, sheet.default_row), at(&ys, range.last_row + 1, sheet.default_row));
            let (ex0, ex1) = ((shape.frame.x + l) as f32 * scale, (shape.frame.x + r) as f32 * scale);
            let (ey0, ey1) = ((shape.frame.y + t) as f32 * scale, (shape.frame.y + b) as f32 * scale);
            let default_style = self.presentation().workbook.as_ref().and_then(|w| w.default_style.as_ref());
            let ts = WindowTextSystem::new(cx.text_system().clone());
            let layout = super::sheet_view::edit_layout(&ts, sheet, default_style, &self.resources, row, column, &text, runs.clone().map(std::sync::Arc::new), (l as f32 * scale, t as f32 * scale), (ex1 - ex0, ey1 - ey0), px_per_point, self.cell_editor_width(ex0));
            let max_box_h = self.max_edit_box_height_px(row, ey0, ey1, px_per_point);
            let overflows = layout.height - max_box_h > 0.5;
            if text.contains('\n') && (overflows || layout.height > (ey1 - ey0) + 0.5) {
                caret = 0;
            }
            let bar_w = if overflows { 10.0 } else { 0.0 };
            scroll_span = Some((range.first_column, layout.width + 1.0 + bar_w));
        }
        if let Some((first_col, width_px)) = scroll_span {
            self.scroll_span_horizontally_into_view(first_col, width_px);
        }
        if let Some(state) = self.sheet_state_mut() {
            state.validation_warning = None;
            state.cell_edit = Some(CellEdit { slide, column, row, text, caret, marked: None, mode, anchor: None, runs, pointed: None, reference_drag:false, rect: Rc::new(Cell::new(None)), pixels_per_point: Rc::new(Cell::new(0.0)), layout: Default::default(), dragging: None, dragging_in_bar: false, pending: None, scroll_y: Default::default(), followed_caret: Default::default() });
        }
        cx.notify();
    }

    /// Give up typing (Esc).
    pub(crate) fn cancel_cell_edit(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(s) = self.sheet_state_mut() { s.validation_choices = None; }
        let had = self.sheet_state_mut().and_then(|s| s.cell_edit.take()).is_some();
        if had {
            cx.notify();
        }
        had
    }

    /// Finish typing: put what was typed into the cell as one step.
    pub(crate) fn commit_cell_edit(&mut self, cx: &mut Context<Self>) {
        let candidates = self.filtered_validation_choices();
        let chosen = self.sheet_state().and_then(|s| s.validation_choices.as_ref()).and_then(|_| candidates.get(self.validation_choice_index(candidates.len()))).cloned();
        if let Some(s) = self.sheet_state_mut() { s.validation_choices = None; }
        let Some(mut edit) = self.sheet_state_mut().and_then(|s| s.cell_edit.take()) else { return };
        if let Some(chosen) = chosen { edit.text = chosen; }
        if super::formula_reference::is_formula(&edit.text){edit.text=super::formula_reference::close_parentheses(&edit.text);}
        if let Some(s) = self.sheet_state_mut() {
            s.last_fill = None;
            s.validation_warning = None;
        }
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let sheet = self.presentation().slide(edit.slide).and_then(ibis_doc_model::sheet_calc::sheet_of_slide);
        // A cell formatted as Text (@) keeps what is typed as text, a date or
        // a formula too, as Excel does (#13).
        let text_formatted = sheet.and_then(|sheet| sheet.cell(edit.row, edit.column)).zip(sheet).is_some_and(|(c, sheet)| sheet.styles[c.style as usize].number_format == "@");
        let (input, format): (CellInput, Option<String>) = if edit.text.is_empty() {
            (CellInput::Clear, None)
        } else if text_formatted {
            (CellInput::Value(CellValue::Text(edit.text.strip_prefix('\'').unwrap_or(&edit.text).to_string())), None)
        } else {
            match parse_input(&edit.text, date1904) {
                Typed::Formula(f) => {
                    let format = sheet.and_then(|sheet| referenced_format(sheet, &f));
                    (CellInput::Formula(f), format)
                }
                // オートコレクト: the text typed in is corrected as Excel's is.
                Typed::Value(CellValue::Text(t), f) => (CellInput::Value(CellValue::Text(super::autocorrect::correct_typed(&t))), f.map(str::to_string)),
                Typed::Value(v, f) => (CellInput::Value(v), f.map(str::to_string)),
            }
        };
        // データの入力規則: a value it refuses stays being typed, with the message.
        let mut warning = None;
        let check_val = match &input {
            CellInput::Value(v) => Some(v),
            CellInput::Clear => Some(&CellValue::Empty),
            _ => None,
        };
        if let Some(value) = check_val {
            match self.check_entry(edit.row, edit.column, value) {
                Err(message) => {
                    self.status = Some(message.clone().into());
                    if self.current_rule().is_some_and(|r|r.kind=="list") {
                        self.show_panel_message(message.into(), cx);
                        self.wb_mut().validation_rejected = Some((edit.column, edit.row));
                    }
                    if let Some(state) = self.sheet_state_mut() {
                        state.cell_edit = Some(edit);
                    }
                    cx.notify();
                    return;
                }
                Ok(w) => warning = w,
            }
        }
        // A workbook the formula names that the file does not link yet: linked in the same step.
        let link_commands = match &input {
            CellInput::Formula(f) if ibis_calc::formula::has_external_book(f) => self.link_new_books_commands(f),
            _ => Vec::new(),
        };
        let sheet = self.presentation().slide(edit.slide).and_then(ibis_doc_model::sheet_calc::sheet_of_slide);
        // The number format Excel gives what was typed, if the cell has none.
        let general = sheet
            .map(|sheet| sheet.cell(edit.row, edit.column).map_or("General".to_string(), |c| sheet.styles[c.style as usize].number_format.clone()))
            .is_some_and(|f| f == "General");
        let mut commands = link_commands;
        commands.push(Command::Sheet(SheetCommand::SetCells { slide: edit.slide, edits: vec![ModelEdit { row: edit.row, column: edit.column, input: input.clone() }] }));
        let auto_format = matches!(input, CellInput::Value(CellValue::Number(_))) && format.is_some() && general;
        if let (Some(code), true) = (format.clone(), general) {
            let range = CellRange { first_row: edit.row, first_column: edit.column, last_row: edit.row, last_column: edit.column };
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide: edit.slide, ranges: vec![range], change: StyleChange::NumberFormat(code) }));
        }
        // オートコレクト: a URL or an email address typed alone becomes a link
        // (the link's colour and underline, one step with the entry).
        if let CellInput::Value(CellValue::Text(t)) = &input
            && let Some(target) = link_target_of_typed(t)
        {
            let range = CellRange { first_row: edit.row, first_column: edit.column, last_row: edit.row, last_column: edit.column };
            let link = ibis_doc_model::Hyperlink { range, target: Some(target), location: None, tooltip: None };
            commands.push(Command::Sheet(SheetCommand::SetHyperlink { slide: edit.slide, range, link: Some(link) }));
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide: edit.slide, ranges: vec![range], change: StyleChange::Color(ibis_doc_model::Rgb(0x0563C1)) }));
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide: edit.slide, ranges: vec![range], change: StyleChange::Underline(true) }));
        }
        // Text with a line break (⌥Enter): the cell wraps and its row grows
        // to the lines, as Excel's do (in the same step).
        let orig_cell = sheet.and_then(|sh| sh.cell(edit.row, edit.column));
        let orig_text = orig_cell.map(|c| c.text.as_str()).unwrap_or("");
        let content_changed = orig_cell.map_or(!edit.text.is_empty(), |c| {
            if c.formula.is_some() {
                true
            } else {
                match &c.value {
                    CellValue::Text(t) => t != &edit.text,
                    CellValue::Number(n) => ibis_calc::eval::number_to_text(*n) != edit.text,
                    CellValue::Empty => !edit.text.is_empty(),
                    _ => c.text != edit.text,
                }
            }
        });
        let broken = matches!(&input, CellInput::Value(CellValue::Text(t)) if t.contains('\n'));
        let newly_broken = broken && !orig_text.contains('\n');
        if newly_broken && sheet.is_some_and(|sheet| !sheet.cell(edit.row, edit.column).is_some_and(|c| sheet.styles[c.style as usize].wrap)) {
            let range = CellRange { first_row: edit.row, first_column: edit.column, last_row: edit.row, last_column: edit.column };
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide: edit.slide, ranges: vec![range], change: StyleChange::Wrap(true) }));
        }
        // Parts in their own fonts (set on a selection while typing) go with the text.
        if let (Some(runs), CellInput::Value(CellValue::Text(t))) = (&edit.runs, &input)
            && runs.iter().map(|r| r.text.as_str()).collect::<String>() == *t
        {
            commands.push(Command::Sheet(SheetCommand::SetRuns { slide: edit.slide, row: edit.row, column: edit.column, runs: runs.clone() }));
        }
        let cell_size = self.presentation().slide(edit.slide).and_then(ibis_doc_model::sheet_calc::sheet_of_slide).and_then(|sh| sh.cell(edit.row, edit.column).map(|c| sh.styles[c.style as usize].font_size)).unwrap_or(1100);
        // A part given another size while typing grows the row too, though the text is the same (Excel fits the row on the entry).
        let runs_changed = edit.runs.as_ref().map(|r| r.to_vec()) != orig_cell.and_then(|c| c.runs.as_ref().map(|r| r.to_vec()));
        let taller = (content_changed || runs_changed) && matches!((&edit.runs, &input), (Some(runs), CellInput::Value(CellValue::Text(_))) if runs.iter().any(|r| r.size.is_some_and(|size| size > cell_size)));
        let is_merged = sheet.is_some_and(|sh| sh.merge_at(edit.row, edit.column).is_some_and(|m| m.first_row != m.last_row || m.first_column != m.last_column));
        let mut result = self.document.execute(commands.clone());
        if !is_merged && (newly_broken || taller) && result.is_ok() && edit.slide == self.current_slide {
            let ts = cx.text_system().clone();
            let rows = self.current_sheet().map(|(_, s)| s.rows.get(edit.row as usize).copied().unwrap_or(0)).unwrap_or(0);
            let fit: Vec<Command> = self
                .fit_rows_commands(edit.row, edit.row, &ts)
                .into_iter()
                .filter(|c| matches!(c, Command::Sheet(SheetCommand::SetSize { size, .. }) if *size > rows))
                .collect();
            if !fit.is_empty() {
                self.document.undo();
                commands.extend(fit);
                result = self.document.execute(commands.clone());
            }
        }
        // A number entered that is wider than its column widens the column, unless the width was set by hand (as in Excel).
        // Measured once the cell is entered, so a formula's result and the format Excel gives what was typed count; one undo step with the entry.
        if result.is_ok()
            && let Some(widen) = self.auto_widen_command(edit.slide, edit.row, edit.column, &cx.text_system().clone())
        {
            self.document.undo();
            commands.push(widen);
            result = self.document.execute(commands);
        }
        match result {
            Ok(_) => {
                self.after_table_entry(edit.slide, edit.row, edit.column, &input, cx);
                self.sync_shape_formulas(cx);
            }
            Err(err) => log::warn!("cell entry rejected: {err}"),
        }
        if let Some((w, is_warning)) = warning {
            self.status = Some(w.clone().into());
            if is_warning {
                if let Some(s) = self.sheet_state_mut() {
                    s.validation_warning = Some(super::validation::ValidationWarning { message: w, edit });
                }
            }
        }
        self.sync_workbook_peers(cx);
        cx.notify();
    }

    /// The command that widens the column of a number just entered to show it, if the column is wider by nothing but the sheet's
    /// own choice (not set by hand), shows nothing narrower, and the cell is a number (a formula's result too). Only widens.
    /// A decimal fraction is rounded to fit by the display, so only the whole part decides.
    pub(crate) fn auto_widen_command(&self, slide: SlideId, row: u32, column: u32, ts: &TextSystem) -> Option<Command> {
        let sheet = self.presentation().slide(slide).and_then(ibis_doc_model::sheet_calc::sheet_of_slide)?;
        let cell = sheet.cell(row, column)?;
        let current = sheet.columns.get(column as usize).copied().unwrap_or(0);
        if !matches!(cell.value, CellValue::Number(_)) || cell.text.is_empty() || current <= 0 || sheet.custom_columns.contains(&column) {
            return None;
        }
        if sheet.merge_at(row, column).is_some_and(|m| m.first_row != m.last_row || m.first_column != m.last_column) {
            return None;
        }
        let style = &sheet.styles[cell.style as usize];
        let shown = match cell.text.split_once('.') {
            Some((whole, _)) if style.number_format == "General" && !cell.text.contains('E') => whole,
            _ => cell.text.as_str(),
        };
        let needed = super::headers::fitted_width(self.excel_text_width(shown, style, ts));
        (needed > current).then_some(Command::Sheet(SheetCommand::SetColumnWidthAuto { slide, first: column, last: column, size: needed }))
    }

    /// Clear the selected cells' values (Delete when not typing).
    pub(crate) fn clear_selected_cells(&mut self, cx: &mut Context<Self>) {
        // With the ranges 条件を選択してジャンプ selected with it.
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let ranges = self.selected_ranges();
        let Some((_, sheet)) = self.current_sheet() else { return };
        let edits: Vec<ModelEdit> = sheet
            .cells
            .iter()
            .filter(|c| ranges.iter().any(|range| (range.first_row..=range.last_row).contains(&c.row) && (range.first_column..=range.last_column).contains(&c.column)))
            .filter(|c| !c.value.is_empty() || c.formula.is_some())
            .map(|c| ModelEdit { row: c.row, column: c.column, input: CellInput::Clear })
            .collect();
        if edits.is_empty() {
            return;
        }
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits })]) {
            log::warn!("clearing rejected: {err}");
        }
        self.sync_workbook_peers(cx);
        cx.notify();
    }

    /// Backspace or Delete: a character while typing, the cells' values otherwise.
    pub(crate) fn cell_delete(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(edit) = self.cell_edit_mut() else {
            return self.clear_selected_cells(cx);
        };
        if let Some(sel) = edit.selection() {
            edit.replace(sel, "");
            return cx.notify();
        }
        let caret = edit.caret;
        let range = if forward {
            let b0 = byte_at(&edit.text, caret);
            match edit.text[b0..].chars().next() {
                Some(c) => caret..caret + c.len_utf16(),
                None => return,
            }
        } else {
            let b1 = byte_at(&edit.text, caret);
            match edit.text[..b1].chars().next_back() {
                Some(c) => caret - c.len_utf16()..caret,
                None => return,
            }
        };
        edit.replace(range, "");
        cx.notify();
    }

    /// ← or → while typing in Edit mode: move the caret. False when the
    /// arrow is for moving between cells.
    pub(crate) fn cell_caret(&mut self, dx: i32, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.cell_edit_mut() else { return false };
        if edit.mode != EditMode::Edit || dx == 0 {
            return false;
        }
        let len = utf16_len(&edit.text);
        // With a selection, ← and → go to its start and end.
        edit.caret = match (edit.selection(), dx < 0) {
            (Some(sel), true) => sel.start,
            (Some(sel), false) => sel.end,
            (None, true) => edit.caret.saturating_sub(1),
            (None, false) => (edit.caret + 1).min(len),
        };
        edit.anchor = None;
        edit.pending = None;
        cx.notify();
        true
    }

    /// ↑ or ↓ while typing in Edit mode into text of several lines: the caret goes a line up or down (nothing at the first or last line, as in Excel). False when the text is one line, or not in Edit mode: the arrow is for the cells then.
    pub(crate) fn cell_caret_line(&mut self, dy: i32, extend: bool, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.cell_edit_mut() else { return false };
        if edit.mode != EditMode::Edit || dy == 0 || !edit.text.contains('\n') {
            return false;
        }
        if let Some(to) = edit.caret_after_line_move(dy) {
            if extend {
                edit.anchor = Some(edit.anchor.unwrap_or(edit.caret));
            } else {
                edit.anchor = None;
            }
            edit.caret = to;
            edit.pending = None;
            cx.notify();
        }
        true
    }

    /// PageUp or PageDown while typing in Edit mode into text of several lines: the caret goes up or down by about the lines
    /// the box shows (to the first or last line at most); the box then scrolls to show it (issue #17). False when the text is
    /// one line, or not in Edit mode: the key is for the sheet then.
    pub(crate) fn cell_caret_page(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.cell_edit_mut() else { return false };
        if edit.mode != EditMode::Edit || !edit.text.contains('\n') {
            return false;
        }
        let lines = text_lines(&edit.text);
        // The lines a box of the highest size shows, one kept in view.
        let step = edit
            .layout
            .borrow()
            .as_ref()
            .and_then(|layout| {
                let first = layout.lines.first()?;
                let second = layout.lines.get(1)?;
                let pitch = (second.baseline - first.baseline).abs().max(1.0);
                let visible = (409.5 * edit.pixels_per_point.get()).min(layout.height.max(pitch));
                Some(((visible / pitch).floor() as i32 - 1).max(1))
            })
            .unwrap_or(15);
        let current = edit.caret_line() as i32;
        let target = (current + if forward { step } else { -step }).clamp(0, lines.len() as i32 - 1);
        if target != current {
            let column = edit.caret - lines[current as usize].0;
            let (start, line) = lines[target as usize];
            edit.caret = start + column.min(utf16_len(line));
        }
        edit.anchor = None;
        edit.pending = None;
        cx.notify();
        true
    }

    /// Shift+← or →: the selection of the text being edited grows or shrinks by a character. False when not editing in Edit mode.
    pub(crate) fn cell_extend_selection(&mut self, dx: i32, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.cell_edit_mut() else { return false };
        if edit.mode != EditMode::Edit || dx == 0 {
            return false;
        }
        let len = utf16_len(&edit.text);
        let anchor = edit.anchor.unwrap_or(edit.caret);
        edit.caret = if dx < 0 { edit.caret.saturating_sub(1) } else { (edit.caret + 1).min(len) };
        edit.anchor = Some(anchor);
        cx.notify();
        true
    }

    /// ⌘A while editing: the whole text selected.
    pub(crate) fn cell_select_all_text(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.cell_edit_mut() else { return false };
        edit.anchor = Some(0);
        edit.caret = utf16_len(&edit.text);
        cx.notify();
        true
    }

    /// A font change on the selected text while editing (太字・斜体・下線・取り消し線・文字の色・サイズ): the selected part gets it, in its own run. False when there is no selection (the whole cell is formatted then).
    pub(crate) fn format_edit_selection(&mut self, change: &StyleChange, cx: &mut Context<Self>) -> bool {
        let slide = self.current_slide;
        let style = self.current_style();
        let Some(edit) = self.cell_edit_mut() else { return false };
        if edit.slide != slide || super::formula_reference::is_formula(&edit.text) {
            return false;
        }
        if !matches!(change, StyleChange::Bold(_) | StyleChange::Italic(_) | StyleChange::Underline(_) | StyleChange::UnderlineKind(_) | StyleChange::Strike(_) | StyleChange::Color(_) | StyleChange::ColorTheme(..) | StyleChange::FontSize(_) | StyleChange::Font(_)) {
            return false;
        }
        // The caret alone: the change waits for the text typed next (the same toggle again takes it back).
        let Some(sel) = edit.selection() else {
            let pending = edit.pending.get_or_insert_with(|| PendingFormat { changes: Vec::new(), style: style.clone() });
            let same = |a: &StyleChange, b: &StyleChange| std::mem::discriminant(a) == std::mem::discriminant(b);
            let toggles = matches!(change, StyleChange::Bold(_) | StyleChange::Italic(_) | StyleChange::Underline(_) | StyleChange::Strike(_));
            if toggles && pending.changes.iter().any(|c| same(c, change)) {
                pending.changes.retain(|c| !same(c, change));
            } else {
                pending.changes.retain(|c| !same(c, change));
                pending.changes.push(change.clone());
            }
            if pending.changes.is_empty() {
                edit.pending = None;
            }
            // When editing text with no selection, apply font changes to the entire text
            // so the edited cell immediately reflects the selected font.
            if matches!(change, StyleChange::Font(_) | StyleChange::FontSize(_)) && !edit.text.is_empty() {
                let b_end = edit.text.len();
                let runs = edit.runs.take().unwrap_or_else(|| vec![whole_run(&edit.text, &style)]);
                edit.runs = Some(apply_to_runs(runs, 0, b_end, change, &style));
            }
            cx.notify();
            return true;
        };
        let (b0, b1) = (byte_at(&edit.text, sel.start), byte_at(&edit.text, sel.end));
        let runs = edit.runs.take().unwrap_or_else(|| vec![whole_run(&edit.text, &style)]);
        edit.runs = Some(apply_to_runs(runs, b0, b1, change, &style));
        cx.notify();
        true
    }

    /// A click on a cell while typing a formula where a reference may go:
    /// its reference put in at the caret (and remembered, for a drag to
    /// make it a range). False when the click is for leaving the entry.
    pub(crate) fn point_reference(&mut self, column: u32, row: u32, cx: &mut Context<Self>) -> bool {
        let slide = self.current_slide;
        let Some(edit) = self.cell_edit_mut() else { return false };
        if edit.slide != slide || !super::formula_reference::is_formula(&edit.text) {
            return false;
        }
        let caret = byte_at(&edit.text, edit.caret);
        let before = edit.text[..caret].trim_end();
        // Right after a reference put in by pointing, a new click replaces it.
        let replacing = edit.pointed.filter(|&(s, e, _)| e == caret && s <= caret);
        let open = replacing.is_some() || before.ends_with(['=', '(', ',', '+', '-', '*', '/', '^', '&', '<', '>', ':']);
        if !open {
            return false;
        }
        let reference = ibis_doc_model::cell_name(column, row);
        let start = replacing.map_or(caret, |(s, _, _)| s);
        edit.text.replace_range(start..caret, &reference);
        let end = start + reference.len();
        edit.caret = utf16_len(&edit.text[..end]);
        edit.pointed = Some((start, end, (column, row)));
        cx.notify();
        true
    }

    /// The button up after pointing: the reference becomes the range to
    /// the cell let go on.
    pub(crate) fn finish_pointing(&mut self, column: u32, row: u32, cx: &mut Context<Self>) {
        let Some(edit) = self.cell_edit_mut() else { return };
        let Some((start, end, anchor)) = edit.pointed else { return };
        if anchor == (column, row) {
            return;
        }
        let (c0, c1) = (anchor.0.min(column), anchor.0.max(column));
        let (r0, r1) = (anchor.1.min(row), anchor.1.max(row));
        let range = format!("{}:{}", ibis_doc_model::cell_name(c0, r0), ibis_doc_model::cell_name(c1, r1));
        edit.text.replace_range(start..end, &range);
        let new_end = start + range.len();
        edit.caret = utf16_len(&edit.text[..new_end]);
        edit.pointed = Some((start, new_end, anchor));
        cx.notify();
    }

    /// F4 while typing a formula: the reference at (or just before) the
    /// caret cycles `A1` → `$A$1` → `A$1` → `$A1` → `A1`, as in Excel.
    pub(crate) fn cycle_reference(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(edit) = self.cell_edit_mut() else { return false };
        if !super::formula_reference::is_formula(&edit.text) {
            return false;
        }
        let caret = byte_at(&edit.text, edit.caret);
        let Some((start, end, new)) = next_reference_form(&edit.text, caret) else { return false };
        edit.text.replace_range(start..end, &new);
        edit.caret = utf16_len(&edit.text[..start + new.len()]);
        cx.notify();
        true
    }

    // ----- The input hooks (shell/input.rs) -----------------------------

    pub(crate) fn cell_text_for_range(&self, range: Range<usize>) -> Option<String> {
        let text = self.cell_edit().map_or("", |e| e.text.as_str());
        let (b0, b1) = (byte_at(text, range.start), byte_at(text, range.end));
        Some(text[b0..b1.max(b0)].to_string())
    }

    pub(crate) fn cell_selected_range(&self) -> Option<UTF16Selection> {
        let caret = self.cell_edit().map_or(0, |e| e.caret);
        Some(UTF16Selection { range: caret..caret, reversed: false })
    }

    pub(crate) fn cell_marked_range(&self) -> Option<Range<usize>> {
        self.cell_edit().and_then(|e| e.marked.clone())
    }

    pub(crate) fn cell_unmark(&mut self) {
        if let Some(e) = self.cell_edit_mut() {
            e.marked = None;
        }
    }

    /// The bytes `start..end` of the text being typed replaced with `text`,
    /// the caret after it.
    pub(crate) fn replace_cell_text(&mut self, start: usize, end: usize, text: &str, cx: &mut Context<Self>) {
        let Some(edit) = self.cell_edit_mut() else { return };
        let (a, b) = (utf16_len(&edit.text[..start.min(edit.text.len())]), utf16_len(&edit.text[..end.min(edit.text.len())]));
        edit.pointed = None;
        edit.marked = None;
        edit.replace(a..b, text);
        cx.notify();
    }

    /// ⌥Enter: a line break at the caret while typing (the cell then wraps
    /// when the entry is finished, as Excel's does).
    pub(crate) fn cell_line_break(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.cell_edit_mut() else { return };
        edit.pointed = None;
        let range = edit.marked.take().unwrap_or(edit.caret..edit.caret);
        edit.replace(range, "\n");
        cx.notify();
    }

    /// Text typed (or committed by the input method).
    pub(crate) fn cell_replace(&mut self, range: Option<Range<usize>>, text: &str, mark: Option<Option<Range<usize>>>, cx: &mut Context<Self>) {
        // Keys that finish or move are actions, not text.
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if self.cell_edit().is_none() {
            if text.is_empty() && mark.is_none() {
                return;
            }
            self.begin_cell_edit(EditMode::Enter, false, cx);
        }
        if !text.is_empty() { self.show_typed_validation_choices(); }
        let Some(edit) = self.cell_edit_mut() else { return };
        edit.pointed = None;
        let range = range.or_else(|| edit.marked.clone()).or_else(|| edit.selection()).unwrap_or(edit.caret..edit.caret);
        let inserted = edit.replace(range, &text);
        match mark {
            Some(selected) => {
                edit.marked = (!inserted.is_empty()).then_some(inserted.clone());
                if let Some(sel) = selected {
                    edit.caret = inserted.start + sel.end.min(inserted.len());
                }
            }
            None => edit.marked = None,
        }
        cx.notify();
    }

    pub(crate) fn cell_bounds_for_range(&self, element: Bounds<Pixels>) -> Option<Bounds<Pixels>> {
        let edit = self.cell_edit()?;
        Some(edit.rect.get().unwrap_or(Bounds::new(element.origin, size(px(100.), px(20.)))))
    }
}

/// The number format a formula typed into a General cell takes, as Excel
/// gives it: that of the first cell it refers to on its own sheet (not for
/// the COUNT functions, whose results are counts).
/// A piece of the text being typed, cut only at formatting run boundaries so moving the caret never splits text runs.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EditItem {
    pub(crate) text: String,
    pub(crate) run: Option<CellRun>,
}

/// Selection and caret byte offsets within one line of the inline cell editor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineOverlay {
    pub(crate) selection_bytes: Option<Range<usize>>,
    pub(crate) caret_byte: Option<usize>,
}

impl CellEdit {
    /// The text as formatting runs only (never cut at the selection or caret).
    pub(crate) fn items(&self) -> Vec<EditItem> {
        let len = self.text.len();
        let mut cuts = vec![0, len];
        let mut at = 0;
        for run in self.runs.iter().flatten() {
            at += run.text.len();
            cuts.push(at.min(len));
        }
        cuts.sort_unstable();
        cuts.dedup();
        let run_at = |b: usize| -> Option<CellRun> {
            let mut start = 0;
            for run in self.runs.iter().flatten() {
                let end = start + run.text.len();
                if b >= start && b < end {
                    return Some(run.clone());
                }
                start = end;
            }
            None
        };
        let mut out = Vec::new();
        for w in cuts.windows(2) {
            out.push(EditItem { text: self.text[w[0]..w[1]].to_string(), run: run_at(w[0]) });
        }
        if out.is_empty() {
            out.push(EditItem { text: String::new(), run: None });
        }
        out
    }

    /// Selection and caret byte offsets for each `'\n'`-separated line of `text`.
    pub(crate) fn line_overlays(&self) -> Vec<LineOverlay> {
        text_lines(&self.text)
            .into_iter()
            .map(|(start, line)| {
                let bar = self.bar_line(start, line);
                LineOverlay { selection_bytes: bar.selection_bytes, caret_byte: bar.caret_byte }
            })
            .collect()
    }
}

/// One run of all of `text` in the cell's font, every property set (as Excel writes a run).
fn whole_run(text: &str, style: &ibis_doc_model::CellStyle) -> CellRun {
    CellRun {
        text: text.to_string(),
        font: None,
        size: Some(style.font_size),
        bold: Some(style.bold),
        italic: Some(style.italic),
        underline: Some(style.underline),
        strike: Some(style.strike),
        color: Some(style.color),
    }
}

/// `runs` with `change` applied to the bytes `b0..b1` of their text (a toggle
/// goes off when the whole selection has it on, else on).
fn apply_to_runs(runs: Vec<CellRun>, b0: usize, b1: usize, change: &StyleChange, style: &ibis_doc_model::CellStyle) -> Vec<CellRun> {
    // Split at both ends.
    let mut split: Vec<CellRun> = Vec::new();
    let mut at = 0usize;
    for run in runs {
        let (start, end) = (at, at + run.text.len());
        at = end;
        let mut cuts = vec![start];
        cuts.extend([b0, b1].into_iter().filter(|&b| b > start && b < end));
        cuts.push(end);
        for w in cuts.windows(2) {
            split.push(CellRun { text: run.text[w[0] - start..w[1] - start].to_string(), ..run.clone() });
        }
    }
    let inside = |i: usize, split: &[CellRun]| {
        let start: usize = split[..i].iter().map(|r| r.text.len()).sum();
        start >= b0 && start + split[i].text.len() <= b1
    };
    let all = |f: &dyn Fn(&CellRun) -> bool| split.iter().enumerate().filter(|(i, _)| inside(*i, &split)).all(|(_, r)| f(r));
    let bold_on = !all(&|r| r.bold.unwrap_or(style.bold));
    let italic_on = !all(&|r| r.italic.unwrap_or(style.italic));
    let underline_on = !all(&|r| r.underline.unwrap_or(style.underline).enabled());
    let strike_on = !all(&|r| r.strike.unwrap_or(style.strike));
    let flags: Vec<bool> = (0..split.len()).map(|i| inside(i, &split)).collect();
    for (run, hit) in split.iter_mut().zip(flags) {
        if !hit {
            continue;
        }
        match change {
            StyleChange::Bold(_) => run.bold = Some(bold_on),
            StyleChange::Italic(_) => run.italic = Some(italic_on),
            StyleChange::Underline(_) => run.underline = Some(if underline_on { ibis_doc_model::CellUnderline::Single } else { ibis_doc_model::CellUnderline::None }),
            StyleChange::UnderlineKind(u) => run.underline = Some(*u),
            StyleChange::Strike(_) => run.strike = Some(strike_on),
            StyleChange::Color(c) | StyleChange::ColorTheme(c, _) => run.color = Some(*c),
            StyleChange::FontSize(s) => run.size = Some(*s),
            StyleChange::Font(f) => run.font = f.clone(),
            _ => {}
        }
    }
    merge_runs(split)
}

fn referenced_format(sheet: &ibis_doc_model::Sheet, formula: &str) -> Option<String> {
    // Excel gives the result the format of the cell it refers to for
    // arithmetic and a few functions (not for counts, DATEDIF, EDATE, …).
    let upper = formula.trim_start().to_ascii_uppercase();
    if let Some(paren) = upper.find('(') {
        let name = upper[..paren].trim_start_matches(['+', '-', '@']);
        let inherits = ["SUM", "AVERAGE", "MIN", "MAX", "ROUND", "ROUNDUP", "ROUNDDOWN", "ABS", "IF", "MEDIAN", "SUBTOTAL"];
        if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_') && !name.is_empty() && !inherits.contains(&name) {
            return None;
        }
    }
    let mut first = None;
    ibis_calc::formula::rewrite_references(formula, |r| {
        if first.is_none() && r.sheet.is_none() && !r.whole_columns && !r.whole_rows {
            first = Some((r.start.row, r.start.column));
        }
        Some(r.clone())
    });
    let (row, column) = first?;
    let code = &sheet.styles[sheet.cell(row, column)?.style as usize].number_format;
    (code != "General" && code != "@").then(|| code.clone())
}

/// The cell reference touching byte `caret` in `text` (`$A$1`, `A1`, a
/// range's either end) and its next form for F4: (start, end, new text).
pub(crate) fn next_reference_form(text: &str, caret: usize) -> Option<(usize, usize, String)> {
    let bytes = text.as_bytes();
    let is_part = |b: u8| b.is_ascii_alphanumeric() || b == b'$';
    // The run of reference characters around the caret.
    let mut start = caret.min(bytes.len());
    while start > 0 && is_part(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = caret.min(bytes.len());
    while end < bytes.len() && is_part(bytes[end]) {
        end += 1;
    }
    let word = &text[start..end];
    let plain = word.replace('$', "");
    let letters = plain.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    let digits = &plain[letters..];
    if letters == 0 || letters > 3 || digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (column, row) = (&plain[..letters], digits);
    let column_fixed = word.starts_with('$');
    let row_fixed = word[1..].contains('$');
    let next = match (column_fixed, row_fixed) {
        (false, false) => format!("${column}${row}"),
        (true, true) => format!("{column}${row}"),
        (false, true) => format!("${column}{row}"),
        (true, false) => format!("{column}{row}"),
    };
    Some((start, end, next))
}

#[cfg(test)]
mod tests {
    use super::next_reference_form;

    #[test]
    fn f4() {
        let step = |t: &str, c: usize| next_reference_form(t, c).map(|(s, e, n)| format!("{}{}{}", &t[..s], n, &t[e..]));
        assert_eq!(step("=A1*2", 3).as_deref(), Some("=$A$1*2"));
        assert_eq!(step("=$A$1*2", 5).as_deref(), Some("=A$1*2"));
        assert_eq!(step("=A$1*2", 4).as_deref(), Some("=$A1*2"));
        assert_eq!(step("=$A1*2", 4).as_deref(), Some("=A1*2"));
        assert_eq!(step("=SUM(B2:C3)", 10).as_deref(), Some("=SUM(B2:$C$3)"));
        assert_eq!(step("=SUM(", 5), None);
    }
}

/// The address a typed text is a link to, as Excel's オートコレクト makes
/// one: `http://…`, `https://…`, `ftp://…`, `www.…` (made `http://www.…`),
/// `mailto:…` or an email address (made `mailto:…`), all in one word.
pub(crate) fn link_target_of_typed(text: &str) -> Option<String> {
    let t = text.trim();
    if t.is_empty() || t.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    for scheme in ["http://", "https://", "ftp://", "file://"] {
        if lower.starts_with(scheme) && t.len() > scheme.len() {
            return Some(t.to_string());
        }
    }
    if lower.starts_with("mailto:") && t.len() > 7 {
        return Some(t.to_string());
    }
    if lower.starts_with("www.") && t.len() > 4 && t[4..].contains('.') {
        return Some(format!("http://{t}"));
    }
    if let Some((local, domain)) = t.split_once('@')
        && !local.is_empty()
        && !domain.contains('@')
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && local.chars().all(|c| c.is_ascii_alphanumeric() || "._%+-".contains(c))
        && domain.chars().all(|c| c.is_ascii_alphanumeric() || ".-".contains(c))
    {
        return Some(format!("mailto:{t}"));
    }
    None
}

#[cfg(test)]
mod link_tests {
    use super::link_target_of_typed as l;

    #[test]
    fn typed_addresses_become_links() {
        assert_eq!(l("https://example.com/a?b=1").as_deref(), Some("https://example.com/a?b=1"));
        assert_eq!(l("www.example.com").as_deref(), Some("http://www.example.com"));
        assert_eq!(l("user@example.com").as_deref(), Some("mailto:user@example.com"));
        assert_eq!(l("mailto:a@b.jp").as_deref(), Some("mailto:a@b.jp"));
        assert_eq!(l("see https://x.jp"), None);
        assert_eq!(l("abc"), None);
        assert_eq!(l("a@b"), None);
        assert_eq!(l("www.x"), None);
    }
}

impl SlideEditor {
    /// ⌃; and ⌘;: today's date (or the time now) in the selected cell, as a
    /// value (it does not change), or at the caret while typing.
    pub(crate) fn insert_now_stamp(&mut self, date: bool, cx: &mut Context<Self>) {
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let Some(now) = ibis_calc::eval::convert::now_serial(date1904) else { return };
        let (value, code) = if date { (now.floor(), "yyyy/m/d") } else { (now - now.floor(), "h:mm") };
        let text = ibis_calc::numfmt::format(&CellValue::Number(value), code, date1904).text;
        if self.cell_edit().is_some() {
            return self.cell_replace(None, &text, None, cx);
        }
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        self.begin_cell_edit(EditMode::Enter, false, cx);
        self.cell_replace(None, &text, None, cx);
        self.commit_cell_edit(cx);
    }
}

static CELL_EDIT_CLIPBOARD: std::sync::Mutex<Option<(String, Vec<CellRun>)>> = std::sync::Mutex::new(None);

impl SlideEditor {
    /// ⌘C and ⌘X while typing into a cell: the selected text on the system
    /// clipboard (nothing selected: nothing happens), cut taking it out.
    pub(crate) fn edit_copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        let Some(edit) = self.cell_edit() else { return };
        let Some(sel) = edit.selection() else { return };
        let (b0, b1) = (byte_at(&edit.text, sel.start), byte_at(&edit.text, sel.end));
        let text = edit.text[b0..b1].to_string();
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));

        // Save styled runs for internal paste
        let copied_runs = if let Some(runs) = &edit.runs {
            super::sheet_view::runs_between(runs, b0, b1)
        } else {
            vec![CellRun { text: text.clone(), ..Default::default() }]
        };
        if let Ok(mut clip) = CELL_EDIT_CLIPBOARD.lock() {
            *clip = Some((text, copied_runs));
        }

        if cut {
            self.replace_cell_text(b0, b1, "", cx);
        }
        cx.notify();
    }

    /// ⌘V while typing into a cell: the clipboard's text at the caret (over
    /// the selection), its line breaks kept as the cell's own.
    pub(crate) fn edit_paste(&mut self, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) else { return };
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let text = text.trim_end_matches('\n').to_string();
        if text.is_empty() {
            return;
        }
        let Some(edit) = self.cell_edit_mut() else { return };
        let range = edit.selection().unwrap_or(edit.caret..edit.caret);

        // Check if internal clipboard has styled runs matching this text
        let matched_runs = if let Ok(clip) = CELL_EDIT_CLIPBOARD.lock() {
            if let Some((clip_text, runs)) = clip.as_ref() {
                if clip_text == &text {
                    Some(runs.clone())
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        if let Some(runs) = matched_runs {
            edit.replace_runs(range, &text, &runs);
            cx.notify();
        } else {
            edit.replace(range, &text);
            cx.notify();
        }
    }
}

#[cfg(test)]
mod caret_items_tests {
    use super::{Cell, CellEdit, EditMode, Rc, SlideId, text_lines, utf16_len};

    #[test]
    fn caret_move_does_not_split_text_items() {
        let text = "Hello AV 日本語\n2行目 Test".to_string();
        let mut edit = CellEdit {
            slide: SlideId(1),
            column: 0,
            row: 0,
            text: text.clone(),
            caret: 0,
            marked: None,
            mode: EditMode::Edit,
            anchor: None,
            runs: None,
            pointed: None, reference_drag:false,
            rect: Rc::new(Cell::new(None)),
            pixels_per_point: Rc::new(Cell::new(1.0)),
            layout: Default::default(),
            dragging: None,
            dragging_in_bar: false,
            pending: None,
            scroll_y: Default::default(),
            followed_caret: Default::default(),
        };
        let baseline_items = edit.items();
        assert_eq!(baseline_items.len(), 1);
        assert_eq!(baseline_items[0].text, text);

        let total_u16 = utf16_len(&text);
        for pos in 0..=total_u16 {
            edit.caret = pos;
            edit.anchor = None;
            assert_eq!(edit.items(), baseline_items);
            for (start, line) in text_lines(&edit.text) {
                let bar = edit.bar_line(start, line);
                assert_eq!(bar.text, line.trim_end_matches('\r'));
            }
            if pos > 0 {
                edit.anchor = Some(0);
                assert_eq!(edit.items(), baseline_items);
            }
        }
    }
}

