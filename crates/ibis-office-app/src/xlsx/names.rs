// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 名前付き範囲と名前付き数式: a name typed in the name box for the selected range
//! defines it (as in Excel), the name box and ジャンプ go to a name, and
//! the sidebar's 名前の管理 lists the names, changes what one stands for
//! (to an arbitrary formula, constant, or the selected range), edits names,
//! and deletes names.

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Command, SheetCommand};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The name manager, open in the sidebar.
pub(crate) struct NamesPanel {
    /// The field for a new or edited name.
    pub(crate) new_name: Entity<InputState>,
    /// The field for the formula, constant, or reference.
    pub(crate) new_formula: Entity<InputState>,
    /// Scope for a new or edited name (`None`: workbook scope, `Some(idx)`: sheet scope).
    pub(crate) new_sheet: Option<usize>,
    /// Index of the name currently being edited.
    pub(crate) editing: Option<usize>,
}

/// Whether `text` can be a defined name (Excel's rules, roughly): a letter,
/// `_` or `\\` first, then no spaces or operators, and not a cell
/// reference (`A1`, `R1C1`) or TRUE/FALSE.
pub(crate) fn valid_name(text: &str) -> bool {
    let Some(first) = text.chars().next() else { return false };
    if !(first.is_alphabetic() || first == '_' || first == '\\') || text.chars().count() > 255 {
        return false;
    }
    if !text.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '\\' | '?')) {
        return false;
    }
    let upper = text.to_ascii_uppercase();
    if upper == "TRUE" || upper == "FALSE" || upper == "R" || upper == "C" {
        return false;
    }
    // A cell reference: letters then digits within the sheet.
    let letters = upper.chars().take_while(|c| c.is_ascii_alphabetic()).count();
    let digits = &upper[letters..];
    if letters > 0 && letters <= 3 && !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // R1C1 forms.
    let r1c1 = upper.starts_with('R') && upper[1..].chars().all(|c| c.is_ascii_digit() || c == 'C');
    !r1c1
}

/// Turn an arbitrary label (e.g. from a header cell) into a valid defined name
/// according to Excel's naming rules:
/// - Replaces spaces and invalid punctuation with `_`
/// - Prefixes with `_` if starting with a non-alphabetic character or conflicting with a cell reference
pub(crate) fn sanitize_name(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut out = String::new();
    let first = text.chars().next()?;
    if !first.is_alphabetic() && first != '_' && first != '\\' {
        out.push('_');
    }
    for c in text.chars() {
        if c.is_alphanumeric() || matches!(c, '_' | '.' | '\\' | '?') {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    if out.chars().count() > 255 {
        out = out.chars().take(255).collect();
    }
    if valid_name(&out) {
        Some(out)
    } else {
        let prefixed = format!("_{out}");
        if valid_name(&prefixed) {
            Some(prefixed)
        } else {
            None
        }
    }
}

impl SlideEditor {
    /// The selected range as a name's formula: `Sheet!$A$1:$B$3`.
    fn selection_formula(&self) -> Option<String> {
        let range = self.selected_range()?;
        let name = self.slide().name.clone()?;
        let a = |c: u32, r: u32| format!("${}${}", ibis_doc_model::cell_name(c, 0).trim_end_matches('1'), r + 1);
        let area = if (range.first_row, range.first_column) == (range.last_row, range.last_column) {
            a(range.first_column, range.first_row)
        } else {
            format!("{}:{}", a(range.first_column, range.first_row), a(range.last_column, range.last_row))
        };
        Some(format!("{}!{area}", ibis_calc::formula::quote_sheet(&name)))
    }

    /// Return the workbook sheet index of the currently active slide.
    pub(crate) fn current_workbook_sheet_index(&self) -> Option<usize> {
        let slide_id = self.slide().id;
        self.presentation().workbook.as_ref().and_then(|w| w.sheets.iter().position(|s| s.slide == Some(slide_id)))
    }

    /// Define `name` with a formula or constant (without leading `=`) in workbook scope (`sheet = None`), or fallback to the selected range.
    pub(crate) fn set_named_formula(&mut self, name: &str, formula: &str, cx: &mut Context<Self>) -> bool {
        self.set_named_formula_with_scope(name, None, formula, cx)
    }

    /// Define `name` with an explicit scope (`None` for workbook, `Some(sheet_index)` for sheet-local) and formula/constant.
    pub(crate) fn set_named_formula_with_scope(&mut self, name: &str, sheet: Option<usize>, formula: &str, cx: &mut Context<Self>) -> bool {
        if !valid_name(name) {
            self.status = Some(crate::shell::i18n::format("_xlsx.this-name-cannot-be-used-name", &[("name", &name)]).into());
            cx.notify();
            return false;
        }
        let clean = formula.trim().trim_start_matches('=').trim().to_string();
        let current_sheet = self.slide().name.clone().unwrap_or_else(|| "Sheet1".to_string());
        let target_formula = if clean.is_empty() {
            self.selection_formula().unwrap_or_default()
        } else {
            let qualified = ibis_calc::formula::rewrite_references(&clean, |area| {
                if area.sheet.is_none() {
                    let mut a = area.clone();
                    a.sheet = Some(ibis_calc::formula::SheetRef { first: current_sheet.clone(), last: None, book: None, path: None });
                    a.start.row_fixed = true;
                    a.start.column_fixed = true;
                    a.end.row_fixed = true;
                    a.end.column_fixed = true;
                    Some(a)
                } else {
                    Some(area.clone())
                }
            });
            ibis_calc::formula::strip_formula_spaces(&qualified)
        };
        if target_formula.is_empty() {
            self.status = Some(crate::shell::i18n::text("_xlsx.enter-a-reference-range-or-formula").into());
            cx.notify();
            return false;
        }
        let ok = self.document.execute(vec![Command::Sheet(SheetCommand::SetName { name: name.to_string(), sheet, formula: Some(target_formula) })]).is_ok();
        cx.notify();
        ok
    }

    /// Define `name` (for the whole workbook) as the selected range, or
    /// change what it stands for to it.
    pub(crate) fn name_selection(&mut self, name: &str, cx: &mut Context<Self>) -> bool {
        self.name_selection_with_scope(name, None, cx)
    }

    /// Define `name` with explicit scope as the selected range.
    pub(crate) fn name_selection_with_scope(&mut self, name: &str, sheet: Option<usize>, cx: &mut Context<Self>) -> bool {
        let Some(formula) = self.selection_formula() else { return false };
        self.set_named_formula_with_scope(name, sheet, &formula, cx)
    }

    /// Insert `name` into the active cell:
    /// - If editing: inserts at caret position
    /// - If not editing: begins editing with `={name}`
    pub(crate) fn use_name_in_formula(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.cell_edit().is_none() && self.selection.is_none() && self.current_cell().is_some() {
            self.begin_cell_edit(super::cell_edit::EditMode::Edit, true, cx);
        }
        if let Some(edit) = self.cell_edit_mut() {
            if edit.text.is_empty() {
                edit.text = format!("={name}");
            } else if !edit.text.starts_with('=') {
                edit.text.insert(0, '=');
                edit.text.push_str(name);
            } else {
                let caret = edit.caret.min(edit.text.encode_utf16().count());
                let b = super::cell_edit::byte_at(&edit.text, caret);
                edit.text.insert_str(b, name);
            }
            edit.caret = edit.text.encode_utf16().count();
            edit.anchor = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Create defined names from the edges of the selected range (Excel's 選択範囲から作成 / CreateNames):
    /// - `top_row`: use labels in the top row as names for the cell ranges in columns below
    /// - `left_column`: use labels in the leftmost column as names for the cell ranges in rows to the right
    pub(crate) fn create_names_from_selection(&mut self, top_row: bool, left_column: bool, cx: &mut Context<Self>) -> usize {
        let Some(range) = self.selected_range() else { return 0 };
        let Some((_, sheet)) = self.current_sheet() else { return 0 };
        let sheet_name = match &self.slide().name {
            Some(n) => ibis_calc::formula::quote_sheet(n),
            None => return 0,
        };
        let mut commands = Vec::new();
        let (first_col, last_col, first_row, last_row) = (range.first_column, range.last_column, range.first_row, range.last_row);

        let cell_text = |col: u32, row: u32| -> Option<String> {
            sheet.cell(row, col).and_then(|c| match &c.value {
                ibis_doc_model::CellValue::Text(t) => Some(t.clone()),
                ibis_doc_model::CellValue::Number(n) => Some(n.to_string()),
                _ => None,
            })
        };

        let col_name = |c: u32| ibis_doc_model::cell_name(c, 0).trim_end_matches('1').to_string();

        if top_row && last_row > first_row {
            let data_first_row = first_row + 1;
            let start_col = if left_column && last_col > first_col { first_col + 1 } else { first_col };
            for col in start_col..=last_col {
                if let Some(text) = cell_text(col, first_row) {
                    if let Some(name) = sanitize_name(&text) {
                        let c = col_name(col);
                        let formula = format!("{sheet_name}!${c}${}:${c}${}", data_first_row + 1, last_row + 1);
                        commands.push(Command::Sheet(SheetCommand::SetName {
                            name,
                            sheet: None,
                            formula: Some(formula),
                        }));
                    }
                }
            }
        }

        if left_column && last_col > first_col {
            let data_first_col = first_col + 1;
            let start_row = if top_row && last_row > first_row { first_row + 1 } else { first_row };
            for row in start_row..=last_row {
                if let Some(text) = cell_text(first_col, row) {
                    if let Some(name) = sanitize_name(&text) {
                        let c1 = col_name(data_first_col);
                        let c2 = col_name(last_col);
                        let formula = format!("{sheet_name}!${c1}${}:${c2}${}", row + 1, row + 1);
                        commands.push(Command::Sheet(SheetCommand::SetName {
                            name,
                            sheet: None,
                            formula: Some(formula),
                        }));
                    }
                }
            }
        }

        let count = commands.len();
        if !commands.is_empty() {
            if let Err(err) = self.document.execute(commands) {
                log::warn!("create names from selection error: {err}");
            }
            cx.notify();
        }
        count
    }

    /// Start editing an existing defined name.
    pub(crate) fn edit_name(&mut self, index: usize, name: &str, sheet: Option<usize>, formula: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.sheet_state_mut().and_then(|s| s.names.as_mut()) {
            state.editing = Some(index);
            state.new_sheet = sheet;
            state.new_name.update(cx, |i, cx| i.set_value(name, window, cx));
            state.new_formula.update(cx, |i, cx| i.set_value(format!("={formula}"), window, cx));
        }
        cx.notify();
    }

    /// Delete a name.
    pub(crate) fn delete_name(&mut self, name: &str, sheet: Option<usize>, cx: &mut Context<Self>) {
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetName { name: name.to_string(), sheet, formula: None })]) {
            log::warn!("deleting a name rejected: {err}");
        }
        cx.notify();
    }

    /// Open the name manager in the sidebar.
    pub(crate) fn open_names(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_name = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.new-name")));
        let initial_formula = self.selection_formula().map(|f| format!("={f}")).unwrap_or_default();
        let placeholder = crate::shell::i18n::text("_xlsx.refers-to-or-formula-e-g-sum-a1-a5-or-0-1");
        let new_formula = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder(placeholder);
            if !initial_formula.is_empty() {
                state.set_value(initial_formula, window, cx);
            }
            state
        });
        if let Some(state) = self.sheet_state_mut() {
            state.names = Some(NamesPanel { new_name, new_formula, new_sheet: None, editing: None });
        }
        if let Some(s) = self.sheet_state_mut() {
            s.page_setup = false;
        }
        if let Some(s) = self.sheet_state_mut() {
            s.custom_format = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// The name manager's panel.
    pub(crate) fn names_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let (new_name, new_formula, new_sheet, editing) = {
            let p = self.sheet_state()?.names.as_ref()?;
            (p.new_name.clone(), p.new_formula.clone(), p.new_sheet, p.editing)
        };
        let cur_sheet_idx = self.current_workbook_sheet_index();
        let sheet_names: Vec<String> = self
            .presentation()
            .workbook
            .as_ref()
            .map(|w| w.sheets.iter().map(|s| s.name.clone()).collect())
            .unwrap_or_default();
        let names: Vec<(String, Option<usize>, String)> =
            self.presentation().workbook.as_ref().map(|w| w.names.iter().filter(|n| !n.hidden && !n.name.starts_with("_xlnm.")).map(|n| (n.name.clone(), n.sheet, n.formula.clone())).collect()).unwrap_or_default();
        let rows = names.into_iter().enumerate().map(|(i, (name, sheet, formula))| {
            let (n1, n2, n3, n4, f3) = (name.clone(), name.clone(), name.clone(), name.clone(), formula.clone());
            let scope_badge = match sheet.and_then(|idx| sheet_names.get(idx)) {
                Some(sh_name) => format!(" [{sh_name}]"),
                None => String::new(),
            };
            let goto_ref = match sheet.and_then(|idx| sheet_names.get(idx)) {
                Some(sh_name) => format!("{}!{name}", ibis_calc::formula::quote_sheet(sh_name)),
                None => name.clone(),
            };
            v_flex()
                .gap_1()
                .py(px(4.))
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(SharedString::from(format!("{name}{scope_badge}"))))
                .child(div().text_sm().child(SharedString::from(format!("={formula}"))))
                .child(
                    h_flex()
                        .flex_wrap()
                        .gap_1()
                        .child(crate::shell::chrome::action_button(("name-goto", i)).small().label(crate::shell::i18n::text("_xlsx.go-to")).on_click(cx.listener(move |this, _, window, cx| {
                            this.go_to_reference(&goto_ref, window, cx);
                        })))
                        .child(crate::shell::chrome::action_button(("name-use", i)).small().label(crate::shell::i18n::text("xlsx.use-in-formula")).on_click(cx.listener(move |this, _, window, cx| {
                            this.use_name_in_formula(&n4, window, cx);
                        })))
                        .child(crate::shell::chrome::action_button(("name-edit", i)).small().label(crate::shell::i18n::text("_xlsx.edit")).on_click(cx.listener(move |this, _, window, cx| {
                            this.edit_name(i, &n3, sheet, &f3, window, cx);
                        })))
                        .child(crate::shell::chrome::action_button(("name-set", i)).small().label(crate::shell::i18n::text("_xlsx.use-selection")).on_click(cx.listener(move |this, _, _, cx| {
                            this.name_selection_with_scope(&n1, sheet, cx);
                        })))
                        .child(crate::shell::chrome::destructive_button(("name-delete", i),cx).small().label(crate::shell::i18n::text("_shell.delete")).on_click(cx.listener(move |this, _, _, cx| this.delete_name(&n2, sheet, cx)))),
                )
        });
        let name_input = new_name.clone();
        let formula_input = new_formula.clone();
        let is_editing = editing.is_some();
        let scope_is_workbook = new_sheet.is_none();
        let current_sheet_label = new_sheet
            .or(cur_sheet_idx)
            .and_then(|idx| sheet_names.get(idx))
            .cloned()
            .unwrap_or_else(|| crate::shell::i18n::text("xlsx.names.scope.sheet").to_string());
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("_shell.name")))
            .child(crate::shell::chrome::text_field(&new_name).full_width())
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("xlsx.names.scope")))
            .child(
                h_flex()
                    .gap_1()
                    .child({
                        crate::shell::chrome::radio_button("name-scope-workbook",scope_is_workbook,false,crate::shell::i18n::text("xlsx.names.scope.workbook"),cx.listener(|this, _, _, cx| {
                            if let Some(state) = this.sheet_state_mut().and_then(|s| s.names.as_mut()) {
                                state.new_sheet = None;
                            }
                            cx.notify();
                        }))
                    })
                    .child({
                        crate::shell::chrome::radio_button("name-scope-sheet",!scope_is_workbook,false,SharedString::from(current_sheet_label),cx.listener(move |this, _, _, cx| {
                            let target_idx = this.current_workbook_sheet_index();
                            if let Some(state) = this.sheet_state_mut().and_then(|s| s.names.as_mut()) {
                                state.new_sheet = target_idx;
                            }
                            cx.notify();
                        }))
                    }),
            )
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("_xlsx.refers-to-or-formula")))
            .child(super::range_field::local_range_field(&new_formula,"names-new_formula",false,cx))
            .child({
                let (add_name, add_formula) = (name_input.clone(), formula_input.clone());
                let mut btn_row = h_flex()
                    .gap_1()
                    .child(crate::shell::chrome::primary_button("name-add").small().label(if is_editing { crate::shell::i18n::text("_xlsx.update-name") } else { crate::shell::i18n::text("_xlsx.add-name") }).on_click(cx.listener(move |this, _, window, cx| {
                        let name_text = add_name.read(cx).value().trim().to_string();
                        let formula_text = add_formula.read(cx).value().trim().to_string();
                        let scope = this.sheet_state().and_then(|s| s.names.as_ref()).and_then(|p| p.new_sheet);
                        if this.set_named_formula_with_scope(&name_text, scope, &formula_text, cx) {
                            add_name.update(cx, |i, cx| i.set_value("", window, cx));
                            let default_formula = this.selection_formula().map(|f| format!("={f}")).unwrap_or_default();
                            add_formula.update(cx, |i, cx| i.set_value(default_formula, window, cx));
                            if let Some(state) = this.sheet_state_mut().and_then(|s| s.names.as_mut()) {
                                state.editing = None;
                                state.new_sheet = None;
                            }
                        }
                    })));
                if is_editing {
                    let (cancel_name, cancel_formula) = (name_input.clone(), formula_input.clone());
                    btn_row = btn_row.child(crate::shell::chrome::action_button("name-cancel").small().label(crate::shell::i18n::text("_shell.cancel")).on_click(cx.listener(move |this, _, window, cx| {
                        cancel_name.update(cx, |i, cx| i.set_value("", window, cx));
                        let default_formula = this.selection_formula().map(|f| format!("={f}")).unwrap_or_default();
                        cancel_formula.update(cx, |i, cx| i.set_value(default_formula, window, cx));
                        if let Some(state) = this.sheet_state_mut().and_then(|s| s.names.as_mut()) {
                            state.editing = None;
                            state.new_sheet = None;
                        }
                        cx.notify();
                    })));
                }
                btn_row
            })
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("xlsx.create-from-selection")))
            .child(
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .child(crate::shell::chrome::action_button("name-create-top").small().label(crate::shell::i18n::text("xlsx.create-names-top-row")).on_click(cx.listener(|this, _, _, cx| {
                        this.create_names_from_selection(true, false, cx);
                    })))
                    .child(crate::shell::chrome::action_button("name-create-left").small().label(crate::shell::i18n::text("xlsx.create-names-left-col")).on_click(cx.listener(|this, _, _, cx| {
                        this.create_names_from_selection(false, true, cx);
                    })))
                    .child(crate::shell::chrome::action_button("name-create-both").small().label(crate::shell::i18n::text("xlsx.create-names-both")).on_click(cx.listener(|this, _, _, cx| {
                        this.create_names_from_selection(true, true, cx);
                    }))),
            )
            .children(rows);
        Some(SidebarPanel { title: "_xlsx.name-manager".into(), body: body.into_any_element() })
    }
}

#[cfg(test)]
mod tests {
    use super::{sanitize_name, valid_name};

    #[test]
    fn names() {
        assert!(valid_name("売上"));
        assert!(valid_name("Tax_Rate"));
        assert!(!valid_name("A1"));
        assert!(!valid_name("XFD100"));
        assert!(!valid_name("R1C1"));
        assert!(!valid_name("1st"));
        assert!(!valid_name("my name"));
        assert!(valid_name("ABCD1"));

        assert_eq!(sanitize_name("売上 4月"), Some("売上_4月".into()));
        assert_eq!(sanitize_name("2026年"), Some("_2026年".into()));
        assert_eq!(sanitize_name("A1"), Some("_A1".into()));
    }
}
