// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > What-If 分析 > シナリオの登録と管理…:
//! Sheet scenarios management (`SheetCommand::AddScenario`, `EditScenario`,
//! `DeleteScenario`, `ShowScenario`, `CreateScenarioSummary`).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{Command, ScenarioInputCell, SheetCommand, SheetScenario, SlideId};

use super::sheet_ui::{Target, parse_reference};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::{SlideEditor, sheet_of};
use crate::shell::i18n::text;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScenarioDialogMode {
    List,
    AddOrEdit,
    Summary,
    Merge,
}

pub(crate) struct ScenarioManagerPanel {
    pub(crate) mode: ScenarioDialogMode,
    pub(crate) selected_index: Option<usize>,
    pub(crate) name_input: Entity<InputState>,
    pub(crate) changing_cells_input: Entity<InputState>,
    pub(crate) comment_input: Entity<InputState>,
    pub(crate) values_inputs: Vec<(String, Entity<InputState>)>,
    pub(crate) result_cells_input: Entity<InputState>,
    pub(crate) merge_selected_sheet: Option<SlideId>,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

fn parse_cells_input(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for piece in text.split([',', ' ', ';']) {
        let trimmed = piece.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(r) = parse_reference(trimmed) {
            match r.target {
                Target::Cells(range) => {
                    for row in range.first_row..=range.last_row {
                        for col in range.first_column..=range.last_column {
                            let name = ibis_doc_model::cell_name(col, row);
                            if !out.contains(&name) {
                                out.push(name);
                            }
                        }
                    }
                }
                _ => {}
            }
        } else if let Some(cell) = ibis_calc::formula::parser::parse_cell(trimmed) {
            let name = ibis_doc_model::cell_name(cell.column, cell.row);
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out
}

impl SlideEditor {
    pub(crate) fn current_selection_a1_string(&self) -> String {
        self.selected_range()
            .map(|r| {
                if r.first_row == r.last_row && r.first_column == r.last_column {
                    ibis_doc_model::cell_name(r.first_column, r.first_row)
                } else {
                    format!(
                        "{}:{}",
                        ibis_doc_model::cell_name(r.first_column, r.first_row),
                        ibis_doc_model::cell_name(r.last_column, r.last_row)
                    )
                }
            })
            .or_else(|| self.current_cell().map(|(c, r)| ibis_doc_model::cell_name(c, r)))
            .unwrap_or_default()
    }

    pub(crate) fn resolve_scenario_cells(&self, text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for piece in text.split([',', ' ', ';']) {
            let trimmed = piece.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some((slide_id, range)) = parse_reference(trimmed).and_then(|ref_| self.resolve_reference(&ref_))
                && slide_id == self.current_slide
            {
                for row in range.first_row..=range.last_row {
                    for col in range.first_column..=range.last_column {
                        let name = ibis_doc_model::cell_name(col, row);
                        if !out.contains(&name) {
                            out.push(name);
                        }
                    }
                }
                continue;
            }
            for c in parse_cells_input(trimmed) {
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        out
    }

    pub(crate) fn set_scenario_changing_from_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let here = self.current_selection_a1_string();
        if here.is_empty() {
            return;
        }
        if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
            p.changing_cells_input.update(cx, |i, cx| i.set_value(here, window, cx));
        }
        self.prepare_scenario_value_inputs(window, cx);
    }

    pub(crate) fn set_scenario_results_from_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let here = self.current_selection_a1_string();
        if here.is_empty() {
            return;
        }
        if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
            p.result_cells_input.update(cx, |i, cx| i.set_value(here, window, cx));
        }
        cx.notify();
    }

    pub(crate) fn duplicate_selected_scenario(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some((sc, existing_names)) = (|| {
            let p = self.sheet_state()?.panels.scenario_manager.as_ref()?;
            let idx = p.selected_index?;
            let (_, sheet) = self.current_sheet()?;
            let scs = sheet.scenarios.as_ref()?;
            let sc = scs.scenarios.get(idx)?.clone();
            let names: Vec<String> = scs.scenarios.iter().map(|s| s.name.clone()).collect();
            Some((sc, names))
        })() else {
            return;
        };
        let mut n = 2usize;
        let new_name = loop {
            let candidate = format!("{} ({n})", sc.name);
            if !existing_names.contains(&candidate) {
                break candidate;
            }
            n += 1;
        };
        let mut dup = sc;
        dup.name = new_name;
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::AddScenario { slide, scenario: dup })], cx);
        let total = self
            .presentation()
            .slide(slide)
            .and_then(sheet_of)
            .and_then(|s| s.1.scenarios.as_ref())
            .map(|sc| sc.scenarios.len())
            .unwrap_or(1);
        if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
            p.selected_index = Some(total.saturating_sub(1));
        }
        cx.notify();
    }

    pub(crate) fn open_scenario_manager(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let current_scenarios = self.current_sheet().and_then(|(_, s)| s.scenarios.clone());
        let current_idx = current_scenarios.as_ref().and_then(|sc| sc.current).map(|v| v as usize).or_else(|| {
            if current_scenarios.as_ref().is_some_and(|sc| !sc.scenarios.is_empty()) {
                Some(0)
            } else {
                None
            }
        });
        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };
        let here = self.current_selection_a1_string();

        let panel = ScenarioManagerPanel {
            mode: ScenarioDialogMode::List,
            selected_index: current_idx,
            name_input: input(String::new(), window, cx),
            changing_cells_input: input(here.clone(), window, cx),
            comment_input: input(String::new(), window, cx),
            values_inputs: Vec::new(),
            result_cells_input: input(here, window, cx),
            merge_selected_sheet: None,
            message: String::new(),
            ui: Default::default(),
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
        state.panels.scenario_manager = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn show_selected_scenario(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(p) = self.sheet_state().and_then(|s| s.panels.scenario_manager.as_ref()) else { return };
        let Some(idx) = p.selected_index else { return };
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::ShowScenario { slide, index: idx })], cx);
    }

    pub(crate) fn delete_selected_scenario(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.scenario_manager.as_ref()) else { return };
        let Some(idx) = p.selected_index else { return };
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::DeleteScenario { slide, index: idx })], cx);
        let total = self.presentation().slide(slide).and_then(sheet_of).and_then(|s| s.1.scenarios.as_ref()).map(|sc| sc.scenarios.len()).unwrap_or(0);
        if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
            if total == 0 {
                p.selected_index = None;
            } else if idx >= total {
                p.selected_index = Some(total - 1);
            }
        }
        cx.notify();
    }

    pub(crate) fn open_add_scenario(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let here = self.current_selection_a1_string();

        if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
            p.mode = ScenarioDialogMode::AddOrEdit;
            p.selected_index = None;
            p.name_input.update(cx, |i, cx| i.set_value(String::new(), window, cx));
            p.changing_cells_input.update(cx, |i, cx| i.set_value(here.clone(), window, cx));
            p.comment_input.update(cx, |i, cx| i.set_value(String::new(), window, cx));
            p.message.clear();
        }
        self.prepare_scenario_value_inputs(window, cx);
        cx.notify();
    }

    pub(crate) fn open_edit_scenario(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((idx, sc)) = (|| {
            let p = self.sheet_state()?.panels.scenario_manager.as_ref()?;
            let idx = p.selected_index?;
            let (_, sheet) = self.current_sheet()?;
            let sc = sheet.scenarios.as_ref()?.scenarios.get(idx)?.clone();
            Some((idx, sc))
        })() else {
            return;
        };

        let changing_str = sc.input_cells.iter().map(|ic| ic.r.clone()).collect::<Vec<_>>().join(", ");
        if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
            p.mode = ScenarioDialogMode::AddOrEdit;
            p.selected_index = Some(idx);
            p.name_input.update(cx, |i, cx| i.set_value(sc.name.clone(), window, cx));
            p.changing_cells_input.update(cx, |i, cx| i.set_value(changing_str, window, cx));
            p.comment_input.update(cx, |i, cx| i.set_value(sc.comment.clone().unwrap_or_default(), window, cx));
            p.message.clear();
            p.values_inputs.clear();
            for ic in &sc.input_cells {
                let state = cx.new(|cx| InputState::new(window, cx));
                state.update(cx, |i, cx| i.set_value(ic.val.clone(), window, cx));
                p.values_inputs.push((ic.r.clone(), state));
            }
        }
        cx.notify();
    }

    pub(crate) fn prepare_scenario_value_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let cells_str = if let Some(p) = self.sheet_state().and_then(|s| s.panels.scenario_manager.as_ref()) {
            p.changing_cells_input.read(cx).value().to_string()
        } else {
            return;
        };
        let cells = self.resolve_scenario_cells(&cells_str);
        let (_, sheet) = if let Some(s) = self.current_sheet() { s } else { return };
        let mut new_inputs = Vec::new();
        for r_name in cells {
            let default_val = if let Some(c) = ibis_calc::formula::parser::parse_cell(&r_name) {
                sheet
                    .cells
                    .iter()
                    .find(|cell| cell.row == c.row && cell.column == c.column)
                    .map(|cell| match &cell.value {
                        ibis_doc_model::CellValue::Number(n) => n.to_string(),
                        ibis_doc_model::CellValue::Text(t) => t.clone(),
                        ibis_doc_model::CellValue::Bool(b) => b.to_string(),
                        _ => String::new(),
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(default_val, window, cx));
            new_inputs.push((r_name, state));
        }
        if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
            p.values_inputs = new_inputs;
        }
        cx.notify();
    }

    pub(crate) fn save_scenario(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.scenario_manager.as_ref()) else { return };
        let name = p.name_input.read(cx).value().trim().to_string();
        if name.is_empty() {
            if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
                p.message = text("_xlsx.enter-a-scenario-name").to_string();
            }
            return cx.notify();
        }
        let comment = {
            let c = p.comment_input.read(cx).value().trim().to_string();
            if c.is_empty() { None } else { Some(c) }
        };
        let mut input_cells = Vec::new();
        for (r, inp) in &p.values_inputs {
            let val = inp.read(cx).value().trim().to_string();
            input_cells.push(ScenarioInputCell {
                r: r.clone(),
                val,
                num_fmt_id: None,
                deleted: false,
            });
        }
        if input_cells.is_empty() {
            if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
                p.message = text("_xlsx.specify-the-changing-cells").to_string();
            }
            return cx.notify();
        }
        let scenario = SheetScenario {
            name,
            comment,
            user: Some("ibis".into()),
            locked: false,
            hidden: false,
            input_cells,
        };
        let slide = self.current_slide;
        let edit_index = p.selected_index;
        let cmd = match edit_index {
            Some(idx) => SheetCommand::EditScenario { slide, index: idx, scenario },
            None => SheetCommand::AddScenario { slide, scenario },
        };
        self.run_sheet(vec![Command::Sheet(cmd)], cx);
        let total = self.presentation().slide(slide).and_then(sheet_of).and_then(|s| s.1.scenarios.as_ref()).map(|sc| sc.scenarios.len()).unwrap_or(1);
        if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
            p.mode = ScenarioDialogMode::List;
            p.selected_index = Some(edit_index.unwrap_or(total.saturating_sub(1)));
            p.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn open_scenario_summary_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let here = self
            .selected_range()
            .map(|r| {
                if r.first_row == r.last_row && r.first_column == r.last_column {
                    ibis_doc_model::cell_name(r.first_column, r.first_row)
                } else {
                    format!("{}:{}", ibis_doc_model::cell_name(r.first_column, r.first_row), ibis_doc_model::cell_name(r.last_column, r.last_row))
                }
            })
            .or_else(|| self.current_cell().map(|(c, r)| ibis_doc_model::cell_name(c, r)))
            .unwrap_or_default();

        if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
            p.mode = ScenarioDialogMode::Summary;
            p.result_cells_input.update(cx, |i, cx| i.set_value(here, window, cx));
            p.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn run_scenario_summary(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.scenario_manager.as_ref()) else { return };
        let results_str = p.result_cells_input.read(cx).value().trim().to_string();
        let result_cells: Vec<String> = results_str.split([',', ' ', ';']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        if result_cells.is_empty() {
            if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
                p.message = text("_xlsx.specify-the-result-cells").to_string();
            }
            return cx.notify();
        }
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::CreateScenarioSummary { slide, result_cells })], cx);
        if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
            p.mode = ScenarioDialogMode::List;
            p.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn open_merge_scenarios(&mut self, cx: &mut Context<Self>) {
        let other_sheets: Vec<SlideId> = self
            .presentation()
            .slides
            .iter()
            .map(|s| s.id)
            .filter(|&id| id != self.current_slide)
            .collect();
        let first_other = other_sheets.first().copied();
        if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
            p.mode = ScenarioDialogMode::Merge;
            p.merge_selected_sheet = first_other;
            p.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn apply_merge_scenarios_ui(&mut self, cx: &mut Context<Self>) {
        let Some(from_slide) = self.sheet_state().and_then(|s| s.panels.scenario_manager.as_ref()).and_then(|p| p.merge_selected_sheet) else {
            return;
        };
        let slide = self.current_slide;
        self.run_sheet(vec![Command::Sheet(SheetCommand::MergeScenarios { slide, from_slide })], cx);
        if let Some(p) = self.wb_mut().panels.scenario_manager.as_mut() {
            p.mode = ScenarioDialogMode::List;
            p.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn scenario_manager_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.sheet_state()?.panels.scenario_manager.as_ref()?;
        self.render_scenario_manager_panel(cx)
    }

    #[inline(never)]
    fn render_scenario_manager_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let mut panel_fixed_actions = crate::shell::chrome::action_bar();
        let p = self.sheet_state()?.panels.scenario_manager.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(
                    canvas(|_, _, _| {}, move |bounds, _, _, _| {
                        ui.borrow_mut().insert(id.clone(), bounds);
                    })
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
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

        match p.mode {
            ScenarioDialogMode::List => {
                let scenarios = self.current_sheet().and_then(|(_, s)| s.scenarios.clone()).map(|sc| sc.scenarios).unwrap_or_default();
                let sel_idx = p.selected_index;

                body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.scenarios-colon")));

                let mut list_box = v_flex().gap_1().p(px(4.)).border_1().border_color(cx.theme().border).rounded_md().min_h(px(100.));
                if scenarios.is_empty() {
                    list_box = list_box.child(div().text_sm().text_color(cx.theme().muted).child(text("_xlsx.no-scenarios-defined")));
                } else {
                    for (i, sc) in scenarios.iter().enumerate() {
                        let is_selected = Some(i) == sel_idx;
                        let item_el = div()
                            .px(px(6.))
                            .py(px(4.))
                            .rounded_sm()
                            .text_sm()
                            .bg(if is_selected { cx.theme().accent.opacity(0.2) } else { hsla(0.0, 0.0, 0.0, 0.0) })
                            .child(SharedString::from(sc.name.clone()));
                        list_box = list_box.child(mark(
                            &format!("scenario-item-{i}"),
                            Button::new(format!("sc-btn-{i}"))
                                .small()
                                .ghost()
                                .child(item_el)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(p) = this.wb_mut().panels.scenario_manager.as_mut() {
                                        p.selected_index = Some(i);
                                    }
                                    cx.notify();
                                }))
                                .into_any_element(),
                        ));
                    }
                }
                body = body.child(list_box);

                // Scenario details preview (changing cells, comment)
                if let Some(sc) = sel_idx.and_then(|i| scenarios.get(i)) {
                    let changing_labels = sc.input_cells.iter().map(|ic| format!("{}={}", ic.r, ic.val)).collect::<Vec<_>>().join(", ");
                    body = body.child(
                        v_flex()
                            .gap_1()
                            .p(px(6.))
                            .rounded_sm()
                            .bg(cx.theme().muted.opacity(0.1))
                            .child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.changing-cells-colon")))
                            .child(div().text_sm().child(SharedString::from(changing_labels)))
                            .when_some(sc.comment.as_ref(), |el, c| {
                                el.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.comment-colon")))
                                    .child(div().text_sm().child(SharedString::from(c.clone())))
                            }),
                    );
                }

                if !p.message.is_empty() {
                    body = body.children(crate::shell::chrome::panel_message(&p.message));
                }

                // Action buttons grid
                let has_selection = sel_idx.is_some() && !scenarios.is_empty();
                let mut row1 = crate::shell::chrome::action_bar();
                row1 = row1.action(mark_action("scenario-show", crate::shell::chrome::ActionDefinition::new("sc-show-btn", "", crate::shell::chrome::ActionButtonKind::Primary)
                        .disabled(!has_selection)
                        .label(text("_shell.view"))
                        .on_click(cx.listener(|this, _, _, cx| this.show_selected_scenario(cx)))));
                row1 = row1.action(mark_action("scenario-add", crate::shell::chrome::ActionDefinition::new("sc-add-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                        .label(text("_xlsx.add"))
                        .on_click(cx.listener(|this, _, window, cx| this.open_add_scenario(window, cx)))));
                panel_fixed_actions = panel_fixed_actions.actions((row1).into_definitions());

                let mut row2 = crate::shell::chrome::action_bar();
                row2 = row2.action(mark_action("scenario-edit", crate::shell::chrome::ActionDefinition::new("sc-edit-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                        .disabled(!has_selection)
                        .label(text("_xlsx.edit"))
                        .on_click(cx.listener(|this, _, window, cx| this.open_edit_scenario(window, cx)))));
                row2 = row2.action(mark_action("scenario-duplicate", crate::shell::chrome::ActionDefinition::new("sc-dup-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                        .disabled(!has_selection)
                        .label(text("xlsx.scenario-duplicate"))
                        .on_click(cx.listener(|this, _, _, cx| this.duplicate_selected_scenario(cx)))));
                row2 = row2.action(mark_action("scenario-del", crate::shell::chrome::ActionDefinition::new("sc-del-btn", "", crate::shell::chrome::ActionButtonKind::Destructive)
                        .disabled(!has_selection)
                        .label(text("_shell.delete"))
                        .on_click(cx.listener(|this, _, _, cx| this.delete_selected_scenario(cx)))));
                panel_fixed_actions = panel_fixed_actions.actions((row2).into_definitions());

                let mut row3 = crate::shell::chrome::action_bar();
                row3 = row3.action(mark_action("scenario-merge", crate::shell::chrome::ActionDefinition::new("sc-merge-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                        .label(text("_xlsx.merge-ellipsis"))
                        .on_click(cx.listener(|this, _, _, cx| this.open_merge_scenarios(cx)))));
                row3 = row3.action(mark_action("scenario-summary", crate::shell::chrome::ActionDefinition::new("sc-sum-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                        .disabled(scenarios.is_empty())
                        .label(text("_xlsx.summary-ellipsis"))
                        .on_click(cx.listener(|this, _, window, cx| this.open_scenario_summary_dialog(window, cx)))));
                panel_fixed_actions = panel_fixed_actions.actions((row3).into_definitions());

                let mut row4 = crate::shell::chrome::action_bar();
                row4 = row4.action(mark_action("scenario-close", crate::shell::chrome::ActionDefinition::new("sc-close-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                        .label(text("_shell.close"))
                        .on_click(cx.listener(|e, _, w, cx| e.close_panel_dialog(w, cx)))));
                panel_fixed_actions = panel_fixed_actions.actions((row4).into_definitions());
            }
            ScenarioDialogMode::AddOrEdit => {
                let is_edit = p.selected_index.is_some();
                body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text(if is_edit { "fix.protection.edit-scenarios" } else { "シナリオの追加" })));

                body = body.child(div().text_sm().child(text("_xlsx.scenario-name-colon")));
                body = body.child(mark("scenario-name", crate::shell::chrome::text_field(&p.name_input).full_width().into_any_element()));

                body = body.child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(div().text_sm().child(text("_xlsx.changing-cells-colon"))),
                );
                let mut changing_row = h_flex().gap_2();
                changing_row = changing_row.child(div().flex_1().child(mark("scenario-changing", super::range_field::local_range_field(&p.changing_cells_input,"scenario_manager-p-changing_cells_input",false,cx).into_any_element())));
                changing_row = changing_row.child(mark(
                    "scenario-reload-cells",
                    crate::shell::chrome::action_button("sc-reload-btn")
                        .small()
                        .label(text("_xlsx.load-values"))
                        .on_click(cx.listener(|this, _, window, cx| this.prepare_scenario_value_inputs(window, cx)))
                        .into_any_element(),
                ));
                body = body.child(changing_row);

                if !p.values_inputs.is_empty() {
                    body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.values-for-each-changing-cell-colon")));
                    let mut vals_box = v_flex().gap_1().p(px(4.)).border_1().border_color(cx.theme().border).rounded_md().max_h(px(140.));
                    for (r_name, inp) in &p.values_inputs {
                        let mut r_row = h_flex().gap_2().items_center();
                        r_row = r_row.child(div().w(px(60.)).text_sm().child(SharedString::from(r_name.clone())));
                        r_row = r_row.child(div().flex_1().child(mark(&format!("scenario-val-{r_name}"), crate::shell::chrome::text_field(inp).full_width().into_any_element())));
                        vals_box = vals_box.child(r_row);
                    }
                    body = body.child(vals_box);
                }

                body = body.child(div().text_sm().child(text("_xlsx.comment-colon")));
                body = body.child(mark("scenario-comment", crate::shell::chrome::text_field(&p.comment_input).full_width().into_any_element()));

                if !p.message.is_empty() {
                    body = body.child(div().text_sm().text_color(cx.theme().danger).child(SharedString::from(p.message.clone())));
                }

                let mut btn_row = crate::shell::chrome::action_bar().flex_nowrap();
                btn_row = btn_row.action(mark_action("scenario-save", crate::shell::chrome::ActionDefinition::new("sc-save-btn", "", crate::shell::chrome::ActionButtonKind::Primary)
                        .label(text("_shell.save"))
                        .on_click(cx.listener(|this, _, _, cx| this.save_scenario(cx)))));
                btn_row = btn_row.action(mark_action("scenario-cancel", crate::shell::chrome::ActionDefinition::new("sc-cancel-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                        .label(text("_shell.cancel"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(p) = this.wb_mut().panels.scenario_manager.as_mut() {
                                p.mode = ScenarioDialogMode::List;
                                p.message.clear();
                            }
                            cx.notify();
                        }))));
                panel_fixed_actions = panel_fixed_actions.actions((btn_row).into_definitions());
            }
            ScenarioDialogMode::Summary => {
                body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.scenario-summary-report")));
                body = body.child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(div().text_sm().child(text("_xlsx.result-cells-colon"))),
                );
                body = body.child(mark("scenario-result-cells", super::range_field::local_range_field(&p.result_cells_input,"scenario_manager-p-result_cells_input",false,cx).into_any_element()));

                if !p.message.is_empty() {
                    body = body.child(div().text_sm().text_color(cx.theme().danger).child(SharedString::from(p.message.clone())));
                }

                let mut btn_row = crate::shell::chrome::action_bar().flex_nowrap();
                btn_row = btn_row.action(mark_action("scenario-run-summary", crate::shell::chrome::ActionDefinition::new("sc-run-sum-btn", "", crate::shell::chrome::ActionButtonKind::Primary)
                        .label(text("OK"))
                        .on_click(cx.listener(|this, _, _, cx| this.run_scenario_summary(cx)))));
                btn_row = btn_row.action(mark_action("scenario-cancel-summary", crate::shell::chrome::ActionDefinition::new("sc-cancel-sum-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                        .label(text("_shell.cancel"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(p) = this.wb_mut().panels.scenario_manager.as_mut() {
                                p.mode = ScenarioDialogMode::List;
                                p.message.clear();
                            }
                            cx.notify();
                        }))));
                panel_fixed_actions = panel_fixed_actions.actions((btn_row).into_definitions());
            }
            ScenarioDialogMode::Merge => {
                body = body.child(div().text_sm().font_weight(FontWeight::BOLD).child(text("_xlsx.merge-scenarios")));
                body = body.child(div().text_sm().child(text("_xlsx.select-the-sheet-to-copy-scenarios-from")));

                let current_id = self.current_slide;
                let other_sheets: Vec<(SlideId, String, usize)> = self
                    .presentation()
                    .slides
                    .iter()
                    .filter(|s| s.id != current_id)
                    .map(|s| {
                        let name = s.name.clone().unwrap_or_default();
                        let count = sheet_of(s).and_then(|(_, sh)| sh.scenarios.as_ref()).map(|sc| sc.scenarios.len()).unwrap_or(0);
                        (s.id, name, count)
                    })
                    .collect();

                let mut sheet_list = v_flex().gap_1().p(px(4.)).border_1().border_color(cx.theme().border).rounded_md().min_h(px(80.));
                if other_sheets.is_empty() {
                    sheet_list = sheet_list.child(div().text_sm().text_color(cx.theme().muted).child(text("_xlsx.no-other-sheets")));
                } else {
                    for (slide_id, sheet_name, count) in &other_sheets {
                        let is_selected = p.merge_selected_sheet == Some(*slide_id);
                        let sid = *slide_id;
                        let label = format!("{sheet_name} ({count} シナリオ)");
                        let item_el = div()
                            .px(px(6.))
                            .py(px(4.))
                            .rounded_sm()
                            .text_sm()
                            .bg(if is_selected { cx.theme().accent.opacity(0.2) } else { hsla(0.0, 0.0, 0.0, 0.0) })
                            .child(SharedString::from(label));
                        sheet_list = sheet_list.child(mark(
                            &format!("scenario-merge-sheet-{}", sid.0),
                            Button::new(format!("sc-merge-sh-{}", sid.0))
                                .small()
                                .ghost()
                                .child(item_el)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(p) = this.wb_mut().panels.scenario_manager.as_mut() {
                                        p.merge_selected_sheet = Some(sid);
                                    }
                                    cx.notify();
                                }))
                                .into_any_element(),
                        ));
                    }
                }
                body = body.child(sheet_list);

                if !p.message.is_empty() {
                    body = body.child(div().text_sm().text_color(cx.theme().danger).child(SharedString::from(p.message.clone())));
                }

                let can_merge = p.merge_selected_sheet.is_some() && !other_sheets.is_empty();
                let mut actions = crate::shell::chrome::action_bar().flex_nowrap();
                actions = actions.action(mark_action("scenario-merge-ok", crate::shell::chrome::ActionDefinition::new("sc-merge-ok-btn", "", crate::shell::chrome::ActionButtonKind::Primary)
                        .disabled(!can_merge)
                        .label(text("OK"))
                        .on_click(cx.listener(|this, _, _, cx| this.apply_merge_scenarios_ui(cx)))));
                actions = actions.action(mark_action("scenario-merge-cancel", crate::shell::chrome::ActionDefinition::new("sc-merge-cancel-btn", "", crate::shell::chrome::ActionButtonKind::Normal)
                        .label(text("_shell.cancel"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(p) = this.wb_mut().panels.scenario_manager.as_mut() {
                                p.mode = ScenarioDialogMode::List;
                                p.message.clear();
                            }
                            cx.notify();
                        }))));
                panel_fixed_actions = panel_fixed_actions.actions((actions).into_definitions());
            }
        }

        Some(SidebarPanel::with_footer(text("_xlsx.scenario-manager"), body.into_any_element(), panel_fixed_actions))
    }
}
