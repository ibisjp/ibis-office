// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Axial and radial shadings are drawn as gradients: the colours at the two ends and the Extend setting follow PDF 32000-1 8.7.4.5.

#![cfg(feature = "interpret")]

use ibis_pdf::PdfContainer;
use ibis_pdf::interpret::interpret_page_scene;
use ibis_pdf::scene::DisplayCmd;

fn pdf_with(shading: &str, content: &str) -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] /MediaBox [0 0 400 300] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /Resources << /Pattern << /P1 << /PatternType 2 /Shading ".to_string() + shading + " >> >> >> /Contents 4 0 R >>",
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
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

/// The picture the one shaded rectangle became: (width, height, rgba).
fn picture(shading: &str, content: &str) -> (u32, u32, Vec<u8>) {
    let mut container = PdfContainer::from_bytes(pdf_with(shading, content)).unwrap();
    let scene = interpret_page_scene(&mut container, 0).unwrap();
    scene
        .display_list
        .iter()
        .find_map(|cmd| match cmd {
            DisplayCmd::DrawImage { width, height, rgba, .. } => Some((*width, *height, rgba.clone())),
            _ => None,
        })
        .expect("the gradient is a picture")
}

fn pixel(p: &(u32, u32, Vec<u8>), x: u32, y: u32) -> [u8; 4] {
    let at = ((y * p.0 + x) * 4) as usize;
    [p.2[at], p.2[at + 1], p.2[at + 2], p.2[at + 3]]
}

const RED_TO_BLUE: &str = "/FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1";

#[test]
fn an_axial_gradient_is_red_at_one_end_and_blue_at_the_other() {
    let p = picture(&format!("<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [100 0 300 0] /Function << {RED_TO_BLUE} >> >>"), "/Pattern cs /P1 scn 100 100 200 100 re f");
    let y = p.1 / 2;
    let left = pixel(&p, 1, y);
    let right = pixel(&p, p.0 - 2, y);
    let middle = pixel(&p, p.0 / 2, y);
    assert!(left[0] > 240 && left[2] < 15 && left[3] == 255, "left end {left:?}");
    assert!(right[2] > 240 && right[0] < 15 && right[3] == 255, "right end {right:?}");
    assert!((middle[0] as i32 - 128).abs() < 8 && (middle[2] as i32 - 128).abs() < 8, "middle {middle:?}");
}

#[test]
fn extend_decides_whether_the_ends_are_painted_beyond_the_axis() {
    // The axis covers the middle half of the rectangle (150 to 250 of 100 to 300).
    let not_extended = picture(&format!("<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [150 0 250 0] /Function << {RED_TO_BLUE} >> >>"), "/Pattern cs /P1 scn 100 100 200 100 re f");
    let y = not_extended.1 / 2;
    assert_eq!(pixel(&not_extended, 1, y)[3], 0, "left of the axis without Extend");
    assert_eq!(pixel(&not_extended, not_extended.0 - 2, y)[3], 0, "right of the axis without Extend");
    let extended = picture(&format!("<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [150 0 250 0] /Extend [true true] /Function << {RED_TO_BLUE} >> >>"), "/Pattern cs /P1 scn 100 100 200 100 re f");
    let left = pixel(&extended, 1, y);
    let right = pixel(&extended, extended.0 - 2, y);
    assert!(left[0] > 240 && left[3] == 255, "left extended {left:?}");
    assert!(right[2] > 240 && right[3] == 255, "right extended {right:?}");
}

#[test]
fn a_radial_gradient_goes_from_the_inner_circle_to_the_outer_one() {
    let p = picture(&format!("<< /ShadingType 3 /ColorSpace /DeviceRGB /Coords [200 150 0 200 150 50] /Function << {RED_TO_BLUE} >> >>"), "/Pattern cs /P1 scn 150 100 100 100 re f");
    let centre = pixel(&p, p.0 / 2, p.1 / 2);
    let edge = pixel(&p, p.0 - 2, p.1 / 2);
    assert!(centre[0] > 230 && centre[2] < 25, "centre {centre:?}");
    assert!(edge[2] > 225 && edge[0] < 30, "edge {edge:?}");
}

#[test]
fn a_stitching_function_changes_its_colour_at_the_bound() {
    let function = "/FunctionType 3 /Domain [0 1] /Bounds [0.5] /Encode [0 1 0 1] /Functions [<< /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 1 0] /N 1 >> << /FunctionType 2 /Domain [0 1] /C0 [0 1 0] /C1 [0 0 1] /N 1 >>]";
    let p = picture(&format!("<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [100 0 300 0] /Function << {function} >> >>"), "/Pattern cs /P1 scn 100 100 200 100 re f");
    let y = p.1 / 2;
    let middle = pixel(&p, p.0 / 2, y);
    assert!(middle[1] > 235 && middle[0] < 20 && middle[2] < 20, "the bound is green {middle:?}");
}
