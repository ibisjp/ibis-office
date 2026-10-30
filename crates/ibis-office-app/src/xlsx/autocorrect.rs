// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ツール > オートコレクトのオプション… (XLSX-OPTIONS-005): the workbook's side of the AutoCorrect that
//! `shell::autocorrect` holds (the settings, the corrections and the panel).

use gpui_kit::*;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
#[cfg(feature = "scenarios")]
pub(crate) use crate::shell::autocorrect::AutoCorrect;
pub(crate) use crate::shell::autocorrect::{AutoCorrectPanel, correct_typed};
use crate::shell::autocorrect::{PanelAccess, render_panel, set_settings};

impl SlideEditor {
    pub(crate) fn open_autocorrect_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let panel = AutoCorrectPanel::new(window, cx);
        let state = self.wb_mut();
        state.close_settings();
        state.panels.autocorrect = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn apply_autocorrect_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.autocorrect.take() {
            set_settings(p.working);
        }
        cx.notify();
    }

    pub(crate) fn autocorrect_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.sheet_state()?.panels.autocorrect.as_ref()?;
        render_panel(
            self,
            PanelAccess { get: |e| e.wb_mut().panels.autocorrect.as_mut(), apply: |e, cx| e.apply_autocorrect_panel(cx), close: |e| e.wb_mut().panels.autocorrect = None },
            cx,
        )
    }
}
