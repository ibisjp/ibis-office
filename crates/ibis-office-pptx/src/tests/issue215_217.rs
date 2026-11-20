// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! issues #215 (the wheel of 開始・終了 leaves stripes and steps), #216 (zoom, grow and turn lay the text out again while they play),
//! #217 (turn: the shape vanishes where it is seen from behind, the text is squeezed both ways). Frames are read as pixels.
use crate::shell::scenario::Scenario;
use crate::slideshow::SlideshowExt;
use ibis_doc_model::{AnimationClass, Color, Command, Fill, Rgb};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue215_wheel_covers_the_shape_without_stripes_and_spreads_by_the_frames_own_units", wheel_frames),
        ("pptx::issue216_zoom_keeps_the_text_in_its_lines_while_it_grows", zoom_keeps_lines),
        ("pptx::issue217_turn_shows_the_shape_from_behind_and_keeps_its_height", turn_flips),
    ]
}

/// The first slide's title as a red box with black text on a white slide, given the effect `id` (option `sub`) of `class`, played; the
/// picture at `ms` of it, and the box's rectangle on it.
fn frame(class: AnimationClass, id: u32, sub: Option<u32>, ms: f32) -> (image::RgbaImage, [f32; 4]) {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(960., 540.);
    let (slide, shape) = s.with(|e, _| (e.presentation().slides[0].id, e.presentation().slides[0].shapes[0].id));
    s.update(|e, _, cx| {
        e.run(
            vec![
                Command::SetBackground { slide, fill: Fill::Solid(Color::opaque(Rgb::from_rgb(255, 255, 255))) },
                Command::SetFill { slide, shape, fill: Fill::Solid(Color::opaque(Rgb::from_rgb(255, 0, 0))) },
            ],
            cx,
        )
    });
    s.select_shape(0);
    let (code, menu) = match class {
        AnimationClass::Exit => ("exit", "pptx.animation.cat.exit"),
        _ => ("entr", "pptx.animation.cat.entr"),
    };
    let _ = code;
    s.toolbar_menu("motion", &format!("{menu}.{id}"));
    let class = if class == AnimationClass::Exit { AnimationClass::Exit } else { AnimationClass::Entrance };
    if ibis_formats::animation_catalog::effect(class, id).is_some() && sub.is_some() {
        s.toolbar_menu("anim-options", &format!("{menu}.{id}.{}", sub.unwrap_or(0)));
    }
    let rect = s.with(|e, _| {
        let f = e.presentation().slides[0].shapes[0].frame;
        let size = e.presentation().size_of(e.slide());
        [f.x as f32 / size.width as f32, f.y as f32 / size.height as f32, (f.x + f.width) as f32 / size.width as f32, (f.y + f.height) as f32 / size.height as f32]
    });
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    s.settle();
    s.key("space");
    s.update(|e, _, cx| {
        if let Some(show) = e.slideshow_mut() {
            show.anim_at = Some(ms);
        }
        cx.notify();
    });
    s.settle();
    let path = s.screenshot(&format!("{id}-{ms:.0}"));
    (image::open(path).unwrap().to_rgba8(), rect)
}

fn is_red(p: &image::Rgba<u8>) -> bool {
    p[0] > 200 && p[0] as i32 - p[1] as i32 > 25 && (p[1] as i32 - p[2] as i32).abs() < 12
}

/// #215: half way through the wheel the red is one piece: no row of the red part is cut by a white line (a pixel row in the middle of
/// red rows that has nothing red where the rows next to it have), and the sector spreads in the frame's own units (half of its
/// width and half of its height are both 1): on a frame wider than tall, the wheel of 2 spokes at a quarter of its time has covered
/// the part between straight up and the line to its upper right corner, which is further from the middle across than up.
fn wheel_frames() {
    let (img, r) = frame(AnimationClass::Entrance, 21, Some(2), 500.0);
    let (w, h) = (img.width() as f32, img.height() as f32);
    let (x0, x1, y0, y1) = ((r[0] * w) as u32, (r[2] * w) as u32, (r[1] * h) as u32, (r[3] * h) as u32);
    let mut cut = 0;
    let mut shown = 0;
    for y in y0 + 1..y1 - 1 {
        for x in x0..x1 {
            let white = |p: &image::Rgba<u8>| p[0] > 245 && p[1] > 245 && p[2] > 245;
            if is_red(img.get_pixel(x, y)) {
                shown += 1;
            }
            if white(img.get_pixel(x, y)) && is_red(img.get_pixel(x, y - 1)) && is_red(img.get_pixel(x, y + 1)) {
                cut += 1;
            }
        }
    }
    assert!(cut < 20, "{cut} white pixels lie between red ones above and below (a stripe)");
    assert!(shown > 1000, "nothing of the box is shown");
    // Half way (the sector's front at 45 degrees of its 90, the corner's direction in the frame's units): a point at 0.5 of the half
    // width across and 0.9 of the half height up is in the sector (29 degrees; in the picture's pixels it would be 65 and outside), a
    // point at 0.9 across and 0.3 up is out of it (71 degrees).
    let (cx, cy) = ((x0 + x1) / 2, (y0 + y1) / 2);
    let at = |fx: f32, fy: f32| is_red(img.get_pixel((cx as f32 + fx * (x1 - x0) as f32 / 2.0) as u32, (cy as f32 - fy * (y1 - y0) as f32 / 2.0) as u32));
    assert!(at(0.5, 0.9), "the wide frame's sector does not spread to its upper right");
    assert!(!at(0.9, 0.3), "the sector is wider than the frame's own angle says");
    // The front is soft (measured on PowerPoint: a half cosine band 0.197 of the sector wide, its centre at -0.07 + 1.14 q of it; q is
    // 0.25 here, the sector 180 degrees: the centre at 38.7 degrees, the band from 21 to 56 degrees): the pixel at the centre is half
    // way between red and white, one at 10 degrees is red, one at 70 degrees is white.
    let green = |deg: f32| {
        let (u, v) = (deg.to_radians().sin() * 0.6, deg.to_radians().cos() * 0.6);
        img.get_pixel((cx as f32 + u * (x1 - x0) as f32 / 2.0) as u32, (cy as f32 - v * (y1 - y0) as f32 / 2.0) as u32)[1]
    };
    let (inside, centre, outside) = (green(10.0), green(38.7), green(70.0));
    assert!(inside < 40, "behind the band the box is red: green {inside}");
    assert!((100..160).contains(&centre), "the middle of the band is half way: green {centre}");
    assert!(outside > 220, "ahead of the band nothing is shown: green {outside}");
}

/// #216: the box zooming in at a third of its time is one picture, scaled: the lines of its text are the same as at the end (nothing is
/// laid out again in the smaller frame), so the black of the text stays inside the red.
fn zoom_keeps_lines() {
    let (img, _) = frame(AnimationClass::Entrance, 53, None, 250.0);
    let (mut left, mut right, mut top, mut bottom) = (i32::MAX, 0, i32::MAX, 0);
    for y in 0..img.height() as i32 {
        for x in 0..img.width() as i32 {
            if is_red(img.get_pixel(x as u32, y as u32)) {
                (left, right, top, bottom) = (left.min(x), right.max(x), top.min(y), bottom.max(y));
            }
        }
    }
    assert!(right > left && bottom > top, "no box");
    // What is dark (the text) from the box's top to a little below it: all of it inside the box.
    let (mut dl, mut dr, mut dt, mut db) = (i32::MAX, 0, i32::MAX, 0);
    for y in top..(bottom + 60).min(img.height() as i32) {
        for x in 0..img.width() as i32 {
            let p = img.get_pixel(x as u32, y as u32);
            if p[0] < 80 && p[1] < 80 && p[2] < 80 {
                (dl, dr, dt, db) = (dl.min(x), dr.max(x), dt.min(y), db.max(y));
            }
        }
    }
    assert!(dr == 0 || (dl >= left - 2 && dr <= right + 2 && dt >= top - 2 && db <= bottom + 2), "text ({dl},{dt})-({dr},{db}) is outside the box ({left},{top})-({right},{bottom})");
}

/// #217: turn (the width follows `sin(2.5 pi t)`): where the width has gone negative the box is seen from behind, still there; its height
/// stays the same through the whole effect.
fn turn_flips() {
    // sin(2.5 pi t) < 0 for t in (0.4, 0.8): t = 0.6 of 2 seconds.
    let (back, r) = frame(AnimationClass::Entrance, 45, None, 1200.0);
    let (front, _) = frame(AnimationClass::Entrance, 45, None, 400.0);
    let height = |img: &image::RgbaImage| {
        let ys: Vec<u32> = (0..img.height()).filter(|&y| (0..img.width()).any(|x| is_red(img.get_pixel(x, y)))).collect();
        (ys.first().copied(), ys.last().copied())
    };
    let (b, f) = (height(&back), height(&front));
    assert!(b.0.is_some(), "the box vanishes where it is seen from behind");
    assert!(f.0.is_some());
    let (bh, fh) = (b.1.unwrap() as i64 - b.0.unwrap() as i64, f.1.unwrap() as i64 - f.0.unwrap() as i64);
    assert!((bh - fh).abs() <= 3, "the height changes: {bh} behind, {fh} before ({r:?})");
}
