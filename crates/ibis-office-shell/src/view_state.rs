// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The view of a window: where it is scrolled, how far it is zoomed and what is selected, taken before a
//! file is read again (読み込み直す after another app changed it, `shell/file_watch.rs`) and put back after.
//!
//! What is kept:
//! - every screen: the scroll of the main content (x, y) and the zoom (a fit-to-window zoom stays a fit);
//! - what only one screen has, kept and put back by that screen (`FormatEditor::view_kept`, `view_put_back`):
//!   the workbook's shown sheet and the selected cell and range of every sheet (`xlsx/view_state.rs`), the
//!   document's caret and the other end of the selection (`docx/view_state.rs`), the presentation's shown slide
//!   and selected shapes (`pptx/view_state.rs`).
//!
//! Scrolling is put back as far as the new content allows (`clamp_workspace_scroll`).

use std::any::Any;

use gpui_kit::*;

use crate::editor::SlideEditor;
use crate::format::with_format;

/// A window's view.
pub struct ViewState {
    pub scroll: Point<Pixels>,
    pub zoom: Option<f32>,
    /// What only the window's screen has, as that screen kept it (`FormatEditor::view_kept`).
    pub format: Option<Box<dyn Any>>,
}

impl SlideEditor {
    /// The view now.
    pub(crate) fn save_view_state(&self) -> ViewState {
        ViewState { scroll: self.workspace_scroll.offset(), zoom: self.zoom, format: with_format!(self, view_kept(self)) }
    }

    /// Put `view` back on what was read (the sheet, slide, selection, then the zoom and the scroll).
    pub(crate) fn restore_view_state(&mut self, view: &ViewState, window: &mut Window, cx: &mut Context<Self>) {
        // The screen's own part: a screen leaves a value that is not its own (kept by another kind of screen).
        if let Some(kept) = view.format.as_deref() {
            with_format!(self, view_put_back(self, kept, window, cx));
        }
        // A worksheet opens at its own zoom (`show_page`): the zoom the window had is put back over it.
        self.zoom = view.zoom;
        self.workspace_scroll.set_offset(view.scroll);
        self.clamp_workspace_scroll();
        cx.notify();
    }
}
