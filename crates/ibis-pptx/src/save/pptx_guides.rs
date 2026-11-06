// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ガイド線: the guides (`p15:sldGuideLst`) written into the `p:extLst` of a part's root
//! (the presentation's, a slide's, a master's or a layout's), the other
//! extensions left as they are.
use ibis_doc_model::SlideGuide;
use ibis_ooxml::{NodeId, XmlDocument};

use super::pptx_new::Prefixes;
use crate::Error;

const NS_P15: &str = "http://schemas.microsoft.com/office/powerpoint/2012/main";
/// The extension of the guides of the normal view (in `presentation.xml`).
pub(crate) const URI_PRESENTATION: &str = "{EFAFB233-063F-42B5-8137-9DF3F51BA10A}";
/// The extension of the guides of a slide master.
pub(crate) const URI_MASTER: &str = "{27BBF7A9-308A-43DC-89C8-2F10F3537804}";
/// The extension of the guides of a layout or a slide.
pub(crate) const URI_PAGE: &str = "{DCECCB84-F9BA-43D5-87BE-67443E8EF086}";

fn guides_xml(x: &Prefixes, guides: &[SlideGuide]) -> String {
    let mut s = format!("<p15:sldGuideLst xmlns:p15=\"{NS_P15}\">");
    for g in guides {
        s.push_str(&format!("<p15:guide id=\"{}\"", g.id));
        if g.horizontal {
            s.push_str(" orient=\"horz\"");
        }
        if g.pos != 0 {
            s.push_str(&format!(" pos=\"{}\"", g.pos));
        }
        if g.user_drawn {
            s.push_str(" userDrawn=\"1\"");
        }
        match g.color {
            Some(c) => s.push_str(&format!("><p15:clr><{a}srgbClr val=\"{c:06X}\"/></p15:clr></p15:guide>", a = x.a)),
            None => s.push_str("/>"),
        }
    }
    s.push_str("</p15:sldGuideLst>");
    s
}

/// The extension (`p:ext`) of `root`'s `p:extLst` that holds the guides.
fn guide_ext(doc: &XmlDocument, root: NodeId) -> Option<(NodeId, NodeId)> {
    let list = doc.elements(root).find(|&n| doc.local_name(n) == "extLst")?;
    let ext = doc.elements(list).find(|&e| doc.elements(e).any(|c| doc.local_name(c) == "sldGuideLst"))?;
    Some((list, ext))
}

/// The guides of `root` made `guides`; a new extension takes `uri`.
pub(crate) fn write(doc: &mut XmlDocument, root: NodeId, guides: &[SlideGuide], uri: &str) -> Result<(), Error> {
    let old = guide_ext(doc, root);
    let uri_kept = old.and_then(|(_, e)| doc.attr(e, "uri").map(str::to_string)).unwrap_or_else(|| uri.to_string());
    if let Some((list, ext)) = old {
        doc.detach(ext);
        if doc.elements(list).next().is_none() {
            doc.detach(list);
        }
    }
    if guides.is_empty() {
        return Ok(());
    }
    let existing = doc.elements(root).find(|&n| doc.local_name(n) == "extLst");
    let list = match existing {
        Some(l) => l,
        None => {
            let x = Prefixes::at(doc, root);
            let node = x.import(doc, &format!("<{p}extLst/>", p = x.p))?;
            let at = doc.children(root).len();
            doc.insert(root, at, node);
            node
        }
    };
    let x = Prefixes::at(doc, list);
    let ext = x.import(doc, &format!("<{p}ext uri=\"{uri_kept}\">{g}</{p}ext>", p = x.p, g = guides_xml(&x, guides)))?;
    let at = doc.children(list).len();
    doc.insert(list, at, ext);
    Ok(())
}

/// The guides of `root`'s `p:extLst` as a document holds them.
pub(crate) fn read(doc: &XmlDocument, root: NodeId) -> Vec<SlideGuide> {
    let Some((_, ext)) = guide_ext(doc, root) else { return Vec::new() };
    let Some(list) = doc.elements(ext).find(|&c| doc.local_name(c) == "sldGuideLst") else { return Vec::new() };
    doc.elements(list)
        .filter(|&g| doc.local_name(g) == "guide")
        .map(|g| {
            let color = doc.elements(g).find(|&c| doc.local_name(c) == "clr").and_then(|c| doc.elements(c).find(|&n| doc.local_name(n) == "srgbClr")).and_then(|n| doc.attr(n, "val")).and_then(|v| u32::from_str_radix(v, 16).ok());
            SlideGuide {
                id: doc.attr(g, "id").and_then(|v| v.parse().ok()).unwrap_or(0),
                horizontal: doc.attr(g, "orient") == Some("horz"),
                pos: doc.attr(g, "pos").and_then(|v| v.parse().ok()).unwrap_or(0),
                color,
                user_drawn: matches!(doc.attr(g, "userDrawn"), Some("1" | "true")),
            }
        })
        .collect()
}
