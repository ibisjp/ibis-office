// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! One content/action layout for every right pane: flowing when it fits, sticky when it overflows.
use super::*;
#[derive(Clone, Copy, Default)]
pub(crate) struct PanelMeasurements {
    pub body: f32,
    pub footer: f32,
    pub room: f32,
}
fn measure(
    slot: u8,
    state: Rc<std::cell::Cell<PanelMeasurements>>,
    editor: WeakEntity<SlideEditor>,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |b, _, _, cx| {
            let mut m = state.get();
            let value = f32::from(b.size.height);
            let old = match slot {
                0 => m.body,
                1 => m.footer,
                _ => m.room,
            };
            if (old - value).abs() > 0.5 {
                match slot {
                    0 => m.body = value,
                    1 => m.footer = value,
                    _ => m.room = value,
                };
                state.set(m);
                let e = editor.clone();
                cx.defer(move |cx| {
                    let _ = e.update(cx, |_, cx| cx.notify());
                });
            }
        },
    )
    .absolute()
    .inset_0()
}
pub(crate) fn panel_contents(
    body: AnyElement,
    pinned: Option<AnyElement>,
    footer: Option<crate::chrome::ActionBar>,
    scroll: ScrollHandle,
    state: Rc<std::cell::Cell<PanelMeasurements>>,
    editor: WeakEntity<SlideEditor>,
    fill: bool,
) -> Div {
    if fill {
        return fill_contents(body, footer);
    }
    let m = state.get();
    // A pinned part keeps the body scrolling above it, whatever the room.
    let sticky = pinned.is_some() || footer.is_none() || m.room == 0. || m.body + m.footer > m.room;
    let body = div()
        .relative()
        .w_full()
        .flex_none()
        .bg(rgb(0xffffff))
        .child(super::panel_body(body))
        .child(measure(0, state.clone(), editor.clone()));
    let body = div()
        .relative()
        .w_full()
        .min_h_0()
        .when(sticky, |d| d.flex_1())
        .when(!sticky, |d| d.flex_none())
        .child(
            div()
                .id("sidebar-body")
                .w_full()
                .when(sticky, |d| {
                    d.h_full().overflow_y_scroll().track_scroll(&scroll)
                })
                .child(body),
        )
        .when(sticky, |d| {
            d.child(always_visible_scrollbar_overlay(
                "sidebar-scrollbar",
                scroll,
                None,
            ))
        });
    let flow = v_flex()
        .w_full()
        .min_h_0()
        .when(sticky, |d| d.h_full())
        .child(body)
        .when_some(pinned, |d, p| {
            d.child(div().id("sidebar-pinned").w_full().flex_none().px(px(super::form::FORM_PADDING)).py(px(8.)).border_t_1().border_color(rgb(super::BORDER)).bg(rgb(0xffffff)).child(p))
        })
        .when_some(footer, |d, f| {
            d.child(
                action_bar_host(f.access_scope(crate::access_key::Scope::Off)).id("sidebar-footer")
                    .child(measure(1, state.clone(), editor.clone())),
            )
        });
    div()
        .relative()
        .flex_1()
        .min_h_0()
        .w_full()
        .overflow_hidden()
        .bg(rgb(0xf4f5f6))
        .child(flow)
        .child(measure(2, state, editor))
}
/// A pane with one table: no scrolling of the pane itself. The body is the pane's height less the footer, so the
/// table inside decides what scrolls.
fn fill_contents(body: AnyElement, footer: Option<crate::chrome::ActionBar>) -> Div {
    let flow = v_flex()
        .w_full()
        .h_full()
        .min_h_0()
        .child(
            div()
                .w_full()
                .flex_1()
                .min_h_0()
                .bg(rgb(0xffffff))
                .p(px(super::form::FORM_PADDING))
                .child(div().size_full().child(body)),
        )
        .when_some(footer, |d, f| d.child(action_bar_host(f.access_scope(crate::access_key::Scope::Off)).id("sidebar-footer")));
    div().relative().flex_1().min_h_0().w_full().overflow_hidden().bg(rgb(0xf4f5f6)).child(flow)
}
/// The height of the panel header area (a tab control: 32 px, no space above it: the title area has its own). Fixed, so a dialog can reserve it.
pub(crate) const PANEL_HEADER_HEIGHT: f32 = 32.;
/// The panel header area: under the title, above the body, never scrolling with it.
pub(crate) fn panel_header_host(header: AnyElement) -> Stateful<Div> {
    div().id("panel-header").w_full().h(px(PANEL_HEADER_HEIGHT)).flex_none().px(px(super::form::FORM_PADDING)).bg(rgb(0xffffff)).child(header)
}
pub(crate) fn panel_title(title: &str) -> SharedString {
    let translated = crate::i18n::text(title);
    let title = translated.trim_end();
    title
        .strip_suffix('…')
        .or_else(|| title.strip_suffix("..."))
        .unwrap_or(title)
        .trim_end()
        .to_string()
        .into()
}
