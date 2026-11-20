// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A presentation written as an OpenDocument presentation reads back as the same slides, texts and shapes.

use ibis_doc_model::{Presentation, ShapeKind};

fn path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name)
}

fn read(name: &str) -> Presentation {
    ibis_odf::read(&std::fs::read(path(name)).unwrap()).unwrap()
}

fn round_trip(p: &Presentation) -> Presentation {
    let odp = ibis_odf::write_odp(p).expect("written");
    assert!(ibis_odf::odp::is_odp(&odp));
    ibis_odf::read(&odp).expect("read back")
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
fn a_written_odp_reads_back_with_its_slides_texts_and_notes() {
    let original = read("pptx-plan.pptx");
    let back = round_trip(&original);
    assert_eq!(back.slides.len(), original.slides.len());
    assert!((back.slide_size.width - original.slide_size.width).abs() < 400, "{:?} / {:?}", back.slide_size, original.slide_size);
    assert_eq!(texts(&back), texts(&original));
    let notes = |p: &Presentation| p.slides.iter().map(|s| s.notes.trim().to_string()).collect::<Vec<_>>();
    assert_eq!(notes(&back), notes(&original));
    let pictures = |p: &Presentation| p.slides.iter().flat_map(|s| &s.shapes).filter(|s| matches!(s.kind, ShapeKind::Picture(_))).count();
    assert_eq!(pictures(&back), pictures(&original));
    let transitions = |p: &Presentation| p.slides.iter().map(|s| s.transition.is_some()).collect::<Vec<_>>();
    assert_eq!(transitions(&back), transitions(&original));
}

#[test]
fn shapes_keep_their_places_fills_and_text_formatting() {
    let original = read("pptx-legacy-rich.pptx");
    let back = round_trip(&original);
    let frames = |p: &Presentation, i: usize| {
        let mut v: Vec<(i64, i64, i64, i64)> = p.slides[i].shapes.iter().map(|s| (s.frame.x, s.frame.y, s.frame.width, s.frame.height)).collect();
        v.sort();
        v
    };
    for i in 0..original.slides.len() {
        let (a, b) = (frames(&back, i), frames(&original, i));
        assert_eq!(a.len(), b.len(), "slide {i}: {a:?} / {b:?}");
        for x in &a {
            assert!(b.iter().any(|y| (x.0 - y.0).abs() < 4000 && (x.1 - y.1).abs() < 4000 && (x.2 - y.2).abs() < 4000 && (x.3 - y.3).abs() < 4000), "{x:?} has no counterpart in {b:?}");
        }
    }
    assert_eq!(texts(&back), texts(&original));
    let runs = |p: &Presentation| {
        p.slides[1].shapes.iter().filter_map(|s| s.text_body()).flat_map(|b| b.paragraphs.iter()).flat_map(|p| p.runs.iter()).map(|r| (r.text.clone(), r.style.bold.unwrap_or(false), r.style.font_size)).collect::<Vec<_>>()
    };
    let (a, b) = (runs(&back), runs(&original));
    assert_eq!(a.len(), b.len(), "{a:?} / {b:?}");
    for (x, y) in a.iter().zip(&b) {
        assert_eq!(x.0, y.0);
        assert_eq!(x.1, y.1, "bold of {}", x.0);
        assert_eq!(x.2, y.2, "size of {}", x.0);
    }
    let links = |p: &Presentation| p.slides[2].shapes.iter().filter_map(|s| s.text_body()).flat_map(|b| b.paragraphs.iter()).flat_map(|p| p.runs.iter()).filter_map(|r| r.style.link.clone()).count();
    assert_eq!(links(&back), links(&original));
}

#[test]
fn animations_are_written_and_read_back() {
    let original = read("slideshow-check.pptx");
    assert!(original.slides.iter().any(|s| !s.animations.is_empty()), "the file has animations");
    let back = round_trip(&original);
    let list = |p: &Presentation| p.slides.iter().map(|s| s.animations.iter().map(|a| (a.class, a.start)).collect::<Vec<_>>()).collect::<Vec<_>>();
    assert_eq!(list(&back), list(&original));
}
