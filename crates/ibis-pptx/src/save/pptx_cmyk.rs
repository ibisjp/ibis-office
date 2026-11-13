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
