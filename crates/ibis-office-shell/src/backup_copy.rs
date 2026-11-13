// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! バックアップ コピーの保存 (DOCX-FILE-016, Word's 常にバックアップ ファイルを作成する): when it is on, saving over a
//! file first copies what the file was to `<名前> のバックアップ.wbk` in the same folder.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use gpui_kit::*;

use crate::editor::SlideEditor;

static ON: Mutex<Option<bool>> = Mutex::new(None);

fn store_path() -> Option<PathBuf> {
    if cfg!(feature = "scenarios") {
        return None;
    }
    let base = crate::app_data::base();
    Some(base?.join("ibisOffice").join("docx-backup.tsv"))
}

/// Whether a backup copy is made when a document is saved over a file.
/// Back to the default (the scenarios start each from it).
pub(crate) fn reset() {
    *ON.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

pub fn backup_enabled() -> bool {
    let mut guard = ON.lock().unwrap_or_else(|e| e.into_inner());
    *guard.get_or_insert_with(|| store_path().and_then(|p| std::fs::read_to_string(p).ok()).is_some_and(|c| c.lines().any(|l| l.trim() == "always_create_backup\t1")))
}

pub(crate) fn set_backup_enabled(on: bool) {
    if let Some(path) = store_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, format!("always_create_backup\t{}\n", u8::from(on)));
    }
    *ON.lock().unwrap_or_else(|e| e.into_inner()) = Some(on);
}

/// The name of the backup copy of `path`.
pub(crate) fn backup_path(path: &Path) -> PathBuf {
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Document");
    path.with_file_name(crate::i18n::format("_docx.backup-of-stem", &[("stem", &stem)]))
}

/// Before saving over `path`: what the file is now goes to its backup copy, when the option is on. The copy's path.
#[cfg(not(target_family = "wasm"))]
pub fn make_backup(path: &Path) -> Result<Option<PathBuf>, String> {
    if !backup_enabled() || !path.exists() {
        return Ok(None);
    }
    let to = backup_path(path);
    std::fs::copy(path, &to).map_err(|e| e.to_string())?;
    Ok(Some(to))
}

impl SlideEditor {
    /// ファイル > 常にバックアップ コピーを作成する: on or off.
    pub fn toggle_doc_backup(&mut self, cx: &mut Context<Self>) {
        set_backup_enabled(!backup_enabled());
        cx.notify();
    }
}
