// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-11 (#153, #M88, #M92): the soft edge (`a:softEdge`) of a filled shape of any outline, on the screen. The opacity is read from the
//! screenshot against the same shape without the soft edge (white page, a red shape: alpha = (255 - G) / 255) and compared with the fade the
//! measurements give: half way 0.87 of the radius inside the outline (rad 10 pt: 2 % at 1.82 pt, 50 % at 8.7 pt, 98 % at 15.18 pt).
use crate::shell::scenario::Scenario;
use ibis_doc_model::{Color, EmuRect, Fill, Geometry, Rgb, Shape, ShapeKind, Stroke};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::softedge_20261111_rectangle_and_rounded_rectangle", rectangle),
        ("pptx::softedge_20261111_ellipse_and_rotated_triangle", curved_and_rotated),
        ("pptx::softedge_20261111_line_and_no_fill", line_and_no_fill),
        ("pptx::softedge_20261111_dashed_and_compound_lines", dashed_and_compound_lines),
        ("pptx::softedge_20261111_gradient_and_pattern_fills", gradient_and_pattern_fills),
        ("pptx::softedge_20261111_round_dots_keep_their_inner_half", round_dots),
        ("pptx::softedge_20261111_pattern_is_laid_from_the_bounding_rectangle", pattern_origin),
        ("pptx::softedge_20261111_picture_fill_stretched_with_a_line", picture_stretched),
        ("pptx::softedge_20261111_picture_fill_turned_and_flipped", picture_turned),
        ("pptx::softedge_20261111_picture_fill_tiled_and_cropped", picture_tiled_and_cropped),
        ("pptx::softedge_20261111_the_fade_starts_at_the_fill_rectangle", fill_rect_fade),
        ("pptx::softedge_20261111_tiled_and_cropped_pictures_turn_and_flip_as_with_the_fade", turned_and_flipped_tiles),
        ("pptx::softedge_20261111_an_upright_picture_fill_is_mirrored_with_its_shape", picture_mirrored),
        ("pptx::softedge_20261111_a_picture_shape_fades_as_a_filled_shape_does", picture_shape),
        ("pptx::softedge_20261111_the_shadow_is_made_from_the_faded_shape", shadow_of_the_faded_shape),
        ("pptx::softedge_20261111_the_glow_is_made_from_the_shape_before_the_fade", glow_of_the_whole_shape),
        ("pptx::softedge_20261111_a_group_is_faded_as_one_picture", group_faded_as_one),
    ]
}

const RED: Color = Color { rgb: Rgb(0xFF0000), alpha: 255 };
const RAD: i64 = 127_000; // 10 pt

/// The screenshot of a white slide with the shape on it. (0 rotation is upright.)
fn shot(geometry: Geometry, rotation: i32, fill: Fill, stroke: Option<Stroke>, soft: Option<i64>, label: &str) -> image::RgbaImage {
    shot_at(geometry, rotation, fill, stroke, soft, 1_500_000, label)
}

/// `shot`, the shape's left edge `left` EMU from the slide's.
fn shot_at(geometry: Geometry, rotation: i32, fill: Fill, stroke: Option<Stroke>, soft: Option<i64>, left: i64, label: &str) -> image::RgbaImage {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| {
        let size = e.presentation().size_of(e.slide());
        let back = e.document.ids().new_shape_id();
        let white = ibis_doc_model::AutoShape::new(Geometry::Rect, Fill::Solid(Color { rgb: Rgb(0xFFFFFF), alpha: 255 }), None);
        e.insert_shape(Shape::new(back, EmuRect::new(0, 0, size.width, size.height), ShapeKind::Auto(white)), cx);
        let id = e.document.ids().new_shape_id();
        let mut auto = ibis_doc_model::AutoShape::new(geometry, fill, stroke);
        auto.soft_edge = soft;
        let mut shape = Shape::new(id, EmuRect::new(left, 1_200_000, 4_000_000, 3_000_000), ShapeKind::Auto(auto));
        shape.rotation = rotation;
        e.insert_shape(shape, cx);
    });
    s.key("escape");
    let path = s.screenshot(label);
    image::open(path).expect("the screenshot").to_rgba8()
}

/// The opacity (0 to 1) of the shape in `soft` at each pixel of `hard`'s: (255 - G) / 255 of the red shape on white.
fn alpha(image: &image::RgbaImage, x: u32, y: u32) -> f32 {
    (255.0 - f32::from(image.get_pixel(x, y)[1])) / 255.0
}

/// The columns where the shape starts and ends on the row `y` of its upright picture, and the same on the column `x`: (left, right, top, bottom).
fn extent(hard: &image::RgbaImage, x: u32, y: u32) -> (u32, u32, u32, u32) {
    let on = |x: u32, y: u32| alpha(hard, x, y) > 0.5;
    let (mut l, mut r, mut t, mut b) = (x, x, y, y);
    while l > 0 && on(l - 1, y) { l -= 1; }
    while r + 1 < hard.width() && on(r + 1, y) { r += 1; }
    while t > 0 && on(x, t - 1) { t -= 1; }
    while b + 1 < hard.height() && on(x, b + 1) { b += 1; }
    (l, r, t, b)
}

fn centre_of_shape(hard: &image::RgbaImage) -> (u32, u32) {
    let (mut sx, mut sy, mut n) = (0u64, 0u64, 0u64);
    for (x, y, p) in hard.enumerate_pixels() {
        // (Only the slide's area: the red of the thumbnail at the left and of the title row's icons is not the shape, and moves the centre with them.)
        if x > 600 && y > 150 && p[1] < 128 {
            sx += u64::from(x);
            sy += u64::from(y);
            n += 1;
        }
    }
    assert!(n > 100, "the shape is on the screen");
    ((sx / n) as u32, (sy / n) as u32)
}

fn rectangle() {
    for (geometry, name) in [(Geometry::Rect, "rect"), (Geometry::RoundRect { radius: 16_667 }, "round")] {
        let hard = shot(geometry.clone(), 0, Fill::Solid(RED), None, None, &format!("{name}-hard"));
        let soft = shot(geometry, 0, Fill::Solid(RED), None, Some(RAD), &format!("{name}-soft"));
        let (cx, cy) = centre_of_shape(&hard);
        let (l, r, _, _) = extent(&hard, cx, cy);
        // Pixels a point: the shape is 4 000 000 EMU = 314.96 pt wide.
        let px_pt = (r - l + 1) as f32 / (4_000_000.0 / 12_700.0);
        let at = |t_pt: f32| alpha(&soft, l + (t_pt * px_pt).round() as u32, cy);
        for (t, want, tolerance) in [(1.82, 0.02, 0.05), (8.7, 0.5, 0.08), (15.18, 0.98, 0.05)] {
            assert!((at(t) - want).abs() < tolerance, "{name}: alpha at {t} pt is {} (want {want})", at(t));
        }
        // The outline itself is gone, the middle is whole.
        assert!(alpha(&soft, l, cy) < 0.01 && alpha(&soft, cx, cy) > 0.99, "{name}");
    }
}

fn curved_and_rotated() {
    // An ellipse: through its horizontal and its vertical diameter the fade is that of a straight edge (#M88: 0.90 to 0.94 of rad for PowerPoint
    // against 0.87 here, which the measurement leaves as a rest).
    let hard = shot(Geometry::Ellipse, 0, Fill::Solid(RED), None, None, "ellipse-hard");
    let soft = shot(Geometry::Ellipse, 0, Fill::Solid(RED), None, Some(RAD), "ellipse-soft");
    let (cx, cy) = centre_of_shape(&hard);
    let (l, r, t, b) = extent(&hard, cx, cy);
    let px_pt = (r - l + 1) as f32 / (4_000_000.0 / 12_700.0);
    let half_left = alpha(&soft, l + (8.7 * px_pt).round() as u32, cy);
    let half_top = alpha(&soft, cx, t + (8.7 * (b - t + 1) as f32 / (3_000_000.0 / 12_700.0)).round() as u32);
    assert!((half_left - 0.5).abs() < 0.12 && (half_top - 0.5).abs() < 0.12, "ellipse: {half_left} {half_top}");
    assert!(alpha(&soft, l, cy) < 0.01 && alpha(&soft, cx, cy) > 0.99);
    // A triangle turned by 30 degrees: it fades along its turned outline, so its corner (the vertex) is fainter than the middle of an edge.
    let hard = shot(ibis_pptx::preset_geometry("triangle").expect("the triangle preset"), 1_800_000, Fill::Solid(RED), None, None, "triangle-hard");
    let soft = shot(ibis_pptx::preset_geometry("triangle").expect("the triangle preset"), 1_800_000, Fill::Solid(RED), None, Some(RAD), "triangle-soft");
    let (cx, cy) = centre_of_shape(&hard);
    assert!(alpha(&soft, cx, cy) > 0.9, "the middle of the triangle stays");
    let mut faded = 0;
    let mut whole = 0;
    for (x, y, p) in hard.enumerate_pixels() {
        if p[1] < 128 {
            if alpha(&soft, x, y) < 0.9 { faded += 1; } else { whole += 1; }
        }
    }
    assert!(faded > 0 && whole > 0, "part of the triangle fades ({faded}) and part stays ({whole})");
    // Nothing is drawn outside the outline.
    for (x, y, p) in hard.enumerate_pixels() {
        if p[1] > 250 {
            assert!(alpha(&soft, x, y) < 0.02, "outside the triangle at {x},{y}");
        }
    }
}

fn line_and_no_fill() {
    // A line round the shape fades with it: at its outer edge the line is nearly gone, deep inside the shape it is whole.
    let line = Some(Stroke::solid(Color { rgb: Rgb(0x000096), alpha: 255 }, 76_200));
    let soft = shot(Geometry::Rect, 0, Fill::Solid(RED), line.clone(), Some(RAD), "line-soft");
    let (cx, cy) = centre_of_shape(&soft);
    // The outer edge of the line (a 6 pt line) is where the picture starts: nothing of it there.
    let mut left = cx;
    while left > 0 && soft.get_pixel(left - 1, cy)[1] < 250 { left -= 1; }
    assert!(alpha(&soft, left, cy) < 0.2, "the line is faded at its outer edge");
    assert!(alpha(&soft, cx, cy) > 0.99);
    // No fill: nothing at all, the line neither.
    let none = shot(Geometry::Rect, 0, Fill::None, line, Some(RAD), "none-soft");
    let blank = shot(Geometry::Rect, 0, Fill::None, None, None, "none-blank");
    assert!(none.pixels().zip(blank.pixels()).all(|(a, b)| a == b), "a shape with no fill and a soft edge draws nothing (not even its line)");
}

/// How much of a shape's colour is left at `(x, y)` of `soft` against the same shape without the soft edge: the distance from white of `soft`
/// over that of `hard` (all channels summed, 1 = as whole as without the fade, 0 = white).
fn kept(hard: &image::RgbaImage, soft: &image::RgbaImage, x: u32, y: u32) -> f32 {
    let away = |p: &image::Rgba<u8>| 3.0 * 255.0 - f32::from(p[0]) - f32::from(p[1]) - f32::from(p[2]);
    let h = away(hard.get_pixel(x, y));
    if h < 1.0 { 1.0 } else { away(soft.get_pixel(x, y)) / h }
}

/// A shape drawn with a soft edge is the same picture as without it, faded: whole deep inside, gone at the outline, and nothing outside it.
fn assert_faded(hard: &image::RgbaImage, soft: &image::RgbaImage, what: &str) {
    let edge = 0.2;
    let (cx, cy) = centre_of_shape(hard);
    let (l, _, t, _) = extent(hard, cx, cy);
    assert!(kept(hard, soft, l, cy) < edge && kept(hard, soft, cx, t) < edge, "{what}: gone at the outline ({} {})", kept(hard, soft, l, cy), kept(hard, soft, cx, t));
    // (The middle: the average of a block, as a pattern's cells and a gradient's colours differ from pixel to pixel.)
    let (mut h, mut s) = (0.0, 0.0);
    for dy in 0..8 {
        for dx in 0..8 {
            h += 3.0 * 255.0 - hard.get_pixel(cx + dx, cy + dy).0[..3].iter().map(|v| f32::from(*v)).sum::<f32>();
            s += 3.0 * 255.0 - soft.get_pixel(cx + dx, cy + dy).0[..3].iter().map(|v| f32::from(*v)).sum::<f32>();
        }
    }
    assert!((s / h - 1.0).abs() < 0.03, "{what}: whole in the middle ({})", s / h);
    for (x, y, p) in hard.enumerate_pixels() {
        if p[0] > 250 && p[1] > 250 && p[2] > 250 {
            // (Nothing is added there: the soft picture is white, or at least not darker than the hard one's own pixel, which may be a shade off white.)
            let s = soft.get_pixel(x, y);
            assert!((0..3).all(|c| s[c] > 252 || s[c] >= p[c]), "{what}: nothing outside the outline at {x},{y}");
        }
    }
}

/// A dashed line and a compound one go through the same fade as a plain one (they were drawn with no fade at all): the shape is the hard
/// picture faded, its line included.
fn dashed_and_compound_lines() {
    for (name, dashed, compound) in [("dashed", true, ibis_doc_model::LineCompound::Single), ("compound", false, ibis_doc_model::LineCompound::Double)] {
        let mut line = Stroke::solid(Color { rgb: Rgb(0x000096), alpha: 255 }, 76_200);
        line.dashed = dashed;
        if dashed {
            line.dash = ibis_doc_model::LineDash::Dash;
        }
        line.compound = compound;
        let hard = shot(Geometry::Rect, 0, Fill::Solid(RED), Some(line.clone()), None, &format!("{name}-hard"));
        let soft = shot(Geometry::Rect, 0, Fill::Solid(RED), Some(line), Some(RAD), &format!("{name}-soft"));
        assert_faded(&hard, &soft, name);
        // And it is faded: the soft picture is not the hard one.
        assert!(hard.pixels().zip(soft.pixels()).filter(|(a, b)| a != b).count() > 1000, "{name}: the fade is applied");
    }
}

/// Gradient and pattern fills fade as a plain colour does (they were drawn with no fade at all).
fn gradient_and_pattern_fills() {
    let stop = |position, rgb| ibis_doc_model::GradientStop { position, color: Color { rgb: Rgb(rgb), alpha: 255 } };
    let linear = Fill::Linear { angle: 0, scaled: false, stops: vec![stop(0, 0xFF0000), stop(100_000, 0xFF8000)] };
    let path = Fill::Path { path: ibis_doc_model::GradientPath::Circle, focus: [50_000; 4], stops: vec![stop(0, 0xFF0000), stop(100_000, 0xC00000)] };
    let pattern = Fill::Pattern { preset: "pct50".into(), foreground: RED, background: Color { rgb: Rgb(0xFF4040), alpha: 255 } };
    for (name, fill) in [("linear", linear), ("path", path), ("pattern", pattern)] {
        let hard = shot(Geometry::Rect, 0, fill.clone(), None, None, &format!("{name}-hard"));
        let soft = shot(Geometry::Rect, 0, fill, None, Some(RAD), &format!("{name}-soft"));
        assert_faded(&hard, &soft, name);
    }
}

/// A picture of 64 x 64 pixels: red on the left half, blue on the right (so a picture put on the shape the wrong way round is seen).
/// Red on the left and blue on the right, greener towards the bottom (so it is not symmetric either way).
fn two_colour_png() -> Vec<u8> {
    let picture = image::RgbaImage::from_fn(64, 64, |x, y| if x < 32 { image::Rgba([255, y as u8 * 3, 0, 255]) } else { image::Rgba([0, y as u8 * 3, 255, 255]) });
    let mut data = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut data, image::ImageFormat::Png).expect("a PNG");
    data.into_inner()
}

/// The screenshot of a white slide with a shape on it filled with the picture as `fill_of(media)` says.
fn picture_shot(fill_of: &dyn Fn(ibis_doc_model::MediaId) -> Fill, rotation: i32, flip_h: bool, stroke: Option<Stroke>, soft: Option<i64>, label: &str) -> image::RgbaImage {
    picture_shot_flips(fill_of, rotation, flip_h, false, stroke, soft, label)
}

/// An upright picture-filled rectangle, flipped as asked.
fn picture_shot_flipped(fill_of: &dyn Fn(ibis_doc_model::MediaId) -> Fill, flip_h: bool, flip_v: bool, soft: Option<i64>, label: &str) -> image::RgbaImage {
    picture_shot_flips(fill_of, 0, flip_h, flip_v, None, soft, label)
}

fn picture_shot_flips(fill_of: &dyn Fn(ibis_doc_model::MediaId) -> Fill, rotation: i32, flip_h: bool, flip_v: bool, stroke: Option<Stroke>, soft: Option<i64>, label: &str) -> image::RgbaImage {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| {
        let size = e.presentation().size_of(e.slide());
        let back = e.document.ids().new_shape_id();
        let white = ibis_doc_model::AutoShape::new(Geometry::Rect, Fill::Solid(Color { rgb: Rgb(0xFFFFFF), alpha: 255 }), None);
        e.insert_shape(Shape::new(back, EmuRect::new(0, 0, size.width, size.height), ShapeKind::Auto(white)), cx);
        let data = two_colour_png();
        let media = e.document.ids().add_media(ibis_doc_model::MediaFormat::Png, data.clone());
        e.resources.add_picture(media, ibis_doc_model::MediaFormat::Png, &data);
        let id = e.document.ids().new_shape_id();
        let mut auto = ibis_doc_model::AutoShape::new(Geometry::Rect, fill_of(media), stroke);
        auto.soft_edge = soft;
        let mut shape = Shape::new(id, EmuRect::new(1_500_000, 1_200_000, 4_000_000, 3_000_000), ShapeKind::Auto(auto));
        shape.rotation = rotation;
        shape.flip_h = flip_h;
        shape.flip_v = flip_v;
        e.insert_shape(shape, cx);
    });
    s.key("escape");
    s.settle();
    let path = s.screenshot(label);
    image::open(path).expect("the screenshot").to_rgba8()
}

/// Where the faded picture is whole (as dark as the same picture without the fade), it is the same colour: the picture is put on the shape the
/// way it is without the fade (same side up, same turn and flips). Returns how many pixels were compared.
fn same_colours_where_whole(hard: &image::RgbaImage, soft: &image::RgbaImage, what: &str) -> usize {
    let away = |p: &image::Rgba<u8>| 3.0 * 255.0 - f32::from(p[0]) - f32::from(p[1]) - f32::from(p[2]);
    let mut compared = 0;
    // (Not within 3 pixels of a change of colour in the picture: the seam between the halves is a pixel or so off where the picture is drawn on
    // whole pixels, and its pixels are mixed there.)
    let flat = |x: u32, y: u32| -> bool {
        let h = hard.get_pixel(x, y);
        (-3i32..=3).all(|dy| (-3i32..=3).all(|dx| {
            let (nx, ny) = ((x as i32 + dx).clamp(0, hard.width() as i32 - 1) as u32, (y as i32 + dy).clamp(0, hard.height() as i32 - 1) as u32);
            (0..3).all(|k| (i32::from(hard.get_pixel(nx, ny)[k]) - i32::from(h[k])).abs() <= 24)
        }))
    };
    for (x, y, h) in hard.enumerate_pixels() {
        let s = soft.get_pixel(x, y);
        // (Not the thumbnail column at the left: the small picture there is its own scale's.)
        if x > 600 && away(h) > 400.0 && away(s) > 0.99 * away(h) && flat(x, y) {
            compared += 1;
            for k in 0..3 {
                assert!((i32::from(h[k]) - i32::from(s[k])).abs() <= 24, "{what}: at {x},{y} the picture is {:?} where it was {:?}", s.0, h.0);
            }
        }
    }
    compared
}

/// A picture fill stretched over a shape with a line round it fades like a colour: the line with it, the picture whole inside, the same
/// colours on the same sides.
fn picture_stretched() {
    // (A line whose green is under half, as the solid-colour test's: the shape's edge is found from the green channel, and a lighter line is not seen as
    // the shape, so its edge would be found a line's width further in, 6 pt, where the fade is already Phi((6 - 8.7) / 3.3) = 0.21 of the way up.)
    let line = Some(Stroke::solid(Color { rgb: Rgb(0x000096), alpha: 255 }, 76_200));
    let fill = |media| Fill::Image(media);
    let hard = picture_shot(&fill, 0, false, line.clone(), None, "picture-hard");
    let soft = picture_shot(&fill, 0, false, line, Some(RAD), "picture-soft");
    assert_faded(&hard, &soft, "picture");
    let (cx, cy) = centre_of_shape(&hard);
    let (l, r, _, _) = extent(&hard, cx, cy);
    // Red on the left half, blue on the right, as without the fade.
    let (left, right) = (soft.get_pixel(l + (r - l) / 4, cy), soft.get_pixel(r - (r - l) / 4, cy));
    assert!(left[0] > 200 && left[2] < 60 && right[2] > 200 && right[0] < 60, "{left:?} {right:?}");
    assert!(same_colours_where_whole(&hard, &soft, "picture") > 10_000);
}

/// Turned by 30 degrees, and flipped across: the picture goes with the shape, as it does without the fade.
fn picture_turned() {
    let fill = |media| Fill::Image(media);
    for (rotation, flip, name) in [(1_800_000, false, "turned"), (0, true, "flipped"), (1_800_000, true, "turned-flipped")] {
        let hard = picture_shot(&fill, rotation, flip, None, None, &format!("{name}-hard"));
        let soft = picture_shot(&fill, rotation, flip, None, Some(RAD), &format!("{name}-soft"));
        assert!(same_colours_where_whole(&hard, &soft, name) > 5_000, "{name}");
        assert!(hard.pixels().zip(soft.pixels()).filter(|(a, b)| a != b).count() > 1_000, "{name}: the fade is applied");
    }
}

/// A tiled picture (tiles of the picture's own size, alternate ones mirrored) and a cropped one with a fill rectangle.
fn picture_tiled_and_cropped() {
    let tiled = |media| Fill::TileImage { media, tx: 0, ty: 0, sx: 100_000, sy: 100_000, flip: ibis_doc_model::TileFlip::X, align: ibis_doc_model::TileAlign::TopLeft };
    let hard = picture_shot(&tiled, 0, false, None, None, "tile-hard");
    let soft = picture_shot(&tiled, 0, false, None, Some(RAD), "tile-soft");
    assert!(same_colours_where_whole(&hard, &soft, "tile") > 5_000);
    assert!(hard.pixels().zip(soft.pixels()).filter(|(a, b)| a != b).count() > 1_000, "tile: the fade is applied");
    let cropped = |media| {
        let mut blip = ibis_doc_model::BlipFill::new(media);
        blip.crop = ibis_doc_model::Crop { left: 25_000, top: 0, right: 0, bottom: 0 };
        Fill::Blip(Box::new(blip))
    };
    let hard = picture_shot(&cropped, 0, false, None, None, "crop-hard");
    let soft = picture_shot(&cropped, 0, false, None, Some(RAD), "crop-soft");
    assert!(same_colours_where_whole(&hard, &soft, "crop") > 5_000);
}

/// A dotted line with round caps (12 pt, soft edge 5 pt): each dot is a disc, and the half of it inside the shape is left faintly (it was drawn
/// flat, a dot of no length, and faded away altogether). The line's ink is the blue that red and white do not have.
fn round_dots() {
    let mut line = Stroke::solid(Color { rgb: Rgb(0x000096), alpha: 255 }, 152_400);
    line.dashed = true;
    line.dash = ibis_doc_model::LineDash::Dot;
    line.cap = ibis_doc_model::LineCap::Round;
    let soft = shot(Geometry::Rect, 0, Fill::Solid(RED), Some(line), Some(63_500), "dots-soft");
    let ink = soft.pixels().filter(|p| i32::from(p[2]) - i32::from(p[0]) > 12).count();
    assert!(ink > 200, "the dots leave their ink: {ink} pixels");
}

/// With a soft edge the pattern's tile is laid from the top-left of the shape's bounding rectangle (PowerPoint, measured on Windows), so the same
/// shape put half a cell further right shows the same pattern relative to itself.
fn pattern_origin() {
    let pattern = Fill::Pattern { preset: "pct50".into(), foreground: RED, background: Color { rgb: Rgb(0xFF4040), alpha: 255 } };
    let one = shot_at(Geometry::Rect, 0, pattern.clone(), None, Some(RAD), 1_500_000, "pattern-a");
    let other = shot_at(Geometry::Rect, 0, pattern, None, Some(RAD), 1_500_000 + 4_762, "pattern-b");
    // The shape's own corner is read from the same shape with no soft edge (its edge is sharp; the faded one's is a slope in which the checker's
    // dark and light cells move the place where the opacity crosses a half), the pattern relative to that corner.
    let place = |at: i64, name: &str| {
        let hard = shot_at(Geometry::Rect, 0, Fill::Solid(RED), None, None, at, name);
        let (cx, cy) = centre_of_shape(&hard);
        let (l, _, t, _) = extent(&hard, cx, cy);
        (l, t)
    };
    let ((l1, t1), (l2, t2)) = (place(1_500_000, "pattern-a-corner"), place(1_500_000 + 4_762, "pattern-b-corner"));
    let mut differing = 0;
    for dy in 40..120 {
        for dx in 40..120 {
            let (a, b) = (one.get_pixel(l1 + dx, t1 + dy), other.get_pixel(l2 + dx, t2 + dy));
            if (0..3).any(|k| (i32::from(a[k]) - i32::from(b[k])).abs() > 40) {
                differing += 1;
            }
        }
    }
    assert!(differing < 640, "the pattern is the same relative to the shape: {differing} of 6400 pixels differ");
}

/// An upright stretched picture fill is mirrored with its shape, with a soft edge or none (PowerPoint, #M104: the picture's dark band moves to the
/// other side): the flipped picture is the unflipped one mirrored across the shape's box.
fn picture_mirrored() {
    let fill = |media| Fill::Image(media);
    for soft in [None, Some(RAD)] {
        for (flip_v, name) in [(false, "h"), (true, "v")] {
            let plain = picture_shot_flipped(&fill, false, false, soft, &format!("plain-{name}"));
            let flipped = picture_shot_flipped(&fill, !flip_v, flip_v, soft, &format!("flipped-{name}"));
            let (mut x0, mut x1, mut y0, mut y1) = (u32::MAX, 0, u32::MAX, 0);
            // (The box of what differs between the two pictures in the slide's area, right of the thumbnails: the shape.)
            for (x, y, p) in plain.enumerate_pixels() {
                if x > 600 && x < plain.width() * 9 / 10 && y > 150 && p != flipped.get_pixel(x, y) {
                    (x0, x1, y0, y1) = (x0.min(x), x1.max(x), y0.min(y), y1.max(y));
                }
            }
            let (mut sum, mut n) = (0.0f64, 0u32);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let (mx, my) = if flip_v { (x, y0 + y1 - y) } else { (x0 + x1 - x, y) };
                    let (a, b) = (flipped.get_pixel(x, y), plain.get_pixel(mx, my));
                    sum += (0..3).map(|c| (f64::from(a[c]) - f64::from(b[c])).abs()).sum::<f64>() / 3.0;
                    n += 1;
                }
            }
            let mean = sum / f64::from(n) / 255.0;
            let unmirrored: f64 = plain.pixels().zip(flipped.pixels()).map(|(a, b)| (0..3).map(|c| (f64::from(a[c]) - f64::from(b[c])).abs()).sum::<f64>() / 3.0).sum::<f64>() / f64::from(n) / 255.0;
            assert!(mean < 0.03 && unmirrored > mean * 3.0, "soft {soft:?} {name}: mirrored differs by {mean}, not mirrored by {unmirrored}");
        }
    }
}

/// A picture with a fill rectangle (`l` 20 %) is drawn only inside it, and the soft edge fades it from the rectangle's edge, not from the shape's
/// outline (#M104, PowerPoint): the strip between the outline and the rectangle is empty, and the picture is faint just inside the rectangle's edge.
fn fill_rect_fade() {
    let blip = |media| {
        let mut blip = ibis_doc_model::BlipFill::new(media);
        blip.fill_rect = Some([20_000, 0, 0, 0]);
        Fill::Blip(Box::new(blip))
    };
    let hard = picture_shot(&blip, 0, false, None, None, "fillrect-hard");
    let soft = picture_shot(&blip, 0, false, None, Some(RAD), "fillrect-soft");
    // The picture's colours (red or blue) against white: the picture's extent along a row in the middle of the slide area.
    let ink = |p: &image::Rgba<u8>| (p[0] > 200 && p[2] < 60) || (p[2] > 200 && p[0] < 60) || (p[1] < 230 && (p[0] < 120 || p[2] < 120));
    let y = hard.height() / 2;
    let xs: Vec<u32> = (601..hard.width() * 9 / 10).filter(|&x| ink(hard.get_pixel(x, y))).collect();
    let (left, right) = (*xs.first().expect("the picture"), *xs.last().unwrap());
    // Just inside the rectangle's left edge (3 px) the faded picture is faint; further in (a third of the way) it is whole.
    let faint = (255.0 - f32::from(soft.get_pixel(left + 3, y)[1])) / (255.0 - f32::from(hard.get_pixel(left + 3, y)[1]).min(254.0));
    assert!(faint < 0.35, "faint at the rectangle's edge: {faint}");
    let whole_x = left + (right - left) / 3;
    let kept = (255.0 - f32::from(soft.get_pixel(whole_x, y)[1])) / (255.0 - f32::from(hard.get_pixel(whole_x, y)[1]).min(254.0));
    assert!(kept > 0.9, "whole inside: {kept}");
}

/// A tiled picture, and a cropped one, in a turned or flipped shape: drawn as with the soft edge (which PowerPoint's measurements agree with, #M104),
/// where the fade leaves the picture whole.
fn turned_and_flipped_tiles() {
    let tiled = |media| Fill::TileImage { media, tx: 0, ty: 0, sx: 50_000, sy: 50_000, flip: ibis_doc_model::TileFlip::None, align: ibis_doc_model::TileAlign::TopLeft };
    let cropped = |media| {
        let mut blip = ibis_doc_model::BlipFill::new(media);
        blip.crop = ibis_doc_model::Crop { left: 10_000, top: 20_000, right: 30_000, bottom: 0 };
        Fill::Blip(Box::new(blip))
    };
    let cases: [(&str, &dyn Fn(ibis_doc_model::MediaId) -> Fill, i32, bool); 4] = [
        ("tile-turned", &tiled, 1_800_000, false),
        ("tile-flipped", &tiled, 0, true),
        ("crop-flipped", &cropped, 0, true),
        ("tile-turned-flipped", &tiled, 1_800_000, true),
    ];
    for (name, fill, rotation, flip) in cases {
        let hard = picture_shot(fill, rotation, flip, None, None, &format!("{name}-hard"));
        let soft = picture_shot(fill, rotation, flip, None, Some(RAD), &format!("{name}-soft"));
        assert!(same_colours_where_whole(&hard, &soft, name) > 3_000, "{name}");
    }
}

/// A red rectangle (soft edge 10 pt) on white with `effects` set on it: the screenshot, and the rectangle's left, right, top and bottom pixel
/// (read from the same rectangle with no effect and no soft edge), with the pixels a point.
fn shot_effects(effects: impl Fn(&mut ibis_doc_model::AutoShape), label: &str) -> (image::RgbaImage, (u32, u32, u32, u32), f32) {
    let hard = shot(Geometry::Rect, 0, Fill::Solid(RED), None, None, &format!("{label}-hard"));
    let (cx, cy) = centre_of_shape(&hard);
    let (l, r, t, b) = extent(&hard, cx, cy);
    let px_pt = (r - l + 1) as f32 / (4_000_000.0 / 12_700.0);
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| {
        let size = e.presentation().size_of(e.slide());
        let back = e.document.ids().new_shape_id();
        let white = ibis_doc_model::AutoShape::new(Geometry::Rect, Fill::Solid(Color { rgb: Rgb(0xFFFFFF), alpha: 255 }), None);
        e.insert_shape(Shape::new(back, EmuRect::new(0, 0, size.width, size.height), ShapeKind::Auto(white)), cx);
        let id = e.document.ids().new_shape_id();
        let mut auto = ibis_doc_model::AutoShape::new(Geometry::Rect, Fill::Solid(RED), None);
        auto.soft_edge = Some(RAD);
        effects(&mut auto);
        e.insert_shape(Shape::new(id, EmuRect::new(1_500_000, 1_200_000, 4_000_000, 3_000_000), ShapeKind::Auto(auto)), cx);
    });
    s.key("escape");
    let path = s.screenshot(label);
    (image::open(path).expect("the screenshot").to_rgba8(), (l, r, t, b), px_pt)
}

/// The shadow of a soft-edged shape is made from the shape as it is faded (PowerPoint, #M92 section 5: 0.0012 against 0.119 for the hard shape's),
/// not drawn at all and not from the whole outline. A black shadow (sharp, 10 pt straight down) of a red rectangle with a soft edge of 10 pt:
/// 1 pt under the bottom edge it is the fade's value 9 pt inside the edge (half way is at 8.7 pt: about 0.54), 5 pt under it the fade's value 5 pt
/// inside (about 0.13).
fn shadow_of_the_faded_shape() {
    let (img, (l, r, _, b), px_pt) = shot_effects(|a| a.shadow = Some(ibis_doc_model::Shadow::simple(Color { rgb: Rgb(0), alpha: 255 }, 0, RAD, 5_400_000)), "shadow-soft");
    let col = (l + r) / 2;
    let at = |t_pt: f32| 1.0 - f32::from(img.get_pixel(col, b + (t_pt * px_pt).round() as u32)[1]) / 255.0;
    let (near, far) = (at(1.0), at(5.0));
    assert!((near - 0.54).abs() < 0.14, "the shadow 1 pt under the bottom edge: {near} (want about 0.54)");
    assert!((far - 0.13).abs() < 0.10, "the shadow 5 pt under the bottom edge: {far} (want about 0.13)");
}

/// The glow of a soft-edged shape is made from the shape before the fade (#M92 section 5: it is the same with and without the soft edge, and it
/// goes on under the faded fill): a green glow of 10 pt round a red rectangle with a soft edge, 1 pt inside the left edge where the red is gone,
/// and 1 pt outside it, is green in both places.
fn glow_of_the_whole_shape() {
    let (img, (l, _, _, _), px_pt) = shot_effects(|a| a.glow = Some((Color { rgb: Rgb(0x00FF00), alpha: 255 }, 127_000)), "glow-soft");
    let row = img.height() / 2;
    for (name, x) in [("inside", l + px_pt.round() as u32), ("outside", l - px_pt.round() as u32)] {
        let p = img.get_pixel(x, row);
        assert!(p[0] < 110 && p[1] > 200, "{name}: the glow is there, green: {:?}", p.0);
    }
}

/// A picture (a picture shape, not a picture fill) cut to an ellipse fades by the same formula as a filled ellipse (it was made with an SVG
/// `feMorphology`, a square erosion, which does not fade a curve as a straight edge): the fraction of the picture's colour that is left at each pixel
/// (against the same picture with no soft edge) equals the filled ellipse's opacity.
fn picture_shape() {
    let shot_picture = |soft: Option<i64>, label: &str| {
        let mut s = Scenario::open("test/editing/pptx-plan.pptx");
        s.update(|e, _, cx| {
            let size = e.presentation().size_of(e.slide());
            let back = e.document.ids().new_shape_id();
            let white = ibis_doc_model::AutoShape::new(Geometry::Rect, Fill::Solid(Color { rgb: Rgb(0xFFFFFF), alpha: 255 }), None);
            e.insert_shape(Shape::new(back, EmuRect::new(0, 0, size.width, size.height), ShapeKind::Auto(white)), cx);
            let data = two_colour_png();
            let media = e.document.ids().add_media(ibis_doc_model::MediaFormat::Png, data.clone());
            e.resources.add_picture(media, ibis_doc_model::MediaFormat::Png, &data);
            let id = e.document.ids().new_shape_id();
            let mut picture = ibis_doc_model::Picture::new(media);
            picture.geometry = Some(Geometry::Ellipse);
            picture.soft_edge = soft;
            e.insert_shape(Shape::new(id, EmuRect::new(1_500_000, 1_200_000, 4_000_000, 3_000_000), ShapeKind::Picture(picture)), cx);
        });
        s.key("escape");
        s.settle();
        image::open(s.screenshot(label)).expect("the screenshot").to_rgba8()
    };
    let hard = shot_picture(None, "pictureshape-hard");
    let soft = shot_picture(Some(RAD), "pictureshape-soft");
    let ellipse = shot(Geometry::Ellipse, 0, Fill::Solid(RED), None, Some(RAD), "pictureshape-ellipse");
    let ellipse_hard = shot(Geometry::Ellipse, 0, Fill::Solid(RED), None, None, "pictureshape-ellipse-hard");
    let (mut worst, mut compared) = (0.0f32, 0);
    for (x, y, h) in hard.enumerate_pixels() {
        let away = |p: &image::Rgba<u8>| 3.0 * 255.0 - f32::from(p[0]) - f32::from(p[1]) - f32::from(p[2]);
        // (The slide's own area: the thumbnails at the left are not compared.)
        if x > 600 && away(h) > 150.0 && alpha(&ellipse_hard, x, y) > 0.99 {
            let kept = away(soft.get_pixel(x, y)) / away(h);
            worst = worst.max((kept - alpha(&ellipse, x, y)).abs());
            compared += 1;
        }
    }
    assert!(compared > 10_000, "{compared}");
    assert!(worst < 0.2, "the picture's fade differs from the filled ellipse's by {worst}");
}

/// Two red rectangles side by side (touching), on a white page: the screenshot, with the two put in a group that has `soft` as its own soft edge
/// (`group_soft`), or each with its own `own` soft edge.
fn shot_pair(group_soft: Option<i64>, own: Option<i64>, label: &str) -> image::RgbaImage {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| {
        let size = e.presentation().size_of(e.slide());
        let back = e.document.ids().new_shape_id();
        let white = ibis_doc_model::AutoShape::new(Geometry::Rect, Fill::Solid(Color { rgb: Rgb(0xFFFFFF), alpha: 255 }), None);
        e.insert_shape(Shape::new(back, EmuRect::new(0, 0, size.width, size.height), ShapeKind::Auto(white)), cx);
        let mut ids = Vec::new();
        for left in [1_500_000, 3_500_000] {
            let id = e.document.ids().new_shape_id();
            let mut auto = ibis_doc_model::AutoShape::new(Geometry::Rect, Fill::Solid(RED), None);
            auto.soft_edge = own;
            e.insert_shape(Shape::new(id, EmuRect::new(left, 1_200_000, 2_000_000, 3_000_000), ShapeKind::Auto(auto)), cx);
            ids.push(id);
        }
        if group_soft.is_some() {
            let slide = e.current_slide;
            let mut group = ibis_doc_model::ShapeGroup::from_shape_ids("900", "pair", &ids);
            group.soft_edge = group_soft;
            let mut groups = e.slide().groups.clone();
            groups.push(group);
            e.run(vec![ibis_doc_model::Command::SetShapeGroups { slide, groups }], cx);
        }
    });
    s.key("escape");
    let path = s.screenshot(label);
    image::open(path).expect("the screenshot").to_rgba8()
}

/// A group's own soft edge (`p:grpSpPr/a:effectLst/a:softEdge`) fades its children as one picture: the outline of the two rectangles together is
/// what fades, so the seam between them stays whole; each rectangle with a soft edge of its own fades along the seam too.
fn group_faded_as_one() {
    let hard = shot_pair(None, None, "pair-hard");
    let grouped = shot_pair(Some(RAD), None, "pair-group");
    let apart = shot_pair(None, Some(RAD), "pair-own");
    let (cx, cy) = centre_of_shape(&hard);
    let (l, r, t, _) = extent(&hard, cx, cy);
    // The seam is where the two rectangles meet: half way along the row (they are of the same width).
    let seam = (l + r) / 2;
    // (The seam pixel itself is two half-covered edges laid over one another, 0.75: a thin line, as the shapes make with no soft edge.)
    assert!(alpha(&grouped, seam - 3, cy) > 0.99 && alpha(&grouped, seam + 3, cy) > 0.99 && alpha(&grouped, seam, cy) > 0.7, "the group's seam stays whole");
    assert!(alpha(&apart, seam, cy) < 0.2, "rectangles with a soft edge each fade along the seam: {}", alpha(&apart, seam, cy));
    // The outer edge fades as a single shape's does: gone at the outline, half at 0.87 of the radius inside it.
    let px_pt = (r - l + 1) as f32 / (4_000_000.0 / 12_700.0);
    assert!(alpha(&grouped, l, cy) < 0.01, "the group's outline is faded away");
    let half = alpha(&grouped, l + (8.7 * px_pt).round() as u32, cy);
    assert!((half - 0.5).abs() < 0.08, "half way at 8.7 pt: {half}");
    let half_top = alpha(&grouped, seam, t + (8.7 * px_pt).round() as u32);
    assert!((half_top - 0.5).abs() < 0.08, "half way at 8.7 pt from the top: {half_top}");
}
