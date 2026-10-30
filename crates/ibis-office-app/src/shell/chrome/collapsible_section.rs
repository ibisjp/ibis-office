// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Full-width section headers and animated reveal in their owning scroll view.
use super::*;
use std::cell::Cell;
use std::time::Duration;

#[derive(Default)]
pub(crate) struct SectionState {
    pub reveal: Cell<bool>,
    pub generation: Cell<u64>,
    pub bounds: Cell<Option<Bounds<Pixels>>>,
}

pub(crate) fn reveal_offset(
    section: Bounds<Pixels>,
    viewport: Bounds<Pixels>,
    offset: Pixels,
) -> Pixels {
    if section.origin.y < viewport.origin.y || section.size.height > viewport.size.height {
        offset + viewport.origin.y - section.origin.y
    } else if section.bottom() > viewport.bottom() {
        offset + viewport.bottom() - section.bottom()
    } else {
        offset
    }
}

pub(crate) fn collapsible_section(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    open: bool,
    body: AnyElement,
    state: Rc<SectionState>,
    scroll: ScrollHandle,
    toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let click_state = state.clone();
    v_flex()
        .w_full()
        .gap_2()
        .child(
            div()
                .id(id)
                .w_full()
                .h(px(34.))
                .flex()
                .items_center()
                .gap_1()
                .bg(crate::shell::palette::shade(0., 0.06))
                .hover(|s| s.bg(crate::shell::palette::shade(0., 0.10)))
                .cursor_pointer()
                .child(icon(
                    if open {
                        "arrow_drop_down"
                    } else {
                        "arrow_right"
                    },
                    20.,
                    rgb(TEXT).into(),
                ))
                .child(label.into())
                .on_click(move |event, w, cx| {
                    click_state.reveal.set(!open);
                    click_state.generation.set(click_state.generation.get() + 1);
                    toggle(event, w, cx);
                }),
        )
        .when(open, |d| d.child(body))
        .relative()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, cx| {
                    state.bounds.set(Some(bounds));
                    if !open || !state.reveal.replace(false) {
                        return;
                    }
                    let offset = scroll.offset();
                    let target = reveal_offset(bounds, scroll.bounds(), offset.y)
                        .clamp(-scroll.max_offset().y, px(0.));
                    if target == offset.y {
                        return;
                    }
                    let generation = state.generation.get();
                    window
                        .spawn(cx, async move |cx| {
                            // Twelve frames over 180ms; easing affects motion only, never geometry.
                            for frame in 1..=12 {
                                cx.background_executor()
                                    .timer(Duration::from_millis(15))
                                    .await;
                                if state.generation.get() != generation {
                                    break;
                                }
                                let progress = frame as f32 / 12.;
                                let eased = 1. - (1. - progress).powi(3);
                                scroll.set_offset(point(
                                    offset.x,
                                    offset.y + (target - offset.y) * eased,
                                ));
                                let _ = cx.update(|w, _| w.refresh());
                            }
                        })
                        .detach();
                },
            )
            .absolute()
            .inset_0(),
        )
}
