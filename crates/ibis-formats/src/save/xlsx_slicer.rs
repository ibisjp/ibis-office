// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Saving slicers (スライサー): a slicer's parts are kept as they were; when
//! the selection of a pivot table's slicer changed in the app, the items
//! its cache (`xl/slicerCaches/slicerCache*.xml`) lists as selected (`s`)
//! follow it. A table's slicer keeps its selection in the table's filter
//! (saved with the table), so its cache has nothing to change.

use std::collections::HashMap;

use ibis_doc_model::slicer::{Slicer, SlicerSource};
use ibis_doc_model::{Presentation, ShapeKind};
use ibis_ooxml::Package;

use crate::Error;

fn slicers_of(p: &Presentation) -> HashMap<(ibis_doc_model::SlideId, ibis_doc_model::ShapeId), Slicer> {
    p.slides.iter().flat_map(|sl| sl.shapes.iter().filter_map(move |sh| match &sh.kind {
        ShapeKind::Slicer(s) => Some(((sl.id, sh.id), (**s).clone())),
        _ => None,
    })).collect()
}

/// The shared-item index (`x`) of each of the slicer's items, in order.
fn shared_indexes(current: &Presentation, slicer: &Slicer) -> Option<Vec<u32>> {
    let SlicerSource::Pivot { field, pivots } = &slicer.source else { return None };
    let (slide, index) = pivots.iter().find_map(|n| ibis_doc_model::slicer::locate_pivot(current, n))?;
    let sheet = current.slide(slide).and_then(ibis_doc_model::sheet_calc::sheet_of_slide)?;
    let f = sheet.pivots.get(index)?.fields.iter().find(|f| &f.name == field)?;
    let mut indexes: Vec<u32> = f.items.iter().filter(|it| it.kind.is_none()).filter_map(|it| it.x).collect();
    if slicer.descending {
        indexes.reverse();
    }
    Some(indexes)
}

/// Make the slicer caches of the pivot slicers whose selection changed say so.
pub(super) fn write_slicer_selection(package: &mut Package, workbook_part: &str, baseline: &Presentation, current: &Presentation) -> Result<(), Error> {
    let (old, new) = (slicers_of(baseline), slicers_of(current));
    let changed: Vec<&Slicer> = new
        .iter()
        .filter(|(k, s)| {
            matches!(s.source, SlicerSource::Pivot { .. })
                && old.get(k).or_else(|| old.values().find(|o| o.name == s.name)).is_some_and(|o| o.items != s.items)
        })
        .map(|(_, s)| s)
        .collect();
    if changed.is_empty() {
        return Ok(());
    }
    let caches: Vec<String> = package.relationships(workbook_part)?.iter().filter(|r| r.kind.ends_with("/slicerCache")).filter_map(|r| r.target_part(workbook_part)).collect();
    for slicer in changed {
        let Some(indexes) = shared_indexes(current, slicer) else { continue };
        for part in &caches {
            let doc = package.xml_mut(part)?;
            let Some(root) = doc.root_element() else { continue };
            if doc.attr(root, "name") != Some(slicer.cache.as_str()) {
                continue;
            }
            for i in doc.descendants(root) {
                if doc.local_name(i) != "i" {
                    continue;
                }
                let Some(x) = doc.attr(i, "x").and_then(|x| x.parse::<u32>().ok()) else { continue };
                let Some(item) = indexes.iter().position(|&v| v == x).and_then(|k| slicer.items.get(k)) else { continue };
                if item.selected {
                    doc.set_attr(i, "s", "1");
                } else {
                    doc.remove_attr(i, "s");
                }
            }
        }
    }
    Ok(())
}

/// A moved or resized slicer: its anchor in the sheet's drawing set anew.
pub(super) fn write_slicer_frames(package: &mut Package, sheet_part: &str, sheet: &ibis_doc_model::Sheet, old: &[ibis_doc_model::Shape], new: &[ibis_doc_model::Shape]) -> Result<(), Error> {
    let moved: Vec<(&str, ibis_doc_model::EmuRect)> = new
        .iter()
        .filter_map(|n| match &n.kind {
            ShapeKind::Slicer(s) => old.iter().find(|o| o.id == n.id).filter(|o| o.frame != n.frame).map(|o| {
                let name = match &o.kind {
                    ShapeKind::Slicer(os) => os.name.as_str(),
                    _ => s.name.as_str(),
                };
                (name, n.frame)
            }),
            _ => None,
        })
        .collect();
    if moved.is_empty() {
        return Ok(());
    }
    let drawing = package
        .relationships(sheet_part)?
        .iter()
        .find(|r| r.kind.ends_with("/drawing"))
        .and_then(|r| r.target_part(sheet_part))
        .ok_or_else(|| Error::new("スライサーの描画が見つかりません"))?;
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let cell = |offsets: &[i64], v: i64| -> (usize, i64) {
        let i = offsets.partition_point(|&o| o <= v).saturating_sub(1).min(offsets.len().saturating_sub(2));
        (i, v - offsets.get(i).copied().unwrap_or(0))
    };
    let doc = package.xml_mut(&drawing)?;
    let root = doc.root_element().ok_or_else(|| Error::new("描画の XML が空です"))?;
    for (name, frame) in moved {
        let node = doc.descendants(root).into_iter().find(|&n| doc.local_name(n) == "slicer" && doc.attr(n, "name") == Some(name));
        let mut anchor = node;
        while let Some(a) = anchor {
            if matches!(doc.local_name(a), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor") {
                break;
            }
            anchor = doc.parent(a);
        }
        let Some(anchor) = anchor else { continue };
        let kind = doc.local_name(anchor).to_string();
        if kind == "absoluteAnchor" {
            if let Some(p) = doc.child(anchor, "pos") {
                doc.set_attr(p, "x", &frame.x.to_string());
                doc.set_attr(p, "y", &frame.y.to_string());
            }
            if let Some(e) = doc.child(anchor, "ext") {
                doc.set_attr(e, "cx", &frame.width.to_string());
                doc.set_attr(e, "cy", &frame.height.to_string());
            }
            continue;
        }
        let corners: Vec<(&str, i64, i64)> = if kind == "twoCellAnchor" { vec![("from", frame.x, frame.y), ("to", frame.x + frame.width, frame.y + frame.height)] } else { vec![("from", frame.x, frame.y)] };
        for (corner_name, x, y) in corners {
            let Some(corner) = doc.child(anchor, corner_name) else { continue };
            let ((c, co), (r, ro)) = (cell(&xs, x), cell(&ys, y));
            for (n, v) in [("col", c as i64), ("colOff", co), ("row", r as i64), ("rowOff", ro)] {
                if let Some(e) = doc.child(corner, n) {
                    doc.set_text(e, &v.to_string());
                }
            }
        }
        if kind == "oneCellAnchor"
            && let Some(e) = doc.child(anchor, "ext")
        {
            doc.set_attr(e, "cx", &frame.width.to_string());
            doc.set_attr(e, "cy", &frame.height.to_string());
        }
    }
    Ok(())
}

/// Slicers deleted in the app: each one's anchor leaves the drawing and its
/// `x14:slicer` the slicers part (the part goes with its last slicer, with
/// its relationship and the sheet's `slicerList`); a cache no slicer uses
/// any more goes the same way from the workbook.
pub(super) fn write_slicer_removals(package: &mut Package, sheet_part: &str, workbook_part: &str, removed: &[Slicer], remaining: &[Slicer]) -> Result<(), Error> {
    if removed.is_empty() {
        return Ok(());
    }
    // Their anchors in the sheet's drawing.
    if let Some(drawing) = package.relationships(sheet_part)?.iter().find(|r| r.kind.ends_with("/drawing")).and_then(|r| r.target_part(sheet_part)) {
        let doc = package.xml_mut(&drawing)?;
        if let Some(root) = doc.root_element() {
            for s in removed {
                let node = doc.descendants(root).into_iter().find(|&n| doc.local_name(n) == "slicer" && doc.attr(n, "name") == Some(s.name.as_str()));
                let mut anchor = node;
                while let Some(a) = anchor {
                    if matches!(doc.local_name(a), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor") {
                        break;
                    }
                    anchor = doc.parent(a);
                }
                if let Some(a) = anchor {
                    // The `mc:AlternateContent` Excel wraps it in goes too.
                    let target = doc.parent(a).filter(|&p| doc.local_name(p) == "Choice").and_then(|c| doc.parent(c)).filter(|&ac| doc.local_name(ac) == "AlternateContent").unwrap_or(a);
                    doc.detach(target);
                }
            }
        }
    }
    // Their entries in the slicers parts.
    for rel in package.relationships(sheet_part)?.into_iter().filter(|r| r.kind.ends_with("/slicer")) {
        let Some(part) = rel.target_part(sheet_part) else { continue };
        let doc = package.xml_mut(&part)?;
        let Some(root) = doc.root_element() else { continue };
        for n in doc.children_named(root, "slicer").collect::<Vec<_>>() {
            if doc.attr(n, "name").is_some_and(|name| removed.iter().any(|s| s.name == name)) {
                doc.detach(n);
            }
        }
        if doc.children_named(root, "slicer").next().is_some() {
            continue;
        }
        package.remove_relationship(sheet_part, &rel.id)?;
        package.remove_override(&part)?;
        package.remove(&part);
        let sheet = package.xml_mut(sheet_part)?;
        let Some(sheet_root) = sheet.root_element() else { continue };
        for n in sheet.descendants(sheet_root) {
            if sheet.local_name(n) == "slicer" && sheet.attr_local(n, "id") == Some(rel.id.as_str()) {
                sheet.detach(n);
            }
        }
        for list in sheet.descendants(sheet_root).into_iter().filter(|&n| sheet.local_name(n) == "slicerList").collect::<Vec<_>>() {
            if sheet.children(list).iter().all(|&c| !sheet.is_element(c)) {
                // The `ext`, and the `extLst` when it was the only one.
                let ext = sheet.parent(list).filter(|&e| sheet.local_name(e) == "ext");
                let ext_lst = ext.and_then(|e| sheet.parent(e));
                if let Some(e) = ext {
                    sheet.detach(e);
                }
                if let Some(l) = ext_lst.filter(|&l| sheet.local_name(l) == "extLst" && sheet.elements(l).next().is_none()) {
                    sheet.detach(l);
                }
            }
        }
    }
    // The caches nothing uses now.
    let unused: Vec<&str> = removed.iter().map(|s| s.cache.as_str()).filter(|c| !remaining.iter().any(|s| s.cache == *c)).collect();
    if unused.is_empty() {
        return Ok(());
    }
    for rel in package.relationships(workbook_part)?.into_iter().filter(|r| r.kind.ends_with("/slicerCache")) {
        let Some(part) = rel.target_part(workbook_part) else { continue };
        let name = package.xml(&part).ok().and_then(|d| d.root_element().and_then(|r| d.attr(r, "name").map(str::to_string)));
        if !name.as_deref().is_some_and(|n| unused.contains(&n)) {
            continue;
        }
        package.remove_relationship(workbook_part, &rel.id)?;
        package.remove_override(&part)?;
        package.remove(&part);
        let wb = package.xml_mut(workbook_part)?;
        let Some(wb_root) = wb.root_element() else { continue };
        // The name Excel makes for the slicer (`スライサー_商品`, `Slicer_Product`).
        if let Some(name) = name.as_deref() {
            for list in wb.child(wb_root, "definedNames").into_iter().collect::<Vec<_>>() {
                for n in wb.children_named(list, "definedName").collect::<Vec<_>>() {
                    if wb.attr(n, "name") == Some(name) {
                        wb.detach(n);
                    }
                }
                if wb.children_named(list, "definedName").next().is_none() {
                    wb.detach(list);
                }
            }
        }
        for n in wb.descendants(wb_root) {
            if wb.local_name(n) == "slicerCache" && wb.attr_local(n, "id") == Some(rel.id.as_str()) {
                wb.detach(n);
            }
        }
        for list in wb.descendants(wb_root).into_iter().filter(|&n| wb.local_name(n) == "slicerCaches").collect::<Vec<_>>() {
            if wb.elements(list).next().is_none() {
                let ext = wb.parent(list).filter(|&e| wb.local_name(e) == "ext");
                let ext_lst = ext.and_then(|e| wb.parent(e));
                if let Some(e) = ext {
                    wb.detach(e);
                }
                if let Some(l) = ext_lst.filter(|&l| wb.local_name(l) == "extLst" && wb.elements(l).next().is_none()) {
                    wb.detach(l);
                }
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------ slicers added

const REL_SLICER: &str = "http://schemas.microsoft.com/office/2007/relationships/slicer";
const REL_SLICER_CACHE: &str = "http://schemas.microsoft.com/office/2007/relationships/slicerCache";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const X15: &str = "http://schemas.microsoft.com/office/spreadsheetml/2010/11/main";
/// The `ext` of a sheet's `extLst` that lists its slicers: pivot tables' and tables'.
const SHEET_EXT_PIVOT: &str = "{A8765BA9-456A-4dab-B4F3-ACF838C121DE}";
const SHEET_EXT_TABLE: &str = "{3A4CF648-6AED-40f4-86FF-DC5316D8AED3}";
/// The `ext` of the workbook's `extLst` that lists the slicer caches.
const BOOK_EXT_PIVOT: &str = "{BBE1A952-AA13-448e-AADC-164F8A28A991}";
const BOOK_EXT_TABLE: &str = "{46BE6895-7355-4a93-B00E-2C351335B9C9}";

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A fragment of XML read as a document, to be imported into another.
fn fragment(xml: &str) -> Result<ibis_ooxml::XmlDocument, Error> {
    ibis_ooxml::XmlDocument::parse(xml.as_bytes()).map_err(|e| Error::new(format!("XML の断片: {e:?}")))
}

fn next_part(package: &Package, directory: &str, stem: &str) -> String {
    (1..).map(|n| format!("{directory}/{stem}{n}.xml")).find(|p| !package.contains(p)).expect("a free part name")
}

fn relative_to(from: &str, to: &str) -> String {
    super::super::pptx::relative(from, to)
}

/// The `ext` of `list` (an `extLst`) with `uri`, made if it is not there.
fn ext_with_uri(doc: &mut ibis_ooxml::XmlDocument, list: ibis_ooxml::NodeId, uri: &str, namespace_prefix: &str, namespace: &str) -> ibis_ooxml::NodeId {
    if let Some(e) = doc.children_named(list, "ext").find(|&e| doc.attr(e, "uri") == Some(uri)) {
        return e;
    }
    let e = doc.append_element(list, "ext");
    doc.set_attr(e, "uri", uri);
    doc.set_attr(e, &format!("xmlns:{namespace_prefix}"), namespace);
    e
}

/// The root's `extLst`, made (last) if it is not there.
fn ext_list(doc: &mut ibis_ooxml::XmlDocument, root: ibis_ooxml::NodeId) -> ibis_ooxml::NodeId {
    match doc.child(root, "extLst") {
        Some(l) => l,
        None => doc.append_element(root, "extLst"),
    }
}

fn r_prefix(doc: &mut ibis_ooxml::XmlDocument, root: ibis_ooxml::NodeId) -> String {
    if doc.lookup_prefix(root, REL_NS).is_none() {
        doc.set_attr(root, "xmlns:r", REL_NS);
    }
    doc.lookup_prefix(root, REL_NS).unwrap_or_else(|| "r".into())
}

/// Add a slicer shape the app made: its anchor in the sheet's drawing, its
/// entry in the sheet's slicers part, and its cache (with the workbook's
/// reference to it).
pub(super) fn add_slicer(package: &mut Package, map: &super::SourceMap, sheet_part: &str, sheet: &ibis_doc_model::Sheet, shape: &ibis_doc_model::Shape, slicer: &Slicer, current: &Presentation) -> Result<(), Error> {
    let table = matches!(slicer.source, SlicerSource::Table { .. });
    let drawing = match package.relationships(sheet_part)?.into_iter().find(|r| r.kind.ends_with("/drawing") && !r.external) {
        Some(rel) => rel.target_part(sheet_part).ok_or_else(|| Error::new("シートの描画パートが見つかりません"))?,
        None => super::create_drawing(package, sheet_part)?,
    };
    add_slicer_anchor(package, &drawing, sheet, shape, slicer, table)?;
    add_slicer_entry(package, sheet_part, slicer, table)?;
    add_slicer_cache(package, map, current, slicer)?;
    Ok(())
}

fn add_slicer_anchor(package: &mut Package, drawing: &str, sheet: &ibis_doc_model::Sheet, shape: &ibis_doc_model::Shape, slicer: &Slicer, table: bool) -> Result<(), Error> {
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let cell = |offsets: &[i64], v: i64| -> (usize, i64) {
        let i = offsets.partition_point(|&o| o <= v).saturating_sub(1).min(offsets.len().saturating_sub(2));
        (i, v - offsets.get(i).copied().unwrap_or(0))
    };
    let f = shape.frame;
    let ((c0, co0), (r0, ro0)) = (cell(&xs, f.x), cell(&ys, f.y));
    let ((c1, co1), (r1, ro1)) = (cell(&xs, f.x + f.width), cell(&ys, f.y + f.height));
    let doc = package.xml_mut(drawing)?;
    let root = doc.root_element().ok_or_else(|| Error::new("描画の XML が空です"))?;
    let id = doc.descendants(root).into_iter().filter(|&n| doc.local_name(n) == "cNvPr").filter_map(|n| doc.attr(n, "id")?.parse::<u32>().ok()).max().unwrap_or(1) + 1;
    // The fallback shape has an id of its own too (ids are unique within a drawing).
    let fallback_id = id + 1;
    let name = escape(&slicer.name);
    let (requires, declaration) = if table { ("sle15", "xmlns:sle15=\"http://schemas.microsoft.com/office/drawing/2012/slicer\"") } else { ("a14", "xmlns:a14=\"http://schemas.microsoft.com/office/drawing/2010/main\"") };
    let what = if table { "Table slicer" } else { "Slicer" };
    let xml = format!(
        "<xdr:twoCellAnchor xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" editAs=\"oneCell\">\
<xdr:from><xdr:col>{c0}</xdr:col><xdr:colOff>{co0}</xdr:colOff><xdr:row>{r0}</xdr:row><xdr:rowOff>{ro0}</xdr:rowOff></xdr:from>\
<xdr:to><xdr:col>{c1}</xdr:col><xdr:colOff>{co1}</xdr:colOff><xdr:row>{r1}</xdr:row><xdr:rowOff>{ro1}</xdr:rowOff></xdr:to>\
<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice {declaration} Requires=\"{requires}\">\
<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"{name}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm>\
<a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/drawing/2010/slicer\"><sle:slicer xmlns:sle=\"http://schemas.microsoft.com/office/drawing/2010/slicer\" name=\"{name}\"/></a:graphicData></a:graphic></xdr:graphicFrame></mc:Choice>\
<mc:Fallback><xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"{fallback_id}\" name=\"\"/><xdr:cNvSpPr><a:spLocks noTextEdit=\"1\"/></xdr:cNvSpPr></xdr:nvSpPr>\
<xdr:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:solidFill><a:prstClr val=\"white\"/></a:solidFill><a:ln w=\"1\"><a:solidFill><a:prstClr val=\"green\"/></a:solidFill></a:ln></xdr:spPr>\
<xdr:txBody><a:bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\"/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\" sz=\"1100\"/><a:t>This shape represents a {what_lower}. {what} is supported in Excel 2010 or later.</a:t></a:r></a:p></xdr:txBody></xdr:sp></mc:Fallback></mc:AlternateContent><xdr:clientData/></xdr:twoCellAnchor>",
        x = f.x,
        y = f.y,
        w = f.width,
        h = f.height,
        what_lower = what.to_lowercase(),
    );
    let fragment = fragment(&xml)?;
    let node = fragment.root_element().ok_or_else(|| Error::new("XML の断片が空です"))?;
    let imported = doc.import(&fragment, node);
    let count = doc.children(root).len();
    doc.insert(root, count, imported);
    Ok(())
}

/// The slicer's entry in a slicers part of the sheet (made, with the
/// sheet's `slicerList`, when there is none for its kind).
fn add_slicer_entry(package: &mut Package, sheet_part: &str, slicer: &Slicer, table: bool) -> Result<(), Error> {
    let uri = if table { SHEET_EXT_TABLE } else { SHEET_EXT_PIVOT };
    let mut attributes = format!("name=\"{}\" cache=\"{}\" caption=\"{}\" rowHeight=\"{}\"", escape(&slicer.name), escape(&slicer.cache), escape(&slicer.caption), slicer.button_height());
    if !slicer.show_caption {
        attributes.push_str(" showCaption=\"0\"");
    }
    if slicer.columns > 1 {
        attributes.push_str(&format!(" columnCount=\"{}\"", slicer.columns));
    }
    if !slicer.style.is_empty() {
        attributes.push_str(&format!(" style=\"{}\"", escape(&slicer.style)));
    }
    let entry = format!("<slicer xmlns=\"{X14}\" {attributes}/>");
    // The slicers part this kind already has: through the `slicerList`'s relationship.
    let existing = {
        let sheet = package.xml(sheet_part)?;
        let root = sheet.root_element().ok_or_else(|| Error::new("シート XML が空です"))?;
        let id = sheet
            .descendants(root)
            .into_iter()
            .filter(|&n| sheet.local_name(n) == "ext" && sheet.attr(n, "uri") == Some(uri))
            .flat_map(|e| sheet.descendants(e))
            .find(|&n| sheet.local_name(n) == "slicer")
            .and_then(|n| sheet.attr_local(n, "id").map(str::to_string));
        id.and_then(|id| package.relationships(sheet_part).ok()?.into_iter().find(|r| r.id == id)).and_then(|r| r.target_part(sheet_part))
    };
    if let Some(part) = existing {
        let fragment = fragment(&entry)?;
        let doc = package.xml_mut(&part)?;
        let root = doc.root_element().ok_or_else(|| Error::new("スライサーの XML が空です"))?;
        let node = fragment.root_element().ok_or_else(|| Error::new("XML の断片が空です"))?;
        let imported = doc.import(&fragment, node);
        let count = doc.children(root).len();
        doc.insert(root, count, imported);
        return Ok(());
    }
    let part = next_part(package, "xl/slicers", "slicer");
    let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<slicers xmlns=\"{X14}\" xmlns:x=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">{}</slicers>", entry.replacen(&format!(" xmlns=\"{X14}\""), "", 1));
    package.set_bytes(&part, xml.into_bytes());
    package.set_override(&part, "application/vnd.ms-excel.slicer+xml")?;
    let rid = package.add_relationship(sheet_part, REL_SLICER, &relative_to(sheet_part, &part), false)?;
    let sheet = package.xml_mut(sheet_part)?;
    let root = sheet.root_element().ok_or_else(|| Error::new("シート XML が空です"))?;
    let r = r_prefix(sheet, root);
    let list = ext_list(sheet, root);
    let ext = ext_with_uri(sheet, list, uri, "x14", X14);
    let found = sheet.children_named(ext, "slicerList").next();
    let slicer_list = match found {
        Some(l) => l,
        None => sheet.append_element(ext, "x14:slicerList"),
    };
    let el = sheet.append_element(slicer_list, "x14:slicer");
    sheet.set_attr(el, &format!("{r}:id"), &rid);
    Ok(())
}

/// The id Excel gives a pivot cache for its slicers (`x14:pivotCacheDefinition/@pivotCacheId`), made if the cache has none.
fn pivot_cache_id(package: &mut Package, pivot: &str) -> Result<Option<u32>, Error> {
    let names: Vec<String> = package.names().filter(|n| n.starts_with("xl/pivotTables/") && n.ends_with(".xml")).map(str::to_string).collect();
    for part in names {
        let doc = package.xml(&part)?;
        let Some(root) = doc.root_element() else { continue };
        if doc.attr(root, "name") != Some(pivot) {
            continue;
        }
        let Some(cache) = package.relationships(&part)?.into_iter().find(|r| r.kind.ends_with("/pivotCacheDefinition")).and_then(|r| r.target_part(&part)) else { return Ok(None) };
        let doc = package.xml(&cache)?;
        if let Some(root) = doc.root_element()
            && let Some(id) = doc.descendants(root).into_iter().find(|&n| doc.local_name(n) == "pivotCacheDefinition" && doc.attr(n, "pivotCacheId").is_some()).and_then(|n| doc.attr(n, "pivotCacheId")?.parse::<u32>().ok())
        {
            return Ok(Some(id));
        }
        // None yet: one made from the part's name, and written to the cache.
        let id = 100_000_000 + cache.bytes().fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b))) % 800_000_000;
        let doc = package.xml_mut(&cache)?;
        let root = doc.root_element().ok_or_else(|| Error::new("ピボット キャッシュの XML が空です"))?;
        let list = ext_list(doc, root);
        let ext = ext_with_uri(doc, list, "{725AE2AE-9491-48be-B2B4-4EB974FC3084}", "x14", X14);
        let el = doc.append_element(ext, "x14:pivotCacheDefinition");
        doc.set_attr(el, "pivotCacheId", &id.to_string());
        return Ok(Some(id));
    }
    Ok(None)
}

/// A table's `id` and the `id` of its column `field`, from its table part (or from `current` if the table is newly added in this save).
fn table_ids(package: &Package, current: &Presentation, table: &str, field: &str) -> Option<(u32, u32)> {
    if let Some(found) = package.names().filter(|n| n.starts_with("xl/tables/") && n.ends_with(".xml")).find_map(|part| {
        let doc = package.xml(part).ok()?;
        let root = doc.root_element()?;
        let named = doc.attr(root, "name") == Some(table) || doc.attr(root, "displayName") == Some(table);
        if !named {
            return None;
        }
        let id = doc.attr(root, "id")?.parse().ok()?;
        let columns = doc.child(root, "tableColumns")?;
        let column = doc.children_named(columns, "tableColumn").find(|&c| doc.attr(c, "name") == Some(field))?;
        Some((id, doc.attr(column, "id")?.parse().ok()?))
    }) {
        return Some(found);
    }
    let mut next_id = 0u32;
    let mut existing_names = std::collections::HashSet::new();
    for part in package.names().filter(|n| n.starts_with("xl/tables/") && n.ends_with(".xml")) {
        if let Ok(doc) = package.xml(part)
            && let Some(root) = doc.root_element()
        {
            next_id = next_id.max(doc.attr(root, "id").and_then(|v| v.parse().ok()).unwrap_or(0));
            if let Some(n) = doc.attr(root, "name") {
                existing_names.insert(n.to_lowercase());
            }
        }
    }
    for slide in &current.slides {
        let Some(sheet) = super::sheet_of(slide) else { continue };
        for t in &sheet.tables {
            if !existing_names.contains(&t.name.to_lowercase()) {
                next_id += 1;
                if t.name == table {
                    let col = t.columns.iter().position(|c| c == field)? as u32 + 1;
                    return Some((next_id, col));
                }
            }
        }
    }
    None
}

fn add_slicer_cache(package: &mut Package, map: &super::SourceMap, current: &Presentation, slicer: &Slicer) -> Result<(), Error> {
    let part = next_part(package, "xl/slicerCaches", "slicerCache");
    let head = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<slicerCacheDefinition xmlns=\"{X14}\" xmlns:x=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"");
    let sort_attr = if slicer.descending { " sortOrder=\"descending\"" } else { "" };
    let (xml, table) = match &slicer.source {
        SlicerSource::Table { table, field } => {
            let (table_id, column) = table_ids(package, current, table, field).ok_or_else(|| Error::new("スライサーのテーブルまたは列が見つかりません"))?;
            (format!("{head} name=\"{}\" sourceName=\"{}\"><extLst><x:ext uri=\"{{2F2917AC-EB37-4324-AD4E-5DD8C200BD13}}\" xmlns:x15=\"{X15}\"><x15:tableSlicerCache tableId=\"{table_id}\" column=\"{column}\"{sort_attr}/></x:ext></extLst></slicerCacheDefinition>", escape(&slicer.cache), escape(field)), true)
        }
        SlicerSource::Pivot { field, pivots } => {
            let pivot = pivots.first().ok_or_else(|| Error::new("スライサーのピボットテーブルがありません"))?;
            let cache_id = pivot_cache_id(package, pivot)?.ok_or_else(|| Error::new("ピボット テーブルのキャッシュが見つかりません"))?;
            let tab_of = |name: &str| -> Result<u32, Error> {
                let (slide, _) = ibis_doc_model::slicer::locate_pivot(current, name).ok_or_else(|| Error::new("ピボット テーブルが見つかりません"))?;
                current.workbook.as_deref().and_then(|w| w.sheets.iter().position(|s| s.slide == Some(slide))).and_then(|i| map.sheet_ids.get(i).copied()).ok_or_else(|| Error::new("ピボット テーブルのシートが見つかりません"))
            };
            let indexes = shared_indexes(current, slicer).unwrap_or_default();
            let items: String = indexes
                .iter()
                .enumerate()
                .map(|(k, x)| {
                    let selected = slicer.items.get(k).is_none_or(|it| it.selected);
                    if selected {
                        format!("<i x=\"{x}\" s=\"1\"/>")
                    } else {
                        format!("<i x=\"{x}\"/>")
                    }
                })
                .collect();
            let mut pivots_xml = String::new();
            for p in pivots {
                let tab = tab_of(p)?;
                pivots_xml.push_str(&format!("<pivotTable tabId=\"{tab}\" name=\"{}\"/>", escape(p)));
            }
            (format!("{head} name=\"{}\" sourceName=\"{}\"><pivotTables>{pivots_xml}</pivotTables><data><tabular pivotCacheId=\"{cache_id}\"{sort_attr}><items count=\"{}\">{items}</items></tabular></data></slicerCacheDefinition>", escape(&slicer.cache), escape(field), indexes.len()), false)
        }
    };
    package.set_bytes(&part, xml.into_bytes());
    package.set_override(&part, "application/vnd.ms-excel.slicerCache+xml")?;
    let rid = package.add_relationship(&map.workbook, REL_SLICER_CACHE, &relative_to(&map.workbook, &part), false)?;
    let wb = package.xml_mut(&map.workbook)?;
    let root = wb.root_element().ok_or_else(|| Error::new("ブックの XML が空です"))?;
    let r = r_prefix(wb, root);
    let list = ext_list(wb, root);
    let (uri, prefix, namespace) = if table { (BOOK_EXT_TABLE, "x15", X15) } else { (BOOK_EXT_PIVOT, "x14", X14) };
    let ext = ext_with_uri(wb, list, uri, prefix, namespace);
    let caches_name = if table { "x15:slicerCaches" } else { "x14:slicerCaches" };
    let found = wb.children_named(ext, "slicerCaches").next();
    let caches = match found {
        Some(c) => c,
        None => {
            let c = wb.append_element(ext, caches_name);
            if table {
                wb.set_attr(c, "xmlns:x14", X14);
            }
            c
        }
    };
    let el = wb.append_element(caches, "x14:slicerCache");
    wb.set_attr(el, &format!("{r}:id"), &rid);
    Ok(())
}

/// Slicers whose settings changed in the app (the heading, the columns of
/// buttons, the button's height, whether the heading shows, and the pivot
/// tables a pivot slicer is connected to): their entries in the slicers
/// parts and their caches are written anew.
pub(super) fn write_slicer_settings(package: &mut Package, map: &super::SourceMap, baseline: &Presentation, current: &Presentation) -> Result<(), Error> {
    let (old, new) = (slicers_of(baseline), slicers_of(current));
    let parts: Vec<String> = package.names().filter(|n| n.starts_with("xl/slicers/") && n.ends_with(".xml")).map(str::to_string).collect();
    let drawings: Vec<String> = package.names().filter(|n| n.starts_with("xl/drawings/") && n.ends_with(".xml")).map(str::to_string).collect();
    let caches: Vec<String> = package.names().filter(|n| n.starts_with("xl/slicerCaches/") && n.ends_with(".xml")).map(str::to_string).collect();
    for (k, s) in &new {
        let Some(o) = old.get(k).or_else(|| old.values().find(|o| o.name == s.name)) else { continue };
        if (o.name.as_str(), o.caption.as_str(), o.columns, o.row_height, o.show_caption, o.style.as_str()) != (s.name.as_str(), s.caption.as_str(), s.columns, s.row_height, s.show_caption, s.style.as_str()) {
            for part in &parts {
                let doc = package.xml_mut(part)?;
                let Some(root) = doc.root_element() else { continue };
                let Some(entry) = doc.children_named(root, "slicer").find(|&n| doc.attr(n, "name") == Some(o.name.as_str()) || doc.attr(n, "name") == Some(s.name.as_str())) else { continue };
                doc.set_attr(entry, "name", &s.name);
                doc.set_attr(entry, "caption", &s.caption);
                if s.style.is_empty() {
                    doc.remove_attr(entry, "style");
                } else {
                    doc.set_attr(entry, "style", &s.style);
                }
                if s.columns > 1 {
                    doc.set_attr(entry, "columnCount", &s.columns.to_string());
                } else {
                    doc.remove_attr(entry, "columnCount");
                }
                if s.row_height > 0 {
                    doc.set_attr(entry, "rowHeight", &s.row_height.to_string());
                }
                if s.show_caption {
                    doc.remove_attr(entry, "showCaption");
                } else {
                    doc.set_attr(entry, "showCaption", "0");
                }
            }
            if o.name != s.name {
                for drawing in &drawings {
                    let doc = package.xml_mut(drawing)?;
                    let Some(root) = doc.root_element() else { continue };
                    for n in doc.descendants(root) {
                        if doc.local_name(n) == "slicer" && doc.attr(n, "name") == Some(o.name.as_str()) {
                            doc.set_attr(n, "name", &s.name);
                            let mut p = doc.parent(n);
                            while let Some(curr) = p {
                                if doc.local_name(curr) == "graphicFrame" {
                                    if let Some(c_nv_pr) = doc.path(curr, &["nvGraphicFramePr", "cNvPr"]) {
                                        doc.set_attr(c_nv_pr, "name", &s.name);
                                    }
                                    break;
                                }
                                p = doc.parent(curr);
                            }
                        }
                    }
                }
            }
        }
        if o.descending != s.descending {
            for part in &caches {
                let doc = package.xml_mut(part)?;
                let Some(root) = doc.root_element() else { continue };
                if doc.attr(root, "name") != Some(s.cache.as_str()) {
                    continue;
                }
                for n in doc.descendants(root) {
                    if matches!(doc.local_name(n), "tabular" | "tableSlicerCache") {
                        if s.descending {
                            doc.set_attr(n, "sortOrder", "descending");
                        } else {
                            doc.remove_attr(n, "sortOrder");
                        }
                    }
                }
            }
        }
        let (SlicerSource::Pivot { pivots: before, .. }, SlicerSource::Pivot { pivots: after, .. }) = (&o.source, &s.source) else { continue };
        if before == after {
            continue;
        }
        let tab_of = |name: &str| -> Option<u32> {
            let (slide, _) = ibis_doc_model::slicer::locate_pivot(current, name)?;
            let i = current.workbook.as_deref()?.sheets.iter().position(|w| w.slide == Some(slide))?;
            map.sheet_ids.get(i).copied()
        };
        for part in &caches {
            let doc = package.xml_mut(part)?;
            let Some(root) = doc.root_element() else { continue };
            if doc.attr(root, "name") != Some(s.cache.as_str()) {
                continue;
            }
            let Some(list) = doc.child(root, "pivotTables") else { continue };
            for n in doc.children(list).to_vec() {
                doc.detach(n);
            }
            for name in after {
                let Some(tab) = tab_of(name) else { continue };
                let el = doc.append_element(list, "pivotTable");
                doc.set_attr(el, "tabId", &tab.to_string());
                doc.set_attr(el, "name", name);
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------ timelines

const REL_TIMELINE: &str = "http://schemas.microsoft.com/office/2011/relationships/timeline";
const REL_TIMELINE_CACHE: &str = "http://schemas.microsoft.com/office/2011/relationships/timelineCache";
const SHEET_EXT_TIMELINE: &str = "{7E03D99C-DC04-49d9-9315-930204A7B6E9}";
const BOOK_EXT_TIMELINE: &str = "{7F5E0A19-D2C0-4B19-A40A-B593EB0098A9}";

fn timelines_of(p: &Presentation) -> HashMap<(ibis_doc_model::SlideId, ibis_doc_model::ShapeId), ibis_doc_model::timeline::Timeline> {
    p.slides.iter().flat_map(|sl| sl.shapes.iter().filter_map(move |sh| match &sh.kind {
        ShapeKind::Timeline(t) => Some(((sl.id, sh.id), (**t).clone())),
        _ => None,
    })).collect()
}

fn timeline_date(serial: i64) -> String {
    let (y, m, d) = ibis_calc::eval::convert::civil_from_days(serial - 25_569);
    format!("{y:04}-{m:02}-{d:02}T00:00:00")
}

/// Update the drawing anchor when a timeline shape is moved or resized.
pub(super) fn write_timeline_frames(package: &mut Package, sheet_part: &str, sheet: &ibis_doc_model::Sheet, old: &[ibis_doc_model::Shape], new: &[ibis_doc_model::Shape]) -> Result<(), Error> {
    let moved: Vec<(&str, ibis_doc_model::EmuRect)> = new
        .iter()
        .filter_map(|n| match &n.kind {
            ShapeKind::Timeline(t) => old.iter().find(|o| o.id == n.id).filter(|o| o.frame != n.frame).map(|o| {
                let name = match &o.kind {
                    ShapeKind::Timeline(ot) => ot.name.as_str(),
                    _ => t.name.as_str(),
                };
                (name, n.frame)
            }),
            _ => None,
        })
        .collect();
    if moved.is_empty() {
        return Ok(());
    }
    let drawing = package
        .relationships(sheet_part)?
        .iter()
        .find(|r| r.kind.ends_with("/drawing"))
        .and_then(|r| r.target_part(sheet_part))
        .ok_or_else(|| Error::new("タイムラインの描画が見つかりません"))?;
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let cell = |offsets: &[i64], v: i64| -> (usize, i64) {
        let i = offsets.partition_point(|&o| o <= v).saturating_sub(1).min(offsets.len().saturating_sub(2));
        (i, v - offsets.get(i).copied().unwrap_or(0))
    };
    let doc = package.xml_mut(&drawing)?;
    let root = doc.root_element().ok_or_else(|| Error::new("描画の XML が空です"))?;
    for (name, frame) in moved {
        let node = doc.descendants(root).into_iter().find(|&n| doc.local_name(n) == "timeslicer" && doc.attr(n, "name") == Some(name));
        let mut anchor = node;
        while let Some(a) = anchor {
            if matches!(doc.local_name(a), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor") {
                break;
            }
            anchor = doc.parent(a);
        }
        let Some(anchor) = anchor else { continue };
        let kind = doc.local_name(anchor).to_string();
        if kind == "absoluteAnchor" {
            if let Some(p) = doc.child(anchor, "pos") {
                doc.set_attr(p, "x", &frame.x.to_string());
                doc.set_attr(p, "y", &frame.y.to_string());
            }
            if let Some(e) = doc.child(anchor, "ext") {
                doc.set_attr(e, "cx", &frame.width.to_string());
                doc.set_attr(e, "cy", &frame.height.to_string());
            }
            continue;
        }
        let corners: Vec<(&str, i64, i64)> = if kind == "twoCellAnchor" {
            vec![("from", frame.x, frame.y), ("to", frame.x + frame.width, frame.y + frame.height)]
        } else {
            vec![("from", frame.x, frame.y)]
        };
        for (corner_name, x, y) in corners {
            let Some(corner) = doc.child(anchor, corner_name) else { continue };
            let ((c, co), (r, ro)) = (cell(&xs, x), cell(&ys, y));
            for (n, v) in [("col", c as i64), ("colOff", co), ("row", r as i64), ("rowOff", ro)] {
                if let Some(e) = doc.child(corner, n) {
                    doc.set_text(e, &v.to_string());
                }
            }
        }
        if kind == "oneCellAnchor"
            && let Some(e) = doc.child(anchor, "ext")
        {
            doc.set_attr(e, "cx", &frame.width.to_string());
            doc.set_attr(e, "cy", &frame.height.to_string());
        }
    }
    Ok(())
}

/// Remove deleted timelines from the drawing, sheet timeline parts, and workbook timeline caches.
pub(super) fn write_timeline_removals(
    package: &mut Package,
    sheet_part: &str,
    workbook_part: &str,
    removed: &[ibis_doc_model::timeline::Timeline],
    remaining: &[ibis_doc_model::timeline::Timeline],
) -> Result<(), Error> {
    if removed.is_empty() {
        return Ok(());
    }
    if let Some(drawing) = package.relationships(sheet_part)?.iter().find(|r| r.kind.ends_with("/drawing")).and_then(|r| r.target_part(sheet_part)) {
        let doc = package.xml_mut(&drawing)?;
        if let Some(root) = doc.root_element() {
            for t in removed {
                let node = doc.descendants(root).into_iter().find(|&n| doc.local_name(n) == "timeslicer" && doc.attr(n, "name") == Some(t.name.as_str()));
                let mut anchor = node;
                while let Some(a) = anchor {
                    if matches!(doc.local_name(a), "twoCellAnchor" | "oneCellAnchor" | "absoluteAnchor") {
                        break;
                    }
                    anchor = doc.parent(a);
                }
                if let Some(a) = anchor {
                    let target = doc.parent(a).filter(|&p| doc.local_name(p) == "Choice").and_then(|c| doc.parent(c)).filter(|&ac| doc.local_name(ac) == "AlternateContent").unwrap_or(a);
                    doc.detach(target);
                }
            }
        }
    }
    for rel in package.relationships(sheet_part)?.into_iter().filter(|r| r.kind.ends_with("/timeline")) {
        let Some(part) = rel.target_part(sheet_part) else { continue };
        let doc = package.xml_mut(&part)?;
        let Some(root) = doc.root_element() else { continue };
        for n in doc.children_named(root, "timeline").collect::<Vec<_>>() {
            if doc.attr(n, "name").is_some_and(|name| removed.iter().any(|t| t.name == name)) {
                doc.detach(n);
            }
        }
        if doc.children_named(root, "timeline").next().is_some() {
            continue;
        }
        package.remove_relationship(sheet_part, &rel.id)?;
        package.remove_override(&part)?;
        package.remove(&part);
        let sheet = package.xml_mut(sheet_part)?;
        let Some(sheet_root) = sheet.root_element() else { continue };
        for n in sheet.descendants(sheet_root) {
            if sheet.local_name(n) == "timeline" && sheet.attr_local(n, "id") == Some(rel.id.as_str()) {
                sheet.detach(n);
            }
        }
        for list in sheet.descendants(sheet_root).into_iter().filter(|&n| sheet.local_name(n) == "timelines").collect::<Vec<_>>() {
            if sheet.children(list).iter().all(|&c| !sheet.is_element(c)) {
                let ext = sheet.parent(list).filter(|&e| sheet.local_name(e) == "ext");
                let ext_lst = ext.and_then(|e| sheet.parent(e));
                if let Some(e) = ext {
                    sheet.detach(e);
                }
                if let Some(l) = ext_lst.filter(|&l| sheet.local_name(l) == "extLst" && sheet.elements(l).next().is_none()) {
                    sheet.detach(l);
                }
            }
        }
    }
    let unused: Vec<&str> = removed.iter().map(|t| t.cache.as_str()).filter(|c| !remaining.iter().any(|t| t.cache == *c)).collect();
    if unused.is_empty() {
        return Ok(());
    }
    for rel in package.relationships(workbook_part)?.into_iter().filter(|r| r.kind.ends_with("/timelineCache")) {
        let Some(part) = rel.target_part(workbook_part) else { continue };
        let name = package.xml(&part).ok().and_then(|d| d.root_element().and_then(|r| d.attr(r, "name").map(str::to_string)));
        if !name.as_deref().is_some_and(|n| unused.contains(&n)) {
            continue;
        }
        package.remove_relationship(workbook_part, &rel.id)?;
        package.remove_override(&part)?;
        package.remove(&part);
        let wb = package.xml_mut(workbook_part)?;
        let Some(wb_root) = wb.root_element() else { continue };
        for n in wb.descendants(wb_root) {
            if wb.local_name(n) == "timelineCacheRef" && wb.attr_local(n, "id") == Some(rel.id.as_str()) {
                wb.detach(n);
            }
        }
        for list in wb.descendants(wb_root).into_iter().filter(|&n| wb.local_name(n) == "timelineCacheRefs").collect::<Vec<_>>() {
            if wb.elements(list).next().is_none() {
                let ext = wb.parent(list).filter(|&e| wb.local_name(e) == "ext");
                let ext_lst = ext.and_then(|e| wb.parent(e));
                if let Some(e) = ext {
                    wb.detach(e);
                }
                if let Some(l) = ext_lst.filter(|&l| wb.local_name(l) == "extLst" && wb.elements(l).next().is_none()) {
                    wb.detach(l);
                }
            }
        }
    }
    Ok(())
}

/// Add a newly inserted timeline shape: its drawing anchor, sheet timeline part, and workbook timeline cache part.
pub(super) fn add_timeline(
    package: &mut Package,
    map: &super::SourceMap,
    sheet_part: &str,
    sheet: &ibis_doc_model::Sheet,
    shape: &ibis_doc_model::Shape,
    timeline: &ibis_doc_model::timeline::Timeline,
    current: &Presentation,
) -> Result<(), Error> {
    let drawing = match package.relationships(sheet_part)?.into_iter().find(|r| r.kind.ends_with("/drawing") && !r.external) {
        Some(rel) => rel.target_part(sheet_part).ok_or_else(|| Error::new("シートの描画パートが見つかりません"))?,
        None => super::create_drawing(package, sheet_part)?,
    };
    add_timeline_anchor(package, &drawing, sheet, shape, timeline)?;
    add_timeline_entry(package, sheet_part, timeline)?;
    add_timeline_cache(package, map, current, timeline)?;
    Ok(())
}

fn add_timeline_anchor(
    package: &mut Package,
    drawing: &str,
    sheet: &ibis_doc_model::Sheet,
    shape: &ibis_doc_model::Shape,
    timeline: &ibis_doc_model::timeline::Timeline,
) -> Result<(), Error> {
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let cell = |offsets: &[i64], v: i64| -> (usize, i64) {
        let i = offsets.partition_point(|&o| o <= v).saturating_sub(1).min(offsets.len().saturating_sub(2));
        (i, v - offsets.get(i).copied().unwrap_or(0))
    };
    let f = shape.frame;
    let ((c0, co0), (r0, ro0)) = (cell(&xs, f.x), cell(&ys, f.y));
    let ((c1, co1), (r1, ro1)) = (cell(&xs, f.x + f.width), cell(&ys, f.y + f.height));
    let doc = package.xml_mut(drawing)?;
    let root = doc.root_element().ok_or_else(|| Error::new("描画の XML が空です"))?;
    let id = doc.descendants(root).into_iter().filter(|&n| doc.local_name(n) == "cNvPr").filter_map(|n| doc.attr(n, "id")?.parse::<u32>().ok()).max().unwrap_or(1) + 1;
    let fallback_id = id + 1;
    let name = escape(&timeline.name);
    let xml = format!(
        "<xdr:twoCellAnchor xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" editAs=\"oneCell\">\
<xdr:from><xdr:col>{c0}</xdr:col><xdr:colOff>{co0}</xdr:colOff><xdr:row>{r0}</xdr:row><xdr:rowOff>{ro0}</xdr:rowOff></xdr:from>\
<xdr:to><xdr:col>{c1}</xdr:col><xdr:colOff>{co1}</xdr:colOff><xdr:row>{r1}</xdr:row><xdr:rowOff>{ro1}</xdr:rowOff></xdr:to>\
<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice xmlns:tsle=\"http://schemas.microsoft.com/office/drawing/2012/timeslicer\" Requires=\"tsle\">\
<xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{id}\" name=\"{name}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm>\
<a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/drawing/2012/timeslicer\"><tsle:timeslicer name=\"{name}\"/></a:graphicData></a:graphic></xdr:graphicFrame></mc:Choice>\
<mc:Fallback><xdr:sp macro=\"\" textlink=\"\"><xdr:nvSpPr><xdr:cNvPr id=\"{fallback_id}\" name=\"\"/><xdr:cNvSpPr><a:spLocks noTextEdit=\"1\"/></xdr:cNvSpPr></xdr:nvSpPr>\
<xdr:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:solidFill><a:prstClr val=\"white\"/></a:solidFill><a:ln w=\"1\"><a:solidFill><a:prstClr val=\"green\"/></a:solidFill></a:ln></xdr:spPr>\
<xdr:txBody><a:bodyPr vertOverflow=\"clip\" horzOverflow=\"clip\"/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\" sz=\"1100\"/><a:t>This shape represents a timeline. Timelines are supported in Excel 2013 or later.</a:t></a:r></a:p></xdr:txBody></xdr:sp></mc:Fallback></mc:AlternateContent><xdr:clientData/></xdr:twoCellAnchor>",
        x = f.x,
        y = f.y,
        w = f.width,
        h = f.height,
    );
    let fragment = fragment(&xml)?;
    let node = fragment.root_element().ok_or_else(|| Error::new("XML の断片が空です"))?;
    let imported = doc.import(&fragment, node);
    let count = doc.children(root).len();
    doc.insert(root, count, imported);
    Ok(())
}

fn add_timeline_entry(package: &mut Package, sheet_part: &str, timeline: &ibis_doc_model::timeline::Timeline) -> Result<(), Error> {
    let mut attributes = format!(
        "name=\"{}\" cache=\"{}\" caption=\"{}\" level=\"{}\" selectionLevel=\"{}\" scrollPosition=\"{}\"",
        escape(&timeline.name),
        escape(&timeline.cache),
        escape(&timeline.caption),
        timeline.level,
        timeline.selection_level,
        timeline_date(timeline.scroll),
    );
    if !timeline.show_header {
        attributes.push_str(" showHeader=\"0\"");
    }
    if !timeline.show_selection_label {
        attributes.push_str(" showSelectionLabel=\"0\"");
    }
    if !timeline.show_time_level {
        attributes.push_str(" showTimeLevel=\"0\"");
    }
    if !timeline.show_scroll_bar {
        attributes.push_str(" showHorizontalScrollbar=\"0\"");
    }
    if !timeline.style.is_empty() {
        attributes.push_str(&format!(" style=\"{}\"", escape(&timeline.style)));
    }
    let entry = format!("<timeline xmlns=\"{X15}\" {attributes}/>");
    let existing = {
        let sheet = package.xml(sheet_part)?;
        let root = sheet.root_element().ok_or_else(|| Error::new("シート XML が空です"))?;
        let id = sheet
            .descendants(root)
            .into_iter()
            .filter(|&n| sheet.local_name(n) == "ext" && sheet.attr(n, "uri") == Some(SHEET_EXT_TIMELINE))
            .flat_map(|e| sheet.descendants(e))
            .find(|&n| sheet.local_name(n) == "timeline")
            .and_then(|n| sheet.attr_local(n, "id").map(str::to_string));
        id.and_then(|id| package.relationships(sheet_part).ok()?.into_iter().find(|r| r.id == id)).and_then(|r| r.target_part(sheet_part))
    };
    if let Some(part) = existing {
        let fragment = fragment(&entry)?;
        let doc = package.xml_mut(&part)?;
        let root = doc.root_element().ok_or_else(|| Error::new("タイムラインの XML が空です"))?;
        let node = fragment.root_element().ok_or_else(|| Error::new("XML の断片が空です"))?;
        let imported = doc.import(&fragment, node);
        let count = doc.children(root).len();
        doc.insert(root, count, imported);
        return Ok(());
    }
    let part = next_part(package, "xl/timelines", "timeline");
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<timelines xmlns=\"{X15}\" xmlns:x=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\">{}</timelines>",
        entry.replacen(&format!(" xmlns=\"{X15}\""), "", 1)
    );
    package.set_bytes(&part, xml.into_bytes());
    package.set_override(&part, "application/vnd.ms-excel.timeline+xml")?;
    let rid = package.add_relationship(sheet_part, REL_TIMELINE, &relative_to(sheet_part, &part), false)?;
    let sheet = package.xml_mut(sheet_part)?;
    let root = sheet.root_element().ok_or_else(|| Error::new("シート XML が空です"))?;
    let r = r_prefix(sheet, root);
    let list = ext_list(sheet, root);
    let ext = ext_with_uri(sheet, list, SHEET_EXT_TIMELINE, "x15", X15);
    let found = sheet.children_named(ext, "timelines").next();
    let timelines_node = match found {
        Some(l) => l,
        None => sheet.append_element(ext, "x15:timelines"),
    };
    let el = sheet.append_element(timelines_node, "x15:timeline");
    sheet.set_attr(el, &format!("{r}:id"), &rid);
    Ok(())
}

fn add_timeline_cache(
    package: &mut Package,
    map: &super::SourceMap,
    current: &Presentation,
    timeline: &ibis_doc_model::timeline::Timeline,
) -> Result<(), Error> {
    let part = next_part(package, "xl/timelineCaches", "timelineCache");
    let pivots_xml: String = timeline
        .pivots
        .iter()
        .filter_map(|p| {
            let (slide, _) = ibis_doc_model::slicer::locate_pivot(current, p)?;
            let tab = current.workbook.as_deref()?.sheets.iter().position(|s| s.slide == Some(slide)).and_then(|i| map.sheet_ids.get(i).copied())?;
            Some(format!("<pivotTable tabId=\"{tab}\" name=\"{}\"/>", escape(p)))
        })
        .collect();
    let selection_xml = match timeline.selection {
        Some((a, b)) => format!("<selection startDate=\"{}\" endDate=\"{}\"/>", timeline_date(a), timeline_date((b - 1).max(a))),
        None => String::new(),
    };
    let bounds_xml = format!(
        "<bounds startDate=\"{}\" endDate=\"{}\"/>",
        timeline_date(timeline.bounds.0),
        timeline_date((timeline.bounds.1 - 1).max(timeline.bounds.0)),
    );
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<timelineCacheDefinition xmlns=\"{X15}\" xmlns:x=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" name=\"{}\" sourceName=\"{}\"><pivotTables>{pivots_xml}</pivotTables><state>{selection_xml}{bounds_xml}</state></timelineCacheDefinition>",
        escape(&timeline.cache),
        escape(&timeline.source),
    );
    package.set_bytes(&part, xml.into_bytes());
    package.set_override(&part, "application/vnd.ms-excel.timelineCache+xml")?;
    let rid = package.add_relationship(&map.workbook, REL_TIMELINE_CACHE, &relative_to(&map.workbook, &part), false)?;
    let wb = package.xml_mut(&map.workbook)?;
    let root = wb.root_element().ok_or_else(|| Error::new("ブックの XML が空です"))?;
    let r = r_prefix(wb, root);
    let list = ext_list(wb, root);
    let ext = ext_with_uri(wb, list, BOOK_EXT_TIMELINE, "x15", X15);
    let found = wb.children_named(ext, "timelineCacheRefs").next();
    let caches = match found {
        Some(c) => c,
        None => wb.append_element(ext, "x15:timelineCacheRefs"),
    };
    let el = wb.append_element(caches, "x15:timelineCacheRef");
    wb.set_attr(el, &format!("{r}:id"), &rid);
    Ok(())
}

/// Write changes to existing timelines (name, caption, style, level, scroll, display flags, selection, and connected pivot tables).
pub(super) fn write_timeline_settings_and_state(
    package: &mut Package,
    map: &super::SourceMap,
    baseline: &Presentation,
    current: &Presentation,
) -> Result<(), Error> {
    let (old, new) = (timelines_of(baseline), timelines_of(current));
    let parts: Vec<String> = package.names().filter(|n| n.starts_with("xl/timelines/") && n.ends_with(".xml")).map(str::to_string).collect();
    let drawings: Vec<String> = package.names().filter(|n| n.starts_with("xl/drawings/") && n.ends_with(".xml")).map(str::to_string).collect();
    let caches: Vec<String> = package.names().filter(|n| n.starts_with("xl/timelineCaches/") && n.ends_with(".xml")).map(str::to_string).collect();
    for (k, t) in &new {
        let Some(o) = old.get(k).or_else(|| old.values().find(|o| o.cache == t.cache && o.name == t.name)) else { continue };
        if o == t {
            continue;
        }
        for part in &parts {
            let doc = package.xml_mut(part)?;
            let Some(root) = doc.root_element() else { continue };
            let Some(entry) = doc.children_named(root, "timeline").find(|&n| doc.attr(n, "name") == Some(o.name.as_str()) || doc.attr(n, "name") == Some(t.name.as_str())) else { continue };
            doc.set_attr(entry, "name", &t.name);
            doc.set_attr(entry, "caption", &t.caption);
            doc.set_attr(entry, "level", &t.level.to_string());
            doc.set_attr(entry, "selectionLevel", &t.selection_level.to_string());
            doc.set_attr(entry, "scrollPosition", &timeline_date(t.scroll));
            if t.style.is_empty() {
                doc.remove_attr(entry, "style");
            } else {
                doc.set_attr(entry, "style", &t.style);
            }
            for (attr, on) in [
                ("showHeader", t.show_header),
                ("showSelectionLabel", t.show_selection_label),
                ("showTimeLevel", t.show_time_level),
                ("showHorizontalScrollbar", t.show_scroll_bar),
            ] {
                if on {
                    doc.remove_attr(entry, attr);
                } else {
                    doc.set_attr(entry, attr, "0");
                }
            }
        }
        if o.name != t.name {
            for drawing in &drawings {
                let doc = package.xml_mut(drawing)?;
                let Some(root) = doc.root_element() else { continue };
                for n in doc.descendants(root) {
                    if doc.local_name(n) == "timeslicer" && doc.attr(n, "name") == Some(o.name.as_str()) {
                        doc.set_attr(n, "name", &t.name);
                        let mut p = doc.parent(n);
                        while let Some(curr) = p {
                            if doc.local_name(curr) == "graphicFrame" {
                                if let Some(c_nv_pr) = doc.path(curr, &["nvGraphicFramePr", "cNvPr"]) {
                                    doc.set_attr(c_nv_pr, "name", &t.name);
                                }
                                break;
                            }
                            p = doc.parent(curr);
                        }
                    }
                }
            }
        }
        if o.selection != t.selection || o.pivots != t.pivots {
            let tab_of = |name: &str| -> Option<u32> {
                let (slide, _) = ibis_doc_model::slicer::locate_pivot(current, name)?;
                let i = current.workbook.as_deref()?.sheets.iter().position(|w| w.slide == Some(slide))?;
                map.sheet_ids.get(i).copied()
            };
            for part in &caches {
                let doc = package.xml_mut(part)?;
                let Some(root) = doc.root_element() else { continue };
                if doc.attr(root, "name") != Some(t.cache.as_str()) {
                    continue;
                }
                if o.selection != t.selection
                    && let Some(state) = doc.child(root, "state")
                {
                    if let Some(old_sel) = doc.child(state, "selection") {
                        doc.detach(old_sel);
                    }
                    if let Some((a, b)) = t.selection {
                        let sel = doc.insert_element(state, 0, "selection");
                        doc.set_attr(sel, "startDate", &timeline_date(a));
                        doc.set_attr(sel, "endDate", &timeline_date((b - 1).max(a)));
                    }
                }
                if o.pivots != t.pivots
                    && let Some(list) = doc.child(root, "pivotTables")
                {
                    for n in doc.children(list).to_vec() {
                        doc.detach(n);
                    }
                    for name in &t.pivots {
                        let Some(tab) = tab_of(name) else { continue };
                        let el = doc.append_element(list, "pivotTable");
                        doc.set_attr(el, "tabId", &tab.to_string());
                        doc.set_attr(el, "name", name);
                    }
                }
            }
        }
    }
    Ok(())
}
