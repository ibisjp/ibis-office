// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Percentage crop inputs; preserve the picture frame, media and other effects.
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, Crop, ShapeId, ShapeKind, SlideId};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

pub(crate) struct CropPanel {
    pub(crate) fields: Vec<Entity<InputState>>,
    targets: Vec<(SlideId, ShapeId)>,
    error: Option<SharedString>,
}

fn percentage(value: &str) -> Option<i32> {
    let value: f64 = value.trim().parse().ok()?;
    let scaled = (value * 1000.).round();
    (value.is_finite() && (i32::MIN as f64..=i32::MAX as f64).contains(&scaled)).then_some(scaled as i32)
}

impl SlideEditor {
    pub(crate) fn open_crop_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let pictures: Vec<_> = self.selected_shapes().into_iter().filter_map(|s| match &s.kind {
            ShapeKind::Picture(p) => Some((s.id, p.crop)), _ => None,
        }).collect();
        let Some((_, crop)) = pictures.first() else { return };
        let fields: Vec<_> = [crop.left, crop.top, crop.right, crop.bottom].into_iter().map(|v| cx.new(|cx| InputState::new(window, cx).default_value(format!("{:.3}", v as f64 / 1000.)))).collect();
        let first = fields[0].clone();
        let targets = pictures.into_iter().map(|(id, _)| (self.current_slide, id)).collect();
        if let Some(s) = self.pptx_mut() {
            s.timing_panel = None; s.section_panel=None;s.gradient_panel = None; s.shadow_panel = None; s.glow_panel = None; s.paragraph_panel = None; s.find = None; s.size_panel = None; s.frame_panel = None; s.comments = None; s.chart_panel = None; s.header_footer = None; s.insets_panel = None;
            s.crop_panel = Some(CropPanel { fields, targets, error: None });
        }
        self.sidebar_open = true;
        first.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn apply_crop_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.crop_panel.as_ref()) else { return };
        let values: Option<Vec<_>> = panel.fields.iter().map(|f| percentage(f.read(cx).value().as_ref())).collect();
        let valid = values.filter(|v| i64::from(v[0]) + i64::from(v[2]) < 100_000 && i64::from(v[1]) + i64::from(v[3]) < 100_000);
        let Some(v) = valid else {
            if let Some(p) = self.pptx_mut().and_then(|s| s.crop_panel.as_mut()) { p.error = Some(crate::shell::i18n::text("pptx.crop-panel.enter-finite-numbers-left-right").into()); }
            return cx.notify();
        };
        let crop = Crop { left: v[0], top: v[1], right: v[2], bottom: v[3] };
        let commands = panel.targets.clone().into_iter().filter_map(|(slide, shape)| {
            let target = self.presentation().slide(slide)?.shape(shape)?;
            let ShapeKind::Picture(p) = &target.kind else { return None };
            if p.crop == crop { return None; }
            let mut p = p.clone(); p.crop = crop;
            Some(Command::SetKind { slide, shape, kind: Box::new(ShapeKind::Picture(p)) })
        }).collect();
        self.run(commands, cx);
        self.close_crop_panel(window, cx);
    }

    pub(crate) fn close_crop_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() { s.crop_panel = None; }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn crop_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.crop_panel.as_ref()?;
        let (fields, error) = (p.fields.clone(), p.error.clone());
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.))
            .p_0().child(div().text_sm().child(crate::shell::i18n::text("pptx.crop-panel.keeps-picture-frame-negative")));
        for (i, (label, field)) in [crate::shell::i18n::text("pptx.crop-panel.left-percent"), crate::shell::i18n::text("pptx.crop-panel.top-percent"), crate::shell::i18n::text("pptx.crop-panel.right-percent"), crate::shell::i18n::text("pptx.crop-panel.bottom-percent")].into_iter().zip(fields).enumerate() {
            // Applied with 適用, so ▲▼ only change the field. Negative values extend the picture.
            let spinner = crate::shell::chrome::SpinnerSpec::new(1.0).max(100.0);
            body = body.child(crate::shell::chrome::text_field_row(label, crate::shell::chrome::text_field(&field).unit("%", px(24.)).spinner(format!("pptx-crop-field-{i}"), spinner, cx, |_, _, _, _| {})));
        }
        if let Some(error) = error { body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error)); }
        let panel_actions = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::ActionDefinition::new("pptx-crop-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_crop_panel(w, cx))))
            .action(crate::shell::chrome::ActionDefinition::new("pptx-crop-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_crop_panel(w, cx))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("pptx.crop-panel.crop-picture"), body.into_any_element(), panel_actions))
    }
}
