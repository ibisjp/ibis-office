// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > ピボットテーブル > 集計アイテム… (ピボットテーブルの計算アイテムの追加・変更・削除)
//! Panel to configure calculated items (計算アイテム) on a pivot table.

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{CellValue, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct PivotCalcItemPanel {
    pub(crate) target: (SlideId, usize),
    pub(crate) selected_field: usize,
    pub(crate) fields: Vec<(usize, String)>,
    pub(crate) name_input: Entity<InputState>,
    pub(crate) formula_input: Entity<InputState>,
    pub(crate) selected_item: usize,
    pub(crate) items: Vec<String>,
    pub(crate) calc_items: Vec<(String, String)>,
    pub(crate) message: String,
}

impl SlideEditor {
    /// データ > 集計アイテム…: open the calculated item panel.
    pub(crate) fn open_pivot_calc_item(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let target = (self.current_slide, index);
        let Some(pivot) = self.pivot_def(target) else { return };

        let fields: Vec<(usize, String)> = pivot
            .fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.formula.is_none())
            .map(|(i, f)| (i, f.name.clone()))
            .collect();

        if fields.is_empty() {
            self.status = Some(text("_xlsx.no-fields-available").into());
            return cx.notify();
        }

        let first_field_idx = fields[0].0;
        let mut items: Vec<String> = if let Some(f) = pivot.fields.get(first_field_idx) {
            f.shared
                .iter()
                .filter_map(|v| match v {
                    CellValue::Text(s) if !s.is_empty() => Some(s.clone()),
                    CellValue::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .collect()
        } else {
            Vec::new()
        };
        let calc_items: Vec<(String, String)> = pivot
            .calculated_items_for_field(first_field_idx)
            .into_iter()
            .map(|(n, f)| (n.to_string(), f.to_string()))
            .collect();

        for (ci_name, _) in &calc_items {
            if !items.contains(ci_name) {
                items.push(ci_name.clone());
            }
        }

        let initial_name = crate::shell::i18n::format("_xlsx.formula-n", &[("n", &(calc_items.len() + 1))]);
        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };

        let name_input = input(initial_name, window, cx);
        let formula_input = input("=0".into(), window, cx);

        let panel = PivotCalcItemPanel {
            target,
            selected_field: 0,
            fields,
            name_input,
            formula_input,
            selected_item: 0,
            items,
            calc_items,
            message: String::new(),
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
        s.panels.pivot_calc_item = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// Select a field from the fields list in calculated item panel.
    pub(crate) fn select_calc_item_field(&mut self, field_list_idx: usize, cx: &mut Context<Self>) {
        let Some(p) = self.wb_mut().panels.pivot_calc_item.as_mut() else { return };
        if field_list_idx >= p.fields.len() { return; }
        p.selected_field = field_list_idx;
        let target = p.target;
        let field_idx = p.fields[field_list_idx].0;

        let Some(pivot) = self.pivot_def(target) else { return };
        let mut items: Vec<String> = if let Some(f) = pivot.fields.get(field_idx) {
            f.shared
                .iter()
                .filter_map(|v| match v {
                    CellValue::Text(s) if !s.is_empty() => Some(s.clone()),
                    CellValue::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .collect()
        } else {
            Vec::new()
        };
        let calc_items: Vec<(String, String)> = pivot
            .calculated_items_for_field(field_idx)
            .into_iter()
            .map(|(n, f)| (n.to_string(), f.to_string()))
            .collect();

        for (ci_name, _) in &calc_items {
            if !items.contains(ci_name) {
                items.push(ci_name.clone());
            }
        }

        if let Some(p) = self.wb_mut().panels.pivot_calc_item.as_mut() {
            p.items = items;
            p.calc_items = calc_items;
            p.selected_item = 0;
        }
        cx.notify();
    }

    /// Select an item from the items list.
    pub(crate) fn select_calc_item_entry(&mut self, item_idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.wb_mut().panels.pivot_calc_item.as_mut() else { return };
        if item_idx >= p.items.len() { return; }
        p.selected_item = item_idx;
        let item_name = p.items[item_idx].clone();

        if let Some((_, fml)) = p.calc_items.iter().find(|(n, _)| n == &item_name).cloned() {
            p.name_input.update(cx, |input, cx| input.set_value(item_name, window, cx));
            p.formula_input.update(cx, |input, cx| input.set_value(fml, window, cx));
        }
        cx.notify();
    }

    /// Insert the currently selected item name into the formula input.
    pub(crate) fn insert_item_to_calc_formula(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.wb_mut().panels.pivot_calc_item.as_mut() else { return };
        let Some(item_name) = p.items.get(p.selected_item).cloned() else { return };
        let formula_input = p.formula_input.clone();
        let cur = formula_input.read(cx).value().to_string();
        let formatted = if item_name.contains(' ') || item_name.contains('-') || item_name.contains('+') || item_name.contains('*') || item_name.contains('/') {
            format!("'{item_name}'")
        } else {
            item_name
        };
        let new_text = if cur.ends_with('=') || cur.ends_with('+') || cur.ends_with('-') || cur.ends_with('*') || cur.ends_with('/') || cur.ends_with('(') || cur.ends_with(',') || cur.ends_with(' ') {
            format!("{cur}{formatted}")
        } else if cur == "=0" {
            format!("={formatted}")
        } else {
            format!("{cur} + {formatted}")
        };
        formula_input.update(cx, |input, cx| {
            input.set_value(new_text, window, cx);
        });
        cx.notify();
    }

    /// Add or modify the calculated item.
    pub(crate) fn add_or_modify_calc_item(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.wb_mut().panels.pivot_calc_item.as_mut() else { return };
        let name = p.name_input.read(cx).value().to_string().trim().to_string();
        let formula = p.formula_input.read(cx).value().to_string().trim().to_string();
        let target = p.target;
        let selected_field_list_idx = p.selected_field;
        let field_idx = p.fields.get(selected_field_list_idx).map(|(i, _)| *i).unwrap_or(0);

        if name.is_empty() {
            if let Some(p) = self.wb_mut().panels.pivot_calc_item.as_mut() {
                p.message = text("_xlsx.please-enter-an-item-name").to_string();
            }
            return cx.notify();
        }
        if formula.is_empty() || formula == "=" {
            if let Some(p) = self.wb_mut().panels.pivot_calc_item.as_mut() {
                p.message = text("_xlsx.please-enter-a-formula-2").to_string();
            }
            return cx.notify();
        }

        let mut error = None;
        self.change_pivot(target, |pivot| {
            if pivot.is_calculated_item(field_idx, &name) {
                if let Err(e) = pivot.modify_calculated_item(field_idx, &name, &formula) {
                    error = Some(e);
                }
            } else if let Err(e) = pivot.add_calculated_item(field_idx, &name, &formula) {
                error = Some(e);
            }
        }, cx);

        if let Some(err) = error {
            if let Some(p) = self.wb_mut().panels.pivot_calc_item.as_mut() {
                p.message = err;
            }
        } else {
            self.select_calc_item_field(selected_field_list_idx, cx);
            if let Some(p) = self.wb_mut().panels.pivot_calc_item.as_mut() {
                p.message = text("_xlsx.calculated-item-added").to_string();
            }
        }
        cx.notify();
    }

    /// Delete the calculated item with the current name.
    pub(crate) fn delete_calc_item(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.wb_mut().panels.pivot_calc_item.as_mut() else { return };
        let name = p.name_input.read(cx).value().to_string().trim().to_string();
        let target = p.target;
        let selected_field_list_idx = p.selected_field;
        let field_idx = p.fields.get(selected_field_list_idx).map(|(i, _)| *i).unwrap_or(0);

        if name.is_empty() {
            return;
        }

        let mut error = None;
        self.change_pivot(target, |pivot| {
            if let Err(e) = pivot.delete_calculated_item(field_idx, &name) {
                error = Some(e);
            }
        }, cx);

        if let Some(err) = error {
            if let Some(p) = self.wb_mut().panels.pivot_calc_item.as_mut() {
                p.message = err;
            }
        } else {
            self.select_calc_item_field(selected_field_list_idx, cx);
            if let Some(p) = self.wb_mut().panels.pivot_calc_item.as_mut() {
                p.message = text("_xlsx.calculated-item-deleted").to_string();
            }
        }
        cx.notify();
    }

    /// Render the calculated item panel in the sidebar.
    pub(crate) fn pivot_calc_item_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.wb().panels.pivot_calc_item.as_ref()?;

        let mut body = v_flex().gap_3().p_3().p_0();

        body = body
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.name")))
                    .child(crate::shell::chrome::text_field(&p.name_input).full_width())
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.formula")))
                    .child(crate::shell::chrome::text_field(&p.formula_input).full_width())
            );

        // Fields list (left) and Items list (right)
        let mut field_list = div().id("calc-item-fields-list").h(px(160.)).overflow_y_scroll();
        for (idx, (_, f_name)) in p.fields.iter().enumerate() {
            let is_sel = idx == p.selected_field;
            field_list = field_list.child(
                div()
                    .id(ElementId::Name(format!("field-item-{idx}").into()))
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .text_sm()
                    .when(is_sel, |d| d.bg(crate::shell::palette::rgb(crate::shell::chrome::SELECTED_ROW_BG)))
                    .when(!is_sel, |d| d.hover(|h| h.bg(crate::shell::palette::rgb(0xE5E7EB))))
                    .child(f_name.clone())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_calc_item_field(idx, cx);
                    }))
            );
        }

        let mut item_list = div().id("calc-items-list").h(px(160.)).overflow_y_scroll();
        for (idx, it_name) in p.items.iter().enumerate() {
            let is_sel = idx == p.selected_item;
            item_list = item_list.child(
                div()
                    .id(ElementId::Name(format!("item-entry-{idx}").into()))
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .text_sm()
                    .when(is_sel, |d| d.bg(crate::shell::palette::rgb(crate::shell::chrome::SELECTED_ROW_BG)))
                    .when(!is_sel, |d| d.hover(|h| h.bg(crate::shell::palette::rgb(0xE5E7EB))))
                    .child(it_name.clone())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_calc_item_entry(idx, window, cx);
                    }))
            );
        }

        body = body.child(
            v_flex()
                .gap_1()
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            v_flex()
                                .flex_1()
                                .gap_1()
                                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.fields")))
                                .child(
                                    div()
                                        .p_1()
                                        .border_1()
                                        .border_color(crate::shell::palette::rgb(0xD1D5DB))
                                        .rounded_md()
                                        .child(field_list)
                                )
                        )
                        .child(
                            v_flex()
                                .flex_1()
                                .gap_1()
                                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("_xlsx.items-2")))
                                .child(
                                    div()
                                        .p_1()
                                        .border_1()
                                        .border_color(crate::shell::palette::rgb(0xD1D5DB))
                                        .rounded_md()
                                        .child(item_list)
                                )
                        )
                )
                .child(
                    h_flex()
                        .justify_end()
                        .child(
                            crate::shell::chrome::primary_button("insert-item-btn")
                                .small()
                                .label(text("_xlsx.insert-item"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.insert_item_to_calc_formula(window, cx);
                                }))
                        )
                )
        );

        // Message
        if !p.message.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(crate::shell::palette::rgb(0x2563EB))
                    .child(p.message.clone())
            );
        }

        // Action buttons
        let panel_actions = crate::shell::chrome::action_bar()
                .action(
                    crate::shell::chrome::ActionDefinition::new("calc-item-add", "", crate::shell::chrome::ActionButtonKind::Primary)
                        .label(text("fix.common.add"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.add_or_modify_calc_item(cx);
                        }))
                )

            .action(crate::shell::chrome::dialog_cancel_definition(cx))
                .action(
                    crate::shell::chrome::ActionDefinition::new("calc-item-delete", "", crate::shell::chrome::ActionButtonKind::Destructive)
                        .label(text("_shell.delete"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.delete_calc_item(cx);
                        }))
                );
        Some(SidebarPanel::with_footer(text("_xlsx.insert-calculated-item").to_string(), body.into_any_element(), panel_actions))
    }
}
