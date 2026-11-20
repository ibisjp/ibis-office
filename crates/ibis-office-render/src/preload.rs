// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Decoding a document's pictures and embedded fonts on other threads, from
//! the moment the file is parsed, so the window opens while they decode and
//! the first slide shows as soon as its own pictures are ready.
//!
//! Pictures are decoded in the order they are first seen: the first slide's
//! first, then the next slide's, and so on, as PowerPoint fills its slide
//! list. `prioritize` puts a slide's pictures ahead when it is shown.
//! Drawing uses a picture once it is decoded (`Preload::decoded`), and a
//! thumbnail waits for all of its slide's pictures (`Preload::slide_ready`).
//! The editor redraws as they come (`Preload::generation`).
//!
//! In the browser, which has no threads here, nothing is decoded ahead:
//! pictures are left to GPUI's image loading, fonts decoded when asked for.
// The browser uses only part of this.
#![cfg_attr(target_family = "wasm", allow(dead_code))]

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use gpui_kit::{Image, ImageFormat, RenderImage};
use ibis_doc_model::{EmbeddedFont, Fill, Media, MediaFormat, MediaId, Presentation, Shape, ShapeKind, Slide, TextBody};

use crate::picture_svg::Encoding;

/// A picture made from a media part: GPUI's image, and its encoded data
/// when it can be wrapped in SVG (to rotate, cut or recolour it).
pub(crate) type Prepared = (Arc<Image>, Option<(Encoding, Arc<[u8]>)>);

/// An embedded font decoded to TrueType: its family and data, or `None`
/// when it could not be decoded.
pub(crate) type DecodedFont = Option<(String, Vec<u8>)>;

enum Picture {
    Queued,
    Decoding,
    Done(Arc<RenderImage>),
    /// Could not be decoded here; left to GPUI's own loading.
    Failed,
}

enum FontSlot {
    Pending,
    Ready(DecodedFont),
    Taken,
}

#[derive(Default)]
struct State {
    /// Media parts not yet taken to be prepared, by index.
    next_prepare: usize,
    /// Media parts prepared (or found unusable) so far.
    prepared_count: usize,
    prepared: HashMap<MediaId, Prepared>,
    /// Pictures to decode, the next first.
    queue: VecDeque<MediaId>,
    pictures: HashMap<MediaId, Picture>,
    /// Embedded fonts by the address of their data.
    fonts: HashMap<usize, FontSlot>,
    /// Fonts whose data is the same as another's, to the other's address.
    aliases: HashMap<usize, usize>,
}

struct Shared {
    media: Vec<Media>,
    state: Mutex<State>,
    changed: Condvar,
    /// Counts decoded pictures, for the editor to redraw when it moves.
    generation: AtomicU64,
}

/// The pictures and fonts of one presentation, decoding.
#[derive(Clone)]
pub struct Preload {
    shared: Arc<Shared>,
    /// Whether pictures are decoded here (not in the browser).
    threaded: bool,
}

/// The preload started for the document just read, waiting for its
/// `Resources`.
static NEXT: Mutex<Option<(Vec<usize>, Preload)>> = Mutex::new(None);

/// The addresses of a presentation's media data, which identify it.
fn key(presentation: &Presentation) -> Vec<usize> {
    presentation.media.iter().map(|m| Arc::as_ptr(&m.data) as *const u8 as usize).collect()
}

fn font_key(font: &EmbeddedFont) -> usize {
    Arc::as_ptr(&font.data) as *const u8 as usize
}

impl Preload {
    /// Start decoding `presentation`'s pictures and fonts, for the
    /// `Resources` made for it next (`Preload::for_presentation`).
    pub fn start_for_next(presentation: &Presentation) {
        let preload = Self::start(presentation);
        *NEXT.lock().unwrap_or_else(|e| e.into_inner()) = Some((key(presentation), preload));
    }

    /// The preload started for `presentation`, or a new one.
    pub(crate) fn for_presentation(presentation: &Presentation) -> Self {
        let mut next = NEXT.lock().unwrap_or_else(|e| e.into_inner());
        if next.as_ref().is_some_and(|(k, _)| *k == key(presentation)) {
            return next.take().expect("just checked").1;
        }
        drop(next);
        Self::start(presentation)
    }

    fn start(presentation: &Presentation) -> Self {
        let shared = Arc::new(Shared {
            media: presentation.media.clone(),
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            generation: AtomicU64::new(0),
        });
        let threaded = cfg!(not(target_family = "wasm"));
        let preload = Self { shared, threaded };
        if !threaded {
            // Prepared at once; decoded by GPUI as drawn.
            let mut state = preload.lock();
            for media in &preload.shared.media {
                if let Some(prepared) = prepare(media) {
                    state.prepared.insert(media.id, prepared);
                }
            }
            state.next_prepare = preload.shared.media.len();
            state.prepared_count = preload.shared.media.len();
            drop(state);
            return preload;
        }
        #[cfg(not(target_family = "wasm"))]
        {
            let mut state = preload.lock();
            // The pictures in the order the slides show them, then the rest.
            let mut order = Vec::new();
            for slide in &presentation.slides {
                slide_media(slide, &mut order);
            }
            order.extend(presentation.media.iter().map(|m| m.id));
            for id in order {
                if let std::collections::hash_map::Entry::Vacant(e) = state.pictures.entry(id) {
                    e.insert(Picture::Queued);
                    state.queue.push_back(id);
                }
            }
            // Each font on a thread of its own: the large ones take longest.
            // A font stored twice (for two styles) is decoded once.
            let mut seen: Vec<&EmbeddedFont> = Vec::new();
            for font in &presentation.fonts {
                let k = font_key(font);
                if state.fonts.contains_key(&k) {
                    continue;
                }
                if let Some(first) = seen.iter().find(|f| f.data == font.data) {
                    state.aliases.insert(k, font_key(first));
                    continue;
                }
                seen.push(font);
                state.fonts.insert(k, FontSlot::Pending);
                let (shared, data) = (preload.shared.clone(), font.data.clone());
                std::thread::spawn(move || {
                    // The window waits for the fonts: ahead of the pictures.
                    set_priority(Priority::Interactive);
                    let decoded = decode_font(&data);
                    let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
                    state.fonts.insert(k, FontSlot::Ready(decoded));
                    shared.changed.notify_all();
                });
            }
            // Cores left by the main thread and the fonts decoding.
            let fonts = state.fonts.len();
            drop(state);
            let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
            let workers = cores.saturating_sub(1 + fonts).clamp(2, 8);
            for _ in 0..workers {
                let shared = preload.shared.clone();
                std::thread::spawn(move || {
                    set_priority(Priority::Initiated);
                    work(&shared)
                });
            }
        }
        preload
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.shared.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The pictures made from the media so far, without waiting.
    pub(crate) fn prepared_now(&self) -> HashMap<MediaId, Prepared> {
        self.lock().prepared.clone()
    }

    /// Prepare a single media item on demand if not already prepared.
    pub(crate) fn prepared_one(&self, id: MediaId) -> Option<Prepared> {
        {
            let state = self.lock();
            if let Some(prepared) = state.prepared.get(&id) {
                return Some(prepared.clone());
            }
            if matches!(state.pictures.get(&id), Some(Picture::Failed)) {
                return None;
            }
        }
        let media = self.shared.media.iter().find(|m| m.id == id)?.clone();
        let prepared = prepare(&media);
        let mut state = self.lock();
        if let Some(p) = state.prepared.get(&id) {
            return Some(p.clone());
        }
        match &prepared {
            Some(p) => {
                state.prepared.insert(id, p.clone());
                state.prepared_count += 1;
            }
            None => {
                state.pictures.insert(id, Picture::Failed);
                state.prepared_count += 1;
            }
        }
        self.shared.changed.notify_all();
        prepared
    }

    /// Whether `id` refers to a media format that can be drawn as an image.
    pub(crate) fn is_image_supported(&self, id: MediaId) -> bool {
        self.shared.media.iter().find(|m| m.id == id).is_some_and(|m| image_format(m.format).is_some() || matches!(m.format, MediaFormat::Emf | MediaFormat::Wmf))
    }

    /// The pictures made from the media, waiting until all are.
    #[allow(dead_code)]
    pub fn prepared(&self) -> HashMap<MediaId, Prepared> {
        let mut state = self.lock();
        while state.prepared_count < self.shared.media.len() {
            state = self.shared.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        state.prepared.clone()
    }

    /// `font` decoded, waiting for it if it is still decoding.
    pub fn font(&self, font: &EmbeddedFont) -> DecodedFont {
        let mut state = self.lock();
        let k = font_key(font);
        let k = state.aliases.get(&k).copied().unwrap_or(k);
        loop {
            match state.fonts.get(&k) {
                Some(FontSlot::Pending) => state = self.shared.changed.wait(state).unwrap_or_else(|e| e.into_inner()),
                Some(FontSlot::Ready(_)) => {
                    // Taken once: the same data is registered once
                    // (`Fonts::add_embedded` finds it by its hash after).
                    let Some(FontSlot::Ready(decoded)) = state.fonts.insert(k, FontSlot::Taken) else { unreachable!() };
                    return decoded;
                }
                Some(FontSlot::Taken) | None => {
                    drop(state);
                    return decode_font(&font.data);
                }
            }
        }
    }

    /// The picture of `media`, decoded: `Ok(None)` while it is still
    /// decoding, `Err(())` when it is left to GPUI (not decoded here).
    pub fn decoded(&self, media: MediaId) -> Result<Option<Arc<RenderImage>>, ()> {
        if !self.threaded {
            return Err(());
        }
        match self.lock().pictures.get(&media) {
            Some(Picture::Done(image)) => Ok(Some(image.clone())),
            Some(Picture::Queued | Picture::Decoding) => Ok(None),
            Some(Picture::Failed) | None => Err(()),
        }
    }

    /// Whether all of `slide`'s pictures are decoded (or left to GPUI).
    pub fn slide_ready(&self, slide: &Slide) -> bool {
        if !self.threaded {
            return true;
        }
        let mut media = Vec::new();
        slide_media(slide, &mut media);
        let state = self.lock();
        media.iter().all(|m| !matches!(state.pictures.get(m), Some(Picture::Queued | Picture::Decoding)))
    }

    /// Decode `slide`'s pictures before the others still waiting.
    pub fn prioritize(&self, slide: &Slide) {
        let mut media = Vec::new();
        slide_media(slide, &mut media);
        let mut state = self.lock();
        for m in media.into_iter().rev() {
            if matches!(state.pictures.get(&m), Some(Picture::Queued)) {
                state.queue.push_front(m);
            }
        }
        self.shared.changed.notify_all();
    }

    /// Changes each time a picture is decoded.
    pub fn generation(&self) -> u64 {
        self.shared.generation.load(Ordering::Acquire)
    }

    /// Whether every picture is decoded (or left to GPUI).
    pub fn finished(&self) -> bool {
        !self.threaded || self.lock().pictures.values().all(|p| matches!(p, Picture::Done(_) | Picture::Failed))
    }

    /// Wait until every picture is decoded, as for printing.
    #[cfg(any(headless_export, target_family = "wasm"))]
    pub fn wait_all(&self) {
        let mut state = self.lock();
        while self.threaded && (state.prepared_count < self.shared.media.len() || state.pictures.values().any(|p| matches!(p, Picture::Queued | Picture::Decoding))) {
            state = self.shared.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
    }
}

#[cfg(not(target_family = "wasm"))]
enum Priority {
    /// What the window waits for.
    Interactive,
    /// What is shown as soon as it is ready.
    Initiated,
}

/// Run the current thread at `priority` (macOS's quality of service, which
/// also picks the cores it runs on).
#[cfg(not(target_family = "wasm"))]
fn set_priority(priority: Priority) {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
        }
        let class = match priority {
            Priority::Interactive => 0x21, // QOS_CLASS_USER_INTERACTIVE
            Priority::Initiated => 0x19,   // QOS_CLASS_USER_INITIATED
        };
        // SAFETY: sets only the calling thread's own scheduling class.
        unsafe { pthread_set_qos_class_self_np(class, 0) };
    }
    #[cfg(not(target_os = "macos"))]
    let _ = priority;
}

/// A worker: prepares media parts while any are left, then decodes pictures
/// from the queue until none are.
#[cfg(not(target_family = "wasm"))]
fn work(shared: &Shared) {
    let renderer = gpui_kit::SvgRenderer::new(Arc::new(()));
    let lock = || shared.state.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        let (media, already_done) = {
            let mut state = lock();
            if state.next_prepare >= shared.media.len() {
                break;
            }
            let index = state.next_prepare;
            state.next_prepare += 1;
            let m = &shared.media[index];
            let done = state.prepared.contains_key(&m.id) || matches!(state.pictures.get(&m.id), Some(Picture::Failed));
            (m.clone(), done)
        };
        if already_done {
            continue;
        }
        let prepared = prepare(&media);
        let mut state = lock();
        if !state.prepared.contains_key(&media.id) && !matches!(state.pictures.get(&media.id), Some(Picture::Failed)) {
            match prepared {
                Some(prepared) => {
                    state.prepared.insert(media.id, prepared);
                }
                None => {
                    state.pictures.insert(media.id, Picture::Failed);
                }
            }
            state.prepared_count += 1;
            shared.changed.notify_all();
        }
    }
    loop {
        let (id, image) = {
            let mut state = lock();
            loop {
                // A picture can wait here to be prepared by another worker.
                let next = state.queue.iter().position(|id| match state.pictures.get(id) {
                    Some(Picture::Queued) => state.prepared.contains_key(id),
                    _ => false,
                });
                if let Some(at) = next {
                    let id = state.queue.remove(at).expect("found in the queue");
                    state.pictures.insert(id, Picture::Decoding);
                    break (id, state.prepared[&id].0.clone());
                }
                let State { queue, pictures, .. } = &mut *state;
                queue.retain(|id| matches!(pictures.get(id), Some(Picture::Queued)));
                let preparing = state.prepared_count < shared.media.len();
                if state.queue.is_empty() && !preparing {
                    return;
                }
                if !preparing {
                    // Queued media that is not a picture.
                    let rest: Vec<MediaId> = state.queue.drain(..).collect();
                    for id in rest {
                        state.pictures.insert(id, Picture::Failed);
                    }
                    shared.changed.notify_all();
                    return;
                }
                state = shared.changed.wait(state).unwrap_or_else(|e| e.into_inner());
            }
        };
        let decoded = image.to_image_data(renderer.clone());
        let mut state = lock();
        state.pictures.insert(
            id,
            match decoded {
                Ok(image) => Picture::Done(image),
                Err(err) => {
                    log::warn!("picture could not be decoded: {err}");
                    Picture::Failed
                }
            },
        );
        shared.generation.fetch_add(1, Ordering::AcqRel);
        shared.changed.notify_all();
    }
}

/// The picture GPUI draws for `media`, if it can draw one.
fn prepare(media: &Media) -> Option<Prepared> {
    let (format, bytes) = decode(media)?;
    let encoding = match format {
        ImageFormat::Png => Some(Encoding::Png),
        ImageFormat::Jpeg => Some(Encoding::Jpeg),
        ImageFormat::Gif => Some(Encoding::Gif),
        ImageFormat::Webp => Some(Encoding::Webp),
        ImageFormat::Svg => Some(Encoding::Svg),
        _ => None,
    };
    let source = encoding.map(|encoding| (encoding, Arc::from(bytes.as_slice())));
    Some((Arc::new(Image::from_bytes(format, bytes)), source))
}

/// What GPUI draws of a metafile put in after the file was opened (pasted, inserted): the SVG it draws, or the bitmap it holds.
pub(crate) fn metafile_picture(format: MediaFormat, data: &[u8]) -> Option<(ImageFormat, Vec<u8>)> {
    decode(&Media { id: MediaId(0), format, data: data.into() })
}

/// A picture GPUI can draw, as its encoding and bytes. Metafiles give the
/// bitmap they carry when they draw nothing.
fn decode(media: &Media) -> Option<(ImageFormat, Vec<u8>)> {
    match media.format {
        MediaFormat::Emf => {
            // The bitmap when that is all the metafile holds, else what it draws as SVG, else a bitmap it carries.
            match ibis_emf::emf_picture(&media.data) {
                Some(ibis_emf::Picture::Svg(svg)) => return Some((ImageFormat::Svg, svg.into_bytes())),
                Some(ibis_emf::Picture::Image(format, bytes)) => return Some((metafile_image_format(format), bytes)),
                None => {}
            }
            let (format, bytes) = ibis_emf::extract_image(&media.data)?;
            Some((metafile_image_format(format), bytes))
        }
        MediaFormat::Wmf => ibis_emf::wmf_to_svg(&media.data).map(|svg| (ImageFormat::Svg, svg.into_bytes())),
        // A CMYK (or YCCK) JPEG is turned into RGB here: wrapped as SVG (for a transparent colour, a
        // rotation, a duotone) it would be decoded as three components a pixel.
        MediaFormat::Jpeg if jpeg_components(&media.data) == Some(4) => match cmyk_jpeg_as_png(&media.data) {
            Some(png) => Some((ImageFormat::Png, png)),
            None => Some((ImageFormat::Jpeg, media.data.to_vec())),
        },
        _ => Some((image_format(media.format)?, media.data.to_vec())),
    }
}

/// How many colour components a JPEG has (from its frame header), if it has one.
fn jpeg_components(data: &[u8]) -> Option<u8> {
    let mut i = 2;
    while i + 4 <= data.len() {
        if data[i] != 0xff {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        match marker {
            0xff => i += 1,
            0xd8 | 0x01 | 0xd0..=0xd7 => i += 2,
            // A frame header (not DHT, JPG, DAC): precision, height, width, components.
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => return data.get(i + 9).copied(),
            0xda | 0xd9 => return None,
            _ => i += 2 + u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize,
        }
    }
    None
}

/// A CMYK or YCCK JPEG as an RGB PNG.
#[cfg(all(feature = "export", not(target_family = "wasm")))]
fn cmyk_jpeg_as_png(data: &[u8]) -> Option<Vec<u8>> {
    use image::ImageEncoder as _;
    // The four planes are read as they are; the profile inside the picture (whatever its connection space), else the CMYK converter in use (the chosen
    // ICC profile, or the open model), makes the colours (`render/cmyk_jpeg.rs`).
    let rgba = crate::cmyk_jpeg::decode(data, &*ibis_office_base::cmyk::converter())
        .or_else(|| image::load_from_memory_with_format(data, image::ImageFormat::Jpeg).ok().map(|img| img.to_rgba8()))?;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8).ok()?;
    Some(png)
}

#[cfg(any(not(feature = "export"), target_family = "wasm"))]
fn cmyk_jpeg_as_png(_: &[u8]) -> Option<Vec<u8>> {
    None
}

fn metafile_image_format(format: ibis_emf::ImageFormat) -> ImageFormat {
    match format {
        ibis_emf::ImageFormat::Png => ImageFormat::Png,
        ibis_emf::ImageFormat::Jpeg => ImageFormat::Jpeg,
        ibis_emf::ImageFormat::Gif => ImageFormat::Gif,
        ibis_emf::ImageFormat::Bmp => ImageFormat::Bmp,
        ibis_emf::ImageFormat::Tiff => ImageFormat::Tiff,
    }
}

fn image_format(format: MediaFormat) -> Option<ImageFormat> {
    Some(match format {
        MediaFormat::Png => ImageFormat::Png,
        MediaFormat::Jpeg => ImageFormat::Jpeg,
        MediaFormat::Gif => ImageFormat::Gif,
        MediaFormat::Bmp => ImageFormat::Bmp,
        MediaFormat::Tiff => ImageFormat::Tiff,
        MediaFormat::Webp => ImageFormat::Webp,
        MediaFormat::Svg => ImageFormat::Svg,
        _ => return None,
    })
}

/// An embedded font as TrueType, unless its licence forbids using it here.
fn decode_font(data: &[u8]) -> DecodedFont {
    // A plain font file (a Word file's embedded font, already restored from its obfuscation).
    if let Some(embedding) = ibis_eot::embedding_of_font(data) {
        if embedding == ibis_eot::Embedding::Restricted {
            return None;
        }
        return Some((ibis_eot::family_name(data)?, data.to_vec()));
    }
    match ibis_eot::header(data).map(|h| h.embedding) {
        Ok(ibis_eot::Embedding::Restricted) | Err(_) => return None,
        Ok(_) => {}
    }
    let ttf = ibis_eot::decode(data).ok()?;
    Some((ibis_eot::family_name(&ttf)?, ttf))
}

/// The media `slide` draws, in drawing order, added to `out`.
pub fn slide_media(slide: &Slide, out: &mut Vec<MediaId>) {
    fill_media(&slide.background, out);
    for shape in slide.backdrop.iter().chain(&slide.shapes).chain(&slide.notes_shapes) {
        shape_media(shape, out);
    }
}

fn shape_media(shape: &Shape, out: &mut Vec<MediaId>) {
    match &shape.kind {
        ShapeKind::Picture(picture) => out.push(picture.media),
        ShapeKind::Auto(auto) => {
            fill_media(&auto.fill, out);
            if let Some(text) = &auto.text {
                text_media(text, out);
            }
        }
        ShapeKind::TextBox(text) => text_media(text, out),
        ShapeKind::Table(table) => {
            for cell in table.rows.iter().flat_map(|row| &row.cells) {
                fill_media(&cell.fill, out);
                text_media(&cell.text, out);
            }
        }
        _ => {}
    }
}

fn fill_media(fill: &Fill, out: &mut Vec<MediaId>) {
    match fill {
        Fill::Image(media) | Fill::TileImage { media, .. } => out.push(*media),
        Fill::Blip(blip) => out.push(blip.media),
        _ => {}
    }
}

fn text_media(text: &TextBody, out: &mut Vec<MediaId>) {
    out.extend(text.paragraphs.iter().filter_map(|p| p.bullet.as_ref()?.picture));
}
