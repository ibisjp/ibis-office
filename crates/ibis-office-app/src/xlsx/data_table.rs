// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > What-If 分析 > データテーブル…: with the table's cells selected
//! (the values across its top and down its left included), the cells that
//! take those values: 行の入力セル (for the values across), 列の入力セル (for
//! the values down), one or both. The results fill the rest of the
//! selection (`ibis_doc_model::data_table`).
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellRange, Command, DataTable, SheetCommand};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct DataTablePanel {
    /// The cells selected: the table with its headers.
    pub(crate) range: CellRange,
    pub(crate) row_input: Entity<InputState>,
    pub(crate) column_input: Entity<InputState>,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

impl SlideEditor {
    pub(crate) fn open_data_table(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(range) = self.selected_range().filter(|r| r.last_row > r.first_row && r.last_column > r.first_column) else {
            self.status = Some(text("_xlsx.select-the-cells-of-the-data-table-including-the").into());
            return cx.notify();
        };
        let input = |window: &mut Window, cx: &mut Context<Self>| cx.new(|cx| InputState::new(window, cx));
        let panel = DataTablePanel {
            range,
            row_input: input(window, cx),
            column_input: input(window, cx),
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
        s.panels.data_table = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn set_data_table_input_from_selection(&mut self, for_column: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((col, row)) = self.current_cell() else { return };
        let cell_ref = ibis_doc_model::cell_name(col, row);
        let Some(p) = self.sheet_state().and_then(|s| s.panels.data_table.as_ref()) else { return };
        let target_input = if for_column { p.column_input.clone() } else { p.row_input.clone() };
        target_input.update(cx, |i, cx| i.set_value(cell_ref, window, cx));
        cx.notify();
    }

    fn data_table_message(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.data_table.as_mut() {
            p.message = message.to_string();
        }
        cx.notify();
    }

    /// OK: the table made, its results filled in (one step to undo).
    pub(crate) fn apply_data_table(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(p) = self.sheet_state().and_then(|s| s.panels.data_table.as_ref()) else { return };
        let (range, row_text, column_text) = (
            p.range,
            p.row_input.read(cx).value().to_string(),
            p.column_input.read(cx).value().to_string(),
        );
        let cell = |t: &str| -> Result<Option<(u32, u32)>, ()> {
            if t.trim().is_empty() {
                return Ok(None);
            }
            self.resolve_single_cell_on_current_sheet(t).map(Some).ok_or(())
        };
        let (Ok(row_input), Ok(column_input)) = (cell(&row_text), cell(&column_text)) else {
            return self.data_table_message(text("_xlsx.an-input-cell-must-be-a-single-cell-on-this-shee"), cx);
        };
        if row_input.is_none() && column_input.is_none() {
            return self.data_table_message(text("_xlsx.specify-a-row-input-cell-a-column-input-cell-or"), cx);
        }
        let results = CellRange { first_row: range.first_row + 1, first_column: range.first_column + 1, ..range };
        let table = DataTable { range: results, row_input, column_input };
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetDataTable { slide, table })]) {
            return self.data_table_message(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into(), cx);
        }
        self.wb_mut().panels.data_table = None;
        cx.notify();
    }

    pub(crate) fn data_table_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.data_table.as_ref()?;
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
                    .child(div().text_sm().child(text("_xlsx.row-input-cell"))),
            )
            .child(mark("xlsx.data-table.row-input", super::range_field::local_range_field(&p.row_input,"data_table-p-row_input",true,cx).into_any_element()))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(div().text_sm().child(text("_xlsx.column-input-cell"))),
            )
            .child(mark("xlsx.data-table.column-input", super::range_field::local_range_field(&p.column_input,"data_table-p-column_input",true,cx).into_any_element()))
            .children(crate::shell::chrome::panel_message(&p.message))
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(mark_action("xlsx.data-table.ok", crate::shell::chrome::ActionDefinition::new("data-table-ok", "", crate::shell::chrome::ActionButtonKind::Primary)
                            .label(text("OK"))
                            .on_click(cx.listener(|this, _, _, cx| this.apply_data_table(cx)))))

            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("_xlsx.data-table").to_string(), body.into_any_element(), panel_actions))
    }
}
