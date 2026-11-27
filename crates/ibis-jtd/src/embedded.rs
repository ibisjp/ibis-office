// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Diagnostic extraction of one observed EmbeddedPress metafile envelope.
//!
//! No page association, geometry, paint order, or fidelity is inferred here.
use ibis_formats::Error;

const MAX_CANDIDATES: usize = 256;
const MAX_TOTAL_BYTES: usize = 32 * 1024 * 1024;
const WMF_OFFSET: usize = 48;

/// A structurally bounded WMF candidate. This is not a positioned image.
#[derive(Debug, PartialEq, Eq)]
pub struct EmbeddedMetafile {
    pub embedding_index: u32,
    pub wmf: Vec<u8>,
}

/// Observations are limited to the named, directly nested stream pattern.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EmbeddedMetafileInspection {
    pub metafiles: Vec<EmbeddedMetafile>,
    pub rejected: usize,
}

/// Inspect observed EmbeddedPress streams in a JTD/JTT/JTTC document.
///
/// Validating the WMF envelope does not establish that it can be rendered
/// correctly, or determine which page and position it belongs to.
pub fn inspect_embedded_metafiles(data: &[u8]) -> Result<EmbeddedMetafileInspection, Error> {
    let document = super::source_document(data)?;
    let entries = ibis_ooxml::cfb::directory(&document).map_err(|e| Error::new(e.to_string()))?;
    let mut result = EmbeddedMetafileInspection::default();
    let mut total = 0usize;
    for entry in entries {
        if entry.is_storage {
            continue;
        }
        let mut parts = entry.path.split('/');
        let (Some("EmbedItems"), Some(storage), Some("\u{3}EmbeddedPress"), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let Some(index) = storage
            .strip_prefix("Embedding ")
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        if result.metafiles.len() + result.rejected >= MAX_CANDIDATES {
            return Err(Error::new("埋め込み画像の数が多すぎます"));
        }
        let stream = ibis_ooxml::cfb::stream_path(&document, &entry.path)
            .map_err(|e| Error::new(e.to_string()))?;
        total = total
            .checked_add(stream.len())
            .ok_or_else(|| Error::new("埋め込み画像が大きすぎます"))?;
        if total > MAX_TOTAL_BYTES {
            return Err(Error::new("埋め込み画像が大きすぎます"));
        }
        if let Some(wmf) = bounded_wmf(&stream) {
            result.metafiles.push(EmbeddedMetafile {
                embedding_index: index,
                wmf: wmf.to_vec(),
            });
        } else {
            result.rejected += 1;
        }
    }
    Ok(result)
}

fn bounded_wmf(stream: &[u8]) -> Option<&[u8]> {
    if !stream.starts_with(b"METAFILE") {
        return None;
    }
    let data = stream.get(WMF_OFFSET..)?;
    let u16_at = |at| Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at| Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?));
    if !matches!(u16_at(0)?, 1 | 2) || u16_at(2)? != 9 {
        return None;
    }
    let length = usize::try_from(u32_at(6)?).ok()?.checked_mul(2)?;
    if length < 24 {
        return None;
    }
    let wmf = data.get(..length)?;
    let mut offset = 18usize;
    while offset.checked_add(6)? <= wmf.len() {
        let words = u32::from_le_bytes(wmf.get(offset..offset + 4)?.try_into().ok()?);
        let size = usize::try_from(words).ok()?.checked_mul(2)?;
        if size < 6 {
            return None;
        }
        let end = offset.checked_add(size)?;
        if end > wmf.len() {
            return None;
        }
        let function = u16::from_le_bytes(wmf.get(offset + 4..offset + 6)?.try_into().ok()?);
        if function == 0 {
            return (size == 6 && end == wmf.len()).then_some(wmf);
        }
        offset = end;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope() -> Vec<u8> {
        let mut data = vec![0; WMF_OFFSET];
        data[..8].copy_from_slice(b"METAFILE");
        data.extend_from_slice(&[1, 0, 9, 0, 0, 3, 12, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0]);
        data.extend_from_slice(&[3, 0, 0, 0, 0, 0]);
        data
    }

    #[test]
    fn validates_length_and_end_record() {
        let original = envelope();
        assert_eq!(bounded_wmf(&original).unwrap().len(), 24);
        assert!(bounded_wmf(&original[..original.len() - 1]).is_none());
        let mut bad = original.clone();
        bad[WMF_OFFSET + 6..WMF_OFFSET + 10].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(bounded_wmf(&bad).is_none());
        let mut bad = original.clone();
        bad[WMF_OFFSET + 18] = 2;
        assert!(bounded_wmf(&bad).is_none());
        let mut bad = original;
        bad[WMF_OFFSET + 22] = 1;
        assert!(bounded_wmf(&bad).is_none());
    }

    #[test]
    fn inspects_only_direct_embedded_press_streams() {
        use ibis_ooxml::cfb::Item;
        let data = ibis_ooxml::cfb::write(&[
            Item::stream("DocumentText", crate::decode::fixture(&[0x7532])),
            Item::storage("EmbedItems"),
            Item::storage("EmbedItems/Embedding 7"),
            Item::stream("EmbedItems/Embedding 7/\u{3}EmbeddedPress", envelope()),
            Item::storage("EmbedItems/Embedding 8"),
            Item::stream("EmbedItems/Embedding 8/\u{3}EmbeddedPress", b"bad".to_vec()),
            Item::storage("EmbedItems/Nested"),
            Item::storage("EmbedItems/Nested/Embedding 9"),
            Item::stream(
                "EmbedItems/Nested/Embedding 9/\u{3}EmbeddedPress",
                envelope(),
            ),
        ]);
        let result = inspect_embedded_metafiles(&data).unwrap();
        assert_eq!(result.rejected, 1);
        assert_eq!(result.metafiles.len(), 1);
        assert_eq!(result.metafiles[0].embedding_index, 7);
        assert_eq!(result.metafiles[0].wmf, envelope()[WMF_OFFSET..]);
        let compressed = ibis_ooxml::cfb::write(&[Item::stream(
            "JSCompDocument",
            crate::compressed::fixture(&data),
        )]);
        assert_eq!(inspect_embedded_metafiles(&compressed).unwrap(), result);
    }

    #[test]
    fn preview_falls_back_only_for_a_decoded_empty_body() {
        use ibis_ooxml::cfb::Item;
        let document = |units: &[u16]| ibis_ooxml::cfb::write(&[
            Item::stream("DocumentText", crate::decode::fixture(units)),
            Item::storage("EmbedItems"),
            Item::storage("EmbedItems/Embedding 7"),
            Item::stream("EmbedItems/Embedding 7/\u{3}EmbeddedPress", envelope()),
        ]);
        let empty = document(&[10]);
        assert!(crate::read(&empty).is_err());
        assert!(matches!(crate::read_preview(&empty), Ok(crate::Preview::UnplacedImages(_))));
        let compressed = ibis_ooxml::cfb::write(&[Item::stream("JSCompDocument", crate::compressed::fixture(&empty))]);
        assert!(matches!(crate::read_preview(&compressed), Ok(crate::Preview::UnplacedImages(_))));
        assert!(matches!(crate::read_preview(&document(&[])), Ok(crate::Preview::UnplacedImages(_))));
        let blank = ibis_ooxml::cfb::write(&[Item::stream("DocumentText", crate::decode::fixture(&[]))]);
        assert!(matches!(crate::read_preview(&blank), Ok(crate::Preview::Document(_))));
        assert!(matches!(crate::read_preview(&document(&[0x7532])), Ok(crate::Preview::Document(_))));
        let invalid = ibis_ooxml::cfb::write(&[
            Item::stream("DocumentText", b"not a document".to_vec()),
            Item::storage("EmbedItems"),
            Item::storage("EmbedItems/Embedding 7"),
            Item::stream("EmbedItems/Embedding 7/\u{3}EmbeddedPress", envelope()),
        ]);
        assert!(crate::read_preview(&invalid).is_err());
    }

    #[test]
    #[ignore = "requires a local document and independently extracted WMF files"]
    fn local_metafiles_match_independent_extraction() {
        let document = std::fs::read(std::env::var_os("IBIS_JTD_SAMPLE").unwrap()).unwrap();
        let reference =
            std::path::PathBuf::from(std::env::var_os("IBIS_WMF_REFERENCE_DIR").unwrap());
        let result = inspect_embedded_metafiles(&document).unwrap();
        assert_eq!(result.rejected, 0);
        assert_eq!(
            result.metafiles.len(),
            std::fs::read_dir(&reference).unwrap().count()
        );
        for candidate in result.metafiles {
            let expected =
                std::fs::read(reference.join(format!("{}.wmf", candidate.embedding_index)))
                    .unwrap();
            assert!(
                candidate.wmf == expected,
                "extracted candidate bytes differ"
            );
        }
    }
}
