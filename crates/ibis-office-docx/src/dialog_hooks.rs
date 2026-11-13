// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The window's dialogs, as far as the document is concerned: what closing and discarding them does to its state, and the like.
//! These were in `shell/dialog.rs`; `FormatEditor`'s dialog hooks call them.

use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The dialogs' parts that are the document's (`FormatEditor`'s dialog hooks; the code came from `shell/dialog.rs`).
#[ibis_office_macros::editor_ext(DialogHooks)]
impl SlideEditor {
    /// Closing a dialog the document closes in its own way: true if it did.
    pub(crate) fn docx_close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.dialog.key.is_some_and(|key|key.starts_with("merge_")) {
            self.cancel_merge_workflow_dialog(window,cx);
            self.dialog.active=false;self.dialog.overlay=None;self.dialog.key=None;
            window.release_pointer();self.focus_handle.focus(window,cx);cx.notify();return true;
        }
        if self.dialog.key==Some("tab_stop_editor") {
            if let Some(p)=self.doc_state_mut().and_then(|s|s.tab_edit.as_mut()){p.editor=None;}
            self.dialog.active=false;self.dialog.overlay=None;self.dialog.key=None;
            window.release_pointer();self.focus_handle.focus(window,cx);cx.notify();return true;
        }
        false
    }

    /// The panel state of the dialog `key` is thrown away: `FormatEditor::discard_dialog`.
    pub(crate) fn discard_docx_dialog(&mut self, key: &'static str) {
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
            "tate_chu_yoko_panel" => { state.tate_edit = None; }
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
}
