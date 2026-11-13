// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What one window shows of tracked changes and hidden text of a Word document. It is held by `Resources` (so it is here, in the
//! drawing crate); the document's layout (`docx/flow_layout.rs`) reads it.

use std::sync::Arc;

/// Tracked changes display mode (Word's Display for Review: Simple, All, None, Original).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum MarkupDisplayMode {
    #[default]
    All = 0,        // すべての変更履歴 (All Markup)
    Simple = 1,     // シンプルな変更履歴 (Simple Markup)
    None = 2,       // 変更履歴なし (No Markup / 最終版)
    Original = 3,   // 元の文書 (Original / 原文)
}

/// What one window shows of tracked changes and hidden text: the markup mode, whether hidden text is shown, and the reviewers
/// whose changes are shown (`None`: all). It belongs to the window's own `Resources` (`Resources::markup_view`), not to the
/// process: two documents open at once each keep their own. It is part of the layout caches' keys, so a change of it lays
/// the pages out again by itself.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct MarkupView {
    pub mode: MarkupDisplayMode,
    pub show_hidden_text: bool,
    pub shown_reviewers: Option<Arc<std::collections::BTreeSet<String>>>,
}

impl MarkupView {
    /// Whether tracked changes are drawn (すべての変更履歴 or シンプルな変更履歴).
    pub fn showing_markup(&self) -> bool {
        matches!(self.mode, MarkupDisplayMode::All | MarkupDisplayMode::Simple)
    }

    pub fn is_reviewer_shown(&self, author: &str) -> bool {
        self.shown_reviewers.as_ref().map_or(true, |set| set.contains(author))
    }
}

