// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ノートの文字の書式: a slide's notes page edited as a slide. `build` makes,
//! from a presentation's file, another one holding one slide, the notes page
//! (its size the notes page's, its body the slide's text, as the notes master
//! styles it), which the ordinary editor opens; `apply` puts the paragraphs of
//! its body, as edited, back into the notes page of the file (made if the slide
//! had none), and the frames of its slide image and body, and its other
//! shapes (pictures, boxes, lines). What else the notes page holds is left as it was.

use ibis_ooxml::{NodeId, Package, SaveOptions, XmlDocument};

use crate::Error;
use crate::save::pptx::{next_part_name, relative};
use crate::save::pptx_new::Prefixes;

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_OFFICE_DOCUMENT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument";
const REL_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
const REL_SLIDE_LAYOUT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
const REL_SLIDE_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
const REL_NOTES_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide";
const REL_NOTES_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster";
const CT_SLIDE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";

fn err(what: impl Into<String>) -> Error {
    Error::new(what)
}

pub(crate) fn rel_id(doc: &XmlDocument, node: NodeId) -> Option<String> {
    doc.attributes(node).iter().find(|a| a.name.rsplit_once(':').is_some_and(|(p, l)| l == "id" && doc.resolve_prefix(node, Some(p)) == Some(NS_R))).map(|a| a.value.clone())
}

pub(crate) fn main_part(package: &Package) -> Result<String, Error> {
    package.relationships("")?.into_iter().find(|r| r.kind == REL_OFFICE_DOCUMENT && !r.external).and_then(|r| r.target_part("")).ok_or_else(|| err("プレゼンテーションの本体がありません"))
}

/// The slide part number `index` of the presentation.
pub(crate) fn slide_part(package: &Package, main: &str, index: usize) -> Result<String, Error> {
    let doc = package.xml(main)?;
    let root = doc.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
    let list = doc.child(root, "sldIdLst").ok_or_else(|| err("スライドの一覧がありません"))?;
    let entry = doc.elements(list).nth(index).ok_or_else(|| err("そのスライドはありません"))?;
    let rid = rel_id(&doc, entry).ok_or_else(|| err("スライドの関係がありません"))?;
    package.relationships(main)?.into_iter().find(|r| r.id == rid).and_then(|r| r.target_part(main)).ok_or_else(|| err("スライドの部品がありません"))
}

fn notes_of(package: &Package, slide: &str) -> Result<Option<String>, Error> {
    Ok(package.relationships(slide)?.into_iter().find(|r| r.kind == REL_NOTES_SLIDE && !r.external).and_then(|r| r.target_part(slide)))
}

pub(crate) fn rels_part(part: &str) -> String {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") }
}

/// The body placeholder (`p:sp` with `p:ph type="body"`) of a notes tree, and its `idx`.
fn body_of(doc: &XmlDocument, tree: NodeId) -> Option<(NodeId, Option<String>)> {
    doc.children_named(tree, "sp").find_map(|sp| {
        let ph = doc.path(sp, &["nvSpPr", "nvPr", "ph"])?;
        (doc.attr(ph, "type") == Some("body")).then(|| (sp, doc.attr(ph, "idx").map(str::to_string)))
    })
}

pub(crate) fn xml_of(node: NodeId, doc: &XmlDocument) -> String {
    // The element as text, for copying a few of the notes master's pieces into a new tree.
    let mut out = String::new();
    fn write(doc: &XmlDocument, node: NodeId, out: &mut String) {
        if !doc.is_element(node) {
            out.push_str(&doc.text(node).replace('&', "&amp;").replace('<', "&lt;"));
            return;
        }
        out.push('<');
        out.push_str(doc.name(node));
        for a in doc.attributes(node) {
            out.push_str(&format!(" {}=\"{}\"", a.name, a.value.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;")));
        }
        if doc.children(node).is_empty() {
            out.push_str("/>");
        } else {
            out.push('>');
            for &c in doc.children(node) {
                write(doc, c, out);
            }
            out.push_str("</");
            out.push_str(doc.name(node));
            out.push('>');
        }
    }
    write(doc, node, &mut out);
    out
}

/// A file like `data` with one slide: the notes page of slide `index` (made up if the slide has none).
pub fn build(data: &[u8], index: usize) -> Result<Vec<u8>, Error> {
    let mut package = Package::open(data.to_vec())?;
    let main = main_part(&package)?;
    let slide = slide_part(&package, &main, index)?;
    let notes = notes_of(&package, &slide)?;
    // The notes master: where the body goes and how its text looks, and the page's size.
    let master = package.relationships(&main)?.into_iter().find(|r| r.kind == REL_NOTES_MASTER && !r.external).and_then(|r| r.target_part(&main));
    let (mut body_xfrm, mut image_xfrm, mut notes_style) = (String::new(), String::new(), String::new());
    if let Some(m) = master.as_ref().filter(|m| package.contains(m)) {
        let doc = package.xml(m)?;
        if let Some(root) = doc.root_element() {
            if let Some(tree) = doc.path(root, &["cSld", "spTree"]) {
                for sp in doc.children_named(tree, "sp") {
                    let Some(ph) = doc.path(sp, &["nvSpPr", "nvPr", "ph"]) else { continue };
                    let xfrm = doc.path(sp, &["spPr", "xfrm"]).map(|x| xml_of(x, &doc)).unwrap_or_default();
                    match doc.attr(ph, "type") {
                        Some("body") => body_xfrm = xfrm,
                        Some("sldImg") => image_xfrm = xfrm,
                        _ => {}
                    }
                }
            }
            if let Some(style) = doc.child(root, "notesStyle") {
                notes_style = doc.elements(style).filter(|&l| doc.local_name(l).starts_with("lvl")).map(|l| xml_of(l, &doc)).collect();
            }
        }
    }
    let main_doc = package.xml(&main)?;
    let main_root = main_doc.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
    let notes_size = main_doc.child(main_root, "notesSz").map(|n| (main_doc.attr(n, "cx").unwrap_or("6858000").to_string(), main_doc.attr(n, "cy").unwrap_or("9144000").to_string())).unwrap_or(("6858000".into(), "9144000".into()));
    // The slide: the notes page as a `p:sld`, or one made up.
    let temp = next_part_name(&package, "ppt/slides/slide1.xml");
    let mut doc;
    let mut body_idx = "1".to_string();
    match &notes {
        Some(n) => {
            doc = package.xml(n)?;
            let root = doc.root_element().ok_or_else(|| err("ノートが空です"))?;
            let prefix = doc.prefix(root).map(|p| format!("{p}:")).unwrap_or_default();
            doc.rename(root, &format!("{prefix}sld"));
            if let Some(tree) = doc.path(root, &["cSld", "spTree"])
                && let Some((_, idx)) = body_of(&doc, tree)
                && let Some(idx) = idx
            {
                body_idx = idx;
            }
        }
        None => {
            doc = XmlDocument::parse(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/><p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"スライド イメージ プレースホルダー 1\"/><p:cNvSpPr><a:spLocks noGrp=\"1\" noRot=\"1\" noChangeAspect=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"sldImg\"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp><p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"ノート プレースホルダー 2\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph type=\"body\" idx=\"1\"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang=\"ja-JP\" altLang=\"en-US\"/></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>".as_bytes(),
            )
            .map_err(|e| err(format!("ノートを作れません: {e}")))?;
        }
    }
    let root = doc.root_element().ok_or_else(|| err("ノートが空です"))?;
    // No shapes of the master and layout under it; white.
    doc.set_attr(root, "showMasterSp", "0");
    for child in doc.children(root).to_vec() {
        if doc.is_element(child) && matches!(doc.local_name(child), "clrMapOvr") {
            doc.detach(child);
        }
    }
    if let Some(csld) = doc.child(root, "cSld") {
        if doc.child(csld, "bg").is_none() {
            let x = Prefixes::at(&doc, csld);
            let bg = x.import(&mut doc, &format!("<{p}bg><{p}bgPr><{a}solidFill><{a}srgbClr val=\"FFFFFF\"/></{a}solidFill><{a}effectLst/></{p}bgPr></{p}bg>", p = x.p, a = x.a))?;
            doc.insert(csld, 0, bg);
        }
    }
    package.set_xml(&temp, doc);
    package.set_override(&temp, CT_SLIDE)?;
    // The notes page's own relationships (a hyperlink or a picture in it) keep their ids.
    if let Some(n) = &notes {
        for rel in package.relationships(n)?.into_iter().filter(|r| r.kind != REL_NOTES_MASTER && r.kind != REL_SLIDE) {
            let target = if rel.external { rel.target.clone() } else { match rel.target_part(n) { Some(t) => relative(&temp, &t), None => continue } };
            add_relationship_with_id(&mut package, &temp, &rel.id, &rel.kind, &target, rel.external)?;
        }
    }
    // The layout the slide follows: the first one, its shapes those of the notes master's (the page's image, the body).
    let layout = package.relationships(&slide)?.into_iter().find(|r| r.kind == REL_SLIDE_LAYOUT && !r.external).and_then(|r| r.target_part(&slide)).ok_or_else(|| err("スライドのレイアウトがありません"))?;
    {
        let ldoc = package.xml_mut(&layout)?;
        let lroot = ldoc.root_element().ok_or_else(|| err("レイアウトが空です"))?;
        let tree = ldoc.path(lroot, &["cSld", "spTree"]).ok_or_else(|| err("レイアウトの図形の木がありません"))?;
        for child in ldoc.children(tree).to_vec() {
            if ldoc.is_element(child) && !matches!(ldoc.local_name(child), "nvGrpSpPr" | "grpSpPr") {
                ldoc.detach(child);
            }
        }
        let x = Prefixes::at(ldoc, tree);
        let spp = |xfrm: &str| if xfrm.is_empty() { String::new() } else { xfrm.to_string() };
        let xml = format!(
            "<{p}sp><{p}nvSpPr><{p}cNvPr id=\"2\" name=\"スライド イメージ\"/><{p}cNvSpPr><{a}spLocks noGrp=\"1\" noRot=\"1\" noChangeAspect=\"1\"/></{p}cNvSpPr><{p}nvPr><{p}ph type=\"sldImg\"/></{p}nvPr></{p}nvSpPr><{p}spPr>{}<{a}prstGeom prst=\"rect\"><{a}avLst/></{a}prstGeom><{a}ln w=\"12700\"><{a}solidFill><{a}prstClr val=\"black\"/></{a}solidFill></{a}ln></{p}spPr></{p}sp>",
            spp(&image_xfrm),
            p = x.p,
            a = x.a
        );
        let image = x.import(ldoc, &xml)?;
        let xml = format!(
            "<{p}sp><{p}nvSpPr><{p}cNvPr id=\"3\" name=\"ノート\"/><{p}cNvSpPr><{a}spLocks noGrp=\"1\"/></{p}cNvSpPr><{p}nvPr><{p}ph type=\"body\" idx=\"{body_idx}\"/></{p}nvPr></{p}nvSpPr><{p}spPr>{}</{p}spPr><{p}txBody><{a}bodyPr/><{a}lstStyle>{notes_style}</{a}lstStyle><{a}p><{a}endParaRPr lang=\"ja-JP\"/></{a}p></{p}txBody></{p}sp>",
            spp(&body_xfrm),
            p = x.p,
            a = x.a
        );
        let body = x.import(ldoc, &xml)?;
        // Notes have no bullets of their own: the slide master's body style, which the placeholder
        // would take its bullets from, is not theirs.
        if let Some(list) = ldoc.path(body, &["txBody", "lstStyle"]) {
            let levels: Vec<NodeId> = ldoc.elements(list).filter(|&l| ldoc.local_name(l).starts_with("lvl")).collect();
            let a = x.a.clone();
            if levels.is_empty() {
                let level = x.import(ldoc, &format!("<{a}lvl1pPr marL=\"0\" indent=\"0\"><{a}buNone/></{a}lvl1pPr>"))?;
                ldoc.insert(list, 0, level);
            }
            for level in levels {
                let has_bullet = ldoc.elements(level).any(|c| ldoc.local_name(c).starts_with("bu") && ldoc.local_name(c) != "buClr" && ldoc.local_name(c) != "buSzPct" && ldoc.local_name(c) != "buFont");
                if has_bullet {
                    continue;
                }
                let none = x.import(ldoc, &format!("<{a}buNone/>"))?;
                let at = ldoc.children(level).iter().position(|&c| ldoc.is_element(c) && matches!(ldoc.local_name(c), "tabLst" | "defRPr" | "extLst")).unwrap_or(ldoc.children(level).len());
                ldoc.insert(level, at, none);
            }
        }
        let at = ldoc.children(tree).len();
        ldoc.insert(tree, at, image);
        ldoc.insert(tree, at + 1, body);
    }
    package.add_relationship(&temp, REL_SLIDE_LAYOUT, &relative(&temp, &layout), false)?;
    // The presentation: this slide only, the notes page's size.
    let rid = package.add_relationship(&main, REL_SLIDE, &relative(&main, &temp), false)?;
    let pdoc = package.xml_mut(&main)?;
    let proot = pdoc.root_element().ok_or_else(|| err("プレゼンテーションが空です"))?;
    let list = pdoc.child(proot, "sldIdLst").ok_or_else(|| err("スライドの一覧がありません"))?;
    for child in pdoc.children(list).to_vec() {
        pdoc.detach(child);
    }
    let x = Prefixes::at(pdoc, list);
    let entry = x.import(pdoc, &format!("<{p}sldId id=\"256\" {r}id=\"{rid}\"/>", p = x.p, r = x.r))?;
    pdoc.insert(list, 0, entry);
    if let Some(size) = pdoc.child(proot, "sldSz") {
        pdoc.set_attr(size, "cx", &notes_size.0);
        pdoc.set_attr(size, "cy", &notes_size.1);
        pdoc.remove_attr(size, "type");
    }
    if let Some(shows) = pdoc.child(proot, "custShowLst") {
        pdoc.detach(shows);
    }
    if let Some(ext) = pdoc.child(proot, "extLst") {
        for e in pdoc.elements(ext).collect::<Vec<_>>() {
            if pdoc.descendants(e).into_iter().any(|d| pdoc.is_element(d) && pdoc.local_name(d) == "sectionLst") {
                pdoc.detach(e);
            }
        }
    }
    let _ = REL_SLIDE_MASTER;
    Ok(package.save(SaveOptions::default())?)
}

pub(crate) fn add_relationship_with_id(package: &mut Package, source: &str, id: &str, kind: &str, target: &str, external: bool) -> Result<(), Error> {
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

/// `data` with the paragraphs of the body of slide `index`'s notes page those of the
/// slide of `edited` (a file `build` made from `data`, then edited).
pub fn apply(data: &[u8], edited: &[u8], index: usize) -> Result<Vec<u8>, Error> {
    let mut package = Package::open(data.to_vec())?;
    let edited = Package::open(edited.to_vec())?;
    // The edited page: its only slide, its body.
    let emain = main_part(&edited)?;
    let eslide = slide_part(&edited, &emain, 0)?;
    let edoc = edited.xml(&eslide)?;
    let eroot = edoc.root_element().ok_or_else(|| err("スライドが空です"))?;
    let etree = edoc.path(eroot, &["cSld", "spTree"]).ok_or_else(|| err("スライドの図形の木がありません"))?;
    let (ebody, _) = body_of(&edoc, etree).ok_or_else(|| err("ノートの本文がありません"))?;
    let etx = edoc.child(ebody, "txBody").ok_or_else(|| err("ノートの本文に文字がありません"))?;
    let paragraphs: Vec<NodeId> = edoc.children_named(etx, "p").collect();
    // The notes page of the file (made if the slide has none).
    let main = main_part(&package)?;
    let slide = slide_part(&package, &main, index)?;
    let notes = match notes_of(&package, &slide)? {
        Some(n) => n,
        None => {
            crate::save::pptx_notes::new_notes(&mut package, &main, &slide, "")?;
            notes_of(&package, &slide)?.ok_or_else(|| err("ノートを作れません"))?
        }
    };
    let mut doc = package.xml(&notes)?;
    let root = doc.root_element().ok_or_else(|| err("ノートが空です"))?;
    let tree = doc.path(root, &["cSld", "spTree"]).ok_or_else(|| err("ノートの図形の木がありません"))?;
    let (body, _) = body_of(&doc, tree).ok_or_else(|| err("本文のプレースホルダーの無いノートです"))?;
    let tx = doc.child(body, "txBody").ok_or_else(|| err("ノートの本文に文字がありません"))?;
    for p in doc.children_named(tx, "p").collect::<Vec<_>>() {
        doc.detach(p);
    }
    let erels = edited.relationships(&eslide)?;
    let mut mapped: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut added: Vec<(String, String, String, bool)> = Vec::new();
    for p in &paragraphs {
        let copy = doc.import(&edoc, *p);
        let end = doc.children(tx).len();
        doc.insert(tx, end, copy);
        for node in std::iter::once(copy).chain(doc.descendants(copy)) {
            if !doc.is_element(node) {
                continue;
            }
            let names: Vec<(String, String)> = doc.attributes(node).iter().filter(|a| a.name.rsplit_once(':').is_some_and(|(pre, _)| doc.resolve_prefix(node, Some(pre)) == Some(NS_R))).map(|a| (a.name.clone(), a.value.clone())).collect();
            for (name, value) in names {
                let Some(rel) = erels.iter().find(|r| r.id == value) else { continue };
                let id = mapped.entry(value.clone()).or_insert_with(|| {
                    let id = format!("ibis-new-{}", added.len());
                    added.push((id.clone(), rel.kind.clone(), if rel.external { rel.target.clone() } else { rel.target_part(&eslide).unwrap_or_default() }, rel.external));
                    id
                });
                let id = id.clone();
                doc.set_attr(node, &name, &id);
            }
        }
    }
    // The slide image's and the body's places as edited, and the other shapes (pictures, boxes, lines) as edited.
    let ph_of = |d: &XmlDocument, node: NodeId| -> Option<String> {
        if d.local_name(node) != "sp" {
            return None;
        }
        d.path(node, &["nvSpPr", "nvPr", "ph"]).and_then(|ph| d.attr(ph, "type")).map(str::to_string)
    };
    let is_page_part = |d: &XmlDocument, node: NodeId| matches!(ph_of(d, node).as_deref(), Some("sldImg" | "body"));
    for esp in edoc.children(etree).to_vec() {
        if !edoc.is_element(esp) || !is_page_part(&edoc, esp) {
            continue;
        }
        let kind = ph_of(&edoc, esp).unwrap_or_default();
        let Some(exfrm) = edoc.path(esp, &["spPr", "xfrm"]) else { continue };
        let Some(sp) = doc.children_named(tree, "sp").find(|&n| ph_of(&doc, n).as_deref() == Some(kind.as_str())) else { continue };
        let sppr = match doc.child(sp, "spPr") {
            Some(s) => s,
            None => {
                let x = Prefixes::at(&doc, sp);
                let node = x.import(&mut doc, &format!("<{p}spPr/>", p = x.p))?;
                let at = doc.children(sp).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "txBody").unwrap_or(doc.children(sp).len());
                doc.insert(sp, at, node);
                node
            }
        };
        if let Some(old) = doc.child(sppr, "xfrm") {
            doc.detach(old);
        }
        let copy = doc.import(&edoc, exfrm);
        doc.insert(sppr, 0, copy);
    }
    for node in doc.children(tree).to_vec() {
        if doc.is_element(node) && !matches!(doc.local_name(node), "nvGrpSpPr" | "grpSpPr" | "extLst") && !is_page_part(&doc, node) {
            doc.detach(node);
        }
    }
    let at = doc.children(tree).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "extLst").unwrap_or(doc.children(tree).len());
    let mut put = 0;
    for shape in edoc.children(etree).to_vec() {
        if !edoc.is_element(shape) || matches!(edoc.local_name(shape), "nvGrpSpPr" | "grpSpPr" | "extLst") || is_page_part(&edoc, shape) {
            continue;
        }
        let copy = doc.import(&edoc, shape);
        doc.insert(tree, at + put, copy);
        put += 1;
        for node in std::iter::once(copy).chain(doc.descendants(copy)) {
            if !doc.is_element(node) {
                continue;
            }
            let names: Vec<(String, String)> = doc.attributes(node).iter().filter(|a| a.name.rsplit_once(':').is_some_and(|(pre, _)| doc.resolve_prefix(node, Some(pre)) == Some(NS_R))).map(|a| (a.name.clone(), a.value.clone())).collect();
            for (name, value) in names {
                let Some(rel) = erels.iter().find(|r| r.id == value) else { continue };
                let id = mapped.entry(value.clone()).or_insert_with(|| {
                    let id = format!("ibis-new-{}", added.len());
                    added.push((id.clone(), rel.kind.clone(), if rel.external { rel.target.clone() } else { rel.target_part(&eslide).unwrap_or_default() }, rel.external));
                    id
                });
                let id = id.clone();
                doc.set_attr(node, &name, &id);
            }
        }
    }
    package.set_xml(&notes, doc);
    // A link in the text: related to what it names (outside links only; a link to a slide or part is kept as the part's).
    if !added.is_empty() {
        let mut real = std::collections::HashMap::new();
        for (placeholder, kind, target, external) in added {
            // The relationship the page already has to the same thing is kept (not made again at each closing).
            let existing = package.relationships(&notes)?.into_iter().find(|r| r.kind == kind && r.external == external && if external { r.target == target } else { r.target_part(&notes).as_deref() == Some(target.as_str()) });
            let id = if let Some(r) = existing { r.id } else if external { package.add_relationship(&notes, &kind, &target, true)? } else if package.contains(&target) { package.add_relationship(&notes, &kind, &relative(&notes, &target), false)? } else { continue };
            real.insert(placeholder, id);
        }
        let doc = package.xml_mut(&notes)?;
        let root = doc.root_element().ok_or_else(|| err("ノートが空です"))?;
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
