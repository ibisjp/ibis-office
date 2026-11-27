// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The browser version embedded in another page (a cloud service's preview):
//! the 閉じる and ダウンロード buttons at the top right, shown only when the
//! page asks (`www/main.ts`: `?close=1&download=1` or `window.ibisOfficeOptions`,
//! see `publish/docs/web-embedding.md`). A press hands the embedding page an
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
static FONT_PROMPT: AtomicBool = AtomicBool::new(true);

/// Whether the browser version leads the user to the fonts that can be downloaded (`?fd=0` turns it off; default on): the
/// セットアップ dialog that opens by itself at the first start of a version, and the ヘルプ > セットアップ entry. Called by
/// `www/main.ts` before `run`. The playground (GitHub Pages) starts with it off.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn set_font_prompt(on: bool) {
    FONT_PROMPT.store(on, Ordering::Relaxed);
}

/// See [`set_font_prompt`].
pub fn font_prompt() -> bool {
    // A host that switched `features.fontDownload` off has no downloads of fonts (nor the guidance to them).
    FONT_PROMPT.load(Ordering::Relaxed) && crate::host::options().font_download
}

/// `?fd=` of a query string with the rules of `www/main.ts`: on unless `0`, `false`, `no` or `off`.
#[allow(dead_code)]
pub fn resolve_font_prompt_query(query: &str) -> bool {
    let q = query.strip_prefix('?').unwrap_or(query);
    for pair in q.split('&').filter(|s| !s.is_empty()) {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k == "fd" {
            return !matches!(v.to_ascii_lowercase().as_str(), "0" | "false" | "no" | "off");
        }
    }
    true
}

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
    #[wasm_bindgen(js_name = ibisOfficeDownloadDocx)]
    fn download_docx_js(name: &str, bytes: Vec<u8>);
    #[wasm_bindgen(js_name = ibisOfficeDownloadHtml)]
    fn download_html_js(name: &str, bytes: Vec<u8>);
}

#[cfg(target_arch = "wasm32")]
impl SlideEditor {
    /// The close button: the embedding page is told to close the preview.
    pub(crate) fn embed_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        // With a host, the question about unsaved changes comes first, and the host is told to close once it is answered.
        if crate::host::is_connected() {
            self.host_after_unsaved_check(crate::host_editor::HostAfter::Close, window, cx);
            return;
        }
        embed_action_js("close", "", None);
    }

    /// Download only the content currently recognized from the read-only JTD.
    pub(crate) fn export_jtd_docx(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_jtd_read_only() || self.jtd_gallery { return; }
        match ibis_jtd::export_docx(self.presentation()) {
            Ok(bytes) => {
                let name = self.file_name.as_deref().unwrap_or("document.jtd");
                let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
                download_docx_js(&format!("{stem}.docx"), bytes);
                self.set_status(crate::i18n::text("file.docx-done").to_string(), cx);
            }
            Err(err) => self.set_status(crate::i18n::format("file.save-failed", &[("error", &err.to_string())]), cx),
        }
    }

    /// Download the content currently recognized from the read-only JTD as HTML.
    pub(crate) fn export_jtd_html(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_jtd_read_only() { return; }
        match ibis_jtd::export_html(self.presentation()) {
            Ok(bytes) => {
                let name = self.file_name.as_deref().unwrap_or("document.jtd");
                let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
                download_html_js(&format!("{stem}.html"), bytes);
                self.set_status(crate::i18n::text("file.html-done").to_string(), cx);
            }
            Err(err) => self.set_status(crate::i18n::format("file.save-failed", &[("error", &err.to_string())]), cx),
        }
    }

    /// The download button: the document as it is now, in its own format, goes to the embedding page.
    pub(crate) fn embed_download(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_pending_edit(window, cx);
        if let Some(bytes) = self.jtd_original.as_ref() {
            embed_action_js("download", self.file_name.as_deref().unwrap_or(""), Some(bytes.to_vec()));
            return;
        }
        let Some(source) = self.source.clone() else {
            self.status = Some(crate::i18n::text("_shell.saving-new-documents-is-not-yet-supported").into());
            return cx.notify();
        };
        match source.save(&self.presentation_for_save(), ibis_formats::SaveOptions::default()) {
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
