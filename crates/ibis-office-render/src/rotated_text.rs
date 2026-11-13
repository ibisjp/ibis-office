// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Rotated lines of text, such as vertical chart axis labels.
//!
//! Each line is shaped by GPUI as usual, then every glyph is painted with a
//! rotation about the line's centre (`Window::paint_glyph_transformed`,
//! from ibisOffice's GPUI fork).

use gpui_kit::*;

/// Which point of the rotated text's bounding box sits at `at`.
#[derive(Clone, Copy, Debug)]
pub enum Attach {
    /// The middle of the top edge (labels below a horizontal axis).
    TopCenter,
    /// The middle of the right edge (labels left of a vertical axis).
    RightMiddle,
    /// The end of the line, half a line below it (slanted labels below a
    /// horizontal axis, which end at their tick).
    TopEnd,
    /// Centered at `at` (such as vertical axis titles).
    Center,
}

#[derive(Clone, Debug)]
pub(crate) struct RotatedLine {
    pub text: String,
    pub font: Font,
    pub size: Pixels,
    pub color: Hsla,
    /// Relative to the element's top-left corner.
    pub at: (f32, f32),
    pub attach: Attach,
    /// Clockwise, in degrees.
    pub degrees: f32,
    /// The longest the line may be; a longer one is drawn smaller to fit.
    pub(crate) max_len: Option<f32>,
}

/// An element filling its parent that paints `lines`.
pub fn rotated_lines(lines: Vec<RotatedLine>) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            for line in &lines {
                paint_line(line, bounds.origin, window);
            }
        },
    )
    .absolute()
    .size_full()
}

fn paint_line(line: &RotatedLine, origin: Point<Pixels>, window: &mut Window) {
    let run = TextRun {
        len: line.text.len(),
        font: line.font.clone(),
        color: line.color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let mut size = line.size;
    let mut shaped = window.text_system().shape_line(line.text.clone().into(), size, std::slice::from_ref(&run), None);
    if let Some(max) = line.max_len
        && f32::from(shaped.width) > max
        && max > 0.0
    {
        size *= max / f32::from(shaped.width);
        shaped = window.text_system().shape_line(line.text.clone().into(), size, &[run], None);
    }
    let (w, h) = (f32::from(shaped.width), f32::from(shaped.ascent + shaped.descent));
    let angle = line.degrees.to_radians();
    let (sin, cos) = angle.sin_cos();
    let (bw, bh) = (w * cos.abs() + h * sin.abs(), w * sin.abs() + h * cos.abs());
    let (ax, ay) = line.at;
    let (cx, cy) = match line.attach {
        Attach::TopCenter => (ax, ay + bh / 2.0),
        Attach::RightMiddle => (ax - bw / 2.0, ay),
        Attach::TopEnd => {
            // Back from the end along the line, and half a line across it.
            let (ux, uy) = (cos, sin);
            (ax - ux * w / 2.0 - uy * h / 2.0, ay - uy * w / 2.0 + ux * h / 2.0)
        }
        Attach::Center => (ax, ay),
    };
    let center = point(origin.x + px(cx), origin.y + px(cy));

    // Each glyph is moved to where the rotation about the centre takes it
    // and turned about its own origin there, so its unturned box is where it
    // ends up (glyphs whose unturned place fell outside the frame were cut
    // off: the last label of an upright axis at the frame's edge).
    let scale = window.scale_factor();
    let baseline = point(center.x - px(w / 2.0), center.y - px(h / 2.0) + shaped.ascent);
    for run in &shaped.runs {
        for glyph in &run.glyphs {
            let at = point(baseline.x + glyph.position.x, baseline.y + glyph.position.y);
            let (dx, dy) = (f32::from(at.x - center.x), f32::from(at.y - center.y));
            let turned = point(center.x + px(dx * cos - dy * sin), center.y + px(dx * sin + dy * cos));
            let pivot = point(ScaledPixels(f32::from(turned.x) * scale), ScaledPixels(f32::from(turned.y) * scale));
            let transformation = TransformationMatrix::unit()
                .translate(pivot)
                .rotate(Radians(angle))
                .translate(point(ScaledPixels(-pivot.x.0), ScaledPixels(-pivot.y.0)));
            let _ = window.paint_glyph_transformed(turned, run.font_id, glyph.id, size, line.color, transformation);
        }
    }
}
