// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The layout of a narrow window (K0061, K0077): under [`NARROW_LAYOUT_WIDTH`] **on a touch device** the fingers' home is the bottom of the
//! screen. A narrow window with a mouse (a desktop window dragged narrow) keeps the wide order, the bars on top, folding into ︙ and ▾
//! (the pointer is at the top; only the fingers' reach moves the bars). [`bars_at_bottom`] decides; [`is_narrow`] is the width alone.
//!
//! * The order of the window's parts when [`bars_at_bottom`] is title bar, document, status bar, menu bar, toolbar (otherwise: title bar,
//!   menu bar, toolbar, document, status bar). The window is laid out again as it is resized: nothing is kept between frames.
//! * The toolbar's ^ (which hides the menu bar and the title row) is not shown: the bars are at the bottom and stay.
//! * On a touch device the menu bar and the toolbar are not folded into a ︙ or a ▾: they scroll sideways by a swipe (no scroll
//!   bar), and a black shadow at each end that has more to scroll to ([`edge_shadows`]) says so. The toolbar's pill (its rounded
//!   ends included) scrolls with its buttons. With a mouse the bars fold as they did (the ︙ and the ▾).
//!
//! The scroll positions are in the window's editor (`SlideEditor::menu_bar_scroll`, `toolbar_scroll`). That a device is a touch
//! device is `SlideEditor::is_touch_device`: always on iOS and Android, and in the other systems (the Web version in a phone's
//! browser, a touch screen of a PC) from the first touch of the window (`TouchSessionEvent`, `gesture.rs`).

use gpui_kit::*;

use crate::editor::SlideEditor;

/// The window is narrow under this width (points, the viewport's).
pub const NARROW_LAYOUT_WIDTH: f32 = 720.0;
/// How wide the shadow at an end of a scrolling bar is: the width of the sheet tabs' shadows (xlsx `sheet_ui.rs`, 18 px), so the
/// bars at the bottom and the tabs look alike (user, 2026-11-26: 30 px was too wide).
pub(crate) const EDGE_SHADOW_WIDTH: f32 = 18.0;
/// How dark the shadow is at the very end (black in the light and the dark theme alike).
pub(crate) const EDGE_SHADOW_ALPHA: f32 = 0.30;

/// Whether the window is narrow, by its width alone (the sideways scroll of the bars of a touch device also uses it).
pub fn is_narrow(window: &Window) -> bool {
    f32::from(window.viewport_size().width) < NARROW_LAYOUT_WIDTH
}

/// Whether the menu bar and the toolbar are at the bottom of the window: it is narrow and on a touch device (K0077).
pub fn bars_at_bottom(editor: &SlideEditor, window: &Window) -> bool {
    is_narrow(window) && editor.is_touch_device()
}

/// The slide show's button when it is round (its play icon alone, no ▾): its height is its width (`title_buttons.rs` of the presentation).
pub const SLIDESHOW_ROUND_WIDTH: f32 = 30.0;
/// The gap between the slide show's button and its neighbour in the title row.
pub const SLIDESHOW_GAP: f32 = 8.0;
/// What the pill of the slide show's button is wide before it has been drawn once (afterwards the width it was drawn with is used).
pub const SLIDESHOW_PILL_FIRST_WIDTH: f32 = 120.0;

/// Whether the slide show's button is the round one (K0082). `room` is what the title row has for the button, the file name and the toolbar (the
/// window less the row's other buttons), `pill` the width of the pill, `toolbar` the natural width of all the toolbar's buttons when the toolbar
/// is in this row (`None`: it is in its own row), `name_min` the room the file name keeps. The pill goes while the toolbar fits whole with it;
/// when the toolbar is in its own row the file name is what gives way first, down to `name_min`, and then the pill. The file name itself may
/// have no room at all in the end.
pub fn slideshow_is_round(room: f32, pill: f32, toolbar: Option<f32>, name_min: f32) -> bool {
    let pill = pill + SLIDESHOW_GAP;
    match toolbar {
        Some(toolbar) => room < pill + toolbar,
        None => room < pill + name_min,
    }
}

/// Whether the slide list (the thumbnail pane) of a presentation is shown (K0083): the content beside it, `body` (the width of the screen's body,
/// the window less the right pane) less the list `pane` and its `divider`, keeps at least twice the list's width. Decided from the width of every
/// frame, so the list comes back when the window is widened; it is not a choice of the user's (there is none for this list).
pub fn slide_list_fits(body: f32, pane: f32, divider: f32) -> bool {
    body - pane - divider >= 2.0 * pane
}

impl SlideEditor {
    /// Whether the window is on a touch device (see the module docs).
    pub fn is_touch_device(&self) -> bool {
        cfg!(any(target_os = "ios", target_os = "android")) || self.touch_seen
    }

    /// Whether the menu bar and the toolbar scroll sideways in place of folding: a narrow window of a touch device.
    pub(crate) fn bars_scroll(&self, window: &Window) -> bool {
        is_narrow(window) && self.is_touch_device()
    }
}

/// Which ends of a sideways scroll have more to show: `(left, right)`.
pub fn more_to_scroll(handle: &ScrollHandle) -> (bool, bool) {
    let offset = f32::from(handle.offset().x);
    let max = f32::from(handle.max_offset().x);
    (offset < -0.5, max > 0.5 && -offset < max - 0.5)
}

/// The shadows at the ends of a bar that scrolls sideways, drawn over its whole area (put it last in a `relative` parent of the
/// bar's size). It reads the scroll at paint time, so it follows the scroll without the bar being built again.
pub(crate) fn edge_shadows(handle: &ScrollHandle) -> impl IntoElement {
    let handle = handle.clone();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let (left, right) = more_to_scroll(&handle);
            let dark: Hsla = Hsla { h: 0., s: 0., l: 0., a: EDGE_SHADOW_ALPHA };
            let clear: Hsla = Hsla { h: 0., s: 0., l: 0., a: 0. };
            let width = px(EDGE_SHADOW_WIDTH).min(bounds.size.width / 2.);
            if left {
                let shadow = Bounds { origin: bounds.origin, size: size(width, bounds.size.height) };
                window.paint_quad(fill(shadow, linear_gradient(90., linear_color_stop(dark, 0.), linear_color_stop(clear, 1.))));
            }
            if right {
                let shadow = Bounds { origin: point(bounds.right() - width, bounds.origin.y), size: size(width, bounds.size.height) };
                window.paint_quad(fill(shadow, linear_gradient(90., linear_color_stop(clear, 0.), linear_color_stop(dark, 1.))));
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
