// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! An SVG written as an Enhanced Metafile (EMF): its paths as GDI paths (filled and stroked), its text as the outlines of the
//! glyphs (usvg lays the text out with the fonts of the computer). The picture of a copy as a picture goes on the Windows
//! clipboard in this form (`CF_ENHMETAFILE`): Excel, Word and PowerPoint for Windows paste it as a vector picture.
//! One unit of the SVG is a pixel at 96 dpi, which is what the metafile's device is.

// Written to the clipboard on Windows only; the other platforms build it for the tests.
#![cfg_attr(not(windows), allow(dead_code))]

use usvg::tiny_skia_path;

struct Emf {
    bytes: Vec<u8>,
    records: u32,
    handles: u32,
}

impl Emf {
    fn record(&mut self, kind: u32, body: &[u8]) {
        debug_assert!(body.len() % 4 == 0);
        self.bytes.extend_from_slice(&kind.to_le_bytes());
        self.bytes.extend_from_slice(&(body.len() as u32 + 8).to_le_bytes());
        self.bytes.extend_from_slice(body);
        self.records += 1;
    }
    fn words(&mut self, kind: u32, words: &[u32]) {
        let body: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        self.record(kind, &body);
    }
    fn handle(&mut self) -> u32 {
        self.handles += 1;
        self.handles
    }
}

fn colorref(c: usvg::Color) -> u32 {
    u32::from(c.red) | (u32::from(c.green) << 8) | (u32::from(c.blue) << 16)
}

/// The points of a path (after `by`) as GDI drawing records inside `BEGINPATH` ... `ENDPATH`.
fn write_path(emf: &mut Emf, path: &tiny_skia_path::Path, by: tiny_skia_path::Transform, wrap: bool) {
    use tiny_skia_path::PathSegment;
    let map = |p: tiny_skia_path::Point| {
        let mut p = p;
        by.map_point(&mut p);
        (p.x, p.y)
    };
    let round = |v: f32| v.round() as i32 as u32;
    // POLYBEZIERTO: bounds (4 words), count, then the points.
    let bezier = |emf: &mut Emf, points: &[(f32, f32)]| {
        let mut words = vec![0, 0, 0, 0, points.len() as u32];
        for (x, y) in points {
            words.push(round(*x));
            words.push(round(*y));
        }
        let (min_x, max_x) = points.iter().fold((i32::MAX, i32::MIN), |(a, b), (x, _)| (a.min(*x as i32), b.max(*x as i32)));
        let (min_y, max_y) = points.iter().fold((i32::MAX, i32::MIN), |(a, b), (_, y)| (a.min(*y as i32), b.max(*y as i32)));
        words[0] = min_x as u32;
        words[1] = min_y as u32;
        words[2] = max_x as u32;
        words[3] = max_y as u32;
        emf.words(5, &words);
    };
    if wrap {
        emf.words(0x3B, &[]);
    }
    let mut current = (0.0f32, 0.0f32);
    for segment in path.segments() {
        match segment {
            PathSegment::MoveTo(p) => {
                let (x, y) = map(p);
                current = (x, y);
                emf.words(0x1B, &[round(x), round(y)]);
            }
            PathSegment::LineTo(p) => {
                let (x, y) = map(p);
                current = (x, y);
                emf.words(0x36, &[round(x), round(y)]);
            }
            PathSegment::QuadTo(c, p) => {
                // A quadratic curve as the cubic through the same points.
                let ((cx, cy), (x, y)) = (map(c), map(p));
                let (x0, y0) = current;
                bezier(emf, &[(x0 + 2.0 / 3.0 * (cx - x0), y0 + 2.0 / 3.0 * (cy - y0)), (x + 2.0 / 3.0 * (cx - x), y + 2.0 / 3.0 * (cy - y)), (x, y)]);
                current = (x, y);
            }
            PathSegment::CubicTo(c1, c2, p) => {
                let (a, b, e) = (map(c1), map(c2), map(p));
                bezier(emf, &[a, b, e]);
                current = e;
            }
            PathSegment::Close => emf.words(0x3D, &[]),
        }
    }
    if wrap {
        emf.words(0x3C, &[]);
    }
}

fn write_group(emf: &mut Emf, group: &usvg::Group, by: tiny_skia_path::Transform) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => write_nested(emf, g, by),
            usvg::Node::Text(t) => write_nested(emf, t.flattened(), by),
            usvg::Node::Image(_) => {}
            usvg::Node::Path(p) => {
                if !p.is_visible() {
                    continue;
                }
                let fill = p.fill().and_then(|f| if let usvg::Paint::Color(c) = f.paint() { Some((*c, f.rule())) } else { None });
                let stroke = p.stroke().and_then(|s| if let usvg::Paint::Color(c) = s.paint() { Some((*c, s)) } else { None });
                if fill.is_none() && stroke.is_none() {
                    continue;
                }
                // Brush and pen: solid, the pen as wide as the stroke after the transform's scale.
                let scale = (by.sx * by.sy - by.kx * by.ky).abs().sqrt();
                let brush = emf.handle();
                emf.words(0x27, &[brush, 0, fill.map_or(0, |f| colorref(f.0)), 0]);
                let pen = emf.handle();
                let (pen_style, pen_width, pen_color) = match stroke {
                    Some((c, s)) => (if s.dasharray().is_some() { 1 } else { 0 }, (s.width().get() * scale).round().max(1.0) as u32, colorref(c)),
                    None => (5, 0, 0),
                };
                emf.words(0x26, &[pen, pen_style, pen_width, 0, pen_color]);
                // Selected: the brush (a null one when there is no fill), then the pen.
                emf.words(0x25, &[if fill.is_some() { brush } else { 0x8000_0005 }]);
                emf.words(0x25, &[pen]);
                emf.words(0x13, &[if matches!(fill, Some((_, usvg::FillRule::EvenOdd))) { 1 } else { 2 }]);
                write_path(emf, p.data(), by, true);
                emf.words(
                    match (fill.is_some(), stroke.is_some()) {
                        (true, true) => 0x3F,
                        (true, false) => 0x3E,
                        _ => 0x40,
                    },
                    &[0, 0, 0, 0],
                );
                // The objects are deleted once drawn.
                emf.words(0x28, &[brush]);
                emf.words(0x28, &[pen]);
            }
        }
    }
}

fn write_nested(emf: &mut Emf, g: &usvg::Group, by: tiny_skia_path::Transform) {
    let inner = by.pre_concat(g.transform());
    if let Some(clip) = g.clip_path() {
        // The clip is the union of its paths, set inside a saved state that is restored after the group.
        emf.words(0x21, &[]);
        let mut paths = Vec::new();
        collect_clip(clip.root(), inner.pre_concat(clip.transform()), &mut paths);
        emf.words(0x3B, &[]);
        for (path, transform) in &paths {
            write_path(emf, path, *transform, false);
        }
        emf.words(0x3C, &[]);
        // RGN_AND: the clip is where the path is.
        emf.words(0x43, &[1]);
    }
    write_group(emf, g, inner);
    if g.clip_path().is_some() {
        emf.words(0x22, &[u32::MAX]);
    }
}

fn collect_clip(group: &usvg::Group, by: tiny_skia_path::Transform, out: &mut Vec<(tiny_skia_path::Path, tiny_skia_path::Transform)>) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => collect_clip(g, by.pre_concat(g.transform()), out),
            usvg::Node::Path(p) => out.push((p.data().clone(), by)),
            _ => {}
        }
    }
}

/// Logical units in an SVG unit: GDI's coordinates are whole numbers, and the device's size in millimetres too, so both are
/// given in hundredths of a pixel (the picture's size is the frame's, whatever the device).
const PRECISION: i32 = 100;

/// The EMF of `svg` (one SVG unit a pixel at 96 dpi), or `None` for an SVG that cannot be read or has no size.
pub(crate) fn svg_to_emf(svg: &str) -> Option<Vec<u8>> {
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(svg, &options).ok()?;
    let (width, height) = (tree.size().width().ceil() as i32, tree.size().height().ceil() as i32);
    if width < 1 || height < 1 {
        return None;
    }
    let mut emf = Emf { bytes: Vec::new(), records: 0, handles: 0 };
    write_group(&mut emf, tree.root(), tiny_skia_path::Transform::from_scale(PRECISION as f32, PRECISION as f32));
    emf.words(14, &[0, 16, 20]);
    // The header (EMR_HEADER, 100 bytes with its first extension): bounds and frame (0.01 mm) of the picture, a device of
    // `width` x `height` pixels as large as 96 dpi makes it.
    let mm100 = |px: i32| (f64::from(px) * 2540.0 / 96.0).round() as u32;
    let mut header = Vec::new();
    let mut put = |v: u32| header.extend_from_slice(&v.to_le_bytes());
    put(1);
    put(108);
    for v in [0, 0, (width * PRECISION - 1) as u32, (height * PRECISION - 1) as u32] {
        put(v);
    }
    for v in [0, 0, mm100(width), mm100(height)] {
        put(v);
    }
    put(0x464D_4520); // " EMF"
    put(0x0001_0000);
    put(108 + emf.bytes.len() as u32);
    put(emf.records + 1);
    // The handles (the first is the metafile's own), a reserved word, no description, no palette.
    put(emf.handles + 1);
    put(0);
    put(0);
    put(0);
    // The device in pixels and in millimetres (so the reader's scale is 96 dpi), no pixel format, no OpenGL, the device in
    // micrometres.
    put((width * PRECISION) as u32);
    put((height * PRECISION) as u32);
    put((f64::from(width * PRECISION) * 25.4 / 96.0).round().max(1.0) as u32);
    put((f64::from(height * PRECISION) * 25.4 / 96.0).round().max(1.0) as u32);
    put(0);
    put(0);
    put(0);
    put((f64::from(width) * 25400.0 / 96.0).round() as u32);
    put((f64::from(height) * 25400.0 / 96.0).round() as u32);
    debug_assert_eq!(header.len(), 108);
    header.extend_from_slice(&emf.bytes);
    Some(header)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The metafile of an SVG reads back (with ibisOffice's own EMF reader) as the same shapes.
    #[::core::prelude::v1::test]
    fn the_metafile_of_an_svg_reads_back() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40" viewBox="0 0 80 40"><rect width="80" height="40" fill="#ffffff"/><path d="M5 35 L40 5 L75 35 Z" fill="#2060c0" stroke="#102030" stroke-width="2"/><line x1="0" y1="20" x2="80" y2="20" stroke="#ff0000" stroke-width="1" stroke-dasharray="3 2"/></svg>"##;
        let emf = svg_to_emf(svg).expect("emf");
        assert!(emf.len() > 108 && emf[40..44] == *b" EMF", "a header");
        assert_eq!(u32::from_le_bytes(emf[48..52].try_into().unwrap()) as usize, emf.len(), "the size the header says");
        let back = ibis_emf::emf_to_svg(&emf).expect("read back");
        std::fs::create_dir_all("../../target/pdfpicture").unwrap();
        std::fs::write("../../target/pdfpicture/svg-emf-roundtrip.emf", &emf).unwrap();
        std::fs::write("../../target/pdfpicture/svg-emf-roundtrip.svg", &back).unwrap();
        assert!(back.matches("<path").count() >= 3 && back.to_lowercase().contains("2060c0"), "{back}");
        // The size the reader finds is the SVG's (80 x 40) within the millimetres' rounding.
        let width: f64 = back.split("width=\"").nth(1).and_then(|t| t.split('"').next()?.parse().ok()).expect("a width");
        let height: f64 = back.split("height=\"").nth(1).and_then(|t| t.split('"').next()?.parse().ok()).expect("a height");
        assert!((width / height - 2.0).abs() < 0.02, "{width} x {height}");
    }
}
