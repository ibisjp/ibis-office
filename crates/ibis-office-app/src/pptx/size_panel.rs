// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Custom slide canvas dimensions, and what happens to the contents (kept,
//! maximized or fitted: 最大化・サイズに合わせて調整).
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::EmuSize;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

pub(crate) struct SizePanel {
    pub(crate) width: Entity<InputState>,
    pub(crate) height: Entity<InputState>,
    /// スライド番号の開始値.
    pub(crate) first: Entity<InputState>,
    error: Option<SharedString>,
    pub(crate) mode: ScaleMode,
    original_size: EmuSize,
    initial_dimensions: [String; 2],
}

/// What a new slide size does to the slides' contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScaleMode {
    /// Shapes keep their place and size.
    Keep,
    /// Scaled up to fill the new canvas (content may not fit).
    Maximize,
    /// Scaled to fit inside the new canvas.
    Fit,
}

fn dimension(value: &str) -> Option<i64> {
    let cm: f64 = value.trim().parse().ok()?;
    if !cm.is_finite() { return None; }
    let emu = (cm * 360_000.).round();
    if !(914_400. ..=51_206_400.).contains(&emu) { return None; }
    Some(emu as i64)
}

impl SlideEditor {
    pub(crate) fn open_slide_size_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_slide_size_panel_for(self.presentation().slide_size, ScaleMode::Keep, window, cx);
    }

    /// 縦・横: the panel with the width and height swapped to the other
    /// orientation (nothing happens when it is the one the slide has).
    pub(crate) fn open_slide_orientation(&mut self, portrait: bool, window: &mut Window, cx: &mut Context<Self>) {
        let size = self.presentation().slide_size;
        if (size.height > size.width) == portrait { return; }
        self.open_slide_size_panel_for(EmuSize::new(size.height, size.width), ScaleMode::Fit, window, cx);
    }

    fn open_slide_size_panel_for(&mut self, size: EmuSize, mode: ScaleMode, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        // Share the size/position panel's cm display precision. Preserve exact imported EMU
        // dimensions when the displayed field is unchanged, so formatting never changes the model.
        let initial_dimensions = [super::frame_panel::cm(size.width), super::frame_panel::cm(size.height)];
        let width = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("pptx.size-panel.width-cm")).default_value(initial_dimensions[0].clone()));
        let height = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("pptx.size-panel.height-cm")).default_value(initial_dimensions[1].clone()));
        let first = cx.new(|cx| InputState::new(window, cx).default_value(self.presentation().first_slide_number.to_string()));
        if let Some(state) = self.pptx_mut() {
            state.section_panel=None;state.gradient_panel = None; state.shadow_panel = None; state.glow_panel = None; state.paragraph_panel = None; state.timing_panel = None; state.crop_panel = None; state.find = None; state.insets_panel = None; state.frame_panel = None; state.comments = None; state.chart_panel = None; state.header_footer = None;
            state.size_panel = Some(SizePanel { width: width.clone(), height, first, error: None, mode, original_size: size, initial_dimensions });
        }
        self.sidebar_open = true;
        width.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn apply_slide_size_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.pptx().and_then(|s| s.size_panel.as_ref()) else { return };
        let displayed_width = panel.width.read(cx).value();
        let displayed_height = panel.height.read(cx).value();
        let width = if displayed_width.trim() == panel.initial_dimensions[0] {Some(panel.original_size.width)} else {dimension(displayed_width.as_ref())};
        let height = if displayed_height.trim() == panel.initial_dimensions[1] {Some(panel.original_size.height)} else {dimension(displayed_height.as_ref())};
        let size = width.zip(height);
        let Some((width, height)) = size else {
            if let Some(panel) = self.pptx_mut().and_then(|s| s.size_panel.as_mut()) { panel.error = Some(crate::shell::i18n::text("pptx.size-panel.enter-width-height-between-2").into()); }
            return cx.notify();
        };
        let mode = panel.mode;
        let first = panel.first.read(cx).value().trim().parse::<usize>().ok().filter(|n| *n <= 9999);
        if first.is_none() {
            if let Some(panel) = self.pptx_mut().and_then(|s| s.size_panel.as_mut()) { panel.error = Some(crate::shell::i18n::text("pptx.size-panel.enter-whole-number-0-9999").into()); }
            return cx.notify();
        }
        self.set_slide_size_scaled(EmuSize::new(width, height), mode, window, cx);
        if let Some(number) = first.filter(|n| *n != self.presentation().first_slide_number) {
            self.run(vec![ibis_doc_model::Command::SetFirstSlideNumber { number }], cx);
        }
        if let Some(state) = self.pptx_mut() { state.size_panel = None; }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn close_slide_size_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.pptx_mut() { state.size_panel = None; }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn slide_size_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.pptx()?.size_panel.as_ref()?;
        let (width, height, error, mode, first) = (panel.width.clone(), panel.height.clone(), panel.error.clone(), panel.mode, panel.first.clone());
        let choice = |id: &'static str, label: &'static str, this: ScaleMode| {
            crate::shell::chrome::segment_control::SegmentItem::label(id, crate::shell::i18n::text(label), mode == this).on_click(cx.listener(move |e, _, _, cx| {
                if let Some(p) = e.pptx_mut().and_then(|s| s.size_panel.as_mut()) { p.mode = this; }
                cx.notify();
            }))
        };
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.))
            .p_0().child(div().text_sm().child(crate::shell::i18n::text(match mode {
                ScaleMode::Keep => "pptx.size-panel.keeps-position-size-shapes",
                ScaleMode::Maximize => "pptx.size-panel.shapes-scaled-fill-new-size",
                ScaleMode::Fit => "pptx.size-panel.shapes-scaled-fit-new-size",
            })))
            .child(crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                .child(choice("pptx-size-keep", "pptx.size-panel.keep", ScaleMode::Keep))
                .child(choice("pptx-size-maximize", "pptx.size-panel.maximize", ScaleMode::Maximize))
                .child(choice("pptx-size-fit", "pptx.size-panel.ensure-fit", ScaleMode::Fit)))
            .child(crate::shell::chrome::text_field_row(crate::shell::i18n::text("pptx.frame-panel.width-cm"), crate::shell::chrome::text_field(&width).unit("cm", px(24.)).spinner("pptx-size-width", crate::shell::chrome::SpinnerSpec::new(0.1).range(2.54, 142.24), cx, |_, _, _, _| {})))
            .child(crate::shell::chrome::text_field_row(crate::shell::i18n::text("pptx.frame-panel.height-cm"), crate::shell::chrome::text_field(&height).unit("cm", px(24.)).spinner("pptx-size-height", crate::shell::chrome::SpinnerSpec::new(0.1).range(2.54, 142.24), cx, |_, _, _, _| {})))
            .child(div().text_sm().child(crate::shell::i18n::text("pptx.size-panel.first-slide-number")))
            .child(crate::shell::chrome::text_field(&first).full_width().spinner("pptx-size-first", crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0, 9999.0).integer_only(), cx, |_, _, _, _| {}));
        if let Some(error) = error { body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error)); }
        let body = body;
        let panel_actions = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::ActionDefinition::new("pptx-size-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_slide_size_panel(w, cx))))
            .action(crate::shell::chrome::ActionDefinition::new("pptx-size-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_slide_size_panel(w, cx))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("pptx.size-panel.custom-slide-size"), body.into_any_element(), panel_actions))
    }
}
