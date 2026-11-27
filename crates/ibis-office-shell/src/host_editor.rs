// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The editor's side of a host (`host.rs`): the document the host gave (`load`), saving it back (`saveFile`), saving it somewhere
//! else (`saveFileAs`: 名前を付けて保存, 書き出し, コピーを作成), what a failure of the host leads to (5.7 of the design), the
//! question about unsaved changes before another file replaces the document, and the notices to the host (dirty, saved, close).
//!
//! The editor does not know locks, URLs or accounts: it holds the version the host gave (opaque) and hands it back with a save. A
//! document the host did not give (opened from local storage, a new one) has no `binding`, and saving it asks where (the file
//! window, `host_file_dialog.rs`).
//!
//! Everything here is for every platform, so the scenario tests of the desktop build (with a `MemoryHost`) run it; the browser's
//! `www/main.ts` reaches it through `ibis-office-app`'s `host_*` exports.

use gpui_kit::component::Root;
use gpui_kit::*;

use crate::editor::SlideEditor;
use crate::host::{self, DocumentSource, HostEvent, HostFailure, HostReason, SaveAsRequest, SaveKind, SaveTrigger, SavedAs};
use crate::i18n::{format, text};

/// The host's file the document is tied to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostBinding {
    /// The folder of the file, when the host says (`HostDocument.folderId`): the file window starts there.
    pub folder_id: Option<String>,
    /// The version of the contents the host gave or last accepted (opaque; handed back with the next save).
    pub version: String,
}

/// What goes on after the question about unsaved changes is answered (保存 or 保存しない; キャンセル drops it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostAfter {
    /// The file window (開く) opens.
    Open,
    /// The browser's file picker (the page has one editor: the file replaces the document, K0054).
    OpenLocal,
    /// A new file of this kind replaces the document.
    New(ibis_formats::Kind),
    /// The host is told to close (the close button).
    Close,
}

/// A notice of the host (`notify`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostNotice {
    pub level: HostNoticeLevel,
    pub message: String,
    pub id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostNoticeLevel {
    Info,
    Warn,
    Error,
}

/// The editor's state about the host (one field of `SlideEditor`).
#[derive(Default)]
pub struct EditorHost {
    /// The host's file the document is tied to; `None` for a document opened from local storage or a new one.
    pub binding: Option<HostBinding>,
    /// Read-only because the host said so (`readOnly`, `setReadOnly`), with the words for the information bar (`None`: the editor's own).
    /// The editor's own view mode (`?view=1`) is `SlideEditor::read_only`; this one still lets 名前を付けて保存 and 書き出し through.
    pub read_only: Option<Option<String>>,
    /// A save is going on (a second one waits for it: the host is asked one at a time).
    pub saving: bool,
    /// What the host was last told about unsaved changes.
    pub last_dirty: bool,
    pub after_confirm: Option<HostAfter>,
    /// The host's notice shown in the information bar (the latest of each level).
    pub notices: Vec<HostNotice>,
    /// The file window (`host_file_dialog.rs`), while it is open.
    pub dialog: Option<crate::host_file_dialog::HostFileDialogState>,
}

/// The editor of the (one) window, if there is one: what the browser's exports act on.
pub fn with_editor<R>(cx: &mut App, f: impl FnOnce(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) -> R) -> Option<R> {
    for window in cx.windows() {
        let Some(root) = window.downcast::<Root>() else { continue };
        let editor = root.update(cx, |root, _, _| root.view().clone().downcast::<SlideEditor>().ok()).ok().flatten();
        if let Some(editor) = editor {
            return window.update(cx, |_, window, cx| editor.update(cx, |e, cx| f(e, window, cx))).ok();
        }
    }
    None
}

impl SlideEditor {
    /// Whether the host keeps this document (it gave it, and the host is still there).
    pub fn is_host_bound(&self) -> bool {
        self.host.binding.is_some() && host::is_connected()
    }

    /// The document the host gave: shown as a file is (`apply_opened`), tied to the host's file when `bound`, read-only when the host
    /// said so. A file that cannot be read is an error for the host's `load`.
    #[allow(clippy::too_many_arguments)]
    pub fn host_load(&mut self, name: String, bytes: Vec<u8>, version: String, folder_id: Option<String>, read_only: bool, message: Option<String>, bound: bool, window: &mut Window, cx: &mut Context<Self>) -> Result<(), String> {
        self.commit_edit(window, cx);
        // An old binary file (.xls, .doc, .ppt) is read by its content, as the desktop's opening does: saving it goes by 名前を付けて保存 (a current format).
        let is_legacy_binary = ibis_formats::is_legacy_binary(&bytes);
        let mut opened = crate::fixed_layout::open_any(&name, bytes).map_err(|e| format("file.open-failed", &[("name", &name), ("error", &e)]))?;
        opened.presentation.opened_from_legacy = is_legacy_binary;
        self.apply_opened(name.clone().into(), opened, window, cx);
        self.host.binding = bound.then_some(HostBinding { folder_id, version });
        self.host.read_only = read_only.then_some(message);
        self.host.last_dirty = false;
        self.host.saving = false;
        self.host.after_confirm = None;
        let kind = match ibis_formats::Kind::of(self.presentation()) {
            ibis_formats::Kind::Document => "docx",
            ibis_formats::Kind::Workbook => "xlsx",
            ibis_formats::Kind::Presentation => "pptx",
            ibis_formats::Kind::Pdf => "pdf",
        };
        host::emit(HostEvent::Loaded { name, kind });
        self.host_remember_recent(cx);
        cx.notify();
        Ok(())
    }

    /// The open file is put first in the host's recent files (its address from `getFileLocation`), and the list is read back for the menu (ファイル > 最近使ったファイル). A host that
    /// gave neither `addRecentFile` nor `listRecentFiles` is not asked.
    pub(crate) fn host_remember_recent(&mut self, cx: &mut Context<Self>) {
        let caps = host::capabilities();
        if !host::is_connected() || !(caps.add_recent_file || caps.list_recent_files) {
            return;
        }
        let Some(name) = self.file_name.as_ref().map(|n| n.to_string()) else { return };
        cx.spawn(async move |this, cx| {
            if caps.add_recent_file
                && let (Some(locate), Some(add)) = (host::bridge_with(|c| c.get_file_location), host::bridge_with(|c| c.add_recent_file))
                && let Ok(url) = locate.get_file_location().await
            {
                let _ = add.add_recent_file(host::RecentFile { name, url }).await;
            }
            if let Some(list) = host::bridge_with(|c| c.list_recent_files)
                && let Ok(files) = list.list_recent_files().await
            {
                host::set_recent_files(files);
            }
            this.update(cx, |_, cx| cx.notify()).ok();
        })
        .detach();
    }

    /// The host switches read-only on or off (the lock was lost or got back). The words are for the information bar.
    pub fn host_set_read_only(&mut self, read_only: bool, message: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if read_only {
            // What is being typed is entered first, then the editing stops.
            self.commit_pending_edit(window, cx);
        }
        self.host.read_only = read_only.then_some(message);
        cx.notify();
    }

    /// The host's notice in the information bar (`None`: clear them). A notice of a level replaces the one before of that level.
    pub fn host_notify(&mut self, notice: Option<HostNotice>, cx: &mut Context<Self>) {
        match notice {
            Some(notice) => {
                self.host.notices.retain(|n| n.level != notice.level);
                self.host.notices.push(notice);
            }
            None => self.host.notices.clear(),
        }
        cx.notify();
    }

    /// An export made in the browser (HTML and the like): to the host's storage through its file window when the host takes saves (and
    /// `features.export` allows the format), else downloaded as before. True when the file window took it (the caller does not say it
    /// was exported: it is not yet).
    pub fn browser_export(&mut self, name: String, mime: &str, bytes: Vec<u8>, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.browser_export_noted(name, mime, bytes, None, window, cx)
    }

    /// `browser_export` with a `note` about what the export lacks (characters left out of a PDF): when the host's file window writes it,
    /// the note, not 書き出しました alone, is what the status bar says at the end (B-7).
    pub fn browser_export_noted(&mut self, name: String, mime: &str, bytes: Vec<u8>, note: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let caps = host::capabilities();
        let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        // `features.export` does not allow it: neither the host's storage nor a download (the file does not leave).
        if !host::export_allowed(&extension) {
            self.set_status(text("shell.host.export-not-allowed").to_string(), cx);
            return true;
        }
        if host::is_connected() && caps.list_folder && caps.save_file_as {
            self.open_host_export_dialog(crate::host_file_dialog::ExportJob { name, extension, bytes, note }, window, cx);
            return true;
        }
        #[cfg(target_family = "wasm")]
        crate::web::download_bytes(&name, mime, &bytes);
        #[cfg(not(target_family = "wasm"))]
        let _ = (name, mime, bytes);
        false
    }

    /// The notice the bar shows: the one of the highest level (error, then warn, then info).
    pub(crate) fn host_notice_shown(&self) -> Option<HostNotice> {
        self.host.notices.iter().max_by_key(|n| match n.level {
            HostNoticeLevel::Error => 2,
            HostNoticeLevel::Warn => 1,
            HostNoticeLevel::Info => 0,
        }).cloned()
    }

    /// The document as the host would store it: the file's own format, with what is typed in a cell entered first.
    pub fn host_bytes(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<Vec<u8>, String> {
        let name = self.file_name.as_ref().map(|n| n.to_string());
        self.host_bytes_named(name.as_deref(), window, cx)
    }

    /// The text format (`csv` or `tsv`) a workbook is stored in under `name`: .csv, and .tsv or .tab (tab-separated). `None` for any other
    /// name or for a document that is not a workbook (its own package is stored).
    pub(crate) fn host_text_format(&self, name: Option<&str>) -> Option<&'static str> {
        if ibis_formats::Kind::of(self.presentation()) != ibis_formats::Kind::Workbook {
            return None;
        }
        let extension = name?.rsplit_once('.')?.1;
        if extension.eq_ignore_ascii_case("csv") {
            Some("csv")
        } else if extension.eq_ignore_ascii_case("tsv") || extension.eq_ignore_ascii_case("tab") {
            Some("tsv")
        } else {
            None
        }
    }

    /// The document as the host would store it under `name`: a workbook named .csv / .tsv / .tab is the sheet shown as text (UTF-8 with a BOM,
    /// CRLF; the values as they show, as the desktop's csv save), else the file's own format.
    pub fn host_bytes_named(&mut self, name: Option<&str>, window: &mut Window, cx: &mut Context<Self>) -> Result<Vec<u8>, String> {
        self.commit_pending_edit(window, cx);
        if let Some(why) = crate::format::with_format!(self, save_refused(self)) {
            return Err(why);
        }
        if let Some(format) = self.host_text_format(name) {
            let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
            return Ok(if format == "csv" { ibis_formats::csv::sheet_to_csv(self.presentation(), index) } else { ibis_formats::csv::sheet_to_tsv(self.presentation(), index) });
        }
        let source = self.source.clone().ok_or_else(|| text("_shell.this-file-cannot-be-saved-yet").to_string())?;
        let bytes = source.save(&self.presentation_for_save(), ibis_formats::SaveOptions::default()).map_err(|e| e.to_string())?;
        // A signed file that was edited is stored without the signatures; the bar goes with them.
        let (bytes, removed) = self.without_signatures(bytes, false);
        if removed {
            self.signatures_removed();
        }
        Ok(bytes)
    }

    /// After a save as text (.csv, .tsv): when the file holds more than the values of the sheet shown (other sheets, merged cells, formats),
    /// the status bar says that only those values were saved (the warning Excel gives when it saves a workbook as CSV).
    pub(crate) fn host_text_save_note(&mut self, name: Option<&str>, cx: &mut Context<Self>) {
        if self.host_text_format(name).is_none() {
            return;
        }
        let lossy = self.presentation().slides.len() > 1
            || self.presentation().slides.iter().any(|slide| {
                slide.shapes.iter().any(|shape| matches!(&shape.kind, ibis_doc_model::ShapeKind::Sheet(sheet) if !sheet.merges.is_empty() || sheet.styles.iter().any(|style| text_file_only_style(style) != ibis_doc_model::CellStyle::default())))
            });
        if lossy {
            self.set_status(text("shell.host.text-save-values-only").to_string(), cx);
        }
    }

    /// Tell the host when the unsaved-changes mark changed (called each time the window is drawn).
    pub(crate) fn host_sync_dirty(&mut self) {
        if !host::is_connected() || !self.has_document {
            return;
        }
        let dirty = self.is_dirty();
        if dirty != self.host.last_dirty {
            self.host.last_dirty = dirty;
            host::emit(HostEvent::Dirty(dirty));
        }
    }

    /// ⌘S with a host: `saveFile` for a document the host gave (and may be written), else 名前を付けて保存 (the file window). `false`
    /// when there is no host to ask (the editor's own way goes on). `then` runs once the host accepted the save.
    pub(crate) fn host_save(&mut self, trigger: SaveTrigger, window: &mut Window, cx: &mut Context<Self>, then: &mut Option<Box<dyn FnOnce(&mut Self, &mut Window, &mut Context<Self>)>>) -> bool {
        if !host::is_connected() {
            return false;
        }
        let caps = host::capabilities();
        // A document read from an old binary format is not written back as the new format over the old name: 名前を付けて保存 asks for the new name.
        let writable = self.host.binding.is_some() && self.host.read_only.is_none() && !self.read_only && !self.opened_from_legacy;
        if writable && caps.save_file {
            self.host_save_file(trigger, window, cx, then.take());
            return true;
        }
        if caps.list_folder && caps.save_file_as {
            self.open_host_file_dialog(crate::host_file_dialog::Mode::SaveAs, then.take(), window, cx);
            return true;
        }
        false
    }

    fn host_save_file(&mut self, trigger: SaveTrigger, window: &mut Window, cx: &mut Context<Self>, then: Option<Box<dyn FnOnce(&mut Self, &mut Window, &mut Context<Self>)>>) {
        if self.host.saving {
            return;
        }
        let Some(bridge) = host::bridge_with(|c| c.save_file) else { return };
        let Some(version) = self.host.binding.as_ref().map(|b| b.version.clone()) else { return };
        let bytes = match self.host_bytes(window, cx) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.set_status(format("file.save-failed", &[("error", &error)]), cx);
                return;
            }
        };
        self.host.saving = true;
        self.set_status(text("shell.host.saving").to_string(), cx);
        let name_saved = self.file_name.as_ref().map(|n| n.to_string());
        cx.spawn_in(window, async move |this, cx| {
            let result = bridge.save_file(bytes, version, trigger).await;
            this.update_in(cx, |this, window, cx| {
                this.host.saving = false;
                match result {
                    Ok(version) => {
                        if let Some(binding) = &mut this.host.binding {
                            binding.version = version;
                        }
                        this.document.mark_clean();
                        this.cleanup_active_autorecover_file();
                        this.set_status(text("_shell.saved").to_string(), cx);
                        this.host_text_save_note(name_saved.as_deref(), cx);
                        host::emit(HostEvent::Saved(trigger));
                        this.host_sync_dirty();
                        if let Some(then) = then {
                            then(this, window, cx);
                        }
                    }
                    Err(failure) => this.host_failed(failure, window, cx),
                }
            })
            .ok();
        })
        .detach();
    }

    /// A host's refusal, as the design's table (5.7) has it: a message box with the host's words (or the editor's own); for the
    /// reasons that leave the document unsaved where it is, 名前を付けて保存 is offered next; a lost connection is in the status bar
    /// too. The changes are kept (the document stays dirty).
    pub(crate) fn host_failed(&mut self, failure: HostFailure, window: &mut Window, cx: &mut Context<Self>) {
        let own = match failure.reason {
            HostReason::Network => text("shell.host.error-network"),
            HostReason::Unauthorized => text("shell.host.error-unauthorized"),
            HostReason::Exists => text("shell.host.error-exists"),
            HostReason::TooLarge => text("shell.host.error-too-large"),
            HostReason::Other => text("shell.host.error-other"),
            _ => text("shell.host.error-save-as"),
        };
        let message = failure.message.clone().filter(|m| !m.is_empty()).unwrap_or_else(|| own.to_string());
        if failure.reason == HostReason::Network {
            self.set_status(text("shell.host.error-network-status").to_string(), cx);
        }
        self.show_alert(text("shell.host.error-title"), message.into(), cx);
        let offer = matches!(failure.reason, HostReason::Locked | HostReason::Conflict | HostReason::NotFound | HostReason::NoPermission | HostReason::ReadOnly) && failure.offers_save_as();
        if offer && host::capabilities().list_folder && host::capabilities().save_file_as {
            self.open_host_file_dialog(crate::host_file_dialog::Mode::SaveAs, None, window, cx);
        }
    }

    /// 名前を付けて保存 / 書き出し / コピーを作成 once the file window decided: the bytes go to `saveFileAs` (to `overwrite` when the user
    /// agreed to replace a file). Saving as ties the document to the new file. `done` gets the host's answer (the window closes on `Ok`).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn host_save_as(
        &mut self,
        kind: SaveKind,
        folder_id: String,
        name: String,
        bytes: Vec<u8>,
        overwrite: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
        done: Box<dyn FnOnce(&mut Self, Result<SavedAs, HostFailure>, &mut Window, &mut Context<Self>)>,
    ) {
        let Some(bridge) = host::bridge_with(|c| c.save_file_as) else { return };
        let folder_hint = folder_id.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = bridge.save_file_as(SaveAsRequest { kind, folder_id, name, bytes, overwrite_file_id: overwrite }).await;
            this.update_in(cx, |this, window, cx| {
                if let Ok(saved) = &result {
                    match kind {
                        SaveKind::SaveAs => {
                            this.host.binding = Some(HostBinding { folder_id: Some(folder_hint), version: saved.version.clone() });
                            // The document is the new file now (and writable: the host gave the lock with the file).
                            this.host.read_only = None;
                            this.file_name = Some(saved.file_name.clone().into());
                            window.set_window_title(&format!("{} - ibisOffice", saved.file_name));
                            this.document.mark_clean();
                            this.cleanup_active_autorecover_file();
                            this.opened_from_legacy = false;
                            this.set_status(text("_shell.saved").to_string(), cx);
                            this.host_text_save_note(Some(&saved.file_name), cx);
                            host::emit(HostEvent::Saved(SaveTrigger::User));
                            this.host_sync_dirty();
                            this.host_remember_recent(cx);
                        }
                        SaveKind::Export => this.set_status(text("shell.host.exported").to_string(), cx),
                        SaveKind::Copy => this.set_status(text("_shell.copy-saved").to_string(), cx),
                    }
                }
                done(this, result, window, cx);
            })
            .ok();
        })
        .detach();
    }

    /// The question about unsaved changes, if there are any, before `after` (another file replaces the document); without any, `after` goes
    /// on at once.
    pub(crate) fn host_after_unsaved_check(&mut self, after: HostAfter, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_pending_edit(window, cx);
        if self.is_dirty() {
            self.host.after_confirm = Some(after);
            self.ask_to_save(false, cx);
        } else {
            self.host_run_after(after, window, cx);
        }
    }

    pub(crate) fn host_run_after(&mut self, after: HostAfter, window: &mut Window, cx: &mut Context<Self>) {
        match after {
            HostAfter::Open => self.open_host_file_dialog(crate::host_file_dialog::Mode::Open, None, window, cx),
            HostAfter::OpenLocal => self.open_file_local(window, cx),
            HostAfter::New(kind) => self.new_file_in_window(kind, window, cx),
            HostAfter::Close => host::emit(HostEvent::CloseRequested),
        }
    }

    /// A document that is not the host's replaced this one (opened from local storage, a new one): the host is told, so it lets go of the
    /// lock of the file before.
    pub(crate) fn host_document_changed(&mut self, source: DocumentSource) {
        self.host.binding = None;
        self.host.read_only = None;
        self.host.last_dirty = false;
        self.host.saving = false;
        host::emit(HostEvent::DocumentChanged(source));
    }
}

/// The host's watermark over the window: the text repeated in rows, in a faint grey, the pointer going through it (it takes no clicks).
/// Not part of the file; drawn above the document and below the dialogs.
pub(crate) fn watermark_overlay(watermark: &host::Watermark, window: &Window) -> impl IntoElement {
    let size = window.viewport_size();
    let (font, gap) = (px(28.), px(90.));
    // Rows and columns enough to fill the window (about 0.6 em a character; the text and the gap decide the cell).
    let cell_w = font * (0.6 * watermark.text.chars().count() as f32 + 3.0);
    let columns = ((size.width / cell_w).ceil() as usize).max(1) + 1;
    let rows = ((size.height / (font + gap)).ceil() as usize).max(1) + 1;
    div()
        .id("host-watermark")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .overflow_hidden()
        .flex()
        .flex_col()
        .gap(gap)
        .pt(gap)
        .children((0..rows).map(|row| {
            div().flex().flex_none().gap(font * 3.0).ml(if row % 2 == 0 { px(0.) } else { -cell_w / 2.0 }).children((0..columns).map(|_| {
                div().flex_none().text_size(font).text_color(hsla(0., 0., 0.45, watermark.opacity)).child(watermark.text.clone())
            }))
        }))
}

/// A cell style as a text file (csv/tsv) opened gives it: the file's own font (#243), the cells at the top (#17), the number format that keeps
/// a value as typed (a leading zero, a date-like text), the theme's text colour and the format's index are what reading the text file puts there, not formatting
/// the file lost, so the "only the values are saved" note leaves them out.
fn text_file_only_style(style: &ibis_doc_model::CellStyle) -> ibis_doc_model::CellStyle {
    let default = ibis_doc_model::CellStyle::default();
    let mut style = style.clone();
    style.font = default.font;
    style.font_size = default.font_size;
    style.vertical = default.vertical;
    style.vertical_set = default.vertical_set;
    style.number_format = default.number_format;
    // The font colour of the workbook's theme (the text colour).
    style.color = default.color;
    style.color_theme = default.color_theme;
    style.xf = None;
    style
}
