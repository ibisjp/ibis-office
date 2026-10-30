// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pages written as a vector PDF from what GPUI painted (its
//! `PaintRecord`s): text as text in the fonts it was drawn with (subset and
//! embedded, with the characters for copying and searching), shapes as
//! paths, pictures as images. A blurred shadow (a drop shadow, a soft edge, a glow) is a picture of its blur (as PowerPoint writes one: a bitmap
//! with an alpha mask, PDF having no blur); an inner shadow is left out.
//!
//! The records are in logical pixels from the page's top-left corner; each
//! page is scaled to its size in points.

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::{Background, BorderStyle, Bounds, Corners, FontId, Hsla, OutlineVerb, PaintRecord, Pixels, Rgba};
use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{Path, PathBuilder, Point, Size, Transform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, LineCap, LineJoin, LinearGradient, SpreadMethod, Stop, Stroke, StrokeDash};
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph};

/// The font data of a font GPUI drew with: the file's bytes and the font's
/// index in it.
pub type FontData = Option<(Arc<Vec<u8>>, u32)>;

/// The axis values (OpenType tag, value) a variable font is drawn at; empty for a font that is not variable.
pub type Variations = Vec<([u8; 4], f32)>;

/// How far a synthetic italic slants (x by this much of y), and the width of a synthetic bold's outline in ems: as the
/// screen draws them (the macOS text system).
const SYNTHETIC_SLANT: f32 = 0.2;
const SYNTHETIC_BOLD_STROKE: f32 = 0.035;

/// A page to write: its records, the window's width in logical pixels and
/// device pixels per logical one, and its size in points.
struct PendingPage {
    records: Vec<PaintRecord>,
    width_px: f32,
    page_pt: (f32, f32),
    scale_factor: f32,
    /// Points the whole page is moved by.
    shift: (f32, f32),
}

/// A PDF being written: pages are collected, then written together, once it
/// is known which glyphs of each font are used (a font that cannot be
/// subset to them is drawn as outlines).
pub struct PdfWriter {
    pages: Vec<PendingPage>,
    document: Document,
    /// Fonts by GPUI's id, with the characters of their glyphs; `None` for
    /// one whose data is not available (its text is left out).
    fonts: HashMap<FontId, Option<PdfFont>>,
    /// Images by the address of GPUI's image.
    images: HashMap<usize, Image>,
    /// The same pictures by what they hold (size and a hash of the pixels, then the pixels themselves compared in full, so a hash that happens to match never stands in for another picture): one GPUI image rebuilt for another slide is compressed once.
    images_by_content: HashMap<(u32, u32, u64), Vec<(Vec<u8>, Image)>>,
    /// Glyphs left out for want of their font.
    pub missing_glyphs: usize,
}

impl PdfWriter {
    pub fn new() -> Self {
        Self { pages: Vec::new(), document: Document::new(), fonts: HashMap::new(), images: HashMap::new(), images_by_content: HashMap::new(), missing_glyphs: 0 }
    }

    /// Add a page `page_pt` points large from `records`, painted in a window
    /// `width_px` logical pixels wide at `scale_factor` device pixels each.
    /// `font_data` gives a font's data by its id.
    /// The pages added so far.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn add_page(&mut self, records: Vec<PaintRecord>, width_px: f32, page_pt: (f32, f32), scale_factor: f32, font_data: &mut dyn FnMut(FontId) -> (FontData, (bool, bool), Variations)) {
        self.add_page_shifted(records, width_px, page_pt, scale_factor, (0.0, 0.0), font_data);
    }

    /// [`Self::add_page`], with everything on the page moved by `shift`
    /// points (to put what the window drew on whole pixels where the paper
    /// wants it between them).
    /// `font_data` also tells whether the font is drawn emboldened and slanted (bold or italic asked of a family without
    /// that face): it is written so too.
    pub fn add_page_shifted(&mut self, records: Vec<PaintRecord>, width_px: f32, page_pt: (f32, f32), scale_factor: f32, shift: (f32, f32), font_data: &mut dyn FnMut(FontId) -> (FontData, (bool, bool), Variations)) {
        for record in &records {
            if let PaintRecord::Glyph { font_id, glyph_id, .. } = record {
                let font = self.fonts.entry(*font_id).or_insert_with(|| {
                    let (data, synthetic, variations) = font_data(*font_id);
                    data.and_then(|(data, index)| {
                        let characters = Arc::new(glyph_characters(&data, index));
                        let glyph_count = glyph_count(&data, index);
                        let extent = vertical_extent(&data, index);
                        let advances = glyph_advances(&data, index, &variations);
                        let coords: Vec<(krilla::text::Tag, f32)> = variations.iter().map(|(tag, value)| (krilla::text::Tag::new(tag), *value)).collect();
                        let font = Font::new_variable(data.clone().into(), index, &coords)?;
                        Some(PdfFont { font, characters, data, index, glyph_count, extent, advances, glyphs: Vec::new(), outlined: false, synthetic })
                    })
                });
                if let Some(font) = font
                    && glyph_id.0 < font.glyph_count
                {
                    font.glyphs.push(glyph_id.0 as u16);
                }
            }
        }
        self.pages.push(PendingPage { records, width_px, page_pt, scale_factor, shift });
    }

    fn write_page(&mut self, pending: &PendingPage) {
        let PendingPage { records, width_px, page_pt, scale_factor, shift } = pending;
        let (records, width_px, page_pt, scale_factor, shift) = (records.as_slice(), *width_px, *page_pt, *scale_factor, *shift);
        let Some(settings) = PageSettings::from_wh(page_pt.0, page_pt.1) else { return };
        let k = page_pt.0 / width_px.max(1.0);
        let mut page = self.document.start_page_with(settings);
        let mut surface = page.surface();
        surface.push_transform(&Transform::from_translate(shift.0, shift.1));
        surface.push_transform(&Transform::from_scale(k, k));
        let mut next = 0;
        while let Some(record) = records.get(next) {
            next += 1;
            // What lies outside its clip is not drawn (a page of a sheet
            // records the charts beside it too).
            if !shows(record) {
                continue;
            }
            match record {
                PaintRecord::Quad { bounds, clip, background, border_color, border_widths, corner_radii, border_style } => {
                    with_clip(&mut surface, clip, within(bounds, clip), |s| {
                        let outer = rounded_rect(bounds, corner_radii);
                        if let (Some(path), Some(fill)) = (&outer, fill_of(background, bounds)) {
                            s.set_stroke(None);
                            s.set_fill(Some(fill));
                            s.draw_path(path);
                        }
                        let widths = [border_widths.top, border_widths.right, border_widths.bottom, border_widths.left].map(f32::from);
                        if widths.iter().all(|w| *w <= 0.0) || border_color.a <= 0.0 {
                            return;
                        }
                        if widths.iter().all(|w| (*w - widths[0]).abs() < 0.01) {
                            // An even border: a line along the middle of it.
                            let w = widths[0];
                            let inset = Bounds::from_corners(
                                gpui_kit::point(bounds.left() + gpui_kit::px(w / 2.0), bounds.top() + gpui_kit::px(w / 2.0)),
                                gpui_kit::point(bounds.right() - gpui_kit::px(w / 2.0), bounds.bottom() - gpui_kit::px(w / 2.0)),
                            );
                            let radii = shrink(corner_radii, w / 2.0);
                            if let Some(path) = rounded_rect(&inset, &radii) {
                                s.set_fill(None);
                                let (paint, opacity) = color(*border_color);
                                let dash = (*border_style == BorderStyle::Dashed).then(|| StrokeDash { array: vec![w * 3.0, w * 3.0], offset: 0.0 });
                                s.set_stroke(Some(Stroke { paint: paint.into(), width: w, opacity, dash, ..Stroke::default() }));
                                s.draw_path(&path);
                                s.set_stroke(None);
                            }
                        } else {
                            // Uneven: each side as a band.
                            let (l, t, r, b) = (f32::from(bounds.left()), f32::from(bounds.top()), f32::from(bounds.right()), f32::from(bounds.bottom()));
                            let bands = [(l, t, r, t + widths[0]), (r - widths[1], t, r, b), (l, b - widths[2], r, b), (l, t, l + widths[3], b)];
                            let (paint, opacity) = color(*border_color);
                            s.set_fill(Some(Fill { paint: paint.into(), opacity, rule: FillRule::NonZero }));
                            for (x0, y0, x1, y1) in bands {
                                if let Some(path) = rect_path(x0, y0, x1, y1) {
                                    s.draw_path(&path);
                                }
                            }
                        }
                    });
                }
                PaintRecord::Path { outline, triangles, color: background, clip } => {
                    let points: Vec<gpui_kit::Point<Pixels>> = match outline {
                        Some(outline) => outline.verbs.iter().flat_map(verb_points).collect(),
                        None => triangles.iter().flatten().copied().collect(),
                    };
                    let Some(bounds) = bounds_of(&points) else { continue };
                    let reach = outline.as_ref().and_then(|o| o.stroke).map_or(Pixels::ZERO, |s| s.width);
                    with_clip(&mut surface, clip, within(&bounds.dilate(reach), clip), |s| {
                        let path = match outline {
                            Some(outline) => outline_path(&outline.verbs),
                            None => triangles_path(triangles),
                        };
                        let Some(path) = path else { return };
                        let Some(fill) = fill_of(background, &bounds) else { return };
                        match outline.as_ref().and_then(|o| o.stroke) {
                            Some(stroke) => {
                                s.set_fill(None);
                                s.set_stroke(Some(Stroke {
                                    paint: fill.paint,
                                    width: f32::from(stroke.width),
                                    opacity: fill.opacity,
                                    line_cap: if stroke.round_caps { LineCap::Round } else { LineCap::Butt },
                                    line_join: if stroke.round_joins { LineJoin::Round } else { LineJoin::Miter },
                                    // A dashed line is one line with its pattern (`d`: [on off ...] phase), not a path of dashes.
                                    dash: stroke.dash.map(|d| StrokeDash { array: d.pattern().to_vec(), offset: d.phase }),
                                    ..Stroke::default()
                                }));
                                s.draw_path(&path);
                                s.set_stroke(None);
                            }
                            None => {
                                s.set_stroke(None);
                                s.set_fill(Some(fill));
                                s.draw_path(&path);
                            }
                        }
                    });
                }
                PaintRecord::Glyph { origin, font_id, glyph_id, font_size, color: glyph_color, transformation, clip, emoji } => {
                    let Some(Some(pdf_font)) = self.fonts.get(font_id) else {
                        self.missing_glyphs += 1;
                        continue;
                    };
                    // Not a glyph of the font: CoreText leaves 0xFFFF where
                    // a ligature took the characters, and it draws nothing.
                    if glyph_id.0 >= pdf_font.glyph_count {
                        continue;
                    }
                    let (font, characters, outlined, glyph_count, (ascent, descent), (bold, italic)) =
                        (pdf_font.font.clone(), pdf_font.characters.clone(), pdf_font.outlined, pdf_font.glyph_count, pdf_font.extent, pdf_font.synthetic);
                    // The glyphs after it on the same line, in the same
                    // font, size, color and clip, go in one run: each
                    // advances to where the next one is.
                    let size = f32::from(*font_size);
                    let mut run = vec![(*glyph_id, origin.x)];
                    if transformation.is_none() && !emoji {
                        while let Some(PaintRecord::Glyph { origin: o, font_id: f, glyph_id: g, font_size: z, color: c, transformation: None, clip: k, emoji: false }) = records.get(next)
                            && f == font_id && z == font_size && c == glyph_color && k == clip && o.y == origin.y
                            && o.x > run.last().map_or(o.x, |(_, x)| *x)
                            && g.0 < glyph_count
                        {
                            run.push((*g, o.x));
                            next += 1;
                        }
                    }
                    // Where the run's glyphs reach: from the first glyph's
                    // origin (half a pixel before) to past the last one's
                    // advance, over the font's ascent and descent.
                    let (last_id, last_x) = run.last().copied().unwrap_or((*glyph_id, origin.x));
                    let last_advance = pdf_font.advances.get(last_id.0 as usize).copied().unwrap_or(1.2);
                    let reach = Bounds::from_corners(
                        gpui_kit::point(origin.x - gpui_kit::px(0.5), origin.y - *font_size * ascent),
                        gpui_kit::point(last_x + *font_size * last_advance + gpui_kit::px(0.5), origin.y + *font_size * descent),
                    );
                    let inside = transformation.is_none() && !italic && within(&reach.dilate(*font_size * 0.05), clip);
                    with_clip(&mut surface, clip, inside, |s| {
                        if let Some(m) = transformation {
                            // In scaled pixels about the window's origin.
                            let [[a, b], [c, d]] = m.rotation_scale;
                            s.push_transform(&Transform::from_row(a, c, b, d, m.translation[0] / scale_factor, m.translation[1] / scale_factor));
                        }
                        let (paint, opacity) = color(*glyph_color);
                        // Slanted about the baseline: x moves left by a fifth of the height below it (y grows down).
                        if italic {
                            let y0 = f32::from(origin.y);
                            s.push_transform(&Transform::from_row(1.0, 0.0, -SYNTHETIC_SLANT, 1.0, SYNTHETIC_SLANT * y0, 0.0));
                        }
                        // Emboldened: outlined too, in the same colour.
                        let stroke = bold.then(|| Stroke { paint: paint.clone().into(), width: size * SYNTHETIC_BOLD_STROKE, opacity, line_join: LineJoin::Round, ..Stroke::default() });
                        s.set_stroke(stroke);
                        s.set_fill(Some(Fill { paint: paint.into(), opacity, rule: FillRule::NonZero }));
                        let mut text = String::new();
                        let mut glyphs = Vec::new();
                        for (i, (id, x)) in run.iter().enumerate() {
                            let start = text.len();
                            if let Some(c) = characters.get(&id.0) {
                                text.push(*c);
                            }
                            let advance = run.get(i + 1).map_or(0.0, |(_, next_x)| f32::from(*next_x - *x) / size.max(0.001));
                            glyphs.push(KrillaGlyph::new(GlyphId::new(id.0), advance, 0.0, 0.0, 0.0, start..text.len(), None));
                        }
                        s.draw_glyphs(Point::from_xy(f32::from(origin.x), f32::from(origin.y)), &glyphs, font, &text, size, outlined);
                        s.set_stroke(None);
                        if italic {
                            s.pop();
                        }
                        if transformation.is_some() {
                            s.pop();
                        }
                    });
                }
                PaintRecord::Image { bounds, clip, corner_radii, image, frame_index, opacity, .. } => {
                    let key = Arc::as_ptr(image) as *const u8 as usize;
                    let pdf_image = match self.images.get(&key) {
                        Some(i) => i.clone(),
                        None => {
                            let Some(bytes) = image.as_bytes(*frame_index) else { continue };
                            let size = image.size(*frame_index);
                            let (w, h) = (size.width.0.max(1) as u32, size.height.0.max(1) as u32);
                            let content = (w, h, pixel_hash(bytes));
                            let same = self.images_by_content.get(&content).and_then(|v| v.iter().find(|(held, _)| held.as_slice() == bytes).map(|(_, i)| i.clone()));
                            let i = match same {
                                Some(i) => i,
                                None => {
                                    // BGRA to RGBA, into one buffer of the right size.
                                    let mut rgba = vec![0u8; bytes.len()];
                                    for (to, from) in rgba.chunks_exact_mut(4).zip(bytes.chunks_exact(4)) {
                                        to[0] = from[2];
                                        to[1] = from[1];
                                        to[2] = from[0];
                                        to[3] = from[3];
                                    }
                                    let i = Image::from_rgba8(rgba, w, h);
                                    self.images_by_content.entry(content).or_default().push((bytes.to_vec(), i.clone()));
                                    i
                                }
                            };
                            self.images.insert(key, i.clone());
                            i
                        }
                    };
                    with_clip(&mut surface, clip, within(bounds, clip), |s| {
                        let rounded = corner_radii.top_left > Pixels::ZERO || corner_radii.top_right > Pixels::ZERO || corner_radii.bottom_left > Pixels::ZERO || corner_radii.bottom_right > Pixels::ZERO;
                        if rounded && let Some(path) = rounded_rect(bounds, corner_radii) {
                            s.push_clip_path(&path, &FillRule::NonZero);
                        }
                        let faded = *opacity < 1.0;
                        if faded {
                            s.push_opacity(NormalizedF32::new(opacity.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE));
                        }
                        s.push_transform(&Transform::from_translate(f32::from(bounds.left()), f32::from(bounds.top())));
                        if let Some(size) = Size::from_wh(f32::from(bounds.size.width), f32::from(bounds.size.height)) {
                            s.draw_image(pdf_image.clone(), size);
                        }
                        s.pop();
                        if faded {
                            s.pop();
                        }
                        if rounded {
                            s.pop();
                        }
                    });
                }
                PaintRecord::Line { bounds, color: line_color, clip, .. } => {
                    with_clip(&mut surface, clip, within(bounds, clip), |s| {
                        let (paint, opacity) = color(*line_color);
                        if let Some(path) = rect_path(f32::from(bounds.left()), f32::from(bounds.top()), f32::from(bounds.right()), f32::from(bounds.bottom())) {
                            s.set_stroke(None);
                            s.set_fill(Some(Fill { paint: paint.into(), opacity, rule: FillRule::NonZero }));
                            s.draw_path(&path);
                        }
                    });
                }
                PaintRecord::Shadow { bounds, corner_radii, color: shadow_color, blur_radius, inset, clip } => {
                    if *inset {
                        continue;
                    }
                    let Some((image, rect)) = shadow_image(bounds, corner_radii, *shadow_color, f32::from(*blur_radius)) else { continue };
                    with_clip(&mut surface, clip, within(&rect, clip), |s| {
                        s.push_transform(&Transform::from_translate(f32::from(rect.left()), f32::from(rect.top())));
                        if let Some(size) = Size::from_wh(f32::from(rect.size.width), f32::from(rect.size.height)) {
                            s.draw_image(image, size);
                        }
                        s.pop();
                    });
                }
            }
        }
        surface.pop();
        surface.pop();
        surface.finish();
        page.finish();
    }

    /// Write the pages; the PDF's bytes, and the (PostScript) names of the
    /// fonts that went in as outlines.
    pub fn finish(mut self) -> Result<(Vec<u8>, Vec<String>), String> {
        // A font that cannot be subset to the glyphs it draws goes in as
        // outlines (its text cannot be copied, but it shows).
        for font in self.fonts.values_mut().flatten() {
            font.glyphs.sort_unstable();
            font.glyphs.dedup();
            let remapper = subsetter::GlyphRemapper::new_from_glyphs_sorted(&font.glyphs);
            if subsetter::subset(&font.data, font.index, &remapper).is_err() {
                font.outlined = true;
            }
        }
        let pages = std::mem::take(&mut self.pages);
        for page in &pages {
            self.write_page(page);
        }
        let mut outlined: Vec<String> = self.fonts.values().flatten().filter(|f| f.outlined).map(|f| font_name(&f.data, f.index)).collect();
        outlined.sort();
        outlined.dedup();
        let pdf = self.document.finish().map_err(|e| crate::shell::i18n::format("_app.cannot-write-the-pdf-error", &[("error", &format!("{e:?}"))]))?;
        Ok((pdf, outlined))
    }
}

/// A font as embedded: krilla's font, the characters of its glyphs, its
/// data (for trying the subset), and the glyphs drawn with it.
struct PdfFont {
    font: Font,
    characters: Arc<HashMap<u32, char>>,
    data: Arc<Vec<u8>>,
    index: u32,
    glyph_count: u32,
    /// How far the glyphs reach above and below the baseline, in ems (the
    /// font's bounding box).
    extent: (f32, f32),
    /// Each glyph's advance, in ems.
    advances: Vec<f32>,
    glyphs: Vec<u16>,
    outlined: bool,
    /// Drawn emboldened (filled and outlined) and slanted, as the screen draws a font without a bold or italic face.
    synthetic: (bool, bool),
}

/// Draw `f` inside `clip` (a rectangle in pixels).
/// Whether `record` reaches into its clip.
fn shows(record: &PaintRecord) -> bool {
    let (bounds, clip) = match record {
        PaintRecord::Quad { bounds, clip, .. } | PaintRecord::Image { bounds, clip, .. } | PaintRecord::Line { bounds, clip, .. } => (*bounds, clip),
        PaintRecord::Shadow { .. } => return true,
        PaintRecord::Glyph { origin, font_size, transformation, clip, .. } => {
            if transformation.is_some() {
                return true;
            }
            // Generously: a glyph can reach past its em square.
            let size = *font_size * 2.0;
            (Bounds::new(gpui_kit::point(origin.x - size, origin.y - size), gpui_kit::size(size * 2.0, size * 2.0)), clip)
        }
        PaintRecord::Path { outline, triangles, clip, .. } => {
            let points: Vec<gpui_kit::Point<Pixels>> = match outline {
                Some(outline) => outline.verbs.iter().flat_map(verb_points).collect(),
                None => triangles.iter().flatten().copied().collect(),
            };
            let Some(bounds) = bounds_of(&points) else { return false };
            let reach = outline.as_ref().and_then(|o| o.stroke).map_or(Pixels::ZERO, |s| s.width);
            (bounds.dilate(reach), clip)
        }
    };
    bounds.intersects(clip)
}

/// Whether `bounds` lie within `clip` (then drawn without it).
fn within(bounds: &Bounds<Pixels>, clip: &Bounds<Pixels>) -> bool {
    let e = gpui_kit::px(0.01);
    bounds.left() >= clip.left() - e && bounds.top() >= clip.top() - e && bounds.right() <= clip.right() + e && bounds.bottom() <= clip.bottom() + e
}

/// Draw `f` within `clip`; drawn as it is when it lies `inside` it (a
/// clip for every cell's text would make a large sheet's PDF much bigger).
fn with_clip(s: &mut Surface, clip: &Bounds<Pixels>, inside: bool, f: impl FnOnce(&mut Surface)) {
    if inside {
        return f(s);
    }
    match rect_path(f32::from(clip.left()), f32::from(clip.top()), f32::from(clip.right()), f32::from(clip.bottom())) {
        Some(path) => {
            s.push_clip_path(&path, &FillRule::NonZero);
            f(s);
            s.pop();
        }
        None => {}
    }
}

/// The picture of a blurred shadow: the rounded rectangle `bounds` (the shadow before it is blurred), its coverage blurred by a Gaussian of deviation
/// `sigma` (pixels) and coloured, as straight RGBA with the shadow's alpha where the rectangle is whole. As PowerPoint writes a shadow, the picture's
/// pixels are as coarse as keeps the blur at about 2.2 of them (`sigma / 2.2` logical pixels, at least half a pixel; PowerPoint 16.0.20527.20088's PDFs
/// of blurRad 1.5pt to 24pt: the deviation is 2.2 to 2.5 pixels of the shadow's bitmap, whose resolution goes from 200 dpi down to 29 dpi), at most
/// 1500 across. The rectangle the picture covers goes with it.
fn shadow_image(bounds: &Bounds<Pixels>, radii: &Corners<Pixels>, shadow_color: Hsla, sigma: f32) -> Option<(Image, Bounds<Pixels>)> {
    let rgba = shadow_color.to_rgb();
    let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    if rgba.a <= 0.0 || bw <= 0.0 || bh <= 0.0 {
        return None;
    }
    let sigma = sigma.max(0.0);
    let margin = 3.0 * sigma;
    let pitch = (sigma / 2.2).max(0.5).max((bw + 2.0 * margin) / 1500.0).max((bh + 2.0 * margin) / 1500.0);
    let cols = ((bw + 2.0 * margin) / pitch).ceil() as usize + 1;
    let rows = ((bh + 2.0 * margin) / pitch).ceil() as usize + 1;
    let (left, top) = (f32::from(bounds.left()) - margin, f32::from(bounds.top()) - margin);
    let (cx, cy) = (f32::from(bounds.left()) + bw / 2.0, f32::from(bounds.top()) + bh / 2.0);
    let (hx, hy) = (bw / 2.0, bh / 2.0);
    let radius = |right: bool, bottom: bool| {
        let r = match (right, bottom) {
            (false, false) => radii.top_left,
            (true, false) => radii.top_right,
            (false, true) => radii.bottom_left,
            (true, true) => radii.bottom_right,
        };
        f32::from(r).min(hx).min(hy).max(0.0)
    };
    let mut coverage = vec![0f32; cols * rows];
    for j in 0..rows {
        let y = top + (j as f32 + 0.5) * pitch;
        for i in 0..cols {
            let x = left + (i as f32 + 0.5) * pitch;
            let r = radius(x > cx, y > cy);
            let (qx, qy) = ((x - cx).abs() - hx + r, (y - cy).abs() - hy + r);
            let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
            coverage[j * cols + i] = (0.5 - d / pitch).clamp(0.0, 1.0);
        }
    }
    let deviation = sigma / pitch;
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
                        sum += w * coverage[j * cols + at as usize];
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
                coverage[j as usize * cols + i] = sum;
            }
        }
    }
    let (r, g, b) = ((rgba.r * 255.0).round() as u8, (rgba.g * 255.0).round() as u8, (rgba.b * 255.0).round() as u8);
    let pixels: Vec<u8> = coverage.iter().flat_map(|c| [r, g, b, (c * rgba.a * 255.0).round().clamp(0.0, 255.0) as u8]).collect();
    let image = Image::from_rgba8(pixels, cols as u32, rows as u32);
    let rect = Bounds::new(gpui_kit::point(gpui_kit::px(left), gpui_kit::px(top)), gpui_kit::size(gpui_kit::px(cols as f32 * pitch), gpui_kit::px(rows as f32 * pitch)));
    Some((image, rect))
}

/// A hash of all the bytes of a picture (8 at a time, multiplied and folded in), to tell pictures by what they hold.
fn pixel_hash(bytes: &[u8]) -> u64 {
    const K: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut h = bytes.len() as u64;
    let mut chunks = bytes.chunks_exact(8);
    for c in &mut chunks {
        let v = u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]);
        h = (h.rotate_left(5) ^ v).wrapping_mul(K);
    }
    for b in chunks.remainder() {
        h = (h.rotate_left(5) ^ *b as u64).wrapping_mul(K);
    }
    h
}

fn rect_path(x0: f32, y0: f32, x1: f32, y1: f32) -> Option<Path> {
    let rect = krilla::geom::Rect::from_ltrb(x0, y0, x1.max(x0 + 0.001), y1.max(y0 + 0.001))?;
    let mut pb = PathBuilder::new();
    pb.push_rect(rect);
    pb.finish()
}

fn shrink(radii: &Corners<Pixels>, by: f32) -> Corners<Pixels> {
    let s = |r: Pixels| gpui_kit::px((f32::from(r) - by).max(0.0));
    Corners { top_left: s(radii.top_left), top_right: s(radii.top_right), bottom_right: s(radii.bottom_right), bottom_left: s(radii.bottom_left) }
}

/// A rectangle with rounded corners (quarter circles as cubic curves).
fn rounded_rect(bounds: &Bounds<Pixels>, radii: &Corners<Pixels>) -> Option<Path> {
    let (l, t, r, b) = (f32::from(bounds.left()), f32::from(bounds.top()), f32::from(bounds.right()), f32::from(bounds.bottom()));
    let (w, h) = (r - l, b - t);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let cap = |v: Pixels| f32::from(v).clamp(0.0, w.min(h) / 2.0);
    let (tl, tr, br, bl) = (cap(radii.top_left), cap(radii.top_right), cap(radii.bottom_right), cap(radii.bottom_left));
    if tl + tr + br + bl <= 0.0 {
        return rect_path(l, t, r, b);
    }
    // The control distance of a quarter circle.
    const K: f32 = 0.552_284_8;
    let mut pb = PathBuilder::new();
    pb.move_to(l + tl, t);
    pb.line_to(r - tr, t);
    pb.cubic_to(r - tr + tr * K, t, r, t + tr - tr * K, r, t + tr);
    pb.line_to(r, b - br);
    pb.cubic_to(r, b - br + br * K, r - br + br * K, b, r - br, b);
    pb.line_to(l + bl, b);
    pb.cubic_to(l + bl - bl * K, b, l, b - bl + bl * K, l, b - bl);
    pb.line_to(l, t + tl);
    pb.cubic_to(l, t + tl - tl * K, l + tl - tl * K, t, l + tl, t);
    pb.close();
    pb.finish()
}

fn outline_path(verbs: &[OutlineVerb]) -> Option<Path> {
    let f = f32::from;
    let mut pb = PathBuilder::new();
    for verb in verbs {
        match *verb {
            OutlineVerb::MoveTo(p) => pb.move_to(f(p.x), f(p.y)),
            OutlineVerb::LineTo(p) => pb.line_to(f(p.x), f(p.y)),
            OutlineVerb::QuadTo(c, p) => pb.quad_to(f(c.x), f(c.y), f(p.x), f(p.y)),
            OutlineVerb::CubicTo(c1, c2, p) => pb.cubic_to(f(c1.x), f(c1.y), f(c2.x), f(c2.y), f(p.x), f(p.y)),
            OutlineVerb::Close => pb.close(),
        }
    }
    pb.finish()
}

fn verb_points(verb: &OutlineVerb) -> Vec<gpui_kit::Point<Pixels>> {
    match *verb {
        OutlineVerb::MoveTo(p) | OutlineVerb::LineTo(p) => vec![p],
        OutlineVerb::QuadTo(c, p) => vec![c, p],
        OutlineVerb::CubicTo(c1, c2, p) => vec![c1, c2, p],
        OutlineVerb::Close => Vec::new(),
    }
}

/// The box around `points`.
fn bounds_of(points: &[gpui_kit::Point<Pixels>]) -> Option<Bounds<Pixels>> {
    let (first, rest) = points.split_first()?;
    let (mut lo, mut hi) = (*first, *first);
    for p in rest {
        lo = gpui_kit::point(lo.x.min(p.x), lo.y.min(p.y));
        hi = gpui_kit::point(hi.x.max(p.x), hi.y.max(p.y));
    }
    Some(Bounds::from_corners(lo, hi))
}

fn triangles_path(triangles: &[[gpui_kit::Point<Pixels>; 3]]) -> Option<Path> {
    let f = f32::from;
    let mut pb = PathBuilder::new();
    for [a, b, c] in triangles {
        pb.move_to(f(a.x), f(a.y));
        pb.line_to(f(b.x), f(b.y));
        pb.line_to(f(c.x), f(c.y));
        pb.close();
    }
    pb.finish()
}

/// An sRGB color and its opacity.
fn color(c: Hsla) -> (rgb::Color, NormalizedF32) {
    let Rgba { r, g, b, a } = Rgba::from(c);
    let u = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    (rgb::Color::new(u(r), u(g), u(b)), NormalizedF32::new(a.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE))
}

/// The fill for a background over `bounds`: a color, or a linear gradient
/// (at the CSS angle GPUI's gradients have); `None` when transparent.
fn fill_of(background: &Background, bounds: &Bounds<Pixels>) -> Option<Fill> {
    if let Some(c) = background.as_solid() {
        if c.a <= 0.0 {
            return None;
        }
        let (paint, opacity) = color(c);
        return Some(Fill { paint: paint.into(), opacity, rule: FillRule::NonZero });
    }
    let (angle, stops) = background.as_linear_gradient()?;
    let (cx, cy) = (f32::from(bounds.center().x), f32::from(bounds.center().y));
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let a = angle.to_radians();
    let (dx, dy) = (a.sin(), -a.cos());
    let half = ((w * a.sin()).abs() + (h * a.cos()).abs()) / 2.0;
    let stops = stops
        .iter()
        .map(|s| {
            let (paint, opacity) = color(s.color);
            Stop { offset: NormalizedF32::new(s.percentage.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ZERO), color: paint.into(), opacity }
        })
        .collect();
    let gradient = LinearGradient {
        x1: cx - dx * half,
        y1: cy - dy * half,
        x2: cx + dx * half,
        y2: cy + dy * half,
        transform: Transform::default(),
        spread_method: SpreadMethod::Pad,
        stops,
        anti_alias: false,
    };
    Some(Fill { paint: gradient.into(), opacity: NormalizedF32::ONE, rule: FillRule::NonZero })
}

/// The character each glyph of a font stands for (the first one mapped to
/// it), for the text of the PDF.
/// How far a font's glyphs reach above and below the baseline, in ems:
/// Windows' clipping ascent and descent (`OS/2`, meant to hold the glyphs),
/// else the bounding box (`head`), else generously.
fn vertical_extent(data: &[u8], index: u32) -> (f32, f32) {
    use skrifa::raw::TableProvider as _;
    let Ok(font) = skrifa::FontRef::from_index(data, index) else { return (1.5, 1.0) };
    let Ok(head) = font.head() else { return (1.5, 1.0) };
    let em = f32::from(head.units_per_em()).max(1.0);
    let windows = font.os2().ok().map(|os2| (f32::from(os2.us_win_ascent()) / em, f32::from(os2.us_win_descent()) / em));
    let bounds = (f32::from(head.y_max()) / em, -f32::from(head.y_min()) / em);
    windows.filter(|(a, _)| *a > 0.0).or(Some(bounds)).filter(|(a, d)| *a > 0.0 && *d >= 0.0).unwrap_or((1.5, 1.0))
}

/// The advances of a font's glyphs, in ems (`hmtx`); empty when they
/// cannot be read.
fn glyph_advances(data: &[u8], index: u32, variations: &Variations) -> Vec<f32> {
    use skrifa::MetadataProvider as _;
    let Ok(font) = skrifa::FontRef::from_index(data, index) else { return Vec::new() };
    // At the variable font's axis values (the default instance when it is not variable).
    let location = font.axes().location(variations.iter().map(|(tag, value)| (skrifa::Tag::new(tag), *value)));
    let metrics = font.glyph_metrics(skrifa::instance::Size::unscaled(), &location);
    let em = font.metrics(skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default()).units_per_em.max(1) as f32;
    (0..glyph_count(data, index)).map(|g| metrics.advance_width(skrifa::GlyphId::new(g)).unwrap_or(em) / em).collect()
}

/// How many glyphs a font has.
fn glyph_count(data: &[u8], index: u32) -> u32 {
    use skrifa::raw::TableProvider as _;
    skrifa::FontRef::from_index(data, index).ok().and_then(|font| font.maxp().ok()).map_or(0, |maxp| u32::from(maxp.num_glyphs()))
}

/// A font's PostScript name, for messages.
fn font_name(data: &[u8], index: u32) -> String {
    use skrifa::MetadataProvider as _;
    skrifa::FontRef::from_index(data, index)
        .ok()
        .and_then(|font| font.localized_strings(skrifa::string::StringId::POSTSCRIPT_NAME).english_or_first())
        .map_or_else(|| "?".to_string(), |name| name.to_string())
}

/// The character a glyph is copied as. Several characters often share a
/// glyph (the em dash U+2014 is also U+0336's in some fonts, the en dash
/// U+2013 is the figure dash U+2012's): the one a person typed is the plain
/// one, so combining marks, private use, odd spaces and the figure dash
/// lose to the rest, and the lowest code wins among equals. (Found by
/// pdftotext on the exported coverage files docx-DREF-04 and 05.)
fn glyph_characters(data: &[u8], index: u32) -> HashMap<u32, char> {
    use skrifa::MetadataProvider as _;
    let mut out: HashMap<u32, (u32, char)> = HashMap::new();
    let Ok(font) = skrifa::FontRef::from_index(data, index) else { return HashMap::new() };
    let penalty = |code: u32| -> u32 {
        match code {
            0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F => 3,
            0xE000..=0xF8FF => 3,
            // Kangxi radicals and the radicals supplement (⼤ for 大, ⻄ for 西:
            // a Japanese font maps them to the ideograph's glyph), and the
            // compatibility ideographs.
            0x2E80..=0x2FDF | 0xF900..=0xFAFF | 0x2F800..=0x2FA1F => 3,
            0x02BC | 0x2012 | 0x00A0 | 0x00AD | 0x2000..=0x200B | 0x202F | 0x205F => 2,
            _ => 0,
        }
    };
    for (code, glyph) in font.charmap().mappings() {
        let Some(c) = char::from_u32(code) else { continue };
        let candidate = (penalty(code), c);
        out.entry(glyph.to_u32())
            .and_modify(|best| {
                if (candidate.0, candidate.1 as u32) < (best.0, best.1 as u32) {
                    *best = candidate;
                }
            })
            .or_insert(candidate);
    }
    use skrifa::raw::{TableProvider as _, tables::gsub::{SingleSubst, SubstitutionSubtables}};
    if let Ok(gsub) = font.gsub()
        && let Ok(lookups) = gsub.lookup_list()
    {
        for lookup in lookups.lookups().iter().flatten() {
            if let Ok(SubstitutionSubtables::Single(subs)) = lookup.subtables() {
                for sub in subs.iter().flatten() {
                    match sub {
                        SingleSubst::Format1(f1) => {
                            let delta = f1.delta_glyph_id() as i32;
                            if let Ok(cov) = f1.coverage() {
                                for src in cov.iter() {
                                    let dst = (src.to_u32() as i32 + delta).rem_euclid(65536) as u32;
                                    if let Some(&cand) = out.get(&src.to_u32()) {
                                        out.entry(dst).or_insert(cand);
                                    }
                                }
                            }
                        }
                        SingleSubst::Format2(f2) => {
                            if let Ok(cov) = f2.coverage() {
                                for (src, dst) in cov.iter().zip(f2.substitute_glyph_ids()) {
                                    if let Some(&cand) = out.get(&src.to_u32()) {
                                        out.entry(dst.get().to_u32()).or_insert(cand);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    out.into_iter().map(|(g, (_, c))| (g, c)).collect()
}
