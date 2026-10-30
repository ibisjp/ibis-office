// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 挿入… and 削除… of the right button on a cell: Excel's one dialog that
//! asks how: the cells beside move right or down (left or up), or the whole
//! row or column goes in (out).
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::xlsx::toggle::toggle_row;
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::sheet_command::Axis;

pub(crate) struct CellsDialog {
    /// 挿入 (true) or 削除.
    pub(crate) insert: bool,
    /// 0 shift right (left), 1 shift down (up), 2 the whole row, 3 the whole column.
    pub(crate) choice: usize,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

const INSERT_OPTIONS: [&str; 4] = ["_shell.insert-cells-shift-cells-right", "docx.context-menu.insert-cells-shift-down", "_xlsx.insert-row", "_xlsx.insert-column"];
const DELETE_OPTIONS: [&str; 4] = ["_shell.delete-cells-shift-cells-left", "docx.context-menu.delete-cells-shift-up", "docx.context-menu.delete-row", "docx.context-menu.delete-column"];

impl SlideEditor {
    /// Whether the selection covers whole rows (either via row header selection,
    /// Shift+Space, or selecting all columns in the sheet).
    pub(crate) fn is_entire_rows_selected(&self) -> bool {
        if let Some(state) = self.sheet_state() {
            if state.header_select == Some(Axis::Rows) {
                return true;
            }
        }
        if let (Some(r), Some((_, sheet))) = (self.selected_range(), self.current_sheet()) {
            let cols = sheet.columns.len().max(1) as u32;
            if r.first_column == 0 && (r.last_column + 1 >= cols || r.last_column >= super::sheet_ui::MAX_COLUMNS - 1) {
                return true;
            }
        }
        false
    }

    /// Whether the selection covers whole columns (either via column header selection,
    /// Ctrl+Space, or selecting all rows in the sheet).
    pub(crate) fn is_entire_columns_selected(&self) -> bool {
        if let Some(state) = self.sheet_state() {
            if state.header_select == Some(Axis::Columns) {
                return true;
            }
        }
        if let (Some(r), Some((_, sheet))) = (self.selected_range(), self.current_sheet()) {
            let rows = sheet.rows.len().max(1) as u32;
            if r.first_row == 0 && (r.last_row + 1 >= rows || r.last_row >= super::sheet_ui::MAX_ROWS - 1) {
                return true;
            }
        }
        false
    }

    /// Excel's ⌘+ (Ctrl++):
    /// - If whole rows are selected: inserts rows above.
    /// - If whole columns are selected: inserts columns to the left.
    /// - Otherwise: opens the 挿入… (insert cells) dialog.
    pub(crate) fn smart_insert(&mut self, cx: &mut Context<Self>) {
        if self.is_entire_rows_selected() {
            self.insert_lines(Axis::Rows, false, cx);
        } else if self.is_entire_columns_selected() {
            self.insert_lines(Axis::Columns, false, cx);
        } else {
            self.open_cells_dialog(true, cx);
        }
    }

    /// Excel's ⌘- (Ctrl+-):
    /// - If whole rows are selected: deletes the rows.
    /// - If whole columns are selected: deletes the columns.
    /// - Otherwise: opens the 削除… (delete cells) dialog.
    pub(crate) fn smart_delete(&mut self, cx: &mut Context<Self>) {
        if self.is_entire_rows_selected() {
            self.delete_lines(Axis::Rows, cx);
        } else if self.is_entire_columns_selected() {
            self.delete_lines(Axis::Columns, cx);
        } else {
            self.open_cells_dialog(false, cx);
        }
    }

    /// 挿入… / 削除… of the cell menu.
    pub(crate) fn open_cells_dialog(&mut self, insert: bool, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let state = self.wb_mut();
        state.close_settings();
        state.panels.cells_dialog = Some(CellsDialog { insert, choice: 0, ui: Default::default() });
        self.sidebar_open = true;
        cx.notify();
    }

    /// OK: the choice made.
    /// Up or Down in the dialog: the next or previous option. Whether a cells dialog is open.
    pub(crate) fn cells_dialog_step(&mut self, step: i32, cx: &mut Context<Self>) -> bool {
        let Some(d) = self.sheet_state_mut().and_then(|s| s.panels.cells_dialog.as_mut()) else { return false };
        d.choice = (d.choice as i32 + step).clamp(0, 3) as usize;
        cx.notify();
        true
    }

    pub(crate) fn apply_cells_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(d) = self.sheet_state().and_then(|s| s.panels.cells_dialog.as_ref()) else { return };
        let (insert, choice) = (d.insert, d.choice);
        match (choice, insert) {
            (0, _) => self.shift_cells(false, insert, cx),
            (1, _) => self.shift_cells(true, insert, cx),
            (2, true) => self.insert_lines(Axis::Rows, false, cx),
            (3, true) => self.insert_lines(Axis::Columns, false, cx),
            (2, false) => self.delete_lines(Axis::Rows, cx),
            _ => self.delete_lines(Axis::Columns, cx),
        }
        if let Some(state) = self.sheet_state_mut() {
            state.panels.cells_dialog = None;
        }
        cx.notify();
    }

    pub(crate) fn cells_dialog_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let d = self.sheet_state()?.panels.cells_dialog.as_ref()?;
        let mark = |id: String, child: AnyElement| {
            let ui = d.ui.clone();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: String, action: crate::shell::chrome::ActionDefinition| {
            let ui = d.ui.clone();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let options = if d.insert { INSERT_OPTIONS } else { DELETE_OPTIONS };
        let mut body = v_flex().gap_1().px(px(18.)).py(px(8.)).p_0();
        for (i, key) in options.iter().enumerate() {
            body = body.child(mark(
                format!("option-{i}"),
                toggle_row(SharedString::from(format!("cells-option-{i}")), d.choice == i, crate::shell::i18n::text(key), cx.listener(move |this, _, _, cx| {
                    if let Some(d) = this.sheet_state_mut().and_then(|s| s.panels.cells_dialog.as_mut()) {
                        d.choice = i;
                    }
                    cx.notify();
                }))
                .into_any_element(),
            ));
        }
        let title = if d.insert { "cells.insert-title" } else { "cells.delete-title" };
        let panel_terminal_actions = crate::shell::chrome::action_bar()
                .action(mark_action("ok".into(), crate::shell::chrome::ActionDefinition::new("cells-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label(crate::shell::i18n::text("query.ok")).on_click(cx.listener(|this, _, _, cx| this.apply_cells_dialog(cx)))))
                .action(mark_action("cancel".into(), crate::shell::chrome::ActionDefinition::new("cells-cancel", "", crate::shell::chrome::ActionButtonKind::Normal)
                        .label(crate::shell::i18n::text("query.cancel"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(state) = this.sheet_state_mut() {
                                state.panels.cells_dialog = None;
                            }
                            cx.notify();
                        }))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text(title), body.into_any_element(), panel_terminal_actions))
    }
}
