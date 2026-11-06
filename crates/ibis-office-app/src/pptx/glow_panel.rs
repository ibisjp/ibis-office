// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Arbitrary shape glow with frozen targets and serialization preflight.
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Color, Command, Rgb, Shape, ShapeKind, SlideId};
use crate::shell::chrome::{Shown, ShownNumber, ShownText, SidebarPanel, Typed};
use crate::shell::editor::SlideEditor;

pub(crate) struct GlowPanel {
    pub(crate) fields: Vec<Entity<InputState>>,
    /// The radius as the model has it (EMU) and as `fields[0]` shows it: an Apply without an edit keeps the exact radius.
    /// Empty when the selected shapes differ: such a field is not written unless a value is put in.
    pub(crate) radius: ShownNumber,
    /// The colour code and the opacity of the selected shapes (`fields[1]`, `fields[2]`).
    pub(crate) color: EffectColor,
    /// The opacity slider (0 to 100 %); it writes the opacity field (`fields[2]`).
    pub(crate) alpha: Entity<gpui_kit::component::slider::SliderState>,
    _subscriptions: Vec<Subscription>,
    targets: Vec<(SlideId, Shape)>,
    error: Option<SharedString>,
}

/// The radius, the distance and the like (pt) are shown with one digit after the decimal point.
pub(super) fn pt_spec(max:f32)->crate::shell::chrome::SpinnerSpec {crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0,max).decimals(1)}

/// The glow of a shape without one (what the panel starts from).
const DEFAULT_GLOW:(Color,i64)=(Color::BLACK,63_500);

/// The colour code and the opacity of an effect for the selected shapes: each field shows the common value, or is empty.
pub(super) struct EffectColor {rgb:ShownText,opacity:ShownText}
impl EffectColor {
    pub(super) fn of(colors:&[Color])->Self {
        Self {rgb:ShownText::new(Shown::of(colors,|c|format!("{:06X}",c.rgb.0))),opacity:ShownText::new(Shown::of(colors,|c|format!("{:.8}",c.alpha as f64*100./255.)))}
    }
    /// The code field and the opacity field.
    pub(super) fn inputs(&self,window:&mut Window,cx:&mut App)->[Entity<InputState>;2] {[self.rgb.input(window,cx),self.opacity.input(window,cx)]}
    /// What the opacity shows (for the slider).
    pub(super) fn opacity(&self)->&Shown<String> {self.opacity.shown()}
    /// What the two fields mean: the code and the alpha to write, each `Untouched` while a mixed field is left empty.
    /// `None` when a text is not a value.
    fn resolve(&self,rgb_field:&Entity<InputState>,opacity_field:&Entity<InputState>,cx:&App)->Option<(Typed<Rgb>,Typed<u8>)> {
        let rgb=self.rgb.resolve(rgb_field.read(cx).value().as_ref()).and_then(|text|{
            let hex=text.trim_start_matches('#');
            (hex.len()==6 && hex.bytes().all(|c|c.is_ascii_hexdigit())).then(||u32::from_str_radix(hex,16).ok()).flatten().map(Rgb)
        });
        let alpha=self.opacity.resolve(opacity_field.read(cx).value().as_ref()).and_then(|text|{
            let opacity:f64=text.parse().ok()?;
            (opacity.is_finite() && (0. ..=100.).contains(&opacity)).then(||(opacity*255./100.).round() as u8)
        });
        (!rgb.is_invalid() && !alpha.is_invalid()).then_some((rgb,alpha))
    }
}

/// What the fields of a glow (radius, colour, opacity) ask for: each is written to a shape only when it holds a value.
pub(super) struct GlowEdit {radius:Typed<i64>,rgb:Typed<Rgb>,alpha:Typed<u8>}
impl GlowEdit {
    /// `None` when a field holds something that is not a value.
    pub(super) fn read(fields:&[Entity<InputState>],radius:&ShownNumber,color:&EffectColor,cx:&App)->Option<Self> {
        let radius=radius.resolve(fields[0].read(cx).value().as_ref()).and_then(|emu|(0. ..=i32::MAX as f64).contains(&emu).then_some(emu as i64));
        let (rgb,alpha)=color.resolve(&fields[1],&fields[2],cx)?;
        (!radius.is_invalid()).then_some(Self {radius,rgb,alpha})
    }
    /// Nothing to write (every field showed "mixed" and was left empty).
    pub(super) fn is_empty(&self)->bool {self.radius==Typed::Untouched && self.rgb==Typed::Untouched && self.alpha==Typed::Untouched}
    pub(super) fn color(&self,own:Color)->Color {Color {rgb:self.rgb.clone().or(own.rgb).unwrap_or(own.rgb),alpha:self.alpha.clone().or(own.alpha).unwrap_or(own.alpha)}}
    pub(super) fn radius(&self,own:i64)->i64 {self.radius.clone().or(own).unwrap_or(own)}
}

/// A field of the effect panels (glow, shadow, gradient, pattern): `None` is a colour code (RRGGBB, no ✕, 2026-10-23
/// ユーザー); a number has ▲▼ by `spec`. These panels apply with 適用, so ▲▼ only change the field.
pub(super) fn effect_field(input:&Entity<InputState>,id:String,spec:Option<crate::shell::chrome::SpinnerSpec>,unit:Option<&'static str>,cx:&Context<SlideEditor>)->crate::shell::chrome::TextField {
    let field=crate::shell::chrome::text_field(input).full_width();
    let field=match unit {Some(u)=>field.unit(u,px(20.)),None=>field};
    match spec {Some(spec)=>field.spinner(id,spec,cx,|_,_,_,_|{}),None=>field.no_clear()}
}
impl SlideEditor {
    pub(crate) fn open_glow_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_marked(cx);
        let targets: Vec<_> = self.selected_shapes().into_iter().map(|s| (self.current_slide, s.clone())).collect();
        if targets.is_empty() || targets.iter().any(|(_,s)| !matches!(s.kind, ShapeKind::Auto(_))) { return; }
        let glows:Vec<(Color,i64)>=targets.iter().filter_map(|(_,s)|match &s.kind {ShapeKind::Auto(auto)=>Some(auto.glow.unwrap_or(DEFAULT_GLOW)),_=>None}).collect();
        let colors:Vec<Color>=glows.iter().map(|g|g.0).collect();
        let radius = ShownNumber::new(Shown::of(&glows,|g|g.1),12_700.,pt_spec(150.));
        let color = EffectColor::of(&colors);
        let mut fields = vec![radius.input(window,cx)];
        fields.extend(color.inputs(window,cx));
        let first = fields[0].clone();
        let mut subscriptions=Vec::new();let alpha=super::gradient_panel::alpha_slider(&fields[2],window,cx,&mut subscriptions);
        if let Some(s)=self.pptx_mut() {
            s.section_panel=None;s.gradient_panel=None;s.shadow_panel=None;s.paragraph_panel=None;s.timing_panel=None;s.crop_panel=None;s.insets_panel=None;s.size_panel=None; s.frame_panel = None; s.comments = None; s.chart_panel = None; s.header_footer = None;s.find=None;
            s.glow_panel=Some(GlowPanel {fields,radius,color,alpha,_subscriptions:subscriptions,targets,error:None});
        }
        self.sidebar_open=true;first.update(cx,|f,cx|f.focus(window,cx));cx.notify();
    }

    pub(crate) fn apply_glow_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p)=self.pptx().and_then(|s|s.glow_panel.as_ref()) else {return};
        let valid=GlowEdit::read(&p.fields,&p.radius,&p.color,cx);
        let conflict=self.text_editing().is_some_and(|e|e.marked.is_some()) || p.targets.iter().any(|(slide,snapshot)| self.presentation().slide(*slide).and_then(|s|s.shape(snapshot.id))!=Some(snapshot));
        let error=if conflict {Some(crate::shell::i18n::text("pptx.glow-panel.target-shape-has-changed-reopen"))} else if valid.is_none() {Some(crate::shell::i18n::text("pptx.glow-panel.enter-radius-0-169093-20055118"))} else {None};
        if let Some(error)=error {
            if let Some(p)=self.pptx_mut().and_then(|s|s.glow_panel.as_mut()) {p.error=Some(error.into());}return cx.notify();
        }
        let edit=valid.unwrap();
        // Only a field that holds a value is written; the others stay as each shape has them.
        let commands=p.targets.iter().filter(|_|!edit.is_empty()).filter_map(|(slide,shape)| {
            let ShapeKind::Auto(auto)=&shape.kind else {return None};
            let (own_color,own_radius)=auto.glow.unwrap_or(DEFAULT_GLOW);
            let glow=(edit.color(own_color),edit.radius(own_radius));
            if auto.glow==Some(glow) {return None;}
            let mut auto=auto.clone();auto.glow=Some(glow);Some(Command::SetKind {slide:*slide,shape:shape.id,kind:Box::new(ShapeKind::Auto(auto))})
        }).collect();
        if self.apply_pptx_commands_checked(commands,cx) {self.close_glow_panel(window,cx);} else {
            if let Some(p)=self.pptx_mut().and_then(|s|s.glow_panel.as_mut()) {p.error=Some(crate::shell::i18n::text("pptx.glow-panel.glow-cannot-saved-selected-shape").into());}cx.notify();
        }
    }

    pub(crate) fn close_glow_panel(&mut self, window:&mut Window,cx:&mut Context<Self>) {
        if let Some(s)=self.pptx_mut() {s.glow_panel=None;}self.focus_handle.focus(window,cx);cx.notify();
    }

    pub(crate) fn glow_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel> {
        let p=self.pptx()?.glow_panel.as_ref()?;let fields=p.fields.clone();let error=p.error.clone();let title=crate::shell::i18n::format("_pptx.glow-count-shapes", &[("count", &p.targets.len())]);
        let mut body=v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        let alpha=p.alpha.clone();
        // Radius: a number row; colour: the shared colour field row; opacity: the shared slider row (all on one line each).
        body=body.child(crate::shell::chrome::text_field_row(crate::shell::i18n::text("pptx.glow-panel.radius-pt"),effect_field(&fields[0],"pptx-glow-field-0".into(),Some(pt_spec(150.)),Some("pt"),cx)))
            .child(crate::shell::chrome::color_text_field_row(crate::shell::i18n::text("pptx.gradient-panel.color-rgb-rrggbb"),crate::shell::chrome::panel_color_field("pptx-glow-field-1",&fields[1],cx)))
            .child(crate::shell::chrome::slider_row(crate::shell::i18n::text("pptx.gradient-panel.intensity-percent"),crate::shell::chrome::slider("pptx-glow-field-2",&alpha).shown(p.color.opacity()).value(cx).unit("%")));
        if let Some(error)=error {body=body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error));}
        let panel_actions = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("pptx-glow-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e,_,w,cx|e.apply_glow_panel(w,cx)))).action(crate::shell::chrome::ActionDefinition::new("pptx-glow-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e,_,w,cx|e.close_glow_panel(w,cx))));
        Some(SidebarPanel::with_footer(title, body.into_any_element(), panel_actions))
    }
}
