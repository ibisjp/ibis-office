// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The blurred shadow of an outline (`a:outerShdw` with `blurRad`) of a shape that is not a plain box: the shape's coverage, blurred by a Gaussian,
//! painted as a picture.
//!
//! PowerPoint blurs the shadow's silhouette with a Gaussian whose deviation is a third of `blurRad` (measured on its PNG export of 6, 12 and 24 pt:
//! the shadow of a rectangle falls from 95 % to 5 % of its alpha over 2 x 1.64 deviations either side of the edge, the same as the box shadows
//! of `shape_view.rs`), then multiplies the shadow's colour alpha into the coverage. A stack of offset copies of the outline, as this module
//! replaces, adds the copies' alphas up unevenly (the shadow of a 60 % black came out 52 % where it should be 60 %) and steps at the edges.

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use gpui_kit::*;

/// A deviation of a mask pixel at most this many pixels: a shadow is smooth, so its picture is as coarse as keeps the blur at about this many of
/// its pixels (and the picture is stretched back up when painted).
const SIGMA_IN_MASK_PIXELS: f32 = 2.2;
/// Cells across the largest picture.
const MAX_CELLS: f32 = 1500.0;
/// Vertical samples per mask row when the outline is rasterised (the horizontal coverage of each is exact).
const SUBROWS: usize = 4;

/// One closed outline, as polygons (the holes by even-odd), in logical pixels.
pub(crate) type Polygons = Vec<Vec<(f32, f32)>>;

/// The quadrilaterals of a stroke `width` wide along a polyline (each segment square-ended, so the corners of a closed outline are filled).
pub(crate) fn stroke_quads(points: &[(f32, f32)], closed: bool, width: f32) -> Vec<Polygons> {
    let half = width / 2.0;
    let n = points.len();
    let segments = if closed { n } else { n.saturating_sub(1) };
    (0..segments)
        .filter_map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-6 {
                return None;
            }
            let (ux, uy) = (dx / len, dy / len);
            let (nx, ny) = (-uy * half, ux * half);
            let (a, b) = ((a.0 - ux * half, a.1 - uy * half), (b.0 + ux * half, b.1 + uy * half));
            Some(vec![vec![(a.0 + nx, a.1 + ny), (b.0 + nx, b.1 + ny), (b.0 - nx, b.1 - ny), (a.0 - nx, a.1 - ny)]])
        })
        .collect()
}

thread_local! {
    static PICTURES: RefCell<HashMap<u64, Arc<RenderImage>>> = RefCell::new(HashMap::new());
}

/// Paint the shadow of `outlines` (their union), blurred by a Gaussian of deviation `sigma` (logical pixels), in `color` (whose alpha is the shadow's).
pub(crate) fn paint(window: &mut Window, outlines: &[Polygons], sigma: f32, color: Hsla) {
    let rgba = color.to_rgb();
    if rgba.a <= 0.0 {
        return;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in outlines.iter().flatten().flatten() {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    if x0 >= x1 || y0 >= y1 || !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
        return;
    }
    let sigma = sigma.max(0.0);
    let margin = 3.0 * sigma + 1.0;
    let pitch = (sigma / SIGMA_IN_MASK_PIXELS).max(0.5).max((x1 - x0 + 2.0 * margin) / MAX_CELLS).max((y1 - y0 + 2.0 * margin) / MAX_CELLS);
    let (left, top) = ((x0 - margin).floor(), (y0 - margin).floor());
    let cols = ((x1 + margin - left) / pitch).ceil() as usize + 1;
    let rows = ((y1 + margin - top) / pitch).ceil() as usize + 1;

    // The picture depends on the outline relative to its corner, so a shadow that only moves is not drawn again.
    let mut hasher = DefaultHasher::new();
    let quantum = |v: f32| (v * 16.0).round() as i32;
    for (k, poly) in outlines.iter().flatten().enumerate() {
        (k, poly.len()).hash(&mut hasher);
        for &(x, y) in poly {
            (quantum(x - left), quantum(y - top)).hash(&mut hasher);
        }
    }
    (sigma.to_bits(), pitch.to_bits(), rgba.r.to_bits(), rgba.g.to_bits(), rgba.b.to_bits(), rgba.a.to_bits(), cols, rows).hash(&mut hasher);
    let key = hasher.finish();
    let picture = PICTURES.with(|cache| cache.borrow().get(&key).cloned());
    let picture = picture.or_else(|| {
        let coverage = blurred_coverage(outlines, left, top, pitch, cols, rows, sigma / pitch);
        let (r, g, b) = ((rgba.r * 255.0).round(), (rgba.g * 255.0).round(), (rgba.b * 255.0).round());
        let mut bytes = Vec::with_capacity(cols * rows * 4);
        for c in &coverage {
            let a = (c * rgba.a).clamp(0.0, 1.0);
            // Premultiplied, BGRA (what the picture painter takes).
            bytes.extend_from_slice(&[(b * a).round() as u8, (g * a).round() as u8, (r * a).round() as u8, (a * 255.0).round() as u8]);
        }
        let buffer = image::RgbaImage::from_raw(cols as u32, rows as u32, bytes)?;
        let picture = Arc::new(RenderImage::new([image::Frame::new(buffer)]));
        PICTURES.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() >= 128 {
                cache.clear();
            }
            cache.insert(key, picture.clone());
        });
        Some(picture)
    });
    if let Some(picture) = picture {
        let area = Bounds::new(point(px(left), px(top)), size(px(cols as f32 * pitch), px(rows as f32 * pitch)));
        let _ = window.paint_image(area, area, Corners::default(), picture, 0, false);
    }
}

/// The coverage (0 to 1) of the union of `outlines` on a grid of `cols` x `rows` cells of `pitch` pixels from (`left`, `top`), blurred by a Gaussian of
/// `deviation` cells.
fn blurred_coverage(outlines: &[Polygons], left: f32, top: f32, pitch: f32, cols: usize, rows: usize, deviation: f32) -> Vec<f32> {
    let mut mask = vec![0f32; cols * rows];
    let mut one = vec![0f32; cols * rows];
    for outline in outlines {
        let (mut ly0, mut ly1) = (f32::MAX, f32::MIN);
        for &(_, y) in outline.iter().flatten() {
            ly0 = ly0.min(y);
            ly1 = ly1.max(y);
        }
        let first = (((ly0 - top) / pitch).floor().max(0.0)) as usize;
        let last = ((((ly1 - top) / pitch).ceil()).max(0.0) as usize).min(rows.saturating_sub(1));
        if first > last {
            continue;
        }
        for row in &mut one[first * cols..(last + 1) * cols] {
            *row = 0.0;
        }
        let mut crossings: Vec<f32> = Vec::new();
        for j in first..=last {
            for s in 0..SUBROWS {
                let y = top + (j as f32 + (s as f32 + 0.5) / SUBROWS as f32) * pitch;
                crossings.clear();
                for poly in outline {
                    let n = poly.len();
                    for i in 0..n {
                        let (a, b) = (poly[i], poly[(i + 1) % n]);
                        if (a.1 <= y) != (b.1 <= y) {
                            crossings.push(a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0));
                        }
                    }
                }
                crossings.sort_by(|a, b| a.total_cmp(b));
                for pair in crossings.chunks_exact(2) {
                    let (xa, xb) = ((pair[0] - left) / pitch, (pair[1] - left) / pitch);
                    if xb <= 0.0 || xa >= cols as f32 {
                        continue;
                    }
                    let (xa, xb) = (xa.max(0.0), xb.min(cols as f32));
                    let (ia, ib) = (xa.floor() as usize, (xb.ceil() as usize).min(cols));
                    for i in ia..ib {
                        let overlap = (xb.min(i as f32 + 1.0) - xa.max(i as f32)).max(0.0);
                        one[j * cols + i] += overlap / SUBROWS as f32;
                    }
                }
            }
        }
        for (m, o) in mask[first * cols..(last + 1) * cols].iter_mut().zip(&one[first * cols..(last + 1) * cols]) {
            *m = m.max(o.min(1.0));
        }
    }
    if deviation > 0.3 {
        let reach = (3.0 * deviation).ceil() as isize;
        let kernel: Vec<f32> = (-reach..=reach).map(|k| (-((k * k) as f32) / (2.0 * deviation * deviation)).exp()).collect();
        let total: f32 = kernel.iter().sum();
        let kernel: Vec<f32> = kernel.iter().map(|k| k / total).collect();
        let mut across = vec![0f32; cols * rows];
        for j in 0..rows {
            for i in 0..cols as isize {
                let mut sum = 0.0;
                for (n, w) in kernel.iter().enumerate() {
                    let at = i + n as isize - reach;
                    if at >= 0 && at < cols as isize {
                        sum += w * mask[j * cols + at as usize];
                    }
                }
                across[j * cols + i as usize] = sum;
            }
        }
        for j in 0..rows as isize {
            for i in 0..cols {
                let mut sum = 0.0;
                for (n, w) in kernel.iter().enumerate() {
                    let at = j + n as isize - reach;
                    if at >= 0 && at < rows as isize {
                        sum += w * across[at as usize * cols + i];
                    }
                }
                mask[j as usize * cols + i] = sum;
            }
        }
    }
    mask
}
