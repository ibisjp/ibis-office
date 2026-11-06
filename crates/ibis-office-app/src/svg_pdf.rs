// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! An SVG written as a one-page vector PDF: its paths as paths, its text as the outlines of the glyphs (usvg lays the text out
//! with the fonts of the computer, as the screen does). The picture of a copy as a picture goes on the clipboard in this form,
//! beside its SVG and its bitmap: Office on the Mac takes a PDF from the pasteboard as a vector picture.

use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Transform};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, LineCap, LineJoin, Stroke, StrokeDash};
use krilla::surface::Surface;
use usvg::tiny_skia_path;

/// Points in a CSS pixel (72 per inch over 96 per inch).
const POINT_PER_PIXEL: f32 = 72.0 / 96.0;

/// The PDF of `svg`, a page as large as the SVG is (an SVG unit is a CSS pixel, 1/96 inch; a PDF's is a point, 1/72 inch), or
/// `None` for an SVG that cannot be read or has no size.
pub(crate) fn svg_to_pdf(svg: &str) -> Option<Vec<u8>> {
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(svg, &options).ok()?;
    let size = tree.size();
    let mut document = Document::new();
    {
        let mut page = document.start_page_with(PageSettings::from_wh(size.width() * POINT_PER_PIXEL, size.height() * POINT_PER_PIXEL)?);
        let mut surface = page.surface();
        surface.push_transform(&Transform::from_scale(POINT_PER_PIXEL, POINT_PER_PIXEL));
        draw_group(&mut surface, tree.root());
        surface.pop();
        surface.finish();
        page.finish();
    }
    document.finish().ok()
}

fn transform(t: tiny_skia_path::Transform) -> Transform {
    Transform::from_row(t.sx, t.ky, t.kx, t.sy, t.tx, t.ty)
}

/// `path` with `by` applied to its points, added to `out`.
fn add_path(out: &mut PathBuilder, path: &tiny_skia_path::Path, by: tiny_skia_path::Transform) {
    use tiny_skia_path::PathSegment;
    let map = |p: tiny_skia_path::Point| {
        let mut p = p;
        by.map_point(&mut p);
        (p.x, p.y)
    };
    for segment in path.segments() {
        match segment {
            PathSegment::MoveTo(p) => out.move_to(map(p).0, map(p).1),
            PathSegment::LineTo(p) => out.line_to(map(p).0, map(p).1),
            PathSegment::QuadTo(c, p) => out.quad_to(map(c).0, map(c).1, map(p).0, map(p).1),
            PathSegment::CubicTo(c1, c2, p) => out.cubic_to(map(c1).0, map(c1).1, map(c2).0, map(c2).1, map(p).0, map(p).1),
            PathSegment::Close => out.close(),
        }
    }
}

/// The outline of a clip path (its paths, in its own space) as one path.
fn clip_outline(group: &usvg::Group, by: tiny_skia_path::Transform, out: &mut PathBuilder) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => clip_outline(g, by.pre_concat(g.transform()), out),
            usvg::Node::Path(p) => add_path(out, p.data(), by),
            _ => {}
        }
    }
}

fn opacity(o: usvg::Opacity) -> NormalizedF32 {
    NormalizedF32::new(o.get()).unwrap_or(NormalizedF32::ONE)
}

fn color(paint: &usvg::Paint) -> Option<rgb::Color> {
    match paint {
        usvg::Paint::Color(c) => Some(rgb::Color::new(c.red, c.green, c.blue)),
        // Gradients and patterns are not in what the sheet's SVG draws.
        _ => None,
    }
}

fn draw_group(surface: &mut Surface, group: &usvg::Group) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => draw_nested(surface, g),
            usvg::Node::Path(p) => {
                if !p.is_visible() {
                    continue;
                }
                let mut builder = PathBuilder::new();
                add_path(&mut builder, p.data(), tiny_skia_path::Transform::identity());
                let Some(path) = builder.finish() else { continue };
                match p.fill().and_then(|f| color(f.paint()).map(|c| (c, f))) {
                    Some((paint, fill)) => surface.set_fill(Some(Fill {
                        paint: paint.into(),
                        opacity: opacity(fill.opacity()),
                        rule: if fill.rule() == usvg::FillRule::EvenOdd { FillRule::EvenOdd } else { FillRule::NonZero },
                    })),
                    None => surface.set_fill(None),
                }
                match p.stroke().and_then(|s| color(s.paint()).map(|c| (c, s))) {
                    Some((paint, stroke)) => surface.set_stroke(Some(Stroke {
                        paint: paint.into(),
                        width: stroke.width().get(),
                        opacity: opacity(stroke.opacity()),
                        line_cap: match stroke.linecap() {
                            usvg::LineCap::Butt => LineCap::Butt,
                            usvg::LineCap::Round => LineCap::Round,
                            usvg::LineCap::Square => LineCap::Square,
                        },
                        line_join: match stroke.linejoin() {
                            usvg::LineJoin::Round => LineJoin::Round,
                            usvg::LineJoin::Bevel => LineJoin::Bevel,
                            _ => LineJoin::Miter,
                        },
                        miter_limit: stroke.miterlimit().get(),
                        dash: stroke.dasharray().map(|d| StrokeDash { array: d.to_vec(), offset: stroke.dashoffset() }),
                    })),
                    None => surface.set_stroke(None),
                }
                surface.draw_path(&path);
            }
            usvg::Node::Text(t) => draw_nested(surface, t.flattened()),
            // A bitmap in the SVG is not drawn in the PDF (the sheet's SVG has none).
            usvg::Node::Image(_) => {}
        }
    }
}

/// A group inside another: its transform, clip and opacity, then what it holds.
fn draw_nested(surface: &mut Surface, g: &usvg::Group) {
    let mut pushed = 0;
    surface.push_transform(&transform(g.transform()));
    pushed += 1;
    if let Some(clip) = g.clip_path() {
        let mut outline = PathBuilder::new();
        clip_outline(clip.root(), clip.transform(), &mut outline);
        if let Some(path) = outline.finish() {
            surface.push_clip_path(&path, &FillRule::NonZero);
            pushed += 1;
        }
    }
    if g.opacity().get() < 1.0 {
        surface.push_opacity(opacity(g.opacity()));
        pushed += 1;
    }
    draw_group(surface, g);
    for _ in 0..pushed {
        surface.pop();
    }
}
