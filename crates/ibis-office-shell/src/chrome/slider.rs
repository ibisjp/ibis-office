// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The app's one slider: a track filled in the primary colour, and (when asked) the value at its left, next to the track. The value
//! can carry a unit (`.unit("%")`); it is grey under the pointer (it can be clicked), and a click turns it into a plain text field
//! (no ✕, no ▲▼) with all its text selected, in which a number is typed (Enter or leaving the field sets it, Esc gives it up; the
//! value is clamped to the slider's range and rounded to its step; a field left as it was changes nothing). [`super::slider_row`]
//! puts a label in front. A disabled slider is drawn grey: the grooves and the thumb under a half-transparent grey cover.
//!
//! The value lives in the caller's `Entity<SliderState>` as always (`SliderEvent::Change` is sent for a drag AND for a typed or
//! stepped number), so a screen subscribes once and does not care which way the value was set.
use super::*;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState, SliderValue};

/// The room of the value at the slider's left: the number field of `TextField::compact_spinner` (80 px with its 18 px ▲▼ and the
/// 4 px between) without the ▲▼, so the number does not jump when it turns into the field. The number is set against the track.
pub const VALUE_WIDTH: Pixels = px(80. - 18. - 4.);
/// How thick the grey cover of a disabled slider is (the user's 2026-11-05 decision: grey, half transparent).
const DISABLED_COVER_ALPHA: f32 = 0.5;
/// The size of the ordinary slider's parts (gpui_kit's `Slider`: a 24 px row, a 6 px groove, a 16 px thumb) that a disabled slider
/// is drawn in as well.
const ROW_HEIGHT: Pixels = px(24.);
const GROOVE_HEIGHT: Pixels = px(6.);
const THUMB_SIZE: Pixels = px(16.);
/// The narrowest the track is made before a [`super::slider_row`] goes to two lines.
pub(crate) const TRACK_MIN_WIDTH: Pixels = px(48.);
const GAP: Pixels = px(8.);

/// A slider on `state`, drawn bare (the track only). `.value(cx)` adds the value at the left.
pub fn slider(id: impl Into<SharedString>, state: &Entity<SliderState>) -> ValueSlider {
    ValueSlider { id: id.into(), state: state.clone(), unit: None, unit_width: px(18.), editor: None, disabled: false, track_min: TRACK_MIN_WIDTH, mixed: false }
}

#[derive(IntoElement)]
pub struct ValueSlider {
    id: SharedString,
    state: Entity<SliderState>,
    unit: Option<SharedString>,
    unit_width: Pixels,
    /// `Some`: the value is shown (and can be typed over).
    editor: Option<WeakEntity<SlideEditor>>,
    disabled: bool,
    /// The narrowest the track is drawn ([`TRACK_MIN_WIDTH`] unless the screen asks for more).
    track_min: Pixels,
    /// The objects the slider edits have different values (`Shown::Mixed`), see [`ValueSlider::mixed`].
    mixed: bool,
}
/// Whether the user has moved a mixed slider (a drag, a typed or stepped number): from then on it is drawn as an ordinary one.
/// Kept per slider state, so the screen only says `mixed(shown.is_mixed())` at every render.
struct MixedTouch {
    touched: bool,
    _subscription: Subscription,
}
impl ValueSlider {
    /// The objects the slider edits have different values (`Shown::Mixed`): the value is blank, and the track is drawn pale
    /// with the thumb at the minimum, so no value is claimed. Moving the slider (or typing a number) sets the value and
    /// the slider is drawn as usual from then on. The screen should give the state its minimum as the value and write
    /// nothing for this attribute at Apply while the slider has not been moved (`Typed::Untouched`).
    fn mixed(mut self, mixed: bool) -> Self {
        self.mixed = mixed;
        self
    }
    /// The state of the objects the slider edits (the only entry for the empty state): `Shown::Mixed` draws the blank slider
    /// (see `mixed`'s description above); the value of a common `One` is the `SliderState`'s as always.
    pub(crate) fn shown<V>(self, shown: &Shown<V>) -> Self {
        self.mixed(shown.is_mixed())
    }
    /// A track that must stay at least this wide: a row of it goes to two lines (the slider on the full width) sooner than shrink it.
    pub fn track_min_width(mut self, width: Pixels) -> Self {
        self.track_min = width;
        self
    }
    /// Show the value at the slider's left; a click on it opens the number field.
    pub fn value(mut self, cx: &Context<SlideEditor>) -> Self {
        self.editor = Some(cx.weak_entity());
        self
    }
    /// The unit written after the value (`%`, `°`, `cm`). Translate it before.
    pub fn unit(mut self, unit: impl Into<SharedString>) -> Self {
        self.unit = Some(unit.into());
        self
    }
    /// The room of the unit inside the number field (the same across a screen).
    pub fn unit_width(mut self, width: Pixels) -> Self {
        self.unit_width = width;
        self
    }
    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// The narrowest this slider is drawn: what a row needs to keep it on one line with its label.
    pub(crate) fn min_width(&self) -> Pixels {
        if self.editor.is_some() { VALUE_WIDTH + GAP + self.track_min } else { self.track_min }
    }
}

/// How many decimals the value is shown with: those of the step (1 → 0, 0.1 → 1, 0.25 → 2).
pub(crate) fn decimals_of(step: f32) -> usize {
    (0..=3).find(|&d| {
        let scaled = step * 10f32.powi(d as i32);
        (scaled - scaled.round()).abs() < 1e-3
    }).unwrap_or(3)
}

/// The value as the slider shows it, with its unit.
pub(crate) fn value_text(value: f32, step: f32, unit: Option<&str>) -> String {
    format!("{value:.*}{}", decimals_of(step), unit.unwrap_or(""))
}

/// Set `value` (clamped to the range, rounded to the step) and tell the subscribers, as a drag does.
fn set_and_send(state: &Entity<SliderState>, value: f32, window: &mut Window, cx: &mut impl AppContext) {
    state.update(cx, |s, cx| {
        let (min, max, step) = (s.min_value(), s.max_value(), s.step_value());
        let stepped = if step > 0. { min + ((value - min) / step).round() * step } else { value };
        let value = stepped.clamp(min, max);
        s.set_value(value, window, cx);
        cx.emit(SliderEvent::Change(SliderValue::Single(value)));
    });
}

/// The value at the left of the slider: the number (a click turns it into the field), or the field.
struct ValueView {
    id: SharedString,
    state: Entity<SliderState>,
    unit: Option<SharedString>,
    unit_width: Pixels,
    disabled: bool,
    editing: bool,
    /// A mixed slider the user has not moved: no value is shown.
    blank: bool,
    /// The text the field was given (what a field left as it is means: no change).
    given: String,
    input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}
impl ValueView {
    fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.editing { return; }
        self.editing = false;
        let text = self.input.read(cx).value().to_string();
        // Left as it was given (or empty on a mixed slider): the value is not touched, not even rounded or marked as set.
        if text.trim() != self.given.trim() {
            if let Some((value, _)) = split_number(&text) {
                set_and_send(&self.state, value, window, cx);
            }
        }
        cx.notify();
    }
    /// Esc: the typed text is dropped and the number is shown again.
    fn cancel(&mut self, cx: &mut Context<Self>) {
        self.editing = false;
        cx.notify();
    }
    fn begin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled { return; }
        let state = self.state.read(cx);
        let text = if self.blank { String::new() } else { value_text(state.value().end(), state.step_value(), None) };
        self.editing = true;
        self.given = text.clone();
        self.input.update(cx, |i, cx| {
            i.set_value(text, window, cx);
            i.focus(window, cx);
            i.select_all(window, cx);
        });
        cx.notify();
    }
}
impl Render for ValueView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (value, step) = {
            let s = self.state.read(cx);
            (s.value().end(), s.step_value())
        };
        if self.editing {
            let field = transient_text_field(&self.input).no_clear().select_on_focus().width(VALUE_WIDTH);
            let probe=format!("{}-field", self.id);
            // Esc is taken before the field sees it (as the font size list does), and stops there.
            let this = cx.entity().downgrade();
            let esc = canvas(|_, _, _| (), move |_, _, window, _| {
                let this = this.clone();
                window.on_key_event(move |event: &KeyDownEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && event.keystroke.key == "escape" {
                        if this.update(cx, |view, cx| if view.editing { view.cancel(cx); true } else { false }).unwrap_or(false) {
                            cx.stop_propagation();
                        }
                    }
                });
            }).absolute().size_0();
            return probed(div().id(SharedString::from(probe.clone())).w(VALUE_WIDTH).flex_none().relative().child(esc)
                .on_action(cx.listener(|this, _: &gpui_kit::component::input::Escape, _, cx| this.cancel(cx)))
                .child(field), probe).into_any_element();
        }
        let shown = if self.blank { "-".to_string() } else { value_text(value, step, self.unit.as_deref()) };
        let probe=format!("{}-value", self.id);
        probed(div()
            .id(SharedString::from(probe.clone()))
            .w(VALUE_WIDTH)
            .flex_none()
            .px(px(4.))
            .py(px(2.))
            .rounded(px(4.))
            .text_size(px(14.))
            .text_right()
            .whitespace_nowrap()
            .when(!self.disabled, |d| d.cursor_pointer().hover(|h| h.bg(crate::palette::shade(0.0, 0.08))).on_click(cx.listener(|this, _, window, cx| this.begin(window, cx))))
            .when(self.disabled, |d| d.text_color(disabled_color()))
            .when(self.blank, |d| d.text_color(disabled_color()))
            .child(shown), probe)
            .into_any_element()
    }
}
/// The scenarios measure where the value and the number field are drawn (`<id>-value`, `<id>-field`).
#[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
fn probed(element: Stateful<Div>, key: String) -> Stateful<Div> {
    element.relative().child(canvas(|_, _, _| (), move |b, _, _, _| super::super::scenario::button_probe::record_bounds(&key, b)).absolute().inset_0())
}
#[cfg(not(all(feature="scenarios",any(target_os="macos",target_os="linux"))))]
fn probed(element: Stateful<Div>, _key: String) -> Stateful<Div> { element }

impl RenderOnce for ValueSlider {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let primary = rgb(PRIMARY_BUTTON_BG);
        let blank = self.mixed && {
            let state = self.state.clone();
            let key = ElementId::from(SharedString::from(format!("slider-mixed-{}", state.entity_id().as_u64())));
            let touch = window.use_keyed_state(key, cx, move |window, cx| MixedTouch {
                touched: false,
                _subscription: cx.subscribe_in(&state, window, |this: &mut MixedTouch, _, event: &SliderEvent, _, cx| {
                    if matches!(event, SliderEvent::Change(_)) {
                        this.touched = true;
                        cx.notify();
                    }
                }),
            });
            !touch.read(cx).touched
        };
        let track = Slider::new(&self.state).disabled(self.disabled).bg(primary);
        // Mixed and not moved: the real slider stays on top (invisible, it still takes the drag and the click) and the screen
        // shows a groove split at 50 % (the left half a little darker grey, the right half as the ordinary right groove) with
        // a ring for the thumb (no fill). The first move sets a value and draws the ordinary slider.
        let track = if blank {
            let right = primary.opacity(0.2);
            let left = rgb(0x9aa0a6);
            div().relative().w_full().h(px(16.))
                .child(div().absolute().left_0().top(px(6.)).w_1_2().h(px(4.)).rounded_l_full().bg(left))
                .child(div().absolute().right_0().top(px(6.)).w_1_2().h(px(4.)).rounded_r_full().bg(right))
                .child(div().absolute().left_1_2().top(px(1.)).ml(px(-7.)).size(px(14.)).rounded_full().border_2().border_color(left))
                .child(div().absolute().inset_0().opacity(0.).child(track))
        } else if self.disabled {
            // The grooves are the ordinary ones (the primary, and the primary at 20 % over the surface), the thumb is the
            // ordinary one's size, with an opaque surface and a grey ring; a grey cover of half transparency is laid over
            // each groove (alpha compositing, in the chrome's own palette, so the dark appearance follows).
            let surface = crate::palette::white();
            let grey = crate::palette::over(surface, disabled_color());
            let cover = Hsla { a: DISABLED_COVER_ALPHA, ..grey };
            let left = crate::palette::over(primary.into(), cover);
            let right = crate::palette::over(crate::palette::over(surface, Hsla::from(primary).opacity(0.2)), cover);
            let at = self.state.read(cx).percentage().end;
            let top = (ROW_HEIGHT - GROOVE_HEIGHT) / 2.;
            div().relative().w_full().h(ROW_HEIGHT)
                .child(div().absolute().left_0().top(top).w(relative(at)).h(GROOVE_HEIGHT).rounded_l_full().bg(left))
                .child(div().absolute().right_0().top(top).w(relative(1. - at)).h(GROOVE_HEIGHT).rounded_r_full().bg(right))
                .child(div().absolute().left(relative(at)).top((ROW_HEIGHT - THUMB_SIZE) / 2.).ml(-THUMB_SIZE / 2.).size(THUMB_SIZE).p(px(1.)).rounded_full().bg(grey)
                    .child(div().size_full().rounded_full().bg(surface)))
                .child(div().absolute().inset_0().opacity(0.).child(track))
        } else {
            div().w_full().child(track)
        };
        if self.editor.is_none() {
            return div().w_full().flex_1().min_w(self.track_min).child(track).into_any_element();
        }
        let key = ElementId::from(SharedString::from(format!("value-slider-{}", self.state.entity_id().as_u64())));
        let (id, state) = (self.id.clone(), self.state.clone());
        let view = window.use_keyed_state(key, cx, move |window, cx| {
            let input = cx.new(|cx| InputState::new(window, cx));
            let subscriptions = vec![
                cx.subscribe_in(&input, window, |this: &mut ValueView, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) { this.commit(window, cx); }
                }),
                // The number follows the track while it is dragged.
                cx.observe(&state, |_, _, cx| cx.notify()),
            ];
            ValueView { id, state, unit: None, unit_width: px(18.), disabled: false, editing: false, blank: false, given: String::new(), input, _subscriptions: subscriptions }
        });
        view.update(cx, |v, _| {
            v.unit = self.unit;
            v.unit_width = self.unit_width;
            v.disabled = self.disabled;
            v.blank = blank;
        });
        h_flex().w_full().items_center().gap(GAP)
            .child(view)
            .child(div().flex_1().min_w(self.track_min).child(track))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn decimals_follow_the_step() {
        assert_eq!(decimals_of(1.), 0);
        assert_eq!(decimals_of(0.1), 1);
        assert_eq!(decimals_of(0.25), 2);
        assert_eq!(decimals_of(0.01), 2);
    }

    #[core::prelude::v1::test]
    fn value_text_has_the_unit() {
        assert_eq!(value_text(100., 1., Some("%")), "100%");
        assert_eq!(value_text(3.5, 0.1, Some(" cm")), "3.5 cm");
        assert_eq!(value_text(7., 1., None), "7");
    }
}
