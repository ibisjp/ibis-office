// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The document's own lines of the View menu: the interaction modes (editing, reviewing, viewing), the view modes
//! (read mode, print layout, web layout, outline, draft, focus), the formatting marks, the white space between pages,
//! the page movement and the zooms (one page, multiple pages, page width, 100%); and the Window menu's split line.
//! `menu_items.rs` lists the lines (their place, icon and shortcut); whether they are checked or
//! disabled is here, and what they do is connected in `toolbar.rs` (`menus()`).

use crate::shell::chrome::{MenuEntry, MenuId, Menus};
use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// Mark the line `id` as checked or not (nothing if the menus do not have it).
fn check(menus: &mut Menus, id: &str, checked: bool) {
    if let Some(item) = menus.find_mut(id) {
        item.checked = checked;
    }
}

/// Give the state of the View menu's lines of a document, and of the Window menu's split line. Called first by the
/// document's `menus()`, while the menus hold only the shell's lines (so an id names one line).
pub(crate) fn connect(editor: &SlideEditor, menus: &mut Menus) {
    // A document has its own 100% (`docx.view.zoom-100`, among its lines of the View menu): the shared one of the zoom submenu
    // would be a second 「100%」 line. Taken out first, before anything else reads the menus.
    for (_, entries) in menus.menus.iter_mut().filter(|(id, _)| *id == MenuId::View) {
        for entry in entries {
            if let MenuEntry::Submenu(s) = entry {
                if s.id == "view.zoom" {
                    s.items.retain(|e| !matches!(e, MenuEntry::Item(i) if i.id == "view.zoom-100"));
                }
            }
        }
    }
    check(menus, "docx.interaction.editing", editor.is_editing_mode());
    if let Some(item) = menus.find_mut("docx.interaction.editing") {
        item.disabled = editor.is_track_changes_locked();
    }
    check(menus, "docx.interaction.reviewing", editor.is_reviewing_mode());
    check(menus, "docx.interaction.viewing", editor.is_viewing_mode());
    check(menus, "docx.view.read-mode", editor.is_read_mode());
    check(menus, "docx.print-layout", editor.is_print_layout_mode());
    check(menus, "docx.view.web-layout", editor.is_web_layout_mode());
    check(menus, "docx.view.outline", editor.is_outline_mode());
    check(menus, "docx.view.draft", editor.is_draft_mode());
    check(menus, "docx.view.focus", editor.is_focus_mode());
    check(menus, "docx.formatting-marks", editor.doc_prefs().formatting_marks_open);
    check(menus, "docx.white-space", editor.doc_prefs().show_white_space);
    let side_to_side = editor.is_page_movement_side_to_side();
    check(menus, "docx.page-movement.vertical", !side_to_side);
    check(menus, "docx.page-movement.side-to-side", side_to_side);
    check(menus, "docx.view.one-page", editor.is_zoom_one_page());
    check(menus, "docx.view.multiple-pages", editor.is_zoom_multiple_pages());
    check(menus, "docx.view.page-width", editor.is_zoom_page_width());
    check(menus, "docx.view.zoom-100", editor.is_zoom_100());
    // ウィンドウ > 分割: while the document is split, the line is 分割の解除 (`docx.remove-split`), checked.
    if editor.is_document_split()
        && let Some(item) = menus.find_mut("docx.split")
    {
        item.id = "docx.remove-split";
        item.label = crate::shell::i18n::text("docx.remove-split");
        item.checked = true;
    }
}
