// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Independent Latin, East Asian, and Complex Script fonts, using the toolbar's searchable font list.
use gpui_kit::*;
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputEvent,InputState};
use gpui_kit::component::{Sizable as _,h_flex,v_flex};
use ibis_doc_model::{FlowPos,RunFormat,FlowDocument,Block,RevisionKind};
use ibis_doc_model::flow_command;
use std::sync::Arc;
use crate::shell::{chrome::SidebarPanel,editor::SlideEditor};
pub(crate) struct FontEditState {
    pub(crate) fields:[Entity<InputState>;3],
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<&'static str, Bounds<Pixels>>>>,
    target:(FlowPos,FlowPos),
    initial:[String;3],
    document:Arc<FlowDocument>,
    pub(crate) error:Option<String>,
    _subscriptions:Vec<Subscription>,
}
impl SlideEditor {
    fn selected_font_names(&self)->Option<[String;3]> {
        let (start,end)=self.ordered_selection()?;
        if start==end {
            let (_,style)=self.current_format()?;
            return Some([style.font.unwrap_or_default(),style.east_asian_font.unwrap_or_default(),style.cs_font.unwrap_or_default()]);
        }
        let doc=self.flow()?;let (container,first)=flow_command::locate(doc,start.para)?;
        let (other,last)=flow_command::locate(doc,end.para)?;
        if container!=other || first>last {return None;}
        let blocks=flow_command::blocks(doc,container)?;
        let mut fonts:Option<[Option<String>;3]>=None;
        for (index,block) in blocks[first..=last].iter().enumerate() {
            let selected=match block {
                Block::Paragraph(p)=>vec![(p,if index==0 {start.offset} else {0},if first+index==last {end.offset} else {p.len()})],
                Block::Table(_)=>super::block_tree::paragraphs(std::slice::from_ref(block)).into_iter().map(|p|(p,0,p.len())).collect(),
            };
            for (p,a,b) in selected {
            let mut offset=0;
            for run in &p.runs {
                let next=offset+run.len();
                let hidden=!super::flow_layout::showing_markup() && matches!(run.revision.as_ref().map(|r|&r.kind),Some(RevisionKind::Delete));
                if offset<b && next>a && !hidden {
                    let names=[run.style.font.clone(),run.style.east_asian_font.clone(),run.style.cs_font.clone()];
                    if let Some(common)=fonts.as_mut() {for i in 0..3 {if common[i]!=names[i] {common[i]=None;}}} else {fonts=Some(names);}
                }
                offset=next;
            }
            }
        }
        fonts.map(|values|values.map(Option::unwrap_or_default))
    }
    pub(crate) fn open_font_edit(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(target)=self.ordered_selection() else{return};let Some(document)=self.flow().cloned() else{return};
        let Some(initial)=self.selected_font_names() else{return};
        let fields=initial.clone().map(|value|cx.new(|cx| {let mut input=InputState::new(window,cx);input.set_value(value,window,cx);input}));
        let subscriptions=fields.iter().map(|field|cx.subscribe_in(field,window,|this,_,event:&InputEvent,window,cx| {
            if let InputEvent::PressEnter {..}=event {this.apply_font_edit(window,cx);}
        })).collect();
        if let Some(state)=self.doc_state_mut() {
            state.font_edit=Some(FontEditState {fields,ui:Default::default(),target,initial,document,error:None,_subscriptions:subscriptions});
            state.color_edit=None;state.paragraph_edit=None;state.picture_edit=None;state.table_size=None;state.style_edit=None;state.link=None;state.find=None;state.comments=None;
        }
        self.sidebar_open=true;cx.notify();
    }
    pub(crate) fn apply_font_edit(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        let Some(edit)=self.doc_state().and_then(|s|s.font_edit.as_ref()) else{return};
        let values=edit.fields.clone().map(|field|field.read(cx).value().trim().to_string());
        let changed=[values[0]!=edit.initial[0],values[1]!=edit.initial[1],values[2]!=edit.initial[2]];
        let invalid=values.iter().enumerate().any(|(i,value)|changed[i] && (value.is_empty() || value.chars().count()>128 || value.chars().any(char::is_control)));
        if invalid || self.ordered_selection()!=Some(edit.target) || self.flow()!=Some(&edit.document) {
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.font_edit.as_mut()) {edit.error=Some(crate::shell::i18n::text("docx.font-panel.check-font-name-selection-if").into());}
            cx.notify();return;
        }
        let properties=RunFormat {font:changed[0].then(||values[0].clone()),east_asian_font:changed[1].then(||values[1].clone()),cs_font:changed[2].then(||values[2].clone()),..Default::default()};
        if !changed.into_iter().any(|v|v) || self.apply_text_properties(properties,cx) {
            let document=self.flow().unwrap().clone();
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.font_edit.as_mut()) {edit.initial=values;edit.document=document;edit.error=None;}
            if !self.panel_dialog_is_modal() { self.focus_handle.focus(window,cx); }
        }
        cx.notify();
    }
    /// Keyboard activation of the same recent/system choices used by the font dialog popovers.
    pub(crate) fn doc_font_menu_entries(&self, id: &str) -> Option<Vec<crate::shell::chrome::MenuEntry>> {
        let (index, menu_id) = match id {
            "docx-font-latin-list" => (0, "docx-font-latin-list"),
            "docx-font-east-asian-list" => (1, "docx-font-east-asian-list"),
            "docx-font-complex-script-list" => (2, "docx-font-complex-script-list"),
            _ => return None,
        };
        let input = self.doc_state()?.font_edit.as_ref()?.fields[index].clone();
        Some(crate::shell::font_list::build_font_menu_entries(menu_id, Vec::new(), None,
            move |name, _, window, cx| {
                input.update(cx, |state, cx| state.set_value(name.to_string(), window, cx));
                cx.notify();
            }))
    }

    pub(crate) fn font_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let edit = self.doc_state()?.font_edit.as_ref()?;
        let weak = cx.entity().downgrade();
        let row = |index: usize, label: &'static str, menu_id: &'static str| {
            let field = edit.fields[index].clone();
            let current = field.read(cx).value().to_string();
            let input = field.clone();
            let entries = crate::shell::font_list::build_font_menu_entries(
                menu_id, Vec::new(), (!current.is_empty()).then_some(current.as_str()),
                move |name, _, window, cx| {
                    input.update(cx, |state, cx| state.set_value(name.to_string(), window, cx));
                    cx.notify();
                },
            );
            v_flex().gap_1()
                .child(div().text_sm().child(crate::shell::i18n::text(label)))
                .child(div().relative().w(crate::shell::chrome::FONT_DROPDOWN_WIDTH).flex_none()
                    .child(crate::shell::chrome::font_dropdown(menu_id, &current, entries, index, &weak))
                    .child(canvas(|_, _, _| {}, {
                        let ui = edit.ui.clone();
                        move |bounds, _, _, _| { ui.borrow_mut().insert(menu_id, bounds); }
                    }).absolute().inset_0()))
        };
        let body = v_flex().gap_2().px(px(18.)).py(px(8.))
            .p_0().child(div().text_sm().child(crate::shell::i18n::text("docx.font-panel.blank-means-not-specified-mixed")))
            .child(row(0, "docx.font-panel.latin-font", "docx-font-latin-list"))
            .child(row(1, "docx.font-panel.east-asian-font", "docx-font-east-asian-list"))
            .child(row(2, "docx.font-panel.complex-script-font", "docx-font-complex-script-list"))
            .children(edit.error.clone().map(|error| div().text_sm().child(error)));
        let footer = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::ActionDefinition::new("docx-font-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary)
                .on_click(cx.listener(|this, _, window, cx| this.apply_font_edit(window, cx))))
            .action(crate::shell::chrome::ActionDefinition::new("docx-font-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Some(state) = this.doc_state_mut() { state.font_edit = None; }
                    this.sidebar_open = false;
                    this.focus_handle.focus(window, cx);
                    cx.notify();
                })));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("_shell.font"), body.into_any_element(), footer))
    }
}
