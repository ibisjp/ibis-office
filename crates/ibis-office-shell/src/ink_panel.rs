// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! All document formats describe ink commands; this module owns the complete panel layout.
use std::rc::Rc;
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::{Sizable as _,Disableable as _,h_flex,v_flex};
use gpui_kit::component::input::{InputState,InputEvent};
use super::{chrome,editor::SlideEditor,i18n::text};
use chrome::segment_control::{Mode,SegmentItem};
use ibis_doc_model::Rgb;

type Handler=chrome::Handler;
pub(crate) type ColorHandler=Rc<dyn Fn(&mut SlideEditor,Rgb,&mut Window,&mut Context<SlideEditor>)>;
/// What a pen of an effect draws with: the square at the left of the effect's name in the list is painted from it.
#[derive(Clone)]
pub enum InkTexture {
    /// The pen's own colour (no effect).
    Solid(u32),
    /// Colours that run along the stroke (position 0..=100000, 0xRRGGBB), as the spreadsheet and document effects draw it.
    Gradient(Vec<(u32,u32)>),
    /// Colours that follow each other along the stroke, one per segment, as the presentation effects draw it.
    Bands(Vec<u32>),
}
impl InkTexture {
    /// The colour at `x` of `width` (the strip the thumbnail paints).
    fn color_at(&self,x:f32,width:f32)->u32 {
        match self {
            Self::Solid(rgb)=>*rgb,
            Self::Gradient(stops)=>{
                let stops:Vec<ibis_doc_model::GradientStop>=stops.iter().map(|&(position,rgb)|ibis_doc_model::GradientStop{position,color:ibis_doc_model::Color::opaque(Rgb(rgb))}).collect();
                chrome::gradient_slider::interpolated(&stops,((x/width.max(1.)).clamp(0.,1.)*100_000.).round() as u32).rgb.0
            }
            Self::Bands(colors)=>colors.get(((x/width.max(1.)).clamp(0.,0.9999)*colors.len() as f32) as usize).copied().unwrap_or(0),
        }
    }
}
/// The texture's square (the thumbnail): painted in 1 px strips, as a gradient quad is not drawn inside a list.
fn thumbnail(texture:InkTexture)->AnyElement {
    div().relative().flex_none().size(px(THUMB)).rounded(px(2.)).border_1().border_color(gpui_kit::rgb(chrome::BORDER)).overflow_hidden()
        .child(canvas(|_,_,_|(),move|bounds,_,window,_|{
            let (w,h)=(f32::from(bounds.size.width),f32::from(bounds.size.height));
            let mut x=0.;
            while x<w{
                let strip=(w-x).min(1.);
                window.paint_quad(fill(Bounds::new(bounds.origin+point(px(x),px(0.)),size(px(strip),px(h))),gpui_kit::rgb(texture.color_at(x+strip/2.,w))));
                x+=1.;
            }
        }).absolute().inset_0()).into_any_element()
}
/// The side of the texture's square in the list of effects.
const THUMB:f32=20.;
pub struct InkChoice {
    pub id:ElementId,pub label:SharedString,pub selected:bool,pub change:Handler,
    pub bounds:Option<Rc<dyn Fn(Bounds<Pixels>)>>,
    pub texture:Option<InkTexture>,
    /// The width in pt, for a choice of the pen's width.
    pub points:f32,
}
impl InkChoice {
    pub fn new(id:impl Into<ElementId>,label:impl Into<SharedString>,selected:bool,change:impl Fn(&mut SlideEditor,&mut Window,&mut Context<SlideEditor>)+'static)->Self{
        Self{id:id.into(),label:label.into(),selected,change:Rc::new(change),bounds:None,texture:None,points:0.}
    }
    pub fn on_bounds(mut self,callback:impl Fn(Bounds<Pixels>)+'static)->Self{self.bounds=Some(Rc::new(callback));self}
    /// What the effect draws with (the square before its name in the list of effects).
    /// The pen's width in pt that the choice sets (the line its item draws).
    pub fn points(mut self,points:f32)->Self{self.points=points;self}
    pub fn texture(mut self,texture:InkTexture)->Self{self.texture=Some(texture);self}
}
pub struct InkToggle {pub id:&'static str,pub label:SharedString,pub checked:bool,pub change:Handler,pub bounds:Option<Rc<dyn Fn(Bounds<Pixels>)>>}
impl InkToggle {
    pub fn new(id:&'static str,label:impl Into<SharedString>,checked:bool,change:impl Fn(&mut SlideEditor,&mut Window,&mut Context<SlideEditor>)+'static)->Self{Self{id,label:label.into(),checked,change:Rc::new(change),bounds:None}}
    pub fn on_bounds(mut self,callback:impl Fn(Bounds<Pixels>)+'static)->Self{self.bounds=Some(Rc::new(callback));self}
}
pub struct InkCommand {pub id:&'static str,pub label:SharedString,pub enabled:bool,pub destructive:bool,pub run:Handler,pub bounds:Option<Rc<dyn Fn(Bounds<Pixels>)>>}
impl InkCommand {
    pub fn new(id:&'static str,label:impl Into<SharedString>,enabled:bool,run:impl Fn(&mut SlideEditor,&mut Window,&mut Context<SlideEditor>)+'static)->Self{Self{id,label:label.into(),enabled,destructive:false,run:Rc::new(run),bounds:None}}
    #[allow(dead_code)] // Shared-control option (a red command); no ink panel has one yet.
    pub(crate) fn destructive(mut self)->Self{self.destructive=true;self}
    #[allow(dead_code)] // Shared-control option (where the command was painted); no ink panel asks for it yet.
    pub(crate) fn on_bounds(mut self,callback:impl Fn(Bounds<Pixels>) + 'static)->Self{self.bounds=Some(Rc::new(callback));self}
}
/// `Commands` are small buttons side by side (the ruler's turns); `Group` is a `button_group` of equal-width buttons, one per line.
pub enum InkRow {Toggle(InkToggle),Commands(Vec<InkCommand>),Group(Vec<InkCommand>),Label(SharedString)}
pub struct InkSection {pub title:Option<SharedString>,pub rows:Vec<InkRow>}
/// What the actions of every format's ink pane are (the same rows in the same order for the document, spreadsheet and presentation
/// screens): convert to a shape, [text, math, replay: presentation only], select all, erase the selection, erase all. The screen gives the
/// handlers; `None` hides the row it cannot do.
pub struct InkActions {
    pub selected:usize,pub has_ink:bool,
    pub to_shape:Handler,pub select_all:Handler,pub erase_selected:Handler,pub erase_all:Handler,
    pub to_text:Option<Handler>,pub to_math:Option<Handler>,pub replay:Option<Handler>,
}
impl InkActions {
    pub fn section(self)->InkSection {
        let command=|id:&'static str,label:SharedString,enabled:bool,run:Handler|InkCommand{id,label,enabled,destructive:false,run,bounds:None};
        let has=self.selected>0;
        let mut commands=vec![command("ink-to-shape",text("xlsx.ink.to-shape"),has,self.to_shape)];
        if let Some(run)=self.to_text{commands.push(command("ink-to-text",text("pptx.ink.to-text"),has,run));}
        if let Some(run)=self.to_math{commands.push(command("ink-to-math",text("pptx.ink.to-math"),has,run));}
        if let Some(run)=self.replay{commands.push(command("ink-replay",text("pptx.ink.replay"),self.has_ink,run));}
        commands.push(command("ink-select-all",text("docx.ink.select-all"),self.has_ink,self.select_all));
        commands.push(InkCommand{destructive:true,..command("ink-erase-selected",text("docx.ink.erase-selected"),has,self.erase_selected)});
        commands.push(InkCommand{destructive:true,..command("ink-erase-all",text("docx.ink.erase-all"),self.has_ink,self.erase_all)});
        InkSection{title:Some(format!("{} ({})",text("docx.ink.actions"),self.selected).into()),rows:vec![InkRow::Group(commands)]}
    }
}
pub struct InkPanelSpec {
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
/// The width of the pen: a dropdown whose items draw the line (see `line_width_dropdown`). The pen not drawing (the eraser, select) dims it and takes its clicks.
fn width_row(widths:Vec<InkChoice>,drawing:bool,cx:&Context<SlideEditor>)->impl IntoElement{
    let selected=widths.iter().position(|c|c.selected).unwrap_or(0);
    let items=widths.into_iter().map(|item|(item.points,item.label,item.change)).collect();
    let dropdown=chrome::preview_dropdown::line_width_dropdown("ink-width",items,selected,chrome::preview_dropdown::PreviewSize::default().width(px(138.)),cx);
    let control=div().relative().child(dropdown).when(!drawing,|d|d.opacity(0.5).child(div().absolute().inset_0().occlude()));
    chrome::dropdown_row(text("ink.panel.width"),control)
}
pub fn panel(spec:InkPanelSpec,cx:&Context<SlideEditor>)->chrome::SidebarPanel{
    let mut body=chrome::panel_form().p_0()
        .child(chrome::SegmentControlRow::new(Mode::Intrinsic).label(text("ink.panel.tools")).children(spec.tools.into_iter().map(|item|segment(item,true,cx))))
        .child(width_row(spec.widths,spec.drawing,cx))
        .child(chrome::color_field_row(text("ink.panel.color"),InkColorField{id:spec.color_id,color:spec.color,enabled:spec.drawing,change:spec.change_color,editor:cx.weak_entity()}));
    if !spec.effects.is_empty(){
        // The effects are a drop-down list: a square of the texture at the left of each item, its name at the right (the same on the
        // closed button). The pen not drawing (the eraser, select) dims it and takes its clicks.
        let selected=spec.effects.iter().position(|c|c.selected).unwrap_or(0);
        let choices:Vec<chrome::preview_dropdown::PreviewChoice>=spec.effects.into_iter().map(|item|{
            let (texture,label)=(item.texture.unwrap_or(InkTexture::Solid(spec.color.0)),item.label.clone());
            let change=item.change.clone();
            chrome::preview_dropdown::PreviewChoice{
                label:item.label,
                content:Rc::new(move||h_flex().w_full().min_w_0().items_center().gap(px(8.)).child(thumbnail(texture.clone())).child(div().min_w_0().truncate().text_size(px(14.)).child(label.clone())).into_any_element()),
                choose:change,
            }
        }).collect();
        let dropdown=chrome::preview_dropdown::preview_dropdown_sized("ink-effect",choices,selected,chrome::preview_dropdown::PreviewSize::default().width(px(138.)),cx);
        let control=div().relative().child(dropdown).when(!spec.drawing,|d|d.opacity(0.5).child(div().absolute().inset_0().occlude()));
        body=body.child(chrome::dropdown_row(text("ink.panel.effects"),control));
    }
    body=body.child(toggle(spec.mouse,cx));
    for section in spec.sections{
        let mut contents=v_flex().w_full().gap_2();
        if let Some(title)=section.title{contents=contents.child(div().text_sm().child(title));}
        for row in section.rows{contents=match row{
            InkRow::Toggle(item)=>contents.child(toggle(item,cx)),
            InkRow::Label(label)=>contents.child(div().text_sm().child(label)),
            InkRow::Group(commands)=>contents.child(chrome::button_group(commands.into_iter().map(|command|{
                let run=command.run;
                chrome::ButtonGroupItem::new(command.id,command.label).kind(if command.destructive{chrome::ActionButtonKind::Destructive}else{chrome::ActionButtonKind::Normal}).disabled(!command.enabled)
                    .on_click(cx.listener(move|e,_,w,cx|run(e,w,cx)))
            })).stacked()),
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
