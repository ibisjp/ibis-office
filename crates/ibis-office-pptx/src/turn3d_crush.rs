// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! クラッシュ (`crush`, `p15:prstTrans prst="crush"`): the old slide is a sheet of paper, a grid of points (a polygon mesh with the slide as its
//! texture) that is crumpled into a ball from the four corners, and the ball then falls out of the picture's bottom; the new slide is behind
//! it all the time (ibisOffice #88; the order of the frames is that of PowerPoint for Windows 16.0.20527's video of the 2 s transition,
//! `target/mac-com/anim-audit/audit-trans-flat-16x9-dur2000-720p-30fps.mp4`, the 106th slide).
//!
//! How it moves (the time `q` is 0 to 1 of the transition, linear; lengths are shares of the slide's height `h`):
//! * Every point of the grid goes from its place on the flat sheet to a place on a rough ball. The ball's place for the point `(u, v)` (0 to 1
//!   across and down the sheet) is the point of a sphere of longitude `(u - 1/2) LON_SPAN` and latitude `(v - 1/2) LAT_SPAN` (the middle of the
//!   sheet faces the viewer, the sides wrap round the back), at the distance `BALL_RADIUS (1 + ROUGHNESS n)` from the ball's centre, `n` a
//!   smooth noise of the point (the paper's creases are not round).
//! * The corners go first: a point starts crumpling at `s = min over the four corners k (delay_k + SPREAD d_k)`, `d_k` its distance from the
//!   corner over the diagonal (0 to 1) and `delay_k` a random time of that corner (`CORNER_DELAY`, fixed seeds), so the middle of the sheet is
//!   the last to go and stays flat (as the picture's white disc does in the video). Each point takes `CRUMPLE_TIME` of the transition to get there,
//!   by a smoothstep.
//! * Why these times (the video's share of the frame the sheet covers, by `|frame - the frame after the transition| > 60`, every 3rd frame, q = frame
//!   time over 60 frames): 0.82 at the start, 0.74 at q = 0.09, 0.60 at 0.19, 0.44 at 0.29, 0.28 at 0.39, 0.16 at 0.49, 0.14 from 0.54. Starts spread
//!   over `SPREAD` of the corner distance and a long `CRUMPLE_TIME` give this curve (0.79, 0.72, 0.61, 0.35, 0.21, 0.14 at the same moments): the
//!   area falls slowly while only the corners have gone, then at a steady rate, and ends gently. The fall's start (`FALL_AT`) and speed are
//!   from the same frames' ball (its centre's height 0.55, 0.59, 0.65, 0.73, 0.80, 0.86, 0.92, 0.97 at q = 0.59 to 0.94 by 0.05: ours 0.01 q behind).
//! * While a point is on its way it is also thrown about by a noise of `WRINKLE` of the height, the most at the middle of its way (`sin(pi c)`):
//!   that is the paper being squeezed, with the sheet's edge drawn in ahead of its middle.
//! * From `FALL_AT` the ball falls: its speed grows in proportion to the time (a fall by gravity) for `FALL_RAMP`, then stays `FALL_SPEED` (heights per
//!   whole transition), so the fall is `FALL_SPEED tau^2 / (2 FALL_RAMP)` for `tau < FALL_RAMP` and `FALL_SPEED (tau - FALL_RAMP / 2)` after it (`tau`
//!   the time since `FALL_AT`).
//! * Measured on the video (the share of each frame that differs from the picture after the transition; 20 frames): the sheet's share shrinks
//!   steadily from 0.82 at the start to 0.14 at q = 0.56 (so the crumpling spans the first 0.56 of the time, it is not done early), the ball is then
//!   0.34 of the slide's width and 0.5 of its height across, and stays that size; its top falls by 0.02, 0.05, 0.12, 0.20, 0.31, 0.42, 0.54, 0.66 of the
//!   height at q = 0.61 to 0.97 in steps of 0.05, the steps growing 0.03 to 0.12 and then staying (the ball leaves the picture at q = 0.97).
//! * The light is from the upper left in front: a triangle's shade is `SHADE_BASE + SHADE_RANGE |n . L|` (`n` its normal, `L` the light), at most 1,
//!   with the small random change of `FACET_SHADE` of each triangle, which is what shows the creases. The triangles are drawn in bands of shade.

use crate::turn3d::{pose_card, Card, Face, Mesh};
use gpui_kit::{MeshVertex, Transform3D};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The grid: cells across the slide (down as many as the slide's shape gives).
const CELLS_ACROSS: usize = 64;
/// The sphere the sheet is wrapped on: the span of longitude and latitude (radians) the sheet's width and height cover.
const LON_SPAN: f32 = 1.7 * std::f32::consts::PI;
const LAT_SPAN: f32 = std::f32::consts::PI;
/// The ball's radii over the slide's height (across and up and down: the ball is flatter than a sphere) and how rough it is (the share its
/// radius changes by).
const BALL_RADIUS: (f32, f32) = (0.30, 0.23);
const ROUGHNESS: f32 = 0.15;
/// The creases: a finer noise in the ball's radius (the share it changes by), and the random shade of each triangle (`FACET_SHADE` below).
const CREASES: f32 = 0.07;
/// The ball's centre, over the slide's width and height from its middle.
const BALL_CENTER: (f32, f32) = (-0.01, 0.04);
/// The corners' start: `s = min_k(delay_k + SPREAD d_k)`; each point then crumples in `CRUMPLE_TIME` (shares of the transition).
const SPREAD: f32 = 0.30;
const CRUMPLE_TIME: f32 = 0.45;
/// The pace is a smoothstep to this power (above 1: slower in the middle of a point's way, where the sheet's area falls fastest).
const CRUMPLE_EASE: f32 = 1.0;
const CORNER_DELAY: [f32; 4] = [0.020, 0.004, 0.030, 0.012];
/// How far a point is thrown about on its way, over the slide's height.
const WRINKLE: f32 = 0.07;
/// The fall: when it starts, how long the speed grows for and the speed after that (heights per whole transition).
const FALL_AT: f32 = 0.55;
const FALL_RAMP: f32 = 0.22;
const FALL_SPEED: f32 = 2.2;
/// The camera's focal length over the slide's width.
const FOCAL: f32 = 1.2;
const LIGHT: [f32; 3] = [-0.35, -0.45, 0.82];
const SHADE_BASE: f32 = 0.40;
const SHADE_RANGE: f32 = 0.75;
const FACET_SHADE: f32 = 0.14;
const SHADE_STEP: f32 = 0.04;

/// A number of 0 to 1 from a lattice point and a seed (an integer hash, so the same each time).
fn hash01(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B) ^ seed.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

/// A smooth noise of -1 to 1 (bilinear between lattice values, smoothed), two octaves.
fn noise(x: f32, y: f32, seed: u32) -> f32 {
    let octave = |x: f32, y: f32, seed: u32| {
        let (ix, iy) = (x.floor(), y.floor());
        let (fx, fy) = (x - ix, y - iy);
        let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
        let at = |dx: i32, dy: i32| hash01(ix as i32 + dx, iy as i32 + dy, seed);
        let top = at(0, 0) + (at(1, 0) - at(0, 0)) * sx;
        let bottom = at(0, 1) + (at(1, 1) - at(0, 1)) * sx;
        (top + (bottom - top) * sy) * 2.0 - 1.0
    };
    (octave(x, y, seed) * 2.0 + octave(x * 2.3 + 7.1, y * 2.3 + 3.7, seed ^ 0x55) * 1.0) / 3.0
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The cards of クラッシュ at `q` (0 to 1) on a slide of `w` x `h` logical pixels: the new slide whole, and the old slide's mesh over it in
/// its shade bands.
pub(crate) fn crush(q: f32, w: f32, h: f32) -> Vec<Card> {
    let mut cards = vec![pose_card(Face::New, [0.0; 6], 1.0, w, h)];
    if q >= 1.0 {
        return cards;
    }
    if q <= 0.0 {
        cards.push(pose_card(Face::Old, [0.0; 6], 1.0, w, h));
        return cards;
    }
    let cols = CELLS_ACROSS;
    let rows = ((cols as f32 * h / w).round() as usize).max(2);
    let diagonal = (w * w + h * h).sqrt();
    let tau = (q - FALL_AT).max(0.0);
    let fallen = if tau < FALL_RAMP { FALL_SPEED * tau * tau / (2.0 * FALL_RAMP) } else { FALL_SPEED * (tau - FALL_RAMP / 2.0) };
    let (cx, cy) = (BALL_CENTER.0 * w, (BALL_CENTER.1 + fallen) * h);
    let corners = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)];
    let mut vertices = Vec::with_capacity((cols + 1) * (rows + 1));
    for j in 0..=rows {
        for i in 0..=cols {
            let (u, v) = (i as f32 / cols as f32, j as f32 / rows as f32);
            let (fx, fy) = ((u - 0.5) * w, (v - 0.5) * h);
            let start = corners
                .iter()
                .zip(CORNER_DELAY)
                .map(|(&(kx, ky), delay)| delay + SPREAD * ((fx + w / 2.0 - kx).hypot(fy + h / 2.0 - ky) / diagonal))
                .fold(f32::MAX, f32::min);
            let c = smoothstep((q - start) / CRUMPLE_TIME).powf(CRUMPLE_EASE);
            let (lon, lat) = ((u - 0.5) * LON_SPAN, (v - 0.5) * LAT_SPAN);
            let rough = 1.0 + ROUGHNESS * noise(u * 9.0, v * 6.0, 1) + CREASES * noise(u * 26.0, v * 15.0, 5);
            let (rx, ry) = (BALL_RADIUS.0 * h * rough, BALL_RADIUS.1 * h * rough);
            let ball = [rx * lon.sin() * lat.cos() + cx, ry * lat.sin() + cy, rx * lon.cos() * lat.cos()];
            let thrown = WRINKLE * h * (std::f32::consts::PI * c).sin();
            let p = [
                fx + (ball[0] - fx) * c + thrown * noise(u * 7.0 + 11.0, v * 5.0, 2),
                fy + (ball[1] - fy) * c + thrown * noise(u * 7.0, v * 5.0 + 13.0, 3),
                ball[2] * c + thrown * 1.5 * noise(u * 8.0 + 3.0, v * 6.0 + 5.0, 4),
            ];
            vertices.push(MeshVertex::new(p, [u, v]));
        }
    }
    let id = |i: usize, j: usize| (j * (cols + 1) + i) as u32;
    let mut bands: BTreeMap<i32, Vec<u32>> = BTreeMap::new();
    let light_length = (LIGHT[0] * LIGHT[0] + LIGHT[1] * LIGHT[1] + LIGHT[2] * LIGHT[2]).sqrt();
    for j in 0..rows {
        for i in 0..cols {
            let (tl, tr, bl, br) = (id(i, j), id(i + 1, j), id(i, j + 1), id(i + 1, j + 1));
            for (n, tri) in [[tl, tr, bl], [tr, br, bl]].into_iter().enumerate() {
                let p = |k: usize| vertices[tri[k] as usize].position;
                let (a, b, d) = (p(0), p(1), p(2));
                let (e1, e2) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [d[0] - a[0], d[1] - a[1], d[2] - a[2]]);
                let normal = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt().max(1e-6);
                let facing = ((normal[0] * LIGHT[0] + normal[1] * LIGHT[1] + normal[2] * LIGHT[2]) / (length * light_length)).abs();
                let facet = (hash01((j * cols + i) as i32, n as i32, 9) * 2.0 - 1.0) * FACET_SHADE * smoothstep(q * 6.0);
                let shade = (SHADE_BASE + SHADE_RANGE * facing + facet).clamp(0.0, 1.0);
                bands.entry((shade / SHADE_STEP).round() as i32).or_default().extend(tri);
            }
        }
    }
    let vertices = Arc::new(vertices);
    let transform = Transform3D::perspective(FOCAL * w).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0));
    for (band, indices) in bands {
        cards.push(Card {
            face: Face::Old,
            transform,
            cull_back: false,
            shade: (band as f32 * SHADE_STEP).min(1.0),
            mesh: Some(Mesh { vertices: vertices.clone(), indices }),
            white: 0.0,
            opacity: 1.0,
        });
    }
    cards
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points(cards: &[Card]) -> Vec<[f32; 3]> {
        cards.iter().filter_map(|c| c.mesh.as_ref()).flat_map(|m| m.vertices.iter().map(|v| v.position)).collect()
    }

    #[test]
    fn it_starts_with_the_flat_old_slide_and_ends_with_the_new_one_alone() {
        assert!(crush(0.0, 640.0, 360.0).iter().all(|c| c.mesh.is_none() && c.face != Face::New || c.face == Face::New));
        assert_eq!(crush(0.0, 640.0, 360.0).len(), 2);
        let end = crush(1.0, 640.0, 360.0);
        assert_eq!((end.len(), end[0].face), (1, Face::New));
    }

    #[test]
    fn the_sheet_is_crumpled_into_a_ball_held_and_dropped() {
        let (w, h) = (640.0, 360.0);
        let extent = |q: f32| {
            let p = points(&crush(q, w, h));
            let (x0, x1) = p.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[0]), a.1.max(p[0])));
            let (y0, y1) = p.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p[1]), a.1.max(p[1])));
            (x1 - x0, y1 - y0, (y0 + y1) / 2.0)
        };
        // Early: still about the whole sheet; held: a ball a little over the height across, near the middle; later: it is falling.
        let (early_w, _, _) = extent(0.02);
        assert!(early_w > 0.9 * w, "{early_w}");
        let (held_w, held_h, held_y) = extent(0.56);
        assert!(held_w < 0.5 * w && held_h < 0.9 * h, "{held_w} {held_h}");
        assert!(held_y.abs() < 0.1 * h, "{held_y}");
        let (_, _, falling_y) = extent(0.9);
        assert!(falling_y > 0.3 * h, "{falling_y}");
        // The ball is gone from the picture (its top below the slide's bottom) by the end of the fall.
        let p = points(&crush(0.995, w, h));
        assert!(p.iter().all(|p| p[1] > h / 2.0 * 0.9), "the ball is out at the bottom");
    }

    #[test]
    fn the_corners_go_first_and_the_middle_stays_flat_longest() {
        let (w, h) = (640.0, 360.0);
        let p = points(&crush(0.1, w, h));
        let cols = CELLS_ACROSS + 1;
        let rows = ((CELLS_ACROSS as f32 * h / w).round() as usize).max(2) + 1;
        let at = |i: usize, j: usize| p[j * cols + i];
        let moved = |i: usize, j: usize| {
            let (u, v) = (i as f32 / (cols - 1) as f32, j as f32 / (rows - 1) as f32);
            ((at(i, j)[0] - (u - 0.5) * w).hypot(at(i, j)[1] - (v - 0.5) * h)).hypot(at(i, j)[2])
        };
        assert!(moved(1, 1) > moved(cols / 2, rows / 2) + 5.0, "{} {}", moved(1, 1), moved(cols / 2, rows / 2));
    }

    #[test]
    fn the_shades_are_bands_and_the_creases_darken() {
        let shades: Vec<f32> = crush(0.3, 640.0, 360.0).iter().filter(|c| c.face == Face::Old && c.mesh.is_some()).map(|c| c.shade).collect();
        let (lo, hi) = shades.iter().fold((2.0_f32, 0.0_f32), |(a, b), &s| (a.min(s), b.max(s)));
        assert!(hi > 0.9 && lo < 0.8, "{lo} {hi}");
    }
}
