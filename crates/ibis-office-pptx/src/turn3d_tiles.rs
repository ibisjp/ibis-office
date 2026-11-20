// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The transitions made of tiles of the two slides, each tile placed by its own motion: くし形 (`comb`, flat bands), ブラインド (`blinds`) and
//! チェッカー (`checker`, tiles turning over in perspective). A tile is a quad of the slide's texture (`turn3d::Mesh` with the tile's own
//! texture points).
//!
//! Measured on PowerPoint for Windows (Microsoft 365 16.0.20527; the slideshow of `audit-trans-flat-16x9-dur2000.pptx` exported with
//! `Presentation.CreateVideo`, 1280 x 720, 30 frames a second, a 2 second transition), by the colours of the two slides' four quadrants:
//! * くし形 across (`comb(horizontal)`): 7 horizontal bands that each pull the old slide's band out sideways (alternately right and left, the top
//!   one to the right), uncovering the new slide, which stays where it is. The bands do not move together: the bottom one goes first, each band
//!   after it 2.88 frames (0.048 of the time) later; each band's share follows `cubic-bezier(0.283, 0, 0.862, 0.591)` over 35.9 frames (0.598 of
//!   the time) from 0.46 frames in. Fitted to the shares of the 7 bands over 66 frames (rms 0.0136). Squared difference of the pictures to the
//!   video 0.0042 (0.076 before).
//! * くし形 down (`comb(vertical)`): 10 vertical bands of 128 px, alternately up and down (the first up), all at the same time:
//!   `cubic-bezier(0.542, 0, 1, 0.846)` over 27.1 frames (0.452 of the time) from 0.18 frames in (rms 0.018). Squared difference 0.0011 (0.080
//!   before).
//! * ブラインド, チェッカー, 扉, 窓: see `blinds`, `checker` and `doors`.

use super::turn3d::{Card, Face, Mesh};
use gpui_kit::{MeshVertex, Transform3D};
use ibis_doc_model::transition_plan::bezier_xy;
use std::sync::Arc;

/// A flat tile: the part `src` (x0, y0, x1, y1 in the slide's pixels) of a slide's texture drawn at `dst` (the same size), the slide `w` x `h`.
fn tile(face: Face, src: [f32; 4], dx: f32, dy: f32, w: f32, h: f32) -> Card {
    let [x0, y0, x1, y1] = src;
    let at = |x: f32, y: f32| MeshVertex::new([x + dx - w / 2.0, y + dy - h / 2.0, 0.0], [x / w, y / h]);
    let vertices = Arc::new(vec![at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1)]);
    Card { face, transform: Transform3D::translate(w / 2.0, h / 2.0, 0.0), cull_back: false, shade: 1.0, mesh: Some(Mesh { vertices, indices: vec![0, 1, 2, 1, 3, 2] }), white: 0.0, opacity: 1.0 }
}

/// One band of くし形 pulled by `s` (0 to 1) of the way: `band` is its range (x0, y0, x1, y1), `along_x` whether the old slide's band
/// moves sideways, `positive` whether it moves right (or down), uncovering the new slide behind it, which stays where it is.
fn pull_band(band: [f32; 4], along_x: bool, positive: bool, s: f32, w: f32, h: f32, cards: &mut Vec<Card>) {
    let [x0, y0, x1, y1] = band;
    let len = if along_x { w } else { h };
    let shift = s * len * if positive { 1.0 } else { -1.0 };
    let (lo, hi) = if along_x { (x0, x1) } else { (y0, y1) };
    let _ = (lo, hi);
    // The new slide is whole under the band; the old band over it, `shift` along.
    let cut = |face: Face, a: f32, b: f32, d: f32, cards: &mut Vec<Card>| {
        let (a, b) = (a.max(0.0), b.min(len));
        if b - a < 0.01 {
            return;
        }
        if along_x {
            cards.push(tile(face, [a - d, y0, b - d, y1], d, 0.0, w, h));
        } else {
            cards.push(tile(face, [x0, a - d, x1, b - d], 0.0, d, w, h));
        }
    };
    cut(Face::New, 0.0, len, 0.0, cards);
    cut(Face::Old, shift, len + shift, shift, cards);
}

/// くし形 at `q` (0 to 1, linear) on a slide of `w` x `h`: `horizontal` is the across option (7 horizontal bands), else down (8 vertical bands).
pub(crate) fn comb(horizontal: bool, q: f32, w: f32, h: f32) -> Vec<Card> {
    let q = f64::from(q);
    let mut cards = Vec::new();
    if horizontal {
        const N: usize = 7;
        for i in 0..N {
            let j = (N - 1 - i) as f64;
            let u = (q - (0.46 + 2.88 * j) / 60.0) / (35.87 / 60.0);
            let s = bezier_xy(0.283, 0.0, 0.862, 0.591, u) as f32;
            let (y0, y1) = (h * i as f32 / N as f32, h * (i + 1) as f32 / N as f32);
            pull_band([0.0, y0, w, y1], true, i % 2 == 0, s, w, h, &mut cards);
        }
    } else {
        const N: usize = 10;
        let u = (q - 0.18 / 60.0) / (27.1 / 60.0);
        let s = bezier_xy(0.542, 0.0, 1.0, 0.846, u) as f32;
        for i in 0..N {
            let (x0, x1) = (w * i as f32 / N as f32, w * (i + 1) as f32 / N as f32);
            pull_band([x0, 0.0, x1, h], false, i % 2 == 1, s, w, h, &mut cards);
        }
    }
    cards
}

/// A tile of a slide's texture, `rect` (x0, y0, x1, y1 in the slide's pixels), turned `angle` about its own horizontal (`across_x`:
/// the axis along x through its middle) or vertical axis, seen in perspective of focal length `focal` from the slide's middle.
fn turned_tile(face: Face, rect: [f32; 4], horizontal_axis: bool, angle: f32, focal: f32, w: f32, h: f32) -> Card {
    let [x0, y0, x1, y1] = rect;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let at = |x: f32, y: f32| MeshVertex::new([x, y, 0.0], [x / w, y / h]);
    let vertices = Arc::new(vec![at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1)]);
    let rot = if horizontal_axis { Transform3D::rotate_x(angle) } else { Transform3D::rotate_y(angle) };
    let transform = Transform3D::translate(-cx, -cy, 0.0).then(rot).then(Transform3D::translate(cx - w / 2.0, cy - h / 2.0, 0.0)).then(Transform3D::perspective(focal)).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0));
    Card { face, transform, cull_back: true, shade: 1.0, mesh: Some(Mesh { vertices, indices: vec![0, 1, 2, 1, 3, 2] }), white: 0.0, opacity: 1.0 }
}

/// ブラインド at `q` (0 to 1, linear): `n` slats, each turning over about its own middle line, the old slide on its front and the new one on its
/// back, seen in perspective, with the new slide itself staying where it is behind them. The turns start in the middle (the middle slats
/// at 6 frames of the 60) and go out to both ends 2.9 frames per slat later, each taking 20 frames, at an even speed; the camera's focal
/// length 1.33 times the slide's width. Fitted to the video of the horizontal option (18 pictures, the blurred squared difference 0.011; the
/// slats alone on black 0.016, the earlier flat slats 0.036); the vertical option (18 slats about their vertical lines) fitted the same way: start 6.75, 2.2
/// frames per slat, 17 frames a turn, focal length 9.58 times the width (0.016 against 0.044 before).
pub(crate) fn blinds(horizontal: bool, n: usize, q: f32, w: f32, h: f32) -> Vec<Card> {
    // (start of the middle slats in frames of 60, frames per slat from the middle, frames a turn takes, focal length over the slide's width)
    let (start, per_slat, turn, focal) = if horizontal { (6.0, 2.9, 20.0, 1.33) } else { (6.75, 2.2, 17.0, 9.58) };
    let k = q * 60.0;
    let mut cards = vec![Card::flat(Face::New, Transform3D::identity(), false, 1.0)];
    for i in 0..n {
        let order = (i as f32 - (n as f32 - 1.0) / 2.0).abs();
        let angle = std::f32::consts::PI * ((k - (start + per_slat * order)) / turn).clamp(0.0, 1.0);
        let rect = if horizontal { [0.0, h * i as f32 / n as f32, w, h * (i + 1) as f32 / n as f32] } else { [w * i as f32 / n as f32, 0.0, w * (i + 1) as f32 / n as f32, h] };
        cards.push(turned_tile(Face::Old, rect, horizontal, angle, focal * w, w, h));
        cards.push(turned_tile(Face::New, rect, horizontal, angle - std::f32::consts::PI, focal * w, w, h));
    }
    cards
}

/// The frame (of 60) at which each square of チェッカー turns edge-on, `[row][column]`: the across option (`checker(horizontal)`) turns about
/// vertical lines and goes on from the left to the right, the down option about horizontal lines and goes on from the top downwards, each square
/// with a time of its own besides (read off the video: the frame at which the square's middle shows the new slide; the squares the label and the
/// circle hide in the picture have their column's (across) or row's (down) mean).
const CHECKER_ACROSS: [[f32; 7]; 5] = [
    [14.0, 28.0, 26.0, 25.0, 28.0, 36.0, 39.0],
    [23.0, 22.0, 28.0, 28.0, 25.0, 40.0, 33.0],
    [19.0, 20.0, 21.0, 25.0, 25.0, 34.0, 40.0],
    [24.0, 18.0, 30.0, 25.0, 34.0, 27.0, 30.0],
    [10.0, 16.0, 26.0, 22.0, 26.0, 31.0, 34.0],
];
const CHECKER_DOWN: [[f32; 7]; 5] = [
    [14.0, 17.0, 17.0, 17.0, 17.0, 19.0, 17.0],
    [28.0, 24.0, 26.0, 23.0, 15.0, 27.0, 16.0],
    [29.0, 27.0, 24.0, 26.0, 22.0, 26.0, 30.0],
    [39.0, 30.0, 39.0, 33.0, 38.0, 27.0, 26.0],
    [31.0, 34.0, 40.0, 32.0, 35.0, 36.0, 35.0],
];

/// チェッカー at `q` (0 to 1, linear): 7 x 5 squares, each turning over (the old slide on its front, the new one on its back) in perspective
/// over black.
pub(crate) fn checker(across: bool, q: f32, w: f32, h: f32) -> Vec<Card> {
    // (frames a turn takes, focal length over the slide's width, shift of the table's frames): fitted to the video of each option (18 pictures;
    // the blurred squared difference 0.094 and 0.095 before, 0.012 and 0.015 now).
    let (turn, focal, shift) = if across { (20.0, 1.48, -3.0) } else { (20.0, 6.73, -4.0) };
    let table = if across { &CHECKER_ACROSS } else { &CHECKER_DOWN };
    let k = q * 60.0;
    let mut cards = Vec::new();
    for (r, row) in table.iter().enumerate() {
        for (c, half) in row.iter().enumerate() {
            let rect = [w * c as f32 / 7.0, h * r as f32 / 5.0, w * (c + 1) as f32 / 7.0, h * (r + 1) as f32 / 5.0];
            let angle = std::f32::consts::PI * ((k - (half + shift - turn / 2.0)) / turn).clamp(0.0, 1.0);
            cards.push(turned_tile(Face::Old, rect, !across, angle, focal * w, w, h));
            cards.push(turned_tile(Face::New, rect, !across, angle - std::f32::consts::PI, focal * w, w, h));
        }
    }
    cards
}

/// A number in 0..1 from a hexagon's place in the lattice and the `salt` of what it is for (splitmix64): the hexagons' own times are
/// random in PowerPoint, so only the way they are spread is reproduced, not which hexagon goes when.
fn lattice_random(i: i32, j: i32, salt: u64) -> f32 {
    let mut z = (i as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (j as i64 as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F) ^ salt.wrapping_mul(0x1656_67B1_9E37_79F9);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

/// `polygon` (points) clipped to the rectangle 0..w x 0..h (Sutherland-Hodgman; the polygon is convex).
fn clip_to_slide(polygon: Vec<(f32, f32)>, w: f32, h: f32) -> Vec<(f32, f32)> {
    let mut out = polygon;
    for edge in 0..4 {
        let inside = |p: (f32, f32)| match edge {
            0 => p.0 >= 0.0,
            1 => p.0 <= w,
            2 => p.1 >= 0.0,
            _ => p.1 <= h,
        };
        let cut = |a: (f32, f32), b: (f32, f32)| {
            let t = match edge {
                0 => (0.0 - a.0) / (b.0 - a.0),
                1 => (w - a.0) / (b.0 - a.0),
                2 => (0.0 - a.1) / (b.1 - a.1),
                _ => (h - a.1) / (b.1 - a.1),
            };
            (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
        };
        let input = std::mem::take(&mut out);
        for (n, &cur) in input.iter().enumerate() {
            let prev = input[(n + input.len() - 1) % input.len()];
            match (inside(prev), inside(cur)) {
                (true, true) => out.push(cur),
                (true, false) => out.push(cut(prev, cur)),
                (false, true) => {
                    out.push(cut(prev, cur));
                    out.push(cur);
                }
                (false, false) => {}
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

/// A fan of triangles over the convex `polygon` (slide pixels), the texture the slide's own.
fn polygon_card(face: Face, polygon: &[(f32, f32)], transform: Transform3D, opacity: f32, w: f32, h: f32) -> Option<Card> {
    if polygon.len() < 3 {
        return None;
    }
    let n = polygon.len() as f32;
    let (cx, cy) = (polygon.iter().map(|p| p.0).sum::<f32>() / n, polygon.iter().map(|p| p.1).sum::<f32>() / n);
    let at = |x: f32, y: f32| MeshVertex::new([x, y, 0.0], [x / w, y / h]);
    let mut vertices = vec![at(cx, cy)];
    vertices.extend(polygon.iter().map(|&(x, y)| at(x, y)));
    let count = polygon.len() as u32;
    let indices = (0..count).flat_map(|k| [0, 1 + k, 1 + (k + 1) % count]).collect();
    Some(Card { face, transform, cull_back: false, shade: 1.0, mesh: Some(Mesh { vertices: Arc::new(vertices), indices }), white: 0.0, opacity })
}

/// ハニカム at `q` (0 to 1, linear): the slide is a plane of hexagons, each of its own on the lattice below, seen over black.
///
/// Measured on PowerPoint for Windows (Microsoft 365 16.0.20527; the slideshow of `audit-trans-flat-16x9-dur2000.pptx` exported with
/// `Presentation.CreateVideo`, 1280 x 720, 30 frames a second, a 2 second transition = 60 frames; the planes' places fitted to the video's
/// frames as homographies (a plane turned about the viewing axis, moved and seen in perspective of focal length 1000 for a slide 640
/// wide), 0.002 to 0.008 squared difference over the frames where a plane could be fitted), `k` the frame:
/// * The lattice: PowerPoint makes regular hexagons (a point up and down, vertical sides, rows 1.5 r down for a width of the square root of 3 times
///   r) in the coordinates of a 4:3 slide and stretches them across with the slide's shape (measured on the 16:9 video only: the autocorrelation
///   of the gap lines in its frames, 1280 x 720, frames 6 to 8, gives centres 151 apart across and the next row 95 down and 75 across, 1.37 times
///   wider than a regular lattice of that row; 16:9 over 4:3 is 1.333; a video of a 4:3 slide to confirm that the hexagons stay regular there is
///   to be made on Windows: tasks/doc/requests/k88-honeycomb-4x3-video.md). Here: radius 30 in a frame of 480 x 360 (rows 45 down in the 360 high
///   slide as measured; across 69.3 against the 71 fitted), stretched to 640 x 360 (and to any slide's shape). The gap is 3.75 in the frame (5 across
///   the 16:9 slide, as measured).
/// * The old plane: from the start it moves with an even acceleration (the fit is exact to 1 frame in the frames 1 to 22): turned about the
///   viewing axis by -0.00089 k^2 radians, moved by (0.078, -0.037) k^2 and toward the viewer by 1.62 k^2, so it is carried through the camera
///   (focal length 1000) at frame 23.5 and gone.
/// * The gaps of the old plane (the lines) go dark from the left to the right: half dark at k = 3.9 + 0.0123 x (x across the 640), over 6 frames.
/// * Each old hexagon goes (fades to black in 3 frames) at a time of its own, spread evenly from frame 7 to 45 (the share of those still there
///   falls from 1 at frame 7 to 0.5 at frame 22 of the hexagons in the picture, and the ones seen late are the middle ones); which goes when is
///   random, and is here a hash of the hexagon's place.
/// * The new plane settles from far and turned: with m = 60 - k its turn is 0.00076 m^2 radians, its move (0.0205, -0.007) m^2, and away
///   from the viewer 0.0135 m^3 (fitted to frames 24 to 70 of the video, m' of the law found equal to m within 1 frame to m = 24, within 2 to 34).
/// * Each new hexagon comes in (over 1.5 frames) at a time of its own, spread evenly from frame 24 to 55 (the share present: 0.04 at 24,
///   0.45 at 40, 0.75 at 48, 1 at 55). The new plane's gap lines show from the left to the right at k = 37 + 0.0125 x (over 2 frames) and go
///   from the right to the left at k = 57 - 0.008 x (over 3 frames), the plane then being the new slide itself.
pub(crate) fn honeycomb(q: f32, w: f32, h: f32) -> Vec<Card> {
    // The lattice, in a frame of 480 x 360 (4:3), where the hexagons are regular: radius R, rows 1.5 R down. The slide's picture is that frame
    // stretched to the slide's own shape (the slide 640 x 360 the figures above are in: across by 640 / 480; a 4:3 slide keeps them regular).
    const R: f32 = 30.0;
    const ORIGIN: (f32, f32) = (25.5, 46.0);
    const GAP: f32 = 3.75;
    const WIDE: f32 = 640.0 / 480.0;
    let r = R;
    let root3 = 3.0_f32.sqrt();
    let a_step = (root3 * r, 0.0_f32);
    let b_step = (root3 * r / 2.0, 1.5 * r);
    let cell = [(root3 * r / 2.0, r / 2.0), (0.0, r), (-root3 * r / 2.0, r / 2.0), (-root3 * r / 2.0, -r / 2.0), (0.0, -r), (root3 * r / 2.0, -r / 2.0)];
    let inset = |d: f32| {
        let apothem = root3 * r / 2.0 - d;
        let rr = apothem * 2.0 / root3;
        [(apothem, rr / 2.0), (0.0, rr), (-apothem, rr / 2.0), (-apothem, -rr / 2.0), (0.0, -rr), (apothem, -rr / 2.0)]
    };
    let inner = inset(GAP / 2.0);
    let (sx, sy) = (w / 640.0, h / 360.0);
    let focal = 1000.0 * sx;
    let k = q * 60.0;
    let place = |rz: f32, tx: f32, ty: f32, tz: f32| {
        Transform3D::translate(-w / 2.0, -h / 2.0, 0.0)
            .then(Transform3D::rotate_z(rz))
            .then(Transform3D::translate(tx * sx, ty * sx, tz * sx))
            .then(Transform3D::perspective(focal))
            .then(Transform3D::translate(w / 2.0, h / 2.0, 0.0))
    };
    let ramp = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let old_plane = (k < 23.4).then(|| {
        let e = k * k;
        place(-0.00089 * e, 0.078 * e, -0.037 * e, 1.62 * e)
    });
    let new_plane = (k >= 24.0).then(|| {
        let m = 60.0 - k;
        place(0.00076 * m * m, 0.0205 * m * m, -0.007 * m * m, -0.0135 * m * m * m)
    });
    let mut cards = Vec::new();
    for (face, plane) in [(Face::Old, old_plane), (Face::New, new_plane)] {
        let Some(transform) = plane else { continue };
        for i in -14..=24 {
            for j in -6..=16 {
                let c = (ORIGIN.0 + i as f32 * a_step.0 + j as f32 * b_step.0, ORIGIN.1 + i as f32 * a_step.1 + j as f32 * b_step.1);
                let shape = |corners: &[(f32, f32); 6]| clip_to_slide(corners.iter().map(|p| ((c.0 + p.0) * WIDE * sx, (c.1 + p.1) * sy)).collect(), w, h);
                let outer = shape(&cell);
                if outer.len() < 3 {
                    continue;
                }
                let (alpha, gap) = if face == Face::Old {
                    let gone = 7.0 + 38.0 * lattice_random(i, j, 1);
                    (1.0 - ((k - gone) / 3.0).clamp(0.0, 1.0), ramp((k - (3.9 + 0.0123 * c.0 * WIDE)) / 6.0 + 0.5))
                } else {
                    let came = 24.0 + 31.0 * lattice_random(i, j, 2);
                    ((((k - came) / 1.5).clamp(0.0, 1.0)), ramp((k - (37.0 + 0.0125 * c.0 * WIDE)) / 2.0 + 0.5) * (1.0 - ramp((k - (57.0 - 0.008 * c.0 * WIDE)) / 3.0 + 0.5)))
                };
                if alpha <= 0.0 {
                    continue;
                }
                if gap < 0.997 {
                    cards.extend(polygon_card(face, &outer, transform, alpha * (1.0 - gap), w, h));
                }
                if gap > 0.003 {
                    cards.extend(polygon_card(face, &clip_to_slide(inner.iter().map(|p| ((c.0 + p.0) * WIDE * sx, (c.1 + p.1) * sy)).collect(), w, h), transform, alpha, w, h));
                }
            }
        }
    }
    cards
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doors_end_with_the_new_slide_alone() {
        for window in [false, true] {
            for horizontal in [false, true] {
                let end = doors(window, horizontal, 1.0, 640.0, 360.0);
                // The halves are edge-on or gone by the end; what is in front is the new slide.
                assert!(end.last().is_some_and(|c| c.face == Face::New), "{window} {horizontal}");
                let start = doors(window, horizontal, 0.0, 640.0, 360.0);
                assert!(start.iter().filter(|c| c.face == Face::Old && c.opacity > 0.0).count() == 2);
            }
        }
    }

    #[test]
    fn a_honeycomb_starts_with_the_old_slide_whole_and_ends_with_the_new_one_whole() {
        let start = honeycomb(0.0, 640.0, 360.0);
        assert!(!start.is_empty() && start.iter().all(|c| c.face == Face::Old && c.opacity == 1.0));
        let end = honeycomb(1.0, 640.0, 360.0);
        assert!(!end.is_empty() && end.iter().all(|c| c.face == Face::New && c.opacity == 1.0));
        // The old plane has gone through the camera by frame 24, and the new one has not come before it.
        assert!(honeycomb(24.5 / 60.0, 640.0, 360.0).iter().all(|c| c.face == Face::New));
        assert!(honeycomb(10.0 / 60.0, 640.0, 360.0).iter().all(|c| c.face == Face::Old));
    }

    #[test]
    fn a_comb_shows_only_the_old_slide_at_the_start_and_only_the_new_one_at_the_end() {
        for horizontal in [true, false] {
            let start = comb(horizontal, 0.0, 640.0, 360.0);
            let end = comb(horizontal, 1.0, 640.0, 360.0);
            assert!(start.iter().any(|c| c.face == Face::Old));
            assert!(end.iter().all(|c| c.face == Face::New), "{horizontal}");
        }
    }
}

/// 扉 (`doors`) and 窓 (`window`) at `q` (0 to 1, linear): the old slide splits in two halves (above and below for the horizontal option, left and
/// right for the vertical one) that swing back from their outer edges, in perspective, and the new slide comes up from the middle of the opening,
/// growing until it fills the picture. `window` is the same scene on a grey ground with the halves pale (`window`), 扉 on black.
pub(crate) fn doors(window: bool, horizontal: bool, q: f32, w: f32, h: f32) -> Vec<Card> {
    // Fitted to the video of the horizontal option of each (18 pictures, the blurred squared difference 0.052 to 0.017 for the doors, 0.047 to 0.019
    // for the window; the vertical options with the same figures: 0.051 to 0.019 for the doors): the halves swing from 17.75 to 40 frames of the 60
    // through 95 degrees, the camera's focal length 1.675 times the width, the new slide grows from 0.45 of its size at frame 22 to 1 at frame 46,
    // fading in over frames 10.75 to 29.5, behind the halves at full strength and in front of them at 0.475 (the window: 0.175); the window's ground is
    // 0.48 white and its halves 0.6 opaque (the doors' ground is black, their halves opaque).
    let (a0, a1, amax, focal_w, b0, b1, s0, c0, c1) = (17.75, 40.0, 95.0_f32, 1.675, 22.0, 46.0, 0.45, 10.75, 29.5);
    let (ground, half_opacity, ghost) = if window { (0.48, 0.6, 0.175) } else { (0.0, 1.0, 0.475) };
    let k = q * 60.0;
    let frac = |a: f32, b: f32| ((k - a) / (b - a)).clamp(0.0, 1.0);
    let focal = focal_w * w;
    let quad = |x0: f32, y0: f32, x1: f32, y1: f32| {
        let at = |x: f32, y: f32| MeshVertex::new([x, y, 0.0], [x / w, y / h]);
        Mesh { vertices: Arc::new(vec![at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1)]), indices: vec![0, 1, 2, 1, 3, 2] }
    };
    let mut cards = Vec::new();
    // The ground.
    if ground > 0.0 {
        cards.push(Card { face: Face::Old, transform: Transform3D::identity(), cull_back: false, shade: 1.0, mesh: Some(quad(0.0, 0.0, w, h)), white: ground, opacity: 0.0 });
    }
    // The new slide, behind the halves, and a paler copy of it in front of them.
    let scale = s0 + (1.0 - s0) * frac(b0, b1);
    let new_card = |opacity: f32| Card {
        face: Face::New,
        transform: Transform3D::translate(-w / 2.0, -h / 2.0, 0.0).then(Transform3D::scale(scale, scale, 1.0)).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0)),
        cull_back: false,
        shade: 1.0,
        mesh: Some(quad(0.0, 0.0, w, h)),
        white: 0.0,
        opacity,
    };
    cards.push(new_card(1.0));
    // The halves.
    let angle = amax.to_radians() * frac(a0, a1);
    let halves: [([f32; 4], f32); 2] = if horizontal { [([0.0, 0.0, w, h / 2.0], 1.0), ([0.0, h / 2.0, w, h], -1.0)] } else { [([0.0, 0.0, w / 2.0, h], 1.0), ([w / 2.0, 0.0, w, h], -1.0)] };
    for (rect, side) in halves {
        let hinge = if horizontal { if side > 0.0 { -h / 2.0 } else { h / 2.0 } } else if side > 0.0 { -w / 2.0 } else { w / 2.0 };
        let (to_hinge, rot, from_hinge) = if horizontal {
            (Transform3D::translate(0.0, -hinge, 0.0), Transform3D::rotate_x(angle * side), Transform3D::translate(0.0, hinge, 0.0))
        } else {
            (Transform3D::translate(-hinge, 0.0, 0.0), Transform3D::rotate_y(-angle * side), Transform3D::translate(hinge, 0.0, 0.0))
        };
        cards.push(Card {
            face: Face::Old,
            transform: Transform3D::translate(-w / 2.0, -h / 2.0, 0.0).then(to_hinge).then(rot).then(from_hinge).then(Transform3D::perspective(focal)).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0)),
            cull_back: false,
            shade: 1.0,
            mesh: Some(quad(rect[0], rect[1], rect[2], rect[3])),
            white: 0.0,
            opacity: if angle > 0.0 { half_opacity } else { 1.0 },
        });
    }
    cards.push(new_card(ghost * frac(c0, c1)));
    cards
}

/// The boundaries of 細断's strips across a 1920 px slide, as PowerPoint for Windows draws them (its random is fixed: the same in every run and for strip in and out;
/// the video's gaps between neighbours of different depth at 3.12 to 3.18 s, `shred-strip-boundaries.txt` of tasks/doc/requests/2026-11-18-shred-dissolve-tsv; 564/578 and
/// 1048/1057 are one boundary each): 24 strips, 9 to 171 px wide. The camera recordings' 7-colour video found 25 boundaries (26 strips of 12 to 184 px: 13 of the earlier 23 are in it, the others hid in the grid picture): these are used.
const SHRED_BOUNDARIES: [f32; 25] = [56.0, 191.0, 203.0, 326.0, 370.0, 430.0, 461.0, 564.0, 596.0, 701.0, 730.0, 778.0, 864.0, 1048.0, 1130.0, 1228.0, 1263.0, 1311.0, 1397.0, 1498.0, 1529.0, 1579.0, 1663.0, 1796.0, 1860.0];
/// The heights (of the slide's) at which the horizontal cuts of 細断's pairs fell where they could be seen (3.26 s): 0.18 to 0.87, one to a pair of strips.
/// The columns' boundaries of 細断's particles across 1920 px (the same video's: 104 columns of 7 to 33 px, unequal like the strips; the rows are even: 103 of 10.5 px in 1080).
const SHRED_PARTICLE_BOUNDARIES: [f32; 103] = [1.0, 14.0, 44.0, 62.0, 74.0, 91.0, 104.0, 114.0, 135.0, 144.0, 164.0, 183.0, 195.0, 202.0, 225.0, 244.0, 255.0, 286.0, 300.0, 316.0, 330.0, 338.0, 346.0, 374.0, 406.0, 435.0, 465.0, 496.0, 511.0, 526.0, 542.0, 557.0, 586.0, 615.0, 634.0, 646.0, 656.0, 677.0, 705.0, 736.0, 765.0, 786.0, 795.0, 803.0, 824.0, 843.0, 855.0, 864.0, 877.0, 886.0, 902.0, 915.0, 928.0, 946.0, 975.0, 989.0, 1020.0, 1028.0, 1037.0, 1048.0, 1058.0, 1081.0, 1108.0, 1121.0, 1138.0, 1168.0, 1196.0, 1214.0, 1227.0, 1245.0, 1257.0, 1266.0, 1286.0, 1299.0, 1308.0, 1316.0, 1345.0, 1375.0, 1383.0, 1392.0, 1406.0, 1435.0, 1458.0, 1474.0, 1494.0, 1524.0, 1553.0, 1567.0, 1583.0, 1611.0, 1630.0, 1642.0, 1655.0, 1670.0, 1699.0, 1713.0, 1729.0, 1759.0, 1788.0, 1821.0, 1847.0, 1878.0, 1906.0];
const SHRED_PARTICLE_ROWS: usize = 103;
const SHRED_CUTS: [f32; 10] = [0.18, 0.37, 0.37, 0.54, 0.55, 0.59, 0.61, 0.70, 0.85, 0.87];
/// The camera, from PowerPoint for Windows' recordings (tasks/doc/requests/2026-11-18-shred-camera-result.md: a grid picture in 16:9 and 4:3, 1080p60 and 720p30, the whole
/// fitted frame by frame as one rigid body in a pinhole camera): the eye is 1.3 slide widths from the slide in 16:9 (1.0 to 1.3 fit best, 0.6 to 2.0 cannot be told apart; in 4:3
/// the angles agree with 16:9 when the distance is 1.73 widths, so it is a multiple of the slide's HEIGHT: 2.311), and the whole turns about one axis through the middle of the
/// slide, 40 degrees up to the right from the horizontal, the right going away and the top coming near (the top of PowerPoint's pieces is larger than the bottom, seen on its 1080p grid and 720p strip videos; the shares of the turn: 0.62 about the vertical axis, 0.78 about the horizontal). The
/// angle (degrees) by the seconds since the start: 0.6 at 0.1, 2.8 at 0.2, 5.7 at 0.3, 8.8 at 0.4, 11.7 at 0.5, 14.7 at 0.6, 22.6 at 0.8, then about 22 to 23 (3.5 s on is the video's
/// rough part: the pieces' own depths spoil the whole's fit). The new slide's pieces: the same turn goes down to 0 at the end: 16.2 at 0.7 s before it (4.3 s), 13.1 at 0.5, 10.4 at 0.3,
/// 4.3 at 0.2, 1.9 at 0.1, 0.5 at 0.05.
const SHRED_FOCAL_OVER_HEIGHT: f32 = 2.311;
const SHRED_AXIS: (f32, f32) = (0.62, 0.78);
const SHRED_TURN: [(f32, f32); 11] = [(0.0, 0.0), (0.1, 0.6), (0.2, 2.8), (0.3, 5.7), (0.4, 8.8), (0.5, 11.7), (0.6, 14.7), (0.8, 22.6), (1.0, 22.0), (1.2, 23.3), (2.0, 23.3)];
const SHRED_TURN_BACK: [(f32, f32); 8] = [(0.0, 0.0), (0.05, 0.5), (0.1, 1.9), (0.2, 4.3), (0.3, 10.4), (0.5, 13.1), (0.7, 16.2), (2.0, 16.2)];

fn shred_turn(table: &[(f32, f32)], seconds: f32) -> f32 {
    table.windows(2).find(|w| seconds <= w[1].0).map_or(table[table.len() - 1].1, |w| w[0].1 + (w[1].1 - w[0].1) * (seconds - w[0].0) / (w[1].0 - w[0].0)).to_radians()
}
/// The shares of the transition (2 s): the old slide's pieces sink for the first 0.6 (1.2 s), the new one's come back in the last 0.35 (0.7 s), black between.
const SHRED_SINK: f32 = 0.6;
const SHRED_RETURN: f32 = 0.35;

/// 細断 at `q` (0 to 1, linear): the old slide's pieces sink away, each its own depth as `z = 0.7 s tau^2.3` of the eye's distance (the median depth of PowerPoint's pieces at
/// 3.6, 4.0 and 4.08 s was 0.20, 0.50 and 0.61; the spread of the pieces' depths 20 to 24 to 1, `s` from 0.14 to 7.3), the whole turning about its vertical axis (the right going
/// back) until the pieces are a small bunch far away; then the new slide's pieces come back from there at the right, the depths
/// shrinking to 0 as the same law run backwards, until they are one flat slide. The pieces are strips of the lattice above, in pairs cut across at one height into two pieces
/// each: three strips make an L and a turned L (the stair steps sideways by a strip's width), or (`particle`) 104 columns of the video's unequal widths and 103 even rows of tiles, with the same law of depths as the strips (PowerPoint's rectangle pattern: its cloud's coverage on the 720p recording, 0.39 at 0.93 s, 0.38 here; with the earlier mean 0.12 it was 0.55).
pub(crate) fn shred(particle: bool, q: f32, w: f32, h: f32) -> Vec<Card> {
    let (sink, back) = (q < SHRED_SINK, q > 1.0 - SHRED_RETURN);
    if !sink && !back {
        return Vec::new();
    }
    let focal = SHRED_FOCAL_OVER_HEIGHT * h;
    let (face, tau, sign) = if sink { (Face::Old, q / SHRED_SINK, 1.0f32) } else { (Face::New, (1.0 - q) / SHRED_RETURN, -1.0f32) };
    let random = |i: usize, salt: u64| lattice_random(i as i32, salt as i32, 0x5eed + salt);
    let depth_of = |piece: usize| -> f32 {
        let u = random(piece, if sink { 1 } else { 2 });
        let spread = 3.97;
        let mean = 1.0;
        0.7 * mean * (spread * (u - 0.5)).exp() * tau.powf(2.3)
    };
    // The whole's turn about its axis through the middle: by the seconds since the start (the transition is 2 s) going down, or before the end coming back.
    let angle = if sink { shred_turn(&SHRED_TURN, q * 2.0) } else { shred_turn(&SHRED_TURN_BACK, (1.0 - q) * 2.0) };
    // rotate_x at a positive angle brings the bottom toward the viewer; PowerPoint's pieces tilt the other way (the top larger than the bottom, seen on its 1080p grid video), so the horizontal share is negative.
    let (yaw, pitch) = (angle * SHRED_AXIS.0, -angle * SHRED_AXIS.1);
    let _ = sign;
    // The pieces (rectangles in the slide's pixels, grouped by piece).
    let mut pieces: Vec<Vec<[f32; 4]>> = Vec::new();
    if particle {
        let mut edges = vec![0.0f32];
        edges.extend(SHRED_PARTICLE_BOUNDARIES.iter().map(|b| b / 1920.0 * w));
        edges.push(w);
        for r in 0..SHRED_PARTICLE_ROWS {
            for c in edges.windows(2) {
                pieces.push(vec![[c[0], r as f32 * h / SHRED_PARTICLE_ROWS as f32, c[1], (r + 1) as f32 * h / SHRED_PARTICLE_ROWS as f32]]);
            }
        }
    } else {
        let mut edges = vec![0.0f32];
        edges.extend(SHRED_BOUNDARIES.iter().map(|b| b / 1920.0 * w));
        edges.push(w);
        // Three strips make a pair of pieces, an L and a turned L (the video's gaps: a gap runs down, steps sideways by the width of a strip at the cut and goes on
        // down): the left piece is the first strip and the lower part of the second, the right one is the upper part of the second and the third strip.
        for (group, strips) in edges.windows(2).collect::<Vec<_>>().chunks(3).enumerate() {
            if strips.len() < 3 {
                for e in strips {
                    pieces.push(vec![[e[0], 0.0, e[1], h]]);
                }
                continue;
            }
            let cut = SHRED_CUTS[(group * 3 + 1) % SHRED_CUTS.len()] * h;
            let (a, b, c) = (strips[0], strips[1], strips[2]);
            pieces.push(vec![[a[0], 0.0, a[1], h], [b[0], cut, b[1], h]]);
            pieces.push(vec![[b[0], 0.0, b[1], cut], [c[0], 0.0, c[1], h]]);
        }
    }
    let mut order: Vec<(f32, usize)> = pieces.iter().enumerate().map(|(i, _)| (depth_of(i), i)).collect();
    // The far ones first (the cards are drawn in order).
    order.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut cards = Vec::new();
    for (z, i) in order {
        let transform = Transform3D::translate(-w / 2.0, -h / 2.0, -z * focal)
            .then(Transform3D::rotate_y(yaw))
            .then(Transform3D::rotate_x(pitch))
            .then(Transform3D::perspective(focal))
            .then(Transform3D::translate(w / 2.0, h / 2.0, 0.0));
        for &[x0, y0, x1, y1] in &pieces[i] {
            if x1 - x0 < 0.5 || y1 - y0 < 0.5 {
                continue;
            }
            let at = |x: f32, y: f32| MeshVertex::new([x, y, 0.0], [x / w, y / h]);
            let vertices = Arc::new(vec![at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1)]);
            cards.push(Card { face, transform, cull_back: false, shade: 1.0, mesh: Some(Mesh { vertices, indices: vec![0, 1, 2, 1, 3, 2] }), white: 0.0, opacity: 1.0 });
        }
    }
    cards
}

/// The depth (toward the viewer, in the eye's distance) of the axis 渦巻き's tiles swirl about, and the turn (radians) a tile has made when its life is over.
const VORTEX_AXIS: f32 = 0.3;
const VORTEX_SWIRL: f32 = 1.0;

/// The vortex's camera: the focal length over the slide's width it was made and shown to the user with (1.5625, the shred's before the shred took Windows'
/// measured camera; the user accepted the vortex's video with it, 2026-11-19).
const VORTEX_FOCAL: f32 = 1.5625;

/// 渦巻き at `q` (0 to 1, linear) in 3D: each tile of the old slide, once the front has passed it, is thrown toward the viewer (growing as it comes, flying outward from the
/// middle, turning), and the new slide's tiles come back the same way run backwards, over black. The tiles' own motion is fitted loosely: PowerPoint for Windows' 1080p60
/// video (white to four colours): the tiles in the picture are 162 px^2 (15 x 10.8) at rest, the middle one stays about that size (117 to 193 px^2 in the frames 3.2 to
/// 3.9 s), a quarter are 1.5 to 2 times, a tenth 5 times (area; 800 px^2) and the greatest about 40 times (6 000 to 8 500 px^2: 6.5 times the width, the eye's distance
/// over 1 - z with z about 0.85); the growth comes with the age after the front, to its end at 45 frames. The share of the tiles shown follows `VORTEX_REMAIN` as the flat one
/// did. Which depth and turn a tile has is a fixed shuffle (the distribution of the depths read off the areas; the turns, and the way the sizes grow with time, are guesses to be
/// fitted: the camera of the viewing is the honeycomb's).
pub(crate) fn vortex(motion: (f64, f64), q: f32, w: f32, h: f32) -> Vec<Card> {
    use ibis_doc_model::transition_plan::{VORTEX_COLS, VORTEX_LIFE, VORTEX_ROWS, vortex_pieces};
    let token = match motion {
        (x, _) if x > 0.5 => "r",
        (_, y) if y < -0.5 => "u",
        (_, y) if y > 0.5 => "d",
        _ => "l",
    };
    let focal = VORTEX_FOCAL * w;
    let (cw, ch) = (w / VORTEX_COLS as f32, h / VORTEX_ROWS as f32);
    let mut cards: Vec<(f32, Card)> = Vec::new();
    for tile in vortex_pieces(token, f64::from(q)) {
        let n = tile.row * VORTEX_COLS + tile.col;
        for (face, age, salt) in [(Face::Old, tile.old, 11u64), (Face::New, tile.new, 12)] {
            let Some(age) = age else { continue };
            let progress = (age as f32 / VORTEX_LIFE as f32).clamp(0.0, 1.0);
            // The share of the video's pixels a tile age shows already has the tiles' growth in it (they cover more as they come): so only the share of tiles that, grown, cover
            // that much are drawn (1 over the mean area of a tile of that age).
            let grown: f32 = (0..20).map(|k| { let u = (k as f32 + 0.5) / 20.0; let z = (0.1 + 0.8 * ((u - 0.5).max(0.0) * 2.0).powf(1.6)) * progress * progress; 1.0 / ((1.0 - z) * (1.0 - z)) }).sum::<f32>() / 20.0;
            if lattice_random(n as i32, salt as i32 + 3, 0x51ee + salt) >= 1.0 / grown {
                continue;
            }
            // The depth toward the viewer the tile reaches at the end of its life: about half the tiles stay (0.1), a quarter are thrown to 0.3 and the farthest to 0.9.
            let u = lattice_random(n as i32, salt as i32, 0x7027 + salt);
            let reach = 0.1 + 0.8 * ((u - 0.5).max(0.0) * 2.0).powf(1.6);
            let z = reach * progress * progress;
            let (a, b) = (lattice_random(n as i32, salt as i32 + 5, 0x7a11 + salt) - 0.5, lattice_random(n as i32, salt as i32 + 9, 0x7a22 + salt) - 0.5);
            let (x0, y0) = (tile.col as f32 * cw, tile.row as f32 * ch);
            let (cx, cy) = (x0 + cw / 2.0, y0 + ch / 2.0);
            let at = |x: f32, y: f32| MeshVertex::new([x, y, 0.0], [x / w, y / h]);
            let vertices = Arc::new(vec![at(x0, y0), at(x0 + cw, y0), at(x0, y0 + ch), at(x0 + cw, y0 + ch)]);
            // The whole swirls about the axis through the middle of the screen at the depth `VORTEX_AXIS` (the sweep's direction: about the vertical axis for the sides, the horizontal
            // one for the top and the bottom): the tiles nearer than the axis go toward the side the sweep starts at, the ones farther the other way (the user's reading of PowerPoint's
            // recording; the size of the turn is a guess to be fitted). `l` starts at the right: a positive turn takes the near points to the right.
            let swirl = VORTEX_SWIRL * progress;
            let axis = VORTEX_AXIS * focal;
            let whole = Transform3D::translate(0.0, 0.0, -axis)
                .then(match token {
                    "l" => Transform3D::rotate_y(swirl),
                    "r" => Transform3D::rotate_y(-swirl),
                    "u" => Transform3D::rotate_x(-swirl),
                    _ => Transform3D::rotate_x(swirl),
                })
                .then(Transform3D::translate(0.0, 0.0, axis));
            let transform = Transform3D::translate(-cx, -cy, 0.0)
                .then(Transform3D::rotate_x(a * 2.4 * progress))
                .then(Transform3D::rotate_y(b * 2.4 * progress))
                .then(Transform3D::translate(cx - w / 2.0, cy - h / 2.0, z * focal))
                .then(whole)
                .then(Transform3D::perspective(focal))
                .then(Transform3D::translate(w / 2.0, h / 2.0, 0.0));
            cards.push((z, Card { face, transform, cull_back: false, shade: 1.0, mesh: Some(Mesh { vertices, indices: vec![0, 1, 2, 1, 3, 2] }), white: 0.0, opacity: 1.0 }));
        }
    }
    // The far ones first.
    cards.sort_by(|a, b| a.0.total_cmp(&b.0));
    cards.into_iter().map(|(_, c)| c).collect()
}

#[cfg(test)]
mod shred_tests {
    use super::*;

    /// Where a card's four corners are drawn (the slide's pixels).
    fn corners(card: &Card) -> Vec<(f32, f32)> {
        let mesh = card.mesh.as_ref().expect("a mesh");
        mesh.vertices.iter().filter_map(|v| card.transform.project(v.position[0], v.position[1], v.position[2])).collect()
    }

    /// The area (px) of the rectangles a set of cards draws (their bounding boxes' areas: the corners are drawn as a quad of nearly the same shape).
    fn area(cards: &[Card]) -> f32 {
        cards.iter().map(|c| {
            let p = corners(c);
            let (x0, x1) = p.iter().fold((f32::MAX, f32::MIN), |a, q| (a.0.min(q.0), a.1.max(q.0)));
            let (y0, y1) = p.iter().fold((f32::MAX, f32::MIN), |a, q| (a.0.min(q.1), a.1.max(q.1)));
            (x1 - x0) * (y1 - y0)
        }).sum()
    }

    #[test]
    fn the_strips_are_24_in_12_pairs_each_cut_across_into_two_l_shaped_pieces() {
        let (w, h) = (1920.0, 1080.0);
        let cards = shred(false, 0.3, w, h);
        // 26 strips: 8 groups of 3, each a pair of pieces (an L and a turned L) of 2 rectangles, and the last 2 strips whole.
        assert_eq!(cards.len(), 8 * 2 * 2 + 2);
        assert!(cards.iter().all(|c| c.face == Face::Old));
        // At the start the pieces together are the whole slide, drawn where it is.
        let start = shred(false, 0.0, w, h);
        assert!((area(&start) - w * h).abs() < w * h * 0.001, "{}", area(&start));
        // Each pair of pieces: the left piece is a whole strip and the lower part of the next (an L), the right one the upper part of that strip and a whole one (a turned L):
        // the first rectangle of the left piece is as high as the slide, the second ends at the slide's bottom, the right piece's first ends where the left's second begins.
        for pair in start.chunks(4).take(8) {
            let (top, bottom): (Vec<f32>, Vec<f32>) = pair.iter().map(|c| { let p = corners(c); (p.iter().map(|q| q.1).fold(f32::MAX, f32::min), p.iter().map(|q| q.1).fold(f32::MIN, f32::max)) }).unzip();
            assert!(top[0].abs() < 0.5 && (bottom[0] - h).abs() < 0.5, "the left stem is the slide's height: {top:?} {bottom:?}");
            assert!((bottom[1] - h).abs() < 0.5 && top[1] > 1.0, "the foot ends at the bottom, under the cut: {top:?} {bottom:?}");
            assert!(top[2].abs() < 0.5 && (bottom[2] - top[1]).abs() < 0.5, "the upper arm ends where the foot begins: {top:?} {bottom:?}");
            assert!(top[3].abs() < 0.5 && (bottom[3] - h).abs() < 0.5, "the right stem is the slide's height: {top:?} {bottom:?}");
        }
    }

    #[test]
    fn the_squares_are_small_and_the_pieces_sink_at_different_depths() {
        let (w, h) = (1920.0, 1080.0);
        let squares = shred(true, 0.3, w, h);
        assert_eq!(squares.len(), 104 * 103);
        // Small: the tiles are about 1 % of the width.
        assert!(squares.iter().all(|c| { let p = corners(c); (p[1].0 - p[0].0).abs() < w * 0.03 }));
        let depth = |cards: &[Card]| -> Vec<f32> { cards.iter().map(|c| { let p = corners(c); (p[1].0 - p[0].0).abs() }).collect() };
        let widths = depth(&squares);
        let (lo, hi) = widths.iter().fold((f32::MAX, f32::MIN), |a, &x| (a.0.min(x), a.1.max(x)));
        assert!(hi > lo * 1.02, "the squares are all at one depth");
    }

    #[test]
    fn the_old_slide_sinks_for_the_first_0_6_the_new_one_returns_in_the_last_035_and_black_is_between() {
        let (w, h) = (1920.0, 1080.0);
        for particle in [false, true] {
            assert!(shred(particle, 0.59, w, h).iter().all(|c| c.face == Face::Old) && !shred(particle, 0.59, w, h).is_empty());
            assert!(shred(particle, 0.62, w, h).is_empty(), "black between");
            assert!(shred(particle, 0.66, w, h).iter().all(|c| c.face == Face::New) && !shred(particle, 0.66, w, h).is_empty());
            // The sinking old slide is smaller than the whole, the returning new one too, and at the end the new slide is one flat piece of the whole.
            // Sunk and turned, the pieces are no longer where they began (the sum of their boxes is not the slide's area).
            assert!((area(&shred(particle, 0.5, w, h)) - w * h).abs() > w * h * 0.005, "{}", area(&shred(particle, 0.5, w, h)) / (w * h));
            // The returning new slide's pieces are still smaller than they end (they come from far away).
            let width = |cards: &[Card]| -> f32 { cards.iter().map(|c| { let p = corners(c); (p[1].0 - p[0].0).abs() }).sum::<f32>() / cards.len() as f32 };
            assert!(width(&shred(particle, 0.7, w, h)) < width(&shred(particle, 1.0, w, h)));
            let end = shred(particle, 1.0, w, h);
            assert!(end.iter().all(|c| c.face == Face::New));
            assert!((area(&end) - w * h).abs() < w * h * 0.001, "{}", area(&end));
        }
    }
}
