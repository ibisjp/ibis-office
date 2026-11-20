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

/// `file()` with `profile` put in as `APP2` `ICC_PROFILE` segments of `piece` bytes (written in reverse order: the sequence numbers decide).
fn with_profile(profile: &[u8], piece: usize) -> Vec<u8> {
    let jpeg = file();
    let chunks: Vec<&[u8]> = profile.chunks(piece).collect();
    let mut out = jpeg[..2].to_vec();
    for (n, chunk) in chunks.iter().enumerate().rev() {
        out.extend([0xff, 0xe2]);
        out.extend(((chunk.len() + 16) as u16).to_be_bytes());
        out.extend(b"ICC_PROFILE\0");
        out.extend([n as u8 + 1, chunks.len() as u8]);
        out.extend(*chunk);
    }
    out.extend(&jpeg[2..]);
    out
}

/// The profile inside the picture makes the colours, not the converter in use (PowerPoint's way); the pieces of a profile are joined by their numbers.
#[test]
fn the_profile_inside_the_picture_makes_the_colours() {
    let Ok(profile) = std::fs::read("/System/Library/ColorSync/Profiles/Generic CMYK Profile.icc") else { return };
    assert_eq!(super::embedded_profile(&with_profile(&profile, 20_000)).as_deref(), Some(&profile[..]));
    let own = super::EmbeddedProfile::new(&profile).expect("a CMYK profile");
    for piece in [20_000, 60_000] {
        let img = decode(&with_profile(&profile, piece), &OpenCmykConverter).expect("a four-component JPEG");
        for ((x, y), ink) in [((8, 8), [1.0, 0.0, 0.0, 0.0]), ((56, 8), [0.0, 1.0, 0.0, 0.0]), ((8, 24), [0.0, 0.0, 1.0, 0.0]), ((56, 24), [0.0, 0.0, 0.0, 1.0])] {
            assert!(near(*img.get_pixel(x, y), own.to_rgb_fractions(ink), 6), "{ink:?}");
        }
    }
    // Not the open model's colours (a profile is a different press).
    let img = decode(&with_profile(&profile, 60_000), &OpenCmykConverter).unwrap();
    assert!(!near(*img.get_pixel(8, 8), OpenCmykConverter.to_rgb(Cmyk::new(100, 0, 0, 0)), 3));
}

/// Measured on PowerPoint for Windows: a Lab-based CMYK JPEG (Adobe `APP14` transform 0, a 557 KB profile) of the `.ppt` corpus 017 against the picture PowerPoint wrote to its
/// PDF. The files are private (`target/cm/`, made by `scripts/` of the corpus; skipped when absent): the mean absolute error per channel must stay below 6, 6.5 and 9.5
/// (5.2, 5.9, 9.1 when measured; the old table path gave 12.4, 15.3, 18.7).
#[test]
fn a_lab_based_profile_in_the_picture_gives_what_powerpoint_wrote() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/cm");
    let (Ok(jpeg), Ok(truth)) = (std::fs::read(root.join("img4.jpeg")), std::fs::read(root.join("truth.rgb"))) else { return };
    let img = decode(&jpeg, &OpenCmykConverter).expect("a four-component JPEG");
    assert_eq!((img.width(), img.height()), (922, 922));
    let mut sum = [0f64; 3];
    let mut n = 0f64;
    for i in (0..922 * 922).step_by(7) {
        let p = img.as_raw();
        for k in 0..3 {
            sum[k] += (p[i * 4 + k] as f64 - truth[i * 3 + k] as f64).abs();
        }
        n += 1.0;
    }
    let mean = sum.map(|s| s / n);
    assert!(mean[0] < 6.0 && mean[1] < 6.5 && mean[2] < 9.5, "{mean:?}");
}
