// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! OPC packages: parts and the relationships between them.

use std::collections::HashMap;

use crate::Error;
use crate::zip::Archive;

pub(crate) const REL_OFFICE_DOCUMENT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
pub(crate) const REL_SLIDE_LAYOUT: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
pub(crate) const REL_SLIDE_MASTER: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
pub(crate) const REL_CHART_USER_SHAPES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartUserShapes";
pub(crate) const REL_THEME: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
pub(crate) const REL_THEME_OVERRIDE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/themeOverride";
pub(crate) const REL_NOTES_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster";
pub(crate) const REL_NOTES_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide";
pub(crate) const REL_TABLE_STYLES: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/tableStyles";
pub(crate) const REL_THREADED_COMMENTS: &str = "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment";
pub(crate) const REL_PERSONS: &str = "http://schemas.microsoft.com/office/2017/10/relationships/person";
pub(crate) const REL_COMMENTS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";

#[derive(Clone, Debug)]
pub(crate) struct Relationship {
    pub(crate) kind: String,
    /// Part name inside the package (no leading slash), or the URL of an
    /// external target.
    pub(crate) target: String,
    pub(crate) external: bool,
}

pub(crate) struct Package<'a> {
    archive: Archive<'a>,
}

impl<'a> Package<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Result<Self, Error> {
        let archive = Archive::new(data)?;
        if !archive.contains("[Content_Types].xml") {
            return Err(Error::new("Office のファイルではありません（[Content_Types].xml がありません）"));
        }
        Ok(Self { archive })
    }

    pub(crate) fn read(&self, part: &str) -> Result<Vec<u8>, Error> {
        self.archive.read(part)?.ok_or_else(|| Error::new(format!("{part} がありません")))
    }

    pub(crate) fn read_xml(&self, part: &str) -> Result<String, Error> {
        let bytes = self.read(part)?;
        // Parts are UTF-8; strip a byte order mark if present.
        let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&bytes);
        String::from_utf8(bytes.to_vec()).map_err(|_| Error::new(format!("{part} が UTF-8 ではありません")))
    }

    /// Relationships of `part` ("" for the package itself), by id.
    pub(crate) fn relationships(&self, part: &str) -> Result<HashMap<String, Relationship>, Error> {
        let (dir, file) = split_part(part);
        let rels_part = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
        let Some(bytes) = self.archive.read(&rels_part)? else { return Ok(HashMap::new()) };
        let xml = String::from_utf8_lossy(&bytes).into_owned();
        let doc = roxmltree::Document::parse(&xml).map_err(|e| Error::xml(&rels_part, e))?;
        let mut rels = HashMap::new();
        for node in doc.root_element().children().filter(|n| n.has_tag_name("Relationship")) {
            let (Some(id), Some(kind), Some(target)) =
                (node.attribute("Id"), node.attribute("Type"), node.attribute("Target"))
            else {
                continue;
            };
            let external = node.attribute("TargetMode") == Some("External");
            let target = if external { target.to_string() } else { resolve(dir, target) };
            rels.insert(id.to_string(), Relationship { kind: kind.to_string(), target, external });
        }
        Ok(rels)
    }

    /// The target of the first relationship of `kind` from `part`.
    pub(crate) fn related(&self, part: &str, kind: &str) -> Result<Option<String>, Error> {
        let rels = self.relationships(part)?;
        let mut targets: Vec<_> = rels.into_iter().filter(|(_, r)| r.kind == kind && !r.external).collect();
        targets.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(targets.into_iter().next().map(|(_, r)| r.target))
    }
}

fn split_part(part: &str) -> (&str, &str) {
    match part.rfind('/') {
        Some(i) => (&part[..i], &part[i + 1..]),
        None => ("", part),
    }
}

/// Resolve a relationship target against the directory of its source part.
fn resolve(dir: &str, target: &str) -> String {
    let mut segments: Vec<&str> = if let Some(absolute) = target.strip_prefix('/') {
        return normalize(absolute.split('/'));
    } else if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    segments.extend(target.split('/'));
    normalize(segments.into_iter())
}

fn normalize<'s>(segments: impl Iterator<Item = &'s str>) -> String {
    let mut out: Vec<&str> = Vec::new();
    for segment in segments {
        match segment {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// Read core and extended document properties (`docProps/core.xml`, `docProps/app.xml`).
pub(crate) fn read_doc_properties(package: &Package) -> ibis_doc_model::DocProperties {
    let mut properties = ibis_doc_model::DocProperties::default();
    let root_rels = package.relationships("").unwrap_or_default();
    let core_part = root_rels
        .values()
        .find(|r| !r.external && r.kind.ends_with("/metadata/core-properties"))
        .map(|r| r.target.clone())
        .unwrap_or_else(|| "docProps/core.xml".to_string());
    if let Ok(xml) = package.read_xml(&core_part) {
        if let Ok(doc) = roxmltree::Document::parse(&xml) {
            for node in doc.root_element().descendants().filter(|n| n.is_element()) {
                let val = node.text().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                match node.tag_name().name() {
                    "title" => properties.title = val,
                    "subject" => properties.subject = val,
                    "creator" => properties.creator = val,
                    "keywords" => properties.keywords = val,
                    "description" => properties.description = val,
                    "lastModifiedBy" => properties.last_modified_by = val,
                    "revision" => properties.revision = val,
                    "category" => properties.category = val,
                    "created" => properties.created = val,
                    "modified" => properties.modified = val,
                    "lastPrinted" => properties.last_printed = val,
                    _ => {}
                }
            }
        }
    }
    let app_part = root_rels
        .values()
        .find(|r| !r.external && r.kind.ends_with("/extended-properties"))
        .map(|r| r.target.clone())
        .unwrap_or_else(|| "docProps/app.xml".to_string());
    if let Ok(xml) = package.read_xml(&app_part) {
        if let Ok(doc) = roxmltree::Document::parse(&xml) {
            for node in doc.root_element().descendants().filter(|n| n.is_element()) {
                let val = node.text().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                match node.tag_name().name() {
                    "Company" => properties.company = val,
                    "Manager" => properties.manager = val,
                    _ => {}
                }
            }
        }
    }
    let custom_part = root_rels
        .values()
        .find(|r| !r.external && r.kind.ends_with("/custom-properties"))
        .map(|r| r.target.clone())
        .unwrap_or_else(|| "docProps/custom.xml".to_string());
    if let Ok(xml) = package.read_xml(&custom_part) {
        if let Ok(doc) = roxmltree::Document::parse(&xml) {
            for node in doc.root_element().children().filter(|n| n.is_element() && n.tag_name().name() == "property") {
                if let Some(name) = node.attribute("name") {
                    if let Some(val_node) = node.children().find(|n| n.is_element()) {
                        let text = val_node.text().unwrap_or("").trim().to_string();
                        let value = match val_node.tag_name().name() {
                            "bool" => ibis_doc_model::CustomPropertyValue::Bool(text == "true" || text == "1"),
                            "r8" | "i4" | "int" | "r4" | "ui4" | "i8" | "ui8" => ibis_doc_model::CustomPropertyValue::Number(text),
                            "filetime" | "date" => ibis_doc_model::CustomPropertyValue::DateTime(text),
                            _ => ibis_doc_model::CustomPropertyValue::Text(text),
                        };
                        properties.custom.push(ibis_doc_model::CustomProperty {
                            name: name.to_string(),
                            value,
                        });
                    }
                }
            }
        }
    }
    properties
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_targets() {
        assert_eq!(resolve("ppt/slides", "../media/image1.png"), "ppt/media/image1.png");
        assert_eq!(resolve("ppt", "slides/slide1.xml"), "ppt/slides/slide1.xml");
        assert_eq!(resolve("", "ppt/presentation.xml"), "ppt/presentation.xml");
        assert_eq!(resolve("ppt/slides", "/ppt/theme/theme1.xml"), "ppt/theme/theme1.xml");
    }
}
