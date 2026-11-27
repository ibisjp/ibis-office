// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ファイル > 新規作成 (⌘N for a document) and the file actions every
//! format shares (コピーを作成…, 書き出し, 印刷…).
//!
//! A new file is an empty package written by `ibis_formats::blank_file`,
//! opened as a file is (`ibis_formats::open`), so it saves as an opened file
//! does; it has no path until saved (⌘S asks where).

use gpui_kit::*;
use crate::format::with_format;

use crate::editor::SlideEditor;

impl SlideEditor {
    /// A new, empty file of `kind`: in this window when it shows no
    /// document, else in a new window.
    pub fn new_file(&mut self, kind: ibis_formats::Kind, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        let opened = match ibis_formats::blank_file(kind).and_then(ibis_formats::open) {
            Ok(opened) => opened,
            Err(err) => {
                self.status = Some(crate::i18n::format("file.new-failed", &[("error", &err)]).into());
                return cx.notify();
            }
        };
        // A host that keeps the files: the new file replaces the document of this page (the host's iframe is one editor; another tab
        // would not be connected to the host), after the question about unsaved changes.
        if crate::host::is_connected() && self.has_document {
            self.host_after_unsaved_check(crate::host_editor::HostAfter::New(kind), window, cx);
            return;
        }
        let name: SharedString = next_new_file_name(kind);
        // The Web version has one editor per page: the new file is another tab (K0016).
        #[cfg(target_family = "wasm")]
        if self.has_document {
            crate::web::open_in_new_tab(crate::web::new_query(kind));
            return;
        }
        if self.has_document {
            crate::app::open_window_with(cx, move |editor, window, cx| editor.show_new(opened, name, window, cx));
        } else {
            self.show_new(opened, name, window, cx);
        }
    }

    /// A new, empty file of `kind` in this window, in place of the document (with a host; the unsaved changes were asked about). The
    /// document is not the host's: the host is told.
    pub(crate) fn new_file_in_window(&mut self, kind: ibis_formats::Kind, window: &mut Window, cx: &mut Context<Self>) {
        match ibis_formats::blank_file(kind).and_then(ibis_formats::open) {
            Ok(opened) => {
                self.show_new(opened, next_new_file_name(kind), window, cx);
                self.host_document_changed(crate::host::DocumentSource::New);
            }
            Err(err) => self.set_status(crate::i18n::format("file.new-failed", &[("error", &err)]), cx),
        }
    }

    /// Show `opened`, a new file named `name` that has no path yet.
    fn show_new(&mut self, opened: ibis_formats::Opened, name: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        self.load(opened.presentation, name, window, cx);
        self.source = Some(std::sync::Arc::new(opened.source));
        // (A document gets its default design; a new presentation opens without the speaker notes.)
        with_format!(self, new_file_shown(self, cx));
        self.document.reset_history();
    }

    /// ファイル > 新規 > テンプレートから新規作成… (New from Template...)
    pub fn new_from_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false, extensions: Vec::new(),
                multiple: false,
                prompt: Some(crate::i18n::text("pptx.new-from-template").into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update_in(cx, |this, window, cx| this.open_template(path, window, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.status = Some(crate::i18n::text("_shell.not-available-in-the-browser-version-yet").into());
            cx.notify();
        }
    }

    /// The template picked: a new file made of it in a window of its own when this window shows a document (as `new_file` does, and as
    /// Office opens a template: the open workbook, with a cell being typed in and edits not saved, is not replaced), else in this one.
    #[cfg(not(target_family = "wasm"))]
    pub fn open_template(&mut self, path: std::path::PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if self.has_document {
            crate::app::open_window_with(cx, move |editor, window, cx| editor.new_from_template_path(path, window, cx));
        } else {
            self.new_from_template_path(path, window, cx);
        }
    }

    /// テンプレートファイル `path` を読み込み、未保存の新規プレゼンテーションとして開く。
    #[cfg(not(target_family = "wasm"))]
    pub fn new_from_template_path(&mut self, path: std::path::PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        let read_path = path.clone();
        let tmpl_name: SharedString = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().into();
        self.set_status(crate::i18n::format("file.opening", &[("name", &tmpl_name)]), cx);
        let closed = self.closed;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move { crate::read_file::read_file(&read_path) }).await;
            this.update_in(cx, |this, window, cx| match result {
                _ if this.closed != closed => {}
                Ok(loaded) => {
                    if loaded.jtd_original.is_some() {
                        this.fail_loading(crate::i18n::text("file.jtd-read-only").to_string(), cx);
                        return;
                    }
                    let name: SharedString = next_new_file_name(ibis_formats::Kind::of(&loaded.presentation));
                    this.load(loaded.presentation, name, window, cx);
                    this.set_loaded_source(loaded.source);
                    this.path = None;
                    this.document.set_file(None);
                    this.document.reset_history();
                    this.recent_files = crate::recent_files::add(&path);
                    crate::app::set_app_menus(cx);
                    this.set_status(crate::i18n::text("file.new-from-template-done").to_string(), cx);
                    crate::timing::document_loaded(window);
                }
                Err(err) => {
                    this.fail_loading(crate::i18n::format("file.open-failed", &[("name", &tmpl_name), ("error", &err)]), cx);
                }
            })
            .ok();
        })
        .detach();
    }
}

/// The name a new file shows until it is saved (the first one: 文書 1.docx, Book1.xlsx, Presentation1.pptx).
pub fn new_file_name(kind: ibis_formats::Kind) -> SharedString {
    (crate::format::info(kind).new_file_name)()
}

thread_local! {
    /// How many new files of each kind (document, workbook, presentation) this session has made. Closing one does not give its number back (as in Office).
    static NEW_FILES: std::cell::Cell<[u32; 4]> = const { std::cell::Cell::new([0; 4]) };
}

fn kind_slot(kind: ibis_formats::Kind) -> usize {
    match kind {
        ibis_formats::Kind::Document => 0,
        ibis_formats::Kind::Workbook => 1,
        ibis_formats::Kind::Presentation => 2,
        ibis_formats::Kind::Pdf => 3,
    }
}

/// `name` (such as `文書 1.docx`) with the number before the extension replaced by `n`.
pub(crate) fn numbered_name(name: &str, n: u32) -> String {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) => (stem, format!(".{ext}")),
        None => (name, String::new()),
    };
    format!("{}{n}{ext}", stem.trim_end_matches(|c: char| c.is_ascii_digit()))
}

/// The name of the next new file of `kind`: the first is 1, then 2, 3… counted per kind over the session. Both the window's title and the
/// default name of the Save dialog come from it (the file's name until it is saved).
pub fn next_new_file_name(kind: ibis_formats::Kind) -> SharedString {
    let n = NEW_FILES.with(|c| {
        let mut counts = c.get();
        counts[kind_slot(kind)] += 1;
        c.set(counts);
        counts[kind_slot(kind)]
    });
    numbered_name(&new_file_name(kind), n).into()
}

/// Start counting from 1 again (the scenarios).
pub fn reset_new_file_numbers() {
    NEW_FILES.with(|c| c.set([0; 4]));
}

impl SlideEditor {
    /// The open file as it would be saved now.
    pub fn current_bytes(&self) -> Result<Vec<u8>, String> {
        if self.is_jtd_read_only() {
            if self.jtd_gallery { return Err(crate::i18n::text("file.jtd-gallery-export-disabled").to_string()); }
            return ibis_jtd::export_docx(self.presentation()).map_err(|e| e.to_string());
        }
        if let Some(why) = crate::format::with_format!(self, save_refused(self)) {
            return Err(why);
        }
        let source = self.source.clone().ok_or(crate::i18n::text("_shell.this-file-cannot-be-saved-yet").to_string())?;
        let bytes = source.save(&self.presentation_for_save(), ibis_formats::SaveOptions::default()).map_err(|e| e.to_string())?;
        Ok(self.without_signatures(bytes, false).0)
    }

    /// Write the file as it is now to `path`, leaving the file's own path.
    #[cfg(not(target_family = "wasm"))]
    pub fn copy_to(&self, path: &std::path::Path) -> Result<(), String> {

        if self.is_jtd_read_only() { return Err(crate::i18n::text("file.jtd-read-only").to_string()); }
        if self.jtd_source_path.as_deref().is_some_and(|original| crate::save::same_file(original, path) || path.extension().and_then(|ext| ext.to_str()).is_some_and(ibis_jtd::is_native_extension)) {
            return Err(crate::i18n::text("file.jtd-source-protected").to_string());
        }
        if let Some(why) = crate::format::with_format!(self, save_refused(self)) {
            return Err(why);
        }
        let source = self.source.clone().ok_or(crate::i18n::text("_shell.this-file-cannot-be-saved-yet").to_string())?;
        let (bytes, may_protect) = match crate::format::with_format!(self, bytes_for_path(self, path, &source, true)) {
            Some(other) => {
                let other = other?;
                (other.bytes, other.protect)
            }
            None => (source.save(&self.presentation_for_save(), ibis_formats::SaveOptions::default()).map_err(|e| e.to_string())?, true),
        };
        // A signed file that was edited: the copy is written without the signatures (as a save is).
        let other_kind = self.path.as_deref().and_then(|own| own.extension()).map(|e| e.to_ascii_lowercase()) != path.extension().map(|e| e.to_ascii_lowercase());
        let (bytes, _) = self.without_signatures(bytes, other_kind);
        // A copy keeps the protection of the file.
        let bytes = match (&self.protection_password, may_protect) {
            (Some(password), true) => crate::save::protect(&bytes, password)?,
            _ => bytes,
        };
        crate::save::write_replacing(path, &bytes).map_err(|e| crate::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)]))
    }

    /// Export the recognized JTD content as a separate, editable DOCX.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn export_jtd_docx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_jtd_read_only() || self.jtd_gallery { return; }
        let (stem, _) = self.name_parts();
        self.ask_path(format!("{stem}.docx"), window, cx, |this, path, _, cx| {
            let result = this.export_jtd_docx_to(&path)
                .map(|()| crate::i18n::text("file.docx-done").to_string())
                .map_err(|error| crate::i18n::format("file.save-failed", &[("error", &error)]));
            this.report(result, cx);
        });
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn export_jtd_docx_to(&self, path: &std::path::Path) -> Result<(), String> {
        if !self.is_jtd_read_only() || self.jtd_gallery || !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("docx"))
            || self.path.as_deref().is_some_and(|original| crate::save::same_file(original, path)) {
            return Err(crate::i18n::text("file.jtd-read-only").to_string());
        }
        let bytes = ibis_jtd::export_docx(self.presentation()).map_err(|e| e.to_string())?;
        crate::save::write_replacing(path, &bytes).map_err(|e| e.to_string())
    }

    /// Export the recognized JTD content as a separate HTML document.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn export_jtd_html(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_jtd_read_only() { return; }
        let (stem, _) = self.name_parts();
        self.ask_path(format!("{stem}.html"), window, cx, |this, path, _, cx| {
            let result = this.export_jtd_html_to(&path)
                .map(|()| crate::i18n::text("file.html-done").to_string())
                .map_err(|error| crate::i18n::format("file.save-failed", &[("error", &error)]));
            this.report(result, cx);
        });
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn export_jtd_html_to(&self, path: &std::path::Path) -> Result<(), String> {
        if !self.is_jtd_read_only() || !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("html"))
            || self.path.as_deref().is_some_and(|original| crate::save::same_file(original, path)) {
            return Err(crate::i18n::text("file.jtd-read-only").to_string());
        }
        let bytes = ibis_jtd::export_html(self.presentation()).map_err(|e| e.to_string())?;
        crate::save::write_replacing(path, &bytes).map_err(|e| e.to_string())
    }

    /// ファイル > パスワードで保護…: the window that sets the password (or takes it off).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn protect_with_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let had = self.protection_password.is_some();
        self.ask_password_to_set(had, window, cx);
    }

    /// (The Web version does not protect files with a password yet.)
    #[cfg(target_family = "wasm")]
    pub(crate) fn protect_with_password(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_status(crate::i18n::text("_shell.not-available-in-the-browser-version-yet").to_string(), cx);
    }

    /// The password of the file set (`Some`) or taken off (`None`), and the file saved so (the
    /// password is in what is saved).
    #[cfg(not(target_family = "wasm"))]
    pub fn set_protection(&mut self, password: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let on = password.is_some();
        self.protection_password = password;
        match self.path.clone() {
            Some(path) => {
                let result = self.save_to(path, window);
                let message = match result {
                    Ok(()) => i18n_status(on),
                    Err(err) => crate::i18n::format("file.save-failed", &[("error", &err)]),
                };
                self.set_status(message, cx);
            }
            None => self.set_status(crate::i18n::text("_shell.the-password-is-applied-when-you-save-the-file").to_string(), cx),
        }
    }

    /// The file's name without its extension, and its extension.
    pub fn name_parts(&self) -> (String, String) {
        let name = self.file_name.clone().unwrap_or_default().to_string();
        match name.rsplit_once('.') {
            Some((stem, ext)) => (stem.to_string(), ext.to_ascii_lowercase()),
            None => (name, String::new()),
        }
    }

    /// Ask for a new path (in the file's folder, named `name`), then run
    /// `then` with it.
    #[cfg(not(target_family = "wasm"))]
    fn ask_path(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>, then: impl FnOnce(&mut Self, std::path::PathBuf, &mut Window, &mut Context<Self>) + 'static) {
        self.ask_path_as(crate::save::RefusedWay::Export, name, window, cx, then);
    }

    /// `ask_path`, a refusal told as `way` says.
    #[cfg(not(target_family = "wasm"))]
    fn ask_path_as(&mut self, way: crate::save::RefusedWay, name: String, window: &mut Window, cx: &mut Context<Self>, then: impl FnOnce(&mut Self, std::path::PathBuf, &mut Window, &mut Context<Self>) + 'static) {
        // The one place every export and copy asks where to write: the format's refusal (a presentation's master view is closed
        // first) is told here, before the dialog, and again once the path is chosen, so no kind of file can be left out.
        if self.refuse_as(way, cx) {
            return;
        }
        // A signed file: an export or a changed copy does not carry valid signatures, so asked first (the one place for them all).
        let then = std::rc::Rc::new(std::cell::RefCell::new(Some(then)));
        let later = then.clone();
        let again_name = name.clone();
        let signature_way = match way {
            crate::save::RefusedWay::Export | crate::save::RefusedWay::Print => crate::signature::SignatureWay::Export,
            crate::save::RefusedWay::Save | crate::save::RefusedWay::Copy => crate::signature::SignatureWay::Save,
        };
        if self.confirm_signature_loss(signature_way, window, cx, move |this, window, cx| {
            if let Some(then) = later.borrow_mut().take() {
                this.ask_path_as(way, again_name.clone(), window, cx, then);
            }
        }) {
            return;
        }
        let Some(then) = then.borrow_mut().take() else { return };
        let directory = self
            .path
            .as_deref()
            .and_then(|p| p.parent())
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| self.new_file_save_folder());
        let picked = cx.prompt_for_new_path(&directory, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else { return };
            this.update_in(cx, |this, window, cx| {

                if this.refuse_as(way, cx) {
                    return;
                }
                then(this, path, window, cx)
            })
            .ok();
        })
        .detach();
    }

    pub fn report(&mut self, result: Result<String, String>, cx: &mut Context<Self>) {
        self.status = Some(match result {
            Ok(done) => done.into(),
            Err(err) => err.into(),
        });
        cx.notify();
    }

    /// ファイル > コピーを作成…: save the file as it is now to another
    /// path; the window keeps the file's own path.
    pub fn save_copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_jtd_read_only() {
            self.set_status(crate::i18n::text("file.jtd-read-only").to_string(), cx);
            return;
        }
        self.commit_pending_edit(window, cx);
        // The browser version (with or without a host): the copy is downloaded at once, as the file is now (the changes so far included). It is not a save, so the unsaved-changes
        // mark stays (K0037).
        #[cfg(target_family = "wasm")]
        {
            if !self.refuse_as(crate::save::RefusedWay::Copy, cx) {
                let (stem, ext) = self.name_parts();
                let name = crate::i18n::format("file.copy-name", &[("name", &stem), ("ext", &ext)]);
                match self.host_bytes(window, cx) {
                    Ok(bytes) => {
                        crate::web::download_bytes(&name, "application/octet-stream", &bytes);
                        self.set_status(crate::i18n::text("_shell.copy-saved").to_string(), cx);
                    }
                    Err(error) => self.set_status(crate::i18n::format("file.copy-failed", &[("error", &error)]), cx),
                }
            }
            return;
        }
        // A host that keeps the files: its file window, and the copy goes to `saveFileAs` (the document stays the host's own file).
        if crate::host::capabilities().list_folder && crate::host::capabilities().save_file_as {
            if !self.refuse_as(crate::save::RefusedWay::Copy, cx) {
                // A signed file that was edited: asked first.
                if self.confirm_signature_loss(crate::signature::SignatureWay::Save, window, cx, |this, window, cx| this.save_copy(window, cx)) {
                    return;
                }
                self.open_host_copy_dialog(window, cx);
            }
            return;
        }
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, ext) = self.name_parts();
            self.ask_path_as(crate::save::RefusedWay::Copy, crate::i18n::format("file.copy-name", &[("name", &stem), ("ext", &ext)]), window, cx, |this, path, _, cx| {
                let result = this.copy_to(&path).map(|()| crate::i18n::text("_shell.copy-saved").to_string()).map_err(|e| crate::i18n::format("file.copy-failed", &[("error", &e)]));
                this.report(result, cx);
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::i18n::text("_shell.saving-is-not-yet-available-in-the-browser-versi").into()), cx);
    }

    /// The file as it is now, as a PDF at `pdf`: saved to a temporary file
    /// and drawn by this program's `--export pdf` (on another thread).
    #[cfg(not(target_family = "wasm"))]
    pub fn write_pdf(&self, pdf: std::path::PathBuf) -> Result<Box<dyn FnOnce() -> Result<std::path::PathBuf, String> + Send + 'static>, String> {
        self.write_pdf_with(pdf, false)
    }

    /// As `write_pdf`; `printing` makes a presentation's PDF as 印刷の設定 say
    /// (the slides, handouts, colour, frame).
    #[cfg(not(target_family = "wasm"))]
    pub fn write_pdf_with(&self, pdf: std::path::PathBuf, printing: bool) -> Result<Box<dyn FnOnce() -> Result<std::path::PathBuf, String> + Send + 'static>, String> {
        // A PDF (or an XPS or OFD file) that is looked at is the PDF to write: its file as it is (made again through the export it would have no pages).
        if self.kind() == ibis_formats::Kind::Pdf
            && let Some(original) = self.presentation().pdf_bytes.clone()
        {
            return Ok(Box::new(move || {
                crate::save::write_replacing(&pdf, &original).map_err(|e| crate::i18n::format("file.cannot-write", &[("path", &pdf.display()), ("error", &e)]))?;
                Ok(pdf)
            }));
        }

        if self.jtd_source_path.as_deref().is_some_and(|original| crate::save::same_file(original, &pdf)) {
            return Err(crate::i18n::text("file.jtd-source-protected").to_string());
        }
        if self.is_jtd_read_only() && self.path.as_deref().is_some_and(|p| crate::save::same_file(p, &pdf)) {
            return Err(crate::i18n::text("file.jtd-read-only").to_string());
        }

        let jtd_original_path = self.jtd_source_path.clone().or_else(|| self.is_jtd_read_only().then(|| self.path.clone()).flatten());
        let crate::format::PdfJob { bytes, slides, format, args: print_args } = with_format!(self, pdf_job(self, printing))?;
        let (stem, mut ext) = self.name_parts();
        if self.is_jtd_read_only() { ext = "docx".into(); }
        let own_args = with_format!(self, export_args(self));
        Ok(Box::new(move || {
            let dir = std::env::temp_dir().join(format!("ibisoffice-export-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let out = dir.join("out");
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            let file = dir.join(format!("{stem}.{ext}"));
            std::fs::write(&file, &bytes).map_err(|e| e.to_string())?;
            let program = export_program()?;
            let mut cmd = std::process::Command::new(program);
            cmd.args(["--export", format, "--out-dir"]).arg(&out);
            if let Some(args) = &print_args {
                cmd.args(args);
            } else if let Some(slides) = &slides {
                let list = slides.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",");
                cmd.args(["--range", &list]);
            }
            cmd.args(&own_args);
            cmd.arg(&file);
            let status = cmd.status().map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(crate::i18n::format("file.pdf-exit", &[("status", &status)]));
            }
            let made = std::fs::read_dir(&out)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
                .ok_or(crate::i18n::text("_shell.could-not-create-the-pdf").to_string())?;
            let data = std::fs::read(&made).map_err(|e| e.to_string())?;
            let mut dest = pdf;
            if let Err(e) = crate::save::write_replacing(&dest, &data) {
                if !printing {
                    return Err(crate::i18n::format("file.cannot-write", &[("path", &dest.display()), ("error", &e)]));
                }
                let parent = dest.parent().map(std::path::Path::to_path_buf).unwrap_or_else(std::env::temp_dir);
                let mut wrote = false;
                for n in 1..=99 {
                    let alt = parent.join(format!("{stem}-{n}.pdf"));
                    if jtd_original_path.as_deref().is_some_and(|original| crate::save::same_file(original, &alt)) {
                        continue;
                    }
                    if crate::save::write_replacing(&alt, &data).is_ok() {
                        dest = alt;
                        wrote = true;
                        break;
                    }
                }
                if !wrote {
                    return Err(crate::i18n::format("file.cannot-write", &[("path", &dest.display()), ("error", &e)]));
                }
            }
            let _ = std::fs::remove_dir_all(&dir);
            Ok(dest)
        }))
    }

    /// ファイル > 書き出し > Web ページ: the document as one HTML page at `target`, its charts and SmartArt as pictures (#208). The work is `--export html` of a copy of the document
    /// (the one place that draws them: `ibis_office_app::html_export`), a job to run off the window's thread.
    #[cfg(not(target_family = "wasm"))]
    pub fn write_html_with_pictures(&self, target: std::path::PathBuf, sheet: Option<usize>) -> Result<Box<dyn FnOnce() -> Result<std::path::PathBuf, String> + Send + 'static>, String> {
        let crate::format::PdfJob { bytes, .. } = with_format!(self, pdf_job(self, false))?;
        let (stem, ext) = self.name_parts();
        let own_args = with_format!(self, export_args(self));
        Ok(Box::new(move || {
            let dir = std::env::temp_dir().join(format!("ibisoffice-html-job-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let out = dir.join("out");
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            let file = dir.join(format!("{stem}.{ext}"));
            std::fs::write(&file, &bytes).map_err(|e| e.to_string())?;
            let mut cmd = std::process::Command::new(export_program()?);
            cmd.args(["--export", "html", "--out-dir"]).arg(&out).args(&own_args);
            if let Some(sheet) = sheet {
                // A workbook's page is one sheet (the 1-based `--range` of it).
                cmd.args(["--range", &sheet.to_string()]);
            }
            cmd.arg(&file);
            let status = cmd.status().map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(crate::i18n::format("file.pdf-exit", &[("status", &status)]));
            }
            let made = std::fs::read(out.join(format!("{stem}.html"))).map_err(|e| e.to_string())?;
            crate::save::write_replacing(&target, &made).map_err(|e| crate::i18n::format("file.cannot-write", &[("path", &target.display()), ("error", &e)]))?;
            let _ = std::fs::remove_dir_all(&dir);
            Ok(target)
        }))
    }

    /// The file as it is now, as PNG pictures at `png`: saved to a temporary file
    /// and drawn by this program's `--export png` (on another thread).
    #[cfg(not(target_family = "wasm"))]
    pub fn write_png(&self, png: std::path::PathBuf, slide: Option<usize>) -> Result<Box<dyn FnOnce() -> Result<std::path::PathBuf, String> + Send + 'static>, String> {

        if self.jtd_source_path.as_deref().is_some_and(|original| crate::save::same_file(original, &png)) {
            return Err(crate::i18n::text("file.jtd-source-protected").to_string());
        }
        if self.is_jtd_read_only() && self.path.as_deref().is_some_and(|p| crate::save::same_file(p, &png)) {
            return Err(crate::i18n::text("file.jtd-read-only").to_string());
        }
        let jtd_original_path = self.jtd_source_path.clone().or_else(|| self.is_jtd_read_only().then(|| self.path.clone()).flatten());

        let bytes = self.current_bytes()?;
        let (stem, mut ext) = self.name_parts();
        if self.is_jtd_read_only() { ext = "docx".into(); }
        let own_args = with_format!(self, export_args(self));
        Ok(Box::new(move || {
            let dir = std::env::temp_dir().join(format!("ibisoffice-export-png-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let out = dir.join("out");
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            let file = dir.join(format!("{stem}.{ext}"));
            std::fs::write(&file, &bytes).map_err(|e| e.to_string())?;
            let program = export_program()?;
            let mut cmd = std::process::Command::new(program);
            cmd.args(["--export", "png", "--out-dir"]).arg(&out);
            if let Some(num) = slide {
                cmd.args(["--range", &num.to_string()]);
            }
            cmd.args(&own_args);
            let status = cmd.arg(&file).status().map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(crate::i18n::format("file.png-exit", &[("status", &status)]));
            }
            let mut made: Vec<_> = std::fs::read_dir(&out)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
                .collect();
            made.sort();
            if made.is_empty() {
                return Err(crate::i18n::text("_shell.could-not-create-the-png").to_string());
            }
            if made.len() == 1 {
                let data = std::fs::read(&made[0]).map_err(|e| e.to_string())?;
                crate::save::write_replacing(&png, &data).map_err(|e| crate::i18n::format("file.cannot-write", &[("path", &png.display()), ("error", &e)]))?;
            } else {
                let parent = png.parent().unwrap_or_else(|| std::path::Path::new("."));
                let target_stem = png.file_stem().and_then(|s| s.to_str()).unwrap_or(&stem);
                for (idx, p) in made.iter().enumerate() {
                    let dest = parent.join(format!("{target_stem}-{:02}.png", idx + 1));

                    if jtd_original_path.as_deref().is_some_and(|original| crate::save::same_file(original, &dest)) {
                        return Err(crate::i18n::text("file.jtd-read-only").to_string());
                    }
                    let data = std::fs::read(p).map_err(|e| e.to_string())?;
                    crate::save::write_replacing(&dest, &data).map_err(|e| crate::i18n::format("file.cannot-write", &[("path", &dest.display()), ("error", &e)]))?;
                }
            }
            let _ = std::fs::remove_dir_all(&dir);
            Ok(png)
        }))
    }

    /// ファイル > 書き出し > アニメーション GIF（.gif）: the presentation as an animated GIF picture.
    #[cfg(not(target_family = "wasm"))]
    pub fn write_gif(&self, gif_path: std::path::PathBuf, seconds_per_slide: Option<f32>) -> Result<Box<dyn FnOnce() -> Result<std::path::PathBuf, String> + Send + 'static>, String> {
        if self.jtd_source_path.as_deref().is_some_and(|original| crate::save::same_file(original, &gif_path))
            || (self.is_jtd_read_only() && self.path.as_deref().is_some_and(|original| crate::save::same_file(original, &gif_path))) {
            return Err(crate::i18n::text("file.jtd-source-protected").to_string());
        }
        let bytes = self.current_bytes()?;
        let (stem, ext) = self.name_parts();
        let own_args = with_format!(self, export_args(self));
        let delay_ms = ((seconds_per_slide.unwrap_or(2.0).max(0.1)) * 1000.0) as u64;
        Ok(Box::new(move || {
            let dir = std::env::temp_dir().join(format!("ibisoffice-export-gif-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let out = dir.join("out");
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            let file = dir.join(format!("{stem}.{ext}"));
            std::fs::write(&file, &bytes).map_err(|e| e.to_string())?;
            let program = export_program()?;
            let mut cmd = std::process::Command::new(program);
            cmd.args(["--export", "png", "--out-dir"]).arg(&out);
            cmd.args(&own_args);
            let status = cmd.arg(&file).status().map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(crate::i18n::format("file.png-exit", &[("status", &status)]));
            }
            let mut made: Vec<_> = std::fs::read_dir(&out)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
                .collect();
            made.sort();
            if made.is_empty() {
                return Err(crate::i18n::text("_shell.could-not-create-the-gif").to_string());
            }

            use image::codecs::gif::{GifEncoder, Repeat};
            use image::{Delay, Frame};
            use std::fs::File;

            let file_out = File::create(&gif_path).map_err(|e| e.to_string())?;
            let mut encoder = GifEncoder::new(file_out);
            encoder.set_repeat(Repeat::Infinite).map_err(|e| e.to_string())?;

            for p in &made {
                let img = image::open(p).map_err(|e| e.to_string())?.to_rgba8();
                let frame = Frame::from_parts(img, 0, 0, Delay::from_saturating_duration(std::time::Duration::from_millis(delay_ms)));
                encoder.encode_frame(frame).map_err(|e| e.to_string())?;
            }

            let _ = std::fs::remove_dir_all(&dir);
            Ok(gif_path)
        }))
    }

    /// ファイル > 書き出し > PDF: the file as it is now, as a PDF where the
    /// user says.
    pub fn export_pdf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_pending_edit(window, cx);
        with_format!(self, prepare_print_or_export(self, cx));
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.pdf"), window, cx, |this, path, window, cx| {
                let job = match this.write_pdf(path) {
                    Ok(job) => job,
                    Err(err) => return this.report(Err(crate::i18n::format("file.pdf-failed", &[("error", &err)])), cx),
                };
                this.report(Ok(crate::i18n::text("_shell.exporting-to-pdf").into()), cx);
                let task = cx.background_executor().spawn(async move { job() });
                cx.spawn_in(window, async move |this, cx| {
                    let result = task.await;
                    this.update(cx, |this, cx| this.report(result.map(|p| crate::i18n::format("file.pdf-done", &[("path", &p.display())])).map_err(|e| crate::i18n::format("file.pdf-failed", &[("error", &e)])), cx)).ok();
                })
                .detach();
            });
        }
        #[cfg(target_family = "wasm")]
        {
            // A signed file: the PDF does not carry the signatures, so asked first (a native export asks in `ask_path_as`).
            if self.confirm_signature_loss(crate::signature::SignatureWay::Export, window, cx, |this, window, cx| this.export_pdf(window, cx)) {
                return;
            }
            self.web_export_pdf(window, cx);
        }
    }

    /// ファイル > 書き出し > PNG 画像（.png）: the file as PNG pictures where
    /// the user says.
    pub fn export_png(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_pending_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.png"), window, cx, |this, path, window, cx| {
                let job = match this.write_png(path, None) {
                    Ok(job) => job,
                    Err(err) => return this.report(Err(crate::i18n::format("file.png-failed", &[("error", &err)])), cx),
                };
                this.report(Ok(crate::i18n::text("_shell.exporting-png").into()), cx);
                let task = cx.background_executor().spawn(async move { job() });
                cx.spawn_in(window, async move |this, cx| {
                    let result = task.await;
                    this.update(cx, |this, cx| this.report(result.map(|p| crate::i18n::format("file.png-done", &[("path", &p.display())])).map_err(|e| crate::i18n::format("file.png-failed", &[("error", &e)])), cx)).ok();
                })
                .detach();
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// ファイル > 書き出し > OpenDocument プレゼンテーション（.odp）: the presentation as an OpenDocument presentation.
    pub fn export_odp(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_pending_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.odp"), window, cx, |this, path, _window, cx| {
                let result = ibis_odf::write_odp(this.presentation()).map_err(|e| e.to_string()).and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()));
                #[cfg(target_os = "android")]
                if result.is_ok() {
                    gpui_android::sync_saved_file(&path);
                }
                #[cfg(target_os = "ios")]
                if result.is_ok() {
                    gpui_ios::sync_saved_file(&path);
                }
                this.report(result.map(|()| crate::i18n::format("file.odp-done", &[("path", &path.display())])).map_err(|e| crate::i18n::format("file.odp-failed", &[("error", &e)])), cx);
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// スライド > 現在のスライドを画像として書き出し…: the current slide as a PNG picture.
    pub fn export_current_slide_png(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_pending_edit(window, cx);
        let slide_index = self.presentation().slide_index(self.current_slide).map_or(0, |i| i + 1);
        #[cfg(target_family = "wasm")]
        let _ = slide_index;
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(crate::i18n::format("_shell.stem-slide-slide-index-png", &[("stem", &stem), ("slide_index", &slide_index)]), window, cx, move |this, path, window, cx| {
                let job = match this.write_png(path, Some(slide_index)) {
                    Ok(job) => job,
                    Err(err) => return this.report(Err(crate::i18n::format("file.png-failed", &[("error", &err)])), cx),
                };
                this.report(Ok(crate::i18n::text("_shell.exporting-png").into()), cx);
                let task = cx.background_executor().spawn(async move { job() });
                cx.spawn_in(window, async move |this, cx| {
                    let result = task.await;
                    this.update(cx, |this, cx| this.report(result.map(|p| crate::i18n::format("file.png-done", &[("path", &p.display())])).map_err(|e| crate::i18n::format("file.png-failed", &[("error", &e)])), cx)).ok();
                })
                .detach();
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// スライド > すべてのスライドを画像として書き出し…: all slides as PNG pictures.
    pub fn export_all_slides_png(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.export_png(window, cx);
    }

    /// ファイル > 書き出し > アニメーション GIF（.gif）: save as an animated GIF picture.
    pub fn export_gif(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_pending_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.gif"), window, cx, |this, path, window, cx| {
                let job = match this.write_gif(path, None) {
                    Ok(job) => job,
                    Err(err) => return this.report(Err(crate::i18n::format("file.gif-failed", &[("error", &err)])), cx),
                };
                this.report(Ok(crate::i18n::text("_shell.exporting-gif").into()), cx);
                let task = cx.background_executor().spawn(async move { job() });
                cx.spawn_in(window, async move |this, cx| {
                    let result = task.await;
                    this.update(cx, |this, cx| this.report(result.map(|p| crate::i18n::format("file.gif-done", &[("path", &p.display())])).map_err(|e| crate::i18n::format("file.gif-failed", &[("error", &e)])), cx)).ok();
                })
                .detach();
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// ファイル > 書き出し > PowerPoint スライドショー（.ppsx）: save as a .ppsx slide show.
    pub fn export_ppsx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_pending_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.ppsx"), window, cx, |this, path, _, cx| {
                let source = match this.source.clone() {
                    Some(s) => s,
                    None => return this.report(Err(crate::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string()), cx),
                };
                let result = source.save_slideshow(this.presentation(), ibis_formats::SaveOptions::default())
                    .map(|bytes| this.without_signatures(bytes, true).0)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| crate::save::write_replacing(&path, &bytes).map_err(|e| crate::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)])))
                    .map(|()| crate::i18n::format("file.ppsx-done", &[("path", &path.display())]))
                    .map_err(|e| crate::i18n::format("file.save-failed", &[("error", &e)]));
                this.report(result, cx);
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// ファイル > 書き出し > PowerPoint テンプレート（.potx）: save as a .potx template.
    pub fn export_potx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_pending_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.potx"), window, cx, |this, path, _, cx| {
                let source = match this.source.clone() {
                    Some(s) => s,
                    None => return this.report(Err(crate::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string()), cx),
                };
                let result = source.save_template(this.presentation(), ibis_formats::SaveOptions::default())
                    .map(|bytes| this.without_signatures(bytes, true).0)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| crate::save::write_replacing(&path, &bytes).map_err(|e| crate::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)])))
                    .map(|()| crate::i18n::format("file.potx-done", &[("path", &path.display())]))
                    .map_err(|e| crate::i18n::format("file.save-failed", &[("error", &e)]));
                this.report(result, cx);
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// ファイル > 書き出し > Office テーマ（.thmx）: save the current presentation theme as a .thmx file.
    pub fn export_thmx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_pending_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.thmx"), window, cx, |this, path, _, cx| {
                let result = this.export_theme_to(path.clone())
                    .map(|()| crate::i18n::format("file.thmx-done", &[("path", &path.display())]))
                    .map_err(|e| crate::i18n::format("file.save-failed", &[("error", &e)]));
                this.report(result, cx);
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// テーマファイル（.thmx）を指定パスへ書き出す。
    #[cfg(not(target_family = "wasm"))]
    pub fn export_theme_to(&mut self, path: std::path::PathBuf) -> Result<(), String> {
        let source = self.source.clone().ok_or_else(|| crate::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string())?;
        let theme_name = path.file_stem().and_then(|s| s.to_str());
        let bytes = source.save_theme(self.presentation(), theme_name).map_err(|e| e.to_string())?;
        crate::save::write_replacing(&path, &bytes).map_err(|e| crate::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)]))?;
        Ok(())
    }

    /// ファイル > 書き出し > Excel テンプレート（.xltx）
    pub fn export_xltx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_pending_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.xltx"), window, cx, |this, path, _, cx| {
                let source = match this.source.clone() {
                    Some(s) => s,
                    None => return this.report(Err(crate::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string()), cx),
                };
                let result = source.save_template(this.presentation(), ibis_formats::SaveOptions::default())
                    .map(|bytes| this.without_signatures(bytes, true).0)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| crate::save::write_replacing(&path, &bytes).map_err(|e| crate::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)])))
                    .map(|()| crate::i18n::format("file.xltx-done", &[("path", &path.display())]))
                    .map_err(|e| crate::i18n::format("file.save-failed", &[("error", &e)]));
                this.report(result, cx);
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// ファイル > 書き出し > マクロなしで書き出し: the file without its macros
    /// (`.xlsx` from an `.xlsm`, `.xltx` from an `.xltm`, …), the loss told in the message.
    pub fn export_without_macros(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_pending_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, extension) = self.name_parts();
            let plain = match extension.to_ascii_lowercase().as_str() {
                "xlsm" => "xlsx",
                "xltm" => "xltx",
                "docm" => "docx",
                "dotm" => "dotx",
                "pptm" => "pptx",
                "ppsm" => "ppsx",
                "potm" => "potx",
                other => if other.is_empty() { "xlsx" } else { other },
            }
            .to_string();
            self.ask_path(format!("{stem}.{plain}"), window, cx, |this, path, _, cx| {
                let source = match this.source.clone() {
                    Some(s) => s,
                    None => return this.report(Err(crate::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string()), cx),
                };
                let result = source.save_without_macros(this.presentation(), ibis_formats::SaveOptions::default())
                    .map(|bytes| this.without_signatures(bytes, true).0)
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| crate::save::write_replacing(&path, &bytes).map_err(|e| crate::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)])))
                    .map(|()| crate::i18n::format("file.without-macros-done", &[("path", &path.display())]))
                    .map_err(|e| crate::i18n::format("file.save-failed", &[("error", &e)]));
                this.report(result, cx);
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// ファイル > 印刷… (⌘P): the file as it is now, as a PDF given to the
    /// system to print (Preview's print dialog on macOS).
    pub fn print(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A host that gave no printPdf (or switched `features.print` off) has no printing (the menu item is disabled too).
        if !crate::host::print_allowed() {
            return;
        }
        // Each format asks what to print in its own way (a presentation: 印刷の設定; a document and a workbook: their print panels).
        if with_format!(self, open_print_panel(self, window, cx)) {
            return;
        }
        self.print_now(window, cx);
    }

    /// The print made, with the settings of a presentation.
    pub fn print_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_pending_edit(window, cx);
        with_format!(self, prepare_print_or_export(self, cx));
        #[cfg(target_os = "macos")]
        {
            let (stem, _) = self.name_parts();
            let pdf = std::env::temp_dir().join(format!("{stem}.pdf"));
            let job = match self.write_pdf_with(pdf, true) {
                Ok(job) => job,
                Err(err) => return self.report(Err(crate::i18n::format("file.print-failed", &[("error", &err)])), cx),
            };
            self.report(Ok(crate::i18n::text("_shell.preparing-to-print").into()), cx);
            let task = cx.background_executor().spawn(async move {
                let pdf = job()?;
                // Through the gate: a test run opens no Preview (external.rs).
                if !crate::external::may_launch(|| format!("print {} in Preview", pdf.display())) {
                    return Ok(false);
                }
                let script = format!("tell application \"Preview\"\nactivate\nopen POSIX file \"{}\"\nprint front document with print dialog\nend tell", pdf.display().to_string().replace('"', "\\\""));
                // Preview's print dialog through Apple Events. macOS asks the user once
                // to let ibisOffice control Preview (NSAppleEventsUsageDescription);
                // if that is refused (or fails), the PDF is opened in Preview instead,
                // to print with ⌘P there.
                let status = std::process::Command::new("osascript").args(["-e", &script]).output().map_err(|e| e.to_string())?;
                if status.status.success() {
                    return Ok(false);
                }
                log::warn!("print through Preview failed: {}", String::from_utf8_lossy(&status.stderr));
                let opened = std::process::Command::new("open").args(["-a", "Preview"]).arg(&pdf).status().map_err(|e| e.to_string())?;
                if opened.success() { Ok(true) } else { Err(crate::i18n::format("file.print-exit", &[("status", &status.status)])) }
            });
            cx.spawn_in(window, async move |this, cx| {
                let result = task.await;
                this.update(cx, |this, cx| this.report(result.map(|in_preview| crate::i18n::text(if in_preview { "fix.file-actions.opened-preview-press-p-print" } else { "_shell.opened-the-print-dialog" }).to_string()).map_err(|e| crate::i18n::format("file.print-failed", &[("error", &e)])), cx)).ok();
            })
            .detach();
        }
        #[cfg(all(not(target_os = "macos"), not(target_family = "wasm")))]
        {
            let (stem, _) = self.name_parts();
            let pdf = std::env::temp_dir().join(format!("{stem}.pdf"));
            let job = match self.write_pdf_with(pdf, true) {
                Ok(job) => job,
                Err(err) => return self.report(Err(crate::i18n::format("file.print-failed", &[("error", &err)])), cx),
            };
            self.report(Ok(crate::i18n::text("_shell.preparing-to-print").into()), cx);
            let task = cx.background_executor().spawn(async move { job() });
            cx.spawn_in(window, async move |this, cx| {
                let result = task.await;
                this.update(cx, |this, cx| {
                    let msg = result.and_then(|pdf| {
                        open_pdf_in_default_app(&pdf, cx)?;
                        Ok(crate::i18n::text("file.print-opened-pdf").to_string())
                    });
                    this.report(msg.map_err(|e| crate::i18n::format("file.print-failed", &[("error", &e)])), cx);
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        self.web_print_pdf(window, cx);
    }
}

/// Whether this is a test run (it must not open a PDF app, nor a print dialog): the gate's (`external.rs`).
#[cfg(not(target_family = "wasm"))]
pub fn running_under_test() -> bool {
    crate::external::running_under_test()
}

#[cfg(all(test, not(target_family = "wasm")))]
mod running_under_test_tests {
    use crate::external::is_test_program_name as is_test_binary_name;

    #[test]
    fn the_app_is_not_a_test_binary_whatever_file_it_opens() {
        // The program's name is the app's; its arguments (a file named latest.docx, a folder named test) are not looked at.
        for name in ["ibisOffice.exe", "ibisOffice", "ibisOffice-debug.exe"] {
            assert!(!is_test_binary_name(name), "{name}");
        }
    }

    #[test]
    fn test_binaries_are_told_by_their_name() {
        for name in ["ibis_office_app-0123456789abcdef.exe", "scenarios-0123456789abcdef", "clock_rule-1a2b3c.exe"] {
            assert!(is_test_binary_name(name), "{name}");
        }
    }
}

/// Open a PDF in the system's PDF viewer (printing on Windows and Linux; macOS prints through Preview instead).
#[cfg(all(not(target_os = "macos"), not(target_family = "wasm")))]
pub fn open_pdf_in_default_app(pdf: &std::path::Path, cx: &mut App) -> Result<(), String> {
    if !crate::external::may_launch(|| format!("open {} in the PDF app", pdf.display())) {
        let _ = (pdf, cx);
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        cx.open_url(&pdf.to_string_lossy());
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = cx;
        std::process::Command::new("xdg-open").arg(pdf).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

#[cfg(not(target_family = "wasm"))]
pub fn export_program() -> Result<std::path::PathBuf, String> {
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    let file_name = current.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if file_name.starts_with("ibisOffice") && !file_name.contains("test") {
        return Ok(current);
    }
    // Next to this program, or in a folder above it: a scenarios binary built by cargo's build script lives in
    // `target/release/build/<crate>/<hash>/out/` (Windows), the program in `target/release/` five folders up.
    for dir in current.ancestors().skip(1).take(6) {
        for name in ["ibisOffice", "ibisOffice.exe"] {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    // No ibisOffice next to this program. A test binary (cargo names it `<name>-<16 hex digits>`, e.g. a scenarios binary copied out of `target/release/deps`)
    // would start itself again with `--export`, each run starting the next and none finishing (merge014), so that is an error;
    // any other program (an ibisOffice the user renamed) is itself.
    let stem = file_name.trim_end_matches(".exe");
    let cargo_test_binary = stem.rsplit_once('-').is_some_and(|(_, h)| h.len() == 16 && h.bytes().all(|b| b.is_ascii_hexdigit()));
    if cargo_test_binary || file_name.contains("test") {
        return Err(format!("the ibisOffice program is not next to {}", current.display()));
    }
    Ok(current)
}


#[cfg(not(target_family = "wasm"))]
fn i18n_status(on: bool) -> String {
    crate::i18n::text(if on { "_shell.saved-with-password" } else { "_shell.saved-without-password" }).to_string()
}
