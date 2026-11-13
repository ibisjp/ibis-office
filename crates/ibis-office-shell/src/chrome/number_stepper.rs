// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A number with [-] [typed value + unit] [v candidates] [+]: the toolbar's font size control as a common part for panels
//! (a duration, a delay). `plain()` takes the white background of the value box away (the panel's own background shows).
use super::*;
use gpui_kit::component::input::{InputEvent, InputState};

type Change = Rc<dyn Fn(&mut SlideEditor, f32, &mut Window, &mut Context<SlideEditor>)>;

#[derive(IntoElement)]
pub struct NumberStepper {
    id: SharedString,
    value: f32,
    step: f32,
    min: f32,
    max: f32,
    decimals: usize,
    unit: SharedString,
    candidates: &'static [f32],
    plain: bool,
    editor: WeakEntity<SlideEditor>,
    changed: Change,
    /// The input is owned by the caller (the toolbar's font size): the stepper neither fills it nor listens to it.
    bound: Option<Entity<InputState>>,
    disabled: bool,
    value_width: Pixels,
    tooltips: Option<(SharedString, SharedString)>,
    /// [-] and [+] call these instead of stepping `value` (a bound stepper).
    on_minus: Option<Handler>,
    on_plus: Option<Handler>,
    /// Tab and Shift+Tab in the value box call this (and stay in the box).
    on_commit: Option<Handler>,
    popup: Option<StepperPopup>,
}

/// A candidate list the caller owns (it opens and closes it, and draws it): the caret toggles it.
#[derive(Clone)]
pub struct StepperPopup {
    pub open: bool,
    pub toggle: Rc<dyn Fn(&mut Window, &mut App)>,
    pub(crate) open_changed: Rc<dyn Fn(bool, &mut Window, &mut App)>,
    pub content: Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>,
}

pub fn number_stepper(
    id: impl Into<SharedString>,
    value: f32,
    cx: &Context<SlideEditor>,
    changed: impl Fn(&mut SlideEditor, f32, &mut Window, &mut Context<SlideEditor>) + 'static,
) -> NumberStepper {
    NumberStepper { id: id.into(), value, step: 0.5, min: 0., max: f32::MAX, decimals: 1, unit: SharedString::default(), candidates: &[], plain: false, editor: cx.weak_entity(), changed: Rc::new(changed), bound: None, disabled: false, value_width: px(72.), tooltips: None, on_minus: None, on_plus: None, on_commit: None, popup: None }
}

/// A stepper over an input the caller owns (and keeps in step with its value): [`NumberStepper::on_step`] says what [-] and [+] do.
pub fn bound_number_stepper(id: impl Into<SharedString>, input: &Entity<InputState>, editor: &WeakEntity<SlideEditor>) -> NumberStepper {
    NumberStepper { id: id.into(), value: 0., step: 0.5, min: 0., max: f32::MAX, decimals: 1, unit: SharedString::default(), candidates: &[], plain: false, editor: editor.clone(), changed: Rc::new(|_, _, _, _| {}), bound: Some(input.clone()), disabled: false, value_width: px(72.), tooltips: None, on_minus: None, on_plus: None, on_commit: None, popup: None }
}

/// The text a value is shown as (fixed decimals).
pub(crate) fn stepper_text(value: f32, decimals: usize) -> String { format!("{value:.decimals$}") }

/// The value `text` types (a number, a unit after it is ignored), clamped to the range; `None` when it is not a number.
pub(crate) fn parse_stepper_text(text: &str, min: f32, max: f32) -> Option<f32> {
    let t = text.trim();
    let end = t.char_indices().find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || (c == '-' && i == 0))).map_or(t.len(), |(i, _)| i);
    t[..end].parse::<f32>().ok().filter(|v| v.is_finite()).map(|v| v.clamp(min, max))
}

impl NumberStepper {
    pub fn step(mut self, step: f32) -> Self { self.step = step; self }
    pub fn range(mut self, min: f32, max: f32) -> Self { self.min = min; self.max = max; self }
    #[allow(dead_code)] // Shared-control option; no field asks for it yet.
    pub(crate) fn decimals(mut self, decimals: usize) -> Self { self.decimals = decimals; self }
    pub fn unit(mut self, unit: impl Into<SharedString>) -> Self { self.unit = unit.into(); self }
    pub fn candidates(mut self, candidates: &'static [f32]) -> Self { self.candidates = candidates; self }
    /// No background colour in the value box.
    pub fn plain(mut self) -> Self { self.plain = true; self }
    pub fn disabled(mut self, disabled: bool) -> Self { self.disabled = disabled; self }
    pub(crate) fn value_width(mut self, width: Pixels) -> Self { self.value_width = width; self }
    pub(crate) fn tooltips(mut self, minus: impl Into<SharedString>, plus: impl Into<SharedString>) -> Self { self.tooltips = Some((minus.into(), plus.into())); self }
    /// What [-] and [+] do (`None`: the button is disabled).
    pub(crate) fn on_step(mut self, minus: Option<Handler>, plus: Option<Handler>) -> Self { self.on_minus = minus; self.on_plus = plus; self }
    pub(crate) fn on_commit(mut self, commit: Handler) -> Self { self.on_commit = Some(commit); self }
    pub(crate) fn popup(mut self, popup: StepperPopup) -> Self { self.popup = Some(popup); self }
}

struct Subscribed(Option<Subscription>);

impl RenderOnce for NumberStepper {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let text = stepper_text(self.value, self.decimals);
        let initial = text.clone();
        let bound = self.bound.is_some();
        let owned = window.use_keyed_state(SharedString::from(format!("{}-input", self.id)), cx, move |w, cx| {
            let mut i = InputState::new(w, cx);
            i.set_value(initial, w, cx);
            i
        });
        let input = self.bound.clone().unwrap_or_else(|| owned.clone());
        let subscribed = input.clone();
        let (editor, changed, min, max) = (self.editor.clone(), self.changed.clone(), self.min, self.max);
        let _subscription = window.use_keyed_state(SharedString::from(format!("{}-subscription", self.id)), cx, move |w, cx| {
            Subscribed((!bound).then(|| editor.update(cx, |_, cx| cx.subscribe_in(&subscribed, w, move |e, input, event: &InputEvent, w, cx| {
                if matches!(event, InputEvent::Change | InputEvent::PressEnter { .. }) {
                    if let Some(v) = parse_stepper_text(input.read(cx).value().as_ref(), min, max) { changed(e, v, w, cx); }
                }
            })).ok()).flatten())
        });
        let _ = _subscription.read(cx).0.is_some();
        let focused = input.read(cx).focus_handle(cx).is_focused(window);
        let typed = parse_stepper_text(input.read(cx).value().as_ref(), self.min, self.max);
        if !bound && !focused && typed.map_or(true, |t| stepper_text(t, self.decimals) != text) {
            input.update(cx, |i, cx| i.set_value(text.clone(), window, cx));
        }
        let (decimals, step, value, lo, hi) = (self.decimals, self.step, self.value, self.min, self.max);
        let stepped = |sign: f32| {
            let next = ((value + sign * step).clamp(lo, hi) * 10f32.powi(decimals as i32)).round() / 10f32.powi(decimals as i32);
            next
        };
        let button = |suffix: &str, name: &'static str, sign: f32, disabled: bool| {
            let (editor, changed) = (self.editor.clone(), self.changed.clone());
            let next = stepped(sign);
            let handler = if sign < 0. { self.on_minus.clone() } else { self.on_plus.clone() };
            let disabled = self.disabled || if bound { handler.is_none() } else { disabled };
            let tip = self.tooltips.as_ref().map(|(minus, plus)| if sign < 0. { minus.clone() } else { plus.clone() });
            action_button(SharedString::from(format!("{}-{suffix}", self.id)))
                .ghost().xsmall().w(px(26.)).h(px(28.)).px_0().disabled(disabled)
                .when_some(tip, |b, tip| b.tooltip(tip))
                .child(icon(name, 16., if disabled { disabled_color() } else { rgb(TEXT).into() }))
                .on_click(move |_, w, cx| {
                    if bound {
                        if let Some(handler) = handler.clone() { let _ = editor.update(cx, |e, cx| handler(e, w, cx)); }
                    } else { let changed = changed.clone(); let _ = editor.update(cx, |e, cx| changed(e, next, w, cx)); }
                })
        };
        let open = window.use_keyed_state(SharedString::from(format!("{}-open", self.id)), cx, |_, _| false);
        let is_open = *open.read(cx);
        let border = if focused { PRIMARY_BUTTON_BG } else { BORDER };
        let value_box = div()
            .id(SharedString::from(format!("{}-box", self.id)))
            .relative().flex().flex_none().items_center().w(self.value_width).h(px(26.)).px_1().gap_1()
            .border_1().border_color(rgb(border)).rounded(px(4.))
            .when(!self.plain, |d| d.bg(rgb(0xffffff)))
            .when_some(self.on_commit.clone(), |d, commit| {
                let (a, b, c) = (commit.clone(), commit.clone(), commit);
                let (e1, e2, e3) = (self.editor.clone(), self.editor.clone(), self.editor.clone());
                d.capture_action(move |_: &gpui_kit::component::input::IndentInline, w, cx| {
                    let _ = e1.update(cx, |e, cx| a(e, w, cx));
                    cx.stop_propagation();
                })
                .capture_action(move |_: &gpui_kit::component::input::OutdentInline, w, cx| {
                    let _ = e2.update(cx, |e, cx| b(e, w, cx));
                    cx.stop_propagation();
                })
                .capture_key_down(move |event: &KeyDownEvent, w, cx| {
                    if event.keystroke.key == "tab" {
                        let _ = e3.update(cx, |e, cx| c(e, w, cx));
                        w.prevent_default();
                        cx.stop_propagation();
                    }
                })
            })
            // The first press takes the focus and selects the text; a press in a focused box puts the caret (as Office's fields do).
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .capture_any_mouse_down({
                let (field, owner) = (input.clone(), self.editor.clone());
                move |_, w, cx| {
                    let select_all = !field.read(cx).focus_handle(cx).is_focused(w);
                    let (field, owner) = (field.clone(), owner.clone());
                    w.defer(cx, move |w, cx| {
                        field.update(cx, |i, cx| {
                            i.focus(w, cx);
                            if select_all { i.select_all(w, cx); }
                        });
                        let _ = owner.update(cx, |_, cx| cx.notify());
                    });
                }
            })
            .child(canvas(|_, _, _| (), {
                #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
                let key = format!("{}-edit", self.id);
                move |bounds, _, _, _| {
                    #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
                    crate::scenario::button_probe::record_bounds(&key, bounds);
                    #[cfg(not(all(feature = "scenarios", any(target_os = "macos", target_os = "linux"))))]
                    let _ = bounds;
                }
            }).absolute().inset_0())
            .child(div().flex_1().min_w_0().child(gpui_kit::component::input::Input::new(&input).small().appearance(false).px_0().cleanable(false).disabled(self.disabled)))
            .when(!self.unit.is_empty(), |d| d.child(div().flex_none().text_sm().text_color(rgb(TEXT_SECONDARY)).child(self.unit.clone())));
        let candidates = self.candidates;
        let custom = self.popup.clone().map(|popup| {
            let disabled = self.disabled;
            let toggle = popup.toggle.clone();
            let (content, open_changed) = (popup.content.clone(), popup.open_changed.clone());
            let anchor = Popover::new(SharedString::from(format!("{}-popover", self.id)))
                .anchor(Anchor::TopLeft)
                .open(popup.open)
                .trigger(Button::new(SharedString::from(format!("{}-anchor", self.id))).ghost().xsmall().p_0().w(px(0.)).min_w(px(0.)).h(px(28.)))
                .on_open_change(move |o, w, cx| open_changed(*o, w, cx))
                .content(move |_, w, cx| content(w, cx))
                .into_any_element();
            let caret = action_button(SharedString::from(format!("{}-caret", self.id))).ghost().xsmall().w(px(TOOLBAR_CARET_WIDTH)).h(px(28.)).px_0()
                .disabled(disabled)
                .child(icon("arrow_drop_down", 16., if disabled { disabled_color() } else { rgb(TEXT).into() }))
                .on_click(move |_, w, cx| toggle(w, cx))
                .into_any_element();
            (anchor, caret)
        });
        let caret = (!candidates.is_empty() && custom.is_none()).then(|| {
            let (editor, changed, open_set, id, unit, dec) = (self.editor.clone(), self.changed.clone(), open.clone(), self.id.clone(), self.unit.clone(), self.decimals);
            let open_toggle = open.clone();
            Popover::new(SharedString::from(format!("{}-popover", self.id)))
                .anchor(Anchor::TopLeft)
                .open(is_open)
                .trigger(
                    action_button(SharedString::from(format!("{}-caret", self.id))).ghost().xsmall().w(px(TOOLBAR_CARET_WIDTH)).h(px(28.)).px_0()
                        .child(icon("arrow_drop_down", 16., rgb(TEXT).into()))
                        .on_click(move |_, _, cx| { open_toggle.update(cx, |o, cx| { *o = !*o; cx.notify(); }); }),
                )
                .on_open_change({ let open = open.clone(); move |o, _, cx| { open.update(cx, |s, cx| { *s = *o; cx.notify(); }); } })
                .content(move |_, _, _| {
                    v_flex().w(px(96.)).p(px(4.)).on_mouse_down_stop().children(candidates.iter().enumerate().map(|(n, &c)| {
                        let (editor, changed, open_set) = (editor.clone(), changed.clone(), open_set.clone());
                        dropdown_list::option((SharedString::from(format!("{id}-candidate")), n), false, false, true)
                            .h(px(24.)).px(px(8.)).text_size(px(13.)).text_color(rgb(TEXT))
                            .child(format!("{} {unit}", stepper_text(c, dec)).trim().to_string())
                            .on_mouse_down_stop()
                            .on_click(move |_, w, cx| {
                                open_set.update(cx, |s, cx| { *s = false; cx.notify(); });
                                let changed = changed.clone();
                                let _ = editor.update(cx, |e, cx| changed(e, c, w, cx));
                            })
                            .into_any_element()
                    })).into_any_element()
                })
                .into_any_element()
        });
        let (anchor, custom_caret) = custom.unzip();
        h_flex().h(px(28.)).flex_none().items_center()
            .children(anchor)
            .child(button("minus", "remove", -1., self.value <= self.min))
            .child(value_box)
            .children(caret)
            .children(custom_caret)
            .child(button("plus", "add", 1., self.value >= self.max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn typed_text_is_a_number_clamped_to_the_range() {
        assert_eq!(parse_stepper_text("1.5", 0.1, 60.), Some(1.5));
        assert_eq!(parse_stepper_text(" 2.0 s", 0.1, 60.), Some(2.0));
        assert_eq!(parse_stepper_text("0", 0.1, 60.), Some(0.1));
        assert_eq!(parse_stepper_text("999", 0.1, 60.), Some(60.));
        assert_eq!(parse_stepper_text("", 0.1, 60.), None);
        assert_eq!(parse_stepper_text("abc", 0.1, 60.), None);
    }

    #[core::prelude::v1::test]
    fn value_text_has_fixed_decimals() {
        assert_eq!(stepper_text(1.0, 1), "1.0");
        assert_eq!(stepper_text(0.25, 2), "0.25");
    }
}
