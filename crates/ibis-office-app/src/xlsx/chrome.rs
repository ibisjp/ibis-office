// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A presentation's menus, toolbar and sidebar, registered with the shell's
//! chrome (`shell/chrome.rs`) as the mock has them
//! (`tasks/doc/design/toolbar.html`): the tools (選択・テキスト ボックス・
//! 画像・図形・線) and, after them, what fits the selection (a shape, a
//! line, a picture, several shapes, or the slide when nothing is selected).

use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{CharacterFormat, Color, Fill, GradientPath, LineSpacing, ListKind, ParagraphFormat, Rgb, ShapeKind, Stroke, TextAlign, step_font_size};

use super::editing::{Align, ArrowKind, COLORS, LineKind, Order, SHAPES, SelectionKind, Tool, WIDTHS};
use crate::shell::chrome::{Handler, MenuEntry, MenuId, MenuItem, Menus, SidebarPanel, Submenu, ToolButton, ToolItem, text_secondary};
use crate::shell::editor::SlideEditor;

fn sep() -> ToolItem {
    ToolItem::Separator
}

fn handler(f: impl Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) -> Handler {
    std::rc::Rc::new(f)
}

/// Replace the items of submenu `id` (in any menu).
fn fill_submenu(menus: &mut Menus, id: &str, items: Vec<MenuEntry>) {
    fn find<'a>(entries: &'a mut [MenuEntry], id: &str) -> Option<&'a mut Submenu> {
        for entry in entries {
            if let MenuEntry::Submenu(sub) = entry {
                if sub.id == id {
                    return Some(sub);
                }
                if let Some(found) = find(&mut sub.items, id) {
                    return Some(found);
                }
            }
        }
        None
    }
    for (_, entries) in &mut menus.menus {
        if let Some(sub) = find(entries, id) {
            sub.items = items;
            return;
        }
    }
}

fn shape_items() -> Vec<MenuEntry> {
    SHAPES
        .iter()
        .map(|&(preset, label, icon, id)| MenuItem::new(id, label).icon(icon).on(move |this, window, cx| this.set_pptx_tool(Tool::Shape(preset), window, cx)).into())
        .collect()
}

fn text_body_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let body = editor.edited_body().or_else(|| editor.selected_shapes().into_iter().find_map(|s| s.text_body()));
    let on = body.is_some();
    let anchor = body.map(|b| b.anchor);
    let mut items: Vec<MenuEntry> = [("pptx.text.top", "_shell.top", ibis_doc_model::VerticalAnchor::Top), ("pptx.text.middle", "pptx.text.middle", ibis_doc_model::VerticalAnchor::Middle), ("pptx.text.bottom", "_shell.bottom", ibis_doc_model::VerticalAnchor::Bottom)].into_iter().map(|(id, name, value)| {
        MenuItem::new(id, name).disabled(!on).checked(anchor == Some(value)).on(move |e, _, cx| e.format_text_body(|b| b.anchor = value, cx)).into()
    }).collect();
    items.push(MenuEntry::Separator);
    for (id, name, vertical, sideways) in [("pptx.text.horizontal", "_shell.horizontal", false, 0), ("pptx.text.vertical", "_shell.vertical", true, 0), ("pptx.text.clockwise", "_xlsx.rotate-90-clockwise", false, 5_400_000), ("pptx.text.counterclockwise", "_xlsx.rotate-90-counterclockwise", false, 16_200_000)] {
        items.push(MenuItem::new(id, name).disabled(!on).checked(body.is_some_and(|b| b.vertical == vertical && (!vertical || b.vertical_kind == ibis_doc_model::VerticalKind::EastAsian) && b.sideways == sideways)).on(move |e, _, cx| e.format_text_body(|b| { b.vertical = vertical; b.vertical_kind = ibis_doc_model::VerticalKind::EastAsian; b.sideways = sideways; }, cx)).into());
    }
    items.push(MenuEntry::Separator);
    for (id,label,scale) in [("pptx.text.scale100","pptx.text.scale100",100_000),("pptx.text.scale80","pptx.text.scale80",80_000),("pptx.text.scale50","pptx.text.scale50",50_000)] {
        items.push(MenuItem::new(id,label).disabled(!on).checked(body.is_some_and(|b|b.font_scale==scale)).on(move |e,_,cx|e.set_text_font_scale(scale,cx)).into());
    }
    items.push(MenuItem::new("pptx.text.wrap", "_shell.wrap-text-in-shape").disabled(!on).checked(body.is_some_and(|b| b.wrap)).on(|e, _, cx| e.format_text_body(|b| b.wrap = !b.wrap, cx)).into());
    items.push(MenuItem::new("pptx.text.insets-none", "_shell.no-internal-margins").disabled(!on).on(|e, _, cx| e.format_text_body(|b| b.insets = ibis_doc_model::Insets { left: 0, top: 0, right: 0, bottom: 0 }, cx)).into());
    items.push(MenuItem::new("pptx.text.insets-default", "_shell.default-internal-margins").disabled(!on).on(|e, _, cx| e.format_text_body(|b| b.insets = ibis_doc_model::Insets::default(), cx)).into());
    items.push(MenuItem::new("pptx.text.insets-custom", "_shell.internal-margins-and-text-scale").disabled(!on).on(|e, w, cx| e.open_text_insets_panel(w, cx)).into());
    items
}

fn format_copy_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let text = editor.edited_body().is_some() || editor.selected_shapes().into_iter().any(|s| s.text_body().is_some());
    vec![
        MenuItem::new("pptx.paragraph-copy", "_shell.copy-paragraph-format").disabled(!text).on(|e, _, cx| e.copy_paragraph_format(cx)).into(),
        MenuItem::new("pptx.paragraph-paste", "_shell.paste-paragraph-format").disabled(!text || editor.pptx().is_none_or(|s| s.paragraph_format_copy.is_none())).on(|e, w, cx| e.paste_paragraph_format(w, cx)).into(),
        MenuItem::new("pptx.format-copy", "_shell.copy-text-format").disabled(!text).on(|e, w, cx| e.copy_character_format(w, cx)).into(),
        MenuItem::new("pptx.format-paste", "_shell.paste-text-format").disabled(!text || editor.pptx().is_none_or(|s| s.character_format_copy.is_none())).on(|e, w, cx| e.paste_character_format(w, cx)).into(),
    ]
}

fn crop_items() -> Vec<MenuEntry> {
    use ibis_doc_model::Crop;
    [("pptx.crop.left", "_shell.crop-10-from-left", Crop { left: 10_000, ..Default::default() }),
     ("pptx.crop.right", "_shell.crop-10-from-right", Crop { right: 10_000, ..Default::default() }),
     ("pptx.crop.top", "_shell.crop-10-from-top", Crop { top: 10_000, ..Default::default() }),
     ("pptx.crop.bottom", "_shell.crop-10-from-bottom", Crop { bottom: 10_000, ..Default::default() }),
     ("pptx.crop.center", "_shell.crop-10-from-all-sides", Crop { left: 10_000, top: 10_000, right: 10_000, bottom: 10_000 })]
        .into_iter().map(|(id, name, crop)| MenuItem::new(id, name).on(move |e, _, cx| e.crop_selected_pictures(crop, cx)).into()).chain(std::iter::once(MenuItem::new("pptx.crop.custom", "_shell.crop-settings").on(|e, w, cx| e.open_crop_panel(w, cx)).into())).collect()
}

fn image_items() -> Vec<MenuEntry> {
    vec![MenuItem::new("image.file", "_shell.upload-from-computer").icon("upload").on(|this, window, cx| this.pick_picture(window, cx)).into()]
}

fn line_items() -> Vec<MenuEntry> {
    vec![
        MenuItem::new("pptx.line", "_shell.line-2").icon("horizontal_rule").on(|this, window, cx| this.set_pptx_tool(Tool::Line { kind: LineKind::Straight, arrow: ArrowKind::None }, window, cx)).into(),
        MenuItem::new("pptx.arrow", "_shell.arrow").icon("arrow_right_alt").on(|this, window, cx| this.set_pptx_tool(Tool::Line { kind: LineKind::Straight, arrow: ArrowKind::Single }, window, cx)).into(),
    ]
}

fn layout_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    editor
        .presentation()
        .layouts
        .iter()
        .map(|layout| {
            let id = layout.id;
            let name = if layout.name.is_empty() { crate::shell::i18n::text("pptx.chrome.no-name").to_string() } else { layout.name.clone() };
            MenuItem::new("pptx.new-slide.layout", name).icon("view_quilt").on(move |this, window, cx| this.add_slide_with_layout(id, window, cx)).into()
        })
        .collect()
}

pub(crate) fn menus(editor: &SlideEditor, menus: &mut Menus) {
    let has = !editor.selected_ids().is_empty();
    let several = editor.selected_ids().len() >= 2;
    menus.connect("pptx.duplicate-slide", |this, window, cx| this.duplicate_current_slide(window, cx));
    menus.connect("pptx.skip-slide", |this, window, cx| this.toggle_skip_slide(window, cx));
    menus.connect("pptx.notes", |this, window, cx| this.toggle_notes(window, cx));
    let notes = editor.notes_shown();
    if let Some(item) = menus.find_mut("pptx.notes") {
        item.checked = notes;
    }
    let skipped = editor.slide().hidden;
    if let Some(item) = menus.find_mut("pptx.skip-slide") {
        item.checked = skipped;
    }
    menus.connect("pptx.background", |this, _, cx| {
        this.set_selection(Vec::new());
        this.sidebar_open = true;
        cx.notify();
    });
    if editor.pptx().is_some_and(|s|s.section_panel.is_some()) {
        menus.items_mut(MenuId::Slide).push(Submenu::new("pptx.section-actions","view_quilt","_shell.section-input",vec![MenuItem::new("pptx.section.apply","_shell.apply").on(|e,w,cx|e.apply_section_panel(w,cx)).into(),MenuItem::new("pptx.section.cancel","_shell.cancel").on(|e,w,cx|e.close_section_panel(w,cx)).into()]).into());
    }
    if editor.pptx().is_some_and(|s| s.size_panel.is_some()) {
        menus.items_mut(MenuId::Slide).push(Submenu::new("pptx.slide-size-actions", "aspect_ratio", "_shell.slide-size-input", vec![
            MenuItem::new("pptx.slide-size.apply", "_shell.apply").on(|e, w, cx| e.apply_slide_size_panel(w, cx)).into(),
            MenuItem::new("pptx.slide-size.cancel", "_shell.cancel").on(|e, w, cx| e.close_slide_size_panel(w, cx)).into(),
        ]).into());
    }
    if editor.pptx().is_some_and(|s| s.insets_panel.is_some()) {
        menus.items_mut(MenuId::Format).push(Submenu::new("pptx.insets-actions", "format_indent_increase", "_shell.internal-margins-input", vec![
            MenuItem::new("pptx.insets.apply", "_shell.apply").on(|e, w, cx| e.apply_text_insets_panel(w, cx)).into(),
            MenuItem::new("pptx.insets.cancel", "_shell.cancel").on(|e, w, cx| e.close_text_insets_panel(w, cx)).into(),
        ]).into());
    }
    if editor.pptx().is_some_and(|s| s.frame_panel.is_some()) {
        menus.items_mut(MenuId::Format).push(Submenu::new("pptx.frame-actions", "aspect_ratio", "_shell.size-position-input", vec![
            MenuItem::new("pptx.frame.apply", "_shell.apply").on(|e, w, cx| e.apply_frame_panel(w, cx)).into(),
            MenuItem::new("pptx.frame.cancel", "_shell.cancel").on(|e, w, cx| e.close_frame_panel(w, cx)).into(),
        ]).into());
    }
    if editor.pptx().is_some_and(|s| s.crop_panel.is_some()) {
        menus.items_mut(MenuId::Format).push(Submenu::new("pptx.crop-actions", "crop", "_shell.crop-input", vec![
            MenuItem::new("pptx.crop.apply", "_shell.apply").on(|e, w, cx| e.apply_crop_panel(w, cx)).into(),
            MenuItem::new("pptx.crop.cancel", "_shell.cancel").on(|e, w, cx| e.close_crop_panel(w, cx)).into(),
        ]).into());
    }
    if let Some(p)=editor.pptx().and_then(|s|s.gradient_panel.as_ref()) {
        let mut actions=vec![MenuItem::new("pptx.gradient.apply","_shell.apply").on(|e,w,cx|e.apply_gradient_panel(w,cx)).into(),MenuItem::new("pptx.gradient.cancel","_shell.cancel").on(|e,w,cx|e.close_gradient_panel(w,cx)).into(),MenuItem::new("pptx.gradient.add","pptx.chrome.add-stop").disabled(p.pattern.is_some()).on(|e,w,cx|e.add_gradient_stop(w,cx)).into()];
        if let Some(selected)=&p.pattern {
            for &(id,preset,label) in super::gradient_panel::PATTERN_CHOICES {actions.push(MenuItem::new(id,label).checked(selected==preset).on(move|e,_,cx|e.set_pattern_preset(preset,cx)).into());}
        }
        if let Some(selected)=p.path {
            for (id,label,path) in [("pptx.gradient.circle","pptx.common.circle",GradientPath::Circle),("pptx.gradient.rect","pptx.chrome.square",GradientPath::Rect),("pptx.gradient.shape","insert.shape",GradientPath::Shape)] {actions.push(MenuItem::new(id,label).checked(selected==path).on(move|e,_,cx|e.set_gradient_path(path,cx)).into());}
        }
        actions.push(MenuItem::new("pptx.gradient.remove-first","_shell.delete-first-stop").disabled(p.pattern.is_some() || p.stops.len()<=2).on(|e,w,cx|e.remove_gradient_stop(0,w,cx)).into());
        actions.push(MenuItem::new("pptx.gradient.remove-last","_shell.delete-last-stop").disabled(p.pattern.is_some() || p.stops.len()<=2).on(|e,w,cx| {let Some(n)=e.pptx().and_then(|s|s.gradient_panel.as_ref()).map(|p|p.stops.len()) else {return};e.remove_gradient_stop(n-1,w,cx);}).into());
        menus.items_mut(MenuId::Format).push(Submenu::new("pptx.gradient-actions","format_color_fill",if p.pattern.is_some() {"pptx.chrome.pattern-fill"} else {"pptx.chrome.gradient-fill"},actions).into());
    }
    if editor.pptx().is_some_and(|s| s.glow_panel.is_some()) {
        menus.items_mut(MenuId::Format).push(Submenu::new("pptx.glow-actions", "format_color_fill", "_shell.glow-input", vec![
            MenuItem::new("pptx.glow.apply", "_shell.apply").on(|e,w,cx|e.apply_glow_panel(w,cx)).into(),
            MenuItem::new("pptx.glow.cancel", "_shell.cancel").on(|e,w,cx|e.close_glow_panel(w,cx)).into(),
        ]).into());
    }
    if editor.pptx().is_some_and(|s| s.shadow_panel.is_some()) {
        menus.items_mut(MenuId::Format).push(Submenu::new("pptx.shadow-actions", "format_color_fill", "_shell.shadow-input", vec![
            MenuItem::new("pptx.shadow.apply", "_shell.apply").on(|e,w,cx|e.apply_shadow_panel(w,cx)).into(),
            MenuItem::new("pptx.shadow.cancel", "_shell.cancel").on(|e,w,cx|e.close_shadow_panel(w,cx)).into(),
        ]).into());
    }
    if editor.pptx().is_some_and(|s| s.timing_panel.is_some()) {
        menus.items_mut(MenuId::Slide).push(Submenu::new("pptx.timing-actions", "transition", "_shell.timing-input", vec![
            MenuItem::new("pptx.timing.auto", "_shell.automatically").on(|e, _, cx| e.toggle_timing_panel(true, cx)).into(),
            MenuItem::new("pptx.timing.click", "_shell.on-click").on(|e, _, cx| e.toggle_timing_panel(false, cx)).into(),
            MenuItem::new("pptx.timing.apply", "_shell.apply").on(|e, w, cx| e.apply_timing_panel(w, cx)).into(),
            MenuItem::new("pptx.timing.cancel", "_shell.cancel").on(|e, w, cx| e.close_timing_panel(w, cx)).into(),
        ]).into());
    }
    menus.items_mut(MenuId::Format).push(Submenu::new("pptx.text-outline", "format_color_text", "_shell.text-outline", vec![
        MenuItem::new("pptx.text-outline.black", "_shell.black-1-pt").on(|e, w, cx| e.format_characters(w, cx, ibis_doc_model::CharacterFormat { outline: Some(Some((ibis_doc_model::Rgb(0), 12_700))), ..Default::default() })).into(),
        MenuItem::new("pptx.text-outline.none", "_shell.no-outline").on(|e, w, cx| e.format_characters(w, cx, ibis_doc_model::CharacterFormat { outline: Some(None), ..Default::default() })).into(),
    ]).into());
    menus.items_mut(MenuId::Format).push(MenuItem::new("pptx.frame.settings", "_shell.size-position").disabled(editor.selected_shapes().is_empty()).on(|e, w, cx| e.open_frame_panel(w, cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::new("pptx.paragraph.settings", "_shell.line-and-paragraph-spacing-options").on(|e, w, cx| e.open_paragraph_panel(w, cx)).into());
    if editor.pptx().is_some_and(|s| s.paragraph_panel.is_some()) {
        use super::paragraph_panel::Mode;
        menus.items_mut(MenuId::Format).push(Submenu::new("pptx.paragraph-actions", "format_line_spacing", "_shell.paragraph-spacing-input", vec![
            MenuItem::new("pptx.paragraph.default", "_shell.default").on(|e, _, cx| e.set_paragraph_panel_mode(Mode::Default, cx)).into(),
            MenuItem::new("pptx.paragraph.multiple", "_shell.multiple").on(|e, _, cx| e.set_paragraph_panel_mode(Mode::Multiple, cx)).into(),
            MenuItem::new("pptx.paragraph.points", "docx.common.exactly").on(|e, _, cx| e.set_paragraph_panel_mode(Mode::Points, cx)).into(),
            Submenu::new("pptx.paragraph.font-align", "format_align_left", "_shell.font-size-alignment", [
                ("pptx.paragraph.font-auto", "_shell.auto", ibis_doc_model::FontAlign::Auto),
                ("pptx.paragraph.font-top", "_shell.top", ibis_doc_model::FontAlign::Top),
                ("pptx.paragraph.font-center", "_shell.center", ibis_doc_model::FontAlign::Center),
                ("pptx.paragraph.font-baseline", "pptx.chrome.baseline", ibis_doc_model::FontAlign::Baseline),
                ("pptx.paragraph.font-bottom", "_shell.bottom", ibis_doc_model::FontAlign::Bottom),
            ].into_iter().map(|(id, label, value)| MenuItem::new(id, label).on(move |e, _, cx| e.set_paragraph_panel_font_align(value, cx)).into()).collect()).into(),
            MenuItem::new("pptx.paragraph.apply", "_shell.apply").on(|e, w, cx| e.apply_paragraph_panel(w, cx)).into(),
            MenuItem::new("pptx.paragraph.cancel", "_shell.cancel").on(|e, w, cx| e.close_paragraph_panel(w, cx)).into(),
        ]).into());
    }
    menus.connect("edit.find-replace", |e, w, cx| e.open_pptx_find(w, cx));
    menus.items_mut(MenuId::Edit).push(Submenu::new("pptx.find-menu", "find_replace", "_shell.find-and-replace-actions", vec![
        MenuItem::new("pptx.find-case", "_shell.match-case").on(|e, _, cx| e.toggle_pptx_find_option(true, cx)).into(),
        MenuItem::new("pptx.find-word", "pptx.chrome.find-whole-words-only").on(|e, _, cx| e.toggle_pptx_find_option(false, cx)).into(),
        MenuItem::new("pptx.find-next", "_xlsx.find-next").on(|e, w, cx| e.pptx_find_next(true, w, cx)).into(),
        MenuItem::new("pptx.find-previous", "_shell.find-previous").on(|e, w, cx| e.pptx_find_next(false, w, cx)).into(),
        MenuItem::new("pptx.replace-one", "_shell.replace").on(|e, w, cx| e.pptx_replace(false, w, cx)).into(),
        MenuItem::new("pptx.replace-all", "_shell.replace-all").on(|e, w, cx| e.pptx_replace(true, w, cx)).into(),
    ]).into());
    menus.connect("edit.select-all", |this, window, cx| this.select_all_shapes(window, cx));
    if has {
        if editor.selected_shapes().iter().all(|shape| ibis_pptx::save::check_shape_insertion(shape).is_ok()) {
            menus.connect("edit.duplicate", |this, _, cx| this.duplicate_selected(cx));
            menus.connect("edit.copy", |this, _, _| this.copy_selected());
            menus.connect("edit.cut", |this, _, cx| this.cut_selected(cx));
        }
        for (id, order) in [("pptx.order.front", Order::Front), ("pptx.order.forward", Order::Forward), ("pptx.order.backward", Order::Backward), ("pptx.order.back", Order::Back)] {
            menus.connect(id, move |this, _, cx| this.order_selected(order, cx));
        }
        menus.connect("pptx.rotate.right", |this, _, cx| this.rotate_selected(90, cx));
        menus.connect("pptx.rotate.left", |this, _, cx| this.rotate_selected(-90, cx));
        menus.connect("pptx.rotate.flip-h", |this, _, cx| this.flip_selected(true, cx));
        menus.connect("pptx.rotate.flip-v", |this, _, cx| this.flip_selected(false, cx));
        let align = |id: &'static str, icon: &'static str, label: &'static str, a: Align| -> MenuEntry { MenuItem::new(id, label).icon(icon).on(move |this, _, cx| this.align_selected(a, cx)).into() };
        fill_submenu(menus, "pptx.align", vec![
            align("pptx.align.left", "align_horizontal_left", "align.left", Align::Left),
            align("pptx.align.center", "align_horizontal_center", "_shell.align-center", Align::Center),
            align("pptx.align.right", "align_horizontal_right", "align.right", Align::Right),
            MenuEntry::Separator,
            align("pptx.align.top", "align_vertical_top", "_shell.align-top", Align::Top),
            align("pptx.align.middle", "align_vertical_center", "docx.picture.align.middle", Align::Middle),
            align("pptx.align.bottom", "align_vertical_bottom", "_shell.align-bottom", Align::Bottom),
        ]);
        fill_submenu(menus, "pptx.center", vec![
            MenuItem::new("pptx.center.h", "_shell.align-center-2").icon("align_horizontal_center").on(|this, _, cx| this.center_selected(true, cx)).into(),
            MenuItem::new("pptx.center.v", "_shell.align-middle").icon("align_vertical_center").on(|this, _, cx| this.center_selected(false, cx)).into(),
        ]);
    }
    if several {
        fill_submenu(menus, "pptx.distribute", vec![
            MenuItem::new("pptx.distribute.h", "_shell.distribute-horizontally").icon("horizontal_distribute").disabled(editor.selected_ids().len() < 3).on(|this, _, cx| this.distribute_selected(true, cx)).into(),
            MenuItem::new("pptx.distribute.v", "_shell.distribute-vertically").icon("vertical_distribute").disabled(editor.selected_ids().len() < 3).on(|this, _, cx| this.distribute_selected(false, cx)).into(),
        ]);
    }
    if editor.can_group() {
        menus.connect("pptx.group", |this, _, cx| this.group_selected(cx));
    }
    if editor.can_ungroup() {
        menus.connect("pptx.ungroup", |this, _, cx| this.ungroup_selected(cx));
    }
    if editor.pptx().is_some_and(|s| SlideEditor::can_copy_shapes(&s.clipboard)) {
        menus.connect("edit.paste", |this, _, cx| {
            this.paste_shapes(cx);
        });
    }
    fill_submenu(menus, "insert.shape", shape_items());
    fill_submenu(menus, "insert.table", table_size_items());
    // 表示形式 > テキスト: 上付き・下付き・文字の間隔.
    let baseline = editor.selected_baseline();
    fill_submenu(menus, "format.text", vec![
        MenuItem::new("text.superscript", "docx.chrome.superscript").checked(baseline > 0).on(|e, w, cx| e.toggle_baseline(30_000, w, cx)).into(),
        MenuItem::new("text.subscript", "docx.chrome.subscript").checked(baseline < 0).on(|e, w, cx| e.toggle_baseline(-25_000, w, cx)).into(),
        MenuEntry::Separator,
        MenuItem::new("text.spacing-tight", "_shell.character-spacing-tight").on(|e, w, cx| e.format_characters(w, cx, ibis_doc_model::CharacterFormat { spacing: Some(-150), ..Default::default() })).into(),
        MenuItem::new("text.spacing-normal", "_shell.character-spacing-normal").on(|e, w, cx| e.format_characters(w, cx, ibis_doc_model::CharacterFormat { spacing: Some(0), ..Default::default() })).into(),
        MenuItem::new("text.spacing-loose", "_shell.character-spacing-loose").on(|e, w, cx| e.format_characters(w, cx, ibis_doc_model::CharacterFormat { spacing: Some(300), ..Default::default() })).into(),
    ]);
    menus.items_mut(MenuId::Format).push(Submenu::new("pptx.format-copy-menu", "format_paint", "_shell.copy-paste-formatting", format_copy_items(editor)).into());
    menus.items_mut(MenuId::Format).push(Submenu::new("pptx.text-body", "format_textdirection_l_to_r", "_shell.text-box", text_body_items(editor)).into());
    menus.items_mut(MenuId::Format).push(Submenu::new("pptx.shadow","format_color_fill","_shell.outer-shadow",[
        ("pptx.shadow.none","_shell.no-outer-shadow",None),
        ("pptx.shadow.right","pptx.shadow.right",Some((0,0))),
        ("pptx.shadow.diagonal","pptx.shadow.diagonal",Some((2_700_000,0))),
        ("pptx.shadow.soft","pptx.shadow.soft",Some((2_700_000,50_800))),
    ].into_iter().map(|(id,label,value)| {
        let shadow=value.map(|(direction,blur)| ibis_doc_model::Shadow::simple(Color {rgb:Rgb(0),alpha:128},blur,50_800,direction));
        MenuItem::new(id,label).disabled(!editor.selected_shapes().iter().any(|s|matches!(s.kind,ShapeKind::Auto(_)))).on(move |e,_,cx|e.set_shape_shadow(shadow,cx)).into()
    }).collect()).into());
    menus.items_mut(MenuId::Format).push(MenuItem::new("pptx.fill.custom","_shell.linear-gradient-options").disabled(editor.selected_shapes().is_empty() || editor.selected_shapes().iter().any(|s|!matches!(s.kind,ShapeKind::Auto(_)|ShapeKind::Table(_)))).on(|e,w,cx|e.open_gradient_panel(false,w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::new("pptx.background.custom","_shell.background-linear-gradient-options").on(|e,w,cx|e.open_gradient_panel(true,w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::new("pptx.fill.custom-path","_shell.path-gradient-options").disabled(editor.selected_shapes().is_empty() || editor.selected_shapes().iter().any(|s|!matches!(s.kind,ShapeKind::Auto(_)|ShapeKind::Table(_)))).on(|e,w,cx|e.open_path_gradient_panel(false,w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::new("pptx.background.custom-path","_shell.background-path-gradient-options").on(|e,w,cx|e.open_path_gradient_panel(true,w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::new("pptx.fill.custom-pattern","_shell.pattern-options").disabled(editor.selected_shapes().is_empty() || editor.selected_shapes().iter().any(|s|!matches!(s.kind,ShapeKind::Auto(_)|ShapeKind::Table(_)))).on(|e,w,cx|e.open_pattern_panel(false,w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::new("pptx.background.custom-pattern","_shell.background-pattern-options").on(|e,w,cx|e.open_pattern_panel(true,w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::new("pptx.glow.custom", "_shell.glow-options").disabled(editor.selected_shapes().is_empty() || editor.selected_shapes().iter().any(|s| !matches!(s.kind,ShapeKind::Auto(_)))).on(|e,w,cx|e.open_glow_panel(w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::new("pptx.shadow.custom", "_shell.shadow-options").disabled(editor.selected_shapes().is_empty() || editor.selected_shapes().iter().any(|s| !matches!(s.kind,ShapeKind::Auto(_)))).on(|e,w,cx|e.open_shadow_panel(w,cx)).into());
    menus.items_mut(MenuId::Format).push(Submenu::new("pptx.glow","format_color_fill","_shell.glow",[
        ("pptx.glow.none","_shell.no-glow",None),
        ("pptx.glow.blue5","pptx.glow.blue5",Some((Color { rgb:Rgb(0x4472C4),alpha:128 },63_500))),
        ("pptx.glow.blue10","pptx.glow.blue10",Some((Color { rgb:Rgb(0x4472C4),alpha:128 },127_000))),
        ("pptx.glow.yellow10","pptx.glow.yellow10",Some((Color { rgb:Rgb(0xFFC000),alpha:160 },127_000))),
    ].into_iter().map(|(id,label,value)| MenuItem::new(id,label).disabled(!editor.selected_shapes().iter().any(|s| matches!(s.kind,ShapeKind::Auto(_)))).on(move |e,_,cx| e.set_shape_glow(value,cx)).into()).collect()).into());
    menus.items_mut(MenuId::Format).push(MenuItem::new("pptx.picture.reset-colors", "_shell.reset-picture-color-effects")
        .disabled(!editor.selected_shapes().iter().any(|s| matches!(&s.kind, ShapeKind::Picture(p) if p.duotone.is_some() || p.clear.is_some())))
        .on(|e, _, cx| e.reset_picture_color_effects(cx)).into());
    menus.connect("format.clear", |this, window, cx| this.clear_formats(window, cx));
    fill_submenu(menus, "insert.image", image_items());
    fill_submenu(menus, "pptx.layout", apply_layout_items(editor));
    // 新しいスライド from a chosen layout, after 新しいスライド.
    let size = editor.presentation().slide_size;
    let slide_menu = menus.items_mut(MenuId::Slide);
    slide_menu.push(MenuItem::new("pptx.reset-placement", "_shell.reset-placeholder-position").disabled(editor.slide().layout.is_none()).on(|e, w, cx| e.reset_placeholder_frames(w, cx)).into());
    slide_menu.push(Submenu::new("pptx.sections","view_quilt","pptx.goto.section",vec![MenuItem::new("pptx.section.add","_shell.add-section").on(|e,w,cx|e.open_section_panel(false,w,cx)).into(),MenuItem::new("pptx.section.rename","_shell.rename").disabled(editor.current_section().is_none()).on(|e,w,cx|e.open_section_panel(true,w,cx)).into(),MenuItem::new("pptx.section.fold","_shell.collapse-expand").disabled(editor.current_section().is_none()).on(|e,_,cx|e.toggle_current_section_fold(cx)).into(),MenuItem::new("pptx.section.move-up","_shell.move-section-up").disabled(editor.current_section().is_none_or(|i|i==0)).on(|e,w,cx|e.move_current_section(-1,w,cx)).into(),MenuItem::new("pptx.section.move-down","_shell.move-section-down").disabled(editor.current_section().is_none_or(|i|i+1>=editor.presentation().sections.len())).on(|e,w,cx|e.move_current_section(1,w,cx)).into(),MenuItem::new("pptx.section.remove","_shell.remove-section").disabled(editor.current_section().is_none()).on(|e,w,cx|e.remove_current_section(w,cx)).into()]).into());
    slide_menu.push(Submenu::new("pptx.slide-size", "aspect_ratio", "_shell.slide-size-keep-shape-size", vec![
        MenuItem::new("pptx.slide-size.standard", "pptx.chrome.standard-4-3").checked(size == ibis_doc_model::EmuSize::new(9_144_000, 6_858_000)).on(|e, w, cx| e.set_slide_size(ibis_doc_model::EmuSize::new(9_144_000, 6_858_000), w, cx)).into(),
        MenuItem::new("pptx.slide-size.wide", "pptx.chrome.widescreen-16-9").checked(size == ibis_doc_model::EmuSize::WIDESCREEN).on(|e, w, cx| e.set_slide_size(ibis_doc_model::EmuSize::WIDESCREEN, w, cx)).into(),
        MenuItem::new("pptx.slide-size.custom", "_shell.custom-slide-size").on(|e, w, cx| e.open_slide_size_panel(w, cx)).into(),
    ]).into());
    let at = slide_menu.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "pptx.new-slide")).map_or(0, |i| i + 1);
    slide_menu.insert(at, Submenu::new("pptx.new-slide-layout", "view_quilt", "_shell.new-slide-with-layout", new_slide_menu(editor)).into());
    menus.connect("insert.text-box", |this, window, cx| this.set_pptx_tool(Tool::TextBox, window, cx));
    menus.connect("insert.symbol", |this, _, cx| this.open_symbols(cx));
}

pub(crate) fn toolbar_left(editor: &SlideEditor) -> Vec<ToolItem> {
    let _ = editor;
    Vec::new()
}

/// レイアウトを適用: each layout, the current slide's checked.
fn apply_layout_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let current = editor.slide().layout;
    editor
        .presentation()
        .layouts
        .iter()
        .map(|layout| {
            let id = layout.id;
            let name = if layout.name.is_empty() { crate::shell::i18n::text("pptx.chrome.no-name").to_string() } else { layout.name.clone() };
            MenuItem::new("pptx.layout.apply", name).icon("view_quilt").checked(current == Some(id)).on(move |this, window, cx| this.apply_layout(id, window, cx)).into()
        })
        .collect()
}

/// A new slide from each layout.
fn new_slide_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    layout_items(editor)
}

/// The colours a button offers, as a palette (`shell/color_palette.rs`): "none" when `none` names it, the
/// workbook's theme colours with their tints, the standard colours, those used lately.
fn color_menu(editor: &SlideEditor, _id: &'static str, none: Option<&'static str>, f: impl Fn(&mut SlideEditor, Option<Rgb>, &mut Window, &mut Context<SlideEditor>) + Clone + 'static) -> Vec<MenuEntry> {
    use crate::shell::color_palette::{theme_rows, Palette, PickColor, Swatch};
    let pick: PickColor = std::rc::Rc::new(f.clone());
    let none = none.map(|label| {
        let g = f.clone();
        MenuItem::new("color.none", label).icon("format_color_reset").on(move |this, window, cx| g(this, None, window, cx))
    });
    let theme = theme_rows(&editor.current_theme_palette_colors(), &pick, None);
    // The standard colours: the ten of the colour list that are not shades of grey.
    let standard = COLORS
        .iter()
        .filter(|&&(_, rgb, _)| !matches!(rgb, 0x000000 | 0x404040 | 0x7f7f7f | 0xd9d9d9 | 0xffffff | 0x4472c4))
        .map(|&(label, rgb, id)| Swatch::with_id(id, Rgb(rgb), label, false, &pick))
        .collect();
    vec![MenuEntry::Palette(Palette { none, theme, standard, pick: Some(pick), more: None, button: None, current:None })]
}

fn fill_presets(background: bool) -> Vec<MenuEntry> {
    use ibis_doc_model::{GradientPath, GradientStop};
    let stops = vec![GradientStop { position: 0, color: Color::opaque(Rgb(0x4472c4)) }, GradientStop { position: 100_000, color: Color::WHITE }];
    let mut presets = Vec::new();
    for (id, label, angle) in [("fill.gradient.0", "fill.gradient.0", 0), ("fill.gradient.90", "fill.gradient.90", 5_400_000), ("fill.gradient.180", "fill.gradient.180", 10_800_000), ("fill.gradient.270", "fill.gradient.270", 16_200_000)] {
        presets.push((id, label, Fill::Linear { angle, scaled: false, stops: stops.clone() }));
    }
    for (id, label, path) in [("fill.path.circle", "fill.path.circle", GradientPath::Circle), ("fill.path.rect", "fill.path.rect", GradientPath::Rect), ("fill.path.shape", "fill.path.shape", GradientPath::Shape)] {
        presets.push((id, label, Fill::Path { path, focus: [50_000; 4], stops: stops.clone() }));
    }
    let gradients = presets.into_iter().map(|(id, label, fill)| MenuItem::new(id, label).on(move |e, _, cx| if background { e.set_background(fill.clone(), cx) } else { e.fill_selected(fill.clone(), cx) }).into()).collect();
    let patterns = [("pct25", "pptx.chrome.black-white-25-percent"), ("pct50", "pptx.chrome.black-white-50-percent"), ("pct75", "pptx.chrome.black-white-75-percent"), ("horz", "pptx.chrome.black-white-horizontal-lines"), ("vert", "pptx.chrome.black-white-vertical-lines"), ("diagCross", "pptx.chrome.black-white-diagonal-grid")].into_iter().map(|(preset, label)| {
        let fill = Fill::Pattern { preset: preset.into(), foreground: Color::BLACK, background: Color::WHITE };
        MenuItem::new(preset, label).on(move |e, _, cx| if background { e.set_background(fill.clone(), cx) } else { e.fill_selected(fill.clone(), cx) }).into()
    }).collect();
    vec![Submenu::new("pptx.fill-gradients", "format_color_fill", "pptx.chrome.gradient", gradients).into(), Submenu::new("pptx.fill-patterns", "format_color_fill", "pptx.chrome.pattern", patterns).into()]
}

/// 背景: a colour, or the layout's background back.
fn background_menu() -> Vec<MenuEntry> {
    let mut items = color_menu(editor, "pptx.background", None, |this, rgb, _, cx| {
        if let Some(rgb) = rgb {
            this.set_background(Fill::Solid(Color::opaque(rgb)), cx);
        }
    });
    items.push(MenuEntry::Separator);
    items.extend(fill_presets(true));
    items.push(
        MenuItem::new("background.reset", "_shell.reset-to-theme")
            .icon("format_color_reset")
            .on(|this, _, cx| {
                let layout = this.slide().layout.and_then(|l| this.presentation().layout(l)).map(|l| l.background.clone());
                if let Some(fill) = layout {
                    this.set_background(fill, cx);
                }
            })
            .into(),
    );
    items
}

/// 表の挿入: the sizes offered (rows × columns).
fn table_size_items() -> Vec<MenuEntry> {
    const SIZES: [(usize, usize, &str, &str); 6] = [
        (2, 2, "table.new.2x2", "_xlsx.2-2-table"),
        (3, 3, "table.new.3x3", "_xlsx.3-3-table"),
        (3, 4, "table.new.3x4", "_xlsx.3-4-table"),
        (4, 4, "table.new.4x4", "_xlsx.4-4-table"),
        (5, 5, "table.new.5x5", "_xlsx.5-5-table"),
        (6, 4, "table.new.6x4", "_xlsx.6-4-table"),
    ];
    SIZES.iter().map(|&(r, c, id, label)| MenuItem::new(id, label).icon("table").on(move |this, window, cx| this.insert_slide_table(r, c, window, cx)).into()).collect()
}

/// 行と列: insert and remove rows and columns of the selected table.
fn table_button(editor: &SlideEditor) -> ToolItem {
    let merged = editor.selected_shapes().first().is_some_and(|s| matches!(&s.kind, ShapeKind::Table(t) if t.has_merges()));
    let edit = editor.text_editing();
    let can_merge = edit.is_some_and(|e| e.cell.is_some() && e.cell_anchor.is_some());
    let can_split = edit.and_then(|e| Some((e.shape, e.cell?))).is_some_and(|(id, (r, c))| {
        editor.slide().shape(id).is_some_and(|s| matches!(&s.kind, ShapeKind::Table(t) if t.rows.get(r).and_then(|row| row.cells.get(c)).is_some_and(|x| !x.merged && (x.row_span > 1 || x.column_span > 1))))
    });
    let item = |id: &'static str, icon: &'static str, label: &'static str, f: fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>)| -> MenuEntry {
        MenuItem::new(id, label).icon(icon).disabled(merged).on(move |this, window, cx| f(this, window, cx)).into()
    };
    ToolButton::new("table.rows", "table", "pptx.chrome.rows-columns")
        .menu(vec![
            item("table.insert-above", "arrow_upward", "_shell.insert-row-above", |e, w, cx| e.table_insert_row(false, w, cx)),
            item("table.insert-below", "arrow_downward", "_shell.insert-row-below", |e, w, cx| e.table_insert_row(true, w, cx)),
            item("table.insert-left", "border_left", "_shell.insert-column-left", |e, w, cx| e.table_insert_column(false, w, cx)),
            item("table.insert-right", "arrow_forward", "_shell.insert-column-right", |e, w, cx| e.table_insert_column(true, w, cx)),
            MenuEntry::Separator,
            item("table.remove-row", "delete", "docx.context-menu.delete-row", |e, w, cx| e.table_remove_row(w, cx)),
            item("table.remove-column", "delete", "docx.context-menu.delete-column", |e, w, cx| e.table_remove_column(w, cx)),
            MenuEntry::Separator,
            MenuItem::new("table.merge", "_shell.merge-cells-cells-selected-with-shift-click")
                .icon("table")
                .disabled(!can_merge)
                .on(|e, w, cx| e.merge_table_cells(w, cx))
                .into(),
            MenuItem::new("table.split", "_shell.split-cells").icon("table").disabled(!can_split).on(|e, w, cx| e.split_table_cell(w, cx)).into(),
        ])
        .into()
}

/// The transitions offered (画面切り替え), with PowerPoint's names.
pub(crate) const TRANSITIONS: [(&str, &str, &str); 15] = [
    ("cut", "_shell.cut", "transition.cut"),
    ("fade", "_shell.fade", "transition.fade"),
    ("push", "_shell.push", "transition.push"),
    ("wipe", "_shell.wipe", "transition.wipe"),
    ("split", "_shell.split", "transition.split"),
    ("cover", "_shell.cover", "transition.cover"),
    ("pull", "_shell.uncover", "transition.pull"),
    ("randomBar", "_shell.random-bars", "transition.randomBar"),
    ("circle", "insert.shape", "transition.circle"),
    ("dissolve", "_shell.dissolve", "transition.dissolve"),
    ("zoom", "zoom.dialog.title", "transition.zoom"),
    ("vortex", "transition.vortex", "transition.vortex"),
    ("ripple", "_shell.ripple", "transition.ripple"),
    ("glitter", "_shell.glitter", "transition.glitter"),
    ("wheel", "_shell.wheel", "transition.wheel"),
];

/// The name shown for a slide's transition.
pub(crate) fn transition_name(t: Option<&ibis_doc_model::Transition>) -> String {
    match t {
        None => crate::shell::i18n::text("_shell.none").into(),
        Some(t) if t.effect.is_empty() => crate::shell::i18n::text("pptx.chrome.none-timing-only").to_string(),
        Some(t) => {
            let name = TRANSITIONS.iter().find(|(e, _, _)| *e == t.effect).map_or(t.effect.clone(), |(_, n, _)| crate::shell::i18n::text(n).to_string());
            let seconds = format!("{:.1}", t.duration as f32 / 1000.0);
            crate::shell::i18n::format("_xlsx.name-seconds-s", &[("name", &name), ("seconds", &seconds)])
        }
    }
}

fn transition_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    let current = editor.slide().transition.clone();
    let mut items: Vec<MenuEntry> = vec![
        MenuItem::new("transition.none", "_shell.none").checked(current.as_ref().is_none_or(|t| t.effect.is_empty())).on(|this, _, cx| this.set_transition(None, cx)).into(),
    ];
    items.extend(TRANSITIONS.iter().map(|(effect, name, id)| {
        let on = current.as_ref().is_some_and(|t| t.effect == *effect);
        MenuItem::new(id, *name).checked(on).on(move |this, _, cx| this.set_transition(Some(effect), cx)).into()
    }));
    items.push(MenuEntry::Separator);
    for (ms, label, id) in [(500u32, "_shell.duration-0-5-s", "transition.duration.500"), (1000, "_shell.duration-1-s", "transition.duration.1000"), (2000, "_shell.duration-2-s", "transition.duration.2000")] {
        let on = current.as_ref().is_some_and(|t| t.duration == ms);
        items.push(MenuItem::new(id, label).checked(on).disabled(current.is_none()).on(move |this, _, cx| this.set_transition_duration(ms, cx)).into());
    }
    items.push(MenuEntry::Separator);
    items.push(MenuItem::new("transition.on-click", "_shell.on-mouse-click").checked(current.as_ref().is_none_or(|t| t.on_click)).on(|e, _, cx| e.toggle_transition_click(cx)).into());
    for (after, label, id) in [(None, "transition.after.none", "transition.after.none"), (Some(0), "transition.after.0", "transition.after.0"), (Some(1000), "transition.after.1000", "transition.after.1000"), (Some(5000), "transition.after.5000", "transition.after.5000"), (Some(10000), "transition.after.10000", "transition.after.10000")] {
        items.push(MenuItem::new(id, label).checked(current.as_ref().and_then(|t| t.after) == after).on(move |e, _, cx| e.set_transition_after(after, cx)).into());
    }
    items.push(MenuEntry::Separator);
    items.push(MenuItem::new("transition.timing", "_shell.timing-options").on(|e, w, cx| e.open_timing_panel(w, cx)).into());
    items.push(MenuItem::new("transition.all", "_shell.apply-to-all-slides").icon("done_all").on(|this, _, cx| this.apply_transition_to_all(cx)).into());
    items
}

fn outline_buttons(editor: &SlideEditor, line: bool) -> Vec<ToolItem> {
    let stroke = editor.selected_shapes().into_iter().find_map(|s| match &s.kind {
        ShapeKind::Auto(a) => a.stroke.clone(),
        ShapeKind::Picture(p) => p.stroke.clone(),
        ShapeKind::Line(l) => Some(l.clone()),
        _ => None,
    });
    let color = stroke.as_ref().map_or(rgb(0x444746).into(), |s| crate::render::shape_view::hsla(s.color));
    let (color_label, width_label, style_label) = if line { ("pptx.chrome.border-color", "pptx.chrome.line-weight", "pptx.chrome.border-style-2") } else { ("_shell.border-color", "pptx.chrome.border-weight", "pptx.chrome.border-style") };
    let base = |s: Option<Stroke>| s.unwrap_or_else(|| Stroke::solid(Color::BLACK, Stroke::DEFAULT_WIDTH));
    let widths: Vec<MenuEntry> = WIDTHS
        .iter()
        .map(|&pt| {
            let emu = (pt * 12_700.0).round() as i64;
            MenuItem::new(width_id(pt), format!("{pt} pt"))
                .checked(stroke.as_ref().is_some_and(|s| s.width == emu))
                .on(move |this, _, cx| this.stroke_selected(move |s| Some(Stroke { width: emu, ..base(s) }), cx))
                .into()
        })
        .collect();
    let dash_label = |d: ibis_doc_model::LineDash| match d {
        ibis_doc_model::LineDash::Solid => "pptx.chrome.solid",
        ibis_doc_model::LineDash::Dot => "pptx.chrome.dotted-round",
        ibis_doc_model::LineDash::Dash => "docx.picture.border.dash.dash",
        ibis_doc_model::LineDash::LgDash => "pptx.chrome.long-dash",
        ibis_doc_model::LineDash::DashDot => "docx.picture.border.dash.dashdot",
        ibis_doc_model::LineDash::LgDashDot => "pptx.chrome.long-dash-dot",
        ibis_doc_model::LineDash::LgDashDotDot => "_xlsx.dash-dot-dot",
        ibis_doc_model::LineDash::SysDash => "pptx.chrome.dashed-fine",
        ibis_doc_model::LineDash::SysDot => "pptx.chrome.dotted-square",
        ibis_doc_model::LineDash::SysDashDot => "pptx.chrome.dash-dot-fine",
        ibis_doc_model::LineDash::SysDashDotDot => "pptx.chrome.dash-dot-dot-fine",
    };
    let mut styles: Vec<MenuEntry> = ibis_doc_model::LineDash::ALL
        .into_iter()
        .map(|d| -> MenuEntry {
            let id = match d {
                ibis_doc_model::LineDash::Solid => "pptx.stroke-solid",
                ibis_doc_model::LineDash::Dash => "pptx.stroke-dash",
                other => other.name(),
            };
            MenuItem::new(id, dash_label(d)).checked(stroke.as_ref().is_some_and(|s| s.dash == d)).on(move |this, _, cx| this.stroke_selected(move |s| Some(base(s).with_dash(d)), cx)).into()
        })
        .collect();
    use ibis_doc_model::{LineCap, LineCompound};
    styles.push(MenuEntry::Separator);
    styles.push(Submenu::new("pptx.stroke-caps", "line_style", "_shell.cap-type", [
        ("cap.flat", "_shell.flat", LineCap::Flat), ("cap.round", "_shell.round", LineCap::Round), ("cap.square", "pptx.chrome.square", LineCap::Square)
    ].into_iter().map(|(id, label, cap)| MenuItem::new(id, label).checked(stroke.as_ref().is_some_and(|s| s.cap == cap)).on(move |e, _, cx| e.stroke_selected(move |s| Some(Stroke { cap, ..base(s) }), cx)).into()).collect()).into());
    styles.push(Submenu::new("pptx.stroke-compound", "line_style", "_shell.compound-type", [
        ("compound.single", "_shell.single", LineCompound::Single), ("compound.double", "_shell.double", LineCompound::Double),
        ("compound.thick-thin", "_shell.thick-thin", LineCompound::ThickThin), ("compound.thin-thick", "_shell.thin-thick", LineCompound::ThinThick), ("compound.triple", "docx.chrome.triple", LineCompound::Triple)
    ].into_iter().map(|(id, label, compound)| MenuItem::new(id, label).checked(stroke.as_ref().is_some_and(|s| s.compound == compound)).on(move |e, _, cx| e.stroke_selected(move |s| Some(Stroke { compound, ..base(s) }), cx)).into()).collect()).into());
    vec![
        ToolButton::new("border-color", "border_color", color_label)
            .color(color)
            .menu(color_menu(editor, "pptx.stroke-color", (!line).then_some("pptx.chrome.no-border"), move |this, rgb, _, cx| {
                this.stroke_selected(move |s| rgb.map(|rgb| Stroke { color: Color::opaque(rgb), ..base(s) }), cx)
            }))
            .into(),
        ToolButton::new("line-weight", "line_weight", width_label).menu(widths).into(),
        ToolButton::new("line-style", "line_style", style_label).menu(styles).into(),
    ]
}

fn width_id(pt: f32) -> &'static str {
    match (pt * 100.0).round() as i32 {
        25 => "width.0.25",
        50 => "width.0.5",
        75 => "width.0.75",
        100 => "width.1",
        150 => "width.1.5",
        225 => "width.2.25",
        300 => "width.3",
        450 => "width.4.5",
        _ => "width.6",
    }
}

fn end_size(end: ibis_doc_model::LineEnd) -> Option<ibis_doc_model::EndSize> {
    use ibis_doc_model::LineEnd::*;
    match end { None => std::option::Option::None, Triangle(s) | Arrow(s) | Stealth(s) | Diamond(s) | Oval(s) => Some(s) }
}

fn with_end_size(end: ibis_doc_model::LineEnd, size: ibis_doc_model::EndSize) -> ibis_doc_model::LineEnd {
    use ibis_doc_model::LineEnd::*;
    match end { None => None, Triangle(_) => Triangle(size), Arrow(_) => Arrow(size), Stealth(_) => Stealth(size), Diamond(_) => Diamond(size), Oval(_) => Oval(size) }
}

fn line_end_menu(editor: &SlideEditor, head: bool) -> Vec<MenuEntry> {
    use ibis_doc_model::{EndSize, LineEnd};
    let current = editor.selected_shapes().into_iter().find_map(|s| match &s.kind { ShapeKind::Line(s) => Some(if head { s.head } else { s.tail }), _ => None }).unwrap_or(LineEnd::None);
    let m = EndSize { width: 1, length: 1 };
    let ends = [("end.none", "_shell.none", LineEnd::None), ("end.triangle", "_shell.triangle", LineEnd::Triangle(m)), ("end.arrow", "_shell.open-arrow", LineEnd::Arrow(m)), ("end.stealth", "end.stealth", LineEnd::Stealth(m)), ("end.diamond", "_shell.diamond", LineEnd::Diamond(m)), ("end.oval", "pptx.common.circle", LineEnd::Oval(m))];
    let mut items: Vec<MenuEntry> = ends.into_iter().map(|(id, label, end)| {
        MenuItem::new(id, label).checked(std::mem::discriminant(&current) == std::mem::discriminant(&end)).on(move |e, _, cx| {
            e.stroke_selected(move |s| s.map(|s| {
                let old = if head { s.head } else { s.tail };
                let end = with_end_size(end, end_size(old).unwrap_or(m));
                if head { Stroke { head: end, ..s } } else { Stroke { tail: end, ..s } }
            }), cx)
        }).into()
    }).collect();
    for (width, id, label) in [(true, "end.width", "_shell.width"), (false, "end.length", "_shell.length")] {
        let sizes = [("small", "_shell.small", 0), ("medium", "_shell.medium", 1), ("large", "_shell.large", 2)].into_iter().map(|(suffix, name, value)| {
            let id = match (width, suffix) {
                (true, "small") => "end.width.small", (true, "medium") => "end.width.medium", (true, _) => "end.width.large",
                (false, "small") => "end.length.small", (false, "medium") => "end.length.medium", _ => "end.length.large",
            };
            MenuItem::new(id, name).disabled(end_size(current).is_none()).checked(end_size(current).is_some_and(|s| if width { s.width == value } else { s.length == value })).on(move |e, _, cx| {
                e.stroke_selected(move |s| s.map(|s| {
                    let old = if head { s.head } else { s.tail };
                    let Some(mut size) = end_size(old) else { return s };
                    if width { size.width = value; } else { size.length = value; }
                    let end = with_end_size(old, size);
                    if head { Stroke { head: end, ..s } } else { Stroke { tail: end, ..s } }
                }), cx)
            }).into()
        }).collect();
        items.push(Submenu::new(id, "line_end_arrow", label, sizes).into());
    }
    items
}

fn text_buttons(editor: &SlideEditor) -> Vec<ToolItem> {
    let style = editor.selected_text_style();
    let on = style.is_some();
    let font: SharedString = style.as_ref().and_then(|s| s.font.clone()).unwrap_or_else(|| crate::shell::i18n::text("pptx.chrome.theme-fonts").to_string()).into();
    let align = style.as_ref().map(|s| s.align);
    let align_item = |id: &'static str, icon: &'static str, label: &'static str, a: TextAlign| -> MenuEntry {
        MenuItem::new(id, label).icon(icon).checked(align == Some(a)).on(move |this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { align: Some(a), ..Default::default() })).into()
    };
    let first_run = editor.selected_shapes().into_iter().find_map(|s| s.text_body()).and_then(|b| b.paragraphs.iter().flat_map(|p| p.runs.first()).next().map(|r| r.style.clone()));
    let underline = first_run.as_ref().is_some_and(|r| r.underline);
    let strike = first_run.as_ref().is_some_and(|r| r.strike != ibis_doc_model::Strike::None);
    let list = editor.selected_list();
    let text_color = style.as_ref().map_or(rgb(0x000000).into(), |s| crate::render::shape_view::hsla(Color::opaque(s.color)));
    vec![
        sep(),
        ToolItem::Select { id: "font", value: font, width: 128.0, tooltip: "_shell.font".into(), menu: Vec::new(), disabled: !on },
        sep(),
        ToolItem::FontSize {
            value: style.as_ref().map(|s| format!("{}", s.font_size_pt()).into()),
            smaller: on.then(|| handler(|this, window, cx| this.text_style_selected(window, cx, |s| s.font_size = step_font_size(s.font_size, false)))),
            larger: on.then(|| handler(|this, window, cx| this.text_style_selected(window, cx, |s| s.font_size = step_font_size(s.font_size, true)))),
        },
        sep(),
        ToolButton::new("bold", "format_bold", "_shell.bold-b").on(style.as_ref().is_some_and(|s| s.bold)).disabled(!on).click(|this, window, cx| {
            this.toggle_characters(window, cx, |b, r| b.with_run(r).bold, |on| CharacterFormat { bold: Some(on), ..Default::default() })
        }).into(),
        ToolButton::new("italic", "format_italic", "_shell.italic-i").on(style.as_ref().is_some_and(|s| s.italic)).disabled(!on).click(|this, window, cx| {
            this.toggle_characters(window, cx, |b, r| b.with_run(r).italic, |on| CharacterFormat { italic: Some(on), ..Default::default() })
        }).into(),
        ToolButton::new("underline", "format_underlined", "_shell.underline-u").on(underline).disabled(!on).click(|this, window, cx| {
            this.toggle_characters(window, cx, |_, r| r.underline, |on| CharacterFormat { underline: Some(on), ..Default::default() })
        }).into(),
        ToolButton::new("strikethrough", "format_strikethrough", "pptx.chrome.strikethrough").on(strike).disabled(!on).click(|this, window, cx| this.strike_characters(window, cx)).into(),
        ToolButton::new("text-color", "format_color_text", "_shell.font-color")
            .color(text_color)
            .disabled(!on)
            .menu(color_menu(editor, "pptx.text-color", None, |this, rgb, window, cx| {
                if let Some(rgb) = rgb {
                    this.format_characters(window, cx, CharacterFormat { color: Some(rgb), ..Default::default() });
                }
            }))
            .into(),
        sep(),
        ToolButton::new("align", "format_align_left", "_shell.arrange")
            .disabled(!on)
            .menu(vec![
                align_item("align.left", "format_align_left", "align.left", TextAlign::Left),
                align_item("align.center", "format_align_center", "_shell.center-2", TextAlign::Center),
                align_item("align.right", "format_align_right", "align.right", TextAlign::Right),
            ])
            .into(),
        ToolButton::new("line-spacing", "format_line_spacing", "_shell.line-paragraph-spacing")
            .disabled(!on)
            .menu(
                [("spacing.1", "1.0", 100_000), ("spacing.1.15", "1.15", 115_000), ("spacing.1.5", "1.5", 150_000), ("spacing.2", "2.0", 200_000)]
                    .into_iter()
                    .map(|(id, label, v)| -> MenuEntry {
                        MenuItem::new(id, label).on(move |this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { line_spacing: Some(Some(LineSpacing::Percent(v))), ..Default::default() })).into()
                    })
                    .chain(std::iter::once(MenuItem::new("pptx.paragraph.settings", "_shell.line-and-paragraph-spacing-options").on(|e, w, cx| e.open_paragraph_panel(w, cx)).into()))
                    .collect(),
            )
            .into(),
        ToolButton::new("bullets", "format_list_bulleted", "_shell.bullets").on(list == Some(ListKind::Bullet)).disabled(!on).click(move |this, window, cx| {
            let kind = if list == Some(ListKind::Bullet) { ListKind::None } else { ListKind::Bullet };
            this.format_paragraphs(window, cx, ParagraphFormat { list: Some(kind), ..Default::default() })
        }).into(),
        ToolButton::new("numbered", "format_list_numbered", "_shell.numbering").on(list == Some(ListKind::Numbered)).disabled(!on).click(move |this, window, cx| {
            let kind = if list == Some(ListKind::Numbered) { ListKind::None } else { ListKind::Numbered };
            this.format_paragraphs(window, cx, ParagraphFormat { list: Some(kind), ..Default::default() })
        }).into(),
        ToolButton::new("indent-less", "format_indent_decrease", "_shell.decrease-indent").disabled(!on).click(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { indent_by: Some(-457_200), ..Default::default() })).into(),
        ToolButton::new("indent-more", "format_indent_increase", "_shell.increase-indent").disabled(!on).click(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { indent_by: Some(457_200), ..Default::default() })).into(),
        ToolButton::new("highlight", "ink_highlighter", "pptx.chrome.highlight-color")
            .disabled(!on)
            .menu(color_menu(editor, "pptx.highlight", Some("pptx.chrome.no-color"), |this, rgb, window, cx| {
                this.format_characters(window, cx, ibis_doc_model::CharacterFormat { highlight: Some(rgb), ..Default::default() })
            }))
            .into(),
        ToolButton::new("pptx-format-paint", "format_paint", "_shell.copy-paste-formatting").disabled(!on).menu(format_copy_items(editor)).into(),
        ToolButton::new("clear-format", "format_clear", "pptx.chrome.clear-all-formatting").disabled(!on).click(|this, window, cx| this.clear_formats(window, cx)).into(),
    ]
}

fn arrange_buttons(editor: &SlideEditor) -> Vec<ToolItem> {
    let order = |id: &'static str, icon: &'static str, label: &'static str, o: Order| -> MenuEntry { MenuItem::new(id, label).icon(icon).on(move |this, _, cx| this.order_selected(o, cx)).into() };
    let align = |id: &'static str, icon: &'static str, label: &'static str, a: Align| -> MenuEntry { MenuItem::new(id, label).icon(icon).on(move |this, _, cx| this.align_selected(a, cx)).into() };
    let _ = editor;
    vec![
        sep(),
        ToolButton::new("order", "flip_to_front", "pptx.chrome.order")
            .menu(vec![
                order("order.front", "flip_to_front", "_shell.bring-to-front", Order::Front),
                order("order.forward", "arrow_upward", "_shell.bring-forward", Order::Forward),
                order("order.backward", "arrow_downward", "_shell.send-backward", Order::Backward),
                order("order.back", "flip_to_back", "_shell.send-to-back", Order::Back),
            ])
            .into(),
        ToolButton::new("arrange", "align_horizontal_left", "_shell.arrange")
            .menu(vec![
                align("arrange.left", "align_horizontal_left", "align.left", Align::Left),
                align("arrange.center", "align_horizontal_center", "_shell.align-center", Align::Center),
                align("arrange.right", "align_horizontal_right", "align.right", Align::Right),
                align("arrange.top", "align_vertical_top", "_shell.align-top", Align::Top),
                align("arrange.middle", "align_vertical_center", "docx.picture.align.middle", Align::Middle),
                align("arrange.bottom", "align_vertical_bottom", "_shell.align-bottom", Align::Bottom),
            ])
            .into(),
    ]
}

pub(crate) fn toolbar_right(editor: &SlideEditor) -> Vec<ToolItem> {
    let tool = editor.pptx_tool();
    let mut items: Vec<ToolItem> = vec![
        ToolButton::new("tool-select", "arrow_selector_tool", "_shell.select-esc").on(tool == Tool::Select).click(|this, window, cx| this.set_pptx_tool(Tool::Select, window, cx)).into(),
        ToolButton::new("tool-text", "text_fields", "_shell.text-box").on(tool == Tool::TextBox).click(|this, window, cx| this.set_pptx_tool(Tool::TextBox, window, cx)).into(),
        ToolButton::new("insert-image", "image", "_shell.insert-picture").menu(image_items()).into(),
        ToolButton::new("insert-shape", "shapes", "_shell.shape").on(matches!(tool, Tool::Shape(_))).menu(shape_items()).into(),
        ToolButton::new("insert-line", "horizontal_rule", "_shell.line-2").on(matches!(tool, Tool::Line { .. })).menu(line_items()).into(),
    ];
    let fill_color = editor.selected_shapes().into_iter().find_map(|s| match &s.kind {
        ShapeKind::Auto(a) => match &a.fill {
            Fill::Solid(c) => Some(crate::render::shape_view::hsla(*c)),
            _ => None,
        },
        _ => None,
    });
    let mut fill_items = color_menu(editor, "pptx.fill", Some("_shell.no-fill"), |this, rgb, _, cx| {
        this.fill_selected(rgb.map_or(Fill::None, |rgb| Fill::Solid(Color::opaque(rgb))), cx)
    });
    fill_items.push(MenuEntry::Separator);
    fill_items.extend(fill_presets(false));
    let fill = ToolButton::new("fill", "format_color_fill", "pptx.common.fill")
        .color(fill_color.unwrap_or(crate::shell::palette::doc(0xffffff).into()))
        .menu(fill_items);
    let options = ToolButton::text("format-options", "_shell.format-options", "pptx.chrome.format-options-sidebar").click(|this, _, cx| {
        this.sidebar_open = true;
        cx.notify();
    });
    let motion = ToolButton::text("motion", "pptx.chrome.animations", "pptx.chrome.animations-motion").disabled(true);
    match editor.selection_kind() {
        SelectionKind::Shape => {
            items.push(sep());
            items.push(fill.into());
            items.extend(outline_buttons(editor, false));
            items.extend(text_buttons(editor));
            items.extend([sep(), options.into(), motion.into()]);
        }
        SelectionKind::Picture => {
            items.extend([sep(), ToolButton::new("crop", "crop", "pptx.chrome.crop-picture").menu(crop_items()).into(), ToolButton::new("reset-image", "reset_image", "pptx.chrome.reset-crop").click(|e, _, cx| e.crop_selected_pictures(ibis_doc_model::Crop::default(), cx)).into(), ToolButton::new("replace-image", "swap_horiz", "pptx.chrome.replace-picture").click(|e, w, cx| e.pick_replacement_picture(w, cx)).into(), sep()]);
            items.extend(outline_buttons(editor, false));
            items.extend([sep(), options.into(), motion.into()]);
            items.extend(arrange_buttons(editor));
        }
        SelectionKind::Line => {
            items.push(sep());
            items.extend(outline_buttons(editor, true));
            items.push(ToolButton::new("line-start", "line_start", "pptx.chrome.begin-arrow").menu(line_end_menu(editor, true)).into());
            items.push(ToolButton::new("line-end", "line_end_arrow_notch", "pptx.chrome.end-arrow").menu(line_end_menu(editor, false)).into());
            items.extend([sep(), options.into(), motion.into()]);
            items.extend(arrange_buttons(editor));
        }
        // A table cell being edited: its text's formats.
        SelectionKind::Table if editor.text_editing().is_some_and(|e| e.cell.is_some()) => {
            items.push(sep());
            items.extend(text_buttons(editor));
            items.extend([sep(), fill.clone().into()]);
            items.extend(outline_buttons(editor, false));
            items.extend([table_button(editor), options.into()]);
            items.extend(arrange_buttons(editor));
        }
        SelectionKind::Table => {
            items.extend([sep(), fill.clone().into()]);
            items.extend(outline_buttons(editor, false));
            items.extend([table_button(editor), options.into()]);
            items.extend(arrange_buttons(editor));
        }
        SelectionKind::Chart => {
            items.extend([sep(), options.into()]);
            items.extend(arrange_buttons(editor));
        }
        SelectionKind::Multi => {
            items.push(sep());
            items.push(fill.into());
            items.extend(outline_buttons(editor, false));
            items.extend([sep(), options.into(), motion.into()]);
            items.extend(arrange_buttons(editor));
        }
        SelectionKind::None => {
            items.extend([
                sep(),
                ToolButton::text("background", "_shell.background", "_shell.background").menu(background_menu()).into(),
                ToolButton::text("layout", "_shell.layout", "_shell.layout").menu(apply_layout_items(editor)).into(),
                ToolButton::text("theme", "_shell.theme", "_shell.theme").disabled(true).into(),
                ToolButton::text("transition", "_shell.transitions", "_shell.transitions").menu(transition_menu(editor)).into(),
            ]);
        }
    }
    items
}

fn field(label: &'static str, value: String) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .py(px(4.))
        .child(div().w(px(72.)).text_size(px(13.)).text_color(text_secondary()).child(crate::shell::i18n::text(label)))
        .child(div().flex_1().h(px(32.)).px(px(8.)).flex().items_center().border_1().border_color(rgb(0x747775)).rounded(px(4.)).text_size(px(14.)).child(value))
}

fn heading(text: impl Into<SharedString>) -> Div {
    div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).pt(px(10.)).pb(px(6.)).child(crate::shell::i18n::text(&text.into()))
}

/// The sidebar: 書式設定オプション for the selection, or the slide's settings.
pub(crate) fn sidebar(editor: &mut SlideEditor) -> Option<SidebarPanel> {
    let cm = |emu: i64| format!("{:.2} cm", emu as f64 / 360_000.0);
    let shapes = editor.selected_shapes();
    if let Some(shape) = shapes.first() {
        let mut body = div().px(px(18.)).py(px(8.)).p_0();
        if shapes.len() > 1 {
            body = body.child(div().text_size(px(13.)).text_color(text_secondary()).child(crate::shell::i18n::format("_xlsx.count-shapes-selected", &[("count", &shapes.len())])));
        }
        if let ShapeKind::Auto(auto) = &shape.kind {
            let fill = match &auto.fill {
                Fill::None => crate::shell::i18n::text("_shell.none").to_string(),
                Fill::Solid(c) => format!("#{:06X}", c.rgb.0),
                Fill::Linear { .. } | Fill::TileLinear { .. } | Fill::Path { .. } => crate::shell::i18n::text("pptx.chrome.gradient").to_string(),
                Fill::Image(_) | Fill::TileImage { .. } | Fill::Blip(_) => crate::shell::i18n::text("pptx.chrome.picture").to_string(),
                Fill::Pattern { preset, .. } => crate::shell::i18n::format("pptx.chrome.pattern-preset", &[("preset", preset)]),
            };
            let line = auto.stroke.as_ref().map_or(crate::shell::i18n::text("_shell.none").to_string(), |s| {
                let dashed = if s.dashed { crate::shell::i18n::text("pptx.chrome.dashed").to_string() } else { String::new() };
                crate::shell::i18n::format("_xlsx.hex-width-pt-dashed", &[("hex", &format!("{:06X}", s.color.rgb.0)), ("width", &(s.width as f64 / 12_700.0)), ("dashed", &dashed)])
            });
            body = body.child(heading("pptx.chrome.fill-line")).child(field("pptx.common.fill", fill)).child(field("_shell.line", line));
        }
        body = body
            .child(heading("_shell.size-rotation"))
            .child(field("_shell.width", cm(shape.frame.width)))
            .child(field("_shell.height", cm(shape.frame.height)))
            .child(field("_shell.rotate", format!("{}°", (shape.rotation as f64 / 60_000.0 * 10.0).round() / 10.0)))
            .child(heading("_shell.position"))
            .child(field("X", cm(shape.frame.x)))
            .child(field("Y", cm(shape.frame.y)));
        return Some(SidebarPanel { title: "format.options".into(), body: body.into_any_element() });
    }
    let slide = editor.slide();
    let index = editor.presentation().slide_index(editor.current_slide).unwrap_or(0) + 1;
    let layout = slide.layout.and_then(|l| editor.presentation().layout(l)).map(|l| l.name.clone()).unwrap_or_else(|| "—".into());
    let background = match &slide.background {
        Fill::None => crate::shell::i18n::text("_shell.none").to_string(),
        Fill::Solid(c) => format!("#{:06X}", c.rgb.0),
        Fill::Linear { .. } | Fill::TileLinear { .. } | Fill::Path { .. } => crate::shell::i18n::text("pptx.chrome.gradient").to_string(),
        Fill::Image(_) | Fill::TileImage { .. } | Fill::Blip(_) => crate::shell::i18n::text("pptx.chrome.picture").to_string(),
        Fill::Pattern { preset, .. } => crate::shell::i18n::format("pptx.chrome.pattern-preset", &[("preset", preset)]),
    };
    let size = editor.presentation().slide_size;
    let ratio = size.width as f64 / size.height.max(1) as f64;
    let kind = if (ratio - 16.0 / 9.0).abs() < 0.01 { "pptx.chrome.widescreen-16-9" } else if (ratio - 4.0 / 3.0).abs() < 0.01 { "pptx.chrome.standard-4-3" } else { crate::shell::i18n::text("_shell.custom") };
    let body = div()
        .px(px(18.))
        .py(px(8.))
        .p_0().child(heading(crate::shell::i18n::format("pptx.chrome.slide-index", &[("index", &index)])))
        .child(field("_shell.background", background))
        .child(field("_shell.layout", layout))
        .child(field("_shell.transitions", transition_name(editor.slide().transition.as_ref())))
        .child(heading("pptx.chrome.slide-size"))
        .child(field("_shell.size", crate::shell::i18n::text(kind).to_string()))
        .child(field("_shell.width", cm(size.width)))
        .child(field("_shell.height", cm(size.height)));
    Some(SidebarPanel { title: "menu.slide".into(), body: body.into_any_element() })
}
