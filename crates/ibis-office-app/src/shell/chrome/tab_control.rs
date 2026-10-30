// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! App-wide measured tabs: intrinsic scrolling tabs or equal tabs with intrinsic fallback.
use super::{PRIMARY_BUTTON_BG, action_button};
use crate::shell::palette::rgb;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::{h_flex,Sizable as _};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::{cell::Cell, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabMode {
    Intrinsic,
    Equal,
}
struct TabItem {
    label: SharedString,
    click: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>,
}
struct Indicator { from:(f32,f32), target:(f32,f32), started:crate::clock::Instant }
impl Indicator {
    fn value(&self)->((f32,f32),bool){let t=(self.started.elapsed().as_secs_f32()/0.18).min(1.);let t=1.-(1.-t).powi(3);((self.from.0+(self.target.0-self.from.0)*t,self.from.1+(self.target.1-self.from.1)*t),t<1.)}
}
struct Tabs {
    id: &'static str,
    items: Vec<TabItem>,
    selected: usize,
    mode: TabMode,
}
pub(crate) fn tab_control<T: 'static>(
    labels: impl IntoIterator<Item = SharedString>,
    selected: usize,
    id: &'static str,
    change: impl Fn(&mut T, usize, &mut Context<T>) + 'static,
    cx: &Context<T>,
) -> Div {
    tab_control_with_mode(labels, selected, id, TabMode::Equal, change, cx)
}
pub(crate) fn tab_control_with_mode<T: 'static>(
    labels: impl IntoIterator<Item = SharedString>,
    selected: usize,
    id: &'static str,
    mode: TabMode,
    change: impl Fn(&mut T, usize, &mut Context<T>) + 'static,
    cx: &Context<T>,
) -> Div {
    let change = Rc::new(change);
    let items = labels
        .into_iter()
        .enumerate()
        .map(|(i, label)| {
            let change = change.clone();
            TabItem {
                label,
                click: Rc::new(cx.listener(move |owner, _, _, cx| change(owner, i, cx))),
            }
        })
        .collect();
    div().w_full().min_w_0().child(Tabs {
        id,
        items,
        selected,
        mode,
    })
}
impl IntoElement for Tabs {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Tabs {
    type RequestLayoutState = (AnyElement, Rc<Cell<Pixels>>);
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        Some(SharedString::from(format!("tabs-layout-{}", self.id)).into())
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let measured = id
            .map(|id| {
                window.with_element_state(id, |old: Option<Rc<Cell<Pixels>>>, _| {
                    let v = old.unwrap_or_else(|| Rc::new(Cell::new(px(264.))));
                    (v.clone(), v)
                })
            })
            .unwrap_or_default();
        let available = f32::from(measured.get()).max(1.);
        let natural = self
            .items
            .iter()
            .map(|item| {
                let mut probe = div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .child(item.label.clone())
                    .into_any_element();
                f32::from(
                    probe
                        .layout_as_root(
                            size(AvailableSpace::MaxContent, AvailableSpace::MaxContent),
                            window,
                            cx,
                        )
                        .width,
                ) + 22.
            })
            .collect::<Vec<_>>();
        let equal = available / self.items.len().max(1) as f32;
        let intrinsic = self.mode == TabMode::Intrinsic || natural.iter().any(|w| *w > equal);
        let widths = natural
            .iter()
            .map(|w| if intrinsic { *w } else { equal })
            .collect::<Vec<_>>();
        let total = widths.iter().sum::<f32>();
        let max_scroll = (total - available).max(0.);
        let scroll = window
            .use_keyed_state(
                SharedString::from(format!("tabs-scroll-{}", self.id)),
                cx,
                |_, _| ScrollHandle::new(),
            )
            .read(cx)
            .clone();
        let wheel = scroll.clone();
        let selected=self.selected.min(widths.len().saturating_sub(1));
        let target=(widths.iter().take(selected).sum::<f32>(),widths.get(selected).copied().unwrap_or(0.));
        let indicator=window.use_keyed_state(SharedString::from(format!("tabs-indicator-{}",self.id)),cx,|_,_|std::cell::RefCell::new(Indicator{from:target,target,started:crate::clock::Instant::now()}));
        let (at,animating)={let mut state=indicator.read(cx).borrow_mut();let (current,_)=state.value();if state.target!=target{state.from=current;state.target=target;state.started=crate::clock::Instant::now();}if cx.reduce_motion(){(target,false)}else{state.value()}};
        if animating {window.request_animation_frame();}
        let mut strip = h_flex().relative()
            .w(px(total))
            .min_w(px(total))
            .flex_none()
            .items_stretch();
        for (i, (item, width)) in self.items.iter().zip(widths.iter().copied()).enumerate() {
            let click = item.click.clone();
            let slot = item.click.clone();
            strip = strip.child(
                div()
                    .id((
                        SharedString::from(format!("tab-control-slot-{}", self.id)),
                        i,
                    ))
                    .w(px(width))
                    .flex_none()
                    .relative()
                    .h(px(32.))
                    .hover(|s|s.bg(rgb(0xeeeeee)))
                    .on_click(move |e, w, cx| slot(e, w, cx))
                    .child(
                        action_button((self.id, i))
                            .small()
                            .ghost()
                            .h(px(32.))
                            .min_h(px(32.))
                            .max_h(px(32.))
                            .w_full()
                            .px(px(8.))
                            .rounded(px(0.))
                            .hover(|s|s.bg(gpui_kit::transparent_black()).text_color(rgb(if self.selected==i{PRIMARY_BUTTON_BG}else{0x1f1f1f})))
                            .text_size(px(14.))
                            .font_weight(FontWeight::MEDIUM)
                            .label(item.label.clone())
                            .tooltip(item.label.clone())
                            .text_color(rgb(if self.selected == i {
                                PRIMARY_BUTTON_BG
                            } else {
                                0x1f1f1f
                            }))
                            .on_click(move |e, w, cx| {
                                click(e, w, cx);
                                cx.stop_propagation();
                            }),
                    )
            );
        }
        let probe_id=format!("tab-indicator-{}",self.id);
        strip=strip.child(div().absolute().left(px(at.0)).w(px(at.1)).bottom_0().h(px(4.)).bg(rgb(PRIMARY_BUTTON_BG)).child(canvas(|_,_,_|(),move|bounds,_,_,_|{
            #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
            crate::shell::scenario::button_probe::record_bounds(&probe_id,bounds);
        }).absolute().inset_0()));
        let mut root = div()
            .relative()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .id(SharedString::from(format!("tabs-viewport-{}", self.id)))
                    .w_full()
                    .min_w_0()
                    .overflow_x_scroll()
                    .track_scroll(&scroll)
                    .on_scroll_wheel(move |e: &ScrollWheelEvent, w, cx| {
                        if max_scroll > 0. {
                            let delta = e.delta.pixel_delta(px(20.));
                            let dx = if delta.x != px(0.) { delta.x } else { delta.y };
                            wheel.set_offset(point(
                                (wheel.offset().x + dx).clamp(px(-max_scroll), px(0.)),
                                px(0.),
                            ));
                            w.refresh();
                            cx.stop_propagation();
                        }
                    })
                    .child(strip),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(1.))
                    .bg(rgb(PRIMARY_BUTTON_BG)),
            )
            .into_any_element();
        (root.request_layout(window, cx), (root, measured))
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if (state.1.get() - bounds.size.width).abs() > px(0.5) {
            state.1.set(bounds.size.width);
            window.refresh();
        }
        state.0.prepaint(window, cx);
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        state.0.paint(window, cx);
    }
}
