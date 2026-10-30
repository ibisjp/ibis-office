// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! All document formats describe ink commands; this module owns the complete panel layout.
use std::rc::Rc;
use gpui_kit::*;
use gpui_kit::component::{Sizable as _,Disableable as _,h_flex,v_flex};
use gpui_kit::component::input::{InputState,InputEvent};
use super::{chrome,editor::SlideEditor,i18n::text};
use chrome::segment_control::{Mode,SegmentItem};
use ibis_doc_model::Rgb;

type Handler=chrome::Handler;
pub(crate) type ColorHandler=Rc<dyn Fn(&mut SlideEditor,Rgb,&mut Window,&mut Context<SlideEditor>)>;
pub(crate) struct InkChoice {
    pub id:ElementId,pub label:SharedString,pub selected:bool,pub change:Handler,
    pub bounds:Option<Rc<dyn Fn(Bounds<Pixels>)>>,
}
impl InkChoice {
    pub(crate) fn new(id:impl Into<ElementId>,label:impl Into<SharedString>,selected:bool,change:impl Fn(&mut SlideEditor,&mut Window,&mut Context<SlideEditor>)+'static)->Self{
        Self{id:id.into(),label:label.into(),selected,change:Rc::new(change),bounds:None}
    }
    pub(crate) fn on_bounds(mut self,callback:impl Fn(Bounds<Pixels>)+'static)->Self{self.bounds=Some(Rc::new(callback));self}
}
pub(crate) struct InkToggle {pub id:&'static str,pub label:SharedString,pub checked:bool,pub change:Handler,pub bounds:Option<Rc<dyn Fn(Bounds<Pixels>)>>}
impl InkToggle {
    pub(crate) fn new(id:&'static str,label:impl Into<SharedString>,checked:bool,change:impl Fn(&mut SlideEditor,&mut Window,&mut Context<SlideEditor>)+'static)->Self{Self{id,label:label.into(),checked,change:Rc::new(change),bounds:None}}
    pub(crate) fn on_bounds(mut self,callback:impl Fn(Bounds<Pixels>)+'static)->Self{self.bounds=Some(Rc::new(callback));self}
}
pub(crate) struct InkCommand {pub id:&'static str,pub label:SharedString,pub enabled:bool,pub destructive:bool,pub run:Handler,pub bounds:Option<Rc<dyn Fn(Bounds<Pixels>)>>}
impl InkCommand {
    pub(crate) fn new(id:&'static str,label:impl Into<SharedString>,enabled:bool,run:impl Fn(&mut SlideEditor,&mut Window,&mut Context<SlideEditor>)+'static)->Self{Self{id,label:label.into(),enabled,destructive:false,run:Rc::new(run),bounds:None}}
    pub(crate) fn destructive(mut self)->Self{self.destructive=true;self}
    pub(crate) fn on_bounds(mut self,callback:impl Fn(Bounds<Pixels>) + 'static)->Self{self.bounds=Some(Rc::new(callback));self}
}
pub(crate) enum InkRow {Toggle(InkToggle),Commands(Vec<InkCommand>),Label(SharedString)}
pub(crate) struct InkSection {pub title:Option<SharedString>,pub rows:Vec<InkRow>}
pub(crate) struct InkPanelSpec {
    pub color_id:&'static str,pub tools:Vec<InkChoice>,pub widths:Vec<InkChoice>,
    pub drawing:bool,pub color:Rgb,pub change_color:ColorHandler,pub effects:Vec<InkChoice>,
    pub mouse:InkToggle,pub sections:Vec<InkSection>,pub message:String,
    pub context_menu:Option<Rc<dyn Fn(&mut SlideEditor,Point<Pixels>,&mut Window,&mut Context<SlideEditor>)>>,
}
fn observed(element:impl IntoElement,bounds:Option<Rc<dyn Fn(Bounds<Pixels>)>>)->AnyElement{
    let Some(callback)=bounds else{return element.into_any_element();};
    div().relative().child(element).child(canvas(|_,_,_|(),move|bounds,_,_,_|callback(bounds)).absolute().inset_0()).into_any_element()
}
fn toggle(item:InkToggle,cx:&Context<SlideEditor>)->impl IntoElement{
    observed(chrome::checkbox_button(item.id,item.label,item.checked,false,cx.listener(move|e,_,w,cx|(item.change)(e,w,cx))),item.bounds)
}
fn segment(item:InkChoice,enabled:bool,cx:&Context<SlideEditor>)->SegmentItem{
    let mut result=SegmentItem::label(item.id,item.label,item.selected).disabled(!enabled).on_click(cx.listener(move|e,_,w,cx|(item.change)(e,w,cx)));
    if let Some(bounds)=item.bounds{result=result.on_bounds(move|b|bounds(b));}result
}
pub(crate) fn panel(spec:InkPanelSpec,cx:&Context<SlideEditor>)->chrome::SidebarPanel{
    let mut body=chrome::panel_form().p_0()
        .child(chrome::SegmentControlRow::new(Mode::Intrinsic).label(text("ink.panel.tools")).children(spec.tools.into_iter().map(|item|segment(item,true,cx))))
        .child(chrome::SegmentControlRow::new(Mode::Intrinsic).label(text("ink.panel.width")).children(spec.widths.into_iter().map(|item|segment(item,spec.drawing,cx))))
        .child(chrome::color_field_row(text("ink.panel.color"),InkColorField{id:spec.color_id,color:spec.color,enabled:spec.drawing,change:spec.change_color,editor:cx.weak_entity()}));
    if !spec.effects.is_empty(){
        let mut effects=chrome::radio_group().layout(chrome::RadioGroupLayout::Flow);
        for item in spec.effects{
            let choice=chrome::radio_button(item.id,item.selected,!spec.drawing,item.label,cx.listener(move|e,_,w,cx|(item.change)(e,w,cx)));
            effects=effects.child(observed(choice,item.bounds));
        }
        body=body.child(chrome::control_row(text("ink.panel.effects"),effects));
    }
    body=body.child(toggle(spec.mouse,cx));
    for section in spec.sections{
        let mut contents=v_flex().w_full().gap_2();
        if let Some(title)=section.title{contents=contents.child(div().text_sm().child(title));}
        for row in section.rows{contents=match row{
            InkRow::Toggle(item)=>contents.child(toggle(item,cx)),
            InkRow::Label(label)=>contents.child(div().text_sm().child(label)),
            InkRow::Commands(commands)=>{
                let mut buttons=h_flex().w_full().flex_wrap().gap_2();
                for command in commands{
                    let style=chrome::ActionButtonStyle{kind:if command.destructive{chrome::ActionButtonKind::Destructive}else{chrome::ActionButtonKind::Normal}};
                    buttons=buttons.child(observed(chrome::styled_action_button(command.id,style,Some(cx)).small().label(command.label).disabled(!command.enabled).on_click(cx.listener(move|e,_,w,cx|(command.run)(e,w,cx))),command.bounds));
                }
                contents.child(buttons)
            }
        };}
        body=body.child(contents);
    }
    body=body.children(chrome::panel_message(&spec.message));
    let body=if let Some(context_menu)=spec.context_menu{div().id("shared-ink-panel").child(body).on_mouse_down(MouseButton::Right,cx.listener(move|e,event:&MouseDownEvent,w,cx|context_menu(e,event.position,w,cx))).into_any_element()}else{body.into_any_element()};
    chrome::SidebarPanel{title:text("ink.panel.title"),body}
}

/// The same color TextField used elsewhere, with persistent input and subscription per format.
#[derive(IntoElement)]
struct InkColorField{id:&'static str,color:Rgb,enabled:bool,change:ColorHandler,editor:WeakEntity<SlideEditor>}
impl RenderOnce for InkColorField {
    fn render(self,w:&mut Window,cx:&mut App)->impl IntoElement{
        let initial=format!("{:06X}",self.color.0);
        let input=w.use_keyed_state(self.id,cx,|w,cx|{let mut i=InputState::new(w,cx);i.set_value(initial.clone(),w,cx);i});
        let subscribe_input=input.clone();let owner=self.editor.clone();let change=self.change;
        let _subscription=w.use_keyed_state(SharedString::from(format!("{}-subscription",self.id)),cx,move|w,cx|owner.update(cx,|_,cx|cx.subscribe_in(&subscribe_input,w,move|e,input,event:&InputEvent,w,cx|{
            if matches!(event,InputEvent::Change|InputEvent::PressEnter{..}){
                let value=input.read(cx).value().trim().trim_start_matches('#').to_string();
                if value.len()==6{if let Ok(color)=u32::from_str_radix(&value,16){change(e,Rgb(color),w,cx);}}
            }
        })).ok());
        if !input.read(cx).focus_handle(cx).is_focused(w)&&input.read(cx).value().as_ref()!=initial{
            input.update(cx,|i,cx|i.set_value(initial,w,cx));
        }
        self.editor.update(cx,|_,cx|chrome::panel_color_field_enabled(self.id,&input,self.enabled,cx)).unwrap_or_else(|_|div().into_any_element())
    }
}
