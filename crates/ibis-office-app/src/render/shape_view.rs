// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing slide content: backgrounds, shapes, pictures and text.
//!
//! These functions only draw; `slide_view.rs` adds selection and editing on
//! top, and `thumbnails.rs` reuses them for the slide list.

use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;
use std::sync::Arc;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{
    AutoShape, Color as DocColor, EMU_PER_POINT, EmuRect, Fill, Geometry, LineCap, LineEnd,
    LineJoin, LineSpacing, MediaFormat, MediaId, Picture, Placeholder, Presentation, Rgb, Shape,
    ShapeKind, Slide, Stroke, TextAlign, TextBody, TextStyle, VerticalAnchor,
};

/// Line height of single spacing, as a multiple of the font size. Shared by
/// display and the text field, so text does not jump when editing starts.
pub(crate) const LINE_HEIGHT: f32 = crate::render::font_metrics::POWERPOINT_SINGLE;

/// Decoded pictures and the fonts available to draw with, for one presentation.
/// Identifies one rotated drawing of a picture.
type RotatedKey = (MediaId, u64);

/// Color and contrast rendering mode for presentations and accessibility (表示 > カラー/白黒).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum ColorMode {
    #[default]
    Color,
    Grayscale,
    HighContrast,
}

#[cfg(all(windows,feature="export"))]
type LiveFrame=Arc<RenderImage>;
#[cfg(not(all(windows,feature="export")))]
type LiveFrame=crate::media_player::Frame;

pub(crate) struct Resources {
    /// Color and contrast rendering mode (Color, Grayscale, HighContrast).
    pub(crate) color_mode: ColorMode,
    pub(crate) images: HashMap<MediaId, Arc<Image>>,
    /// Encoded data of pictures that can be wrapped in SVG to rotate them.
    sources: std::cell::RefCell<HashMap<MediaId, (crate::render::picture_svg::Encoding, Arc<[u8]>)>>,
    /// Rotated pictures made so far, with their bounds relative to the frame.
    rotated: std::cell::RefCell<HashMap<RotatedKey, (Arc<Image>, EmuRect)>>,
    /// Pictures cut to the outlines of shapes they fill, by picture and a
    /// hash of the outline.
    clipped: std::cell::RefCell<HashMap<(MediaId, u64), Arc<Image>>>,
    /// Pictures recoloured as duotones, by picture and colours (and with a
    /// colour made clear, by picture and that colour twice).
    duotones: std::cell::RefCell<HashMap<(MediaId, ibis_doc_model::Rgb, ibis_doc_model::Rgb), Arc<Image>>>,
    /// Those SVG pictures (rotated, cut and recoloured) rasterized ahead,
    /// by the image's id: see `rasterize_svg_pictures`.
    rasters: std::cell::RefCell<HashMap<u64, Arc<RenderImage>>>,
    /// Pictures scaled down to their drawn pixel size so GPUI's bilinear
    /// sampler does not alias fine lines (`m1_downscaled_picture_is_averaged`).
    downscaled: std::cell::RefCell<HashMap<(MediaId, u32, u32), Arc<RenderImage>>>,
    /// Decoding the pictures and fonts, on other threads.
    pub(crate) preload: crate::render::preload::Preload,
    /// Formats of all media, for labelling pictures that cannot be drawn.
    formats: HashMap<MediaId, MediaFormat>,
    sizes: HashMap<MediaId, (u32, u32)>,
    fonts: Fonts,
    text_system: Arc<TextSystem>,
    /// Invalidates worksheet layouts when document fonts/resources change.
    pub(crate) layout_generation: usize,
    /// Ascent, descent and line gap of fonts, in ems.
    metrics: std::cell::RefCell<HashMap<Font, (f32, f32, f32)>>,
    row_metrics: std::cell::RefCell<HashMap<String, Option<std::rc::Rc<crate::render::font_metrics::WinRowMetrics>>>>,
    /// Whether the installed font of a family supports the Japanese code page (`font_metrics::font_file_has_japanese`).
    japanese: std::cell::RefCell<HashMap<String, Option<bool>>>,
    /// Draw as printed: sheets without their grid lines.
    pub(crate) print: bool,
    /// The slide show is drawn: an animated GIF plays (elsewhere it shows its first frame).
    pub(crate) animate_pictures: std::cell::Cell<bool>,
    /// The GIFs put in after the file was opened (not decoded ahead), decoded with all their frames when the slide
    /// show first draws them.
    live_gifs: std::cell::RefCell<HashMap<MediaId, Arc<RenderImage>>>,
    /// 印刷オプション: 背景の色とイメージを印刷する (DOCX-OPTIONS-010).
    pub(crate) print_backgrounds: bool,
    /// 印刷オプション: 隠し文字を印刷する (DOCX-OPTIONS-010).
    pub(crate) print_hidden_text: bool,
    /// 印刷オプション: 変更履歴とコメントを印刷する (DOCX-SEC-014).
    pub(crate) print_markup: bool,
    /// Word documents: hide document body text during header/footer editing (Word Header & Footer Tools > オプション > 文書内のテキストを表示).
    pub(crate) hide_document_text: bool,
    /// Word documents: the page is shown dark (表示 > ダークモード: a dark paper, the black of text and lines light), on the screen only.
    pub(crate) dark_page: bool,
    /// Word documents: the headings (paragraph ids) shown collapsed; what is under them is
    /// not laid out (the navigation pane's 折りたたみ).
    pub(crate) collapsed_headings: Arc<std::collections::BTreeSet<u32>>,
    /// Word documents: the values the merge fields show (差し込み印刷's 結果のプレビュー); `None`: the field codes' own text.
    pub(crate) merge_values: Option<Arc<std::collections::HashMap<String, String>>>,
    /// The scale a printed sheet is drawn at on its paper (for where Excel
    /// starts its dashed borders), set by the export before each sheet.
    pub(crate) print_scale: std::cell::Cell<f64>,
    /// Where each printed page of the sheet starts (row tops and column lefts in EMU of the sheet),
    /// set by the export with the scale: a fill does not reach back over the page's edge.
    pub(crate) print_breaks: std::cell::RefCell<Arc<(Vec<i64>, Vec<i64>)>>,
    /// Draw shapes where they are meant to be, as a PDF rasterizer draws
    /// them (the export's pictures, which the coverage tests compare with
    /// PowerPoint's PDF rasterized): level and upright edges are not moved
    /// onto whole pixels as the screen has them (crisp, as PowerPoint draws
    /// its window), and rectangles are drawn as paths (GPUI puts the
    /// quads it paints on whole pixels).
    pub(crate) exact: bool,
    /// The document's body font set, for text with no typeface of its own.
    theme_fonts: Option<ibis_doc_model::FontSet>,
    /// The frame of each video playing now, drawn in place of its poster.
    live: std::cell::RefCell<HashMap<MediaId, LiveFrame>>,
    /// Worksheets in the workbook by sheet name (original and lower-case).
    pub(crate) sheets: HashMap<String, Arc<ibis_doc_model::Sheet>>,
    /// Word documents: the text box currently being edited, its recorded `TextPositions`,
    /// and its composing `TextBody` when IME marked text is active.
    pub(crate) edited_box: Option<(
        ibis_doc_model::ShapeId,
        crate::render::rich_text::TextPositions,
        Option<ibis_doc_model::TextBody>,
    )>,
}

impl Resources {
    pub(crate) fn new(presentation: &Presentation, cx: &App) -> Self {
        static NEXT_LAYOUT_GENERATION: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(1);
        let layout_generation = NEXT_LAYOUT_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // (The text of metafile pictures is put by the metrics of the fonts: before the pictures are made.)
        crate::render::metafile_fonts::install(cx);
        let t = crate::clock::debug_timing::start();
        let preload = crate::render::preload::Preload::for_presentation(presentation);
        crate::clock::debug_timing::report("resources: preload", t);
        let t = crate::clock::debug_timing::start();
        let mut images = HashMap::new();
        let mut sources = HashMap::new();
        for (id, (image, source)) in preload.prepared_now() {
            images.insert(id, image);
            if let Some(source) = source {
                sources.insert(id, source);
            }
        }
        crate::clock::debug_timing::report("resources: prepared", t);
        let t = crate::clock::debug_timing::start();
        let formats = presentation.media.iter().map(|m| (m.id, m.format)).collect();
        let sizes = presentation
            .media
            .iter()
            .filter_map(|m| crate::pptx::editing::picture_size(&m.data).map(|s| (m.id, s)))
            .collect();
        crate::clock::debug_timing::report("resources: sizes", t);
        let t = crate::clock::debug_timing::start();
        let mut fonts = Fonts::new(cx);
        fonts.substitutions = presentation.font_substitutions.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        // A presentation's text; Word's and Excel's draw such a typeface otherwise (`Fonts::family`).
        fonts.calibri_for_missing = presentation.flow.is_none() && presentation.workbook.is_none();
        fonts.docx_font_families = presentation.flow.as_ref().map(|f| f.font_families.clone());
        crate::clock::debug_timing::report("resources: Fonts::new", t);
        let t = crate::clock::debug_timing::start();
        fonts.add_embedded(&presentation.fonts, &preload, cx);
        crate::clock::debug_timing::report("resources: add_embedded", t);
        let text_system = cx.text_system().clone();
        let mut sheets = HashMap::new();
        if let Some(ref wb) = presentation.workbook {
            for ws in &wb.sheets {
                let sheet_arc = if let Some(slide_id) = ws.slide {
                    presentation.slide(slide_id).and_then(|slide| {
                        slide.shapes.iter().find_map(|sh| {
                            if let ibis_doc_model::ShapeKind::Sheet(s) = &sh.kind {
                                Some(s.clone())
                            } else {
                                None
                            }
                        })
                    })
                } else {
                    ws.hidden.clone()
                };
                if let Some(s) = sheet_arc {
                    sheets.insert(ws.name.clone(), s.clone());
                    sheets.insert(ws.name.to_lowercase(), s);
                }
            }
        }
        Self {
            color_mode: ColorMode::Color,
            images,
            sources: std::cell::RefCell::new(sources),
            preload,
            rotated: Default::default(),
            clipped: Default::default(),
            duotones: Default::default(),
            rasters: Default::default(),
            downscaled: Default::default(),
            formats,
            sizes,
            fonts,
            text_system,
            layout_generation,
            metrics: Default::default(),
            row_metrics: Default::default(),
            japanese: Default::default(),
            print: false,
            animate_pictures: std::cell::Cell::new(false),
            live_gifs: Default::default(),
            print_backgrounds: false,
            print_hidden_text: false,
            print_markup: true,
            hide_document_text: false,
            dark_page: false,
            collapsed_headings: Default::default(),
            merge_values: None,
            print_scale: std::cell::Cell::new(1.0),
            print_breaks: Default::default(),
            exact: false,
            theme_fonts: presentation.theme_fonts.as_ref().map(|t| t.minor.clone()),
            live: Default::default(),
            sheets,
            edited_box: None,
        }
    }

    /// The frame of the video of clip `media` to draw now (`None`: its poster).
    pub(crate) fn set_live_frame(&self, media: MediaId, frame: Option<crate::media_player::Frame>) {
        #[cfg(all(windows,feature="export"))]
        let frame=frame.and_then(|f|image::RgbaImage::from_raw(f.width,f.height,(*f.rgba).clone()).map(|rgba|Arc::new(RenderImage::new([image::Frame::new(rgba)]))));
        match frame {
            Some(frame) => self.live.borrow_mut().insert(media, frame),
            None => self.live.borrow_mut().remove(&media),
        };
    }

    /// Whether a frame of the video of clip `media` is being drawn.
    pub(crate) fn has_live_frame(&self, media: MediaId) -> bool {
        self.live.borrow().contains_key(&media)
    }

    #[cfg(any(target_os = "macos",all(windows,feature="export")))]
    fn live_frame(&self, media: MediaId) -> Option<LiveFrame> {
        self.live.borrow().get(&media).cloned()
    }

    /// A picture put in the document after it was opened (inserted), for
    /// GPUI to decode.
    pub(crate) fn add_picture(&mut self, media: MediaId, format: MediaFormat, data: &[u8]) {
        let image_format = match format {
            MediaFormat::Png => ImageFormat::Png,
            MediaFormat::Jpeg => ImageFormat::Jpeg,
            MediaFormat::Gif => ImageFormat::Gif,
            MediaFormat::Bmp => ImageFormat::Bmp,
            MediaFormat::Tiff => ImageFormat::Tiff,
            MediaFormat::Webp => ImageFormat::Webp,
            MediaFormat::Svg => ImageFormat::Svg,
            _ => return,
        };
        self.images.insert(media, Arc::new(Image::from_bytes(image_format, data.to_vec())));
        self.formats.insert(media, format);
        if let Some(s) = crate::pptx::editing::picture_size(data) {
            self.sizes.insert(media, s);
        }
    }

    /// The original pixel dimensions of `media`.
    pub(crate) fn image_size(&self, media: MediaId) -> Option<(u32, u32)> {
        self.sizes.get(&media).copied()
    }

    /// Whether `media` is a picture that can be drawn or is preparing.
    pub(crate) fn has_picture(&self, media: MediaId) -> bool {
        self.images.contains_key(&media) || self.preload.is_image_supported(media)
    }

    /// The encoded SVG source of `media`, on demand.
    pub(crate) fn source(&self, media: MediaId) -> Option<(crate::render::picture_svg::Encoding, Arc<[u8]>)> {
        if let Some(source) = self.sources.borrow().get(&media) {
            return Some(source.clone());
        }
        if let Some((_, Some(source))) = self.preload.prepared_one(media) {
            self.sources.borrow_mut().insert(media, source.clone());
            return Some(source);
        }
        None
    }

    /// The picture to draw for `media`: decoded, or left to GPUI to load.
    /// `None` while it is still decoding (it is drawn when it is ready) or
    /// when it cannot be drawn.
    pub(crate) fn picture(&self, media: MediaId) -> Option<ImageSource> {
        match self.preload.decoded(media) {
            Ok(Some(image)) => Some(ImageSource::Render(image)),
            Ok(None) => None,
            Err(()) => {
                let image = self.images.get(&media).cloned().or_else(|| self.preload.prepared_one(media).map(|(img, _)| img))?;
                // A GIF put in after the file was opened plays in the slide show as one read from the file does.
                if self.animate_pictures.get() && self.formats.get(&media) == Some(&MediaFormat::Gif) {
                    if let Some(frames) = self.live_gifs.borrow().get(&media) {
                        return Some(ImageSource::Render(frames.clone()));
                    }
                    if let Ok(frames) = image.to_image_data(SvgRenderer::new(Arc::new(()))) {
                        self.live_gifs.borrow_mut().insert(media, frames.clone());
                        return Some(ImageSource::Render(frames));
                    }
                }
                Some(ImageSource::Image(image))
            }
        }
    }

    /// Sample the whole source at the density required by its visible cropped region.
    pub(crate) fn picture_at_crop_size(&self, media: MediaId, w: f32, h: f32, crop: ibis_doc_model::Crop) -> Option<ImageSource> {
        let visible_w = 1. - (crop.left as f32 + crop.right as f32) / 100_000.;
        let visible_h = 1. - (crop.top as f32 + crop.bottom as f32) / 100_000.;
        if visible_w <= 0. || visible_h <= 0. { return self.picture(media); }
        self.picture_at_size(media, w / visible_w, h / visible_h)
    }

    /// `picture(media)`, averaged down to the drawn size `(w, h)` (logical pixels) when the image is more than twice as
    /// large, so GPUI's 2×2 bilinear sampler does not drop thin lines between samples.
    pub(crate) fn picture_at_size(&self, media: MediaId, w: f32, h: f32) -> Option<ImageSource> {
        let base = self.picture(media)?;
        let ImageSource::Render(render) = &base else { return Some(base) };
        if render.frame_count() != 1 {
            return Some(base);
        }
        let size = render.size(0);
        let (sw, sh) = (u32::from(size.width.0.max(0) as u32), u32::from(size.height.0.max(0) as u32));
        // Account for up to 2× Retina scale factor so the texture fed to the 2×2 bilinear sampler is at most ~2× the device pixel size.
        let tw = (w * 2.0).round().max(1.0) as u32;
        let th = (h * 2.0).round().max(1.0) as u32;
        if sw <= tw * 2 && sh <= th * 2 {
            return Some(base);
        }
        let key = (media, tw, th);
        if let Some(found) = self.downscaled.borrow().get(&key) {
            return Some(ImageSource::Render(found.clone()));
        }
        // The browser version has no `image` crate (not a dependency for wasm): drawn as it is there.
        #[cfg(target_family = "wasm")]
        return Some(base);
        #[cfg(not(target_family = "wasm"))]
        {
            let bytes = render.as_bytes(0)?;
            let buf = image::RgbaImage::from_raw(sw, sh, bytes.to_vec())?;
            let resized = image::imageops::resize(&buf, tw, th, image::imageops::FilterType::Triangle);
            let out = Arc::new(RenderImage::new([image::Frame::new(resized)]));
            self.downscaled.borrow_mut().insert(key, out.clone());
            Some(ImageSource::Render(out))
        }
    }

    /// An SVG picture (rotated, cut or recoloured) to draw: rasterized
    /// ahead, or left to GPUI to rasterize.
    pub(crate) fn svg_picture(&self, image: Arc<Image>) -> ImageSource {
        match self.rasters.borrow().get(&image.id()) {
            Some(raster) => ImageSource::Render(raster.clone()),
            None => ImageSource::Image(image),
        }
    }

    /// Rasterize the SVG pictures made so far, on several threads. An
    /// export does so before drawing: GPUI would rasterize them one after
    /// another (its headless executor runs on one thread), at the size of
    /// the picture itself.
    #[cfg(headless_export)]
    pub(crate) fn rasterize_svg_pictures(&self, renderer: &SvgRenderer) {
        let mut images: Vec<Arc<Image>> = self.rotated.borrow().values().map(|(image, _)| image.clone()).collect();
        images.extend(self.clipped.borrow().values().cloned());
        images.extend(self.duotones.borrow().values().cloned());
        images.extend(crate::docx::art_border::all_images());
        let done = self.rasters.borrow();
        images.retain(|image| !done.contains_key(&image.id()));
        drop(done);
        images.sort_by_key(|image| image.id());
        images.dedup_by_key(|image| image.id());
        if images.is_empty() {
            return;
        }
        #[cfg(target_family = "wasm")]
        let threads = 1;
        #[cfg(not(target_family = "wasm"))]
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8).min(images.len());
        if threads <= 1 {
            let rasters: Vec<(u64, Arc<RenderImage>)> = images
                .iter()
                .filter_map(|image| image.to_image_data(renderer.clone()).ok().map(|raster| (image.id(), raster)))
                .collect();
            self.rasters.borrow_mut().extend(rasters);
            return;
        }
        let next = std::sync::atomic::AtomicUsize::new(0);
        let rasters: Vec<(u64, Arc<RenderImage>)> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|_| {
                    scope.spawn(|| {
                        let mut done = Vec::new();
                        while let Some(image) = images.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)) {
                            if let Ok(raster) = image.to_image_data(renderer.clone()) {
                                done.push((image.id(), raster));
                            }
                        }
                        done
                    })
                })
                .collect();
            workers.into_iter().flat_map(|w| w.join().unwrap_or_default()).collect()
        });
        self.rasters.borrow_mut().extend(rasters);
    }

    /// The ascent and descent of `font`, in ems.
    pub(crate) fn metrics(&self, font: &Font) -> (f32, f32) {
        let (a, d, _) = self.line_metrics(font);
        (a, d)
    }

    /// Whether the typeface `name` is installed (or embedded), under its own name or one this system gives it.
    pub(crate) fn has_typeface(&self, name: &str) -> bool {
        self.fonts.available(name).is_some()
    }

    pub(crate) fn text_system(&self) -> &Arc<TextSystem> {
        &self.text_system
    }

    /// The width of `text` in `font` at `size` pixels, from the characters'
    /// advances (text is laid out without kerning).
    pub(crate) fn text_width(&self, text: &str, font: &Font, size: f32) -> f32 {
        let id = self.text_system.resolve_font(font);
        text.chars()
            .map(|c| match self.text_system.advance(id, px(size), c) {
                Ok(advance) => f32::from(advance.width),
                Err(_) if c as u32 >= 0x2e80 => size,
                Err(_) => size * 0.5,
            })
            .sum()
    }

    /// Whether the font of typeface `name` says it supports the Japanese code page (see `font_metrics::font_file_has_japanese`); `None` when
    /// the typeface is not installed (another font would stand in for it, and its file says nothing about the typeface).
    pub(crate) fn is_japanese_typeface(&self, name: &str) -> Option<bool> {
        if let Some(&v) = self.japanese.borrow().get(name) {
            return v;
        }
        let v = self.fonts.available(name).map(|_| {
            let font = self.font(&TextStyle { font: Some(name.to_string()), ..TextStyle::default() });
            let id = self.text_system.resolve_font(&font);
            self.text_system.font_data(id).is_some_and(|(data, index)| crate::render::font_metrics::font_file_has_japanese(&data, index))
        });
        self.japanese.borrow_mut().insert(name.to_string(), v);
        v
    }

    /// What Microsoft Excel's row height follows (`font_metrics::WinRowMetrics`) in the file of the
    /// installed typeface `name`; `None` when it is not installed (a stand-in's file would give the stand-in's rows) or its file
    /// cannot be read.
    pub(crate) fn win_row_metrics(&self, name: &str) -> Option<std::rc::Rc<crate::render::font_metrics::WinRowMetrics>> {
        if let Some(m) = self.row_metrics.borrow().get(name) {
            return m.clone();
        }
        let m = (|| {
            if !self.has_typeface(name) {
                return None;
            }
            let font = self.font(&TextStyle { font: Some(name.into()), ..TextStyle::default() });
            let id = self.text_system.resolve_font(&font);
            let (data, index) = self.text_system.font_data(id)?;
            crate::render::font_metrics::font_file_win_row_metrics(&data, index).map(std::rc::Rc::new)
        })();
        self.row_metrics.borrow_mut().insert(name.to_string(), m.clone());
        m
    }

    /// The ascent, descent and line gap of `font`, in ems.
    pub(crate) fn line_metrics(&self, font: &Font) -> (f32, f32, f32) {
        if let Some(&m) = self.metrics.borrow().get(font) {
            return m;
        }
        let id = self.text_system.resolve_font(font);
        let em = px(1000.0);
        let line_gap = f32::from(self.text_system.line_gap(id, em)).max(0.0) / 1000.0;
        // The ascent and descent the Office apps take (`font_metrics`): from
        // the font's file, else those of the typeface it is (macOS's
        // YuGothic stands for 游ゴシック), else the text system's.
        let (ascent, descent) = self
            .text_system
            .font_data(id)
            .and_then(|(data, index)| crate::render::font_metrics::font_file_metrics(&data, index))
            .filter(|_| !STAND_INS.contains(&font.family.as_ref()))
            .or_else(|| crate::render::font_metrics::office_metrics(&font.family))
            .unwrap_or_else(|| (f32::from(self.text_system.ascent(id, em)).abs() / 1000.0, f32::from(self.text_system.descent(id, em)).abs() / 1000.0));
        let m = (ascent, descent, line_gap);
        if std::env::var_os("IBIS_DEBUG_METRICS").is_some() {
            eprintln!("metrics {} {:?}: {m:?}", font.family, font.weight);
        }
        self.metrics.borrow_mut().insert(font.clone(), m);
        m
    }

    /// How far to move a line of `font` at `size` down from where GPUI puts
    /// it in a line box `line` high (GPUI centres the font's height in the
    /// box) to where PowerPoint does (`rich_text::baseline`).
    pub(crate) fn baseline_shift(&self, font: &Font, size: f32, line: f32) -> f32 {
        let (a, d) = self.metrics(font);
        // GPUI's baseline: the font's height centred in the line.
        let gpui = (line - (a + d) * size) / 2.0 + a * size;
        crate::render::font_metrics::powerpoint_baseline(line, LINE_HEIGHT * size, a, d) - gpui
    }

    /// How much smaller to draw `font`, the font standing in for the
    /// typeface `requested` when that is not installed, so that it takes
    /// the room the typeface would: the ratio of their digits' widths. 1
    /// when the typeface is installed or its widths are not known.
    pub(crate) fn substitute_scale(&self, requested: Option<&str>, font: &Font) -> f32 {
        let Some(name) = requested else { return 1.0 };
        if self.fonts.available(name).is_some() {
            return 1.0;
        }
        // Digit widths in ems, as Office prints them. (Calibri, 0.507, is
        // left out: its substitute drawn to its width looks smaller than it,
        // and matched its PDF worse.)
        let expected = match name {
            n if n.starts_with("ＭＳ") || n.starts_with("MS ") => if ibis_pptx::EXCEL_METRICS == ibis_pptx::ExcelMetrics::Windows { 0.5 } else { 0.54 }, // i18n-ok（フォント名の照合（ＭＳ で始まる名前の判定）。ファイルの中の名前。i18n-ok）
            // Noto Sans JP's digits (0.555 em, from the widths in Excel's PDF of a user's workbook).
            n if n.starts_with("Noto Sans") => 0.555,
            _ => return 1.0,
        };
        let actual = self.text_width("0", font, 100.0) / 100.0;
        if actual <= 0.0 { 1.0 } else { (expected / actual).min(1.0) }
    }

    /// `substitute_scale` for a text of Japanese characters: the ratio of
    /// the widths of the text in the typeface (ＭＳ Ｐゴシック: kanji and
    /// fullwidth signs 1 em, kana 0.86, digits 0.54, other Latin 0.55, signs
    /// and spaces about 0.4; measured on Excel's PDF of a chart of titles and
    /// category names) and in the font standing in, not of their digits (which
    /// would draw kanji at 0.76 of their size).
    pub(crate) fn substitute_scale_for(&self, requested: Option<&str>, font: &Font, sample: &str) -> f32 {
        let Some(name) = requested else { return 1.0 };
        if !(name.starts_with("ＭＳ") || name.starts_with("MS ")) || sample.is_ascii() || self.fonts.available(name).is_some() { // i18n-ok（フォント名の照合（ＭＳ で始まる名前の判定）。i18n-ok）
            return self.substitute_scale(requested, font);
        }
        let expected: f32 = sample
            .chars()
            .map(|c| match c {
                '0'..='9' => 0.54,
                'a'..='z' | 'A'..='Z' => 0.55,
                ' ' | ',' | '.' | '/' | ':' | '-' | '(' | ')' | '%' => 0.4,
                '\u{3040}'..='\u{30ff}' => 0.86,
                '（' | '）' | '「' | '」' | '、' | '。' => 0.5,
                _ => 1.0,
            })
            .sum();
        let actual = self.text_width(sample, font, 100.0) / 100.0;
        if actual <= 0.0 { 1.0 } else { (expected / actual).clamp(0.5, 1.0) }
    }

    /// `style` for `text`, a run with no typeface of its own taking the
    /// theme's body font for its script (as PowerPoint's `+mn-lt` / `+mn-ea`).
    pub(crate) fn with_theme_font(&self, mut style: TextStyle, text: &str) -> TextStyle {
        if style.font.is_none()
            && let Some(fonts) = &self.theme_fonts
        {
            style.font = fonts.for_text(text).map(str::to_string);
            style.fallback_font = fonts.other_for_text(text).map(str::to_string);
        }
        style
    }

    /// The installed font to draw `style` with.
    pub(crate) fn font(&self, style: &TextStyle) -> Font {
        self.fonts.font(style)
    }

    /// The font a slide's East Asian characters are drawn in when their
    /// run (`style`) has no East Asian typeface: ＭＳ ゴシック, as PowerPoint
    /// draws them (measured on its PDF: runs of `lang="en-US"` with no
    /// `ea` font), when it is installed; `None` otherwise (the system's
    /// font draws them, as before).
    pub(crate) fn default_east_asian_font(&self, style: &TextStyle) -> Option<Font> {
        if style.font.is_none() || style.font.as_deref().is_some_and(is_east_asian_typeface) {
            return None;
        }
        if let Some(ea) = style.fallback_font.as_deref().filter(|f| is_east_asian_typeface(f)) {
            return Some(self.fonts.font(&TextStyle { font: Some(ea.into()), fallback_font: style.font.clone(), ..style.clone() }));
        }
        if style.bold
            && let Some(ea) = self.fonts.east_asian_fallback.as_deref()
        {
            return Some(self.fonts.font(&TextStyle { font: Some(ea.into()), fallback_font: None, ..style.clone() }));
        }
        self.fonts.available("MS Gothic")?;
        Some(self.fonts.font(&TextStyle { font: Some("MS Gothic".into()), fallback_font: None, ..style.clone() }))
    }
}

/// Maps the typefaces a document asks for to installed fonts.
struct Fonts {
    installed: HashSet<String>,
    /// OpenType `name` (ID 1 / ID 16 / ID 4) and `OS/2` weight index for
    /// fonts embedded in the current document.
    embedded_index: OpenTypeIndex,
    /// Used for East Asian typefaces that are not installed.
    east_asian_fallback: Option<SharedString>,
    substitutions: HashMap<String, String>,
    /// Whether a Latin typeface that is not installed is drawn with Calibri (PowerPoint's choice; see `Fonts::family`).
    calibri_for_missing: bool,
    /// A Word document's font table (typeface to `w:family`), where one is read: see `family`.
    docx_font_families: Option<std::collections::BTreeMap<String, String>>,
}

/// Weight words at the end of a typeface name, such as "Poppins SemiBold"
/// or GDI sub-family names like "Segoe UI Semilight".
/// Compound weight words ("ExtraLight", "SemiLight", "SemiBold", "ExtraBold"...)
/// are listed before their shorter suffixes ("Light", "Bold") so suffix
/// stripping matches the full weight word first.
pub(crate) const WEIGHT_SUFFIXES: &[(&str, FontWeight)] = &[
    ("ExtraLight", FontWeight::EXTRA_LIGHT),
    ("Extralight", FontWeight::EXTRA_LIGHT),
    ("UltraLight", FontWeight::EXTRA_LIGHT),
    ("Ultralight", FontWeight::EXTRA_LIGHT),
    ("SemiLight", FontWeight(350.0)),
    ("Semilight", FontWeight(350.0)),
    ("DemiLight", FontWeight(350.0)),
    ("Demilight", FontWeight(350.0)),
    ("Thin", FontWeight::THIN),
    ("Light", FontWeight::LIGHT),
    ("Regular", FontWeight::NORMAL),
    ("Normal", FontWeight::NORMAL),
    ("Book", FontWeight::NORMAL),
    ("Medium", FontWeight::MEDIUM),
    ("SemiBold", FontWeight::SEMIBOLD),
    ("Semibold", FontWeight::SEMIBOLD),
    ("DemiBold", FontWeight::SEMIBOLD),
    ("Demibold", FontWeight::SEMIBOLD),
    ("ExtraBold", FontWeight::EXTRA_BOLD),
    ("Extrabold", FontWeight::EXTRA_BOLD),
    ("UltraBold", FontWeight::EXTRA_BOLD),
    ("Ultrabold", FontWeight::EXTRA_BOLD),
    ("Bold", FontWeight::BOLD),
    ("Black", FontWeight::BLACK),
    ("Heavy", FontWeight::BLACK),
    // Hiragino's weights ("Hiragino Kaku Gothic Std W8").
    ("W0", FontWeight(100.0)),
    ("W1", FontWeight(200.0)),
    ("W2", FontWeight(250.0)),
    ("W3", FontWeight(300.0)),
    ("W4", FontWeight(400.0)),
    ("W5", FontWeight(500.0)),
    ("W6", FontWeight(600.0)),
    ("W7", FontWeight(700.0)),
    ("W8", FontWeight(800.0)),
    ("W9", FontWeight(900.0)),
    // Ricoh HG/HGP/HGS and other Japanese typeface weight suffixes ("HGP創英角ｺﾞｼｯｸUB", "HGPｺﾞｼｯｸE").
    ("UB", FontWeight::EXTRA_BOLD),
    ("EB", FontWeight::EXTRA_BOLD),
];

/// Strip a trailing weight word from `name` (either an exact suffix in
/// `WEIGHT_SUFFIXES` or a case-insensitive weight word after `' '` or `'-'`),
/// returning the base family name and its `FontWeight`.
pub(crate) fn strip_weight_suffix(name: &str) -> Option<(&str, FontWeight)> {
    let name = name.trim();
    for &(suffix, weight) in WEIGHT_SUFFIXES {
        if let Some(stem) = name.strip_suffix(suffix) {
            let base = stem.trim_end_matches([' ', '-']);
            if !base.is_empty() {
                return Some((base, weight));
            }
        }
    }
    // Single-letter Japanese typeface weight suffixes ("HGPｺﾞｼｯｸE", "HG明朝B", "HG丸ｺﾞｼｯｸM-PRO")
    // directly after a non-ASCII character.
    for &(suffix, weight) in &[("E", FontWeight::EXTRA_BOLD), ("B", FontWeight::BOLD), ("M", FontWeight::MEDIUM), ("L", FontWeight::LIGHT)] {
        if let Some(stem) = name.strip_suffix(suffix)
            && stem.chars().last().is_some_and(|c| !c.is_ascii())
        {
            return Some((stem, weight));
        }
    }
    if let Some((stem, last_word)) = name.rsplit_once([' ', '-']) {
        let base = stem.trim_end_matches([' ', '-']);
        if !base.is_empty()
            && let Some(&(_, weight)) = WEIGHT_SUFFIXES.iter().find(|(suffix, _)| last_word.eq_ignore_ascii_case(suffix))
        {
            return Some((base, weight));
        }
    }
    // A Japanese typeface's regular and bold faces are named with a hyphen and one letter after the family ("UD デジタル 教科書体 NK-R", "-B"; the font's own
    // `name` table has the family without it). Looked up only when the name itself is not found (`resolve_font_with_index` tries the name first).
    if let Some((stem, letter)) = name.rsplit_once('-')
        && !stem.trim().is_empty()
        && stem.chars().any(|c| !c.is_ascii())
    {
        match letter {
            "R" => return Some((stem.trim_end(), FontWeight::NORMAL)),
            "B" => return Some((stem.trim_end(), FontWeight::BOLD)),
            _ => {}
        }
    }
    None
}

/// Installed fonts that stand for a typeface Office has in other files,
/// whose ascent and descent are the typeface's (`font_metrics`), not their
/// own: macOS's 游ゴシック and 游明朝 (0.88 / 0.12 em against Windows' 0.985 /
/// 0.302).
const STAND_INS: &[&str] = &["YuGothic", "YuMincho"];

/// Office's names of typefaces and the names other systems install them
/// under (macOS calls 游ゴシック "YuGothic").
const TYPEFACE_ALIASES: &[(&str, &str)] = &[
    // Windows' 游ゴシック first: it is what Office draws with (Office for
    // Mac brings it), before macOS' own (heavier) one.
    ("游ゴシック", "Yu Gothic"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("游ゴシック", "YuGothic"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("Yu Gothic", "YuGothic"),
    ("游明朝", "Yu Mincho"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("游明朝", "YuMincho"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ角ゴ Pro", "Hiragino Kaku Gothic Pro"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ角ゴ ProN", "Hiragino Kaku Gothic ProN"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ角ゴ Std", "Hiragino Kaku Gothic Std"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ角ゴ StdN", "Hiragino Kaku Gothic StdN"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ角ゴシック", "Hiragino Sans"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ丸ゴ Pro", "Hiragino Maru Gothic Pro"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ丸ゴ ProN", "Hiragino Maru Gothic ProN"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ明朝 Pro", "Hiragino Mincho Pro"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ヒラギノ明朝 ProN", "Hiragino Mincho ProN"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("Yu Mincho", "YuMincho"),
    // Century is not on macOS; Times New Roman is the nearest serif.
    ("Century", "Times New Roman"),
    // Calibri is not on macOS; Carlito, bundled (`BUNDLED_FONTS`), has its
    // widths.
    ("Calibri", "Carlito"),
    // Cambria Math is Office's (and not on macOS); STIX Two Math and STIXGeneral, which macOS has, are serifs with the maths glyphs.
    ("Cambria Math", "STIX Two Math"),
    ("Cambria Math", "STIXGeneral"),
    ("メイリオ", "Meiryo"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ＭＳ Ｐゴシック", "MS PGothic"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ＭＳ ゴシック", "MS Gothic"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ＭＳ Ｐ明朝", "MS PMincho"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    ("ＭＳ 明朝", "MS Mincho"), // i18n-ok（フォント名の対応表（ファイルに保存されるフォント名）。font_label の対）
    // Windows' FontSubstitutes (HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\FontSubstitutes): the names it maps to
    // installed typefaces, which Office follows.
    ("Helvetica", "Arial"),
    ("Times", "Times New Roman"),
    // Linux has none of Office's typefaces; the Liberation fonts (Liberation
    // Serif, Sans and Mono: Tinos, Arimo and Cousine under ChromeOS' names)
    // and Caladea have the widths of Times New Roman, Arial, Courier New
    // and Cambria, and DejaVu Math TeX Gyre and Latin Modern Math are maths
    // fonts.
    ("Times New Roman", "Liberation Serif"),
    ("Times New Roman", "Tinos"),
    ("Century", "Liberation Serif"),
    ("Arial", "Liberation Sans"),
    ("Arial", "Arimo"),
    ("Helvetica", "Liberation Sans"),
    ("Arial Narrow", "Liberation Sans Narrow"),
    ("Courier New", "Liberation Mono"),
    ("Courier New", "Cousine"),
    ("Cambria", "Caladea"),
    ("Cambria Math", "DejaVu Math TeX Gyre"),
    ("Cambria Math", "Latin Modern Math"),
];

/// For the web build: the typefaces the document asks for, under the names
/// browsers know them by, for the text the bundled fonts lack (Japanese),
/// which the browser draws with its own fonts. Without them it takes its
/// default Japanese font, whose widths differ from the document's
/// (a company name on a slide of a user's private presentation, sized to
/// Hiragino, wrapped).
#[cfg(target_family = "wasm")]
fn browser_fallbacks(style: &TextStyle) -> Option<FontFallbacks> {
    let mut names: Vec<String> = Vec::new();
    for name in [style.font.as_deref(), style.fallback_font.as_deref()].into_iter().flatten() {
        let base = WEIGHT_SUFFIXES.iter().find_map(|(suffix, _)| name.strip_suffix(suffix)).map_or(name, str::trim_end);
        for n in [name, base] {
            if !names.iter().any(|x| x == n) {
                names.push(n.to_string());
            }
            for (_, other) in TYPEFACE_ALIASES.iter().filter(|(office, _)| *office == n) {
                if !names.iter().any(|x| x == other) {
                    names.push(other.to_string());
                }
            }
        }
        if is_east_asian_typeface(name) {
            // Close in widths to Office's Japanese typefaces.
            for n in ["Hiragino Kaku Gothic ProN", "Hiragino Sans", "Yu Gothic", "YuGothic", "Meiryo", "Noto Sans JP"] {
                if !names.iter().any(|x| x == n) {
                    names.push(n.to_string());
                }
            }
        }
    }
    (!names.is_empty()).then(|| FontFallbacks::from_fonts(names))
}

#[cfg(not(target_family = "wasm"))]
fn browser_fallbacks(_: &TextStyle) -> Option<FontFallbacks> {
    None
}

/// Typefaces made for Japanese (or other East Asian) text.
pub(crate) fn is_east_asian_typeface(name: &str) -> bool {
    const HINTS: &[&str] = &[
        "M PLUS", "Noto Sans JP", "Noto Serif JP", "Noto Sans CJK", "Yu Gothic", "Yu Mincho",
        "Meiryo", "Hiragino", "MS Gothic", "MS Mincho", "MS PGothic", "BIZ UD", "Kosugi",
        "Sawarabi", "IPA", "Source Han",
    ];
    name.chars().any(|c| c as u32 >= 0x2e80) || HINTS.iter().any(|h| name.contains(h))
}

/// Whether an uninstalled Latin typeface name is a serif face (drawn with
/// Times New Roman / Liberation Serif rather than a sans-serif stand-in).
fn is_serif_latin_typeface(name: &str) -> bool {
    const HINTS: &[&str] = &[
        "Serif", "Goudy", "Merriweather", "Garamond", "Baskerville", "Palatino", "Bookman",
        "Caslon", "Bodoni", "Didot", "Cambria", "Constantia", "Georgia", "Charter", "Rockwell",
        "Slab", "Antiqua", "Minion", "Sabon", "Times",
    ];
    !name.contains("Sans") && HINTS.iter().any(|h| name.contains(h))
}

/// Fonts bundled with the desktop app (`fonts/carlito/`, OFL-1.1): Carlito,
/// with Calibri's widths, stands in for it. Not in the web build, whose
/// download it would add 2.7 MB to.
#[cfg(not(target_family = "wasm"))]
const BUNDLED_FONTS: &[&[u8]] = &[
    include_bytes!("../../fonts/carlito/Carlito-Regular.ttf"),
    include_bytes!("../../fonts/carlito/Carlito-Bold.ttf"),
    include_bytes!("../../fonts/carlito/Carlito-Italic.ttf"),
    include_bytes!("../../fonts/carlito/Carlito-BoldItalic.ttf"),
];
#[cfg(target_family = "wasm")]
const BUNDLED_FONTS: &[&[u8]] = &[];
/// The family of `BUNDLED_FONTS`.
const BUNDLED_FAMILY: &str = "Carlito";

/// For tests only: the fonts in the folders `IBIS_EXTRA_FONT_DIRS` names
/// (separated by `:`), e.g. the ones the real Office apps ship, so the
/// coverage tests (test/coverage/) draw with the same font files as the
/// reference and compare the drawing alone. Nothing happens without the
/// variable; the fonts are only held in memory.
#[cfg(not(target_family = "wasm"))]
fn add_extra_font_dirs(cx: &App, names: &mut HashSet<String>) {
    let Some(dirs) = std::env::var_os("IBIS_EXTRA_FONT_DIRS") else { return };
    let mut fonts = Vec::new();
    for dir in std::env::split_paths(&dirs) {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
            if matches!(ext.as_deref(), Some("ttf" | "otf" | "ttc"))
                && let Ok(data) = std::fs::read(&path)
            {
                crate::render::math_font::offer_font(&data);
                if ext.as_deref() == Some("ttc") {
                    // The text system reads the first font of a collection only: each is given on its own (Cambria Math is
                    // the second of Cambria's).
                    fonts.extend(split_collection(&data).into_iter().map(std::borrow::Cow::Owned));
                } else {
                    fonts.push(std::borrow::Cow::Owned(data));
                }
            }
        }
    }
    if fonts.is_empty() {
        return;
    }
    match cx.text_system().add_fonts(fonts) {
        Ok(()) => names.extend(cx.text_system().all_font_names()),
        Err(err) => log::warn!("IBIS_EXTRA_FONT_DIRS: fonts could not be loaded: {err}"),
    }
}

/// The fonts of a TrueType collection (`.ttc`) as one font file each (their tables copied).
#[cfg(not(target_family = "wasm"))]
fn split_collection(data: &[u8]) -> Vec<Vec<u8>> {
    let be32 = |at: usize| data.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let be16 = |at: usize| data.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize);
    if data.get(..4) != Some(b"ttcf") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..be32(8).unwrap_or(0) {
        let Some(font) = be32(12 + 4 * i) else { break };
        let Some(count) = be16(font + 4) else { break };
        let mut head = data.get(font..font + 12).map(<[u8]>::to_vec).unwrap_or_default();
        let mut tables: Vec<u8> = Vec::new();
        let mut records: Vec<u8> = Vec::new();
        let base = 12 + 16 * count;
        let mut ok = head.len() == 12;
        for t in 0..count {
            let at = font + 12 + 16 * t;
            let (Some(offset), Some(length), Some(tag)) = (be32(at + 8), be32(at + 12), data.get(at..at + 8)) else {
                ok = false;
                break;
            };
            let Some(body) = data.get(offset..offset + length) else {
                ok = false;
                break;
            };
            records.extend_from_slice(tag);
            records.extend_from_slice(&((base + tables.len()) as u32).to_be_bytes());
            records.extend_from_slice(&(length as u32).to_be_bytes());
            tables.extend_from_slice(body);
            tables.resize(tables.len().div_ceil(4) * 4, 0);
        }
        if ok {
            head.extend(records);
            head.extend(tables);
            out.push(head);
        }
    }
    out
}

#[cfg(target_family = "wasm")]
fn add_extra_font_dirs(_: &App, _: &mut HashSet<String>) {}

/// Names and style read from the `name` and `OS/2` tables of an OpenType /
/// TrueType font face.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OpenTypeFaceInfo {
    /// Typographic family names (Name ID 16).
    pub(crate) id16_names: Vec<String>,
    /// Font family / GDI family names (Name ID 1).
    pub(crate) id1_names: Vec<String>,
    /// Full font names (Name ID 4).
    pub(crate) id4_names: Vec<String>,
    /// `OS/2` `usWeightClass` (100..=1000, default 400).
    pub(crate) weight: u16,
    /// `OS/2` `fsSelection` italic or oblique flag.
    pub(crate) italic: bool,
    /// For a variable font with a weight axis: the weight its family is regular at (400 kept within the axis' range), which is not
    /// the weight of the file's default instance (`usWeightClass`; Noto Sans JP's file is its Thin).
    pub(crate) family_weight: Option<u16>,
}

#[derive(Clone, Debug)]
struct OpenTypeEntry {
    /// Candidate family names as registered by the platform's text system
    /// (Name ID 16 first, then Name ID 1).
    family_candidates: Vec<String>,
    /// `OS/2` `usWeightClass`.
    weight: u16,
    /// `OS/2` italic or oblique flag.
    italic: bool,
    /// 0 for Name ID 16 / Name ID 1, 1 for Name ID 4 (full font name).
    tier: u8,
}

impl OpenTypeEntry {
    fn resolve_in(&self, installed: &HashSet<String>) -> Option<SharedString> {
        for cand in &self.family_candidates {
            if let Some(exact) = installed.get(cand.as_str()) {
                return Some(SharedString::from(exact.clone()));
            }
        }
        for cand in &self.family_candidates {
            if let Some(found) = installed.iter().find(|f| f.eq_ignore_ascii_case(cand)) {
                return Some(SharedString::from(found.clone()));
            }
        }
        for cand in &self.family_candidates {
            if let Some(aliased) = alias_in_installed(cand, installed) {
                return Some(aliased);
            }
        }
        None
    }
}

/// Maps OpenType GDI family names (Name ID 1), typographic family names
/// (Name ID 16), and full names (Name ID 4) across locales to the family
/// names the platform's text system registers and the face's `OS/2` weight.
#[derive(Clone, Debug, Default)]
pub(crate) struct OpenTypeIndex {
    entries: HashMap<String, OpenTypeEntry>,
}

impl OpenTypeIndex {
    pub(crate) fn insert_face(&mut self, face: &OpenTypeFaceInfo) {
        let mut all_families: Vec<String> = Vec::with_capacity(face.id16_names.len() + face.id1_names.len());
        for n in face.id16_names.iter().chain(face.id1_names.iter()) {
            if !all_families.iter().any(|x| x == n) {
                all_families.push(n.clone());
            }
        }
        if all_families.is_empty() {
            return;
        }
        let id16_only: Vec<String> = if face.id16_names.is_empty() {
            all_families.clone()
        } else {
            face.id16_names.clone()
        };
        let family_weight = face.family_weight.unwrap_or(face.weight);
        for name16 in &face.id16_names {
            self.insert_candidate(name16, &id16_only, family_weight, face.italic, 0);
        }
        for name1 in &face.id1_names {
            self.insert_candidate(name1, &all_families, family_weight, face.italic, 0);
        }
        for name4 in &face.id4_names {
            self.insert_candidate(name4, &all_families, face.weight, face.italic, 1);
        }
    }

    pub(crate) fn insert_explicit(&mut self, query: &str, family: &str, weight: u16, italic: bool) {
        let query = query.trim();
        let family = family.trim();
        if query.is_empty() || family.is_empty() {
            return;
        }
        let mut cands = vec![family.to_string()];
        if !query.eq_ignore_ascii_case(family) {
            cands.push(query.to_string());
        }
        self.insert_candidate(query, &cands, weight, italic, 0);
    }

    fn insert_candidate(&mut self, key_name: &str, candidates: &[String], weight: u16, italic: bool, tier: u8) {
        let key = key_name.trim().to_lowercase();
        if key.is_empty() {
            return;
        }
        let new_score = (tier, italic, (i32::from(weight) - 400).abs());
        match self.entries.get_mut(&key) {
            Some(existing) => {
                for c in candidates {
                    if !existing.family_candidates.iter().any(|x| x == c) {
                        existing.family_candidates.push(c.clone());
                    }
                }
                let old_score = (existing.tier, existing.italic, (i32::from(existing.weight) - 400).abs());
                if new_score < old_score {
                    existing.weight = weight;
                    existing.italic = italic;
                    existing.tier = tier;
                }
            }
            None => {
                self.entries.insert(
                    key,
                    OpenTypeEntry {
                        family_candidates: candidates.to_vec(),
                        weight,
                        italic,
                        tier,
                    },
                );
            }
        }
    }

    fn get(&self, name: &str) -> Option<&OpenTypeEntry> {
        let key = name.trim().to_lowercase();
        self.entries.get(&key)
    }
}

fn parse_os2_table(os2: &[u8]) -> (u16, bool) {
    if os2.len() < 6 {
        return (400, false);
    }
    let raw_w = u16::from_be_bytes([os2[4], os2[5]]);
    let weight = if (1..=9).contains(&raw_w) {
        raw_w * 100
    } else if (100..=1000).contains(&raw_w) {
        raw_w
    } else {
        400
    };
    let italic = if os2.len() >= 64 {
        let fs_selection = u16::from_be_bytes([os2[62], os2[63]]);
        (fs_selection & 0x0201) != 0
    } else {
        false
    };
    (weight, italic)
}

fn parse_name_table(name: &[u8]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut id16_names = Vec::new();
    let mut id1_names = Vec::new();
    let mut id4_names = Vec::new();
    if name.len() < 6 {
        return (id16_names, id1_names, id4_names);
    }
    let count = u16::from_be_bytes([name[2], name[3]]) as usize;
    let storage = u16::from_be_bytes([name[4], name[5]]) as usize;
    for i in 0..count.min(512) {
        let rec = 6 + i * 12;
        let Some(r) = name.get(rec..rec + 12) else { break };
        let platform = u16::from_be_bytes([r[0], r[1]]);
        let encoding = u16::from_be_bytes([r[2], r[3]]);
        let name_id = u16::from_be_bytes([r[6], r[7]]);
        if !matches!(name_id, 1 | 4 | 16) {
            continue;
        }
        let len = u16::from_be_bytes([r[8], r[9]]) as usize;
        let off = storage.saturating_add(u16::from_be_bytes([r[10], r[11]]) as usize);
        let Some(raw) = name.get(off..off.saturating_add(len)) else { continue };
        let decoded = if platform == 0 || (platform == 3 && matches!(encoding, 1 | 10)) {
            if raw.is_empty() || raw.len() % 2 != 0 {
                None
            } else {
                let u16s: Vec<u16> = raw.chunks_exact(2).map(|b| u16::from_be_bytes([b[0], b[1]])).collect();
                String::from_utf16(&u16s).ok()
            }
        } else if platform == 1 && encoding == 0 && !raw.is_empty() && raw.is_ascii() {
            std::str::from_utf8(raw).ok().map(str::to_string)
        } else {
            None
        };
        let Some(s) = decoded else { continue };
        let s = s.trim_matches('\0').trim();
        if s.is_empty() || s.starts_with('.') {
            continue;
        }
        let target = match name_id {
            16 => &mut id16_names,
            1 => &mut id1_names,
            4 => &mut id4_names,
            _ => continue,
        };
        if !target.iter().any(|x| x == s) {
            target.push(s.to_string());
        }
    }
    (id16_names, id1_names, id4_names)
}

/// The weight a variable font's family is regular at: 400 kept within the range of the `wght` axis of its `fvar` table; `None` for a font with
/// no weight axis. The file's default instance is not it (Noto Sans JP's default is the Thin, its `usWeightClass` 100).
fn variable_family_weight(fvar: &[u8]) -> Option<u16> {
    let u16_at = |at: usize| fvar.get(at..at + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize);
    let fixed_at = |at: usize| fvar.get(at..at + 4).map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]) as f32 / 65536.0);
    let (axes_at, count, size) = (u16_at(4)?, u16_at(8)?, u16_at(10)?);
    for axis in 0..count {
        let at = axes_at + axis * size;
        if fvar.get(at..at + 4)? == b"wght" {
            let (min, max) = (fixed_at(at + 4)?, fixed_at(at + 12)?);
            return Some(400f32.clamp(min, max.max(min)).round() as u16);
        }
    }
    None
}

fn parse_opentype_faces<R: std::io::Read + std::io::Seek>(reader: &mut R) -> Vec<OpenTypeFaceInfo> {
    use std::io::SeekFrom;
    let mut hdr = [0u8; 12];
    if reader.seek(SeekFrom::Start(0)).is_err() || reader.read_exact(&mut hdr).is_err() {
        return Vec::new();
    }
    let offsets: Vec<u64> = if &hdr[0..4] == b"ttcf" {
        let count = u32::from_be_bytes([hdr[8], hdr[9], hdr[10], hdr[11]]) as usize;
        let count = count.min(256);
        let mut buf = vec![0u8; count * 4];
        if reader.read_exact(&mut buf).is_err() {
            return Vec::new();
        }
        buf.chunks_exact(4).map(|b| u64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))).collect()
    } else if matches!(&hdr[0..4], b"\x00\x01\x00\x00" | b"OTTO" | b"true" | b"typ1") {
        vec![0]
    } else {
        return Vec::new();
    };

    let mut out = Vec::with_capacity(offsets.len());
    for font_off in offsets {
        let mut sfnt = [0u8; 12];
        if reader.seek(SeekFrom::Start(font_off)).is_err() || reader.read_exact(&mut sfnt).is_err() {
            continue;
        }
        let num_tables = (u16::from_be_bytes([sfnt[4], sfnt[5]]) as usize).min(128);
        let mut dir = vec![0u8; num_tables * 16];
        if reader.read_exact(&mut dir).is_err() {
            continue;
        }
        let mut name_span: Option<(u64, usize)> = None;
        let mut os2_span: Option<(u64, usize)> = None;
        let mut fvar_span: Option<(u64, usize)> = None;
        for rec in dir.chunks_exact(16) {
            let tag = &rec[0..4];
            let off = u64::from(u32::from_be_bytes([rec[8], rec[9], rec[10], rec[11]]));
            let len = u32::from_be_bytes([rec[12], rec[13], rec[14], rec[15]]) as usize;
            if tag == b"name" {
                name_span = Some((off, len));
            } else if tag == b"OS/2" {
                os2_span = Some((off, len));
            } else if tag == b"fvar" {
                fvar_span = Some((off, len));
            }
        }
        let Some((name_off, name_len)) = name_span else { continue };
        if name_len == 0 || name_len > 256 * 1024 {
            continue;
        }
        let mut name_buf = vec![0u8; name_len];
        if reader.seek(SeekFrom::Start(name_off)).is_err() || reader.read_exact(&mut name_buf).is_err() {
            continue;
        }
        let (id16_names, id1_names, id4_names) = parse_name_table(&name_buf);
        if id16_names.is_empty() && id1_names.is_empty() {
            continue;
        }
        let (weight, italic) = match os2_span {
            Some((os2_off, os2_len)) if os2_len >= 6 => {
                let mut os2_buf = vec![0u8; os2_len.min(64)];
                if reader.seek(SeekFrom::Start(os2_off)).is_ok() && reader.read_exact(&mut os2_buf).is_ok() {
                    parse_os2_table(&os2_buf)
                } else {
                    (400, false)
                }
            }
            _ => (400, false),
        };
        let family_weight = fvar_span.filter(|&(_, len)| (44..=16 * 1024).contains(&len)).and_then(|(off, len)| {
            let mut fvar = vec![0u8; len];
            (reader.seek(SeekFrom::Start(off)).is_ok() && reader.read_exact(&mut fvar).is_ok()).then(|| variable_family_weight(&fvar)).flatten()
        });
        out.push(OpenTypeFaceInfo { id16_names, id1_names, id4_names, weight, italic, family_weight });
    }
    out
}

pub(crate) fn parse_opentype_faces_from_slice(data: &[u8]) -> Vec<OpenTypeFaceInfo> {
    let mut cursor = std::io::Cursor::new(data);
    parse_opentype_faces(&mut cursor)
}

#[cfg(not(target_family = "wasm"))]
fn scan_font_dir_into(dir: &std::path::Path, depth: usize, index: &mut OpenTypeIndex) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_font_dir_into(&path, depth + 1, index);
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        if matches!(ext.as_deref(), Some("ttf" | "otf" | "ttc"))
            && let Ok(mut file) = std::fs::File::open(&path)
        {
            for face in parse_opentype_faces(&mut file) {
                index.insert_face(&face);
            }
        }
    }
}

#[cfg(not(target_family = "wasm"))]
fn system_font_directories() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    #[cfg(target_os = "windows")]
    {
        if let Some(root) = std::env::var_os("SystemRoot").or_else(|| std::env::var_os("WINDIR")) {
            dirs.push(std::path::PathBuf::from(root).join("Fonts"));
        } else {
            dirs.push(std::path::PathBuf::from(r"C:\Windows\Fonts"));
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            dirs.push(std::path::PathBuf::from(local).join("Microsoft").join("Windows").join("Fonts"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(std::path::PathBuf::from("/System/Library/Fonts"));
        dirs.push(std::path::PathBuf::from("/System/Library/Fonts/Supplemental"));
        dirs.push(std::path::PathBuf::from("/Library/Fonts"));
        if let Some(home) = std::env::var_os("HOME") {
            dirs.push(std::path::PathBuf::from(home).join("Library").join("Fonts"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        dirs.push(std::path::PathBuf::from("/usr/share/fonts"));
        dirs.push(std::path::PathBuf::from("/usr/local/share/fonts"));
        if let Some(home) = std::env::var_os("HOME") {
            let home = std::path::PathBuf::from(home);
            dirs.push(home.join(".local/share/fonts"));
            dirs.push(home.join(".fonts"));
        }
    }
    if let Some(extra) = std::env::var_os("IBIS_EXTRA_FONT_DIRS") {
        dirs.extend(std::env::split_paths(&extra));
    }
    dirs
}

static SYSTEM_OPENTYPE_INDEX: std::sync::OnceLock<OpenTypeIndex> = std::sync::OnceLock::new();

pub(crate) fn system_opentype_index() -> &'static OpenTypeIndex {
    SYSTEM_OPENTYPE_INDEX.get_or_init(|| {
        let mut index = OpenTypeIndex::default();
        for data in BUNDLED_FONTS {
            for face in parse_opentype_faces_from_slice(data) {
                index.insert_face(&face);
            }
        }
        #[cfg(not(target_family = "wasm"))]
        for dir in system_font_directories() {
            scan_font_dir_into(&dir, 0, &mut index);
        }
        index
    })
}

fn alias_in_installed(name: &str, installed: &HashSet<String>) -> Option<SharedString> {
    TYPEFACE_ALIASES
        .iter()
        .filter(|(office, _)| *office == name || office.eq_ignore_ascii_case(name))
        .find_map(|(_, other)| {
            installed
                .get(*other)
                .or_else(|| installed.iter().find(|f| f.eq_ignore_ascii_case(other)))
                .map(|s| SharedString::from(s.clone()))
        })
}

fn installed_or_alias(name: &str, installed: &HashSet<String>) -> Option<SharedString> {
    if let Some(exact) = installed.get(name) {
        return Some(SharedString::from(exact.clone()));
    }
    if let Some(found) = installed.iter().find(|f| f.eq_ignore_ascii_case(name)) {
        return Some(SharedString::from(found.clone()));
    }
    alias_in_installed(name, installed)
}

/// Resolve `name` against `installed`, consulting `extra_index` (e.g.
/// embedded fonts), the system OpenType `name` + `OS/2` index (when
/// `use_system_index` is true), `TYPEFACE_ALIASES`, and `WEIGHT_SUFFIXES`.
pub(crate) fn resolve_font_with_index(
    name: &str,
    installed: &HashSet<String>,
    extra_index: Option<&OpenTypeIndex>,
    use_system_index: bool,
) -> Option<(SharedString, FontWeight, bool)> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let lookup_index = |q: &str| -> Option<(SharedString, FontWeight, bool)> {
        if let Some(idx) = extra_index
            && let Some(entry) = idx.get(q)
            && let Some(family) = entry.resolve_in(installed)
        {
            return Some((family, FontWeight(f32::from(entry.weight)), entry.italic));
        }
        if use_system_index
            && let Some(entry) = system_opentype_index().get(q)
            && let Some(family) = entry.resolve_in(installed)
        {
            return Some((family, FontWeight(f32::from(entry.weight)), entry.italic));
        }
        None
    };

    // 1. OpenType `name` table (ID 1 / ID 16 / ID 4) + `OS/2` weight resolution.
    if let Some(resolved) = lookup_index(name) {
        return Some(resolved);
    }

    // 2. Direct match or typeface alias in `installed`.
    if let Some(family) = installed_or_alias(name, installed) {
        let weight = strip_weight_suffix(name).map_or(FontWeight::NORMAL, |(_, w)| w);
        return Some((family, weight, false));
    }

    // 3. Weight suffix stripping ("Segoe UI Semilight" -> "Segoe UI" at weight 350).
    if let Some((base, weight)) = strip_weight_suffix(name) {
        if let Some((family, _, italic)) = lookup_index(base) {
            return Some((family, weight, italic));
        }
        if let Some(family) = installed_or_alias(base, installed) {
            return Some((family, weight, false));
        }
    }

    None
}

/// Resolve a typeface name against the system's installed fonts (or
/// `fallback_installed` if `SYSTEM_FONTS` has not been initialized yet).
pub(crate) fn resolve_system_font(
    name: &str,
    fallback_installed: &HashSet<String>,
) -> Option<(SharedString, FontWeight, bool)> {
    let installed = system_font_names().unwrap_or(fallback_installed);
    resolve_font_with_index(name, installed, None, true)
}

/// Whether `name` is available for `presentation` (installed on the system,
/// aliased, weight-suffixed, mapped via OpenType `name` ID 1 ⇔ ID 16, or
/// embedded in `presentation.fonts`).
pub(crate) fn is_presentation_font_available(
    name: &str,
    presentation: &ibis_doc_model::Presentation,
) -> bool {
    let name = name.trim();
    if name.is_empty() {
        return true;
    }
    let name = presentation
        .font_substitutions
        .get(name)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or(name);

    let matches_embedded = |candidate: &str| -> bool {
        presentation
            .fonts
            .iter()
            .any(|f| f.typeface.trim().eq_ignore_ascii_case(candidate))
    };
    if matches_embedded(name) {
        return true;
    }
    if let Some((base, _)) = strip_weight_suffix(name)
        && matches_embedded(base)
    {
        return true;
    }

    if let Ok(registered) = REGISTERED_FONTS.lock()
        && !registered.is_empty()
    {
        let mut embedded_installed = HashSet::new();
        let mut embedded_index = OpenTypeIndex::default();
        for (_, family, faces) in registered.iter() {
            embedded_installed.insert(family.clone());
            for face in faces {
                embedded_index.insert_face(face);
            }
        }
        for f in &presentation.fonts {
            embedded_installed.insert(f.typeface.clone());
        }
        if resolve_font_with_index(name, &embedded_installed, Some(&embedded_index), false).is_some() {
            return true;
        }
    }

    if let Some(sys) = system_font_names() {
        if name.eq_ignore_ascii_case(BUNDLED_FAMILY) {
            return true;
        }
        if resolve_font_with_index(name, sys, None, true).is_some() {
            return true;
        }
    }

    false
}

/// Embedded fonts already given to the text system, by a hash of their
/// data, with their family and OpenType `name`/`OS/2` metadata. The text
/// system cannot drop fonts, so each is added once per run of the app.
static REGISTERED_FONTS: std::sync::Mutex<Vec<(u64, String, Vec<OpenTypeFaceInfo>)>> = std::sync::Mutex::new(Vec::new());

fn data_hash(data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    h.finish()
}

/// The system's fonts (and the bundled ones), listed once per run on the first use.
static SYSTEM_FONTS: std::sync::OnceLock<HashSet<String>> = std::sync::OnceLock::new();

/// The family names of the fonts of the system and the ones bundled with the app, once a
/// document has been drawn (the list is made then, not at start-up); `None` before that.
pub(crate) fn system_font_names() -> Option<&'static HashSet<String>> {
    SYSTEM_FONTS.get()
}

impl Fonts {
    /// Make the presentation's embedded fonts available for drawing it.
    ///
    /// Fonts are only held in memory, never installed or written out. A
    /// typeface already installed is used as installed; a font whose
    /// licence forbids embedded use is not used.
    fn add_embedded(&mut self, fonts: &[ibis_doc_model::EmbeddedFont], preload: &crate::render::preload::Preload, cx: &App) {
        let mut registered = REGISTERED_FONTS.lock().unwrap_or_else(|e| e.into_inner());
        let mut new_fonts = Vec::new();
        for font in fonts {
            let installed = self.available(&font.typeface).is_some_and(|(family, _)| !registered.iter().any(|(_, f, _)| **f == *family));
            if installed {
                continue; // installed on this system
            }
            let hash = data_hash(&font.data);
            if let Some((_, family, faces)) = registered.iter().find(|(h, _, _)| *h == hash) {
                self.installed.insert(family.clone());
                for face in faces {
                    self.embedded_index.insert_face(face);
                }
                let weight = faces.first().map_or_else(
                    || strip_weight_suffix(&font.typeface).map_or(400, |(_, w)| w.0 as u16),
                    |f| f.weight,
                );
                let italic = faces.first().is_some_and(|f| f.italic);
                self.embedded_index.insert_explicit(&font.typeface, family, weight, italic);
                continue;
            }
            // Decoded on another thread since the file was read (none when
            // its licence forbids using it).
            match preload.font(font) {
                Some((family, ttf)) => {
                    let faces = parse_opentype_faces_from_slice(&ttf);
                    for face in &faces {
                        self.embedded_index.insert_face(face);
                    }
                    let weight = faces.first().map_or_else(
                        || strip_weight_suffix(&font.typeface).map_or(400, |(_, w)| w.0 as u16),
                        |f| f.weight,
                    );
                    let italic = faces.first().is_some_and(|f| f.italic);
                    self.embedded_index.insert_explicit(&font.typeface, &family, weight, italic);
                    registered.push((hash, family.clone(), faces));
                    self.installed.insert(family);
                    new_fonts.push(std::borrow::Cow::Owned(ttf));
                }
                None => log::warn!("embedded font {} could not be used", font.typeface),
            }
        }
        if !new_fonts.is_empty()
            && let Err(err) = cx.text_system().add_fonts(new_fonts)
        {
            log::warn!("embedded fonts could not be loaded: {err}");
        }
    }

    fn new(cx: &App) -> Self {
        // The system's fonts, listed once per run (it takes a while), with
        // the fonts bundled with the app; fonts embedded in documents are
        // added apart, by `add_embedded`.
        // Another text system than the first one (each scenario of the tests has its own)
        // is given the bundled fonts too: the list above is kept, they are not.
        let first = SYSTEM_FONTS.get().is_none();
        let installed = SYSTEM_FONTS
            .get_or_init(|| {
                let mut names: HashSet<String> = cx.text_system().all_font_names().into_iter().collect();
                if !BUNDLED_FONTS.is_empty() && !names.contains(BUNDLED_FAMILY) {
                    let fonts = BUNDLED_FONTS.iter().map(|data| std::borrow::Cow::Borrowed(*data)).collect();
                    match cx.text_system().add_fonts(fonts) {
                        Ok(()) => {
                            names.insert(BUNDLED_FAMILY.to_string());
                        }
                        Err(err) => log::warn!("bundled fonts could not be loaded: {err}"),
                    }
                }
                add_extra_font_dirs(cx, &mut names);
                names
            })
            .clone();
        // Listing the fonts takes a while: once for each live text system (the app has one;
        // `Weak` avoids address-reuse false hits when short-lived test `App`s are dropped).
        static CHECKED_SYSTEMS: std::sync::Mutex<Vec<std::sync::Weak<gpui_kit::TextSystem>>> = std::sync::Mutex::new(Vec::new());
        let weak = Arc::downgrade(cx.text_system());
        let new_system = {
            let mut checked = CHECKED_SYSTEMS.lock().unwrap_or_else(|e| e.into_inner());
            checked.retain(|w| w.strong_count() > 0);
            let already = checked.iter().any(|w| w.ptr_eq(&weak));
            if !already {
                checked.push(weak);
            }
            !first && !already
        };
        if new_system && !BUNDLED_FONTS.is_empty() {
            let fonts = BUNDLED_FONTS.iter().map(|data| std::borrow::Cow::Borrowed(*data)).collect();
            if let Err(err) = cx.text_system().add_fonts(fonts) {
                log::warn!("bundled fonts could not be loaded: {err}");
            }
        }
        let east_asian_fallback = ["Hiragino Sans", "Hiragino Kaku Gothic ProN", "Yu Gothic", "Noto Sans CJK JP", "IPAexGothic"]
            .into_iter()
            .find(|f| installed.contains(*f))
            .map(SharedString::from);
        Self {
            installed,
            embedded_index: OpenTypeIndex::default(),
            east_asian_fallback,
            substitutions: HashMap::new(),
            calibri_for_missing: false,
            docx_font_families: None,
        }
    }

    fn resolve<'a>(&'a self, name: &'a str) -> &'a str {
        self.substitutions.get(name).map(|s| s.as_str()).unwrap_or(name)
    }

    fn font(&self, style: &TextStyle) -> Font {
        let (family, mut weight) = match &style.font {
            Some(name) => self.family(name),
            None => (SharedString::from(".SystemUIFont"), FontWeight::NORMAL),
        };
        // A typeface named for its weight ("Poppins SemiBold", "Segoe UI Semilight")
        // is its own family to PowerPoint, whose bold member is usually the same face,
        // so bold only applies to plain typefaces (or faces lighter than Semibold).
        // ＭＳ 明朝 has no bold face of its own (Word uses the regular face, w5_mincho_bold_not_heavier_face).
        let resolved_name = style.font.as_deref().map(|n| self.resolve(n));
        let ms_mincho_standin = resolved_name.is_some_and(|n| {
            self.available(n).is_none() && matches!(n, "ＭＳ 明朝" | "MS Mincho" | "ＭＳ Ｐ明朝" | "MS PMincho" | "ＭＳ明朝" | "ＭＳＰ明朝") // i18n-ok（フォント名の判定）
        });
        let named_weight = resolved_name.is_some_and(|n| strip_weight_suffix(n).is_some()) || weight != FontWeight::NORMAL;
        if style.bold && !ms_mincho_standin && (!named_weight || weight < FontWeight::SEMIBOLD) {
            weight = FontWeight::BOLD;
        }
        // A proportional Japanese typeface (MS P Gothic, HGP, Meiryo UI) narrows its kana and punctuation; the
        // font standing in for it does so with proportional widths (`palt`), as the typeface's lines are.
        let proportional = style.font.as_deref().is_some_and(|n| {
            self.available(n).is_none() && (n.starts_with("ＭＳ Ｐ") || n.starts_with("MS P") || n.starts_with("HGP") || n.starts_with("Meiryo UI")) // i18n-ok（フォント名の判定（表示名ではなく、フォント名の接頭辞））
        });
        let mut features = vec![("kern".into(), u32::from(style.kern.0.is_some_and(|k| k > 0 && style.font_size > k)))];
        if proportional {
            features.push(("palt".into(), 1));
            features.push(("pwid".into(), 1));
        }
        Font {
            family,
            weight,
            style: if style.italic { FontStyle::Italic } else { FontStyle::Normal },
            // Kerned above the size the file says (`kern`, inherited from the master: 1200 is 12 pt; text of exactly 12 pt is not kerned in
            // PowerPoint's PDFs, PTXT-00107); not kerned when it says nothing.
            features: FontFeatures(Arc::new(features)),
            fallbacks: browser_fallbacks(style),
            ..font(".SystemUIFont")
        }
    }

    /// The available family and weight for a typeface name, if any: the
    /// name itself, a GDI sub-family mapped by its OpenType `name` (ID 1
    /// to ID 16) and `OS/2` tables, or the name without a weight word
    /// ("Poppins SemiBold" is Poppins at 600, "Segoe UI Semilight" is
    /// Segoe UI at 350).
    fn available(&self, name: &str) -> Option<(SharedString, FontWeight)> {
        let name = self.resolve(name);
        resolve_font_with_index(name, &self.installed, Some(&self.embedded_index), true)
            .map(|(family, weight, _)| (family, weight))
    }

    /// The family and weight to draw a typeface with, falling back to a
    /// system font.
    fn family(&self, name: &str) -> (SharedString, FontWeight) {
        let name = self.resolve(name);
        if let Some(found) = self.available(name) {
            return found;
        }
        let weight = strip_weight_suffix(name).map_or(FontWeight::NORMAL, |(_, w)| w);
        // A Mincho (serif) typeface gives way to an installed Mincho.
        if (name.contains("明朝") || name.contains("Mincho")) // i18n-ok（フォント名の照合（部分一致で明朝を判定）。i18n-ok）
            && let Some(mincho) = ["Hiragino Mincho ProN", "YuMincho", "BIZ UDMincho", "Noto Serif CJK JP", "IPAexMincho"].into_iter().find(|f| self.installed.contains(*f))
        {
            return (mincho.into(), weight);
        }
        match &self.east_asian_fallback {
            Some(fallback) if is_east_asian_typeface(name) => {
                // ＭＳ ゴシック's strokes are heavier than 游ゴシック's / Hiragino Sans W3's regular weight (c006_b2).
                let ms_gothic = matches!(name, "ＭＳ ゴシック" | "MS Gothic" | "ＭＳ Ｐゴシック" | "MS PGothic" | "ＭＳゴシック" | "ＭＳＰゴシック"); // i18n-ok（フォント名の判定）
                let w = if ms_gothic && weight == FontWeight::NORMAL { FontWeight::MEDIUM } else { weight };
                (fallback.clone(), w)
            }
            _ if is_serif_latin_typeface(name) => match ["Times New Roman", "Liberation Serif", "Tinos", "Georgia"].into_iter().find(|f| self.installed.contains(*f)) {
                Some(serif) => (serif.into(), weight),
                None => (".SystemUIFont".into(), weight),
            },
            // Humanist sans-serifs whose widths match Calibri (`Carlito`) closer than Arial.
            _ if name.starts_with("Meta") && self.installed.contains(BUNDLED_FAMILY) => (BUNDLED_FAMILY.into(), weight),
            // PowerPoint draws every line of a presentation in a typeface that is not on the PC with Calibri (Calibri-Bold
            // for bold), whatever the theme's typefaces and the `pitchFamily` and `panose` the run names are (its PDFs of
            // `missing-font-substitute-*.pptx`, with the themes' typefaces Arial, Calibri and Franklin Gothic Book). Word
            // draws it with Cambria and Excel with Arial (their PDFs of `missing-font-substitute.docx` and `.xlsx`), so
            // only a presentation's text. The names Windows' `FontSubstitutes` maps (Helvetica to Arial, Times to Times
            // New Roman) are in `TYPEFACE_ALIASES`. Carlito (bundled) has Calibri's widths where Calibri is not installed.
            _ if self.calibri_for_missing && ["Calibri", BUNDLED_FAMILY].iter().any(|f| self.installed.contains(*f)) => {
                (["Calibri", BUNDLED_FAMILY].into_iter().find(|f| self.installed.contains(*f)).unwrap_or(BUNDLED_FAMILY).into(), weight)
            }
            // Word draws a Latin typeface that the PC does not have with Cambria (Cambria-Bold for bold) if the font table of its file
            // says `w:family` `roman` or does not list it, and with Calibri if the table lists it with another family or none, whatever
            // its panose and pitch (its PDFs of `missing-font-fonttable-family.docx`: only the `roman` one is Cambria; of
            // `missing-font-ctx-*.docx`, with and without a font table and theme: four of five typefaces Cambria, "Noto Sans JP", a typeface
            // Word itself knows, Calibri: not decided here).
            _ if self.docx_font_families.is_some() => {
                let listed_other = self.docx_font_families.as_ref().is_some_and(|m| m.get(name).is_some_and(|f| f != "roman"));
                let wanted: &[&str] = if listed_other { &["Calibri", BUNDLED_FAMILY] } else { &["Cambria"] };
                match wanted.iter().find(|f| self.installed.contains(**f)) {
                    Some(found) => ((*found).into(), weight),
                    None => (if listed_other { "Arial" } else { "Times New Roman" }.into(), weight),
                }
            }
            // A Latin typeface that is not installed gives way to a common one of about its width
            // (the system UI font is wider than most: a line takes 13% more room).
            _ => match ["Arial", "Helvetica", "Liberation Sans"].into_iter().find(|f| self.installed.contains(*f)) {
                Some(common) => (common.into(), weight),
                None => (".SystemUIFont".into(), weight),
            },
        }
    }
}

pub(crate) fn hsla(c: DocColor) -> Hsla {
    let Rgb(v) = c.rgb;
    Rgba {
        r: ((v >> 16) & 0xff) as f32 / 255.0,
        g: ((v >> 8) & 0xff) as f32 / 255.0,
        b: (v & 0xff) as f32 / 255.0,
        a: c.alpha as f32 / 255.0,
    }
    .into()
}

pub(crate) fn hsla_mode(c: DocColor, mode: ColorMode) -> Hsla {
    match mode {
        ColorMode::Color => hsla(c),
        ColorMode::Grayscale => {
            let Rgb(v) = c.rgb;
            let r = ((v >> 16) & 0xff) as f32;
            let g = ((v >> 8) & 0xff) as f32;
            let b = (v & 0xff) as f32;
            let gray = (0.299 * r + 0.587 * g + 0.114 * b).round().clamp(0.0, 255.0) / 255.0;
            Rgba { r: gray, g: gray, b: gray, a: c.alpha as f32 / 255.0 }.into()
        }
        ColorMode::HighContrast => {
            let Rgb(v) = c.rgb;
            let r = ((v >> 16) & 0xff) as f32;
            let g = ((v >> 8) & 0xff) as f32;
            let b = (v & 0xff) as f32;
            let luma = 0.299 * r + 0.587 * g + 0.114 * b;
            let val = if luma < 128.0 { 0.0 } else { 1.0 };
            Rgba { r: val, g: val, b: val, a: if c.alpha == 0 { 0.0 } else { 1.0 } }.into()
        }
    }
}

#[allow(dead_code)]
pub(crate) fn rgb_hsla(rgb: Rgb) -> Hsla {
    hsla(DocColor::opaque(rgb))
}

pub(crate) fn rgb_hsla_mode(rgb: Rgb, mode: ColorMode) -> Hsla {
    hsla_mode(DocColor::opaque(rgb), mode)
}

/// A fill as a GPUI background with color mode support.
pub(crate) fn background_with_mode(fill: &Fill, mode: ColorMode) -> Option<Background> {
    match fill {
        Fill::Solid(c) => Some(hsla_mode(*c, mode).into()),
        Fill::Linear { angle, stops, .. } => {
            let (first, last) = (stops.first()?, stops.last()?);
            // OOXML's 0° runs left to right; CSS's runs bottom to top.
            let degrees = *angle as f32 / 60_000.0 + 90.0;
            Some(linear_gradient(
                degrees,
                linear_color_stop(hsla_mode(first.color, mode), first.position as f32 / 100_000.0),
                linear_color_stop(hsla_mode(last.color, mode), last.position as f32 / 100_000.0),
            ))
        }
        // The background colour; the foreground's cells are painted over it
        // (`paint_pattern`).
        Fill::Pattern { background, .. } => Some(hsla_mode(*background, mode).into()),
        // (Painted band by band where it can be: `gradient::paint_gradient`.)
        Fill::Path { stops, .. } => Some(hsla_mode(stops.last()?.color, mode).into()),
        Fill::None | Fill::Image(_) | Fill::TileImage { .. } | Fill::Blip(_) | Fill::TileLinear { .. } => None,
    }
}

/// A fill as a GPUI background, for fills that are not pictures.
pub(crate) fn background(fill: &Fill) -> Option<Background> {
    background_with_mode(fill, ColorMode::Color)
}

/// The dashes of a stroke along a polyline (window pixels), in its dash
/// pattern (`LineDash::pattern`, in line widths).
pub(crate) fn dash_pieces(points: &[Point<Pixels>], closed: bool, stroke: &Stroke, width: f32) -> Vec<Vec<Point<Pixels>>> {
    let pattern = match stroke.dash.pattern() {
        [] => &[4.0, 3.0][..],
        p => p,
    };
    let w = width;
    let step_len = |index: usize, _is_on: bool| pattern[index] * w;
    let mut pts: Vec<(f32, f32)> = points.iter().map(|p| (f32::from(p.x), f32::from(p.y))).collect();
    if closed && let Some(&first) = pts.first() {
        pts.push(first);
    }
    let mut out: Vec<Vec<Point<Pixels>>> = Vec::new();
    let mut current: Vec<Point<Pixels>> = Vec::new();
    let mut index = 0usize;
    let mut on = true;
    let mut left = step_len(index, on);
    for seg in pts.windows(2) {
        let ((x0, y0), (x1, y1)) = (seg[0], seg[1]);
        let len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
        if len == 0.0 {
            continue;
        }
        let at = |t: f32| point(px(x0 + (x1 - x0) * t / len), px(y0 + (y1 - y0) * t / len));
        let mut t = 0.0;
        while t < len {
            let step = left.min(len - t);
            if on {
                if current.is_empty() {
                    current.push(at(t));
                }
                current.push(at(t + step));
            }
            t += step;
            left -= step;
            if left <= 1e-4 {
                if on && current.len() >= 2 {
                    out.push(std::mem::take(&mut current));
                }
                current.clear();
                on = !on;
                index = (index + 1) % pattern.len();
                left = step_len(index, on);
            }
        }
    }
    if on && current.len() >= 2 {
        out.push(current);
    }
    if stroke.cap == ibis_doc_model::LineCap::Round {
        for piece in &mut out {
            let n = piece.len();
            if n >= 2 {
                let (first, last) = (piece[0], piece[n - 1]);
                let (dx, dy) = (f32::from(last.x - first.x), f32::from(last.y - first.y));
                let len = (dx * dx + dy * dy).sqrt();
                if len <= w {
                    piece[0] = first;
                    piece[n - 1] = first;
                } else {
                    let (ux, uy) = (dx / len * w, dy / len * w);
                    piece[0] = first;
                    piece[n - 1] = point(last.x - px(ux), last.y - px(uy));
                }
            }
        }
    }
    out
}

/// Round caps: a disc of the line's width at each end of each piece.
pub(crate) fn paint_round_caps(window: &mut Window, pieces: &[Vec<Point<Pixels>>], width: f32, color: Hsla) {
    let r = width / 2.0;
    for piece in pieces {
        let ends: Vec<&Point<Pixels>> = match (piece.first(), piece.last()) {
            (Some(f), Some(l)) if (f32::from(f.x - l.x)).hypot(f32::from(f.y - l.y)) <= 1e-4 => vec![f],
            (Some(f), Some(l)) => vec![f, l],
            (Some(f), None) => vec![f],
            _ => vec![],
        };
        for end in ends {
            let disc: Vec<Point<Pixels>> = (0..16).map(|i| {
                let t = std::f32::consts::TAU * i as f32 / 16.0;
                point(end.x + px(r * t.cos()), end.y + px(r * t.sin()))
            }).collect();
            if let Some(p) = polygon(&disc, None, true) {
                window.paint_path(p, color);
            }
        }
    }
}

/// Round caps on a compound line: an outer semicircular arch connecting the
/// outer bands, plus discs for inner bands, so the line ends in concentric
/// arcs rather than separate tiny dots.
pub(crate) fn paint_compound_round_caps(
    window: &mut Window,
    pieces: &[Vec<Point<Pixels>>],
    width: f32,
    compound: ibis_doc_model::LineCompound,
    color: Hsla,
    head: bool,
    tail: bool,
) {
    if compound == ibis_doc_model::LineCompound::Single || width < 1.5 {
        return;
    }
    let n_pieces = pieces.len();
    for (idx, piece) in pieces.iter().enumerate() {
        if piece.len() < 2 {
            continue;
        }
        let apply_head = if idx == 0 { head } else { true };
        let apply_tail = if idx + 1 == n_pieces { tail } else { true };
        let p0 = piece[0];
        let p1 = piece[1];
        let pn = *piece.last().unwrap();
        let pn_prev = piece[piece.len() - 2];

        for (at, neighbor, apply, is_head) in [(p0, p1, apply_head, true), (pn, pn_prev, apply_tail, false)] {
            if !apply {
                continue;
            }
            let (dx, dy) = if is_head {
                (f32::from(neighbor.x - at.x), f32::from(neighbor.y - at.y))
            } else {
                (f32::from(at.x - neighbor.x), f32::from(at.y - neighbor.y))
            };
            let len = dx.hypot(dy);
            if len <= 1e-4 {
                continue;
            }
            let (dir_x, dir_y) = (dx / len, dy / len);
            let (nx, ny) = (-dir_y, dir_x);
            let (out_x, out_y) = if is_head { (-dir_x, -dir_y) } else { (dir_x, dir_y) };

            let make_arc = |window: &mut Window, radius: f32, stroke_w: f32| {
                let pts: Vec<Point<Pixels>> = (0..=16)
                    .map(|i| {
                        let theta = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 16.0;
                        let (sin_t, cos_t) = (theta.sin(), theta.cos());
                        point(
                            at.x + px(radius * (sin_t * nx + cos_t * out_x)),
                            at.y + px(radius * (sin_t * ny + cos_t * out_y)),
                        )
                    })
                    .collect();
                if let Some(p) = polygon(&pts, Some(px(stroke_w.max(0.5))), false) {
                    window.paint_path(p, color);
                }
            };

            let make_disc = |window: &mut Window, center: Point<Pixels>, disc_w: f32| {
                let r = disc_w / 2.0;
                let disc: Vec<Point<Pixels>> = (0..16)
                    .map(|i| {
                        let t = std::f32::consts::TAU * i as f32 / 16.0;
                        point(center.x + px(r * t.cos()), center.y + px(r * t.sin()))
                    })
                    .collect();
                if let Some(p) = polygon(&disc, None, true) {
                    window.paint_path(p, color);
                }
            };

            match compound {
                ibis_doc_model::LineCompound::Double => {
                    make_arc(window, width / 3.0, width / 3.0);
                    make_disc(window, point(at.x - px(width / 3.0 * nx), at.y - px(width / 3.0 * ny)), width / 3.0);
                    make_disc(window, point(at.x + px(width / 3.0 * nx), at.y + px(width / 3.0 * ny)), width / 3.0);
                }
                ibis_doc_model::LineCompound::Triple => {
                    make_arc(window, 5.0 / 12.0 * width, width / 6.0);
                    make_disc(window, point(at.x - px(5.0 / 12.0 * width * nx), at.y - px(5.0 / 12.0 * width * ny)), width / 6.0);
                    make_disc(window, at, width / 3.0);
                    make_disc(window, point(at.x + px(5.0 / 12.0 * width * nx), at.y + px(5.0 / 12.0 * width * ny)), width / 6.0);
                }
                ibis_doc_model::LineCompound::ThickThin => {
                    make_arc(window, 0.4 * width, 0.2 * width);
                    make_disc(window, point(at.x - px(0.2 * width * nx), at.y - px(0.2 * width * ny)), 0.6 * width);
                    make_disc(window, point(at.x + px(0.4 * width * nx), at.y + px(0.4 * width * ny)), 0.2 * width);
                }
                ibis_doc_model::LineCompound::ThinThick => {
                    make_arc(window, 0.4 * width, 0.2 * width);
                    make_disc(window, point(at.x - px(0.4 * width * nx), at.y - px(0.4 * width * ny)), 0.2 * width);
                    make_disc(window, point(at.x + px(0.2 * width * nx), at.y + px(0.2 * width * ny)), 0.6 * width);
                }
                ibis_doc_model::LineCompound::Single => {}
            }
        }
    }
}

/// The pieces of an open line with square caps: half a width longer at
/// specified ends.
pub(crate) fn square_capped(piece: &[Point<Pixels>], width: f32, head: bool, tail: bool) -> Vec<Point<Pixels>> {
    let mut out = piece.to_vec();
    let n = out.len();
    if n < 2 {
        return out;
    }
    let extend = |a: Point<Pixels>, b: Point<Pixels>| {
        let (dx, dy) = (f32::from(a.x - b.x), f32::from(a.y - b.y));
        let len = (dx * dx + dy * dy).sqrt().max(1e-4);
        point(a.x + px(dx / len * width / 2.0), a.y + px(dy / len * width / 2.0))
    };
    if head {
        out[0] = extend(piece[0], piece[1]);
    }
    if tail {
        out[n - 1] = extend(piece[n - 1], piece[n - 2]);
    }
    out
}

#[derive(Clone, Copy, Default)]
struct PatternRun {
    offset: i8,
    len: u8,
}

#[derive(Clone, Copy, Default)]
struct TileRuns {
    count: u8,
    runs: [PatternRun; 4],
}

/// Precomputed runs of 1-bits within an 8-cell tile for each byte.
const RUN_TABLE: [TileRuns; 256] = {
    let mut table = [TileRuns { count: 0, runs: [PatternRun { offset: 0, len: 0 }; 4] }; 256];
    let mut b = 0usize;
    while b < 256 {
        let bits = b as u8;
        if bits != 0 && bits != 0xff {
            // Find a zero bit to ensure runs do not wrap around the scan boundary.
            let mut zero_c = 0;
            while zero_c < 8 {
                if (bits & (0x80 >> zero_c)) == 0 {
                    break;
                }
                zero_c += 1;
            }
            let mut count = 0;
            let mut runs = [PatternRun { offset: 0, len: 0 }; 4];
            let mut i = 1;
            while i <= 8 {
                let c = (zero_c + i) % 8;
                if (bits & (0x80 >> c)) != 0 {
                    let start = (zero_c + i) % 8;
                    let mut len = 1;
                    while i + 1 <= 8 && (bits & (0x80 >> ((zero_c + i + 1) % 8))) != 0 {
                        i += 1;
                        len += 1;
                    }
                    runs[count] = PatternRun {
                        offset: start as i8,
                        len,
                    };
                    count += 1;
                }
                i += 1;
            }
            table[b] = TileRuns { count: count as u8, runs };
        }
        b += 1;
    }
    table
};

/// Check if `subpaths` represents a single axis-aligned rectangle.
/// Returns `Some((min_x, min_y, max_x, max_y))` if so.
fn as_axis_aligned_rect(subpaths: &[Vec<Point<Pixels>>]) -> Option<(f32, f32, f32, f32)> {
    if subpaths.len() != 1 {
        return None;
    }
    let pts = &subpaths[0];
    let pts = if pts.len() == 5 && pts[0] == pts[4] {
        &pts[..4]
    } else if pts.len() == 4 {
        &pts[..]
    } else {
        return None;
    };

    let xs = [f32::from(pts[0].x), f32::from(pts[1].x), f32::from(pts[2].x), f32::from(pts[3].x)];
    let ys = [f32::from(pts[0].y), f32::from(pts[1].y), f32::from(pts[2].y), f32::from(pts[3].y)];

    let min_x = xs.iter().copied().fold(f32::MAX, f32::min);
    let max_x = xs.iter().copied().fold(f32::MIN, f32::max);
    let min_y = ys.iter().copied().fold(f32::MAX, f32::min);
    let max_y = ys.iter().copied().fold(f32::MIN, f32::max);

    if max_x <= min_x || max_y <= min_y {
        return None;
    }

    for i in 0..4 {
        let (x0, y0) = (xs[i], ys[i]);
        let (x1, y1) = (xs[(i + 1) % 4], ys[(i + 1) % 4]);
        if (x0 - x1).abs() > 0.001 && (y0 - y1).abs() > 0.001 {
            return None;
        }
    }

    for i in 0..4 {
        let (x, y) = (xs[i], ys[i]);
        let at_x = (x - min_x).abs() <= 0.001 || (x - max_x).abs() <= 0.001;
        let at_y = (y - min_y).abs() <= 0.001 || (y - max_y).abs() <= 0.001;
        if !at_x || !at_y {
            return None;
        }
    }

    Some((min_x, min_y, max_x, max_y))
}

#[inline]
fn paint_pattern_span(
    window: &mut Window,
    bits: u8,
    left: f32,
    right: f32,
    top_px: f32,
    bottom_px: f32,
    ox: f32,
    cell: f32,
    color: Hsla,
) {
    if bits == 0 || right <= left || bottom_px <= top_px {
        return;
    }
    if bits == 0xff {
        window.paint_quad(gpui_kit::fill(
            Bounds::new(point(px(left), px(top_px)), size(px(right - left), px(bottom_px - top_px))),
            color,
        ));
        return;
    }
    let tile_width = 8.0 * cell;
    let first_tile = ((left - ox) / tile_width).floor() as i64 - 1;
    let last_tile = ((right - ox) / tile_width).ceil() as i64;
    let tile_runs = &RUN_TABLE[bits as usize];
    for t in first_tile..=last_tile {
        for run in &tile_runs.runs[..tile_runs.count as usize] {
            let col_start = t * 8 + run.offset as i64;
            let col_end = col_start + run.len as i64;
            let x0 = (ox + col_start as f32 * cell).max(left);
            let x1 = (ox + col_end as f32 * cell).min(right);
            if x1 > x0 {
                window.paint_quad(gpui_kit::fill(
                    Bounds::new(point(px(x0), px(top_px)), size(px(x1 - x0), px(bottom_px - top_px))),
                    color,
                ));
            }
        }
    }
}

/// A pattern's cell: a pixel of a 96 dpi screen, in points.
const PATTERN_CELL_POINTS: f32 = 72.0 / 96.0;

/// Paint the foreground cells of pattern `fill` inside the closed polygons
/// `subpaths` (window pixels, even–odd), its 8 × 8 tile of cells of a pixel of a 96 dpi screen
/// (`PATTERN_CELL_POINTS` = 0.75 pt: PowerPoint on Windows makes the tile a bitmap of 8 × 8 screen pixels, and its PDF tiles it at 6 pt:
/// `/Matrix [0.375 0 0 0.375 0 540]` with a 16-unit tile, in every PDF of the coverage corpus with a pattern fill) anchored at `origin` (the slide's corner, as PowerPoint anchors it),
/// `scale` pixels per EMU. Each row of cells is cut to where the row's
/// middle line crosses the polygons.
pub(crate) fn paint_pattern(window: &mut Window, fill: &Fill, subpaths: &[Vec<Point<Pixels>>], origin: Point<Pixels>, scale: f32) {
    let Fill::Pattern { preset, foreground, .. } = fill else { return };
    let Some(tile) = ibis_doc_model::pattern_tile(preset) else { return };
    let cell = PATTERN_CELL_POINTS * ibis_doc_model::EMU_PER_POINT as f32 * scale;
    if cell <= 0.0 || subpaths.is_empty() {
        return;
    }
    let color = hsla(*foreground);
    // For a PDF (the window only records): an axis-aligned area is one picture of the tile's cells, a pixel to a cell, which a PDF viewer averages
    // when it shows the page smaller than the cells (PowerPoint's PDF tiles a picture of the cells likewise); quads of cells smaller than a pixel come out
    // as a coarse grid there.
    #[cfg(not(target_family = "wasm"))]
    if window.is_recording_paints_only()
        && color.a >= 1.0
        && let Some((left, top, right, bottom)) = as_axis_aligned_rect(subpaths)
        && right > left
        && bottom > top
    {
        let (ox, oy) = (f32::from(origin.x), f32::from(origin.y));
        let (c0, c1) = (((left - ox) / cell).floor() as i64, ((right - ox) / cell).ceil() as i64);
        let (r0, r1) = (((top - oy) / cell).floor() as i64, ((bottom - oy) / cell).ceil() as i64);
        let (w, h) = ((c1 - c0) as u32, (r1 - r0) as u32);
        if w > 0 && h > 0 && (w as u64) * (h as u64) <= 16_000_000 {
            let rgba: Rgba = color.into();
            let ink = [(rgba.b * 255.0).round() as u8, (rgba.g * 255.0).round() as u8, (rgba.r * 255.0).round() as u8, 255];
            let mut bytes = vec![0u8; (w * h * 4) as usize];
            for row in 0..h as i64 {
                let bits = tile[(r0 + row).rem_euclid(8) as usize];
                for col in 0..w as i64 {
                    if bits & (0x80 >> (c0 + col).rem_euclid(8)) != 0 {
                        let at = ((row as u32 * w + col as u32) * 4) as usize;
                        bytes[at..at + 4].copy_from_slice(&ink);
                    }
                }
            }
            if let Some(buffer) = image::RgbaImage::from_raw(w, h, bytes) {
                let picture = Arc::new(RenderImage::new([image::Frame::new(buffer)]));
                let area = Bounds::new(point(px(left), px(top)), size(px(right - left), px(bottom - top)));
                let whole = Bounds::new(point(px(ox + c0 as f32 * cell), px(oy + r0 as f32 * cell)), size(px(w as f32 * cell), px(h as f32 * cell)));
                let _ = window.paint_image(area, whole, Corners::default(), picture, 0, false);
                return;
            }
        }
    }
    // Cells smaller than a device pixel cannot be drawn as cells (a quad covers whole pixels, so a dotted line of 0.5 px cells comes out a solid line and the
    // pattern 4 times too dark). Paint the foreground's share of the tile (the set bits of 64) as one translucent fill, what a raster of the pattern averages to.
    if cell * window.scale_factor() < 1.0 {
        let share = tile.iter().map(|b| b.count_ones()).sum::<u32>() as f32 / 64.0;
        let mean = Hsla { a: color.a * share, ..color };
        let closed: Vec<(Vec<Point<Pixels>>, bool)> = subpaths.iter().map(|p| (p.clone(), true)).collect();
        if let Some((left, top, right, bottom)) = as_axis_aligned_rect(subpaths) {
            window.paint_quad(gpui_kit::fill(Bounds::new(point(px(left), px(top)), size(px(right - left), px(bottom - top))), mean));
        } else if let Some(path) = compound_path(&closed, None) {
            window.paint_path(path, mean);
        }
        return;
    }
    let (ox, oy) = (f32::from(origin.x), f32::from(origin.y));

    // Fast path: axis-aligned rectangle (covers slide background, table cells, and regular rectangular shapes).
    if let Some((left, top, right, bottom)) = as_axis_aligned_rect(subpaths) {
        if right <= left || bottom <= top {
            return;
        }

        // Fast path for vertical patterns (all 8 rows have identical bit patterns):
        // Merge the entire column into single vertical quads spanning from top to bottom.
        let is_vertical = tile.windows(2).all(|w| w[0] == w[1]);
        if is_vertical {
            paint_pattern_span(window, tile[0], left, right, top, bottom, ox, cell, color);
            return;
        }

        let first = ((top - oy) / cell).floor() as i64;
        let last = ((bottom - oy) / cell).ceil() as i64;
        let mut row = first;
        while row < last {
            let bits = tile[row.rem_euclid(8) as usize];
            if bits == 0 {
                row += 1;
                continue;
            }
            if bits == 0xff {
                // Merge consecutive 0xff rows into a single vertical quad.
                let row_start = row;
                while row + 1 < last && tile[(row + 1).rem_euclid(8) as usize] == 0xff {
                    row += 1;
                }
                let y0 = oy + row_start as f32 * cell;
                let y1 = oy + (row + 1) as f32 * cell;
                let top_px = y0.max(top);
                let bottom_px = y1.min(bottom);
                if bottom_px > top_px {
                    window.paint_quad(gpui_kit::fill(
                        Bounds::new(point(px(left), px(top_px)), size(px(right - left), px(bottom_px - top_px))),
                        color,
                    ));
                }
                row += 1;
                continue;
            }
            let y0 = oy + row as f32 * cell;
            let top_px = y0.max(top);
            let bottom_px = (y0 + cell).min(bottom);
            paint_pattern_span(window, bits, left, right, top_px, bottom_px, ox, cell, color);
            row += 1;
        }
        return;
    }

    // General polygon path. Reuses `xs` buffer across rows to eliminate heap allocations.
    let ys = subpaths.iter().flatten().map(|p| f32::from(p.y));
    let (top, bottom) = ys.clone().fold((f32::MAX, f32::MIN), |(a, b), y| (a.min(y), b.max(y)));
    let first = ((top - oy) / cell).floor() as i64;
    let last = ((bottom - oy) / cell).ceil() as i64;
    let mut xs: Vec<f32> = Vec::with_capacity(16);
    for row in first..last {
        let bits = tile[row.rem_euclid(8) as usize];
        if bits == 0 {
            continue;
        }
        let y0 = oy + row as f32 * cell;
        let middle = y0 + cell / 2.0;
        xs.clear();
        for points in subpaths {
            for i in 0..points.len() {
                let (a, b) = (points[i], points[(i + 1) % points.len()]);
                let (ay, by) = (f32::from(a.y), f32::from(b.y));
                if (ay <= middle) != (by <= middle) {
                    let t = (middle - ay) / (by - ay);
                    xs.push(f32::from(a.x) + t * (f32::from(b.x) - f32::from(a.x)));
                }
            }
        }
        xs.sort_by(|a, b| a.total_cmp(b));
        let top_px = y0.max(top);
        let bottom_px = (y0 + cell).min(bottom);
        if bottom_px <= top_px {
            continue;
        }
        for span in xs.chunks_exact(2) {
            paint_pattern_span(window, bits, span[0], span[1], top_px, bottom_px, ox, cell, color);
        }
    }
}

/// The picture `media` cut to `outlines` (frame-relative EMU, flipped as
/// the shape is), if its format can be wrapped in SVG.
fn clipped_fill(media: MediaId, shape: &Shape, outlines: &[ibis_doc_model::Outline], frame: EmuRect, res: &Resources) -> Option<Arc<Image>> {
    let (w, h) = (frame.width as f64, frame.height as f64);
    let subpaths: Vec<Vec<(f64, f64)>> = outlines
        .iter()
        .filter(|o| o.fill != ibis_doc_model::PathFill::None)
        .flat_map(|o| &o.subpaths)
        .map(|s| {
            s.points
                .iter()
                .map(|&(x, y)| (if shape.flip_h { w - x } else { x }, if shape.flip_v { h - y } else { y }))
                .collect()
        })
        .collect();
    let key = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (frame.width, frame.height).hash(&mut hasher);
        for p in subpaths.iter().flatten() {
            (p.0.to_bits(), p.1.to_bits()).hash(&mut hasher);
        }
        (media, hasher.finish())
    };
    if let Some(found) = res.clipped.borrow().get(&key) {
        return Some(found.clone());
    }
    let (encoding, data) = res.source(media)?;
    let svg = crate::render::picture_svg::clipped_picture(&data, encoding, frame.width, frame.height, &subpaths);
    let image = Arc::new(Image::from_bytes(ImageFormat::Svg, svg));
    res.clipped.borrow_mut().insert(key, image.clone());
    Some(image)
}

/// `fill` turned with its shape: a gradient's direction follows the shape's
/// flips and rotation (OOXML's `rotWithShape`, on by default).
fn turned_fill(fill: &Fill, shape: &Shape) -> Fill {
    match fill {
        Fill::Linear { angle, scaled, stops } => {
            let mut angle = *angle;
            if shape.flip_v {
                angle = -angle;
            }
            if shape.flip_h {
                angle = 10_800_000 - angle;
            }
            Fill::Linear { angle: (angle + shape.rotation).rem_euclid(21_600_000), scaled: *scaled, stops: stops.clone() }
        }
        Fill::TileLinear { angle, scaled, stops, tile, flip } => {
            let mut angle = *angle;
            if shape.flip_v {
                angle = -angle;
            }
            if shape.flip_h {
                angle = 10_800_000 - angle;
            }
            Fill::TileLinear { angle: (angle + shape.rotation).rem_euclid(21_600_000), scaled: *scaled, stops: stops.clone(), tile: *tile, flip: *flip }
        }
        other => other.clone(),
    }
}

/// A shadow's offset, blur radius (pixels) and colour.
fn shadow_parts(shadow: ibis_doc_model::Shadow, scale: f32) -> (f32, f32, f32, Hsla) {
    let angle = (shadow.direction as f32 / 60_000.0).to_radians();
    let distance = shadow.distance as f32 * scale;
    (distance * angle.cos(), distance * angle.sin(), shadow.blur as f32 * scale, hsla(shadow.color))
}

fn shadow_anchor(align: ibis_doc_model::TileAlign, w: f64, h: f64, stroke_half: f64) -> (f64, f64) {
    let s = stroke_half;
    let w_ext = w + 2.0 * s;
    let h_ext = h + 2.0 * s;
    let (ax, ay) = match align {
        ibis_doc_model::TileAlign::TopLeft => (0.0, 0.0),
        ibis_doc_model::TileAlign::Top => (w_ext / 2.0, 0.0),
        ibis_doc_model::TileAlign::TopRight => (w_ext, 0.0),
        ibis_doc_model::TileAlign::Left => (0.0, h_ext / 2.0),
        ibis_doc_model::TileAlign::Center => (w_ext / 2.0, h_ext / 2.0),
        ibis_doc_model::TileAlign::Right => (w_ext, h_ext / 2.0),
        ibis_doc_model::TileAlign::BottomLeft => (0.0, h_ext),
        ibis_doc_model::TileAlign::Bottom => (w_ext / 2.0, h_ext),
        ibis_doc_model::TileAlign::BottomRight => (w_ext, h_ext),
    };
    (ax - s, ay - s)
}

fn render_transformed_shadow(
    window: &mut Window,
    outlines: &[ibis_doc_model::Outline],
    anchor: (f64, f64),
    shadow: &ibis_doc_model::Shadow,
    blur: f32,
    color: Hsla,
    exact: Point<Pixels>,
    scale: f32,
) {
    let (xa, ya) = anchor;
    let sx = shadow.sx as f64 / 100_000.0;
    let sy = shadow.sy as f64 / 100_000.0;
    let kx_rad = (shadow.kx as f64 / 60_000.0).to_radians();
    let ky_rad = (shadow.ky as f64 / 60_000.0).to_radians();
    let tan_kx = kx_rad.tan();
    let tan_ky = ky_rad.tan();

    let dir_rad = (shadow.direction as f64 / 60_000.0).to_radians();
    let dist = shadow.distance as f64;
    let off_x = dist * dir_rad.cos();
    let off_y = dist * dir_rad.sin();

    let transform_pt = |p: (f64, f64)| -> (f64, f64) {
        let dx_rel = p.0 - xa;
        let dy_rel = p.1 - ya;
        let x_prime = sx * dx_rel + dy_rel * tan_kx;
        let y_prime = sy * dy_rel + dx_rel * tan_ky;
        (xa + x_prime + off_x, ya + y_prime + off_y)
    };

    let spots: Vec<(f32, f32, f32)> = if blur < 0.5 {
        vec![(0.0, 0.0, 1.0)]
    } else {
        let mut s = Vec::with_capacity(37);
        s.push((0.0, 0.0, 1.0));
        let r1 = blur * 0.30;
        for i in 0..8 {
            let (sin, cos) = (i as f32 * std::f32::consts::FRAC_PI_4).sin_cos();
            s.push((cos * r1, sin * r1, 0.7));
        }
        let r2 = blur * 0.70;
        for i in 0..12 {
            let (sin, cos) = (i as f32 * std::f32::consts::TAU / 12.0).sin_cos();
            s.push((cos * r2, sin * r2, 0.6));
        }
        let r3 = blur * 1.05;
        for i in 0..16 {
            let (sin, cos) = (i as f32 * std::f32::consts::TAU / 16.0).sin_cos();
            s.push((cos * r3, sin * r3, 0.25));
        }
        s
    };

    let total_weight: f32 = spots.iter().map(|(_, _, w)| *w).sum();

    for &(ox, oy, weight) in &spots {
        let faint = Hsla { a: (color.a * (weight / total_weight) * 1.25).min(1.0), ..color };
        for outline in outlines.iter().filter(|o| o.fill != ibis_doc_model::PathFill::None) {
            let subpaths: Vec<(Vec<Point<Pixels>>, bool)> = outline
                .subpaths
                .iter()
                .map(|s| {
                    let pts: Vec<Point<Pixels>> = s
                        .points
                        .iter()
                        .map(|&p| {
                            let (tx, ty) = transform_pt(p);
                            point(
                                exact.x + px((tx * scale as f64) as f32 + ox),
                                exact.y + px((ty * scale as f64) as f32 + oy),
                            )
                        })
                        .collect();
                    (pts, s.closed)
                })
                .collect();
            if let Some(p) = compound_path(&subpaths, None) {
                window.paint_path(p, faint);
            }
        }
    }
}

/// Where a shape `w` × `h` pixels tilted by `tilt` appears in its frame, as
/// x, y, width and height: an orthographic view shortens it by the cosine
/// of each turn, and its lift off the slide moves it by the sine
/// (measured against PowerPoint's PDF output: a lime band under a heading
/// turned 75° about the horizontal axis and lifted 6 pt).
fn tilted_frame(tilt: ibis_doc_model::Tilt, w: f32, h: f32, scale: f32) -> (f32, f32, f32, f32) {
    let lat = (tilt.lat as f32 / 60_000.0).to_radians();
    let lon = (tilt.lon as f32 / 60_000.0).to_radians();
    let depth = tilt.depth as f32 * scale;
    let (tw, th) = (w * lon.cos().abs(), h * lat.cos().abs());
    let x = (w - tw) / 2.0 + depth * lon.sin();
    let y = (h - th) / 2.0 + depth * lat.sin();
    // PowerPoint draws the tilted shape's edges too, about a point around it.
    let edge = 0.75 * scale * EMU_PER_POINT as f32;
    (x - edge, y, tw + edge * 2.0, th + edge)
}

/// Place an element at `frame` on a slide drawn at `scale`.
pub(crate) fn place<E: Styled>(element: E, frame: EmuRect, scale: f32) -> E {
    let x0 = frame.x as f32 * scale;
    let y0 = frame.y as f32 * scale;
    let x1 = (frame.x + frame.width) as f32 * scale;
    let y1 = (frame.y + frame.height) as f32 * scale;
    let snap = |v: f32| {
        if (v - v.trunc()).abs() == 0.5 { v.trunc() } else { v.round() }
    };
    let w = (snap(x1) - snap(x0)).max(0.0);
    let h = (snap(y1) - snap(y0)).max(0.0);
    element
        .absolute()
        .left(px(x0))
        .top(px(y0))
        .w(px(w))
        .h(px(h))
}

/// The picture of a [`BlipFill`](ibis_doc_model::BlipFill) over a frame, with its range, place, tiling and
/// colour effects; `geometry` cuts it to an outline other than the frame.
fn blip_fill_image(blip: &ibis_doc_model::BlipFill, geometry: Option<&Geometry>, flips: (bool, bool), frame: EmuRect, res: &Resources) -> Option<Arc<Image>> {
    let mut picture = Picture::new(blip.media);
    picture.crop = blip.crop;
    picture.fill_rect = blip.fill_rect;
    picture.tile = blip.tile;
    picture.duotone = blip.duotone;
    picture.luminance = blip.luminance;
    picture.effects = blip.effects;
    picture.geometry = geometry.filter(|g| **g != Geometry::Rect).cloned();
    styled_image(flips.0, flips.1, &picture, frame, res)
}

/// Paint `fill` over the whole of `element`.
pub(crate) fn with_fill(element: Div, fill: &Fill, res: &Resources) -> Div {
    match fill {
        // The background colour (`slide_backdrop` paints the cells).
        Fill::Pattern { background: bg, .. } => element.bg(hsla_mode(*bg, res.color_mode)),
        Fill::Image(media) | Fill::TileImage { media, .. } => match res.picture(*media) {
            Some(image) => element.child(img(image).absolute().size_full().object_fit(ObjectFit::Fill)),
            None => element,
        },
        // Where the frame is not known (a table cell): the picture as is, with its opacity.
        Fill::Blip(blip) => match res.picture(blip.media) {
            Some(image) => element.child(img(image).absolute().size_full().object_fit(ObjectFit::Fill).opacity(blip.effects.opacity.map_or(1.0, |o| o as f32 / 100_000.0))),
            None => element,
        },
        other => match background_with_mode(other, res.color_mode) {
            Some(bg) => element.bg(bg),
            None => element,
        },
    }
}

/// Paint a tiled picture over `bounds_emu` (in EMU), at `scale` pixels per EMU.
pub(crate) fn tile_fill_element(
    media: MediaId,
    tx: i64,
    ty: i64,
    sx: i32,
    sy: i32,
    flip: ibis_doc_model::TileFlip,
    align: ibis_doc_model::TileAlign,
    bounds_emu: (i64, i64),
    scale: f32,
    res: &Resources,
) -> Div {
    let container = div().absolute().size_full().overflow_hidden();
    let Some(image) = res.picture(media) else { return container };
    let Some((img_w_px, img_h_px)) = res.image_size(media) else { return container };
    if img_w_px == 0 || img_h_px == 0 {
        return container;
    }

    // In Office, picture original dimensions are 1 px = 1 pt = 12700 EMU.
    let orig_w_emu = img_w_px as i64 * ibis_doc_model::EMU_PER_POINT;
    let orig_h_emu = img_h_px as i64 * ibis_doc_model::EMU_PER_POINT;

    let sx_f = (sx.max(1) as f64) / 100_000.0;
    let sy_f = (sy.max(1) as f64) / 100_000.0;

    let tile_w_emu = ((orig_w_emu as f64 * sx_f).round() as i64).max(ibis_doc_model::EMU_PER_POINT);
    let tile_h_emu = ((orig_h_emu as f64 * sy_f).round() as i64).max(ibis_doc_model::EMU_PER_POINT);

    let (dest_w_emu, dest_h_emu) = bounds_emu;

    let (base_x, base_y) = match align {
        ibis_doc_model::TileAlign::TopLeft => (0, 0),
        ibis_doc_model::TileAlign::Top => ((dest_w_emu - tile_w_emu) / 2, 0),
        ibis_doc_model::TileAlign::TopRight => (dest_w_emu - tile_w_emu, 0),
        ibis_doc_model::TileAlign::Left => (0, (dest_h_emu - tile_h_emu) / 2),
        ibis_doc_model::TileAlign::Center => ((dest_w_emu - tile_w_emu) / 2, (dest_h_emu - tile_h_emu) / 2),
        ibis_doc_model::TileAlign::Right => (dest_w_emu - tile_w_emu, (dest_h_emu - tile_h_emu) / 2),
        ibis_doc_model::TileAlign::BottomLeft => (0, dest_h_emu - tile_h_emu),
        ibis_doc_model::TileAlign::Bottom => ((dest_w_emu - tile_w_emu) / 2, dest_h_emu - tile_h_emu),
        ibis_doc_model::TileAlign::BottomRight => (dest_w_emu - tile_w_emu, dest_h_emu - tile_h_emu),
    };

    let anchor_x_emu = base_x + tx;
    let anchor_y_emu = base_y + ty;

    // The tiles, with the alternate ones mirrored if the fill says, as one SVG.
    if let Some((encoding, data)) = res.source(media) {
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (dest_w_emu, dest_h_emu, tile_w_emu, tile_h_emu, anchor_x_emu, anchor_y_emu, flip).hash(&mut hasher);
            (media, hasher.finish())
        };
        let found = res.clipped.borrow().get(&key).cloned();
        let image = found.unwrap_or_else(|| {
            let layout = crate::render::picture_svg::TileLayout { width: tile_w_emu as f64, height: tile_h_emu as f64, x: anchor_x_emu as f64, y: anchor_y_emu as f64, flip_x: matches!(flip, ibis_doc_model::TileFlip::X | ibis_doc_model::TileFlip::Xy), flip_y: matches!(flip, ibis_doc_model::TileFlip::Y | ibis_doc_model::TileFlip::Xy) };
            let svg = crate::render::picture_svg::styled_picture(&data, encoding, dest_w_emu, dest_h_emu, None, ibis_doc_model::Crop::default(), None, Some(layout), &Default::default());
            let image = Arc::new(Image::from_bytes(ImageFormat::Svg, svg));
            res.clipped.borrow_mut().insert(key, image.clone());
            image
        });
        return container.child(img(res.svg_picture(image)).absolute().size_full().object_fit(ObjectFit::Fill));
    }
    let tile_w_px = tile_w_emu as f32 * scale;
    let tile_h_px = tile_h_emu as f32 * scale;
    let anchor_x_px = anchor_x_emu as f32 * scale;
    let anchor_y_px = anchor_y_emu as f32 * scale;
    let dest_w_px = dest_w_emu as f32 * scale;
    let dest_h_px = dest_h_emu as f32 * scale;

    let first_col = ((-anchor_x_px) / tile_w_px).floor() as i32;
    let last_col = ((dest_w_px - anchor_x_px) / tile_w_px).ceil() as i32;
    let first_row = ((-anchor_y_px) / tile_h_px).floor() as i32;
    let last_row = ((dest_h_px - anchor_y_px) / tile_h_px).ceil() as i32;

    let col_count = (last_col - first_col).max(0) as usize;
    let row_count = (last_row - first_row).max(0) as usize;
    if col_count * row_count > 10_000 {
        return container;
    }

    let mut grid = div().absolute().size_full();
    for row in first_row..last_row {
        for col in first_col..last_col {
            let x = anchor_x_px + col as f32 * tile_w_px;
            let y = anchor_y_px + row as f32 * tile_h_px;
            grid = grid.child(
                img(image.clone())
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(tile_w_px))
                    .h(px(tile_h_px))
                    .object_fit(ObjectFit::Fill),
            );
        }
    }
    container.child(grid)
}

/// The slide background and the layout's and master's shapes.
pub(crate) fn slide_backdrop(slide: &Slide, scale: f32, res: &Resources) -> Div {
    let base = match &slide.background {
        Fill::TileImage { media, tx, ty, sx, sy, flip, align } => {
            let size = slide.size.unwrap_or(ibis_doc_model::EmuSize {
                width: 12_192_000,
                height: 6_858_000,
            });
            div().absolute().size_full().top_0().left_0().child(tile_fill_element(
                *media,
                *tx,
                *ty,
                *sx,
                *sy,
                *flip,
                *align,
                (size.width, size.height),
                scale,
                res,
            ))
        }
        Fill::Blip(blip) => {
            let size = slide.size.unwrap_or(ibis_doc_model::EmuSize { width: 12_192_000, height: 6_858_000 });
            let frame = EmuRect { x: 0, y: 0, width: size.width, height: size.height };
            let base = div().absolute().size_full().top_0().left_0().overflow_hidden();
            match blip_fill_image(blip, None, (false, false), frame, res) {
                Some(image) => base.child(img(res.svg_picture(image)).absolute().size_full().object_fit(ObjectFit::Fill)),
                None => with_fill(base, &slide.background, res),
            }
        }
        _ => with_fill(div().absolute().size_full().top_0().left_0(), &slide.background, res),
    };
    // A pattern's cells over the whole slide, from its corner.
    let base = match &slide.background {
        pattern @ Fill::Pattern { .. } => {
            let pattern = pattern.clone();
            base.child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let (o, s) = (bounds.origin, bounds.size);
                        let rect = vec![o, point(o.x + s.width, o.y), point(o.x + s.width, o.y + s.height), point(o.x, o.y + s.height)];
                        paint_pattern(window, &pattern, &[rect], o, scale);
                    },
                )
                .absolute()
                .size_full(),
            )
        }
        gradient if crate::render::gradient::is_gradient(gradient) => {
            let gradient = gradient.clone();
            base.child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let (o, s) = (bounds.origin, bounds.size);
                        let (w, h) = (f32::from(s.width), f32::from(s.height));
                        let rect = vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
                        crate::render::gradient::paint_gradient(window, &gradient, &[rect], (w, h), 1.0, &|pts| {
                            pts.iter().map(|&(x, y)| point(o.x + px(x), o.y + px(y))).collect()
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
        }
        _ => base,
    };
    base
        .overflow_hidden()
        .children(slide.shown_backdrop().iter().map(|shape| shape_element(shape, shape.frame, scale, res, None)))
}

/// A shape drawn at `frame` (which may differ from `shape.frame` while it is
/// dragged). `text` replaces the drawn text, for the in-place text field.
pub(crate) fn shape_element(
    shape: &Shape,
    frame: EmuRect,
    scale: f32,
    res: &Resources,
    text: Option<AnyElement>,
) -> AnyElement {
    shape_element_with_positions(shape, frame, scale, res, text, None)
}

/// `shape_element`, recording where the shape's text is painted in
/// `positions` (for the caret and clicks while the text is edited).
pub(crate) fn shape_element_with_positions(
    shape: &Shape,
    frame: EmuRect,
    scale: f32,
    res: &Resources,
    text: Option<AnyElement>,
    positions: Option<&crate::render::rich_text::TextPositions>,
) -> AnyElement {
    let container = place(div(), frame, scale);
    // The text rectangle: the geometry's (the frame for a text box),
    // flipped with the shape.
    let (fw, fh) = (frame.width as f64, frame.height as f64);
    let (mut l, mut t, mut r, mut b) = match &shape.kind {
        ShapeKind::Auto(auto) => auto.geometry.text_rect(fw, fh),
        _ => (0.0, 0.0, fw, fh),
    };
    if shape.flip_h {
        (l, r) = (fw - r, fw - l);
    }
    if shape.flip_v {
        (t, b) = (fh - b, fh - t);
    }
    let body = shape.text_body();
    let text_turn = body.and_then(|body| text_turn(shape, body, (l, t, r, b), frame, scale));
    // Turned a quarter turn (`vert`), the text is laid out in the rectangle
    let sideways = body.is_some_and(|b| (quarter_turn(b).rem_euclid(10_800_000) == 5_400_000) != (b.upright && upright_swaps(shape.rotation)));
    let (cx, cy) = ((l + r) / 2.0, (t + b) / 2.0);
    let (w, h) = if sideways { (b - t, r - l) } else { (r - l, b - t) };
    let is_1d_line = match &shape.kind {
        ShapeKind::Auto(auto) => match &auto.geometry {
            Geometry::Formula(f) => matches!(f.preset.as_deref(), Some("line" | "lineInv" | "straightConnector1")),
            _ => false,
        },
        ShapeKind::Line(_) => true,
        _ => false,
    };
    let is_point_text = is_1d_line && w <= 0.0 && h <= 0.0;
    let text = text.or_else(|| {
        // Edited text is laid out even when empty, for its caret.
        let body = body.filter(|b| !b.is_empty() || positions.is_some())?;
        if !is_1d_line && (w <= 0.0 || h <= 0.0) && positions.is_none() {
            return None;
        }
        let (body, max_w) = if is_point_text {
            let mut modified = body.clone();
            modified.insets = ibis_doc_model::Insets::default();
            modified.wrap = false;
            (std::borrow::Cow::Owned(modified), Some(0.0))
        } else if is_1d_line {
            let mut modified = body.clone();
            modified.wrap = false;
            (std::borrow::Cow::Owned(modified), None)
        } else {
            // Turned a quarter turn by `vert`, the insets stay on the
            // shape's sides (PowerPoint's PDF: `lIns` is still at the left
            // of the shape), so they are given to the turned layout turned
            // back.
            let quarter = quarter_turn(body).rem_euclid(21_600_000);
            let turned_insets = (quarter == 5_400_000 || quarter == 16_200_000) && !(body.upright && upright_swaps(shape.rotation));
            let turned = turned_insets.then(|| {
                let mut b = body.clone();
                let i = body.insets;
                b.insets = if quarter == 5_400_000 {
                    ibis_doc_model::Insets { left: i.top, top: i.right, right: i.bottom, bottom: i.left }
                } else {
                    ibis_doc_model::Insets { left: i.bottom, top: i.left, right: i.top, bottom: i.right }
                };
                b
            });
            let fitted = fit_insets(turned.as_ref().unwrap_or(body), w, h);
            let b = match (fitted, turned) {
                (Some(f), _) => std::borrow::Cow::Owned(f),
                (None, Some(t)) => std::borrow::Cow::Owned(t),
                (None, None) => std::borrow::Cow::Borrowed(body),
            };
            (b, Some(w))
        };
        let has_stroke = match &shape.kind {
            ShapeKind::Auto(auto) => auto.stroke.is_some(),
            ShapeKind::Line(_) => true,
            _ => false,
        };
        let origin_x = if has_stroke {
            None
        } else {
            let text_left = (cx - w / 2.0) as f32 * scale;
            Some(frame.x as f32 * scale + text_left)
        };
        Some(text_body_element(&body, max_w, Some(h), scale, res, positions, text_turn, origin_x).into_any_element())
    });
    // The text goes in the text rectangle.
    let clip_text = body.is_some_and(|b| b.clip);
    let text = text.map(|text| {
        let cell = div()
            .absolute()
            .left(px((cx - w / 2.0) as f32 * scale))
            .top(px((cy - h / 2.0) as f32 * scale))
            .w(px(w.max(0.0) as f32 * scale))
            .h(px(h.max(0.0) as f32 * scale))
            .child(text);
        // Cut at the shape's edge, not the text rectangle's.
        if clip_text { div().absolute().left_0().top_0().size_full().overflow_hidden().child(cell) } else { cell }
    });
    match &shape.kind {
        ShapeKind::TextBox(_) => container.children(text).into_any_element(),
        ShapeKind::Auto(auto) => auto_shape(container, shape, auto, frame, scale, res).children(text).into_any_element(),
        ShapeKind::Picture(picture) => match res.has_picture(picture.media) {
            true => picture_element(container, shape, picture, frame, scale, res).into_any_element(),
            // A picture that cannot be drawn keeps its place visible.
            false => {
                let format = res.formats.get(&picture.media).copied().unwrap_or(MediaFormat::Other);
                placeholder_element(container, &Placeholder::Picture(format), frame, scale).into_any_element()
            }
        },
        ShapeKind::Line(stroke) => line_element(shape, stroke.clone(), frame, scale, res.color_mode).into_any_element(),
        // A SmartArt graphic's shapes are drawn on their own.
        ShapeKind::Placeholder(Placeholder::Diagram) => container.into_any_element(),
        ShapeKind::Placeholder(kind) => placeholder_element(container, kind, frame, scale).into_any_element(),
        ShapeKind::Chart(chart) => container.child(crate::render::chart_view::chart_element(chart, frame, scale, res)).into_any_element(),
        // A table's rotation and flip are not applied (PowerPoint draws it upright, as measured by PTBL-00251 to 00255).
        ShapeKind::Table(table) => container.child(crate::render::table_view::table_element(table, frame, scale, res)).into_any_element(),
        ShapeKind::Sheet(sheet) => container.child(crate::xlsx::sheet_view::sheet_element(sheet, frame, scale, res)).into_any_element(),
        ShapeKind::Page { document, index } => container.child(crate::docx::page_view::page_element(document, *index, frame, scale, res)).into_any_element(),
        ShapeKind::Slicer(slicer) => container.child(crate::xlsx::slicer_view::slicer_element(slicer, frame, scale)).into_any_element(),
        ShapeKind::Timeline(timeline) => container.child(crate::xlsx::timeline_view::timeline_element(timeline, frame, scale)).into_any_element(),
        ShapeKind::Control(control) => container.child(crate::xlsx::control_view::control_element(control, frame, scale)).into_any_element(),
    }
}

/// `body` with insets that fit a `w` by `h` text rectangle, if its own do
/// not: insets wider than the rectangle keep the middle of the space between
/// them and shrink until the text has the whole rectangle, as PowerPoint
/// centres text in a box narrower than its insets (a 0.14 in box with 0.1 in
/// insets). `None` if the insets fit.
fn fit_insets(body: &TextBody, w: f64, h: f64) -> Option<TextBody> {
    let fit = |a: i64, b: i64, size: f64| -> Option<(i64, i64)> {
        if (a + b) as f64 <= size || size <= 0.0 {
            return None;
        }
        let middle = ((a as f64 + size - b as f64) / 2.0).clamp(0.0, size);
        let half = middle.min(size - middle);
        Some(((middle - half).round() as i64, (size - middle - half).round() as i64))
    };
    let i = body.insets;
    let across = fit(i.left, i.right, w);
    let down = fit(i.top, i.bottom, h);
    if across.is_none() && down.is_none() {
        return None;
    }
    let mut body = body.clone();
    if let Some((l, r)) = across {
        (body.insets.left, body.insets.right) = (l, r);
    }
    if let Some((t, b)) = down {
        (body.insets.top, body.insets.bottom) = (t, b);
    }
    Some(body)
}

/// How a shape's text is turned, as PowerPoint turns it (measured on its
/// PDF: the glyphs' angles in the shapes of the PXFM coverage tests): with
/// the shape's rotation, about the shape's middle; a shape flipped upside
/// down has its text turned half a turn more (a flip left to right does
/// not turn it), and `bodyPr/@rot` and `vert` turn it more, about the
/// middle of its rectangle (`rect`, frame EMU, already flipped). Upright
/// text (`upright`) turns with neither the shape nor its flips: its
/// rectangle's middle goes round with the shape, the text stays level
/// there (laid out in the rectangle turned a quarter turn when the shape
/// is turned nearer a quarter turn than level, `upright_swaps`). `None`
/// when it is not turned.
/// Whether upright text in a shape turned `rotation` is laid out in its
/// rectangle turned a quarter turn: when the shape is nearer upright on its
/// side (measured: 90° and 270° do, 30°, 135° and 180° do not).
fn upright_swaps(rotation: i32) -> bool {
    let a = rotation.rem_euclid(10_800_000);
    (2_700_000..8_100_000).contains(&a)
}

/// How far the text is turned as a whole for `vert`: East Asian vertical
/// writing is laid out as the text turned a quarter turn clockwise (`vert`),
/// with its East Asian characters drawn upright.
fn quarter_turn(body: &TextBody) -> i32 {
    if body.vertical && matches!(body.vertical_kind, ibis_doc_model::VerticalKind::EastAsian | ibis_doc_model::VerticalKind::Mongolian) {
        5_400_000
    } else {
        body.sideways
    }
}

fn text_turn(shape: &Shape, body: &TextBody, (l, t, r, b): (f64, f64, f64, f64), frame: EmuRect, scale: f32) -> Option<crate::render::rich_text::TextTurn> {
    let degrees = |v: i32| v.rem_euclid(21_600_000) as f32 / 60_000.0;
    let shape_turn = degrees(shape.rotation);
    // Upright text is turned back as much as the shape turns it.
    let flip_turn = if body.upright { 360.0 - shape_turn } else if shape.flip_v { 180.0 } else { 0.0 };
    let text = (degrees(body.rotation) + degrees(quarter_turn(body)) + flip_turn) % 360.0;
    if shape_turn == 0.0 && text == 0.0 {
        return None;
    }
    let (fw, fh) = (frame.width as f64, frame.height as f64);
    let frame_offset = (((fw - l - r) / 2.0) as f32 * scale, ((fh - t - b) / 2.0) as f32 * scale);
    let shape_turned = (shape_turn != 0.0 || shape.flip_v) && !body.upright;
    Some(crate::render::rich_text::TextTurn { text, shape: shape_turn, frame: frame_offset, shape_turned })
}

/// The spread and blur, in pixels, of the box shadow drawn for a glow
/// `r` pixels wide. PowerPoint's glow is its full colour at the shape's edge
/// and fades out at the radius (measured against its PDF: a 15 pt glow is
/// full colour a third of the way out, half at half the radius and gone
/// at the radius).
fn glow_spread_blur(r: f32) -> (f32, f32) {
    (r * 0.5, r * 0.2)
}

/// The outline of a rectangle, rounded rectangle or ellipse `w` by `h`
/// pixels: centred on its edge, half of it outside, as PowerPoint draws it.
/// A dashed rectangle's dashes are PowerPoint's "dash", four line widths
/// with gaps of three, starting at each corner (a dashed box on a slide of a
/// user's private presentation, against the PDF).
/// `at` is where the frame is meant to be on the slide (pixels, before
/// GPUI snaps the element to whole pixels).
fn outline_element(stroke: &Stroke, w: f32, h: f32, radius: Pixels, scale: f32, rect: bool, at: (f32, f32), mode: ColorMode) -> AnyElement {
    let width = f32::from(stroke_px(stroke, scale));
    let half = width / 2.0;
    let color = hsla_mode(stroke.color, mode);
    if stroke.dashed && rect {
        let (dash, gap) = ((width * 4.0).max(3.0), (width * 3.0).max(2.0));
        return canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let o = bounds.origin;
                // Each edge from its corner, a dash at a time.
                let mut edge = |x0: f32, y0: f32, len: f32, across: bool| {
                    let mut t = 0.0;
                    while t < len {
                        let l = dash.min(len - t);
                        let r = if across {
                            Bounds::new(point(o.x + px(x0 + t - half), o.y + px(y0 - half)), size(px(l + if t + l >= len { width } else { 0.0 }), px(width)))
                        } else {
                            Bounds::new(point(o.x + px(x0 - half), o.y + px(y0 + t - half)), size(px(width), px(l + if t + l >= len { width } else { 0.0 })))
                        };
                        window.paint_quad(fill(r, color));
                        t += dash + gap;
                    }
                };
                edge(0.0, 0.0, w, true);
                edge(0.0, h, w, true);
                edge(0.0, 0.0, h, false);
                edge(w, 0.0, h, false);
            },
        )
        .absolute()
        .left_0()
        .top_0()
        .w(px(w))
        .h(px(h))
        .into_any_element();
    }
    let inset = stroke.alignment == ibis_doc_model::LineAlignment::Inset;
    if rect && !stroke.dashed {
        // Each edge of the outline on the pixels nearest to where it is
        // meant to be, as PowerPoint puts it, not from the element's
        // snapped corner (a pixel off on one side or the other).
        return canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let sf = window.scale_factor();
                let dev = |v: f32| (v * sf).round() / sf;
                let exact = exact_origin(bounds.origin, at, sf);
                let (ox, oy) = (f32::from(exact.x), f32::from(exact.y));
                let (l, r) = if inset {
                    (dev(ox), dev(ox + w))
                } else {
                    (dev(ox - half), dev(ox + w + half))
                };
                let (t, b) = if inset {
                    (dev(oy), dev(oy + h))
                } else {
                    (dev(oy - half), dev(oy + h + half))
                };
                let th = dev(width).max(1.0 / sf).min((r - l) / 2.0).min((b - t) / 2.0);
                let quad = |x: f32, y: f32, qw: f32, qh: f32| fill(Bounds::new(point(px(x), px(y)), size(px(qw), px(qh))), color);
                window.paint_quad(quad(l, t, r - l, th));
                window.paint_quad(quad(l, b - th, r - l, th));
                window.paint_quad(quad(l, t + th, th, b - t - 2.0 * th));
                window.paint_quad(quad(r - th, t + th, th, b - t - 2.0 * th));
            },
        )
        .absolute()
        .left_0()
        .top_0()
        .w(px(w))
        .h(px(h))
        .into_any_element();
    }
    if !stroke.dashed {
        let stroke_half = if inset { px(0.) } else { px(half) };
        return canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let sf = window.scale_factor();
                let dev = |v: f32| (v * sf).round() / sf;
                let exact = exact_origin(bounds.origin, at, sf);
                let (ox, oy) = (f32::from(exact.x), f32::from(exact.y));
                let (l, r) = if inset {
                    (dev(ox), dev(ox + w))
                } else {
                    (dev(ox - half), dev(ox + w + half))
                };
                let (t, b) = if inset {
                    (dev(oy), dev(oy + h))
                } else {
                    (dev(oy - half), dev(oy + h + half))
                };
                let quad_bounds = Bounds::new(point(px(l), px(t)), size(px(r - l), px(b - t)));
                let border_widths = Edges::all(px(dev(width).max(1.0 / sf)));
                window.paint_quad(quad(quad_bounds, radius + stroke_half, transparent_black(), border_widths, color, BorderStyle::default()));
            },
        )
        .absolute()
        .left_0()
        .top_0()
        .w(px(w))
        .h(px(h))
        .into_any_element();
    }
    div()
        .absolute()
        .left(px(-half))
        .top(px(-half))
        .w(px(w + width))
        .h(px(h + width))
        .rounded(radius + px(half))
        .border(px(width))
        .border_color(color)
        .when(stroke.dashed, |d| d.border_dashed())
        .into_any_element()
}

/// Stroke width on screen. GPUI's path rasterizer drops paths thinner than a
/// pixel, so thin lines are drawn one pixel wide (as PowerPoint does too).
fn stroke_px(stroke: &Stroke, scale: f32) -> Pixels {
    px((stroke.width as f32 * scale).max(1.0))
}

/// Stroke width for painting: at least a pixel of the screen, so a thin line
/// on a high-density screen is one device pixel wide, as PowerPoint draws it.
fn painted_stroke_px(stroke: &Stroke, scale: f32, window: &Window) -> Pixels {
    px((stroke.width as f32 * scale).max(1.0 / window.scale_factor()))
}

/// The colour of a face a bevel is shaded from: a solid fill's, or a gradient's first stop.
fn bevel_base(fill: &Fill) -> Option<([u8; 3], bool)> {
    let rgb = match fill {
        Fill::Solid(c) => c.rgb,
        Fill::Linear { stops, .. } | Fill::TileLinear { stops, .. } => stops.first()?.color.rgb,
        _ => return None,
    };
    let ibis_doc_model::Rgb(v) = rgb;
    Some(([((v >> 16) & 0xff) as u8, ((v >> 8) & 0xff) as u8, (v & 0xff) as u8], matches!(fill, Fill::Solid(c) if c.alpha == 255)))
}

/// Whether a bevelled shape's outline is drawn with the bevel (lit by its sides): a plain line round a solid face.
fn bevel_outline(auto: &AutoShape) -> bool {
    auto.bevel.is_some() && bevel_base(&auto.fill).is_some_and(|(_, solid)| solid) && auto.stroke.as_ref().is_some_and(|s| !s.dashed && s.compound == ibis_doc_model::LineCompound::Single && s.color.alpha == 255)
}

/// Paint the outline of a bevelled shape, each edge lit by its side.
fn paint_bevel_stroke(window: &mut Window, pts: &[Point<Pixels>], bevel: &ibis_doc_model::Bevel, scale: f32, stroke: &Stroke) {
    let flat: Vec<(f32, f32)> = pts.iter().map(|p| (f32::from(p.x), f32::from(p.y))).collect();
    let ratio = if bevel.width > 0 { bevel.height as f32 / bevel.width as f32 } else { 1.0 };
    let ibis_doc_model::Rgb(v) = stroke.color.rgb;
    let rgb = [((v >> 16) & 0xff) as u8, ((v >> 8) & 0xff) as u8, (v & 0xff) as u8];
    let width = f32::from(painted_stroke_px(stroke, scale, window));
    for (poly, [r, g, b]) in crate::render::bevel::stroke_pieces(&flat, width, ratio, bevel.preset, rgb) {
        let corners: Vec<Point<Pixels>> = poly.iter().map(|&(x, y)| point(px(x), px(y))).collect();
        if let Some(path) = compound_path(&[(corners, true)], None) {
            window.paint_path(path, hsla(ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)), alpha: 255 }));
        }
    }
}

/// Paint a bevel (面取り) along the closed outline `pts` (pixels).
fn paint_bevel(window: &mut Window, pts: &[Point<Pixels>], bevel: &ibis_doc_model::Bevel, scale: f32, (base, solid): ([u8; 3], bool)) {
    let flat: Vec<(f32, f32)> = pts.iter().map(|p| (f32::from(p.x), f32::from(p.y))).collect();
    let width = bevel.width as f32 * scale;
    let ratio = if bevel.width > 0 { bevel.height as f32 / bevel.width as f32 } else { 1.0 };
    for (poly, [r, g, b]) in crate::render::bevel::pieces(&flat, width, ratio, bevel.preset, base, solid) {
        let corners: Vec<Point<Pixels>> = poly.iter().map(|&(x, y)| point(px(x), px(y))).collect();
        if let Some(path) = compound_path(&[(corners, true)], None) {
            window.paint_path(path, hsla(ibis_doc_model::Color { rgb: ibis_doc_model::Rgb(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)), alpha: 255 }));
        }
    }
}

fn auto_shape(container: Div, shape: &Shape, auto: &AutoShape, frame: EmuRect, scale: f32, res: &Resources) -> Div {
    let rotated = shape.rotation.rem_euclid(21_600_000) != 0;
    // Soft edges (`a:softEdge`) on a plain filled rectangle: the shape fades
    // out over the radius inside its edge, and its outline goes with it
    // (PEFF-00081〜00083 against PowerPoint's PDF). Drawn as the blurred
    // shadow of a box inside the shape: fitting a Gaussian edge to the
    // reference's fades puts it 0.8 of the radius in (8.5 px for a 10 px
    // radius, 15.6 for 20) with a deviation of a third of the radius
    // (3.4 px, 6.8 px).
    if let (Some(soft), Fill::Solid(color), Geometry::Rect | Geometry::RoundRect { .. }) = (auto.soft_edge, &auto.fill, &auto.geometry)
        && !rotated
        && auto.text.as_ref().is_none_or(|t| t.paragraphs.iter().all(|p| p.runs.is_empty()))
    {
        let w = frame.width as f32 * scale;
        let h = frame.height as f32 * scale;
        let r = (soft as f32 * scale).min(w / 2.0).min(h / 2.0);
        let corner = match auto.geometry {
            Geometry::RoundRect { radius } => w.min(h) * radius as f32 / 100_000.0,
            _ => 0.0,
        };
        let inset = r * 0.8;
        return container.child(
            div()
                .absolute()
                .left(px(inset))
                .top(px(inset))
                .w(px((w - 2.0 * inset).max(0.0)))
                .h(px((h - 2.0 * inset).max(0.0)))
                .rounded(px((corner - inset).max(0.0)))
                .shadow(vec![BoxShadow { color: hsla(*color), offset: point(px(0.), px(0.)), blur_radius: px(r / 3.0), spread_radius: px(0.), inset: false }]),
        );
    }
    // (A pattern is painted cell by cell along the outline, so as a path.)
    let simple = !rotated
        && !matches!(auto.fill, Fill::Pattern { .. })
        && !crate::render::gradient::is_gradient(&auto.fill)
        && !auto.stroke.as_ref().is_some_and(|s| s.dashed || s.compound != ibis_doc_model::LineCompound::Single)
        && matches!(auto.geometry, Geometry::Rect | Geometry::RoundRect { .. })
        // Drawn exactly, as paths (the quads would be put on whole pixels),
        // but for what only the elements draw (a picture fill, shadows).
        && (!res.exact || matches!(auto.fill, Fill::Image(_)) || auto.shadow.is_some() || auto.glow.is_some() || auto.inner_shadow.is_some() || auto.reflection.is_some());

    if simple {
        let w = frame.width as f32 * scale;
        let h = frame.height as f32 * scale;
        let radius = match auto.geometry {
            Geometry::RoundRect { radius } => px(w.min(h) * radius as f32 / 100_000.0),
            Geometry::Ellipse => px(w.max(h)),
            _ => px(0.),
        };
        let paint = |container: Div| {
            let mut container = container;
            let stroke_half = auto.stroke.as_ref().map(|s| stroke_px(s, scale) / 2.0).unwrap_or(px(0.));
            let half = f32::from(stroke_half);
            let at = (frame.x as f32 * scale, frame.y as f32 * scale);

            // 1. Hard shadows (blur < 0.5) drawn exactly onto the background via paint_quad
            if let Some(s) = auto.shadow.filter(|s| auto.fill != Fill::None && !s.has_transform()) {
                let (dx, dy, blur, color) = shadow_parts(s, scale);
                if blur < 0.5 {
                    container = container.child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                let sf = window.scale_factor();
                                let dev = |v: f32| (v * sf).round() / sf;
                                let exact = exact_origin(bounds.origin, at, sf);
                                let (ox, oy) = (f32::from(exact.x), f32::from(exact.y));
                                let frac = |v: f32| {
                                    let s = v * sf;
                                    s - s.floor()
                                };
                                let pad_l = if dx.abs() < 0.5 { 1.0 / sf } else { 0.0 };
                                let pad_r = if dx.abs() < 0.5 && frac(ox + w + half) < 0.3 { 1.0 / sf } else { 0.0 };
                                let pad_t = if dy.abs() < 0.5 && frac(oy - half) > 0.85 { 1.0 / sf } else { 0.0 };
                                let pad_b = if dy.abs() < 0.5 && frac(oy + h + half) > 0.85 { 1.0 / sf } else { 0.0 };
                                let (l, r) = (dev(ox + dx - half - pad_l), dev(ox + dx + w + half + pad_r));
                                let (t, b) = (dev(oy + dy - half - pad_t), dev(oy + dy + h + half + pad_b));
                                let quad_bounds = Bounds::new(point(px(l), px(t)), size(px(r - l), px(b - t)));
                                window.paint_quad(quad(quad_bounds, radius + stroke_half + px(pad_l.max(pad_r).max(pad_t).max(pad_b)), color, Edges::default(), transparent_black(), BorderStyle::default()));
                            },
                        )
                        .absolute()
                        .size_full(),
                    );
                }
            }

            // 1b. Transformed shadows (outerShdw with sx, sy, kx, ky, algn)
            if let Some(s) = auto.shadow.filter(|s| auto.fill != Fill::None && s.has_transform()) {
                let stroke_half_emu = auto.stroke.as_ref().map(|st| st.width / 2).unwrap_or(0);
                let (w_emu, h_emu) = (frame.width as f64, frame.height as f64);
                let s_emu = stroke_half_emu as f64;
                let anchor = shadow_anchor(s.align, w_emu, h_emu, s_emu);
                let outlines = shadow_outlines(&auto.geometry, frame, s_emu);
                let blur = s.blur as f32 * scale;
                let color = hsla(s.color);
                container = container.child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            let sf = window.scale_factor();
                            let exact = exact_origin(bounds.origin, at, sf);
                            render_transformed_shadow(
                                window,
                                &outlines,
                                anchor,
                                &s,
                                blur,
                                color,
                                exact,
                                scale,
                            );
                        },
                    )
                    .absolute()
                    .size_full(),
                );
            }

            // 2. Shape body fill
            // A bevelled shape's outline is lit with it (drawn with the bevel).
            let bevel_stroked = bevel_outline(auto);
            let round_rect_exact = matches!(auto.geometry, Geometry::RoundRect { .. })
                && matches!(auto.fill, Fill::Solid(_) | Fill::None)
                && !auto.stroke.as_ref().is_some_and(|s| s.dashed)
                && !bevel_stroked;

            let mut body = div().size_full().rounded(radius);
            body = match &auto.fill {
                _ if round_rect_exact => {
                    let fill_color = match &auto.fill {
                        Fill::Solid(color) => hsla_mode(*color, res.color_mode),
                        _ => transparent_black(),
                    };
                    let stroke_info = if auto.inner_shadow.is_none() {
                        auto.stroke.as_ref().map(|s| (stroke_px(s, scale), hsla_mode(s.color, res.color_mode)))
                    } else {
                        None
                    };
                    body.child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                let sf = window.scale_factor();
                                let dev = |v: f32| (v * sf).round() / sf;
                                let exact = exact_origin(bounds.origin, at, sf);
                                let (ox, oy) = (f32::from(exact.x), f32::from(exact.y));
                                let (l, r) = (dev(ox - half), dev(ox + w + half));
                                let (t, b) = (dev(oy - half), dev(oy + h + half));
                                let quad_bounds = Bounds::new(point(px(l), px(t)), size(px(r - l), px(b - t)));
                                let (border_widths, border_color) = match stroke_info {
                                    Some((spx, sclr)) => (Edges::all(spx), sclr),
                                    None => (Edges::default(), transparent_black()),
                                };
                                window.paint_quad(quad(quad_bounds, radius + stroke_half, fill_color, border_widths, border_color, BorderStyle::default()));
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                }
                Fill::Solid(color) if auto.geometry == Geometry::Rect => {
                    let color = hsla_mode(*color, res.color_mode);
                    body.child(
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, _| {
                                let sf = window.scale_factor();
                                let dev = |v: f32| (v * sf).round() / sf;
                                let exact = exact_origin(bounds.origin, at, sf);
                                let (ox, oy) = (f32::from(exact.x), f32::from(exact.y));
                                let (l, r) = (dev(ox), dev(ox + w));
                                let (t, b) = (dev(oy), dev(oy + h));
                                window.paint_quad(fill(Bounds::new(point(px(l), px(t)), size(px(r - l), px(b - t))), color));
                            },
                        )
                        .absolute()
                        .size_full(),
                    )
                }
                Fill::TileImage { media, tx, ty, sx, sy, flip, align } => body.child(tile_fill_element(
                    *media,
                    *tx,
                    *ty,
                    *sx,
                    *sy,
                    *flip,
                    *align,
                    (frame.width, frame.height),
                    scale,
                    res,
                )),
                Fill::Blip(blip) => match blip_fill_image(blip, Some(&auto.geometry), (shape.flip_h, shape.flip_v), frame, res) {
                    Some(image) => body.child(img(res.svg_picture(image)).absolute().size_full().object_fit(ObjectFit::Fill)),
                    None => with_fill(body, &auto.fill, res),
                },
                _ => with_fill(body, &auto.fill, res),
            };
            if matches!(auto.fill, Fill::Image(_) | Fill::TileImage { .. } | Fill::Blip(_)) {
                body = body.overflow_hidden();
            }

            // 3. Blurred shadow or glow
            let mut shadows = Vec::new();
            if let Some(s) = auto.shadow.filter(|s| auto.fill != Fill::None && !s.has_transform()) {
                let (dx, dy, blur, color) = shadow_parts(s, scale);
                if blur >= 0.5 {
                    // GPUI's blur radius is the Gaussian's standard deviation;
                    // PowerPoint's shadow fades over about its blur radius
                    // either side of the edge, a deviation of a third of it
                    // (measured on its PDF: an 8 pt blur fades from 10 % to
                    // 90 % over 7 pt). PEFF 23 → 46.
                    shadows.push(BoxShadow { color, offset: point(px(dx), px(dy)), blur_radius: px(blur / 3.0), spread_radius: stroke_half, inset: false });
                }
            }
            // The glow apart, on a box grown by its spread with its corners
            // rounded as much more: PowerPoint's glow follows the outline
            // out, round at the corners (PEFF-00079), where a spread box
            // shadow keeps the shape's corners.
            let glow = auto.glow.filter(|_| auto.fill != Fill::None || auto.stroke.is_some()).map(|(color, glow_radius)| {
                let r = glow_radius as f32 * scale;
                let (spread, blur) = glow_spread_blur(r);
                div()
                    .absolute()
                    .left(px(-spread))
                    .top(px(-spread))
                    .w(px(w + 2.0 * spread))
                    .h(px(h + 2.0 * spread))
                    .rounded(radius + px(spread))
                    .shadow(vec![BoxShadow { color: hsla(color), offset: point(px(0.), px(0.)), blur_radius: px(blur), spread_radius: px(0.), inset: false }])
            });
            if !shadows.is_empty() {
                body = body.shadow(shadows);
            }
            if let Some(glow) = glow {
                container = container.child(glow);
            }
            // A reflection mirrored down under the shape (`sy` −100 %, put
            // under it, fading downwards): the shape's fill, outline
            // included in its size, fading from `stA` at `stPos` to `endA`
            // at `endPos` of its height, `dist` below the outline
            // (PEFF-00084・00085). Its blur is not drawn.
            if let (Some(r), Fill::Solid(fill_color)) = (auto.reflection, &auto.fill)
                && r.sy == -100_000
                && r.fade_direction == 5_400_000
                && (r.distance == 0 || r.direction == 5_400_000)
                && matches!(r.align, ibis_doc_model::TileAlign::Bottom | ibis_doc_model::TileAlign::BottomLeft)
            {
                let alpha = |a: i32| {
                    let mut c = hsla(*fill_color);
                    c.a *= (a as f32 / 100_000.0).clamp(0.0, 1.0);
                    c
                };
                let gap = r.distance as f32 * scale;
                container = container.child(
                    div()
                        .absolute()
                        .left(px(-half))
                        .top(px(h + half + gap))
                        .w(px(w + 2.0 * half))
                        .h(px(h + 2.0 * half))
                        .rounded(radius + stroke_half)
                        .bg(linear_gradient(
                            180.0,
                            linear_color_stop(alpha(r.start_alpha), r.start_pos as f32 / 100_000.0),
                            linear_color_stop(alpha(r.end_alpha), r.end_pos as f32 / 100_000.0),
                        )),
                );
            }

            // 4. Add body and outline
            container = container.child(body);
            // A shadow inside (`a:innerShdw`), over the fill (which the body
            // may paint in a child), blurred as the outer one.
            if let Some(s) = auto.inner_shadow.filter(|_| auto.fill != Fill::None) {
                let (dx, dy, blur, color) = shadow_parts(s, scale);
                // GPUI moves the hole by the offset, so the shadow falls on the
                // other side: PowerPoint's falls towards its direction.
                let inner = BoxShadow { color, offset: point(px(-dx), px(-dy)), blur_radius: px(blur / 2.0), spread_radius: px(0.), inset: true };

                container = container.child(div().absolute().top_0().left_0().size_full().rounded(radius).shadow(vec![inner]));
            }

            // 3D bevel effect (面取り): the strips along the edges, shaded by the preset's profile (render/bevel.rs).
            if let (Some(bevel), Some(base)) = (auto.bevel, bevel_base(&auto.fill)) {
                let geometry = auto.geometry.clone();
                let stroke_for_bevel = auto.stroke.clone();
                container = container.child(
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            for outline in geometry_outlines(&geometry, frame).iter().filter(|o| o.fill == ibis_doc_model::PathFill::Normal) {
                                for sub in outline.subpaths.iter().filter(|s| s.closed) {
                                    let pts: Vec<Point<Pixels>> = sub.points.iter().map(|&(x, y)| point(bounds.origin.x + px(x as f32 * scale), bounds.origin.y + px(y as f32 * scale))).collect();
                                    paint_bevel(window, &pts, &bevel, scale, base);
                                    if let (true, Some(stroke)) = (bevel_stroked, &stroke_for_bevel) {
                                        paint_bevel_stroke(window, &pts, &bevel, scale, stroke);
                                    }
                                }
                            }
                        },
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                );
            }

            if let Some(stroke) = auto.stroke.as_ref().filter(|_| !bevel_stroked) {
                if round_rect_exact && auto.inner_shadow.is_none() {
                    // Handled inside body canvas above
                } else {
                    container = container.child(outline_element(stroke, w, h, radius, scale, auto.geometry == Geometry::Rect, at, res.color_mode));
                }
            }
            container

        };
        return match auto.tilt {
            // The tilted body is drawn apart from the frame, which keeps the text.
            Some(tilt) => {
                let (x, y, tw, th) = tilted_frame(tilt, w, h, scale);
                let front = paint(div().absolute().left(px(x)).top(px(y)).w(px(tw)).h(px(th)));
                if tilt.extrusion_h > 0 {
                    let ext_px = (tilt.extrusion_h as f32 * scale).max(1.0);
                    let ext_clr = tilt
                        .extrusion_color
                        .map(hsla)
                        .unwrap_or_else(|| Hsla { a: 0.75, ..hsla(DocColor::opaque(Rgb(0x404040))) });
                    let ext_div = div()
                        .absolute()
                        .left(px(x))
                        .top(px(y + 1.0))
                        .w(px(tw))
                        .h(px(th + ext_px))
                        .rounded(radius)
                        .bg(ext_clr);
                    container.child(ext_div).child(front)
                } else {
                    container.child(front)
                }
            }
            None => paint(container),
        };
    }

    // Everything else is drawn as paths, which can also be rotated and flipped.
    // A picture fill is cut to the outline (photos in rounded frames); when
    // it cannot be, or the shape is rotated, it covers the frame.
    let outlines = geometry_outlines(&auto.geometry, frame);
    let container = match &auto.fill {
        Fill::Image(media) => match (!rotated).then(|| clipped_fill(*media, shape, &outlines, frame, res)).flatten() {
            Some(image) => container.child(img(res.svg_picture(image)).absolute().size_full().object_fit(ObjectFit::Fill)),
            None => match rotated
                .then(|| {
                    let mut pic = Picture::new(*media);
                    pic.geometry = Some(auto.geometry.clone());
                    rotated_image(shape, &pic, frame, false, res)
                })
                .flatten()
            {
                Some((image, bounds)) => container.child(place(img(res.svg_picture(image)), bounds, scale).object_fit(ObjectFit::Fill)),
                None => with_fill(container.overflow_hidden(), &auto.fill, res),
            },
        },
        Fill::TileImage { media, tx, ty, sx, sy, flip, align } => container.child(tile_fill_element(
            *media,
            *tx,
            *ty,
            *sx,
            *sy,
            *flip,
            *align,
            (frame.width, frame.height),
            scale,
            res,
        )),
        Fill::Blip(blip) => match (!rotated).then(|| blip_fill_image(blip, Some(&auto.geometry), (shape.flip_h, shape.flip_v), frame, res)).flatten() {
            Some(image) => container.child(img(res.svg_picture(image)).absolute().size_full().object_fit(ObjectFit::Fill)),
            None => match rotated
                .then(|| {
                    let mut pic = Picture::new(blip.media);
                    pic.crop = blip.crop;
                    pic.fill_rect = blip.fill_rect;
                    pic.tile = blip.tile;
                    pic.duotone = blip.duotone;
                    pic.luminance = blip.luminance;
                    pic.effects = blip.effects;
                    pic.geometry = Some(auto.geometry.clone());
                    rotated_image(shape, &pic, frame, !blip.rot_with_shape, res)
                })
                .flatten()
            {
                Some((image, bounds)) => container.child(place(img(res.svg_picture(image)), bounds, scale).object_fit(ObjectFit::Fill)),
                None => with_fill(container.overflow_hidden(), &auto.fill, res),
            },
        },
        _ => container,
    };
    let fill = turned_fill(&auto.fill, shape);
    // A gradient is painted in the shape's own coordinates, so it turns
    // and flips with the shape (`rotWithShape`).
    let own_fill = auto.fill.clone();
    let bevel_spec = auto.bevel;
    let bevel_stroked_path = bevel_outline(auto);
    let box_size = (frame.width as f32, frame.height as f32);
    let stroke = auto.stroke.clone();
    let place = Placement::new(shape, frame);
    // The slide's corner, from the shape's place on it (patterns start there).
    let at = (shape.frame.x as f32 * scale, shape.frame.y as f32 * scale);
    // (Shapes are laid out on whole pixels; the slide's corner is on one too.)
    let slide_origin = move |o: Point<Pixels>| point(px((f32::from(o.x) - at.0).round()), px((f32::from(o.y) - at.1).round()));
    let stroke_half_emu = auto.stroke.as_ref().map(|st| st.width / 2).unwrap_or(0);
    let shadow_outlines = shadow_outlines(&auto.geometry, frame, stroke_half_emu as f64);
    let shadow_spec = auto.shadow.filter(|_| auto.fill != Fill::None);
    let at_px = (frame.x as f32 * scale, frame.y as f32 * scale);
    // Level and upright edges on whole pixels, as the screen has them; a
    // closed outline's of two pixels or more even when drawn exactly, as
    // poppler adjusts them (at 2 px a point: a rectangle's 1.5 pt outline
    // on whole pixels, its 0.75 pt one and open lines smoothed across them).
    let on_pixels = !res.exact;
    let color_mode = res.color_mode;
    let path_closes: Vec<bool> = match &auto.geometry {
        Geometry::Formula(f) => f.def.paths.iter().map(|p| p.ops.contains(&ibis_doc_model::PathOp::Close)).collect(),
        _ => outlines.iter().map(|o| o.subpaths.iter().any(|s| s.closed)).collect(),
    };
    container.child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                // Where the frame is meant to be (GPUI put the element on
                // whole pixels): the outline is drawn there, as PowerPoint
                // draws it.
                let exact = exact_origin(bounds.origin, at_px, window.scale_factor());
                // The shadow: the shape's outline moved by the offset, painted
                // a few times around it, faintly, in place of a blur.
                if let Some(s) = shadow_spec {
                    if s.has_transform() {
                        let (w_emu, h_emu) = (frame.width as f64, frame.height as f64);
                        let anchor = shadow_anchor(s.align, w_emu, h_emu, stroke_half_emu as f64);
                        let blur = s.blur as f32 * scale;
                        let color = hsla_mode(s.color, color_mode);
                        render_transformed_shadow(
                            window,
                            &shadow_outlines,
                            anchor,
                            &s,
                            blur,
                            color,
                            exact,
                            scale,
                        );
                    } else {
                        let (dx, dy, blur, color) = shadow_parts(s, scale);
                        let spots: Vec<(f32, f32)> = if blur < 0.5 {
                            vec![(0.0, 0.0)]
                        } else {
                            (0..8).map(|i| (i as f32 * std::f32::consts::FRAC_PI_4).sin_cos()).map(|(s, c)| (c * blur / 2.0, s * blur / 2.0)).chain([(0.0, 0.0)]).collect()
                        };
                        let faint = Hsla { a: color.a / spots.len() as f32 * 1.6, ..color };
                        for (sx, sy) in &spots {
                            let origin = point(exact.x + px(dx + sx), exact.y + px(dy + sy));
                            for outline in outlines.iter().filter(|o| o.fill != ibis_doc_model::PathFill::None) {
                                let subpaths: Vec<(Vec<Point<Pixels>>, bool)> = outline
                                    .subpaths
                                    .iter()
                                    .map(|s| {
                                        let points: Vec<(f32, f32)> = s.points.iter().map(|&(x, y)| (x as f32, y as f32)).collect();
                                        (place.map(&points, origin, scale), s.closed)
                                    })
                                    .collect();
                                if let Some(p) = compound_path(&subpaths, None) {
                                    window.paint_path(p, faint);
                                }
                            }
                        }
                    }
                }
                for (outline_idx, outline) in outlines.iter().enumerate() {
                    let path_has_close = path_closes.get(outline_idx).copied().unwrap_or(false);
                    let subpaths: Vec<(Vec<Point<Pixels>>, bool)> = outline
                        .subpaths
                        .iter()
                        .map(|s| {
                            let points: Vec<(f32, f32)> = s.points.iter().map(|&(x, y)| (x as f32, y as f32)).collect();
                            (place.map(&points, exact, scale), s.closed)
                        })
                        .collect();
                    if outline.fill == ibis_doc_model::PathFill::Normal && crate::render::gradient::is_gradient(&own_fill) {
                        let own: Vec<Vec<(f32, f32)>> = outline.subpaths.iter().map(|s| s.points.iter().map(|&(x, y)| (x as f32, y as f32)).collect()).collect();
                        let origin = exact;
                        crate::render::gradient::paint_gradient(window, &own_fill, &own, box_size, scale, &|pts| place.map(pts, origin, scale));
                    } else if let Some(bg) = shaded_background(&fill, outline.fill)
                        && let Some(p) = compound_path(&subpaths, None)
                    {
                        window.paint_path(p, bg);
                        if matches!(fill, Fill::Pattern { .. }) {
                            let closed: Vec<Vec<Point<Pixels>>> = subpaths.iter().filter(|(_, c)| *c).map(|(p, _)| p.clone()).collect();
                            paint_pattern(window, &fill, &closed, slide_origin(bounds.origin), scale);
                        }
                    }
                    // The bevel over the face (under the outline).
                    if let (Some(bevel), Some(base), true) = (bevel_spec, bevel_base(&own_fill), outline.fill == ibis_doc_model::PathFill::Normal) {
                        for (pts, closed) in subpaths.iter().filter(|(_, c)| *c) {
                            let _ = closed;
                            paint_bevel(window, pts, &bevel, scale, base);
                        }
                    }
                    if let (Some(bevel), true, true, Some(stroke)) = (bevel_spec, bevel_stroked_path, outline.stroke, &stroke) {
                        for (pts, _) in subpaths.iter().filter(|(_, c)| *c) {
                            paint_bevel_stroke(window, pts, &bevel, scale, stroke);
                        }
                    }
                    if outline.stroke
                        && !bevel_stroked_path
                        && let Some(stroke) = &stroke
                    {
                        let width = painted_stroke_px(stroke, scale, window);
                        let inset_offset = if stroke.alignment == ibis_doc_model::LineAlignment::Inset {
                            f32::from(width) / 2.0
                        } else {
                            0.0
                        };
                        let subpaths: Vec<(Vec<Point<Pixels>>, bool)> = subpaths
                            .iter()
                            .map(|(p, c)| {
                                let pts = if *c && inset_offset > 0.0 {
                                    offset_polyline(p, true, inset_offset)
                                } else {
                                    p.clone()
                                };
                                (if on_pixels || *c { stroke_adjusted(&pts, *c, f32::from(width), window.scale_factor()) } else { pts }, *c)
                            })
                            .collect();
                        // The line stops inside a filled head, so its end
                        // does not poke out past the head's point (as a
                        // straight line's, `line_element`). A preset path that
                        // contains `<close/>` (such as the accent bar of
                        // `accentCallout1..3` / `accentBorderCallout1..3`) is
                        // treated by PowerPoint as closed for line ends and
                        // receives no arrowheads.
                        let tips = subpaths.clone();
                        let mut subpaths = subpaths;
                        if !path_has_close && (stroke.head != LineEnd::None || stroke.tail != LineEnd::None) {
                            let head_base = END_BASE_POINTS * EMU_PER_POINT as f32 * scale;
                            if let Some((points, _)) = subpaths.iter_mut().find(|(p, closed)| !*closed && p.len() >= 2) {
                                let n = points.len();
                                for (i, j, end) in [(0, 1, stroke.head), (n - 1, n - 2, stroke.tail)] {
                                    let back = end_setback(end, f32::from(width), head_base);
                                    let (dx, dy) = (f32::from(points[j].x - points[i].x), f32::from(points[j].y - points[i].y));
                                    let len = (dx * dx + dy * dy).sqrt();
                                    if back > 0.0 && len > back {
                                        points[i] = point(points[i].x + px(dx / len * back), points[i].y + px(dy / len * back));
                                    }
                                }
                            }
                        }
                        if stroke.compound != ibis_doc_model::LineCompound::Single && !stroke.dashed {
                            for (points, closed) in &subpaths {
                                for (line, w) in compound_lines(points, *closed, f32::from(width), stroke.compound) {
                                    if let Some(p) = polygon_styled(&line, Some(px(w)), *closed, stroke.cap, stroke.join) {
                                        window.paint_path(p, hsla(stroke.color));
                                    }
                                }
                                if !*closed && stroke.cap == ibis_doc_model::LineCap::Round && f32::from(width) >= 1.5 {
                                    paint_compound_round_caps(window, &[points.clone()], f32::from(width), stroke.compound, hsla(stroke.color), true, true);
                                }
                            }
                        } else if stroke.dashed {
                            for (points, closed) in &subpaths {
                                let pieces = dash_pieces(points, *closed, stroke, f32::from(width));
                                let square = stroke.cap == ibis_doc_model::LineCap::Square;
                                for piece in &pieces {
                                    for (line, w) in compound_lines(piece, false, f32::from(width), stroke.compound) {
                                        let line = if square && line.len() >= 2 {
                                            square_capped(&line, w.min(1.0), true, true)
                                        } else {
                                            line
                                        };
                                        if let Some(p) = polygon_styled(&line, Some(px(w)), false, ibis_doc_model::LineCap::Flat, stroke.join) {
                                            window.paint_path(p, hsla(stroke.color));
                                        }
                                    }
                                }
                                if stroke.cap == ibis_doc_model::LineCap::Round {
                                    let w_px = f32::from(width);
                                    if stroke.compound != ibis_doc_model::LineCompound::Single && w_px >= 1.5 {
                                        let is_dot = |p: &[Point<Pixels>]| match (p.first(), p.last()) {
                                            (Some(f), Some(l)) => (f32::from(l.x - f.x)).hypot(f32::from(l.y - f.y)) <= 1e-4,
                                            _ => false,
                                        };
                                        let dot_pieces: Vec<Vec<Point<Pixels>>> = pieces.iter().filter(|p| is_dot(p)).cloned().collect();
                                        if !dot_pieces.is_empty() {
                                            paint_round_caps(window, &dot_pieces, w_px, hsla_mode(stroke.color, color_mode));
                                        }
                                        paint_compound_round_caps(window, &pieces, w_px, stroke.compound, hsla_mode(stroke.color, color_mode), true, true);
                                    } else {
                                        paint_round_caps(window, &pieces, w_px, hsla_mode(stroke.color, color_mode));
                                    }
                                }
                            }
                        } else if let Some(p) = compound_path_styled(&subpaths, Some(width), stroke.cap, stroke.join) {
                            let mut painted_gradient = false;
                            if let Some(
                                ibis_doc_model::Fill::Linear { angle, scaled, stops }
                                | ibis_doc_model::Fill::TileLinear { angle, scaled, stops, .. },
                            ) = &stroke.fill
                            {
                                if !stops.is_empty() {
                                    let all_pts: Vec<(f32, f32)> = subpaths
                                        .iter()
                                        .flat_map(|(pts, _)| pts.iter().map(|pt| (f32::from(pt.x), f32::from(pt.y))))
                                        .collect();
                                    if all_pts.len() >= 2 {
                                        let min_x = all_pts.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
                                        let max_x = all_pts.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
                                        let min_y = all_pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
                                        let max_y = all_pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
                                        let bw = (max_x - min_x).max(1.0);
                                        let bh = (max_y - min_y).max(1.0);
                                        let rad = (*angle as f32 / 60_000.0).to_radians();
                                        let (dx, dy) = if *scaled {
                                            let (s, c) = rad.sin_cos();
                                            let (vx, vy) = (c * bw, s * bh);
                                            let len = vx.hypot(vy).max(1e-6);
                                            (vx / len, vy / len)
                                        } else {
                                            (rad.cos(), rad.sin())
                                        };
                                        let corners = [(min_x, min_y), (max_x, min_y), (max_x, max_y), (min_x, max_y)];
                                        let t0 = corners.iter().map(|&(x, y)| x * dx + y * dy).fold(f32::INFINITY, f32::min);
                                        let t1 = corners.iter().map(|&(x, y)| x * dx + y * dy).fold(f32::NEG_INFINITY, f32::max);
                                        let span = (t1 - t0).max(1e-6);
                                        for (pts, closed) in &subpaths {
                                            if pts.len() >= 2 {
                                                let edges: Vec<(Point<Pixels>, Point<Pixels>)> = if *closed && pts.len() >= 3 {
                                                    let mut e: Vec<_> = pts.windows(2).map(|w| (w[0], w[1])).collect();
                                                    if let (Some(&first), Some(&last)) = (pts.first(), pts.last()) {
                                                        if (f32::from(last.x - first.x)).hypot(f32::from(last.y - first.y)) > 1e-3 {
                                                            e.push((last, first));
                                                        }
                                                    }
                                                    e
                                                } else {
                                                    pts.windows(2).map(|w| (w[0], w[1])).collect()
                                                };
                                                for (p0, p1) in edges {
                                                    let (x0, y0) = (f32::from(p0.x), f32::from(p0.y));
                                                    let (x1, y1) = (f32::from(p1.x), f32::from(p1.y));
                                                    let seg_len = (x1 - x0).hypot(y1 - y0);
                                                    let steps = ((seg_len / 4.0).ceil() as usize).clamp(1, 64);
                                                    for step in 0..steps {
                                                        let u0 = step as f32 / steps as f32;
                                                        let u1 = (step + 1) as f32 / steps as f32;
                                                        let um = (u0 + u1) * 0.5;
                                                        let (mx, my) = (x0 + (x1 - x0) * um, y0 + (y1 - y0) * um);
                                                        let t = ((mx * dx + my * dy) - t0) / span;
                                                        let seg_color = crate::render::gradient::color_at_mode(stops, t, false);
                                                        let q0 = point(px(x0 + (x1 - x0) * u0), px(y0 + (y1 - y0) * u0));
                                                        let q1 = point(px(x0 + (x1 - x0) * u1), px(y0 + (y1 - y0) * u1));
                                                        let cap = if steps == 1 { stroke.cap } else { LineCap::Flat };
                                                        if let Some(seg_p) = polygon_styled(&[q0, q1], Some(width), false, cap, stroke.join) {
                                                            window.paint_path(seg_p, seg_color);
                                                        }
                                                    }
                                                }
                                                painted_gradient = true;
                                            }
                                        }
                                    }
                                }
                            } else if let Some(ibis_doc_model::Fill::Path { stops, .. }) = &stroke.fill {
                                if !stops.is_empty() {
                                    for (pts, closed) in &subpaths {
                                        if pts.len() >= 2 {
                                            let edges: Vec<(Point<Pixels>, Point<Pixels>)> = if *closed && pts.len() >= 3 {
                                                let mut e: Vec<_> = pts.windows(2).map(|w| (w[0], w[1])).collect();
                                                if let (Some(&first), Some(&last)) = (pts.first(), pts.last()) {
                                                    if (f32::from(last.x - first.x)).hypot(f32::from(last.y - first.y)) > 1e-3 {
                                                        e.push((last, first));
                                                    }
                                                }
                                                e
                                            } else {
                                                pts.windows(2).map(|w| (w[0], w[1])).collect()
                                            };
                                            let total_len: f32 = edges.iter().map(|(p0, p1)| (f32::from(p1.x - p0.x)).hypot(f32::from(p1.y - p0.y))).sum();
                                            if total_len > 0.0 {
                                                let mut cum_len = 0.0f32;
                                                for (p0, p1) in edges {
                                                    let seg_len = (f32::from(p1.x - p0.x)).hypot(f32::from(p1.y - p0.y));
                                                    let t = (cum_len + seg_len * 0.5) / total_len;
                                                    let seg_color = crate::render::gradient::color_at_mode(stops, t, false);
                                                    if let Some(seg_p) = polygon_styled(&[p0, p1], Some(width), false, stroke.cap, stroke.join) {
                                                        window.paint_path(seg_p, seg_color);
                                                    }
                                                    cum_len += seg_len;
                                                }
                                                painted_gradient = true;
                                            }
                                        }
                                    }
                                }
                            }
                            if !painted_gradient {
                                window.paint_path(p, hsla_mode(stroke.color, color_mode));
                            }
                        }
                        // Arrowheads at the ends of an open outline (bent
                        // and curved connectors, arcs).
                        if !path_has_close && (stroke.head != LineEnd::None || stroke.tail != LineEnd::None) {
                            let head_base = END_BASE_POINTS * EMU_PER_POINT as f32 * scale;
                            if let Some((points, _)) = tips.iter().find(|(p, closed)| !*closed && p.len() >= 2) {
                                // The direction from a point a little way along, for a curve.
                                let toward = |tip: Point<Pixels>, rest: &mut dyn Iterator<Item = &Point<Pixels>>| {
                                    let mut last = tip;
                                    for &p in rest {
                                        last = p;
                                        if (f32::from(p.x - tip.x)).hypot(f32::from(p.y - tip.y)) >= f32::from(width) * 2.0 {
                                            break;
                                        }
                                    }
                                    last
                                };
                                let (first, last) = (points[0], points[points.len() - 1]);
                                let from_first = toward(first, &mut points.iter().skip(1));
                                let from_last = toward(last, &mut points.iter().rev().skip(1));
                                let color = hsla_mode(stroke.color, color_mode);
                                paint_line_end(stroke.head, first, from_first, f32::from(width), head_base, color, window);
                                paint_line_end(stroke.tail, last, from_last, f32::from(width), head_base, color, window);
                            }
                        }
                    }
                }
            },
        )
        .absolute()
        .size_full(),
    )
}

/// `points` (a polyline stroked `width` wide) with its level and upright
/// segments moved so that the stroke's edges fall on device pixels, as
/// PowerPoint's outlines are drawn (crisp, a pixel off where they are
/// meant to be at most); slanting ones are left where they are.
fn stroke_adjusted(points: &[Point<Pixels>], closed: bool, width: f32, scale_factor: f32) -> Vec<Point<Pixels>> {
    let mut out = points.to_vec();
    let n = points.len();
    if n < 2 {
        return out;
    }
    let half = width / 2.0;
    let snap = |v: f32| (((v - half) * scale_factor).round() / scale_factor + ((v + half) * scale_factor).round() / scale_factor) / 2.0;
    let segments = if closed { n } else { n - 1 };
    let level = |i: usize| {
        let (a, b) = (points[i % n], points[(i + 1) % n]);
        (f32::from(a.y - b.y)).abs() < 0.01 || (f32::from(a.x - b.x)).abs() < 0.01
    };
    for i in 0..segments {
        let j = (i + 1) % n;
        let (a, b) = (points[i], points[j]);
        // Only between level or upright segments (or the line's ends): a
        // slanting one would move with the corner.
        let before = if i == 0 && !closed { true } else { level((i + segments - 1) % segments) };
        let after = if j == n - 1 && !closed { true } else { level((i + 1) % segments) };
        if !(before && after) {
            continue;
        }
        if (f32::from(a.y - b.y)).abs() < 0.01 && (f32::from(a.x - b.x)).abs() >= 0.01 {
            let y = px(snap(f32::from(a.y)));
            out[i].y = y;
            out[j].y = y;
        } else if (f32::from(a.x - b.x)).abs() < 0.01 && (f32::from(a.y - b.y)).abs() >= 0.01 {
            let x = px(snap(f32::from(a.x)));
            out[i].x = x;
            out[j].x = x;
        }
    }
    out
}

/// Where an element meant to be at `at` (pixels) is drawn exactly, from
/// its `origin` as GPUI placed it: GPUI snaps authored positions to device
/// pixels (halves toward zero), which moves it by up to half a pixel.
pub(crate) fn exact_origin(origin: Point<Pixels>, at: (f32, f32), scale_factor: f32) -> Point<Pixels> {
    let snap = |v: f32| {
        let d = v * scale_factor;
        (if (d - d.trunc()).abs() == 0.5 { d.trunc() } else { d.round() }) / scale_factor
    };
    point(origin.x + px(at.0 - snap(at.0)), origin.y + px(at.1 - snap(at.1)))
}

/// The fill of a path with its `fill` mode: none, as is, or lightened or
/// darkened (the shading of 3-D looking presets such as `can` and `cube`).
fn shaded_background(fill: &Fill, mode: ibis_doc_model::PathFill) -> Option<Background> {
    use ibis_doc_model::PathFill;
    let factor = match mode {
        PathFill::None => return None,
        PathFill::Normal => return background(fill),
        PathFill::Lighten => 1.4,
        PathFill::LightenLess => 1.2,
        PathFill::Darken => 0.6,
        PathFill::DarkenLess => 0.8,
    };
    match fill {
        Fill::Solid(c) => {
            let mut color = hsla(*c);
            color.l = (color.l * factor).clamp(0.0, 1.0);
            Some(color.into())
        }
        other => background(other),
    }
}

/// A path of several subpaths, filled together (so inner subpaths make
/// holes) or stroked with `stroke` width.
pub(crate) fn compound_path_styled(
    subpaths: &[(Vec<Point<Pixels>>, bool)],
    stroke: Option<Pixels>,
    cap: LineCap,
    join: LineJoin,
) -> Option<Path<Pixels>> {
    let mut builder = match stroke {
        Some(width) => {
            let lyon_cap = match cap {
                LineCap::Round => lyon_tessellation::LineCap::Round,
                LineCap::Square => lyon_tessellation::LineCap::Square,
                LineCap::Flat => lyon_tessellation::LineCap::Butt,
            };
            let lyon_join = match join {
                LineJoin::Round => lyon_tessellation::LineJoin::Round,
                LineJoin::Bevel => lyon_tessellation::LineJoin::Bevel,
                LineJoin::Miter => lyon_tessellation::LineJoin::Miter,
            };
            let opts = lyon_tessellation::StrokeOptions::default()
                .with_line_width(f32::from(width))
                .with_line_cap(lyon_cap)
                .with_line_join(lyon_join);
            PathBuilder::default().with_style(gpui_kit::PathStyle::Stroke(opts))
        }
        None => PathBuilder::fill(),
    };
    let mut any = false;
    for (points, closed) in subpaths {
        let Some((first, rest)) = points.split_first() else { continue };
        builder.move_to(*first);
        for p in rest {
            builder.line_to(*p);
        }
        // Fills close every subpath; strokes only the closed ones.
        if *closed || stroke.is_none() {
            builder.close();
        }
        any = true;
    }
    if any { builder.build().ok() } else { None }
}

pub(crate) fn compound_path(subpaths: &[(Vec<Point<Pixels>>, bool)], stroke: Option<Pixels>) -> Option<Path<Pixels>> {
    compound_path_styled(subpaths, stroke, LineCap::Flat, LineJoin::Miter)
}

/// Rotation and flips of a shape, applied to points in its frame.
#[derive(Clone, Copy)]
struct Placement {
    width: f32,
    height: f32,
    flip_h: bool,
    flip_v: bool,
    sin: f32,
    cos: f32,
}

impl Placement {
    fn new(shape: &Shape, frame: EmuRect) -> Self {
        let angle = shape.rotation as f32 / 60_000.0 * PI / 180.0;
        Self {
            width: frame.width as f32,
            height: frame.height as f32,
            flip_h: shape.flip_h,
            flip_v: shape.flip_v,
            sin: angle.sin(),
            cos: angle.cos(),
        }
    }

    /// Frame-relative EMU points to window pixels, the frame at `origin`.
    fn map(&self, points: &[(f32, f32)], origin: Point<Pixels>, scale: f32) -> Vec<Point<Pixels>> {
        let (cx, cy) = (self.width / 2.0, self.height / 2.0);
        points
            .iter()
            .map(|&(x, y)| {
                let x = if self.flip_h { self.width - x } else { x } - cx;
                let y = if self.flip_v { self.height - y } else { y } - cy;
                let (x, y) = (x * self.cos - y * self.sin + cx, x * self.sin + y * self.cos + cy);
                point(origin.x + px(x * scale), origin.y + px(y * scale))
            })
            .collect()
    }
}

/// `points` moved `d` pixels to the right of their direction (a negative
/// `d` to the left), each corner on the meeting of its two sides' moved
/// lines (not further than four times `d` from it).
fn offset_polyline(points: &[Point<Pixels>], closed: bool, d: f32) -> Vec<Point<Pixels>> {
    let n = points.len();
    if n < 2 || d == 0.0 {
        return points.to_vec();
    }
    let normal = |i: usize, j: usize| {
        let (a, b) = (points[i], points[j]);
        let (dx, dy) = (f32::from(b.x - a.x), f32::from(b.y - a.y));
        let len = dx.hypot(dy);
        if len == 0.0 { None } else { Some((-dy / len, dx / len)) }
    };
    (0..n)
        .map(|i| {
            let before = if i > 0 { normal(i - 1, i) } else if closed { normal(n - 1, 0) } else { None };
            let after = if i + 1 < n { normal(i, i + 1) } else if closed { normal(n - 1, 0) } else { None };
            let (nx, ny) = match (before, after) {
                (Some(a), Some(b)) => {
                    let (sx, sy) = (a.0 + b.0, a.1 + b.1);
                    let len = sx.hypot(sy);
                    if len < 1e-3 {
                        a
                    } else {
                        let (mx, my) = (sx / len, sy / len);
                        let k = (1.0 / (mx * a.0 + my * a.1)).min(4.0);
                        (mx * k, my * k)
                    }
                }
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => (0.0, 0.0),
            };
            point(points[i].x + px(nx * d), points[i].y + px(ny * d))
        })
        .collect()
}

/// The lines a compound stroke `width` wide is drawn as, along `points`:
/// each moved across and its width.
fn compound_lines(points: &[Point<Pixels>], closed: bool, width: f32, compound: ibis_doc_model::LineCompound) -> Vec<(Vec<Point<Pixels>>, f32)> {
    if compound == ibis_doc_model::LineCompound::Single || width < 1.5 {
        return vec![(points.to_vec(), width)];
    }
    compound
        .bands()
        .iter()
        .map(|&(a, b)| (offset_polyline(points, closed, ((a + b) / 2.0 - 0.5) * width), ((b - a) * width).max(0.5)))
        .collect()
}

pub(crate) fn polygon_styled(
    points: &[Point<Pixels>],
    stroke: Option<Pixels>,
    closed: bool,
    cap: LineCap,
    join: LineJoin,
) -> Option<Path<Pixels>> {
    let (first, rest) = points.split_first()?;
    let mut builder = match stroke {
        Some(width) => {
            let lyon_cap = match cap {
                LineCap::Round => lyon_tessellation::LineCap::Round,
                LineCap::Square => lyon_tessellation::LineCap::Square,
                LineCap::Flat => lyon_tessellation::LineCap::Butt,
            };
            let lyon_join = match join {
                LineJoin::Round => lyon_tessellation::LineJoin::Round,
                LineJoin::Bevel => lyon_tessellation::LineJoin::Bevel,
                LineJoin::Miter => lyon_tessellation::LineJoin::Miter,
            };
            let opts = lyon_tessellation::StrokeOptions::default()
                .with_line_width(f32::from(width))
                .with_line_cap(lyon_cap)
                .with_line_join(lyon_join);
            PathBuilder::default().with_style(gpui_kit::PathStyle::Stroke(opts))
        }
        None => PathBuilder::fill(),
    };
    builder.move_to(*first);
    for p in rest {
        builder.line_to(*p);
    }
    if closed {
        builder.close();
    }
    builder.build().ok()
}

/// A line along `points` stroked `width` wide with butt ends and dashed as `pattern` (the lengths, in pixels, of the
/// dashes and the gaps in turn) from `phase` into it, as one path: dashes on the screen, and in a PDF one line with
/// its dash pattern (`d`), however many dashes there are.
pub(crate) fn dashed_line_path(points: &[Point<Pixels>], width: Pixels, pattern: &[f32], phase: f32) -> Option<Path<Pixels>> {
    let (first, rest) = points.split_first()?;
    if rest.is_empty() || pattern.is_empty() || pattern.iter().sum::<f32>() <= 0.0 {
        return None;
    }
    let opts = lyon_tessellation::StrokeOptions::default().with_line_width(f32::from(width)).with_line_cap(lyon_tessellation::LineCap::Butt);
    let lengths: Vec<Pixels> = pattern.iter().map(|l| px(*l)).collect();
    let mut builder = PathBuilder::default().with_style(gpui_kit::PathStyle::Stroke(opts)).dash_array(&lengths).dash_offset(px(phase));
    builder.move_to(*first);
    for p in rest {
        builder.line_to(*p);
    }
    builder.build().ok()
}

pub(crate) fn polygon(points: &[Point<Pixels>], stroke: Option<Pixels>, closed: bool) -> Option<Path<Pixels>> {
    polygon_styled(points, stroke, closed, LineCap::Flat, LineJoin::Miter)
}

/// Outlines of a geometry for `frame`, in frame-relative EMU.
fn geometry_outlines(geometry: &Geometry, frame: EmuRect) -> Vec<ibis_doc_model::Outline> {
    use ibis_doc_model::{Outline, PathFill, Subpath};
    let (w, h) = (frame.width as f64, frame.height as f64);
    let shape = |points: Vec<(f64, f64)>| vec![Outline { subpaths: vec![Subpath { points, closed: true }], fill: PathFill::Normal, stroke: true }];
    let ellipse = |cx: f64, cy: f64, rx: f64, ry: f64, start: f64, steps: usize| {
        (0..=steps).map(move |i| {
            let t = start + std::f64::consts::FRAC_PI_2 * i as f64 / steps as f64;
            (cx + rx * t.cos(), cy + ry * t.sin())
        })
    };
    match geometry {
        Geometry::Rect => shape(vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]),
        Geometry::Ellipse => shape((0..64).map(|i| {
            let t = std::f64::consts::TAU * i as f64 / 64.0;
            (w / 2.0 + w / 2.0 * t.cos(), h / 2.0 + h / 2.0 * t.sin())
        }).collect()),
        Geometry::RoundRect { radius } => {
            let r = w.min(h) * *radius as f64 / 100_000.0;
            let mut points = Vec::new();
            for (cx, cy, start) in [(w - r, r, -std::f64::consts::FRAC_PI_2), (w - r, h - r, 0.0), (r, h - r, std::f64::consts::FRAC_PI_2), (r, r, std::f64::consts::PI)] {
                // 8 steps a corner, as before cce4268e: with 32, Word's
                // rounded rectangles' straight edges came out on whole
                // pixels, where Word's PDF has them between (DSHP-00076).
                points.extend(ellipse(cx, cy, r, r, start, 8));
            }
            shape(points)
        }
        Geometry::Formula(g) => g.outline(w, h),
    }
}

/// Outlines of a geometry expanded by `stroke_half` for shadow rendering, in frame-relative EMU.
fn shadow_outlines(geometry: &Geometry, frame: EmuRect, stroke_half: f64) -> Vec<ibis_doc_model::Outline> {
    use ibis_doc_model::{Outline, PathFill, Subpath};
    let s = stroke_half;
    let (w, h) = (frame.width as f64, frame.height as f64);
    let shape = |points: Vec<(f64, f64)>| vec![Outline { subpaths: vec![Subpath { points, closed: true }], fill: PathFill::Normal, stroke: true }];
    let ellipse = |cx: f64, cy: f64, rx: f64, ry: f64, start: f64, steps: usize| {
        (0..=steps).map(move |i| {
            let t = start + std::f64::consts::FRAC_PI_2 * i as f64 / steps as f64;
            (cx + rx * t.cos(), cy + ry * t.sin())
        })
    };
    match geometry {
        Geometry::Rect => shape(vec![(-s, -s), (w + s, -s), (w + s, h + s), (-s, h + s)]),
        Geometry::Ellipse => shape((0..64).map(|i| {
            let t = std::f64::consts::TAU * i as f64 / 64.0;
            let rx = w / 2.0 + s;
            let ry = h / 2.0 + s;
            (w / 2.0 + rx * t.cos(), h / 2.0 + ry * t.sin())
        }).collect()),
        Geometry::RoundRect { radius } => {
            let r = w.min(h) * *radius as f64 / 100_000.0;
            let r_ext = r + s;
            let mut points = Vec::new();
            for (cx, cy, start) in [(w - r, r, -std::f64::consts::FRAC_PI_2), (w - r, h - r, 0.0), (r, h - r, std::f64::consts::FRAC_PI_2), (r, r, std::f64::consts::PI)] {
                points.extend(ellipse(cx, cy, r_ext, r_ext, start, 32));
            }
            shape(points)
        }
        Geometry::Formula(g) => g.outline(w, h),
    }
}

fn line_element(shape: &Shape, stroke: Stroke, frame: EmuRect, scale: f32, mode: ColorMode) -> Div {
    let place = Placement::new(shape, frame);
    let (w, h) = (frame.width as f32, frame.height as f32);

    // Horizontal and vertical lines have an empty frame, and GPUI skips
    // painting empty bounds, so the element extends past the frame on all
    // sides (by enough for the arrowheads).
    let head_base = END_BASE_POINTS * EMU_PER_POINT as f32 * scale;
    let margin = f32::from(stroke_px(&stroke, scale)).max(head_base) * 6.0 + 1.0;
    div()
        .absolute()
        .left(px(frame.x as f32 * scale - margin))
        .top(px(frame.y as f32 * scale - margin))
        .w(px(w * scale + margin * 2.0))
        .h(px(h * scale + margin * 2.0))
        .child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                // Where the frame is meant to be (not where GPUI snapped
                // the element to): the line is drawn there, as PowerPoint
                // draws it, smoothed across the pixels it crosses.
                let corner = exact_origin(bounds.origin, (frame.x as f32 * scale - margin, frame.y as f32 * scale - margin), window.scale_factor());
                let origin = point(corner.x + px(margin), corner.y + px(margin));
                let mut ends = place.map(&[(0.0, 0.0), (w, h)], origin, scale);
                let width = painted_stroke_px(&stroke, scale, window);
                // The line stops inside a filled head, so its square end
                // does not poke out past the head's point.
                let (tip0, tip1) = (ends[0], ends[1]);
                for (i, end, toward) in [(0, stroke.head, tip1), (1, stroke.tail, tip0)] {
                    let back = end_setback(end, f32::from(width), head_base);
                    let (dx, dy) = (f32::from(toward.x - ends[i].x), f32::from(toward.y - ends[i].y));
                    let len = (dx * dx + dy * dy).sqrt();
                    if back > 0.0 && len > back * 2.0 {
                        ends[i] = point(ends[i].x + px(dx / len * back), ends[i].y + px(dy / len * back));
                    }
                }
                let color = hsla_mode(stroke.color, mode);
                let head_plain = stroke.head == LineEnd::None;
                let tail_plain = stroke.tail == LineEnd::None;
                // Dashes in the line's pattern; the ends in its caps.
                let pieces: Vec<Vec<Point<Pixels>>> = if stroke.dashed {
                    let mut pieces = dash_pieces(&[ends[0], ends[1]], false, &stroke, f32::from(width));
                    // A head that the line is drawn short of (`end_setback`:
                    // triangle, stealth and arrow, 8cc4cb7e) starts the
                    // dash pattern at the shortened line's start, so the
                    // round-capped dashes do not reach half a width back
                    // past it as they do on a plain start: PowerPoint's are
                    // there (PARW-00221, stealth: 2 px later). Diamond and
                    // oval heads do not shorten the line, and their dashes
                    // stay where they were (PARW-00242, 00260 measured).
                    if stroke.cap == ibis_doc_model::LineCap::Round && end_setback(stroke.head, f32::from(width), head_base) > 0.0 {
                        let (dx, dy) = (f32::from(ends[1].x - ends[0].x), f32::from(ends[1].y - ends[0].y));
                        let len = (dx * dx + dy * dy).sqrt();
                        if len > 0.0 {
                            let (ux, uy) = (dx / len * f32::from(width) / 2.0, dy / len * f32::from(width) / 2.0);
                            for piece in &mut pieces {
                                for p in piece.iter_mut() {
                                    *p = point(p.x + px(ux), p.y + px(uy));
                                }
                            }
                        }
                    }
                    if stroke.cap == ibis_doc_model::LineCap::Square && !pieces.is_empty() {
                        let (dx, dy) = (f32::from(tip1.x - tip0.x), f32::from(tip1.y - tip0.y));
                        let len = (dx * dx + dy * dy).sqrt();
                        if len > 0.0 {
                            let (ux, uy) = (dx / len * f32::from(width) / 2.0, dy / len * f32::from(width) / 2.0);
                            if head_plain && let Some(first) = pieces.first_mut().and_then(|p| p.first_mut()) {
                                *first = point(first.x - px(ux), first.y - px(uy));
                            }
                            if tail_plain && let Some(last) = pieces.last_mut().and_then(|p| p.last_mut()) {
                                *last = point(last.x + px(ux), last.y + px(uy));
                            }
                        }
                    }
                    pieces
                } else if stroke.cap == ibis_doc_model::LineCap::Square && (head_plain || tail_plain) {
                    vec![square_capped(&[ends[0], ends[1]], f32::from(width), head_plain, tail_plain)]
                } else {
                    vec![vec![ends[0], ends[1]]]
                };
                let grad_stops: Option<(i32, Vec<ibis_doc_model::GradientStop>)> = match &stroke.fill {
                    Some(Fill::Linear { angle, stops, .. } | Fill::TileLinear { angle, stops, .. })
                        if !stops.is_empty() && !stroke.dashed && stroke.compound == ibis_doc_model::LineCompound::Single =>
                    {
                        Some((*angle, stops.clone()))
                    }
                    _ => None,
                };
                let color_at = |pt: Point<Pixels>| -> Hsla {
                    let Some((angle, ref stops)) = grad_stops else { return color };
                    let rad = (angle as f32 / 60_000.0).to_radians();
                    let (gx, gy) = (rad.cos(), rad.sin());
                    let p0 = f32::from(tip0.x) * gx + f32::from(tip0.y) * gy;
                    let p1 = f32::from(tip1.x) * gx + f32::from(tip1.y) * gy;
                    let span = p1 - p0;
                    let t = if span.abs() > 1e-3 {
                        let p = f32::from(pt.x) * gx + f32::from(pt.y) * gy;
                        ((p - p0.min(p1)) / span.abs()).clamp(0.0, 1.0)
                    } else {
                        let (dx, dy) = (f32::from(tip1.x - tip0.x), f32::from(tip1.y - tip0.y));
                        let len2 = dx * dx + dy * dy;
                        if len2 > 1e-3 {
                            ((f32::from(pt.x - tip0.x) * dx + f32::from(pt.y - tip0.y) * dy) / len2).clamp(0.0, 1.0)
                        } else {
                            0.0
                        }
                    };
                    let _ = mode;
                    crate::render::gradient::color_at(stops, t)
                };
                if stroke.cap == ibis_doc_model::LineCap::Round {
                    let w_px = f32::from(width);
                    if stroke.dashed {
                        let is_dot = |p: &[Point<Pixels>]| match (p.first(), p.last()) {
                            (Some(f), Some(l)) => (f32::from(l.x - f.x)).hypot(f32::from(l.y - f.y)) <= 1e-4,
                            _ => false,
                        };
                        let dot_pieces: Vec<Vec<Point<Pixels>>> = pieces.iter().filter(|p| is_dot(p)).cloned().collect();
                        if stroke.compound == ibis_doc_model::LineCompound::Single || w_px < 1.5 {
                            paint_round_caps(window, &pieces, w_px, color);
                        } else if !dot_pieces.is_empty() {
                            paint_round_caps(window, &dot_pieces, w_px, color);
                        }
                    } else if stroke.compound == ibis_doc_model::LineCompound::Single || w_px < 1.5 {
                        let r = w_px / 2.0;
                        for (pt, plain) in [(ends[0], head_plain), (ends[1], tail_plain)] {
                            if plain {
                                let disc: Vec<Point<Pixels>> = (0..16)
                                    .map(|i| {
                                        let t = std::f32::consts::TAU * i as f32 / 16.0;
                                        point(pt.x + px(r * t.cos()), pt.y + px(r * t.sin()))
                                    })
                                    .collect();
                                if let Some(p) = polygon(&disc, None, true) {
                                    window.paint_path(p, color_at(pt));
                                }
                            }
                        }
                    }
                }
                if grad_stops.is_some() {
                    for piece in &pieces {
                        if piece.len() >= 2 {
                            let (p0, p1) = (piece[0], piece[piece.len() - 1]);
                            let n_seg = 32usize;
                            for k in 0..n_seg {
                                let f0 = k as f32 / n_seg as f32;
                                let f1 = (k + 1) as f32 / n_seg as f32;
                                let fm = (f0 + f1) * 0.5;
                                let lerp = |f: f32| point(p0.x + px(f32::from(p1.x - p0.x) * f), p0.y + px(f32::from(p1.y - p0.y) * f));
                                let seg = [lerp(f0), lerp(f1)];
                                if let Some(p) = polygon(&seg, Some(width), false) {
                                    window.paint_path(p, color_at(lerp(fm)));
                                }
                            }
                        }
                    }
                } else {
                    for piece in &pieces {
                        for (line, w) in compound_lines(piece, false, f32::from(width), stroke.compound) {
                            if let Some(p) = polygon(&line, Some(px(w)), false) {
                                window.paint_path(p, color);
                            }
                        }
                    }
                }
                if stroke.cap == ibis_doc_model::LineCap::Round
                    && stroke.compound != ibis_doc_model::LineCompound::Single
                    && f32::from(width) >= 1.5
                {
                    paint_compound_round_caps(window, &pieces, f32::from(width), stroke.compound, color, head_plain, tail_plain);
                }
                for (at, from, end) in [(tip1, tip0, stroke.tail), (tip0, tip1, stroke.head)] {
                    paint_line_end(end, at, from, f32::from(width), head_base, color_at(at), window);
                }
            },
        )
        .absolute()
        .size_full(),
    )
}

/// The narrowest line width, in points, that line ends are sized by: a
/// thinner line has the heads of a 2 pt one (PowerPoint's small head on a
/// 1 pt line is 4 pt wide and long, measured against its PDF).
const END_BASE_POINTS: f32 = 2.0;

/// The width and length, in pixels, of the decoration `end` of a line
/// `width` pixels wide: 2, 3 or 5 times the line's width (small, medium,
/// large), but at least those of a line `base` pixels wide.
fn line_end_size(end: LineEnd, width: f32, base: f32) -> Option<(f32, f32)> {
    let size = match end {
        LineEnd::None => return None,
        LineEnd::Arrow(s) => {
            let bw = width.max(base).max(1.0);
            let fw = [3.0, 3.0, 5.0][s.width.min(2) as usize] * bw;
            let fl = [2.5, 3.5, 5.0][s.length.min(2) as usize] * bw;
            return Some((fw, fl));
        }
        LineEnd::Triangle(s) | LineEnd::Stealth(s) | LineEnd::Diamond(s) | LineEnd::Oval(s) => s,
    };
    let factor = |v: u8| [2.0, 3.0, 5.0][v.min(2) as usize] * width.max(base).max(1.0);
    Some((factor(size.width), factor(size.length)))
}

/// How far a line stops short of its end so that it ends inside the end's
/// filled head (half the head's length); 0 for the other ends.
fn end_setback(end: LineEnd, width: f32, base: f32) -> f32 {
    match (end, line_end_size(end, width, base)) {
        (LineEnd::Triangle(_) | LineEnd::Stealth(_), Some((_, long))) => long / 2.0,
        (LineEnd::Arrow(_), _) => width,
        _ => 0.0,
    }
}

/// Paint the decoration `end` of a line `width` pixels wide that ends at
/// `at`, coming from `from`, sized as [`line_end_size`] says.
fn paint_line_end(end: LineEnd, at: Point<Pixels>, from: Point<Pixels>, width: f32, base: f32, color: Hsla, window: &mut Window) {
    let Some((wide, long)) = line_end_size(end, width, base) else { return };
    let half = wide / 2.0;
    let (dx, dy) = (f32::from(at.x - from.x), f32::from(at.y - from.y));
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / len, dy / len);
    // A point `back` behind the end along the line and `side` to its left.
    let p = |back: f32, side: f32| point(at.x - px(ux * back) - px(uy * side), at.y - px(uy * back) + px(ux * side));
    let shape: Vec<Point<Pixels>> = match end {
        LineEnd::Triangle(_) => vec![p(0.0, 0.0), p(long, half), p(long, -half)],
        LineEnd::Stealth(_) => vec![p(0.0, 0.0), p(long, half), p(long * 0.6, 0.0), p(long, -half)],
        LineEnd::Diamond(_) => vec![p(-long / 2.0, 0.0), p(0.0, half), p(long / 2.0, 0.0), p(0.0, -half)],
        LineEnd::Oval(_) => (0..24)
            .map(|i| (i as f32 / 24.0 * 2.0 * PI).sin_cos())
            .map(|(s, c)| p(c * long / 2.0, s * half))
            .collect(),
        LineEnd::Arrow(_) => {
            let stroke = px(width.max(1.0));
            for side in [half, -half] {
                if let Some(path) = polygon(&[p(long, side), p(0.0, 0.0)], Some(stroke), false) {
                    window.paint_path(path, color);
                }
            }
            return;
        }
        LineEnd::None => return,
    };
    if let Some(path) = polygon(&shape, None, true) {
        window.paint_path(path, color);
    }
}

fn picture_element(container: Div, shape: &Shape, picture: &Picture, frame: EmuRect, scale: f32, res: &Resources) -> Div {
    let (w, h) = (frame.width as f32 * scale, frame.height as f32 * scale);
    let radius = px(w.min(h) * picture.corner as f32 / 100_000.0);
    // The outline, over the picture.
    // Centred on the picture's edge, half of it outside, and dashed as the line is (as PowerPoint draws a picture's border).
    let at = (shape.frame.x as f32 * scale, shape.frame.y as f32 * scale);
    let outline = picture.stroke.as_ref().map(|stroke| outline_element(stroke, w, h, radius, scale, picture.corner == 0 && picture.geometry.is_none(), at, res.color_mode));

    // A video playing: its newest frame, in place of the poster, preserving corner, outline and shadow.
    #[cfg(any(target_os = "macos",all(windows,feature="export")))]
    if let Some(ref clip) = picture.clip
        && let Some(live) = res.live_frame(clip.media)
    {
        #[cfg(target_os="macos")]
        let live=gpui::surface(live).absolute().size_full().object_fit(ObjectFit::Fill).into_any_element();
        #[cfg(all(windows,feature="export"))]
        let live=img(ImageSource::Render(live)).absolute().size_full().object_fit(ObjectFit::Fill).into_any_element();
        let mut el = container
            .rounded(radius)
            .overflow_hidden()
            .child(live)
            .children(outline);
        if let Some(s) = picture.shadow {
            let (dx, dy, blur, color) = shadow_parts(s, scale);
            el = el.shadow(vec![BoxShadow { color, offset: point(px(dx), px(dy)), blur_radius: px(blur / 3.0), spread_radius: px(0.), inset: false }]);
        }
        return el;
    }
    let turned = shape.rotation.rem_euclid(21_600_000) != 0 || shape.flip_h || shape.flip_v;
    if turned && let Some((image, bounds)) = rotated_image(shape, picture, frame, false, res) {
        let geom = picture
            .geometry
            .clone()
            .or_else(|| (picture.corner > 0).then_some(Geometry::RoundRect { radius: picture.corner }))
            .unwrap_or(Geometry::Rect);
        let outlines = geometry_outlines(&geom, frame);
        let place_map = Placement::new(shape, frame);
        let stroke_half_emu = picture.stroke.as_ref().map(|st| st.width / 2).unwrap_or(0);
        let shadow_outlines = shadow_outlines(&geom, frame, stroke_half_emu as f64);
        let at_px = (frame.x as f32 * scale, frame.y as f32 * scale);
        let on_pixels = !res.exact;
        let color_mode = res.color_mode;
        let mut cont = container;
        if let Some(s) = picture.shadow {
            let shadow_outlines = shadow_outlines.clone();
            cont = cont.child(
                canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        let exact = exact_origin(b.origin, at_px, window.scale_factor());
                        if s.has_transform() {
                            let (w_emu, h_emu) = (frame.width as f64, frame.height as f64);
                            let anchor = shadow_anchor(s.align, w_emu, h_emu, stroke_half_emu as f64);
                            let blur = s.blur as f32 * scale;
                            let color = hsla_mode(s.color, color_mode);
                            render_transformed_shadow(window, &shadow_outlines, anchor, &s, blur, color, exact, scale);
                        } else {
                            let (dx, dy, blur, color) = shadow_parts(s, scale);
                            let spots: Vec<(f32, f32)> = if blur < 0.5 {
                                vec![(0.0, 0.0)]
                            } else {
                                (0..8)
                                    .map(|i| (i as f32 * std::f32::consts::FRAC_PI_4).sin_cos())
                                    .map(|(s, c)| (c * blur / 2.0, s * blur / 2.0))
                                    .chain([(0.0, 0.0)])
                                    .collect()
                            };
                            let faint = Hsla { a: color.a / spots.len() as f32 * 1.6, ..color };
                            for (sx, sy) in &spots {
                                let origin = point(exact.x + px(dx + sx), exact.y + px(dy + sy));
                                for outline in shadow_outlines.iter().filter(|o| o.fill != ibis_doc_model::PathFill::None) {
                                    let subpaths: Vec<(Vec<Point<Pixels>>, bool)> = outline
                                        .subpaths
                                        .iter()
                                        .map(|sub| {
                                            let points: Vec<(f32, f32)> = sub.points.iter().map(|&(x, y)| (x as f32, y as f32)).collect();
                                            (place_map.map(&points, origin, scale), sub.closed)
                                        })
                                        .collect();
                                    if let Some(p) = compound_path(&subpaths, None) {
                                        window.paint_path(p, faint);
                                    }
                                }
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            );
        }
        if let Some(bg_color) = picture.effects.background {
            let fill_outlines = outlines.clone();
            let fill_map = place_map;
            cont = cont.child(
                canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        let exact = exact_origin(b.origin, at_px, window.scale_factor());
                        for outline in fill_outlines.iter().filter(|o| o.fill != ibis_doc_model::PathFill::None) {
                            let subpaths: Vec<(Vec<Point<Pixels>>, bool)> = outline
                                .subpaths
                                .iter()
                                .map(|sub| {
                                    let points: Vec<(f32, f32)> = sub.points.iter().map(|&(x, y)| (x as f32, y as f32)).collect();
                                    (fill_map.map(&points, exact, scale), sub.closed)
                                })
                                .collect();
                            if let Some(p) = compound_path(&subpaths, None) {
                                window.paint_path(p, hsla_mode(bg_color, color_mode));
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            );
        }
        // The rotated picture covers its bounding box, around the frame.
        cont = cont.child(
            place(img(res.svg_picture(image)), bounds, scale).object_fit(ObjectFit::Fill),
        );
        if let Some(stroke) = picture.stroke.clone() {
            cont = cont.child(
                canvas(
                    |_, _, _| {},
                    move |b, _, window, _| {
                        let exact = exact_origin(b.origin, at_px, window.scale_factor());
                        let width = painted_stroke_px(&stroke, scale, window);
                        let inset_offset = if stroke.alignment == ibis_doc_model::LineAlignment::Inset {
                            f32::from(width) / 2.0
                        } else {
                            0.0
                        };
                        for outline in outlines.iter().filter(|o| o.stroke) {
                            let subpaths: Vec<(Vec<Point<Pixels>>, bool)> = outline
                                .subpaths
                                .iter()
                                .map(|sub| {
                                    let points: Vec<(f32, f32)> = sub.points.iter().map(|&(x, y)| (x as f32, y as f32)).collect();
                                    let mapped = place_map.map(&points, exact, scale);
                                    let pts = if sub.closed && inset_offset > 0.0 {
                                        offset_polyline(&mapped, true, inset_offset)
                                    } else {
                                        mapped
                                    };
                                    (if on_pixels || sub.closed { stroke_adjusted(&pts, sub.closed, f32::from(width), window.scale_factor()) } else { pts }, sub.closed)
                                })
                                .collect();
                            if stroke.compound != ibis_doc_model::LineCompound::Single && !stroke.dashed {
                                for (points, closed) in &subpaths {
                                    for (line, w) in compound_lines(points, *closed, f32::from(width), stroke.compound) {
                                        if let Some(p) = polygon_styled(&line, Some(px(w)), *closed, stroke.cap, stroke.join) {
                                            window.paint_path(p, hsla_mode(stroke.color, color_mode));
                                        }
                                    }
                                }
                            } else if stroke.dashed {
                                for (points, closed) in &subpaths {
                                    let pieces = dash_pieces(points, *closed, &stroke, f32::from(width));
                                    for piece in &pieces {
                                        for (line, w) in compound_lines(piece, false, f32::from(width), stroke.compound) {
                                            if let Some(p) = polygon_styled(&line, Some(px(w)), false, ibis_doc_model::LineCap::Flat, stroke.join) {
                                                window.paint_path(p, hsla_mode(stroke.color, color_mode));
                                            }
                                        }
                                    }
                                    if stroke.cap == ibis_doc_model::LineCap::Round {
                                        paint_round_caps(window, &pieces, f32::from(width), hsla_mode(stroke.color, color_mode));
                                    }
                                }
                            } else if let Some(p) = compound_path_styled(&subpaths, Some(width), stroke.cap, stroke.join) {
                                window.paint_path(p, hsla_mode(stroke.color, color_mode));
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            );
        }
        return cont;
    }
    if !res.has_picture(picture.media) {
        return container;
    }
    // Cut to a shape, or given colour effects, or upscaled beyond its source pixels (where GPUI's
    // unpadded texture atlas would bleed transparent edge texels under linear filtering): one SVG
    // rasterized with edge clamping.
    let upscaled = res.image_size(picture.media).is_some_and(|(pw, ph)| {
        let frac = |v: i32| v as f32 / 100_000.0;
        let vw = (1.0 - frac(picture.crop.left) - frac(picture.crop.right)).max(0.01);
        let vh = (1.0 - frac(picture.crop.top) - frac(picture.crop.bottom)).max(0.01);
        let draw_w = (w * 2.0).max(frame.width as f32 / 9_525.0);
        let draw_h = (h * 2.0).max(frame.height as f32 / 9_525.0);
        draw_w > pw as f32 * vw + 0.5 || draw_h > ph as f32 * vh + 0.5
    });
    let styled = (!picture.effects.is_none()
        || upscaled
        || picture.geometry.is_some()
        || picture.corner > 0
        || picture.luminance.is_some()
        || (picture.duotone.is_some() && picture.clear.is_some())
        || picture.tile.is_some()
        || picture.fill_rect.is_some()
        || picture.soft_edge.is_some()
        || picture.artistic_effect.is_some())
        .then(|| styled_picture_image(shape, picture, frame, res))
        .flatten();
    let recoloured = picture.duotone.filter(|_| styled.is_none()).and_then(|(dark, light)| {
        let key = (picture.media, dark, light);
        if let Some(found) = res.duotones.borrow().get(&key) {
            return Some(found.clone());
        }
        let (encoding, data) = res.source(picture.media)?;
        let rgb = |c: ibis_doc_model::Rgb| (c.r(), c.g(), c.b());
        let image = Arc::new(Image::from_bytes(ImageFormat::Svg, crate::render::picture_svg::duotone_picture(&data, encoding, rgb(dark), rgb(light))));
        res.duotones.borrow_mut().insert(key, image.clone());
        Some(image)
    });
    // A colour set transparent (not combined with a duotone).
    let cleared = picture.clear.filter(|_| recoloured.is_none() && styled.is_none()).and_then(|clear| {
        let key = (picture.media, clear, clear);
        if let Some(found) = res.duotones.borrow().get(&key) {
            return Some(found.clone());
        }
        let (encoding, data) = res.source(picture.media)?;
        let image = Arc::new(Image::from_bytes(ImageFormat::Svg, crate::render::picture_svg::clear_colour_picture(&data, encoding, (clear.r(), clear.g(), clear.b()))));
        res.duotones.borrow_mut().insert(key, image.clone());
        Some(image)
    });
    // Recoloured pictures are loaded by GPUI; the picture itself once decoded.
    let Some(image) = styled.clone().or(recoloured).or(cleared).map(|image| res.svg_picture(image)).or_else(|| res.picture_at_crop_size(picture.media, w, h, picture.crop)) else { return container };
    // (The crop is in the styled picture already.)
    let c = if styled.is_some() { ibis_doc_model::Crop::default() } else { picture.crop };
    let frac = |v: i32| v as f32 / 100_000.0;
    let visible_w = (1.0 - frac(c.left) - frac(c.right)).max(0.01);
    let visible_h = (1.0 - frac(c.top) - frac(c.bottom)).max(0.01);
    // A brightness overlay reflecting luminance.
    let lum_overlay = picture.luminance.filter(|_| styled.is_none()).and_then(|(bright, _)| {
        if bright > 0 {
            let alpha = ((bright as f32 / 100_000.0).clamp(0.0, 0.85) * 255.0) as u8;
            Some(div().absolute().size_full().bg(hsla(DocColor { rgb: ibis_doc_model::Rgb(0xffffff), alpha })))
        } else if bright < 0 {
            let alpha = (((-bright) as f32 / 100_000.0).clamp(0.0, 0.85) * 255.0) as u8;
            Some(div().absolute().size_full().bg(hsla(DocColor { rgb: ibis_doc_model::Rgb(0x000000), alpha })))
        } else {
            None
        }
    });

    let mut cont = container.rounded(radius);
    if let Some(s) = picture.shadow {
        let (dx, dy, blur, color) = shadow_parts(s, scale);
        // A picture that can be transparent casts a shadow of its shape (its opaque parts in the shadow's
        // colour), not of its frame; a JPEG has no transparent parts, so the frame's will do.
        let silhouette = res
            .source(picture.media)
            .filter(|(encoding, _)| !matches!(encoding, crate::render::picture_svg::Encoding::Jpeg))
            .and_then(|_| {
                let mut shade = Picture::new(picture.media);
                shade.crop = picture.crop;
                shade.geometry = picture.geometry.clone();
                shade.effects.replace = Some(s.color.rgb);
                shade.effects.opacity = Some((s.color.alpha as i32 * 100_000 / 255).clamp(0, 100_000));
                // The blur's deviation, in the picture's own pixels (the SVG's units).
                let k = res.image_size(picture.media).map_or(0.0, |(px_w, _)| px_w as f64 / frame.width.max(1) as f64);
                shade.soft_edge = (s.blur > 0).then(|| (s.blur as f64 / 3.0 * k * 12_700.0) as i64).filter(|v| *v > 0);
                styled_image(shape.flip_h, shape.flip_v, &shade, frame, res)
            });
        match silhouette {
            Some(image) => {
                cont = cont.child(div().absolute().left(px(dx)).top(px(dy)).w(px(w)).h(px(h)).child(img(res.svg_picture(image)).absolute().size_full().object_fit(ObjectFit::Fill)));
            }
            None => {
                cont = cont.shadow(vec![BoxShadow { color, offset: point(px(dx), px(dy)), blur_radius: px(blur / 3.0), spread_radius: px(0.), inset: false }]);
            }
        }
    }
    if let Some(r) = picture.reflection
        && r.sy == -100_000
        && (r.distance == 0 || r.direction == 5_400_000)
    {
        let gap = r.distance as f32 * scale;
        cont = cont.child(
            div()
                .absolute()
                .left(px(0.))
                .top(px(h + gap))
                .w(px(w))
                .h(px(h))
                .rounded(radius)
                .overflow_hidden()
                .opacity(0.35)
                .child(
                    img(image.clone())
                        .absolute()
                        .size_full()
                        .object_fit(ObjectFit::Fill),
                ),
        );
    }

    // The whole picture, sized and offset so the cropped part fills the frame.
    let whole = img(image.clone())
        .absolute()
        .left(relative(-frac(c.left) / visible_w))
        .top(relative(-frac(c.top) / visible_h))
        .w(relative(1.0 / visible_w))
        .h(relative(1.0 / visible_h))
        .object_fit(ObjectFit::Fill);
    // An animated GIF plays in the slide show: an image with an id keeps its
    // frame (GPUI advances it by the frames' delays).
    let animated = res.animate_pictures.get() && matches!(&image, ImageSource::Render(data) if data.frame_count() > 1);
    // The picture is cut to the frame; its outline, half of it outside, is not.
    let cut = div()
        .absolute()
        .size_full()
        .rounded(radius)
        .overflow_hidden()
        .when_some(picture.effects.background, |d, c| d.bg(hsla_mode(c, res.color_mode)));
    let cut = if animated { cut.child(whole.id(ElementId::Name(format!("gif-{}", shape.id.0).into()))) } else { cut.child(whole) };
    cont.child(cut.children(lum_overlay)).children(outline)
}

/// The SVG of `picture` cropped, cut to its geometry and given its colour effects, if its format can be wrapped.
fn styled_picture_image(shape: &Shape, picture: &Picture, frame: EmuRect, res: &Resources) -> Option<Arc<Image>> {
    styled_image(shape.flip_h, shape.flip_v, picture, frame, res)
}

/// A picture wrapped as SVG for a frame, with the effects it carries (see [`styled_picture_image`]).
fn styled_image(flip_h: bool, flip_v: bool, picture: &Picture, frame: EmuRect, res: &Resources) -> Option<Arc<Image>> {
    let (w, h) = (frame.width as f64, frame.height as f64);
    let geom = picture
        .geometry
        .clone()
        .or_else(|| (picture.corner > 0).then_some(Geometry::RoundRect { radius: picture.corner }));
    let subpaths: Option<Vec<Vec<(f64, f64)>>> = geom.as_ref().map(|g| {
        geometry_outlines(g, frame)
            .iter()
            .filter(|o| o.fill != ibis_doc_model::PathFill::None)
            .flat_map(|o| &o.subpaths)
            .map(|s| s.points.iter().map(|&(x, y)| (if flip_h { w - x } else { x }, if flip_v { h - y } else { y })).collect())
            .collect()
    });
    let key = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (frame.width, frame.height, picture.crop, picture.clear, picture.luminance, picture.duotone, picture.effects, picture.soft_edge).hash(&mut hasher);
        (flip_h, flip_v, picture.tile, picture.fill_rect, picture.artistic_effect).hash(&mut hasher);
        for p in subpaths.iter().flatten().flatten() {
            (p.0.to_bits(), p.1.to_bits()).hash(&mut hasher);
        }
        (picture.media, hasher.finish())
    };
    if let Some(found) = res.clipped.borrow().get(&key) {
        return Some(found.clone());
    }
    let (encoding, data) = res.source(picture.media)?;
    let rgb = |c: ibis_doc_model::Rgb| (c.r(), c.g(), c.b());
    let look = crate::render::picture_svg::Look { clear: picture.clear.map(rgb), luminance: picture.luminance, duotone: picture.duotone.map(|(a, b)| (rgb(a), rgb(b))), effects: picture.effects, soft_edge: picture.soft_edge, artistic_effect: picture.artistic_effect, fill_rect: None, upright: false };
    // Tiles: a tile is the picture's pixels at one point each, scaled (`a:tile/@sx`, `@sy`); the first sits at
    // the frame's corner `align` names, moved by `tx`, `ty`.
    let tile = picture.tile.and_then(|t| {
        let (px_w, px_h) = res.image_size(picture.media)?;
        let tw = ((px_w as f64 * ibis_doc_model::EMU_PER_POINT as f64) * (t.sx.max(1) as f64 / 100_000.0)).max(ibis_doc_model::EMU_PER_POINT as f64);
        let th = ((px_h as f64 * ibis_doc_model::EMU_PER_POINT as f64) * (t.sy.max(1) as f64 / 100_000.0)).max(ibis_doc_model::EMU_PER_POINT as f64);
        use ibis_doc_model::TileAlign as A;
        let (bx, by) = match t.align {
            A::TopLeft => (0.0, 0.0),
            A::Top => ((w - tw) / 2.0, 0.0),
            A::TopRight => (w - tw, 0.0),
            A::Left => (0.0, (h - th) / 2.0),
            A::Center => ((w - tw) / 2.0, (h - th) / 2.0),
            A::Right => (w - tw, (h - th) / 2.0),
            A::BottomLeft => (0.0, h - th),
            A::Bottom => ((w - tw) / 2.0, h - th),
            A::BottomRight => (w - tw, h - th),
        };
        let flip = t.flip;
        Some(crate::render::picture_svg::TileLayout { width: tw, height: th, x: bx + t.tx as f64, y: by + t.ty as f64, flip_x: matches!(flip, ibis_doc_model::TileFlip::X | ibis_doc_model::TileFlip::Xy), flip_y: matches!(flip, ibis_doc_model::TileFlip::Y | ibis_doc_model::TileFlip::Xy) })
    });
    let svg = crate::render::picture_svg::styled_picture(&data, encoding, frame.width, frame.height, subpaths.as_deref(), picture.crop, picture.fill_rect, tile, &look);
    let image = Arc::new(Image::from_bytes(ImageFormat::Svg, svg));
    res.clipped.borrow_mut().insert(key, image.clone());
    Some(image)
}

/// The picture of `shape` rotated and flipped, with its bounds relative to
/// the frame's top-left corner; `None` if its format cannot be wrapped.
fn rotated_image(shape: &Shape, picture: &Picture, frame: EmuRect, upright: bool, res: &Resources) -> Option<(Arc<Image>, EmuRect)> {
    let (w, h) = (frame.width as f64, frame.height as f64);
    let geom = picture
        .geometry
        .clone()
        .or_else(|| (picture.corner > 0).then_some(Geometry::RoundRect { radius: picture.corner }))
        .filter(|g| *g != Geometry::Rect);
    let subpaths: Option<Vec<Vec<(f64, f64)>>> = geom.as_ref().map(|g| {
        geometry_outlines(g, frame)
            .iter()
            .filter(|o| o.fill != ibis_doc_model::PathFill::None)
            .flat_map(|o| &o.subpaths)
            .map(|s| s.points.iter().map(|&(x, y)| (if shape.flip_h { w - x } else { x }, if shape.flip_v { h - y } else { y })).collect())
            .collect()
    });
    let key = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        (shape.rotation, shape.flip_h, shape.flip_v, picture.crop, frame.width, frame.height, picture.clear, picture.luminance, picture.duotone, picture.effects, picture.soft_edge, picture.artistic_effect).hash(&mut hasher);
        (upright, picture.fill_rect).hash(&mut hasher);
        for p in subpaths.iter().flatten().flatten() {
            (p.0.to_bits(), p.1.to_bits()).hash(&mut hasher);
        }
        (picture.media, hasher.finish())
    };
    if let Some(found) = res.rotated.borrow().get(&key) {
        return Some(found.clone());
    }
    let (encoding, data) = res.source(picture.media)?;
    let local = EmuRect { x: 0, y: 0, ..frame };
    let rgb = |c: ibis_doc_model::Rgb| (c.r(), c.g(), c.b());
    let look = crate::render::picture_svg::Look { clear: picture.clear.map(rgb), luminance: picture.luminance, duotone: picture.duotone.map(|(a, b)| (rgb(a), rgb(b))), effects: picture.effects, soft_edge: picture.soft_edge, artistic_effect: picture.artistic_effect, fill_rect: picture.fill_rect, upright };
    let wrapped = crate::render::picture_svg::rotated_picture(&data, encoding, local, shape.rotation, shape.flip_h, shape.flip_v, picture.crop, subpaths.as_deref(), &look);
    let entry = (Arc::new(Image::from_bytes(ImageFormat::Svg, wrapped.svg)), wrapped.bounds);
    res.rotated.borrow_mut().insert(key, entry.clone());
    Some(entry)
}

fn placeholder_element(container: Div, kind: &Placeholder, frame: EmuRect, scale: f32) -> Div {
    let label = match kind {
        Placeholder::Chart => "_render.chart-unsupported",
        Placeholder::Table => "_render.table-unsupported",
        Placeholder::Picture(MediaFormat::Emf) => "_render.emf-image-unsupported",
        Placeholder::Picture(MediaFormat::Wmf) => "_render.wmf-image-unsupported",
        Placeholder::Picture(_) => "_render.image-unsupported",
        Placeholder::Diagram => "",
    };
    let side = frame.width.min(frame.height) as f32 * scale;
    container
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .bg(rgba(0x8080800f))
        .border_1()
        .border_dashed()
        .border_color(rgba(0x80808080))
        .text_color(rgb(0x808080))
        .text_size(px((side * 0.12).clamp(4.0, 14.0)))
        .when(side > 24.0, |d| d.child(if label.is_empty() { SharedString::default() } else { crate::shell::i18n::text(label) }))
}

/// Text of a body laid out inside its frame at `scale` pixels per EMU;
/// `width` is the width of the frame's text area in EMU when known (the
/// lines break at it exactly, the same at every zoom). `positions`, when
/// given, gets where each paragraph's characters and lines are painted
/// (for a caret, a selection and clicks).
pub(crate) fn text_body_element(
    body: &TextBody,
    width: Option<f64>,
    // The height of the box the text is in (EMU), for text in columns.
    height: Option<f64>,
    scale: f32,
    res: &Resources,
    positions: Option<&crate::render::rich_text::TextPositions>,
    turn: Option<crate::render::rich_text::TextTurn>,
    origin_x: Option<f32>,
) -> Div {
    if body.vertical && matches!(body.vertical_kind, ibis_doc_model::VerticalKind::WordArt | ibis_doc_model::VerticalKind::WordArtRtl) {
        return vertical_text_element(body, scale, res);
    }
    let px_per_point = scale * EMU_PER_POINT as f32;
    let font_scale = body.font_scale as f32 / 100_000.0;
    let size_px = |style: &TextStyle| px(ibis_doc_model::autofit_font_size_pt(style.font_size_pt(), body.font_scale) * px_per_point);

    // The paragraphs are placed together, from their unrounded heights.
    let block = crate::render::rich_text::TextBlock::default();
    block.heights.borrow_mut().resize(body.paragraphs.len(), 0.0);
    // Text in columns runs down a column of the box's height, then on in the next (`TextBlock::flow`).
    if body.columns > 1 && !body.vertical && let (Some(w), Some(h)) = (width, height) {
        let cols = body.columns.clamp(1, 16) as usize;
        let total_w = (w as f32 - (body.insets.left + body.insets.right) as f32).max(0.0);
        let col_w = ((total_w - (cols as f32 - 1.0) * body.column_spacing as f32) / cols as f32).max(1.0);
        let column_h = (h as f32 - (body.insets.top + body.insets.bottom) as f32).max(1.0);
        block.flow.set(Some((column_h * scale, (col_w + body.column_spacing as f32) * scale, cols)));
    }
    // Text bent to a warp (`a:prstTxWarp`) that has two guide curves: its glyphs are collected and painted
    // bent after the paragraphs (the other warps are not drawn yet).
    if let Some(geometry) = body.warp.as_deref().and_then(|name| ibis_pptx::text_warp_geometry(name, &body.warp_adjust))
        && matches!(geometry.outline(100.0, 100.0).len(), 1 | 2 | 3 | 4 | 6)
    {
        *block.warp.borrow_mut() = Some(crate::render::rich_text::Warp::new(geometry));
    }
    let warped = block.clone();
    // (A text on one curve is a single line.)
    let single_path = block.warp.borrow().as_ref().is_some_and(|w| matches!(w.geometry.outline(100.0, 100.0).len(), 1 | 3));
    let slots = positions.map(|p| p.slots(body.paragraphs.len()));
    let paragraphs = body.paragraphs.iter().enumerate().map(|(index, paragraph)| {
        let align = paragraph.align.unwrap_or(body.style.align);
        // (Vertical text is not kerned: PowerPoint's PDFs of PTXT-00158 to 00162 are not.)
        let unkerned = |mut s: TextStyle| {
            if body.vertical || body.sideways != 0 {
                s.kern = ibis_doc_model::KernMin(None);
            }
            s
        };
        let styles: Vec<TextStyle> = paragraph.runs.iter().map(|r| unkerned(res.with_theme_font(body.style.with_run(&r.style), &r.text))).collect();
        let end_style = unkerned(res.with_theme_font(body.style.with_run(&paragraph.end_style), ""));
        let first = styles.first().unwrap_or(&end_style);
        let line_height = match paragraph.line_spacing {
            Some(LineSpacing::Percent(p)) => relative(LINE_HEIGHT * p as f32 / 100_000.0),
            Some(LineSpacing::Points(p)) => px(p as f32 / 100.0 * px_per_point).into(),
            None => relative(LINE_HEIGHT),
        };
        let first_size = f32::from(size_px(first));
        let line_px = match paragraph.line_spacing {
            Some(LineSpacing::Percent(p)) => LINE_HEIGHT * p as f32 / 100_000.0 * first_size,
            Some(LineSpacing::Points(p)) => p as f32 / 100.0 * px_per_point,
            None => LINE_HEIGHT * first_size,
        };
        let shift = px(res.baseline_shift(&res.fonts.font(first), first_size, line_px));

        // Superscript and subscript are drawn at two thirds of the size,
        // raised or lowered by `baseline` of the full size.
        let script_scale = |r: &ibis_doc_model::Run| if r.style.baseline != 0 { 2.0 / 3.0 } else { 1.0 };
        // Without a bullet the first line starts at the indent and the others
        // at the margin; with one, the text starts at the margin unless a
        // non-hanging bullet (`indent >= 0`) pushes the first line right.
        let has_text = paragraph.runs.iter().any(|r| !r.text.is_empty());
        let first_line = if let Some(b) = paragraph.bullet.as_ref().filter(|_| has_text) {
            let bullet_emu = (first.font_size as f32 / 100.0 * font_scale * (b.size as f32 / 100_000.0) * 0.55 * EMU_PER_POINT as f32) as i64;
            let text_chars = b.text.chars().count().max(1) as i64;
            (paragraph.indent + bullet_emu * text_chars).max(0)
        } else if paragraph.bullet.is_some() {
            0
        } else {
            (paragraph.margin_left + paragraph.indent).max(0) - paragraph.margin_left
        };
        // PowerPoint puts no space above the first paragraph of a body, nor
        // below the last (measured: at the bottom of its box).
        let space_before = if index == 0 {
            0
        } else if paragraph.space_before_exact {
            paragraph.space_before
        } else {
            // A space before in points is drawn rounded to a whole point (see `Paragraph::space_before_exact`; a half goes up: 5.5 is 6).
            (paragraph.space_before + 50) / 100 * 100
        };
        let space_after = if index + 1 == body.paragraphs.len() { 0 } else { paragraph.space_after };
        // An empty paragraph still takes a line at its mark's size.
        let mark = [ibis_doc_model::Run { text: String::new(), style: paragraph.end_style.clone() }];
        let (runs, run_styles) = if paragraph.runs.is_empty() {
            (&mark[..], std::slice::from_ref(&end_style))
        } else {
            (&paragraph.runs[..], &styles[..])
        };
        // Turned text paints its (text) bullet with its lines.
        // A number or character bullet (not turned) is painted with its first line too: it is in the text's face and
        // size, so on the text's baseline, where a box of its own put it a little low.
        let turned_bullet = Some(()).filter(|_| turn.is_some() || paragraph.bullet.as_ref().is_some_and(|b| b.auto.is_some() || b.text == "•")).and_then(|_| {
            let b = paragraph.bullet.as_ref().filter(|b| has_text && b.picture.is_none())?;
            let bullet_style = if b.auto.is_none()
                && !b.text.chars().any(ibis_doc_model::is_east_asian)
                && first.font.as_deref().is_some_and(is_east_asian_typeface)
                && first.fallback_font.is_some()
            {
                TextStyle { font: first.fallback_font.clone(), bold: false, ..first.clone() }
            } else if b.auto.is_none() {
                TextStyle { bold: false, ..first.clone() }
            } else {
                first.clone()
            };
            let font = res.fonts.font(&bullet_style);
            let first_font = res.fonts.font(first);
            let size = f32::from(size_px(first)) * (b.size as f32 / 100_000.0);
            // As the unturned bullet: its font's height centred in the first
            // line, moved as the first line's text is (`baseline_shift`).
            let (ba, bd) = res.metrics(&font);
            let (fa, fd) = res.metrics(&first_font);
            let centred = |a: f32, d: f32, s: f32| (line_px - (a + d) * s) / 2.0 + a * s;
            Some(crate::render::rich_text::BulletMark {
                text: b.text.clone(),
                font,
                size,
                color: rgb_hsla_mode(b.color.unwrap_or(first.color), res.color_mode),
                x: paragraph.indent as f32 * scale,
                drop: centred(ba, bd, size) - centred(fa, fd, first_size),
            })
        });
        let content: AnyElement = {
            // The ascent and descent Office gives typeface `name`: the
            // font's that draws it, unless another font stands in for it.
            let typeface = |name: Option<&str>, style: &TextStyle| -> (f32, f32) {
                let font = res.fonts.font(&TextStyle { font: name.map(str::to_string), ..style.clone() });
                name.filter(|name| *name != font.family.as_ref())
                    .and_then(crate::render::font_metrics::office_metrics)
                    .unwrap_or_else(|| res.metrics(&font))
            };
            let pieces = runs
                .iter()
                .zip(run_styles)
                .flat_map(|(r, s)| {
                    let full = f32::from(size_px(s));
                    let size = full * script_scale(r);
                    let font = res.fonts.font(s);
                    let default_east_asian = res.default_east_asian_font(s);
                    let latin_scale = res.substitute_scale(s.font.as_deref(), &font);
                    let kana_widths = s
                        .font
                        .as_deref()
                        .filter(|n| res.fonts.available(n).is_none())
                        .and_then(crate::render::font_metrics::fallback_kana_widths);
                    let proportional_alnum = s
                        .font
                        .as_deref()
                        .is_some_and(|n| res.fonts.available(n).is_none() && crate::render::font_metrics::is_proportional_japanese_typeface(n));
                    // A run's East Asian characters are in its East Asian
                    // typeface (`s.font` when it has them) and the others in
                    // its Latin one (`s.fallback_font`). A line is as high as
                    // its highest typeface, so the run is split where the
                    // script changes. (Without an East Asian typeface,
                    // PowerPoint for Mac draws them with ＭＳ ゴシック
                    // (`default_east_asian_font`), and the lines are as its
                    // Latin typeface's in most cases measured, so the run's
                    // typeface's height is kept.)
                    let has_east_asian = r.text.chars().any(crate::render::rich_text::is_cjk);
                    let east_asian = s
                        .font
                        .as_deref()
                        .filter(|name| has_east_asian && is_east_asian_typeface(name))
                        .map(|name| typeface(Some(name), s));
                    let latin = match (has_east_asian, s.fallback_font.as_deref()) {
                        (true, Some(latin)) if s.font.as_deref().is_some_and(is_east_asian_typeface) => typeface(Some(latin), s),
                        _ => typeface(s.font.as_deref(), s),
                    };
                    // Right/left-pointing triangles (U+25B6, U+25B7, U+25C0, U+25C1) are ~0.5 em in MS Gothic / MS PGothic
                    // (which PowerPoint also uses when a Latin font like Verdana lacks them), whereas Hiragino Sans draws
                    // them at 0.93 em; Arial Unicode MS has the MS Gothic-sized ~0.5 em glyphs.
                    let msgothic_triangle_font = s
                        .font
                        .as_deref()
                        .filter(|n| res.fonts.available(n).is_none() || !is_east_asian_typeface(n))
                        .and_then(|_| res.fonts.available("Arial Unicode MS"))
                        .map(|_| res.fonts.font(&TextStyle { font: Some("Arial Unicode MS".into()), fallback_font: None, ..s.clone() }));
                    // Segments: text, ascent and descent, font override (0 = run's, 1 = default East Asian, 2 = MS Gothic symbol stand-in), and the substitute font scale.
                    let mut segments: Vec<(String, (f32, f32), u8, f32)> = Vec::new();
                    for c in r.text.chars() {
                        let cjk = crate::render::rich_text::is_cjk(c);
                        let m = match east_asian {
                            Some(m) if cjk => m,
                            _ => latin,
                        };
                        let other = if matches!(c, '▶' | '▷' | '◀' | '◁') && msgothic_triangle_font.is_some() {
                            2
                        } else if cjk && default_east_asian.is_some() {
                            1
                        } else {
                            0
                        };
                        let sub = if c.is_ascii() { latin_scale } else { 1.0 };
                        match segments.last_mut() {
                            Some((text, last, o, sc)) if *last == m && *o == other && *sc == sub => text.push(c),
                            _ => segments.push((c.to_string(), m, other, sub)),
                        }
                    }
                    if r.style.math.is_some() {
                        // A formula is one piece.
                        segments = vec![(r.text.clone(), latin, 0, 1.0)];
                    }
                    if segments.is_empty() {
                        segments.push((String::new(), latin, 0, 1.0));
                    }
                    // Capitals (`a:rPr/@cap`): each letter is drawn as its
                    // capital (one character for one, so that the caret's
                    // positions stay), small capitals smaller.
                    let segments: Vec<(String, (f32, f32), u8, f32)> = segments
                        .into_iter()
                        .flat_map(|(text, m, other, sub)| crate::render::rich_text::capitals(&text, r.style.caps).into_iter().map(move |(t, k)| (t, m, other, k * sub)))
                        .collect();
                    segments
                        .into_iter()
                        .map(|(text, (a, d), other, k)| {
                            let piece_font = match other {
                                2 => msgothic_triangle_font.clone().unwrap_or_else(|| font.clone()),
                                1 => default_east_asian.clone().unwrap_or_else(|| font.clone()),
                                _ => font.clone(),
                            };
                            let bold_arrow = !r.style.invisible && s.bold && !text.is_empty() && text.chars().all(|c| ('\u{2190}'..='\u{21ff}').contains(&c));
                            crate::render::rich_text::Piece {
                                text,
                                font: piece_font,
                                size: size * k,
                                color: if r.style.invisible {
                                    Hsla::transparent_black()
                                } else {
                                    rgb_hsla_mode(s.color, res.color_mode)
                                },
                                underline: r.style.underline && !r.style.invisible,
                                underline_kind: r.style.underline_kind,
                                raise: r.style.baseline as f32 / 100_000.0 * full,
                                spacing: r.style.spacing as f32 / 100.0 * px_per_point,
                                synthetic_bold: Default::default(),
                                highlight: r.style.highlight.filter(|_| !r.style.invisible).map(|c| rgb_hsla_mode(c, res.color_mode)),
                                outline: r
                                    .style
                                    .outline
                                    .filter(|_| !r.style.invisible)
                                    .map(|(c, w)| (rgb_hsla_mode(c, res.color_mode), w as f32 * scale))
                                    .or_else(|| bold_arrow.then(|| (rgb_hsla_mode(s.color, res.color_mode), 0.035 * size * k))),
                                shadow: if res.color_mode == ColorMode::HighContrast {
                                    None
                                } else {
                                    r.style.shadow.filter(|s| !r.style.invisible && s.sx != 0).map(|s| {
                                        let rad = (s.direction as f32 / 60_000.0).to_radians();
                                        let dist = s.distance as f32 * scale;
                                        (hsla_mode(s.color, res.color_mode), dist * rad.cos(), dist * rad.sin(), s.blur as f32 * scale)
                                    })
                                },
                                inner_shadow: if res.color_mode == ColorMode::HighContrast {
                                    None
                                } else {
                                    r.style.shadow.filter(|s| !r.style.invisible && s.sx == 0).map(|s| {
                                        let rad = (s.direction as f32 / 60_000.0).to_radians();
                                        let dist = s.distance as f32 * scale;
                                        (hsla_mode(s.color, res.color_mode), dist * rad.cos(), dist * rad.sin(), s.blur as f32 * scale)
                                    })
                                },
                                glow: r.style.glow.filter(|_| !r.style.invisible && res.color_mode != ColorMode::HighContrast).map(|(c, rad)| (hsla_mode(c, res.color_mode), rad as f32 * scale)),
                                fill: r.style.fill.clone().filter(|_| !r.style.invisible),
                                ascent: a * size,
                                descent: d * size,
                                strike: if r.style.invisible { ibis_doc_model::Strike::None } else { r.style.strike },
                                strike_size: size,
                                math: r.style.math.as_deref().cloned(),
                                border: r.style.border.filter(|_| !r.style.invisible).map(|b| (rgb_hsla_mode(b.color, res.color_mode), (b.width as f32 * scale).max(0.5), b.space as f32 * scale)),
                                kana_widths,
                                proportional_alnum,
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .collect();
            let line = match paragraph.line_spacing {
                Some(LineSpacing::Percent(p)) => crate::render::rich_text::LineRule::Multiple(LINE_HEIGHT * p as f32 / 100_000.0),
                Some(LineSpacing::Points(p)) => crate::render::rich_text::LineRule::Fixed(p as f32 / 100.0 * px_per_point),
                None => crate::render::rich_text::LineRule::Multiple(LINE_HEIGHT),
            };
            crate::render::rich_text::rich_paragraph(crate::render::rich_text::RichParagraph {
                pieces,
                align,
                font_align: paragraph.font_align,
                line,
                wrap: body.wrap && !single_path,
                first_line: first_line as f32 * scale,
                margin_left: paragraph.margin_left as f32 * scale,
                tabs: paragraph.tabs.iter().map(|t| t.position as f32 * scale).collect(),
                default_tab: paragraph.default_tab.unwrap_or(914_400).max(1) as f32 * scale,
                room: width.map(|w| {
                    let total_w = (w as f32 - (body.insets.left + body.insets.right) as f32).max(0.0);
                    let col_w = if body.columns > 1 && !body.vertical {
                        let cols = body.columns.clamp(1, 16) as f32;
                        let total_spc = (cols - 1.0) * body.column_spacing as f32;
                        ((total_w - total_spc) / cols).max(0.0)
                    } else {
                        total_w
                    };
                    (col_w - paragraph.margin_left as f32).max(0.0) * scale
                }),
                px_per_point,
                text_top: Some(crate::render::rich_text::TextTop {
                    block: block.clone(),
                    index,
                    before: space_before as f32 / 100.0 * px_per_point,
                    after: space_after as f32 / 100.0 * px_per_point,
                }),
                bullet: turned_bullet.clone(),
                positions: slots.as_ref().map(|s| s[index].clone()),
                last: index + 1 == body.paragraphs.len(),
                upright: body.vertical,
                mirror: body.vertical && body.vertical_kind == ibis_doc_model::VerticalKind::Mongolian,
                at_x: origin_x.filter(|_| turn.is_none() && !body.vertical && paragraph.align.unwrap_or(body.style.align) == TextAlign::Left).map(|x0| {
                    x0 + (body.insets.left + paragraph.margin_left) as f32 * scale + crate::render::rich_text::at_x_offset()
                }),
            })
            .into_any_element()
        };

        let above = px(space_before as f32 / 100.0 * px_per_point);
        let bullet = paragraph.bullet.as_ref().filter(|b| has_text && (b.picture.is_some() || (turn.is_none() && b.auto.is_none() && b.text != "•"))).map(|b| {
            let color = b.color.unwrap_or(first.color);
            // A bullet of a symbol typeface (Wingdings) is drawn with the typeface where it is installed: its own glyph, at the
            // bullet's size (the size is scaled down for the stand-in character, whose glyph is bigger than Wingdings').
            let symbol = b.symbol.as_ref().filter(|s| res.fonts.installed.contains(s.font.as_str()));
            let size = size_px(first) * (b.size as f32 / 100_000.0) * symbol.map_or(1.0, |s| 100.0 / s.stand_in_percent.max(1) as f32);
            let left = px((paragraph.margin_left + paragraph.indent) as f32 * scale);
            match b.picture.and_then(|m| res.picture(m)) {
                // A picture bullet is a square in the middle of the first line,
                // 0.7 of the bullet's size (measured against PowerPoint's PDF:
                // a check mark at 142 % of 16 pt).
                Some(image) => {
                    let size = size * 0.7;
                    div()
                    .absolute()
                    .top(above + (px(line_px) - size) / 2.0)
                    .left(left)
                    .size(size)
                    .child(img(image).size_full().object_fit(ObjectFit::Contain))
                }
                None => div()
                    .absolute()
                    .top(above + shift)
                    .left(left)
                    .text_size(size)
                    // A smaller bullet sits in the middle of the first line.
                    .line_height(px(line_px))
                    // A bullet takes the weight of the text's first run (bold headings have bold numbers).
                    .when(b.auto.is_some(), |d| {
                        // A number is set in the face of the text's first run (a bold Japanese heading has a bold number
                        // of the same face).
                        let font = res.fonts.font(first);
                        d.font_family(font.family.clone()).font_weight(font.weight)
                    })
                    .when_some(symbol, |d, s| d.font_family(SharedString::from(s.font.clone())))
                    .text_color(rgb_hsla_mode(color, res.color_mode))
                    .child(SharedString::from(symbol.map_or_else(|| b.text.clone(), |s| s.code.clone()))),
            }
        });

        div()
            .relative()
            .w_full()
            .pl(px(paragraph.margin_left as f32 * scale))
            .pt(above)
            .pb(px(space_after as f32 / 100.0 * px_per_point))
            .line_height(line_height)
            .map(|d| align_text(d, align))
            .children(bullet)
            .child(content)
    });

    div()
        .size_full()
        .flex()
        .flex_col()
        .map(|d| match body.anchor {
            VerticalAnchor::Top => d.justify_start(),
            VerticalAnchor::Middle => d.justify_center(),
            VerticalAnchor::Bottom => d.justify_end(),
        })
        .pl(px(body.insets.left as f32 * scale))
        .pr(px(body.insets.right as f32 * scale))
        .pt(px(body.insets.top as f32 * scale))
        .pb(px(body.insets.bottom as f32 * scale))
        .when(!body.wrap, |d| d.whitespace_nowrap())
        .child({
            if body.columns > 1 && !body.vertical && width.is_some() {
                // Columns: the paragraphs run down the first column to the bottom of the box, then go on at the top of
                // the next (a wrapping column of fixed height, a paragraph at a time).
                let cols = body.columns.clamp(1, 16) as f32;
                let total_w = (width.unwrap_or(0.0) as f32 - (body.insets.left + body.insets.right) as f32).max(0.0);
                let col_w = ((total_w - (cols - 1.0) * body.column_spacing as f32) / cols).max(1.0) * scale;
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .children(paragraphs.map(|p| div().w(px(col_w)).flex_shrink_0().child(p)))
                    .into_any_element()
            } else {
                div().w_full().flex().flex_col().children(paragraphs).into_any_element()
            }
        })
        // After the paragraphs are prepainted (their heights known), the
        // text's place in the whole area (the padding is the insets).
        .child({
            let insets = (body.insets.top as f32 * scale, body.insets.bottom as f32 * scale);
            let anchor = match body.anchor {
                VerticalAnchor::Top => 0.0,
                VerticalAnchor::Middle => 0.5,
                VerticalAnchor::Bottom => 1.0,
            };
            canvas(
                move |bounds, _, _| {
                    let (top, height) = (f32::from(bounds.origin.y), f32::from(bounds.size.height));
                    block.place(top + insets.0, height - insets.0 - insets.1, anchor);
                    let middle = bounds.center();
                    block.turn(f32::from(middle.x), f32::from(middle.y), turn);
                    // (The glyphs of a bent text are collected again as the paragraphs paint.)
                    if let Some(w) = block.warp.borrow_mut().as_mut() {
                        w.glyphs.clear();
                        w.lines.clear();
                    }
                },
                move |bounds, _, window, _| {
                    if let Some(w) = warped.warp.borrow().as_ref() {
                        crate::render::rich_text::paint_warp(window, w, (f32::from(bounds.origin.x), f32::from(bounds.origin.y), f32::from(bounds.size.width), f32::from(bounds.size.height)));
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full()
        })
}

/// WordArt vertical writing (`wordArtVert`, `wordArtVertRtl`): every
/// character upright in a cell of its own, stacked down a column, the
/// columns from left to right (from right to left for `wordArtVertRtl`),
/// a paragraph starting a column and a column wrapping at the frame's
/// height. Measured in PowerPoint's PDF at 14 pt: a Latin character's cell
/// is 1.42 em square, an East Asian one's 1.29 em wide and 1.17 em high. The
/// anchor places the columns across the frame (top: where they start) and
/// a paragraph's alignment its characters along its column.
fn vertical_text_element(body: &TextBody, scale: f32, res: &Resources) -> Div {
    let px_per_point = scale * EMU_PER_POINT as f32;
    let font_scale = body.font_scale as f32 / 100_000.0;
    let rtl = body.vertical_kind == ibis_doc_model::VerticalKind::WordArtRtl;
    let mut cells: Vec<AnyElement> = Vec::new();
    for (n, paragraph) in body.paragraphs.iter().enumerate() {
        if n > 0 {
            // A new paragraph starts a column.
            cells.push(div().w(px(0.0)).h_full().into_any_element());
        }
        for run in &paragraph.runs {
            let style = res.with_theme_font(body.style.with_run(&run.style), &run.text);
            let font = res.font(&style);
            let size = ibis_doc_model::autofit_font_size_pt(style.font_size_pt(), body.font_scale) * px_per_point;
            let color = rgb_hsla_mode(style.color, res.color_mode);
            for c in run.text.chars() {
                let (w, h) = if crate::render::rich_text::is_cjk(c) { (1.2857 * size, 1.1657 * size) } else { (1.4229 * size, 1.4229 * size) };
                cells.push(
                    div()
                        .flex_shrink_0()
                        .w(px(w))
                        .h(px(h))
                        .flex()
                        .justify_center()
                        .items_center()
                        .font_family(font.family.clone())
                        .font_weight(font.weight)
                        .font_features(font.features.clone())
                        .text_size(px(size))
                        .line_height(px(h))
                        .text_color(color)
                        .child(SharedString::from(c.to_string()))
                        .into_any_element(),
                );
            }
        }
    }
    let align = body.paragraphs.first().and_then(|p| p.align).unwrap_or(body.style.align);
    div()
        .size_full()
        .flex()
        .flex_col()
        .map(|d| if rtl { d.flex_wrap_reverse() } else { d.flex_wrap() })
        .map(|d| match body.anchor {
            VerticalAnchor::Top => d.content_start(),
            VerticalAnchor::Middle => d.content_center(),
            VerticalAnchor::Bottom => d.content_end(),
        })
        .map(|d| match align {
            TextAlign::Left | TextAlign::Justify | TextAlign::Distributed => d.justify_start(),
            TextAlign::Center => d.justify_center(),
            TextAlign::Right => d.justify_end(),
        })
        .pl(px(body.insets.left as f32 * scale))
        .pr(px(body.insets.right as f32 * scale))
        .pt(px(body.insets.top as f32 * scale))
        .pb(px(body.insets.bottom as f32 * scale))
        .children(cells)
}

pub(crate) fn align_text<E: Styled>(element: E, align: TextAlign) -> E {
    match align {
        TextAlign::Left | TextAlign::Justify | TextAlign::Distributed => element.text_left(),
        TextAlign::Center => element.text_center(),
        TextAlign::Right => element.text_right(),
    }
}

/// The font of a body's style, for the text field that edits it.
pub(crate) fn body_font(style: &TextStyle, res: &Resources) -> Font {
    res.fonts.font(style)
}

thread_local! {
    /// Pair kerning by (font, first, second), in ems.
    static PAIR_KERNING: std::cell::RefCell<HashMap<(FontId, char, char), f32>> = std::cell::RefCell::new(HashMap::new());
    /// A font's shaping data, built once: building it (the font's tables,
    /// tens of milliseconds for a Japanese font) for every new pair made
    /// opening a 144-page Japanese document take 10 s.
    static SHAPER_DATA: std::cell::RefCell<HashMap<FontId, Option<std::rc::Rc<(std::sync::Arc<Vec<u8>>, u32, harfrust::ShaperData)>>>> = std::cell::RefCell::new(HashMap::new());
}

impl Resources {
    /// How much the pair `a` `b` is kerned in `font` at `size` (points;
    /// negative: closer), from the kerning of the font file drawn (its kern
    /// table or GPOS pair adjustments, through harfrust): the advance of the
    /// two shaped with kerning less that without. 0 when the font's data is
    /// not available. (Word's own Calibri was used only to check that
    /// kerning explains Word's line breaks: DBDY-00007.)
    pub(crate) fn pair_kerning(&self, font: &Font, size: f32, a: char, b: char) -> f32 {
        let id = self.text_system.resolve_font(font);
        if let Some(em) = PAIR_KERNING.with(|c| c.borrow().get(&(id, a, b)).copied()) {
            return em * size;
        }
        let entry = SHAPER_DATA.with(|c| {
            c.borrow_mut()
                .entry(id)
                .or_insert_with(|| {
                    let (data, index) = self.text_system.font_data(id)?;
                    let font = harfrust::FontRef::from_index(&data, index).ok()?;
                    let shaper_data = harfrust::ShaperData::new(&font);
                    Some(std::rc::Rc::new((data, index, shaper_data)))
                })
                .clone()
        });
        let em = entry
            .and_then(|entry| {
                let (data, index, shaper_data) = &*entry;
                let font = harfrust::FontRef::from_index(data, *index).ok()?;
                let shaper = shaper_data.shaper(&font).build();
                let advance = |kern: u32| {
                    let mut buffer = harfrust::UnicodeBuffer::new();
                    let mut text = [0u8; 8];
                    let mut s = String::new();
                    s.push_str(a.encode_utf8(&mut text));
                    s.push_str(b.encode_utf8(&mut text));
                    buffer.push_str(&s);
                    buffer.guess_segment_properties();
                    let features = [harfrust::Feature::new(harfrust::Tag::new(b"kern"), kern, ..)];
                    let shaped = shaper.shape(buffer, &features);
                    shaped.glyph_positions().iter().map(|p| p.x_advance).sum::<i32>()
                };
                let upem = shaper.units_per_em().max(1) as f32;
                Some((advance(1) - advance(0)) as f32 / upem)
            })
            .unwrap_or(0.0);
        PAIR_KERNING.with(|c| c.borrow_mut().insert((id, a, b), em));
        em * size
    }
}

#[cfg(test)]
mod dashed_line_tests {
    use super::dashed_line_path;
    use gpui_kit::{OutlineVerb, Pixels, Point, point, px};

    fn along(x0: f32, x1: f32) -> Vec<Point<Pixels>> {
        vec![point(px(x0), px(5.0)), point(px(x1), px(5.0))]
    }

    /// A dashed line is one path, on the screen (its dashes, tessellated) and in a PDF (its outline: the line whole,
    /// stroked at the line's width with butt ends and the pattern as the `d` operator has it), however many dashes.
    #[test]
    fn a_dashed_line_is_one_path_with_its_pattern_for_a_pdf() {
        let path = dashed_line_path(&along(0.0, 300.0), px(2.0), &[4.0, 3.0], 1.5).expect("a path");
        assert!(!path.vertices.is_empty(), "tessellated into dashes for the screen");
        let outline = path.outline.as_ref().expect("an outline for a PDF");
        assert_eq!(outline.verbs.len(), 2, "the line whole, not its dashes");
        assert!(matches!(outline.verbs[0], OutlineVerb::MoveTo(p) if p == point(px(0.0), px(5.0))));
        assert!(matches!(outline.verbs[1], OutlineVerb::LineTo(p) if p == point(px(300.0), px(5.0))));
        let stroke = outline.stroke.expect("stroked");
        assert_eq!(f32::from(stroke.width), 2.0);
        assert!(!stroke.round_caps, "butt ends: a dash is as long as it is said");
        let dash = stroke.dash.expect("with its dashes");
        assert_eq!(dash.pattern(), &[4.0, 3.0]);
        assert_eq!(dash.phase, 1.5);
    }

    /// The patterns of every line type fit (the longest has six lengths); a line along a bend keeps its corner.
    #[test]
    fn a_pattern_of_six_lengths_and_a_bend_are_kept() {
        let bent = vec![point(px(0.0), px(0.0)), point(px(40.0), px(0.0)), point(px(40.0), px(30.0))];
        let pattern = [4.0, 3.0, 1.0, 3.0, 1.0, 3.0];
        let path = dashed_line_path(&bent, px(1.0), &pattern, 0.0).expect("a path");
        let outline = path.outline.as_ref().expect("an outline");
        assert_eq!(outline.verbs.len(), 3, "a move and two lines");
        assert_eq!(outline.stroke.and_then(|s| s.dash).map(|d| d.pattern().to_vec()), Some(pattern.to_vec()));
    }

    /// Nothing to draw is no path: no pattern, one point, or a pattern of no length.
    #[test]
    fn nothing_to_dash_is_no_path() {
        assert!(dashed_line_path(&along(0.0, 10.0), px(1.0), &[], 0.0).is_none());
        assert!(dashed_line_path(&[point(px(1.0), px(1.0))], px(1.0), &[1.0, 1.0], 0.0).is_none());
        assert!(dashed_line_path(&along(0.0, 10.0), px(1.0), &[0.0, 0.0], 0.0).is_none());
    }

    /// Where the line starts in the pattern: a phase of the dash's length starts in the gap, so the screen's dashes begin
    /// later than the line's start (the PDF's `d` phase is the same number).
    #[test]
    fn the_phase_moves_the_dashes_along() {
        let at_start = dashed_line_path(&along(0.0, 20.0), px(1.0), &[4.0, 4.0], 0.0).expect("a path");
        let shifted = dashed_line_path(&along(0.0, 20.0), px(1.0), &[4.0, 4.0], 4.0).expect("a path");
        let left = |p: &gpui_kit::Path<Pixels>| p.vertices.iter().map(|v| f32::from(v.xy_position.x)).fold(f32::MAX, f32::min);
        assert!(left(&at_start) < 0.5, "a dash starts at the start: {}", left(&at_start));
        assert!(left(&shifted) > 3.5, "the first dash is a gap in: {}", left(&shifted));
    }
}

#[cfg(test)]
mod font_resolution_tests {
    use super::*;

    /// PowerPoint draws a typeface that is not installed with Calibri; Word and Excel do not (Cambria and Arial): only a
    /// presentation's fonts say so. Windows' FontSubstitutes name (Helvetica) goes to its installed typeface first.
    #[test]
    fn a_missing_latin_typeface_is_calibri_in_a_presentation_only() {
        let installed: HashSet<String> = ["Calibri", "Arial", "Carlito"].iter().map(|s| s.to_string()).collect();
        let mut fonts = Fonts { installed, embedded_index: OpenTypeIndex::default(), east_asian_fallback: None, substitutions: HashMap::new(), calibri_for_missing: true, docx_font_families: None };
        assert_eq!(fonts.family("Zzqx Missing Sans").0.as_ref(), "Calibri");
        assert_eq!(fonts.family("Helvetica").0.as_ref(), "Arial");
        fonts.calibri_for_missing = false;
        assert_eq!(fonts.family("Zzqx Missing Sans").0.as_ref(), "Arial");
    }
    use core::prelude::v1::test;

    /// Build a minimal valid TrueType font binary with a `name` table and an `OS/2` table.
    fn make_test_ttf(records: &[(u16, &str)], weight: u16, italic: bool) -> Vec<u8> {
        // Build `name` table (format 0) with Windows Unicode BMP (platform 3, encoding 1) records.
        let mut string_storage = Vec::new();
        let mut name_records = Vec::new();
        for &(name_id, text) in records {
            let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
            let offset = string_storage.len() as u16;
            let len = utf16.len() as u16;
            string_storage.extend_from_slice(&utf16);
            let mut rec = Vec::with_capacity(12);
            rec.extend_from_slice(&3u16.to_be_bytes()); // platformID = 3 (Windows)
            rec.extend_from_slice(&1u16.to_be_bytes()); // encodingID = 1 (Unicode BMP)
            rec.extend_from_slice(&0x0409u16.to_be_bytes()); // languageID
            rec.extend_from_slice(&name_id.to_be_bytes());
            rec.extend_from_slice(&len.to_be_bytes());
            rec.extend_from_slice(&offset.to_be_bytes());
            name_records.push(rec);
        }
        let count = name_records.len() as u16;
        let storage_offset = 6 + count * 12;
        let mut name_table = Vec::new();
        name_table.extend_from_slice(&0u16.to_be_bytes()); // format = 0
        name_table.extend_from_slice(&count.to_be_bytes());
        name_table.extend_from_slice(&storage_offset.to_be_bytes());
        for rec in name_records {
            name_table.extend_from_slice(&rec);
        }
        name_table.extend_from_slice(&string_storage);

        // Build `OS/2` table (64 bytes).
        let mut os2_table = vec![0u8; 64];
        os2_table[4..6].copy_from_slice(&weight.to_be_bytes());
        let fs_selection: u16 = if italic { 0x0001 } else { 0x0040 };
        os2_table[62..64].copy_from_slice(&fs_selection.to_be_bytes());

        // Assemble SFNT with 2 tables (`OS/2` and `name`).
        let num_tables = 2u16;
        let os2_offset = 12 + u32::from(num_tables) * 16;
        let os2_len = os2_table.len() as u32;
        let name_offset = os2_offset + os2_len;
        let name_len = name_table.len() as u32;

        let mut ttf = Vec::new();
        ttf.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        ttf.extend_from_slice(&num_tables.to_be_bytes());
        ttf.extend_from_slice(&32u16.to_be_bytes());
        ttf.extend_from_slice(&1u16.to_be_bytes());
        ttf.extend_from_slice(&0u16.to_be_bytes());

        // Record 0: OS/2
        ttf.extend_from_slice(b"OS/2");
        ttf.extend_from_slice(&0u32.to_be_bytes());
        ttf.extend_from_slice(&os2_offset.to_be_bytes());
        ttf.extend_from_slice(&os2_len.to_be_bytes());

        // Record 1: name
        ttf.extend_from_slice(b"name");
        ttf.extend_from_slice(&0u32.to_be_bytes());
        ttf.extend_from_slice(&name_offset.to_be_bytes());
        ttf.extend_from_slice(&name_len.to_be_bytes());

        ttf.extend_from_slice(&os2_table);
        ttf.extend_from_slice(&name_table);
        ttf
    }

    #[test]
    fn a_variable_font_family_is_regular_at_400_not_at_its_default_instance() {
        let fvar = |min: f32, default: f32, max: f32| {
            let mut t = vec![0, 1, 0, 0, 0, 16, 0, 2, 0, 1, 0, 20, 0, 0, 0, 0];
            t.extend_from_slice(b"wght");
            for v in [min, default, max] {
                t.extend_from_slice(&((v * 65536.0) as i32).to_be_bytes());
            }
            t.extend_from_slice(&[0, 0, 1, 1]);
            t
        };
        assert_eq!(variable_family_weight(&fvar(100.0, 100.0, 900.0)), Some(400)); // Noto Sans JP's file: default Thin
        assert_eq!(variable_family_weight(&fvar(500.0, 600.0, 900.0)), Some(500));
        assert_eq!(variable_family_weight(&fvar(100.0, 300.0, 300.0)), Some(300));
        let mut other = fvar(100.0, 100.0, 900.0);
        other[16..20].copy_from_slice(b"wdth");
        assert_eq!(variable_family_weight(&other), None);
    }

    #[test]
    fn strip_weight_suffix_handles_semilight_and_compound_weights() {
        assert_eq!(strip_weight_suffix("UD デジタル 教科書体 NK-R"), Some(("UD デジタル 教科書体 NK", FontWeight::NORMAL))); // i18n-ok（フォント名の試験）
        assert_eq!(strip_weight_suffix("UD デジタル 教科書体 NK-B"), Some(("UD デジタル 教科書体 NK", FontWeight::BOLD))); // i18n-ok（フォント名の試験）
        assert_eq!(strip_weight_suffix("Foo-R"), None, "only a name with a non-ASCII (Japanese) family");
        assert_eq!(strip_weight_suffix("Segoe UI Semilight"), Some(("Segoe UI", FontWeight(350.0))));
        assert_eq!(strip_weight_suffix("Segoe UI SemiLight"), Some(("Segoe UI", FontWeight(350.0))));
        assert_eq!(strip_weight_suffix("SEGOE UI SEMILIGHT"), Some(("SEGOE UI", FontWeight(350.0))));
        assert_eq!(strip_weight_suffix("Yu Gothic UI Semilight"), Some(("Yu Gothic UI", FontWeight(350.0))));
        assert_eq!(strip_weight_suffix("Segoe UI Semibold"), Some(("Segoe UI", FontWeight::SEMIBOLD)));
        assert_eq!(strip_weight_suffix("Poppins SemiBold"), Some(("Poppins", FontWeight::SEMIBOLD)));
        assert_eq!(strip_weight_suffix("Poppins ExtraLight"), Some(("Poppins", FontWeight::EXTRA_LIGHT)));
        assert_eq!(strip_weight_suffix("Poppins Extralight"), Some(("Poppins", FontWeight::EXTRA_LIGHT)));
        assert_eq!(strip_weight_suffix("Segoe UI Light"), Some(("Segoe UI", FontWeight::LIGHT)));
        assert_eq!(strip_weight_suffix("Segoe UI Black"), Some(("Segoe UI", FontWeight::BLACK)));
        assert_eq!(strip_weight_suffix("Hiragino Kaku Gothic Std W8"), Some(("Hiragino Kaku Gothic Std", FontWeight(800.0))));
        assert_eq!(strip_weight_suffix("Segoe UI"), None);
        assert_eq!(strip_weight_suffix("Light"), None);
    }

    #[test]
    fn opentype_index_maps_id1_subfamily_to_id16_family_and_os2_weight() {
        let regular_ttf = make_test_ttf(
            &[(1, "Segoe UI"), (4, "Segoe UI"), (16, "Segoe UI")],
            400,
            false,
        );
        let bold_ttf = make_test_ttf(
            &[(1, "Segoe UI"), (4, "Segoe UI Bold"), (16, "Segoe UI")],
            700,
            false,
        );
        let semilight_ttf = make_test_ttf(
            &[(1, "Segoe UI Semilight"), (4, "Segoe UI Semilight"), (16, "Segoe UI")],
            350,
            false,
        );
        let semilight_italic_ttf = make_test_ttf(
            &[(1, "Segoe UI Semilight"), (4, "Segoe UI Semilight Italic"), (16, "Segoe UI")],
            350,
            true,
        );

        let mut index = OpenTypeIndex::default();
        for data in [&regular_ttf, &bold_ttf, &semilight_ttf, &semilight_italic_ttf] {
            let faces = parse_opentype_faces_from_slice(data);
            assert_eq!(faces.len(), 1);
            index.insert_face(&faces[0]);
        }

        let installed: HashSet<String> = ["Segoe UI".to_string()].into_iter().collect();

        // Plain family resolves to weight 400, non-italic.
        let (fam, w, it) = resolve_font_with_index("Segoe UI", &installed, Some(&index), false).expect("Segoe UI");
        assert_eq!(fam.as_ref(), "Segoe UI");
        assert_eq!(w, FontWeight(400.0));
        assert!(!it);

        // GDI subfamily "Segoe UI Semilight" resolves to typographic family "Segoe UI" at weight 350, non-italic.
        let (fam, w, it) = resolve_font_with_index("Segoe UI Semilight", &installed, Some(&index), false).expect("Segoe UI Semilight");
        assert_eq!(fam.as_ref(), "Segoe UI");
        assert_eq!(w, FontWeight(350.0));
        assert!(!it);

        // Full name "Segoe UI Semilight Italic" (ID 4) resolves to "Segoe UI" at weight 350, italic.
        let (fam, w, it) = resolve_font_with_index("Segoe UI Semilight Italic", &installed, Some(&index), false).expect("Segoe UI Semilight Italic");
        assert_eq!(fam.as_ref(), "Segoe UI");
        assert_eq!(w, FontWeight(350.0));
        assert!(it);
    }

    #[test]
    fn opentype_index_resolves_localized_japanese_and_english_names() {
        let yugoth_m = make_test_ttf(
            &[
                (1, "Yu Gothic Medium"),
                (1, "游ゴシック Medium"),
                (16, "Yu Gothic"),
                (16, "游ゴシック"),
            ],
            500,
            false,
        );
        let mut index = OpenTypeIndex::default();
        for face in parse_opentype_faces_from_slice(&yugoth_m) {
            index.insert_face(&face);
        }

        // Japanese Windows (text_system lists "游ゴシック")
        let ja_installed: HashSet<String> = ["游ゴシック".to_string()].into_iter().collect();
        let (fam, w, _) = resolve_font_with_index("Yu Gothic Medium", &ja_installed, Some(&index), false).expect("resolved on ja-JP");
        assert_eq!(fam.as_ref(), "游ゴシック");
        assert_eq!(w, FontWeight(500.0));

        // English Windows (text_system lists "Yu Gothic")
        let en_installed: HashSet<String> = ["Yu Gothic".to_string()].into_iter().collect();
        let (fam, w, _) = resolve_font_with_index("游ゴシック Medium", &en_installed, Some(&index), false).expect("resolved on en-US");
        assert_eq!(fam.as_ref(), "Yu Gothic");
        assert_eq!(w, FontWeight(500.0));

        // macOS (text_system lists "YuGothic" via TYPEFACE_ALIASES)
        let mac_installed: HashSet<String> = ["YuGothic".to_string()].into_iter().collect();
        let (fam, w, _) = resolve_font_with_index("游ゴシック Medium", &mac_installed, Some(&index), false).expect("resolved on macOS");
        assert_eq!(fam.as_ref(), "YuGothic");
        assert_eq!(w, FontWeight(500.0));
    }

    #[test]
    fn fonts_struct_resolves_weight_suffixed_typefaces_and_bold_interaction() {
        let fonts = Fonts {
            installed: ["Segoe UI".to_string(), "YuGothic".to_string()].into_iter().collect(),
            embedded_index: OpenTypeIndex::default(),
            east_asian_fallback: None,
            substitutions: HashMap::new(),
            calibri_for_missing: false,
            docx_font_families: None,
        };

        assert_eq!(
            fonts.available("Segoe UI Semilight"),
            Some((SharedString::from("Segoe UI"), FontWeight(350.0)))
        );
        assert_eq!(
            fonts.available("游ゴシック Light"),
            Some((SharedString::from("YuGothic"), FontWeight::LIGHT))
        );
        assert!(fonts.available("UninstalledFont Semilight").is_none());

        // Plain Semilight run has weight 350.
        let f_plain = fonts.font(&TextStyle {
            font: Some("Segoe UI Semilight".to_string()),
            bold: false,
            ..TextStyle::default()
        });
        assert_eq!(f_plain.family.as_ref(), "Segoe UI");
        assert_eq!(f_plain.weight, FontWeight(350.0));

        // Bold on a Semilight run promotes weight to BOLD (700).
        let f_bold = fonts.font(&TextStyle {
            font: Some("Segoe UI Semilight".to_string()),
            bold: true,
            ..TextStyle::default()
        });
        assert_eq!(f_bold.family.as_ref(), "Segoe UI");
        assert_eq!(f_bold.weight, FontWeight::BOLD);

        // Bold on a Black (900) run preserves weight 900 rather than demoting to 700.
        let f_black = fonts.font(&TextStyle {
            font: Some("Segoe UI Black".to_string()),
            bold: true,
            ..TextStyle::default()
        });
        assert_eq!(f_black.family.as_ref(), "Segoe UI");
        assert_eq!(f_black.weight, FontWeight::BLACK);
    }

    #[test]
    fn presentation_font_availability_checks_embedded_base_and_substitutions() {
        let mut pres = ibis_doc_model::Presentation::default();
        pres.fonts.push(ibis_doc_model::EmbeddedFont {
            typeface: "Segoe UI".to_string(),
            bold: false,
            italic: false,
            data: vec![1, 2, 3].into(),
        });
        pres.font_substitutions.insert("CustomMissing".to_string(), "Segoe UI Semilight".to_string());

        assert!(is_presentation_font_available("Segoe UI", &pres));
        assert!(is_presentation_font_available("Segoe UI Semilight", &pres));
        assert!(is_presentation_font_available("Segoe UI Semibold", &pres));
        assert!(is_presentation_font_available("CustomMissing", &pres));
        assert!(!is_presentation_font_available("CompletelyUnknownFont Semilight", &pres));
    }
}

