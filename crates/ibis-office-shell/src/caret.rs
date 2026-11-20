// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Shared editing-caret timing, refreshed twice per second only while painted.
use gpui_kit::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
struct Blink {
    activity: String,
    changed: crate::clock::Instant,
    seen: Rc<Cell<crate::clock::Instant>>,
    task: Option<Task<()>>,
}
pub fn visible(activity: impl Into<String>, window: &mut Window, cx: &mut App) -> bool {
    let activity = activity.into();
    let state = window.use_keyed_state("editing-caret-clock", cx, |_, _| {
        RefCell::new(Blink {
            activity: String::new(),
            changed: crate::clock::Instant::now(),
            seen: Rc::new(Cell::new(crate::clock::Instant::now())),
            task: None,
        })
    });
    let mut state = state.read(cx).borrow_mut();
    if state.task.is_none() || state.seen.get().elapsed() > Duration::from_millis(1100) {
        let seen = state.seen.clone();
        state.task = Some(window.spawn(cx, async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(500))
                    .await;
                if seen.get().elapsed() > Duration::from_millis(1100) {
                    break;
                }
                if cx.update(|w, _| w.refresh()).is_err() {
                    break;
                }
            }
        }));
    }
    state.seen.set(crate::clock::Instant::now());
    if state.activity != activity {
        state.activity = activity;
        state.changed = crate::clock::Instant::now();
    }
    state.changed.elapsed().as_millis() % 1000 < 500
}

#[derive(IntoElement)]
pub struct Caret {
    child: Div,
    activity: String,
    focus: FocusHandle,
}
pub fn element(child: Div, activity: impl Into<String>, focus: FocusHandle) -> Caret {
    Caret {
        child,
        activity: activity.into(),
        focus,
    }
}
impl RenderOnce for Caret {
    fn render(self, w: &mut Window, cx: &mut App) -> impl IntoElement {
        self.child.opacity(
            if self.focus.is_focused(w) && visible(self.activity, w, cx) {
                1.
            } else {
                0.
            },
        )
    }
}

/// The colour of an editing caret over `background` (the colour under it: a shape's fill, a slide's or a page's, a cell's), `fallback` when that is not known:
/// black over a light one, white over a dark one (Rec.709 luma, [`ibis_doc_model::Rgb::caret_over`]). It does not follow the appearance of the screen (a dark
/// screen has white slides, pages and cells all the same): all three formats' own-drawn carets take it.
pub fn color_over(background: Option<ibis_doc_model::Rgb>, fallback: ibis_doc_model::Rgb) -> Hsla {
    rgb(background.unwrap_or(fallback).caret_over().0).into()
}

/// `color_over` for a background that is a theme colour (the formula bar's, a field's): the colour of the caret over it.
pub fn color_over_theme(background: Hsla) -> Hsla {
    let c: Rgba = background.into();
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    color_over(Some(ibis_doc_model::Rgb::from_rgb(byte(c.r), byte(c.g), byte(c.b))), ibis_doc_model::Rgb::WHITE)
}
