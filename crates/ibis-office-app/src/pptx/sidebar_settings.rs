// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Live slide/selection settings. Input entities survive document changes and rerenders.
use gpui_kit::*;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::v_flex;
use ibis_doc_model::{Color, Command, EmuSize, Fill, Rgb, ShapeId, ShapeKind, SlideId, Stroke};
use crate::shell::chrome::{self, MenuEntry, SidebarPanel, SpinnerSpec};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct SidebarSettings {
    key: (SlideId, Vec<ShapeId>),
    pub(crate) fields: Vec<Entity<InputState>>,
    values: Vec<String>,
    _subscriptions: Vec<Subscription>,
}
fn number(value: f64) -> String {format!("{value:.4}").trim_end_matches('0').trim_end_matches('.').to_string()}
fn color(fill: &Fill) -> String {if let Fill::Solid(c) = fill {format!("{:06X}", c.rgb.0)} else {String::new()}}
fn values(editor: &SlideEditor) -> Vec<String> {
    if let Some(shape) = editor.selected_shapes().first() {
        let (fill, stroke) = match &shape.kind {
            ShapeKind::Auto(a) => (color(&a.fill), a.stroke.as_ref().map(|s|format!("{:06X}",s.color.rgb.0)).unwrap_or_default()),
            _ => (String::new(), String::new()),
        };
        vec![number(shape.frame.width as f64/360_000.), number(shape.frame.height as f64/360_000.), number(shape.rotation as f64/60_000.), number(shape.frame.x as f64/360_000.), number(shape.frame.y as f64/360_000.), fill, stroke]
    } else {
        let size = editor.presentation().slide_size;
        vec![number(size.width as f64/360_000.), number(size.height as f64/360_000.), String::new(), String::new(), String::new(), color(&editor.slide().background), String::new()]
    }
}
fn parsed_color(value: &str) -> Option<Option<Color>> {
    let value = value.trim().trim_start_matches('#');
    if value.is_empty() {return Some(None);}
    if value.len() != 6 {return None;}
    u32::from_str_radix(value,16).ok().map(|v|Some(Color::opaque(Rgb(v))))
}
impl SlideEditor {
    fn apply_sidebar_setting(&mut self, field: usize, raw: &str, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.selected_ids();
        let key = (self.current_slide, ids.clone());
        if !self.pptx().and_then(|s|s.sidebar_settings.as_ref()).is_some_and(|s|s.key == key) {return;}
        if field >= 5 {
            let Some(color) = parsed_color(raw) else {return;};
            let fill = color.map(Fill::Solid).unwrap_or(Fill::None);
            if ids.is_empty() {
                if self.slide().background != fill {self.set_background(fill,cx);}
            } else {
                let commands = ids.into_iter().filter_map(|id| {
                    let shape = self.slide().shape(id)?;
                    let ShapeKind::Auto(auto) = &shape.kind else {return None;};
                    if field == 5 {
                        (auto.fill != fill).then(||Command::SetFill {slide:self.current_slide,shape:id,fill:fill.clone()})
                    } else {
                        let stroke = color.map(|color|{let mut stroke = auto.stroke.clone().unwrap_or_else(||Stroke::solid(color,12_700));stroke.color=color;stroke});
                        (auto.stroke != stroke).then(||Command::SetStroke {slide:self.current_slide,shape:id,stroke})
                    }
                }).collect();
                self.run(commands,cx);
            }
        } else {
            let Some(value) = raw.trim().parse::<f64>().ok().filter(|v|v.is_finite()) else {return;};
            let scale = if field == 2 {60_000.} else {360_000.};
            let emu = (value*scale).round();
            if emu < i32::MIN as f64 || emu > i32::MAX as f64 || (field <= 1 && value <= 0.) {return;}
            if ids.is_empty() {
                if field > 1 || !(2.54..=142.24).contains(&value) {return;}
                let mut size = self.presentation().slide_size;
                if field == 0 {size.width=emu as i64;}else{size.height=emu as i64;}
                self.set_slide_size(size,window,cx);
            } else {
                let commands=ids.into_iter().filter_map(|id| {
                    let shape=self.slide().shape(id)?;
                    if field==2 {
                        (shape.rotation != emu as i32).then(||Command::SetTransform {slide:self.current_slide,shape:id,rotation:emu as i32,flip_h:shape.flip_h,flip_v:shape.flip_v})
                    } else {
                        let mut frame=shape.frame;
                        match field {0=>frame.width=emu as i64,1=>frame.height=emu as i64,3=>frame.x=emu as i64,_=>frame.y=emu as i64}
                        (frame!=shape.frame).then(||Command::SetFrame {slide:self.current_slide,shape:id,frame})
                    }
                }).collect();
                self.run(commands,cx);
            }
        }
        let current=values(self);
        if let Some(s)=self.pptx_mut().and_then(|s|s.sidebar_settings.as_mut()) {s.values=current;}
        cx.notify();
    }
    fn ensure_sidebar_settings(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        let key=(self.current_slide,self.selected_ids());let current=values(self);
        if self.pptx().and_then(|s|s.sidebar_settings.as_ref()).is_none_or(|s|s.key!=key) {
            let fields:Vec<_>=current.iter().map(|value|cx.new(|cx|InputState::new(window,cx).default_value(value.clone()))).collect();
            let subscriptions=fields.iter().enumerate().map(|(index,field)|cx.subscribe_in(field,window,move|e,field,event:&InputEvent,w,cx| {
                if matches!(event,InputEvent::Change) {let value=field.read(cx).value().to_string();e.apply_sidebar_setting(index,&value,w,cx);}
            })).collect();
            if let Some(p)=self.pptx_mut() {p.sidebar_settings=Some(SidebarSettings {key,fields,values:current,_subscriptions:subscriptions});}
        } else {
            let pending=self.pptx().and_then(|s|s.sidebar_settings.as_ref()).map(|s|s.fields.iter().zip(&s.values).zip(&current).filter(|((_,old),new)|old!=new).map(|((f,_),new)|(f.clone(),new.clone())).collect::<Vec<_>>()).unwrap_or_default();
            if let Some(s)=self.pptx_mut().and_then(|s|s.sidebar_settings.as_mut()){s.values=current;}
            for (field,value) in pending {if field.read(cx).value().as_str()!=value {field.update(cx,|s,cx|s.set_value(value,window,cx));}}
        }
    }
}
fn dropdown(id:&'static str,label:SharedString,entries:Vec<MenuEntry>,cx:&Context<SlideEditor>)->AnyElement {
    chrome::Dropdown::from_entries(id,label,entries,true,0,&cx.weak_entity()).into_any_element()
}
pub(crate) fn sidebar(editor:&mut SlideEditor,window:&mut Window,cx:&mut Context<SlideEditor>)->Option<SidebarPanel> {
    editor.ensure_sidebar_settings(window,cx);
    let state=editor.pptx()?.sidebar_settings.as_ref()?;let fields=state.fields.clone();let selected=!state.key.1.is_empty();
    let mut body=v_flex().gap_2();
    if selected {
        let has_fill=editor.selected_shapes().first().is_some_and(|s|matches!(s.kind,ShapeKind::Auto(_)));
        if has_fill {
            body=body.child(chrome::color_field_row(text("pptx.common.fill"),chrome::panel_color_field("pptx-sidebar-fill",&fields[5],cx)))
                .child(chrome::color_field_row(text("_shell.line"),chrome::panel_color_field("pptx-sidebar-stroke",&fields[6],cx)));
        }
    } else {
        let slide=editor.slide();
        let layout=slide.layout.and_then(|id|editor.presentation().layout(id)).map(|l|l.name.clone().into()).unwrap_or_else(||text("_shell.none"));
        let size=editor.presentation().slide_size;
        let size_label=if size==EmuSize::WIDESCREEN {text("pptx.chrome.widescreen-16-9")}else if size==EmuSize::new(9_144_000,6_858_000){text("pptx.chrome.standard-4-3")}else{text("_shell.custom")};
        body=body.child(chrome::color_field_row(text("_shell.background"),chrome::panel_color_field("pptx-sidebar-background",&fields[5],cx)))
            .child(chrome::dropdown_row(text("_shell.layout"),dropdown("pptx-sidebar-layout",layout,super::chrome::apply_layout_items(editor),cx)))
            .child(chrome::dropdown_row(text("_shell.transitions"),dropdown("pptx-sidebar-transition",super::chrome::transition_name(editor.slide().transition.as_ref()).into(),super::chrome::transition_menu(editor),cx)))
            .child(chrome::dropdown_row(text("_shell.size"),dropdown("pptx-sidebar-size",size_label,super::chrome::slide_size_menu_items(editor),cx)));
    }
    let labels=[text("_shell.width"),text("_shell.height"),text("_shell.rotate"),SharedString::from("X"),SharedString::from("Y")];
    for (index,label) in labels.into_iter().enumerate().take(if selected {5}else{2}) {
        let spec=if index==2 {SpinnerSpec::new(1.).range(-360.,360.)}else if index<=1 {SpinnerSpec::new(0.1).min(if selected {0.01}else{2.54})}else{SpinnerSpec::new(0.1)};
        body=body.child(chrome::text_field_row(label,chrome::text_field(&fields[index]).unit(if index==2 {"°"}else{"cm"},px(24.)).spinner(SharedString::from(format!("pptx-sidebar-field-{index}")),spec,cx,|_,_,_,_|{})));
    }
    Some(SidebarPanel {title:text(if selected {"_shell.format-options"}else{"_shell.slide"}),body:body.into_any_element()})
}
