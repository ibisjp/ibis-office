// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The shared equation editor used by document and presentation dialogs.
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::v_flex;
use gpui_kit::*;
use ibis_doc_model::math::{Math, parse_latex, parse_linear};
use super::chrome::{self, ActionDefinition, SidebarPanel};
use super::editor::SlideEditor;
use super::i18n::text;
use super::palette::rgb;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum MathFormat {
    #[default]
    UnicodeMath,
    Latex,
}
impl MathFormat {
    pub(crate) fn parse(self, value: &str) -> Math {
        let value = value.trim();
        if self == Self::Latex || value.starts_with('\\') { parse_latex(value) } else { parse_linear(value) }
    }
}

/// Input lifecycle and refresh behavior are identical in every equation editor.
pub(crate) struct MathEditorState {
    pub(crate) input: Entity<InputState>,
    pub(crate) format: MathFormat,
    _subscriptions: Vec<Subscription>,
}
impl MathEditorState {
    pub(crate) fn new(
        initial: &str,
        window: &mut Window,
        cx: &mut Context<SlideEditor>,
        on_enter: fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>),
    ) -> Self {
        let input = cx.new(|cx| {
            let mut input = InputState::new(window, cx)
                .placeholder(r"x^2/2 + √(a+b) or \frac{-b \pm \sqrt{b^2-4ac}}{2a}");
            input.set_value(initial, window, cx);
            input.focus(window, cx);
            input
        });
        let subscription = cx.subscribe_in(&input, window, move |editor, _, event: &InputEvent, window, cx| {
            // Changes must redraw both the input and its typeset preview.
            cx.notify();
            if matches!(event, InputEvent::PressEnter { .. }) { on_enter(editor, window, cx); }
        });
        Self { input, format: MathFormat::UnicodeMath, _subscriptions: vec![subscription] }
    }
}

pub(crate) struct MathDialogSpec {
    pub(crate) input: Entity<InputState>,
    pub(crate) format: MathFormat,
    pub(crate) set_format: fn(&mut SlideEditor, MathFormat, &mut Context<SlideEditor>),
    pub(crate) options: Vec<AnyElement>,
    pub(crate) error: Option<String>,
    pub(crate) context_menu: Option<fn(&mut SlideEditor, Point<Pixels>, &mut Window, &mut Context<SlideEditor>)>,
    pub(crate) insert: ActionDefinition,
    pub(crate) commands: Vec<ActionDefinition>,
}
/// Layout, preview rendering, field styling and terminal action construction live here.
pub(crate) fn math_dialog(spec: MathDialogSpec, cx: &Context<SlideEditor>) -> SidebarPanel {
    let set_format = spec.set_format;
    let format = spec.format;
    let format_control = chrome::SegmentControlRow::new(chrome::segment_control::Mode::Intrinsic)
        .child(chrome::segment_control::SegmentItem::label("math-editor-unicodemath", "UnicodeMath", format == MathFormat::UnicodeMath)
            .on_click(cx.listener(move |e, _, _, cx| set_format(e, MathFormat::UnicodeMath, cx))))
        .child(chrome::segment_control::SegmentItem::label("math-editor-latex", "LaTeX", format == MathFormat::Latex)
            .on_click(cx.listener(move |e, _, _, cx| set_format(e, MathFormat::Latex, cx))));
    let parsed = format.parse(&spec.input.read(cx).value());
    let mut body = v_flex().gap_3().w_full().min_w_0()
        .child(format_control)
        .child(MathPreview { math: parsed })
        .child(div().text_sm().text_color(chrome::text_secondary()).child(text("math.editor.hint")))
        .child(chrome::text_field(&spec.input).full_width())
        .children(spec.options);
    if let Some(open) = spec.context_menu {
        body = body.on_mouse_down(MouseButton::Right, cx.listener(move |e, event: &MouseDownEvent, w, cx| open(e, event.position, w, cx)));
    }
    if let Some(error) = spec.error { body = body.child(div().text_sm().text_color(rgb(0xd32f2f)).child(error)); }
    let footer = chrome::action_bar().action(spec.insert).action(chrome::dialog_cancel_definition(cx)).actions(spec.commands);
    SidebarPanel::with_footer(text("math.editor.title"), body.into_any_element(), footer)
}

/// Uses the main content's math typesetter and measures the complete equation height.
#[derive(IntoElement)]
struct MathPreview { math: Math }
impl RenderOnce for MathPreview {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let font = gpui_kit::font("Cambria Math");
        let boxed = crate::render::math_view::typeset(&self.math, 22., &mut crate::render::math_view::window_measure(font.clone(), window));
        let height = (boxed.ascent + boxed.descent + 24.).max(80.);
        div().w_full().h(px(height)).flex_none().border_1().border_color(rgb(chrome::BORDER)).rounded_md().overflow_hidden()
            .child(canvas(|_, _, _| (), move |bounds, _, window, cx| {
                #[cfg(all(feature="scenarios", any(target_os="macos", target_os="linux")))]
                super::scenario::button_probe::record_bounds("math-editor-preview", bounds);
                let origin = point(bounds.origin.x + px(12.), bounds.origin.y + px(12. + boxed.ascent));
                crate::render::math_view::paint(&boxed, origin, chrome::text_secondary(), &font, window, cx);
            }).size_full())
    }
}
