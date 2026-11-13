// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The document's views (表示 > 閲覧モード, 印刷レイアウト, Web レイアウト, アウトライン, 下書き), the way its pages
//! move (縦 / 左右) and the 閲覧・レビュー・編集 modes (DOCX-VIEW-001..008, 021). The view mode and the interaction
//! mode are kept in the document's model (`FlowDocument::view_mode`, `interaction_mode`); what the window shows of
//! them is in the document's state (`State::read_mode`, `State::page_movement_side_to_side`) and in a field of the
//! window (`outline_open`). The window's split into two panes (表示 > 分割, DOCX-VIEW-019) and フォーカス
//! (DOCX-VIEW-006) are here too, their state in the document's (`State::split`…, `State::focus_mode`). The code was
//! in `shell/editor.rs`.

use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::ext::*;

#[ibis_office_macros::editor_ext(ViewMode)]
impl SlideEditor {
    /// Word: ページの左右移動モードかどうか (DOCX-VIEW-008).
    pub(crate) fn is_page_movement_side_to_side(&self) -> bool {
        self.doc_state().is_some_and(|s| s.page_movement_side_to_side)
    }

    /// Word: ページの移動方向（縦／左右）を設定する (DOCX-VIEW-008).
    pub(crate) fn set_page_movement_side_to_side(&mut self, side_to_side: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_page_movement_side_to_side() == side_to_side {
            return;
        }
        let visible = self.visible_page();
        let Some(state) = self.doc_state_mut() else { return };
        state.page_movement_side_to_side = side_to_side;
        state.split_scroll.set_offset(Point::default());
        self.workspace_scroll.set_offset(Point::default());
        self.scroll_to_page(visible, cx);
        cx.notify();
        window.refresh();
    }

    /// Word: ページの移動方向（縦／左右）を切り替える (DOCX-VIEW-008).
    #[allow(dead_code)]
    pub(crate) fn toggle_page_movement_side_to_side(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = !self.is_page_movement_side_to_side();
        self.set_page_movement_side_to_side(next, window, cx);
    }

    /// Word: 文書のビューモードを取得する (DOCX-VIEW-001..005).
    pub(crate) fn document_view_mode(&self) -> ibis_doc_model::DocumentViewMode {
        if let Some(flow) = self.presentation().flow.as_ref() {
            flow.view_mode
        } else if self.is_read_mode() {
            ibis_doc_model::DocumentViewMode::Reading
        } else {
            ibis_doc_model::DocumentViewMode::Print
        }
    }

    /// Word: 文書のビューモードを設定する (DOCX-VIEW-001..005).
    pub(crate) fn set_document_view_mode(&mut self, mode: ibis_doc_model::DocumentViewMode, window: &mut Window, cx: &mut Context<Self>) {
        self.keep_read_mode(mode == ibis_doc_model::DocumentViewMode::Reading);
        if mode == ibis_doc_model::DocumentViewMode::Outline {
            self.outline_open = true;
        }
        if self.is_document() {
            self.run_flow(vec![ibis_doc_model::FlowCommand::SetViewMode(mode)], cx);
        }
        cx.notify();
        window.refresh();
    }

    /// Word: 閲覧モードかどうか (DOCX-VIEW-005).
    pub(crate) fn is_read_mode(&self) -> bool {
        self.doc_state().is_some_and(|s| s.read_mode)
    }

    /// The window is, or is not, in 閲覧モード from now (the model's view mode is the caller's to change).
    fn keep_read_mode(&mut self, read: bool) {
        if let Some(state) = self.doc_state_mut() {
            state.read_mode = read;
        }
    }

    /// Word: 閲覧モードを設定する (DOCX-VIEW-005).
    pub(crate) fn set_read_mode(&mut self, read: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mode = if read { ibis_doc_model::DocumentViewMode::Reading } else { ibis_doc_model::DocumentViewMode::Print };
        self.set_document_view_mode(mode, window, cx);
        if !read && self.flow().is_some_and(|d| d.interaction_mode == ibis_doc_model::DocumentInteractionMode::Viewing) {
            self.run_flow(vec![ibis_doc_model::FlowCommand::SetInteractionMode(ibis_doc_model::DocumentInteractionMode::Editing)], cx);
        }
    }

    /// Word: 閲覧モードを切り替える (DOCX-VIEW-005).
    #[allow(dead_code)]
    pub(crate) fn toggle_read_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = !self.is_read_mode();
        self.set_read_mode(next, window, cx);
    }

    /// Word: 閲覧・レビュー・編集モードを取得する (DOCX-VIEW-021).
    pub(crate) fn interaction_mode(&self) -> ibis_doc_model::DocumentInteractionMode {
        if let Some(doc) = self.flow() {
            if self.is_read_only() || self.is_read_mode() || doc.interaction_mode == ibis_doc_model::DocumentInteractionMode::Viewing || self.is_protection_read_only() {
                ibis_doc_model::DocumentInteractionMode::Viewing
            } else if doc.interaction_mode == ibis_doc_model::DocumentInteractionMode::Reviewing || doc.track_changes {
                ibis_doc_model::DocumentInteractionMode::Reviewing
            } else {
                ibis_doc_model::DocumentInteractionMode::Editing
            }
        } else {
            ibis_doc_model::DocumentInteractionMode::Editing
        }
    }

    /// Word: 編集モードかどうか (DOCX-VIEW-021).
    pub(crate) fn is_editing_mode(&self) -> bool {
        self.interaction_mode() == ibis_doc_model::DocumentInteractionMode::Editing
    }

    /// Word: レビューモードかどうか (DOCX-VIEW-021).
    pub(crate) fn is_reviewing_mode(&self) -> bool {
        self.interaction_mode() == ibis_doc_model::DocumentInteractionMode::Reviewing
    }

    /// Word: 閲覧モードかどうか (DOCX-VIEW-021).
    pub fn is_viewing_mode(&self) -> bool {
        self.interaction_mode() == ibis_doc_model::DocumentInteractionMode::Viewing
    }

    /// Word: 閲覧・レビュー・編集モードを設定する (DOCX-VIEW-021).
    pub(crate) fn set_interaction_mode(&mut self, mode: ibis_doc_model::DocumentInteractionMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.interaction_mode() == mode {
            return;
        }
        if mode == ibis_doc_model::DocumentInteractionMode::Editing && self.is_track_changes_locked() {
            self.status = Some(crate::shell::i18n::text("docx.interaction.locked-warning").into());
            cx.notify();
            return;
        }
        if mode != ibis_doc_model::DocumentInteractionMode::Viewing && self.is_read_mode() {
            self.keep_read_mode(false);
        }
        if self.is_document() {
            self.run_flow(vec![ibis_doc_model::FlowCommand::SetInteractionMode(mode)], cx);
        }
        cx.notify();
        window.refresh();
    }

    /// Word: 印刷レイアウト表示かどうか (DOCX-VIEW-001).
    pub(crate) fn is_print_layout_mode(&self) -> bool {
        self.document_view_mode() == ibis_doc_model::DocumentViewMode::Print
    }

    /// Word: Web レイアウト表示かどうか (DOCX-VIEW-002).
    pub(crate) fn is_web_layout_mode(&self) -> bool {
        self.document_view_mode() == ibis_doc_model::DocumentViewMode::Web
    }

    /// Word: 下書き表示かどうか (DOCX-VIEW-003).
    pub(crate) fn is_draft_mode(&self) -> bool {
        self.document_view_mode() == ibis_doc_model::DocumentViewMode::Draft
    }

    /// Word: アウトライン表示かどうか (DOCX-VIEW-004).
    pub(crate) fn is_outline_mode(&self) -> bool {
        self.document_view_mode() == ibis_doc_model::DocumentViewMode::Outline
    }

    /// The view mode the file keeps has changed under the window (an undo, a redo, another window of the same
    /// document): the window's reading mode and outline follow it.
    pub(crate) fn sync_document_view_mode(&mut self) {
        if self.is_document() {
            if let Some(view_mode) = self.flow().map(|f| f.view_mode) {
                self.keep_read_mode(view_mode == ibis_doc_model::DocumentViewMode::Reading);
                if view_mode == ibis_doc_model::DocumentViewMode::Outline && !self.outline_open {
                    self.outline_open = true;
                }
            }
        }
    }

    /// Word: ウィンドウの分割状態 (DOCX-VIEW-019).
    pub(crate) fn is_document_split(&self) -> bool {
        self.doc_split().is_some()
    }

    /// The split's ratio: the part of the workspace's height the top pane takes (DOCX-VIEW-019).
    pub(crate) fn doc_split(&self) -> Option<f32> {
        self.doc_state().and_then(|s| s.split)
    }

    /// The top pane was the one pressed last (DOCX-VIEW-019).
    pub(crate) fn doc_split_active(&self) -> bool {
        self.doc_state().is_some_and(|s| s.split_active)
    }

    pub(crate) fn set_doc_split_active(&mut self, top: bool) {
        if let Some(state) = self.doc_state_mut() {
            state.split_active = top;
        }
    }

    /// The scrolling of the pane the keys and the wheel move: the top pane's while the window is split and that pane
    /// was the one pressed last, the workspace's otherwise. (The handle is shared, so the copy scrolls the pane.)
    pub(crate) fn doc_pane_scroll(&self) -> ScrollHandle {
        match self.doc_state() {
            Some(state) if state.split_active && state.split.is_some() => state.split_scroll.clone(),
            _ => self.workspace_scroll.clone(),
        }
    }

    /// Word: ウィンドウの分割を切り替える (表示 > ウィンドウ > 分割 / 分割の解除, ⌥⌘S, DOCX-VIEW-019).
    pub(crate) fn toggle_document_split(&mut self, cx: &mut Context<Self>) {
        let offset = self.workspace_scroll.offset();
        if let Some(state) = self.doc_state_mut() {
            if state.split.is_some() {
                state.split = None;
                state.split_dragging = false;
                state.split_active = false;
            } else {
                state.split = Some(0.5);
                state.split_scroll.set_offset(offset);
                state.split_active = false;
            }
        }
        cx.notify();
    }

    /// Word: ウィンドウの分割比率を設定する (DOCX-VIEW-019).
    pub(crate) fn set_document_split(&mut self, split: Option<f32>, cx: &mut Context<Self>) {
        let offset = self.workspace_scroll.offset();
        if let Some(state) = self.doc_state_mut() {
            state.split = split;
            if split.is_some() {
                state.split_scroll.set_offset(offset);
            } else {
                state.split_dragging = false;
                state.split_active = false;
            }
        }
        cx.notify();
    }

    /// Word: フォーカス表示モードかどうか (DOCX-VIEW-006).
    pub(crate) fn is_focus_mode(&self) -> bool {
        self.doc_state().is_some_and(|s| s.focus_mode)
    }

    /// Word: フォーカス表示モードを設定する (DOCX-VIEW-006).
    pub(crate) fn set_focus_mode(&mut self, focus: bool, _window: &mut Window, cx: &mut Context<Self>) {
        if self.is_focus_mode() == focus {
            return;
        }
        if let Some(state) = self.doc_state_mut() {
            state.focus_mode = focus;
        }
        cx.notify();
    }

    /// Word: フォーカス表示モードを切り替える (DOCX-VIEW-006).
    pub(crate) fn toggle_focus_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let next = !self.is_focus_mode();
        self.set_focus_mode(next, window, cx);
    }
}
