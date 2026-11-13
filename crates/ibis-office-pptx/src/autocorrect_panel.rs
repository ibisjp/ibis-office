// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ツール > オートコレクトのオプション… (PPTX-FILE-014): the presentation's side of the AutoCorrect that
//! `shell::autocorrect` holds (the settings, the corrections and the panel), with the switches for
//! correcting and for marking misspelled words as text is typed.

use gpui_kit::*;

use crate::shell::autocorrect::{AutoCorrectPanel, PanelAccess, render_panel, set_settings};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::ext::*;

#[ibis_office_macros::editor_ext(AutocorrectPanel)]
impl SlideEditor {
    pub fn open_pptx_autocorrect_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let panel = AutoCorrectPanel::new(window, cx);
        if let Some(s) = self.pptx_mut() {
            s.ink = None;
            self.shape_edit.autocorrect = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn apply_pptx_autocorrect_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.shape_edit.autocorrect.take() {
            set_settings(p.working);
        }
        cx.notify();
    }

    pub fn pptx_autocorrect_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.shape_edit.autocorrect.as_ref()?;
        render_panel(
            self,
            PanelAccess {
                get: |e| e.shape_edit.autocorrect.as_mut(),
                apply: |e, cx| e.apply_pptx_autocorrect_panel(cx),
            },
            cx,
        )
    }
}
