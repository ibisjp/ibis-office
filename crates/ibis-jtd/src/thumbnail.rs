// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Bounded inspection and decoding of one observed thumbnail envelope.
//! Freshness, page association and correspondence to document content are unknown.
use ibis_formats::Error;

const MAX_THUMBNAIL: usize = 16 * 1024 * 1024;

/// Candidate metadata, not a decoded bitmap or a verified page preview.
#[derive(Debug, PartialEq, Eq)]
pub struct ThumbnailCandidate {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u16,
    pub declared_dib_bytes: u32,
    pub stored_dib_bytes: u32,
    pub stored_payload_bytes: usize,
}

/// Read the observed Thumbnail envelope. Other versions are rejected.
pub fn inspect_thumbnail(data: &[u8]) -> Result<Option<ThumbnailCandidate>, Error> {
    thumbnail_stream(data)?
        .map(|bytes| parse(&bytes))
        .transpose()
}

/// Decode the observed 24-bit thumbnail into a BMP file, without inferring placement.
/// The image is an embedded snapshot; it may be stale or omit document content.
pub fn decode_thumbnail_bmp(data: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    thumbnail_stream(data)?
        .map(|bytes| decode(&bytes))
        .transpose()
}

fn thumbnail_stream(data: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    let document = super::source_document(data)?;
    let entries = ibis_ooxml::cfb::directory(&document).map_err(|e| Error::new(e.to_string()))?;
    if !entries
        .iter()
        .any(|e| !e.is_storage && e.path == "Thumbnail")
    {
        return Ok(None);
    }
    let bytes = ibis_ooxml::cfb::stream_path(&document, "Thumbnail")
        .map_err(|e| Error::new(e.to_string()))?;
    Ok(Some(bytes))
}
fn bad() -> Error {
    Error::new("サムネイルの診断データが破損しているか未対応です")
}
fn u32_at(b: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4)
            .ok_or_else(bad)?
            .try_into()
            .map_err(|_| bad())?,
    ))
}
fn u16_at(b: &[u8], at: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(
        b.get(at..at + 2)
            .ok_or_else(bad)?
            .try_into()
            .map_err(|_| bad())?,
    ))
}
fn parse(bytes: &[u8]) -> Result<ThumbnailCandidate, Error> {
    if bytes.len() <= 56 || bytes.len() > MAX_THUMBNAIL || bytes[8..16] != [0; 8] {
        return Err(bad());
    }
    let declared = u32_at(bytes, 0)?;
    let stored = u32_at(bytes, 4)?;
    let width = u32_at(bytes, 20)?;
    let height = u32_at(bytes, 24)?;
    if u32_at(bytes, 16)? != 40
        || width == 0
        || height == 0
        || width > 65535
        || height > 65535
        || u16_at(bytes, 28)? != 1
        || u16_at(bytes, 30)? != 24
        || u32_at(bytes, 32)? != 0
        || usize::try_from(stored).ok() != Some(bytes.len() - 16)
    {
        return Err(bad());
    }
    let stride = width
        .checked_mul(3)
        .and_then(|v| v.checked_add(3))
        .map(|v| v & !3)
        .ok_or_else(bad)?;
    let expected = stride
        .checked_mul(height)
        .and_then(|v| v.checked_add(40))
        .ok_or_else(bad)?;
    if expected != declared || expected as usize > MAX_THUMBNAIL {
        return Err(bad());
    }
    Ok(ThumbnailCandidate {
        width,
        height,
        bit_depth: 24,
        declared_dib_bytes: declared,
        stored_dib_bytes: stored,
        stored_payload_bytes: bytes.len() - 56,
    })
}

fn decode(bytes: &[u8]) -> Result<Vec<u8>, Error> {
    let candidate = parse(bytes)?;
    // A 24-bit DIB with a palette requires additional handling.
    if u32_at(bytes, 48)? != 0 {
        return Err(bad());
    }
    let pixel_bytes = usize::try_from(candidate.declared_dib_bytes - 40).map_err(|_| bad())?;
    let bmp_size = 54usize.checked_add(pixel_bytes).ok_or_else(bad)?;
    let mut bmp = Vec::with_capacity(bmp_size);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&u32::try_from(bmp_size).map_err(|_| bad())?.to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&bytes[16..56]);
    // Emit an ordinary BI_RGB bitmap with an explicit uncompressed image size.
    bmp[34..38].copy_from_slice(&u32::try_from(pixel_bytes).map_err(|_| bad())?.to_le_bytes());
    let mut pos = 56usize;
    while pos < bytes.len() {
        let value = bytes[pos];
        pos += 1;
        let count = if bytes.get(pos) == Some(&value) {
            pos += 1;
            let count = usize::from(*bytes.get(pos).ok_or_else(bad)?);
            pos += 1;
            if count < 2 {
                return Err(bad());
            }
            count
        } else {
            1
        };
        let end = bmp.len().checked_add(count).ok_or_else(bad)?;
        if end > bmp_size {
            return Err(bad());
        }
        bmp.resize(end, value);
    }
    if bmp.len() != bmp_size {
        return Err(bad());
    }
    Ok(bmp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_ooxml::cfb::{self, Item};
    fn thumbnail() -> Vec<u8> {
        let mut b = vec![0; 57];
        b[0..4].copy_from_slice(&64u32.to_le_bytes()); // 3 pixels, padded to 12 bytes, 2 rows
        b[4..8].copy_from_slice(&41u32.to_le_bytes());
        b[16..20].copy_from_slice(&40u32.to_le_bytes());
        b[20..24].copy_from_slice(&3u32.to_le_bytes());
        b[24..28].copy_from_slice(&2u32.to_le_bytes());
        b[28..30].copy_from_slice(&1u16.to_le_bytes());
        b[30..32].copy_from_slice(&24u16.to_le_bytes());
        b
    }
    #[test]
    fn inspects_only_bounded_observed_envelope() {
        let b = thumbnail();
        let result = parse(&b).unwrap();
        assert_eq!(
            (
                result.width,
                result.height,
                result.declared_dib_bytes,
                result.stored_payload_bytes
            ),
            (3, 2, 64, 1)
        );
        for n in 0..57 {
            assert!(parse(&b[..n]).is_err());
        }
        for offset in [0, 4, 8, 16, 20, 24, 28, 30, 32] {
            let mut wrong = b.clone();
            wrong[offset] ^= 0x80;
            assert!(parse(&wrong).is_err(), "offset {offset}");
        }
        let mut huge = b;
        huge[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&huge).is_err());
    }
    #[test]
    fn inspects_uncompressed_and_wrapped_documents_without_writing() {
        let body = crate::decode::fixture(&[0x7532]);
        let none = cfb::write(&[Item::stream("DocumentText", body.clone())]);
        assert_eq!(inspect_thumbnail(&none).unwrap(), None);
        let inner = cfb::write(&[
            Item::stream("DocumentText", body),
            Item::stream("Thumbnail", thumbnail()),
        ]);
        let expected = inspect_thumbnail(&inner).unwrap();
        let outer = cfb::write(&[Item::stream(
            "JSCompDocument",
            crate::compressed::fixture(&inner),
        )]);
        assert_eq!(inspect_thumbnail(&outer).unwrap(), expected);
    }

    fn encoded_bitmap() -> Vec<u8> {
        let mut b = thumbnail();
        b.truncate(56);
        b[0..4].copy_from_slice(&56u32.to_le_bytes()); // 2 rows, each 8 bytes
        b[20..24].copy_from_slice(&2u32.to_le_bytes());
        // Two bottom-up BGR rows and two padding bytes per row.
        b.extend_from_slice(&[1, 2, 3, 4, 5, 6, 0, 0, 2, 7, 8, 9, 10, 11, 12, 0, 0, 2]);
        let stored = (b.len() - 16) as u32;
        b[4..8].copy_from_slice(&stored.to_le_bytes());
        b
    }

    #[test]
    fn decodes_bgr_rows_and_repeated_bytes_to_bmp() {
        let bmp = decode(&encoded_bitmap()).unwrap();
        assert_eq!(&bmp[..2], b"BM");
        assert_eq!(u32_at(&bmp, 2).unwrap(), 70);
        assert_eq!(u32_at(&bmp, 10).unwrap(), 54);
        assert_eq!(u32_at(&bmp, 34).unwrap(), 16);
        assert_eq!(
            &bmp[54..],
            &[1, 2, 3, 4, 5, 6, 0, 0, 7, 8, 9, 10, 11, 12, 0, 0]
        );
        let input = [7, 7, 255];
        let mut large = thumbnail();
        large.truncate(56);
        large[20..24].copy_from_slice(&85u32.to_le_bytes()); // stride 256
        large[24..28].copy_from_slice(&1u32.to_le_bytes());
        large[0..4].copy_from_slice(&296u32.to_le_bytes());
        large.extend(input);
        large.push(8);
        large[4..8].copy_from_slice(&44u32.to_le_bytes());
        let bmp = decode(&large).unwrap();
        assert_eq!(bmp.len(), 54 + 256);
        assert!(bmp[54..309].iter().all(|v| *v == 7));
        assert_eq!(bmp[309], 8);
    }

    #[test]
    fn rejects_truncated_runs_zero_counts_overflows_and_extra_data() {
        let adjust = |mut b: Vec<u8>| {
            let n = (b.len() - 16) as u32;
            b[4..8].copy_from_slice(&n.to_le_bytes());
            b
        };
        let data = encoded_bitmap();
        assert!(decode(&adjust(data[..data.len() - 1].to_vec())).is_err());
        let mut bad_count = data.clone();
        bad_count[64] = 0;
        assert!(decode(&bad_count).is_err());
        let mut one_count = data.clone();
        one_count[64] = 1;
        assert!(decode(&one_count).is_err());
        let mut overflow = data.clone();
        overflow[64] = 255;
        assert!(decode(&overflow).is_err());
        let mut extra = data.clone();
        extra.push(17);
        assert!(decode(&adjust(extra)).is_err());
        let mut palette = data;
        palette[48] = 1;
        assert!(decode(&palette).is_err());
    }

    #[test]
    fn decodes_raw_and_compressed_documents_and_handles_absence() {
        let body = crate::decode::fixture(&[0x7532]);
        let none = cfb::write(&[Item::stream("DocumentText", body.clone())]);
        assert_eq!(decode_thumbnail_bmp(&none).unwrap(), None);
        let inner = cfb::write(&[
            Item::stream("DocumentText", body),
            Item::stream("Thumbnail", encoded_bitmap()),
        ]);
        let expected = decode_thumbnail_bmp(&inner).unwrap();
        let outer = cfb::write(&[Item::stream(
            "JSCompDocument",
            crate::compressed::fixture(&inner),
        )]);
        assert_eq!(decode_thumbnail_bmp(&outer).unwrap(), expected);
    }
}
