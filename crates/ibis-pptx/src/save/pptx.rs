// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Writing changes to a presentation (`.pptx`) — the presentation担当's
//! file, and the example the other formats' writers follow.
//!
//! The reader (`reader.rs`) records:
//!
//! - for each slide, its part and the id of the relationship
//!   `presentation.xml` names it by;
//! - for each shape of a slide, the slide part and the element path of its
//!   `p:sp` / `p:cxnSp` / `p:pic` / `p:graphicFrame`, the group transform
//!   it was read through, and for each run of its text the `a:t` element
//!   and the characters of it the run holds (a run of the model is part of
//!   one `a:r`: the reader splits runs where the script changes and turns
//!   line breaks into paragraphs);
//! - for each layout, its part.
//!
//! What can be saved so far:
//!
//! - Slides added (a new slide from a layout, or a copy of a slide: the
//!   copy's part is the original's, with its notes and charts copied),
//!   removed (with their notes; sections follow) and reordered.
//! - Shapes added (text boxes, auto shapes of preset geometry, lines,
//!   pictures), removed (with their animations), moved in the z-order,
//!   moved, resized, rotated and flipped (`pptx_shapes.rs`).
//! - The fill (none, solid, linear gradient) and outline (none, colour,
//!   width, dashed, arrowheads) of a shape.
//! - Text typed or deleted inside the runs of a shape (no paragraph added
//!   or removed, no format changed).
//!
//! Anything else that changed is reported as not saved yet.

use std::collections::{HashMap, HashSet};

use ibis_doc_model::{LayoutId, Presentation, Shape, ShapeId, ShapeKind, Slide, SlideId};
use ibis_ooxml::package::{relationships_part, resolve};
use ibis_ooxml::{NodeId, Package, XmlDocument};

use super::pptx_new::{self as new, NS_R, Prefixes};
use super::pptx_shapes::{MediaParts, write_slide};
use super::unsupported;
use crate::Error;

const REL_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
const REL_SLIDE_LAYOUT: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout";
const CT_SLIDE: &str = "application/vnd.openxmlformats-officedocument.presentationml.slide+xml";
const URI_READONLY_RECOMMENDED: &str = "{D42A27BB-41C6-473E-B5CE-A1CE805E0A31}";
const NS_P1710: &str = "http://schemas.microsoft.com/office/powerpoint/2017/10/main";

/// Where the characters of one run of the model are in the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunSource {
    /// Element path of the `a:t` in the slide part.
    pub t: Vec<u32>,
    /// The run's first character in the `a:t`'s text, and its length, in characters.
    pub start: usize,
    pub len: usize,
}

/// A group's rotation (60 000ths of a degree) and flips about its centre
/// on the slide (`cx`, `cy`, EMU).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupTurn {
    pub cx: f64,
    pub cy: f64,
    pub rotation: i32,
    pub flip_h: bool,
    pub flip_v: bool,
}

/// Where a slide shape came from.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeSource {
    /// The slide part, such as `ppt/slides/slide3.xml`.
    pub part: String,
    /// Element path of the shape's element in the part.
    pub path: Vec<u32>,
    /// For each paragraph of the shape's text, where each run came from
    /// (`None` for text the reader made, such as a slide number).
    pub runs: Option<Vec<Vec<Option<RunSource>>>>,
    /// For each paragraph of the shape's text (as `runs`), its `a:p` and
    /// the element of each run (empty if not recorded).
    pub paragraphs: Vec<ParagraphSource>,
    /// The turns and flips of the groups the shape is in, outermost first
    /// (the shape's place on the slide went through them, the innermost
    /// first).
    pub turns: Vec<GroupTurn>,
    /// The transform from the coordinates of the groups the shape is in to
    /// the slide's (`[sx, sy, tx, ty]`: slide = child × s + t), if any.
    pub group: Option<[f64; 4]>,
}

/// Where a paragraph of the model came from: a line of an `a:p` (lines
/// after the first follow an `a:br`), and the `a:r` / `a:fld` of each run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParagraphSource {
    pub p: Vec<u32>,
    pub line: usize,
    pub runs: Vec<Vec<u32>>,
}

/// Where a slide came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlideSource {
    /// The slide part, such as `ppt/slides/slide3.xml`.
    pub part: String,
    /// The id of the relationship from the presentation part to it.
    pub rel_id: String,
}

/// Where the things of a presentation's model came from.
#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    /// The presentation part (`ppt/presentation.xml`).
    pub presentation: String,
    pub shapes: HashMap<ShapeId, ShapeSource>,
    pub slides: HashMap<SlideId, SlideSource>,
    /// Layout parts.
    pub layouts: HashMap<LayoutId, String>,
    /// Master parts.
    pub masters: HashMap<ibis_doc_model::MasterId, String>,
    /// The shapes drawn over a chart (its `c:userShapes`), read as shapes
    /// of the slide after it, by the chart's shape id. They are part of the
    /// chart's file and follow its frame.
    pub attached: HashMap<ShapeId, Vec<ShapeId>>,
}

/// Whether two models have the same content (slides and what is on them).
pub(crate) fn same_content(a: &Presentation, b: &Presentation) -> bool {
    a.slide_size == b.slide_size && a.first_slide_number == b.first_slide_number && a.layout_scale == b.layout_scale && !super::pptx_layouts::changed(a, b) && a.slides == b.slides && a.sections == b.sections && a.header_footer == b.header_footer && a.notes_marks == b.notes_marks && a.show_properties == b.show_properties && a.custom_shows == b.custom_shows && a.mark_as_final == b.mark_as_final && a.read_only_recommended == b.read_only_recommended && a.modify_verifier == b.modify_verifier && a.properties == b.properties && a.object_defaults == b.object_defaults && a.theme_colors == b.theme_colors && a.theme_fonts == b.theme_fonts && a.theme_effects == b.theme_effects && a.fonts == b.fonts && a.font_substitutions == b.font_substitutions
}

/// Change only p:sldSz; slide parts, masters and p:notesSz stay intact.
fn write_slide_size(package: &mut Package, map: &SourceMap, size: ibis_doc_model::EmuSize) -> Result<(), Error> {
    if !(914_400..=51_206_400).contains(&size.width) || !(914_400..=51_206_400).contains(&size.height) {
        return Err(Error::new("スライドの幅と高さは1〜56インチにしてください"));
    }
    let doc = package.xml_mut(&map.presentation)?;
    let root = doc.root_element().ok_or_else(|| Error::new("プレゼンテーションが空です"))?;
    let node = match doc.child(root, "sldSz") {
        Some(node) => node,
        None => {
            let x = Prefixes::at(doc, root);
            let node = x.import(doc, &format!("<{}sldSz/>", x.p))?;
            // p:sldSz follows the slide list and precedes notesSz and text defaults.
            let next = doc.elements(root).find(|&n| matches!(doc.local_name(n), "notesSz" | "smartTags" | "embeddedFontLst" | "custShowLst" | "photoAlbum" | "custDataLst" | "kinsoku" | "defaultTextStyle" | "modifyVerifier" | "extLst"));
            if let Some(next) = next { doc.insert_before(next, node); } else { doc.insert(root, doc.children(root).len(), node); }
            node
        }
    };
    doc.set_attr(node, "cx", &size.width.to_string());
    doc.set_attr(node, "cy", &size.height.to_string());
    let kind = if size == ibis_doc_model::EmuSize::WIDESCREEN { "screen16x9" } else if size == ibis_doc_model::EmuSize::new(9_144_000, 6_858_000) { "screen4x3" } else { "custom" };
    doc.set_attr(node, "type", kind);
    Ok(())
}

/// スライドのサイズの拡大縮小: the layout and master parts' shapes (`a:off`,
/// `a:ext`; not a group's child space) scaled as the model's layouts are.
fn scale_layout_parts(package: &mut Package, map: &SourceMap, scale: ibis_doc_model::LayoutScale) -> Result<(), Error> {
    let mut parts: Vec<String> = Vec::new();
    for layout in map.layouts.values() {
        if !parts.contains(layout) { parts.push(layout.clone()); }
        for rel in package.relationships(layout)? {
            if rel.kind.ends_with("/slideMaster") && !rel.external
                && let Some(master) = rel.target_part(layout)
                && !parts.contains(&master)
            {
                parts.push(master);
            }
        }
    }
    let f = |v: i64, offset: i64| (v as i128 * scale.ppm as i128 / 1_000_000) as i64 + offset;
    for part in parts {
        let doc = package.xml_mut(&part)?;
        let Some(root) = doc.root_element() else { continue };
        for node in doc.descendants(root) {
            let (is_off, is_ext) = (doc.local_name(node) == "off", doc.local_name(node) == "ext");
            if !(is_off || is_ext) || doc.parent(node).is_none_or(|p| doc.local_name(p) != "xfrm") { continue }
            let (a, b) = if is_off { ("x", "y") } else { ("cx", "cy") };
            let (da, db) = if is_off { (scale.dx, scale.dy) } else { (0, 0) };
            for (name, offset) in [(a, da), (b, db)] {
                if let Some(v) = doc.attr(node, name).and_then(|v| v.parse::<i64>().ok()) {
                    doc.set_attr(node, name, &f(v, offset).to_string());
                }
            }
        }
    }
    Ok(())
}

/// What a slide of the current model is written into.
/// A new slide made with its layout's placeholders (`Source::new_slide`):
/// those shapes as they read back once written, under the model's ids, and
/// where they are in the slide part (its name is the part's at saving).
#[derive(Clone, Debug, Default)]
pub(crate) struct NewSlide {
    pub shapes: Vec<Shape>,
    pub groups: Vec<ibis_doc_model::ShapeGroup>,
    pub animations: Vec<ibis_doc_model::Animation>,
    pub sources: HashMap<ShapeId, ShapeSource>,
    /// Pasted from another file: its slide part copied, not made blank.
    pub foreign: Option<super::pptx_foreign::ForeignSlide>,
}

pub(crate) struct Target<'a> {
    /// The part to write.
    pub part: String,
    /// The part the source map's paths of `old` refer to (the original of
    /// a copy), and the shapes as they are in it.
    pub lookup: Option<String>,
    pub old: &'a [Shape],
    pub old_groups: &'a [ibis_doc_model::ShapeGroup],
    /// The slide's layout before (a read slide).
    pub old_layout: Option<LayoutId>,
}

pub(crate) fn write_changes<'a>(package: &mut Package, map: &SourceMap, baseline: &'a Presentation, current: &Presentation, new_slides: &'a HashMap<SlideId, NewSlide>, new_charts: &HashMap<ShapeId, super::NewChartEntry>, chart_data: &super::pptx_chart_data::ChartDataEdits) -> Result<(), Error> {
    // A chartEx chart is drawn from its part as written, so a data edit leaves the model as it is.
    let ex_data_edited = |slide: &Slide| slide.shapes.iter().any(|s| matches!(&s.kind, ShapeKind::Chart(c) if c.ex.is_some() && super::pptx_chart_data::matching(chart_data, s.id, c).is_some()));
    if same_content(baseline, current) && !current.slides.iter().any(|s| ex_data_edited(s)) {
        return Ok(());
    }
    if baseline.slide_size != current.slide_size {
        write_slide_size(package, map, current.slide_size)?;
    }
    if baseline.first_slide_number != current.first_slide_number {
        let doc = package.xml_mut(&map.presentation)?;
        let root = doc.root_element().ok_or_else(|| Error::new("プレゼンテーションが空です"))?;
        if current.first_slide_number == 1 {
            doc.remove_attr(root, "firstSlideNum");
        } else {
            doc.set_attr(root, "firstSlideNum", &current.first_slide_number.to_string());
        }
    }
    if baseline.layout_scale != current.layout_scale {
        scale_layout_parts(package, map, current.layout_scale)?;
    }
    let layouts_extended;
    let map = if super::pptx_layouts::changed(baseline, current) {
        let made = super::pptx_layouts::write(package, map, baseline, current)?;
        if made.is_empty() {
            map
        } else {
            let mut m = map.clone();
            m.layouts.extend(made);
            layouts_extended = m;
            &layouts_extended
        }
    } else {
        map
    };
    if baseline.header_footer != current.header_footer {
        super::pptx_header_footer::write(package, &map.presentation, current.header_footer)?;
    }
    let show_props_need_update = baseline.show_properties != current.show_properties
        || (baseline.custom_shows != current.custom_shows
            && matches!(current.show_properties.slide_range, ibis_doc_model::ShowSlideRange::Custom(_)));
    if show_props_need_update {
        super::pptx_show::write(package, &map.presentation, &current.show_properties, &current.custom_shows)?;
    }
    if baseline.custom_shows != current.custom_shows {
        write_custom_shows(package, &map.presentation, &current.custom_shows, map)?;
    }
    if baseline.photo_album != current.photo_album {
        write_photo_album(package, &map.presentation, current.photo_album.as_ref())?;
    }
    if baseline.properties != current.properties || baseline.mark_as_final != current.mark_as_final || baseline.font_substitutions != current.font_substitutions {
        super::docx::write_core_properties(package, &current.properties)?;
        super::docx::write_app_properties(package, &current.properties)?;
        let mut custom_props = current.properties.custom.clone();
        custom_props.retain(|p| !p.name.starts_with("_FontSub_"));
        for (target, sub) in &current.font_substitutions {
            custom_props.push(ibis_doc_model::CustomProperty {
                name: format!("_FontSub_{target}"),
                value: ibis_doc_model::CustomPropertyValue::Text(sub.clone()),
            });
        }
        super::docx::write_custom_properties(package, &custom_props, current.mark_as_final)?;
    }
    if baseline.modify_verifier != current.modify_verifier {
        write_modify_verifier(package, map, current.modify_verifier.as_ref())?;
    }
    if baseline.read_only_recommended != current.read_only_recommended {
        write_read_only_recommended(package, map, current.read_only_recommended)?;
    }
    if baseline.object_defaults != current.object_defaults {
        super::pptx_theme::write_defaults(package, &map.presentation, &current.object_defaults)?;
    }
    if baseline.theme_colors != current.theme_colors || baseline.theme_fonts != current.theme_fonts {
        super::pptx_theme::write(package, &map.presentation, &current.theme_colors, current.theme_fonts.as_ref())?;
    }
    if baseline.theme_effects != current.theme_effects {
        super::pptx_theme::write_effects(package, &map.presentation, current.theme_effects.as_deref())?;
    }
    if baseline.fonts != current.fonts {
        write_embedded_fonts(package, &map.presentation, &current.fonts)?;
    }
    let old_slides: HashMap<SlideId, &Slide> = baseline.slides.iter().map(|s| (s.id, s)).collect();
    // The shapes comments are on, as they are in the file before anything
    // is written (their drawing ids stay; their places in the part may not).
    let comment_marks = comment_marks(package, map, current)?;
    // Slides whose animations changed (those of shapes still there), with
    // their shapes' drawing ids as the file has them.
    let animated = changed_animations(map, baseline, current, new_slides);
    let mut native_ids: HashMap<SlideId, HashMap<ShapeId, String>> = HashMap::new();
    for slide in current.slides.iter().filter(|s| animated.contains(&s.id)) {
        native_ids.insert(slide.id, drawing_ids(package, map, slide)?);
    }
    // Shapes named anew or shown or hidden (選択ウィンドウ), with the drawing
    // ids of those already in the file.
    let labels = shape_labels(&old_slides, current);
    let mut label_ids: HashMap<SlideId, HashMap<ShapeId, String>> = HashMap::new();
    for slide in current.slides.iter().filter(|s| labels.contains_key(&s.id)) {
        label_ids.insert(slide.id, drawing_ids(package, map, slide)?);
    }
    let ids = |p: &Presentation| p.slides.iter().map(|s| s.id).collect::<Vec<_>>();
    let mut targets: Vec<Target<'a>> = Vec::new();
    // Written after the slide list (copies are made from the originals) and the shapes.
    let mut backgrounds = Vec::new();
    for slide in &current.slides {
        match (old_slides.get(&slide.id), map.slides.get(&slide.id)) {
            (Some(old), Some(source)) => {
                let layout_changed = old.layout != slide.layout;
                if layout_changed {
                    set_layout(package, map, &source.part, slide)?;
                }
                // A background that follows the new layout's is the layout's own.
                let follows_layout = layout_changed && slide.layout.and_then(|l| current.layout(l)).is_some_and(|l| l.background == slide.background) && !has_own_background(package, &source.part)?;
                if old.background != slide.background && !follows_layout {
                    backgrounds.push((source.part.clone(), old.background.clone(), slide.background.clone()));
                }
                targets.push(Target { part: source.part.clone(), lookup: Some(source.part.clone()), old: &old.shapes, old_groups: &old.groups, old_layout: old.layout });
            }
            (Some(_), None) => return Err(unsupported("このスライドの変更")),
            (None, _) => targets.push(Target { part: String::new(), lookup: None, old: &[], old_groups: &[], old_layout: None }),
        }
    }
    // New slides' placeholders are found where they were written.
    let extended;
    let map = if ids(baseline) != ids(current) {
        let added = write_slide_list(package, map, baseline, current, &old_slides, &mut targets, new_slides)?;
        if added.is_empty() {
            map
        } else {
            let mut m = map.clone();
            m.shapes.extend(added);
            extended = m;
            &extended
        }
    } else {
        map
    };
    if baseline.sections!=current.sections {
        super::pptx_sections::write(package,&map.presentation,current)?;
    }
    let mut media = MediaParts::default();
    // Where each slide is written (for hyperlinks to slides).
    let mut created_ids: HashMap<SlideId, HashMap<ShapeId, String>> = HashMap::new();
    let slide_parts = super::pptx_links::slide_parts(current, &targets.iter().map(|t| t.part.clone()).collect::<Vec<_>>());
    for (slide, target) in current.slides.iter().zip(&targets) {
        let same_placeholders = old_slides.get(&slide.id).is_none_or(|o| o.placeholders == slide.placeholders);
        let unchanged = target.lookup.as_deref() == Some(target.part.as_str()) && target.old == slide.shapes.as_slice() && target.old_groups == slide.groups.as_slice() && same_placeholders;
        if unchanged && !ex_data_edited(slide) {
            continue;
        }
        let links = super::pptx_links::links_of(slide);
        let link_ids = if links.is_empty() { HashMap::new() } else { super::pptx_links::link_rels(package, &target.part, &links, &slide_parts)? };
        let created = super::pptx_links::with_link_ids(link_ids, || write_slide(package, map, current, target, slide, &mut media, new_charts, chart_data))?;
        if let Some(ids) = label_ids.get_mut(&slide.id) {
            ids.extend(created.clone());
        }
        if let Some(ids) = native_ids.get_mut(&slide.id) {
            ids.extend(created.clone());
        }
        created_ids.insert(slide.id, created);
    }
    // Last: a new `p:bg` shifts the paths of the shapes after it.
    for (part, old, fill) in &backgrounds {
        write_background(package, current, &mut media, part, old, fill)?;
    }
    // Speaker notes (a copy's notes page is the original's copy).
    for (slide, target) in current.slides.iter().zip(&targets) {
        let changed = match old_slides.get(&slide.id) {
            Some(old) => old.notes != slide.notes,
            // A copy has its original's notes page: written only if they differ.
            None => match &target.lookup {
                Some(origin) if *origin != target.part => baseline.slides.iter().find(|b| map.slides.get(&b.id).is_some_and(|s| &s.part == origin)).is_none_or(|b| b.notes != slide.notes),
                _ => !slide.notes.is_empty(),
            },
        };
        if changed {
            super::pptx_notes::write_notes(package, &map.presentation, &target.part, &slide.notes, &current.notes_marks, current.slides.iter().position(|s| s.id == slide.id).map_or(1, |i| i + current.first_slide_number))?;
        }
    }
    // A picture changed for another (図の圧縮, 画像の置換): the old one goes.
    if super::pptx_media_prune::changed(baseline, current) {
        super::pptx_media_prune::prune(package, &targets.iter().map(|t| t.part.clone()).collect::<Vec<_>>())?;
    }
    // The header, date, footer and number of the notes pages.
    if baseline.notes_marks != current.notes_marks {
        for (i, target) in targets.iter().enumerate() {
            super::pptx_notes::write_marks(package, &map.presentation, &target.part, &current.notes_marks, i + current.first_slide_number)?;
        }
        // The handout master's places say the same (what a handout prints in its margins).
        crate::handout_master::write_marks(package, &map.presentation, &current.notes_marks)?;
    }
    // Transitions (a copy has its original's).
    for (slide, target) in current.slides.iter().zip(&targets) {
        let original=old_slides.get(&slide.id).copied().or_else(||target.lookup.as_ref().and_then(|origin|baseline.slides.iter().find(|b|map.slides.get(&b.id).is_some_and(|s|&s.part==origin))));
        let before=original.and_then(|s|s.transition.as_ref());
        if before!=slide.transition.as_ref() {
            super::pptx_transition::write_transition(package,&target.part,before,slide.transition.as_ref())?;
        }
    }
    // Animations (アニメーション): the timing rebuilt from the model.
    for (slide, target) in current.slides.iter().zip(&targets) {
        let Some(ids) = native_ids.get(&slide.id) else { continue };
        let doc = package.xml_mut(&target.part)?;
        super::pptx_timing::write_timing(doc, &slide.animations, ids)?;
    }
    // Videos and audios (動画・音声): how they are played (the timing PowerPoint plays them by).
    for (slide, target) in current.slides.iter().zip(&targets) {
        let before = old_slides.get(&slide.id);
        let mut plays = Vec::new();
        let mut update = Vec::new();
        for shape in &slide.shapes {
            let ShapeKind::Picture(pic) = &shape.kind else { continue };
            let Some(clip) = &pic.clip else { continue };
            let old_clip = before.and_then(|b| b.shape(shape.id)).and_then(|s| if let ShapeKind::Picture(p) = &s.kind { p.clip.as_ref() } else { None });
            if old_clip.is_some_and(|o| o.options == clip.options && o.audio == clip.audio) {
                continue;
            }
            let spid = match created_ids.get(&slide.id).and_then(|ids| ids.get(&shape.id)) {
                Some(spid) => spid.clone(),
                None => match drawing_ids(package, map, slide)?.get(&shape.id) {
                    Some(spid) => spid.clone(),
                    None => continue,
                },
            };
            let duration_ms = current.media(clip.media).and_then(|m| super::pptx_media_timing::duration_ms(&m.data));
            let play = super::pptx_media_timing::MediaPlay { spid, audio: clip.audio, options: clip.options.clone(), duration_ms };
            if old_clip.is_some() { update.push(play) } else { plays.push(play) }
        }
        if plays.is_empty() && update.is_empty() {
            continue;
        }
        let doc = package.xml_mut(&target.part)?;
        super::pptx_media_timing::sync(doc, &plays, false)?;
        super::pptx_media_timing::sync(doc, &update, true)?;
    }
    // Comments (コメント), last: they add to the slide's extension list.
    for (slide, target) in current.slides.iter().zip(&targets) {
        let before = old_slides.get(&slide.id).map_or(&[][..], |o| o.comments.as_slice());
        if before == slide.comments.as_slice() {
            continue;
        }
        let (sld_id, cid) = slide_numbers(package, &map.presentation, &target.part)?;
        let mark = |id: ShapeId| comment_marks.get(&id).cloned();
        super::pptx_comments::write_comments(package, &map.presentation, &target.part, sld_id, cid, &slide.comments, &mark)?;
    }
    // Shapes' names and hidden shapes.
    for (slide, target) in current.slides.iter().zip(&targets) {
        let (Some(changes), Some(ids)) = (labels.get(&slide.id), label_ids.get(&slide.id)) else { continue };
        let links = super::pptx_links::links_of(slide);
        let link_ids = if links.is_empty() { HashMap::new() } else { super::pptx_links::link_rels(package, &target.part, &links, &slide_parts)? };
        let doc = package.xml_mut(&target.part)?;
        let Some(root) = doc.root_element() else { continue };
        let found: HashMap<String, NodeId> = doc.descendants(root).into_iter().filter(|&n| doc.local_name(n) == "cNvPr").filter_map(|n| doc.attr(n, "id").map(|id| (id.to_string(), n))).collect();
        let was_there = |id: &ShapeId| old_slides.get(&slide.id).is_some_and(|o| o.shape(*id).is_some());
        for change in changes {
            let (shape, name, hidden, link, lock, alt, dec, tooltip, rtz, zoom_show_bg, zoom_duration) = (&change.shape, &change.name, &change.hidden, &change.link, &change.lock, &change.alt, &change.dec, &change.tooltip, &change.return_to_zoom, &change.zoom_show_bg, &change.zoom_duration);
            let Some(node) = ids.get(shape).and_then(|id| found.get(id)).copied() else {
                // A new shape is written with its own name, if not found here.
                if was_there(shape) {
                    return Err(unsupported("この図形の名前・表示の変更"));
                }
                continue;
            };
            if let Some(name) = name {
                doc.set_attr(node, "name", name);
            }
            if let Some(alt_val) = alt {
                match alt_val {
                    Some(a) => {
                        if a.description.is_empty() {
                            doc.remove_attr(node, "descr");
                        } else {
                            doc.set_attr(node, "descr", &a.description);
                        }
                        if a.title.is_empty() {
                            doc.remove_attr(node, "title");
                        } else {
                            doc.set_attr(node, "title", &a.title);
                        }
                    }
                    None => {
                        doc.remove_attr(node, "descr");
                        doc.remove_attr(node, "title");
                    }
                }
            }
            if let Some(is_decorative) = dec {
                const DEC_URI: &str = "{C183D7F6-B498-43B3-948B-1728B52AA6E4}";
                if *is_decorative {
                    doc.remove_attr(node, "descr");
                    doc.remove_attr(node, "title");
                    let ext_lst = doc.child(node, "extLst").unwrap_or_else(|| {
                        doc.append_element(node, "a:extLst")
                    });
                    if doc.resolve_prefix(ext_lst, Some("a")).is_none() {
                        doc.set_attr(ext_lst, "xmlns:a", "http://schemas.openxmlformats.org/drawingml/2006/main");
                    }
                    let ext = doc.children(ext_lst).to_vec().into_iter().find(|&c| {
                        doc.is_element(c) && doc.local_name(c) == "ext" && doc.attr(c, "uri") == Some(DEC_URI)
                    }).unwrap_or_else(|| {
                        let e = doc.append_element(ext_lst, "a:ext");
                        doc.set_attr(e, "uri", DEC_URI);
                        e
                    });
                    let decorative_node = doc.children(ext).to_vec().into_iter().find(|&c| {
                        doc.is_element(c) && doc.local_name(c) == "decorative"
                    }).unwrap_or_else(|| {
                        let d = doc.append_element(ext, "a1611:decorative");
                        doc.set_attr(d, "xmlns:a1611", "http://schemas.microsoft.com/office/drawing/2016/11/main");
                        d
                    });
                    doc.set_attr(decorative_node, "val", "1");
                } else if let Some(ext_lst) = doc.child(node, "extLst") {
                    let to_remove: Vec<_> = doc.children(ext_lst).to_vec().into_iter().filter(|&c| {
                        doc.is_element(c) && doc.local_name(c) == "ext" && (
                            doc.attr(c, "uri") == Some(DEC_URI) ||
                            doc.descendants(c).into_iter().any(|d| doc.is_element(d) && doc.local_name(d) == "decorative")
                        )
                    }).collect();
                    for ext in to_remove {
                        doc.detach(ext);
                    }
                    if doc.elements(ext_lst).next().is_none() {
                        doc.detach(ext_lst);
                    }
                }
            }
            let has_zoom_changes = rtz.is_some() || zoom_show_bg.is_some() || zoom_duration.is_some();
            if has_zoom_changes {
                const ZOOM_URI: &str = "{D3508630-5900-4276-8680-4D969FE36771}";
                let cur_rtz = slide.shape_return_to_zoom.contains(shape);
                let cur_show_bg = slide.shape_zoom_show_bg.get(shape).copied();
                let cur_dur = slide.shape_zoom_durations.get(shape).copied();

                let ext_lst = doc.child(node, "extLst");
                let existing_ext = ext_lst.and_then(|el| {
                    doc.children(el).to_vec().into_iter().find(|&c| {
                        doc.is_element(c) && doc.local_name(c) == "ext" && doc.attr(c, "uri") == Some(ZOOM_URI)
                    })
                });
                let existing_zoom = existing_ext.and_then(|ext| {
                    doc.children(ext).to_vec().into_iter().find(|&c| {
                        doc.is_element(c) && doc.local_name(c) == "zoom"
                    })
                });

                let needs_zoom = cur_rtz || cur_show_bg.is_some() || cur_dur.is_some() || existing_zoom.is_some();
                let should_remove = *rtz == Some(false) && cur_show_bg.is_none() && cur_dur.is_none();

                if needs_zoom && !should_remove {
                    let ext_lst = ext_lst.unwrap_or_else(|| {
                        doc.append_element(node, "a:extLst")
                    });
                    if doc.resolve_prefix(ext_lst, Some("a")).is_none() {
                        doc.set_attr(ext_lst, "xmlns:a", "http://schemas.openxmlformats.org/drawingml/2006/main");
                    }
                    let ext = existing_ext.unwrap_or_else(|| {
                        let e = doc.append_element(ext_lst, "a:ext");
                        doc.set_attr(e, "uri", ZOOM_URI);
                        e
                    });
                    let zoom_node = existing_zoom.unwrap_or_else(|| {
                        let z = doc.append_element(ext, "p16:zoom");
                        doc.set_attr(z, "xmlns:p16", "http://schemas.microsoft.com/office/powerpoint/2015/main");
                        z
                    });

                    if cur_rtz {
                        doc.set_attr(zoom_node, "returnToZoom", "1");
                    } else {
                        doc.remove_attr(zoom_node, "returnToZoom");
                    }

                    if let Some(show) = cur_show_bg {
                        doc.set_attr(zoom_node, "showBg", if show { "1" } else { "0" });
                    }

                    if let Some(d) = cur_dur {
                        doc.set_attr(zoom_node, "dur", &d.to_string());
                    }
                } else if let Some(ext_lst) = ext_lst {
                    let to_remove: Vec<_> = doc.children(ext_lst).to_vec().into_iter().filter(|&c| {
                        doc.is_element(c) && doc.local_name(c) == "ext" && (
                            doc.attr(c, "uri") == Some(ZOOM_URI) ||
                            doc.descendants(c).into_iter().any(|d| doc.is_element(d) && doc.local_name(d) == "zoom")
                        )
                    }).collect();
                    for ext in to_remove {
                        doc.detach(ext);
                    }
                    if doc.elements(ext_lst).next().is_none() {
                        doc.detach(ext_lst);
                    }
                }
            }
            match hidden {
                Some(true) => doc.set_attr(node, "hidden", "1"),
                Some(false) => doc.remove_attr(node, "hidden"),
                None => {}
            }
            // 縦横比を固定: `noChangeAspect` of the shape's locks.
            if let Some(locked) = lock {
                write_aspect_lock(doc, node, *locked);
            }
            // 図形のリンク: the first child of `cNvPr`.
            if link.is_some() || tooltip.is_some() {
                for c in doc.elements(node).filter(|&c| doc.local_name(c) == "hlinkClick").collect::<Vec<_>>() {
                    doc.detach(c);
                }
                if let Some(link) = slide.shape_links.get(shape) {
                    let tip = slide.shape_link_tooltips.get(shape).map(String::as_str);
                    let x = Prefixes::at(doc, node);
                    let xml = super::pptx_links::with_link_ids(link_ids.clone(), || super::pptx_links::hlink_xml(&x, link, tip)).ok_or_else(|| unsupported("この図形のリンク"))?;
                    let element = x.import(doc, &xml)?;
                    doc.insert(node, 0, element);
                }
            }
        }
    }
    // 背景グラフィックを表示しない.
    for (slide, target) in current.slides.iter().zip(&targets) {
        let was = old_slides.get(&slide.id).map(|o| o.hide_backdrop);
        if was == Some(slide.hide_backdrop) || (was.is_none() && !slide.hide_backdrop) {
            continue;
        }
        let doc = package.xml_mut(&target.part)?;
        let root = doc.root_element().ok_or_else(|| Error::new(format!("{} が空です", target.part)))?;
        if slide.hide_backdrop {
            doc.set_attr(root, "showMasterSp", "0");
        } else {
            doc.remove_attr(root, "showMasterSp");
        }
    }
    // Slides skipped in the show.
    for (slide, target) in current.slides.iter().zip(&targets) {
        let was = old_slides.get(&slide.id).map(|o| o.hidden);
        if was == Some(slide.hidden) || (was.is_none() && !slide.hidden && target.lookup.is_none()) {
            continue;
        }
        let doc = package.xml_mut(&target.part)?;
        let root = doc.root_element().ok_or_else(|| Error::new(format!("{} が空です", target.part)))?;
        if slide.hidden {
            doc.set_attr(root, "show", "0");
        } else {
            doc.remove_attr(root, "show");
        }
    }
    // Slide name (p:cSld/@name, used for hidden slide titles / slide names).
    for (slide, target) in current.slides.iter().zip(&targets) {
        let was = old_slides.get(&slide.id).and_then(|o| o.name.as_deref());
        let now = slide.name.as_deref();
        if was == now || (was.is_none() && now.is_none() && target.lookup.is_none()) {
            continue;
        }
        let doc = package.xml_mut(&target.part)?;
        let root = doc.root_element().ok_or_else(|| Error::new(format!("{} が空です", target.part)))?;
        let csld = doc.child(root, "cSld");
        if let Some(csld) = csld {
            match now {
                Some(name) if !name.is_empty() => doc.set_attr(csld, "name", name),
                _ => doc.remove_attr(csld, "name"),
            }
        }
    }
    Ok(())
}

/// `noChangeAspect` of the locks next to `c_nv_pr` (`cNvPr` of a shape):
/// set, or taken away (an empty locks element goes too).
fn write_aspect_lock(doc: &mut XmlDocument, c_nv_pr: NodeId, locked: bool) {
    let Some(parent) = doc.parent(c_nv_pr) else { return };
    let Some(props) = doc.elements(parent).find(|&n| doc.local_name(n).starts_with("cNv") && doc.local_name(n) != "cNvPr") else { return };
    let existing = doc.elements(props).find(|&n| doc.local_name(n).ends_with("Locks"));
    match (existing, locked) {
        (Some(l), true) => doc.set_attr(l, "noChangeAspect", "1"),
        (Some(l), false) => {
            doc.remove_attr(l, "noChangeAspect");
            if doc.attributes(l).is_empty() && doc.children(l).is_empty() {
                doc.detach(l);
            }
        }
        (None, true) => {
            let name = match doc.local_name(props) {
                "cNvPicPr" => "picLocks",
                "cNvGraphicFramePr" => "graphicFrameLocks",
                "cNvCxnSpPr" => "cxnSpLocks",
                _ => "spLocks",
            };
            let prefix = doc.lookup_prefix(props, "http://schemas.openxmlformats.org/drawingml/2006/main").map(|p| format!("{p}:")).unwrap_or_default();
            let l = doc.insert_element(props, 0, &format!("{prefix}{name}"));
            doc.set_attr(l, "noChangeAspect", "1");
        }
        (None, false) => {}
    }
}

/// The shapes whose names or showing changed, by slide: (shape, the new
/// name, whether hidden now). A shape new to its slide has its name and
#[derive(Clone, Debug)]
struct ShapeLabelChange {
    shape: ShapeId,
    name: Option<String>,
    hidden: Option<bool>,
    link: Option<Option<ibis_doc_model::SlideLink>>,
    lock: Option<bool>,
    alt: Option<Option<ibis_doc_model::AltText>>,
    dec: Option<bool>,
    tooltip: Option<Option<String>>,
    return_to_zoom: Option<bool>,
    zoom_show_bg: Option<bool>,
    zoom_duration: Option<u32>,
}

/// The slide shapes that need their name, hidden state, link, aspect lock, alt
/// text, decorative state, link tooltip, or zoom settings (return-to-zoom, showBg, dur) written if they changed.
fn shape_labels(old_slides: &HashMap<SlideId, &Slide>, current: &Presentation) -> HashMap<SlideId, Vec<ShapeLabelChange>> {
    let mut out = HashMap::new();
    for slide in &current.slides {
        let old = old_slides.get(&slide.id);
        let mut changes = Vec::new();
        for shape in &slide.shapes {
            let name = slide.shape_names.get(&shape.id);
            let hidden = slide.hidden_shapes.contains(&shape.id);
            let link = slide.shape_links.get(&shape.id);
            let locked = slide.aspect_locked.contains(&shape.id);
            let alt = slide.shape_alt_texts.get(&shape.id);
            let dec = slide.decorative_shapes.contains(&shape.id);
            let tooltip = slide.shape_link_tooltips.get(&shape.id);
            let rtz = slide.shape_return_to_zoom.contains(&shape.id);
            let show_bg = slide.shape_zoom_show_bg.get(&shape.id).copied();
            let dur = slide.shape_zoom_durations.get(&shape.id).copied();
            let (name_change, hidden_change, link_change, lock_change, alt_change, dec_change, tooltip_change, rtz_change, show_bg_change, dur_change) = match old.filter(|o| o.shape(shape.id).is_some()) {
                Some(o) => (
                    (o.shape_names.get(&shape.id) != name).then(|| name.cloned().unwrap_or_default()),
                    (o.hidden_shapes.contains(&shape.id) != hidden).then_some(hidden),
                    (o.shape_links.get(&shape.id) != link).then(|| link.cloned()),
                    (o.aspect_locked.contains(&shape.id) != locked).then_some(locked),
                    (o.shape_alt_texts.get(&shape.id) != alt).then(|| alt.cloned()),
                    (o.decorative_shapes.contains(&shape.id) != dec).then_some(dec),
                    (o.shape_link_tooltips.get(&shape.id) != tooltip).then(|| tooltip.cloned()),
                    (o.shape_return_to_zoom.contains(&shape.id) != rtz).then_some(rtz),
                    (o.shape_zoom_show_bg.get(&shape.id).copied() != show_bg).then(|| show_bg.unwrap_or(true)),
                    (o.shape_zoom_durations.get(&shape.id).copied() != dur).then(|| dur.unwrap_or(1000)),
                ),
                None => (
                    name.cloned(),
                    hidden.then_some(true),
                    link.map(|l| Some(l.clone())),
                    None,
                    alt.cloned().map(Some),
                    dec.then_some(true),
                    tooltip.cloned().map(Some),
                    rtz.then_some(true),
                    show_bg,
                    dur,
                ),
            };
            if name_change.is_some() || hidden_change.is_some() || link_change.is_some() || lock_change.is_some() || alt_change.is_some() || dec_change.is_some() || tooltip_change.is_some() || rtz_change.is_some() || show_bg_change.is_some() || dur_change.is_some() {
                changes.push(ShapeLabelChange {
                    shape: shape.id,
                    name: name_change,
                    hidden: hidden_change,
                    link: link_change,
                    lock: lock_change,
                    alt: alt_change,
                    dec: dec_change,
                    tooltip: tooltip_change,
                    return_to_zoom: rtz_change,
                    zoom_show_bg: show_bg_change,
                    zoom_duration: dur_change,
                });
            }
        }
        if !changes.is_empty() {
            out.insert(slide.id, changes);
        }
    }
    out
}

/// The slides whose animations changed: those of their shapes still on
/// them, against the file's (a copied slide's original's, a new slide's
/// template's).
fn changed_animations(map: &SourceMap, baseline: &Presentation, current: &Presentation, new_slides: &HashMap<SlideId, NewSlide>) -> HashSet<SlideId> {
    let old_of = |slide: &Slide| -> Option<&Vec<ibis_doc_model::Animation>> {
        if let Some(old) = baseline.slides.iter().find(|b| b.id == slide.id) {
            return Some(&old.animations);
        }
        if let Some(t) = new_slides.get(&slide.id) {
            return Some(&t.animations);
        }
        // A copy: its original's (the part its shapes came from).
        let part = slide.shapes.iter().find_map(|s| map.shapes.get(&s.id)).map(|s| s.part.as_str())?;
        baseline.slides.iter().find(|b| map.slides.get(&b.id).is_some_and(|s| s.part == part)).map(|b| &b.animations)
    };
    current
        .slides
        .iter()
        .filter(|slide| {
            let here = |a: &&ibis_doc_model::Animation| slide.shape(a.shape).is_some();
            let old: Vec<_> = old_of(slide).map(|o| o.iter().filter(here).cloned().collect()).unwrap_or_default();
            let new: Vec<_> = slide.animations.iter().filter(here).cloned().collect();
            old != new
        })
        .map(|s| s.id)
        .collect()
}

/// The drawing ids (`cNvPr/@id`) of a slide's shapes read from the file.
fn drawing_ids(package: &Package, map: &SourceMap, slide: &Slide) -> Result<HashMap<ShapeId, String>, Error> {
    let mut out = HashMap::new();
    let mut docs: HashMap<String, XmlDocument> = HashMap::new();
    for shape in &slide.shapes {
        let Some(source) = map.shapes.get(&shape.id) else { continue };
        if !docs.contains_key(&source.part) {
            docs.insert(source.part.clone(), package.xml(&source.part)?);
        }
        let doc = &docs[&source.part];
        let Some(node) = doc.by_path(&source.path) else { continue };
        if let Some(id) = doc.descendants(node).into_iter().find(|&n| doc.local_name(n) == "cNvPr").and_then(|n| doc.attr(n, "id")) {
            out.insert(shape.id, id.to_string());
        }
    }
    Ok(out)
}

/// The drawing ids of the shapes the slides' comments are on, read from
/// the parts they came from.
fn comment_marks(package: &Package, map: &SourceMap, current: &Presentation) -> Result<HashMap<ShapeId, super::pptx_comments::ShapeMark>, Error> {
    let mut out = HashMap::new();
    for id in current.slides.iter().flat_map(|s| s.comments.iter().filter_map(|c| c.shape)) {
        let Some(source) = map.shapes.get(&id) else { continue };
        let doc = package.xml(&source.part)?;
        let Some(node) = doc.by_path(&source.path) else { continue };
        let Some(c_nv_pr) = doc.descendants(node).into_iter().find(|&n| doc.local_name(n) == "cNvPr") else { continue };
        let Some(native) = doc.attr(c_nv_pr, "id") else { continue };
        let creation = doc.descendants(c_nv_pr).into_iter().find(|&n| doc.local_name(n) == "creationId").and_then(|n| doc.attr(n, "id")).map(str::to_string);
        out.insert(id, super::pptx_comments::ShapeMark { id: native.to_string(), creation });
    }
    Ok(out)
}

/// Slide part `slide`'s number in the presentation's slide list
/// (`p:sldId/@id`) and its creation id (`p14:creationId/@val`, 0 if none).
fn slide_numbers(package: &Package, presentation: &str, slide: &str) -> Result<(u32, u32), Error> {
    let rel = package.relationships(presentation)?.into_iter().find(|r| r.kind == REL_SLIDE && r.target_part(presentation).as_deref() == Some(slide)).map(|r| r.id);
    let doc = package.xml(presentation)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{presentation} が空です")))?;
    let number = doc
        .child(root, "sldIdLst")
        .and_then(|list| doc.elements(list).find(|&e| rel.as_deref().is_some_and(|r| rel_attr(&doc, e) == Some(r))))
        .and_then(|e| doc.attr(e, "id"))
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| unsupported("スライドの一覧に無いスライドのコメント"))?;
    let sdoc = package.xml(slide)?;
    let sroot = sdoc.root_element().ok_or_else(|| Error::new(format!("{slide} が空です")))?;
    let cid = sdoc
        .child(sroot, "extLst")
        .and_then(|l| sdoc.descendants(l).into_iter().find(|&n| sdoc.local_name(n) == "creationId"))
        .and_then(|n| sdoc.attr(n, "val"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    Ok((number, cid))
}

/// Point slide `part`'s layout relationship at `slide`'s new layout.
fn set_layout(package: &mut Package, map: &SourceMap, part: &str, slide: &Slide) -> Result<(), Error> {
    let layout = slide.layout.and_then(|l| map.layouts.get(&l)).ok_or_else(|| unsupported("このレイアウトの適用"))?;
    let target = relative(part, layout);
    let rels = relationships_part(part);
    let doc = package.xml_mut(&rels)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{rels} が空です")))?;
    let rel = doc
        .children_named(root, "Relationship")
        .find(|&r| doc.attr(r, "Type") == Some(REL_SLIDE_LAYOUT))
        .ok_or_else(|| Error::new(format!("{part} のレイアウトの関係がありません")))?;
    doc.set_attr(rel, "Target", &target);
    Ok(())
}

/// Whether slide `part` has a background of its own (`p:cSld/p:bg`).
fn has_own_background(package: &Package, part: &str) -> Result<bool, Error> {
    let doc = package.xml(part)?;
    Ok(doc.root_element().and_then(|r| doc.child(r, "cSld")).and_then(|c| doc.child(c, "bg")).is_some())
}

/// Write a slide's own background (`p:cSld/p:bg/p:bgPr`), in place of the
/// one it has or inherits.
fn write_background(
    package: &mut Package,
    current: &Presentation,
    media_parts: &mut super::pptx_shapes::MediaParts,
    part: &str,
    old: &ibis_doc_model::Fill,
    fill: &ibis_doc_model::Fill,
) -> Result<(), Error> {
    if let Some(m) = fill.media().as_ref() {
        let media_p = super::pptx_shapes::media_part(package, current, *m, media_parts)?;
        let rel = package.add_relationship(part, super::pptx_shapes::REL_IMAGE, &relative(part, &media_p), false)?;
        let mut rels = HashMap::new();
        rels.insert(*m, rel);
        super::pptx_new::set_media_rels(rels);
        if current.media(*m).is_some_and(|media| media.format == ibis_doc_model::MediaFormat::Svg) {
            let mut svg_set = std::collections::HashSet::new();
            svg_set.insert(*m);
            super::pptx_new::set_svg_media(svg_set);
        }
    }
    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{part} が空です")))?;
    let c_sld = doc.child(root, "cSld").ok_or_else(|| Error::new(format!("{part} に p:cSld がありません")))?;
    let x = Prefixes::at(doc, c_sld);
    let bg = match doc.child(c_sld, "bg") {
        Some(bg) => bg,
        None => {
            let bg = x.import(doc, &format!("<{}bg/>", x.p))?;
            doc.insert(c_sld, 0, bg); bg
        }
    };
    let pr = match doc.child(bg, "bgPr") {
        Some(pr) => pr,
        None => {
            // Explicit fill replaces a theme reference, retaining bg's other content.
            for node in doc.elements(bg).filter(|&n| doc.namespace(n) == Some(new::NS_P) && doc.local_name(n) == "bgRef").collect::<Vec<_>>() { doc.detach(node); }
            let x = Prefixes::at(doc, bg);
            let pr = x.import(doc, &format!("<{p}bgPr><{a}effectLst/></{p}bgPr>", p = x.p, a = x.a))?;
            doc.insert(bg, 0, pr); pr
        }
    };
    new::check_fill(fill)?;
    if super::pptx_shapes::write_fill_properties(doc, pr, old, fill)? { return Ok(()); }
    let x = Prefixes::at(doc, pr);
    let fill = x.import(doc, &new::fill_xml(&x, fill)?)?;
    const FILLS: &[&str] = &["noFill", "solidFill", "gradFill", "blipFill", "pattFill", "grpFill"];
    for node in doc.elements(pr).filter(|&n| doc.namespace(n) == Some(new::NS_A) && FILLS.contains(&doc.local_name(n))).collect::<Vec<_>>() { doc.detach(node); }
    // A fill precedes effectLst/effectDag; attributes and all other children stay.
    doc.insert(pr, 0, fill);
    Ok(())
}

// ----- The slide list ----------------------------------------------------------

/// Add, remove and reorder slides: parts, relationships, content types,
/// `p:sldIdLst` and sections. Fills in the targets of new slides, and
/// returns where the placeholders of new slides made with them are.
fn write_slide_list<'a>(
    package: &mut Package,
    map: &SourceMap,
    baseline: &'a Presentation,
    current: &Presentation,
    old_slides: &HashMap<SlideId, &'a Slide>,
    targets: &mut [Target<'a>],
    new_slides: &'a HashMap<SlideId, NewSlide>,
) -> Result<Vec<(ShapeId, ShapeSource)>, Error> {
    let mut added = Vec::new();
    let main = map.presentation.clone();
    // New slides first, while the originals of copies are all still there.
    let mut new_rels: HashMap<SlideId, String> = HashMap::new();
    for (index, slide) in current.slides.iter().enumerate() {
        if old_slides.contains_key(&slide.id) {
            continue;
        }
        let part = next_part_name(package, "ppt/slides/slide1.xml");
        match copy_origin(map, baseline, slide)? {
            Some(original) => {
                let source = &map.slides[&original.id];
                if original.layout != slide.layout || original.background != slide.background {
                    return Err(unsupported("複製したスライドのレイアウト・背景の変更"));
                }
                copy_slide_part(package, &source.part, &part)?;
                targets[index] = Target { part: part.clone(), lookup: Some(source.part.clone()), old: &original.shapes, old_groups: &original.groups, old_layout: None };
            }
            None => {
                let template = new_slides.get(&slide.id);
                match template.and_then(|t| t.foreign.as_ref()) {
                    // Pasted from another file: its part (and, keeping its
                    // formatting, its layout and master) as it was.
                    Some(foreign) => {
                        let layout = if foreign.keep_source { None } else { Some(layout_for(map, current, index)?) };
                        super::pptx_foreign::foreign_slide_part(package, &main, layout.as_deref(), &part, foreign)?
                    }
                    None => {
                        let layout = layout_for(map, current, index)?;
                        if let Some(l) = slide.layout.and_then(|id| current.layout(id))
                            && l.background != slide.background
                        {
                            return Err(unsupported("新しいスライドの背景の変更"));
                        }
                        blank_slide_part(package, &layout, &part, template.is_some())?
                    }
                }
                targets[index] = match template {
                    // Its placeholders as written, changed as the slide is.
                    Some(t) => {
                        added.extend(t.sources.iter().map(|(id, s)| (*id, ShapeSource { part: part.clone(), ..s.clone() })));
                        Target { part: part.clone(), lookup: Some(part.clone()), old: &t.shapes, old_groups: &t.groups, old_layout: None }
                    }
                    None => Target { part: part.clone(), lookup: None, old: &[], old_groups: &[], old_layout: None },
                };
            }
        }
        package.set_override(&part, CT_SLIDE)?;
        let rel = package.add_relationship(&main, REL_SLIDE, &relative(&main, &part), false)?;
        new_rels.insert(slide.id, rel);
    }
    // Removed slides.
    let current_ids: HashSet<SlideId> = current.slides.iter().map(|s| s.id).collect();
    let mut removed_rels: Vec<String> = Vec::new();
    for slide in &baseline.slides {
        if current_ids.contains(&slide.id) {
            continue;
        }
        let source = map.slides.get(&slide.id).ok_or_else(|| unsupported("このスライドの削除"))?;
        remove_slide_parts(package, &main, source)?;
        removed_rels.push(source.rel_id.clone());
    }
    // The slide list, in the current order.
    let doc = package.xml_mut(&main)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{main} が空です")))?;
    let list = doc.child(root, "sldIdLst").ok_or_else(|| Error::new(format!("{main} にスライドの一覧がありません")))?;
    let entries: Vec<NodeId> = doc.children_named(list, "sldId").collect();
    let mut by_rel: HashMap<String, NodeId> = HashMap::new();
    let mut numbers: HashMap<NodeId, u32> = HashMap::new();
    for &e in &entries {
        if let Some(rel) = rel_attr(doc, e) {
            by_rel.insert(rel.to_string(), e);
        }
        numbers.insert(e, doc.attr(e, "id").and_then(|v| v.parse().ok()).unwrap_or(0));
    }
    let mut next_number = numbers.values().copied().max().unwrap_or(0).max(255) + 1;
    let x = Prefixes::at(doc, list);
    let element_name = entries.first().map(|&e| doc.name(e).to_string()).unwrap_or_else(|| format!("{}sldId", x.p));
    for rel in &removed_rels {
        if let Some(&e) = by_rel.get(rel) {
            doc.detach(e);
        }
    }
    let mut order: Vec<NodeId> = Vec::new();
    let mut new_order: Vec<u32> = Vec::new();
    for slide in &current.slides {
        let node = match new_rels.get(&slide.id) {
            Some(rel) => {
                let e = doc.append_element(list, &element_name);
                doc.set_attr(e, "id", &next_number.to_string());
                doc.set_attr(e, &format!("{}id", x.r), rel);
                numbers.insert(e, next_number);
                next_number += 1;
                e
            }
            None => {
                let rel = &map.slides[&slide.id].rel_id;
                *by_rel.get(rel).ok_or_else(|| Error::new(format!("{main} にスライド {rel} がありません")))?
            }
        };
        new_order.push(numbers[&node]);
        order.push(node);
    }
    for &e in &order {
        doc.detach(e);
    }
    for &e in &order {
        let index = doc.children(list).len();
        doc.insert(list, index, e);
    }
    write_sections(doc, root, &new_order)?;
    Ok(added)
}

/// The slide of the baseline a new slide is a copy of: the one its shapes
/// came from (a copy keeps the shape ids of its original).
fn copy_origin<'a>(map: &SourceMap, baseline: &'a Presentation, slide: &Slide) -> Result<Option<&'a Slide>, Error> {
    let parts: HashSet<&str> = slide.shapes.iter().filter_map(|s| map.shapes.get(&s.id)).map(|s| s.part.as_str()).collect();
    match parts.len() {
        0 => Ok(None),
        1 => {
            let part = parts.into_iter().next().expect("one part");
            Ok(baseline.slides.iter().find(|s| map.slides.get(&s.id).is_some_and(|src| src.part == part)))
        }
        _ => Err(unsupported("複数のスライドの図形を持つ新しいスライド")),
    }
}

/// The layout part a new slide at `index` follows: its own, or the one of
/// the slide before it (as PowerPoint's "New Slide"), or the first.
fn layout_for(map: &SourceMap, current: &Presentation, index: usize) -> Result<String, Error> {
    let own = current.slides[index].layout;
    let near = current.slides[..index].iter().rev().chain(current.slides[index + 1..].iter()).find_map(|s| s.layout);
    let first = current.layouts.first().map(|l| l.id);
    own.or(near).or(first).and_then(|id| map.layouts.get(&id)).cloned().ok_or_else(|| unsupported("レイアウトの無い新しいスライド"))
}

/// A new slide part following `layout`, with its placeholders empty (as
/// PowerPoint makes a new slide) when `placeholders`, else with no shapes.
fn blank_slide_part(package: &mut Package, layout: &str, part: &str, with_placeholders: bool) -> Result<(), Error> {
    let layout_doc = package.xml(layout)?;
    let mut placeholders = String::new();
    let mut id = 2;
    if let Some(tree) = layout_doc.root_element().and_then(|r| layout_doc.path(r, &["cSld", "spTree"])).filter(|_| with_placeholders) {
        for sp in layout_doc.children_named(tree, "sp") {
            let Some(ph) = layout_doc.path(sp, &["nvSpPr", "nvPr", "ph"]) else { continue };
            // Date, footer, slide number and header are the user's to add.
            if matches!(layout_doc.attr(ph, "type"), Some("dt" | "ftr" | "sldNum" | "hdr")) {
                continue;
            }
            let name = layout_doc.path(sp, &["nvSpPr", "cNvPr"]).and_then(|c| layout_doc.attr(c, "name")).unwrap_or("");
            let mut attrs = String::new();
            for key in ["type", "orient", "sz", "idx"] {
                if let Some(v) = layout_doc.attr(ph, key) {
                    attrs.push_str(&format!(" {key}=\"{}\"", new::escape(v)));
                }
            }
            placeholders.push_str(&new::placeholder_xml(id, name, &attrs));
            id += 1;
        }
    }
    let doc = XmlDocument::parse(new::slide_part_xml(&placeholders).as_bytes()).map_err(|e| Error::new(format!("新しいスライドを作れません: {e}")))?;
    package.set_xml(part, doc);
    package.add_relationship(part, REL_SLIDE_LAYOUT, &relative(part, layout), false)?;
    Ok(())
}

/// Relationship kinds whose targets belong to one source part, so a copy
/// of the source gets copies of them (the others are shared).
fn owned_kind(kind: &str) -> bool {
    let name = kind.rsplit('/').next().unwrap_or("");
    matches!(
        name,
        "chart" | "package" | "oleObject" | "chartUserShapes" | "diagramData" | "diagramLayout" | "diagramQuickStyle" | "diagramColors"
            | "diagramDrawing" | "comments" | "themeOverride" | "chartStyle" | "chartColorStyle" | "notesSlide" | "tags"
    )
}

/// Copy slide part `from` to `to`, with its relationships: notes, charts
/// and the like copied, pictures and the layout shared.
fn copy_slide_part(package: &mut Package, from: &str, to: &str) -> Result<(), Error> {
    let mut copies: HashMap<String, String> = HashMap::new();
    copies.insert(from.to_string(), to.to_string());
    copy_part(package, from, to, &mut copies)
}

/// Copy part `from` (its bytes, content type and relationships) to `to`.
/// Targets of owned relationships are copied too (once, by `copies`); a
/// relationship back to a part being copied points to its copy.
fn copy_part(package: &mut Package, from: &str, to: &str, copies: &mut HashMap<String, String>) -> Result<(), Error> {
    let bytes = package.read(from)?;
    package.set_bytes(to, bytes);
    if let Some(ct) = package.content_type(from)?
        && package.content_type(to)?.as_deref() != Some(ct.as_str())
    {
        package.set_override(to, &ct)?;
    }
    let rels = relationships_part(from);
    if !package.contains(&rels) {
        return Ok(());
    }
    let mut doc = package.xml(&rels)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{rels} が空です")))?;
    let entries: Vec<NodeId> = doc.children_named(root, "Relationship").collect();
    for r in entries {
        if doc.attr(r, "TargetMode") == Some("External") {
            continue;
        }
        let (Some(kind), Some(target)) = (doc.attr(r, "Type").map(str::to_string), doc.attr(r, "Target").map(str::to_string)) else { continue };
        let target_part = resolve(dir_of(from), &target);
        let copy = match copies.get(&target_part) {
            Some(copy) => copy.clone(),
            None if owned_kind(&kind) && package.contains(&target_part) => {
                let copy = next_part_name(package, &target_part);
                copies.insert(target_part.clone(), copy.clone());
                copy_part(package, &target_part, &copy, copies)?;
                copy
            }
            None => continue,
        };
        doc.set_attr(r, "Target", &relative(to, &copy));
    }
    package.set_xml(&relationships_part(to), doc);
    Ok(())
}

/// Remove a slide's part, its relationships part, content type, its notes
/// and the presentation's relationship to it.
fn remove_slide_parts(package: &mut Package, main: &str, source: &SlideSource) -> Result<(), Error> {
    let part = &source.part;
    let notes: Vec<String> = package
        .relationships(part)?
        .into_iter()
        .filter(|r| !r.external && r.kind.ends_with("/notesSlide"))
        .filter_map(|r| r.target_part(part))
        .collect();
    // Nothing else may point at the slide (a link from another slide).
    let rels_parts: Vec<String> = package.names().filter(|n| n.ends_with(".rels")).map(str::to_string).collect();
    for rels in rels_parts {
        let Some(owner) = owner_of(&rels) else { continue };
        if owner == main || owner == *part || notes.contains(&owner) {
            continue;
        }
        if package.relationships(&owner)?.iter().any(|r| r.target_part(&owner).as_deref() == Some(part.as_str())) {
            return Err(unsupported("ほかの所から参照されているスライドの削除"));
        }
    }
    let doc = package.xml_mut(main)?;
    if let Some(root) = doc.root_element()
        && let Some(shows) = doc.child(root, "custShowLst")
    {
        let to_remove = doc.descendants(shows).into_iter().filter(|&n| doc.is_element(n) && rel_attr(doc, n) == Some(source.rel_id.as_str())).collect::<Vec<_>>();
        for n in to_remove {
            doc.detach(n);
        }
    }
    for n in notes.iter().chain(std::iter::once(part)) {
        package.remove(n);
        package.remove(&relationships_part(n));
        package.remove_override(n)?;
    }
    package.remove_relationship(main, &source.rel_id)?;
    Ok(())
}

/// Keep the sections (`p14:sectionLst`) in step with the slide list: each
/// slide stays in its section unless the new order moves it past a later
/// section; a new slide goes into the section of the slide before it.
fn write_sections(doc: &mut XmlDocument, root: NodeId, new_order: &[u32]) -> Result<(), Error> {
    const NS_S:&str="http://schemas.microsoft.com/office/powerpoint/2010/main";
    const URI:&str="{521415D9-36F7-43E2-AB2F-B90AF26B5E84}";
    let extensions=doc.elements(root).filter(|&n|doc.namespace(n)==Some(new::NS_P)&&doc.local_name(n)=="extLst").collect::<Vec<_>>();
    if extensions.len()>1 {return Err(unsupported("複数のpresentation extLst"));}
    let Some(&ext)=extensions.first() else {return Ok(())};
    let lists=doc.elements(ext).filter(|&n|doc.namespace(n)==Some(new::NS_P)&&doc.local_name(n)=="ext"&&doc.attr(n,"uri")==Some(URI)).flat_map(|n|doc.elements(n)).filter(|&n|doc.namespace(n)==Some(NS_S)&&doc.local_name(n)=="sectionLst").collect::<Vec<_>>();
    if lists.len()>1 {return Err(unsupported("複数のsectionLst"));}
    let Some(&list)=lists.first() else {return Ok(())};
    let sections:Vec<NodeId>=doc.elements(list).filter(|&n|doc.namespace(n)==Some(NS_S)&&doc.local_name(n)=="section").collect();
    if sections.is_empty() {return Ok(());}
    let prefix_of = |doc: &XmlDocument, n: NodeId| doc.prefix(n).map(|p| format!("{p}:")).unwrap_or_default();
    let mut member: HashMap<u32, usize> = HashMap::new();
    let mut nodes: HashMap<u32, NodeId> = HashMap::new();
    let mut bindings:HashMap<NodeId,Vec<(Option<String>,String)>>=HashMap::new();
    let mut lists: Vec<Option<NodeId>> = Vec::new();
    let mut element_name = None;
    for (i, &section) in sections.iter().enumerate() {
        let candidates=doc.elements(section).filter(|&n|doc.namespace(n)==Some(NS_S)&&doc.local_name(n)=="sldIdLst").collect::<Vec<_>>();
        if candidates.len()>1 {return Err(unsupported("複数のsection sldIdLst"));}
        let ids=candidates.first().copied();
        lists.push(ids);
        let Some(ids) = ids else { continue };
        for e in doc.elements(ids).filter(|&n|doc.namespace(n)==Some(NS_S)&&doc.local_name(n)=="sldId").collect::<Vec<_>>() {
            element_name.get_or_insert_with(|| doc.name(e).to_string());
            if let Some(n) = doc.attr(e, "id").and_then(|v| v.parse().ok()) {
                if member.insert(n,i).is_some() {return Err(unsupported("セクション内の重複スライドID"));}
                nodes.insert(n, e);
            }
            // Moving an ID into another section must not rebind element or
            // opaque attribute/child prefixes inherited from its old section.
            let mut prefixes=std::collections::BTreeSet::new();
            for node in std::iter::once(e).chain(doc.descendants(e)) {
                if !doc.is_element(node) {continue;}
                prefixes.insert(doc.prefix(node).map(str::to_owned));
                for attr in doc.attributes(node) {
                    if let Some((prefix,_))=attr.name.split_once(':') {if prefix!="xmlns" {prefixes.insert(Some(prefix.to_owned()));}}
                }
            }
            bindings.insert(e,prefixes.into_iter().filter_map(|prefix|doc.resolve_prefix(e,prefix.as_deref()).map(|ns|(prefix,ns.to_owned()))).collect());
            doc.detach(e);
        }
    }
    let element_name = element_name.unwrap_or_else(|| format!("{}sldId", prefix_of(doc, list)));
    let mut at = 0usize;
    for &n in new_order {
        if let Some(&s) = member.get(&n) {
            at = at.max(s);
        }
        let ids = match lists[at] {
            Some(ids) => ids,
            None => {
                let name = format!("{}sldIdLst", prefix_of(doc, sections[at]));
                let ids = doc.insert_element(sections[at], 0, &name);
                lists[at] = Some(ids);
                ids
            }
        };
        let e = match nodes.get(&n) {
            Some(&e) => e,
            None => {
                let e = doc.append_element(ids, &element_name);
                doc.detach(e);
                doc.set_attr(e, "id", &n.to_string());
                e
            }
        };
        let index = doc.children(ids).len();
        doc.insert(ids, index, e);
        let required=bindings.get(&e).cloned().unwrap_or_else(||vec![(doc.prefix(e).map(str::to_owned),NS_S.to_owned())]);
        for (prefix,namespace) in required {
            if doc.resolve_prefix(e,prefix.as_deref())!=Some(namespace.as_str()) {
                let attr=prefix.map(|p|format!("xmlns:{p}")).unwrap_or_else(||"xmlns".to_owned());
                doc.set_attr(e,&attr,&namespace);
            }
        }
    }
    Ok(())
}

// ----- Part names ----------------------------------------------------------------

/// An unused part name like `like`: the same directory, stem (without its
/// number) and extension, numbered from 1.
pub(crate) fn next_part_name(package: &Package, like: &str) -> String {
    let (dir, file) = like.rsplit_once('/').unwrap_or(("", like));
    let (stem, ext) = file.rsplit_once('.').unwrap_or((file, ""));
    let stem = stem.trim_end_matches(|c: char| c.is_ascii_digit());
    (1..)
        .map(|n| {
            let name = if ext.is_empty() { format!("{stem}{n}") } else { format!("{stem}{n}.{ext}") };
            if dir.is_empty() { name } else { format!("{dir}/{name}") }
        })
        .find(|name| !package.names().any(|existing| existing.eq_ignore_ascii_case(name)))
        .expect("an unused name")
}

fn dir_of(part: &str) -> &str {
    part.rfind('/').map_or("", |i| &part[..i])
}

/// The part a relationships part belongs to (`ppt/slides/_rels/slide1.xml.rels`
/// → `ppt/slides/slide1.xml`; the package's own is "").
fn owner_of(rels: &str) -> Option<String> {
    let (dir, file) = rels.rsplit_once('/').unwrap_or(("", rels));
    let file = file.strip_suffix(".rels")?;
    let dir = dir.strip_suffix("_rels")?.trim_end_matches('/');
    Some(if dir.is_empty() { file.to_string() } else { format!("{dir}/{file}") })
}

/// `to` relative to the directory of part `from`.
pub(crate) fn relative(from: &str, to: &str) -> String {
    let from_dir: Vec<&str> = dir_of(from).split('/').filter(|s| !s.is_empty()).collect();
    let to_parts: Vec<&str> = to.split('/').collect();
    let common = from_dir.iter().zip(&to_parts).take_while(|(a, b)| a == b).count().min(to_parts.len() - 1);
    let mut out: Vec<&str> = vec![".."; from_dir.len() - common];
    out.extend(&to_parts[common..]);
    out.join("/")
}

/// The value of the `r:id` attribute of `node` (by namespace, whatever the prefix).
pub(crate) fn rel_attr<'a>(doc: &'a XmlDocument, node: NodeId) -> Option<&'a str> {
    doc.attributes(node)
        .iter()
        .find(|a| a.name.split_once(':').is_some_and(|(p, l)| l == "id" && p != "xmlns" && doc.resolve_prefix(node, Some(p)) == Some(NS_R)))
        .map(|a| a.value.as_str())
}

/// Write or update the list of custom shows (`p:custShowLst`).
fn write_custom_shows(package: &mut Package, presentation_part: &str, custom_shows: &[ibis_doc_model::CustomShow], map: &SourceMap) -> Result<(), Error> {
    let doc = package.xml_mut(presentation_part)?;
    let root = doc.root_element().ok_or_else(|| Error::new("プレゼンテーションが空です"))?;
    let x = Prefixes::at(doc, root);

    if let Some(existing) = doc.child(root, "custShowLst") {
        doc.detach(existing);
    }

    if custom_shows.is_empty() {
        return Ok(());
    }

    let mut xml = format!("<{}custShowLst>", x.p);
    for show in custom_shows {
        xml.push_str(&format!("<{}custShow name=\"{}\" id=\"{}\"><{}sldLst>", x.p, new::escape(&show.name), show.id, x.p));
        for &slide_id in &show.slides {
            if let Some(source) = map.slides.get(&slide_id) {
                xml.push_str(&format!("<{}sld {}id=\"{}\"/>", x.p, x.r, source.rel_id));
            }
        }
        xml.push_str(&format!("</{}sldLst></{}custShow>", x.p, x.p));
    }
    xml.push_str(&format!("</{}custShowLst>", x.p));

    let node = x.import(doc, &xml)?;

    let next = doc.elements(root).find(|&n| matches!(doc.local_name(n), "photoAlbum" | "custDataLst" | "kinsoku" | "defaultTextStyle" | "modifyVerifier" | "extLst"));
    if let Some(next) = next {
        doc.insert_before(next, node);
    } else {
        doc.insert(root, doc.children(root).len(), node);
    }

    Ok(())
}

/// Write or update `<p:photoAlbum>` of the presentation.
fn write_photo_album(package: &mut Package, presentation_part: &str, album: Option<&ibis_doc_model::PhotoAlbum>) -> Result<(), Error> {
    let doc = package.xml_mut(presentation_part)?;
    let root = doc.root_element().ok_or_else(|| Error::new("プレゼンテーションが空です"))?;
    for old in doc.children(root).to_vec() {
        if doc.is_element(old) && doc.local_name(old) == "photoAlbum" {
            doc.detach(old);
        }
    }
    let Some(album) = album else { return Ok(()); };
    let x = Prefixes::at(doc, root);
    let mut xml = format!(r#"<{}photoAlbum"#, x.p);
    if album.bw {
        xml.push_str(r#" bw="1""#);
    }
    if album.show_captions {
        xml.push_str(r#" showCaptions="1""#);
    }
    xml.push_str(&format!(r#" layout="{}" frame="{}"/>"#, album.layout.as_str(), album.frame.as_str()));
    let node = x.import(doc, &xml)?;
    let next = doc.elements(root).find(|&n| matches!(doc.local_name(n), "custDataLst" | "kinsoku" | "defaultTextStyle" | "modifyVerifier" | "extLst"));
    if let Some(next) = next {
        doc.insert_before(next, node);
    } else {
        doc.insert(root, doc.children(root).len(), node);
    }
    Ok(())
}

/// `p:modifyVerifier` of the presentation as `verifier` says (none takes it away): after the
/// default text style, before the extension list.
fn write_modify_verifier(package: &mut Package, map: &SourceMap, verifier: Option<&ibis_doc_model::ModifyVerifier>) -> Result<(), Error> {
    let doc = package.xml_mut(&map.presentation)?;
    let root = doc.root_element().ok_or_else(|| Error::new("プレゼンテーションが空です"))?;
    for old in doc.children(root).to_vec() {
        if doc.is_element(old) && doc.local_name(old) == "modifyVerifier" {
            doc.detach(old);
        }
    }
    let Some(v) = verifier else { return Ok(()) };
    let x = Prefixes::at(doc, root);
    let xml = format!(
        "<{p}modifyVerifier cryptProviderType=\"rsaAES\" cryptAlgorithmClass=\"hash\" cryptAlgorithmType=\"typeAny\" cryptAlgorithmSid=\"{}\" spinCount=\"{}\" saltData=\"{}\" hashData=\"{}\"/>",
        v.algorithm_sid,
        v.spin_count,
        v.salt,
        v.hash,
        p = x.p
    );
    let node = x.import(doc, &xml)?;
    let at = doc.children(root).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "extLst").unwrap_or(doc.children(root).len());
    doc.insert(root, at, node);
    Ok(())
}

fn write_read_only_recommended(package: &mut Package, map: &SourceMap, recommended: bool) -> Result<(), Error> {
    let doc = package.xml_mut(&map.presentation)?;
    let root = doc.root_element().ok_or_else(|| Error::new("プレゼンテーションが空です"))?;
    let x = Prefixes::at(doc, root);
    let ext_lst = match doc.child(root, "extLst") {
        Some(e) => e,
        None => {
            if !recommended {
                return Ok(());
            }
            let node = x.import(doc, &format!("<{}extLst/>", x.p))?;
            doc.insert(root, doc.children(root).len(), node);
            node
        }
    };

    let mut found_ext = None;
    for ext in doc.elements(ext_lst).filter(|&n| doc.local_name(n) == "ext") {
        let uri = doc.attr(ext, "uri");
        let has_node = doc.elements(ext).any(|c| doc.local_name(c) == "readonlyRecommended");
        if uri == Some(URI_READONLY_RECOMMENDED) || has_node {
            found_ext = Some(ext);
            break;
        }
    }

    if recommended {
        if let Some(ext) = found_ext {
            let rr = doc.elements(ext).find(|&c| doc.local_name(c) == "readonlyRecommended");
            if let Some(rr) = rr {
                doc.set_attr(rr, "val", "1");
            } else {
                let node = x.import(doc, &format!("<p1710:readonlyRecommended xmlns:p1710=\"{}\" val=\"1\"/>", NS_P1710))?;
                doc.insert(ext, doc.children(ext).len(), node);
            }
        } else {
            let ext_xml = format!(
                r#"<{}ext uri="{}"><p1710:readonlyRecommended xmlns:p1710="{}" val="1"/></{}ext>"#,
                x.p, URI_READONLY_RECOMMENDED, NS_P1710, x.p
            );
            let ext_node = x.import(doc, &ext_xml)?;
            doc.insert(ext_lst, doc.children(ext_lst).len(), ext_node);
        }
    } else if let Some(ext) = found_ext {
        doc.detach(ext);
    }
    Ok(())
}

fn write_embedded_fonts(package: &mut Package, presentation_part: &str, fonts: &[ibis_doc_model::EmbeddedFont]) -> Result<(), Error> {
    // 1. Collect all relationship IDs from any existing embeddedFontLst in doc, and detach it.
    let (old_rids, x) = {
        let doc = package.xml_mut(presentation_part)?;
        let root = doc.root_element().ok_or_else(|| Error::new("プレゼンテーションが空です"))?;
        let x = Prefixes::at(doc, root);
        let mut old_rids: Vec<String> = Vec::new();
        let existing_list = doc.elements(root).find(|&n| doc.local_name(n) == "embeddedFontLst");
        if let Some(list) = existing_list {
            let entries: Vec<_> = doc.elements(list).filter(|&n| doc.local_name(n) == "embeddedFont").collect();
            for entry in entries {
                let children: Vec<_> = doc.elements(entry).collect();
                for child in children {
                    if let Some(rid) = doc.attr(child, "id").or_else(|| doc.attr(child, "r:id")) {
                        old_rids.push(rid.to_string());
                    }
                }
            }
            doc.detach(list);
        }
        (old_rids, x)
    };

    // 2. Remove old font relationships and parts in package.
    for rid in old_rids {
        if let Ok(rels) = package.relationships(presentation_part) {
            if let Some(rel) = rels.into_iter().find(|r| r.id == rid) {
                let target_part = if rel.target.starts_with('/') {
                    rel.target.trim_start_matches('/').to_string()
                } else {
                    format!("ppt/{}", rel.target.trim_start_matches('/'))
                };
                let _ = package.remove_override(&target_part);
                package.remove(&target_part);
            }
        }
        let _ = package.remove_relationship(presentation_part, &rid);
    }

    if fonts.is_empty() {
        return Ok(());
    }

    // 3. Group fonts by typeface.
    let mut grouped: std::collections::BTreeMap<&str, Vec<&ibis_doc_model::EmbeddedFont>> = std::collections::BTreeMap::new();
    for f in fonts {
        grouped.entry(&f.typeface).or_default().push(f);
    }

    // 4. For each font entry, write font file, add override, add relationship, build XML.
    let mut font_idx = 1usize;
    for name in package.names() {
        if let Some(rest) = name.strip_prefix("ppt/fonts/font") {
            if let Some(num_str) = rest.strip_suffix(".fntdata") {
                if let Ok(n) = num_str.parse::<usize>() {
                    font_idx = font_idx.max(n + 1);
                }
            }
        }
    }

    let mut xml = format!("<{}embeddedFontLst>", x.p);
    for (typeface, variants) in grouped {
        xml.push_str(&format!(r#"<{}embeddedFont><{}font typeface="{}"/>"#, x.p, x.p, super::pptx_new::escape_attr(typeface)));
        for v in variants {
            let part_name = format!("ppt/fonts/font{font_idx}.fntdata");
            let rel_target = format!("fonts/font{font_idx}.fntdata");
            font_idx += 1;

            package.set_bytes(&part_name, v.data.clone());
            let _ = package.set_override(&part_name, "application/x-fontdata");
            let rid = package.add_relationship(presentation_part, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/font", &rel_target, false)?;

            let tag = match (v.bold, v.italic) {
                (false, false) => "regular",
                (true, false) => "bold",
                (false, true) => "italic",
                (true, true) => "boldItalic",
            };
            xml.push_str(&format!(r#"<{p}{tag} {r}id="{rid}"/>"#, p = x.p, tag = tag, r = x.r, rid = rid));
        }
        xml.push_str(&format!("</{}embeddedFont>", x.p));
    }
    xml.push_str(&format!("</{}embeddedFontLst>", x.p));

    // 5. Re-borrow doc to import and insert the new embeddedFontLst node.
    let doc = package.xml_mut(presentation_part)?;
    let root = doc.root_element().ok_or_else(|| Error::new("プレゼンテーションが空です"))?;
    let node = x.import(doc, &xml)?;
    let next = doc.elements(root).find(|&n| matches!(doc.local_name(n), "custShowLst" | "photoAlbum" | "custDataLst" | "kinsoku" | "defaultTextStyle" | "modifyVerifier" | "extLst"));
    if let Some(next) = next {
        doc.insert_before(next, node);
    } else {
        doc.insert(root, doc.children(root).len(), node);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slide_size_without_original_node_preserves_child_order_and_notes() {
        let data = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/pptx-plan.pptx")).unwrap();
        let mut package = Package::open(data).unwrap();
        let map = SourceMap { presentation: "ppt/presentation.xml".into(), ..Default::default() };
        let doc = package.xml_mut(&map.presentation).unwrap();
        let root = doc.root_element().unwrap();
        let node = doc.child(root, "sldSz").unwrap();
        doc.detach(node);
        let notes = doc.child(root, "notesSz").map(|n| doc.attr(n, "cx").unwrap().to_owned());
        write_slide_size(&mut package, &map, ibis_doc_model::EmuSize::WIDESCREEN).unwrap();
        let doc = package.xml_mut(&map.presentation).unwrap();
        let node = doc.child(root, "sldSz").unwrap();
        assert_eq!(doc.attr(node, "type"), Some("screen16x9"));
        let children = doc.elements(root).map(|n| doc.local_name(n).to_owned()).collect::<Vec<_>>();
        assert!(children.iter().position(|n| n == "sldSz").unwrap() < children.iter().position(|n| n == "notesSz").unwrap());
        assert_eq!(doc.child(root, "notesSz").map(|n| doc.attr(n, "cx").unwrap().to_owned()), notes);
    }

    #[test]
    fn relative_targets() {
        assert_eq!(relative("ppt/presentation.xml", "ppt/slides/slide3.xml"), "slides/slide3.xml");
        assert_eq!(relative("ppt/slides/slide3.xml", "ppt/slideLayouts/slideLayout2.xml"), "../slideLayouts/slideLayout2.xml");
        assert_eq!(relative("ppt/slides/slide3.xml", "ppt/slides/slide4.xml"), "slide4.xml");
        assert_eq!(relative("ppt/slides/slide3.xml", "ppt/media/image1.png"), "../media/image1.png");
    }

    #[test]
    fn owners_of_relationship_parts() {
        assert_eq!(owner_of("ppt/slides/_rels/slide1.xml.rels").as_deref(), Some("ppt/slides/slide1.xml"));
        assert_eq!(owner_of("_rels/.rels").as_deref(), Some(""));
    }
}

