// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Gradients painted band by band: linear gradients (`Fill::Linear`) and
//! path gradients (`Fill::Path`), with any number of stops, the colours
//! mixed as PowerPoint mixes them (measured on its PDFs of the coverage
//! suites PFIL and PSHP): between the stops of a gradient of three or more,
//! the sRGB values in proportion; in a gradient of just two, each channel
//! stays near its higher value, moving by the square of the distance from
//! it (half way from yellow to blue is a light pinkish grey, not the darker
//! olive of the plain mix).
//!
//! The area (subpaths in a shape's own coordinates, 0..w × 0..h) is cut
//! into thin bands, each painted in one colour: across the gradient's
//! direction for a linear one; for a path one, from the outside in, each
//! band the area inside a smaller outline (an ellipse, a rectangle or the
//! shape itself) laid over the one before.

use gpui_kit::*;
use ibis_doc_model::{Color as DocColor, Fill, GradientPath, GradientStop};

type Pt = (f32, f32);

/// Whether `fill` is painted by `paint_gradient`.
pub(crate) fn is_gradient(fill: &Fill) -> bool {
    matches!(fill, Fill::Linear { .. } | Fill::TileLinear { .. } | Fill::Path { .. })
}

/// The colour of `stops` at `t` (0 to 1).
#[allow(dead_code)]
pub fn color_at(stops: &[GradientStop], t: f32) -> Hsla {
    color_at_mode(stops, t, true)
}

pub fn color_at_mode(stops: &[GradientStop], t: f32, powerpoint: bool) -> Hsla {
    let pos = t.clamp(0.0, 1.0) * 100_000.0;
    let (a, b) = match stops.iter().position(|s| s.position as f32 >= pos) {
        Some(0) => (stops[0], stops[0]),
        Some(i) => (stops[i - 1], stops[i]),
        None => {
            let last = *stops.last().unwrap_or(&GradientStop { position: 0, color: DocColor::BLACK });
            (last, last)
        }
    };
    let span = b.position as f32 - a.position as f32;
    let f = if span > 0.0 { (pos - a.position as f32) / span } else { 0.0 };
    let channels = |c: DocColor| {
        let v = c.rgb.0;
        [((v >> 16) & 0xff) as f32 / 255.0, ((v >> 8) & 0xff) as f32 / 255.0, (v & 0xff) as f32 / 255.0, c.alpha as f32 / 255.0]
    };
    let (ca, cb) = (channels(a.color), channels(b.color));
    let two = powerpoint && stops.len() == 2;
    let mix = |i: usize| {
        let (a, b) = (ca[i], cb[i]);
        if !two {
            a + (b - a) * f
        } else if b < a {
            a - (a - b) * f * f
        } else {
            b - (b - a) * (1.0 - f) * (1.0 - f)
        }
    };
    Rgba { r: mix(0), g: mix(1), b: mix(2), a: ca[3] + (cb[3] - ca[3]) * f }.into()
}

/// `polygon` cut to the side of a line where `inside` is not negative.
fn clip_half(polygon: &[Pt], inside: impl Fn(Pt) -> f32) -> Vec<Pt> {
    let mut out = Vec::with_capacity(polygon.len() + 4);
    for (i, &p) in polygon.iter().enumerate() {
        let q = polygon[(i + 1) % polygon.len()];
        let (dp, dq) = (inside(p), inside(q));
        if dp >= 0.0 {
            out.push(p);
        }
        if (dp >= 0.0) != (dq >= 0.0) {
            let f = dp / (dp - dq);
            out.push((p.0 + (q.0 - p.0) * f, p.1 + (q.1 - p.1) * f));
        }
    }
    out
}

/// `polygon` cut to the inside of the convex polygon `clip`.
pub(crate) fn clip_convex(polygon: &[Pt], clip: &[Pt]) -> Vec<Pt> {
    let area: f32 = clip.iter().enumerate().map(|(i, a)| {
        let b = clip[(i + 1) % clip.len()];
        a.0 * b.1 - b.0 * a.1
    }).sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let mut out = polygon.to_vec();
    for (i, &a) in clip.iter().enumerate() {
        if out.len() < 3 {
            break;
        }
        let b = clip[(i + 1) % clip.len()];
        out = clip_half(&out, |p| sign * ((b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)));
    }
    out
}

/// Paint `fill` (a gradient) over the closed `subpaths` (a shape's own
/// coordinates in a `w` × `h` box), placed in the window by `map`;
/// `px_per_unit` is how many pixels a unit is (for the bands' width).
pub(crate) fn paint_gradient(
    window: &mut Window,
    fill: &Fill,
    subpaths: &[Vec<Pt>],
    (w, h): (f32, f32),
    px_per_unit: f32,
    map: &dyn Fn(&[Pt]) -> Vec<Point<Pixels>>,
) {
    paint_gradient_mode(window, fill, subpaths, (w, h), px_per_unit, true, map);
}

pub fn paint_gradient_mode(
    window: &mut Window,
    fill: &Fill,
    subpaths: &[Vec<Pt>],
    (w, h): (f32, f32),
    px_per_unit: f32,
    powerpoint: bool,
    map: &dyn Fn(&[Pt]) -> Vec<Point<Pixels>>,
) {
    let mut paint = |pieces: Vec<Vec<Pt>>, color: Hsla| {
        let mut builder = PathBuilder::fill();
        let mut any = false;
        for piece in pieces.iter().filter(|p| p.len() >= 3) {
            let points = map(piece);
            builder.move_to(points[0]);
            for p in &points[1..] {
                builder.line_to(*p);
            }
            builder.close();
            any = true;
        }
        if any && let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
    };
    gradient_pieces(fill, subpaths, (w, h), px_per_unit, powerpoint, &mut paint);
}

/// The bands `paint_gradient_mode` paints, in painting order (each laid over the ones before): polygons in the shape's own coordinates and the
/// colour of each, handed to `paint`. A soft edge (`soft_edge.rs`) takes them as layers of its picture, so every gradient is one code path.
pub(crate) fn gradient_pieces(
    fill: &Fill,
    subpaths: &[Vec<Pt>],
    (w, h): (f32, f32),
    px_per_unit: f32,
    powerpoint: bool,
    paint: &mut dyn FnMut(Vec<Vec<Pt>>, Hsla),
) {
    match fill {
        Fill::Linear { angle, scaled, stops } if !stops.is_empty() => {
            let a = (*angle as f32 / 60_000.0).to_radians();
            let n = if *scaled && w > 0.0 && h > 0.0 {
                (a.cos() / w, a.sin() / h)
            } else {
                (a.cos(), a.sin())
            };
            let t = |p: Pt| n.0 * p.0 + n.1 * p.1;
            let corners = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(t);
            let (t0, t1) = corners.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
            let extent = if *scaled && w > 0.0 && h > 0.0 {
                w.hypot(h) * px_per_unit
            } else {
                (t1 - t0) * px_per_unit
            };
            let bands = (extent.ceil() as usize).clamp(1, 256);
            let step = (t1 - t0) / bands as f32;
            // Each band reaches a pixel into the next, which is painted
            // over it, so that no seam shows between them.
            let overlap = n.0.hypot(n.1) / px_per_unit.max(1e-6);
            for i in 0..bands {
                let color = color_at_mode(stops, (i as f32 + 0.5) / bands as f32, powerpoint);
                // (Not a see-through band: twice over, it would be darker.)
                let overlap = if color.a >= 1.0 { overlap } else { 0.0 };
                let lo = t0 + step * i as f32;
                let hi = if i + 1 == bands { t1 + overlap } else { lo + step + overlap };
                let pieces = subpaths
                    .iter()
                    .map(|s| {
                        let s = if i == 0 { s.clone() } else { clip_half(s, |p| t(p) - lo) };
                        if i + 1 == bands { s } else { clip_half(&s, |p| hi - t(p)) }
                    })
                    .collect();
                paint(pieces, color);
            }
        }
        Fill::TileLinear { angle, scaled, stops, tile, flip } if !stops.is_empty() => {
            let [l, t, r, b] = tile.map(|v| v as f32 / 100_000.0);
            let tw = (w * (1.0 - r - l)).max(1.0);
            let th = (h * (1.0 - b - t)).max(1.0);
            let ox = w * l;
            let oy = h * t;
            let col_min = ((0.0 - ox) / tw).floor() as i32;
            let col_max = ((w - ox) / tw).ceil() as i32;
            let row_min = ((0.0 - oy) / th).floor() as i32;
            let row_max = ((h - oy) / th).ceil() as i32;
            let col_min = col_min.clamp(-10, 10);
            let col_max = col_max.clamp(-10, 10);
            let row_min = row_min.clamp(-10, 10);
            let row_max = row_max.clamp(-10, 10);

            for row in row_min..row_max {
                for col in col_min..col_max {
                    let x0 = ox + col as f32 * tw;
                    let x1 = x0 + tw;
                    let y0 = oy + row as f32 * th;
                    let y1 = y0 + th;

                    let is_flip_x = match flip {
                        ibis_doc_model::TileFlip::X | ibis_doc_model::TileFlip::Xy => col.rem_euclid(2) == 1,
                        _ => false,
                    };
                    let is_flip_y = match flip {
                        ibis_doc_model::TileFlip::Y | ibis_doc_model::TileFlip::Xy => row.rem_euclid(2) == 1,
                        _ => false,
                    };

                    let mut tile_angle = *angle;
                    if is_flip_x {
                        tile_angle = 10_800_000 - tile_angle;
                    }
                    if is_flip_y {
                        tile_angle = -tile_angle;
                    }
                    let tile_angle = tile_angle.rem_euclid(21_600_000);

                    let tile_subpaths: Vec<Vec<Pt>> = subpaths
                        .iter()
                        .map(|s| {
                            let s = clip_half(s, |p| p.0 - x0);
                            let s = clip_half(&s, |p| x1 - p.0);
                            let s = clip_half(&s, |p| p.1 - y0);
                            clip_half(&s, |p| y1 - p.1)
                        })
                        .filter(|s| s.len() >= 3)
                        .collect();
                    if tile_subpaths.is_empty() {
                        continue;
                    }

                    let a = (tile_angle as f32 / 60_000.0).to_radians();
                    let n = if *scaled && tw > 0.0 && th > 0.0 {
                        (a.cos() / tw, a.sin() / th)
                    } else {
                        (a.cos(), a.sin())
                    };
                    let t_proj = |p: Pt| n.0 * (p.0 - x0) + n.1 * (p.1 - y0);
                    let corners = [(0.0, 0.0), (tw, 0.0), (tw, th), (0.0, th)].map(|(px, py)| n.0 * px + n.1 * py);
                    let (t0, t1) = corners.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
                    let extent = if *scaled && tw > 0.0 && th > 0.0 {
                        tw.hypot(th) * px_per_unit
                    } else {
                        (t1 - t0) * px_per_unit
                    };
                    let bands = (extent.ceil() as usize).clamp(1, 256);
                    let step = (t1 - t0) / bands as f32;
                    let overlap = n.0.hypot(n.1) / px_per_unit.max(1e-6);
                    for i in 0..bands {
                        let color = color_at(stops, (i as f32 + 0.5) / bands as f32);
                        let overlap = if color.a >= 1.0 { overlap } else { 0.0 };
                        let lo = t0 + step * i as f32;
                        let hi = if i + 1 == bands { t1 + overlap } else { lo + step + overlap };
                        let pieces = tile_subpaths
                            .iter()
                            .map(|s| clip_half(&clip_half(s, |p| t_proj(p) - lo), |p| hi - t_proj(p)))
                            .collect();
                        paint(pieces, color);
                    }
                }
            }
        }
        Fill::Path { path, focus, stops } if !stops.is_empty() => {
            let [l, t, r, b] = focus.map(|v| v as f32 / 100_000.0);
            let inner = (l * w, t * h, w - r * w, h - b * h);
            let inner = (inner.0.min(inner.2), inner.1.min(inner.3), inner.0.max(inner.2), inner.1.max(inner.3));
            let centre = ((inner.0 + inner.2) / 2.0, (inner.1 + inner.3) / 2.0);
            let lerp = |a: f32, b: f32, s: f32| a + (b - a) * s;
            let extent = w.max(h) / 2.0 * px_per_unit * if *path == GradientPath::Circle { std::f32::consts::SQRT_2 } else { 1.0 };
            let bands = (extent.ceil() as usize).clamp(1, 256);
            // From the outside in: the band at `s` is what is inside the
            // outline `s` of the way from the focus to the edge.
            for i in (0..bands).rev() {
                let s = (i + 1) as f32 / bands as f32;
                let color = color_at_mode(stops, (i as f32 + 0.5) / bands as f32, powerpoint);
                let pieces: Vec<Vec<Pt>> = match path {
                    GradientPath::Shape => {
                        let scaled = |sub: &Vec<Pt>| -> Vec<Pt> { sub.iter().map(|&(x, y)| (centre.0 + (x - centre.0) * s, centre.1 + (y - centre.1) * s)).collect() };
                        // A hole (a part inside a bigger one, as a donut's) stays where it is, cut to the
                        // band's outline: scaled with the rest it would be filled with the bands.
                        let area = |p: &Vec<Pt>| -> f32 { p.iter().enumerate().map(|(i, a)| { let b = p[(i + 1) % p.len()]; a.0 * b.1 - b.0 * a.1 }).sum::<f32>().abs() / 2.0 };
                        let inside = |p: &Vec<Pt>, point: Pt| -> bool {
                            let mut inside = false;
                            for (i, a) in p.iter().enumerate() {
                                let b = p[(i + 1) % p.len()];
                                if (a.1 > point.1) != (b.1 > point.1) && point.0 < (b.0 - a.0) * (point.1 - a.1) / (b.1 - a.1) + a.0 {
                                    inside = !inside;
                                }
                            }
                            inside
                        };
                        subpaths
                            .iter()
                            .enumerate()
                            .map(|(j, sub)| {
                                let first = sub.first().copied().unwrap_or((0.0, 0.0));
                                let holder = subpaths.iter().enumerate().find(|(i, other)| *i != j && other.len() >= 3 && area(other) > area(sub) && inside(other, first));
                                match holder {
                                    Some((_, outer)) if sub.len() >= 3 => clip_convex(sub, &scaled(outer)),
                                    _ => scaled(sub),
                                }
                            })
                            .collect()
                    }
                    GradientPath::Rect => {
                        let rect = [
                            (lerp(inner.0, 0.0, s), lerp(inner.1, 0.0, s)),
                            (lerp(inner.2, w, s), lerp(inner.1, 0.0, s)),
                            (lerp(inner.2, w, s), lerp(inner.3, h, s)),
                            (lerp(inner.0, 0.0, s), lerp(inner.3, h, s)),
                        ];
                        subpaths.iter().map(|sub| clip_convex(sub, &rect)).collect()
                    }
                    GradientPath::Circle => {
                        // The last stop is on the circle through the box's
                        // corners (measured: round, not an ellipse, in a
                        // box wider than it is tall).
                        let c = (lerp(centre.0, w / 2.0, s), lerp(centre.1, h / 2.0, s));
                        let r = lerp((inner.2 - inner.0).hypot(inner.3 - inner.1) / 2.0, w.hypot(h) / 2.0, s);
                        let (rx, ry) = (r, r);
                        let ellipse: Vec<Pt> = (0..72)
                            .map(|j| {
                                let a = j as f32 / 72.0 * std::f32::consts::TAU;
                                (c.0 + rx * a.cos(), c.1 + ry * a.sin())
                            })
                            .collect();
                        subpaths.iter().map(|sub| clip_convex(sub, &ellipse)).collect()
                    }
                };
                paint(pieces, color);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{DocColor, GradientStop, clip_convex, clip_half, color_at};
    use gpui_kit::Rgba;
    use ibis_doc_model::Rgb;

    #[test]
    fn colours_mix_as_powerpoint_mixes_them() {
        let stop = |position, rgb| GradientStop { position, color: DocColor::opaque(Rgb(rgb)) };
        let stops = [stop(0, 0xFFC000), stop(100_000, 0x4472C4)];
        let mid: Rgba = color_at(&stops, 0.5).into();
        // PowerPoint paints (206, 173, 143) there; not (162, 153, 98).
        assert!((mid.r * 255.0 - 208.0).abs() < 2.0 && (mid.g * 255.0 - 172.5).abs() < 2.0 && (mid.b * 255.0 - 147.0).abs() < 2.0, "{mid:?}");
        // With three stops, the plain mix.
        let three = [stop(0, 0xFF0000), stop(50_000, 0xFFFF00), stop(100_000, 0x0070C0)];
        let c: Rgba = color_at(&three, 0.75).into();
        assert!((c.r * 255.0 - 127.5).abs() < 1.0 && (c.g * 255.0 - 183.5).abs() < 1.0, "{c:?}");
    }

    #[test]
    fn a_square_cut_to_a_band() {
        let square = vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        let band = clip_half(&clip_half(&square, |p| p.0 - 2.0), |p| 5.0 - p.0);
        let xs: Vec<f32> = band.iter().map(|p| p.0).collect();
        assert!(xs.iter().all(|x| (2.0..=5.0).contains(x)), "{band:?}");
        let diamond = [(5.0, 0.0), (10.0, 5.0), (5.0, 10.0), (0.0, 5.0)];
        let cut = clip_convex(&square, &diamond);
        let area: f32 = cut.iter().enumerate().map(|(i, a)| { let b = cut[(i + 1) % cut.len()]; a.0 * b.1 - b.0 * a.1 }).sum::<f32>().abs() / 2.0;
        assert!((area - 50.0).abs() < 0.01, "{cut:?}");
    }
}
