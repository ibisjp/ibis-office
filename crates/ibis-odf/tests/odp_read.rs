// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! An OpenDocument presentation (.odp) is read as the presentation made from it: the same slides, texts, shapes, notes and
//! pictures as the .pptx it was made from (test/editing/pptx-plan.odp and pptx-legacy-rich.odp are the .pptx files of the same
//! names saved as OpenDocument by LibreOffice).

use ibis_doc_model::{Presentation, ShapeKind};

fn path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name)
}

fn read(name: &str) -> Presentation {
    ibis_odf::read(&std::fs::read(path(name)).unwrap()).unwrap()
}

fn texts(p: &Presentation) -> Vec<Vec<String>> {
    p.slides
        .iter()
        .map(|s| {
            let mut v: Vec<String> = s.shapes.iter().filter_map(|sh| sh.text_body().map(|b| b.text())).filter(|t| !t.trim().is_empty()).map(|t| t.trim().to_string()).collect();
            v.sort();
            v
        })
        .collect()
}

#[test]
fn an_odp_has_the_slides_texts_notes_and_pictures_of_its_pptx() {
    let odp = read("pptx-plan.odp");
    let original = read("pptx-plan.pptx");
    assert_eq!(odp.slides.len(), original.slides.len());
    // (The size is the same to the hundredth of a millimetre.)
    assert!((odp.slide_size.width - original.slide_size.width).abs() < 400 && (odp.slide_size.height - original.slide_size.height).abs() < 400, "{:?} / {:?}", odp.slide_size, original.slide_size);
    assert_eq!(texts(&odp), texts(&original));
    let notes = |p: &Presentation| p.slides.iter().map(|s| s.notes.trim().to_string()).collect::<Vec<_>>();
    assert_eq!(notes(&odp), notes(&original));
    assert!(odp.slides.iter().any(|s| !s.notes.is_empty()), "the file has notes");
    let transitions = |p: &Presentation| p.slides.iter().map(|s| s.transition.is_some()).collect::<Vec<_>>();
    assert_eq!(transitions(&odp), transitions(&original));
    let count = |p: &Presentation| p.slides.iter().flat_map(|s| &s.shapes).filter(|s| matches!(s.kind, ShapeKind::Picture(_))).count();
    assert_eq!(count(&odp), count(&original));
    assert!(count(&odp) > 0);
}

/// The rich slide (groups, a turned shape, preset shapes, a freeform, a link in the text): the shapes are where they were.
#[test]
fn an_odp_keeps_the_shapes_where_they_were_and_the_links() {
    use ibis_doc_model::SlideLink;
    let odp = read("pptx-legacy-rich.odp");
    let original = read("pptx-legacy-rich.pptx");
    let frames = |p: &Presentation| {
        let mut v: Vec<(i64, i64, i64, i64)> = p.slides[2].shapes.iter().map(|s| (s.frame.x, s.frame.y, s.frame.width, s.frame.height)).collect();
        v.sort();
        v
    };
    let (a, b) = (frames(&odp), frames(&original));
    assert_eq!(a.len(), b.len(), "{a:?} / {b:?}");
    // Each shape has its counterpart (the files differ by a few hundredths of a millimetre).
    for x in &a {
        assert!(b.iter().any(|y| (x.0 - y.0).abs() < 4000 && (x.1 - y.1).abs() < 4000 && (x.2 - y.2).abs() < 4000 && (x.3 - y.3).abs() < 4000), "{x:?} has no counterpart in {b:?}");
    }
    let links = |p: &Presentation| {
        p.slides[2].shapes.iter().filter_map(|s| s.text_body()).flat_map(|b| b.paragraphs.iter()).flat_map(|p| p.runs.iter()).filter_map(|r| r.style.link.clone()).collect::<Vec<_>>()
    };
    assert_eq!(links(&odp), vec![SlideLink::Url("https://example.com/".to_string())]);
    // The star's shadow, the picture's crop, the rotation of the turned shape.
    let shadows = |p: &Presentation| p.slides[2].shapes.iter().filter(|s| matches!(&s.kind, ShapeKind::Auto(a) if a.shadow.is_some())).count();
    assert_eq!(shadows(&odp), shadows(&original));
    let turned = |p: &Presentation| p.slides[2].shapes.iter().map(|s| s.rotation).filter(|r| *r != 0).collect::<Vec<_>>();
    assert_eq!(turned(&odp).len(), turned(&original).len());
    for (x, y) in turned(&odp).iter().zip(turned(&original)) {
        assert!((x - y).abs() < 20_000, "{x} / {y}");
    }
    let crop = |p: &Presentation| p.slides[3].shapes.iter().find_map(|s| match &s.kind { ShapeKind::Picture(pic) => Some(pic.crop), _ => None });
    let (c, d) = (crop(&odp).unwrap(), crop(&original).unwrap());
    for (x, y) in [(c.left, d.left), (c.top, d.top), (c.right, d.right), (c.bottom, d.bottom)] {
        assert!((x - y).abs() < 3000, "{c:?} / {d:?}");
    }
}

#[test]
fn the_text_formatting_of_an_odp_is_kept() {
    let odp = read("pptx-legacy-rich.odp");
    let original = read("pptx-legacy-rich.pptx");
    // The runs of slide 2's outline: bold, colour and size agree.
    let runs = |p: &Presentation| {
        p.slides[1]
            .shapes
            .iter()
            .filter_map(|s| s.text_body())
            .flat_map(|b| b.paragraphs.iter())
            .flat_map(|p| p.runs.iter())
            .map(|r| (r.text.clone(), r.style.bold, r.style.font_size))
            .collect::<Vec<_>>()
    };
    let (a, b) = (runs(&odp), runs(&original));
    assert_eq!(a.len(), b.len(), "{a:?} / {b:?}");
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.0, y.0);
        assert_eq!(x.1, y.1, "bold of {}", x.0);
        assert_eq!(x.2, y.2, "size of {}", x.0);
    }
}

/// The animations of the .pptx come through the .odp: the same effects, classes and starts, shape by shape.
#[test]
fn the_animations_of_an_odp_are_those_of_its_pptx() {
    let odp = read("slideshow-check.odp");
    let original = read("slideshow-check.pptx");
    assert_eq!(odp.slides.len(), original.slides.len());
    let list = |p: &Presentation| {
        p.slides
            .iter()
            .map(|s| s.animations.iter().map(|a| (a.class, a.start, a.preset.0)).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    };
    let (a, b) = (list(&odp), list(&original));
    assert!(b.iter().any(|v| !v.is_empty()), "the file has animations");
    assert_eq!(a, b);
}
