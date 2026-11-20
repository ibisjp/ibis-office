// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Saving presentation show properties (`p:showPr`).
//!
//! Stored in `ppt/presProps.xml` as `<p:showPr>` inside `<p:presentationPr>`.

use ibis_doc_model::{ShowProperties, ShowSlideRange, ShowType};
use ibis_ooxml::Package;

use super::pptx::relative;
use super::pptx_new::Prefixes;
use crate::Error;

const REL_PRES_PROPS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/presProps";
const CT_PRES_PROPS: &str = "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml";

pub(crate) fn write(package: &mut Package, presentation: &str, show: &ShowProperties, custom_shows: &[ibis_doc_model::CustomShow]) -> Result<(), Error> {
    let pres_props_part = package.relationships(presentation)?
        .into_iter()
        .find(|r| r.kind == REL_PRES_PROPS && !r.external)
        .and_then(|r| r.target_part(presentation))
        .unwrap_or_else(|| "ppt/presProps.xml".to_string());

    if !package.contains(&pres_props_part) {
        let xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:presentationPr xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"/>"#;
        package.set_xml(&pres_props_part, ibis_ooxml::XmlDocument::parse(xml.as_bytes()).map_err(|e| Error::new(format!("presProps.xml を作れません: {e}")))?);
        package.set_override(&pres_props_part, CT_PRES_PROPS)?;
        package.add_relationship(presentation, REL_PRES_PROPS, &relative(presentation, &pres_props_part), false)?;
    }

    let doc = package.xml_mut(&pres_props_part)?;
    let root = doc.root_element().ok_or_else(|| Error::new("presProps.xml が空です"))?;
    let x = Prefixes::at(doc, root);

    if let Some(existing) = doc.child(root, "showPr") {
        doc.detach(existing);
    }

    let show_xml = show_pr_xml(&x, show, custom_shows);
    let show_node = x.import(doc, &show_xml)?;

    let next = doc.elements(root).find(|&n| matches!(doc.local_name(n), "clrMru" | "extLst"));
    if let Some(next) = next {
        doc.insert_before(next, show_node);
    } else {
        let end = doc.children(root).len();
        doc.insert(root, end, show_node);
    }

    Ok(())
}

fn show_pr_xml(x: &Prefixes, show: &ShowProperties, custom_shows: &[ibis_doc_model::CustomShow]) -> String {
    let mut attrs = Vec::new();
    if show.loop_continuously {
        attrs.push("loop=\"1\"".to_string());
    }
    if !show.show_narration {
        attrs.push("showNarration=\"0\"".to_string());
    }
    if !show.show_animation {
        attrs.push("showAnimation=\"0\"".to_string());
    }
    if !show.use_timings {
        attrs.push("useTimings=\"0\"".to_string());
    }
    let attr_str = if attrs.is_empty() { String::new() } else { format!(" {}", attrs.join(" ")) };

    let mut inner = String::new();
    match show.show_type {
        ShowType::Presenter => {
            inner.push_str(&format!("<{}present/>", x.p));
        }
        ShowType::Browse => {
            inner.push_str(&format!("<{}browse/>", x.p));
        }
        ShowType::Kiosk { restart } => {
            if let Some(r) = restart {
                inner.push_str(&format!("<{}kiosk restart=\"{}\"/>", x.p, r));
            } else {
                inner.push_str(&format!("<{}kiosk/>", x.p));
            }
        }
    }

    match show.slide_range {
        ShowSlideRange::All => {
            inner.push_str(&format!("<{}sldAll/>", x.p));
        }
        ShowSlideRange::Range { start, end } => {
            inner.push_str(&format!("<{}sldRg st=\"{}\" end=\"{}\"/>", x.p, start, end));
        }
        ShowSlideRange::Custom(id) if custom_shows.iter().any(|c| c.id == id) => {
            inner.push_str(&format!("<{}custShow id=\"{}\"/>", x.p, id));
        }
        ShowSlideRange::Custom(_) => {
            // Dangling custom show reference: fall back to all slides so PowerPoint doesn't hang.
            inner.push_str(&format!("<{}sldAll/>", x.p));
        }
    }

    if let Some(rgb) = show.pen_color {
        inner.push_str(&format!("<{}penClr><{}srgbClr val=\"{:06X}\"/></{}penClr>", x.p, x.a, rgb.0, x.p));
    }

    format!("<{}showPr{}>{}</{}showPr>", x.p, attr_str, inner, x.p)
}
