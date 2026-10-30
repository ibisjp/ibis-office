// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Document template attachment and automatic style update panel (DOCX-STYLE-017, DOCX-FILE-009).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use ibis_doc_model::flow_command::FlowCommand;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;
use crate::shell::icons::Material;

/// Panel state for attaching a document template and managing automatic style updates.
pub(crate) struct TemplateAttachPanel {
    pub(crate) path_input: Entity<InputState>,
    pub(crate) status_message: Option<String>,
    pub(crate) error_message: Option<String>,
}

impl TemplateAttachPanel {
    pub(crate) fn new(initial_path: Option<&str>, window: &mut Window, cx: &mut Context<SlideEditor>) -> Self {
        let text_val = initial_path.unwrap_or("").to_string();
        let input = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(text_val, window, cx);
            s
        });
        Self {
            path_input: input,
            status_message: None,
            error_message: None,
        }
    }
}

impl SlideEditor {
    /// ツール/開発 > テンプレートとアドイン…: open the template attachment panel.
    pub(crate) fn open_docx_template_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let current_path = self.docx_attached_template().map(str::to_string);
        let panel = TemplateAttachPanel::new(current_path.as_deref(), window, cx);
        if let Some(state) = self.doc_state_mut() {
            state.template_panel = Some(panel);
        }
        cx.notify();
    }

    /// Close the template attachment panel.
    pub(crate) fn close_docx_template_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            if state.template_panel.take().is_some() {
                cx.notify();
            }
        }
    }

    /// Whether the template attachment panel is currently open.
    pub(crate) fn is_docx_template_panel_open(&self) -> bool {
        self.doc_state().and_then(|s| s.template_panel.as_ref()).is_some()
    }

    /// Currently attached template path, if any.
    pub(crate) fn docx_attached_template(&self) -> Option<&str> {
        let flow = self.flow()?;
        flow.attached_template.as_deref()
    }

    /// Whether automatic style updating from the attached template is enabled.
    pub(crate) fn docx_link_styles(&self) -> bool {
        self.flow().is_some_and(|f| f.link_styles)
    }

    /// Attach a template path and set link_styles option (1-step undoable).
    pub(crate) fn set_docx_attached_template(&mut self, template: Option<String>, link_styles: bool, cx: &mut Context<Self>) {
        let cmd = FlowCommand::AttachTemplate {
            template: template.filter(|s| !s.trim().is_empty()),
            link_styles,
        };
        self.run_flow(vec![cmd], cx);
    }

    /// Toggle automatic style update setting (`link_styles`).
    pub(crate) fn toggle_docx_link_styles(&mut self, cx: &mut Context<Self>) {
        let current_template = self.docx_attached_template().map(str::to_string);
        let next_link = !self.docx_link_styles();
        self.set_docx_attached_template(current_template, next_link, cx);
    }

    /// Detach template and disable link_styles.
    pub(crate) fn detach_docx_template(&mut self, cx: &mut Context<Self>) {
        self.set_docx_attached_template(None, false, cx);
        if let Some(panel) = self.doc_state_mut().and_then(|s| s.template_panel.as_mut()) {
            panel.status_message = Some(text("docx.template.detached").to_string());
            panel.error_message = None;
        }
        cx.notify();
    }

    /// Apply template path from the input field in the panel.
    pub(crate) fn apply_docx_template_from_panel(&mut self, cx: &mut Context<Self>) {
        let (input_text, current_link) = {
            let Some(panel) = self.doc_state().and_then(|s| s.template_panel.as_ref()) else { return };
            let val = panel.path_input.read(cx).text().to_string();
            (val, self.docx_link_styles())
        };
        let cleaned = input_text.trim();
        let tmpl = if cleaned.is_empty() { None } else { Some(cleaned.to_string()) };
        self.set_docx_attached_template(tmpl, current_link, cx);
        if let Some(panel) = self.doc_state_mut().and_then(|s| s.template_panel.as_mut()) {
            panel.status_message = Some(text("docx.template.attached").to_string());
            panel.error_message = None;
        }
        cx.notify();
    }

    /// Update document styles from an external template file (.dotx, .dotm, .docx).
    pub(crate) fn update_docx_styles_from_template_file(&mut self, path: &Path, cx: &mut Context<Self>) -> Result<usize, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let presentation = ibis_pptx::read(&bytes).map_err(|e| crate::shell::i18n::format("docx.template.parse-error", &[("error", &e)]))?;
        let foreign_flow = presentation.flow.ok_or_else(|| crate::shell::i18n::text("docx.template.no-word-data").to_string())?;

        let current_flow = self.flow().ok_or_else(|| crate::shell::i18n::text("docx.template.no-document").to_string())?;
        let mut new_styles = (*current_flow.styles).clone();

        let count = foreign_flow.styles.styles.len();
        for (id, style) in &foreign_flow.styles.styles {
            new_styles.styles.insert(id.clone(), style.clone());
        }

        // Apply 1-step undoable command
        self.run_flow(vec![FlowCommand::UpdateStyles(Arc::new(new_styles))], cx);
        Ok(count)
    }

    /// Update styles now button action.
    pub(crate) fn docx_update_styles_now(&mut self, cx: &mut Context<Self>) {
        let path_str = self.docx_attached_template().map(str::to_string).or_else(|| {
            self.doc_state().and_then(|s| s.template_panel.as_ref()).map(|p| p.path_input.read(cx).text().to_string())
        });

        let Some(path_str) = path_str.filter(|s| !s.trim().is_empty()) else {
            if let Some(panel) = self.doc_state_mut().and_then(|s| s.template_panel.as_mut()) {
                panel.error_message = Some(text("docx.template.no_path").to_string());
                panel.status_message = None;
            }
            cx.notify();
            return;
        };

        let path = PathBuf::from(path_str.trim());
        match self.update_docx_styles_from_template_file(&path, cx) {
            Ok(count) => {
                if let Some(panel) = self.doc_state_mut().and_then(|s| s.template_panel.as_mut()) {
                    panel.status_message = Some(crate::shell::i18n::format("docx.template.updated-count", &[("title", &text("docx.template.updated")), ("n", &count)]));
                    panel.error_message = None;
                }
            }
            Err(err) => {
                if let Some(panel) = self.doc_state_mut().and_then(|s| s.template_panel.as_mut()) {
                    panel.error_message = Some(err);
                    panel.status_message = None;
                }
            }
        }
        cx.notify();
    }

    /// Render the template attachment panel in the sidebar.
    pub(crate) fn doc_template_panel(&self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel_state = self.doc_state()?.template_panel.as_ref()?;
        let path_input = panel_state.path_input.clone();
        let status = panel_state.status_message.clone();
        let error = panel_state.error_message.clone();

        let current_attached = self.docx_attached_template().map(str::to_string);
        let link_styles = self.docx_link_styles();

        let body = v_flex()
            .gap_3()
            .p_3()
            .text_sm()
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(text("docx.template.desc")),
            )
            // Current status banner
            .child(
                v_flex()
                    .p_2()
                    .rounded_md()
                    .bg(cx.theme().muted.opacity(0.5))
                    .gap_1()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(text("docx.template.current_status")),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground)
                            .child(match &current_attached {
                                Some(p) => format!("{}: {}", text("docx.template.attached_file"), p),
                                None => text("docx.template.normal_template").to_string(),
                            }),
                    ),
            )
            // File path input section
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(text("docx.template.path_label")),
                    )
                    .child(crate::shell::chrome::text_field(&path_input).full_width().path_picker("template-browse", crate::shell::chrome::PathMode::File, &["dotx","dotm","docx","docm"],cx,|_,_,_,_|{}))
                    .child(
                        crate::shell::chrome::action_bar()
                            .action(
                                crate::shell::chrome::ActionDefinition::new("template-apply-btn", text("docx.template.apply"), crate::shell::chrome::ActionButtonKind::Primary)
                                    .on_click(cx.listener(|editor, _, _, cx| {
                                        editor.apply_docx_template_from_panel(cx);
                                    })),
                            )
                            .action(
                                crate::shell::chrome::ActionDefinition::new("template-detach-btn", text("docx.template.detach"), crate::shell::chrome::ActionButtonKind::Destructive)
                                    .on_click(cx.listener(|editor, _, _, cx| {
                                        editor.detach_docx_template(cx);
                                    })),
                            ),
                    ),
            )
            // Options: Automatically update document styles
            .child(
                v_flex()
                    .gap_2()
                    .pt_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        crate::shell::chrome::action_button("template-toggle-link-styles")
                            .small()
                            .icon(if link_styles {
                                Icon::new(Material("check_box"))
                            } else {
                                Icon::new(Material("check_box_outline_blank"))
                            })
                            .label(text("docx.template.link_styles"))
                            .on_click(cx.listener(|editor, _, _, cx| {
                                editor.toggle_docx_link_styles(cx);
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(text("docx.template.link_styles_desc")),
                    ),
            )
            // Actions: Update styles now
            .child(
                v_flex()
                    .gap_2()
                    .pt_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        crate::shell::chrome::action_button("template-update-now-btn")
                            .label(text("docx.template.update_now"))
                            .icon(Icon::new(Material("sync")))
                            .primary()
                            .on_click(cx.listener(|editor, _, _, cx| {
                                editor.docx_update_styles_now(cx);
                            })),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(text("docx.template.update_now_desc")),
                    ),
            )
            // Status & Error messages
            .when_some(status, |this, msg| {
                this.child(
                    div()
                        .p_2()
                        .rounded_md()
                        .bg(cx.theme().success.opacity(0.15))
                        .text_color(cx.theme().success)
                        .text_sm()
                        .child(msg),
                )
            })
            .when_some(error, |this, err| {
                this.child(
                    div()
                        .p_2()
                        .rounded_md()
                        .bg(cx.theme().danger.opacity(0.15))
                        .text_color(cx.theme().danger)
                        .text_sm()
                        .child(err),
                )
            })

            .into_any_element();

        let panel_terminal_actions = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::dialog_close_definition(cx));
        Some(SidebarPanel::with_footer(text("docx.template.title"), body, panel_terminal_actions))
    }
}
