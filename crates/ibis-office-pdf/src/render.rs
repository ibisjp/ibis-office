// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GPUI drawing pass for [`ibis_pdf::PageScene`].

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::{
    App, Bounds, ContentMask, Font, FontStyle, FontWeight, Hsla, PathBuilder, Pixels, Point,
    Radians, RenderImage, Rgba, ScaledPixels, SharedString, TextAlign, TextRun,
    TransformationMatrix, Window, fill, point, px, size,
};
use ibis_pdf::scene::{DisplayCmd, PageScene, PathCmd, PdfRect};

fn rgba_to_hsla([r, g, b, a]: [u8; 4]) -> Hsla {
    Rgba {
        r: f32::from(r) / 255.0,
        g: f32::from(g) / 255.0,
        b: f32::from(b) / 255.0,
        a: f32::from(a) / 255.0,
    }
    .into()
}

fn map_pt(origin: Point<Pixels>, scale: f32, x: f32, y: f32) -> Point<Pixels> {
    point(origin.x + px(x * scale), origin.y + px(y * scale))
}

fn map_rect(origin: Point<Pixels>, scale: f32, r: PdfRect) -> Bounds<Pixels> {
    let x0 = r.x0.min(r.x1);
    let y0 = r.y0.min(r.y1);
    Bounds::new(
        map_pt(origin, scale, x0, y0),
        size(px(r.width() * scale), px(r.height() * scale)),
    )
}

fn build_gpui_path(
    commands: &[PathCmd],
    origin: Point<Pixels>,
    scale: f32,
    stroke_width: Option<f32>,
) -> Option<gpui_kit::Path<Pixels>> {
    let mut builder = match stroke_width {
        Some(w) => PathBuilder::stroke(px((w * scale).max(0.5))),
        None => PathBuilder::fill(),
    };
    let mut has_point = false;
    for &cmd in commands {
        match cmd {
            PathCmd::MoveTo(x, y) => {
                builder.move_to(map_pt(origin, scale, x, y));
                has_point = true;
            }
            PathCmd::LineTo(x, y) => {
                builder.line_to(map_pt(origin, scale, x, y));
                has_point = true;
            }
            PathCmd::QuadTo(cx, cy, x, y) => {
                builder.curve_to(map_pt(origin, scale, x, y), map_pt(origin, scale, cx, cy));
                has_point = true;
            }
            PathCmd::CubicTo(cx1, cy1, cx2, cy2, x, y) => {
                builder.cubic_bezier_to(
                    map_pt(origin, scale, x, y),
                    map_pt(origin, scale, cx1, cy1),
                    map_pt(origin, scale, cx2, cy2),
                );
                has_point = true;
            }
            PathCmd::Close => builder.close(),
        }
    }
    if !has_point {
        return None;
    }
    builder.build().ok()
}

fn rgba_to_render_image(width: u32, height: u32, rgba: &[u8]) -> Option<Arc<RenderImage>> {
    if width == 0 || height == 0 || rgba.len() != (width as usize) * (height as usize) * 4 {
        return None;
    }
    let mut bgra = Vec::with_capacity(rgba.len());
    for px_rgba in rgba.chunks_exact(4) {
        bgra.extend_from_slice(&[px_rgba[2], px_rgba[1], px_rgba[0], px_rgba[3]]);
    }
    let buffer = image::RgbaImage::from_raw(width, height, bgra)?;
    Some(Arc::new(RenderImage::new([image::Frame::new(buffer)])))
}

/// Paint a [`PageScene`] onto the GPUI `window` within `page_bounds`.
pub fn paint_page_scene(
    page_index: usize,
    scene: &PageScene,
    page_bounds: Bounds<Pixels>,
    scale: f32,
    image_cache: &mut HashMap<(usize, usize), Option<Arc<RenderImage>>>,
    window: &mut Window,
    cx: &mut App,
) {
    // White page paper background.
    window.paint_quad(fill(page_bounds, gpui_kit::white()));

    let origin = page_bounds.origin;
    let mut mask = None;
    let mut font_names = None;
    for (cmd_idx, cmd) in scene.display_list.iter().enumerate() {
        if let DisplayCmd::SetClipRect(rect) = cmd {
            mask = rect.map(|rect| ContentMask {
                bounds: map_rect(origin, scale, rect).intersect(&page_bounds),
            });
            continue;
        }
        window.with_content_mask(mask, |window| match cmd {
            DisplayCmd::DrawRect { rect, fill_rgba } => {
                if let Some(rgba) = fill_rgba {
                    window.paint_quad(fill(map_rect(origin, scale, *rect), rgba_to_hsla(*rgba)));
                }
            }
            DisplayCmd::DrawPath {
                commands,
                fill_rgba,
                stroke_rgba,
                stroke_width,
                ..
            } => {
                if let Some(rgba) = fill_rgba
                    && let Some(path) = build_gpui_path(commands, origin, scale, None)
                {
                    window.paint_path(path, rgba_to_hsla(*rgba));
                }
                if let Some(rgba) = stroke_rgba
                    && let Some(path) =
                        build_gpui_path(commands, origin, scale, Some(*stroke_width))
                {
                    window.paint_path(path, rgba_to_hsla(*rgba));
                }
            }
            DisplayCmd::DrawImage {
                bounds,
                width,
                height,
                rgba,
            } => {
                let img = image_cache
                    .entry((page_index, cmd_idx))
                    .or_insert_with(|| rgba_to_render_image(*width, *height, rgba))
                    .clone();
                if let Some(img) = img {
                    let dest = map_rect(origin, scale, *bounds);
                    let _ =
                        window.paint_image(dest, dest, gpui_kit::Corners::default(), img, 0, false);
                }
            }
            DisplayCmd::DrawGlyphRun {
                text,
                origin: (gx, gy),
                font_size,
                rgba,
            } => {
                if !text.is_empty() {
                    let size_px = (*font_size * scale).max(1.0);
                    let run = TextRun {
                        len: text.len(),
                        font: Font {
                            family: SharedString::from(".SystemUIFont"),
                            ..Default::default()
                        },
                        color: rgba_to_hsla(*rgba),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let shaped = window.text_system().shape_line(
                        SharedString::from(text.clone()),
                        px(size_px),
                        &[run],
                        None,
                    );
                    let pos = point(
                        origin.x + px(*gx * scale),
                        origin.y + px(*gy * scale) - shaped.ascent,
                    );
                    let _ = shaped.paint(
                        pos,
                        shaped.ascent + shaped.descent,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                }
            }
            DisplayCmd::DrawStyledGlyphRun {
                text,
                origin: (x, y),
                font_size,
                family,
                weight,
                italic,
                angle,
                horizontal_scale,
                max_width,
                rgba,
            } => {
                if text.is_empty() || *font_size <= 0.0 {
                    return;
                }
                let names = font_names.get_or_insert_with(|| window.text_system().all_font_names());
                let family = names
                    .iter()
                    .find(|name| name.eq_ignore_ascii_case(family))
                    .map(String::as_str)
                    .unwrap_or(".SystemUIFont");
                let color = rgba_to_hsla(*rgba);
                let font_size = px(*font_size * scale);
                let run = TextRun {
                    len: text.len(),
                    font: Font {
                        family: SharedString::from(family.to_owned()),
                        weight: FontWeight(f32::from(*weight).clamp(100.0, 900.0)),
                        style: if *italic {
                            FontStyle::Italic
                        } else {
                            FontStyle::Normal
                        },
                        ..Default::default()
                    },
                    color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(
                    SharedString::from(text.clone()),
                    font_size,
                    &[run],
                    None,
                );
                let mut sx = *horizontal_scale;
                if let Some(width) = max_width
                    && shaped.width > px(0.0)
                {
                    sx = sx.min(width * scale / f32::from(shaped.width));
                }
                let baseline = map_pt(origin, scale, *x, *y);
                let (sin, cos) = angle.sin_cos();
                let device_scale = window.scale_factor();
                for run in &shaped.runs {
                    for glyph in &run.glyphs {
                        let dx = f32::from(glyph.position.x) * sx;
                        let dy = f32::from(glyph.position.y);
                        let at = point(
                            baseline.x + px(dx * cos - dy * sin),
                            baseline.y + px(dx * sin + dy * cos),
                        );
                        if glyph.is_emoji {
                            let _ = window.paint_emoji(at, run.font_id, glyph.id, font_size);
                        } else if *angle == 0.0 && sx == 1.0 {
                            let _ = window.paint_glyph(at, run.font_id, glyph.id, font_size, color);
                        } else {
                            let pivot = point(
                                ScaledPixels(f32::from(at.x) * device_scale),
                                ScaledPixels(f32::from(at.y) * device_scale),
                            );
                            let transform = TransformationMatrix::unit()
                                .translate(pivot)
                                .rotate(Radians(*angle))
                                .scale(size(sx, 1.0))
                                .translate(point(
                                    ScaledPixels(-pivot.x.0),
                                    ScaledPixels(-pivot.y.0),
                                ));
                            let _ = window.paint_glyph_transformed(
                                at,
                                run.font_id,
                                glyph.id,
                                font_size,
                                color,
                                transform,
                            );
                        }
                    }
                }
            }
            DisplayCmd::SetClipRect(_)
            | DisplayCmd::PushClip(_)
            | DisplayCmd::PopClip
            | DisplayCmd::PushTransparencyGroup { .. }
            | DisplayCmd::PopTransparencyGroup => {}
        });
    }
}
