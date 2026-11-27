// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The compound file (OLE 2) container of the old Office formats and of
//! password-protected files, from its public specification (MS-CFB): read
//! (the header, the sector chains (FAT and mini FAT), the directory, as
//! far as finding a stream by name) and written (version 3, 512-byte
//! sectors, with storages and streams of any size).

use crate::Error;


const MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const FREE: u32 = 0xFFFF_FFFA;
const END: u32 = 0xFFFF_FFFE;

/// Whether `data` is a compound file.
pub fn is_cfb(data: &[u8]) -> bool {
    data.starts_with(&MAGIC)
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(d: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(d.get(at..at + 8)?.try_into().ok()?))
}

struct Entry {
    name: String,
    kind: u8,
    start: u32,
    size: u64,
    left: u32,
    right: u32,
    child: u32,
}

/// The bytes of the first stream with this leaf name, at any depth. Use
/// [`stream_path`] when the storage containing it matters.
pub fn stream(data: &[u8], name: &str) -> Result<Vec<u8>, Error> {
    read_directory_or_stream(data, Some((name, false))).map(|(_, bytes)| bytes.unwrap_or_default())
}

/// A reachable compound-file directory entry. Paths are relative to the root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryEntry {
    pub path: String,
    pub is_storage: bool,
}

/// List the reachable directory tree, rejecting cycles and invalid references.
pub fn directory(data: &[u8]) -> Result<Vec<DirectoryEntry>, Error> {
    read_directory_or_stream(data, None).map(|(entries, _)| entries)
}

/// Read the stream at an exact root-relative path.
pub fn stream_path(data: &[u8], path: &str) -> Result<Vec<u8>, Error> {
    read_directory_or_stream(data, Some((path.trim_start_matches('/'), true)))
        .map(|(_, bytes)| bytes.unwrap_or_default())
}

fn read_directory_or_stream(data: &[u8], request: Option<(&str, bool)>) -> Result<(Vec<DirectoryEntry>, Option<Vec<u8>>), Error> {
    let bad = || Error::new("複合ファイルを読めませんでした");
    if !is_cfb(data) {
        return Err(bad());
    }
    let sector_shift = u16_at(data, 30).ok_or_else(bad)?;
    let mini_shift = u16_at(data, 32).ok_or_else(bad)?;
    if !matches!(sector_shift, 9 | 12) || mini_shift != 6 { return Err(bad()); }
    let sector_size = 1usize << sector_shift;
    let mini_size = 1usize << mini_shift;
    let sector_count = data.len() / sector_size;
    if sector_count == 0 { return Err(bad()); }
    let dir_start = u32_at(data, 48).ok_or_else(bad)?;
    let mini_cutoff = u32_at(data, 56).ok_or_else(bad)? as u64;
    let mini_fat_start = u32_at(data, 60).ok_or_else(bad)?;
    let difat_start = u32_at(data, 68).ok_or_else(bad)?;
    let sector = |i: u32| -> Option<&[u8]> {
        let at = (i as usize + 1).checked_mul(sector_size)?;
        data.get(at..at + sector_size)
    };
    // The sectors of the FAT: 109 in the header, then the DIFAT's chain.
    let mut fat_sectors: Vec<u32> = (0..109).filter_map(|i| u32_at(data, 76 + i * 4)).filter(|&s| s < FREE).collect();
    let mut next = difat_start;
    let mut visited_difat = std::collections::HashSet::new();
    while next < FREE {
        if next as usize >= sector_count || !visited_difat.insert(next) { return Err(bad()); }
        let s = sector(next).ok_or_else(bad)?;
        let per = sector_size / 4;
        fat_sectors.extend((0..per - 1).filter_map(|i| u32_at(s, i * 4)).filter(|&x| x < FREE));
        next = u32_at(s, (per - 1) * 4).ok_or_else(bad)?;
        if fat_sectors.len() > sector_count { return Err(bad()); }
    }
    let mut seen_fat = std::collections::HashSet::new();
    let mut fat: Vec<u32> = Vec::new();
    for s in fat_sectors {
        if s as usize >= sector_count || !seen_fat.insert(s) { return Err(bad()); }
        let b = sector(s).ok_or_else(bad)?;
        fat.extend((0..sector_size / 4).filter_map(|i| u32_at(b, i * 4)));
    }
    let chain = |start: u32, table: &[u32]| -> Result<Vec<u32>, Error> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut s = start;
        while s < FREE {
            if !seen.insert(s) { return Err(bad()); }
            out.push(s);
            s = *table.get(s as usize).ok_or_else(bad)?;
        }
        Ok(out)
    };
    let read_chain = |start: u32| -> Result<Vec<u8>, Error> {
        let mut out = Vec::new();
        for s in chain(start, &fat)? {
            out.extend_from_slice(sector(s).ok_or_else(bad)?);
        }
        Ok(out)
    };
    let dir = read_chain(dir_start)?;
    let entries: Vec<Entry> = (0..dir.len() / 128)
        .filter_map(|i| {
            let e = &dir[i * 128..(i + 1) * 128];
            let len = (u16_at(e, 64)? as usize).saturating_sub(2).min(64);
            let units: Vec<u16> = (0..len / 2).filter_map(|k| u16_at(e, k * 2)).collect();
            Some(Entry { name: String::from_utf16_lossy(&units), kind: e[66], start: u32_at(e, 116)?, size: u64_at(e, 120)?, left: u32_at(e, 68)?, right: u32_at(e, 72)?, child: u32_at(e, 76)? })
        })
        .collect();
    let mut paths = Vec::new();
    let mut target_index = None;
    if request.is_none() || request.is_some_and(|(_, exact)| exact) {
        let root = entries.first().filter(|entry| entry.kind == 5).ok_or_else(bad)?;
        let mut stack = vec![(root.child, String::new())];
        let mut seen = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        let mut path_bytes = 0usize;
        while let Some((id, parent)) = stack.pop() {
            if id == u32::MAX { continue; }
            let entry = entries.get(id as usize).filter(|e| matches!(e.kind, 1 | 2)).ok_or_else(bad)?;
            if id == 0 || !seen.insert(id) || entry.name.is_empty() || entry.name.contains('/') { return Err(bad()); }
            let path = if parent.is_empty() { entry.name.clone() } else { format!("{parent}/{}", entry.name) };
            path_bytes = path_bytes.checked_add(path.len()).ok_or_else(bad)?;
            if seen.len() > 100_000 || path.len() > 4096 || path_bytes > 16 * 1024 * 1024 || !names.insert(path.clone()) { return Err(bad()); }
            if entry.kind == 2 && request.is_some_and(|(wanted, _)| wanted == path) { target_index = Some(id as usize); }
            paths.push(DirectoryEntry { path: path.clone(), is_storage: entry.kind == 1 });
            stack.push((entry.left, parent.clone()));
            stack.push((entry.right, parent));
            if entry.kind == 1 { stack.push((entry.child, path)); }
        }
    }
    let Some((name, exact)) = request else { return Ok((paths, None)); };
    let target = if exact { target_index.and_then(|i| entries.get(i)) } else { entries.iter().find(|e| e.kind == 2 && e.name == name) }
        .ok_or_else(|| Error::new(format!("{name} のデータがありません")))?;
    if target.size >= mini_cutoff {
        let mut bytes = read_chain(target.start)?;
        bytes.truncate(target.size as usize);
        return Ok((paths, Some(bytes)));
    }
    // A short stream: in the mini stream (the root's data), by the mini FAT.
    let root = entries.first().ok_or_else(bad)?;
    let mini_stream = read_chain(root.start)?;
    let mini_fat_bytes = read_chain(mini_fat_start)?;
    let mini_fat: Vec<u32> = (0..mini_fat_bytes.len() / 4).filter_map(|i| u32_at(&mini_fat_bytes, i * 4)).collect();
    let mut out = Vec::new();
    for s in chain(target.start, &mini_fat)? {
        let at = s as usize * mini_size;
        out.extend_from_slice(mini_stream.get(at..at + mini_size).ok_or_else(bad)?);
    }
    out.truncate(target.size as usize);
    Ok((paths, Some(out)))
}


/// A storage or stream to write: `path` names it from the root with `/`
/// between storages (`"\u{6}DataSpaces/Version"`).
pub struct Item {
    pub path: String,
    /// The bytes of a stream; `None` for a storage.
    pub data: Option<Vec<u8>>,
}

impl Item {
    pub fn stream(path: &str, data: Vec<u8>) -> Self {
        Self { path: path.to_string(), data: Some(data) }
    }

    pub fn storage(path: &str) -> Self {
        Self { path: path.to_string(), data: None }
    }
}

const SECTOR: usize = 512;
const MINI_SECTOR: usize = 64;
const MINI_CUTOFF: usize = 4096;
const NOSTREAM: u32 = 0xFFFF_FFFF;
const FATSECT: u32 = 0xFFFF_FFFD;
const DIFSECT: u32 = 0xFFFF_FFFC;

struct Node {
    name: String,
    kind: u8,
    data: Vec<u8>,
    children: Vec<usize>,
    left: u32,
    right: u32,
    child: u32,
    red: bool,
    start: u32,
}

/// How the directory orders names (MS-CFB 2.6.4): shorter first, then by upper-cased UTF-16 code units.
fn name_order(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b): (Vec<u16>, Vec<u16>) = (a.to_uppercase().encode_utf16().collect(), b.to_uppercase().encode_utf16().collect());
    a.len().cmp(&b.len()).then(a.cmp(&b))
}

/// The siblings `ids` (sorted) as a balanced search tree: the root's index, with `left`, `right` and the colours set (the deepest, incomplete level red).
fn link(nodes: &mut [Node], ids: &[usize]) -> u32 {
    fn build(nodes: &mut [Node], ids: &[usize], depth: usize, full: usize) -> u32 {
        if ids.is_empty() {
            return NOSTREAM;
        }
        let mid = ids.len() / 2;
        let id = ids[mid];
        let (left, right) = (build(nodes, &ids[..mid], depth + 1, full), build(nodes, &ids[mid + 1..], depth + 1, full));
        nodes[id].left = left;
        nodes[id].right = right;
        // A tree that is not complete: its last level is red, so every path has as many black nodes.
        nodes[id].red = depth > 0 && depth == full && ids.len() < 1 << (usize::BITS - 1);
        id as u32
    }
    // The depth of the complete part: floor(log2(n + 1)) levels are full.
    let n = ids.len();
    let levels = (usize::BITS - (n + 1).leading_zeros() - 1) as usize;
    let complete = (1usize << levels) - 1 == n;
    let root = build(nodes, ids, 0, if complete { usize::MAX } else { levels });
    if root != NOSTREAM {
        nodes[root as usize].red = false;
    }
    root
}

/// A compound file holding `items` (storages must come before what is in them, as `a`, `a/b`).
pub fn write(items: &[Item]) -> Vec<u8> {
    write_with_clsid(items, [0; 16])
}

/// A compound file whose root storage has the class `clsid` (16 bytes as stored: an OLE object's file names its class there).
pub fn write_with_clsid(items: &[Item], clsid: [u8; 16]) -> Vec<u8> {
    // The tree of entries: node 0 is the root storage.
    let mut nodes = vec![Node { name: "Root Entry".into(), kind: 5, data: Vec::new(), children: Vec::new(), left: NOSTREAM, right: NOSTREAM, child: NOSTREAM, red: false, start: END }];
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    index.insert(String::new(), 0);
    for item in items {
        let (parent, name) = match item.path.rsplit_once('/') {
            Some((p, n)) => (p.to_string(), n.to_string()),
            None => (String::new(), item.path.clone()),
        };
        let parent = *index.get(&parent).expect("a storage before what is in it");
        let id = nodes.len();
        nodes.push(Node { name, kind: if item.data.is_some() { 2 } else { 1 }, data: item.data.clone().unwrap_or_default(), children: Vec::new(), left: NOSTREAM, right: NOSTREAM, child: NOSTREAM, red: false, start: END });
        nodes[parent].children.push(id);
        index.insert(item.path.clone(), id);
    }
    // Siblings as search trees.
    for id in 0..nodes.len() {
        let mut kids = nodes[id].children.clone();
        kids.sort_by(|&a, &b| name_order(&nodes[a].name, &nodes[b].name));
        let root = link(&mut nodes, &kids);
        nodes[id].child = root;
    }
    // Short streams go in the mini stream, the others in sectors of their own.
    let mut mini_stream: Vec<u8> = Vec::new();
    let mut mini_fat: Vec<u32> = Vec::new();
    for node in nodes.iter_mut().filter(|n| n.kind == 2) {
        if node.data.is_empty() {
            node.start = END;
        } else if node.data.len() < MINI_CUTOFF {
            node.start = mini_fat.len() as u32;
            let count = node.data.len().div_ceil(MINI_SECTOR);
            for i in 0..count {
                mini_fat.push(if i + 1 == count { END } else { mini_fat.len() as u32 + 1 });
            }
            mini_stream.extend_from_slice(&node.data);
            mini_stream.resize(mini_stream.len().next_multiple_of(MINI_SECTOR), 0);
        }
    }
    let big: Vec<usize> = (0..nodes.len()).filter(|&i| nodes[i].kind == 2 && nodes[i].data.len() >= MINI_CUTOFF).collect();
    // How many sectors each part takes.
    let dir_sectors = nodes.len().div_ceil(SECTOR / 128);
    let mini_fat_sectors = (mini_fat.len() * 4).div_ceil(SECTOR);
    let mini_stream_sectors = mini_stream.len().div_ceil(SECTOR);
    let big_sectors: usize = big.iter().map(|&i| nodes[i].data.len().div_ceil(SECTOR)).sum();
    let data_sectors = dir_sectors + mini_fat_sectors + mini_stream_sectors + big_sectors;
    let per_fat = SECTOR / 4;
    let (mut fat_sectors, mut difat_sectors) = (1usize, 0usize);
    loop {
        let need_difat = fat_sectors.saturating_sub(109).div_ceil(per_fat - 1);
        let need_fat = (data_sectors + fat_sectors + need_difat).div_ceil(per_fat);
        if need_fat == fat_sectors && need_difat == difat_sectors {
            break;
        }
        fat_sectors = need_fat;
        difat_sectors = need_difat;
    }
    // Where each goes: directory, mini FAT, mini stream, big streams, FAT, DIFAT.
    let mut fat: Vec<u32> = vec![NOSTREAM; (data_sectors + fat_sectors + difat_sectors).next_multiple_of(per_fat)];
    let mut next_sector = 0u32;
    let mut allocate = |fat: &mut Vec<u32>, count: usize| -> u32 {
        let start = next_sector;
        for i in 0..count {
            fat[start as usize + i] = if i + 1 == count { END } else { start + i as u32 + 1 };
        }
        next_sector += count as u32;
        if count == 0 { END } else { start }
    };
    let dir_start = allocate(&mut fat, dir_sectors);
    let mini_fat_start = allocate(&mut fat, mini_fat_sectors);
    let mini_stream_start = allocate(&mut fat, mini_stream_sectors);
    let mut big_starts = Vec::new();
    for &i in &big {
        big_starts.push((i, allocate(&mut fat, nodes[i].data.len().div_ceil(SECTOR))));
    }
    let fat_start = next_sector;
    for i in 0..fat_sectors {
        fat[(fat_start as usize) + i] = FATSECT;
    }
    next_sector += fat_sectors as u32;
    let difat_start = next_sector;
    for i in 0..difat_sectors {
        fat[(difat_start as usize) + i] = DIFSECT;
    }
    for (i, start) in big_starts {
        nodes[i].start = start;
    }
    nodes[0].start = if mini_stream.is_empty() { END } else { mini_stream_start };
    // The file.
    let total = 1 + data_sectors + fat_sectors + difat_sectors;
    let mut out = vec![0u8; total * SECTOR];
    let put = |out: &mut [u8], sector: u32, bytes: &[u8]| {
        let at = (sector as usize + 1) * SECTOR;
        out[at..at + bytes.len()].copy_from_slice(bytes);
    };
    // The directory.
    let mut dir = Vec::with_capacity(dir_sectors * SECTOR);
    for node in &nodes {
        let mut e = [0u8; 128];
        let units: Vec<u16> = node.name.encode_utf16().collect();
        for (i, u) in units.iter().enumerate() {
            e[i * 2..i * 2 + 2].copy_from_slice(&u.to_le_bytes());
        }
        e[64..66].copy_from_slice(&(((units.len() + 1) * 2) as u16).to_le_bytes());
        e[66] = node.kind;
        e[67] = if node.red { 0 } else { 1 };
        if node.kind == 5 {
            e[80..96].copy_from_slice(&clsid);
        }
        e[68..72].copy_from_slice(&node.left.to_le_bytes());
        e[72..76].copy_from_slice(&node.right.to_le_bytes());
        e[76..80].copy_from_slice(&node.child.to_le_bytes());
        let size = if node.kind == 5 { mini_stream.len() as u64 } else { node.data.len() as u64 };
        let start = if node.kind == 1 { 0 } else { node.start };
        e[116..120].copy_from_slice(&start.to_le_bytes());
        e[120..128].copy_from_slice(&size.to_le_bytes());
        dir.extend_from_slice(&e);
    }
    // Unused entries are free ones.
    while dir.len() < dir_sectors * SECTOR {
        let mut e = [0u8; 128];
        e[68..80].copy_from_slice(&[0xFF; 12]);
        dir.extend_from_slice(&e);
    }
    put(&mut out, dir_start, &dir);
    if mini_fat_sectors > 0 {
        let mut bytes: Vec<u8> = mini_fat.iter().flat_map(|v| v.to_le_bytes()).collect();
        bytes.resize(mini_fat_sectors * SECTOR, 0xFF);
        put(&mut out, mini_fat_start, &bytes);
    }
    if !mini_stream.is_empty() {
        put(&mut out, mini_stream_start, &mini_stream);
    }
    for &i in &big {
        let start = nodes[i].start;
        put(&mut out, start, &nodes[i].data);
    }
    // The FAT.
    for (i, chunk) in fat.chunks(per_fat).take(fat_sectors).enumerate() {
        let bytes: Vec<u8> = chunk.iter().flat_map(|v| v.to_le_bytes()).collect();
        put(&mut out, fat_start + i as u32, &bytes);
    }
    // The header.
    let mut h = [0u8; SECTOR];
    h[..8].copy_from_slice(&MAGIC);
    h[24..26].copy_from_slice(&0x003Eu16.to_le_bytes());
    h[26..28].copy_from_slice(&3u16.to_le_bytes());
    h[28..30].copy_from_slice(&0xFFFEu16.to_le_bytes());
    h[30..32].copy_from_slice(&9u16.to_le_bytes());
    h[32..34].copy_from_slice(&6u16.to_le_bytes());
    h[44..48].copy_from_slice(&(fat_sectors as u32).to_le_bytes());
    h[48..52].copy_from_slice(&dir_start.to_le_bytes());
    h[56..60].copy_from_slice(&(MINI_CUTOFF as u32).to_le_bytes());
    h[60..64].copy_from_slice(&(if mini_fat_sectors > 0 { mini_fat_start } else { END }).to_le_bytes());
    h[64..68].copy_from_slice(&(mini_fat_sectors as u32).to_le_bytes());
    h[68..72].copy_from_slice(&(if difat_sectors > 0 { difat_start } else { END }).to_le_bytes());
    h[72..76].copy_from_slice(&(difat_sectors as u32).to_le_bytes());
    for i in 0..109 {
        let v = if i < fat_sectors { fat_start + i as u32 } else { NOSTREAM };
        h[76 + i * 4..80 + i * 4].copy_from_slice(&v.to_le_bytes());
    }
    out[..SECTOR].copy_from_slice(&h);
    // The DIFAT sectors: the FAT sectors past the 109 in the header, 127 to a sector and the next one's number last.
    for d in 0..difat_sectors {
        let mut bytes = [0xFFu8; SECTOR];
        for k in 0..per_fat - 1 {
            let n = 109 + d * (per_fat - 1) + k;
            if n < fat_sectors {
                bytes[k * 4..k * 4 + 4].copy_from_slice(&(fat_start + n as u32).to_le_bytes());
            }
        }
        let next = if d + 1 < difat_sectors { difat_start + d as u32 + 1 } else { END };
        bytes[(per_fat - 1) * 4..].copy_from_slice(&next.to_le_bytes());
        put(&mut out, difat_start + d as u32, &bytes);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(n: usize, seed: u8) -> Vec<u8> {
        (0..n).map(|i| (i as u8).wrapping_mul(7).wrapping_add(seed)).collect()
    }

    #[test]
    fn what_is_written_is_read() {
        // Short streams (mini stream), long ones, an empty one, storages.
        let items = vec![
            Item::stream("Short", bytes(100, 1)),
            Item::stream("Edge", bytes(4095, 2)),
            Item::stream("Long", bytes(4096, 3)),
            Item::stream("Huge", bytes(600_000, 4)),
            Item::stream("Empty", Vec::new()),
            Item::storage("\u{6}Data"),
            Item::stream("\u{6}Data/Version", bytes(76, 5)),
            Item::storage("\u{6}Data/Info"),
            Item::stream("\u{6}Data/Info/Primary", bytes(5000, 6)),
        ];
        let file = write(&items);
        assert!(is_cfb(&file) && file.len() % 512 == 0);
        for item in items.iter().filter(|i| i.data.is_some()) {
            let leaf = item.path.rsplit('/').next().unwrap();
            assert_eq!(stream(&file, leaf).unwrap(), item.data.clone().unwrap(), "{}", item.path);
        }
    }

    #[test]
    fn exact_paths_distinguish_nested_streams_and_reject_cycles() {
        let file = write(&[
            Item::storage("Folder"),
            Item::stream("Folder/DocumentText", b"nested".to_vec()),
            Item::stream("DocumentText", b"root".to_vec()),
        ]);
        assert_eq!(stream_path(&file, "DocumentText").unwrap(), b"root");
        assert_eq!(stream_path(&file, "Folder/DocumentText").unwrap(), b"nested");
        assert!(stream_path(&file, "Missing/DocumentText").is_err());
        assert!(directory(&file).unwrap().iter().any(|e| e.path == "Folder/DocumentText" && !e.is_storage));
        let mut cycle = file;
        let directory_sector = u32_at(&cycle, 48).unwrap() as usize;
        let root = (directory_sector + 1) * 512;
        cycle[root + 76..root + 80].copy_from_slice(&0u32.to_le_bytes());
        assert!(directory(&cycle).is_err());
    }

    #[test]
    fn a_large_file_needs_the_difat() {
        // More than 109 FAT sectors (about 7 MB).
        let data = bytes(9_000_000, 9);
        let file = write(&[Item::stream("EncryptedPackage", data.clone())]);
        assert_eq!(stream(&file, "EncryptedPackage").unwrap(), data);
    }

    #[test]
    fn rejects_invalid_sector_shift_and_cyclic_directory_chain() {
        let original = write(&[Item::stream("DocumentText", bytes(100, 1))]);
        let mut invalid = original.clone();
        invalid[30..32].copy_from_slice(&255u16.to_le_bytes());
        assert!(stream(&invalid, "DocumentText").is_err());
        let mut cyclic = original;
        let directory = u32_at(&cyclic, 48).unwrap();
        let fat = u32_at(&cyclic, 76).unwrap();
        let offset = (fat as usize + 1) * 512 + directory as usize * 4;
        cyclic[offset..offset + 4].copy_from_slice(&directory.to_le_bytes());
        assert!(stream(&cyclic, "DocumentText").is_err());
    }

    #[test]
    fn the_trees_are_searchable() {
        // Names in the order the directory wants: shorter first, then upper-cased.
        let names = ["b", "a", "ab", "AA", "zz", "c", "d", "e", "f"];
        let items: Vec<Item> = names.iter().map(|n| Item::stream(n, bytes(10, 1))).collect();
        let file = write(&items);
        for n in names {
            assert_eq!(stream(&file, n).unwrap().len(), 10, "{n}");
        }
    }
}
