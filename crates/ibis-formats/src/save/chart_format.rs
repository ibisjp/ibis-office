// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Explicit chart-area, plot-area and text formatting, preserving the rest of each OOXML element.
use crate::Error;
use ibis_doc_model::{ChartText, Fill, Stroke};
use ibis_ooxml::{NodeId, XmlDocument};
fn ensure(doc: &mut XmlDocument, parent: NodeId, name: &str, before: &[&str]) -> NodeId {
    let local = name.rsplit(':').next().unwrap();
    if let Some(node) = doc.child(parent, local) {
        return node;
    }
    let node = doc.append_element(parent, name);
    doc.detach(node);
    let target = doc
        .elements(parent)
        .find(|&n| before.contains(&doc.local_name(n)));
    if let Some(target) = target {
        doc.insert_before(target, node);
    } else {
        let at = doc.children(parent).len();
        doc.insert(parent, at, node);
    }
    node
}
pub(crate) fn area(
    doc: &mut XmlDocument,
    parent: NodeId,
    fill: Option<&Fill>,
    stroke: Option<Option<&Stroke>>,
    c: &str,
    a: &str,
    before: &[&str],
) -> Result<(), Error> {
    ensure(doc, parent, &format!("{c}spPr"), before);
    if let Some(fill) = fill {
        super::pptx_chart::write_series_fill(doc, parent, fill, c, a)?;
    }
    if let Some(stroke) = stroke {
        super::pptx_chart::write_series_stroke(doc, parent, stroke, c, a)?;
    }
    Ok(())
}
pub(crate) fn text(doc: &mut XmlDocument, parent: NodeId, style: &ChartText, c: &str, a: &str) {
    let body = if let Some(rich) = doc.path(parent, &["tx", "rich"]) {
        rich
    } else {
        ensure(
            doc,
            parent,
            &format!("{c}txPr"),
            &[
                "crossAx",
                "crosses",
                "crossesAt",
                "crossBetween",
                "dLblPos",
                "showLegendKey",
                "showVal",
                "showCatName",
                "showSerName",
                "showPercent",
                "showBubbleSize",
                "extLst",
            ],
        )
    };
    ensure(doc, body, &format!("{a}:bodyPr"), &["lstStyle", "p"]);
    ensure(doc, body, &format!("{a}:lstStyle"), &["p"]);
    if doc.child(body, "p").is_none() {
        ensure(doc, body, &format!("{a}:p"), &[]);
    }
    let paragraphs: Vec<_> = doc.children_named(body, "p").collect();
    for p in paragraphs {
        let ppr = ensure(
            doc,
            p,
            &format!("{a}:pPr"),
            &["r", "br", "fld", "endParaRPr"],
        );
        ensure(doc, ppr, &format!("{a}:defRPr"), &["extLst"]);
    }
    let properties: Vec<_> = doc
        .descendants(body)
        .into_iter()
        .filter(|&n| matches!(doc.local_name(n), "rPr" | "defRPr" | "endParaRPr"))
        .collect();
    for props in properties {
        doc.set_attr(props, "sz", &style.size.to_string());
        doc.set_attr(props, "b", if style.bold { "1" } else { "0" });
        if let Some(color) = style.color {
            super::pptx_chart::write_solid_fill(doc, props, color, a);
        } else {
            for n in doc
                .elements(props)
                .filter(|&n| matches!(doc.local_name(n), "solidFill" | "noFill" | "gradFill"))
                .collect::<Vec<_>>()
            {
                doc.detach(n);
            }
            ensure(
                doc,
                props,
                &format!("{a}:noFill"),
                &["latin", "ea", "cs", "extLst"],
            );
        }
        for (kind, font) in [("latin", &style.font), ("ea", &style.east_asian_font)] {
            if let Some(font) = font {
                let node = ensure(doc, props, &format!("{a}:{kind}"), &["extLst"]);
                doc.set_attr(node, "typeface", font);
            } else if let Some(node) = doc.child(props, kind) {
                doc.detach(node);
            }
        }
    }
}
