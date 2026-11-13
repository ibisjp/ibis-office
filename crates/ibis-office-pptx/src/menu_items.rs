// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A presentation's own lines of the menus, and its button of the toolbar's left part.
//!
//! The shell's table of the commands (`shell/chrome_defaults.rs`) has the lines every screen shares and asks the screen
//! for its own at the places of [`MenuPart`] (`FormatEditor::menu_items`). The lines without a handler are placeholders:
//! the screen's `menus()` connects them (`Menus::connect`).

use gpui_kit::*;

use crate::shell::chrome::{MenuEntry, MenuId, MenuItem, MenuPart, SidebarPanel, ToolButton, ToolItem, text_secondary};
use crate::shell::chrome_defaults::{SEP, item, sub};
use crate::shell::editor::SlideEditor;
use crate::shell::palette::rgb;

/// `FormatEditor::menu_items`.
pub(crate) fn menu_items(editor: &SlideEditor, part: MenuPart) -> Vec<MenuEntry> {
    match part {
        // A presentation can also be written as an OpenDocument presentation.
        MenuPart::FileExport => vec![item("file.export.odp", "slideshow").on(|this, window, cx| this.export_odp(window, cx)).into()],
        MenuPart::ViewStart => view_start(editor),
        MenuPart::Own(MenuId::Slide) => slide_menu(editor),
        MenuPart::Own(MenuId::Arrange) => arrange_menu(),
        _ => Vec::new(),
    }
}

/// 表示: the views, the slide show, the grid and the guides, the notes and the colour mode.
fn view_start(editor: &SlideEditor) -> Vec<MenuEntry> {
    vec![
        // The view switches, the slide show and the guides are connected by the presentation's `menus()` (`pptx/view_menu.rs`).
        item("pptx.normal-view", "view_compact").into(),
        item("pptx.outline-view", "format_list_bulleted").shortcut("⌃⌥2").into(),
        item("pptx.grid-view", "grid_view").shortcut("⌃⌥1").into(),
        item("pptx.notes-page-view", "sticky_note_2").shortcut("⌃⌥3").into(),
        item("pptx.master-view", "dashboard_customize").into(),
        item("pptx.notes-format", "format_bold").into(),
        item("pptx.notes-master", "sticky_note_2").into(),
        item("pptx.handout-master", "view_agenda").into(),
        item("pptx.reading-view", "chrome_reader_mode").into(),
        SEP,
        item("view.slideshow", "play_arrow").shortcut("⌘↩").into(),
        item("pptx.motion", "animation").shortcut("⌥⌘⇧B").into(),
        SEP,
        item("pptx.gridlines", "grid_on").shortcut("⇧F9").checked(editor.is_pptx_gridlines_enabled()).on(|this, _, cx| this.toggle_pptx_gridlines(cx)).into(),
        item("pptx.guides", "straighten").shortcut("⌥F9").checked(editor.is_pptx_guides_enabled()).into(),
        item("pptx.guides.add-vertical", "horizontal_rule_rotate90").into(),
        item("pptx.guides.add-horizontal", "horizontal_rule").into(),
        item("pptx.guides.remove-all", "delete_sweep").into(),
        SEP,
        item("pptx.snap-to-grid", "").checked(editor.is_pptx_snap_to_grid_enabled()).on(|this, _, cx| this.toggle_pptx_snap_to_grid(cx)).into(),
        item("pptx.snap-to-shape", "").checked(editor.is_pptx_snap_to_shape_enabled()).on(|this, _, cx| this.toggle_pptx_snap_to_shape(cx)).into(),
        SEP,
        item("pptx.notes", "").into(),
        SEP,
        sub("pptx.color-mode", "contrast", vec![
            item("pptx.view.color", "palette")
                .checked(editor.color_mode() == crate::render::ColorMode::Color)
                .on(|this, _, cx| {
                    this.set_color_mode(crate::render::ColorMode::Color, cx);
                })
                .into(),
            item("pptx.view.grayscale", "filter_b_and_w")
                .checked(editor.color_mode() == crate::render::ColorMode::Grayscale)
                .on(|this, _, cx| {
                    this.set_color_mode(crate::render::ColorMode::Grayscale, cx);
                })
                .into(),
            item("pptx.view.high-contrast", "contrast")
                .shortcut("⌥⌘H")
                .checked(editor.color_mode() == crate::render::ColorMode::HighContrast)
                .on(|this, _, cx| {
                    this.toggle_high_contrast(cx);
                })
                .into(),
        ]),
        SEP,
    ]
}

/// スライド.
fn slide_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    let can_delete = editor.presentation().slides.len() > 1;
    vec![
        item("pptx.new-slide", "add").shortcut("⌘M").on(|this, window, cx| this.add_slide(window, cx)).into(),
        item("pptx.duplicate-slide", "content_copy").into(),
        item("pptx.delete-slide", "delete").disabled(!can_delete).on(|this, window, cx| this.delete_slide(window, cx)).into(),
        item("pptx.skip-slide", "visibility_off").into(),
        SEP,
        item("pptx.transition", "animation").into(),
        item("pptx.background", "wallpaper").into(),
        sub("pptx.layout", "view_quilt", vec![]),
        item("pptx.theme", "palette").into(),
        item("pptx.edit-theme", "dashboard_customize").into(),
        SEP,
        item("slideshow.settings", "settings").into(),
    ]
}

/// 配置.
fn arrange_menu() -> Vec<MenuEntry> {
    vec![
        sub("pptx.order", "flip_to_front", vec![
            item("pptx.order.front", "flip_to_front").shortcut("⌥⇧⌘↑").into(),
            item("pptx.order.forward", "arrow_upward").shortcut("⌥⌘↑").into(),
            item("pptx.order.backward", "arrow_downward").shortcut("⌥⌘↓").into(),
            item("pptx.order.back", "flip_to_back").shortcut("⌥⇧⌘↓").into(),
        ]),
        sub("pptx.align", "align_horizontal_left", vec![]),
        sub("pptx.distribute", "horizontal_distribute", vec![]),
        sub("pptx.center", "center_focus_strong", vec![]),
        sub("pptx.merge", "cell_merge", vec![]),
        item("pptx.edit-points", "polyline").into(),
        item("pptx.edit-points.smooth", "line_curve").into(),
        item("pptx.edit-points.straight", "polyline").into(),
        item("pptx.edit-points.corner", "turn_sharp_right").into(),
        item("pptx.smartart-to-shapes", "shapes").into(),
        item("pptx.smartart.convert-text", "account_tree").into(),
        item("pptx.smartart.add-shape", "add_circle").into(),
        item("pptx.smartart.remove-shape", "remove_circle").into(),
        item("pptx.smartart.add-assistant", "person_add").into(),
        item("pptx.smartart.move-up", "arrow_upward").into(),
        item("pptx.smartart.move-down", "arrow_downward").into(),
        item("pptx.smartart.promote", "keyboard_double_arrow_up").into(),
        item("pptx.smartart.demote", "keyboard_double_arrow_down").into(),
        item("pptx.smartart.text-pane", "list_alt").into(),
        sub("pptx.smartart.change-layout", "dashboard_customize", vec![
            item("pptx.smartart.layout.list", "view_list").into(),
            item("pptx.smartart.layout.vlist", "format_list_bulleted").into(),
            item("pptx.smartart.layout.process", "arrow_forward").into(),
            item("pptx.smartart.layout.chevron", "double_arrow").into(),
            item("pptx.smartart.layout.cycle", "autorenew").into(),
            item("pptx.smartart.layout.hierarchy", "account_tree").into(),
            item("pptx.smartart.layout.relationship", "sync_alt").into(),
            item("pptx.smartart.layout.matrix", "grid_view").into(),
            item("pptx.smartart.layout.pyramid", "change_history").into(),
            item("pptx.smartart.layout.picture", "insert_photo").into(),
        ]),
        sub("pptx.smartart.color", "palette", vec![
            item("pptx.smartart.color.colorful", "palette").into(),
            item("pptx.smartart.color.accent1", "palette").into(),
            item("pptx.smartart.color.accent2", "palette").into(),
            item("pptx.smartart.color.accent3", "palette").into(),
            item("pptx.smartart.color.outline", "check_box_outline_blank").into(),
        ]),
        sub("pptx.rotate", "rotate_90_degrees_cw", vec![
            item("pptx.rotate.right", "rotate_90_degrees_cw").into(),
            item("pptx.rotate.left", "rotate_90_degrees_ccw").into(),
            SEP,
            item("pptx.rotate.flip-h", "flip").into(),
            item("pptx.rotate.flip-v", "flip").into(),
        ]),
        SEP,
        item("pptx.group", "group_work").shortcut("⌥⌘G").into(),
        item("pptx.ungroup", "workspaces").shortcut("⌥⇧⌘G").into(),
    ]
}

/// `FormatEditor::toolbar_left`: 新しいスライド.
pub(crate) fn toolbar_left(editor: &SlideEditor) -> Vec<ToolItem> {
    vec![
        // A split button as PowerPoint's: the left half adds a slide in the layout of the current one, the ▼ has the
        // layouts to add one in.
        ToolButton::new("new-slide", "add", crate::shell::i18n::text("_shell.new-slide-m"))
            .menu({
                let mut items = super::chrome::new_slide_menu(editor);
                if items.is_empty() {
                    items.push(MenuItem::tr("pptx.new-slide.blank").icon("add").on(|this, window, cx| this.add_slide(window, cx)).into());
                }
                items
            })
            .quick(|this, window, cx| this.add_slide(window, cx))
            .into(),
    ]
}

/// A labelled value of a sidebar panel.
fn field(label: &'static str, value: String) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .py(px(4.))
        .child(div().w(px(72.)).text_size(px(13.)).text_color(text_secondary()).child(label))
        .child(
            div()
                .flex_1()
                .h(px(32.))
                .px(px(8.))
                .flex()
                .items_center()
                .border_1()
                .border_color(rgb(0x747775))
                .rounded(px(4.))
                .text_size(px(14.))
                .child(value),
        )
}

/// The sidebar when no panel of the presentation's has anything to show: the selected shape's size and rotation (read from
/// the model), or the slide's.
pub(crate) fn size_sidebar(editor: &SlideEditor) -> Option<SidebarPanel> {
    let cm = |emu: i64| format!("{:.2} cm", emu as f64 / 360_000.0);
    let body = match editor.selected_shape() {
        Some(shape) => div()
            .px(px(18.))
            .py(px(8.))
            .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).py(px(6.)).child(crate::shell::i18n::text("_shell.size-rotation")))
            .child(field("_shell.width", cm(shape.frame.width)))
            .child(field("_shell.height", cm(shape.frame.height)))
            .child(field("_shell.rotate", format!("{}°", shape.rotation as f64 / 60_000.0)))
            .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).pt(px(14.)).pb(px(6.)).child(crate::shell::i18n::text("_shell.position")))
            .child(field("X", cm(shape.frame.x)))
            .child(field("Y", cm(shape.frame.y))),
        None => {
            let index = editor.presentation().slide_index(editor.current_slide).unwrap_or(0) + 1;
            let size = editor.presentation().slide_size;
            div()
                .px(px(18.))
                .py(px(8.))
                .child(div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).py(px(6.)).child(crate::shell::i18n::format("pptx.chrome.slide-index", &[("index", &index)])))
                .child(field("_shell.width", cm(size.width)))
                .child(field("_shell.height", cm(size.height)))
        }
    };
    let title = if editor.selection.is_some() { "format.options" } else { "menu.slide" };
    Some(SidebarPanel { title: title.into(), body: body.into_any_element() })
}
