// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 0.8.5 review (pptx 144 and others): the text of a custom shape whose text rectangle has no area (`<a:rect l="0" t="0" r="0" b="0"/>`, as
//! LibreOffice and others write it) was laid out in a box of no width and broken into one character a line. It is one line from that
//! point, as a shape with the full text rectangle (the control) has it. On Windows a `#[test]` of `cargo test --lib`; elsewhere run by the scenario runner (GPUI's macOS platform needs the main thread).

use crate::export::{Format, Options, export};

/// The check, run by the scenario runner (`--test scenarios -- pptx::text_rect`).
#[cfg(not(windows))]
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("pptx::text_rect::a_zero_text_rectangle_of_a_custom_shape_is_one_line", a_zero_text_rectangle_of_a_custom_shape_is_one_line)]
}

/// The number of bands of rows in `area` (points of the slide) with dark pixels, and the dark pixels' left and right edges.
fn bands(image: &image::RgbaImage, px_per_pt: f64, (x, y, w, h): (f64, f64, f64, f64)) -> (usize, Option<(u32, u32)>) {
    let (x0, y0, x1, y1) = ((x * px_per_pt) as u32, (y * px_per_pt) as u32, ((x + w) * px_per_pt) as u32, ((y + h) * px_per_pt) as u32);
    let dark = |px: u32, py: u32| image.get_pixel(px, py).0[..3].iter().all(|c| *c < 90);
    let mut rows = Vec::new();
    let (mut left, mut right) = (u32::MAX, 0);
    for py in y0..y1.min(image.height()) {
        let mut any = false;
        for px in x0..x1.min(image.width()) {
            if dark(px, py) {
                any = true;
                (left, right) = (left.min(px), right.max(px));
            }
        }
        rows.push(any);
    }
    let mut count = 0;
    let mut run = 0;
    for any in rows.into_iter().chain([false]) {
        if any {
            run += 1;
        } else {
            // A band is at least a third of a 20 pt letter tall (a stray pixel of a glyph is not a line).
            if run as f64 >= 20.0 * 0.3 * px_per_pt {
                count += 1;
            }
            run = 0;
        }
    }
    (count, (left <= right).then_some((left, right)))
}

#[cfg_attr(windows, test)]
fn a_zero_text_rectangle_of_a_custom_shape_is_one_line() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/corpus-report/pptx/custgeom-zero-text-rect.pptx");
    let presentation = ibis_pptx::read(&std::fs::read(&path).expect("the test file")).expect("a presentation");
    let slide_width = presentation.size_of(&presentation.slides[0]).width as f64;
    let out_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/pptx-text-rect");
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).unwrap();
    let options = Options {
        format: Format::Png,
        out_dir: out_dir.clone(),
        name: "slide".into(),
        width: 960,
        slides: Vec::new(),
        layout: false,
        file_name: Some("custgeom-zero-text-rect.pptx".into()),
        print_backgrounds: false,
        print_hidden_text: false,
        print_markup: true,
        color: Default::default(),
        handout: None,
        frame: false,
        handout_landscape: false,
        notes_pages: false,
        outline_pages: false,
        notes_source: None,
        handout_marks: Default::default(),
    };
    export(presentation, &options).expect("the export");
    let image = image::open(out_dir.join("slide-01.png")).expect("the picture").to_rgba8();
    let px_per_pt = image.width() as f64 * 12_700.0 / slide_width;
    for (name, top) in [("zero rect", 130.0), ("full rect (control)", 275.0)] {
        let (lines, edges) = bands(&image, px_per_pt, (20.0, top, 500.0, 100.0));
        assert_eq!(lines, 1, "{name}: {lines} lines of text, 1 expected");
        let (left, right) = edges.expect("text");
        assert!((right - left) as f64 > 20.0 * 8.0 * px_per_pt, "{name}: the line is {} px wide, at least {:.0} (8 em)", right - left, 20.0 * 8.0 * px_per_pt);
    }
}
