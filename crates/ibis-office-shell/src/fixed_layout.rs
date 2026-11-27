// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Files that are looked at as pages (PDF, OFD, XPS, DocuWorks): the same for the desktop's reader (`read_file`) and the web's opening
//! (the file picker, the host's `load`), so a file opens the same way in both.

/// Whether the file is one of the page formats: by its extension, or by what it starts with.
pub fn is_fixed_layout(extension: Option<&str>, data: &[u8]) -> bool {
    extension.is_some_and(|ext| ["pdf", "ofd", "xps", "oxps", "xdw", "xbd"].iter().any(|x| ext.eq_ignore_ascii_case(x)))
        || data.starts_with(b"%PDF-")
        || ibis_ofd::is_ofd(data)
        || ibis_xps::is_xps(data)
        || ibis_xdw::is_xdw(data)
}

/// The sizes (points) of the pages of the file. A DocuWorks file (read-only) takes its pages from `ibis_xdw`, with no PDF behind it.
fn page_boxes(raw: &std::sync::Arc<[u8]>) -> Result<Vec<(f32, f32)>, String> {
    let sizes = |infos: Vec<ibis_pdf::PageInfo>| infos.into_iter().map(|p| (p.crop_box.width(), p.crop_box.height())).collect();
    Ok(if ibis_xdw::is_xdw(raw) {
        sizes(ibis_xdw::XdwContainer::from_bytes(raw.clone()).map_err(|e| e.to_string())?.page_infos())
    } else if ibis_ofd::is_ofd(raw) {
        sizes(ibis_ofd::OfdContainer::from_bytes(raw.clone()).map_err(|e| e.to_string())?.page_infos())
    } else if ibis_xps::is_xps(raw) {
        sizes(ibis_xps::XpsContainer::from_bytes(raw.clone()).map_err(|e| e.to_string())?.page_infos())
    } else {
        let mut container = ibis_pdf::PdfContainer::from_bytes(raw.clone()).map_err(|e| e.to_string())?;
        sizes(ibis_pdf::scene::collect_pages(&mut container).map_err(|e| e.to_string())?)
    })
}

/// The file as a model with one empty slide per page (the viewer reads the pages from `pdf_bytes`).
pub fn open_fixed_layout(raw: std::sync::Arc<[u8]>) -> Result<ibis_formats::Opened, String> {
    let page_boxes = page_boxes(&raw)?;
    let size = |w: f32, h: f32| ibis_doc_model::EmuSize::new((w as i64) * ibis_doc_model::EMU_PER_POINT, (h as i64) * ibis_doc_model::EMU_PER_POINT);
    let first_size = page_boxes.first().map(|&(w, h)| size(w, h)).unwrap_or_else(|| size(595.0, 842.0));
    let mut presentation = ibis_doc_model::Presentation::empty(first_size);
    for &(w, h) in &page_boxes {
        let mut slide = ibis_doc_model::Slide::new(presentation.new_slide_id());
        slide.size = Some(size(w, h));
        presentation.slides.push(slide);
    }
    presentation.pdf_bytes = Some(raw);
    let blank = ibis_formats::blank_file(ibis_formats::Kind::Document).map_err(|e| e.to_string())?;
    let source = ibis_formats::open(blank).map_err(|e| e.to_string())?.source;
    Ok(ibis_formats::Opened { presentation, source })
}

/// `ibis_odf::open` for any file, the page formats included (the web's opening and the host's `load`).
pub fn open_any(name: &str, data: Vec<u8>) -> Result<ibis_formats::Opened, String> {
    let extension = name.rsplit_once('.').map(|(_, e)| e);
    if is_fixed_layout(extension, &data) {
        return open_fixed_layout(data.into());
    }
    ibis_odf::open(data).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_docuworks_file_opens_as_pages_by_extension_or_content() {
        let bytes = include_bytes!("../../ibis-xdw-read/tests/fixtures/rotated-emf-scan.xdw").to_vec();
        assert!(is_fixed_layout(Some("XDW"), &[]));
        assert!(is_fixed_layout(None, &bytes));
        let opened = open_any("scan.xdw", bytes.clone()).unwrap();
        assert_eq!(opened.presentation.slides.len(), 2);
        assert!(opened.presentation.pdf_bytes.as_deref().is_some_and(ibis_xdw::is_xdw));
        // The same bytes under another name are found by what they start with.
        assert_eq!(open_any("scan.bin", bytes).unwrap().presentation.slides.len(), 2);
    }

    #[test]
    fn a_docx_is_not_a_page_file() {
        assert!(!is_fixed_layout(Some("docx"), b"PK\x03\x04"));
    }
}
