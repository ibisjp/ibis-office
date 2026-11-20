// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! さざなみ (`ripple`): the slide is a surface lifted and sunk in Z by a ring wave that spreads from the middle (or a corner), lit
//! from one side so that the wave is shaded, and the new slide shows through the ring: inside it the new slide, outside the old one,
//! the two cross-faded over the ring's width. Drawn as a displaced mesh (`turn3d::Mesh`) of the old slide, the new slide's mesh over it
//! at the wave's opacity, both in the shade bands of the lit surface (the cards have one shade each).
//!
//! Measured on PowerPoint for Windows (Microsoft 365 16.0.20527; `trans3d2.pptx` and `target/t88/ripple/*` (request #M144), the slides of 4
//! colours (and, for the wave's height, a grid of lines 80 px apart), exported with `Presentation.CreateVideo`, 30 frames a second;
//! lengths below are px of a video 720 px high, `u = h / 720`):
//! * The ring is a circle in pixels, about the origin (the middle, or the corner the option's direction leaves: `lu` the bottom right, `ru` the
//!   bottom left, `ld` the top right, `rd` the top left): contour of the new slide's share 0.5 at a radius that grows linearly (residual 0.8 to
//!   3.6 px, 18 to 30 frames each). The radius at the cut (the picture is whole at the end of the transition) is 0.9 of the farthest corner's distance
//!   of a 1280 x 720 picture, whichever the slide's width: 658 px for the middle and 1335 for a corner (measured; 0.9 x 734 and 0.9 x 1469), and 646
//!   px for the middle of a 4:3 slide (960 x 720: the speed 11.84 px a frame, the same as 16:9's 11.83). The ring starts at the origin at `q0` of the
//!   transition: 4.4 frames of 60 at the middle, 6.4 at a corner; with `p14:dur` 1 s and 4 s the speed is in the inverse ratio of the time (23.4 and
//!   5.92 px a frame; the start is 1.0 and 11.8 frames: not proportional, taken as the 2 s one).
//! * The new slide's share across the ring does not change with time or radius, over the radius from the 50% contour: `PROFILE` (the middle, 2 s,
//!   16:9, 4:3 and 4 s: -80 px 0.98 to 0.99, -40 0.76 to 0.78, 0 0.50, +40 0.23 to 0.25 (0.18 in the first video), +80 0.00). For a corner the profile
//!   is wider in proportion to the speed (2.1 times).
//! * Shade: on the white of the slide a single dark band, 0.81 at its darkest at the 50% contour (frame 52), 0.86 (68), 0.92 (76), 0.97 (88): symmetric
//!   all around the ring, never brighter than the white. It is the surface's slope toward a light, one-sided (`light_at`: `min(1, (1 + t z') /
//!   sqrt(1 + z'^2))`, `t` = 1/3) for the slope `z' = 0.6 (1.02 - q)` at the steepest part (0.793, 0.885, 0.926, 0.962, 0.978 against 0.81, 0.86,
//!   0.92, 0.96, 0.97).
//! * The wave's height: the grid's lines move along the radius by `eps = z / (F - z)` of their distance from the middle (perspective); `eps` over the
//!   radius from the contour (medians, two rows, 38 frames): +0.7% at -80 px, +1.2 at -60, +1.7 at -40, +1.4 at -20, +0.9 at 0, 0 at +20, -1.0 at +40,
//!   -1.5 at +60, 0 from +80 (an abrupt end: the ring's front), and behind the crest a trough of -0.7 to -0.9% at -140 to -160. That is a sine wave of
//!   wavelength `LAMBDA` = 196 px, zero at +13, crest 48 px behind it, trough 49 ahead, cut off at the front between 62 and 80 px, and decaying behind the crest
//!   (0.55 after a half wavelength): `wave_at`. For a corner the whole profile is `s` = 2 times as long (and as high: the slope is the same).
//! * The camera's focal length `RIPPLE_FOCAL` comes from the crest's `eps` over its slope: the crest is 1.6% (median) for the steepest slope 0.25
//!   of the middle of the transition (12.9 px of height, 6.6% of eps for each unit of slope), 472 px.
//! * Still to be measured: the corner on a 4:3 slide (the farthest corner of the 16:9 picture is used for the speed, whatever the slide's shape).

use crate::turn3d::{pose_card, Card, Face, Mesh};
use gpui_kit::{MeshVertex, Transform3D};
use std::sync::Arc;

/// The farthest corner's distance of a 1280 x 720 picture from its middle (734.2) and from a corner (1469): the radius at the cut is
/// `CUT_REACH` of the farthest corner's distance of such a picture from the origin, and the lengths of a corner's wave are in proportion to it
/// (`scale`: 1 for the middle).
const FAR_MIDDLE: f32 = 734.2;
const CUT_REACH: f32 = 0.9;
/// The time the ring starts at the origin, over the transition: 4.4 frames of 60 at the middle, 6.4 at a corner (`Q_START_MIDDLE` + `Q_START_PER_SCALE`
/// per unit of the wave's scale over 1).
const Q_START_MIDDLE: f32 = 4.42 / 60.0;
const Q_START_PER_SCALE: f32 = 2.0 / 60.0;
/// The distance (px of a 720 px picture) of the new slide's share across the ring: (offset from the 50% contour, share). Linear between, 1 before the
/// first, 0 after the last.
const PROFILE: [(f32, f32); 9] = [(-80.0, 0.99), (-60.0, 0.92), (-40.0, 0.78), (-20.0, 0.64), (0.0, 0.50), (20.0, 0.35), (40.0, 0.21), (60.0, 0.06), (80.0, 0.0)];
/// The wave: a sine of wavelength `LAMBDA` that is zero (at the steepest slope) `WAVE_ZERO` ahead of the contour, the crest behind it and the trough
/// ahead; ahead of `WAVE_CUT` px it is cut off to nothing by `WAVE_CUT_END`, and behind the crest it decays by `WAVE_DECAY` px (e-fold).
const LAMBDA: f32 = 196.0;
const WAVE_ZERO: f32 = 13.0;
const WAVE_CUT: f32 = 62.0;
const WAVE_CUT_END: f32 = 80.0;
const WAVE_CREST: f32 = WAVE_ZERO - LAMBDA / 4.0;
const WAVE_DECAY: f32 = 164.0;
/// The slope at the steepest part: `RING_SLOPE_0 (RING_SLOPE_END - q)`.
const RING_SLOPE_0: f32 = 0.6;
const RING_SLOPE_END: f32 = 1.02;
/// The light: how much a slope falling away from the ring's middle darkens, `t` of `shade = min(1, (1 + t z') / sqrt(1 + z'^2))`.
const LIGHT_TILT: f32 = 1.0 / 3.0;
/// The camera's focal length over the slide's height (472 px of 720).
const RIPPLE_FOCAL: f32 = 472.0 / 720.0;
/// The mesh's cells across the slide, and the steps the shade and the share are cut into for drawing (each step is a card).
const CELLS_ACROSS: usize = 160;
const SHADE_STEP: f32 = 0.02;
const SHARE_STEP: f32 = 0.05;

/// The new slide's share (0 to 1) at `rel` px (of a 720 px picture) from the ring's 50% contour (positive is outside it).
pub(crate) fn share_at(rel: f32) -> f32 {
    if rel <= PROFILE[0].0 {
        return 1.0;
    }
    for p in PROFILE.windows(2) {
        if rel <= p[1].0 {
            return p[0].1 + (p[1].1 - p[0].1) * (rel - p[0].0) / (p[1].0 - p[0].0);
        }
    }
    0.0
}

/// The wave's height at `rel` px from the 50% contour, for the slope `steep` at its steepest part (the crest is above the plane, behind the contour,
/// the trough below it, ahead): a sine cut off at the front and decaying behind the crest.
fn height_at(rel: f32, steep: f32) -> f32 {
    let amplitude = steep * LAMBDA / std::f32::consts::TAU;
    let lead = if rel <= WAVE_CUT {
        1.0
    } else if rel >= WAVE_CUT_END {
        0.0
    } else {
        let u = (rel - WAVE_CUT) / (WAVE_CUT_END - WAVE_CUT);
        1.0 - u * u * (3.0 - 2.0 * u)
    };
    let behind = if rel < WAVE_CREST { ((rel - WAVE_CREST) / WAVE_DECAY).exp() } else { 1.0 };
    -amplitude * (std::f32::consts::TAU * (rel - WAVE_ZERO) / LAMBDA).sin() * lead * behind
}

/// The wave's height and its slope (the derivative over the radius) at `rel` px from the 50% contour.
pub(crate) fn wave_at(rel: f32, steep: f32) -> (f32, f32) {
    const H: f32 = 0.5;
    (height_at(rel, steep), (height_at(rel + H, steep) - height_at(rel - H, steep)) / (2.0 * H))
}

/// The brightness of the surface whose slope over the radius is `slope` (the light is one-sided: a surface falling away from the middle
/// is dark, the others as bright as the flat one).
pub(crate) fn light_at(slope: f32) -> f32 {
    ((1.0 + LIGHT_TILT * slope) / (1.0 + slope * slope).sqrt()).min(1.0)
}

/// The ring for an origin of the picture (`origin`: -1 to 1 each way) of a 1280 x 720 picture: the scale of its lengths (1 at the middle, 2 at a
/// corner) and the radius at the cut.
fn reach(origin: (f64, f64)) -> (f32, f32) {
    let (ox, oy) = ((origin.0 as f32 + 1.0) / 2.0 * 1280.0, (origin.1 as f32 + 1.0) / 2.0 * 720.0);
    let far = [(0.0, 0.0), (1280.0, 0.0), (0.0, 720.0), (1280.0, 720.0)].iter().map(|&(x, y): &(f32, f32)| ((x - ox).powi(2) + (y - oy).powi(2)).sqrt()).fold(0.0_f32, f32::max);
    (far / FAR_MIDDLE, CUT_REACH * far)
}

/// The ring's 50% contour (px of a 720 px picture) at time `q` (0 to 1): `None` before the ring starts.
fn ring_radius(q: f32, scale: f32, cut: f32) -> Option<f32> {
    let q0 = Q_START_MIDDLE + Q_START_PER_SCALE * (scale - 1.0);
    (q >= q0).then(|| cut * (q - q0) / (1.0 - q0))
}

fn quantize(v: f32, step: f32) -> i32 {
    (v / step).round() as i32
}

/// The cards of さざなみ at `q` (0 to 1 of the transition) on a slide of `w` x `h` logical pixels: the old slide's displaced mesh in its shade
/// bands, and the new slide's over it at the share's opacity. `origin` is (-1 to 1, -1 to 1): the middle (0, 0) or a corner.
pub(crate) fn ripple(origin: (f64, f64), q: f32, w: f32, h: f32) -> Vec<Card> {
    let rest = |face| pose_card(face, [0.0; 6], 1.0, w, h);
    if q >= 1.0 {
        return vec![rest(Face::New)];
    }
    // Everything is measured in px of a picture 720 px high; `unit` is a unit's size on this slide.
    let unit = h / 720.0;
    let width = w / unit;
    let (scale, cut) = reach(origin);
    let Some(radius) = ring_radius(q, scale, cut) else {
        return vec![rest(Face::Old)];
    };
    let (ox, oy) = ((origin.0 as f32 + 1.0) / 2.0 * width, (origin.1 as f32 + 1.0) / 2.0 * 720.0);
    let steep = (RING_SLOPE_0 * (RING_SLOPE_END - q)).max(0.0);
    let cols = CELLS_ACROSS;
    let rows = ((cols as f32 * h / w).round() as usize).max(2);
    let mut vertices = Vec::with_capacity((cols + 1) * (rows + 1));
    let mut share = Vec::with_capacity(vertices.capacity());
    let mut light = Vec::with_capacity(vertices.capacity());
    for j in 0..=rows {
        for i in 0..=cols {
            let (u, v) = (i as f32 / cols as f32, j as f32 / rows as f32);
            let (x, y) = (u * width, v * 720.0);
            let rel = (((x - ox).powi(2) + (y - oy).powi(2)).sqrt() - radius) / scale;
            // The lengths of a corner's wave are `scale` times those of the middle's, the slope the same.
            let (z, slope) = wave_at(rel, steep);
            vertices.push(MeshVertex::new([(u - 0.5) * w, (v - 0.5) * h, z * scale * unit], [u, v]));
            share.push(share_at(rel));
            light.push(light_at(slope));
        }
    }
    let vertices = Arc::new(vertices);
    let id = |i: usize, j: usize| (j * (cols + 1) + i) as u32;
    // Triangles grouped by their shade band (old) and their shade and share bands (new).
    let mut old_bands: std::collections::BTreeMap<i32, Vec<u32>> = std::collections::BTreeMap::new();
    let mut new_bands: std::collections::BTreeMap<(i32, i32), Vec<u32>> = std::collections::BTreeMap::new();
    for j in 0..rows {
        for i in 0..cols {
            let (tl, tr, bl, br) = (id(i, j), id(i + 1, j), id(i, j + 1), id(i + 1, j + 1));
            for tri in [[tl, tr, bl], [tr, br, bl]] {
                let mean = |a: &Vec<f32>| tri.iter().map(|&n| a[n as usize]).sum::<f32>() / 3.0;
                let (l, s) = (mean(&light), mean(&share));
                old_bands.entry(quantize(l, SHADE_STEP)).or_default().extend(tri);
                if s > 0.0 {
                    new_bands.entry((quantize(l, SHADE_STEP), quantize(s, SHARE_STEP))).or_default().extend(tri);
                }
            }
        }
    }
    let transform = Transform3D::perspective(RIPPLE_FOCAL * h).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0));
    let mut cards = Vec::new();
    for (face, bands) in [(Face::Old, old_bands.into_iter().map(|(l, t)| ((l, 20), t)).collect::<Vec<_>>()), (Face::New, new_bands.into_iter().collect())] {
        for ((l, s), indices) in bands {
            cards.push(Card {
                face,
                transform,
                cull_back: false,
                shade: (l as f32 * SHADE_STEP).min(1.0),
                mesh: Some(Mesh { vertices: vertices.clone(), indices }),
                white: 0.0,
                opacity: if face == Face::Old { 1.0 } else { (s as f32 * SHARE_STEP).min(1.0) },
            });
        }
    }
    cards
}

#[cfg(test)]
mod tests {
    use super::*;

    fn z_of(cards: &[Card]) -> Vec<f32> {
        cards.iter().filter_map(|c| c.mesh.as_ref()).flat_map(|m| m.vertices.iter().map(|v| v.position[2])).collect()
    }

    /// The waves lift and sink the surface in a ring, and leave the rest flat.
    #[test]
    fn the_surface_is_lifted_and_sunk_in_a_ring_at_the_middle_of_the_transition() {
        let (w, h) = (640.0, 360.0);
        let cards = ripple((0.0, 0.0), 0.5, w, h);
        let mesh = cards.iter().find_map(|c| c.mesh.as_ref()).expect("a mesh");
        let (mut up, mut down, mut flat) = (0, 0, 0);
        let mut ring = Vec::new();
        for v in mesh.vertices.iter() {
            let r = (v.position[0].powi(2) + v.position[1].powi(2)).sqrt();
            if v.position[2] > 0.5 {
                up += 1;
                ring.push(r);
            } else if v.position[2] < -0.5 {
                down += 1;
                ring.push(r);
            } else {
                flat += 1;
            }
        }
        assert!(up > 50 && down > 50 && flat > 1000, "{up} {down} {flat}");
        // Nothing is displaced ahead of the front (the contour at 152 px of this 360 px slide, the wave cut off 40 px of 720 ahead of it).
        let hi = ring.iter().fold(0.0_f32, |b, &r| b.max(r));
        assert!(hi < 152.0 + 40.0 + 1.0 && hi > 152.0, "{hi}");
        // The ring is a circle: the points at the same radius have the same height whichever way they lie.
        let at = |x: f32, y: f32| {
            let m = mesh.vertices.iter().min_by(|a, b| ((a.position[0] - x).powi(2) + (a.position[1] - y).powi(2)).total_cmp(&((b.position[0] - x).powi(2) + (b.position[1] - y).powi(2)))).unwrap();
            m.position[2]
        };
        let r = 100.0_f32;
        let (a, b) = (at(r, 0.0), at(0.0, r));
        assert!((a - b).abs() < 0.5, "{a} {b}");
    }

    /// The light makes the ring's slope dark and leaves the flat parts white; the new slide's cards fade across the ring.
    #[test]
    fn the_shading_varies_across_the_ring_and_the_new_slide_fades_in() {
        let cards = ripple((0.0, 0.0), 0.4, 640.0, 360.0);
        let shades: Vec<f32> = cards.iter().filter(|c| c.face == Face::Old).map(|c| c.shade).collect();
        let (lo, hi) = shades.iter().fold((2.0_f32, 0.0_f32), |(a, b), &s| (a.min(s), b.max(s)));
        assert!(hi > 0.99 && lo < 0.9 && lo > 0.6, "{lo} {hi}");
        let opacities: Vec<f32> = cards.iter().filter(|c| c.face == Face::New).map(|c| c.opacity).collect();
        assert!(opacities.iter().any(|&o| o > 0.95) && opacities.iter().any(|&o| (0.2..0.8).contains(&o)), "{opacities:?}");
        // A lit wave falls off with time: the later the lighter.
        let darkest = |q: f32| ripple((0.0, 0.0), q, 640.0, 360.0).iter().filter(|c| c.face == Face::Old).map(|c| c.shade).fold(2.0_f32, f32::min);
        assert!(darkest(0.35) < darkest(0.7) && darkest(0.7) < darkest(0.95), "{} {} {}", darkest(0.35), darkest(0.7), darkest(0.95));
    }

    /// The curve and the light are those measured: a half and a flat surface.
    #[test]
    fn the_profile_and_the_light_are_as_measured() {
        assert!((share_at(0.0) - 0.5).abs() < 1e-6 && share_at(-200.0) == 1.0 && share_at(200.0) == 0.0);
        assert!((share_at(10.0) - 0.425).abs() < 1e-4);
        // The crest is behind the contour (above the plane), the trough ahead of it (below); nothing ahead of the cut.
        assert!(wave_at(WAVE_CREST, 0.5).0 > 0.0 && wave_at(WAVE_ZERO + LAMBDA / 4.0, 0.5).0 < 0.0 && wave_at(WAVE_CUT_END + 1.0, 0.5).0 == 0.0);
        // It dies away behind the crest: the trailing trough is lower than the leading one.
        assert!(wave_at(WAVE_CREST - LAMBDA / 2.0, 0.5).0.abs() < 0.7 * wave_at(WAVE_ZERO + LAMBDA / 4.0, 0.5).0.abs());
        assert!((light_at(0.0) - 1.0).abs() < 1e-6 && light_at(-0.42) < 0.82 && light_at(0.5) == 1.0);
        // The steepest slope falls away from the middle (negative) at the contour's zero.
        assert!((wave_at(WAVE_ZERO, 0.4).1 + 0.4).abs() < 0.01);
        // Measured at the steepest part, 0.81 at frame 52 (q = 0.317), 0.92 at 76 (q = 0.717).
        assert!((light_at(-RING_SLOPE_0 * (RING_SLOPE_END - 0.317)) - 0.81).abs() < 0.03);
        assert!((light_at(-RING_SLOPE_0 * (RING_SLOPE_END - 0.717)) - 0.92).abs() < 0.03);
        // The middle's ring is 11.83 px a frame of the 60: at frame 17 of the transition 149 px; at the end 0.9 of the farthest corner.
        let (scale, cut) = reach((0.0, 0.0));
        assert!((scale - 1.0).abs() < 1e-3);
        let r = ring_radius(17.0 / 60.0, scale, cut).unwrap();
        assert!((r - 149.0).abs() < 3.0, "{r}");
        assert!((cut - 660.8).abs() < 1.0);
        // A corner: twice the middle's scale, the radius at the cut 1322 (measured 1335), the speed 24.9 px a frame measured against 24.5.
        let (cs, ccut) = reach((-1.0, -1.0));
        assert!((cs - 2.0).abs() < 0.01 && (ccut - 1322.0).abs() < 3.0, "{cs} {ccut}");
        let speed = (ring_radius(50.0 / 60.0, cs, ccut).unwrap() - ring_radius(30.0 / 60.0, cs, ccut).unwrap()) / 20.0;
        assert!((speed - 24.9).abs() < 1.0, "{speed}");
    }

    /// Nothing is drawn but the old slide before the ring starts, and but the new one at the end.
    #[test]
    fn it_starts_and_ends_with_the_whole_slides() {
        let start = ripple((0.0, 0.0), 0.0, 640.0, 360.0);
        assert!(start.len() == 1 && start[0].face == Face::Old);
        let end = ripple((0.0, 0.0), 1.0, 640.0, 360.0);
        assert!(end.len() == 1 && end[0].face == Face::New);
    }

    /// A corner origin puts the middle of the ring at that corner: the waves are on the slide's far side after a while.
    #[test]
    fn a_corner_origin_spreads_from_the_corner() {
        let cards = ripple((-1.0, -1.0), 0.4, 640.0, 360.0);
        let mesh = cards.iter().find_map(|c| c.mesh.as_ref()).unwrap();
        let mean_x = mesh.vertices.iter().filter(|v| v.position[2].abs() > 0.5).map(|v| v.position[0]).sum::<f32>() / mesh.vertices.iter().filter(|v| v.position[2].abs() > 0.5).count().max(1) as f32;
        assert!(mean_x < 0.0, "{mean_x}");
    }
}
