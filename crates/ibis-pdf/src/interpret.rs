// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Bridge from `hayro-interpret` (`Device` trait) to `ibis-pdf` [`PageScene`].

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use crate::softmask::MaskRaster;
use hayro_interpret::CacheKey;

use hayro_interpret::font::Glyph;
use hayro_interpret::pattern::Pattern;
use hayro_interpret::shading::ShadingType;
use hayro_interpret::hayro_cmap::BfString;
use hayro_interpret::hayro_syntax::Pdf;
use hayro_interpret::{
    BlendMode, ClipPath, Context, Device, FillRule as HayroFillRule, GlyphDrawMode, Image,
    ImageData, InterpreterCache, InterpreterSettings, LumaData, Paint, PathDrawMode, SoftMask,
    interpret_page,
};
use kurbo::{Affine, BezPath, PathEl, Rect, Shape};

use crate::container::PdfContainer;
use crate::scene::{
    CharBox, DisplayCmd, FillRule, PageInfo, PageScene, PageTextIndex, PathCmd, PdfRect,
    collect_pages,
};
use crate::{Error, Result};

/// Interpret page `page_index` (0-based) into a [`PageScene`] containing both
/// GPUI-ready display commands and a character-level [`PageTextIndex`].
pub fn interpret_page_scene(
    container: &mut PdfContainer,
    page_index: usize,
) -> Result<PageScene> {
    let pages = collect_pages(container)?;
    let info = pages
        .get(page_index)
        .cloned()
        .ok_or_else(|| Error::new(format!("page index {page_index} out of range")))?;

    let pdf = parse_for_interpretation(container)?;
    interpret_scene(&pdf, &InterpreterCache::new(), info, Purpose::View)
}

/// Visit each page for export, keeping glyph outlines only when Unicode is unavailable.
pub fn visit_export_scenes(
    container: &mut PdfContainer,
    mut visit: impl FnMut(PageScene) -> Result<()>,
) -> Result<()> {
    let pages = collect_pages(container)?;
    let pdf = parse_for_interpretation(container)?;
    let cache = InterpreterCache::new();
    for info in pages {
        visit(interpret_scene(&pdf, &cache, info, Purpose::Export)?)?;
    }
    Ok(())
}

/// Visit each page's characters only: nothing is drawn, no image decoded (the plain text export).
/// The scenes have an empty display list.
pub fn visit_text_scenes(
    container: &mut PdfContainer,
    mut visit: impl FnMut(PageScene) -> Result<()>,
) -> Result<()> {
    let pages = collect_pages(container)?;
    let pdf = parse_for_interpretation(container)?;
    let cache = InterpreterCache::new();
    for info in pages {
        visit(interpret_scene(&pdf, &cache, info, Purpose::Text)?)?;
    }
    Ok(())
}

/// What a scene is made for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Purpose {
    /// The viewer: every glyph drawn, each character's box reaching the next character (for selection).
    View,
    /// The Markdown export: the drawings, glyphs only where they have no Unicode; the boxes as wide as the font says.
    Export,
    /// The text export: the characters only.
    Text,
}

fn parse_for_interpretation(container: &PdfContainer) -> Result<Pdf> {
    Pdf::new(Arc::new(container.raw_bytes().to_vec()))
        .map_err(|error| Error::new(format!("hayro-syntax failed to parse PDF: {error:?}")))
}

fn interpret_scene<'a>(
    pdf: &'a Pdf,
    cache: &'a InterpreterCache<'a>,
    info: PageInfo,
    purpose: Purpose,
) -> Result<PageScene> {
    let page_index = info.index;
    let page = pdf
        .pages()
        .get(page_index)
        .ok_or_else(|| Error::new(format!("page {page_index} missing in hayro-syntax")))?;

    let settings = InterpreterSettings::default();
    let initial_transform = Affine::new(page.initial_transform(true).as_coeffs());
    let (pw, ph) = page.render_dimensions();
    let bbox = Rect::new(0.0, 0.0, pw as f64, ph as f64);

    let mut ctx = Context::new(initial_transform, bbox, cache, pdf.xref(), settings);
    let mut device = SceneDevice::new(info, purpose);
    device.page = bbox;
    interpret_page(page, &mut ctx, &mut device);
    Ok(device.finish())
}

struct SceneDevice {
    info: PageInfo,
    display_list: Vec<DisplayCmd>,
    text_index: PageTextIndex,
    op_counter: usize,
    purpose: Purpose,
    page: Rect,
    /// The soft mask in force for what is drawn next, as a picture of the page.
    active_mask: Option<Rc<MaskRaster>>,
    /// The soft mask of each open transparency group and where in the display list the group starts.
    group_masks: Vec<Option<(Rc<MaskRaster>, usize)>>,
    masks: HashMap<u128, Rc<MaskRaster>>,
    /// Set for the device a mask's own group is drawn into: no masks inside masks.
    in_mask: bool,
}

impl SceneDevice {
    fn new(info: PageInfo, purpose: Purpose) -> Self {
        Self {
            info,
            display_list: Vec::new(),
            text_index: PageTextIndex::default(),
            op_counter: 0,
            purpose,
            page: Rect::new(0.0, 0.0, 0.0, 0.0),
            active_mask: None,
            group_masks: Vec::new(),
            masks: HashMap::new(),
            in_mask: false,
        }
    }

    /// The soft mask as a picture of the page (drawn once for each mask and transform).
    fn raster_of<'a>(&mut self, mask: &SoftMask<'a>) -> Option<Rc<MaskRaster>> {
        if self.in_mask || self.purpose == Purpose::Text || self.page.width() <= 0.0 {
            return None;
        }
        let key = mask.cache_key();
        if let Some(raster) = self.masks.get(&key) {
            return Some(raster.clone());
        }
        let mut inner = SceneDevice::new(self.info.clone(), Purpose::View);
        inner.in_mask = true;
        inner.page = self.page;
        mask.interpret(&mut inner);
        let raster = Rc::new(crate::softmask::build(mask, &inner.display_list, self.page));
        self.masks.insert(key, raster.clone());
        Some(raster)
    }

    /// Apply the mask to the commands from `start` on.
    fn mask_from(&mut self, start: usize, mask: &MaskRaster) {
        if start >= self.display_list.len() {
            return;
        }
        let tail = self.display_list.split_off(start);
        self.display_list.extend(crate::softmask::apply(tail, mask));
    }

    fn finish(mut self) -> PageScene {
        // The viewer's selection wants each box to reach the next character (ibisOffice). The export reads the gaps
        // between characters as spaces and keeps the widths the font gives: a box widened over a word space hides it.
        if self.purpose == Purpose::View {
            self.text_index.fit_widths();
        }
        PageScene {
            info: self.info,
            display_list: self.display_list,
            text_index: self.text_index,
        }
    }

    fn push_bezpath(
        &mut self,
        path: &BezPath,
        transform: Affine,
        paint: &Paint<'_>,
        draw_mode: &PathDrawMode,
    ) {
        let transformed = transform * path;
        if let (Paint::Pattern(pattern), PathDrawMode::Fill(rule)) = (paint, draw_mode)
            && let Pattern::Shading(shading_pattern) = &**pattern
            && let Some(cmd) = crate::gradient::bake(&transformed, shading_pattern, matches!(rule, HayroFillRule::EvenOdd))
        {
            self.display_list.push(cmd);
            return;
        }
        let bounds = rect_to_pdf_rect(transformed.bounding_box());
        let commands: Vec<PathCmd> = transformed
            .elements()
            .iter()
            .map(|el| match el {
                PathEl::MoveTo(p) => PathCmd::MoveTo(p.x as f32, p.y as f32),
                PathEl::LineTo(p) => PathCmd::LineTo(p.x as f32, p.y as f32),
                PathEl::QuadTo(c, p) => {
                    PathCmd::QuadTo(c.x as f32, c.y as f32, p.x as f32, p.y as f32)
                }
                PathEl::CurveTo(c1, c2, p) => PathCmd::CubicTo(
                    c1.x as f32,
                    c1.y as f32,
                    c2.x as f32,
                    c2.y as f32,
                    p.x as f32,
                    p.y as f32,
                ),
                PathEl::ClosePath => PathCmd::Close,
            })
            .collect();

        if commands.is_empty() {
            return;
        }
        let rgba = paint_rgba(paint);
        let (fill_rule, fill_rgba, stroke_rgba, stroke_width) = match draw_mode {
            PathDrawMode::Fill(rule) => (
                match rule {
                    HayroFillRule::NonZero => FillRule::NonZero,
                    HayroFillRule::EvenOdd => FillRule::EvenOdd,
                },
                Some(rgba),
                None,
                0.0,
            ),
            PathDrawMode::Stroke(props) => (FillRule::NonZero, None, Some(rgba), props.line_width),
        };
        self.display_list.push(DisplayCmd::DrawPath {
            bounds,
            commands,
            fill_rule,
            fill_rgba,
            stroke_rgba,
            stroke_width,
        });
    }
}

fn rect_to_pdf_rect(r: Rect) -> PdfRect {
    PdfRect {
        x0: r.x0 as f32,
        y0: r.y0 as f32,
        x1: r.x1 as f32,
        y1: r.y1 as f32,
    }
}

fn paint_rgba(paint: &Paint<'_>) -> [u8; 4] {
    match paint {
        Paint::Color(c) => c.to_rgba().to_rgba8(),
        Paint::Pattern(pattern) => pattern_rgba(pattern),
    }
}

/// The colour a pattern is drawn in where the display list has only flat fills: an axial or radial shading (the gradients of Segoe UI Emoji's colour
/// glyphs that Krilla writes into an exported PDF) in the colour its function gives half way along its domain, anything else black as before. A flat
/// colour is an approximation of a gradient, but it keeps the picture in colour (a black silhouette before, K0066 e6 on Windows).
fn pattern_rgba(pattern: &Pattern<'_>) -> [u8; 4] {
    const BLACK: [u8; 4] = [0, 0, 0, 255];
    let Pattern::Shading(shading_pattern) = pattern else { return BLACK };
    let shading = &shading_pattern.shading;
    let ShadingType::RadialAxial { domain, function, .. } = &*shading.shading_type else { return BLACK };
    let middle = (domain[0] + domain[1]) / 2.0;
    let Some(components) = function.eval(&std::iter::once(middle).collect()) else { return BLACK };
    shading.color_space.to_rgba(&components, shading_pattern.opacity, false).to_rgba8()
}

impl<'a> Device<'a> for SceneDevice {
    fn set_soft_mask(&mut self, mask: Option<SoftMask<'a>>) {
        self.active_mask = mask.and_then(|m| self.raster_of(&m));
    }

    fn set_blend_mode(&mut self, _blend_mode: BlendMode) {}

    fn draw_path(
        &mut self,
        path: &BezPath,
        transform: Affine,
        paint: &Paint<'a>,
        draw_mode: &PathDrawMode,
    ) {
        self.op_counter += 1;
        if self.purpose != Purpose::Text {
            let start = self.display_list.len();
            self.push_bezpath(path, transform, paint, draw_mode);
            if let Some(mask) = self.active_mask.clone() {
                self.mask_from(start, &mask);
            }
        }
    }

    fn push_clip_path(&mut self, clip_path: &ClipPath) {
        if self.purpose == Purpose::Text {
            return;
        }
        let bounds = rect_to_pdf_rect(clip_path.path.bounding_box());
        self.display_list.push(DisplayCmd::PushClip(bounds));
    }

    fn push_transparency_group(
        &mut self,
        opacity: f32,
        mask: Option<SoftMask<'a>>,
        _blend_mode: BlendMode,
    ) {
        if self.purpose == Purpose::Text {
            return;
        }
        let raster = mask.and_then(|m| self.raster_of(&m));
        self.group_masks.push(raster.map(|r| (r, self.display_list.len() + 1)));
        self.display_list
            .push(DisplayCmd::PushTransparencyGroup { opacity });
    }

    fn draw_glyph(
        &mut self,
        glyph: &Glyph<'a>,
        transform: Affine,
        glyph_transform: Affine,
        paint: &Paint<'a>,
        draw_mode: &GlyphDrawMode,
    ) {
        self.op_counter += 1;
        let combined = transform * glyph_transform;
        let coeffs = combined.as_coeffs();

        // Always index text (including invisible OCR text layers) for search and selection.
        if let Some(unicode) = glyph.as_unicode() {
            let text = match unicode {
                BfString::Char(c) => c.to_string(),
                BfString::String(s) => s,
            };
            // The glyph's advance where the font gives it (in thousandths of the size), else what the character
            // usually takes: a full-width character one em, any other 0.55.
            let advance = match glyph {
                Glyph::Outline(outline) => outline.advance_width().filter(|w| *w > 0.0 && *w < 4000.0).map(|w| w / 1000.0),
                Glyph::Type3(_) => None,
            }
            .unwrap_or_else(|| text.chars().map(crate::scene::estimated_advance_em).sum());
            // The glyph's cell (its advance by one em, in the thousandths of glyph space) as the page draws it. A
            // glyph turned a quarter (a Latin word set sideways in vertical text) gets the box it takes on the page,
            // and its advance runs down the page: it reads as vertical text.
            let rect_pt = glyph_cell(combined, advance);
            let vertical = coeffs[1].abs() > coeffs[0].abs();
            for ch in text.chars() {
                self.text_index.chars.push(CharBox {
                    ch,
                    rect_pt,
                    vertical,
                    stream_op_index: self.op_counter,
                });
            }
            if self.purpose == Purpose::Export && !text.is_empty() && !text.contains('\u{fffd}') {
                return;
            }
        }
        if self.purpose == Purpose::Text {
            return;
        }

        // Emit vector outlines / Type3 instructions for GPUI rendering unless invisible.
        let path_mode = match draw_mode {
            GlyphDrawMode::Fill => Some(PathDrawMode::Fill(HayroFillRule::NonZero)),
            GlyphDrawMode::Stroke(props) => Some(PathDrawMode::Stroke(props.clone())),
            GlyphDrawMode::Invisible => None,
        };
        if let Some(mode) = path_mode {
            match glyph {
                Glyph::Outline(outline_glyph) => {
                    let outline = outline_glyph.outline();
                    self.push_bezpath(&outline, combined, paint, &mode);
                }
                Glyph::Type3(t3) => {
                    t3.interpret(self, transform, glyph_transform, paint);
                }
            }
        }
    }

    fn draw_image(&mut self, image: Image<'a, '_>, transform: Affine) {
        let start = self.display_list.len();
        self.op_counter += 1;
        if self.purpose == Purpose::Text {
            return;
        }
        match image {
            Image::Stencil(s) => {
                let unit = Rect::new(0.0, 0.0, s.width() as f64, s.height() as f64);
                let bounds = rect_to_pdf_rect(transform.transform_rect_bbox(unit));
                self.display_list.push(DisplayCmd::DrawImage {
                    bounds,
                    width: s.width(),
                    height: s.height(),
                    rgba: Vec::new(),
                });
            }
            Image::Raster(r) => {
                let mut decoded_out = None;
                r.with_rgba(
                    |img, alpha| {
                        decoded_out = Some(image_to_rgba(img, alpha));
                    },
                    None,
                );
                if let Some((width, height, rgba)) = decoded_out {
                    let unit = Rect::new(0.0, 0.0, width as f64, height as f64);
                    let bounds = rect_to_pdf_rect(transform.transform_rect_bbox(unit));
                    // PDF 32000 8.9.5.3: `/Interpolate` (inline image: `/I`) false (the default) means no smoothing when the image is enlarged.
                    let dict = r.stream().dict();
                    let interpolate = dict
                        .get::<bool>(b"Interpolate".as_slice())
                        .or_else(|| dict.get::<bool>(b"I".as_slice()))
                        .unwrap_or(false);
                    let (width, height, rgba) = if interpolate {
                        (width, height, rgba)
                    } else {
                        enlarge_nearest(width, height, rgba, &bounds)
                    };
                    self.display_list.push(DisplayCmd::DrawImage {
                        bounds,
                        width,
                        height,
                        rgba,
                    });
                }
            }
        }
        if let Some(mask) = self.active_mask.clone() {
            self.mask_from(start, &mask);
        }
    }

    fn pop_clip_path(&mut self) {
        if self.purpose != Purpose::Text {
            self.display_list.push(DisplayCmd::PopClip);
        }
    }

    fn pop_transparency_group(&mut self) {
        if self.purpose != Purpose::Text {
            if let Some(Some((mask, start))) = self.group_masks.pop() {
                self.mask_from(start, &mask);
            }
            self.display_list.push(DisplayCmd::PopTransparencyGroup);
        }
    }
}

/// The box on the page of a glyph cell `advance` ems wide and one em high, drawn by `combined` (glyph space, a
/// thousand units to the em, to page points).
fn glyph_cell(combined: Affine, advance: f32) -> PdfRect {
    let width = f64::from(advance) * 1000.0;
    let corners = [(0.0, 0.0), (width, 0.0), (0.0, 1000.0), (width, 1000.0)].map(|(x, y)| combined * kurbo::Point::new(x, y));
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in corners {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    PdfRect { x0: x0 as f32, y0: y0 as f32, x1: x1 as f32, y1: y1 as f32 }
}

/// Device pixels per point the enlargement of an image without `/Interpolate` is prepared for (216 dpi). The display list carries pixels, and the
/// shell's sampler is bilinear, so each source pixel is repeated a whole number of times: the blended band at a pixel edge is then at most
/// 1/factor of a source pixel wide. Shrinking keeps the smooth resampling (PDFium does the same: it only stops smoothing on enlargement).
const NEAREST_PREPARED_PX_PER_PT: f32 = 3.0;
/// The most pixels along a side after repeating (16 MB of RGBA at most per image).
const NEAREST_MAX_SIDE: u32 = 2048;

fn enlarge_nearest(width: u32, height: u32, rgba: Vec<u8>, bounds: &PdfRect) -> (u32, u32, Vec<u8>) {
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        return (width, height, rgba);
    }
    let factor = |source: u32, extent_pt: f32| -> u32 {
        let wanted = (extent_pt.abs() * NEAREST_PREPARED_PX_PER_PT / source as f32).ceil();
        let cap = (NEAREST_MAX_SIDE / source).max(1) as f32;
        wanted.min(cap).max(1.0) as u32
    };
    let kx = factor(width, bounds.x1 - bounds.x0);
    let ky = factor(height, bounds.y1 - bounds.y0);
    if kx == 1 && ky == 1 {
        return (width, height, rgba);
    }
    let (w, h) = (width as usize, height as usize);
    let (nw, nh) = (w * kx as usize, h * ky as usize);
    let mut out = Vec::with_capacity(nw * nh * 4);
    for y in 0..h {
        let start = out.len();
        for x in 0..w {
            let px = &rgba[(y * w + x) * 4..(y * w + x) * 4 + 4];
            for _ in 0..kx {
                out.extend_from_slice(px);
            }
        }
        let row = out[start..].to_vec();
        for _ in 1..ky {
            out.extend_from_slice(&row);
        }
    }
    (nw as u32, nh as u32, out)
}

fn image_to_rgba(img: ImageData, alpha: Option<LumaData>) -> (u32, u32, Vec<u8>) {
    match img {
        ImageData::Rgb(rgb) => {
            let count = (rgb.width as usize) * (rgb.height as usize);
            let mut rgba = Vec::with_capacity(count * 4);
            for i in 0..count {
                let base = i * 3;
                let a = alpha
                    .as_ref()
                    .and_then(|al| al.data.get(i).copied())
                    .unwrap_or(255);
                rgba.extend_from_slice(&[
                    rgb.data[base],
                    rgb.data[base + 1],
                    rgb.data[base + 2],
                    a,
                ]);
            }
            (rgb.width, rgb.height, rgba)
        }
        ImageData::Luma(luma) => {
            let count = (luma.width as usize) * (luma.height as usize);
            let mut rgba = Vec::with_capacity(count * 4);
            for i in 0..count {
                let v = luma.data[i];
                let a = alpha
                    .as_ref()
                    .and_then(|al| al.data.get(i).copied())
                    .unwrap_or(255);
                rgba.extend_from_slice(&[v, v, v, a]);
            }
            (luma.width, luma.height, rgba)
        }
    }
}
