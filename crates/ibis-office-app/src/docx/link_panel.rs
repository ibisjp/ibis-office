// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! リンク (X1): the sidebar panel that makes the selection a link to an
//! address, or removes the link (⌘K, 挿入 > リンク, the toolbar's button).

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use ibis_doc_model::{FlowCommand, Link};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The panel's address field.
pub(crate) struct LinkState {
    pub(crate) address: Entity<InputState>,
    pub(crate) target: (ibis_doc_model::FlowPos, ibis_doc_model::FlowPos),
    _subscription: Subscription,
}

impl SlideEditor {
    /// The link of the selection's first character.
    pub(crate) fn link_at_selection(&self) -> Option<Link> {
        let (start,end)=self.ordered_selection()?;
        self.doc_link_range(start).or_else(||(start==end).then(||start.offset.checked_sub(1).and_then(|offset|self.doc_link_range(ibis_doc_model::FlowPos::new(start.para,offset)))).flatten()).map(|(_,_,l)|l)
    }

    /// The complete linked span, including adjacent runs with the same target.
    pub(crate) fn doc_link_range(&self, pos: ibis_doc_model::FlowPos) -> Option<(ibis_doc_model::FlowPos,ibis_doc_model::FlowPos,Link)> {
        let p=ibis_doc_model::flow_command::paragraph(self.flow()?,pos.para)?;
        let mut at=0;let mut spans=Vec::new();
        for r in &p.runs {spans.push((at,at+r.len(),r.link.clone()));at+=r.len();}
        let index=spans.iter().position(|(a,b,l)|*a<=pos.offset&&pos.offset<*b&&l.is_some())?;
        let link=spans[index].2.clone()?;let mut left=index;let mut right=index;
        while left>0&&spans[left-1].2.as_ref()==Some(&link){left-=1;}
        while right+1<spans.len()&&spans[right+1].2.as_ref()==Some(&link){right+=1;}
        Some((ibis_doc_model::FlowPos::new(pos.para,spans[left].0),ibis_doc_model::FlowPos::new(pos.para,spans[right].1),link))
    }

    /// Hit-test actual laid-out character intervals, including the last character and vertical text.
    pub(crate) fn doc_link_position_at_point(&self,page:usize,x:f32,y:f32)->Option<ibis_doc_model::FlowPos>{
        let lines=self.flow_lines();
        for line in lines.iter().filter(|l|l.page==page){
            let axis=if line.vertical {if x<line.x||x>=line.x+line.h{continue;}y-line.y}
                else {if y<line.y||y>=line.y+line.h{continue;}x-line.x};
            for pair in line.stops.windows(2){
                let (a,ax)=pair[0];let (b,bx)=pair[1];
                if axis>=ax.min(bx)&&axis<ax.max(bx){let pos=ibis_doc_model::FlowPos::new(line.para,a.min(b));if self.doc_link_range(pos).is_some(){return Some(pos);}}
            }
        }None
    }

    /// Make the selection a link to `address`, or no
    /// link (an empty address). When adding a link, apply the Word
    /// `Hyperlink` character style; when removing it, clear the style.
    pub(crate) fn set_link(&mut self, address: &str, cx: &mut Context<Self>) {
        self.apply_link_to_selection(address,cx);
    }
    fn apply_link_to_selection(&mut self,address:&str,cx:&mut Context<Self>)->bool {
        let Some((mut start, mut end)) = self.ordered_selection() else { return false };
        let address = address.trim();
        let mut insertion=None;
        if start==end {
            let at=self.doc_link_range(start).or_else(||start.offset.checked_sub(1).and_then(|offset|self.doc_link_range(ibis_doc_model::FlowPos::new(start.para,offset))));
            if let Some((a,b,_))=at {start=a;end=b;}
            else if !address.is_empty(){insertion=Some(FlowCommand::InsertText{at:start,text:address.to_string(),format:None});end.offset+=address.chars().count();}
            else {return false;}
        }
        let link = if address.is_empty() {
            None
        } else if let Some(bookmark) = address.strip_prefix('#') {
            Some(Link::Anchor(bookmark.to_string()))
        } else {
            Some(Link::Url(address.to_string()))
        };
        let keep = self.text_selection();
        let Some(doc) = self.flow() else { return false };
        let inserting=insertion.is_some();
        let mut commands = insertion.into_iter().collect::<Vec<_>>();
        match link {
            Some(link) => {
                // Word applies the Hyperlink character style to newly linked text.
                let style_id = doc
                    .styles
                    .styles
                    .iter()
                    .find(|(_, s)| s.kind == ibis_doc_model::StyleKind::Character && s.name.eq_ignore_ascii_case("hyperlink"))
                    .map(|(id, _)| id.clone())
                    .or_else(|| doc.styles.styles.get("Hyperlink").map(|_| "Hyperlink".to_string()))
                    .unwrap_or_else(|| {
                        let id = "Hyperlink".to_string();
                        let style = ibis_doc_model::FlowStyle {
                            kind: ibis_doc_model::StyleKind::Character,
                            name: "Hyperlink".to_string(),
                            based_on: Some("DefaultParagraphFont".to_string()),
                            run: ibis_doc_model::RunFormat {
                                color: Some(ibis_doc_model::Rgb(0x0563C1)),
                                underline: Some(true),
                                underline_style: Some(ibis_doc_model::UnderlineStyle::Single),
                                ..Default::default()
                            },
                            ..Default::default()
                        };
                        commands.push(FlowCommand::SetStyle { id: id.clone(), style: Some(style) });
                        id
                    });
                commands.push(FlowCommand::SetLink { from: start, to: end, link: Some(link) });
                commands.push(FlowCommand::SetCharStyle { from: start, to: end, style: Some(style_id) });
            }
            None => {
                commands.push(FlowCommand::SetLink { from: start, to: end, link: None });
                // If the selection had a Hyperlink character style, remove it.
                if let Some(p) = ibis_doc_model::flow_command::paragraph(doc, start.para) {
                    let mut at = 0;
                    let mut has_hyperlink = false;
                    for run in &p.runs {
                        let len = run.len();
                        let run_end = at + len;
                        if start.offset < run_end && at < end.offset
                            && run.char_style.as_deref().is_some_and(|s| s.eq_ignore_ascii_case("hyperlink") || doc.styles.styles.get(s).is_some_and(|st| st.name.eq_ignore_ascii_case("hyperlink")))
                        {
                            has_hyperlink = true;
                            break;
                        }
                        at = run_end;
                    }
                    if has_hyperlink {
                        commands.push(FlowCommand::SetCharStyle { from: start, to: end, style: None });
                    }
                }
            }
        }
        // Model text commands address one paragraph. Expand a multi-paragraph
        // selection into one atomic command batch, preserving the selection.
        if start.para!=end.para {
            let order=super::editing::order(doc);
            let (Some(a),Some(b))=(order.iter().position(|p|*p==start.para),order.iter().position(|p|*p==end.para))else{return false;};
            let ranges=order[a..=b].iter().filter_map(|id|{
                let p=ibis_doc_model::flow_command::paragraph(doc,*id)?;
                let from=ibis_doc_model::FlowPos::new(*id,if *id==start.para{start.offset}else{0});
                let to=ibis_doc_model::FlowPos::new(*id,if *id==end.para{end.offset}else{p.len()});
                (from!=to).then_some((from,to))
            }).collect::<Vec<_>>();
            commands=commands.into_iter().flat_map(|c|match c{
                FlowCommand::SetLink{link,..}=>ranges.iter().map(|(from,to)|FlowCommand::SetLink{from:*from,to:*to,link:link.clone()}).collect(),
                FlowCommand::SetCharStyle{style,..}=>ranges.iter().map(|(from,to)|FlowCommand::SetCharStyle{from:*from,to:*to,style:style.clone()}).collect(),
                other=>vec![other],
            }).collect();
        }
        if !self.run_flow(commands, cx){return false;}
        if inserting {self.set_caret(end,false,cx);return true;}
        if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
            state.anchor = Some(a);
            state.caret = Some(c);
        }
        true
    }

    /// ⌘K: the panel, with the selection's link's address.
    pub(crate) fn open_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(mut target)=self.ordered_selection() else{return;};
        if target.0==target.1 {
            if let Some((a,b,_))=self.doc_link_range(target.0).or_else(||target.0.offset.checked_sub(1).and_then(|offset|self.doc_link_range(ibis_doc_model::FlowPos::new(target.0.para,offset)))) {target=(a,b);}
        }
        let current = match self.link_at_selection() {
            Some(Link::Url(u)) => u,
            Some(Link::Anchor(a)) => format!("#{a}"),
            None => String::new(),
        };
        if self.doc_state().is_some_and(|s| s.link.is_none()) {
            let address = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.link-panel.https-start-bookmark")));
            let subscription = cx.subscribe_in(&address, window, |this, input, event: &InputEvent, _window, cx| {
                let _=input;cx.notify();
                if let InputEvent::PressEnter { .. } = event {this.apply_doc_link(_window,cx);}
            });
            if let Some(state) = self.doc_state_mut() {
                state.link = Some(LinkState { address, target, _subscription: subscription });
                state.find = None;
            }
        }
        if let Some(p)=self.doc_state_mut().and_then(|s|s.link.as_mut()){p.target=target;}
        if let Some(address) = self.doc_state().and_then(|s| s.link.as_ref()).map(|l| l.address.clone()) {
            address.update(cx, |a, cx| {
                a.set_value(current, window, cx);
                a.focus(window, cx);
            });
        }
        if let Some(state) = self.doc_state_mut() {
            state.find = None;
            state.style_edit = None;
            state.font_edit=None; state.color_edit=None; state.paragraph_edit = None; state.picture_edit = None; state.table_size = None;
            state.comments = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn apply_doc_link(&mut self,window:&mut Window,cx:&mut Context<Self>){
        let Some(p)=self.doc_state().and_then(|s|s.link.as_ref()) else{return;};
        let address=p.address.read(cx).value().trim().to_string();let target=p.target;
        if address.is_empty(){return;}
        if let Some(s)=self.doc_state_mut(){s.anchor=Some(target.0);s.caret=Some(target.1);}
        if self.apply_link_to_selection(&address,cx){self.close_panel_dialog(window,cx);}
    }

    pub(crate) fn link_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let address = self.doc_state()?.link.as_ref()?.address.clone();
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(crate::shell::chrome::text_field(&address).full_width())
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("link-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).disabled(address.read(cx).value().trim().is_empty()).on_click(cx.listener(|this,_,w,cx|this.apply_doc_link(w,cx))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx))
                    .when(self.link_at_selection().is_some(),|bar|bar.action(crate::shell::chrome::ActionDefinition::new("link-remove", crate::shell::i18n::text("_shell.remove-link"), crate::shell::chrome::ActionButtonKind::Destructive).on_click(cx.listener(|this, _, w, cx| {this.set_link("", cx);this.close_panel_dialog(w,cx);})))) ;
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("_shell.link"), body.into_any_element(), panel_actions))
    }
}

impl SlideEditor {
    pub(crate) fn doc_hyperlink_menu(&self)->Vec<crate::shell::chrome::MenuEntry>{
        use crate::shell::chrome::{MenuItem,Submenu};
        if self.link_at_selection().is_none(){return Vec::new();}
        vec![Submenu::tr("hyperlink.menu","link",vec![
            MenuItem::tr("hyperlink.open").on(|e,_,cx|{if let Some((a,_))=e.ordered_selection(){let p=e.doc_link_range(a).map(|(a,_,_)|a).unwrap_or(a);e.follow_link(p,cx);}}).into(),
            MenuItem::tr("hyperlink.edit").on(|e,w,cx|e.open_link(w,cx)).into(),
            MenuItem::tr("hyperlink.remove").on(|e,_,cx|e.set_link("",cx)).into(),
        ]).into()]
    }
}
