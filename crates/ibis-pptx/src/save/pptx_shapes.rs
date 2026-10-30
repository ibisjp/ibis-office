// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Writing the shapes of a slide (see `pptx.rs`): shapes added, removed,
//! moved in the z-order, moved, resized, rotated, flipped, filled and
//! outlined, and text typed inside runs. The presentation担当's file.
//!
//! Nodes of a part are found by their paths before anything of the part
//! changes (paths shift when elements are added or removed; node ids do not).

use std::collections::{HashMap, HashSet};

use ibis_doc_model::{EmuRect, Fill, MediaFormat, MediaId, Presentation, Shape, ShapeId, ShapeKind, Slide, Stroke, TextBody};
use ibis_ooxml::{NodeId, Package, XmlDocument};

use super::pptx::{ShapeSource, SourceMap, Target, next_part_name, relative};
use super::pptx_new::{self as new, Prefixes};
use super::unsupported;
use crate::Error;

pub(super) const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";

/// Media parts written in this save, by media id.
#[derive(Default)]
pub(crate) struct MediaParts {
    parts: HashMap<MediaId, String>,
}

/// The chart part the graphic frame at `path` of slide part `part` shows.
pub(crate) fn chart_part_at(package: &Package, part: &str, path: &[u32]) -> Result<String, Error> {
    let doc = package.xml(part)?;
    let node = doc.by_path(path).ok_or_else(|| unsupported("このグラフの変更"))?;
    let reference = doc.descendants(node).into_iter().find(|&n| doc.local_name(n) == "chart").ok_or_else(|| unsupported("このグラフの変更"))?;
    let rid = doc
        .attributes(reference)
        .iter()
        .find(|a| a.name.ends_with(":id") && !a.name.starts_with("xmlns"))
        .map(|a| a.value.clone())
        .ok_or_else(|| unsupported("このグラフの変更"))?;
    package.relationships(part)?.into_iter().find(|r| r.id == rid).and_then(|r| r.target_part(part)).ok_or_else(|| unsupported("このグラフの変更"))
}

/// Chart part `part` changed from `before` to `after`: with `data` (データ
/// の編集 in the workbook editor) its series and workbook from that first,
/// then the rest; else the changes, and the cells changed in its workbook.
pub(crate) fn write_chart_changes(package: &mut Package, part: &str, before: &ibis_doc_model::Chart, after: &ibis_doc_model::Chart, data: Option<&super::pptx_chart_data::ChartData>) -> Result<(), Error> {
    // A chartEx chart: its data (from the workbook), its title and its legend are written; nothing else is yet.
    if super::pptx_chart_ex::is_chart_ex(package, part) {
        if let Some(data) = data {
            super::pptx_chart_ex::write_chart_data(package, part, data)?;
        }
        // (With the data edited, the rest of the chart differs by what the workbook gave it.)
        return super::pptx_chart_ex::write_chart_format(package, part, before, after, data.is_some());
    }
    match data {
        Some(data) => {
            super::pptx_chart_data::write_chart_data(package, part, after, data)?;
            super::pptx_chart::write_chart(package, part, &super::pptx_chart_data::as_before(before, after), after)?;
        }
        None => {
            let cells = super::pptx_chart::write_chart(package, part, before, after)?;
            super::pptx_chart::write_embedded_cells(package, part, &cells)?;
        }
    }
    Ok(())
}

/// Write the shapes of `slide` into its target part.
pub(crate) fn write_slide(
    package: &mut Package,
    map: &SourceMap,
    current: &Presentation,
    target: &Target,
    slide: &Slide,
    media: &mut MediaParts,
    new_charts: &HashMap<ShapeId, super::NewChartEntry>,
    chart_data: &super::pptx_chart_data::ChartDataEdits,
) -> Result<HashMap<ShapeId, String>, Error> {
    let layout_changed = target.old_layout.is_some_and(|l| Some(l) != slide.layout);
    let old_ids: HashSet<ShapeId> = target.old.iter().map(|s| s.id).collect();
    let new_ids: HashSet<ShapeId> = slide.shapes.iter().map(|s| s.id).collect();
    // Relationships for new pictures.
    let mut picture_rels: HashMap<ShapeId, String> = HashMap::new();
    let mut clip_rels: HashMap<ShapeId, (String, String)> = HashMap::new();
    let mut slide_media_rels: HashMap<MediaId, String> = HashMap::new();
    for shape in slide.shapes.iter().filter(|s| {
        !old_ids.contains(&s.id)
            || match (&s.kind, target.old.iter().find(|old| old.id == s.id).map(|old| &old.kind)) {
                (ShapeKind::Picture(new), Some(ShapeKind::Picture(old))) => old.media != new.media,
                (ShapeKind::Picture(_), Some(_)) => true,
                _ => false,
            }
    }) {
        if let ShapeKind::Picture(picture) = &shape.kind {
            let rel = match slide_media_rels.get(&picture.media) {
                Some(rel) => rel.clone(),
                None => {
                    let part = media_part(package, current, picture.media, media)?;
                    let rel = package.add_relationship(&target.part, REL_IMAGE, &relative(&target.part, &part), false)?;
                    slide_media_rels.insert(picture.media, rel.clone());
                    rel
                }
            };
            picture_rels.insert(shape.id, rel);
            // A new video or audio clip: its file, related twice (the way
            // PowerPoint writes it: as a video or audio, and as 2010 media).
            if let Some(clip) = picture.clip.as_ref().filter(|_| !old_ids.contains(&shape.id)) {
                let kind = if clip.audio { REL_AUDIO } else { REL_VIDEO };
                if let Some(file) = &clip.link {
                    // ファイルへのリンク: related to the file outside, kept out of the package.
                    let link = package.add_relationship(&target.part, kind, file, true)?;
                    clip_rels.insert(shape.id, (link, String::new()));
                } else {
                    let part = media_part(package, current, clip.media, media)?;
                    let to = relative(&target.part, &part);
                    let link = package.add_relationship(&target.part, kind, &to, false)?;
                    let embed = package.add_relationship(&target.part, REL_MEDIA_2010, &to, false)?;
                    clip_rels.insert(shape.id, (link, embed));
                }
            }
        }
    }
    // Relationships for shape and table cell image fills.
    for shape in &slide.shapes {
        match &shape.kind {
            ShapeKind::Auto(a) => match a.fill.media() {
                Some(m) => {
                    if !slide_media_rels.contains_key(&m) {
                        let part = media_part(package, current, m, media)?;
                        let rel = package.add_relationship(&target.part, REL_IMAGE, &relative(&target.part, &part), false)?;
                        slide_media_rels.insert(m, rel);
                    }
                }
                _ => {}
            },
            ShapeKind::Table(t) => {
                for cell in t.rows.iter().flat_map(|r| &r.cells) {
                    match cell.fill.media() {
                        Some(m) => {
                            if !slide_media_rels.contains_key(&m) {
                                let part = media_part(package, current, m, media)?;
                                let rel = package.add_relationship(&target.part, REL_IMAGE, &relative(&target.part, &part), false)?;
                                slide_media_rels.insert(m, rel);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    let mut svg_media_ids: HashSet<MediaId> = HashSet::new();
    for shape in &slide.shapes {
        match &shape.kind {
            ShapeKind::Picture(p) => {
                if current.media(p.media).is_some_and(|m| m.format == MediaFormat::Svg) {
                    svg_media_ids.insert(p.media);
                }
            }
            ShapeKind::Auto(a) => {
                if let Some(m) = a.fill.media() {
                    if current.media(m).is_some_and(|m| m.format == MediaFormat::Svg) {
                        svg_media_ids.insert(m);
                    }
                }
            }
            ShapeKind::Table(t) => {
                for cell in t.rows.iter().flat_map(|r| &r.cells) {
                    if let Some(m) = cell.fill.media() {
                        if current.media(m).is_some_and(|m| m.format == MediaFormat::Svg) {
                            svg_media_ids.insert(m);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    super::pptx_new::set_media_rels(slide_media_rels.clone());
    super::pptx_new::set_svg_media(svg_media_ids);
    super::pptx_new::set_clip_rels(clip_rels);
    super::pptx_new::set_aspect_locked(slide.aspect_locked.clone());

    // New charts (挿入 > グラフ): their parts from the template, then the
    // changes made to them since.
    let mut chart_rels: HashMap<ShapeId, String> = HashMap::new();
    for shape in slide.shapes.iter().filter(|s| !old_ids.contains(&s.id)) {
        let ShapeKind::Chart(after) = &shape.kind else { continue };
        let entry = new_charts.get(&shape.id).ok_or_else(|| unsupported("グラフの複製・貼り付け"))?;
        let (rel, part) = super::pptx_new_chart::add_chart_part(package, &target.part, entry.kind)?;
        if let Some(template) = &entry.template {
            write_chart_changes(package, &part, template, after, super::pptx_chart_data::matching(chart_data, shape.id, after))?;
        }
        chart_rels.insert(shape.id, rel);
    }
    // Charts whose data changed: written into their chart parts.
    let mut charts_written: HashSet<ShapeId> = HashSet::new();
    for shape in &slide.shapes {
        let (Some(old), ShapeKind::Chart(after)) = (target.old.iter().find(|o| o.id == shape.id), &shape.kind) else { continue };
        let ShapeKind::Chart(before) = &old.kind else { continue };
        let data = super::pptx_chart_data::matching(chart_data, shape.id, after);
        if before == after && data.is_none() {
            continue;
        }
        let source = map.shapes.get(&shape.id).filter(|s| Some(&s.part) == target.lookup.as_ref()).ok_or_else(|| unsupported("このグラフの変更"))?;
        let chart_part = chart_part_at(package, &target.part, &source.path)?;
        write_chart_changes(package, &chart_part, before, after, data)?;
        charts_written.insert(shape.id);
    }

    let part = target.part.clone();
    let doc = package.xml_mut(&part)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{part} が空です")))?;
    let tree = doc.path(root, &["cSld", "spTree"]).ok_or_else(|| Error::new(format!("{part} に図形の一覧がありません")))?;
    // Find every shape read from the part while the paths are valid.
    let mut nodes: HashMap<ShapeId, NodeId> = HashMap::new();
    let mut sources: HashMap<ShapeId, &ShapeSource> = HashMap::new();
    let mut created_ids: HashMap<ShapeId, String> = HashMap::new();
    for shape in target.old {
        let Some(source) = map.shapes.get(&shape.id).filter(|s| Some(&s.part) == target.lookup.as_ref()) else { continue };
        let node = doc
            .by_path(&source.path)
            .filter(|&n| matches!(doc.local_name(n), "sp" | "cxnSp" | "pic" | "graphicFrame"))
            .ok_or_else(|| Error::new(format!("{part} の図形の位置が見つかりません")))?;
        nodes.insert(shape.id, node);
        sources.insert(shape.id, source);
    }
    let mut all_native_ids: HashMap<u64, String> = HashMap::new();
    for (&id, &node) in &nodes {
        if let Some(c_nv_pr) = doc.descendants(node).into_iter().find(|&n| doc.is_element(n) && doc.local_name(n) == "cNvPr") {
            if let Some(native) = doc.attr(c_nv_pr, "id") {
                all_native_ids.insert(id.0, native.to_string());
            }
        }
    }
    // Changes inside shapes first: text is found by paths too.
    let old_by_id: HashMap<ShapeId, &Shape> = target.old.iter().map(|s| (s.id, s)).collect();
    for shape in &slide.shapes {
        let Some(old) = old_by_id.get(&shape.id) else { continue };
        if *old == shape || attached_frame_only(map, old, shape, &old_by_id, slide) {
            continue;
        }
        if let Some(attached) = map.attached.get(&shape.id) {
            check_attached(old, shape, attached, &old_by_id, slide)?;
        }
        let (Some(&node), Some(source)) = (nodes.get(&shape.id), sources.get(&shape.id)) else {
            return Err(unsupported("グラフの上の図形・インクなどの変更"));
        };
        // A placeholder that takes its place from the layout: a new place
        // is the layout's (nothing to write); one that keeps its place now
        // gets it written.
        let placeholder = slide.placeholders.get(&shape.id);
        let mut old = (*old).clone();
        if charts_written.contains(&shape.id) {
            old.kind = shape.kind.clone();
        }
        if placeholder.is_some_and(|p| !p.own_frame) {
            old.frame = shape.frame;
            old.rotation = shape.rotation;
            old.flip_h = shape.flip_h;
            old.flip_v = shape.flip_v;
        }
        // A new layout (and its master and theme) restyles what inherits:
        // text of the same runs in other formats, other theme colours, a
        // table's look. That is the layout's, not written. (A format given
        // to the slide's shapes in the same session is not told apart yet.)
        if layout_changed && std::mem::discriminant(&old.kind) == std::mem::discriminant(&shape.kind) {
            let same_text = match (old.text_body(), shape.text_body()) {
                (Some(a), Some(b)) => same_runs(a, b),
                (None, None) => true,
                _ => false,
            };
            if same_text {
                old.kind = shape.kind.clone();
            }
        }
        if old != *shape {
            let kind_changed = std::mem::discriminant(&old.kind) != std::mem::discriminant(&shape.kind);
            if kind_changed && matches!((&old.kind, &shape.kind), (ShapeKind::Auto(_), ShapeKind::Picture(_)) | (ShapeKind::Picture(_), ShapeKind::Auto(_))) {
                let drawing_id = doc.descendants(node).into_iter()
                    .find(|&n| doc.is_element(n) && doc.local_name(n) == "cNvPr")
                    .and_then(|n| doc.attr(n, "id").and_then(|v| v.parse::<u32>().ok()))
                    .unwrap_or(shape.id.0 as u32);
                let x = Prefixes::at(doc, tree);
                let xml = new::shape_xml(&x, shape, drawing_id, picture_rels.get(&shape.id).map(String::as_str))?;
                let new_node = x.import(doc, &xml)?;
                doc.insert_before(node, new_node);
                doc.detach(node);
                nodes.insert(shape.id, new_node);
                created_ids.insert(shape.id, drawing_id.to_string());
            } else {
                write_shape(doc, node, source, &old, shape, picture_rels.get(&shape.id).map(String::as_str))?;
            }
        }
    }
    // Placeholders that now keep their own place (their new layout has none).
    for shape in &slide.shapes {
        let (Some(&node), Some(source)) = (nodes.get(&shape.id), sources.get(&shape.id)) else { continue };
        let Some(ph) = slide.placeholders.get(&shape.id) else {
            // A layout can turn an entered placeholder into an ordinary shape.
            // Remove its XML placeholder too, and preserve its inherited frame.
            let placeholder = ["nvSpPr", "nvPicPr", "nvGraphicFramePr", "nvCxnSpPr"].into_iter()
                .find_map(|nv|doc.path(node,&[nv,"nvPr","ph"]));
            if let Some(ph) = placeholder {
                doc.detach(ph);
                pin_frame(doc,node,&untwisted(shape,&source.turns),source.group)?;
            }
            continue;
        };
        if ph.own_frame {
            pin_frame(doc, node, &untwisted(shape, &source.turns), source.group)?;
        } else if let Some(props) = doc.child(node, "spPr") {
            // Reset/layout inheritance: remove only the explicit placement.
            // Do this after writing text, whose original paths must stay valid.
            if let Some(xfrm) = doc.child(props, "xfrm") { doc.detach(xfrm); }
        }
    }
    // Removed shapes, with their animations.
    for old in target.old {
        if new_ids.contains(&old.id) {
            continue;
        }
        let Some(&node) = nodes.get(&old.id) else {
            // A shape drawn over a chart goes with the chart.
            let owner = map.attached.iter().find(|(_, ids)| ids.contains(&old.id)).map(|(chart, _)| *chart);
            if owner.is_some_and(|chart| !new_ids.contains(&chart)) {
                continue;
            }
            return Err(unsupported("グラフの上の図形・インクなどの削除"));
        };
        let spid = drawing_id(doc, node);
        remove_shape_element(doc, node, tree);
        if let Some(spid) = spid {
            remove_animations(doc, root, &spid);
        }
    }
    // New shapes.
    let mut next_id = doc
        .descendants(root)
        .into_iter()
        .filter(|&n| doc.is_element(n) && doc.local_name(n) == "cNvPr")
        .filter_map(|n| doc.attr(n, "id").and_then(|v| v.parse::<u32>().ok()))
        .max()
        .unwrap_or(1)
        + 1;
    let x = Prefixes::at(doc, tree);
    let mut created: HashMap<ShapeId, NodeId> = HashMap::new();
    for shape in slide.shapes.iter().filter(|s| !old_ids.contains(&s.id)) {
        let xml = match chart_rels.get(&shape.id) {
            Some(rel) if new_charts.get(&shape.id).is_some_and(|e| e.kind.is_chart_ex()) => super::pptx_new_chart::frame_ex_xml(&x.p, &x.a, &x.r, next_id, &crate::chart_name(next_id - 1), shape.frame, rel),
            Some(rel) => super::pptx_new_chart::frame_xml(&x.p, &x.a, &x.r, next_id, &crate::chart_name(next_id - 1), shape.frame, rel),
            // A new placeholder (header, footer, slide number…): as one.
            None => match slide.placeholders.get(&shape.id) {
                Some(ph) if shape.text_body().is_some() => new::placeholder_shape_xml(&x, shape, ph, next_id)?,
                _ => new::shape_xml_with_glue(&x, shape, next_id, picture_rels.get(&shape.id).map(String::as_str), slide.shape_glues.get(&shape.id), Some(&all_native_ids))?,
            },
        };
        created_ids.insert(shape.id, next_id.to_string());
        all_native_ids.insert(shape.id.0, next_id.to_string());
        next_id += 1;
        // A chartEx frame has its fallback's id too.
        if new_charts.get(&shape.id).is_some_and(|e| e.kind.is_chart_ex()) && chart_rels.contains_key(&shape.id) {
            next_id += 1;
        }
        created.insert(shape.id, x.import(doc, &xml)?);
    }
    for shape in &slide.shapes {
        if let Some(&node) = nodes.get(&shape.id) {
            if doc.local_name(node) == "cxnSp" {
                write_connector_glue(doc, node, slide.shape_glues.get(&shape.id), &all_native_ids)?;
            }
        }
    }
    let old_groups=slide.live_groups(target.old_groups);
    if old_groups!=slide.groups {
        let leaves=nodes.iter().filter(|(id,_)|new_ids.contains(id)).map(|(&id,&node)|(id,node)).chain(created.iter().map(|(&id,&node)|(id,node))).collect();
        super::pptx_groups::write(doc,tree,slide,&old_groups,&leaves)?;
    }
    arrange(doc, tree, slide, &nodes, &created)?;
    Ok(created_ids)
}

/// Whether `old` is drawn over a chart and only its frame changed (it
/// follows the chart, see `check_attached`).
fn attached_frame_only(map: &SourceMap, old: &Shape, new: &Shape, old_by_id: &HashMap<ShapeId, &Shape>, slide: &Slide) -> bool {
    let Some(chart) = map.attached.iter().find(|(_, ids)| ids.contains(&old.id)).map(|(chart, _)| *chart) else { return false };
    // The chart moved too (and `check_attached` checks they moved together).
    let chart_moved = old_by_id.get(&chart).zip(slide.shape(chart)).is_some_and(|(a, b)| a.frame != b.frame);
    chart_moved && Shape { frame: new.frame, ..old.clone() } == *new
}

/// A chart moved or resized must take the shapes drawn over it along
/// (they are stored relative to it).
fn check_attached(old: &Shape, new: &Shape, attached: &[ShapeId], old_by_id: &HashMap<ShapeId, &Shape>, slide: &Slide) -> Result<(), Error> {
    if old.frame == new.frame {
        return Ok(());
    }
    let refuse = || unsupported("グラフの上の図形とずれるグラフの移動・大きさの変更");
    if old.rotation != new.rotation || old.flip_h != new.flip_h || old.flip_v != new.flip_v {
        return Err(refuse());
    }
    let (o, n) = (old.frame, new.frame);
    let map_x = |x: i64| n.x as f64 + (x - o.x) as f64 * if o.width == 0 { 1.0 } else { n.width as f64 / o.width as f64 };
    let map_y = |y: i64| n.y as f64 + (y - o.y) as f64 * if o.height == 0 { 1.0 } else { n.height as f64 / o.height as f64 };
    for id in attached {
        let (Some(before), Some(after)) = (old_by_id.get(id), slide.shape(*id)) else { continue };
        let b = before.frame;
        let expected = [map_x(b.x), map_y(b.y), map_x(b.right()), map_y(b.bottom())];
        let a = after.frame;
        let actual = [a.x as f64, a.y as f64, a.right() as f64, a.bottom() as f64];
        if expected.iter().zip(actual).any(|(e, v)| (e - v).abs() > 2.0) {
            return Err(refuse());
        }
    }
    Ok(())
}

/// Put the top-level elements of the slide's shapes in the model's order,
/// inserting the new ones.
fn arrange(doc: &mut XmlDocument, tree: NodeId, slide: &Slide, nodes: &HashMap<ShapeId, NodeId>, created: &HashMap<ShapeId, NodeId>) -> Result<(), Error> {
    let top = |doc: &XmlDocument, mut n: NodeId| {
        while let Some(p) = doc.parent(n) {
            if p == tree {
                break;
            }
            n = p;
        }
        n
    };
    let mut desired: Vec<NodeId> = Vec::new();
    for shape in &slide.shapes {
        let node = match (created.get(&shape.id), nodes.get(&shape.id)) {
            (Some(&n), _) => top(doc, n),
            (None, Some(&n)) => top(doc, n),
            // Shapes the reader made (over charts, ink) stay where they are.
            (None, None) => continue,
        };
        if desired.last() != Some(&node) {
            desired.push(node);
        }
    }
    if desired.iter().collect::<HashSet<_>>().len() != desired.len() {
        return Err(unsupported("グループの中の図形の重なり順の変更"));
    }
    let wanted: HashSet<NodeId> = desired.iter().copied().collect();
    let attached: Vec<NodeId> = doc.elements(tree).filter(|n| wanted.contains(n)).collect();
    let attached_set: HashSet<NodeId> = attached.iter().copied().collect();
    let in_desired: Vec<NodeId> = desired.iter().copied().filter(|n| attached_set.contains(n)).collect();
    if attached == in_desired {
        // Only insertions: each new element after the one before it.
        for (i, &node) in desired.iter().enumerate() {
            if attached_set.contains(&node) {
                continue;
            }
            if i > 0 {
                doc.insert_after(desired[i - 1], node);
            } else if let Some(&next) = desired.iter().find(|n| attached_set.contains(n)) {
                doc.insert_before(next, node);
            } else {
                append_shape(doc, tree, node);
            }
        }
    } else {
        // Reordered: all of them after what came before the first.
        let first = attached[0];
        let children: Vec<NodeId> = doc.elements(tree).collect();
        let anchor = children.iter().position(|&c| c == first).and_then(|i| i.checked_sub(1)).map(|i| children[i]);
        for &n in &attached {
            doc.detach(n);
        }
        let mut previous = anchor;
        for &node in &desired {
            match previous {
                Some(p) => doc.insert_after(p, node),
                None => doc.insert(tree, 0, node),
            }
            previous = Some(node);
        }
    }
    Ok(())
}

/// Insert `node` as the last shape of the tree (before its `p:extLst`).
fn append_shape(doc: &mut XmlDocument, tree: NodeId, node: NodeId) {
    match doc.child(tree, "extLst") {
        Some(ext) => doc.insert_before(ext, node),
        None => {
            let index = doc.children(tree).len();
            doc.insert(tree, index, node);
        }
    }
}

/// Take a shape's element out, and the groups it leaves empty.
fn remove_shape_element(doc: &mut XmlDocument, node: NodeId, tree: NodeId) {
    const SHAPES: [&str; 7] = ["sp", "cxnSp", "pic", "graphicFrame", "grpSp", "contentPart", "AlternateContent"];
    let mut at = node;
    loop {
        let parent = doc.parent(at);
        doc.detach(at);
        match parent {
            Some(p) if p != tree && doc.local_name(p) == "grpSp" && !doc.elements(p).any(|c| SHAPES.contains(&doc.local_name(c))) => at = p,
            _ => break,
        }
    }
}

/// The drawing-object id (`p:cNvPr/@id`) of a shape's element.
fn drawing_id(doc: &XmlDocument, node: NodeId) -> Option<String> {
    let nv = doc.elements(node).find(|&n| doc.local_name(n).starts_with("nv"))?;
    let c = doc.child(nv, "cNvPr")?;
    doc.attr(c, "id").map(str::to_string)
}

fn attached_under(doc: &XmlDocument, mut n: NodeId, ancestor: NodeId) -> bool {
    while let Some(p) = doc.parent(n) {
        if p == ancestor {
            return true;
        }
        n = p;
    }
    false
}

/// Remove the animations of the shape with drawing-object id `spid`: its
/// effects in `p:timing` and its entries in `p:bldLst`. When what is left
/// cannot be told apart (a trigger on the shape), all the slide's
/// animations are removed.
fn remove_animations(doc: &mut XmlDocument, root: NodeId, spid: &str) {
    let Some(timing) = doc.child(root, "timing") else { return };
    let targets: Vec<NodeId> =
        doc.descendants(timing).into_iter().filter(|&n| doc.is_element(n) && doc.local_name(n) == "spTgt" && doc.attr(n, "spid") == Some(spid)).collect();
    for t in targets {
        if !attached_under(doc, t, timing) {
            continue;
        }
        // The effect: the nearest `p:par` whose `p:cTn` has a preset class.
        let mut at = doc.parent(t);
        let mut effect = None;
        while let Some(n) = at {
            if n == timing {
                break;
            }
            if doc.local_name(n) == "par" && doc.child(n, "cTn").is_some_and(|c| doc.attr(c, "presetClass").is_some()) {
                effect = Some(n);
                break;
            }
            at = doc.parent(n);
        }
        let Some(effect) = effect else {
            doc.detach(timing);
            return;
        };
        // Groups the effect leaves empty go too, up to the sequence.
        let mut remove = effect;
        while let Some(list) = doc.parent(remove) {
            doc.detach(remove);
            if doc.elements(list).next().is_some() || doc.local_name(list) != "childTnLst" {
                break;
            }
            let Some(ctn) = doc.parent(list) else { break };
            let Some(par) = doc.parent(ctn) else { break };
            if doc.local_name(par) != "par" || doc.attr(ctn, "nodeType").is_some_and(|t| matches!(t, "mainSeq" | "tmRoot" | "interactiveSeq")) {
                break;
            }
            remove = par;
        }
    }
    if let Some(builds) = doc.child(timing, "bldLst") {
        for b in doc.elements(builds).collect::<Vec<_>>() {
            if doc.attr(b, "spid") == Some(spid) {
                doc.detach(b);
            }
        }
        if doc.elements(builds).next().is_none() {
            doc.detach(builds);
        }
    }
    // With no animation left, no timing.
    if !doc.descendants(timing).into_iter().any(|n| doc.is_element(n) && doc.local_name(n) == "spTgt") {
        doc.detach(timing);
    }
}

/// Write what a connector is attached to (`a:stCxn`, `a:endCxn`).
fn write_connector_glue(
    doc: &mut XmlDocument,
    node: NodeId,
    glue: Option<&ibis_doc_model::Glue>,
    id_map: &HashMap<u64, String>,
) -> Result<(), Error> {
    let nv_cxn_pr = doc.elements(node).find(|&n| doc.local_name(n) == "nvCxnSpPr");
    let Some(nv) = nv_cxn_pr else { return Ok(()); };
    let c_nv_cxn_pr = doc.elements(nv).find(|&n| doc.local_name(n) == "cNvCxnSpPr");
    let c_nv_cxn = match c_nv_cxn_pr {
        Some(c) => c,
        None => return Ok(()),
    };
    let to_remove: Vec<NodeId> = doc.elements(c_nv_cxn).filter(|&n| matches!(doc.local_name(n), "stCxn" | "endCxn")).collect();
    for child in to_remove {
        doc.detach(child);
    }
    if let Some(g) = glue {
        let a = Prefixes::at(doc, c_nv_cxn).a;
        if let Some((target_id, idx)) = g.start {
            let fallback = target_id.to_string();
            let native_id = id_map.get(&target_id).unwrap_or(&fallback);
            let el = doc.append_element(c_nv_cxn, &format!("{a}stCxn"));
            doc.set_attr(el, "id", native_id);
            doc.set_attr(el, "idx", &idx.to_string());
        }
        if let Some((target_id, idx)) = g.end {
            let fallback = target_id.to_string();
            let native_id = id_map.get(&target_id).unwrap_or(&fallback);
            let el = doc.append_element(c_nv_cxn, &format!("{a}endCxn"));
            doc.set_attr(el, "id", native_id);
            doc.set_attr(el, "idx", &idx.to_string());
        }
    }
    Ok(())
}

/// Write the changes from `old` to `new` of one shape.
fn write_shape(doc: &mut XmlDocument, node: NodeId, source: &ShapeSource, old: &Shape, new: &Shape, picture_rel: Option<&str>) -> Result<(), Error> {
    // What may change: the frame, rotation and flips, text inside runs, fill and outline.
    let mut rest = old.clone();
    rest.frame = new.frame;
    rest.rotation = new.rotation;
    rest.flip_h = new.flip_h;
    rest.flip_v = new.flip_v;
    if let (Some(body), Some(new_body)) = (rest.text_body_mut(), new.text_body()) {
        *body = new_body.clone();
    }
    match (&mut rest.kind, &new.kind) {
        (ShapeKind::Auto(a), ShapeKind::Auto(b)) => {
            a.fill = b.fill.clone();
            a.stroke = b.stroke.clone();
            a.glow = b.glow;
            a.shadow = b.shadow;
            a.reflection = b.reflection;
            a.soft_edge = b.soft_edge;
            a.bevel = b.bevel;
            a.tilt = b.tilt;
            // 図形の変更: one preset for another.
            // 頂点の編集 and the like: a freeform for a preset, or one for another, are written whole.
            if a.geometry != b.geometry {
                a.geometry = b.geometry.clone();
            }
        }
        (ShapeKind::Picture(a), ShapeKind::Picture(b)) => {
            a.stroke = b.stroke.clone(); a.crop = b.crop; a.media = b.media;
            a.corner = b.corner;
            a.duotone = b.duotone;
            a.clear = b.clear;
            a.luminance = b.luminance;
            a.shadow = b.shadow.clone();
            a.reflection = b.reflection.clone();
            a.soft_edge = b.soft_edge;
            a.clip = b.clip.clone();
            a.artistic_effect = b.artistic_effect;
        },
        (ShapeKind::Line(a), ShapeKind::Line(b)) => *a = b.clone(),
        // A table's cells' text (セルの文字の編集).
        (ShapeKind::Table(a), ShapeKind::Table(b)) if super::pptx_table::writable(a, b) => **a = (**b).clone(),
        _ => {}
    }
    if rest != *new {
        return Err(unsupported("図形の種類・効果・文字の書式などの変更"));
    }
    if let (ShapeKind::Auto(a), ShapeKind::Auto(b)) = (&old.kind, &new.kind)
        && a.geometry != b.geometry
    {
        write_geometry(doc, node, &b.geometry)?;
    }
    if let (Some(old_body), Some(new_body)) = (old.text_body(), new.text_body())
        && old_body != new_body
    {
        if runs_only_changed(source, old_body, new_body) {
            write_text(doc, source, old_body, new_body)?;
        } else {
            super::pptx_text::rewrite_text(doc, node, source, old_body, new_body)?;
        }
    }
    if (old.frame, old.rotation, old.flip_h, old.flip_v) != (new.frame, new.rotation, new.flip_h, new.flip_v) {
        write_xfrm(doc, node, source, old, new)?;
    }
    match (&old.kind, &new.kind) {
        (ShapeKind::Auto(a), ShapeKind::Auto(b)) => {
            if a.fill != b.fill {
                write_fill(doc, node, &a.fill, &b.fill)?;
            }
            if a.stroke != b.stroke {
                write_stroke(doc, node, a.stroke.as_ref(), b.stroke.as_ref())?;
            }
            if a.glow != b.glow { write_shape_glow(doc, node, a, b)?; }
            if a.shadow != b.shadow { write_shape_shadow(doc, node, a, b)?; }
            if a.reflection != b.reflection { write_shape_reflection(doc, node, a, b)?; }
            if a.soft_edge != b.soft_edge { write_shape_soft_edge(doc, node, a, b)?; }
            if a.bevel != b.bevel { write_shape_bevel(doc, node, a, b)?; }
            if a.tilt != b.tilt { write_shape_tilt(doc, node, a, b)?; }
        }
        (ShapeKind::Picture(a), ShapeKind::Picture(b)) => {
            if a.media != b.media {
                let rel = picture_rel.ok_or_else(|| Error::new("置換画像の関係がありません"))?;
                let blip = doc.path(node, &["blipFill", "blip"]).ok_or_else(|| unsupported("この画像の置換"))?;
                let x = Prefixes::at(doc, blip);
                doc.set_attr(blip, &format!("{}embed", x.r), rel);
                let is_svg = super::pptx_new::is_svg_media(b.media);
                let svg_blip = doc.descendants(blip).into_iter().find(|&n| doc.is_element(n) && doc.local_name(n) == "svgBlip");
                if is_svg {
                    if let Some(svg) = svg_blip {
                        doc.set_attr(svg, &format!("{}embed", x.r), rel);
                    } else {
                        let ext_lst = match doc.child(blip, "extLst") {
                            Some(e) => e,
                            None => {
                                let e = x.import(doc, &format!("<{}extLst/>", x.a))?;
                                let at = doc.children(blip).len();
                                doc.insert(blip, at, e);
                                e
                            }
                        };
                        let ext_xml = format!(
                            r#"<{}ext uri="{{96DAC542-7B81-4868-9F9E-1EA1F31DC1F3}}"><asvg:svgBlip xmlns:asvg="http://schemas.microsoft.com/office/drawing/2016/SVG/main" {}embed="{rel}"/></{}ext>"#,
                            x.a, x.r, x.a
                        );
                        let ext = x.import(doc, &ext_xml)?;
                        let at = doc.children(ext_lst).len();
                        doc.insert(ext_lst, at, ext);
                    }
                } else if let Some(svg) = svg_blip {
                    if let Some(parent_ext) = doc.parent(svg) {
                        doc.detach(parent_ext);
                    }
                }
            }
            if a.stroke != b.stroke { write_stroke(doc, node, a.stroke.as_ref(), b.stroke.as_ref())?; }
            if a.crop != b.crop { write_picture_crop(doc, node, b.crop)?; }
            if a.corner != b.corner { write_picture_corner(doc, node, b.corner)?; }
            if a.duotone != b.duotone { write_picture_duotone(doc, node, b.duotone)?; }
            if a.clear != b.clear { write_picture_clear(doc, node, b.clear)?; }
            if a.luminance != b.luminance { write_picture_luminance(doc, node, b.luminance)?; }
            if a.shadow != b.shadow { write_picture_shadow(doc, node, a, b)?; }
            if a.reflection != b.reflection { write_picture_reflection(doc, node, a, b)?; }
            if a.soft_edge != b.soft_edge { write_picture_soft_edge(doc, node, a, b)?; }
            if a.clip != b.clip { write_picture_clip(doc, node, b.clip.clone())?; }
            if a.artistic_effect != b.artistic_effect { write_picture_artistic_effect(doc, node, b.artistic_effect)?; }
        }
        (ShapeKind::Line(a), ShapeKind::Line(b)) if a != b => write_stroke(doc, node, Some(a), Some(b))?,
        (ShapeKind::Table(a), ShapeKind::Table(b)) if a != b => super::pptx_table::write_table(doc, node, a, b)?,
        _ => {}
    }
    Ok(())
}

/// Get a direct effect list; reject implicit inheritance before overriding it.
fn direct_effect_list(doc: &mut XmlDocument, node: NodeId, old: &ibis_doc_model::AutoShape) -> Result<NodeId, Error> {
    let pr = doc.elements(node).find(|&n| doc.namespace(n) == Some(new::NS_P) && doc.local_name(n) == "spPr").ok_or_else(|| unsupported("この図形の効果"))?;
    let list = doc.elements(pr).find(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "effectLst");
    let list = match list {
        Some(list) => list,
        None => {
            let inherited = old.shadow.is_some() || old.glow.is_some() || old.reflection.is_some() || old.soft_edge.is_some() || doc.path(node,&["nvSpPr","nvPr","ph"]).is_some()
                || doc.path(node,&["style","effectRef"]).is_some_and(|r| doc.attr(r,"idx") != Some("0"));
            if inherited || doc.elements(pr).any(|n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "effectDag") {
                return Err(unsupported("継承された効果/effectDagの上書き"));
            }
            let x = Prefixes::at(doc,pr);
            let index = doc.children(pr).iter().position(|&n| doc.is_element(n) && doc.namespace(n) == Some(new::NS_A) && matches!(doc.local_name(n),"scene3d"|"sp3d"|"extLst")).unwrap_or(doc.children(pr).len());
            doc.insert_element(pr,index,&format!("{}effectLst",x.a))
        }
    };
    Ok(list)
}

/// Edit the modelled glow without replacing unrelated effects or extensions.
fn write_shape_glow(doc: &mut XmlDocument, node: NodeId, old: &ibis_doc_model::AutoShape, new_shape: &ibis_doc_model::AutoShape) -> Result<(), Error> {
    let list = direct_effect_list(doc,node,old)?;
    let glows: Vec<_> = doc.elements(list).filter(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "glow").collect();
    let Some((color,radius)) = new_shape.glow else {
        for n in glows { doc.detach(n); }
        return Ok(());
    };
    if !(0..=i32::MAX as i64).contains(&radius) { return Err(unsupported("この光彩半径の範囲")); }
    let x = Prefixes::at(doc,list);
    let glow = match glows.first() {
        Some(&n) => n,
        None => {
            let index = doc.children(list).iter().position(|&n| doc.is_element(n) && doc.namespace(n) == Some(new::NS_A) && matches!(doc.local_name(n),"innerShdw"|"outerShdw"|"prstShdw"|"reflection"|"softEdge")).unwrap_or(doc.children(list).len());
            doc.insert_element(list,index,&format!("{}glow",x.a))
        }
    };
    doc.set_attr(glow,"rad",&radius.to_string());
    write_effect_color(doc,glow,old.glow.map(|g|g.0),color)?;
    Ok(())
}

fn write_effect_color(doc:&mut XmlDocument,node:NodeId,old:Option<ibis_doc_model::Color>,color:ibis_doc_model::Color)->Result<(),Error> {
    if old != Some(color) {
        let colors:Vec<_>=doc.elements(node).filter(|&n|doc.namespace(n)==Some(new::NS_A)&&matches!(doc.local_name(n),"srgbClr"|"scrgbClr"|"hslClr"|"sysClr"|"schemeClr"|"prstClr")).collect();
        for n in colors {doc.detach(n);}
        let x=Prefixes::at(doc,node);let element=x.import(doc,&new::color_xml(&x,color))?;doc.insert(node,0,element);
    }
    Ok(())
}

fn apply_shadow(doc: &mut XmlDocument, list: NodeId, old_color: Option<ibis_doc_model::Color>, shadow: Option<&ibis_doc_model::Shadow>) -> Result<(), Error> {
    let shadows: Vec<_> = doc.elements(list).filter(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "outerShdw").collect();
    let Some(shadow) = shadow else { for n in shadows { doc.detach(n); } return Ok(()); };
    if !(0..=i32::MAX as i64).contains(&shadow.blur) || !(0..=i32::MAX as i64).contains(&shadow.distance) || !(0..21_600_000).contains(&shadow.direction) { return Err(unsupported("この影の値")); }
    let x = Prefixes::at(doc, list);
    let node = match shadows.first() {
        Some(&n) => n,
        None => {
            let index = doc.children(list).iter().position(|&n| doc.is_element(n) && doc.namespace(n) == Some(new::NS_A) && matches!(doc.local_name(n), "prstShdw" | "reflection" | "softEdge")).unwrap_or(doc.children(list).len());
            doc.insert_element(list, index, &format!("{}outerShdw", x.a))
        }
    };
    for (name, value) in [("blurRad", shadow.blur), ("dist", shadow.distance), ("dir", shadow.direction as i64)] { doc.set_attr(node, name, &value.to_string()); }
    if shadow.sx != 100_000 { doc.set_attr(node, "sx", &shadow.sx.to_string()); } else { doc.remove_attr(node, "sx"); }
    if shadow.sy != 100_000 { doc.set_attr(node, "sy", &shadow.sy.to_string()); } else { doc.remove_attr(node, "sy"); }
    if shadow.kx != 0 { doc.set_attr(node, "kx", &shadow.kx.to_string()); } else { doc.remove_attr(node, "kx"); }
    if shadow.ky != 0 { doc.set_attr(node, "ky", &shadow.ky.to_string()); } else { doc.remove_attr(node, "ky"); }
    if shadow.align != ibis_doc_model::TileAlign::Bottom {
        let algn = match shadow.align {
            ibis_doc_model::TileAlign::TopLeft => "tl",
            ibis_doc_model::TileAlign::Top => "t",
            ibis_doc_model::TileAlign::TopRight => "tr",
            ibis_doc_model::TileAlign::Left => "l",
            ibis_doc_model::TileAlign::Center => "ctr",
            ibis_doc_model::TileAlign::Right => "r",
            ibis_doc_model::TileAlign::BottomLeft => "bl",
            ibis_doc_model::TileAlign::Bottom => "b",
            ibis_doc_model::TileAlign::BottomRight => "br",
        };
        doc.set_attr(node, "algn", algn);
    } else {
        doc.remove_attr(node, "algn");
    }
    if !shadow.rot_with_shape {
        doc.set_attr(node, "rotWithShape", "0");
    } else {
        doc.remove_attr(node, "rotWithShape");
    }
    write_effect_color(doc, node, old_color, shadow.color)
}

fn apply_reflection(doc: &mut XmlDocument, list: NodeId, reflection: Option<&ibis_doc_model::Reflection>) -> Result<(), Error> {
    let reflections: Vec<_> = doc.elements(list).filter(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "reflection").collect();
    let Some(r) = reflection else {
        for n in reflections { doc.detach(n); }
        return Ok(());
    };
    let x = Prefixes::at(doc, list);
    let node = match reflections.first() {
        Some(&n) => n,
        None => {
            let index = doc.children(list).iter().position(|&n| doc.is_element(n) && doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "softEdge").unwrap_or(doc.children(list).len());
            doc.insert_element(list, index, &format!("{}reflection", x.a))
        }
    };
    let algn = match r.align {
        ibis_doc_model::TileAlign::TopLeft => "tl",
        ibis_doc_model::TileAlign::Top => "t",
        ibis_doc_model::TileAlign::TopRight => "tr",
        ibis_doc_model::TileAlign::Left => "l",
        ibis_doc_model::TileAlign::Center => "ctr",
        ibis_doc_model::TileAlign::Right => "r",
        ibis_doc_model::TileAlign::BottomLeft => "bl",
        ibis_doc_model::TileAlign::Bottom => "b",
        ibis_doc_model::TileAlign::BottomRight => "br",
    };
    doc.set_attr(node, "blurRad", &r.blur.to_string());
    doc.set_attr(node, "stA", &r.start_alpha.to_string());
    doc.set_attr(node, "stPos", &r.start_pos.to_string());
    doc.set_attr(node, "endA", &r.end_alpha.to_string());
    doc.set_attr(node, "endPos", &r.end_pos.to_string());
    doc.set_attr(node, "dist", &r.distance.to_string());
    doc.set_attr(node, "dir", &r.direction.to_string());
    doc.set_attr(node, "fadeDir", &r.fade_direction.to_string());
    doc.set_attr(node, "sx", &r.sx.to_string());
    doc.set_attr(node, "sy", &r.sy.to_string());
    doc.set_attr(node, "kx", &r.kx.to_string());
    doc.set_attr(node, "ky", &r.ky.to_string());
    doc.set_attr(node, "algn", algn);
    doc.set_attr(node, "rotWithShape", if r.rot_with_shape { "1" } else { "0" });
    Ok(())
}

fn apply_soft_edge(doc: &mut XmlDocument, list: NodeId, soft_edge: Option<i64>) -> Result<(), Error> {
    let soft_edges: Vec<_> = doc.elements(list).filter(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "softEdge").collect();
    let Some(radius) = soft_edge else {
        for n in soft_edges { doc.detach(n); }
        return Ok(());
    };
    if !(0..=i32::MAX as i64).contains(&radius) { return Err(unsupported("このぼかしの半径の範囲")); }
    let x = Prefixes::at(doc, list);
    let node = match soft_edges.first() {
        Some(&n) => n,
        None => {
            let index = doc.children(list).len();
            doc.insert_element(list, index, &format!("{}softEdge", x.a))
        }
    };
    doc.set_attr(node, "rad", &radius.to_string());
    Ok(())
}

fn write_shape_shadow(doc: &mut XmlDocument, node: NodeId, old: &ibis_doc_model::AutoShape, new_shape: &ibis_doc_model::AutoShape) -> Result<(), Error> {
    let list = direct_effect_list(doc, node, old)?;
    apply_shadow(doc, list, old.shadow.map(|s| s.color), new_shape.shadow.as_ref())
}

fn write_shape_reflection(doc: &mut XmlDocument, node: NodeId, old: &ibis_doc_model::AutoShape, new_shape: &ibis_doc_model::AutoShape) -> Result<(), Error> {
    let list = direct_effect_list(doc, node, old)?;
    apply_reflection(doc, list, new_shape.reflection.as_ref())
}

fn write_shape_soft_edge(doc: &mut XmlDocument, node: NodeId, old: &ibis_doc_model::AutoShape, new_shape: &ibis_doc_model::AutoShape) -> Result<(), Error> {
    let list = direct_effect_list(doc, node, old)?;
    apply_soft_edge(doc, list, new_shape.soft_edge)
}

fn picture_effect_list(doc: &mut XmlDocument, node: NodeId, old: &ibis_doc_model::Picture) -> Result<NodeId, Error> {
    let pr = doc.elements(node).find(|&n| doc.namespace(n) == Some(new::NS_P) && doc.local_name(n) == "spPr").ok_or_else(|| unsupported("この画像の効果"))?;
    let list = doc.elements(pr).find(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "effectLst");
    let list = match list {
        Some(list) => list,
        None => {
            let inherited = old.shadow.is_some() || old.reflection.is_some() || old.soft_edge.is_some() || doc.path(node, &["nvPicPr", "nvPr", "ph"]).is_some();
            if inherited || doc.elements(pr).any(|n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "effectDag") {
                return Err(unsupported("継承された効果/effectDagの上書き"));
            }
            let x = Prefixes::at(doc, pr);
            let index = doc.children(pr).iter().position(|&n| doc.is_element(n) && doc.namespace(n) == Some(new::NS_A) && matches!(doc.local_name(n), "scene3d" | "sp3d" | "extLst")).unwrap_or(doc.children(pr).len());
            doc.insert_element(pr, index, &format!("{}effectLst", x.a))
        }
    };
    Ok(list)
}

fn write_picture_shadow(doc: &mut XmlDocument, node: NodeId, old: &ibis_doc_model::Picture, new_picture: &ibis_doc_model::Picture) -> Result<(), Error> {
    let list = picture_effect_list(doc, node, old)?;
    apply_shadow(doc, list, old.shadow.map(|s| s.color), new_picture.shadow.as_ref())
}

fn write_picture_reflection(doc: &mut XmlDocument, node: NodeId, old: &ibis_doc_model::Picture, new_picture: &ibis_doc_model::Picture) -> Result<(), Error> {
    let list = picture_effect_list(doc, node, old)?;
    apply_reflection(doc, list, new_picture.reflection.as_ref())
}

fn write_picture_soft_edge(doc: &mut XmlDocument, node: NodeId, old: &ibis_doc_model::Picture, new_picture: &ibis_doc_model::Picture) -> Result<(), Error> {
    let list = picture_effect_list(doc, node, old)?;
    apply_soft_edge(doc, list, new_picture.soft_edge)
}

fn write_shape_bevel(doc: &mut XmlDocument, node: NodeId, _old: &ibis_doc_model::AutoShape, new_shape: &ibis_doc_model::AutoShape) -> Result<(), Error> {
    let pr = doc.elements(node).find(|&n| doc.namespace(n) == Some(new::NS_P) && doc.local_name(n) == "spPr").ok_or_else(|| unsupported("この図形の効果"))?;
    let sp3d = doc.elements(pr).find(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "sp3d");
    let Some(ref bevel) = new_shape.bevel else {
        if let Some(sp3d_node) = sp3d {
            let bevels: Vec<_> = doc.elements(sp3d_node).filter(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "bevelT").collect();
            for n in bevels { doc.detach(n); }
            if doc.elements(sp3d_node).count() == 0 && doc.attributes(sp3d_node).is_empty() {
                doc.detach(sp3d_node);
            }
        }
        return Ok(());
    };
    if !(0..=i32::MAX as i64).contains(&bevel.width) || !(0..=i32::MAX as i64).contains(&bevel.height) {
        return Err(unsupported("この面取りの幅・高さの範囲"));
    }
    let x = Prefixes::at(doc, pr);
    let sp3d_node = match sp3d {
        Some(n) => n,
        None => {
            let index = doc.children(pr).iter().position(|&n| doc.is_element(n) && doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "extLst").unwrap_or(doc.children(pr).len());
            doc.insert_element(pr, index, &format!("{}sp3d", x.a))
        }
    };
    let bevels: Vec<_> = doc.elements(sp3d_node).filter(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "bevelT").collect();
    let bevel_node = match bevels.first() {
        Some(&n) => n,
        None => {
            let index = 0;
            doc.insert_element(sp3d_node, index, &format!("{}bevelT", x.a))
        }
    };
    doc.set_attr(bevel_node, "prst", bevel.preset.as_str());
    doc.set_attr(bevel_node, "w", &bevel.width.to_string());
    doc.set_attr(bevel_node, "h", &bevel.height.to_string());
    Ok(())
}

fn write_shape_tilt(doc: &mut XmlDocument, node: NodeId, _old: &ibis_doc_model::AutoShape, new_shape: &ibis_doc_model::AutoShape) -> Result<(), Error> {
    let pr = doc.elements(node).find(|&n| doc.namespace(n) == Some(new::NS_P) && doc.local_name(n) == "spPr").ok_or_else(|| unsupported("この図形の効果"))?;
    let x = Prefixes::at(doc, pr);

    // 1. scene3d
    let scene3d = doc.elements(pr).find(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "scene3d");
    match &new_shape.tilt {
        None => {
            if let Some(scene3d_node) = scene3d {
                doc.detach(scene3d_node);
            }
        }
        Some(tilt) => {
            let scene3d_node = match scene3d {
                Some(n) => n,
                None => {
                    let index = doc.children(pr).iter().position(|&n| {
                        doc.is_element(n) && doc.namespace(n) == Some(new::NS_A) && sp_pr_rank(doc.local_name(n)) > 5
                    }).unwrap_or(doc.children(pr).len());
                    doc.insert_element(pr, index, &format!("{}scene3d", x.a))
                }
            };
            let camera = match doc.child(scene3d_node, "camera") {
                Some(c) => c,
                None => {
                    let index = 0;
                    doc.insert_element(scene3d_node, index, &format!("{}camera", x.a))
                }
            };
            let prst = tilt.camera_preset.map(|p| p.as_str()).unwrap_or("orthographicFront");
            doc.set_attr(camera, "prst", prst);
            let rot = match doc.child(camera, "rot") {
                Some(r) => r,
                None => doc.append_element(camera, &format!("{}rot", x.a)),
            };
            doc.set_attr(rot, "lat", &super::pptx_new::normalize_ooxml_angle(tilt.lat).to_string());
            doc.set_attr(rot, "lon", &super::pptx_new::normalize_ooxml_angle(tilt.lon).to_string());
            doc.set_attr(rot, "rev", &super::pptx_new::normalize_ooxml_angle(tilt.rev).to_string());

            if doc.child(scene3d_node, "lightRig").is_none() {
                let light = doc.append_element(scene3d_node, &format!("{}lightRig", x.a));
                doc.set_attr(light, "rig", "threePt");
                doc.set_attr(light, "dir", "t");
            }
        }
    }

    // 2. sp3d
    let sp3d = doc.elements(pr).find(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "sp3d");
    match &new_shape.tilt {
        None => {
            if let Some(sp3d_node) = sp3d {
                doc.remove_attr(sp3d_node, "z");
                doc.remove_attr(sp3d_node, "extrusionH");
                let clrs: Vec<_> = doc.elements(sp3d_node).filter(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "extrusionClr").collect();
                for c in clrs { doc.detach(c); }
                if doc.elements(sp3d_node).count() == 0 && doc.attributes(sp3d_node).is_empty() {
                    doc.detach(sp3d_node);
                }
            }
        }
        Some(tilt) => {
            let sp3d_node = match sp3d {
                Some(n) => n,
                None => {
                    let index = doc.children(pr).iter().position(|&n| {
                        doc.is_element(n) && doc.namespace(n) == Some(new::NS_A) && sp_pr_rank(doc.local_name(n)) > 6
                    }).unwrap_or(doc.children(pr).len());
                    doc.insert_element(pr, index, &format!("{}sp3d", x.a))
                }
            };
            if tilt.depth != 0 {
                doc.set_attr(sp3d_node, "z", &tilt.depth.to_string());
            } else {
                doc.remove_attr(sp3d_node, "z");
            }
            if tilt.extrusion_h != 0 {
                doc.set_attr(sp3d_node, "extrusionH", &tilt.extrusion_h.to_string());
            } else {
                doc.remove_attr(sp3d_node, "extrusionH");
            }
            let clrs: Vec<_> = doc.elements(sp3d_node).filter(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "extrusionClr").collect();
            for c in clrs { doc.detach(c); }
            if let Some(color) = tilt.extrusion_color {
                let clr_node = doc.append_element(sp3d_node, &format!("{}extrusionClr", x.a));
                let srgb = doc.append_element(clr_node, &format!("{}srgbClr", x.a));
                let hex = format!("{:06X}", color.rgb.0 & 0xffffff);
                doc.set_attr(srgb, "val", &hex);
            }
        }
    }

    Ok(())
}

/// Keep image relationships, fill options and unknown attributes untouched.
fn write_picture_crop(doc: &mut XmlDocument, node: NodeId, crop: ibis_doc_model::Crop) -> Result<(), Error> {
    let fill = doc.child(node, "blipFill").ok_or_else(|| unsupported("この画像のトリミング"))?;
    let rect = match doc.child(fill, "srcRect") {
        Some(rect) => rect,
        None => {
            if crop == ibis_doc_model::Crop::default() { return Ok(()); }
            let x = Prefixes::at(doc, fill);
            let rect = x.import(doc, &format!("<{}srcRect/>", x.a))?;
            let after = doc.elements(fill).find(|&n| matches!(doc.local_name(n), "tile" | "stretch" | "extLst"));
            if let Some(after) = after { doc.insert_before(after, rect); } else { doc.insert(fill, doc.children(fill).len(), rect); }
            rect
        }
    };
    for (name, value) in [("l", crop.left), ("t", crop.top), ("r", crop.right), ("b", crop.bottom)] {
        doc.set_attr(rect, name, &value.to_string());
    }
    Ok(())
}

fn write_picture_corner(doc: &mut XmlDocument, node: NodeId, corner: u32) -> Result<(), Error> {
    let props = doc.child(node, "spPr").ok_or_else(|| unsupported("この画像の角丸"))?;
    let prst = match doc.child(props, "prstGeom") {
        Some(prst) => prst,
        None => {
            let a = Prefixes::at(doc, props).a;
            doc.append_element(props, &format!("{a}prstGeom"))
        }
    };
    let a = Prefixes::at(doc, prst).a;
    if corner > 0 {
        doc.set_attr(prst, "prst", "roundRect");
        let av = match doc.child(prst, "avLst") {
            Some(av) => av,
            None => doc.insert_element(prst, 0, &format!("{a}avLst")),
        };
        for c in doc.children(av).to_vec() {
            doc.detach(c);
        }
        let gd = doc.append_element(av, &format!("{a}gd"));
        doc.set_attr(gd, "name", "adj");
        doc.set_attr(gd, "fmla", &format!("val {corner}"));
    } else {
        doc.set_attr(prst, "prst", "rect");
        if let Some(av) = doc.child(prst, "avLst") {
            for c in doc.children(av).to_vec() {
                doc.detach(c);
            }
        }
    }
    Ok(())
}

fn write_picture_duotone(doc: &mut XmlDocument, node: NodeId, duotone: Option<(ibis_doc_model::Rgb, ibis_doc_model::Rgb)>) -> Result<(), Error> {
    let blip = doc.path(node, &["blipFill", "blip"]).ok_or_else(|| unsupported("この画像の色効果"))?;
    let existing: Vec<_> = doc.elements(blip).filter(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "duotone").collect();
    for n in existing {
        doc.detach(n);
    }
    if let Some((dark, light)) = duotone {
        let x = Prefixes::at(doc, blip);
        let dark_xml = new::color_xml(&x, ibis_doc_model::Color::opaque(dark));
        let light_xml = new::color_xml(&x, ibis_doc_model::Color::opaque(light));
        let duo = x.import(doc, &format!("<{}duotone>{dark_xml}{light_xml}</{}duotone>", x.a, x.a))?;
        doc.insert(blip, 0, duo);
    }
    Ok(())
}

fn write_picture_clear(doc: &mut XmlDocument, node: NodeId, clear: Option<ibis_doc_model::Rgb>) -> Result<(), Error> {
    let blip = doc.path(node, &["blipFill", "blip"]).ok_or_else(|| unsupported("この画像の色効果"))?;
    let existing: Vec<_> = doc.elements(blip).filter(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "clrChange").collect();
    for n in existing {
        doc.detach(n);
    }
    if let Some(color) = clear {
        let x = Prefixes::at(doc, blip);
        let from_xml = new::color_xml(&x, ibis_doc_model::Color::opaque(color));
        let to_xml = new::color_xml(&x, ibis_doc_model::Color { rgb: color, alpha: 0 });
        let change = x.import(doc, &format!("<{}clrChange><{}clrFrom>{from_xml}</{}clrFrom><{}clrTo>{to_xml}</{}clrTo></{}clrChange>", x.a, x.a, x.a, x.a, x.a, x.a))?;
        doc.insert(blip, 0, change);
    }
    Ok(())
}


fn write_picture_luminance(doc: &mut XmlDocument, node: NodeId, luminance: Option<(i32, i32)>) -> Result<(), Error> {
    let blip = doc.path(node, &["blipFill", "blip"]).ok_or_else(|| unsupported("この画像の明るさ・コントラスト"))?;
    let existing: Vec<_> = doc.elements(blip).filter(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "lum").collect();
    for n in existing {
        doc.detach(n);
    }
    if let Some((bright, contrast)) = luminance {
        let x = Prefixes::at(doc, blip);
        let mut attrs = String::new();
        if bright != 0 {
            attrs.push_str(&format!(r#" bright="{bright}""#));
        }
        if contrast != 0 {
            attrs.push_str(&format!(r#" contrast="{contrast}""#));
        }
        if !attrs.is_empty() {
            let lum = x.import(doc, &format!("<{}lum{attrs}/>", x.a))?;
            doc.insert(blip, 0, lum);
        }
    }
    Ok(())
}

fn write_picture_artistic_effect(doc: &mut XmlDocument, node: NodeId, artistic_effect: Option<ibis_doc_model::ArtisticEffect>) -> Result<(), Error> {
    let blip = doc.path(node, &["blipFill", "blip"]).ok_or_else(|| unsupported("この画像のアート効果"))?;
    let x = Prefixes::at(doc, blip);
    let ext_lst = doc.elements(blip).find(|&n| doc.local_name(n) == "extLst");
    let mut existing_art_ext = None;
    if let Some(list) = ext_lst {
        for ext in doc.elements(list) {
            if doc.local_name(ext) == "ext" {
                if doc.attr(ext, "uri") == Some("{BEBA8EAE-BF5A-486C-A8C5-ECC9F3942E4B}")
                    || doc.descendants(ext).into_iter().any(|n| doc.is_element(n) && (doc.local_name(n) == "imgProps" || doc.local_name(n) == "imgEffect"))
                {
                    existing_art_ext = Some(ext);
                    break;
                }
            }
        }
    }
    match artistic_effect {
        Some(effect) => {
            let eff_xml = super::pptx_new::artistic_effect_xml(&effect);
            let ext_content = format!(
                r#"<{}ext uri="{{BEBA8EAE-BF5A-486C-A8C5-ECC9F3942E4B}}"><a14:imgProps xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main"><a14:imgLayer><a14:imgEffect>{eff_xml}</a14:imgEffect></a14:imgLayer></a14:imgProps></{}ext>"#,
                x.a, x.a
            );
            let imported = x.import(doc, &ext_content)?;
            if let Some(old_ext) = existing_art_ext {
                doc.insert_before(old_ext, imported);
                doc.detach(old_ext);
            } else {
                let list = match ext_lst {
                    Some(l) => l,
                    None => {
                        let l = x.import(doc, &format!("<{}extLst/>", x.a))?;
                        let at = doc.children(blip).len();
                        doc.insert(blip, at, l);
                        l
                    }
                };
                let at = doc.children(list).len();
                doc.insert(list, at, imported);
            }
        }
        None => {
            if let Some(old_ext) = existing_art_ext {
                doc.detach(old_ext);
                if let Some(list) = ext_lst {
                    if doc.elements(list).next().is_none() {
                        doc.detach(list);
                    }
                }
            }
        }
    }
    Ok(())
}

fn write_picture_clip(doc: &mut XmlDocument, node: NodeId, clip: Option<ibis_doc_model::Clip>) -> Result<(), Error> {
    let Some(clip) = clip else { return Ok(()) };
    let Some(nv_pr) = doc.path(node, &["nvPicPr", "nvPr"]) else { return Ok(()) };
    let media_node = doc.descendants(nv_pr).into_iter().find(|&n| doc.is_element(n) && doc.local_name(n) == "media");
    if let Some(m) = media_node {
        // The playing options are those of the timing (`p:cMediaNode`), not attributes of
        // `p14:media`: the ones an earlier version wrote there are taken away.
        for stale in ["vol", "loop", "rewind", "autoPlay", "fullScreen", "showWhenStopped", "isNarration", "playAcrossSlides"] {
            doc.remove_attr(m, stale);
        }
        let existing_trim = doc.elements(m).find(|&n| doc.local_name(n) == "trim");
        if clip.options.trim_start_ms > 0 || clip.options.trim_end_ms > 0 {
            let trim_node = match existing_trim {
                Some(t) => t,
                None => {
                    let p14 = doc.prefix(m).map(|p| format!("{p}:")).unwrap_or_else(|| "p14:".to_string());
                    doc.append_element(m, &format!("{p14}trim"))
                }
            };
            if clip.options.trim_start_ms > 0 {
                doc.set_attr(trim_node, "st", &clip.options.trim_start_ms.to_string());
            } else {
                doc.remove_attr(trim_node, "st");
            }
            if clip.options.trim_end_ms > 0 {
                doc.set_attr(trim_node, "end", &clip.options.trim_end_ms.to_string());
            } else {
                doc.remove_attr(trim_node, "end");
            }
        } else if let Some(t) = existing_trim {
            doc.detach(t);
        }

        let existing_fade = doc.elements(m).find(|&n| doc.local_name(n) == "fade");
        if clip.options.fade_in_ms > 0 || clip.options.fade_out_ms > 0 {
            let fade_node = match existing_fade {
                Some(f) => f,
                None => {
                    let p14 = doc.prefix(m).map(|p| format!("{p}:")).unwrap_or_else(|| "p14:".to_string());
                    doc.append_element(m, &format!("{p14}fade"))
                }
            };
            if clip.options.fade_in_ms > 0 {
                doc.set_attr(fade_node, "in", &clip.options.fade_in_ms.to_string());
            } else {
                doc.remove_attr(fade_node, "in");
            }
            if clip.options.fade_out_ms > 0 {
                doc.set_attr(fade_node, "out", &clip.options.fade_out_ms.to_string());
            } else {
                doc.remove_attr(fade_node, "out");
            }
        } else if let Some(f) = existing_fade {
            doc.detach(f);
        }

        let existing_bmk_lst = doc.elements(m).find(|&n| doc.local_name(n) == "bmkLst");
        if !clip.options.bookmarks.is_empty() {
            let bmk_lst_node = match existing_bmk_lst {
                Some(b) => {
                    let children: Vec<_> = doc.elements(b).collect();
                    for c in children {
                        doc.detach(c);
                    }
                    b
                }
                None => {
                    let p14 = doc.prefix(m).map(|p| format!("{p}:")).unwrap_or_else(|| "p14:".to_string());
                    doc.append_element(m, &format!("{p14}bmkLst"))
                }
            };
            let p14 = doc.prefix(m).map(|p| format!("{p}:")).unwrap_or_else(|| "p14:".to_string());
            for bmk in &clip.options.bookmarks {
                let bmk_node = doc.append_element(bmk_lst_node, &format!("{p14}bmk"));
                doc.set_attr(bmk_node, "name", &bmk.name);
                doc.set_attr(bmk_node, "time", &bmk.time_ms.to_string());
            }
        } else if let Some(b) = existing_bmk_lst {
            doc.detach(b);
        }

        let existing_captions = doc.elements(m).find(|&n| doc.local_name(n) == "captionTracks");
        if !clip.options.captions.is_empty() {
            let captions_node = match existing_captions {
                Some(c) => {
                    let children: Vec<_> = doc.elements(c).collect();
                    for ch in children {
                        doc.detach(ch);
                    }
                    c
                }
                None => {
                    let p14 = doc.prefix(m).map(|p| format!("{p}:")).unwrap_or_else(|| "p14:".to_string());
                    doc.append_element(m, &format!("{p14}captionTracks"))
                }
            };
            let p14 = doc.prefix(m).map(|p| format!("{p}:")).unwrap_or_else(|| "p14:".to_string());
            for track in &clip.options.captions {
                let track_node = doc.append_element(captions_node, &format!("{p14}captionTrack"));
                doc.set_attr(track_node, "label", &track.label);
                doc.set_attr(track_node, "lang", &track.lang);
                doc.set_attr(track_node, "vtt", &track.vtt);
            }
        } else if let Some(c) = existing_captions {
            doc.detach(c);
        }
    }
    Ok(())
}

/// A frame in slide coordinates, in the coordinates of the shape's groups.
/// A shape turned about a quarter (`rotation`) takes the vertical scale on its width and the horizontal one on
/// its height (see the reader's `turned_sideways`).
fn to_group(frame: EmuRect, group: Option<[f64; 4]>, rotation: i32) -> EmuRect {
    let Some([sx, sy, tx, ty]) = group else { return frame };
    let div = |v: f64, s: f64| if s == 0.0 { v } else { v / s };
    let (w_scale, h_scale) = if crate::reader::turned_sideways(rotation) { (sy, sx) } else { (sx, sy) };
    let (cx, cy) = (frame.x as f64 + frame.width as f64 / 2.0, frame.y as f64 + frame.height as f64 / 2.0);
    let (w, h) = (div(frame.width as f64, w_scale), div(frame.height as f64, h_scale));
    if w_scale == sx && h_scale == sy {
        return EmuRect::new(div(frame.x as f64 - tx, sx).round() as i64, div(frame.y as f64 - ty, sy).round() as i64, w.round() as i64, h.round() as i64);
    }
    // The centre goes through the group's scale, the size through the swapped one.
    let (cx, cy) = (div(cx - tx, sx), div(cy - ty, sy));
    EmuRect::new((cx - w / 2.0).round() as i64, (cy - h / 2.0).round() as i64, w.round() as i64, h.round() as i64)
}

/// Whether two texts have the same paragraphs and runs (texts), whatever their formats.
fn same_runs(a: &TextBody, b: &TextBody) -> bool {
    a.paragraphs.len() == b.paragraphs.len()
        && a.paragraphs.iter().zip(&b.paragraphs).all(|(p, q)| p.runs.len() == q.runs.len() && p.runs.iter().zip(&q.runs).all(|(x, y)| x.text == y.text))
}

/// Give a shape an `a:xfrm` of its own if it has none (a placeholder that
/// took its layout's place keeps it).
fn pin_frame(doc: &mut XmlDocument, node: NodeId, shape: &Shape, group: Option<[f64; 4]>) -> Result<(), Error> {
    if doc.local_name(node) == "graphicFrame" {
        return Ok(());
    }
    xfrm_of(doc, node, shape, group).map(|_| ())
}

/// The shape's `a:xfrm` (`p:xfrm` of a graphic frame), made from `shape`
/// if it has none (a placeholder placed by its layout).
fn xfrm_of(doc: &mut XmlDocument, node: NodeId, shape: &Shape, group: Option<[f64; 4]>) -> Result<(NodeId, bool), Error> {
    if doc.local_name(node) == "graphicFrame" {
        return doc.child(node, "xfrm").map(|x| (x, false)).ok_or_else(|| unsupported("位置の無い図形の移動"));
    }
    let sp_pr = doc.child(node, "spPr").ok_or_else(|| unsupported("位置の無い図形の移動"))?;
    if let Some(x) = doc.child(sp_pr, "xfrm") {
        return Ok((x, false));
    }
    let x = Prefixes::at(doc, sp_pr);
    let mut local = shape.clone();
    local.frame = to_group(shape.frame, group, shape.rotation);
    let xfrm = x.import(doc, &new::xfrm_xml(&x, &format!("{}xfrm", x.a), &local))?;
    doc.insert(sp_pr, 0, xfrm);
    Ok((xfrm, true))
}

/// `shape` as its turned groups have it (undoing what the reader did,
/// `Transform::turn`): its place before the groups' turns and flips, the
/// outermost undone first.
pub(crate) fn untwisted(shape: &Shape, turns: &[super::pptx::GroupTurn]) -> Shape {
    let mut s = shape.clone();
    for t in turns {
        let (x, y) = (s.frame.x as f64 + s.frame.width as f64 / 2.0, s.frame.y as f64 + s.frame.height as f64 / 2.0);
        let a = -(t.rotation as f64 / 60_000.0).to_radians();
        let (dx, dy) = (x - t.cx, y - t.cy);
        let (mut x, mut y) = (t.cx + dx * a.cos() - dy * a.sin(), t.cy + dx * a.sin() + dy * a.cos());
        if t.flip_h {
            x = 2.0 * t.cx - x;
        }
        if t.flip_v {
            y = 2.0 * t.cy - y;
        }
        s.frame.x = (x - s.frame.width as f64 / 2.0).round() as i64;
        s.frame.y = (y - s.frame.height as f64 / 2.0).round() as i64;
        let mut rotation = s.rotation - t.rotation;
        if t.flip_h != t.flip_v {
            rotation = -rotation;
        }
        s.rotation = rotation.rem_euclid(21_600_000);
        s.flip_h ^= t.flip_h;
        s.flip_v ^= t.flip_v;
    }
    s
}

fn write_xfrm(doc: &mut XmlDocument, node: NodeId, source: &ShapeSource, old: &Shape, new: &Shape) -> Result<(), Error> {
    // In turned groups, the place in them.
    let (old, new) = (&untwisted(old, &source.turns), &untwisted(new, &source.turns));
    let (xfrm, made) = xfrm_of(doc, node, new, source.group)?;
    if made {
        return Ok(());
    }
    let b = to_group(new.frame, source.group, new.rotation);
    let a_prefix = Prefixes::at(doc, xfrm).a;
    let sub = |doc: &mut XmlDocument, local: &str, index: usize| -> NodeId {
        match doc.child(xfrm, local) {
            Some(n) => n,
            None => doc.insert_element(xfrm, index, &format!("{a_prefix}{local}")),
        }
    };
    if old.frame.x != new.frame.x || old.frame.y != new.frame.y {
        let off = sub(doc, "off", 0);
        if old.frame.x != new.frame.x || doc.attr(off, "x").is_none() {
            doc.set_attr(off, "x", &b.x.to_string());
        }
        if old.frame.y != new.frame.y || doc.attr(off, "y").is_none() {
            doc.set_attr(off, "y", &b.y.to_string());
        }
    }
    if old.frame.width != new.frame.width || old.frame.height != new.frame.height {
        let index = doc.child(xfrm, "off").and_then(|o| doc.children(xfrm).iter().position(|&c| c == o)).map_or(0, |i| i + 1);
        let ext = sub(doc, "ext", index);
        if old.frame.width != new.frame.width || doc.attr(ext, "cx").is_none() {
            doc.set_attr(ext, "cx", &b.width.to_string());
        }
        if old.frame.height != new.frame.height || doc.attr(ext, "cy").is_none() {
            doc.set_attr(ext, "cy", &b.height.to_string());
        }
    }
    if old.rotation != new.rotation {
        if new.rotation == 0 {
            doc.remove_attr(xfrm, "rot");
        } else {
            doc.set_attr(xfrm, "rot", &new.rotation.to_string());
        }
    }
    for (name, was, is) in [("flipH", old.flip_h, new.flip_h), ("flipV", old.flip_v, new.flip_v)] {
        if was != is {
            if is {
                doc.set_attr(xfrm, name, "1");
            } else {
                doc.remove_attr(xfrm, name);
            }
        }
    }
    Ok(())
}

const FILLS: [&str; 6] = ["noFill", "solidFill", "gradFill", "blipFill", "pattFill", "grpFill"];

/// The order of the children of `p:spPr`, by local name.
fn sp_pr_rank(local: &str) -> usize {
    match local {
        "xfrm" => 0,
        "custGeom" | "prstGeom" => 1,
        l if FILLS.contains(&l) => 2,
        "ln" => 3,
        "effectLst" | "effectDag" => 4,
        "scene3d" => 5,
        "sp3d" => 6,
        _ => 7,
    }
}

/// The order of the children of `a:ln`, by local name.
fn ln_rank(local: &str) -> usize {
    match local {
        l if FILLS.contains(&l) => 0,
        "prstDash" | "custDash" => 1,
        "round" | "bevel" | "miter" => 2,
        "headEnd" => 3,
        "tailEnd" => 4,
        _ => 5,
    }
}

/// Insert detached `node` among the element children of `parent` where
/// `rank` puts it.
fn insert_ordered(doc: &mut XmlDocument, parent: NodeId, node: NodeId, rank: fn(&str) -> usize) {
    let r = rank(doc.local_name(node));
    let next = doc.elements(parent).find(|&c| rank(doc.local_name(c)) > r);
    match next {
        Some(n) => doc.insert_before(n, node),
        None => {
            let index = doc.children(parent).len();
            doc.insert(parent, index, node);
        }
    }
}

/// Update fields that keep the original colour nodes and opaque contents.
pub(super) fn write_fill_properties(doc: &mut XmlDocument, parent: NodeId, old: &Fill, new: &Fill) -> Result<bool, Error> {
    if let (Fill::Pattern {foreground:old_fg,background:old_bg,..},Fill::Pattern {preset,foreground,background})=(old,new) {
        let fills=doc.elements(parent).filter(|&n|doc.namespace(n)==Some(new::NS_A)&&FILLS.contains(&doc.local_name(n))).collect::<Vec<_>>();
        let Some(&pattern)=fills.iter().find(|&&n|doc.local_name(n)=="pattFill") else {return Ok(false)};
        if fills.len()!=1 {return Err(unsupported("このパターンの原本構造"));}
        write_pattern_colour(doc,pattern,"fgClr",*old_fg,*foreground)?;
        write_pattern_colour(doc,pattern,"bgClr",*old_bg,*background)?;
        doc.set_attr(pattern,"prst",preset);return Ok(true);
    }
    write_gradient_properties(doc,parent,old,new)
}

/// Keep original nodes in place, including foreign attributes and children.
/// RGB conversion of themed/transformed colours is deliberately refused until
/// the semantics and retention of those source transforms are covered.
fn write_pattern_colour(doc: &mut XmlDocument, pattern: NodeId, name: &str, old: ibis_doc_model::Color, new: ibis_doc_model::Color) -> Result<(), Error> {
    if old==new {return Ok(());}
    let containers=doc.elements(pattern).filter(|&n|doc.namespace(n)==Some(new::NS_A)&&doc.local_name(n)==name).collect::<Vec<_>>();
    if containers.len()>1 {return Err(unsupported("パターン色の原本構造"));}
    let container=if let Some(&n)=containers.first() {n} else {
        let x=Prefixes::at(doc,pattern);
        let node=x.import(doc,&format!("<{}{name}>{}</{}{name}>",x.a,new::color_xml(&x,new),x.a))?;
        let position=if name=="fgClr" {0} else {doc.children(pattern).len()};
        doc.insert(pattern,position,node);return Ok(());
    };
    write_fill_colour(doc,container,old,new)
}

/// Edit a colour in its original container; preserve foreign siblings and
/// metadata inside the colour and opacity transforms.
fn write_fill_colour(doc: &mut XmlDocument, container: NodeId, old: ibis_doc_model::Color, new: ibis_doc_model::Color) -> Result<(), Error> {
    if old==new {return Ok(());}
    let colours=doc.elements(container).filter(|&n|doc.namespace(n)==Some(new::NS_A)&&["srgbClr","sysClr","schemeClr","prstClr","scrgbClr","hslClr"].contains(&doc.local_name(n))).collect::<Vec<_>>();
    if colours.len()>1 {return Err(unsupported("パターン色の原本構造"));}
    let Some(&colour)=colours.first() else {
        let x=Prefixes::at(doc,container);let node=x.import(doc,&new::color_xml(&x,new))?;doc.insert(container,0,node);return Ok(());
    };
    let transforms=doc.elements(colour).filter(|&n|doc.namespace(n)==Some(new::NS_A)).collect::<Vec<_>>();
    if old.rgb!=new.rgb {
        if doc.local_name(colour)!="srgbClr" || transforms.iter().any(|&n|!["alpha","alphaMod","alphaOff"].contains(&doc.local_name(n))) {
            return Err(unsupported("変換・テーマ色を持つ塗りのRGB編集"));
        }
        doc.set_attr(colour,"val",&format!("{:06X}",new.rgb.0&0xff_ffff));
    }
    if old.alpha!=new.alpha {
        let mut absolute=false;
        let value=((new.alpha as f64/255.0*100_000.0).round() as i64).to_string();
        for node in transforms {
            match doc.local_name(node) {
                "alpha"=>{absolute=true;doc.set_attr(node,"val",&value);},
                "alphaMod"=>doc.set_attr(node,"val","100000"),
                "alphaOff"=>doc.set_attr(node,"val","0"),
                _=>{},
            }
        }
        if !absolute && new.alpha!=255 {
            let x=Prefixes::at(doc,colour);let node=x.import(doc,&format!("<{}alpha val=\"{value}\"/>",x.a))?;
            let position=doc.children(colour).len();doc.insert(colour,position,node);
        }
    }
    Ok(())
}

/// Keep the original gradient tree, updating matched stops in place.
fn write_gradient_properties(doc: &mut XmlDocument, parent: NodeId, old: &Fill, new: &Fill) -> Result<bool, Error> {
    let old_stops=match old {Fill::Linear {stops,..}|Fill::TileLinear {stops,..}|Fill::Path {stops,..}=>stops,_=>return Ok(false)};
    let (stops,linear,path)=match new {
        Fill::Linear {angle,stops,..}|Fill::TileLinear {angle,stops,..}=>(stops,Some(*angle),None),
        Fill::Path {path,focus,stops}=>(stops,None,Some((*path,*focus))),
        _=>return Ok(false),
    };
    let fills=doc.elements(parent).filter(|&n|doc.namespace(n)==Some(new::NS_A)&&FILLS.contains(&doc.local_name(n))).collect::<Vec<_>>();
    let Some(&grad)=fills.iter().find(|&&n|doc.local_name(n)=="gradFill") else {return Ok(false)};
    if fills.len()!=1 || linear.is_some_and(|angle|!(0..21_600_000).contains(&angle)) || stops.iter().any(|s|s.position>100_000) || stops.windows(2).any(|s|s[0].position>s[1].position) {return Err(unsupported("このグラデーションの角度・位置変更"));}
    let list=doc.elements(grad).filter(|&n|doc.namespace(n)==Some(new::NS_A)&&doc.local_name(n)=="gsLst").collect::<Vec<_>>();
    let geometry_name=if linear.is_some() {"lin"} else {"path"};
    let geometry=doc.elements(grad).filter(|&n|doc.namespace(n)==Some(new::NS_A)&&["lin","path"].contains(&doc.local_name(n))).collect::<Vec<_>>();
    if list.len()!=1 || geometry.len()!=1 {return Err(unsupported("このグラデーションの原本構造"));}
    let mut nodes=doc.elements(list[0]).filter(|&n|doc.namespace(n)==Some(new::NS_A)&&doc.local_name(n)=="gs").collect::<Vec<_>>();
    nodes.sort_by_key(|&n|doc.attr(n,"pos").and_then(|v|v.parse::<u32>().ok()).unwrap_or(u32::MAX));
    if nodes.len()!=old_stops.len() || nodes.iter().zip(old_stops).any(|(&n,s)|doc.attr(n,"pos").and_then(|v|v.parse::<u32>().ok())!=Some(s.position)) {return Err(unsupported("このグラデーションの原本stop対応"));}
    if doc.local_name(geometry[0])!=geometry_name {
        if linear.is_some() {
            let rectangles=doc.elements(geometry[0]).filter(|&n|doc.namespace(n)==Some(new::NS_A)&&doc.local_name(n)=="fillToRect").collect::<Vec<_>>();
            // This focus disappears with the requested geometry change. Never
            // silently drop unknown metadata inside that disappearing node.
            if rectangles.iter().any(|&n|doc.elements(n).next().is_some() || !doc.text(n).trim().is_empty() || doc.attributes(n).iter().any(|a|!["l","t","r","b"].contains(&a.name.as_str())&&a.name!="xmlns"&&!a.name.starts_with("xmlns:"))) {
                return Err(unsupported("未知情報を持つfocusの線形gradient切替"));
            }
            for node in rectangles {doc.detach(node);}
            doc.remove_attr(geometry[0],"path");
        } else {
            doc.remove_attr(geometry[0],"ang");doc.remove_attr(geometry[0],"scaled");
        }
        let x=Prefixes::at(doc,geometry[0]);doc.rename(geometry[0],&format!("{}{geometry_name}",x.a));
    }
    if let Some(angle)=linear {doc.set_attr(geometry[0],"ang",&angle.to_string());}
    if let Some((path,focus))=path {
        let rectangles=doc.elements(geometry[0]).filter(|&n|doc.namespace(n)==Some(new::NS_A)&&doc.local_name(n)=="fillToRect").collect::<Vec<_>>();
        if rectangles.len()>1 {return Err(unsupported("このグラデーションのfocus原本構造"));}
        doc.set_attr(geometry[0],"path",path.name());
        if !rectangles.is_empty() || focus!=[0;4] {
            let rectangle=if let Some(&n)=rectangles.first() {n} else {let a=Prefixes::at(doc,geometry[0]).a;doc.insert_element(geometry[0],0,&format!("{a}fillToRect"))};
            for (key,value) in ["l","t","r","b"].into_iter().zip(focus) {doc.set_attr(rectangle,key,&value.to_string());}
        }
    }
    let original_nodes=nodes;
    // The model currently has no stop origin ID. For count changes, retain
    // exact survivors first, then stops with the same position or colour.
    // Remaining simultaneous edits use stable ordinal correspondence; this
    // cannot prove UI-row identity for completely changed/reordered stops.
    let mut origins=vec![None;stops.len()];let mut used=vec![false;old_stops.len()];
    if old_stops.len()==stops.len() {
        for (i,origin) in origins.iter_mut().enumerate() {*origin=Some(i);used[i]=true;}
    } else {
        for criterion in 0..3 {
            for (stop,origin) in stops.iter().zip(&mut origins) {
                if origin.is_some() {continue;}
                if let Some(i)=old_stops.iter().enumerate().position(|(i,previous)|!used[i]&&match criterion {0=>previous==stop,1=>previous.position==stop.position,_=>previous.color==stop.color}) {
                    *origin=Some(i);used[i]=true;
                }
            }
        }
        let mut remaining=(0..old_stops.len()).filter(|&i|!used[i]);
        for origin in origins.iter_mut().filter(|origin|origin.is_none()) {*origin=remaining.next();}
    }
    let mut nodes=Vec::with_capacity(stops.len());
    for (stop,origin) in stops.iter().zip(&origins) {
        let node=if let Some(i)=origin {
            let node=original_nodes[*i];write_fill_colour(doc,node,old_stops[*i].color,stop.color)?;node
        } else {
            let x=Prefixes::at(doc,list[0]);
            let node=x.import(doc,&format!("<{}gs pos=\"{}\">{}</{}gs>",x.a,stop.position,new::color_xml(&x,stop.color),x.a))?;
            let position=doc.children(list[0]).len();doc.insert(list[0],position,node);node
        };
        doc.set_attr(node,"pos",&stop.position.to_string());nodes.push(node);
    }
    for (i,node) in original_nodes.into_iter().enumerate() {
        if !origins.contains(&Some(i)) {doc.detach(node);}
    }
    // The reader's stable position sort follows XML order for ties. Preserve
    // the new model's tie order without moving unrelated or opaque children.
    let physical=doc.elements(list[0]).filter(|&n|doc.namespace(n)==Some(new::NS_A)&&doc.local_name(n)=="gs").collect::<Vec<_>>();
    let mut desired=physical.clone();let mut begin=0;
    while begin<stops.len() {
        let mut end=begin+1;while end<stops.len() && stops[end].position==stops[begin].position {end+=1;}
        if end-begin>1 {
            let group=&nodes[begin..end];let slots=physical.iter().enumerate().filter_map(|(i,n)|group.contains(n).then_some(i)).collect::<Vec<_>>();
            for (slot,&node) in slots.into_iter().zip(group) {desired[slot]=node;}
        }
        begin=end;
    }
    if desired!=physical {
        let slots=doc.children(list[0]).iter().enumerate().filter_map(|(i,n)|physical.contains(n).then_some(i)).collect::<Vec<_>>();
        for node in physical {doc.detach(node);}
        for (slot,node) in slots.into_iter().zip(desired) {doc.insert(list[0],slot,node);}
    }
    Ok(true)
}

fn write_fill(doc: &mut XmlDocument, node: NodeId, old: &Fill, fill: &Fill) -> Result<(), Error> {
    let sp_pr = doc.child(node, "spPr").ok_or_else(|| unsupported("この図形の塗りつぶしの変更"))?;
    new::check_fill(fill)?;
    if write_fill_properties(doc, sp_pr, old, fill)? { return Ok(()); }
    let x = Prefixes::at(doc, sp_pr);
    let xml = new::fill_xml(&x, fill)?;
    for c in doc.elements(sp_pr).filter(|&c| doc.namespace(c)==Some(new::NS_A) && FILLS.contains(&doc.local_name(c))).collect::<Vec<_>>() {
        doc.detach(c);
    }
    let element = x.import(doc, &xml)?;
    insert_ordered(doc, sp_pr, element, sp_pr_rank);
    Ok(())
}

fn write_stroke(doc: &mut XmlDocument, node: NodeId, old: Option<&Stroke>, new: Option<&Stroke>) -> Result<(), Error> {
    let sp_pr = doc.child(node, "spPr").ok_or_else(|| unsupported("この図形の枠線の変更"))?;
    let x = Prefixes::at(doc, sp_pr);
    let Some(ln) = doc.child(sp_pr, "ln") else {
        let element = x.import(doc, &new::line_xml(&x, new))?;
        insert_ordered(doc, sp_pr, element, sp_pr_rank);
        return Ok(());
    };
    let replace = |doc: &mut XmlDocument, names: &[&str], xml: Option<String>| -> Result<(), Error> {
        for c in doc.elements(ln).filter(|&c| doc.namespace(c) == Some(new::NS_A) && names.contains(&doc.local_name(c))).collect::<Vec<_>>() {
            doc.detach(c);
        }
        if let Some(xml) = xml {
            let element = x.import(doc, &xml)?;
            insert_ordered(doc, ln, element, ln_rank);
        }
        Ok(())
    };
    let Some(s) = new else {
        return replace(doc, &FILLS, Some(format!("<{}noFill/>", x.a)));
    };
    if old.is_none_or(|o| o.width != s.width) {
        doc.set_attr(ln, "w", &s.width.to_string());
    }
    if old.is_none_or(|o| o.color != s.color) {
        replace(doc, &FILLS, Some(format!("<{a}solidFill>{}</{a}solidFill>", new::color_xml(&x, s.color), a = x.a)))?;
    }
    if old.is_none_or(|o| o.dashed != s.dashed || o.dash != s.dash) {
        replace(doc, &["prstDash", "custDash"], new::dash_name(s).map(|d| format!("<{}prstDash val=\"{d}\"/>", x.a)))?;
    }
    if old.map_or(s.compound != ibis_doc_model::LineCompound::Single, |o| o.compound != s.compound) {
        doc.set_attr(ln, "cmpd", s.compound.name());
    }
    if old.map_or(s.alignment != ibis_doc_model::LineAlignment::Center, |o| o.alignment != s.alignment) {
        if s.alignment == ibis_doc_model::LineAlignment::Center {
            doc.remove_attr(ln, "algn");
        } else {
            doc.set_attr(ln, "algn", s.alignment.name());
        }
    }
    if old.is_none_or(|o| o.cap != s.cap) {
        doc.set_attr(ln, "cap", new::cap_name(s.cap));
    }
    if old.is_none_or(|o| o.head != s.head) {
        replace(doc, &["headEnd"], new::end_element(&x, "headEnd", s.head))?;
    }
    if old.is_none_or(|o| o.tail != s.tail) {
        replace(doc, &["tailEnd"], new::end_element(&x, "tailEnd", s.tail))?;
    }
    if old.is_none_or(|o| o.join != s.join) {
        replace(doc, &["round", "bevel", "miter"], new::join_element(&x, s.join))?;
    }
    Ok(())
}

/// Whether only the texts of runs changed (the same paragraphs and runs,
/// with the same formats), which `write_text` writes in place.
fn runs_only_changed(source: &ShapeSource, old: &TextBody, new: &TextBody) -> bool {
    let Some(runs) = source.runs.as_ref() else { return false };
    runs.len() == old.paragraphs.len()
        && old.paragraphs.len() == new.paragraphs.len()
        && old.paragraphs.iter().zip(&new.paragraphs).zip(runs).all(|((a, b), sources)| {
            a.runs.len() == b.runs.len()
                && a.runs.len() == sources.len()
                && a.runs.iter().zip(&b.runs).all(|(x, y)| x.style == y.style)
                && a.runs.iter().zip(&b.runs).zip(sources).all(|((x, y), src)| x.text == y.text || src.is_some() || y.style.field.is_some())
                && ibis_doc_model::Paragraph { runs: Vec::new(), ..a.clone() } == ibis_doc_model::Paragraph { runs: Vec::new(), ..b.clone() }
        })
        && TextBody { paragraphs: Vec::new(), ..old.clone() } == TextBody { paragraphs: Vec::new(), ..new.clone() }
}

/// Write text typed or deleted inside runs into the runs' `a:t` elements.
fn write_text(doc: &mut XmlDocument, source: &ShapeSource, old: &TextBody, new: &TextBody) -> Result<(), Error> {
    let not_yet = || unsupported("段落の追加・削除や書式の変更を含む文字の変更");
    let runs = source.runs.as_ref().ok_or_else(not_yet)?;
    // The same paragraphs and runs, with the same formats: only run texts differ.
    let same_shape = runs.len() == old.paragraphs.len()
        && old.paragraphs.len() == new.paragraphs.len()
        && old.paragraphs.iter().zip(&new.paragraphs).zip(runs).all(|((a, b), sources)| {
            a.runs.len() == b.runs.len()
                && a.runs.len() == sources.len()
                && a.runs.iter().zip(&b.runs).all(|(x, y)| x.style == y.style)
                && ibis_doc_model::Paragraph { runs: Vec::new(), ..a.clone() } == ibis_doc_model::Paragraph { runs: Vec::new(), ..b.clone() }
        })
        && TextBody { paragraphs: Vec::new(), ..old.clone() } == TextBody { paragraphs: Vec::new(), ..new.clone() };
    if !same_shape {
        return Err(not_yet());
    }
    // Replacements for each `a:t`: (start, length, text), in characters.
    let mut edits: HashMap<&[u32], Vec<(usize, usize, &str)>> = HashMap::new();
    for (paragraph_index, ((a, b), sources)) in old.paragraphs.iter().zip(&new.paragraphs).zip(runs).enumerate() {
        for (run_index, ((x, y), run_source)) in a.runs.iter().zip(&b.runs).zip(sources).enumerate() {
            if x.text == y.text { continue; }
            if let Some(kind) = y.style.field {
                let field_type = super::pptx_new::field_type(kind);
                let field = source.paragraphs.get(paragraph_index).and_then(|p| p.runs.get(run_index)).and_then(|path| doc.by_path(path))
                    .filter(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "fld" && doc.attr(n,"type") == Some(field_type.as_str()))
                    .ok_or_else(|| unsupported("この計算フィールドの保存位置"))?;
                // Refresh the cache, leaving the field id, run properties and
                // unrelated extension children in their original nodes.
                let existing = doc.elements(field).find(|&n| doc.namespace(n) == Some(new::NS_A) && doc.local_name(n) == "t");
                let t = match existing {
                    Some(t) => t,
                    None => { let a = Prefixes::at(doc, field).a; doc.append_element(field, &format!("{a}t")) },
                };
                // A field read as several runs (split where the script
                // changes) is one element: its text is theirs together.
                let path = source.paragraphs.get(paragraph_index).and_then(|p| p.runs.get(run_index));
                let whole: String = b.runs.iter().enumerate().filter(|(k, r)| r.style.field == y.style.field && source.paragraphs.get(paragraph_index).and_then(|p| p.runs.get(*k)) == path).map(|(_, r)| r.text.as_str()).collect();
                doc.set_text(t, &whole);
                continue;
            }
            let run_source = run_source.as_ref().ok_or_else(|| unsupported("自動で入る文字（スライド番号など）の変更"))?;
            edits.entry(run_source.t.as_slice()).or_default().push((run_source.start, run_source.len, y.text.as_str()));
        }
    }
    for (path, mut replacements) in edits {
        let t = doc.by_path(path).filter(|&t| doc.local_name(t) == "t").ok_or_else(|| Error::new(format!("{} の文字の位置が見つかりません", source.part)))?;
        let mut chars: Vec<char> = doc.text(t).chars().collect();
        // From the end, so earlier positions stay valid.
        replacements.sort_by(|a, b| b.0.cmp(&a.0));
        for (start, len, text) in replacements {
            if start + len > chars.len() {
                return Err(Error::new(format!("{} の文字の位置が合いません", source.part)));
            }
            chars.splice(start..start + len, text.chars());
        }
        let text: String = chars.into_iter().collect();
        doc.set_text(t, &text);
    }
    Ok(())
}

/// The part holding media `id`, written in this save if it is not yet.
const REL_VIDEO: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/video";
const REL_AUDIO: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/audio";
const REL_MEDIA_2010: &str = "http://schemas.microsoft.com/office/2007/relationships/media";

pub(super) fn media_part(package: &mut Package, current: &Presentation, id: MediaId, media: &mut MediaParts) -> Result<String, Error> {
    if let Some(part) = media.parts.get(&id) {
        return Ok(part.clone());
    }
    let m = current.media(id).ok_or_else(|| Error::new("画像のデータがありません"))?;
    if let Some((ext, content_type)) = m.format.av_type() {
        let part = next_part_name(package, &format!("ppt/media/media1.{ext}"));
        package.set_bytes(&part, m.data.clone());
        if package.content_type(&part)?.is_none() {
            add_default_content_type(package, ext, content_type)?;
        }
        media.parts.insert(id, part.clone());
        return Ok(part);
    }
    let (ext, content_type) = match m.format {
        MediaFormat::Png => ("png", "image/png"),
        MediaFormat::Jpeg => ("jpeg", "image/jpeg"),
        MediaFormat::Gif => ("gif", "image/gif"),
        MediaFormat::Bmp => ("bmp", "image/bmp"),
        MediaFormat::Tiff => ("tiff", "image/tiff"),
        MediaFormat::Emf => ("emf", "image/x-emf"),
        MediaFormat::Wmf => ("wmf", "image/x-wmf"),
        MediaFormat::Svg => ("svg", "image/svg+xml"),
        MediaFormat::Webp | MediaFormat::Other | _ => return Err(unsupported("この形式の画像（WebP など）の追加")),
    };
    let part = next_part_name(package, &format!("ppt/media/image1.{ext}"));
    package.set_bytes(&part, m.data.clone());
    if package.content_type(&part)?.is_none() {
        add_default_content_type(package, ext, content_type)?;
    }
    media.parts.insert(id, part.clone());
    Ok(part)
}

pub(super) fn add_default_content_type(package: &mut Package, ext: &str, content_type: &str) -> Result<(), Error> {
    let doc = package.xml_mut(ibis_ooxml::package::CONTENT_TYPES)?;
    let root = doc.root_element().ok_or_else(|| Error::new("[Content_Types].xml が空です"))?;
    let prefix = doc.prefix(root).map(|p| format!("{p}:")).unwrap_or_default();
    let first_override = doc.children_named(root, "Override").next();
    let index = first_override.and_then(|o| doc.children(root).iter().position(|&c| c == o)).unwrap_or(doc.children(root).len());
    let d = doc.insert_element(root, index, &format!("{prefix}Default"));
    doc.set_attr(d, "Extension", ext);
    doc.set_attr(d, "ContentType", content_type);
    Ok(())
}

/// The preset name of a geometry (`rect`, `roundRect`, `ellipse`, a
/// formula preset's), `None` for a custom one.
fn preset_name(geometry: &ibis_doc_model::Geometry) -> Option<&str> {
    match geometry {
        ibis_doc_model::Geometry::Rect => Some("rect"),
        ibis_doc_model::Geometry::RoundRect { .. } => Some("roundRect"),
        ibis_doc_model::Geometry::Ellipse => Some("ellipse"),
        ibis_doc_model::Geometry::Formula(f) => f.preset.as_deref(),
    }
}

/// 図形の変更: the shape's preset geometry (`a:prstGeom`) made `geometry`,
/// its adjustments (`a:avLst`) the model's.
fn write_geometry(doc: &mut XmlDocument, node: NodeId, geometry: &ibis_doc_model::Geometry) -> Result<(), Error> {
    let props = doc.child(node, "spPr").ok_or_else(|| unsupported("この図形の形の変更"))?;
    // A custom geometry (a freeform), or a preset for a custom one: the element made whole.
    let custom = preset_name(geometry).is_none();
    let old_custom = doc.child(props, "custGeom");
    if custom || old_custom.is_some() {
        let old = old_custom.or_else(|| doc.child(props, "prstGeom")).ok_or_else(|| unsupported("この図形の形の変更"))?;
        let x = Prefixes::at(doc, props);
        let xml = super::pptx_new::geometry_xml(&x, geometry)?;
        let new = x.import(doc, &xml)?;
        doc.insert_before(old, new);
        doc.detach(old);
        return Ok(());
    }
    let name = preset_name(geometry).ok_or_else(|| unsupported("任意の形への変更"))?;
    let prst = doc.child(props, "prstGeom").ok_or_else(|| unsupported("任意の形の図形の形の変更"))?;
    doc.set_attr(prst, "prst", name);
    let a = Prefixes::at(doc, prst).a;
    let av = match doc.child(prst, "avLst") {
        Some(av) => av,
        None => doc.insert_element(prst, 0, &format!("{a}avLst")),
    };
    for c in doc.children(av).to_vec() {
        doc.detach(c);
    }
    let guides: Vec<(String, String)> = match geometry {
        // The reader's default radius is the preset's own: nothing written.
        ibis_doc_model::Geometry::RoundRect { radius } if *radius != 16_667 => vec![("adj".into(), format!("val {radius}"))],
        ibis_doc_model::Geometry::Formula(f) => f.adjust.iter().map(|g| (g.name.clone(), g.formula.clone())).collect(),
        _ => Vec::new(),
    };
    for (n, f) in guides {
        let gd = doc.append_element(av, &format!("{a}gd"));
        doc.set_attr(gd, "name", &n);
        doc.set_attr(gd, "fmla", &f);
    }
    Ok(())
}
