// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! コメント: a slide's comments written as PowerPoint's threaded comments
//! (`p188:cmLst`, [MS-PPTX]), as PowerPoint saves them (see the samples
//! test/editing/pptx-comments-modern.pptx): the slide's comments part
//! (`ppt/comments/modernComment_<sldId>_<creationId>.xml`, hexadecimal),
//! related from the slide and named in its `p:extLst`
//! (`p188:commentRel`), and the authors in `ppt/authors.xml`, related from
//! the presentation. The part is written whole from the model; a slide's
//! older comments (`p:cmLst`) are not rewritten.

use ibis_doc_model::{Author, SlideComment};
use ibis_ooxml::{NodeId, Package, XmlDocument};

use super::pptx::{next_part_name, relative};
use super::pptx_new::{Prefixes, escape};
use crate::Error;
use crate::comments::{REL_AUTHORS_MODERN, REL_COMMENTS_LEGACY, REL_COMMENTS_MODERN};
use crate::save::unsupported;

const NS_P188: &str = "http://schemas.microsoft.com/office/powerpoint/2018/8/main";
const NS_PC: &str = "http://schemas.microsoft.com/office/powerpoint/2013/main/command";
const NS_AC: &str = "http://schemas.microsoft.com/office/drawing/2013/main/command";
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CT_COMMENTS: &str = "application/vnd.ms-powerpoint.comments+xml";
const CT_AUTHORS: &str = "application/vnd.ms-powerpoint.authors+xml";
/// The slide's `p:ext` naming its comments part.
const EXT_COMMENTS: &str = "{6950BFC3-D8DA-4A85-94F7-54DA5524770B}";
/// A comment mark's place in `pos` units (1/8 point, see `comments.rs`).
const POS_EMU: f64 = 1587.5;

/// What the writer needs to know of a shape a comment is on: its drawing
/// id (`cNvPr/@id`) and creation id (`a16:creationId`), if it has one.
#[derive(Clone, Debug)]
pub(crate) struct ShapeMark {
    pub id: String,
    pub creation: Option<String>,
}

/// A GUID made from `seed`, the same for the same seed (saving twice gives
/// the same file): two FNV-1a hashes, in the form of a version 4 GUID.
fn guid(seed: &str) -> String {
    let hash = |offset: u64| seed.bytes().fold(offset, |h, b| (h ^ b as u64).wrapping_mul(0x100000001b3));
    let (a, b) = (hash(0xcbf29ce484222325), hash(0x84222325cbf29ce4));
    format!(
        "{{{:08X}-{:04X}-4{:03X}-{:04X}-{:012X}}}",
        a >> 32,
        (a >> 16) & 0xffff,
        a & 0xfff,
        0x8000 | (b >> 48) & 0x3fff,
        b & 0xffff_ffff_ffff
    )
}

/// The first relationship of `kind` from `source`: its id and target part.
fn related(package: &Package, source: &str, kind: &str) -> Result<Option<(String, String)>, Error> {
    Ok(package.relationships(source)?.into_iter().find(|r| r.kind == kind).and_then(|r| Some((r.id.clone(), r.target_part(source)?))))
}

/// The presentation's authors part, made if it has none.
fn authors_part(package: &mut Package, presentation: &str) -> Result<String, Error> {
    if let Some((_, part)) = related(package, presentation, REL_AUTHORS_MODERN)? {
        return Ok(part);
    }
    let part = "ppt/authors.xml".to_string();
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<p188:authorLst xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p188=\"{NS_P188}\"/>"
    );
    package.set_xml(&part, XmlDocument::parse(xml.as_bytes()).map_err(|e| Error::new(format!("作者の一覧を作れません: {e}")))?);
    package.set_override(&part, CT_AUTHORS)?;
    package.add_relationship(presentation, REL_AUTHORS_MODERN, &relative(presentation, &part), false)?;
    Ok(part)
}

/// The id of `author` in the authors part (by name and initials), added
/// if it is not there.
fn author_id(package: &mut Package, part: &str, author: &Author) -> Result<String, Error> {
    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{part} が空です")))?;
    let found = doc
        .elements(root)
        .filter(|&a| doc.local_name(a) == "author")
        .find(|&a| doc.attr(a, "name") == Some(author.name.as_str()) && doc.attr(a, "initials").unwrap_or("") == author.initials)
        .and_then(|a| doc.attr(a, "id").map(str::to_string));
    if let Some(id) = found {
        return Ok(id);
    }
    let id = guid(&format!("author\u{0}{}\u{0}{}", author.name, author.initials));
    // Made in place (in the list's own namespace), as PowerPoint writes it.
    let prefix = doc.name(root).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();
    let end = doc.children(root).len();
    let node = doc.insert_element(root, end, &format!("{prefix}author"));
    for (key, value) in [("id", id.as_str()), ("name", author.name.as_str()), ("initials", author.initials.as_str()), ("userId", author.name.as_str()), ("providerId", "None")] {
        doc.set_attr(node, key, value);
    }
    Ok(id)
}

/// A comment's or answer's text body: one paragraph of one run, its line
/// breaks a CR LF in the text, as PowerPoint writes them (Enter and
/// Shift+Enter alike; test/editing/pptx-comment-newline-*.pptx).
fn body(text: &str) -> String {
    let text = text.split('\n').map(escape).collect::<Vec<_>>().join("\r\n");
    let run = if text.is_empty() { String::new() } else { format!("<a:r><a:rPr lang=\"ja-JP\" altLang=\"en-US\"/><a:t>{text}</a:t></a:r>") };
    format!("<p188:txBody><a:bodyPr/><a:lstStyle/><a:p>{run}</a:p></p188:txBody>")
}

/// A date as the comments part writes it: without a zone letter.
fn created(date: &str) -> String {
    escape(date.trim_end_matches('Z'))
}

/// Write `comments` as slide `slide`'s comments (slide `sld_id` of the
/// presentation part `presentation`, creation id `cid`); `mark` tells the
/// shapes they are on.
pub(crate) fn write_comments(
    package: &mut Package,
    presentation: &str,
    slide: &str,
    sld_id: u32,
    cid: u32,
    comments: &[SlideComment],
    mark: &dyn Fn(ibis_doc_model::ShapeId) -> Option<ShapeMark>,
) -> Result<(), Error> {
    if related(package, slide, REL_COMMENTS_LEGACY)?.is_some() {
        return Err(unsupported("旧形式のコメントのあるスライドのコメントの変更"));
    }
    let existing = related(package, slide, REL_COMMENTS_MODERN)?;
    if comments.is_empty() {
        if let Some((rel, part)) = existing {
            package.remove_relationship(slide, &rel)?;
            package.remove(&part);
            package.remove_override(&part)?;
            remove_slide_ext(package, slide)?;
        }
        return Ok(());
    }
    let authors = authors_part(package, presentation)?;
    let mut items = String::new();
    for (i, c) in comments.iter().enumerate() {
        let id = if c.id.starts_with('{') { c.id.clone() } else { guid(&format!("comment\u{0}{slide}\u{0}{i}\u{0}{}\u{0}{}", c.created, c.text)) };
        let author = author_id(package, &authors, &c.author)?;
        let shape = c.shape.and_then(mark);
        let anchor = match &shape {
            Some(m) => format!(
                "<ac:deMkLst xmlns:ac=\"{NS_AC}\"><pc:docMk xmlns:pc=\"{NS_PC}\"/><pc:sldMk xmlns:pc=\"{NS_PC}\" cId=\"{cid}\" sldId=\"{sld_id}\"/><ac:spMk id=\"{}\"{}/></ac:deMkLst>",
                escape(&m.id),
                m.creation.as_ref().map(|c| format!(" creationId=\"{}\"", escape(c))).unwrap_or_default()
            ),
            None => format!("<pc:sldMkLst xmlns:pc=\"{NS_PC}\"><pc:docMk/><pc:sldMk cId=\"{cid}\" sldId=\"{sld_id}\"/></pc:sldMkLst>"),
        };
        let pos = match (&shape, c.position) {
            (None, Some(p)) => format!("<p188:pos x=\"{}\" y=\"{}\"/>", (p.x as f64 / POS_EMU).round() as i64, (p.y as f64 / POS_EMU).round() as i64),
            _ => String::new(),
        };
        let mut replies = String::new();
        for (j, r) in c.replies.iter().enumerate() {
            let rid = if r.id.starts_with('{') { r.id.clone() } else { guid(&format!("reply\u{0}{id}\u{0}{j}\u{0}{}\u{0}{}", r.created, r.text)) };
            let rauthor = author_id(package, &authors, &r.author)?;
            replies.push_str(&format!("<p188:reply id=\"{rid}\" authorId=\"{rauthor}\" created=\"{}\">{}</p188:reply>", created(&r.created), body(&r.text)));
        }
        let replies = if replies.is_empty() { String::new() } else { format!("<p188:replyLst>{replies}</p188:replyLst>") };
        let status = if c.resolved { " status=\"resolved\"" } else { "" };
        items.push_str(&format!(
            "<p188:cm id=\"{id}\" authorId=\"{author}\"{status} created=\"{}\">{anchor}{pos}{replies}{}</p188:cm>",
            created(&c.created),
            body(&c.text)
        ));
    }
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<p188:cmLst xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" xmlns:p188=\"{NS_P188}\">{items}</p188:cmLst>"
    );
    // Written as bytes: parsed, its CR LF would become LF.
    XmlDocument::parse(xml.as_bytes()).map_err(|e| Error::new(format!("コメントを書けません: {e}")))?;
    match existing {
        Some((_, part)) => package.set_bytes(&part, xml.into_bytes()),
        None => {
            let name = format!("ppt/comments/modernComment_{sld_id:X}_{cid:X}.xml");
            let part = if package.contains(&name) { next_part_name(package, &name) } else { name };
            package.set_bytes(&part, xml.into_bytes());
            package.set_override(&part, CT_COMMENTS)?;
            let rel = package.add_relationship(slide, REL_COMMENTS_MODERN, &relative(slide, &part), false)?;
            add_slide_ext(package, slide, &rel)?;
        }
    }
    Ok(())
}

/// The slide's `p:extLst`, made (last of the slide) if it has none.
fn ext_list(doc: &mut XmlDocument, root: NodeId) -> Result<NodeId, Error> {
    if let Some(list) = doc.child(root, "extLst") {
        return Ok(list);
    }
    let x = Prefixes::at(doc, root);
    let list = x.import(doc, &format!("<{p}extLst/>", p = x.p))?;
    let end = doc.children(root).len();
    doc.insert(root, end, list);
    Ok(list)
}

/// `p:ext` naming the slide's comments part (`p188:commentRel`), first in
/// its extension list as PowerPoint writes it.
fn add_slide_ext(package: &mut Package, slide: &str, rel: &str) -> Result<(), Error> {
    let doc = package.xml_mut(slide)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{slide} が空です")))?;
    let list = ext_list(doc, root)?;
    let x = Prefixes::at(doc, list);
    let xml = format!("<{p}ext uri=\"{EXT_COMMENTS}\"><p188:commentRel xmlns:p188=\"{NS_P188}\" {r}id=\"{}\"/></{p}ext>", escape(rel), p = x.p, r = x.r);
    let ext = x.import(doc, &xml)?;
    doc.insert(list, 0, ext);
    Ok(())
}

/// The slide's comments `p:ext` taken out (and an extension list left empty).
fn remove_slide_ext(package: &mut Package, slide: &str) -> Result<(), Error> {
    let doc = package.xml_mut(slide)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{slide} が空です")))?;
    let Some(list) = doc.child(root, "extLst") else { return Ok(()) };
    let exts: Vec<NodeId> = doc.elements(list).filter(|&e| doc.attr(e, "uri") == Some(EXT_COMMENTS)).collect();
    for e in exts {
        doc.detach(e);
    }
    if doc.elements(list).next().is_none() {
        doc.detach(list);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_breaks_are_cr_lf_in_one_run() {
        assert_eq!(body("1 行目\n2 行目"), "<p188:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"ja-JP\" altLang=\"en-US\"/><a:t>1 行目\r\n2 行目</a:t></a:r></a:p></p188:txBody>");
    }

    #[test]
    fn guids_are_made_the_same_from_the_same_seed() {
        let g = guid("a");
        assert_eq!(g, guid("a"));
        assert_ne!(g, guid("b"));
        assert_eq!(g.len(), 38);
        assert_eq!(&g[15..16], "4");
    }
}
