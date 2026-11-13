// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Arbitrary shape shadow with frozen targets and serialization preflight.
use gpui_kit::component::input::InputState;
use gpui_kit::component::v_flex;
use gpui_kit::*;
use crate::palette::rgb;
use ibis_doc_model::{Color, Command, Shape, ShapeKind, SlideId, Shadow};
use crate::chrome::{Shown, ShownNumber, SidebarPanel, Typed};
use crate::glow_panel::{EffectColor, GlowEdit};
use crate::editor::SlideEditor;
use crate::format::with_format;

pub struct ShadowPanel {
    pub fields: Vec<Entity<InputState>>,
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
fn angle_spec()->crate::chrome::SpinnerSpec {crate::chrome::SpinnerSpec::new(1.0).range(0.0,359.0).decimals(1)}

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
    pub fn open_shadow_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_marked(cx);
        let targets: Vec<_> = self.selected_shapes().into_iter().map(|s| (self.current_slide, s.clone())).collect();
        if targets.is_empty() || targets.iter().any(|(_,s)| !matches!(s.kind, ShapeKind::Auto(_))) { return; }
        let shadows:Vec<Shadow>=targets.iter().filter_map(|(_,s)|match &s.kind {ShapeKind::Auto(auto)=>Some(auto.shadow.unwrap_or_else(default_shadow)),_=>None}).collect();
        let colors:Vec<Color>=shadows.iter().map(|s|s.color).collect();
        let shown=[ShownNumber::new(Shown::of(&shadows,|s|s.blur),12_700.,crate::glow_panel::pt_spec(100.)),ShownNumber::new(Shown::of(&shadows,|s|s.distance),12_700.,crate::glow_panel::pt_spec(200.)),ShownNumber::new(Shown::of(&shadows,|s|s.direction as i64),60_000.,angle_spec())];
        let color=EffectColor::of(&colors);
        let mut fields=vec![shown[0].input(window,cx)];
        fields.extend(color.inputs(window,cx));
        fields.push(shown[1].input(window,cx));fields.push(shown[2].input(window,cx));
        let first = fields[0].clone();
        let mut subscriptions=Vec::new();let alpha=crate::gradient_panel::alpha_slider(&fields[2],window,cx,&mut subscriptions);
        self.shape_edit.gradient_panel=None; self.shape_edit.glow_panel=None; self.shape_edit.paragraph_panel=None; self.shape_edit.insets_panel=None; self.shape_edit.size_panel=None;
        self.shape_edit.crop_panel=None; self.shape_edit.frame_panel = None; self.shape_edit.header_footer = None; self.shape_edit.find=None;
        if self.uses_shape_edit() {
            with_format!(self, panel_opening(self, crate::shape_state::ShapePanel::Shadow));
            self.shape_edit.chart_panel = None;
            self.shape_edit.shadow_panel=Some(ShadowPanel {fields,shown,color,alpha,_subscriptions:subscriptions,targets,error:None});
        }
        self.sidebar_open=true;first.update(cx,|f,cx|f.focus(window,cx));cx.notify();
    }

    pub fn apply_shadow_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p)=self.shape_edit.shadow_panel.as_ref() else {return};
        let valid=ShadowEdit::read(&p.fields,&p.shown,&p.color,cx);
        let conflict=self.text_editing().is_some_and(|e|e.marked.is_some()) || p.targets.iter().any(|(slide,snapshot)| self.presentation().slide(*slide).and_then(|s|s.shape(snapshot.id))!=Some(snapshot));
        let error=if conflict {Some(crate::i18n::text("pptx.glow-panel.target-shape-has-changed-reopen"))} else if valid.is_none() {Some(crate::i18n::text("pptx.shadow-panel.enter-blur-distance-0-169093"))} else {None};
        if let Some(error)=error {
            if let Some(p)=self.shape_edit.shadow_panel.as_mut() {p.error=Some(error.into());}return cx.notify();
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
            if let Some(p)=self.shape_edit.shadow_panel.as_mut() {p.error=Some(crate::i18n::text("pptx.shadow-panel.outer-shadow-cannot-saved-selected").into());}cx.notify();
        }
    }

    pub fn close_shadow_panel(&mut self, window:&mut Window,cx:&mut Context<Self>) {
        if self.uses_shape_edit() {self.shape_edit.shadow_panel=None;}self.focus_handle.focus(window,cx);cx.notify();
    }

    pub fn shadow_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel> {
        let p=self.shape_edit.shadow_panel.as_ref()?;let fields=p.fields.clone();let error=p.error.clone();let title=crate::i18n::format("_pptx.outer-shadow-count-shapes", &[("count", &p.targets.len())]);
        let mut body=v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        use crate::glow_panel::{effect_field,pt_spec};
        let t=crate::i18n::text;let alpha=p.alpha.clone();
        body=body.child(crate::chrome::text_field_row(t("pptx.shadow-panel.blur-pt"),effect_field(&fields[0],"pptx-shadow-field-0".into(),Some(pt_spec(100.)),Some("pt"),cx)))
            .child(crate::chrome::color_text_field_row(t("pptx.gradient-panel.color-rgb-rrggbb"),crate::chrome::panel_color_field("pptx-shadow-field-1",&fields[1],cx)))
            .child(crate::chrome::slider_row(t("pptx.gradient-panel.intensity-percent"),crate::chrome::slider("pptx-shadow-field-2",&alpha).shown(p.color.opacity()).value(cx).unit("%")))
            .child(crate::chrome::text_field_row(t("pptx.shadow-panel.distance-pt"),effect_field(&fields[3],"pptx-shadow-field-3".into(),Some(pt_spec(200.)),Some("pt"),cx)))
            .child(crate::chrome::text_field_row(t("pptx.gradient-panel.angle-degrees"),effect_field(&fields[4],"pptx-shadow-field-4".into(),Some(angle_spec()),Some("°"),cx)));
        if let Some(error)=error {body=body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error));}
        let panel_actions = crate::chrome::action_bar().action(crate::chrome::ActionDefinition::new("pptx-shadow-apply", crate::i18n::text("_shell.apply"), crate::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e,_,w,cx|e.apply_shadow_panel(w,cx)))).action(crate::chrome::ActionDefinition::new("pptx-shadow-cancel", crate::i18n::text("_shell.cancel"), crate::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e,_,w,cx|e.close_shadow_panel(w,cx))));
        Some(SidebarPanel::with_footer(title, body.into_any_element(), panel_actions))
    }
}
