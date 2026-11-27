// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Rasterize a clipped drawing locally when the viewer's rectangular mask
//! cannot represent the DocuWorks path.

use ibis_pdf::{
    DisplayCmd, PdfRect,
    scene::{FillRule, PathCmd},
};
use tiny_skia::{IntSize, Mask, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

use crate::{Error, Result};

fn path(commands: &[PathCmd]) -> Result<Path> {
    let mut path = PathBuilder::new();
    for command in commands {
        match *command {
            PathCmd::MoveTo(x, y) => path.move_to(x, y),
            PathCmd::LineTo(x, y) => path.line_to(x, y),
            PathCmd::QuadTo(a, b, x, y) => path.quad_to(a, b, x, y),
            PathCmd::CubicTo(a, b, c, d, x, y) => path.cubic_to(a, b, c, d, x, y),
            PathCmd::Close => path.close(),
        }
    }
    path.finish().ok_or(Error::Invalid)
}

pub(crate) fn apply(
    command: DisplayCmd,
    clip: &[PathCmd],
    bounds: PdfRect,
    budget: &mut u64,
) -> Result<Option<DisplayCmd>> {
    let (draw, margin) = match &command {
        DisplayCmd::DrawPath {
            bounds,
            stroke_width,
            stroke_rgba,
            ..
        } => (
            *bounds,
            if stroke_rgba.is_some() {
                *stroke_width * 2.0
            } else {
                0.0
            },
        ),
        DisplayCmd::DrawImage { bounds, .. } => (*bounds, 0.0),
        DisplayCmd::SetClipRect(_) => return Ok(Some(command)),
        _ => return Err(Error::UnsupportedDrawing),
    };
    let bounds = PdfRect {
        x0: bounds.x0.max(draw.x0 - margin),
        y0: bounds.y0.max(draw.y0 - margin),
        x1: bounds.x1.min(draw.x1 + margin),
        y1: bounds.y1.min(draw.y1 + margin),
    };
    if bounds.x1 <= bounds.x0 || bounds.y1 <= bounds.y0 {
        return Ok(None);
    }
    const PIXELS_PER_POINT: f32 = 4.0;
    let width = ((bounds.x1 - bounds.x0) * PIXELS_PER_POINT).ceil() as u32;
    let height = ((bounds.y1 - bounds.y0) * PIXELS_PER_POINT).ceil() as u32;
    *budget = budget
        .checked_sub(u64::from(width) * u64::from(height))
        .ok_or(Error::Limit)?;
    let sx = width as f32 / (bounds.x1 - bounds.x0);
    let sy = height as f32 / (bounds.y1 - bounds.y0);
    let transform = Transform::from_row(sx, 0.0, 0.0, sy, -bounds.x0 * sx, -bounds.y0 * sy);
    let mut mask = Mask::new(width, height).ok_or(Error::Limit)?;
    mask.fill_path(&path(clip)?, tiny_skia::FillRule::Winding, true, transform);
    let mut result = Pixmap::new(width, height).ok_or(Error::Limit)?;
    match command {
        DisplayCmd::DrawPath {
            commands,
            fill_rule,
            fill_rgba,
            stroke_rgba,
            stroke_width,
            ..
        } => {
            let path = path(&commands)?;
            let mut paint = Paint::default();
            if let Some([r, g, b, a]) = fill_rgba {
                paint.set_color_rgba8(r, g, b, a);
                let rule = match fill_rule {
                    FillRule::NonZero => tiny_skia::FillRule::Winding,
                    FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
                };
                result.fill_path(&path, &paint, rule, transform, Some(&mask));
            }
            if let Some([r, g, b, a]) = stroke_rgba {
                paint.set_color_rgba8(r, g, b, a);
                result.stroke_path(
                    &path,
                    &paint,
                    &Stroke {
                        width: stroke_width,
                        ..Default::default()
                    },
                    transform,
                    Some(&mask),
                );
            }
        }
        DisplayCmd::DrawImage {
            width,
            height,
            mut rgba,
            ..
        } => {
            for px in rgba.chunks_exact_mut(4) {
                let alpha = u16::from(px[3]);
                for channel in &mut px[..3] {
                    *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
                }
            }
            let source =
                Pixmap::from_vec(rgba, IntSize::from_wh(width, height).ok_or(Error::Invalid)?)
                    .ok_or(Error::Invalid)?;
            let transform = Transform::from_row(
                (draw.x1 - draw.x0) * sx / width as f32,
                0.0,
                0.0,
                (draw.y1 - draw.y0) * sy / height as f32,
                (draw.x0 - bounds.x0) * sx,
                (draw.y0 - bounds.y0) * sy,
            );
            result.draw_pixmap(
                0,
                0,
                source.as_ref(),
                &PixmapPaint::default(),
                transform,
                Some(&mask),
            );
        }
        _ => unreachable!(),
    }
    let mut rgba = result.take();
    for px in rgba.chunks_exact_mut(4) {
        let alpha = u16::from(px[3]);
        if alpha == 0 {
            continue;
        }
        for channel in &mut px[..3] {
            *channel = ((u16::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
    Ok(Some(DisplayCmd::DrawImage {
        bounds,
        width,
        height,
        rgba,
    }))
}

pub(crate) fn tile_fill(
    command: DisplayCmd,
    tile: &image::RgbaImage,
    origin: (f32, f32),
    unit: (f32, f32),
    clip: PdfRect,
    budget: &mut u64,
) -> Result<Option<DisplayCmd>> {
    let DisplayCmd::DrawPath {
        bounds,
        commands,
        fill_rule,
        fill_rgba: Some(fill),
        stroke_rgba: None,
        ..
    } = command
    else {
        return Err(Error::UnsupportedDrawing);
    };
    let bounds = PdfRect {
        x0: bounds.x0.max(clip.x0),
        y0: bounds.y0.max(clip.y0),
        x1: bounds.x1.min(clip.x1),
        y1: bounds.y1.min(clip.y1),
    };
    if bounds.x1 <= bounds.x0 || bounds.y1 <= bounds.y0 {
        return Ok(None);
    }
    let width = ((bounds.x1 - bounds.x0) / unit.0).ceil() as u32;
    let height = ((bounds.y1 - bounds.y0) / unit.1).ceil() as u32;
    let count = u64::from(width) * u64::from(height);
    *budget = budget.checked_sub(count).ok_or(Error::Limit)?;
    let sx = width as f32 / (bounds.x1 - bounds.x0);
    let sy = height as f32 / (bounds.y1 - bounds.y0);
    let transform = Transform::from_row(sx, 0.0, 0.0, sy, -bounds.x0 * sx, -bounds.y0 * sy);
    let mut mask = Mask::new(width, height).ok_or(Error::Limit)?;
    let rule = match fill_rule {
        FillRule::NonZero => tiny_skia::FillRule::Winding,
        FillRule::EvenOdd => tiny_skia::FillRule::EvenOdd,
    };
    mask.fill_path(&path(&commands)?, rule, true, transform);
    let mut rgba = Vec::with_capacity(count as usize * 4);
    for y in 0..height {
        let ty = ((((y as f32 + 0.5) / sy + bounds.y0 - origin.1) / unit.1).floor() as i32)
            .rem_euclid(tile.height() as i32) as u32;
        for x in 0..width {
            let tx = ((((x as f32 + 0.5) / sx + bounds.x0 - origin.0) / unit.0).floor() as i32)
                .rem_euclid(tile.width() as i32) as u32;
            let [r, g, b, a] = tile.get_pixel(tx, ty).0;
            let coverage = u32::from(mask.data()[y as usize * width as usize + x as usize]);
            let alpha = (u32::from(a) * u32::from(fill[3]) * coverage + 32_512) / 65_025;
            rgba.extend_from_slice(&[r, g, b, alpha as u8]);
        }
    }
    Ok(Some(DisplayCmd::DrawImage {
        bounds,
        width,
        height,
        rgba,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> DisplayCmd {
        DisplayCmd::DrawPath {
            bounds: PdfRect {
                x0: 0.0,
                y0: 0.0,
                x1: 4.0,
                y1: 4.0,
            },
            commands: vec![
                PathCmd::MoveTo(0.0, 0.0),
                PathCmd::LineTo(4.0, 0.0),
                PathCmd::LineTo(4.0, 4.0),
                PathCmd::LineTo(0.0, 4.0),
                PathCmd::Close,
            ],
            fill_rule: FillRule::NonZero,
            fill_rgba: Some([200, 50, 20, 255]),
            stroke_rgba: None,
            stroke_width: 0.0,
        }
    }

    #[test]
    fn clips_a_solid_fill_to_triangle_not_its_bounding_box() {
        let bounds = PdfRect {
            x0: 0.0,
            y0: 0.0,
            x1: 4.0,
            y1: 4.0,
        };
        let clip = [
            PathCmd::MoveTo(0.0, 0.0),
            PathCmd::LineTo(4.0, 0.0),
            PathCmd::LineTo(0.0, 4.0),
            PathCmd::Close,
        ];
        let mut budget = 256;
        let DisplayCmd::DrawImage {
            width,
            height,
            rgba,
            ..
        } = apply(square(), &clip, bounds, &mut budget)
            .unwrap()
            .unwrap()
        else {
            panic!("expected masked pixels");
        };
        assert_eq!((width, height, budget), (16, 16, 0));
        assert_eq!(&rgba[..4], &[200, 50, 20, 255]);
        assert_eq!(*rgba.last().unwrap(), 0);
    }

    #[test]
    fn repeats_pattern_in_device_units() {
        let bounds = PdfRect {
            x0: 0.0,
            y0: 0.0,
            x1: 4.0,
            y1: 4.0,
        };
        let tile =
            image::RgbaImage::from_raw(2, 1, vec![255, 255, 255, 255, 0, 0, 0, 255]).unwrap();
        let DisplayCmd::DrawImage { rgba, .. } =
            tile_fill(square(), &tile, (0.0, 0.0), (1.0, 1.0), bounds, &mut 16)
                .unwrap()
                .unwrap()
        else {
            panic!("expected pattern pixels");
        };
        assert_eq!(
            &rgba[..16],
            &[
                255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255, 0, 0, 0, 255
            ]
        );
    }
}
