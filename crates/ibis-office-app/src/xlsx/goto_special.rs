// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 編集 > 条件を選択してジャンプ (Go To Special): the cells of a kind
//! selected (comments, constants or formulas of some types, blanks, the
//! current region, the last cell, the visible cells, conditional formats,
//! data validation), within the selected range or, when one cell is
//! selected, the sheet's used range. Excel selects them as several ranges:
//! the first is the selection, the others are kept with it
//! (`SlideEditor::selected_ranges`) until the selection changes, and
//! formatting, Delete and ⌃Enter take them all.
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellRange, CellValue, Sheet, SlideId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Special {
    Comments,
    Constants,
    Formulas,
    Blanks,
    CurrentRegion,
    Objects,
    RowDifferences,
    ColumnDifferences,
    Precedents,
    Dependents,
    LastCell,
    Visible,
    Conditional,
    Validation,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Scope {
    #[default]
    All,
    Same,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TraceLevel {
    #[default]
    DirectOnly,
    AllLevels,
}

/// Which values constants and formulas take: numbers, text, logical values, errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Types {
    pub(crate) numbers: bool,
    pub(crate) text: bool,
    pub(crate) logicals: bool,
    pub(crate) errors: bool,
}

impl Types {
    pub(crate) const ALL: Types = Types { numbers: true, text: true, logicals: true, errors: true };

    fn takes(&self, value: &CellValue) -> bool {
        match value {
            CellValue::Number(_) => self.numbers,
            CellValue::Text(_) => self.text,
            CellValue::Bool(_) => self.logicals,
            CellValue::Error(_) => self.errors,
            CellValue::Empty => false,
        }
    }
}

pub(crate) struct GotoSpecialPanel {
    pub(crate) kind: Special,
    pub(crate) types: Types,
    pub(crate) scope: Scope,
    pub(crate) trace_level: TraceLevel,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

/// The ranges selected with the selection (条件を選択してジャンプ): kept
/// while the selection is the one they were selected with.
pub(crate) struct ExtraRanges {
    slide: SlideId,
    /// The selected cell and the range's other corner then.
    cell: (u32, u32),
    end: Option<(u32, u32)>,
    pub(crate) ranges: Vec<CellRange>,
}

/// The cells' places merged into ranges: runs along each row, then runs of
/// rows with the same column runs.
pub(crate) fn to_ranges(mut cells: Vec<(u32, u32)>) -> Vec<CellRange> {
    cells.sort();
    cells.dedup();
    // Runs along rows.
    let mut runs: Vec<(u32, u32, u32)> = Vec::new();
    for (row, column) in cells {
        match runs.last_mut() {
            Some((r, _, last)) if *r == row && *last + 1 == column => *last = column,
            _ => runs.push((row, column, column)),
        }
    }
    // Rows below with the same run join it.
    let mut out: Vec<CellRange> = Vec::new();
    for (row, first, last) in runs {
        match out.iter_mut().find(|r| r.last_row + 1 == row && r.first_column == first && r.last_column == last) {
            Some(r) => r.last_row = row,
            None => out.push(CellRange { first_row: row, last_row: row, first_column: first, last_column: last }),
        }
    }
    out.sort_by_key(|r| (r.first_row, r.first_column));
    out
}

/// Subtract rectangle `b` from rectangle `a`.
/// Returns 0 to 4 disjoint rectangles that represent `a \ b`.
pub(crate) fn subtract_rect(a: CellRange, b: CellRange) -> Vec<CellRange> {
    let r0 = a.first_row.max(b.first_row);
    let r1 = a.last_row.min(b.last_row);
    let c0 = a.first_column.max(b.first_column);
    let c1 = a.last_column.min(b.last_column);

    // No intersection
    if r0 > r1 || c0 > c1 {
        return vec![a];
    }

    let mut out = Vec::new();
    // 1. Top slice (full width of a)
    if a.first_row < r0 {
        out.push(CellRange {
            first_row: a.first_row,
            last_row: r0 - 1,
            first_column: a.first_column,
            last_column: a.last_column,
        });
    }
    // 2. Bottom slice (full width of a)
    if a.last_row > r1 {
        out.push(CellRange {
            first_row: r1 + 1,
            last_row: a.last_row,
            first_column: a.first_column,
            last_column: a.last_column,
        });
    }
    // 3. Left slice (height restricted to intersection rows r0..=r1)
    if a.first_column < c0 {
        out.push(CellRange {
            first_row: r0,
            last_row: r1,
            first_column: a.first_column,
            last_column: c0 - 1,
        });
    }
    // 4. Right slice (height restricted to intersection rows r0..=r1)
    if a.last_column > c1 {
        out.push(CellRange {
            first_row: r0,
            last_row: r1,
            first_column: c1 + 1,
            last_column: a.last_column,
        });
    }
    out
}

/// Subtract rectangle `b` from a list of rectangles `ranges`.
pub(crate) fn subtract_ranges(ranges: &[CellRange], b: CellRange) -> Vec<CellRange> {
    let mut out = Vec::new();
    for a in ranges {
        out.extend(subtract_rect(*a, b));
    }
    let mut clean = Vec::new();
    for r in out {
        if r.first_row <= r.last_row && r.first_column <= r.last_column && !clean.contains(&r) {
            clean.push(r);
        }
    }
    clean
}

fn cells_equal(a: Option<&ibis_doc_model::SheetCell>, b: Option<&ibis_doc_model::SheetCell>) -> bool {
    let empty_a = a.is_none_or(|c| c.value.is_empty() && c.formula.is_none());
    let empty_b = b.is_none_or(|c| c.value.is_empty() && c.formula.is_none());
    if empty_a && empty_b {
        return true;
    }
    if empty_a != empty_b {
        return false;
    }
    let ca = a.unwrap();
    let cb = b.unwrap();
    ca.formula == cb.formula && ca.value == cb.value
}

/// The cells of `kind` in `area` (the current region and the last cell
/// do not look at it: `selected` is the selected cell (column, row)).
pub(crate) fn find(
    sheet: &Sheet,
    kind: Special,
    types: Types,
    scope: Scope,
    trace_level: TraceLevel,
    area: CellRange,
    selected: (u32, u32),
    sheet_name: Option<&str>,
) -> Vec<CellRange> {
    let inside = |r: u32, c: u32| (area.first_row..=area.last_row).contains(&r) && (area.first_column..=area.last_column).contains(&c);
    let filled = |r: u32, c: u32| sheet.cell(r, c).is_some_and(|x| !x.value.is_empty() || x.formula.is_some());
    let clip = |r: CellRange| -> Option<CellRange> {
        let out = CellRange {
            first_row: r.first_row.max(area.first_row),
            last_row: r.last_row.min(area.last_row),
            first_column: r.first_column.max(area.first_column),
            last_column: r.last_column.min(area.last_column),
        };
        (out.first_row <= out.last_row && out.first_column <= out.last_column).then_some(out)
    };
    match kind {
        Special::Comments => to_ranges(sheet.comments.iter().filter(|c| inside(c.row, c.column)).map(|c| (c.row, c.column)).collect()),
        Special::Constants | Special::Formulas => {
            let formulas = kind == Special::Formulas;
            to_ranges(sheet.cells.iter().filter(|c| inside(c.row, c.column) && c.formula.is_some() == formulas && types.takes(&c.value)).map(|c| (c.row, c.column)).collect())
        }
        Special::Blanks => {
            let mut cells = Vec::new();
            for r in area.first_row..=area.last_row {
                for c in area.first_column..=area.last_column {
                    if !filled(r, c) && sheet.merge_at(r, c).is_none_or(|m| (m.first_row, m.first_column) == (r, c)) {
                        cells.push((r, c));
                    }
                }
            }
            to_ranges(cells)
        }
        Special::CurrentRegion => {
            let (column, row) = selected;
            let mut r = CellRange { first_row: row, first_column: column, last_row: row, last_column: column };
            loop {
                let mut grown = r;
                let rows = r.first_row.saturating_sub(1)..=r.last_row + 1;
                let columns = r.first_column.saturating_sub(1)..=r.last_column + 1;
                if r.first_row > 0 && columns.clone().any(|c| filled(r.first_row - 1, c)) {
                    grown.first_row -= 1;
                }
                if columns.clone().any(|c| filled(r.last_row + 1, c)) {
                    grown.last_row += 1;
                }
                if r.first_column > 0 && rows.clone().any(|x| filled(x, r.first_column - 1)) {
                    grown.first_column -= 1;
                }
                if rows.clone().any(|x| filled(x, r.last_column + 1)) {
                    grown.last_column += 1;
                }
                if grown == r {
                    break vec![r];
                }
                r = grown;
            }
        }
        Special::LastCell => {
            let used = used_range(sheet);
            vec![CellRange { first_row: used.last_row, last_row: used.last_row, first_column: used.last_column, last_column: used.last_column }]
        }
        Special::Visible => {
            let hidden_row = |r: u32| sheet.rows.get(r as usize).is_some_and(|&h| h == 0);
            let hidden_column = |c: u32| sheet.columns.get(c as usize).is_some_and(|&w| w == 0);
            let mut cells = Vec::new();
            for r in (area.first_row..=area.last_row).filter(|&r| !hidden_row(r)) {
                for c in (area.first_column..=area.last_column).filter(|&c| !hidden_column(c)) {
                    cells.push((r, c));
                }
            }
            to_ranges(cells)
        }
        Special::Conditional => {
            if scope == Scope::Same {
                let (col, row) = selected;
                let target = sheet.conditional.iter().find(|c| {
                    c.ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&col))
                });
                match target {
                    Some(target) => merge_overlapping(
                        sheet.conditional.iter()
                            .filter(|c| c.rules == target.rules)
                            .flat_map(|c| c.ranges.iter().copied())
                            .filter_map(clip)
                            .collect(),
                    ),
                    None => Vec::new(),
                }
            } else {
                merge_overlapping(sheet.conditional.iter().flat_map(|f| f.ranges.iter().copied()).filter_map(clip).collect())
            }
        }
        Special::Validation => {
            if scope == Scope::Same {
                let (col, row) = selected;
                let target = sheet.validations.iter().find(|v| {
                    v.ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&col))
                });
                match target {
                    Some(target) => merge_overlapping(
                        sheet.validations.iter()
                            .filter(|v| v.kind == target.kind && v.operator == target.operator && v.formula1 == target.formula1 && v.formula2 == target.formula2 && v.allow_blank == target.allow_blank)
                            .flat_map(|v| v.ranges.iter().copied())
                            .filter_map(clip)
                            .collect(),
                    ),
                    None => Vec::new(),
                }
            } else {
                merge_overlapping(sheet.validations.iter().flat_map(|v| v.ranges.iter().copied()).filter_map(clip).collect())
            }
        }
        Special::RowDifferences => {
            let (selected_col, _) = selected;
            let base_col = if (area.first_column..=area.last_column).contains(&selected_col) {
                selected_col
            } else {
                area.first_column
            };
            if area.first_column == area.last_column {
                return Vec::new();
            }
            let mut cells = Vec::new();
            for r in area.first_row..=area.last_row {
                let base = sheet.cell(r, base_col);
                for c in area.first_column..=area.last_column {
                    if c == base_col {
                        continue;
                    }
                    let target = sheet.cell(r, c);
                    if !cells_equal(base, target) {
                        cells.push((r, c));
                    }
                }
            }
            to_ranges(cells)
        }
        Special::ColumnDifferences => {
            let (_, selected_row) = selected;
            let base_row = if (area.first_row..=area.last_row).contains(&selected_row) {
                selected_row
            } else {
                area.first_row
            };
            if area.first_row == area.last_row {
                return Vec::new();
            }
            let mut cells = Vec::new();
            for c in area.first_column..=area.last_column {
                let base = sheet.cell(base_row, c);
                for r in area.first_row..=area.last_row {
                    if r == base_row {
                        continue;
                    }
                    let target = sheet.cell(r, c);
                    if !cells_equal(base, target) {
                        cells.push((r, c));
                    }
                }
            }
            to_ranges(cells)
        }
        Special::Precedents => {
            let (col, row) = selected;
            let mut queue = std::collections::VecDeque::new();
            let mut visited = std::collections::HashSet::new();
            let mut found = Vec::new();

            if let Some(c) = sheet.cell(row, col) {
                if let Some(f) = &c.formula {
                    if let Ok(expr) = ibis_calc::formula::parse(f.trim_start_matches('=')) {
                        let mut refs = Vec::new();
                        crate::xlsx::trace::collect_references(&expr, &mut refs);
                        for r in refs {
                            let is_same = r.sheet.is_none() || r.sheet.as_deref() == sheet_name;
                            if !is_same {
                                continue;
                            }
                            for r_row in r.min_row..=r.max_row {
                                for r_col in r.min_col..=r.max_col {
                                    if visited.insert((r_col, r_row)) {
                                        found.push((r_row, r_col));
                                        if trace_level == TraceLevel::AllLevels {
                                            queue.push_back((r_col, r_row));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            while let Some((q_col, q_row)) = queue.pop_front() {
                if let Some(c) = sheet.cell(q_row, q_col) {
                    if let Some(f) = &c.formula {
                        if let Ok(expr) = ibis_calc::formula::parse(f.trim_start_matches('=')) {
                            let mut refs = Vec::new();
                            crate::xlsx::trace::collect_references(&expr, &mut refs);
                            for r in refs {
                                let is_same = r.sheet.is_none() || r.sheet.as_deref() == sheet_name;
                                if !is_same {
                                    continue;
                                }
                                for r_row in r.min_row..=r.max_row {
                                    for r_col in r.min_col..=r.max_col {
                                        if visited.insert((r_col, r_row)) {
                                            found.push((r_row, r_col));
                                            queue.push_back((r_col, r_row));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            to_ranges(found)
        }
        Special::Dependents => {
            let mut formula_refs = Vec::new();
            for cell in &sheet.cells {
                if let Some(f) = &cell.formula {
                    if let Ok(expr) = ibis_calc::formula::parse(f.trim_start_matches('=')) {
                        let mut refs = Vec::new();
                        crate::xlsx::trace::collect_references(&expr, &mut refs);
                        formula_refs.push((cell.column, cell.row, refs));
                    }
                }
            }

            let mut queue = std::collections::VecDeque::new();
            queue.push_back(selected);
            let mut visited = std::collections::HashSet::new();
            visited.insert(selected);
            let mut found = Vec::new();

            while let Some((t_col, t_row)) = queue.pop_front() {
                for (f_col, f_row, refs) in &formula_refs {
                    if visited.contains(&(*f_col, *f_row)) {
                        continue;
                    }
                    let refers = refs.iter().any(|r| {
                        let is_same = r.sheet.is_none() || r.sheet.as_deref() == sheet_name;
                        is_same && r.min_col <= t_col && t_col <= r.max_col && r.min_row <= t_row && t_row <= r.max_row
                    });
                    if refers {
                        visited.insert((*f_col, *f_row));
                        found.push((*f_row, *f_col));
                        if trace_level == TraceLevel::AllLevels {
                            queue.push_back((*f_col, *f_row));
                        }
                    }
                }
            }
            to_ranges(found)
        }
        Special::Objects => Vec::new(),
    }
}

/// Ranges that may overlap, as the cells they cover.
fn merge_overlapping(ranges: Vec<CellRange>) -> Vec<CellRange> {
    let mut cells = Vec::new();
    for r in ranges {
        for row in r.first_row..=r.last_row {
            for column in r.first_column..=r.last_column {
                cells.push((row, column));
            }
        }
    }
    to_ranges(cells)
}

/// From A1 to the last row and column holding something (a value, a
/// formula or a format).
pub(crate) fn used_range(sheet: &Sheet) -> CellRange {
    let last_row = sheet.cells.iter().map(|c| c.row).max().unwrap_or(0);
    let last_column = sheet.cells.iter().map(|c| c.column).max().unwrap_or(0);
    CellRange { first_row: 0, last_row, first_column: 0, last_column }
}

impl SlideEditor {
    /// The selected ranges: the selection, and those selected with it by
    /// 条件を選択してジャンプ while it is the same.
    pub(crate) fn selected_ranges(&self) -> Vec<CellRange> {
        let Some(range) = self.selected_range() else { return Vec::new() };
        let mut out = vec![range];
        if let (Some(extra), Some(state)) = (self.sheet_state().and_then(|s| s.panels.extra_ranges.as_ref()), self.sheet_state())
            && extra.slide == self.current_slide
            && state.cells.get(&self.current_slide).copied() == Some(extra.cell)
            && state.cell_ends.get(&self.current_slide).copied() == extra.end
        {
            out.extend(extra.ranges.iter().copied());
        }
        out
    }

    /// ⌘-click (Excel's Ctrl+クリック): the ranges selected so far kept with
    /// the new selection, which has just been made.
    pub(crate) fn keep_with_selection(&mut self, kept: Vec<CellRange>) {
        let slide = self.current_slide;
        let Some(cell) = self.current_cell() else { return };
        let state = self.wb_mut();
        let end = state.cell_ends.get(&slide).copied();
        state.panels.extra_ranges = Some(ExtraRanges { slide, cell, end, ranges: kept });
    }

    /// Set the selection to exactly the given ranges.
    pub(crate) fn set_selected_ranges(&mut self, ranges: &[CellRange]) {
        let slide = self.current_slide;
        if ranges.is_empty() {
            return;
        }
        let first = ranges[0];
        let state = self.wb_mut();
        state.cells.insert(slide, (first.first_column, first.first_row));
        if first.first_column == first.last_column && first.first_row == first.last_row {
            state.cell_ends.remove(&slide);
        } else {
            state.cell_ends.insert(slide, (first.last_column, first.last_row));
        }
        if ranges.len() > 1 {
            let extra = ranges[1..].to_vec();
            self.keep_with_selection(extra);
        } else {
            self.wb_mut().panels.extra_ranges = None;
        }
    }

    /// Check if a cell is covered by the current selection.
    pub(crate) fn is_cell_selected(&self, column: u32, row: u32) -> bool {
        self.selected_ranges().iter().any(|r| {
            (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&column)
        })
    }

    /// Deselect a cell from the current selection.
    /// Returns true if the cell was selected and has been deselected.
    #[allow(dead_code)]
    pub(crate) fn deselect_cell(&mut self, column: u32, row: u32) -> bool {
        let current = self.selected_ranges();
        let in_sel = current.iter().any(|r| {
            (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&column)
        });
        if !in_sel {
            return false;
        }
        let deselect_rect = CellRange { first_row: row, last_row: row, first_column: column, last_column: column };
        let remaining = subtract_ranges(&current, deselect_rect);
        if remaining.is_empty() {
            return false;
        }
        self.set_selected_ranges(&remaining);
        true
    }

    /// Deselect a range of cells from a base set of ranges.
    pub(crate) fn deselect_range(&mut self, base: &[CellRange], deselect_rect: CellRange) {
        let remaining = subtract_ranges(base, deselect_rect);
        if !remaining.is_empty() {
            self.set_selected_ranges(&remaining);
        }
    }

    /// The selection's corner moved by dragging: the kept ranges stay with it
    /// (`was_kept`: whether they were with it before the move).
    pub(crate) fn extra_ranges_follow(&mut self, was_kept: bool) {
        if !was_kept {
            return;
        }
        let slide = self.current_slide;
        let state = self.wb_mut();
        let end = state.cell_ends.get(&slide).copied();
        if let Some(extra) = state.panels.extra_ranges.as_mut() {
            extra.end = end;
        }
    }

    /// ⌃Enter: what is being typed put into every selected cell, a formula
    /// moved with each (as Excel fills a selection); the selection stays.
    pub(crate) fn fill_selection_with_entry(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(crate::xlsx::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(edit) = self.cell_edit().filter(|e| e.slide == self.current_slide).map(|e| (e.column, e.row, e.text.clone())) else { return };
        let ranges = self.selected_ranges();
        if ranges.iter().all(|r| r.first_row == r.last_row && r.first_column == r.last_column) && ranges.len() == 1 {
            // One cell: as Enter, without moving.
            self.commit_cell_edit(cx);
            return;
        }
        self.cancel_cell_edit(cx);
        let (column, row, text) = edit;
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let typed = (!text.is_empty()).then(|| ibis_calc::input::parse_input(&text, date1904));
        let mut edits = Vec::new();
        for r in &ranges {
            for to_row in r.first_row..=r.last_row {
                for to_column in r.first_column..=r.last_column {
                    let input = match &typed {
                        None => ibis_doc_model::CellInput::Clear,
                        Some(ibis_calc::input::Typed::Formula(f)) => ibis_doc_model::CellInput::Formula(ibis_calc::formula::shift(f, to_row as i64 - row as i64, to_column as i64 - column as i64)),
                        Some(ibis_calc::input::Typed::Value(v, _)) => ibis_doc_model::CellInput::Value(v.clone()),
                    };
                    edits.push(ibis_doc_model::CellEdit { row: to_row, column: to_column, input });
                }
            }
        }
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetCells { slide, edits })]) {
            self.status = Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into());
        }
        cx.notify();
    }

    pub(crate) fn open_goto_special_panel(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let panel = GotoSpecialPanel {
            kind: Special::Blanks,
            types: Types::ALL,
            scope: Scope::All,
            trace_level: TraceLevel::DirectOnly,
            message: String::new(),
            ui: Default::default(),
        };
        let state = self.wb_mut();
        state.custom_format = None;
        state.page_setup = false;
        state.names = None;
        state.validation = None;
        state.find = None;
        state.link = None;
        state.conditional_edit = None;
        state.sort_edit = None;
        state.close_settings();
        state.panels.goto_special = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// Select the cells or objects the panel asks for.
    /// Returns true when the cells or objects were selected (the dialog then closes, as Excel's OK does).
    pub(crate) fn apply_goto_special(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.goto_special.as_ref()) else { return false };
        let (kind, types, scope, trace_level) = (p.kind, p.types, p.scope, p.trace_level);

        if kind == Special::Objects {
            let object_ids: Vec<ibis_doc_model::ShapeId> = self.slide().shapes.iter()
                .filter(|s| !matches!(s.kind, ibis_doc_model::ShapeKind::Sheet(_)))
                .map(|s| s.id)
                .collect();
            if object_ids.is_empty() {
                if let Some(p) = self.wb_mut().panels.goto_special.as_mut() {
                    p.message = crate::shell::i18n::text("_xlsx.no-matching-objects-found").into();
                }
                cx.notify();
                return false;
            }
            let count = object_ids.len() as u32;
            self.set_selection(object_ids);
            if let Some(state) = self.sheet_state_mut() {
                state.panels.extra_ranges = None;
            }
            if let Some(p) = self.wb_mut().panels.goto_special.as_mut() {
                p.message = crate::shell::i18n::format("goto-special.done", &[("count", &count)]);
            }
            cx.notify();
            return true;
        }

        let (Some((_, sheet)), Some(selection), Some(cell)) = (self.current_sheet(), self.selected_range(), self.current_cell()) else { return false };
        let single = sheet.merge_at(cell.1, cell.0).unwrap_or(CellRange { first_row: cell.1, last_row: cell.1, first_column: cell.0, last_column: cell.0 });
        let area = if selection == single { used_range(sheet) } else { selection };
        let sheet_name = self.presentation().workbook.as_ref().and_then(|wb| {
            wb.sheets.iter().find(|s| s.slide == Some(self.slide().id)).map(|s| s.name.clone())
        });
        let found = find(sheet, kind, types, scope, trace_level, area, cell, sheet_name.as_deref());
        let Some(first) = found.first().copied() else {
            if let Some(p) = self.wb_mut().panels.goto_special.as_mut() {
                p.message = crate::shell::i18n::text("_xlsx.no-matching-cells-were-found").into();
            }
            cx.notify();
                return false;
        };
        let slide = self.current_slide;
        let count: u32 = found.iter().map(|r| (r.last_row - r.first_row + 1) * (r.last_column - r.first_column + 1)).sum();
        let state = self.wb_mut();
        state.cells.insert(slide, (first.first_column, first.first_row));
        let end = ((first.last_column, first.last_row) != (first.first_column, first.first_row)).then_some((first.last_column, first.last_row));
        match end {
            Some(e) => {
                state.cell_ends.insert(slide, e);
            }
            None => {
                state.cell_ends.remove(&slide);
            }
        }
        state.panels.extra_ranges = Some(ExtraRanges { slide, cell: (first.first_column, first.first_row), end, ranges: found[1..].to_vec() });
        if let Some(p) = state.panels.goto_special.as_mut() {
            p.message = crate::shell::i18n::format("goto-special.done", &[("count", &count)]);
        }
        cx.notify();
        true
    }

    /// ホーム > 編集 > 検索と選択: 1 クリックで指定種類のセルまたはオブジェクトを選択する.
    pub(crate) fn select_special_kind(&mut self, kind: Special, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        if kind == Special::Objects {
            let object_ids: Vec<ibis_doc_model::ShapeId> = self
                .slide()
                .shapes
                .iter()
                .filter(|s| !matches!(s.kind, ibis_doc_model::ShapeKind::Sheet(_)))
                .map(|s| s.id)
                .collect();
            if object_ids.is_empty() {
                self.status = Some(crate::shell::i18n::text("_xlsx.no-matching-objects-found"));
                return cx.notify();
            }
            let count = object_ids.len() as u32;
            self.set_selection(object_ids);
            if let Some(state) = self.sheet_state_mut() {
                state.panels.extra_ranges = None;
            }
            self.status = Some(crate::shell::i18n::format("goto-special.done", &[("count", &count)]).into());
            return cx.notify();
        }

        let (Some((_, sheet)), Some(selection), Some(cell)) = (self.current_sheet(), self.selected_range(), self.current_cell()) else { return };
        let single = sheet.merge_at(cell.1, cell.0).unwrap_or(CellRange {
            first_row: cell.1,
            last_row: cell.1,
            first_column: cell.0,
            last_column: cell.0,
        });
        let area = if selection == single { used_range(sheet) } else { selection };
        let sheet_name = self.presentation().workbook.as_ref().and_then(|wb| {
            wb.sheets.iter().find(|s| s.slide == Some(self.slide().id)).map(|s| s.name.clone())
        });
        let found = find(sheet, kind, Types::ALL, Scope::All, TraceLevel::DirectOnly, area, cell, sheet_name.as_deref());
        let Some(first) = found.first().copied() else {
            self.status = Some(crate::shell::i18n::text("_xlsx.no-matching-cells-were-found"));
            return cx.notify();
        };
        let slide = self.current_slide;
        let count: u32 = found.iter().map(|r| (r.last_row - r.first_row + 1) * (r.last_column - r.first_column + 1)).sum();
        let state = self.wb_mut();
        state.cells.insert(slide, (first.first_column, first.first_row));
        let end = ((first.last_column, first.last_row) != (first.first_column, first.first_row)).then_some((first.last_column, first.last_row));
        match end {
            Some(e) => {
                state.cell_ends.insert(slide, e);
            }
            None => {
                state.cell_ends.remove(&slide);
            }
        }
        state.panels.extra_ranges = Some(ExtraRanges {
            slide,
            cell: (first.first_column, first.first_row),
            end,
            ranges: found[1..].to_vec(),
        });
        self.status = Some(crate::shell::i18n::format("goto-special.done", &[("count", &count)]).into());
        cx.notify();
    }

    pub(crate) fn goto_special_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.goto_special.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let mut body = v_flex().gap_1().px(px(18.)).py(px(8.)).p_0().child(crate::shell::i18n::text("fix.goto-special.select"));
        let mut options = crate::shell::chrome::radio_group();
        let kinds = [
            (Special::Comments, "comments", "docx.find.scope.comments"),
            (Special::Constants, "constants", "fix.goto-special.constants"),
            (Special::Formulas, "formulas", "_xlsx.formulas"),
            (Special::Blanks, "blanks", "fix.goto-special.blanks"),
            (Special::CurrentRegion, "current-region", "fix.goto-special.current-region"),
            (Special::Objects, "objects", "_xlsx.objects"),
            (Special::RowDifferences, "row-differences", "_xlsx.row-differences"),
            (Special::ColumnDifferences, "column-differences", "_xlsx.column-differences"),
            (Special::Precedents, "precedents", "_xlsx.precedents"),
            (Special::Dependents, "dependents", "_xlsx.dependents"),
            (Special::LastCell, "last-cell", "fix.goto-special.last-cell"),
            (Special::Visible, "visible", "fix.goto-special.visible-cells-only"),
            (Special::Conditional, "conditional", "_xlsx.conditional-formats"),
            (Special::Validation, "validation", "_shell.data-validation"),
        ];
        for (kind, id, label) in kinds {
            let on = p.kind == kind;
            options = options.child(mark(
                id,
                crate::shell::chrome::radio_button(
                    id,
                    on,
                    false,
                    crate::shell::i18n::text(label),
                    cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.goto_special.as_mut() {
                            p.kind = kind;
                        }
                        cx.notify();
                    }),
                )
                .into_any_element(),
            ));
            // The types under 定数 and 数式, as in Excel's dialog.
            if on && (kind == Special::Formulas || kind == Special::Constants) {
                let types = p.types;
                type Flip = fn(&mut Types);
                let toggle = |id: &'static str, checked: bool, name: &'static str, flip: Flip| {
                    mark(
                        id,
                        crate::xlsx::toggle::toggle_row(
                            id,
                            checked,
                            crate::shell::i18n::text(name),
                            cx.listener(move |this, _, _, cx| {
                                if let Some(p) = this.wb_mut().panels.goto_special.as_mut() {
                                    flip(&mut p.types);
                                }
                                cx.notify();
                            }),
                        )
                        .into_any_element(),
                    )
                };
                options = options.child(
                    v_flex()
                        .pl_4()
                        .gap_0p5()
                        .child(toggle("numbers", types.numbers, "_xlsx.numbers", |t| t.numbers = !t.numbers))
                        .child(toggle("text", types.text, "fix.goto-special.text", |t| t.text = !t.text))
                        .child(toggle("logicals", types.logicals, "fix.goto-special.logicals", |t| t.logicals = !t.logicals))
                        .child(toggle("errors", types.errors, "fix.goto-special.errors", |t| t.errors = !t.errors)),
                );
            }
            // Scope radio options for Conditional formats and Data validation
            if on && (kind == Special::Conditional || kind == Special::Validation) {
                let scope = p.scope;
                let same_label = if kind == Special::Conditional { "_xlsx.same-conditional-format" } else { "_xlsx.same-data-validation" };
                options = options.child(
                    v_flex()
                        .pl_4()
                        .gap_0p5()
                        .child(mark(
                            "scope-all",
                            crate::shell::chrome::radio_button(
                                "scope-all",
                                scope == Scope::All,
                                false,
                                crate::shell::i18n::text("_xlsx.all"),
                                cx.listener(|this, _, _, cx| {
                                    if let Some(p) = this.wb_mut().panels.goto_special.as_mut() {
                                        p.scope = Scope::All;
                                    }
                                    cx.notify();
                                }),
                            )
                            .into_any_element(),
                        ))
                        .child(mark(
                            "scope-same",
                            crate::shell::chrome::radio_button(
                                "scope-same",
                                scope == Scope::Same,
                                false,
                                crate::shell::i18n::text(same_label),
                                cx.listener(|this, _, _, cx| {
                                    if let Some(p) = this.wb_mut().panels.goto_special.as_mut() {
                                        p.scope = Scope::Same;
                                    }
                                    cx.notify();
                                }),
                            )
                            .into_any_element(),
                        )),
                );
            }
            // Trace level radio options for Precedents and Dependents
            if on && (kind == Special::Precedents || kind == Special::Dependents) {
                let trace_level = p.trace_level;
                options = options.child(
                    v_flex()
                        .pl_4()
                        .gap_0p5()
                        .child(mark(
                            "trace-direct",
                            crate::shell::chrome::radio_button(
                                "trace-direct",
                                trace_level == TraceLevel::DirectOnly,
                                false,
                                crate::shell::i18n::text("_xlsx.direct-only"),
                                cx.listener(|this, _, _, cx| {
                                    if let Some(p) = this.wb_mut().panels.goto_special.as_mut() {
                                        p.trace_level = TraceLevel::DirectOnly;
                                    }
                                    cx.notify();
                                }),
                            )
                            .into_any_element(),
                        ))
                        .child(mark(
                            "trace-all",
                            crate::shell::chrome::radio_button(
                                "trace-all",
                                trace_level == TraceLevel::AllLevels,
                                false,
                                crate::shell::i18n::text("_xlsx.all-levels"),
                                cx.listener(|this, _, _, cx| {
                                    if let Some(p) = this.wb_mut().panels.goto_special.as_mut() {
                                        p.trace_level = TraceLevel::AllLevels;
                                    }
                                    cx.notify();
                                }),
                            )
                            .into_any_element(),
                        )),
                );
            }
        }
        body = body.child(options);
        if !p.message.is_empty() {
            body = body.children(crate::shell::chrome::panel_message(&p.message));
        }
        let footer = mark_action("apply", crate::shell::chrome::ActionDefinition::new("goto-special-apply", "", crate::shell::chrome::ActionButtonKind::Primary)
                .label(crate::shell::i18n::text("fix.goto-special.select"))
                .on_click(cx.listener(|this, _, window, cx| {
                    if this.apply_goto_special(cx) {
                        this.close_panel_dialog(window, cx);
                    }
                })));
        Some(SidebarPanel::with_footer(
            "fix.goto-special.go-special",
            body.into_any_element(),
            crate::shell::chrome::action_bar().action(footer).action(crate::shell::chrome::dialog_cancel_definition(cx)),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{find, to_ranges, Scope, Special, TraceLevel, Types};
    use ibis_doc_model::{CellKind, CellRange, CellValue, Sheet, SheetCell, Validation};

    fn make_cell(row: u32, column: u32, value: CellValue) -> SheetCell {
        SheetCell {
            row,
            column,
            value,
            text: String::new(),
            kind: CellKind::Empty,
            formula: None,
            style: 0,
            phonetic: None,
            runs: None,
        }
    }

    fn make_formula_cell(row: u32, column: u32, formula: &str) -> SheetCell {
        SheetCell {
            row,
            column,
            value: CellValue::Empty,
            text: String::new(),
            kind: CellKind::Empty,
            formula: Some(formula.to_string()),
            style: 0,
            phonetic: None,
            runs: None,
        }
    }

    fn check_cells_into_ranges() {
        let r = |a, b, c, d| CellRange { first_row: a, last_row: b, first_column: c, last_column: d };
        assert_eq!(to_ranges(vec![(0, 0), (0, 1), (1, 0), (1, 1), (3, 2)]), vec![r(0, 1, 0, 1), r(3, 3, 2, 2)]);
        assert_eq!(to_ranges(vec![(0, 0), (0, 2), (1, 0)]), vec![r(0, 1, 0, 0), r(0, 0, 2, 2)]);
        assert!(to_ranges(Vec::new()).is_empty());
    }

    fn check_row_differences() {
        let mut sheet = Sheet::default();
        sheet.cells.push(make_cell(0, 0, CellValue::Number(10.0)));
        sheet.cells.push(make_cell(0, 1, CellValue::Number(10.0)));
        sheet.cells.push(make_cell(0, 2, CellValue::Number(20.0)));
        sheet.cells.push(make_cell(1, 0, CellValue::Number(30.0)));
        sheet.cells.push(make_cell(1, 1, CellValue::Number(40.0)));
        sheet.cells.push(make_cell(1, 2, CellValue::Number(30.0)));

        let area = CellRange { first_row: 0, last_row: 1, first_column: 0, last_column: 2 };
        let found = find(&sheet, Special::RowDifferences, Types::ALL, Scope::All, TraceLevel::DirectOnly, area, (0, 0), None);
        assert_eq!(found, vec![
            CellRange { first_row: 0, last_row: 0, first_column: 2, last_column: 2 },
            CellRange { first_row: 1, last_row: 1, first_column: 1, last_column: 1 },
        ]);
    }

    fn check_column_differences() {
        let mut sheet2 = Sheet::default();
        sheet2.cells.push(make_cell(0, 0, CellValue::Number(10.0)));
        sheet2.cells.push(make_cell(0, 1, CellValue::Number(30.0)));
        sheet2.cells.push(make_cell(1, 0, CellValue::Number(10.0)));
        sheet2.cells.push(make_cell(1, 1, CellValue::Number(40.0)));
        sheet2.cells.push(make_cell(2, 0, CellValue::Number(20.0)));
        sheet2.cells.push(make_cell(2, 1, CellValue::Number(30.0)));

        let area2 = CellRange { first_row: 0, last_row: 2, first_column: 0, last_column: 1 };
        let found2 = find(&sheet2, Special::ColumnDifferences, Types::ALL, Scope::All, TraceLevel::DirectOnly, area2, (0, 0), None);
        assert_eq!(found2, vec![
            CellRange { first_row: 1, last_row: 1, first_column: 1, last_column: 1 },
            CellRange { first_row: 2, last_row: 2, first_column: 0, last_column: 0 },
        ]);
    }

    fn check_validation_same_and_all() {
        let mut sheet3 = Sheet::default();
        sheet3.validations.push(Validation {
            ranges: vec![CellRange { first_row: 0, last_row: 2, first_column: 0, last_column: 0 }],
            kind: "whole".into(),
            operator: "greaterThan".into(),
            formula1: Some("0".into()),
            ..Default::default()
        });
        sheet3.validations.push(Validation {
            ranges: vec![CellRange { first_row: 0, last_row: 2, first_column: 1, last_column: 1 }],
            kind: "list".into(),
            formula1: Some("\"A,B,C\"".into()),
            ..Default::default()
        });

        let area3 = CellRange { first_row: 0, last_row: 2, first_column: 0, last_column: 1 };
        let all = find(&sheet3, Special::Validation, Types::ALL, Scope::All, TraceLevel::DirectOnly, area3, (0, 0), None);
        assert_eq!(all, vec![CellRange { first_row: 0, last_row: 2, first_column: 0, last_column: 1 }]);

        let same_a = find(&sheet3, Special::Validation, Types::ALL, Scope::Same, TraceLevel::DirectOnly, area3, (0, 0), None);
        assert_eq!(same_a, vec![CellRange { first_row: 0, last_row: 2, first_column: 0, last_column: 0 }]);

        let same_b = find(&sheet3, Special::Validation, Types::ALL, Scope::Same, TraceLevel::DirectOnly, area3, (1, 0), None);
        assert_eq!(same_b, vec![CellRange { first_row: 0, last_row: 2, first_column: 1, last_column: 1 }]);
    }

    fn check_precedents_and_dependents() {
        let mut sheet4 = Sheet::default();
        sheet4.cells.push(make_cell(0, 0, CellValue::Number(10.0)));
        sheet4.cells.push(make_formula_cell(0, 1, "=A1*2"));
        sheet4.cells.push(make_formula_cell(0, 2, "=B1+5"));

        let area4 = CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 2 };

        let prec_direct = find(&sheet4, Special::Precedents, Types::ALL, Scope::All, TraceLevel::DirectOnly, area4, (1, 0), None);
        assert_eq!(prec_direct, vec![CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 0 }]);

        let prec_c_direct = find(&sheet4, Special::Precedents, Types::ALL, Scope::All, TraceLevel::DirectOnly, area4, (2, 0), None);
        assert_eq!(prec_c_direct, vec![CellRange { first_row: 0, last_row: 0, first_column: 1, last_column: 1 }]);
        let prec_c_all = find(&sheet4, Special::Precedents, Types::ALL, Scope::All, TraceLevel::AllLevels, area4, (2, 0), None);
        assert_eq!(prec_c_all, vec![
            CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 1 },
        ]);

        let dep_direct = find(&sheet4, Special::Dependents, Types::ALL, Scope::All, TraceLevel::DirectOnly, area4, (0, 0), None);
        assert_eq!(dep_direct, vec![CellRange { first_row: 0, last_row: 0, first_column: 1, last_column: 1 }]);
        let dep_all = find(&sheet4, Special::Dependents, Types::ALL, Scope::All, TraceLevel::AllLevels, area4, (0, 0), None);
        assert_eq!(dep_all, vec![
            CellRange { first_row: 0, last_row: 0, first_column: 1, last_column: 2 },
        ]);
    }

    #[test]
    fn test_goto_special() {
        check_cells_into_ranges();
        check_row_differences();
        check_column_differences();
        check_validation_same_and_all();
        check_precedents_and_dependents();
    }
}
