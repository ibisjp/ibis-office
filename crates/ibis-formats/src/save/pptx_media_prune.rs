// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 図の圧縮 and picture replacement leave the old picture's part behind (its
//! relationship still in the slide): when the pictures the slides show
//! changed, the relationships no slide names are taken off, and the media
//! parts nothing relates to.

use std::collections::HashSet;

use ibis_doc_model::{Placeholder, Presentation, ShapeKind};
use ibis_ooxml::Package;

use crate::Error;

const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";

/// The pictures the slides show: (shape, medium).
fn pictures(p: &Presentation) -> Vec<(ibis_doc_model::ShapeId, ibis_doc_model::MediaId)> {
    p.slides
        .iter()
        .flat_map(|s| s.shapes.iter())
        .filter_map(|s| match &s.kind {
            ShapeKind::Picture(pic) => Some((s.id, pic.media)),
            _ => None,
        })
        .collect()
}

/// The SmartArt graphics' frames the slides have.
fn diagrams(p: &Presentation) -> Vec<ibis_doc_model::ShapeId> {
    p.slides.iter().flat_map(|s| s.shapes.iter()).filter(|s| matches!(s.kind, ShapeKind::Placeholder(Placeholder::Diagram))).map(|s| s.id).collect()
}

/// Whether a picture of the baseline shows another medium now, or a SmartArt
/// graphic of it is gone (図形に変換, 削除).
pub(crate) fn changed(baseline: &Presentation, current: &Presentation) -> bool {
    let now = pictures(current);
    let diagrams_now = diagrams(current);
    pictures(baseline).iter().any(|(shape, media)| now.iter().any(|(s, m)| s == shape && m != media)) || diagrams(baseline).iter().any(|d| !diagrams_now.contains(d))
}

/// A relationship to a part of a SmartArt graphic (`dgm:relIds` and the
/// drawing PowerPoint keeps of it name them).
fn is_diagram_rel(kind: &str) -> bool {
    ["/diagramData", "/diagramLayout", "/diagramQuickStyle", "/diagramColors", "/diagramDrawing"].iter().any(|k| kind.ends_with(k))
}

/// The image relationships of `parts` that nothing in the part names, and
/// the media parts that no relationship points at, taken off.
pub(crate) fn prune(package: &mut Package, parts: &[String]) -> Result<(), Error> {
    for part in parts {
        if !package.contains(part) {
            continue;
        }
        let named: HashSet<String> = {
            let doc = package.xml(part)?;
            let root = doc.root_element();
            root.map(|r| doc.descendants(r)).unwrap_or_default().into_iter().filter(|&n| doc.is_element(n)).flat_map(|n| doc.attributes(n).iter().map(|a| a.value.clone()).collect::<Vec<_>>()).collect()
        };
        for rel in package.relationships(part)? {
            if (rel.kind == REL_IMAGE || is_diagram_rel(&rel.kind) && !rel.kind.ends_with("/diagramDrawing")) && !rel.external && !named.contains(&rel.id) {
                package.remove_relationship(part, &rel.id)?;
            }
        }
        // The drawing PowerPoint keeps of a graphic is named by its data part (`dsp:dataModelExt/@relId`).
        let mut drawings: HashSet<String> = HashSet::new();
        for rel in package.relationships(part)? {
            if rel.kind.ends_with("/diagramData") && !rel.external {
                if let Some(data) = rel.target_part(part).filter(|d| package.contains(d)) {
                    let doc = package.xml(&data)?;
                    let root = doc.root_element();
                    for n in root.map(|r| doc.descendants(r)).unwrap_or_default().into_iter().filter(|&n| doc.is_element(n) && doc.local_name(n) == "dataModelExt") {
                        if let Some(id) = doc.attr(n, "relId") {
                            drawings.insert(id.to_string());
                        }
                    }
                }
            }
        }
        for rel in package.relationships(part)? {
            if rel.kind.ends_with("/diagramDrawing") && !rel.external && !drawings.contains(&rel.id) {
                package.remove_relationship(part, &rel.id)?;
            }
        }
    }
    let names: Vec<String> = package.names().map(str::to_string).collect();
    let mut referenced: HashSet<String> = HashSet::new();
    for name in names.iter().map(String::as_str).chain(std::iter::once("")) {
        if name.contains("_rels/") && name.ends_with(".rels") {
            continue;
        }
        for rel in package.relationships(name)? {
            if let Some(target) = rel.target_part(name) {
                referenced.insert(target);
            }
        }
    }
    for name in names.iter().filter(|n| n.starts_with("ppt/media/") || n.starts_with("ppt/diagrams/") && !n.contains("_rels/")) {
        if !referenced.contains(name) {
            package.remove(name);
            package.remove_override(name)?;
            // The part's own relationships go with it.
            if let Some((dir, file)) = name.rsplit_once('/') {
                package.remove(&format!("{dir}/_rels/{file}.rels"));
            }
        }
    }
    Ok(())
}
