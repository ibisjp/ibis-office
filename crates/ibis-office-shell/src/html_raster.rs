// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The pictures of the Web page's charts and SmartArt in the browser version (GitHub issue #208): the desktop's `--export html` draws each
//! such shape alone on a slide of its size in a headless window and keeps the PNG; the browser's headless window has no renderer, so the
//! shape is drawn in the editor's own window instead, into an offscreen texture (`Window::paint_offscreen`: nothing reaches the screen), whose
//! pixels are read back without waiting (`Window::read_back_offscreen`) and written as PNG.

use futures::channel::oneshot;
use gpui_kit::*;
use ibis_doc_model::{EmuRect, Shape};

use crate::editor::SlideEditor;

/// How many pictures are drawn in one frame (each is a texture of the window's size).
const PER_FRAME: usize = 4;

/// One picture to make: the shape (alone, at the origin of a page of its size) with the shapes attached to it (SmartArt members, drawn over it, their
/// frames moved by the shape's corner), `width` device pixels wide at most.
pub struct RasterJob {
    pub shape: Shape,
    pub members: Vec<Shape>,
    pub width: u32,
    done: oneshot::Sender<Option<Vec<u8>>>,
}

/// The jobs waiting for a frame and how many are being drawn.
#[derive(Default)]
pub struct HtmlRasters {
    waiting: Vec<RasterJob>,
    inflight: usize,
}

/// Device pixels wide the picture of `shape` is: 144 dpi (twice the points), 64 to 2400 (the desktop's `html_export::width_for`).
pub fn width_for(shape: &Shape) -> u32 {
    ((shape.frame.width as f64 / 12_700.0 * 2.0).round() as u32).clamp(64, 2400)
}

impl SlideEditor {
    /// Ask for the pictures of `wanted` (a shape and the shapes attached to it each): they are drawn in the next frames. The answers come in
    /// the order asked, a PNG or `None` where the shape could not be drawn (the picture was not painted).
    pub fn rasterize_shapes(&mut self, wanted: Vec<(Shape, Vec<Shape>)>, cx: &mut Context<Self>) -> Vec<oneshot::Receiver<Option<Vec<u8>>>> {
        let mut answers = Vec::new();
        for (shape, members) in wanted {
            let (done, answer) = oneshot::channel();
            let width = width_for(&shape);
            self.html_rasters.waiting.push(RasterJob { shape, members, width, done });
            answers.push(answer);
        }
        cx.notify();
        answers
    }

    /// The page `build` makes (it takes the function that gives the picture of a shape and the shapes attached to it) with the pictures of its charts and
    /// SmartArt, then `finish` with the page. The first run of `build` only lists what it asks for (its page is the one without pictures: what `finish` gets where
    /// there is nothing to draw, or no frame came within 20 seconds, a hidden tab); the second has them.
    pub fn html_with_pictures(
        &mut self,
        build: impl Fn(&dyn Fn(&Shape, &[Shape]) -> Option<Vec<u8>>) -> String + 'static,
        finish: impl FnOnce(&mut Self, String, &mut Window, &mut Context<Self>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let asked = std::cell::RefCell::new(Vec::new());
        let plain = build(&|shape, members| {
            asked.borrow_mut().push((shape.clone(), members.to_vec()));
            None
        });
        let asked = asked.into_inner();
        if asked.is_empty() {
            return finish(self, plain, window, cx);
        }
        let keys: Vec<String> = asked.iter().map(|(shape, _)| picture_key(shape)).collect();
        let answers = self.rasterize_shapes(asked, cx);
        cx.spawn_in(window, async move |this, cx| {
            let waiting = Box::pin(futures::future::join_all(answers));
            let limit = Box::pin(cx.background_executor().timer(std::time::Duration::from_secs(20)));
            let pictures: Vec<Option<Vec<u8>>> = match futures::future::select(waiting, limit).await {
                futures::future::Either::Left((made, _)) => made.into_iter().map(|a| a.ok().flatten()).collect(),
                futures::future::Either::Right(_) => vec![None; keys.len()],
            };
            let pool = std::cell::RefCell::new(keys.into_iter().zip(pictures).collect::<Vec<(String, Option<Vec<u8>>)>>());
            let html = build(&|shape, _| {
                let key = picture_key(shape);
                let mut pool = pool.borrow_mut();
                let at = pool.iter().position(|(k, _)| *k == key)?;
                pool.remove(at).1
            });
            this.update_in(cx, |this, window, cx| finish(this, html, window, cx)).ok();
        })
        .detach();
    }

    /// The elements that draw the jobs of this frame (none while the window shows nothing of them: they paint into textures only).
    pub(crate) fn html_raster_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.html_rasters.waiting.is_empty() || self.html_rasters.inflight > 0 {
            return Vec::new();
        }
        let take = PER_FRAME.min(self.html_rasters.waiting.len());
        let jobs: Vec<RasterJob> = self.html_rasters.waiting.drain(..take).collect();
        self.html_rasters.inflight = jobs.len();
        let dpr = window.scale_factor().max(0.1);
        let viewport = window.viewport_size();
        let entity = cx.entity();
        let mut out = Vec::new();
        for (n, job) in jobs.into_iter().enumerate() {
            let RasterJob { shape, members, width, done } = job;
            let (w_emu, h_emu) = (shape.frame.width.max(12_700), shape.frame.height.max(12_700));
            // Logical pixels: `width` device pixels, within the window (the texture is cut to its drawable).
            let mut logical_w = width as f32 / dpr;
            let mut logical_h = logical_w * h_emu as f32 / w_emu as f32;
            let fit = (f32::from(viewport.width) / logical_w).min(f32::from(viewport.height) / logical_h).min(1.0);
            logical_w *= fit;
            logical_h *= fit;
            let scale = logical_w / w_emu as f32;
            let mut alone = shape.clone();
            alone.rotation = 0;
            alone.floating = None;
            let frame = EmuRect::new(0, 0, w_emu, h_emu);
            let mut page = div().relative().w(px(logical_w)).h(px(logical_h)).overflow_hidden().bg(rgb(0xffffff)).child(crate::render::shape_view::shape_element(&alone, frame, scale, &self.resources, None));
            for member in &members {
                let moved = EmuRect::new(member.frame.x - shape.frame.x, member.frame.y - shape.frame.y, member.frame.width, member.frame.height);
                page = page.child(crate::render::shape_view::shape_element(member, moved, scale, &self.resources, None));
            }
            let (pixel_w, pixel_h) = ((logical_w * dpr).round() as u32, (logical_h * dpr).round() as u32);
            let id = OffscreenId(0x6877_0000 + n as u32);
            let entity = entity.clone();
            out.push(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .w(px(logical_w))
                    .h(px(logical_h))
                    .child(offscreen(id, page).on_read_back(move |readback, _, cx| {
                        cx.spawn(async move |cx| {
                            let png = match readback.await {
                                Some(pixels) if pixels.width > 0 && pixels.height > 0 => encode_png(&pixels, pixel_w, pixel_h),
                                _ => None,
                            };
                            let _ = done.send(png);
                            let _ = entity.update(cx, |this, cx| {
                                this.html_rasters.inflight = this.html_rasters.inflight.saturating_sub(1);
                                // The next jobs, in the next frame.
                                cx.notify();
                            });
                        })
                        .detach();
                    }))
                    .into_any_element(),
            );
        }
        out
    }
}

/// The pixels as a PNG (straight RGBA; the page is opaque white under them).
fn encode_png(pixels: &OffscreenPixels, _width: u32, _height: u32) -> Option<Vec<u8>> {
    use image::ImageEncoder as _;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(&pixels.rgba, pixels.width, pixels.height, image::ExtendedColorType::Rgba8).ok()?;
    Some(png)
}

/// What tells a shape from the others of the page when its picture is looked up (the two runs ask in the same order; this guards a difference).
fn picture_key(shape: &Shape) -> String {
    format!("{:?}:{}:{}:{}:{}", shape.id, shape.frame.x, shape.frame.y, shape.frame.width, shape.frame.height)
}
