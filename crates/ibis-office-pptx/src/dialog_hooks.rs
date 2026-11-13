// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The window's dialogs, as far as the presentation is concerned: what closing and discarding them does to its state, and the like.
//! These were in `shell/dialog.rs`; `FormatEditor`'s dialog hooks call them.


use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The dialogs' parts that are the presentation's (`FormatEditor`'s dialog hooks; the code came from `shell/dialog.rs`).
#[ibis_office_macros::editor_ext(DialogHooks)]
impl SlideEditor {
    /// The panel state of the dialog `key` is thrown away: `FormatEditor::discard_dialog`.
    pub(crate) fn discard_pptx_dialog(&mut self, key: &'static str) {
        let Some(state) = self.pptx_mut() else { return; };
        match key {
            "pptx_trust_center_panel" => { state.trust_center = None; }
            "audio_record_panel" => { state.audio_record = None; }
            "pptx_gradient_panel" => { self.shape_edit.gradient_panel = None; }
            "shadow_panel" => { self.shape_edit.shadow_panel = None; }
            "glow_panel" => { self.shape_edit.glow_panel = None; }
            "frame_panel" => { self.shape_edit.frame_panel = None; }
            "text_insets_panel" => { self.shape_edit.insets_panel = None; }
            "alt_text_panel" => { self.shape_edit.alt_text_panel = None; }
            "chart_panel" => { self.shape_edit.chart_panel = None; }
            "compare_panel" => { state.compare_panel = None; }
            "photo_album_panel" => { state.photo_album = None; }
            "pptx_autocorrect_panel" => { self.shape_edit.autocorrect = None; }
            "pptx_math_panel" => { state.math_panel = None; }
            "animation_options_panel" => { state.animation_options_panel = None; }
            "pptx_compatibility_panel" => { state.compatibility_panel = None; }
            "presentation_inspect_panel" => { state.inspect_panel = None; }
            "presentation_properties_panel" => { state.properties_panel = None; }
            "notes_marks_panel" => { state.notes_marks_panel = None; }
            "compress_panel" => { self.shape_edit.compress_panel = None; }
            "print_panel" => { state.print_panel = None; }
            "layout_name_panel" => { state.layout_panel = None; }
            "pptx_section_panel" => { state.section_panel = None; }
            "pptx_paragraph_panel" => { self.shape_edit.paragraph_panel = None; }
            "timing_panel" => { state.timing_panel = None; }
            "show_panel" => { state.show_panel = None; }
            "crop_panel" => { self.shape_edit.crop_panel = None; }
            "cell_size_panel" => { state.cell_size_panel = None; }
            "header_footer_panel" => { self.shape_edit.header_footer = None; }
            "slide_link_panel" => { state.link_panel = None; }
            "theme_panel" => { self.shape_edit.theme_panel = None; }
            "slide_size_panel" => { self.shape_edit.size_panel = None; }
            "pptx_find_panel" => { self.shape_edit.find = None; }
            _ => {}
        }
    }
}
