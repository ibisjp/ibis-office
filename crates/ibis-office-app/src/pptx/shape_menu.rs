// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 右クリックメニュー on the slide: for the shapes under the pointer (cut, copy, paste, duplicate,
//! delete, order, group, size and position, alt text, link), for the text being edited (cut, copy,
//! paste), or for the empty slide (paste).
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::*;
use ibis_doc_model::{CharacterFormat, ListKind, ParagraphFormat, ShapeKind, TextAlign, TextAutofit, VerticalAnchor, VerticalKind};

use crate::pptx::SlidePopup;
use crate::pptx::editing::{Align, Order};
use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
use crate::shell::editor::SlideEditor;
use crate::xlsx::ink::stroke_points;

fn target_text_body(editor: &SlideEditor) -> Option<&ibis_doc_model::TextBody> {
    editor.edited_body().or_else(|| {
        editor.selected_shapes().into_iter().find_map(|s| match &s.kind {
            ShapeKind::Table(t) => t.rows.first().and_then(|r| r.cells.first()).map(|c| &c.text),
            _ => s.text_body(),
        })
    })
}

fn text_direction_submenu(editor: &SlideEditor) -> MenuEntry {
    let body = target_text_body(editor);
    let on = body.is_some();
    Submenu::tr("pptx.text-direction", "text_rotation_none", vec![
        MenuItem::tr("pptx.text.horizontal")
            .icon("format_textdirection_l_to_r")
            .disabled(!on)
            .checked(body.as_ref().is_some_and(|b| !b.vertical && b.sideways == 0))
            .on(|e, _, cx| e.format_text_body(|b| { b.vertical = false; b.sideways = 0; }, cx))
            .into(),
        MenuItem::tr("pptx.text.vertical")
            .icon("format_textdirection_vertical")
            .disabled(!on)
            .checked(body.as_ref().is_some_and(|b| b.vertical && b.vertical_kind == VerticalKind::EastAsian && b.sideways == 0))
            .on(|e, _, cx| e.format_text_body(|b| { b.vertical = true; b.vertical_kind = VerticalKind::EastAsian; b.sideways = 0; }, cx))
            .into(),
        MenuItem::tr("pptx.text.clockwise")
            .icon("rotate_right")
            .disabled(!on)
            .checked(body.as_ref().is_some_and(|b| !b.vertical && b.sideways == 5_400_000))
            .on(|e, _, cx| e.format_text_body(|b| { b.vertical = false; b.sideways = 5_400_000; }, cx))
            .into(),
        MenuItem::tr("pptx.text.counterclockwise")
            .icon("rotate_left")
            .disabled(!on)
            .checked(body.as_ref().is_some_and(|b| !b.vertical && b.sideways == 16_200_000))
            .on(|e, _, cx| e.format_text_body(|b| { b.vertical = false; b.sideways = 16_200_000; }, cx))
            .into(),
    ]).into()
}

fn text_vertical_alignment_submenu(editor: &SlideEditor) -> MenuEntry {
    let body = target_text_body(editor);
    let on = body.is_some();
    let anchor = body.as_ref().map(|b| b.anchor);
    Submenu::tr("_shell.vertical-alignment", "vertical_align_center", vec![
        MenuItem::tr("pptx.text.top")
            .icon("vertical_align_top")
            .disabled(!on)
            .checked(anchor == Some(VerticalAnchor::Top))
            .on(|e, _, cx| e.format_text_body(|b| b.anchor = VerticalAnchor::Top, cx))
            .into(),
        MenuItem::tr("pptx.text.middle")
            .icon("vertical_align_center")
            .disabled(!on)
            .checked(anchor == Some(VerticalAnchor::Middle))
            .on(|e, _, cx| e.format_text_body(|b| b.anchor = VerticalAnchor::Middle, cx))
            .into(),
        MenuItem::tr("pptx.text.bottom")
            .icon("vertical_align_bottom")
            .disabled(!on)
            .checked(anchor == Some(VerticalAnchor::Bottom))
            .on(|e, _, cx| e.format_text_body(|b| b.anchor = VerticalAnchor::Bottom, cx))
            .into(),
    ]).into()
}

fn text_autofit_submenu(editor: &SlideEditor) -> MenuEntry {
    let body = target_text_body(editor);
    let on = body.is_some();
    let autofit = body.as_ref().map(|b| b.autofit);
    Submenu::tr("pptx.text.autofit", "fit_screen", vec![
        MenuItem::tr("pptx.text.autofit-none")
            .icon("block")
            .disabled(!on)
            .checked(autofit == Some(TextAutofit::None))
            .on(|e, _, cx| e.set_text_autofit(TextAutofit::None, cx))
            .into(),
        MenuItem::tr("pptx.text.autofit-shrink")
            .icon("compress")
            .disabled(!on)
            .checked(autofit == Some(TextAutofit::Shrink))
            .on(|e, _, cx| e.set_text_autofit(TextAutofit::Shrink, cx))
            .into(),
        MenuItem::tr("pptx.text.autofit-resize-shape")
            .icon("open_in_full")
            .disabled(!on)
            .checked(autofit == Some(TextAutofit::ResizeShape))
            .on(|e, _, cx| e.set_text_autofit(TextAutofit::ResizeShape, cx))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.text.wrap")
            .icon("wrap_text")
            .disabled(!on)
            .checked(body.as_ref().is_some_and(|b| b.wrap))
            .on(|e, _, cx| e.format_text_body(|b| b.wrap = !b.wrap, cx))
            .into(),
    ]).into()
}

fn text_columns_submenu(editor: &SlideEditor) -> MenuEntry {
    let body = target_text_body(editor);
    let on = body.is_some();
    let current_cols = body.as_ref().map(|b| b.columns).unwrap_or(1);
    let current_spc = body.as_ref().map(|b| b.column_spacing).unwrap_or(0);
    Submenu::tr("pptx.text.columns", "view_column", vec![
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
                let cols = target_text_body(e).map(|b| b.columns).unwrap_or(1);
                e.set_text_columns(cols, spc, cx);
            }).into()
        }).collect()).into(),
    ]).into()
}

fn text_indent_submenu(editor: &SlideEditor) -> MenuEntry {
    let body = target_text_body(editor);
    let on = body.is_some();
    let first_p = body.and_then(|b| b.paragraphs.first());
    let current_ml = first_p.map(|p| p.margin_left).unwrap_or(0);
    let current_ind = first_p.map(|p| p.indent).unwrap_or(0);
    Submenu::tr("pptx.text.indent", "format_indent_increase", vec![
        MenuItem::tr("pptx.text.indent.first-line").disabled(!on).checked(current_ind > 0).on(|e, w, cx| e.set_paragraph_indent(None, Some(228_600), w, cx)).into(),
        MenuItem::tr("pptx.text.indent.hanging").disabled(!on).checked(current_ind < 0).on(|e, w, cx| e.set_paragraph_indent(Some(228_600), Some(-228_600), w, cx)).into(),
        MenuItem::tr("pptx.text.indent.none").disabled(!on).checked(current_ind == 0 && current_ml == 0).on(|e, w, cx| e.set_paragraph_indent(Some(0), Some(0), w, cx)).into(),
    ]).into()
}

fn text_tabs_submenu(editor: &SlideEditor) -> MenuEntry {
    let body = target_text_body(editor);
    let on = body.is_some();
    let has_tabs = body.and_then(|b| b.paragraphs.first()).is_some_and(|p| !p.tabs.is_empty());
    Submenu::tr("pptx.text.tabs", "keyboard_tab", vec![
        MenuItem::tr("pptx.text.tabs.default").disabled(!on).checked(has_tabs).on(|e, w, cx| e.set_paragraph_tabs(vec![
            ibis_doc_model::TextTabStop { position: 720_000, align: ibis_doc_model::TextTabAlign::Left },
            ibis_doc_model::TextTabStop { position: 1_440_000, align: ibis_doc_model::TextTabAlign::Left },
        ], w, cx)).into(),
        MenuItem::tr("pptx.text.tabs.clear").disabled(!on).checked(!has_tabs).on(|e, w, cx| e.set_paragraph_tabs(Vec::new(), w, cx)).into(),
    ]).into()
}

fn text_wordart_styles_submenu(editor: &SlideEditor) -> MenuEntry {
    Submenu::tr("pptx.text.wordart-styles", "title", crate::pptx::chrome::wordart_style_items(editor)).into()
}

fn text_warp_submenu(editor: &SlideEditor) -> MenuEntry {
    Submenu::tr("pptx.text.warp", "transform", crate::pptx::chrome::text_warp_items(editor)).into()
}

fn text_outline_submenu(_editor: &SlideEditor) -> MenuEntry {
    Submenu::tr("pptx.text-outline", "format_color_text", vec![
        MenuItem::tr("pptx.text-outline.none").on(|e, w, cx| e.set_text_outline_color(w, cx, None)).into(),
        MenuItem::tr("pptx.text-outline.black").on(|e, w, cx| e.set_text_outline_color(w, cx, Some(ibis_doc_model::Rgb(0x000000)))).into(),
        MenuItem::tr("pptx.text-outline.white").on(|e, w, cx| e.set_text_outline_color(w, cx, Some(ibis_doc_model::Rgb(0xffffff)))).into(),
        MenuItem::tr("pptx.text-outline.red").on(|e, w, cx| e.set_text_outline_color(w, cx, Some(ibis_doc_model::Rgb(0xc00000)))).into(),
        MenuItem::tr("pptx.text-outline.blue").on(|e, w, cx| e.set_text_outline_color(w, cx, Some(ibis_doc_model::Rgb(0x4472c4)))).into(),
        MenuItem::tr("pptx.text-outline.yellow").on(|e, w, cx| e.set_text_outline_color(w, cx, Some(ibis_doc_model::Rgb(0xffc000)))).into(),
        MenuItem::tr("pptx.text-outline.green").on(|e, w, cx| e.set_text_outline_color(w, cx, Some(ibis_doc_model::Rgb(0x70ad47)))).into(),
        Submenu::tr("pptx.text-outline.weight", "line_style", vec![
            MenuItem::tr("pptx.text-outline.0-5pt").on(|e, w, cx| e.set_text_outline_weight(w, cx, 6_350)).into(),
            MenuItem::tr("pptx.text-outline.1pt").on(|e, w, cx| e.set_text_outline_weight(w, cx, 12_700)).into(),
            MenuItem::tr("pptx.text-outline.1-5pt").on(|e, w, cx| e.set_text_outline_weight(w, cx, 19_050)).into(),
            MenuItem::tr("pptx.text-outline.2-25pt").on(|e, w, cx| e.set_text_outline_weight(w, cx, 28_575)).into(),
            MenuItem::tr("pptx.text-outline.3pt").on(|e, w, cx| e.set_text_outline_weight(w, cx, 38_100)).into(),
            MenuItem::tr("pptx.text-outline.4-5pt").on(|e, w, cx| e.set_text_outline_weight(w, cx, 57_150)).into(),
            MenuItem::tr("pptx.text-outline.6pt").on(|e, w, cx| e.set_text_outline_weight(w, cx, 76_200)).into(),
        ]).into(),
    ]).into()
}

fn text_case_submenu(_editor: &SlideEditor) -> MenuEntry {
    Submenu::tr("pptx.text.case", "format_size", vec![
        MenuItem::tr("pptx.case.sentence").on(|e, w, cx| e.change_case(crate::pptx::editing::TextCase::Sentence, w, cx)).into(),
        MenuItem::tr("pptx.case.lower").on(|e, w, cx| e.change_case(crate::pptx::editing::TextCase::Lower, w, cx)).into(),
        MenuItem::tr("pptx.case.upper").on(|e, w, cx| e.change_case(crate::pptx::editing::TextCase::Upper, w, cx)).into(),
        MenuItem::tr("pptx.case.title").on(|e, w, cx| e.change_case(crate::pptx::editing::TextCase::Title, w, cx)).into(),
        MenuItem::tr("pptx.case.toggle").on(|e, w, cx| e.change_case(crate::pptx::editing::TextCase::Toggle, w, cx)).into(),
    ]).into()
}

fn text_proofing_language_submenu(editor: &SlideEditor) -> MenuEntry {
    Submenu::tr("pptx.proofing-language", "language", crate::pptx::chrome::proofing_language_menu_items(editor)).into()
}

fn text_font_submenu(editor: &SlideEditor) -> MenuEntry {
    let style = editor.selected_text_style();
    let first_run = target_text_body(editor)
        .and_then(|b| b.paragraphs.iter().flat_map(|p| p.runs.first()).next().map(|r| r.style.clone()));
    let underline = first_run.as_ref().is_some_and(|r| r.underline);
    let underline_kind = first_run.as_ref().filter(|r| r.underline).map(|r| r.underline_kind);
    let strike = first_run.as_ref().is_some_and(|r| r.strike != ibis_doc_model::Strike::None);
    let caps = editor.selected_caps();
    let spacing = editor.selected_spacing();
    Submenu::tr("_shell.font", "font_download", vec![
        MenuItem::tr("_shell.bold-b").icon("format_bold").shortcut("⌘B").checked(style.as_ref().is_some_and(|s| s.bold)).on(|this, window, cx| {
            this.toggle_characters(window, cx, |b, r| b.with_run(r).bold, |on| CharacterFormat { bold: Some(on), ..Default::default() })
        }).into(),
        MenuItem::tr("_shell.italic-i").icon("format_italic").shortcut("⌘I").checked(style.as_ref().is_some_and(|s| s.italic)).on(|this, window, cx| {
            this.toggle_characters(window, cx, |b, r| b.with_run(r).italic, |on| CharacterFormat { italic: Some(on), ..Default::default() })
        }).into(),
        MenuItem::tr("_shell.underline-u").icon("format_underlined").shortcut("⌘U").checked(underline).on(|this, window, cx| {
            this.toggle_characters(window, cx, |_, r| r.underline, |on| CharacterFormat { underline: Some(on), ..Default::default() })
        }).into(),
        Submenu::new("pptx.underline-style", "format_underlined", crate::shell::i18n::text("_shell.underline-u"), crate::pptx::chrome::underline_kind_items(underline_kind)).into(),
        MenuItem::tr("pptx.chrome.strikethrough").icon("format_strikethrough").checked(strike).on(|this, window, cx| {
            this.strike_characters(window, cx)
        }).into(),
        MenuItem::tr("text.superscript").icon("superscript").shortcut("⇧⌘+").checked(editor.selected_baseline() > 0).on(|e, w, cx| e.toggle_baseline(30_000, w, cx)).into(),
        MenuItem::tr("text.subscript").icon("subscript").shortcut("⌘=").checked(editor.selected_baseline() < 0).on(|e, w, cx| e.toggle_baseline(-25_000, w, cx)).into(),
        MenuItem::tr("pptx.text.shadow").icon("ev_shadow").checked(editor.selected_shadow().is_some()).on(|this, window, cx| {
            this.toggle_text_shadow(window, cx)
        }).into(),
        MenuItem::tr("text.caps").checked(caps == ibis_doc_model::Caps::All).on(|e, w, cx| e.toggle_text_caps(ibis_doc_model::Caps::All, w, cx)).into(),
        MenuItem::tr("text.small-caps").checked(caps == ibis_doc_model::Caps::Small).on(|e, w, cx| e.toggle_text_caps(ibis_doc_model::Caps::Small, w, cx)).into(),
        Submenu::tr("pptx.text.character-spacing", "space_bar", vec![
            MenuItem::tr("pptx.spacing.very-tight").checked(spacing == -300).on(|this, window, cx| this.set_slide_character_spacing(-300, window, cx)).into(),
            MenuItem::tr("pptx.spacing.tight").checked(spacing == -150).on(|this, window, cx| this.set_slide_character_spacing(-150, window, cx)).into(),
            MenuItem::tr("pptx.spacing.normal").checked(spacing == 0).on(|this, window, cx| this.set_slide_character_spacing(0, window, cx)).into(),
            MenuItem::tr("pptx.spacing.loose").checked(spacing == 300).on(|this, window, cx| this.set_slide_character_spacing(300, window, cx)).into(),
            MenuItem::tr("pptx.spacing.very-loose").checked(spacing == 600).on(|this, window, cx| this.set_slide_character_spacing(600, window, cx)).into(),
        ]).into(),
        Submenu::tr("_shell.font-color", "format_color_text", crate::pptx::chrome::text_color_menu_items(editor)).into(),
        Submenu::tr("pptx.chrome.highlight-color", "ink_highlighter", crate::pptx::chrome::text_highlight_menu_items(editor)).into(),
        MenuItem::tr("pptx.replace-fonts").icon("font_download").on(|e, w, cx| e.open_replace_fonts_panel(w, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("_shell.clear-formatting").icon("format_clear").shortcut("⌘\\").on(|this, window, cx| {
            this.clear_formats(window, cx)
        }).into(),
    ]).into()
}

fn text_horizontal_align_submenu(editor: &SlideEditor) -> MenuEntry {
    let align = editor.selected_text_style().map(|s| s.align);
    Submenu::tr("_shell.arrange", "format_align_left", vec![
        MenuItem::tr("align.left").icon("format_align_left").checked(align == Some(TextAlign::Left)).on(|this, window, cx| {
            this.format_paragraphs(window, cx, ParagraphFormat { align: Some(TextAlign::Left), ..Default::default() })
        }).into(),
        MenuItem::tr("align.center").icon("format_align_center").checked(align == Some(TextAlign::Center)).on(|this, window, cx| {
            this.format_paragraphs(window, cx, ParagraphFormat { align: Some(TextAlign::Center), ..Default::default() })
        }).into(),
        MenuItem::tr("align.right").icon("format_align_right").checked(align == Some(TextAlign::Right)).on(|this, window, cx| {
            this.format_paragraphs(window, cx, ParagraphFormat { align: Some(TextAlign::Right), ..Default::default() })
        }).into(),
        MenuItem::tr("align.justify").icon("format_align_justify").checked(align == Some(TextAlign::Justify)).on(|this, window, cx| {
            this.format_paragraphs(window, cx, ParagraphFormat { align: Some(TextAlign::Justify), ..Default::default() })
        }).into(),
        MenuItem::tr("align.distributed").icon("format_align_justify").checked(align == Some(TextAlign::Distributed)).on(|this, window, cx| {
            this.format_paragraphs(window, cx, ParagraphFormat { align: Some(TextAlign::Distributed), ..Default::default() })
        }).into(),
    ]).into()
}

fn text_line_spacing_submenu(editor: &SlideEditor) -> MenuEntry {
    let current_ls = target_text_body(editor)
        .and_then(|b| b.paragraphs.first())
        .and_then(|p| p.line_spacing);
    Submenu::tr("_shell.line-paragraph-spacing", "format_line_spacing", vec![
        MenuItem::new("line-spacing.1", "1.0")
            .checked(current_ls == Some(ibis_doc_model::LineSpacing::Percent(100_000)))
            .on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { line_spacing: Some(Some(ibis_doc_model::LineSpacing::Percent(100_000))), ..Default::default() }))
            .into(),
        MenuItem::new("line-spacing.1-15", "1.15")
            .checked(current_ls == Some(ibis_doc_model::LineSpacing::Percent(115_000)))
            .on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { line_spacing: Some(Some(ibis_doc_model::LineSpacing::Percent(115_000))), ..Default::default() }))
            .into(),
        MenuItem::new("line-spacing.1-5", "1.5")
            .checked(current_ls == Some(ibis_doc_model::LineSpacing::Percent(150_000)))
            .on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { line_spacing: Some(Some(ibis_doc_model::LineSpacing::Percent(150_000))), ..Default::default() }))
            .into(),
        MenuItem::new("line-spacing.2", "2.0")
            .checked(current_ls == Some(ibis_doc_model::LineSpacing::Percent(200_000)))
            .on(|this, window, cx| this.format_paragraphs(window, cx, ParagraphFormat { line_spacing: Some(Some(ibis_doc_model::LineSpacing::Percent(200_000))), ..Default::default() }))
            .into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.paragraph.settings").icon("tune").on(|e, w, cx| e.open_paragraph_panel(w, cx)).into(),
    ]).into()
}

fn text_bullets_submenu(editor: &SlideEditor) -> MenuEntry {
    let list = editor.selected_list();
    let bullet_on = matches!(list, Some(ListKind::Bullet | ListKind::Symbol(_)));
    let number_on = matches!(list, Some(ListKind::Numbered | ListKind::Scheme(_)));
    let mut items: Vec<MenuEntry> = vec![
        MenuItem::tr("_shell.bullets").icon("format_list_bulleted").checked(bullet_on).on(|this, window, cx| {
            let on = matches!(this.selected_list(), Some(ListKind::Bullet | ListKind::Symbol(_)));
            let kind = if on { ListKind::None } else { ListKind::Bullet };
            this.format_paragraphs(window, cx, ParagraphFormat { list: Some(kind), ..Default::default() })
        }).into(),
        MenuItem::tr("_shell.numbering").icon("format_list_numbered").checked(number_on).on(|this, window, cx| {
            let on = matches!(this.selected_list(), Some(ListKind::Numbered | ListKind::Scheme(_)));
            let kind = if on { ListKind::None } else { ListKind::Numbered };
            this.format_paragraphs(window, cx, ParagraphFormat { list: Some(kind), ..Default::default() })
        }).into(),
        MenuEntry::Separator,
    ];
    items.extend(crate::pptx::chrome::bullet_symbol_menu_items(editor));
    items.push(MenuEntry::Separator);
    items.push(Submenu::tr("pptx.chrome.numbering-style", "format_list_numbered", crate::pptx::chrome::numbered_scheme_menu_items(editor)).into());
    Submenu::tr("_shell.bullets", "format_list_bulleted", items).into()
}

impl SlideEditor {
    /// The SmartArt commands of a selected diagram's menu (the text pane, the shapes, the layout and the colours): a
    /// slide's, and a worksheet's.
    pub(crate) fn smartart_shape_menu_entries(&self) -> Vec<MenuEntry> {
        let mut entries: Vec<MenuEntry> = Vec::new();
        let current_sa = self.selected_smartart_data();
        let current_kind = current_sa.as_ref().map(|(_, k, ..)| *k);
        let current_style = current_sa.as_ref().map(|(_, _, st, ..)| *st);
        entries.push(MenuItem::tr("pptx.smartart.text-pane").icon("list_alt").checked(self.pptx().is_some_and(|s| s.smartart_text_pane)).on(|e, _, cx| e.toggle_smartart_text_pane(cx)).into());
        entries.push(MenuItem::tr("pptx.smartart.add-shape").icon("add").on(|e, _, cx| e.smartart_add_shape(cx)).into());
        entries.push(MenuItem::tr("pptx.smartart.remove-shape").icon("remove").on(|e, _, cx| e.smartart_remove_shape(cx)).into());
        entries.push(MenuItem::tr("pptx.smartart.move-up").icon("arrow_upward").on(|e, _, cx| e.smartart_move_item_up(cx)).into());
        entries.push(MenuItem::tr("pptx.smartart.move-down").icon("arrow_downward").on(|e, _, cx| e.smartart_move_item_down(cx)).into());
        entries.push(MenuItem::tr("pptx.smartart.promote").icon("format_indent_decrease").on(|e, _, cx| e.smartart_promote(cx)).into());
        entries.push(MenuItem::tr("pptx.smartart.demote").icon("format_indent_increase").on(|e, _, cx| e.smartart_demote(cx)).into());
        if current_kind == Some(crate::pptx::SmartArtKind::OrganizationChart) {
            entries.push(MenuItem::tr("pptx.smartart.add-assistant").icon("person_add").on(|e, _, cx| e.smartart_add_assistant(cx)).into());
        }
        entries.push(Submenu::tr("pptx.smartart.change-layout", "account_tree", [
            ("pptx.smartart.layout.list", crate::pptx::SmartArtKind::BasicBlockList),
            ("pptx.smartart.layout.vlist", crate::pptx::SmartArtKind::VerticalList),
            ("pptx.smartart.layout.process", crate::pptx::SmartArtKind::BasicProcess),
            ("pptx.smartart.layout.chevron", crate::pptx::SmartArtKind::ContinuousChevronProcess),
            ("pptx.smartart.layout.cycle", crate::pptx::SmartArtKind::BasicCycle),
            ("pptx.smartart.layout.hierarchy", crate::pptx::SmartArtKind::OrganizationChart),
            ("pptx.smartart.layout.relationship", crate::pptx::SmartArtKind::BasicRelationship),
            ("pptx.smartart.layout.matrix", crate::pptx::SmartArtKind::BasicMatrix),
            ("pptx.smartart.layout.pyramid", crate::pptx::SmartArtKind::BasicPyramid),
            ("pptx.smartart.layout.picture", crate::pptx::SmartArtKind::PictureList),
        ].into_iter().map(|(id, k)| {
            MenuItem::tr(id).checked(current_kind == Some(k)).on(move |e, _, cx| e.change_smartart_layout(k, cx)).into()
        }).collect()).into());
        entries.push(Submenu::tr("pptx.smartart.color", "palette", [
            ("pptx.smartart.color.colorful", crate::pptx::SmartArtColorStyle::Colorful),
            ("pptx.smartart.color.accent1", crate::pptx::SmartArtColorStyle::Accent1),
            ("pptx.smartart.color.accent2", crate::pptx::SmartArtColorStyle::Accent2),
            ("pptx.smartart.color.accent3", crate::pptx::SmartArtColorStyle::Accent3),
            ("pptx.smartart.color.outline", crate::pptx::SmartArtColorStyle::Outline),
        ].into_iter().map(|(id, st)| {
            MenuItem::tr(id).checked(current_style == Some(st)).on(move |e, _, cx| e.set_smartart_color_style(st, cx)).into()
        }).collect()).into());
        entries
    }

    /// The menu's entries for what is selected or edited now.
    pub(crate) fn shape_context_menu_entries(&self) -> Vec<MenuEntry> {
        let editing = self.text_editing().is_some();
        let edit = self.text_editing();
        let is_table_cell_editing = edit.as_ref().is_some_and(|e| e.cell.is_some());
        let has_selection = !self.selected_ids().is_empty();
        let has_table_selected = self.selected_shapes().iter().any(|s| matches!(s.kind, ShapeKind::Table(_)));
        let is_table = is_table_cell_editing || has_table_selected;
        let range = edit.as_ref().is_some_and(|e| !e.range().is_empty());
        let can_copy = if editing { range } else { has_selection && self.selected_shapes().iter().all(|s| ibis_pptx::save::check_shape_insertion(s).is_ok()) };
        let mut entries: Vec<MenuEntry> = vec![
            MenuItem::tr("edit.cut").icon("content_cut").shortcut("⌘X").disabled(!can_copy).on(|e, _, cx| {
                if e.text_editing().is_some() {
                    e.copy_edited_text(true, cx);
                } else {
                    e.cut_selected(cx);
                }
            }).into(),
            MenuItem::tr("edit.copy").icon("content_copy").shortcut("⌘C").disabled(!can_copy).on(|e, _, cx| {
                if e.text_editing().is_some() {
                    e.copy_edited_text(false, cx);
                } else {
                    e.copy_selected();
                    e.write_shapes_to_system_clipboard(cx);
                    e.stamp_shape_clipboard(cx);
                }
            }).into(),
            MenuItem::tr("edit.paste").icon("content_paste").shortcut("⌘V").on(|e, w, cx| {
                if !e.paste_edited_text(cx) {
                    e.paste_from_clipboards(w, cx);
                }
            }).into(),
        ];

        // Table operations (available when editing a table cell or when a table shape is selected)
        if is_table {
            let can_merge = edit.as_ref().is_some_and(|e| e.cell.is_some() && e.cell_anchor.is_some());
            let can_split = edit.as_ref().and_then(|e| Some((e.shape, e.cell?))).is_some_and(|(id, (r, c))| {
                self.slide().shape(id).is_some_and(|s| matches!(&s.kind, ShapeKind::Table(t) if t.rows.get(r).and_then(|row| row.cells.get(c)).is_some_and(|x| !x.merged && (x.row_span > 1 || x.column_span > 1))))
            });
            let merged = self.selected_shapes().first().is_some_and(|s| matches!(&s.kind, ShapeKind::Table(t) if t.has_merges()));

            entries.extend([
                MenuEntry::Separator,
                Submenu::tr("_shell.insert", "add", vec![
                    MenuItem::tr("table.insert-above").icon("arrow_upward").disabled(merged).on(|e, w, cx| e.table_insert_row(false, w, cx)).into(),
                    MenuItem::tr("table.insert-below").icon("arrow_downward").disabled(merged).on(|e, w, cx| e.table_insert_row(true, w, cx)).into(),
                    MenuItem::tr("table.insert-left").icon("border_left").disabled(merged).on(|e, w, cx| e.table_insert_column(false, w, cx)).into(),
                    MenuItem::tr("table.insert-right").icon("arrow_forward").disabled(merged).on(|e, w, cx| e.table_insert_column(true, w, cx)).into(),
                ]).into(),
                Submenu::tr("_shell.delete", "delete", vec![
                    MenuItem::tr("table.remove-row").icon("delete").disabled(merged).on(|e, w, cx| e.table_remove_row(w, cx)).into(),
                    MenuItem::tr("table.remove-column").icon("delete").disabled(merged).on(|e, w, cx| e.table_remove_column(w, cx)).into(),
                    MenuItem::tr("table.remove-table").icon("delete_forever").on(|e, _, cx| e.delete_selected(cx)).into(),
                ]).into(),
                MenuItem::tr("table.merge").icon("table").disabled(!can_merge).on(|e, w, cx| e.merge_table_cells(w, cx)).into(),
                MenuItem::tr("table.split").icon("table").disabled(!can_split).on(|e, w, cx| e.split_table_cell(w, cx)).into(),
                Submenu::tr("pptx.table.distribute", "horizontal_distribute", vec![
                    MenuItem::tr("table.distribute-rows").icon("vertical_distribute").on(|e, w, cx| e.distribute_table(true, w, cx)).into(),
                    MenuItem::tr("table.distribute-columns").icon("horizontal_distribute").on(|e, w, cx| e.distribute_table(false, w, cx)).into(),
                ]).into(),
                MenuItem::tr("table.cell-size").icon("height").disabled(!self.can_set_cell_size()).on(|e, w, cx| e.open_cell_size_panel(w, cx)).into(),
                Submenu::tr("pptx.common.fill", "format_color_fill", crate::pptx::chrome::shape_fill_menu_items(self)).into(),
                Submenu::tr("_shell.border-color", "border_color", crate::pptx::chrome::shape_stroke_color_items(self, false)).into(),
                Submenu::tr("pptx.chrome.border-weight", "line_weight", crate::pptx::chrome::shape_stroke_width_items(self)).into(),
                Submenu::tr("pptx.chrome.border-style", "line_style", crate::pptx::chrome::shape_stroke_style_items(self)).into(),
                Submenu::tr("table.borders", "border_all", crate::pptx::chrome::table_borders_menu_items(true)).into(),
                Submenu::tr("pptx.table-style", "grid_view", crate::pptx::chrome::table_style_items(self, true)).into(),
                Submenu::tr("pptx.table-style-options", "table", crate::pptx::chrome::table_style_option_items(self, true)).into(),
            ]);
            if !editing {
                entries.extend([
                    text_vertical_alignment_submenu(self),
                    text_direction_submenu(self),
                    MenuItem::tr("pptx.insets-actions").icon("padding").on(|e, w, cx| e.open_text_insets_panel(w, cx)).into(),
                ]);
            }
        }

        // Text editing operations (when editing text, including in table cells)
        if editing {
            entries.extend([
                MenuEntry::Separator,
                MenuItem::tr("pptx.format-copy").icon("format_paint").shortcut("⌥⌘C").on(|e, w, cx| e.copy_character_format(w, cx)).into(),
                MenuItem::tr("pptx.format-paste").icon("content_paste").shortcut("⌥⌘V").disabled(self.pptx().is_none_or(|s| s.character_format_copy.is_none())).on(|e, w, cx| e.paste_character_format(w, cx)).into(),
                MenuItem::tr("pptx.paragraph-copy").icon("format_paint").on(|e, _, cx| e.copy_paragraph_format(cx)).into(),
                MenuItem::tr("pptx.paragraph-paste").icon("content_paste").disabled(self.pptx().is_none_or(|s| s.paragraph_format_copy.is_none())).on(|e, w, cx| e.paste_paragraph_format(w, cx)).into(),
                MenuItem::tr("_shell.clear-formatting").icon("format_clear").shortcut("⌘\\").on(|this, window, cx| this.clear_formats(window, cx)).into(),
                MenuEntry::Separator,
                text_font_submenu(self),
                text_horizontal_align_submenu(self),
                text_line_spacing_submenu(self),
                text_bullets_submenu(self),
                text_direction_submenu(self),
                text_vertical_alignment_submenu(self),
                text_autofit_submenu(self),
                text_columns_submenu(self),
                text_indent_submenu(self),
                text_tabs_submenu(self),
                text_wordart_styles_submenu(self),
                text_warp_submenu(self),
                text_outline_submenu(self),
                text_case_submenu(self),
                text_proofing_language_submenu(self),
                MenuItem::tr("pptx.spelling.check").icon("spellcheck").on(|e, w, cx| e.open_spelling(w, cx)).into(),
                MenuItem::tr("pptx.thesaurus").icon("menu_book").on(|e, w, cx| e.open_pptx_thesaurus(w, cx)).into(),
                MenuItem::tr("pptx.insets-actions").icon("padding").on(|e, w, cx| e.open_text_insets_panel(w, cx)).into(),
                self.slide_hyperlink_context_entry(),
            ]);
            return entries;
        }

        if !has_selection {
            let can_delete = self.presentation().slides.len() >= 2;
            let is_skipped = self.slide().hidden;
            entries.extend([
                MenuEntry::Separator,
                MenuItem::tr("pptx.new-slide").icon("add").shortcut("⌘M").on(|e, w, cx| e.add_slide(w, cx)).into(),
                Submenu::tr("pptx.new-slide-layout", "view_quilt", crate::pptx::chrome::new_slide_menu(self)).into(),
                MenuItem::tr("pptx.duplicate-slide").icon("control_point_duplicate").shortcut("⌘D").on(|e, w, cx| e.duplicate_current_slide(w, cx)).into(),
                MenuItem::tr("pptx.delete-slide").icon("delete").disabled(!can_delete).on(|e, w, cx| e.delete_slide(w, cx)).into(),
                MenuItem::tr("pptx.section.add").icon("create_new_folder").on(|e, w, cx| e.open_section_panel(false, w, cx)).into(),
                Submenu::tr("pptx.sections", "view_quilt", crate::pptx::chrome::section_menu_items(self)).into(),
                MenuEntry::Separator,
                Submenu::tr("pptx.layout", "view_quilt", crate::pptx::chrome::apply_layout_items(self)).into(),
                MenuItem::tr("pptx.reset-slide").icon("restart_alt").disabled(self.slide().layout.is_none()).on(|e, w, cx| e.reset_slide(w, cx)).into(),
                MenuItem::tr("pptx.reset-placement").disabled(self.slide().layout.is_none()).on(|e, w, cx| e.reset_placeholder_frames(w, cx)).into(),
                Submenu::tr("pptx.transition", "animation", crate::pptx::chrome::transition_menu(self)).into(),
                Submenu::tr("pptx.slide-size", "aspect_ratio", crate::pptx::chrome::slide_size_menu_items(self)).into(),
                MenuEntry::Separator,
                Submenu::tr("pptx.grid-and-guides", "grid_on", vec![
                    MenuItem::tr("pptx.gridlines").icon("grid_on").shortcut("⇧F9").checked(self.is_pptx_gridlines_enabled()).on(|this, _, cx| this.toggle_pptx_gridlines(cx)).into(),
                    MenuItem::tr("pptx.guides").icon("straighten").shortcut("⌥F9").checked(self.is_pptx_guides_enabled()).on(|this, _, cx| this.toggle_pptx_guides(cx)).into(),
                    MenuEntry::Separator,
                    MenuItem::tr("pptx.snap-to-grid").checked(self.is_pptx_snap_to_grid_enabled()).on(|this, _, cx| this.toggle_pptx_snap_to_grid(cx)).into(),
                    MenuItem::tr("pptx.snap-to-shape").checked(self.is_pptx_snap_to_shape_enabled()).on(|this, _, cx| this.toggle_pptx_snap_to_shape(cx)).into(),
                ]).into(),
                Submenu::tr("pptx.background", "wallpaper", crate::pptx::chrome::background_menu(self)).into(),
                Submenu::tr("pptx.theme.palette-selected", "palette", crate::pptx::chrome::theme_palette_selected_items(self)).into(),
                MenuEntry::Separator,
                MenuItem::tr("pptx.skip-slide").icon("visibility_off").checked(is_skipped).on(|e, w, cx| e.toggle_skip_slide(w, cx)).into(),
                MenuItem::tr("pptx.slide.add-hidden-title").icon("title").on(|e, _, cx| {
                    let s_id = e.current_slide;
                    e.add_hidden_slide_title(s_id, None, cx);
                }).into(),
                MenuItem::tr("insert.comment").icon("add_comment").shortcut("⌥⌘M").on(|e, w, cx| e.open_slide_comments(w, cx)).into(),
                MenuItem::tr("pptx.notes").icon("notes").checked(self.notes_shown()).on(|e, w, cx| e.toggle_notes(w, cx)).into(),
                MenuItem::tr("pptx.header-footer").icon("view_quilt").on(|e, w, cx| e.open_header_footer(w, cx)).into(),
                MenuEntry::Separator,
                MenuItem::tr("pptx.export-slide-png").icon("image").on(|e, w, cx| e.export_current_slide_png(w, cx)).into(),
                MenuItem::tr("pptx.export-all-slides-png").icon("image").on(|e, w, cx| e.export_all_slides_png(w, cx)).into(),
                MenuItem::tr("pptx.export-gif").icon("animation").on(|e, w, cx| e.export_gif(w, cx)).into(),
            ]);
            if self.is_master_view() {
                let placeholder_items: Vec<MenuEntry> = [
                    (ibis_pptx::master_page::PlaceholderKind::Title, "pptx.master-view.placeholder.title", "title"),
                    (ibis_pptx::master_page::PlaceholderKind::Text, "pptx.master-view.placeholder.text", "notes"),
                    (ibis_pptx::master_page::PlaceholderKind::Content, "pptx.master-view.placeholder.content", "dashboard"),
                    (ibis_pptx::master_page::PlaceholderKind::Picture, "pptx.master-view.placeholder.picture", "image"),
                    (ibis_pptx::master_page::PlaceholderKind::Chart, "pptx.master-view.placeholder.chart", "insert_chart"),
                    (ibis_pptx::master_page::PlaceholderKind::Table, "pptx.master-view.placeholder.table", "table"),
                    (ibis_pptx::master_page::PlaceholderKind::Media, "pptx.master-view.placeholder.media", "movie"),
                ]
                .into_iter()
                .map(|(kind, label, icon)| {
                    MenuItem::tr(label)
                        .icon(icon)
                        .on(move |e, w, cx| e.insert_master_placeholder(kind, w, cx))
                        .into()
                })
                .collect();
                entries.extend([
                    MenuEntry::Separator,
                    Submenu::tr("pptx.master-view.placeholder.insert", "add_box", placeholder_items).into(),
                    MenuItem::tr("pptx.theme-colors").icon("palette").on(|e, w, cx| e.open_theme_panel(w, cx)).into(),
                    MenuItem::tr("pptx.master-view.close-master-view").icon("close").on(|e, w, cx| e.close_master_view(w, cx)).into(),
                ]);
            }
            if self.is_page_view() {
                entries.push(MenuEntry::Separator);
                entries.extend(self.notes_format_context_menu_entries());
            }
            return entries;
        }

        let mut shape_actions: Vec<MenuEntry> = Vec::new();
        if self.selected_ids().len() == 1 {
            let shape_id = self.selected_ids()[0];
            let can_edit_text = self.slide().shape(shape_id).is_some_and(|s| s.text_body().is_some() || matches!(s.kind, ShapeKind::Auto(_)));
            if can_edit_text {
                shape_actions.push(MenuItem::tr("pptx.text.edit").icon("edit").on(move |e, window, cx| {
                    e.begin_text_edit(shape_id, false, false, window, cx);
                }).into());
            }
        }
        if self.selected_shapes().iter().any(|s| matches!(s.kind, ShapeKind::Auto(_))) {
            let change_items: Vec<MenuEntry> = crate::pptx::editing::SHAPES
                .iter()
                .zip(crate::pptx::editing::CHANGE_SHAPE_IDS)
                .map(|(&(preset, icon, label), id)| {
                    MenuItem::new(id, crate::shell::i18n::text(label))
                        .icon(icon)
                        .on(move |e, _, cx| e.change_selected_shapes(preset, cx))
                        .into()
                })
                .collect();
            shape_actions.push(Submenu::tr("pptx.chrome.change-shape", "shapes", change_items).into());
        }
        if self.can_edit_points() {
            let editing_pts = self.is_editing_points();
            shape_actions.push(
                MenuItem::tr("pptx.edit-points")
                    .icon("polyline")
                    .checked(editing_pts)
                    .on(move |e, _, cx| {
                        if editing_pts {
                            e.end_edit_points(cx);
                        } else {
                            e.begin_edit_points(cx);
                        }
                    })
                    .into(),
            );
            if let Some(kind) = self.edit_point_selected_kind() {
                use ibis_doc_model::shape_merge::NodeKind;
                for (id, icon, k) in [
                    ("pptx.edit-points.smooth", "line_curve", NodeKind::Smooth),
                    ("pptx.edit-points.straight", "polyline", NodeKind::Straight),
                    ("pptx.edit-points.corner", "turn_sharp_right", NodeKind::Corner),
                ] {
                    shape_actions.push(
                        MenuItem::tr(id)
                            .icon(icon)
                            .checked(kind == k)
                            .on(move |e, _, cx| e.set_selected_vertex_kind(k, cx))
                            .into(),
                    );
                }
            }
        }

        if !shape_actions.is_empty() {
            entries.push(MenuEntry::Separator);
            entries.extend(shape_actions);
        }

        entries.extend([
            MenuItem::tr("edit.duplicate").icon("control_point_duplicate").shortcut("⌘D").on(|e, _, cx| e.duplicate_selected(cx)).into(),
            MenuItem::tr("edit.delete").icon("delete").shortcut("⌫").on(|e, _, cx| e.delete_selected(cx)).into(),
            MenuEntry::Separator,
            Submenu::tr("pptx.order", "flip_to_front", vec![
                MenuItem::tr("pptx.order.front").icon("flip_to_front").on(|e, _, cx| e.order_selected(Order::Front, cx)).into(),
                MenuItem::tr("pptx.order.forward").icon("arrow_upward").on(|e, _, cx| e.order_selected(Order::Forward, cx)).into(),
                MenuItem::tr("pptx.order.backward").icon("arrow_downward").on(|e, _, cx| e.order_selected(Order::Backward, cx)).into(),
                MenuItem::tr("pptx.order.back").icon("flip_to_back").on(|e, _, cx| e.order_selected(Order::Back, cx)).into(),
            ]).into(),
            Submenu::tr("pptx.align", "align_horizontal_left", vec![
                MenuItem::tr("pptx.align.left").icon("align_horizontal_left").on(|e, _, cx| e.align_selected(Align::Left, cx)).into(),
                MenuItem::tr("pptx.align.center").icon("align_horizontal_center").on(|e, _, cx| e.align_selected(Align::Center, cx)).into(),
                MenuItem::tr("pptx.align.right").icon("align_horizontal_right").on(|e, _, cx| e.align_selected(Align::Right, cx)).into(),
                MenuEntry::Separator,
                MenuItem::tr("pptx.align.top").icon("align_vertical_top").on(|e, _, cx| e.align_selected(Align::Top, cx)).into(),
                MenuItem::tr("pptx.align.middle").icon("align_vertical_center").on(|e, _, cx| e.align_selected(Align::Middle, cx)).into(),
                MenuItem::tr("pptx.align.bottom").icon("align_vertical_bottom").on(|e, _, cx| e.align_selected(Align::Bottom, cx)).into(),
                MenuEntry::Separator,
                MenuItem::tr("pptx.distribute.h").icon("horizontal_distribute").disabled(self.selected_ids().len() < 3).on(|e, _, cx| e.distribute_selected(true, cx)).into(),
                MenuItem::tr("pptx.distribute.v").icon("vertical_distribute").disabled(self.selected_ids().len() < 3).on(|e, _, cx| e.distribute_selected(false, cx)).into(),
                MenuEntry::Separator,
                MenuItem::tr("pptx.center.h").icon("align_horizontal_center").on(|e, _, cx| e.center_selected(true, cx)).into(),
                MenuItem::tr("pptx.center.v").icon("align_vertical_center").on(|e, _, cx| e.center_selected(false, cx)).into(),
                MenuEntry::Separator,
                MenuItem::tr("pptx.align.to-slide").icon("crop_landscape").checked(self.pptx().is_some_and(|s| s.align_to_slide)).on(|this, _, cx| {
                    if let Some(s) = this.pptx_mut() {
                        s.align_to_slide = !s.align_to_slide;
                    }
                    cx.notify();
                }).into(),
            ]).into(),
            Submenu::tr("pptx.rotate", "rotate_right", vec![
                MenuItem::tr("pptx.rotate.right").icon("rotate_90_degrees_cw").on(|e, _, cx| e.rotate_selected(90, cx)).into(),
                MenuItem::tr("pptx.rotate.left").icon("rotate_90_degrees_ccw").on(|e, _, cx| e.rotate_selected(-90, cx)).into(),
                MenuItem::tr("pptx.rotate.flip-h").icon("flip").on(|e, _, cx| e.flip_selected(true, cx)).into(),
                MenuItem::tr("pptx.rotate.flip-v").icon("flip").on(|e, _, cx| e.flip_selected(false, cx)).into(),
            ]).into(),
        ]);
        if self.can_group() {
            entries.push(MenuItem::tr("pptx.group").icon("group_work").shortcut("⌥⌘G").on(|e, _, cx| e.group_selected(cx)).into());
        }
        if self.can_ungroup() {
            entries.push(MenuItem::tr("pptx.ungroup").icon("workspaces").shortcut("⌥⇧⌘G").on(|e, _, cx| e.ungroup_selected(cx)).into());
        }
        if self.can_merge_shapes() {
            use ibis_doc_model::shape_merge::MergeOp;
            let item = |id: &'static str, icon: &'static str, op: MergeOp| -> MenuEntry {
                MenuItem::tr(id).icon(icon).on(move |this, window, cx| this.merge_selected_shapes(op, window, cx)).into()
            };
            entries.push(
                Submenu::tr("pptx.merge", "cell_merge", vec![
                    item("pptx.merge.union", "cell_merge", MergeOp::Union),
                    item("pptx.merge.combine", "cell_merge", MergeOp::Combine),
                    item("pptx.merge.fragment", "content_cut", MergeOp::Fragment),
                    item("pptx.merge.intersect", "cell_merge", MergeOp::Intersect),
                    item("pptx.merge.subtract", "cell_merge", MergeOp::Subtract),
                ])
                .into(),
            );
        }
        if self.can_convert_diagram() {
            entries.push(MenuItem::tr("pptx.smartart-to-shapes").icon("shapes").on(|e, _, cx| e.convert_diagram_to_shapes(cx)).into());
        }
        if self.can_modify_smartart_items() {
            entries.extend(self.smartart_shape_menu_entries());
        }
        if self.can_convert_text_to_smartart() {
            entries.push(MenuItem::tr("pptx.smartart.convert-text").icon("account_tree").on(|e, _, cx| e.convert_selected_text_to_smartart(crate::pptx::SmartArtKind::BasicProcess, cx)).into());
        }
        let has_line_or_connector = self.selected_shapes().iter().any(|s| match &s.kind {
            ibis_doc_model::ShapeKind::Line(_) => true,
            ibis_doc_model::ShapeKind::Auto(auto) => crate::pptx::editing::is_connector_geometry(&auto.geometry),
            _ => false,
        });
        if has_line_or_connector {
            entries.push(Submenu::tr("pptx.chrome.begin-arrow", "line_start", crate::pptx::chrome::line_end_menu(self, true)).into());
            entries.push(Submenu::tr("pptx.chrome.end-arrow", "line_end_arrow_notch", crate::pptx::chrome::line_end_menu(self, false)).into());
        }
        let has_auto = self.selected_shapes().iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Auto(_)));
        let has_pic = self.selected_shapes().iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Picture(_)));
        if has_auto && !is_table {
            entries.push(Submenu::tr("pptx.common.fill", "format_color_fill", crate::pptx::chrome::shape_fill_menu_items(self)).into());
        }
        if (has_auto || has_pic || has_line_or_connector) && !is_table {
            let only_line = has_line_or_connector && !has_auto && !has_pic;
            entries.push(Submenu::tr("_shell.border-color", "border_color", crate::pptx::chrome::shape_stroke_color_items(self, only_line)).into());
            entries.push(Submenu::tr("pptx.chrome.border-weight", "line_weight", crate::pptx::chrome::shape_stroke_width_items(self)).into());
            entries.push(Submenu::tr("pptx.chrome.border-style", "line_style", crate::pptx::chrome::shape_stroke_style_items(self)).into());
        }
        if has_auto || has_pic {
            let mut shadow_items: Vec<MenuEntry> = [
                ("pptx.shadow.none", None),
                ("pptx.shadow.right", Some((0, 0))),
                ("pptx.shadow.diagonal", Some((2_700_000, 0))),
                ("pptx.shadow.soft", Some((2_700_000, 50_800))),
            ].into_iter().map(|(id, value)| {
                let shadow = value.map(|(direction, blur)| ibis_doc_model::Shadow::simple(ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0), alpha: 128 }, blur, 50_800, direction));
                MenuItem::tr(id).on(move |e, _, cx| e.set_shape_shadow(shadow, cx)).into()
            }).collect();
            if has_auto {
                shadow_items.push(MenuEntry::Separator);
                shadow_items.push(MenuItem::tr("pptx.shadow.custom").on(|e, w, cx| e.open_shadow_panel(w, cx)).into());
            }
            entries.push(Submenu::tr("pptx.shadow", "format_color_fill", shadow_items).into());
            entries.push(Submenu::tr("pptx.reflection", "flip", [
                ("pptx.reflection.none", None),
                ("pptx.reflection.tight", Some(crate::pptx::chrome::reflection_preset(0, 35_000))),
                ("pptx.reflection.half", Some(crate::pptx::chrome::reflection_preset(38_100, 55_000))),
                ("pptx.reflection.full", Some(crate::pptx::chrome::reflection_preset(38_100, 90_000))),
            ].into_iter().map(|(id, value)| MenuItem::tr(id).on(move |e, _, cx| e.set_shape_reflection(value, cx)).into()).collect()).into());
            entries.push(Submenu::tr("pptx.soft-edge", "auto_awesome", [
                ("pptx.soft-edge.none", None),
                ("pptx.soft-edge.1pt", Some(12_700)),
                ("pptx.soft-edge.2-5pt", Some(31_750)),
                ("pptx.soft-edge.5pt", Some(63_500)),
                ("pptx.soft-edge.10pt", Some(127_000)),
                ("pptx.soft-edge.25pt", Some(317_500)),
                ("pptx.soft-edge.50pt", Some(635_000)),
            ].into_iter().map(|(id, value)| MenuItem::tr(id).on(move |e, _, cx| e.set_shape_soft_edge(value, cx)).into()).collect()).into());
        }
        if has_auto {
            let mut glow_items: Vec<MenuEntry> = [
                ("pptx.glow.none", None),
                ("pptx.glow.blue5", Some((ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0x4472C4), alpha: 128 }, 63_500))),
                ("pptx.glow.blue10", Some((ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0x4472C4), alpha: 128 }, 127_000))),
                ("pptx.glow.yellow10", Some((ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(0xFFC000), alpha: 160 }, 127_000))),
            ].into_iter().map(|(id, value)| MenuItem::tr(id).on(move |e, _, cx| e.set_shape_glow(value, cx)).into()).collect();
            glow_items.push(MenuEntry::Separator);
            glow_items.push(MenuItem::tr("pptx.glow.custom").on(|e, w, cx| e.open_glow_panel(w, cx)).into());
            entries.push(Submenu::tr("pptx.glow", "format_color_fill", glow_items).into());
            entries.push(Submenu::tr("pptx.bevel", "layers", [
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
            ].into_iter().map(|(id, value)| MenuItem::tr(id).on(move |e, _, cx| e.set_shape_bevel(value, cx)).into()).collect()).into());
            entries.push(Submenu::tr("pptx.3d-rotation", "3d_rotation", [
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
            ].into_iter().map(|(id, value)| MenuItem::tr(id).on(move |e, _, cx| e.set_shape_3d_rotation_preset(value, cx)).into()).collect()).into());
            entries.push(Submenu::tr("pptx.3d-depth", "view_in_ar", [
                ("pptx.3d-depth.none", 0),
                ("pptx.3d-depth.10pt", 127_000),
                ("pptx.3d-depth.20pt", 254_000),
                ("pptx.3d-depth.36pt", 457_200),
                ("pptx.3d-depth.72pt", 914_400),
            ].into_iter().map(|(id, value)| MenuItem::tr(id).on(move |e, _, cx| e.set_shape_3d_depth(value, None, cx)).into()).collect()).into());
        }
        if has_pic {
            entries.push(MenuItem::tr("pptx.crop-actions").icon("crop").on(|e, w, cx| e.open_crop_panel(w, cx)).into());
            entries.push(Submenu::tr("pptx.chrome.crop-picture", "crop", crate::pptx::chrome::crop_items()).into());
            entries.push(MenuItem::tr("pptx.chrome.reset-crop").icon("reset_image").on(|e, _, cx| e.crop_selected_pictures(ibis_doc_model::Crop::default(), cx)).into());
            entries.push(Submenu::tr("pptx.video.shape", "crop_original", crate::pptx::chrome::video_shape_items(self, true)).into());
            entries.push(MenuItem::tr("pptx.chrome.replace-picture").icon("swap_horiz").on(|e, w, cx| e.pick_replacement_picture(w, cx)).into());
            entries.push(MenuItem::tr("pptx.picture.remove-background").icon("auto_fix_high").on(|e, _, cx| e.set_selected_pictures_clear(Some(ibis_doc_model::Rgb(0xffffff)), cx)).into());
            entries.push(Submenu::tr("pptx.picture.color", "palette", crate::pptx::chrome::picture_color_items(true)).into());
            entries.push(Submenu::tr("pptx.picture.correction", "tune", crate::pptx::chrome::picture_correction_items(self, true)).into());
            entries.push(Submenu::tr("pptx.picture.artistic-effect", "auto_fix_high", crate::pptx::chrome::picture_artistic_effect_items(true)).into());
            #[cfg(all(target_os = "macos", feature = "export"))]
            entries.push(MenuItem::tr("pptx.picture.compress").icon("image").on(|e, w, cx| e.open_compress_panel(w, cx)).into());
            entries.push(MenuItem::tr("pptx.picture.reset-colors")
                .icon("restart_alt")
                .disabled(!self.selected_shapes().iter().any(|s| matches!(&s.kind, ibis_doc_model::ShapeKind::Picture(p) if p.duotone.is_some() || p.clear.is_some() || p.luminance.is_some() || p.artistic_effect.is_some())))
                .on(|e, _, cx| e.reset_picture_color_effects(cx)).into());
            entries.push(MenuItem::tr("pptx.video.reset-effects")
                .icon("restart_alt")
                .disabled(!self.selected_shapes().iter().any(|s| matches!(&s.kind, ibis_doc_model::ShapeKind::Picture(p) if p.stroke.is_some() || p.corner > 0 || p.shadow.is_some() || p.reflection.is_some() || p.soft_edge.is_some() || p.artistic_effect.is_some())))
                .on(|e, _, cx| e.reset_picture_effects(cx)).into());
            if self.has_selected_svg_picture() {
                entries.push(MenuItem::tr("pptx.picture.convert-to-shape").icon("category").on(|e, _, cx| { e.convert_selected_svg_to_shape(cx); }).into());
            }
        }
        if self.selected_shapes().iter().any(|s| SlideEditor::clip_of(s).is_some()) {
            entries.push(Submenu::tr("pptx.clip.playback", "play_circle", crate::pptx::chrome::clip_playback_items(self)).into());
            entries.push(Submenu::tr("pptx.video.styles", "style", crate::pptx::chrome::video_style_items(self, true)).into());
            entries.push(Submenu::tr("pptx.media.compress", "video_settings", vec![
                MenuItem::tr("pptx.media.compress-1080p").on(|e, _, cx| { e.compress_presentation_media(crate::pptx::MediaQuality::FullHd, cx); }).into(),
                MenuItem::tr("pptx.media.compress-720p").on(|e, _, cx| { e.compress_presentation_media(crate::pptx::MediaQuality::Hd, cx); }).into(),
                MenuItem::tr("pptx.media.compress-480p").on(|e, _, cx| { e.compress_presentation_media(crate::pptx::MediaQuality::Standard, cx); }).into(),
                MenuItem::tr("pptx.media.optimize-compatibility").on(|e, _, cx| { e.optimize_media_compatibility(cx); }).into(),
            ]).into());
        }
        entries.extend(crate::pptx::chrome::zoom_context_menu_items(self));
        if self.selected_shapes().iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Chart(_))) {
            entries.extend(crate::pptx::chrome::chart_context_menu_items(self));
        }
        if self.selected_shapes().iter().any(|s| stroke_points(s).is_some()) {
            entries.push(MenuItem::tr("pptx.ink.to-shape").icon("shapes").on(|e, _, cx| e.pptx_ink_to_shape(cx)).into());
            entries.push(MenuItem::tr("pptx.ink.to-text").icon("text_fields").on(|e, _, cx| e.pptx_ink_to_text(cx)).into());
            entries.push(MenuItem::tr("pptx.ink.to-math").icon("functions").on(|e, _, cx| e.pptx_ink_to_math(cx)).into());
            entries.push(MenuItem::tr("pptx.ink.replay").icon("play_arrow").on(|e, _, cx| e.pptx_ink_replay(cx)).into());
        }
        if self.selected_shapes().iter().any(|s| s.text_body().is_some()) {
            entries.extend([
                MenuEntry::Separator,
                text_font_submenu(self),
                text_horizontal_align_submenu(self),
                text_line_spacing_submenu(self),
                text_bullets_submenu(self),
                text_direction_submenu(self),
                text_vertical_alignment_submenu(self),
                text_autofit_submenu(self),
                text_columns_submenu(self),
                text_indent_submenu(self),
                text_tabs_submenu(self),
                text_wordart_styles_submenu(self),
                text_warp_submenu(self),
                text_outline_submenu(self),
                text_case_submenu(self),
                text_proofing_language_submenu(self),
                MenuItem::tr("pptx.format-copy").icon("format_paint").shortcut("⌥⌘C").on(|e, w, cx| e.copy_character_format(w, cx)).into(),
                MenuItem::tr("pptx.format-paste").icon("content_paste").shortcut("⌥⌘V").disabled(self.pptx().is_none_or(|s| s.character_format_copy.is_none())).on(|e, w, cx| e.paste_character_format(w, cx)).into(),
                MenuItem::tr("pptx.paragraph-copy").icon("format_paint").on(|e, _, cx| e.copy_paragraph_format(cx)).into(),
                MenuItem::tr("pptx.paragraph-paste").icon("content_paste").disabled(self.pptx().is_none_or(|s| s.paragraph_format_copy.is_none())).on(|e, w, cx| e.paste_paragraph_format(w, cx)).into(),
                MenuItem::tr("_shell.clear-formatting").icon("format_clear").shortcut("⌘\\").on(|this, window, cx| this.clear_formats(window, cx)).into(),
                MenuItem::tr("pptx.spelling.check").icon("spellcheck").on(|e, w, cx| e.open_spelling(w, cx)).into(),
                MenuItem::tr("pptx.thesaurus").icon("menu_book").on(|e, w, cx| e.open_pptx_thesaurus(w, cx)).into(),
                MenuItem::tr("pptx.insets-actions").icon("padding").on(|e, w, cx| e.open_text_insets_panel(w, cx)).into(),
            ]);
        }
        entries.extend([
            MenuEntry::Separator,
            Submenu::tr("pptx.menu.animation", "animation", crate::pptx::animation_ui::animation_items(self)).into(),
            self.slide_hyperlink_context_entry(),
            MenuItem::tr("pptx.frame.settings").icon("straighten").on(|e, w, cx| e.open_frame_panel(w, cx)).into(),
            MenuItem::tr("pptx.alt-text").icon("accessibility").on(|e, w, cx| e.open_alt_text_panel(w, cx)).into(),
        ]);
        if self.selected_shapes().iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Auto(_) | ibis_doc_model::ShapeKind::Line(_))) {
            entries.push(MenuItem::tr("pptx.default-shape").icon("bookmark").disabled(!matches!(self.selected_shapes().as_slice(), [s] if matches!(s.kind, ibis_doc_model::ShapeKind::Auto(_) | ibis_doc_model::ShapeKind::Line(_)))).on(|e, _, cx| e.set_object_default(cx)).into());
        }
        if self.selected_shapes().iter().any(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Picture(_))) {
            entries.push(MenuItem::tr("pptx.format-picture").icon("image").on(|e, w, cx| e.open_crop_panel(w, cx)).into());
        } else if has_selection {
            entries.push(MenuItem::tr("pptx.format-shape").icon("format_shapes").on(|e, w, cx| e.open_gradient_panel(false, w, cx)).into());
        }
        if self.selected_math().is_some() {
            entries.push(MenuItem::tr("pptx.math.title").icon("functions").on(|e, w, cx| e.open_pptx_math_panel_for_selected(w, cx)).into());
            entries.push(MenuItem::tr("pptx.math.copy-selection-latex").icon("functions").on(|e, _, cx| e.copy_selected_math_as_latex(cx)).into());
        }
        entries
    }

    /// A right click on the slide at `position` (the window's): the shape under it is selected (when it is not
    /// part of the selection) and the menu opens there.
    pub(crate) fn on_slide_right_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(layout) = self.slide_layout() else { return };
        if self.pptx_tool() != crate::pptx::editing::Tool::Select || self.slideshow.is_some() {
            return;
        }
        cx.stop_propagation();
        let p = crate::pptx::interaction::P::of(layout.to_emu(event.position));
        if self.is_editing_points() {
            let v_slop = crate::pptx::interaction::HANDLE_HIT_PX as f64 / layout.scale as f64;
            let nodes = self.edit_point_nodes();
            let mut hit_vertex = None;
            for (l, figure) in nodes.iter().enumerate() {
                for (i, n) in figure.iter().enumerate() {
                    if (n.p.0 - p.x).hypot(n.p.1 - p.y) <= v_slop {
                        hit_vertex = Some((l, i));
                        break;
                    }
                }
                if hit_vertex.is_some() {
                    break;
                }
            }
            if let Some((l, i)) = hit_vertex {
                if let Some(e) = self.pptx_mut().and_then(|s| s.edit_points.as_mut()) {
                    e.selected = Some((l, i));
                }
                self.focus_handle.focus(window, cx);
                let entries = std::rc::Rc::new(self.shape_context_menu_entries());
                let editor = cx.entity().downgrade();
                let menu = PopupMenu::build(window, cx, move |menu, window, cx| crate::shell::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx));
                let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
                    if let Some(state) = this.pptx_mut() {
                        state.slide_popup = None;
                    }
                    cx.notify();
                    this.focus_handle.focus(window, cx);
                });
                menu.read(cx).focus_handle(cx).focus(window, cx);
                if let Some(state) = self.pptx_mut() {
                    state.slide_popup = Some(SlidePopup { menu, position: event.position, _dismiss: dismiss });
                }
                cx.notify();
                return;
            }
        }
        if let Some((target,link))=self.slide_link_target_at(layout.to_emu(event.position)){
            let id=match &target{crate::pptx::links::LinkTarget::Shape(id)|crate::pptx::links::LinkTarget::Text{shape:id,..}|crate::pptx::links::LinkTarget::CellText{shape:id,..}=>*id};
            self.set_selection(vec![id]);
            let entry=self.slide_hyperlink_menu_for(target,link);
            let mut entries=self.shape_context_menu_entries();
            entries.retain(|e|match e{crate::shell::chrome::MenuEntry::Item(i)=>i.id!="pptx.link",crate::shell::chrome::MenuEntry::Submenu(s)=>s.id!="hyperlink.menu",_=>true});
            entries.push(entry);
            self.open_hyperlink_context(event.position,entries,window,cx);return;
        }
        let slop = crate::pptx::interaction::LINE_SLOP_PX as f64 / layout.scale as f64;
        let hit = crate::pptx::interaction::hit_test(self.slide(), p, slop);
        let in_edited_text = hit.is_some_and(|id| self.text_editing().is_some_and(|e| e.shape == id));
        if !in_edited_text {
            self.end_text_edit(window, cx);
            if let Some(id) = hit {
                if !self.selected_ids().contains(&id) {
                    let mut target = vec![id];
                    for g in &self.slide().groups {
                        let mut leaves = Vec::new();
                        Self::collect_group_leaves(g, &mut leaves);
                        if leaves.contains(&id) {
                            target = leaves;
                            break;
                        }
                    }
                    self.set_selection(target);
                }
                // If right-clicked on a table, start editing the clicked cell so that table row/col actions target it
                if let Some(shape) = self.slide().shape(id)
                    && let Some(cell) = crate::pptx::interaction::table_cell_at(shape, p)
                {
                    self.begin_table_cell_edit(id, cell, window, cx);
                }
            } else {
                self.set_selection(Vec::new());
            }
        } else if let Some(id) = hit
            && let Some(shape) = self.slide().shape(id)
            && let Some(cell) = crate::pptx::interaction::table_cell_at(shape, p)
            && self.text_editing().is_some_and(|e| e.shape == id && e.cell.is_some_and(|c| c != cell))
        {
            // Right-clicked a different cell in the currently edited table
            self.begin_table_cell_edit(id, cell, window, cx);
        }
        self.focus_handle.focus(window, cx);
        let entries = std::rc::Rc::new(self.shape_context_menu_entries());
        let editor = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| crate::shell::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx));
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(state) = this.pptx_mut() {
                state.slide_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        if let Some(state) = self.pptx_mut() {
            state.slide_popup = Some(SlidePopup { menu, position: event.position, _dismiss: dismiss });
        }
        cx.notify();
    }
}
