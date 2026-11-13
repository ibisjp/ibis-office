// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 名前の管理 (issue #102): the list of defined names in the right pane (one row per name: kind, name, value,
//! reference or formula, scope), with search, filter, sort and column widths, and the dialog that adds or edits
//! one name. The list works on the workbook as it stands (modeless); the dialog is modal without dimming and
//! changes the workbook once, when it is confirmed. `names_list.rs` has what needs no screen.

use std::rc::Rc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand};

use super::names_list::{LambdaParam, NameFilter, NameKind, NameProblem, NameRow, ScopeFilter, Sort, SortColumn, classify, lambda_parts, lambda_summary, name_problem, paste_table, plain_formula, shown_rows};
use super::sheet_ui::parse_reference;
use crate::shell::chrome::data_table::DataTable;
use crate::shell::chrome::virtual_table::{TableColumn, TableRow, TableState};
use crate::shell::chrome::{ActionButtonKind, ActionDefinition, Dropdown, DropdownItem, SidebarPanel, TEXT_SECONDARY, icon};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};
use crate::shell::icons::Material;
use crate::shell::palette::rgb;
use crate::ext::*;

/// The columns that can be dragged: the narrowest, the width to begin with and the widest.
const NAME_WIDTH: (f32, f32, f32) = (60., 118., 260.);
const VALUE_COLUMN: (f32, f32, f32) = (50., 110., 300.);
const SCOPE_COLUMN: (f32, f32, f32) = (50., 76., 200.);
/// From this pane width on, the list has the value and scope columns.
const WIDE_PANE: f32 = 480.;
const ROW_HEIGHT: f32 = 32.;
const KIND_WIDTH: f32 = 28.;
const ACTIONS_WIDTH: f32 = 60.;
const ERROR_RED: u32 = 0xd93025;

/// The name manager, open in the right pane.
pub(crate) struct NamesPanel {
    pub(crate) search: Entity<InputState>,
    pub(crate) filter: NameFilter,
    pub(crate) sort: Sort,
    /// The selected name: lower-case name and scope.
    pub(crate) selected: Option<(String, Option<usize>)>,
    /// The list has it while keys work on the list.
    pub(crate) focus: FocusHandle,
    /// The table's scroll position and column widths.
    pub(crate) table: TableState,
    pub(crate) dialog: Option<NameDialog>,
    _search: Subscription,
}

/// The dialog that adds a name or edits one.
pub(crate) struct NameDialog {
    /// The name being edited (lower-case name and scope); `None` for a new one.
    pub(crate) editing: Option<(String, Option<usize>)>,
    pub(crate) name: Entity<InputState>,
    /// The scope (`None`: the workbook); an edited name starts with its own and may be given another.
    pub(crate) scope: Option<usize>,
    pub(crate) formula: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

fn plain_icon_button(id: impl Into<ElementId>, name: &'static str, tooltip: SharedString, color: u32) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(28.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .hover(|s| s.bg(crate::shell::palette::shade(0.27, 0.08)))
        .child(icon(name, 18., rgb(color).into()))
        .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx))
}

#[ibis_office_macros::editor_ext(NamesPanelExt)]
impl SlideEditor {
    fn names_panel_state(&self) -> Option<&NamesPanel> {
        self.sheet_state()?.names.as_ref()
    }

    fn names_panel_mut(&mut self) -> Option<&mut NamesPanel> {
        self.sheet_state_mut()?.names.as_mut()
    }

    /// Open the name manager in the right pane.
    pub fn open_names(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let placeholder = text("xlsx.names.search.placeholder");
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscription = cx.subscribe_in(&search, window, |_, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let focus = cx.focus_handle();
        let state = self.wb_mut();
        state.close_settings();
        state.names = Some(NamesPanel {
            search,
            filter: NameFilter::default(),
            sort: Sort { column: Some((SortColumn::Name, true)) },
            selected: None,
            focus,
            table: TableState::default(),
            dialog: None,
            _search: subscription,
        });
        self.sidebar_open = true;
        cx.notify();
    }

    /// Every name of the workbook that the list shows (not the hidden ones and the print areas), with its value.
    pub(crate) fn names_rows(&self) -> Vec<NameRow> {
        let mut rows = self.names_rows_bare();
        for r in &mut rows {
            r.value = self.name_value_text(r.kind, r.error, &r.formula, r.sheet);
        }
        rows
    }

    /// The names without their values (working a value out can mean calculating a formula, so a long list
    /// works the values out only for the rows in view).
    pub(crate) fn names_rows_bare(&self) -> Vec<NameRow> {
        let Some(wb) = self.presentation().workbook.as_ref() else { return Vec::new() };
        let sheet_names: Vec<String> = wb.sheets.iter().map(|s| s.name.clone()).collect();
        wb.names
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.hidden && !n.name.starts_with("_xlnm."))
            .map(|(index, n)| {
                let formula = plain_formula(&n.formula);
                let (kind, error) = classify(&formula, &sheet_names);
                let summary = lambda_summary(&formula).unwrap_or_else(|| format!("={formula}"));
                let value = String::new();
                let scope = n.sheet.and_then(|i| sheet_names.get(i)).cloned().unwrap_or_default();
                NameRow { index, name: n.name.clone(), sheet: n.sheet, formula, kind, error, summary, value, scope }
            })
            .collect()
    }

    /// The result of a name as text: a constant as it is, a range's cells (as `{a;b;...}` when there are several),
    /// a formula's result worked out in the scope's sheet, a LAMBDA as 関数.
    fn name_value_text(&self, kind: NameKind, error: bool, formula: &str, scope: Option<usize>) -> String {
        match kind {
            NameKind::Lambda => text("xlsx.names.value.function").to_string(),
            NameKind::Constant => formula.trim().to_string(),
            NameKind::Range if error => "#REF!".to_string(),
            NameKind::Range => {
                let Some((slide, range)) = parse_reference(formula).and_then(|r| self.resolve_reference(&r)) else { return "#REF!".to_string() };
                let Some((_, sheet)) = self.presentation().slides.iter().find(|s| s.id == slide).and_then(crate::shell::editor::sheet_of) else { return String::new() };
                let cell = |row: u32, column: u32| sheet.cell(row, column).map(|c| ibis_doc_model::query::value_text(&c.value)).unwrap_or_default();
                if (range.first_row, range.first_column) == (range.last_row, range.last_column) {
                    return cell(range.first_row, range.first_column);
                }
                let mut parts = Vec::new();
                'rows: for row in range.first_row..=range.last_row {
                    let mut line = Vec::new();
                    for column in range.first_column..=range.last_column {
                        line.push(cell(row, column));
                        if parts.len() + line.len() >= 6 {
                            parts.push(line.join(","));
                            break 'rows;
                        }
                    }
                    parts.push(line.join(","));
                }
                let more = range.last_row.saturating_sub(range.first_row) as usize + 1 > parts.len();
                format!("{{{}{}}}", parts.join(";"), if more { ";…" } else { "" })
            }
            NameKind::Formula => {
                if error {
                    return "#REF!".to_string();
                }
                let Some(wb) = self.presentation().workbook.as_ref() else { return String::new() };
                let sheet = scope.or_else(|| wb.sheets.iter().position(|s| s.slide == Some(self.current_slide))).unwrap_or(0);
                let view = ibis_doc_model::sheet_calc::BookView::new(self.presentation());
                let (value, unsupported) = ibis_calc::recalc::evaluate(&view, (sheet, 0, 0), formula);
                if unsupported { String::new() } else { ibis_doc_model::query::value_text(&value) }
            }
        }
    }

    /// The rows the list shows now: search, filter and sort applied.
    pub(crate) fn names_shown(&self, cx: &App) -> Vec<NameRow> {
        let Some(p) = self.names_panel_state() else { return Vec::new() };
        let needle = p.search.read(cx).value().to_string();
        shown_rows(&self.names_rows(), &needle, &p.filter, p.sort)
    }

    /// The same rows in the same order, without the values (for the keys and the clicks).
    fn names_shown_bare(&self, cx: &App) -> Vec<NameRow> {
        let Some(p) = self.names_panel_state() else { return Vec::new() };
        let needle = p.search.read(cx).value().to_string();
        shown_rows(&self.names_rows_bare(), &needle, &p.filter, p.sort)
    }

    /// The right pane, or the dialog over it.
    pub fn names_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        if self.names_panel_state()?.dialog.is_some() {
            return self.names_dialog_panel(cx).map(|p| p.modal_named("names_dialog"));
        }
        self.names_list_panel(cx)
    }

    fn scope_label(&self, scope: Option<usize>) -> String {
        match scope {
            None => text("xlsx.names.scope.book").to_string(),
            Some(i) => self.presentation().workbook.as_ref().and_then(|w| w.sheets.get(i)).map(|s| s.name.clone()).unwrap_or_default(),
        }
    }

    // ----- Actions of the list ------------------------------------------------------------------------------

    fn names_find(&self, key: &(String, Option<usize>)) -> Option<NameRow> {
        self.names_rows_bare().into_iter().find(|r| &r.key() == key)
    }

    /// Select a name, and for a range go to it (`jump`).
    pub(crate) fn names_select(&mut self, key: (String, Option<usize>), jump: bool, window: &mut Window, cx: &mut Context<Self>) {
        let row = self.names_find(&key);
        let at = self.names_shown_bare(cx).iter().position(|r| r.key() == key);
        if let Some(p) = self.names_panel_mut() {
            p.selected = Some(key);
            p.focus.clone().focus(window, cx);
            if let Some(at) = at {
                p.table.scroll_to(at);
            }
        }
        if jump && let Some(row) = row && row.kind == NameKind::Range && !row.error {
            self.go_to_reference(&row.formula, window, cx);
            // The pane keeps the keys: the sheet was only shown.
            if let Some(p) = self.names_panel_mut() {
                p.focus.clone().focus(window, cx);
            }
        }
        cx.notify();
    }

    /// Whether a name can go into a formula being typed now.
    pub(crate) fn names_formula_editing(&self) -> bool {
        self.cell_edit().is_some()
    }

    /// Put a name into the formula being typed (a name of another sheet's scope with its sheet).
    pub(crate) fn names_insert(&mut self, key: (String, Option<usize>), window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.names_find(&key) else { return };
        let text = match row.sheet {
            Some(i) if self.current_workbook_sheet_index() != Some(i) => format!("{}!{}", ibis_calc::formula::quote_sheet(&self.scope_label(Some(i))), row.name),
            _ => row.name.clone(),
        };
        if let Some(p) = self.names_panel_mut() {
            p.selected = Some(key);
        }
        self.use_name_in_formula(&text, window, cx);
    }

    /// A row pressed once or twice: twice puts the name into the formula being typed (it never opens the dialog:
    /// the pencil and F2 do).
    pub(crate) fn names_row_clicked(&mut self, key: (String, Option<usize>), clicks: usize, window: &mut Window, cx: &mut Context<Self>) {
        if clicks >= 2 && self.names_formula_editing() {
            self.names_insert(key, window, cx);
        } else {
            self.names_select(key, clicks < 2, window, cx);
        }
    }

    /// Delete a name from the list: the next row is selected, the status says so, Undo restores it.
    pub(crate) fn names_delete_row(&mut self, key: (String, Option<usize>), window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.names_find(&key) else { return };
        let shown = self.names_shown_bare(cx);
        let at = shown.iter().position(|r| r.key() == key);
        let next = at.and_then(|i| shown.get(i + 1).or_else(|| i.checked_sub(1).and_then(|j| shown.get(j)))).map(|r| r.key());
        self.delete_name(&row.name, row.sheet, cx);
        if let Some(p) = self.names_panel_mut() {
            p.selected = next;
            p.focus.clone().focus(window, cx);
        }
        self.status = Some(format("xlsx.names.toast.deleted", &[("name", &row.name)]).into());
        cx.notify();
    }

    /// Delete every name that has an error, as one undo step.
    pub(crate) fn names_delete_errors(&mut self, cx: &mut Context<Self>) {
        let commands: Vec<Command> = self
            .names_rows_bare()
            .into_iter()
            .filter(|r| r.error)
            .map(|r| Command::Sheet(SheetCommand::SetName { name: r.name, sheet: r.sheet, formula: None }))
            .collect();
        let n = commands.len();
        if n == 0 {
            return;
        }
        if let Err(err) = self.document.execute(commands) {
            log::warn!("deleting the names with errors rejected: {err}");
            return;
        }
        if let Some(p) = self.names_panel_mut() {
            p.selected = None;
        }
        self.status = Some(format("xlsx.names.toast.deleted-errors", &[("n", &n.to_string())]).into());
        cx.notify();
    }

    /// 一覧を貼り付け: the names shown, from the active cell, as one undo step.
    pub(crate) fn names_paste_list(&mut self, cx: &mut Context<Self>) {
        let Some((column, row)) = self.current_cell() else { return };
        let shown = self.names_shown(cx);
        if shown.is_empty() {
            return;
        }
        let headings = [text("xlsx.names.col.name").to_string(), text("xlsx.names.col.ref").to_string(), text("xlsx.names.col.scope").to_string(), text("xlsx.names.col.value").to_string()];
        let table = paste_table(&shown, headings, &text("xlsx.names.scope.book"));
        let mut edits = Vec::new();
        let mut rows_written = 0usize;
        for (r, line) in table.iter().enumerate() {
            let target_row = row + r as u32;
            if target_row >= crate::sheet_ui::MAX_SHEET_ROWS {
                break;
            }
            rows_written += 1;
            for (c, value) in line.iter().enumerate() {
                let target_column = column + c as u32;
                if target_column >= crate::sheet_ui::MAX_SHEET_COLUMNS {
                    break;
                }
                let input = if value.is_empty() { CellInput::Clear } else { CellInput::Value(CellValue::Text(value.clone())) };
                edits.push(CellEdit { row: target_row, column: target_column, input });
            }
        }
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::SetCells { slide, edits })], cx);
        let cell = ibis_doc_model::cell_name(column, row);
        self.status = Some(format("xlsx.names.toast.pasted", &[("n", &rows_written.saturating_sub(1).to_string()), ("cell", &cell)]).into());
        cx.notify();
    }

    /// The names shown, as tab-separated text, onto the clipboard.
    fn names_copy_tsv(&mut self, cx: &mut Context<Self>) {
        let shown = self.names_shown(cx);
        let headings = [text("xlsx.names.col.name").to_string(), text("xlsx.names.col.ref").to_string(), text("xlsx.names.col.scope").to_string(), text("xlsx.names.col.value").to_string()];
        let table = paste_table(&shown, headings, &text("xlsx.names.scope.book"));
        let tsv = table.iter().map(|l| l.iter().map(|c| c.replace(['\t', '\n'], " ")).collect::<Vec<_>>().join("\t")).collect::<Vec<_>>().join("\n");
        cx.write_to_clipboard(ClipboardItem::new_string(tsv));
        self.status = Some(format("xlsx.names.toast.copied", &[("n", &shown.len().to_string())]).into());
        cx.notify();
    }

    /// Keys while the list has the focus. True when the key was used.
    pub(crate) fn names_list_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(p) = self.names_panel_state() else { return false };
        if p.dialog.is_some() || !p.focus.is_focused(window) {
            return false;
        }
        let m = &key.modifiers;
        let command = m.platform || m.control;
        let shown = self.names_shown_bare(cx);
        let current = p.selected.clone();
        let at = current.as_ref().and_then(|k| shown.iter().position(|r| &r.key() == k));
        let page = 8usize;
        let target = match key.key.as_str() {
            "f" if command => {
                let search = p.search.clone();
                search.update(cx, |i, cx| i.focus(window, cx));
                return true;
            }
            "down" => Some(at.map_or(0, |i| (i + 1).min(shown.len().saturating_sub(1)))),
            "up" => Some(at.map_or(0, |i| i.saturating_sub(1))),
            "home" => Some(0),
            "end" => Some(shown.len().saturating_sub(1)),
            "pagedown" => Some(at.map_or(0, |i| (i + page).min(shown.len().saturating_sub(1)))),
            "pageup" => Some(at.map_or(0, |i| i.saturating_sub(page))),
            "enter" => {
                if let Some(k) = current {
                    if self.names_formula_editing() {
                        self.names_insert(k, window, cx);
                    } else {
                        self.names_select(k, true, window, cx);
                    }
                }
                return true;
            }
            "f2" => {
                if let Some(k) = current {
                    self.names_open_dialog(Some(k), window, cx);
                }
                return true;
            }
            "delete" | "backspace" => {
                if let Some(k) = current {
                    self.names_delete_row(k, window, cx);
                }
                return true;
            }
            _ => None,
        };
        if let Some(i) = target {
            if let Some(r) = shown.get(i) {
                self.names_select(r.key(), false, window, cx);
            }
            return true;
        }
        // A letter: the next name that starts with it.
        if !command && !m.alt && let Some(ch) = key.key_char.as_deref().filter(|c| c.chars().count() == 1) {
            let ch = ch.to_lowercase();
            let start = at.map_or(0, |i| i + 1);
            let found = (0..shown.len()).map(|d| (start + d) % shown.len().max(1)).find(|i| shown[*i].name.to_lowercase().starts_with(&ch));
            if let Some(i) = found {
                self.names_select(shown[i].key(), false, window, cx);
            }
            return true;
        }
        false
    }

    // ----- The dialog ----------------------------------------------------------------------------------------

    /// Open the dialog: for a new name (`None`) or for the one `edit` names.
    pub(crate) fn names_open_dialog(&mut self, edit: Option<(String, Option<usize>)>, window: &mut Window, cx: &mut Context<Self>) {
        let row = edit.as_ref().and_then(|k| self.names_find(k));
        if edit.is_some() && row.is_none() {
            return;
        }
        let name_value = row.as_ref().map(|r| r.name.clone()).unwrap_or_default();
        let formula_value = match &row {
            Some(r) => format!("={}", r.formula),
            None => self.selection_formula().map(|f| format!("={f}")).unwrap_or_default(),
        };
        let name = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(name_value, window, cx);
            s
        });
        let formula = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder(text("xlsx.names.dialog.ref"));
            s.set_value(formula_value, window, cx);
            s
        });
        let subs = [&name, &formula]
            .into_iter()
            .map(|e| {
                cx.subscribe_in(e, window, |_, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Change) {
                        cx.notify();
                    }
                })
            })
            .collect();
        let scope = row.as_ref().and_then(|r| r.sheet);
        if let Some(p) = self.names_panel_mut() {
            if let Some(k) = &edit {
                p.selected = Some(k.clone());
            }
            let focus_name = name.clone();
            p.dialog = Some(NameDialog { editing: edit, name, scope, formula, _subscriptions: subs });
            focus_name.update(cx, |i, cx| i.focus(window, cx));
        }
        cx.notify();
    }

    /// Close the dialog without changing anything.
    pub(crate) fn names_close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = self.names_panel_mut() {
            p.dialog = None;
            p.focus.clone().focus(window, cx);
        }
        self.dialog.active = false;
        self.dialog.key = None;
        self.dialog.bounds = None;
        self.dialog.overlay = None;
        window.release_pointer();
        cx.notify();
    }

    /// What is wrong with what the dialog holds, as the message to show; `None` when it can be confirmed.
    fn names_dialog_problem(&self, cx: &App) -> Option<(NameProblem, String)> {
        let d = self.names_panel_state()?.dialog.as_ref()?;
        let name = d.name.read(cx).value().trim().to_string();
        let formula = d.formula.read(cx).value().trim().trim_start_matches('=').trim().to_string();
        let sheet = d.scope;
        let taken = self
            .presentation()
            .workbook
            .as_ref()
            .is_some_and(|w| w.names.iter().any(|n| n.name.eq_ignore_ascii_case(&name) && n.sheet == sheet && d.editing.as_ref().is_none_or(|k| k.0 != n.name.to_lowercase() || k.1 != n.sheet)));
        let problem = name_problem(&name, sheet, taken).or_else(|| formula.is_empty().then_some(NameProblem::ReferenceEmpty))?;
        let message = match &problem {
            NameProblem::Empty => text("xlsx.names.err.empty").to_string(),
            NameProblem::CellReference => text("xlsx.names.err.cell-ref").to_string(),
            NameProblem::FirstCharacter => text("xlsx.names.err.first-char").to_string(),
            NameProblem::Characters => text("xlsx.names.err.chars").to_string(),
            NameProblem::DuplicateBook => text("xlsx.names.err.dupe-book").to_string(),
            NameProblem::DuplicateSheet(i) => format("xlsx.names.err.dupe-sheet", &[("sheet", &self.scope_label(Some(*i)))]),
            NameProblem::ReferenceEmpty => text("xlsx.names.err.ref-empty").to_string(),
        };
        Some((problem, message))
    }

    /// 追加 or 更新: the workbook changes once (one undo step), the dialog closes.
    pub(crate) fn names_confirm_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.names_dialog_problem(cx).is_some() {
            return;
        }
        let Some(d) = self.names_panel_state().and_then(|p| p.dialog.as_ref()) else { return };
        let name = d.name.read(cx).value().trim().to_string();
        let formula = d.formula.read(cx).value().to_string();
        let editing = d.editing.clone();
        let sheet = d.scope;
        let Some(target) = self.name_target_formula(&formula) else { return };
        let mut commands = Vec::new();
        if let Some((old, old_sheet)) = &editing {
            let old_name = self.names_rows_bare().into_iter().find(|r| &r.name.to_lowercase() == old && r.sheet == *old_sheet).map(|r| r.name);
            if let Some(old_name) = old_name {
                // A changed scope moves the name first, so that the formulas that use it follow it.
                if *old_sheet != sheet {
                    commands.push(Command::Sheet(SheetCommand::MoveName { name: old_name.clone(), from: *old_sheet, to: sheet }));
                }
                if old_name != name {
                    commands.push(Command::Sheet(SheetCommand::SetName { name: old_name, sheet, formula: None }));
                }
            }
        }
        commands.push(Command::Sheet(SheetCommand::SetName { name: name.clone(), sheet, formula: Some(target) }));
        if let Err(err) = self.document.execute(commands) {
            log::warn!("defining a name rejected: {err}");
            return;
        }
        let id = if editing.is_some() { "xlsx.names.toast.updated" } else { "xlsx.names.toast.added" };
        self.status = Some(format(id, &[("name", &name)]).into());
        if let Some(p) = self.names_panel_mut() {
            p.selected = Some((name.to_lowercase(), sheet));
        }
        self.names_close_dialog(window, cx);
    }

    // ----- Drawing --------------------------------------------------------------------------------------------

    fn names_dialog_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let problem = self.names_dialog_problem(cx);
        let p = self.names_panel_state()?;
        let d = p.dialog.as_ref()?;
        let editing = d.editing.is_some();
        let (name_input, formula_input) = (d.name.clone(), d.formula.clone());
        let new_scope = d.scope;
        let formula_text = formula_input.read(cx).value().to_string();
        let name_text = name_input.read(cx).value().to_string();
        let sheet_names: Vec<String> = self.presentation().workbook.as_ref().map(|w| w.sheets.iter().filter(|s| s.slide.is_some()).map(|s| s.name.clone()).collect()).unwrap_or_default();
        let all_sheets: Vec<String> = self.presentation().workbook.as_ref().map(|w| w.sheets.iter().map(|s| s.name.clone()).collect()).unwrap_or_default();
        let (kind, kind_error) = classify(&formula_text, &all_sheets);
        let owner = cx.weak_entity();

        let name_error = problem.as_ref().filter(|(p, _)| !matches!(p, NameProblem::ReferenceEmpty)).map(|(_, m)| m.clone());
        let ref_error = problem.as_ref().filter(|(p, _)| matches!(p, NameProblem::ReferenceEmpty)).map(|(_, m)| m.clone());
        let label = |id: &'static str| div().text_sm().font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT_SECONDARY)).child(text(id));
        let red = |m: String| div().text_sm().text_color(rgb(ERROR_RED)).child(SharedString::from(m));

        // The scope: chosen for a new name and changeable for an edited one (the workbook or one of the sheets).
        let scope_row = {
            let mut items = vec![DropdownItem::new(text("xlsx.names.scope.book")).checked(new_scope.is_none()).on_select(Rc::new(|e: &mut SlideEditor, _, cx| {
                if let Some(d) = e.names_panel_mut().and_then(|p| p.dialog.as_mut()) {
                    d.scope = None;
                }
                cx.notify();
            }))];
            let workbook_indexes: Vec<usize> = self.presentation().workbook.as_ref().map(|w| w.sheets.iter().enumerate().filter(|(_, s)| s.slide.is_some()).map(|(i, _)| i).collect()).unwrap_or_default();
            for (i, sheet) in workbook_indexes.into_iter().zip(sheet_names.iter()) {
                items.push(DropdownItem::new(sheet.clone()).checked(new_scope == Some(i)).on_select(Rc::new(move |e: &mut SlideEditor, _, cx| {
                    if let Some(d) = e.names_panel_mut().and_then(|p| p.dialog.as_mut()) {
                        d.scope = Some(i);
                    }
                    cx.notify();
                })));
            }
            let current = self.scope_label(new_scope);
            v_flex()
                .gap_1()
                .child(label("xlsx.names.dialog.scope"))
                .child(Dropdown::new("names-dialog-scope", current, items, &owner).fill_width())
        };

        // LAMBDA: its parameters, as chips.
        let params: Vec<LambdaParam> = if kind == NameKind::Lambda { lambda_parts(&formula_text).map(|(p, _)| p).unwrap_or_default() } else { Vec::new() };
        let kind_label = text(kind.label_id());
        let current_value = editing.then(|| {
            let key = d.editing.clone().unwrap();
            self.names_find(&key).map(|r| self.name_value_text(r.kind, r.error, &r.formula, r.sheet)).unwrap_or_default()
        });
        let length = formula_text.chars().count();

        let mut body = v_flex()
            .gap_3()
            .child(v_flex().gap_1().child(label("xlsx.names.dialog.name")).child(crate::shell::chrome::text_field(&name_input).full_width()).children(name_error.map(red)))
            .child(scope_row)
            .child(
                v_flex()
                    .gap_1()
                    .child(h_flex().justify_between().child(label("xlsx.names.dialog.ref")).child(div().text_sm().text_color(rgb(TEXT_SECONDARY)).child(kind_label)))
                    .child(super::range_field::local_range_field(&formula_input, "names-dialog-ref", false, cx))
                    .when(length > 120, |d| d.child(div().text_xs().text_color(rgb(TEXT_SECONDARY)).child(format("xlsx.names.dialog.length", &[("n", &length.to_string())]))))
                    .children(ref_error.map(red))
                    .when(kind_error && !editing_blank(&formula_text), |d| d.child(red(text("xlsx.names.dialog.error-ref").to_string()))),
            );
        if !params.is_empty() {
            body = body.child(
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .items_center()
                    .child(label("xlsx.names.dialog.lambda-args"))
                    .children(params.into_iter().map(|p| {
                        h_flex()
                            .gap_1()
                            .px_2()
                            .h(px(24.))
                            .items_center()
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(crate::shell::chrome::BORDER))
                            .text_sm()
                            .child(div().font_weight(FontWeight::MEDIUM).child(SharedString::from(p.name)))
                            .child(div().text_color(rgb(TEXT_SECONDARY)).child(text(if p.optional { "xlsx.names.dialog.arg-optional" } else { "xlsx.names.dialog.arg-required" })))
                    })),
            );
        }
        if let Some(value) = current_value {
            body = body.child(
                v_flex().gap_1().child(label("xlsx.names.dialog.current-value")).child(
                    div().min_h(px(32.)).px_2().py_1().rounded(px(4.)).bg(rgb(0xf3f5f8)).text_sm().text_color(rgb(TEXT_SECONDARY)).child(SharedString::from(value)),
                ),
            );
        }
        let ready = problem.is_none() && !name_text.trim().is_empty();
        let primary = ActionDefinition::new("names-dialog-ok", text(if editing { "xlsx.names.action.update" } else { "xlsx.names.action.add" }), ActionButtonKind::Primary)
            .disabled(!ready)
            .on_click(cx.listener(|e, _, w, cx| e.names_confirm_dialog(w, cx)));
        let cancel = ActionDefinition::new("names-dialog-cancel", text("xlsx.names.action.cancel"), ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.names_close_dialog(w, cx)));
        let footer = crate::shell::chrome::action_bar().action(primary).action(cancel);
        let title = text(if editing { "xlsx.names.dialog.title-edit" } else { "xlsx.names.dialog.title-new" });
        Some(SidebarPanel::with_footer(title, body.into_any_element(), footer))
    }

    /// The list in the right pane.
    pub(crate) fn names_list_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let all = self.names_rows_bare();
        let (search, filter, sort, selected, focus, table) = {
            let p = self.names_panel_state()?;
            (p.search.clone(), p.filter.clone(), p.sort, p.selected.clone(), p.focus.clone(), p.table.clone())
        };
        let needle = search.read(cx).value().to_string();
        let shown = shown_rows(&all, &needle, &filter, sort);
        let shown_count = shown.len();
        let wide = self.sidebar_width >= WIDE_PANE;
        let owner = cx.weak_entity();
        let error_count = all.iter().filter(|r| r.error).count();
        let sheets: Vec<(usize, String)> = self.presentation().workbook.as_ref().map(|w| w.sheets.iter().enumerate().filter(|(_, s)| s.slide.is_some()).map(|(i, s)| (i, s.name.clone())).collect()).unwrap_or_default();

        // The filter menu.
        let toggle = |f: Rc<dyn Fn(&mut NameFilter)>| -> crate::shell::chrome::Handler {
            Rc::new(move |e: &mut SlideEditor, _, cx| {
                if let Some(p) = e.names_panel_mut() {
                    f(&mut p.filter);
                }
                cx.notify();
            })
        };
        let mut items: Vec<DropdownItem> = vec![
            DropdownItem::new(text("xlsx.names.filter.scope-all")).checked(filter.scope == ScopeFilter::All).on_select(toggle(Rc::new(|f| f.scope = ScopeFilter::All))),
            DropdownItem::new(text("xlsx.names.filter.scope-book")).checked(filter.scope == ScopeFilter::Workbook).on_select(toggle(Rc::new(|f| f.scope = ScopeFilter::Workbook))),
        ];
        for (i, name) in &sheets {
            let i = *i;
            items.push(DropdownItem::new(format("xlsx.names.filter.scope-sheet", &[("sheet", name)])).checked(filter.scope == ScopeFilter::Sheet(i)).on_select(toggle(Rc::new(move |f| f.scope = ScopeFilter::Sheet(i)))));
        }
        for kind in NameKind::ALL {
            let on = filter.kinds.contains(&kind);
            items.push(DropdownItem::new(text(kind.label_id())).checked(on).on_select(toggle(Rc::new(move |f| {
                if let Some(at) = f.kinds.iter().position(|k| *k == kind) {
                    f.kinds.remove(at);
                } else {
                    f.kinds.push(kind);
                }
            }))));
        }
        items.push(DropdownItem::new(text("xlsx.names.filter.errors-only")).checked(filter.errors_only).on_select(toggle(Rc::new(|f| f.errors_only = !f.errors_only))));
        items.push(DropdownItem::new(text("xlsx.names.filter.hide-helpers")).checked(filter.hide_helpers).on_select(toggle(Rc::new(|f| f.hide_helpers = !f.hide_helpers))));
        items.push(DropdownItem::new(text("xlsx.names.filter.clear")).icon("filter_alt_off").on_select(toggle(Rc::new(|f| *f = NameFilter::default()))));
        let filter_button = crate::shell::chrome::action_button("names-filter-button")
            .w(px(28.))
            .h(px(28.))
            .px_0()
            .icon(Material(if filter.active() > 0 { "filter_alt" } else { "filter_list" }))
            .tooltip(text("xlsx.names.filter.button"));
        let filter_menu = Dropdown::with_button("names-filter", filter_button, items.into_iter().map(|i| i.menu().into()).collect(), false, 0, &owner).checkmarks(true).searchable(false);

        // The "more" menu.
        let cell = self.current_cell().map(|(c, r)| ibis_doc_model::cell_name(c, r)).unwrap_or_default();
        let more_items: Vec<DropdownItem> = vec![
            DropdownItem::new(format("xlsx.names.menu.paste", &[("n", &shown.len().to_string()), ("cell", &cell)])).icon("content_paste").on_select(Rc::new(|e: &mut SlideEditor, _, cx| e.names_paste_list(cx))),
            DropdownItem::new(text("xlsx.names.menu.copy")).icon("content_copy").on_select(Rc::new(|e: &mut SlideEditor, _, cx| e.names_copy_tsv(cx))),
            DropdownItem::new(text("xlsx.names.menu.create-from-selection")).icon("table_chart").on_select(Rc::new(|e: &mut SlideEditor, _, cx| {
                e.create_names_from_selection(true, false, cx);
            })),
            DropdownItem::new(text("xlsx.names.menu.create-left")).icon("view_column").on_select(Rc::new(|e: &mut SlideEditor, _, cx| {
                e.create_names_from_selection(false, true, cx);
            })),
            DropdownItem::new(text("xlsx.names.menu.create-both")).icon("grid_view").on_select(Rc::new(|e: &mut SlideEditor, _, cx| {
                e.create_names_from_selection(true, true, cx);
            })),
            {
                let mut item = DropdownItem::new(format("xlsx.names.menu.delete-errors", &[("n", &error_count.to_string())])).icon("delete_sweep").on_select(Rc::new(|e: &mut SlideEditor, _, cx| e.names_delete_errors(cx)));
                item.disabled = error_count == 0;
                item
            },
        ];
        let more_button = crate::shell::chrome::action_button("names-more-button").w(px(28.)).h(px(28.)).px_0().icon(Material("more_vert")).tooltip(text("xlsx.names.action.more"));
        let more_menu = Dropdown::with_button("names-more", more_button, more_items.into_iter().map(|i| i.menu().into()).collect(), false, 0, &owner).checkmarks(false).searchable(false);

        // The count and the tags of the filter.
        let count = if shown.len() == all.len() { format("xlsx.names.count", &[("total", &all.len().to_string())]) } else { format("xlsx.names.count-filtered", &[("total", &all.len().to_string()), ("shown", &shown.len().to_string())]) };
        let top = h_flex()
            .w_full()
            .gap_1()
            .items_center()
            .child(div().flex_1().min_w_0().child(crate::shell::chrome::text_field(&search).full_width()))
            .child(filter_menu)
            .child(more_menu);
        let counts = h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .child(div().id("names-count").text_sm().text_color(rgb(TEXT_SECONDARY)).child(count))
            .child(crate::shell::chrome::action_button("names-add").label(text("xlsx.names.action.add")).on_click(cx.listener(|e, _, w, cx| e.names_open_dialog(None, w, cx))));

        // The table: the heading row stays, only the rows in view are made.
        let pressed = |column: SortColumn| -> crate::shell::chrome::Handler {
            Rc::new(move |e: &mut SlideEditor, _, cx| {
                if let Some(p) = e.names_panel_mut() {
                    p.sort = p.sort.pressed(column);
                }
                cx.notify();
            })
        };
        let heading = |id: &'static str, label: &'static str, column: SortColumn| {
            let arrow = sort.direction(column).map(|up| icon(if up { "arrow_upward" } else { "arrow_downward" }, 14., rgb(TEXT_SECONDARY).into()));
            let handler = pressed(column);
            h_flex()
                .id(id)
                .h_full()
                .max_w_full()
                .gap_1()
                .items_center()
                .overflow_hidden()
                .whitespace_nowrap()
                .cursor_pointer()
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(TEXT_SECONDARY))
                .child(text(label))
                .children(arrow)
                .on_click(cx.listener(move |e, _, w, cx| handler(e, w, cx)))
                .into_any_element()
        };
        let kind_heading = div()
            .id("names-sort-kind")
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new(text("xlsx.names.col.kind")).build(window, cx))
            .child(icon(match sort.direction(SortColumn::Kind) { Some(true) => "arrow_upward", Some(false) => "arrow_downward", None => "sort" }, 14., rgb(TEXT_SECONDARY).into()))
            .on_click({
                let handler = pressed(SortColumn::Kind);
                cx.listener(move |e, _, w, cx| handler(e, w, cx))
            })
            .into_any_element();
        let mut columns = vec![TableColumn::fixed("kind", KIND_WIDTH), TableColumn::resizable("name", NAME_WIDTH.0, NAME_WIDTH.1, NAME_WIDTH.2)];
        let mut header = vec![kind_heading, heading("names-sort-name", "xlsx.names.col.name", SortColumn::Name)];
        if wide {
            columns.push(TableColumn::resizable("value", VALUE_COLUMN.0, VALUE_COLUMN.1, VALUE_COLUMN.2));
            header.push(heading("names-sort-value", "xlsx.names.col.value", SortColumn::Value));
        }
        columns.push(TableColumn::flex("ref"));
        header.push(heading("names-sort-ref", "xlsx.names.col.ref", SortColumn::Reference));
        if wide {
            columns.push(TableColumn::resizable("scope", SCOPE_COLUMN.0, SCOPE_COLUMN.1, SCOPE_COLUMN.2));
            header.push(heading("names-sort-scope", "xlsx.names.col.scope", SortColumn::Scope));
        }
        columns.push(TableColumn::fixed("actions", ACTIONS_WIDTH));
        header.push(div().into_any_element());

        // The values are worked out for the rows around the view only (all of them when the list is sorted by value).
        let mut shown = shown;
        if sort.column.is_some_and(|(c, _)| c == SortColumn::Value) {
            let values: std::collections::HashMap<_, _> = self.names_rows().into_iter().map(|r| (r.key(), r.value)).collect();
            for r in &mut shown {
                r.value = values.get(&r.key()).cloned().unwrap_or_default();
            }
        } else {
            let window = table.window(ROW_HEIGHT, shown.len());
            for r in &mut shown[window] {
                r.value = self.name_value_text(r.kind, r.error, &r.formula, r.sheet);
            }
        }
        let shown = Rc::new(shown);
        let editor = cx.entity().downgrade();
        let build = {
            let shown = shown.clone();
            let editor = editor.clone();
            move |range: std::ops::Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
                range
                    .filter_map(|i| shown.get(i).map(|r| (i, r)))
                    .map(|(i, r)| {
                        let key = r.key();
                        let is_selected = selected.as_ref() == Some(&key);
                        let kind_icon = if r.error { icon("error", 18., rgb(ERROR_RED).into()) } else { icon(r.kind.icon(), 18., rgb(0x444746).into()) };
                        let kind_tip = if r.error { text("xlsx.names.kind.error") } else { text(r.kind.label_id()) };
                        let (edit_key, delete_key, click_key) = (key.clone(), key.clone(), key.clone());
                        let full = SharedString::from(format!("={}", r.formula));
                        let scope_tag = (!wide && !r.scope.is_empty()).then(|| div().flex_none().px_1().rounded_full().bg(rgb(0xeceff1)).text_xs().text_color(rgb(TEXT_SECONDARY)).child(SharedString::from(r.scope.clone())));
                        let mut cells: Vec<AnyElement> = vec![
                            div()
                                .id(("names-row-kind", i))
                                .w_full()
                                .flex()
                                .justify_center()
                                .child(kind_icon)
                                .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(kind_tip.clone()).build(window, cx))
                                .into_any_element(),
                            h_flex().w_full().gap_1().items_center().pr_1().child(div().min_w_0().truncate().text_sm().child(SharedString::from(r.name.clone()))).children(scope_tag).into_any_element(),
                        ];
                        if wide {
                            cells.push(div().w_full().pr_1().truncate().text_xs().text_color(rgb(TEXT_SECONDARY)).child(SharedString::from(r.value.clone())).into_any_element());
                        }
                        cells.push(
                            div()
                                .id(("names-row-ref", i))
                                .w_full()
                                .pr_1()
                                .truncate()
                                .text_xs()
                                .when(r.kind == NameKind::Lambda, |d| d.text_color(rgb(0x0b57d0)))
                                .child(SharedString::from(r.summary.clone()))
                                .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(full.clone()).build(window, cx))
                                .into_any_element(),
                        );
                        if wide {
                            cells.push(div().w_full().pr_1().truncate().text_xs().text_color(rgb(TEXT_SECONDARY)).child(if r.scope.is_empty() { text("xlsx.names.scope.book") } else { SharedString::from(r.scope.clone()) }).into_any_element());
                        }
                        let (edit_editor, delete_editor, click_editor) = (editor.clone(), editor.clone(), editor.clone());
                        cells.push(
                            h_flex()
                                .child(plain_icon_button(("names-edit", i), "edit", format("xlsx.names.action.edit-row", &[("name", &r.name)]).into(), 0x1f1f1f).on_click(move |_, w, cx| {
                                    let key = edit_key.clone();
                                    let _ = edit_editor.update(cx, |e, cx| e.names_open_dialog(Some(key), w, cx));
                                    cx.stop_propagation();
                                }))
                                .child(plain_icon_button(("names-delete", i), "delete", format("xlsx.names.action.delete-row", &[("name", &r.name)]).into(), 0x1f1f1f).on_click(move |_, w, cx| {
                                    let key = delete_key.clone();
                                    let _ = delete_editor.update(cx, |e, cx| e.names_delete_row(key, w, cx));
                                    cx.stop_propagation();
                                }))
                                .into_any_element(),
                        );
                        let on_click: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)> = Rc::new(move |ev, w, cx| {
                            let key = click_key.clone();
                            let clicks = ev.click_count();
                            let _ = click_editor.update(cx, |e, cx| e.names_row_clicked(key, clicks, w, cx));
                        });
                        TableRow { cells, selected: is_selected, on_click: Some(on_click), ..TableRow::default() }
                    })
                    .collect()
            }
        };
        let empty = div().w_full().py_4().flex().justify_center().text_sm().text_color(rgb(TEXT_SECONDARY)).child(text(if all.is_empty() { "xlsx.names.empty.none" } else { "xlsx.names.empty.nomatch" }));
        let table_element = DataTable::virtualized("names-table", columns, header, table, shown_count, build, cx).row_height(ROW_HEIGHT).empty(empty.into_any_element());
        let body = crate::shell::chrome::panel_form()
            .p_0()
            .size_full()
            .child(top)
            .child(counts)
            .child(div().id("names-list").track_focus(&focus).key_context("NamesList").w_full().flex_1().min_h_0().child(table_element));
        Some(SidebarPanel { title: text("xlsx.names.title"), body: body.into_any_element() }.fill())
    }
}

fn editing_blank(formula: &str) -> bool {
    formula.trim().trim_start_matches('=').trim().is_empty()
}
