// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Saving: a file as it was read, with the changes made to its
//! [`Presentation`] written into it.
//!
//! How a save works:
//!
//! 1. [`open`] reads a file into a `Presentation` (as [`crate::read`] does)
//!    and keeps, in a [`Source`], everything needed to write it again: the
//!    whole original package (every part's bytes), the `Presentation` as it
//!    was read (the *baseline*), and a *source map* telling where in the
//!    package each thing of the model came from (a part and the path of an
//!    element in it, see `ibis_ooxml::XmlDocument::element_path`).
//! 2. Edits go through `ibis_doc_model::Command`s, with undo and redo, and
//!    change only the model.
//! 3. [`Source::save`] compares the current `Presentation` with the
//!    baseline. For each difference, the format's writer (`pptx.rs`,
//!    `xlsx.rs`, `docx.rs`) finds the element the source map names in the
//!    original part, parsed as a tree that keeps everything
//!    (`ibis_ooxml::xml`), and changes that element only. Parts with no
//!    change are written back as their original compressed bytes.
//!
//! A save is a function of the original file, the baseline and the current
//! model, so saving again (after more edits) starts from the original file
//! again and needs nothing from the previous save.
//!
//! A change a writer cannot write yet is an error naming it, never dropped
//! silently: the file is not written, and the user is told.
//!
//! Adding an edit a format can save: see `tasks/doc/editing-status.md`
//! (「形式の担当が機能を足す手順」).

pub mod docx;
pub mod pptx;
pub(crate) mod pptx_new;
mod pptx_sections;
mod pptx_groups;
pub(crate) mod pptx_chart;
mod pptx_comments;
mod pptx_media_timing;
mod pptx_timing;
mod pptx_foreign;
mod pptx_header_footer;
mod pptx_links;
mod pptx_theme;
mod pptx_chart_type;
pub(crate) mod chart_topology;
pub(crate) mod pptx_chart_data;
pub use pptx_foreign::ForeignSlide;
pub(crate) mod pptx_new_chart;
pub(crate) mod pptx_chart_ex;
pub use pptx_new_chart::NewChartKind;

#[derive(Clone)]
pub(crate) struct NewChartEntry {
    pub(crate) kind: NewChartKind,
    pub(crate) template: Option<ibis_doc_model::Chart>,
    /// A chart of a sheet: where its series take their data from.
    pub(crate) refs: Vec<pptx_chart_data::SeriesRefs>,
}

pub(crate) mod pptx_notes;
mod pptx_media_prune;
mod pptx_layouts;
pub(crate) mod pptx_shapes;
pub(crate) mod pptx_table;
pub(crate) mod pptx_transition;
mod pptx_show;
mod pptx_text;
pub mod xlsx;

use std::sync::Arc;

use ibis_doc_model::Presentation;
pub use ibis_ooxml::SaveOptions;
use ibis_ooxml::Package;
use ibis_ooxml::zip::{EntryMeta, Writer};

use crate::{Error, Kind};

/// Reject shapes unsupported by the new-shape writer before destructive edits.
/// Successful serialization does not establish Office rendering fidelity.
pub fn check_shape_insertion(shape: &ibis_doc_model::Shape) -> Result<(), Error> {
    pptx_new::check_shape_insertion(shape)
}

impl From<ibis_ooxml::Error> for Error {
    fn from(err: ibis_ooxml::Error) -> Self {
        Error::new(err.message())
    }
}

/// Where the things of a model came from, by format.
#[derive(Clone, Debug)]
pub(crate) enum SourceMap {
    Presentation(pptx::SourceMap),
    Workbook(xlsx::SourceMap),
    Document(docx::SourceMap),
}

/// What is needed to save a file that was read: see the module's documentation.
#[derive(Clone)]
pub struct Source {
    kind: Kind,
    package: Package,
    baseline: Presentation,
    map: SourceMap,
    /// New slides made with their layout's placeholders ([`Source::new_slide`]).
    new_slides: Arc<std::sync::Mutex<std::collections::HashMap<ibis_doc_model::SlideId, pptx::NewSlide>>>,
    /// New charts ([`Source::new_chart`]): the template as read (`None`
    /// while it is being made).
    new_charts: Arc<std::sync::Mutex<std::collections::HashMap<ibis_doc_model::ShapeId, NewChartEntry>>>,
    /// Charts' data edited in the workbook editor ([`Source::set_chart_data`]).
    chart_data: Arc<std::sync::Mutex<pptx_chart_data::ChartDataEdits>>,
}

/// A file read for editing: its model, and what saving it needs.
pub struct Opened {
    pub presentation: Presentation,
    pub source: Source,
}

/// A chunk of streaming CSV cells and row heights sent to the UI during progressive loading.
#[derive(Clone, Debug)]
pub struct CsvProgressiveChunk {
    pub cells: Vec<ibis_doc_model::SheetCell>,
    pub heights: Vec<ibis_doc_model::Emu>,
    pub current_rows: usize,
    pub is_last: bool,
}

/// Read a `.pptx`, `.xlsx`, `.docx`, or `.csv` for editing and saving.
/// For large CSV files exceeding `first_chunk_rows`, the presentation returned holds the first
/// `first_chunk_rows` rows (so the UI can render immediately), and a channel receiver yields
/// subsequent chunks in the background.
pub fn open_progressive(
    data: impl Into<Arc<[u8]>>,
    first_chunk_rows: usize,
) -> Result<(Opened, Option<std::sync::mpsc::Receiver<CsvProgressiveChunk>>), Error> {
    let mut data: Arc<[u8]> = data.into();
    crate::progress::reset();
    // An old workbook (.xls): read into the .xlsx the rest reads.
    if crate::ppt::is_ppt(&data) {
        data = crate::ppt_write::to_pptx(&data)?.into();
    }
    if crate::xls::is_xls(&data) {
        data = crate::xls::to_xlsx(&data)?.into();
    }
    // An OpenDocument spreadsheet (.ods) likewise.
    if crate::ods::is_ods(&data) {
        data = crate::ods::to_xlsx(&data)?.into();
    }
    // A binary workbook (.xlsb) likewise.
    if crate::xlsb::is_xlsb(&data) {
        data = crate::xlsb::to_xlsx(&data)?.into();
    }
    // An OpenDocument presentation (.odp) likewise.
    if crate::odp::is_odp(&data) {
        data = crate::odp::to_pptx(&data)?.into();
    }
    // An OpenDocument text (.odt) likewise.
    if crate::docx_convert::is_odt(&data) {
        data = crate::docx_convert::import_odt_to_docx_bytes(&data)?.into();
    }

    let mut csv_records = None;
    #[allow(unused_mut)]
    let mut csv_rx = None;

    if crate::csv::is_csv(&data) {
        let text: std::sync::Arc<str> = std::sync::Arc::from(crate::csv::decode(&data)?);
        crate::progress::fraction(0.02);
        let delimiter = crate::csv::detect_delimiter(&text);
        let mut iter = crate::csv::CsvRecordIter::new(&text, delimiter);

        let mut first = Vec::new();
        while first.len() < first_chunk_rows {
            if let Some(record) = iter.next() {
                first.push(record);
            } else {
                break;
            }
        }
        crate::progress::fraction(0.15);

        let remaining_offset = iter.position();
        // The cells the first rows make: room the last chunk's list keeps in front of its own, for the app to put them there
        // without a copy of the whole list.
        let first_cells_hint: usize = first.iter().map(|r| r.len()).sum();
        // The cells the rest will make, from the size of the text left against the first rows' (a few per cent over): the list
        // that gets them is made this big at once, so that it neither grows by doubling (3.7 GB for 2.3 GB of 21 million cells)
        // nor stands beside the batches it is filled from.
        let estimated_cells = if first.is_empty() || remaining_offset == 0 {
            0
        } else {
            let rest_bytes = (text.len() - remaining_offset.min(text.len())) as f64;
            let per_row = remaining_offset as f64 / first.len() as f64;
            let cells_per_row = first_cells_hint as f64 / first.len() as f64;
            (rest_bytes / per_row.max(1.0) * cells_per_row * 1.03) as usize
        };
        if remaining_offset < text.len() {
            #[cfg(not(target_family = "wasm"))]
            {
                let (tx, rx) = std::sync::mpsc::channel();
                csv_rx = Some(rx);

                let text_clone = text.clone();
                std::thread::spawn(move || {
                    let mut iter = crate::csv::CsvRecordIter::new(&text_clone[remaining_offset..], delimiter);
                    const BATCH_SIZE: usize = 100_000;
                    let mut batch = Vec::with_capacity(BATCH_SIZE);
                    let mut processed = first_chunk_rows;
                    let base_style = ibis_doc_model::CellStyle { vertical: ibis_doc_model::CellVerticalAlign::Top, vertical_set: true, ..Default::default() };
                    let mut sheet_styles = Vec::new();
                    let mut all_cells: Vec<ibis_doc_model::SheetCell> = Vec::with_capacity(estimated_cells + first_cells_hint);
                    let mut all_heights = Vec::new();

                    while let Some(rec) = iter.next() {
                        batch.push(rec);
                        if batch.len() >= BATCH_SIZE {
                            let (cells, heights) = crate::xlsx::process_csv_chunk(
                                processed,
                                &batch,
                                &base_style,
                                &mut sheet_styles,
                            );
                            processed += batch.len();
                            batch.clear();
                            all_cells.extend(cells);
                            all_heights.extend(heights);

                            // Send lightweight progress update to UI without moving heavy cells
                            if tx.send(CsvProgressiveChunk {
                                cells: Vec::new(),
                                heights: Vec::new(),
                                current_rows: processed,
                                is_last: false,
                            }).is_err() {
                                return;
                            }
                        }
                    }
                    if !batch.is_empty() {
                        let (cells, heights) = crate::xlsx::process_csv_chunk(
                            processed,
                            &batch,
                            &base_style,
                            &mut sheet_styles,
                        );
                        processed += batch.len();
                        all_cells.extend(cells);
                        all_heights.extend(heights);
                    }
                    let _ = tx.send(CsvProgressiveChunk {
                        cells: all_cells,
                        heights: all_heights,
                        current_rows: processed,
                        is_last: true,
                    });
                });
            }
        }

        csv_records = Some(first);
        data = crate::csv::workbook_from("", "Sheet1")?.into();
    }

    let package = Package::open(data.clone())?;
    let mut csv_baseline = None;
    let (kind, presentation, map) = if crate::reader::is_workbook(&data)? {
        let (presentation, map) = match &csv_records {
            // The file as it was read is the empty sheet: the cells are changes to save.
            Some(records) => {
                let (empty, map) = crate::reader::read_xlsx_with_sources(&data)?;
                csv_baseline = Some(empty);
                (crate::reader::read_xlsx_with_records(&data, Some(records))?.0, map)
            }
            None => crate::reader::read_xlsx_with_sources(&data)?,
        };
        (Kind::Workbook, presentation, SourceMap::Workbook(map))
    } else if crate::docx::is_document(&crate::package::Package::new(&data)?)? {
        let (presentation, map) = crate::docx::read_with_sources(&data)?;
        (Kind::Document, presentation, SourceMap::Document(map))
    } else {
        let (presentation, map) = crate::reader::read_with_sources(&data)?;
        (Kind::Presentation, presentation, SourceMap::Presentation(map))
    };
    let source = Source { kind, package, baseline: csv_baseline.unwrap_or_else(|| presentation.clone()), map, new_slides: Default::default(), new_charts: Default::default(), chart_data: Default::default() };
    crate::progress::fraction(1.0);
    #[cfg(not(target_family = "wasm"))]
    std::thread::spawn(move || drop(csv_records));
    Ok((Opened { presentation, source }, csv_rx))
}

/// Read a `.pptx`, `.xlsx` or `.docx` for editing and saving.
pub fn open(data: impl Into<Arc<[u8]>>) -> Result<Opened, Error> {
    open_progressive(data, usize::MAX).map(|(opened, _)| opened)
}

impl Source {
    /// The package read (for the parts the model does not keep, such as the theme).
    pub(crate) fn package(&self) -> &Package {
        &self.package
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// The model as it was read.
    pub fn baseline(&self) -> &Presentation {
        &self.baseline
    }

    /// The file with the changes from the baseline to `current` written in.
    /// With no changes, it is the original file byte for byte (unless
    /// `options.regenerate_xml`).
    pub fn save(&self, current: &Presentation, options: SaveOptions) -> Result<Vec<u8>, Error> {
        // テーマの色・フォント・効果 changed: the themes written first, and the
        // file read back as the baseline (what the theme gives the shapes
        // the model has not changed).
        if let SourceMap::Presentation(_) = &self.map
            && (current.theme_colors != self.baseline.theme_colors
                || current.theme_fonts != self.baseline.theme_fonts
                || current.theme_effects != self.baseline.theme_effects)
        {
            let themed = self.themed(&current.theme_colors, current.theme_fonts.as_ref(), current.theme_effects.as_deref())?;
            return themed.save_as_is(current, options);
        }
        self.save_as_is(current, options)
    }

    /// Save as a PowerPoint Slide Show (`.ppsx` or `.ppsm`), setting the main
    /// presentation part's content type to `presentationml.slideshow.main+xml`
    /// (or `slideshow.macroEnabled.main+xml`).
    pub fn save_slideshow(&self, current: &Presentation, options: SaveOptions) -> Result<Vec<u8>, Error> {
        let SourceMap::Presentation(map) = &self.map else {
            return Err(Error::new("プレゼンテーションではありません"));
        };
        let is_macro = self.package.content_type(&map.presentation)?
            .is_some_and(|ct| ct.contains("macroEnabled"));
        let ct = if is_macro {
            "application/vnd.ms-powerpoint.slideshow.macroEnabled.main+xml"
        } else {
            "application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml"
        };
        let mut s = self.clone();
        s.package.set_override(&map.presentation, ct)?;
        s.save(current, options)
    }

    /// Save as a PowerPoint Presentation (`.pptx` or `.pptm`), setting the main
    /// presentation part's content type to `presentationml.presentation.main+xml`
    /// (or `presentation.macroEnabled.main+xml`).
    pub fn save_presentation(&self, current: &Presentation, options: SaveOptions) -> Result<Vec<u8>, Error> {
        let SourceMap::Presentation(map) = &self.map else {
            return Err(Error::new("プレゼンテーションではありません"));
        };
        let is_macro = self.package.content_type(&map.presentation)?
            .is_some_and(|ct| ct.contains("macroEnabled"));
        let ct = if is_macro {
            "application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml"
        } else {
            "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"
        };
        let mut s = self.clone();
        s.package.set_override(&map.presentation, ct)?;
        s.save(current, options)
    }

    /// Save as an Office Template (`.potx`, `.potm`, `.xltx`, `.xltm`, `.dotx`, `.dotm`),
    /// setting the main part's content type to the appropriate template content type.
    pub fn save_template(&self, current: &Presentation, options: SaveOptions) -> Result<Vec<u8>, Error> {
        let (main_part, ct) = match &self.map {
            SourceMap::Presentation(map) => {
                let is_macro = self.package.content_type(&map.presentation)?
                    .is_some_and(|ct| ct.contains("macroEnabled"));
                let ct = if is_macro {
                    "application/vnd.ms-powerpoint.template.macroEnabled.main+xml"
                } else {
                    "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml"
                };
                (&map.presentation, ct)
            }
            SourceMap::Workbook(map) => {
                let is_macro = self.package.content_type(&map.workbook)?
                    .is_some_and(|ct| ct.contains("macroEnabled"));
                let ct = if is_macro {
                    "application/vnd.ms-excel.template.macroEnabled.main+xml"
                } else {
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml"
                };
                (&map.workbook, ct)
            }
            SourceMap::Document(map) => {
                let is_macro = self.package.content_type(&map.body_part)?
                    .is_some_and(|ct| ct.contains("macroEnabled"));
                let ct = if is_macro {
                    "application/vnd.ms-word.template.macroEnabled.main+xml"
                } else {
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml"
                };
                (&map.body_part, ct)
            }
        };
        let mut s = self.clone();
        s.package.set_override(main_part, ct)?;
        s.save(current, options)
    }

    /// Save the active theme as an Office Theme file (`.thmx`).
    pub fn save_theme(&self, current: &Presentation, theme_name: Option<&str>) -> Result<Vec<u8>, Error> {
        let theme_part = match &self.map {
            SourceMap::Presentation(map) => {
                let parts = pptx_theme::theme_parts(&self.package, &map.presentation)?;
                parts.into_iter().next()
                    .or_else(|| self.package.names().find(|n| n.ends_with("theme1.xml")).map(|s| s.to_string()))
            }
            SourceMap::Workbook(_) | SourceMap::Document(_) => {
                self.package.names().find(|n| n.ends_with("theme1.xml")).map(|s| s.to_string())
            }
        };

        let theme_xml = if let Some(part) = theme_part {
            let mut doc = self.package.xml(&part)?.clone();
            pptx_theme::write_theme_elements(&mut doc, &current.theme_colors, current.theme_fonts.as_ref());
            pptx_theme::write_object_defaults(&mut doc, &current.object_defaults)?;
            if let Some(effects) = current.theme_effects.as_deref() {
                pptx_theme::write_theme_effects(&mut doc, effects)?;
            }
            if let Some(name) = theme_name {
                if let Some(root) = doc.root_element() {
                    doc.set_attr(root, "name", name);
                }
            }
            doc.to_bytes()
        } else {
            let name = theme_name.unwrap_or("Office Theme");
            let mut colors_xml = String::new();
            for (k, rgb) in &current.theme_colors {
                colors_xml.push_str(&format!("<a:{k}><a:srgbClr val=\"{:06X}\"/></a:{k}>", rgb.0 & 0xffffff));
            }
            let fmt_scheme = current.theme_effects.as_deref()
                .and_then(crate::theme_effects::find_effect_preset)
                .map(crate::theme_effects::generate_fmt_scheme_xml)
                .unwrap_or_else(|| {
                    format!(
                        r#"<a:fmtScheme name="{name}">
      <a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:tint val="50000"/><a:satMod val="300000"/></a:schemeClr></a:gs><a:gs pos="35000"><a:schemeClr val="phClr"><a:tint val="37000"/><a:satMod val="220000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:tint val="15000"/><a:satMod val="131000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="16200000" scaled="1"/></a:gradFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:shade val="100000"/><a:satMod val="130000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:tint val="50000"/><a:satMod val="350000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="16200000" scaled="0"/></a:gradFill></a:fillStyleLst>
      <a:lnStyleLst><a:ln w="9525" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln><a:ln w="25400" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln><a:ln w="38100" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/></a:ln></a:lnStyleLst>
      <a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>
      <a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"><a:tint val="95000"/><a:satMod val="170000"/></a:schemeClr></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:tint val="93000"/><a:satMod val="150000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:shade val="98000"/><a:satMod val="130000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill></a:bgFillStyleLst>
    </a:fmtScheme>"#
                    )
                });
            format!(
                r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="{name}">
  <a:themeElements>
    <a:clrScheme name="{name}">
      {colors_xml}
    </a:clrScheme>
    <a:fontScheme name="{name}">
      <a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont>
      <a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont>
    </a:fontScheme>
    {fmt_scheme}
  </a:themeElements>
  <a:objectDefaults/>
  <a:extraClrSchemeLst/>
</a:theme>"#
            ).into_bytes()
        };

        let mut writer = Writer::new();
        let meta = EntryMeta::new();

        let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/theme/theme/themeManager.xml" ContentType="application/vnd.openxmlformats-officedocument.themeManager+xml"/>
  <Override PartName="/theme/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
</Types>"#;
        writer.add("[Content_Types].xml", meta, content_types.as_bytes())?;

        let root_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="theme/theme/themeManager.xml"/>
</Relationships>"#;
        writer.add("_rels/.rels", meta, root_rels.as_bytes())?;

        let theme_mgr_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme1.xml"/>
</Relationships>"#;
        writer.add("theme/theme/_rels/themeManager.xml.rels", meta, theme_mgr_rels.as_bytes())?;

        let theme_mgr = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<thm:themeManager xmlns:thm="http://schemas.openxmlformats.org/drawingml/2006/thememanager"/>"#;
        writer.add("theme/theme/themeManager.xml", meta, theme_mgr.as_bytes())?;

        writer.add("theme/theme/theme1.xml", meta, &theme_xml)?;

        let bytes = writer.finish()?;
        Ok(bytes)
    }

    /// Save without the macros (`.xlsx` from an `.xlsm`, `.xltx` from an
    /// `.xltm`, and so on): the main part's content type the one without
    /// macros, and the macros' parts (`vbaProject.bin`, its signature and
    /// `vbaData.xml`) and the relationships to them left out. A file
    /// without macros is saved as it is.
    pub fn save_without_macros(&self, current: &Presentation, options: SaveOptions) -> Result<Vec<u8>, Error> {
        let main_part = match &self.map {
            SourceMap::Presentation(map) => &map.presentation,
            SourceMap::Workbook(map) => &map.workbook,
            SourceMap::Document(map) => &map.body_part,
        };
        let mut s = self.clone();
        let current_type = s.package.content_type(main_part)?.unwrap_or_default();
        if let Some(plain) = current_type.contains("macroEnabled").then(|| {
            current_type
                .replace("application/vnd.ms-excel.sheet.macroEnabled.main+xml", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml")
                .replace("application/vnd.ms-excel.template.macroEnabled.main+xml", "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml")
                .replace("application/vnd.ms-word.document.macroEnabled.main+xml", "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml")
                .replace("application/vnd.ms-word.template.macroEnabled.main+xml", "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml")
                .replace("application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml", "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml")
                .replace("application/vnd.ms-powerpoint.slideshow.macroEnabled.main+xml", "application/vnd.openxmlformats-officedocument.presentationml.slideshow.main+xml")
                .replace("application/vnd.ms-powerpoint.template.macroEnabled.main+xml", "application/vnd.openxmlformats-officedocument.presentationml.template.main+xml")
        }) {
            s.package.set_override(main_part, &plain)?;
        }
        let is_macro_part = |name: &str| {
            let l = name.to_ascii_lowercase();
            let base = l.rsplit('/').next().unwrap_or("");
            base == "vbaproject.bin" || base == "vbaprojectsignature.bin" || base == "vbadata.xml"
        };
        let names: Vec<String> = s.package.names().map(str::to_string).collect();
        for name in &names {
            if name.ends_with(".rels") || is_macro_part(name) {
                continue;
            }
            let Ok(rels) = s.package.relationships(name) else { continue };
            for rel in rels.iter().filter(|r| !r.external && is_macro_part(&r.target)).collect::<Vec<_>>() {
                s.package.remove_relationship(name, &rel.id)?;
            }
        }
        for name in names.iter().filter(|n| is_macro_part(n)) {
            let _ = s.package.remove_override(name);
            s.package.remove(name);
        }
        s.save(current, options)
    }

    /// The file as read, with the theme `colors`, `fonts`, and optional `effects` written in.
    fn themed_bytes(&self, colors: &[(String, ibis_doc_model::Rgb)], fonts: Option<&ibis_doc_model::ThemeFonts>, effects: Option<&str>) -> Result<Vec<u8>, Error> {
        let SourceMap::Presentation(map) = &self.map else { return Err(unsupported("テーマ")) };
        let mut package = self.package.clone();
        pptx_theme::write(&mut package, &map.presentation, colors, fonts)?;
        if let Some(effects) = effects {
            pptx_theme::write_effects(&mut package, &map.presentation, Some(effects))?;
        }
        Ok(package.save(SaveOptions::default())?)
    }

    /// The source for the file with the theme `colors` and `fonts`: the
    /// themes written, the baseline read back; the new slides and charts
    /// stay.
    fn themed(&self, colors: &[(String, ibis_doc_model::Rgb)], fonts: Option<&ibis_doc_model::ThemeFonts>, effects: Option<&str>) -> Result<Source, Error> {
        let data = self.themed_bytes(colors, fonts, effects)?;
        let opened = open(data)?;
        let mut source = opened.source;
        source.new_slides = self.new_slides.clone();
        source.new_charts = self.new_charts.clone();
        source.chart_data = self.chart_data.clone();
        Ok(source)
    }

    /// テーマの色・フォント: the commands that give `current` the theme
    /// `colors` and `fonts`, with the shapes, backgrounds, backdrops and
    /// layouts it draws in the new colours (shapes the model has changed
    /// since it was read keep what they have).
    pub fn retheme(&self, current: &Presentation, colors: Vec<(String, ibis_doc_model::Rgb)>, fonts: Option<ibis_doc_model::ThemeFonts>) -> Result<Vec<ibis_doc_model::Command>, Error> {
        use ibis_doc_model::Command;
        let before = crate::read(&self.themed_bytes(&current.theme_colors, current.theme_fonts.as_ref(), current.theme_effects.as_deref())?)?;
        let after = crate::read(&self.themed_bytes(&colors, fonts.as_ref(), current.theme_effects.as_deref())?)?;
        let mut commands = vec![Command::SetTheme { colors, fonts }];
        for slide in &current.slides {
            let (Some(was), Some(now)) = (before.slide(slide.id), after.slide(slide.id)) else { continue };
            let mut new = slide.clone();
            for shape in &mut new.shapes {
                let (Some(old), Some(fresh)) = (was.shape(shape.id), now.shape(shape.id)) else { continue };
                let mut same = old.clone();
                same.frame = shape.frame;
                if same == *shape {
                    let frame = shape.frame;
                    *shape = fresh.clone();
                    shape.frame = frame;
                }
            }
            if slide.background == was.background { new.background = now.background.clone(); }
            new.backdrop = now.backdrop.iter().map(|s| { let mut s = s.clone(); s.frame = current.layout_scale.rect(s.frame); s }).collect();
            if new != *slide { commands.push(Command::SetSlide { slide: new }); }
        }
        let layouts = after.layouts.iter().map(|l| l.scaled(current.layout_scale)).collect();
        commands.push(Command::SetLayouts { layouts, scale: current.layout_scale });
        Ok(commands)
    }

    /// Apply theme `colors` and `fonts` to only the specified slides (`slide_ids`).
    /// The shapes, backgrounds and backdrops of the specified slides are updated to
    /// their appearance under the new theme colors, while leaving the rest of the
    /// presentation intact.
    pub fn retheme_slides(&self, current: &Presentation, slide_ids: &[ibis_doc_model::SlideId], colors: Vec<(String, ibis_doc_model::Rgb)>, fonts: Option<ibis_doc_model::ThemeFonts>) -> Result<Vec<ibis_doc_model::Command>, Error> {
        use ibis_doc_model::Command;
        let before = crate::read(&self.themed_bytes(&current.theme_colors, current.theme_fonts.as_ref(), None)?)?;
        let after = crate::read(&self.themed_bytes(&colors, fonts.as_ref(), None)?)?;
        let mut commands = Vec::new();
        for slide in &current.slides {
            if !slide_ids.contains(&slide.id) { continue; }
            let (Some(was), Some(now)) = (before.slide(slide.id), after.slide(slide.id)) else { continue };
            let mut new = slide.clone();
            for shape in &mut new.shapes {
                let (Some(old), Some(fresh)) = (was.shape(shape.id), now.shape(shape.id)) else { continue };
                let mut same = old.clone();
                same.frame = shape.frame;
                if same == *shape {
                    let frame = shape.frame;
                    *shape = fresh.clone();
                    shape.frame = frame;
                }
            }
            if slide.background == was.background { new.background = now.background.clone(); }
            new.backdrop = now.backdrop.iter().map(|s| { let mut s = s.clone(); s.frame = current.layout_scale.rect(s.frame); s }).collect();
            if new != *slide { commands.push(Command::SetSlide { slide: new }); }
        }
        Ok(commands)
    }

    /// テーマ効果 (Theme Effects: `a:fmtScheme`): the commands that give `current` the new
    /// theme effect preset, re-evaluating shapes styled with theme effects.
    pub fn retheme_effects(&self, current: &Presentation, effects: &str) -> Result<Vec<ibis_doc_model::Command>, Error> {
        use ibis_doc_model::Command;
        let before = crate::read(&self.themed_bytes(&current.theme_colors, current.theme_fonts.as_ref(), current.theme_effects.as_deref())?)?;
        let after = crate::read(&self.themed_bytes(&current.theme_colors, current.theme_fonts.as_ref(), Some(effects))?)?;
        let mut commands = vec![Command::SetThemeEffects { name: Some(effects.to_string()) }];
        for slide in &current.slides {
            let (Some(was), Some(now)) = (before.slide(slide.id), after.slide(slide.id)) else { continue };
            let mut new = slide.clone();
            for shape in &mut new.shapes {
                let (Some(old), Some(fresh)) = (was.shape(shape.id), now.shape(shape.id)) else { continue };
                let mut same = old.clone();
                same.frame = shape.frame;
                if same == *shape {
                    let frame = shape.frame;
                    *shape = fresh.clone();
                    shape.frame = frame;
                }
            }
            if slide.background == was.background { new.background = now.background.clone(); }
            new.backdrop = now.backdrop.iter().map(|s| { let mut s = s.clone(); s.frame = current.layout_scale.rect(s.frame); s }).collect();
            if new != *slide { commands.push(Command::SetSlide { slide: new }); }
        }
        let layouts = after.layouts.iter().map(|l| l.scaled(current.layout_scale)).collect();
        commands.push(Command::SetLayouts { layouts, scale: current.layout_scale });
        Ok(commands)
    }


    fn save_as_is(&self, current: &Presentation, options: SaveOptions) -> Result<Vec<u8>, Error> {
        let mut package = self.package.clone();
        match &self.map {
            SourceMap::Presentation(map) => {
                let new_slides = self.new_slides.lock().unwrap_or_else(|e| e.into_inner());
                let new_charts = self.new_charts.lock().unwrap_or_else(|e| e.into_inner());
                let chart_data = self.chart_data.lock().unwrap_or_else(|e| e.into_inner());
                pptx::write_changes(&mut package, map, &self.baseline, current, &new_slides, &new_charts, &chart_data)?
            }
            SourceMap::Workbook(map) => {
                let chart_data = self.chart_data.lock().unwrap_or_else(|e| e.into_inner());
                let new_charts = self.new_charts.lock().unwrap_or_else(|e| e.into_inner());
                xlsx::write_changes(&mut package, map, &self.baseline, current, &chart_data, &new_charts)?
            }
            SourceMap::Document(map) => {
                let chart_data = self.chart_data.lock().unwrap_or_else(|e| e.into_inner());
                let new_charts = self.new_charts.lock().unwrap_or_else(|e| e.into_inner());
                docx::write_changes(&mut package, map, &self.baseline, current, &new_charts, &chart_data)?
            }
        }
        Ok(package.save(options)?)
    }
}

impl Source {
    /// 挿入 > グラフ: a new chart `id` in `frame` on slide `slide` of
    /// `current`, as PowerPoint makes one (a clustered column chart of its
    /// sample data, in an embedded workbook): written and read back, as the
    /// model holds it. Saving writes its chart part and the changes to it.
    pub fn new_chart(&self, current: &Presentation, slide: ibis_doc_model::SlideId, id: ibis_doc_model::ShapeId, frame: ibis_doc_model::EmuRect) -> Result<ibis_doc_model::Shape, Error> {
        self.new_chart_kind(current, slide, id, frame, NewChartKind::ClusteredColumn)
    }

    /// 挿入 > グラフ: a new chart of `kind` (`ClusteredColumn`, `Pie`, etc.)
    /// with its sample data in an embedded workbook.
    pub fn new_chart_kind(&self, current: &Presentation, slide: ibis_doc_model::SlideId, id: ibis_doc_model::ShapeId, frame: ibis_doc_model::EmuRect, kind: NewChartKind) -> Result<ibis_doc_model::Shape, Error> {
        if !matches!(self.map, SourceMap::Presentation(_)) {
            return Err(Error::new("プレゼンテーションではありません"));
        }
        let index = current.slide_index(slide).ok_or_else(|| Error::new("そのスライドがありません"))?;
        let empty = ibis_doc_model::Chart {
            groups: Vec::new(),
            axes: Vec::new(),
            legend: None,
            title: None,
            plot_area: None,
            background: ibis_doc_model::Fill::None,
            border: None,
            plot_background: ibis_doc_model::Fill::None,
            plot_border: None,
            disp_blanks_as: None,
            plot_vis_only: true,
            data_table: None,
            style: None,
            pivot_source: None,
            rounded_corners: false,
            title_overlay: false,
            ex: None,
            ex_format: None,
            excel_layout: false,
            word_layout: false,
            cell_refs: Vec::new(),
            ex_source: None,
        };
        let mut changed = current.clone();
        changed.slides[index].shapes.push(ibis_doc_model::Shape { id, frame, rotation: 0, flip_h: false, flip_v: false, kind: ibis_doc_model::ShapeKind::Chart(Box::new(empty)), floating: None });
        let lock = || self.new_charts.lock().unwrap_or_else(|e| e.into_inner());
        lock().insert(id, NewChartEntry { kind, template: None, refs: Vec::new() });
        let made = (|| {
            let again = crate::read(&self.save(&changed, SaveOptions::default())?)?;
            let read = again.slides.get(index).and_then(|s| s.shapes.last()).cloned().ok_or_else(|| Error::new("新しいグラフを読み直せません"))?;
            match &read.kind {
                ibis_doc_model::ShapeKind::Chart(c) => Ok(((**c).clone(), ibis_doc_model::Shape { id, ..read })),
                _ => Err(Error::new("新しいグラフを読み直せません")),
            }
        })();
        match made {
            Ok((chart, shape)) => {
                lock().insert(id, NewChartEntry { kind, template: Some(chart), refs: Vec::new() });
                Ok(shape)
            }
            Err(e) => {
                lock().remove(&id);
                Err(e)
            }
        }
    }

    /// Whether chart `id` was inserted (挿入 > グラフ) since the file was opened: only such a chart's changes can be saved.
    pub fn has_new_chart(&self, id: ibis_doc_model::ShapeId) -> bool {
        self.new_charts.lock().unwrap_or_else(|e| e.into_inner()).contains_key(&id)
    }

    /// 挿入 > グラフ in a document: a new inline chart of `kind` with its sample data in an embedded workbook
    /// at `at` (in the body), `size` big, written and read back as the model holds it. The shape's id is `id`.
    /// Saving writes its chart part and the changes made to it since.
    pub fn new_doc_chart(&self, current: &Presentation, at: ibis_doc_model::FlowPos, id: ibis_doc_model::ShapeId, size: ibis_doc_model::EmuSize, kind: NewChartKind) -> Result<ibis_doc_model::Shape, Error> {
        use ibis_doc_model::{FlowCommand, FlowRunKind, ShapeKind};
        if !matches!(self.map, SourceMap::Document(_)) {
            return Err(Error::new("文書ではありません"));
        }
        fn charts(blocks: &[ibis_doc_model::Block], out: &mut Vec<ibis_doc_model::Shape>) {
            for b in blocks {
                match b {
                    ibis_doc_model::Block::Paragraph(p) => {
                        for r in &p.runs {
                            if let FlowRunKind::Shape(s) = &r.kind && matches!(s.kind, ShapeKind::Chart(_)) {
                                out.push((**s).clone());
                            }
                        }
                    }
                    ibis_doc_model::Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| charts(&c.blocks, out)),
                }
            }
        }
        let flow = current.flow.as_ref().ok_or_else(|| Error::new("文書ではありません"))?;
        let shape = ibis_doc_model::Shape { id, frame: ibis_doc_model::EmuRect::new(0, 0, size.width, size.height), rotation: 0, flip_h: false, flip_v: false, kind: ShapeKind::Chart(Box::new(ibis_doc_model::Chart::empty())), floating: None };
        let mut changed = current.clone();
        let mut doc = (**flow).clone();
        ibis_doc_model::flow_command::apply_to(&mut doc, FlowCommand::InsertChart { at, shape: Box::new(shape) }).map_err(|_| Error::new("ここにはグラフを挿入できません"))?;
        let mut before = Vec::new();
        charts(&doc.body, &mut before);
        let ordinal = before.iter().position(|s| s.id == id).ok_or_else(|| Error::new("ここにはグラフを挿入できません"))?;
        changed.flow = Some(std::sync::Arc::new(doc));
        let lock = || self.new_charts.lock().unwrap_or_else(|e| e.into_inner());
        lock().insert(id, NewChartEntry { kind, template: None, refs: Vec::new() });
        let made = (|| {
            let again = crate::read(&self.save(&changed, SaveOptions::default())?)?;
            let mut read = Vec::new();
            charts(&again.flow.as_ref().ok_or_else(|| Error::new("新しいグラフを読み直せません"))?.body, &mut read);
            let read = read.get(ordinal).cloned().ok_or_else(|| Error::new("新しいグラフを読み直せません"))?;
            match &read.kind {
                ShapeKind::Chart(c) => Ok(((**c).clone(), ibis_doc_model::Shape { id, frame: ibis_doc_model::EmuRect::new(0, 0, size.width, size.height), floating: None, ..read })),
                _ => Err(Error::new("新しいグラフを読み直せません")),
            }
        })();
        match made {
            Ok((chart, shape)) => {
                lock().insert(id, NewChartEntry { kind, template: Some(chart), refs: Vec::new() });
                Ok(shape)
            }
            Err(e) => {
                lock().remove(&id);
                Err(e)
            }
        }
    }

    /// 挿入 > グラフ in a workbook: a new chart of `kind` on sheet `slide`
    /// of `current` over the cells `refs` name (the series by columns or
    /// rows: see [`pptx_chart_data::refs_for_range`]), no workbook of its
    /// own. Written and read back for its look, then valued from the cells.
    /// Saving writes its chart part and the changes made to it since.
    pub fn new_sheet_chart(&self, current: &Presentation, slide: ibis_doc_model::SlideId, id: ibis_doc_model::ShapeId, frame: ibis_doc_model::EmuRect, kind: NewChartKind, refs: Vec<pptx_chart_data::SeriesRefs>) -> Result<ibis_doc_model::Shape, Error> {
        if !matches!(self.map, SourceMap::Workbook(_)) {
            return Err(Error::new("ブックではありません"));
        }
        let index = current.slide_index(slide).ok_or_else(|| Error::new("そのシートがありません"))?;
        let empty = ibis_doc_model::Chart {
            groups: Vec::new(),
            axes: Vec::new(),
            legend: None,
            title: None,
            plot_area: None,
            background: ibis_doc_model::Fill::None,
            border: None,
            plot_background: ibis_doc_model::Fill::None,
            plot_border: None,
            disp_blanks_as: None,
            plot_vis_only: true,
            data_table: None,
            style: None,
            pivot_source: None,
            rounded_corners: false,
            title_overlay: false,
            ex: None,
            ex_format: None,
            excel_layout: false,
            word_layout: false,
            cell_refs: Vec::new(),
            ex_source: None,
        };
        let mut changed = current.clone();
        changed.slides[index].shapes.push(ibis_doc_model::Shape { id, frame, rotation: 0, flip_h: false, flip_v: false, kind: ibis_doc_model::ShapeKind::Chart(Box::new(empty)), floating: None });
        let lock = || self.new_charts.lock().unwrap_or_else(|e| e.into_inner());
        lock().insert(id, NewChartEntry { kind, template: None, refs: refs.clone() });
        let made = (|| {
            let again = crate::read(&self.save(&changed, SaveOptions::default())?)?;
            let read = again.slides.get(index).and_then(|s| s.shapes.last()).cloned().ok_or_else(|| Error::new("新しいグラフを読み直せません"))?;
            match &read.kind {
                ibis_doc_model::ShapeKind::Chart(c) => Ok(((**c).clone(), read)),
                _ => Err(Error::new("新しいグラフを読み直せません")),
            }
        })();
        match made {
            Ok((template, read)) => {
                // A chartEx chart (waterfall…) was read back with its cells' values cached: it is that.
                let mut chart = if kind.is_chart_ex() || refs.is_empty() { template.clone() } else { pptx_chart_data::sheet_chart_from_data(&template, current, &refs) };
                // A series with no name cell is named by its number, as Excel does ("系列1"), not by the template's sample name.
                if !kind.is_chart_ex() {
                    for (i, (series, r)) in chart.groups.iter_mut().flat_map(|g| g.series.iter_mut()).zip(refs.iter()).enumerate() {
                        if r.name.is_none() {
                            series.name = if crate::english_names() { format!("Series{}", i + 1) } else { format!("系列{}", i + 1) }; // i18n-ok（文書に保存する名前）
                        }
                    }
                }
                lock().insert(id, NewChartEntry { kind, template: Some(template), refs });
                Ok(ibis_doc_model::Shape { id, kind: ibis_doc_model::ShapeKind::Chart(Box::new(chart)), ..read })
            }
            Err(e) => {
                lock().remove(&id);
                Err(e)
            }
        }
    }

    /// The slide parts of a file's slides, in their order (for pasting its
    /// slides into another file: see [`Source::paste_slide`]).
    pub fn slide_parts(&self, presentation: &Presentation) -> Vec<Option<String>> {
        let SourceMap::Presentation(map) = &self.map else { return Vec::new() };
        presentation.slides.iter().map(|s| map.slides.get(&s.id).map(|src| src.part.clone())).collect()
    }

    /// スライドの貼り付け from another file (貼り付け先のテーマを使用): a
    /// new slide `id` at `index` of `current` made of `foreign`'s slide
    /// part (its shapes, pictures, charts…), following layout `layout` of
    /// this file: written and read back, its shapes numbered by `new_id`.
    /// Saving copies the part so, and writes the changes made to it since.
    /// Also returns the media (pictures) it shows, to add to the model, and
    /// the layouts it brings (keeping its formatting: its master's), to add
    /// to the model's.
    pub fn paste_slide(
        &self,
        current: &Presentation,
        index: usize,
        id: ibis_doc_model::SlideId,
        layout: ibis_doc_model::LayoutId,
        foreign: ForeignSlide,
        mut new_id: impl FnMut() -> ibis_doc_model::ShapeId,
    ) -> Result<(ibis_doc_model::Slide, Vec<ibis_doc_model::Media>, Vec<ibis_doc_model::Layout>), Error> {
        if !matches!(self.map, SourceMap::Presentation(_)) {
            return Err(Error::new("プレゼンテーションではありません"));
        }
        let layout = current.layout(layout).ok_or_else(|| Error::new("そのレイアウトがありません"))?;
        let index = index.min(current.slides.len());
        let mut changed = current.clone();
        changed.slides.insert(index, ibis_doc_model::Slide::from_layout(id, layout));
        let lock = || self.new_slides.lock().unwrap_or_else(|e| e.into_inner());
        lock().insert(id, pptx::NewSlide { shapes: Vec::new(), groups: Vec::new(), animations: Vec::new(), sources: Default::default(), foreign: Some(foreign.clone()) });
        let made = (|| {
            let opened = open(self.save(&changed, SaveOptions::default())?)?;
            let SourceMap::Presentation(map) = &opened.source.map else { return Err(Error::new("読み直せません")) };
            let again = opened.presentation.slides.get(index).ok_or_else(|| Error::new("貼り付けたスライドを読み直せません"))?;
            let mut ids: std::collections::HashMap<ibis_doc_model::ShapeId, ibis_doc_model::ShapeId> = std::collections::HashMap::new();
            for shape in &again.shapes {
                ids.insert(shape.id, new_id());
            }
            let sources = again.shapes.iter().filter_map(|s| Some((ids[&s.id], map.shapes.get(&s.id)?.clone()))).collect();
            let mut slide = renumbered(again, id, &ids);
            // The layouts it brings: the read-back file's after this file's
            // (its master is added last), numbered here.
            let mut layouts: Vec<ibis_doc_model::Layout> = opened.presentation.layouts.iter().skip(current.layouts.len()).cloned().collect();
            let mut layout_ids = std::collections::HashMap::new();
            for l in &mut layouts {
                let fresh = ibis_doc_model::LayoutId(new_id().0);
                layout_ids.insert(l.id, fresh);
                l.id = fresh;
                l.master = None;
            }
            // This file's layout (the read-back file numbers its own).
            slide.layout = match again.layout.and_then(|l| layout_ids.get(&l)) {
                Some(own) => Some(*own),
                None => Some(layout.id),
            };
            // The pictures it and its layouts show (wherever a media id is named).
            let named = format!("{slide:?}{layouts:?}");
            let media: Vec<ibis_doc_model::Media> = opened.presentation.media.iter().filter(|m| named.contains(&format!("{:?}", m.id))).cloned().collect();
            Ok((slide, sources, media, layouts))
        })();
        match made {
            Ok((slide, sources, media, layouts)) => {
                lock().insert(id, pptx::NewSlide { shapes: slide.shapes.clone(), groups: slide.groups.clone(), animations: slide.animations.clone(), sources, foreign: Some(foreign) });
                Ok((slide, media, layouts))
            }
            Err(e) => {
                lock().remove(&id);
                Err(e)
            }
        }
    }

    /// A new slide `id` to put at `index` of `current`, following layout
    /// `layout` with its placeholders empty, as PowerPoint makes one
    /// (新しいスライド): the file is written with it and the slide read back
    /// for them (their places, text formats and prompts), its shapes
    /// numbered by `new_id`. Saving writes them as the placeholders they are.
    pub fn new_slide(
        &self,
        current: &Presentation,
        index: usize,
        id: ibis_doc_model::SlideId,
        layout: ibis_doc_model::LayoutId,
        mut new_id: impl FnMut() -> ibis_doc_model::ShapeId,
    ) -> Result<ibis_doc_model::Slide, Error> {
        if !matches!(self.map, SourceMap::Presentation(_)) {
            return Err(Error::new("プレゼンテーションではありません"));
        }
        let layout = current.layout(layout).ok_or_else(|| Error::new("そのレイアウトがありません"))?;
        let index = index.min(current.slides.len());
        let mut changed = current.clone();
        changed.slides.insert(index, ibis_doc_model::Slide::from_layout(id, layout));
        let lock = || self.new_slides.lock().unwrap_or_else(|e| e.into_inner());
        lock().insert(id, pptx::NewSlide::default());
        let made = (|| {
            let opened = open(self.save(&changed, SaveOptions::default())?)?;
            let SourceMap::Presentation(map) = &opened.source.map else { return Err(Error::new("読み直せません")) };
            let again = opened.presentation.slides.get(index).ok_or_else(|| Error::new("新しいスライドを読み直せません"))?;
            let mut slide = ibis_doc_model::Slide::from_layout(id, layout);
            let mut sources = std::collections::HashMap::new();
            let mut ids = std::collections::HashMap::new();
            for shape in &again.shapes {
                let fresh = new_id();
                ids.insert(shape.id, fresh);
                if let Some(source) = map.shapes.get(&shape.id) {
                    sources.insert(fresh, source.clone());
                }
                slide.shapes.push(ibis_doc_model::Shape { id: fresh, ..shape.clone() });
            }
            slide.placeholders = again.placeholders.iter().filter_map(|(k, v)| Some((*ids.get(k)?, v.clone()))).collect();
            slide.shape_names = again.shape_names.iter().filter_map(|(k, v)| Some((*ids.get(k)?, v.clone()))).collect();
            slide.hidden_shapes = again.hidden_shapes.iter().filter_map(|k| ids.get(k).copied()).collect();
            slide.shape_links = again.shape_links.iter().filter_map(|(k, v)| Some((*ids.get(k)?, v.clone()))).collect();
            slide.shape_link_tooltips = again.shape_link_tooltips.iter().filter_map(|(k, v)| Some((*ids.get(k)?, v.clone()))).collect();
            slide.aspect_locked = again.aspect_locked.iter().filter_map(|k| ids.get(k).copied()).collect();
            slide.shape_alt_texts = again.shape_alt_texts.iter().filter_map(|(k, v)| Some((*ids.get(k)?, v.clone()))).collect();
            slide.decorative_shapes = again.decorative_shapes.iter().filter_map(|k| ids.get(k).copied()).collect();
            Ok((slide, sources))
        })();
        match made {
            Ok((slide, sources)) => {
                lock().insert(id, pptx::NewSlide { shapes: slide.shapes.clone(), groups: slide.groups.clone(), animations: slide.animations.clone(), sources, foreign: None });
                Ok(slide)
            }
            Err(e) => {
                lock().remove(&id);
                Err(e)
            }
        }
    }

    /// Slide `slide` of `current` following layout `layout` (レイアウトの
    /// 適用, see `Presentation::with_layout`), with what it inherits from
    /// the new layout and master (text formats, theme colours): the file is
    /// written with the change and the slide read back for that.
    pub fn with_layout(&self, current: &Presentation, slide: ibis_doc_model::SlideId, layout: ibis_doc_model::LayoutId) -> Result<ibis_doc_model::Slide, Error> {
        let mut new = current.with_layout(slide, layout).ok_or_else(|| Error::new("そのスライドかレイアウトがありません"))?;
        let index = current.slide_index(slide).ok_or_else(|| Error::new("そのスライドがありません"))?;
        let mut changed = current.clone();
        changed.slides[index] = new.clone();
        let data = self.save(&changed, SaveOptions::default())?;
        let reread = crate::read(&data)?;
        let Some(again) = reread.slides.get(index) else { return Ok(new) };
        if again.shapes.len() != new.shapes.len() {
            return Ok(new);
        }
        // Everything as read back (the new layout and master give text its
        // format, and colours their theme), under the model's ids.
        for (shape, read) in new.shapes.iter_mut().zip(&again.shapes) {
            let id = shape.id;
            *shape = read.clone();
            shape.id = id;
        }
        new.background = again.background.clone();
        Ok(new)
    }
}

impl Source {
    /// データの編集: chart `shape` of slide `slide` of `current` as its
    /// workbook and the references its series take their data from (as
    /// saving `current` writes them); a workbook made from the chart's data
    /// for a chart with none.
    pub fn chart_data(&self, current: &Presentation, slide: ibis_doc_model::SlideId, shape: ibis_doc_model::ShapeId) -> Result<pptx_chart_data::ChartData, Error> {
        let index = current.slide_index(slide).ok_or_else(|| Error::new("そのスライドがありません"))?;
        let at = current.slides[index].shape_index(shape).ok_or_else(|| Error::new("その図形がありません"))?;
        let ibis_doc_model::ShapeKind::Chart(chart) = &current.slides[index].shapes[at].kind else { return Err(Error::new("グラフではありません")) };
        let again = open(self.save(current, SaveOptions::default())?)?;
        let SourceMap::Presentation(map) = &again.source.map else { return Err(Error::new("プレゼンテーションではありません")) };
        let read = again.presentation.slides.get(index).filter(|s| s.shapes.len() == current.slides[index].shapes.len()).ok_or_else(|| Error::new("読み直せません"))?;
        let source = map.shapes.get(&read.shapes[at].id).ok_or_else(|| Error::new("グラフの部品がありません"))?;
        let part = pptx_shapes::chart_part_at(&again.source.package, &source.part, &source.path)?;
        pptx_chart_data::read_chart_data(&again.source.package, &part, chart)
    }

    /// The references the series of sheet chart `shape` (as `chart`) take their data from: those of the latest data edit that gave it these series, else the file's.
    pub fn sheet_chart_refs(&self, shape: ibis_doc_model::ShapeId, chart: &ibis_doc_model::Chart) -> Option<Vec<pptx_chart_data::SeriesRefs>> {
        // The chart knows what it reads (and follows the cells as they change).
        if !chart.cell_refs.is_empty() {
            return Some(pptx_chart_data::refs_of(chart));
        }
        if let Some(d) = self.chart_data.lock().unwrap_or_else(|e| e.into_inner()).get(&shape).and_then(|edits| edits.iter().rev().find(|(c, _)| c.groups == chart.groups)).map(|(_, d)| d.series.clone()) {
            return Some(d);
        }
        let SourceMap::Workbook(map) = &self.map else { return None };
        let source = map.charts.iter().find(|s| s.shape == shape)?;
        Some(pptx_chart_data::chart_refs(&self.package.xml(&source.chart).ok()?))
    }

    /// Chart `shape` made (as `chart`) from `data` edited in the workbook
    /// editor: saving a model with that chart writes its series and caches
    /// from the data, and the workbook into the file.
    pub fn set_chart_data(&self, shape: ibis_doc_model::ShapeId, chart: ibis_doc_model::Chart, data: pptx_chart_data::ChartData) {
        self.chart_data.lock().unwrap_or_else(|e| e.into_inner()).entry(shape).or_default().push((chart, data));
    }

    /// A chartEx chart `shape` with its data edited in the workbook editor (`data`): the chart as
    /// it reads back from the file written with that data, under the model's id. The data is
    /// kept for the chart read back, not for the one before (an undo gives the old data back).
    pub fn chart_ex_with_data(&self, current: &Presentation, slide: ibis_doc_model::SlideId, shape: ibis_doc_model::ShapeId, data: pptx_chart_data::ChartData) -> Result<ibis_doc_model::Shape, Error> {
        let index = current.slide_index(slide).ok_or_else(|| Error::new("そのスライドがありません"))?;
        let at = current.slides[index].shape_index(shape).ok_or_else(|| Error::new("その図形がありません"))?;
        let ibis_doc_model::ShapeKind::Chart(old) = &current.slides[index].shapes[at].kind else { return Err(Error::new("グラフではありません")) };
        let lock = || self.chart_data.lock().unwrap_or_else(|e| e.into_inner());
        // The data as written once, for reading the chart back.
        lock().entry(shape).or_default().push(((**old).clone(), data.clone()));
        let read = self.as_read(current, slide, shape);
        if let Some(edits) = lock().get_mut(&shape) {
            edits.pop();
        }
        let read = read?;
        if let ibis_doc_model::ShapeKind::Chart(c) = &read.kind {
            self.set_chart_data(shape, (**c).clone(), data);
        }
        Ok(read)
    }

    /// Shape `shape` of slide `slide` of `current` as it reads back once
    /// saved (what a table's style gives a new row's cells, and the like),
    /// under the model's id; the shape as it is when it cannot be.
    pub fn as_read(&self, current: &Presentation, slide: ibis_doc_model::SlideId, shape: ibis_doc_model::ShapeId) -> Result<ibis_doc_model::Shape, Error> {
        let index = current.slide_index(slide).ok_or_else(|| Error::new("そのスライドがありません"))?;
        let at = current.slides[index].shape_index(shape).ok_or_else(|| Error::new("その図形がありません"))?;
        let data = self.save(current, SaveOptions::default())?;
        let reread = crate::read(&data)?;
        let again = reread.slides.get(index).filter(|s| s.shapes.len() == current.slides[index].shapes.len()).ok_or_else(|| Error::new("読み直せません"))?;
        let mut read = again.shapes[at].clone();
        read.id = shape;
        // A table's rows and columns keep where they came from in the
        // original (the saved file numbers them anew).
        if let (ibis_doc_model::ShapeKind::Table(r), ibis_doc_model::ShapeKind::Table(c)) = (&mut read.kind, &current.slides[index].shapes[at].kind)
            && r.rows.len() == c.rows.len()
        {
            r.column_sources = c.column_sources.clone();
            for (row, was) in r.rows.iter_mut().zip(&c.rows) {
                row.source = was.source;
            }
        }
        Ok(read)
    }
}

/// An error for a change the writer cannot save yet.
pub(crate) fn unsupported(what: impl std::fmt::Display) -> Error {
    Error::new(format!("{what}はまだ保存できません"))
}

/// Slide `slide` as slide `id`, its shapes' ids changed by `ids` wherever
/// they are named (placeholders, attached shapes, groups, animations,
/// comments).
fn renumbered(slide: &ibis_doc_model::Slide, id: ibis_doc_model::SlideId, ids: &std::collections::HashMap<ibis_doc_model::ShapeId, ibis_doc_model::ShapeId>) -> ibis_doc_model::Slide {
    use ibis_doc_model::{GroupMember, ShapeGroup};
    let map = |s: ibis_doc_model::ShapeId| ids.get(&s).copied().unwrap_or(s);
    fn group(g: &ShapeGroup, map: &dyn Fn(ibis_doc_model::ShapeId) -> ibis_doc_model::ShapeId) -> ShapeGroup {
        ShapeGroup {
            children: g
                .children
                .iter()
                .map(|c| match c {
                    GroupMember::Shape(s) => GroupMember::Shape(map(*s)),
                    GroupMember::Group(inner) => GroupMember::Group(Box::new(group(inner, map))),
                })
                .collect(),
            ..g.clone()
        }
    }
    let mut out = slide.clone();
    out.id = id;
    for shape in &mut out.shapes {
        shape.id = map(shape.id);
    }
    out.placeholders = slide.placeholders.iter().map(|(k, v)| (map(*k), v.clone())).collect();
    out.shape_names = slide.shape_names.iter().map(|(k, v)| (map(*k), v.clone())).collect();
    out.hidden_shapes = slide.hidden_shapes.iter().map(|k| map(*k)).collect();
    out.shape_links = slide.shape_links.iter().map(|(k, v)| (map(*k), v.clone())).collect();
    out.shape_link_tooltips = slide.shape_link_tooltips.iter().map(|(k, v)| (map(*k), v.clone())).collect();
    out.aspect_locked = slide.aspect_locked.iter().map(|k| map(*k)).collect();
    out.shape_alt_texts = slide.shape_alt_texts.iter().map(|(k, v)| (map(*k), v.clone())).collect();
    out.decorative_shapes = slide.decorative_shapes.iter().map(|k| map(*k)).collect();
    out.shape_placements = slide.shape_placements.iter().map(|(k, v)| (map(*k), *v)).collect();
    out.shape_prints = slide.shape_prints.iter().map(|(k, v)| (map(*k), *v)).collect();
    out.shape_formulas = slide.shape_formulas.iter().map(|(k, v)| (map(*k), v.clone())).collect();
    out.attached = slide.attached.iter().map(|(k, v)| (map(*k), v.iter().map(|s| map(*s)).collect())).collect();
    out.groups = slide.groups.iter().map(|g| group(g, &map)).collect();
    for a in &mut out.animations {
        a.shape = map(a.shape);
    }
    for c in &mut out.comments {
        c.shape = c.shape.map(map);
    }
    out
}

mod chart_format;
