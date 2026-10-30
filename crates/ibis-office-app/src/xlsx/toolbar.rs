// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A workbook's toolbar and menus (the mock's ⑤-4 「スプレッドシート：セル
//! を選択中」): the buttons show the selected cell's format and change the
//! selected range's; the menus insert and delete rows, columns and sheets.
//! The editing itself is the model's commands (`sheet_command.rs`).

use gpui_kit::*;
use ibis_doc_model::{
    Border, BorderLine, CellAlign, CellRange, CellStyle, CellVerticalAlign, Command, Edge, Rgb, SheetCommand, SlideId, StyleChange, sheet_command::Axis,
};

use crate::shell::chrome::{Handler, MenuEntry, MenuItem, Menus, Submenu, ToolButton, ToolItem};
use crate::shell::editor::SlideEditor;

fn sep() -> ToolItem {
    ToolItem::Separator
}

fn handler(f: impl Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) -> Handler {
    std::rc::Rc::new(f)
}

fn fill_submenu(menus: &mut Menus, id: &str, items: Vec<MenuEntry>) {
    fn find<'a>(entries: &'a mut [MenuEntry], id: &str) -> Option<&'a mut Submenu> {
        for entry in entries {
            if let MenuEntry::Submenu(sub) = entry {
                if sub.id == id {
                    return Some(sub);
                }
                if let Some(found) = find(&mut sub.items, id) {
                    return Some(found);
                }
            }
        }
        None
    }
    for (_, entries) in &mut menus.menus {
        if let Some(sub) = find(entries, id) {
            sub.items = items;
            return;
        }
    }
}

/// Fill colours (the light ones first, as a highlight).
/// The theme colours in SpreadsheetML's order: Excel's names, the menu ids, and the tints its palette gives each (millionths).
const THEME_NAMES: [&str; 10] = ["_xlsx.background-1", "_xlsx.text-1", "_xlsx.background-2", "_xlsx.text-2", "_xlsx.accent-1", "_xlsx.accent-2", "_xlsx.accent-3", "_xlsx.accent-4", "_xlsx.accent-5", "_xlsx.accent-6"];
pub(super) const FILLS: &[(&str, u32)] = &[("_xlsx.light-yellow", 0xfff2cc), ("_shell.light-green", 0xe2efda), ("_shell.light-blue", 0xddebf7), ("_xlsx.light-orange", 0xfce4d6), ("_xlsx.light-gray", 0xededed), ("_shell.yellow", 0xffff00), ("_shell.green", 0x92d050), ("_shell.blue", 0x9bc2e6), ("_shell.gray", 0xbfbfbf), ("_xlsx.black", 0x000000)];
pub(super) const COLORS: &[(&str, u32)] = &[("_xlsx.black", 0x000000), ("_shell.dark-red", 0xc00000), ("_shell.red", 0xff0000), ("_shell.orange", 0xffc000), ("_shell.yellow", 0xffff00), ("_shell.light-green", 0x92d050), ("_shell.green", 0x00b050), ("_shell.light-blue", 0x00b0f0), ("_shell.blue", 0x0070c0), ("_shell.dark-blue", 0x002060), ("_shell.purple", 0x7030a0), ("_shell.white", 0xffffff)];
/// Number formats of 表示形式の詳細 (123 ▼).
pub(super) const NUMBER_FORMATS: &[(&str, &str)] = &[
    ("_xlsx.general", "General"),
    ("_xlsx.number", "#,##0"),
    ("_xlsx.number-2-decimal-places-2", "#,##0.00"),
    ("_xlsx.currency", "\"¥\"#,##0;\"¥\"\\-#,##0"),
    ("_xlsx.accounting", "_ \"¥\"* #,##0_ ;_ \"¥\"* \\-#,##0_ ;_ \"¥\"* \"-\"_ ;_ @_ "),
    ("docx.field.date", "yyyy/m/d"),
    ("_xlsx.date-year-month-day", "yyyy\"年\"m\"月\"d\"日\""), // i18n-ok（表示形式の書式コード（ファイルに保存される値））
    ("docx.field.time", "h:mm"),
    ("_xlsx.percentage", "0%"),
    ("_xlsx.fraction", "# ?/?"),
    ("_xlsx.scientific", "0.00E+00"),
    ("_xlsx.text", "@"),
];

impl SlideEditor {
    /// The selected cell's format (reflecting pending or run styles when editing a cell or worksheet shape).
    pub(crate) fn current_style(&self) -> CellStyle {
        let default = || self.presentation().workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
        let mut style = match (self.current_sheet(), self.current_cell()) {
            (Some((_, sheet)), Some((column, row))) => sheet.cell(row, column).map_or_else(default, |c| sheet.styles[c.style as usize].clone()),
            _ => default(),
        };
        if let Some(edit) = self.editing.as_ref()
            && let Some(body) = self.slide().shape(edit.shape).and_then(|s| s.text_body())
        {
            let range = edit.edit.range();
            let char_off = range.start;
            let is_caret = range.is_empty();
            let mut pos = 0;
            let mut found_run = None;
            let mut found_para_align = None;
            'outer: for para in &body.paragraphs {
                for r in &para.runs {
                    let n = r.text.chars().count();
                    if char_off < pos + n || (is_caret && n > 0 && char_off == pos + n) {
                        found_run = Some(&r.style);
                        found_para_align = para.align;
                        break 'outer;
                    }
                    pos += n;
                }
                if found_para_align.is_none() && char_off <= pos {
                    found_para_align = para.align;
                }
                pos += 1;
            }
            style.bold = found_run.and_then(|s| s.bold).unwrap_or(body.style.bold);
            style.italic = found_run.and_then(|s| s.italic).unwrap_or(body.style.italic);
            style.underline = match found_run {
                Some(s) if s.underline => match s.underline_kind {
                    ibis_doc_model::Underline::Double | ibis_doc_model::Underline::WavyDouble => ibis_doc_model::CellUnderline::Double,
                    _ => ibis_doc_model::CellUnderline::Single,
                },
                _ => ibis_doc_model::CellUnderline::None,
            };
            style.strike = found_run.is_some_and(|s| s.strike != ibis_doc_model::Strike::None);
            style.font_size = found_run.and_then(|s| s.font_size).unwrap_or(body.style.font_size);
            style.color = found_run.and_then(|s| s.color).unwrap_or(body.style.color);
            style.font = found_run.and_then(|s| s.font.clone()).or_else(|| body.style.font.clone());
            style.align = match found_para_align.unwrap_or(body.style.align) {
                ibis_doc_model::TextAlign::Left => CellAlign::Left,
                ibis_doc_model::TextAlign::Center => CellAlign::Center,
                ibis_doc_model::TextAlign::Right => CellAlign::Right,
                ibis_doc_model::TextAlign::Justify => CellAlign::Justify,
                ibis_doc_model::TextAlign::Distributed => CellAlign::Distributed,
            };
            style.vertical = match body.anchor {
                ibis_doc_model::VerticalAnchor::Top => CellVerticalAlign::Top,
                ibis_doc_model::VerticalAnchor::Middle => CellVerticalAlign::Center,
                ibis_doc_model::VerticalAnchor::Bottom => CellVerticalAlign::Bottom,
            };
            style.wrap = body.wrap;
            return style;
        }
        let primary_text_shape = self
            .selected_sheet_shape()
            .filter(|id| self.selected_sheet_text_shape_ids().contains(id))
            .or_else(|| self.selected_sheet_text_shape_ids().first().copied());
        if let Some(shape_id) = primary_text_shape
            && let Some(shape) = self.slide().shape(shape_id)
            && matches!(shape.kind, ibis_doc_model::ShapeKind::Auto(_) | ibis_doc_model::ShapeKind::TextBox(_))
        {
            if let Some(body) = shape.text_body() {
                let found_run = body.paragraphs.iter().flat_map(|p| &p.runs).find(|r| !r.text.is_empty()).or_else(|| body.paragraphs.first().and_then(|p| p.runs.first())).map(|r| &r.style);
                let found_para_align = body.paragraphs.first().and_then(|p| p.align);
                style.bold = found_run.and_then(|s| s.bold).unwrap_or(body.style.bold);
                style.italic = found_run.and_then(|s| s.italic).unwrap_or(body.style.italic);
                style.underline = match found_run {
                    Some(s) if s.underline => match s.underline_kind {
                        ibis_doc_model::Underline::Double | ibis_doc_model::Underline::WavyDouble => ibis_doc_model::CellUnderline::Double,
                        _ => ibis_doc_model::CellUnderline::Single,
                    },
                    _ => ibis_doc_model::CellUnderline::None,
                };
                style.strike = found_run.is_some_and(|s| s.strike != ibis_doc_model::Strike::None);
                style.font_size = found_run.and_then(|s| s.font_size).unwrap_or(body.style.font_size);
                style.color = found_run.and_then(|s| s.color).unwrap_or(body.style.color);
                style.font = found_run.and_then(|s| s.font.clone()).or_else(|| body.style.font.clone());
                style.align = match found_para_align.unwrap_or(body.style.align) {
                    ibis_doc_model::TextAlign::Left => CellAlign::Left,
                    ibis_doc_model::TextAlign::Center => CellAlign::Center,
                    ibis_doc_model::TextAlign::Right => CellAlign::Right,
                    ibis_doc_model::TextAlign::Justify => CellAlign::Justify,
                    ibis_doc_model::TextAlign::Distributed => CellAlign::Distributed,
                };
                style.vertical = match body.anchor {
                    ibis_doc_model::VerticalAnchor::Top => CellVerticalAlign::Top,
                    ibis_doc_model::VerticalAnchor::Middle => CellVerticalAlign::Center,
                    ibis_doc_model::VerticalAnchor::Bottom => CellVerticalAlign::Bottom,
                };
                style.wrap = body.wrap;
            } else {
                let ts = ibis_doc_model::TextStyle::default();
                style.bold = ts.bold;
                style.italic = ts.italic;
                style.underline = ibis_doc_model::CellUnderline::None;
                style.strike = false;
                style.font_size = ts.font_size;
                style.color = ts.color;
                style.font = ts.font;
                style.align = CellAlign::Left;
                style.vertical = CellVerticalAlign::Top;
                style.wrap = true;
            }
            return style;
        }
        // When editing a cell, reflect the pending style changes or active run style immediately in the toolbar.
        if let Some(edit) = self.cell_edit() {
            if let Some((col, row)) = self.current_cell() {
                if edit.slide == self.current_slide && edit.column == col && edit.row == row {
                    if let Some(pending) = &edit.pending {
                        for change in &pending.changes {
                            change.apply(&mut style);
                        }
                    } else if let Some(runs) = &edit.runs {
                        let caret_byte = super::cell_edit::byte_at(&edit.text, edit.caret);
                        let mut at = 0usize;
                        for run in runs {
                            let next = at + run.text.len();
                            if caret_byte <= next {
                                if let Some(font) = &run.font {
                                    style.font = Some(font.clone());
                                }
                                if let Some(size) = run.size {
                                    style.font_size = size;
                                }
                                if let Some(bold) = run.bold {
                                    style.bold = bold;
                                }
                                if let Some(italic) = run.italic {
                                    style.italic = italic;
                                }
                                if let Some(underline) = run.underline {
                                    style.underline = underline;
                                }
                                if let Some(strike) = run.strike {
                                    style.strike = strike;
                                }
                                if let Some(color) = run.color {
                                    style.color = color;
                                }
                                break;
                            }
                            at = next;
                        }
                    }
                }
            }
        }
        style
    }

    pub(crate) fn current_style_bold(&self) -> bool {
        self.current_style().bold
    }

    pub(crate) fn current_style_italic(&self) -> bool {
        self.current_style().italic
    }

    pub(crate) fn current_style_underline(&self) -> bool {
        self.current_style().underline.enabled()
    }

    fn xlsx_target_run_fonts_and_sizes(&self, default_font: &str) -> Vec<(String, u32)> {
        let mut out = Vec::new();
        if let Some(edit) = self.editing.as_ref()
            && let Some(body) = self.slide().shape(edit.shape).and_then(|s| s.text_body())
        {
            let range = edit.edit.range();
            let char_len = body.text().chars().count();
            let (start, end) = if range.start < range.end {
                (range.start, range.end)
            } else if range.start == range.end {
                // Caret only: single run at caret
                return Vec::new();
            } else {
                (0, char_len)
            };
            let body_font = body.style.font.as_deref().unwrap_or(default_font);
            let mut pos = 0usize;
            for p in &body.paragraphs {
                for r in &p.runs {
                    let n = r.text.chars().count();
                    if pos < end && start < pos + n && n > 0 {
                        let f = r.style.font.as_deref().unwrap_or(body_font).to_string();
                        let sz = r.style.font_size.unwrap_or(body.style.font_size);
                        out.push((f, sz));
                    }
                    pos += n;
                }
                pos += 1;
            }
            return out;
        }
        let shape_ids = self.selected_sheet_text_shape_ids();
        if !shape_ids.is_empty() {
            let mut any_body = false;
            for shape_id in shape_ids {
                if let Some(body) = self.slide().shape(shape_id).and_then(|s| s.text_body()) {
                    any_body = true;
                    let body_font = body.style.font.as_deref().unwrap_or(default_font);
                    let mut any_non_empty = false;
                    for p in &body.paragraphs {
                        for r in &p.runs {
                            if !r.text.is_empty() {
                                let f = r.style.font.as_deref().unwrap_or(body_font).to_string();
                                let sz = r.style.font_size.unwrap_or(body.style.font_size);
                                out.push((f, sz));
                                any_non_empty = true;
                            }
                        }
                    }
                    if !any_non_empty {
                        out.push((body_font.to_string(), body.style.font_size));
                    }
                } else if self
                    .slide()
                    .shape(shape_id)
                    .is_some_and(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Auto(_)))
                {
                    any_body = true;
                    let ts = ibis_doc_model::TextStyle::default();
                    let f = ts.font.as_deref().unwrap_or(default_font).to_string();
                    out.push((f, ts.font_size));
                }
            }
            if any_body {
                return out;
            }
        }
        if let Some(edit) = self.cell_edit() {
            if let Some(sel) = edit.selection()
                && !sel.is_empty()
                && let Some(runs) = &edit.runs
            {
                let b0 = super::cell_edit::byte_at(&edit.text, sel.start);
                let b1 = super::cell_edit::byte_at(&edit.text, sel.end);
                let cell_style = match (self.current_sheet(), self.current_cell()) {
                    (Some((_, sheet)), Some((col, row))) => sheet.cell(row, col).map_or_else(CellStyle::default, |c| sheet.styles[c.style as usize].clone()),
                    _ => CellStyle::default(),
                };
                let base_font = cell_style.font.as_deref().unwrap_or(default_font);
                let mut at = 0usize;
                for run in runs {
                    let next = at + run.text.len();
                    if at < b1 && b0 < next && !run.text.is_empty() {
                        let f = run.font.as_deref().unwrap_or(base_font).to_string();
                        let sz = run.size.unwrap_or(cell_style.font_size);
                        out.push((f, sz));
                    }
                    at = next;
                }
            }
            return out;
        }
        if let Some((_, sheet)) = self.current_sheet() {
            let wb_default = self.presentation().workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
            for r in self.selected_ranges() {
                let cell_count = (r.last_row.saturating_sub(r.first_row) + 1) as u64 * (r.last_column.saturating_sub(r.first_column) + 1) as u64;
                if cell_count > 1024 {
                    break;
                }
                for row in r.first_row..=r.last_row {
                    for col in r.first_column..=r.last_column {
                        let cell = sheet.cell(row, col);
                        let st = cell.map(|c| &sheet.styles[c.style as usize]).unwrap_or(&wb_default);
                        let base_font = st.font.as_deref().unwrap_or(default_font);
                        if let Some(runs) = cell.and_then(|c| c.runs.as_ref()).filter(|runs| !runs.is_empty()) {
                            for run in runs.iter() {
                                if !run.text.is_empty() {
                                    let f = run.font.as_deref().unwrap_or(base_font).to_string();
                                    let sz = run.size.unwrap_or(st.font_size);
                                    out.push((f, sz));
                                }
                            }
                        } else {
                            out.push((base_font.to_string(), st.font_size));
                        }
                    }
                }
            }
        }
        out
    }

    pub(crate) fn toolbar_font_display(&self, default_font: &str) -> String {
        let items = self.xlsx_target_run_fonts_and_sizes(default_font);
        if let Some((first_font, _)) = items.first() {
            if items.iter().any(|(f, _)| f != first_font) {
                return String::new();
            }
            return first_font.clone();
        }
        default_font.to_string()
    }

    pub(crate) fn toolbar_font_size_display(&self, default_size: u32) -> String {
        let items = self.xlsx_target_run_fonts_and_sizes("游ゴシック"); // i18n-ok
        if let Some(&(_, first_size)) = items.first() {
            let min_size = items.iter().map(|&(_, sz)| sz).min().unwrap_or(first_size);
            let max_size = items.iter().map(|&(_, sz)| sz).max().unwrap_or(first_size);
            let s = format!("{}", min_size as f32 / 100.0);
            if min_size != max_size {
                return format!("{s}+");
            }
            return s;
        }
        format!("{}", default_size as f32 / 100.0)
    }

    pub(crate) fn step_font_size_selection(&mut self, larger: bool, cx: &mut Context<Self>) {
        // 1. If selecting text in a cell edit with rich-text runs, step each selected run individually.
        if let Some(sel) = self.cell_edit().and_then(|e| e.selection()).filter(|r| !r.is_empty()) {
            let cell_style = match (self.current_sheet(), self.current_cell()) {
                (Some((_, sheet)), Some((col, row))) => sheet.cell(row, col).map_or_else(CellStyle::default, |c| sheet.styles[c.style as usize].clone()),
                _ => CellStyle::default(),
            };
            if let Some(edit) = self.cell_edit_mut()
                && !edit.text.trim_start().starts_with('=')
            {
                let b0 = super::cell_edit::byte_at(&edit.text, sel.start);
                let b1 = super::cell_edit::byte_at(&edit.text, sel.end);
                if b0 < b1 {
                    let runs = edit.runs.take().unwrap_or_else(|| {
                        vec![ibis_doc_model::CellRun {
                            text: edit.text.clone(),
                            font: None,
                            size: Some(cell_style.font_size),
                            bold: Some(cell_style.bold),
                            italic: Some(cell_style.italic),
                            underline: Some(cell_style.underline),
                            strike: Some(cell_style.strike),
                            color: Some(cell_style.color),
                        }]
                    });
                    let mut split: Vec<ibis_doc_model::CellRun> = Vec::new();
                    let mut at = 0usize;
                    for run in runs {
                        let (start, end) = (at, at + run.text.len());
                        at = end;
                        let mut cuts = vec![start];
                        cuts.extend([b0, b1].into_iter().filter(|&b| b > start && b < end));
                        cuts.push(end);
                        for w in cuts.windows(2) {
                            split.push(ibis_doc_model::CellRun {
                                text: run.text[w[0] - start..w[1] - start].to_string(),
                                ..run.clone()
                            });
                        }
                    }
                    let mut pos = 0usize;
                    for run in &mut split {
                        let end = pos + run.text.len();
                        if pos >= b0 && end <= b1 {
                            let cur = run.size.unwrap_or(cell_style.font_size);
                            run.size = Some(ibis_doc_model::step_font_size(cur, larger));
                        }
                        pos = end;
                    }
                    edit.runs = Some(split);
                    cx.notify();
                    return;
                }
            }
        }
        // 2. If editing or selecting a worksheet shape, step each run in the shape individually.
        if self.step_sheet_shape_font_size(larger, cx) {
            return;
        }
        // 3. Otherwise step the cell font size (stepping each run/cell individually when mixed).
        self.commit_cell_edit(cx);
        if self.protection_refuses(super::protection::Guarded::FormatCells, cx) {
            return;
        }
        if let Some((_, sheet)) = self.current_sheet() {
            let ranges = self.selected_ranges();
            let total_cells: u64 = ranges
                .iter()
                .map(|r| (r.last_row.saturating_sub(r.first_row) + 1) as u64 * (r.last_column.saturating_sub(r.first_column) + 1) as u64)
                .sum();
            if total_cells <= 1024 {
                let wb_default = self.presentation().workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
                let mut has_runs = false;
                let mut first_sz = None;
                let mut mixed_cells = false;
                for r in &ranges {
                    for row in r.first_row..=r.last_row {
                        for col in r.first_column..=r.last_column {
                            let cell = sheet.cell(row, col);
                            let st = cell.map(|c| &sheet.styles[c.style as usize]).unwrap_or(&wb_default);
                            if cell.and_then(|c| c.runs.as_ref()).is_some_and(|runs| !runs.is_empty()) {
                                has_runs = true;
                            }
                            match first_sz {
                                None => first_sz = Some(st.font_size),
                                Some(sz) if sz != st.font_size => mixed_cells = true,
                                _ => {}
                            }
                        }
                    }
                }
                if has_runs || mixed_cells {
                    let slide = self.current_slide;
                    let mut commands = Vec::new();
                    for r in &ranges {
                        for row in r.first_row..=r.last_row {
                            for col in r.first_column..=r.last_column {
                                let cell = sheet.cell(row, col);
                                let st = cell.map(|c| &sheet.styles[c.style as usize]).unwrap_or(&wb_default);
                                let next_cell_sz = ibis_doc_model::step_font_size(st.font_size, larger);
                                let cell_range = CellRange { first_row: row, first_column: col, last_row: row, last_column: col };
                                commands.push(Command::Sheet(SheetCommand::SetFormat {
                                    slide,
                                    ranges: vec![cell_range],
                                    change: StyleChange::FontSize(next_cell_sz),
                                }));
                                if let Some(runs) = cell.and_then(|c| c.runs.as_ref()).filter(|runs| !runs.is_empty()) {
                                    let new_runs: Vec<ibis_doc_model::CellRun> = runs
                                        .iter()
                                        .map(|run| {
                                            let cur = run.size.unwrap_or(st.font_size);
                                            ibis_doc_model::CellRun {
                                                size: Some(ibis_doc_model::step_font_size(cur, larger)),
                                                ..run.clone()
                                            }
                                        })
                                        .collect();
                                    commands.push(Command::Sheet(SheetCommand::SetRuns {
                                        slide,
                                        row,
                                        column: col,
                                        runs: new_runs,
                                    }));
                                }
                            }
                        }
                    }
                    if !commands.is_empty() {
                        self.run_sheet(commands, cx);
                        return;
                    }
                }
            }
        }
        let size = self.current_style().font_size;
        self.format_cells(StyleChange::FontSize(ibis_doc_model::step_font_size(size, larger)), cx);
    }

    pub(crate) fn step_sheet_indent(&mut self, more: bool, cx: &mut Context<Self>) {
        if self.step_sheet_shape_indent(more, cx) {
            return;
        }
        let cur = self.current_style().indent;
        let next = if more { cur + 1 } else { cur.saturating_sub(1) };
        self.format_cells(StyleChange::Indent(next), cx);
    }

    /// Change the selected range's format (one undo step).
    pub(crate) fn format_cells(&mut self, change: StyleChange, cx: &mut Context<Self>) {
        // Text selected while typing: the font change is for that part alone.
        if self.format_edit_selection(&change, cx) {
            return;
        }
        if self.format_sheet_shape_text(&change, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        if self.protection_refuses(super::protection::Guarded::FormatCells, cx) {
            return;
        }
        let Some(range) = self.selected_range() else { return };
        let slide = self.current_slide;
        let wrap_on = matches!(change, StyleChange::Wrap(true) | StyleChange::Rotation(1..=255));
        // With the ranges 条件を選択してジャンプ selected with it.
        let format = Command::Sheet(SheetCommand::SetFormat { slide, ranges: self.selected_ranges(), change });
        self.run_sheet(vec![format.clone()], cx);
        // Wrapping or turning on: the rows grow to the lines, as Excel's do (in the same step).
        if wrap_on {
            let ts = cx.text_system().clone();
            let fit = self.fit_rows_commands(range.first_row, range.last_row, &ts);
            if !fit.is_empty() {
                self.document.undo();
                let mut commands = vec![format];
                commands.extend(fit);
                self.run_sheet(commands, cx);
            }
        }
    }

    pub(crate) fn run_sheet(&mut self, commands: Vec<Command>, cx: &mut Context<Self>) {
        if self.is_read_only() {
            let view_only = commands.iter().all(|c| match c {
                Command::Sheet(sc) => matches!(
                    sc,
                    ibis_doc_model::SheetCommand::SetZoom { .. }
                        | ibis_doc_model::SheetCommand::SetGridLines { .. }
                        | ibis_doc_model::SheetCommand::SetHeadings { .. }
                        | ibis_doc_model::SheetCommand::SetPageBreakPreview { .. }
                        | ibis_doc_model::SheetCommand::SetPageLayout { .. }
                        | ibis_doc_model::SheetCommand::SetShowRuler { .. }
                        | ibis_doc_model::SheetCommand::SetHideInk { .. }
                        | ibis_doc_model::SheetCommand::Freeze { .. }
                        | ibis_doc_model::SheetCommand::Split { .. }
                ),
                _ => false,
            });
            if !view_only {
                return;
            }
        }
        if let Some(s) = self.sheet_state_mut() {
            s.last_fill = None;
        }
        super::sheet_view::release_layouts();
        let started = crate::debug_timing::start();
        let executed = self.document.execute(commands);
        crate::debug_timing::report("sheet edit: execute", started);
        if let Err(err) = executed {
            log::warn!("edit rejected: {err}");
            self.status = Some(crate::shell::i18n::format("_xlsx.this-operation-can-t-be-done-err", &[("err", &err)]).into());
        }
        if self.presentation().slide(self.current_slide).is_none() {
            self.current_slide = self
                .presentation()
                .workbook
                .as_ref()
                .and_then(|w| w.sheets.iter().find_map(|s| s.slide))
                .unwrap_or(self.presentation().slides[0].id);
        }
        let started = crate::debug_timing::start();
        self.sync_workbook_peers(cx);
        crate::debug_timing::report("sheet edit: sync peers", started);
        cx.notify();
    }

    /// Borders on the selected range: every line (格子), the outside, the bottom, or none.
    pub(super) fn set_borders(&mut self, kind: &'static str, cx: &mut Context<Self>) {
        self.apply_borders(kind, false, cx);
    }

    /// The border line of `edge` of the selected range at its cell `along` (0 = the first), as the cells have it.
    pub(crate) fn range_edge(&self, kind: &str) -> Option<Border> {
        let r = self.selected_range()?;
        let (_, sheet) = self.current_sheet()?;
        let at = |column: u32, row: u32| sheet.cell(row, column).and_then(|c| sheet.styles.get(c.style as usize)).cloned().unwrap_or_default();
        let (cells, pick): (Vec<(u32, u32)>, fn(&CellStyle) -> Option<Border>) = match kind {
            "top" => ((r.first_column..=r.last_column).map(|c| (c, r.first_row)).collect(), |s| s.top),
            "bottom" => ((r.first_column..=r.last_column).map(|c| (c, r.last_row)).collect(), |s| s.bottom),
            "left" => ((r.first_row..=r.last_row).map(|row| (r.first_column, row)).collect(), |s| s.left),
            "right" => ((r.first_row..=r.last_row).map(|row| (r.last_column, row)).collect(), |s| s.right),
            "inside-h" => ((r.first_column..=r.last_column).flat_map(|c| (r.first_row..r.last_row).map(move |row| (c, row))).collect(), |s| s.bottom),
            "inside-v" => ((r.first_row..=r.last_row).flat_map(|row| (r.first_column..r.last_column).map(move |c| (c, row))).collect(), |s| s.right),
            "diagonal-up" => (vec![(r.first_column, r.first_row)], |s| s.diagonal_up),
            "diagonal-down" => (vec![(r.first_column, r.first_row)], |s| s.diagonal_down),
            _ => return None,
        };
        let first = pick(&at(cells.first()?.0, cells.first()?.1))?;
        cells.iter().all(|&(c, row)| pick(&at(c, row)).is_some()).then_some(first)
    }

    /// A line of the format pane's sample was pressed: the line is taken off if the cells have it, else drawn.
    pub(crate) fn toggle_border(&mut self, kind: &'static str, cx: &mut Context<Self>) {
        let has = self.range_edge(kind).is_some();
        self.apply_borders(kind, has, cx);
    }

    /// `set_borders`, or (`remove`) the lines of `kind` taken off.
    pub(crate) fn apply_borders(&mut self, kind: &'static str, remove: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(r) = self.selected_range() else { return };
        let slide = self.current_slide;
        let chosen = Border { line: self.wb().border_line, color: self.wb().border_color };
        let line = (!remove).then_some(chosen);
        let edge = |ranges: Vec<CellRange>, e: Edge, b: Option<Border>| Command::Sheet(SheetCommand::SetFormat { slide, ranges, change: StyleChange::Border(e, b) });
        let row = |i: u32| CellRange { first_row: i, last_row: i, ..r };
        let column = |i: u32| CellRange { first_column: i, last_column: i, ..r };
        let commands = match kind {
            "all" => [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom].into_iter().map(|e| edge(vec![r], e, line)).collect(),
            "outside" => vec![edge(vec![row(r.first_row)], Edge::Top, line), edge(vec![row(r.last_row)], Edge::Bottom, line), edge(vec![column(r.first_column)], Edge::Left, line), edge(vec![column(r.last_column)], Edge::Right, line)],
            "bottom" => vec![edge(vec![row(r.last_row)], Edge::Bottom, line)],
            "top" => vec![edge(vec![row(r.first_row)], Edge::Top, line)],
            "left" => vec![edge(vec![column(r.first_column)], Edge::Left, line)],
            "right" => vec![edge(vec![column(r.last_column)], Edge::Right, line)],
            "inside-h" if r.last_row > r.first_row => vec![edge(vec![CellRange { last_row: r.last_row - 1, ..r }], Edge::Bottom, line)],
            "inside-v" if r.last_column > r.first_column => vec![edge(vec![CellRange { last_column: r.last_column - 1, ..r }], Edge::Right, line)],
            "inside" => [(r.last_row > r.first_row).then(|| edge(vec![CellRange { last_row: r.last_row - 1, ..r }], Edge::Bottom, line)), (r.last_column > r.first_column).then(|| edge(vec![CellRange { last_column: r.last_column - 1, ..r }], Edge::Right, line))].into_iter().flatten().collect(),
            "inside-h" | "inside-v" => return,
            "diagonal-up" => vec![edge(vec![r], Edge::DiagonalUp, line)],
            "diagonal-down" => vec![edge(vec![r], Edge::DiagonalDown, line)],
            "thick-bottom" => vec![edge(vec![row(r.last_row)], Edge::Bottom, Some(Border { line: BorderLine::Thick, ..chosen }))],
            "double-bottom" => vec![edge(vec![row(r.last_row)], Edge::Bottom, Some(Border { line: BorderLine::Double, ..chosen }))],
            _ => [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom, Edge::DiagonalUp, Edge::DiagonalDown].into_iter().map(|e| edge(vec![r], e, None)).collect(),
        };
        self.run_sheet(commands, cx);
    }

    pub(crate) fn merge_cells(&mut self, center: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(range) = self.selected_range() else { return };
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::Merge { slide, range, center })], cx);
    }

    /// 横方向に結合: each row of the selection merged on its own.
    fn merge_across(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(r) = self.selected_range() else { return };
        if r.first_column == r.last_column {
            return;
        }
        let slide = self.current_slide;
        let commands = (r.first_row..=r.last_row).map(|row| Command::Sheet(SheetCommand::Merge { slide, range: CellRange { first_row: row, last_row: row, ..r }, center: false })).collect();
        self.run_sheet(commands, cx);
    }

    pub(crate) fn status_summary_flag(&self, flag: u8) -> bool {
        self.sheet_state().map_or(super::STATUS_SUMMARY_DEFAULT, |s| s.status_summary_mask) & flag != 0
    }

    pub(crate) fn toggle_status_summary_flag(&mut self, flag: u8, cx: &mut Context<Self>) {
        if let Some(state) = self.sheet_state_mut() {
            state.status_summary_mask ^= flag;
            cx.notify();
        }
    }

    /// The status bar's 平均・データの個数・数値の個数・最小値・最大値・合計 of the selected range (more
    /// than one cell; hidden rows and columns left out), in the selected
    /// cell's number format.
    pub(crate) fn range_summary(&self) -> Option<String> {
        let mask = self.sheet_state().map_or(super::STATUS_SUMMARY_DEFAULT, |s| s.status_summary_mask);
        if mask == 0 {
            return None;
        }
        let (ranges, (_, sheet)) = (self.selected_ranges(), self.current_sheet()?);
        if ranges.len() < 2 && ranges.iter().all(|r| r.first_row == r.last_row && r.first_column == r.last_column) {
            return None;
        }
        // Over all the ranges selected (⌘-click), each cell once (not
        // measured against Excel for overlapping ranges).
        let inside = |c: &ibis_doc_model::SheetCell| ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column));
        let (mut count, mut numbers, mut sum) = (0usize, 0usize, 0.0f64);
        let (mut min_val, mut max_val): (Option<f64>, Option<f64>) = (None, None);
        for c in &sheet.cells {
            if !inside(c) {
                continue;
            }
            if sheet.rows.get(c.row as usize) == Some(&0) || sheet.columns.get(c.column as usize) == Some(&0) || c.value.is_empty() {
                continue;
            }
            count += 1;
            if let ibis_doc_model::CellValue::Number(n) = c.value {
                numbers += 1;
                sum += n;
                min_val = Some(min_val.map_or(n, |m| m.min(n)));
                max_val = Some(max_val.map_or(n, |m| m.max(n)));
            }
        }
        // Excel shows them from two cells with something in them.
        if count < 2 {
            return None;
        }
        let code = self.current_style().number_format;
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let show = |v: f64| ibis_calc::numfmt::format(&ibis_doc_model::CellValue::Number(v), &code, date1904).text;
        if mask == super::STATUS_SUMMARY_DEFAULT {
            return Some(if numbers == 0 {
                crate::shell::i18n::format("_xlsx.count-count", &[("count", &count)])
            } else {
                crate::shell::i18n::format("_xlsx.average-average-count-count-sum-sum", &[("average", &show(sum / numbers as f64)), ("count", &count), ("sum", &show(sum))])
            });
        }
        let mut parts = Vec::new();
        if (mask & super::STATUS_SUMMARY_AVG) != 0 && numbers > 0 {
            parts.push(format!("{}: {}", crate::shell::i18n::text("xlsx.status-summary-average"), show(sum / numbers as f64)));
        }
        if (mask & super::STATUS_SUMMARY_COUNT) != 0 {
            parts.push(format!("{}: {count}", crate::shell::i18n::text("xlsx.status-summary-count")));
        }
        if (mask & super::STATUS_SUMMARY_NUM_COUNT) != 0 && numbers > 0 {
            parts.push(format!("{}: {numbers}", crate::shell::i18n::text("xlsx.status-summary-num-count")));
        }
        if (mask & super::STATUS_SUMMARY_MIN) != 0 {
            if let Some(m) = min_val {
                parts.push(format!("{}: {}", crate::shell::i18n::text("xlsx.status-summary-min"), show(m)));
            }
        }
        if (mask & super::STATUS_SUMMARY_MAX) != 0 {
            if let Some(m) = max_val {
                parts.push(format!("{}: {}", crate::shell::i18n::text("xlsx.status-summary-max"), show(m)));
            }
        }
        if (mask & super::STATUS_SUMMARY_SUM) != 0 && numbers > 0 {
            parts.push(format!("{}: {}", crate::shell::i18n::text("xlsx.status-summary-sum"), show(sum)));
        }
        (!parts.is_empty()).then(|| parts.join("   "))
    }

    /// Select every cell of the sheet (⌘A).
    pub(crate) fn select_all_cells(&mut self, cx: &mut Context<Self>) {
        if self.cell_edit().is_some_and(|e| e.mode == super::cell_edit::EditMode::Edit) && self.cell_select_all_text(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let end = (sheet.columns.len().max(1) as u32 - 1, sheet.rows.len().max(1) as u32 - 1);
        let slide = self.current_slide;
        self.selection = None;
        let state = self.wb_mut();
        state.panels.extra_ranges = None;
        state.cells.insert(slide, (0, 0));
        state.cell_ends.insert(slide, end);
        cx.notify();
    }

    /// すべてクリア: the selected cells' values and formats.
    pub(crate) fn clear_all_cells(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let (Some(range), Some((_, sheet))) = (self.selected_range(), self.current_sheet()) else { return };
        let edits: Vec<ibis_doc_model::CellEdit> = sheet
            .cells
            .iter()
            .filter(|c| (range.first_row..=range.last_row).contains(&c.row) && (range.first_column..=range.last_column).contains(&c.column))
            .filter(|c| !c.value.is_empty() || c.formula.is_some())
            .map(|c| ibis_doc_model::CellEdit { row: c.row, column: c.column, input: ibis_doc_model::CellInput::Clear })
            .collect();
        let slide = self.current_slide;
        let mut commands = Vec::new();
        if !edits.is_empty() {
            commands.push(Command::Sheet(SheetCommand::SetCells { slide, edits }));
        }
        commands.push(Command::Sheet(SheetCommand::ClearFormats { slide, ranges: vec![range] }));
        self.run_sheet(commands, cx);
    }

    fn unmerge_cells(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(range) = self.selected_range() else { return };
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::Unmerge { slide, range })], cx);
    }

    /// Insert rows (or columns) at the selection, before it or after it.
    pub(crate) fn insert_lines(&mut self, axis: Axis, after: bool, cx: &mut Context<Self>) {
        let guard = if axis == Axis::Rows { super::protection::Guarded::InsertRows } else { super::protection::Guarded::InsertColumns };
        if self.protection_refuses(guard, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some(r) = self.selected_range() else { return };
        let (first, last) = match axis {
            Axis::Rows => (r.first_row, r.last_row),
            Axis::Columns => (r.first_column, r.last_column),
        };
        let slide = self.current_slide;
        let at = if after { last + 1 } else { first };
        self.run_sheet(vec![Command::Sheet(SheetCommand::Insert { slide, axis, at, count: last - first + 1 })], cx);
    }

    pub(crate) fn delete_lines(&mut self, axis: Axis, cx: &mut Context<Self>) {
        let guard = if axis == Axis::Rows { super::protection::Guarded::DeleteRows } else { super::protection::Guarded::DeleteColumns };
        if self.protection_refuses(guard, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some(r) = self.selected_range() else { return };
        let (first, last) = match axis {
            Axis::Rows => (r.first_row, r.last_row),
            Axis::Columns => (r.first_column, r.last_column),
        };
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::Delete { slide, axis, at: first, count: last - first + 1 })], cx);
    }

    /// Hide the selected rows (or columns), or show the hidden ones among them.
    pub(crate) fn hide_lines(&mut self, axis: Axis, hidden: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(r) = self.selected_range() else { return };
        let (first, last) = match axis {
            Axis::Rows => (r.first_row, r.last_row),
            Axis::Columns => (r.first_column, r.last_column),
        };
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::Hide { slide, axis, first, last, hidden })]) {
            log::debug!("nothing to hide or show: {err}");
        }
        cx.notify();
    }

    /// A new sheet after the current one, named シートN.
    pub(crate) fn insert_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some(wb) = self.presentation().workbook.clone() else { return };
        let index = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide)).map_or(wb.sheets.len(), |i| i + 1);
        let mut n = wb.sheets.len() + 1;
        let name = loop {
            let name = format!("Sheet{n}");
            if ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &name, None) {
                break name;
            }
            n += 1;
        };
        let (slide, shape) = (self.document.ids().new_slide_id(), self.document.ids().new_shape_id());
        self.run_sheet(vec![Command::Sheet(SheetCommand::AddSheet { index, name, slide, shape })], cx);
        if self.presentation().slide(slide).is_some() {
            self.select_slide(slide, window, cx);
        }
    }

    fn delete_sheet(&mut self, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::DeleteSheet { slide })], cx);
    }

    /// ウィンドウ枠の固定（任意位置）：the rows above and columns left of selected cell stay in view.
    pub(crate) fn freeze_panes(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((_, _sheet)) = self.current_sheet() else { return };
        let slide = self.current_slide;
        let frozen = self.current_cell().filter(|&(c, r)| c > 0 || r > 0);
        // Frozen panes take the place of split ones (表示 > 分割).
        self.wb_mut().panels.splits.remove(&slide);
        self.run_sheet(vec![Command::Sheet(SheetCommand::Freeze { slide, frozen })], cx);
    }

    /// 先頭行の固定（Freeze Top Row）：keeps row 1 visible while scrolling.
    pub(crate) fn freeze_top_row(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        self.wb_mut().panels.splits.remove(&slide);
        self.run_sheet(vec![Command::Sheet(SheetCommand::Freeze { slide, frozen: Some((0, 1)) })], cx);
    }

    /// 先頭列の固定（Freeze First Column）：keeps column A visible while scrolling.
    pub(crate) fn freeze_first_column(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        self.wb_mut().panels.splits.remove(&slide);
        self.run_sheet(vec![Command::Sheet(SheetCommand::Freeze { slide, frozen: Some((1, 0)) })], cx);
    }

    /// ウィンドウ枠固定の解除（Unfreeze Panes）：removes frozen panes.
    pub(crate) fn unfreeze_panes(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::Freeze { slide, frozen: None })], cx);
    }

    /// ウィンドウ枠の固定（トグル）：既存互換。
    pub(crate) fn toggle_freeze(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let slide = self.current_slide;
        let frozen = match sheet.frozen {
            Some(_) => None,
            None => self.current_cell().filter(|&(c, r)| c > 0 || r > 0),
        };
        if frozen.is_none() && sheet.frozen.is_none() {
            return;
        }
        self.wb_mut().panels.splits.remove(&slide);
        self.run_sheet(vec![Command::Sheet(SheetCommand::Freeze { slide, frozen })], cx);
    }

    /// セルの挿入・削除 at the selected range, the cells beside moving down
    /// or right (up or left).
    pub(crate) fn shift_cells(&mut self, down: bool, insert: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(range) = self.selected_range() else { return };
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::ShiftCells { slide, range, down, insert })]) {
            log::warn!("shifting cells rejected: {err}");
            self.status = Some(crate::shell::i18n::text("_xlsx.cannot-move-part-of-a-merged-cell"));
        }
        cx.notify();
    }

    /// Ask where, and write the sheet shown there as CSV.
    pub(crate) fn export_csv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).or_else(|| std::env::var_os("HOME").map(Into::into)).unwrap_or_else(|| ".".into());
            let stem = self.path.as_deref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Sheet1".into());
            let name = format!("{stem}.csv");
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update(cx, |this, cx| {
                    let r = this.write_csv(&path);
                    this.status = Some(match r {
                        Ok(()) => crate::shell::i18n::format("_xlsx.exported-to-path", &[("path", &path.display())]).into(),
                        Err(e) => crate::shell::i18n::format("_xlsx.couldn-t-export-error", &[("error", &e)]).into(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.status = Some(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve"));
            cx.notify();
        }
    }

    /// The sheet shown, as CSV, to `path`.
    pub(crate) fn write_csv(&self, path: &std::path::Path) -> std::io::Result<()> {
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        std::fs::write(path, ibis_pptx::csv::sheet_to_csv(self.presentation(), index))
    }

    /// Ask where, and write the sheet shown there as Shift_JIS CSV.
    pub(crate) fn export_csv_sjis(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).or_else(|| std::env::var_os("HOME").map(Into::into)).unwrap_or_else(|| ".".into());
            let stem = self.path.as_deref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Sheet1".into());
            let name = format!("{stem}.csv");
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update(cx, |this, cx| {
                    let r = this.write_csv_sjis(&path);
                    this.status = Some(match r {
                        Ok(()) => crate::shell::i18n::format("_xlsx.exported-to-path", &[("path", &path.display())]).into(),
                        Err(e) => crate::shell::i18n::format("_xlsx.couldn-t-export-error", &[("error", &e)]).into(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.status = Some(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve"));
            cx.notify();
        }
    }

    /// Ask where, and write the sheet shown there as TSV (tab-separated values).
    pub(crate) fn export_tsv(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).or_else(|| std::env::var_os("HOME").map(Into::into)).unwrap_or_else(|| ".".into());
            let stem = self.path.as_deref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Sheet1".into());
            let name = format!("{stem}.tsv");
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update(cx, |this, cx| {
                    let r = this.write_tsv(&path);
                    this.status = Some(match r {
                        Ok(()) => crate::shell::i18n::format("_xlsx.exported-to-path", &[("path", &path.display())]).into(),
                        Err(e) => crate::shell::i18n::format("_xlsx.couldn-t-export-error", &[("error", &e)]).into(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.status = Some(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve"));
            cx.notify();
        }
    }

    /// The whole workbook, as an OpenDocument spreadsheet, to `path`.
    pub(crate) fn write_ods(&self, path: &std::path::Path) -> std::io::Result<()> {
        let bytes = match self.source.as_deref() {
            Some(source) => ibis_pptx::write_ods_with_source(self.presentation(), source),
            None => ibis_pptx::write_ods(self.presentation()),
        }
        .map_err(|e| std::io::Error::other(e.to_string()))?;
        std::fs::write(path, bytes)
    }

    /// The whole workbook, as an old Excel file, to `path`; what was left out.
    pub(crate) fn write_xls(&self, path: &std::path::Path) -> std::io::Result<Option<SharedString>> {
        let (bytes, report) = ibis_pptx::write_xls_report(self.presentation()).map_err(|e| std::io::Error::other(e.to_string()))?;
        std::fs::write(path, bytes)?;
        Ok(super::export_xls::note(&report))
    }

    /// Ask where, and write the workbook there as an old Excel file (.xls).
    pub(crate) fn export_xls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).or_else(|| std::env::var_os("HOME").map(Into::into)).unwrap_or_else(|| ".".into());
            let stem = self.path.as_deref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Book1".into());
            let name = format!("{stem}.xls");
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update(cx, |this, cx| {
                    let r = this.write_xls(&path);
                    this.status = Some(match r {
                        Ok(Some(note)) => note,
                        Ok(None) => crate::shell::i18n::format("_xlsx.exported-to-path", &[("path", &path.display())]).into(),
                        Err(e) => crate::shell::i18n::format("_xlsx.couldn-t-export-error", &[("error", &e)]).into(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.status = Some(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve"));
            cx.notify();
        }
    }

    /// Ask where, and write the workbook there as an OpenDocument spreadsheet (.ods).
    pub(crate) fn export_ods(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).or_else(|| std::env::var_os("HOME").map(Into::into)).unwrap_or_else(|| ".".into());
            let stem = self.path.as_deref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Book1".into());
            let name = format!("{stem}.ods");
            let picked = cx.prompt_for_new_path(&directory, Some(&name));
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update(cx, |this, cx| {
                    let r = this.write_ods(&path);
                    this.status = Some(match r {
                        Ok(()) => crate::shell::i18n::format("_xlsx.exported-to-path", &[("path", &path.display())]).into(),
                        Err(e) => crate::shell::i18n::format("_xlsx.couldn-t-export-error", &[("error", &e)]).into(),
                    });
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.status = Some(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve"));
            cx.notify();
        }
    }

    /// The sheet shown, as Shift_JIS CSV, to `path`.
    pub(crate) fn write_csv_sjis(&self, path: &std::path::Path) -> std::io::Result<()> {
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        std::fs::write(path, ibis_pptx::csv::sheet_to_csv_sjis(self.presentation(), index))
    }

    /// The sheet shown, as TSV, to `path`.
    pub(crate) fn write_tsv(&self, path: &std::path::Path) -> std::io::Result<()> {
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        std::fs::write(path, ibis_pptx::csv::sheet_to_tsv(self.presentation(), index))
    }

    /// The axis an outline acts on: columns when whole columns are selected, else rows.
    fn outline_axis(&self) -> (Axis, u32, u32) {
        let r = self.selected_range();
        match (self.selected_lines(Axis::Columns), r) {
            (Some((a, b)), _) => (Axis::Columns, a, b),
            (None, Some(r)) => (Axis::Rows, r.first_row, r.last_row),
            _ => (Axis::Rows, 0, 0),
        }
    }

    /// グループ化 (`deeper`) or グループ解除 of the selected rows (or columns).
    pub(crate) fn group_lines(&mut self, deeper: bool, cx: &mut Context<Self>) {
        let (axis, _, _) = self.outline_axis();
        self.group_lines_on_axis(axis, deeper, cx);
    }

    /// グループ化 (`deeper`) or グループ解除 along an explicit `axis` (rows or columns).
    pub(crate) fn group_lines_on_axis(&mut self, axis: Axis, deeper: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(r) = self.selected_range() else { return };
        let (first, last) = match axis {
            Axis::Columns => (r.first_column, r.last_column),
            Axis::Rows => (r.first_row, r.last_row),
        };
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::Group { slide, axis, first, last, deeper })], cx);
    }

    /// 詳細を表示しない (`collapse`) or 詳細を表示: the group the selected row
    /// (or column) is in hidden or shown.
    pub(crate) fn collapse_group(&mut self, collapse: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let (axis, at, _) = self.outline_axis();
        let levels = if axis == Axis::Rows { &sheet.outline_rows } else { &sheet.outline_columns };
        let Some(&level) = levels.get(&at) else { return };
        let (mut first, mut last) = (at, at);
        while first > 0 && levels.get(&(first - 1)).is_some_and(|&l| l >= level) {
            first -= 1;
        }
        while levels.get(&(last + 1)).is_some_and(|&l| l >= level) {
            last += 1;
        }
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::Hide { slide, axis, first, last, hidden: collapse })], cx);
    }

    /// シートを複製: a copy right after it, named as Excel names it (「月次売上 (2)」).
    pub(crate) fn copy_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        let Some(base) = self.slide().name.clone() else { return };
        let mut n = 2;
        let name = loop {
            let name = format!("{base} ({n})");
            if ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &name, None) {
                break name;
            }
            n += 1;
        };
        let (new_slide, shape) = (self.document.ids().new_slide_id(), self.document.ids().new_shape_id());
        self.run_sheet(vec![Command::Sheet(SheetCommand::CopySheet { slide, name, new_slide, shape })], cx);
        if self.presentation().slide(new_slide).is_some() {
            self.select_slide(new_slide, window, cx);
        }
    }

    /// 並べ替え: the table around the selected cell (or the selected range)
    /// sorted by the selected cell's column; a heading row (text over a
    /// column of numbers, or bold) stays on top, as Excel guesses.
    pub(crate) fn sort_selection(&self) -> Option<(CellRange, u32, bool)> {
        let ((column, row), (_, sheet)) = (self.current_cell()?, self.current_sheet()?);
        let filled = |r: u32, c: u32| sheet.cell(r, c).is_some_and(|x| !x.value.is_empty() || x.formula.is_some());
        let range = match self.selected_range() {
            Some(r) if r.first_row != r.last_row => r,
            _ => {
                // The current region: grow while the rows and columns next to it hold something.
                let mut r = CellRange { first_row: row, first_column: column, last_row: row, last_column: column };
                loop {
                    let mut grown = r;
                    if r.first_row > 0 && (r.first_column..=r.last_column).any(|c| filled(r.first_row - 1, c)) {
                        grown.first_row -= 1;
                    }
                    if (r.first_column..=r.last_column).any(|c| filled(r.last_row + 1, c)) {
                        grown.last_row += 1;
                    }
                    if r.first_column > 0 && (r.first_row..=r.last_row).any(|x| filled(x, r.first_column - 1)) {
                        grown.first_column -= 1;
                    }
                    if (r.first_row..=r.last_row).any(|x| filled(x, r.last_column + 1)) {
                        grown.last_column += 1;
                    }
                    if grown == r {
                        break r;
                    }
                    r = grown;
                }
            }
        };
        let top = |c: u32| sheet.cell(range.first_row, c);
        let below = |c: u32| sheet.cell(range.first_row + 1, c);
        let header = (range.first_column..=range.last_column).any(|c| {
            let t = top(c);
            t.is_some_and(|x| sheet.styles[x.style as usize].bold) || (matches!(t.map(|x| &x.value), Some(ibis_doc_model::CellValue::Text(_))) && matches!(below(c).map(|x| &x.value), Some(ibis_doc_model::CellValue::Number(_))))
        });
        Some((range, column, header))
    }

    pub(crate) fn sort_by_selected_column(&mut self, ascending: bool, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Sort, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((range, column, header)) = self.sort_selection() else { return };
        self.run_sheet(vec![Command::Sheet(SheetCommand::Sort { slide: self.current_slide, range, keys: vec![(column, ascending)], header })], cx);
    }

    /// The current sheet's tab colour.
    pub(crate) fn set_tab_color(&mut self, color: Option<Rgb>, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::SetTabColor { slide, color })], cx);
    }

    /// The current sheet one place left (`-1`) or right (`1`).
    pub(crate) fn move_sheet(&mut self, step: i32, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        let Some(wb) = self.presentation().workbook.clone() else { return };
        let Some(at) = wb.sheets.iter().position(|s| s.slide == Some(slide)) else { return };
        let to = at as i64 + step as i64;
        if to < 0 || to as usize >= wb.sheets.len() {
            return;
        }
        self.run_sheet(vec![Command::Sheet(SheetCommand::MoveSheet { slide, to: to as usize })], cx);
    }

    /// Move the current sheet to the very first (`to_first == true`) or very last (`to_first == false`) position in the workbook.
    pub(crate) fn move_sheet_to_edge(&mut self, to_first: bool, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        let Some(wb) = self.presentation().workbook.clone() else { return };
        let Some(at) = wb.sheets.iter().position(|s| s.slide == Some(slide)) else { return };
        let to = if to_first { 0 } else { wb.sheets.len().saturating_sub(1) };
        if at == to {
            return;
        }
        self.run_sheet(vec![Command::Sheet(SheetCommand::MoveSheet { slide, to })], cx);
    }

    /// Hide the current sheet (シートを非表示).
    pub(crate) fn hide_sheet(&mut self, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        let Some(sheet) = self.presentation().workbook.as_ref().and_then(|w| w.sheets.iter().position(|s| s.slide == Some(slide))) else { return };
        let shape = self.document.ids().new_shape_id();
        self.run_sheet(vec![Command::Sheet(SheetCommand::HideSheet { slide, sheet, hidden: true, shape })], cx);
    }

    /// Show hidden sheet `sheet` again, and go to it.
    pub(crate) fn unhide_sheet(&mut self, sheet: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let (slide, shape) = (self.document.ids().new_slide_id(), self.document.ids().new_shape_id());
        self.run_sheet(vec![Command::Sheet(SheetCommand::HideSheet { slide, sheet, hidden: false, shape })], cx);
        if self.presentation().slide(slide).is_some() {
            self.select_slide(slide, window, cx);
        }
    }

    /// Show all hidden sheets in the workbook at once.
    pub(crate) fn unhide_all_sheets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let hidden_indices: Vec<usize> = self
            .presentation()
            .workbook
            .as_ref()
            .map(|w| w.sheets.iter().enumerate().filter(|(_, s)| s.slide.is_none() && s.hidden.is_some() && !s.other).map(|(i, _)| i).collect())
            .unwrap_or_default();
        if hidden_indices.is_empty() {
            return;
        }
        let mut last_slide = None;
        let commands: Vec<Command> = hidden_indices
            .into_iter()
            .map(|sheet| {
                let (slide, shape) = (self.document.ids().new_slide_id(), self.document.ids().new_shape_id());
                last_slide = Some(slide);
                Command::Sheet(SheetCommand::HideSheet { slide, sheet, hidden: false, shape })
            })
            .collect();
        self.run_sheet(commands, cx);
        if let Some(slide) = last_slide.filter(|&id| self.presentation().slide(id).is_some()) {
            self.select_slide(slide, window, cx);
        }
    }

    /// Open the name field over a sheet's tab, its name selected.
    pub(crate) fn begin_sheet_rename(&mut self, slide: SlideId, window: &mut Window, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        if slide != self.current_slide {
            self.select_slide(slide, window, cx);
        }
        let Some(index) = self.presentation().slides.iter().position(|s| s.id == slide) else { return };
        let name = self.presentation().slides[index].name.clone().unwrap_or_else(|| format!("Sheet{}", index + 1));
        let Some(state) = self.sheet_state_mut() else { return };
        state.jump.renaming = Some(slide);
        let input = state.jump.tab_rename.clone();
        input.update(cx, |input, cx| {
            input.set_value(name, window, cx);
            input.select_all(window, cx);
        });
        input.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    /// The name field's Enter (or leaving it): rename the sheet. A name
    /// Excel does not take leaves the field open, to be put right.
    pub(crate) fn confirm_sheet_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.sheet_state_mut() else { return };
        let Some(slide) = state.jump.renaming else { return };
        let name = state.jump.tab_rename.read(cx).value().trim().to_string();
        let old = self.presentation().slide(slide).and_then(|s| s.name.clone());
        if name.is_empty() || old.as_deref() == Some(name.as_str()) {
            return self.end_sheet_rename(window, cx);
        }
        let index = self.presentation().workbook.as_ref().and_then(|w| w.sheets.iter().position(|s| s.slide == Some(slide)));
        if !ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), &name, index) {
            self.status = Some(format!("{}: {name}", crate::shell::i18n::text("_xlsx.this-name-cannot-be-used-as-a-sheet-name")).into());
            cx.notify();
            return;
        }
        self.run_sheet(vec![Command::Sheet(SheetCommand::RenameSheet { slide, name })], cx);
        self.end_sheet_rename(window, cx);
    }

    fn end_sheet_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.sheet_state_mut() {
            state.jump.renaming = None;
        }
        if self.status.as_ref().is_some_and(|s| s.starts_with(crate::shell::i18n::text("_xlsx.this-name-cannot-be-used-as-a-sheet-name").as_ref())) {
            self.status = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// The number format with one decimal more or less.
    fn step_decimals(&mut self, more: bool, cx: &mut Context<Self>) {
        let code = self.current_style().number_format;
        let new = step_decimals(&code, more);
        if new != code {
            self.format_cells(StyleChange::NumberFormat(new), cx);
        }
    }
}

/// `code` with one decimal place more or less in each section.
fn step_decimals(code: &str, more: bool) -> String {
    if code.eq_ignore_ascii_case("General") {
        return if more { "0.0".into() } else { "0".into() };
    }
    code.split(';')
        .map(|section| {
            // The last run of digit placeholders.
            let Some(last) = section.rfind(['0', '#']) else { return section.to_string() };
            let (head, tail) = section.split_at(last + 1);
            match head.rfind('.') {
                Some(dot) if head[dot + 1..].chars().all(|c| c == '0' || c == '#') => {
                    if more {
                        format!("{head}0{tail}")
                    } else if head.len() - dot - 1 <= 1 {
                        format!("{}{tail}", &head[..dot])
                    } else {
                        format!("{}{tail}", &head[..head.len() - 1])
                    }
                }
                _ if more => format!("{head}.0{tail}"),
                _ => section.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// The palette of 文字の色 (`fill` false) or 塗りつぶし (true) for the selected cells: the toolbar's and the
/// format pane's.
pub(crate) fn cell_color_menu(editor: &SlideEditor, fill: bool) -> Vec<MenuEntry> {
    let format = |change: StyleChange| move |this: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>| this.format_cells(change.clone(), cx);
    use crate::shell::color_palette::{Palette, PickColor, Swatch, THEME_TINTS};
    let apply = move |this: &mut SlideEditor, change: StyleChange, cx: &mut Context<SlideEditor>| this.format_cells(change, cx);
    // Picking a colour from the lately used ones, or a standard one.
    let pick: PickColor = std::rc::Rc::new(move |this, c, _, cx| {
        let change = match (fill, c) {
            (true, c) => StyleChange::Fill(c),
            (false, Some(c)) => StyleChange::Color(c),
            (false, None) => return,
        };
        apply(this, change, cx);
    });
    // テーマの色: the workbook's ten theme colours, each with its five tints.
    let colors = editor.current_theme_palette_colors();
    let theme: Vec<Vec<Swatch>> = (0..6)
        .map(|row| {
            (0..10)
                .map(|i| {
                    let tint = THEME_TINTS[i][row];
                    let theme = ibis_doc_model::ThemeColor { index: i as u8, tint };
                    let rgb = ibis_pptx::tinted(colors[i], tint);
                    let name = crate::shell::i18n::text(THEME_NAMES[i]);
                    let label = match tint {
                        0 => name.to_string(),
                        t if t > 0 => crate::shell::i18n::format("xlsx.theme-lighter", &[("name", &name), ("n", &(t / 10_000))]),
                        t => crate::shell::i18n::format("xlsx.theme-darker", &[("name", &name), ("n", &(-t / 10_000))]),
                    };
                    let change = if fill { StyleChange::FillTheme(rgb, theme) } else { StyleChange::ColorTheme(rgb, theme) };
                    Swatch { rgb, item: MenuItem::new(if fill { "xlsx.theme-fill" } else { "xlsx.theme-color" }, label).on(format(change)) }
                })
                .collect()
        })
        .collect();
    let standard: Vec<Swatch> = (if fill { FILLS } else { COLORS })
        .iter()
        .map(|&(name, c)| Swatch::with_id(if fill { "xlsx.fill" } else { "xlsx.color" }, Rgb(c), crate::shell::i18n::text(name), false, &pick))
        .collect();
    let none = fill.then(|| MenuItem::tr("xlsx.fill-none").icon("format_color_reset").on(format(StyleChange::Fill(None))));
    vec![MenuEntry::Palette(Palette { none, theme, standard, pick: Some(pick), more: None, button: None, current:None })]
}

/// The palette of the colour new border lines are drawn in.
pub(crate) fn border_color_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    let colors: Vec<(SharedString, Rgb, &'static str)> = COLORS.iter().map(|&(label, c)| (SharedString::from(crate::shell::i18n::text(label)), Rgb(c), "xlsx.border-color")).collect();
    let theme = editor.current_theme_palette_colors();
    crate::shell::color_palette::simple_palette(None, Some(&theme), &colors, Some(editor.wb().border_color), std::rc::Rc::new(|this, c, _, cx| {
        if let Some(c) = c {
            this.wb_mut().border_color = c;
            cx.notify();
        }
    }))
}

/// The palette of the toolbar's 枠線の色 (`"border-color"`) button (#115):
/// sets shape stroke color when a shape is selected, or cell border color when cells are selected.
pub(crate) fn toolbar_border_color_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    let colors: Vec<(SharedString, Rgb, &'static str)> = COLORS
        .iter()
        .map(|&(label, c)| (SharedString::from(crate::shell::i18n::text(label)), Rgb(c), "xlsx.toolbar-border-color"))
        .collect();
    let theme = editor.current_theme_palette_colors();
    let none = MenuItem::tr("xlsx.shape-stroke.none")
        .icon("format_color_reset")
        .on(|this, _, cx| {
            if !this.selected_ids().is_empty() || this.selected_sheet_shape().is_some() {
                this.stroke_sheet_shape_color(None, cx);
            } else {
                this.set_borders("none", cx);
            }
        });
    crate::shell::color_palette::simple_palette(
        Some(none),
        Some(&theme),
        &colors,
        Some(editor.wb().border_color),
        std::rc::Rc::new(|this, c, _, cx| {
            if !this.selected_ids().is_empty() || this.selected_sheet_shape().is_some() {
                this.stroke_sheet_shape_color(c.map(ibis_doc_model::Color::opaque), cx);
            } else if let Some(rgb) = c {
                this.wb_mut().border_color = rgb;
                let edges = ["top", "bottom", "left", "right", "inside-h", "inside-v", "diagonal-up", "diagonal-down"];
                let existing: Vec<&'static str> = edges.into_iter().filter(|&k| this.range_edge(k).is_some()).collect();
                if existing.is_empty() {
                    this.set_borders("outside", cx);
                } else {
                    for k in existing {
                        this.set_borders(k, cx);
                    }
                }
            } else {
                this.set_borders("none", cx);
            }
        }),
    )
}

/// Build a color `Palette` menu entry from `(item_id, label_key, hex)` items (preserving item IDs for menu connection/tests)
/// plus the workbook's theme colors (#113).
pub(crate) fn palette_menu_with_ids(
    editor: &SlideEditor,
    none: Option<MenuItem>,
    items: &[(&'static str, &'static str, u32)],
    disabled: bool,
    pick: crate::shell::color_palette::PickColor,
) -> Vec<MenuEntry> {
    use crate::shell::color_palette::{Palette, Swatch, THEME_TINTS};
    let base = editor.current_theme_palette_colors();
    let mut theme: Vec<Vec<Swatch>> = vec![
        base.iter()
            .map(|&rgb| {
                let mut sw = Swatch::new(rgb, format!("#{:06X}", rgb.0), false, &pick);
                sw.item.disabled = disabled;
                sw
            })
            .collect(),
    ];
    if base.len() == 10 {
        for row in 1..6 {
            theme.push(
                (0..10)
                    .map(|i| {
                        let rgb = ibis_pptx::tinted(base[i], THEME_TINTS[i][row]);
                        let mut sw = Swatch::new(rgb, format!("#{:06X}", rgb.0), false, &pick);
                        sw.item.disabled = disabled;
                        sw
                    })
                    .collect(),
            );
        }
    }
    let standard: Vec<Swatch> = items
        .iter()
        .map(|&(item_id, label_key, hex)| {
            let mut sw = Swatch::with_id(item_id, Rgb(hex), crate::shell::i18n::text(label_key), false, &pick);
            sw.item.disabled = disabled;
            sw
        })
        .collect();
    vec![MenuEntry::Palette(Palette {
        none,
        theme,
        standard,
        pick: Some(pick),
        more: None,
        button: None, current:None,
    })]
}

/// The items for the line style of new border lines.
pub(crate) fn border_style_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    BorderLine::ALL.into_iter().map(|line| {
        let label = match line {
            BorderLine::Thin => crate::shell::i18n::text("_xlsx.thin"), BorderLine::Hair => crate::shell::i18n::text("_xlsx.hair"), BorderLine::Medium => crate::shell::i18n::text("_xlsx.medium"), BorderLine::Thick => crate::shell::i18n::text("_shell.thick"),
            BorderLine::Double => crate::shell::i18n::text("_shell.double"), BorderLine::Dotted => crate::shell::i18n::text("_shell.dotted"), BorderLine::Dashed => crate::shell::i18n::text("_shell.dashed"),
            BorderLine::DashDot => crate::shell::i18n::text("docx.picture.border.dash.dashdot"), BorderLine::DashDotDot => crate::shell::i18n::text("_xlsx.dash-dot-dot"), BorderLine::MediumDashed => crate::shell::i18n::text("_xlsx.medium-dashed"),
            BorderLine::MediumDashDot => crate::shell::i18n::text("_xlsx.medium-dash-dot-2"), BorderLine::MediumDashDotDot => crate::shell::i18n::text("_xlsx.medium-dash-dot-dot-2"), BorderLine::SlantDashDot => crate::shell::i18n::text("_xlsx.slant-dash-dot-2"),
        };
        let icon: &'static str = match line {
            BorderLine::Thin => "ibis/border-thin", BorderLine::Hair => "ibis/border-hair", BorderLine::Medium => "ibis/border-medium", BorderLine::Thick => "ibis/border-thick",
            BorderLine::Double => "ibis/border-double", BorderLine::Dotted => "ibis/border-dotted", BorderLine::Dashed => "ibis/border-dashed",
            BorderLine::DashDot => "ibis/border-dashdot", BorderLine::DashDotDot => "ibis/border-dashdotdot", BorderLine::MediumDashed => "ibis/border-mediumdashed",
            BorderLine::MediumDashDot => "ibis/border-mediumdashdot", BorderLine::MediumDashDotDot => "ibis/border-mediumdashdotdot", BorderLine::SlantDashDot => "ibis/border-slantdashdot",
        };
        MenuItem::new(line.ooxml(), label).icon(icon).checked(editor.wb().border_line == line).on(move |this, _, cx| { this.wb_mut().border_line = line; cx.notify(); }).into()
    }).collect()
}

/// The 17 DrawingML underline styles (`a:TextUnderlineType`) for worksheet shapes/textboxes (#156).
/// Excludes cell-only `SingleAccounting` and `DoubleAccounting`.
pub(crate) fn shape_underline_kind_items(now: Option<ibis_doc_model::Underline>) -> Vec<MenuEntry> {
    use ibis_doc_model::Underline as U;
    const KINDS: [(&str, &str, U); 17] = [
        ("pptx.underline.sng", "docx.paragraph-decorations.single", U::Single),
        ("pptx.underline.words", "docx.toolbar.words-only", U::Words),
        ("pptx.underline.dbl", "_docx.double-underline", U::Double),
        ("pptx.underline.heavy", "_docx.thick-underline", U::Heavy),
        ("pptx.underline.dotted", "_shell.dotted", U::Dotted),
        ("pptx.underline.dottedHeavy", "docx.toolbar.thick-dotted", U::DottedHeavy),
        ("pptx.underline.dash", "_shell.dashed", U::Dash),
        ("pptx.underline.dashHeavy", "docx.toolbar.thick-dashed", U::DashHeavy),
        ("pptx.underline.dashLong", "docx.toolbar.long-dashed", U::DashLong),
        ("pptx.underline.dashLongHeavy", "docx.toolbar.thick-long-dashed", U::DashLongHeavy),
        ("pptx.underline.dotDash", "docx.picture.border.dash.dashdot", U::DotDash),
        ("pptx.underline.dotDashHeavy", "docx.toolbar.thick-dash-dot", U::DotDashHeavy),
        ("pptx.underline.dotDotDash", "_docx.dash-dot-dot", U::DotDotDash),
        ("pptx.underline.dotDotDashHeavy", "docx.toolbar.thick-dash-dot-dot", U::DotDotDashHeavy),
        ("pptx.underline.wavy", "_shell.wave", U::Wavy),
        ("pptx.underline.wavyHeavy", "docx.toolbar.thick-wave", U::WavyHeavy),
        ("pptx.underline.wavyDbl", "docx.paragraph-decorations.double-wave", U::WavyDouble),
    ];
    KINDS
        .into_iter()
        .map(|(id, label, kind)| {
            MenuItem::new(id, crate::shell::i18n::text(label))
                .checked(now == Some(kind))
                .on(move |this, _, cx| {
                    this.set_sheet_shape_underline(Some(kind), cx);
                })
                .into()
        })
        .collect()
}

/// Text highlight (background) color palette for worksheet shapes/textboxes (#152).
pub(crate) fn shape_highlight_color_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    let colors: Vec<(SharedString, Rgb, &'static str)> = COLORS
        .iter()
        .map(|&(label, c)| (SharedString::from(crate::shell::i18n::text(label)), Rgb(c), "xlsx.shape-highlight-color"))
        .collect();
    let theme = editor.current_theme_palette_colors();
    let none = MenuItem::tr("pptx.chrome.no-color")
        .icon("format_color_reset")
        .on(|this, _, cx| {
            this.set_sheet_shape_highlight(None, cx);
        });
    crate::shell::color_palette::simple_palette(
        Some(none),
        Some(&theme),
        &colors,
        None,
        std::rc::Rc::new(|this, c, _, cx| {
            this.set_sheet_shape_highlight(c, cx);
        }),
    )
}

pub(crate) fn toolbar_right(editor: &SlideEditor) -> Vec<ToolItem> {
    let style = editor.current_style();
    let format = |change: StyleChange| move |this: &mut SlideEditor, _: &mut Window, cx: &mut Context<SlideEditor>| this.format_cells(change.clone(), cx);
    let mut number_menu: Vec<MenuEntry> = NUMBER_FORMATS.iter().map(|&(name, code)| MenuItem::new("xlsx.number-format", crate::shell::i18n::text(name)).checked(style.number_format == code).on(format(StyleChange::NumberFormat(code.into()))).into()).collect();
    number_menu.push(MenuEntry::Separator);
    number_menu.push(MenuItem::tr("xlsx.number-format-custom").on(|this, window, cx| this.open_custom_format(window, cx)).into());
    let default_font = style.font.clone().unwrap_or_else(|| "游ゴシック".into()); // i18n-ok（既定のフォント名（スタイルに保存される値））
    let font = editor.toolbar_font_display(&default_font);
    let font_menu = crate::shell::font_list::build_font_menu_entries(
        "xlsx.font",
        Vec::new(),
        (!font.is_empty()).then_some(font.as_str()),
        |f, this, _, cx| {
            this.format_cells(StyleChange::Font(Some(f.to_string())), cx);
        },
    );
    let size = style.font_size;
    let size_display = editor.toolbar_font_size_display(size);
    let halign = |id: &'static str, icon: &'static str, label: &'static str, a: CellAlign| -> MenuEntry { MenuItem::new(id, crate::shell::i18n::text(label)).icon(icon).checked(style.align == a).on(format(StyleChange::Align(a))).into() };
    let valign = |id: &'static str, icon: &'static str, label: &'static str, a: CellVerticalAlign| -> MenuEntry { MenuItem::new(id, crate::shell::i18n::text(label)).icon(icon).checked(style.vertical == a).on(format(StyleChange::Vertical(a))).into() };
    let border = |id: &'static str, icon: &'static str, label: &'static str, kind: &'static str| -> MenuEntry { MenuItem::new(id, label).icon(icon).on(move |this, _, cx| this.set_borders(kind, cx)).into() };
    let border_styles = border_style_items(editor);
    let border_colors = border_color_menu(editor);
    let current_text_align = match style.align {
        CellAlign::Left => Some(ibis_doc_model::TextAlign::Left),
        CellAlign::Center | CellAlign::CenterAcross => Some(ibis_doc_model::TextAlign::Center),
        CellAlign::Right => Some(ibis_doc_model::TextAlign::Right),
        CellAlign::Justify => Some(ibis_doc_model::TextAlign::Justify),
        CellAlign::Distributed => Some(ibis_doc_model::TextAlign::Distributed),
        CellAlign::General | CellAlign::Fill => None,
    };
    let [align_l, align_c, align_r, align_drop] = crate::shell::chrome::align_h_toolbar_items_with_id(
        "halign",
        current_text_align,
        true,
        handler(format(StyleChange::Align(CellAlign::Left))),
        handler(format(StyleChange::Align(CellAlign::Center))),
        handler(format(StyleChange::Align(CellAlign::Right))),
        vec![
            halign("xlsx.align-general", "format_align_left", "_xlsx.general", CellAlign::General),
            halign("xlsx.align-left", "format_align_left", "_shell.left", CellAlign::Left),
            halign("xlsx.align-center", "format_align_center", "_shell.center", CellAlign::Center),
            halign("xlsx.align-right", "format_align_right", "_shell.right", CellAlign::Right),
            halign("xlsx.align-center-across", "format_align_center", "_xlsx.center-across-selection", CellAlign::CenterAcross),
            halign("xlsx.align-distributed", "format_align_justify", "_shell.distributed", CellAlign::Distributed),
            halign("xlsx.align-fill", "format_align_left", "_xlsx.fill", CellAlign::Fill),
            halign("xlsx.align-justify", "format_align_justify", "align.justify", CellAlign::Justify),
            MenuEntry::Separator,
            MenuItem::tr("xlsx.shrink").checked(style.shrink).on(move |this, _, cx| this.format_cells(StyleChange::Shrink(!style.shrink), cx)).into(),
        ],
    );
    let current_valign = match style.vertical {
        CellVerticalAlign::Top => Some(ibis_doc_model::VerticalAnchor::Top),
        CellVerticalAlign::Center => Some(ibis_doc_model::VerticalAnchor::Middle),
        CellVerticalAlign::Bottom => Some(ibis_doc_model::VerticalAnchor::Bottom),
        CellVerticalAlign::Justify | CellVerticalAlign::Distributed => None,
    };
    let [valign_t, valign_m, valign_b, valign_drop] = crate::shell::chrome::align_v_toolbar_items_with_id(
        "valign",
        current_valign,
        true,
        handler(format(StyleChange::Vertical(CellVerticalAlign::Top))),
        handler(format(StyleChange::Vertical(CellVerticalAlign::Center))),
        handler(format(StyleChange::Vertical(CellVerticalAlign::Bottom))),
        vec![
            valign("xlsx.valign-top", "vertical_align_top", "_shell.top", CellVerticalAlign::Top),
            valign("xlsx.valign-center", "vertical_align_center", "_shell.center", CellVerticalAlign::Center),
            valign("xlsx.valign-bottom", "vertical_align_bottom", "_shell.bottom", CellVerticalAlign::Bottom),
            valign("xlsx.valign-justify", "vertical_align_center", "align.justify", CellVerticalAlign::Justify),
            valign("xlsx.valign-distributed", "vertical_align_center", "_shell.distributed", CellVerticalAlign::Distributed),
        ],
    );
    // Whether the selection holds a merged range (the merge button is then pressed).
    let merged = match (editor.selected_range(), editor.current_sheet()) {
        (Some(r), Some((_, sheet))) => sheet.merges.iter().any(|m| m.first_row <= r.last_row && r.first_row <= m.last_row && m.first_column <= r.last_column && r.first_column <= m.last_column),
        _ => false,
    };
    vec![
        // A split button as Excel's: the left half is the accounting format of the currency last chosen (¥ at first),
        // the ▼ has the currencies and その他の会計表示形式….
        ToolButton::text("currency", "¥", "_shell.accounting-number-format")
            .menu(vec![
                currency_item("xlsx.currency.jpy", "¥", 0, &style.number_format),
                currency_item("xlsx.currency.usd", "$", 2, &style.number_format),
                currency_item("xlsx.currency.gbp", "£", 2, &style.number_format),
                currency_item("xlsx.currency.eur", "€", 2, &style.number_format),
                currency_item("xlsx.currency.cny", "¥", 2, &style.number_format),
                currency_item("xlsx.currency.chf", "CHF", 2, &style.number_format),
                MenuEntry::Separator,
                MenuItem::tr("xlsx.currency.more").on(|this, window, cx| this.open_custom_format(window, cx)).into(),
            ])
            .last_used(editor, "currency", |i| i.id != "xlsx.currency.more", |i| i.id == "xlsx.currency.jpy", false)
            .into(),
        ToolButton::new("percent", "percent", "_shell.percent-style").click(format(StyleChange::NumberFormat("0%".into()))).into(),
        ToolButton::text("comma", ",", "_shell.comma-style").click(format(StyleChange::NumberFormat("#,##0".into()))).into(),
        ToolButton::new("decimal-less", "decimal_decrease", "_shell.decrease-decimal").click(|this, _, cx| this.step_decimals(false, cx)).into(),
        ToolButton::new("decimal-more", "decimal_increase", "_shell.increase-decimal").click(|this, _, cx| this.step_decimals(true, cx)).into(),
        ToolButton::text("number-format", "123", "_shell.more-number-formats").menu(number_menu).into(),
        sep(),
        ToolItem::Select { id: "font", value: font.into(), width: 128.0, tooltip: crate::shell::i18n::text("_shell.font").into(), menu: font_menu, disabled: false },
        sep(),
        ToolItem::FontSize {
            value: Some(size_display.into()),
            smaller: Some(handler(|this, _, cx| this.step_font_size_selection(false, cx))),
            larger: Some(handler(|this, _, cx| this.step_font_size_selection(true, cx))),
        },
        sep(),
        ToolButton::new("bold", "format_bold", "_shell.bold-b").on(style.bold).click(format(StyleChange::Bold(!style.bold))).into(),
        ToolButton::new("italic", "format_italic", "_shell.italic-i").on(style.italic).click(format(StyleChange::Italic(!style.italic))).into(),
        if editor.has_selected_sheet_text_shapes() {
            let now = editor.selected_sheet_shape_underline();
            ToolButton::new("underline", "format_underlined", "_shell.underline-u")
                .on(now.is_some())
                .click(format(StyleChange::Underline(!now.is_some())))
                .menu(shape_underline_kind_items(now))
                .last_used(editor, "shape-underline", |_| true, |i| i.id == "pptx.underline.sng", false)
                .quick_off(now.is_some(), |this, _, cx| {
                    this.set_sheet_shape_underline(None, cx);
                })
                .into()
        } else {
            ToolButton::new("underline", "format_underlined", "_shell.underline-u").on(style.underline.enabled()).click(format(StyleChange::Underline(!style.underline.enabled())))
                // A split button as Excel's: the left half underlines in the kind last chosen (single at first) or takes the underline off.
                .menu(ibis_doc_model::CellUnderline::ALL.into_iter().map(|u| {
                let label = match u { ibis_doc_model::CellUnderline::None => crate::shell::i18n::text("_xlsx.no-underline"), ibis_doc_model::CellUnderline::Single => crate::shell::i18n::text("_xlsx.single-underline"), ibis_doc_model::CellUnderline::Double => crate::shell::i18n::text("_xlsx.double-underline"), ibis_doc_model::CellUnderline::SingleAccounting => crate::shell::i18n::text("_xlsx.single-accounting"), ibis_doc_model::CellUnderline::DoubleAccounting => crate::shell::i18n::text("_xlsx.double-accounting") };
                MenuItem::new(u.ooxml(), label).checked(style.underline == u).on(format(StyleChange::UnderlineKind(u))).into()
            }).collect())
                .last_used(editor, "underline", |i| i.id != "none", |i| i.id == "single", false)
                .quick_off(style.underline.enabled(), format(StyleChange::UnderlineKind(ibis_doc_model::CellUnderline::None)))
                .into()
        },
        ToolButton::new("strikethrough", "format_strikethrough", "pptx.chrome.strikethrough")
            .on(style.strike)
            .click(format(StyleChange::Strike(!style.strike)))
            .into(),
        ToolButton::new("text-color", "format_color_text", "_shell.font-color").palette(editor, "text-color", 0xff0000, cell_color_menu(editor, false)).into(),
        ToolButton::new("highlight", "ink_highlighter", "pptx.chrome.highlight-color")
            .palette(editor, "highlight", 0xffff00, shape_highlight_color_menu(editor))
            .into(),
        ToolButton::new("clear-format", "format_clear", crate::shell::i18n::text("pptx.chrome.clear-all-formatting")).click(|this, _, cx| this.clear_cell_formats(cx)).into(),
        sep(),
        ToolButton::new("fill", "format_color_fill", "_shell.fill-color").palette(editor, "fill", 0xffff00, cell_color_menu(editor, true)).into(),
        ToolButton::new("border-color", "border_color", "_shell.border-color").palette(editor, "border-color", 0x000000, toolbar_border_color_menu(editor)).into(),
        ToolButton::new("sheet-list", "menu", "").searchable().single_select().hidden().menu(crate::xlsx::sheet_ui::sheet_list_menu_entries(editor)).into(),
        // A split button as Excel's: the left half draws the border last chosen (the bottom border at first), with its icon.
        ToolButton::new("borders", "border_bottom", "_shell.line")
            .menu(vec![
                border("xlsx.border-all", "border_all", "_xlsx.all-borders", "all"),
                border("xlsx.border-outside", "border_outer", "_xlsx.outside-borders", "outside"),
                border("xlsx.border-bottom", "border_bottom", "_xlsx.bottom-border", "bottom"),
                border("xlsx.border-top", "border_top", "_xlsx.top-border", "top"),
                border("xlsx.border-left", "border_left", "_xlsx.left-border", "left"),
                border("xlsx.border-right", "border_right", "_xlsx.right-border", "right"),
                border("xlsx.border-diagonal-up", "border_all", "_xlsx.diagonal-up-border", "diagonal-up"),
                border("xlsx.border-diagonal-down", "border_all", "_xlsx.diagonal-down-border", "diagonal-down"),
                border("xlsx.border-thick-bottom", "border_bottom", "_xlsx.thick-bottom-border", "thick-bottom"),
                border("xlsx.border-double-bottom", "border_bottom", "_xlsx.double-bottom-border", "double-bottom"),
                border("xlsx.border-none", "border_clear", "_xlsx.no-border", "none"),
                MenuEntry::Separator,
                Submenu::tr("xlsx.border-styles", "border_all", border_styles).into(),
                Submenu::tr("xlsx.border-colors", "palette", border_colors).into(),
            ])
            .last_used(editor, "borders", |i| i.id.starts_with("xlsx.border-") && i.id != "xlsx.border-color", |i| i.id == "xlsx.border-bottom", true)
            .into(),
        // A split button as Excel's: the left half is 結合して中央揃え (and takes the merge off where the selection has one).
        ToolButton::new("merge", "cell_merge", "_shell.merge-cells")
            .on(merged)
            .menu(vec![
                MenuItem::tr("xlsx.merge-center").icon("ibis/merge-center").on(|this, _, cx| this.merge_cells(true, cx)).into(),
                MenuItem::tr("xlsx.merge-across").icon("ibis/merge-across").on(|this, _, cx| this.merge_across(cx)).into(),
                MenuItem::tr("xlsx.merge").icon("cell_merge").on(|this, _, cx| this.merge_cells(false, cx)).into(),
                MenuItem::tr("xlsx.unmerge").icon("ibis/unmerge").on(|this, _, cx| this.unmerge_cells(cx)).into(),
            ])
            .quick(move |this, _, cx| if merged { this.unmerge_cells(cx) } else { this.merge_cells(true, cx) })
            .into(),
        sep(),
        align_l,
        align_c,
        align_r,
        align_drop,
        valign_t,
        valign_m,
        valign_b,
        valign_drop,
        ToolButton::new("wrap", "wrap_text", "_shell.wrap-text").on(style.wrap).click(format(StyleChange::Wrap(!style.wrap))).into(),
        {
            let cur_list = editor.selected_sheet_shape_list();
            let bullets_on = matches!(cur_list, Some(ibis_doc_model::ListKind::Bullet))
                || matches!(cur_list, Some(ibis_doc_model::ListKind::Symbol(c)) if c != '☐' && c != '☑');
            ToolButton::new("bullets", "format_list_bulleted", "_shell.bullets")
                .on(bullets_on)
                .click(|this, _, cx| {
                    this.toggle_sheet_shape_list(ibis_doc_model::ListKind::Bullet, cx);
                })
                .into()
        },
        {
            let cur_list = editor.selected_sheet_shape_list();
            let numbered_on = matches!(cur_list, Some(ibis_doc_model::ListKind::Numbered | ibis_doc_model::ListKind::Scheme(_)));
            ToolButton::new("numbered", "format_list_numbered", "_shell.numbering")
                .on(numbered_on)
                .click(|this, _, cx| {
                    this.toggle_sheet_shape_list(ibis_doc_model::ListKind::Numbered, cx);
                })
                .into()
        },
        {
            let cur_list = editor.selected_sheet_shape_list();
            let checklist_on = matches!(cur_list, Some(ibis_doc_model::ListKind::Symbol('☐' | '☑')));
            ToolButton::new("checklist", "checklist", "checklist")
                .on(checklist_on)
                .click(|this, _, cx| {
                    this.toggle_sheet_shape_list(ibis_doc_model::ListKind::Symbol('☐'), cx);
                })
                .into()
        },
        ToolButton::new("indent-less", "format_indent_decrease", "_xlsx.decrease-indent").click(|this, _, cx| this.step_sheet_indent(false, cx)).into(),
        ToolButton::new("indent-more", "format_indent_increase", "_xlsx.increase-indent").click(|this, _, cx| this.step_sheet_indent(true, cx)).into(),
        sep(),
        ToolButton::new("functions", "functions", "_shell.functions")
            .menu(super::autosum::FUNCTIONS.iter().map(|&(f, label)| MenuItem::new("xlsx.function", crate::shell::i18n::format("xlsx.toolbar.function-item", &[("name", &f), ("label", &crate::shell::i18n::text(label))])).on(move |this, _, cx| this.auto_function(f, cx)).into()).collect())
            // A split button as Excel's: the left half is the sum.
            .quick(|this, _, cx| this.auto_function("SUM", cx))
            .into(),
    ]
}

/// Put `new` at the end of the submenu `id` (in whatever menu it is).
fn push_into(entries: &mut Vec<MenuEntry>, id: &str, new: MenuEntry) -> Result<(), MenuEntry> {
    let mut new = new;
    for e in entries.iter_mut() {
        if let MenuEntry::Submenu(sub) = e {
            if sub.id == id {
                sub.items.push(new);
                return Ok(());
            }
            match push_into(&mut sub.items, id, new) {
                Ok(()) => return Ok(()),
                Err(back) => new = back,
            }
        }
    }
    Err(new)
}

/// Put `new` after the item `id` (in whatever menu or submenu it is).
fn insert_after(entries: &mut Vec<MenuEntry>, id: &str, new: MenuEntry) -> Result<(), MenuEntry> {
    if let Some(i) = entries.iter().position(|e| match e {
        MenuEntry::Item(item) => item.id == id,
        MenuEntry::Submenu(sub) => sub.id == id,
        _ => false,
    }) {
        entries.insert(i + 1, new);
        return Ok(());
    }
    let mut new = new;
    for e in entries.iter_mut() {
        if let MenuEntry::Submenu(sub) = e {
            match insert_after(&mut sub.items, id, new) {
                Ok(()) => return Ok(()),
                Err(back) => new = back,
            }
        }
    }
    Err(new)
}

/// ワードアート・文字変形のプリセット (ECMA-376 a:prstTxWarp)
pub(crate) const XLSX_TEXT_WARP_PRESETS: &[(&str, &str, &str)] = &[
    ("textArchUp", "xlsx.text.warp.arch-up", "上アーチ"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textArchDown", "xlsx.text.warp.arch-down", "下アーチ"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textCircle", "xlsx.text.warp.circle", "pptx.common.circle"),
    ("textButton", "xlsx.text.warp.button", "ボタン"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textCurveUp", "xlsx.text.warp.curve-up", "上カーブ"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textCurveDown", "xlsx.text.warp.curve-down", "下カーブ"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textWave1", "xlsx.text.warp.wave1", "波 1"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textWave2", "xlsx.text.warp.wave2", "波 2"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textDoubleWave1", "xlsx.text.warp.double-wave1", "二重波 1"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textInflate", "xlsx.text.warp.inflate", "膨張"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textDeflate", "xlsx.text.warp.deflate", "収縮"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textStop", "xlsx.text.warp.stop", "停止標識"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textTriangle", "xlsx.text.warp.triangle", "三角形"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textTriangleInverted", "xlsx.text.warp.triangle-inverted", "逆三角形"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textChevron", "xlsx.text.warp.chevron", "山形"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textChevronInverted", "xlsx.text.warp.chevron-inverted", "逆山形"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textFadeRight", "xlsx.text.warp.fade-right", "右フェード"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textFadeLeft", "xlsx.text.warp.fade-left", "左フェード"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textFadeUp", "xlsx.text.warp.fade-up", "上フェード"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
    ("textFadeDown", "xlsx.text.warp.fade-down", "下フェード"), // i18n-ok（未使用の欄（_name。表示は MenuItem::tr(id) の表の文字を使）
];

const XLSX_WORDART_STYLES: &[(&'static str, &'static str, &'static str)] = &[
    ("xlsx.wordart.style1", "xlsx.insert.wordart.style1", "xlsx.text.wordart.style1"),
    ("xlsx.wordart.style2", "xlsx.insert.wordart.style2", "xlsx.text.wordart.style2"),
    ("xlsx.wordart.style3", "xlsx.insert.wordart.style3", "xlsx.text.wordart.style3"),
    ("xlsx.wordart.style4", "xlsx.insert.wordart.style4", "xlsx.text.wordart.style4"),
    ("xlsx.wordart.style5", "xlsx.insert.wordart.style5", "xlsx.text.wordart.style5"),
    ("xlsx.wordart.style6", "xlsx.insert.wordart.style6", "xlsx.text.wordart.style6"),
];

fn wordart_insert_items() -> Vec<MenuEntry> {
    XLSX_WORDART_STYLES.iter().enumerate().map(|(idx, &(name_id, insert_id, _))| {
        MenuItem::new(insert_id, crate::shell::i18n::text(name_id)).on(move |e, _, cx| {
            e.insert_sheet_wordart(idx, None, cx);
        }).into()
    }).collect()
}

fn wordart_style_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let has_text = editor.edited_body().is_some() || editor.selected_sheet_shape().and_then(|id| editor.slide().shape(id)).is_some_and(|s| s.text_body().is_some() || matches!(s.kind, ibis_doc_model::ShapeKind::TextBox(_) | ibis_doc_model::ShapeKind::Auto(_)));
    XLSX_WORDART_STYLES.iter().enumerate().map(|(idx, &(name_id, _, apply_id))| {
        MenuItem::new(apply_id, crate::shell::i18n::text(name_id)).disabled(!has_text).on(move |e, w, cx| {
            e.set_selected_sheet_wordart_style(idx, w, cx);
        }).into()
    }).collect()
}

fn text_warp_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let current_warp = editor.selected_sheet_text_warp();
    let has_text = editor.edited_body().is_some() || editor.selected_sheet_shape().and_then(|id| editor.slide().shape(id)).is_some_and(|s| s.text_body().is_some() || matches!(s.kind, ibis_doc_model::ShapeKind::TextBox(_) | ibis_doc_model::ShapeKind::Auto(_)));
    let mut items: Vec<MenuEntry> = Vec::new();
    items.push(MenuItem::tr("xlsx.text.warp.none").disabled(!has_text).checked(current_warp.is_none()).on(|e, _, cx| e.set_sheet_text_warp(None, cx)).into());
    items.push(MenuEntry::Separator);
    for &(prst, id, _name) in XLSX_TEXT_WARP_PRESETS {
        let is_checked = current_warp.as_deref() == Some(prst);
        items.push(MenuItem::tr(id).disabled(!has_text).checked(is_checked).on(move |e, _, cx| e.set_sheet_text_warp(Some(prst), cx)).into());
    }
    items
}

pub(crate) fn menus(editor: &SlideEditor, menus: &mut Menus) {
    // ツール > スペル > スペルチェック… / 類義語辞典
    fill_submenu(
        menus,
        "tools.spelling",
        vec![
            MenuItem::tr("xlsx.spelling.check").icon("spellcheck").on(|e, w, cx| e.open_sheet_spelling(w, cx)).into(),
            MenuItem::tr("pptx.spelling.check").icon("spellcheck").on(|e, w, cx| e.open_sheet_spelling(w, cx)).into(),
            MenuItem::tr("xlsx.thesaurus").icon("menu_book").on(|e, w, cx| e.open_sheet_thesaurus(w, cx)).into(),
        ],
    );
    // 書式 > テキスト: 下線（下線・二重下線）
    let style = editor.current_style();
    let underline_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.underline-single")
            .icon("format_underlined")
            .shortcut("⌘U")
            .checked(style.underline == ibis_doc_model::CellUnderline::Single)
            .on(|this, _, cx| this.format_cells(ibis_doc_model::StyleChange::UnderlineKind(ibis_doc_model::CellUnderline::Single), cx))
            .into(),
        MenuItem::tr("xlsx.underline-double")
            .icon("format_underlined")
            .checked(style.underline == ibis_doc_model::CellUnderline::Double)
            .on(|this, _, cx| this.format_cells(ibis_doc_model::StyleChange::UnderlineKind(ibis_doc_model::CellUnderline::Double), cx))
            .into(),
    ];
    fill_submenu(
        menus,
        "format.text",
        vec![
            crate::shell::chrome::Submenu::tr("xlsx.underline-menu", "format_underlined", underline_items).into(),
        ],
    );
    // 編集 > 画像としてコピー (after コピー).
    let mut copy_as_pic: MenuEntry = MenuItem::tr("xlsx.copy-as-picture").icon("image").shortcut("⌥⇧⌘C").on(|this, _, cx| this.copy_as_picture(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "edit.copy", copy_as_pic) {
            Ok(()) => break,
            Err(back) => copy_as_pic = back,
        }
    }
    // シートを再表示: the hidden sheets, after シートを非表示.
    let hidden: Vec<(usize, String)> = editor
        .presentation()
        .workbook
        .as_ref()
        .map(|w| w.sheets.iter().enumerate().filter(|(_, s)| s.slide.is_none() && s.hidden.is_some() && !s.other).map(|(i, s)| (i, s.name.clone())).collect())
        .unwrap_or_default();
    let mut items: Vec<MenuEntry> = hidden.into_iter().map(|(i, name)| MenuItem::new("xlsx.unhide-sheet", name).on(move |this, window, cx| this.unhide_sheet(i, window, cx)).into()).collect();
    if !items.is_empty() {
        items.push(MenuEntry::Separator);
        items.push(MenuItem::tr("xlsx.unhide-all-sheets").on(|this, window, cx| this.unhide_all_sheets(window, cx)).into());
    }
    let mut entry: MenuEntry = if items.is_empty() {
        MenuItem::tr("xlsx.unhide-sheet").icon("visibility").disabled(true).into()
    } else {
        crate::shell::chrome::Submenu::tr("xlsx.unhide-sheets", "visibility", items).into()
    };
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.hide-sheet", entry) {
            Ok(()) => break,
            Err(back) => entry = back,
        }
    }
    menus.connect("xlsx.hide-sheet", |this, _, cx| this.hide_sheet(cx));
    menus.connect("xlsx.duplicate-sheet", |this, window, cx| this.copy_sheet(window, cx));
    // 見出しの色, after シートの名前を変更.
    let colors: Vec<MenuEntry> = {
        let list: Vec<(SharedString, Rgb, &'static str)> = COLORS.iter().map(|&(name, c)| (SharedString::from(crate::shell::i18n::text(name)), Rgb(c), "xlsx.tab-color")).collect();
        let theme = editor.current_theme_palette_colors();
        let none = MenuItem::tr("xlsx.tab-color-none").icon("format_color_reset").on(|this, _, cx| this.set_tab_color(None, cx));
        crate::shell::color_palette::simple_palette(Some(none), Some(&theme), &list, None, std::rc::Rc::new(|this, c, _, cx| this.set_tab_color(c, cx)))
    };
    let mut tab: MenuEntry = crate::shell::chrome::Submenu::tr("xlsx.tab-colors", "palette", colors).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.rename-sheet", tab) {
            Ok(()) => break,
            Err(back) => tab = back,
        }
    }
    for (after, entry) in [
        ("xlsx.duplicate-sheet", MenuItem::tr("xlsx.move-sheet-left").on(|this, _, cx| this.move_sheet(-1, cx))),
        ("xlsx.move-sheet-left", MenuItem::tr("xlsx.move-sheet-right").on(|this, _, cx| this.move_sheet(1, cx))),
        ("xlsx.move-sheet-right", MenuItem::tr("xlsx.move-sheet-first").on(|this, _, cx| this.move_sheet_to_edge(true, cx))),
        ("xlsx.move-sheet-first", MenuItem::tr("xlsx.move-sheet-last").on(|this, _, cx| this.move_sheet_to_edge(false, cx))),
    ] {
        let mut entry: MenuEntry = entry.into();
        for (_, entries) in menus.menus.iter_mut() {
            match insert_after(entries, after, entry) {
                Ok(()) => break,
                Err(back) => entry = back,
            }
        }
    }
    menus.connect("xlsx.freeze", |this, _, cx| this.toggle_freeze(cx));
    menus.connect("xlsx.freeze-panes", |this, _, cx| this.freeze_panes(cx));
    menus.connect("xlsx.unfreeze-panes", |this, _, cx| this.unfreeze_panes(cx));
    menus.connect("xlsx.freeze-top-row", |this, _, cx| this.freeze_top_row(cx));
    menus.connect("xlsx.freeze-first-column", |this, _, cx| this.freeze_first_column(cx));
    // 表示 > 分割 (after ウィンドウ枠の固定).
    let mut split: MenuEntry = MenuItem::tr("xlsx.split").checked(editor.split_panes().is_some()).on(|this, _, cx| this.toggle_split(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.freeze", split) {
            Ok(()) => break,
            Err(back) => split = back,
        }
    }
    menus.connect("xlsx.sort", |this, _, cx| this.sort_by_selected_column(true, cx));
    let mut sort_manage: MenuEntry = MenuItem::tr("xlsx.sort-manage").icon("sort").on(|this, window, cx| this.open_sort_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.sort", sort_manage) {
            Ok(()) => break,
            Err(back) => sort_manage = back,
        }
    }
    menus.connect("xlsx.filter", |this, _, cx| this.toggle_auto_filter(cx));
    let mut adv_filter: MenuEntry = MenuItem::tr("xlsx.advanced-filter").icon("filter_alt").on(|this, window, cx| this.open_advanced_filter_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.filter", adv_filter) {
            Ok(()) => break,
            Err(back) => adv_filter = back,
        }
    }
    menus.connect("xlsx.advanced-filter", |this, window, cx| this.open_advanced_filter_panel(window, cx));
    // 並べ替えとフィルター > 再適用 (Excel's ホーム / データ > 並べ替えとフィルター > 再適用, ⌥⌘L)
    let has_filter = editor.current_sheet().is_some_and(|(_, s)| s.auto_filter.is_some() || s.tables.iter().any(|t| t.filter.is_some()));
    let mut reapply: MenuEntry = MenuItem::tr("xlsx.reapply-filter").icon("refresh").shortcut("⌥⌘L").disabled(!has_filter).on(|this, _, cx| this.reapply_auto_filter(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.advanced-filter", reapply) {
            Ok(()) => break,
            Err(back) => reapply = back,
        }
    }
    menus.connect("xlsx.reapply-filter", |this, _, cx| this.reapply_auto_filter(cx));
    menus.connect("xlsx.filter.reapply", |this, _, cx| this.reapply_auto_filter(cx));

    // 並べ替えとフィルター > クリア (Excel's ホーム / データ > 並べ替えとフィルター > クリア)
    let mut clear_flt: MenuEntry = MenuItem::tr("xlsx.clear-filter").icon("filter_alt_off").disabled(!has_filter).on(|this, _, cx| this.clear_auto_filter(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.reapply-filter", clear_flt) {
            Ok(()) => break,
            Err(back) => clear_flt = back,
        }
    }
    menus.connect("xlsx.clear-filter", |this, _, cx| this.clear_auto_filter(cx));
    menus.connect("xlsx.filter.clear", |this, _, cx| this.clear_auto_filter(cx));
    menus.connect("xlsx.filter-clear", |this, _, cx| this.clear_auto_filter(cx));
    menus.connect("xlsx.sort-ascending", |this, _, cx| this.sort_by_selected_column(true, cx));
    // データ > 重複の削除 (after the sorting).
    let mut dedupe: MenuEntry = MenuItem::tr("xlsx.remove-duplicates").icon("sort").on(|this, window, cx| this.open_dedupe_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.sort-manage", dedupe) {
            Ok(()) => break,
            Err(back) => dedupe = back,
        }
    }
    // 編集 > ジャンプ... (after 検索と置換).
    let mut goto_menu: MenuEntry = MenuItem::tr("xlsx.goto").shortcut("⌃G").icon("find_replace").on(|this, window, cx| this.open_goto(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "edit.find-replace", goto_menu) {
            Ok(()) => break,
            Err(back) => goto_menu = back,
        }
    }
    menus.connect("xlsx.goto", |this, window, cx| this.open_goto(window, cx));

    // 編集 > 条件を選択してジャンプ (after ジャンプ).
    let mut goto_special: MenuEntry = MenuItem::tr("xlsx.goto-special").icon("find_replace").on(|this, _, cx| this.open_goto_special_panel(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.goto", goto_special) {
            Ok(()) => break,
            Err(back) => goto_special = back,
        }
    }
    menus.connect("xlsx.goto-special", |this, _, cx| this.open_goto_special_panel(cx));
    // 編集 > 検索と選択 (Excel's ホーム > 編集 > 検索と選択: 1 クリック選択)
    use super::goto_special::Special as GS;
    let find_select_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.select-formulas").on(|this, _, cx| this.select_special_kind(GS::Formulas, cx)).into(),
        MenuItem::tr("xlsx.select-notes").on(|this, _, cx| this.select_special_kind(GS::Comments, cx)).into(),
        MenuItem::tr("xlsx.select-conditional-formats").on(|this, _, cx| this.select_special_kind(GS::Conditional, cx)).into(),
        MenuItem::tr("xlsx.select-constants").on(|this, _, cx| this.select_special_kind(GS::Constants, cx)).into(),
        MenuItem::tr("xlsx.select-data-validation").on(|this, _, cx| this.select_special_kind(GS::Validation, cx)).into(),
        MenuItem::tr("xlsx.select-blanks").on(|this, _, cx| this.select_special_kind(GS::Blanks, cx)).into(),
        MenuItem::tr("xlsx.select-visible-cells").on(|this, _, cx| this.select_special_kind(GS::Visible, cx)).into(),
        MenuItem::tr("xlsx.select-current-region").on(|this, _, cx| this.select_special_kind(GS::CurrentRegion, cx)).into(),
        MenuItem::tr("xlsx.select-objects").on(|this, _, cx| this.select_special_kind(GS::Objects, cx)).into(),
    ];
    let mut find_and_select_menu: MenuEntry = crate::shell::chrome::Submenu::tr("xlsx.find-and-select", "find_replace", find_select_items).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.goto-special", find_and_select_menu) {
            Ok(()) => break,
            Err(back) => find_and_select_menu = back,
        }
    }
    // 編集 > オブジェクトの選択と表示 (Excel's ホーム > 編集 / ページ レイアウト > 配置)
    let sel_pane_open = editor.sheet_state().is_some_and(|s| s.panels.selection_pane.is_some());
    let mut sel_pane_edit: MenuEntry = MenuItem::tr("xlsx.selection-pane").icon("visibility").checked(sel_pane_open).on(|this, window, cx| { this.toggle_xlsx_selection_pane(window, cx); }).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.find-and-select", sel_pane_edit) {
            Ok(()) => break,
            Err(back) => sel_pane_edit = back,
        }
    }
    menus.items_mut(crate::shell::chrome::MenuId::Arrange).push(
        MenuItem::tr("xlsx.selection-pane")
            .icon("visibility")
            .checked(sel_pane_open)
            .on(|this, window, cx| {
                this.toggle_xlsx_selection_pane(window, cx);
            })
            .into(),
    );
    // 挿入 > メモ/コメント (7 items grouped into a submenu, #139; コメントとメモのクリア stays in 編集 > クリア, #140).
    let has_note = editor.current_cell().zip(editor.current_sheet()).is_some_and(|((c, r), (_, sheet))| sheet.comment_at(r, c).is_some());
    let has_thread = editor.current_cell().zip(editor.current_sheet()).is_some_and(|((c, r), (_, sheet))| sheet.thread_at(r, c).is_some());
    let has_any_comments = editor.current_sheet().is_some_and(|(_, s)| !s.comments.is_empty() || !s.threads.is_empty());
    let show_all_notes = editor.sheet_state().is_some_and(|s| s.show_all_notes);
    let notes_comments_items: Vec<MenuEntry> = vec![
        MenuItem::tr("insert.comment").icon("add_comment").shortcut("⌥⌘M").on(|this, window, cx| this.open_threaded_comment_panel(window, cx)).into(),
        MenuItem::tr("xlsx.comment-delete").icon("delete").disabled(!has_thread).on(|this, _, cx| {
            if let Some((c, r)) = this.current_cell() {
                this.delete_threaded_comment(r, c, cx);
            }
        }).into(),
        MenuEntry::Separator,
        MenuItem::tr(if has_note { "xlsx.note-edit" } else { "xlsx.note-new" }).icon("note_add").shortcut("⇧F2").on(|this, window, cx| this.open_note_panel(window, cx)).into(),
        MenuItem::tr("xlsx.note-delete").icon("delete").disabled(!has_note).on(|this, _, cx| this.delete_notes(cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.previous-comment").icon("arrow_back").disabled(!has_any_comments).on(|this, _, cx| this.select_next_comment(false, cx)).into(),
        MenuItem::tr("xlsx.next-comment").icon("arrow_forward").disabled(!has_any_comments).on(|this, _, cx| this.select_next_comment(true, cx)).into(),
        MenuItem::tr("xlsx.show-all-notes").icon("visibility").checked(show_all_notes).on(|this, _, cx| this.toggle_show_all_notes(cx)).into(),
    ];
    let notes_comments_sub: MenuEntry = crate::shell::chrome::Submenu::tr("xlsx.notes-comments-menu", "comment", notes_comments_items).into();
    let insert_entries = menus.items_mut(crate::shell::chrome::MenuId::Insert);
    if let Some(pos) = insert_entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "insert.comment")) {
        insert_entries[pos] = notes_comments_sub;
    } else {
        insert_entries.push(notes_comments_sub);
    }

    let is_landscape = editor.current_print().is_some_and(|p| p.landscape);
    let orientation_items = vec![
        MenuItem::tr("xlsx.orientation-portrait")
            .icon("crop_portrait")
            .checked(!is_landscape)
            .on(|this, _, cx| {
                this.set_print(|p| p.landscape = false, None, cx);
            })
            .into(),
        MenuItem::tr("xlsx.orientation-landscape")
            .icon("crop_landscape")
            .checked(is_landscape)
            .on(|this, _, cx| {
                this.set_print(|p| p.landscape = true, None, cx);
            })
            .into(),
    ];
    let mut page_setup_menu: MenuEntry = MenuItem::tr("xlsx.page-setup").icon("settings").on(|this, _, cx| this.open_page_setup(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "file.page-setup", page_setup_menu) {
            Ok(()) => break,
            Err(back) => page_setup_menu = back,
        }
    }
    let mut margins_menu: MenuEntry = MenuItem::tr("xlsx.margins").icon("crop_portrait").on(|this, _, cx| this.open_page_setup(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.page-setup", margins_menu) {
            Ok(()) => break,
            Err(back) => margins_menu = back,
        }
    }
    let mut hf_menu: MenuEntry = MenuItem::tr("xlsx.header-footer").icon("view_headline").on(|this, window, cx| this.open_header_footer_edit(super::header_footer_edit::HfTarget::Header, window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.margins", hf_menu) {
            Ok(()) => break,
            Err(back) => hf_menu = back,
        }
    }

    let mut page_orient: MenuEntry = crate::shell::chrome::Submenu::tr("xlsx.page-orientation", "rotate_right", orientation_items).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.header-footer", page_orient) {
            Ok(()) => break,
            Err(back) => page_orient = back,
        }
    }

    let current_paper = editor.current_print().map(|p| p.paper).unwrap_or(ibis_doc_model::SheetPrint::A4);
    let paper_items = vec![
        MenuItem::tr("xlsx.paper-a4")
            .checked(current_paper == ibis_doc_model::SheetPrint::A4)
            .on(|this, _, cx| {
                this.set_print(|p| p.paper = ibis_doc_model::SheetPrint::A4, None, cx);
            })
            .into(),
        MenuItem::tr("xlsx.paper-a3")
            .checked(current_paper == (297 * 36_000, 420 * 36_000))
            .on(|this, _, cx| {
                this.set_print(|p| p.paper = (297 * 36_000, 420 * 36_000), None, cx);
            })
            .into(),
        MenuItem::tr("xlsx.paper-b5")
            .checked(current_paper == (182 * 36_000, 257 * 36_000))
            .on(|this, _, cx| {
                this.set_print(|p| p.paper = (182 * 36_000, 257 * 36_000), None, cx);
            })
            .into(),
        MenuItem::tr("xlsx.paper-letter")
            .checked(current_paper == (7_772_400, 10_058_400))
            .on(|this, _, cx| {
                this.set_print(|p| p.paper = (7_772_400, 10_058_400), None, cx);
            })
            .into(),
    ];
    let mut paper_menu: MenuEntry = crate::shell::chrome::Submenu::tr("xlsx.paper-size", "description", paper_items).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.page-orientation", paper_menu) {
            Ok(()) => break,
            Err(back) => paper_menu = back,
        }
    }

    let has_titles = editor.print_title_rows().is_some() || editor.print_title_columns().is_some();
    let print_title_items = vec![
        MenuItem::tr("xlsx.print-titles-dialog")
            .icon("settings")
            .on(|this, _, cx| {
                this.open_page_setup(cx);
            })
            .into(),
        MenuItem::tr("xlsx.print-titles-set-rows")
            .icon("view_stream")
            .on(|this, _, cx| {
                let rows = this.selected_range().map(|r| (r.first_row, r.last_row)).or_else(|| this.current_cell().map(|(_, r)| (r, r)));
                this.set_print_title_rows(rows, cx);
            })
            .into(),
        MenuItem::tr("xlsx.print-titles-clear")
            .icon("clear")
            .disabled(!has_titles)
            .on(|this, _, cx| {
                this.set_print_title_rows(None, cx);
                this.set_print_title_columns(None, cx);
            })
            .into(),
    ];
    let mut titles_menu: MenuEntry = crate::shell::chrome::Submenu::tr("xlsx.print-titles", "view_stream", print_title_items).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.paper-size", titles_menu) {
            Ok(()) => break,
            Err(back) => titles_menu = back,
        }
    }
    let cur_print = editor.current_print().unwrap_or_default();
    let fit_w = cur_print.fit_to.map(|(w, _)| w).unwrap_or(0);
    let fit_h = cur_print.fit_to.map(|(_, h)| h).unwrap_or(0);
    let scale_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.scale-width-auto")
            .checked(fit_w == 0)
            .on(|this, _, cx| this.set_print_fit_width(0, cx))
            .into(),
        MenuItem::tr("xlsx.scale-width-1")
            .checked(fit_w == 1)
            .on(|this, _, cx| this.set_print_fit_width(1, cx))
            .into(),
        MenuItem::tr("xlsx.scale-width-2")
            .checked(fit_w == 2)
            .on(|this, _, cx| this.set_print_fit_width(2, cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.scale-height-auto")
            .checked(fit_h == 0)
            .on(|this, _, cx| this.set_print_fit_height(0, cx))
            .into(),
        MenuItem::tr("xlsx.scale-height-1")
            .checked(fit_h == 1)
            .on(|this, _, cx| this.set_print_fit_height(1, cx))
            .into(),
        MenuItem::tr("xlsx.scale-height-2")
            .checked(fit_h == 2)
            .on(|this, _, cx| this.set_print_fit_height(2, cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.scale-100")
            .checked(cur_print.fit_to.is_none() && cur_print.scale == 100)
            .on(|this, _, cx| this.set_print_scale_percent(100, cx))
            .into(),
        MenuItem::tr("xlsx.scale-80")
            .checked(cur_print.fit_to.is_none() && cur_print.scale == 80)
            .on(|this, _, cx| this.set_print_scale_percent(80, cx))
            .into(),
        MenuItem::tr("xlsx.scale-50")
            .checked(cur_print.fit_to.is_none() && cur_print.scale == 50)
            .on(|this, _, cx| this.set_print_scale_percent(50, cx))
            .into(),
    ];
    let mut scale_menu: MenuEntry = crate::shell::chrome::Submenu::tr("xlsx.scale-to-fit", "fit_screen", scale_items).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.print-titles", scale_menu) {
            Ok(()) => break,
            Err(back) => scale_menu = back,
        }
    }
    // ツール > シートの保護… / シート保護の解除…, ブックの保護… / ブック保護の解除…, and 表示形式 > セルのロック・数式を表示しない.
    let protected = editor.sheet_protection().is_some();
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr(if protected { "xlsx.unprotect-sheet" } else { "xlsx.protect-sheet" }).icon("lock").on(|this, window, cx| this.open_protect_panel(window, cx)).into());
    let wb_protected = editor.workbook_protection().is_some();
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr(if wb_protected { "xlsx.unprotect-workbook" } else { "xlsx.protect-workbook" }).icon("lock").on(|this, window, cx| this.open_protect_workbook_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.allow-edit-ranges").icon("lock").on(|this, window, cx| this.open_allow_edit_ranges_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.accessibility-check").icon("accessibility").on(|this, window, cx| this.open_xlsx_accessibility_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.compatibility-check").icon("rule").on(|this, window, cx| this.open_xlsx_compatibility_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.error-options").icon("rule").on(|this, window, cx| this.open_error_options_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.autocorrect").icon("spellcheck").on(|this, window, cx| this.open_autocorrect_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.autorecover-settings").icon("settings_backup_restore").on(|this, window, cx| { this.wb_mut().close_settings(); this.open_autorecover_settings(window, cx) }).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.recover-unsaved").icon("restore_page").on(|this, _window, cx| { this.wb_mut().close_settings(); this.open_recover_unsaved(cx) }).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.backup-save-settings").icon("save_as").on(|this, window, cx| this.open_xlsx_backup_save_settings_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.open-and-repair").icon("cleaning_services").disabled(super::repair::damaged_workbook().is_none()).on(|this, window, cx| this.open_xlsx_repair_damaged(window, cx)).into());
    let hide_ink = editor.current_sheet().is_some_and(|(_, s)| s.hide_ink);
    menus.items_mut(crate::shell::chrome::MenuId::Tools).push(MenuItem::tr("xlsx.hide-ink").icon("visibility_off").checked(hide_ink).on(|this, _, cx| this.toggle_hide_ink(cx)).into());
    let style = editor.current_style();
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(MenuItem::tr("xlsx.lock-cells").checked(!style.unlocked).on(|this, _, cx| this.toggle_cell_protection(false, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(MenuItem::tr("xlsx.hide-formulas").checked(style.formula_hidden).on(|this, _, cx| this.toggle_cell_protection(true, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(MenuItem::tr("xlsx.comma-style").checked(style.number_format == "#,##0").on(|this, _, cx| this.format_cells(ibis_doc_model::StyleChange::NumberFormat("#,##0".into()), cx)).into());
    let show_phonetic = editor
        .presentation()
        .slide(editor.current_slide)
        .and_then(ibis_doc_model::sheet_calc::sheet_of_slide)
        .map(|s| s.show_phonetic)
        .unwrap_or(false);
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.phonetic-toggle")
            .checked(show_phonetic)
            .on(|this, _, cx| this.toggle_show_phonetic(cx))
            .into(),
    );
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.phonetic-edit")
            .shortcut("⌥⇧↑")
            .on(|this, window, cx| this.open_phonetic_panel(window, cx))
            .into(),
    );
    // データ > フラッシュ フィル (⌃E), after 区切り位置.
    let mut flash: MenuEntry = MenuItem::tr("xlsx.flash-fill").icon("sort").shortcut("⌃E").on(|this, _, cx| this.flash_fill_column(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.remove-duplicates", flash) {
            Ok(()) => break,
            Err(back) => flash = back,
        }
    }
    let mut text_columns: MenuEntry = MenuItem::tr("xlsx.text-to-columns").icon("sort").on(|this, window, cx| this.open_text_columns_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.remove-duplicates", text_columns) {
            Ok(()) => break,
            Err(back) => text_columns = back,
        }
    }
    let mut get_data: MenuEntry = MenuItem::tr("xlsx.get-data").icon("cloud_download").on(|this, window, cx| this.open_import_text_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.text-to-columns", get_data) {
            Ok(()) => break,
            Err(back) => get_data = back,
        }
    }
    let mut import_text: MenuEntry = MenuItem::tr("xlsx.import-text").icon("sort").on(|this, window, cx| this.open_import_text_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.text-to-columns", import_text) {
            Ok(()) => break,
            Err(back) => import_text = back,
        }
    }
    let mut from_table: MenuEntry = MenuItem::tr("xlsx.query-from-table").icon("table_chart").on(|this, window, cx| this.open_query_from_selection(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.import-text", from_table) {
            Ok(()) => break,
            Err(back) => from_table = back,
        }
    }
    let mut query_blank: MenuEntry = MenuItem::tr("xlsx.query-blank").icon("note_add").on(|this, window, cx| this.open_blank_query(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.query-from-table", query_blank) {
            Ok(()) => break,
            Err(back) => query_blank = back,
        }
    }
    let mut queries: MenuEntry = MenuItem::tr("xlsx.queries").icon("link").on(|this, _window, cx| this.open_queries_panel(cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.query-from-table", queries) {
            Ok(()) => break,
            Err(back) => queries = back,
        }
    }
    let mut external_links: MenuEntry = MenuItem::tr("xlsx.external-links").icon("link").on(|this, window, cx| this.open_external_links_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.queries", external_links) {
            Ok(()) => break,
            Err(back) => external_links = back,
        }
    }
    let mut consolidate: MenuEntry = MenuItem::tr("xlsx.consolidate").icon("table_chart").on(|this, window, cx| this.open_consolidate_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.text-to-columns", consolidate) {
            Ok(()) => break,
            Err(back) => consolidate = back,
        }
    }
    menus.connect("xlsx.consolidate", |this, window, cx| this.open_consolidate_panel(window, cx));
    let mut subtotal: MenuEntry = MenuItem::tr("xlsx.subtotal").icon("functions").on(|this, window, cx| this.open_subtotal_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.consolidate", subtotal) {
            Ok(()) => break,
            Err(back) => subtotal = back,
        }
    }
    menus.connect("xlsx.subtotal", |this, window, cx| this.open_subtotal_panel(window, cx));
    let mut analysis: MenuEntry = MenuItem::tr("xlsx.data-analysis").icon("functions").on(|this, window, cx| this.open_analysis_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.subtotal", analysis) {
            Ok(()) => break,
            Err(back) => analysis = back,
        }
    }
    menus.connect("xlsx.data-analysis", |this, window, cx| this.open_analysis_panel(window, cx));
    let mut solver: MenuEntry = MenuItem::tr("xlsx.solver").icon("functions").on(|this, window, cx| this.open_solver_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.data-analysis", solver) {
            Ok(()) => break,
            Err(back) => solver = back,
        }
    }
    menus.connect("xlsx.solver", |this, window, cx| this.open_solver_panel(window, cx));
    let mut forecast: MenuEntry = MenuItem::tr("xlsx.forecast-sheet").icon("show_chart").on(|this, window, cx| this.open_forecast_panel(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.data-analysis", forecast) {
            Ok(()) => break,
            Err(back) => forecast = back,
        }
    }
    menus.connect("xlsx.forecast-sheet", |this, window, cx| this.open_forecast_panel(window, cx));
    let mut sort_down: MenuEntry = MenuItem::tr("xlsx.sort-descending").icon("sort").on(|this, _, cx| this.sort_by_selected_column(false, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.sort", sort_down) {
            Ok(()) => break,
            Err(back) => sort_down = back,
        }
    }
    let grid = editor.current_sheet().is_some_and(|(_, s)| s.grid_lines);
    if let Some(item) = menus.find_mut("xlsx.gridlines") {
        item.checked = grid;
    }
    menus.connect("xlsx.gridlines", |this, _, cx| {
        let Some((_, sheet)) = this.current_sheet() else { return };
        let (slide, shown) = (this.current_slide, !sheet.grid_lines);
        this.run_sheet(vec![Command::Sheet(SheetCommand::SetGridLines { slide, shown })], cx);
    });
    let headings = editor.current_sheet().map_or(true, |(_, s)| s.headings);
    if let Some(item) = menus.find_mut("xlsx.headings") {
        item.checked = headings;
    }
    menus.connect("xlsx.headings", |this, _, cx| {
        let Some((_, sheet)) = this.current_sheet() else { return };
        let (slide, shown) = (this.current_slide, !sheet.headings);
        this.run_sheet(vec![Command::Sheet(SheetCommand::SetHeadings { slide, shown })], cx);
    });
    let formula_bar = editor.sheet_state().map_or(true, |s| s.show_formula_bar);
    if let Some(item) = menus.find_mut("xlsx.formula-bar") {
        item.checked = formula_bar;
    }
    menus.connect("xlsx.formula-bar", |this, _, cx| {
        if let Some(state) = this.sheet_state_mut() {
            state.show_formula_bar = !state.show_formula_bar;
            cx.notify();
        }
    });
    let summary_items: Vec<MenuEntry> = [
        ("xlsx.status-summary-average", super::STATUS_SUMMARY_AVG),
        ("xlsx.status-summary-count", super::STATUS_SUMMARY_COUNT),
        ("xlsx.status-summary-num-count", super::STATUS_SUMMARY_NUM_COUNT),
        ("xlsx.status-summary-min", super::STATUS_SUMMARY_MIN),
        ("xlsx.status-summary-max", super::STATUS_SUMMARY_MAX),
        ("xlsx.status-summary-sum", super::STATUS_SUMMARY_SUM),
    ]
    .into_iter()
    .map(|(id, flag)| {
        MenuItem::tr(id)
            .checked(editor.status_summary_flag(flag))
            .on(move |this, _, cx| this.toggle_status_summary_flag(flag, cx))
            .into()
    })
    .collect();
    let mut status_summary_menu: MenuEntry = crate::shell::chrome::Submenu::tr("xlsx.status-bar-summary", "calculate", summary_items).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.formula-bar", status_summary_menu) {
            Ok(()) => break,
            Err(back) => status_summary_menu = back,
        }
    }
    let show_formulas = editor.current_sheet().is_some_and(|(_, s)| s.show_formulas);
    if let Some(item) = menus.find_mut("xlsx.show-formulas") {
        item.checked = show_formulas;
    }
    menus.connect("xlsx.show-formulas", |this, _, cx| {
        this.toggle_show_formulas(cx);
    });
    let show_zeros = editor.current_sheet().map_or(true, |(_, s)| s.show_zeros);
    if let Some(item) = menus.find_mut("xlsx.show-zeros") {
        item.checked = show_zeros;
    }
    menus.connect("xlsx.show-zeros", |this, _, cx| {
        this.toggle_show_zeros(cx);
    });
    menus.connect("xlsx.hide-ink", |this, _, cx| {
        this.toggle_hide_ink(cx);
    });
    let is_normal = editor.current_sheet().is_some_and(|(_, s)| !s.page_break_preview && !s.page_layout);
    if let Some(item) = menus.find_mut("xlsx.normal-view") {
        item.checked = is_normal;
    }
    menus.connect("xlsx.normal-view", |this, _, cx| {
        let Some((_, sheet)) = this.current_sheet() else { return };
        let slide = this.current_slide;
        let mut commands = Vec::new();
        if sheet.page_break_preview {
            commands.push(Command::Sheet(SheetCommand::SetPageBreakPreview { slide, shown: false }));
        }
        if sheet.page_layout {
            commands.push(Command::Sheet(SheetCommand::SetPageLayout { slide, shown: false }));
        }
        if !commands.is_empty() {
            this.run_sheet(commands, cx);
        }
    });
    let preview = editor.current_sheet().is_some_and(|(_, s)| s.page_break_preview);
    if let Some(item) = menus.find_mut("xlsx.page-break-preview") {
        item.checked = preview;
    }
    menus.connect("xlsx.page-break-preview", |this, _, cx| {
        let Some((_, sheet)) = this.current_sheet() else { return };
        let (slide, shown) = (this.current_slide, !sheet.page_break_preview);
        this.run_sheet(vec![Command::Sheet(SheetCommand::SetPageBreakPreview { slide, shown })], cx);
    });
    let layout = editor.current_sheet().is_some_and(|(_, s)| s.page_layout);
    if let Some(item) = menus.find_mut("xlsx.page-layout") {
        item.checked = layout;
    }
    menus.connect("xlsx.page-layout", |this, _, cx| {
        let Some((_, sheet)) = this.current_sheet() else { return };
        let (slide, shown) = (this.current_slide, !sheet.page_layout);
        this.run_sheet(vec![Command::Sheet(SheetCommand::SetPageLayout { slide, shown })], cx);
    });
    let is_page_layout = editor.current_sheet().is_some_and(|(_, s)| s.page_layout);
    let show_ruler = editor.current_sheet().map_or(true, |(_, s)| s.show_ruler);
    if let Some(item) = menus.find_mut("xlsx.ruler") {
        item.disabled = !is_page_layout;
        item.checked = is_page_layout && show_ruler;
    }
    menus.connect("xlsx.ruler", |this, _, cx| {
        let Some((_, sheet)) = this.current_sheet() else { return };
        if !sheet.page_layout { return };
        let (slide, shown) = (this.current_slide, !sheet.show_ruler);
        this.run_sheet(vec![Command::Sheet(SheetCommand::SetShowRuler { slide, shown })], cx);
    });
    menus.connect("xlsx.ruler-unit.cm", |this, _, cx| this.set_ruler_unit(crate::shell::editor::RulerUnit::Centimeters, cx));
    menus.connect("xlsx.ruler-unit.mm", |this, _, cx| this.set_ruler_unit(crate::shell::editor::RulerUnit::Millimeters, cx));
    menus.connect("xlsx.ruler-unit.inch", |this, _, cx| this.set_ruler_unit(crate::shell::editor::RulerUnit::Inches, cx));
    menus.connect("xlsx.ruler-unit.point", |this, _, cx| this.set_ruler_unit(crate::shell::editor::RulerUnit::Points, cx));
    menus.connect("xlsx.ruler-unit.pica", |this, _, cx| this.set_ruler_unit(crate::shell::editor::RulerUnit::Picas, cx));
    let has_cell_selection = editor.selection.is_none() && editor.selected_ids().is_empty() && editor.selected_range().is_some();
    if let Some(item) = menus.find_mut("xlsx.zoom-selection") {
        item.disabled = !has_cell_selection;
    }
    menus.connect("xlsx.zoom-selection", |this, window, cx| {
        this.zoom_to_selection(window, cx);
    });
    let has_tables = editor.presentation().has_tables();
    if let Some(item) = menus.find_mut("xlsx.custom-views") {
        item.disabled = has_tables;
    }
    menus.connect("xlsx.custom-views", |this, window, cx| {
        this.toggle_custom_views_panel(window, cx);
    });
    menus.connect("xlsx.new-window", |this, _window, cx| {
        this.new_window(cx);
    });
    menus.connect("xlsx.arrange-all", |this, window, cx| {
        this.toggle_arrange_windows_panel(window, cx);
    });
    if let Some(item) = menus.find_mut("xlsx.view-side-by-side") {
        item.checked = editor.side_by_side;
    }
    menus.connect("xlsx.view-side-by-side", |this, window, cx| {
        this.toggle_view_side_by_side(window, cx);
    });
    if let Some(item) = menus.find_mut("xlsx.sync-scrolling") {
        item.checked = editor.sync_scrolling;
        item.disabled = !editor.side_by_side;
    }
    menus.connect("xlsx.sync-scrolling", |this, _window, cx| {
        this.toggle_sync_scrolling(cx);
    });
    if let Some(item) = menus.find_mut("xlsx.reset-window-position") {
        item.disabled = !editor.side_by_side;
    }
    menus.connect("xlsx.reset-window-position", |this, window, cx| {
        this.reset_side_by_side_positions(window, cx);
    });
    let is_hidden = editor.presentation().is_workbook_window_hidden();
    let is_locked = editor.presentation().is_workbook_windows_locked();
    if let Some(item) = menus.find_mut("xlsx.hide-window") {
        item.disabled = is_hidden || is_locked;
    }
    menus.connect("xlsx.hide-window", |this, _window, cx| {
        this.hide_workbook_window(cx);
    });
    if let Some(item) = menus.find_mut("xlsx.unhide-window") {
        item.disabled = !is_hidden || is_locked;
    }
    menus.connect("xlsx.unhide-window", |this, window, cx| {
        this.toggle_unhide_window_panel(window, cx);
    });
    // ファイル > ダウンロード > CSV: the sheet shown, as CSV.
    let mut csv: MenuEntry = MenuItem::tr("xlsx.export-csv").icon("description").on(|this, window, cx| this.export_csv(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "file.export.svg", csv) {
            Ok(()) => break,
            Err(back) => csv = back,
        }
    }
    // ファイル > 書き出し > CSV (Shift_JIS)
    let mut csv_sjis: MenuEntry = MenuItem::tr("xlsx.export-csv-sjis").icon("description").on(|this, window, cx| this.export_csv_sjis(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.export-csv", csv_sjis) {
            Ok(()) => break,
            Err(back) => csv_sjis = back,
        }
    }
    menus.connect("xlsx.export-csv-sjis", |this, window, cx| this.export_csv_sjis(window, cx));

    // ファイル > 書き出し > テキスト (タブ区切り)（.tsv）
    let mut tsv: MenuEntry = MenuItem::tr("xlsx.export-tsv").icon("description").on(|this, window, cx| this.export_tsv(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.export-csv-sjis", tsv) {
            Ok(()) => break,
            Err(back) => tsv = back,
        }
    }
    menus.connect("xlsx.export-tsv", |this, window, cx| this.export_tsv(window, cx));

    // ファイル > 書き出し > OpenDocument スプレッドシート（.ods）
    let mut ods: MenuEntry = MenuItem::tr("xlsx.export-ods").icon("description").on(|this, window, cx| this.export_ods(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.export-tsv", ods) {
            Ok(()) => break,
            Err(back) => ods = back,
        }
    }
    menus.connect("xlsx.export-ods", |this, window, cx| this.export_ods(window, cx));

    // ファイル > 書き出し > Excel 97-2003 ブック（.xls）
    let mut xls: MenuEntry = MenuItem::tr("xlsx.export-xls").icon("description").on(|this, window, cx| this.export_xls(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.export-ods", xls) {
            Ok(()) => break,
            Err(back) => xls = back,
        }
    }
    menus.connect("xlsx.export-xls", |this, window, cx| this.export_xls(window, cx));

    // ファイル > 書き出し > Excel テンプレート（.xltx）
    let mut xltx: MenuEntry = MenuItem::tr("xlsx.export-xltx").icon("description").on(|this, window, cx| this.export_xltx(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.export-ods", xltx) {
            Ok(()) => break,
            Err(back) => xltx = back,
        }
    }
    menus.connect("xlsx.export-xltx", |this, window, cx| this.export_xltx(window, cx));
    // ファイル > 書き出し > マクロなしで書き出し…
    let mut no_macros: MenuEntry = MenuItem::tr("xlsx.export-without-macros").icon("description").on(|this, window, cx| this.export_without_macros(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.export-xltx", no_macros) {
            Ok(()) => break,
            Err(back) => no_macros = back,
        }
    }
    menus.connect("xlsx.export-without-macros", |this, window, cx| this.export_without_macros(window, cx));

    // ファイル > 新規 > テンプレートから新規作成…
    let mut tmpl_entry: MenuEntry = MenuItem::tr("xlsx.new-from-template")
        .icon("dashboard_customize")
        .on(|this, window, cx| this.new_from_template(window, cx))
        .into();
    for (_, entries) in menus.menus.iter_mut() {
        match push_into(entries, "file.new", tmpl_entry) {
            Ok(()) => break,
            Err(back) => tmpl_entry = back,
        }
    }
    menus.connect("xlsx.new-from-template", |this, window, cx| this.new_from_template(window, cx));

    // ファイル > 未保存のブックの回復…
    let mut recover_entry: MenuEntry = MenuItem::tr("xlsx.recover-unsaved")
        .icon("restore_page")
        .on(|this, _window, cx| { this.wb_mut().close_settings(); this.open_recover_unsaved(cx) })
        .into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "file.open", recover_entry) {
            Ok(()) => break,
            Err(back) => recover_entry = back,
        }
    }
    // ファイル > 開いて修復… (XLSX-FILE-020)
    let mut repair_file_entry: MenuEntry = MenuItem::tr("xlsx.open-and-repair")
        .icon("cleaning_services")
        .disabled(super::repair::damaged_workbook().is_none())
        .on(|this, window, cx| this.open_xlsx_repair_damaged(window, cx))
        .into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "xlsx.recover-unsaved", repair_file_entry) {
            Ok(()) => break,
            Err(back) => repair_file_entry = back,
        }
    }
    // セルの挿入・削除: down (up) from the menu item, and the other way after it.
    menus.connect("xlsx.insert-cells", |this, _, cx| this.shift_cells(true, true, cx));
    menus.connect("xlsx.delete-cells", |this, _, cx| this.shift_cells(true, false, cx));
    for (after, entry) in [
        ("xlsx.insert-cells", MenuItem::tr("xlsx.insert-cells-right").icon("add").on(|this, _, cx| this.shift_cells(false, true, cx))),
        ("xlsx.delete-cells", MenuItem::tr("xlsx.delete-cells-left").icon("delete").on(|this, _, cx| this.shift_cells(false, false, cx))),
    ] {
        let mut entry: MenuEntry = entry.into();
        for (_, entries) in menus.menus.iter_mut() {
            match insert_after(entries, after, entry) {
                Ok(()) => break,
                Err(back) => entry = back,
            }
        }
    }
    menus.connect("edit.find-replace", |this, window, cx| this.open_sheet_find(window, cx));
    // データ > グループ化 and the rest of the outline.
    let data = menus.items_mut(crate::shell::chrome::MenuId::Data);
    data.push(MenuEntry::Separator);
    data.push(MenuItem::tr("xlsx.group").on(|this, _, cx| this.group_lines(true, cx)).into());
    data.push(MenuItem::tr("xlsx.ungroup").on(|this, _, cx| this.group_lines(false, cx)).into());
    data.push(MenuItem::tr("xlsx.collapse").on(|this, _, cx| this.collapse_group(true, cx)).into());
    data.push(MenuItem::tr("xlsx.expand").on(|this, _, cx| this.collapse_group(false, cx)).into());
    data.push(MenuEntry::Separator);
    data.extend(super::pivot::menu_items());
    data.push(MenuEntry::Separator);
    data.push(MenuItem::tr("xlsx.goal-seek").on(|this, window, cx| this.open_goal_seek(window, cx)).into());
    data.push(MenuItem::tr("xlsx.data-table").on(|this, window, cx| this.open_data_table(window, cx)).into());
    data.push(MenuItem::tr("xlsx.scenario-manager").on(|this, window, cx| this.open_scenario_manager(window, cx)).into());
    let selected_version = editor.presentation().workbook.as_deref().and_then(|w| w.compatibility_version).unwrap_or(1);
    let versions = ibis_doc_model::CalculationCompatibilityVersion::ALL.into_iter().map(|version| {
        let (id, label) = match version {
            ibis_doc_model::CalculationCompatibilityVersion::V1 => ("xlsx.compatibility-v1", crate::shell::i18n::text("_xlsx.version-1")),
            ibis_doc_model::CalculationCompatibilityVersion::V2 => ("xlsx.compatibility-v2", crate::shell::i18n::text("_xlsx.version-2")),
        };
        MenuItem::new(id, label).checked(selected_version == version.number()).disabled(selected_version == version.number()).on(move |this, _, cx| {
            this.run_sheet(vec![Command::Sheet(SheetCommand::SetCompatibilityVersion { version })], cx);
        }).into()
    }).collect();
    data.push(crate::shell::chrome::Submenu::tr("xlsx.compatibility-version", "functions", versions).into());
    let calc_mode = editor.presentation().workbook.as_deref().map(|w| w.calc_mode).unwrap_or_default();
    let modes = [
        (ibis_doc_model::CalcMode::Auto, "xlsx.calc-auto"),
        (ibis_doc_model::CalcMode::AutoNoTable, "xlsx.calc-auto-no-table"),
        (ibis_doc_model::CalcMode::Manual, "xlsx.calc-manual"),
    ].into_iter().map(|(mode, key)| {
        MenuItem::tr(key).checked(calc_mode == mode).disabled(calc_mode == mode).on(move |this, _, cx| {
            this.run_sheet(vec![Command::Sheet(SheetCommand::SetCalcMode { mode })], cx);
        }).into()
    }).collect();
    data.push(crate::shell::chrome::Submenu::tr("xlsx.calc-options", "tune", modes).into());
    data.push(MenuItem::tr("xlsx.insert-function").icon("functions").shortcut("⇧F3").on(|this, window, cx| {
        this.insert_function_action(window, cx);
    }).into());

    let fn_item = |id: &'static str, name: &'static str| -> MenuEntry {
        MenuItem::tr(id).on(move |this, window, cx| {
            this.insert_function_named(name, window, cx);
        }).into()
    };
    let recent_fn_items: Vec<MenuEntry> = vec![
        fn_item("xlsx.fn.sum", "SUM"),
        fn_item("xlsx.fn.average", "AVERAGE"),
        fn_item("xlsx.fn.if", "IF"),
        fn_item("xlsx.fn.count", "COUNT"),
        fn_item("xlsx.fn.max", "MAX"),
        fn_item("xlsx.fn.min", "MIN"),
        fn_item("xlsx.fn.vlookup", "VLOOKUP"),
        fn_item("xlsx.fn.xlookup", "XLOOKUP"),
    ];
    let financial_fn_items: Vec<MenuEntry> = vec![
        fn_item("xlsx.fn.pmt", "PMT"),
        fn_item("xlsx.fn.fv", "FV"),
        fn_item("xlsx.fn.pv", "PV"),
        fn_item("xlsx.fn.npv", "NPV"),
        fn_item("xlsx.fn.irr", "IRR"),
        fn_item("xlsx.fn.rate", "RATE"),
        fn_item("xlsx.fn.nper", "NPER"),
    ];
    let text_fn_items: Vec<MenuEntry> = vec![
        fn_item("xlsx.fn.concat", "CONCAT"),
        fn_item("xlsx.fn.text", "TEXT"),
        fn_item("xlsx.fn.left", "LEFT"),
        fn_item("xlsx.fn.right", "RIGHT"),
        fn_item("xlsx.fn.mid", "MID"),
        fn_item("xlsx.fn.len", "LEN"),
        fn_item("xlsx.fn.trim", "TRIM"),
        fn_item("xlsx.fn.substitute", "SUBSTITUTE"),
    ];
    let lookup_fn_items: Vec<MenuEntry> = vec![
        fn_item("xlsx.fn.xlookup", "XLOOKUP"),
        fn_item("xlsx.fn.vlookup", "VLOOKUP"),
        fn_item("xlsx.fn.hlookup", "HLOOKUP"),
        fn_item("xlsx.fn.index", "INDEX"),
        fn_item("xlsx.fn.match", "MATCH"),
        fn_item("xlsx.fn.xmatch", "XMATCH"),
        fn_item("xlsx.fn.filter", "FILTER"),
        fn_item("xlsx.fn.unique", "UNIQUE"),
        fn_item("xlsx.fn.sort", "SORT"),
    ];
    let math_fn_items: Vec<MenuEntry> = vec![
        fn_item("xlsx.fn.sumif", "SUMIF"),
        fn_item("xlsx.fn.sumifs", "SUMIFS"),
        fn_item("xlsx.fn.round", "ROUND"),
        fn_item("xlsx.fn.roundup", "ROUNDUP"),
        fn_item("xlsx.fn.rounddown", "ROUNDDOWN"),
        fn_item("xlsx.fn.abs", "ABS"),
        fn_item("xlsx.fn.product", "PRODUCT"),
        fn_item("xlsx.fn.mod", "MOD"),
    ];
    let logical_fn_items: Vec<MenuEntry> = vec![
        fn_item("xlsx.fn.if", "IF"),
        fn_item("xlsx.fn.ifs", "IFS"),
        fn_item("xlsx.fn.iferror", "IFERROR"),
        fn_item("xlsx.fn.and", "AND"),
        fn_item("xlsx.fn.or", "OR"),
        fn_item("xlsx.fn.not", "NOT"),
    ];
    let other_fn_items: Vec<MenuEntry> = vec![
        fn_item("xlsx.fn.countif", "COUNTIF"),
        fn_item("xlsx.fn.countifs", "COUNTIFS"),
        fn_item("xlsx.fn.averageif", "AVERAGEIF"),
        fn_item("xlsx.fn.median", "MEDIAN"),
        fn_item("xlsx.fn.stdev-s", "STDEV.S"),
        fn_item("xlsx.fn.isblank", "ISBLANK"),
        fn_item("xlsx.fn.iserror", "ISERROR"),
    ];

    let defined_names: Vec<String> = editor.presentation().workbook.as_deref().map(|w| {
        w.names.iter().map(|n| n.name.clone()).collect()
    }).unwrap_or_default();
    let mut use_name_items: Vec<MenuEntry> = defined_names.into_iter().take(15).map(|n| {
        let name_clone = n.clone();
        let item_id: &'static str = Box::leak(format!("xlsx.use-name-{n}").into_boxed_str());
        MenuItem::new(item_id, n).on(move |this, window, cx| {
            this.use_name_in_formula(&name_clone, window, cx);
        }).into()
    }).collect();
    use_name_items.push(MenuItem::tr("xlsx.use-in-formula-paste").icon("content_paste").on(|this, window, cx| {
        this.open_names(window, cx);
    }).into());

    let create_names_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.create-names-top-row").icon("table_rows").on(|this, _, cx| {
            this.create_names_from_selection(true, false, cx);
        }).into(),
        MenuItem::tr("xlsx.create-names-left-col").icon("view_column").on(|this, _, cx| {
            this.create_names_from_selection(false, true, cx);
        }).into(),
        MenuItem::tr("xlsx.create-names-both").icon("grid_view").on(|this, _, cx| {
            this.create_names_from_selection(true, true, cx);
        }).into(),
    ];

    data.push(crate::shell::chrome::Submenu::tr("xlsx.recent-functions", "schedule", recent_fn_items).into());
    data.push(crate::shell::chrome::Submenu::tr("xlsx.financial-functions", "payments", financial_fn_items).into());
    data.push(crate::shell::chrome::Submenu::tr("xlsx.logical-functions", "rule", logical_fn_items).into());
    data.push(crate::shell::chrome::Submenu::tr("xlsx.text-functions", "title", text_fn_items).into());
    data.push(crate::shell::chrome::Submenu::tr("xlsx.lookup-functions", "search", lookup_fn_items).into());
    data.push(crate::shell::chrome::Submenu::tr("xlsx.math-functions", "calculate", math_fn_items).into());
    data.push(crate::shell::chrome::Submenu::tr("xlsx.other-functions", "functions", other_fn_items).into());
    data.push(MenuItem::tr("xlsx.name-manager").icon("label").on(|this, window, cx| {
        if let Some(s) = this.sheet_state_mut() {
            s.validation = None;
            s.find = None;
        }
        this.open_names(window, cx);
    }).into());
    data.push(MenuItem::tr("xlsx.define-name").icon("new_label").on(|this, window, cx| {
        if let Some(s) = this.sheet_state_mut() {
            s.validation = None;
            s.find = None;
        }
        this.open_names(window, cx);
    }).into());
    data.push(crate::shell::chrome::Submenu::tr("xlsx.use-in-formula", "label", use_name_items).into());
    data.push(crate::shell::chrome::Submenu::tr("xlsx.create-from-selection", "new_label", create_names_items).into());
    data.push(MenuItem::tr("xlsx.calculate-now").shortcut("F9").on(|this, _, cx| {
        this.run_sheet(vec![Command::Sheet(SheetCommand::CalculateNow)], cx);
    }).into());
    data.push(MenuItem::tr("xlsx.calculate-sheet").shortcut("⇧F9").on(|this, _, cx| {
        let slide = this.current_slide;
        this.run_sheet(vec![Command::Sheet(SheetCommand::CalculateSheet { slide })], cx);
    }).into());
    let full_precision = editor.presentation().workbook.as_deref().map(|w| w.full_precision).unwrap_or(true);
    let precision_as_displayed = !full_precision;
    data.push(MenuItem::tr("xlsx.precision-as-displayed").checked(precision_as_displayed).on(move |this, _, cx| {
        this.run_sheet(vec![Command::Sheet(SheetCommand::SetFullPrecision { full_precision: precision_as_displayed })], cx);
    }).into());
    let iterate = editor.presentation().workbook.as_deref().map(|w| w.iterate).unwrap_or(false);
    data.push(MenuItem::tr("xlsx.iterative-calculation").checked(iterate).on(move |this, _, cx| {
        this.run_sheet(vec![Command::Sheet(SheetCommand::SetIterativeCalculation {
            iterate: !iterate,
            max_iterations: 100,
            max_change: 0.001.into(),
        })], cx);
    }).into());
    data.push(MenuItem::tr("xlsx.trace-precedents").icon("arrow_forward").on(|this, _, cx| this.trace_precedents(cx)).into());
    data.push(MenuItem::tr("xlsx.trace-dependents").icon("arrow_back").on(|this, _, cx| this.trace_dependents(cx)).into());
    let remove_arrow_items = vec![
        MenuItem::tr("xlsx.remove-all-arrows").on(|this, _, cx| this.remove_all_arrows(cx)).into(),
        MenuItem::tr("xlsx.remove-precedent-arrows").on(|this, _, cx| this.remove_precedent_arrows(cx)).into(),
        MenuItem::tr("xlsx.remove-dependent-arrows").on(|this, _, cx| this.remove_dependent_arrows(cx)).into(),
    ];
    data.push(crate::shell::chrome::Submenu::tr("xlsx.remove-arrows", "clear", remove_arrow_items).into());
    data.push(MenuItem::tr("xlsx.evaluate-formula").icon("functions").on(|this, window, cx| this.open_evaluate_formula(window, cx)).into());
    data.push(MenuItem::tr("xlsx.watch-window").icon("visibility").on(|this, window, cx| this.open_watch_window(window, cx)).into());
    data.push(MenuItem::tr("xlsx.show-formulas").icon("functions").shortcut("⌘`").checked(show_formulas).on(|this, _, cx| this.toggle_show_formulas(cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(MenuItem::tr("xlsx.slicer-settings").icon("tune").on(|this, window, cx| this.open_slicer_settings_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(MenuItem::tr("xlsx.control-properties").icon("tune").on(|this, window, cx| this.open_control_properties_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(MenuItem::tr("xlsx.design-mode").icon("design_services").on(|this, _, cx| this.toggle_design_mode(cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(MenuItem::tr("xlsx.view-code").icon("code").on(|this, _, cx| this.view_code(cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(MenuItem::tr("xlsx.xml-source").icon("data_object").on(|this, window, cx| this.open_xml_source(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(MenuItem::tr("xlsx.xml-map-properties").icon("tune").on(|this, window, cx| this.open_xml_map_properties(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Insert).push(MenuItem::tr("xlsx.slicer-insert").icon("filter_alt").on(|this, window, cx| this.open_slicer_insert_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Insert).push(MenuItem::tr("xlsx.timeline-insert").icon("schedule").on(|this, window, cx| this.open_timeline_insert_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Insert).push(crate::shell::chrome::Submenu::new("xlsx.insert-control", "checklist", crate::shell::i18n::text("xlsx.insert-control"), super::control_insert::control_insert_items()).into());
    menus.items_mut(crate::shell::chrome::MenuId::Insert).push(MenuItem::tr("xlsx.ink").icon("draw").on(|this, window, cx| this.open_ink_panel(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Insert).push(MenuItem::tr("xlsx.pivot-insert").icon("table_chart").on(|this, window, cx| this.open_create_pivot(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Insert).push(MenuItem::tr("xlsx.recommended-pivots").icon("insert_chart").on(|this, window, cx| this.open_recommended_pivots(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Insert).push(MenuItem::tr("xlsx.pivot-chart-insert").icon("insert_chart").on(|this, window, cx| this.open_insert_pivot_chart(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Insert).push(MenuItem::tr("xlsx.3d-model").icon("shapes").on(|this, window, cx| this.pick_sheet_3d_model(window, cx)).into());
    menus.items_mut(crate::shell::chrome::MenuId::Insert).push(MenuItem::tr("xlsx.signature-line").icon("draw").on(|this, _, cx| { this.insert_sheet_signature_line("", "", cx); }).into());
    let spark_style_items: Vec<MenuEntry> = [
        ("xlsx.sparkline-style-1", 0),
        ("xlsx.sparkline-style-2", 1),
        ("xlsx.sparkline-style-3", 2),
        ("xlsx.sparkline-style-4", 3),
        ("xlsx.sparkline-style-5", 4),
        ("xlsx.sparkline-style-6", 5),
    ]
    .into_iter()
    .map(|(id, idx)| {
        MenuItem::tr(id)
            .on(move |this, _, cx| {
                this.set_selected_sparklines_style(idx, cx);
            })
            .into()
    })
    .collect();
    const SPARK_COLOR_SWATCHES: [(&str, &str, u32); 8] = [
        ("xlsx.sparkline-color-blue", "xlsx.sparkline-color-blue", 0x376092),
        ("xlsx.sparkline-color-red", "xlsx.sparkline-color-red", 0xC00000),
        ("xlsx.sparkline-color-green", "xlsx.sparkline-color-green", 0x70AD47),
        ("xlsx.sparkline-color-purple", "xlsx.sparkline-color-purple", 0x7030A0),
        ("xlsx.sparkline-color-cyan", "xlsx.sparkline-color-cyan", 0x4BACC6),
        ("xlsx.sparkline-color-orange", "xlsx.sparkline-color-orange", 0xED7D31),
        ("xlsx.sparkline-color-gray", "xlsx.sparkline-color-gray", 0x7F7F7F),
        ("xlsx.sparkline-color-black", "xlsx.sparkline-color-black", 0x000000),
    ];
    let spark_color_items: Vec<MenuEntry> = palette_menu_with_ids(
        editor,
        None,
        &SPARK_COLOR_SWATCHES,
        false,
        std::rc::Rc::new(|this, c, _, cx| {
            if let Some(rgb) = c {
                this.set_selected_sparklines_series_color(Some(rgb), cx);
            }
        }),
    );
    let spark_hidden_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.sparkline-display-hidden-on").on(|this, _, cx| this.set_selected_sparklines_display_hidden(true, cx)).into(),
        MenuItem::tr("xlsx.sparkline-display-hidden-off").on(|this, _, cx| this.set_selected_sparklines_display_hidden(false, cx)).into(),
    ];
    let spark_axis_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.sparkline-date-axis-on").on(|this, _, cx| this.set_selected_sparklines_date_axis(true, cx)).into(),
        MenuItem::tr("xlsx.sparkline-date-axis-off").on(|this, _, cx| this.set_selected_sparklines_date_axis(false, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.sparkline-show-axis").on(|this, _, cx| this.toggle_selected_sparklines_display_x_axis(cx)).into(),
        MenuItem::tr("xlsx.sparkline-right-to-left").on(|this, _, cx| this.toggle_selected_sparklines_right_to_left(cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.sparkline-min-individual").on(|this, _, cx| this.set_selected_sparklines_min_axis(ibis_doc_model::SparklineAxis::Individual, cx)).into(),
        MenuItem::tr("xlsx.sparkline-min-group").on(|this, _, cx| this.set_selected_sparklines_min_axis(ibis_doc_model::SparklineAxis::Group, cx)).into(),
        MenuItem::tr("xlsx.sparkline-min-custom").on(|this, window, cx| this.open_sparkline_panel(None, window, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.sparkline-max-individual").on(|this, _, cx| this.set_selected_sparklines_max_axis(ibis_doc_model::SparklineAxis::Individual, cx)).into(),
        MenuItem::tr("xlsx.sparkline-max-group").on(|this, _, cx| this.set_selected_sparklines_max_axis(ibis_doc_model::SparklineAxis::Group, cx)).into(),
        MenuItem::tr("xlsx.sparkline-max-custom").on(|this, window, cx| this.open_sparkline_panel(None, window, cx)).into(),
    ];
    use crate::xlsx::sparkline_dialog::SparklinePointKind as SPK;
    let spark_points_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.sparkline-high-point").on(|this, _, cx| this.toggle_selected_sparklines_point(SPK::High, cx)).into(),
        MenuItem::tr("xlsx.sparkline-low-point").on(|this, _, cx| this.toggle_selected_sparklines_point(SPK::Low, cx)).into(),
        MenuItem::tr("xlsx.sparkline-negative-points").on(|this, _, cx| this.toggle_selected_sparklines_point(SPK::Negative, cx)).into(),
        MenuItem::tr("xlsx.sparkline-first-point").on(|this, _, cx| this.toggle_selected_sparklines_point(SPK::First, cx)).into(),
        MenuItem::tr("xlsx.sparkline-last-point").on(|this, _, cx| this.toggle_selected_sparklines_point(SPK::Last, cx)).into(),
        MenuItem::tr("xlsx.sparkline-markers").on(|this, _, cx| this.toggle_selected_sparklines_point(SPK::Markers, cx)).into(),
    ];
    const MARKER_COLOR_MENUS: [(
        &str,
        &str,
        SPK,
        [(&str, &str, u32); 8],
    ); 6] = [
        (
            "xlsx.sparkline-marker-color-negative",
            "xlsx.sparkline-negative-points",
            SPK::Negative,
            [
                ("xlsx.sparkline-mc-negative-blue", "xlsx.sparkline-color-blue", 0x376092),
                ("xlsx.sparkline-mc-negative-red", "xlsx.sparkline-color-red", 0xC00000),
                ("xlsx.sparkline-mc-negative-green", "xlsx.sparkline-color-green", 0x70AD47),
                ("xlsx.sparkline-mc-negative-purple", "xlsx.sparkline-color-purple", 0x7030A0),
                ("xlsx.sparkline-mc-negative-cyan", "xlsx.sparkline-color-cyan", 0x4BACC6),
                ("xlsx.sparkline-mc-negative-orange", "xlsx.sparkline-color-orange", 0xED7D31),
                ("xlsx.sparkline-mc-negative-gray", "xlsx.sparkline-color-gray", 0x7F7F7F),
                ("xlsx.sparkline-mc-negative-black", "xlsx.sparkline-color-black", 0x000000),
            ],
        ),
        (
            "xlsx.sparkline-marker-color-markers",
            "xlsx.sparkline-markers",
            SPK::Markers,
            [
                ("xlsx.sparkline-mc-markers-blue", "xlsx.sparkline-color-blue", 0x376092),
                ("xlsx.sparkline-mc-markers-red", "xlsx.sparkline-color-red", 0xC00000),
                ("xlsx.sparkline-mc-markers-green", "xlsx.sparkline-color-green", 0x70AD47),
                ("xlsx.sparkline-mc-markers-purple", "xlsx.sparkline-color-purple", 0x7030A0),
                ("xlsx.sparkline-mc-markers-cyan", "xlsx.sparkline-color-cyan", 0x4BACC6),
                ("xlsx.sparkline-mc-markers-orange", "xlsx.sparkline-color-orange", 0xED7D31),
                ("xlsx.sparkline-mc-markers-gray", "xlsx.sparkline-color-gray", 0x7F7F7F),
                ("xlsx.sparkline-mc-markers-black", "xlsx.sparkline-color-black", 0x000000),
            ],
        ),
        (
            "xlsx.sparkline-marker-color-high",
            "xlsx.sparkline-high-point",
            SPK::High,
            [
                ("xlsx.sparkline-mc-high-blue", "xlsx.sparkline-color-blue", 0x376092),
                ("xlsx.sparkline-mc-high-red", "xlsx.sparkline-color-red", 0xC00000),
                ("xlsx.sparkline-mc-high-green", "xlsx.sparkline-color-green", 0x70AD47),
                ("xlsx.sparkline-mc-high-purple", "xlsx.sparkline-color-purple", 0x7030A0),
                ("xlsx.sparkline-mc-high-cyan", "xlsx.sparkline-color-cyan", 0x4BACC6),
                ("xlsx.sparkline-mc-high-orange", "xlsx.sparkline-color-orange", 0xED7D31),
                ("xlsx.sparkline-mc-high-gray", "xlsx.sparkline-color-gray", 0x7F7F7F),
                ("xlsx.sparkline-mc-high-black", "xlsx.sparkline-color-black", 0x000000),
            ],
        ),
        (
            "xlsx.sparkline-marker-color-low",
            "xlsx.sparkline-low-point",
            SPK::Low,
            [
                ("xlsx.sparkline-mc-low-blue", "xlsx.sparkline-color-blue", 0x376092),
                ("xlsx.sparkline-mc-low-red", "xlsx.sparkline-color-red", 0xC00000),
                ("xlsx.sparkline-mc-low-green", "xlsx.sparkline-color-green", 0x70AD47),
                ("xlsx.sparkline-mc-low-purple", "xlsx.sparkline-color-purple", 0x7030A0),
                ("xlsx.sparkline-mc-low-cyan", "xlsx.sparkline-color-cyan", 0x4BACC6),
                ("xlsx.sparkline-mc-low-orange", "xlsx.sparkline-color-orange", 0xED7D31),
                ("xlsx.sparkline-mc-low-gray", "xlsx.sparkline-color-gray", 0x7F7F7F),
                ("xlsx.sparkline-mc-low-black", "xlsx.sparkline-color-black", 0x000000),
            ],
        ),
        (
            "xlsx.sparkline-marker-color-first",
            "xlsx.sparkline-first-point",
            SPK::First,
            [
                ("xlsx.sparkline-mc-first-blue", "xlsx.sparkline-color-blue", 0x376092),
                ("xlsx.sparkline-mc-first-red", "xlsx.sparkline-color-red", 0xC00000),
                ("xlsx.sparkline-mc-first-green", "xlsx.sparkline-color-green", 0x70AD47),
                ("xlsx.sparkline-mc-first-purple", "xlsx.sparkline-color-purple", 0x7030A0),
                ("xlsx.sparkline-mc-first-cyan", "xlsx.sparkline-color-cyan", 0x4BACC6),
                ("xlsx.sparkline-mc-first-orange", "xlsx.sparkline-color-orange", 0xED7D31),
                ("xlsx.sparkline-mc-first-gray", "xlsx.sparkline-color-gray", 0x7F7F7F),
                ("xlsx.sparkline-mc-first-black", "xlsx.sparkline-color-black", 0x000000),
            ],
        ),
        (
            "xlsx.sparkline-marker-color-last",
            "xlsx.sparkline-last-point",
            SPK::Last,
            [
                ("xlsx.sparkline-mc-last-blue", "xlsx.sparkline-color-blue", 0x376092),
                ("xlsx.sparkline-mc-last-red", "xlsx.sparkline-color-red", 0xC00000),
                ("xlsx.sparkline-mc-last-green", "xlsx.sparkline-color-green", 0x70AD47),
                ("xlsx.sparkline-mc-last-purple", "xlsx.sparkline-color-purple", 0x7030A0),
                ("xlsx.sparkline-mc-last-cyan", "xlsx.sparkline-color-cyan", 0x4BACC6),
                ("xlsx.sparkline-mc-last-orange", "xlsx.sparkline-color-orange", 0xED7D31),
                ("xlsx.sparkline-mc-last-gray", "xlsx.sparkline-color-gray", 0x7F7F7F),
                ("xlsx.sparkline-mc-last-black", "xlsx.sparkline-color-black", 0x000000),
            ],
        ),
    ];
    let spark_marker_color_items: Vec<MenuEntry> = MARKER_COLOR_MENUS
        .into_iter()
        .map(|(sub_id, label_key, pt, palette)| {
            let items: Vec<MenuEntry> = palette_menu_with_ids(
                editor,
                None,
                &palette,
                false,
                std::rc::Rc::new(move |this, c, _, cx| {
                    if let Some(rgb) = c {
                        this.set_selected_sparklines_point_color(pt, Some(rgb), cx);
                    }
                }),
            );
            crate::shell::chrome::Submenu::new(sub_id, "palette", label_key, items).into()
        })
        .collect();
    let spark_weight_items: Vec<MenuEntry> = vec![
        MenuItem::new("xlsx.sparkline-weight-75", "0.75 pt").on(|this, _, cx| this.set_selected_sparklines_line_weight(75, cx)).into(),
        MenuItem::new("xlsx.sparkline-weight-100", "1.0 pt").on(|this, _, cx| this.set_selected_sparklines_line_weight(100, cx)).into(),
        MenuItem::new("xlsx.sparkline-weight-150", "1.5 pt").on(|this, _, cx| this.set_selected_sparklines_line_weight(150, cx)).into(),
        MenuItem::new("xlsx.sparkline-weight-225", "2.25 pt").on(|this, _, cx| this.set_selected_sparklines_line_weight(225, cx)).into(),
    ];
    let spark_empty_items: Vec<MenuEntry> = vec![
        MenuItem::new("xlsx.sparkline-empty-gap", "_xlsx.gaps").on(|this, _, cx| this.set_selected_sparklines_empty(ibis_doc_model::SparklineEmpty::Gap, cx)).into(),
        MenuItem::new("xlsx.sparkline-empty-zero", "_xlsx.zeros").on(|this, _, cx| this.set_selected_sparklines_empty(ibis_doc_model::SparklineEmpty::Zero, cx)).into(),
        MenuItem::new("xlsx.sparkline-empty-span", "_xlsx.connect-data-points-with-line").on(|this, _, cx| this.set_selected_sparklines_empty(ibis_doc_model::SparklineEmpty::Span, cx)).into(),
    ];
    let spark_items = vec![
        MenuItem::tr("xlsx.sparkline-line").on(|this, window, cx| this.open_sparkline_panel(Some(ibis_doc_model::SparklineKind::Line), window, cx)).into(),
        MenuItem::tr("xlsx.sparkline-column").on(|this, window, cx| this.open_sparkline_panel(Some(ibis_doc_model::SparklineKind::Column), window, cx)).into(),
        MenuItem::tr("xlsx.sparkline-winloss").on(|this, window, cx| this.open_sparkline_panel(Some(ibis_doc_model::SparklineKind::WinLoss), window, cx)).into(),
        MenuEntry::Separator,
        crate::shell::chrome::Submenu::tr("xlsx.sparkline-styles", "palette", spark_style_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.sparkline-colors", "format_color_text", spark_color_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.sparkline-marker-colors", "palette", spark_marker_color_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.sparkline-weight", "line_weight", spark_weight_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.sparkline-points", "scatter_plot", spark_points_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.sparkline-empty-cells", "space_bar", spark_empty_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.sparkline-display-hidden", "visibility", spark_hidden_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.sparkline-date-axis-menu", "timeline", spark_axis_items).into(),
        MenuEntry::Separator,
        MenuItem::new("xlsx.sparkline-group", crate::shell::i18n::text("xlsx.group")).on(|this, _, cx| this.group_selected_sparklines(cx)).into(),
        MenuItem::new("xlsx.sparkline-ungroup", crate::shell::i18n::text("xlsx.ungroup")).on(|this, _, cx| this.ungroup_selected_sparklines(cx)).into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Insert).push(crate::shell::chrome::Submenu::tr("xlsx.sparklines", "insert_chart", spark_items).into());
    let shape_entries: Vec<MenuEntry> = super::shape_edit::SHEET_SHAPES
        .iter()
        .map(|&(preset, id, _label, icon)| {
            MenuItem::tr(id)
                .icon(icon)
                .on(move |this, _, cx| {
                    this.insert_sheet_shape(preset, cx);
                })
                .into()
        })
        .chain(std::iter::once(MenuEntry::Separator))
        .chain([
            MenuItem::tr("xlsx.shape.line")
                .icon("horizontal_rule")
                .on(|this, _, cx| {
                    this.insert_sheet_line(false, cx);
                })
                .into(),
            MenuItem::tr("xlsx.shape.line-arrow")
                .icon("arrow_right_alt")
                .on(|this, _, cx| {
                    this.insert_sheet_line(true, cx);
                })
                .into(),
        ])
        .collect();

    let insert = menus.items_mut(crate::shell::chrome::MenuId::Insert);
    insert.push(MenuItem::tr("xlsx.insert-function").icon("functions").shortcut("⇧F3").on(|this, window, cx| { this.insert_function_action(window, cx); }).into());
    let picture_subitems = vec![
        MenuItem::tr("xlsx.insert-picture-in-cell").icon("table_rows").on(|this, window, cx| { this.pick_sheet_pictures_in_cells(window, cx); }).into(),
        MenuItem::tr("xlsx.insert-picture-over-cells").icon("image").on(|this, window, cx| { this.pick_sheet_picture(window, cx); }).into(),
        MenuItem::tr("xlsx.camera-tool").icon("photo_camera").shortcut("⌥⇧⌘X").on(|this, _, cx| { this.insert_camera_picture(cx); }).into(),
    ];
    let screenshot_subitems = vec![
        MenuItem::tr("xlsx.screenshot.available-windows").icon("fit_screen").on(|this, _, cx| { this.insert_sheet_screenshot_window(cx); }).into(),
        MenuItem::tr("xlsx.screenshot.clipping").icon("crop").on(|this, _, cx| { this.insert_sheet_screenshot_clipping(cx); }).into(),
        MenuItem::tr("xlsx.screenshot.clipboard").icon("content_paste").on(|this, _, cx| { this.insert_sheet_screenshot_from_clipboard(cx); }).into(),
    ];
    let screenshot_menu = crate::shell::chrome::Submenu::tr("xlsx.screenshot", "fit_screen", screenshot_subitems);
    insert.push(MenuItem::tr("xlsx.insert.wordart").icon("text_fields").on(|this, _, cx| { this.insert_sheet_wordart(0, None, cx); }).into());
    insert.push(crate::shell::chrome::Submenu::tr("xlsx.insert.wordart-styles", "text_fields", wordart_insert_items()).into());
    let smartart_items = crate::pptx::chrome::smartart_insert_items();

    fill_submenu(menus, "insert.shape", shape_entries);
    fill_submenu(menus, "insert.chart", super::chart_insert::chart_items());
    fill_submenu(menus, "insert.image", picture_subitems);
    let mut screenshot_entry: MenuEntry = screenshot_menu.into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "insert.image", screenshot_entry) {
            Ok(()) => break,
            Err(back) => screenshot_entry = back,
        }
    }
    menus.connect("xlsx.screenshot", |this, _, cx| { this.insert_sheet_screenshot(cx); });
    menus.connect("xlsx.screenshot.available-windows", |this, _, cx| { this.insert_sheet_screenshot_window(cx); });
    menus.connect("xlsx.screenshot.clipping", |this, _, cx| { this.insert_sheet_screenshot_clipping(cx); });
    menus.connect("xlsx.screenshot.clipboard", |this, _, cx| { this.insert_sheet_screenshot_from_clipboard(cx); });
    menus.connect("xlsx.insert.screenshot", |this, _, cx| { this.insert_sheet_screenshot(cx); });
    menus.connect("xlsx.insert.screenshot.available-windows", |this, _, cx| { this.insert_sheet_screenshot_window(cx); });
    menus.connect("xlsx.insert.screenshot.clipping", |this, _, cx| { this.insert_sheet_screenshot_clipping(cx); });
    menus.connect("xlsx.insert.screenshot.clipboard", |this, _, cx| { this.insert_sheet_screenshot_from_clipboard(cx); });
    fill_submenu(menus, "insert.smartart", smartart_items);
    menus.connect("insert.smartart", |this, _, cx| this.insert_smartart(crate::pptx::SmartArtKind::BasicProcess, cx));
    menus.connect("pptx.insert.smartart.list", |this, _, cx| this.insert_smartart(crate::pptx::SmartArtKind::BasicBlockList, cx));
    menus.connect("pptx.insert.smartart.vlist", |this, _, cx| this.insert_smartart(crate::pptx::SmartArtKind::VerticalList, cx));
    menus.connect("pptx.insert.smartart.process", |this, _, cx| this.insert_smartart(crate::pptx::SmartArtKind::BasicProcess, cx));
    menus.connect("pptx.insert.smartart.chevron", |this, _, cx| this.insert_smartart(crate::pptx::SmartArtKind::ContinuousChevronProcess, cx));
    menus.connect("pptx.insert.smartart.cycle", |this, _, cx| this.insert_smartart(crate::pptx::SmartArtKind::BasicCycle, cx));
    menus.connect("pptx.insert.smartart.hierarchy", |this, _, cx| this.insert_smartart(crate::pptx::SmartArtKind::OrganizationChart, cx));
    menus.connect("pptx.insert.smartart.relationship", |this, _, cx| this.insert_smartart(crate::pptx::SmartArtKind::BasicRelationship, cx));
    menus.connect("pptx.insert.smartart.matrix", |this, _, cx| this.insert_smartart(crate::pptx::SmartArtKind::BasicMatrix, cx));
    menus.connect("pptx.insert.smartart.pyramid", |this, _, cx| this.insert_smartart(crate::pptx::SmartArtKind::BasicPyramid, cx));
    menus.connect("pptx.insert.smartart.picture", |this, _, cx| this.insert_smartart(crate::pptx::SmartArtKind::PictureList, cx));
    menus.connect("insert.symbol", |this, _, cx| this.open_symbols(cx));
    menus.connect("insert.link", |this, window, cx| this.open_link_panel(window, cx));
    menus.connect("insert.comment", |this, window, cx| this.open_threaded_comment_panel(window, cx));
    menus.connect("xlsx.comment-delete", |this, _, cx| {
        if let Some((c, r)) = this.current_cell() {
            this.delete_threaded_comment(r, c, cx);
        }
    });
    menus.connect("xlsx.clear-comments", |this, _, cx| this.delete_threaded_comments_and_notes(cx));
    menus.connect("file.page-setup", |this, _, cx| this.open_page_setup(cx));
    menus.connect("xlsx.page-setup", |this, _, cx| this.open_page_setup(cx));
    menus.connect("xlsx.margins", |this, _, cx| this.open_page_setup(cx));
    menus.connect("xlsx.header-footer", |this, window, cx| this.open_header_footer_edit(super::header_footer_edit::HfTarget::Header, window, cx));
    menus.connect("xlsx.spelling.check", |this, window, cx| this.open_sheet_spelling(window, cx));
    menus.connect("xlsx.spelling", |this, window, cx| this.open_sheet_spelling(window, cx));
    menus.connect("xlsx.insert-link", |this, window, cx| this.open_link_panel(window, cx));
    menus.connect("xlsx.insert-symbol", |this, _, cx| this.open_symbols(cx));
    menus.connect("xlsx.logical-functions", |this, window, cx| this.insert_function_named("IF", window, cx));
    menus.connect("xlsx.datetime-functions", |this, window, cx| this.insert_function_named("TODAY", window, cx));
    menus.connect("xlsx.previous-comment", |this, _, cx| this.select_next_comment(false, cx));
    menus.connect("xlsx.next-comment", |this, _, cx| this.select_next_comment(true, cx));
    menus.connect("xlsx.show-all-notes", |this, _, cx| this.toggle_show_all_notes(cx));
    menus.connect("xlsx.page-orientation", |this, _, cx| this.open_page_setup(cx));
    menus.connect("xlsx.orientation-portrait", |this, _, cx| this.set_print(|p| p.landscape = false, None, cx));
    menus.connect("xlsx.orientation-landscape", |this, _, cx| this.set_print(|p| p.landscape = true, None, cx));
    menus.connect("xlsx.paper-size", |this, _, cx| this.open_page_setup(cx));
    menus.connect("xlsx.paper-a4", |this, _, cx| this.set_print(|p| p.paper = ibis_doc_model::SheetPrint::A4, None, cx));
    menus.connect("xlsx.paper-a3", |this, _, cx| this.set_print(|p| p.paper = (297 * 36_000, 420 * 36_000), None, cx));
    menus.connect("xlsx.paper-b5", |this, _, cx| this.set_print(|p| p.paper = (182 * 36_000, 257 * 36_000), None, cx));
    menus.connect("xlsx.paper-letter", |this, _, cx| this.set_print(|p| p.paper = (7_772_400, 10_058_400), None, cx));
    menus.connect("xlsx.print-titles", |this, _, cx| this.open_page_setup(cx));
    menus.connect("xlsx.print-titles-dialog", |this, _, cx| this.open_page_setup(cx));
    menus.connect("xlsx.print-titles-set-rows", |this, _, cx| {
        let rows = this.selected_range().map(|r| (r.first_row, r.last_row)).or_else(|| this.current_cell().map(|(_, r)| (r, r)));
        this.set_print_title_rows(rows, cx);
    });
    menus.connect("xlsx.print-titles-clear", |this, _, cx| {
        this.set_print_title_rows(None, cx);
        this.set_print_title_columns(None, cx);
    });
    menus.connect("xlsx.insert-function", |this, window, cx| this.insert_function_action(window, cx));
    menus.connect("xlsx.recent-functions", |this, window, cx| this.insert_function_named("SUM", window, cx));
    menus.connect("xlsx.financial-functions", |this, window, cx| this.insert_function_named("PMT", window, cx));
    menus.connect("xlsx.text-functions", |this, window, cx| this.insert_function_named("CONCAT", window, cx));
    menus.connect("xlsx.lookup-functions", |this, window, cx| this.insert_function_named("XLOOKUP", window, cx));
    menus.connect("xlsx.math-functions", |this, window, cx| this.insert_function_named("SUMIF", window, cx));
    menus.connect("xlsx.other-functions", |this, window, cx| this.insert_function_named("COUNTIF", window, cx));
    menus.connect("xlsx.use-in-formula", |this, window, cx| this.open_names(window, cx));
    menus.connect("xlsx.use-in-formula-paste", |this, window, cx| this.open_names(window, cx));
    menus.connect("xlsx.create-from-selection", |this, _, cx| { this.create_names_from_selection(true, false, cx); });
    menus.connect("xlsx.create-names-top-row", |this, _, cx| { this.create_names_from_selection(true, false, cx); });
    menus.connect("xlsx.create-names-left-col", |this, _, cx| { this.create_names_from_selection(false, true, cx); });
    menus.connect("xlsx.create-names-both", |this, _, cx| { this.create_names_from_selection(true, true, cx); });

    menus.connect("xlsx.fn.sum", |this, window, cx| this.insert_function_named("SUM", window, cx));
    menus.connect("xlsx.fn.average", |this, window, cx| this.insert_function_named("AVERAGE", window, cx));
    menus.connect("xlsx.fn.if", |this, window, cx| this.insert_function_named("IF", window, cx));
    menus.connect("xlsx.fn.ifs", |this, window, cx| this.insert_function_named("IFS", window, cx));
    menus.connect("xlsx.fn.iferror", |this, window, cx| this.insert_function_named("IFERROR", window, cx));
    menus.connect("xlsx.fn.and", |this, window, cx| this.insert_function_named("AND", window, cx));
    menus.connect("xlsx.fn.or", |this, window, cx| this.insert_function_named("OR", window, cx));
    menus.connect("xlsx.fn.not", |this, window, cx| this.insert_function_named("NOT", window, cx));
    menus.connect("xlsx.fn.count", |this, window, cx| this.insert_function_named("COUNT", window, cx));
    menus.connect("xlsx.fn.max", |this, window, cx| this.insert_function_named("MAX", window, cx));
    menus.connect("xlsx.fn.min", |this, window, cx| this.insert_function_named("MIN", window, cx));
    menus.connect("xlsx.fn.vlookup", |this, window, cx| this.insert_function_named("VLOOKUP", window, cx));
    menus.connect("xlsx.fn.xlookup", |this, window, cx| this.insert_function_named("XLOOKUP", window, cx));
    menus.connect("xlsx.fn.pmt", |this, window, cx| this.insert_function_named("PMT", window, cx));
    menus.connect("xlsx.fn.fv", |this, window, cx| this.insert_function_named("FV", window, cx));
    menus.connect("xlsx.fn.pv", |this, window, cx| this.insert_function_named("PV", window, cx));
    menus.connect("xlsx.fn.npv", |this, window, cx| this.insert_function_named("NPV", window, cx));
    menus.connect("xlsx.fn.irr", |this, window, cx| this.insert_function_named("IRR", window, cx));
    menus.connect("xlsx.fn.rate", |this, window, cx| this.insert_function_named("RATE", window, cx));
    menus.connect("xlsx.fn.nper", |this, window, cx| this.insert_function_named("NPER", window, cx));
    menus.connect("xlsx.fn.concat", |this, window, cx| this.insert_function_named("CONCAT", window, cx));
    menus.connect("xlsx.fn.text", |this, window, cx| this.insert_function_named("TEXT", window, cx));
    menus.connect("xlsx.fn.left", |this, window, cx| this.insert_function_named("LEFT", window, cx));
    menus.connect("xlsx.fn.right", |this, window, cx| this.insert_function_named("RIGHT", window, cx));
    menus.connect("xlsx.fn.mid", |this, window, cx| this.insert_function_named("MID", window, cx));
    menus.connect("xlsx.fn.len", |this, window, cx| this.insert_function_named("LEN", window, cx));
    menus.connect("xlsx.fn.trim", |this, window, cx| this.insert_function_named("TRIM", window, cx));
    menus.connect("xlsx.fn.substitute", |this, window, cx| this.insert_function_named("SUBSTITUTE", window, cx));
    menus.connect("xlsx.fn.hlookup", |this, window, cx| this.insert_function_named("HLOOKUP", window, cx));
    menus.connect("xlsx.fn.index", |this, window, cx| this.insert_function_named("INDEX", window, cx));
    menus.connect("xlsx.fn.match", |this, window, cx| this.insert_function_named("MATCH", window, cx));
    menus.connect("xlsx.fn.xmatch", |this, window, cx| this.insert_function_named("XMATCH", window, cx));
    menus.connect("xlsx.fn.filter", |this, window, cx| this.insert_function_named("FILTER", window, cx));
    menus.connect("xlsx.fn.unique", |this, window, cx| this.insert_function_named("UNIQUE", window, cx));
    menus.connect("xlsx.fn.sort", |this, window, cx| this.insert_function_named("SORT", window, cx));
    menus.connect("xlsx.fn.sumif", |this, window, cx| this.insert_function_named("SUMIF", window, cx));
    menus.connect("xlsx.fn.sumifs", |this, window, cx| this.insert_function_named("SUMIFS", window, cx));
    menus.connect("xlsx.fn.round", |this, window, cx| this.insert_function_named("ROUND", window, cx));
    menus.connect("xlsx.fn.roundup", |this, window, cx| this.insert_function_named("ROUNDUP", window, cx));
    menus.connect("xlsx.fn.rounddown", |this, window, cx| this.insert_function_named("ROUNDDOWN", window, cx));
    menus.connect("xlsx.fn.abs", |this, window, cx| this.insert_function_named("ABS", window, cx));
    menus.connect("xlsx.fn.product", |this, window, cx| this.insert_function_named("PRODUCT", window, cx));
    menus.connect("xlsx.fn.mod", |this, window, cx| this.insert_function_named("MOD", window, cx));
    menus.connect("xlsx.fn.countif", |this, window, cx| this.insert_function_named("COUNTIF", window, cx));
    menus.connect("xlsx.fn.countifs", |this, window, cx| this.insert_function_named("COUNTIFS", window, cx));
    menus.connect("xlsx.fn.averageif", |this, window, cx| this.insert_function_named("AVERAGEIF", window, cx));
    menus.connect("xlsx.fn.median", |this, window, cx| this.insert_function_named("MEDIAN", window, cx));
    menus.connect("xlsx.fn.stdev-s", |this, window, cx| this.insert_function_named("STDEV.S", window, cx));
    menus.connect("xlsx.fn.isblank", |this, window, cx| this.insert_function_named("ISBLANK", window, cx));
    menus.connect("xlsx.fn.iserror", |this, window, cx| this.insert_function_named("ISERROR", window, cx));
    menus.connect("file.info", |this, window, cx| this.open_sheet_doc_properties_panel(window, cx));
    menus.connect("xlsx.properties", |this, window, cx| this.open_sheet_doc_properties_panel(window, cx));
    menus.connect("xlsx.named-ranges", |this, window, cx| {
        if let Some(s) = this.sheet_state_mut() {
            s.validation = None;
            s.find = None;
        }
        this.open_names(window, cx);
    });
    menus.connect("xlsx.name-manager", |this, window, cx| {
        if let Some(s) = this.sheet_state_mut() {
            s.validation = None;
            s.find = None;
        }
        this.open_names(window, cx);
    });
    menus.connect("xlsx.define-name", |this, window, cx| {
        if let Some(s) = this.sheet_state_mut() {
            s.validation = None;
            s.find = None;
        }
        this.open_names(window, cx);
    });
    menus.connect("xlsx.remove-duplicates", |this, window, cx| this.open_dedupe_panel(window, cx));
    menus.connect("xlsx.scenario-manager", |this, window, cx| this.open_scenario_manager(window, cx));
    menus.connect("xlsx.text-to-columns", |this, window, cx| this.open_text_columns_panel(window, cx));
    menus.connect("xlsx.validation", |this, window, cx| {
        if let Some(s) = this.sheet_state_mut() {
            s.names = None;
            s.find = None;
        }
        this.open_validation_panel(window, cx);
    });
    menus.connect("xlsx.validation.circle-invalid", |this, _, cx| this.circle_invalid_data(cx));
    menus.connect("xlsx.validation.clear-circles", |this, _, cx| this.clear_validation_circles(cx));
    menus.connect("xlsx.trace-precedents", |this, _, cx| this.trace_precedents(cx));
    menus.connect("xlsx.trace-dependents", |this, _, cx| this.trace_dependents(cx));
    menus.connect("xlsx.remove-arrows", |this, _, cx| this.remove_all_arrows(cx));
    menus.connect("xlsx.remove-all-arrows", |this, _, cx| this.remove_all_arrows(cx));
    menus.connect("xlsx.remove-precedent-arrows", |this, _, cx| this.remove_precedent_arrows(cx));
    menus.connect("xlsx.remove-dependent-arrows", |this, _, cx| this.remove_dependent_arrows(cx));
    menus.connect("xlsx.evaluate-formula", |this, window, cx| this.open_evaluate_formula(window, cx));
    menus.connect("xlsx.watch-window", |this, window, cx| this.open_watch_window(window, cx));
    menus.connect("xlsx.accessibility-check", |this, window, cx| this.open_xlsx_accessibility_panel(window, cx));
    menus.connect("xlsx.compatibility-check", |this, window, cx| this.open_xlsx_compatibility_panel(window, cx));
    menus.connect("xlsx.error-options", |this, window, cx| this.open_error_options_panel(window, cx));
    menus.connect("xlsx.slicer-settings", |this, window, cx| this.open_slicer_settings_panel(window, cx));
    menus.connect("xlsx.slicer-insert", |this, window, cx| this.open_slicer_insert_panel(window, cx));
    menus.connect("xlsx.timeline-insert", |this, window, cx| this.open_timeline_insert_panel(window, cx));
    menus.connect("xlsx.pivot-insert-timeline", |this, window, cx| this.open_timeline_insert_panel(window, cx));
    menus.connect("xlsx.get-data", |this, window, cx| this.open_import_text_panel(window, cx));
    menus.connect("xlsx.query-from-table", |this, window, cx| this.open_query_from_selection(window, cx));
    menus.connect("xlsx.query-blank", |this, window, cx| this.open_blank_query(window, cx));
    menus.connect("xlsx.queries", |this, _, cx| this.open_queries_panel(cx));
    menus.connect("xlsx.control-properties", |this, window, cx| this.open_control_properties_panel(window, cx));
    menus.connect("xlsx.insert-control", |this, _, cx| this.insert_form_control(ibis_doc_model::ControlKind::Button, cx));
    menus.connect("xlsx.insert-control.button", |this, _, cx| this.insert_form_control(ibis_doc_model::ControlKind::Button, cx));
    menus.connect("xlsx.insert-control.checkbox", |this, _, cx| this.insert_form_control(ibis_doc_model::ControlKind::CheckBox, cx));
    menus.connect("xlsx.insert-control.radio", |this, _, cx| this.insert_form_control(ibis_doc_model::ControlKind::Radio, cx));
    menus.connect("xlsx.insert-control.dropdown", |this, _, cx| this.insert_form_control(ibis_doc_model::ControlKind::Drop, cx));
    menus.connect("xlsx.insert-control.list", |this, _, cx| this.insert_form_control(ibis_doc_model::ControlKind::List, cx));
    menus.connect("xlsx.insert-control.spin", |this, _, cx| this.insert_form_control(ibis_doc_model::ControlKind::Spin, cx));
    menus.connect("xlsx.insert-control.scroll", |this, _, cx| this.insert_form_control(ibis_doc_model::ControlKind::Scroll, cx));
    menus.connect("xlsx.insert-control.groupbox", |this, _, cx| this.insert_form_control(ibis_doc_model::ControlKind::GroupBox, cx));
    menus.connect("xlsx.insert-control.label", |this, _, cx| this.insert_form_control(ibis_doc_model::ControlKind::Label, cx));
    menus.connect("xlsx.design-mode", |this, _, cx| this.toggle_design_mode(cx));
    menus.connect("xlsx.view-code", |this, _, cx| this.view_code(cx));
    menus.connect("xlsx.xml-source", |this, window, cx| this.open_xml_source(window, cx));
    menus.connect("xlsx.xml-map-properties", |this, window, cx| this.open_xml_map_properties(window, cx));
    menus.connect("xlsx.3d-model", |this, window, cx| this.pick_sheet_3d_model(window, cx));
    menus.connect("xlsx.signature-line", |this, _, cx| { this.insert_sheet_signature_line("", "", cx); });
    menus.connect("xlsx.ink", |this, window, cx| this.open_ink_panel(window, cx));
    menus.connect("xlsx.autocorrect", |this, window, cx| this.open_autocorrect_panel(window, cx));
    menus.connect("xlsx.autorecover-settings", |this, window, cx| { this.wb_mut().close_settings(); this.open_autorecover_settings(window, cx) });
    menus.connect("xlsx.recover-unsaved", |this, _window, cx| { this.wb_mut().close_settings(); this.open_recover_unsaved(cx) });
    menus.connect("xlsx.backup-save-settings", |this, window, cx| this.open_xlsx_backup_save_settings_panel(window, cx));
    menus.connect("xlsx.open-and-repair", |this, window, cx| this.open_xlsx_repair_damaged(window, cx));
    menus.connect("xlsx.print-active-sheet", |this, _, cx| this.set_print_what(super::PrintWhat::ActiveSheet, cx));
    menus.connect("xlsx.print-entire-workbook", |this, _, cx| this.set_print_what(super::PrintWhat::EntireWorkbook, cx));
    menus.connect("xlsx.find-and-select", |this, _, cx| this.open_goto_special_panel(cx));
    menus.connect("xlsx.select-formulas", |this, _, cx| this.select_special_kind(GS::Formulas, cx));
    menus.connect("xlsx.select-notes", |this, _, cx| this.select_special_kind(GS::Comments, cx));
    menus.connect("xlsx.select-conditional-formats", |this, _, cx| this.select_special_kind(GS::Conditional, cx));
    menus.connect("xlsx.select-constants", |this, _, cx| this.select_special_kind(GS::Constants, cx));
    menus.connect("xlsx.select-data-validation", |this, _, cx| this.select_special_kind(GS::Validation, cx));
    menus.connect("xlsx.select-objects", |this, _, cx| this.select_special_kind(GS::Objects, cx));
    menus.connect("xlsx.scale-to-fit", |this, _, cx| this.open_page_setup(cx));
    menus.connect("xlsx.scale-width-auto", |this, _, cx| this.set_print_fit_width(0, cx));
    menus.connect("xlsx.scale-width-1", |this, _, cx| this.set_print_fit_width(1, cx));
    menus.connect("xlsx.scale-width-2", |this, _, cx| this.set_print_fit_width(2, cx));
    menus.connect("xlsx.scale-height-auto", |this, _, cx| this.set_print_fit_height(0, cx));
    menus.connect("xlsx.scale-height-1", |this, _, cx| this.set_print_fit_height(1, cx));
    menus.connect("xlsx.scale-height-2", |this, _, cx| this.set_print_fit_height(2, cx));
    menus.connect("xlsx.scale-100", |this, _, cx| this.set_print_scale_percent(100, cx));
    menus.connect("xlsx.scale-80", |this, _, cx| this.set_print_scale_percent(80, cx));
    menus.connect("xlsx.scale-50", |this, _, cx| this.set_print_scale_percent(50, cx));
    let text = |id: &str| crate::shell::i18n::text(id);
    let cur_table_style = editor
        .table_at_selection()
        .and_then(|idx| editor.current_sheet().and_then(|(_, s)| s.tables.get(idx)))
        .and_then(|t| t.style.as_ref().map(|st| st.name.clone()));
    const FORMAT_AS_TABLE_PRESETS: [(&str, &str); 7] = [
        ("xlsx.format-as-table.light-1", "TableStyleLight1"),
        ("xlsx.format-as-table.light-9", "TableStyleLight9"),
        ("xlsx.format-as-table.medium-2", "TableStyleMedium2"),
        ("xlsx.format-as-table.medium-4", "TableStyleMedium4"),
        ("xlsx.format-as-table.medium-9", "TableStyleMedium9"),
        ("xlsx.format-as-table.dark-1", "TableStyleDark1"),
        ("xlsx.format-as-table.dark-2", "TableStyleDark2"),
    ];
    let mut format_as_table_items: Vec<MenuEntry> = FORMAT_AS_TABLE_PRESETS
        .into_iter()
        .map(|(item_id, style_name)| {
            MenuItem::tr(item_id)
                .checked(cur_table_style.as_deref() == Some(style_name))
                .on(move |this, _, cx| this.format_as_table_with_style(style_name, cx))
                .into()
        })
        .collect();
    format_as_table_items.push(MenuEntry::Separator);
    format_as_table_items.push(
        MenuItem::tr("xlsx.new-table-style")
            .on(|this, window, cx| this.open_new_table_style(window, cx))
            .into(),
    );
    menus.connect("xlsx.format-as-table", |this, window, cx| this.open_table_dialog(window, cx));
    for (item_id, style_name) in FORMAT_AS_TABLE_PRESETS {
        menus.connect(item_id, move |this, _, cx| this.format_as_table_with_style(style_name, cx));
    }
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        Submenu::tr("xlsx.format-as-table", "table_chart", format_as_table_items).into(),
    );
    let table_menu = vec![
        MenuItem::tr("xlsx.table-design").icon("table").on(|this, window, cx| this.open_table_design(window, cx)).into(),
        MenuItem::new("xlsx.table-resize", text("_xlsx.table-resize")).on(|this, window, cx| this.resize_table_dialog(window, cx)).into(),
        MenuEntry::Separator,
        MenuItem::new("xlsx.table-header-row", text("docx.table-design.header-row")).on(|this, _, cx| this.toggle_table_header_row_at_selection(cx)).into(),
        MenuItem::new("xlsx.table-total-row", text("docx.table-design.total-row")).on(|this, _, cx| this.toggle_table_totals_row_at_selection(cx)).into(),
        MenuItem::new("xlsx.table-banded-rows", text("_xlsx.banded-rows")).on(|this, _, cx| this.toggle_table_banded_rows_at_selection(cx)).into(),
        MenuItem::new("xlsx.table-banded-columns", text("_xlsx.banded-columns")).on(|this, _, cx| this.toggle_table_banded_columns_at_selection(cx)).into(),
        MenuItem::new("xlsx.table-first-column", text("docx.table-design.first-column")).on(|this, _, cx| this.toggle_table_first_column_at_selection(cx)).into(),
        MenuItem::new("xlsx.table-last-column", text("docx.table-design.last-column")).on(|this, _, cx| this.toggle_table_last_column_at_selection(cx)).into(),
        MenuItem::new("xlsx.table-filter-button", text("_xlsx.filter-button")).on(|this, _, cx| this.toggle_table_filter_button_at_selection(cx)).into(),
        MenuItem::tr("xlsx.new-table-style").on(|this, window, cx| this.open_new_table_style(window, cx)).into(),
        MenuItem::new("xlsx.table-style-clear", text("_xlsx.clear")).on(|this, _, cx| this.clear_table_style_at_selection(cx)).into(),
    ];
    menus.connect("xlsx.new-table-style", |this, window, cx| this.open_new_table_style(window, cx));
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(Submenu::new("xlsx.table-menu", "table", text("_xlsx.table-design"), table_menu).into());

    let row_menu = vec![
        MenuItem::new("xlsx.row-height", text("_xlsx.row-height")).on(|this, window, cx| this.open_row_height_panel(window, cx)).into(),
        MenuItem::new("xlsx.autofit-row-height", text("_xlsx.autofit-row-height")).on(|this, window, cx| this.fit_selected(Axis::Rows, window, cx)).into(),
        MenuItem::new("xlsx.hide-rows", crate::shell::i18n::format("_xlsx.hide-unit", &[("unit", &text("_xlsx.row"))])).on(|this, _, cx| this.hide_lines(Axis::Rows, true, cx)).into(),
        MenuItem::new("xlsx.unhide-rows", crate::shell::i18n::format("_xlsx.unhide-unit", &[("unit", &text("_xlsx.row"))])).on(|this, _, cx| this.hide_lines(Axis::Rows, false, cx)).into(),
        MenuItem::new("xlsx.page-break", text("_xlsx.page-break")).on(|this, _, cx| this.toggle_row_break(cx)).into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(Submenu::new("xlsx.rows-menu", "table_rows", text("_xlsx.row"), row_menu).into());

    let col_menu = vec![
        MenuItem::new("xlsx.column-width", text("_xlsx.column-width")).on(|this, window, cx| this.open_column_width_panel(window, cx)).into(),
        MenuItem::new("xlsx.autofit-column-width", text("_xlsx.autofit-column-width")).on(|this, window, cx| this.fit_selected(Axis::Columns, window, cx)).into(),
        MenuItem::new("xlsx.hide-columns", crate::shell::i18n::format("_xlsx.hide-unit", &[("unit", &text("_xlsx.column"))])).on(|this, _, cx| this.hide_lines(Axis::Columns, true, cx)).into(),
        MenuItem::new("xlsx.unhide-columns", crate::shell::i18n::format("_xlsx.unhide-unit", &[("unit", &text("_xlsx.column"))])).on(|this, _, cx| this.hide_lines(Axis::Columns, false, cx)).into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(Submenu::new("xlsx.columns-menu", "view_column", text("_xlsx.column"), col_menu).into());

    let has_print_area = editor.current_sheet().is_some_and(|(_, s)| s.print_area.is_some());
    let print_area_menu = vec![
        MenuItem::tr("xlsx.print-area-set").on(|this, _, cx| this.set_print_area_from_selection(cx)).into(),
        MenuItem::tr("xlsx.print-area-add").on(|this, _, cx| this.add_to_print_area_from_selection(cx)).into(),
        MenuItem::tr("xlsx.print-area-clear").disabled(!has_print_area).on(|this, _, cx| this.clear_print_area(cx)).into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(Submenu::tr("xlsx.print-area-menu", "print", print_area_menu).into());

    menus.items_mut(crate::shell::chrome::MenuId::Format).push(super::conditional_quick::menu());
    menus.items_mut(crate::shell::chrome::MenuId::Format).extend(super::format_more::menus());
    // 表示形式 > セルのスタイル (Excel's ホーム > セルのスタイル).
    if let Some(styles) = editor.cell_style_menu() {
        menus.items_mut(crate::shell::chrome::MenuId::Format).push(styles);
    }
    // 表示形式 > テーマの配色 & テーマの色とフォント…
    let theme_palette_items: Vec<MenuEntry> = crate::pptx::theme_panel::THEME_PALETTES
        .iter()
        .map(|preset| {
            let pid = preset.id;
            let active = editor.is_xlsx_theme_palette_active(pid);
            let menu_id = match pid {
                "office" => "xlsx.theme-palette.office",
                "aspect" => "xlsx.theme-palette.aspect",
                "apex" => "xlsx.theme-palette.apex",
                "slate" => "xlsx.theme-palette.slate",
                "median" => "xlsx.theme-palette.median",
                "orient" => "xlsx.theme-palette.orient",
                "blue" => "xlsx.theme-palette.blue",
                _ => "xlsx.theme-palette",
            };
            MenuItem::new(menu_id, crate::shell::i18n::text(preset.label_key))
                .checked(active)
                .on(move |this, _, cx| this.apply_xlsx_theme_palette(pid, cx))
                .into()
        })
        .collect();
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        Submenu::tr("xlsx.theme-palette", "palette", theme_palette_items).into(),
    );
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.theme-colors")
            .icon("tune")
            .on(|this, window, cx| this.open_xlsx_theme_panel(window, cx))
            .into(),
    );
    let theme_fonts_items: Vec<MenuEntry> = super::theme_panel::THEME_FONTS_PRESETS
        .iter()
        .map(|preset| {
            let pid = preset.id;
            let active = editor.is_xlsx_theme_fonts_active(pid);
            MenuItem::new(preset.action_id, preset.label)
                .checked(active)
                .on(move |this, _, cx| this.apply_xlsx_theme_fonts(pid, cx))
                .into()
        })
        .chain(std::iter::once(
            MenuItem::tr("xlsx.theme-fonts.customize")
                .icon("tune")
                .on(|this, window, cx| this.open_xlsx_theme_panel(window, cx))
                .into(),
        ))
        .collect();
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        Submenu::tr("xlsx.theme-fonts", "font_download", theme_fonts_items).into(),
    );
    let theme_effects_items: Vec<MenuEntry> = ibis_pptx::theme_effects::THEME_EFFECTS
        .iter()
        .map(|preset| {
            let pid = preset.id;
            let active = editor.is_xlsx_theme_effects_active(pid);
            MenuItem::new(super::theme_panel::theme_effect_action_id(pid), crate::shell::i18n::text(preset.label_key))
                .checked(active)
                .on(move |this, _, cx| this.apply_xlsx_theme_effects(pid, cx))
                .into()
        })
        .collect();
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        Submenu::tr("xlsx.theme-effects", "auto_awesome", theme_effects_items).into(),
    );
    let has_chart = editor.sheet_chart().is_some();
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::new("xlsx.chart-switch-row-column", text("_xlsx.switch-row-column"))
            .icon("swap_horiz")
            .disabled(!has_chart)
            .on(|this, window, cx| this.switch_sheet_chart_row_column(window, cx))
            .into(),
    );
    let has_data_table = editor.sheet_chart().is_some_and(|(_, c)| c.data_table.is_some());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.chart-data-table")
            .icon("table_chart")
            .disabled(!has_chart)
            .checked(has_data_table)
            .on(|this, _, cx| {
                let shown = this.sheet_chart().map(|(_, c)| c.data_table.is_some()).unwrap_or(false);
                this.set_sheet_chart_data_table_shown(!shown, cx);
            })
            .into(),
    );
    let has_trendline = editor.sheet_chart().is_some_and(|(_, c)| {
        c.groups.iter().flat_map(|g| &g.series).any(|s| s.trendline.is_some())
    });
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.chart-trendline")
            .icon("show_chart")
            .disabled(!has_chart)
            .checked(has_trendline)
            .on(|this, _, cx| {
                this.toggle_sheet_chart_trendline(cx);
            })
            .into(),
    );
    let has_error_bars = editor.sheet_chart().is_some_and(|(_, c)| {
        c.groups.iter().flat_map(|g| &g.series).any(|s| s.error_bars.is_some())
    });
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.chart-error-bars")
            .icon("stacked_bar_chart")
            .disabled(!has_chart)
            .checked(has_error_bars)
            .on(|this, _, cx| {
                this.toggle_sheet_chart_error_bars(cx);
            })
            .into(),
    );
    let style_items:Vec<MenuEntry>=super::chart_edit::CHART_STYLES.iter().map(|(key,style)|{
        let style=*style;MenuItem::tr(key).disabled(!has_chart).on(move |e,_,cx|e.set_sheet_chart_visual_style(style,cx)).into()
    }).collect();
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.chart-styles", "palette", style_items).into(),
    );
    let palette_items:Vec<MenuEntry>=super::chart_edit::COLOR_PALETTES.iter().enumerate().map(|(i,(key,colors))| {
        let mut item=MenuItem::tr(key).disabled(!has_chart).on(move |e,_,cx|e.set_sheet_chart_color_palette(i,cx));
        item.dropdown_content=Some(crate::shell::chrome::dropdown::ItemContent::Palette(colors.to_vec()));item.into()
    }).collect();
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.chart-palettes", "color_lens", palette_items).into(),
    );
    let chart_obj = editor.sheet_chart().map(|(_, c)| c);
    let chart_type_items = crate::pptx::chrome::chart_type_items_for(chart_obj, SlideEditor::set_sheet_chart_type);
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.chart-type", "insert_chart", chart_type_items).into(),
    );
    const QUICK_LAYOUTS: [(&str, u32); 10] = [
        ("xlsx.chart-quick-layout-1", 1),
        ("xlsx.chart-quick-layout-2", 2),
        ("xlsx.chart-quick-layout-3", 3),
        ("xlsx.chart-quick-layout-4", 4),
        ("xlsx.chart-quick-layout-5", 5),
        ("xlsx.chart-quick-layout-6", 6),
        ("xlsx.chart-quick-layout-7", 7),
        ("xlsx.chart-quick-layout-8", 8),
        ("xlsx.chart-quick-layout-9", 9),
        ("xlsx.chart-quick-layout-10", 10),
    ];
    let quick_layout_items: Vec<MenuEntry> = QUICK_LAYOUTS
        .iter()
        .map(|&(id, layout)| {
            MenuItem::tr(id)
                .disabled(!has_chart)
                .on(move |this, _, cx| this.set_sheet_chart_quick_layout(layout, cx))
                .into()
        })
        .collect();
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.chart-quick-layout", "dashboard_customize", quick_layout_items).into(),
    );
    menus.connect("xlsx.theme-palette.office", |this, _, cx| this.apply_xlsx_theme_palette("office", cx));
    menus.connect("xlsx.theme-palette.aspect", |this, _, cx| this.apply_xlsx_theme_palette("aspect", cx));
    menus.connect("xlsx.theme-palette.apex", |this, _, cx| this.apply_xlsx_theme_palette("apex", cx));
    menus.connect("xlsx.theme-palette.slate", |this, _, cx| this.apply_xlsx_theme_palette("slate", cx));
    menus.connect("xlsx.theme-palette.median", |this, _, cx| this.apply_xlsx_theme_palette("median", cx));
    menus.connect("xlsx.theme-palette.orient", |this, _, cx| this.apply_xlsx_theme_palette("orient", cx));
    menus.connect("xlsx.theme-palette.blue", |this, _, cx| this.apply_xlsx_theme_palette("blue", cx));
    menus.connect("xlsx.theme-colors", |this, window, cx| this.open_xlsx_theme_panel(window, cx));
    menus.connect("xlsx.theme", |this, window, cx| this.open_xlsx_theme_panel(window, cx));
    menus.connect("xlsx.theme-fonts", |this, window, cx| this.open_xlsx_theme_panel(window, cx));
    menus.connect("xlsx.theme-fonts.customize", |this, window, cx| this.open_xlsx_theme_panel(window, cx));
    for preset in &super::theme_panel::THEME_FONTS_PRESETS {
        let pid = preset.id;
        menus.connect(preset.action_id, move |this, _, cx| this.apply_xlsx_theme_fonts(pid, cx));
    }
    menus.connect("xlsx.theme-font.office", |this, _, cx| this.apply_xlsx_theme_fonts("office", cx));
    menus.connect("xlsx.theme-font.aptos", |this, _, cx| this.apply_xlsx_theme_fonts("aptos", cx));
    menus.connect("xlsx.theme-font.arial", |this, _, cx| this.apply_xlsx_theme_fonts("arial", cx));
    menus.connect("xlsx.theme-font.century-gothic", |this, _, cx| this.apply_xlsx_theme_fonts("century-gothic", cx));
    menus.connect("xlsx.theme-font.meiryo", |this, _, cx| this.apply_xlsx_theme_fonts("meiryo", cx));
    menus.connect("xlsx.theme-font.yu-gothic", |this, _, cx| this.apply_xlsx_theme_fonts("yu-gothic", cx));
    menus.connect("xlsx.theme-effects", |this, _, cx| this.apply_xlsx_theme_effects("office", cx));
    for preset in &ibis_pptx::theme_effects::THEME_EFFECTS {
        let pid = preset.id;
        menus.connect(super::theme_panel::theme_effect_action_id(pid), move |this, _, cx| this.apply_xlsx_theme_effects(pid, cx));
    }
    menus.connect("xlsx.theme-effects.office", |this, _, cx| this.apply_xlsx_theme_effects("office", cx));
    menus.connect("xlsx.theme-effects.subtle", |this, _, cx| this.apply_xlsx_theme_effects("subtle", cx));
    menus.connect("xlsx.theme-effects.moderate", |this, _, cx| this.apply_xlsx_theme_effects("moderate", cx));
    menus.connect("xlsx.theme-effects.intense", |this, _, cx| this.apply_xlsx_theme_effects("intense", cx));
    menus.connect("xlsx.chart-data-table", |this, _, cx| {
        let shown = this.sheet_chart().map(|(_, c)| c.data_table.is_some()).unwrap_or(false);
        this.set_sheet_chart_data_table_shown(!shown, cx);
    });
    menus.connect("xlsx.chart-trendline", |this, _, cx| {
        this.toggle_sheet_chart_trendline(cx);
    });
    menus.connect("xlsx.chart-error-bars", |this, _, cx| {
        this.toggle_sheet_chart_error_bars(cx);
    });
    for (key,style) in super::chart_edit::CHART_STYLES {menus.connect(key,move |e,_,cx|e.set_sheet_chart_visual_style(style,cx));}
    menus.connect("xlsx.chart-palette-1", |this, _, cx| this.set_sheet_chart_color_palette(0, cx));
    menus.connect("xlsx.chart-palette-2", |this, _, cx| this.set_sheet_chart_color_palette(1, cx));
    menus.connect("xlsx.chart-palette-3", |this, _, cx| this.set_sheet_chart_color_palette(2, cx));
    menus.connect("xlsx.chart-palette-4", |this, _, cx| this.set_sheet_chart_color_palette(3, cx));
    for &(preset, id) in &crate::pptx::chart_panel::ChartPreset::ALL {
        menus.connect(id, move |this, _, cx| this.set_sheet_chart_type(preset, cx));
    }
    menus.connect("xlsx.chart-quick-layout-1", |this, _, cx| this.set_sheet_chart_quick_layout(1, cx));
    menus.connect("xlsx.chart-quick-layout-2", |this, _, cx| this.set_sheet_chart_quick_layout(2, cx));
    menus.connect("xlsx.chart-quick-layout-3", |this, _, cx| this.set_sheet_chart_quick_layout(3, cx));
    menus.connect("xlsx.chart-quick-layout-4", |this, _, cx| this.set_sheet_chart_quick_layout(4, cx));
    menus.connect("xlsx.chart-quick-layout-5", |this, _, cx| this.set_sheet_chart_quick_layout(5, cx));
    menus.connect("xlsx.chart-quick-layout-6", |this, _, cx| this.set_sheet_chart_quick_layout(6, cx));
    menus.connect("xlsx.chart-quick-layout-7", |this, _, cx| this.set_sheet_chart_quick_layout(7, cx));
    menus.connect("xlsx.chart-quick-layout-8", |this, _, cx| this.set_sheet_chart_quick_layout(8, cx));
    menus.connect("xlsx.chart-quick-layout-9", |this, _, cx| this.set_sheet_chart_quick_layout(9, cx));
    menus.connect("xlsx.chart-quick-layout-10", |this, _, cx| this.set_sheet_chart_quick_layout(10, cx));
    menus.connect("xlsx.chart-quick-layout", |this, _, cx| this.set_sheet_chart_quick_layout(1, cx));
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.alt-text")
            .icon("accessibility")
            .disabled(editor.selected_sheet_shape().is_none())
            .on(|this, window, cx| this.open_sheet_alt_text_panel(window, cx))
            .into(),
    );
    let has_shape = editor.selected_sheet_shape().is_some();
    let order_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.order-front")
            .icon("flip_to_front")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.order_sheet_shape(crate::pptx::editing::Order::Front, cx);
            })
            .into(),
        MenuItem::tr("xlsx.order-forward")
            .icon("arrow_upward")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.order_sheet_shape(crate::pptx::editing::Order::Forward, cx);
            })
            .into(),
        MenuItem::tr("xlsx.order-backward")
            .icon("arrow_downward")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.order_sheet_shape(crate::pptx::editing::Order::Backward, cx);
            })
            .into(),
        MenuItem::tr("xlsx.order-back")
            .icon("flip_to_back")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.order_sheet_shape(crate::pptx::editing::Order::Back, cx);
            })
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.order", "layers", order_items).into(),
    );
    let align_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.align.left")
            .icon("align_horizontal_left")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.align_sheet_shape(crate::pptx::editing::Align::Left, cx);
            })
            .into(),
        MenuItem::tr("xlsx.align.center")
            .icon("align_horizontal_center")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.align_sheet_shape(crate::pptx::editing::Align::Center, cx);
            })
            .into(),
        MenuItem::tr("xlsx.align.right")
            .icon("align_horizontal_right")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.align_sheet_shape(crate::pptx::editing::Align::Right, cx);
            })
            .into(),
        MenuItem::tr("xlsx.align.top")
            .icon("align_vertical_top")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.align_sheet_shape(crate::pptx::editing::Align::Top, cx);
            })
            .into(),
        MenuItem::tr("xlsx.align.middle")
            .icon("align_vertical_center")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.align_sheet_shape(crate::pptx::editing::Align::Middle, cx);
            })
            .into(),
        MenuItem::tr("xlsx.align.bottom")
            .icon("align_vertical_bottom")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                this.align_sheet_shape(crate::pptx::editing::Align::Bottom, cx);
            })
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.distribute.horizontal")
            .icon("horizontal_distribute")
            .disabled(editor.slide().shapes.len() < 4)
            .on(|this, _, cx| {
                this.distribute_sheet_shapes(true, cx);
            })
            .into(),
        MenuItem::tr("xlsx.distribute.vertical")
            .icon("vertical_distribute")
            .disabled(editor.slide().shapes.len() < 4)
            .on(|this, _, cx| {
                this.distribute_sheet_shapes(false, cx);
            })
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.align", "format_align_left", align_items).into(),
    );

    // 図形のグループ化 (xlsx.group-shapes)
    let can_grp = editor.can_group();
    let can_ungrp = editor.can_ungroup();
    let group_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.group-shapes.group")
            .icon("group_work")
            .disabled(!can_grp)
            .on(|this, _, cx| this.group_selected(cx))
            .into(),
        MenuItem::tr("xlsx.group-shapes.ungroup")
            .icon("ungroup")
            .disabled(!can_ungrp)
            .on(|this, _, cx| this.ungroup_selected(cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.group-shapes", "group_work", group_items).into(),
    );

    // 画像のトリミング (xlsx.crop)
    let is_picture = editor.selected_shapes().iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Picture(_)));
    let crop_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.crop.left")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures(ibis_doc_model::Crop { left: 10_000, ..Default::default() }, cx))
            .into(),
        MenuItem::tr("xlsx.crop.right")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures(ibis_doc_model::Crop { right: 10_000, ..Default::default() }, cx))
            .into(),
        MenuItem::tr("xlsx.crop.top")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures(ibis_doc_model::Crop { top: 10_000, ..Default::default() }, cx))
            .into(),
        MenuItem::tr("xlsx.crop.bottom")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures(ibis_doc_model::Crop { bottom: 10_000, ..Default::default() }, cx))
            .into(),
        MenuItem::tr("xlsx.crop.center")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures(ibis_doc_model::Crop { left: 10_000, top: 10_000, right: 10_000, bottom: 10_000 }, cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.crop.reset")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures(ibis_doc_model::Crop::default(), cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.crop", "crop", crop_items).into(),
    );

    // 画像の色 (xlsx.picture.color)
    let color_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.picture.color.none")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(None, cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.grayscale")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x000000), ibis_doc_model::Rgb(0xffffff))), cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.sepia")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x402000), ibis_doc_model::Rgb(0xffe0c0))), cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.blue")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x002060), ibis_doc_model::Rgb(0xd9e1f2))), cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.green")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x004010), ibis_doc_model::Rgb(0xe2efda))), cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.red")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x600000), ibis_doc_model::Rgb(0xfce4d6))), cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.orange")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x803000), ibis_doc_model::Rgb(0xffe6cc))), cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.purple")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x400060), ibis_doc_model::Rgb(0xf2e6ff))), cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.picture.color.transparent-white")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_clear(Some(ibis_doc_model::Rgb(0xffffff)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.transparent-black")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_clear(Some(ibis_doc_model::Rgb(0x000000)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.color.transparent-clear")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_clear(None, cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.picture.color", "palette", color_items).into(),
    );

    // 背景の削除（透明色） (xlsx.picture.remove-background)
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.picture.remove-background")
            .icon("auto_fix_high")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_clear(Some(ibis_doc_model::Rgb(0xffffff)), cx))
            .into(),
    );

    // 画像の補正 (xlsx.picture.correction)
    let correction_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.picture.lum.normal")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(None, cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.picture.lum.bright-p40")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((40_000, 0)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.lum.bright-p20")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((20_000, 0)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.lum.bright-m20")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((-20_000, 0)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.lum.bright-m40")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((-40_000, 0)), cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.picture.lum.contrast-p40")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((0, 40_000)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.lum.contrast-p20")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((0, 20_000)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.lum.contrast-m20")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((0, -20_000)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.lum.contrast-m40")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((0, -40_000)), cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.picture.lum.bright-contrast-high")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((20_000, 20_000)), cx))
            .into(),
        MenuItem::tr("xlsx.picture.lum.bright-contrast-low")
            .disabled(!is_picture)
            .on(|this, _, cx| this.set_selected_pictures_luminance(Some((-20_000, -20_000)), cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.picture.correction", "tune", correction_items).into(),
    );

    // 画像の角丸スタイル (xlsx.picture.corner)
    let corner_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.picture.corner.none")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures_corner(0, cx))
            .into(),
        MenuItem::tr("xlsx.picture.corner.small")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures_corner(5_000, cx))
            .into(),
        MenuItem::tr("xlsx.picture.corner.medium")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures_corner(10_000, cx))
            .into(),
        MenuItem::tr("xlsx.picture.corner.large")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures_corner(20_000, cx))
            .into(),
        MenuItem::tr("xlsx.picture.corner.round")
            .disabled(!is_picture)
            .on(|this, _, cx| this.crop_selected_pictures_corner(50_000, cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.picture.corner", "rounded_corner", corner_items).into(),
    );

    // 画像の色と効果のリセット (xlsx.picture.reset)
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.picture.reset")
            .icon("restart_alt")
            .disabled(!is_picture)
            .on(|this, _, cx| this.reset_picture_effects(cx))
            .into(),
    );

    // 図の圧縮 (xlsx.picture.compress)
    let compress_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.picture.compress.print")
            .disabled(!is_picture)
            .on(|this, _, cx| {
                this.compress_sheet_pictures(crate::pptx::compress::Resolution::Print, false, true, cx);
            })
            .into(),
        MenuItem::tr("xlsx.picture.compress.web")
            .disabled(!is_picture)
            .on(|this, _, cx| {
                this.compress_sheet_pictures(crate::pptx::compress::Resolution::Web, false, true, cx);
            })
            .into(),
        MenuItem::tr("xlsx.picture.compress.email")
            .disabled(!is_picture)
            .on(|this, _, cx| {
                this.compress_sheet_pictures(crate::pptx::compress::Resolution::Email, false, true, cx);
            })
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.picture.compress.delete-crop")
            .disabled(!is_picture)
            .on(|this, _, cx| {
                this.compress_sheet_pictures(crate::pptx::compress::Resolution::Print, true, true, cx);
            })
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.picture.compress", "photo_size_select_small", compress_items).into(),
    );

    // 図形の変更 (xlsx.change-shape)
    let is_auto_or_textbox = editor.selected_sheet_shape().and_then(|id| editor.slide().shape(id)).is_some_and(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Auto(_) | ibis_doc_model::ShapeKind::TextBox(_)));
    let change_shape_items: Vec<MenuEntry> = super::shape_edit::CHANGE_SHAPES
        .iter()
        .map(|&(preset, item_id, _, icon)| {
            MenuItem::tr(item_id)
                .icon(icon)
                .disabled(!is_auto_or_textbox)
                .on(move |this, _, cx| this.change_sheet_shape(preset, cx))
                .into()
        })
        .collect();
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.change-shape", "shapes", change_shape_items).into(),
    );

    // セルと連動 (xlsx.shape-textlink)
    let has_shape_formula = editor.selected_sheet_shape().and_then(|id| editor.slide().shape_formulas.get(&id)).is_some();
    let textlink_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.shape-textlink.current-cell")
            .icon("link")
            .disabled(!is_auto_or_textbox)
            .on(|this, _, cx| this.link_selected_shape_to_current_cell(cx))
            .into(),
        MenuItem::tr("xlsx.shape-textlink.clear")
            .icon("link_off")
            .disabled(!has_shape_formula)
            .on(|this, _, cx| this.clear_selected_shape_formula(cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.shape-textlink", "link", textlink_items).into(),
    );

    // 図形の塗りつぶし (xlsx.shape-fill)
    const SHAPE_FILL_SWATCHES: [(&str, &str, u32); 10] = [
        ("xlsx.shape-fill.black", "xlsx.shape-fill.black", 0x000000),
        ("xlsx.shape-fill.white", "xlsx.shape-fill.white", 0xffffff),
        ("xlsx.shape-fill.blue", "xlsx.shape-fill.blue", 0x2b579a),
        ("xlsx.shape-fill.cyan", "xlsx.shape-fill.cyan", 0x17a2b8),
        ("xlsx.shape-fill.green", "xlsx.shape-fill.green", 0x107c41),
        ("xlsx.shape-fill.yellow", "xlsx.shape-fill.yellow", 0xffc000),
        ("xlsx.shape-fill.orange", "xlsx.shape-fill.orange", 0xed7d31),
        ("xlsx.shape-fill.red", "xlsx.shape-fill.red", 0xc00000),
        ("xlsx.shape-fill.purple", "xlsx.shape-fill.purple", 0x7030a0),
        ("xlsx.shape-fill.gray", "xlsx.shape-fill.gray", 0x7f7f7f),
    ];
    let fill_none = MenuItem::tr("xlsx.shape-fill.none")
        .icon("format_color_reset")
        .disabled(!has_shape)
        .on(|this, _, cx| this.fill_sheet_shape(ibis_doc_model::Fill::None, cx));
    let fill_color_items: Vec<MenuEntry> = palette_menu_with_ids(
        editor,
        Some(fill_none),
        &SHAPE_FILL_SWATCHES,
        !has_shape,
        std::rc::Rc::new(|this, c, _, cx| {
            let fill = match c {
                Some(rgb) => ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(rgb)),
                None => ibis_doc_model::Fill::None,
            };
            this.fill_sheet_shape(fill, cx);
        }),
    );
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.shape-fill", "format_color_fill", fill_color_items).into(),
    );

    // 図形の枠線 (xlsx.shape-stroke)
    let width_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.shape-stroke.width-0-5")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_width(6_350, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.width-0-75")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_width(9_525, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.width-1-5")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_width(19_050, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.width-2-25")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_width(28_575, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.width-3")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_width(38_100, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.width-4-5")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_width(57_150, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.width-6")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_width(76_200, cx))
            .into(),
    ];
    let dash_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.shape-stroke.dash-solid")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::Solid, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.dash-dash")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::Dash, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.dash-dot")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::Dot, cx))
            .into(),
        MenuItem::tr("xlsx.shape-stroke.dash-dash-dot")
            .disabled(!has_shape)
            .on(|this, _, cx| this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::DashDot, cx))
            .into(),
    ];
    const SHAPE_STROKE_SWATCHES: [(&str, &str, u32); 10] = [
        ("xlsx.shape-stroke.black", "xlsx.shape-stroke.black", 0x000000),
        ("xlsx.shape-stroke.white", "xlsx.shape-stroke.white", 0xffffff),
        ("xlsx.shape-stroke.blue", "xlsx.shape-stroke.blue", 0x2b579a),
        ("xlsx.shape-stroke.cyan", "xlsx.shape-stroke.cyan", 0x17a2b8),
        ("xlsx.shape-stroke.green", "xlsx.shape-stroke.green", 0x107c41),
        ("xlsx.shape-stroke.yellow", "xlsx.shape-stroke.yellow", 0xffc000),
        ("xlsx.shape-stroke.orange", "xlsx.shape-stroke.orange", 0xed7d31),
        ("xlsx.shape-stroke.red", "xlsx.shape-stroke.red", 0xc00000),
        ("xlsx.shape-stroke.purple", "xlsx.shape-stroke.purple", 0x7030a0),
        ("xlsx.shape-stroke.gray", "xlsx.shape-stroke.gray", 0x7f7f7f),
    ];
    let stroke_none = MenuItem::tr("xlsx.shape-stroke.none")
        .icon("format_color_reset")
        .disabled(!has_shape)
        .on(|this, _, cx| this.stroke_sheet_shape_color(None, cx));
    let mut stroke_items: Vec<MenuEntry> = palette_menu_with_ids(
        editor,
        Some(stroke_none),
        &SHAPE_STROKE_SWATCHES,
        !has_shape,
        std::rc::Rc::new(|this, c, _, cx| {
            this.stroke_sheet_shape_color(c.map(ibis_doc_model::Color::opaque), cx);
        }),
    );
    stroke_items.push(MenuEntry::Separator);
    stroke_items.push(crate::shell::chrome::Submenu::tr("xlsx.shape-stroke.width", "line_weight", width_items).into());
    stroke_items.push(crate::shell::chrome::Submenu::tr("xlsx.shape-stroke.dash", "border_style", dash_items).into());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.shape-stroke", "border_color", stroke_items).into(),
    );

    // 図形の効果 (xlsx.shape-effect)
    let effect_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.shape-effect.shadow-none")
            .disabled(!has_shape)
            .on(|this, _, cx| this.shadow_sheet_shape(None, cx))
            .into(),
        MenuItem::tr("xlsx.shape-effect.shadow-bottom-right")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                let shadow = ibis_doc_model::Shadow::simple(
                    ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 128 },
                    0,
                    50_800,
                    2_700_000,
                );
                this.shadow_sheet_shape(Some(shadow), cx);
            })
            .into(),
        MenuItem::tr("xlsx.shape-effect.shadow-right")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                let shadow = ibis_doc_model::Shadow::simple(
                    ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 128 },
                    0,
                    50_800,
                    0,
                );
                this.shadow_sheet_shape(Some(shadow), cx);
            })
            .into(),
        MenuItem::tr("xlsx.shape-effect.shadow-soft")
            .disabled(!has_shape)
            .on(|this, _, cx| {
                let shadow = ibis_doc_model::Shadow::simple(
                    ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 128 },
                    50_800,
                    50_800,
                    2_700_000,
                );
                this.shadow_sheet_shape(Some(shadow), cx);
            })
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.shape-effect", "blur_on", effect_items).into(),
    );

    // テキストボックス・図形内テキストの書式設定 (XLSX-OBJECT-012 & XLSX-OBJECT-013)
    let has_text = editor.has_selected_sheet_text_shapes();
    const SHAPE_TEXT_COLOR_SWATCHES: [(&str, &str, u32); 10] = [
        ("xlsx.text.color-black", "xlsx.text.color-black", 0x000000),
        ("xlsx.text.color-white", "xlsx.text.color-white", 0xffffff),
        ("xlsx.text.color-blue", "xlsx.text.color-blue", 0x2b579a),
        ("xlsx.text.color-cyan", "xlsx.text.color-cyan", 0x17a2b8),
        ("xlsx.text.color-green", "xlsx.text.color-green", 0x107c41),
        ("xlsx.text.color-yellow", "xlsx.text.color-yellow", 0xffc000),
        ("xlsx.text.color-orange", "xlsx.text.color-orange", 0xed7d31),
        ("xlsx.text.color-red", "xlsx.text.color-red", 0xc00000),
        ("xlsx.text.color-purple", "xlsx.text.color-purple", 0x7030a0),
        ("xlsx.text.color-gray", "xlsx.text.color-gray", 0x7f7f7f),
    ];
    let text_color_items: Vec<MenuEntry> = palette_menu_with_ids(
        editor,
        None,
        &SHAPE_TEXT_COLOR_SWATCHES,
        !has_text,
        std::rc::Rc::new(|this, c, w, cx| {
            if let Some(rgb) = c {
                this.format_sheet_text_color(rgb, w, cx);
            }
        }),
    );
    const TEXT_SIZES: [(&'static str, u32); 15] = [
        ("xlsx.text.size-8", 8),
        ("xlsx.text.size-9", 9),
        ("xlsx.text.size-10", 10),
        ("xlsx.text.size-11", 11),
        ("xlsx.text.size-12", 12),
        ("xlsx.text.size-14", 14),
        ("xlsx.text.size-16", 16),
        ("xlsx.text.size-18", 18),
        ("xlsx.text.size-20", 20),
        ("xlsx.text.size-24", 24),
        ("xlsx.text.size-28", 28),
        ("xlsx.text.size-32", 32),
        ("xlsx.text.size-36", 36),
        ("xlsx.text.size-44", 44),
        ("xlsx.text.size-72", 72),
    ];
    let text_size_items: Vec<MenuEntry> = TEXT_SIZES.into_iter().map(|(id, pt)| {
        MenuItem::new(id, format!("{pt} pt")).disabled(!has_text).on(move |this, w, cx| this.format_sheet_text_size(pt, w, cx)).into()
    }).collect();

    let text_format_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.text.bold").icon("format_bold").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_bold(w, cx)).into(),
        MenuItem::tr("xlsx.text.italic").icon("format_italic").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_italic(w, cx)).into(),
        MenuItem::tr("xlsx.text.underline").icon("format_underlined").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_underline(w, cx)).into(),
        MenuItem::tr("xlsx.text.strikethrough").icon("format_strikethrough").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_strikethrough(w, cx)).into(),
        MenuEntry::Separator,
        crate::shell::chrome::Submenu::tr("xlsx.text.color", "format_color_text", text_color_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.text.size", "format_size", text_size_items).into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.text-format", "text_format", text_format_items).into(),
    );

    let text_align_items: Vec<MenuEntry> = vec![
        MenuItem::tr("align.left").icon("format_align_left").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_align(ibis_doc_model::TextAlign::Left, w, cx)).into(),
        MenuItem::tr("align.center").icon("format_align_center").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_align(ibis_doc_model::TextAlign::Center, w, cx)).into(),
        MenuItem::tr("align.right").icon("format_align_right").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_align(ibis_doc_model::TextAlign::Right, w, cx)).into(),
        MenuItem::tr("align.justify").icon("format_align_justify").disabled(!has_text).on(|this, w, cx| this.format_sheet_text_align(ibis_doc_model::TextAlign::Justify, w, cx)).into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.text-align", "format_align_left", text_align_items).into(),
    );

    let vertical_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.text-vertical.top").icon("vertical_align_top").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_vertical(ibis_doc_model::VerticalAnchor::Top, cx)).into(),
        MenuItem::tr("xlsx.text-vertical.middle").icon("vertical_align_center").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_vertical(ibis_doc_model::VerticalAnchor::Middle, cx)).into(),
        MenuItem::tr("xlsx.text-vertical.bottom").icon("vertical_align_bottom").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_vertical(ibis_doc_model::VerticalAnchor::Bottom, cx)).into(),
    ];
    let direction_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.text-direction.horizontal").icon("format_textdirection_l_to_r").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_direction(false, 0, cx)).into(),
        MenuItem::tr("xlsx.text-direction.vertical").icon("format_textdirection_vertical").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_direction(true, 0, cx)).into(),
        MenuItem::tr("xlsx.text-direction.rotate-90").icon("rotate_90_degrees_cw").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_direction(false, 5_400_000, cx)).into(),
        MenuItem::tr("xlsx.text-direction.rotate-270").icon("rotate_90_degrees_ccw").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_direction(false, 16_200_000, cx)).into(),
    ];
    let insets_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.text-insets.none").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_insets(ibis_doc_model::Insets { left: 0, top: 0, right: 0, bottom: 0 }, cx)).into(),
        MenuItem::tr("xlsx.text-insets.normal").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_insets(ibis_doc_model::Insets::default(), cx)).into(),
        MenuItem::tr("xlsx.text-insets.wide").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_insets(ibis_doc_model::Insets { left: 182_880, top: 91_440, right: 182_880, bottom: 91_440 }, cx)).into(),
    ];

    let text_box_items: Vec<MenuEntry> = vec![
        crate::shell::chrome::Submenu::tr("xlsx.text-vertical", "vertical_align_center", vertical_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.text-direction", "format_textdirection_l_to_r", direction_items).into(),
        crate::shell::chrome::Submenu::tr("xlsx.text-insets", "padding", insets_items).into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.text.wrap").disabled(!has_text).on(|this, _, cx| this.format_sheet_text_wrap(cx)).into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.text-box", "text_fields", text_box_items).into(),
    );
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.text.wordart-styles", "title", wordart_style_items(editor)).into(),
    );
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.text.warp", "transform", text_warp_items(editor)).into(),
    );

    let shape_placement = editor.selected_sheet_shape().and_then(|id| editor.slide().shape_placements.get(&id).copied()).unwrap_or_default();
    let shape_print = editor.selected_sheet_shape().and_then(|id| editor.slide().shape_prints.get(&id).copied()).unwrap_or(true);

    let placement_items: Vec<MenuEntry> = vec![
        MenuItem::tr("xlsx.placement.move-and-size")
            .disabled(!has_shape)
            .checked(has_shape && shape_placement == ibis_doc_model::ObjectPlacement::MoveAndSize)
            .on(|this, _, cx| this.set_sheet_shape_placement(ibis_doc_model::ObjectPlacement::MoveAndSize, cx))
            .into(),
        MenuItem::tr("xlsx.placement.move-only")
            .disabled(!has_shape)
            .checked(has_shape && shape_placement == ibis_doc_model::ObjectPlacement::MoveOnly)
            .on(|this, _, cx| this.set_sheet_shape_placement(ibis_doc_model::ObjectPlacement::MoveOnly, cx))
            .into(),
        MenuItem::tr("xlsx.placement.free")
            .disabled(!has_shape)
            .checked(has_shape && shape_placement == ibis_doc_model::ObjectPlacement::Free)
            .on(|this, _, cx| this.set_sheet_shape_placement(ibis_doc_model::ObjectPlacement::Free, cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.placement.print")
            .disabled(!has_shape)
            .checked(has_shape && shape_print)
            .on(|this, _, cx| this.toggle_sheet_shape_print(cx))
            .into(),
    ];
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        crate::shell::chrome::Submenu::tr("xlsx.object-properties", "settings", placement_items).into(),
    );

    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.frame-settings")
            .icon("straighten")
            .disabled(!has_shape)
            .on(|this, window, cx| {
                this.open_sheet_frame_panel(window, cx);
            })
            .into(),
    );
    let sel_pane_open = editor.sheet_state().is_some_and(|s| s.panels.selection_pane.is_some());
    menus.items_mut(crate::shell::chrome::MenuId::Format).push(
        MenuItem::tr("xlsx.selection-pane")
            .icon("visibility")
            .checked(sel_pane_open)
            .on(|this, window, cx| {
                this.toggle_xlsx_selection_pane(window, cx);
            })
            .into(),
    );
    menus.items_mut(crate::shell::chrome::MenuId::View).push(
        MenuItem::tr("xlsx.selection-pane")
            .icon("visibility")
            .checked(sel_pane_open)
            .on(|this, window, cx| {
                this.toggle_xlsx_selection_pane(window, cx);
            })
            .into(),
    );
    // 編集 > 形式を選択して貼り付け, after 書式なしで貼り付け.
    use super::clipboard::PasteSpecial as P;
    let special = |id: &'static str, label: &'static str, kind: P| -> MenuEntry { MenuItem::new(id, crate::shell::i18n::text(label)).on(move |this, _, cx| this.paste_special(kind, cx)).into() };
    let submenu: MenuEntry = crate::shell::chrome::Submenu::tr("xlsx.paste-special", "content_paste",
        vec![
            MenuItem::new("xlsx.paste-special-dialog", crate::shell::i18n::text("xlsx.paste-special-dots")).icon("content_paste").shortcut("⌘⌥V").on(|this, _, cx| this.open_paste_special_dialog_panel(cx)).into(),
            MenuEntry::Separator,
            special("xlsx.paste-all", "_xlsx.all", P::All),
            special("xlsx.paste-values", "_xlsx.values", P::Values),
            special("xlsx.paste-formulas", "_xlsx.equation", P::Formulas),
            special("xlsx.paste-formats", "_xlsx.formats", P::Formats),
            special("xlsx.paste-comments", "xlsx.paste-comments", P::Comments),
            special("xlsx.paste-validation", "xlsx.paste-validation", P::Validation),
            special("xlsx.paste-all-except-borders", "xlsx.paste-all-except-borders", P::AllExceptBorders),
            special("xlsx.paste-column-widths", "xlsx.paste-column-widths", P::ColumnWidths),
            special("xlsx.paste-formulas-and-number-formats", "xlsx.paste-formulas-and-number-formats", P::FormulasAndNumberFormats),
            special("xlsx.paste-values-and-number-formats", "xlsx.paste-values-and-number-formats", P::ValuesAndNumberFormats),
            special("xlsx.paste-transpose", "_xlsx.transpose", P::Transpose),
            special("xlsx.paste-link", "_xlsx.paste-link", P::Link),
            MenuItem::tr("xlsx.paste-picture").icon("image").on(|this, _, cx| this.paste_as_picture(false, cx)).into(),
            MenuItem::tr("xlsx.paste-linked-picture").icon("add_photo_alternate").on(|this, _, cx| this.paste_as_picture(true, cx)).into(),
            MenuEntry::Separator,
            special("xlsx.paste-add", "_xlsx.add", P::Add),
            special("xlsx.paste-subtract", "_xlsx.subtract", P::Subtract),
            special("xlsx.paste-multiply", "_xlsx.multiply", P::Multiply),
            special("xlsx.paste-divide", "_xlsx.divide", P::Divide),
            MenuEntry::Separator,
            // A setting for the items above, as the dialog's check box.
            MenuItem::tr("xlsx.paste-skip-blanks")
                .checked(editor.sheet_state().is_some_and(|s| s.panels.skip_blanks))
                .on(|this, _, cx| {
                    if let Some(s) = this.sheet_state_mut() {
                        s.panels.skip_blanks = !s.panels.skip_blanks;
                    }
                    cx.notify();
                })
                .into(),
        ],
    )
    .into();
    for (_, entries) in menus.menus.iter_mut() {
        if insert_after(entries, "edit.paste-plain", submenu.clone()).is_ok() {
            break;
        }
    }
    // 編集 > クリア (Excel's ホーム > 編集 > クリア)
    let edit = menus.items_mut(crate::shell::chrome::MenuId::Edit);
    let at = edit.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "edit.delete")).map_or(edit.len(), |i| i + 1);
    let clear_submenu = vec![
        MenuItem::tr("xlsx.clear-all").icon("delete").on(|this, _, cx| this.clear_all_cells(cx)).into(),
        MenuItem::tr("xlsx.clear-formats").icon("format_clear").on(|this, _, cx| this.clear_cell_formats(cx)).into(),
        MenuItem::tr("xlsx.clear-contents").icon("backspace").on(|this, _, cx| this.clear_selected_cells(cx)).into(),
        MenuItem::tr("xlsx.clear-comments")
            .icon("comment")
            .on(|this, window, cx| {
                this.delete_threaded_comments_and_notes(cx);
                this.focus_handle.focus(window, cx);
            })
            .into(),
        MenuItem::tr("xlsx.clear-hyperlinks").icon("link_off").on(|this, _, cx| this.remove_cell_link(cx)).into(),
    ];
    edit.insert(at, crate::shell::chrome::Submenu::tr("xlsx.clear-menu", "delete", clear_submenu).into());

    let clear_sparklines_submenu = vec![
        MenuItem::tr("xlsx.sparkline-clear-selected").on(|this, _, cx| this.clear_sparklines_action(false, cx)).into(),
        MenuItem::tr("xlsx.sparkline-clear-group").on(|this, _, cx| this.clear_sparklines_action(true, cx)).into(),
    ];
    edit.insert(at + 1, crate::shell::chrome::Submenu::tr("xlsx.clear-sparklines", "delete", clear_sparklines_submenu).into());

    // 編集 > フィル (Excel's ホーム > 編集 > フィル)
    let fill_submenu = vec![
        MenuItem::tr("xlsx.fill-down").icon("arrow_downward").shortcut("⌘D").on(|this, _, cx| this.fill_selection_edge(crate::xlsx::fill::FillDirection::Down, cx)).into(),
        MenuItem::tr("xlsx.fill-right").icon("arrow_forward").shortcut("⌘R").on(|this, _, cx| this.fill_selection_edge(crate::xlsx::fill::FillDirection::Right, cx)).into(),
        MenuItem::tr("xlsx.fill-up").icon("arrow_upward").on(|this, _, cx| this.fill_selection_edge(crate::xlsx::fill::FillDirection::Up, cx)).into(),
        MenuItem::tr("xlsx.fill-left").icon("arrow_back").on(|this, _, cx| this.fill_selection_edge(crate::xlsx::fill::FillDirection::Left, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("xlsx.series").icon("format_list_numbered").on(|this, window, cx| this.open_series_panel(window, cx)).into(),
        MenuItem::tr("flash-fill.done").icon("auto_fix_high").shortcut("⌃E").on(|this, _, cx| this.flash_fill_column(cx)).into(),
    ];
    edit.insert(at + 2, crate::shell::chrome::Submenu::tr("xlsx.fill-menu", "format_color_fill", fill_submenu).into());

    // 編集 > オートSUM (Excel's ホーム > 編集 > オートSUM)
    let autosum_submenu = vec![
        MenuItem::new("xlsx.autosum", crate::shell::i18n::text("_xlsx.sum")).icon("functions").shortcut("⌥=").on(|this, _, cx| this.auto_function("SUM", cx)).into(),
        MenuItem::new("xlsx.autosum.sum", crate::shell::i18n::text("_xlsx.sum")).icon("functions").on(|this, _, cx| this.auto_function("SUM", cx)).into(),
        MenuItem::new("xlsx.autosum.average", crate::shell::i18n::text("_xlsx.average")).on(|this, _, cx| this.auto_function("AVERAGE", cx)).into(),
        MenuItem::new("xlsx.autosum.count", crate::shell::i18n::text("_xlsx.count-numbers")).on(|this, _, cx| this.auto_function("COUNT", cx)).into(),
        MenuItem::new("xlsx.autosum.max", crate::shell::i18n::text("_xlsx.max")).on(|this, _, cx| this.auto_function("MAX", cx)).into(),
        MenuItem::new("xlsx.autosum.min", crate::shell::i18n::text("_xlsx.min-2")).on(|this, _, cx| this.auto_function("MIN", cx)).into(),
    ];
    edit.insert(at + 3, crate::shell::chrome::Submenu::new("xlsx.autosum-menu", "functions", crate::shell::i18n::text("_shell.functions"), autosum_submenu).into());
    menus.connect("xlsx.autosum", |this, _, cx| this.auto_function("SUM", cx));
    menus.connect("xlsx.autosum.sum", |this, _, cx| this.auto_function("SUM", cx));
    menus.connect("xlsx.autosum.average", |this, _, cx| this.auto_function("AVERAGE", cx));
    menus.connect("xlsx.autosum.count", |this, _, cx| this.auto_function("COUNT", cx));
    menus.connect("xlsx.autosum.max", |this, _, cx| this.auto_function("MAX", cx));
    menus.connect("xlsx.autosum.min", |this, _, cx| this.auto_function("MIN", cx));
    menus.connect("edit.select-all", |this, _, cx| this.select_all_cells(cx));
    menus.connect("xlsx.insert-row-above", |this, _, cx| this.insert_lines(Axis::Rows, false, cx));
    menus.connect("xlsx.insert-row-below", |this, _, cx| this.insert_lines(Axis::Rows, true, cx));
    menus.connect("xlsx.insert-column-left", |this, _, cx| this.insert_lines(Axis::Columns, false, cx));
    menus.connect("xlsx.insert-column-right", |this, _, cx| this.insert_lines(Axis::Columns, true, cx));
    menus.connect("xlsx.delete-row", |this, _, cx| this.delete_lines(Axis::Rows, cx));
    menus.connect("xlsx.delete-column", |this, _, cx| this.delete_lines(Axis::Columns, cx));
    menus.connect("xlsx.insert-sheet", |this, window, cx| this.insert_sheet(window, cx));
    menus.connect("xlsx.insert-table", |this, window, cx| this.open_table_dialog(window, cx));
    menus.connect("xlsx.table-to-range", |this, _, cx| this.table_to_range(cx));
    menus.connect("xlsx.table-resize", |this, window, cx| this.resize_table_dialog(window, cx));
    menus.connect("xlsx.table-design", |this, window, cx| this.open_table_design(window, cx));
    menus.connect("xlsx.table-header-row", |this, _, cx| this.toggle_table_header_row_at_selection(cx));
    menus.connect("xlsx.table-total-row", |this, _, cx| this.toggle_table_totals_row_at_selection(cx));
    menus.connect("xlsx.table-banded-rows", |this, _, cx| this.toggle_table_banded_rows_at_selection(cx));
    menus.connect("xlsx.table-banded-columns", |this, _, cx| this.toggle_table_banded_columns_at_selection(cx));
    menus.connect("xlsx.table-first-column", |this, _, cx| this.toggle_table_first_column_at_selection(cx));
    menus.connect("xlsx.table-last-column", |this, _, cx| this.toggle_table_last_column_at_selection(cx));
    menus.connect("xlsx.table-filter-button", |this, _, cx| this.toggle_table_filter_button_at_selection(cx));
    menus.connect("xlsx.table-style-clear", |this, _, cx| this.clear_table_style_at_selection(cx));
    menus.connect("xlsx.table-insert-row-above", |this, _, cx| this.insert_table_rows_above(cx));
    menus.connect("xlsx.table-insert-row-below", |this, _, cx| this.insert_table_rows_below(cx));
    menus.connect("xlsx.table-insert-col-left", |this, _, cx| this.insert_table_columns_left(cx));
    menus.connect("xlsx.table-insert-col-right", |this, _, cx| this.insert_table_columns_right(cx));
    menus.connect("xlsx.table-delete-row", |this, _, cx| this.delete_table_rows(cx));
    menus.connect("xlsx.table-delete-col", |this, _, cx| this.delete_table_columns(cx));
    menus.connect("xlsx.row-height", |this, window, cx| this.open_row_height_panel(window, cx));
    menus.connect("xlsx.column-width", |this, window, cx| this.open_column_width_panel(window, cx));
    menus.connect("xlsx.autofit-column-width", |this, window, cx| this.fit_selected(Axis::Columns, window, cx));
    menus.connect("xlsx.autofit-row-height", |this, window, cx| this.fit_selected(Axis::Rows, window, cx));
    menus.connect("xlsx.hide-rows", |this, _, cx| this.hide_lines(Axis::Rows, true, cx));
    menus.connect("xlsx.unhide-rows", |this, _, cx| this.hide_lines(Axis::Rows, false, cx));
    menus.connect("xlsx.hide-columns", |this, _, cx| this.hide_lines(Axis::Columns, true, cx));
    menus.connect("xlsx.unhide-columns", |this, _, cx| this.hide_lines(Axis::Columns, false, cx));
    menus.connect("xlsx.print-area-set", |this, _, cx| this.set_print_area_from_selection(cx));
    menus.connect("xlsx.print-area-add", |this, _, cx| this.add_to_print_area_from_selection(cx));
    menus.connect("xlsx.print-area-clear", |this, _, cx| this.clear_print_area(cx));
    menus.connect("xlsx.page-break", |this, _, cx| this.toggle_row_break(cx));
    menus.connect("xlsx.pivot-insert", |this, window, cx| this.open_create_pivot(window, cx));
    menus.connect("xlsx.recommended-pivots", |this, window, cx| this.open_recommended_pivots(window, cx));
    // "xlsx.recommended-pivot-tables" (the menu list's id) is an alias of "xlsx.recommended-pivots" (see `canonical_menu_id`): both insert the recommended PivotTable; connecting the alias again would replace this.
    menus.connect("xlsx.pivot-chart-insert", |this, window, cx| this.open_insert_pivot_chart(window, cx));
    menus.connect("xlsx.chart-format", |this, window, cx| this.open_sheet_chart_panel(window, cx));
    menus.connect("xlsx.chart-switch-row-column", |this, window, cx| this.switch_sheet_chart_row_column(window, cx));
    menus.connect("xlsx.switch-row-column", |this, window, cx| this.switch_sheet_chart_row_column(window, cx));
    menus.connect("xlsx.move-chart", |this, window, cx| this.open_move_chart_panel(window, cx));
    menus.connect("xlsx.chart.save-template", |this, window, cx| this.open_chart_template_panel(crate::xlsx::chart_template::ChartTemplateMode::Save, window, cx));
    menus.connect("xlsx.chart-save-template", |this, window, cx| this.open_chart_template_panel(crate::xlsx::chart_template::ChartTemplateMode::Save, window, cx));
    menus.connect("xlsx.chart.apply-template", |this, window, cx| this.open_chart_template_panel(crate::xlsx::chart_template::ChartTemplateMode::Apply, window, cx));
    menus.connect("xlsx.chart-apply-template", |this, window, cx| this.open_chart_template_panel(crate::xlsx::chart_template::ChartTemplateMode::Apply, window, cx));
    menus.connect("xlsx.alt-text", |this, window, cx| this.open_sheet_alt_text_panel(window, cx));
    menus.connect("shape.alt-text", |this, window, cx| this.open_sheet_alt_text_panel(window, cx));
    menus.connect("xlsx.chart-alt-text", |this, window, cx| this.open_sheet_alt_text_panel(window, cx));
    menus.connect("xlsx.delete-sheet", |this, _, cx| this.delete_sheet(cx));
    menus.connect("xlsx.rename-sheet", |this, window, cx| this.begin_sheet_rename(this.current_slide, window, cx));
    menus.connect("xlsx.sparkline-line", |this, window, cx| this.open_sparkline_panel(Some(ibis_doc_model::SparklineKind::Line), window, cx));
    menus.connect("xlsx.sparkline-column", |this, window, cx| this.open_sparkline_panel(Some(ibis_doc_model::SparklineKind::Column), window, cx));
    menus.connect("xlsx.sparkline-winloss", |this, window, cx| this.open_sparkline_panel(Some(ibis_doc_model::SparklineKind::WinLoss), window, cx));
    menus.connect("xlsx.sparkline-clear-selected", |this, _, cx| this.clear_sparklines_action(false, cx));
    menus.connect("xlsx.sparkline-clear-group", |this, _, cx| this.clear_sparklines_action(true, cx));
    menus.connect("xlsx.sparkline-group", |this, _, cx| this.group_selected_sparklines(cx));
    menus.connect("xlsx.sparkline-ungroup", |this, _, cx| this.ungroup_selected_sparklines(cx));
    menus.connect("xlsx.sparkline-style-1", |this, _, cx| this.set_selected_sparklines_style(0, cx));
    menus.connect("xlsx.sparkline-style-2", |this, _, cx| this.set_selected_sparklines_style(1, cx));
    menus.connect("xlsx.sparkline-style-3", |this, _, cx| this.set_selected_sparklines_style(2, cx));
    menus.connect("xlsx.sparkline-style-4", |this, _, cx| this.set_selected_sparklines_style(3, cx));
    menus.connect("xlsx.sparkline-style-5", |this, _, cx| this.set_selected_sparklines_style(4, cx));
    menus.connect("xlsx.sparkline-style-6", |this, _, cx| this.set_selected_sparklines_style(5, cx));
    menus.connect("xlsx.sparkline-color-blue", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x376092)), cx));
    menus.connect("xlsx.sparkline-color-red", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0xC00000)), cx));
    menus.connect("xlsx.sparkline-color-green", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x70AD47)), cx));
    menus.connect("xlsx.sparkline-color-purple", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x7030A0)), cx));
    menus.connect("xlsx.sparkline-color-cyan", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x4BACC6)), cx));
    menus.connect("xlsx.sparkline-color-orange", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0xED7D31)), cx));
    menus.connect("xlsx.sparkline-color-gray", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x7F7F7F)), cx));
    menus.connect("xlsx.sparkline-color-black", |this, _, cx| this.set_selected_sparklines_series_color(Some(ibis_doc_model::Rgb(0x000000)), cx));
    menus.connect("xlsx.sparkline-display-hidden-on", |this, _, cx| this.set_selected_sparklines_display_hidden(true, cx));
    menus.connect("xlsx.sparkline-display-hidden-off", |this, _, cx| this.set_selected_sparklines_display_hidden(false, cx));
    menus.connect("xlsx.sparkline-date-axis-on", |this, _, cx| this.set_selected_sparklines_date_axis(true, cx));
    menus.connect("xlsx.sparkline-date-axis-off", |this, _, cx| this.set_selected_sparklines_date_axis(false, cx));
    menus.connect("xlsx.sparkline-empty-gap", |this, _, cx| this.set_selected_sparklines_empty(ibis_doc_model::SparklineEmpty::Gap, cx));
    menus.connect("xlsx.sparkline-empty-zero", |this, _, cx| this.set_selected_sparklines_empty(ibis_doc_model::SparklineEmpty::Zero, cx));
    menus.connect("xlsx.sparkline-empty-span", |this, _, cx| this.set_selected_sparklines_empty(ibis_doc_model::SparklineEmpty::Span, cx));
    menus.connect("xlsx.sparkline-high-point", |this, _, cx| this.toggle_selected_sparklines_point(crate::xlsx::sparkline_dialog::SparklinePointKind::High, cx));
    menus.connect("xlsx.sparkline-low-point", |this, _, cx| this.toggle_selected_sparklines_point(crate::xlsx::sparkline_dialog::SparklinePointKind::Low, cx));
    menus.connect("xlsx.sparkline-negative-points", |this, _, cx| this.toggle_selected_sparklines_point(crate::xlsx::sparkline_dialog::SparklinePointKind::Negative, cx));
    menus.connect("xlsx.sparkline-first-point", |this, _, cx| this.toggle_selected_sparklines_point(crate::xlsx::sparkline_dialog::SparklinePointKind::First, cx));
    menus.connect("xlsx.sparkline-last-point", |this, _, cx| this.toggle_selected_sparklines_point(crate::xlsx::sparkline_dialog::SparklinePointKind::Last, cx));
    menus.connect("xlsx.sparkline-markers", |this, _, cx| this.toggle_selected_sparklines_point(crate::xlsx::sparkline_dialog::SparklinePointKind::Markers, cx));
    menus.connect("xlsx.sparkline-weight-75", |this, _, cx| this.set_selected_sparklines_line_weight(75, cx));
    menus.connect("xlsx.sparkline-weight-100", |this, _, cx| this.set_selected_sparklines_line_weight(100, cx));
    menus.connect("xlsx.sparkline-weight-150", |this, _, cx| this.set_selected_sparklines_line_weight(150, cx));
    menus.connect("xlsx.sparkline-weight-225", |this, _, cx| this.set_selected_sparklines_line_weight(225, cx));
    menus.connect("xlsx.sparkline-show-axis", |this, _, cx| this.toggle_selected_sparklines_display_x_axis(cx));
    menus.connect("xlsx.sparkline-right-to-left", |this, _, cx| this.toggle_selected_sparklines_right_to_left(cx));
    menus.connect("xlsx.sparkline-min-individual", |this, _, cx| this.set_selected_sparklines_min_axis(ibis_doc_model::SparklineAxis::Individual, cx));
    menus.connect("xlsx.sparkline-min-group", |this, _, cx| this.set_selected_sparklines_min_axis(ibis_doc_model::SparklineAxis::Group, cx));
    menus.connect("xlsx.sparkline-min-custom", |this, window, cx| this.open_sparkline_panel(None, window, cx));
    menus.connect("xlsx.sparkline-max-individual", |this, _, cx| this.set_selected_sparklines_max_axis(ibis_doc_model::SparklineAxis::Individual, cx));
    menus.connect("xlsx.sparkline-max-group", |this, _, cx| this.set_selected_sparklines_max_axis(ibis_doc_model::SparklineAxis::Group, cx));
    menus.connect("xlsx.sparkline-max-custom", |this, window, cx| this.open_sparkline_panel(None, window, cx));
    for (_, _, pt, palette) in MARKER_COLOR_MENUS {
        for (item_id, _, hex) in palette {
            menus.connect(item_id, move |this, _, cx| {
                this.set_selected_sparklines_point_color(pt, Some(ibis_doc_model::Rgb(hex)), cx);
            });
        }
    }
    menus.connect("xlsx.pivot-fields", |this, window, cx| this.open_pivot_fields(window, cx));
    menus.connect("xlsx.pivot-value-settings", |this, window, cx| this.open_pivot_value_settings(window, cx));
    menus.connect("xlsx.pivot-options", |this, window, cx| this.open_pivot_options(window, cx));
    menus.connect("xlsx.pivot-change-data-source", |this, window, cx| this.open_pivot_change_source(window, cx));
    menus.connect("xlsx.pivot-drill-buttons", |this, _, cx| this.toggle_pivot_drill_buttons(cx));
    menus.connect("xlsx.pivot-show-details", |this, _, cx| this.show_pivot_details_at_selection(cx));
    menus.connect("xlsx.pivot-calculated-field", |this, window, cx| this.open_pivot_calc_field(window, cx));
    menus.connect("xlsx.pivot-calculated-item", |this, window, cx| this.open_pivot_calc_item(window, cx));
    menus.connect("xlsx.pivot-group", |this, window, cx| this.open_pivot_group(window, cx));
    menus.connect("xlsx.pivot-ungroup", |this, _, cx| this.pivot_ungroup(cx));
    menus.connect("xlsx.pivot-refresh", |this, _, cx| this.refresh_pivots(false, cx));
    menus.connect("xlsx.pivot-refresh-all", |this, _, cx| this.refresh_pivots(true, cx));
    menus.connect("xlsx.pivot-clear-all", |this, _, cx| this.clear_pivot_all(cx));
    menus.connect("xlsx.pivot-clear-filters", |this, _, cx| this.clear_pivot_filters(cx));
    menus.connect("xlsx.pivot-select-entire-table", |this, _, cx| this.select_pivot_part(0, cx));
    menus.connect("xlsx.pivot-select-values", |this, _, cx| this.select_pivot_part(1, cx));
    menus.connect("xlsx.pivot-select-labels", |this, _, cx| this.select_pivot_part(2, cx));
    menus.connect("xlsx.pivot-insert-blank-line", |this, _, cx| this.set_pivot_blank_line(true, cx));
    menus.connect("xlsx.pivot-remove-blank-line", |this, _, cx| this.set_pivot_blank_line(false, cx));
    menus.connect("xlsx.pivot-banded-rows", |this, _, cx| this.toggle_pivot_banded_rows(cx));
    menus.connect("xlsx.pivot-banded-columns", |this, _, cx| this.toggle_pivot_banded_columns(cx));
    for &(id, name, _) in &crate::xlsx::pivot::PIVOT_STYLES {
        menus.connect(id, move |this, _, cx| this.set_pivot_style_named(name, cx));
    }
    menus.connect("xlsx.pivot-report-layout", |this, _, cx| this.set_pivot_report_layout(crate::xlsx::pivot::PivotReportLayout::Compact, cx));
    menus.connect("xlsx.pivot-layout-compact", |this, _, cx| this.set_pivot_report_layout(crate::xlsx::pivot::PivotReportLayout::Compact, cx));
    menus.connect("xlsx.pivot-layout-outline", |this, _, cx| this.set_pivot_report_layout(crate::xlsx::pivot::PivotReportLayout::Outline, cx));
    menus.connect("xlsx.pivot-layout-tabular", |this, _, cx| this.set_pivot_report_layout(crate::xlsx::pivot::PivotReportLayout::Tabular, cx));
    menus.connect("xlsx.pivot-subtotal-off", |this, _, cx| this.set_pivot_subtotals(0, cx));
    menus.connect("xlsx.pivot-subtotal-bottom", |this, _, cx| this.set_pivot_subtotals(1, cx));
    menus.connect("xlsx.pivot-subtotal-top", |this, _, cx| this.set_pivot_subtotals(2, cx));
    menus.connect("xlsx.pivot-grand-rows", |this, _, cx| this.toggle_pivot_row_grand_totals(cx));
    menus.connect("xlsx.pivot-grand-cols", |this, _, cx| this.toggle_pivot_col_grand_totals(cx));
    menus.connect("xlsx.pivot-repeat-labels", |this, _, cx| this.toggle_pivot_repeat_item_labels(cx));
    menus.connect("xlsx.pivot-clear", |this, _, cx| this.clear_pivot_fields(cx));
    menus.connect("xlsx.pivot-select", |this, _, cx| this.select_entire_pivot(cx));
    menus.connect("xlsx.pivot-move", |this, window, cx| this.open_pivot_move(window, cx));
    menus.connect("xlsx.pivot-blank-rows", |this, _, cx| this.toggle_pivot_blank_rows(cx));
    menus.connect("xlsx.pivot-row-headers", |this, _, cx| this.toggle_pivot_row_headers(cx));
    menus.connect("xlsx.pivot-col-headers", |this, _, cx| this.toggle_pivot_col_headers(cx));
    menus.connect("xlsx.pivot-styles", |this, _, cx| this.set_pivot_style("PivotStyleLight16", cx));
    menus.connect("xlsx.recommended-charts", |this, window, cx| this.open_recommended_charts(window, cx));
    menus.connect("xlsx.thesaurus", |this, window, cx| this.open_sheet_thesaurus(window, cx));
    menus.connect("tools.dictionary", |this, window, cx| this.open_sheet_thesaurus(window, cx));
    menus.connect("edit.copy", |this, _, cx| this.copy_cells(false, cx));
    menus.connect("xlsx.copy-as-picture", |this, _, cx| this.copy_as_picture(cx));
    menus.connect("edit.cut", |this, _, cx| this.copy_cells(true, cx));
    menus.connect("edit.paste", |this, _, cx| this.paste_cells(false, cx));
    menus.connect("edit.paste-plain", |this, _, cx| this.paste_cells(true, cx));
    // 書式に合わせて貼り付け: the values (and formulas as values) in the cells' own formats.
    menus.connect("edit.paste-match", |this, _, cx| this.paste_cells(true, cx));
    // 書式のコピー / 書式の貼り付け (⌥⌘C / ⌥⌘V): the selected cells' formats picked up; the selection then takes them.
    menus.connect("edit.copy-format", |this, _, cx| {
        if this.sheet_state().is_some_and(|s| s.painter.is_none()) {
            this.toggle_format_paint(cx);
        }
    });
    menus.connect("edit.paste-format", |this, _, cx| this.finish_format_paint(cx));
    menus.connect("xlsx.protect", |this, window, cx| this.open_protect_panel(window, cx));
    menus.connect("xlsx.split-text", |this, window, cx| this.open_text_columns_panel(window, cx));
    menus.connect("tools.word-count", |this, window, cx| this.open_sheet_word_count(window, cx));
    menus.connect("tools.accessibility", |this, window, cx| this.open_xlsx_accessibility_panel(window, cx));
    menus.connect("format.clear", |this, _, cx| this.clear_cell_formats(cx));
    menus.connect("xlsx.clear-formats", |this, _, cx| this.clear_cell_formats(cx));
    menus.connect("xlsx.clear-contents", |this, _, cx| this.clear_selected_cells(cx));
    menus.connect("xlsx.clear-hyperlinks", |this, _, cx| this.remove_cell_link(cx));
    menus.connect("xlsx.cell-link-open", |this, window, cx| {
        this.follow_cell_link(window, cx);
    });
    menus.connect("xlsx.cell-link-edit", |this, window, cx| {
        this.open_link_panel(window, cx);
    });
    menus.connect("xlsx.cell-link-remove", |this, _, cx| {
        this.remove_cell_link(cx);
    });
    menus.connect("xlsx.fill-down", |this, _, cx| this.fill_selection_edge(crate::xlsx::fill::FillDirection::Down, cx));
    menus.connect("xlsx.fill-right", |this, _, cx| this.fill_selection_edge(crate::xlsx::fill::FillDirection::Right, cx));
    menus.connect("xlsx.fill-up", |this, _, cx| this.fill_selection_edge(crate::xlsx::fill::FillDirection::Up, cx));
    menus.connect("xlsx.fill-left", |this, _, cx| this.fill_selection_edge(crate::xlsx::fill::FillDirection::Left, cx));
    menus.connect("xlsx.underline-single", |this, _, cx| this.format_cells(ibis_doc_model::StyleChange::UnderlineKind(ibis_doc_model::CellUnderline::Single), cx));
    menus.connect("xlsx.underline-double", |this, _, cx| this.format_cells(ibis_doc_model::StyleChange::UnderlineKind(ibis_doc_model::CellUnderline::Double), cx));
    menus.connect("xlsx.comma-style", |this, _, cx| this.format_cells(ibis_doc_model::StyleChange::NumberFormat("#,##0".into()), cx));
    menus.connect("_shell.comma-style", |this, _, cx| this.format_cells(ibis_doc_model::StyleChange::NumberFormat("#,##0".into()), cx));
    menus.connect("xlsx.percent-style", |this, _, cx| this.format_cells(ibis_doc_model::StyleChange::NumberFormat("0%".into()), cx));
    menus.connect("xlsx.decrease-decimal", |this, _, cx| this.step_decimals(false, cx));
    menus.connect("xlsx.increase-decimal", |this, _, cx| this.step_decimals(true, cx));
    menus.connect("xlsx.merge-center", |this, _, cx| this.merge_cells(true, cx));
    menus.connect("xlsx.merge-across", |this, _, cx| this.merge_across(cx));
    menus.connect("xlsx.merge", |this, _, cx| this.merge_cells(false, cx));
    menus.connect("xlsx.unmerge", |this, _, cx| this.unmerge_cells(cx));
    menus.connect("xlsx.wrap-text", |this, _, cx| {
        let wrap = this.current_style().wrap;
        this.format_cells(ibis_doc_model::StyleChange::Wrap(!wrap), cx);
    });
    menus.connect("xlsx.decrease-indent", |this, _, cx| {
        let indent = this.current_style().indent.saturating_sub(1);
        this.format_cells(ibis_doc_model::StyleChange::Indent(indent), cx);
    });
    menus.connect("xlsx.increase-indent", |this, _, cx| {
        let indent = this.current_style().indent + 1;
        this.format_cells(ibis_doc_model::StyleChange::Indent(indent), cx);
    });
    menus.connect("xlsx.format-cells", |this, window, cx| this.open_custom_format(window, cx));
    menus.connect("xlsx.cell-format", |this, window, cx| this.open_custom_format(window, cx));
    menus.connect("xlsx.protect-workbook", |this, window, cx| this.open_protect_workbook_panel(window, cx));
    menus.connect("xlsx.unprotect-workbook", |this, window, cx| this.open_protect_workbook_panel(window, cx));
    menus.connect("xlsx.allow-edit-ranges", |this, window, cx| this.open_allow_edit_ranges_panel(window, cx));
    menus.connect("xlsx.paste-special-dialog", |this, _, cx| this.open_paste_special_dialog_panel(cx));
    menus.connect("xlsx.paste-validation", |this, _, cx| this.paste_special(super::clipboard::PasteSpecial::Validation, cx));
    menus.connect("xlsx.paste-picture", |this, _, cx| this.paste_as_picture(false, cx));
    menus.connect("xlsx.paste-linked-picture", |this, _, cx| this.paste_as_picture(true, cx));
    menus.connect("xlsx.camera-tool", |this, _, cx| this.insert_camera_picture(cx));
    menus.connect("insert.text-box", |this, window, cx| { this.insert_sheet_textbox(window, cx); });
    menus.connect("xlsx.insert-textbox", |this, window, cx| { this.insert_sheet_textbox(window, cx); });
    menus.connect("xlsx.shape.rect", |this, _, cx| { this.insert_sheet_shape("rect", cx); });
    menus.connect("xlsx.shape.round-rect", |this, _, cx| { this.insert_sheet_shape("roundRect", cx); });
    menus.connect("xlsx.shape.ellipse", |this, _, cx| { this.insert_sheet_shape("ellipse", cx); });
    menus.connect("xlsx.shape.triangle", |this, _, cx| { this.insert_sheet_shape("triangle", cx); });
    menus.connect("xlsx.shape.right-arrow", |this, _, cx| { this.insert_sheet_shape("rightArrow", cx); });
    menus.connect("xlsx.shape.star5", |this, _, cx| { this.insert_sheet_shape("star5", cx); });
    menus.connect("xlsx.shape.callout", |this, _, cx| { this.insert_sheet_shape("wedgeRectCallout", cx); });
    menus.connect("xlsx.shape.heart", |this, _, cx| { this.insert_sheet_shape("heart", cx); });
    menus.connect("xlsx.shape.line", |this, _, cx| { this.insert_sheet_line(false, cx); });
    menus.connect("xlsx.shape.line-arrow", |this, _, cx| { this.insert_sheet_line(true, cx); });
    menus.connect("shape.rect", |this, _, cx| { this.insert_sheet_shape("rect", cx); });
    menus.connect("shape.roundRect", |this, _, cx| { this.insert_sheet_shape("roundRect", cx); });
    menus.connect("shape.ellipse", |this, _, cx| { this.insert_sheet_shape("ellipse", cx); });
    menus.connect("shape.triangle", |this, _, cx| { this.insert_sheet_shape("triangle", cx); });
    menus.connect("shape.rightArrow", |this, _, cx| { this.insert_sheet_shape("rightArrow", cx); });
    menus.connect("shape.star5", |this, _, cx| { this.insert_sheet_shape("star5", cx); });
    menus.connect("shape.wedgeRectCallout", |this, _, cx| { this.insert_sheet_shape("wedgeRectCallout", cx); });
    menus.connect("shape.heart", |this, _, cx| { this.insert_sheet_shape("heart", cx); });
    menus.connect("xlsx.insert-picture", |this, window, cx| { this.pick_sheet_picture(window, cx); });
    menus.connect("xlsx.insert-picture-over-cells", |this, window, cx| { this.pick_sheet_picture(window, cx); });
    menus.connect("xlsx.insert-picture-in-cell", |this, window, cx| { this.pick_sheet_pictures_in_cells(window, cx); });
    menus.connect("xlsx.place-in-cell", |this, _, cx| {
        if let Some(id) = this.selected_sheet_shape() {
            this.place_picture_in_cell(id, cx);
        }
    });
    menus.connect("xlsx.place-over-cells", |this, _, cx| {
        if let Some(id) = this.selected_sheet_shape() {
            this.place_picture_over_cells(id, cx);
        } else {
            this.place_current_cell_picture_over_cells(cx);
        }
    });
    menus.connect("xlsx.image-file", |this, window, cx| { this.pick_sheet_picture(window, cx); });
    menus.connect("image.file", |this, window, cx| { this.pick_sheet_picture(window, cx); });
    menus.connect("xlsx.order-front", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Front, cx); });
    menus.connect("xlsx.order-back", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Back, cx); });
    menus.connect("xlsx.order-forward", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Forward, cx); });
    menus.connect("xlsx.order-backward", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Backward, cx); });
    menus.connect("order.front", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Front, cx); });
    menus.connect("order.back", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Back, cx); });
    menus.connect("order.forward", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Forward, cx); });
    menus.connect("order.backward", |this, _, cx| { this.order_sheet_shape(crate::pptx::editing::Order::Backward, cx); });
    menus.connect("xlsx.align.left", |this, _, cx| { this.align_sheet_shape(crate::pptx::editing::Align::Left, cx); });
    menus.connect("xlsx.align.center", |this, _, cx| { this.align_sheet_shape(crate::pptx::editing::Align::Center, cx); });
    menus.connect("xlsx.align.right", |this, _, cx| { this.align_sheet_shape(crate::pptx::editing::Align::Right, cx); });
    menus.connect("xlsx.align.top", |this, _, cx| { this.align_sheet_shape(crate::pptx::editing::Align::Top, cx); });
    menus.connect("xlsx.align.middle", |this, _, cx| { this.align_sheet_shape(crate::pptx::editing::Align::Middle, cx); });
    menus.connect("xlsx.align.bottom", |this, _, cx| { this.align_sheet_shape(crate::pptx::editing::Align::Bottom, cx); });
    menus.connect("xlsx.distribute.horizontal", |this, _, cx| { this.distribute_sheet_shapes(true, cx); });
    menus.connect("xlsx.distribute.vertical", |this, _, cx| { this.distribute_sheet_shapes(false, cx); });
    menus.connect("xlsx.frame-settings", |this, window, cx| { this.open_sheet_frame_panel(window, cx); });
    menus.connect("xlsx.selection-pane", |this, window, cx| { this.toggle_xlsx_selection_pane(window, cx); });

    menus.connect("xlsx.shape-fill.none", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::None, cx); });
    menus.connect("xlsx.shape-fill.black", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::BLACK), cx); });
    menus.connect("xlsx.shape-fill.white", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::WHITE), cx); });
    menus.connect("xlsx.shape-fill.blue", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x2b579a))), cx); });
    menus.connect("xlsx.shape-fill.cyan", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x17a2b8))), cx); });
    menus.connect("xlsx.shape-fill.green", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x107c41))), cx); });
    menus.connect("xlsx.shape-fill.yellow", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffc000))), cx); });
    menus.connect("xlsx.shape-fill.orange", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xed7d31))), cx); });
    menus.connect("xlsx.shape-fill.red", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xc00000))), cx); });
    menus.connect("xlsx.shape-fill.purple", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x7030a0))), cx); });
    menus.connect("xlsx.shape-fill.gray", |this, _, cx| { this.fill_sheet_shape(ibis_doc_model::Fill::Solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x7f7f7f))), cx); });

    menus.connect("xlsx.shape-stroke.none", |this, _, cx| { this.stroke_sheet_shape_color(None, cx); });
    menus.connect("xlsx.shape-stroke.black", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::BLACK), cx); });
    menus.connect("xlsx.shape-stroke.white", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::WHITE), cx); });
    menus.connect("xlsx.shape-stroke.blue", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x2b579a))), cx); });
    menus.connect("xlsx.shape-stroke.cyan", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x17a2b8))), cx); });
    menus.connect("xlsx.shape-stroke.green", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x107c41))), cx); });
    menus.connect("xlsx.shape-stroke.yellow", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xffc000))), cx); });
    menus.connect("xlsx.shape-stroke.orange", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xed7d31))), cx); });
    menus.connect("xlsx.shape-stroke.red", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xc00000))), cx); });
    menus.connect("xlsx.shape-stroke.purple", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x7030a0))), cx); });
    menus.connect("xlsx.shape-stroke.gray", |this, _, cx| { this.stroke_sheet_shape_color(Some(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0x7f7f7f))), cx); });

    menus.connect("xlsx.shape-stroke.width-0-5", |this, _, cx| { this.stroke_sheet_shape_width(6_350, cx); });
    menus.connect("xlsx.shape-stroke.width-0-75", |this, _, cx| { this.stroke_sheet_shape_width(9_525, cx); });
    menus.connect("xlsx.shape-stroke.width-1-5", |this, _, cx| { this.stroke_sheet_shape_width(19_050, cx); });
    menus.connect("xlsx.shape-stroke.width-2-25", |this, _, cx| { this.stroke_sheet_shape_width(28_575, cx); });
    menus.connect("xlsx.shape-stroke.width-3", |this, _, cx| { this.stroke_sheet_shape_width(38_100, cx); });
    menus.connect("xlsx.shape-stroke.width-4-5", |this, _, cx| { this.stroke_sheet_shape_width(57_150, cx); });
    menus.connect("xlsx.shape-stroke.width-6", |this, _, cx| { this.stroke_sheet_shape_width(76_200, cx); });

    menus.connect("xlsx.shape-stroke.dash-solid", |this, _, cx| { this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::Solid, cx); });
    menus.connect("xlsx.shape-stroke.dash-dash", |this, _, cx| { this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::Dash, cx); });
    menus.connect("xlsx.shape-stroke.dash-dot", |this, _, cx| { this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::Dot, cx); });
    menus.connect("xlsx.shape-stroke.dash-dash-dot", |this, _, cx| { this.stroke_sheet_shape_dash(ibis_doc_model::LineDash::DashDot, cx); });

    menus.connect("xlsx.shape-effect.shadow-none", |this, _, cx| { this.shadow_sheet_shape(None, cx); });
    menus.connect("xlsx.shape-effect.shadow-bottom-right", |this, _, cx| {
        let shadow = ibis_doc_model::Shadow::simple(ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 128 }, 0, 50_800, 2_700_000);
        this.shadow_sheet_shape(Some(shadow), cx);
    });
    menus.connect("xlsx.shape-effect.shadow-right", |this, _, cx| {
        let shadow = ibis_doc_model::Shadow::simple(ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 128 }, 0, 50_800, 0);
        this.shadow_sheet_shape(Some(shadow), cx);
    });
    menus.connect("xlsx.shape-effect.shadow-soft", |this, _, cx| {
        let shadow = ibis_doc_model::Shadow::simple(ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 128 }, 50_800, 50_800, 2_700_000);
        this.shadow_sheet_shape(Some(shadow), cx);
    });

    menus.connect("xlsx.group-shapes", |this, _, cx| { this.group_selected(cx); });
    menus.connect("xlsx.group-shapes.group", |this, _, cx| { this.group_selected(cx); });
    menus.connect("xlsx.group-shapes.ungroup", |this, _, cx| { this.ungroup_selected(cx); });

    menus.connect("xlsx.crop.left", |this, _, cx| { this.crop_selected_pictures(ibis_doc_model::Crop { left: 10_000, ..Default::default() }, cx); });
    menus.connect("xlsx.crop.right", |this, _, cx| { this.crop_selected_pictures(ibis_doc_model::Crop { right: 10_000, ..Default::default() }, cx); });
    menus.connect("xlsx.crop.top", |this, _, cx| { this.crop_selected_pictures(ibis_doc_model::Crop { top: 10_000, ..Default::default() }, cx); });
    menus.connect("xlsx.crop.bottom", |this, _, cx| { this.crop_selected_pictures(ibis_doc_model::Crop { bottom: 10_000, ..Default::default() }, cx); });
    menus.connect("xlsx.crop.center", |this, _, cx| { this.crop_selected_pictures(ibis_doc_model::Crop { left: 10_000, top: 10_000, right: 10_000, bottom: 10_000 }, cx); });
    menus.connect("xlsx.crop.reset", |this, _, cx| { this.crop_selected_pictures(ibis_doc_model::Crop::default(), cx); });

    menus.connect("xlsx.picture.color.none", |this, _, cx| { this.set_selected_pictures_duotone(None, cx); });
    menus.connect("xlsx.picture.color.grayscale", |this, _, cx| { this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x000000), ibis_doc_model::Rgb(0xffffff))), cx); });
    menus.connect("xlsx.picture.color.sepia", |this, _, cx| { this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x402000), ibis_doc_model::Rgb(0xffe0c0))), cx); });
    menus.connect("xlsx.picture.color.blue", |this, _, cx| { this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x002060), ibis_doc_model::Rgb(0xd9e1f2))), cx); });
    menus.connect("xlsx.picture.color.green", |this, _, cx| { this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x004010), ibis_doc_model::Rgb(0xe2efda))), cx); });
    menus.connect("xlsx.picture.color.red", |this, _, cx| { this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x600000), ibis_doc_model::Rgb(0xfce4d6))), cx); });
    menus.connect("xlsx.picture.color.orange", |this, _, cx| { this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x803000), ibis_doc_model::Rgb(0xffe6cc))), cx); });
    menus.connect("xlsx.picture.color.purple", |this, _, cx| { this.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x400060), ibis_doc_model::Rgb(0xf2e6ff))), cx); });
    menus.connect("xlsx.picture.remove-background", |this, _, cx| { this.set_selected_pictures_clear(Some(ibis_doc_model::Rgb(0xffffff)), cx); });
    menus.connect("xlsx.picture.color.transparent-white", |this, _, cx| { this.set_selected_pictures_clear(Some(ibis_doc_model::Rgb(0xffffff)), cx); });
    menus.connect("xlsx.picture.color.transparent-black", |this, _, cx| { this.set_selected_pictures_clear(Some(ibis_doc_model::Rgb(0x000000)), cx); });
    menus.connect("xlsx.picture.color.transparent-clear", |this, _, cx| { this.set_selected_pictures_clear(None, cx); });

    menus.connect("xlsx.picture.lum.normal", |this, _, cx| { this.set_selected_pictures_luminance(None, cx); });
    menus.connect("xlsx.picture.lum.bright-p40", |this, _, cx| { this.set_selected_pictures_luminance(Some((40_000, 0)), cx); });
    menus.connect("xlsx.picture.lum.bright-p20", |this, _, cx| { this.set_selected_pictures_luminance(Some((20_000, 0)), cx); });
    menus.connect("xlsx.picture.lum.bright-m20", |this, _, cx| { this.set_selected_pictures_luminance(Some((-20_000, 0)), cx); });
    menus.connect("xlsx.picture.lum.bright-m40", |this, _, cx| { this.set_selected_pictures_luminance(Some((-40_000, 0)), cx); });
    menus.connect("xlsx.picture.lum.contrast-p40", |this, _, cx| { this.set_selected_pictures_luminance(Some((0, 40_000)), cx); });
    menus.connect("xlsx.picture.lum.contrast-p20", |this, _, cx| { this.set_selected_pictures_luminance(Some((0, 20_000)), cx); });
    menus.connect("xlsx.picture.lum.contrast-m20", |this, _, cx| { this.set_selected_pictures_luminance(Some((0, -20_000)), cx); });
    menus.connect("xlsx.picture.lum.contrast-m40", |this, _, cx| { this.set_selected_pictures_luminance(Some((0, -40_000)), cx); });
    menus.connect("xlsx.picture.lum.bright-contrast-high", |this, _, cx| { this.set_selected_pictures_luminance(Some((20_000, 20_000)), cx); });
    menus.connect("xlsx.picture.lum.bright-contrast-low", |this, _, cx| { this.set_selected_pictures_luminance(Some((-20_000, -20_000)), cx); });

    menus.connect("xlsx.picture.corner.none", |this, _, cx| { this.crop_selected_pictures_corner(0, cx); });
    menus.connect("xlsx.picture.corner.small", |this, _, cx| { this.crop_selected_pictures_corner(5_000, cx); });
    menus.connect("xlsx.picture.corner.medium", |this, _, cx| { this.crop_selected_pictures_corner(10_000, cx); });
    menus.connect("xlsx.picture.corner.large", |this, _, cx| { this.crop_selected_pictures_corner(20_000, cx); });
    menus.connect("xlsx.picture.corner.round", |this, _, cx| { this.crop_selected_pictures_corner(50_000, cx); });

    menus.connect("xlsx.picture.reset", |this, _, cx| { this.reset_picture_effects(cx); });

    menus.connect("xlsx.picture.compress", |this, _, cx| {
        this.compress_sheet_pictures(crate::pptx::compress::Resolution::Print, true, true, cx);
    });
    menus.connect("xlsx.picture.compress.print", |this, _, cx| {
        this.compress_sheet_pictures(crate::pptx::compress::Resolution::Print, false, true, cx);
    });
    menus.connect("xlsx.picture.compress.web", |this, _, cx| {
        this.compress_sheet_pictures(crate::pptx::compress::Resolution::Web, false, true, cx);
    });
    menus.connect("xlsx.picture.compress.email", |this, _, cx| {
        this.compress_sheet_pictures(crate::pptx::compress::Resolution::Email, false, true, cx);
    });
    menus.connect("xlsx.picture.compress.delete-crop", |this, _, cx| {
        this.compress_sheet_pictures(crate::pptx::compress::Resolution::Print, true, true, cx);
    });

    menus.connect("xlsx.text.bold", |this, window, cx| { this.format_sheet_text_bold(window, cx); });
    menus.connect("xlsx.text.italic", |this, window, cx| { this.format_sheet_text_italic(window, cx); });
    menus.connect("xlsx.text.underline", |this, window, cx| { this.format_sheet_text_underline(window, cx); });
    menus.connect("xlsx.text.strikethrough", |this, window, cx| { this.format_sheet_text_strikethrough(window, cx); });

    menus.connect("xlsx.text.color-black", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0x000000), w, cx); });
    menus.connect("xlsx.text.color-white", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0xffffff), w, cx); });
    menus.connect("xlsx.text.color-blue", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0x2b579a), w, cx); });
    menus.connect("xlsx.text.color-cyan", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0x17a2b8), w, cx); });
    menus.connect("xlsx.text.color-green", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0x107c41), w, cx); });
    menus.connect("xlsx.text.color-yellow", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0xffc000), w, cx); });
    menus.connect("xlsx.text.color-orange", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0xed7d31), w, cx); });
    menus.connect("xlsx.text.color-red", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0xc00000), w, cx); });
    menus.connect("xlsx.text.color-purple", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0x7030a0), w, cx); });
    menus.connect("xlsx.text.color-gray", |this, w, cx| { this.format_sheet_text_color(ibis_doc_model::Rgb(0x7f7f7f), w, cx); });

    menus.connect("xlsx.text-vertical.top", |this, _, cx| { this.format_sheet_text_vertical(ibis_doc_model::VerticalAnchor::Top, cx); });
    menus.connect("xlsx.text-vertical.middle", |this, _, cx| { this.format_sheet_text_vertical(ibis_doc_model::VerticalAnchor::Middle, cx); });
    menus.connect("xlsx.text-vertical.bottom", |this, _, cx| { this.format_sheet_text_vertical(ibis_doc_model::VerticalAnchor::Bottom, cx); });

    menus.connect("xlsx.text-direction.horizontal", |this, _, cx| { this.format_sheet_text_direction(false, 0, cx); });
    menus.connect("xlsx.text-direction.vertical", |this, _, cx| { this.format_sheet_text_direction(true, 0, cx); });
    menus.connect("xlsx.text-direction.rotate-90", |this, _, cx| { this.format_sheet_text_direction(false, 5_400_000, cx); });
    menus.connect("xlsx.text-direction.rotate-270", |this, _, cx| { this.format_sheet_text_direction(false, 16_200_000, cx); });

    menus.connect("xlsx.text-insets.none", |this, _, cx| { this.format_sheet_text_insets(ibis_doc_model::Insets { left: 0, top: 0, right: 0, bottom: 0 }, cx); });
    menus.connect("xlsx.text-insets.normal", |this, _, cx| { this.format_sheet_text_insets(ibis_doc_model::Insets::default(), cx); });
    menus.connect("xlsx.text-insets.wide", |this, _, cx| { this.format_sheet_text_insets(ibis_doc_model::Insets { left: 182_880, top: 91_440, right: 182_880, bottom: 91_440 }, cx); });

    for &(id, pt) in &TEXT_SIZES {
        menus.connect(id, move |this, w, cx| { this.format_sheet_text_size(pt, w, cx); });
    }

    menus.connect("align.left", |this, w, cx| { this.format_sheet_text_align(ibis_doc_model::TextAlign::Left, w, cx); });
    menus.connect("align.center", |this, w, cx| { this.format_sheet_text_align(ibis_doc_model::TextAlign::Center, w, cx); });
    menus.connect("align.right", |this, w, cx| { this.format_sheet_text_align(ibis_doc_model::TextAlign::Right, w, cx); });
    menus.connect("align.justify", |this, w, cx| { this.format_sheet_text_align(ibis_doc_model::TextAlign::Justify, w, cx); });

    menus.connect("xlsx.placement.move-and-size", |this, _, cx| { this.set_sheet_shape_placement(ibis_doc_model::ObjectPlacement::MoveAndSize, cx); });
    menus.connect("xlsx.placement.move-only", |this, _, cx| { this.set_sheet_shape_placement(ibis_doc_model::ObjectPlacement::MoveOnly, cx); });
    menus.connect("xlsx.placement.free", |this, _, cx| { this.set_sheet_shape_placement(ibis_doc_model::ObjectPlacement::Free, cx); });
    menus.connect("xlsx.placement.print", |this, _, cx| { this.toggle_sheet_shape_print(cx); });
    menus.connect("xlsx.placement.print-on", |this, _, cx| { this.set_sheet_shape_print(true, cx); });
    menus.connect("xlsx.placement.print-off", |this, _, cx| { this.set_sheet_shape_print(false, cx); });

    menus.connect("xlsx.text.wrap", |this, _, cx| { this.format_sheet_text_wrap(cx); });

    for &(preset, item_id, _, _) in super::shape_edit::CHANGE_SHAPES {
        menus.connect(item_id, move |this, _, cx| { this.change_sheet_shape(preset, cx); });
    }

    menus.connect("xlsx.shape-textlink.current-cell", |this, _, cx| { this.link_selected_shape_to_current_cell(cx); });
    menus.connect("xlsx.shape-textlink.clear", |this, _, cx| { this.clear_selected_shape_formula(cx); });

    menus.connect("xlsx.insert.wordart", |this, _, cx| {
        this.insert_sheet_wordart(0, None, cx);
    });
    menus.connect("insert.wordart", |this, _, cx| {
        this.insert_sheet_wordart(0, None, cx);
    });
    for (idx, &(_, insert_id, apply_id)) in XLSX_WORDART_STYLES.iter().enumerate() {
        menus.connect(insert_id, move |this, _, cx| {
            this.insert_sheet_wordart(idx, None, cx);
        });
        menus.connect(apply_id, move |this, window, cx| {
            this.set_selected_sheet_wordart_style(idx, window, cx);
        });
    }
    menus.connect("xlsx.text.warp.none", |this, _, cx| {
        this.set_sheet_text_warp(None, cx);
    });
    for &(prst, id, _) in XLSX_TEXT_WARP_PRESETS {
        menus.connect(id, move |this, _, cx| {
            this.set_sheet_text_warp(Some(prst), cx);
        });
    }
    menus.connect("xlsx.phonetic-toggle", |this, _, cx| {
        this.toggle_show_phonetic(cx);
    });
    menus.connect("xlsx.phonetic-edit", |this, window, cx| {
        this.open_phonetic_panel(window, cx);
    });

    if editor.workbook_structure_locked() {
        fn disable_ids(entries: &mut [MenuEntry], ids: &[&str]) {
            for entry in entries {
                match entry {
                    MenuEntry::Item(item) => {
                        if ids.contains(&item.id) {
                            item.disabled = true;
                        }
                    }
                    MenuEntry::Submenu(sub) => {
                        if ids.contains(&sub.id) {
                            for item in &mut sub.items {
                                if let MenuEntry::Item(i) = item {
                                    i.disabled = true;
                                }
                            }
                        }
                        disable_ids(&mut sub.items, ids);
                    }
                    _ => {}
                }
            }
        }
        for (_, entries) in menus.menus.iter_mut() {
            disable_ids(entries, &[
                "xlsx.insert-sheet",
                "xlsx.delete-sheet",
                "xlsx.rename-sheet",
                "xlsx.hide-sheet",
                "xlsx.unhide-sheet",
                "xlsx.unhide-sheets",
                "xlsx.duplicate-sheet",
                "xlsx.move-sheet-left",
                "xlsx.move-sheet-right",
                "xlsx.tab-colors",
                "xlsx.tab-color",
                "xlsx.tab-color-none",
            ]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::step_decimals;

    #[test]
    fn decimals() {
        assert_eq!(step_decimals("General", true), "0.0");
        assert_eq!(step_decimals("#,##0", true), "#,##0.0");
        assert_eq!(step_decimals("#,##0.00", false), "#,##0.0");
        assert_eq!(step_decimals("0.0%", false), "0%");
        assert_eq!(step_decimals("\"¥\"#,##0;\"¥\"\\-#,##0", true), "\"¥\"#,##0.0;\"¥\"\\-#,##0.0");
    }
}

/// An item of the ¥ button's menu: the accounting number format of `symbol` with `decimals` decimal places, checked
/// when the selected cell (`current`) has it, labelled by its id.
fn currency_item(id: &'static str, symbol: &str, decimals: usize, current: &str) -> MenuEntry {
    let places = if decimals == 0 { String::new() } else { format!(".{}", "0".repeat(decimals)) };
    let dash = if decimals == 0 { "\"-\"".to_string() } else { format!("\"-\"{}", "?".repeat(decimals)) };
    let code = format!("_ \"{symbol}\"* #,##0{places}_ ;_ \"{symbol}\"* \\-#,##0{places}_ ;_ \"{symbol}\"* {dash}_ ;_ @_ ");
    MenuItem::tr(id).checked(current == code).on(move |this, _, cx| this.format_cells(StyleChange::NumberFormat(code.clone()), cx)).into()
}

impl SlideEditor {
    pub(crate) fn toggle_show_formulas(&mut self, cx: &mut Context<Self>) {
        let Some((_, sheet)) = self.current_sheet() else { return };
        let (slide, shown) = (self.current_slide, !sheet.show_formulas);
        self.run_sheet(vec![Command::Sheet(SheetCommand::SetShowFormulas { slide, shown })], cx);
    }

    pub(crate) fn toggle_show_zeros(&mut self, cx: &mut Context<Self>) {
        let Some((_, sheet)) = self.current_sheet() else { return };
        let (slide, shown) = (self.current_slide, !sheet.show_zeros);
        self.run_sheet(vec![Command::Sheet(SheetCommand::SetShowZeros { slide, shown })], cx);
    }

    pub(crate) fn toggle_hide_ink(&mut self, cx: &mut Context<Self>) {
        let Some((_, sheet)) = self.current_sheet() else { return };
        let (slide, hidden) = (self.current_slide, !sheet.hide_ink);
        self.run_sheet(vec![Command::Sheet(SheetCommand::SetHideInk { slide, hidden })], cx);
    }
}

