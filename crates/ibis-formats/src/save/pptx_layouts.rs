// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スライド マスターとレイアウトの名前の変更・複製・削除:
//! - a layout renamed is written as `p:cSld/@name`; a master renamed is its
//!   theme's name (`a:theme/@name`);
//! - a layout copied (`Layout::copy_of`) is a new layout part, a copy of the
//!   original's, added to its master's `p:sldLayoutIdLst`;
//! - a master copied (`Master::copy_of`) is a new master part with a copy of
//!   its theme and of every layout of the original, added to the
//!   presentation's `p:sldMasterIdLst`; the model's layouts of the copy
//!   (copies of the original's) are those parts;
//! - a layout or master removed has its parts removed with the entries that
//!   list them.

use std::collections::HashMap;

use ibis_doc_model::{LayoutId, MasterId, Presentation};
use ibis_ooxml::{NodeId, Package};

use super::pptx::{SourceMap, next_part_name, relative, rel_attr};
use super::pptx_new::Prefixes;
use crate::Error;

const REL_SLIDE_LAYOUT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
const REL_SLIDE_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
const REL_THEME: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";

/// Whether a master or a layout was renamed, copied or removed since `baseline`.
pub(crate) fn changed(baseline: &Presentation, current: &Presentation) -> bool {
    let layouts = current.layouts.iter().any(|l| match baseline.layouts.iter().find(|b| b.id == l.id) {
        Some(b) => b.name != l.name,
        None => l.copy_of.is_some(),
    }) || baseline.layouts.iter().any(|b| current.layout(b.id).is_none());
    let masters = current.masters.iter().any(|m| match baseline.master(m.id) {
        Some(b) => b.name != m.name,
        None => m.copy_of.is_some(),
    }) || baseline.masters.iter().any(|b| current.master(b.id).is_none());
    layouts || masters
}

/// What was made: the parts of the layouts copied (and of those of copied
/// masters), by the model's layout.
pub(crate) fn write(package: &mut Package, map: &SourceMap, baseline: &Presentation, current: &Presentation) -> Result<HashMap<LayoutId, String>, Error> {
    let mut made: HashMap<LayoutId, String> = HashMap::new();
    let mut made_masters: HashMap<MasterId, String> = HashMap::new();
    // Layouts of a copied master: the original layout's part → its copy.
    let mut master_layouts: HashMap<MasterId, HashMap<String, String>> = HashMap::new();
    for master in &current.masters {
        match baseline.master(master.id) {
            Some(old) => {
                if old.name != master.name
                    && let Some(part) = map.masters.get(&master.id)
                {
                    set_theme_name(package, part, &master.name)?;
                }
            }
            None => {
                let Some(source) = master.copy_of else { continue };
                let Some(from) = map.masters.get(&source).or_else(|| made_masters.get(&source)).cloned() else { continue };
                let (part, layouts) = copy_master(package, &map.presentation, &from)?;
                set_theme_name(package, &part, &master.name)?;
                made_masters.insert(master.id, part);
                master_layouts.insert(master.id, layouts);
            }
        }
    }
    for layout in &current.layouts {
        match baseline.layouts.iter().find(|b| b.id == layout.id) {
            Some(old) => {
                if old.name != layout.name
                    && let Some(part) = map.layouts.get(&layout.id)
                {
                    set_name(package, part, &layout.name)?;
                }
            }
            None => {
                let Some(source) = layout.copy_of else { continue };
                let Some(from) = map.layouts.get(&source).or_else(|| made.get(&source)).cloned() else { continue };
                // A layout of a copied master, copied with it.
                let inherited = layout.master.and_then(|m| master_layouts.get(&m)).and_then(|l| l.get(&from)).cloned();
                let part = match inherited {
                    Some(part) => {
                        set_name(package, &part, &layout.name)?;
                        part
                    }
                    None => copy_layout(package, &map.presentation, &from, &layout.name)?,
                };
                made.insert(layout.id, part);
            }
        }
    }
    // Removed: the layouts (those of a removed master go with it), then the masters.
    let gone_masters: Vec<MasterId> = baseline.masters.iter().filter(|b| current.master(b.id).is_none()).map(|b| b.id).collect();
    for layout in baseline.layouts.iter().filter(|b| current.layout(b.id).is_none() && !b.master.is_some_and(|m| gone_masters.contains(&m))) {
        if let Some(part) = map.layouts.get(&layout.id) {
            remove_layout(package, part)?;
        }
    }
    for id in gone_masters {
        let Some(part) = map.masters.get(&id) else { continue };
        let own: Vec<String> = baseline.layouts.iter().filter(|l| l.master == Some(id)).filter_map(|l| map.layouts.get(&l.id).cloned()).collect();
        let others: Vec<String> = current.masters.iter().filter_map(|m| map.masters.get(&m.id).cloned()).collect();
        remove_master(package, &map.presentation, part, &own, &others)?;
    }
    Ok(made)
}

fn set_name(package: &mut Package, part: &str, name: &str) -> Result<(), Error> {
    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{part} が空です")))?;
    let csld = doc.child(root, "cSld").ok_or_else(|| crate::save::unsupported("この形のレイアウト"))?;
    if name.is_empty() {
        doc.remove_attr(csld, "name");
    } else {
        doc.set_attr(csld, "name", name);
    }
    Ok(())
}

/// The theme part of master `master`.
fn theme_of(package: &Package, master: &str) -> Result<Option<String>, Error> {
    Ok(package.relationships(master)?.into_iter().find(|r| r.kind == REL_THEME && !r.external).and_then(|r| r.target_part(master)))
}

fn set_theme_name(package: &mut Package, master: &str, name: &str) -> Result<(), Error> {
    let Some(theme) = theme_of(package, master)? else { return Err(crate::save::unsupported("テーマの無いマスターの名前の変更")) };
    let doc = package.xml_mut(&theme)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{theme} が空です")))?;
    doc.set_attr(root, "name", name);
    Ok(())
}

/// Part `part` copied to a new part named like it, with its content type
/// and the relationships it has but those of kinds in `skip`.
fn copy_part(package: &mut Package, part: &str, skip: &[&str]) -> Result<String, Error> {
    let copy = next_part_name(package, part);
    if package.xml(part).is_ok() {
        let doc = package.xml(part)?;
        package.set_xml(&copy, doc);
    } else {
        package.set_bytes(&copy, package.read(part)?);
    }
    if let Some(kind) = package.content_type(part)? {
        package.set_override(&copy, &kind)?;
    }
    for rel in package.relationships(part)? {
        if !skip.contains(&rel.kind.as_str()) {
            package.add_relationship(&copy, &rel.kind, &rel.target, rel.external)?;
        }
    }
    Ok(copy)
}

/// A copy of layout part `from` named `name`, in the same master.
fn copy_layout(package: &mut Package, presentation: &str, from: &str, name: &str) -> Result<String, Error> {
    let master = package.relationships(from)?.into_iter().find(|r| r.kind == REL_SLIDE_MASTER).and_then(|r| r.target_part(from)).ok_or_else(|| crate::save::unsupported("マスターの無いレイアウトの複製"))?;
    let part = copy_part(package, from, &[])?;
    set_name(package, &part, name)?;
    let id = super::pptx_foreign::used_master_ids(package, presentation)? + 1;
    add_to_master(package, &master, &part, id)?;
    Ok(part)
}

/// Layout part `part` listed in master `master` (entry id `id`).
fn add_to_master(package: &mut Package, master: &str, part: &str, id: u64) -> Result<(), Error> {
    let rel = package.add_relationship(master, REL_SLIDE_LAYOUT, &relative(master, part), false)?;
    let mdoc = package.xml_mut(master)?;
    let root = mdoc.root_element().ok_or_else(|| Error::new("マスターが空です"))?;
    let list = mdoc.child(root, "sldLayoutIdLst").ok_or_else(|| crate::save::unsupported("レイアウトの一覧の無いマスター"))?;
    let x = Prefixes::at(mdoc, list);
    let entry = x.import(mdoc, &format!("<{p}sldLayoutId id=\"{id}\" {r}id=\"{rel}\"/>", p = x.p, r = x.r))?;
    let end = mdoc.children(list).len();
    mdoc.insert(list, end, entry);
    Ok(())
}

/// A copy of master part `from` with its theme and layouts, added to the
/// presentation's masters; returns the master part and, for each layout part
/// of the original, its copy.
fn copy_master(package: &mut Package, presentation: &str, from: &str) -> Result<(String, HashMap<String, String>), Error> {
    let start = super::pptx_foreign::used_master_ids(package, presentation)? + 1;
    let master = copy_part(package, from, &[REL_SLIDE_LAYOUT, REL_THEME])?;
    if let Some(theme) = theme_of(package, from)? {
        let copy = copy_part(package, &theme, &[])?;
        package.add_relationship(&master, REL_THEME, &relative(&master, &copy), false)?;
    }
    // The original's layouts, in its order.
    let originals: Vec<String> = {
        let doc = package.xml(from)?;
        let rels = package.relationships(from)?;
        let root = doc.root_element().ok_or_else(|| Error::new("マスターが空です"))?;
        doc.child(root, "sldLayoutIdLst")
            .map(|list| doc.elements(list).collect::<Vec<NodeId>>())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| rel_attr(&doc, e).map(str::to_string))
            .filter_map(|rid| rels.iter().find(|r| r.id == rid && r.kind == REL_SLIDE_LAYOUT).and_then(|r| r.target_part(from)))
            .collect()
    };
    {
        let mdoc = package.xml_mut(&master)?;
        let root = mdoc.root_element().ok_or_else(|| Error::new("マスターが空です"))?;
        if let Some(list) = mdoc.child(root, "sldLayoutIdLst") {
            for e in mdoc.elements(list).collect::<Vec<_>>() {
                mdoc.detach(e);
            }
        }
    }
    let mut copies = HashMap::new();
    for (i, layout) in originals.iter().enumerate() {
        let part = copy_part(package, layout, &[REL_SLIDE_MASTER])?;
        package.add_relationship(&part, REL_SLIDE_MASTER, &relative(&part, &master), false)?;
        add_to_master(package, &master, &part, start + 1 + i as u64)?;
        copies.insert(layout.clone(), part);
    }
    // Listed with the presentation's masters.
    let rel = package.add_relationship(presentation, REL_SLIDE_MASTER, &relative(presentation, &master), false)?;
    let pdoc = package.xml_mut(presentation)?;
    let proot = pdoc.root_element().ok_or_else(|| Error::new("プレゼンテーションが空です"))?;
    let list = pdoc.child(proot, "sldMasterIdLst").ok_or_else(|| crate::save::unsupported("マスターの一覧の無いプレゼンテーション"))?;
    let x = Prefixes::at(pdoc, list);
    let entry = x.import(pdoc, &format!("<{p}sldMasterId id=\"{start}\" {r}id=\"{rel}\"/>", p = x.p, r = x.r))?;
    let end = pdoc.children(list).len();
    pdoc.insert(list, end, entry);
    Ok((master, copies))
}

fn rels_part(part: &str) -> String {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") }
}

/// Part `part` removed with its relationships part and content type.
fn remove_part(package: &mut Package, part: &str) -> Result<(), Error> {
    package.remove(&rels_part(part));
    package.remove_override(part)?;
    package.remove(part);
    Ok(())
}

/// The relationship of `source` to `target` (kind `kind`), and the list entry
/// that names it (`list` of its root), removed.
fn unlist(package: &mut Package, source: &str, kind: &str, target: &str, list: &str) -> Result<(), Error> {
    let rels = package.relationships(source)?;
    for rel in rels.iter().filter(|r| r.kind == kind && r.target_part(source).as_deref() == Some(target)) {
        let doc = package.xml_mut(source)?;
        if let Some(root) = doc.root_element()
            && let Some(l) = doc.child(root, list)
        {
            let gone: Vec<NodeId> = doc.elements(l).collect::<Vec<_>>().into_iter().filter(|&e| rel_attr(doc, e) == Some(rel.id.as_str())).collect();
            for e in gone {
                doc.detach(e);
            }
        }
        package.remove_relationship(source, &rel.id)?;
    }
    Ok(())
}

fn remove_layout(package: &mut Package, part: &str) -> Result<(), Error> {
    if let Some(master) = package.relationships(part)?.into_iter().find(|r| r.kind == REL_SLIDE_MASTER).and_then(|r| r.target_part(part)) {
        unlist(package, &master, REL_SLIDE_LAYOUT, part, "sldLayoutIdLst")?;
    }
    remove_part(package, part)
}

/// Master part `master` removed with its layouts (`layouts`) and its theme
/// (unless a master in `others` has it too).
fn remove_master(package: &mut Package, presentation: &str, master: &str, layouts: &[String], others: &[String]) -> Result<(), Error> {
    unlist(package, presentation, REL_SLIDE_MASTER, master, "sldMasterIdLst")?;
    let theme = theme_of(package, master)?;
    for layout in layouts {
        remove_part(package, layout)?;
    }
    remove_part(package, master)?;
    if let Some(theme) = theme {
        let mut shared = false;
        for other in others {
            if other != master && theme_of(package, other)?.as_deref() == Some(theme.as_str()) {
                shared = true;
            }
        }
        if !shared {
            remove_part(package, &theme)?;
        }
    }
    Ok(())
}
