// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Lossless PDF container supporting lazy object resolution, revision snapshots,
//! and non-destructive incremental updates.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use crate::cos::{CosDict, CosObj, Lexer, ObjectId, StreamData, Token, parse_indirect_object, write_cos};
use crate::xref::{Revision, XrefEntry, decode_stream_bytes, parse_revisions};
use crate::{Error, Result};

/// How [`PdfContainer::save`] writes changes back to bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveMode {
    /// Append modified/new objects and a new xref/trailer after the original bytes.
    /// Required to preserve existing digital signatures.
    Incremental,
    /// Rewrite the entire PDF from scratch (drops revision history and invalidates signatures).
    FullRewrite,
}

/// Lossless PDF container keeping original bytes, revision history, and pending edits.
pub struct PdfContainer {
    raw: Arc<[u8]>,
    revisions: Vec<Revision>,
    dirty: BTreeMap<ObjectId, CosObj>,
    cache: HashMap<(usize, ObjectId), CosObj>,
    next_obj_num: u32,
}

impl PdfContainer {
    /// Parse the PDF header/trailer/xref chain without eagerly parsing every object.
    pub fn from_bytes(bytes: impl Into<Arc<[u8]>>) -> Result<Self> {
        let raw: Arc<[u8]> = bytes.into();
        if !raw.starts_with(b"%PDF-") {
            return Err(Error::new("not a PDF file (missing %PDF- header)"));
        }
        let revisions = parse_revisions(&raw)?;
        if revisions.is_empty() {
            return Err(Error::new("PDF contains no xref revisions"));
        }
        let mut max_num = 0u32;
        for rev in &revisions {
            if let Some(size) = rev.trailer.get_int(b"Size").filter(|&s| s > 0) {
                max_num = max_num.max(size as u32);
            }
            for &num in rev.entries.keys() {
                max_num = max_num.max(num + 1);
            }
        }
        Ok(Self {
            raw,
            revisions,
            dirty: BTreeMap::new(),
            cache: HashMap::new(),
            next_obj_num: max_num.max(1),
        })
    }

    pub fn raw_bytes(&self) -> &[u8] {
        &self.raw
    }

    pub fn revisions(&self) -> &[Revision] {
        &self.revisions
    }

    pub fn latest_revision_index(&self) -> usize {
        self.revisions.len() - 1
    }

    /// Resolve an object as it existed at `revision_index` (ignoring pending `dirty` edits).
    pub fn resolve_at(&mut self, id: ObjectId, revision_index: usize) -> Result<CosObj> {
        if revision_index >= self.revisions.len() {
            return Err(Error::new("revision index out of range"));
        }
        if let Some(cached) = self.cache.get(&(revision_index, id)) {
            return Ok(cached.clone());
        }

        let mut found = None;
        for rev in self.revisions[..=revision_index].iter().rev() {
            if let Some(&entry) = rev.entries.get(&id.num) {
                found = Some(entry);
                break;
            }
        }

        let obj = match found {
            None | Some(XrefEntry::Free { .. }) => CosObj::Null,
            Some(XrefEntry::InUse { offset, generation }) => {
                if generation != id.generation {
                    CosObj::Null
                } else {
                    let (parsed_id, parsed_obj) = parse_indirect_object(&self.raw, offset, 0)?;
                    if parsed_id != id {
                        return Err(Error::new(format!(
                            "xref offset {} pointed to {} {} obj instead of {} {} obj",
                            offset, parsed_id.num, parsed_id.generation, id.num, id.generation
                        )));
                    }
                    parsed_obj
                }
            }
            Some(XrefEntry::Compressed { obj_stm_num, index }) => {
                if id.generation != 0 {
                    CosObj::Null
                } else {
                    self.read_from_obj_stm(obj_stm_num, id.num, index, revision_index)?
                }
            }
        };

        self.cache.insert((revision_index, id), obj.clone());
        Ok(obj)
    }

    /// Resolve an object in the current working state (`dirty` overrides latest revision).
    pub fn resolve(&mut self, id: ObjectId) -> Result<CosObj> {
        if let Some(obj) = self.dirty.get(&id) {
            return Ok(obj.clone());
        }
        self.resolve_at(id, self.latest_revision_index())
    }

    /// Dereference `obj` if it is a `CosObj::Ref`, otherwise clone it as-is.
    pub fn deref(&mut self, obj: &CosObj) -> Result<CosObj> {
        match obj {
            CosObj::Ref(id) => self.resolve(*id),
            other => Ok(other.clone()),
        }
    }

    /// Return the document `/Root` catalog `(ObjectId, CosDict)`.
    pub fn catalog(&mut self) -> Result<(ObjectId, CosDict)> {
        let rev_idx = self.latest_revision_index();
        let root_id = self.revisions[rev_idx]
            .trailer
            .get_ref(b"Root")
            .ok_or_else(|| Error::new("trailer missing /Root catalog"))?;
        let obj = self.resolve(root_id)?;
        let dict = obj
            .as_dict()
            .cloned()
            .ok_or_else(|| Error::new("/Root catalog is not a dictionary"))?;
        Ok((root_id, dict))
    }

    /// Decode a stream's payload into plain bytes.
    pub fn decode_stream(&self, dict: &CosDict, data: &StreamData) -> Result<Vec<u8>> {
        decode_stream_bytes(dict, data, &self.raw)
    }

    /// Allocate a new indirect object in the pending `dirty` map.
    pub fn alloc_object(&mut self, obj: CosObj) -> ObjectId {
        let id = ObjectId::new(self.next_obj_num, 0);
        self.next_obj_num += 1;
        self.dirty.insert(id, obj);
        id
    }

    /// Stage a modification to an existing indirect object.
    pub fn set_object(&mut self, id: ObjectId, obj: CosObj) {
        if id.num >= self.next_obj_num {
            self.next_obj_num = id.num + 1;
        }
        self.dirty.insert(id, obj);
    }

    pub fn has_pending_edits(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// List object numbers added or modified in revisions strictly after `after_rev`.
    pub fn changed_objects_after(&self, after_rev: usize) -> Vec<u32> {
        let mut nums: Vec<u32> = self.revisions[after_rev.saturating_add(1)..]
            .iter()
            .flat_map(|r| r.entries.keys().copied())
            .collect();
        nums.sort_unstable();
        nums.dedup();
        nums
    }

    /// Save the PDF using the requested [`SaveMode`].
    pub fn save(&mut self, mode: SaveMode) -> Result<Vec<u8>> {
        match mode {
            SaveMode::Incremental => self.save_incremental(),
            SaveMode::FullRewrite => self.save_full_rewrite(),
        }
    }

    fn save_incremental(&self) -> Result<Vec<u8>> {
        use std::io::Write as _;
        if self.dirty.is_empty() {
            return Ok(self.raw.to_vec());
        }

        let mut out = Vec::with_capacity(self.raw.len() + 1024);
        out.extend_from_slice(&self.raw);
        if !out.ends_with(b"\n") && !out.ends_with(b"\r") {
            out.push(b'\n');
        }

        let mut new_entries: Vec<(ObjectId, usize)> = Vec::with_capacity(self.dirty.len());
        for (&id, obj) in &self.dirty {
            let offset = out.len();
            let _ = writeln!(out, "{} {} obj", id.num, id.generation);
            write_cos(obj, &self.raw, &mut out);
            out.extend_from_slice(b"\nendobj\n");
            new_entries.push((id, offset));
        }

        let xref_offset = out.len();
        out.extend_from_slice(b"xref\n");
        // Emit contiguous subsections for dirty objects.
        let mut idx = 0;
        while idx < new_entries.len() {
            let start = idx;
            while idx + 1 < new_entries.len()
                && new_entries[idx + 1].0.num == new_entries[idx].0.num + 1
            {
                idx += 1;
            }
            let count = idx - start + 1;
            let _ = writeln!(out, "{} {}", new_entries[start].0.num, count);
            for &(id, offset) in &new_entries[start..=idx] {
                let _ = writeln!(out, "{offset:010} {:05} n ", id.generation);
            }
            idx += 1;
        }

        let latest = &self.revisions[self.latest_revision_index()];
        let mut trailer = latest.trailer.clone();
        trailer.remove(b"Length");
        trailer.remove(b"Filter");
        trailer.remove(b"DecodeParms");
        trailer.remove(b"W");
        trailer.remove(b"Index");
        trailer.remove(b"Type");
        trailer.remove(b"XRefStm");
        trailer.set(b"Size".to_vec(), CosObj::Int(self.next_obj_num as i64));
        trailer.set(b"Prev".to_vec(), CosObj::Int(latest.xref_offset as i64));

        out.extend_from_slice(b"trailer\n");
        write_cos(&CosObj::Dict(trailer), &self.raw, &mut out);
        let _ = write!(out, "\nstartxref\n{xref_offset}\n%%EOF\n");
        Ok(out)
    }

    fn save_full_rewrite(&mut self) -> Result<Vec<u8>> {
        use std::io::Write as _;
        let mut all_nums: Vec<u32> = self
            .revisions
            .iter()
            .flat_map(|r| r.entries.keys().copied())
            .chain(self.dirty.keys().map(|id| id.num))
            .filter(|&n| n > 0)
            .collect();
        all_nums.sort_unstable();
        all_nums.dedup();

        let mut out = Vec::with_capacity(self.raw.len());
        out.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");

        let mut offsets: BTreeMap<u32, (u16, usize)> = BTreeMap::new();
        for num in all_nums {
            let id = self.latest_object_id(num);
            let obj = self.resolve(id)?;
            if matches!(obj, CosObj::Null) {
                continue;
            }
            // Skip internal XRef/ObjStm objects when writing a flat xref table.
            if let Some(dict) = obj.as_dict()
                && matches!(dict.get_name(b"Type"), Some(b"XRef" | b"ObjStm"))
            {
                continue;
            }
            let offset = out.len();
            let _ = writeln!(out, "{} {} obj", id.num, id.generation);
            write_cos(&obj, &self.raw, &mut out);
            out.extend_from_slice(b"\nendobj\n");
            offsets.insert(id.num, (id.generation, offset));
        }

        let max_num = offsets.keys().next_back().copied().unwrap_or(0) + 1;
        let xref_offset = out.len();
        let _ = writeln!(out, "xref\n0 {max_num}");
        out.extend_from_slice(b"0000000000 65535 f \n");
        for num in 1..max_num {
            if let Some(&(generation, offset)) = offsets.get(&num) {
                let _ = writeln!(out, "{offset:010} {generation:05} n ");
            } else {
                out.extend_from_slice(b"0000000000 00000 f \n");
            }
        }

        let latest = &self.revisions[self.latest_revision_index()];
        let mut trailer = CosDict::new();
        trailer.set(b"Size".to_vec(), CosObj::Int(max_num as i64));
        if let Some(root) = latest.trailer.get(b"Root") {
            trailer.set(b"Root".to_vec(), root.clone());
        }
        if let Some(info) = latest.trailer.get(b"Info") {
            trailer.set(b"Info".to_vec(), info.clone());
        }
        out.extend_from_slice(b"trailer\n");
        write_cos(&CosObj::Dict(trailer), &self.raw, &mut out);
        let _ = write!(out, "\nstartxref\n{xref_offset}\n%%EOF\n");
        Ok(out)
    }

    fn latest_object_id(&self, num: u32) -> ObjectId {
        if let Some(id) = self.dirty.keys().rfind(|id| id.num == num) {
            return *id;
        }
        for rev in self.revisions.iter().rev() {
            if let Some(entry) = rev.entries.get(&num) {
                let generation = match entry {
                    XrefEntry::InUse { generation, .. } | XrefEntry::Free { generation, .. } => {
                        *generation
                    }
                    XrefEntry::Compressed { .. } => 0,
                };
                return ObjectId::new(num, generation);
            }
        }
        ObjectId::new(num, 0)
    }

    fn read_from_obj_stm(
        &mut self,
        obj_stm_num: u32,
        target_num: u32,
        _expected_index: u16,
        revision_index: usize,
    ) -> Result<CosObj> {
        let stm_obj = self.resolve_at(ObjectId::new(obj_stm_num, 0), revision_index)?;
        let CosObj::Stream { dict, data } = stm_obj else {
            return Err(Error::new("compressed object container is not a stream"));
        };
        let bytes = decode_stream_bytes(&dict, &data, &self.raw)?;
        let count = dict.get_int(b"N").unwrap_or(0).max(0) as usize;
        let first = dict.get_int(b"First").unwrap_or(0).max(0) as usize;

        let mut head = Lexer::new(&bytes, 0);
        for _ in 0..count {
            let Token::Obj(CosObj::Int(n)) = head.next_token()? else {
                break;
            };
            let Token::Obj(CosObj::Int(rel_off)) = head.next_token()? else {
                break;
            };
            if n as u32 == target_num {
                let mut body = Lexer::new(&bytes, first + rel_off.max(0) as usize);
                if let Token::Obj(obj) = body.next_token()? {
                    return Ok(obj);
                }
            }
        }
        Ok(CosObj::Null)
    }
}
