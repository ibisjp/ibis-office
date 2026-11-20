// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! OpenDocument files for ibisOffice: text (`.odt`, `.ott`), spreadsheets (`.ods`, `.ots`) and presentations (`.odp`, `.otp`).
//!
//! A file is read as the OOXML package made from it (`ods` -> `.xlsx`, `odp` -> `.pptx`, `odt` -> `.docx`), which `ibis-formats`
//! then reads; a workbook, a presentation or a text document is written as OpenDocument. This crate sits above `ibis-formats`:
//! `ibis_formats::read` and `ibis_formats::open` do not read OpenDocument, [`read`] and [`open`] here do (and read every other
//! format as `ibis-formats` does).

mod enhanced_geometry;
mod ods;
mod ods_write;
pub mod odp;
mod odp_anim;
mod odp_chart_style;
mod odp_shape;
mod odp_style;
mod odp_text;
mod odp_write;
pub mod odt;
mod odt_import;

use std::sync::Arc;

use ibis_doc_model::Presentation;
use ibis_formats::{CsvProgressiveChunk, Error, Opened, Source};

pub use odp::to_pptx as odp_to_pptx;
pub use odt::{export_docx_to_odt, export_odt, import_odt, import_odt_to_docx_bytes, is_odt};

/// Whether `data` is an OpenDocument spreadsheet, presentation or text document.
pub fn is_odf(data: &[u8]) -> bool {
    ods::is_ods(data) || odp::is_odp(data) || odt::is_odt(data)
}

/// The presentation as an OpenDocument presentation (`.odp`).
pub fn write_odp(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    odp_write::write(presentation)
}

/// The workbook as an OpenDocument spreadsheet (`.ods`).
pub fn write_ods(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    ods_write::write(presentation)
}

/// The workbook as an OpenDocument spreadsheet, its charts' series reading the cells they read in the file `source` was
/// opened from (so the charts follow the cells in the other program too).
pub fn write_ods_with_source(presentation: &Presentation, source: &Source) -> Result<Vec<u8>, Error> {
    ods_write::write_with(presentation, &|id, chart| source.sheet_chart_refs(id, chart))
}

/// `data` as the OOXML package made from it when it is an OpenDocument file, else `None`.
pub fn to_ooxml(data: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    if ods::is_ods(data) {
        ods::to_xlsx(data).map(Some)
    } else if odp::is_odp(data) {
        odp::to_pptx(data).map(Some)
    } else if odt::is_odt(data) {
        odt::import_odt_to_docx_bytes(data).map(Some)
    } else {
        Ok(None)
    }
}

/// `ibis_formats::read`, for OpenDocument files too.
pub fn read(data: &[u8]) -> Result<Presentation, Error> {
    match to_ooxml(data)? {
        Some(converted) => ibis_formats::read(&converted),
        None => ibis_formats::read(data),
    }
}

/// `ibis_formats::open`, for OpenDocument files too.
pub fn open(data: impl Into<Arc<[u8]>>) -> Result<Opened, Error> {
    let data: Arc<[u8]> = data.into();
    match to_ooxml(&data)? {
        Some(converted) => ibis_formats::open(converted),
        None => ibis_formats::open(data),
    }
}

/// `ibis_formats::open_progressive`, for OpenDocument files too.
pub fn open_progressive(
    data: impl Into<Arc<[u8]>>,
    first_chunk_rows: usize,
) -> Result<(Opened, Option<std::sync::mpsc::Receiver<CsvProgressiveChunk>>), Error> {
    open_progressive_with(data, first_chunk_rows, None)
}

/// `ibis_formats::open_progressive_with`, for OpenDocument files too.
pub fn open_progressive_with(
    data: impl Into<Arc<[u8]>>,
    first_chunk_rows: usize,
    csv_font: Option<&ibis_formats::csv::CsvFont>,
) -> Result<(Opened, Option<std::sync::mpsc::Receiver<CsvProgressiveChunk>>), Error> {
    let data: Arc<[u8]> = data.into();
    match to_ooxml(&data)? {
        Some(converted) => ibis_formats::open_progressive_with(converted, first_chunk_rows, csv_font),
        None => ibis_formats::open_progressive_with(data, first_chunk_rows, csv_font),
    }
}
