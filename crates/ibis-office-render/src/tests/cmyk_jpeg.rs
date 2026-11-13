// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `test/editing/cmyk-inks.jpg` (Pillow, Adobe `APP14`, inverted inks): quadrants of pure C, M, Y and K at the top, a cyan ramp
//! under them. The picture is read as the inks it holds and the converter in use makes the colours.

use super::{adobe_transform, decode};
use ibis_doc_model::{Cmyk, CmykConverter, OpenCmykConverter};

fn file() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/cmyk-inks.jpg")).expect("the test picture")
}

fn near(a: image::Rgba<u8>, b: ibis_doc_model::Rgb, tolerance: i32) -> bool {
    (a[0] as i32 - b.r() as i32).abs() <= tolerance && (a[1] as i32 - b.g() as i32).abs() <= tolerance && (a[2] as i32 - b.b() as i32).abs() <= tolerance
}

#[test]
fn the_picture_is_adobe_cmyk() {
    assert_eq!(adobe_transform(&file()), Some(0));
}

#[test]
fn the_inks_are_read_as_they_are_and_the_converter_makes_the_colours() {
    let converter = OpenCmykConverter;
    let img = decode(&file(), &converter).expect("a four-component JPEG");
    assert_eq!((img.width(), img.height()), (64, 64));
    for ((x, y), ink) in [((8, 8), Cmyk::new(100, 0, 0, 0)), ((56, 8), Cmyk::new(0, 100, 0, 0)), ((8, 24), Cmyk::new(0, 0, 100, 0)), ((56, 24), Cmyk::new(0, 0, 0, 100))] {
        let want = converter.to_rgb(ink);
        let got = *img.get_pixel(x, y);
        assert!(near(got, want, 6), "{ink:?} at ({x},{y}): {got:?}, the converter gives {want:?}");
    }
    // The ramp goes from no ink to full cyan, a step at a time (no steps of a percent: the converter is read on fractions).
    let a = img.get_pixel(0, 48)[0] as i32;
    let b = img.get_pixel(63, 48)[0] as i32;
    assert!(a > 240 && b < 20, "paper to cyan: red {a} to {b}");
    let mut last = 256;
    for x in 0..64 {
        let r = img.get_pixel(x, 48)[0] as i32;
        assert!(r <= last + 3, "the ramp does not rise: {last} then {r} at {x}");
        last = last.min(r);
    }
}

#[test]
fn another_converter_makes_other_colours() {
    /// A converter that maps every ink to the same grey.
    struct Grey;
    impl CmykConverter for Grey {
        fn name(&self) -> String {
            "grey".into()
        }
        fn to_rgb(&self, _: Cmyk) -> ibis_doc_model::Rgb {
            ibis_doc_model::Rgb::from_rgb(120, 120, 120)
        }
        fn to_cmyk(&self, _: ibis_doc_model::Rgb) -> Cmyk {
            Cmyk::default()
        }
    }
    let img = decode(&file(), &Grey).expect("a four-component JPEG");
    assert_eq!(img.get_pixel(8, 8).0, [120, 120, 120, 255]);
}

#[test]
fn a_three_component_jpeg_is_not_taken() {
    let mut rgb = Vec::new();
    let img = image::RgbImage::new(8, 8);
    image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut rgb), image::ImageFormat::Jpeg).unwrap();
    assert!(decode(&rgb, &OpenCmykConverter).is_none());
}

/// A picture of a file (pptx, docx, xlsx all go through `preload::decode`) comes out as an RGB PNG, with the colours of the
/// converter in use.
#[test]
fn a_picture_of_a_file_is_drawn_as_rgb() {
    let (format, bytes) = crate::preload::metafile_picture(ibis_doc_model::MediaFormat::Jpeg, &file()).expect("drawable");
    assert!(matches!(format, gpui_kit::ImageFormat::Png), "the four-component JPEG is made into a PNG");
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png).unwrap().to_rgba8();
    let want = ibis_office_base::cmyk::converter().to_rgb(Cmyk::new(100, 0, 0, 0));
    assert!(near(*img.get_pixel(8, 8), want, 6));
}
