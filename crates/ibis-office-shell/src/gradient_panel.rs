// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Linear gradient stop editing with frozen shape/cell/background targets.
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::component::slider::{SliderEvent, SliderState};
use gpui_kit::*;
use crate::palette::rgb;
use gpui_kit::component::Disableable as _;
use ibis_doc_model::{Color,Command,Fill,GradientPath,GradientStop,Rgb,Shape,ShapeId,ShapeKind,SlideId};
use crate::chrome::SidebarPanel;
use crate::editor::SlideEditor;
use crate::format::with_format;

// UI choices from the existing model pattern_tile names; no new pattern data.
pub const PATTERN_CHOICES:&[(&str,&str,&str)]=&[
    ("pptx.pattern.pct5","pct5","5%"),
    ("pptx.pattern.pct10","pct10","10%"),
    ("pptx.pattern.pct20","pct20","20%"),
    ("pptx.pattern.pct25","pct25","25%"),
    ("pptx.pattern.pct30","pct30","30%"),
    ("pptx.pattern.pct40","pct40","40%"),
    ("pptx.pattern.pct50","pct50","50%"),
    ("pptx.pattern.pct60","pct60","60%"),
    ("pptx.pattern.pct70","pct70","70%"),
    ("pptx.pattern.pct75","pct75","75%"),
    ("pptx.pattern.pct80","pct80","80%"),
    ("pptx.pattern.pct90","pct90","90%"),
    ("pptx.pattern.horz","horz","pptx.gradient-panel.horizontal-lines"),
    ("pptx.pattern.vert","vert","pptx.gradient-panel.vertical-lines"),
    ("pptx.pattern.ltHorz","ltHorz","pptx.gradient-panel.light-horizontal-lines"),
    ("pptx.pattern.ltVert","ltVert","pptx.gradient-panel.light-vertical-lines"),
    ("pptx.pattern.dkHorz","dkHorz","pptx.gradient-panel.dark-horizontal-lines"),
    ("pptx.pattern.dkVert","dkVert","pptx.gradient-panel.dark-vertical-lines"),
    ("pptx.pattern.narHorz","narHorz","pptx.gradient-panel.narrow-horizontal-lines"),
    ("pptx.pattern.narVert","narVert","pptx.gradient-panel.narrow-vertical-lines"),
    ("pptx.pattern.dashHorz","dashHorz","pptx.gradient-panel.dashed-horizontal-lines"),
    ("pptx.pattern.dashVert","dashVert","pptx.gradient-panel.dashed-vertical-lines"),
    ("pptx.pattern.cross","cross","pptx.gradient-panel.cross"),
    ("pptx.pattern.lgGrid","lgGrid","pptx.gradient-panel.large-grid"),
    ("pptx.pattern.smGrid","smGrid","pptx.gradient-panel.small-grid"),
    ("pptx.pattern.dotGrid","dotGrid","pptx.gradient-panel.dotted-grid"),
    ("pptx.pattern.dnDiag","dnDiag","pptx.gradient-panel.downward-diagonal"),
    ("pptx.pattern.upDiag","upDiag","pptx.gradient-panel.upward-diagonal"),
    ("pptx.pattern.ltDnDiag","ltDnDiag","pptx.gradient-panel.light-downward-diagonal"),
    ("pptx.pattern.ltUpDiag","ltUpDiag","pptx.gradient-panel.light-upward-diagonal"),
    ("pptx.pattern.dkDnDiag","dkDnDiag","pptx.gradient-panel.dark-downward-diagonal"),
    ("pptx.pattern.dkUpDiag","dkUpDiag","pptx.gradient-panel.dark-upward-diagonal"),
    ("pptx.pattern.wdDnDiag","wdDnDiag","pptx.gradient-panel.wide-downward-diagonal"),
    ("pptx.pattern.wdUpDiag","wdUpDiag","pptx.gradient-panel.wide-upward-diagonal"),
    ("pptx.pattern.dashDnDiag","dashDnDiag","pptx.gradient-panel.dashed-downward-diagonal"),
    ("pptx.pattern.dashUpDiag","dashUpDiag","pptx.gradient-panel.dashed-upward-diagonal"),
    ("pptx.pattern.diagCross","diagCross","pptx.gradient-panel.diagonal-cross"),
    ("pptx.pattern.smCheck","smCheck","pptx.gradient-panel.small-checker-board"),
    ("pptx.pattern.lgCheck","lgCheck","pptx.gradient-panel.large-checker-board"),
    ("pptx.pattern.smConfetti","smConfetti","pptx.gradient-panel.small-confetti"),
    ("pptx.pattern.lgConfetti","lgConfetti","pptx.gradient-panel.large-confetti"),
    ("pptx.pattern.horzBrick","horzBrick","pptx.gradient-panel.horizontal-brick"),
    ("pptx.pattern.diagBrick","diagBrick","pptx.gradient-panel.diagonal-brick"),
    ("pptx.pattern.solidDmnd","solidDmnd","pptx.gradient-panel.solid-diamond"),
    ("pptx.pattern.openDmnd","openDmnd","pptx.gradient-panel.outlined-diamond"),
    ("pptx.pattern.dotDmnd","dotDmnd","pptx.gradient-panel.dotted-diamond"),
    ("pptx.pattern.plaid","plaid","pptx.gradient-panel.plaid"),
    ("pptx.pattern.sphere","sphere","pptx.gradient-panel.sphere"),
    ("pptx.pattern.weave","weave","pptx.gradient-panel.weave"),
    ("pptx.pattern.divot","divot","pptx.gradient-panel.divot"),
    ("pptx.pattern.shingle","shingle","pptx.gradient-panel.shingle"),
    ("pptx.pattern.wave","wave","_shell.wave"),
    ("pptx.pattern.trellis","trellis","pptx.gradient-panel.trellis"),
    ("pptx.pattern.zigZag","zigZag","pptx.gradient-panel.zig-zag"),
];

/// The pattern type dropdown: its width in dropdown widths, and the width of a pattern's picture in its items.
const PATTERN_DROPDOWN_SCALE:f32=1.5;
const PATTERN_PICTURE_WIDTH:f32=48.;
type Row=[Entity<InputState>;3];
type Cells=(ShapeId,usize,usize,usize,usize);
enum Target { Shapes {slide:SlideId,shapes:Vec<Shape>,cells:Option<Cells>}, Background {slide:SlideId,fill:Fill} }
pub struct GradientPanel {
    pub angle:Entity<InputState>,
    pub stops:Vec<Row>,
    pub path:Option<GradientPath>,
    pub pattern:Option<String>,
    pub focus:[Entity<InputState>;4],
    slider: crate::chrome::gradient_slider::GradientSliderState,
    /// The opacity slider of each stop (the same index as `stops`); it writes the stop's opacity field.
    pub(crate) alphas:Vec<Entity<SliderState>>,
    _subscriptions:Vec<Subscription>,
    selected_stop: usize,
    target:Target,
    error:Option<SharedString>,
}
/// The opacity slider (0 to 100 %) of a row: a drag (or a typed number) writes the row's opacity field.
pub(super) fn alpha_slider(field:&Entity<InputState>,window:&mut Window,cx:&mut Context<SlideEditor>,subscriptions:&mut Vec<Subscription>)->Entity<SliderState> {
    // An empty field (a mixed opacity of several shapes) starts the slider at its minimum; `ValueSlider::mixed` draws it blank.
    let text=field.read(cx).value();let percent=if text.trim().is_empty() {0.} else {text.trim().parse::<f32>().unwrap_or(100.).clamp(0.,100.)};
    let slider=cx.new(|_|SliderState::new().min(0.).max(100.).step(1.).default_value(percent));
    let field=field.clone();
    subscriptions.push(cx.subscribe_in(&slider,window,move|_,_,event:&SliderEvent,w,cx|{
        let SliderEvent::Change(value)=event else {return};
        let text=format!("{:.8}",value.end());
        if field.read(cx).value().as_str()!=text {field.update(cx,|f,cx|f.set_value(text,w,cx));}
        cx.notify();
    }));
    slider
}
/// An id that lives as long as the app, for the parts that take `&'static str` ids (one per prefix and index).
fn static_id(prefix:&str,index:usize)->&'static str {
    thread_local!{static IDS:std::cell::RefCell<std::collections::HashMap<(String,usize),&'static str>>=Default::default();}
    IDS.with(|ids|*ids.borrow_mut().entry((prefix.into(),index)).or_insert_with(||Box::leak(format!("{prefix}-{index}").into_boxed_str())))
}
/// A colour from a code field and an opacity field (the gradient panel's stops; they show one value each, no mixed state).
fn color(rgb_field:&Entity<InputState>,opacity_field:&Entity<InputState>,cx:&App)->Option<Color> {
    let text=rgb_field.read(cx).value();let hex=text.trim().trim_start_matches('#');
    if hex.len()!=6 || !hex.bytes().all(|c|c.is_ascii_hexdigit()) {return None;}
    let rgb=Rgb(u32::from_str_radix(hex,16).ok()?);let opacity:f64=opacity_field.read(cx).value().trim().parse().ok()?;
    if !opacity.is_finite() || !(0. ..=100.).contains(&opacity) {return None;}
    Some(Color {rgb,alpha:(opacity*255./100.).round() as u8})
}

fn row(stop:GradientStop,window:&mut Window,cx:&mut Context<SlideEditor>)->Row {
    [format!("{:.3}",stop.position as f64/1000.),format!("{:06X}",stop.color.rgb.0),format!("{:.8}",stop.color.alpha as f64*100./255.)].map(|v|cx.new(|cx|InputState::new(window,cx).default_value(v)))
}
fn value(panel:&GradientPanel,cx:&App)->Option<Fill> {
    if let Some(preset)=&panel.pattern {
        if panel.stops.len()!=2 || ibis_doc_model::pattern_tile(preset).is_none() {return None;}
        let foreground=color(&panel.stops[0][1],&panel.stops[0][2],cx)?;
        let background=color(&panel.stops[1][1],&panel.stops[1][2],cx)?;
        return Some(Fill::Pattern {preset:preset.clone(),foreground,background});
    }
    let angle:f64=if panel.path.is_some() {0.} else {panel.angle.read(cx).value().trim().parse().ok()?};let units=(angle*60_000.).round();
    if !angle.is_finite() || angle<0. || !(0. ..21_600_000.).contains(&units) || panel.stops.len()<2 {return None;}
    let mut stops=Vec::new();
    for fields in &panel.stops {
        let position:f64=fields[0].read(cx).value().trim().parse().ok()?;
        if !position.is_finite() || !(0. ..=100.).contains(&position) {return None;}
        let color=color(&fields[1],&fields[2],cx)?;
        stops.push(GradientStop {position:(position*1000.).round() as u32,color});
    }
    // Sorting moves each colour together with its position; ties stay in input order.
    stops.sort_by_key(|s|s.position);
    if let Some(path)=panel.path {
        let mut focus=[0;4];
        for (field,value) in panel.focus.iter().zip(&mut focus) {
            let percent:f64=field.read(cx).value().trim().parse().ok()?;let units=(percent*1000.).round();
            if !percent.is_finite() || !(i32::MIN as f64..=i32::MAX as f64).contains(&units) {return None;}
            *value=units as i32;
        }
        Some(Fill::Path {path,focus,stops})
    } else {Some(Fill::Linear {angle:units as i32,scaled:false,stops})}
}
impl SlideEditor {
    pub fn open_gradient_panel(&mut self,background:bool,window:&mut Window,cx:&mut Context<Self>) {
        self.open_gradient_panel_mode(background,false,false,window,cx);
    }
    pub fn open_path_gradient_panel(&mut self,background:bool,window:&mut Window,cx:&mut Context<Self>) {
        self.open_gradient_panel_mode(background,true,false,window,cx);
    }
    pub fn open_pattern_panel(&mut self,background:bool,window:&mut Window,cx:&mut Context<Self>) {
        self.open_gradient_panel_mode(background,false,true,window,cx);
    }
    fn open_gradient_panel_mode(&mut self,background:bool,path_mode:bool,pattern_mode:bool,window:&mut Window,cx:&mut Context<Self>) {
        self.commit_marked(cx);
        self.drop_untouched_body();
        let (target,fill)=if background {
            let fill=self.slide().background.clone();(Target::Background {slide:self.current_slide,fill:fill.clone()},fill)
        } else {
            let shapes:Vec<_>=self.selected_shapes().into_iter().cloned().collect();
            if shapes.is_empty() || shapes.iter().any(|s|!matches!(s.kind,ShapeKind::Auto(_)|ShapeKind::Table(_))) {return;}
            let cells=self.text_editing().and_then(|e|e.cell.map(|at| {let other=e.cell_anchor.unwrap_or(at);(e.shape,at.0.min(other.0),at.0.max(other.0),at.1.min(other.1),at.1.max(other.1))}));
            let fill=match &shapes[0].kind {
                ShapeKind::Auto(a)=>a.fill.clone(),
                ShapeKind::Table(t)=>{let (r,c)=cells.filter(|v|v.0==shapes[0].id).map(|v|(v.1,v.3)).unwrap_or((0,0));let Some(cell)=t.rows.get(r).and_then(|r|r.cells.get(c)) else {return};cell.fill.clone()},
                _=>return,
            };
            (Target::Shapes {slide:self.current_slide,shapes,cells},fill)
        };
        let pattern=pattern_mode.then(||if let Fill::Pattern {preset,..}=&fill {preset.clone()} else {"pct5".into()});
        let (angle,mut stops,path_kind,focus)=match fill {
            Fill::Pattern {foreground,background,..}=>(0,vec![GradientStop {position:0,color:foreground},GradientStop {position:100_000,color:background}],GradientPath::Circle,[50_000;4]),
            Fill::Linear {angle,stops,..} if stops.len()>=2=>(angle,stops,GradientPath::Circle,[50_000;4]),
            Fill::Path {path,focus,stops} if stops.len()>=2=>(0,stops,path,focus),
            other=>{
            let start=if let Fill::Solid(color)=other {color} else {Color::BLACK};(0,vec![GradientStop {position:0,color:start},GradientStop {position:100_000,color:Color::WHITE}],GradientPath::Circle,[50_000;4])
        }};
        if pattern_mode {stops=vec![GradientStop {position:0,color:stops[0].color},GradientStop {position:100_000,color:stops.last().unwrap().color}];}
        let angle=cx.new(|cx|InputState::new(window,cx).default_value(format!("{:.8}",angle as f64/60_000.)));
        let stops:Vec<Row>=stops.into_iter().map(|s|row(s,window,cx)).collect();
        let mut subscriptions=Vec::new();
        let alphas:Vec<_>=stops.iter().map(|fields|alpha_slider(&fields[2],window,cx,&mut subscriptions)).collect();
        let focus=focus.map(|v|cx.new(|cx|InputState::new(window,cx).default_value(format!("{:.3}",v as f64/1000.))));
        let path=path_mode.then_some(path_kind);let first=if pattern_mode {stops[0][1].clone()} else if path_mode {focus[0].clone()} else {angle.clone()};
        self.shape_edit.glow_panel=None; self.shape_edit.shadow_panel=None; self.shape_edit.paragraph_panel=None; self.shape_edit.insets_panel=None; self.shape_edit.size_panel=None;
        self.shape_edit.crop_panel=None; self.shape_edit.frame_panel = None; self.shape_edit.header_footer = None; self.shape_edit.find=None;
        if self.uses_shape_edit() { with_format!(self, panel_opening(self, crate::shape_state::ShapePanel::Gradient)); self.shape_edit.chart_panel = None; self.shape_edit.gradient_panel=Some(GradientPanel {angle,stops,path,pattern,focus,target,error:None,slider:Default::default(),selected_stop:0,alphas,_subscriptions:subscriptions}); }
        self.sidebar_open=true;first.update(cx,|f,cx|f.focus(window,cx));cx.notify();
    }
    pub fn set_pattern_preset(&mut self,preset:&str,cx:&mut Context<Self>) {
        if ibis_doc_model::pattern_tile(preset).is_none() {return;}
        if let Some(p)=self.shape_edit.gradient_panel.as_mut() {if p.pattern.is_some() {p.pattern=Some(preset.into());p.error=None;cx.notify();}}
    }
    pub fn set_gradient_path(&mut self,path:GradientPath,cx:&mut Context<Self>) {
        if let Some(p)=self.shape_edit.gradient_panel.as_mut() {if p.path.is_some() {p.path=Some(path);p.error=None;cx.notify();}}
    }
    pub fn add_gradient_stop(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        if self.shape_edit.gradient_panel.as_ref().is_none_or(|p|p.pattern.is_some()) {return;}
        let fields=row(GradientStop {position:50_000,color:Color::WHITE},window,cx);let focus=fields[0].clone();
        let mut subscriptions=Vec::new();let alpha=alpha_slider(&fields[2],window,cx,&mut subscriptions);
        if let Some(p)=self.shape_edit.gradient_panel.as_mut() {p.stops.push(fields);p.alphas.push(alpha);p._subscriptions.extend(subscriptions);}
        focus.update(cx,|f,cx|f.focus(window,cx));cx.notify();
    }
    pub fn remove_gradient_stop(&mut self,index:usize,window:&mut Window,cx:&mut Context<Self>) {
        let Some(p)=self.shape_edit.gradient_panel.as_mut() else {return};
        if p.pattern.is_some() || p.stops.len()<=2 || index>=p.stops.len() {return;}
        p.stops.remove(index);if index<p.alphas.len() {p.alphas.remove(index);}p.selected_stop=p.selected_stop.min(p.stops.len().saturating_sub(1));let focus=p.angle.clone();focus.update(cx,|f,cx|f.focus(window,cx));cx.notify();
    }
    pub fn apply_gradient_panel(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        let Some(p)=self.shape_edit.gradient_panel.as_ref() else {return};let fill=value(p,cx);
        let conflict=self.text_editing().is_some_and(|e|e.marked.is_some()) || match &p.target {
            Target::Shapes {slide,shapes,..}=>shapes.iter().any(|snapshot|self.presentation().slide(*slide).and_then(|s|s.shape(snapshot.id))!=Some(snapshot)),
            Target::Background {slide,fill}=>self.presentation().slide(*slide).map(|s|&s.background)!=Some(fill),
        };
        let error=if conflict {Some(crate::i18n::text("pptx.gradient-panel.target-has-changed-reopen-settings"))} else if fill.is_none() {Some(if p.pattern.is_some() {crate::i18n::text("pptx.gradient-panel.enter-6-digit-rgb-color")} else if p.path.is_some() {crate::i18n::text("pptx.gradient-panel.enter-position-intensity-0-100")} else {crate::i18n::text("pptx.gradient-panel.enter-angle-0-more-less")})} else {None};
        if let Some(error)=error {if let Some(p)=self.shape_edit.gradient_panel.as_mut() {p.error=Some(error.into());}return cx.notify();}
        let fill=fill.unwrap();let commands=match &p.target {
            Target::Background {slide,fill:old}=>if *old==fill {Vec::new()} else {vec![Command::SetBackground {slide:*slide,fill}]},
            Target::Shapes {slide,shapes,cells}=>shapes.iter().filter_map(|shape|match &shape.kind {
                ShapeKind::Auto(a)=>if a.fill==fill {None} else {Some(Command::SetFill {slide:*slide,shape:shape.id,fill:fill.clone()})},
                ShapeKind::Table(t)=>{let mut table=(**t).clone();for (r,row) in table.rows.iter_mut().enumerate() {for (c,cell) in row.cells.iter_mut().enumerate() {if cells.is_none_or(|(id,r0,r1,c0,c1)|id!=shape.id || (r0..=r1).contains(&r)&&(c0..=c1).contains(&c)) {cell.fill=fill.clone();}}}if table==**t {None} else {Some(Command::SetKind {slide:*slide,shape:shape.id,kind:Box::new(ShapeKind::Table(Box::new(table)))})}},
                _=>None,
            }).collect(),
        };
        if self.apply_pptx_commands_checked(commands,cx) {self.close_gradient_panel(window,cx);} else {if let Some(p)=self.shape_edit.gradient_panel.as_mut() {p.error=Some(crate::i18n::text("pptx.gradient-panel.fill-cannot-saved-selected-target").into());}cx.notify();}
    }
    pub fn close_gradient_panel(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        if self.uses_shape_edit() { with_format!(self, panel_closed(self, crate::shape_state::ShapePanel::Gradient)); self.shape_edit.gradient_panel=None; }self.focus_handle.focus(window,cx);cx.notify();
    }
    pub fn pptx_gradient_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel> {
        let p=self.shape_edit.gradient_panel.as_ref()?;let angle=p.angle.clone();let stops=p.stops.clone();let p_alphas=p.alphas.clone();let error=p.error.clone();let path=p.path;let pattern=p.pattern.clone();let focus=p.focus.clone();let kind=if pattern.is_some() {crate::i18n::text("pptx.chrome.pattern")} else if path.is_some() {crate::i18n::text("pptx.gradient-panel.path-gradient")} else {crate::i18n::text("pptx.gradient-panel.linear-gradient")};let title=match &p.target {Target::Background {..}=>crate::i18n::format("_pptx.kind-background", &[("kind", &kind)]),Target::Shapes {shapes,..}=>crate::i18n::format("_pptx.kind-count-shapes", &[("kind", &kind), ("count", &shapes.len())])};
        let mut body=v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        let stops_rgb:Vec<String>=stops.iter().map(|f|f[1].read(cx).value().to_string()).collect();
        if pattern.is_none() {
            let points = stops.iter().filter_map(|fields| {
                let position = fields[0].read(cx).value().trim().parse::<f64>().ok()?;
                if !position.is_finite() || !(0. ..=100.).contains(&position) {return None;}
                let color = color(&fields[1], &fields[2], cx)?;
                Some(GradientStop {position: (position * 1000.).round() as u32, color})
            }).collect::<Vec<_>>();
            if points.len() == stops.len() {
                body = body.child(crate::chrome::gradient_slider::gradient_slider(points, p.selected_stop, true, p.slider.clone(), cx, |e, points, selected, w, cx| {
                    let old = e.shape_edit.gradient_panel.as_ref().map(|p| p.stops.clone()).unwrap_or_default();
                    let fields = points.into_iter().enumerate().map(|(i, point)| {
                        if let Some(fields) = old.get(i) {
                            let values = [format!("{:.3}", point.position as f64 / 1000.), format!("{:06X}", point.color.rgb.0), format!("{:.8}", point.color.alpha as f64 * 100. / 255.)];
                            for (field, value) in fields.iter().zip(values) {if field.read(cx).value().as_str() != value {field.update(cx, |f, cx| f.set_value(value, w, cx));}}
                            fields.clone()
                        } else {row(point, w, cx)}
                    }).collect::<Vec<Row>>();
                    let mut subscriptions = Vec::new();
                    let alphas = fields.iter().enumerate().map(|(i, fields)| match e.shape_edit.gradient_panel.as_ref().and_then(|p| p.alphas.get(i)) {
                        Some(slider) => slider.clone(),
                        None => alpha_slider(&fields[2], w, cx, &mut subscriptions),
                    }).collect();
                    if let Some(p) = e.shape_edit.gradient_panel.as_mut() {p.stops = fields;p.alphas = alphas;p._subscriptions.extend(subscriptions);p.selected_stop = selected;}
                    cx.notify();
                }));
            }
        }
        if pattern.is_some() {
            // The type is chosen with the dropdown below the opacities.
        } else if let Some(selected)=path {
            // Circle, square, shape: one choice of three, a segment control.
            let items=[("pptx-gradient-circle","pptx.common.circle",GradientPath::Circle),("pptx-gradient-rect","pptx.chrome.square",GradientPath::Rect),("pptx-gradient-shape","_shell.shape",GradientPath::Shape)].into_iter()
                .map(|(id,label,path)|crate::chrome::segment_control::SegmentItem::label(id,crate::i18n::text(label),path==selected).on_click(cx.listener(move|e,_,_,cx|e.set_gradient_path(path,cx))));
            body=body.child(crate::chrome::segment_control_row(crate::i18n::text("pptx.gradient-panel.path-shape"),items));
            for (i,(label,field)) in [crate::i18n::text("pptx.gradient-panel.focus-left-percent"),crate::i18n::text("pptx.gradient-panel.focus-top-percent"),crate::i18n::text("pptx.gradient-panel.focus-right-percent"),crate::i18n::text("pptx.gradient-panel.focus-bottom-percent")].into_iter().zip(focus).enumerate() {
                body=body.child(crate::chrome::text_field_row(label,crate::glow_panel::effect_field(&field,format!("pptx-gradient-focus-{i}"),Some(crate::chrome::SpinnerSpec::new(1.0).range(0.0,100.0)),None,cx).unit("%",px(24.))));
            }
        } else {
            body=body.child(crate::chrome::text_field_row(crate::i18n::text("pptx.gradient-panel.angle-degrees"),crate::glow_panel::effect_field(&angle,"pptx-gradient-angle".into(),Some(crate::chrome::SpinnerSpec::new(1.0).range(0.0,359.0)),None,cx).unit("°",px(24.))));
        }
        let can_remove=stops.len()>2;
        let alphas=p_alphas;
        // A colour is chosen with the shared colour field (swatch dropdown + code), an opacity with the shared slider (value at its left).
        let colour_row=|id:&'static str,input:&Entity<InputState>,cx:&Context<SlideEditor>|crate::chrome::color_row(crate::i18n::text("pptx.gradient-panel.color-rgb-rrggbb"),crate::chrome::panel_color_field(id,input,cx));
        let opacity_row=|id:&'static str,slider:Option<&Entity<SliderState>>,cx:&Context<SlideEditor>|slider.map(|s|crate::chrome::slider_row(crate::i18n::text("pptx.gradient-panel.intensity-percent"),crate::chrome::slider(id,s).value(cx).unit("%")));
        if pattern.is_some() {
            for (side,(label,fields)) in [crate::i18n::text("pptx.gradient-panel.foreground"),crate::i18n::text("_shell.background")].into_iter().zip(stops).enumerate() {
                body=body.child(div().child(label))
                    .child(colour_row(static_id("pptx-pattern-color",side),&fields[1],cx))
                    .children(opacity_row(static_id("pptx-pattern-opacity",side),alphas.get(side),cx));
            }
        } else {
        for (i,fields) in stops.into_iter().enumerate() {
            body=body.child(h_flex().justify_between().child(div().child(crate::i18n::format("pptx.gradient-panel.stop-n",&[("n",&(i+1))]))).child(crate::chrome::destructive_button(ElementId::Name(format!("pptx-gradient-remove-{i}").into()),cx).small().label(crate::i18n::text("_shell.delete")).disabled(!can_remove).on_click(cx.listener(move|e,_,w,cx|e.remove_gradient_stop(i,w,cx)))))
                .child(crate::chrome::text_field_row(crate::i18n::text("pptx.gradient-panel.position-percent"),crate::glow_panel::effect_field(&fields[0],format!("pptx-gradient-stop-{i}-0"),Some(crate::chrome::SpinnerSpec::new(1.0).range(0.0,100.0)),None,cx).unit("%",px(24.))))
                .child(colour_row(static_id("pptx-gradient-stop-color",i),&fields[1],cx))
                .children(opacity_row(static_id("pptx-gradient-stop-opacity",i),alphas.get(i),cx));
        }
        body=body.child(crate::chrome::action_button("pptx-gradient-add").small().label(crate::i18n::text("pptx.chrome.add-stop")).on_click(cx.listener(|e,_,w,cx|e.add_gradient_stop(w,cx))));
        }
        if let Some(error)=error {body=body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error));}
        if let Some(selected)=&pattern {
            // The type: a dropdown of pictures (the shared pattern picture of the xlsx cell format), drawn in the colours chosen above.
            let colour=|row:usize,fallback:u32|ibis_doc_model::Rgb(u32::from_str_radix(stops_rgb[row].trim().trim_start_matches('#'),16).unwrap_or(fallback));
            let (fg,bg)=(colour(0,0),colour(1,0xFFFFFF));
            let choices=PATTERN_CHOICES.iter().map(|&(_,preset,label)|{
                let label=crate::i18n::text(label);let name=label.clone();
                crate::chrome::preview_dropdown::PreviewChoice {
                    label:label.clone(),
                    content:std::rc::Rc::new(move||h_flex().w_full().items_center().gap(px(8.)).child(div().w(px(PATTERN_PICTURE_WIDTH)).flex_none().flex().child(crate::chrome::preview_dropdown::pattern_preview(ibis_doc_model::pattern_tile(preset),fg,bg,name.clone()))).child(div().flex_1().min_w_0().truncate().text_size(px(13.)).child(name.clone())).into_any_element()),
                    choose:std::rc::Rc::new(move|e:&mut SlideEditor,_,cx|e.set_pattern_preset(preset,cx)),
                }
            }).collect();
            let index=PATTERN_CHOICES.iter().position(|c|c.1==selected).unwrap_or(0);
            let size=crate::chrome::preview_dropdown::PreviewSize::default().width(crate::chrome::FIELD_WIDTH*PATTERN_DROPDOWN_SCALE);
            body=body.child(crate::chrome::dropdown_row_scaled(crate::i18n::text("pptx.gradient-panel.select-type"),crate::chrome::preview_dropdown::preview_dropdown_sized("pptx-pattern-type",choices,index,size,cx),PATTERN_DROPDOWN_SCALE));
        }
        let footer = crate::chrome::action_bar().action(crate::chrome::ActionDefinition::new("pptx-gradient-apply", crate::i18n::text("_shell.apply"), crate::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e,_,w,cx|e.apply_gradient_panel(w,cx)))).action(crate::chrome::ActionDefinition::new("pptx-gradient-cancel", crate::i18n::text("_shell.cancel"), crate::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e,_,w,cx|e.close_gradient_panel(w,cx))));
        Some(SidebarPanel::with_footer(title, body.into_any_element(), footer))
    }
}
