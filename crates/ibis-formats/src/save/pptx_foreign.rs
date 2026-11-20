// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スライドの貼り付け from another file (貼り付け先のテーマを使用): the
//! slide part copied from the other package with what it holds (pictures,
//! media, charts and their workbooks, SmartArt parts…, each copied and
//! related anew, the slide's relationship ids renumbered), following a
//! layout of this presentation. Its notes page and comments stay behind
//! (the notes are written from the model).

use std::collections::HashMap;
use std::sync::Arc;

use ibis_ooxml::{NodeId, Package, XmlDocument};

use super::pptx::{next_part_name, relative};
use crate::Error;

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_SLIDE_LAYOUT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
/// The slide's `p:ext` naming its threaded comments.
const EXT_COMMENTS: &str = "{6950BFC3-D8DA-4A85-94F7-54DA5524770B}";

/// A slide of another file to paste: that file (as saved when the slide
/// was copied) and the slide's part in it; `keep_source` brings its master
/// (with its layouts and theme) along (元の書式を保持), else it follows a
/// layout of this file (貼り付け先のテーマを使用).
#[derive(Clone, Debug)]
pub struct ForeignSlide {
    pub data: Arc<[u8]>,
    pub part: String,
    pub keep_source: bool,
}

/// Relationships not carried over: the layout (this presentation's is
/// used), the notes page, comments, and customer data (customXml).
fn left_behind(kind: &str) -> bool {
    let name = kind.rsplit('/').next().unwrap_or("");
    matches!(name, "slideLayout" | "notesSlide" | "comments" | "commentAuthors" | "authors" | "customXml" | "customXmlProps")
}

/// Relationships that should never be copied across parts (package-level customer data, comments).
fn unneeded_rel(kind: &str) -> bool {
    let name = kind.rsplit('/').next().unwrap_or("");
    matches!(name, "comments" | "commentAuthors" | "authors" | "customXml" | "customXmlProps")
}

/// Copy part `from` of `src` into `dest` (a new name like it), with the
/// parts it relates to (not those left behind), once each (`copies`).
fn copy_across(src: &Package, from: &str, dest: &mut Package, copies: &mut HashMap<String, String>) -> Result<String, Error> {
    if let Some(done) = copies.get(from) {
        return Ok(done.clone());
    }
    let to = next_part_name(dest, from);
    copies.insert(from.to_string(), to.clone());
    dest.set_bytes(&to, src.read(from)?);
    if let Some(ct) = src.content_type(from)? {
        dest.set_override(&to, &ct)?;
    }
    let mut renamed: HashMap<String, String> = HashMap::new();
    for rel in src.relationships(from)? {
        if unneeded_rel(&rel.kind) {
            continue;
        }
        let new_id = if rel.external {
            dest.add_relationship(&to, &rel.kind, &rel.target, true)?
        } else {
            let Some(target) = rel.target_part(from) else { continue };
            if !src.contains(&target) {
                continue;
            }
            let copied = copy_across(src, &target, dest, copies)?;
            dest.add_relationship(&to, &rel.kind, &relative(&to, &copied), false)?
        };
        renamed.insert(rel.id.clone(), new_id);
    }
    // An XML part names its relationships by id: the new ones.
    if ibis_ooxml::package::is_xml_name(&to) {
        let mut doc = dest.xml(&to)?;
        if !renamed.is_empty() {
            rename_ids(&mut doc, &renamed);
        }
        strip_cust_data(&mut doc);
        dest.set_xml(&to, doc);
    }
    Ok(to)
}

/// Remove customer data (`custDataLst`) elements whose `customXml` relationships are not copied.
fn strip_cust_data(doc: &mut XmlDocument) {
    let Some(root) = doc.root_element() else { return };
    let cust_nodes: Vec<NodeId> = doc
        .descendants(root)
        .into_iter()
        .filter(|&e| doc.is_element(e) && doc.name(e).split_once(':').map_or(doc.name(e), |(_, n)| n) == "custDataLst")
        .collect();
    for e in cust_nodes {
        doc.detach(e);
    }
}

/// Every relationship id (`r:id`, `r:embed`, `r:dm`, …) in `doc` renamed.
fn rename_ids(doc: &mut XmlDocument, renamed: &HashMap<String, String>) {
    let Some(root) = doc.root_element() else { return };
    for node in doc.descendants(root) {
        if !doc.is_element(node) {
            continue;
        }
        let changes: Vec<(String, String)> = doc
            .attributes(node)
            .iter()
            .filter(|a| a.name.split_once(':').is_some_and(|(p, _)| p != "xmlns" && doc.resolve_prefix(node, Some(p)) == Some(NS_R)))
            .filter_map(|a| renamed.get(&a.value).map(|n| (a.name.clone(), n.clone())))
            .collect();
        for (name, value) in changes {
            doc.set_attr(node, &name, &value);
        }
    }
}

const REL_SLIDE_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";

/// The first related part of `kind` from `part` in `src`.
fn related(src: &Package, part: &str, kind: &str) -> Result<Option<String>, Error> {
    Ok(src.relationships(part)?.into_iter().find(|r| r.kind == kind && !r.external).and_then(|r| r.target_part(part)))
}

/// The ids of slide masters and layouts in use in `dest` (they share one
/// range, from 2 147 483 648).
pub(crate) fn used_master_ids(dest: &Package, presentation: &str) -> Result<u64, Error> {
    let mut max = 2_147_483_647u64;
    let mut look = |doc: &XmlDocument, list: &str| {
        if let Some(root) = doc.root_element()
            && let Some(l) = doc.child(root, list)
        {
            for e in doc.elements(l) {
                if let Some(v) = doc.attr(e, "id").and_then(|v| v.parse::<u64>().ok()) {
                    max = max.max(v);
                }
            }
        }
    };
    look(&dest.xml(presentation)?, "sldMasterIdLst");
    for rel in dest.relationships(presentation)?.into_iter().filter(|r| r.kind == REL_SLIDE_MASTER) {
        if let Some(master) = rel.target_part(presentation)
            && dest.contains(&master)
        {
            look(&dest.xml(&master)?, "sldLayoutIdLst");
        }
    }
    Ok(max)
}

/// The slide's master copied from `src` (its layouts and theme with it)
/// and added to the presentation's masters; returns the copied parts.
fn copy_master(src: &Package, master: &str, dest: &mut Package, presentation: &str, copies: &mut HashMap<String, String>) -> Result<(), Error> {
    let mut next = used_master_ids(dest, presentation)? + 1;
    let copied = copy_across(src, master, dest, copies)?;
    // Its layouts' ids, unique here.
    let mut doc = dest.xml(&copied)?;
    if let Some(root) = doc.root_element()
        && let Some(list) = doc.child(root, "sldLayoutIdLst")
    {
        let master_id = next;
        next += 1;
        for e in doc.elements(list).collect::<Vec<_>>() {
            doc.set_attr(e, "id", &next.to_string());
            next += 1;
        }
        dest.set_xml(&copied, doc);
        let rel = dest.add_relationship(presentation, REL_SLIDE_MASTER, &relative(presentation, &copied), false)?;
        let pdoc = dest.xml_mut(presentation)?;
        let proot = pdoc.root_element().ok_or_else(|| Error::new("プレゼンテーションが空です"))?;
        let list = pdoc.child(proot, "sldMasterIdLst").ok_or_else(|| crate::save::unsupported("マスターの一覧の無いプレゼンテーション"))?;
        let x = super::pptx_new::Prefixes::at(pdoc, list);
        let entry = x.import(pdoc, &format!("<{p}sldMasterId id=\"{master_id}\" {r}id=\"{rel}\"/>", p = x.p, r = x.r))?;
        let end = pdoc.children(list).len();
        pdoc.insert(list, end, entry);
    }
    Ok(())
}

/// Slide part `part` of `dest` (presentation part `presentation`) made from
/// `foreign`: following layout part `layout` of `dest`, or (keeping its
/// formatting) its own layout, copied with its master.
pub(crate) fn foreign_slide_part(dest: &mut Package, presentation: &str, layout: Option<&str>, part: &str, foreign: &ForeignSlide) -> Result<(), Error> {
    let src = Package::open(foreign.data.clone())?;
    let mut doc = src.xml(&foreign.part)?;
    let mut copies: HashMap<String, String> = HashMap::new();
    let layout = if foreign.keep_source {
        let src_layout = related(&src, &foreign.part, REL_SLIDE_LAYOUT)?.ok_or_else(|| crate::save::unsupported("レイアウトの無いスライドの貼り付け"))?;
        let src_master = related(&src, &src_layout, REL_SLIDE_MASTER)?.ok_or_else(|| crate::save::unsupported("マスターの無いレイアウトの貼り付け"))?;
        copy_master(&src, &src_master, dest, presentation, &mut copies)?;
        copies.get(&src_layout).cloned().ok_or_else(|| crate::save::unsupported("マスターに無いレイアウトの貼り付け"))?
    } else {
        layout.ok_or_else(|| Error::new("貼り付け先のレイアウトがありません"))?.to_string()
    };
    let mut renamed: HashMap<String, String> = HashMap::new();
    // The part first (empty), for its relationships.
    dest.set_xml(part, XmlDocument::parse(b"<p:sld xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"/>").map_err(|e| Error::new(format!("スライドを貼り付けられません: {e}")))?);
    for rel in src.relationships(&foreign.part)? {
        if left_behind(&rel.kind) {
            continue;
        }
        let new_id = if rel.external {
            dest.add_relationship(part, &rel.kind, &rel.target, true)?
        } else {
            let Some(target) = rel.target_part(&foreign.part) else { continue };
            if !src.contains(&target) {
                continue;
            }
            let copied = copy_across(&src, &target, dest, &mut copies)?;
            dest.add_relationship(part, &rel.kind, &relative(part, &copied), false)?
        };
        renamed.insert(rel.id.clone(), new_id);
    }
    dest.add_relationship(part, REL_SLIDE_LAYOUT, &relative(part, &layout), false)?;
    rename_ids(&mut doc, &renamed);
    strip_cust_data(&mut doc);
    // The threaded comments' reference goes with them.
    if let Some(root) = doc.root_element()
        && let Some(list) = doc.child(root, "extLst")
    {
        let gone: Vec<NodeId> = doc.elements(list).filter(|&e| doc.attr(e, "uri") == Some(EXT_COMMENTS)).collect();
        for e in gone {
            doc.detach(e);
        }
        if doc.elements(list).next().is_none() {
            doc.detach(list);
        }
    }
    dest.set_xml(part, doc);
    Ok(())
}
