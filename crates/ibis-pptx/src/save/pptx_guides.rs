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
        // PowerPoint cannot open a file whose p15:guide has no p15:clr ("corrupted and unreadable", PowerPoint 365, #M132,
        // 2026-11-12): every guide is written with its colour; one without a colour of its own takes PowerPoint's for the
        // guides a user draws (A4A3A4, gray), as PowerPoint writes it.
        let c = g.color.unwrap_or(0xA4A3A4);
        s.push_str(&format!("><p15:clr><{a}srgbClr val=\"{c:06X}\"/></p15:clr></p15:guide>", a = x.a));
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
    // Every extension of guides (a file may have an empty one beside another).
    while let Some((list, ext)) = guide_ext(doc, root) {
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

const REL_VIEW_PROPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/viewProps";
const CT_VIEW_PROPS: &str = "application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml";

/// The part of the view properties (`ppt/viewProps.xml`), if the presentation names one.
pub(crate) fn view_props_part(package: &ibis_ooxml::Package, presentation: &str) -> Option<String> {
    package.relationships(presentation).ok()?.into_iter().find(|r| r.kind == REL_VIEW_PROPS && !r.external).and_then(|r| r.target_part(presentation))
}

/// The guides of the normal view as PowerPoint keeps them: `p:viewPr/p:slideViewPr/p:cSldViewPr/p:guideLst` of
/// `viewProps.xml` (`p:guide` with `orient` and `pos` in 1/8 pt, no id and no colour); `text` is that part.
pub(crate) fn read_view(text: &str) -> Vec<SlideGuide> {
    let Ok(doc) = roxmltree::Document::parse(text) else { return Vec::new() };
    fn path<'a, 'i>(from: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
        from.children().find(|n| n.is_element() && n.tag_name().name() == name)
    }
    let Some(list) = path(doc.root_element(), "slideViewPr").and_then(|n| path(n, "cSldViewPr")).and_then(|n| path(n, "guideLst")) else { return Vec::new() };
    list.children()
        .filter(|g| g.is_element() && g.tag_name().name() == "guide")
        .enumerate()
        .map(|(i, g)| SlideGuide {
            id: i as u32 + 1,
            horizontal: g.attribute("orient") == Some("horz"),
            pos: g.attribute("pos").and_then(|v| v.parse().ok()).unwrap_or(0),
            color: None,
            user_drawn: true,
        })
        .collect()
}

/// How the notes pane stands when the file is opened: `p:normalViewPr`'s `restoredTop@sz` and `horzBarState` of `viewProps.xml`
/// (`text` is that part; a file with no such part is read as `None` by the caller, which is the folded default).
pub(crate) fn read_notes_pane(text: &str) -> ibis_doc_model::NotesPaneOpening {
    let Ok(doc) = roxmltree::Document::parse(text) else { return ibis_doc_model::NotesPaneOpening::default() };
    let Some(normal) = doc.root_element().children().find(|n| n.is_element() && n.tag_name().name() == "normalViewPr") else { return ibis_doc_model::NotesPaneOpening::default() };
    let top = normal.children().find(|n| n.is_element() && n.tag_name().name() == "restoredTop").and_then(|n| n.attribute("sz")).and_then(|v| v.parse().ok());
    ibis_doc_model::NotesPaneOpening::from_view(top, normal.attribute("horzBarState"))
}

/// The view properties as PowerPoint 365 writes them for a file that has none (tasks/doc/m114-pptx-notes-pane-save.md, (c)): the normal view at
/// its defaults (`restoredLeft` 15620, `restoredTop` 94660), the slide and notes views and the grid.
const NEW_VIEW_PROPS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:viewPr xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:normalViewPr><p:restoredLeft sz="15620"/><p:restoredTop sz="94660"/></p:normalViewPr><p:slideViewPr><p:cSldViewPr><p:cViewPr varScale="1"><p:scale><a:sx n="100" d="100"/><a:sy n="100" d="100"/></p:scale><p:origin x="0" y="0"/></p:cViewPr><p:guideLst/></p:cSldViewPr></p:slideViewPr><p:notesTextViewPr><p:cViewPr><p:scale><a:sx n="1" d="1"/><a:sy n="1" d="1"/></p:scale><p:origin x="0" y="0"/></p:cViewPr></p:notesTextViewPr><p:gridSpacing cx="76200" cy="76200"/></p:viewPr>"#;

/// What the user did to the notes pane, written to `p:normalViewPr` of `viewProps.xml` as PowerPoint 365 writes it (tasks/doc/m101-softedge-1b-notes-pane.md,
/// (3); tasks/doc/m114-pptx-notes-pane-save.md): a moved border is `restoredTop@sz` and `autoAdjust="0"` with no `horzBarState`; `minimized` and
/// `maximized` write `restoredTop` as the border stood before (moved: that value and `autoAdjust="0"`; not moved: the file's own); shown again has
/// no `horzBarState`. A file with no `viewProps.xml` or no `normalViewPr` gets them (PowerPoint makes both at every save), also with `save` None.
pub(crate) fn write_notes_pane(package: &mut ibis_ooxml::Package, presentation: &str, save: Option<ibis_doc_model::NotesPaneSave>) -> Result<(), Error> {
    use ibis_doc_model::NotesPaneSave;
    let part = match view_props_part(package, presentation) {
        Some(p) => p,
        None => {
            let part = "ppt/viewProps.xml".to_string();
            package.set_xml(&part, XmlDocument::parse(NEW_VIEW_PROPS.as_bytes()).map_err(|e| Error::new(format!("viewProps.xml を作れません: {e}")))?);
            package.set_override(&part, CT_VIEW_PROPS)?;
            package.add_relationship(presentation, REL_VIEW_PROPS, &super::pptx::relative(presentation, &part), false)?;
            part
        }
    };
    let doc = package.xml_mut(&part)?;
    let Some(root) = doc.root_element() else { return Ok(()) };
    let found = doc.elements(root).find(|&n| doc.local_name(n) == "normalViewPr");
    let normal = match found {
        Some(n) => n,
        None => {
            let x = Prefixes::at(doc, root);
            let node = x.import(doc, &format!("<{p}normalViewPr><{p}restoredLeft sz=\"15620\"/><{p}restoredTop sz=\"94660\"/></{p}normalViewPr>", p = x.p))?;
            // The first of the views.
            doc.insert(root, 0, node);
            node
        }
    };
    let Some(save) = save else { return Ok(()) };
    let set_top = |doc: &mut XmlDocument, sz: u32| {
        let existing = doc.elements(normal).find(|&n| doc.local_name(n) == "restoredTop");
        let top = match existing {
            Some(top) => top,
            None => {
                let name = doc.name(normal).replace("normalViewPr", "restoredTop");
                let at = doc.children(normal).len();
                doc.insert_element(normal, at, &name)
            }
        };
        doc.set_attr(top, "sz", &sz.to_string());
        doc.set_attr(top, "autoAdjust", "0");
    };
    match save {
        NotesPaneSave::Moved(sz) => {
            set_top(doc, sz);
            doc.remove_attr(normal, "horzBarState");
        }
        NotesPaneSave::Minimized(restored) | NotesPaneSave::Maximized(restored) => {
            if let Some(sz) = restored {
                set_top(doc, sz);
            }
            doc.set_attr(normal, "horzBarState", if matches!(save, NotesPaneSave::Minimized(_)) { "minimized" } else { "maximized" });
        }
        NotesPaneSave::Shown => doc.remove_attr(normal, "horzBarState"),
    }
    Ok(())
}

/// The guides of the normal view made `guides` in `viewProps.xml` (the part made when there is none).
pub(crate) fn write_view(package: &mut ibis_ooxml::Package, presentation: &str, guides: &[SlideGuide]) -> Result<(), Error> {
    let part = match view_props_part(package, presentation) {
        Some(p) => p,
        None => {
            if guides.is_empty() {
                return Ok(());
            }
            let part = "ppt/viewProps.xml".to_string();
            let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:viewPr xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"/>"#;
            package.set_xml(&part, XmlDocument::parse(xml.as_bytes()).map_err(|e| Error::new(format!("viewProps.xml を作れません: {e}")))?);
            package.set_override(&part, CT_VIEW_PROPS)?;
            package.add_relationship(presentation, REL_VIEW_PROPS, &super::pptx::relative(presentation, &part), false)?;
            part
        }
    };
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| Error::new("viewProps.xml が空です"))?;
    let x = Prefixes::at(doc, root);
    let find = |doc: &XmlDocument, from: NodeId, name: &str| doc.elements(from).find(|&n| doc.local_name(n) == name);
    let slide_view = match find(doc, root, "slideViewPr") {
        Some(n) => n,
        None => {
            if guides.is_empty() {
                return Ok(());
            }
            let node = x.import(doc, &format!("<{p}slideViewPr><{p}cSldViewPr><{p}cViewPr><{p}scale><{a}sx n=\"100\" d=\"100\"/><{a}sy n=\"100\" d=\"100\"/></{p}scale><{p}origin x=\"0\" y=\"0\"/></{p}cViewPr></{p}cSldViewPr></{p}slideViewPr>", p = x.p, a = x.a))?;
            // After normalViewPr, before the other views, the grid spacing and the extensions.
            let next = doc.elements(root).find(|&n| matches!(doc.local_name(n), "outlineViewPr" | "notesTextViewPr" | "sorterViewPr" | "notesViewPr" | "gridSpacing" | "extLst"));
            match next {
                Some(next) => doc.insert_before(next, node),
                None => {
                    let end = doc.children(root).len();
                    doc.insert(root, end, node);
                }
            }
            node
        }
    };
    let Some(common) = find(doc, slide_view, "cSldViewPr") else { return Ok(()) };
    if let Some(old) = find(doc, common, "guideLst") {
        doc.detach(old);
    }
    if guides.is_empty() {
        return Ok(());
    }
    let mut xml = format!("<{p}guideLst>", p = x.p);
    for g in guides {
        xml.push_str(&format!("<{p}guide", p = x.p));
        if g.horizontal {
            xml.push_str(" orient=\"horz\"");
        }
        if g.pos != 0 {
            xml.push_str(&format!(" pos=\"{}\"", g.pos));
        }
        xml.push_str("/>");
    }
    xml.push_str(&format!("</{p}guideLst>", p = x.p));
    let node = x.import(doc, &xml)?;
    // After the view's own properties (`cViewPr`), at the end of the element.
    let end = doc.children(common).len();
    doc.insert(common, end, node);
    Ok(())
}
