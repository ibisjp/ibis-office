// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The look of the insert mode, which the 文書画面, 表計算画面 and プレゼンテーション画面 share (K0035). Insert > a shape or a text box
//! arms the mode: the pointer is a cross, the drag from the mouse-down point to the mouse-up point draws a translucent box with an
//! outline (this file), Esc drops the drag, and a plain click puts the shape at its usual size. The three screens keep their own
//! pointer state (the page's points, the slide's, the sheet's) and draw the box here, so the fill, the outline and the cursor
//! cannot drift apart.

use gpui_kit::*;

use crate::render::shape_view::polygon;

/// The fill of the box, as the opacity of the theme's primary colour.
pub const BAND_FILL_ALPHA: f32 = 0.08;
/// The outline of the box, in pixels.
pub const BAND_OUTLINE_WIDTH: f32 = 1.0;

/// The pointer while the insert mode is on.
pub fn insert_cursor() -> CursorStyle {
    CursorStyle::Crosshair
}

/// Paint a box of a drag: the translucent fill (a closed shape with more than two corners) and the outline.
pub fn paint_band(window: &mut Window, corners: &[Point<Pixels>], color: Hsla) {
    let closed = corners.len() > 2;
    if closed {
        let mut fill = color;
        fill.a = BAND_FILL_ALPHA;
        if let Some(path) = polygon(corners, None, true) {
            window.paint_path(path, fill);
        }
    }
    if let Some(path) = polygon(corners, Some(px(BAND_OUTLINE_WIDTH)), closed) {
        window.paint_path(path, color);
    }
}

/// The box of an insert drag as an element, `rect` in the parent's pixels (the parent is a positioned box).
pub fn band_element(rect: Bounds<Pixels>, color: Hsla) -> AnyElement {
    div()
        .absolute()
        .left(rect.origin.x)
        .top(rect.origin.y)
        .w(rect.size.width)
        .h(rect.size.height)
        .child(canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let corners = [bounds.origin, point(bounds.right(), bounds.origin.y), point(bounds.right(), bounds.bottom()), point(bounds.origin.x, bounds.bottom())];
                paint_band(window, &corners, color);
            },
        ).size_full())
        .into_any_element()
}
