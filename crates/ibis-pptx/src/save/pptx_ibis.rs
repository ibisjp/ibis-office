// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibisOffice's own features PowerPoint does not have (text wrap round pictures and videos, linked text boxes), kept in
//! extensions (`ext`) of the part's `extLst` that PowerPoint ignores (an `ext` with an unknown `uri` is skipped; it is
//! saved back as it is).
//!
//! * `presentation.xml`: `<ibis:features/>` says the features are on for the file (the user answered the question once).
//! * a shape's `cNvPr/a:extLst`: `<ibis:shape/>` (its wrap round it, whether its text ignores the wrap, its place in a chain).
use ibis_doc_model::{IbisShapeExt, ObjectWrap, ObjectWrapKind};
use ibis_ooxml::{NodeId, XmlDocument};

use super::pptx_new::Prefixes;
use crate::Error;

/// The namespace of ibisOffice's own elements.
pub(crate) const NS_IBIS: &str = "https://ibisoffice.jp/ns/office/2026";
/// The `uri` of ibisOffice's extension in `presentation.xml`.
pub(crate) const URI_FEATURES: &str = "{1B15C0DE-0001-4B15-9E77-1B15A0F0C001}";
/// The `uri` of ibisOffice's extension in a shape's `cNvPr`.
pub(crate) const URI_SHAPE: &str = "{1B15C0DE-0002-4B15-9E77-1B15A0F0C002}";

/// The `ext` of `holder`'s `extLst` with `uri`.
pub(crate) fn find_ext(doc: &XmlDocument, holder: NodeId, uri: &str) -> Option<(NodeId, NodeId)> {
    let list = doc.elements(holder).find(|&n| doc.local_name(n) == "extLst")?;
    let ext = doc.elements(list).find(|&e| doc.local_name(e) == "ext" && doc.attr(e, "uri") == Some(uri))?;
    Some((list, ext))
}

/// `holder`'s extension `uri` made to hold `inner` (an element in ibisOffice's namespace, written with its own `xmlns:ibis`);
/// `None` removes it (and the `extLst` when it is then empty). `holder` is the part's root or a `cNvPr`; a new `extLst` goes
/// where the schema puts it: last.
pub(crate) fn set_ext(doc: &mut XmlDocument, holder: NodeId, uri: &str, inner: Option<&str>) -> Result<(), Error> {
    if let Some((list, ext)) = find_ext(doc, holder, uri) {
        doc.detach(ext);
        if doc.elements(list).next().is_none() {
            doc.detach(list);
        }
    }
    let Some(inner) = inner else { return Ok(()) };
    let existing = doc.elements(holder).find(|&n| doc.local_name(n) == "extLst");
    let list = match existing {
        Some(l) => l,
        None => {
            let x = Prefixes::at(doc, holder);
            // (The `a:` extLst of a `cNvPr`; the `p:` one of a part's root: the prefix of the holder's own namespace.)
            let prefix = if doc.local_name(holder) == "cNvPr" { x.a.clone() } else { x.p.clone() };
            let node = x.import(doc, &format!("<{prefix}extLst/>"))?;
            let at = doc.children(holder).len();
            doc.insert(holder, at, node);
            node
        }
    };
    let x = Prefixes::at(doc, list);
    let prefix = if doc.local_name(holder) == "cNvPr" { x.a.clone() } else { x.p.clone() };
    let ext = x.import(doc, &format!("<{prefix}ext uri=\"{uri}\">{inner}</{prefix}ext>"))?;
    let at = doc.children(list).len();
    doc.insert(list, at, ext);
    Ok(())
}

/// The file's ibisOffice features made on (`true`) or off.
pub(crate) fn write_features(doc: &mut XmlDocument, root: NodeId, enabled: bool) -> Result<(), Error> {
    let inner = format!("<ibis:features xmlns:ibis=\"{NS_IBIS}\" version=\"1\"/>");
    set_ext(doc, root, URI_FEATURES, enabled.then_some(inner.as_str()))
}

/// A shape's extension read from the attributes of its `ibis:shape` (`attr` gives one by name).
pub(crate) fn shape_ext_from<'a>(attr: impl Fn(&str) -> Option<&'a str>) -> IbisShapeExt {
    let kind = match attr("wrap") {
        Some("square") => ObjectWrapKind::Square,
        Some("topBottom") => ObjectWrapKind::TopBottom,
        _ => ObjectWrapKind::None,
    };
    let num = |name: &str| attr(name).and_then(|v| v.parse::<u32>().ok());
    let flag = |name: &str| matches!(attr(name), Some("1" | "true"));
    IbisShapeExt {
        wrap: ObjectWrap {
            kind,
            // (`margin` alone: one space for the four sides.)
            margins: ["marginL", "marginT", "marginR", "marginB"].map(|name| attr(name).or_else(|| attr("margin")).and_then(|v| v.parse().ok()).unwrap_or(ObjectWrap::DEFAULT_MARGIN)),
        },
        ignore_wrap: flag("ignoreWrap"),
        chain: num("chain").zip(num("order")).map(|(chain, order)| ibis_doc_model::ibis_text::ChainLink { chain, order, joined: flag("joined"), padding: flag("padding") }),
    }
}

/// The `ibis:shape` element for `ext`.
fn shape_xml(ext: &IbisShapeExt) -> String {
    let mut s = format!("<ibis:shape xmlns:ibis=\"{NS_IBIS}\"");
    match ext.wrap.kind {
        ObjectWrapKind::None => {}
        ObjectWrapKind::Square | ObjectWrapKind::TopBottom => {
            let [l, t, r, b] = ext.wrap.margins;
            let name = if ext.wrap.kind == ObjectWrapKind::Square { "square" } else { "topBottom" };
            s.push_str(&format!(" wrap=\"{name}\" marginL=\"{l}\" marginT=\"{t}\" marginR=\"{r}\" marginB=\"{b}\""));
        }
    }
    if ext.ignore_wrap {
        s.push_str(" ignoreWrap=\"1\"");
    }
    if let Some(link) = ext.chain {
        s.push_str(&format!(" chain=\"{}\" order=\"{}\"", link.chain, link.order));
        if link.joined {
            s.push_str(" joined=\"1\"");
        }
        if link.padding {
            s.push_str(" padding=\"1\"");
        }
    }
    s.push_str("/>");
    s
}

/// A shape's (its `cNvPr`'s) extension made `ext` (`None`: taken away).
pub(crate) fn write_shape_ext(doc: &mut XmlDocument, c_nv_pr: NodeId, ext: Option<&IbisShapeExt>) -> Result<(), Error> {
    let inner = ext.map(shape_xml);
    set_ext(doc, c_nv_pr, URI_SHAPE, inner.as_deref())
}
