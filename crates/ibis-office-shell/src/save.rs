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

use std::borrow::Cow;

use gpui_kit::*;
use ibis_doc_model::Presentation;

use crate::editor::SlideEditor;
use crate::actions::{Save, SaveAs};

/// What a refused save was asked as (`SlideEditor::refuse_as`): the words of the status differ.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RefusedWay {
    Save,
    Export,
    Copy,
    Print,
}

impl SlideEditor {
    /// What is typed and not yet entered (a workbook's cell being edited, `FormatEditor::enter_pending_edit`) is entered, then the
    /// shell's own edit (a shape's text) is finished: what a save, a copy or an export that follows writes or shows is the document
    /// as it is now. Every way out of the window that writes the document starts here (⌘S used only `commit_edit`: the cell stayed
    /// open and was not in the saved file).
    pub(crate) fn commit_pending_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::format::with_format!(self, enter_pending_edit(self, cx));
        self.commit_edit(window, cx);
    }

    /// The model the file is written from: the open one, or the screen's copy of it with what it keeps outside the
    /// model put in (`FormatEditor::model_for_save`).
    pub(crate) fn presentation_for_save(&self) -> Cow<'_, Presentation> {
        crate::format::with_format!(self, model_for_save(self))
    }

    /// The format refuses a save for now (a presentation's master view is closed first): told in the status bar, before any
    /// dialog asks where to save (the master view's page is a file of its own with no path, so ⌘S would ask a name first and
    /// refuse only after it). True when refused.
    pub fn refuse_save(&mut self, cx: &mut Context<Self>) -> bool {
        self.refuse_as(RefusedWay::Save, cx)
    }

    /// `refuse_save`, told in the words of what was asked for (saving, exporting, saving a copy, printing).
    pub fn refuse_as(&mut self, way: RefusedWay, cx: &mut Context<Self>) -> bool {
        match crate::format::with_format!(self, save_refused(self)) {
            Some(why) => {
                let key = match way {
                    RefusedWay::Save => "file.save-failed",
                    RefusedWay::Export => "file.export-failed",
                    RefusedWay::Copy => "file.copy-failed",
                    RefusedWay::Print => "file.print-failed",
                };
                self.status = Some(crate::i18n::format(key, &[("error", &why)]).into());
                cx.notify();
                true
            }
            None => false,
        }
    }

    /// ⌘S: save to the file it was opened from (or ask where, for a file
    /// that has no path yet).
    pub fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_pending_edit(window, cx);
        if self.refuse_save(cx) {
            return;
        }
        #[cfg(not(target_family = "wasm"))]
        // A file opened read-only for its password to modify is not written over: 名前を付けて保存 (as Office does).
        match self.path.clone().filter(|_| !self.is_modify_locked()) {
            Some(path) => {
                let result = self.save_to(path, window);
                self.report_save(result, cx);
            }
            None => self.save_as(&SaveAs, window, cx),
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = window;
            self.status = Some(crate::i18n::text("_shell.saving-is-not-yet-available-in-the-browser-versi").into());
            cx.notify();
        }
    }

    /// ⌘⇧S: ask where, and save there.
    pub fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        self.save_as_then(window, cx, |_, _, _| {});
    }

    /// ⌘⇧S, and then, once the file is written, `then`. A cancelled or failed save does not go on to `then`.
    pub(crate) fn save_as_then(&mut self, window: &mut Window, cx: &mut Context<Self>, then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static) {
        self.commit_pending_edit(window, cx);
        if self.refuse_save(cx) {
            return;
        }
        if self.source.is_none() {
            self.status = Some(crate::i18n::text("_shell.saving-new-documents-is-not-yet-supported").into());
            return cx.notify();
        }
        #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
        if crate::scenario::button_probe::save_as_caught() {
            return;
        }
        #[cfg(not(target_family = "wasm"))]
        {
            let directory = self.save_dialog_folder();
            let name = self.file_name.clone().unwrap_or_default();
            let picked = cx.prompt_for_new_path_with_formats(&directory, Some(&name), &self.save_formats());
            cx.spawn_in(window, async move |this, cx| {
                // The dialog failing to open is told in a message box (not taken for a cancel).
                let path = match picked.await {
                    Ok(Ok(Some(path))) => path,
                    Ok(Err(error)) => {
                        this.update(cx, |this, cx| this.show_alert(crate::i18n::text("shell.save-failed.title"), error.to_string().into(), cx)).ok();
                        return;
                    }
                    _ => return,
                };
                this.update_in(cx, |this, window, cx| {
                    let remembered = path.clone();
                    let result = this.save_to(path, window);
                    let saved = result.is_ok();
                    if saved {
                        crate::save_location::remember_save_folder(&remembered);
                    }
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
            self.status = Some(crate::i18n::text("_shell.saving-is-not-yet-available-in-the-browser-versi").into());
            cx.notify();
        }
    }

    /// Save (as ⌘S, asking where for a file that has no path yet) and then, once the file is written, `then` (the
    /// save confirmation's 保存: close afterwards). A cancelled or failed save does not go on to `then`. What is being
    /// typed in a cell is put in first, so it is saved too.
    pub fn save_then(&mut self, window: &mut Window, cx: &mut Context<Self>, then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static) {
        self.commit_pending_edit(window, cx);
        if self.refuse_save(cx) {
            return;
        }
        #[cfg(not(target_family = "wasm"))]
        {
            if let Some(path) = self.path.clone().filter(|_| !self.is_modify_locked()) {
                let result = self.save_to(path, window);
                let saved = result.is_ok();
                self.report_save(result, cx);
                if saved {
                    then(self, window, cx);
                }
                return;
            }
            if self.source.is_none() {
                self.status = Some(crate::i18n::text("_shell.saving-new-documents-is-not-yet-supported").into());
                return cx.notify();
            }
            let directory = self.new_file_save_folder();
            let name = self.file_name.clone().unwrap_or_default();
            let picked = cx.prompt_for_new_path_with_formats(&directory, Some(&name), &self.save_formats());
            cx.spawn_in(window, async move |this, cx| {
                // The dialog failing to open is told in a message box (not taken for a cancel).
                let path = match picked.await {
                    Ok(Ok(Some(path))) => path,
                    Ok(Err(error)) => {
                        this.update(cx, |this, cx| this.show_alert(crate::i18n::text("shell.save-failed.title"), error.to_string().into(), cx)).ok();
                        return;
                    }
                    _ => return,
                };
                this.update_in(cx, |this, window, cx| {
                    let remembered = path.clone();
                    let result = this.save_to(path, window);
                    let saved = result.is_ok();
                    if saved {
                        crate::save_location::remember_save_folder(&remembered);
                    }
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
            self.status = Some(crate::i18n::text("_shell.saving-is-not-yet-available-in-the-browser-versi").into());
            cx.notify();
        }
    }

    /// The folder 名前を付けて保存 opens in: the file's own folder, or (a new file) `new_file_save_folder`.
    #[cfg(not(target_family = "wasm"))]
    pub fn save_dialog_folder(&self) -> std::path::PathBuf {
        self.path.as_deref().and_then(|p| p.parent()).map(std::path::Path::to_path_buf).unwrap_or_else(|| self.new_file_save_folder())
    }

    /// The folder the save dialog opens in for a file with no folder: the one a workbook's setting names (ツール > 設定,
    /// the user's own choice), else the folder saved to last, else the OS's documents folder (`save_location.rs`).
    #[cfg(not(target_family = "wasm"))]
    pub fn new_file_save_folder(&self) -> std::path::PathBuf {
        if let Some(custom) = crate::format::with_format!(self, new_file_save_folder(self)) {
            return custom;
        }
        crate::save_location::default_save_folder()
    }

    /// The formats the save dialog offers for the kind of the open file (the pop-up of the dialog; a name typed with another
    /// extension is kept). Each is one `save_to` writes by its extension.
    #[cfg(not(target_family = "wasm"))]
    fn save_formats(&self) -> Vec<SaveFormat> {
        use crate::i18n::text;
        self.format_info().save_formats.iter().map(|(label, extension)| SaveFormat { label: text(label), extensions: vec![extension.to_string()] }).collect()
    }

    /// Save to `path`, and make it the file's path.
    #[cfg(not(target_family = "wasm"))]
    pub fn save_to(&mut self, path: std::path::PathBuf, window: &mut Window) -> Result<(), String> {
        // The format may refuse for now (a presentation's master view is closed first: what is saved is the file, with what was made in it).
        if let Some(why) = crate::format::with_format!(self, save_refused(self)) {
            return Err(why);
        }
        // The password to modify not given: the file itself is not written over (another name is).
        if self.is_modify_locked() && self.path.as_deref().is_some_and(|own| same_file(own, &path)) {
            return Err(crate::i18n::text("file.modify-locked-no-overwrite").to_string());
        }
        let source = self.source.clone().ok_or(crate::i18n::text("_shell.saving-new-documents-is-not-yet-supported").to_string())?;
        // The format's own package, or what the format writes under this extension (a workbook saved as .csv is the sheet shown, as
        // Excel's 「CSV UTF-8」).
        let (bytes, may_protect) = match crate::format::with_format!(self, bytes_for_path(self, &path, &source, false)) {
            Some(other) => {
                let other = other?;
                if let Some(note) = other.note {
                    self.save_note = note;
                }
                (other.bytes, other.protect)
            }
            None => (source.save(&self.presentation_for_save(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string())?, true),
        };
        // 常にバックアップ コピーを作成する: what the file is now is kept, where the format has the option.
        crate::format::with_format!(self, backup_before_write(self, &path));
        // Protected with the password the file has (パスワードを使用して暗号化).
        let bytes = if let (Some(password), true) = (&self.protection_password, may_protect) { protect(&bytes, password)? } else { bytes };
        write_replacing(&path, &bytes).map_err(|e| crate::i18n::format("file.cannot-write", &[("path", &path.display()), ("error", &e)]))?;
        let name: SharedString = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().into();
        window.set_window_title(&format!("{name} - ibisOffice"));
        self.file_name = Some(name);
        // A newly created file becomes recent as soon as its first save succeeds.
        self.recent_files = crate::recent_files::add(&path);
        self.set_file_path(Some(path.clone()));
        // Written in a current format: no longer Compatibility Mode.
        if !path.extension().and_then(|e| e.to_str()).is_some_and(|x| ["xls", "ppt", "pps", "pot", "doc", "dot"].iter().any(|o| x.eq_ignore_ascii_case(o))) {
            self.opened_from_legacy = false;
        }
        // The file as written here: the watching for other apps' changes (shell/file_watch.rs) does not take it for one.
        self.watch.known = crate::file_watch::DiskStamp::after_write(&path, &bytes);
        self.watch.asked = None;
        self.document.mark_clean();
        self.cleanup_active_autorecover_file();
        self.is_recovered = false;
        self.is_repaired = false;
        Ok(())
    }

    #[cfg(not(target_family = "wasm"))]
    fn report_save(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self.status = Some(match result {
            Ok(()) => self.save_note.take().unwrap_or_else(|| crate::i18n::text("_shell.saved").into()),
            Err(err) => crate::i18n::format("file.save-failed", &[("error", &err)]).into(),
        });
        cx.notify();
    }
}

/// `package` protected with `password`: an OOXML package kept encrypted in a compound file.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn protect(package: &[u8], password: &str) -> Result<Vec<u8>, String> {
    // The OS's random generator (getrandom: BCryptGenRandom on Windows, which has no /dev/urandom; #M146).
    let mut failed = false;
    let encrypted = ibis_pptx::encrypt(package, password, &mut |bytes: &mut [u8]| {
        if getrandom::fill(bytes).is_err() {
            failed = true;
        }
    });
    if failed {
        return Err(crate::i18n::text("_shell.passwords-cannot-be-set-in-this-environment").to_string());
    }
    Ok(encrypted)
}

/// Write `bytes` to a new file next to `path`, then move it over `path`.
#[cfg(not(target_family = "wasm"))]
pub fn write_replacing(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temporary = path.with_file_name(format!(".{name}.ibis-saving"));
    let moved = std::fs::write(&temporary, bytes).and_then(|()| std::fs::rename(&temporary, path));
    let Err(error) = moved else { return Ok(()) };
    let _ = std::fs::remove_file(&temporary);
    // Only "not found" is met by writing in place: a folder a sync client keeps (OneDrive) can take the new file away
    // before it is moved (os error 2, GitHub issue #144; the cause is a guess, not seen on OneDrive itself). Any other
    // error (a full disk, no permission, a lock) is returned: writing in place would start the file over.
    if !falls_back_to_writing_in_place(&error) {
        return Err(error);
    }
    write_in_place(path, bytes)
}

/// Whether a failed save by a new file beside the target is tried again by writing the target itself.
#[cfg(not(target_family = "wasm"))]
fn falls_back_to_writing_in_place(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound
}

/// Write `bytes` over `path` itself, after a copy of the file as it is (put back when the writing fails midway). When the
/// copy cannot be made nothing is written.
#[cfg(not(target_family = "wasm"))]
fn write_in_place(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let copy = path.with_file_name(format!(".{name}.ibis-previous"));
    let existed = path.exists();
    if existed {
        std::fs::copy(path, &copy)?;
    }
    match std::fs::write(path, bytes) {
        Ok(()) => {
            let _ = std::fs::remove_file(&copy);
            Ok(())
        }
        Err(error) => {
            // The copy stays when it cannot be put back.
            if existed && std::fs::copy(&copy, path).is_ok() {
                let _ = std::fs::remove_file(&copy);
            }
            Err(error)
        }
    }
}

/// Whether `a` and `b` are one file (the same path, or one that links to the other).
#[cfg(not(target_family = "wasm"))]
fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    a == b || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

#[cfg(all(test, not(target_family = "wasm")))]
mod write_tests {
    use super::write_replacing;

    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/save-tests")).join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_file_is_replaced_in_a_folder_of_a_japanese_name() {
        let dir = folder("\u{66f8}\u{304d}\u{8fbc}\u{307f}/\u{30c9}\u{30ad}\u{30e5}\u{30e1}\u{30f3}\u{30c8}");
        let path = dir.join("\u{30d5}\u{30a1}\u{30a4}\u{30eb}.xlsx");
        write_replacing(&path, b"old").unwrap();
        write_replacing(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert!(!dir.join(".\u{30d5}\u{30a1}\u{30a4}\u{30eb}.xlsx.ibis-saving").exists());
    }

    #[test]
    fn only_not_found_is_written_in_place() {
        use std::io::{Error, ErrorKind};
        assert!(super::falls_back_to_writing_in_place(&Error::from(ErrorKind::NotFound)));
        for kind in [ErrorKind::PermissionDenied, ErrorKind::StorageFull, ErrorKind::ReadOnlyFilesystem, ErrorKind::AlreadyExists, ErrorKind::Other] {
            assert!(!super::falls_back_to_writing_in_place(&Error::from(kind)), "{kind:?}");
        }
    }

    #[test]
    fn a_file_is_written_in_place_and_its_copy_removed() {
        let dir = folder("write-in-place");
        let path = dir.join("book.xlsx");
        std::fs::write(&path, b"old").unwrap();
        super::write_in_place(&path, b"new").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert!(!dir.join(".book.xlsx.ibis-previous").exists());
        // A file that is not there yet is written too.
        let fresh = dir.join("fresh.xlsx");
        super::write_in_place(&fresh, b"new").unwrap();
        assert_eq!(std::fs::read(&fresh).unwrap(), b"new");
    }

    #[test]
    fn nothing_is_written_in_place_when_the_copy_cannot_be_made() {
        // The place of the copy is taken (by a folder): the file is left as it was and the error is returned.
        let dir = folder("write-in-place-no-copy");
        let path = dir.join("book.xlsx");
        std::fs::write(&path, b"old").unwrap();
        std::fs::create_dir(dir.join(".book.xlsx.ibis-previous")).unwrap();
        assert!(super::write_in_place(&path, b"new").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
    }

    #[test]
    fn another_error_leaves_the_file_as_it_was() {
        // The new file's place is taken (by a folder): the error is not "not found", so it is returned, the file stays.
        let dir = folder("write-other-error");
        let path = dir.join("book.xlsx");
        std::fs::write(&path, b"old").unwrap();
        std::fs::create_dir(dir.join(".book.xlsx.ibis-saving")).unwrap();
        assert!(write_replacing(&path, b"new").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"old");
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod protect_tests {
    /// A password-protected save gets its random bytes from the OS on every OS (Windows has no /dev/urandom: #M146), and
    /// the result opens again with the password.
    #[test]
    fn a_protected_package_opens_with_its_password() {
        let package = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/docx-report.docx")).expect("the sample");
        let locked = super::protect(&package, "ibis123").expect("random bytes from the OS");
        assert_ne!(locked, package);
        let opened = ibis_pptx::decrypt(&locked, "ibis123").expect("opens with the password");
        assert_eq!(opened, package);
        assert!(ibis_pptx::decrypt(&locked, "wrong").is_err());
    }
}
