// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > 小計…: グループごとの小計行と総計行の挿入、アウトラインの自動生成。
//! Subtotal panel (`SheetCommand::ApplySubtotal`, `SheetCommand::RemoveSubtotal`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellRange, Command, SheetCommand, SlideId, SubtotalParams};

use super::dedupe_edit::{column_name, parse_range};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// Built-in functions for SUBTOTAL in Excel order: (code, label_ja, label_en).
pub(crate) const SUBTOTAL_FUNCTIONS: &[(u32, &str, &str)] = &[
    (9, "_xlsx.sum", "Sum"),
    (2, "_xlsx.count", "Count"),
    (1, "_xlsx.average", "Average"),
    (4, "_xlsx.max-2", "Max"),
    (5, "_xlsx.min", "Min"),
    (6, "_xlsx.product", "Product"),
    (3, "_xlsx.count-numbers-2", "Count Numbers"),
    (7, "_xlsx.stddev", "StdDev"),
    (8, "_xlsx.stddevp-2", "StdDevp"),
    (10, "_xlsx.var-2", "Var"),
    (11, "_xlsx.varp-2", "Varp"),
];

pub(crate) struct SubtotalPanel {
    pub(crate) range: Entity<InputState>,
    pub(crate) fallback_range: CellRange,
    pub(crate) columns: Vec<(u32, String)>,
    pub(crate) group_by_column: u32,
    pub(crate) function_code: u32,
    pub(crate) subtotal_columns: Vec<u32>,
    pub(crate) replace_existing: bool,
    pub(crate) page_breaks: bool,
    pub(crate) summary_below: bool,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
    pub(crate) slide: SlideId,
    column_scroll: ScrollHandle,
}

impl SlideEditor {
    pub(crate) fn open_subtotal_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);

        let (r, def_col, _) = match self.sort_selection() {
            Some(s) => s,
            None => {
                let range = self.selected_range().unwrap_or(CellRange { first_row: 0, last_row: 1, first_column: 0, last_column: 0 });
                (range, range.first_column, true)
            }
        };

        let ((_, _), (_, sheet)) = match (self.current_cell(), self.current_sheet()) {
            (Some(c), Some(s)) => (c, s),
            _ => return,
        };

        // Extract column headers
        let mut cols = Vec::new();
        let mut default_subtotals = Vec::new();
        for col in r.first_column..=r.last_column {
            let label = if let Some(cell) = sheet.cell(r.first_row, col) {
                if !cell.text.is_empty() {
                    cell.text.clone()
                } else {
                    column_name(col)
                }
            } else {
                column_name(col)
            };
            cols.push((col, label));

            // Default: check columns that hold numeric data in data row
            if let Some(below) = sheet.cell(r.first_row + 1, col) {
                if matches!(below.value, ibis_doc_model::CellValue::Number(_)) {
                    default_subtotals.push(col);
                }
            }
        }
        if default_subtotals.is_empty() {
            default_subtotals.push(r.last_column);
        }

        let range_text = format!("{}:{}", ibis_doc_model::cell_name(r.first_column, r.first_row), ibis_doc_model::cell_name(r.last_column, r.last_row));
        let input = cx.new(|cx| {
            let mut s = InputState::new(window, cx).placeholder("A1:C10");
            s.set_value(range_text, window, cx);
            s
        });

        let group_col = if def_col >= r.first_column && def_col <= r.last_column {
            def_col
        } else {
            r.first_column
        };

        let panel = SubtotalPanel {
            range: input,
            fallback_range: r,
            columns: cols,
            group_by_column: group_col,
            function_code: 9, // SUM
            subtotal_columns: default_subtotals,
            replace_existing: true,
            page_breaks: false,
            summary_below: true,
            message: String::new(),
            ui: Default::default(),
            slide: self.current_slide,
            column_scroll:ScrollHandle::new(),
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
        state.panels.subtotal = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn resolve_subtotal_range(&self, raw: &str) -> Option<CellRange> {
        self.resolve_advanced_filter_range(raw).or_else(|| parse_range(raw))
    }

    pub(crate) fn apply_subtotal(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(p) = self.sheet_state().and_then(|s| s.panels.subtotal.as_ref()) else { return };
        let range_str = p.range.read(cx).value();
        let range = self.resolve_subtotal_range(&range_str).unwrap_or(p.fallback_range);

        if p.subtotal_columns.is_empty() {
            if let Some(p) = self.sheet_state_mut().and_then(|s| s.panels.subtotal.as_mut()) {
                p.message = text("_xlsx.select-at-least-one-column-to-subtotal").to_string();
            }
            cx.notify();
            return;
        }

        let group_by_column = if (range.first_column..=range.last_column).contains(&p.group_by_column) {
            p.group_by_column
        } else {
            range.first_column
        };
        let mut subtotal_columns: Vec<u32> = p
            .subtotal_columns
            .iter()
            .copied()
            .filter(|c| (range.first_column..=range.last_column).contains(c))
            .collect();
        if subtotal_columns.is_empty() {
            if let Some((_, sheet)) = self.current_sheet() {
                for col in range.first_column..=range.last_column {
                    if let Some(below) = sheet.cell(range.first_row + 1, col)
                        && matches!(below.value, ibis_doc_model::CellValue::Number(_))
                    {
                        subtotal_columns.push(col);
                    }
                }
            }
            if subtotal_columns.is_empty() {
                subtotal_columns.push(range.last_column);
            }
        }

        let params = SubtotalParams {
            range,
            group_by_column,
            function_code: p.function_code,
            subtotal_columns,
            replace_existing: p.replace_existing,
            summary_below: p.summary_below,
            page_breaks: p.page_breaks,
        };

        let slide = p.slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::ApplySubtotal { slide, params })], cx);

        if let Some(s) = self.sheet_state_mut() {
            s.panels.subtotal = None;
        }
        cx.notify();
    }

    pub(crate) fn remove_subtotal(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(p) = self.sheet_state().and_then(|s| s.panels.subtotal.as_ref()) else { return };
        let range_str = p.range.read(cx).value();
        let range = self.resolve_subtotal_range(&range_str).unwrap_or(p.fallback_range);
        let slide = p.slide;

        self.run_sheet(vec![Command::Sheet(SheetCommand::RemoveSubtotal { slide, range })], cx);

        if let Some(s) = self.sheet_state_mut() {
            s.panels.subtotal = None;
        }
        cx.notify();
    }

    pub(crate) fn select_all_subtotal_columns(&mut self, select_all: bool, cx: &mut Context<Self>) {
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.panels.subtotal.as_mut()) {
            if select_all {
                p.subtotal_columns = p.columns.iter().map(|&(col, _)| col).collect();
            } else {
                p.subtotal_columns.clear();
            }
            p.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn refresh_subtotal_range_from_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (r, def_col, _) = match self.sort_selection() {
            Some(s) => s,
            None => {
                let range = self.selected_range().unwrap_or(CellRange {
                    first_row: 0,
                    last_row: 1,
                    first_column: 0,
                    last_column: 0,
                });
                (range, range.first_column, true)
            }
        };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut cols = Vec::new();
        let mut default_subtotals = Vec::new();
        for col in r.first_column..=r.last_column {
            let label = if let Some(cell) = sheet.cell(r.first_row, col) {
                if !cell.text.is_empty() {
                    cell.text.clone()
                } else {
                    column_name(col)
                }
            } else {
                column_name(col)
            };
            cols.push((col, label));
            if let Some(below) = sheet.cell(r.first_row + 1, col) {
                if matches!(below.value, ibis_doc_model::CellValue::Number(_)) {
                    default_subtotals.push(col);
                }
            }
        }
        if default_subtotals.is_empty() {
            default_subtotals.push(r.last_column);
        }
        let range_text = format!(
            "{}:{}",
            ibis_doc_model::cell_name(r.first_column, r.first_row),
            ibis_doc_model::cell_name(r.last_column, r.last_row)
        );
        let group_col = if def_col >= r.first_column && def_col <= r.last_column {
            def_col
        } else {
            r.first_column
        };
        let Some(range_input) = self.sheet_state().and_then(|s| s.panels.subtotal.as_ref()).map(|p| p.range.clone()) else { return };
        range_input.update(cx, |i, cx| i.set_value(range_text, window, cx));
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.panels.subtotal.as_mut()) {
            p.fallback_range = r;
            p.columns = cols;
            p.group_by_column = group_col;
            p.subtotal_columns = default_subtotals;
            p.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn subtotal_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.subtotal.as_ref()?;
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

        let current_func_label = SUBTOTAL_FUNCTIONS
            .iter()
            .find(|(c, _, _)| *c == p.function_code)
            .map(|(_, ja, _)| *ja)
            .unwrap_or("_xlsx.sum");

        let current_group_label = p
            .columns
            .iter()
            .find(|(col, _)| *col == p.group_by_column)
            .map(|(_, l)| l.as_str())
            .unwrap_or("");

        let replace_existing = p.replace_existing;
        let page_breaks = p.page_breaks;
        let summary_below = p.summary_below;

        let mut body = v_flex().gap_3().p_3().p_0();

        // 0. 対象範囲
        body = body.child(
            v_flex()
                .gap_1()
                .child(
                    h_flex()
                        .items_center()
                        .justify_between()
                        .child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.range"))),
                )
                .child(mark("xlsx.subtotal.range", super::range_field::local_range_field(&p.range,"subtotal_dialog-p-range",false,cx).into_any_element())),
        );

        // 1. グループの基準 (Group by column)
        let entity = cx.entity().downgrade();
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.at-each-change-in")))
                .child(mark(
                    "xlsx.subtotal.group-by",
                    crate::shell::chrome::Dropdown::new("subtotal-group-by",text(current_group_label),
                        p.columns.iter().map(|(column,label)|{let column=*column;
                            crate::shell::chrome::DropdownItem::new(text(label)).checked(column==p.group_by_column)
                                .on_select(std::rc::Rc::new(move|e,_,cx|{if let Some(p)=e.wb_mut().panels.subtotal.as_mut(){p.group_by_column=column;}cx.notify();}))
                        }).collect(),&cx.weak_entity())
                        .into_any_element(),
                )),
        );

        // 2. 使用する関数 (Subtotal function)
        let entity_fn = cx.entity().downgrade();
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.use-function")))
                .child(mark(
                    "xlsx.subtotal.function",
                    crate::shell::chrome::Dropdown::new("subtotal-function",text(current_func_label),
                        SUBTOTAL_FUNCTIONS.iter().map(|&(code,label,_)|crate::shell::chrome::DropdownItem::new(text(label))
                            .checked(code==p.function_code).on_select(std::rc::Rc::new(move|e,_,cx|{if let Some(p)=e.wb_mut().panels.subtotal.as_mut(){p.function_code=code;}cx.notify();}))).collect(),&cx.weak_entity())
                        .into_any_element(),
                )),
        );

        // The header checkbox applies to all target columns; only data rows scroll.
        let all_checked=!p.columns.is_empty()&&p.columns.iter().all(|(col,_)|p.subtotal_columns.contains(col));
        let header=crate::shell::chrome::table_row_with_actions(vec![
            mark("xlsx.subtotal.select-all",crate::shell::chrome::checkbox_button("subtotal-select-all",text("_xlsx.add-subtotal-to"),all_checked,false,
                cx.listener(move|this,_,_,cx|this.select_all_subtotal_columns(!all_checked,cx))).into_any_element())
        ],div().into_any_element(),false,px(0.));
        let mut rows=v_flex();
        for &(col,ref label) in &p.columns {
            let checked=p.subtotal_columns.contains(&col);
            let check=crate::shell::chrome::checkbox_button(("subtotal-column",col as usize),text(label),checked,false,cx.listener(move|this,_,_,cx|{
                if let Some(p)=this.wb_mut().panels.subtotal.as_mut(){
                    if checked {p.subtotal_columns.retain(|&c|c!=col);}else{p.subtotal_columns.push(col);}
                }
                cx.notify();
            }));
            rows=rows.child(crate::shell::chrome::table_row_with_actions(vec![check.into_any_element()],div().into_any_element(),false,px(0.)));
        }
        body=body.child(mark("xlsx.subtotal.columns-box",crate::shell::chrome::table("subtotal-columns-table",header.into_any_element(),rows.into_any_element(),p.column_scroll.clone(),px(180.)).into_any_element()));

        // 4. オプション (Checkboxes)
        body = body.child(
            v_flex()
                .gap_1()
                .child(mark(
                    "xlsx.subtotal.replace",
                    Checkbox::new("subtotal-replace")
                        .label(text("_xlsx.replace-current-subtotals"))
                        .checked(replace_existing)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            if let Some(p) = this.wb_mut().panels.subtotal.as_mut() {
                                p.replace_existing = *on;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(mark(
                    "xlsx.subtotal.page-breaks",
                    Checkbox::new("subtotal-page-breaks")
                        .label(text("_xlsx.page-break-between-groups"))
                        .checked(page_breaks)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            if let Some(p) = this.wb_mut().panels.subtotal.as_mut() {
                                p.page_breaks = *on;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ))
                .child(mark(
                    "xlsx.subtotal.summary-below",
                    Checkbox::new("subtotal-summary-below")
                        .label(text("_xlsx.summary-below-data"))
                        .checked(summary_below)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            if let Some(p) = this.wb_mut().panels.subtotal.as_mut() {
                                p.summary_below = *on;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                )),
        );

        // Message if any
        if !p.message.is_empty() {
            body = body.child(div().text_sm().text_color(cx.theme().danger).child(p.message.clone()));
        }

        // 5. Actions (OK, すべて削除, キャンセル)
        let mut actions = crate::shell::chrome::action_bar();
        actions = actions.action(mark_action("xlsx.subtotal.ok", crate::shell::chrome::ActionDefinition::new("subtotal-ok", "", crate::shell::chrome::ActionButtonKind::Primary)
                .label(text("OK"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.apply_subtotal(cx);
                }))));
        actions = actions.action(mark_action("xlsx.subtotal.cancel", crate::shell::chrome::ActionDefinition::new("subtotal-cancel", "", crate::shell::chrome::ActionButtonKind::Normal)
                .label(text("_shell.cancel"))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(s) = this.sheet_state_mut() {
                        s.panels.subtotal = None;
                    }
                    cx.notify();
                }))));
        actions = actions.action(mark_action("xlsx.subtotal.remove-all", crate::shell::chrome::ActionDefinition::new("subtotal-remove-all", "", crate::shell::chrome::ActionButtonKind::Destructive)
                .label(text("_xlsx.delete-all"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.remove_subtotal(cx);
                }))));



        Some(SidebarPanel::with_footer(text("_xlsx.subtotal"), body.into_any_element(), actions))
    }
}
