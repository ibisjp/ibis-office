// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ファイル > 新規作成 (⌘N for a document) and the file actions every
//! format shares (コピーを作成…, 書き出し, 印刷…).
//!
//! A new file is an empty package written by `ibis_pptx::blank_file`,
//! opened as a file is (`ibis_pptx::open`), so it saves as an opened file
//! does; it has no path until saved (⌘S asks where).

use gpui_kit::*;

use crate::shell::editor::SlideEditor;

impl SlideEditor {
    /// A new, empty file of `kind`: in this window when it shows no
    /// document, else in a new window.
    pub(crate) fn new_file(&mut self, kind: ibis_pptx::Kind, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        let opened = match ibis_pptx::blank_file(kind).and_then(ibis_pptx::open) {
            Ok(opened) => opened,
            Err(err) => {
                self.status = Some(crate::shell::i18n::format("file.new-failed", &[("error", &err)]).into());
                return cx.notify();
            }
        };
        let name: SharedString = new_file_name(kind).into();
        if self.has_document {
            crate::open_window_with(cx, move |editor, window, cx| editor.show_new(opened, name, window, cx));
        } else {
            self.show_new(opened, name, window, cx);
        }
    }

    /// Show `opened`, a new file named `name` that has no path yet.
    fn show_new(&mut self, opened: ibis_pptx::Opened, name: SharedString, window: &mut Window, cx: &mut Context<Self>) {
        self.load(opened.presentation, name, window, cx);
        self.source = Some(std::sync::Arc::new(opened.source));
        if self.format.kind() == ibis_pptx::Kind::Document {
            self.apply_docx_default_design(cx);
        }
        self.document.reset_history();
        // A new presentation opens without the speaker notes (the title row's notes button shows them).
        if let Some(state) = self.pptx_mut() {
            state.notes_hidden = true;
        }
    }

    /// ファイル > 新規 > テンプレートから新規作成… (New from Template...)
    pub(crate) fn new_from_template(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let picked = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false, extensions: Vec::new(),
                multiple: false,
                prompt: Some(crate::shell::i18n::text("pptx.new-from-template").into()),
            });
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(paths))) = picked.await else { return };
                let Some(path) = paths.into_iter().next() else { return };
                this.update_in(cx, |this, window, cx| this.new_from_template_path(path, window, cx)).ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.status = Some(crate::shell::i18n::text("_shell.not-available-in-the-browser-version-yet").into());
            cx.notify();
        }
    }

    /// テンプレートファイル `path` を読み込み、未保存の新規プレゼンテーションとして開く。
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn new_from_template_path(&mut self, path: std::path::PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        let read_path = path.clone();
        let tmpl_name: SharedString = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().into();
        self.set_status(crate::shell::i18n::format("file.opening", &[("name", &tmpl_name)]), cx);
        let closed = self.closed;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move { crate::read_file(&read_path) }).await;
            this.update_in(cx, |this, window, cx| match result {
                _ if this.closed != closed => {}
                Ok(loaded) => {
                    let name: SharedString = new_file_name(ibis_pptx::Kind::of(&loaded.presentation));
                    this.load(loaded.presentation, name, window, cx);
                    this.source = Some(loaded.source);
                    this.path = None;
                    this.document.set_file(None);
                    this.document.reset_history();
                    this.recent_files = crate::shell::recent_files::add(&path);
                    this.set_status(crate::shell::i18n::text("file.new-from-template-done").to_string(), cx);
                    crate::timing::document_loaded(window);
                }
                Err(err) => {
                    this.fail_loading(crate::shell::i18n::format("file.open-failed", &[("name", &tmpl_name), ("error", &err)]), cx);
                }
            })
            .ok();
        })
        .detach();
    }
}

/// The name a new file shows until it is saved.
pub(crate) fn new_file_name(kind: ibis_pptx::Kind) -> SharedString {
    match kind {
        ibis_pptx::Kind::Document => crate::shell::i18n::text("_shell.document-1-docx"),
        ibis_pptx::Kind::Workbook => "Book1.xlsx".into(),
        ibis_pptx::Kind::Presentation => crate::shell::i18n::text("_shell.presentation-1-pptx"),
    }
}

impl SlideEditor {
    /// The open file as it would be saved now.
    #[cfg(not(target_family = "wasm"))]
    fn current_bytes(&self) -> Result<Vec<u8>, String> {
        let source = self.source.clone().ok_or(crate::shell::i18n::text("_shell.this-file-cannot-be-saved-yet").to_string())?;
        source.save(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())
    }

    /// Write the file as it is now to `path`, leaving the file's own path.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn copy_to(&self, path: &std::path::Path) -> Result<(), String> {
        let source = self.source.clone().ok_or(crate::shell::i18n::text("_shell.this-file-cannot-be-saved-yet").to_string())?;
        let csv = self.is_workbook() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("csv"));
        let ods = self.is_workbook() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ods"));
        let odt = self.is_document() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("odt"));
        let odp = self.is_presentation() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("odp"));
        let xls = self.is_workbook() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xls"));
        let xltx = self.is_workbook() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("xltx") || e.eq_ignore_ascii_case("xltm"));
        let ppsx = self.is_presentation() && path.extension().and_then(|e| e.to_str()).is_some_and(ibis_pptx::is_slideshow);
        let potx = self.is_presentation() && path.extension().and_then(|e| e.to_str()).is_some_and(ibis_pptx::is_template);
        let dotx = self.is_document() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("dotx") || e.eq_ignore_ascii_case("dotm"));
        let pptx = self.is_presentation() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pptx") || e.eq_ignore_ascii_case("pptm"));
        let bytes = if csv {
            let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
            ibis_pptx::csv::sheet_to_csv(self.presentation(), index)
        } else if ods {
            ibis_pptx::write_ods_with_source(self.presentation(), &source).map_err(|e| e.to_string())?
        } else if odt {
            ibis_pptx::docx_convert::export_docx_to_odt(self.presentation()).map_err(|e| e.to_string())?
        } else if odp {
            ibis_pptx::write_odp(self.presentation()).map_err(|e| e.to_string())?
        } else if xls {
            ibis_pptx::write_xls(self.presentation()).map_err(|e| e.to_string())?
        } else if xltx || potx || dotx {
            source.save_template(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?
        } else if ppsx {
            source.save_slideshow(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?
        } else if pptx {
            source.save_presentation(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?
        } else {
            source.save(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?
        };
        // A copy keeps the protection of the file.
        let bytes = match (&self.protection_password, csv || ods || xls || odt || odp) {
            (Some(password), false) => crate::shell::save::protect(&bytes, password)?,
            _ => bytes,
        };
        crate::shell::save::write_replacing(path, &bytes).map_err(|e| crate::shell::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)]))
    }

    /// ファイル > パスワードで保護…: the window that sets the password (or takes it off).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn protect_with_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let had = self.protection_password.is_some();
        crate::shell::password_window::ask_to_set(cx.entity().downgrade(), had, window, cx);
    }

    /// (The Web version does not protect files with a password yet.)
    #[cfg(target_family = "wasm")]
    pub(crate) fn protect_with_password(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_status(crate::shell::i18n::text("_shell.not-available-in-the-browser-version-yet").to_string(), cx);
    }

    /// The password of the file set (`Some`) or taken off (`None`), and the file saved so (the
    /// password is in what is saved).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn set_protection(&mut self, password: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let on = password.is_some();
        self.protection_password = password;
        match self.path.clone() {
            Some(path) => {
                let result = self.save_to(path, window);
                let message = match result {
                    Ok(()) => i18n_status(on),
                    Err(err) => crate::shell::i18n::format("file.save-failed", &[("error", &err)]),
                };
                self.set_status(message, cx);
            }
            None => self.set_status(crate::shell::i18n::text("_shell.the-password-is-applied-when-you-save-the-file").to_string(), cx),
        }
    }

    /// The file's name without its extension, and its extension.
    pub(crate) fn name_parts(&self) -> (String, String) {
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
        let directory = self
            .path
            .as_deref()
            .and_then(|p| p.parent())
            .map(std::path::Path::to_path_buf)
            .or_else(|| std::env::var_os("HOME").map(Into::into))
            .unwrap_or_else(|| ".".into());
        let picked = cx.prompt_for_new_path(&directory, Some(&name));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else { return };
            this.update_in(cx, |this, window, cx| then(this, path, window, cx)).ok();
        })
        .detach();
    }

    pub(crate) fn report(&mut self, result: Result<String, String>, cx: &mut Context<Self>) {
        self.status = Some(match result {
            Ok(done) => done.into(),
            Err(err) => err.into(),
        });
        cx.notify();
    }

    /// ファイル > コピーを作成…: save the file as it is now to another
    /// path; the window keeps the file's own path.
    pub(crate) fn save_copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, ext) = self.name_parts();
            self.ask_path(crate::shell::i18n::format("file.copy-name", &[("name", &stem), ("ext", &ext)]), window, cx, |this, path, _, cx| {
                let result = this.copy_to(&path).map(|()| crate::shell::i18n::text("_shell.copy-saved").to_string()).map_err(|e| crate::shell::i18n::format("file.copy-failed", &[("error", &e)]));
                this.report(result, cx);
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::shell::i18n::text("_shell.saving-is-not-yet-available-in-the-browser-versi").into()), cx);
    }

    /// The file as it is now, as a PDF at `pdf`: saved to a temporary file
    /// and drawn by this program's `--export pdf` (on another thread).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn write_pdf(&self, pdf: std::path::PathBuf) -> Result<Box<dyn FnOnce() -> Result<std::path::PathBuf, String> + Send + 'static>, String> {
        self.write_pdf_with(pdf, false)
    }

    /// As `write_pdf`; `printing` makes a presentation's PDF as 印刷の設定 say
    /// (the slides, handouts, colour, frame).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn write_pdf_with(&self, pdf: std::path::PathBuf, printing: bool) -> Result<Box<dyn FnOnce() -> Result<std::path::PathBuf, String> + Send + 'static>, String> {
        let (bytes, slides, docx_print_args) = if self.presentation().flow.is_some() {
            let b = self.current_bytes()?;
            if printing {
                let (pages, args) = self.docx_print_job_args();
                (b, pages, args)
            } else {
                (b, None, None)
            }
        } else if self.is_workbook() {
            let print_what = self.wb().print_what;
            let current_idx = self.presentation().slide_index(self.current_slide).unwrap_or(0);
            match print_what {
                crate::xlsx::PrintWhat::ActiveSheet => {
                    let b = self.current_bytes()?;
                    (b, Some(vec![current_idx + 1]), None)
                }
                crate::xlsx::PrintWhat::EntireWorkbook => {
                    let b = self.current_bytes()?;
                    (b, None, None)
                }
                crate::xlsx::PrintWhat::Selection => {
                    let range = self.selected_range().or_else(|| self.current_cell().map(|(c, r)| ibis_doc_model::CellRange { first_row: r, first_column: c, last_row: r, last_column: c }));
                    let mut p = self.presentation().clone();
                    if let Some(r) = range {
                        if let Some(slide) = p.slide_mut(self.current_slide) {
                            for shape in &mut slide.shapes {
                                if let ibis_doc_model::ShapeKind::Sheet(sheet) = &mut shape.kind {
                                    let sheet = std::sync::Arc::make_mut(sheet);
                                    sheet.print_area = Some(r);
                                }
                            }
                        }
                    }
                    let source = self.source.clone().ok_or(crate::shell::i18n::text("_shell.this-file-cannot-be-saved-yet").to_string())?;
                    let b = source.save(&p, ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?;
                    (b, Some(vec![current_idx + 1]), None)
                }
            }
        } else {
            let b = self.current_bytes()?;
            (b, None, None)
        };
        let (stem, ext) = self.name_parts();
        let print_bg = self.is_print_backgrounds();
        let print_hidden = self.is_print_hidden_text();
        let print_markup = self.is_print_markup();
        // A presentation's print: the settings' slides, and pictures when the
        // colour, a frame or handouts ask for them.
        let settings = (printing && self.pages_are_slides() && !self.is_workbook() && self.presentation().flow.is_none()).then(|| self.print_settings());
        let (format, print_args) = if let Some(args) = docx_print_args {
            let format = if args.iter().any(|a| a == "pdf-image") { "pdf-image" } else { "pdf" };
            let filtered: Vec<String> = args.into_iter().filter(|a| a != "--export" && a != "pdf-image").collect();
            (format, Some(filtered))
        } else {
            match &settings {
                Some(settings) => {
                    let current = self.presentation().slide_index(self.current_slide).map_or(1, |i| i + 1);
                    (if settings.needs_pictures() { "pdf-image" } else { "pdf" }, Some(settings.arguments(current, &self.presentation().notes_marks)))
                }
                None => ("pdf", None),
            }
        };
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
                cmd.args(["--slides", &list]);
            }
            if print_bg {
                cmd.arg("--print-backgrounds");
            }
            if print_hidden {
                cmd.arg("--print-hidden-text");
            }
            if !print_markup {
                cmd.arg("--no-print-markup");
            }
            cmd.arg(&file);
            let status = cmd.status().map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(crate::shell::i18n::format("file.pdf-exit", &[("status", &status)]));
            }
            let made = std::fs::read_dir(&out)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")))
                .ok_or(crate::shell::i18n::text("_shell.could-not-create-the-pdf").to_string())?;
            let data = std::fs::read(&made).map_err(|e| e.to_string())?;
            let mut dest = pdf;
            if let Err(e) = crate::shell::save::write_replacing(&dest, &data) {
                if !printing {
                    return Err(crate::shell::i18n::format("file.cannot-write", &[("path", &dest.display()), ("error", &e)]));
                }
                let parent = dest.parent().map(std::path::Path::to_path_buf).unwrap_or_else(std::env::temp_dir);
                let mut wrote = false;
                for n in 1..=99 {
                    let alt = parent.join(format!("{stem}-{n}.pdf"));
                    if crate::shell::save::write_replacing(&alt, &data).is_ok() {
                        dest = alt;
                        wrote = true;
                        break;
                    }
                }
                if !wrote {
                    return Err(crate::shell::i18n::format("file.cannot-write", &[("path", &dest.display()), ("error", &e)]));
                }
            }
            let _ = std::fs::remove_dir_all(&dir);
            Ok(dest)
        }))
    }

    /// The file as it is now, as PNG pictures at `png`: saved to a temporary file
    /// and drawn by this program's `--export png` (on another thread).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn write_png(&self, png: std::path::PathBuf, slide: Option<usize>) -> Result<Box<dyn FnOnce() -> Result<std::path::PathBuf, String> + Send + 'static>, String> {
        let bytes = self.current_bytes()?;
        let (stem, ext) = self.name_parts();
        let print_bg = self.is_print_backgrounds();
        let print_hidden = self.is_print_hidden_text();
        let print_markup = self.is_print_markup();
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
                cmd.args(["--slides", &num.to_string()]);
            }
            if print_bg {
                cmd.arg("--print-backgrounds");
            }
            if print_hidden {
                cmd.arg("--print-hidden-text");
            }
            if !print_markup {
                cmd.arg("--no-print-markup");
            }
            let status = cmd.arg(&file).status().map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(crate::shell::i18n::format("file.png-exit", &[("status", &status)]));
            }
            let mut made: Vec<_> = std::fs::read_dir(&out)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
                .collect();
            made.sort();
            if made.is_empty() {
                return Err(crate::shell::i18n::text("_shell.could-not-create-the-png").to_string());
            }
            if made.len() == 1 {
                let data = std::fs::read(&made[0]).map_err(|e| e.to_string())?;
                crate::shell::save::write_replacing(&png, &data).map_err(|e| crate::shell::i18n::format("file.cannot-write", &[("path", &png.display()), ("error", &e)]))?;
            } else {
                let parent = png.parent().unwrap_or_else(|| std::path::Path::new("."));
                let target_stem = png.file_stem().and_then(|s| s.to_str()).unwrap_or(&stem);
                for (idx, p) in made.iter().enumerate() {
                    let dest = parent.join(format!("{target_stem}-{:02}.png", idx + 1));
                    let data = std::fs::read(p).map_err(|e| e.to_string())?;
                    crate::shell::save::write_replacing(&dest, &data).map_err(|e| crate::shell::i18n::format("file.cannot-write", &[("path", &dest.display()), ("error", &e)]))?;
                }
            }
            let _ = std::fs::remove_dir_all(&dir);
            Ok(png)
        }))
    }

    /// ファイル > 書き出し > アニメーション GIF（.gif）: the presentation as an animated GIF picture.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn write_gif(&self, gif_path: std::path::PathBuf, seconds_per_slide: Option<f32>) -> Result<Box<dyn FnOnce() -> Result<std::path::PathBuf, String> + Send + 'static>, String> {
        let bytes = self.current_bytes()?;
        let (stem, ext) = self.name_parts();
        let print_bg = self.is_print_backgrounds();
        let print_hidden = self.is_print_hidden_text();
        let print_markup = self.is_print_markup();
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
            if print_bg {
                cmd.arg("--print-backgrounds");
            }
            if print_hidden {
                cmd.arg("--print-hidden-text");
            }
            if !print_markup {
                cmd.arg("--no-print-markup");
            }
            let status = cmd.arg(&file).status().map_err(|e| e.to_string())?;
            if !status.success() {
                return Err(crate::shell::i18n::format("file.png-exit", &[("status", &status)]));
            }
            let mut made: Vec<_> = std::fs::read_dir(&out)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
                .collect();
            made.sort();
            if made.is_empty() {
                return Err(crate::shell::i18n::text("_shell.could-not-create-the-gif").to_string());
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
    pub(crate) fn export_pdf(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        self.prepare_print_or_export(cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.pdf"), window, cx, |this, path, window, cx| {
                let job = match this.write_pdf(path) {
                    Ok(job) => job,
                    Err(err) => return this.report(Err(crate::shell::i18n::format("file.pdf-failed", &[("error", &err)])), cx),
                };
                this.report(Ok(crate::shell::i18n::text("_shell.exporting-to-pdf").into()), cx);
                let task = cx.background_executor().spawn(async move { job() });
                cx.spawn_in(window, async move |this, cx| {
                    let result = task.await;
                    this.update(cx, |this, cx| this.report(result.map(|p| crate::shell::i18n::format("file.pdf-done", &[("path", &p.display())])).map_err(|e| crate::shell::i18n::format("file.pdf-failed", &[("error", &e)])), cx)).ok();
                })
                .detach();
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// ファイル > 書き出し > PNG 画像（.png）: the file as PNG pictures where
    /// the user says.
    pub(crate) fn export_png(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.png"), window, cx, |this, path, window, cx| {
                let job = match this.write_png(path, None) {
                    Ok(job) => job,
                    Err(err) => return this.report(Err(crate::shell::i18n::format("file.png-failed", &[("error", &err)])), cx),
                };
                this.report(Ok(crate::shell::i18n::text("_shell.exporting-png").into()), cx);
                let task = cx.background_executor().spawn(async move { job() });
                cx.spawn_in(window, async move |this, cx| {
                    let result = task.await;
                    this.update(cx, |this, cx| this.report(result.map(|p| crate::shell::i18n::format("file.png-done", &[("path", &p.display())])).map_err(|e| crate::shell::i18n::format("file.png-failed", &[("error", &e)])), cx)).ok();
                })
                .detach();
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// ファイル > 書き出し > OpenDocument プレゼンテーション（.odp）: the presentation as an OpenDocument presentation.
    pub(crate) fn export_odp(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.odp"), window, cx, |this, path, _window, cx| {
                let result = ibis_pptx::write_odp(this.presentation()).map_err(|e| e.to_string()).and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()));
                this.report(result.map(|()| crate::shell::i18n::format("file.odp-done", &[("path", &path.display())])).map_err(|e| crate::shell::i18n::format("file.odp-failed", &[("error", &e)])), cx);
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// スライド > 現在のスライドを画像として書き出し…: the current slide as a PNG picture.
    pub(crate) fn export_current_slide_png(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        let slide_index = self.presentation().slide_index(self.current_slide).map_or(0, |i| i + 1);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(crate::shell::i18n::format("_shell.stem-slide-slide-index-png", &[("stem", &stem), ("slide_index", &slide_index)]), window, cx, move |this, path, window, cx| {
                let job = match this.write_png(path, Some(slide_index)) {
                    Ok(job) => job,
                    Err(err) => return this.report(Err(crate::shell::i18n::format("file.png-failed", &[("error", &err)])), cx),
                };
                this.report(Ok(crate::shell::i18n::text("_shell.exporting-png").into()), cx);
                let task = cx.background_executor().spawn(async move { job() });
                cx.spawn_in(window, async move |this, cx| {
                    let result = task.await;
                    this.update(cx, |this, cx| this.report(result.map(|p| crate::shell::i18n::format("file.png-done", &[("path", &p.display())])).map_err(|e| crate::shell::i18n::format("file.png-failed", &[("error", &e)])), cx)).ok();
                })
                .detach();
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// スライド > すべてのスライドを画像として書き出し…: all slides as PNG pictures.
    pub(crate) fn export_all_slides_png(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.export_png(window, cx);
    }

    /// ファイル > 書き出し > アニメーション GIF（.gif）: save as an animated GIF picture.
    pub(crate) fn export_gif(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.gif"), window, cx, |this, path, window, cx| {
                let job = match this.write_gif(path, None) {
                    Ok(job) => job,
                    Err(err) => return this.report(Err(crate::shell::i18n::format("file.gif-failed", &[("error", &err)])), cx),
                };
                this.report(Ok(crate::shell::i18n::text("_shell.exporting-gif").into()), cx);
                let task = cx.background_executor().spawn(async move { job() });
                cx.spawn_in(window, async move |this, cx| {
                    let result = task.await;
                    this.update(cx, |this, cx| this.report(result.map(|p| crate::shell::i18n::format("file.gif-done", &[("path", &p.display())])).map_err(|e| crate::shell::i18n::format("file.gif-failed", &[("error", &e)])), cx)).ok();
                })
                .detach();
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// ファイル > 書き出し > PowerPoint スライドショー（.ppsx）: save as a .ppsx slide show.
    pub(crate) fn export_ppsx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.ppsx"), window, cx, |this, path, _, cx| {
                let source = match this.source.clone() {
                    Some(s) => s,
                    None => return this.report(Err(crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string()), cx),
                };
                let result = source.save_slideshow(this.presentation(), ibis_pptx::SaveOptions::default())
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| crate::shell::save::write_replacing(&path, &bytes).map_err(|e| crate::shell::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)])))
                    .map(|()| crate::shell::i18n::format("file.ppsx-done", &[("path", &path.display())]))
                    .map_err(|e| crate::shell::i18n::format("file.save-failed", &[("error", &e)]));
                this.report(result, cx);
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// ファイル > 書き出し > PowerPoint テンプレート（.potx）: save as a .potx template.
    pub(crate) fn export_potx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.potx"), window, cx, |this, path, _, cx| {
                let source = match this.source.clone() {
                    Some(s) => s,
                    None => return this.report(Err(crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string()), cx),
                };
                let result = source.save_template(this.presentation(), ibis_pptx::SaveOptions::default())
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| crate::shell::save::write_replacing(&path, &bytes).map_err(|e| crate::shell::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)])))
                    .map(|()| crate::shell::i18n::format("file.potx-done", &[("path", &path.display())]))
                    .map_err(|e| crate::shell::i18n::format("file.save-failed", &[("error", &e)]));
                this.report(result, cx);
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// ファイル > 書き出し > Office テーマ（.thmx）: save the current presentation theme as a .thmx file.
    pub(crate) fn export_thmx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.thmx"), window, cx, |this, path, _, cx| {
                let result = this.export_theme_to(path.clone())
                    .map(|()| crate::shell::i18n::format("file.thmx-done", &[("path", &path.display())]))
                    .map_err(|e| crate::shell::i18n::format("file.save-failed", &[("error", &e)]));
                this.report(result, cx);
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// テーマファイル（.thmx）を指定パスへ書き出す。
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn export_theme_to(&mut self, path: std::path::PathBuf) -> Result<(), String> {
        let source = self.source.clone().ok_or_else(|| crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string())?;
        let theme_name = path.file_stem().and_then(|s| s.to_str());
        let bytes = source.save_theme(self.presentation(), theme_name).map_err(|e| e.to_string())?;
        crate::shell::save::write_replacing(&path, &bytes).map_err(|e| crate::shell::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)]))?;
        Ok(())
    }

    /// ファイル > 書き出し > Excel テンプレート（.xltx）
    pub(crate) fn export_xltx(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            let (stem, _) = self.name_parts();
            self.ask_path(format!("{stem}.xltx"), window, cx, |this, path, _, cx| {
                let source = match this.source.clone() {
                    Some(s) => s,
                    None => return this.report(Err(crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string()), cx),
                };
                let result = source.save_template(this.presentation(), ibis_pptx::SaveOptions::default())
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| crate::shell::save::write_replacing(&path, &bytes).map_err(|e| crate::shell::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)])))
                    .map(|()| crate::shell::i18n::format("file.xltx-done", &[("path", &path.display())]))
                    .map_err(|e| crate::shell::i18n::format("file.save-failed", &[("error", &e)]));
                this.report(result, cx);
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// ファイル > 書き出し > マクロなしで書き出し: the file without its macros
    /// (`.xlsx` from an `.xlsm`, `.xltx` from an `.xltm`, …), the loss told in the message.
    pub(crate) fn export_without_macros(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
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
                    None => return this.report(Err(crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string()), cx),
                };
                let result = source.save_without_macros(this.presentation(), ibis_pptx::SaveOptions::default())
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| crate::shell::save::write_replacing(&path, &bytes).map_err(|e| crate::shell::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)])))
                    .map(|()| crate::shell::i18n::format("file.without-macros-done", &[("path", &path.display())]))
                    .map_err(|e| crate::shell::i18n::format("file.save-failed", &[("error", &e)]));
                this.report(result, cx);
            });
        }
        #[cfg(target_family = "wasm")]
        self.report(Err(crate::shell::i18n::text("_shell.exporting-is-not-yet-available-in-the-browser-ve").into()), cx);
    }

    /// ファイル > 印刷… (⌘P): the file as it is now, as a PDF given to the
    /// system to print (Preview's print dialog on macOS).
    pub(crate) fn print(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A presentation first asks what to print (印刷の設定).
        if self.pages_are_slides() && !self.is_workbook() && self.presentation().flow.is_none() {
            self.commit_edit(window, cx);
            return self.open_print_panel(window, cx);
        }
        if self.presentation().flow.is_some() {
            self.commit_edit(window, cx);
            return self.open_doc_print_panel(window, cx);
        }
        self.print_now(window, cx);
    }

    /// The print made, with the settings of a presentation.
    pub(crate) fn print_now(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        self.prepare_print_or_export(cx);
        #[cfg(target_os = "macos")]
        {
            let (stem, _) = self.name_parts();
            let pdf = std::env::temp_dir().join(format!("{stem}.pdf"));
            let job = match self.write_pdf_with(pdf, true) {
                Ok(job) => job,
                Err(err) => return self.report(Err(crate::shell::i18n::format("file.print-failed", &[("error", &err)])), cx),
            };
            self.report(Ok(crate::shell::i18n::text("_shell.preparing-to-print").into()), cx);
            let task = cx.background_executor().spawn(async move {
                let pdf = job()?;
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
                if opened.success() { Ok(true) } else { Err(crate::shell::i18n::format("file.print-exit", &[("status", &status.status)])) }
            });
            cx.spawn_in(window, async move |this, cx| {
                let result = task.await;
                this.update(cx, |this, cx| this.report(result.map(|in_preview| crate::shell::i18n::text(if in_preview { "fix.file-actions.opened-preview-press-p-print" } else { "_shell.opened-the-print-dialog" }).to_string()).map_err(|e| crate::shell::i18n::format("file.print-failed", &[("error", &e)])), cx)).ok();
            })
            .detach();
        }
        #[cfg(all(not(target_os = "macos"), not(target_family = "wasm")))]
        {
            let (stem, _) = self.name_parts();
            let pdf = std::env::temp_dir().join(format!("{stem}.pdf"));
            let job = match self.write_pdf_with(pdf, true) {
                Ok(job) => job,
                Err(err) => return self.report(Err(crate::shell::i18n::format("file.print-failed", &[("error", &err)])), cx),
            };
            self.report(Ok(crate::shell::i18n::text("_shell.preparing-to-print").into()), cx);
            let task = cx.background_executor().spawn(async move { job() });
            cx.spawn_in(window, async move |this, cx| {
                let result = task.await;
                this.update(cx, |this, cx| {
                    let msg = result.and_then(|pdf| {
                        open_pdf_in_default_app(&pdf, cx)?;
                        Ok(crate::shell::i18n::text("file.print-opened-pdf").to_string())
                    });
                    this.report(msg.map_err(|e| crate::shell::i18n::format("file.print-failed", &[("error", &e)])), cx);
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.report(Err(crate::shell::i18n::text("_shell.printing-is-not-yet-available-in-this-environmen").into()), cx);
        }
    }
}

/// Whether this is a test run (it must not open a PDF app, nor a print dialog), told by the headless switch or by the program's name
/// (the app is `ibisOffice[.exe]`; a test binary is named after its crate or file). Not by the arguments: a file named `latest.docx` is
/// an argument too (issue #19: the PDF app did not start for it).
#[cfg(not(target_family = "wasm"))]
pub(crate) fn running_under_test() -> bool {
    if std::env::var_os("IBIS_HEADLESS").is_some() {
        return true;
    }
    let exe = std::env::current_exe().ok();
    let name = exe.as_deref().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("");
    is_test_binary_name(name)
}

#[cfg(not(target_family = "wasm"))]
fn is_test_binary_name(name: &str) -> bool {
    !name.starts_with("ibisOffice")
}

#[cfg(all(test, not(target_family = "wasm")))]
mod running_under_test_tests {
    use super::is_test_binary_name;

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

#[cfg(not(target_family = "wasm"))]
pub(crate) fn open_pdf_in_default_app(pdf: &std::path::Path, cx: &mut App) -> Result<(), String> {
    if running_under_test() {
        let _ = (pdf, cx);
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        cx.open_url(&pdf.to_string_lossy());
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        let _ = cx;
        let status = std::process::Command::new("open").arg(pdf).status().map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(crate::shell::i18n::format("file.print-exit", &[("status", &status)]))
        }
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let _ = cx;
        std::process::Command::new("xdg-open").arg(pdf).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

#[cfg(not(target_family = "wasm"))]
pub(crate) fn export_program() -> Result<std::path::PathBuf, String> {
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    let file_name = current.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if file_name.starts_with("ibisOffice") && !file_name.contains("test") {
        return Ok(current);
    }
    if let Some(dir) = current.parent() {
        for name in ["ibisOffice", "ibisOffice.exe"] {
            let c1 = dir.join(name);
            if c1.exists() {
                return Ok(c1);
            }
        }
        if let Some(parent) = dir.parent() {
            for name in ["ibisOffice", "ibisOffice.exe"] {
                let c2 = parent.join(name);
                if c2.exists() {
                    return Ok(c2);
                }
            }
        }
    }
    Ok(current)
}


#[cfg(not(target_family = "wasm"))]
fn i18n_status(on: bool) -> String {
    crate::shell::i18n::text(if on { "_shell.saved-with-password" } else { "_shell.saved-without-password" }).to_string()
}
