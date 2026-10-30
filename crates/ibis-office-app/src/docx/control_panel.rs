// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! コンテンツ コントロールのプロパティ (DOCX-FORM-010/012/013): the sidebar panel
//! that sets the title and tag of the content control at the caret, and
//! whether it can be deleted and its content edited (`w:alias`, `w:tag`,
//! `w:lock`).

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{ControlInfo, ControlLock, FlowCommand, FlowPos};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The panel's fields and the control it edits (its paragraph and first position).
pub(crate) struct ControlState {
    pub(crate) para: u32,
    pub(crate) at: usize,
    pub(crate) title: Entity<InputState>,
    pub(crate) tag: Entity<InputState>,
    pub(crate) lock: ControlLock,
    pub(crate) gallery_fields: Option<(Entity<InputState>, Entity<InputState>)>,
    pub(crate) repeating_fields: Option<(Entity<InputState>, bool)>,
}

impl SlideEditor {
    /// The content control the caret is in (or at the end of): its
    /// paragraph, first position, what it says about itself, optional gallery meta, and optional repeating section meta.
    pub(crate) fn control_at_caret(&self) -> Option<(u32, usize, ControlInfo, Option<(String, String)>, Option<(String, bool)>)> {
        let (start, _) = self.ordered_selection()?;
        let p = ibis_doc_model::flow_command::paragraph(self.flow()?, start.para)?;
        let mut at = 0;
        for run in &p.runs {
            let len = run.len();
            if run.is_content_control() && start.offset >= at && start.offset <= at + len {
                let gallery_meta = match &run.kind {
                    ibis_doc_model::FlowRunKind::BuildingBlockGallery { gallery, category, .. } => {
                        Some((gallery.clone(), category.clone()))
                    }
                    _ => None,
                };
                let repeating_meta = match &run.kind {
                    ibis_doc_model::FlowRunKind::RepeatingSection { section_title, do_not_allow_insert_delete, .. } => {
                        Some((section_title.clone(), *do_not_allow_insert_delete))
                    }
                    _ => None,
                };
                return Some((start.para, at, run.control.as_deref().cloned().unwrap_or_default(), gallery_meta, repeating_meta));
            }
            at += len;
        }
        None
    }

    /// 表示形式 > コントロールのプロパティ…: the panel, with the control's.
    pub(crate) fn open_control_properties(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some((para, at, info, gallery_meta, repeating_meta)) = self.control_at_caret() else {
            self.status = Some(crate::shell::i18n::text("docx.control-panel.select-control"));
            cx.notify();
            return;
        };
        let title = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.control-panel.title")));
        let tag = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.control-panel.tag")));
        title.update(cx, |a, cx| a.set_value(info.title.clone(), window, cx));
        tag.update(cx, |a, cx| a.set_value(info.tag.clone(), window, cx));

        let gallery_fields = gallery_meta.map(|(g, c)| {
            let gallery_in = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.control-panel.gallery")));
            let category_in = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.control-panel.category")));
            gallery_in.update(cx, |a, cx| a.set_value(g, window, cx));
            category_in.update(cx, |a, cx| a.set_value(c, window, cx));
            (gallery_in, category_in)
        });

        let repeating_fields = repeating_meta.map(|(st, no_edit)| {
            let section_title_in = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.control-panel.section-title")));
            section_title_in.update(cx, |a, cx| a.set_value(st, window, cx));
            (section_title_in, no_edit)
        });

        if let Some(state) = self.doc_state_mut() {
            state.find = None;
            state.link = None;
            state.ruby = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.text_box = None;
            state.merge = None;
            state.table_size = None;
            state.comments = None;
            state.control = Some(ControlState { para, at, title, tag, lock: info.lock, gallery_fields, repeating_fields });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Set what the panel says on its control and close it.
    pub(crate) fn apply_control_properties(&mut self, cx: &mut Context<Self>) {
        let Some((para, at, title, tag, lock, gallery_meta, repeating_meta)) = self.doc_state().and_then(|s| s.control.as_ref()).map(|c| {
            let g = c.gallery_fields.as_ref().map(|(g, cat)| (g.read(cx).value().trim().to_string(), cat.read(cx).value().trim().to_string()));
            let r = c.repeating_fields.as_ref().map(|(st, no_edit)| (st.read(cx).value().trim().to_string(), *no_edit));
            (c.para, c.at, c.title.read(cx).value().trim().to_string(), c.tag.read(cx).value().trim().to_string(), c.lock, g, r)
        }) else { return };
        let info = ControlInfo { title, tag, lock };
        let info = if info == ControlInfo::default() { None } else { Some(Box::new(info)) };
        let mut cmds = vec![FlowCommand::SetControlInfo { at: FlowPos::new(para, at), info }];
        if let Some((gallery, category)) = gallery_meta {
            let gallery = if gallery.is_empty() { ibis_doc_model::DEFAULT_BUILDING_BLOCK_GALLERY_NAME.to_string() } else { gallery };
            let category = if category.is_empty() { ibis_doc_model::DEFAULT_BUILDING_BLOCK_GALLERY_CATEGORY.to_string() } else { category };
            cmds.push(FlowCommand::SetBuildingBlockGalleryMeta { at: FlowPos::new(para, at), gallery, category });
        }
        if let Some((section_title, do_not_allow_insert_delete)) = repeating_meta {
            let section_title = if section_title.is_empty() { ibis_doc_model::DEFAULT_REPEATING_SECTION_TITLE.to_string() } else { section_title };
            cmds.push(FlowCommand::SetRepeatingSectionMeta { at: FlowPos::new(para, at), section_title, do_not_allow_insert_delete });
        }
        self.run_flow(cmds, cx);
        if let Some(state) = self.doc_state_mut() {
            state.control = None;
        }
        cx.notify();
    }

    pub(crate) fn control_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.doc_state()?.control.as_ref()?;
        let (title, tag, lock, gallery_fields, repeating_fields) = (state.title.clone(), state.tag.clone(), state.lock, state.gallery_fields.clone(), state.repeating_fields.clone());
        let text = crate::shell::i18n::text;
        let choice = |id: &'static str, label: &'static str, value: ControlLock, cx: &mut Context<Self>| {
            crate::shell::chrome::radio_button(id, lock == value, false, text(label), cx.listener(move |this, _, _, cx| {
                if let Some(state) = this.doc_state_mut().and_then(|s| s.control.as_mut()) {
                    state.lock = value;
                }
                cx.notify();
            }))
        };
        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(div().text_sm().child(text("docx.control-panel.title")))
            .child(crate::shell::chrome::text_field(&title).full_width())
            .child(div().text_sm().child(text("docx.control-panel.tag")))
            .child(crate::shell::chrome::text_field(&tag).full_width());

        if let Some((gallery_in, category_in)) = gallery_fields {
            body = body
                .child(div().text_sm().child(text("docx.control-panel.gallery")))
                .child(crate::shell::chrome::text_field(&gallery_in).full_width())
                .child(div().text_sm().child(text("docx.control-panel.category")))
                .child(crate::shell::chrome::text_field(&category_in).full_width());
        }

        if let Some((section_title_in, no_edit)) = repeating_fields {
            body = body
                .child(div().text_sm().child(text("docx.control-panel.section-title")))
                .child(crate::shell::chrome::text_field(&section_title_in).full_width())
                .child(crate::shell::chrome::checkbox_button("control-repeating-no-insert-delete", text("docx.control-panel.no-insert-delete"), no_edit, false, cx.listener(move |this, _, _, cx| {
                    if let Some(state) = this.doc_state_mut().and_then(|s| s.control.as_mut()) {
                        if let Some((_, ref mut flag)) = state.repeating_fields {
                            *flag = !*flag;
                        }
                    }
                    cx.notify();
                })));
        }

        body = body
            .child(div().text_sm().child(text("docx.control-panel.lock")))
            .child(choice("control-lock-none", "docx.control-panel.lock-none", ControlLock::None, cx))
            .child(choice("control-lock-control", "docx.control-panel.lock-control", ControlLock::Control, cx))
            .child(choice("control-lock-content", "docx.control-panel.lock-content", ControlLock::Content, cx))
            .child(choice("control-lock-both", "docx.control-panel.lock-both", ControlLock::Both, cx))
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("control-apply", text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|this, _, _, cx| this.apply_control_properties(cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("control-cancel", text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|this, _, _, cx| {
                        if let Some(state) = this.doc_state_mut() {
                            state.control = None;
                        }
                        cx.notify();
                    })));
        Some(SidebarPanel::with_footer(text("docx.control-panel.panel-title"), body.into_any_element(), panel_actions))
    }
}
