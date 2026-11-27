// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Soft masks (ExtGState /SMask) multiply what they cover: a luminosity mask by the grey of its group over BC, an alpha mask by the group's alpha.

#![cfg(feature = "interpret")]

use ibis_pdf::PdfContainer;
use ibis_pdf::interpret::interpret_page_scene;
use ibis_pdf::scene::DisplayCmd;

/// A page of 400 x 300 whose content is `content`, with /GS1 holding the soft mask `smask` whose group draws `group`.
fn pdf_with(smask: &str, group: &str, content: &str) -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] /MediaBox [0 0 400 300] >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /Resources << /ExtGState << /GS1 << /SMask << /Type /Mask {smask} /G 5 0 R >> >> >> >> /Contents 4 0 R >>"),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        format!("<< /Type /XObject /Subtype /Form /BBox [0 0 400 300] /Group << /S /Transparency /CS /DeviceGray >> /Length {} >>\nstream\n{group}\nendstream", group.len()),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF", objects.len() + 1).as_bytes());
    out
}

/// The picture the one masked rectangle became: (bounds x0, x1, width, height, rgba).
fn picture(smask: &str, group: &str, content: &str) -> (f32, f32, u32, u32, Vec<u8>) {
    let mut container = PdfContainer::from_bytes(pdf_with(smask, group, content)).unwrap();
    let scene = interpret_page_scene(&mut container, 0).unwrap();
    scene
        .display_list
        .iter()
        .find_map(|cmd| match cmd {
            DisplayCmd::DrawImage { bounds, width, height, rgba } => Some((bounds.x0, bounds.x1, *width, *height, rgba.clone())),
            _ => None,
        })
        .expect("the masked rectangle is a picture")
}

/// The pixel at the page position `x` (points) in the middle row.
fn at(p: &(f32, f32, u32, u32, Vec<u8>), x: f32) -> [u8; 4] {
    let px = (((x - p.0) / (p.1 - p.0)) * p.2 as f32) as u32;
    let i = (((p.3 / 2) * p.2 + px.min(p.2 - 1)) * 4) as usize;
    [p.4[i], p.4[i + 1], p.4[i + 2], p.4[i + 3]]
}

const STRIPES: &str = "0 g 0 0 133 300 re f 0.5 g 133 0 133 300 re f 1 g 266 0 134 300 re f";
const RED_BAND: &str = "/GS1 gs 1 0 0 rg 0 100 400 100 re f";

#[test]
fn a_luminosity_mask_multiplies_the_alpha_by_the_grey_of_its_group() {
    let p = picture("/S /Luminosity", STRIPES, RED_BAND);
    assert_eq!(at(&p, 60.0)[3], 0, "black hides");
    let half = at(&p, 200.0);
    assert!((half[3] as i32 - 128).abs() <= 3, "mid grey is half: {half:?}");
    let full = at(&p, 330.0);
    assert!(full[3] >= 252 && full[0] > 250 && full[1] < 5, "white shows: {full:?}");
}

#[test]
fn the_background_colour_shows_where_the_group_draws_nothing() {
    // The group paints only the right third; BC 1 (white) makes the rest show.
    let p = picture("/S /Luminosity /BC [1]", "0 g 266 0 134 300 re f", RED_BAND);
    assert!(at(&p, 60.0)[3] >= 252, "the backdrop is white");
    assert_eq!(at(&p, 330.0)[3], 0, "the black group hides");
}

#[test]
fn an_alpha_mask_follows_the_alpha_of_its_group() {
    let p = picture("/S /Alpha", "0 g 266 0 134 300 re f", RED_BAND);
    assert_eq!(at(&p, 60.0)[3], 0, "no group, no alpha");
    assert!(at(&p, 330.0)[3] >= 252, "the group is opaque");
}

#[test]
fn the_transfer_function_is_applied_to_the_mask() {
    // TR inverts: black shows and white hides.
    let p = picture("/S /Luminosity /TR << /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [0] /N 1 >>", STRIPES, RED_BAND);
    assert!(at(&p, 60.0)[3] >= 252, "black shows through the inverting TR");
    assert_eq!(at(&p, 330.0)[3], 0, "white hides");
}
