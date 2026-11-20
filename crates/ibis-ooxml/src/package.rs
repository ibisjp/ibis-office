// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! OPC packages for writing: the parts of a file as read, changed in place
//! and written back.
//!
//! A [`Package`] keeps every part of the file it was opened from. A part
//! that is not touched is written back as its original compressed bytes; a
//! part that is changed is kept as a parsed [`xml::Document`] (which keeps
//! what it does not change, see [`crate::xml`]) or as new bytes.

use std::collections::HashMap;
use std::sync::Arc;

use crate::xml::{Document, NodeId};
use crate::zip::{Archive, Entry, EntryMeta, Writer};
use crate::Error;

pub const CONTENT_TYPES: &str = "[Content_Types].xml";
const CONTENT_TYPES_NS: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const RELATIONSHIPS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

#[derive(Clone)]
enum Content {
    /// As read: entry `usize` of the archive.
    Original(usize),
    Bytes(Arc<[u8]>),
    Xml(Box<Document>),
}

#[derive(Clone)]
struct Part {
    name: String,
    meta: EntryMeta,
    content: Content,
}

/// How [`Package::save`] writes the parts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SaveOptions {
    /// Parse every XML part and write it from its parts
    /// ([`Document::to_bytes_regenerated`]), changed or not. For tests: it
    /// shows that writing the tree keeps the meaning of every part.
    pub regenerate_xml: bool,
}

/// A relationship of a part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relationship {
    pub id: String,
    pub kind: String,
    /// As written in the relationships part (relative to the source part).
    pub target: String,
    pub external: bool,
}

impl Relationship {
    /// The part name the target names, resolved against `source` (the part
    /// whose relationship it is; "" for the package). `None` if external.
    pub fn target_part(&self, source: &str) -> Option<String> {
        (!self.external).then(|| resolve(dir_of(source), &self.target))
    }
}

#[derive(Clone)]
pub struct Package {
    archive: Archive,
    parts: Vec<Part>,
    index: HashMap<String, usize>,
}

impl Package {
    pub fn open(data: impl Into<Arc<[u8]>>) -> Result<Self, Error> {
        let archive = Archive::new(data.into())?;
        let mut parts = Vec::with_capacity(archive.entries().len());
        let mut index = HashMap::new();
        for (i, entry) in archive.entries().iter().enumerate() {
            if index.insert(entry.name.clone(), parts.len()).is_some() {
                return Err(Error::new(format!("{} が 2 つあります", entry.name)));
            }
            parts.push(Part { name: entry.name.clone(), meta: entry.meta, content: Content::Original(i) });
        }
        if !index.contains_key(CONTENT_TYPES) {
            return Err(Error::new("Office のファイルではありません（[Content_Types].xml がありません）"));
        }
        Ok(Self { archive, parts, index })
    }

    /// Part names (ZIP entry names, no leading slash) in the archive's order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.parts.iter().map(|p| p.name.as_str())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.index.contains_key(name)
    }

    fn part(&self, name: &str) -> Result<&Part, Error> {
        self.index.get(name).map(|&i| &self.parts[i]).ok_or_else(|| Error::new(format!("{name} がありません")))
    }

    fn entry(&self, i: usize) -> &Entry {
        &self.archive.entries()[i]
    }

    /// The bytes of part `name`, uncompressed, as they would be saved now.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, Error> {
        match &self.part(name)?.content {
            Content::Original(i) => self.archive.read(self.entry(*i)),
            Content::Bytes(b) => Ok(b.to_vec()),
            Content::Xml(doc) => Ok(doc.to_bytes()),
        }
    }

    /// Part `name`, parsed (a copy: changes to it are not saved).
    pub fn xml(&self, name: &str) -> Result<Document, Error> {
        match &self.part(name)?.content {
            Content::Xml(doc) => Ok((**doc).clone()),
            _ => Document::parse(&self.read(name)?).map_err(|e| Error::new(format!("{name}: {e}"))),
        }
    }

    /// Part `name`, parsed, to change: it is saved from the tree.
    pub fn xml_mut(&mut self, name: &str) -> Result<&mut Document, Error> {
        let i = *self.index.get(name).ok_or_else(|| Error::new(format!("{name} がありません")))?;
        if !matches!(self.parts[i].content, Content::Xml(_)) {
            let doc = self.xml(name)?;
            self.parts[i].content = Content::Xml(Box::new(doc));
        }
        match &mut self.parts[i].content {
            Content::Xml(doc) => Ok(doc),
            _ => unreachable!("just parsed"),
        }
    }

    /// Whether part `name` is changed (or new) since the package was opened.
    pub fn is_changed(&self, name: &str) -> bool {
        self.part(name).is_ok_and(|p| !matches!(p.content, Content::Original(_)))
    }

    /// Set the bytes of part `name`, adding it (at the end) if new. Its
    /// content type is not set here; see [`set_override`](Self::set_override).
    pub fn set_bytes(&mut self, name: &str, bytes: impl Into<Arc<[u8]>>) {
        let content = Content::Bytes(bytes.into());
        match self.index.get(name) {
            Some(&i) => self.parts[i].content = content,
            None => {
                self.index.insert(name.to_string(), self.parts.len());
                self.parts.push(Part { name: name.to_string(), meta: EntryMeta::new(), content });
            }
        }
    }

    /// Add a new XML part.
    pub fn set_xml(&mut self, name: &str, doc: Document) {
        self.set_bytes(name, Vec::new());
        let i = self.index[name];
        self.parts[i].content = Content::Xml(Box::new(doc));
    }

    /// Remove part `name` (and nothing else: its relationships part, content
    /// type override and the relationships to it are the caller's).
    pub fn remove(&mut self, name: &str) {
        if let Some(i) = self.index.remove(name) {
            self.parts.remove(i);
            for v in self.index.values_mut() {
                if *v > i {
                    *v -= 1;
                }
            }
        }
    }

    // ----- Content types --------------------------------------------------

    /// The content type of part `name`: its override, or the default of its extension.
    pub fn content_type(&self, name: &str) -> Result<Option<String>, Error> {
        let doc = self.xml(CONTENT_TYPES)?;
        let Some(root) = doc.root_element() else { return Ok(None) };
        let part_name = format!("/{name}");
        for o in doc.children_named(root, "Override") {
            if doc.attr(o, "PartName").is_some_and(|p| p.eq_ignore_ascii_case(&part_name)) {
                return Ok(doc.attr(o, "ContentType").map(str::to_string));
            }
        }
        let extension = name.rsplit_once('.').map_or("", |(_, e)| e);
        for d in doc.children_named(root, "Default") {
            if doc.attr(d, "Extension").is_some_and(|e| e.eq_ignore_ascii_case(extension)) {
                return Ok(doc.attr(d, "ContentType").map(str::to_string));
            }
        }
        Ok(None)
    }

    /// Give part `name` the content type `content_type` with an `Override`.
    pub fn set_override(&mut self, name: &str, content_type: &str) -> Result<(), Error> {
        let part_name = format!("/{name}");
        let doc = self.xml_mut(CONTENT_TYPES)?;
        let root = doc.root_element().ok_or_else(|| Error::new("[Content_Types].xml が空です"))?;
        let existing = doc.children_named(root, "Override").find(|&o| doc.attr(o, "PartName").is_some_and(|p| p.eq_ignore_ascii_case(&part_name)));
        let node = match existing {
            Some(o) => o,
            None => {
                let prefix = element_prefix(doc, root, CONTENT_TYPES_NS);
                let o = doc.append_element(root, &format!("{prefix}Override"));
                doc.set_attr(o, "PartName", &part_name);
                o
            }
        };
        doc.set_attr(node, "ContentType", content_type);
        Ok(())
    }

    /// Remove the `Override` of part `name`, if it has one.
    pub fn remove_override(&mut self, name: &str) -> Result<(), Error> {
        let part_name = format!("/{name}");
        let doc = self.xml_mut(CONTENT_TYPES)?;
        let Some(root) = doc.root_element() else { return Ok(()) };
        let found: Vec<NodeId> = doc.children_named(root, "Override").filter(|&o| doc.attr(o, "PartName").is_some_and(|p| p.eq_ignore_ascii_case(&part_name))).collect();
        for o in found {
            doc.detach(o);
        }
        Ok(())
    }

    /// Remove the `Default` of extension `extension` (no part name, no case), if it has one.
    pub fn remove_default(&mut self, extension: &str) -> Result<(), Error> {
        let doc = self.xml_mut(CONTENT_TYPES)?;
        let Some(root) = doc.root_element() else { return Ok(()) };
        let found: Vec<NodeId> = doc.children_named(root, "Default").filter(|&d| doc.attr(d, "Extension").is_some_and(|e| e.eq_ignore_ascii_case(extension))).collect();
        for d in found {
            doc.detach(d);
        }
        Ok(())
    }

    // ----- Relationships --------------------------------------------------

    /// The relationships of `source` ("" for the package), in document order.
    pub fn relationships(&self, source: &str) -> Result<Vec<Relationship>, Error> {
        let rels = relationships_part(source);
        if !self.contains(&rels) {
            return Ok(Vec::new());
        }
        let doc = self.xml(&rels)?;
        let Some(root) = doc.root_element() else { return Ok(Vec::new()) };
        Ok(doc
            .children_named(root, "Relationship")
            .filter_map(|r| {
                Some(Relationship {
                    id: doc.attr(r, "Id")?.to_string(),
                    kind: doc.attr(r, "Type")?.to_string(),
                    target: doc.attr(r, "Target")?.to_string(),
                    external: doc.attr(r, "TargetMode") == Some("External"),
                })
            })
            .collect())
    }

    /// Add a relationship from `source` to `target` (written as given,
    /// relative to `source`), returning its new id (`rIdN`, unused so far).
    /// The relationships part is made if there is none.
    pub fn add_relationship(&mut self, source: &str, kind: &str, target: &str, external: bool) -> Result<String, Error> {
        let rels = relationships_part(source);
        if !self.contains(&rels) {
            let mut doc = Document::new_with_declaration();
            let root = doc.append_element(Document::ROOT, "Relationships");
            doc.set_attr(root, "xmlns", RELATIONSHIPS_NS);
            self.set_xml(&rels, doc);
            let extension_default = self.content_type(&rels)?;
            if extension_default.is_none() {
                self.set_override(&rels, "application/vnd.openxmlformats-package.relationships+xml")?;
            }
        }
        let used: std::collections::HashSet<String> = self.relationships(source)?.into_iter().map(|r| r.id).collect();
        let id = (1..).map(|n| format!("rId{n}")).find(|id| !used.contains(id)).expect("an unused id");
        let doc = self.xml_mut(&rels)?;
        let root = doc.root_element().ok_or_else(|| Error::new(format!("{rels} が空です")))?;
        let prefix = element_prefix(doc, root, RELATIONSHIPS_NS);
        let r = doc.append_element(root, &format!("{prefix}Relationship"));
        doc.set_attr(r, "Id", &id);
        doc.set_attr(r, "Type", kind);
        doc.set_attr(r, "Target", target);
        if external {
            doc.set_attr(r, "TargetMode", "External");
        }
        Ok(id)
    }

    /// Remove relationship `id` of `source`.
    pub fn remove_relationship(&mut self, source: &str, id: &str) -> Result<(), Error> {
        let rels = relationships_part(source);
        if !self.contains(&rels) {
            return Ok(());
        }
        let doc = self.xml_mut(&rels)?;
        let Some(root) = doc.root_element() else { return Ok(()) };
        let found: Vec<NodeId> = doc.children_named(root, "Relationship").filter(|&r| doc.attr(r, "Id") == Some(id)).collect();
        for r in found {
            doc.detach(r);
        }
        Ok(())
    }

    // ----- Saving ---------------------------------------------------------

    /// The package as a ZIP archive.
    pub fn save(&self, options: SaveOptions) -> Result<Vec<u8>, Error> {
        let mut writer = Writer::new();
        writer.set_comment(self.archive.comment());
        for part in &self.parts {
            match &part.content {
                Content::Original(i) if options.regenerate_xml && is_xml_name(&part.name) => {
                    let entry = self.entry(*i);
                    let bytes = self.archive.read(entry)?;
                    match Document::parse(&bytes) {
                        Ok(doc) => writer.add(&part.name, part.meta, &doc.to_bytes_regenerated())?,
                        // Not well-formed (Excel's VML is not always): kept as it was.
                        Err(_) => writer.copy(&self.archive, entry)?,
                    }
                }
                Content::Original(i) => writer.copy(&self.archive, self.entry(*i))?,
                Content::Bytes(bytes) => writer.add(&part.name, part.meta, bytes)?,
                Content::Xml(doc) => {
                    let bytes = if options.regenerate_xml { doc.to_bytes_regenerated() } else { doc.to_bytes() };
                    writer.add(&part.name, part.meta, &bytes)?;
                }
            }
        }
        writer.finish()
    }
}

/// Whether a part with this name holds XML (by extension; content types
/// such as `…+xml` are always so named in Office files).
pub fn is_xml_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".xml") || lower.ends_with(".rels") || lower.ends_with(".vml")
}

/// The prefix (with its colon, or "") to name a new child of `parent` in
/// `namespace`.
fn element_prefix(doc: &Document, parent: NodeId, namespace: &str) -> String {
    match doc.lookup_prefix(parent, namespace) {
        Some(p) if p.is_empty() => String::new(),
        Some(p) => format!("{p}:"),
        None => String::new(),
    }
}

/// The relationships part of `source` ("" for the package's own).
pub fn relationships_part(source: &str) -> String {
    match source.rfind('/') {
        Some(i) => format!("{}/_rels/{}.rels", &source[..i], &source[i + 1..]),
        None => format!("_rels/{source}.rels"),
    }
}

fn dir_of(part: &str) -> &str {
    part.rfind('/').map_or("", |i| &part[..i])
}

/// Resolve a relationship target against the directory of its source part.
pub fn resolve(dir: &str, target: &str) -> String {
    let target = target.split(['#', '?']).next().unwrap_or(target);
    let mut segments: Vec<&str> = Vec::new();
    let joined;
    let path = if let Some(absolute) = target.strip_prefix('/') {
        absolute
    } else if dir.is_empty() {
        target
    } else {
        joined = format!("{dir}/{target}");
        &joined
    };
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            s => segments.push(s),
        }
    }
    segments.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<u8> {
        let mut w = Writer::new();
        w.add(CONTENT_TYPES, EntryMeta::new(), br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="doc"/></Types>"#).unwrap();
        w.add("_rels/.rels", EntryMeta::new(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="t/officeDocument" Target="word/document.xml"/></Relationships>"#).unwrap();
        w.add("word/document.xml", EntryMeta { time: 7, ..EntryMeta::new() }, b"<w:document xmlns:w=\"urn:w\"><w:body><w:t>a</w:t></w:body></w:document>").unwrap();
        w.add("word/media/image1.png", EntryMeta::new(), b"\x89PNG....").unwrap();
        w.finish().unwrap()
    }

    #[test]
    fn untouched_package_saves_identical() {
        let data = sample();
        let package = Package::open(data.clone()).unwrap();
        assert_eq!(package.save(SaveOptions::default()).unwrap(), data);
    }

    #[test]
    fn regenerated_package_keeps_every_part() {
        let data = sample();
        let package = Package::open(data).unwrap();
        let again = Package::open(package.save(SaveOptions { regenerate_xml: true }).unwrap()).unwrap();
        assert_eq!(again.names().collect::<Vec<_>>(), package.names().collect::<Vec<_>>());
        assert_eq!(again.read("word/media/image1.png").unwrap(), b"\x89PNG....");
    }

    #[test]
    fn changed_part_is_written_and_others_copied() {
        let mut package = Package::open(sample()).unwrap();
        let doc = package.xml_mut("word/document.xml").unwrap();
        let t = doc.descendants(Document::ROOT).into_iter().find(|&n| doc.local_name(n) == "t").unwrap();
        doc.set_text(t, "b");
        let saved = Package::open(package.save(SaveOptions::default()).unwrap()).unwrap();
        assert_eq!(saved.read("word/document.xml").unwrap(), b"<w:document xmlns:w=\"urn:w\"><w:body><w:t>b</w:t></w:body></w:document>");
        assert!(!saved.is_changed("word/document.xml"));
    }

    #[test]
    fn content_types_and_relationships() {
        let mut package = Package::open(sample()).unwrap();
        assert_eq!(package.content_type("word/document.xml").unwrap().as_deref(), Some("doc"));
        assert_eq!(package.content_type("word/styles.xml").unwrap().as_deref(), Some("application/xml"));
        package.set_bytes("word/styles.xml", &b"<w:styles xmlns:w=\"urn:w\"/>"[..]);
        package.set_override("word/styles.xml", "styles").unwrap();
        let id = package.add_relationship("word/document.xml", "t/styles", "styles.xml", false).unwrap();
        assert_eq!(id, "rId1");
        let rels = package.relationships("word/document.xml").unwrap();
        assert_eq!(rels[0].target_part("word/document.xml").as_deref(), Some("word/styles.xml"));
        let saved = Package::open(package.save(SaveOptions::default()).unwrap()).unwrap();
        assert_eq!(saved.content_type("word/styles.xml").unwrap().as_deref(), Some("styles"));
        assert_eq!(saved.relationships("word/document.xml").unwrap().len(), 1);
        let id2 = package.add_relationship("", "t/x", "x.xml", false).unwrap();
        assert_eq!(id2, "rId2");
    }

    #[test]
    fn resolves_targets() {
        assert_eq!(resolve("ppt/slides", "../media/image1.png"), "ppt/media/image1.png");
        assert_eq!(resolve("", "ppt/presentation.xml"), "ppt/presentation.xml");
        assert_eq!(resolve("ppt/slides", "/ppt/theme/theme1.xml"), "ppt/theme/theme1.xml");
        assert_eq!(relationships_part("ppt/slides/slide1.xml"), "ppt/slides/_rels/slide1.xml.rels");
        assert_eq!(relationships_part(""), "_rels/.rels");
    }
}
