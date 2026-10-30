// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Named sections: capture slide structure before collecting a name.
use gpui_kit::*;
use crate::shell::palette::rgb;
use gpui_kit::component::{Sizable as _,h_flex,v_flex};
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use ibis_doc_model::{Command,Section,SlideId};
use crate::shell::{chrome::SidebarPanel,editor::SlideEditor};
pub(crate) struct SectionPanel {
    pub(crate) name:Entity<InputState>,
    slides:Vec<SlideId>,
    sections:Vec<Section>,
    target:Target,
    error:Option<SharedString>,
}
enum Target {Add(usize),Rename(usize)}
fn fresh(sections:&[Section])->String {
    let mut value=crate::clock::unix_millis() as u128*1_000_000;
    loop {let h=format!("{value:032X}");let id=format!("{{{}-{}-{}-{}-{}}}",&h[..8],&h[8..12],&h[12..16],&h[16..20],&h[20..]);if !sections.iter().any(|s|s.id.eq_ignore_ascii_case(&id)) {return id;}value=value.wrapping_add(1);}
}
impl SlideEditor {
    pub(crate) fn current_section(&self)->Option<usize> {
        if let Some((id,anchor))=self.pptx().and_then(|s|s.section_selection.as_ref()) {if *anchor==self.current_slide {if let Some(index)=self.presentation().sections.iter().position(|s|&s.id==id) {return Some(index);}}}
        self.presentation().sections.iter().position(|s|s.slides.contains(&self.current_slide))
    }
    pub(crate) fn select_section(&mut self,id:&str,w:&mut Window,cx:&mut Context<Self>) {
        let Some(section)=self.presentation().sections.iter().find(|s|s.id==id) else {return};let first=section.slides.first().copied();
        self.end_text_edit(w,cx);if let Some(first)=first {self.select_slide(first,w,cx);}let anchor=self.current_slide;
        if let Some(s)=self.pptx_mut() {s.section_selection=Some((id.to_owned(),anchor));}cx.notify();
    }
    pub(crate) fn toggle_section_fold(&mut self,id:&str,cx:&mut Context<Self>) {
        if !self.presentation().sections.iter().any(|s|s.id==id) {return;}
        if let Some(s)=self.pptx_mut() {if !s.folded_sections.remove(id) {s.folded_sections.insert(id.to_owned());}}cx.notify();
    }
    pub(crate) fn toggle_current_section_fold(&mut self,cx:&mut Context<Self>) {let Some(index)=self.current_section() else {return};let id=self.presentation().sections[index].id.clone();self.toggle_section_fold(&id,cx);}
    pub(crate) fn move_current_section(&mut self,direction:isize,w:&mut Window,cx:&mut Context<Self>) {
        self.end_text_edit(w,cx);let Some(index)=self.current_section() else {return};let mut sections=self.presentation().sections.clone();let next=index as isize+direction;
        if next<0 || next>=sections.len() as isize {return;}sections.swap(index,next as usize);
        let desired=sections.iter().flat_map(|s|s.slides.iter().copied()).collect::<Vec<_>>();let mut order=self.presentation().slides.iter().map(|s|s.id).collect::<Vec<_>>();let mut commands=Vec::new();
        for (at,&slide) in desired.iter().enumerate() {let Some(from)=order.iter().position(|&s|s==slide) else {return};if from!=at {commands.push(Command::MoveSlide {slide,index:at});order.remove(from);order.insert(at,slide);}}
        commands.push(Command::SetSections {sections});self.apply_pptx_commands_checked(vec![Command::Group(commands)],cx);
    }
    pub(crate) fn open_section_panel(&mut self,rename:bool,w:&mut Window,cx:&mut Context<Self>) {
        self.end_text_edit(w,cx);
        let sections=self.presentation().sections.clone();let slides=self.presentation().slides.iter().map(|s|s.id).collect::<Vec<_>>();
        let (target,value)=if rename {let Some(index)=self.current_section() else {return};(Target::Rename(index),sections[index].name.clone())} else {let Some(index)=slides.iter().position(|&s|s==self.current_slide) else {return};(Target::Add(index),crate::shell::i18n::text("pptx.section-panel.new-section").to_string())};
        let name=cx.new(|cx|InputState::new(w,cx).default_value(value));
        if let Some(s)=self.pptx_mut() {s.gradient_panel=None;s.shadow_panel=None;s.glow_panel=None;s.paragraph_panel=None;s.timing_panel=None;s.crop_panel=None;s.insets_panel=None;s.size_panel=None; s.frame_panel = None; s.comments = None; s.chart_panel = None; s.header_footer = None;s.find=None;s.section_panel=Some(SectionPanel {name:name.clone(),slides,sections,target,error:None});}
        self.sidebar_open=true;name.update(cx,|s,cx|s.focus(w,cx));cx.notify();
    }
    pub(crate) fn apply_section_panel(&mut self,w:&mut Window,cx:&mut Context<Self>) {
        let Some(p)=self.pptx().and_then(|s|s.section_panel.as_ref()) else {return};
        let name=p.name.read(cx).value().trim().to_owned();
        let conflict=self.text_editing().is_some_and(|e|e.marked.is_some()) || self.presentation().sections!=p.sections || !self.presentation().slides.iter().map(|s|s.id).eq(p.slides.iter().copied());
        if conflict || name.is_empty() {
            if let Some(p)=self.pptx_mut().and_then(|s|s.section_panel.as_mut()) {p.error=Some(if conflict {crate::shell::i18n::text("pptx.section-panel.slide-structure-has-changed")} else {crate::shell::i18n::text("pptx.section-panel.enter-name")}.into());}return cx.notify();
        }
        let mut sections=p.sections.clone();
        match p.target {
            Target::Rename(index)=>sections[index].name=name,
            Target::Add(at)=>{
                let id=fresh(&sections);
                if sections.is_empty() {
                    if at>0 {sections.push(Section {id:fresh(&[Section {id:id.clone(),name:String::new(),slides:vec![]}]),name:crate::shell::i18n::text("pptx.section-panel.default-section").to_string(),slides:p.slides[..at].to_vec()});}
                    sections.push(Section {id,name,slides:p.slides[at..].to_vec()});
                } else {
                    let target=p.slides[at];let Some(index)=sections.iter().position(|s|s.slides.contains(&target)) else {return};
                    let split=sections[index].slides.iter().position(|&s|s==target).unwrap();let slides=sections[index].slides.split_off(split);sections.insert(index+1,Section {id,name,slides});
                }
            }
        }
        if !self.apply_pptx_commands_checked(vec![Command::SetSections {sections}],cx) {
            if let Some(p)=self.pptx_mut().and_then(|s|s.section_panel.as_mut()) {p.error=Some(crate::shell::i18n::text("pptx.section-panel.cannot-apply-because-original").into());}return cx.notify();
        }
        self.close_section_panel(w,cx);
    }
    pub(crate) fn remove_current_section(&mut self,w:&mut Window,cx:&mut Context<Self>) {
        self.end_text_edit(w,cx);let Some(index)=self.current_section() else {return};let mut sections=self.presentation().sections.clone();let removed=sections.remove(index);
        if index>0 {sections[index-1].slides.extend(removed.slides);} else if let Some(next)=sections.first_mut() {let mut slides=removed.slides;slides.append(&mut next.slides);next.slides=slides;}
        self.apply_pptx_commands_checked(vec![Command::SetSections {sections}],cx);
    }
    pub(crate) fn close_section_panel(&mut self,w:&mut Window,cx:&mut Context<Self>) {if let Some(s)=self.pptx_mut() {s.section_panel=None;}self.focus_handle.focus(w,cx);cx.notify();}
    pub(crate) fn pptx_section_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel> {
        let p=self.pptx()?.section_panel.as_ref()?;let name=p.name.clone();let error=p.error.clone();let mut body=v_flex().gap_2().px(px(18.)).py(px(8.)).p_0().child(div().child(crate::shell::i18n::text("_shell.name"))).child(crate::shell::chrome::text_field(&name).full_width());
        if let Some(error)=error {body=body.child(div().text_color(rgb(0xb3261e)).child(error));}
        let panel_actions = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("pptx-section-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e,_,w,cx|e.apply_section_panel(w,cx)))).action(crate::shell::chrome::ActionDefinition::new("pptx-section-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e,_,w,cx|e.close_section_panel(w,cx))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("pptx.goto.section"), body.into_any_element(), panel_actions))
    }
}
