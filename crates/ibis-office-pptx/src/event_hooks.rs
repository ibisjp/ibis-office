// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What the presentation does with the window's events (`FormatEditor`'s event hooks): the code was in `shell/editor.rs`.

use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::ext::*;

#[ibis_office_macros::editor_ext(EventHooks)]
impl SlideEditor {
    /// Escape, before the window's dialogs: the slide show in progress takes it.
    pub(crate) fn pptx_cancel_tool_early(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(show) = super::slideshow::Slideshow::of_mut(&mut *self.format) {
            if let Some(rehearsal) = show.rehearsal.as_mut() {
                if rehearsal.dialog_open {
                    { self.discard_rehearsal_timings(window, cx); return true; }
                } else {
                    let elapsed_ms = (rehearsal.slide_accumulated + rehearsal.slide_started.elapsed()).as_millis().max(1000) as u32;
                    rehearsal.recorded_timings.insert(self.current_slide, elapsed_ms);
                    rehearsal.dialog_open = true;
                    let total = rehearsal.total_accumulated + rehearsal.total_started.elapsed();
                    rehearsal.final_total = Some(total);
                    cx.notify();
                    return true;
                }
            }
            if show.grid_open {
                show.grid_open = false;
                cx.notify();
                return true;
            }
            if show.zoom > 1.001 || show.pan != (0.0, 0.0) {
                self.reset_slideshow_zoom(cx);
                return true;
            }
            if show.has_inks() {
                if show.ink_dialog_open {
                    { self.end_slideshow_with_ink_choice(false, window, cx); return true; }
                } else {
                    show.ink_dialog_open = true;
                    cx.notify();
                    return true;
                }
            }
            { self.end_slideshow(window, cx); return true; }
        }
        false
    }

    /// A key before the shape text's keys (`on_text_key`): while a path is drawn, Enter ends it and Backspace or Delete
    /// takes its last point back; while a line of the outline is edited, the keys are the line's. `true`: the key is not
    /// the shape text's.
    pub(crate) fn pptx_text_key_early(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.is_drawing_path() {
            match event.keystroke.key.as_str() {
                "enter" => {
                    cx.stop_propagation();
                    self.finish_path_draw(cx);
                }
                "backspace" | "delete" => {
                    cx.stop_propagation();
                    self.path_draw_undo_point(cx);
                }
                _ => {}
            }
            return true;
        }
        // The outline view's line being edited has the keys.
        if self.is_outline_editing() {
            if self.outline_key(event.keystroke.key.as_str(), &event.keystroke.modifiers, window, cx) {
                cx.stop_propagation();
            }
            return true;
        }
        false
    }

    /// Escape, after the window's dialogs: the presentation's zoom dialogs, a drag in progress, grid view and notes page view.
    pub(crate) fn pptx_cancel_tool_late(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.pptx().is_some_and(|p| p.slide_zoom_dialog.is_some()) {
            self.close_slide_zoom_dialog(cx);
            return true;
        }
        if self.pptx().is_some_and(|p| p.section_zoom_dialog.is_some()) {
            self.close_section_zoom_dialog(cx);
            return true;
        }
        if self.pptx().is_some_and(|p| p.summary_zoom_dialog.is_some()) {
            self.close_summary_zoom_dialog(cx);
            return true;
        }
        // A drag in progress is dropped (a rubber band, a text selection, a pane's edge, a path being drawn; then the shell's).
        if self.cancel_pptx_drag(cx) || self.cancel_pointer_drag(cx) {
            return true;
        }
        if self.is_grid_view() {
            self.close_grid_view(cx);
            return true;
        }
        if self.is_notes_page_view() {
            self.switch_to_normal_view(window, cx);
            return true;
        }
        false
    }

    /// The font size (points) chosen in the toolbar, for the characters selected.
    pub(crate) fn pptx_set_font_size(&mut self, pt: f32, window: &mut Window, cx: &mut Context<Self>) {
        let hundredths = (pt * 100.0).round().clamp(100.0, 400_000.0) as u32;
        self.format_characters(
            window,
            cx,
            ibis_doc_model::CharacterFormat {
                font_size: Some(hundredths),
                ..Default::default()
            },
        );
    }
}
