// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! エラー チェック: a cell marked with the green triangle shows a button
//! at its left when it is the selected cell; the button opens Excel's menu
//! (the message, 数値に変換, エラーを無視, 数式バーで編集).
use crate::shell::chrome::{MenuEntry, MenuItem};
use crate::shell::editor::SlideEditor;
use gpui_kit::*;
use ibis_calc::input::{Typed, parse_input};
use ibis_doc_model::error_check::{ErrorKind, Rules, errors_at};
use ibis_doc_model::{CellEdit, CellInput, CellRange, CellValue, Command, SheetCommand, StyleChange};

impl SlideEditor {
    /// The first error of the selected cell, if it has one not ignored.
    pub(crate) fn current_error(&self) -> Option<ErrorKind> {
        ensure_rules();
        let (column, row) = self.current_cell()?;
        let (_, sheet) = self.current_sheet()?;
        errors_at(sheet, row, column).first().copied()
    }

    /// The menu of the button: the message, and what can be done.
    pub(crate) fn error_menu(&self) -> Vec<MenuEntry> {
        let Some(kind) = self.current_error() else { return Vec::new() };
        let mut out: Vec<MenuEntry> = vec![MenuItem::new("xlsx.error-message", crate::shell::i18n::text(kind_id(kind)).to_string()).disabled(true).into(), MenuEntry::Separator];
        if kind == ErrorKind::NumberAsText {
            out.push(MenuItem::new("xlsx.error-convert", "_xlsx.convert-to-number").on(|this, _, cx| this.convert_text_numbers(cx)).into());
        } else if kind == ErrorKind::CalculatedColumn {
            out.push(MenuItem::new("xlsx.error-restore-column", "xlsx.error-restore-column").on(|this, _, cx| this.restore_calculated_column(cx)).into());
        }
        out.push(MenuItem::new("xlsx.error-ignore", "_xlsx.ignore-error").on(|this, _, cx| this.ignore_error(cx)).into());
        out.push(MenuItem::new("xlsx.error-edit", "_xlsx.edit-in-formula-bar").on(|this, _, cx| this.begin_cell_edit(super::cell_edit::EditMode::Edit, true, cx)).into());
        out
    }

    pub(crate) fn open_error_menu(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let entries = self.error_menu();
        if !entries.is_empty() {
            self.open_popup_menu(entries, position, window, cx);
        }
    }

    /// エラーを無視: the selected cell's kind of error ignored in the selection.
    pub(crate) fn ignore_error(&mut self, cx: &mut Context<Self>) {
        let Some(kind) = self.current_error() else { return };
        let Some(range) = self.selected_range() else { return };
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::IgnoreErrors { slide, range, kind })], cx);
    }

    /// 計算列の数式を復元: restore the calculated column formula across the table column
    pub(crate) fn restore_calculated_column(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(crate::xlsx::protection::Guarded::Cells, cx) {
            return;
        }
        let Some((column, row)) = self.current_cell() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some(table) = sheet.tables.iter().find(|t| {
            let first_row = t.range.first_row + t.header_rows;
            let last_row = t.range.last_row.saturating_sub(t.totals_rows);
            first_row <= row && row <= last_row && t.range.first_column <= column && column <= t.range.last_column
        }) else {
            return;
        };
        let start_row = table.range.first_row + table.header_rows;
        let end_row = table.range.last_row.saturating_sub(table.totals_rows);
        let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for r in start_row..=end_row {
            if let Some(f) = sheet.cell(r, column).and_then(|c| c.formula.as_deref()) {
                let norm = ibis_calc::formula::shift(f, start_row as i64 - r as i64, 0);
                *counts.entry(norm).or_insert(0) += 1;
            }
        }
        let Some((majority_formula, _)) = counts.iter().max_by_key(|e| *e.1) else {
            return;
        };
        let target_formula = ibis_calc::formula::shift(majority_formula, row as i64 - start_row as i64, 0);
        let slide = self.current_slide;
        let edits = vec![CellEdit {
            row,
            column,
            input: CellInput::Formula(target_formula),
        }];
        self.run_sheet(vec![Command::Sheet(SheetCommand::SetCells { slide, edits })], cx);
    }

    /// 数値に変換: the text constants of the selection that read as numbers
    /// become those numbers (with the format typing them gives).
    pub(crate) fn convert_text_numbers(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(crate::xlsx::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(range) = self.selected_range() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let mut edits = Vec::new();
        let mut formats: Vec<(CellRange, String)> = Vec::new();
        for row in range.first_row..=range.last_row {
            for column in range.first_column..=range.last_column {
                if !errors_at(sheet, row, column).contains(&ErrorKind::NumberAsText) {
                    continue;
                }
                let Some(CellValue::Text(t)) = sheet.cell(row, column).map(|c| c.value.clone()) else { continue };
                if let Typed::Value(CellValue::Number(n), format) = parse_input(t.trim(), date1904) {
                    if let Some(code) = format {
                        formats.push((CellRange { first_row: row, last_row: row, first_column: column, last_column: column }, code.to_string()));
                    }
                    edits.push(CellEdit { row, column, input: CellInput::Value(CellValue::Number(n)) });
                }
            }
        }
        if edits.is_empty() {
            return;
        }
        let slide = self.current_slide;
        let mut commands = vec![Command::Sheet(SheetCommand::SetCells { slide, edits })];
        for (range, code) in formats {
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: StyleChange::NumberFormat(code) }));
        }
        self.run_sheet(commands, cx);
    }
}

/// The id of the message Excel shows for an error.
pub(crate) fn kind_id(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::NumberAsText => "xlsx.error.number-as-text",
        ErrorKind::EvalError => "xlsx.error.eval-error",
        ErrorKind::InconsistentFormula => "xlsx.error.inconsistent-formula",
        ErrorKind::TwoDigitYear => "xlsx.error.two-digit-year",
        ErrorKind::UnlockedFormula => "xlsx.error.unlocked-formula",
        ErrorKind::EmptyCellReference => "xlsx.error.empty-cell-reference",
        ErrorKind::FormulaRange => "xlsx.error.formula-range",
        ErrorKind::CalculatedColumn => "xlsx.error.calculated-column",
        ErrorKind::ListDataValidation => "xlsx.error.list-data-validation",
    }
}

fn rules_path() -> Option<std::path::PathBuf> {
    if cfg!(feature = "scenarios") {
        return None;
    }
    let base = crate::shell::app_data::base();
    Some(base?.join("ibisOffice").join("error-check.tsv"))
}

const KEYS: [&str; 10] = ["background", "number_as_text", "eval_error", "inconsistent_formula", "two_digit_year", "unlocked_formula", "empty_cell_reference", "formula_range", "calculated_column", "list_data_validation"];

fn fields(r: &mut Rules) -> [&mut bool; 10] {
    let Rules { background, number_as_text, eval_error, inconsistent_formula, two_digit_year, unlocked_formula, empty_cell_reference, formula_range, calculated_column, list_data_validation } = r;
    [background, number_as_text, eval_error, inconsistent_formula, two_digit_year, unlocked_formula, empty_cell_reference, formula_range, calculated_column, list_data_validation]
}

fn parse_rules(text: &str) -> Rules {
    let mut rules = Rules::default();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('\t')
            && let Some(i) = KEYS.iter().position(|k| *k == key)
        {
            *fields(&mut rules)[i] = value == "1";
        }
    }
    rules
}

fn serialize_rules(rules: &Rules) -> String {
    let mut r = *rules;
    fields(&mut r).iter().zip(KEYS).map(|(v, k)| format!("{k}\t{}\n", if **v { 1 } else { 0 })).collect()
}

/// The rules saved (read once, before the first cell is checked).
pub(crate) fn ensure_rules() {
    static LOADED: std::sync::Once = std::sync::Once::new();
    LOADED.call_once(|| {
        if let Some(text) = rules_path().and_then(|p| std::fs::read_to_string(p).ok()) {
            ibis_doc_model::error_check::set_rules(parse_rules(&text));
        }
    });
}

fn save_rules(rules: &Rules) {
    ibis_doc_model::error_check::set_rules(*rules);
    if let Some(path) = rules_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, serialize_rules(rules));
    }
}

pub(crate) struct ErrorOptionsPanel {
    pub(crate) rules: Rules,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

const RULE_IDS: [&str; 10] = [
    "xlsx.error-options.background",
    "xlsx.error-options.number-as-text",
    "xlsx.error-options.eval-error",
    "xlsx.error-options.inconsistent-formula",
    "xlsx.error-options.two-digit-year",
    "xlsx.error-options.unlocked-formula",
    "xlsx.error-options.empty-cell-reference",
    "xlsx.error-options.formula-range",
    "xlsx.error-options.calculated-column",
    "xlsx.error-options.list-data-validation",
];

impl SlideEditor {
    pub(crate) fn open_error_options_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        ensure_rules();
        let panel = ErrorOptionsPanel { rules: ibis_doc_model::error_check::rules(), ui: Default::default() };
        let state = self.wb_mut();
        state.close_settings();
        state.panels.error_options = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// OK: the rules in force, and saved.
    pub(crate) fn apply_error_options_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.error_options.take() {
            save_rules(&p.rules);
        }
        cx.notify();
    }

    pub(crate) fn error_options_panel(&mut self, cx: &mut Context<Self>) -> Option<crate::shell::chrome::SidebarPanel> {
        use gpui_kit::component::button::{ButtonVariants as _};
        use gpui_kit::component::{Sizable as _, v_flex};
        use crate::shell::i18n::text;
        let p = self.sheet_state()?.panels.error_options.as_ref()?;
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
        let mut body = v_flex().gap_2().px(px(16.)).py(px(8.)).p_0();
        let mut rules = p.rules;
        let values: Vec<bool> = fields(&mut rules).iter().map(|v| **v).collect();
        for (i, on) in values.into_iter().enumerate() {
            let id = RULE_IDS[i];
            let key = KEYS[i];
            body = body.child(mark(
                key,
                super::toggle::toggle_row(
                    SharedString::from(format!("error-rule-{key}")),
                    on,
                    text(id),
                    cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.error_options.as_mut() {
                            let mut r = p.rules;
                            *fields(&mut r)[i] = !*fields(&mut r)[i];
                            p.rules = r;
                        }
                        cx.notify();
                    }),
                )
                .into_any_element(),
            ));
        }
        body = body.child(mark("reset", crate::shell::chrome::action_button("error-reset").small().label(text("xlsx.error-options.reset-ignored")).on_click(cx.listener(|this, _, _, cx| {
            let slide = this.current_slide;
            this.run_sheet(vec![Command::Sheet(SheetCommand::ResetIgnoredErrors { slide })], cx);
        })).into_any_element()));
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("error-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label(text("OK")).on_click(cx.listener(|this, _, _, cx| this.apply_error_options_panel(cx)))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(crate::shell::chrome::SidebarPanel::with_footer(text("xlsx.error-options.title"), body.into_any_element(), panel_terminal_actions))
    }
}

#[cfg(test)]
mod tests {
    // Named, not `super::*`: that glob brings in gpui_kit's `test` macro, which
    // replaces `#[test]` here and recurses ("recursion limit reached").
    use super::{ErrorKind, RULE_IDS, Rules, kind_id, parse_rules, serialize_rules};

    #[test]
    fn the_rules_round_trip_through_the_file_text() {
        let rules = Rules { empty_cell_reference: true, two_digit_year: false, ..Rules::default() };
        assert_eq!(parse_rules(&serialize_rules(&rules)), rules);
        assert_eq!(parse_rules(""), Rules::default());
    }

    #[test]
    fn every_caption_is_in_both_languages() {
        let ids = ErrorKind::ALL.iter().map(|k| kind_id(*k)).chain(RULE_IDS).chain(["xlsx.error-options", "xlsx.error-options.title", "xlsx.error-options.reset-ignored", "xlsx.error-restore-column"]);
        let ids: Vec<&str> = ids.collect();
        for table in [include_str!("../../i18n/ja/xlsx.tsv"), include_str!("../../i18n/en/xlsx.tsv")] {
            for id in &ids {
                assert!(table.lines().any(|l| l.split('\t').next() == Some(*id)), "{id} is missing");
            }
        }
    }
}
