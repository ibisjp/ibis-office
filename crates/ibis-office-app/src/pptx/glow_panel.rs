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
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

pub(crate) struct GlowPanel {
    pub(crate) fields: Vec<Entity<InputState>>,
    targets: Vec<(SlideId, Shape)>,
    error: Option<SharedString>,
}

pub(super) fn value(fields: &[Entity<InputState>], cx: &App) -> Option<(Color, i64)> {
    let radius: f64 = fields[0].read(cx).value().trim().parse().ok()?;
    let emu = (radius * 12_700.).round();
    if !radius.is_finite() || radius < 0. || emu > i32::MAX as f64 { return None; }
    Some((color(&fields[1],&fields[2],cx)?,emu as i64))
}
/// A field of the effect panels (glow, shadow, gradient, pattern): `None` is a colour code (RRGGBB, no ✕, 2026-10-23
/// ユーザー); a number has ▲▼ by `spec`. These panels apply with 適用, so ▲▼ only change the field.
pub(super) fn effect_field(input:&Entity<InputState>,id:String,spec:Option<crate::shell::chrome::SpinnerSpec>,cx:&Context<SlideEditor>)->crate::shell::chrome::TextField {
    let field=crate::shell::chrome::text_field(input).full_width();
    match spec {Some(spec)=>field.spinner(id,spec,cx,|_,_,_,_|{}),None=>field.no_clear()}
}
pub(super) fn color(rgb_field:&Entity<InputState>,opacity_field:&Entity<InputState>,cx:&App)->Option<Color> {
    let text=rgb_field.read(cx).value();let hex=text.trim().trim_start_matches('#');
    if hex.len()!=6 || !hex.bytes().all(|c|c.is_ascii_hexdigit()) {return None;}
    let rgb=Rgb(u32::from_str_radix(hex,16).ok()?);let opacity:f64=opacity_field.read(cx).value().trim().parse().ok()?;
    if !opacity.is_finite() || !(0. ..=100.).contains(&opacity) {return None;}
    Some(Color {rgb,alpha:(opacity*255./100.).round() as u8})
}

impl SlideEditor {
    pub(crate) fn open_glow_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_marked(cx);
        let targets: Vec<_> = self.selected_shapes().into_iter().map(|s| (self.current_slide, s.clone())).collect();
        if targets.is_empty() || targets.iter().any(|(_,s)| !matches!(s.kind, ShapeKind::Auto(_))) { return; }
        let ShapeKind::Auto(auto) = &targets[0].1.kind else { return };
        let (color, radius) = auto.glow.unwrap_or((Color::BLACK, 63_500));
        let defaults = [format!("{:.8}",radius as f64 / 12_700.),format!("{:06X}",color.rgb.0),format!("{:.8}",color.alpha as f64 * 100. / 255.)];
        let fields: Vec<_> = defaults.into_iter().map(|v| cx.new(|cx| InputState::new(window,cx).default_value(v))).collect();
        let first = fields[0].clone();
        if let Some(s)=self.pptx_mut() {
            s.section_panel=None;s.gradient_panel=None;s.shadow_panel=None;s.paragraph_panel=None;s.timing_panel=None;s.crop_panel=None;s.insets_panel=None;s.size_panel=None; s.frame_panel = None; s.comments = None; s.chart_panel = None; s.header_footer = None;s.find=None;
            s.glow_panel=Some(GlowPanel {fields,targets,error:None});
        }
        self.sidebar_open=true;first.update(cx,|f,cx|f.focus(window,cx));cx.notify();
    }

    pub(crate) fn apply_glow_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p)=self.pptx().and_then(|s|s.glow_panel.as_ref()) else {return};
        let valid=value(&p.fields,cx);
        let conflict=self.text_editing().is_some_and(|e|e.marked.is_some()) || p.targets.iter().any(|(slide,snapshot)| self.presentation().slide(*slide).and_then(|s|s.shape(snapshot.id))!=Some(snapshot));
        let error=if conflict {Some(crate::shell::i18n::text("pptx.glow-panel.target-shape-has-changed-reopen"))} else if valid.is_none() {Some(crate::shell::i18n::text("pptx.glow-panel.enter-radius-0-169093-20055118"))} else {None};
        if let Some(error)=error {
            if let Some(p)=self.pptx_mut().and_then(|s|s.glow_panel.as_mut()) {p.error=Some(error.into());}return cx.notify();
        }
        let glow=valid.unwrap();
        let commands=p.targets.iter().filter_map(|(slide,shape)| {
            let ShapeKind::Auto(auto)=&shape.kind else {return None};if auto.glow==Some(glow) {return None;}
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
        for (i,(label,field)) in [crate::shell::i18n::text("pptx.glow-panel.radius-pt"),crate::shell::i18n::text("pptx.gradient-panel.color-rgb-rrggbb"),crate::shell::i18n::text("pptx.gradient-panel.intensity-percent")].into_iter().zip(fields).enumerate() {
            use crate::shell::chrome::SpinnerSpec;
            let spec=[Some(SpinnerSpec::new(1.0).range(0.0,150.0)),None,Some(SpinnerSpec::new(1.0).range(0.0,100.0))][i];
            body=body.child(div().text_sm().child(label)).child(effect_field(&field,format!("pptx-glow-field-{i}"),spec,cx));
        }
        if let Some(error)=error {body=body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error));}
        let panel_actions = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("pptx-glow-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e,_,w,cx|e.apply_glow_panel(w,cx)))).action(crate::shell::chrome::ActionDefinition::new("pptx-glow-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e,_,w,cx|e.close_glow_panel(w,cx))));
        Some(SidebarPanel::with_footer(title, body.into_any_element(), panel_actions))
    }
}
