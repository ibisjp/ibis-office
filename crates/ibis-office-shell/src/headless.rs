// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing without a window (the export, the scenarios, the pictures put on the clipboard).

use gpui_kit::*;

/// The platform's renderer drawing without a window: GPUI's Metal one, or
/// on Windows the DirectX one and on Linux the wgpu one of ibisOffice's
/// forks (GPUI has none there).
pub fn headless_renderer() -> Option<Box<dyn PlatformHeadlessRenderer>> {
    #[cfg(windows)]
    return gpui_windows::DirectXHeadlessRenderer::new()
        .map_err(|e| log::error!("DirectX: {e:#}"))
        .ok()
        .map(|renderer| Box::new(renderer) as Box<dyn PlatformHeadlessRenderer>);
    #[cfg(target_os = "linux")]
    return gpui_wgpu::WgpuHeadlessRenderer::new()
        .map_err(|e| log::error!("wgpu: {e:#}"))
        .ok()
        .map(|renderer| Box::new(renderer) as Box<dyn PlatformHeadlessRenderer>);
    #[cfg(not(any(windows, target_os = "linux")))]
    gpui_kit::platform::current_headless_renderer()
}
