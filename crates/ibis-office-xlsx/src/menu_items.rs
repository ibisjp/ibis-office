// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A workbook's own lines of the menus.
//!
//! The shell's table of the commands (`shell/chrome_defaults.rs`) has the lines every screen shares and asks the screen
//! for its own at the places of [`MenuPart`] (`FormatEditor::menu_items`). The lines without a handler are placeholders:
//! the screen's `menus()` connects them (`Menus::connect`).

use crate::shell::chrome::{MenuEntry, MenuId, MenuPart};
use crate::shell::chrome_defaults::{SEP, item, sub};
use crate::shell::editor::{RulerUnit, SlideEditor};

/// `FormatEditor::menu_items`.
pub(crate) fn menu_items(editor: &SlideEditor, part: MenuPart) -> Vec<MenuEntry> {
    match part {
        MenuPart::EditEnd => edit_end(),
        MenuPart::ViewStart => view_start(editor),
        MenuPart::WindowEnd => window_end(editor),
        MenuPart::InsertStart => insert_start(),
        MenuPart::FormatEnd => format_end(),
        MenuPart::Own(MenuId::Data) => data_menu(),
        _ => Vec::new(),
    }
}

/// 編集: deleting rows, columns and cells, and the sheet's operations.
fn edit_end() -> Vec<MenuEntry> {
    vec![
        SEP,
        sub("xlsx.delete-menu", "delete", vec![
            item("xlsx.delete-row", "delete").into(),
            item("xlsx.delete-column", "delete").into(),
            item("xlsx.delete-cells", "delete").into(),
        ]),
        SEP,
        sub("xlsx.sheet-menu", "table_chart", vec![
            // 新しいシート is a sheet operation, so it is the sheet submenu's first line (Excel's 挿入 button of the Home tab
            // has シートの挿入 beside セルの挿入 only because that button is one for everything that is inserted).
            item("xlsx.insert-sheet", "note_add").shortcut("⇧F11").into(),
            SEP,
            item("xlsx.rename-sheet", "edit").into(),
            item("xlsx.duplicate-sheet", "content_copy").into(),
            item("xlsx.hide-sheet", "visibility_off").into(),
            item("xlsx.delete-sheet", "delete").into(),
        ]),
    ]
}

/// 表示: freezing the panes, the ruler, what the sheet shows and its views.
fn view_start(editor: &SlideEditor) -> Vec<MenuEntry> {
    vec![
        // The workbook's `menus()` (`xlsx/toolbar.rs`) connects the lines of freezing the panes, the zoom to the selection
        // and the custom views, and tells whether the zoom to the selection can be used.
        sub("xlsx.freeze", "view_column", vec![
            item("xlsx.freeze", "").into(),
            item("xlsx.freeze-panes", "").into(),
            item("xlsx.freeze-at-selection", "").into(),
            item("xlsx.freeze-top-row", "").into(),
            item("xlsx.freeze-first-column", "").into(),
            item("xlsx.unfreeze-panes", "").into(),
        ]).into(),
        item("xlsx.ruler", "rule").into(),
        sub("xlsx.ruler-unit", "straighten", vec![
            item("xlsx.ruler-unit.cm", "")
                .checked(editor.ruler_unit == RulerUnit::Centimeters)
                .disabled(!editor.current_sheet().is_some_and(|(_, s)| s.page_layout && s.show_ruler))
                .on(|this, _, cx| {
                    this.set_ruler_unit(RulerUnit::Centimeters, cx);
                })
                .into(),
            item("xlsx.ruler-unit.mm", "")
                .checked(editor.ruler_unit == RulerUnit::Millimeters)
                .disabled(!editor.current_sheet().is_some_and(|(_, s)| s.page_layout && s.show_ruler))
                .on(|this, _, cx| {
                    this.set_ruler_unit(RulerUnit::Millimeters, cx);
                })
                .into(),
            item("xlsx.ruler-unit.inch", "")
                .checked(editor.ruler_unit == RulerUnit::Inches)
                .disabled(!editor.current_sheet().is_some_and(|(_, s)| s.page_layout && s.show_ruler))
                .on(|this, _, cx| {
                    this.set_ruler_unit(RulerUnit::Inches, cx);
                })
                .into(),
            item("xlsx.ruler-unit.point", "")
                .checked(editor.ruler_unit == RulerUnit::Points)
                .disabled(!editor.current_sheet().is_some_and(|(_, s)| s.page_layout && s.show_ruler))
                .on(|this, _, cx| {
                    this.set_ruler_unit(RulerUnit::Points, cx);
                })
                .into(),
            item("xlsx.ruler-unit.pica", "")
                .checked(editor.ruler_unit == RulerUnit::Picas)
                .disabled(!editor.current_sheet().is_some_and(|(_, s)| s.page_layout && s.show_ruler))
                .on(|this, _, cx| {
                    this.set_ruler_unit(RulerUnit::Picas, cx);
                })
                .into(),
        ]),
        item("xlsx.gridlines", "grid_on").into(),
        item("xlsx.headings", "view_headline").into(),
        item("xlsx.formula-bar", "functions").into(),
        item("xlsx.show-formulas", "functions")
            .shortcut("⌘`")
            .into(),
        item("xlsx.show-zeros", "exposure_zero").into(),
        sub("xlsx.view-mode", "grid_view", vec![
            item("xlsx.normal-view", "grid_view").into(),
            item("xlsx.page-break-preview", "insert_page_break").into(),
            item("xlsx.page-layout", "description").into(),
        ]),
        item("xlsx.zoom-selection", "aspect_ratio").into(),
        item("xlsx.custom-views", "view_quilt").disabled(editor.presentation().has_tables()).into(),
        SEP,
    ]
}

/// ウィンドウ: the workbook's windows.
fn window_end(editor: &SlideEditor) -> Vec<MenuEntry> {
    vec![
        SEP,
        item("xlsx.new-window", "open_in_new")
            .shortcut("⌘⇧N")
            .on(|this, _, cx| {
                this.new_window(cx);
            })
            .into(),
        // The workbook's `menus()` (`xlsx/toolbar.rs`) connects the panel of arranging the windows, hiding the window and
        // the panel of showing a hidden window.
        item("xlsx.arrange-all", "dashboard").into(),
        item("xlsx.view-side-by-side", "splitscreen")
            .checked(editor.side_by_side)
            .on(|this, window, cx| {
                this.toggle_view_side_by_side(window, cx);
            })
            .into(),
        item("xlsx.sync-scrolling", "sync")
            .checked(editor.sync_scrolling)
            .disabled(!editor.side_by_side)
            .on(|this, _window, cx| {
                this.toggle_sync_scrolling(cx);
            })
            .into(),
        item("xlsx.reset-window-position", "restart_alt")
            .disabled(!editor.side_by_side)
            .on(|this, window, cx| {
                this.reset_side_by_side_positions(window, cx);
            })
            .into(),
        item("xlsx.hide-window", "visibility_off")
            .disabled(editor.presentation().is_workbook_window_hidden() || editor.presentation().is_workbook_windows_locked())
            .into(),
        item("xlsx.unhide-window", "visibility")
            .disabled(!editor.presentation().is_workbook_window_hidden() || editor.presentation().is_workbook_windows_locked())
            .into(),
    ]
}

/// 挿入: rows, columns and cells, and a table.
fn insert_start() -> Vec<MenuEntry> {
    vec![
        sub("xlsx.insert-cells-menu", "add", vec![
            item("xlsx.insert-row-above", "add").into(),
            item("xlsx.insert-row-below", "add").into(),
            item("xlsx.insert-column-left", "add").into(),
            item("xlsx.insert-column-right", "add").into(),
            item("xlsx.insert-cells", "add").into(),
        ]),
        SEP,
        item("xlsx.insert-table", "table").shortcut("⌘T").into(),
        SEP,
    ]
}

/// 書式: a chart's lines.
fn format_end() -> Vec<MenuEntry> {
    vec![
        SEP,
        item("xlsx.chart-format", "bar_chart").into(),
        item("xlsx.move-chart", "drive_file_move").into(),
        item("xlsx.chart.save-template", "save").into(),
        item("xlsx.chart.apply-template", "style").into(),
    ]
}

/// データ.
fn data_menu() -> Vec<MenuEntry> {
    vec![
        item("xlsx.sort", "sort").into(),
        item("xlsx.filter", "filter_alt").into(),
        item("xlsx.table-to-range", "table").into(),
        SEP,
        item("xlsx.protect", "lock").into(),
        item("xlsx.named-ranges", "label").into(),
        item("xlsx.validation", "rule").into(),
        item("xlsx.validation.circle-invalid", "radio_button_unchecked").into(),
        item("xlsx.validation.clear-circles", "clear").into(),
        item("xlsx.split-text", "view_column").into(),
    ]
}
