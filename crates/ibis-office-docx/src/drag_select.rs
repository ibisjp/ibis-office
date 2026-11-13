// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! DOCX-OPTIONS-005: ドラッグ編集と選択方法（Drag-and-Drop Editing & Selection Options）
//!
//! Word 標準の「編集オプション」（Word Options > 詳細設定 > 編集オプション）をクリーンルーム実装。
//! 1. 「ドラッグ アンド ドロップによるテキスト編集を可能にする」（Allow text to be dragged and dropped: 既定 true）
//! 2. 「選択時に単語全体を自動的に選択する」（When selecting, automatically select entire word: 既定 true）
//!
//! 編集メニュー「編集 > 選択とドラッグ編集」（`docx.edit.drag-select`）での動的切り替えに対応。

use gpui_kit::*;
use ibis_doc_model::FlowPos;

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// ドラッグ編集と選択方法の設定状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DragSelectOptionsState {
    /// ドラッグ アンド ドロップによるテキスト編集を可能にする（Word 既定値: true）
    pub(crate) allow_drag_and_drop: bool,
    /// 選択時に単語全体を自動的に選択する（Word 既定値: true）
    pub(crate) auto_select_entire_word: bool,
}

impl Default for DragSelectOptionsState {
    fn default() -> Self {
        Self {
            allow_drag_and_drop: true,
            auto_select_entire_word: true,
        }
    }
}

#[ibis_office_macros::editor_ext(DragSelect)]
impl SlideEditor {
    /// ドラッグ アンド ドロップ編集が有効かどうか
    pub(crate) fn is_drag_drop_text_editing_enabled(&self) -> bool {
        self.doc_state()
            .and_then(|s| s.drag_select.as_ref())
            .map_or(true, |st| st.allow_drag_and_drop)
    }

    /// ドラッグ アンド ドロップ編集の有効/無効を設定
    pub(crate) fn set_drag_drop_text_editing_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            if let Some(st) = s.drag_select.as_mut() {
                st.allow_drag_and_drop = enabled;
            } else {
                s.drag_select = Some(DragSelectOptionsState {
                    allow_drag_and_drop: enabled,
                    ..Default::default()
                });
            }
        }
        cx.notify();
    }

    /// ドラッグ アンド ドロップ編集の有効/無効をトグル
    pub(crate) fn toggle_drag_drop_text_editing(&mut self, cx: &mut Context<Self>) {
        let current = self.is_drag_drop_text_editing_enabled();
        self.set_drag_drop_text_editing_enabled(!current, cx);
    }

    /// 選択時に単語全体を自動選択するかどうか
    pub(crate) fn is_auto_select_entire_word_enabled(&self) -> bool {
        self.doc_state()
            .and_then(|s| s.drag_select.as_ref())
            .map_or(true, |st| st.auto_select_entire_word)
    }

    /// 単語全体の自動選択の有効/無効を設定
    pub(crate) fn set_auto_select_entire_word_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            if let Some(st) = s.drag_select.as_mut() {
                st.auto_select_entire_word = enabled;
            } else {
                s.drag_select = Some(DragSelectOptionsState {
                    auto_select_entire_word: enabled,
                    ..Default::default()
                });
            }
        }
        cx.notify();
    }

    /// 単語全体の自動選択の有効/無効をトグル
    pub(crate) fn toggle_auto_select_entire_word(&mut self, cx: &mut Context<Self>) {
        let current = self.is_auto_select_entire_word_enabled();
        self.set_auto_select_entire_word_enabled(!current, cx);
    }

    /// 選択範囲を単語境界にスナップ拡張する（auto_select_entire_word が有効な場合）
    pub(crate) fn snap_selection_to_word_boundaries(
        &self,
        start: FlowPos,
        end: FlowPos,
    ) -> (FlowPos, FlowPos) {
        if !self.is_auto_select_entire_word_enabled() {
            return (start, end);
        }

        let Some(p) = self.paragraph_of(start.para) else {
            return (start, end);
        };
        let text = p.text();
        if text.is_empty() {
            return (start, end);
        }

        // 単語境界判定関数
        let is_word_char = |c: char| -> bool {
            c.is_alphanumeric() || c == '_' || ('\u{4e00}'..='\u{9fff}').contains(&c) || ('\u{3040}'..='\u{309f}').contains(&c) || ('\u{30a0}'..='\u{30ff}').contains(&c)
        };

        let chars: Vec<(usize, char)> = text.char_indices().collect();
        if chars.is_empty() {
            return (start, end);
        }

        // 開始位置を単語の先頭へ拡張
        let mut new_start_offset = start.offset;
        if start.offset < text.len() {
            let start_char_idx = chars.iter().position(|&(idx, _)| idx >= start.offset).unwrap_or(chars.len() - 1);
            if is_word_char(chars[start_char_idx].1) {
                let mut left_idx = start_char_idx;
                while left_idx > 0 && is_word_char(chars[left_idx - 1].1) {
                    left_idx -= 1;
                }
                new_start_offset = chars[left_idx].0;
            }
        }

        // 終了位置を単語の末尾へ拡張
        let mut new_end_offset = end.offset;
        if end.offset > 0 && end.offset <= text.len() {
            let end_char_idx = chars.iter().rposition(|&(idx, _)| idx < end.offset).unwrap_or(0);
            if is_word_char(chars[end_char_idx].1) {
                let mut right_idx = end_char_idx;
                while right_idx + 1 < chars.len() && is_word_char(chars[right_idx + 1].1) {
                    right_idx += 1;
                }
                // 次の文字の開始オフセット、またはテキスト長
                new_end_offset = if right_idx + 1 < chars.len() {
                    chars[right_idx + 1].0
                } else {
                    text.len()
                };
            }
        }

        (
            FlowPos::new(start.para, new_start_offset),
            FlowPos::new(end.para, new_end_offset),
        )
    }
}

// ---------------------------------------------------------------------------
// ユニットテスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::DragSelectOptionsState;

    #[test]
    fn test_drag_select_options_default() {
        let state = DragSelectOptionsState::default();
        assert!(state.allow_drag_and_drop, "allow_drag_and_drop should be true by default");
        assert!(state.auto_select_entire_word, "auto_select_entire_word should be true by default");
    }
}
