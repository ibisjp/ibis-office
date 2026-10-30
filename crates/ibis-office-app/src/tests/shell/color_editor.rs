// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::{hsv_to_rgb, rgb_to_hsv};
use ibis_doc_model::Rgb;

#[test]
fn hsv_reference_primary_colors() {
    for (hex, hue) in [
        (0xff0000, 0.),
        (0xffff00, 60.),
        (0x00ff00, 120.),
        (0x00ffff, 180.),
        (0x0000ff, 240.),
        (0xff00ff, 300.),
    ] {
        assert_eq!(hsv_to_rgb([hue, 1., 1.]), Rgb(hex));
        assert_eq!(rgb_to_hsv(Rgb(hex)), [hue, 1., 1.]);
    }
    assert_eq!(hsv_to_rgb([137., 0., 1.]), Rgb(0xffffff));
    assert_eq!(hsv_to_rgb([290., 1., 0.]), Rgb(0x000000));
}

#[test]
fn rgb_round_trip_preserves_eight_bit_channels() {
    for r in [0, 1, 32, 127, 128, 254, 255] {
        for g in [0, 1, 32, 127, 128, 254, 255] {
            for b in [0, 1, 32, 127, 128, 254, 255] {
                let color = Rgb((r << 16) | (g << 8) | b);
                assert_eq!(hsv_to_rgb(rgb_to_hsv(color)), color);
            }
        }
    }
}
