// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Read-only observations of one embedding/frame record shape, not placement.
use ibis_formats::Error;
use std::collections::BTreeMap;

const MAX_ROWS: usize = 4096;
const MAX_INFO: usize = 4 * 1024 * 1024;
const MAX_LINKS: usize = 16384;

/// Raw values from a candidate Frame record. Units and page are unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameCandidate {
    pub row: usize,
    pub id: u16,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// A candidate link, including every ID match to expose ambiguity.
#[derive(Debug, PartialEq, Eq)]
pub struct FrameLink {
    pub embedding_index: u32,
    pub frame_ref: u32,
    pub width: u32,
    pub height: u32,
    /// Presence of the observed stream name, not validity or renderability.
    pub embedded_press_present: bool,
    pub matches: Vec<FrameCandidate>,
}

/// Inspect one observed record shape. No image, geometry or page is reconstructed.
pub fn inspect_frame_links(data: &[u8]) -> Result<Vec<FrameLink>, Error> {
    let document = super::source_document(data)?;
    let entries = ibis_ooxml::cfb::directory(&document).map_err(|e| Error::new(e.to_string()))?;
    let exists = |path: &str| entries.iter().any(|e| !e.is_storage && e.path == path);
    match (exists("EmbedItems/EmbeddingInfo"), exists("Frame")) {
        (false, false) => return Ok(Vec::new()),
        (true, true) => {}
        _ => return Err(bad()),
    }
    let info = ibis_ooxml::cfb::stream_path(&document, "EmbedItems/EmbeddingInfo")
        .map_err(|e| Error::new(e.to_string()))?;
    let frame =
        ibis_ooxml::cfb::stream_path(&document, "Frame").map_err(|e| Error::new(e.to_string()))?;
    if info.len() > MAX_INFO || frame.len() > MAX_INFO {
        return Err(bad());
    }
    let mut links = parse(&info, &frame)?;
    for link in &mut links {
        link.embedded_press_present = exists(&format!(
            "EmbedItems/Embedding {}/\u{3}EmbeddedPress",
            link.embedding_index
        ));
    }
    Ok(links)
}

fn bad() -> Error {
    Error::new("埋め込み枠の診断データが破損しているか未対応です")
}
fn le32(b: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        b.get(at..at.checked_add(4).ok_or_else(bad)?)
            .ok_or_else(bad)?
            .try_into()
            .map_err(|_| bad())?,
    ))
}
fn be16(b: &[u8], at: usize) -> Result<u16, Error> {
    Ok(u16::from_be_bytes(
        b.get(at..at.checked_add(2).ok_or_else(bad)?)
            .ok_or_else(bad)?
            .try_into()
            .map_err(|_| bad())?,
    ))
}
fn parse(info: &[u8], frame: &[u8]) -> Result<Vec<FrameLink>, Error> {
    let info_count = le32(info, 0)? as usize;
    let frame_count = be16(frame, 14)? as usize;
    if info_count > MAX_ROWS
        || frame_count > MAX_ROWS
        || info.len() < 16
        || frame.len() < 16 + frame_count * 60
    {
        return Err(bad());
    }
    let mut frames: BTreeMap<u32, Vec<FrameCandidate>> = BTreeMap::new();
    for row in 0..frame_count {
        let b = &frame[16 + row * 60..16 + (row + 1) * 60];
        let candidate = FrameCandidate {
            row,
            id: be16(b, 6)?,
            x: be16(b, 28)?,
            y: be16(b, 32)?,
            width: be16(b, 36)?,
            height: be16(b, 40)?,
        };
        frames
            .entry(u32::from(candidate.id))
            .or_default()
            .push(candidate);
    }
    let mut cursor = 16usize;
    let mut links = Vec::with_capacity(info_count);
    let mut matched_count = 0usize;
    for _ in 0..info_count {
        let class_len = le32(info, cursor.checked_add(42).ok_or_else(bad)?)? as usize;
        if class_len == 0 || class_len % 2 != 0 {
            return Err(bad());
        }
        let tail = cursor
            .checked_add(46)
            .and_then(|v| v.checked_add(class_len))
            .ok_or_else(bad)?;
        let next = tail.checked_add(80).ok_or_else(bad)?;
        if next > info.len() {
            return Err(bad());
        }
        let embedding_index = le32(info, cursor + 8)?;
        let frame_ref = le32(info, tail)?;
        let candidates = frames
            .get(&frame_ref)
            .map(Vec::as_slice)
            .unwrap_or_default();
        matched_count = matched_count
            .checked_add(candidates.len())
            .ok_or_else(bad)?;
        if matched_count > MAX_LINKS {
            return Err(bad());
        }
        let matches = candidates.to_vec();
        links.push(FrameLink {
            embedding_index,
            frame_ref,
            width: le32(info, tail + 4)?,
            height: le32(info, tail + 8)?,
            embedded_press_present: false,
            matches,
        });
        cursor = next;
    }
    // Unknown trailing bytes might be a different version; do not imply a full parse.
    if cursor != info.len() {
        return Err(bad());
    }
    Ok(links)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_ooxml::cfb::{self, Item};

    fn info(entries: &[(u32, u32, u32, u32)]) -> Vec<u8> {
        let mut data = vec![0u8; 16];
        data[..4].copy_from_slice(&(entries.len() as u32).to_le_bytes());
        for &(index, reference, width, height) in entries {
            let mut row = vec![0u8; 46];
            row[8..12].copy_from_slice(&index.to_le_bytes());
            row[42..46].copy_from_slice(&4u32.to_le_bytes());
            data.extend(row);
            data.extend([b'A', 0, 0, 0]);
            let mut tail = [0u8; 80];
            tail[..4].copy_from_slice(&reference.to_le_bytes());
            tail[4..8].copy_from_slice(&width.to_le_bytes());
            tail[8..12].copy_from_slice(&height.to_le_bytes());
            data.extend(tail);
        }
        data
    }
    fn frame(entries: &[(u16, u16, u16)]) -> Vec<u8> {
        let mut data = vec![0u8; 16];
        data[14..16].copy_from_slice(&(entries.len() as u16).to_be_bytes());
        for &(id, width, height) in entries {
            let mut row = [0u8; 60];
            row[6..8].copy_from_slice(&id.to_be_bytes());
            row[36..38].copy_from_slice(&width.to_be_bytes());
            row[40..42].copy_from_slice(&height.to_be_bytes());
            data.extend(row);
        }
        data
    }
    #[test]
    fn links_by_id_including_zero_and_exposes_ambiguity_and_mismatch() {
        let links = parse(
            &info(&[(7, 0, 100, 200), (3, 5, 4, 6), (4, 8, 1, 2)]),
            &frame(&[(5, 4, 5), (0, 100, 200), (5, 4, 6)]),
        )
        .unwrap();
        assert_eq!(links[0].matches.len(), 1);
        assert_eq!(links[0].matches[0].row, 1);
        assert_eq!(links[0].matches[0].width, 100);
        assert_eq!(links[1].matches.len(), 2);
        assert_ne!(u32::from(links[1].matches[0].height), links[1].height);
        assert!(links[2].matches.is_empty());
    }
    #[test]
    fn rejects_truncated_excessive_and_unknown_data_without_partial_results() {
        let valid_info = info(&[(1, 0, 1, 2)]);
        let valid_frame = frame(&[(0, 1, 2)]);
        for n in 0..valid_info.len() {
            assert!(parse(&valid_info[..n], &valid_frame).is_err());
        }
        for n in 0..valid_frame.len() {
            assert!(parse(&valid_info, &valid_frame[..n]).is_err());
        }
        let mut tail = valid_info.clone();
        tail.push(0);
        assert!(parse(&tail, &valid_frame).is_err());
        let mut count = valid_info.clone();
        count[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse(&count, &valid_frame).is_err());
    }
    #[test]
    fn cfb_and_compressed_document_report_stream_presence() {
        let inner = cfb::write(&[
            Item::stream("DocumentText", crate::decode::fixture(&[0x7532])),
            Item::storage("EmbedItems"),
            Item::stream("EmbedItems/EmbeddingInfo", info(&[(7, 0, 100, 200)])),
            Item::storage("EmbedItems/Embedding 7"),
            Item::stream(
                "EmbedItems/Embedding 7/\u{3}EmbeddedPress",
                b"not necessarily a valid WMF".to_vec(),
            ),
            Item::stream("Frame", frame(&[(0, 100, 200)])),
        ]);
        let links = inspect_frame_links(&inner).unwrap();
        assert_eq!(links.len(), 1);
        assert!(links[0].embedded_press_present);
        let compressed = cfb::write(&[Item::stream(
            "JSCompDocument",
            crate::compressed::fixture(&inner),
        )]);
        assert_eq!(inspect_frame_links(&compressed).unwrap(), links);
    }

    #[test]
    fn missing_streams_and_excessive_ambiguous_links_are_not_silently_accepted() {
        let body = crate::decode::fixture(&[0x7532]);
        let no_metadata = cfb::write(&[Item::stream("DocumentText", body.clone())]);
        assert!(inspect_frame_links(&no_metadata).unwrap().is_empty());
        let one_stream = cfb::write(&[
            Item::stream("DocumentText", body),
            Item::stream("Frame", frame(&[(0, 1, 2)])),
        ]);
        assert!(inspect_frame_links(&one_stream).is_err());
        let many_infos = vec![(1, 0, 1, 2); 129];
        let many_frames = vec![(0, 1, 2); 128];
        assert!(parse(&info(&many_infos), &frame(&many_frames)).is_err());
    }
}
