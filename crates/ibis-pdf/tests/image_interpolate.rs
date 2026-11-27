// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

//! PDF 32000 8.9.5.3: an image without `/Interpolate true` (inline image: `/I true`) is not smoothed when enlarged; the display list carries the
//! pixels repeated so the shell's bilinear sampler keeps the pixel edges sharp.

#![cfg(feature = "interpret")]

use ibis_pdf::PdfContainer;
use ibis_pdf::interpret::interpret_page_scene;
use ibis_pdf::scene::DisplayCmd;
use std::io::Write as _;

/// A 2x2 RGB image (red, green / blue, white) drawn 200 pt square.
const PIXELS: [u8; 12] = [255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];

fn pdf(content: &[u8], image_extra: &str, with_xobject: bool) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n");
    let mut offsets = Vec::new();
    offsets.push(out.len());
    out.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    offsets.push(out.len());
    out.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] /MediaBox [0 0 300 300] >>\nendobj\n");
    offsets.push(out.len());
    out.extend_from_slice(b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n");
    offsets.push(out.len());
    let _ = writeln!(out, "4 0 obj\n<< /Length {} >>\nstream", content.len());
    out.extend_from_slice(content);
    out.extend_from_slice(b"\nendstream\nendobj\n");
    offsets.push(out.len());
    if with_xobject {
        let _ = writeln!(
            out,
            "5 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 2 {image_extra} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >>\nstream",
            PIXELS.len()
        );
        out.extend_from_slice(&PIXELS);
        out.extend_from_slice(b"\nendstream\nendobj\n");
    } else {
        out.extend_from_slice(b"5 0 obj\n<< >>\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for off in &offsets {
        let _ = writeln!(out, "{off:010} 00000 n ");
    }
    let _ = writeln!(out, "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF");
    out
}

fn image(bytes: Vec<u8>) -> (u32, u32, Vec<u8>) {
    let mut container = PdfContainer::from_bytes(bytes).unwrap();
    let scene = interpret_page_scene(&mut container, 0).unwrap();
    scene
        .display_list
        .iter()
        .find_map(|cmd| match cmd {
            DisplayCmd::DrawImage { width, height, rgba, .. } => Some((*width, *height, rgba.clone())),
            _ => None,
        })
        .expect("an image")
}

fn colours(rgba: &[u8]) -> std::collections::BTreeSet<[u8; 4]> {
    rgba.chunks(4).map(|p| [p[0], p[1], p[2], p[3]]).collect()
}

#[test]
fn enlarged_image_without_interpolate_keeps_only_the_source_colours() {
    let (w, h, rgba) = image(pdf(b"q 200 0 0 200 50 50 cm /Im1 Do Q", "/Interpolate false", true));
    assert!(w > 2 && h > 2 && w % 2 == 0 && h % 2 == 0, "{w}x{h}");
    assert_eq!(colours(&rgba).len(), 4, "no in-between colour");
    // The boundary between the red (left) and green (right) pixel is a hard step.
    let row = &rgba[0..w as usize * 4];
    let half = w as usize / 2;
    assert_eq!(&row[(half - 1) * 4..half * 4], &[255, 0, 0, 255]);
    assert_eq!(&row[half * 4..half * 4 + 4], &[0, 255, 0, 255]);
}

#[test]
fn missing_interpolate_is_false() {
    let (w, _, rgba) = image(pdf(b"q 200 0 0 200 50 50 cm /Im1 Do Q", "", true));
    assert!(w > 2);
    assert_eq!(colours(&rgba).len(), 4);
}

#[test]
fn interpolate_true_keeps_the_source_pixels_for_the_smooth_sampler() {
    let (w, h, rgba) = image(pdf(b"q 200 0 0 200 50 50 cm /Im1 Do Q", "/Interpolate true", true));
    assert_eq!((w, h), (2, 2));
    assert_eq!(rgba.len(), 16);
}

#[test]
fn shrunk_image_stays_as_is() {
    let (w, h, _) = image(pdf(b"q 0.5 0 0 0.5 50 50 cm /Im1 Do Q", "/Interpolate false", true));
    assert_eq!((w, h), (2, 2));
}

#[test]
fn inline_image_follows_i() {
    let mut content = b"q 200 0 0 200 50 50 cm BI /W 2 /H 2 /CS /RGB /BPC 8 /I false ID ".to_vec();
    content.extend_from_slice(&PIXELS);
    content.extend_from_slice(b"\nEI Q");
    let (w, _, rgba) = image(pdf(&content, "", false));
    assert!(w > 2, "{w}");
    assert_eq!(colours(&rgba).len(), 4);

    let mut content = b"q 200 0 0 200 50 50 cm BI /W 2 /H 2 /CS /RGB /BPC 8 /I true ID ".to_vec();
    content.extend_from_slice(&PIXELS);
    content.extend_from_slice(b"\nEI Q");
    let (w, h, _) = image(pdf(&content, "", false));
    assert_eq!((w, h), (2, 2));
}
