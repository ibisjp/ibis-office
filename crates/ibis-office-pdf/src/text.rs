// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Plain text export for the fixed-layout viewer: the characters of each page as lines, nothing else.

use ibis_pdf::text_layout::{TextLayout, page_text};
use ibis_pdf::{Error, Result};

use crate::{PdfViewerState, ViewerSource};

/// The text of a PDF, XPS or OFD document: each page's lines (see [`page_text`]), each page ended by a form feed as
/// `pdftotext` ends it. No Markdown escape, no drawing; for a PDF nothing is drawn or decoded but the characters.
/// None means the input is not a fixed-layout document.
pub fn export(bytes: &[u8], layout: TextLayout) -> Result<Option<String>> {
    if !bytes.starts_with(b"%PDF-") && !ibis_ofd::is_ofd(bytes) && !ibis_xps::is_xps(bytes) {
        return Ok(None);
    }
    let mut text = String::new();
    if bytes.starts_with(b"%PDF-") {
        let mut container = ibis_pdf::PdfContainer::from_bytes(bytes.to_vec())?;
        ibis_pdf::interpret::visit_text_scenes(&mut container, |scene| {
            text.push_str(&page_text(&scene.text_index, layout));
            text.push('\u{c}');
            Ok(())
        })?;
        return Ok(Some(text));
    }
    let mut viewer = PdfViewerState::open(bytes.to_vec())?;
    if !matches!(viewer.source, ViewerSource::Ofd(_) | ViewerSource::Xps(_)) {
        return Ok(None);
    }
    for index in 0..viewer.pages.len() {
        let scene = viewer
            .page_scene(index)
            .map_err(|error| Error::new(format!("Page {}: {error}", index + 1)))?;
        text.push_str(&page_text(&scene.text_index, layout));
        text.push('\u{c}');
        viewer.scene_cache.remove(&index);
    }
    Ok(Some(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PDF of one Helvetica page per stream.
    fn pdf(pages: &[&[u8]]) -> Vec<u8> {
        use std::io::Write as _;
        let n = pages.len();
        let mut out = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        offsets.push(out.len());
        out.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
        offsets.push(out.len());
        let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
        let _ = write!(out, "2 0 obj\n<< /Type /Pages /Count {n} /Kids [{}] /MediaBox [0 0 595 842] >>\nendobj\n", kids.join(" "));
        offsets.push(out.len());
        out.extend_from_slice(b"3 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n");
        for (i, stream) in pages.iter().enumerate() {
            let (page, contents) = (4 + 2 * i, 5 + 2 * i);
            offsets.push(out.len());
            let _ = write!(out, "{page} 0 obj\n<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 3 0 R >> >> /Contents {contents} 0 R >>\nendobj\n");
            offsets.push(out.len());
            let _ = write!(out, "{contents} 0 obj\n<< /Length {} >>\nstream\n", stream.len());
            out.extend_from_slice(stream);
            out.extend_from_slice(b"\nendstream\nendobj\n");
        }
        let xref = out.len();
        let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1);
        for off in &offsets {
            let _ = writeln!(out, "{off:010} 00000 n ");
        }
        let _ = write!(out, "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF", offsets.len() + 1);
        out
    }

    #[test]
    fn each_page_is_its_lines_ended_by_a_form_feed_without_escapes() {
        let bytes = pdf(&[
            b"BT /F1 10 Tf 72 720 Td (1.1.1. [A-1] *x*) Tj ET",
            b"BT /F1 10 Tf 72 720 Td (second) Tj ET 0 0 1 rg 50 50 100 100 re f",
        ]);
        let text = export(&bytes, TextLayout::Reading).unwrap().unwrap();
        assert_eq!(text, "1.1.1. [A-1] *x*\n\u{c}second\n\u{c}");
    }

    #[test]
    fn the_physical_layout_places_cells_at_their_columns() {
        let bytes = pdf(&[b"BT /F1 10 Tf 72 720 Td (ID) Tj 100 0 Td (A) Tj ET BT /F1 10 Tf 72 708 Td (0190001) Tj 100 0 Td (B) Tj ET"]);
        let text = export(&bytes, TextLayout::Physical).unwrap().unwrap();
        let lines: Vec<&str> = text.trim_end_matches('\u{c}').lines().collect();
        assert_eq!(lines[0].find('A'), lines[1].find('B'));
    }

    #[test]
    fn other_documents_are_not_fixed_layout() {
        assert_eq!(export(b"PK\x03\x04", TextLayout::Reading).unwrap(), None);
    }
}
