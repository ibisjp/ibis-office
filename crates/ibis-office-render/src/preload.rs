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
    #[cfg(headless_export)]
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
    // A profile inside the picture (a Lab-based CMYK profile) gives its own colours; else the four planes are read as they are
    // and the CMYK converter in use (the chosen ICC profile, or the open model) makes the colours (`render/cmyk_jpeg.rs`).
    let rgba = cmyk_icc_jpeg_to_rgba(data)
        .or_else(|| crate::cmyk_jpeg::decode(data, &*ibis_office_base::cmyk::converter()))
        .or_else(|| image::load_from_memory_with_format(data, image::ImageFormat::Jpeg).ok().map(|img| img.to_rgba8()))?;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(rgba.as_raw(), rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8).ok()?;
    Some(png)
}

#[cfg(all(feature = "export", not(target_family = "wasm")))]
struct LutMft {
    is_16bit: bool,
    in_ch: usize,
    out_ch: usize,
    grid: usize,
    in_tables: Vec<Vec<f32>>,
    clut: Vec<f32>,
    out_tables: Vec<Vec<f32>>,
}

#[cfg(all(feature = "export", not(target_family = "wasm")))]
impl LutMft {
    fn parse(icc: &[u8], off: usize, size: usize) -> Option<Self> {
        if off.checked_add(size)? > icc.len() || size < 48 {
            return None;
        }
        let tag = &icc[off..off + size];
        let is_16bit = match &tag[0..4] {
            b"mft1" => false,
            b"mft2" => true,
            _ => return None,
        };
        let in_ch = tag[8] as usize;
        let out_ch = tag[9] as usize;
        let grid = tag[10] as usize;
        if in_ch == 0 || out_ch == 0 || grid < 2 {
            return None;
        }
        let mut grid_pts = 1usize;
        for _ in 0..in_ch {
            grid_pts = grid_pts.checked_mul(grid)?;
        }
        let clut_len = grid_pts.checked_mul(out_ch)?;
        if !is_16bit {
            let in_end = 48usize.checked_add(in_ch * 256)?;
            let clut_end = in_end.checked_add(clut_len)?;
            let out_end = clut_end.checked_add(out_ch * 256)?;
            if out_end > tag.len() {
                return None;
            }
            let in_tables = (0..in_ch)
                .map(|c| tag[48 + c * 256..48 + (c + 1) * 256].iter().map(|&b| b as f32).collect())
                .collect();
            let clut = tag[in_end..clut_end].iter().map(|&b| b as f32).collect();
            let out_tables = (0..out_ch)
                .map(|c| tag[clut_end + c * 256..clut_end + (c + 1) * 256].iter().map(|&b| b as f32).collect())
                .collect();
            Some(Self { is_16bit, in_ch, out_ch, grid, in_tables, clut, out_tables })
        } else {
            if tag.len() < 52 {
                return None;
            }
            let in_entries = u16::from_be_bytes([tag[48], tag[49]]) as usize;
            let out_entries = u16::from_be_bytes([tag[50], tag[51]]) as usize;
            if in_entries < 2 || out_entries < 2 {
                return None;
            }
            let in_bytes = in_ch.checked_mul(in_entries)?.checked_mul(2)?;
            let clut_bytes = clut_len.checked_mul(2)?;
            let out_bytes = out_ch.checked_mul(out_entries)?.checked_mul(2)?;
            let in_end = 52usize.checked_add(in_bytes)?;
            let clut_end = in_end.checked_add(clut_bytes)?;
            let out_end = clut_end.checked_add(out_bytes)?;
            if out_end > tag.len() {
                return None;
            }
            let read_u16_scaled = |slice: &[u8]| -> Vec<f32> {
                slice.chunks_exact(2).map(|b| u16::from_be_bytes([b[0], b[1]]) as f32 * (255.0 / 65535.0)).collect()
            };
            let in_tables = (0..in_ch)
                .map(|c| read_u16_scaled(&tag[52 + c * in_entries * 2..52 + (c + 1) * in_entries * 2]))
                .collect();
            let clut = read_u16_scaled(&tag[in_end..clut_end]);
            let out_tables = (0..out_ch)
                .map(|c| read_u16_scaled(&tag[clut_end + c * out_entries * 2..clut_end + (c + 1) * out_entries * 2]))
                .collect();
            Some(Self { is_16bit, in_ch, out_ch, grid, in_tables, clut, out_tables })
        }
    }

    fn eval(&self, inputs: &[f32], out: &mut [f32]) {
        let _ = self.is_16bit;
        let sample_table = |tbl: &[f32], x: f32| -> f32 {
            let n = (tbl.len() - 1) as f32;
            let pos = (x.clamp(0.0, 255.0) / 255.0) * n;
            let idx = (pos.floor() as usize).min(tbl.len().saturating_sub(2));
            let frac = pos - idx as f32;
            tbl[idx] * (1.0 - frac) + tbl[idx + 1] * frac
        };
        let mut coords = [0usize; 4];
        let mut fracs = [0.0f32; 4];
        let g_max = (self.grid - 1) as f32;
        for c in 0..self.in_ch.min(4) {
            let v = sample_table(&self.in_tables[c], inputs[c]);
            let pos = (v.clamp(0.0, 255.0) / 255.0) * g_max;
            let idx = (pos.floor() as usize).min(self.grid - 2);
            coords[c] = idx;
            fracs[c] = pos - idx as f32;
        }
        for v in out.iter_mut().take(self.out_ch) {
            *v = 0.0;
        }
        let corners = 1usize << self.in_ch;
        for mask in 0..corners {
            let mut w = 1.0f32;
            let mut flat = 0usize;
            for d in 0..self.in_ch {
                let bit = (mask >> (self.in_ch - 1 - d)) & 1;
                w *= if bit == 1 { fracs[d] } else { 1.0 - fracs[d] };
                flat = flat * self.grid + coords[d] + bit;
            }
            let base = flat * self.out_ch;
            for o in 0..self.out_ch {
                out[o] += w * self.clut[base + o];
            }
        }
        for o in 0..self.out_ch {
            out[o] = sample_table(&self.out_tables[o], out[o]);
        }
    }
}

/// Decode a 4-component CMYK/YCCK JPEG with an embedded CMYK->Lab ICC profile (`APP2` `ICC_PROFILE\0`).
#[cfg(all(feature = "export", not(target_family = "wasm")))]
fn cmyk_icc_jpeg_to_rgba(data: &[u8]) -> Option<image::RgbaImage> {
    let mut icc_chunks: Vec<(u8, &[u8])> = Vec::new();
    let mut app14_transform: Option<u8> = None;
    let mut sof_offset: Option<usize> = None;
    let mut sos_offset: Option<usize> = None;
    let mut i = 2usize;
    while i + 4 <= data.len() {
        if data[i] != 0xff {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        match marker {
            0xff => i += 1,
            0xd8 | 0x01 | 0xd0..=0xd7 => i += 2,
            0xda => {
                sos_offset = Some(i);
                break;
            }
            0xd9 => break,
            _ => {
                let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
                if seg_len < 2 || i + 2 + seg_len > data.len() {
                    break;
                }
                let payload = &data[i + 4..i + 2 + seg_len];
                if marker == 0xe2 && payload.len() > 14 && &payload[..12] == b"ICC_PROFILE\0" {
                    icc_chunks.push((payload[12], &payload[14..]));
                } else if marker == 0xee && payload.len() >= 12 && &payload[..5] == b"Adobe" {
                    app14_transform = Some(payload[11]);
                } else if matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
                    sof_offset = Some(i);
                }
                i += 2 + seg_len;
            }
        }
    }
    if icc_chunks.is_empty() {
        return None;
    }
    icc_chunks.sort_by_key(|(seq, _)| *seq);
    let mut icc = Vec::new();
    for (_, bytes) in icc_chunks {
        icc.extend_from_slice(bytes);
    }
    if icc.len() < 132 || &icc[16..20] != b"CMYK" || &icc[20..24] != b"Lab " {
        return None;
    }
    let tag_count = u32::from_be_bytes([icc[128], icc[129], icc[130], icc[131]]) as usize;
    if 132 + tag_count * 12 > icc.len() {
        return None;
    }
    let mut a2b0: Option<LutMft> = None;
    let mut b2a0: Option<LutMft> = None;
    for t in 0..tag_count {
        let base = 132 + t * 12;
        let sig = &icc[base..base + 4];
        let off = u32::from_be_bytes([icc[base + 4], icc[base + 5], icc[base + 6], icc[base + 7]]) as usize;
        let sz = u32::from_be_bytes([icc[base + 8], icc[base + 9], icc[base + 10], icc[base + 11]]) as usize;
        if sig == b"A2B0" && a2b0.is_none() {
            a2b0 = LutMft::parse(&icc, off, sz).filter(|l| l.in_ch == 4 && l.out_ch == 3);
        } else if sig == b"B2A0" && b2a0.is_none() {
            b2a0 = LutMft::parse(&icc, off, sz).filter(|l| l.in_ch == 3 && l.out_ch == 4);
        }
    }
    let a2b0 = a2b0?;
    let sof_pos = sof_offset?;
    let sos_pos = sos_offset?;
    // SOF header: FF C0 [len:2] [P:1] [Y:2] [X:2] [Nf:1] [comp0:3] [comp1:3] [comp2:3] [comp3:3]
    if sof_pos + 10 + 12 > sos_pos || data[sof_pos + 9] != 4 {
        return None;
    }
    // Build Pass 1 JPEG: insert APP14 with transform = 1 (YCbCr) right before SOS so the JPEG
    // decoder outputs (comp0, comp1, comp2) after YCbCr->RGB conversion (for YCCK), or if APP14
    // transform == 0 (untransformed CMYK), zero out each component individually.
    let app14_ycbcr: [u8; 16] = [
        0xff, 0xee, 0x00, 0x0e, b'A', b'd', b'o', b'b', b'e', 0x00, 0x64, 0x00, 0x00, 0x00, 0x00, 0x01,
    ];
    let zero_dqt3: [u8; 69] = {
        let mut d = [0u8; 69];
        d[0] = 0xff;
        d[1] = 0xdb;
        d[2] = 0x00;
        d[3] = 0x43;
        d[4] = 0x03;
        d
    };
    let decode_single_comp = |comp_idx: usize| -> Option<image::RgbImage> {
        let mut buf = Vec::with_capacity(data.len() + 85);
        buf.extend_from_slice(&data[..sos_pos]);
        let comp_base = sof_pos + 10;
        let orig = data[comp_base..comp_base + 12].to_vec();
        if comp_idx != 0 {
            buf[comp_base..comp_base + 3].copy_from_slice(&orig[comp_idx * 3..comp_idx * 3 + 3]);
            buf[comp_base + comp_idx * 3..comp_base + comp_idx * 3 + 3].copy_from_slice(&orig[0..3]);
        }
        // Point components 1 and 2 at quantization table 3 (all zeros -> IDCT outputs 128 -> YCbCr->RGB outputs comp_idx in R).
        buf[comp_base + 3 + 2] = 3;
        buf[comp_base + 6 + 2] = 3;
        buf.extend_from_slice(&zero_dqt3);
        buf.extend_from_slice(&app14_ycbcr);
        buf.extend_from_slice(&data[sos_pos..]);
        Some(image::load_from_memory_with_format(&buf, image::ImageFormat::Jpeg).ok()?.to_rgb8())
    };
    let (cmy_rgb, is_ycck) = if app14_transform == Some(0) {
        let c0 = decode_single_comp(0)?;
        let c1 = decode_single_comp(1)?;
        let c2 = decode_single_comp(2)?;
        let (w, h) = (c0.width(), c0.height());
        let mut combined = image::RgbImage::new(w, h);
        for (p, ((p0, p1), p2)) in combined.pixels_mut().zip(c0.pixels().zip(c1.pixels()).zip(c2.pixels())) {
            *p = image::Rgb([p0.0[0], p1.0[0], p2.0[0]]);
        }
        (combined, false)
    } else {
        let mut buf = Vec::with_capacity(data.len() + app14_ycbcr.len());
        buf.extend_from_slice(&data[..sos_pos]);
        buf.extend_from_slice(&app14_ycbcr);
        buf.extend_from_slice(&data[sos_pos..]);
        (image::load_from_memory_with_format(&buf, image::ImageFormat::Jpeg).ok()?.to_rgb8(), true)
    };
    let k_img = decode_single_comp(3)?;
    let (w, h) = (cmy_rgb.width(), cmy_rgb.height());
    if k_img.width() != w || k_img.height() != h {
        return None;
    }

    let mut cache: std::collections::HashMap<[u8; 4], [u8; 3]> = std::collections::HashMap::new();
    let mut out = image::RgbaImage::new(w, h);
    for ((out_px, cmy_px), k_px) in out.pixels_mut().zip(cmy_rgb.pixels()).zip(k_img.pixels()) {
        let (c, m, y) = if is_ycck {
            (cmy_px.0[0], cmy_px.0[1], cmy_px.0[2])
        } else {
            (255 - cmy_px.0[0], 255 - cmy_px.0[1], 255 - cmy_px.0[2])
        };
        let k = 255 - k_px.0[0];
        let rgb = *cache.entry([c, m, y, k]).or_insert_with(|| {
            let cmyk_f = [c as f32, m as f32, y as f32, k as f32];
            let mut u = [0.0f32; 3];
            a2b0.eval(&cmyk_f, &mut u);
            if let Some(ref b2a) = b2a0 {
                let u0 = u;
                let mut best_u = u0;
                let mut tmp = [0.0f32; 4];
                let cost = |cand: &[f32; 3], tmp: &mut [f32; 4]| -> f32 {
                    b2a.eval(cand, tmp);
                    let err: f32 = (0..4).map(|i| (tmp[i] - cmyk_f[i]).powi(2)).sum();
                    let reg: f32 = (0..3).map(|i| (cand[i] - u0[i]).powi(2)).sum();
                    err + 0.02 * reg
                };
                let mut best_cost = cost(&best_u, &mut tmp);
                for step in [8.0f32, 4.0, 2.0, 1.0] {
                    for _ in 0..6 {
                        let mut improved = false;
                        for d in 0..3 {
                            for dir in [-1.0f32, 1.0] {
                                let mut cand = best_u;
                                cand[d] = (cand[d] + dir * step).clamp(0.0, 255.0);
                                let c_val = cost(&cand, &mut tmp);
                                if c_val + 1e-4 < best_cost {
                                    best_cost = c_val;
                                    best_u = cand;
                                    improved = true;
                                }
                            }
                        }
                        if !improved {
                            break;
                        }
                    }
                }
                u = best_u;
            }
            let l_star = u[0] / 2.55;
            let a_star = u[1] - 128.0;
            let b_star = u[2] - 128.0;
            let fy = (l_star + 16.0) / 116.0;
            let fx = fy + a_star / 500.0;
            let fz = fy - b_star / 200.0;
            let finv = |t: f32| {
                let delta = 6.0 / 29.0;
                if t > delta { t * t * t } else { 3.0 * delta * delta * (t - 4.0 / 29.0) }
            };
            let x50 = 0.9642 * finv(fx);
            let y50 = 1.0000 * finv(fy);
            let z50 = 0.8249 * finv(fz);
            let rl = 3.1338561 * x50 - 1.6168667 * y50 - 0.4906146 * z50;
            let gl = -0.9787684 * x50 + 1.9161415 * y50 + 0.0334540 * z50;
            let bl = 0.0719453 * x50 - 0.2289914 * y50 + 1.4052427 * z50;
            let to_srgb = |v: f32| -> u8 {
                let v = v.clamp(0.0, 1.0);
                let s = if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
                (s * 255.0).round().clamp(0.0, 255.0) as u8
            };
            [to_srgb(rl), to_srgb(gl), to_srgb(bl)]
        });
        *out_px = image::Rgba([rgb[0], rgb[1], rgb[2], 255]);
    }
    Some(out)
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
