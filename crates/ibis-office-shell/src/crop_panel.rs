// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Percentage crop inputs; preserve the picture frame, media and other effects.
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::v_flex;
use gpui_kit::*;
use crate::palette::rgb;
use ibis_doc_model::{Crop, ShapeId, ShapeKind, SlideId};
use crate::chrome::SidebarPanel;
use crate::editor::SlideEditor;
use crate::crop_units::{CropUnit, cropped_size};
use crate::format::with_format;

pub struct CropPanel {
    pub fields: Vec<Entity<InputState>>,
    targets: Vec<(SlideId, ShapeId)>,
    error: Option<SharedString>,
    /// The unit the fields are typed and shown in (the first input of the panel).
    pub(crate) unit: CropUnit,
    /// The natural size of the first picture in pixels (width, height): what an amount in a length unit is a fraction of.
    source: (f64, f64),
    _subscriptions: Vec<Subscription>,
}

/// The value of a field as a fraction of the source side `source_px`, in 1/100000 (the file's `a:srcRect` is in 1/1000 %).
fn crop_value(unit: CropUnit, value: &str, source_px: f64) -> Option<i32> {
    let amount: f64 = value.trim().parse().ok()?;
    let scaled = (unit.fraction(amount, source_px) * 100_000.).round();
    (amount.is_finite() && (i32::MIN as f64..=i32::MAX as f64).contains(&scaled)).then_some(scaled as i32)
}

/// The picture's size in pixels: its file's (the media), else its frame's at 96 dpi.
fn source_pixels(editor: &SlideEditor, shape: &ibis_doc_model::Shape, media: ibis_doc_model::MediaId) -> (f64, f64) {
    editor.presentation().media.iter().find(|m| m.id == media).and_then(|m| crate::shape_items::picture_size(&m.data))
        .map(|(w, h)| (f64::from(w), f64::from(h)))
        .unwrap_or((shape.frame.width as f64 / 9525., shape.frame.height as f64 / 9525.))
}

impl SlideEditor {
    pub fn open_crop_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let pictures: Vec<_> = self.selected_shapes().into_iter().filter_map(|s| match &s.kind {
            ShapeKind::Picture(p) => Some((s.id, p.crop, source_pixels(self, s, p.media))), _ => None,
        }).collect();
        let Some(&(_, crop, source)) = pictures.first() else { return };
        let unit = CropUnit::default();
        let sides = [crop.left, crop.top, crop.right, crop.bottom];
        let fields: Vec<_> = sides.into_iter().enumerate().map(|(i, v)| {
            let side = if i % 2 == 0 { source.0 } else { source.1 };
            cx.new(|cx| InputState::new(window, cx).default_value(unit.format(unit.amount(f64::from(v) / 100_000., side))))
        }).collect();
        // The size line follows what is typed.
        let subscriptions = fields.iter().map(|f| cx.subscribe_in(f, window, |_, _, event: &InputEvent, _, cx| if matches!(event, InputEvent::Change) { cx.notify(); })).collect();
        let first = fields[0].clone();
        let targets = pictures.into_iter().map(|(id, _, _)| (self.current_slide, id)).collect();
        self.shape_edit.gradient_panel = None; self.shape_edit.shadow_panel = None; self.shape_edit.glow_panel = None; self.shape_edit.paragraph_panel = None; self.shape_edit.size_panel = None; self.shape_edit.insets_panel = None;
        self.shape_edit.find = None; self.shape_edit.frame_panel = None; self.shape_edit.header_footer = None;
        if self.uses_shape_edit() {
            with_format!(self, panel_opening(self, crate::shape_state::ShapePanel::Crop));
            self.shape_edit.chart_panel = None;
            self.shape_edit.crop_panel = Some(CropPanel { fields, targets, error: None, unit, source, _subscriptions: subscriptions });
        }
        self.sidebar_open = true;
        first.update(cx, |f, cx| f.focus(window, cx));
        cx.notify();
    }

    pub fn apply_crop_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.shape_edit.crop_panel.as_ref() else { return };
        let (unit, source) = (panel.unit, panel.source);
        let values: Option<Vec<_>> = panel.fields.iter().enumerate().map(|(i, f)| crop_value(unit, f.read(cx).value().as_ref(), if i % 2 == 0 { source.0 } else { source.1 })).collect();
        let valid = values.filter(|v| i64::from(v[0]) + i64::from(v[2]) < 100_000 && i64::from(v[1]) + i64::from(v[3]) < 100_000);
        let Some(v) = valid else {
            if let Some(p) = self.shape_edit.crop_panel.as_mut() { p.error = Some(crate::i18n::text("pptx.crop-panel.enter-finite-numbers-left-right").into()); }
            return cx.notify();
        };
        let crop = Crop { left: v[0], top: v[1], right: v[2], bottom: v[3] };
        let commands = panel.targets.clone().into_iter().filter_map(|(slide, shape)| {
            let target = self.presentation().slide(slide)?.shape(shape)?;
            let ShapeKind::Picture(p) = &target.kind else { return None };
            Some(crate::shape_editing::crop_commands(slide, target, p, crop))
        }).flatten().collect();
        self.run(commands, cx);
        self.close_crop_panel(window, cx);
    }

    pub fn close_crop_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.uses_shape_edit() { self.shape_edit.crop_panel = None; }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// 単位: the fields' values written in `unit` (what is typed is kept as the same lengths; a field that is not a number stays).
    pub fn set_crop_unit(&mut self, unit: CropUnit, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.shape_edit.crop_panel.as_ref() else { return };
        if panel.unit == unit { return; }
        let (old, source, fields) = (panel.unit, panel.source, panel.fields.clone());
        for (i, field) in fields.iter().enumerate() {
            let side = if i % 2 == 0 { source.0 } else { source.1 };
            let Ok(amount) = field.read(cx).value().trim().parse::<f64>() else { continue };
            let text = unit.format(unit.amount(old.fraction(amount, side), side));
            field.update(cx, |f, cx| f.set_value(text, window, cx));
        }
        if let Some(p) = self.shape_edit.crop_panel.as_mut() { p.unit = unit; p.error = None; }
        cx.notify();
    }

    pub fn crop_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.shape_edit.crop_panel.as_ref()?;
        let (fields, error, unit, source) = (p.fields.clone(), p.error.clone(), p.unit, p.source);
        let t = crate::i18n::text;
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.))
            .p_0().child(div().text_sm().child(t("pptx.crop-panel.keeps-picture-frame-negative")));
        // The unit first: the fields below are typed in it.
        body = body.child(crate::chrome::segment_control_row(t("pptx.crop-panel.unit"), CropUnit::ALL.map(|u| {
            crate::chrome::segment_control::SegmentItem::label(format!("pptx-crop-unit-{}", u.abbreviation()), u.abbreviation(), u == unit)
                .on_click(cx.listener(move |e, _, w, cx| e.set_crop_unit(u, w, cx)))
        })));
        let mut fractions = [0.; 4];
        for (i, (label, field)) in [t("pptx.crop-panel.left-percent"), t("pptx.crop-panel.top-percent"), t("pptx.crop-panel.right-percent"), t("pptx.crop-panel.bottom-percent")].into_iter().zip(fields).enumerate() {
            let side = if i % 2 == 0 { source.0 } else { source.1 };
            if let Ok(amount) = field.read(cx).value().trim().parse::<f64>() { fractions[i] = unit.fraction(amount, side); }
            // Applied with 適用, so ▲▼ only change the field. Negative values extend the picture.
            let spinner = crate::chrome::SpinnerSpec::new(unit.step() as f32);
            let spinner = if unit == CropUnit::Percent { spinner.max(100.0) } else { spinner };
            body = body.child(crate::chrome::text_field_row(label, crate::chrome::text_field(&field).unit(unit.abbreviation(), px(32.)).spinner(format!("pptx-crop-field-{i}"), spinner, cx, |_, _, _, _| {})));
        }
        // The size of the picture left after the crop (what is typed, as in the unit).
        let (w, h) = cropped_size(unit, source, fractions);
        body = body.child(div().id("pptx-crop-size").text_sm().child(crate::i18n::format("pptx.crop-panel.size", &[
            ("width", &format!("{} {}", unit.format(w), unit.abbreviation())), ("height", &format!("{} {}", unit.format(h), unit.abbreviation()))])));
        if let Some(error) = error { body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error)); }
        let panel_actions = crate::chrome::action_bar()
            .action(crate::chrome::ActionDefinition::new("pptx-crop-apply", t("_shell.apply"), crate::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_crop_panel(w, cx))))
            .action(crate::chrome::ActionDefinition::new("pptx-crop-cancel", t("_shell.cancel"), crate::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_crop_panel(w, cx))));
        Some(SidebarPanel::with_footer(t("pptx.crop-panel.crop-picture"), body.into_any_element(), panel_actions))
    }
}
