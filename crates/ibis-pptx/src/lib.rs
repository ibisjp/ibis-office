// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Reads PowerPoint (`.pptx`) files into an [`ibis_doc_model::Presentation`],
//! Excel (`.xlsx`) workbooks as one slide per worksheet, and Word (`.docx`)
//! documents as a flowing document to be laid out into pages.
//!
//! Slides, layouts and masters are flattened: placeholders take their
//! position and text format from the layout and master, and the layout's and
//! master's own shapes become each slide's backdrop. Charts are read from
//! the values cached in their parts; tables are kept as placeholders.

/// Whether the names ibisOffice gives what it makes in a document (a shape's
/// name when a document is saved) are English, as the app's UI language is
/// (set by the app; Japanese by default).
static ENGLISH_NAMES: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Give the names made in documents in English (`true`) or Japanese, as the
/// UI shows.
pub fn set_english_names(english: bool) {
    ENGLISH_NAMES.store(english, std::sync::atomic::Ordering::Relaxed);
}

/// The name Office gives a new picture (`Picture 3`), in the UI's language.
pub(crate) fn picture_name(n: impl std::fmt::Display) -> String {
    if english_names() { format!("Picture {n}") } else { format!("図 {n}") } // i18n-ok（文書に保存する名前）
}

/// The name Office gives a new chart (`Chart 3`), in the UI's language.
pub(crate) fn chart_name(n: impl std::fmt::Display) -> String {
    if english_names() { format!("Chart {n}") } else { format!("グラフ {n}") } // i18n-ok（文書に保存する名前）
}

pub(crate) fn english_names() -> bool {
    ENGLISH_NAMES.load(std::sync::atomic::Ordering::Relaxed)
}

mod animation;
mod comments;
pub mod art_borders;
mod blank;
mod cfb;
mod cell_style;
mod clip_poster;
pub use clip_poster::{clip_poster, screenshot_snapshot};
mod chart;
mod chart_ex;
pub mod csv;
pub mod progress;
mod docx;
pub mod docx_convert;
pub(crate) mod drawing_table_style;
pub mod geometry;
pub mod shape_clipboard;
pub mod math_xml;
mod new_file;
mod package;
mod reader;
pub mod save;
mod style;
mod table_style;
pub use table_style::styled_with_table;
pub use xlsx_external::{link_display, link_path, new_external_book, read_external_book, refreshed_sheets};
pub use xlsx_pivot::{pivot_data, pivot_source, rebuilt_pivot, refreshed_pivot};
#[doc(hidden)]
pub mod test_files;
pub mod svg_convert;
pub mod theme_effects;
mod theme;
mod ods;
mod xlsb;
mod xlsb_formula;
mod ods_write;
mod xls;
pub mod odp;
mod odp_anim;
mod odp_shape;
mod odp_write;
mod odp_style;
mod odp_text;
pub mod ppt;
mod ppt_timing;
mod ppt_write;
mod xlsx_timeline;
mod xls_chart;
mod xls_write;
mod xls_drawing;
mod xls_features;
pub mod master_page;
pub mod handout_master;
pub mod notes_page;
mod xls_formula;
pub mod font_digit;
pub mod locale_paper;
mod xlsx;
pub use xlsx::{EXCEL_METRICS, ExcelMetrics};
mod xlsx_external;
mod xlsx_pivot;
mod xlsx_slicer;
mod xml;
mod zip;
pub mod repair;
pub use repair::{RepairLogEntry, RepairLogLevel, RepairMode, RepairReport, RepairedWorkbook, RepairedDocument, repair_workbook, repair_document};
pub mod chart_template;
pub use chart_template::{apply_chart_template, chart_to_crtx, chart_to_xml, read_crtx};
pub use ibis_ooxml;

/// The presentation as an OpenDocument presentation (`.odp`).
pub fn write_odp(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    odp_write::write(presentation)
}

/// What was left out of an old Excel file (`.xls`) a workbook was written as.
pub use xls_write::Report as XlsReport;

/// The workbook as an old Excel file (`.xls`).
pub fn write_xls(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    xls_write::write(presentation)
}

/// The workbook as an old Excel file, and what had to be left out of it.
pub fn write_xls_report(presentation: &Presentation) -> Result<(Vec<u8>, XlsReport), Error> {
    xls_write::write_report(presentation)
}

/// The workbook as an OpenDocument spreadsheet (`.ods`).
pub fn write_ods(presentation: &Presentation) -> Result<Vec<u8>, Error> {
    ods_write::write(presentation)
}

/// The workbook as an OpenDocument spreadsheet, its charts' series reading the cells they read in the file `source` was
/// opened from (so the charts follow the cells in the other program too).
pub fn write_ods_with_source(presentation: &Presentation, source: &save::Source) -> Result<Vec<u8>, Error> {
    ods_write::write_with(presentation, &|id, chart| source.sheet_chart_refs(id, chart))
}
/// Password-protected files: whether `data` is one, its package opened with a password, a package protected.
pub use ibis_ooxml::encryption::{encrypt, DecryptError};

/// Whether `data` is protected with a password: a package kept encrypted, or an old workbook (`.xls`) or presentation (`.ppt`) with RC4.
pub fn is_encrypted(data: &[u8]) -> bool {
    ibis_ooxml::encryption::is_encrypted(data) || xls::is_encrypted(data) || ppt::is_encrypted(data)
}

/// `data` opened with `password`: the package, or (for an old workbook) the `.xlsx` made from it.
pub fn decrypt(data: &[u8], password: &str) -> Result<Vec<u8>, DecryptError> {
    if xls::is_encrypted(data) {
        xls::decrypt(data, password)
    } else if ppt::is_encrypted(data) {
        ppt::decrypt(data, password)
    } else {
        ibis_ooxml::encryption::decrypt(data, password)
    }
}

use ibis_doc_model::Presentation;

pub use geometry::preset as preset_geometry;
pub use geometry::text_warp as text_warp_geometry;
pub use new_file::new_presentation;
pub use save::{CsvProgressiveChunk, ForeignSlide, NewChartKind, Opened, SaveOptions, Source, open, open_progressive};
pub use reader::script_pieces;
pub use save::pptx_chart_data::{ChartData, SeriesRefs, bubble_refs_for_range, chart_from_data, sheet_chart_from_data, format_ref, grown_refs, parse_ref, quote_sheet, refs_for_range, preferred_series_columns, refs_range, scatter_refs_for_range, sheet_of, stock_refs_for_range};
pub use svg_convert::convert_svg_to_shapes;

/// Why a file could not be read. The message is for the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    pub(crate) fn xml(part: &str, err: roxmltree::Error) -> Self {
        Self(format!("{part} を読めません: {err}"))
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Read a `.pptx` presentation, a `.xlsx` workbook (see [`read_xlsx`]) or a
/// `.docx` document (see [`read_docx`]): which one is told by the package's
/// main part.
pub fn read(data: &[u8]) -> Result<Presentation, Error> {
    // An old workbook (.xls) is read as the .xlsx made from it.
    let converted;
    let data = if ppt::is_ppt(data) {
        converted = ppt_write::to_pptx(data)?;
        converted.as_slice()
    } else if xls::is_xls(data) {
        converted = xls::to_xlsx(data)?;
        converted.as_slice()
    } else if ods::is_ods(data) {
        converted = ods::to_xlsx(data)?;
        converted.as_slice()
    } else if xlsb::is_xlsb(data) {
        converted = xlsb::to_xlsx(data)?;
        converted.as_slice()
    } else if odp::is_odp(data) {
        converted = odp::to_pptx(data)?;
        converted.as_slice()
    } else if docx_convert::is_odt(data) {
        converted = docx_convert::import_odt_to_docx_bytes(data)?;
        converted.as_slice()
    } else {
        data
    };
    if reader::is_workbook(data)? {
        reader::read_xlsx(data)
    } else if docx::is_document(&package::Package::new(data)?)? {
        docx::read(data)
    } else {
        reader::read(data)
    }
}

/// The kinds of document, as their files are named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Presentation,
    Workbook,
    Document,
}

impl Kind {
    /// The kind a file's extension names (`pptx`, `xlsx`, `docx`, any case; `csv`, `tsv` and `tab` text tables are workbooks;
    /// with macros `pptm`, `xlsm`, `docm`, opened and saved as they are
    /// without running the macros).
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "pptx" | "pptm" | "ppsx" | "ppsm" | "potx" | "potm" | "ppt" | "pps" | "pot" | "odp" | "otp" => Some(Kind::Presentation),
            "xlsx" | "xlsm" | "xlsb" | "xls" | "ods" | "ots" | "csv" | "tsv" | "tab" | "xltx" | "xltm" | "xlk" => Some(Kind::Workbook),
            "docx" | "docm" | "dotx" | "dotm" | "odt" | "ott" => Some(Kind::Document),
            _ => None,
        }
    }

    /// The extensions of the files that can be opened (what `from_extension` knows), for the file dialog.
    pub const OPENABLE_EXTENSIONS: &[&str] = &["pptx", "pptm", "ppsx", "ppsm", "potx", "potm", "ppt", "pps", "pot", "odp", "otp", "xlsx", "xlsm", "xlsb", "xls", "ods", "ots", "csv", "tsv", "tab", "xltx", "xltm", "xlk", "docx", "docm", "dotx", "dotm"];

    /// The kind of document contained in `presentation`.
    pub fn of(presentation: &Presentation) -> Self {
        if presentation.workbook.is_some() {
            Kind::Workbook
        } else if presentation.flow.is_some() {
            Kind::Document
        } else {
            Kind::Presentation
        }
    }

    /// Whether `extension` names a file with macros (`docm`, `xlsm`, `pptm`, `ppsm`, `potm`).
    pub fn has_macros(extension: &str) -> bool {
        matches!(extension.to_ascii_lowercase().as_str(), "docm" | "xlsm" | "pptm" | "ppsm" | "potm")
    }
}

/// Whether `extension` names a PowerPoint slide show (`ppsx` or `ppsm`).
pub fn is_slideshow(extension: &str) -> bool {
    matches!(extension.to_ascii_lowercase().as_str(), "ppsx" | "ppsm")
}

/// Whether `extension` names an Office template (`potx`, `potm`, `xltx`, `xltm`, `dotx`, `dotm`, `crtx`).
pub fn is_template(extension: &str) -> bool {
    matches!(extension.to_ascii_lowercase().as_str(), "potx" | "potm" | "xltx" | "xltm" | "dotx" | "dotm" | "crtx")
}

/// Whether `extension` names an Office chart template (`crtx`).
pub fn is_chart_template(extension: &str) -> bool {
    extension.eq_ignore_ascii_case("crtx")
}

pub use blank::blank_file;
pub use ppt::dump as ppt_dump;
pub use ppt_write::to_pptx as ppt_to_pptx;
pub use odp::to_pptx as odp_to_pptx;
pub use xlsb::to_xlsx as xlsb_to_xlsx;
pub use cell_style::{
    PresetCellStyle, preset_cell_styles, preset_cell_styles_of, preset_cell_styles_with_theme, tinted,
    workbook_theme_colors, workbook_theme_colors_from_slice,
};

/// An empty document of `kind`, as a new one looks: one blank slide, an
/// empty worksheet with its grid, or one blank A4 page.
pub fn blank(kind: Kind) -> Presentation {
    use ibis_doc_model::{EmuRect, EmuSize, FlowDocument, PageSetup, Shape, ShapeKind, Sheet, Slide};
    let frame_shape = |presentation: &mut Presentation, frame: EmuRect, kind: ShapeKind| Shape {
        id: presentation.new_shape_id(),
        frame,
        rotation: 0,
        flip_h: false,
        flip_v: false,
        kind,
        floating: None,
    };
    match kind {
        Kind::Presentation => Presentation::new(),
        Kind::Workbook => {
            // Excel's defaults: 8.43 characters (64 px) wide, 15 pt high.
            let (width, height) = (64 * ibis_doc_model::EMU_PER_POINT, 15 * ibis_doc_model::EMU_PER_POINT);
            let (columns, rows) = (reader::MIN_GRID_COLUMNS as usize, reader::MIN_GRID_ROWS as usize);
            let sheet = Sheet { columns: vec![width; columns], rows: vec![height; rows], grid_lines: true, headings: true, show_ruler: true, ..Sheet::default() };
            let mut presentation = Presentation::empty(EmuSize::new(1, 1));
            let mut slide = Slide::new(presentation.new_slide_id());
            slide.name = Some("Sheet1".into());
            slide.size = Some(EmuSize::new(1, 1));
            let frame = EmuRect::new(0, 0, width * columns as i64, height * rows as i64);
            let slide_id = slide.id;
            slide.shapes.push(frame_shape(&mut presentation, frame, ShapeKind::Sheet(std::sync::Arc::new(sheet))));
            presentation.slides.push(slide);
            let workbook = ibis_doc_model::Workbook {
                sheets: vec![ibis_doc_model::WorkbookSheet {
                    name: "Sheet1".into(),
                    state: ibis_doc_model::SheetState::Visible,
                    slide: Some(slide_id),
                    hidden: None,
                    other: false,
                }],
                ..Default::default()
            };
            presentation.workbook = Some(std::sync::Arc::new(workbook));
            presentation
        }
        Kind::Document => {
            let page = PageSetup::default();
            let document = std::sync::Arc::new(FlowDocument { page, ..FlowDocument::default() });
            let mut presentation = Presentation::empty(page.size);
            let mut slide = Slide::new(presentation.new_slide_id());
            let frame = EmuRect::new(0, 0, page.size.width, page.size.height);
            slide.shapes.push(frame_shape(&mut presentation, frame, ShapeKind::Page { document: document.clone(), index: 0 }));
            presentation.slides.push(slide);
            presentation.flow = Some(document);
            presentation
        }
    }
}

/// Read a `.docx` document. Its content is in [`Presentation::flow`], for
/// the viewer to lay out into pages; the presentation holds one slide for
/// the first page.
pub fn read_docx(data: &[u8]) -> Result<Presentation, Error> {
    docx::read(data)
}

/// Read a `.xlsx` workbook: one slide per visible worksheet.
pub fn read_xlsx(data: &[u8]) -> Result<Presentation, Error> {
    reader::read_xlsx(data)
}
