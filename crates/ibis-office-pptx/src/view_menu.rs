// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The presentation's own lines of the View menu: the view switches (normal, outline, slide sorter, notes page, the
//! masters, reading view), the slide show and the guides. The shell lists the lines (`shell/chrome_defaults.rs`: their
//! place, icon and shortcut); what they do and whether they are checked is here.

use crate::shell::chrome::Menus;
use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// Mark the line `id` as checked or not (nothing if the menus do not have it).
fn check(menus: &mut Menus, id: &str, checked: bool) {
    if let Some(item) = menus.find_mut(id) {
        item.checked = checked;
    }
}

/// Connect the View menu's lines of a presentation. Called first by the presentation's `menus()`, while the menus hold
/// only the shell's lines (so an id names one line).
pub(crate) fn connect(editor: &SlideEditor, menus: &mut Menus) {
    check(menus, "pptx.normal-view", editor.is_normal_view());
    menus.connect("pptx.normal-view", |this, window, cx| this.switch_to_normal_view(window, cx));
    check(menus, "pptx.outline-view", editor.is_outline_view());
    menus.connect("pptx.outline-view", |this, window, cx| {
        if this.is_outline_view() {
            this.switch_to_normal_view(window, cx);
        } else {
            this.switch_to_outline_view(window, cx);
        }
    });
    check(menus, "pptx.grid-view", editor.is_grid_view());
    menus.connect("pptx.grid-view", |this, window, cx| this.toggle_grid_view(window, cx));
    check(menus, "pptx.notes-page-view", editor.is_notes_page_view());
    menus.connect("pptx.notes-page-view", |this, window, cx| {
        if this.is_notes_page_view() {
            this.switch_to_normal_view(window, cx);
        } else {
            this.switch_to_notes_page_view(window, cx);
        }
    });
    check(menus, "pptx.master-view", editor.is_master_view());
    menus.connect("pptx.master-view", |this, window, cx| {
        if this.is_master_view() {
            this.close_master_view(window, cx);
        } else {
            this.open_master_view(window, cx);
        }
    });
    check(menus, "pptx.notes-format", editor.is_notes_format());
    menus.connect("pptx.notes-format", |this, window, cx| {
        if this.is_notes_format() {
            this.close_notes_format(window, cx);
        } else {
            this.open_notes_format(window, cx);
        }
    });
    check(menus, "pptx.notes-master", editor.is_notes_master());
    menus.connect("pptx.notes-master", |this, window, cx| {
        if this.is_notes_master() {
            this.close_notes_format(window, cx);
        } else {
            this.open_notes_master(window, cx);
        }
    });
    check(menus, "pptx.handout-master", editor.is_handout_master());
    menus.connect("pptx.handout-master", |this, window, cx| {
        if this.is_handout_master() {
            this.close_notes_format(window, cx);
        } else {
            this.open_handout_master(window, cx);
        }
    });
    check(menus, "pptx.reading-view", editor.is_reading_view());
    menus.connect("pptx.reading-view", |this, window, cx| {
        if this.is_reading_view() {
            this.switch_to_normal_view(window, cx);
        } else {
            this.start_reading_view(window, cx);
        }
    });
    menus.connect("view.slideshow", |this, window, cx| this.start_slideshow(window, cx));
    // The guides (the check mark of `pptx.guides` is the shell's: the setting is shared state).
    menus.connect("pptx.guides", |this, _, cx| this.toggle_pptx_guides(cx));
    menus.connect("pptx.guides.add-vertical", |this, _, cx| this.add_guide(false, cx));
    menus.connect("pptx.guides.add-horizontal", |this, _, cx| this.add_guide(true, cx));
    if let Some(item) = menus.find_mut("pptx.guides.remove-all") {
        item.disabled = editor.guide_list().is_empty();
    }
    menus.connect("pptx.guides.remove-all", |this, _, cx| this.remove_all_guides(cx));
}
