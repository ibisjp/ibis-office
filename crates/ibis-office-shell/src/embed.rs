// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The browser version embedded in another page (a cloud service's preview):
//! the 閉じる and ダウンロード buttons at the top right, shown only when the
//! page asks (`www/main.ts`: `?close=1&download=1` or `window.ibisOfficeOptions`,
//! see `publish/doc/web-embedding.md`). A press hands the embedding page an
//! event (`www/main.ts`: `ibisOfficeEmbedAction`), which does the closing or
//! the saving itself. The desktop apps have neither button.

use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_arch = "wasm32")]
use gpui_kit::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use crate::editor::SlideEditor;

static CLOSE: AtomicBool = AtomicBool::new(false);
static DOWNLOAD: AtomicBool = AtomicBool::new(false);
static VIEW_ONLY: AtomicBool = AtomicBool::new(false);
static SHOW_MENU_BAR: AtomicBool = AtomicBool::new(true);
static SHOW_TOOL_BAR: AtomicBool = AtomicBool::new(true);

/// Whether the page shows the close button and the download button (both off
/// by default), whether it runs in read-only viewing mode (`?view=1`), and
/// whether the menu bar and toolbar are shown (`?menuBar=1`, `?toolBar=1`;
/// both default to on, including when `view=1`; pass `?menuBar=0` / `?toolBar=0` to hide). Called by
/// `www/main.ts` before `run`.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn set_embed_options(close: bool, download: bool, view: bool, menu_bar: bool, tool_bar: bool) {
    CLOSE.store(close, Ordering::Relaxed);
    DOWNLOAD.store(download, Ordering::Relaxed);
    VIEW_ONLY.store(view, Ordering::Relaxed);
    SHOW_MENU_BAR.store(menu_bar, Ordering::Relaxed);
    SHOW_TOOL_BAR.store(tool_bar, Ordering::Relaxed);
}

#[allow(dead_code)]
pub(crate) fn show_close() -> bool {
    CLOSE.load(Ordering::Relaxed)
}

#[allow(dead_code)]
pub(crate) fn show_download() -> bool {
    DOWNLOAD.load(Ordering::Relaxed)
}

#[allow(dead_code)]
pub fn is_view_only() -> bool {
    VIEW_ONLY.load(Ordering::Relaxed)
}

#[allow(dead_code)]
pub fn show_menu_bar() -> bool {
    SHOW_MENU_BAR.load(Ordering::Relaxed)
}

#[allow(dead_code)]
pub fn show_tool_bar() -> bool {
    SHOW_TOOL_BAR.load(Ordering::Relaxed)
}

/// Parse URL query parameters (`?view=1`, `?menuBar=0`, `?toolBar=0`, `?close=1`, `?download=1`)
/// using the exact default rules of `www/main.ts` (`menuBar` and `toolBar` default to `true` even when `view=1`).
#[allow(dead_code)]
pub fn resolve_embed_query_options(query: &str) -> (bool, bool, bool, bool, bool) {
    let q = query.strip_prefix('?').unwrap_or(query);
    let get = |name: &str| -> Option<bool> {
        for pair in q.split('&').filter(|s| !s.is_empty()) {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            if k == name {
                return Some(!matches!(v, "0" | "false" | ""));
            }
        }
        None
    };
    let close = get("close").unwrap_or(false);
    let download = get("download").unwrap_or(false);
    let view = get("view").unwrap_or(false);
    let menu_bar = get("menuBar").or_else(|| get("menubar")).unwrap_or(true);
    let tool_bar = get("toolBar").or_else(|| get("toolbar")).unwrap_or(true);
    (close, download, view, menu_bar, tool_bar)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    /// Defined by `www/main.ts`: tells the embedding page (`"close"`, or
    /// `"download"` with the file's name and bytes).
    #[wasm_bindgen(js_name = ibisOfficeEmbedAction)]
    fn embed_action_js(kind: &str, name: &str, bytes: Option<Vec<u8>>);
}

#[cfg(target_arch = "wasm32")]
impl SlideEditor {
    /// The close button: the embedding page is told to close the preview.
    pub(crate) fn embed_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        embed_action_js("close", "", None);
    }

    /// The download button: the document as it is now, in its own format, goes to the embedding page.
    pub(crate) fn embed_download(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // What is typed in a cell is entered first, as a save does (`commit_pending_edit`): the file is the workbook as it is now.
        self.commit_pending_edit(window, cx);
        let Some(source) = self.source.clone() else {
            self.status = Some(crate::i18n::text("_shell.saving-new-documents-is-not-yet-supported").into());
            return cx.notify();
        };
        match source.save(&self.presentation_for_save(), ibis_pptx::SaveOptions::default()) {
            Ok(bytes) => embed_action_js("download", self.file_name.as_deref().unwrap_or(""), Some(bytes)),
            Err(err) => {
                self.status = Some(crate::i18n::format("file.save-failed", &[("error", &err)]).into());
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embed_view_mode_keeps_menu_bar_and_tool_bar_enabled_by_default() {
        // Default atomic state starts with SHOW_MENU_BAR = true and SHOW_TOOL_BAR = true
        assert!(show_menu_bar());
        assert!(show_tool_bar());
        assert!(!is_view_only());

        // ?view=1 keeps menuBar=true and toolBar=true by default
        let (close, download, view, menu_bar, tool_bar) = resolve_embed_query_options("?view=1");
        assert!(!close);
        assert!(!download);
        assert!(view);
        assert!(menu_bar, "menuBar must default to true even when view=1");
        assert!(tool_bar, "toolBar must default to true even when view=1");

        set_embed_options(close, download, view, menu_bar, tool_bar);
        assert!(is_view_only());
        assert!(show_menu_bar());
        assert!(show_tool_bar());

        // ?view=1&menuBar=0&toolBar=0 explicitly hides both
        let (_, _, view2, menu_bar2, tool_bar2) = resolve_embed_query_options("?view=1&menuBar=0&toolBar=0");
        assert!(view2);
        assert!(!menu_bar2);
        assert!(!tool_bar2);

        // Reset defaults
        set_embed_options(false, false, false, true, true);
    }
}
