// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Glyphs as outlines, for text painted with a gradient or a pattern
//! (`rich_text`): GPUI paints a glyph in one colour, so such text is
//! painted as the polygons of its glyphs instead.

/// Polygons (the glyph's contours, flattened) of glyph `glyph` of the font
/// in `data` (the `index`th of a collection) at `size` pixels, from its
/// origin on the baseline, y down. `None` when the outline cannot be read
/// (the web build reads the bytes of the fonts it carries, `font_data` of gpui-pre-wgpu, with skrifa as the desktop does).
#[cfg(any(all(any(target_os = "macos", target_os = "linux", windows), feature = "export"), target_family = "wasm"))]
pub(crate) fn glyph_polygons(data: &[u8], index: u32, glyph: u32, size: f32) -> Option<Vec<Vec<(f32, f32)>>> {
    use skrifa::MetadataProvider as _;
    use skrifa::instance::{LocationRef, Size};
    use skrifa::outline::DrawSettings;
    let font = skrifa::FontRef::from_index(data, index).ok()?;
    let outlines = font.outline_glyphs();
    let outline = outlines.get(skrifa::GlyphId::new(glyph))?;
    let mut pen = Flatten::default();
    outline.draw(DrawSettings::unhinted(Size::new(size), LocationRef::default()), &mut pen).ok()?;
    pen.close_contour();
    Some(pen.contours)
}

#[cfg(not(any(all(any(target_os = "macos", target_os = "linux", windows), feature = "export"), target_family = "wasm")))]
pub(crate) fn glyph_polygons(_data: &[u8], _index: u32, _glyph: u32, _size: f32) -> Option<Vec<Vec<(f32, f32)>>> {
    None
}

/// Collects a glyph's contours as polygons, curves cut into short lines,
/// y turned down.
#[cfg(any(all(any(target_os = "macos", target_os = "linux", windows), feature = "export"), target_family = "wasm"))]
#[derive(Default)]
struct Flatten {
    contours: Vec<Vec<(f32, f32)>>,
    current: Vec<(f32, f32)>,
}

#[cfg(any(all(any(target_os = "macos", target_os = "linux", windows), feature = "export"), target_family = "wasm"))]
impl Flatten {
    fn close_contour(&mut self) {
        if self.current.len() >= 3 {
            self.contours.push(std::mem::take(&mut self.current));
        } else {
            self.current.clear();
        }
    }

    fn last(&self) -> (f32, f32) {
        self.current.last().copied().unwrap_or((0.0, 0.0))
    }
}

/// Lines a curve is cut into.
#[cfg(any(all(any(target_os = "macos", target_os = "linux", windows), feature = "export"), target_family = "wasm"))]
const STEPS: usize = 8;

#[cfg(any(all(any(target_os = "macos", target_os = "linux", windows), feature = "export"), target_family = "wasm"))]
impl skrifa::outline::OutlinePen for Flatten {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close_contour();
        self.current.push((x, -y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.current.push((x, -y));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (x0, y0) = self.last();
        let (c, e) = ((cx0, -cy0), (x, -y));
        for i in 1..=STEPS {
            let t = i as f32 / STEPS as f32;
            let u = 1.0 - t;
            self.current.push((u * u * x0 + 2.0 * u * t * c.0 + t * t * e.0, u * u * y0 + 2.0 * u * t * c.1 + t * t * e.1));
        }
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (x0, y0) = self.last();
        let (a, b, e) = ((cx0, -cy0), (cx1, -cy1), (x, -y));
        for i in 1..=STEPS {
            let t = i as f32 / STEPS as f32;
            let u = 1.0 - t;
            let (k0, k1, k2, k3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.current.push((k0 * x0 + k1 * a.0 + k2 * b.0 + k3 * e.0, k0 * y0 + k1 * a.1 + k2 * b.1 + k3 * e.1));
        }
    }

    fn close(&mut self) {
        self.close_contour();
    }
}
