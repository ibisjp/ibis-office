// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A PDF PowerPoint 365 wrote (`fixtures/office-powerpoint-gradient-transparency.pdf`, made with COM from a slide of a rectangle filled with a gradient whose
//! second stop is fully transparent): the fill is a flat blue and a soft mask (luminosity, its group a gradient) makes it fade out down the rectangle.

#![cfg(feature = "interpret")]

use ibis_pdf::PdfContainer;
use ibis_pdf::interpret::interpret_page_scene;
use ibis_pdf::scene::DisplayCmd;

#[test]
fn the_rectangle_fades_by_its_soft_mask() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/office-powerpoint-gradient-transparency.pdf")).unwrap();
    let mut container = PdfContainer::from_bytes(bytes).unwrap();
    let scene = interpret_page_scene(&mut container, 0).unwrap();
    // The rectangle at 100,100 to 500,350 (slide points from the top left).
    let (width, height, rgba) = scene
        .display_list
        .iter()
        .find_map(|cmd| match cmd {
            DisplayCmd::DrawImage { bounds, width, height, rgba } if bounds.x0 == 100.0 && bounds.x1 == 500.0 && (bounds.y0 - 100.0).abs() < 0.01 => Some((*width as usize, *height as usize, rgba.clone())),
            _ => None,
        })
        .expect("the masked rectangle is a picture");
    let column = width / 2;
    let alpha = |row: usize| rgba[(row * width + column) * 4 + 3];
    let top = alpha(2);
    let middle = alpha(height / 2);
    let bottom = alpha(height - 3);
    // The mask is the gradient from opaque to transparent along the rectangle: monotone and spanning most of the range.
    assert!(top > 230, "top {top}");
    assert!(bottom < 25, "bottom {bottom}");
    assert!((middle as i32 - 128).abs() < 20, "middle {middle}");
    let steps: Vec<u8> = (0..10).map(|i| alpha(2 + i * (height - 5) / 9)).collect();
    assert!(steps.windows(2).all(|w| w[0] >= w[1]), "falling: {steps:?}");
    // The colour is the fill's flat blue (224, 96, 32 in RGB) where it shows.
    let at = (height / 4 * width + column) * 4;
    for (got, want) in rgba[at..at + 3].iter().zip([224i32, 96, 32]) {
        assert!((*got as i32 - want).abs() <= 2, "colour {:?}", &rgba[at..at + 3]);
    }
}
