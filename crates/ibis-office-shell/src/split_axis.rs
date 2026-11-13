// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Which way a split view is split: the window (workspace) draws the split bars of a worksheet or a document, the workbook and the
//! document decide where they are.

use gpui_kit::actions;

actions!(
    docx,
    [
        /// ウィンドウの分割の切り替え (⌥⌘S / Alt+Ctrl+S DOCX-VIEW-019).
        ToggleSplit,
    ]
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitAxis {
    Horizontal,
    Vertical,
    Both,
    ScrollHLeft,
    ScrollHRight,
    ScrollVTop,
    ScrollVBottom,
}
