// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! How the open windows are arranged (ウィンドウ > 整列): named by the window switcher, the menus and the workbook's panel, carried out by shell/arrange_windows.rs.

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ArrangeKind {
    #[default]
    Tiled,
    Horizontal,
    Vertical,
    Cascade,
}
