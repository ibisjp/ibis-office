// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Arbitrary shape shadow with frozen targets and serialization preflight.
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Color, Command, Shape, ShapeKind, SlideId, Shadow};
use crate::shell::chrome::{Shown, ShownNumber, SidebarPanel, Typed};
use super::glow_panel::{EffectColor, GlowEdit};
use crate::shell::editor::SlideEditor;

pub(crate) struct ShadowPanel {
    pub(crate) fields: Vec<Entity<InputState>>,
    /// The blur, the distance (EMU) and the angle (60 000ths of a degree) as the model has them and as `fields[0]`, `[3]`, `[4]` show them.
    /// A field is empty when the selected shapes differ, and then is not written unless a value is put in.
    shown: [ShownNumber; 3],
    /// The colour code and the opacity (`fields[1]`, `fields[2]`).
    color: EffectColor,
    /// The opacity slider (0 to 100 %); it writes the opacity field (`fields[2]`).
    pub(crate) alpha: Entity<gpui_kit::component::slider::SliderState>,
    _subscriptions: Vec<Subscription>,
    targets: Vec<(SlideId, Shape)>,
    error: Option<SharedString>,
}

/// The angle (degrees) is shown with one digit after the decimal point.
fn angle_spec()->crate::shell::chrome::SpinnerSpec {crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0,359.0).decimals(1)}

/// What the fields of a shadow ask for: each is written to a shape only when it holds a value.
struct ShadowEdit {glow:GlowEdit,distance:Typed<i64>,direction:Typed<i32>}
impl ShadowEdit {
    fn read(fields: &[Entity<InputState>], shown: &[ShownNumber; 3], color: &EffectColor, cx: &App) -> Option<Self> {
        let glow=GlowEdit::read(fields,&shown[0],color,cx)?;
        let distance=shown[1].resolve(fields[3].read(cx).value().as_ref()).and_then(|emu|(0. ..=i32::MAX as f64).contains(&emu).then_some(emu as i64));
        let direction=shown[2].resolve(fields[4].read(cx).value().as_ref()).and_then(|d|(0. ..21_600_000.).contains(&d).then_some(d as i32));
        (!distance.is_invalid() && !direction.is_invalid()).then_some(Self {glow,distance,direction})
    }
    fn is_empty(&self)->bool {self.glow.is_empty() && self.distance==Typed::Untouched && self.direction==Typed::Untouched}
    /// The shadow of a shape after the edit: the fields that hold a value replace the shape's own, the rest stays.
    fn on(&self,own:Shadow)->Shadow {
        Shadow {color:self.glow.color(own.color),blur:self.glow.radius(own.blur),distance:self.distance.clone().or(own.distance).unwrap_or(own.distance),direction:self.direction.clone().or(own.direction).unwrap_or(own.direction),..own}
    }
}

/// The shadow of a shape without one (what the panel starts from).
fn default_shadow()->Shadow {Shadow::simple(Color::BLACK,50_800,50_800,0)}

impl SlideEditor {
    pub(crate) fn open_shadow_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_marked(cx);
        let targets: Vec<_> = self.selected_shapes().into_iter().map(|s| (self.current_slide, s.clone())).collect();
        if targets.is_empty() || targets.iter().any(|(_,s)| !matches!(s.kind, ShapeKind::Auto(_))) { return; }
        let shadows:Vec<Shadow>=targets.iter().filter_map(|(_,s)|match &s.kind {ShapeKind::Auto(auto)=>Some(auto.shadow.unwrap_or_else(default_shadow)),_=>None}).collect();
        let colors:Vec<Color>=shadows.iter().map(|s|s.color).collect();
        let shown=[ShownNumber::new(Shown::of(&shadows,|s|s.blur),12_700.,super::glow_panel::pt_spec(100.)),ShownNumber::new(Shown::of(&shadows,|s|s.distance),12_700.,super::glow_panel::pt_spec(200.)),ShownNumber::new(Shown::of(&shadows,|s|s.direction as i64),60_000.,angle_spec())];
        let color=EffectColor::of(&colors);
        let mut fields=vec![shown[0].input(window,cx)];
        fields.extend(color.inputs(window,cx));
        fields.push(shown[1].input(window,cx));fields.push(shown[2].input(window,cx));
        let first = fields[0].clone();
        let mut subscriptions=Vec::new();let alpha=super::gradient_panel::alpha_slider(&fields[2],window,cx,&mut subscriptions);
        if let Some(s)=self.pptx_mut() {
            s.section_panel=None;s.gradient_panel=None;s.glow_panel=None;s.paragraph_panel=None;s.timing_panel=None;s.crop_panel=None;s.insets_panel=None;s.size_panel=None; s.frame_panel = None; s.comments = None; s.chart_panel = None; s.header_footer = None;s.find=None;
            s.shadow_panel=Some(ShadowPanel {fields,shown,color,alpha,_subscriptions:subscriptions,targets,error:None});
        }
        self.sidebar_open=true;first.update(cx,|f,cx|f.focus(window,cx));cx.notify();
    }

    pub(crate) fn apply_shadow_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p)=self.pptx().and_then(|s|s.shadow_panel.as_ref()) else {return};
        let valid=ShadowEdit::read(&p.fields,&p.shown,&p.color,cx);
        let conflict=self.text_editing().is_some_and(|e|e.marked.is_some()) || p.targets.iter().any(|(slide,snapshot)| self.presentation().slide(*slide).and_then(|s|s.shape(snapshot.id))!=Some(snapshot));
        let error=if conflict {Some(crate::shell::i18n::text("pptx.glow-panel.target-shape-has-changed-reopen"))} else if valid.is_none() {Some(crate::shell::i18n::text("pptx.shadow-panel.enter-blur-distance-0-169093"))} else {None};
        if let Some(error)=error {
            if let Some(p)=self.pptx_mut().and_then(|s|s.shadow_panel.as_mut()) {p.error=Some(error.into());}return cx.notify();
        }
        let edit=valid.unwrap();
        // Only a field that holds a value is written; the others stay as each shape has them.
        let commands=p.targets.iter().filter(|_|!edit.is_empty()).filter_map(|(slide,shape)| {
            let ShapeKind::Auto(auto)=&shape.kind else {return None};
            let shadow=edit.on(auto.shadow.unwrap_or_else(default_shadow));
            if auto.shadow==Some(shadow) {return None;}
            let mut auto=auto.clone();auto.shadow=Some(shadow);Some(Command::SetKind {slide:*slide,shape:shape.id,kind:Box::new(ShapeKind::Auto(auto))})
        }).collect();
        if self.apply_pptx_commands_checked(commands,cx) {self.close_shadow_panel(window,cx);} else {
            if let Some(p)=self.pptx_mut().and_then(|s|s.shadow_panel.as_mut()) {p.error=Some(crate::shell::i18n::text("pptx.shadow-panel.outer-shadow-cannot-saved-selected").into());}cx.notify();
        }
    }

    pub(crate) fn close_shadow_panel(&mut self, window:&mut Window,cx:&mut Context<Self>) {
        if let Some(s)=self.pptx_mut() {s.shadow_panel=None;}self.focus_handle.focus(window,cx);cx.notify();
    }

    pub(crate) fn shadow_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel> {
        let p=self.pptx()?.shadow_panel.as_ref()?;let fields=p.fields.clone();let error=p.error.clone();let title=crate::shell::i18n::format("_pptx.outer-shadow-count-shapes", &[("count", &p.targets.len())]);
        let mut body=v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        use super::glow_panel::{effect_field,pt_spec};
        let t=crate::shell::i18n::text;let alpha=p.alpha.clone();
        body=body.child(crate::shell::chrome::text_field_row(t("pptx.shadow-panel.blur-pt"),effect_field(&fields[0],"pptx-shadow-field-0".into(),Some(pt_spec(100.)),Some("pt"),cx)))
            .child(crate::shell::chrome::color_text_field_row(t("pptx.gradient-panel.color-rgb-rrggbb"),crate::shell::chrome::panel_color_field("pptx-shadow-field-1",&fields[1],cx)))
            .child(crate::shell::chrome::slider_row(t("pptx.gradient-panel.intensity-percent"),crate::shell::chrome::slider("pptx-shadow-field-2",&alpha).shown(p.color.opacity()).value(cx).unit("%")))
            .child(crate::shell::chrome::text_field_row(t("pptx.shadow-panel.distance-pt"),effect_field(&fields[3],"pptx-shadow-field-3".into(),Some(pt_spec(200.)),Some("pt"),cx)))
            .child(crate::shell::chrome::text_field_row(t("pptx.gradient-panel.angle-degrees"),effect_field(&fields[4],"pptx-shadow-field-4".into(),Some(angle_spec()),Some("°"),cx)));
        if let Some(error)=error {body=body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error));}
        let panel_actions = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("pptx-shadow-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e,_,w,cx|e.apply_shadow_panel(w,cx)))).action(crate::shell::chrome::ActionDefinition::new("pptx-shadow-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e,_,w,cx|e.close_shadow_panel(w,cx))));
        Some(SidebarPanel::with_footer(title, body.into_any_element(), panel_actions))
    }
}
