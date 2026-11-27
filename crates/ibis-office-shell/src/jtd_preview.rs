// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Explicitly labelled auxiliary previews for JTD documents.
use ibis_doc_model::{
    Block, EmuSize, FlowParagraph, FlowRun, FlowRunKind, FlowRunStyle, MediaFormat, PictureLook,
    Presentation,
};

pub fn read(data: &[u8]) -> Result<(Presentation, bool), String> {
    match ibis_jtd::read_preview(data).map_err(|e| e.to_string())? {
        ibis_jtd::Preview::Document(document) => Ok((document, false)),
        ibis_jtd::Preview::UnplacedImages(images) => {
            gallery(images, "view.jtd-gallery-unavailable").map(|document| (document, true))
        }
    }
}

/// Inspect unplaced images separately even when the document has body text.
pub fn images(data: &[u8]) -> Result<Presentation, String> {
    let images = ibis_jtd::inspect_embedded_metafiles(data).map_err(|e| e.to_string())?;
    gallery(images, "view.jtd-no-embedded-images")
}

/// An embedded snapshot, which may be stale and is not a reconstructed page.
pub fn thumbnail(data: &[u8]) -> Result<Presentation, String> {
    let bmp = ibis_jtd::decode_thumbnail_bmp(data)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| crate::i18n::text("view.jtd-thumbnail-unavailable").to_string())?;
    let (width, height) = crate::shape_items::validated_picture_size(MediaFormat::Bmp, &bmp)
        .ok_or_else(|| crate::i18n::text("view.jtd-thumbnail-unavailable").to_string())?;
    let bytes = ibis_formats::blank_file(ibis_formats::Kind::Document).map_err(|e| e.to_string())?;
    let mut document = ibis_formats::read(&bytes).map_err(|e| e.to_string())?;
    let media = document.add_media(MediaFormat::Bmp, bmp);
    let longest = u64::from(width.max(height).max(1));
    let scale = |value: u32| ((u64::from(value) * (5 * 914_400)) / longest).max(1) as i64;
    let body = vec![
        paragraph(1, FlowRunKind::Text(crate::i18n::text("view.jtd-thumbnail-heading").to_string())),
        paragraph(2, FlowRunKind::Picture { media, size: EmuSize::new(scale(width), scale(height)), look: PictureLook::default() }),
    ];
    let flow = document.flow.as_mut().ok_or("cannot create thumbnail preview")?;
    std::sync::Arc::make_mut(flow).body = body;
    Ok(document)
}

fn gallery(mut images: ibis_jtd::EmbeddedMetafileInspection, empty_key: &str) -> Result<Presentation, String> {
    let bytes = ibis_formats::blank_file(ibis_formats::Kind::Document).map_err(|e| e.to_string())?;
    let mut document = ibis_formats::read(&bytes).map_err(|e| e.to_string())?;
    images.metafiles.sort_by_key(|entry| entry.embedding_index);
    let candidate_count = images.metafiles.len() + images.rejected;
    let mut blocks = vec![paragraph(
        1,
        FlowRunKind::Text(crate::i18n::text("view.jtd-gallery-heading").to_string()),
    )];
    let mut inspected_bytes = 0usize;
    let mut displayed = 0usize;
    for image in images.metafiles.into_iter().take(32) {
        inspected_bytes = inspected_bytes.saturating_add(image.wmf.len());
        if inspected_bytes > 8 * 1024 * 1024 {
            break;
        }
        let Some((width, height)) =
            crate::shape_items::validated_picture_size(MediaFormat::Wmf, &image.wmf)
        else {
            continue;
        };
        // An inspection scale, not a source-document dimension. Preserve aspect ratio.
        let size = inspection_size(width, height);
        let media = document.add_media(MediaFormat::Wmf, image.wmf);
        let id = blocks.len() as u32 + 1;
        blocks.push(paragraph(id, FlowRunKind::Text(crate::i18n::format(
            "view.jtd-gallery-item", &[("index", &image.embedding_index)],
        ))));
        let id = blocks.len() as u32 + 1;
        blocks.push(paragraph(
            id,
            FlowRunKind::Picture {
                media,
                size,
                look: PictureLook::default(),
            },
        ));
        displayed += 1;
    }
    if blocks.len() == 1 {
        return Err(crate::i18n::text(empty_key).to_string());
    }
    let summary = crate::i18n::format("view.jtd-gallery-count", &[
        ("displayed", &displayed), ("candidates", &candidate_count),
    ]);
    blocks.insert(1, paragraph(blocks.len() as u32 + 1, FlowRunKind::Text(summary)));
    let flow = document.flow.as_mut().ok_or("cannot create image list")?;
    std::sync::Arc::make_mut(flow).body = blocks;
    Ok(document)
}

fn paragraph(id: u32, kind: FlowRunKind) -> Block {
    Block::Paragraph(FlowParagraph {
        id,
        runs: vec![FlowRun::new(kind, FlowRunStyle::default())],
        ..FlowParagraph::default()
    })
}

pub fn inspection_size(width: u32, height: u32) -> EmuSize {
    let (w, h) = (u64::from(width) * 9_525, u64::from(height) * 9_525);
    let limit = 5 * 914_400u64;
    let longest = w.max(h).max(1);
    let fit = |n: u64| {
        if longest > limit {
            ((u128::from(n) * u128::from(limit)) / u128::from(longest)) as i64
        } else {
            n as i64
        }
    };
    EmuSize::new(fit(w).max(1), fit(h).max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspection_dimensions_are_bounded_even_for_hostile_sizes() {
        let size = inspection_size(u32::MAX, 1);
        assert_eq!(size.width, 5 * 914_400);
        assert_eq!(size.height, 1);
        let size = inspection_size(100, 200);
        assert_eq!(size.width * 2, size.height);
    }
}
