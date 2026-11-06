// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Embedded OpenType (EOT) fonts, as PowerPoint embeds them (`.fntdata`).
//!
//! [`decode`] reads the EOT header and turns its font data back into an
//! OpenType file (TrueType or CFF outlines), undoing the XOR obfuscation and MicroType Express
//! compression. The result is for drawing the document it came with: keep
//! it in memory and never install it (see [`Embedding`]).

mod ctf;
mod lzcomp;

/// Why a font could not be decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error(&'static str);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Error {}

/// What the font's licence allows for embedded use (OS/2 `fsType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Embedding {
    /// May be installed; still only loaded temporarily here.
    Installable,
    /// May be used to view and edit the document, then discarded.
    Editable,
    /// May be used to view and print the document; the document must be
    /// opened read-only.
    PreviewAndPrint,
    /// Must not be used.
    Restricted,
}

impl Embedding {
    fn from_fs_type(fs_type: u16) -> Self {
        // Older fonts may set several bits; the least restrictive applies.
        if fs_type & 0x000f == 0 {
            Embedding::Installable
        } else if fs_type & 0x0008 != 0 {
            Embedding::Editable
        } else if fs_type & 0x0004 != 0 {
            Embedding::PreviewAndPrint
        } else {
            Embedding::Restricted
        }
    }
}

/// An embedded font's header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub family: String,
    pub style: String,
    pub weight: u32,
    pub italic: bool,
    pub embedding: Embedding,
    /// The data is only the glyphs the document uses.
    pub subset: bool,
}

const TTEMBED_SUBSET: u32 = 0x0000_0001;
const TTEMBED_TTCOMPRESSED: u32 = 0x0000_0004;
const TTEMBED_XORENCRYPTDATA: u32 = 0x1000_0000;
const MAGIC: u16 = 0x504C;

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn utf16_at(d: &[u8], at: &mut usize) -> Option<String> {
    let size = u16_at(d, *at)? as usize;
    let bytes = d.get(*at + 2..*at + 2 + size)?;
    *at += 2 + size;
    let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
    Some(String::from_utf16_lossy(&units))
}

/// Read an EOT header.
pub fn header(eot: &[u8]) -> Result<Header, Error> {
    let bad = Error("EOT 形式ではありません");
    if u16_at(eot, 34) != Some(MAGIC) {
        return Err(bad);
    }
    let flags = u32_at(eot, 12).ok_or(bad)?;
    // Names start after the fixed part (80 bytes) and a padding word; each
    // is preceded by its size, the later ones by another padding word.
    let mut at = 82;
    let family = utf16_at(eot, &mut at).ok_or(bad)?;
    at += 2;
    let style = utf16_at(eot, &mut at).ok_or(bad)?;
    Ok(Header {
        family,
        style,
        weight: u32_at(eot, 28).ok_or(bad)?,
        italic: eot.get(27) == Some(&1),
        embedding: Embedding::from_fs_type(u16_at(eot, 32).ok_or(bad)?),
        subset: flags & TTEMBED_SUBSET != 0,
    })
}

/// Decode an EOT file into a TrueType font.
pub fn decode(eot: &[u8]) -> Result<Vec<u8>, Error> {
    let bad = Error("EOT 形式ではありません");
    header(eot)?;
    let eot_size = (u32_at(eot, 0).ok_or(bad)? as usize).min(eot.len());
    let data_size = u32_at(eot, 4).ok_or(bad)? as usize;
    let flags = u32_at(eot, 12).ok_or(bad)?;
    // The font data is the last part of the structure.
    let start = eot_size.checked_sub(data_size).ok_or(bad)?;
    let mut data = eot[start..eot_size].to_vec();
    if flags & TTEMBED_XORENCRYPTDATA != 0 {
        data.iter_mut().for_each(|b| *b ^= 0x50);
    }
    if flags & TTEMBED_TTCOMPRESSED == 0 {
        return Ok(data);
    }
    mtx(&data)
}

/// What the licence of a plain TrueType / OpenType font file allows for embedded use (its OS/2 `fsType`; a font with no OS/2 table
/// is unrestricted). `None` if the data is not a TrueType or OpenType file (a collection is not one).
pub fn embedding_of_font(ttf: &[u8]) -> Option<Embedding> {
    if !matches!(ttf.get(..4)?, [0, 1, 0, 0] | b"OTTO" | b"true") {
        return None;
    }
    let be16 = |at: usize| -> Option<usize> { Some(u16::from_be_bytes(ttf.get(at..at + 2)?.try_into().ok()?) as usize) };
    let be32 = |at: usize| -> Option<usize> { Some(u32::from_be_bytes(ttf.get(at..at + 4)?.try_into().ok()?) as usize) };
    let tables = be16(4)?;
    let Some(os2) = (0..tables).map(|i| 12 + 16 * i).find(|&at| ttf.get(at..at + 4) == Some(b"OS/2")) else { return Some(Embedding::Installable) };
    let fs_type = be16(be32(os2 + 8)? + 8)?;
    Some(Embedding::from_fs_type(fs_type as u16))
}

/// The family a TrueType font belongs to, as systems group it: the
/// typographic family (name ID 16) if present, else the family (ID 1).
/// "Poppins SemiBold" is in family "Poppins".
pub fn family_name(ttf: &[u8]) -> Option<String> {
    let be16 = |at: usize| -> Option<usize> { Some(u16::from_be_bytes(ttf.get(at..at + 2)?.try_into().ok()?) as usize) };
    let be32 = |at: usize| -> Option<usize> { Some(u32::from_be_bytes(ttf.get(at..at + 4)?.try_into().ok()?) as usize) };
    let tables = be16(4)?;
    let name = (0..tables).map(|i| 12 + 16 * i).find(|&at| ttf.get(at..at + 4) == Some(b"name"))?;
    let base = be32(name + 8)?;
    let (count, strings) = (be16(base + 2)?, base + be16(base + 4)?);
    let mut best: Option<(u32, String)> = None;
    for i in 0..count {
        let r = base + 6 + 12 * i;
        let (platform, encoding, language, id) = (be16(r)?, be16(r + 2)?, be16(r + 4)?, be16(r + 6)?);
        let (len, off) = (be16(r + 8)?, be16(r + 10)?);
        let rank_id = match id {
            16 => 0,
            1 => 2,
            _ => continue,
        };
        let bytes = ttf.get(strings + off..strings + off + len)?;
        let (rank_platform, text) = match (platform, encoding) {
            (3, 1 | 10) | (0, _) => {
                let units: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
                (if language == 0x409 || platform == 0 { 0 } else { 1 }, String::from_utf16_lossy(&units))
            }
            (1, 0) if language == 0 => (1, bytes.iter().map(|&b| b as char).collect()),
            _ => continue,
        };
        let rank = rank_id + rank_platform;
        if !text.is_empty() && best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, text));
        }
    }
    best.map(|(_, name)| name)
}

/// MicroType Express: a 10-byte header and three LZCOMP blocks (section 2).
fn mtx(data: &[u8]) -> Result<Vec<u8>, Error> {
    let bad = Error("MicroType Express 形式ではありません");
    let u24 = |at: usize| -> Option<usize> { Some(((*data.get(at)? as usize) << 16) | ((*data.get(at + 1)? as usize) << 8) | *data.get(at + 2)? as usize) };
    let (offset2, offset3) = (u24(4).ok_or(bad)?, u24(7).ok_or(bad)?);
    if !(10 <= offset2 && offset2 <= offset3 && offset3 <= data.len()) {
        return Err(bad);
    }
    // The three blocks are compressed on their own: decompress them at once.
    let block = |range: std::ops::Range<usize>| if range.is_empty() { Ok(Vec::new()) } else { lzcomp::decompress(&data[range]) };
    #[cfg(target_family = "wasm")]
    let (block1, block2, block3) = (block(10..offset2), block(offset2..offset3), block(offset3..data.len()));
    #[cfg(not(target_family = "wasm"))]
    let (block1, block2, block3) = std::thread::scope(|scope| {
        let block2 = scope.spawn(|| block(offset2..offset3));
        let block3 = scope.spawn(|| block(offset3..data.len()));
        let block1 = block(10..offset2);
        let join = |h: std::thread::ScopedJoinHandle<'_, Result<Vec<u8>, Error>>| h.join().unwrap_or(Err(bad));
        (block1, join(block2), join(block3))
    });
    ctf::to_truetype(&block1?, &block2?, &block3?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_levels() {
        assert_eq!(Embedding::from_fs_type(0), Embedding::Installable);
        assert_eq!(Embedding::from_fs_type(8), Embedding::Editable);
        assert_eq!(Embedding::from_fs_type(4), Embedding::PreviewAndPrint);
        assert_eq!(Embedding::from_fs_type(2), Embedding::Restricted);
        assert_eq!(Embedding::from_fs_type(0x0104), Embedding::PreviewAndPrint);
    }

    #[test]
    fn rejects_non_eot() {
        assert!(decode(b"definitely not an embedded font file, just some bytes here").is_err());
    }
}
