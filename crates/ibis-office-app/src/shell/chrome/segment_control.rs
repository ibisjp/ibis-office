// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A closed segmented-control API: its track either hugs its items or is fully apportioned.
use super::{TEXT, action_button};
use crate::shell::palette::rgb;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{Disableable as _, Selectable as _, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;

// These are visual padding/gap tokens, not text measurement approximations.
const TRACK_PADDING: f32 = 3.;
const GAP: f32 = 2.;
// Matches the existing small action button horizontal inset (8 px per side).
const ITEM_PADDING: f32 = 8.;

#[derive(Clone, Copy, Debug)]
pub(crate) enum Mode {
    Intrinsic,
    Proportional { width: Pixels },
}

type Content = Rc<dyn Fn() -> AnyElement>;
type Click = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Content is private so a caller cannot turn an item into an independent button.
#[derive(Clone)]
pub(crate) struct SegmentItem {
    pub(super) id: ElementId,
    content: Content,
    icon_only: bool,
    selected: bool,
    disabled: bool,
    tooltip: Option<SharedString>,
    accessibility_label: Option<SharedString>,
    click: Option<Click>,
    bounds: Option<Rc<dyn Fn(Bounds<Pixels>)>>,
}
impl SegmentItem {
    pub(crate) fn label(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        selected: bool,
    ) -> Self {
        let label = label.into();
        let name = label.clone();
        let mut item = Self::custom(
            id,
            move || {
                div()
                    .whitespace_nowrap()
                    .child(label.clone())
                    .into_any_element()
            },
            selected,
        );
        item.tooltip = Some(name.clone());
        item.accessibility_label = Some(name);
        item
    }
    pub(crate) fn icon(
        id: impl Into<ElementId>,
        content: impl Fn() -> AnyElement + 'static,
        selected: bool,
    ) -> Self {
        let mut item = Self::custom(id, content, selected);
        item.icon_only = true;
        item
    }
    /// The factory is evaluated for GPUI max-content measurement and for the visible item.
    /// It must create the same content each time, without changing application state.
    pub(crate) fn custom(
        id: impl Into<ElementId>,
        content: impl Fn() -> AnyElement + 'static,
        selected: bool,
    ) -> Self {
        Self {
            id: id.into(),
            content: Rc::new(content),
            icon_only: false,
            selected,
            disabled: false,
            tooltip: None,
            accessibility_label: None,
            click: None,
            bounds: None,
        }
    }
    pub(crate) fn on_bounds(mut self, observe: impl Fn(Bounds<Pixels>) + 'static) -> Self {
        self.bounds = Some(Rc::new(observe));
        self
    }
    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub(crate) fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }
    pub(crate) fn on_click(
        mut self,
        click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.click = Some(Rc::new(click));
        self
    }
}

#[derive(IntoElement)]
pub(crate) struct SegmentControl {
    mode: Mode,
    items: Vec<SegmentItem>,
    available: Option<Pixels>,
}
impl SegmentControl {
    pub(crate) fn new(mode: Mode) -> Self {
        Self {
            mode,
            items: Vec::new(),
            available: None,
        }
    }
    pub(crate) fn available_width(mut self, width: Pixels) -> Self {
        self.available = Some(width);
        self
    }
    pub(crate) fn child(mut self, item: SegmentItem) -> Self {
        self.items.push(item);
        self
    }
    pub(crate) fn children(mut self, items: impl IntoIterator<Item = SegmentItem>) -> Self {
        self.items.extend(items);
        self
    }
}

fn widths(natural: &[f32], mode: Mode) -> (f32, Vec<f32>) {
    let decoration = TRACK_PADDING * 2. + GAP * natural.len().saturating_sub(1) as f32;
    match mode {
        Mode::Intrinsic => {
            let items: Vec<_> = natural.iter().map(|w| w + ITEM_PADDING * 2.).collect();
            (decoration + items.iter().sum::<f32>(), items)
        }
        Mode::Proportional { width } => {
            let width = f32::from(width).max(0.);
            let available = (width - decoration).max(0.);
            let sum = natural.iter().sum::<f32>();
            let mut items: Vec<_> = natural
                .iter()
                .map(|w| {
                    if sum > 0. {
                        available * w / sum
                    } else {
                        available / natural.len().max(1) as f32
                    }
                })
                .collect();
            if let Some((last, preceding)) = items.split_last_mut() {
                *last = (available - preceding.iter().sum::<f32>()).max(0.);
            }
            (width, items)
        }
    }
}
fn content_width(icon_only: bool, width: f32) -> f32 {
    if icon_only {
        width.max(0.)
    } else {
        (width - ITEM_PADDING * 2.).max(0.)
    }
}
fn centered(icon_only: bool, natural: f32, available: f32) -> bool {
    icon_only || natural <= available
}

impl SegmentControl {
    /// GPUI max-content measurement, shared by layout and responsive row decisions.
    pub(crate) fn intrinsic_width(&self, window: &mut Window, cx: &mut App) -> Pixels {
        px(widths(&self.natural_widths(window, cx), Mode::Intrinsic).0)
    }
    fn natural_widths(&self, window: &mut Window, cx: &mut App) -> Vec<f32> {
        self.items
            .iter()
            .map(|item| {
                let mut probe = div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .flex_none()
                    .child((item.content)())
                    .into_any_element();
                f32::from(
                    probe
                        .layout_as_root(
                            size(AvailableSpace::MaxContent, AvailableSpace::MaxContent),
                            window,
                            cx,
                        )
                        .width,
                )
            })
            .collect()
    }
}

impl RenderOnce for SegmentControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if self.items.is_empty() {
            return div().into_any_element();
        }
        let natural = self.natural_widths(window, cx);
        let limit = self
            .available
            .map(f32::from)
            .unwrap_or_else(|| match self.mode {
                Mode::Proportional { width } => f32::from(width),
                Mode::Intrinsic => f32::MAX,
            });
        let requested = match self.mode {
            Mode::Proportional { width } => Mode::Proportional {
                width: px(f32::from(width).min(limit)),
            },
            mode => mode,
        };
        let (_, proportional) = widths(&natural, requested);
        let fallback = matches!(requested, Mode::Proportional { .. })
            && natural
                .iter()
                .zip(&proportional)
                .zip(&self.items)
                .any(|((n, w), item)| *n > content_width(item.icon_only, *w));
        let mode = if fallback { Mode::Intrinsic } else { requested };
        let (width, allocations) = widths(&natural, mode);
        let width = width.min(limit);
        let mut cursor = (TRACK_PADDING, TRACK_PADDING);
        let mut target = None;
        for (item, allocation) in self.items.iter().zip(&allocations) {
            let w = allocation.min((limit - TRACK_PADDING * 2.).max(0.));
            if cursor.0 > TRACK_PADDING && cursor.0 + w > width - TRACK_PADDING {
                cursor = (TRACK_PADDING, cursor.1 + super::BUTTON_HEIGHT + GAP);
            }
            if item.selected && target.is_none() {
                target = Some([cursor.0, cursor.1, w, super::BUTTON_HEIGHT]);
            }
            cursor.0 += w + GAP;
        }
        let target = target.unwrap_or([0.; 4]);
        let key = SharedString::from(format!("segment-motion-{:?}", self.items[0].id));
        let motion = window.use_keyed_state(key, cx, |_, _| {
            std::cell::RefCell::new(SelectionMotion {
                from: target,
                to: target,
                started: crate::clock::Instant::now(),
            })
        });
        let (at, animating) = {
            let mut m = motion.read(cx).borrow_mut();
            let (at, _) = m.at();
            if m.to != target {
                m.from = at;
                m.to = target;
                m.started = crate::clock::Instant::now();
            }
            if cx.reduce_motion() {
                (target, false)
            } else {
                m.at()
            }
        };
        if animating {
            window.request_animation_frame();
        }
        let probe_id = format!("segment-highlight-{:?}", self.items[0].id);
        h_flex()
            .relative()
            .child(
                div()
                    .absolute()
                    .left(px(at[0]))
                    .top(px(at[1]))
                    .w(px(at[2]))
                    .h(px(at[3]))
                    .rounded(px(5.))
                    .bg(rgb(0xffffff))
                    .shadow(vec![BoxShadow {
                        color: gpui_kit::black().opacity(0.18),
                        offset: point(px(1.), px(1.)),
                        blur_radius: px(1.),
                        spread_radius: px(0.),
                        inset: false,
                    }])
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |b, _, _, _| {
                                #[cfg(all(
                                    feature = "scenarios",
                                    any(target_os = "macos", target_os = "linux")
                                ))]
                                crate::shell::scenario::button_probe::record_bounds(&probe_id, b);
                            },
                        )
                        .absolute()
                        .inset_0(),
                    ),
            )
            .self_start()
            .flex_none()
            .w(px(width))
            .min_w_0()
            .max_w_full()
            .flex_wrap()
            .items_center()
            .gap(px(GAP))
            .p(px(TRACK_PADDING))
            .rounded(px(8.))
            .bg(rgb(0xeeeeee))
            .overflow_hidden()
            .children(self.items.into_iter().zip(natural).zip(allocations).map(
                |((item, natural), width)| {
                    let width = width.min((limit - TRACK_PADDING * 2.).max(0.));
                    let available = content_width(item.icon_only, width);
                    let content = h_flex()
                        .text_size(px(14.))
                        .font_weight(FontWeight::MEDIUM)
                        .w(px(available))
                        .min_w(px(available))
                        .flex_none()
                        .overflow_hidden()
                        .items_center()
                        .when(centered(item.icon_only, natural, available), |b| {
                            b.justify_center()
                        })
                        .when(!centered(item.icon_only, natural, available), |b| {
                            b.justify_start()
                        })
                        .child(
                            div()
                                .flex_none()
                                .w(px(natural))
                                .whitespace_nowrap()
                                .child((item.content)()),
                        );
                    let content =
                        div()
                            .relative()
                            .child(content)
                            .when_some(item.bounds, |d, observe| {
                                d.child(
                                    canvas(|_, _, _| (), move |bounds, _, _, _| observe(bounds))
                                        .absolute()
                                        .inset_0(),
                                )
                            });
                    let mut button = action_button(item.id)
                        .small()
                        .ghost()
                        .selected(item.selected)
                        .disabled(item.disabled)
                        .w(px(width))
                        .min_w(px(0.))
                        .max_w(px(width))
                        .px(px(0.))
                        .flex_none()
                        .overflow_hidden()
                        .rounded(px(5.))
                        .text_size(px(14.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(TEXT))
                        .bg(gpui_kit::transparent_black())
                        .hover(|s| s.bg(gpui_kit::transparent_black()))
                        .child(content);
                    if let Some(name) = item.accessibility_label.or_else(|| item.tooltip.clone()) {
                        button = button.accessibility_label(name);
                    }
                    if let Some(tooltip) = item.tooltip {
                        button = button.tooltip(tooltip);
                    }
                    if let Some(click) = item.click {
                        button = button.on_click(move |event, window, cx| click(event, window, cx));
                    }
                    button
                },
            ))
            .into_any_element()
    }
}

#[cfg(any(test, feature = "scenarios"))]
#[path = "../../tests/shell/segment_control_layout.rs"]
pub(crate) mod tests;

struct SelectionMotion {
    from: [f32; 4],
    to: [f32; 4],
    started: crate::clock::Instant,
}
impl SelectionMotion {
    fn at(&self) -> ([f32; 4], bool) {
        let t = (self.started.elapsed().as_secs_f32() / 0.18).min(1.);
        let eased = 1. - (1. - t).powi(3);
        (
            std::array::from_fn(|i| self.from[i] + (self.to[i] - self.from[i]) * eased),
            t < 1.,
        )
    }
}
