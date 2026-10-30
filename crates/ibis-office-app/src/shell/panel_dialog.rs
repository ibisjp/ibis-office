// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Movable, resizable settings dialogs. Modal dialogs freeze edits, not the document viewport.
mod resize;
use resize::ResizeEdge;
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::{h_flex, v_flex};
use super::{chrome::{SidebarPanel, BORDER, TEXT, icon}, editor::SlideEditor, palette::rgb};

pub(crate) mod form;
pub(crate) use form::SubmitType;
actions!(panel_dialog,[SubmitEnter,SubmitModifiedEnter]);
pub(crate) fn init(cx:&mut App){cx.bind_keys([
    KeyBinding::new("enter",SubmitEnter,Some("PanelDialog")),
    KeyBinding::new(if cfg!(target_os="macos"){"cmd-enter"}else{"ctrl-enter"},SubmitModifiedEnter,Some("PanelDialog")),
]);}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PanelMode {
    #[default]
    Sidebar,
    Modal,
    Modeless,
}

/// One authority for input ownership. A confirmation always supersedes a settings/range session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InteractionLayer { Document, Settings, SheetRange, Message, Zoom, Save, External }
impl InteractionLayer {
    pub(crate) fn allows_range_pick(self) -> bool { self == Self::SheetRange }
    pub(crate) fn is_confirmation(self) -> bool { matches!(self,Self::Message|Self::Zoom|Self::Save|Self::External) }
}

#[derive(Clone, Copy)]
pub(crate) enum DialogDrag {
    Move { pointer: Point<Pixels>, origin: Point<Pixels> },
    Resize { pointer: Point<Pixels>, bounds: Bounds<Pixels>, edge: ResizeEdge },
}

#[derive(Default)]
pub(crate) struct PanelDialogState {
    pub mode: PanelMode,
    pub key: Option<&'static str>,
    pub active: bool,
    pub title: SharedString,
    pub bounds: Option<Bounds<Pixels>>,
    pub drag: Option<DialogDrag>,
    pub user_resized: bool,
    pub user_moved: bool,
    /// Center once using the measured content, then preserve the top across content changes.
    pub content_measured: bool,
    pub footer_height: Pixels,
    pub focus: Option<FocusHandle>,
    pub focus_initialized: bool,
    pub form_controls:Vec<gpui_kit::component::form_controls::FormControl>,
    pub submit:form::SubmitType,
    pub submit_handler:Option<super::chrome::action_bar::ClickHandler>,
    pub single_normal:bool,
    pub has_submit_action:bool,
    pub confirmation_restore: Option<FocusHandle>,
    pub scroll: ScrollHandle,
    pub overlay: Option<AnyElement>,
}

impl SlideEditor {
    pub(crate) fn interaction_layer(&self) -> InteractionLayer {
        if self.office_path_confirm_dialog_open {InteractionLayer::External}
        else if self.save_confirm_dialog_open {InteractionLayer::Save}
        else if self.zoom_dialog_open {InteractionLayer::Zoom}
        else if self.panel_message.is_some() {InteractionLayer::Message}
        else if self.sheet_range_pick_active() {InteractionLayer::SheetRange}
        else if self.sidebar_open && self.panel_dialog.active {InteractionLayer::Settings}
        else {InteractionLayer::Document}
    }
    /// Render and route from the same discriminant. Callers cannot place a confirmation below a modeless panel.
    pub(crate) fn render_dialog_stack(&mut self, d: Stateful<Div>, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let layer=self.interaction_layer();
        if layer.is_confirmation() && layer != InteractionLayer::Zoom {
            // Native text input and widget actions must not keep targeting a covered field.
            if self.panel_dialog.confirmation_restore.is_none() {
                self.panel_dialog.confirmation_restore=window.focused(cx).or_else(||Some(self.focus_handle.clone()));
                self.focus_handle.focus(window,cx);
            }
        } else if let Some(previous)=self.panel_dialog.confirmation_restore.take() {
            if self.has_document {previous.focus(window,cx);} else {self.focus_handle.focus(window,cx);}
        }
        let d=d.children(self.panel_dialog.overlay.take());
        match layer {
            InteractionLayer::External=>d.child(self.render_office_path_confirm_dialog(window,cx)),
            InteractionLayer::Save=>d.child(self.render_save_confirm_dialog(window,cx)),
            InteractionLayer::Zoom=>d.child(self.render_zoom_dialog(window,cx)),
            InteractionLayer::Message=>d.child(self.render_panel_message(cx)),
            _=>d.children(self.render_hyperlink_context()),
        }
    }
    pub(crate) fn show_panel_message(&mut self, text: SharedString, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() { s.validation_rejected = None; s.validation_message_bounds = None; }
        self.panel_message=Some(text);
        self.panel_dialog.drag=None;
        if let Some(state)=&self.active_table_reorder {state.borrow_mut().cancel();}
        cx.notify();
    }
    pub(crate) fn dismiss_panel_message(&mut self, cx: &mut Context<Self>) {
        self.panel_message = None;
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
        cx.notify();
    }
    pub(crate) fn render_panel_message(&self, cx: &Context<Self>) -> impl IntoElement {
        let editor_measure = cx.entity().downgrade();
        // The same occluding shell is used by every blocking confirmation.
        confirmation_surface("panel-message-backdrop").bg(gpui_kit::rgba(0x00000000))
            .when(self.sidebar_open && self.sheet_state().and_then(|s|s.validation_rejected).is_none(),|d|d.justify_end())
            .child(v_flex().id("panel-message").relative()
                .when_some(self.sheet_state().and_then(|s|s.validation_rejected).and_then(|(c,r)|self.validation_cell_bounds(c,r)).and_then(|b| {
                    // Position against the cell only when the complete box fits.
                    let measured = self.sheet_state()?.validation_message_bounds?.size;
                    let origin=point(b.right()+px(88.),b.center().y-measured.height/2.);
                    let viewport=self.workspace_scroll.bounds();
                    (origin.x>=px(0.) && origin.y>=px(0.) && origin.x+measured.width<=viewport.right() && origin.y+measured.height<=viewport.bottom()).then_some(origin)
                }),|d,p|d.absolute().left(p.x).top(p.y))
                .w(px(380.)).max_w_full().p_5().gap_4()
                .child(canvas(|_,_,_|(),move |bounds,_,_,cx| {
                    let editor=editor_measure.clone();
                    cx.defer(move |cx| { let _=editor.update(cx,|e,cx| {
                        if let Some(s)=e.sheet_state_mut() {
                            if s.validation_rejected.is_some() && s.validation_message_bounds != Some(bounds) {
                                s.validation_message_bounds=Some(bounds); cx.notify();
                            }
                        }
                    }); });
                }).absolute().inset_0())
                .bg(rgb(0xffffff)).rounded_lg().shadow_lg()
                .child(self.panel_message.clone().unwrap_or_default())
                .child(h_flex().justify_center().child(super::chrome::primary_button("panel-message-ok")
                    .label("OK")
                    .on_click(cx.listener(|e,_,_,cx|{e.dismiss_panel_message(cx);})))))
    }
    pub(crate) fn panel_dialog_is_modal(&self) -> bool {
        self.sidebar_open && self.panel_dialog.active && self.panel_dialog.mode == PanelMode::Modal
    }

    pub(crate) fn close_panel_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel_dialog.key==Some("color_picker_dialog"){self.close_color_picker_dialog(window,cx);return;}
        if self.theme_colors_ui.editing && matches!(self.panel_dialog.key,Some("docx_theme_panel"|"xlsx_theme_panel"|"theme_panel")) {self.set_theme_colors_editor(false,window,cx);return;}
        if self.panel_dialog.key==Some("solver_constraint_editor") { if let Some(p)=self.sheet_state_mut().and_then(|s|s.panels.solver.as_mut()){p.editor=None;} self.panel_dialog=Default::default();cx.notify();return; }
        if self.panel_dialog.key==Some("watch_editor") { if let Some(p)=self.sheet_state_mut().and_then(|s|s.panels.watch_window.as_mut()){p.editor=None;} self.panel_dialog=Default::default();cx.notify();return; }
        if self.sheet_range_pick_active(){self.finish_sheet_range_pick(true,window,cx);return;}
        if self.panel_dialog.key==Some("sort_level_editor")&&!self.sheet_range_pick_active(){self.finish_sort_level(true,window,cx);return;}
        if self.panel_dialog.key == Some("chart_series_editor") {
            if self.sheet_range_pick_active() { self.finish_sheet_range_pick(true, window, cx); return; }
            if let Some(panel) = self.sheet_state_mut().and_then(|s| s.chart_edit.as_mut()) { panel.series_editor = None; }
            self.panel_dialog.active = false; self.panel_dialog.overlay = None; self.panel_dialog.key = None; self.panel_dialog.bounds = None;
            window.release_pointer(); self.focus_handle.focus(window, cx); cx.notify(); return;
        }
        if self.panel_dialog.key.is_some_and(|key|key.contains("autocorrect")) && self.cancel_autocorrect_entry() { self.panel_dialog.bounds=None;cx.notify();return; }
        if self.panel_dialog.key.is_some_and(|key|key.starts_with("merge_")) {
            self.cancel_merge_workflow_dialog(window,cx);
            self.panel_dialog.active=false;self.panel_dialog.overlay=None;self.panel_dialog.key=None;
            window.release_pointer();self.focus_handle.focus(window,cx);cx.notify();return;
        }
        if self.panel_dialog.key==Some("tab_stop_editor") {
            if let Some(p)=self.doc_state_mut().and_then(|s|s.tab_edit.as_mut()){p.editor=None;}
            self.panel_dialog.active=false;self.panel_dialog.overlay=None;self.panel_dialog.key=None;
            window.release_pointer();self.focus_handle.focus(window,cx);cx.notify();return;
        }
        let keep_chart_pane = self.is_workbook() && self.sheet_state().is_some_and(|state|state.chart_edit.is_some())
            && matches!(self.panel_dialog.key, Some("move_chart_panel" | "chart_template_panel"));
        self.discard_current_panel_dialog();
        self.sidebar_open = keep_chart_pane;
        self.panel_dialog.active = false;
        self.panel_dialog.drag = None;
        window.release_pointer();
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// Discard only the current command's UI draft. Applied document edits and unrelated live panes stay intact.
    fn discard_current_panel_dialog(&mut self) {
        let Some(key) = self.panel_dialog.key.take() else { return; };
        match key {
            "symbol_panel" => {
                self.symbol = None;
                if let Some(state) = self.doc_state_mut() { state.symbol = None; }
                return;
            }
            "autorecover_settings_panel" => { self.autorecover_settings = None; return; }
            "recover_unsaved_panel" => { self.recover_unsaved = None; return; }
            _ => {}
        }
        match self.format.kind() {
            ibis_pptx::Kind::Document => {
                let Some(state) = self.doc_state_mut() else { return; };
                match key {
                    "font_panel" => { state.font_edit = None; }
                    "style_panel" => { state.style_edit = None; }
                    "new_style_panel" => { state.helper.new_style = None; }
                    "color_panel" => { state.color_edit = None; }
                    "table_size_panel" => { state.table_size = None; }
                    "table_formula_panel" => { state.table_formula = None; }
                    "docx_compress_panel" => { state.compress_panel = None; }
                    "paragraph_panel" => { state.paragraph_edit = None; }
                    "tab_panel" => { state.tab_edit = None; }
                    "link_panel" => { state.link = None; }
                    "find_panel" => { state.find = None; }
                    "ruby_panel" => { state.ruby = None; }
                    "fit_text_panel" => { state.fit_text_edit = None; }
                    "merge_panel" => { if let Some(merge) = state.merge.as_mut() { merge.visible = false; } }
                    "doc_page_setup_panel" => { state.helper.page_setup = None; }
                    "notes_panel" => { state.helper.notes = None; }
                    "bookmark_panel" => { state.helper.bookmarks = None; }
                    "watermark_panel" => { state.helper.watermark = None; }
                    "word_count_panel" => { state.helper.word_count = None; }
                    "field_panel" => { state.helper.field = None; }
                    "caption_panel" => { state.helper.caption = None; }
                    "doc_properties_panel" => { state.helper.properties = None; }
                    "cross_reference_panel" => { state.helper.cross_reference = None; }
                    "update_toc_panel" => { state.helper.update_toc = None; }
                    "user_info_panel" => { state.helper.user_info = None; }
                    "kinsoku_panel" => { state.helper.kinsoku = None; }
                    "go_to_panel" => { state.helper.go_to = None; }
                    "control_panel" => { state.control = None; }
                    "doc_autocorrect_panel" => { state.autocorrect = None; }
                    "docx_math_panel" => { state.docx_math = None; }
                    "quick_parts_panel" => { state.quick_parts = None; }
                    "doc_inspect_panel" => { state.inspect_panel = None; }
                    "doc_compatibility_panel" => { state.compatibility_panel = None; }
                    "doc_print_panel" => { state.print_panel = None; }
                    "doc_source_manager_panel" => { state.source_manager = None; }
                    "doc_repair_panel" => { state.repair = None; }
                    "doc_template_panel" => { state.template_panel = None; }
                    "list_style_panel" => { state.list_style = None; }
                    "docx_theme_panel" => { state.theme_panel = None; }
                    _ => {}
                }
            }
            ibis_pptx::Kind::Workbook => {
                let Some(state) = self.sheet_state_mut() else { return; };
                match key {
                    "page_setup_panel" => { state.page_setup = false; state.header_footer_edit = None; }
                    "cell_link_panel" => { state.link = None; }
                    "sheet_find_panel" => { state.find = None; }
                    "sheet_spelling_panel" => { state.spelling = None; }
                    "validation_panel" => { state.validation = None; }
                    "names_panel" => {state.names=None;}
                    "conditional_panel" => {state.conditional_edit=None;}
                    "dedupe_panel" => { state.panels.dedupe = None; }
                    "recommended_charts_panel" => { state.panels.recommended_charts = None; }
                    "text_columns_panel" => { state.panels.text_columns = None; }
                    "import_text_panel" => { state.panels.import_text = None; }
                    "queries_panel" => { state.panels.queries = None; }
                    "cells_dialog_panel" => { state.panels.cells_dialog = None; }
                    "external_links_panel" => { state.panels.external_links = None; }
                    "goto_special_panel" => { state.panels.goto_special = None; }
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
            ibis_pptx::Kind::Presentation => {
                let Some(state) = self.pptx_mut() else { return; };
                match key {
                    "pptx_trust_center_panel" => { state.trust_center = None; }
                    "audio_record_panel" => { state.audio_record = None; }
                    "pptx_gradient_panel" => { state.gradient_panel = None; }
                    "shadow_panel" => { state.shadow_panel = None; }
                    "glow_panel" => { state.glow_panel = None; }
                    "frame_panel" => { state.frame_panel = None; }
                    "text_insets_panel" => { state.insets_panel = None; }
                    "alt_text_panel" => { state.alt_text_panel = None; }
                    "chart_panel" => { state.chart_panel = None; }
                    "compare_panel" => { state.compare_panel = None; }
                    "photo_album_panel" => { state.photo_album = None; }
                    "pptx_autocorrect_panel" => { state.autocorrect = None; }
                    "pptx_math_panel" => { state.math_panel = None; }
                    "animation_options_panel" => { state.animation_options_panel = None; }
                    "pptx_compatibility_panel" => { state.compatibility_panel = None; }
                    "presentation_inspect_panel" => { state.inspect_panel = None; }
                    "presentation_properties_panel" => { state.properties_panel = None; }
                    "notes_marks_panel" => { state.notes_marks_panel = None; }
                    "compress_panel" => { state.compress_panel = None; }
                    "print_panel" => { state.print_panel = None; }
                    "layout_name_panel" => { state.layout_panel = None; }
                    "pptx_section_panel" => { state.section_panel = None; }
                    "pptx_paragraph_panel" => { state.paragraph_panel = None; }
                    "timing_panel" => { state.timing_panel = None; }
                    "show_panel" => { state.show_panel = None; }
                    "crop_panel" => { state.crop_panel = None; }
                    "cell_size_panel" => { state.cell_size_panel = None; }
                    "header_footer_panel" => { state.header_footer = None; }
                    "slide_link_panel" => { state.link_panel = None; }
                    "theme_panel" => { state.theme_panel = None; }
                    "slide_size_panel" => { state.size_panel = None; }
                    "pptx_find_panel" => { state.find = None; }
                    "replace_fonts_panel" => { state.replace_fonts = None; }
                    _ => {}
                }
            }
        }
    }

    /// Modal settings block background selection and commands, but retain the scrollbars and zoom controls.
    pub(crate) fn panel_dialog_allows_pointer(&self, position: Point<Pixels>, _window: &Window) -> bool {
        // The save question is a higher-priority overlay and supplies its own backdrop.
        if self.interaction_layer().is_confirmation() { return true; }
        if !self.panel_dialog_is_modal() { return true; }
        if self.panel_dialog.bounds.is_some_and(|b| b.contains(&position)) { return true; }
        // Native title row: window dragging and window controls remain available, and so does the logo, which opens the start window.
        if position.y < px(36.) { return true; }
        if self.logo_bounds.get().is_some_and(|b| b.contains(&position)) { return true; }
        if self.workspace_bounds.is_some_and(|b| b.contains(&position)) { return true; }
        self.zoom_controls_bounds.get().is_some_and(|b| b.contains(&position))
    }

    /// Run before keymap resolution, so format-specific superscript bindings cannot consume viewport zoom.
    pub(crate) fn modal_viewport_keystroke(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) {
        if !self.panel_dialog_is_modal() || self.save_confirm_dialog_open { return; }
        // Some formats bind Cmd++ to character formatting. Modal viewport zoom takes precedence.
        if (key.modifiers.platform || key.modifiers.control) && matches!(key.key.as_str(), "+" | "=" | "-" | "0") {
            match key.key.as_str() {
                "+" | "=" => self.step_zoom(true, window, cx),
                "-" => self.step_zoom(false, window, cx),
                "0" if key.modifiers.alt => self.set_zoom(None, window, cx),
                _ => self.set_zoom(Some(1.), window, cx),
            }
            cx.stop_propagation();
            return;
        }
    }

    pub(crate) fn cancel_table_reorder(&mut self,window:&mut Window,cx:&mut Context<Self>)->bool{
        if self.active_table_reorder.as_ref().is_some_and(|s|s.borrow_mut().cancel()){window.release_pointer();cx.notify();true}else{false}
    }
    pub(crate) fn panel_dialog_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key=="escape"&&self.cancel_table_reorder(window,cx){cx.stop_propagation();return;}
        let key = &event.keystroke;
        if self.interaction_layer() == InteractionLayer::Zoom {return;}
        if self.interaction_layer().is_confirmation() {
            if self.panel_message.is_some() && matches!(key.key.as_str(),"escape"|"enter") {self.dismiss_panel_message(cx);}
            else if self.save_confirm_dialog_open && key.key == "escape" {self.close_save_confirm_dialog(cx);}
            cx.stop_propagation();
            return;
        }
        if key.key=="enter" && !key.modifiers.alt && self.dialog_enter(if cfg!(target_os="macos"){key.modifiers.platform}else{key.modifiers.control},key.modifiers.shift,window,cx) {window.prevent_default();cx.stop_propagation();return;}
        // Up and Down choose among the options of the insert or delete cells dialog.
        if matches!(key.key.as_str(),"up"|"down") && !key.modifiers.platform && !key.modifiers.control && !key.modifiers.alt && !key.modifiers.shift && self.cells_dialog_step(if key.key=="down"{1}else{-1},cx) {cx.stop_propagation();return;}
        if !self.panel_dialog_is_modal() { return; }
        if self.save_confirm_dialog_open {
            if key.key == "escape" { self.close_save_confirm_dialog(cx); }
            cx.stop_propagation();
            return;
        }
        if key.key == "escape" {
            if self.dismiss_toolbar_dropdown(window,cx) {cx.stop_propagation();return;}
            if self.active_table_reorder.as_ref().is_some_and(|s|s.borrow_mut().cancel()){window.release_pointer();cx.stop_propagation();cx.notify();return;}
            self.close_panel_dialog(window, cx);
            cx.stop_propagation();
            return;
        }
        // A focused field still edits its own text; keyboard navigation must never reach the document.
        let searching = self.toolbar_search_open.and_then(|id| self.toolbar_search_inputs.get(id))
            .is_some_and(|input| input.read(cx).focus_handle(cx).is_focused(window));
        if !searching && !self.panel_dialog.focus.as_ref().is_some_and(|f| f.contains_focused(window, cx)) {
            cx.stop_propagation();
        }
    }

    fn dialog_enter(&mut self,modified:bool,shift:bool,window:&mut Window,cx:&mut Context<Self>)->bool {
        if shift || !self.sidebar_open || !self.panel_dialog.active || self.font_size_menu_open || self.toolbar_search_open.is_some()
            || !(self.panel_dialog_is_modal() || self.panel_dialog.focus.as_ref().is_some_and(|f|f.contains_focused(window,cx))) {return false;}
        let composing=self.panel_dialog.form_controls.iter().find(|c|c.focus.as_ref().is_some_and(|f|f.is_focused(window))).and_then(|c|c.composing.clone());
        if composing.is_some_and(|f|f(window,cx)){return false;}
        if self.panel_dialog.has_submit_action && form::shortcut(self.panel_dialog.submit,&self.panel_dialog.form_controls,self.panel_dialog.single_normal,modified) {if let Some(handler)=self.panel_dialog.submit_handler.clone(){window.defer(cx,move|w,cx|handler(&ClickEvent::default(),w,cx));}return true;}
        // Do not let old single-field Enter callbacks bypass a multi-field form's policy.
        // A field with its own Enter (the import dialog's file path loads the file; it does not submit the form) keeps it.
        if self.import_text_path_focused(window,cx){return false;}
        self.panel_dialog.has_submit_action && self.panel_dialog.form_controls.iter().any(|c|c.kind==gpui_kit::component::form_controls::ControlKind::Text && c.focus.as_ref().is_some_and(|f|f.is_focused(window)))
    }

    /// Actions dispatch before ordinary key handlers. Block document actions in capture,
    /// while leaving the input widget's own copy/paste/undo actions available.
    pub(crate) fn guard_panel_actions(&self, mut element: Stateful<Div>, cx: &Context<Self>) -> Stateful<Div> {
        element=element.capture_action(cx.listener(|e,action:&gpui_kit::component::input::Enter,w,cx|{if e.dialog_enter(action.secondary,action.shift,w,cx){cx.stop_propagation();}}));
        let names: Vec<_> = cx.all_action_names().iter().copied().filter(|name| {
            name.starts_with("ibis_office::") || name.starts_with("docx::") || name.starts_with("pptx::")
        }).collect();
        for name in names {
            if ["::OpenStartWindow", "::Quit"]
                .iter().any(|suffix| name.ends_with(suffix)) { continue; }
            let Ok(action) = cx.build_action(name, None) else { continue; };
            let editor = cx.weak_entity();
            element.interactivity().capture_action_type(action.as_any().type_id(), move |_, window, app| {
                editor.update(app, |this, cx| {
                    if name.ends_with("::CancelTool")&&this.dismiss_toolbar_dropdown(window,cx){cx.stop_propagation();return;}
                    if name.ends_with("::CancelTool")&&this.cancel_table_reorder(window,cx){cx.stop_propagation();return;}
                    if this.panel_dialog_is_modal() || this.interaction_layer().is_confirmation() {
                        if name.ends_with("::CancelTool") {
                            if this.save_confirm_dialog_open { this.close_save_confirm_dialog(cx); }
                            else if this.panel_message.is_some() {this.dismiss_panel_message(cx);}
                            else if this.interaction_layer()==InteractionLayer::Zoom {this.close_zoom_dialog(cx);}
                            else if !this.interaction_layer().is_confirmation() { this.close_panel_dialog(window, cx); }
                        }
                        cx.stop_propagation();
                    }
                }).ok();
            });
        }
        element
    }

    pub(crate) fn move_panel_dialog(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.panel_dialog.drag else { return false; };
        let Some(mut bounds) = self.panel_dialog.bounds else { return false; };
        let viewport = window.viewport_size();
        match drag {
            DialogDrag::Move { pointer, origin } => {
                let offset = event.position - pointer;
                self.panel_dialog.user_moved = true;
                bounds.origin.x = origin.x + offset.x;
                bounds.origin.y = origin.y + offset.y;
            }
            DialogDrag::Resize { pointer, bounds: original, edge } => {
                self.panel_dialog.user_resized = true;
                bounds = edge.bounds(original, event.position-pointer, viewport);
            }
        }
        self.panel_dialog.bounds = Some(bounds);
        cx.notify();
        true
    }

    pub(crate) fn render_panel_dialog(&mut self, panel: SidebarPanel, footer: Option<crate::shell::chrome::ActionBar>, mode: PanelMode,
        window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (panel,footer,mode)=if self.sheet_range_pick_active() && self.panel_dialog.key!=Some("chart_series_editor") {
            let owner_title=panel.title.clone();
            let mut picker=self.generic_range_pick_panel(cx).unwrap();picker.title=owner_title;
            let footer=super::chrome::take_sidebar_footer();
            (picker,footer,PanelMode::Modeless)
        } else {(panel,footer,mode)};
        let footer=footer.or_else(||Some(super::chrome::action_bar().action(super::chrome::ActionDefinition::new("dialog-default-close",super::i18n::text("_shell.close"),super::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e,_,w,cx|e.close_panel_dialog(w,cx))))));
        let options=form::OPTIONS.with(|o|o.get());
        let (submit_handler,single_normal,has_submit_action)=footer.as_ref().map(|f|f.submit_action()).unwrap_or_default();
        self.panel_dialog.submit=options.submit;self.panel_dialog.submit_handler=submit_handler;self.panel_dialog.single_normal=single_normal;self.panel_dialog.has_submit_action=has_submit_action;
        let viewport = window.viewport_size();
        if self.panel_dialog.title != panel.title || self.panel_dialog.bounds.is_none() {
            let preferred = match self.panel_dialog.key {
                Some("presentation_properties_panel") | Some("sheet_doc_properties_panel") => 520. * 0.7,
                Some("font_panel") => 520. * 0.6,
                Some("docx_compress_panel") | Some("compress_panel") => 520. * 0.55,
                Some("fit_text_panel") => 320.,
                Some("ruby_panel") => 520. * 0.4,
                Some("style_panel") | Some("doc_properties_panel") => 520. * 0.8,
                Some("tab_stop_editor") => 320.,
                Some("data_table_panel") => 364.,
                Some("conditional_panel") => 650.,
                Some("chart_series_editor") => if self.sheet_range_pick_active() {320.} else {520.},
                Some("merge_recipients") | Some("merge_import") => 820.,
                Some("scenario_manager_panel") => 760.,
                Some("paragraph_panel") => 520. * 0.8,
                Some("tab_panel") => 520. * 0.7,
                _ => 520.,
            };
            let width = px(preferred).min(viewport.width).max(px(320.));
            self.panel_dialog.user_resized = false;
            self.panel_dialog.user_moved = false;
            self.panel_dialog.content_measured = false;
            self.panel_dialog.focus_initialized = false;
            self.panel_dialog.footer_height = if footer.is_some() { px(super::chrome::BUTTON_HEIGHT + 2. * super::chrome::ACTION_BAR_PADDING_Y + 1.) } else { px(0.) };
            let height = px(560.).min((viewport.height - px(80.)).max(px(180.)));
            self.panel_dialog.bounds = Some(Bounds::new(
                point(if matches!(self.panel_dialog.key,Some("tab_stop_editor" | "chart_series_editor")) {viewport.width-width} else {(viewport.width - width) / 2.}, (viewport.height - height) / 2.), size(width, height)));
            self.panel_dialog.title = panel.title.clone();
            self.panel_dialog.scroll = ScrollHandle::new();
        }
        let focus = self.panel_dialog.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        self.panel_dialog.mode = mode;
        self.panel_dialog.active = true;
        let bounds = self.panel_dialog.bounds.unwrap();
        let scroll = self.panel_dialog.scroll.clone();
        let manual = self.panel_dialog.user_resized;
        let autocorrect_parent=match self.format.kind() {
            ibis_pptx::Kind::Document=>self.doc_state().and_then(|s|s.autocorrect.as_ref()),
            ibis_pptx::Kind::Workbook=>self.wb().panels.autocorrect.as_ref(),
            _=>self.pptx().and_then(|s|s.autocorrect.as_ref()),
        }.is_some_and(|p|!p.entry_open);
        let fill_height = !self.sheet_range_pick_active() && (matches!(self.panel_dialog.key,Some("field_panel"|"function_panel"|"dedupe_panel")) || (autocorrect_parent && matches!(self.panel_dialog.key,Some("doc_autocorrect_panel"|"autocorrect_panel"|"pptx_autocorrect_panel"))));
        let max_height = (viewport.height - px(80.)).max(px(100.));
        let max_body = (max_height - px(46.) - self.panel_dialog.footer_height).max(px(0.));
        let editor_measure = cx.entity().downgrade();
        let footer_measure = cx.entity().downgrade();
        // Track the actual content as a direct scroll child so its minimum width
        // contributes to horizontal overflow, while auto-width content still reflows.
        let body=form::Content{body:panel.body,editor:cx.weak_entity(),key:self.panel_dialog.key,options}.into_any_element();
        let body = if fill_height {super::chrome::panel_body(body).h_full().into_any_element()} else {body};
        v_flex().id("settings-dialog").occlude().absolute().left(bounds.origin.x).top(bounds.origin.y)
            .w(bounds.size.width).min_w(px(320.))
            .when(manual || fill_height, |d| d.h(if fill_height && !manual { max_height } else { bounds.size.height }).min_h(if fill_height {px(260.)} else {px(180.)}))
            .when(!manual && !fill_height, |d| d.h_auto().max_h(max_height))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .track_focus(&focus).key_context("PanelDialog")
            .on_action(cx.listener(|e,_:&SubmitEnter,w,cx|{if e.dialog_enter(false,false,w,cx){cx.stop_propagation();}else{cx.propagate();}}))
            .on_action(cx.listener(|e,_:&SubmitModifiedEnter,w,cx|{if e.dialog_enter(true,false,w,cx){cx.stop_propagation();}else{cx.propagate();}}))
            .bg(rgb(0xffffff)).border_1().border_color(rgb(BORDER)).rounded(px(8.)).shadow_lg()
            .child(h_flex().id("settings-dialog-title").relative().h(px(44.)).flex_none().px(px(12.))
                .items_center().justify_between().border_b_1().border_color(rgb(BORDER))
                .child(dialog_drag_surface(cx))
                .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT))
                    .child(super::chrome::panel::panel_title(&panel.title)))
                .child(div().id("settings-dialog-close").size(px(28.)).flex().items_center().justify_center().cursor_pointer()
                    .child(icon("close", 18., rgb(TEXT).into()))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, window, cx| this.close_panel_dialog(window, cx)))))
            .child(div().relative().min_h_0().w_full()
                .when(manual || fill_height, |d| d.flex_1())
                .child(div().id("settings-dialog-body").w_full()
                    .when(manual || fill_height, |d| d.h_full())
                    .when(!manual && !fill_height, |d| d.max_h(max_body))
                    .when(!fill_height, |d|d.p(px(super::chrome::FORM_PADDING)).overflow_scroll().track_scroll(&scroll))
                    .when(fill_height, |d|d.overflow_hidden()).child(body))
                .when(!fill_height, |d|d.child(super::chrome::always_visible_scrollbar_overlay("settings-dialog-scrollbar", scroll.clone(), None))
                    .child(super::chrome::horizontal_scrollbar_overlay("settings-dialog-horizontal-scrollbar", scroll))))
            .when_some(footer, |d, footer| d.child(super::chrome::action_bar_host(footer)
                .child(canvas(|_, _, _| (), move |measured, _, _, cx| {
                    let editor = footer_measure.clone();
                    cx.defer(move |cx| { let _ = editor.update(cx, |this, cx| {
                        if (this.panel_dialog.footer_height - measured.size.height).abs() > px(0.5) {
                            this.panel_dialog.footer_height = measured.size.height; cx.notify();
                        }
                    }); });
                }).absolute().inset_0())))
            .child(canvas(|_, _, _| (), move |measured, _, _, cx| {
                let editor = editor_measure.clone();
                cx.defer(move |cx| { let _ = editor.update(cx, |this, cx| {
                    let Some(mut actual) = this.panel_dialog.bounds else { return; };
                    if !this.panel_dialog.active { return; }
                    // The absolute measurement canvas covers the padding box; include both 1px borders.
                    let outer_height = measured.size.height + px(2.);
                    let first_measurement = !this.panel_dialog.content_measured;
                    this.panel_dialog.content_measured = true;
                    if first_measurement || (actual.size.height - outer_height).abs() > px(0.5) {
                        actual.size.height = outer_height;
                        if !this.panel_dialog.user_resized &&
                            ((first_measurement && !this.panel_dialog.user_moved) || actual.bottom() > viewport.height) {
                            actual.origin.y = (viewport.height - actual.size.height) / 2.;
                        }
                        this.panel_dialog.bounds = Some(actual); cx.notify();
                    }
                }); });
            }).absolute().inset_0())
            .children(resize::handles(cx))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.panel_dialog.drag = None;
                cx.stop_propagation();
            }))
            .into_any_element()
    }
}

/// Cover the document's hitboxes, underneath its actual scrollbars. Wheel events pass through.
pub(crate) fn viewport_shield() -> Div {
    div().absolute().inset_0().block_mouse_except_scroll()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// Capture the title grab so fields cannot become hovered during a drag.
fn dialog_drag_surface(cx: &Context<SlideEditor>) -> impl IntoElement {
    let editor = cx.entity().downgrade();
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            let down = editor.clone();
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                    let _ = down.update(cx, |this, cx| {
                        if this.toolbar_search_open.is_some() || this.interaction_layer().is_confirmation() { return; }
                        if let Some(bounds)=this.panel_dialog.bounds {
                            if let Some(edge)=resize::edge_at(bounds,event.position) {
                                this.panel_dialog.drag=Some(DialogDrag::Resize{pointer:event.position,bounds,edge});
                                window.capture_pointer(hitbox.id);cx.notify();cx.stop_propagation();
                            }
                        }
                    });
                }
                if phase == DispatchPhase::Bubble && event.button == MouseButton::Left && hitbox.is_hovered(window) {
                    let _ = down.update(cx, |this, cx| {
                        if let Some(bounds) = this.panel_dialog.bounds {
                            this.panel_dialog.drag = Some(DialogDrag::Move { pointer: event.position, origin: bounds.origin });
                            window.capture_pointer(hitbox.id);
                            cx.notify();
                            cx.stop_propagation();
                        }
                    });
                }
            });
            let released = editor.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                    let _ = released.update(cx, |this, cx| {
                        if this.panel_dialog.drag.take().is_some() {
                            window.release_pointer(); cx.notify(); cx.stop_propagation();
                        }
                    });
                }
            });
            let moving = editor.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture {
                    let _ = moving.update(cx, |this, cx| {
                        if this.move_panel_dialog(event, window, cx) { cx.stop_propagation(); }
                    });
                }
            });
        },
    ).absolute().left_0().top_0().bottom_0().right(px(44.))
}

/// A blocking overlay must always occlude lower hitboxes and consume all pointer phases.
pub(crate) fn confirmation_surface(id: &'static str) -> Stateful<Div> {
    div().id(id).absolute().inset_0().occlude().flex().items_center().justify_center()
        .bg(gpui_kit::rgba(0x00000066))
        .on_mouse_down(MouseButton::Left,|_,_,cx|cx.stop_propagation())
        .on_mouse_down(MouseButton::Right,|_,_,cx|cx.stop_propagation())
        .on_mouse_up(MouseButton::Left,|_,_,cx|cx.stop_propagation())
        .on_scroll_wheel(|_,_,cx|cx.stop_propagation())
}
