// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 検索と置換 in the sidebar: finding cells matching text (with options: match case,
//! entire cell contents, look in formulas), finding next, finding all with a result list,
//! and replacing in one cell or all with undo/redo.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// Options controlling search and replace matching.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct FindOptions {
    /// Look in formulas too (Excel's 検索対象: 数式); otherwise only text
    /// and displayed values are matched.
    pub(crate) formulas: bool,
    /// Match case (Excel's 大文字と小文字を区別する).
    pub(crate) match_case: bool,
    /// Match entire cell contents (Excel's セル内容が完全に同一であるものを検索する).
    pub(crate) entire_cell: bool,
    /// Search across all visible sheets in the workbook (Excel's 検索場所: ブック).
    pub(crate) search_workbook: bool,
    /// Search column-by-column instead of row-by-row (Excel's 検索方向: 列).
    pub(crate) by_columns: bool,
    /// Search in cell notes and threaded comments too (Excel's 検索対象: メモ・コメント).
    pub(crate) comments: bool,
}

/// A matched cell from "Find All".
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FindMatch {
    pub(crate) slide: ibis_doc_model::SlideId,
    pub(crate) row: u32,
    pub(crate) column: u32,
    pub(crate) reference: String,
    pub(crate) text: String,
    pub(crate) formula: Option<String>,
}

pub(crate) struct FindPanel {
    pub(crate) query: Entity<InputState>,
    pub(crate) replacement: Entity<InputState>,
    pub(crate) replace_mode: bool,
    pub(crate) formulas: bool,
    pub(crate) match_case: bool,
    pub(crate) entire_cell: bool,
    pub(crate) search_workbook: bool,
    pub(crate) by_columns: bool,
    pub(crate) comments: bool,
    pub(crate) results: Vec<FindMatch>,
}

/// Check if `text` matches `query` under the given `options`.
pub(crate) fn is_match(text: &str, query: &str, options: FindOptions) -> bool {
    if query.is_empty() {
        return false;
    }
    if options.match_case {
        if options.entire_cell {
            text == query
        } else {
            text.contains(query)
        }
    } else {
        if options.entire_cell {
            text.eq_ignore_ascii_case(query) || text.to_lowercase() == query.to_lowercase()
        } else {
            text.to_lowercase().contains(&query.to_lowercase())
        }
    }
}

/// Replace `query` in `text` with `with` according to `options`.
pub(crate) fn replace_text_with_options(text: &str, query: &str, with: &str, options: FindOptions) -> Option<String> {
    if query.is_empty() {
        return None;
    }
    if options.entire_cell {
        if is_match(text, query, options) {
            return Some(with.to_string());
        }
        return None;
    }
    if options.match_case {
        if text.contains(query) {
            return Some(text.replace(query, with));
        }
        return None;
    }
    // Case-insensitive partial replacement
    let lower = text.to_lowercase();
    let q = query.to_lowercase();
    if !lower.contains(&q) || lower.len() != text.len() {
        return text.contains(query).then(|| text.replace(query, with));
    }
    let mut out = String::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find(&q) {
        out.push_str(&text[at..at + i]);
        out.push_str(with);
        at += i + q.len();
    }
    out.push_str(&text[at..]);
    Some(out)
}

/// Replace `query` in `text` without regard to case (legacy default).
#[allow(dead_code)]
pub(crate) fn replace_text(text: &str, query: &str, with: &str) -> Option<String> {
    replace_text_with_options(text, query, with, FindOptions::default())
}

fn searched_in_sheet(sheet: &ibis_doc_model::Sheet, row: u32, column: u32, formulas: bool) -> Option<String> {
    let c = sheet.cell(row, column)?;
    match (&c.formula, formulas) {
        (Some(f), true) => Some(format!("={f}")),
        _ => (!c.text.is_empty()).then(|| c.text.clone()),
    }
}

fn match_cell_in_sheet(
    sheet: &ibis_doc_model::Sheet,
    row: u32,
    column: u32,
    query: &str,
    options: FindOptions,
) -> Option<(String, Option<String>)> {
    if let Some(target) = searched_in_sheet(sheet, row, column, options.formulas) {
        if is_match(&target, query, options) {
            let c = sheet.cell(row, column)?;
            return Some((c.text.clone(), c.formula.as_ref().map(|f| format!("={f}"))));
        }
    }
    if options.comments {
        if let Some(note) = sheet.comment_at(row, column) {
            if is_match(&note.text, query, options) {
                return Some((note.text.clone(), None));
            }
        }
        if let Some(thread) = sheet.thread_at(row, column) {
            for post in &thread.posts {
                if is_match(&post.text, query, options) {
                    return Some((post.text.clone(), None));
                }
            }
        }
    }
    None
}

fn sheet_candidate_coords(sheet: &ibis_doc_model::Sheet, include_comments: bool) -> Vec<(u32, u32)> {
    let mut coords: Vec<(u32, u32)> = sheet.cells.iter().map(|c| (c.row, c.column)).collect();
    if include_comments {
        for n in &sheet.comments {
            coords.push((n.row, n.column));
        }
        for t in &sheet.threads {
            coords.push((t.row, t.column));
        }
    }
    coords.sort_unstable();
    coords.dedup();
    coords
}

fn replacement_in_sheet(
    sheet: &ibis_doc_model::Sheet,
    row: u32,
    column: u32,
    query: &str,
    with: &str,
    options: FindOptions,
) -> Option<CellEdit> {
    let c = sheet.cell(row, column)?;
    let input = match (&c.formula, &c.value) {
        (Some(f), _) if options.formulas => {
            let formula_str = format!("={f}");
            let rep = replace_text_with_options(&formula_str, query, with, options)?;
            let clean_f = rep.strip_prefix('=').unwrap_or(&rep).to_string();
            CellInput::Formula(clean_f)
        }
        (Some(_), _) => return None,
        (None, CellValue::Text(t)) => CellInput::Value(CellValue::Text(replace_text_with_options(t, query, with, options)?)),
        (None, v @ (CellValue::Number(_) | CellValue::Bool(_))) => {
            let text = replace_text_with_options(&c.text, query, with, options)?;
            let _ = v;
            match ibis_calc::input::parse_input(&text, false) {
                ibis_calc::input::Typed::Value(v, _) => CellInput::Value(v),
                ibis_calc::input::Typed::Formula(f) => CellInput::Formula(f),
            }
        }
        _ => return None,
    };
    Some(CellEdit { row, column, input })
}

impl SlideEditor {
    pub(crate) fn open_sheet_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.find-what")));
        let replacement = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.replace-with")));
        query.update(cx, |q, cx| q.focus(window, cx));
        if let Some(s) = self.sheet_state_mut() {
            s.names = None;
            s.validation = None;
            s.find = Some(FindPanel {
                query,
                replacement,
                replace_mode: false,
                formulas: false,
                match_case: false,
                entire_cell: false,
                search_workbook: false,
                by_columns: false,
                comments: false,
                results: Vec::new(),
            });
            s.page_setup = false;
            s.custom_format = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    fn find_target_sheets(&self, search_workbook: bool) -> Vec<(ibis_doc_model::SlideId, String)> {
        if !search_workbook {
            let slide = self.current_slide;
            let name = self
                .presentation()
                .workbook
                .as_ref()
                .and_then(|w| w.sheets.iter().find(|s| s.slide == Some(slide)).map(|s| s.name.clone()))
                .or_else(|| self.presentation().slide(slide).and_then(|s| s.name.clone()))
                .unwrap_or_default();
            return vec![(slide, name)];
        }
        if let Some(w) = self.presentation().workbook.as_ref() {
            let list: Vec<(ibis_doc_model::SlideId, String)> = w
                .sheets
                .iter()
                .filter_map(|s| s.slide.map(|id| (id, s.name.clone())))
                .collect();
            if !list.is_empty() {
                return list;
            }
        }
        self.presentation()
            .slides
            .iter()
            .filter(|sl| ibis_doc_model::sheet_calc::sheet_of_slide(sl).is_some())
            .map(|sl| (sl.id, sl.name.clone().unwrap_or_default()))
            .collect()
    }

    fn find_step_cell_with_options(
        &mut self,
        query: &str,
        options: FindOptions,
        forward: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        self.commit_cell_edit(cx);
        if query.is_empty() {
            return false;
        }
        let sheets = self.find_target_sheets(options.search_workbook);
        if sheets.is_empty() {
            return false;
        }
        let pos_key = |r: u32, c: u32| if options.by_columns { (c, r) } else { (r, c) };
        let cur_slide = self.current_slide;
        let cur_sheet_idx = sheets.iter().position(|(id, _)| *id == cur_slide).unwrap_or(0);
        let (cur_col, cur_row) = self.current_cell().unwrap_or((0, 0));
        let cur_key = (cur_sheet_idx, pos_key(cur_row, cur_col));

        let mut hits: Vec<((usize, (u32, u32)), ibis_doc_model::SlideId, u32, u32)> = Vec::new();
        for (sheet_idx, (slide_id, _)) in sheets.iter().enumerate() {
            let Some(slide) = self.presentation().slide(*slide_id) else { continue };
            let Some(sheet) = ibis_doc_model::sheet_calc::sheet_of_slide(slide) else { continue };
            for (r, c) in sheet_candidate_coords(sheet, options.comments) {
                if match_cell_in_sheet(sheet, r, c, query, options).is_some() {
                    hits.push(((sheet_idx, pos_key(r, c)), *slide_id, r, c));
                }
            }
        }
        hits.sort_by_key(|h| h.0);

        let chosen = if forward {
            hits.iter()
                .find(|(k, _, _, _)| *k > cur_key)
                .or_else(|| hits.first())
                .copied()
        } else {
            hits.iter()
                .rev()
                .find(|(k, _, _, _)| *k < cur_key)
                .or_else(|| hits.last())
                .copied()
        };

        match chosen {
            Some((_, slide, row, column)) => {
                self.jump_to_sheet_cell(slide, row, column, cx);
                self.status = None;
                cx.notify();
                true
            }
            None => {
                self.status = Some(crate::shell::i18n::format("_xlsx.cannot-find-query", &[("query", &query)]).into());
                cx.notify();
                false
            }
        }
    }

    /// Select the next cell matching `query` under `options`. Whether one was found.
    pub(crate) fn find_next_cell_with_options(&mut self, query: &str, options: FindOptions, cx: &mut Context<Self>) -> bool {
        self.find_step_cell_with_options(query, options, true, cx)
    }

    /// Select the previous cell matching `query` under `options`. Whether one was found.
    pub(crate) fn find_prev_cell_with_options(&mut self, query: &str, options: FindOptions, cx: &mut Context<Self>) -> bool {
        self.find_step_cell_with_options(query, options, false, cx)
    }

    /// Select the next cell matching `query` (legacy default options).
    pub(crate) fn find_next_cell(&mut self, query: &str, formulas: bool, cx: &mut Context<Self>) -> bool {
        self.find_next_cell_with_options(query, FindOptions { formulas, ..Default::default() }, cx)
    }

    /// Find all cells matching `query` under `options`, updating results list and selecting first match.
    pub(crate) fn find_all_cells(&mut self, query: &str, options: FindOptions, cx: &mut Context<Self>) -> Vec<FindMatch> {
        self.commit_cell_edit(cx);
        if query.is_empty() {
            if let Some(p) = self.sheet_state_mut().and_then(|s| s.find.as_mut()) {
                p.results.clear();
            }
            cx.notify();
            return Vec::new();
        }
        let sheets = self.find_target_sheets(options.search_workbook);
        let pos_key = |r: u32, c: u32| if options.by_columns { (c, r) } else { (r, c) };
        let mut matches = Vec::new();
        for (slide_id, sheet_name) in &sheets {
            let Some(slide) = self.presentation().slide(*slide_id) else { continue };
            let Some(sheet) = ibis_doc_model::sheet_calc::sheet_of_slide(slide) else { continue };
            let mut sheet_matches = Vec::new();
            for (r, c) in sheet_candidate_coords(sheet, options.comments) {
                if let Some((text, formula)) = match_cell_in_sheet(sheet, r, c, query, options) {
                    let cell_ref = ibis_doc_model::cell_name(c, r);
                    let reference = if options.search_workbook && !sheet_name.is_empty() {
                        format!("{}!{cell_ref}", ibis_calc::formula::quote_sheet(sheet_name))
                    } else {
                        cell_ref
                    };
                    sheet_matches.push(FindMatch {
                        slide: *slide_id,
                        row: r,
                        column: c,
                        reference,
                        text,
                        formula,
                    });
                }
            }
            sheet_matches.sort_by_key(|m| pos_key(m.row, m.column));
            matches.extend(sheet_matches);
        }

        if matches.is_empty() {
            self.status = Some(crate::shell::i18n::format("_xlsx.cannot-find-query", &[("query", &query)]).into());
        } else {
            let n = matches.len();
            self.status = Some(crate::shell::i18n::format("_xlsx.find-matches-count", &[("n", &n)]).into());
            let first = &matches[0];
            self.jump_to_sheet_cell(first.slide, first.row, first.column, cx);
        }

        if let Some(p) = self.sheet_state_mut().and_then(|s| s.find.as_mut()) {
            p.results = matches.clone();
        }
        cx.notify();
        matches
    }

    /// Replace in the selected cell, then go to the next.
    pub(crate) fn replace_one_with_options(&mut self, query: &str, with: &str, options: FindOptions, cx: &mut Context<Self>) {
        if let Some((column, row)) = self.current_cell() {
            let slide = self.current_slide;
            let mut commands = Vec::new();
            if let Some((_, sheet)) = self.current_sheet() {
                if let Some(edit) = replacement_in_sheet(sheet, row, column, query, with, options) {
                    commands.push(Command::Sheet(SheetCommand::SetCells { slide, edits: vec![edit] }));
                }
                if options.comments
                    && let Some(note) = sheet.comment_at(row, column)
                    && let Some(new_text) = replace_text_with_options(&note.text, query, with, options)
                {
                    commands.push(Command::Sheet(SheetCommand::SetComment {
                        slide,
                        row,
                        column,
                        comment: Some(ibis_doc_model::SheetComment {
                            text: new_text,
                            ..note.clone()
                        }),
                    }));
                }
            }
            if !commands.is_empty()
                && let Err(err) = self.document.execute(commands)
            {
                log::warn!("replace rejected: {err}");
            }
        }
        self.find_next_cell_with_options(query, options, cx);
    }

    /// Legacy replace_one.
    #[allow(dead_code)]
    pub(crate) fn replace_one(&mut self, query: &str, with: &str, formulas: bool, cx: &mut Context<Self>) {
        self.replace_one_with_options(query, with, FindOptions { formulas, ..Default::default() }, cx);
    }

    /// Replace in every matching cell of the sheet (or workbook when `options.search_workbook` is set) in one atomic step.
    pub(crate) fn replace_all_cells_with_options(&mut self, query: &str, with: &str, options: FindOptions, cx: &mut Context<Self>) -> usize {
        self.commit_cell_edit(cx);
        let sheets = self.find_target_sheets(options.search_workbook);
        let mut commands = Vec::new();
        let mut total_replaced = 0usize;

        for (slide, _) in sheets {
            let Some(sl) = self.presentation().slide(slide) else { continue };
            let Some(sheet) = ibis_doc_model::sheet_calc::sheet_of_slide(sl) else { continue };
            let positions: Vec<(u32, u32)> = sheet.cells.iter().map(|c| (c.row, c.column)).collect();
            let edits: Vec<CellEdit> = positions
                .into_iter()
                .filter_map(|(r, c)| replacement_in_sheet(sheet, r, c, query, with, options))
                .collect();
            if !edits.is_empty() {
                total_replaced += edits.len();
                commands.push(Command::Sheet(SheetCommand::SetCells { slide, edits }));
            }
            if options.comments {
                let notes = sheet.comments.clone();
                for note in notes {
                    if let Some(new_text) = replace_text_with_options(&note.text, query, with, options) {
                        total_replaced += 1;
                        commands.push(Command::Sheet(SheetCommand::SetComment {
                            slide,
                            row: note.row,
                            column: note.column,
                            comment: Some(ibis_doc_model::SheetComment {
                                text: new_text,
                                ..note
                            }),
                        }));
                    }
                }
            }
        }

        if !commands.is_empty()
            && let Err(err) = self.document.execute(commands)
        {
            log::warn!("replace all rejected: {err}");
        }
        self.status = Some(crate::shell::i18n::format("_xlsx.replaced-n-occurrence-s", &[("n", &total_replaced)]).into());
        // Also refresh find all results if panel is open
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.find.as_mut()) {
            p.results.clear();
        }
        cx.notify();
        total_replaced
    }

    /// Legacy replace_all_cells.
    pub(crate) fn replace_all_cells(&mut self, query: &str, with: &str, formulas: bool, cx: &mut Context<Self>) -> usize {
        self.replace_all_cells_with_options(query, with, FindOptions { formulas, ..Default::default() }, cx)
    }

    /// Jump to a specific sheet and cell in the workbook.
    pub(crate) fn jump_to_sheet_cell(&mut self, slide: ibis_doc_model::SlideId, row: u32, column: u32, cx: &mut Context<Self>) {
        self.current_slide = slide;
        self.selection = None;
        let s = self.wb_mut();
        s.cells.insert(slide, (column, row));
        s.cell_ends.remove(&slide);
        self.scroll_range_into_view(true);
        cx.notify();
    }

    /// Jump to a specific cell in the current worksheet.
    pub(crate) fn jump_to_cell(&mut self, row: u32, column: u32, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        self.jump_to_sheet_cell(slide, row, column, cx);
    }

    pub(crate) fn sheet_find_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.sheet_state()?.find.as_ref()?;
        let (query, replacement) = (panel.query.clone(), panel.replacement.clone());
        let (formulas, match_case, entire_cell, search_workbook, by_columns, comments) = (
            panel.formulas,
            panel.match_case,
            panel.entire_cell,
            panel.search_workbook,
            panel.by_columns,
            panel.comments,
        );
        let results = panel.results.clone();
        let replace_mode = panel.replace_mode;

        let get_values = |this: &SlideEditor, cx: &App| -> (String, String, FindOptions) {
            let p = this.sheet_state().and_then(|s| s.find.as_ref());
            let q = p.map(|p| p.query.read(cx).value().to_string()).unwrap_or_default();
            let r = p.map(|p| p.replacement.read(cx).value().to_string()).unwrap_or_default();
            let opts = p.map_or(FindOptions::default(), |p| FindOptions {
                formulas: p.formulas,
                match_case: p.match_case,
                entire_cell: p.entire_cell,
                search_workbook: p.search_workbook,
                by_columns: p.by_columns,
                comments: p.comments,
            });
            (q, r, opts)
        };

        let current_slide = self.current_slide;
        let current_cell = self.current_cell();

        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                .child(crate::shell::chrome::radio_button("xlsx-find-mode-search", !replace_mode, false, crate::shell::i18n::text("_shell.find"), cx.listener(|this, _, _, cx| { if let Some(f) = this.sheet_state_mut().and_then(|s| s.find.as_mut()) { f.replace_mode = false; } cx.notify(); })))
                .child(crate::shell::chrome::radio_button("xlsx-find-mode-replace", replace_mode, false, crate::shell::i18n::text("_shell.replace"), cx.listener(|this, _, _, cx| { if let Some(f) = this.sheet_state_mut().and_then(|s| s.find.as_mut()) { f.replace_mode = true; } cx.notify(); }))))
            .child(crate::shell::chrome::text_field(&query).full_width())
            .when(replace_mode, |d| d.child(crate::shell::chrome::text_field(&replacement).full_width()))
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        crate::shell::chrome::checkbox_button("find-match-case",crate::shell::i18n::text("_shell.match-case"),match_case,false,cx.listener(move |this, _, _, cx| {
                                if let Some(p) = this.sheet_state_mut().and_then(|s| s.find.as_mut()) {
                                    p.match_case = !match_case;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        crate::shell::chrome::checkbox_button("find-formulas",crate::shell::i18n::text("_xlsx.include-formulas"),formulas,false,cx.listener(move |this, _, _, cx| {
                                if let Some(p) = this.sheet_state_mut().and_then(|s| s.find.as_mut()) {
                                    p.formulas = !formulas;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        crate::shell::chrome::checkbox_button("find-entire-cell",crate::shell::i18n::text("xlsx.match-entire-cell"),entire_cell,false,cx.listener(move |this, _, _, cx| {
                                if let Some(p) = this.sheet_state_mut().and_then(|s| s.find.as_mut()) {
                                    p.entire_cell = !entire_cell;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        crate::shell::chrome::checkbox_button("find-workbook",crate::shell::i18n::text("xlsx.find-within-workbook"),search_workbook,false,cx.listener(move |this, _, _, cx| {
                                if let Some(p) = this.sheet_state_mut().and_then(|s| s.find.as_mut()) {
                                    p.search_workbook = !search_workbook;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        crate::shell::chrome::checkbox_button("find-by-columns",crate::shell::i18n::text("xlsx.find-by-columns"),by_columns,false,cx.listener(move |this, _, _, cx| {
                                if let Some(p) = this.sheet_state_mut().and_then(|s| s.find.as_mut()) {
                                    p.by_columns = !by_columns;
                                }
                                cx.notify();
                            })),
                    )
                    .child(
                        crate::shell::chrome::checkbox_button("find-comments",crate::shell::i18n::text("xlsx.find-include-comments"),comments,false,cx.listener(move |this, _, _, cx| {
                                if let Some(p) = this.sheet_state_mut().and_then(|s| s.find.as_mut()) {
                                    p.comments = !comments;
                                }
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .child(crate::shell::chrome::action_button("xfind-prev").small().label(crate::shell::i18n::text("_shell.previous")).on_click(cx.listener(move |this, _, _, cx| {
                        let (q, _, opts) = get_values(this, cx);
                        this.find_prev_cell_with_options(&q, opts, cx);
                    })))
                    .child(crate::shell::chrome::action_button("xfind-next").small().label(crate::shell::i18n::text("_shell.next")).on_click(cx.listener(move |this, _, _, cx| {
                        let (q, _, opts) = get_values(this, cx);
                        this.find_next_cell_with_options(&q, opts, cx);
                    })))
                    .child(crate::shell::chrome::action_button("xfind-all").small().label(crate::shell::i18n::text("_xlsx.all")).on_click(cx.listener(move |this, _, _, cx| {
                        let (q, _, opts) = get_values(this, cx);
                        this.find_all_cells(&q, opts, cx);
                    })))
                    .when(replace_mode, |d| d.child(crate::shell::chrome::action_button("xreplace-one").small().label(crate::shell::i18n::text("_shell.replace")).on_click(cx.listener(move |this, _, _, cx| {
                        let (q, r, opts) = get_values(this, cx);
                        this.replace_one_with_options(&q, &r, opts, cx);
                    })))
                    .child(crate::shell::chrome::action_button("xreplace-all").small().label(crate::shell::i18n::text("_shell.replace-all")).on_click(cx.listener(move |this, _, _, cx| {
                        let (q, r, opts) = get_values(this, cx);
                        this.replace_all_cells_with_options(&q, &r, opts, cx);
                    })))),
            );

        if !results.is_empty() {
            let mut list = div().id("find-results-list").flex().flex_col().gap_1().max_h(px(200.)).overflow_y_scroll();
            for m in &results {
                let is_sel = m.slide == current_slide && current_cell == Some((m.column, m.row));
                let (slide, r, c) = (m.slide, m.row, m.column);
                let display_text = if let Some(ref f) = m.formula {
                    format!("{} ({})", m.text, f)
                } else {
                    m.text.clone()
                };
                list = list.child(
                    h_flex()
                        .gap_2()
                        .px(px(4.))
                        .py(px(2.))
                        .rounded(px(3.))
                        .cursor_pointer()
                        .when(is_sel, |el| el.bg(rgb(0xCCE8FF)))
                        .hover(|el| if is_sel { el.bg(rgb(0xB8DCFF)) } else { el.bg(rgb(0xF0F0F0)) })
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| {
                            this.jump_to_sheet_cell(slide, r, c, cx);
                        }))
                        .child(div().min_w(px(36.)).text_sm().font_weight(FontWeight::BOLD).text_color(rgb(0x107C41)).child(m.reference.clone()))
                        .child(div().flex_1().text_sm().text_ellipsis().child(display_text)),
                );
            }
            body = body
                .child(div().h(px(1.)).bg(rgb(0xE0E0E0)).my(px(4.)))
                .child(h_flex().justify_between().child(div().text_sm().font_weight(FontWeight::BOLD).child(crate::shell::i18n::text("xlsx.find-results"))).child(div().text_sm().text_color(rgb(0x666666)).child(format!("{} hits", results.len()))))
                .child(list);
        }

        Some(SidebarPanel { title: "_shell.find-and-replace".into(), body: body.into_any_element() })
    }
}

#[cfg(test)]
mod tests {
    use super::{is_match, replace_text_with_options, FindOptions};

    #[test]
    fn test_is_match() {
        let opt_def = FindOptions::default();
        assert!(is_match("Apple Store", "apple", opt_def));
        assert!(is_match("Apple Store", "STORE", opt_def));
        assert!(!is_match("Apple Store", "banana", opt_def));

        let opt_case = FindOptions { match_case: true, ..Default::default() };
        assert!(is_match("Apple Store", "Apple", opt_case));
        assert!(!is_match("Apple Store", "apple", opt_case));

        let opt_entire = FindOptions { entire_cell: true, ..Default::default() };
        assert!(is_match("100", "100", opt_entire));
        assert!(is_match("Apple", "apple", opt_entire));
        assert!(!is_match("Apple Store", "Apple", opt_entire));

        let opt_case_entire = FindOptions { match_case: true, entire_cell: true, ..Default::default() };
        assert!(is_match("Apple", "Apple", opt_case_entire));
        assert!(!is_match("Apple", "apple", opt_case_entire));
    }

    #[test]
    fn test_replacing_with_options() {
        let opt_def = FindOptions::default();
        assert_eq!(replace_text_with_options("北町店の店舗", "店舗", "支店", opt_def).as_deref(), Some("北町店の支店"));
        assert_eq!(replace_text_with_options("Apple apple", "APPLE", "pear", opt_def).as_deref(), Some("pear pear"));
        assert_eq!(replace_text_with_options("abc", "x", "y", opt_def), None);

        let opt_case = FindOptions { match_case: true, ..Default::default() };
        assert_eq!(replace_text_with_options("Apple apple", "Apple", "Pear", opt_case).as_deref(), Some("Pear apple"));

        let opt_entire = FindOptions { entire_cell: true, ..Default::default() };
        assert_eq!(replace_text_with_options("Apple", "apple", "Orange", opt_entire).as_deref(), Some("Orange"));
        assert_eq!(replace_text_with_options("Apple Tree", "Apple", "Orange", opt_entire), None);
    }
}

