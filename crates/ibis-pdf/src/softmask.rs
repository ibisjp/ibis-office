// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Soft masks (PDF 32000-1 11.6.5.2): the mask group is drawn into a grey picture of the page, and what the mask covers is multiplied by it.
//!
//! The display list has no mask command, so the paths and pictures a mask covers are baked into pictures (`DrawImage`) whose alpha has the mask's value
//! at each pixel. What cannot be baked (glyph runs, strokes) is left as it was.

use hayro_interpret::{MaskType, SoftMask};
use kurbo::{BezPath, Point, Rect, Shape};

use crate::scene::{DisplayCmd, FillRule, PathCmd, PdfRect};

/// Pixels of the mask picture of a page at most, and the most pixels to a point.
const MASK_PIXELS: f64 = 1_000_000.0;
const MASK_PER_POINT: f64 = 2.0;
/// The same limits for a baked picture of a path.
const BAKE_PIXELS: f64 = 250_000.0;
const BAKE_PER_POINT: f64 = 4.0;

/// The value (0 to 1) of a mask at each pixel of the page.
pub struct MaskRaster {
    x0: f64,
    y0: f64,
    per_point: f64,
    width: usize,
    height: usize,
    values: Vec<f32>,
    /// The value outside the picture (the mask's background).
    outside: f32,
}

impl MaskRaster {
    pub fn at(&self, x: f64, y: f64) -> f32 {
        let px = ((x - self.x0) * self.per_point).floor();
        let py = ((y - self.y0) * self.per_point).floor();
        if px < 0.0 || py < 0.0 || px >= self.width as f64 || py >= self.height as f64 {
            return self.outside;
        }
        self.values[py as usize * self.width + px as usize]
    }
}

pub fn to_bezpath(commands: &[PathCmd]) -> BezPath {
    let mut path = BezPath::new();
    for cmd in commands {
        match *cmd {
            PathCmd::MoveTo(x, y) => path.move_to((x as f64, y as f64)),
            PathCmd::LineTo(x, y) => path.line_to((x as f64, y as f64)),
            PathCmd::QuadTo(cx, cy, x, y) => path.quad_to((cx as f64, cy as f64), (x as f64, y as f64)),
            PathCmd::CubicTo(a, b, c, d, x, y) => path.curve_to((a as f64, b as f64), (c as f64, d as f64), (x as f64, y as f64)),
            PathCmd::Close => path.close_path(),
        }
    }
    path
}

fn covered(path: &BezPath, rule: FillRule, p: Point) -> bool {
    let w = path.winding(p);
    match rule {
        FillRule::NonZero => w != 0,
        FillRule::EvenOdd => w % 2 != 0,
    }
}

fn intersect(a: Rect, b: Rect) -> Rect {
    let (x0, y0) = (a.x0.max(b.x0), a.y0.max(b.y0));
    Rect::new(x0, y0, a.x1.min(b.x1).max(x0), a.y1.min(b.y1).max(y0))
}

fn rect_of(r: PdfRect) -> Rect {
    Rect::new(r.x0 as f64, r.y0 as f64, r.x1 as f64, r.y1 as f64)
}

/// The mask `mask` as a picture of `page`, from the display list its group was drawn into. A luminosity mask is the luminosity (0.30 R + 0.59 G + 0.11 B) of
/// the group composited over the mask's background colour BC, an alpha mask is the group's alpha; the transfer function TR is applied after.
pub fn build(mask: &SoftMask<'_>, group: &[DisplayCmd], page: Rect) -> MaskRaster {
    let (w, h) = (page.width().max(1.0), page.height().max(1.0));
    let per_point = MASK_PER_POINT.min((MASK_PIXELS / (w * h)).sqrt());
    let width = ((w * per_point).ceil() as usize).max(1);
    let height = ((h * per_point).ceil() as usize).max(1);
    let luminosity = mask.mask_type() == MaskType::Luminosity;

    // Premultiplied colour and alpha of each pixel, starting from the backdrop.
    let backdrop = if luminosity {
        let bc = mask.background_color().to_rgba().to_rgba8();
        [bc[0] as f32 / 255.0, bc[1] as f32 / 255.0, bc[2] as f32 / 255.0, 1.0]
    } else {
        [0.0; 4]
    };
    let mut buffer = vec![backdrop; width * height];
    let mut paint = |x: usize, y: usize, rgba: [u8; 4], opacity: f32| {
        let a = rgba[3] as f32 / 255.0 * opacity;
        if a <= 0.0 {
            return;
        }
        let d = &mut buffer[y * width + x];
        for k in 0..3 {
            d[k] = rgba[k] as f32 / 255.0 * a + d[k] * (1.0 - a);
        }
        d[3] = a + d[3] * (1.0 - a);
    };

    let mut clips: Vec<Rect> = vec![page];
    let mut opacities: Vec<f32> = vec![1.0];
    for cmd in group {
        let clip = *clips.last().unwrap();
        let opacity = *opacities.last().unwrap();
        match cmd {
            DisplayCmd::PushClip(r) => clips.push(intersect(clip, rect_of(*r))),
            DisplayCmd::PopClip => {
                if clips.len() > 1 {
                    clips.pop();
                }
            }
            DisplayCmd::PushTransparencyGroup { opacity: o } => opacities.push(opacity * o),
            DisplayCmd::PopTransparencyGroup => {
                if opacities.len() > 1 {
                    opacities.pop();
                }
            }
            DisplayCmd::DrawRect { rect, fill_rgba: Some(rgba) } => {
                let area = intersect(clip, rect_of(*rect));
                fill_pixels(area, page, per_point, width, height, |_, _| true, |x, y| paint(x, y, *rgba, opacity));
            }
            DisplayCmd::DrawPath { bounds, commands, fill_rule, fill_rgba: Some(rgba), .. } => {
                let path = to_bezpath(commands);
                let area = intersect(clip, rect_of(*bounds));
                fill_pixels(area, page, per_point, width, height, |x, y| covered(&path, *fill_rule, Point::new(x, y)), |x, y| paint(x, y, *rgba, opacity));
            }
            DisplayCmd::DrawImage { bounds, width: iw, height: ih, rgba } if !rgba.is_empty() && rgba.len() == (*iw as usize) * (*ih as usize) * 4 => {
                let b = rect_of(*bounds);
                if b.width() <= 0.0 || b.height() <= 0.0 {
                    continue;
                }
                let area = intersect(clip, b);
                fill_pixels(area, page, per_point, width, height, |_, _| true, |x, y| {
                    let (dx, dy) = ((x as f64 + 0.5) / per_point + page.x0, (y as f64 + 0.5) / per_point + page.y0);
                    let ix = (((dx - b.x0) / b.width() * *iw as f64) as usize).min(*iw as usize - 1);
                    let iy = (((dy - b.y0) / b.height() * *ih as f64) as usize).min(*ih as usize - 1);
                    let at = (iy * *iw as usize + ix) * 4;
                    paint(x, y, [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]], opacity);
                });
            }
            _ => {}
        }
    }

    let transfer = |v: f32| mask.transfer_function().map_or(v, |f| f.apply(v.clamp(0.0, 1.0)));
    let value_of = |p: [f32; 4]| -> f32 {
        let v = if luminosity {
            // The group over BC is opaque: premultiplied equals straight.
            0.30 * p[0] + 0.59 * p[1] + 0.11 * p[2]
        } else {
            p[3]
        };
        transfer(v.clamp(0.0, 1.0))
    };
    let values: Vec<f32> = buffer.iter().map(|p| value_of(*p)).collect();
    MaskRaster { x0: page.x0, y0: page.y0, per_point, width, height, values, outside: value_of(backdrop) }
}

/// Visit the pixels of `area` within the mask picture of `page` whose centres `inside` accepts.
fn fill_pixels(area: Rect, page: Rect, per_point: f64, width: usize, height: usize, inside: impl Fn(f64, f64) -> bool, mut each: impl FnMut(usize, usize)) {
    if area.width() <= 0.0 || area.height() <= 0.0 {
        return;
    }
    let x_from = ((area.x0 - page.x0) * per_point).floor().max(0.0) as usize;
    let y_from = ((area.y0 - page.y0) * per_point).floor().max(0.0) as usize;
    let x_to = (((area.x1 - page.x0) * per_point).ceil().max(0.0) as usize).min(width);
    let y_to = (((area.y1 - page.y0) * per_point).ceil().max(0.0) as usize).min(height);
    for y in y_from..y_to {
        for x in x_from..x_to {
            let (cx, cy) = ((x as f64 + 0.5) / per_point + page.x0, (y as f64 + 0.5) / per_point + page.y0);
            if cx < area.x0 || cx > area.x1 || cy < area.y0 || cy > area.y1 {
                continue;
            }
            if inside(cx, cy) {
                each(x, y);
            }
        }
    }
}

/// The commands with the mask applied: a flat fill of a path or a rectangle becomes a picture whose alpha is its coverage times the mask, a picture has its
/// alpha multiplied by the mask. Strokes, glyph runs and anything else stay as they were.
pub fn apply(commands: Vec<DisplayCmd>, mask: &MaskRaster) -> Vec<DisplayCmd> {
    let mut out = Vec::with_capacity(commands.len());
    for cmd in commands {
        match cmd {
            DisplayCmd::DrawRect { rect, fill_rgba: Some(rgba) } => {
                let path = rect_of(rect).to_path(0.1);
                match bake_flat(&path, FillRule::NonZero, rgba, mask) {
                    Some(image) => out.push(image),
                    None => out.push(DisplayCmd::DrawRect { rect, fill_rgba: Some(rgba) }),
                }
            }
            DisplayCmd::DrawPath { bounds, commands: path_commands, fill_rule, fill_rgba: Some(rgba), stroke_rgba: None, stroke_width } => {
                let path = to_bezpath(&path_commands);
                match bake_flat(&path, fill_rule, rgba, mask) {
                    Some(image) => out.push(image),
                    None => out.push(DisplayCmd::DrawPath { bounds, commands: path_commands, fill_rule, fill_rgba: Some(rgba), stroke_rgba: None, stroke_width }),
                }
            }
            DisplayCmd::DrawImage { bounds, width, height, mut rgba } if !rgba.is_empty() && rgba.len() == width as usize * height as usize * 4 => {
                let (bw, bh) = ((bounds.x1 - bounds.x0) as f64, (bounds.y1 - bounds.y0) as f64);
                for py in 0..height as usize {
                    for px in 0..width as usize {
                        let x = bounds.x0 as f64 + (px as f64 + 0.5) / width as f64 * bw;
                        let y = bounds.y0 as f64 + (py as f64 + 0.5) / height as f64 * bh;
                        let at = (py * width as usize + px) * 4 + 3;
                        rgba[at] = (rgba[at] as f32 * mask.at(x, y)).round() as u8;
                    }
                }
                out.push(DisplayCmd::DrawImage { bounds, width, height, rgba });
            }
            other => out.push(other),
        }
    }
    out
}

/// A path filled with `rgba` as a picture whose alpha is the coverage times the mask.
fn bake_flat(path: &BezPath, rule: FillRule, rgba: [u8; 4], mask: &MaskRaster) -> Option<DisplayCmd> {
    let bbox = path.bounding_box();
    let (w, h) = (bbox.width(), bbox.height());
    if !(w > 0.0 && h > 0.0) {
        return None;
    }
    let per_point = BAKE_PER_POINT.min((BAKE_PIXELS / (w * h)).sqrt()).min(1024.0 / w.max(h));
    let width = ((w * per_point).ceil() as u32).max(1);
    let height = ((h * per_point).ceil() as u32).max(1);
    let mut out = vec![0u8; width as usize * height as usize * 4];
    for py in 0..height {
        for px in 0..width {
            let p = Point::new(bbox.x0 + (px as f64 + 0.5) / per_point, bbox.y0 + (py as f64 + 0.5) / per_point);
            if !covered(path, rule, p) {
                continue;
            }
            let at = (py as usize * width as usize + px as usize) * 4;
            out[at..at + 3].copy_from_slice(&rgba[..3]);
            out[at + 3] = (rgba[3] as f32 * mask.at(p.x, p.y)).round() as u8;
        }
    }
    Some(DisplayCmd::DrawImage {
        bounds: PdfRect { x0: bbox.x0 as f32, y0: bbox.y0 as f32, x1: bbox.x1 as f32, y1: bbox.y1 as f32 },
        width,
        height,
        rgba: out,
    })
}
