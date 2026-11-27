// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! フラクチャ (`fracture`, `p15:prstTrans prst="fracture"`): the old slide is a pane that is struck near its middle and cracks into wedges
//! (ibisOffice #88; measured on PowerPoint for Windows 16.0.20527's video of the 2 s transition, `audit-trans-flat-16x9-dur2000-720p-30fps.mp4`,
//! the 105th slide, the frames counted from `k` = 0, the first frame of the motion, `q` * 60).
//!
//! What is read off the video (every frame; the share of the picture that differs from the picture after the transition, and where its middle is):
//! * The new slide is whole behind the old one all the time (its white disc shows through the hole of the old one).
//! * The old slide is cut by cracks that all start at one point near the slide's middle (about 7 of 360 above it): its pieces are wedges. The wedges
//!   on the top half are few and wide, the ones on the lower half thin; the core around the hit is blown out first (a hole growing to about 0.19 of
//!   the height in 12 frames). No ring of cracks round the hit is seen.
//! * The pieces fall: the lower ones first, the upper ones last; a piece (the one with the title bar, followed by eye) moves 7 to 8 of 360 a
//!   frame from about k = 14 on, without speeding up. The middle of what remains falls 6 a frame from k = 24 to 40.
//! * The share of the picture the old slide covers: 0.98 at k = 0, 0.90 at 8, 0.71 at 16, 0.54 at 24, 0.37 at 32, 0.13 at 40, nothing at 48.
//! * The number, widths and order of the wedges are random in PowerPoint; here they are a hash of the wedge's number.
//!
//! The constants below are fitted by eye to the share and middle above and to the frames side by side (not measured one by one).

use crate::turn3d::{pose_card, Card, Face, Mesh};
use gpui_kit::{MeshVertex, Transform3D};
use std::sync::Arc;

const WEDGES: usize = 16;
/// The hit, from the slide's middle over its width and height.
const HIT: (f32, f32) = (0.0, -0.02);
/// The frames over which the cracks open, and the turn about a wedge's radial line (radians): `OPEN_TURN` for the narrowest wedge, the least for the widest.
const OPEN_FRAMES: f32 = 16.0;
const OPEN_TURN: f32 = 1.1;
/// A piece's fall starts at `FALL_FROM` frames (earlier for the pieces pointing down, by `FALL_ORDER`, and by a random `FALL_SPREAD`).
const FALL_FROM: f32 = 7.0;
const FALL_ORDER: f32 = 6.0;
/// (The random spread of the release times was 5 frames with the earlier wedges; with the forked pieces the audit's error was 0.0392 at 9, 0.0335 at 5 and
/// 0.0255 at 1 frame, the pieces being released in the order of their height alone.)
const FALL_SPREAD: f32 = 1.0;
/// The fall is a gravity `FALL_G` (heights per frame squared) with air drag `FALL_DRAG` (per frame): the steady speed is `g / drag` = 11 of 360 a
/// frame as measured, reached in about `1 / drag` = 3 frames.
const FALL_DRAG: f32 = 1.0 / 3.0;
const FALL_G: f32 = FALL_DRAG * 11.0 / 360.0;
/// The share of the wedges' cracks that fork, where along the crack the fork starts (over its length) and how far it turns into the wedge (over the
/// wedge's angle).
const BRANCH_SHARE: f32 = 0.7;
const BRANCH_AT: (f32, f32) = (0.2, 0.6);
const BRANCH_TURN: (f32, f32) = (0.25, 0.6);
/// The core of the pane around the hit is blown out: the hole's radius over the slide's height and the frames it takes to open.
const HOLE: f32 = 0.19;
const HOLE_FRAMES: f32 = 12.0;
/// The camera's focal length over the slide's width.
const FOCAL: f32 = 1.25;

fn hash01(a: i32, seed: u32) -> f32 {
    let mut h = (a as u32).wrapping_mul(0x9E37_79B1) ^ seed.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Where the ray from `p` at `angle` leaves the rectangle of half sizes `(hw, hh)` about the origin.
fn leave(p: (f32, f32), angle: f32, hw: f32, hh: f32) -> (f32, f32) {
    let (dx, dy) = (angle.cos(), angle.sin());
    let tx = if dx > 1e-6 { (hw - p.0) / dx } else if dx < -1e-6 { (-hw - p.0) / dx } else { f32::MAX };
    let ty = if dy > 1e-6 { (hh - p.1) / dy } else if dy < -1e-6 { (-hh - p.1) / dy } else { f32::MAX };
    let t = tx.min(ty);
    (p.0 + dx * t, p.1 + dy * t)
}

/// The part of the convex polygon `poly` on one side of the line through `b` along `d` (`keep_left`: where `d x (p - b)` >= 0).
fn clip(poly: &[(f32, f32)], b: (f32, f32), d: (f32, f32), keep_left: bool) -> Vec<(f32, f32)> {
    let side = |p: (f32, f32)| (d.0 * (p.1 - b.1) - d.1 * (p.0 - b.0)) * if keep_left { 1.0 } else { -1.0 };
    let mut out = Vec::new();
    for i in 0..poly.len() {
        let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
        let (sp, sq) = (side(p), side(q));
        if sp >= 0.0 {
            out.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            let t = sp / (sp - sq);
            out.push((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t));
        }
    }
    out
}

/// How far a piece has fallen `tau` frames after it is let go (fractions of the slide's height): a body under gravity `FALL_G` (heights per frame
/// squared) with air drag `FALL_DRAG` (per frame), `v' = g - drag v`: it speeds up over about `1 / drag` frames to the steady speed `g / drag`, as the
/// pieces in PowerPoint's video do (the same law as the earlier ramp and steady speed, written as the force that makes it).
fn fallen(tau: f32) -> f32 {
    let terminal = FALL_G / FALL_DRAG;
    terminal * (tau - (1.0 - (-FALL_DRAG * tau).exp()) / FALL_DRAG)
}

pub(crate) fn fracture(q: f32, w: f32, h: f32) -> Vec<Card> {
    let mut cards = vec![pose_card(Face::New, [0.0; 6], 1.0, w, h)];
    if q >= 1.0 {
        return cards;
    }
    if q <= 0.0 {
        cards.push(pose_card(Face::Old, [0.0; 6], 1.0, w, h));
        return cards;
    }
    let k = q * 60.0;
    let (hw, hh) = (w / 2.0, h / 2.0);
    let hit = (HIT.0 * w, HIT.1 * h);
    // The cracks' angles: random widths, the upper ones wide (the shift of each is a hash of its number, so every run is the same).
    let raw: Vec<f32> = (0..WEDGES).map(|i| 0.2 + hash01(i as i32, 1).powi(2)).collect();
    let total: f32 = raw.iter().sum();
    let mut edges = vec![-std::f32::consts::FRAC_PI_2];
    for r in &raw {
        edges.push(edges.last().copied().unwrap_or(0.0) + r / total * std::f32::consts::TAU);
    }
    let widest = raw.iter().cloned().fold(0.0_f32, f32::max);
    let corners = [(hw, hh), (-hw, hh), (-hw, -hh), (hw, -hh)];
    let view = |t: Transform3D| t.then(Transform3D::perspective(FOCAL * w)).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0));
    let hole = HOLE * h * (k / HOLE_FRAMES).clamp(0.0, 1.0);
    let ang_from = |p: (f32, f32), a0: f32| {
        let mut d = (p.1 - hit.1).atan2(p.0 - hit.0) - a0;
        while d < -1e-4 {
            d += std::f32::consts::TAU;
        }
        d
    };
    for i in 0..WEDGES {
        let (a0, a1) = (edges[i], edges[i + 1]);
        let mut inside: Vec<(f32, f32)> = corners.iter().copied().filter(|&c| ang_from(c, a0) < a1 - a0).collect();
        inside.sort_by(|x, y| ang_from(*x, a0).partial_cmp(&ang_from(*y, a0)).unwrap_or(std::cmp::Ordering::Equal));
        let at_hole = |a: f32| (hit.0 + a.cos() * hole, hit.1 + a.sin() * hole);
        let mut wedge = vec![at_hole(a0), leave(hit, a0, hw, hh)];
        wedge.extend(inside);
        wedge.push(leave(hit, a1, hw, hh));
        wedge.push(at_hole(a1));
        // Glass cracks fork: from a point part way along each of the wedge's two cracks a branch runs into the wedge (to its far edge), cutting it
        // into pieces, one of which starts away from the hit. The branches are chosen by a hash of the wedge's number.
        let mut pieces = vec![wedge];
        for (side, a) in [(0, a0), (1, a1)] {
            if hash01(i as i32, 10 + side) > BRANCH_SHARE {
                continue;
            }
            let end = leave(hit, a, hw, hh);
            let length = ((end.0 - hit.0).powi(2) + (end.1 - hit.1).powi(2)).sqrt();
            let r = length * (BRANCH_AT.0 + (BRANCH_AT.1 - BRANCH_AT.0) * hash01(i as i32, 12 + side));
            let b = (hit.0 + a.cos() * r, hit.1 + a.sin() * r);
            // Into the wedge: from a0 turning toward a1, from a1 turning back.
            let turn = (a1 - a0) * (BRANCH_TURN.0 + (BRANCH_TURN.1 - BRANCH_TURN.0) * hash01(i as i32, 14 + side));
            let dir = if side == 0 { a0 + turn } else { a1 - turn };
            let d = (dir.cos(), dir.sin());
            pieces = pieces
                .into_iter()
                .flat_map(|p| {
                    let (l, r) = (clip(&p, b, d, true), clip(&p, b, d, false));
                    if l.len() >= 3 && r.len() >= 3 {
                        vec![l, r]
                    } else {
                        vec![p]
                    }
                })
                .collect();
        }
        for (pi, poly) in pieces.iter().enumerate() {
            let n = poly.len() as f32;
            let (cx, cy) = (poly.iter().map(|p| p.0).sum::<f32>() / n, poly.iter().map(|p| p.1).sum::<f32>() / n);
            let at = |x: f32, y: f32| MeshVertex::new([x, y, 0.0], [(x + hw) / w, (y + hh) / h]);
            let mut vertices = vec![at(cx, cy)];
            vertices.extend(poly.iter().map(|p| at(p.0, p.1)));
            let count = poly.len() as u32;
            let indices = (0..count).flat_map(|j| [0, 1 + j, 1 + (j + 1) % count]).collect();
            // The piece turns about the radial line through its middle and falls at a time of its own.
            let mid = (cy - hit.1).atan2(cx - hit.0);
            let id = (i * 8 + pi) as i32;
            let open = (k / OPEN_FRAMES).clamp(0.0, 1.0);
            let open = open * open * (3.0 - 2.0 * open);
            let narrow = 1.0 - raw[i] / widest;
            let turn = OPEN_TURN * (0.25 + 0.75 * narrow) * (if hash01(id, 2) < 0.5 { -1.0 } else { 1.0 }) * open;
            let fall_at = FALL_FROM + FALL_ORDER * (1.0 - mid.sin()) / 2.0 + FALL_SPREAD * hash01(id, 3);
            let dy = fallen((k - fall_at).max(0.0)) * h;
            let spin = 0.004 * (hash01(id, 4) * 2.0 - 1.0) * (k - fall_at).max(0.0);
            let t = Transform3D::translate(-hit.0, -hit.1, 0.0)
                .then(Transform3D::rotate_z(-mid))
                .then(Transform3D::rotate_x(turn))
                .then(Transform3D::rotate_z(mid + spin))
                .then(Transform3D::translate(hit.0, hit.1 + dy, 0.0));
            cards.push(Card { face: Face::Old, transform: view(t), cull_back: false, shade: 1.0, mesh: Some(Mesh { vertices: Arc::new(vertices), indices }), white: 0.0, opacity: 1.0 });
        }
    }
    cards
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_starts_with_the_old_slide_and_ends_with_the_new_one_alone() {
        let start = fracture(0.0, 640.0, 360.0);
        assert_eq!((start.len(), start[1].face), (2, Face::Old));
        let end = fracture(1.0, 640.0, 360.0);
        assert_eq!((end.len(), end[0].face), (1, Face::New));
    }
}
