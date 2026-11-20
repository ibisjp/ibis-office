// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::tilted_frame;

/// The probe of K0049 (PowerPoint for Windows, a rectangle 833562 x 200055 EMU seen at 1920 px across, 6350 EMU a pixel, `orthographicFront`):
/// 75 degrees about the horizontal axis lifted by 76200 EMU is a strip 8 px high whose middle is 11.6 px below the shape's middle; 75 degrees about the
/// vertical axis shortens the width and moves it to the left.
#[test]
fn an_orthographic_turn_shortens_by_the_cosine_and_the_lift_moves_by_the_sine() {
    let tilt = |lat_deg: i32, lon_deg: i32| ibis_doc_model::Tilt { lat: lat_deg * 60_000, lon: lon_deg * 60_000, depth: 76_200, ..ibis_doc_model::Tilt::new(0, 0, 0) };
    let (w, h) = (833_562.0, 200_055.0);
    let (x, y, tw, th) = tilted_frame(tilt(75, 0), w, h, 1.0);
    assert!((th - h * 75f32.to_radians().cos()).abs() < 1.0 && (tw - w).abs() < 1e-3 && x.abs() < 1e-3);
    assert!(((y + th / 2.0) - (h / 2.0 + 76_200.0 * 75f32.to_radians().sin())).abs() < 1.0);
    let (x, _, tw, th) = tilted_frame(tilt(0, 75), w, h, 1.0);
    assert!((tw - w * 75f32.to_radians().cos()).abs() < 1.0 && (th - h).abs() < 1e-3);
    assert!(((x + tw / 2.0) - (w / 2.0 - 76_200.0 * 75f32.to_radians().sin())).abs() < 1.0);
}
