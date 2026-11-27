// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Reading a file for editing (on a thread of its own), with its password if it has one.

use crate::timing;

/// A file read for editing: its model, and what saving it needs.
#[cfg(not(target_family = "wasm"))]
pub struct Loaded {
    pub presentation: ibis_doc_model::Presentation,
    pub source: std::sync::Arc<ibis_formats::Source>,
    /// The password a protected file was opened with: saving protects it again.
    pub password: Option<String>,
    pub jtd_original: Option<Vec<u8>>,
    pub jtd_gallery: bool,
    pub(crate) csv_stream: Option<std::sync::mpsc::Receiver<ibis_formats::CsvProgressiveChunk>>,
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
    if path.extension().and_then(|e| e.to_str()).is_some_and(ibis_jtd::is_extension) || ibis_jtd::is_jtd(&data) {
        let (presentation, jtd_gallery) = crate::jtd_preview::read(&data)?;
        let blank = ibis_formats::blank_file(ibis_formats::Kind::Document).map_err(|e| e.to_string())?;
        let source = ibis_formats::open(blank).map_err(|e| e.to_string())?.source;
        crate::render::preload::Preload::start_for_next(&presentation);
        return Ok(Loaded { presentation, source: std::sync::Arc::new(source), password: None, csv_stream: None, jtd_original: Some(data), jtd_gallery });
    }
    if crate::fixed_layout::is_fixed_layout(path.extension().and_then(|e| e.to_str()), &data) {
        let opened = crate::fixed_layout::open_fixed_layout(data.into())?;
        return Ok(Loaded { presentation: opened.presentation, source: std::sync::Arc::new(opened.source), password: None, csv_stream: None, jtd_original: None, jtd_gallery: false });
    }
    let read_fs_ms = read_t0.elapsed().as_secs_f64() * 1000.0;
    timing::mark("read");
    let mut used = None;
    if ibis_formats::is_encrypted(&data) {
        let Some(password) = password else { return Err(NEEDS_PASSWORD.to_string()) };
        data = ibis_formats::decrypt(&data, password).map_err(|e| if e == ibis_formats::DecryptError::WrongPassword { WRONG_PASSWORD.to_string() } else { e.to_string() })?;
        used = Some(password.to_string());
    }
    let open_t0 = crate::clock::Instant::now();
    // An old binary file (.xls, .ppt, .doc): read by its content, not by its extension.
    let is_legacy_binary = ibis_formats::is_legacy_binary(&data);
    let external_document_format = ibis_formats::docx_convert::DocxConvertFormat::from_path(path);
    // A DPC E/F or RECEIPTC file is a plain text file by its name (EF....txt): taken by what is in it, a sheet, not the text of a document.
    let external_document_format = external_document_format.filter(|f| *f != ibis_formats::docx_convert::DocxConvertFormat::Txt || !(ibis_formats::dpc::is_dpc_ef(&data) || ibis_formats::uke::is_uke(&data)));
    let open_result = if let Some(law_md) = ibis_formats::lawtext::path_input_to_markdown(path, &data).map_err(|e| {
        note_open_failed(path);
        e.to_string()
    })? {
        ibis_formats::markdown::open_markdown_with_base_dir(law_md.into_bytes(), path.parent()).map(|opened| (opened, None))
    } else if let Some(dicom_md) = ibis_formats::dicom::input_to_markdown(&data).map_err(|e| {
        note_open_failed(path);
        e.to_string()
    })? {
        ibis_formats::markdown::open_markdown_with_base_dir(dicom_md.into_bytes(), path.parent()).map(|opened| (opened, None))
    } else if let Some(fhir_md) = ibis_formats::fhir::input_to_markdown(&data).map_err(|e| {
        note_open_failed(path);
        e.to_string()
    })? {
        ibis_formats::markdown::open_markdown_with_base_dir(fhir_md.into_bytes(), path.parent()).map(|opened| (opened, None))
    } else if external_document_format == Some(ibis_formats::docx_convert::DocxConvertFormat::Markdown) {
        ibis_formats::markdown::open_markdown_with_base_dir(data, path.parent()).map(|opened| (opened, None))
    } else if let Some(format) = external_document_format {
        (if format == ibis_formats::docx_convert::DocxConvertFormat::Odt { ibis_odf::import_odt(&data) } else { ibis_formats::docx_convert::import_docx_format(&data, format) }).map(|opened| (opened, None))
    } else {
        // The volatile cells calculated again on open (CELL("filename") among them) need the file's path.
        ibis_calc::eval::convert::with_opening_file(Some(path.to_string_lossy().into_owned()), || ibis_odf::open_progressive_with(data, 100_000, crate::settings_dialog::csv_font().as_ref()))
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
    Ok(Loaded { presentation: opened.presentation, source: std::sync::Arc::new(opened.source), password: used, csv_stream, jtd_original: None, jtd_gallery: false })
}

/// Tell every format that the file at `path` could not be parsed (in the order of their registration, as they were told
/// before the shell was a crate of its own: the workbook's, then the document's).
fn note_open_failed(path: &std::path::Path) {
    for (_, driver) in crate::format::drivers() {
        driver.note_open_failed(path);
    }
}

/// Let the queries of a workbook read the files their sources name (`Excel.Workbook(File.Contents(path))`): the model has no file reader of
/// its own, so it is given this one (the app's, which reads every format). Called once at start; the browser reads no files.
#[cfg(not(target_family = "wasm"))]
pub fn register_query_workbook_loader() {
    ibis_doc_model::query::set_workbook_loader(|path| read_file(std::path::Path::new(path)).map(|loaded| loaded.presentation));
}
