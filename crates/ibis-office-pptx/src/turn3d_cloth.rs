// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The cloth of プレステージ (`prestige`) and カーテン (`curtains`) (ibisOffice #88; PowerPoint for Windows 16.0.20527's video of the 2 s transition, 30
//! frames a second, `audit-trans-flat-16x9-dur2000-720p-30fps.mp4`, the 104th and the 37th slide): the old slide is a polygon mesh of nodes of equal mass
//! run as a mass-spring cloth, frame by frame from the start so that a picture is the same every time, and shaded from its triangles' normals.
//!
//! The mechanism, shared by both (`Cloth`):
//! * Every node is joined to its four neighbours and its two diagonal ones by a spring of stiffness `STIFFNESS` whose rest length is their distance on
//!   the flat slide (`F = k (|d| - rest)`, in three dimensions: a cloth gathered from the sides buckles into folds in Z), and to the nodes two steps
//!   away along the grid by a spring of `BEND` of that stiffness (the cloth resists a sharp fold).
//! * Every node is slowed by friction `FRICTION` (`v' = -c v`): the nodes that are not held follow the held ones late (inertia), so what a hand
//!   pulls up or opens at the top edge reaches the lower edge behind it.
//! * Some nodes are held, moved by the caller: prestige's grip near the middle of the top edge, and curtains' top edge.
//! * (Curtains only) a spring `plane` pulls each node's Z back to the plane.
//! * The nodes start off the plane by a deterministic hash of `SEED_Z` of the node spacing (the buckling needs something to start from).
//! * Shade: a triangle's shade is `SHADE_BASE + SHADE_RANGE |n . L|` (`n` its normal, `L` the light from the upper left in front), at most 1; the
//!   triangles are drawn in bands of shade as 'クラッシュ' (`turn3d_crush`) does.
//!
//! プレステージ: the grip is the nodes within `GRIP_RADIUS` of a point near the middle of the top edge, carried up by `PULL_FALL` of the slide's height
//! times the square of the share of `PULL_FRAMES` frames that has passed since `PULL_START` (a pull at a steady acceleration). Fitted to the lower
//! edge's height at 38 columns over the frames 14 to 37 of the video (rms 0.085 of the height); lengths are in ninths of the slide's height.
//!
//! カーテン: the held nodes are the top and the lower edge of each half, spread between the corners read off the video (`CURTAINS` of `turn3d_tracks`:
//! where the half's inner and outer edges are at the top and at the bottom, per frame: the lower edge lags the top one as the curtain opens, which
//! is the inertia); with `PLEATS` folds over the half's width, carried by both edges in Z (`PLEAT_DEPTH`, growing with the way the half has gathered),
//! and the cloth between them takes the folds and their shade.
//! The curtain's cloth keeps the prestige's `STIFFNESS` and `FRICTION`; the audit's error hardly depends on `PLEATS` (8 to 20) and `PLEAT_DEPTH` (0 to 0.7):
//! they are chosen to look like the video's pleats. Across the picture the nodes are placed between the two measured edges (a node's own sag in X and
//! Y would only add error); only their Z is the cloth's, with the spring `CURTAIN_PLANE` back to the plane.

use crate::turn3d::{pose_card, Card, Face, Mesh};
use gpui_kit::{MeshVertex, Transform3D};
use std::collections::BTreeMap;
use std::sync::Arc;

/// The sub-steps of a frame (30 frames a second) the springs are integrated in.
const SUBSTEPS: usize = 6;
const STIFFNESS: f32 = 1444.0;
const BEND: f32 = 0.3;
const FRICTION: f32 = 13.6;
const SEED_Z: f32 = 0.02;
/// The camera's focal length over the slide's width.
const FOCAL: f32 = 1000.0 / 640.0;
const LIGHT: [f32; 3] = [-0.35, -0.45, 0.82];
const SHADE_BASE: f32 = 0.55;
const SHADE_RANGE: f32 = 0.45;
const SHADE_STEP: f32 = 0.04;

/// A number of 0 to 1 from two lattice numbers (an integer hash).
fn hash01(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297A_2D39);
    h ^= h >> 15;
    (h & 0xFFFF) as f32 / 65535.0
}

/// A mass-spring cloth of `nx` x `ny` nodes over `width` x `height` (any unit: the springs' rest lengths are those of the flat grid).
struct Cloth {
    nx: usize,
    ny: usize,
    rest: Vec<[f32; 3]>,
    x: Vec<[f32; 3]>,
    v: Vec<[f32; 3]>,
    held: Vec<bool>,
    springs: Vec<(usize, usize, f32, f32)>,
    /// A spring pulling each node's Z back to the plane (per second squared): the cloth hangs in a plane and folds only where it is made to.
    plane: f32,
}

impl Cloth {
    fn new(nx: usize, ny: usize, width: f32, height: f32) -> Self {
        let rest: Vec<[f32; 3]> = (0..nx * ny).map(|n| [(n % nx) as f32 / (nx - 1) as f32 * width, (n / nx) as f32 / (ny - 1) as f32 * height, 0.0]).collect();
        let spacing = width / (nx - 1) as f32;
        let x: Vec<[f32; 3]> = rest.iter().enumerate().map(|(n, p)| [p[0], p[1], SEED_Z * spacing * (hash01((n % nx) as i32, (n / nx) as i32) * 2.0 - 1.0)]).collect();
        let mut springs = Vec::new();
        for j in 0..ny {
            for i in 0..nx {
                // The four neighbours (right, down, two diagonals) at full stiffness and the two steps away along the grid at `BEND` of it.
                for (di, dj, share) in [(1_i32, 0_i32, 1.0), (0, 1, 1.0), (1, 1, 1.0), (-1, 1, 1.0), (2, 0, BEND), (0, 2, BEND)] {
                    let (bi, bj) = (i as i32 + di, j as i32 + dj);
                    if bi < 0 || bj < 0 || bi >= nx as i32 || bj >= ny as i32 {
                        continue;
                    }
                    let (a, b) = (j * nx + i, bj as usize * nx + bi as usize);
                    let l0 = (0..3).map(|c| (rest[a][c] - rest[b][c]).powi(2)).sum::<f32>().sqrt();
                    springs.push((a, b, l0, share));
                }
            }
        }
        Self { nx, ny, v: vec![[0.0; 3]; nx * ny], held: vec![false; nx * ny], rest, x, springs, plane: 0.0 }
    }

    /// One sub-step of `dt` seconds: the springs and the friction move the nodes that are not held; `hold` places the held ones.
    fn step(&mut self, dt: f32, hold: &dyn Fn(usize, [f32; 3]) -> Option<[f32; 3]>) {
        let mut force = vec![[0.0_f32; 3]; self.x.len()];
        for &(a, b, l0, share) in &self.springs {
            let d = [self.x[b][0] - self.x[a][0], self.x[b][1] - self.x[a][1], self.x[b][2] - self.x[a][2]];
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
            let f = STIFFNESS * share * (l - l0) / l;
            for c in 0..3 {
                force[a][c] += f * d[c];
                force[b][c] -= f * d[c];
            }
        }
        let damping = (-FRICTION * dt).exp();
        for n in 0..self.x.len() {
            force[n][2] -= self.plane * self.x[n][2];
            if let Some(p) = hold(n, self.rest[n]) {
                self.x[n] = p;
                self.v[n] = [0.0; 3];
                self.held[n] = true;
            } else {
                for c in 0..3 {
                    self.v[n][c] = (self.v[n][c] + force[n][c] * dt) * damping;
                    self.x[n][c] += self.v[n][c] * dt;
                }
            }
        }
    }

    /// Run `frames` frames from the start (`hold(frame, node, rest)` places the held nodes).
    fn run(&mut self, frames: f32, hold: &dyn Fn(f32, usize, [f32; 3]) -> Option<[f32; 3]>, place_xy: &dyn Fn(f32, [f32; 3]) -> Option<[f32; 2]>) {
        let steps = (frames.max(0.0) * SUBSTEPS as f32).round() as usize;
        let dt = 1.0 / 30.0 / SUBSTEPS as f32;
        for s in 0..steps {
            let frame = s as f32 / SUBSTEPS as f32;
            self.step(dt, &|n, rest| hold(frame, n, rest));
            // Nodes whose place across the picture is given (the curtain's, between its two edges) keep it; only their Z is the cloth's.
            for n in 0..self.x.len() {
                if let Some(xy) = place_xy(frame, self.rest[n]) {
                    self.x[n][0] = xy[0];
                    self.x[n][1] = xy[1];
                    self.v[n][0] = 0.0;
                    self.v[n][1] = 0.0;
                }
            }
        }
    }

    /// The cards of the cloth: its triangles in bands of shade. `place` turns a node (in the cloth's units) into the vertex's position (px from the
    /// slide's middle) and `uv` its texture point.
    fn cards(&self, transform: Transform3D, place: &dyn Fn([f32; 3]) -> [f32; 3], uv: &dyn Fn(usize, usize) -> [f32; 2]) -> Vec<Card> {
        let (nx, ny) = (self.nx, self.ny);
        let vertices: Vec<MeshVertex> = (0..nx * ny).map(|n| MeshVertex::new(place(self.x[n]), uv(n % nx, n / nx))).collect();
        let light_length = (LIGHT[0] * LIGHT[0] + LIGHT[1] * LIGHT[1] + LIGHT[2] * LIGHT[2]).sqrt();
        let front = LIGHT[2].abs() / light_length;
        let mut bands: BTreeMap<i32, Vec<u32>> = BTreeMap::new();
        for j in 0..ny - 1 {
            for i in 0..nx - 1 {
                let id = |a: usize, b: usize| (b * nx + a) as u32;
                let (tl, tr, bl, br) = (id(i, j), id(i + 1, j), id(i, j + 1), id(i + 1, j + 1));
                for tri in [[tl, tr, bl], [tr, br, bl]] {
                    let p = |k: usize| vertices[tri[k] as usize].position;
                    let (a, b, d) = (p(0), p(1), p(2));
                    let (e1, e2) = ([b[0] - a[0], b[1] - a[1], b[2] - a[2]], [d[0] - a[0], d[1] - a[1], d[2] - a[2]]);
                    let normal = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
                    let length = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt().max(1e-6);
                    let facing = ((normal[0] * LIGHT[0] + normal[1] * LIGHT[1] + normal[2] * LIGHT[2]) / (length * light_length)).abs();
                    // A sheet facing the viewer is lit as it is (shade 1), one turned from it or folded is darker.
                    let shade = (SHADE_BASE + SHADE_RANGE * (facing / front).min(1.0)).clamp(0.0, 1.0);
                    bands.entry((shade / SHADE_STEP).round() as i32).or_default().extend(tri);
                }
            }
        }
        let vertices = Arc::new(vertices);
        bands
            .into_iter()
            .map(|(band, indices)| Card { face: Face::Old, transform, cull_back: false, shade: (band as f32 * SHADE_STEP).min(1.0), mesh: Some(Mesh { vertices: vertices.clone(), indices }), white: 0.0, opacity: 1.0 })
            .collect()
    }
}

// プレステージ.
const PRESTIGE_NODES: (usize, usize) = (33, 19);
/// The grip: where (over the slide's width and height) and the radius (ninths of the height) of the nodes held.
const GRIP: (f32, f32) = (0.463, 0.018);
const GRIP_RADIUS: f32 = 3.66;
/// The pull: how far (heights) the grip goes up, when it starts (frames after the transition's first moving frame) and over how many frames.
const PULL_FALL: f32 = 1.96;
const PULL_START: f32 = 3.35;
const PULL_FRAMES: f32 = 13.3;

/// The prestige cloth's nodes after `frames` frames (x, y in ninths of the slide's height from its top left; z the same) on a slide `aspect` times as wide as high.
fn prestige_cloth(frames: f32, aspect: f32) -> Cloth {
    let (nx, ny) = PRESTIGE_NODES;
    let (width, height) = (9.0 * aspect, 9.0);
    let mut cloth = Cloth::new(nx, ny, width, height);
    cloth.run(frames, &|frame, _, rest| {
        let held = ((rest[0] - GRIP.0 * width).powi(2) + (rest[1] - GRIP.1 * height).powi(2)).sqrt() <= GRIP_RADIUS;
        let u = ((frame - PULL_START) / PULL_FRAMES).clamp(0.0, 1.0);
        held.then(|| [rest[0], rest[1] - PULL_FALL * height * u * u, 0.0])
    }, &|_, _| None);
    cloth
}

/// プレステージ at `q` (0 to 1, linear): the new slide, and the old one as the cloth.
pub(crate) fn prestige(q: f32, w: f32, h: f32) -> Vec<Card> {
    let mut cards = vec![pose_card(Face::New, [0.0; 6], 1.0, w, h)];
    if q >= 1.0 {
        return cards;
    }
    if q <= 0.0 {
        cards.push(pose_card(Face::Old, [0.0; 6], 1.0, w, h));
        return cards;
    }
    let cloth = prestige_cloth(q * 60.0, w / h);
    let unit = h / 9.0;
    let (nx, ny) = PRESTIGE_NODES;
    let transform = Transform3D::perspective(FOCAL * w).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0));
    cards.extend(cloth.cards(
        transform,
        &|p| [p[0] * unit - w / 2.0, p[1] * unit - h / 2.0, p[2] * unit],
        &|i, j| [i as f32 / (nx - 1) as f32, j as f32 / (ny - 1) as f32],
    ));
    cards
}

// カーテン.
const CURTAIN_NODES: (usize, usize) = (25, 19);
/// The folds over a half's width and how deep they are (ninths of the slide's height) once the half has gathered as far as it does.
const PLEATS: f32 = 11.0;
const PLEAT_DEPTH: f32 = 0.35;
const CURTAIN_PLANE: f32 = 300.0;

/// The curtain's left half after `frames` frames of the transition (the first moving frame 0; the table's frame 12 is it), the cloth over the left half of
/// a slide `aspect` times as wide as high: the node column 0 is the half's outer edge, the last the inner one.
fn curtain_cloth(frames: f32, aspect: f32) -> Cloth {
    use crate::turn3d_tracks::CURTAINS;
    let (nx, ny) = CURTAIN_NODES;
    let (width, height) = (4.5 * aspect, 9.0);
    let mut cloth = Cloth::new(nx, ny, width, height);
    cloth.plane = CURTAIN_PLANE;
    // The table's px of a 640 px slide in the cloth's units.
    let unit_per_px = 9.0 * aspect / 640.0;
    // The table's edges at a frame of the transition: [inner top, inner bottom, outer top, outer bottom] in px of a 640 px slide.
    let edges = |frame: f32| {
        let table_frame = 12.0 + frame;
        if table_frame < CURTAINS[0].0 { [320.0, 320.0, 0.0, 0.0] } else { crate::turn3d::row_at(CURTAINS, table_frame) }
    };
    // How far the half has gathered at a height `v` (0 top, 1 bottom), 0 (open) to 1: the share of its width that the edge there has lost.
    let gathered = |frame: f32, v: f32| {
        let [lt, lb, ot, ob] = edges(frame);
        let (inner, outer) = (lt + (lb - lt) * v, ot + (ob - ot) * v);
        (1.0 - (inner - outer) / 320.0).clamp(0.0, 1.0)
    };
    let fold = |frame: f32, rest: [f32; 3]| PLEAT_DEPTH * gathered(frame, rest[1] / height).sqrt() * (std::f32::consts::TAU * PLEATS * rest[0] / width).sin();
    cloth.run(
        frames,
        // The top and the lower edge carry the folds in Z; what lies between them is the cloth, which hands them down.
        &|frame, _, rest| {
            let edge = rest[1] < 1e-6 || rest[1] > height - 1e-6;
            edge.then(|| [rest[0], rest[1], fold(frame, rest)])
        },
        // Across the picture every node is between the two edges read off the video (the lower edge lags the top one as the curtain opens: the
        // inertia, measured): the edge's outer and inner ends at its height, the node's share `u` of the way across.
        &|frame, rest| {
            let [lt, lb, ot, ob] = edges(frame);
            let v = rest[1] / height;
            let (inner, outer) = (lt + (lb - lt) * v, ot + (ob - ot) * v);
            Some([(outer + (inner - outer) * rest[0] / width) * unit_per_px, rest[1]])
        },
    );
    cloth
}

/// カーテン at `t` (0 to 1, linear): the new slide, and the old slide's two halves as cloth (the right one the left one's mirror).
pub(crate) fn curtains(t: f32, w: f32, h: f32) -> Vec<Card> {
    use crate::turn3d_tracks::CURTAINS;
    let mut cards = vec![pose_card(Face::New, [0.0; 6], 1.0, w, h)];
    let frame = t.clamp(0.0, 1.0) * 60.0;
    if 12.0 + frame >= CURTAINS[CURTAINS.len() - 1].0 {
        return cards;
    }
    let cloth = curtain_cloth(frame, w / h);
    let unit = h / 9.0;
    let (nx, ny) = CURTAIN_NODES;
    let transform = Transform3D::perspective(FOCAL * w).then(Transform3D::translate(w / 2.0, h / 2.0, 0.0));
    for right in [false, true] {
        cards.extend(cloth.cards(
            transform,
            &|p| {
                let px = p[0] * unit;
                [if right { w / 2.0 - px } else { px - w / 2.0 }, p[1] * unit - h / 2.0, p[2] * unit]
            },
            &|i, j| {
                let u = i as f32 / (nx - 1) as f32;
                [if right { 1.0 - u * 0.5 } else { u * 0.5 }, j as f32 / (ny - 1) as f32]
            },
        ));
    }
    cards
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_starts_with_the_old_slide_and_ends_with_the_new_one_alone() {
        assert_eq!(prestige(0.0, 640.0, 360.0)[1].face, Face::Old);
        assert_eq!(prestige(1.0, 640.0, 360.0).len(), 1);
        assert_eq!(curtains(1.0, 640.0, 360.0).len(), 1);
    }

    #[test]
    fn the_cloth_rises_behind_the_hand_with_its_lower_corners_last() {
        // At frame 23 (the video's lower edge: the middle at 0.84, the corners above 0.95) the middle of the lower edge is higher than the corners.
        let cloth = prestige_cloth(23.0 - 12.0, 16.0 / 9.0);
        let (nx, ny) = PRESTIGE_NODES;
        let bottom = |i: usize| cloth.x[(ny - 1) * nx + i][1] / 9.0;
        assert!(bottom(nx / 2) < bottom(0) && bottom(nx / 2) < bottom(nx - 1), "{} {} {}", bottom(0), bottom(nx / 2), bottom(nx - 1));
        assert!(bottom(nx / 2) < 1.0);
        // It is all above the slide's top by the end.
        let late = prestige_cloth(40.0, 16.0 / 9.0);
        assert!(late.x.iter().all(|p| p[1] < 0.5), "the cloth leaves");
    }

    #[test]
    fn the_curtains_lower_edge_lags_the_top_edge_as_it_opens() {
        // While the top edge's inner corner moves toward the side (frames 14 to 30 of the table), the lower edge's has moved less (inertia).
        let cloth = curtain_cloth(14.0, 16.0 / 9.0);
        let (nx, ny) = CURTAIN_NODES;
        let inner = |j: usize| cloth.x[j * nx + nx - 1][0];
        let rest = cloth.rest[nx - 1][0];
        assert!(rest - inner(0) > rest - inner(ny - 1), "top {} bottom {}", rest - inner(0), rest - inner(ny - 1));
    }
}
