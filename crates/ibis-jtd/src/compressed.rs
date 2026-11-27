// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/80#issuecomment-6868471583)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Bounded extraction of the observed JustCompressedDocument LH5 wrapper.
use ibis_formats::Error;
use delharc::Read;

const MAGIC: &[u8] = b"\x26\0JustCompressedDocument";

pub(super) fn is_wrapper(data: &[u8]) -> bool {
    data.starts_with(MAGIC)
}

pub(super) fn decompress(data: &[u8], max_output: usize) -> Result<Vec<u8>, Error> {
    let invalid = || Error::new("圧縮された一太郎文書が破損しているか未対応です");
    if !is_wrapper(data) { return Err(invalid()); }
    // Observed wrapper length is stored as a little-endian u16. Do not scan
    // arbitrary payload for an apparent archive signature.
    let offset = u16::from_le_bytes(data.get(..2).ok_or_else(invalid)?.try_into().map_err(|_| invalid())?) as usize;
    if offset < MAGIC.len() || data.get(offset + 2..offset + 7) != Some(b"-lh5-") || !matches!(data.get(offset + 20), Some(0 | 1)) { return Err(invalid()); }
    let mut reader = delharc::LhaDecodeReader::new(data.get(offset..).ok_or_else(invalid)?)
        .map_err(|_| invalid())?;
    let header = reader.header();
    if header.compression != *b"-lh5-" || header.original_size == 0 || header.original_size > max_output as u64
        || header.compressed_size > data.len() as u64 || !reader.is_decoder_supported() {
        return Err(invalid());
    }
    let mut output = vec![0u8; header.original_size as usize];
    reader.read_exact(&mut output).map_err(|_| invalid())?;
    reader.crc_check().map_err(|_| invalid())?;
    if !ibis_ooxml::cfb::is_cfb(&output) { return Err(invalid()); }
    Ok(output)
}

#[cfg(test)]
pub(super) fn fixture(inner: &[u8]) -> Vec<u8> {
    // Deliberately simple LH5: each byte is a single-symbol block. This
    // creates fully synthetic bytes without relying on a third-party encoder.
    let mut bytes = Vec::new();
    let mut current = 0u8;
    let mut used = 0u8;
    let mut bits = |value: u32, count: usize| {
        for shift in (0..count).rev() {
            current = (current << 1) | ((value >> shift) as u8 & 1);
            used += 1;
            if used == 8 { bytes.push(current); current = 0; used = 0; }
        }
    };
    for byte in inner {
        for (value, count) in [(1,16), (0,5), (0,5), (0,9), (*byte as u32,9), (0,4), (0,4)] {
            bits(value, count);
        }
    }
    drop(bits);
    if used != 0 { bytes.push(current << (8 - used)); }
    let mut crc = delharc::crc::Crc16::default();
    crc.digest(inner);
    let mut archive = vec![22, 0, b'-', b'l', b'h', b'5', b'-'];
    archive.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    archive.extend_from_slice(&(inner.len() as u32).to_le_bytes());
    archive.extend_from_slice(&0u32.to_le_bytes());
    archive.extend_from_slice(&[0x20, 0, 0]);
    archive.extend_from_slice(&crc.sum16().to_le_bytes());
    archive[1] = archive[2..24].iter().fold(0u8, |sum, b| sum.wrapping_add(*b));
    archive.extend_from_slice(&bytes);
    let mut wrapper = MAGIC.to_vec();
    wrapper.resize(38, 0);
    wrapper.extend(archive);
    wrapper
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_wrapper_checks_crc_truncation_and_magic() {
        let inner = ibis_ooxml::cfb::write(&[ibis_ooxml::cfb::Item::stream("A", b"test".to_vec())]);
        let wrapper = fixture(&inner);
        assert_eq!(decompress(&wrapper, inner.len()).unwrap(), inner);
        assert!(decompress(&wrapper, inner.len() - 1).is_err());
        assert!(decompress(&wrapper[..wrapper.len() - 1], inner.len()).is_err());
        let mut corrupt = wrapper.clone();
        corrupt[38 + 22] ^= 0x01; // data CRC, not the compressed stream
        corrupt[38 + 1] = corrupt[38 + 2..38 + 24].iter().fold(0u8, |s, b| s.wrapping_add(*b));
        assert!(decompress(&corrupt, inner.len()).is_err());
        let mut wrong = wrapper;
        wrong[2] ^= 1;
        assert!(decompress(&wrong, inner.len()).is_err());
    }

    #[test]
    #[ignore = "requires local authorized JTTC and independently extracted reference"]
    fn local_archive_matches_independent_extraction() {
        let original = std::fs::read(std::env::var_os("IBIS_JTTC_SAMPLE").unwrap()).unwrap();
        let reference = std::fs::read(std::env::var_os("IBIS_JTTC_INNER").unwrap()).unwrap();
        let wrapper = ibis_ooxml::cfb::stream_path(&original, "JSCompDocument").unwrap();
        assert!(decompress(&wrapper, crate::MAX_FILE).unwrap() == reference, "independent extracted bytes differ");
    }
}
