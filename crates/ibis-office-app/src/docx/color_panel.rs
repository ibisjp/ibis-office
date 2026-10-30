// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Arbitrary RGB for text and underline, using existing text edit commands.
use gpui_kit::*;
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputEvent,InputState};
use gpui_kit::component::{Sizable as _,h_flex,v_flex};
use ibis_doc_model::{FlowPos,Rgb};
use crate::shell::{chrome::SidebarPanel,editor::SlideEditor};
pub(crate) struct ColorEditState {
    pub(crate) field:Entity<InputState>,
    pub(crate) picker: crate::shell::color_editor::ColorEditorState,
    target:(FlowPos,FlowPos),
    underline:bool,
    applied:Option<(String,std::sync::Arc<ibis_doc_model::FlowDocument>)>,
    pub(crate) error:Option<String>,
    _subscription:Subscription,
}
impl SlideEditor {
    pub(crate) fn open_color_edit(&mut self,underline:bool,window:&mut Window,cx:&mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(target)=self.ordered_selection() else{return};
        let Some(style)=self.current_format().map(|(_,style)|style) else{return};
        let color=if underline {style.underline_color.unwrap_or(style.color)} else {style.color};
        let initial=format!("{:06X}",color.0);
        let field=cx.new(|cx| {let mut input=InputState::new(window,cx);input.set_value(initial.clone(),window,cx);input});
        let picker=crate::shell::color_editor::ColorEditorState::new(color,window,cx,Self::edit_color_picker);
        let subscription=cx.subscribe_in(&field,window,|this,_,event:&InputEvent,window,cx| {
            if matches!(event,InputEvent::Change) {this.edit_color_picker(crate::shell::color_editor::ColorAction::Hex,window,cx);}
            if let InputEvent::PressEnter {..}=event {this.apply_color_edit(window,cx);}
        });
        if let Some(state)=self.doc_state_mut() {
            state.color_edit=Some(ColorEditState {field,picker,target,underline,applied:None,error:None,_subscription:subscription});
            state.font_edit=None; state.paragraph_edit=None;state.picture_edit=None;state.table_size=None;state.style_edit=None;state.link=None;state.find=None;state.comments=None;
        }
        self.sidebar_open=true;cx.notify();
    }
    pub(crate) fn apply_color_edit(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        let Some(edit)=self.doc_state().and_then(|s|s.color_edit.as_ref()) else{return};
        let value=edit.field.read(cx).value().trim().strip_prefix('#').unwrap_or(edit.field.read(cx).value().trim()).to_string();
        let target=edit.target;let underline=edit.underline;
        let color=(value.len()==6 && value.bytes().all(|b|b.is_ascii_hexdigit())).then(||u32::from_str_radix(&value,16).ok()).flatten();
        if color.is_none() || (0..6).any(|i| edit.picker.number_value(i,cx).is_none()) || self.ordered_selection()!=Some(target) {
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.color_edit.as_mut()) {edit.error=Some(crate::shell::i18n::text("docx.color-panel.check-6-digit-rgb-value").into());}
            cx.notify();return;
        }
        let normalized=format!("{:06X}",color.unwrap());
        if !edit.applied.as_ref().is_some_and(|(value,doc)| value==&normalized && self.flow()==Some(doc)) {
            if underline {self.set_underline_color(Some(Rgb(color.unwrap())),cx);} else {self.set_text_color(Rgb(color.unwrap()),cx);}
        }
        self.note_recent_color(Rgb(color.unwrap()));
        let applied=self.flow().cloned().map(|doc|(normalized,doc));
        if let Some(edit)=self.doc_state_mut().and_then(|s|s.color_edit.as_mut()) {edit.applied=applied;edit.error=None;}
        if !self.panel_dialog_is_modal() { self.focus_handle.focus(window,cx); }
        cx.notify();
    }
    pub(crate) fn edit_color_picker(&mut self, action: crate::shell::color_editor::ColorAction, window: &mut Window, cx: &mut Context<Self>) {
        use crate::shell::color_editor::{ColorAction, hsv_to_rgb, rgb_to_hsv};
        let Some(edit) = self.doc_state_mut().and_then(|s| s.color_edit.as_mut()) else { return; };
        if edit.picker.change(action,&edit.field,window,cx){edit.error=None;cx.notify();}
    }

    pub(crate) fn color_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let edit = self.doc_state()?.color_edit.as_ref()?;
        let body = v_flex().gap_2().px(px(18.)).py(px(8.))
            .p_0().child(crate::shell::color_editor::color_editor(&edit.picker, &edit.field, &self.recent_colors, Self::edit_color_picker, cx))
            .children(edit.error.clone().map(|error| div().text_sm().child(error)));
        let footer = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::ActionDefinition::new("docx-color-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary)
                .on_click(cx.listener(|this, _, window, cx| this.apply_color_edit(window, cx))))
            .action(crate::shell::chrome::ActionDefinition::new("docx-color-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Some(state) = this.doc_state_mut() { state.color_edit = None; }
                    this.sidebar_open = false;
                    this.focus_handle.focus(window, cx);
                    cx.notify();
                })));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text(if edit.underline { "docx.color-panel.underline-color" } else { "docx.color-panel.text-color" }), body.into_any_element(), footer))
    }
}
