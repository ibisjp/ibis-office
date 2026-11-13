// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A file that is already open is not opened a second time.
//!
//! Each window edits its own copy of a file in memory. A second window on the same path would
//! hold a second, different copy, and saving either one overwrites the other's edits. So
//! opening a path that a window already shows (or is loading) brings that window to the front
//! instead (Finder, `open`, the Open dialog, recent files, a drop on a window).
//!
//! Only this process is checked: another instance (`open -n`) has its own windows.

use std::path::Path;

use gpui_kit::component::Root;
use gpui_kit::*;

use crate::editor::SlideEditor;

/// Whether `a` and `b` are the same file: through symbolic links, `..`, a trailing separator,
/// and (on the systems whose volumes ignore case) upper and lower case.
pub fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    if let (Ok(x), Ok(y)) = (std::fs::metadata(a), std::fs::metadata(b)) {
        use std::os::unix::fs::MetadataExt;
        // The device and inode name the file itself: a hard link, a symbolic link and a path
        // spelt in another case (a case-insensitive volume) all give the same pair.
        return x.dev() == y.dev() && x.ino() == y.ino();
    }
    let (Ok(x), Ok(y)) = (a.canonicalize(), b.canonicalize()) else { return a == b };
    if cfg!(any(windows, target_os = "macos")) {
        x.to_string_lossy().to_lowercase() == y.to_string_lossy().to_lowercase()
    } else {
        x == y
    }
}

/// The window (other than the one whose editor is `skip`) that shows or is loading `path`.
pub(crate) fn find_open_window(path: &Path, skip: Option<EntityId>, cx: &mut App) -> Option<AnyWindowHandle> {
    for window in cx.windows() {
        let Some(root) = window.downcast::<Root>() else { continue };
        // A window that is being updated right now (the caller's own) cannot be read: it is not another window.
        let editor = root.update(cx, |root, _, _| root.view().clone().downcast::<SlideEditor>().ok()).ok().flatten();
        let Some(editor) = editor else { continue };
        if Some(editor.entity_id()) == skip {
            continue;
        }
        if editor.read(cx).shows_path(path) {
            return Some(window);
        }
    }
    None
}

/// Bring `window` to the front and make it visible (a minimized window is restored by the platform's activation).
pub(crate) fn raise(window: AnyWindowHandle, cx: &mut App) {
    window.update(cx, |_, window, _| window.activate_window()).ok();
    cx.activate(true);
}

/// If a window other than `skip`'s already has `path`, raise it and say so.
pub(crate) fn raise_if_open(path: &Path, skip: Option<EntityId>, cx: &mut App) -> bool {
    match find_open_window(path, skip, cx) {
        Some(window) => {
            raise(window, cx);
            true
        }
        None => false,
    }
}

impl SlideEditor {
    /// Whether this window shows (or is loading) the file at `path`.
    pub(crate) fn shows_path(&self, path: &Path) -> bool {
        self.path.iter().chain(self.watch.opening_path.iter()).any(|own| same_file(own, path))
    }

    /// Open `path` from the user's side (the Open dialog, recent files, the start view): a file
    /// another window has is raised instead; this window's own file is raised too (the window
    /// is already in front, so nothing changes: opening it again must not drop its edits).
    pub fn open_path_or_raise(&mut self, path: std::path::PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let own = self.shows_path(&path);
        if own || raise_if_open(&path, Some(cx.entity_id()), cx) {
            if own {
                window.activate_window();
            }
            return;
        }
        self.open_path(path, None, window, cx);
    }
}
