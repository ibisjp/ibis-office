// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Axial and radial shadings (PDF 32000-1 section 8.7.4.5.2 and 8.7.4.5.4, shading types 2 and 3) baked into a picture.
//!
//! The display list has flat fills and clips by a rectangle only, so a path filled with a gradient is drawn as a [`DisplayCmd::DrawImage`] of the path's
//! bounds: every pixel is the shading's colour at that point, and its alpha is how much of the pixel the path covers.

use hayro_interpret::pattern::ShadingPattern;
use hayro_interpret::shading::ShadingType;
use kurbo::{Affine, BezPath, Point, Shape};

use crate::scene::{DisplayCmd, PdfRect};

/// Pixels of a baked gradient at most, and the most pixels to a point.
const MAX_PIXELS: f64 = 250_000.0;
const MAX_PER_POINT: f64 = 4.0;
/// The samples of the shading's function along its domain.
const LUT: usize = 256;

/// The picture of `path` (in device space, as drawn) filled with the shading `sp`; `even_odd` is the fill rule. None if the shading is not an axial or a
/// radial one, or the function cannot be evaluated.
pub fn bake(path: &BezPath, sp: &ShadingPattern, even_odd: bool) -> Option<DisplayCmd> {
    let ShadingType::RadialAxial { coords, domain, function, extend, axial } = &*sp.shading.shading_type else {
        return None;
    };
    let bbox = path.bounding_box();
    let (w, h) = (bbox.width(), bbox.height());
    if !(w > 0.0 && h > 0.0) {
        return None;
    }
    let inverse = sp.matrix.determinant().abs().gt(&1e-12).then(|| sp.matrix.inverse())?;
    let per_point = MAX_PER_POINT.min((MAX_PIXELS / (w * h)).sqrt()).min(1024.0 / w.max(h));
    let width = ((w * per_point).ceil() as u32).max(1);
    let height = ((h * per_point).ceil() as u32).max(1);

    // The colour at each of LUT steps of the domain.
    let mut lut = Vec::with_capacity(LUT);
    for i in 0..LUT {
        let t = domain[0] + (domain[1] - domain[0]) * i as f32 / (LUT - 1) as f32;
        let values = function.eval(&std::iter::once(t).collect())?;
        lut.push(sp.shading.color_space.to_rgba(&values, sp.opacity, false).to_rgba8());
    }
    let background = sp.shading.background.as_ref().map(|b| sp.shading.color_space.to_rgba(b, sp.opacity, false).to_rgba8());
    let colour_at = |s: f64| lut[((s.clamp(0.0, 1.0) * (LUT - 1) as f64).round() as usize).min(LUT - 1)];

    let c = [coords[0] as f64, coords[1] as f64, coords[2] as f64, coords[3] as f64, coords[4] as f64, coords[5] as f64];
    // The position along the gradient (0 to 1) of a point in shading space, if the shading paints there.
    let position = |p: Point| -> Option<f64> {
        if *axial {
            let (dx, dy) = (c[2] - c[0], c[3] - c[1]);
            let len2 = dx * dx + dy * dy;
            if len2 == 0.0 {
                return None;
            }
            let s = ((p.x - c[0]) * dx + (p.y - c[1]) * dy) / len2;
            if (s < 0.0 && !extend[0]) || (s > 1.0 && !extend[1]) {
                return None;
            }
            Some(s)
        } else {
            // The circle of s has centre c0 + s (c1 - c0) and radius r0 + s (r1 - r0): the largest s with a radius >= 0 whose circle passes through p.
            let (cdx, cdy, dr) = (c[3] - c[0], c[4] - c[1], c[5] - c[2]);
            let (pdx, pdy) = (p.x - c[0], p.y - c[1]);
            let a = cdx * cdx + cdy * cdy - dr * dr;
            let b = pdx * cdx + pdy * cdy + c[2] * dr;
            let cc = pdx * pdx + pdy * pdy - c[2] * c[2];
            let mut candidates = [f64::NAN; 2];
            if a.abs() < 1e-12 {
                if b.abs() > 1e-12 {
                    candidates[0] = cc / (2.0 * b);
                }
            } else {
                let disc = b * b - a * cc;
                if disc >= 0.0 {
                    let root = disc.sqrt();
                    let (s1, s2) = ((b + root) / a, (b - root) / a);
                    candidates = if s1 >= s2 { [s1, s2] } else { [s2, s1] };
                }
            }
            candidates.into_iter().find(|s| {
                s.is_finite() && c[2] + s * dr >= 0.0 && !((*s < 0.0 && !extend[0]) || (*s > 1.0 && !extend[1]))
            })
        }
    };

    // 1 sample to a pixel, at its centre (the picture is drawn smoothed, and at 4 pixels to a point the edge is finer than the screen at 100%).
    const SUB: [f64; 1] = [0.5];
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    for py in 0..height {
        for px in 0..width {
            let mut covered = 0u32;
            let mut sum = [0f64; 4];
            for sy in SUB {
                for sx in SUB {
                    let device = Point::new(bbox.x0 + (px as f64 + sx) / per_point, bbox.y0 + (py as f64 + sy) / per_point);
                    let winding = path.winding(device);
                    if (even_odd && winding % 2 == 0) || (!even_odd && winding == 0) {
                        continue;
                    }
                    let colour = match position(inverse * device) {
                        Some(s) => Some(colour_at(s)),
                        None => background,
                    };
                    if let Some(colour) = colour {
                        covered += 1;
                        let a = colour[3] as f64;
                        for k in 0..3 {
                            sum[k] += colour[k] as f64 * a;
                        }
                        sum[3] += a;
                    }
                }
            }
            if covered == 0 || sum[3] == 0.0 {
                continue;
            }
            let at = (py as usize * width as usize + px as usize) * 4;
            for k in 0..3 {
                rgba[at + k] = (sum[k] / sum[3]).round() as u8;
            }
            rgba[at + 3] = (sum[3] / (SUB.len() * SUB.len()) as f64).round() as u8;
        }
    }
    let _: Affine = inverse;
    Some(DisplayCmd::DrawImage {
        bounds: PdfRect { x0: bbox.x0 as f32, y0: bbox.y0 as f32, x1: bbox.x1 as f32, y1: bbox.y1 as f32 },
        width,
        height,
        rgba,
    })
}
