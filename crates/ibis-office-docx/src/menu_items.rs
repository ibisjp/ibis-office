// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A document's own lines of the menus, and its button of the toolbar's left part.
//!
//! The shell's table of the commands (`shell/chrome_defaults.rs`) has the lines every screen shares and asks the screen
//! for its own at the places of [`MenuPart`] (`FormatEditor::menu_items`). The lines without a handler are placeholders:
//! the screen's `menus()` connects them (`Menus::connect`).

use crate::shell::chrome::{MenuEntry, MenuPart, ToolButton, ToolItem};
use crate::shell::chrome_defaults::{SEP, item, sub};
use crate::shell::editor::SlideEditor;

/// `FormatEditor::menu_items`.
pub(crate) fn menu_items(editor: &SlideEditor, part: MenuPart) -> Vec<MenuEntry> {
    match part {
        MenuPart::ViewStart => view_start(editor),
        MenuPart::WindowEnd => window_end(editor),
        MenuPart::InsertStart => insert_start(),
        MenuPart::InsertEnd => insert_end(),
        MenuPart::FormatEnd => format_end(),
        _ => Vec::new(),
    }
}

/// 表示: the modes, the views, what the page shows, how the pages move and the zooms of a page.
fn view_start(editor: &SlideEditor) -> Vec<MenuEntry> {
    vec![
        // The document's `menus()` connects these lines (`docx/toolbar.rs`) and gives their check marks
        // (`docx/view_menu.rs`).
        sub("docx.interaction", "edit_document", vec![
            item("docx.interaction.editing", "edit").into(),
            item("docx.interaction.reviewing", "rate_review").into(),
            item("docx.interaction.viewing", "visibility").into(),
        ]),
        SEP,
        item("docx.view.read-mode", "chrome_reader_mode").into(),
        item("docx.print-layout", "description").into(),
        item("docx.view.web-layout", "web").into(),
        item("docx.view.outline", "view_list").into(),
        item("docx.view.draft", "article").into(),
        item("docx.view.focus", "center_focus_strong").into(),
        SEP,
        item("docx.ruler", "rule").into(),
        // The ruler panel's 「グリッド線」 check box, as a line of the menu (the redesign's 表示 ▸ グリッド線).
        item("docx.gridlines", "grid_on").checked(editor.gridlines_open).on(|this, _, cx| {
            this.gridlines_open = !this.gridlines_open;
            cx.notify();
        })
        .into(),
        item("docx.formatting-marks", "notes").into(),
        item("docx.white-space", "vertical_distribute").into(),
        item("docx.navigation-pane", "toc").checked(editor.outline_open).on(|this, _, cx| {
            this.outline_open = !this.outline_open;
            cx.notify();
        })
        .into(),
        SEP,
        sub("docx.page-movement", "swap_horiz", vec![
            item("docx.page-movement.vertical", "swap_vert").into(),
            item("docx.page-movement.side-to-side", "swap_horiz").into(),
        ]),
        SEP,
        item("docx.view.one-page", "description").into(),
        item("docx.view.multiple-pages", "filter_none").into(),
        item("docx.view.page-width", "fit_screen").into(),
        item("docx.view.zoom-100", "zoom_in").shortcut("⌘0").into(),
        SEP,
    ]
}

/// ウィンドウ: the document's windows and its split.
fn window_end(editor: &SlideEditor) -> Vec<MenuEntry> {
    vec![
        SEP,
        item("docx.new-window", "open_in_new")
            .shortcut("⌘⇧N")
            .on(|this, _, cx| {
                this.new_window(cx);
            })
            .into(),
        item("docx.arrange-all", "grid_view")
            .on(|_, window, cx| {
                SlideEditor::arrange_all_windows(Some(window), cx);
            })
            .into(),
        // While the document is split, the document's `menus()` (`docx/view_menu.rs`) turns this line into
        // `docx.remove-split` with a check mark; `docx/toolbar.rs` connects both.
        item("docx.split", "splitscreen").shortcut("⌥⌘S").into(),
        item("docx.view-side-by-side", "splitscreen")
            .checked(editor.side_by_side)
            .on(|this, window, cx| {
                this.toggle_view_side_by_side(window, cx);
            })
            .into(),
        item("docx.sync-scrolling", "sync")
            .checked(editor.sync_scrolling)
            .disabled(!editor.side_by_side)
            .on(|this, _window, cx| {
                this.toggle_sync_scrolling(cx);
            })
            .into(),
        item("docx.reset-window-position", "restart_alt")
            .disabled(!editor.side_by_side)
            .on(|this, window, cx| {
                this.reset_side_by_side_positions(window, cx);
            })
            .into(),
    ]
}

/// 挿入: the breaks, the header and footer, the page number and the table of contents.
fn insert_start() -> Vec<MenuEntry> {
    vec![
        item("docx.page-break", "insert_page_break").shortcut("⌘↩").into(),
        item("docx.column-break", "vertical_split").shortcut("⌃⇧↩").into(),
        item("docx.header-footer", "view_quilt").into(),
        item("docx.page-number", "tag").into(),
        item("docx.toc", "toc").into(),
        SEP,
    ]
}

/// 挿入: the content controls and the form fields.
fn insert_end() -> Vec<MenuEntry> {
    vec![
        item("insert.rich-text", "text_fields").into(),
        item("insert.plain-text", "short_text").into(),
        item("insert.picture-control", "image").into(),
        item("insert.building-block-gallery", "dashboard_customize").into(),
        item("insert.repeating-section", "control_point_duplicate").into(),
        item("insert.group", "group_work").into(),
        item("insert.checkbox", "checklist").into(),
        item("insert.dropdown-list", "arrow_drop_down_circle").into(),
        item("insert.combo-box", "arrow_drop_down").into(),
        item("insert.date-picker", "schedule").into(),
        item("insert.form-text", "text_snippet").into(),
        item("insert.form-checkbox", "check_box").into(),
        item("insert.form-dropdown", "list").into(),
    ]
}

/// 書式: the content controls' and the form fields' lines.
fn format_end() -> Vec<MenuEntry> {
    vec![
        SEP,
        item("docx.form.checkbox.toggle", "check").into(),
        item("docx.form.dropdown.cycle", "autorenew").into(),
        item("docx.form.legacy-dropdown.cycle", "autorenew").into(),
        item("docx.form.date.step", "schedule").into(),
        item("docx.form.picture.replace", "image").into(),
        item("docx.form.picture.reset", "restart_alt").into(),
        item("docx.form.gallery.cycle", "autorenew").into(),
        item("docx.form.gallery.reset", "restart_alt").into(),
        item("docx.form.repeating.add", "add").into(),
        item("docx.form.repeating.delete", "delete").into(),
        item("docx.form.repeating.cycle", "autorenew").into(),
        item("docx.form.group", "group_work").into(),
        item("docx.form.ungroup", "workspaces").into(),
        item("docx.form.design-mode", "design_services").into(),
        item("docx.form.properties", "settings").into(),
    ]
}

/// `FormatEditor::toolbar_left`: スペル チェックと文章校正.
pub(crate) fn toolbar_left() -> Vec<ToolItem> {
    vec![ToolButton::new("spelling", "spellcheck", "_shell.spelling-and-grammar").into()]
}
