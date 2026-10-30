// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データの入力規則: a list's drop-down by the selected cell, the message
//! when a cell with one is selected, a value that breaks the rule refused
//! (or warned about), and the sidebar's panel to set a list (and a
//! message) on the selected range or take the rule off.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand, Sheet, Validation};

use crate::shell::chrome::{MenuEntry, MenuItem, SidebarPanel, fill_menu};
use crate::shell::editor::SlideEditor;

/// The panel's fields, as Excel's dialog: 設定 (the kind of value, the
/// comparison, its values, blanks, the drop-down), 入力時メッセージ and
/// エラー メッセージ.
pub(crate) struct ValidationPanel {
    /// `none` (すべての値), `whole`, `decimal`, `list`, `date`, `time`,
    /// `textLength`, `custom`.
    pub(crate) kind: &'static str,
    pub(crate) operator: &'static str,
    /// A list's items or range, a bound, a custom formula.
    pub(crate) items: Entity<InputState>,
    pub(crate) second: Entity<InputState>,
    pub(crate) allow_blank: bool,
    pub(crate) dropdown: bool,
    pub(crate) apply_to_same: bool,
    pub(crate) show_prompt: bool,
    pub(crate) prompt_title: Entity<InputState>,
    pub(crate) prompt: Entity<InputState>,
    pub(crate) show_error: bool,
    /// `stop`, `warning`, `information`.
    pub(crate) style: &'static str,
    pub(crate) error_title: Entity<InputState>,
    pub(crate) error_text: Entity<InputState>,
    pub(crate) message: String,
    /// The tab shown: 0 設定, 1 入力時メッセージ, 2 エラー メッセージ.
    pub(crate) tab: usize,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

/// The kinds of value, as Excel's 入力値の種類 lists them.
pub(crate) const KINDS: [(&str, &str); 8] = [
    ("none", "fix.validation.any-value"),
    ("whole", "fix.validation.whole-number"),
    ("decimal", "fix.validation.decimal"),
    ("list", "fix.validation.list"),
    ("date", "docx.field.date"),
    ("time", "docx.field.time"),
    ("textLength", "fix.validation.text-length"),
    ("custom", "_shell.custom"),
];

/// The comparisons (データ).
pub(crate) const OPERATORS: [(&str, &str); 8] = [
    ("between", "fix.validation.between"),
    ("notBetween", "fix.validation.not-between"),
    ("equal", "fix.validation.equal"),
    ("notEqual", "fix.validation.not-equal"),
    ("greaterThan", "fix.validation.greater-than"),
    ("lessThan", "fix.validation.less-than"),
    ("greaterThanOrEqual", "fix.validation.greater-than-equal"),
    ("lessThanOrEqual", "fix.validation.less-than-equal"),
];

/// The error message's styles (スタイル).
pub(crate) const STYLES: [(&str, &str); 3] = [("stop", "fix.validation.stop"), ("warning", "fix.validation.warning"), ("information", "fix.validation.information")];

fn name_of(list: &[(&'static str, &'static str)], key: &str) -> &'static str {
    list.iter().find(|(k, _)| *k == key).map_or("", |(_, n)| n)
}

fn same_validation_settings(a: &Validation, b: &Validation) -> bool {
    a.kind == b.kind
        && a.operator == b.operator
        && a.formula1 == b.formula1
        && a.formula2 == b.formula2
        && a.allow_blank == b.allow_blank
        && a.dropdown == b.dropdown
        && a.prompt == b.prompt
        && a.error == b.error
}

/// A list's drop-down (or another small menu by the cells), open.
pub(crate) struct ListPopup {
    pub(crate) menu: Entity<PopupMenu>,
    pub(crate) position: Point<Pixels>,
    pub(crate) _dismiss: Subscription,
}

/// Cell-owned list: focus stays in the editor so IME and ordinary typing keep working.
pub(crate) struct ValidationChoices {
    pub(crate) items: Vec<String>,
    pub(crate) selected: usize,
    pub(crate) original: String,
    pub(crate) selection_query: String,
    pub(crate) scroll: ScrollHandle,
    pub(crate) bounds: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
}

/// A pending confirmation for a data validation rule with style `warning` ("注意").
#[derive(Clone, Debug)]
pub(crate) struct ValidationWarning {
    pub(crate) message: String,
    pub(crate) edit: super::cell_edit::CellEdit,
}

/// The rule over a cell (the last one listed wins, as Excel keeps one).
pub(crate) fn rule_at(sheet: &Sheet, row: u32, column: u32) -> Option<&Validation> {
    sheet.validations.iter().rev().find(|v| v.ranges.iter().any(|r| (r.first_row..=r.last_row).contains(&row) && (r.first_column..=r.last_column).contains(&column)))
}

/// A list rule's items: the text in quotes split at commas, or the
/// values of a range on the same sheet.
pub(crate) fn list_items(sheet: &Sheet, rule: &Validation) -> Vec<String> {
    let Some(f) = rule.formula1.as_deref() else { return Vec::new() };
    let f = f.trim();
    if let Some(inner) = f.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        return inner.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    }
    let area = f.rsplit('!').next().unwrap_or(f).replace('$', "");
    let Some(r) = parse_area(&area) else { return Vec::new() };
    let mut out = Vec::new();
    for row in r.0..=r.2 {
        for column in r.1..=r.3 {
            if let Some(c) = sheet.cell(row, column)
                && !c.text.is_empty()
            {
                out.push(c.text.clone());
            }
        }
    }
    out
}

/// Collect unique text candidates from the same column of (`row`, `column`) in Excel's
/// "Pick from Drop-down List" (⌥↓ / Alt+↓) style.
pub(crate) fn column_pick_list_items(sheet: &Sheet, row: u32, column: u32) -> Vec<String> {
    let mut texts = Vec::new();
    // 1. Scan upward from row - 1 until an empty cell or top
    if row > 0 {
        let mut r = row - 1;
        loop {
            if let Some(c) = sheet.cell(r, column) {
                let trimmed = c.text.trim();
                if !trimmed.is_empty() {
                    texts.push(trimmed.to_string());
                } else {
                    break;
                }
            } else {
                break;
            }
            if r == 0 {
                break;
            }
            r -= 1;
        }
    }
    // 2. Scan downward from row + 1 until an empty cell
    let mut r = row + 1;
    loop {
        if let Some(c) = sheet.cell(r, column) {
            let trimmed = c.text.trim();
            if !trimmed.is_empty() {
                texts.push(trimmed.to_string());
            } else {
                break;
            }
        } else {
            break;
        }
        r += 1;
    }
    // 3. Fallback: if no continuous block items, collect from any non-empty cells in the same column
    if texts.is_empty() {
        for c in &sheet.cells {
            if c.column == column && c.row != row {
                let trimmed = c.text.trim();
                if !trimmed.is_empty() {
                    texts.push(trimmed.to_string());
                }
            }
        }
    }

    // Deduplicate and sort alphabetically / case-insensitively
    texts.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b)));
    texts.dedup_by(|a, b| a.to_lowercase() == b.to_lowercase());
    texts
}

/// `A1` or `A1:B3` as (first row, first column, last row, last column).
fn parse_area(text: &str) -> Option<(u32, u32, u32, u32)> {
    let one = |t: &str| -> Option<(u32, u32)> {
        let split = t.find(|c: char| c.is_ascii_digit())?;
        let column = ibis_calc::formula::parser::column_index(&t[..split])?;
        let row: u32 = t[split..].parse().ok()?;
        Some((row.checked_sub(1)?, column))
    };
    let (a, b) = text.split_once(':').unwrap_or((text, text));
    let (r0, c0) = one(a)?;
    let (r1, c1) = one(b)?;
    Some((r0.min(r1), c0.min(c1), r0.max(r1), c0.max(c1)))
}

/// Whether `value` keeps the rule.
pub(crate) fn keeps(sheet: &Sheet, rule: &Validation, value: &CellValue) -> bool {
    if value.is_empty() {
        return rule.allow_blank || rule.kind == "none";
    }
    let bound = |f: &Option<String>| -> Option<f64> {
        let f = f.as_deref()?;
        f.trim().parse::<f64>().ok().or_else(|| {
            // A cell of the same sheet.
            let area = f.rsplit('!').next().unwrap_or(f).replace('$', "");
            let (r, c, _, _) = parse_area(&area)?;
            match sheet.cell(r, c).map(|x| &x.value) {
                Some(CellValue::Number(n)) => Some(*n),
                _ => None,
            }
        })
    };
    let compare = |v: f64| -> bool {
        let (a, b) = (bound(&rule.formula1), bound(&rule.formula2));
        match (rule.operator.as_str(), a) {
            ("between", Some(a)) => b.is_some_and(|b| v >= a.min(b) && v <= a.max(b)),
            ("notBetween", Some(a)) => b.is_some_and(|b| v < a.min(b) || v > a.max(b)),
            ("equal", Some(a)) => v == a,
            ("notEqual", Some(a)) => v != a,
            ("greaterThan", Some(a)) => v > a,
            ("lessThan", Some(a)) => v < a,
            ("greaterThanOrEqual", Some(a)) => v >= a,
            ("lessThanOrEqual", Some(a)) => v <= a,
            _ => true,
        }
    };
    match rule.kind.as_str() {
        "list" => {
            let text = match value {
                CellValue::Number(n) => ibis_calc::eval::number_to_text(*n),
                CellValue::Text(t) => t.clone(),
                CellValue::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
                CellValue::Error(e) => e.as_str().to_string(),
                CellValue::Empty => String::new(),
            };
            list_items(sheet, rule).iter().any(|i| i.eq_ignore_ascii_case(&text))
        }
        "whole" => matches!(value, CellValue::Number(n) if n.fract() == 0.0 && compare(*n)),
        "decimal" | "date" | "time" => matches!(value, CellValue::Number(n) if compare(*n)),
        "textLength" => {
            let len = match value {
                CellValue::Text(t) => t.chars().count(),
                CellValue::Number(n) => ibis_calc::eval::number_to_text(*n).chars().count(),
                _ => 0,
            };
            compare(len as f64)
        }
        _ => true,
    }
}

/// Whether `value` put into (`row`, `column`) keeps the rule: a custom
/// rule's formula worked out with the value in the cell (its references
/// moved from the rule's first cell to this one), true unless it gives
/// FALSE or 0 (as Excel).
pub(crate) fn keeps_at(sheet: &Sheet, rule: &Validation, value: &CellValue, row: u32, column: u32) -> bool {
    if rule.kind != "custom" {
        return keeps(sheet, rule, value);
    }
    if value.is_empty() && rule.allow_blank {
        return true;
    }
    let (Some(formula), Some(first)) = (rule.formula1.as_deref(), rule.ranges.first()) else { return true };
    let mut with = sheet.clone();
    let mut cell = with.cell(row, column).cloned().unwrap_or(ibis_doc_model::SheetCell { runs: None, phonetic: None, row, column, value: CellValue::Empty, text: String::new(), kind: ibis_doc_model::CellKind::Empty, formula: None, style: 0 });
    cell.value = value.clone();
    cell.formula = None;
    with.put_cell(row, column, Some(cell));
    let shifted = ibis_calc::formula::shift(formula, row as i64 - first.first_row as i64, column as i64 - first.first_column as i64);
    let book = super::conditional::OneSheet(&with);
    match ibis_calc::recalc::evaluate(&book, (with.book_context.sheet_index, row, column), &shifted).0 {
        CellValue::Bool(b) => b,
        CellValue::Number(n) => n != 0.0,
        CellValue::Error(_) => false,
        _ => true,
    }
}

impl SlideEditor {
    /// The rule over the selected cell.
    pub(crate) fn current_rule(&self) -> Option<Validation> {
        let (column, row) = self.current_cell()?;
        let (_, sheet) = self.current_sheet()?;
        rule_at(sheet, row, column).cloned()
    }

    fn target_validation_ranges(&self, apply_to_same: bool) -> Vec<ibis_doc_model::CellRange> {
        let mut ranges = self.selected_ranges();
        if apply_to_same
            && let Some(orig) = self.current_rule()
            && let Some((_, sheet)) = self.current_sheet()
        {
            for v in &sheet.validations {
                if same_validation_settings(v, &orig) {
                    for &r in &v.ranges {
                        if !ranges.contains(&r) {
                            ranges.push(r);
                        }
                    }
                }
            }
        }
        ranges
    }

    /// Select cells on the current worksheet that have data validation (`same_only`: only those matching the active cell's rule).
    pub(crate) fn select_validation_cells(&mut self, same_only: bool, cx: &mut Context<Self>) -> bool {
        let orig = self.current_rule();
        let Some((_, sheet)) = self.current_sheet() else { return false };
        let mut matched: Vec<ibis_doc_model::CellRange> = Vec::new();
        for v in &sheet.validations {
            if v.kind == "none" && v.prompt.is_none() && v.error.is_none() {
                continue;
            }
            if same_only {
                let Some(ref o) = orig else { continue };
                if !same_validation_settings(v, o) {
                    continue;
                }
            }
            matched.extend(v.ranges.iter().copied());
        }
        let Some(first) = matched.first().copied() else { return false };
        let min_r = matched.iter().map(|r| r.first_row).min().unwrap_or(first.first_row);
        let max_r = matched.iter().map(|r| r.last_row).max().unwrap_or(first.last_row);
        let min_c = matched.iter().map(|r| r.first_column).min().unwrap_or(first.first_column);
        let max_c = matched.iter().map(|r| r.last_column).max().unwrap_or(first.last_column);
        let slide = self.current_slide;
        if let Some(s) = self.sheet_state_mut() {
            s.cells.insert(slide, (min_c, min_r));
            if min_c != max_c || min_r != max_r {
                s.cell_ends.insert(slide, (max_c, max_r));
            } else {
                s.cell_ends.remove(&slide);
            }
        }
        self.scroll_range_into_view(false);
        cx.notify();
        true
    }

    /// Jump to the next cell on the current worksheet that violates its data validation rule.
    pub(crate) fn jump_to_next_invalid_cell(&mut self, cx: &mut Context<Self>) -> Option<(u32, u32)> {
        let mut invalid = self.invalid_cells();
        if invalid.is_empty() {
            return None;
        }
        // Sort in row-major order (row, col)
        invalid.sort_by_key(|&(c, r)| (r, c));
        let cur = self.current_cell().unwrap_or((0, 0));
        let next = invalid
            .iter()
            .copied()
            .find(|&(c, r)| (r, c) > (cur.1, cur.0))
            .unwrap_or(invalid[0]);
        if let Some(s) = self.sheet_state_mut() {
            s.validation_circles = true;
        }
        self.set_cell(next.0, next.1);
        self.scroll_range_into_view(false);
        cx.notify();
        Some(next)
    }

    pub(crate) fn validation_list_items_in_book(&self, rule: &Validation) -> Vec<String> {
        let Some(f) = rule.formula1.as_deref() else { return Vec::new() };
        let f = f.trim();
        if let Some(inner) = f.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
            return inner.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        }
        let clean = f.trim_start_matches('=').trim();
        if let Ok(resolved) = self.resolve_range(clean) {
            let mut out = Vec::new();
            for row in resolved.range.first_row..=resolved.range.last_row {
                for column in resolved.range.first_column..=resolved.range.last_column {
                    if let Some(c) = resolved.sheet.cell(row, column)
                        && !c.text.is_empty()
                    {
                        out.push(c.text.clone());
                    }
                }
            }
            return out;
        }
        if let Some((_, sheet)) = self.current_sheet() {
            return list_items(sheet, rule);
        }
        Vec::new()
    }

    pub(crate) fn keeps_validation_in_book(&self, rule: &Validation, value: &CellValue, row: u32, column: u32) -> bool {
        let Some((_, sheet)) = self.current_sheet() else { return true };
        if value.is_empty() {
            return rule.allow_blank || rule.kind == "none";
        }
        if rule.kind == "custom" {
            return keeps_at(sheet, rule, value, row, column);
        }
        let bound = |f: &Option<String>| -> Option<f64> {
            let f = f.as_deref()?.trim();
            if let Ok(n) = f.parse::<f64>() {
                return Some(n);
            }
            let clean = f.trim_start_matches('=').trim();
            if let Ok(resolved) = self.resolve_range(clean) {
                if let Some(CellValue::Number(n)) = resolved.sheet.cell(resolved.range.first_row, resolved.range.first_column).map(|x| &x.value) {
                    return Some(*n);
                }
            }
            let book = ibis_doc_model::sheet_calc::BookView::new(self.presentation());
            let sheet_idx = self.presentation().slides.iter().position(|s| s.id == self.current_slide).unwrap_or(0);
            match ibis_calc::recalc::evaluate(&book, (sheet_idx, row, column), clean).0 {
                CellValue::Number(n) => Some(n),
                _ => None,
            }
        };
        let compare = |v: f64| -> bool {
            let (a, b) = (bound(&rule.formula1), bound(&rule.formula2));
            match (rule.operator.as_str(), a) {
                ("between", Some(a)) => b.is_some_and(|b| v >= a.min(b) && v <= a.max(b)),
                ("notBetween", Some(a)) => b.is_some_and(|b| v < a.min(b) || v > a.max(b)),
                ("equal", Some(a)) => v == a,
                ("notEqual", Some(a)) => v != a,
                ("greaterThan", Some(a)) => v > a,
                ("lessThan", Some(a)) => v < a,
                ("greaterThanOrEqual", Some(a)) => v >= a,
                ("lessThanOrEqual", Some(a)) => v <= a,
                _ => true,
            }
        };
        match rule.kind.as_str() {
            "list" => {
                let text = match value {
                    CellValue::Number(n) => ibis_calc::eval::number_to_text(*n),
                    CellValue::Text(t) => t.clone(),
                    CellValue::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
                    CellValue::Error(e) => e.as_str().to_string(),
                    CellValue::Empty => String::new(),
                };
                self.validation_list_items_in_book(rule).iter().any(|i| i.eq_ignore_ascii_case(&text))
            }
            "whole" => matches!(value, CellValue::Number(n) if n.fract() == 0.0 && compare(*n)),
            "decimal" | "date" | "time" => matches!(value, CellValue::Number(n) if compare(*n)),
            "textLength" => {
                let len = match value {
                    CellValue::Text(t) => t.chars().count(),
                    CellValue::Number(n) => ibis_calc::eval::number_to_text(*n).chars().count(),
                    _ => 0,
                };
                compare(len as f64)
            }
            _ => keeps_at(sheet, rule, value, row, column),
        }
    }

    /// Whether `value` may go into (`row`, `column`): `Err` with the
    /// message when the rule refuses it (style `stop`); `Ok` with a warning
    /// message and whether it is a `warning` confirmation (`true`) or `information` (`false`).
    pub(crate) fn check_entry(&self, row: u32, column: u32, value: &CellValue) -> Result<Option<(String, bool)>, String> {
        let Some((_, sheet)) = self.current_sheet() else { return Ok(None) };
        let Some(rule) = rule_at(sheet, row, column) else { return Ok(None) };
        if rule.kind == "list" && !self.keeps_validation_in_book(rule, value, row, column) {
            return Err("データ入力規則の制約があります。リストから選択してください。".to_string()); // i18n-ok: requested validation message
        }
        let Some((style, title, text)) = &rule.error else { return Ok(None) };
        if self.keeps_validation_in_book(rule, value, row, column) {
            return Ok(None);
        }
        let message = if text.is_empty() { crate::shell::i18n::text("fix.validation.value-does-not-match-data").to_string() } else { text.clone() };
        let message = if title.is_empty() { message } else { format!("{title}：{message}") }; // i18n-ok（区切りの記号）
        if style == "stop" { Err(message) } else { Ok(Some((message, style == "warning"))) }
    }

    /// Accept the warning-flagged entry ("はい" / Yes): keep the committed value and dismiss the confirmation card.
    pub(crate) fn validation_warning_yes(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.validation_warning = None;
        }
        cx.notify();
    }

    /// Reject the warning-flagged entry and resume editing ("いいえ" / No): undo the commit and reopen the inline editor with the typed text.
    pub(crate) fn validation_warning_no(&mut self, cx: &mut Context<Self>) {
        let Some(mut warn) = self.sheet_state_mut().and_then(|s| s.validation_warning.take()) else { return };
        self.document.undo();
        self.set_cell(warn.edit.column, warn.edit.row);
        warn.edit.mode = super::cell_edit::EditMode::Edit;
        if let Some(s) = self.sheet_state_mut() {
            s.cell_edit = Some(warn.edit);
        }
        self.sync_workbook_peers(cx);
        cx.notify();
    }

    /// Cancel the warning-flagged entry ("キャンセル" / Cancel): undo the commit and restore the previous cell value.
    pub(crate) fn validation_warning_cancel(&mut self, cx: &mut Context<Self>) {
        let Some(warn) = self.sheet_state_mut().and_then(|s| s.validation_warning.take()) else { return };
        self.document.undo();
        self.set_cell(warn.edit.column, warn.edit.row);
        self.sync_workbook_peers(cx);
        cx.notify();
    }

    /// Open a drop-down list pop-up with `items` at `position` for (`column`, `row`).
    pub(crate) fn open_cell_items_popup(&mut self, items: Vec<String>, column: u32, row: u32, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if items.is_empty() {
            return;
        }
        let slide = self.current_slide;
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let entries: std::rc::Rc<Vec<MenuEntry>> = std::rc::Rc::new(
            items
                .into_iter()
                .map(|item| {
                    let text = item.clone();
                    MenuItem::new("xlsx.validation-item", item)
                        .on(move |this, window, cx| {
                            let input = match ibis_calc::input::parse_input(&text, date1904) {
                                ibis_calc::input::Typed::Value(v, _) => CellInput::Value(v),
                                ibis_calc::input::Typed::Formula(_) => CellInput::Value(CellValue::Text(text.clone())),
                            };
                            if let Err(err) = this.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![CellEdit { row, column, input }] })]) {
                                log::warn!("choosing from the list rejected: {err}");
                            }
                            if let Some(s) = this.sheet_state_mut() {
                                s.list_popup = None;
                            }
                            this.focus_handle.focus(window, cx);
                            cx.notify();
                        })
                        .into()
                })
                .collect(),
        );
        let editor = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| fill_menu(menu.min_w(px(160.)), &entries, &editor, window, cx));
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(s) = this.sheet_state_mut() {
                s.list_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.wb_mut().list_popup = Some(ListPopup { menu, position, _dismiss: dismiss });
        cx.notify();
    }

    /// Open the selected cell's validation list.
    pub(crate) fn open_validation_list(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rule) = self.current_rule() else { return };
        let Some((column, row)) = self.current_cell() else { return };
        let items = self.validation_list_items_in_book(&rule);
        if items.is_empty() { return; }
        if self.cell_edit().is_none() {
            self.begin_cell_edit(super::cell_edit::EditMode::Enter, true, cx);
        }
        let original = self.cell_edit().map(|e| e.text.clone()).unwrap_or_default();
        if let Some(edit) = self.cell_edit_mut() { edit.anchor = Some(0); edit.caret = edit.text.encode_utf16().count(); }
        self.wb_mut().validation_choices = Some(ValidationChoices { items, selected: 0, selection_query: original.clone(), original, scroll: ScrollHandle::new(), bounds: Default::default() });
        self.focus_handle.focus(window, cx);
        let _ = (position, column, row);
        cx.notify();
    }

    /// Start prefix completion on ordinary typing, without moving IME focus.
    pub(crate) fn show_typed_validation_choices(&mut self) {
        if self.sheet_state().is_none_or(|s|s.validation_choices.is_some()) { return; }
        let Some(rule) = self.current_rule().filter(|r|r.kind=="list" && r.dropdown) else { return };
        let items = self.validation_list_items_in_book(&rule);
        if items.is_empty() { return; }
        self.wb_mut().validation_choices = Some(ValidationChoices {
            items, selected: 0, original: String::new(), selection_query: String::new(),
            scroll: ScrollHandle::new(), bounds: Default::default(),
        });
    }

    pub(crate) fn filtered_validation_choices(&self) -> Vec<String> {
        let Some(p) = self.sheet_state().and_then(|s| s.validation_choices.as_ref()) else { return Vec::new() };
        let query = self.cell_edit().map(|e| e.text.as_str()).unwrap_or("");
        let query = if query == p.original { "" } else { query };
        let query = query.to_lowercase();
        p.items.iter().filter(|s| s.to_lowercase().starts_with(&query)).cloned().collect()
    }

    pub(crate) fn validation_choice_index(&self, count: usize) -> usize {
        let Some(p) = self.sheet_state().and_then(|s|s.validation_choices.as_ref()) else { return 0 };
        if self.cell_edit().is_some_and(|e| e.text != p.selection_query) { 0 } else { p.selected.min(count.saturating_sub(1)) }
    }

    pub(crate) fn validation_list_key(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        if self.sheet_state().and_then(|s| s.validation_choices.as_ref()).is_none() { return false; }
        match key.key.as_str() {
            "up" | "down" => {
                let n = self.filtered_validation_choices().len();
                let selected = self.validation_choice_index(n);
                let query = self.cell_edit().map(|e|e.text.clone()).unwrap_or_default();
                let p = self.wb_mut().validation_choices.as_mut().unwrap();
                p.selection_query = query;
                p.selected = if n == 0 { 0 } else if key.key == "down" { (selected + 1) % n } else { (selected + n - 1) % n };
                if n > 0 { p.scroll.scroll_to_item(p.selected); }
                cx.notify(); true
            }
            "escape" => { self.wb_mut().validation_choices = None; self.cancel_cell_edit(cx); true }
            _ => false,
        }
    }

    pub(crate) fn validation_cell_bounds(&self, column: u32, row: u32) -> Option<Bounds<Pixels>> {
        let center = self.cell_center(column, row)?;
        let layout = self.slide_layout()?;
        let (_, sheet) = self.current_sheet()?;
        let width = sheet.columns.get(column as usize).copied().unwrap_or(sheet.default_column);
        let height = sheet.rows.get(row as usize).copied().unwrap_or(sheet.default_row);
        let size = size(layout.to_px(width), layout.to_px(height));
        Some(Bounds::new(point(center.x-size.width/2.,center.y-size.height/2.),size))
    }

    pub(crate) fn render_validation_choices(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let p = self.sheet_state()?.validation_choices.as_ref()?;
        let edit = self.cell_edit()?;
        let bounds = self.validation_cell_bounds(edit.column, edit.row)?;
        let side = f32::from(bounds.size.height).clamp(12.,20.);
        let items = self.filtered_validation_choices();
        if items.is_empty() { return None; }
        let selected = self.validation_choice_index(items.len());
        let labels = items.clone();
        let rows = items.into_iter().enumerate().map(|(i,text)| {
            crate::shell::chrome::dropdown_list::option(("validation-choice",i), false, i==selected, true)
                .child(div().flex_1().truncate().child(SharedString::from(text.clone())))
                .on_mouse_down(MouseButton::Left,cx.listener(move |e,_,_,cx| {
                    cx.stop_propagation();
                    e.wb_mut().validation_choices=None;
                    if let Some(edit)=e.cell_edit_mut(){edit.text=text.clone();}
                    e.commit_cell_edit(cx);
                })).into_any_element()
        }).collect();
        Some(crate::shell::chrome::dropdown_list::AnchoredDropdown {
            trigger: bounds, width: bounds.size.width+px(side+1.), rows, labels,
            scroll: p.scroll.clone(), measured: p.bounds.clone(),
        }.into_any_element())
    }

    /// Check whether "Pick from Drop-down List" (⌥↓ / Alt+↓) can be offered for the active cell.
    pub(crate) fn can_pick_from_list(&self) -> bool {
        if self.current_rule().is_some_and(|r| r.kind == "list") {
            return true;
        }
        let Some((column, row)) = self.current_cell() else { return false };
        let Some((_, sheet)) = self.current_sheet() else { return false };
        !column_pick_list_items(sheet, row, column).is_empty()
    }

    /// Excel's ⌥↓ (Alt+↓) / "Pick from Drop-down List": opens the validation list if
    /// a list rule is set, otherwise opens a drop-down of unique text items from the same column.
    pub(crate) fn pick_from_drop_down_list(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((column, row)) = self.current_cell() else { return };
        let position = self.cell_center(column, row).unwrap_or_else(|| point(px(100.), px(100.)));

        // Priority 1: Data validation list rule
        if self.current_rule().is_some_and(|r| r.kind == "list") {
            self.open_validation_list(position, window, cx);
            return;
        }

        // Priority 2: Column pick list items
        let Some((_, sheet)) = self.current_sheet() else { return };
        let items = column_pick_list_items(sheet, row, column);
        if !items.is_empty() {
            self.open_cell_items_popup(items, column, row, position, window, cx);
        }
    }

    /// Open the panel (データの入力規則), with the selected cell's rule.
    pub(crate) fn open_validation_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rule = self.current_rule();
        let input = |placeholder: &str, text: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text(placeholder)));
            state.update(cx, |i, cx| i.set_value(text, window, cx));
            state
        };
        let r = rule.clone().unwrap_or(Validation { kind: "none".into(), operator: "between".into(), allow_blank: true, dropdown: true, ..Default::default() });
        // Bounds of dates and times shown as typed (a serial number read back).
        let shown = |f: &Option<String>| f.as_deref().unwrap_or("").trim_matches('"').to_string();
        let kind = KINDS.iter().map(|(k, _)| *k).find(|k| *k == r.kind).unwrap_or("none");
        let operator = OPERATORS.iter().map(|(k, _)| *k).find(|k| *k == r.operator).unwrap_or("between");
        let (style, error_title, error_text) = r.error.clone().unwrap_or(("stop".into(), String::new(), String::new()));
        let (prompt_title, prompt_text) = r.prompt.clone().unwrap_or_default();
        let panel = ValidationPanel {
            kind,
            operator,
            items: input("fix.validation.value-range-formula-list-comma", shown(&r.formula1), window, cx),
            second: input("fix.validation.second-value-between", shown(&r.formula2), window, cx),
            allow_blank: r.allow_blank,
            dropdown: r.dropdown,
            apply_to_same: false,
            show_prompt: r.prompt.is_some(),
            prompt_title: input("_shell.title", prompt_title, window, cx),
            prompt: input("fix.validation.input-message-optional", prompt_text, window, cx),
            show_error: r.error.is_some() || rule.is_none(),
            style: STYLES.iter().map(|(k, _)| *k).find(|k| *k == style).unwrap_or("stop"),
            error_title: input("_shell.title", error_title, window, cx),
            error_text: input("fix.validation.error-alert", error_text, window, cx),
            message: String::new(),
            tab: 0,
            ui: Default::default(),
        };
        if let Some(s) = self.sheet_state_mut() {
            s.close_settings();
            s.validation = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    fn validation_message(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.validation.as_mut()) {
            p.message = text;
        }
        cx.notify();
    }

    pub(crate) fn set_validation_range_from_selection(&mut self, second_field: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.selected_range() else { return };
        let val = if r.first_row == r.last_row && r.first_column == r.last_column {
            format!("={}", ibis_doc_model::cell_name(r.first_column, r.first_row))
        } else {
            format!(
                "={}:{}",
                ibis_doc_model::cell_name(r.first_column, r.first_row),
                ibis_doc_model::cell_name(r.last_column, r.last_row)
            )
        };
        let input = self
            .sheet_state()
            .and_then(|s| s.validation.as_ref())
            .map(|p| if second_field { p.second.clone() } else { p.items.clone() });
        if let Some(input) = input {
            input.update(cx, |i, cx| i.set_value(val, window, cx));
            cx.notify();
        }
    }

    /// The panel's rule over the selected ranges (OK).
    pub(crate) fn apply_validation_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.validation.as_ref()) else { return };
        let apply_to_same = p.apply_to_same;
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let value = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let (first, second) = (value(&p.items), value(&p.second));
        // A bound as stored: a number (dates and times as their serial
        // numbers), a reference or formula as typed (without `=`).
        let bound = |text: &str| -> Option<String> {
            if text.is_empty() {
                return None;
            }
            if let Some(f) = text.strip_prefix('=') {
                return Some(f.to_string());
            }
            match ibis_calc::input::parse_input(text, date1904) {
                ibis_calc::input::Typed::Value(CellValue::Number(n), _) => Some(ibis_calc::eval::number_to_text(n)),
                _ => Some(text.to_string()),
            }
        };
        let kind = p.kind;
        let two = matches!(p.operator, "between" | "notBetween");
        let (formula1, formula2) = match kind {
            "none" => (None, None),
            "list" => {
                if first.is_empty() {
                    return self.validation_message(crate::shell::i18n::text("fix.validation.enter-source").into(), cx);
                }
                // A range/Defined Name (=A1:A5, =AllowedItems, A1:A5, or AllowedItems) or comma-separated items.
                let f = if let Some(range) = first.strip_prefix('=') {
                    range.trim().to_string()
                } else if !first.contains(',') && !first.starts_with('"') && self.resolve_range(&first).is_ok() {
                    first.clone()
                } else {
                    format!("\"{}\"", first.split(',').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(","))
                };
                (Some(f), None)
            }
            "custom" => {
                if first.is_empty() {
                    return self.validation_message(crate::shell::i18n::text("fix.validation.enter-formula").into(), cx);
                }
                (Some(first.trim_start_matches('=').to_string()), None)
            }
            _ => {
                let (a, b) = (bound(&first), bound(&second));
                if a.is_none() || (two && b.is_none()) {
                    return self.validation_message(crate::shell::i18n::text("_xlsx.enter-a-value").into(), cx);
                }
                (a, if two { b } else { None })
            }
        };
        let rule = Validation {
            kind: kind.into(),
            operator: if matches!(kind, "list" | "custom" | "none") { "between".into() } else { p.operator.into() },
            formula1,
            formula2,
            allow_blank: p.allow_blank,
            dropdown: p.dropdown,
            prompt: p.show_prompt.then(|| (value(&p.prompt_title), value(&p.prompt))).filter(|(t, m)| !t.is_empty() || !m.is_empty()),
            error: p.show_error.then(|| (p.style.to_string(), value(&p.error_title), value(&p.error_text))),
            ..Default::default()
        };
        let (slide, ranges) = (self.current_slide, self.target_validation_ranges(apply_to_same));
        if ranges.is_empty() {
            return;
        }
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetValidation { slide, ranges, rule: Some(rule) })]) {
            return self.validation_message(err.to_string(), cx);
        }
        self.status=Some(crate::shell::i18n::text("fix.validation.validation-rule-set").into());
        self.wb_mut().validation=None;
        cx.notify();
    }

    /// A list over the selected range (items `a,b,c`), with a message if given.
    pub(crate) fn set_list_validation(&mut self, items: &str, prompt: &str, cx: &mut Context<Self>) {
        let Some(range) = self.selected_range() else { return };
        let items: Vec<&str> = items.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        if items.is_empty() {
            return;
        }
        let rule = Validation {
            kind: "list".into(),
            operator: "between".into(),
            formula1: Some(format!("\"{}\"", items.join(","))),
            allow_blank: true,
            dropdown: true,
            prompt: (!prompt.is_empty()).then(|| (String::new(), prompt.to_string())),
            error: Some(("stop".into(), String::new(), String::new())),
            ..Default::default()
        };
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetValidation { slide, ranges: vec![range], rule: Some(rule) })]) {
            log::warn!("validation rejected: {err}");
        }
        cx.notify();
    }

    /// すべてクリア: the rules taken off the selected ranges (or all matching rules when `apply_to_same` is set).
    pub(crate) fn clear_validation(&mut self, cx: &mut Context<Self>) {
        let apply_to_same = self
            .sheet_state()
            .and_then(|s| s.validation.as_ref())
            .is_some_and(|p| p.apply_to_same);
        let ranges = self.target_validation_ranges(apply_to_same);
        if ranges.is_empty() {
            return;
        }
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetValidation { slide, ranges, rule: None })]) {
            log::warn!("validation rejected: {err}");
        }
        self.validation_message(crate::shell::i18n::text("fix.validation.validation-rules-cleared").into(), cx);
    }

    /// 入力規則のルールに違反しているセル (col, row) の一覧を取得する。
    pub(crate) fn invalid_cells(&self) -> Vec<(u32, u32)> {
        let Some((_, sheet)) = self.current_sheet() else { return Vec::new() };
        let mut invalid = Vec::new();
        for rule in &sheet.validations {
            for range in &rule.ranges {
                for r in range.first_row..=range.last_row {
                    for c in range.first_column..=range.last_column {
                        let value = sheet.cell(r, c).map_or(&CellValue::Empty, |cell| &cell.value);
                        if !self.keeps_validation_in_book(rule, value, r, c) {
                            invalid.push((c, r));
                        }
                    }
                }
            }
        }
        invalid.sort_unstable();
        invalid.dedup();
        invalid
    }

    /// 無効データのマーク表示 (Circle Invalid Data) を有効化する。
    pub(crate) fn circle_invalid_data(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.validation_circles = true;
        }
        cx.notify();
    }

    /// マークのクリア (Clear Validation Circles) を実行する。
    pub(crate) fn clear_validation_circles(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.validation_circles = false;
        }
        cx.notify();
    }

    pub(crate) fn validation_circles(&self) -> bool {
        self.sheet_state().is_some_and(|s| s.validation_circles)
    }

    /// ワークスペース上に無効データのマーク（赤い角丸楕円枠）を描画する。
    pub(crate) fn render_validation_circles(&self, scale: f32) -> Option<Div> {
        if !self.validation_circles() {
            return None;
        }
        let (shape, sheet) = self.current_sheet()?;
        let invalid = self.invalid_cells();
        if invalid.is_empty() {
            return None;
        }
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let at = |offsets: &[i64], i: u32| offsets[(i as usize).min(offsets.len() - 1)];
        let x = |c: u32| (shape.frame.x + at(&xs, c)) as f32 * scale;
        let y = |r: u32| (shape.frame.y + at(&ys, r)) as f32 * scale;

        let red = rgb(0xED1C24);
        let mut circles = div().absolute().top_0().left_0().size_full();
        for (c, r) in invalid {
            let (x0, x1, y0, y1) = (x(c), x(c + 1), y(r), y(r + 1));
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let pad = 2.0 * scale.max(1.0);
            circles = circles.child(
                div()
                    .absolute()
                    .left(px(x0 - pad))
                    .top(px(y0 - pad))
                    .w(px((x1 - x0) + pad * 2.0))
                    .h(px((y1 - y0) + pad * 2.0))
                    .border_2()
                    .border_color(red)
                    .rounded(px(10.0 * scale.max(1.0))),
            );
        }
        Some(circles)
    }

    pub(crate) fn validation_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        use crate::shell::i18n::text;
        use gpui_kit::component::h_flex;
        use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
        let p = self.sheet_state()?.validation.as_ref()?;
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
        let entity = cx.entity().downgrade();
        // The same common dropdown trigger and list sizing used throughout the application.
        let choose = |id: &'static str, list: &'static [(&'static str, &'static str)], current: &str, set: fn(&mut ValidationPanel, &'static str)| {
            let entries=list.iter().map(|&(key,name)|crate::shell::chrome::MenuItem::new(id,text(name)).checked(current==key).on(move|this,_,cx|{
                if let Some(p)=this.sheet_state_mut().and_then(|s|s.validation.as_mut()){set(p,key);}
                cx.notify();
            }).into()).collect();
            mark(id,div().w(px(224.)).max_w(px(224.)).child(crate::shell::chrome::Dropdown::from_entries(id,text(name_of(list,current)),entries,true,0,&cx.weak_entity()).into_any_element()).into_any_element())
        };
        type Flip = fn(&mut ValidationPanel);
        let toggle = |id: &'static str, on: bool, label: &str, flip: Flip| {
            mark(
                id,
                crate::shell::chrome::checkbox_button(id, text(label), on, false, cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.sheet_state_mut().and_then(|s| s.validation.as_mut()) {
                            flip(p);
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };
        let compared = !matches!(p.kind, "none" | "list" | "custom");
        // The dialog's three tabs.
        let tabs=crate::shell::chrome::tab_control_with_mode(["fix.pivot.settings","fix.validation.input-message","fix.validation.error-alert"].map(text),p.tab,"validation-tabs",crate::shell::chrome::TabMode::Intrinsic,|e,i,cx|{if let Some(p)=e.sheet_state_mut().and_then(|s|s.validation.as_mut()){p.tab=i;}cx.notify();},cx);
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0().child(tabs);
        if p.tab == 0 {
        body = body
            .child(div().text_sm().child(text("fix.validation.allow")))
            .child(choose("validation-kind", &KINDS, p.kind, |p, k| p.kind = k));
        if compared {
            body = body.child(div().text_sm().child(text("fix.pivot-options.data"))).child(choose("validation-operator", &OPERATORS, p.operator, |p, k| p.operator = k));
        }
        if p.kind != "none" {
            let label = match p.kind {
                "list" => "fix.validation.source",
                "custom" => "_xlsx.formula-2",
                _ if matches!(p.operator, "between" | "notBetween") => "xlsx.chart-axis-min",
                _ => "chart.labels.show-value",
            };
            body = body
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_sm().child(text(label))),
                )
                .child(mark("items", crate::shell::chrome::text_field(&p.items).full_width().into_any_element()));
            if compared && matches!(p.operator, "between" | "notBetween") {
                body = body
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_sm().child(text("_xlsx.max"))),
                    )
                    .child(mark("second", crate::shell::chrome::text_field(&p.second).full_width().into_any_element()));
            }
            body = body.child(toggle("validation-blank", p.allow_blank, "fix.validation.ignore-blank", |p| p.allow_blank = !p.allow_blank));
            if p.kind == "list" {
                body = body.child(toggle("validation-dropdown", p.dropdown, "fix.validation.cell-dropdown", |p| p.dropdown = !p.dropdown));
            }
        }
        body = body.child(toggle("validation-apply-same", p.apply_to_same, "xlsx.validation.apply-same", |p| p.apply_to_same = !p.apply_to_same));
        }
        if p.tab == 1 {
        body = body
            .child(toggle("validation-show-prompt", p.show_prompt, "fix.validation.show-input-message-when-cell", |p| p.show_prompt = !p.show_prompt));
        if p.show_prompt {
            body = body.child(mark("prompt-title", crate::shell::chrome::text_field(&p.prompt_title).full_width().into_any_element())).child(mark("prompt", crate::shell::chrome::text_field(&p.prompt).full_width().into_any_element()));
        }
        }
        if p.tab == 2 {
        body = body
            .child(toggle("validation-show-error", p.show_error, "fix.validation.show-error-alert-after-invalid", |p| p.show_error = !p.show_error));
        if p.show_error {
            body = body
                .child(h_flex().gap_2().items_center().child(div().text_sm().child(text("_shell.styles"))).child(choose("validation-style", &STYLES, p.style, |p, k| p.style = k)))
                .child(mark("error-title", crate::shell::chrome::text_field(&p.error_title).full_width().into_any_element()))
                .child(mark("error-text", crate::shell::chrome::text_field(&p.error_text).full_width().into_any_element()));
        }
        }
        body=body.children(crate::shell::chrome::panel_message(&p.message))
            .child(div().border_t_1().border_color(rgb(crate::shell::chrome::BORDER)).pt_2().child(text("xlsx.validation.tools")))
            .child(h_flex().gap_2().flex_wrap().children([
                mark("circle-invalid",crate::shell::chrome::action_button("validation-circle").label(text("_xlsx.circle-invalid-data")).on_click(cx.listener(|e,_,_,cx|{e.circle_invalid_data(cx);e.wb_mut().validation=None;cx.notify();})).into_any_element()),
                mark("clear-circles",crate::shell::chrome::action_button("validation-clear-circles").label(text("_xlsx.clear-validation-circles")).on_click(cx.listener(|e,_,_,cx|e.clear_validation_circles(cx))).into_any_element()),
                mark("next-invalid",crate::shell::chrome::action_button("validation-next-invalid").label(text("xlsx.validation.next-invalid")).on_click(cx.listener(|e,_,_,cx|{e.jump_to_next_invalid_cell(cx);e.wb_mut().validation=None;cx.notify();})).into_any_element()),
                mark("select-same",crate::shell::chrome::action_button("validation-select-same").label(text("xlsx.validation.select-same")).on_click(cx.listener(|e,_,_,cx|{e.select_validation_cells(true,cx);e.wb_mut().validation=None;cx.notify();})).into_any_element()),
            ]));
        let footer=crate::shell::chrome::action_bar()
            .action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("validation-set", "", crate::shell::chrome::ActionButtonKind::Primary).label("OK").on_click(cx.listener(|e,_,_,cx|e.apply_validation_panel(cx)))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx))
            .action(mark_action("clear", crate::shell::chrome::ActionDefinition::new("validation-clear", "", crate::shell::chrome::ActionButtonKind::Destructive).label(text("fix.validation.clear-all")).on_click(cx.listener(|e,_,_,cx|e.clear_validation(cx)))));
        Some(SidebarPanel::with_footer(text("_shell.data-validation"),body.into_any_element(),footer))
    }
}

#[cfg(test)]
mod tests {
    use super::{CellValue, Sheet, Validation, keeps};

    #[test]
    fn lists_and_numbers() {
        let sheet = Sheet::default();
        let list = Validation { kind: "list".into(), formula1: Some(r#""りんご,みかん""#.into()), ..Default::default() };
        assert!(keeps(&sheet, &list, &CellValue::Text("みかん".into())));
        assert!(!keeps(&sheet, &list, &CellValue::Text("ぶどう".into())));
        let whole = Validation { kind: "whole".into(), operator: "between".into(), formula1: Some("1".into()), formula2: Some("10".into()), ..Default::default() };
        assert!(keeps(&sheet, &whole, &CellValue::Number(5.0)));
        assert!(!keeps(&sheet, &whole, &CellValue::Number(5.5)));
        assert!(!keeps(&sheet, &whole, &CellValue::Number(11.0)));
    }

    #[test]
    fn test_column_pick_list_items() {
        let mut sheet = Sheet::default();
        sheet.cells.push(ibis_doc_model::SheetCell {
            row: 0,
            column: 0,
            text: "Tokyo".into(),
            value: CellValue::Text("Tokyo".into()),
            runs: None,
            phonetic: None,
            kind: ibis_doc_model::CellKind::Text,
            formula: None,
            style: 0,
        });
        sheet.cells.push(ibis_doc_model::SheetCell {
            row: 1,
            column: 0,
            text: "Osaka".into(),
            value: CellValue::Text("Osaka".into()),
            runs: None,
            phonetic: None,
            kind: ibis_doc_model::CellKind::Text,
            formula: None,
            style: 0,
        });
        sheet.cells.push(ibis_doc_model::SheetCell {
            row: 2,
            column: 0,
            text: "Nagoya".into(),
            value: CellValue::Text("Nagoya".into()),
            runs: None,
            phonetic: None,
            kind: ibis_doc_model::CellKind::Text,
            formula: None,
            style: 0,
        });
        sheet.cells.push(ibis_doc_model::SheetCell {
            row: 3,
            column: 0,
            text: "tokyo".into(), // Duplicate in different case
            value: CellValue::Text("tokyo".into()),
            runs: None,
            phonetic: None,
            kind: ibis_doc_model::CellKind::Text,
            formula: None,
            style: 0,
        });

        // Cell A5 (row 4, column 0) is empty; check pick list items from A5
        let items = super::column_pick_list_items(&sheet, 4, 0);
        // Should be sorted alphabetically (case-insensitive) and deduplicated
        assert_eq!(items, vec!["Nagoya", "Osaka", "Tokyo"]);
    }
}
