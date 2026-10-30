// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! オートフィルター: データ > フィルタを作成 puts ▼ buttons on the table's
//! headings. A button opens Excel's panel for the column: sort ascending,
//! descending or by colour, clear the column's filter, filter by colour, the
//! text / number / date filters (オートフィルター オプション, トップテン, the
//! date periods), and the list of the column's values with a search box,
//! (すべて選択) and (空白セル), then OK or キャンセル.

use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::sheet_command::SortBy;
use ibis_doc_model::{AutoFilter, CellRange, CellValue, ColumnFilter, Command, Rgb, SheetCommand};

use crate::shell::editor::SlideEditor;

/// What a column holds, for which filters its panel offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColumnKind {
    Text,
    Number,
    Date,
}

/// Whose filter a ▼ belongs to: the sheet's (`autoFilter`) or a table's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FilterTarget {
    Sheet,
    Table(usize),
}

/// A part of the panel opened under its heading (Excel's submenus).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Section {
    SortColor,
    Color,
    Conditions,
    /// 期間内の全日付: the quarters and months of any year.
    AllDates,
}

/// A comparison of オートフィルター オプション: what the list says, the
/// operator, and the wildcards around the value typed.
pub(crate) struct Condition {
    pub(crate) label: &'static str,
    pub(crate) op: &'static str,
    pub(crate) before: &'static str,
    pub(crate) after: &'static str,
}

const fn condition(label: &'static str, op: &'static str, before: &'static str, after: &'static str) -> Condition {
    Condition { label, op, before, after }
}

pub(crate) const CONDITIONS: &[Condition] = &[
    condition("_xlsx.equals", "equal", "", ""),
    condition("_xlsx.does-not-equal", "notEqual", "", ""),
    condition("_xlsx.greater-than-2", "greaterThan", "", ""),
    condition("_xlsx.greater-than-or-equal-to-2", "greaterThanOrEqual", "", ""),
    condition("_xlsx.less-than-2", "lessThan", "", ""),
    condition("_xlsx.less-than-or-equal-to-2", "lessThanOrEqual", "", ""),
    condition("_xlsx.begins-with", "equal", "", "*"),
    condition("_xlsx.does-not-begin-with", "notEqual", "", "*"),
    condition("_xlsx.ends-with", "equal", "*", ""),
    condition("_xlsx.does-not-end-with", "notEqual", "*", ""),
    condition("_xlsx.contains", "equal", "*", "*"),
    condition("_xlsx.does-not-contain", "notEqual", "*", "*"),
    condition("_xlsx.after", "greaterThan", "", ""),
    condition("_xlsx.on-or-after", "greaterThanOrEqual", "", ""),
    condition("_xlsx.before", "lessThan", "", ""),
    condition("_xlsx.on-or-before", "lessThanOrEqual", "", ""),
];

/// The comparisons the dialog lists for a kind of column.
fn conditions_for(kind: ColumnKind) -> &'static [usize] {
    match kind {
        ColumnKind::Text | ColumnKind::Number => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
        ColumnKind::Date => &[0, 1, 12, 13, 14, 15],
    }
}

/// The date periods of 日付フィルター (`dynamicFilter/@type`).
pub(crate) const PERIODS: &[(&str, &str)] = &[
    ("xlsx.cf-date.tomorrow", "tomorrow"),
    ("xlsx.cf-date.today", "today"),
    ("xlsx.cf-date.yesterday", "yesterday"),
    ("_xlsx.next-week", "nextWeek"),
    ("_xlsx.this-week", "thisWeek"),
    ("_xlsx.last-week", "lastWeek"),
    ("_xlsx.next-month", "nextMonth"),
    ("_xlsx.this-month", "thisMonth"),
    ("_xlsx.last-month", "lastMonth"),
    ("_xlsx.next-quarter", "nextQuarter"),
    ("_xlsx.this-quarter", "thisQuarter"),
    ("_xlsx.last-quarter", "lastQuarter"),
    ("_xlsx.next-year", "nextYear"),
    ("_xlsx.this-year", "thisYear"),
    ("_xlsx.last-year", "lastYear"),
    ("_xlsx.year-to-date", "yearToDate"),
];

/// オートフィルター オプション: two comparisons, both or either.
pub(crate) struct ConditionDialog {
    /// Indexes into [`CONDITIONS`]; the second `None`: unused.
    pub(crate) ops: [Option<usize>; 2],
    pub(crate) values: [Entity<InputState>; 2],
    pub(crate) and: bool,
    /// The comparison whose list is open.
    pub(crate) choosing: Option<usize>,
    pub(crate) error: Option<SharedString>,
}

/// トップテン オートフィルター.
pub(crate) struct TopDialog {
    pub(crate) top: bool,
    pub(crate) percent: bool,
    pub(crate) count: Entity<InputState>,
    pub(crate) error: Option<SharedString>,
}

pub(crate) enum Dialog {
    Condition(ConditionDialog),
    Top(TopDialog),
}

/// A ▼'s panel, while open.
pub(crate) struct FilterPanel {
    pub(crate) target: FilterTarget,
    pub(crate) column: u32,
    pub(crate) position: Point<Pixels>,
    pub(crate) heading: String,
    pub(crate) kind: ColumnKind,
    pub(crate) search: Entity<InputState>,
    /// The column's values as shown, in Excel's order; `""` for blanks.
    pub(crate) values: Rc<Vec<String>>,
    pub(crate) ticked: HashSet<String>,
    pub(crate) section: Option<Section>,
    pub(crate) dialog: Option<Dialog>,
    /// The fills (`None`: none) and text colours in the column, when more than one.
    pub(crate) fills: Vec<Option<Rgb>>,
    pub(crate) fonts: Vec<Rgb>,
    pub(crate) icons: Vec<(String, u32)>,
    _search: Subscription,
}

impl FilterPanel {
    /// The values the search leaves in the list.
    pub(crate) fn shown(&self, query: &str) -> Vec<String> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.values.as_ref().clone();
        }
        self.values.iter().filter(|v| !v.is_empty() && v.to_lowercase().contains(&q)).cloned().collect()
    }
}

/// How the list shows a value.
fn label(value: &str) -> String {
    if value.is_empty() { crate::shell::i18n::text("_xlsx.blanks").to_string() } else { value.into() }
}

/// A colour's name in the lists: its hex code.
fn color_label(color: Option<Rgb>, cell: bool) -> String {
    match color {
        Some(c) => format!("#{:06X}", c.0 & 0xFF_FFFF),
        None if cell => crate::shell::i18n::text("_shell.no-fill").to_string(),
        None => crate::shell::i18n::text("_shell.auto").to_string(),
    }
}

pub(crate) type Action = Rc<dyn Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>)>;

impl SlideEditor {
    /// The table around the selected cell (the cells next to each other with something in them).
    pub(crate) fn current_region(&self) -> Option<CellRange> {
        let (column, row) = self.current_cell()?;
        let (_, sheet) = self.current_sheet()?;
        // Index occupied coordinates in both directions. Boundary queries then
        // visit one tree range instead of rescanning every already-grown row.
        let occupied: std::collections::BTreeSet<(u32,u32)> = sheet.cells.iter()
            .filter(|cell|!cell.value.is_empty() || cell.formula.is_some())
            .map(|cell|(cell.row,cell.column)).collect();
        let columns: std::collections::BTreeSet<(u32,u32)> = occupied.iter().map(|&(r,c)|(c,r)).collect();
        let horizontal = |row, first, last| occupied.range((row,first)..=(row,last)).next().is_some();
        let vertical = |column, first, last| columns.range((column,first)..=(column,last)).next().is_some();
        let mut region = CellRange {first_row:row,last_row:row,first_column:column,last_column:column};
        loop {
            let old = region;
            if old.first_row > 0 && horizontal(old.first_row-1,old.first_column,old.last_column) {region.first_row-=1;}
            if old.last_row < ibis_calc::formula::MAX_ROW && horizontal(old.last_row+1,old.first_column,old.last_column) {region.last_row+=1;}
            if old.first_column > 0 && vertical(old.first_column-1,old.first_row,old.last_row) {region.first_column-=1;}
            if old.last_column < ibis_calc::formula::MAX_COLUMN && vertical(old.last_column+1,old.first_row,old.last_row) {region.last_column+=1;}
            if old == region {return Some(region);}
        }
    }

    /// フィルタを作成 (on the selected range, or the table around the
    /// selected cell), or take it off.
    pub(crate) fn toggle_auto_filter(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::AutoFilter, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let slide = self.current_slide;
        // In a table: its ▼ on or off, as Excel's フィルター does there.
        if let Some((column, row)) = self.current_cell()
            && let Some(index) = sheet.tables.iter().position(|t| (t.range.first_row..=t.range.last_row).contains(&row) && (t.range.first_column..=t.range.last_column).contains(&column))
        {
            let t = &sheet.tables[index];
            let filter = match t.filter {
                Some(_) => None,
                None => Some(AutoFilter { range: CellRange { last_row: t.range.last_row - t.totals_rows, ..t.range }, columns: Vec::new() }),
            };
            if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetTableFilter { slide, index, filter })]) {
                log::warn!("filter rejected: {err}");
            }
            self.wb_mut().filter_panel = None;
            return cx.notify();
        }
        let filter = match sheet.auto_filter {
            Some(_) => None,
            None => {
                let range = match self.selected_range() {
                    Some(r) if r.first_row != r.last_row => r,
                    _ => match self.current_region() {
                        Some(r) => r,
                        None => return,
                    },
                };
                Some(AutoFilter { range, columns: Vec::new() })
            }
        };
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetAutoFilter { slide, filter })]) {
            log::warn!("filter rejected: {err}");
        }
        self.wb_mut().filter_panel = None;
        cx.notify();
    }

    /// ⌘⌥L / Ctrl+Alt+L (データ / 編集 > 並べ替えとフィルター > 再適用):
    /// Re-evaluates existing auto-filter or table filters on current data.
    pub(crate) fn reapply_auto_filter(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let command = if let Some(f) = sheet.auto_filter.clone() {
            Some(SheetCommand::SetAutoFilter { slide, filter: Some(f) })
        } else if let Some((idx, t)) = sheet.tables.iter().enumerate().find(|(_, t)| t.filter.is_some()) {
            Some(SheetCommand::SetTableFilter { slide, index: idx, filter: t.filter.clone() })
        } else {
            None
        };
        if let Some(cmd) = command {
            if let Err(err) = self.document.execute(vec![Command::Sheet(cmd)]) {
                log::warn!("reapply filter rejected: {err}");
            }
            if let Some(s) = self.sheet_state_mut() {
                s.list_popup = None;
                s.filter_panel = None;
            }
            cx.notify();
        }
    }

    /// フィルターのクリア（データ / 編集 > 並べ替えとフィルター > クリア）:
    /// Clears all column criteria from active auto-filters or table filters without removing the filter arrows.
    pub(crate) fn clear_auto_filter(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::AutoFilter, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let slide = self.current_slide;
        let mut commands = Vec::new();
        if let Some(mut af) = sheet.auto_filter.clone() {
            if !af.columns.is_empty() {
                af.columns.clear();
                commands.push(Command::Sheet(SheetCommand::SetAutoFilter { slide, filter: Some(af) }));
            }
        }
        for (index, t) in sheet.tables.iter().enumerate() {
            if let Some(mut tf) = t.filter.clone() {
                if !tf.columns.is_empty() {
                    tf.columns.clear();
                    commands.push(Command::Sheet(SheetCommand::SetTableFilter { slide, index, filter: Some(tf) }));
                }
            }
        }
        if !commands.is_empty() {
            if let Err(err) = self.document.execute(commands) {
                log::warn!("clear filter rejected: {err}");
            }
            if let Some(s) = self.sheet_state_mut() {
                s.list_popup = None;
                s.filter_panel = None;
            }
            cx.notify();
        }
    }

    /// Filter column `column` by `filter` (`None`: all rows of it again).
    pub(crate) fn filter_column(&mut self, column: u32, filter: Option<ColumnFilter>, cx: &mut Context<Self>) {
        self.filter_column_in(FilterTarget::Sheet, column, filter, cx);
    }

    /// The filter of `target` on the current sheet.
    pub(crate) fn target_filter(&self, target: FilterTarget) -> Option<AutoFilter> {
        let (_, sheet) = self.current_sheet()?;
        match target {
            FilterTarget::Sheet => sheet.auto_filter.clone(),
            FilterTarget::Table(i) => sheet.tables.get(i)?.filter.clone(),
        }
    }

    /// Filter column `column` of `target`'s filter by `filter` (`None`: all
    /// rows of it again).
    pub(crate) fn filter_column_in(&mut self, target: FilterTarget, column: u32, filter: Option<ColumnFilter>, cx: &mut Context<Self>) {
        let Some(mut f) = self.target_filter(target) else { return };
        f.columns.retain(|(c, _)| *c != column);
        if let Some(x) = filter {
            f.columns.push((column, x));
        }
        let slide = self.current_slide;
        let command = match target {
            FilterTarget::Sheet => SheetCommand::SetAutoFilter { slide, filter: Some(f) },
            FilterTarget::Table(index) => SheetCommand::SetTableFilter { slide, index, filter: Some(f) },
        };
        if let Err(err) = self.document.execute(vec![Command::Sheet(command)]) {
            log::warn!("filter rejected: {err}");
        }
        if let Some(s) = self.sheet_state_mut() {
            s.list_popup = None;
            s.filter_panel = None;
        }
        cx.notify();
    }

    fn book_date1904(&self) -> bool {
        self.presentation().workbook.as_ref().is_some_and(|w| w.date1904)
    }

    /// A heading's ▼: its panel.
    #[allow(dead_code)]
    pub(crate) fn open_filter_menu(&mut self, column: u32, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.open_filter_menu_in(FilterTarget::Sheet, column, position, window, cx);
    }

    /// The ▼ of `target`'s heading `column`: its panel.
    pub(crate) fn open_filter_menu_in(&mut self, target: FilterTarget, column: u32, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let d1904 = self.book_date1904();
        let Some(f) = self.target_filter(target) else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        // The rows the other columns' filters leave, as Excel lists them.
        let others = AutoFilter { range: f.range, columns: f.columns.iter().filter(|(c, _)| *c != column).cloned().collect() };
        let hidden_by_others = ibis_doc_model::sheet_command::filtered_rows(sheet, &others, d1904);
        let last = f.range.last_row.min(sheet.rows.len().saturating_sub(1) as u32);
        let rows: Vec<u32> = ((f.range.first_row + 1)..=last).filter(|r| !hidden_by_others.contains(r)).collect();
        // Distinct values: numbers (and dates) by value, then text, blanks last.
        let mut seen = HashSet::new();
        let mut keyed: Vec<(u8, f64, String, String)> = Vec::new();
        let (mut numbers, mut dates, mut texts) = (0, 0, 0);
        let mut fills: Vec<Option<Rgb>> = Vec::new();
        let mut fonts: Vec<Rgb> = Vec::new();
        let mut icons: Vec<(String, u32)> = Vec::new();
        let icon_eval = ibis_doc_model::sheet_command::ColumnIconEvaluator::new(sheet, column);
        for &r in &rows {
            let cell = sheet.cell(r, column);
            let text = cell.map_or(String::new(), |c| c.text.clone());
            let style = cell.and_then(|c| sheet.styles.get(c.style as usize));
            let fill = style.and_then(|s| s.fill);
            if !fills.contains(&fill) {
                fills.push(fill);
            }
            let font = style.map_or(Rgb(0), |s| s.color);
            if !fonts.contains(&font) {
                fonts.push(font);
            }
            if let Some(ic) = icon_eval.icon_of(sheet, r, column) {
                if !icons.contains(&ic) {
                    icons.push(ic);
                }
            }
            let (rank, n) = match cell.map(|c| &c.value) {
                Some(CellValue::Number(n)) => {
                    if style.is_some_and(|s| ibis_calc::numfmt::is_date_format(&s.number_format)) {
                        dates += 1;
                    } else {
                        numbers += 1;
                    }
                    (0, *n)
                }
                _ if text.is_empty() => (3, 0.0),
                Some(CellValue::Bool(_)) => (2, 0.0),
                _ => {
                    texts += 1;
                    (1, 0.0)
                }
            };
            if seen.insert(text.clone()) {
                keyed.push((rank, n, ibis_calc::text_order::sort_key(&text, None), text));
            }
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        let values: Vec<String> = keyed.into_iter().map(|k| k.3).collect();
        let kind = if dates > 0 && dates >= numbers && dates >= texts {
            ColumnKind::Date
        } else if numbers > 0 && numbers >= texts {
            ColumnKind::Number
        } else {
            ColumnKind::Text
        };
        // Ticked: the values the column's filter keeps (all without one).
        let current = f.columns.iter().find(|(c, _)| *c == column).map(|(_, x)| x.clone());
        let ticked: HashSet<String> = match &current {
            None => values.iter().cloned().collect(),
            Some(ColumnFilter::Values(v)) => {
                let selected: HashSet<_> = v.iter().map(|v|v.to_lowercase()).collect();
                values.iter().filter(|x|selected.contains(&x.to_lowercase())).cloned().collect()
            },
            Some(_) => rows.iter().filter(|r| !sheet.filtered_rows.contains(r)).map(|&r| sheet.cell(r, column).map_or(String::new(), |c| c.text.clone())).collect(),
        };
        let heading = sheet.cell(f.range.first_row, column).map(|c| c.text.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| ibis_doc_model::cell_name(column, 0).trim_end_matches('1').to_string());
        if fills.len() < 2 {
            fills.clear();
        }
        if fonts.len() < 2 {
            fonts.clear();
        }
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("_xlsx.search")));
        search.update(cx, |i, cx| i.focus(window, cx));
        let subscription = cx.subscribe_in(&search, window, |this, _, event: &InputEvent, _window, cx| match event {
            InputEvent::Change => cx.notify(),
            InputEvent::PressEnter { .. } => this.filter_panel_ok(cx),
            _ => {}
        });
        let s = self.wb_mut();
        s.list_popup = None;
        s.filter_panel = Some(FilterPanel { target, column, position, heading, kind, search, values: Rc::new(values), ticked, section: None, dialog: None, fills, fonts, icons, _search: subscription });
        cx.notify();
    }

    pub(crate) fn close_filter_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.filter_panel = None;
        }
        cx.notify();
    }

    /// Tick or untick a value of the list (`None`: (すべて選択), or
    /// (検索結果をすべて選択) while searching).
    pub(crate) fn filter_panel_tick(&mut self, value: Option<&str>, on: bool, cx: &mut Context<Self>) {
        let query = self.wb().filter_panel.as_ref().map(|p| p.search.read(cx).value().to_string()).unwrap_or_default();
        let Some(p) = self.wb_mut().filter_panel.as_mut() else { return };
        let all = match value {
            Some(v) => vec![v.to_string()],
            None => p.shown(&query),
        };
        for v in all {
            if on {
                p.ticked.insert(v);
            } else {
                p.ticked.remove(&v);
            }
        }
        cx.notify();
    }

    /// OK: keep the ticked values (of those the search shows, while searching).
    pub(crate) fn filter_panel_ok(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.wb().filter_panel.as_ref() else { return };
        let query = p.search.read(cx).value().to_string();
        let keep: Vec<String> = p.shown(&query).into_iter().filter(|v| p.ticked.contains(v)).collect();
        if keep.is_empty() {
            return;
        }
        let (target, column) = (p.target, p.column);
        let filter = if query.trim().is_empty() && keep.len() == p.values.len() { None } else { Some(ColumnFilter::Values(keep)) };
        self.filter_column_in(target, column, filter, cx);
    }

    /// Open or close a part of the panel.
    pub(crate) fn filter_panel_section(&mut self, section: Section, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().filter_panel.as_mut() {
            p.section = if p.section == Some(section) { None } else { Some(section) };
        }
        cx.notify();
    }

    /// 昇順・降順・色で並べ替え: the filter's rows by the column.
    pub(crate) fn filter_panel_sort(&mut self, by: SortBy, cx: &mut Context<Self>) {
        let Some((target, column)) = self.wb().filter_panel.as_ref().map(|p| (p.target, p.column)) else { return };
        let Some(range) = self.target_filter(target).map(|f| f.range) else { return };
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SortBy { slide, range, keys: vec![(column, by)], header: true, match_case: false })]) {
            log::warn!("sort rejected: {err}");
        }
        self.close_filter_panel(cx);
    }

    /// The column's filter set straight from the panel (a period, the
    /// average, a colour; `None`: clear).
    pub(crate) fn filter_panel_apply(&mut self, filter: Option<ColumnFilter>, cx: &mut Context<Self>) {
        let Some((target, column)) = self.wb().filter_panel.as_ref().map(|p| (p.target, p.column)) else { return };
        self.filter_column_in(target, column, filter, cx);
    }

    /// Open オートフィルター オプション with the comparisons `ops` (both
    /// kept if `and`); ユーザー設定フィルター on a column already filtered so
    /// shows its comparisons.
    pub(crate) fn filter_panel_condition(&mut self, ops: [Option<usize>; 2], and: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((target, column)) = self.wb().filter_panel.as_ref().map(|p| (p.target, p.column)) else { return };
        let current = self.target_filter(target).and_then(|f| f.columns.iter().find(|(c, _)| *c == column).map(|(_, x)| x.clone()));
        let d1904 = self.book_date1904();
        let kind = self.wb().filter_panel.as_ref().map_or(ColumnKind::Text, |p| p.kind);
        let mut texts = [String::new(), String::new()];
        let (mut ops, mut and) = (ops, and);
        if let (Some(ColumnFilter::Custom { and: a, conditions }), [Some(0), None]) = (&current, ops) {
            and = *a;
            for (i, (op, value)) in conditions.iter().take(2).enumerate() {
                let (index, text) = decode_condition(op, value, kind, d1904);
                ops[i] = Some(index);
                texts[i] = text;
            }
        }
        let values = [0, 1].map(|i| {
            let input = cx.new(|cx| InputState::new(window, cx));
            let text = texts[i].clone();
            input.update(cx, |s, cx| s.set_value(text, window, cx));
            input
        });
        values[0].update(cx, |s, cx| s.focus(window, cx));
        if let Some(p) = self.wb_mut().filter_panel.as_mut() {
            p.dialog = Some(Dialog::Condition(ConditionDialog { ops, values, and, choosing: None, error: None }));
        }
        cx.notify();
    }

    /// Open トップテン オートフィルター.
    pub(crate) fn filter_panel_top(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = cx.new(|cx| InputState::new(window, cx));
        count.update(cx, |s, cx| {
            s.set_value("10", window, cx);
            s.focus(window, cx);
        });
        if let Some(p) = self.wb_mut().filter_panel.as_mut() {
            p.dialog = Some(Dialog::Top(TopDialog { top: true, percent: false, count, error: None }));
        }
        cx.notify();
    }

    /// The open dialog's OK.
    pub(crate) fn filter_dialog_ok(&mut self, cx: &mut Context<Self>) {
        let d1904 = self.book_date1904();
        let Some(p) = self.wb().filter_panel.as_ref() else { return };
        let kind = p.kind;
        let result: Result<ColumnFilter, &str> = match &p.dialog {
            Some(Dialog::Condition(d)) => {
                let mut conditions = Vec::new();
                let mut bad = false;
                for i in 0..2 {
                    let Some(op) = d.ops[i] else { continue };
                    let text = d.values[i].read(cx).value().to_string();
                    if text.is_empty() && i == 1 {
                        continue;
                    }
                    match encode_condition(op, &text, kind, d1904) {
                        Some(c) => conditions.push(c),
                        None => bad = true,
                    }
                }
                if bad || conditions.is_empty() {
                    Err(if kind == ColumnKind::Date { "_xlsx.please-enter-a-date-such-as-2026-10-7" } else { "_xlsx.please-enter-a-value" })
                } else {
                    Ok(ColumnFilter::Custom { and: d.and && conditions.len() == 2, conditions })
                }
            }
            Some(Dialog::Top(d)) => {
                let text = d.count.read(cx).value().trim().to_string();
                let most = if d.percent { 100.0 } else { 500.0 };
                match text.parse::<f64>() {
                    Ok(n) if (1.0..=most).contains(&n) => Ok(ColumnFilter::Top { top: d.top, percent: d.percent, value: text }),
                    _ => Err(if d.percent { "_xlsx.please-enter-a-number-from-1-to-100" } else { "_xlsx.please-enter-a-number-from-1-to-500" }),
                }
            }
            None => return,
        };
        match result {
            Ok(filter) => self.filter_panel_apply(Some(filter), cx),
            Err(message) => {
                if let Some(p) = self.wb_mut().filter_panel.as_mut() {
                    match &mut p.dialog {
                        Some(Dialog::Condition(d)) => d.error = Some(crate::shell::i18n::text(message)),
                        Some(Dialog::Top(d)) => d.error = Some(crate::shell::i18n::text(message)),
                        None => {}
                    }
                }
                cx.notify();
            }
        }
    }

    pub(crate) fn with_condition(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut ConditionDialog)) {
        if let Some(Dialog::Condition(d)) = self.wb_mut().filter_panel.as_mut().and_then(|p| p.dialog.as_mut()) {
            f(d);
        }
        cx.notify();
    }

    pub(crate) fn with_top(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut TopDialog)) {
        if let Some(Dialog::Top(d)) = self.wb_mut().filter_panel.as_mut().and_then(|p| p.dialog.as_mut()) {
            f(d);
        }
        cx.notify();
    }

    /// The open panel, drawn where its ▼ was clicked.
    pub(crate) fn render_filter_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.sheet_state()?.filter_panel.as_ref()?;
        let theme = cx.theme().clone();
        let body = match &p.dialog {
            Some(Dialog::Condition(d)) => self.render_condition_dialog(p, d, cx),
            Some(Dialog::Top(d)) => self.render_top_dialog(d, cx),
            None => self.render_filter_menu(p, cx),
        };
        let panel = v_flex()
            .id("filter-panel")
            .w(px(if p.dialog.is_some() { 360. } else { 280. }))
            .p_2()
            .gap_1()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .rounded_md()
            .shadow_lg()
            .text_size(px(12.))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| this.close_filter_panel(cx)))
            .child(body);
        Some(deferred(anchored().position(p.position).snap_to_window_with_margin(px(8.)).child(panel)).with_priority(2).into_any_element())
    }

    /// One clickable line of the panel.
    pub(crate) fn panel_row(&self, id: impl Into<SharedString>, text: impl Into<SharedString>, enabled: bool, action: Action, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = cx.theme().clone();
        div()
            .id(ElementId::Name(id.into()))
            .px_2()
            .py(px(2.))
            .rounded_sm()
            .child(crate::shell::i18n::text(&text.into()))
            .when(!enabled, |d| d.text_color(theme.muted_foreground))
            .when(enabled, |d| d.cursor_pointer().hover(|s| s.bg(theme.accent)).on_click(cx.listener(move |this, _, window, cx| action(this, window, cx))))
    }

    fn render_filter_menu(&self, p: &FilterPanel, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let filtered = self.target_filter(p.target).is_some_and(|f| f.columns.iter().any(|(c, _)| *c == p.column));
        let (asc, desc, conditions) = match p.kind {
            ColumnKind::Text => ("_xlsx.sort-a-to-z", "_xlsx.sort-z-to-a", "_xlsx.text-filters"),
            ColumnKind::Number => ("_xlsx.sort-smallest-to-largest", "_xlsx.sort-largest-to-smallest", "_xlsx.number-filters"),
            ColumnKind::Date => ("_xlsx.sort-oldest-to-newest", "_xlsx.sort-newest-to-oldest", "_xlsx.date-filters"),
        };
        let has_colors = !p.fills.is_empty() || !p.fonts.is_empty() || !p.icons.is_empty();
        let arrow = |open: bool| if open { " ▾" } else { " ▸" };
        let mut menu = v_flex()
            .child(self.panel_row("filter-asc", asc, true, Rc::new(|this, _, cx| this.filter_panel_sort(SortBy::Value(true), cx)), cx))
            .child(self.panel_row("filter-desc", desc, true, Rc::new(|this, _, cx| this.filter_panel_sort(SortBy::Value(false), cx)), cx))
            .child(self.panel_row("filter-sort-color", format!("{}{}", crate::shell::i18n::text("_xlsx.sort-by-color"), arrow(p.section == Some(Section::SortColor))), has_colors, Rc::new(|this, _, cx| this.filter_panel_section(Section::SortColor, cx)), cx));
        if p.section == Some(Section::SortColor) {
            menu = menu.child(self.color_rows(p, true, cx));
        }
        menu = menu
            .child(div().h(px(1.)).my_1().bg(theme.border))
            .child(self.panel_row("filter-clear", crate::shell::i18n::format("_xlsx.clear-filter-from-name", &[("name", &p.heading)]), filtered, Rc::new(|this, _, cx| this.filter_panel_apply(None, cx)), cx))
            .child(self.panel_row("filter-color", format!("{}{}", crate::shell::i18n::text("_xlsx.filter-by-color"), arrow(p.section == Some(Section::Color))), has_colors, Rc::new(|this, _, cx| this.filter_panel_section(Section::Color, cx)), cx));
        if p.section == Some(Section::Color) {
            menu = menu.child(self.color_rows(p, false, cx));
        }
        menu = menu.child(self.panel_row("filter-conditions", format!("{}{}", crate::shell::i18n::text(conditions), arrow(matches!(p.section, Some(Section::Conditions | Section::AllDates)))), true, Rc::new(|this, _, cx| this.filter_panel_section(Section::Conditions, cx)), cx));
        if matches!(p.section, Some(Section::Conditions | Section::AllDates)) {
            menu = menu.child(self.condition_rows(p, cx));
        }
        // The search box and the list (only the lines in view are built).
        let query = p.search.read(cx).value().to_string();
        let searching = !query.trim().is_empty();
        let shown = Rc::new(p.shown(&query));
        let ticked = Rc::new(p.ticked.clone());
        let all_ticked = shown.iter().all(|v| ticked.contains(v));
        let none_ticked = !shown.iter().any(|v| ticked.contains(v));
        let editor = cx.entity().downgrade();
        let count = shown.len() + 1;
        let list = uniform_list("filter-values", count, move |range, _, _| {
            range
                .map(|i| {
                    let editor = editor.clone();
                    let (text, value, checked) = if i == 0 {
                        (crate::shell::i18n::text(if searching { "_xlsx.select-all-search-results" } else { "_xlsx.select-all" }).to_string(), None, all_ticked)
                    } else {
                        let v = shown[i - 1].clone();
                        let on = ticked.contains(&v);
                        (label(&v), Some(v), on)
                    };
                    div().h(px(22.)).px_1().flex().items_center().child(Checkbox::new(("filter-value", i)).label(text).checked(checked).on_click(move |on: &bool, _, cx| {
                        let on = *on;
                        let value = value.clone();
                        editor.update(cx, |this, cx| this.filter_panel_tick(value.as_deref(), on, cx)).ok();
                    }))
                })
                .collect()
        })
        .h_full();
        menu.child(div().h(px(1.)).my_1().bg(theme.border))
            .child(crate::shell::chrome::text_field(&p.search).full_width())
            .child(div().h(px(200.)).border_1().border_color(theme.border).rounded_sm().child(list))
            .child(
                crate::shell::chrome::action_bar_host((crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("filter-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label("OK").disabled(none_ticked).on_click(cx.listener(|this, _, _, cx| this.filter_panel_ok(cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("filter-cancel", "", crate::shell::chrome::ActionButtonKind::Normal).label(crate::shell::i18n::text("_shell.cancel")).on_click(cx.listener(|this, _, _, cx| this.close_filter_panel(cx)))))),
            )
            .into_any_element()
    }

    /// The colours of the column, to sort (`sort`) or filter by.
    fn color_rows(&self, p: &FilterPanel, sort: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let mut list = v_flex().pl_3();
        let swatch = |c: Option<Rgb>| div().flex_shrink_0().size(px(12.)).border_1().border_color(theme.border).when_some(c, |d, c| d.bg(rgb(c.0 & 0xFF_FFFF)));
        if !p.fills.is_empty() {
            list = list.child(div().text_color(theme.muted_foreground).child(crate::shell::i18n::text(if sort { "_xlsx.sort-by-cell-color" } else { "_xlsx.filter-by-cell-color" })));
            for (i, &c) in p.fills.iter().enumerate() {
                let action: Action = if sort {
                    Rc::new(move |this, _, cx| this.filter_panel_sort(SortBy::CellColor { color: c, top: true }, cx))
                } else {
                    Rc::new(move |this, _, cx| this.filter_panel_apply(Some(ColumnFilter::Color { cell: true, color: c }), cx))
                };
                list = list.child(self.panel_row(format!("filter-fill-{sort}-{i}"), color_label(c, true), true, action, cx).flex().gap_2().items_center().child(swatch(c)));
            }
        }
        if !p.fonts.is_empty() {
            list = list.child(div().text_color(theme.muted_foreground).child(crate::shell::i18n::text(if sort { "_xlsx.sort-by-font-color" } else { "_xlsx.filter-by-font-color" })));
            for (i, &c) in p.fonts.iter().enumerate() {
                let color = if c == Rgb(0) { None } else { Some(c) };
                let action: Action = if sort {
                    Rc::new(move |this, _, cx| this.filter_panel_sort(SortBy::FontColor { color, top: true }, cx))
                } else {
                    Rc::new(move |this, _, cx| this.filter_panel_apply(Some(ColumnFilter::Color { cell: false, color }), cx))
                };
                list = list.child(self.panel_row(format!("filter-font-{sort}-{i}"), color_label(color, false), true, action, cx).flex().gap_2().items_center().child(swatch(Some(c))));
            }
        }
        if !p.icons.is_empty() {
            list = list.child(div().text_color(theme.muted_foreground).child(crate::shell::i18n::text(if sort { "_xlsx.sort-by-conditional-formatting-icon" } else { "_xlsx.filter-by-conditional-formatting-icon" })));
            for (i, (set, id)) in p.icons.iter().enumerate() {
                let (icon_set, icon_id) = (set.clone(), *id);
                let action: Action = if sort {
                    let set_c = icon_set.clone();
                    Rc::new(move |this, _, cx| this.filter_panel_sort(SortBy::Icon { icon_set: set_c.clone(), icon_id, top: true }, cx))
                } else {
                    let set_c = icon_set.clone();
                    Rc::new(move |this, _, cx| this.filter_panel_apply(Some(ColumnFilter::Icon { icon_set: set_c.clone(), icon_id }), cx))
                };
                let icon_info = crate::xlsx::conditional::icons_of(&icon_set);
                let icon_color = icon_info.get(icon_id as usize).map(|(c, _)| *c);
                list = list.child(self.panel_row(format!("filter-icon-{sort}-{i}"), format!("{icon_set} ({icon_id})"), true, action, cx).flex().gap_2().items_center().child(swatch(icon_color)));
            }
        }
        list.into_any_element()
    }

    /// The items of テキスト / 数値 / 日付フィルター.
    fn condition_rows(&self, p: &FilterPanel, cx: &mut Context<Self>) -> AnyElement {
        let mut list = v_flex().pl_3();
        let dialog = |ops: [Option<usize>; 2], and: bool| -> Action { Rc::new(move |this, window, cx| this.filter_panel_condition(ops, and, window, cx)) };
        let apply = |filter: ColumnFilter| -> Action { Rc::new(move |this, _, cx| this.filter_panel_apply(Some(filter.clone()), cx)) };
        let comparisons: &[(usize, &str)] = match p.kind {
            ColumnKind::Text => &[(0, "_xlsx.equals-2"), (1, "_xlsx.does-not-equal-2"), (6, "_xlsx.begins-with-2"), (8, "_xlsx.ends-with-2"), (10, "_xlsx.contains-2"), (11, "_xlsx.does-not-contain-2")],
            ColumnKind::Number => &[(0, "_xlsx.equals-2"), (1, "_xlsx.does-not-equal-2"), (2, "_xlsx.greater-than-3"), (3, "_xlsx.greater-than-or-equal-to-3"), (4, "_xlsx.less-than-3"), (5, "_xlsx.less-than-or-equal-to-3")],
            ColumnKind::Date => &[(0, "_xlsx.equals-2"), (14, "_xlsx.before-2"), (12, "_xlsx.after-2")],
        };
        for &(op, text) in comparisons {
            list = list.child(self.panel_row(format!("filter-op-{op}"), text, true, dialog([Some(op), None], true), cx));
        }
        match p.kind {
            ColumnKind::Number => {
                list = list
                    .child(self.panel_row("filter-between", "_xlsx.between-2", true, dialog([Some(3), Some(5)], true), cx))
                    .child(self.panel_row("filter-top", "_xlsx.top-10", true, Rc::new(|this, window, cx| this.filter_panel_top(window, cx)), cx))
                    .child(self.panel_row("filter-above", "_xlsx.above-average", true, apply(ColumnFilter::AboveAverage(true)), cx))
                    .child(self.panel_row("filter-below", "_xlsx.below-average", true, apply(ColumnFilter::AboveAverage(false)), cx));
            }
            ColumnKind::Date => {
                list = list.child(self.panel_row("filter-between", "_xlsx.between-2", true, dialog([Some(13), Some(15)], true), cx));
                for &(text, kind) in PERIODS {
                    list = list.child(self.panel_row(format!("filter-period-{kind}"), text, true, apply(ColumnFilter::Period(kind.into())), cx));
                }
                let open = p.section == Some(Section::AllDates);
                list = list.child(self.panel_row(
                    "filter-all-dates",
                    if open { "_xlsx.all-dates-in-the-period" } else { "_xlsx.all-dates-in-the-period-2" },
                    true,
                    Rc::new(|this, _, cx| {
                        if let Some(p) = this.wb_mut().filter_panel.as_mut() {
                            p.section = Some(if p.section == Some(Section::AllDates) { Section::Conditions } else { Section::AllDates });
                        }
                        cx.notify();
                    }),
                    cx,
                ));
                if open {
                    let mut months = v_flex().pl_3();
                    for q in 1..=4 {
                        months = months.child(self.panel_row(format!("filter-q{q}"), crate::shell::i18n::format("_xlsx.quarter-q", &[("q", &q)]), true, apply(ColumnFilter::Period(format!("Q{q}"))), cx));
                    }
                    for m in 1..=12 {
                        months = months.child(self.panel_row(format!("filter-m{m}"), crate::shell::i18n::format("_xlsx.month-m", &[("m", &m)]), true, apply(ColumnFilter::Period(format!("M{m}"))), cx));
                    }
                    list = list.child(months);
                }
            }
            ColumnKind::Text => {}
        }
        list.child(self.panel_row("filter-custom", "_xlsx.custom-filter", true, dialog([Some(0), None], true), cx)).into_any_element()
    }

    fn render_condition_dialog(&self, p: &FilterPanel, d: &ConditionDialog, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let mut body = v_flex().gap_2().child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("_xlsx.autofilter-options"))).child(crate::shell::i18n::format("_xlsx.show-rows-where-name", &[("name", &p.heading)]));
        for i in 0..2 {
            let current = crate::shell::i18n::text(d.ops[i].map_or("_xlsx.none", |o| CONDITIONS[o].label));
            body = body.child(
                h_flex()
                    .gap_2()
                    .child(crate::shell::chrome::action_button(("filter-dialog-op", i)).small().w(px(160.)).label(format!("{current} ▾")).on_click(cx.listener(move |this, _, _, cx| this.with_condition(cx, |d| d.choosing = if d.choosing == Some(i) { None } else { Some(i) }))))
                    .child(div().flex_1().child(crate::shell::chrome::text_field(&d.values[i]).full_width())),
            );
            if d.choosing == Some(i) {
                let mut list = v_flex().border_1().border_color(theme.border).rounded_sm();
                if i == 1 {
                    list = list.child(self.panel_row("filter-dialog-none", "_xlsx.none", true, Rc::new(|this, _, cx| this.with_condition(cx, |d| {
                        d.ops[1] = None;
                        d.choosing = None;
                    })), cx));
                }
                for &o in conditions_for(p.kind) {
                    list = list.child(self.panel_row(format!("filter-dialog-{i}-{o}"), CONDITIONS[o].label, true, Rc::new(move |this, _, cx| this.with_condition(cx, |d| {
                        d.ops[i] = Some(o);
                        d.choosing = None;
                    })), cx));
                }
                body = body.child(list);
            }
            if i == 0 {
                body = body.child(
                    h_flex()
                        .gap_3()
                        .pl_2()
                        .child(Checkbox::new("filter-dialog-and").label("AND").checked(d.and).on_click(cx.listener(|this, _, _, cx| this.with_condition(cx, |d| d.and = true))))
                        .child(Checkbox::new("filter-dialog-or").label("OR").checked(!d.and).on_click(cx.listener(|this, _, _, cx| this.with_condition(cx, |d| d.and = false)))),
                );
            }
        }
        body.child(div().text_color(theme.muted_foreground).child(crate::shell::i18n::text("_xlsx.use-to-represent-any-single-character-use-to-rep")))
            .when_some(d.error.clone(), |b, e| b.child(div().text_color(theme.danger).child(e)))
            .child(
                crate::shell::chrome::action_bar_host((crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("filter-dialog-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label("OK").on_click(cx.listener(|this, _, _, cx| this.filter_dialog_ok(cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("filter-dialog-cancel", "", crate::shell::chrome::ActionButtonKind::Normal).label(crate::shell::i18n::text("_shell.cancel")).on_click(cx.listener(|this, _, _, cx| this.close_filter_panel(cx)))))),
            )
            .into_any_element()
    }

    fn render_top_dialog(&self, d: &TopDialog, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        v_flex()
            .gap_2()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("_xlsx.top-10-autofilter")))
            .child(
                h_flex()
                    .gap_2()
                    .child(crate::shell::chrome::action_button("filter-top-which").small().label(crate::shell::i18n::text(if d.top { "_xlsx.top" } else { "_xlsx.bottom" })).on_click(cx.listener(|this, _, _, cx| this.with_top(cx, |d| d.top = !d.top))))
                    .child(div().w(px(70.)).child(
                        // Applied by OK (not on Enter): a step only changes the box. Excel: 1–500 items or percent.
                        crate::shell::chrome::text_field(&d.count).full_width().spinner("filter-top-count", crate::shell::chrome::SpinnerSpec::new(1.0).range(1.0, 500.0).integer_only(), cx, |_, _, _, _| {}),
                    ))
                    .child(crate::shell::chrome::action_button("filter-top-unit").small().label(crate::shell::i18n::text(if d.percent { "_xlsx.percent" } else { "_xlsx.items" })).on_click(cx.listener(|this, _, _, cx| this.with_top(cx, |d| d.percent = !d.percent)))),
            )
            .when_some(d.error.clone(), |b, e| b.child(div().text_color(theme.danger).child(e)))
            .child(
                crate::shell::chrome::action_bar_host((crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("filter-top-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label("OK").on_click(cx.listener(|this, _, _, cx| this.filter_dialog_ok(cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("filter-top-cancel", "", crate::shell::chrome::ActionButtonKind::Normal).label(crate::shell::i18n::text("_shell.cancel")).on_click(cx.listener(|this, _, _, cx| this.close_filter_panel(cx)))))),
            )
            .into_any_element()
    }

    /// The ▼ buttons of the filter's headings, laid over the headers' area
    /// (window positions from where the headers were painted).
    pub(crate) fn filter_buttons(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some((_, sheet)) = self.current_sheet() else { return Vec::new() };
        let Some(g) = self.sheet_state().map(|s| s.header_geometry.borrow().clone()) else { return Vec::new() };
        let mut filters: Vec<(FilterTarget, AutoFilter)> = sheet.auto_filter.clone().map(|f| (FilterTarget::Sheet, f)).into_iter().collect();
        filters.extend(sheet.tables.iter().enumerate().filter_map(|(i, t)| t.filter.clone().filter(|_| t.header_rows > 0).map(|f| (FilterTarget::Table(i), f))));
        let (ox, oy) = g.origin;
        let mut out = Vec::new();
        for (target, f) in filters {
            let Some(&(_, top, bottom)) = g.rows.iter().find(|r| r.0 == f.range.first_row) else { continue };
            let side = (bottom - top).clamp(12.0, 18.0);
            let key = match target {
                FilterTarget::Sheet => 0,
                FilterTarget::Table(i) => i + 1,
            };
            for &(column, _, right) in g.columns.iter().filter(|(c, _, _)| (f.range.first_column..=f.range.last_column).contains(c)) {
                let filtered = f.columns.iter().any(|(c, _)| *c == column);
                out.push(
                    div()
                        .id(SharedString::from(format!("filter-button-{key}-{column}")))
                        .absolute()
                        .left(px(right - ox - side - 2.0))
                        .top(px(bottom - oy - side - 2.0))
                        .size(px(side))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgb(0xFFFFFF))
                        .border_1()
                        .border_color(rgb(0xA6A6A6))
                        .text_size(px(side * 0.55))
                        .text_color(if filtered { rgb(0x217346) } else { rgb(0x404040) })
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_filter_menu_in(target, column, e.position, window, cx);
                        }))
                        .child(if filtered { "▼●" } else { "▼" })
                        .into_any_element(),
                );
            }
        }
        out
    }
}

/// A comparison of the dialog as the file has it: the operator, and the
/// value typed with the condition's wildcards (a date as its serial, a
/// number compared by size as a number). `None` if a date or number cannot
/// be read, or a size comparison has no value.
pub(crate) fn encode_condition(op: usize, text: &str, kind: ColumnKind, date1904: bool) -> Option<(String, String)> {
    let c = &CONDITIONS[op];
    let plain = c.before.is_empty() && c.after.is_empty();
    let ordered = matches!(c.op, "greaterThan" | "greaterThanOrEqual" | "lessThan" | "lessThanOrEqual");
    if text.is_empty() && ordered {
        return None;
    }
    let value = match kind {
        ColumnKind::Date if plain && !text.is_empty() => ibis_calc::number_to_xml(ibis_calc::eval::convert::parse_number_text(text, date1904)?.floor()),
        ColumnKind::Number if ordered => ibis_calc::number_to_xml(ibis_calc::eval::convert::parse_number_text(text, date1904)?),
        _ => format!("{}{}{}", c.before, text, c.after),
    };
    Some((c.op.to_string(), value))
}

/// The dialog's comparison (index into [`CONDITIONS`]) and text for one
/// read from the file.
pub(crate) fn decode_condition(op: &str, value: &str, kind: ColumnKind, date1904: bool) -> (usize, String) {
    let before = value.len() > 1 && value.starts_with('*');
    let after = value.len() > 1 && value.ends_with('*') && !value.ends_with("~*");
    let core = value.get(usize::from(before)..value.len() - usize::from(after)).unwrap_or(value);
    let index = match (kind, op) {
        (ColumnKind::Date, "greaterThan") => 12,
        (ColumnKind::Date, "greaterThanOrEqual") => 13,
        (ColumnKind::Date, "lessThan") => 14,
        (ColumnKind::Date, "lessThanOrEqual") => 15,
        _ => CONDITIONS.iter().position(|c| c.op == op && (c.before == "*") == before && (c.after == "*") == after).unwrap_or(0),
    };
    let text = match (kind, core.parse::<f64>()) {
        (ColumnKind::Date, Ok(n)) => {
            let (y, m, d) = ibis_calc::eval::convert::date_from_serial(n.floor() as i64, date1904);
            format!("{y}/{m}/{d}")
        }
        _ => core.to_string(),
    };
    (index, text)
}

#[cfg(test)]
mod tests {
    use super::{ColumnKind, decode_condition, encode_condition};

    #[test]
    fn conditions_are_written_as_excel_writes_them() {
        let text = |op, s: &str| encode_condition(op, s, ColumnKind::Text, false).unwrap();
        assert_eq!(text(10, "店"), ("equal".into(), "*店*".into()));
        assert_eq!(text(7, "駅"), ("notEqual".into(), "駅*".into()));
        assert_eq!(text(8, "店"), ("equal".into(), "*店".into()));
        let date = encode_condition(13, "2026/10/1", ColumnKind::Date, false).unwrap();
        assert_eq!(date, ("greaterThanOrEqual".into(), "46296".into()));
        assert_eq!(encode_condition(2, "1,000", ColumnKind::Number, false).unwrap(), ("greaterThan".into(), "1000".into()));
        assert_eq!(decode_condition("greaterThanOrEqual", "46296", ColumnKind::Date, false), (13, "2026/10/1".into()));
        assert_eq!(decode_condition("notEqual", "*店*", ColumnKind::Text, false), (11, "店".into()));
        assert_eq!(decode_condition("equal", "駅*", ColumnKind::Text, false), (6, "駅".into()));
        assert!(encode_condition(13, "あした", ColumnKind::Date, false).is_none());
        assert!(encode_condition(2, "", ColumnKind::Number, false).is_none());
    }
}
