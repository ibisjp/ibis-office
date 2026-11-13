// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Arranging the open windows on the screen (ウィンドウ > 整列, 重ねて表示,
//! 左右に並べて表示, 上下に並べて表示, and the workbook's ウィンドウの整列 panel):
//! tiled, in rows, in columns or cascaded (`ArrangeKind`). The windows of every
//! format are arranged, so this is the window's, not a format's.

use std::path::PathBuf;

use gpui_kit::*;

use crate::arrange_kind::ArrangeKind;
use crate::editor::SlideEditor;

/// The file a window shows, to tell the other windows of the same file from the windows of other files (ウィンドウの
/// 整列's 作業中のブックのウィンドウを整列する): the kind of file, its path, and the name the window goes by while the
/// file has no path.
pub struct WindowFile {
    kind: ibis_pptx::Kind,
    path: Option<PathBuf>,
    name: String,
}

impl SlideEditor {
    /// The file this window shows.
    pub fn window_file(&self) -> WindowFile {
        WindowFile { kind: self.format.kind(), path: self.file_path(), name: self.workbook_display_name() }
    }

    /// This window shows `file`: a file of the same kind, at the same path when both have one, of the same name
    /// otherwise.
    fn shows_file(&self, file: &WindowFile) -> bool {
        self.format.kind() == file.kind
            && match (&file.path, self.file_path()) {
                (Some(a), Some(b)) => *a == b,
                _ => file.name == self.workbook_display_name(),
            }
    }

    /// Arrange the open windows; with `only`, the windows of that file alone.
    pub fn arrange_windows_impl(
        kind: ArrangeKind,
        current: Option<(&mut Window, AnyWindowHandle)>,
        only: Option<WindowFile>,
        cx: &mut App,
    ) {
        let all_windows = cx.windows();
        if all_windows.is_empty() {
            return;
        }

        let current_h = current.as_ref().map(|(_, h)| *h);
        let mut targets = Vec::new();
        for &w in &all_windows {
            if current_h.is_some_and(|h| w == h) {
                targets.push(w);
            } else if let Some(root) = w.downcast::<gpui_kit::component::Root>() {
                let is_target = root
                    .update(cx, |root, _, app| {
                        if let Ok(editor_entity) = root.view().clone().downcast::<SlideEditor>() {
                            let ed = editor_entity.read(app);
                            only.as_ref().is_none_or(|file| ed.shows_file(file))
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if is_target {
                    targets.push(w);
                }
            }
        }

        if targets.is_empty() {
            if let Some(h) = current_h {
                targets.push(h);
            } else {
                targets = all_windows.clone();
            }
        }

        let count = targets.len().max(1);
        let (screen_w, screen_h) = cx
            .primary_display()
            .map(|d| {
                let b = d.visible_bounds();
                (f32::from(b.size.width), f32::from(b.size.height))
            })
            .unwrap_or((1280.0, 800.0));

        let (cols, rows, tile_w, tile_h) = match kind {
            ArrangeKind::Tiled => {
                let (cols, rows) = match count {
                    1 => (1, 1),
                    2 => (2, 1),
                    3 | 4 => (2, 2),
                    5 | 6 => (3, 2),
                    _ => {
                        let cols = (count as f32).sqrt().ceil() as usize;
                        let rows = ((count as f32) / (cols as f32)).ceil() as usize;
                        (cols, rows)
                    }
                };
                (cols, rows, screen_w / cols as f32, screen_h / rows as f32)
            }
            ArrangeKind::Horizontal => {
                let cols = 1;
                let rows = count;
                (cols, rows, screen_w / cols as f32, screen_h / rows as f32)
            }
            ArrangeKind::Vertical => {
                let cols = count;
                let rows = 1;
                (cols, rows, screen_w / cols as f32, screen_h / rows as f32)
            }
            ArrangeKind::Cascade => {
                let base_w = (screen_w * 0.8).clamp(640.0, screen_w);
                let base_h = (screen_h * 0.8).clamp(480.0, screen_h);
                (1, 1, base_w, base_h)
            }
        };
        let _ = rows;

        let (mut cur_window, cur_handle) = match current {
            Some((w, h)) => (Some(w), Some(h)),
            None => (None, None),
        };

        let mut macos_frames: Vec<(usize, f32, f32, f32, f32)> = Vec::with_capacity(targets.len());
        #[cfg(target_os = "macos")]
        let native_frames = macos_has_native_windows();
        #[cfg(not(target_os = "macos"))]
        let native_frames = false;

        for (idx, &target) in targets.iter().enumerate() {
            let (rel_x, rel_y) = match kind {
                ArrangeKind::Tiled => {
                    let col = idx % cols.max(1);
                    let row = idx / cols.max(1);
                    (col as f32 * tile_w, row as f32 * tile_h)
                }
                ArrangeKind::Horizontal => (0.0, idx as f32 * tile_h),
                ArrangeKind::Vertical => (idx as f32 * tile_w, 0.0),
                ArrangeKind::Cascade => {
                    let step = 28.0;
                    let max_x = (screen_w - tile_w).max(0.0);
                    let max_y = (screen_h - tile_h).max(0.0);
                    ((idx as f32 * step).min(max_x), (idx as f32 * step).min(max_y))
                }
            };
            if let Some(ord_idx) = all_windows.iter().position(|w| *w == target) {
                macos_frames.push((ord_idx, rel_x, rel_y, tile_w, tile_h));
            }
            // On macOS the real windows are placed by their NSWindow frames below; a window that has no NSWindow
            // (the headless windows of the scenarios) is resized through GPUI.
            if !native_frames {
                if let (Some(w), Some(h)) = (cur_window.as_mut(), cur_handle) {
                    if target == h {
                        w.resize(size(px(tile_w), px(tile_h)));
                        w.bounds_changed(cx);
                        continue;
                    }
                }
                target
                    .update(cx, |_, window, cx| {
                        window.resize(size(px(tile_w), px(tile_h)));
                        window.bounds_changed(cx);
                    })
                    .ok();
            }
        }

        #[cfg(target_os = "macos")]
        if native_frames && !macos_frames.is_empty() {
            cx.foreground_executor()
                .spawn(async move {
                    set_macos_window_frames(&macos_frames);
                    redisplay_macos_windows(&macos_frames);
                })
                .detach();
        }
    }
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
fn redisplay_macos_windows(frames: &[(usize, f32, f32, f32, f32)]) {
    use objc::runtime::{Class, Object, BOOL, YES};
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let Some(gpui_cls) = Class::get("GPUIWindow") else { return };
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let ordered: *mut Object = msg_send![app, orderedWindows];
        if ordered.is_null() {
            return;
        }
        let count: usize = msg_send![ordered, count];
        let mut gpui_windows: Vec<*mut Object> = Vec::new();
        for i in 0..count {
            let win: *mut Object = msg_send![ordered, objectAtIndex: i];
            if !win.is_null() {
                let is_gpui: BOOL = msg_send![win, isKindOfClass: gpui_cls];
                if is_gpui == YES {
                    gpui_windows.push(win);
                }
            }
        }
        for &(ord_idx, _, _, _, _) in frames {
            if let Some(&win) = gpui_windows.get(ord_idx) {
                let content_view: *mut Object = msg_send![win, contentView];
                if !content_view.is_null() {
                    let _: () = msg_send![content_view, setNeedsDisplay: YES];
                    let subviews: *mut Object = msg_send![content_view, subviews];
                    if !subviews.is_null() {
                        let sub_count: usize = msg_send![subviews, count];
                        for j in 0..sub_count {
                            let sub: *mut Object = msg_send![subviews, objectAtIndex: j];
                            if !sub.is_null() {
                                let _: () = msg_send![sub, setNeedsDisplay: YES];
                            }
                        }
                    }
                }
                let _: () = msg_send![win, displayIfNeeded];
            }
        }
    }
}

/// Whether this process has an NSWindow of GPUI's (false in the scenarios' headless windows).
#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
fn macos_has_native_windows() -> bool {
    use objc::runtime::{Class, Object, BOOL, YES};
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let Some(gpui_cls) = Class::get("GPUIWindow") else { return false };
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return false;
        }
        let ordered: *mut Object = msg_send![app, orderedWindows];
        if ordered.is_null() {
            return false;
        }
        let count: usize = msg_send![ordered, count];
        (0..count).any(|i| {
            let win: *mut Object = msg_send![ordered, objectAtIndex: i];
            if win.is_null() {
                return false;
            }
            let is_gpui: BOOL = msg_send![win, isKindOfClass: gpui_cls];
            is_gpui == YES
        })
    }
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
fn set_macos_window_frames(frames: &[(usize, f32, f32, f32, f32)]) {
    use objc::runtime::{Class, Object, BOOL, YES};
    use objc::{class, msg_send, sel, sel_impl};

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSSize {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSRect {
        origin: NSPoint,
        size: NSSize,
    }

    unsafe {
        let Some(gpui_cls) = Class::get("GPUIWindow") else { return };
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let ordered: *mut Object = msg_send![app, orderedWindows];
        if ordered.is_null() {
            return;
        }
        let count: usize = msg_send![ordered, count];
        let mut gpui_windows: Vec<*mut Object> = Vec::new();
        for i in 0..count {
            let win: *mut Object = msg_send![ordered, objectAtIndex: i];
            if !win.is_null() {
                let is_gpui: BOOL = msg_send![win, isKindOfClass: gpui_cls];
                if is_gpui == YES {
                    gpui_windows.push(win);
                }
            }
        }
        let main_screen: *mut Object = msg_send![class!(NSScreen), mainScreen];
        let vis: NSRect = if !main_screen.is_null() {
            msg_send![main_screen, visibleFrame]
        } else {
            NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize { width: 1280.0, height: 800.0 },
            }
        };
        for &(ord_idx, rel_x, rel_y, w, h) in frames {
            if let Some(&win) = gpui_windows.get(ord_idx) {
                let screen: *mut Object = msg_send![win, screen];
                let sv: NSRect = if !screen.is_null() {
                    msg_send![screen, visibleFrame]
                } else {
                    vis
                };
                let w64 = w as f64;
                let h64 = h as f64;
                let x64 = sv.origin.x + rel_x as f64;
                let y64 = sv.origin.y + sv.size.height - rel_y as f64 - h64;
                let rect = NSRect {
                    origin: NSPoint { x: x64, y: y64 },
                    size: NSSize { width: w64, height: h64 },
                };
                let _: () = msg_send![win, setFrame: rect display: YES];
            }
        }
    }
}
