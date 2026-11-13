// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-12 (#M115): the shadow (`a:outerShdw`) of a picture-filled shape. PowerPoint draws it under the picture (a turned shape and an ellipse
//! used to have it over the picture, which made the picture dark), and throws none where a picture with a fill rectangle is not drawn.
use crate::shell::scenario::Scenario;
use ibis_doc_model::{Color, EmuRect, Fill, Geometry, Rgb, Shape, ShapeKind};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::picture_shadow_20261112_the_shadow_is_under_the_picture_of_a_turned_shape_and_an_ellipse", under_the_picture),
        ("pptx::picture_shadow_20261112_no_shadow_where_the_fill_rectangle_leaves_the_picture_out", fill_rect_shadow),
    ]
}

const FRAME: (i64, i64, i64, i64) = (1_500_000, 1_200_000, 4_000_000, 3_000_000);

fn png() -> Vec<u8> {
    let picture = image::RgbaImage::from_fn(64, 64, |x, _| if x < 32 { image::Rgba([255, 200, 0, 255]) } else { image::Rgba([0, 200, 255, 255]) });
    let mut data = std::io::Cursor::new(Vec::new());
    picture.write_to(&mut data, image::ImageFormat::Png).expect("a PNG");
    data.into_inner()
}

/// The screenshot of a white slide with a picture-filled shape (and a black shadow of 9 pt towards the lower right when `shadow`).
fn shot(geometry: Geometry, rotation: i32, fill_of: &dyn Fn(ibis_doc_model::MediaId) -> Fill, shadow: bool, label: &str) -> image::RgbaImage {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| {
        let size = e.presentation().size_of(e.slide());
        let back = e.document.ids().new_shape_id();
        let white = ibis_doc_model::AutoShape::new(Geometry::Rect, Fill::Solid(Color { rgb: Rgb(0xFFFFFF), alpha: 255 }), None);
        e.insert_shape(Shape::new(back, EmuRect::new(0, 0, size.width, size.height), ShapeKind::Auto(white)), cx);
        let data = png();
        let media = e.document.ids().add_media(ibis_doc_model::MediaFormat::Png, data.clone());
        e.resources.add_picture(media, ibis_doc_model::MediaFormat::Png, &data);
        let id = e.document.ids().new_shape_id();
        let mut auto = ibis_doc_model::AutoShape::new(geometry, fill_of(media), None);
        if shadow {
            auto.shadow = Some(ibis_doc_model::Shadow::simple(Color { rgb: Rgb(0), alpha: 255 }, 0, 9 * 12_700, 2_700_000));
        }
        let mut shape = Shape::new(id, EmuRect::new(FRAME.0, FRAME.1, FRAME.2, FRAME.3), ShapeKind::Auto(auto));
        shape.rotation = rotation;
        e.insert_shape(shape, cx);
    });
    s.key("escape");
    s.settle();
    let path = s.screenshot(label);
    image::open(path).expect("the screenshot").to_rgba8()
}

/// The picture's extent on the page: the first and last columns and rows with its two colours (the window round it has its own).
fn extent(image: &image::RgbaImage) -> (u32, u32, u32, u32) {
    let (mut l, mut r, mut t, mut b) = (u32::MAX, 0, u32::MAX, 0);
    for (x, y, p) in image.enumerate_pixels() {
        let near = |c: [u8; 3]| (0..3).all(|i| p[i].abs_diff(c[i]) <= 12);
        // (The slide list to the left shows the slide too: the slide itself is right of 600 px.)
        if x > 600 && (near([255, 200, 0]) || near([0, 200, 255])) {
            (l, r, t, b) = (l.min(x), r.max(x), t.min(y), b.max(y));
        }
    }
    (l, r, t, b)
}

fn differs(a: &image::Rgba<u8>, b: &image::Rgba<u8>) -> u32 {
    (0..3).map(|i| u32::from(a[i].abs_diff(b[i]))).max().unwrap_or(0)
}

/// With and without the shadow, the picture inside a shape turned 30 degrees and inside an ellipse is the same, and the shadow shows beyond it.
fn under_the_picture() {
    let blip = |media| Fill::Image(media);
    for (geometry, rotation, name) in [(Geometry::Rect, 1_800_000, "turned"), (Geometry::Ellipse, 0, "ellipse")] {
        let plain = shot(geometry.clone(), rotation, &blip, false, &format!("pictshadow-{name}-plain"));
        let shadowed = shot(geometry, rotation, &blip, true, &format!("pictshadow-{name}"));
        let (l, r, t, b) = extent(&plain);
        // Pixels well inside the shape: as without the shadow.
        let mut checked = 0;
        for (fx, fy) in [(0.5, 0.5), (0.4, 0.5), (0.6, 0.5), (0.5, 0.4), (0.5, 0.6), (0.35, 0.45), (0.65, 0.55)] {
            let (x, y) = (l + ((r - l) as f32 * fx) as u32, t + ((b - t) as f32 * fy) as u32);
            let (a, c) = (plain.get_pixel(x, y), shadowed.get_pixel(x, y));
            assert!(differs(a, c) <= 6, "{name}: the picture at ({x}, {y}) is {a:?} without the shadow and {c:?} with it");
            checked += 1;
        }
        assert_eq!(checked, 7);
        // Beyond the shape on the lower right the shadow shows: some pixel there is not white with it and white without.
        let shown = (l..=r.min(shadowed.width() - 1)).flat_map(|x| (t..=(b + 20).min(shadowed.height() - 1)).map(move |y| (x, y))).any(|(x, y)| {
            let (a, c) = (plain.get_pixel(x, y), shadowed.get_pixel(x, y));
            a[0] > 245 && a[1] > 245 && a[2] > 245 && c[0] < 200 && c[1] < 200 && c[2] < 200
        });
        assert!(shown, "{name}: the shadow shows beside the shape");
    }
}

/// A picture with a fill rectangle (left 20 %) has no shadow in the empty strip to its left (the shadow is 9 pt to the right, so the strip's
/// middle is where the shape's own shadow would have been), and has one under its own part.
fn fill_rect_shadow() {
    let blip = |media| {
        let mut blip = ibis_doc_model::BlipFill::new(media);
        blip.fill_rect = Some([20_000, 0, 0, 0]);
        Fill::Blip(Box::new(blip))
    };
    let plain = shot(Geometry::Rect, 0, &blip, false, "pictshadow-fillrect-plain");
    let shadowed = shot(Geometry::Rect, 0, &blip, true, "pictshadow-fillrect");
    // The picture's extent without the shadow (it starts at the rectangle's left edge); the shape's frame starts 20 % of its width to the left of it.
    let (l, r, t, b) = extent(&plain);
    let frame_width = (r - l) as f32 / 0.8;
    let strip_x = l - (frame_width * 0.1) as u32;
    let y = (t + b) / 2;
    let white = |p: &image::Rgba<u8>| p[0] > 245 && p[1] > 245 && p[2] > 245;
    assert!(white(shadowed.get_pixel(strip_x, y)), "no shadow in the strip with no picture: {:?}", shadowed.get_pixel(strip_x, y));
    // Under the picture's own part: below its bottom edge there is shadow.
    let below = shadowed.get_pixel((l + r) / 2, b + 4);
    assert!(!white(below), "the shadow shows under the picture: {below:?}");
}
