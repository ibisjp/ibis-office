// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > ピボットテーブル > 計算フィールド… (ピボットテーブルの計算フィールドの追加・変更・削除)
//! Panel to configure calculated fields (計算フィールド) on a pivot table.

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::SlideId;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct PivotCalcFieldPanel {
    pub(crate) target: (SlideId, usize),
    pub(crate) name_input: Entity<InputState>,
    pub(crate) formula_input: Entity<InputState>,
    pub(crate) selected_field: usize,
    pub(crate) fields: Vec<String>,
    pub(crate) calc_fields: Vec<(String, String)>,
    pub(crate) message: String,
}

impl SlideEditor {
    /// データ > 計算フィールド…: open the calculated field panel.
    pub(crate) fn open_pivot_calc_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.pivot_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-pivottable").into());
            return cx.notify();
        };
        let target = (self.current_slide, index);
        let Some(pivot) = self.pivot_def(target) else { return };

        let fields: Vec<String> = pivot.fields.iter().filter(|f| f.formula.is_none()).map(|f| f.name.clone()).collect();
        let calc_fields: Vec<(String, String)> = pivot.calculated_fields().into_iter().map(|(_, name, fml)| (name.to_string(), fml.to_string())).collect();

        let initial_name = crate::shell::i18n::format("_xlsx.field-number", &[("number", &(pivot.fields.len() + 1))]).to_string();
        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };

        let name_input = input(initial_name, window, cx);
        let formula_input = input("=0".into(), window, cx);

        let panel = PivotCalcFieldPanel {
            target,
            name_input,
            formula_input,
            selected_field: 0,
            fields,
            calc_fields,
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
        s.panels.pivot_calc_field = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// Insert the currently selected field name into the formula input.
    pub(crate) fn insert_field_to_calc_formula(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.wb_mut().panels.pivot_calc_field.as_mut() else { return };
        let Some(field_name) = p.fields.get(p.selected_field).cloned() else { return };
        let formula_input = p.formula_input.clone();
        let cur = formula_input.read(cx).value().to_string();
        let new_text = if cur.ends_with('=') || cur.ends_with('+') || cur.ends_with('-') || cur.ends_with('*') || cur.ends_with('/') || cur.ends_with('(') || cur.ends_with(',') || cur.ends_with(' ') {
            format!("{cur}{field_name}")
        } else if cur == "=0" {
            format!("={field_name}")
        } else {
            format!("{cur} + {field_name}")
        };
        formula_input.update(cx, |input, cx| {
            input.set_value(new_text, window, cx);
        });
        cx.notify();
    }

    /// Select an existing calculated field or standard field from list.
    pub(crate) fn select_calc_field_item(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.wb_mut().panels.pivot_calc_field.as_mut() else { return };
        if let Some((_, fml)) = p.calc_fields.iter().find(|(n, _)| n == name).cloned() {
            let name_clone = name.to_string();
            p.name_input.update(cx, |input, cx| input.set_value(name_clone, window, cx));
            p.formula_input.update(cx, |input, cx| input.set_value(fml, window, cx));
        } else if let Some(idx) = p.fields.iter().position(|f| f == name) {
            p.selected_field = idx;
        }
        cx.notify();
    }

    /// Add or modify the calculated field.
    pub(crate) fn add_or_modify_calc_field(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.wb_mut().panels.pivot_calc_field.as_mut() else { return };
        let name = p.name_input.read(cx).value().to_string().trim().to_string();
        let formula = p.formula_input.read(cx).value().to_string().trim().to_string();
        let target = p.target;

        if name.is_empty() {
            if let Some(p) = self.wb_mut().panels.pivot_calc_field.as_mut() {
                p.message = text("_xlsx.please-enter-a-field-name").to_string();
            }
            return cx.notify();
        }
        if formula.is_empty() || formula == "=" {
            if let Some(p) = self.wb_mut().panels.pivot_calc_field.as_mut() {
                p.message = text("_xlsx.please-enter-a-formula").to_string();
            }
            return cx.notify();
        }

        let mut error = None;
        self.change_pivot(target, |pivot| {
            if pivot.fields.iter().any(|f| f.name.eq_ignore_ascii_case(&name) && f.formula.is_some()) {
                if let Err(e) = pivot.modify_calculated_field(&name, &formula) {
                    error = Some(e);
                }
            } else if let Err(e) = pivot.add_calculated_field(&name, &formula) {
                error = Some(e);
            }
        }, cx);

        if let Some(err) = error {
            if let Some(p) = self.wb_mut().panels.pivot_calc_field.as_mut() {
                p.message = err;
            }
        } else {
            let new_calcs = self.pivot_def(target).map(|pivot| {
                pivot.calculated_fields().into_iter().map(|(_, n, f)| (n.to_string(), f.to_string())).collect()
            });
            if let Some(p) = self.wb_mut().panels.pivot_calc_field.as_mut() {
                p.message = text("_xlsx.calculated-field-added").to_string();
                if let Some(calcs) = new_calcs {
                    p.calc_fields = calcs;
                }
            }
        }
        cx.notify();
    }

    /// Delete the calculated field with the current name.
    pub(crate) fn delete_calc_field(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.wb_mut().panels.pivot_calc_field.as_mut() else { return };
        let name = p.name_input.read(cx).value().to_string().trim().to_string();
        let target = p.target;

        if name.is_empty() {
            return;
        }

        let mut error = None;
        self.change_pivot(target, |pivot| {
            if let Err(e) = pivot.delete_calculated_field(&name) {
                error = Some(e);
            }
        }, cx);

        if let Some(err) = error {
            if let Some(p) = self.wb_mut().panels.pivot_calc_field.as_mut() {
                p.message = err;
            }
        } else {
            let new_calcs = self.pivot_def(target).map(|pivot| {
                pivot.calculated_fields().into_iter().map(|(_, n, f)| (n.to_string(), f.to_string())).collect()
            });
            if let Some(p) = self.wb_mut().panels.pivot_calc_field.as_mut() {
                p.message = text("_xlsx.calculated-field-deleted").to_string();
                if let Some(calcs) = new_calcs {
                    p.calc_fields = calcs;
                }
            }
        }
        cx.notify();
    }

    /// Render the calculated field panel in the sidebar.
    pub(crate) fn pivot_calc_field_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.wb().panels.pivot_calc_field.as_ref()?;

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

        // Fields list
        let mut field_list = div().id("calc-fields-list").h(px(160.)).overflow_y_scroll();
        for (idx, f_name) in p.fields.iter().enumerate() {
            let is_sel = idx == p.selected_field;
            let name_clone = f_name.clone();
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
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.select_calc_field_item(&name_clone, window, cx);
                    }))
            );
        }

        body = body.child(
            v_flex()
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
                .child(
                    h_flex()
                        .justify_end()
                        .child(
                            crate::shell::chrome::primary_button("insert-field-btn")
                                .small()
                                .label(text("_xlsx.insert-field"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.insert_field_to_calc_formula(window, cx);
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
                    crate::shell::chrome::ActionDefinition::new("calc-field-add", "", crate::shell::chrome::ActionButtonKind::Primary)
                        .label(text("fix.common.add"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.add_or_modify_calc_field(cx);
                        }))
                )

            .action(crate::shell::chrome::dialog_cancel_definition(cx))
                .action(
                    crate::shell::chrome::ActionDefinition::new("calc-field-delete", "", crate::shell::chrome::ActionButtonKind::Destructive)
                        .label(text("_shell.delete"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.delete_calc_field(cx);
                        }))
                );
        Some(SidebarPanel::with_footer(text("_xlsx.insert-calculated-field").to_string(), body.into_any_element(), panel_actions))
    }
}
