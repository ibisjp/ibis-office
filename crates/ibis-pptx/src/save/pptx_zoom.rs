// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ズーム (スライド/セクション/サマリー ズーム): a picture the editor made a zoom card (`Slide::zoom_objects`) is written the way
//! PowerPoint writes one ([MS-PPTX] 2.8.3, and a file PowerPoint for Mac wrote): an `mc:AlternateContent` whose `mc:Choice`
//! is the `p:graphicFrame` of the zoom (`pslz:sldZm`, `psez:sectionZm`, `psuz:summaryZm`) and whose `mc:Fallback` is the
//! picture (a group of the pictures for a summary), which draws it where the zoom is not known and links to its slide.
//! Pictures read from such a file are already in the `Fallback` and stay as they are.

use std::collections::HashMap;
use std::fmt::Write as _;

use ibis_doc_model::{EmuRect, Presentation, ShapeId, Slide, SlideId, ZoomObject, ZoomTarget};
use ibis_ooxml::{NodeId, Package, XmlDocument};

use super::pptx_new::Prefixes;
use crate::Error;

const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const P166: &str = "http://schemas.microsoft.com/office/powerpoint/2016/6/main";
const NS_SLIDE: &str = "http://schemas.microsoft.com/office/powerpoint/2016/slidezoom";
const NS_SECTION: &str = "http://schemas.microsoft.com/office/powerpoint/2016/sectionzoom";
const NS_SUMMARY: &str = "http://schemas.microsoft.com/office/powerpoint/2016/summaryzoom";

/// The numbers `p:sldId/@id` and the creation id (`p14:creationId`, 0 when it has none) a slide part is known by.
pub(crate) type SlideNumbers = HashMap<SlideId, (u32, u32)>;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The attributes of a `zmPr` for the object as the slide has it now (the properties the editor changes are the slide's own
/// sets; `zoom` gives the identity).
fn zm_pr_attrs(slide: &Slide, shape: ShapeId, zoom: &ZoomObject) -> String {
    let mut out = format!(" id=\"{}\"", esc(&zoom.id));
    if !slide.shape_return_to_zoom.contains(&shape) {
        out.push_str(" returnToParent=\"0\"");
    }
    if zoom.cover_image {
        out.push_str(" imageType=\"cover\"");
    }
    let dur = slide.shape_zoom_durations.get(&shape).copied().or(zoom.transition_dur).unwrap_or(1000);
    write!(out, " transitionDur=\"{dur}\"").unwrap();
    if slide.shape_zoom_show_bg.get(&shape) == Some(&false) {
        out.push_str(" showBg=\"0\"");
    }
    out
}

fn zm_body(embed: &str, size: (i64, i64), offset: Option<(i64, i64)>) -> String {
    let (ox, oy) = offset.unwrap_or((0, 0));
    format!(
        "<p166:blipFill xmlns:p166=\"{P166}\"><a:blip r:embed=\"{embed}\"/><a:stretch><a:fillRect/></a:stretch></p166:blipFill><p166:spPr xmlns:p166=\"{P166}\"><a:xfrm><a:off x=\"{ox}\" y=\"{oy}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:ln w=\"3175\"><a:solidFill><a:prstClr val=\"ltGray\"/></a:solidFill></a:ln></p166:spPr>",
        size.0, size.1
    )
}

fn frame_xml(id: &str, name: &str, frame: EmuRect, ns_prefix: &str, ns: &str, inner: &str) -> String {
    format!(
        "<mc:AlternateContent xmlns:mc=\"{MC}\"><mc:Choice xmlns:{ns_prefix}=\"{ns}\" Requires=\"{ns_prefix}\"><p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{id}\" name=\"{}\"/><p:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect=\"1\"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></p:xfrm><a:graphic><a:graphicData uri=\"{ns}\">{inner}</a:graphicData></a:graphic></p:graphicFrame></mc:Choice><mc:Fallback/></mc:AlternateContent>",
        esc(name),
        frame.x, frame.y, frame.width, frame.height
    )
}

struct Card {
    shape: ShapeId,
    zoom: ZoomObject,
    pic: NodeId,
    id: String,
    name: String,
    embed: String,
    frame: EmuRect,
}

fn cnv_pr(doc: &XmlDocument, pic: NodeId) -> Option<NodeId> {
    doc.descendants(pic).into_iter().find(|&n| doc.local_name(n) == "cNvPr")
}

/// Write the zoom cards of `current` that are not yet in the file's form. `native_ids` gives the drawing id of each new picture.
pub(crate) fn write(
    package: &mut Package,
    current: &Presentation,
    parts: &HashMap<SlideId, String>,
    numbers: &SlideNumbers,
    native_ids: &HashMap<SlideId, HashMap<ShapeId, String>>,
) -> Result<(), Error> {
    for slide in &current.slides {
        if slide.zoom_objects.is_empty() {
            continue;
        }
        let (Some(part), Some(ids)) = (parts.get(&slide.id), native_ids.get(&slide.id)) else { continue };
        let doc = package.xml_mut(part)?;
        let Some(root) = doc.root_element() else { continue };
        let all = doc.descendants(root);
        let mut cards = Vec::new();
        for (shape, zoom) in &slide.zoom_objects {
            let Some(native) = ids.get(shape) else { continue };
            let Some(pic) = all.iter().copied().find(|&n| doc.local_name(n) == "pic" && cnv_pr(doc, n).and_then(|c| doc.attr(c, "id")) == Some(native.as_str())) else { continue };
            // Already a fallback of the file's zoom: the file's own form stays.
            if doc.parent(pic).is_some_and(|p| matches!(doc.local_name(p), "Fallback" | "grpSp")) {
                continue;
            }
            let Some(embed) = doc.descendants(pic).into_iter().find(|&n| doc.local_name(n) == "blip").and_then(|b| doc.attr(b, "r:embed").or_else(|| doc.attr(b, "embed"))).map(str::to_string) else { continue };
            let Some(sh) = slide.shape(*shape) else { continue };
            let name = cnv_pr(doc, pic).and_then(|c| doc.attr(c, "name")).unwrap_or("").to_string();
            cards.push(Card { shape: *shape, zoom: zoom.clone(), pic, id: native.clone(), name, embed, frame: sh.frame });
        }
        cards.sort_by_key(|c| (matches!(c.zoom.target, ZoomTarget::Summary { .. }), match &c.zoom.target { ZoomTarget::Summary { index, .. } => *index, _ => 0 }, c.shape.0));
        let (singles, summary): (Vec<Card>, Vec<Card>) = cards.into_iter().partition(|c| !matches!(c.zoom.target, ZoomTarget::Summary { .. }));
        for card in singles {
            let (prefix, ns, inner) = match &card.zoom.target {
                ZoomTarget::Slide { slide: target, .. } => {
                    let Some((sld_id, cid)) = numbers.get(target).copied() else { continue };
                    let cid = if cid != 0 { format!(" cId=\"{cid}\"") } else { String::new() };
                    ("pslz", NS_SLIDE, format!("<pslz:sldZm><pslz:sldZmObj sldId=\"{sld_id}\"{cid}><pslz:zmPr{}>{}</pslz:zmPr></pslz:sldZmObj></pslz:sldZm>", zm_pr_attrs(slide, card.shape, &card.zoom), zm_body(&card.embed, (card.frame.width, card.frame.height), None)))
                }
                ZoomTarget::Section { section_id } => ("psez", NS_SECTION, format!("<psez:sectionZm><psez:sectionZmObj sectionId=\"{}\"><psez:zmPr{}>{}</psez:zmPr></psez:sectionZmObj></psez:sectionZm>", esc(section_id), zm_pr_attrs(slide, card.shape, &card.zoom), zm_body(&card.embed, (card.frame.width, card.frame.height), None))),
                ZoomTarget::Summary { .. } => continue,
            };
            let xml = frame_xml(&card.id, &card.name, card.frame, prefix, ns, &inner);
            wrap(doc, card.pic, &xml, None)?;
        }
        if !summary.is_empty() {
            let (x0, y0) = (summary.iter().map(|c| c.frame.x).min().unwrap_or(0), summary.iter().map(|c| c.frame.y).min().unwrap_or(0));
            let (x1, y1) = (summary.iter().map(|c| c.frame.x + c.frame.width).max().unwrap_or(0), summary.iter().map(|c| c.frame.y + c.frame.height).max().unwrap_or(0));
            let frame = EmuRect::new(x0, y0, x1 - x0, y1 - y0);
            let mut objects = String::new();
            for card in &summary {
                let ZoomTarget::Summary { section_id, .. } = &card.zoom.target else { continue };
                write!(objects, "<psuz:summaryZmObj sectionId=\"{}\"><psuz:zmPr{}>{}</psuz:zmPr></psuz:summaryZmObj>", esc(section_id), zm_pr_attrs(slide, card.shape, &card.zoom), zm_body(&card.embed, (card.frame.width, card.frame.height), Some((card.frame.x - x0, card.frame.y - y0)))).unwrap();
            }
            let inner = format!("<psuz:summaryZm>{objects}<psuz:gridLayout/></psuz:summaryZm>");
            // The frame and the group that stands for it take an id no shape of the slide has.
            let next = doc.descendants(root).into_iter().filter(|&n| doc.local_name(n) == "cNvPr").filter_map(|n| doc.attr(n, "id").and_then(|v| v.parse::<u32>().ok())).max().unwrap_or(1) + 1;
            let xml = frame_xml(&next.to_string(), "サマリー ズーム", frame, "psuz", NS_SUMMARY, &inner);
            let group = format!(
                "<p:grpSp><p:nvGrpSpPr><p:cNvPr id=\"{next}\" name=\"サマリー ズーム\"/><p:cNvGrpSpPr><a:grpSpLocks noGrp=\"1\" noUngrp=\"1\" noRot=\"1\" noChangeAspect=\"1\" noMove=\"1\" noResize=\"1\"/></p:cNvGrpSpPr><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"{x0}\" y=\"{y0}\"/><a:ext cx=\"{}\" cy=\"{}\"/><a:chOff x=\"{x0}\" y=\"{y0}\"/><a:chExt cx=\"{}\" cy=\"{}\"/></a:xfrm></p:grpSpPr></p:grpSp>",
                frame.width, frame.height, frame.width, frame.height
            );
            let first = summary[0].pic;
            let pics: Vec<NodeId> = summary.iter().map(|c| c.pic).collect();
            let alt = wrap(doc, first, &xml, Some(&group))?;
            // The other cards' pictures go into the group, in order.
            let fallback = doc.child(alt, "Fallback");
            let grp = fallback.and_then(|f| doc.child(f, "grpSp"));
            if let Some(grp) = grp {
                for pic in pics.into_iter().skip(1) {
                    doc.detach(pic);
                    let n = doc.children(grp).len();
                    doc.insert(grp, n, pic);
                }
            }
        }
    }
    Ok(())
}

/// `pic` replaced by the `mc:AlternateContent` of `xml`, with `pic` (inside `group`, when given) as its `mc:Fallback`.
fn wrap(doc: &mut XmlDocument, pic: NodeId, xml: &str, group: Option<&str>) -> Result<NodeId, Error> {
    let parent = doc.parent(pic).ok_or_else(|| Error::new("ズームの図が木に無い"))?;
    let x = Prefixes::at(doc, parent);
    let alt = x.import(doc, xml)?;
    let fallback = doc.child(alt, "Fallback").ok_or_else(|| Error::new("ズームの代替が作れない"))?;
    doc.insert_before(pic, alt);
    doc.detach(pic);
    match group {
        Some(g) => {
            let grp = x.import(doc, g)?;
            doc.insert(fallback, 0, grp);
            let n = doc.children(grp).len();
            doc.insert(grp, n, pic);
        }
        None => doc.insert(fallback, 0, pic),
    }
    Ok(alt)
}
