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
use crate::ext::*;
use crate::shell::effect_editor::{self, EffectEditor, EffectHooks, EffectModel, SECTIONS};
use crate::shell::shape_format_pane::{self as pane, DashUi};

pub(crate) struct SidebarSettings {
    key: (SlideId, Vec<ShapeId>),
    pub(crate) fields: Vec<Entity<InputState>>,
    values: Vec<String>,
    /// The shared decoration editor (shadow, glow, soft edge, reflection; the text's outline, shadow, glow) of the selected shape.
    pub(crate) effects: Option<EffectState>,
    /// The shared 図形の書式 pane's line-style dropdown.
    pub(crate) dash: DashUi,
    _subscriptions: Vec<Subscription>,
}
pub(crate) struct EffectState {pub(crate) editor: EffectEditor, pub(crate) enabled: [bool; SECTIONS], pub(crate) initial: EffectModel}
fn hooks_effects() -> EffectHooks {
    EffectHooks {
        get: |e| e.pptx_mut().and_then(|p| p.sidebar_settings.as_mut()).and_then(|s| s.effects.as_mut()).map(|s| &mut s.editor),
        toggle: |e, i| {if let Some(s) = e.pptx_mut().and_then(|p| p.sidebar_settings.as_mut()).and_then(|s| s.effects.as_mut()) {s.enabled[i] = !s.enabled[i];}},
        apply: |e, w, cx| e.apply_sidebar_effects(w, cx),
    }
}
/// The decoration the first selected shape (and its first run) has; the text part is empty for a shape without text.
fn effect_model(editor: &SlideEditor) -> Option<EffectModel> {
    EffectModel::of_shape(editor.selected_shapes().into_iter().next()?)
}
fn effect_flags(m: &EffectModel) -> [bool; SECTIONS] {m.flags()}
/// The colour the text of the first selected shape has (its first run's, or the body's).
fn text_color(editor: &SlideEditor) -> String {
    let Some(shape) = editor.selected_shapes().into_iter().next() else {return String::new()};
    let Some(body) = shape.text_body() else {return String::new()};
    let rgb = body.paragraphs.iter().flat_map(|p| p.runs.iter()).next().and_then(|r| r.style.color).unwrap_or(body.style.color);
    format!("{:06X}", rgb.0)
}
/// The size (points) the text of the first selected shape has (its first run's, or the body's).
fn text_size(editor: &SlideEditor) -> String {
    let Some(shape) = editor.selected_shapes().into_iter().next() else {return String::new()};
    let Some(body) = shape.text_body() else {return String::new()};
    let size = body.paragraphs.iter().flat_map(|p| p.runs.iter()).next().and_then(|r| r.style.font_size).unwrap_or(body.style.font_size);
    number(size as f64/100.)
}
/// The dash pattern the first selected shape's outline has.
fn dash_of(editor: &SlideEditor) -> ibis_doc_model::LineDash {
    match editor.selected_shapes().into_iter().next().map(|s| &s.kind) {Some(ShapeKind::Auto(a)) => a.stroke.as_ref().map_or(ibis_doc_model::LineDash::Solid, |s| s.dash), _ => ibis_doc_model::LineDash::Solid}
}
fn number(value: f64) -> String {format!("{value:.4}").trim_end_matches('0').trim_end_matches('.').to_string()}
fn color(fill: &Fill) -> String {if let Fill::Solid(c) = fill {format!("{:06X}", c.rgb.0)} else {String::new()}}
fn values(editor: &SlideEditor) -> Vec<String> {
    if let Some(shape) = editor.selected_shapes().first() {
        let (fill, stroke) = match &shape.kind {
            ShapeKind::Auto(a) => (color(&a.fill), a.stroke.as_ref().map(|s|format!("{:06X}",s.color.rgb.0)).unwrap_or_default()),
            _ => (String::new(), String::new()),
        };
        let width = match &shape.kind {ShapeKind::Auto(a) => a.stroke.as_ref().map(|s|number(s.width as f64/12_700.)).unwrap_or_else(||"0.75".into()), _ => String::new()};
        vec![number(shape.frame.width as f64/360_000.), number(shape.frame.height as f64/360_000.), number(shape.rotation as f64/60_000.), number(shape.frame.x as f64/360_000.), number(shape.frame.y as f64/360_000.), fill, stroke, text_color(editor), width, text_size(editor)]
    } else {
        let size = editor.presentation().slide_size;
        vec![number(size.width as f64/360_000.), number(size.height as f64/360_000.), String::new(), String::new(), String::new(), color(&editor.slide().background), String::new(), String::new(), String::new(), String::new()]
    }
}
fn parsed_color(value: &str) -> Option<Option<Color>> {
    let value = value.trim().trim_start_matches('#');
    if value.is_empty() {return Some(None);}
    if value.len() != 6 {return None;}
    u32::from_str_radix(value,16).ok().map(|v|Some(Color::opaque(Rgb(v))))
}
#[ibis_office_macros::editor_ext(SidebarSettingsExt)]
impl SlideEditor {
    fn apply_sidebar_setting(&mut self, field: usize, raw: &str, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.selected_ids();
        let key = (self.current_slide, ids.clone());
        if !self.pptx().and_then(|s|s.sidebar_settings.as_ref()).is_some_and(|s|s.key == key) {return;}
        if field == 8 {
            let Ok(pt) = raw.trim().parse::<f64>() else {return;};
            if !pt.is_finite() || pt <= 0. || pt > 1584. {return;}
            let width = (pt*12_700.).round() as i64;
            let commands = ids.into_iter().filter_map(|id| {
                let shape = self.slide().shape(id)?;
                let ShapeKind::Auto(auto) = &shape.kind else {return None;};
                let mut stroke = auto.stroke.clone()?;
                if stroke.width == width {return None;}
                stroke.width = width;
                Some(Command::SetStroke {slide:self.current_slide,shape:id,stroke:Some(stroke)})
            }).collect();
            self.run(commands,cx);
        } else if field == 9 {
            let Ok(pt) = raw.trim().parse::<f32>() else {return;};
            if !(1.0..=1638.).contains(&pt) {return;}
            self.format_characters(window, cx, ibis_doc_model::CharacterFormat {font_size: Some((pt*100.).round() as u32), ..Default::default()});
        } else if field == 7 {
            let value = raw.trim().trim_start_matches('#');
            if value.len() == 6 {
                if let Ok(v) = u32::from_str_radix(value, 16) {self.format_characters(window, cx, ibis_doc_model::CharacterFormat {color: Some(Rgb(v)), ..Default::default()});}
            }
        } else if field >= 5 {
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
    /// The decoration fields changed: write what differs from what was shown (shape effects, and the text's outline, shadow, glow).
    fn apply_sidebar_effects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(st)=self.pptx().and_then(|s|s.sidebar_settings.as_ref()).and_then(|s|s.effects.as_ref()) else {return};
        let Some(model)=effect_editor::model(&st.editor,&st.initial,&st.enabled,false,"",cx) else {return};
        let initial=st.initial.clone();
        if model==initial {return;}
        let commands=self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Auto(auto)=&shape.kind else {return None};
            let next=model.written_to(&initial,auto);
            (next!=*auto).then(||Command::SetKind {slide:self.current_slide,shape:shape.id,kind:Box::new(ShapeKind::Auto(next))})
        }).collect();
        self.apply_pptx_commands_checked(commands,cx);
        let text=model.text_change(&initial);
        if text!=ibis_doc_model::CharacterFormat::default() {self.format_characters(window,cx,text);}
        if let Some(st)=self.pptx_mut().and_then(|s|s.sidebar_settings.as_mut()).and_then(|s|s.effects.as_mut()) {st.initial=model;}
        cx.notify();
    }
    /// The input of the shared pane's field `index` (0 width, 1 height, 2 rotation, 3 X, 4 Y, 5 fill, 6 outline, 7 text colour, 8 outline width, 9 text size).
    pub fn sidebar_field(&self, index: usize) -> Option<Entity<InputState>> {
        self.pptx()?.sidebar_settings.as_ref()?.fields.get(index).cloned()
    }
    /// Choose `dash` in the shared pane's line-style dropdown and apply it.
    pub fn choose_sidebar_dash(&mut self, dash: ibis_doc_model::LineDash, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.pptx_mut().and_then(|s|s.sidebar_settings.as_mut()) {s.dash.dash=dash;}
        self.apply_sidebar_dash(window,cx);
    }
    /// The dash pattern chosen in the shared 図形の書式 pane is written to the selected shapes' outlines.
    fn apply_sidebar_dash(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(dash)=self.pptx().and_then(|s|s.sidebar_settings.as_ref()).map(|s|s.dash.dash) else {return};
        let commands=self.selected_shapes().into_iter().filter_map(|shape| {
            let ShapeKind::Auto(auto)=&shape.kind else {return None};
            let mut stroke=auto.stroke.clone()?;
            if stroke.dash==dash {return None;}
            stroke=stroke.with_dash(dash);
            Some(Command::SetStroke {slide:self.current_slide,shape:shape.id,stroke:Some(stroke)})
        }).collect();
        self.run(commands,cx);
        cx.notify();
    }
    fn ensure_sidebar_settings(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        let key=(self.current_slide,self.selected_ids());let current=values(self);
        if self.pptx().and_then(|s|s.sidebar_settings.as_ref()).is_none_or(|s|s.key!=key) {
            let fields:Vec<_>=current.iter().map(|value|cx.new(|cx|InputState::new(window,cx).default_value(value.clone()))).collect();
            let subscriptions=fields.iter().enumerate().map(|(index,field)|cx.subscribe_in(field,window,move|e,field,event:&InputEvent,w,cx| {
                if matches!(event,InputEvent::Change) {let value=field.read(cx).value().to_string();e.apply_sidebar_setting(index,&value,w,cx);}
            })).collect();
            let mut subscriptions:Vec<Subscription>=subscriptions;
            let effects=effect_model(self).map(|initial| {
                let (editor,subs)=EffectEditor::new(&initial,hooks_effects(),window,cx);
                subscriptions.extend(subs);
                EffectState {enabled:effect_flags(&initial),editor,initial}
            });
            let dash=DashUi::new(dash_of(self));
            if let Some(p)=self.pptx_mut() {p.sidebar_settings=Some(SidebarSettings {key,fields,values:current,effects,dash,_subscriptions:subscriptions});}
        } else {
            let pending=self.pptx().and_then(|s|s.sidebar_settings.as_ref()).map(|s|s.fields.iter().zip(&s.values).zip(&current).filter(|((_,old),new)|old!=new).map(|((f,_),new)|(f.clone(),new.clone())).collect::<Vec<_>>()).unwrap_or_default();
            let dash=dash_of(self);
            if let Some(s)=self.pptx_mut().and_then(|s|s.sidebar_settings.as_mut()){s.values=current;if !s.dash.open {s.dash.dash=dash;}}
            if let Some(model)=effect_model(self) {
                let stale=self.pptx().and_then(|s|s.sidebar_settings.as_ref()).and_then(|s|s.effects.as_ref()).is_some_and(|e|e.initial!=model);
                if stale {
                    if let Some(st)=self.pptx().and_then(|s|s.sidebar_settings.as_ref()).and_then(|s|s.effects.as_ref()) {st.editor.show(&model,hooks_effects(),window,cx);}
                    if let Some(st)=self.pptx_mut().and_then(|s|s.sidebar_settings.as_mut()).and_then(|s|s.effects.as_mut()) {st.enabled=effect_flags(&model);st.initial=model;}
                }
            }
            for (field,value) in pending {if field.read(cx).value().as_str()!=value {field.update(cx,|s,cx|s.set_value(value,window,cx));}}
        }
    }
}
/// The layout and size dropdowns of the slide pane are this many times the usual dropdown width.
const WIDE_DROPDOWN_SCALE:f32=1.5;
fn wide_dropdown(id:&'static str,label:SharedString,entries:Vec<MenuEntry>,cx:&Context<SlideEditor>)->AnyElement {
    chrome::Dropdown::from_entries(id,label,entries,true,0,&cx.weak_entity()).button_width(chrome::FIELD_WIDTH*WIDE_DROPDOWN_SCALE).into_any_element()
}
fn dropdown(id:&'static str,label:SharedString,entries:Vec<MenuEntry>,cx:&Context<SlideEditor>)->AnyElement {
    chrome::Dropdown::from_entries(id,label,entries,true,0,&cx.weak_entity()).into_any_element()
}
pub(crate) fn sidebar(editor:&mut SlideEditor,window:&mut Window,cx:&mut Context<SlideEditor>)->Option<SidebarPanel> {
    editor.ensure_sidebar_settings(window,cx);
    let state=editor.pptx()?.sidebar_settings.as_ref()?;let fields=state.fields.clone();let selected=!state.key.1.is_empty();
    if selected {return Some(shape_format(editor,fields,cx));}
    let mut body=v_flex().gap_2();
    let slide=editor.slide();
    let layout=slide.layout.and_then(|id|editor.presentation().layout(id)).map(|l|l.name.clone().into()).unwrap_or_else(||text("_shell.none"));
    let size=editor.presentation().slide_size;
    let size_label=if size==EmuSize::WIDESCREEN {text("pptx.chrome.widescreen-16-9")}else if size==EmuSize::new(9_144_000,6_858_000){text("pptx.chrome.standard-4-3")}else{text("_shell.custom")};
    body=body.child(chrome::color_field_row(text("_shell.background"),chrome::panel_color_field("pptx-sidebar-background",&fields[5],cx)))
        .child(chrome::dropdown_row_scaled(text("_shell.layout"),wide_dropdown("pptx-sidebar-layout",layout,super::chrome::apply_layout_items(editor),cx),WIDE_DROPDOWN_SCALE))
        .child(chrome::dropdown_row(text("_shell.transitions"),dropdown("pptx-sidebar-transition",super::chrome::transition_name(editor.slide().transition.as_ref()).into(),super::chrome::transition_menu(editor),cx)))
        .child(chrome::dropdown_row_scaled(text("_shell.size"),wide_dropdown("pptx-sidebar-size",size_label,super::chrome::slide_size_menu_items(editor),cx),WIDE_DROPDOWN_SCALE));
    // 効果のオプション of the slide's transition (its direction…), beside the transition.
    if let Some(t)=editor.slide().transition.as_ref().filter(|t|!super::chrome::transition_directions(&t.effect).is_empty()) {
        let options=super::chrome::transition_directions(&t.effect);
        let current=options.iter().find(|(token,_)|Some(*token)==ibis_doc_model::transition_catalog::effective_token(&t.effect,t.direction.as_deref())).or(options.first()).map(|(_,key)|text(key)).unwrap_or_default();
        body=body.child(chrome::dropdown_row(text("pptx.chrome.effect-options"),dropdown("pptx-sidebar-transition-options",current,super::chrome::transition_direction_items(editor),cx)));
    }
    let labels=[text("_shell.width"),text("_shell.height")];
    for (index,label) in labels.into_iter().enumerate() {
        let spec=SpinnerSpec::new(0.1).min(2.54);
        body=body.child(chrome::text_field_row(label,chrome::text_field(&fields[index]).unit("cm",px(24.)).spinner(SharedString::from(format!("pptx-sidebar-field-{index}")),spec,cx,|_,_,_,_|{})));
    }
    Some(SidebarPanel {title:text("_shell.slide"),body:body.into_any_element()})
}
/// The selected shape's pane: the shared 図形の書式 pane (the same as the document and sheet screens'), with this screen's fields.
fn shape_format(editor:&SlideEditor,fields:Vec<Entity<InputState>>,cx:&mut Context<SlideEditor>)->SidebarPanel {
    let (is_auto,has_text)=editor.selected_shapes().first().map(|s|(matches!(s.kind,ShapeKind::Auto(_)),s.text_body().is_some())).unwrap_or((false,false));
    let shared=pane::ShapeFormatFields {
        unit:pane::SizeUnit::Centimetre,
        width:fields[0].clone(),height:fields[1].clone(),rotation:Some(fields[2].clone()),left:Some(fields[3].clone()),top:Some(fields[4].clone()),
        fill:is_auto.then(||fields[5].clone()),line:is_auto.then(||fields[6].clone()),line_width:is_auto.then(||fields[8].clone()),
        font_size:has_text.then(||fields[9].clone()),text_color:has_text.then(||fields[7].clone()),
    };
    let hooks=pane::ShapeFormatHooks {
        set_tab:|e,index|{if let Some(st)=e.pptx_mut().and_then(|s|s.sidebar_settings.as_mut()).and_then(|s|s.effects.as_mut()) {st.editor.tab=index;}},
        dash_ui:|e|e.pptx_mut().and_then(|s|s.sidebar_settings.as_mut()).map(|s|&mut s.dash),
        apply:|e,w,cx|e.apply_sidebar_dash(w,cx),
    };
    let dash=editor.pptx().and_then(|s|s.sidebar_settings.as_ref()).map(|s|s.dash);
    let st=state_effects(editor);
    let spec=pane::ShapeFormatSpec {
        id:"pptx",plain_title_key:"_shell.format-options",
        tab:st.as_ref().map(|s|s.2).unwrap_or(0),
        fields:shared,hooks,dash:dash.filter(|_|is_auto),
        effects:st.as_ref().map(|s|pane::ShapeFormatEffects {editor:s.0,enabled:s.1,hooks:hooks_effects(),scroll:editor.sidebar_scroll.clone()}),
        lead:Vec::new(),after_place:Vec::new(),after_line:Vec::new(),tail:Vec::new(),
    };
    pane::shape_format_panel(spec,cx)
}
fn state_effects(editor:&SlideEditor)->Option<(&EffectEditor,[bool;SECTIONS],usize)> {
    let st=editor.pptx()?.sidebar_settings.as_ref()?.effects.as_ref()?;Some((&st.editor,st.enabled,st.editor.tab))
}
