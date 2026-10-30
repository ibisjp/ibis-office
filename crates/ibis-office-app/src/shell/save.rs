// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ファイル > 保存 (⌘S) and 名前を付けて保存… (⌘⇧S).
//!
//! The file is written by `ibis_pptx::Source::save`: the original file with
//! the edits made since it was opened written into it (see
//! `ibis-pptx/src/save/mod.rs`). An edit the format cannot save yet is
//! reported in the status bar and nothing is written, so no edit is lost
//! silently. The file is written next to its destination and then moved
//! over it, so a failed save never leaves a half-written file.

use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::{Save, SaveAs};

impl SlideEditor {
    /// ⌘S: save to the file it was opened from (or ask where, for a file
    /// that has no path yet).
    pub(crate) fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        match self.path.clone() {
            Some(path) => {
                let result = self.save_to(path, window);
                self.report_save(result, cx);
            }
            None => self.save_as(&SaveAs, window, cx),
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.status = Some(crate::shell::i18n::text("_shell.saving-is-not-yet-available-in-the-browser-versi").into());
            cx.notify();
        }
    }

    /// ⌘⇧S: ask where, and save there.
    pub(crate) fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_edit(window, cx);
        if self.source.is_none() {
            self.status = Some(crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").into());
            return cx.notify();
        }
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self
                .path
                .as_deref()
                .and_then(|p| p.parent())
                .map(std::path::Path::to_path_buf)
                .unwrap_or_else(|| {
                    if self.is_workbook() {
                        crate::xlsx::backup_save::effective_default_save_location()
                    } else {
                        std::env::var_os("HOME").map(Into::into).unwrap_or_else(|| ".".into())
                    }
                });
            let name = self.file_name.clone().unwrap_or_default();
            let picked = cx.prompt_for_new_path_with_formats(&directory, Some(&name), &self.save_formats());
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update_in(cx, |this, window, cx| {
                    let result = this.save_to(path, window);
                    this.report_save(result, cx);
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.status = Some(crate::shell::i18n::text("_shell.saving-is-not-yet-available-in-the-browser-versi").into());
            cx.notify();
        }
    }

    /// Save (as ⌘S, asking where for a file that has no path yet) and then, once the file is written, `then` (the
    /// save confirmation's 保存: close afterwards). A cancelled or failed save does not go on to `then`. What is being
    /// typed in a cell is put in first, so it is saved too.
    pub(crate) fn save_then(&mut self, window: &mut Window, cx: &mut Context<Self>, then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static) {
        self.commit_cell_edit(cx);
        self.commit_edit(window, cx);
        #[cfg(not(target_family = "wasm"))]
        {
            if let Some(path) = self.path.clone() {
                let result = self.save_to(path, window);
                let saved = result.is_ok();
                self.report_save(result, cx);
                if saved {
                    then(self, window, cx);
                }
                return;
            }
            if self.source.is_none() {
                self.status = Some(crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").into());
                return cx.notify();
            }
            let directory = if self.is_workbook() {
                crate::xlsx::backup_save::effective_default_save_location()
            } else {
                std::env::var_os("HOME").map(Into::into).unwrap_or_else(|| ".".into())
            };
            let name = self.file_name.clone().unwrap_or_default();
            let picked = cx.prompt_for_new_path_with_formats(&directory, Some(&name), &self.save_formats());
            cx.spawn_in(window, async move |this, cx| {
                let Ok(Ok(Some(path))) = picked.await else { return };
                this.update_in(cx, |this, window, cx| {
                    let result = this.save_to(path, window);
                    let saved = result.is_ok();
                    this.report_save(result, cx);
                    if saved {
                        then(this, window, cx);
                    }
                })
                .ok();
            })
            .detach();
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = (window, then);
            self.status = Some(crate::shell::i18n::text("_shell.saving-is-not-yet-available-in-the-browser-versi").into());
            cx.notify();
        }
    }

    /// The formats the save dialog offers for the kind of the open file (the pop-up of the dialog; a name typed with another
    /// extension is kept). Each is one `save_to` writes by its extension.
    #[cfg(not(target_family = "wasm"))]
    fn save_formats(&self) -> Vec<SaveFormat> {
        use crate::shell::i18n::text;
        let format = |label: &str, extensions: &[&str]| SaveFormat { label: text(label), extensions: extensions.iter().map(|e| e.to_string()).collect() };
        if self.is_workbook() {
            vec![
                format("file.save-format.xlsx", &["xlsx"]),
                format("file.save-format.xltx", &["xltx"]),
                format("file.save-format.xls", &["xls"]),
                format("file.save-format.ods", &["ods"]),
                format("file.save-format.csv", &["csv"]),
                format("file.save-format.tsv", &["tsv"]),
            ]
        } else if self.is_presentation() {
            vec![format("file.save-format.pptx", &["pptx"]), format("file.save-format.ppsx", &["ppsx"]), format("file.save-format.potx", &["potx"]), format("file.save-format.odp", &["odp"])]
        } else {
            vec![format("file.save-format.docx", &["docx"]), format("file.save-format.dotx", &["dotx"]), format("file.save-format.odt", &["odt"])]
        }
    }

    /// Save to `path`, and make it the file's path.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn save_to(&mut self, path: std::path::PathBuf, window: &mut Window) -> Result<(), String> {
        // The master view is closed first: what is saved is the file, with what was made in it.
        if self.is_master_view() || self.is_page_view() {
            return Err(crate::shell::i18n::text("_shell.close-slide-master-view-and-notes-formatting-bef").to_string());
        }
        let source = self.source.clone().ok_or(crate::shell::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string())?;
        // A workbook saved as .csv: the sheet shown, as CSV (Excel's 「CSV UTF-8」).
        let csv = self.is_workbook() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("csv"));
        let tsv = self.is_workbook() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("tsv") || e.eq_ignore_ascii_case("tab") || e.eq_ignore_ascii_case("txt"));
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
        } else if tsv {
            let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
            ibis_pptx::csv::sheet_to_tsv(self.presentation(), index)
        } else if ods {
            ibis_pptx::write_ods_with_source(self.presentation(), &source).map_err(|e| e.to_string())?
        } else if odt {
            ibis_pptx::docx_convert::export_docx_to_odt(self.presentation()).map_err(|e| e.to_string())?
        } else if odp {
            ibis_pptx::write_odp(self.presentation()).map_err(|e| e.to_string())?
        } else if xls {
            let (bytes, report) = ibis_pptx::write_xls_report(self.presentation()).map_err(|e| e.to_string())?;
            self.save_note = crate::xlsx::export_xls::note(&report);
            bytes
        } else if xltx || potx || dotx {
            source.save_template(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?
        } else if ppsx {
            source.save_slideshow(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?
        } else if pptx {
            source.save_presentation(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?
        } else {
            source.save(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?
        };
        if self.is_workbook() {
            let override_opt = self.sheet_state().and_then(|s| s.always_create_backup);
            if let Ok(Some(backup_path)) = crate::xlsx::backup_save::create_backup_copy_if_enabled(&path, override_opt) {
                log::info!("Created backup copy at: {}", backup_path.display());
            }
        }
        // 常にバックアップ コピーを作成する (a document's).
        if self.is_document() {
            if let Ok(Some(copy)) = crate::docx::backup::make_backup(&path) {
                log::info!("Created backup copy at: {}", copy.display());
            }
        }
        // Protected with the password the file has (パスワードを使用して暗号化).
        let bytes = if let (Some(password), false, false, false, false) = (&self.protection_password, csv, tsv, ods, xls) { protect(&bytes, password)? } else { bytes };
        write_replacing(&path, &bytes).map_err(|e| crate::shell::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)]))?;
        let name: SharedString = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().into();
        window.set_window_title(&format!("{name} - ibisOffice"));
        self.file_name = Some(name);
        // A newly created file becomes recent as soon as its first save succeeds.
        self.recent_files = crate::shell::recent_files::add(&path);
        self.set_file_path(Some(path));
        self.document.mark_clean();
        if self.is_workbook() || self.is_presentation() || self.is_document() {
            self.cleanup_active_autorecover_file();
            self.is_recovered = false;
            self.is_repaired = false;
        }
        Ok(())
    }

    #[cfg(not(target_family = "wasm"))]
    fn report_save(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.status = Some(match result {
            Ok(()) => self.save_note.take().unwrap_or_else(|| crate::shell::i18n::text("_shell.saved").into()),
            Err(err) => crate::shell::i18n::format("file.save-failed", &[("error", &err)]).into(),
        });
        cx.notify();
    }
}

/// `package` protected with `password`: an OOXML package kept encrypted in a compound file.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn protect(package: &[u8], password: &str) -> Result<Vec<u8>, String> {
    let mut urandom = std::fs::File::open("/dev/urandom").map_err(|_| crate::shell::i18n::text("_shell.passwords-cannot-be-set-in-this-environment").to_string())?;
    let mut failed = false;
    let encrypted = ibis_pptx::encrypt(package, password, &mut |bytes: &mut [u8]| {
        use std::io::Read;
        if urandom.read_exact(bytes).is_err() {
            failed = true;
        }
    });
    if failed {
        return Err(crate::shell::i18n::text("_shell.passwords-cannot-be-set-in-this-environment").to_string());
    }
    Ok(encrypted)
}

/// Write `bytes` to a new file next to `path`, then move it over `path`.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn write_replacing(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temporary = path.with_file_name(format!(".{name}.ibis-saving"));
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}
