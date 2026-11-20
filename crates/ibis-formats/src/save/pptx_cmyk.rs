// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The file's CMYK mode (ibisOffice's own; PowerPoint has none), kept in an extension (`ext`) of `presentation.xml`'s `extLst`
//! that PowerPoint ignores: the flag that the mode is on and the name and identifier of the profile it was made with (the
//! profile itself is not in the file). The colours in the body are ordinary `srgbClr`, already inside what the press prints.
//!
//! ```xml
//! <p:ext uri="{1B15C0DE-0003-4B15-9E77-1B15A0F0C003}">
//!   <ibis:cmyk xmlns:ibis="https://ibisoffice.jp/ns/office/2026" version="2" profile="JapanColor2001Coated|0123..."/>
//! </p:ext>
//! ```
//! (Version 1 files of the first builds had `ibis:color` children; they are ignored.)
use ibis_doc_model::CmykMode;
use ibis_ooxml::{NodeId, XmlDocument};

use super::pptx_ibis::{NS_IBIS, set_ext};
use crate::Error;

/// The `uri` of the CMYK extension in `presentation.xml`.
pub(crate) const URI_CMYK: &str = "{1B15C0DE-0003-4B15-9E77-1B15A0F0C003}";

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The extension's text for `mode`.
fn cmyk_xml(mode: &CmykMode) -> String {
    let profile = mode.profile.as_deref().map(|p| format!(" profile=\"{}\"", escape(p))).unwrap_or_default();
    format!("<ibis:cmyk xmlns:ibis=\"{NS_IBIS}\" version=\"2\"{profile}/>")
}

/// `presentation.xml`'s CMYK extension made to say `mode` (taken away when the mode is off). The extension helpers are the
/// ones of ibisOffice's other extensions (`pptx_ibis`).
pub(crate) fn write_cmyk(doc: &mut XmlDocument, root: NodeId, mode: &CmykMode) -> Result<(), Error> {
    let xml = cmyk_xml(mode);
    set_ext(doc, root, URI_CMYK, mode.enabled.then_some(xml.as_str()))
}

/// The CMYK mode a `presentation.xml` says (`root` is `p:presentation`); off when it has no such extension.
pub(crate) fn read_cmyk(root: roxmltree::Node<'_, '_>) -> CmykMode {
    let mut mode = CmykMode::default();
    let Some(list) = root.children().find(|n| n.is_element() && n.tag_name().name() == "extLst") else { return mode };
    for ext in list.children().filter(|n| n.is_element() && n.tag_name().name() == "ext" && n.attribute("uri") == Some(URI_CMYK)) {
        for cmyk in ext.children().filter(|n| n.is_element() && n.tag_name().name() == "cmyk" && n.tag_name().namespace() == Some(NS_IBIS)) {
            mode.enabled = true;
            mode.profile = cmyk.attribute("profile").map(str::to_string);
        }
    }
    mode
}

/// The CMYK mode a whole part's text says: any `ibis:cmyk` element in ibisOffice's namespace (a document's `settings.xml`, where
/// Word ignores it: `ibis` is in the part's `mc:Ignorable`). Off when there is none.
pub(crate) fn read_cmyk_in_text(xml: &str) -> CmykMode {
    let mut mode = CmykMode::default();
    let Ok(parsed) = roxmltree::Document::parse(xml) else { return mode };
    for node in parsed.descendants().filter(|n| n.is_element() && n.tag_name().name() == "cmyk" && n.tag_name().namespace() == Some(NS_IBIS)) {
        mode.enabled = true;
        mode.profile = node.attribute("profile").map(str::to_string);
    }
    mode
}

/// A document's `settings.xml` made to say `mode`: its `ibis:cmyk` element (the last child of `w:settings`) put, replaced or taken
/// away, and `ibis` made an ignorable namespace of the part (Word skips what it does not know, and keeps it).
pub(crate) fn write_cmyk_in_settings(doc: &mut XmlDocument, root: NodeId, mode: &CmykMode) {
    const MC_NS: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
    for c in doc.children(root).to_vec() {
        if doc.is_element(c) && doc.local_name(c) == "cmyk" && doc.namespace(c) == Some(NS_IBIS) {
            doc.detach(c);
        }
    }
    if !mode.enabled {
        return;
    }
    let prefix = doc.lookup_prefix(root, NS_IBIS).unwrap_or_else(|| {
        doc.set_attr(root, "xmlns:ibis", NS_IBIS);
        "ibis".to_string()
    });
    let node = doc.append_element(root, &format!("{prefix}:cmyk"));
    doc.set_attr(node, "version", "2");
    if let Some(profile) = &mode.profile {
        doc.set_attr(node, "profile", profile);
    }
    if doc.lookup_prefix(root, MC_NS).is_none() {
        doc.set_attr(root, "xmlns:mc", MC_NS);
    }
    let mc = doc.lookup_prefix(root, MC_NS).unwrap_or_else(|| "mc".into());
    let name = format!("{mc}:Ignorable");
    let current = doc.attr(root, &name).unwrap_or("").to_string();
    if !current.split_whitespace().any(|p| p == prefix) {
        let value = if current.is_empty() { prefix } else { format!("{current} {prefix}") };
        doc.set_attr(root, &name, &value);
    }
}

/// The CMYK mode a workbook's `workbook.xml` says (`root` is `workbook`): the same extension as a presentation's, under
/// `extLst`; off when it has none.
pub(crate) fn read_cmyk_in_workbook(root: roxmltree::Node<'_, '_>) -> CmykMode {
    read_cmyk(root)
}

/// `workbook.xml`'s CMYK extension made to say `mode` (taken away when the mode is off).
pub(crate) fn write_cmyk_in_workbook(doc: &mut XmlDocument, root: NodeId, mode: &CmykMode) -> Result<(), Error> {
    const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    let local = |doc: &XmlDocument, n: NodeId, name: &str| doc.local_name(n) == name && doc.namespace(n) == Some(MAIN_NS);
    let lists: Vec<_> = doc.elements(root).filter(|&n| local(doc, n, "extLst")).collect();
    for &list in &lists {
        let old: Vec<_> = doc.elements(list).filter(|&e| local(doc, e, "ext") && doc.attr(e, "uri") == Some(URI_CMYK)).collect();
        for e in old {
            doc.detach(e);
        }
    }
    if !mode.enabled {
        for list in lists {
            if doc.elements(list).next().is_none() {
                doc.detach(list);
            }
        }
        return Ok(());
    }
    let prefix = |doc: &XmlDocument, n: NodeId| doc.prefix(n).map(|p| format!("{p}:")).unwrap_or_default();
    let list = match lists.first().copied() {
        Some(list) => list,
        None => {
            let p = prefix(doc, root);
            doc.append_element(root, &format!("{p}extLst"))
        }
    };
    let p = prefix(doc, list);
    let ext = doc.append_element(list, &format!("{p}ext"));
    doc.set_attr(ext, "uri", URI_CMYK);
    let node = doc.append_element(ext, "ibis:cmyk");
    doc.set_attr(node, "xmlns:ibis", NS_IBIS);
    doc.set_attr(node, "version", "2");
    if let Some(profile) = &mode.profile {
        doc.set_attr(node, "profile", profile);
    }
    Ok(())
}
