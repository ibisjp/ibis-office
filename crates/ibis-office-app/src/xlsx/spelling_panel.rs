// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スペル チェック (ツール > スペル > スペルチェック…): the words the system's
//! spell checker does not know in the text values of the cells (not formulas),
//! one at a time in the shared side panel (`shell/spell_panel.rs`). A range
//! selected: its cells; one cell: the whole sheet from that cell on, round to
//! the cells before it (as Excel checks).
use std::collections::HashSet;

use gpui_kit::*;
use ibis_doc_model::{CellEdit, CellInput, CellRange, CellValue, Command, Sheet, SheetCommand, ShapeId, Slide};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

pub(crate) struct SpellPanel {
    pub(crate) issues: Vec<Issue>,
    pub(crate) current: usize,
    /// Words skipped for this check (すべて無視).
    pub(crate) ignored: HashSet<String>,
    /// The cells checked (`None`: the sheet) and where a check of the sheet starts.
    pub(crate) range: Option<CellRange>,
    pub(crate) from: (u32, u32),
    pub(crate) suggestions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Issue {
    pub(crate) row: u32,
    pub(crate) column: u32,
    /// The shape whose text the word is in (`row` and `column` are 0 then).
    pub(crate) shape: Option<ShapeId>,
    /// Character offsets in the cell's text.
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) word: String,
}

/// The text a cell is checked in: a text value that is not a formula's result.
fn checked_text(cell: &ibis_doc_model::SheetCell) -> Option<&str> {
    match (&cell.value, &cell.formula) {
        (CellValue::Text(t), None) if !t.is_empty() => Some(t.as_str()),
        _ => None,
    }
}

/// Every word not known in the cells, in reading order (row by row); a check of
/// the whole sheet (`range` is `None`) starts at the cell `from`.
pub(crate) fn scan(sheet: &Sheet, range: Option<CellRange>, from: (u32, u32), ignored: &HashSet<String>) -> Vec<Issue> {
    let mut cells: Vec<&ibis_doc_model::SheetCell> = sheet
        .cells
        .iter()
        .filter(|c| range.is_none_or(|r| c.row >= r.first_row && c.row <= r.last_row && c.column >= r.first_column && c.column <= r.last_column))
        .collect();
    cells.sort_by_key(|c| (c.row, c.column));
    let mut found = Vec::new();
    for c in cells {
        let Some(text) = checked_text(c) else { continue };
        for m in crate::spelling::check_by_settings(text, None) {
            if !ignored.contains(&m.word) {
                found.push(Issue { row: c.row, column: c.column, shape: None, start: m.start, end: m.end, word: m.word });
            }
        }
    }
    if range.is_none() {
        let at = found.iter().position(|i| (i.row, i.column) >= from).unwrap_or(0);
        found.rotate_left(at);
    }
    found
}

/// The cells' words, and (a check of the whole sheet) after them the words in
/// the text of the sheet's shapes (text boxes, shapes), back to front as drawn.
pub(crate) fn scan_all(slide: &Slide, sheet: &Sheet, range: Option<CellRange>, from: (u32, u32), ignored: &HashSet<String>) -> Vec<Issue> {
    let mut found = scan(sheet, range, from, ignored);
    if range.is_none() {
        for shape in slide.shown_shapes() {
            let Some(body) = shape.text_body() else { continue };
            for m in crate::spelling::check_by_settings(&body.text(), None) {
                if !ignored.contains(&m.word) {
                    found.push(Issue { row: 0, column: 0, shape: Some(shape.id), start: m.start, end: m.end, word: m.word });
                }
            }
        }
    }
    found
}

impl SlideEditor {
    fn spelling_scan(&self, range: Option<CellRange>, from: (u32, u32), ignored: &HashSet<String>) -> Vec<Issue> {
        self.current_sheet().map(|(_, sheet)| scan_all(self.slide(), sheet, range, from, ignored)).unwrap_or_default()
    }

    /// ツール > スペル > スペルチェック…: the panel, at the first word.
    pub(crate) fn open_sheet_spelling(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((column, row)) = self.current_cell() else { return };
        let range = self.selected_range().filter(|r| r.first_row != r.last_row || r.first_column != r.last_column);
        let ignored = HashSet::new();
        let issues = self.spelling_scan(range, (row, column), &ignored);
        if let Some(s) = self.sheet_state_mut() {
            s.find = None;
            s.names = None;
            s.validation = None;
            s.page_setup = false;
            s.custom_format = None;
            s.conditional_edit = None;
            s.sort_edit = None;
            s.close_settings();
            s.spelling = Some(SpellPanel { issues, current: 0, ignored, range, from: (row, column), suggestions: Vec::new() });
        }
        self.sidebar_open = true;
        self.sheet_spelling_goto(0, cx);
    }

    pub(crate) fn close_sheet_spelling(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.spelling = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn spelling_issue(&self) -> Option<Issue> {
        let p = self.sheet_state()?.spelling.as_ref()?;
        p.issues.get(p.current).cloned()
    }

    /// The word number `index` shown: its cell selected, its suggestions.
    fn sheet_spelling_goto(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.spelling.as_ref()) else { return };
        let Some(issue) = p.issues.get(index).cloned() else {
            if let Some(p) = self.sheet_state_mut().and_then(|s| s.spelling.as_mut()) {
                p.current = 0;
                p.suggestions.clear();
            }
            return cx.notify();
        };
        let slide = self.current_slide;
        let suggestions = match issue.shape {
            Some(id) => self.slide().shape(id).and_then(|s| s.text_body()).map(|b| crate::spelling::suggestions(&b.text(), issue.start, issue.end, None)).unwrap_or_default(),
            None => self
                .current_sheet()
                .and_then(|(_, sheet)| sheet.cell(issue.row, issue.column).and_then(checked_text).map(|t| crate::spelling::suggestions(t, issue.start, issue.end, None)))
                .unwrap_or_default(),
        };
        match issue.shape {
            // A shape's word: the shape selected.
            Some(id) => self.set_selection(vec![id]),
            None => {
                self.set_selection(Vec::new());
                let s = self.wb_mut();
                s.cells.insert(slide, (issue.column, issue.row));
                s.cell_ends.remove(&slide);
            }
        }
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.spelling.as_mut()) {
            p.current = index;
            p.suggestions = suggestions;
        }
        cx.notify();
    }

    pub(crate) fn sheet_spelling_step(&mut self, forward: bool, _: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.spelling.as_ref()) else { return };
        let n = p.issues.len();
        if n == 0 {
            return;
        }
        let index = if forward { (p.current + 1) % n } else { (p.current + n - 1) % n };
        self.sheet_spelling_goto(index, cx);
    }

    /// The checks made again (after a change), at the word number `near`.
    fn sheet_spelling_rescan(&mut self, near: usize, cx: &mut Context<Self>) {
        let Some((ignored, range, from)) = self.sheet_state().and_then(|s| s.spelling.as_ref()).map(|p| (p.ignored.clone(), p.range, p.from)) else { return };
        let issues = self.spelling_scan(range, from, &ignored);
        let index = near.min(issues.len().saturating_sub(1));
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.spelling.as_mut()) {
            p.issues = issues;
        }
        self.sheet_spelling_goto(index, cx);
    }

    /// After an undo or redo: the panel's list checked again, at the word shown.
    pub(crate) fn sheet_spelling_refresh(&mut self, cx: &mut Context<Self>) {
        let Some(at) = self.sheet_state().and_then(|s| s.spelling.as_ref()).map(|p| p.current) else { return };
        self.sheet_spelling_rescan(at, cx);
    }

    /// 変更 (or すべて変更: the same word everywhere): the word replaced by `text`, in one step.
    pub(crate) fn sheet_spelling_change(&mut self, text: &str, all: bool, _: &mut Window, cx: &mut Context<Self>) {
        let Some(issue) = self.spelling_issue() else { return };
        let Some(p) = self.sheet_state().and_then(|s| s.spelling.as_ref()) else { return };
        let at = p.current;
        let mut hits: Vec<Issue> = if all { p.issues.iter().filter(|i| i.word == issue.word).cloned().collect() } else { vec![issue] };
        // Back to front within every cell: the earlier offsets stay valid.
        hits.sort_by_key(|i| (i.row, i.column, std::cmp::Reverse(i.start)));
        let mut edits: Vec<CellEdit> = Vec::new();
        let mut stale = false;
        let mut commands: Vec<Command> = Vec::new();
        // The words in shapes' texts (back to front inside a text).
        for hit in hits.iter().filter(|h| h.shape.is_some()) {
            let id = hit.shape.expect("a shape");
            let body_text = self.slide().shape(id).and_then(|s| s.text_body()).map(|b| b.text());
            let ok = body_text.is_some_and(|t| t.chars().skip(hit.start).take(hit.end - hit.start).collect::<String>() == hit.word);
            if !ok {
                stale = true;
                break;
            }
            commands.push(Command::ReplaceText { slide: self.current_slide, shape: id, at: hit.start, remove: hit.end - hit.start, insert: text.into() });
        }
        hits.retain(|h| h.shape.is_none());
        if let Some((_, sheet)) = self.current_sheet() {
            let mut current: Option<(u32, u32, Vec<char>)> = None;
            for hit in &hits {
                if current.as_ref().is_none_or(|(r, c, _)| (*r, *c) != (hit.row, hit.column)) {
                    if let Some((row, column, chars)) = current.take() {
                        edits.push(CellEdit { row, column, input: CellInput::Value(CellValue::Text(chars.into_iter().collect())) });
                    }
                    let Some(t) = sheet.cell(hit.row, hit.column).and_then(checked_text) else {
                        stale = true;
                        break;
                    };
                    current = Some((hit.row, hit.column, t.chars().collect()));
                }
                let (_, _, chars) = current.as_mut().expect("the cell's text");
                if hit.end > chars.len() || chars[hit.start..hit.end].iter().collect::<String>() != hit.word {
                    stale = true;
                    break;
                }
                chars.splice(hit.start..hit.end, text.chars());
            }
            if let (false, Some((row, column, chars))) = (stale, current.take()) {
                edits.push(CellEdit { row, column, input: CellInput::Value(CellValue::Text(chars.into_iter().collect())) });
            }
        }
        if !stale && (!edits.is_empty() || !commands.is_empty()) {
            let slide = self.current_slide;
            if !edits.is_empty() {
                commands.insert(0, Command::Sheet(SheetCommand::SetCells { slide, edits }));
            }
            if let Err(err) = self.document.execute(commands) {
                log::warn!("spelling change rejected: {err}");
            }
        }
        self.sheet_spelling_rescan(at, cx);
    }

    /// 無視 (or すべて無視): this word (or this spelling everywhere) is not told again.
    pub(crate) fn sheet_spelling_ignore(&mut self, all: bool, _: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state_mut().and_then(|s| s.spelling.as_mut()) else { return };
        let Some(issue) = p.issues.get(p.current).cloned() else { return };
        let at = p.current;
        if all {
            p.ignored.insert(issue.word);
            self.sheet_spelling_rescan(at, cx);
        } else {
            p.issues.remove(at);
            let left = p.issues.len();
            self.sheet_spelling_goto(at.min(left.saturating_sub(1)), cx);
        }
    }

    /// 辞書に追加: the word learned by the system's dictionary.
    pub(crate) fn sheet_spelling_learn(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(issue) = self.spelling_issue() else { return };
        crate::spelling::learn(&issue.word);
        let at = self.sheet_state().and_then(|s| s.spelling.as_ref()).map_or(0, |p| p.current);
        self.sheet_spelling_rescan(at, cx);
    }

    pub(crate) fn sheet_spelling_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.spelling.as_ref()?;
        let current = p.current.min(p.issues.len().saturating_sub(1));
        let view = crate::shell::spell_panel::SpellView { prefix: "xlsx-spell", current, total: p.issues.len(), word: p.issues.get(current).map(|i| i.word.as_str()), suggestions: &p.suggestions };
        let actions = crate::shell::spell_panel::SpellActions {
            change: Self::sheet_spelling_change,
            ignore: Self::sheet_spelling_ignore,
            learn: Self::sheet_spelling_learn,
            step: Self::sheet_spelling_step,
            close: Self::close_sheet_spelling,
        };
        Some(crate::shell::spell_panel::sidebar(view, actions, cx))
    }
}
