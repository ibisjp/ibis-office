// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スライド マスター表示: a slide master or a layout edited as if it were a
//! slide. `build` makes, from a presentation's file, another one holding a
//! single slide whose shapes are those of the master (or layout), which the
//! ordinary editor opens; `apply` puts the shapes of that slide, as edited,
//! back into the master's (or layout's) part of the file, with the pictures
//! and links they use. What else the part holds (the background, the
//! text styles, the transition, the other layouts, the theme) is left as it was.

use ibis_ooxml::{NodeId, Package, SaveOptions, XmlDocument};

use crate::Error;
use crate::save::pptx::{next_part_name, relative};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_OFFICE_DOCUMENT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const REL_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
const REL_SLIDE_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
const REL_SLIDE_LAYOUT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
const REL_THEME: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";
const CT_SLIDE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";

/// A page of the master view: a slide master, or one of its layouts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    /// The part of the file.
    pub part: String,
    /// The layout's name, or the master's (its theme's).
    pub name: String,
    pub master: bool,
    /// The master it belongs to (a master's own part).
    pub of: String,
}

fn err(what: impl Into<String>) -> Error {
    Error::new(what)
}

fn main_part(package: &Package) -> Result<String, Error> {
    package.relationships("")?.into_iter().find(|r| r.kind == REL_OFFICE_DOCUMENT && !r.external).and_then(|r| r.target_part("")).ok_or_else(|| err("プレゼンテーションの本体がありません"))
}

fn rels_part(part: &str) -> String {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") }
}

/// The layouts of master `master`, in its order.
fn layouts_of(package: &Package, master: &str) -> Result<Vec<String>, Error> {
    let rels = package.relationships(master)?;
    let doc = package.xml(master)?;
    let root = doc.root_element().ok_or_else(|| err("マスターが空です"))?;
    let Some(list) = doc.child(root, "sldLayoutIdLst") else { return Ok(Vec::new()) };
    Ok(doc
        .elements(list)
        .filter_map(|e| rel_id(&doc, e))
        .filter_map(|rid| rels.iter().find(|r| r.id == rid && r.kind == REL_SLIDE_LAYOUT).and_then(|r| r.target_part(master)))
        .filter(|p| package.contains(p))
        .collect())
}

/// The value of the `r:id` of `node`, whatever prefix the relationships' namespace has.
fn rel_id(doc: &XmlDocument, node: NodeId) -> Option<String> {
    doc.attributes(node).iter().find(|a| a.name.rsplit_once(':').is_some_and(|(p, l)| l == "id" && doc.resolve_prefix(node, Some(p)) == Some(NS_R))).map(|a| a.value.clone())
}

/// The master pages of `data`: each master, then its layouts.
pub fn pages(data: &[u8]) -> Result<Vec<Page>, Error> {
    let package = Package::open(data.to_vec())?;
    let main = main_part(&package)?;
    let mut out = Vec::new();
    for rel in package.relationships(&main)?.into_iter().filter(|r| r.kind == REL_SLIDE_MASTER && !r.external) {
        let Some(master) = rel.target_part(&main).filter(|p| package.contains(p)) else { continue };
        let name = package
            .relationships(&master)?
            .into_iter()
            .find(|r| r.kind == REL_THEME && !r.external)
            .and_then(|r| r.target_part(&master))
            .and_then(|t| package.xml(&t).ok())
            .and_then(|d| d.root_element().and_then(|r| d.attr(r, "name").map(str::to_string)))
            .unwrap_or_default();
        out.push(Page { part: master.clone(), name, master: true, of: master.clone() });
        for layout in layouts_of(&package, &master)? {
            let doc = package.xml(&layout)?;
            let name = doc.root_element().and_then(|r| doc.child(r, "cSld")).and_then(|c| doc.attr(c, "name")).unwrap_or("").to_string();
            out.push(Page { part: layout, name, master: false, of: master.clone() });
        }
    }
    Ok(out)
}

/// The theme part the master (or the layout's master) `part` uses.
fn theme_part_of(package: &Package, part: &str) -> Result<Option<String>, Error> {
    let own = |p: &str| -> Result<Option<String>, Error> { Ok(package.relationships(p)?.into_iter().find(|r| r.kind == REL_THEME && !r.external).and_then(|r| r.target_part(p)).filter(|t| package.contains(t))) };
    if let Some(theme) = own(part)? {
        return Ok(Some(theme));
    }
    match package.relationships(part)?.into_iter().find(|r| r.kind == REL_SLIDE_MASTER && !r.external).and_then(|r| r.target_part(part)) {
        Some(master) => own(&master),
        None => Ok(None),
    }
}

/// The tree of shapes of `doc` (`p:cSld/p:spTree`).
fn tree(doc: &XmlDocument) -> Option<NodeId> {
    let root = doc.root_element()?;
    doc.path(root, &["cSld", "spTree"])
}

/// What of a shape tree is not a shape: its own properties (`nvGrpSpPr`, `grpSpPr`) and extensions.
fn is_tree_property(doc: &XmlDocument, node: NodeId) -> bool {
    matches!(doc.local_name(node), "nvGrpSpPr" | "grpSpPr" | "extLst")
}

/// The shapes of tree `tree` taken out (the elements that are not its properties).
fn empty_tree(doc: &mut XmlDocument, tree: NodeId) {
    for child in doc.children(tree).to_vec() {
        if doc.is_element(child) && !is_tree_property(doc, child) {
            doc.detach(child);
        }
    }
}

/// A file like `data` with one slide: the master (or layout) `part` as a slide
/// of its own, the shapes of the part and of the layout it shows over taken out so
/// that they are not drawn twice.
pub fn build(data: &[u8], part: &str) -> Result<Vec<u8>, Error> {
    let mut package = Package::open(data.to_vec())?;
    let main = main_part(&package)?;
    let root_name = {
        let doc = package.xml(part)?;
        let root = doc.root_element().ok_or_else(|| err("ページが空です"))?;
        doc.local_name(root).to_string()
    };
    let is_master = match root_name.as_str() {
        "sldMaster" => true,
        "sldLayout" => false,
        _ => return Err(err("マスターでもレイアウトでもありません")),
    };
    // The layout the slide follows: the layout itself, or a layout of the master.
    let (master, layout) = if is_master {
        let layout = layouts_of(&package, part)?.into_iter().next().ok_or_else(|| err("レイアウトの無いマスターです"))?;
        (part.to_string(), layout)
    } else {
        let master = package.relationships(part)?.into_iter().find(|r| r.kind == REL_SLIDE_MASTER && !r.external).and_then(|r| r.target_part(part)).ok_or_else(|| err("マスターの無いレイアウトです"))?;
        (master, part.to_string())
    };
    let _ = master;
    // The slide: the part's own tree, as a `p:sld`.
    let slide_part = next_part_name(&package, "ppt/slides/slide1.xml");
    let mut slide = package.xml(part)?;
    let slide_root = slide.root_element().ok_or_else(|| err("ページが空です"))?;
    let prefix = slide.prefix(slide_root).map(|p| format!("{p}:")).unwrap_or_default();
    slide.rename(slide_root, &format!("{prefix}sld"));
    for name in ["type", "preserve", "userDrawn", "matchingName", "showMasterSp"] {
        slide.remove_attr(slide_root, name);
    }
    // (A layout that does not show its master's shapes: its slide does not either.)
    for child in slide.children(slide_root).to_vec() {
        if slide.is_element(child) && matches!(slide.local_name(child), "clrMap" | "sldLayoutIdLst" | "hf" | "txStyles") {
            slide.detach(child);
        }
    }
    package.set_xml(&slide_part, slide);
    package.set_override(&slide_part, CT_SLIDE)?;
    // Its relationships: those of the part (the pictures, the links), and the layout.
    let rels = package.relationships(part)?;
    for rel in rels.iter().filter(|r| !matches!(r.kind.as_str(), k if k == REL_SLIDE_MASTER || k == REL_SLIDE_LAYOUT)) {
        // The same id, for what refers to it.
        let target = if rel.external { rel.target.clone() } else { match rel.target_part(part) { Some(t) => relative(&slide_part, &t), None => continue } };
        add_relationship_with_id(&mut package, &slide_part, &rel.id, &rel.kind, &target, rel.external)?;
    }
    package.add_relationship(&slide_part, REL_SLIDE_LAYOUT, &relative(&slide_part, &layout), false)?;
    // The part and the layout without their shapes.
    {
        let doc = package.xml_mut(part)?;
        if let Some(t) = tree(doc) {
            empty_tree(doc, t);
        }
    }
    if is_master {
        let doc = package.xml_mut(&layout)?;
        if let Some(t) = tree(doc) {
            empty_tree(doc, t);
        }
    }
    // The presentation: this slide only.
    let rid = package.add_relationship(&main, REL_SLIDE, &relative(&main, &slide_part), false)?;
    let doc = package.xml_mut(&main)?;
    let root = doc.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
    let list = doc.child(root, "sldIdLst").ok_or_else(|| err("スライドの一覧がありません"))?;
    for child in doc.children(list).to_vec() {
        doc.detach(child);
    }
    let x = crate::save::pptx_new::Prefixes::at(doc, list);
    let entry = x.import(doc, &format!("<{p}sldId id=\"256\" {r}id=\"{rid}\"/>", p = x.p, r = x.r))?;
    doc.insert(list, 0, entry);
    // Sections and custom shows name slides that are not there.
    if let Some(shows) = doc.child(root, "custShowLst") {
        doc.detach(shows);
    }
    if let Some(ext) = doc.child(root, "extLst") {
        for e in doc.elements(ext).collect::<Vec<_>>() {
            if doc.descendants(e).into_iter().any(|d| doc.is_element(d) && doc.local_name(d) == "sectionLst") {
                doc.detach(e);
            }
        }
    }
    Ok(package.save(SaveOptions::default())?)
}

/// A relationship of `source` with the id `id` (what refers to it keeps working).
fn add_relationship_with_id(package: &mut Package, source: &str, id: &str, kind: &str, target: &str, external: bool) -> Result<(), Error> {
    let rels = rels_part(source);
    if !package.contains(&rels) {
        let doc = XmlDocument::parse(b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"/>").map_err(|e| err(format!("関係を作れません: {e}")))?;
        package.set_xml(&rels, doc);
    }
    let doc = package.xml_mut(&rels)?;
    let root = doc.root_element().ok_or_else(|| err("関係が空です"))?;
    let rel = doc.append_element(root, "Relationship");
    doc.set_attr(rel, "Id", id);
    doc.set_attr(rel, "Type", kind);
    doc.set_attr(rel, "Target", target);
    if external {
        doc.set_attr(rel, "TargetMode", "External");
    }
    Ok(())
}

/// The slide part of a file made by `build` (its only slide), and the document of it.
fn only_slide(package: &Package) -> Result<(String, XmlDocument), Error> {
    let main = main_part(package)?;
    let main_doc = package.xml(&main)?;
    let root = main_doc.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
    let list = main_doc.child(root, "sldIdLst").ok_or_else(|| err("スライドがありません"))?;
    let first = main_doc.elements(list).next().ok_or_else(|| err("スライドがありません"))?;
    let rid = rel_id(&main_doc, first).ok_or_else(|| err("スライドの関係がありません"))?;
    let slide = package.relationships(&main)?.into_iter().find(|r| r.id == rid).and_then(|r| r.target_part(&main)).ok_or_else(|| err("スライドの部品がありません"))?;
    let doc = package.xml(&slide)?;
    Ok((slide, doc))
}

/// `data` with the shapes of master (or layout) `part` those of the slide of `edited` (a file
/// `build` made from `data`, then edited).
pub fn apply(data: &[u8], edited: &[u8], part: &str) -> Result<Vec<u8>, Error> {
    let mut package = Package::open(data.to_vec())?;
    let edited = Package::open(edited.to_vec())?;
    let (slide_part, slide_doc) = only_slide(&edited)?;
    let slide_tree = tree(&slide_doc).ok_or_else(|| err("スライドの図形の木がありません"))?;
    let shapes: Vec<NodeId> = slide_doc.children(slide_tree).iter().copied().filter(|&n| slide_doc.is_element(n) && !is_tree_property(&slide_doc, n)).collect();
    // The part's tree emptied, then given the shapes.
    let mut doc = package.xml(part)?;
    let tree_id = tree(&doc).ok_or_else(|| err("図形の木がありません"))?;
    empty_tree(&mut doc, tree_id);
    let slide_rels = edited.relationships(&slide_part)?;
    let mut mapped: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut added: Vec<(String, String, String, bool)> = Vec::new();
    let at = doc.children(tree_id).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "extLst").unwrap_or(doc.children(tree_id).len());
    // The background (背景の書式設定) as edited: the part's own, in the place the schema has it (first in `p:cSld`).
    let slide_bg = slide_doc.root_element().and_then(|r| slide_doc.child(r, "cSld")).and_then(|c| slide_doc.child(c, "bg"));
    let part_csld = doc.root_element().and_then(|r| doc.child(r, "cSld"));
    if let Some(csld) = part_csld {
        if let Some(old) = doc.child(csld, "bg") {
            doc.detach(old);
        }
    }
    let items: Vec<(NodeId, bool)> = slide_bg.map(|b| (b, true)).into_iter().chain(shapes.iter().map(|&n| (n, false))).collect();
    for (i, (shape, is_bg)) in items.iter().enumerate() {
        let (shape, is_bg) = (shape, *is_bg);
        let copy = doc.import(&slide_doc, *shape);
        // (In the tree first: the prefixes of its attributes are found from its ancestors.)
        match (is_bg, part_csld) {
            (true, Some(csld)) => doc.insert(csld, 0, copy),
            _ => doc.insert(tree_id, at + i - usize::from(slide_bg.is_some()), copy),
        }
        // The relationships the shape names, made the part's own.
        for node in std::iter::once(copy).chain(doc.descendants(copy)) {
            if !doc.is_element(node) {
                continue;
            }
            let names: Vec<(String, String)> = doc
                .attributes(node)
                .iter()
                .filter(|a| a.name.rsplit_once(':').is_some_and(|(p, _)| doc.resolve_prefix(node, Some(p)) == Some(NS_R)))
                .map(|a| (a.name.clone(), a.value.clone()))
                .collect();
            for (name, value) in names {
                if value.is_empty() {
                    continue;
                }
                let Some(rel) = slide_rels.iter().find(|r| r.id == value) else { continue };
                let new_id = match mapped.get(&value) {
                    Some(id) => id.clone(),
                    None => {
                        let target = if rel.external { rel.target.clone() } else { rel.target_part(&slide_part).map(|t| format!("part:{t}")).unwrap_or_default() };
                        // (Added to the package after the tree is set: ids are made then.)
                        let id = format!("ibis-new-{}", mapped.len());
                        mapped.insert(value.clone(), id.clone());
                        added.push((id.clone(), rel.kind.clone(), target, rel.external));
                        id
                    }
                };
                doc.set_attr(node, &name, &new_id);
            }
        }
    }
    // ガイド線 as edited on the page: the part's own (a master's, or a layout's, in its kind of extension).
    if let (Some(slide_root), Some(root)) = (slide_doc.root_element(), doc.root_element()) {
        let guides = crate::save::pptx_guides::read(&slide_doc, slide_root);
        let uri = if doc.local_name(root) == "sldMaster" { crate::save::pptx_guides::URI_MASTER } else { crate::save::pptx_guides::URI_PAGE };
        crate::save::pptx_guides::write(&mut doc, root, &guides, uri)?;
    }
    package.set_xml(part, doc);
    // The theme (配色・フォント) as edited on the page: the master's theme part (a layout's through its master).
    if let Some(theme) = theme_part_of(&package, part)? {
        if edited.contains(&theme) {
            let new = edited.read(&theme)?;
            if package.read(&theme)? != new {
                package.set_bytes(&theme, new);
            }
        }
    }
    // The relationships made, and the parts they name copied from the edited file.
    let mut real: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (placeholder, kind, target, external) in added {
        let new_id = if external {
            package.add_relationship(part, &kind, &target, true)?
        } else {
            let source = target.strip_prefix("part:").unwrap_or(&target).to_string();
            let bytes = edited.read(&source)?;
            // The same part if the file has it with the same bytes, else a copy.
            let name = if package.contains(&source) && package.read(&source)? == bytes {
                source.clone()
            } else if package.contains(&source) {
                next_part_name(&package, &source)
            } else {
                source.clone()
            };
            if name != source || !package.contains(&name) {
                package.set_bytes(&name, bytes);
                if let Some(ct) = edited.content_type(&source)? {
                    let default = package.content_type(&name)?;
                    if default.as_deref() != Some(ct.as_str()) {
                        package.set_override(&name, &ct)?;
                    }
                }
            }
            package.add_relationship(part, &kind, &relative(part, &name), false)?
        };
        real.insert(placeholder, new_id);
    }
    // The placeholders' ids replaced by the real ones.
    if !real.is_empty() {
        let doc = package.xml_mut(part)?;
        let root = doc.root_element().ok_or_else(|| err("ページが空です"))?;
        for node in std::iter::once(root).chain(doc.descendants(root)) {
            if !doc.is_element(node) {
                continue;
            }
            let swaps: Vec<(String, String)> = doc.attributes(node).iter().filter(|a| real.contains_key(&a.value)).map(|a| (a.name.clone(), real[&a.value].clone())).collect();
            for (name, value) in swaps {
                doc.set_attr(node, &name, &value);
            }
        }
    }
    Ok(package.save(SaveOptions::default())?)
}

// ----- レイアウトの名前の変更・複製・削除 (the master view's list) --------------------------------

/// The master that has layout part `layout` in its list.
fn master_of(package: &Package, layout: &str) -> Result<String, Error> {
    package.relationships(layout)?.into_iter().find(|r| r.kind == REL_SLIDE_MASTER && !r.external).and_then(|r| r.target_part(layout)).ok_or_else(|| err("マスターの無いレイアウトです"))
}

/// Layout `part` named `name` (`p:cSld/@name`); a master's name is its theme's.
pub fn rename_page(data: &[u8], part: &str, name: &str) -> Result<Vec<u8>, Error> {
    let mut package = Package::open(data.to_vec())?;
    let target = {
        let doc = package.xml(part)?;
        let root = doc.root_element().ok_or_else(|| err("ページが空です"))?;
        match doc.local_name(root) {
            "sldLayout" => part.to_string(),
            "sldMaster" => theme_part_of(&package, part)?.ok_or_else(|| err("マスターにテーマがありません"))?,
            _ => return Err(err("マスターでもレイアウトでもありません")),
        }
    };
    let doc = package.xml_mut(&target)?;
    let root = doc.root_element().ok_or_else(|| err("ページが空です"))?;
    if doc.local_name(root) == "sldLayout" {
        let csld = doc.child(root, "cSld").ok_or_else(|| err("レイアウトに cSld がありません"))?;
        doc.set_attr(csld, "name", name);
    } else {
        doc.set_attr(root, "name", name);
    }
    Ok(package.save(SaveOptions::default())?)
}

/// The largest id of the masters and layouts of the file (their ids share one range).
fn largest_layout_or_master_id(package: &Package) -> Result<u64, Error> {
    let main = main_part(package)?;
    let mut top: u64 = 2_147_483_647;
    let doc = package.xml(&main)?;
    let root = doc.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
    if let Some(list) = doc.child(root, "sldMasterIdLst") {
        for e in doc.elements(list) {
            top = top.max(doc.attr(e, "id").and_then(|v| v.parse().ok()).unwrap_or(0));
        }
    }
    for rel in package.relationships(&main)?.into_iter().filter(|r| r.kind == REL_SLIDE_MASTER && !r.external) {
        let Some(master) = rel.target_part(&main).filter(|p| package.contains(p)) else { continue };
        let doc = package.xml(&master)?;
        let Some(root) = doc.root_element() else { continue };
        if let Some(list) = doc.child(root, "sldLayoutIdLst") {
            for e in doc.elements(list) {
                top = top.max(doc.attr(e, "id").and_then(|v| v.parse().ok()).unwrap_or(0));
            }
        }
    }
    Ok(top)
}

/// Layout `part` copied, right after it in its master's list, named `name`: the file, and the
/// part of the copy.
pub fn duplicate_layout(data: &[u8], part: &str, name: &str) -> Result<(Vec<u8>, String), Error> {
    let mut package = Package::open(data.to_vec())?;
    let master = master_of(&package, part)?;
    let copy = next_part_name(&package, part);
    // The part, its relationships (the master, the pictures: the same directory, so the same targets), its type.
    package.set_bytes(&copy, package.read(part)?);
    if let Some(ct) = package.content_type(part)? {
        package.set_override(&copy, &ct)?;
    }
    let rels = rels_part(part);
    if package.contains(&rels) {
        package.set_bytes(&rels_part(&copy), package.read(&rels)?);
    }
    {
        let doc = package.xml_mut(&copy)?;
        let root = doc.root_element().ok_or_else(|| err("レイアウトが空です"))?;
        if let Some(csld) = doc.child(root, "cSld") {
            doc.set_attr(csld, "name", name);
        }
        // A copy is not the layout the file names by its type.
        doc.remove_attr(root, "type");
        doc.remove_attr(root, "matchingName");
    }
    // The master's list: a relationship, and an entry after the layout's own.
    let new_rel = package.add_relationship(&master, REL_SLIDE_LAYOUT, &relative(&master, &copy), false)?;
    let id = largest_layout_or_master_id(&package)? + 1;
    let source_rel = package.relationships(&master)?.into_iter().find(|r| r.kind == REL_SLIDE_LAYOUT && r.target_part(&master).as_deref() == Some(part)).map(|r| r.id);
    let doc = package.xml_mut(&master)?;
    let root = doc.root_element().ok_or_else(|| err("マスターが空です"))?;
    let list = doc.child(root, "sldLayoutIdLst").ok_or_else(|| err("レイアウトの一覧がありません"))?;
    let entry = source_rel.and_then(|rid| doc.elements(list).find(|&e| rel_id(doc, e).as_deref() == Some(rid.as_str()))).or_else(|| doc.elements(list).last()).ok_or_else(|| err("レイアウトの一覧が空です"))?;
    // (The name of its `r:id` is found where it is in the tree: a copy has no ancestors to name the prefix.)
    let attr = doc.attributes(entry).iter().find(|a| a.name.rsplit_once(':').is_some_and(|(p, l)| l == "id" && doc.resolve_prefix(entry, Some(p)) == Some(NS_R))).map(|a| a.name.clone()).ok_or_else(|| err("r:id がありません"))?;
    let new = doc.deep_clone(entry);
    doc.set_attr(new, "id", &id.to_string());
    doc.set_attr(new, &attr, &new_rel);
    doc.insert_after(entry, new);
    Ok((package.save(SaveOptions::default())?, copy))
}

/// Whether a slide of the file follows layout `part`.
pub fn layout_in_use(data: &[u8], part: &str) -> Result<bool, Error> {
    let package = Package::open(data.to_vec())?;
    let main = main_part(&package)?;
    for rel in package.relationships(&main)?.into_iter().filter(|r| r.kind == REL_SLIDE && !r.external) {
        let Some(slide) = rel.target_part(&main) else { continue };
        if package.relationships(&slide)?.into_iter().any(|r| r.kind == REL_SLIDE_LAYOUT && r.target_part(&slide).as_deref() == Some(part)) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Layout `part` taken out of its master, its part and relationships with it. A layout a slide follows
/// (or the master's last) is not.
pub fn delete_layout(data: &[u8], part: &str) -> Result<Vec<u8>, Error> {
    if layout_in_use(data, part)? {
        return Err(err("スライドが使っているレイアウトは削除できません"));
    }
    let mut package = Package::open(data.to_vec())?;
    let master = master_of(&package, part)?;
    if layouts_of(&package, &master)?.len() < 2 {
        return Err(err("マスターの最後のレイアウトは削除できません"));
    }
    let rel = package.relationships(&master)?.into_iter().find(|r| r.kind == REL_SLIDE_LAYOUT && r.target_part(&master).as_deref() == Some(part)).ok_or_else(|| err("マスターの関係がありません"))?;
    {
        let doc = package.xml_mut(&master)?;
        let root = doc.root_element().ok_or_else(|| err("マスターが空です"))?;
        if let Some(list) = doc.child(root, "sldLayoutIdLst") {
            let entry = doc.elements(list).find(|&e| rel_id(doc, e).as_deref() == Some(rel.id.as_str()));
            if let Some(entry) = entry {
                doc.detach(entry);
            }
        }
    }
    package.remove_relationship(&master, &rel.id)?;
    package.remove_override(part)?;
    package.remove(part);
    package.remove(&rels_part(part));
    Ok(package.save(SaveOptions::default())?)
}

/// Which text style of a master (`p:txStyles`): the title's, the body's (the
/// placeholders of the text of a slide), or the other texts'.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleKind {
    Title,
    Body,
    Other,
}

impl StyleKind {
    fn element(self) -> &'static str {
        match self {
            StyleKind::Title => "titleStyle",
            StyleKind::Body => "bodyStyle",
            StyleKind::Other => "otherStyle",
        }
    }
}

/// How the text of a level looks (`a:defRPr`): what is not stated is `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LevelStyle {
    /// The size in hundredths of a point (`sz`).
    pub size: Option<u32>,
    pub bold: Option<bool>,
    /// The colour, `0xRRGGBB`, when it is a plain one (a theme colour is `None`).
    pub color: Option<u32>,
    /// The typeface (`a:latin`, or `a:ea` if there is no latin one); `+mj-lt`-like theme fonts as written.
    pub font: Option<String>,
}

/// The text styles of master `master`: for each style and level (1 to 9) it states.
pub fn text_styles(data: &[u8], master: &str) -> Result<Vec<(StyleKind, u8, LevelStyle)>, Error> {
    let package = Package::open(data.to_vec())?;
    let doc = package.xml(master)?;
    let root = doc.root_element().ok_or_else(|| err("マスターが空です"))?;
    let Some(styles) = doc.child(root, "txStyles") else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for kind in [StyleKind::Title, StyleKind::Body, StyleKind::Other] {
        let Some(style) = doc.child(styles, kind.element()) else { continue };
        for level in 1..=9u8 {
            let Some(def) = doc.child(style, &format!("lvl{level}pPr")).and_then(|l| doc.child(l, "defRPr")) else { continue };
            let color = doc.child(def, "solidFill").and_then(|f| doc.child(f, "srgbClr")).and_then(|c| doc.attr(c, "val")).and_then(|v| u32::from_str_radix(v, 16).ok());
            let font = doc.child(def, "latin").or_else(|| doc.child(def, "ea")).and_then(|f| doc.attr(f, "typeface")).map(str::to_string);
            out.push((kind, level, LevelStyle { size: doc.attr(def, "sz").and_then(|v| v.parse().ok()), bold: doc.attr(def, "b").map(|v| v == "1" || v == "true"), color, font }));
        }
    }
    Ok(out)
}

/// `data` with level `level` (1 to 9) of `kind` in master `master` given the look `change` states
/// (what it leaves `None` is left as it is): for every slide that does not say otherwise itself.
pub fn set_text_style(data: &[u8], master: &str, kind: StyleKind, level: u8, change: &LevelStyle) -> Result<Vec<u8>, Error> {
    if !(1..=9).contains(&level) {
        return Err(err("レベルは 1〜9 です"));
    }
    let mut package = Package::open(data.to_vec())?;
    let doc = package.xml_mut(master)?;
    let root = doc.root_element().ok_or_else(|| err("マスターが空です"))?;
    let styles = doc.child(root, "txStyles").ok_or_else(|| err("文字の書式がありません"))?;
    let style = doc.child(styles, kind.element()).ok_or_else(|| err("この文字の書式がありません"))?;
    let x = crate::save::pptx_new::Prefixes::at(doc, style);
    let name = format!("lvl{level}pPr");
    // The level, made in its place if the master has none.
    let lvl = match doc.child(style, &name) {
        Some(l) => l,
        None => {
            let at = doc.children(style).iter().position(|&c| doc.is_element(c) && doc.local_name(c).strip_prefix("lvl").and_then(|r| r.strip_suffix("pPr")).and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| n > level)).unwrap_or(doc.children(style).len());
            let l = x.import(doc, &format!("<{a}{name}/>", a = x.a))?;
            doc.insert(style, at, l);
            l
        }
    };
    let def = match doc.child(lvl, "defRPr") {
        Some(d) => d,
        None => {
            let d = x.import(doc, &format!("<{a}defRPr/>", a = x.a))?;
            // After the paragraph properties' own children (`a:buNone` and the like): last.
            let end = doc.children(lvl).len();
            doc.insert(lvl, end, d);
            d
        }
    };
    if let Some(size) = change.size {
        doc.set_attr(def, "sz", &size.clamp(100, 400_000).to_string());
    }
    if let Some(bold) = change.bold {
        doc.set_attr(def, "b", if bold { "1" } else { "0" });
    }
    if let Some(color) = change.color {
        for fill in doc.children(def).to_vec() {
            if doc.is_element(fill) && matches!(doc.local_name(fill), "solidFill" | "gradFill" | "noFill" | "pattFill" | "blipFill" | "grpFill") {
                doc.detach(fill);
            }
        }
        let fill = x.import(doc, &format!("<{a}solidFill><{a}srgbClr val=\"{color:06X}\"/></{a}solidFill>", a = x.a, color = color & 0xFF_FFFF))?;
        // After the outline (`a:ln`), before the effects and the typefaces.
        let at = doc.children(def).iter().position(|&c| doc.is_element(c) && doc.local_name(c) != "ln").unwrap_or(doc.children(def).len());
        doc.insert(def, at, fill);
    }
    if let Some(font) = &change.font {
        for tag in ["latin", "ea"] {
            match doc.child(def, tag) {
                Some(f) => doc.set_attr(f, "typeface", font),
                None => {
                    let f = x.import(doc, &format!("<{a}{tag} typeface=\"{}\"/>", crate::save::pptx_new::escape(font), a = x.a))?;
                    // Before the other typefaces and the links.
                    let order = ["latin", "ea", "cs", "sym", "hlinkClick", "hlinkMouseOver", "rtl", "extLst"];
                    let mine = order.iter().position(|o| *o == tag).unwrap_or(0);
                    let at = doc.children(def).iter().position(|&c| doc.is_element(c) && order.iter().position(|o| *o == doc.local_name(c)).is_some_and(|p| p > mine)).unwrap_or(doc.children(def).len());
                    doc.insert(def, at, f);
                }
            }
        }
    }
    Ok(package.save(SaveOptions::default())?)
}

/// What a placeholder added to a layout is (`p:ph/@type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceholderKind {
    /// タイトル (`title`).
    Title,
    /// テキスト (`body`).
    Text,
    /// コンテンツ (no type: text, table, chart, picture…).
    Content,
    /// 図 (`pic`).
    Picture,
    /// グラフ (`chart`).
    Chart,
    /// 表 (`tbl`).
    Table,
    /// メディア (`media`).
    Media,
}

impl PlaceholderKind {
    pub const ALL: [PlaceholderKind; 7] = [Self::Title, Self::Text, Self::Content, Self::Picture, Self::Chart, Self::Table, Self::Media];

    /// The `p:ph` attributes (without `idx`), and the base of the shape's name.
    fn attributes(self) -> (&'static str, &'static str) {
        match self {
            Self::Title => (" type=\"title\"", "Title"),
            Self::Text => (" type=\"body\" sz=\"quarter\"", "Text Placeholder"),
            Self::Content => ("", "Content Placeholder"),
            Self::Picture => (" type=\"pic\" sz=\"quarter\"", "Picture Placeholder"),
            Self::Chart => (" type=\"chart\" sz=\"quarter\"", "Chart Placeholder"),
            Self::Table => (" type=\"tbl\" sz=\"quarter\"", "Table Placeholder"),
            Self::Media => (" type=\"media\" sz=\"quarter\"", "Media Placeholder"),
        }
    }
}

/// A placeholder of `kind` put on the layout (or master) `part`, in the middle of the slide, for the slides that
/// follow the layout to take (レイアウトにプレースホルダーを挿入). The rest of the file is left as it was.
pub fn add_placeholder(data: &[u8], part: &str, kind: PlaceholderKind) -> Result<Vec<u8>, Error> {
    let mut package = Package::open(data.to_vec())?;
    let main = main_part(&package)?;
    // The slide's size, for the place.
    let (cx, cy) = {
        let doc = package.xml(&main)?;
        let root = doc.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
        let size = doc.child(root, "sldSz");
        let get = |name: &str, default: i64| size.and_then(|s| doc.attr(s, name)).and_then(|v| v.parse::<i64>().ok()).unwrap_or(default);
        (get("cx", 12_192_000), get("cy", 6_858_000))
    };
    let doc = package.xml_mut(part)?;
    let tree_id = tree(doc).ok_or_else(|| err("図形の木がありません"))?;
    // The next shape id and placeholder index.
    let (mut max_id, mut max_idx) = (1u64, 0u64);
    let root = doc.root_element().ok_or_else(|| err("ページが空です"))?;
    for node in doc.descendants(root) {
        if !doc.is_element(node) {
            continue;
        }
        match doc.local_name(node) {
            "cNvPr" => max_id = max_id.max(doc.attr(node, "id").and_then(|v| v.parse().ok()).unwrap_or(0)),
            "ph" => max_idx = max_idx.max(doc.attr(node, "idx").and_then(|v| v.parse().ok()).unwrap_or(0)),
            _ => {}
        }
    }
    let (attrs, base) = kind.attributes();
    let id = max_id + 1;
    let idx = if kind == PlaceholderKind::Title { String::new() } else { format!(" idx=\"{}\"", max_idx + 1) };
    let (x, y, w, h) = if kind == PlaceholderKind::Title { (cx / 12, cy / 16, cx * 5 / 6, cy / 6) } else { (cx / 12, cy / 4, cx * 5 / 6, cy * 3 / 5) };
    let x_ = crate::save::pptx_new::Prefixes::at(doc, tree_id);
    let (a, p) = (&x_.a, &x_.p);
    let xml = format!(
        "<{p}sp><{p}nvSpPr><{p}cNvPr id=\"{id}\" name=\"{base} {id}\"/><{p}cNvSpPr><{a}spLocks noGrp=\"1\"/></{p}cNvSpPr><{p}nvPr><{p}ph{attrs}{idx}/></{p}nvPr></{p}nvSpPr>\
         <{p}spPr><{a}xfrm><{a}off x=\"{x}\" y=\"{y}\"/><{a}ext cx=\"{w}\" cy=\"{h}\"/></{a}xfrm></{p}spPr>\
         <{p}txBody><{a}bodyPr/><{a}lstStyle/><{a}p><{a}endParaRPr lang=\"ja-JP\"/></{a}p></{p}txBody></{p}sp>"
    );
    let shape = x_.import(doc, &xml)?;
    let at = doc.children(tree_id).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "extLst").unwrap_or(doc.children(tree_id).len());
    doc.insert(tree_id, at, shape);
    Ok(package.save(SaveOptions::default())?)
}

const REL_NOTES_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster";

/// The part of the notes master (ノート マスター) of a file, if it has one.
pub fn notes_master_part(data: &[u8]) -> Result<Option<String>, Error> {
    let package = Package::open(data.to_vec())?;
    let main = main_part(&package)?;
    Ok(package.relationships(&main)?.into_iter().find(|r| r.kind == REL_NOTES_MASTER && !r.external).and_then(|r| r.target_part(&main)).filter(|p| package.contains(p)))
}

/// A file like `data` with one slide: the notes master (the page's image, the notes, the header, date, footer and
/// number places) as a slide of the size of a notes page, which the ordinary editor opens. [`apply`] with the notes
/// master's part puts the shapes, as edited, back; the notes' text styles and the rest are left as they were.
pub fn build_notes_master(data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut package = Package::open(data.to_vec())?;
    let main = main_part(&package)?;
    let part = notes_master_part(data)?.ok_or_else(|| err("ノート マスターがありません"))?;
    // The layout the slide follows (any: it shows nothing).
    let master = package.relationships(&main)?.into_iter().find(|r| r.kind == REL_SLIDE_MASTER && !r.external).and_then(|r| r.target_part(&main)).ok_or_else(|| err("スライド マスターがありません"))?;
    let layout = layouts_of(&package, &master)?.into_iter().next().ok_or_else(|| err("レイアウトの無いマスターです"))?;
    let slide_part = next_part_name(&package, "ppt/slides/slide1.xml");
    let mut slide = package.xml(&part)?;
    let slide_root = slide.root_element().ok_or_else(|| err("ノート マスターが空です"))?;
    let prefix = slide.prefix(slide_root).map(|p| format!("{p}:")).unwrap_or_default();
    slide.rename(slide_root, &format!("{prefix}sld"));
    slide.set_attr(slide_root, "showMasterSp", "0");
    for child in slide.children(slide_root).to_vec() {
        if slide.is_element(child) && matches!(slide.local_name(child), "clrMap" | "hf" | "notesStyle") {
            slide.detach(child);
        }
    }
    if let Some(csld) = slide.child(slide_root, "cSld") {
        if slide.child(csld, "bg").is_none() {
            let x = crate::save::pptx_new::Prefixes::at(&slide, csld);
            let bg = x.import(&mut slide, &format!("<{p}bg><{p}bgPr><{a}solidFill><{a}srgbClr val=\"FFFFFF\"/></{a}solidFill><{a}effectLst/></{p}bgPr></{p}bg>", p = x.p, a = x.a))?;
            slide.insert(csld, 0, bg);
        }
    }
    package.set_xml(&slide_part, slide);
    package.set_override(&slide_part, CT_SLIDE)?;
    for rel in package.relationships(&part)?.iter().filter(|r| r.kind != REL_THEME) {
        let target = if rel.external { rel.target.clone() } else { match rel.target_part(&part) { Some(t) => relative(&slide_part, &t), None => continue } };
        add_relationship_with_id(&mut package, &slide_part, &rel.id, &rel.kind, &target, rel.external)?;
    }
    package.add_relationship(&slide_part, REL_SLIDE_LAYOUT, &relative(&slide_part, &layout), false)?;
    {
        let doc = package.xml_mut(&layout)?;
        if let Some(t) = tree(doc) {
            empty_tree(doc, t);
        }
        let doc = package.xml_mut(&part)?;
        if let Some(t) = tree(doc) {
            empty_tree(doc, t);
        }
    }
    let rid = package.add_relationship(&main, REL_SLIDE, &relative(&main, &slide_part), false)?;
    let doc = package.xml_mut(&main)?;
    let root = doc.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
    let notes_size = doc.child(root, "notesSz").map(|n| (doc.attr(n, "cx").unwrap_or("6858000").to_string(), doc.attr(n, "cy").unwrap_or("9144000").to_string())).unwrap_or(("6858000".into(), "9144000".into()));
    let list = doc.child(root, "sldIdLst").ok_or_else(|| err("スライドの一覧がありません"))?;
    for child in doc.children(list).to_vec() {
        doc.detach(child);
    }
    let x = crate::save::pptx_new::Prefixes::at(doc, list);
    let entry = x.import(doc, &format!("<{p}sldId id=\"256\" {r}id=\"{rid}\"/>", p = x.p, r = x.r))?;
    doc.insert(list, 0, entry);
    if let Some(size) = doc.child(root, "sldSz") {
        doc.set_attr(size, "cx", &notes_size.0);
        doc.set_attr(size, "cy", &notes_size.1);
        doc.remove_attr(size, "type");
    }
    if let Some(shows) = doc.child(root, "custShowLst") {
        doc.detach(shows);
    }
    if let Some(ext) = doc.child(root, "extLst") {
        for e in doc.elements(ext).collect::<Vec<_>>() {
            if doc.descendants(e).into_iter().any(|d| doc.is_element(d) && doc.local_name(d) == "sectionLst") {
                doc.detach(e);
            }
        }
    }
    Ok(package.save(SaveOptions::default())?)
}

/// `data` with the shapes of the notes master those of the slide of `edited` (a file [`build_notes_master`] made from
/// `data`, then edited).
pub fn apply_notes_master(data: &[u8], edited: &[u8]) -> Result<Vec<u8>, Error> {
    let part = notes_master_part(data)?.ok_or_else(|| err("ノート マスターがありません"))?;
    apply(data, edited, &part)
}
