// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > グループ化… (ピボットテーブルの日付・数値フィールドのグループ化)
//! Panel to configure date and numeric grouping on a pivot table field.

use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{DateGroupBy, PivotFieldGrouping, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct PivotGroupPanel {
    pub(crate) target: (SlideId, usize),
    pub(crate) field: usize,
    pub(crate) field_name: String,
    /// true = 日付グループ化, false = 数値グループ化
    pub(crate) is_date: bool,
    /// Number fields
    pub(crate) num_start: Entity<InputState>,
    pub(crate) num_end: Entity<InputState>,
    pub(crate) num_interval: Entity<InputState>,
    pub(crate) num_auto_start: bool,
    pub(crate) num_auto_end: bool,
    /// Date fields
    pub(crate) date_by: DateGroupBy,
    pub(crate) date_start: Entity<InputState>,
    pub(crate) date_end: Entity<InputState>,
    pub(crate) date_auto_start: bool,
    pub(crate) date_auto_end: bool,
    #[allow(dead_code)]
    pub(crate) message: String,
}

fn serial_to_date_string(serial: f64) -> String {
    let (y, m, d) = ibis_calc::eval::convert::date_from_serial(serial.floor() as i64, false);
    format!("{y:04}-{m:02}-{d:02}")
}

impl SlideEditor {
    /// データ > グループ化…: open the pivot field grouping panel.
    pub(crate) fn open_pivot_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let target = (self.current_slide, index);
        let Some(pivot) = self.pivot_def(target) else { return };

        // Identify the field to group: check if the selected cell is in row fields, else default to first row field.
        let field = if let Some((col, row)) = self.current_cell() {
            let loc = pivot.location;
            if (loc.first_column..loc.first_column + pivot.first_data_col).contains(&col) {
                let depth = (col - loc.first_column) as usize;
                pivot.row_fields.get(depth).copied().filter(|&f| f >= 0).map(|f| f as usize)
                    .or_else(|| {
                        let r_idx = row.saturating_sub(loc.first_row + pivot.first_data_row) as usize;
                        pivot.row_items.get(r_idx).and_then(|item| {
                            let d = item.repeat.min(pivot.row_fields.len().saturating_sub(1) as u32) as usize;
                            pivot.row_fields.get(d).copied().filter(|&f| f >= 0).map(|f| f as usize)
                        })
                    })
                    .unwrap_or(0)
            } else {
                pivot.row_fields.first().copied().filter(|&f| f >= 0).map(|f| f as usize).unwrap_or(0)
            }
        } else {
            pivot.row_fields.first().copied().filter(|&f| f >= 0).map(|f| f as usize).unwrap_or(0)
        };

        let field_name = pivot.fields.get(field).map(|f| f.name.clone()).unwrap_or_default();
        let pf = pivot.fields.get(field);

        // Check if multiple cells or text cells are selected in the pivot table for manual grouping
        let ranges = self.selected_ranges();
        let mut selected_text_items: Vec<String> = Vec::new();
        if let Some((_, sheet)) = self.current_sheet() {
            for range in &ranges {
                for r in range.first_row..=range.last_row {
                    for c in range.first_column..=range.last_column {
                        if let Some(cell) = sheet.cell(r, c) {
                            if let ibis_doc_model::CellValue::Text(t) = &cell.value {
                                if !t.is_empty() && !selected_text_items.contains(t) {
                                    selected_text_items.push(t.clone());
                                }
                            }
                        }
                    }
                }
            }
        }

        let numbers: Vec<f64> = pf.map_or(Vec::new(), |f| {
            f.shared.iter().filter_map(|v| if let ibis_doc_model::CellValue::Number(n) = v { Some(*n) } else { None }).collect()
        });
        let is_number_or_date_field = !numbers.is_empty() && pf.is_some_and(|f| !matches!(f.grouping, Some(PivotFieldGrouping::Manual { .. })));

        if (!is_number_or_date_field && !selected_text_items.is_empty()) || selected_text_items.len() >= 2 {
            let mut created_group = None;
            self.change_pivot(target, |p| {
                created_group = p.group_manual(field, &selected_text_items, None);
            }, cx);
            if let Some(g) = created_group {
                self.status = Some(crate::shell::i18n::format("_xlsx.created-group", &[("group", &g)]).into());
            }
            return cx.notify();
        }

        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };

        let panel = match pf.and_then(|f| f.grouping.clone()) {
            Some(PivotFieldGrouping::Number { start, end, interval, auto_start, auto_end }) => {
                PivotGroupPanel {
                    target,
                    field,
                    field_name,
                    is_date: false,
                    num_start: input(start.to_string(), window, cx),
                    num_end: input(end.to_string(), window, cx),
                    num_interval: input(interval.to_string(), window, cx),
                    num_auto_start: auto_start,
                    num_auto_end: auto_end,
                    date_by: DateGroupBy::Months,
                    date_start: input("2026-01-01".into(), window, cx),
                    date_end: input("2026-12-31".into(), window, cx),
                    date_auto_start: true,
                    date_auto_end: true,
                    message: String::new(),
                }
            }
            Some(PivotFieldGrouping::Date { by, start, end, auto_start, auto_end }) => {
                let s_str = start.map(serial_to_date_string).unwrap_or_else(|| "2026-01-01".into());
                let e_str = end.map(serial_to_date_string).unwrap_or_else(|| "2026-12-31".into());
                PivotGroupPanel {
                    target,
                    field,
                    field_name,
                    is_date: true,
                    num_start: input("0".into(), window, cx),
                    num_end: input("100".into(), window, cx),
                    num_interval: input("10".into(), window, cx),
                    num_auto_start: true,
                    num_auto_end: true,
                    date_by: by,
                    date_start: input(s_str, window, cx),
                    date_end: input(e_str, window, cx),
                    date_auto_start: auto_start,
                    date_auto_end: auto_end,
                    message: String::new(),
                }
            }
            _ => {
                let min_val = numbers.iter().copied().fold(f64::INFINITY, f64::min);
                let max_val = numbers.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                let is_date = min_val >= 20000.0 && max_val <= 80000.0 && !numbers.is_empty();

                let (s_num, e_num) = if min_val.is_finite() && max_val.is_finite() {
                    (min_val.to_string(), max_val.to_string())
                } else {
                    ("0".into(), "100".into())
                };

                let (s_date, e_date) = if min_val.is_finite() && max_val.is_finite() && is_date {
                    (serial_to_date_string(min_val), serial_to_date_string(max_val))
                } else {
                    ("2026-01-01".into(), "2026-12-31".into())
                };

                PivotGroupPanel {
                    target,
                    field,
                    field_name,
                    is_date,
                    num_start: input(s_num, window, cx),
                    num_end: input(e_num, window, cx),
                    num_interval: input("10".into(), window, cx),
                    num_auto_start: true,
                    num_auto_end: true,
                    date_by: DateGroupBy::Months,
                    date_start: input(s_date, window, cx),
                    date_end: input(e_date, window, cx),
                    date_auto_start: true,
                    date_auto_end: true,
                    message: String::new(),
                }
            }
        };

        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.close_settings();
        s.panels.pivot_group = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// データ > グループ解除: remove grouping from the current field.
    pub(crate) fn pivot_ungroup(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let target = (self.current_slide, index);
        let Some(pivot) = self.pivot_def(target) else { return };

        // Find which field to ungroup: active row field, or first grouped field.
        let field = if let Some((col, row)) = self.current_cell() {
            let loc = pivot.location;
            if (loc.first_column..loc.first_column + pivot.first_data_col).contains(&col) {
                let depth = (col - loc.first_column) as usize;
                pivot.row_fields.get(depth).copied().filter(|&f| f >= 0).map(|f| f as usize)
                    .or_else(|| {
                        let r_idx = row.saturating_sub(loc.first_row + pivot.first_data_row) as usize;
                        pivot.row_items.get(r_idx).and_then(|item| {
                            let d = item.repeat.min(pivot.row_fields.len().saturating_sub(1) as u32) as usize;
                            pivot.row_fields.get(d).copied().filter(|&f| f >= 0).map(|f| f as usize)
                        })
                    })
                    .unwrap_or(0)
            } else {
                pivot.fields.iter().position(|f| f.grouping.is_some()).unwrap_or(0)
            }
        } else {
            pivot.fields.iter().position(|f| f.grouping.is_some()).unwrap_or(0)
        };

        // Check if a specific manual group or item is selected
        let selected_item = self.current_cell().and_then(|(c, r)| {
            self.current_sheet().and_then(|(_, sh)| sh.cell(r, c)).and_then(|cell| {
                if let ibis_doc_model::CellValue::Text(t) = &cell.value {
                    Some(t.clone())
                } else {
                    None
                }
            })
        });

        self.change_pivot(target, move |p| {
            if let Some(item) = selected_item {
                if p.manual_groups_for_field(field).is_some() {
                    p.ungroup_manual_items(field, &[item]);
                    return;
                }
            }
            p.ungroup_field(field);
        }, cx);
        self.wb_mut().panels.pivot_group = None;
        cx.notify();
    }

    /// Apply grouping configured in PivotGroupPanel.
    pub(crate) fn apply_pivot_group(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.pivot_group.as_ref()) else { return };
        let target = p.target;
        let field = p.field;

        let grouping = if p.is_date {
            let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
            let s_val = p.date_start.read(cx).value().to_string();
            let e_val = p.date_end.read(cx).value().to_string();
            let s_text = s_val.trim();
            let e_text = e_val.trim();
            let start = ibis_calc::eval::convert::parse_date_time(s_text, date1904).or_else(|| s_text.parse::<f64>().ok());
            let end = ibis_calc::eval::convert::parse_date_time(e_text, date1904).or_else(|| e_text.parse::<f64>().ok());
            PivotFieldGrouping::Date {
                by: p.date_by,
                start,
                end,
                auto_start: p.date_auto_start,
                auto_end: p.date_auto_end,
            }
        } else {
            let s_val = p.num_start.read(cx).value().to_string();
            let e_val = p.num_end.read(cx).value().to_string();
            let i_val = p.num_interval.read(cx).value().to_string();
            let s_text = s_val.trim();
            let e_text = e_val.trim();
            let i_text = i_val.trim();
            let start = s_text.parse::<f64>().unwrap_or(0.0);
            let end = e_text.parse::<f64>().unwrap_or(100.0);
            let mut interval = i_text.parse::<f64>().unwrap_or(10.0);
            if interval <= 0.0 {
                interval = 1.0;
            }
            PivotFieldGrouping::Number {
                start,
                end,
                interval,
                auto_start: p.num_auto_start,
                auto_end: p.num_auto_end,
            }
        };

        self.change_pivot(target, move |t| t.group_field(field, grouping), cx);
        self.wb_mut().panels.pivot_group = None;
        cx.notify();
    }

    pub(crate) fn pivot_group_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.pivot_group.as_ref()?;
        let is_date = p.is_date;

        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(
                h_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::BOLD).child(format!("{}: {}", text("_xlsx.field"), p.field_name)))
            )
            .child(
                crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                    .child(
                        crate::shell::chrome::radio_button("pivot-group-type-number", !is_date, false, text("_xlsx.numbers"), cx.listener(|this, _, _, cx| {
                                if let Some(p) = this.wb_mut().panels.pivot_group.as_mut() {
                                    p.is_date = false;
                                }
                                cx.notify();
                            }))
                    )
                    .child(
                        crate::shell::chrome::radio_button("pivot-group-type-date", is_date, false, text("_xlsx.short-date"), cx.listener(|this, _, _, cx| {
                                if let Some(p) = this.wb_mut().panels.pivot_group.as_mut() {
                                    p.is_date = true;
                                }
                                cx.notify();
                            }))
                    )
            );

        if is_date {
            let by = p.date_by;
            let date_by_buttons = crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                .child(
                    crate::shell::chrome::radio_button("pivot-group-by-years", by == DateGroupBy::Years, false, text("fix.series.year"), cx.listener(|this, _, _, cx| {
                            if let Some(p) = this.wb_mut().panels.pivot_group.as_mut() {
                                p.date_by = DateGroupBy::Years;
                            }
                            cx.notify();
                        }))
                )
                .child(
                    crate::shell::chrome::radio_button("pivot-group-by-quarters", by == DateGroupBy::Quarters, false, text("_xlsx.quarters"), cx.listener(|this, _, _, cx| {
                            if let Some(p) = this.wb_mut().panels.pivot_group.as_mut() {
                                p.date_by = DateGroupBy::Quarters;
                            }
                            cx.notify();
                        }))
                )
                .child(
                    crate::shell::chrome::radio_button("pivot-group-by-months", by == DateGroupBy::Months, false, text("fix.series.month"), cx.listener(|this, _, _, cx| {
                            if let Some(p) = this.wb_mut().panels.pivot_group.as_mut() {
                                p.date_by = DateGroupBy::Months;
                            }
                            cx.notify();
                        }))
                )
                .child(
                    crate::shell::chrome::radio_button("pivot-group-by-days", by == DateGroupBy::Days, false, text("fix.series.day"), cx.listener(|this, _, _, cx| {
                            if let Some(p) = this.wb_mut().panels.pivot_group.as_mut() {
                                p.date_by = DateGroupBy::Days;
                            }
                            cx.notify();
                        }))
                );

            body = body
                .child(div().text_sm().child(text("_xlsx.units")))
                .child(date_by_buttons)
                .child(div().text_sm().child(text("_xlsx.starting-at")))
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(crate::shell::chrome::text_field(&p.date_start).full_width())
                        .child(
                            Checkbox::new("pivot-group-date-auto-start")
                                .label(text("_shell.auto"))
                                .checked(p.date_auto_start)
                                .on_click(cx.listener(|this, on: &bool, _, cx| {
                                    if let Some(p) = this.wb_mut().panels.pivot_group.as_mut() {
                                        p.date_auto_start = *on;
                                    }
                                    cx.notify();
                                }))
                        )
                )
                .child(div().text_sm().child(text("_xlsx.ending-at")))
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(crate::shell::chrome::text_field(&p.date_end).full_width())
                        .child(
                            Checkbox::new("pivot-group-date-auto-end")
                                .label(text("_shell.auto"))
                                .checked(p.date_auto_end)
                                .on_click(cx.listener(|this, on: &bool, _, cx| {
                                    if let Some(p) = this.wb_mut().panels.pivot_group.as_mut() {
                                        p.date_auto_end = *on;
                                    }
                                    cx.notify();
                                }))
                        )
                );
        } else {
            body = body
                .child(div().text_sm().child(text("_xlsx.starting-at")))
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(crate::shell::chrome::text_field(&p.num_start).full_width())
                        .child(
                            Checkbox::new("pivot-group-num-auto-start")
                                .label(text("_shell.auto"))
                                .checked(p.num_auto_start)
                                .on_click(cx.listener(|this, on: &bool, _, cx| {
                                    if let Some(p) = this.wb_mut().panels.pivot_group.as_mut() {
                                        p.num_auto_start = *on;
                                    }
                                    cx.notify();
                                }))
                        )
                )
                .child(div().text_sm().child(text("_xlsx.ending-at")))
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(crate::shell::chrome::text_field(&p.num_end).full_width())
                        .child(
                            Checkbox::new("pivot-group-num-auto-end")
                                .label(text("_shell.auto"))
                                .checked(p.num_auto_end)
                                .on_click(cx.listener(|this, on: &bool, _, cx| {
                                    if let Some(p) = this.wb_mut().panels.pivot_group.as_mut() {
                                        p.num_auto_end = *on;
                                    }
                                    cx.notify();
                                }))
                        )
                )
                .child(div().text_sm().child(text("_xlsx.units")))
                .child(crate::shell::chrome::text_field(&p.num_interval).full_width().spinner("pivot-group-num-interval", crate::shell::chrome::SpinnerSpec::new(1.0).min(1.0), cx, |_, _, _, _| {}));
        }

        body = body
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(
                        crate::shell::chrome::ActionDefinition::new("pivot-group-ok", "", crate::shell::chrome::ActionButtonKind::Primary)
                            .label("OK")
                            .on_click(cx.listener(|this, _, _, cx| this.apply_pivot_group(cx)))
                    )
                    .action(crate::shell::chrome::dialog_cancel_definition(cx))
                    .action(
                        crate::shell::chrome::ActionDefinition::new("pivot-group-ungroup", "", crate::shell::chrome::ActionButtonKind::Destructive)
                            .label(text("_shell.ungroup"))
                            .on_click(cx.listener(|this, _, _, cx| this.pivot_ungroup(cx)))
                    )
                    ;
        Some(SidebarPanel::with_footer(text("_shell.group").to_string(), body.into_any_element(), panel_actions))
    }
}
