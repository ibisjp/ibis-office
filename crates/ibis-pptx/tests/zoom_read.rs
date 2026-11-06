// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! PowerPoint's ズーム objects (`pslz:sldZm`, `psez:sectionZm`, `psuz:summaryZm`) read from a file PowerPoint for Mac wrote
//! (`test/zoom-sample/office-zoom.pptx`, not published: the test is skipped without it): their thumbnails are drawn, they
//! keep their properties, and saving the file untouched keeps them.
use ibis_doc_model::{SlideLink, ZoomTarget};

fn sample() -> Option<Vec<u8>> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/zoom-sample/office-zoom.pptx")).ok()
}

#[test]
fn zoom_objects_are_read() {
    let Some(data) = sample() else { return };
    let p = ibis_pptx::read(&data).unwrap();
    assert_eq!(p.slides.len(), 7);
    // The summary slide: one card for each of the three sections, in the order of the file.
    let summary = &p.slides[0];
    let mut cards: Vec<_> = summary.zoom_objects.values().collect();
    cards.sort_by_key(|z| match &z.target { ZoomTarget::Summary { index, .. } => *index, _ => 99 });
    assert_eq!(cards.len(), 3);
    let section_ids: Vec<&str> = p.sections.iter().map(|s| s.id.as_str()).collect();
    for (i, z) in cards.iter().enumerate() {
        let ZoomTarget::Summary { section_id, index } = &z.target else { panic!("{z:?}") };
        assert_eq!(*index, i);
        assert!(section_ids.contains(&section_id.as_str()), "{section_id} in {section_ids:?}");
        assert!(z.return_to_parent && z.show_bg && z.transition_dur == Some(1000));
    }
    // Each card draws its thumbnail and goes to the first slide of its section.
    assert_eq!(summary.shapes.iter().filter(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Picture(_))).count(), 3);
    let linked: Vec<_> = summary.shapes.iter().filter_map(|s| summary.shape_links.get(&s.id)).collect();
    assert_eq!(linked.len(), 3);
    // A slide zoom: the slide it names, the properties of `zmPr` (PowerPoint writes returnToParent off by default).
    let on_slide_1 = &p.slides[1];
    let (id, z) = on_slide_1.zoom_objects.iter().next().expect("a slide zoom");
    let ZoomTarget::Slide { slide, creation_id } = &z.target else { panic!("{z:?}") };
    assert_eq!(*slide, p.slides[3].id);
    assert_eq!(*creation_id, Some(337292935));
    assert!(!z.return_to_parent && z.transition_dur == Some(1000));
    assert_eq!(on_slide_1.shape_links.get(id), Some(&SlideLink::Slide(p.slides[3].id)));
    assert!(!on_slide_1.shape_return_to_zoom.contains(id));
    // A section zoom: the section by its GUID, returning by default.
    let section_zoom = p.slides[3].zoom_objects.values().next().expect("a section zoom");
    let ZoomTarget::Section { section_id } = &section_zoom.target else { panic!("{section_zoom:?}") };
    assert_eq!(section_id, "{48B2D15C-DE74-43AA-BC39-8E9A1345FC97}");
    assert!(section_zoom.return_to_parent);
}

#[test]
fn zoom_objects_survive_an_untouched_save() {
    let Some(data) = sample() else { return };
    let before = ibis_pptx::read(&data).unwrap();
    let opened = ibis_pptx::open(data).unwrap();
    let saved = opened.source.save(&opened.presentation, ibis_pptx::SaveOptions::default()).unwrap();
    let after = ibis_pptx::read(&saved).unwrap();
    for (a, b) in before.slides.iter().zip(&after.slides) {
        let kinds = |s: &ibis_doc_model::Slide| {
            let mut v: Vec<_> = s.zoom_objects.values().map(|z| (z.target.clone(), z.id.clone(), z.return_to_parent, z.transition_dur, z.show_bg)).collect();
            v.sort_by_key(|z| format!("{:?}", z.0));
            v
        };
        let ids = |s: &ibis_doc_model::Slide| s.zoom_objects.len();
        assert_eq!(ids(a), ids(b));
        assert_eq!(a.zoom_objects.len(), b.zoom_objects.len());
        let _ = kinds;
    }
}

/// A zoom card moved: the file PowerPoint opens (the `mc:Choice` frame it uses, and the fallback picture older versions
/// draw) places it in the new position; a card left alone stays as it was.
#[test]
fn a_moved_zoom_card_is_moved_in_the_choice_and_the_fallback() {
    use ibis_doc_model::{Command, Document, EmuRect};
    let Some(data) = sample() else { return };
    let opened = ibis_pptx::open(data).unwrap();
    let mut doc = Document::new(opened.presentation.clone());
    let slide = opened.presentation.slides[1].id;
    let shape = *opened.presentation.slides[1].zoom_objects.keys().next().unwrap();
    let old = opened.presentation.slides[1].shape(shape).unwrap().frame;
    let moved = EmuRect::new(old.x + 100_000, old.y + 50_000, old.width, old.height);
    doc.execute(vec![Command::SetFrame { slide, shape, frame: moved }]).unwrap();
    let saved = opened.source.save(doc.presentation(), ibis_pptx::SaveOptions::default()).unwrap();
    if let Ok(out) = std::env::var("ZOOM_TEST_OUT") {
        std::fs::write(out, &saved).unwrap();
    }
    let after = ibis_pptx::read(&saved).unwrap();
    // Still a zoom of the same slide, in the new place.
    let (id, z) = after.slides[1].zoom_objects.iter().next().expect("the zoom is still a zoom");
    assert!(matches!(z.target, ibis_doc_model::ZoomTarget::Slide { creation_id: Some(337292935), .. }));
    assert_eq!(after.slides[1].shape(*id).unwrap().frame, moved);
}

/// 戻る (returnToParent) turned on for a slide zoom is written as the attribute of the object's `zmPr`, not as an extension of
/// ibisOffice's own.
#[test]
fn return_to_parent_is_written_in_zm_pr() {
    use ibis_doc_model::{Command, Document};
    let Some(data) = sample() else { return };
    let opened = ibis_pptx::open(data).unwrap();
    let mut doc = Document::new(opened.presentation.clone());
    let slide = opened.presentation.slides[1].id;
    let shape = *opened.presentation.slides[1].zoom_objects.keys().next().unwrap();
    doc.execute(vec![Command::SetShapeReturnToZoom { slide, shape, return_to_zoom: true }]).unwrap();
    let saved = opened.source.save(doc.presentation(), ibis_pptx::SaveOptions::default()).unwrap();
    let after = ibis_pptx::read(&saved).unwrap();
    let z = after.slides[1].zoom_objects.values().next().unwrap();
    assert!(z.return_to_parent);
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let xml = String::from_utf8(package.read("ppt/slides/slide2.xml").unwrap().to_vec()).unwrap();
    assert!(xml.contains("returnToParent=\"1\""), "zmPr");
    assert!(!xml.contains("D3508630-5900-4276-8680-4D969FE36771"), "no private extension");
}

fn a_png(data: &[u8]) -> Vec<u8> {
    let package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let name = package.names().find(|n| n.ends_with(".png")).expect("a picture in the file").to_string();
    package.read(&name).unwrap().to_vec()
}

/// The names of the elements under `node`, in order, with the namespaces (the structure, not the values).
fn shape_of(node: roxmltree::Node, out: &mut Vec<String>) {
    for n in node.children().filter(|n| n.is_element()) {
        // The extensions PowerPoint adds (creation ids, the modification id) are not the zoom's structure.
        if matches!(n.tag_name().name(), "extLst") {
            continue;
        }
        // (A named colour and an RGB one are the same structure.)
        out.push(match n.tag_name().name() { "prstClr" => "srgbClr", other => other }.to_string());
        shape_of(n, out);
    }
}

/// The structure of the first `mc:AlternateContent` of slide part `slide_part` whose frame holds `uri`.
fn zoom_xml(saved: &[u8], slide_part: &str, uri: &str) -> String {
    let package = ibis_ooxml::Package::open(saved.to_vec()).unwrap();
    let xml = String::from_utf8(package.read(slide_part).unwrap().to_vec()).unwrap();
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let alt = doc
        .descendants()
        .find(|n| n.tag_name().name() == "AlternateContent" && n.descendants().any(|d| d.tag_name().name() == "graphicData" && d.attribute("uri") == Some(uri)))
        .unwrap_or_else(|| panic!("a zoom of {uri} in {slide_part}"));
    let mut out = Vec::new();
    shape_of(alt, &mut out);
    out.join(" ")
}

/// Cards the editor makes (a picture with its link and its `ZoomObject`) are written as PowerPoint writes them: the structure of
/// the written `mc:AlternateContent` is the structure of the one in PowerPoint's own file, and they read back as zooms.
#[test]
fn new_zoom_cards_are_written_as_powerpoint_writes_them() {
    use ibis_doc_model::{Command, Document, EmuRect, MediaFormat, Picture, Shape, ShapeKind, SlideLink, ZoomObject};
    let Some(data) = sample() else { return };
    let png = a_png(&data);
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let mut doc = Document::new(opened.presentation.clone());
    let slides: Vec<_> = opened.presentation.slides.iter().map(|s| s.id).collect();
    let on = slides[2]; // an ordinary slide (the second of 導入)
    let sections: Vec<String> = opened.presentation.sections.iter().map(|s| s.id.clone()).collect();
    let media = doc.ids().add_media(MediaFormat::Png, png.clone());
    let mut commands = Vec::new();
    let mut card = |doc: &mut Document, frame: EmuRect, link: Option<SlideLink>, zoom: ZoomObject| {
        let id = doc.ids().new_shape_id();
        let mut picture = Picture::new(media);
        // The outline PowerPoint draws round a zoom card (`a:ln w="3175"`, light gray).
        picture.stroke = Some(ibis_doc_model::Stroke::solid(ibis_doc_model::Color::opaque(ibis_doc_model::Rgb(0xD3D3D3)), 3175));
        let shape = Shape::new(id, frame, ShapeKind::Picture(picture));
        let index = opened.presentation.slide(on).unwrap().shapes.len() + commands.iter().filter(|c| matches!(c, Command::InsertShape { .. })).count();
        commands.push(Command::InsertShape { slide: on, index, shape });
        if let Some(link) = link {
            commands.push(Command::SetShapeLink { slide: on, shape: id, link: Some(link) });
        }
        commands.push(Command::SetZoomObject { slide: on, shape: id, zoom: Some(zoom) });
        id
    };
    let mk = |target, id: &str| ZoomObject { target, id: id.into(), return_to_parent: false, transition_dur: Some(1000), show_bg: true, cover_image: false };
    let slide_card = card(&mut doc, EmuRect::new(1_000_000, 3_000_000, 2_400_000, 1_350_000), Some(SlideLink::Slide(slides[5])), mk(ibis_doc_model::ZoomTarget::Slide { slide: slides[5], creation_id: None }, "{11111111-1111-1111-1111-111111111111}"));
    let section_card = card(&mut doc, EmuRect::new(4_000_000, 3_000_000, 2_400_000, 1_350_000), Some(SlideLink::Slide(slides[5])), mk(ibis_doc_model::ZoomTarget::Section { section_id: sections[3].clone() }, "{22222222-2222-2222-2222-222222222222}"));
    for (i, s) in sections[1..].iter().enumerate() {
        card(&mut doc, EmuRect::new(1_000_000 + i as i64 * 2_500_000, 500_000, 2_400_000, 1_350_000), Some(SlideLink::Slide(slides[1 + i * 2])), mk(ibis_doc_model::ZoomTarget::Summary { section_id: s.clone(), index: i }, &format!("{{3333333{i}-3333-3333-3333-333333333333}}")));
    }
    doc.execute(commands).unwrap();
    let saved = opened.source.save(doc.presentation(), ibis_pptx::SaveOptions::default()).unwrap();
    if let Ok(out) = std::env::var("ZOOM_TEST_OUT") {
        std::fs::write(out, &saved).unwrap();
    }
    let slide_uri = "http://schemas.microsoft.com/office/powerpoint/2016/slidezoom";
    let section_uri = "http://schemas.microsoft.com/office/powerpoint/2016/sectionzoom";
    let summary_uri = "http://schemas.microsoft.com/office/powerpoint/2016/summaryzoom";
    // The slide part the new zooms were written to.
    let part = {
        let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
        package.names().filter(|n| n.starts_with("ppt/slides/slide") && n.ends_with(".xml")).find(|n| String::from_utf8_lossy(&package.read(n).unwrap()).contains("22222222-2222-2222-2222-222222222222")).expect("the slide with the new zooms").to_string()
    };
    // The structure of each is the structure of PowerPoint's own: the frame's data, and the picture (or group of pictures) it falls back to.
    assert_eq!(zoom_xml(&saved, &part, section_uri), zoom_xml(&data, "ppt/slides/slide4.xml", section_uri));
    assert_eq!(zoom_xml(&saved, &part, summary_uri), zoom_xml(&data, "ppt/slides/slide1.xml", summary_uri));
    // (The first zoom of the kind in the part: the slide zoom of the file's own is before the new one, and is the same structure.)
    assert_eq!(zoom_xml(&saved, &part, slide_uri), zoom_xml(&data, "ppt/slides/slide2.xml", slide_uri));
    // Read again, they are zooms with the targets the editor gave.
    let after = ibis_pptx::read(&saved).unwrap();
    let slide = after.slides.iter().find(|s| s.zoom_objects.values().any(|z| z.id == "{11111111-1111-1111-1111-111111111111}")).unwrap();
    // (The slide already had a slide zoom of its own.)
    assert_eq!(slide.zoom_objects.len(), 2 + 1 + 3);
    let kinds: Vec<_> = slide.zoom_objects.values().map(|z| match &z.target { ibis_doc_model::ZoomTarget::Slide { .. } => 0, ibis_doc_model::ZoomTarget::Section { .. } => 1, _ => 2 }).collect();
    assert_eq!((kinds.iter().filter(|k| **k == 0).count(), kinds.iter().filter(|k| **k == 1).count(), kinds.iter().filter(|k| **k == 2).count()), (2, 1, 3));
    let slide_zoom = slide.zoom_objects.values().find(|z| z.id == "{11111111-1111-1111-1111-111111111111}").unwrap();
    let ibis_doc_model::ZoomTarget::Slide { slide: to, creation_id } = &slide_zoom.target else { unreachable!() };
    assert_eq!(*to, after.slides[5].id);
    assert!(creation_id.is_some());
    let _ = (slide_card, section_card);
}
