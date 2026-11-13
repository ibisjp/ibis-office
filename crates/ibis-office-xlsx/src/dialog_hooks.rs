// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The window's dialogs, as far as the workbook is concerned: what closing and discarding them does to its state, and the like.
//! These were in `shell/dialog.rs`; `FormatEditor`'s dialog hooks call them.

use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The dialogs' parts that are the workbook's (`FormatEditor`'s dialog hooks; the code came from `shell/dialog.rs`).
#[ibis_office_macros::editor_ext(DialogHooks)]
impl SlideEditor {
    /// Closing a dialog the workbook closes in its own way: true if it did.
    pub(crate) fn xlsx_close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.dialog.key==Some("solver_constraint_editor") { if let Some(p)=self.sheet_state_mut().and_then(|s|s.panels.solver.as_mut()){p.editor=None;} self.dialog=Default::default();cx.notify();return true; }
        if self.dialog.key==Some("watch_editor") { if let Some(p)=self.sheet_state_mut().and_then(|s|s.panels.watch_window.as_mut()){p.editor=None;} self.dialog=Default::default();cx.notify();return true; }
        if self.sheet_range_pick_active(){self.finish_sheet_range_pick(true,window,cx);return true;}
        if matches!(self.dialog.key,Some("pivot_value_dialog"|"pivot_field_dialog")) { self.pivot_dialog_closed(window,cx);return true; }
        if self.dialog.key==Some("names_dialog") { self.names_close_dialog(window,cx);return true; }
        if self.dialog.key==Some("conditional_rule_editor") {
            // Back to the manager the editor was opened from.
            self.finish_conditional_editor();
            if let Some(focus)=self.dialog.focus.clone(){focus.focus(window,cx);}
            window.release_pointer();cx.notify();return true;
        }
        if self.dialog.key==Some("sort_level_editor")&&!self.sheet_range_pick_active(){self.finish_sort_level(true,window,cx);return true;}
        if self.dialog.key == Some("chart_series_editor") {
            if self.sheet_range_pick_active() { self.finish_sheet_range_pick(true, window, cx); return true; }
            if let Some(panel) = self.sheet_state_mut().and_then(|s| s.chart_edit.as_mut()) { panel.series_editor = None; }
            self.dialog.active = false; self.dialog.overlay = None; self.dialog.key = None; self.dialog.bounds = None;
            window.release_pointer(); self.focus_handle.focus(window, cx); cx.notify(); return true;
        }
        false
    }

    /// The panel state of the dialog `key` is thrown away: `FormatEditor::discard_dialog`.
    pub(crate) fn discard_xlsx_dialog(&mut self, key: &'static str) {
        let Some(state) = self.sheet_state_mut() else { return; };
        match key {
            "xlsx_print_panel" => { state.print_panel = false; }
            "page_setup_panel" => { state.page_setup = false; state.header_footer_edit = None; }
            // The custom header / footer editor of the page setup dialog: Escape and × go back to the dialog.
            "header_footer_dialog" => { state.header_footer_edit = None; }
            "cell_link_panel" => { state.link = None; }
            "sheet_find_panel" => { state.find = None; }
            "sheet_spelling_panel" => { state.spelling = None; }
            "validation_panel" => { state.validation = None; }
            "names_dialog" => {if let Some(p)=state.names.as_mut(){p.dialog=None;}}
            "conditional_panel" => {state.conditional_edit=None;}
            "dedupe_panel" => { state.panels.dedupe = None; }
            "recommended_charts_panel" => { state.panels.recommended_charts = None; }
            "text_columns_panel" => { state.panels.text_columns = None; }
            "import_text_panel" => { state.panels.import_text = None; }
            "queries_panel" => { state.panels.queries = None; }
            "cells_dialog_panel" => { state.panels.cells_dialog = None; }
            "external_links_panel" => { state.panels.external_links = None; }
            "goto_special_panel" => { state.panels.goto_special = None; }
            "paste_names_panel" => { state.panels.paste_names = None; state.panels.sheet_in = None; }
            "sheet_transfer_panel" => { state.panels.sheet_transfer = None; }
            "protect_panel" => { state.panels.protect = None; }
            "protect_workbook_panel" => { state.panels.protect_workbook = None; }
            "allow_edit_ranges_panel" => { state.panels.allow_edit_ranges = None; }
            "series_panel" => { state.panels.series = None; }
            "goal_seek_panel" => { state.panels.goal_seek = None; }
            "function_panel" => { state.panels.function = None; }
            "sheet_word_count_panel" => { state.panels.word_count = None; }
            "data_table_panel" => { state.panels.data_table = None; }
            "scenario_manager_panel" => { state.panels.scenario_manager = None; }
            "sparkline_panel" => { state.panels.sparkline = None; }
            "subtotal_panel" => { state.panels.subtotal = None; }
            "pivot_group_panel" => { state.panels.pivot_group = None; }
            "pivot_options_panel" => { state.panels.pivot_options = None; }
            "pivot_calc_field_panel" => { state.panels.pivot_calc_field = None; }
            "pivot_calc_item_panel" => { state.panels.pivot_calc_item = None; }
            "pivot_panel" => { state.panels.pivot = None; }
            "advanced_filter_panel" => { state.panels.advanced_filter = None; }
            "autofilter_condition_dialog" | "autofilter_top_dialog" => { state.panels.filter_dialog = None; }
            "consolidate_panel" => { state.panels.consolidate = None; }
            "analysis_panel" => { state.panels.analysis = None; }
            "solver_panel" => { state.panels.solver = None; }
            "slicer_insert_panel" => { state.panels.slicer_insert = None; }
            "timeline_insert_panel" => { state.panels.timeline_insert = None; }
            "watch_window_panel" => { state.panels.watch_window=None; }
            "control_properties_panel" => { state.panels.control_properties = None; }
            "slicer_settings_panel" => { state.panels.slicer_settings = None; }
            "error_options_panel" => { state.panels.error_options = None; }
            "autocorrect_panel" => { state.panels.autocorrect = None; }
            "forecast_panel" => { state.panels.forecast = None; }
            "new_cell_style_panel" => { state.panels.new_cell_style = None; }
            "new_table_style_panel" => { state.panels.new_table_style = None; }
            "xml_map_properties_panel" => { state.panels.xml_map_properties = None; }
            "evaluate_formula_panel" => { state.panels.evaluate_formula = None; }
            "sheet_doc_properties_panel" => { state.panels.properties = None; }
            "xlsx_compatibility_panel" => { state.panels.compatibility = None; }
            "xlsx_theme_panel" => { state.panels.theme = None; }
            "custom_views_panel" => { state.panels.custom_views = None; }
            "unhide_window_panel" => { state.panels.unhide_window = None; }
            "move_chart_panel" => { state.panels.move_chart = None; }
            "chart_template_panel" => { state.panels.chart_template = None; }
            "arrange_windows_panel" => { state.panels.arrange_windows = None; }
            "xlsx_backup_save_settings_panel" => { state.panels.backup_save = None; }
            "xlsx_repair_panel" => { state.panels.repair = None; }
            "phonetic_panel" => { state.panels.phonetic = None; }
            "row_col_size_panel" => { state.panels.row_col_size = None; }
            "paste_special_panel" => { state.panels.paste_special_dialog = None; }
            _ => {}
        }
    }

    /// A rejected cell entry waited on the message: it is cancelled when the message is dismissed.
    pub(crate) fn xlsx_panel_message_dismissed(&mut self, cx: &mut Context<Self>) {
        let rejected = self.sheet_state_mut().and_then(|s| {
            s.validation_message_bounds = None;
            s.validation_rejected.take()
        });
        if let Some((column, row)) = rejected {
            self.cancel_cell_edit(cx);
            self.status = None;
            self.set_cell(column, row);
            if self.current_sheet().is_some_and(|(_, s)|s.cell(row,column).is_some_and(|c|c.formula.is_some() || !c.value.is_empty())) {
                self.run_sheet(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetCells {
                    slide: self.current_slide,
                    edits: vec![ibis_doc_model::CellEdit {row,column,input:ibis_doc_model::CellInput::Clear}],
                })],cx);
            }
        }
    }

    /// Whether the message belongs to a rejected cell entry.
    pub(crate) fn xlsx_panel_message_anchored(&self) -> bool {
        self.sheet_state().and_then(|s| s.validation_rejected).is_some()
    }

    /// Where the message of a rejected cell entry goes: beside the cell, when the whole box fits.
    pub(crate) fn xlsx_panel_message_origin(&self) -> Option<Point<Pixels>> {
        self.sheet_state().and_then(|s| s.validation_rejected).and_then(|(c, r)| self.validation_cell_bounds(c, r)).and_then(|b| {
            // Position against the cell only when the complete box fits.
            let measured = self.sheet_state()?.validation_message_bounds?.size;
            let origin = point(b.right() + px(88.), b.center().y - measured.height / 2.);
            let viewport = self.workspace_scroll.bounds();
            (origin.x >= px(0.) && origin.y >= px(0.) && origin.x + measured.width <= viewport.right() && origin.y + measured.height <= viewport.bottom()).then_some(origin)
        })
    }
}
