// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Live shape-decoration fields, retaining OOXML parameters which the UI does not edit.
use super::text_box_panel::TextBoxState;
use crate::shell::{
    editor::SlideEditor,
    effect_editor::{self, EffectHooks, SECTIONS},
};
use gpui_kit::*;
use crate::ext::*;
pub use crate::shell::effect_editor::{EffectEditor, EffectModel};

/// The shared decoration editor's hooks for the text box panel.
pub(crate) fn hooks() -> EffectHooks {
    EffectHooks {
        get: |e| e.doc_state_mut().and_then(|d| d.text_box.as_mut()).map(|s| &mut s.effects),
        toggle: |e, index| {
            if let Some(s) = e.doc_state_mut().and_then(|d| d.text_box.as_mut()) {
                match index {
                    0 => s.gradient = !s.gradient,
                    1 => s.shadow = !s.shadow,
                    2 => s.glow = !s.glow,
                    3 => s.soft_edge = !s.soft_edge,
                    4 => s.reflection = !s.reflection,
                    n => s.text_enabled[n - 5] = !s.text_enabled[n - 5],
                }
            }
        },
        apply: |e, _, cx| e.apply_text_box(cx),
    }
}
pub(crate) fn new_editor(model: &EffectModel, window: &mut Window, cx: &mut Context<SlideEditor>) -> (EffectEditor, Vec<Subscription>) {
    EffectEditor::new(model, hooks(), window, cx)
}
pub(crate) fn enabled_flags(s: &TextBoxState) -> [bool; SECTIONS] {
    [s.gradient, s.shadow, s.glow, s.soft_edge, s.reflection, s.text_enabled[0], s.text_enabled[1], s.text_enabled[2]]
}
pub(crate) fn model(s: &TextBoxState, cx: &App) -> Option<EffectModel> {
    effect_editor::model(&s.effects, &s.initial.effects, &enabled_flags(s), s.initial.gradient, s.fill.read(cx).value().as_ref(), cx)
}
