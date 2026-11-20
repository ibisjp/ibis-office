// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Reading a slide's comments (コメント): PowerPoint's threaded comments
//! (`p188:cmLst`, [MS-PPTX] 2.3: a slide's `modernComment_*.xml`, authors
//! in `authors.xml`), with answers and a resolved state, and the older
//! ones (`p:cmLst`, ECMA-376 Part 1, 19.4: `commentN.xml`, authors in
//! `commentAuthors.xml`).

use std::collections::HashMap;

use ibis_doc_model::{Author, CommentReply, EmuPoint, ShapeId, SlideComment};
use roxmltree::{Document, Node};

use crate::package::{Package, Relationship};
use crate::xml::elements;

pub(crate) const REL_COMMENTS_LEGACY: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
pub(crate) const REL_COMMENTS_MODERN: &str = "http://schemas.microsoft.com/office/2018/10/relationships/comments";
pub(crate) const REL_AUTHORS_LEGACY: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/commentAuthors";
pub(crate) const REL_AUTHORS_MODERN: &str = "http://schemas.microsoft.com/office/2018/10/relationships/authors";

/// A comment mark's place (`pos`) is in 1/8 points: this many EMU per
/// two of them (12 700 / 8 = 1587.5). The samples PowerPoint saved
/// (test/editing/pptx-comments-*.pptx) do not tell the unit apart; not
/// checked against PowerPoint's own placing.
const POS_EMU: i64 = 3175;

/// The comments' authors: the threaded ones by id, the older ones by
/// their number.
#[derive(Default)]
pub(crate) struct Authors {
    modern: HashMap<String, Author>,
    legacy: HashMap<String, Author>,
}

/// The authors' parts of the presentation (`main_rels`: its relationships).
pub(crate) fn authors(package: &Package, main_rels: &HashMap<String, Relationship>) -> Authors {
    let mut out = Authors::default();
    for rel in main_rels.values().filter(|r| !r.external) {
        let modern = rel.kind == REL_AUTHORS_MODERN;
        if !modern && rel.kind != REL_AUTHORS_LEGACY {
            continue;
        }
        let Ok(text) = package.read_xml(&rel.target) else { continue };
        let Ok(doc) = Document::parse(&text) else { continue };
        for a in doc.root_element().children().filter(|n| n.is_element() && matches!(n.tag_name().name(), "author" | "cmAuthor")) {
            let Some(id) = a.attribute("id") else { continue };
            let author = Author { name: a.attribute("name").unwrap_or("").to_string(), initials: a.attribute("initials").unwrap_or("").to_string() };
            if modern { out.modern.insert(id.to_string(), author) } else { out.legacy.insert(id.to_string(), author) };
        }
    }
    out
}

/// A comment's text: its paragraphs (`a:p`, or `p:text` for an older one)
/// joined by `\n`.
fn text(node: Node) -> String {
    // PowerPoint's line breaks are CR LF in the text (an escaped CR stays
    // one when read): one `\n` each.
    raw_text(node).replace("\r\n", "\n").replace('\r', "\n")
}

fn raw_text(node: Node) -> String {
    if let Some(t) = elements(node).find(|n| n.tag_name().name() == "text") {
        return t.text().unwrap_or("").to_string();
    }
    let Some(body) = elements(node).find(|n| n.tag_name().name() == "txBody") else { return String::new() };
    elements(body)
        .filter(|p| p.tag_name().name() == "p")
        .map(|p| {
            p.descendants()
                .filter(|n| n.is_element() && matches!(n.tag_name().name(), "t" | "br"))
                .map(|n| if n.tag_name().name() == "br" { "\n".to_string() } else { n.text().unwrap_or("").to_string() })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The comments of slide part `slide`, their shapes by the slide's drawing
/// ids (`native_ids`).
pub(crate) fn read(package: &Package, slide: &str, authors: &Authors, native_ids: &HashMap<String, ShapeId>) -> Vec<SlideComment> {
    let Ok(rels) = package.relationships(slide) else { return Vec::new() };
    let mut rels: Vec<_> = rels.into_iter().filter(|(_, r)| !r.external && (r.kind == REL_COMMENTS_MODERN || r.kind == REL_COMMENTS_LEGACY)).collect();
    rels.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = Vec::new();
    for (_, rel) in rels {
        let modern = rel.kind == REL_COMMENTS_MODERN;
        let Ok(xml) = package.read_xml(&rel.target) else { continue };
        let Ok(doc) = Document::parse(&xml) else { continue };
        for cm in doc.root_element().children().filter(|n| n.is_element() && n.tag_name().name() == "cm") {
            let author_of = |n: Node| -> Author {
                let id = n.attribute("authorId").unwrap_or("");
                (if modern { authors.modern.get(id) } else { authors.legacy.get(id) }).cloned().unwrap_or_default()
            };
            let position = elements(cm)
                .find(|n| n.tag_name().name() == "pos")
                .and_then(|p| Some(EmuPoint::new(p.attribute("x")?.parse::<i64>().ok()? * POS_EMU / 2, p.attribute("y")?.parse::<i64>().ok()? * POS_EMU / 2)));
            // An older comment's answer is a comment naming its parent.
            if !modern
                && let Some(parent) = cm.descendants().find(|n| n.is_element() && n.tag_name().name() == "parentCm")
            {
                let key = format!("{}:{}", parent.attribute("authorId").unwrap_or(""), parent.attribute("idx").unwrap_or(""));
                if let Some(p) = out.iter_mut().find(|c: &&mut SlideComment| c.id == key) {
                    let id = format!("{}:{}", cm.attribute("authorId").unwrap_or(""), cm.attribute("idx").unwrap_or(""));
                    p.replies.push(CommentReply { id, author: author_of(cm), created: cm.attribute("dt").unwrap_or("").to_string(), text: text(cm) });
                    continue;
                }
            }
            // A threaded comment on a shape names it by its drawing id.
            let shape = cm
                .descendants()
                .find(|n| n.is_element() && n.tag_name().name() == "spMk")
                .and_then(|n| n.attribute("id"))
                .and_then(|id| native_ids.get(id).copied());
            let replies = elements(cm)
                .find(|n| n.tag_name().name() == "replyLst")
                .into_iter()
                .flat_map(elements)
                .filter(|r| r.tag_name().name() == "reply")
                .map(|r| CommentReply { id: r.attribute("id").unwrap_or("").to_string(), author: author_of(r), created: r.attribute("created").unwrap_or("").to_string(), text: text(r) })
                .collect();
            out.push(SlideComment {
                id: if modern { cm.attribute("id").unwrap_or("").to_string() } else { format!("{}:{}", cm.attribute("authorId").unwrap_or(""), cm.attribute("idx").unwrap_or("")) },
                author: author_of(cm),
                created: cm.attribute(if modern { "created" } else { "dt" }).unwrap_or("").to_string(),
                text: text(cm),
                position,
                shape,
                resolved: cm.attribute("status") == Some("resolved"),
                replies,
            });
        }
    }
    out
}
