// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The scroll bar of a menu (2026-11-03): the one part that draws it and takes the pointer on it, for the menu a
//! menu bar opens and for every submenu (`menu_bar.rs`). The caller says where the track is (its bounds), which
//! lines are shown of how many, and what scrolling to a line does; this paints the thumb (darker while the pointer
//! is on the track or the thumb is dragged), and presses or drags on the track scroll to where the pointer is.
//! The track is always shown while the menu is cut. The size and place of the track are the same for all menus:
//! [`TRACK_WIDTH`] wide at the popup's right edge, the thumb [`THUMB_WIDTH`] wide and [`THUMB_GAP`] inside it.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::*;

/// The track: this wide, at the popup's right edge.
pub(crate) const TRACK_WIDTH: f32 = 12.0;
/// The thumb: this wide, and this far from the popup's right edge.
pub(crate) const THUMB_WIDTH: f32 = 7.0;
pub(crate) const THUMB_GAP: f32 = 2.0;
/// The thumb is never shorter than this.
const MIN_THUMB_HEIGHT: f32 = 16.0;

/// What a menu keeps between frames about its scroll bar.
#[derive(Default)]
pub(crate) struct ScrollbarState {
    /// The thumb is being dragged.
    pub dragging: Cell<bool>,
    /// The pointer was on the track when it was last drawn.
    hover: Cell<bool>,
}

/// What scrolling to a line does (the first line shown).
pub(crate) type ScrollTo = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// The lines `start..end` are shown of `total`; the track is `track` and the names are what the scenarios find the
/// track and the thumb by.
pub(crate) struct Scrollbar {
    pub track: Bounds<Pixels>,
    pub start: usize,
    pub end: usize,
    pub total: usize,
    pub state: Rc<ScrollbarState>,
    pub scroll_to: ScrollTo,
    #[allow(dead_code)] // Read by the scenarios' build only.
    pub probes: (&'static str, &'static str),
}

/// The thumb of lines `start..end` of `total` in `track`.
pub(crate) fn thumb_bounds(track: Bounds<Pixels>, start: usize, end: usize, total: usize) -> Bounds<Pixels> {
    let total = total.max(1);
    let height = f32::from(track.size.height);
    let shown = ((end - start) as f32 / total as f32).clamp(0.05, 1.0);
    let top = (start as f32 / total as f32).clamp(0.0, 1.0 - shown);
    let thumb_h = (height * shown).max(MIN_THUMB_HEIGHT).min(height.max(MIN_THUMB_HEIGHT));
    let travel = (height - thumb_h).max(0.0) * if shown < 1.0 { top / (1.0 - shown) } else { 0.0 };
    Bounds::new(
        point(track.origin.x + track.size.width - px(THUMB_GAP + THUMB_WIDTH), track.origin.y + px(travel)),
        size(px(THUMB_WIDTH), px(thumb_h)),
    )
}

/// Paint the thumb and take the pointer on the track. Call it while painting.
pub(crate) fn paint(window: &mut Window, bar: Scrollbar) {
    let Scrollbar { track, start, end, total, state, scroll_to, probes } = bar;
    let _ = probes;
    let thumb = thumb_bounds(track, start, end, total);
    #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
    {
        crate::scenario::button_probe::record_bounds(probes.0, track);
        crate::scenario::button_probe::record_bounds(probes.1, thumb);
    }
    let hot = state.dragging.get() || track.contains(&window.mouse_position());
    state.hover.set(hot);
    let shade = if hot { 0.45 } else { 0.28 };
    window.paint_quad(gpui::fill(thumb, crate::palette::shade(0.0, shade)).corner_radii(px(THUMB_WIDTH / 2.0)));

    // The middle of the thumb follows the pointer.
    let at = {
        let total = total.max(1);
        let shown = ((end - start) as f32 / total as f32).clamp(0.05, 1.0);
        move |y: Pixels, window: &mut Window, cx: &mut App| {
            let height = f32::from(track.size.height).max(1.0);
            let fraction = (f32::from(y - track.origin.y) / height).clamp(0.0, 1.0);
            let first = ((fraction - shown / 2.0) * total as f32).round().max(0.0) as usize;
            scroll_to(first, window, cx);
        }
    };
    // Capture phase: this owns only the track; a menu-wide stop in the bubble phase would run before the rows'
    // listeners and stop their clicks.
    let (down_state, down_at) = (state.clone(), at.clone());
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
        if phase == DispatchPhase::Capture && event.button == MouseButton::Left && track.contains(&event.position) {
            down_state.dragging.set(true);
            down_at(event.position.y, window, cx);
            cx.stop_propagation();
        }
    });
    let move_state = state.clone();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
        if phase != DispatchPhase::Capture {
            return;
        }
        if event.dragging() && move_state.dragging.get() {
            at(event.position.y, window, cx);
            cx.stop_propagation();
        } else if track.contains(&event.position) != move_state.hover.get() {
            // The thumb's colour follows the pointer on and off the track.
            window.refresh();
        }
    });
    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, _| {
        if phase == DispatchPhase::Capture && event.button == MouseButton::Left && state.dragging.replace(false) {
            window.refresh();
        }
    });
}
