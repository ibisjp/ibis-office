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
pub(crate) fn visible(activity: impl Into<String>, window: &mut Window, cx: &mut App) -> bool {
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
pub(crate) struct Caret {
    child: Div,
    activity: String,
    focus: FocusHandle,
}
pub(crate) fn element(child: Div, activity: impl Into<String>, focus: FocusHandle) -> Caret {
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
