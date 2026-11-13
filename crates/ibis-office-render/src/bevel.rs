// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The bevel of a flat shape (面取り, `a:sp3d/a:bevelT`) seen from the front, as shading: the outline's edges give strips (as wide
//! as the bevel) towards the inside, each strip coloured by how the preset's profile catches the light on that side. The
//! shading (a diffuse factor and a white specular share across the width, for the top, left, right and bottom, for each preset
//! at a steep and at a shallow bevel) is measured on PowerPoint's own picture (`data/bevel_profiles.tsv`, made by
//! `scripts/coverage/measure_bevel.py`); the camera's turn and the extrusion are not drawn.

use std::collections::HashMap;
use std::sync::LazyLock;

use ibis_doc_model::BevelPreset;

/// Samples across the width of the bevel.
pub(crate) const SAMPLES: usize = 16;

type Profile = [(f32, f32); SAMPLES];

/// The side of a profile: where the surface faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Side {
    Top,
    Left,
    Right,
    Bottom,
}

fn preset_of(name: &str) -> Option<BevelPreset> {
    BevelPreset::parse(match name {
        "relaxedInset" | "relaxedIns" => "relaxedInset",
        other => other,
    })
}

/// `(preset, steep, side)` to the profile, and to the shading of the outline over the edge.
type Tables = (HashMap<(BevelPreset, bool, Side), Profile>, HashMap<(BevelPreset, bool, Side), (f32, f32)>);

static TABLES: LazyLock<Tables> = LazyLock::new(|| {
    let mut map: HashMap<(BevelPreset, bool, Side), Profile> = HashMap::new();
    let mut strokes: HashMap<(BevelPreset, bool, Side), (f32, f32)> = HashMap::new();
    for line in include_str!("../data/bevel_profiles.tsv").lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()) {
        let f: Vec<&str> = line.split('\t').collect();
        let (Some(preset), Some(steep), Some(side), Some(k), Some(d), Some(s)) = (
            f.first().and_then(|p| preset_of(p)),
            f.get(1).map(|c| *c == "steep"),
            f.get(2).and_then(|s| match *s {
                "T" => Some(Side::Top),
                "L" => Some(Side::Left),
                "R" => Some(Side::Right),
                "B" => Some(Side::Bottom),
                _ => None,
            }),
            f.get(3).copied(),
            f.get(4).and_then(|d| d.parse::<f32>().ok()),
            f.get(5).and_then(|s| s.parse::<f32>().ok()),
        ) else {
            continue;
        };
        if k == "stroke" {
            strokes.insert((preset, steep, side), (d, s));
        } else if let Some(k) = k.parse::<usize>().ok().filter(|k| *k < SAMPLES) {
            map.entry((preset, steep, side)).or_insert([(1.0, 0.0); SAMPLES])[k] = (d, s);
        }
    }
    (map, strokes)
});

/// The profile of a side for a bevel `ratio` (height over width) steep (1 or more) to shallow (0.2 or less) and between.
fn profile(preset: BevelPreset, ratio: f32, side: Side) -> Profile {
    let steep = TABLES.0.get(&(preset, true, side)).copied().unwrap_or([(1.0, 0.0); SAMPLES]);
    let shallow = TABLES.0.get(&(preset, false, side)).copied().unwrap_or(steep);
    let t = ((ratio - 0.2) / 0.8).clamp(0.0, 1.0);
    let mut out = steep;
    for k in 0..SAMPLES {
        out[k] = (shallow[k].0 + (steep[k].0 - shallow[k].0) * t, shallow[k].1 + (steep[k].1 - shallow[k].1) * t);
    }
    out
}

/// The point `d` inside the closed outline at each vertex (the corner of the offset edges): `inward` are the unit normals of the
/// edges pointing inside; a very sharp corner does not run off.
fn offset(points: &[(f32, f32)], inward: &[(f32, f32)], d: f32) -> Vec<(f32, f32)> {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (inward[(i + n - 1) % n], inward[i]);
            let denom = 1.0 + a.0 * b.0 + a.1 * b.1;
            let (mx, my) = if denom < 0.25 { (a.0 + b.0, a.1 + b.1) } else { ((a.0 + b.0) / denom, (a.1 + b.1) / denom) };
            (points[i].0 + mx * d, points[i].1 + my * d)
        })
        .collect()
}

/// How the light rig of a bevelled shape (three points, the default) lights the face: the colour the face has is the fill's
/// times this diffuse factor plus this white share (measured on PowerPoint's picture of PEFF-00086..00109).
const FACE_DIFFUSE: f32 = 1.02;
const FACE_SPECULAR: f32 = 0.026;

/// The colour of the lit face of a shape filled with `base`.
pub(crate) fn face_color(base: [u8; 3]) -> [u8; 3] {
    base.map(|c| (f32::from(c) * FACE_DIFFUSE + 255.0 * FACE_SPECULAR).round().clamp(0.0, 255.0) as u8)
}

/// One painted piece: its corners and its colour (red, green, blue).
/// Pixels the lit outline reaches further out than the plain one (PowerPoint draws bevelled shapes a little larger).
const STROKE_GROW: f32 = 0.5;

pub type Piece = (Vec<(f32, f32)>, [u8; 3]);

/// The outline without repeated points, and the unit normals of its edges pointing inside.
fn prepared(points: &[(f32, f32)]) -> Option<(Vec<(f32, f32)>, Vec<(f32, f32)>)> {
    let mut pts: Vec<(f32, f32)> = Vec::with_capacity(points.len());
    for &p in points {
        if pts.last().is_none_or(|l| (l.0 - p.0).hypot(l.1 - p.1) > 1e-3) {
            pts.push(p);
        }
    }
    if pts.len() > 1 && (pts[0].0 - pts[pts.len() - 1].0).hypot(pts[0].1 - pts[pts.len() - 1].1) <= 1e-3 {
        pts.pop();
    }
    let n = pts.len();
    if n < 3 {
        return None;
    }
    let area: f32 = (0..n).map(|i| pts[i].0 * pts[(i + 1) % n].1 - pts[(i + 1) % n].0 * pts[i].1).sum::<f32>() / 2.0;
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let inward: Vec<(f32, f32)> = (0..n)
        .map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = dx.hypot(dy).max(1e-6);
            (-dy / len * sign, dx / len * sign)
        })
        .collect();
    Some((pts, inward))
}

/// How much each side's shading counts for an edge whose outward normal is `(nx, ny)`: top, left, right, bottom.
fn side_weights(nx: f32, ny: f32) -> [f32; 4] {
    let w = [(-ny).max(0.0), (-nx).max(0.0), nx.max(0.0), ny.max(0.0)];
    let total: f32 = w.iter().sum::<f32>().max(1e-6);
    w.map(|v| v / total)
}

/// The pieces that paint the outline of a bevelled shape (a line `width` pixels wide centred on the edge, in `color`): each edge
/// is lit as the edge of the bevel is, so the outline is lighter at the top and darker on the right and at the bottom.
pub(crate) fn stroke_pieces(points: &[(f32, f32)], width: f32, ratio: f32, preset: BevelPreset, color: [u8; 3]) -> Vec<Piece> {
    let Some((pts, inward)) = prepared(points) else { return Vec::new() };
    let n = pts.len();
    let t = ((ratio - 0.2) / 0.8).clamp(0.0, 1.0);
    let shade = |side: Side| {
        let steep = TABLES.1.get(&(preset, true, side)).copied().unwrap_or((1.0, 0.0));
        let shallow = TABLES.1.get(&(preset, false, side)).copied().unwrap_or(steep);
        (shallow.0 + (steep.0 - shallow.0) * t, shallow.1 + (steep.1 - shallow.1) * t)
    };
    let shades = [Side::Top, Side::Left, Side::Right, Side::Bottom].map(shade);
    let (outer, inner) = (offset(&pts, &inward, -width / 2.0 - STROKE_GROW), offset(&pts, &inward, width / 2.0));
    (0..n)
        .map(|i| {
            let j = (i + 1) % n;
            let weights = side_weights(-inward[i].0, -inward[i].1);
            let (d, s) = weights.iter().zip(&shades).fold((0.0, 0.0), |a, (w, p)| (a.0 + w * p.0, a.1 + w * p.1));
            let c = [0, 1, 2].map(|c| (f32::from(color[c]) * d + 255.0 * s).round().clamp(0.0, 255.0) as u8);
            (vec![outer[i], outer[j], inner[j], inner[i]], c)
        })
        .collect()
}

/// The pieces to paint over a shape's fill (in this order) for its bevel: `points` the closed outline in pixels, `width` the
/// bevel's width in pixels, `ratio` its height over its width, `base` the colour of the shape's face.
pub fn pieces(points: &[(f32, f32)], width: f32, ratio: f32, preset: BevelPreset, base: [u8; 3], repaint_face: bool) -> Vec<Piece> {
    let face = face_color(base);
    let Some((pts, inward)) = prepared(points) else { return Vec::new() };
    let n = pts.len();
    if width < 0.5 {
        return Vec::new();
    }
    let (min_x, max_x) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.0), a.1.max(p.0)));
    let (min_y, max_y) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
    let width = width.min((max_x - min_x).min(max_y - min_y) / 2.0 * 0.999);
    let rings: Vec<Vec<(f32, f32)>> = (0..=SAMPLES).map(|k| offset(&pts, &inward, width * k as f32 / SAMPLES as f32)).collect();
    let profiles = [Side::Top, Side::Left, Side::Right, Side::Bottom].map(|s| profile(preset, ratio, s));
    let mut out = Vec::with_capacity(SAMPLES * n + 2);
    if repaint_face {
        out.push((pts.clone(), face));
    }
    // A strip reaches a little way into the next one, so that no seam of the edges' smoothing shows between them.
    let overlap = 0.75f32;
    for k in 0..SAMPLES {
        let reach = if k + 1 < SAMPLES || repaint_face { Some(offset(&pts, &inward, width * (k + 1) as f32 / SAMPLES as f32 + overlap)) } else { None };
        let inner = reach.as_ref().unwrap_or(&rings[SAMPLES]);
        for i in 0..n {
            let j = (i + 1) % n;
            let (nx, ny) = (-inward[i].0, -inward[i].1);
            // How much each side's profile counts for this edge's direction.
            let weights = side_weights(nx, ny);
            let (mut d, mut s) = (0.0, 0.0);
            for (w, p) in weights.iter().zip(&profiles) {
                d += w * p[k].0;
                s += w * p[k].1;
            }
            let color = [0, 1, 2].map(|c| (f32::from(face[c]) * d + 255.0 * s).round().clamp(0.0, 255.0) as u8);
            out.push((vec![rings[k][i], rings[k][j], inner[j], inner[i]], color));
        }
    }
    if repaint_face {
        out.push((rings[SAMPLES].clone(), face));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_measured_profiles_are_there_for_every_preset() {
        for preset in [BevelPreset::RelaxedInset, BevelPreset::Circle, BevelPreset::Slope, BevelPreset::Cross, BevelPreset::Angle, BevelPreset::SoftRound, BevelPreset::Convex, BevelPreset::CoolSlant, BevelPreset::Divot, BevelPreset::Riblet, BevelPreset::HardEdge, BevelPreset::ArtDeco] {
            for steep in [true, false] {
                for side in [Side::Top, Side::Left, Side::Right, Side::Bottom] {
                    assert!(TABLES.0.contains_key(&(preset, steep, side)), "{preset:?} {steep} {side:?}");
                }
            }
        }
    }

    #[test]
    fn a_rectangle_gets_trapezoids_lit_from_the_top_left() {
        let rect = [(0.0, 0.0), (100.0, 0.0), (100.0, 60.0), (0.0, 60.0)];
        let base = [76, 123, 209];
        let all = pieces(&rect, 10.0, 1.0, BevelPreset::Angle, base, false);
        assert_eq!(all.len(), SAMPLES * 4);
        // A strip near the outline on each side (the profile is flat against the face).
        let last = &all[4 * 4..5 * 4];
        let lum = |c: [u8; 3]| u32::from(c[0]) + u32::from(c[1]) + u32::from(c[2]);
        let (top, right, bottom, left) = (lum(last[0].1), lum(last[1].1), lum(last[2].1), lum(last[3].1));
        assert!(top > left && left > right && right >= bottom - 40, "{top} {left} {right} {bottom}");
        // A trapezoid: its outer edge on the outline, its inner one a sixteenth of the width in (and a little over the next strip).
        let t = &all[0].0;
        let inner = 10.0 / SAMPLES as f32 + 0.75;
        assert!((t[0].1).abs() < 1e-3 && (t[1].1).abs() < 1e-3 && (t[2].1 - inner).abs() < 1e-3, "{t:?}");
        assert!((t[3].0 - inner).abs() < 1e-3, "mitered: {t:?}");
    }

    #[test]
    fn the_bevel_does_not_outgrow_the_shape() {
        let small = [(0.0, 0.0), (8.0, 0.0), (8.0, 8.0), (0.0, 8.0)];
        for (poly, _) in pieces(&small, 50.0, 1.0, BevelPreset::Circle, [200, 100, 50], false) {
            for (x, y) in poly {
                assert!((-0.01..=8.01).contains(&x) && (-0.01..=8.01).contains(&y), "{x} {y}");
            }
        }
    }
}
