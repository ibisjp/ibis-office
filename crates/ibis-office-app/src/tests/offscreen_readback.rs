// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The wgpu renderer's side of `Window::read_back_offscreen` (the fork's `read_back_pass`). On macOS the app draws with Metal, so this is the only place
//! the wgpu renderer (Linux, Android, iOS, the browser) runs on the Mac.

use gpui_kit::{BorderStyle, Bounds, ContentMask, Corners, DevicePixels, Edges, Hsla, OffscreenId, PlatformHeadlessRenderer as _, Quad, ScaledPixels, Scene, point, size, solid_background};

/// `Window::read_back_offscreen`'s renderer side (`gpui_wgpu`'s `read_back_pass`): a pass of 40x24 pixels with a red quad over its left half comes back as
/// straight (not premultiplied) opaque red there and transparent beside it, read through the future that does not block. Needs a graphics device.
#[test]
fn offscreen_pass_reads_back_without_blocking() {
    use std::future::Future;
    let mut renderer = match gpui_wgpu::WgpuHeadlessRenderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            eprintln!("no graphics device: skipped ({error:#})");
            return;
        }
    };
    let mut scene = Scene::default();
    let id = OffscreenId(7);
    let origin = point(ScaledPixels(10.0), ScaledPixels(10.0));
    scene.begin_offscreen(id, size(DevicePixels(40), DevicePixels(24)), origin);
    // Primitives are given in window coordinates: the pass's origin (10, 10) is its top left.
    let moved = Bounds { origin, size: size(ScaledPixels(20.0), ScaledPixels(24.0)) };
    scene.insert_primitive(Quad {
        order: 0,
        border_style: BorderStyle::default(),
        bounds: moved,
        content_mask: ContentMask { bounds: moved },
        background: solid_background(Hsla { h: 0.0, s: 1.0, l: 0.5, a: 1.0 }),
        border_color: Hsla::default(),
        corner_radii: Corners::default(),
        border_widths: Edges::default(),
    });
    scene.end_offscreen();
    let readback = scene.request_offscreen_readback(id);
    scene.finish();
    renderer.render_scene(&scene, size(DevicePixels(128), DevicePixels(128))).expect("drawing");
    let mut readback = Box::pin(readback);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let std::task::Poll::Ready(Some(pixels)) = readback.as_mut().poll(&mut cx) else {
        panic!("the pass was not read back");
    };
    assert_eq!((pixels.width, pixels.height), (40, 24));
    let at = |x: usize, y: usize| pixels.rgba[(y * 40 + x) * 4..(y * 40 + x) * 4 + 4].to_vec();
    assert_eq!(at(5, 5), vec![255, 0, 0, 255], "the quad is opaque red");
    assert_eq!(at(30, 5)[3], 0, "beside the quad it is transparent");
    assert_eq!(at(30, 20)[3], 0);
}
