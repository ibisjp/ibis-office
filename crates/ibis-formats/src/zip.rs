// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A minimal ZIP reader: the central directory, stored and deflated entries.
//!
//! OOXML packages need nothing more (no encryption, no multi-disk archives).
//! ZIP64 is not supported; packages over 4 GiB are rejected.

use std::collections::HashMap;

use crate::Error;

const END_OF_CENTRAL_DIRECTORY: u32 = 0x0605_4b50;
const CENTRAL_DIRECTORY_HEADER: u32 = 0x0201_4b50;
const LOCAL_FILE_HEADER: u32 = 0x0403_4b50;

#[derive(Clone, Copy, Debug)]
struct Entry {
    method: u16,
    compressed_size: usize,
    size: usize,
    local_header: usize,
}

pub struct Archive<'a> {
    data: &'a [u8],
    entries: HashMap<String, Entry>,
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

impl<'a> Archive<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self, Error> {
        let bad = || Error::new("ZIP 形式ではないか、壊れています");
        // The end record is 22 bytes plus a comment of up to 64 KiB.
        let search_from = data.len().saturating_sub(22 + 0xffff);
        let end = (search_from..data.len().saturating_sub(21))
            .rev()
            .find(|&i| u32_at(data, i) == Some(END_OF_CENTRAL_DIRECTORY))
            .ok_or_else(bad)?;
        let count = u16_at(data, end + 10).ok_or_else(bad)? as usize;
        let mut at = u32_at(data, end + 16).ok_or_else(bad)? as usize;
        if at == 0xffff_ffff {
            return Err(Error::new("ZIP64 形式には対応していません"));
        }

        let mut entries = HashMap::with_capacity(count);
        for _ in 0..count {
            if u32_at(data, at) != Some(CENTRAL_DIRECTORY_HEADER) {
                return Err(bad());
            }
            let field16 = |offset| u16_at(data, at + offset).ok_or_else(bad);
            let field32 = |offset| u32_at(data, at + offset).ok_or_else(bad);
            let method = field16(10)?;
            let compressed_size = field32(20)? as usize;
            let size = field32(24)? as usize;
            let name_len = field16(28)? as usize;
            let extra_len = field16(30)? as usize;
            let comment_len = field16(32)? as usize;
            let local_header = field32(42)? as usize;
            let name = data.get(at + 46..at + 46 + name_len).ok_or_else(bad)?;
            let name = String::from_utf8_lossy(name).into_owned();
            entries.insert(name, Entry { method, compressed_size, size, local_header });
            at += 46 + name_len + extra_len + comment_len;
        }
        Ok(Self { data, entries })
    }

    /// The entries' names (in no particular order).
    pub fn names(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// The uncompressed contents of `name`, or `None` if there is no such entry.
    pub fn read(&self, name: &str) -> Result<Option<Vec<u8>>, Error> {
        let Some(entry) = self.entries.get(name) else { return Ok(None) };
        let bad = || Error::new(format!("{name} を展開できません"));
        let at = entry.local_header;
        if u32_at(self.data, at) != Some(LOCAL_FILE_HEADER) {
            return Err(bad());
        }
        // The local header repeats name and extra field with its own lengths.
        let name_len = u16_at(self.data, at + 26).ok_or_else(bad)? as usize;
        let extra_len = u16_at(self.data, at + 28).ok_or_else(bad)? as usize;
        let start = at + 30 + name_len + extra_len;
        let raw = self.data.get(start..start + entry.compressed_size).ok_or_else(bad)?;
        let bytes = match entry.method {
            0 => raw.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(raw, entry.size.max(1))
                .map_err(|_| bad())?,
            _ => return Err(Error::new(format!("{name} の圧縮方式には対応していません"))),
        };
        Ok(Some(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-entry archive, stored without compression.
    fn stored_zip(name: &str, body: &[u8]) -> Vec<u8> {
        let mut z = Vec::new();
        let n = name.len() as u16;
        let len = body.len() as u32;
        z.extend(LOCAL_FILE_HEADER.to_le_bytes());
        z.extend([20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // version..crc
        z.extend(len.to_le_bytes());
        z.extend(len.to_le_bytes());
        z.extend(n.to_le_bytes());
        z.extend(0u16.to_le_bytes());
        z.extend(name.as_bytes());
        z.extend(body);
        let cd = z.len() as u32;
        z.extend(CENTRAL_DIRECTORY_HEADER.to_le_bytes());
        z.extend([20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // versions..crc
        z.extend(len.to_le_bytes());
        z.extend(len.to_le_bytes());
        z.extend(n.to_le_bytes());
        z.extend([0; 12]); // extra, comment, disk, attributes
        z.extend(0u32.to_le_bytes()); // local header offset
        z.extend(name.as_bytes());
        let cd_len = z.len() as u32 - cd;
        z.extend(END_OF_CENTRAL_DIRECTORY.to_le_bytes());
        z.extend([0, 0, 0, 0, 1, 0, 1, 0]);
        z.extend(cd_len.to_le_bytes());
        z.extend(cd.to_le_bytes());
        z.extend(0u16.to_le_bytes());
        z
    }

    #[test]
    fn reads_stored_entry() {
        let zip = stored_zip("a/b.xml", b"<x/>");
        let archive = Archive::new(&zip).unwrap();
        assert!(archive.contains("a/b.xml"));
        assert_eq!(archive.read("a/b.xml").unwrap().as_deref(), Some(&b"<x/>"[..]));
        assert_eq!(archive.read("missing").unwrap(), None);
    }

    #[test]
    fn rejects_non_zip() {
        assert!(Archive::new(b"not a zip file at all, just some text").is_err());
    }
}
