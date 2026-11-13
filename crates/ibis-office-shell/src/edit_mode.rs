// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The mode of the cell being edited (入力・編集・参照の指定): the status bar shows it, the workbook's cell editing sets it.

/// How the arrows act while typing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditMode {
    /// Typing replaced the content: the arrows finish the entry and move.
    Enter,
    /// F2 or a double-click: ← and → move the caret.
    Edit,
}
