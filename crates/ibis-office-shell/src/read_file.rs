// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Reading a file for editing (on a thread of its own), with its password if it has one.

use crate::timing;

/// A file read for editing: its model, and what saving it needs.
#[cfg(not(target_family = "wasm"))]
pub struct Loaded {
    pub presentation: ibis_doc_model::Presentation,
    pub source: std::sync::Arc<ibis_pptx::Source>,
    /// The password a protected file was opened with: saving protects it again.
    pub password: Option<String>,
    pub(crate) csv_stream: Option<std::sync::mpsc::Receiver<ibis_pptx::CsvProgressiveChunk>>,
}

/// What `read_file` says when the file is protected and needs its password.
pub const NEEDS_PASSWORD: &str = "\u{1}needs-password";
/// What it says when the password given is not the file's.
pub const WRONG_PASSWORD: &str = "\u{1}wrong-password";

/// The file at `path`, read and parsed. Its pictures and fonts start
/// decoding on other threads (`preload`).
#[cfg(not(target_family = "wasm"))]
pub fn read_file(path: &std::path::Path) -> Result<Loaded, String> {
    read_file_with(path, None)
}

/// As `read_file`; `password` opens a protected file (パスワードを使用して暗号化).
#[cfg(not(target_family = "wasm"))]
pub fn read_file_with(path: &std::path::Path, password: Option<&str>) -> Result<Loaded, String> {
    catch_read_panic(path, || read_file_unguarded(path, password))
}

/// Run `read` for the file at `path`. One file's reader going wrong must close that file only: every window shares this
/// process, so a panic here would take the other open documents with it (review 0.8.5, C1). The panic is caught and becomes
/// this file's open error. (No `panic = "abort"` is set anywhere in the workspace, so the unwind reaches this frame.)
#[cfg(not(target_family = "wasm"))]
pub fn catch_read_panic<T>(path: &std::path::Path, read: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(read)).unwrap_or_else(|payload| {
        let what = payload.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| payload.downcast_ref::<String>().cloned()).unwrap_or_default();
        log::error!("{}: the reader panicked: {what}", path.display());
        note_open_failed(path);
        Err(crate::i18n::text("file.read-crashed").to_string())
    })
}

#[cfg(not(target_family = "wasm"))]
fn read_file_unguarded(path: &std::path::Path, password: Option<&str>) -> Result<Loaded, String> {
    timing::mark_read_start();
    let read_t0 = crate::clock::Instant::now();
    let mut data = std::fs::read(path).map_err(|e| e.to_string())?;
    let read_fs_ms = read_t0.elapsed().as_secs_f64() * 1000.0;
    timing::mark("read");
    let mut used = None;
    if ibis_pptx::is_encrypted(&data) {
        let Some(password) = password else { return Err(NEEDS_PASSWORD.to_string()) };
        data = ibis_pptx::decrypt(&data, password).map_err(|e| if e == ibis_pptx::DecryptError::WrongPassword { WRONG_PASSWORD.to_string() } else { e.to_string() })?;
        used = Some(password.to_string());
    }
    let open_t0 = crate::clock::Instant::now();
    // An old binary file (.xls, .ppt, .doc): read by its content, not by its extension.
    let is_legacy_binary = ibis_pptx::is_legacy_binary(&data);
    let external_document_format=ibis_pptx::docx_convert::DocxConvertFormat::from_path(path);
    let open_result=if let Some(format)=external_document_format {
        ibis_pptx::docx_convert::import_docx_format(&data,format).map(|opened|(opened,None))
    } else {
        // The volatile cells calculated again on open (CELL("filename") among them) need the file's path.
        ibis_calc::eval::convert::with_opening_file(Some(path.to_string_lossy().into_owned()), || ibis_pptx::open_progressive(data,100_000))
    };
    let (opened, csv_stream) = open_result.map_err(|e| {
        note_open_failed(path);
        e.to_string()
    })?;
    for (_, driver) in crate::format::drivers() {
        driver.note_opened(path);
    }
    let mut opened = opened;
    opened.presentation.opened_from_legacy = is_legacy_binary;
    let open_ms = open_t0.elapsed().as_secs_f64() * 1000.0;
    timing::mark("parsed");
    let total_file_ms = read_t0.elapsed().as_secs_f64() * 1000.0;
    eprintln!("ibisOffice timing: read_file total: {total_file_ms:.1} ms (fs::read: {read_fs_ms:.1} ms, open: {open_ms:.1} ms)");
    crate::render::preload::Preload::start_for_next(&opened.presentation);
    Ok(Loaded { presentation: opened.presentation, source: std::sync::Arc::new(opened.source), password: used, csv_stream })
}

/// Tell every format that the file at `path` could not be parsed (in the order of their registration, as they were told
/// before the shell was a crate of its own: the workbook's, then the document's).
fn note_open_failed(path: &std::path::Path) {
    for (_, driver) in crate::format::drivers() {
        driver.note_open_failed(path);
    }
}
