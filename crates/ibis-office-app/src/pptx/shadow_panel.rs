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
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

pub(crate) struct ShadowPanel {
    pub(crate) fields: Vec<Entity<InputState>>,
    targets: Vec<(SlideId, Shape)>,
    error: Option<SharedString>,
}

fn value(fields: &[Entity<InputState>], cx: &App) -> Option<Shadow> {
    let (color, blur)=super::glow_panel::value(fields,cx)?;
    let distance: f64=fields[3].read(cx).value().trim().parse().ok()?;
    let emu=(distance*12_700.).round();
    let angle: f64=fields[4].read(cx).value().trim().parse().ok()?;
    let direction=(angle*60_000.).round();
    if !distance.is_finite() || distance<0. || emu>i32::MAX as f64 || !angle.is_finite() || angle<0. || !(0. ..21_600_000.).contains(&direction) {return None;}
    Some(Shadow::simple(color,blur,emu as i64,direction as i32))
}

impl SlideEditor {
    pub(crate) fn open_shadow_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_marked(cx);
        let targets: Vec<_> = self.selected_shapes().into_iter().map(|s| (self.current_slide, s.clone())).collect();
        if targets.is_empty() || targets.iter().any(|(_,s)| !matches!(s.kind, ShapeKind::Auto(_))) { return; }
        let ShapeKind::Auto(auto) = &targets[0].1.kind else { return };
        let shadow=auto.shadow.unwrap_or(Shadow::simple(Color::BLACK,50_800,50_800,0));let color=shadow.color;let radius=shadow.blur;
        let defaults = [format!("{:.8}",radius as f64 / 12_700.),format!("{:06X}",color.rgb.0),format!("{:.8}",color.alpha as f64 * 100. / 255.),format!("{:.8}",shadow.distance as f64 /12_700.),format!("{:.8}",shadow.direction as f64 /60_000.)];
        let fields: Vec<_> = defaults.into_iter().map(|v| cx.new(|cx| InputState::new(window,cx).default_value(v))).collect();
        let first = fields[0].clone();
        if let Some(s)=self.pptx_mut() {
            s.section_panel=None;s.gradient_panel=None;s.glow_panel=None;s.paragraph_panel=None;s.timing_panel=None;s.crop_panel=None;s.insets_panel=None;s.size_panel=None; s.frame_panel = None; s.comments = None; s.chart_panel = None; s.header_footer = None;s.find=None;
            s.shadow_panel=Some(ShadowPanel {fields,targets,error:None});
        }
        self.sidebar_open=true;first.update(cx,|f,cx|f.focus(window,cx));cx.notify();
    }

    pub(crate) fn apply_shadow_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p)=self.pptx().and_then(|s|s.shadow_panel.as_ref()) else {return};
        let valid=value(&p.fields,cx);
        let conflict=self.text_editing().is_some_and(|e|e.marked.is_some()) || p.targets.iter().any(|(slide,snapshot)| self.presentation().slide(*slide).and_then(|s|s.shape(snapshot.id))!=Some(snapshot));
        let error=if conflict {Some(crate::shell::i18n::text("pptx.glow-panel.target-shape-has-changed-reopen"))} else if valid.is_none() {Some(crate::shell::i18n::text("pptx.shadow-panel.enter-blur-distance-0-169093"))} else {None};
        if let Some(error)=error {
            if let Some(p)=self.pptx_mut().and_then(|s|s.shadow_panel.as_mut()) {p.error=Some(error.into());}return cx.notify();
        }
        let shadow=valid.unwrap();
        let commands=p.targets.iter().filter_map(|(slide,shape)| {
            let ShapeKind::Auto(auto)=&shape.kind else {return None};if auto.shadow==Some(shadow) {return None;}
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
        for (i,(label,field)) in [crate::shell::i18n::text("pptx.shadow-panel.blur-pt"),crate::shell::i18n::text("pptx.gradient-panel.color-rgb-rrggbb"),crate::shell::i18n::text("pptx.gradient-panel.intensity-percent"),crate::shell::i18n::text("pptx.shadow-panel.distance-pt"),crate::shell::i18n::text("pptx.gradient-panel.angle-degrees")].into_iter().zip(fields).enumerate() {
            use crate::shell::chrome::SpinnerSpec;
            let spec=[Some(SpinnerSpec::new(1.0).range(0.0,100.0)),None,Some(SpinnerSpec::new(1.0).range(0.0,100.0)),Some(SpinnerSpec::new(1.0).range(0.0,200.0)),Some(SpinnerSpec::new(1.0).range(0.0,359.0))][i];
            body=body.child(div().text_sm().child(label)).child(super::glow_panel::effect_field(&field,format!("pptx-shadow-field-{i}"),spec,cx));
        }
        if let Some(error)=error {body=body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error));}
        let panel_actions = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("pptx-shadow-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e,_,w,cx|e.apply_shadow_panel(w,cx)))).action(crate::shell::chrome::ActionDefinition::new("pptx-shadow-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e,_,w,cx|e.close_shadow_panel(w,cx))));
        Some(SidebarPanel::with_footer(title, body.into_any_element(), panel_actions))
    }
}
