// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > What-If 分析 > ゴールシーク…: the cell to change (a constant)
//! that makes a formula's result a value (`ibis_doc_model::goal_seek`). The
//! answer is shown first; 適用 puts it in the cell as one step to undo.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::goal_seek::{GoalSeekError, goal_seek};
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand};

use super::sheet_ui::parse_reference;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct GoalSeekPanel {
    /// 数式入力セル.
    pub(crate) formula: Entity<InputState>,
    /// 目標値.
    pub(crate) target: Entity<InputState>,
    /// 変化させるセル.
    pub(crate) changing: Entity<InputState>,
    /// The answer: the cell (row, column) and its value, until applied.
    pub(crate) found: Option<((u32, u32), f64)>,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    pub(crate) fn open_goal_seek(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let input = |t: String, window: &mut Window, cx: &mut Context<Self>| {
            let state = cx.new(|cx| InputState::new(window, cx));
            state.update(cx, |i, cx| i.set_value(t, window, cx));
            state
        };
        let here = self.current_cell().map(|(c, r)| ibis_doc_model::cell_name(c, r)).unwrap_or_default();
        let panel = GoalSeekPanel {
            formula: input(here, window, cx),
            target: input(String::new(), window, cx),
            changing: input(String::new(), window, cx),
            found: None,
            message: String::new(),
            ui: Default::default(),
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
        s.panels.goal_seek = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn set_goal_seek_cell_from_selection(&mut self, for_changing: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((col, row)) = self.current_cell() else { return };
        let cell_ref = ibis_doc_model::cell_name(col, row);
        let Some(p) = self.sheet_state().and_then(|s| s.panels.goal_seek.as_ref()) else { return };
        let target_input = if for_changing { p.changing.clone() } else { p.formula.clone() };
        target_input.update(cx, |i, cx| i.set_value(cell_ref, window, cx));
        cx.notify();
    }

    fn goal_message(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.goal_seek.as_mut() {
            p.message = message.to_string();
            p.found = None;
        }
        cx.notify();
    }

    /// Resolve a single-cell reference on the current worksheet, supporting A1 references,
    /// sheet-qualified references (`Sheet1!$A$1`), and workbook/sheet-scoped Defined Names.
    pub(crate) fn resolve_single_cell_on_current_sheet(&self, raw: &str) -> Option<(u32, u32)> {
        let clean = raw.trim().trim_start_matches('=').trim();
        if clean.is_empty() {
            return None;
        }
        let parsed = parse_reference(clean)?;
        let (slide, range) = self.resolve_reference(&parsed)?;
        if slide == self.current_slide && range.first_row == range.last_row && range.first_column == range.last_column {
            Some((range.first_row, range.first_column))
        } else {
            None
        }
    }

    /// 実行: look for the value.
    pub(crate) fn run_goal_seek(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.goal_seek.as_ref()) else { return };
        let (formula_text, target_text, changing_text) = (
            p.formula.read(cx).value().to_string(),
            p.target.read(cx).value().to_string(),
            p.changing.read(cx).value().to_string(),
        );
        let Some(formula) = self.resolve_single_cell_on_current_sheet(&formula_text) else {
            return self.goal_message(text("_xlsx.set-cell-must-be-a-single-cell-on-this-sheet"), cx);
        };
        let Some(changing) = self.resolve_single_cell_on_current_sheet(&changing_text) else {
            return self.goal_message(text("_xlsx.by-changing-cell-must-be-a-single-cell-on-this-s"), cx);
        };
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let target = match ibis_calc::input::parse_input(target_text.trim(), date1904) {
            ibis_calc::input::Typed::Value(CellValue::Number(n), _) => n,
            _ => return self.goal_message(text("_xlsx.enter-a-number-for-to-value"), cx),
        };
        match goal_seek(self.presentation(), self.current_slide, formula, target, changing) {
            Ok(x) => {
                let shown = ibis_calc::eval::number_to_text(x);
                let message = crate::shell::i18n::format(
                    "goalseek.found",
                    &[
                        ("formula", &ibis_doc_model::cell_name(formula.1, formula.0)),
                        ("target", &ibis_calc::eval::number_to_text(target)),
                        ("cell", &ibis_doc_model::cell_name(changing.1, changing.0)),
                        ("value", &shown),
                    ],
                );
                if let Some(p) = self.wb_mut().panels.goal_seek.as_mut() {
                    p.found = Some((changing, x));
                    p.message = message;
                }
                cx.notify();
            }
            Err(GoalSeekError::ChangingCell) => self.goal_message(text("_xlsx.by-changing-cell-must-be-a-number-cell-without-a"), cx),
            Err(GoalSeekError::NotANumber) => self.goal_message(text("_xlsx.set-cell-must-be-a-cell-with-a-formula-that-retu"), cx),
            Err(GoalSeekError::NotFound) => self.goal_message(text("_xlsx.no-solution-was-found"), cx),
        }
    }

    /// 適用: the answer put in the cell.
    pub(crate) fn apply_goal_seek(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(((row, column), x)) = self.sheet_state().and_then(|s| s.panels.goal_seek.as_ref()).and_then(|p| p.found) else { return };
        let slide = self.current_slide;
        let edit = CellEdit { row, column, input: CellInput::Value(CellValue::Number(x)) };
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![edit] })]) {
            return self.goal_message(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into(), cx);
        }
        self.wb_mut().panels.goal_seek = None;
        cx.notify();
    }

    pub(crate) fn goal_seek_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.goal_seek.as_ref()?;
        let found = p.found.is_some();
        let ui = p.ui.clone();
        let mark = move |id: &str, child: AnyElement| {
            let ui = ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, _, _| {
                            ui.borrow_mut().insert(id.clone(), bounds);
                        },
                    )
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
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(text("_xlsx.set-cell"))),
            )
            .child(mark("xlsx.goal-seek.formula", super::range_field::local_range_field(&p.formula,"goal_seek-p-formula",true,cx).into_any_element()))
            .child(div().text_sm().child(text("_xlsx.to-value")))
            .child(mark("xlsx.goal-seek.target", crate::shell::chrome::text_field(&p.target).full_width().into_any_element()))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(text("_xlsx.by-changing-cell"))),
            )
            .child(mark("xlsx.goal-seek.changing", super::range_field::local_range_field(&p.changing,"goal_seek-p-changing",true,cx).into_any_element()))
            .children(crate::shell::chrome::panel_message(&p.message))
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(mark_action("xlsx.goal-seek.run", crate::shell::chrome::ActionDefinition::new("goal-run", "", crate::shell::chrome::ActionButtonKind::Primary)
                            .label(text("_xlsx.run"))
                            .on_click(cx.listener(|this, _, _, cx| this.run_goal_seek(cx)))))
                    .when(found, |b| {
                        b.action(mark_action("xlsx.goal-seek.apply", crate::shell::chrome::ActionDefinition::new("goal-apply", "", crate::shell::chrome::ActionButtonKind::Primary)
                                .label(text("_shell.apply"))
                                .on_click(cx.listener(|this, _, _, cx| this.apply_goal_seek(cx)))))
                    })

            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("_xlsx.goal-seek").to_string(), body.into_any_element(), panel_actions))
    }
}
