// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pages written as a vector PDF from what GPUI painted (its
//! `PaintRecord`s): text as text in the fonts it was drawn with (subset and
//! embedded, with the characters for copying and searching), shapes as
//! paths, pictures as images. A blurred shadow (a drop shadow, a soft edge, a glow) is a picture of its blur (as PowerPoint writes one: a bitmap
//! with an alpha mask, PDF having no blur); an inner shadow is the same picture of the blur of what is not in its hole, cut to the shape.
//!
//! The records are in logical pixels from the page's top-left corner; each
//! page is scaled to its size in points.

use std::collections::HashMap;
use std::sync::Arc;

use gpui_kit::{Background, BorderStyle, Bounds, Corners, DrawnGlyph, FontId, Hsla, OutlineVerb, PaintRecord, Pixels, Rgba, TextSystem};
use krilla::Document;
use krilla::color::rgb;
use krilla::geom::{Path, PathBuilder, Point, Size, Transform};
use krilla::image::Image;
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use crate::export_print::PaintKind;
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
    page_pt: (f32, f32),
    /// The page's number in its document (from 1; the slide's or the document page's), when the caller said: where a link to a page goes to.
    number: Option<usize>,
    /// The hyperlinks on the page (`/Subtype /Link` annotations).
    links: Vec<PageLink>,
    /// What the page is made of, in order: one window's records for a slide's page, several for a handout or notes page.
    layers: Vec<Layer>,
}

/// Where a link on a page goes.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkTo {
    /// An address outside the file (`/URI`: a web page, a `mailto:`).
    Url(String),
    /// A place of the document: the page numbered `number` (the numbers given with `link_last_page`) and the point (`x`, `y`) on it, in points from its top left (the PDF
    /// has them as whole numbers, as Word's and PowerPoint's do: `x` rounded down and the height above the page's bottom rounded down).
    Page { number: usize, x: f32, y: f32 },
}

/// The point of a link's destination (`x`, `y` in points from the page's top left on a page `page_height` high) as the PDF writes it (whole numbers: `x` rounded down, the
/// height above the page's bottom rounded down), in points from the top left again.
fn link_destination(page_height: f32, x: f32, y: f32) -> (f32, f32) {
    let up = (page_height - y).floor().clamp(0.0, page_height.floor());
    (x.max(0.0).floor(), page_height - up)
}

/// A clickable stretch of a page, in points from its top left.
#[derive(Clone, Debug, PartialEq)]
pub struct PageLink {
    pub rect: (f32, f32, f32, f32),
    pub to: LinkTo,
}

/// What a window painted (`records`), put on a page: its logical pixel is `k` points, its top left is at `origin` points of the page,
/// and only what is inside `clip` (x, y, width, height in points of the page) shows.
pub struct Layer {
    pub records: Vec<PaintRecord>,
    /// Device pixels to a logical pixel of the window.
    pub scale_factor: f32,
    pub k: f32,
    pub origin: (f32, f32),
    pub clip: Option<(f32, f32, f32, f32)>,
    /// The frames (x, y, width, height in the window's logical pixels) of the shapes with a plain fill and no line: in the pure black and white print the
    /// fill of each gets the thin black edge PowerPoint draws (`HAIRLINE_PT`).
    pub hairline: Vec<(f32, f32, f32, f32)>,
    /// The frames of the charts, in the same pixels: what is painted inside one keeps its fills in grey in the pure black and white print (a chart's bars are
    /// not white as a shape's fill is: `tasks/doc/requests/2026-11-20-225-bw-hairline-result.md`).
    pub charts: Vec<(f32, f32, f32, f32)>,
}

/// The width of the black line PowerPoint's pure black and white print draws round a plain fill that has no line: 0.5 of the units of its print PDF (0.75 pt
/// each), whatever the paper's scale, with round ends and joins (Windows PowerPoint's `PrintOut` PDFs, `tasks/doc/requests/2026-11-20-225-bw-hairline-result.md`).
pub const HAIRLINE_PT: f32 = 0.375;

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
    /// Where the fonts' data comes from (the platform's text system), asked when the pages are written.
    text_system: Arc<TextSystem>,
    /// Fonts the caller gave data for (the browser version's substitutes for what its Canvas draws), by the ids it made up.
    given_fonts: HashMap<FontId, (Arc<Vec<u8>>, u32)>,
    /// How the colours are printed (see `PRINT_COLOR`).
    print_color: crate::export_print::PrintColor,
}

impl PdfWriter {
    pub fn new(text_system: Arc<TextSystem>) -> Self {
        crate::render::glyph_text::set_recording(true);
        Self { pages: Vec::new(), document: Document::new(), fonts: HashMap::new(), images: HashMap::new(), images_by_content: HashMap::new(), missing_glyphs: 0, text_system, given_fonts: HashMap::new(), print_color: crate::export_print::PrintColor::Color }
    }

    /// Add a page `page_pt` points large from `records`, painted in a window
    /// `width_px` logical pixels wide at `scale_factor` device pixels each.
    /// `font_data` gives a font's data by its id.
    /// The pages added so far.
    /// Print the colours of the pages as `mode` (the print settings' グレースケール / 白黒), when they are written.
    pub fn set_print_color(&mut self, mode: crate::export_print::PrintColor) {
        self.print_color = mode;
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn add_page(&mut self, records: Vec<PaintRecord>, width_px: f32, page_pt: (f32, f32), scale_factor: f32) {
        self.add_page_shifted(records, width_px, page_pt, scale_factor, (0.0, 0.0));
    }

    /// [`Self::add_page`], with everything on the page moved by `shift`
    /// points (to put what the window drew on whole pixels where the paper
    /// wants it between them).
    pub fn add_page_shifted(&mut self, records: Vec<PaintRecord>, width_px: f32, page_pt: (f32, f32), scale_factor: f32, shift: (f32, f32)) {
        let layer = Layer { records, scale_factor, k: page_pt.0 / width_px.max(1.0), origin: shift, clip: None, hairline: Vec::new(), charts: Vec::new() };
        self.pages.push(PendingPage { page_pt, number: None, links: Vec::new(), layers: vec![layer] });
    }

    /// The page added last has its first layer's plain filled shapes at `frames` (see [`Layer::hairline`]).
    pub fn hairline_last_page(&mut self, frames: Vec<(f32, f32, f32, f32)>, charts: Vec<(f32, f32, f32, f32)>) {
        if let Some(layer) = self.pages.last_mut().and_then(|page| page.layers.first_mut()) {
            layer.hairline = frames;
            layer.charts = charts;
        }
    }

    /// The page added last is numbered `number` (from 1) in its document and has `links` (`/Link` annotations; those that go to a page that is not in the file are left out).
    pub fn link_last_page(&mut self, number: usize, links: Vec<PageLink>) {
        if let Some(page) = self.pages.last_mut() {
            page.number = Some(number);
            page.links = links;
        }
    }

    /// A page `page_pt` points large made of `layers` (a handout page: the slides' records each put in its place, and the text).
    pub fn add_page_layers(&mut self, page_pt: (f32, f32), layers: Vec<Layer>) {
        self.pages.push(PendingPage { page_pt, number: None, links: Vec::new(), layers });
    }

    /// The glyphs the platform drew itself, with what they drew and the font asked for (the browser version's Canvas text): each
    /// distinct (font, glyph) once. Their fonts have no file; `substitute_drawn` gives them one.
    pub fn drawn_glyphs(&self) -> Vec<(FontId, gpui_kit::GlyphId, DrawnGlyph)> {
        let mut seen = std::collections::HashSet::new();
        let mut found = Vec::new();
        for record in self.pages.iter().flat_map(|page| page.layers.iter().flat_map(|layer| &layer.records)) {
            if let PaintRecord::Glyph { font_id, glyph_id, emoji: false, .. } = record
                && seen.insert((*font_id, *glyph_id))
                && let Some(drawn) = self.text_system.drawn_glyph(*font_id, *glyph_id)
            {
                found.push((*font_id, *glyph_id, drawn));
            }
        }
        found
    }

    /// Give the font file `data` (face `index`) for the id `font_id`, which the caller made up.
    pub fn give_font(&mut self, font_id: FontId, data: Arc<Vec<u8>>, index: u32) {
        self.given_fonts.insert(font_id, (data, index));
    }

    /// Replace the font and glyph of the drawn glyphs `replace` has an answer for (a font given with `give_font` and the glyph of its
    /// own that draws the same text).
    pub fn substitute_drawn(&mut self, replace: &mut dyn FnMut(FontId, gpui_kit::GlyphId) -> Option<(FontId, gpui_kit::GlyphId)>) {
        for record in self.pages.iter_mut().flat_map(|page| page.layers.iter_mut().flat_map(|layer| &mut layer.records)) {
            if let PaintRecord::Glyph { font_id, glyph_id, emoji: false, .. } = record
                && let Some((font, glyph)) = replace(*font_id, *glyph_id)
            {
                (*font_id, *glyph_id) = (font, glyph);
            }
        }
    }

    /// Register the fonts the pages draw with, and the glyphs they use of each.
    fn register_fonts(&mut self) {
        let mut used = Vec::new();
        for record in self.pages.iter().flat_map(|page| page.layers.iter().flat_map(|layer| &layer.records)) {
            if let PaintRecord::Glyph { font_id, glyph_id, .. } = record {
                used.push((*font_id, *glyph_id));
            }
        }
        for (font_id, glyph_id) in used {
            let font = self.fonts.entry(font_id).or_insert_with(|| {
                let (data, synthetic, variations) = match self.given_fonts.get(&font_id) {
                    Some(given) => (Some(given.clone()), (false, false), Vec::new()),
                    None => (self.text_system.font_data(font_id), self.text_system.synthetic_style(font_id), self.text_system.font_variations(font_id)),
                };
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

    fn write_page(&mut self, pending: &PendingPage, index_of: &HashMap<usize, usize>) {
        let page_pt = pending.page_pt;
        let Some(settings) = PageSettings::from_wh(page_pt.0, page_pt.1) else { return };
        let mut page = self.document.start_page_with(settings);
        let mut surface = page.surface();
        for layer in &pending.layers {
        let (records, scale_factor, k, shift) = (layer.records.as_slice(), layer.scale_factor, layer.k, layer.origin);
        let hairline = print_color() == crate::export_print::PrintColor::BlackWhite;
        let in_chart = |bounds: &Bounds<Pixels>| hairline && layer.charts.iter().any(|f| f32::from(bounds.left()) >= f.0 - 1.5 && f32::from(bounds.top()) >= f.1 - 1.5 && f32::from(bounds.right()) <= f.0 + f.2 + 1.5 && f32::from(bounds.bottom()) <= f.1 + f.3 + 1.5);
        let edge = |bounds: &Bounds<Pixels>| hairline && layer.hairline.iter().any(|f| (f.0 - f32::from(bounds.left())).abs() < 1.5 && (f.1 - f32::from(bounds.top())).abs() < 1.5 && (f.2 - f32::from(bounds.size.width)).abs() < 1.5 && (f.3 - f32::from(bounds.size.height)).abs() < 1.5);
        let clipped = match layer.clip {
            Some((x, y, w, h)) => match rect_path(x, y, x + w, y + h) {
                Some(path) => {
                    surface.push_clip_path(&path, &FillRule::NonZero);
                    true
                }
                None => continue,
            },
            None => false,
        };
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
                    IN_CHART.with(|c| c.set(in_chart(bounds)));
                    with_clip(&mut surface, clip, within(bounds, clip), |s| {
                        let outer = rounded_rect(bounds, corner_radii);
                        if let (Some(path), Some(fill)) = (&outer, fill_of(background, bounds)) {
                            s.set_stroke(None);
                            s.set_fill(Some(fill));
                            s.draw_path(path);
                            if background.as_solid().is_some() && edge(bounds) {
                                draw_hairline(s, path, k);
                            }
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
                                let (paint, opacity) = color(*border_color, PaintKind::Line);
                                let dash = (*border_style == BorderStyle::Dashed).then(|| StrokeDash { array: vec![w * 3.0, w * 3.0], offset: 0.0 });
                                s.set_stroke(Some(Stroke { paint: paint.into(), width: w, opacity, dash, ..Stroke::default() }));
                                s.draw_path(&path);
                                s.set_stroke(None);
                            }
                        } else {
                            // Uneven: each side as a band.
                            let (l, t, r, b) = (f32::from(bounds.left()), f32::from(bounds.top()), f32::from(bounds.right()), f32::from(bounds.bottom()));
                            let bands = [(l, t, r, t + widths[0]), (r - widths[1], t, r, b), (l, b - widths[2], r, b), (l, t, l + widths[3], b)];
                            let (paint, opacity) = color(*border_color, PaintKind::Line);
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
                    IN_CHART.with(|c| c.set(in_chart(&bounds)));
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
                                if background.as_solid().is_some() && edge(&bounds) {
                                    draw_hairline(s, &path, k);
                                }
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
                        let (paint, opacity) = color(*glyph_color, PaintKind::Text);
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
                            // The character the painting code noted for this glyph (a proportional typeface's stand-in draws オ, （ and １ with the glyphs of
                            // ｵ, ( and 1, which the glyph id alone copies as): unless the glyph is several characters (a ligature, か + U+309A).
                            let noted = gpui_kit::point(*x, origin.y);
                            match (characters.get(&id.0), crate::render::glyph_text::noted(*font_id, *id, noted)) {
                                (Some(c), Some(n)) if c.chars().count() == 1 && c.chars().next() != Some(n) => text.push(n),
                                (Some(c), _) => text.push_str(c),
                                (None, _) => {}
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
                                    let mode = print_color();
                                    for (to, from) in rgba.chunks_exact_mut(4).zip(bytes.chunks_exact(4)) {
                                        let (r, g, b) = mode.map((from[2], from[1], from[0]));
                                        to[0] = r;
                                        to[1] = g;
                                        to[2] = b;
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
                        let (paint, opacity) = color(*line_color, PaintKind::Line);
                        if let Some(path) = rect_path(f32::from(bounds.left()), f32::from(bounds.top()), f32::from(bounds.right()), f32::from(bounds.bottom())) {
                            s.set_stroke(None);
                            s.set_fill(Some(Fill { paint: paint.into(), opacity, rule: FillRule::NonZero }));
                            s.draw_path(&path);
                        }
                    });
                }
                PaintRecord::Shadow { bounds, corner_radii, color: shadow_color, blur_radius, inset, element, element_radii, clip } => {
                    let sigma = f32::from(*blur_radius);
                    let made = if *inset { inner_shadow_image(element, element_radii, bounds, corner_radii, *shadow_color, sigma) } else { shadow_image(bounds, corner_radii, *shadow_color, sigma) };
                    let Some((image, rect)) = made else { continue };
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
        if clipped {
            surface.pop();
        }
        }
        surface.finish();
        for link in &pending.links {
            let (x, y, w, h) = link.rect;
            let Some(rect) = krilla::geom::Rect::from_xywh(x, y, w.max(0.1), h.max(0.1)) else { continue };
            let target = match &link.to {
                LinkTo::Url(url) => krilla::annotation::Target::Action(krilla::action::Action::Link(krilla::action::LinkAction::new(url.clone()))),
                LinkTo::Page { number, x, y } => {
                    let Some(&index) = index_of.get(number) else { continue };
                    let (dest_x, dest_y) = link_destination(pending.page_pt.1, *x, *y);
                    krilla::annotation::Target::Destination(krilla::destination::Destination::Xyz(krilla::destination::XyzDestination::new(index, krilla::geom::Point::from_xy(dest_x, dest_y))))
                }
            };
            page.add_annotation(krilla::annotation::LinkAnnotation::new(rect, target).into());
        }
        page.finish();
    }

    /// Write the pages; the PDF's bytes, the (PostScript) names of the
    /// fonts that went in as outlines, and the glyphs left out for want of their font.
    pub fn finish(mut self) -> Result<(Vec<u8>, Vec<String>, usize), String> {
        self.register_fonts();
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
        let index_of: HashMap<usize, usize> = pages.iter().enumerate().filter_map(|(i, p)| p.number.map(|n| (n, i))).collect();
        PRINT_COLOR.with(|c| c.set(self.print_color));
        for page in &pages {
            self.write_page(page, &index_of);
        }
        PRINT_COLOR.with(|c| c.set(crate::export_print::PrintColor::Color));
        let mut outlined: Vec<String> = self.fonts.values().flatten().filter(|f| f.outlined).map(|f| font_name(&f.data, f.index)).collect();
        outlined.sort();
        outlined.dedup();
        let pdf = self.document.finish().map_err(|e| crate::shell::i18n::format("_app.cannot-write-the-pdf-error", &[("error", &format!("{e:?}"))]))?;
        crate::render::glyph_text::set_recording(false);
        Ok((pdf, outlined, self.missing_glyphs))
    }
}

/// A font as embedded: krilla's font, the characters of its glyphs, its
/// data (for trying the subset), and the glyphs drawn with it.
struct PdfFont {
    font: Font,
    characters: Arc<HashMap<u32, String>>,
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

/// The raster a shadow is made on: `cols` × `rows` cells of `pitch` logical pixels from (`left`, `top`).
struct Grid {
    cols: usize,
    rows: usize,
    left: f32,
    top: f32,
    pitch: f32,
}

impl Grid {
    /// A grid over the rectangle (`left`, `top`, `width`, `height`) with the pixel pitch a blur of deviation `sigma` calls for: as PowerPoint writes a
    /// shadow, the picture's pixels are as coarse as keeps the blur at about 2.2 of them (`sigma / 2.2` logical pixels, at least half a pixel;
    /// PowerPoint 16.0.20527.20088's PDFs of blurRad 1.5pt to 24pt: the deviation is 2.2 to 2.5 pixels of the shadow's bitmap, whose resolution goes
    /// from 200 dpi down to 29 dpi), at most 1500 across.
    fn over(left: f32, top: f32, width: f32, height: f32, sigma: f32) -> Grid {
        let pitch = (sigma / 2.2).max(0.5).max(width / 1500.0).max(height / 1500.0);
        Grid { cols: (width / pitch).ceil() as usize + 1, rows: (height / pitch).ceil() as usize + 1, left, top, pitch }
    }

    /// The coverage (0 to 1, anti-aliased by a pixel) of the rounded rectangle `bounds` at each cell.
    fn coverage(&self, bounds: &Bounds<Pixels>, radii: &Corners<Pixels>) -> Vec<f32> {
        let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
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
        let mut coverage = vec![0f32; self.cols * self.rows];
        for j in 0..self.rows {
            let y = self.top + (j as f32 + 0.5) * self.pitch;
            for i in 0..self.cols {
                let x = self.left + (i as f32 + 0.5) * self.pitch;
                let r = radius(x > cx, y > cy);
                let (qx, qy) = ((x - cx).abs() - hx + r, (y - cy).abs() - hy + r);
                let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
                coverage[j * self.cols + i] = (0.5 - d / self.pitch).clamp(0.0, 1.0);
            }
        }
        coverage
    }

    /// `coverage` blurred by a Gaussian of deviation `sigma` (pixels).
    fn blur(&self, coverage: &mut [f32], sigma: f32) {
        let (cols, rows) = (self.cols, self.rows);
        let deviation = sigma / self.pitch;
        if deviation <= 0.3 {
            return;
        }
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

    /// The picture of `alpha` (0 to 1 at each cell) in `rgba`'s colour, and the rectangle it covers.
    fn image(&self, alpha: &[f32], rgba: Rgba) -> (Image, Bounds<Pixels>) {
        let (r, g, b) = print_color().map_for(crate::export_print::PaintKind::Fill, ((rgba.r * 255.0).round() as u8, (rgba.g * 255.0).round() as u8, (rgba.b * 255.0).round() as u8));
        let pixels: Vec<u8> = alpha.iter().flat_map(|c| [r, g, b, (c * rgba.a * 255.0).round().clamp(0.0, 255.0) as u8]).collect();
        let image = Image::from_rgba8(pixels, self.cols as u32, self.rows as u32);
        let rect = Bounds::new(gpui_kit::point(gpui_kit::px(self.left), gpui_kit::px(self.top)), gpui_kit::size(gpui_kit::px(self.cols as f32 * self.pitch), gpui_kit::px(self.rows as f32 * self.pitch)));
        (image, rect)
    }
}

/// The picture of a blurred shadow: the rounded rectangle `bounds` (the shadow before it is blurred), its coverage blurred by a Gaussian of deviation
/// `sigma` (pixels) and coloured, as straight RGBA with the shadow's alpha where the rectangle is whole. The rectangle the picture covers goes with it.
fn shadow_image(bounds: &Bounds<Pixels>, radii: &Corners<Pixels>, shadow_color: Hsla, sigma: f32) -> Option<(Image, Bounds<Pixels>)> {
    let rgba = shadow_color.to_rgb();
    let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    if rgba.a <= 0.0 || bw <= 0.0 || bh <= 0.0 {
        return None;
    }
    let sigma = sigma.max(0.0);
    let margin = 3.0 * sigma;
    let grid = Grid::over(f32::from(bounds.left()) - margin, f32::from(bounds.top()) - margin, bw + 2.0 * margin, bh + 2.0 * margin, sigma);
    let mut coverage = grid.coverage(bounds, radii);
    grid.blur(&mut coverage, sigma);
    Some(grid.image(&coverage, rgba))
}

/// The shadow inside a shape (`a:innerShdw`) as a coverage over a grid: it falls where the `hole` (the shape moved by the shadow's offset, with the
/// same corners) does not cover, its edge blurred by `sigma`, and only inside the shape `element`. The grid covers the shape's rectangle and the
/// reach of the blur round it (the hole is blurred where it goes past the shape too).
fn inner_shadow_alpha(element: &Bounds<Pixels>, element_radii: &Corners<Pixels>, hole: &Bounds<Pixels>, hole_radii: &Corners<Pixels>, sigma: f32) -> Option<(Grid, Vec<f32>)> {
    let (ew, eh) = (f32::from(element.size.width), f32::from(element.size.height));
    if ew <= 0.0 || eh <= 0.0 {
        return None;
    }
    let sigma = sigma.max(0.0);
    let margin = 3.0 * sigma;
    let grid = Grid::over(f32::from(element.left()) - margin, f32::from(element.top()) - margin, ew + 2.0 * margin, eh + 2.0 * margin, sigma);
    let mut covered = grid.coverage(hole, hole_radii);
    grid.blur(&mut covered, sigma);
    let inside = grid.coverage(element, element_radii);
    let alpha = covered.iter().zip(&inside).map(|(c, i)| (1.0 - c) * i).collect();
    Some((grid, alpha))
}

/// The picture of a shadow inside a shape (`a:innerShdw`, see `inner_shadow_alpha`), and the rectangle it covers.
fn inner_shadow_image(element: &Bounds<Pixels>, element_radii: &Corners<Pixels>, hole: &Bounds<Pixels>, hole_radii: &Corners<Pixels>, shadow_color: Hsla, sigma: f32) -> Option<(Image, Bounds<Pixels>)> {
    let rgba = shadow_color.to_rgb();
    if rgba.a <= 0.0 {
        return None;
    }
    let (grid, alpha) = inner_shadow_alpha(element, element_radii, hole, hole_radii, sigma)?;
    Some(grid.image(&alpha, rgba))
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

/// The thin black line of the pure black and white print over `path` (a fill's own path): [`HAIRLINE_PT`] wide on the page, so `1 / k` of it in the
/// window's pixels; round ends and joins, a miter limit of 10.
fn draw_hairline(s: &mut Surface, path: &Path, k: f32) {
    s.set_fill(None);
    s.set_stroke(Some(Stroke {
        paint: rgb::Color::new(0, 0, 0).into(),
        width: HAIRLINE_PT / k.max(1e-6),
        opacity: NormalizedF32::ONE,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        miter_limit: 10.0,
        ..Stroke::default()
    }));
    s.draw_path(path);
    s.set_stroke(None);
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

thread_local! {
    /// How the pages being written print their colours (the print settings' グレースケール / 白黒, #225): every colour, picture and shadow
    /// of the vector PDF goes through `PrintColor::map`, as the picture route's pixels do.
    static PRINT_COLOR: std::cell::Cell<crate::export_print::PrintColor> = const { std::cell::Cell::new(crate::export_print::PrintColor::Color) };
}

thread_local! {
    /// What is being drawn is inside a chart (the pure black and white print keeps a chart's fills in grey).
    static IN_CHART: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn print_color() -> crate::export_print::PrintColor {
    PRINT_COLOR.with(|c| c.get())
}

/// A colour and its opacity. The PDF is always sRGB: the CMYK conversion of a print is the printer driver's (a file with the CMYK
/// mode on shows its colours as the press would print them on the screen only).
fn color(c: Hsla, kind: crate::export_print::PaintKind) -> (krilla::color::Color, NormalizedF32) {
    let Rgba { r, g, b, a } = Rgba::from(c);
    let u = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let opacity = NormalizedF32::new(a.clamp(0.0, 1.0)).unwrap_or(NormalizedF32::ONE);
    let mode = match (print_color(), kind) {
        (crate::export_print::PrintColor::BlackWhite, PaintKind::Fill) if IN_CHART.with(|c| c.get()) => crate::export_print::PrintColor::Gray,
        (mode, _) => mode,
    };
    let (r, g, b) = mode.map_for(kind, (u(r), u(g), u(b)));
    (rgb::Color::new(r, g, b).into(), opacity)
}

/// The fill for a background over `bounds`: a color, or a linear gradient
/// (at the CSS angle GPUI's gradients have); `None` when transparent.
fn fill_of(background: &Background, bounds: &Bounds<Pixels>) -> Option<Fill> {
    if let Some(c) = background.as_solid() {
        if c.a <= 0.0 {
            return None;
        }
        let (paint, opacity) = color(c, PaintKind::Fill);
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
            let (paint, opacity) = color(s.color, PaintKind::Fill);
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
pub(crate) fn font_name(data: &[u8], index: u32) -> String {
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
fn glyph_characters(data: &[u8], index: u32) -> HashMap<u32, String> {
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
    let mut out: HashMap<u32, String> = out.into_iter().map(|(g, (_, c))| (g, c.to_string())).collect();
    // A glyph only a variation sequence reaches (an IVS of the cmap's format 14: 葛 + U+E0100) is copied as the base and its selector, so that
    // the text copied from the PDF is the pair, not the plain kanji or nothing (GitHub issue #158). A glyph that a plain character also reaches
    // keeps that character unless it is one of the odd ones (compatibility ideographs, private use).
    {
        use skrifa::charmap::MapVariant;
        for (base, selector, variant) in font.charmap().variant_mappings() {
            let MapVariant::Variant(glyph) = variant else { continue };
            let (Some(base), Some(selector)) = (char::from_u32(base), char::from_u32(selector)) else { continue };
            let odd = out.get(&glyph.to_u32()).is_none_or(|t| t.chars().next().is_none_or(|c| penalty(c as u32) > 0));
            if odd {
                out.insert(glyph.to_u32(), format!("{base}{selector}"));
            }
        }
    }
    // A glyph the font's GSUB makes of several (a ligature, or か + U+309A as か゚: no character of the font's cmap is that glyph) is copied as
    // the characters of the glyphs it replaces, in order (GitHub issue #163: the PDF's text for か゚ and the like was empty).
    if let Ok(gsub) = font.gsub()
        && let Ok(lookups) = gsub.lookup_list()
    {
        for lookup in lookups.lookups().iter().flatten() {
            if let Ok(SubstitutionSubtables::Ligature(subs)) = lookup.subtables() {
                for sub in subs.iter().flatten() {
                    let Ok(cov) = sub.coverage() else { continue };
                    for (first, set) in cov.iter().zip(sub.ligature_sets().iter()) {
                        let Ok(set) = set else { continue };
                        for lig in set.ligatures().iter().flatten() {
                            let target = lig.ligature_glyph().to_u32();
                            if out.contains_key(&target) {
                                continue;
                            }
                            let mut text = String::new();
                            let known = std::iter::once(first.to_u32()).chain(lig.component_glyph_ids().iter().map(|g| g.get().to_u32())).all(|g| match out.get(&g) {
                                Some(c) => {
                                    text.push_str(c);
                                    true
                                }
                                None => false,
                            });
                            if known {
                                out.insert(target, text);
                            }
                        }
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{point, px, size};

    /// A glyph that the font's GSUB ligature lookup makes of several characters is copied as all of them, not as nothing (GitHub issue #163, か゚
    /// as Yu Gothic makes it of か and U+309A): Carlito's `ch` and `ffb` ligatures, which no cmap entry reaches.
    #[test]
    fn a_glyph_made_of_several_characters_is_copied_as_them() {
        let path = ["fonts/carlito/Carlito-Regular.ttf", "../ibis-office-render/fonts/carlito/Carlito-Regular.ttf"].into_iter().map(|p| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(p)).find(|p| p.exists()).expect("Carlito");
        let data = std::fs::read(path).unwrap();
        let characters = glyph_characters(&data, 0);
        let copied: std::collections::HashSet<&str> = characters.values().map(|s| s.as_str()).collect();
        assert!(copied.contains("ch") && copied.contains("ffb"), "ligatures that no cmap entry reaches (Carlito's lookups 36 and 37) are copied as their letters");
        assert!(characters.values().any(|s| s.chars().count() > 1), "some glyph is copied as several characters");
        assert!(copied.contains("a") && copied.contains("A"), "single glyphs stay as they were");
    }

    /// GitHub issue #158: a glyph an IVS reaches (IPAmj Mincho's 葛 + U+E0100 and the like) is copied as the base and the selector. Reads the font the
    /// Setup dialog downloaded (or the one under test/github-issue/158) and does nothing when this machine has none.
    #[test]
    fn a_glyph_an_ivs_reaches_is_copied_as_the_pair() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let candidates = [root.join("test/github-issue/158/ipamj-00601/ipamjm.ttf")];
        let Some(path) = candidates.iter().find(|p| p.exists()) else { return };
        let data = std::fs::read(path).unwrap();
        let characters = glyph_characters(&data, 0);
        use skrifa::MetadataProvider as _;
        let font = skrifa::FontRef::from_index(&data, 0).unwrap();
        let mut pairs = 0;
        for (base, selector, variant) in font.charmap().variant_mappings() {
            if let skrifa::charmap::MapVariant::Variant(glyph) = variant {
                let text = characters.get(&glyph.to_u32()).expect("the glyph has a copied text");
                let want = format!("{}{}", char::from_u32(base).unwrap(), char::from_u32(selector).unwrap());
                if text == &want {
                    pairs += 1;
                }
            }
        }
        assert!(pairs > 1000, "the IVS glyphs are copied as base + selector ({pairs} of them)");
    }

    fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    /// An inner shadow (`a:innerShdw`) is no longer left out of the PDF: with the shape moved 6 px right and down as the hole, the shadow is
    /// along the shape's top and left edges inside it, and nowhere else (not in the middle, not along the bottom and right edges, not outside).
    #[test]
    fn an_inner_shadow_falls_inside_the_shape_along_the_edges_the_offset_uncovers() {
        let element = bounds(10.0, 10.0, 60.0, 60.0);
        let hole = bounds(16.0, 16.0, 60.0, 60.0);
        let (grid, alpha) = inner_shadow_alpha(&element, &Corners::default(), &hole, &Corners::default(), 0.1).expect("a shadow");
        let at = |x: f32, y: f32| {
            let (i, j) = (((x - grid.left) / grid.pitch).floor() as usize, ((y - grid.top) / grid.pitch).floor() as usize);
            alpha[j * grid.cols + i]
        };
        assert!(at(12.0, 40.0) > 0.9, "left edge strip: {}", at(12.0, 40.0));
        assert!(at(40.0, 12.0) > 0.9, "top edge strip: {}", at(40.0, 12.0));
        assert!(at(40.0, 40.0) < 0.05, "the middle: {}", at(40.0, 40.0));
        assert!(at(68.0, 40.0) < 0.05, "right edge, covered by the hole: {}", at(68.0, 40.0));
        assert!(at(40.0, 68.0) < 0.05, "bottom edge, covered by the hole: {}", at(40.0, 68.0));
        // Blurred (3 px), the shadow is still nowhere outside the shape: no cell whose centre is a pitch past its edge.
        let (grid, alpha) = inner_shadow_alpha(&element, &Corners::default(), &hole, &Corners::default(), 3.0).expect("a shadow");
        assert!(alpha.iter().any(|a| *a > 0.2), "a blurred shadow is still there");
        for j in 0..grid.rows {
            for i in 0..grid.cols {
                let (x, y) = (grid.left + (i as f32 + 0.5) * grid.pitch, grid.top + (j as f32 + 0.5) * grid.pitch);
                if x < 10.0 - grid.pitch || x > 70.0 + grid.pitch || y < 10.0 - grid.pitch || y > 70.0 + grid.pitch {
                    assert!(alpha[j * grid.cols + i] < 0.05, "outside the shape at ({x}, {y}): {}", alpha[j * grid.cols + i]);
                }
            }
        }
        // The picture is made, and covers the shape.
        let (_, rect) = inner_shadow_image(&element, &Corners::default(), &hole, &Corners::default(), gpui_kit::rgba(0x000000ff).into(), 0.1).expect("a picture");
        assert!(rect.left() <= element.left() && rect.right() >= element.right());
    }
}
