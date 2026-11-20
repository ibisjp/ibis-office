// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! An old presentation (.ppt) is read as the presentation made from it: the same slides, texts and sizes as the
//! .pptx it was saved from (test/editing/pptx-legacy.ppt is pptx-plan.pptx saved as PowerPoint 97–2003).

use ibis_doc_model::{Presentation, ShapeKind};

fn path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name)
}

fn texts(p: &Presentation) -> Vec<Vec<String>> {
    p.slides
        .iter()
        .map(|s| {
            let mut v: Vec<String> = s.shapes.iter().filter_map(|sh| sh.text_body().map(|b| b.text())).filter(|t| !t.is_empty()).collect();
            v.sort();
            v
        })
        .collect()
}

#[test]
fn a_legacy_presentation_is_read_with_its_slides_and_texts() {
    let legacy = ibis_formats::read(&std::fs::read(path("pptx-legacy.ppt")).unwrap()).unwrap();
    let original = ibis_formats::read(&std::fs::read(path("pptx-plan.pptx")).unwrap()).unwrap();
    assert_eq!(legacy.slides.len(), original.slides.len());
    assert_eq!((legacy.slide_size.width, legacy.slide_size.height), (original.slide_size.width, original.slide_size.height));
    assert_eq!(texts(&legacy), texts(&original));
    // The notes and the transitions.
    let notes = |p: &Presentation| p.slides.iter().map(|s| s.notes.clone()).collect::<Vec<_>>();
    assert_eq!(notes(&legacy), notes(&original));
    assert!(legacy.slides.iter().any(|s| !s.notes.is_empty()), "the file has notes");
    let transitions = |p: &Presentation| p.slides.iter().map(|s| s.transition.is_some()).collect::<Vec<_>>();
    assert_eq!(transitions(&legacy), transitions(&original));
    assert!(legacy.slides.iter().any(|s| s.transition.is_some()), "the file has a transition");
    // The pictures are there.
    let count = |p: &Presentation| p.slides.iter().flat_map(|s| &s.shapes).filter(|s| matches!(s.kind, ShapeKind::Picture(_))).count();
    assert_eq!(count(&legacy), count(&original));
}

/// The rich slide (groups, a turned shape, preset shapes, a freeform, a link in the text) saved as .ppt has the same shapes where
/// they were, and the link.
#[test]
fn a_legacy_rich_slide_keeps_its_shapes_and_links() {
    use ibis_doc_model::SlideLink;
    let legacy = ibis_formats::read(&std::fs::read(path("pptx-legacy-rich.ppt")).unwrap()).unwrap();
    let original = ibis_formats::read(&std::fs::read(path("pptx-legacy-rich.pptx")).unwrap()).unwrap();
    let frames = |p: &Presentation| {
        let mut v: Vec<(i64, i64, i64, i64)> = p.slides[2].shapes.iter().map(|s| (s.frame.x, s.frame.y, s.frame.width, s.frame.height)).collect();
        v.sort();
        v
    };
    let (a, b) = (frames(&legacy), frames(&original));
    assert_eq!(a.len(), b.len(), "{a:?} / {b:?}");
    for (x, y) in a.iter().zip(&b) {
        // (The file keeps places in 1/576 inch: 1587.5 EMU.)
        assert!((x.0 - y.0).abs() < 3200 && (x.1 - y.1).abs() < 3200 && (x.2 - y.2).abs() < 3200 && (x.3 - y.3).abs() < 3200, "{x:?} / {y:?}");
    }
    let links = |p: &Presentation| {
        p.slides[2].shapes.iter().filter_map(|s| s.text_body()).flat_map(|b| b.paragraphs.iter()).flat_map(|p| p.runs.iter()).filter_map(|r| r.style.link.clone()).collect::<Vec<_>>()
    };
    assert_eq!(links(&legacy), vec![SlideLink::Url("https://example.com/".to_string())]);
    assert_eq!(links(&original), links(&legacy));
}

/// A shadow and a cropped picture are read: the star has a shadow, the picture of slide 4 is cropped.
#[test]
fn a_legacy_shadow_and_crop_are_read() {
    let legacy = ibis_formats::read(&std::fs::read(path("pptx-legacy-rich.ppt")).unwrap()).unwrap();
    let original = ibis_formats::read(&std::fs::read(path("pptx-legacy-rich.pptx")).unwrap()).unwrap();
    let shadows = |p: &Presentation| p.slides[2].shapes.iter().filter(|s| matches!(&s.kind, ShapeKind::Auto(a) if a.shadow.is_some())).count();
    assert_eq!(shadows(&legacy), shadows(&original));
    assert_eq!(shadows(&legacy), 1);
    let crop = |p: &Presentation| p.slides[3].shapes.iter().find_map(|s| match &s.kind { ShapeKind::Picture(pic) => Some(pic.crop), _ => None });
    let (a, b) = (crop(&legacy).unwrap(), crop(&original).unwrap());
    // (The file keeps the fractions in 16.16 fixed point: a rounding off.)
    for (x, y) in [(a.left, b.left), (a.top, b.top), (a.right, b.right), (a.bottom, b.bottom)] {
        assert!((x - y).abs() < 50, "{a:?} / {b:?}");
    }
}
