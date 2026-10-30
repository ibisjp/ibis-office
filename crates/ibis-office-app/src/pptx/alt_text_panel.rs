// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 代替テキスト (Alt Text): the selected shapes' title and description for accessibility (`cNvPr/@descr`, `@title`).
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{h_flex, v_flex, Selectable as _, Sizable as _};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{AltText, Command, ShapeId, SlideId};

use crate::pptx::SlidePopup;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

pub(crate) struct AltTextPanel {
    pub(crate) title_field: Entity<InputState>,
    pub(crate) desc_field: Entity<InputState>,
    pub(crate) target: (SlideId, ShapeId),
    pub(crate) initial: AltText,
    pub(crate) decorative: bool,
    pub(crate) initial_decorative: bool,
}

impl SlideEditor {
    pub(crate) fn open_alt_text_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let slide = self.current_slide;
        let Some(shape_id) = self.selected_shapes().first().map(|s| s.id) else { return };
        let current_alt = self.slide().shape_alt_texts.get(&shape_id).cloned().unwrap_or_default();
        let current_decorative = self.slide().decorative_shapes.contains(&shape_id);
        let initial = current_alt.clone();
        let title_field = cx.new(|cx| InputState::new(window, cx).default_value(current_alt.title));
        let desc_field = cx.new(|cx| InputState::new(window, cx).default_value(current_alt.description));
        if let Some(s) = self.pptx_mut() {
            s.timing_panel = None;
            s.section_panel = None;
            s.gradient_panel = None;
            s.shadow_panel = None;
            s.glow_panel = None;
            s.paragraph_panel = None;
            s.find = None;
            s.size_panel = None;
            s.insets_panel = None;
            s.crop_panel = None;
            s.comments = None;
            s.chart_panel = None;
            s.header_footer = None;
            s.frame_panel = None;
            s.accessibility_panel = None;
            s.alt_text_panel = Some(AltTextPanel {
                title_field,
                desc_field: desc_field.clone(),
                target: (slide, shape_id),
                initial,
                decorative: current_decorative,
                initial_decorative: current_decorative,
            });
        }
        self.sidebar_open = true;
        desc_field.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn toggle_alt_text_decorative(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = self.pptx_mut().and_then(|s| s.alt_text_panel.as_mut()) {
            panel.decorative = !panel.decorative;
            cx.notify();
        }
    }

    pub(crate) fn apply_alt_text(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.alt_text_panel.as_ref()) else { return };
        let (slide, shape) = panel.target;
        let title = panel.title_field.read(cx).value().trim().to_string();
        let description = panel.desc_field.read(cx).value().trim().to_string();
        let new_alt = AltText { title, description };
        let new_decorative = panel.decorative;

        let mut commands = Vec::new();
        if new_decorative != panel.initial_decorative {
            commands.push(Command::SetShapeDecorative { slide, shape, decorative: new_decorative });
        }
        if new_alt != panel.initial {
            commands.push(Command::SetShapeAltText { slide, shape, alt_text: new_alt.clone() });
        }
        if !commands.is_empty() {
            self.run(commands, cx);
        }
        if let Some(panel) = self.pptx_mut().and_then(|s| s.alt_text_panel.as_mut()) {
            panel.initial = new_alt;
            panel.initial_decorative = new_decorative;
        }
    }

    pub(crate) fn close_alt_text_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_alt_text(cx);
        if let Some(s) = self.pptx_mut() {
            s.alt_text_panel = None;
        }
        self.sidebar_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn clear_alt_text_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.alt_text_panel.as_ref()) else { return };
        let (title_field, desc_field) = (panel.title_field.clone(), panel.desc_field.clone());
        title_field.update(cx, |s, cx| s.set_value("", window, cx));
        desc_field.update(cx, |s, cx| s.set_value("", window, cx));
        if let Some(panel) = self.pptx_mut().and_then(|s| s.alt_text_panel.as_mut()) {
            panel.decorative = false;
        }
        self.apply_alt_text(cx);
        cx.notify();
    }

    pub(crate) fn generate_alt_text_from_shape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.alt_text_panel.as_ref()) else { return };
        let (slide_id, shape_id) = panel.target;
        let (title_field, desc_field) = (panel.title_field.clone(), panel.desc_field.clone());
        let Some(slide) = self.presentation().slide(slide_id) else { return };
        let Some(shape) = slide.shape(shape_id) else { return };
        let title = slide
            .shape_names
            .get(&shape_id)
            .map(|n| n.trim().to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| format!("Shape {}", shape.id.0));
        let text_content: String = shape
            .text_body()
            .map(|b| {
                b.paragraphs
                    .iter()
                    .map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>())
                    .filter(|s| !s.trim().is_empty())
                    .collect::<Vec<_>>()
                    .join(" / ")
            })
            .unwrap_or_default();
        let desc = if !text_content.is_empty() {
            format!("{}: {}", title, text_content)
        } else {
            title.clone()
        };
        title_field.update(cx, |s, cx| s.set_value(title, window, cx));
        desc_field.update(cx, |s, cx| s.set_value(desc, window, cx));
        if let Some(panel) = self.pptx_mut().and_then(|s| s.alt_text_panel.as_mut()) {
            panel.decorative = false;
        }
        self.apply_alt_text(cx);
        cx.notify();
    }

    pub(crate) fn copy_alt_text_description(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.alt_text_panel.as_ref()) else { return };
        let title = panel.title_field.read(cx).value().trim().to_string();
        let desc = panel.desc_field.read(cx).value().trim().to_string();
        let combined = match (!title.is_empty(), !desc.is_empty()) {
            (true, true) => format!("{title}\n{desc}"),
            (true, false) => title,
            (false, true) => desc,
            (false, false) => return,
        };
        cx.write_to_clipboard(ClipboardItem::new_string(combined));
    }

    pub(crate) fn alt_text_panel_context_menu_entries(&self, cx: &App) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let Some(panel) = self.pptx().and_then(|s| s.alt_text_panel.as_ref()) else {
            return Vec::new();
        };
        let decorative = panel.decorative;
        let has_text = !panel.title_field.read(cx).value().trim().is_empty()
            || !panel.desc_field.read(cx).value().trim().is_empty();
        vec![
            MenuItem::tr("pptx.alt-text.apply")
                .icon("check")
                .on(|this, _, cx| this.apply_alt_text(cx))
                .into(),
            MenuItem::tr("pptx.alt-text.decorative")
                .checked(decorative)
                .on(|this, _, cx| this.toggle_alt_text_decorative(cx))
                .into(),
            MenuItem::new(
                "pptx.alt-text.generate-from-text",
                crate::shell::i18n::text("pptx.alt-text"),
            )
            .icon("auto_awesome")
            .on(|this, window, cx| this.generate_alt_text_from_shape(window, cx))
            .into(),
            MenuEntry::Separator,
            MenuItem::tr("edit.copy")
                .icon("content_copy")
                .disabled(!has_text)
                .on(|this, _, cx| this.copy_alt_text_description(cx))
                .into(),
            MenuItem::tr("edit.delete")
                .icon("delete")
                .disabled(!has_text && !decorative)
                .on(|this, window, cx| this.clear_alt_text_fields(window, cx))
                .into(),
            MenuItem::tr("pptx.check-accessibility")
                .icon("accessibility")
                .on(|this, window, cx| this.open_pptx_accessibility_panel(window, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|this, window, cx| this.close_alt_text_panel(window, cx))
                .into(),
        ]
    }

    pub(crate) fn open_alt_text_panel_context_menu(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let items = self.alt_text_panel_context_menu_entries(cx);
        if items.is_empty() {
            return;
        }
        self.focus_handle.focus(window, cx);
        let entries = std::rc::Rc::new(items);
        let editor = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| {
            crate::shell::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx)
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(state) = this.pptx_mut() {
                state.slide_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        if let Some(state) = self.pptx_mut() {
            state.slide_popup = Some(SlidePopup { menu, position, _dismiss: dismiss });
        }
        cx.notify();
    }

    pub(crate) fn alt_text_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.pptx()?.alt_text_panel.as_ref()?;
        let t = crate::shell::i18n::text;
        let title_field = panel.title_field.clone();
        let desc_field = panel.desc_field.clone();
        let decorative = panel.decorative;

        let mut body = v_flex()
            .id("pptx-alt-text-panel")
            .gap_3()
            .p_3()
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_alt_text_panel_context_menu(event.position, window, cx);
                }),
            )
            .child(
                crate::shell::chrome::checkbox_button("alt-text-decorative",t("pptx.alt-text.decorative"),decorative,false,cx.listener(|this, _, _, cx| {
                        this.toggle_alt_text_decorative(cx);
                    })),
            );

        if decorative {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(rgb(0x5f6368))
                    .child(t("pptx.alt-text.decorative.hint")),
            );
        } else {
            body = body
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x5f6368))
                        .child(t("pptx.alt-text.hint")),
                )
                .child(
                    v_flex()
                        .gap_1()
                        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(t("pptx.alt-text.title")))
                        .child(crate::shell::chrome::text_field(&title_field).full_width()),
                )
                .child(
                    v_flex()
                        .gap_1()
                        .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(t("pptx.alt-text.description")))
                        .child(crate::shell::chrome::text_field(&desc_field).full_width()),
                );
        }
        let panel_actions = crate::shell::chrome::action_bar()
                .action(
                    crate::shell::chrome::ActionDefinition::new("alt-text-apply", t("pptx.alt-text.apply"), crate::shell::chrome::ActionButtonKind::Primary)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.close_alt_text_panel(window, cx);
                        })),
                )

            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(t("pptx.alt-text"), body.into_any_element(), panel_actions))
    }
}
