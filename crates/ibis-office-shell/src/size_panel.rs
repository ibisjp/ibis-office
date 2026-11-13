// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Custom slide canvas dimensions, and what happens to the contents (kept,
//! maximized or fitted: 最大化・サイズに合わせて調整).
use gpui_kit::component::input::InputState;
use gpui_kit::component::v_flex;
use gpui_kit::*;
use crate::palette::rgb;
use ibis_doc_model::EmuSize;
use crate::chrome::{ShownNumber, SidebarPanel, SpinnerSpec, Typed};
use crate::editor::SlideEditor;
use crate::format::with_format;

pub struct SizePanel {
    pub width: Entity<InputState>,
    pub height: Entity<InputState>,
    /// スライド番号の開始値.
    pub first: Entity<InputState>,
    error: Option<SharedString>,
    pub(crate) mode: ScaleMode,
    /// The width and the height as the model has them (EMU) and as the fields show them (cm): an Apply without an edit keeps the
    /// exact size (a width of 33 867 000 EMU is shown 94.07).
    numbers: [ShownNumber; 2],
}

/// What a new slide size does to the slides' contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScaleMode {
    /// Shapes keep their place and size.
    Keep,
    /// Scaled up to fill the new canvas (content may not fit).
    Maximize,
    /// Scaled to fit inside the new canvas.
    Fit,
}

/// The digits a length (cm) is shown with (the same as the size and position panel).
fn cm_spec() -> SpinnerSpec { SpinnerSpec::new(0.1).decimals(2) }

fn dimension(number: &ShownNumber, value: &str) -> Option<i64> {
    match number.resolve(value) {
        // The size the slide has is kept as it is even when it is outside what can be typed (an imported file).
        Typed::Value(emu) if emu.is_finite() && ((914_400. ..=51_206_400.).contains(&emu) || value.trim() == number.text()) => Some(emu as i64),
        _ => None,
    }
}

impl SlideEditor {
    pub fn open_slide_size_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_slide_size_panel_for(self.presentation().slide_size, ScaleMode::Keep, window, cx);
    }

    /// 縦・横: the panel with the width and height swapped to the other
    /// orientation (nothing happens when it is the one the slide has).
    pub fn open_slide_orientation(&mut self, portrait: bool, window: &mut Window, cx: &mut Context<Self>) {
        let size = self.presentation().slide_size;
        if (size.height > size.width) == portrait { return; }
        self.open_slide_size_panel_for(EmuSize::new(size.height, size.width), ScaleMode::Fit, window, cx);
    }

    fn open_slide_size_panel_for(&mut self, size: EmuSize, mode: ScaleMode, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        // Share the size/position panel's cm display precision. The exact EMU dimensions stay in `ShownNumber`, so
        // formatting never changes the model.
        let numbers = [ShownNumber::one(size.width, 360_000., cm_spec()), ShownNumber::one(size.height, 360_000., cm_spec())];
        let width = cx.new(|cx| InputState::new(window, cx).placeholder(crate::i18n::text("pptx.size-panel.width-cm")).default_value(numbers[0].text().to_string()));
        let height = cx.new(|cx| InputState::new(window, cx).placeholder(crate::i18n::text("pptx.size-panel.height-cm")).default_value(numbers[1].text().to_string()));
        let first = cx.new(|cx| InputState::new(window, cx).default_value(self.presentation().first_slide_number.to_string()));
        self.shape_edit.gradient_panel = None; self.shape_edit.shadow_panel = None; self.shape_edit.glow_panel = None; self.shape_edit.paragraph_panel = None; self.shape_edit.insets_panel = None;
        self.shape_edit.crop_panel = None; self.shape_edit.find = None; self.shape_edit.frame_panel = None; self.shape_edit.header_footer = None;
        if self.uses_shape_edit() {
            with_format!(self, panel_opening(self, crate::shape_state::ShapePanel::Size));
            self.shape_edit.chart_panel = None;
            self.shape_edit.size_panel = Some(SizePanel { width: width.clone(), height, first, error: None, mode, numbers });
        }
        self.sidebar_open = true;
        width.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub fn apply_slide_size_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.shape_edit.size_panel.as_ref() else { return };
        let displayed_width = panel.width.read(cx).value();
        let displayed_height = panel.height.read(cx).value();
        let width = dimension(&panel.numbers[0], displayed_width.as_ref());
        let height = dimension(&panel.numbers[1], displayed_height.as_ref());
        let size = width.zip(height);
        let Some((width, height)) = size else {
            if let Some(panel) = self.shape_edit.size_panel.as_mut() { panel.error = Some(crate::i18n::text("pptx.size-panel.enter-width-height-between-2").into()); }
            return cx.notify();
        };
        let mode = panel.mode;
        let first = panel.first.read(cx).value().trim().parse::<usize>().ok().filter(|n| *n <= 9999);
        if first.is_none() {
            if let Some(panel) = self.shape_edit.size_panel.as_mut() { panel.error = Some(crate::i18n::text("pptx.size-panel.enter-whole-number-0-9999").into()); }
            return cx.notify();
        }
        self.set_slide_size_scaled(EmuSize::new(width, height), mode, window, cx);
        if let Some(number) = first.filter(|n| *n != self.presentation().first_slide_number) {
            self.run(vec![ibis_doc_model::Command::SetFirstSlideNumber { number }], cx);
        }
        if self.uses_shape_edit() { self.shape_edit.size_panel = None; }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub fn close_slide_size_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.uses_shape_edit() { self.shape_edit.size_panel = None; }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub fn slide_size_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.shape_edit.size_panel.as_ref()?;
        let (width, height, error, mode, first) = (panel.width.clone(), panel.height.clone(), panel.error.clone(), panel.mode, panel.first.clone());
        let choice = |id: &'static str, label: &'static str, this: ScaleMode| {
            crate::chrome::segment_control::SegmentItem::label(id, crate::i18n::text(label), mode == this).on_click(cx.listener(move |e, _, _, cx| {
                if let Some(p) = e.shape_edit.size_panel.as_mut() { p.mode = this; }
                cx.notify();
            }))
        };
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.))
            .p_0().child(div().text_sm().child(crate::i18n::text(match mode {
                ScaleMode::Keep => "pptx.size-panel.keeps-position-size-shapes",
                ScaleMode::Maximize => "pptx.size-panel.shapes-scaled-fill-new-size",
                ScaleMode::Fit => "pptx.size-panel.shapes-scaled-fit-new-size",
            })))
            .child(crate::chrome::SegmentControlRow::new(crate::chrome::segment_control::Mode::Intrinsic)
                .child(choice("pptx-size-keep", "pptx.size-panel.keep", ScaleMode::Keep))
                .child(choice("pptx-size-maximize", "pptx.size-panel.maximize", ScaleMode::Maximize))
                .child(choice("pptx-size-fit", "pptx.size-panel.ensure-fit", ScaleMode::Fit)))
            .child(crate::chrome::text_field_row(crate::i18n::text("pptx.frame-panel.width-cm"), crate::chrome::text_field(&width).unit("cm", px(24.)).spinner("pptx-size-width", crate::chrome::SpinnerSpec::new(0.1).range(2.54, 142.24), cx, |_, _, _, _| {})))
            .child(crate::chrome::text_field_row(crate::i18n::text("pptx.frame-panel.height-cm"), crate::chrome::text_field(&height).unit("cm", px(24.)).spinner("pptx-size-height", crate::chrome::SpinnerSpec::new(0.1).range(2.54, 142.24), cx, |_, _, _, _| {})))
            .child(div().text_sm().child(crate::i18n::text("pptx.size-panel.first-slide-number")))
            .child(crate::chrome::text_field(&first).full_width().spinner("pptx-size-first", crate::chrome::SpinnerSpec::new(1.0).range(0.0, 9999.0).integer_only(), cx, |_, _, _, _| {}));
        if let Some(error) = error { body = body.child(div().text_sm().text_color(rgb(0xb3261e)).child(error)); }
        let body = body;
        let panel_actions = crate::chrome::action_bar()
            .action(crate::chrome::ActionDefinition::new("pptx-size-apply", crate::i18n::text("_shell.apply"), crate::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_slide_size_panel(w, cx))))
            .action(crate::chrome::ActionDefinition::new("pptx-size-cancel", crate::i18n::text("_shell.cancel"), crate::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_slide_size_panel(w, cx))));
        Some(SidebarPanel::with_footer(crate::i18n::text("pptx.size-panel.custom-slide-size"), body.into_any_element(), panel_actions))
    }
}
