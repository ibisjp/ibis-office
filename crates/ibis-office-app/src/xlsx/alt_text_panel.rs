// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 代替テキスト (Alt Text): the selected shapes' title and description for accessibility (`cNvPr/@descr`, `@title`).

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{AltText, Command, ShapeId, SlideId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

pub(crate) struct AltTextPanel {
    pub(crate) title_field: Entity<InputState>,
    pub(crate) desc_field: Entity<InputState>,
    pub(crate) target: (SlideId, ShapeId),
    pub(crate) initial: AltText,
}

impl SlideEditor {
    /// The shape (shape, picture, chart) selected on the current worksheet.
    pub(crate) fn selected_sheet_shape(&self) -> Option<ShapeId> {
        if let Some(id) = self.selection {
            if let Some(shape) = self.slide().shapes.iter().find(|s| s.id == id) {
                if !matches!(shape.kind, ibis_doc_model::ShapeKind::Sheet(_)) {
                    return Some(id);
                }
            }
        }
        None
    }

    /// 書式 > 代替テキスト…: open the Alt Text panel in the sidebar for the selected shape or chart.
    pub(crate) fn open_sheet_alt_text_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        let Some(shape_id) = self.selected_sheet_shape() else {
            self.status = Some(crate::shell::i18n::text("_xlsx.select-a-shape-or-chart").into());
            return cx.notify();
        };
        let current_alt = self.slide().shape_alt_texts.get(&shape_id).cloned().unwrap_or_default();
        let initial = current_alt.clone();
        let title_field = cx.new(|cx| InputState::new(window, cx).default_value(current_alt.title));
        let desc_field = cx.new(|cx| InputState::new(window, cx).default_value(current_alt.description));

        let state = self.wb_mut();
        state.close_settings();
        state.sort_edit = None;
        state.conditional_edit = None;
        state.chart_edit = None;
        state.panels.alt_text = Some(AltTextPanel {
            title_field,
            desc_field: desc_field.clone(),
            target: (slide, shape_id),
            initial,
        });
        self.sidebar_open = true;
        desc_field.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    /// Apply the entered title and description to the shape's alternative text.
    pub(crate) fn apply_sheet_alt_text(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.wb().panels.alt_text.as_ref() else { return };
        let (slide, shape) = panel.target;
        let title = panel.title_field.read(cx).value().trim().to_string();
        let description = panel.desc_field.read(cx).value().trim().to_string();
        let new_alt = AltText { title, description };
        if new_alt != panel.initial {
            self.run_sheet(vec![Command::SetShapeAltText { slide, shape, alt_text: new_alt }], cx);
        }
    }

    /// Close the Alt Text panel in the sidebar.
    pub(crate) fn close_sheet_alt_text_panel(&mut self, cx: &mut Context<Self>) {
        self.apply_sheet_alt_text(cx);
        self.wb_mut().panels.alt_text = None;
        self.sidebar_open = false;
        cx.notify();
    }

    /// Render the Alt Text sidebar panel.
    pub(crate) fn sheet_alt_text_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.wb().panels.alt_text.as_ref()?;
        let t = crate::shell::i18n::text;
        let title_field = panel.title_field.clone();
        let desc_field = panel.desc_field.clone();

        let body = v_flex()
            .gap_3()
            .p_3()
            .p_0().child(
                div()
                    .text_sm()
                    .text_color(rgb(0x5f6368))
                    .child(t("xlsx.alt-text.hint")),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(t("xlsx.alt-text.title")))
                    .child(crate::shell::chrome::text_field(&title_field).full_width()),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(t("xlsx.alt-text.description")))
                    .child(crate::shell::chrome::text_field(&desc_field).full_width()),
            )
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(
                        crate::shell::chrome::ActionDefinition::new("alt-text-apply", "", crate::shell::chrome::ActionButtonKind::Primary)
                            .label(t("xlsx.alt-text.apply"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.apply_sheet_alt_text(cx);
                                this.wb_mut().panels.alt_text = None;
                                this.sidebar_open = false;
                                cx.notify();
                            })),
                    )

        .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(t("xlsx.alt-text"), body.into_any_element(), panel_actions))
    }
}
