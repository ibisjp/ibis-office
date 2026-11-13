// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 名前付き範囲と名前付き数式: a name typed in the name box for the selected range
//! defines it (as in Excel), the name box and ジャンプ go to a name, and
//! the sidebar's 名前の管理 lists the names, changes what one stands for
//! (to an arbitrary formula, constant, or the selected range), edits names,
//! and deletes names.

use gpui_kit::*;
use ibis_doc_model::{Command, SheetCommand};

use crate::shell::editor::SlideEditor;
use crate::ext::*;

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

#[ibis_office_macros::editor_ext(Names)]
impl SlideEditor {
    /// The selected range as a name's formula: `Sheet!$A$1:$B$3`.
    pub(crate) fn selection_formula(&self) -> Option<String> {
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

    /// What a name typed as `formula` stands for: references without a sheet get the active sheet (absolute), an
    /// empty formula is the selected range. `None` when there is nothing to stand for.
    pub(crate) fn name_target_formula(&self, formula: &str) -> Option<String> {
        let clean = formula.trim().trim_start_matches('=').trim().to_string();
        let current_sheet = self.slide().name.clone().unwrap_or_else(|| "Sheet1".to_string());
        let target = if clean.is_empty() {
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
        (!target.is_empty()).then_some(target)
    }

    /// Define `name` with a formula or constant (without leading `=`) in workbook scope (`sheet = None`), or fallback to the selected range.
    pub(crate) fn set_named_formula(&mut self, name: &str, formula: &str, cx: &mut Context<Self>) -> bool {
        self.set_named_formula_with_scope(name, None, formula, cx)
    }

    /// Define `name` with an explicit scope (`None` for workbook, `Some(sheet_index)` for sheet-local) and formula/constant.
    pub(crate) fn set_named_formula_with_scope(&mut self, name: &str, sheet: Option<usize>, formula: &str, cx: &mut Context<Self>) -> bool {
        if !valid_name(name) {
            self.status = Some(crate::shell::i18n::format("xlsx.names.status.invalid-name", &[("name", &name)]).into());
            cx.notify();
            return false;
        }
        let Some(target_formula) = self.name_target_formula(formula) else {
            self.status = Some(crate::shell::i18n::text("xlsx.names.err.ref-empty").into());
            cx.notify();
            return false;
        };
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

    /// Delete a name.
    pub(crate) fn delete_name(&mut self, name: &str, sheet: Option<usize>, cx: &mut Context<Self>) {
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetName { name: name.to_string(), sheet, formula: None })]) {
            log::warn!("deleting a name rejected: {err}");
        }
        cx.notify();
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
