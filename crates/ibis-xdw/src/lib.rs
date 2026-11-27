// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Read-only DocuWorks page adapter for the fixed-layout viewer.

use std::sync::Arc;

use ibis_pdf::{Error, ObjectId, PageInfo, PageScene, PdfRect, Result};
use ibis_xdw_read::Document;

const POINTS_PER_UNIT: f32 = 72.0 / 2540.0;

pub struct XdwContainer {
    document: Document<Arc<[u8]>>,
    pages: Vec<PageInfo>,
}

/// Recognize the common DocuWorks header, including generations 7 and 10.
pub fn is_xdw(bytes: &[u8]) -> bool {
    bytes.get(..5).is_some_and(|header| {
        header[..4] == [0x60, 0x0e, 0x82, 0x01] && matches!(header[4], 7 | 10)
    }) && bytes.get(10..16) == Some(&[0x83, 0x04, 0x01, 0x0d, 0x0a, 0x01])
}

impl XdwContainer {
    pub fn from_bytes(bytes: Arc<[u8]>) -> Result<Self> {
        let document = Document::open(bytes).map_err(map_error)?;
        let pages = document
            .pages()
            .map_err(map_error)?
            .into_iter()
            .enumerate()
            .map(|(index, page)| {
                let bounds = PdfRect {
                    x0: 0.0,
                    y0: 0.0,
                    x1: page.width as f32 * POINTS_PER_UNIT,
                    y1: page.height as f32 * POINTS_PER_UNIT,
                };
                PageInfo {
                    index,
                    object_id: ObjectId::new(index as u32 + 1, 0),
                    media_box: bounds,
                    crop_box: bounds,
                    rotate: 0,
                }
            })
            .collect();
        Ok(Self { document, pages })
    }

    pub fn page_infos(&self) -> Vec<PageInfo> {
        self.pages.clone()
    }

    pub fn interpret_page_scene(&self, index: usize) -> Result<PageScene> {
        let pages = self.document.pages().map_err(map_error)?;
        let page = pages
            .get(index)
            .ok_or_else(|| Error::new("DocuWorks page out of range"))?;
        self.document.page_scene(index, page).map_err(map_error)
    }
}

fn map_error(error: ibis_xdw_read::Error) -> Error {
    use ibis_xdw_read::Error::*;
    Error::new(match error {
        Invalid => "Invalid DocuWorks document",
        Protected => "This DocuWorks document is protected",
        Binder => "DocuWorks binders (.xbd) are unsupported",
        UnsupportedDrawing => "Unsupported DocuWorks drawing",
        Limit => "DocuWorks document exceeds the resource limits",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_scan_without_a_legacy_decoder() {
        let bytes = include_bytes!("../../ibis-xdw-read/tests/fixtures/rotated-emf-scan.xdw");
        let viewer = XdwContainer::from_bytes(Arc::from(bytes.as_slice())).unwrap();
        let pages = viewer.page_infos();
        assert_eq!(pages.len(), 2);
        let scene = viewer.interpret_page_scene(1).unwrap();
        assert_eq!(scene.info, pages[1]);
        assert!(matches!(
            scene.display_list.as_slice(),
            [ibis_pdf::DisplayCmd::DrawImage {
                width: 48,
                height: 32,
                ..
            }]
        ));
        assert!(viewer.interpret_page_scene(2).is_err());
    }
}
