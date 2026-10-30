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
use ibis_doc_model::{CharacterFormat, ChartType, Color, Fill, GradientPath, LineSpacing, ListKind, MediaStart, MediaVolume, ParagraphFormat, Rgb, ShapeKind, Stroke, TextAlign};

use super::editing::{Align, ArrowKind, CHANGE_SHAPE_IDS, COLORS, LineKind, Order, SHAPES, SelectionKind, TableOption, Tool, WIDTHS};
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
    let mut items: Vec<MenuEntry> = SHAPES
        .iter()
        .map(|&(preset, icon, id)| MenuItem::tr(id).icon(icon).on(move |this, window, cx| this.set_pptx_tool(Tool::Shape(preset), window, cx)).into())
        .collect();
    // アクション ボタン.
    items.push(MenuEntry::Separator);
    items.push(
        Submenu::tr(
            "pptx.action-buttons",
            "smart_button",
            super::editing::ACTION_BUTTONS.iter().map(|&(preset, id, _)| MenuItem::tr(id).on(move |this, window, cx| this.set_pptx_tool(Tool::Shape(preset), window, cx)).into()).collect(),
        )
        .into(),
    );
    items
}

fn text_body_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let body = editor.edited_body().or_else(|| editor.selected_shapes().into_iter().find_map(|s| s.text_body()));
    let on = body.is_some();
    let anchor = body.map(|b| b.anchor);
    let mut items: Vec<MenuEntry> = [("pptx.text.top", ibis_doc_model::VerticalAnchor::Top), ("pptx.text.middle", ibis_doc_model::VerticalAnchor::Middle), ("pptx.text.bottom", ibis_doc_model::VerticalAnchor::Bottom)].into_iter().map(|(id, value)| {
        MenuItem::tr(id).disabled(!on).checked(anchor == Some(value)).on(move |e, _, cx| e.format_text_body(|b| b.anchor = value, cx)).into()
    }).collect();
    items.push(MenuEntry::Separator);
    for (id, vertical, sideways) in [("pptx.text.horizontal", false, 0), ("pptx.text.vertical", true, 0), ("pptx.text.clockwise", false, 5_400_000), ("pptx.text.counterclockwise", false, 16_200_000)] {
        items.push(MenuItem::tr(id).disabled(!on).checked(body.is_some_and(|b| b.vertical == vertical && (!vertical || b.vertical_kind == ibis_doc_model::VerticalKind::EastAsian) && b.sideways == sideways)).on(move |e, _, cx| e.format_text_body(|b| { b.vertical = vertical; b.vertical_kind = ibis_doc_model::VerticalKind::EastAsian; b.sideways = sideways; }, cx)).into());
    }
    items.push(MenuEntry::Separator);
    for (id,scale) in [("pptx.text.scale100",100_000),("pptx.text.scale80",80_000),("pptx.text.scale50",50_000)] {
        items.push(MenuItem::tr(id).disabled(!on).checked(body.is_some_and(|b|b.font_scale==scale)).on(move |e,_,cx|e.set_text_font_scale(scale,cx)).into());
    }
    items.push(MenuEntry::Separator);
    let autofit = body.map(|b| b.autofit);
    for (id, fit) in [
        ("pptx.text.autofit-none", ibis_doc_model::TextAutofit::None),
        ("pptx.text.autofit-shrink", ibis_doc_model::TextAutofit::Shrink),
        ("pptx.text.autofit-resize-shape", ibis_doc_model::TextAutofit::ResizeShape),
    ] {
        items.push(MenuItem::tr(id).disabled(!on).checked(autofit == Some(fit)).on(move |e, _, cx| e.set_text_autofit(fit, cx)).into());
    }
    items.push(MenuItem::tr("pptx.text.wrap").disabled(!on).checked(body.is_some_and(|b| b.wrap)).on(|e, _, cx| e.format_text_body(|b| b.wrap = !b.wrap, cx)).into());
    items.push(MenuItem::tr("pptx.text.insets-none").disabled(!on).on(|e, _, cx| e.format_text_body(|b| b.insets = ibis_doc_model::Insets { left: 0, top: 0, right: 0, bottom: 0 }, cx)).into());
    items.push(MenuItem::tr("pptx.text.insets-default").disabled(!on).on(|e, _, cx| e.format_text_body(|b| b.insets = ibis_doc_model::Insets::default(), cx)).into());
    items.push(MenuItem::tr("pptx.text.insets-custom").disabled(!on).on(|e, w, cx| e.open_text_insets_panel(w, cx)).into());
    items.push(MenuEntry::Separator);
    let current_cols = body.map(|b| b.columns).unwrap_or(1);
    let current_spc = body.map(|b| b.column_spacing).unwrap_or(0);
    items.push(Submenu::tr("pptx.text.columns", "view_column", vec![
        MenuItem::tr("pptx.text.columns.1").disabled(!on).checked(current_cols == 1).on(|e, _, cx| e.set_text_columns(1, 0, cx)).into(),
        MenuItem::tr("pptx.text.columns.2").disabled(!on).checked(current_cols == 2).on(|e, _, cx| e.set_text_columns(2, 180_000, cx)).into(),
        MenuItem::tr("pptx.text.columns.3").disabled(!on).checked(current_cols == 3).on(|e, _, cx| e.set_text_columns(3, 180_000, cx)).into(),
        MenuItem::tr("pptx.text.columns.4").disabled(!on).checked(current_cols == 4).on(|e, _, cx| e.set_text_columns(4, 180_000, cx)).into(),
        MenuEntry::Separator,
        Submenu::tr("pptx.text.columns.spacing", "space_bar", [
            ("pptx.text.columns.spacing.0", 0),
            ("pptx.text.columns.spacing.0-5cm", 180_000),
            ("pptx.text.columns.spacing.1cm", 360_000),
            ("pptx.text.columns.spacing.2cm", 720_000),
        ].into_iter().map(|(id, spc)| {
            MenuItem::tr(id).disabled(!on || current_cols <= 1).checked(current_spc == spc).on(move |e, _, cx| {
                let cols = e.edited_body().or_else(|| e.selected_shapes().into_iter().find_map(|s| s.text_body())).map(|b| b.columns).unwrap_or(1);
                e.set_text_columns(cols, spc, cx);
            }).into()
        }).collect()).into(),
    ]).into());
    let first_p = body.and_then(|b| b.paragraphs.first());
    let current_ml = first_p.map(|p| p.margin_left).unwrap_or(0);
    let current_ind = first_p.map(|p| p.indent).unwrap_or(0);
    let has_tabs = first_p.is_some_and(|p| !p.tabs.is_empty());
    items.push(Submenu::tr("pptx.text.indent", "format_indent_increase", vec![
        MenuItem::tr("pptx.text.indent.first-line").disabled(!on).checked(current_ind > 0).on(|e, w, cx| e.set_paragraph_indent(None, Some(228_600), w, cx)).into(),
        MenuItem::tr("pptx.text.indent.hanging").disabled(!on).checked(current_ind < 0).on(|e, w, cx| e.set_paragraph_indent(Some(228_600), Some(-228_600), w, cx)).into(),
        MenuItem::tr("pptx.text.indent.none").disabled(!on).checked(current_ind == 0 && current_ml == 0).on(|e, w, cx| e.set_paragraph_indent(Some(0), Some(0), w, cx)).into(),
    ]).into());
    items.push(Submenu::tr("pptx.text.tabs", "keyboard_tab", vec![
        MenuItem::tr("pptx.text.tabs.default").disabled(!on).checked(has_tabs).on(|e, w, cx| e.set_paragraph_tabs(vec![
            ibis_doc_model::TextTabStop { position: 720_000, align: ibis_doc_model::TextTabAlign::Left },
            ibis_doc_model::TextTabStop { position: 1_440_000, align: ibis_doc_model::TextTabAlign::Left },
        ], w, cx)).into(),
        MenuItem::tr("pptx.text.tabs.clear").disabled(!on).checked(!has_tabs).on(|e, w, cx| e.set_paragraph_tabs(Vec::new(), w, cx)).into(),
    ]).into());
    items
}

fn format_copy_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let text = editor.edited_body().is_some() || editor.selected_shapes().into_iter().any(|s| s.text_body().is_some());
    vec![
        MenuItem::tr("pptx.paragraph-copy").disabled(!text).on(|e, _, cx| e.copy_paragraph_format(cx)).into(),
        MenuItem::tr("pptx.paragraph-paste").disabled(!text || editor.pptx().is_none_or(|s| s.paragraph_format_copy.is_none())).on(|e, w, cx| e.paste_paragraph_format(w, cx)).into(),
        MenuItem::tr("pptx.format-copy").shortcut("⌥⌘C").disabled(!text).on(|e, w, cx| e.copy_character_format(w, cx)).into(),
        MenuItem::tr("pptx.format-paste").shortcut("⌥⌘V").disabled(!text || editor.pptx().is_none_or(|s| s.character_format_copy.is_none())).on(|e, w, cx| e.paste_character_format(w, cx)).into(),
    ]
}

pub(crate) fn crop_items() -> Vec<MenuEntry> {
    use ibis_doc_model::Crop;
    vec![
        MenuItem::tr("pptx.crop.left").on(|e, _, cx| e.crop_selected_pictures(Crop { left: 10_000, ..Default::default() }, cx)).into(),
        MenuItem::tr("pptx.crop.right").on(|e, _, cx| e.crop_selected_pictures(Crop { right: 10_000, ..Default::default() }, cx)).into(),
        MenuItem::tr("pptx.crop.top").on(|e, _, cx| e.crop_selected_pictures(Crop { top: 10_000, ..Default::default() }, cx)).into(),
        MenuItem::tr("pptx.crop.bottom").on(|e, _, cx| e.crop_selected_pictures(Crop { bottom: 10_000, ..Default::default() }, cx)).into(),
        MenuItem::tr("pptx.crop.center").on(|e, _, cx| e.crop_selected_pictures(Crop { left: 10_000, top: 10_000, right: 10_000, bottom: 10_000 }, cx)).into(),
        MenuEntry::Separator,
        Submenu::tr("pptx.crop.aspect", "crop", vec![
            MenuItem::tr("pptx.crop.aspect.1-1").on(|e, _, cx| e.crop_selected_pictures_aspect(1, 1, cx)).into(),
            MenuEntry::Separator,
            Submenu::tr("pptx.crop.aspect.portrait", "crop", vec![
                MenuItem::tr("pptx.crop.aspect.2-3").on(|e, _, cx| e.crop_selected_pictures_aspect(2, 3, cx)).into(),
                MenuItem::tr("pptx.crop.aspect.3-4").on(|e, _, cx| e.crop_selected_pictures_aspect(3, 4, cx)).into(),
                MenuItem::tr("pptx.crop.aspect.3-5").on(|e, _, cx| e.crop_selected_pictures_aspect(3, 5, cx)).into(),
                MenuItem::tr("pptx.crop.aspect.4-5").on(|e, _, cx| e.crop_selected_pictures_aspect(4, 5, cx)).into(),
                MenuItem::tr("pptx.crop.aspect.9-16").on(|e, _, cx| e.crop_selected_pictures_aspect(9, 16, cx)).into(),
            ]).into(),
            Submenu::tr("pptx.crop.aspect.landscape", "crop", vec![
                MenuItem::tr("pptx.crop.aspect.3-2").on(|e, _, cx| e.crop_selected_pictures_aspect(3, 2, cx)).into(),
                MenuItem::tr("pptx.crop.aspect.4-3").on(|e, _, cx| e.crop_selected_pictures_aspect(4, 3, cx)).into(),
                MenuItem::tr("pptx.crop.aspect.5-3").on(|e, _, cx| e.crop_selected_pictures_aspect(5, 3, cx)).into(),
                MenuItem::tr("pptx.crop.aspect.5-4").on(|e, _, cx| e.crop_selected_pictures_aspect(5, 4, cx)).into(),
                MenuItem::tr("pptx.crop.aspect.16-9").on(|e, _, cx| e.crop_selected_pictures_aspect(16, 9, cx)).into(),
                MenuItem::tr("pptx.crop.aspect.16-10").on(|e, _, cx| e.crop_selected_pictures_aspect(16, 10, cx)).into(),
            ]).into(),
        ]).into(),
        Submenu::tr("pptx.crop.shape", "crop", vec![
            MenuItem::tr("pptx.crop.shape.rect").on(|e, _, cx| e.crop_selected_pictures_corner(0, cx)).into(),
            MenuItem::tr("pptx.crop.shape.round-small").on(|e, _, cx| e.crop_selected_pictures_corner(10_000, cx)).into(),
            MenuItem::tr("pptx.crop.shape.round-medium").on(|e, _, cx| e.crop_selected_pictures_corner(20_000, cx)).into(),
            MenuItem::tr("pptx.crop.shape.round-large").on(|e, _, cx| e.crop_selected_pictures_corner(33_333, cx)).into(),
            MenuItem::tr("pptx.crop.shape.round-full").on(|e, _, cx| e.crop_selected_pictures_corner(50_000, cx)).into(),
        ]).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.crop.custom").on(|e, w, cx| e.open_crop_panel(w, cx)).into(),
    ]
}

pub(crate) fn picture_correction_items(editor: &SlideEditor, enabled: bool) -> Vec<MenuEntry> {
    let current = editor.selected_shapes().into_iter().find_map(|s| match &s.kind {
        ShapeKind::Picture(p) => Some(p.luminance),
        _ => None,
    }).flatten();

    let preset = |id: &'static str, lum: Option<(i32, i32)>| -> MenuEntry {
        let checked = current == lum || (current.is_none() && lum == Some((0, 0)));
        MenuItem::tr(id)
            .disabled(!enabled)
            .checked(checked)
            .on(move |e, _, cx| e.set_selected_pictures_luminance(lum, cx))
            .into()
    };

    vec![
        preset("pptx.picture.lum.normal", None),
        MenuEntry::Separator,
        preset("pptx.picture.lum.bright-p40", Some((40_000, 0))),
        preset("pptx.picture.lum.bright-p20", Some((20_000, 0))),
        preset("pptx.picture.lum.bright-m20", Some((-20_000, 0))),
        preset("pptx.picture.lum.bright-m40", Some((-40_000, 0))),
        MenuEntry::Separator,
        preset("pptx.picture.lum.contrast-p40", Some((0, 40_000))),
        preset("pptx.picture.lum.contrast-p20", Some((0, 20_000))),
        preset("pptx.picture.lum.contrast-m20", Some((0, -20_000))),
        preset("pptx.picture.lum.contrast-m40", Some((0, -40_000))),
        MenuEntry::Separator,
        preset("pptx.picture.lum.bright-contrast-high", Some((20_000, 20_000))),
        preset("pptx.picture.lum.bright-contrast-low", Some((-20_000, -20_000))),
    ]
}

fn wordart_insert_items() -> Vec<MenuEntry> {
    use crate::pptx::editing::WORDART_PRESETS;
    WORDART_PRESETS.iter().enumerate().map(|(idx, preset)| {
        MenuItem::new(preset.insert_id, crate::shell::i18n::text(preset.id)).on(move |e, _, cx| {
            e.insert_wordart(idx, None, cx);
        }).into()
    }).collect()
}

pub(crate) fn smartart_insert_items() -> Vec<MenuEntry> {
    vec![
        MenuItem::tr("pptx.insert.smartart.list").icon("view_list").on(|e, _, cx| e.insert_smartart(crate::pptx::SmartArtKind::BasicBlockList, cx)).into(),
        MenuItem::tr("pptx.insert.smartart.vlist").icon("format_list_bulleted").on(|e, _, cx| e.insert_smartart(crate::pptx::SmartArtKind::VerticalList, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.insert.smartart.process").icon("arrow_forward").on(|e, _, cx| e.insert_smartart(crate::pptx::SmartArtKind::BasicProcess, cx)).into(),
        MenuItem::tr("pptx.insert.smartart.chevron").icon("double_arrow").on(|e, _, cx| e.insert_smartart(crate::pptx::SmartArtKind::ContinuousChevronProcess, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.insert.smartart.cycle").icon("autorenew").on(|e, _, cx| e.insert_smartart(crate::pptx::SmartArtKind::BasicCycle, cx)).into(),
        MenuItem::tr("pptx.insert.smartart.hierarchy").icon("account_tree").on(|e, _, cx| e.insert_smartart(crate::pptx::SmartArtKind::OrganizationChart, cx)).into(),
        MenuItem::tr("pptx.insert.smartart.relationship").icon("sync_alt").on(|e, _, cx| e.insert_smartart(crate::pptx::SmartArtKind::BasicRelationship, cx)).into(),
        MenuItem::tr("pptx.insert.smartart.matrix").icon("grid_view").on(|e, _, cx| e.insert_smartart(crate::pptx::SmartArtKind::BasicMatrix, cx)).into(),
        MenuItem::tr("pptx.insert.smartart.pyramid").icon("change_history").on(|e, _, cx| e.insert_smartart(crate::pptx::SmartArtKind::BasicPyramid, cx)).into(),
        MenuItem::tr("pptx.insert.smartart.picture").icon("insert_photo").on(|e, _, cx| e.insert_smartart(crate::pptx::SmartArtKind::PictureList, cx)).into(),
    ]
}

pub(crate) fn text_warp_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    use crate::pptx::editing::TEXT_WARP_PRESETS;
    let current_warp = editor.selected_text_warp();
    let has_text = editor.selected_text_style().is_some() || editor.text_editing().is_some();
    let mut items: Vec<MenuEntry> = Vec::new();
    items.push(MenuItem::tr("pptx.text.warp.none").disabled(!has_text).checked(current_warp.is_none()).on(|e, _, cx| e.set_text_warp(None, cx)).into());
    items.push(MenuEntry::Separator);
    for &(prst, id, _name) in TEXT_WARP_PRESETS {
        let is_checked = current_warp.as_deref() == Some(prst);
        items.push(MenuItem::tr(id).disabled(!has_text).checked(is_checked).on(move |e, _, cx| e.set_text_warp(Some(prst), cx)).into());
    }
    items
}

pub(crate) fn wordart_style_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    use crate::pptx::editing::WORDART_PRESETS;
    let has_text = editor.selected_text_style().is_some() || editor.text_editing().is_some();
    WORDART_PRESETS.iter().enumerate().map(|(idx, preset)| {
        MenuItem::new(preset.apply_id, crate::shell::i18n::text(preset.id)).disabled(!has_text).on(move |e, w, cx| {
            e.set_selected_text_wordart_style(idx, w, cx);
        }).into()
    }).collect()
}

pub(crate) fn picture_color_items(enabled: bool) -> Vec<MenuEntry> {
    vec![
        MenuItem::tr("pptx.picture.color.none").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_duotone(None, cx)).into(),
        MenuItem::tr("pptx.picture.color.grayscale").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x000000), ibis_doc_model::Rgb(0xffffff))), cx)).into(),
        MenuItem::tr("pptx.picture.color.sepia").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x402000), ibis_doc_model::Rgb(0xffe0c0))), cx)).into(),
        MenuItem::tr("pptx.picture.color.blue").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x002060), ibis_doc_model::Rgb(0xd9e1f2))), cx)).into(),
        MenuItem::tr("pptx.picture.color.green").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x004010), ibis_doc_model::Rgb(0xe2efda))), cx)).into(),
        MenuItem::tr("pptx.picture.color.red").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_duotone(Some((ibis_doc_model::Rgb(0x600000), ibis_doc_model::Rgb(0xfce4d6))), cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.picture.color.transparent-white").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_clear(Some(ibis_doc_model::Rgb(0xffffff)), cx)).into(),
        MenuItem::tr("pptx.picture.color.transparent-black").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_clear(Some(ibis_doc_model::Rgb(0x000000)), cx)).into(),
        MenuItem::tr("pptx.picture.color.transparent-clear").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_clear(None, cx)).into(),
    ]
}

pub(crate) fn picture_artistic_effect_items(enabled: bool) -> Vec<MenuEntry> {
    use ibis_doc_model::ArtisticEffect as A;
    vec![
        MenuItem::tr("pptx.picture.artistic.none").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(None, cx)).into(),
        MenuItem::tr("pptx.picture.artistic.marker").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::marker_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.chalk-sketch").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::chalk_sketch_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.crisscross-etching").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::crisscross_etching_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.pencil-grayscale").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::pencil_grayscale_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.pencil-sketch").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::pencil_sketch_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.line-drawing").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::line_drawing_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.watercolor-sponge").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::watercolor_sponge_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.paint-brush").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::paint_brush_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.paint-strokes").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::paint_strokes_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.blur").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::blur_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.glow-diffused").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::glow_diffused_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.glow-edges").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::glow_edges_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.glass").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::glass_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.pastels-smooth").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::pastels_smooth_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.plastic-wrap").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::plastic_wrap_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.mosaic-bubbles").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::mosaic_bubbles_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.texturizer").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::texturizer_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.film-grain").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::film_grain_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.cutout").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::cutout_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.photocopy").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::photocopy_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.cement").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::cement_default()), cx)).into(),
        MenuItem::tr("pptx.picture.artistic.light-screen").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_artistic_effect(Some(A::light_screen_default()), cx)).into(),
    ]
}

pub(crate) fn clip_playback_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let clip_shape = editor.selected_shapes().into_iter().find(|s| SlideEditor::clip_of(s).is_some());
    let has_clip = clip_shape.is_some();
    let opt = editor.selected_clip_options().unwrap_or_default();
    let shape_id = clip_shape.map(|s| s.id);
    let is_audio = clip_shape.and_then(|s| SlideEditor::clip_of(s)).is_some_and(|c| c.audio);

    vec![
        MenuItem::tr("pptx.clip.play")
            .disabled(!has_clip)
            .icon("play_arrow")
            .on(move |e, _, cx| {
                if let Some(id) = shape_id {
                    e.toggle_clip(id, cx);
                }
            })
            .into(),
        Submenu::tr("pptx.clip.start", "play_circle", vec![
            MenuItem::tr("pptx.clip.start.on-click")
                .disabled(!has_clip)
                .checked(opt.start == MediaStart::OnClick)
                .on(|e, _, cx| e.set_selected_clip_start(MediaStart::OnClick, cx))
                .into(),
            MenuItem::tr("pptx.clip.start.auto")
                .disabled(!has_clip)
                .checked(opt.start == MediaStart::Automatically)
                .on(|e, _, cx| e.set_selected_clip_start(MediaStart::Automatically, cx))
                .into(),
        ]).into(),
        Submenu::tr("pptx.clip.volume", "volume_up", vec![
            MenuItem::tr("pptx.clip.volume.high")
                .disabled(!has_clip)
                .checked(opt.volume == MediaVolume::High)
                .on(|e, _, cx| e.set_selected_clip_volume(MediaVolume::High, cx))
                .into(),
            MenuItem::tr("pptx.clip.volume.medium")
                .disabled(!has_clip)
                .checked(opt.volume == MediaVolume::Medium)
                .on(|e, _, cx| e.set_selected_clip_volume(MediaVolume::Medium, cx))
                .into(),
            MenuItem::tr("pptx.clip.volume.low")
                .disabled(!has_clip)
                .checked(opt.volume == MediaVolume::Low)
                .on(|e, _, cx| e.set_selected_clip_volume(MediaVolume::Low, cx))
                .into(),
            MenuItem::tr("pptx.clip.volume.mute")
                .disabled(!has_clip)
                .checked(opt.volume == MediaVolume::Mute)
                .on(|e, _, cx| e.set_selected_clip_volume(MediaVolume::Mute, cx))
                .into(),
        ]).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.clip.loop")
            .disabled(!has_clip)
            .checked(opt.loop_until_stopped)
            .on(|e, _, cx| e.toggle_selected_clip_loop(cx))
            .into(),
        MenuItem::tr("pptx.clip.rewind")
            .disabled(!has_clip)
            .checked(opt.rewind)
            .on(|e, _, cx| e.toggle_selected_clip_rewind(cx))
            .into(),
        MenuItem::tr("pptx.clip.fullscreen")
            .disabled(!has_clip || is_audio)
            .checked(opt.full_screen)
            .on(|e, _, cx| e.toggle_selected_clip_fullscreen(cx))
            .into(),
        MenuItem::tr("pptx.clip.hide-stopped")
            .disabled(!has_clip || is_audio)
            .checked(opt.hide_while_not_playing)
            .on(|e, _, cx| e.toggle_selected_clip_hide_stopped(cx))
            .into(),
        MenuItem::tr("pptx.clip.hide-show")
            .disabled(!has_clip || !is_audio)
            .checked(opt.hide_during_show)
            .on(|e, _, cx| e.toggle_selected_clip_hide_show(cx))
            .into(),
        MenuItem::tr("pptx.clip.play-across")
            .disabled(!has_clip || !is_audio)
            .checked(opt.play_across_slides)
            .on(|e, _, cx| e.toggle_selected_clip_play_across(cx))
            .into(),
        MenuItem::tr("pptx.clip.background")
            .disabled(!has_clip || !is_audio)
            .checked(opt.is_background_play())
            .on(|e, _, cx| e.toggle_selected_clip_background(cx))
            .into(),
        Submenu::tr("pptx.clip.poster", "image", vec![
            MenuItem::tr("pptx.clip.poster.from-file")
                .disabled(!has_clip || is_audio)
                .on(|e, w, cx| e.pick_clip_poster_image(w, cx))
                .into(),
            MenuItem::tr("pptx.clip.poster.reset")
                .disabled(!has_clip || is_audio)
                .on(|e, _, cx| e.reset_selected_clip_poster_image(cx))
                .into(),
        ]).into(),
        MenuEntry::Separator,
        Submenu::tr("pptx.clip.trim", "content_cut", vec![
            MenuItem::tr("pptx.clip.trim.none")
                .disabled(!has_clip)
                .checked(!opt.is_trimmed())
                .on(|e, _, cx| e.set_selected_clip_trim(0, 0, cx))
                .into(),
            MenuItem::tr("pptx.clip.trim.start-1s")
                .disabled(!has_clip)
                .checked(opt.trim_start_ms == 1000 && opt.trim_end_ms == 0)
                .on(|e, _, cx| e.set_selected_clip_trim(1000, 0, cx))
                .into(),
            MenuItem::tr("pptx.clip.trim.start-2s")
                .disabled(!has_clip)
                .checked(opt.trim_start_ms == 2000 && opt.trim_end_ms == 0)
                .on(|e, _, cx| e.set_selected_clip_trim(2000, 0, cx))
                .into(),
            MenuItem::tr("pptx.clip.trim.start-5s")
                .disabled(!has_clip)
                .checked(opt.trim_start_ms == 5000 && opt.trim_end_ms == 0)
                .on(|e, _, cx| e.set_selected_clip_trim(5000, 0, cx))
                .into(),
            MenuItem::tr("pptx.clip.trim.first-5s")
                .disabled(!has_clip)
                .checked(opt.trim_start_ms == 0 && opt.trim_end_ms == 5000)
                .on(|e, _, cx| e.set_selected_clip_trim(0, 5000, cx))
                .into(),
            MenuItem::tr("pptx.clip.trim.first-10s")
                .disabled(!has_clip)
                .checked(opt.trim_start_ms == 0 && opt.trim_end_ms == 10000)
                .on(|e, _, cx| e.set_selected_clip_trim(0, 10000, cx))
                .into(),
        ]).into(),
        Submenu::tr("pptx.clip.fade-in", "tune", vec![
            MenuItem::tr("pptx.clip.fade.none")
                .disabled(!has_clip)
                .checked(opt.fade_in_ms == 0)
                .on(|e, _, cx| e.set_selected_clip_fade_in(0, cx))
                .into(),
            MenuItem::tr("pptx.clip.fade.05s")
                .disabled(!has_clip)
                .checked(opt.fade_in_ms == 500)
                .on(|e, _, cx| e.set_selected_clip_fade_in(500, cx))
                .into(),
            MenuItem::tr("pptx.clip.fade.1s")
                .disabled(!has_clip)
                .checked(opt.fade_in_ms == 1000)
                .on(|e, _, cx| e.set_selected_clip_fade_in(1000, cx))
                .into(),
            MenuItem::tr("pptx.clip.fade.2s")
                .disabled(!has_clip)
                .checked(opt.fade_in_ms == 2000)
                .on(|e, _, cx| e.set_selected_clip_fade_in(2000, cx))
                .into(),
        ]).into(),
        Submenu::tr("pptx.clip.fade-out", "tune", vec![
            MenuItem::tr("pptx.clip.fade.none")
                .disabled(!has_clip)
                .checked(opt.fade_out_ms == 0)
                .on(|e, _, cx| e.set_selected_clip_fade_out(0, cx))
                .into(),
            MenuItem::tr("pptx.clip.fade.05s")
                .disabled(!has_clip)
                .checked(opt.fade_out_ms == 500)
                .on(|e, _, cx| e.set_selected_clip_fade_out(500, cx))
                .into(),
            MenuItem::tr("pptx.clip.fade.1s")
                .disabled(!has_clip)
                .checked(opt.fade_out_ms == 1000)
                .on(|e, _, cx| e.set_selected_clip_fade_out(1000, cx))
                .into(),
            MenuItem::tr("pptx.clip.fade.2s")
                .disabled(!has_clip)
                .checked(opt.fade_out_ms == 2000)
                .on(|e, _, cx| e.set_selected_clip_fade_out(2000, cx))
                .into(),
        ]).into(),
        Submenu::tr("pptx.clip.bookmarks", "bookmark", {
            let mut items = vec![
                MenuItem::tr("pptx.clip.bookmark.add")
                    .disabled(!has_clip)
                    .icon("add")
                    .on(|e, _, cx| e.add_selected_clip_bookmark(None, None, cx))
                    .into(),
                MenuItem::tr("pptx.clip.bookmark.clear")
                    .disabled(!has_clip || opt.bookmarks.is_empty())
                    .icon("clear_all")
                    .on(|e, _, cx| e.clear_selected_clip_bookmarks(cx))
                    .into(),
            ];
            if !opt.bookmarks.is_empty() {
                items.push(MenuEntry::Separator);
                for (i, bmk) in opt.bookmarks.iter().enumerate() {
                    let secs = bmk.time_ms as f32 / 1000.0;
                    let label = format!("{} ({:.1}s)", bmk.name, secs);
                    items.push(
                        MenuItem::new("pptx.clip.bookmark-jump", label)
                            .icon("bookmark")
                            .on(move |e, _, cx| e.seek_selected_clip_to_bookmark(i, cx))
                            .into(),
                    );
                }
            }
            items
        }).into(),
    ]
}

pub(crate) fn video_style_items(_editor: &SlideEditor, enabled: bool) -> Vec<MenuEntry> {
    use super::editing::VideoStylePreset;
    vec![
        MenuItem::tr("pptx.video.style.none").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_video_style(VideoStylePreset::None, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.video.style.simple-white").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_video_style(VideoStylePreset::SimpleFrameWhite, cx)).into(),
        MenuItem::tr("pptx.video.style.simple-black").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_video_style(VideoStylePreset::SimpleFrameBlack, cx)).into(),
        MenuItem::tr("pptx.video.style.rounded").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_video_style(VideoStylePreset::Rounded, cx)).into(),
        MenuItem::tr("pptx.video.style.rounded-frame").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_video_style(VideoStylePreset::RoundedFrame, cx)).into(),
        MenuItem::tr("pptx.video.style.shadow").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_video_style(VideoStylePreset::DropShadow, cx)).into(),
        MenuItem::tr("pptx.video.style.reflection").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_video_style(VideoStylePreset::Reflection, cx)).into(),
        MenuItem::tr("pptx.video.style.rounded-reflection").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_video_style(VideoStylePreset::RoundedReflection, cx)).into(),
        MenuItem::tr("pptx.video.style.rounded-shadow").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_video_style(VideoStylePreset::RoundedShadow, cx)).into(),
        MenuItem::tr("pptx.video.style.soft-edge").disabled(!enabled).on(|e, _, cx| e.set_selected_pictures_video_style(VideoStylePreset::SoftEdge, cx)).into(),
    ]
}

pub(crate) fn video_shape_items(_editor: &SlideEditor, enabled: bool) -> Vec<MenuEntry> {
    vec![
        MenuItem::tr("pptx.crop.shape.rect").disabled(!enabled).on(|e, _, cx| e.crop_selected_pictures_corner(0, cx)).into(),
        MenuItem::tr("pptx.crop.shape.round-small").disabled(!enabled).on(|e, _, cx| e.crop_selected_pictures_corner(10_000, cx)).into(),
        MenuItem::tr("pptx.crop.shape.round-medium").disabled(!enabled).on(|e, _, cx| e.crop_selected_pictures_corner(20_000, cx)).into(),
        MenuItem::tr("pptx.crop.shape.round-large").disabled(!enabled).on(|e, _, cx| e.crop_selected_pictures_corner(33_333, cx)).into(),
        MenuItem::tr("pptx.crop.shape.round-full").disabled(!enabled).on(|e, _, cx| e.crop_selected_pictures_corner(50_000, cx)).into(),
    ]
}

fn video_effects_items(editor: &SlideEditor, enabled: bool) -> Vec<MenuEntry> {
    vec![
        Submenu::tr("pptx.shadow", "format_color_fill", [
            ("pptx.shadow.none", None),
            ("pptx.shadow.right", Some((0, 0))),
            ("pptx.shadow.diagonal", Some((2_700_000, 0))),
            ("pptx.shadow.soft", Some((2_700_000, 50_800))),
        ].into_iter().map(|(id, value)| {
            let shadow = value.map(|(direction, blur)| ibis_doc_model::Shadow::simple(ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 128 }, blur, 50_800, direction));
            MenuItem::tr(id).disabled(!enabled).on(move |e, _, cx| e.set_shape_shadow(shadow, cx)).into()
        }).collect()).into(),
        Submenu::tr("pptx.reflection", "flip", [
            ("pptx.reflection.none", None),
            ("pptx.reflection.tight", Some(reflection_preset(0, 35_000))),
            ("pptx.reflection.half", Some(reflection_preset(38_100, 55_000))),
            ("pptx.reflection.full", Some(reflection_preset(38_100, 90_000))),
        ].into_iter().map(|(id, value)| MenuItem::tr(id).disabled(!enabled).on(move |e, _, cx| e.set_shape_reflection(value, cx)).into()).collect()).into(),
        Submenu::tr("pptx.soft-edge", "auto_awesome", [
            ("pptx.soft-edge.none", None),
            ("pptx.soft-edge.1pt", Some(12_700)),
            ("pptx.soft-edge.2-5pt", Some(31_750)),
            ("pptx.soft-edge.5pt", Some(63_500)),
            ("pptx.soft-edge.10pt", Some(127_000)),
            ("pptx.soft-edge.25pt", Some(317_500)),
            ("pptx.soft-edge.50pt", Some(635_000)),
        ].into_iter().map(|(id, value)| MenuItem::tr(id).disabled(!enabled).on(move |e, _, cx| e.set_shape_soft_edge(value, cx)).into()).collect()).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.video.reset-effects")
            .disabled(!editor.selected_shapes().iter().any(|s| matches!(&s.kind, ShapeKind::Picture(p) if p.stroke.is_some() || p.corner > 0 || p.shadow.is_some() || p.reflection.is_some() || p.soft_edge.is_some())))
            .on(|e, _, cx| e.reset_picture_effects(cx)).into(),
    ]
}

fn image_items() -> Vec<MenuEntry> {
    vec![
        MenuItem::tr("image.file").icon("upload").on(|this, window, cx| this.pick_picture(window, cx)).into(),
        Submenu::tr("pptx.insert.screenshot", "fit_screen", screenshot_menu_items()).into(),
        MenuEntry::Separator,
        Submenu::tr("pptx.insert.icon", "category", icon_menu_items()).into(),
    ]
}

fn screenshot_menu_items() -> Vec<MenuEntry> {
    vec![
        MenuItem::tr("pptx.insert.screenshot.available-windows")
            .icon("fit_screen")
            .on(|this, _, cx| {
                this.insert_screenshot_window(cx);
            })
            .into(),
        MenuItem::tr("pptx.insert.screenshot.clipping")
            .icon("crop")
            .on(|this, _, cx| {
                this.insert_screenshot_clipping(cx);
            })
            .into(),
        MenuItem::tr("pptx.insert.screenshot.clipboard")
            .icon("content_paste")
            .on(|this, _, cx| {
                this.insert_screenshot_from_clipboard(cx);
            })
            .into(),
    ]
}

fn icon_menu_items() -> Vec<MenuEntry> {
    use super::editing::ICON_CATEGORIES;
    ICON_CATEGORIES
        .iter()
        .map(|cat| {
            let sub_items = cat
                .items
                .iter()
                .map(|(icon_name, label_key)| {
                    let iname = *icon_name;
                    MenuItem::tr(label_key)
                        .icon(iname)
                        .on(move |e, _, cx| {
                            e.insert_icon(iname, cx);
                        })
                        .into()
                })
                .collect::<Vec<MenuEntry>>();
            Submenu::tr(cat.label_key, cat.icon, sub_items).into()
        })
        .collect()
}


fn line_items() -> Vec<MenuEntry> {
    vec![
        MenuItem::tr("pptx.line").icon("horizontal_rule").on(|this, window, cx| this.set_pptx_tool(Tool::Line { kind: LineKind::Straight, arrow: ArrowKind::None }, window, cx)).into(),
        MenuItem::tr("pptx.arrow").icon("arrow_right_alt").on(|this, window, cx| this.set_pptx_tool(Tool::Line { kind: LineKind::Straight, arrow: ArrowKind::Single }, window, cx)).into(),
        MenuItem::tr("pptx.double-arrow").icon("sync_alt").on(|this, window, cx| this.set_pptx_tool(Tool::Line { kind: LineKind::Straight, arrow: ArrowKind::Double }, window, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.connector.bent").icon("alt_route").on(|this, window, cx| this.set_pptx_tool(Tool::Line { kind: LineKind::Bent, arrow: ArrowKind::None }, window, cx)).into(),
        MenuItem::tr("pptx.connector.bent-arrow").icon("turn_right").on(|this, window, cx| this.set_pptx_tool(Tool::Line { kind: LineKind::Bent, arrow: ArrowKind::Single }, window, cx)).into(),
        MenuItem::tr("pptx.connector.bent-double-arrow").icon("swap_calls").on(|this, window, cx| this.set_pptx_tool(Tool::Line { kind: LineKind::Bent, arrow: ArrowKind::Double }, window, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.connector.curved").icon("waves").on(|this, window, cx| this.set_pptx_tool(Tool::Line { kind: LineKind::Curved, arrow: ArrowKind::None }, window, cx)).into(),
        MenuItem::tr("pptx.connector.curved-arrow").icon("gesture").on(|this, window, cx| this.set_pptx_tool(Tool::Line { kind: LineKind::Curved, arrow: ArrowKind::Single }, window, cx)).into(),
        MenuItem::tr("pptx.connector.curved-double-arrow").icon("sync_alt").on(|this, window, cx| this.set_pptx_tool(Tool::Line { kind: LineKind::Curved, arrow: ArrowKind::Double }, window, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.line.freehand").icon("draw").on(|this, window, cx| this.set_pptx_tool(Tool::Line { kind: LineKind::Freehand, arrow: ArrowKind::None }, window, cx)).into(),
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

fn theme_palette_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    use super::theme_panel::THEME_PALETTES;
    THEME_PALETTES
        .iter()
        .map(|preset| {
            let pid = preset.id;
            let active = editor.is_theme_palette_active(pid);
            MenuItem::tr(preset.label_key)
                .checked(active)
                .on(move |e, w, cx| e.apply_theme_palette(pid, w, cx))
                .into()
        })
        .collect()
}

pub(crate) fn theme_palette_selected_items(_editor: &SlideEditor) -> Vec<MenuEntry> {
    use super::theme_panel::THEME_PALETTES;
    THEME_PALETTES
        .iter()
        .map(|preset| {
            let pid = preset.id;
            MenuItem::new(preset.selected_action_id, crate::shell::i18n::text(preset.label_key))
                .on(move |e, w, cx| e.apply_theme_palette_to_current_slide(pid, w, cx))
                .into()
        })
        .collect()
}

pub(crate) fn theme_effects_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    ibis_pptx::theme_effects::THEME_EFFECTS
        .iter()
        .map(|preset| {
            let pid = preset.id;
            let active = editor.is_theme_effects_active(pid);
            MenuItem::tr(preset.label_key)
                .checked(active)
                .on(move |e, w, cx| e.apply_theme_effects(pid, w, cx))
                .into()
        })
        .collect()
}

pub(crate) fn theme_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    vec![
        Submenu::tr("pptx.theme.palette", "palette", theme_palette_items(editor)).into(),
        Submenu::tr("pptx.theme.palette-selected", "palette", theme_palette_selected_items(editor)).into(),
        Submenu::tr("pptx.theme.effects", "auto_awesome", theme_effects_items(editor)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.theme-colors").icon("tune").on(|e, w, cx| e.open_theme_panel(w, cx)).into(),
        MenuItem::tr("pptx.theme.save-theme").icon("save").on(|e, w, cx| e.export_thmx(w, cx)).into(),
    ]
}

pub(crate) fn section_menu_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    vec![
        MenuItem::tr("pptx.section.add").on(|e, w, cx| e.open_section_panel(false, w, cx)).into(),
        MenuItem::tr("pptx.section.rename").disabled(editor.current_section().is_none()).on(|e, w, cx| e.open_section_panel(true, w, cx)).into(),
        MenuItem::tr("pptx.section.fold").disabled(editor.current_section().is_none()).on(|e, _, cx| e.toggle_current_section_fold(cx)).into(),
        MenuItem::tr("pptx.section.move-up").disabled(editor.current_section().is_none_or(|i| i == 0)).on(|e, w, cx| e.move_current_section(-1, w, cx)).into(),
        MenuItem::tr("pptx.section.move-down").disabled(editor.current_section().is_none_or(|i| i + 1 >= editor.presentation().sections.len())).on(|e, w, cx| e.move_current_section(1, w, cx)).into(),
        MenuItem::tr("pptx.section.remove").disabled(editor.current_section().is_none()).on(|e, w, cx| e.remove_current_section(w, cx)).into(),
    ]
}

pub(crate) fn slide_size_menu_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let size = editor.presentation().slide_size;
    vec![
        MenuItem::tr("pptx.slide-size.standard").checked(size == ibis_doc_model::EmuSize::new(9_144_000, 6_858_000)).on(|e, w, cx| e.set_slide_size(ibis_doc_model::EmuSize::new(9_144_000, 6_858_000), w, cx)).into(),
        MenuItem::tr("pptx.slide-size.wide").checked(size == ibis_doc_model::EmuSize::WIDESCREEN).on(|e, w, cx| e.set_slide_size(ibis_doc_model::EmuSize::WIDESCREEN, w, cx)).into(),
        MenuItem::tr("pptx.slide-size.landscape").checked(size.width >= size.height).on(|e, w, cx| e.open_slide_orientation(false, w, cx)).into(),
        MenuItem::tr("pptx.slide-size.portrait").checked(size.height > size.width).on(|e, w, cx| e.open_slide_orientation(true, w, cx)).into(),
        MenuItem::tr("pptx.slide-size.custom").on(|e, w, cx| e.open_slide_size_panel(w, cx)).into(),
    ]
}

pub(crate) const PROOFING_LANGUAGES: &[(&str, &str)] = &[
    ("ja-JP", "pptx.proofing-lang.ja-JP"),
    ("en-US", "pptx.proofing-lang.en-US"),
    ("en-GB", "pptx.proofing-lang.en-GB"),
    ("zh-CN", "pptx.proofing-lang.zh-CN"),
    ("zh-TW", "pptx.proofing-lang.zh-TW"),
    ("fr-FR", "pptx.proofing-lang.fr-FR"),
    ("de-DE", "pptx.proofing-lang.de-DE"),
    ("es-ES", "pptx.proofing-lang.es-ES"),
    ("ko-KR", "pptx.proofing-lang.ko-KR"),
    ("it-IT", "pptx.proofing-lang.it-IT"),
];

pub(crate) fn proofing_language_menu_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let current_lang = editor.selected_language();
    let mut items: Vec<MenuEntry> = PROOFING_LANGUAGES
        .iter()
        .map(|&(code, label_id)| {
            let is_selected = current_lang.as_deref() == Some(code);
            MenuItem::tr(label_id)
                .checked(is_selected)
                .on(move |this, window, cx| {
                    this.set_selection_language(Some(code), window, cx);
                })
                .into()
        })
        .collect();
    items.push(MenuEntry::Separator);
    let no_lang = current_lang.is_none();
    items.push(
        MenuItem::tr("pptx.proofing-lang.none")
            .checked(no_lang)
            .on(|this, window, cx| {
                this.set_selection_language(None, window, cx);
            })
            .into(),
    );
    items
}

pub(crate) fn menus(editor: &SlideEditor, menus: &mut Menus) {
    let has = !editor.selected_ids().is_empty();
    menus.connect("pptx.duplicate-slide", |this, window, cx| this.duplicate_current_slide(window, cx));
    // スライドのコピー・切り取り・貼り付け, after 複製.
    {
        let has_clip = super::editing::slide_clipboard().is_some();
        let entries = vec![
            MenuItem::tr("pptx.copy-slide").on(|e, w, cx| e.copy_current_slide(w, cx)).into(),
            MenuItem::tr("pptx.cut-slide").disabled(editor.presentation().slides.len() < 2).on(|e, w, cx| e.cut_current_slide(w, cx)).into(),
            MenuItem::tr("pptx.paste-slide").disabled(!has_clip).on(|e, w, cx| { e.paste_slides(w, cx); }).into(),
            MenuItem::tr("pptx.paste-slide-keep").disabled(!has_clip).on(|e, w, cx| { e.paste_slides_keeping_formatting(w, cx); }).into(),
        ];
        let items = menus.items_mut(MenuId::Slide);
        let at = items.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "pptx.duplicate-slide")).map_or(items.len(), |i| i + 1);
        for (k, entry) in entries.into_iter().enumerate() {
            items.insert(at + k, entry);
        }
    }
    // アニメーションのコピー・貼り付け（書式のコピー・貼り付けの直後）.
    {
        let can_copy = editor.can_copy_animations();
        let can_paste = editor.can_paste_animations();
        let anim_entries = vec![
            MenuItem::tr("pptx.anim.copy").shortcut("⌥⇧⌘C").disabled(!can_copy).on(|e, _, cx| e.copy_shape_animations(cx)).into(),
            MenuItem::tr("pptx.anim.paste").shortcut("⌥⇧⌘V").disabled(!can_paste).on(|e, _, cx| e.paste_shape_animations(cx)).into(),
        ];
        let items = menus.items_mut(MenuId::Edit);
        let at = items.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "edit.paste-format")).map_or(items.len(), |i| i + 1);
        for (k, entry) in anim_entries.into_iter().enumerate() {
            items.insert(at + k, entry);
        }
    }
    // ファイル情報・プロパティパネル
    menus.connect("file.info", |this, window, cx| this.open_presentation_properties_panel(window, cx));
    // ドキュメントの検査（個人情報・非表示要素の除去: PPTX-REVIEW-018）
    menus.connect("pptx.inspect-document", |this, window, cx| this.open_presentation_inspect_panel(window, cx));
    // アクセシビリティのチェック（PPTX-ACCESS-001）
    menus.connect("pptx.check-accessibility", |this, window, cx| this.open_pptx_accessibility_panel(window, cx));
    // 互換性チェック（PPTX-FILE-007）
    menus.connect("pptx.compatibility-check", |this, window, cx| this.open_pptx_compatibility_panel(window, cx));
    // 最終版としてマーク & 読み取り専用を推奨（ファイルメニュー）.
    menus.connect("pptx.mark-as-final", |this, _, cx| this.toggle_pptx_mark_as_final(cx));
    menus.connect("pptx.read-only-recommended", |this, _, cx| this.toggle_pptx_read_only_recommended(cx));
    {
        let file = menus.items_mut(MenuId::File);
        let at = file.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "file.info")).map_or(file.len(), |i| i + 1);
        file.insert(
            at,
            MenuItem::tr("pptx.mark-as-final")
                .icon("lock")
                .checked(editor.is_pptx_mark_as_final())
                .on(|this, _, cx| this.toggle_pptx_mark_as_final(cx))
                .into(),
        );
        file.insert(
            at + 1,
            MenuItem::tr("pptx.read-only-recommended")
                .icon("visibility")
                .checked(editor.is_pptx_read_only_recommended())
                .on(|this, _, cx| this.toggle_pptx_read_only_recommended(cx))
                .into(),
        );
        file.insert(at + 2, MenuItem::tr("pptx.modify-password").icon("edit_off").on(|this, window, cx| this.ask_modify_password_to_set(window, cx)).into());
        file.insert(
            at + 3,
            MenuItem::tr("pptx.inspect-document")
                .icon("find_in_page")
                .on(|this, window, cx| this.open_presentation_inspect_panel(window, cx))
                .into(),
        );
        file.insert(
            at + 4,
            MenuItem::tr("pptx.check-accessibility")
                .icon("accessibility")
                .on(|this, window, cx| this.open_pptx_accessibility_panel(window, cx))
                .into(),
        );
        file.insert(
            at + 5,
            MenuItem::tr("pptx.compatibility-check")
                .icon("rule")
                .on(|this, window, cx| this.open_pptx_compatibility_panel(window, cx))
                .into(),
        );
        file.insert(
            at + 6,
            MenuItem::tr("pptx.recover-unsaved")
                .icon("history")
                .on(|this, _window, cx| this.open_recover_unsaved(cx))
                .into(),
        );
    }
    menus.items_mut(MenuId::Tools).push(
        MenuItem::tr("pptx.check-accessibility")
            .icon("accessibility")
            .on(|this, window, cx| this.open_pptx_accessibility_panel(window, cx))
            .into(),
    );
    menus.items_mut(MenuId::Tools).push(
        MenuItem::tr("pptx.compatibility-check")
            .icon("rule")
            .on(|this, window, cx| this.open_pptx_compatibility_panel(window, cx))
            .into(),
    );
    menus.items_mut(MenuId::Tools).push(
        MenuItem::tr("pptx.autorecover-settings")
            .icon("settings_backup_restore")
            .on(|this, window, cx| this.open_autorecover_settings(window, cx))
            .into(),
    );
    menus.items_mut(MenuId::Tools).push(
        MenuItem::tr("pptx.recover-unsaved")
            .icon("history")
            .on(|this, _window, cx| this.open_recover_unsaved(cx))
            .into(),
    );
    menus.items_mut(MenuId::Tools).push(Submenu::tr("pptx.proofing-language", "language", proofing_language_menu_items(editor)).into());
    // プレゼンテーションの比較と変更の統合 (PPTX-REVIEW-019)
    let comparing = editor.is_comparing_presentations();
    let has_diffs = editor.pptx().and_then(|p| p.compare_panel.as_ref()).is_some_and(|p| !p.diffs.is_empty());
    menus.connect("pptx.review.compare", |this, window, cx| this.pick_compare_presentation_file(window, cx));
    menus.connect("pptx.review.accept", |this, _, cx| this.compare_accept_current(cx));
    menus.connect("pptx.review.accept-all", |this, _, cx| this.compare_accept_all(cx));
    menus.connect("pptx.review.reject", |this, _, cx| this.compare_reject_current(cx));
    menus.connect("pptx.review.reject-all", |this, _, cx| this.compare_reject_all(cx));
    menus.connect("pptx.review.end-review", |this, _, cx| this.compare_end_review(cx));

    menus.items_mut(MenuId::Tools).push(
        MenuItem::tr("pptx.review.compare")
            .icon("rate_review")
            .on(|this, window, cx| this.pick_compare_presentation_file(window, cx))
            .into(),
    );
    if comparing {
        menus.items_mut(MenuId::Tools).push(
            MenuItem::tr("pptx.review.accept")
                .icon("check")
                .disabled(!has_diffs)
                .on(|this, _, cx| this.compare_accept_current(cx))
                .into(),
        );
        menus.items_mut(MenuId::Tools).push(
            MenuItem::tr("pptx.review.accept-all")
                .icon("checklist")
                .disabled(!has_diffs)
                .on(|this, _, cx| this.compare_accept_all(cx))
                .into(),
        );
        menus.items_mut(MenuId::Tools).push(
            MenuItem::tr("pptx.review.reject")
                .icon("close")
                .disabled(!has_diffs)
                .on(|this, _, cx| this.compare_reject_current(cx))
                .into(),
        );
        menus.items_mut(MenuId::Tools).push(
            MenuItem::tr("pptx.review.reject-all")
                .icon("delete")
                .disabled(!has_diffs)
                .on(|this, _, cx| this.compare_reject_all(cx))
                .into(),
        );
        menus.items_mut(MenuId::Tools).push(
            MenuItem::tr("pptx.review.end-review")
                .icon("close")
                .on(|this, _, cx| this.compare_end_review(cx))
                .into(),
        );
    }
    menus.connect("pptx.autorecover-settings", |this, window, cx| this.open_autorecover_settings(window, cx));
    menus.connect("pptx.recover-unsaved", |this, _window, cx| this.open_recover_unsaved(cx));
    menus.connect("pptx.skip-slide", |this, window, cx| this.toggle_skip_slide(window, cx));
    menus.connect("pptx.notes", |this, window, cx| this.toggle_notes(window, cx));
    // メディアの字幕・キャプション (PPTX-ACCESS-010)
    menus.connect("pptx.clip.caption.add", |this, _, cx| this.add_selected_clip_caption(&crate::shell::i18n::text("_pptx.caption-track-language"), "ja-JP", &crate::shell::i18n::text("_pptx.caption-template"), cx));
    menus.connect("pptx.clip.caption.clear", |this, _, cx| this.clear_selected_clip_captions(cx));
    menus.connect("pptx.clip.insert-caption", |this, _, cx| this.add_selected_clip_caption(&crate::shell::i18n::text("_pptx.caption-track-language"), "ja-JP", &crate::shell::i18n::text("_pptx.caption-template"), cx));
    menus.connect("pptx.clip.clear-captions", |this, _, cx| this.clear_selected_clip_captions(cx));
    // 画像の背景削除・透明色指定 (PPTX-IMAGE-008)
    menus.connect("pptx.picture.remove-background", |this, _, cx| this.set_selected_pictures_clear(Some(ibis_doc_model::Rgb(0xffffff)), cx));
    menus.connect("pptx.picture.color.transparent-white", |this, _, cx| this.set_selected_pictures_clear(Some(ibis_doc_model::Rgb(0xffffff)), cx));
    menus.connect("pptx.picture.color.transparent-black", |this, _, cx| this.set_selected_pictures_clear(Some(ibis_doc_model::Rgb(0x000000)), cx));
    menus.connect("pptx.picture.color.transparent-clear", |this, _, cx| this.set_selected_pictures_clear(None, cx));
    // マイクから音声録音 (PPTX-AUDIO-002)
    menus.connect("pptx.audio.record", |this, window, cx| this.open_audio_record_panel(window, cx));
    menus.connect("pptx.audio-record", |this, window, cx| this.open_audio_record_panel(window, cx));
    // メディア圧縮と互換性最適化 (PPTX-VIDEO-017)
    menus.connect("pptx.media.compress-1080p", |this, _, cx| { this.compress_presentation_media(super::MediaQuality::FullHd, cx); });
    menus.connect("pptx.media.compress-720p", |this, _, cx| { this.compress_presentation_media(super::MediaQuality::Hd, cx); });
    menus.connect("pptx.media.compress-480p", |this, _, cx| { this.compress_presentation_media(super::MediaQuality::Standard, cx); });
    menus.connect("pptx.media.optimize-compatibility", |this, _, cx| { this.optimize_media_compatibility(cx); });
    // インク変換と再生 (PPTX-INK-009, PPTX-INK-010)
    menus.connect("pptx.ink.to-shape", |this, _, cx| this.pptx_ink_to_shape(cx));
    menus.connect("pptx.ink.to-text", |this, _, cx| this.pptx_ink_to_text(cx));
    menus.connect("pptx.ink.to-math", |this, _, cx| this.pptx_ink_to_math(cx));
    menus.connect("pptx.ink.replay", |this, _, cx| this.pptx_ink_replay(cx));
    let notes = editor.notes_shown();
    if let Some(item) = menus.find_mut("pptx.notes") {
        item.checked = notes;
    }
    let skipped = editor.slide().hidden;
    if let Some(item) = menus.find_mut("pptx.skip-slide") {
        item.checked = skipped;
    }
    // 非表示スライド タイトルの追加 (PPTX-ACCESS-003)
    menus.connect("pptx.slide.add-hidden-title", |this, _, cx| {
        let slide_id = this.current_slide;
        this.add_hidden_slide_title(slide_id, None, cx);
    });
    {
        let items = menus.items_mut(MenuId::Slide);
        let at = items.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "pptx.skip-slide")).map_or(items.len(), |i| i + 1);
        items.insert(
            at,
            MenuItem::tr("pptx.slide.add-hidden-title")
                .icon("title")
                .on(|this, _, cx| {
                    let slide_id = this.current_slide;
                    this.add_hidden_slide_title(slide_id, None, cx);
                })
                .into(),
        );
    }
    menus.connect("insert.comment", |this, window, cx| this.open_slide_comments(window, cx));
    menus.items_mut(MenuId::Arrange).push(MenuItem::tr("pptx.selection-pane").icon("visibility").checked(editor.pptx().is_some_and(|s| s.selection_pane.is_some())).on(|e, _, cx| e.toggle_selection_pane(cx)).into());
    menus.items_mut(MenuId::Arrange).push(MenuItem::tr("pptx.reading-order").icon("format_list_numbered").checked(editor.pptx().is_some_and(|s| s.reading_order_pane.is_some())).on(|e, _, cx| e.toggle_reading_order_pane(cx)).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.theme.palette", "palette", theme_palette_items(editor)).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.theme.palette-selected", "palette", theme_palette_selected_items(editor)).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.theme.effects", "auto_awesome", theme_effects_items(editor)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.theme-colors").icon("tune").on(|e, w, cx| e.open_theme_panel(w, cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.theme.save-theme").icon("save").on(|e, w, cx| e.export_thmx(w, cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.replace-fonts").icon("font_download").on(|e, w, cx| e.open_replace_fonts_panel(w, cx)).into());
    menus.connect("insert.symbol", |this, window, cx| this.open_symbols_panel(window, cx));
    menus.items_mut(MenuId::Insert).push(Submenu::tr("pptx.lines-and-connectors", "horizontal_rule", line_items()).into());
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.photo-album").icon("photo_library").on(|e, w, cx| e.open_photo_album_panel(w, cx)).into());
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.3d-model").icon("view_in_ar").on(|e, w, cx| e.pick_3d_model(w, cx)).into());
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.insert-object").icon("attachment").on(|e, w, cx| e.pick_insert_object(w, cx)).into());
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.math").icon("functions").on(|e, w, cx| e.open_pptx_math_panel(w, cx)).into());
    menus.items_mut(MenuId::Insert).push(MenuItem::new("pptx.ink", crate::shell::i18n::text("xlsx.ink")).icon("draw").on(|e, w, cx| e.open_pptx_ink_panel(w, cx)).into());
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.video").icon("movie").on(|e, w, cx| e.pick_clip(false, w, cx)).into());
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.audio").icon("music_note").on(|e, w, cx| e.pick_clip(true, w, cx)).into());
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.video-link").icon("movie").on(|e, w, cx| e.pick_clip_link(false, w, cx)).into());
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.audio-link").icon("music_note").on(|e, w, cx| e.pick_clip_link(true, w, cx)).into());
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.audio-record").icon("mic").on(|e, w, cx| e.open_audio_record_panel(w, cx)).into());
    let has_clip = editor.selected_clip_options().is_some();
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.clip.caption.add").icon("closed_caption").disabled(!has_clip).on(|e, _, cx| e.add_selected_clip_caption(&crate::shell::i18n::text("_pptx.caption-track-language"), "ja-JP", &crate::shell::i18n::text("_pptx.caption-template"), cx)).into());
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.clip.caption.clear").icon("delete").disabled(!has_clip).on(|e, _, cx| e.clear_selected_clip_captions(cx)).into());
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.link").icon("link").on(|e, w, cx| e.slide_open_link_panel(w, cx)).into());
    let has_sections = !editor.presentation().sections.is_empty();
    let zoom_shape = editor.selected_shapes().first().map(|s| s.id).filter(|id| {
        matches!(editor.slide().shape_links.get(id), Some(ibis_doc_model::SlideLink::Slide(_)))
    });
    let has_custom_img = zoom_shape.is_some_and(|id| {
        editor.slide().shape(id).is_some_and(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Picture(_)))
    });
    fn zoom_duration_items(editor: &SlideEditor, zoom_shape: Option<ibis_doc_model::ShapeId>) -> Vec<MenuEntry> {
        const DURATIONS: [(u32, &str); 5] = [
            (500, "pptx.zoom.duration-500"),
            (1000, "pptx.zoom.duration-1000"),
            (1500, "pptx.zoom.duration-1500"),
            (2000, "pptx.zoom.duration-2000"),
            (2500, "pptx.zoom.duration-2500"),
        ];
        let cur_dur = zoom_shape.map(|id| editor.shape_zoom_duration(id)).unwrap_or(1000);
        DURATIONS
            .iter()
            .map(|&(ms, label_key)| {
                let is_checked = zoom_shape.is_some() && cur_dur == ms;
                MenuItem::tr(label_key)
                    .disabled(zoom_shape.is_none())
                    .checked(is_checked)
                    .on(move |e, _, cx| {
                        if let Some(id) = zoom_shape {
                            e.set_shape_zoom_duration(id, ms, cx);
                        }
                    })
                    .into()
            })
            .collect()
    }
    let zoom_items: Vec<MenuEntry> = vec![
        MenuItem::tr("pptx.insert.summary-zoom").icon("auto_awesome_mosaic").on(|e, w, cx| e.open_summary_zoom_dialog(w, cx)).into(),
        MenuItem::tr("pptx.insert.slide-zoom").icon("slideshow").on(|e, w, cx| e.open_slide_zoom_dialog(w, cx)).into(),
        MenuItem::tr("pptx.insert.section-zoom").icon("folder").disabled(!has_sections).on(|e, w, cx| e.open_section_zoom_dialog(w, cx)).into(),
        MenuItem::tr("pptx.zoom.change-image").icon("image").disabled(zoom_shape.is_none()).on(|e, w, cx| e.pick_zoom_image(w, cx)).into(),
        MenuItem::tr("pptx.zoom.reset-image").icon("refresh").disabled(!has_custom_img).on(move |e, _, cx| {
            if let Some(id) = zoom_shape {
                e.reset_zoom_image(id, cx);
            }
        }).into(),
        MenuItem::tr("pptx.zoom.return-to-zoom")
            .icon("undo")
            .disabled(zoom_shape.is_none())
            .checked(zoom_shape.is_some_and(|id| editor.slide().shape_return_to_zoom.contains(&id)))
            .on(move |e, _, cx| {
                if let Some(id) = zoom_shape {
                    e.toggle_shape_return_to_zoom(id, cx);
                }
            })
            .into(),
        MenuItem::tr("pptx.zoom.background")
            .icon("wallpaper")
            .disabled(zoom_shape.is_none())
            .checked(zoom_shape.is_some_and(|id| editor.shape_zoom_show_bg(id)))
            .on(move |e, _, cx| {
                if let Some(id) = zoom_shape {
                    e.toggle_shape_zoom_show_bg(id, cx);
                }
            })
            .into(),
        Submenu::tr("pptx.zoom.duration", "timer", zoom_duration_items(editor, zoom_shape)).into(),
    ];
    menus.items_mut(MenuId::Insert).push(Submenu::tr("pptx.insert.zoom", "zoom_in", zoom_items).into());
    menus.connect("pptx.insert.summary-zoom", |this, window, cx| this.open_summary_zoom_dialog(window, cx));
    menus.connect("pptx.insert.slide-zoom", |this, window, cx| this.open_slide_zoom_dialog(window, cx));
    menus.connect("pptx.insert.section-zoom", |this, window, cx| this.open_section_zoom_dialog(window, cx));
    menus.connect("pptx.zoom.change-image", |this, window, cx| this.pick_zoom_image(window, cx));
    menus.connect("pptx.zoom.reset-image", |this, _, cx| {
        if let Some(id) = this.selected_shapes().first().map(|s| s.id).filter(|id| this.is_zoom_shape(*id)) {
            this.reset_zoom_image(id, cx);
        }
    });
    menus.connect("pptx.zoom.return-to-zoom", |this, _, cx| {
        let zoom_shape = this.selected_shapes().first().map(|s| s.id).filter(|id| {
            matches!(this.slide().shape_links.get(id), Some(ibis_doc_model::SlideLink::Slide(_)))
        });
        if let Some(id) = zoom_shape {
            this.toggle_shape_return_to_zoom(id, cx);
        }
    });
    menus.connect("pptx.zoom.background", |this, _, cx| {
        let zoom_shape = this.selected_shapes().first().map(|s| s.id).filter(|id| {
            matches!(this.slide().shape_links.get(id), Some(ibis_doc_model::SlideLink::Slide(_)))
        });
        if let Some(id) = zoom_shape {
            this.toggle_shape_zoom_show_bg(id, cx);
        }
    });
    for &(ms, key) in &[(500, "pptx.zoom.duration-500"), (1000, "pptx.zoom.duration-1000"), (1500, "pptx.zoom.duration-1500"), (2000, "pptx.zoom.duration-2000"), (2500, "pptx.zoom.duration-2500")] {
        menus.connect(key, move |this, _, cx| {
            let zoom_shape = this.selected_shapes().first().map(|s| s.id).filter(|id| {
                matches!(this.slide().shape_links.get(id), Some(ibis_doc_model::SlideLink::Slide(_)))
            });
            if let Some(id) = zoom_shape {
                this.set_shape_zoom_duration(id, ms, cx);
            }
        });
    }
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.header-footer").icon("view_quilt").on(|e, w, cx| e.open_header_footer(w, cx)).into());
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.notes-header-footer").icon("sticky_note_2").on(|e, w, cx| e.open_notes_marks(w, cx)).into());
    menus.items_mut(MenuId::Insert).push(MenuItem::tr("pptx.insert.wordart").icon("title").on(|e, _, cx| { e.insert_wordart(0, None, cx); }).into());
    menus.items_mut(MenuId::Insert).push(Submenu::tr("pptx.insert.wordart-styles", "title", wordart_insert_items()).into());
    for (idx, preset) in crate::pptx::editing::WORDART_PRESETS.iter().enumerate() {
        let insert_id = preset.insert_id;
        let apply_id = preset.apply_id;
        menus.connect(insert_id, move |this, _, cx| {
            this.insert_wordart(idx, None, cx);
        });
        menus.connect(apply_id, move |this, w, cx| {
            this.set_selected_text_wordart_style(idx, w, cx);
        });
    }
    menus.connect("pptx.insert.wordart", |this, _, cx| {
        this.insert_wordart(0, None, cx);
    });
    for &(prst, id, _) in crate::pptx::editing::TEXT_WARP_PRESETS {
        menus.connect(id, move |this, _, cx| this.set_text_warp(Some(prst), cx));
    }
    menus.connect("pptx.text.warp.none", |this, _, cx| this.set_text_warp(None, cx));
    menus.connect("pptx.background", |this, _, cx| {
        this.set_selection(Vec::new());
        this.sidebar_open = true;
        cx.notify();
    });
    menus.connect("slideshow.settings", |this, window, cx| this.open_show_panel(window, cx));
    {
        let custom_shows = editor.presentation().custom_shows.clone();
        let mut custom_items: Vec<MenuEntry> = Vec::new();
        for cs in custom_shows {
            let cs_id = cs.id;
            custom_items.push(MenuItem::new("pptx.custom-show-item", cs.name).icon("play_arrow").on(move |e, w, cx| e.start_custom_slideshow(cs_id, w, cx)).into());
        }
        if !custom_items.is_empty() {
            custom_items.push(MenuEntry::Separator);
        }
        custom_items.push(MenuItem::tr("slideshow.settings").icon("settings").on(|e, w, cx| e.open_show_panel(w, cx)).into());
        menus.items_mut(MenuId::Slide).push(Submenu::tr("slideshow.custom-shows", "filter_none", custom_items).into());
    }
    menus.items_mut(MenuId::Slide).push(MenuItem::tr("slideshow.rehearse-timings").icon("schedule").on(|e, w, cx| e.start_rehearsal(w, cx)).into());
    if editor.pptx().is_some_and(|s|s.section_panel.is_some()) {
        menus.items_mut(MenuId::Slide).push(Submenu::tr("pptx.section-actions", "view_quilt",vec![MenuItem::tr("pptx.section.apply").on(|e,w,cx|e.apply_section_panel(w,cx)).into(),MenuItem::tr("pptx.section.cancel").on(|e,w,cx|e.close_section_panel(w,cx)).into()]).into());
    }
    if editor.pptx().is_some_and(|s| s.show_panel.is_some()) {
        menus.items_mut(MenuId::Slide).push(Submenu::tr("pptx.show-settings-actions", "settings", vec![
            MenuItem::tr("pptx.show-settings.apply").on(|e, w, cx| e.apply_show_panel(w, cx)).into(),
            MenuItem::tr("pptx.show-settings.cancel").on(|e, w, cx| e.close_show_panel(w, cx)).into(),
        ]).into());
    }
    if editor.pptx().is_some_and(|s| s.size_panel.is_some()) {
        menus.items_mut(MenuId::Slide).push(Submenu::tr("pptx.slide-size-actions", "aspect_ratio", vec![
            MenuItem::tr("pptx.slide-size.apply").on(|e, w, cx| e.apply_slide_size_panel(w, cx)).into(),
            MenuItem::tr("pptx.slide-size.cancel").on(|e, w, cx| e.close_slide_size_panel(w, cx)).into(),
        ]).into());
    }
    if editor.pptx().is_some_and(|s| s.insets_panel.is_some()) {
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.insets-actions", "format_indent_increase", vec![
            MenuItem::tr("pptx.insets.apply").on(|e, w, cx| e.apply_text_insets_panel(w, cx)).into(),
            MenuItem::tr("pptx.insets.cancel").on(|e, w, cx| e.close_text_insets_panel(w, cx)).into(),
        ]).into());
    }
    if editor.pptx().is_some_and(|s| s.chart_panel.is_some()) {
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart-actions", "insert_chart", vec![
            MenuItem::tr("pptx.chart.apply").on(|e, w, cx| e.apply_chart_panel(w, cx)).into(),
            MenuItem::tr("pptx.chart.cancel").on(|e, w, cx| e.close_chart_panel(w, cx)).into(),
        ]).into());
    }
    if editor.pptx().is_some_and(|s| s.frame_panel.is_some()) {
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.frame-actions", "aspect_ratio", vec![
            MenuItem::tr("pptx.frame.apply").on(|e, w, cx| e.apply_frame_panel(w, cx)).into(),
            MenuItem::tr("pptx.frame.cancel").on(|e, w, cx| e.close_frame_panel(w, cx)).into(),
        ]).into());
    }
    if editor.pptx().is_some_and(|s| s.crop_panel.is_some()) {
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.crop-actions", "crop", vec![
            MenuItem::tr("pptx.crop.apply").on(|e, w, cx| e.apply_crop_panel(w, cx)).into(),
            MenuItem::tr("pptx.crop.cancel").on(|e, w, cx| e.close_crop_panel(w, cx)).into(),
        ]).into());
    }
    if let Some(p)=editor.pptx().and_then(|s|s.gradient_panel.as_ref()) {
        let mut actions=vec![MenuItem::tr("pptx.gradient.apply").on(|e,w,cx|e.apply_gradient_panel(w,cx)).into(),MenuItem::tr("pptx.gradient.cancel").on(|e,w,cx|e.close_gradient_panel(w,cx)).into(),MenuItem::tr("pptx.gradient.add").disabled(p.pattern.is_some()).on(|e,w,cx|e.add_gradient_stop(w,cx)).into()];
        if let Some(selected)=&p.pattern {
            for &(id,preset,label) in super::gradient_panel::PATTERN_CHOICES {actions.push(MenuItem::new(id,crate::shell::i18n::text(label)).checked(selected==preset).on(move|e,_,cx|e.set_pattern_preset(preset,cx)).into());}
        }
        if let Some(selected)=p.path {
            for (id,path) in [("pptx.gradient.circle",GradientPath::Circle),("pptx.gradient.rect",GradientPath::Rect),("pptx.gradient.shape",GradientPath::Shape)] {actions.push(MenuItem::tr(id).checked(selected==path).on(move|e,_,cx|e.set_gradient_path(path,cx)).into());}
        }
        actions.push(MenuItem::tr("pptx.gradient.remove-first").disabled(p.pattern.is_some() || p.stops.len()<=2).on(|e,w,cx|e.remove_gradient_stop(0,w,cx)).into());
        actions.push(MenuItem::tr("pptx.gradient.remove-last").disabled(p.pattern.is_some() || p.stops.len()<=2).on(|e,w,cx| {let Some(n)=e.pptx().and_then(|s|s.gradient_panel.as_ref()).map(|p|p.stops.len()) else {return};e.remove_gradient_stop(n-1,w,cx);}).into());
        menus.items_mut(MenuId::Format).push(Submenu::new("pptx.gradient-actions","format_color_fill",if p.pattern.is_some() { crate::shell::i18n::text("pptx.chrome.pattern-fill") } else { crate::shell::i18n::text("pptx.chrome.gradient-fill") },actions).into());
    }
    if editor.pptx().is_some_and(|s| s.glow_panel.is_some()) {
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.glow-actions", "format_color_fill", vec![
            MenuItem::tr("pptx.glow.apply").on(|e,w,cx|e.apply_glow_panel(w,cx)).into(),
            MenuItem::tr("pptx.glow.cancel").on(|e,w,cx|e.close_glow_panel(w,cx)).into(),
        ]).into());
    }
    if editor.pptx().is_some_and(|s| s.shadow_panel.is_some()) {
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.shadow-actions", "format_color_fill", vec![
            MenuItem::tr("pptx.shadow.apply").on(|e,w,cx|e.apply_shadow_panel(w,cx)).into(),
            MenuItem::tr("pptx.shadow.cancel").on(|e,w,cx|e.close_shadow_panel(w,cx)).into(),
        ]).into());
    }
    if editor.pptx().is_some_and(|s| s.timing_panel.is_some()) {
        menus.items_mut(MenuId::Slide).push(Submenu::tr("pptx.timing-actions", "transition", vec![
            MenuItem::tr("pptx.timing.auto").on(|e, _, cx| e.toggle_timing_panel(true, cx)).into(),
            MenuItem::tr("pptx.timing.click").on(|e, _, cx| e.toggle_timing_panel(false, cx)).into(),
            MenuItem::tr("pptx.timing.apply").on(|e, w, cx| e.apply_timing_panel(w, cx)).into(),
            MenuItem::tr("pptx.timing.cancel").on(|e, w, cx| e.close_timing_panel(w, cx)).into(),
        ]).into());
    }
    let mut text_outline_items = color_menu(editor, "pptx.text-outline", Some(crate::shell::i18n::text("pptx.text-outline.none")), |e, rgb, w, cx| {
        e.set_text_outline_color(w, cx, rgb);
    });
    if let Some(MenuEntry::Palette(p)) = text_outline_items.first_mut() {
        if let Some(none) = p.none.as_mut() {
            none.id = "pptx.text-outline.none";
        }
        for (hex, id) in [
            (0x000000, "pptx.text-outline.black"),
            (0xffffff, "pptx.text-outline.white"),
            (0xc00000, "pptx.text-outline.red"),
            (0x0070c0, "pptx.text-outline.blue"), // the standard blue; the theme row follows the presentation's theme, which may not have 4472C4
            (0xffc000, "pptx.text-outline.yellow"),
            (0x70ad47, "pptx.text-outline.green"),
        ] {
            if let Some(sw) = p.theme.iter_mut().flatten().chain(p.standard.iter_mut()).find(|s| s.rgb == Rgb(hex)) {
                sw.item.id = id;
            }
        }
    }
    text_outline_items.push(MenuEntry::Separator);
    text_outline_items.push(Submenu::tr("pptx.text-outline.weight", "line_style", vec![
        MenuItem::tr("pptx.text-outline.0-5pt").on(|e, w, cx| e.set_text_outline_weight(w, cx, 6_350)).into(),
        MenuItem::tr("pptx.text-outline.1pt").on(|e, w, cx| e.set_text_outline_weight(w, cx, 12_700)).into(),
        MenuItem::tr("pptx.text-outline.1-5pt").on(|e, w, cx| e.set_text_outline_weight(w, cx, 19_050)).into(),
        MenuItem::tr("pptx.text-outline.2-25pt").on(|e, w, cx| e.set_text_outline_weight(w, cx, 28_575)).into(),
        MenuItem::tr("pptx.text-outline.3pt").on(|e, w, cx| e.set_text_outline_weight(w, cx, 38_100)).into(),
        MenuItem::tr("pptx.text-outline.4-5pt").on(|e, w, cx| e.set_text_outline_weight(w, cx, 57_150)).into(),
        MenuItem::tr("pptx.text-outline.6pt").on(|e, w, cx| e.set_text_outline_weight(w, cx, 76_200)).into(),
    ]).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.text-outline", "format_color_text", text_outline_items).into());
    let has_table = editor.selected_shapes().iter().any(|s| matches!(s.kind, ShapeKind::Table(_)));
    menus.items_mut(MenuId::Format).push(Submenu::tr("table.borders", "border_all", table_borders_menu_items(has_table)).into());
    let has_line_or_connector = editor.selected_shapes().iter().any(|s| matches!(&s.kind, ShapeKind::Line(_)) || matches!(&s.kind, ShapeKind::Auto(a) if super::editing::is_connector_geometry(&a.geometry)));
    if has_line_or_connector {
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chrome.begin-arrow", "line_start", line_end_menu(editor, true)).into());
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chrome.end-arrow", "line_end_arrow_notch", line_end_menu(editor, false)).into());
    }
    let no_chart = !editor.selected_shapes().first().is_some_and(|s| matches!(s.kind, ShapeKind::Chart(_)));
    // A chart of a new kind (chartEx): only its data (in its workbook) is edited yet.
    let chart_ex = editor.selected_shapes().first().is_some_and(|s| matches!(&s.kind, ShapeKind::Chart(c) if c.ex.is_some()));
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.chart.data").disabled(no_chart).on(|e, w, cx| e.open_chart_data(w, cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.chart.data-quick").disabled(no_chart || chart_ex).on(|e, w, cx| e.open_chart_panel(w, cx)).into());
    if !no_chart && !chart_ex {
        let chart_group = editor.selected_shapes().first().and_then(|s| match &s.kind {
            ShapeKind::Chart(c) => c.groups.first().map(|g| g.kind),
            _ => None,
        });
        let is_pie_family = chart_group.is_some_and(|k| matches!(k, ChartType::Pie { .. } | ChartType::Doughnut { .. }));
        let is_doughnut = chart_group.is_some_and(|k| matches!(k, ChartType::Doughnut { .. }));
        let is_bubble = chart_group.is_some_and(|k| matches!(k, ChartType::Bubble { .. }));
        let is_scatter = chart_group.is_some_and(|k| matches!(k, ChartType::Scatter { .. }));
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart.title-menu", "title", chart_title_items(editor)).into());
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart.legend-menu", "legend", legend_items(editor)).into());
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart.labels-menu", "label", chart_labels_items(editor)).into());
        if is_pie_family {
            menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart.first-angle-menu", "rotate_right", chart_first_angle_items(editor)).into());
        }
        if is_doughnut {
            menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart.hole-size-menu", "donut_large", chart_hole_size_items(editor)).into());
        }
        if is_bubble {
            menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart.bubble-scale-menu", "aspect_ratio", chart_bubble_scale_items(editor)).into());
        }
        if is_scatter {
            menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart.scatter-smooth-menu", "polyline", chart_scatter_smooth_items(editor)).into());
        }
        if !is_pie_family {
            menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart.axis-menu", "timeline", chart_axis_items(editor)).into());
            menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart.trendline-menu", "show_chart", chart_trendline_items(editor)).into());
            menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart.error-bars-menu", "error", chart_error_bars_items(editor)).into());
        }
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart.series-format-menu", "palette", chart_series_format_items(editor)).into());
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart.plot-area-menu", "crop_free", chart_plot_area_items(editor)).into());
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart.style-menu", "palette", chart_style_items(editor)).into());
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.chart.palette-menu", "color_lens", chart_palette_items(editor)).into());
    }
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.default-shape").disabled(!matches!(editor.selected_shapes().as_slice(), [s] if matches!(s.kind, ShapeKind::Auto(_) | ShapeKind::Line(_)))).on(|e, _, cx| e.set_object_default(cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.frame.settings").disabled(editor.selected_shapes().is_empty()).on(|e, w, cx| e.open_frame_panel(w, cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.alt-text").disabled(editor.selected_shapes().is_empty()).on(|e, w, cx| e.open_alt_text_panel(w, cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.math.copy-selection-latex").icon("functions").disabled(editor.selected_math().is_none()).on(|e, _, cx| e.copy_selected_math_as_latex(cx)).into());
    if let Some(shape_id) = zoom_shape {
        let is_custom = editor.slide().shape(shape_id).is_some_and(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Picture(_)));
        menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.zoom.change-image").icon("image").on(|e, w, cx| e.pick_zoom_image(w, cx)).into());
        menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.zoom.reset-image").icon("refresh").disabled(!is_custom).on(move |e, _, cx| { e.reset_zoom_image(shape_id, cx); }).into());
        let is_rtz = editor.slide().shape_return_to_zoom.contains(&shape_id);
        menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.zoom.return-to-zoom").icon("undo").checked(is_rtz).on(move |e, _, cx| e.toggle_shape_return_to_zoom(shape_id, cx)).into());
        let is_bg = editor.shape_zoom_show_bg(shape_id);
        menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.zoom.background").icon("wallpaper").checked(is_bg).on(move |e, _, cx| e.toggle_shape_zoom_show_bg(shape_id, cx)).into());
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.zoom.duration", "timer", zoom_duration_items(editor, Some(shape_id))).into());
    }
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.paragraph.settings").on(|e, w, cx| e.open_paragraph_panel(w, cx)).into());
    if editor.pptx().is_some_and(|s| s.paragraph_panel.is_some()) {
        use super::paragraph_panel::Mode;
        menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.paragraph-actions", "format_line_spacing", vec![
            MenuItem::tr("pptx.paragraph.default").on(|e, _, cx| e.set_paragraph_panel_mode(Mode::Default, cx)).into(),
            MenuItem::tr("pptx.paragraph.multiple").on(|e, _, cx| e.set_paragraph_panel_mode(Mode::Multiple, cx)).into(),
            MenuItem::tr("pptx.paragraph.points").on(|e, _, cx| e.set_paragraph_panel_mode(Mode::Points, cx)).into(),
            Submenu::tr("pptx.paragraph.font-align", "format_align_left", [
                ("pptx.paragraph.font-auto", ibis_doc_model::FontAlign::Auto),
                ("pptx.paragraph.font-top", ibis_doc_model::FontAlign::Top),
                ("pptx.paragraph.font-center", ibis_doc_model::FontAlign::Center),
                ("pptx.paragraph.font-baseline", ibis_doc_model::FontAlign::Baseline),
                ("pptx.paragraph.font-bottom", ibis_doc_model::FontAlign::Bottom),
            ].into_iter().map(|(id, value)| MenuItem::tr(id).on(move |e, _, cx| e.set_paragraph_panel_font_align(value, cx)).into()).collect()).into(),
            MenuItem::tr("pptx.paragraph.apply").on(|e, w, cx| e.apply_paragraph_panel(w, cx)).into(),
            MenuItem::tr("pptx.paragraph.cancel").on(|e, w, cx| e.close_paragraph_panel(w, cx)).into(),
        ]).into());
    }
    menus.connect("edit.find-replace", |e, w, cx| e.open_pptx_find(w, cx));
    menus.items_mut(MenuId::Edit).push(Submenu::tr("pptx.find-menu", "find_replace", vec![
        MenuItem::tr("pptx.find-case").on(|e, _, cx| e.toggle_pptx_find_option(true, cx)).into(),
        MenuItem::tr("pptx.find-word").on(|e, _, cx| e.toggle_pptx_find_option(false, cx)).into(),
        MenuItem::tr("pptx.find-next").on(|e, w, cx| e.pptx_find_next(true, w, cx)).into(),
        MenuItem::tr("pptx.find-previous").on(|e, w, cx| e.pptx_find_next(false, w, cx)).into(),
        MenuItem::tr("pptx.replace-one").on(|e, w, cx| e.pptx_replace(false, w, cx)).into(),
        MenuItem::tr("pptx.replace-all").on(|e, w, cx| e.pptx_replace(true, w, cx)).into(),
    ]).into());
    menus.connect("edit.select-all", |this, window, cx| this.select_all_shapes(window, cx));
    if has {
        if editor.selected_shapes().iter().all(|shape| ibis_pptx::save::check_shape_insertion(shape).is_ok()) {
            menus.connect("edit.duplicate", |this, _, cx| this.duplicate_selected(cx));
            menus.connect("edit.copy", |this, _, cx| {
                this.copy_selected();
                this.write_shapes_to_system_clipboard(cx);
                this.stamp_shape_clipboard(cx);
            });
            menus.connect("edit.cut", |this, _, cx| this.cut_selected(cx));
        }
        for (id, order) in [("pptx.order.front", Order::Front), ("pptx.order.forward", Order::Forward), ("pptx.order.backward", Order::Backward), ("pptx.order.back", Order::Back)] {
            menus.connect(id, move |this, _, cx| this.order_selected(order, cx));
        }
        menus.connect("pptx.rotate.right", |this, _, cx| this.rotate_selected(90, cx));
        menus.connect("pptx.rotate.left", |this, _, cx| this.rotate_selected(-90, cx));
        menus.connect("pptx.rotate.flip-h", |this, _, cx| this.flip_selected(true, cx));
        menus.connect("pptx.rotate.flip-v", |this, _, cx| this.flip_selected(false, cx));
    }
    let align = |id: &'static str, icon: &'static str, a: Align| -> MenuEntry { MenuItem::tr(id).icon(icon).disabled(!has).on(move |this, _, cx| this.align_selected(a, cx)).into() };
    fill_submenu(menus, "pptx.align", vec![
        align("pptx.align.left", "align_horizontal_left", Align::Left),
        align("pptx.align.center", "align_horizontal_center", Align::Center),
        align("pptx.align.right", "align_horizontal_right", Align::Right),
        MenuEntry::Separator,
        align("pptx.align.top", "align_vertical_top", Align::Top),
        align("pptx.align.middle", "align_vertical_center", Align::Middle),
        align("pptx.align.bottom", "align_vertical_bottom", Align::Bottom),
        MenuEntry::Separator,
        MenuItem::tr("pptx.align.to-slide").icon("crop_landscape").checked(editor.pptx().is_some_and(|s| s.align_to_slide)).on(|this, _, cx| {
            if let Some(s) = this.pptx_mut() {
                s.align_to_slide = !s.align_to_slide;
            }
            cx.notify();
        }).into(),
    ]);
    fill_submenu(menus, "pptx.center", vec![
        MenuItem::tr("pptx.center.h").icon("align_horizontal_center").disabled(!has).on(|this, _, cx| this.center_selected(true, cx)).into(),
        MenuItem::tr("pptx.center.v").icon("align_vertical_center").disabled(!has).on(|this, _, cx| this.center_selected(false, cx)).into(),
    ]);
    {
        use ibis_doc_model::shape_merge::MergeOp;
        let can = editor.can_merge_shapes();
        let item = |id: &'static str, icon: &'static str, op: MergeOp| -> MenuEntry { MenuItem::tr(id).icon(icon).disabled(!can).on(move |this, window, cx| this.merge_selected_shapes(op, window, cx)).into() };
        fill_submenu(menus, "pptx.merge", vec![
            item("pptx.merge.union", "cell_merge", MergeOp::Union),
            item("pptx.merge.combine", "cell_merge", MergeOp::Combine),
            item("pptx.merge.fragment", "content_cut", MergeOp::Fragment),
            item("pptx.merge.intersect", "cell_merge", MergeOp::Intersect),
            item("pptx.merge.subtract", "cell_merge", MergeOp::Subtract),
        ]);
    }
    {
        fill_submenu(menus, "pptx.distribute", vec![
            MenuItem::tr("pptx.distribute.h").icon("horizontal_distribute").disabled(editor.selected_ids().len() < 3).on(|this, _, cx| this.distribute_selected(true, cx)).into(),
            MenuItem::tr("pptx.distribute.v").icon("vertical_distribute").disabled(editor.selected_ids().len() < 3).on(|this, _, cx| this.distribute_selected(false, cx)).into(),
        ]);
    }
    menus.items_mut(MenuId::Tools).push(MenuItem::tr("pptx.autocorrect-options").icon("spellcheck").on(|e, w, cx| e.open_pptx_autocorrect_panel(w, cx)).into());
    fill_submenu(menus, "tools.spelling", vec![
        MenuItem::tr("pptx.spelling.check").icon("spellcheck").on(|e, w, cx| e.open_spelling(w, cx)).into(),
        MenuItem::tr("pptx.thesaurus").icon("menu_book").on(|e, w, cx| e.open_pptx_thesaurus(w, cx)).into(),
    ]);
    menus.connect("pptx.thesaurus", |this, window, cx| this.open_pptx_thesaurus(window, cx));
    if editor.can_edit_points() {
        menus.connect("pptx.edit-points", |this, _, cx| this.begin_edit_points(cx));
    }
    // 点の種類 (while vertices are edited and one is selected).
    if let Some(kind) = editor.edit_point_selected_kind() {
        use ibis_doc_model::shape_merge::NodeKind;
        for (id, k) in [("pptx.edit-points.smooth", NodeKind::Smooth), ("pptx.edit-points.straight", NodeKind::Straight), ("pptx.edit-points.corner", NodeKind::Corner)] {
            menus.connect(id, move |this, _, cx| this.set_selected_vertex_kind(k, cx));
            if let Some(item) = menus.find_mut(id) {
                item.checked = kind == k;
            }
        }
    }
    if editor.can_convert_diagram() {
        menus.connect("pptx.smartart-to-shapes", |this, _, cx| this.convert_diagram_to_shapes(cx));
    }
    if editor.can_convert_text_to_smartart() {
        menus.connect("pptx.smartart.convert-text", |this, _, cx| this.convert_selected_text_to_smartart(crate::pptx::SmartArtKind::BasicProcess, cx));
    }
    if editor.can_modify_smartart_items() {
        menus.connect("pptx.smartart.add-shape", |this, _, cx| this.smartart_add_shape(cx));
        menus.connect("pptx.smartart.remove-shape", |this, _, cx| this.smartart_remove_shape(cx));
        menus.connect("pptx.smartart.add-assistant", |this, _, cx| this.smartart_add_assistant(cx));
        menus.connect("pptx.smartart.move-up", |this, _, cx| this.smartart_move_item_up(cx));
        menus.connect("pptx.smartart.move-down", |this, _, cx| this.smartart_move_item_down(cx));
        menus.connect("pptx.smartart.promote", |this, _, cx| this.smartart_promote(cx));
        menus.connect("pptx.smartart.demote", |this, _, cx| this.smartart_demote(cx));
        menus.connect("pptx.smartart.text-pane", |this, _, cx| this.toggle_smartart_text_pane(cx));

        menus.connect("pptx.smartart.layout.list", |this, _, cx| this.change_smartart_layout(crate::pptx::SmartArtKind::BasicBlockList, cx));
        menus.connect("pptx.smartart.layout.vlist", |this, _, cx| this.change_smartart_layout(crate::pptx::SmartArtKind::VerticalList, cx));
        menus.connect("pptx.smartart.layout.process", |this, _, cx| this.change_smartart_layout(crate::pptx::SmartArtKind::BasicProcess, cx));
        menus.connect("pptx.smartart.layout.chevron", |this, _, cx| this.change_smartart_layout(crate::pptx::SmartArtKind::ContinuousChevronProcess, cx));
        menus.connect("pptx.smartart.layout.cycle", |this, _, cx| this.change_smartart_layout(crate::pptx::SmartArtKind::BasicCycle, cx));
        menus.connect("pptx.smartart.layout.hierarchy", |this, _, cx| this.change_smartart_layout(crate::pptx::SmartArtKind::OrganizationChart, cx));
        menus.connect("pptx.smartart.layout.relationship", |this, _, cx| this.change_smartart_layout(crate::pptx::SmartArtKind::BasicRelationship, cx));
        menus.connect("pptx.smartart.layout.matrix", |this, _, cx| this.change_smartart_layout(crate::pptx::SmartArtKind::BasicMatrix, cx));
        menus.connect("pptx.smartart.layout.pyramid", |this, _, cx| this.change_smartart_layout(crate::pptx::SmartArtKind::BasicPyramid, cx));
        menus.connect("pptx.smartart.layout.picture", |this, _, cx| this.change_smartart_layout(crate::pptx::SmartArtKind::PictureList, cx));

        menus.connect("pptx.smartart.color.colorful", |this, _, cx| this.set_smartart_color_style(crate::pptx::SmartArtColorStyle::Colorful, cx));
        menus.connect("pptx.smartart.color.accent1", |this, _, cx| this.set_smartart_color_style(crate::pptx::SmartArtColorStyle::Accent1, cx));
        menus.connect("pptx.smartart.color.accent2", |this, _, cx| this.set_smartart_color_style(crate::pptx::SmartArtColorStyle::Accent2, cx));
        menus.connect("pptx.smartart.color.accent3", |this, _, cx| this.set_smartart_color_style(crate::pptx::SmartArtColorStyle::Accent3, cx));
        menus.connect("pptx.smartart.color.outline", |this, _, cx| this.set_smartart_color_style(crate::pptx::SmartArtColorStyle::Outline, cx));

        if let Some((_, kind, style, ..)) = editor.selected_smartart_data() {
            let layout_id = match kind {
                crate::pptx::SmartArtKind::BasicBlockList => "pptx.smartart.layout.list",
                crate::pptx::SmartArtKind::VerticalList => "pptx.smartart.layout.vlist",
                crate::pptx::SmartArtKind::BasicProcess => "pptx.smartart.layout.process",
                crate::pptx::SmartArtKind::ContinuousChevronProcess => "pptx.smartart.layout.chevron",
                crate::pptx::SmartArtKind::BasicCycle => "pptx.smartart.layout.cycle",
                crate::pptx::SmartArtKind::OrganizationChart => "pptx.smartart.layout.hierarchy",
                crate::pptx::SmartArtKind::BasicRelationship => "pptx.smartart.layout.relationship",
                crate::pptx::SmartArtKind::BasicMatrix => "pptx.smartart.layout.matrix",
                crate::pptx::SmartArtKind::BasicPyramid => "pptx.smartart.layout.pyramid",
                crate::pptx::SmartArtKind::PictureList => "pptx.smartart.layout.picture",
            };
            if let Some(item) = menus.find_mut(layout_id) {
                item.checked = true;
            }
            let color_id = match style {
                crate::pptx::SmartArtColorStyle::Colorful => "pptx.smartart.color.colorful",
                crate::pptx::SmartArtColorStyle::Accent1 => "pptx.smartart.color.accent1",
                crate::pptx::SmartArtColorStyle::Accent2 => "pptx.smartart.color.accent2",
                crate::pptx::SmartArtColorStyle::Accent3 => "pptx.smartart.color.accent3",
                crate::pptx::SmartArtColorStyle::Outline => "pptx.smartart.color.outline",
            };
            if let Some(item) = menus.find_mut(color_id) {
                item.checked = true;
            }
        }
        if let Some(item) = menus.find_mut("pptx.smartart.text-pane") {
            item.checked = editor.pptx().is_some_and(|s| s.smartart_text_pane);
        }
    }
    if editor.can_group() {
        menus.connect("pptx.group", |this, _, cx| this.group_selected(cx));
    }
    if editor.can_ungroup() {
        menus.connect("pptx.ungroup", |this, _, cx| this.ungroup_selected(cx));
    }
    // 貼り付け: into the text edited, else the shapes copied, else the clipboard's text as a text box.
    // (Not when the shapes copied cannot be pasted and nothing is edited.)
    if editor.text_editing().is_some() || editor.pptx().is_none_or(|s| s.clipboard.is_empty() || SlideEditor::can_copy_shapes(&s.clipboard)) {
        menus.connect("edit.paste", |this, window, cx| {
            if !this.paste_edited_text(cx) {
                this.paste_from_clipboards(window, cx);
            }
        });
    }
    // 貼り付け（書式なし・書式を合わせる）: what is pasted is the clipboard's text, which takes the place's own format.
    if editor.text_editing().is_some() || editor.pptx().is_none_or(|s| s.clipboard.is_empty() || SlideEditor::can_copy_shapes(&s.clipboard)) {
        for id in ["edit.paste-plain", "edit.paste-match"] {
            // ⌥⇧⌘V is also the animation painter's paste (a copied animation, pasted onto the selected shape): it keeps that.
            if id == "edit.paste-match" && editor.can_paste_animations() {
                continue;
            }
            menus.connect(id, |this, window, cx| {
                if !this.paste_edited_text(cx) {
                    this.paste_from_clipboards(window, cx);
                }
            });
        }
    }
    // With a text edited, ⌘C and ⌘X take the selected text, not the shape.
    if editor.text_editing().is_some() {
        menus.connect("edit.copy", |this, _, cx| {
            this.copy_edited_text(false, cx);
        });
        menus.connect("edit.cut", |this, _, cx| {
            this.copy_edited_text(true, cx);
        });
    }
    fill_submenu(menus, "insert.shape", shape_items());
    fill_submenu(menus, "insert.table", table_size_items());
    fill_submenu(
        menus,
        "insert.chart",
        vec![
            MenuItem::tr("pptx.insert-chart.column").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::ClusteredColumn, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.stacked-column").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::StackedColumn, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.percent-stacked-column").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::PercentStackedColumn, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.3d-clustered-column").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::ClusteredColumn3D, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.3d-stacked-column").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::StackedColumn3D, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.3d-percent-stacked-column").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::PercentStackedColumn3D, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.3d-column").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Column3D, w, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.insert-chart.bar").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::ClusteredBar, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.stacked-bar").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::StackedBar, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.percent-stacked-bar").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::PercentStackedBar, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.3d-clustered-bar").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::ClusteredBar3D, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.3d-stacked-bar").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::StackedBar3D, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.3d-percent-stacked-bar").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::PercentStackedBar3D, w, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.insert-chart.line").icon("show_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Line, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.stacked-line").icon("show_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::StackedLine, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.percent-stacked-line").icon("show_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::PercentStackedLine, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.3d-line").icon("show_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Line3D, w, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.insert-chart.area").icon("area_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Area, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.stacked-area").icon("area_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::StackedArea, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.percent-stacked-area").icon("area_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::PercentStackedArea, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.3d-area").icon("area_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Area3D, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.3d-stacked-area").icon("area_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::StackedArea3D, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.3d-percent-stacked-area").icon("area_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::PercentStackedArea3D, w, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.insert-chart.pie").icon("pie_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Pie, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.3d-pie").icon("pie_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Pie3D, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.doughnut").icon("donut_large").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Doughnut, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.pie-of-pie").icon("pie_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::PieOfPie, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.bar-of-pie").icon("pie_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::BarOfPie, w, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.insert-chart.scatter").icon("polyline").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Scatter, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.scatter-smooth").icon("polyline").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::ScatterSmooth, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.scatter-lines").icon("polyline").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::ScatterLines, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.bubble").icon("dataset").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Bubble, w, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.chrome.radar").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Radar, w, cx)).into(),
            MenuItem::tr("pptx.chrome.filled-radar").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::RadarFilled, w, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.insert-chart.stock-hlc").icon("show_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::StockHLC, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.stock-ohlc").icon("show_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::StockOHLC, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.stock-vhlc").icon("show_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::StockVHLC, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.stock-vohlc").icon("show_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::StockVOHLC, w, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.insert-chart.surface-3d").icon("area_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Surface3D, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.surface-3d-wireframe").icon("area_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Surface3DWireframe, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.contour").icon("area_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Contour, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.contour-wireframe").icon("area_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::ContourWireframe, w, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.insert-chart.combo-column-line").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::ComboColumnLine, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.combo-column-line-secondary").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::ComboColumnLineSecondaryAxis, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.combo-area-column").icon("area_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::ComboAreaColumn, w, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.insert-chart.waterfall").icon("waterfall_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Waterfall, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.histogram").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Histogram, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.pareto").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Pareto, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.funnel").icon("filter_alt").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Funnel, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.box-whisker").icon("bar_chart").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::BoxWhisker, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.treemap").icon("grid_view").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Treemap, w, cx)).into(),
            MenuItem::tr("pptx.insert-chart.sunburst").icon("donut_large").on(|e, w, cx| e.insert_slide_chart_kind(ibis_pptx::NewChartKind::Sunburst, w, cx)).into(),
        ],
    );
    fill_submenu(menus, "insert.smartart", smartart_insert_items());
    // 表示形式 > テキスト: 上付き・下付き・大文字・文字の間隔・大文字と小文字の変更.
    let baseline = editor.selected_baseline();
    let caps = editor.selected_caps();
    let shadow_on = editor.selected_shadow().is_some();
    let spacing = editor.selected_spacing();
    fill_submenu(menus, "format.text", vec![
        MenuItem::tr("pptx.text.shadow").checked(shadow_on).on(|e, w, cx| e.toggle_text_shadow(w, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("text.superscript").shortcut("⇧⌘+").checked(baseline > 0).on(|e, w, cx| e.toggle_baseline(30_000, w, cx)).into(),
        MenuItem::tr("text.subscript").shortcut("⌘=").checked(baseline < 0).on(|e, w, cx| e.toggle_baseline(-25_000, w, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("text.caps").checked(caps == ibis_doc_model::Caps::All).on(|e, w, cx| e.toggle_text_caps(ibis_doc_model::Caps::All, w, cx)).into(),
        MenuItem::tr("text.small-caps").checked(caps == ibis_doc_model::Caps::Small).on(|e, w, cx| e.toggle_text_caps(ibis_doc_model::Caps::Small, w, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("text.spacing-very-tight").checked(spacing == -300).on(|e, w, cx| e.set_slide_character_spacing(-300, w, cx)).into(),
        MenuItem::tr("text.spacing-tight").checked(spacing == -150).on(|e, w, cx| e.set_slide_character_spacing(-150, w, cx)).into(),
        MenuItem::tr("text.spacing-normal").checked(spacing == 0).on(|e, w, cx| e.set_slide_character_spacing(0, w, cx)).into(),
        MenuItem::tr("text.spacing-loose").checked(spacing == 300).on(|e, w, cx| e.set_slide_character_spacing(300, w, cx)).into(),
        MenuItem::tr("text.spacing-very-loose").checked(spacing == 600).on(|e, w, cx| e.set_slide_character_spacing(600, w, cx)).into(),
        MenuEntry::Separator,
        Submenu::tr("pptx.text.case", "format_size", vec![
            MenuItem::tr("pptx.case.sentence").on(|e, w, cx| e.change_case(crate::pptx::editing::TextCase::Sentence, w, cx)).into(),
            MenuItem::tr("pptx.case.lower").on(|e, w, cx| e.change_case(crate::pptx::editing::TextCase::Lower, w, cx)).into(),
            MenuItem::tr("pptx.case.upper").on(|e, w, cx| e.change_case(crate::pptx::editing::TextCase::Upper, w, cx)).into(),
            MenuItem::tr("pptx.case.title").on(|e, w, cx| e.change_case(crate::pptx::editing::TextCase::Title, w, cx)).into(),
            MenuItem::tr("pptx.case.toggle").on(|e, w, cx| e.change_case(crate::pptx::editing::TextCase::Toggle, w, cx)).into(),
        ]).into(),
    ]);
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.proofing-language", "language", proofing_language_menu_items(editor)).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.format-copy-menu", "format_paint", format_copy_items(editor)).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.text-body", "format_textdirection_l_to_r", text_body_items(editor)).into());
    let text_selected = editor.selected_text_style().is_some();
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.text.shadow", "format_color_fill", [
        ("pptx.text.shadow.none", None),
        ("pptx.text.shadow.offset-diagonal-bottom-right", Some((2_700_000, 19_050, 25_400))),
        ("pptx.text.shadow.offset-bottom", Some((5_400_000, 19_050, 25_400))),
        ("pptx.text.shadow.offset-right", Some((0, 19_050, 25_400))),
        ("pptx.text.shadow.offset-diagonal-bottom-left", Some((8_100_000, 19_050, 25_400))),
    ].into_iter().map(|(id, value)| {
        let preset = value.map(|(dir, dist, blur)| ibis_doc_model::Shadow {
            color: Color { rgb: Rgb(0), alpha: 102 },
            blur,
            distance: dist,
            direction: dir,
            sx: 100_000,
            sy: 100_000,
            kx: 0,
            ky: 0,
            align: ibis_doc_model::TileAlign::TopLeft,
            rot_with_shape: true,
        });
        MenuItem::tr(id).disabled(!text_selected).on(move |e, w, cx| e.set_text_shadow(preset, w, cx)).into()
    }).collect()).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.text.warp", "transform", text_warp_items(editor)).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.text.wordart-styles", "title", wordart_style_items(editor)).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.shadow", "format_color_fill",[
        ("pptx.shadow.none",None),
        ("pptx.shadow.right",Some((0,0))),
        ("pptx.shadow.diagonal",Some((2_700_000,0))),
        ("pptx.shadow.soft",Some((2_700_000,50_800))),
    ].into_iter().map(|(id,value)| {
        let shadow=value.map(|(direction,blur)| ibis_doc_model::Shadow::simple(Color {rgb:Rgb(0),alpha:128},blur,50_800,direction));
        MenuItem::tr(id).disabled(!editor.selected_shapes().iter().any(|s|matches!(s.kind,ShapeKind::Auto(_) | ShapeKind::Picture(_)))).on(move |e,_,cx|e.set_shape_shadow(shadow,cx)).into()
    }).collect()).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.fill.custom").disabled(editor.selected_shapes().is_empty() || editor.selected_shapes().iter().any(|s|!matches!(s.kind,ShapeKind::Auto(_)|ShapeKind::Table(_)))).on(|e,w,cx|e.open_gradient_panel(false,w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.background.custom").on(|e,w,cx|e.open_gradient_panel(true,w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.fill.custom-path").disabled(editor.selected_shapes().is_empty() || editor.selected_shapes().iter().any(|s|!matches!(s.kind,ShapeKind::Auto(_)|ShapeKind::Table(_)))).on(|e,w,cx|e.open_path_gradient_panel(false,w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.background.custom-path").on(|e,w,cx|e.open_path_gradient_panel(true,w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.fill.custom-pattern").disabled(editor.selected_shapes().is_empty() || editor.selected_shapes().iter().any(|s|!matches!(s.kind,ShapeKind::Auto(_)|ShapeKind::Table(_)))).on(|e,w,cx|e.open_pattern_panel(false,w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.background.custom-pattern").on(|e,w,cx|e.open_pattern_panel(true,w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.fill.picture").disabled(editor.selected_shapes().is_empty() || editor.selected_shapes().iter().any(|s|!matches!(s.kind,ShapeKind::Auto(_)|ShapeKind::Table(_)))).on(|e,w,cx|e.pick_fill_picture(w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.background.picture").on(|e,w,cx|e.pick_background_picture(w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.glow.custom").disabled(editor.selected_shapes().is_empty() || editor.selected_shapes().iter().any(|s| !matches!(s.kind,ShapeKind::Auto(_)))).on(|e,w,cx|e.open_glow_panel(w,cx)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.shadow.custom").disabled(editor.selected_shapes().is_empty() || editor.selected_shapes().iter().any(|s| !matches!(s.kind,ShapeKind::Auto(_)))).on(|e,w,cx|e.open_shadow_panel(w,cx)).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.glow", "format_color_fill",[
        ("pptx.glow.none",None),
        ("pptx.glow.blue5",Some((Color { rgb:Rgb(0x4472C4),alpha:128 },63_500))),
        ("pptx.glow.blue10",Some((Color { rgb:Rgb(0x4472C4),alpha:128 },127_000))),
        ("pptx.glow.yellow10",Some((Color { rgb:Rgb(0xFFC000),alpha:160 },127_000))),
    ].into_iter().map(|(id,value)| MenuItem::tr(id).disabled(!editor.selected_shapes().iter().any(|s| matches!(s.kind,ShapeKind::Auto(_)))).on(move |e,_,cx| e.set_shape_glow(value,cx)).into()).collect()).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.reflection", "flip", [
        ("pptx.reflection.none", None),
        ("pptx.reflection.tight", Some(reflection_preset(0, 35_000))),
        ("pptx.reflection.half", Some(reflection_preset(38_100, 55_000))),
        ("pptx.reflection.full", Some(reflection_preset(38_100, 90_000))),
    ].into_iter().map(|(id, value)| MenuItem::tr(id).disabled(!editor.selected_shapes().iter().any(|s| matches!(s.kind, ShapeKind::Auto(_) | ShapeKind::Picture(_)))).on(move |e, _, cx| e.set_shape_reflection(value, cx)).into()).collect()).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.soft-edge", "auto_awesome", [
        ("pptx.soft-edge.none", None),
        ("pptx.soft-edge.1pt", Some(12_700)),
        ("pptx.soft-edge.2-5pt", Some(31_750)),
        ("pptx.soft-edge.5pt", Some(63_500)),
        ("pptx.soft-edge.10pt", Some(127_000)),
        ("pptx.soft-edge.25pt", Some(317_500)),
        ("pptx.soft-edge.50pt", Some(635_000)),
    ].into_iter().map(|(id, value)| MenuItem::tr(id).disabled(!editor.selected_shapes().iter().any(|s| matches!(s.kind, ShapeKind::Auto(_) | ShapeKind::Picture(_)))).on(move |e, _, cx| e.set_shape_soft_edge(value, cx)).into()).collect()).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.bevel", "layers", [
        ("pptx.bevel.none", None),
        ("pptx.bevel.circle", Some(ibis_doc_model::Bevel::standard(ibis_doc_model::BevelPreset::Circle))),
        ("pptx.bevel.relaxed-inset", Some(ibis_doc_model::Bevel::standard(ibis_doc_model::BevelPreset::RelaxedInset))),
        ("pptx.bevel.slope", Some(ibis_doc_model::Bevel::standard(ibis_doc_model::BevelPreset::Slope))),
        ("pptx.bevel.cross", Some(ibis_doc_model::Bevel::standard(ibis_doc_model::BevelPreset::Cross))),
        ("pptx.bevel.angle", Some(ibis_doc_model::Bevel::standard(ibis_doc_model::BevelPreset::Angle))),
        ("pptx.bevel.soft-round", Some(ibis_doc_model::Bevel::standard(ibis_doc_model::BevelPreset::SoftRound))),
        ("pptx.bevel.convex", Some(ibis_doc_model::Bevel::standard(ibis_doc_model::BevelPreset::Convex))),
        ("pptx.bevel.cool-slant", Some(ibis_doc_model::Bevel::standard(ibis_doc_model::BevelPreset::CoolSlant))),
        ("pptx.bevel.hard-edge", Some(ibis_doc_model::Bevel::standard(ibis_doc_model::BevelPreset::HardEdge))),
        ("pptx.bevel.art-deco", Some(ibis_doc_model::Bevel::standard(ibis_doc_model::BevelPreset::ArtDeco))),
    ].into_iter().map(|(id, value)| MenuItem::tr(id).disabled(!editor.selected_shapes().iter().any(|s| matches!(s.kind, ShapeKind::Auto(_)))).on(move |e, _, cx| e.set_shape_bevel(value, cx)).into()).collect()).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.3d-rotation", "3d_rotation", [
        ("pptx.3d-rotation.none", None),
        ("pptx.3d-rotation.isometric-right-up", Some(ibis_doc_model::CameraPreset::IsometricRightUp)),
        ("pptx.3d-rotation.isometric-left-up", Some(ibis_doc_model::CameraPreset::IsometricLeftUp)),
        ("pptx.3d-rotation.isometric-top-up", Some(ibis_doc_model::CameraPreset::IsometricTopUp)),
        ("pptx.3d-rotation.off-axis-1-top", Some(ibis_doc_model::CameraPreset::IsometricOffAxis1Top)),
        ("pptx.3d-rotation.off-axis-1-left", Some(ibis_doc_model::CameraPreset::IsometricOffAxis1Left)),
        ("pptx.3d-rotation.off-axis-1-right", Some(ibis_doc_model::CameraPreset::IsometricOffAxis1Right)),
        ("pptx.3d-rotation.perspective-front", Some(ibis_doc_model::CameraPreset::PerspectiveFront)),
        ("pptx.3d-rotation.perspective-above", Some(ibis_doc_model::CameraPreset::PerspectiveAbove)),
        ("pptx.3d-rotation.perspective-below", Some(ibis_doc_model::CameraPreset::PerspectiveBelow)),
        ("pptx.3d-rotation.perspective-left", Some(ibis_doc_model::CameraPreset::PerspectiveLeft)),
        ("pptx.3d-rotation.perspective-right", Some(ibis_doc_model::CameraPreset::PerspectiveRight)),
        ("pptx.3d-rotation.oblique-top-left", Some(ibis_doc_model::CameraPreset::ObliqueTopLeft)),
        ("pptx.3d-rotation.oblique-top-right", Some(ibis_doc_model::CameraPreset::ObliqueTopRight)),
    ].into_iter().map(|(id, value)| MenuItem::tr(id).disabled(!editor.selected_shapes().iter().any(|s| matches!(s.kind, ShapeKind::Auto(_)))).on(move |e, _, cx| e.set_shape_3d_rotation_preset(value, cx)).into()).collect()).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.3d-depth", "view_in_ar", [
        ("pptx.3d-depth.none", 0),
        ("pptx.3d-depth.10pt", 127_000),
        ("pptx.3d-depth.20pt", 254_000),
        ("pptx.3d-depth.36pt", 457_200),
        ("pptx.3d-depth.72pt", 914_400),
    ].into_iter().map(|(id, value)| MenuItem::tr(id).disabled(!editor.selected_shapes().iter().any(|s| matches!(s.kind, ShapeKind::Auto(_)))).on(move |e, _, cx| e.set_shape_3d_depth(value, None, cx)).into()).collect()).into());
    let has_pic = editor.selected_shapes().iter().any(|s| matches!(s.kind, ShapeKind::Picture(_)));
    let has_svg = editor.has_selected_svg_picture();
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.picture.convert-to-shape")
        .icon("category")
        .disabled(!has_svg)
        .on(|e, _, cx| { e.convert_selected_svg_to_shape(cx); }).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.picture.color", "palette", picture_color_items(has_pic)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.picture.remove-background")
        .icon("auto_fix_high")
        .disabled(!has_pic)
        .on(|e, _, cx| { e.set_selected_pictures_clear(Some(ibis_doc_model::Rgb(0xffffff)), cx); }).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.picture.correction", "tune", picture_correction_items(editor, has_pic)).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.picture.artistic-effect", "auto_fix_high", picture_artistic_effect_items(has_pic)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.picture.reset-colors")
        .disabled(!editor.selected_shapes().iter().any(|s| matches!(&s.kind, ShapeKind::Picture(p) if p.duotone.is_some() || p.clear.is_some() || p.luminance.is_some() || p.artistic_effect.is_some())))
        .on(|e, _, cx| e.reset_picture_color_effects(cx)).into());
    #[cfg(all(target_os = "macos", feature = "export"))]
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.picture.compress").icon("image").on(|e, w, cx| e.open_compress_panel(w, cx)).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.media.compress", "video_settings", vec![
        MenuItem::tr("pptx.media.compress-1080p").on(|e, _, cx| { e.compress_presentation_media(super::MediaQuality::FullHd, cx); }).into(),
        MenuItem::tr("pptx.media.compress-720p").on(|e, _, cx| { e.compress_presentation_media(super::MediaQuality::Hd, cx); }).into(),
        MenuItem::tr("pptx.media.compress-480p").on(|e, _, cx| { e.compress_presentation_media(super::MediaQuality::Standard, cx); }).into(),
        MenuItem::tr("pptx.media.optimize-compatibility").on(|e, _, cx| { e.optimize_media_compatibility(cx); }).into(),
    ]).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.clip.playback", "play_circle", clip_playback_items(editor)).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.video.styles", "style", video_style_items(editor, has_pic)).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.video.shape", "crop_original", video_shape_items(editor, has_pic)).into());
    menus.items_mut(MenuId::Format).push(MenuItem::tr("pptx.video.reset-effects")
        .disabled(!editor.selected_shapes().iter().any(|s| matches!(&s.kind, ShapeKind::Picture(p) if p.stroke.is_some() || p.corner > 0 || p.shadow.is_some() || p.reflection.is_some() || p.soft_edge.is_some() || p.artistic_effect.is_some())))
        .on(|e, _, cx| e.reset_picture_effects(cx)).into());
    let has_table = editor.selected_shapes().iter().any(|s| matches!(s.kind, ShapeKind::Table(_)));
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.table-style", "grid_view", table_style_items(editor, has_table)).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.table-style-options", "table", table_style_option_items(editor, has_table)).into());
    menus.items_mut(MenuId::Format).push(Submenu::tr("pptx.menu.animation", "animation", super::animation_ui::animation_items(editor)).into());
    menus.connect("format.clear", |this, window, cx| this.clear_formats(window, cx));
    fill_submenu(menus, "insert.image", image_items());
    let mut screenshot_entry: MenuEntry = Submenu::tr("pptx.insert.screenshot", "fit_screen", screenshot_menu_items()).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "insert.image", screenshot_entry) {
            Ok(()) => break,
            Err(back) => screenshot_entry = back,
        }
    }
    let mut icon_entry: MenuEntry = Submenu::tr("pptx.insert.icon", "category", icon_menu_items()).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "pptx.insert.screenshot", icon_entry) {
            Ok(()) => break,
            Err(back) => icon_entry = back,
        }
    }
    menus.connect("pptx.insert.screenshot.available-windows", |this, _, cx| {
        this.insert_screenshot_window(cx);
    });
    menus.connect("pptx.insert.screenshot.clipping", |this, _, cx| {
        this.insert_screenshot_clipping(cx);
    });
    menus.connect("pptx.insert.screenshot.clipboard", |this, _, cx| {
        this.insert_screenshot_from_clipboard(cx);
    });
    fill_submenu(menus, "pptx.layout", apply_layout_items(editor));
    let has_text = editor.edited_body().is_some() || editor.selected_shapes().into_iter().any(|s| s.text_body().is_some());
    if has_text {
        menus.connect("edit.copy-format", |this, window, cx| this.copy_character_format(window, cx));
        if editor.pptx().is_some_and(|s| s.character_format_copy.is_some()) {
            menus.connect("edit.paste-format", |this, window, cx| this.paste_character_format(window, cx));
        }
    }
    menus.connect("file.page-setup", |this, window, cx| this.open_slide_size_panel(window, cx));
    menus.connect("format.options", |this, _, cx| {
        this.sidebar_open = true;
        cx.notify();
    });
    menus.connect("insert.video", |this, window, cx| this.pick_clip(false, window, cx));
    menus.connect("insert.audio", |this, window, cx| this.pick_clip(true, window, cx));
    menus.connect("insert.equation", |this, window, cx| this.open_pptx_math_panel(window, cx));
    menus.connect("insert.link", |this, window, cx| this.slide_open_link_panel(window, cx));
    menus.connect("tools.dictionary", |this, window, cx| this.open_pptx_thesaurus(window, cx));
    menus.connect("tools.word-count", |this, _, cx| this.show_word_count(cx));
    menus.connect("tools.accessibility", |this, window, cx| this.open_pptx_accessibility_panel(window, cx));
    menus.connect("pptx.motion", |this, _, cx| this.toggle_animation_pane(cx));
    if let Some(item) = menus.find_mut("pptx.motion") {
        item.checked = editor.pptx().is_some_and(|s| s.animation_pane);
    }
    menus.connect("pptx.edit-theme", |this, window, cx| {
        if this.is_master_view() {
            this.close_master_view(window, cx);
        } else {
            this.open_master_view(window, cx);
        }
    });
    if let Some(item) = menus.find_mut("pptx.edit-theme") {
        item.checked = editor.is_master_view();
    }
    // 新しいスライド from a chosen layout, after 新しいスライド.
    let slide_menu = menus.items_mut(MenuId::Slide);
    if let Some(pos) = slide_menu.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "pptx.transition")) {
        slide_menu[pos] = Submenu::tr("pptx.transition", "animation", transition_menu(editor)).into();
    }
    if let Some(pos) = slide_menu.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "pptx.theme")) {
        slide_menu[pos] = Submenu::tr("pptx.theme", "palette", theme_menu(editor)).into();
    }
    slide_menu.push(MenuItem::tr("pptx.reset-slide").icon("restart_alt").disabled(editor.slide().layout.is_none()).on(|e, w, cx| e.reset_slide(w, cx)).into());
    slide_menu.push(MenuItem::tr("pptx.reset-placement").disabled(editor.slide().layout.is_none()).on(|e, w, cx| e.reset_placeholder_frames(w, cx)).into());
    slide_menu.push(Submenu::tr("pptx.sections", "view_quilt", section_menu_items(editor)).into());
    slide_menu.push(Submenu::tr("pptx.slide-size", "aspect_ratio", slide_size_menu_items(editor)).into());
    slide_menu.push(MenuItem::tr("pptx.export-slide-png").icon("image").on(|e, w, cx| e.export_current_slide_png(w, cx)).into());
    slide_menu.push(MenuItem::tr("pptx.export-all-slides-png").icon("image").on(|e, w, cx| e.export_all_slides_png(w, cx)).into());
    slide_menu.push(MenuItem::tr("pptx.export-gif").icon("animation").on(|e, w, cx| e.export_gif(w, cx)).into());
    let at = slide_menu.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "pptx.new-slide")).map_or(0, |i| i + 1);
    slide_menu.insert(at, Submenu::tr("pptx.new-slide-layout", "view_quilt", new_slide_menu(editor)).into());
    menus.connect("insert.text-box", |this, window, cx| this.set_pptx_tool(Tool::TextBox, window, cx));

    // ファイル > 書き出し > PowerPoint スライドショー（.ppsx）
    let mut ppsx_entry: MenuEntry = MenuItem::tr("pptx.export-ppsx").icon("slideshow").on(|this, window, cx| this.export_ppsx(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "file.export.svg", ppsx_entry) {
            Ok(()) => break,
            Err(back) => ppsx_entry = back,
        }
    }

    // ファイル > 書き出し > PowerPoint テンプレート（.potx）
    let mut potx_entry: MenuEntry = MenuItem::tr("pptx.export-potx").icon("description").on(|this, window, cx| this.export_potx(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "pptx.export-ppsx", potx_entry) {
            Ok(()) => break,
            Err(back) => potx_entry = back,
        }
    }

    // ファイル > 書き出し > Office テーマ（.thmx）
    let mut thmx_entry: MenuEntry = MenuItem::tr("pptx.export-thmx").icon("palette").on(|this, window, cx| this.export_thmx(window, cx)).into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "pptx.export-potx", thmx_entry) {
            Ok(()) => break,
            Err(back) => thmx_entry = back,
        }
    }

    // ファイル > 新規 > テンプレートから新規作成…
    let mut tmpl_entry: MenuEntry = MenuItem::tr("pptx.new-from-template")
        .icon("dashboard_customize")
        .shortcut("⇧⌘P")
        .on(|this, window, cx| this.new_from_template(window, cx))
        .into();
    for (_, entries) in menus.menus.iter_mut() {
        match insert_after(entries, "file.new.presentation", tmpl_entry) {
            Ok(()) => break,
            Err(back) => tmpl_entry = back,
        }
    }
}

/// Put `new` after the item or submenu `id` (in whatever menu or submenu it is).
fn insert_after(entries: &mut Vec<MenuEntry>, id: &str, new: MenuEntry) -> Result<(), MenuEntry> {
    if let Some(i) = entries.iter().position(|e| match e {
        MenuEntry::Item(item) => item.id == id,
        MenuEntry::Submenu(sub) => sub.id == id,
        _ => false,
    }) {
        entries.insert(i + 1, new);
        return Ok(());
    }
    let mut new = new;
    for e in entries.iter_mut() {
        if let MenuEntry::Submenu(sub) = e {
            match insert_after(&mut sub.items, id, new) {
                Ok(()) => return Ok(()),
                Err(back) => new = back,
            }
        }
    }
    Err(new)
}

pub(crate) fn toolbar_left(editor: &SlideEditor) -> Vec<ToolItem> {
    let _ = editor;
    Vec::new()
}

/// レイアウトを適用: each layout, the current slide's checked.
pub(crate) fn apply_layout_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let current = editor.slide().layout;
    let mut items: Vec<MenuEntry> = editor
        .presentation()
        .layouts
        .iter()
        .map(|layout| {
            let id = layout.id;
            let name = if layout.name.is_empty() { crate::shell::i18n::text("pptx.chrome.no-name").to_string() } else { layout.name.clone() };
            MenuItem::new("pptx.layout.apply", name).icon("view_quilt").checked(current == Some(id)).on(move |this, window, cx| this.apply_layout(id, window, cx)).into()
        })
        .collect();
    let has_layout = current.is_some_and(|id| editor.presentation().layout(id).is_some());
    items.push(MenuEntry::Separator);
    items.push(MenuItem::tr("pptx.layout.rename").disabled(!has_layout).on(|e, w, cx| e.open_layout_rename(w, cx)).into());
    items.push(MenuItem::tr("pptx.layout.duplicate").disabled(!has_layout).on(|e, w, cx| e.duplicate_layout(w, cx)).into());
    // The masters, each to rename, copy or remove.
    let many = editor.presentation().masters.len() > 1;
    let masters: Vec<MenuEntry> = editor
        .presentation()
        .masters
        .iter()
        .map(|master| {
            let id = master.id;
            let name = if master.name.is_empty() { crate::shell::i18n::text("pptx.chrome.no-name").to_string() } else { master.name.clone() };
            Submenu::new(
                "pptx.layout.master",
                "view_quilt",
                name,
                vec![
                    MenuItem::tr("pptx.layout.master.rename").on(move |e, w, cx| e.open_master_rename(id, w, cx)).into(),
                    MenuItem::tr("pptx.layout.master.duplicate").on(move |e, w, cx| e.duplicate_master(id, w, cx)).into(),
                    MenuItem::tr("pptx.layout.master.remove").disabled(!many || editor.master_in_use(id)).on(move |e, w, cx| e.remove_master(id, w, cx)).into(),
                ],
            )
            .into()
        })
        .collect();
    if !masters.is_empty() {
        items.push(Submenu::tr("pptx.layout.masters", "view_quilt", masters).into());
    }
    items
}

/// A new slide from each layout.
pub(crate) fn new_slide_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    layout_items(editor)
}

/// The colours a button offers, as a palette (`shell/color_palette.rs`): "none" when `none` names it, the
/// presentation's theme colours with their tints, the standard colours, those used lately.
fn color_menu(editor: &SlideEditor, _id: &'static str, none: Option<SharedString>, f: impl Fn(&mut SlideEditor, Option<Rgb>, &mut Window, &mut Context<SlideEditor>) + Clone + 'static) -> Vec<MenuEntry> {
    use crate::shell::color_palette::{theme_rows, Palette, PickColor, Swatch};
    let pick: PickColor = std::rc::Rc::new(f.clone());
    let none = none.map(|label| {
        let g = f.clone();
        MenuItem::new("color.none", label).icon("format_color_reset").on(move |this, window, cx| g(this, None, window, cx))
    });
    let theme_color = |name: &str| editor.presentation().theme_colors.iter().find(|(n, _)| n == name).map(|(_, c)| *c);
    let names = ["lt1", "dk1", "lt2", "dk2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6"];
    let theme = match names.map(theme_color) {
        [Some(a), Some(b), Some(c), Some(d), Some(e), Some(f2), Some(g), Some(h), Some(i), Some(j)] => theme_rows(&[a, b, c, d, e, f2, g, h, i, j], &pick, None),
        _ => Vec::new(),
    };
    // The standard colours: the ten of the colour list that are not shades of grey.
    let standard = COLORS
        .iter()
        .filter(|(rgb, _)| !matches!(rgb, 0x000000 | 0x404040 | 0x7f7f7f | 0xd9d9d9 | 0xffffff | 0x4472c4))
        .map(|&(rgb, id)| Swatch::with_id(id, Rgb(rgb), crate::shell::i18n::text(id), false, &pick))
        .collect();
    vec![MenuEntry::Palette(Palette { none, theme, standard, pick: Some(pick), more: None, button: None, current:None })]
}

fn fill_presets(background: bool) -> Vec<MenuEntry> {
    use ibis_doc_model::{GradientPath, GradientStop};
    let stops = vec![GradientStop { position: 0, color: Color::opaque(Rgb(0x4472c4)) }, GradientStop { position: 100_000, color: Color::WHITE }];
    let mut presets = Vec::new();
    for (id, angle) in [("fill.gradient.0", 0), ("fill.gradient.90", 5_400_000), ("fill.gradient.180", 10_800_000), ("fill.gradient.270", 16_200_000)] {
        presets.push((id, Fill::Linear { angle, scaled: false, stops: stops.clone() }));
    }
    for (id, path) in [("fill.path.circle", GradientPath::Circle), ("fill.path.rect", GradientPath::Rect), ("fill.path.shape", GradientPath::Shape)] {
        presets.push((id, Fill::Path { path, focus: [50_000; 4], stops: stops.clone() }));
    }
    let mut gradients: Vec<MenuEntry> = presets.into_iter().map(|(id, fill)| MenuItem::tr(id).on(move |e, _, cx| if background { e.set_background(fill.clone(), cx) } else { e.fill_selected(fill.clone(), cx) }).into()).collect();
    gradients.push(MenuEntry::Separator);
    gradients.push(MenuItem::tr(if background { "pptx.background.custom" } else { "pptx.fill.custom" }).on(move |e, w, cx| e.open_gradient_panel(background, w, cx)).into());
    gradients.push(MenuItem::tr(if background { "pptx.background.custom-path" } else { "pptx.fill.custom-path" }).on(move |e, w, cx| e.open_path_gradient_panel(background, w, cx)).into());
    let mut patterns: Vec<MenuEntry> = [("pct25", crate::shell::i18n::text("pptx.chrome.black-white-25-percent")), ("pct50", crate::shell::i18n::text("pptx.chrome.black-white-50-percent")), ("pct75", crate::shell::i18n::text("pptx.chrome.black-white-75-percent")), ("horz", crate::shell::i18n::text("pptx.chrome.black-white-horizontal-lines")), ("vert", crate::shell::i18n::text("pptx.chrome.black-white-vertical-lines")), ("diagCross", crate::shell::i18n::text("pptx.chrome.black-white-diagonal-grid"))].into_iter().map(|(preset, label)| {
        let fill = Fill::Pattern { preset: preset.into(), foreground: Color::BLACK, background: Color::WHITE };
        MenuItem::new(preset, label).on(move |e, _, cx| if background { e.set_background(fill.clone(), cx) } else { e.fill_selected(fill.clone(), cx) }).into()
    }).collect();
    patterns.push(MenuEntry::Separator);
    patterns.push(MenuItem::tr(if background { "pptx.background.custom-pattern" } else { "pptx.fill.custom-pattern" }).on(move |e, w, cx| e.open_pattern_panel(background, w, cx)).into());
    vec![Submenu::tr("pptx.fill-gradients", "format_color_fill", gradients).into(), Submenu::tr("pptx.fill-patterns", "format_color_fill", patterns).into()]
}

pub(crate) fn shape_fill_menu_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let mut fill_items = color_menu(editor, "pptx.fill", Some(crate::shell::i18n::text("_shell.no-fill")), |this, rgb, _, cx| {
        this.fill_selected(rgb.map_or(Fill::None, |rgb| Fill::Solid(Color::opaque(rgb))), cx)
    });
    fill_items.push(MenuEntry::Separator);
    fill_items.extend(fill_presets(false));
    fill_items.push(MenuItem::tr("pptx.fill.picture").icon("image").on(|this, w, cx| this.pick_fill_picture(w, cx)).into());
    fill_items
}

/// 背景: a colour, or the layout's background back.
pub(crate) fn background_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    let mut items = color_menu(editor, "pptx.background", None, |this, rgb, _, cx| {
        if let Some(rgb) = rgb {
            this.set_background(Fill::Solid(Color::opaque(rgb)), cx);
        }
    });
    items.push(MenuEntry::Separator);
    items.extend(fill_presets(true));
    items.push(MenuItem::tr("pptx.background.picture").icon("image").on(|this, w, cx| this.pick_background_picture(w, cx)).into());
    items.push(
        MenuItem::tr("background.reset")
            .icon("format_color_reset")
            .on(|this, _, cx| {
                let layout = this.slide().layout.and_then(|l| this.presentation().layout(l)).map(|l| l.background.clone());
                if let Some(fill) = layout {
                    this.set_background(fill, cx);
                }
            })
            .into(),
    );
    items.push(MenuEntry::Separator);
    let hidden = editor.slide().hide_backdrop;
    items.push(
        MenuItem::tr("background.hide-graphics")
            .checked(hidden)
            .on(move |this, _, cx| {
                let slide = this.current_slide;
                this.run(vec![ibis_doc_model::Command::SetBackdropHidden { slide, hidden: !hidden }], cx);
            })
            .into(),
    );
    items.push(MenuItem::tr("background.apply-all").icon("wallpaper").on(|this, _, cx| this.apply_background_to_all(cx)).into());
    items
}

/// 表の挿入: the sizes offered (rows × columns).
fn table_size_items() -> Vec<MenuEntry> {
    const SIZES: [(usize, usize, &str); 6] = [
        (2, 2, "table.new.2x2"),
        (3, 3, "table.new.3x3"),
        (3, 4, "table.new.3x4"),
        (4, 4, "table.new.4x4"),
        (5, 5, "table.new.5x5"),
        (6, 4, "table.new.6x4"),
    ];
    SIZES.iter().map(|&(r, c, id)| MenuItem::tr(id).icon("table").on(move |this, window, cx| this.insert_slide_table(r, c, window, cx)).into()).collect()
}

/// 行と列: insert and remove rows and columns of the selected table.
fn table_button(editor: &SlideEditor) -> ToolItem {
    let merged = editor.selected_shapes().first().is_some_and(|s| matches!(&s.kind, ShapeKind::Table(t) if t.has_merges()));
    let edit = editor.text_editing();
    let can_merge = edit.is_some_and(|e| e.cell.is_some() && e.cell_anchor.is_some());
    let can_split = edit.and_then(|e| Some((e.shape, e.cell?))).is_some_and(|(id, (r, c))| {
        editor.slide().shape(id).is_some_and(|s| matches!(&s.kind, ShapeKind::Table(t) if t.rows.get(r).and_then(|row| row.cells.get(c)).is_some_and(|x| !x.merged && (x.row_span > 1 || x.column_span > 1))))
    });
    let item = |id: &'static str, icon: &'static str, f: fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>)| -> MenuEntry {
        MenuItem::tr(id).icon(icon).disabled(merged).on(move |this, window, cx| f(this, window, cx)).into()
    };
    ToolButton::new("table.rows", "table", crate::shell::i18n::text("pptx.chrome.rows-columns"))
        .menu(vec![
            item("table.insert-above", "arrow_upward", |e, w, cx| e.table_insert_row(false, w, cx)),
            item("table.insert-below", "arrow_downward", |e, w, cx| e.table_insert_row(true, w, cx)),
            item("table.insert-left", "border_left", |e, w, cx| e.table_insert_column(false, w, cx)),
            item("table.insert-right", "arrow_forward", |e, w, cx| e.table_insert_column(true, w, cx)),
            MenuEntry::Separator,
            item("table.remove-row", "delete", |e, w, cx| e.table_remove_row(w, cx)),
            item("table.remove-column", "delete", |e, w, cx| e.table_remove_column(w, cx)),
            MenuEntry::Separator,
            MenuItem::tr("table.distribute-rows").icon("vertical_distribute").on(|e, w, cx| e.distribute_table(true, w, cx)).into(),
            MenuItem::tr("table.distribute-columns").icon("horizontal_distribute").on(|e, w, cx| e.distribute_table(false, w, cx)).into(),
            MenuItem::tr("table.cell-size").icon("height").disabled(!editor.can_set_cell_size()).on(|e, w, cx| e.open_cell_size_panel(w, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("table.merge")
                .icon("table")
                .disabled(!can_merge)
                .on(|e, w, cx| e.merge_table_cells(w, cx))
                .into(),
            MenuItem::tr("table.split").icon("table").disabled(!can_split).on(|e, w, cx| e.split_table_cell(w, cx)).into(),
        ])
        .into()
}

pub(crate) fn table_borders_menu_items(enabled: bool) -> Vec<MenuEntry> {
    use super::editing::BorderPlacement as BP;
    vec![
        MenuItem::tr("table.borders.all").icon("border_all").disabled(!enabled).on(|e, _, cx| e.apply_table_borders(BP::All, cx)).into(),
        MenuItem::tr("table.borders.none").icon("border_clear").disabled(!enabled).on(|e, _, cx| e.apply_table_borders(BP::None, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("table.borders.outside").icon("border_outer").disabled(!enabled).on(|e, _, cx| e.apply_table_borders(BP::Outside, cx)).into(),
        MenuItem::tr("table.borders.inside").icon("border_inner").disabled(!enabled).on(|e, _, cx| e.apply_table_borders(BP::Inside, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("table.borders.top").icon("border_top").disabled(!enabled).on(|e, _, cx| e.apply_table_borders(BP::Top, cx)).into(),
        MenuItem::tr("table.borders.bottom").icon("border_bottom").disabled(!enabled).on(|e, _, cx| e.apply_table_borders(BP::Bottom, cx)).into(),
        MenuItem::tr("table.borders.left").icon("border_left").disabled(!enabled).on(|e, _, cx| e.apply_table_borders(BP::Left, cx)).into(),
        MenuItem::tr("table.borders.right").icon("border_right").disabled(!enabled).on(|e, _, cx| e.apply_table_borders(BP::Right, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("table.borders.diagonal-down").icon("border_style").disabled(!enabled).on(|e, _, cx| e.apply_table_borders(BP::DiagonalDown, cx)).into(),
        MenuItem::tr("table.borders.diagonal-up").icon("border_style").disabled(!enabled).on(|e, _, cx| e.apply_table_borders(BP::DiagonalUp, cx)).into(),
        MenuEntry::Separator,
        Submenu::tr("table.borders.weight", "line_style", vec![
            MenuItem::tr("table.borders.0-5pt").disabled(!enabled).on(|e, _, cx| e.stroke_selected(|s| Some(Stroke { width: 6_350, ..s.unwrap_or_else(|| Stroke::solid(Color::BLACK, 6_350)) }), cx)).into(),
            MenuItem::tr("table.borders.1pt").disabled(!enabled).on(|e, _, cx| e.stroke_selected(|s| Some(Stroke { width: 12_700, ..s.unwrap_or_else(|| Stroke::solid(Color::BLACK, 12_700)) }), cx)).into(),
            MenuItem::tr("table.borders.1-5pt").disabled(!enabled).on(|e, _, cx| e.stroke_selected(|s| Some(Stroke { width: 19_050, ..s.unwrap_or_else(|| Stroke::solid(Color::BLACK, 19_050)) }), cx)).into(),
            MenuItem::tr("table.borders.2-25pt").disabled(!enabled).on(|e, _, cx| e.stroke_selected(|s| Some(Stroke { width: 28_575, ..s.unwrap_or_else(|| Stroke::solid(Color::BLACK, 28_575)) }), cx)).into(),
            MenuItem::tr("table.borders.3pt").disabled(!enabled).on(|e, _, cx| e.stroke_selected(|s| Some(Stroke { width: 38_100, ..s.unwrap_or_else(|| Stroke::solid(Color::BLACK, 38_100)) }), cx)).into(),
            MenuItem::tr("table.borders.4-5pt").disabled(!enabled).on(|e, _, cx| e.stroke_selected(|s| Some(Stroke { width: 57_150, ..s.unwrap_or_else(|| Stroke::solid(Color::BLACK, 57_150)) }), cx)).into(),
            MenuItem::tr("table.borders.6pt").disabled(!enabled).on(|e, _, cx| e.stroke_selected(|s| Some(Stroke { width: 76_200, ..s.unwrap_or_else(|| Stroke::solid(Color::BLACK, 76_200)) }), cx)).into(),
        ]).into(),
        Submenu::tr("table.borders.color", "border_color", {
            use crate::shell::color_palette::{theme_rows, Palette, PickColor, Swatch};
            let pick: PickColor = std::rc::Rc::new(|e, rgb, _, cx| {
                if let Some(rgb) = rgb {
                    e.stroke_selected(move |s| Some(Stroke { color: Color::opaque(rgb), ..s.unwrap_or_else(|| Stroke::solid(Color::opaque(rgb), 12_700)) }), cx);
                }
            });
            let default_theme = [
                Rgb(0xffffff), Rgb(0x000000), Rgb(0xe7e6e6), Rgb(0x44546a),
                Rgb(0x4472c4), Rgb(0xed7d31), Rgb(0xa5a5a5), Rgb(0xffc000), Rgb(0x5b9bd5), Rgb(0x70ad47),
            ];
            let mut theme = theme_rows(&default_theme, &pick, None);
            let mut standard: Vec<Swatch> = COLORS
                .iter()
                .filter(|(rgb, _)| !matches!(rgb, 0x000000 | 0x404040 | 0x7f7f7f | 0xd9d9d9 | 0xffffff | 0x4472c4))
                .map(|&(rgb, id)| Swatch::with_id(id, Rgb(rgb), crate::shell::i18n::text(id), false, &pick))
                .collect();
            for sw in theme.iter_mut().flatten().chain(standard.iter_mut()) {
                sw.item.disabled = !enabled;
            }
            for (hex, id) in [
                (0x000000, "table.borders.black"),
                (0xffffff, "table.borders.white"),
                (0xc00000, "table.borders.red"),
                (0x4472c4, "table.borders.blue"),
                (0xffc000, "table.borders.yellow"),
                (0x70ad47, "table.borders.green"),
            ] {
                if let Some(sw) = theme.iter_mut().flatten().chain(standard.iter_mut()).find(|s| s.rgb == Rgb(hex)) {
                    sw.item.id = id;
                }
            }
            vec![MenuEntry::Palette(Palette { none: None, theme, standard, pick: Some(pick), more: None, button: None, current:None })]
        }).into(),
    ]
}

fn table_border_button(editor: &SlideEditor) -> ToolItem {
    use super::editing::BorderPlacement as BP;
    // A split button as PowerPoint's: the left half draws the border last chosen (all borders at first), with its icon.
    ToolButton::new("table.borders", "border_all", crate::shell::i18n::text("pptx.custom-format.borders"))
        .menu(vec![
            MenuItem::tr("table.borders.all").icon("border_all").on(|e, _, cx| e.apply_table_borders(BP::All, cx)).into(),
            MenuItem::tr("table.borders.none").icon("border_clear").on(|e, _, cx| e.apply_table_borders(BP::None, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("table.borders.outside").icon("border_outer").on(|e, _, cx| e.apply_table_borders(BP::Outside, cx)).into(),
            MenuItem::tr("table.borders.inside").icon("border_inner").on(|e, _, cx| e.apply_table_borders(BP::Inside, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("table.borders.top").icon("border_top").on(|e, _, cx| e.apply_table_borders(BP::Top, cx)).into(),
            MenuItem::tr("table.borders.bottom").icon("border_bottom").on(|e, _, cx| e.apply_table_borders(BP::Bottom, cx)).into(),
            MenuItem::tr("table.borders.left").icon("border_left").on(|e, _, cx| e.apply_table_borders(BP::Left, cx)).into(),
            MenuItem::tr("table.borders.right").icon("border_right").on(|e, _, cx| e.apply_table_borders(BP::Right, cx)).into(),
            MenuEntry::Separator,
            MenuItem::tr("table.borders.diagonal-down").icon("border_style").on(|e, _, cx| e.apply_table_borders(BP::DiagonalDown, cx)).into(),
            MenuItem::tr("table.borders.diagonal-up").icon("border_style").on(|e, _, cx| e.apply_table_borders(BP::DiagonalUp, cx)).into(),
        ])
        .last_used(editor, "table.borders", |_| true, |i| i.id == "table.borders.all", true)
        .into()
}

pub(crate) fn table_style_items(editor: &SlideEditor, enabled: bool) -> Vec<MenuEntry> {
    let current_id = editor.table_style_id();
    super::editing::TABLE_STYLES
        .iter()
        .map(|&(id, guid, _name)| {
            let is_checked = match (guid, &current_id) {
                (None, None) => true,
                (Some(g), Some(c)) => g.eq_ignore_ascii_case(c),
                _ => false,
            };
            MenuItem::tr(id)
                .disabled(!enabled)
                .checked(is_checked)
                .on(move |e, _, cx| e.set_table_style_id(guid.map(str::to_string), cx))
                .into()
        })
        .collect()
}

fn table_style_button(editor: &SlideEditor) -> ToolItem {
    ToolButton::new("table.style", "grid_view", crate::shell::i18n::text("pptx.chrome.table-styles"))
        .menu(table_style_items(editor, true))
        .into()
}

pub(crate) fn table_style_option_items(editor: &SlideEditor, enabled: bool) -> Vec<MenuEntry> {
    const OPTIONS: [(&str, TableOption); 6] = [
        ("pptx.table.first-row", TableOption::FirstRow),
        ("pptx.table.last-row", TableOption::LastRow),
        ("pptx.table.band-row", TableOption::BandRow),
        ("pptx.table.first-col", TableOption::FirstCol),
        ("pptx.table.last-col", TableOption::LastCol),
        ("pptx.table.band-col", TableOption::BandCol),
    ];
    OPTIONS
        .iter()
        .map(|&(id, opt)| {
            MenuItem::tr(id)
                .disabled(!enabled)
                .checked(editor.table_option_checked(opt))
                .on(move |e, _, cx| e.toggle_table_option(opt, cx))
                .into()
        })
        .collect()
}

fn table_style_options_button(editor: &SlideEditor) -> ToolItem {
    ToolButton::new("table.style-options", "tune", crate::shell::i18n::text("pptx.chrome.table-style-options"))
        .menu(table_style_option_items(editor, true))
        .into()
}

/// The transitions offered (画面切り替え), with PowerPoint's names.
pub(crate) const TRANSITIONS: [(&str, &str); 16] = [
    ("cut", "transition.cut"),
    ("morph", "transition.morph"),
    ("fade", "transition.fade"),
    ("push", "transition.push"),
    ("wipe", "transition.wipe"),
    ("split", "transition.split"),
    ("cover", "transition.cover"),
    ("pull", "transition.pull"),
    ("randomBar", "transition.randomBar"),
    ("circle", "transition.circle"),
    ("dissolve", "transition.dissolve"),
    ("zoom", "transition.zoom"),
    ("vortex", "transition.vortex"),
    ("ripple", "transition.ripple"),
    ("glitter", "transition.glitter"),
    ("wheel", "transition.wheel"),
];

/// The directions (効果オプション) offered for each transition effect.
pub(crate) fn transition_directions(effect: &str) -> &'static [(&'static str, &'static str)] {
    match effect {
        "push" | "wipe" | "cover" | "pull" => &[
            ("d", "transition.dir.down"),
            ("l", "transition.dir.left"),
            ("r", "transition.dir.right"),
            ("u", "transition.dir.up"),
        ],
        "split" => &[
            ("in", "transition.dir.split-in"),
            ("out", "transition.dir.split-out"),
            ("horzIn", "transition.dir.horz-in"),
            ("horzOut", "transition.dir.horz-out"),
        ],
        "morph" => &[
            ("byObject", "transition.dir.by-object"),
            ("byWord", "transition.dir.by-word"),
            ("byChar", "transition.dir.by-char"),
        ],
        "randomBar" => &[
            ("horz", "transition.dir.horz"),
            ("vert", "transition.dir.vert"),
        ],
        "circle" | "zoom" => &[
            ("in", "transition.dir.in"),
            ("out", "transition.dir.out"),
        ],
        "wheel" => &[
            ("1", "transition.dir.spokes-1"),
            ("2", "transition.dir.spokes-2"),
            ("3", "transition.dir.spokes-3"),
            ("4", "transition.dir.spokes-4"),
            ("8", "transition.dir.spokes-8"),
        ],
        _ => &[],
    }
}

/// The name shown for a slide's transition.
pub(crate) fn transition_name(t: Option<&ibis_doc_model::Transition>) -> String {
    match t {
        None => crate::shell::i18n::text("_shell.none").to_string(),
        Some(t) if t.effect.is_empty() => crate::shell::i18n::text("pptx.chrome.none-timing-only").to_string(),
        Some(t) => {
            let name = TRANSITIONS.iter().find(|(e, _)| *e == t.effect).map_or(t.effect.clone(), |(_, id)| crate::shell::i18n::text(id).to_string());
            let dir_name = t.direction.as_deref().and_then(|d| transition_directions(&t.effect).iter().find(|(dir, _)| *dir == d).map(|(_, id)| crate::shell::i18n::text(id).to_string()));
            let mut display = match dir_name {
                Some(dir) => format!("{name} ({dir})"),
                None => name,
            };
            if let Some(snd) = &t.sound {
                match snd {
                    ibis_doc_model::TransitionSound::Stop => display.push_str(&crate::shell::i18n::text("_pptx.stop")),
                    ibis_doc_model::TransitionSound::Play { name, loop_sound } => {
                        let snd_label = TRANSITION_SOUNDS.iter().find(|(n, _)| *n == name).map_or_else(|| name.to_string(), |(_, id)| crate::shell::i18n::text(id).to_string());
                        if *loop_sound {
                            display.push_str(&format!(" [♪{snd_label} 🔁]"));
                        } else {
                            display.push_str(&format!(" [♪{snd_label}]"));
                        }
                    }
                }
            }
            crate::shell::i18n::format("pptx.chrome.name-sec-s", &[("name", &display), ("sec", &format!("{:.1}", t.duration as f32 / 1000.0))])
        }
    }
}

fn transition_direction_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let current = editor.slide().transition.clone();
    let effect = current.as_ref().map(|t| t.effect.as_str()).unwrap_or("");
    let dirs = transition_directions(effect);
    let current_dir = current.as_ref().and_then(|t| t.direction.as_deref());
    dirs.iter()
        .map(|&(dir, id)| {
            let on = current_dir == Some(dir);
            MenuItem::tr(id).checked(on).on(move |this, _, cx| this.set_transition_direction(Some(dir), cx)).into()
        })
        .collect()
}

pub(crate) const TRANSITION_SOUNDS: &[(&str, &str)] = &[
    ("Applause", "transition.sound.applause"),
    ("Arrow", "transition.sound.arrow"),
    ("Bomb", "transition.sound.bomb"),
    ("Breeze", "transition.sound.breeze"),
    ("Camera", "transition.sound.camera"),
    ("Cash Register", "transition.sound.cash-register"),
    ("Chime", "transition.sound.chime"),
    ("Click", "transition.sound.click"),
    ("Coin", "transition.sound.coin"),
    ("Drum Roll", "transition.sound.drum-roll"),
    ("Explosion", "transition.sound.explosion"),
    ("Hammer", "transition.sound.hammer"),
    ("Laser", "transition.sound.laser"),
    ("Push", "transition.sound.push"),
    ("Suction", "transition.sound.suction"),
    ("Typewriter", "transition.sound.typewriter"),
    ("Voltage", "transition.sound.voltage"),
    ("Wind", "transition.sound.wind"),
];

pub(crate) fn transition_sound_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let current = editor.slide().transition.as_ref().and_then(|t| t.sound.as_ref());
    let mut items: Vec<MenuEntry> = vec![
        MenuItem::tr("transition.sound.none")
            .checked(current.is_none())
            .on(|this, _, cx| this.set_transition_sound(None, cx))
            .into(),
    ];
    for &(name, id) in TRANSITION_SOUNDS {
        let is_selected = matches!(current, Some(ibis_doc_model::TransitionSound::Play { name: n, .. }) if n == name);
        let loop_sound = matches!(current, Some(ibis_doc_model::TransitionSound::Play { loop_sound: true, .. }));
        items.push(
            MenuItem::tr(id)
                .checked(is_selected)
                .on(move |this, _, cx| {
                    this.set_transition_sound(Some(ibis_doc_model::TransitionSound::Play { name: name.to_string(), loop_sound }), cx);
                })
                .into(),
        );
    }
    items.push(MenuEntry::Separator);
    let is_stop = matches!(current, Some(ibis_doc_model::TransitionSound::Stop));
    items.push(
        MenuItem::tr("transition.sound.stop")
            .checked(is_stop)
            .on(|this, _, cx| this.set_transition_sound(Some(ibis_doc_model::TransitionSound::Stop), cx))
            .into(),
    );
    items.push(MenuEntry::Separator);
    let is_play = matches!(current, Some(ibis_doc_model::TransitionSound::Play { .. }));
    let is_loop = matches!(current, Some(ibis_doc_model::TransitionSound::Play { loop_sound: true, .. }));
    items.push(
        MenuItem::tr("transition.sound.loop")
            .checked(is_loop)
            .disabled(!is_play)
            .on(|this, _, cx| this.toggle_transition_sound_loop(cx))
            .into(),
    );
    items
}

pub(crate) fn transition_menu(editor: &SlideEditor) -> Vec<MenuEntry> {
    let current = editor.slide().transition.clone();
    let mut items: Vec<MenuEntry> = vec![
        MenuItem::tr("transition.none").checked(current.as_ref().is_none_or(|t| t.effect.is_empty())).on(|this, _, cx| this.set_transition(None, cx)).into(),
    ];
    items.extend(TRANSITIONS.iter().map(|(effect, id)| {
        let on = current.as_ref().is_some_and(|t| t.effect == *effect);
        MenuItem::tr(id).checked(on).on(move |this, _, cx| this.set_transition(Some(effect), cx)).into()
    }));
    items.push(MenuEntry::Separator);
    let effect = current.as_ref().map(|t| t.effect.as_str()).unwrap_or("");
    let dirs = transition_directions(effect);
    if !dirs.is_empty() {
        let dir_items = transition_direction_items(editor);
        items.push(Submenu::tr("transition.effect-options", "tune", dir_items).into());
        items.push(MenuEntry::Separator);
    }
    let sound_items = transition_sound_items(editor);
    items.push(Submenu::tr("transition.sound", "volume_up", sound_items).into());
    items.push(MenuEntry::Separator);
    for (ms, id) in [(500u32, "transition.duration.500"), (1000, "transition.duration.1000"), (2000, "transition.duration.2000")] {
        let on = current.as_ref().is_some_and(|t| t.duration == ms);
        items.push(MenuItem::tr(id).checked(on).disabled(current.is_none()).on(move |this, _, cx| this.set_transition_duration(ms, cx)).into());
    }
    items.push(MenuEntry::Separator);
    items.push(MenuItem::tr("transition.on-click").checked(current.as_ref().is_none_or(|t| t.on_click)).on(|e, _, cx| e.toggle_transition_click(cx)).into());
    for (after, id) in [(None, "transition.after.none"), (Some(0), "transition.after.0"), (Some(1000), "transition.after.1000"), (Some(5000), "transition.after.5000"), (Some(10000), "transition.after.10000")] {
        items.push(MenuItem::tr(id).checked(current.as_ref().and_then(|t| t.after) == after).on(move |e, _, cx| e.set_transition_after(after, cx)).into());
    }
    items.push(MenuEntry::Separator);
    items.push(MenuItem::tr("transition.timing").on(|e, w, cx| e.open_timing_panel(w, cx)).into());
    items.push(MenuItem::tr("transition.all").icon("done_all").on(|this, _, cx| this.apply_transition_to_all(cx)).into());
    items
}

fn selected_stroke(editor: &SlideEditor) -> Option<Stroke> {
    editor.selected_shapes().into_iter().find_map(|s| match &s.kind {
        ShapeKind::Auto(a) => a.stroke.clone(),
        ShapeKind::Picture(p) => p.stroke.clone(),
        ShapeKind::Line(l) => Some(l.clone()),
        ShapeKind::Table(t) => {
            let (cell, anchor) = editor.text_editing().filter(|e| e.shape == s.id).map_or((None, None), |e| (e.cell, e.cell_anchor));
            let num_rows = t.rows.len();
            let num_cols = t.columns.len();
            let (r0, r1, c0, c1) = match cell {
                Some(a) => {
                    let b = anchor.unwrap_or(a);
                    (a.0.min(b.0), a.0.max(b.0).min(num_rows.saturating_sub(1)), a.1.min(b.1), a.1.max(b.1).min(num_cols.saturating_sub(1)))
                }
                None => (0, num_rows.saturating_sub(1), 0, num_cols.saturating_sub(1)),
            };
            t.rows.iter().enumerate()
                .filter(|(r, _)| (r0..=r1).contains(r))
                .flat_map(|(_, row)| row.cells.iter().enumerate().filter(|(c, _)| (c0..=c1).contains(c)))
                .find_map(|(_, c)| {
                    c.borders.top.as_ref()
                        .or(c.borders.bottom.as_ref())
                        .or(c.borders.left.as_ref())
                        .or(c.borders.right.as_ref())
                        .cloned()
                })
                .or_else(|| {
                    t.rows.iter().flat_map(|r| r.cells.iter()).find_map(|c| {
                        c.borders.top.as_ref()
                            .or(c.borders.bottom.as_ref())
                            .or(c.borders.left.as_ref())
                            .or(c.borders.right.as_ref())
                            .cloned()
                    })
                })
        }
        _ => None,
    })
}

pub(crate) fn shape_stroke_color_items(editor: &SlideEditor, line: bool) -> Vec<MenuEntry> {
    let base = |s: Option<Stroke>| s.unwrap_or_else(|| Stroke::solid(Color::BLACK, Stroke::DEFAULT_WIDTH));
    color_menu(editor, "pptx.stroke-color", (!line).then(|| crate::shell::i18n::text("pptx.chrome.no-border")), move |this, rgb, _, cx| {
        this.stroke_selected(move |s| rgb.map(|rgb| Stroke { color: Color::opaque(rgb), ..base(s) }), cx)
    })
}

pub(crate) fn shape_stroke_width_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let stroke = selected_stroke(editor);
    let base = |s: Option<Stroke>| s.unwrap_or_else(|| Stroke::solid(Color::BLACK, Stroke::DEFAULT_WIDTH));
    WIDTHS
        .iter()
        .map(|&pt| {
            let emu = (pt * 12_700.0).round() as i64;
            MenuItem::new(width_id(pt), format!("{pt} pt"))
                .checked(stroke.as_ref().is_some_and(|s| s.width == emu))
                .on(move |this, _, cx| this.stroke_selected(move |s| Some(Stroke { width: emu, ..base(s) }), cx))
                .into()
        })
        .collect()
}

pub(crate) fn shape_stroke_style_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let stroke = selected_stroke(editor);
    let base = |s: Option<Stroke>| s.unwrap_or_else(|| Stroke::solid(Color::BLACK, Stroke::DEFAULT_WIDTH));
    let dash_label = |d: ibis_doc_model::LineDash| match d {
        ibis_doc_model::LineDash::Solid => crate::shell::i18n::text("pptx.chrome.solid"),
        ibis_doc_model::LineDash::Dot => crate::shell::i18n::text("pptx.chrome.dotted-round"),
        ibis_doc_model::LineDash::Dash => crate::shell::i18n::text("_shell.dashed"),
        ibis_doc_model::LineDash::LgDash => crate::shell::i18n::text("pptx.chrome.long-dash"),
        ibis_doc_model::LineDash::DashDot => crate::shell::i18n::text("_shell.dash-dot"),
        ibis_doc_model::LineDash::LgDashDot => crate::shell::i18n::text("pptx.chrome.long-dash-dot"),
        ibis_doc_model::LineDash::LgDashDotDot => crate::shell::i18n::text("_shell.dash-dot-dot"),
        ibis_doc_model::LineDash::SysDash => crate::shell::i18n::text("pptx.chrome.dashed-fine"),
        ibis_doc_model::LineDash::SysDot => crate::shell::i18n::text("pptx.chrome.dotted-square"),
        ibis_doc_model::LineDash::SysDashDot => crate::shell::i18n::text("pptx.chrome.dash-dot-fine"),
        ibis_doc_model::LineDash::SysDashDotDot => crate::shell::i18n::text("pptx.chrome.dash-dot-dot-fine"),
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
    styles.push(Submenu::tr("pptx.stroke-caps", "line_style", [
        ("cap.flat", LineCap::Flat), ("cap.round", LineCap::Round), ("cap.square", LineCap::Square)
    ].into_iter().map(|(id, cap)| MenuItem::tr(id).checked(stroke.as_ref().is_some_and(|s| s.cap == cap)).on(move |e, _, cx| e.stroke_selected(move |s| Some(Stroke { cap, ..base(s) }), cx)).into()).collect()).into());
    styles.push(Submenu::tr("pptx.stroke-compound", "line_style", [
        ("compound.single", LineCompound::Single), ("compound.double", LineCompound::Double),
        ("compound.thick-thin", LineCompound::ThickThin), ("compound.thin-thick", LineCompound::ThinThick), ("compound.triple", LineCompound::Triple)
    ].into_iter().map(|(id, compound)| MenuItem::tr(id).checked(stroke.as_ref().is_some_and(|s| s.compound == compound)).on(move |e, _, cx| e.stroke_selected(move |s| Some(Stroke { compound, ..base(s) }), cx)).into()).collect()).into());
    styles
}

pub(crate) fn zoom_context_menu_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let zoom_shape = editor.selected_shapes().first().map(|s| s.id).filter(|id| {
        matches!(editor.slide().shape_links.get(id), Some(ibis_doc_model::SlideLink::Slide(_)))
    });
    let Some(shape_id) = zoom_shape else {
        return Vec::new();
    };
    let is_custom = editor.slide().shape(shape_id).is_some_and(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Picture(_)));
    let is_rtz = editor.slide().shape_return_to_zoom.contains(&shape_id);
    let is_bg = editor.shape_zoom_show_bg(shape_id);
    const DURATIONS: [(u32, &str); 5] = [
        (500, "pptx.zoom.duration-500"),
        (1000, "pptx.zoom.duration-1000"),
        (1500, "pptx.zoom.duration-1500"),
        (2000, "pptx.zoom.duration-2000"),
        (2500, "pptx.zoom.duration-2500"),
    ];
    let cur_dur = editor.shape_zoom_duration(shape_id);
    let duration_items: Vec<MenuEntry> = DURATIONS
        .iter()
        .map(|&(ms, label_key)| {
            MenuItem::tr(label_key)
                .checked(cur_dur == ms)
                .on(move |e, _, cx| e.set_shape_zoom_duration(shape_id, ms, cx))
                .into()
        })
        .collect();
    vec![
        MenuItem::tr("pptx.zoom.change-image").icon("image").on(|e, w, cx| e.pick_zoom_image(w, cx)).into(),
        MenuItem::tr("pptx.zoom.reset-image").icon("refresh").disabled(!is_custom).on(move |e, _, cx| { e.reset_zoom_image(shape_id, cx); }).into(),
        MenuItem::tr("pptx.zoom.return-to-zoom").icon("undo").checked(is_rtz).on(move |e, _, cx| e.toggle_shape_return_to_zoom(shape_id, cx)).into(),
        MenuItem::tr("pptx.zoom.background").icon("wallpaper").checked(is_bg).on(move |e, _, cx| e.toggle_shape_zoom_show_bg(shape_id, cx)).into(),
        Submenu::tr("pptx.zoom.duration", "timer", duration_items).into(),
    ]
}

fn outline_buttons(editor: &SlideEditor, line: bool) -> Vec<ToolItem> {
    let (color_label, width_label, style_label) = if line { (crate::shell::i18n::text("pptx.chrome.border-color"), crate::shell::i18n::text("pptx.chrome.line-weight"), crate::shell::i18n::text("pptx.chrome.border-style-2")) } else { (crate::shell::i18n::text("_shell.border-color"), crate::shell::i18n::text("pptx.chrome.border-weight"), crate::shell::i18n::text("pptx.chrome.border-style")) };
    vec![
        ToolButton::new("border-color", "border_color", color_label)
            .palette(editor, "border-color", 0x4472c4, shape_stroke_color_items(editor, line))
            .into(),
        ToolButton::new("line-weight", "line_weight", width_label).menu(shape_stroke_width_items(editor)).into(),
        ToolButton::new("line-style", "line_style", style_label).menu(shape_stroke_style_items(editor)).into(),
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

pub(crate) fn line_end_menu(editor: &SlideEditor, head: bool) -> Vec<MenuEntry> {
    use ibis_doc_model::{EndSize, LineEnd};
    let current = editor.selected_shapes().into_iter().find_map(|s| match &s.kind {
        ShapeKind::Line(s) => Some(if head { s.head } else { s.tail }),
        ShapeKind::Auto(auto) if super::editing::is_connector_geometry(&auto.geometry) => {
            auto.stroke.as_ref().map(|st| if head { st.head } else { st.tail })
        }
        _ => None,
    }).unwrap_or(LineEnd::None);
    let m = EndSize { width: 1, length: 1 };
    let ends = [("end.none", LineEnd::None), ("end.triangle", LineEnd::Triangle(m)), ("end.arrow", LineEnd::Arrow(m)), ("end.stealth", LineEnd::Stealth(m)), ("end.diamond", LineEnd::Diamond(m)), ("end.oval", LineEnd::Oval(m))];
    let mut items: Vec<MenuEntry> = ends.into_iter().map(|(id, end)| {
        MenuItem::tr(id).checked(std::mem::discriminant(&current) == std::mem::discriminant(&end)).on(move |e, _, cx| {
            e.stroke_selected(move |s| s.map(|s| {
                let old = if head { s.head } else { s.tail };
                let end = with_end_size(end, end_size(old).unwrap_or(m));
                if head { Stroke { head: end, ..s } } else { Stroke { tail: end, ..s } }
            }), cx)
        }).into()
    }).collect();
    for (width, id) in [(true, "end.width"), (false, "end.length")] {
        let sizes = [("small", 0), ("medium", 1), ("large", 2)].into_iter().map(|(suffix, value)| {
            let id = match (width, suffix) {
                (true, "small") => "end.width.small", (true, "medium") => "end.width.medium", (true, _) => "end.width.large",
                (false, "small") => "end.length.small", (false, "medium") => "end.length.medium", _ => "end.length.large",
            };
            MenuItem::tr(id).disabled(end_size(current).is_none()).checked(end_size(current).is_some_and(|s| if width { s.width == value } else { s.length == value })).on(move |e, _, cx| {
                e.stroke_selected(move |s| s.map(|s| {
                    let old = if head { s.head } else { s.tail };
                    let Some(mut size) = end_size(old) else { return s };
                    if width { size.width = value; } else { size.length = value; }
                    let end = with_end_size(old, size);
                    if head { Stroke { head: end, ..s } } else { Stroke { tail: end, ..s } }
                }), cx)
            }).into()
        }).collect();
        items.push(Submenu::tr(id, "line_end_arrow", sizes).into());
    }
    items
}

/// The text and line buttons whatever is selected (grey when they cannot be used), for the Format menu's
/// submenus (`shell/menu_bridge.rs`).
pub(crate) fn menu_buttons(editor: &SlideEditor) -> Vec<ToolItem> {
    let mut items = text_buttons(editor);
    items.extend(outline_buttons(editor, false));
    items
}

fn text_buttons(editor: &SlideEditor) -> Vec<ToolItem> {
    let style = editor.selected_text_style();
    let on = style.is_some();
    let (mixed_fonts, mixed_min_size) = {
        let mut fonts: Vec<Option<String>> = Vec::new();
        let mut sizes: Vec<u32> = Vec::new();
        if let (Some(body), Some(edit)) = (editor.edited_body(), editor.text_editing()) {
            let r = edit.range();
            if r.start < r.end {
                let mut at = 0usize;
                for p in &body.paragraphs {
                    for run in &p.runs {
                        let n = run.text.chars().count();
                        if at < r.end && at + n > r.start && n > 0 {
                            let look = body.style.with_run(&run.style);
                            fonts.push(look.font.clone().or(look.fallback_font.clone()));
                            sizes.push(look.font_size);
                        }
                        at += n;
                    }
                    at += 1;
                }
            }
        } else {
            for shape in editor.selected_shapes() {
                if let Some(body) = shape.text_body() {
                    let mut pushed = false;
                    for p in &body.paragraphs {
                        for run in &p.runs {
                            if !run.text.is_empty() {
                                let look = body.style.with_run(&run.style);
                                fonts.push(look.font.clone().or(look.fallback_font.clone()));
                                sizes.push(look.font_size);
                                pushed = true;
                            }
                        }
                    }
                    if !pushed {
                        fonts.push(body.style.font.clone().or(body.style.fallback_font.clone()));
                        sizes.push(body.style.font_size);
                    }
                }
            }
        }
        let mf = fonts.first().is_some_and(|f0| fonts.iter().any(|f| f != f0));
        let ms = match (sizes.iter().min(), sizes.iter().max()) {
            (Some(&lo), Some(&hi)) if lo != hi => Some(lo),
            _ => None,
        };
        (mf, ms)
    };
    let font: SharedString = if mixed_fonts {
        SharedString::from("")
    } else {
        style
            .as_ref()
            .and_then(|s| s.font.clone())
            .unwrap_or_else(|| crate::shell::i18n::text("pptx.chrome.theme-fonts").to_string())
            .into()
    };
    let font_size_value: Option<SharedString> = match mixed_min_size {
        Some(min_sz) => Some(format!("{}+", min_sz as f32 / 100.0).into()),
        None => style.as_ref().map(|s| format!("{}", s.font_size_pt()).into()),
    };
    let align = style.as_ref().map(|s| s.align);
    let align_item = |id: &'static str, icon: &'static str, a: TextAlign| -> MenuEntry {
        MenuItem::tr(id).icon(icon).checked(align == Some(a)).on(move |this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { align: Some(a), ..Default::default() })).into()
    };
    let first_run = editor.selected_text_run();
    let underline = first_run.as_ref().is_some_and(|r| r.underline);
    let shadow = editor.selected_shadow().is_some();
    let strike = first_run.as_ref().is_some_and(|r| r.strike != ibis_doc_model::Strike::None);
    let spacing = editor.selected_spacing();
    let list = editor.selected_list();
    let _text_color = style.as_ref().map_or(rgb(0x000000).into(), |s| crate::render::shape_view::hsla(Color::opaque(s.color)));
    let current_font = (!mixed_fonts).then(|| style.as_ref().and_then(|s| s.font.as_deref())).flatten();
    let leading = vec![
        MenuItem::tr("pptx.replace-fonts").on(|this, window, cx| this.open_replace_fonts_panel(window, cx)).into(),
    ];
    let font_menu = crate::shell::font_list::build_font_menu_entries(
        "pptx.font",
        leading,
        current_font,
        |f, this, window, cx| {
            this.format_characters(window, cx, ibis_doc_model::CharacterFormat {
                font: Some(Some(f.to_string())),
                ..Default::default()
            });
        },
    );
    // Bullets and numbering are split buttons as PowerPoint's: the left half toggles the list in the bullet (the numbering)
    // last chosen, the ▼ has the choices. `bullet-types` and `number-types` are the same menus for the Format menu (not drawn).
    let (bullet_on, number_on) = (matches!(list, Some(ListKind::Bullet | ListKind::Symbol(_))), matches!(list, Some(ListKind::Numbered | ListKind::Scheme(_))));
    let bullets = ToolButton::new("bullets", "format_list_bulleted", crate::shell::i18n::text("_shell.bullets"))
        .on(bullet_on)
        .disabled(!on)
        .click(move |this, window, cx| {
            let kind = if bullet_on { ListKind::None } else { ListKind::Bullet };
            this.format_paragraphs(window, cx, ParagraphFormat { list: Some(kind), ..Default::default() })
        })
        .menu(
            [("bullet.type.disc", '•'), ("bullet.type.circle", '○'), ("bullet.type.square", '■'), ("bullet.type.box", '□'), ("bullet.type.diamond", '◆'), ("bullet.type.arrow", '►'), ("bullet.type.dash", '–')]
                .into_iter()
                .map(|(id, c)| -> MenuEntry {
                    let now = match list { Some(ListKind::Symbol(s)) => s == c, Some(ListKind::Bullet) => c == '•', _ => false };
                    MenuItem::tr(id).checked(now).on(move |this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { list: Some(if c == '•' { ListKind::Bullet } else { ListKind::Symbol(c) }), ..Default::default() })).into()
                })
                .collect(),
        )
        .last_used(editor, "bullets", |_| true, |_| false, false)
        .quick_off(bullet_on, move |this, window, cx| {
            let kind = if bullet_on { ListKind::None } else { ListKind::Bullet };
            this.format_paragraphs(window, cx, ParagraphFormat { list: Some(kind), ..Default::default() })
        });
    let bullet_types = ToolButton::text("bullet-types", crate::shell::i18n::text("pptx.index-fields.bullets"), crate::shell::i18n::text("pptx.chrome.bullet-symbol")).disabled(!on).menu(bullets.menu.clone().unwrap_or_default()).hidden();
    let numbered = ToolButton::new("numbered", "format_list_numbered", crate::shell::i18n::text("_shell.numbering"))
        .on(number_on)
        .disabled(!on)
        .click(move |this, window, cx| {
            let kind = if number_on { ListKind::None } else { ListKind::Numbered };
            this.format_paragraphs(window, cx, ParagraphFormat { list: Some(kind), ..Default::default() })
        })
        .menu(
            [
                (ibis_doc_model::NumberScheme::ArabicPeriod, "number.type.arabicPeriod"),
                (ibis_doc_model::NumberScheme::ArabicParenR, "number.type.arabicParenR"),
                (ibis_doc_model::NumberScheme::AlphaLcPeriod, "number.type.alphaLcPeriod"),
                (ibis_doc_model::NumberScheme::AlphaUcPeriod, "number.type.alphaUcPeriod"),
                (ibis_doc_model::NumberScheme::AlphaLcParenR, "number.type.alphaLcParenR"),
                (ibis_doc_model::NumberScheme::RomanLcPeriod, "number.type.romanLcPeriod"),
                (ibis_doc_model::NumberScheme::RomanUcPeriod, "number.type.romanUcPeriod"),
            ]
                .into_iter()
                .map(|(s, id)| -> MenuEntry {
                    let now = match list { Some(ListKind::Scheme(c)) => c == s, Some(ListKind::Numbered) => s == ibis_doc_model::NumberScheme::ArabicPeriod, _ => false };
                    MenuItem::tr(id).checked(now).on(move |this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { list: Some(if s == ibis_doc_model::NumberScheme::ArabicPeriod { ListKind::Numbered } else { ListKind::Scheme(s) }), ..Default::default() })).into()
                })
                .chain([
                    MenuEntry::Separator,
                    Submenu::tr("pptx.numbering.start-at", "format_list_numbered", vec![
                        MenuItem::tr("pptx.numbering.start-at-1").on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { start_at: Some(1), ..Default::default() })).into(),
                        MenuItem::tr("pptx.numbering.start-at-2").on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { start_at: Some(2), ..Default::default() })).into(),
                        MenuItem::tr("pptx.numbering.start-at-3").on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { start_at: Some(3), ..Default::default() })).into(),
                        MenuItem::tr("pptx.numbering.start-at-4").on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { start_at: Some(4), ..Default::default() })).into(),
                        MenuItem::tr("pptx.numbering.start-at-5").on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { start_at: Some(5), ..Default::default() })).into(),
                        MenuItem::tr("pptx.numbering.start-at-10").on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { start_at: Some(10), ..Default::default() })).into(),
                        MenuEntry::Separator,
                        MenuItem::tr("pptx.numbering.options").on(|this, window, cx| this.open_paragraph_panel(window, cx)).into(),
                    ]).into(),
                ])
                .collect(),
        )
        .last_used(editor, "numbered", |_| true, |_| false, false)
        .quick_off(number_on, move |this, window, cx| {
            let kind = if number_on { ListKind::None } else { ListKind::Numbered };
            this.format_paragraphs(window, cx, ParagraphFormat { list: Some(kind), ..Default::default() })
        });
    let number_types = ToolButton::text("number-types", crate::shell::i18n::text("pptx.chrome.numbering"), crate::shell::i18n::text("pptx.chrome.numbering-style")).disabled(!on).menu(numbered.menu.clone().unwrap_or_default()).hidden();
    let current_v_anchor = editor
        .edited_body()
        .or_else(|| editor.selected_shapes().into_iter().find_map(|s| s.text_body()))
        .map(|b| b.anchor);
    let [align_l, align_c, align_r, align_drop] = crate::shell::chrome::align_h_toolbar_items(
        align,
        on,
        handler(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { align: Some(TextAlign::Left), ..Default::default() })),
        handler(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { align: Some(TextAlign::Center), ..Default::default() })),
        handler(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { align: Some(TextAlign::Right), ..Default::default() })),
        vec![
            align_item("align.left", "format_align_left", TextAlign::Left),
            align_item("align.center", "format_align_center", TextAlign::Center),
            align_item("align.right", "format_align_right", TextAlign::Right),
            align_item("align.justify", "format_align_justify", TextAlign::Justify),
            align_item("align.distributed", "format_align_justify", TextAlign::Distributed),
        ],
    );
    let [valign_t, valign_m, valign_b, valign_drop] = crate::shell::chrome::align_v_toolbar_items(
        current_v_anchor,
        on,
        handler(|e, _, cx| e.format_text_body(|b| b.anchor = ibis_doc_model::VerticalAnchor::Top, cx)),
        handler(|e, _, cx| e.format_text_body(|b| b.anchor = ibis_doc_model::VerticalAnchor::Middle, cx)),
        handler(|e, _, cx| e.format_text_body(|b| b.anchor = ibis_doc_model::VerticalAnchor::Bottom, cx)),
        vec![
            MenuItem::tr("pptx.text.top").icon("vertical_align_top").checked(current_v_anchor == Some(ibis_doc_model::VerticalAnchor::Top)).on(|e, _, cx| e.format_text_body(|b| b.anchor = ibis_doc_model::VerticalAnchor::Top, cx)).into(),
            MenuItem::tr("pptx.text.middle").icon("vertical_align_center").checked(current_v_anchor == Some(ibis_doc_model::VerticalAnchor::Middle)).on(|e, _, cx| e.format_text_body(|b| b.anchor = ibis_doc_model::VerticalAnchor::Middle, cx)).into(),
            MenuItem::tr("pptx.text.bottom").icon("vertical_align_bottom").checked(current_v_anchor == Some(ibis_doc_model::VerticalAnchor::Bottom)).on(|e, _, cx| e.format_text_body(|b| b.anchor = ibis_doc_model::VerticalAnchor::Bottom, cx)).into(),
        ],
    );
    vec![
        sep(),
        ToolItem::Select { id: "font", value: font, width: 128.0, tooltip: crate::shell::i18n::text("_shell.font"), menu: font_menu, disabled: !on },
        sep(),
        ToolItem::FontSize {
            value: font_size_value,
            smaller: on.then(|| handler(|this, window, cx| this.step_font_size_of_text(false, window, cx))),
            larger: on.then(|| handler(|this, window, cx| this.step_font_size_of_text(true, window, cx))),
        },
        ToolButton::new("clear-format", "format_clear", crate::shell::i18n::text("pptx.chrome.clear-all-formatting")).disabled(!on).click(|this, window, cx| this.clear_formats(window, cx)).into(),
        sep(),
        ToolButton::new("bold", "format_bold", crate::shell::i18n::text("_shell.bold-b")).on(style.as_ref().is_some_and(|s| s.bold)).disabled(!on).click(|this, window, cx| {
            this.toggle_characters(window, cx, |b, r| b.with_run(r).bold, |on| CharacterFormat { bold: Some(on), ..Default::default() })
        }).into(),
        ToolButton::new("italic", "format_italic", crate::shell::i18n::text("_shell.italic-i")).on(style.as_ref().is_some_and(|s| s.italic)).disabled(!on).click(|this, window, cx| {
            this.toggle_characters(window, cx, |b, r| b.with_run(r).italic, |on| CharacterFormat { italic: Some(on), ..Default::default() })
        }).into(),
        // A split button as PowerPoint's: the left half takes the underline off, or underlines in the kind last chosen
        // (single at first); the ▼ has the kinds (`a:rPr/@u`).
        ToolButton::new("underline", "format_underlined", crate::shell::i18n::text("_shell.underline-u")).on(underline).disabled(!on).click(|this, window, cx| {
            this.toggle_characters(window, cx, |_, r| r.underline, |on| CharacterFormat { underline: Some(on), ..Default::default() })
        })
            .menu(underline_kind_items(first_run.as_ref().filter(|r| r.underline).map(|r| r.underline_kind)))
            .last_used(editor, "underline", |_| true, |i| i.id == "pptx.underline.sng", false)
            .quick_off(underline, |this, window, cx| {
                this.toggle_characters(window, cx, |_, r| r.underline, |on| CharacterFormat { underline: Some(on), ..Default::default() })
            })
            .into(),
        ToolButton::new("text-shadow", "ev_shadow", crate::shell::i18n::text("pptx.text.shadow")).on(shadow).disabled(!on).click(|this, window, cx| {
            this.toggle_text_shadow(window, cx)
        }).into(),
        ToolButton::new("strikethrough", "format_strikethrough", crate::shell::i18n::text("pptx.chrome.strikethrough")).on(strike).disabled(!on).click(|this, window, cx| this.strike_characters(window, cx)).into(),
        ToolButton::new("text-color", "format_color_text", crate::shell::i18n::text("_shell.font-color"))
            .disabled(!on)
            // The colour at first: red, as Office's (the colours of the buttons at first are our own picks).
            .palette(editor, "text-color", 0xff0000, color_menu(editor, "pptx.text-color", None, |this, rgb, window, cx| {
                if let Some(rgb) = rgb {
                    this.format_characters(window, cx, CharacterFormat { color: Some(rgb), ..Default::default() });
                }
            }))
            .into(),
        ToolButton::new("highlight", "ink_highlighter", crate::shell::i18n::text("pptx.chrome.highlight-color"))
            .disabled(!on)
            .palette(editor, "highlight", 0xffff00, color_menu(editor, "pptx.highlight", Some(crate::shell::i18n::text("pptx.chrome.no-color")), |this, rgb, window, cx| {
                this.format_characters(window, cx, ibis_doc_model::CharacterFormat { highlight: Some(rgb), ..Default::default() })
            }))
            .into(),
        ToolButton::new("character-spacing", "space_bar", crate::shell::i18n::text("pptx.text.character-spacing"))
            .disabled(!on)
            .menu(vec![
                MenuItem::tr("pptx.spacing.very-tight").checked(spacing == -300).on(|this, window, cx| this.set_slide_character_spacing(-300, window, cx)).into(),
                MenuItem::tr("pptx.spacing.tight").checked(spacing == -150).on(|this, window, cx| this.set_slide_character_spacing(-150, window, cx)).into(),
                MenuItem::tr("pptx.spacing.normal").checked(spacing == 0).on(|this, window, cx| this.set_slide_character_spacing(0, window, cx)).into(),
                MenuItem::tr("pptx.spacing.loose").checked(spacing == 300).on(|this, window, cx| this.set_slide_character_spacing(300, window, cx)).into(),
                MenuItem::tr("pptx.spacing.very-loose").checked(spacing == 600).on(|this, window, cx| this.set_slide_character_spacing(600, window, cx)).into(),
            ])
            .into(),
        ToolButton::new("case", "format_size", crate::shell::i18n::text("pptx.text.case"))
            .disabled(!on)
            .menu(vec![
                MenuItem::tr("pptx.case.sentence").on(|this, window, cx| this.change_case(crate::pptx::editing::TextCase::Sentence, window, cx)).into(),
                MenuItem::tr("pptx.case.lower").on(|this, window, cx| this.change_case(crate::pptx::editing::TextCase::Lower, window, cx)).into(),
                MenuItem::tr("pptx.case.upper").on(|this, window, cx| this.change_case(crate::pptx::editing::TextCase::Upper, window, cx)).into(),
                MenuItem::tr("pptx.case.title").on(|this, window, cx| this.change_case(crate::pptx::editing::TextCase::Title, window, cx)).into(),
                MenuItem::tr("pptx.case.toggle").on(|this, window, cx| this.change_case(crate::pptx::editing::TextCase::Toggle, window, cx)).into(),
            ])
            .into(),
        ToolButton::new("proofing-lang", "language", crate::shell::i18n::text("pptx.proofing-language"))
            .disabled(!on)
            .menu(proofing_language_menu_items(editor))
            .into(),
        ToolButton::new("pptx-format-paint", "format_paint", crate::shell::i18n::text("_shell.copy-paste-formatting")).disabled(!on).menu(format_copy_items(editor)).into(),
        sep(),
        bullets.into(),
        bullet_types.into(),
        numbered.into(),
        number_types.into(),
        ToolButton::new("indent-less", "format_indent_decrease", crate::shell::i18n::text("_shell.decrease-indent")).disabled(!on).click(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { indent_by: Some(-457_200), ..Default::default() })).into(),
        ToolButton::new("indent-more", "format_indent_increase", crate::shell::i18n::text("_shell.increase-indent")).disabled(!on).click(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { indent_by: Some(457_200), ..Default::default() })).into(),
        ToolButton::new("line-spacing", "format_line_spacing", crate::shell::i18n::text("_shell.line-paragraph-spacing"))
            .disabled(!on)
            .menu(
                [("spacing.1", "1.0", 100_000), ("spacing.1.15", "1.15", 115_000), ("spacing.1.5", "1.5", 150_000), ("spacing.2", "2.0", 200_000)]
                    .into_iter()
                    .map(|(id, label, v)| -> MenuEntry {
                        MenuItem::new(id, label).on(move |this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { line_spacing: Some(Some(LineSpacing::Percent(v))), ..Default::default() })).into()
                    })
                    .chain(std::iter::once(MenuItem::tr("pptx.paragraph.settings").on(|e, w, cx| e.open_paragraph_panel(w, cx)).into()))
                    .collect(),
            )
            .into(),
        align_l,
        align_c,
        align_r,
        align_drop,
        valign_t,
        valign_m,
        valign_b,
        valign_drop,
        ToolButton::new("text-direction", "format_textdirection_l_to_r", crate::shell::i18n::text("docx.chrome.text-direction"))
            .disabled(!on)
            .menu(vec![
                MenuItem::tr("pptx.text.horizontal").icon("format_textdirection_l_to_r").on(|e, _, cx| e.format_text_body(|b| { b.vertical = false; b.sideways = 0; }, cx)).into(),
                MenuItem::tr("pptx.text.vertical").icon("format_textdirection_vertical").on(|e, _, cx| e.format_text_body(|b| { b.vertical = true; b.vertical_kind = ibis_doc_model::VerticalKind::EastAsian; b.sideways = 0; }, cx)).into(),
                MenuItem::tr("pptx.text.clockwise").icon("rotate_90_degrees_cw").on(|e, _, cx| e.format_text_body(|b| { b.vertical = false; b.sideways = 5_400_000; }, cx)).into(),
                MenuItem::tr("pptx.text.counterclockwise").icon("rotate_90_degrees_ccw").on(|e, _, cx| e.format_text_body(|b| { b.vertical = false; b.sideways = 16_200_000; }, cx)).into(),
            ])
            .into(),
        ToolButton::new("text-warp", "transform", crate::shell::i18n::text("pptx.text.warp"))
            .disabled(!on)
            .menu(text_warp_items(editor))
            .into(),
        ToolButton::new("columns", "view_column", crate::shell::i18n::text("pptx.text.columns"))
            .disabled(!on)
            .menu({
                let body = editor.edited_body().or_else(|| editor.selected_shapes().into_iter().find_map(|s| s.text_body()));
                let current_cols = body.as_ref().map(|b| b.columns).unwrap_or(1);
                let current_spc = body.as_ref().map(|b| b.column_spacing).unwrap_or(0);
                vec![
                    MenuItem::tr("pptx.text.columns.1").checked(current_cols == 1).on(|e, _, cx| e.set_text_columns(1, 0, cx)).into(),
                    MenuItem::tr("pptx.text.columns.2").checked(current_cols == 2).on(|e, _, cx| e.set_text_columns(2, 180_000, cx)).into(),
                    MenuItem::tr("pptx.text.columns.3").checked(current_cols == 3).on(|e, _, cx| e.set_text_columns(3, 180_000, cx)).into(),
                    MenuItem::tr("pptx.text.columns.4").checked(current_cols == 4).on(|e, _, cx| e.set_text_columns(4, 180_000, cx)).into(),
                    MenuEntry::Separator,
                    Submenu::tr("pptx.text.columns.spacing", "space_bar", [
                        ("pptx.text.columns.spacing.0", 0),
                        ("pptx.text.columns.spacing.0-5cm", 180_000),
                        ("pptx.text.columns.spacing.1cm", 360_000),
                        ("pptx.text.columns.spacing.2cm", 720_000),
                    ].into_iter().map(|(id, spc)| {
                        MenuItem::tr(id).disabled(current_cols <= 1).checked(current_spc == spc).on(move |e, _, cx| {
                            let cols = e.edited_body().or_else(|| e.selected_shapes().into_iter().find_map(|s| s.text_body())).map(|b| b.columns).unwrap_or(1);
                            e.set_text_columns(cols, spc, cx);
                        }).into()
                    }).collect()).into(),
                ]
            })
            .into(),
    ]
}

fn arrange_buttons(editor: &SlideEditor) -> Vec<ToolItem> {
    let order = |id: &'static str, icon: &'static str, o: Order| -> MenuEntry { MenuItem::tr(id).icon(icon).on(move |this, _, cx| this.order_selected(o, cx)).into() };
    let align = |id: &'static str, icon: &'static str, a: Align| -> MenuEntry { MenuItem::tr(id).icon(icon).on(move |this, _, cx| this.align_selected(a, cx)).into() };
    let can_distribute = editor.selected_ids().len() >= 3;
    let to_slide = editor.pptx().is_some_and(|s| s.align_to_slide);
    vec![
        sep(),
        ToolButton::new("order", "flip_to_front", crate::shell::i18n::text("pptx.chrome.order"))
            .menu(vec![
                order("order.front", "flip_to_front", Order::Front),
                order("order.forward", "arrow_upward", Order::Forward),
                order("order.backward", "arrow_downward", Order::Backward),
                order("order.back", "flip_to_back", Order::Back),
                MenuEntry::Separator,
                MenuItem::tr("pptx.selection-pane").icon("visibility").checked(editor.pptx().is_some_and(|s| s.selection_pane.is_some())).on(|e, _, cx| e.toggle_selection_pane(cx)).into(),
                MenuItem::tr("pptx.reading-order").icon("format_list_numbered").checked(editor.pptx().is_some_and(|s| s.reading_order_pane.is_some())).on(|e, _, cx| e.toggle_reading_order_pane(cx)).into(),
            ])
            .into(),
        ToolButton::new("arrange", "align_horizontal_left", crate::shell::i18n::text("_shell.arrange"))
            .menu(vec![
                align("arrange.left", "align_horizontal_left", Align::Left),
                align("arrange.center", "align_horizontal_center", Align::Center),
                align("arrange.right", "align_horizontal_right", Align::Right),
                MenuEntry::Separator,
                align("arrange.top", "align_vertical_top", Align::Top),
                align("arrange.middle", "align_vertical_center", Align::Middle),
                align("arrange.bottom", "align_vertical_bottom", Align::Bottom),
                MenuEntry::Separator,
                MenuItem::tr("pptx.distribute.h").icon("horizontal_distribute").disabled(!can_distribute).on(|this, _, cx| this.distribute_selected(true, cx)).into(),
                MenuItem::tr("pptx.distribute.v").icon("vertical_distribute").disabled(!can_distribute).on(|this, _, cx| this.distribute_selected(false, cx)).into(),
                MenuEntry::Separator,
                MenuItem::tr("pptx.center.h").icon("align_horizontal_center").on(|this, _, cx| this.center_selected(true, cx)).into(),
                MenuItem::tr("pptx.center.v").icon("align_vertical_center").on(|this, _, cx| this.center_selected(false, cx)).into(),
                MenuEntry::Separator,
                MenuItem::tr("pptx.align.to-slide").icon("crop_landscape").checked(to_slide).on(|this, _, cx| {
                    if let Some(s) = this.pptx_mut() {
                        s.align_to_slide = !s.align_to_slide;
                    }
                    cx.notify();
                }).into(),
                MenuEntry::Separator,
                Submenu::tr("pptx.rotate", "rotate_right", vec![
                    MenuItem::tr("pptx.rotate.right").icon("rotate_90_degrees_cw").on(|e, _, cx| e.rotate_selected(90, cx)).into(),
                    MenuItem::tr("pptx.rotate.left").icon("rotate_90_degrees_ccw").on(|e, _, cx| e.rotate_selected(-90, cx)).into(),
                    MenuItem::tr("pptx.rotate.flip-h").icon("flip").on(|e, _, cx| e.flip_selected(true, cx)).into(),
                    MenuItem::tr("pptx.rotate.flip-v").icon("flip").on(|e, _, cx| e.flip_selected(false, cx)).into(),
                ]).into(),
            ])
            .into(),
    ]
}

pub(crate) fn toolbar_right(editor: &SlideEditor) -> Vec<ToolItem> {
    let tool = editor.pptx_tool();
    let mut items: Vec<ToolItem> = vec![
        ToolButton::new("tool-select", "arrow_selector_tool", crate::shell::i18n::text("_shell.select-esc")).on(tool == Tool::Select).click(|this, window, cx| this.set_pptx_tool(Tool::Select, window, cx)).into(),
        ToolButton::new("tool-text", "text_fields", crate::shell::i18n::text("_shell.text-box")).on(tool == Tool::TextBox).click(|this, window, cx| this.set_pptx_tool(Tool::TextBox, window, cx)).into(),
        ToolButton::new("insert-image", "image", crate::shell::i18n::text("_shell.insert-picture")).menu(image_items()).into(),
        ToolButton::new("insert-shape", "shapes", crate::shell::i18n::text("_shell.shape")).on(matches!(tool, Tool::Shape(_))).menu(shape_items()).into(),
        ToolButton::new("insert-line", "horizontal_rule", crate::shell::i18n::text("_shell.line-2")).on(matches!(tool, Tool::Line { .. })).menu(line_items()).into(),
    ];
    let _fill_color = editor.selected_shapes().into_iter().find_map(|s| match &s.kind {
        ShapeKind::Auto(a) => match &a.fill {
            Fill::Solid(c) => Some(crate::render::shape_view::hsla(*c)),
            _ => None,
        },
        _ => None,
    });
    let fill = ToolButton::new("fill", "format_color_fill", crate::shell::i18n::text("pptx.common.fill")).palette(editor, "fill", 0xffc000, shape_fill_menu_items(editor));
    let options = ToolButton::text("format-options", crate::shell::i18n::text("_shell.format-options"), crate::shell::i18n::text("pptx.chrome.format-options-sidebar")).click(|this, _, cx| {
        this.sidebar_open = true;
        cx.notify();
    });
    let motion = ToolButton::text("motion", crate::shell::i18n::text("pptx.chrome.animations"), crate::shell::i18n::text("pptx.chrome.animations-motion")).menu(super::animation_ui::animation_items(editor));
    let has_anim_options = super::animation_ui::has_animation_options(editor);
    let anim_options = ToolButton::text("anim-options", crate::shell::i18n::text("pptx.anim.options"), crate::shell::i18n::text("pptx.anim.options.tooltip"))
        .disabled(!has_anim_options)
        .menu(super::animation_ui::animation_option_items(editor));
    let can_paint = editor.can_copy_animations() || editor.can_paste_animations();
    let has_copied_anim = editor.pptx().is_some_and(|s| s.animation_copy.is_some());
    let anim_painter = ToolButton::text("anim-painter", crate::shell::i18n::text("pptx.anim.painter"), crate::shell::i18n::text("pptx.anim.painter.tooltip"))
        .disabled(!can_paint)
        .on(has_copied_anim)
        .click(|this, _, cx| this.toggle_animation_painter(cx));
    match editor.selection_kind() {
        SelectionKind::Shape => {
            items.push(sep());
            // 図形の変更 (auto shapes only).
            if editor.selected_shapes().iter().any(|s| matches!(s.kind, ShapeKind::Auto(_))) {
                let change = SHAPES
                    .iter()
                    .zip(CHANGE_SHAPE_IDS)
                    .map(|(&(preset, icon, label), id)| MenuItem::new(id, crate::shell::i18n::text(label)).icon(icon).on(move |e, _, cx| e.change_selected_shapes(preset, cx)).into())
                    .collect();
                items.push(ToolButton::new("change-shape", "shapes", crate::shell::i18n::text("pptx.chrome.change-shape")).menu(change).hidden().into());
            }
            if editor.selected_shapes().iter().any(|s| crate::xlsx::ink::stroke_points(s).is_some()) {
                let ink_menu = vec![
                    MenuItem::tr("pptx.ink.to-shape").icon("shapes").on(|e, _, cx| e.pptx_ink_to_shape(cx)).into(),
                    MenuItem::tr("pptx.ink.to-text").icon("text_fields").on(|e, _, cx| e.pptx_ink_to_text(cx)).into(),
                    MenuItem::tr("pptx.ink.to-math").icon("functions").on(|e, _, cx| e.pptx_ink_to_math(cx)).into(),
                    MenuItem::tr("pptx.ink.replay").icon("play_arrow").on(|e, _, cx| e.pptx_ink_replay(cx)).into(),
                ];
                items.push(ToolButton::new("ink-convert-menu", "draw", crate::shell::i18n::text("pptx.ink.to-shape")).menu(ink_menu).into());
            }
            items.push(fill.into());
            items.extend(outline_buttons(editor, false));
            items.extend(text_buttons(editor));
            items.extend([sep(), options.into(), motion.into(), anim_options.into(), anim_painter.into()]);
            items.extend(arrange_buttons(editor));
        }
        SelectionKind::Picture => {
            // A video or audio clip: its 再生 and playback options.
            if let Some(shape) = editor.selected_shapes().into_iter().find(|s| SlideEditor::clip_of(s).is_some()) {
                let id = shape.id;
                let opt = editor.selected_clip_options().unwrap_or_default();
                let vol_icon = match opt.volume {
                    MediaVolume::High => "volume_up",
                    MediaVolume::Medium => "volume_down",
                    MediaVolume::Low => "volume_mute",
                    MediaVolume::Mute => "volume_off",
                };
                items.extend([
                    sep(),
                    ToolButton::new("clip-play", "play_arrow", crate::shell::i18n::text("pptx.clip.play")).click(move |e, _, cx| e.toggle_clip(id, cx)).into(),
                    ToolButton::new("clip-start", "play_circle", crate::shell::i18n::text("pptx.clip.start")).menu(vec![
                        MenuItem::tr("pptx.clip.start.on-click").checked(opt.start == MediaStart::OnClick).on(|e, _, cx| e.set_selected_clip_start(MediaStart::OnClick, cx)).into(),
                        MenuItem::tr("pptx.clip.start.auto").checked(opt.start == MediaStart::Automatically).on(|e, _, cx| e.set_selected_clip_start(MediaStart::Automatically, cx)).into(),
                    ]).into(),
                    ToolButton::new("clip-volume", vol_icon, crate::shell::i18n::text("pptx.clip.volume")).menu(vec![
                        MenuItem::tr("pptx.clip.volume.high").checked(opt.volume == MediaVolume::High).on(|e, _, cx| e.set_selected_clip_volume(MediaVolume::High, cx)).into(),
                        MenuItem::tr("pptx.clip.volume.medium").checked(opt.volume == MediaVolume::Medium).on(|e, _, cx| e.set_selected_clip_volume(MediaVolume::Medium, cx)).into(),
                        MenuItem::tr("pptx.clip.volume.low").checked(opt.volume == MediaVolume::Low).on(|e, _, cx| e.set_selected_clip_volume(MediaVolume::Low, cx)).into(),
                        MenuItem::tr("pptx.clip.volume.mute").checked(opt.volume == MediaVolume::Mute).on(|e, _, cx| e.set_selected_clip_volume(MediaVolume::Mute, cx)).into(),
                    ]).into(),
                    ToolButton::new("clip-loop", "repeat", crate::shell::i18n::text("pptx.clip.loop")).on(opt.loop_until_stopped).click(|e, _, cx| e.toggle_selected_clip_loop(cx)).into(),
                    ToolButton::new("clip-rewind", "replay", crate::shell::i18n::text("pptx.clip.rewind")).on(opt.rewind).click(|e, _, cx| e.toggle_selected_clip_rewind(cx)).into(),
                    ToolButton::new("clip-trim", "content_cut", crate::shell::i18n::text("pptx.clip.trim")).menu(vec![
                        MenuItem::tr("pptx.clip.trim.none").checked(!opt.is_trimmed()).on(|e, _, cx| e.set_selected_clip_trim(0, 0, cx)).into(),
                        MenuItem::tr("pptx.clip.trim.start-1s").checked(opt.trim_start_ms == 1000 && opt.trim_end_ms == 0).on(|e, _, cx| e.set_selected_clip_trim(1000, 0, cx)).into(),
                        MenuItem::tr("pptx.clip.trim.start-2s").checked(opt.trim_start_ms == 2000 && opt.trim_end_ms == 0).on(|e, _, cx| e.set_selected_clip_trim(2000, 0, cx)).into(),
                        MenuItem::tr("pptx.clip.trim.start-5s").checked(opt.trim_start_ms == 5000 && opt.trim_end_ms == 0).on(|e, _, cx| e.set_selected_clip_trim(5000, 0, cx)).into(),
                        MenuItem::tr("pptx.clip.trim.first-5s").checked(opt.trim_start_ms == 0 && opt.trim_end_ms == 5000).on(|e, _, cx| e.set_selected_clip_trim(0, 5000, cx)).into(),
                        MenuItem::tr("pptx.clip.trim.first-10s").checked(opt.trim_start_ms == 0 && opt.trim_end_ms == 10000).on(|e, _, cx| e.set_selected_clip_trim(0, 10000, cx)).into(),
                    ]).into(),
                    ToolButton::new("clip-fade", "tune", crate::shell::i18n::text("pptx.clip.fade")).menu(vec![
                        Submenu::tr("pptx.clip.fade-in", "tune", vec![
                            MenuItem::tr("pptx.clip.fade.none").checked(opt.fade_in_ms == 0).on(|e, _, cx| e.set_selected_clip_fade_in(0, cx)).into(),
                            MenuItem::tr("pptx.clip.fade.05s").checked(opt.fade_in_ms == 500).on(|e, _, cx| e.set_selected_clip_fade_in(500, cx)).into(),
                            MenuItem::tr("pptx.clip.fade.1s").checked(opt.fade_in_ms == 1000).on(|e, _, cx| e.set_selected_clip_fade_in(1000, cx)).into(),
                            MenuItem::tr("pptx.clip.fade.2s").checked(opt.fade_in_ms == 2000).on(|e, _, cx| e.set_selected_clip_fade_in(2000, cx)).into(),
                        ]).into(),
                        Submenu::tr("pptx.clip.fade-out", "tune", vec![
                            MenuItem::tr("pptx.clip.fade.none").checked(opt.fade_out_ms == 0).on(|e, _, cx| e.set_selected_clip_fade_out(0, cx)).into(),
                            MenuItem::tr("pptx.clip.fade.05s").checked(opt.fade_out_ms == 500).on(|e, _, cx| e.set_selected_clip_fade_out(500, cx)).into(),
                            MenuItem::tr("pptx.clip.fade.1s").checked(opt.fade_out_ms == 1000).on(|e, _, cx| e.set_selected_clip_fade_out(1000, cx)).into(),
                            MenuItem::tr("pptx.clip.fade.2s").checked(opt.fade_out_ms == 2000).on(|e, _, cx| e.set_selected_clip_fade_out(2000, cx)).into(),
                        ]).into(),
                    ]).into(),
                    ToolButton::new("clip-bookmark", "bookmark", crate::shell::i18n::text("pptx.clip.bookmarks")).menu({
                        let mut bmk_items = vec![
                            MenuItem::tr("pptx.clip.bookmark.add").on(|e, _, cx| e.add_selected_clip_bookmark(None, None, cx)).into(),
                            MenuItem::tr("pptx.clip.bookmark.clear").disabled(opt.bookmarks.is_empty()).on(|e, _, cx| e.clear_selected_clip_bookmarks(cx)).into(),
                        ];
                        if !opt.bookmarks.is_empty() {
                            bmk_items.push(MenuEntry::Separator);
                            for (i, bmk) in opt.bookmarks.iter().enumerate() {
                                let secs = bmk.time_ms as f32 / 1000.0;
                                let label = format!("{} ({:.1}s)", bmk.name, secs);
                                bmk_items.push(
                                    MenuItem::new("pptx.clip.bookmark-jump", label)
                                        .icon("bookmark")
                                        .on(move |e, _, cx| e.seek_selected_clip_to_bookmark(i, cx))
                                        .into(),
                                );
                            }
                        }
                        bmk_items
                    }).into(),
                    ToolButton::new("clip-captions", "subtitles", crate::shell::i18n::text("pptx.clip.captions")).menu({
                        let mut cap_items = vec![
                            MenuItem::tr("pptx.clip.caption.add").icon("add").on(|e, _, cx| e.add_selected_clip_caption(&crate::shell::i18n::text("_pptx.caption-track-language"), "ja-JP", &crate::shell::i18n::text("_pptx.caption-template"), cx)).into(),
                            MenuItem::tr("pptx.clip.caption.clear").icon("delete").disabled(opt.captions.is_empty()).on(|e, _, cx| e.clear_selected_clip_captions(cx)).into(),
                        ];
                        if !opt.captions.is_empty() {
                            cap_items.push(MenuEntry::Separator);
                            for cap in &opt.captions {
                                let label = format!("{} ({})", cap.label, cap.lang);
                                cap_items.push(
                                    MenuItem::new("pptx.clip.caption-item", label)
                                        .icon("subtitles")
                                        .into(),
                                );
                            }
                        }
                        cap_items
                    }).into(),
                ]);
                let is_audio = SlideEditor::clip_of(&shape).is_some_and(|c| c.audio);
                if !is_audio {
                    items.push(ToolButton::new("clip-fullscreen", "fullscreen", crate::shell::i18n::text("pptx.clip.fullscreen")).on(opt.full_screen).click(|e, _, cx| e.toggle_selected_clip_fullscreen(cx)).into());
                    items.push(ToolButton::new("clip-hide-stopped", "visibility_off", crate::shell::i18n::text("pptx.clip.hide-stopped")).on(opt.hide_while_not_playing).click(|e, _, cx| e.toggle_selected_clip_hide_stopped(cx)).into());
                    items.push(ToolButton::new("clip-poster", "image", crate::shell::i18n::text("pptx.clip.poster")).menu(vec![
                        MenuItem::tr("pptx.clip.poster.from-file").on(|e, w, cx| e.pick_clip_poster_image(w, cx)).into(),
                        MenuItem::tr("pptx.clip.poster.reset").on(|e, _, cx| e.reset_selected_clip_poster_image(cx)).into(),
                    ]).into());
                    items.push(ToolButton::new("video-styles", "style", crate::shell::i18n::text("pptx.video.styles")).menu(video_style_items(editor, true)).into());
                    items.push(ToolButton::new("video-shape", "crop_original", crate::shell::i18n::text("pptx.video.shape")).menu(video_shape_items(editor, true)).into());
                    items.push(ToolButton::new("video-effects", "auto_awesome", crate::shell::i18n::text("pptx.video.effects")).menu(video_effects_items(editor, true)).into());
                } else {
                    items.push(ToolButton::new("clip-play-across", "arrow_forward", crate::shell::i18n::text("pptx.clip.play-across")).on(opt.play_across_slides).click(|e, _, cx| e.toggle_selected_clip_play_across(cx)).into());
                    items.push(ToolButton::new("clip-hide-show", "visibility_off", crate::shell::i18n::text("pptx.clip.hide-show")).on(opt.hide_during_show).click(|e, _, cx| e.toggle_selected_clip_hide_show(cx)).into());
                    items.push(ToolButton::new("clip-background", "music_note", crate::shell::i18n::text("pptx.clip.background")).on(opt.is_background_play()).click(|e, _, cx| e.toggle_selected_clip_background(cx)).into());
                }
            }
            items.extend([sep(), ToolButton::new("crop", "crop", crate::shell::i18n::text("pptx.chrome.crop-picture")).menu(crop_items()).into(), ToolButton::new("reset-image", "reset_image", crate::shell::i18n::text("pptx.chrome.reset-crop")).click(|e, _, cx| e.crop_selected_pictures(ibis_doc_model::Crop::default(), cx)).into(), ToolButton::new("replace-image", "swap_horiz", crate::shell::i18n::text("pptx.chrome.replace-picture")).click(|e, w, cx| e.pick_replacement_picture(w, cx)).into(), ToolButton::new("picture-color", "palette", crate::shell::i18n::text("pptx.chrome.picture-color")).menu(picture_color_items(true)).into(), ToolButton::new("remove-background", "auto_fix_high", crate::shell::i18n::text("pptx.picture.remove-background")).click(|e, _, cx| e.set_selected_pictures_clear(Some(ibis_doc_model::Rgb(0xffffff)), cx)).into(), ToolButton::new("picture-correction", "tune", crate::shell::i18n::text("pptx.chrome.brightness-contrast")).menu(picture_correction_items(editor, true)).into(), ToolButton::new("picture-artistic", "auto_fix_high", crate::shell::i18n::text("pptx.chrome.artistic-effects")).menu(picture_artistic_effect_items(true)).into()]);
            if editor.has_selected_svg_picture() {
                items.push(ToolButton::new("convert-to-shape", "category", crate::shell::i18n::text("pptx.picture.convert-to-shape")).click(|e, _, cx| { e.convert_selected_svg_to_shape(cx); }).into());
            }
            items.push(sep());
            items.extend(outline_buttons(editor, false));
            items.extend([sep(), options.into(), motion.into(), anim_options.into(), anim_painter.into()]);
            items.extend(arrange_buttons(editor));
        }
        SelectionKind::Line => {
            items.push(sep());
            items.extend(outline_buttons(editor, true));
            items.push(ToolButton::new("line-start", "line_start", crate::shell::i18n::text("pptx.chrome.begin-arrow")).menu(line_end_menu(editor, true)).into());
            items.push(ToolButton::new("line-end", "line_end_arrow_notch", crate::shell::i18n::text("pptx.chrome.end-arrow")).menu(line_end_menu(editor, false)).into());
            items.extend([sep(), options.into(), motion.into(), anim_options.into(), anim_painter.into()]);
            items.extend(arrange_buttons(editor));
        }
        // A table cell being edited: its text's formats.
        SelectionKind::Table if editor.text_editing().is_some_and(|e| e.cell.is_some()) => {
            items.push(sep());
            items.extend(text_buttons(editor));
            items.extend([sep(), fill.clone().into()]);
            items.extend(outline_buttons(editor, false));
            items.extend([table_border_button(editor), table_button(editor), table_style_button(editor), table_style_options_button(editor), options.into()]);
            items.extend(arrange_buttons(editor));
        }
        SelectionKind::Table => {
            items.extend([sep(), fill.clone().into()]);
            items.extend(outline_buttons(editor, false));
            items.extend([table_border_button(editor), table_button(editor), table_style_button(editor), table_style_options_button(editor), options.into()]);
            items.extend(arrange_buttons(editor));
        }
        SelectionKind::Chart if editor.selected_shapes().first().is_some_and(|s| matches!(&s.kind, ShapeKind::Chart(c) if c.ex.is_some())) => {
            // A chart of a new kind (chartEx): its data is edited in its workbook, its title and legend are shown or
            // hidden and put on a side; the rest is not written yet.
            items.extend([
                sep(),
                ToolButton::text("chart-title", crate::shell::i18n::text("_xlsx.chart-title"), crate::shell::i18n::text("_xlsx.chart-title")).menu(chart_title_items(editor)).into(),
                ToolButton::text("chart-legend", crate::shell::i18n::text("_shell.legend"), crate::shell::i18n::text("_shell.legend")).menu(legend_items(editor)).into(),
                ToolButton::text("chart-data", crate::shell::i18n::text("pptx.chrome.edit-data"), crate::shell::i18n::text("pptx.chrome.edit-chart-data")).click(|this, window, cx| this.open_chart_data(window, cx)).into(),
            ]);
            // The data labels, the value axis' range and the fill of the series (written into the part: `ex_format`).
            if let Some((kind, _)) = selected_chart_ex(editor) {
                if kind != ExKind::Box {
                    items.push(ToolButton::text("chart-labels", crate::shell::i18n::text("pptx.chrome.data-labels"), crate::shell::i18n::text("pptx.chrome.data-labels")).menu(chart_ex_labels_items(editor)).into());
                }
                if matches!(kind, ExKind::Columns { axis: true, .. } | ExKind::Box) {
                    items.push(ToolButton::text("chart-axis", crate::shell::i18n::text("pptx.chart.axis-menu"), crate::shell::i18n::text("pptx.chart.axis-menu")).menu(chart_ex_axis_items(editor)).into());
                }
                if kind != ExKind::Tree {
                    items.push(ToolButton::text("chart-fill", crate::shell::i18n::text("pptx.chart.series.fill"), crate::shell::i18n::text("pptx.chart.series.fill")).menu(chart_ex_fill_items(editor)).into());
                }
            }
            items.extend([options.into(), motion.into(), anim_options.into(), anim_painter.into()]);
            items.extend(arrange_buttons(editor));
        }
        SelectionKind::Chart => {
            let chart_group = editor.selected_shapes().first().and_then(|s| match &s.kind {
                ShapeKind::Chart(c) => c.groups.first().map(|g| g.kind),
                _ => None,
            });
            let is_pie = matches!(chart_group, Some(ChartType::Pie { .. }));
            let is_doughnut = matches!(chart_group, Some(ChartType::Doughnut { .. }));
            let is_pie_family = is_pie || is_doughnut;
            let is_bubble = matches!(chart_group, Some(ChartType::Bubble { .. }));
            let is_scatter = matches!(chart_group, Some(ChartType::Scatter { .. }));

            items.extend([
                sep(),
                ToolButton::text("chart-data", crate::shell::i18n::text("pptx.chrome.edit-data"), crate::shell::i18n::text("pptx.chrome.edit-chart-data")).click(|this, window, cx| this.open_chart_data(window, cx)).into(),
                ToolButton::text("chart-type", crate::shell::i18n::text("pptx.chrome.chart-type"), crate::shell::i18n::text("pptx.chrome.change-chart-type")).menu(chart_type_items(editor)).into(),
                ToolButton::text("chart-title", crate::shell::i18n::text("_xlsx.chart-title"), crate::shell::i18n::text("_xlsx.chart-title")).menu(chart_title_items(editor)).into(),
                ToolButton::text("chart-legend", crate::shell::i18n::text("_shell.legend"), crate::shell::i18n::text("_shell.legend")).menu(legend_items(editor)).into(),
                ToolButton::text("chart-labels", crate::shell::i18n::text("pptx.chrome.data-labels"), crate::shell::i18n::text("pptx.chrome.data-labels"))
                    .menu(chart_labels_items(editor))
                    .into(),
            ]);
            if is_pie_family {
                items.push(
                    ToolButton::text("chart-first-angle", crate::shell::i18n::text("pptx.chrome.slice-angle"), crate::shell::i18n::text("pptx.chrome.angle-first-slice"))
                        .menu(chart_first_angle_items(editor))
                        .into(),
                );
            }
            if is_doughnut {
                items.push(
                    ToolButton::text("chart-hole-size", crate::shell::i18n::text("pptx.chrome.hole-size"), crate::shell::i18n::text("pptx.chrome.doughnut-hole-size"))
                        .menu(chart_hole_size_items(editor))
                        .into(),
                );
            }
            if is_bubble {
                items.push(
                    ToolButton::text("chart-bubble-scale", crate::shell::i18n::text("pptx.chrome.bubble-size"), crate::shell::i18n::text("pptx.chrome.bubble-size-scale"))
                        .menu(chart_bubble_scale_items(editor))
                        .into(),
                );
            }
            if is_scatter {
                items.push(
                    ToolButton::text("chart-scatter-smooth", crate::shell::i18n::text("pptx.chrome.smooth-lines"), crate::shell::i18n::text("pptx.chrome.smooth-lines"))
                        .menu(chart_scatter_smooth_items(editor))
                        .into(),
                );
            }
            if !is_pie_family {
                items.extend([
                    ToolButton::text("chart-axis", crate::shell::i18n::text("pptx.chrome.axis"), crate::shell::i18n::text("pptx.chrome.format-axis"))
                        .menu(chart_axis_items(editor))
                        .into(),
                    ToolButton::text("chart-trendline", crate::shell::i18n::text("pptx.chrome.trendline"), crate::shell::i18n::text("pptx.chrome.trendline"))
                        .menu(chart_trendline_items(editor))
                        .into(),
                    ToolButton::text("chart-error-bars", crate::shell::i18n::text("pptx.chrome.error-bars"), crate::shell::i18n::text("pptx.chrome.error-bars"))
                        .menu(chart_error_bars_items(editor))
                        .into(),
                ]);
            }
            items.extend([
                ToolButton::text("chart-series-format", crate::shell::i18n::text("pptx.chrome.series-format"), crate::shell::i18n::text("pptx.chrome.format-data-series"))
                    .menu(chart_series_format_items(editor))
                    .into(),
                ToolButton::text("chart-plot-area", crate::shell::i18n::text("pptx.chrome.plot-area"), crate::shell::i18n::text("pptx.chrome.format-plot-area"))
                    .menu(chart_plot_area_items(editor))
                    .into(),
                ToolButton::text("chart-style", crate::shell::i18n::text("_shell.styles"), crate::shell::i18n::text("_shell.chart-styles"))
                    .menu(chart_style_items(editor))
                    .into(),
                ToolButton::text("chart-palette", crate::shell::i18n::text("pptx.chrome.change-colors"), crate::shell::i18n::text("pptx.chrome.change-chart-colors"))
                    .menu(chart_palette_items(editor))
                    .into(),
                options.into(),
                motion.into(),
                anim_options.into(),
                anim_painter.into(),
            ]);
            items.extend(arrange_buttons(editor));
        }
        SelectionKind::Multi => {
            if editor.selected_text_style().is_some() {
                items.extend(text_buttons(editor));
            }
            items.push(sep());
            items.push(fill.into());
            items.extend(outline_buttons(editor, false));
            items.extend([sep(), options.into(), motion.into(), anim_options.into(), anim_painter.into()]);
            items.extend(arrange_buttons(editor));
        }
        SelectionKind::None => {
            let transition_dirs = editor.slide().transition.as_ref().map_or(false, |t| !transition_directions(&t.effect).is_empty());
            items.extend([
                sep(),
                ToolButton::text("background", crate::shell::i18n::text("_shell.background"), crate::shell::i18n::text("_shell.background")).menu(background_menu(editor)).into(),
                ToolButton::text("layout", crate::shell::i18n::text("_shell.layout"), crate::shell::i18n::text("_shell.layout")).menu(apply_layout_items(editor)).into(),
                ToolButton::text("theme", crate::shell::i18n::text("_shell.theme"), crate::shell::i18n::text("_shell.theme")).menu(theme_menu(editor)).into(),
                ToolButton::text("transition", crate::shell::i18n::text("_shell.transitions"), crate::shell::i18n::text("_shell.transitions")).menu(transition_menu(editor)).into(),
                ToolButton::text("transition-options", crate::shell::i18n::text("pptx.chrome.effect-options"), crate::shell::i18n::text("pptx.chrome.transition-effect-options"))
                    .disabled(!transition_dirs)
                    .menu(transition_direction_items(editor))
                    .into(),
                ToolButton::text("transition-sound", crate::shell::i18n::text("pptx.editing.sound"), crate::shell::i18n::text("pptx.chrome.transition-sound"))
                    .menu(transition_sound_items(editor))
                    .into(),
            ]);
        }
    }
    items
}

/// グラフの種類: the presets, the chart's own checked.
fn chart_type_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let chart = editor.selected_shapes().first().and_then(|s| match &s.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    });
    chart_type_items_for(chart.as_ref(), SlideEditor::set_chart_type)
}

/// グラフの種類 for `chart`: the presets of its family, its own checked; one calls `set`.
pub(crate) fn chart_type_items_for(chart: Option<&ibis_doc_model::Chart>, set: fn(&mut SlideEditor, super::chart_panel::ChartPreset, &mut Context<SlideEditor>)) -> Vec<MenuEntry> {
    use super::chart_panel::ChartPreset;
    let kind = chart.and_then(|c| c.groups.first().map(|g| g.kind));
    let is_pie = kind.is_some_and(|k| matches!(k, ChartType::Pie { .. } | ChartType::Doughnut { .. }));
    let is_scatter = kind.is_some_and(|k| matches!(k, ChartType::Scatter { .. } | ChartType::Bubble { .. }));
    let now = chart.and_then(|c| match c.groups.as_slice() {
        [g] => ChartPreset::of(g.kind, g.grouping),
        _ => None,
    });
    if is_pie {
        [ChartPreset::Pie, ChartPreset::Doughnut]
            .iter()
            .map(|&preset| {
                let id = match preset {
                    ChartPreset::Pie => "chart.type.pie",
                    ChartPreset::Doughnut => "chart.type.doughnut",
                    _ => unreachable!(),
                };
                MenuItem::tr(id).checked(now == Some(preset)).on(move |e, _, cx| set(e, preset, cx)).into()
            })
            .collect()
    } else if is_scatter {
        [ChartPreset::Scatter, ChartPreset::ScatterSmooth, ChartPreset::Bubble]
            .iter()
            .map(|&preset| {
                let id = match preset {
                    ChartPreset::Scatter => "chart.type.scatter",
                    ChartPreset::ScatterSmooth => "chart.type.scatter-smooth",
                    ChartPreset::Bubble => "chart.type.bubble",
                    _ => unreachable!(),
                };
                MenuItem::tr(id).checked(now == Some(preset)).on(move |e, _, cx| set(e, preset, cx)).into()
            })
            .collect()
    } else {
        ChartPreset::ALL
            .iter()
            .filter(|(p, _)| !matches!(p, ChartPreset::Pie | ChartPreset::Doughnut | ChartPreset::Scatter | ChartPreset::ScatterSmooth | ChartPreset::Bubble))
            .map(|&(preset, id)| MenuItem::tr(id).checked(now == Some(preset)).on(move |e, _, cx| set(e, preset, cx)).into())
            .collect()
    }
}

/// 最初の扇形の角度: 0, 45, 90, 180, 270.
fn chart_first_angle_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let current_angle = editor.selected_shapes().first().and_then(|s| match &s.kind {
        ShapeKind::Chart(c) => match c.groups.first() {
            Some(g) => match g.kind {
                ChartType::Pie { first_angle } | ChartType::Doughnut { first_angle, .. } => Some(first_angle),
                _ => None,
            },
            None => None,
        },
        _ => None,
    });
    [
        ("pptx.chart.first-angle.0", 0),
        ("pptx.chart.first-angle.45", 45),
        ("pptx.chart.first-angle.90", 90),
        ("pptx.chart.first-angle.180", 180),
        ("pptx.chart.first-angle.270", 270),
    ]
    .into_iter()
    .map(|(id, angle)| {
        MenuItem::tr(id)
            .checked(current_angle == Some(angle))
            .on(move |e, _, cx| e.set_chart_first_slice_angle(angle, cx))
            .into()
    })
    .collect()
}

/// ドーナツの穴の大きさ: 25%, 50%, 75%.
fn chart_hole_size_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let current_hole = editor.selected_shapes().first().and_then(|s| match &s.kind {
        ShapeKind::Chart(c) => match c.groups.first() {
            Some(g) => match g.kind {
                ChartType::Doughnut { hole, .. } => Some(hole),
                _ => None,
            },
            None => None,
        },
        _ => None,
    });
    [
        ("pptx.chart.hole-size.25", 25),
        ("pptx.chart.hole-size.50", 50),
        ("pptx.chart.hole-size.75", 75),
    ]
    .into_iter()
    .map(|(id, hole)| {
        MenuItem::tr(id)
            .checked(current_hole == Some(hole))
            .on(move |e, _, cx| e.set_chart_doughnut_hole_size(hole, cx))
            .into()
    })
    .collect()
}

/// バブルの大きさ（倍率）: 50%, 100%, 150%, 200%, 250%.
fn chart_bubble_scale_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let current_scale = editor.selected_shapes().first().and_then(|s| match &s.kind {
        ShapeKind::Chart(c) => match c.groups.first() {
            Some(g) => match g.kind {
                ChartType::Bubble { scale } => Some(scale),
                _ => None,
            },
            None => None,
        },
        _ => None,
    });
    [
        ("pptx.chart.bubble-scale.50", 50),
        ("pptx.chart.bubble-scale.100", 100),
        ("pptx.chart.bubble-scale.150", 150),
        ("pptx.chart.bubble-scale.200", 200),
        ("pptx.chart.bubble-scale.250", 250),
    ]
    .into_iter()
    .map(|(id, scale)| {
        MenuItem::tr(id)
            .checked(current_scale == Some(scale))
            .on(move |e, _, cx| e.set_chart_bubble_scale(scale, cx))
            .into()
    })
    .collect()
}

/// 散布図の平滑線: 直線 vs 平滑線.
fn chart_scatter_smooth_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let current_smooth = editor.selected_shapes().first().and_then(|s| match &s.kind {
        ShapeKind::Chart(c) => match c.groups.first() {
            Some(g) => match g.kind {
                ChartType::Scatter { smooth } => Some(smooth),
                _ => None,
            },
            None => None,
        },
        _ => None,
    });
    vec![
        MenuItem::tr("pptx.chart.scatter-smooth.straight")
            .checked(current_smooth == Some(false))
            .on(|e, _, cx| e.set_chart_scatter_smooth(false, cx))
            .into(),
        MenuItem::tr("pptx.chart.scatter-smooth.smooth")
            .checked(current_smooth == Some(true))
            .on(|e, _, cx| e.set_chart_scatter_smooth(true, cx))
            .into(),
    ]
}

/// グラフ タイトル: none, or above chart, the chart's own checked.
fn chart_title_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let has_title = editor.selected_shapes().first().and_then(|s| match &s.kind {
        ShapeKind::Chart(c) => Some(c.title.is_some()),
        _ => None,
    });
    vec![
        MenuItem::tr("chart.title.none").checked(has_title == Some(false)).on(|e, _, cx| e.set_chart_title(false, cx)).into(),
        MenuItem::tr("chart.title.above").checked(has_title == Some(true)).on(|e, _, cx| e.set_chart_title(true, cx)).into(),
    ]
}

/// 凡例: none, or a side, the chart's own checked.
fn legend_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    use ibis_doc_model::LegendPosition as P;
    let now = editor.selected_shapes().first().and_then(|s| match &s.kind {
        ShapeKind::Chart(c) => Some(c.legend.as_ref().map(|l| l.source_position)),
        _ => None,
    });
    [("chart.legend.none", None), ("chart.legend.right", Some(P::Right)), ("chart.legend.top", Some(P::Top)), ("chart.legend.left", Some(P::Left)), ("chart.legend.bottom", Some(P::Bottom))]
        .into_iter()
        .map(|(id, p)| {
            let checked = match (now, p) {
                (Some(None), None) => true,
                (Some(Some(at)), Some(p)) => at == Some(p),
                _ => false,
            };
            MenuItem::tr(id).checked(checked).on(move |e, _, cx| e.set_chart_legend(p, cx)).into()
        })
        .collect()
}

/// データラベル: none, value, positions, and content options.
fn chart_labels_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    use ibis_doc_model::{ChartType, LabelPosition as P};
    use super::chart_panel::ChartLabelFlag;

    let chart_info = editor.selected_shapes().first().and_then(|s| match &s.kind {
        ShapeKind::Chart(c) => {
            let first_labels = c.groups.iter().flat_map(|g| &g.series).find_map(|s| s.labels.as_ref());
            let has_labels = first_labels.is_some();
            let pos = first_labels.and_then(|l| l.position);
            let show_val = first_labels.is_some_and(|l| l.show_value);
            let show_cat = first_labels.is_some_and(|l| l.show_category);
            let show_ser = first_labels.is_some_and(|l| l.show_series_name);
            let show_pct = first_labels.is_some_and(|l| l.show_percent);
            let kind = c.groups.first().map(|g| g.kind);
            Some((has_labels, pos, show_val, show_cat, show_ser, show_pct, kind))
        }
        _ => None,
    });

    let (has_labels, current_pos, show_val, show_cat, show_ser, show_pct, chart_kind) = chart_info.unwrap_or((false, None, false, false, false, false, None));

    let mut items: Vec<MenuEntry> = Vec::new();
    items.push(MenuItem::tr("chart.labels.none").checked(!has_labels).on(|e, _, cx| e.set_chart_labels_position(None, cx)).into());
    items.push(MenuItem::tr("chart.labels.value").checked(has_labels && current_pos.is_none()).on(|e, _, cx| e.set_chart_labels_position(Some(None), cx)).into());

    let positions: &[(&str, P)] = match chart_kind {
        Some(ChartType::Pie { .. } | ChartType::Doughnut { .. } | ChartType::OfPie { .. }) => &[
            ("chart.labels.center", P::Center),
            ("chart.labels.inside-end", P::InsideEnd),
            ("chart.labels.outside-end", P::OutsideEnd),
            ("chart.labels.best-fit", P::BestFit),
        ],
        Some(ChartType::Line | ChartType::Area | ChartType::Scatter { .. } | ChartType::Bubble { .. }) => &[
            ("chart.labels.center", P::Center),
            ("chart.labels.left", P::Left),
            ("chart.labels.right", P::Right),
            ("chart.labels.above", P::Above),
            ("chart.labels.below", P::Below),
        ],
        _ => &[
            ("chart.labels.center", P::Center),
            ("chart.labels.inside-end", P::InsideEnd),
            ("chart.labels.inside-base", P::InsideBase),
            ("chart.labels.outside-end", P::OutsideEnd),
        ],
    };

    for &(id, pos) in positions {
        let checked = has_labels && current_pos == Some(pos);
        items.push(MenuItem::tr(id).checked(checked).on(move |e, _, cx| e.set_chart_labels_position(Some(Some(pos)), cx)).into());
    }

    items.push(MenuEntry::Separator);

    let content_items: Vec<MenuEntry> = vec![
        MenuItem::tr("chart.labels.show-value").checked(show_val).disabled(!has_labels).on(|e, _, cx| e.toggle_chart_label_flag(ChartLabelFlag::Value, cx)).into(),
        MenuItem::tr("chart.labels.show-category").checked(show_cat).disabled(!has_labels).on(|e, _, cx| e.toggle_chart_label_flag(ChartLabelFlag::Category, cx)).into(),
        MenuItem::tr("chart.labels.show-series-name").checked(show_ser).disabled(!has_labels).on(|e, _, cx| e.toggle_chart_label_flag(ChartLabelFlag::SeriesName, cx)).into(),
        MenuItem::tr("chart.labels.show-percent").checked(show_pct).disabled(!has_labels).on(|e, _, cx| e.toggle_chart_label_flag(ChartLabelFlag::Percent, cx)).into(),
    ];
    items.push(Submenu::tr("chart.labels.content-menu", "checklist", content_items).into());

    items
}

/// What kind of chartEx chart the selected chart is, for what of its format can be changed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ExKind {
    /// A waterfall (three fills), a histogram or a Pareto (labels, axis, fill), a funnel (labels, fill).
    Columns { waterfall: bool, axis: bool },
    Box,
    Tree,
}

fn selected_chart_ex(editor: &SlideEditor) -> Option<(ExKind, ibis_doc_model::ExFormat)> {
    editor.selected_shapes().first().and_then(|s| match &s.kind {
        ShapeKind::Chart(c) => {
            let format = c.ex_format.clone()?;
            let kind = match c.ex.as_ref()? {
                ibis_doc_model::ChartEx::Box { .. } => ExKind::Box,
                ibis_doc_model::ChartEx::Treemap { .. } | ibis_doc_model::ChartEx::Sunburst(_) => ExKind::Tree,
                ibis_doc_model::ChartEx::Synthesized { .. } => ExKind::Columns { waterfall: format.fills.len() == 3, axis: c.axes.iter().any(|a| a.kind == ibis_doc_model::AxisKind::Value && !a.deleted) },
            };
            Some((kind, format))
        }
        _ => None,
    })
}

/// データ ラベル (chartEx): none, or the values (and the category names), at the kind's own place or at one chosen.
fn chart_ex_labels_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    use ibis_doc_model::{ExLabels, LabelPosition as P};
    let Some((kind, format)) = selected_chart_ex(editor) else { return Vec::new() };
    let labels = format.labels;
    let own = |l: Option<ExLabels>| l.unwrap_or(ExLabels { value: true, category: false, series_name: false, position: None });
    let mut items: Vec<MenuEntry> = vec![
        MenuItem::tr("chart.labels.none").checked(labels.is_none()).on(|e, _, cx| e.set_chart_ex_labels(None, cx)).into(),
        MenuItem::tr("chart.labels.value").checked(labels.is_some_and(|l| l.value && !l.category && l.position.is_none())).on(move |e, _, cx| e.set_chart_ex_labels(Some(ExLabels { value: true, category: false, series_name: false, position: None }), cx)).into(),
    ];
    if kind != ExKind::Box {
        let positions: &[(&str, P)] = match kind {
            ExKind::Tree => &[("chart.labels.center", P::Center), ("chart.labels.inside-end", P::InsideEnd)],
            _ => &[("chart.labels.center", P::Center), ("chart.labels.inside-end", P::InsideEnd), ("chart.labels.inside-base", P::InsideBase), ("chart.labels.outside-end", P::OutsideEnd)],
        };
        for &(id, pos) in positions {
            items.push(MenuItem::tr(id).checked(labels.is_some_and(|l| l.position == Some(pos))).on(move |e, _, cx| e.set_chart_ex_labels(Some(ExLabels { position: Some(pos), ..own(labels) }), cx)).into());
        }
    }
    items.push(MenuEntry::Separator);
    items.push(MenuItem::tr("chart.labels.show-value").checked(labels.is_some_and(|l| l.value)).disabled(labels.is_none()).on(move |e, _, cx| e.set_chart_ex_labels(Some(ExLabels { value: !own(labels).value, ..own(labels) }), cx)).into());
    items.push(MenuItem::tr("chart.labels.show-category").checked(labels.is_some_and(|l| l.category)).disabled(labels.is_none()).on(move |e, _, cx| e.set_chart_ex_labels(Some(ExLabels { category: !own(labels).category, ..own(labels) }), cx)).into());
    items
}

/// 値軸 (chartEx): the least, the greatest and the major unit.
fn chart_ex_axis_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    use ibis_doc_model::Number;
    let Some((_, format)) = selected_chart_ex(editor) else { return Vec::new() };
    let (min, max, major) = (format.axis_min.map(|n| n.0), format.axis_max.map(|n| n.0), format.axis_major.map(|n| n.0));
    let min_items: Vec<MenuEntry> = vec![
        MenuItem::tr("pptx.chart.axis.min-auto").checked(min.is_none()).on(|e, _, cx| e.set_chart_ex_axis(Some(None), None, None, cx)).into(),
        MenuItem::tr("pptx.chart.axis.min-0").checked(min == Some(0.0)).on(|e, _, cx| e.set_chart_ex_axis(Some(Some(Number(0.0))), None, None, cx)).into(),
    ];
    let max_items: Vec<MenuEntry> = vec![
        MenuItem::tr("pptx.chart.axis.max-auto").checked(max.is_none()).on(|e, _, cx| e.set_chart_ex_axis(None, Some(None), None, cx)).into(),
        MenuItem::tr("pptx.chart.axis.max-100").checked(max == Some(100.0)).on(|e, _, cx| e.set_chart_ex_axis(None, Some(Some(Number(100.0))), None, cx)).into(),
        MenuItem::tr("pptx.chart.axis.max-1000").checked(max == Some(1000.0)).on(|e, _, cx| e.set_chart_ex_axis(None, Some(Some(Number(1000.0))), None, cx)).into(),
    ];
    let major_items: Vec<MenuEntry> = vec![
        MenuItem::tr("pptx.chart.axis.major-auto").checked(major.is_none()).on(|e, _, cx| e.set_chart_ex_axis(None, None, Some(None), cx)).into(),
        MenuItem::tr("pptx.chart.axis.major-10").checked(major == Some(10.0)).on(|e, _, cx| e.set_chart_ex_axis(None, None, Some(Some(Number(10.0))), cx)).into(),
        MenuItem::tr("pptx.chart.axis.major-20").checked(major == Some(20.0)).on(|e, _, cx| e.set_chart_ex_axis(None, None, Some(Some(Number(20.0))), cx)).into(),
        MenuItem::tr("pptx.chart.axis.major-50").checked(major == Some(50.0)).on(|e, _, cx| e.set_chart_ex_axis(None, None, Some(Some(Number(50.0))), cx)).into(),
        MenuItem::tr("pptx.chart.axis.major-100").checked(major == Some(100.0)).on(|e, _, cx| e.set_chart_ex_axis(None, None, Some(Some(Number(100.0))), cx)).into(),
    ];
    vec![Submenu::tr("pptx.chart.axis.min", "arrow_downward", min_items).into(), Submenu::tr("pptx.chart.axis.max", "arrow_upward", max_items).into(), Submenu::tr("pptx.chart.axis.major", "sort", major_items).into()]
}

/// 系列の塗りつぶし (chartEx): the one fill, or a waterfall's increase, decrease and total.
fn chart_ex_fill_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let Some((kind, _)) = selected_chart_ex(editor) else { return Vec::new() };
    if kind == ExKind::Tree {
        return Vec::new();
    }
    let one = |index: usize| -> Vec<MenuEntry> { color_menu(editor, "pptx.chart.series.fill", Some(crate::shell::i18n::text("pptx.chart.ex.own-color")), move |this, rgb, _, cx| this.set_chart_ex_fill(index, rgb, cx)) };
    if matches!(kind, ExKind::Columns { waterfall: true, .. }) {
        vec![
            Submenu::tr("pptx.chart.ex.increase", "trending_up", one(0)).into(),
            Submenu::tr("pptx.chart.ex.decrease", "trending_down", one(1)).into(),
            Submenu::tr("pptx.chart.ex.total", "functions", one(2)).into(),
        ]
    } else {
        one(0)
    }
}

fn chart_axis_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    use ibis_doc_model::{AxisKind, Number};
    let chart_info = editor.selected_shapes().first().and_then(|s| match &s.kind {
        ShapeKind::Chart(c) => {
            let val_ax = c.axes.iter().find(|a| a.kind == AxisKind::Value).cloned();
            let cat_ax = c.axes.iter().find(|a| a.kind == AxisKind::Category).cloned();
            Some((val_ax, cat_ax))
        }
        _ => None,
    });
    let (val_ax, cat_ax) = match chart_info {
        Some(pair) => pair,
        None => return Vec::new(),
    };

    let mut items = Vec::new();

    // 縦 (値) 軸のサブメニュー
    if let Some(va) = &val_ax {
        let mut val_items = Vec::new();

        // 最小値
        let min_val = va.min.map(|Number(v)| v);
        let min_items: Vec<MenuEntry> = vec![
            MenuItem::tr("pptx.chart.axis.min-auto").checked(min_val.is_none()).on(|e, _, cx| e.set_chart_axis_min(true, None, cx)).into(),
            MenuItem::tr("pptx.chart.axis.min-0").checked(min_val == Some(0.0)).on(|e, _, cx| e.set_chart_axis_min(true, Some(Number(0.0)), cx)).into(),
        ];
        val_items.push(Submenu::tr("pptx.chart.axis.min", "arrow_downward", min_items).into());

        // 最大値
        let max_val = va.max.map(|Number(v)| v);
        let max_items: Vec<MenuEntry> = vec![
            MenuItem::tr("pptx.chart.axis.max-auto").checked(max_val.is_none()).on(|e, _, cx| e.set_chart_axis_max(true, None, cx)).into(),
            MenuItem::tr("pptx.chart.axis.max-100").checked(max_val == Some(100.0)).on(|e, _, cx| e.set_chart_axis_max(true, Some(Number(100.0)), cx)).into(),
            MenuItem::tr("pptx.chart.axis.max-1000").checked(max_val == Some(1000.0)).on(|e, _, cx| e.set_chart_axis_max(true, Some(Number(1000.0)), cx)).into(),
        ];
        val_items.push(Submenu::tr("pptx.chart.axis.max", "arrow_upward", max_items).into());

        // 主目盛
        let major_val = va.major_unit.map(|Number(v)| v);
        let major_items: Vec<MenuEntry> = vec![
            MenuItem::tr("pptx.chart.axis.major-auto").checked(major_val.is_none()).on(|e, _, cx| e.set_chart_axis_major_unit(true, None, cx)).into(),
            MenuItem::tr("pptx.chart.axis.major-10").checked(major_val == Some(10.0)).on(|e, _, cx| e.set_chart_axis_major_unit(true, Some(Number(10.0)), cx)).into(),
            MenuItem::tr("pptx.chart.axis.major-20").checked(major_val == Some(20.0)).on(|e, _, cx| e.set_chart_axis_major_unit(true, Some(Number(20.0)), cx)).into(),
            MenuItem::tr("pptx.chart.axis.major-50").checked(major_val == Some(50.0)).on(|e, _, cx| e.set_chart_axis_major_unit(true, Some(Number(50.0)), cx)).into(),
            MenuItem::tr("pptx.chart.axis.major-100").checked(major_val == Some(100.0)).on(|e, _, cx| e.set_chart_axis_major_unit(true, Some(Number(100.0)), cx)).into(),
        ];
        val_items.push(Submenu::tr("pptx.chart.axis.major", "sort", major_items).into());

        // 表示単位
        let disp_val = va.display_unit.map(|Number(v)| v);
        let disp_items: Vec<MenuEntry> = vec![
            MenuItem::tr("pptx.chart.axis.disp-none").checked(disp_val.is_none()).on(|e, _, cx| e.set_chart_axis_display_unit(true, None, cx)).into(),
            MenuItem::tr("pptx.chart.axis.disp-hundreds").checked(disp_val == Some(100.0)).on(|e, _, cx| e.set_chart_axis_display_unit(true, Some(Number(100.0)), cx)).into(),
            MenuItem::tr("pptx.chart.axis.disp-thousands").checked(disp_val == Some(1000.0)).on(|e, _, cx| e.set_chart_axis_display_unit(true, Some(Number(1000.0)), cx)).into(),
            MenuItem::tr("pptx.chart.axis.disp-millions").checked(disp_val == Some(1_000_000.0)).on(|e, _, cx| e.set_chart_axis_display_unit(true, Some(Number(1_000_000.0)), cx)).into(),
            MenuItem::tr("pptx.chart.axis.disp-billions").checked(disp_val == Some(1_000_000_000.0)).on(|e, _, cx| e.set_chart_axis_display_unit(true, Some(Number(1_000_000_000.0)), cx)).into(),
        ];
        val_items.push(Submenu::tr("pptx.chart.axis.display-units", "tune", disp_items).into());

        val_items.push(MenuEntry::Separator);

        // 対数目盛
        let is_log = va.log_base.is_some();
        val_items.push(MenuItem::tr("pptx.chart.axis.log-scale").checked(is_log).on(|e, _, cx| e.toggle_chart_axis_log_scale(true, cx)).into());

        // 軸を反転
        let is_rev = va.reversed;
        val_items.push(MenuItem::tr("pptx.chart.axis.reversed").checked(is_rev).on(|e, _, cx| e.toggle_chart_axis_reversed(true, cx)).into());

        items.push(Submenu::tr("pptx.chart.val-axis", "border_vertical", val_items).into());
    }

    // 横 (項目) 軸のサブメニュー
    if let Some(ca) = &cat_ax {
        let is_rev = ca.reversed;
        let cat_items: Vec<MenuEntry> = vec![
            MenuItem::tr("pptx.chart.axis.reversed").checked(is_rev).on(|e, _, cx| e.toggle_chart_axis_reversed(false, cx)).into(),
        ];
        items.push(Submenu::tr("pptx.chart.cat-axis", "border_horizontal", cat_items).into());
    }

    items
}

fn chart_trendline_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    use ibis_doc_model::TrendlineType;
    let trendline_info = editor.selected_shapes().first().and_then(|s| match &s.kind {
        ShapeKind::Chart(c) => {
            let active = c.groups.iter().flat_map(|g| &g.series).find_map(|s| s.trendline.as_ref());
            let kind = active.map(|t| t.kind);
            let disp_eq = active.map_or(false, |t| t.disp_eq);
            let disp_r_sqr = active.map_or(false, |t| t.disp_r_sqr);
            Some((kind, disp_eq, disp_r_sqr))
        }
        _ => None,
    });
    let (active_kind, disp_eq, disp_r_sqr) = match trendline_info {
        Some(info) => info,
        None => return Vec::new(),
    };

    let item = |id: &'static str, target: Option<TrendlineType>| -> MenuEntry {
        let checked = active_kind == target;
        MenuItem::tr(id).checked(checked).on(move |e, _, cx| e.set_chart_trendline_type(target, cx)).into()
    };

    let mut items = vec![
        item("pptx.chart.trendline.none", None),
        item("pptx.chart.trendline.linear", Some(TrendlineType::Linear)),
        item("pptx.chart.trendline.exp", Some(TrendlineType::Exponential)),
        item("pptx.chart.trendline.log", Some(TrendlineType::Logarithmic)),
        item("pptx.chart.trendline.poly", Some(TrendlineType::Polynomial)),
        item("pptx.chart.trendline.power", Some(TrendlineType::Power)),
        item("pptx.chart.trendline.moving-avg", Some(TrendlineType::MovingAverage)),
    ];

    if active_kind.is_some() {
        items.push(MenuEntry::Separator);
        items.push(
            MenuItem::tr("pptx.chart.trendline.disp-eq")
                .checked(disp_eq)
                .on(move |e, _, cx| e.set_chart_trendline_disp_eq(!disp_eq, cx))
                .into(),
        );
        items.push(
            MenuItem::tr("pptx.chart.trendline.disp-rsqr")
                .checked(disp_r_sqr)
                .on(move |e, _, cx| e.set_chart_trendline_disp_r_sqr(!disp_r_sqr, cx))
                .into(),
        );
    }

    items
}

fn chart_error_bars_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    use ibis_doc_model::{ErrorBarType, ErrorBarValueType};
    let eb_info = editor.selected_shapes().first().and_then(|s| match &s.kind {
        ShapeKind::Chart(c) => {
            let active = c.groups.iter().flat_map(|g| &g.series).find_map(|s| s.error_bars.as_ref());
            let has_eb = active.is_some();
            let bar_type = active.map(|e| e.bar_type).unwrap_or_default();
            let val_type = active.map(|e| e.val_type).unwrap_or_default();
            let end_cap = active.map_or(true, |e| !e.no_end_cap);
            Some((has_eb, bar_type, val_type, end_cap))
        }
        _ => None,
    });
    let (has_eb, bar_type, val_type, end_cap) = match eb_info {
        Some(info) => info,
        None => return Vec::new(),
    };

    let mut items = Vec::new();
    items.push(
        MenuItem::tr("pptx.chart.error-bars.none")
            .checked(!has_eb)
            .on(|e, _, cx| e.set_chart_error_bars_shown(false, cx))
            .into(),
    );

    let val_type_item = |id: &'static str, vt: ErrorBarValueType| -> MenuEntry {
        let checked = has_eb && val_type == vt;
        MenuItem::tr(id)
            .checked(checked)
            .on(move |e, _, cx| {
                if !has_eb {
                    e.set_chart_error_bars_shown(true, cx);
                }
                e.set_chart_error_bar_val_type(vt, cx);
            })
            .into()
    };

    items.push(val_type_item("pptx.chart.error-bars.std-err", ErrorBarValueType::StandardError));
    items.push(val_type_item("pptx.chart.error-bars.fixed", ErrorBarValueType::FixedValue));
    items.push(val_type_item("pptx.chart.error-bars.percent", ErrorBarValueType::Percentage));
    items.push(val_type_item("pptx.chart.error-bars.std-dev", ErrorBarValueType::StandardDeviation));

    if has_eb {
        items.push(MenuEntry::Separator);
        let dir_item = |id: &'static str, bt: ErrorBarType| -> MenuEntry {
            let checked = bar_type == bt;
            MenuItem::tr(id).checked(checked).on(move |e, _, cx| e.set_chart_error_bar_type(bt, cx)).into()
        };
        items.push(dir_item("pptx.chart.error-bars.both", ErrorBarType::Both));
        items.push(dir_item("pptx.chart.error-bars.plus", ErrorBarType::Plus));
        items.push(dir_item("pptx.chart.error-bars.minus", ErrorBarType::Minus));

        items.push(MenuEntry::Separator);
        items.push(
            MenuItem::tr("pptx.chart.error-bars.end-cap")
                .checked(end_cap)
                .on(move |e, _, cx| e.set_chart_error_bar_end_cap(!end_cap, cx))
                .into(),
        );
    }

    items
}

/// 系列の書式設定メニュー項目（塗りつぶし、枠線、マーカー）
fn chart_series_format_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let series_list = editor.selected_shapes().first().and_then(|s| match &s.kind {
        ShapeKind::Chart(c) => {
            let mut list = Vec::new();
            for g in &c.groups {
                for s in &g.series {
                    let name = if s.name.is_empty() {
                        crate::shell::i18n::format("_pptx.series-number", &[("number", &(list.len() + 1))])
                    } else {
                        s.name.clone()
                    };
                    list.push((name, s.clone()));
                }
            }
            Some(list)
        }
        _ => None,
    }).unwrap_or_default();

    if series_list.is_empty() {
        return vec![MenuItem::tr("pptx.chart.no-series").disabled(true).into()];
    }

    let make_series_menu = |idx: Option<usize>| -> Vec<MenuEntry> {
        let mut entries = Vec::new();
        // 塗りつぶし
        let fill_entries = color_menu(editor, "pptx.chart.series.fill", Some(crate::shell::i18n::text("_shell.no-fill")), move |this, rgb, _, cx| {
            let fill = rgb.map_or(Fill::None, |r| Fill::Solid(Color::opaque(r)));
            this.set_chart_series_fill(idx, fill, cx);
        });
        entries.push(Submenu::tr("pptx.chart.series.fill", "format_color_fill", fill_entries).into());

        // 枠線
        let mut outline_entries = color_menu(editor, "pptx.chart.series.outline", Some(crate::shell::i18n::text("pptx.chrome.no-border")), move |this, rgb, _, cx| {
            let stroke = rgb.map(|r| Stroke::solid(Color::opaque(r), 19_050));
            this.set_chart_series_stroke(idx, stroke, cx);
        });
        outline_entries.push(MenuEntry::Separator);
        let widths: &[(&str, i64)] = &[
            ("pptx.chart.stroke.0-75", 9_525),
            ("pptx.chart.stroke.1-5", 19_050),
            ("pptx.chart.stroke.2-25", 28_575),
            ("pptx.chart.stroke.3-0", 38_100),
            ("pptx.chart.stroke.4-5", 57_150),
            ("pptx.chart.stroke.6-0", 76_200),
        ];
        for &(id, w) in widths {
            outline_entries.push(MenuItem::tr(id).on(move |this, _, cx| {
                this.set_chart_series_stroke_width(idx, w, cx);
            }).into());
        }
        entries.push(Submenu::tr("pptx.chart.series.outline", "border_color", outline_entries).into());

        // マーカー
        use ibis_doc_model::chart::MarkerSymbol as MS;
        let marker_symbols = [
            ("pptx.chart.marker.none", MS::None),
            ("pptx.chart.marker.circle", MS::Circle),
            ("pptx.chart.marker.square", MS::Square),
            ("pptx.chart.marker.diamond", MS::Diamond),
            ("pptx.chart.marker.triangle", MS::Triangle),
        ];
        let mut marker_entries = Vec::new();
        for (id, sym) in marker_symbols {
            marker_entries.push(MenuItem::tr(id).on(move |this, _, cx| {
                this.set_chart_series_marker_symbol(idx, sym, cx);
            }).into());
        }
        entries.push(Submenu::tr("pptx.chart.series.marker", "radio_button_checked", marker_entries).into());

        entries
    };

    if series_list.len() == 1 {
        make_series_menu(Some(0))
    } else {
        const SERIES_IDS: &[&str] = &[
            "pptx.chart.series.0", "pptx.chart.series.1", "pptx.chart.series.2", "pptx.chart.series.3",
            "pptx.chart.series.4", "pptx.chart.series.5", "pptx.chart.series.6", "pptx.chart.series.7",
            "pptx.chart.series.8", "pptx.chart.series.9", "pptx.chart.series.10", "pptx.chart.series.11",
            "pptx.chart.series.12", "pptx.chart.series.13", "pptx.chart.series.14", "pptx.chart.series.15",
        ];
        let mut items = Vec::new();
        items.push(Submenu::new("pptx.chart.series.all", "all_inclusive", crate::shell::i18n::text("pptx.chrome.all-series"), make_series_menu(None)).into());
        items.push(MenuEntry::Separator);
        for (linear_idx, (name, _)) in series_list.into_iter().enumerate() {
            let id = SERIES_IDS.get(linear_idx).copied().unwrap_or("pptx.chart.series.extra");
            items.push(Submenu::new(id, "bar_chart", SharedString::from(name), make_series_menu(Some(linear_idx))).into());
        }
        items
    }
}

/// プロット領域の書式設定メニュー項目（背景の塗りつぶし）
fn chart_plot_area_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let mut entries = Vec::new();
    let fill_entries = color_menu(editor, "pptx.chart.plot-area.fill", Some(crate::shell::i18n::text("_shell.no-fill")), |this, rgb, _, cx| {
        let fill = rgb.map_or(Fill::None, |r| Fill::Solid(Color::opaque(r)));
        this.set_chart_plot_background(fill, cx);
    });
    entries.push(Submenu::tr("pptx.chart.plot-area.fill", "format_color_fill", fill_entries).into());
    entries
}

/// グラフのスタイルメニュー項目（c:style val="1"〜"8"、またはなし）
fn chart_style_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    use super::chart_panel::CHART_STYLES;
    let current_style = editor.selected_shapes().first().and_then(|s| match &s.kind {
        ShapeKind::Chart(c) => c.style,
        _ => None,
    });
    CHART_STYLES.iter().map(|&(id, style_id)| {
        MenuItem::tr(id)
            .checked(current_style == style_id)
            .on(move |this, _, cx| {
                this.set_chart_style(style_id, cx);
            })
            .into()
    }).collect()
}

/// グラフの配色パレットメニュー項目（カラフル 1〜4、モノクロ 1〜6）
fn chart_palette_items(_editor: &SlideEditor) -> Vec<MenuEntry> {
    use super::chart_panel::COLOR_PALETTES;
    COLOR_PALETTES.iter().enumerate().map(|(idx, &(id, _))| {
        MenuItem::tr(id)
            .on(move |this, _, cx| {
                this.set_chart_color_palette(idx, cx);
            })
            .into()
    }).collect()
}

/// グラフ選択時の右クリックコンテキストメニュー項目一式
pub(crate) fn chart_context_menu_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let chart_ex = editor.selected_shapes().first().is_some_and(|s| matches!(&s.kind, ShapeKind::Chart(c) if c.ex.is_some()));
    let mut entries: Vec<MenuEntry> = vec![
        MenuItem::tr("pptx.chart.data").icon("edit_note").on(|e, w, cx| e.open_chart_data(w, cx)).into(),
        MenuItem::tr("pptx.chart.data-quick").icon("tune").disabled(chart_ex).on(|e, w, cx| e.open_chart_panel(w, cx)).into(),
    ];
    if chart_ex {
        entries.push(Submenu::tr("pptx.chart.title-menu", "title", chart_title_items(editor)).into());
        entries.push(Submenu::tr("pptx.chart.legend-menu", "legend", legend_items(editor)).into());
        if let Some((kind, _)) = selected_chart_ex(editor) {
            if kind != ExKind::Box {
                entries.push(Submenu::tr("pptx.chart.labels-menu", "label", chart_ex_labels_items(editor)).into());
            }
            if matches!(kind, ExKind::Columns { axis: true, .. } | ExKind::Box) {
                entries.push(Submenu::tr("pptx.chart.axis-menu", "timeline", chart_ex_axis_items(editor)).into());
            }
            if kind != ExKind::Tree {
                entries.push(Submenu::tr("pptx.chart.series.fill", "format_color_fill", chart_ex_fill_items(editor)).into());
            }
        }
    } else {
        let chart_group = editor.selected_shapes().first().and_then(|s| match &s.kind {
            ShapeKind::Chart(c) => c.groups.first().map(|g| g.kind),
            _ => None,
        });
        let is_pie_family = chart_group.is_some_and(|k| matches!(k, ChartType::Pie { .. } | ChartType::Doughnut { .. }));
        let is_doughnut = chart_group.is_some_and(|k| matches!(k, ChartType::Doughnut { .. }));
        let is_bubble = chart_group.is_some_and(|k| matches!(k, ChartType::Bubble { .. }));
        let is_scatter = chart_group.is_some_and(|k| matches!(k, ChartType::Scatter { .. }));

        entries.push(Submenu::tr("pptx.chrome.change-chart-type", "insert_chart", chart_type_items(editor)).into());
        entries.push(Submenu::tr("pptx.chart.title-menu", "title", chart_title_items(editor)).into());
        entries.push(Submenu::tr("pptx.chart.legend-menu", "legend", legend_items(editor)).into());
        entries.push(Submenu::tr("pptx.chart.labels-menu", "label", chart_labels_items(editor)).into());
        if is_pie_family {
            entries.push(Submenu::tr("pptx.chart.first-angle-menu", "rotate_right", chart_first_angle_items(editor)).into());
        }
        if is_doughnut {
            entries.push(Submenu::tr("pptx.chart.hole-size-menu", "donut_large", chart_hole_size_items(editor)).into());
        }
        if is_bubble {
            entries.push(Submenu::tr("pptx.chart.bubble-scale-menu", "aspect_ratio", chart_bubble_scale_items(editor)).into());
        }
        if is_scatter {
            entries.push(Submenu::tr("pptx.chart.scatter-smooth-menu", "polyline", chart_scatter_smooth_items(editor)).into());
        }
        if !is_pie_family {
            entries.push(Submenu::tr("pptx.chart.axis-menu", "timeline", chart_axis_items(editor)).into());
            entries.push(Submenu::tr("pptx.chart.trendline-menu", "show_chart", chart_trendline_items(editor)).into());
            entries.push(Submenu::tr("pptx.chart.error-bars-menu", "error", chart_error_bars_items(editor)).into());
        }
        entries.push(Submenu::tr("pptx.chart.series-format-menu", "palette", chart_series_format_items(editor)).into());
        entries.push(Submenu::tr("pptx.chart.plot-area-menu", "crop_free", chart_plot_area_items(editor)).into());
        entries.push(Submenu::tr("pptx.chart.style-menu", "palette", chart_style_items(editor)).into());
        entries.push(Submenu::tr("pptx.chart.palette-menu", "color_lens", chart_palette_items(editor)).into());
    }
    entries
}

fn field(label: SharedString, value: String) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(12.))
        .py(px(4.))
        .child(div().w(px(72.)).text_size(px(13.)).text_color(text_secondary()).child(label))
        .child(div().flex_1().h(px(32.)).px(px(8.)).flex().items_center().border_1().border_color(rgb(0x747775)).rounded(px(4.)).text_size(px(14.)).child(value))
}

fn heading(text: impl Into<SharedString>) -> Div {
    div().text_size(px(14.)).font_weight(FontWeight::MEDIUM).pt(px(10.)).pb(px(6.)).child(text.into())
}

/// The sidebar: 書式設定オプション for the selection, or the slide's settings.
pub(crate) fn sidebar(editor: &mut SlideEditor, window: &mut Window, cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
    super::sidebar_settings::sidebar(editor,window,cx)
}

pub(crate) fn reflection_preset(distance: i64, end_pos: i32) -> ibis_doc_model::Reflection {
    ibis_doc_model::Reflection {
        blur: 6_350,
        start_alpha: 52_000,
        end_alpha: 300,
        start_pos: 0,
        end_pos,
        distance,
        direction: 5_400_000,
        fade_direction: 5_400_000,
        sx: 100_000,
        sy: -100_000,
        kx: 0,
        ky: 0,
        align: ibis_doc_model::TileAlign::BottomLeft,
        rot_with_shape: false,
    }
}

/// The underline kinds PowerPoint offers (the underline button's ▼), the one in use (`now`) checked.
pub(crate) fn underline_kind_items(now: Option<ibis_doc_model::Underline>) -> Vec<MenuEntry> {
    use ibis_doc_model::Underline as U;
    const KINDS: [(&str, &str, U); 17] = [
        ("pptx.underline.sng", "docx.paragraph-decorations.single", U::Single),
        ("pptx.underline.words", "docx.toolbar.words-only", U::Words),
        ("pptx.underline.dbl", "_docx.double-underline", U::Double),
        ("pptx.underline.heavy", "_docx.thick-underline", U::Heavy),
        ("pptx.underline.dotted", "_shell.dotted", U::Dotted),
        ("pptx.underline.dottedHeavy", "docx.toolbar.thick-dotted", U::DottedHeavy),
        ("pptx.underline.dash", "_shell.dashed", U::Dash),
        ("pptx.underline.dashHeavy", "docx.toolbar.thick-dashed", U::DashHeavy),
        ("pptx.underline.dashLong", "docx.toolbar.long-dashed", U::DashLong),
        ("pptx.underline.dashLongHeavy", "docx.toolbar.thick-long-dashed", U::DashLongHeavy),
        ("pptx.underline.dotDash", "docx.picture.border.dash.dashdot", U::DotDash),
        ("pptx.underline.dotDashHeavy", "docx.toolbar.thick-dash-dot", U::DotDashHeavy),
        ("pptx.underline.dotDotDash", "_docx.dash-dot-dot", U::DotDotDash),
        ("pptx.underline.dotDotDashHeavy", "docx.toolbar.thick-dash-dot-dot", U::DotDotDashHeavy),
        ("pptx.underline.wavy", "_shell.wave", U::Wavy),
        ("pptx.underline.wavyHeavy", "docx.toolbar.thick-wave", U::WavyHeavy),
        ("pptx.underline.wavyDbl", "docx.paragraph-decorations.double-wave", U::WavyDouble),
    ];
    KINDS
        .iter()
        .map(|&(id, label, kind)| -> MenuEntry {
            MenuItem::new(id, crate::shell::i18n::text(label))
                .checked(now == Some(kind))
                .on(move |this, window, cx| this.format_characters(window, cx, CharacterFormat { underline: Some(true), underline_kind: Some(kind), ..Default::default() }))
                .into()
        })
        .collect()
}

pub(crate) fn text_color_menu_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    color_menu(editor, "pptx.text-color", None, |this, rgb, window, cx| {
        if let Some(rgb) = rgb {
            this.format_characters(window, cx, CharacterFormat { color: Some(rgb), ..Default::default() });
        }
    })
}

pub(crate) fn text_highlight_menu_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    color_menu(editor, "pptx.highlight", Some(crate::shell::i18n::text("pptx.chrome.no-color")), |this, rgb, window, cx| {
        this.format_characters(window, cx, CharacterFormat { highlight: Some(rgb), ..Default::default() });
    })
}

pub(crate) fn bullet_symbol_menu_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let list = editor.selected_list();
    [
        ("bullet.type.disc", '•'),
        ("bullet.type.circle", '○'),
        ("bullet.type.square", '■'),
        ("bullet.type.box", '□'),
        ("bullet.type.diamond", '◆'),
        ("bullet.type.arrow", '►'),
        ("bullet.type.dash", '–'),
    ]
    .into_iter()
    .map(|(id, c)| -> MenuEntry {
        let now = match list {
            Some(ListKind::Symbol(s)) => s == c,
            Some(ListKind::Bullet) => c == '•',
            _ => false,
        };
        MenuItem::tr(id)
            .checked(now)
            .on(move |this, window, cx| {
                this.format_paragraphs(
                    window,
                    cx,
                    ParagraphFormat {
                        list: Some(if c == '•' { ListKind::Bullet } else { ListKind::Symbol(c) }),
                        ..Default::default()
                    },
                )
            })
            .into()
    })
    .collect()
}

pub(crate) fn numbered_scheme_menu_items(editor: &SlideEditor) -> Vec<MenuEntry> {
    let list = editor.selected_list();
    [
        (ibis_doc_model::NumberScheme::ArabicPeriod, "number.type.arabicPeriod"),
        (ibis_doc_model::NumberScheme::ArabicParenR, "number.type.arabicParenR"),
        (ibis_doc_model::NumberScheme::AlphaLcPeriod, "number.type.alphaLcPeriod"),
        (ibis_doc_model::NumberScheme::AlphaUcPeriod, "number.type.alphaUcPeriod"),
        (ibis_doc_model::NumberScheme::AlphaLcParenR, "number.type.alphaLcParenR"),
        (ibis_doc_model::NumberScheme::RomanLcPeriod, "number.type.romanLcPeriod"),
        (ibis_doc_model::NumberScheme::RomanUcPeriod, "number.type.romanUcPeriod"),
    ]
    .into_iter()
    .map(|(s, id)| -> MenuEntry {
        let now = match list {
            Some(ListKind::Scheme(c)) => c == s,
            Some(ListKind::Numbered) => s == ibis_doc_model::NumberScheme::ArabicPeriod,
            _ => false,
        };
        MenuItem::tr(id)
            .checked(now)
            .on(move |this, window, cx| {
                this.format_paragraphs(
                    window,
                    cx,
                    ParagraphFormat {
                        list: Some(if s == ibis_doc_model::NumberScheme::ArabicPeriod {
                            ListKind::Numbered
                        } else {
                            ListKind::Scheme(s)
                        }),
                        ..Default::default()
                    },
                )
            })
            .into()
    })
    .chain([
        MenuEntry::Separator,
        Submenu::tr(
            "pptx.numbering.start-at",
            "format_list_numbered",
            vec![
                MenuItem::tr("pptx.numbering.start-at-1").on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { start_at: Some(1), ..Default::default() })).into(),
                MenuItem::tr("pptx.numbering.start-at-2").on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { start_at: Some(2), ..Default::default() })).into(),
                MenuItem::tr("pptx.numbering.start-at-3").on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { start_at: Some(3), ..Default::default() })).into(),
                MenuItem::tr("pptx.numbering.start-at-4").on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { start_at: Some(4), ..Default::default() })).into(),
                MenuItem::tr("pptx.numbering.start-at-5").on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { start_at: Some(5), ..Default::default() })).into(),
                MenuItem::tr("pptx.numbering.start-at-10").on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { start_at: Some(10), ..Default::default() })).into(),
                MenuEntry::Separator,
                MenuItem::tr("pptx.numbering.options").on(|this, window, cx| this.open_paragraph_panel(window, cx)).into(),
            ],
        )
        .into(),
    ])
    .collect()
}


/// Words in `text`: runs of letters and digits, and every Japanese or Chinese character on its own (as a word count of
/// PowerPoint's counts them, roughly).
fn count_words(text: &str) -> usize {
    let mut words = 0;
    let mut in_word = false;
    for c in text.chars() {
        let wide = matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff | 0xff66..=0xff9f);
        if wide {
            words += 1;
            in_word = false;
        } else if c.is_alphanumeric() {
            if !in_word {
                words += 1;
                in_word = true;
            }
        } else {
            in_word = false;
        }
    }
    words
}

impl SlideEditor {
    /// ツール > 文字カウント: the slides, words and characters of the presentation's slides, in the status line.
    pub(crate) fn show_word_count(&mut self, cx: &mut Context<Self>) {
        let slides = self.presentation().slides.len();
        let (mut words, mut chars) = (0usize, 0usize);
        for slide in &self.presentation().slides {
            for shape in &slide.shapes {
                if let Some(body) = shape.text_body() {
                    let text = body.text();
                    words += count_words(&text);
                    chars += text.chars().filter(|c| !c.is_whitespace()).count();
                }
            }
        }
        self.status = Some(crate::shell::i18n::format("pptx.tools.word-count", &[("slides", &slides), ("words", &words), ("chars", &chars)]).into());
        cx.notify();
    }
}
