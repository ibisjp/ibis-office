// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue 85: a layout applied to a slide gives it the placeholders the layout defines, also after "blank" took the
//! empty ones away, and for layouts with more than two.
use ibis_doc_model::{Layout, LayoutId, LayoutPlaceholder, Presentation, SlideId};

fn defined(layout: &Layout) -> usize {
    layout.placeholders.iter().filter(|p| !matches!(p.placeholder.kind.as_deref(), Some("dt" | "ftr" | "sldNum" | "hdr"))).count()
}

fn blank_layout(p: &Presentation) -> LayoutId {
    p.layouts.iter().find(|l| defined(l) == 0).expect("a layout with no placeholder").id
}

fn has_body(l: &Layout) -> bool {
    l.placeholders.iter().any(|p| p.placeholder.normalized() == "body")
}

fn with_many(p: &mut Presentation, bodies: usize) -> LayoutId {
    let mut many = p.layouts.iter().find(|l| defined(l) >= 2 && has_body(l)).expect("a layout with a title and a body").clone();
    many.id = LayoutId(777_000 + bodies as u64);
    many.name = "many".into();
    let body = many.placeholders.iter().find(|l| l.placeholder.normalized() == "body").unwrap().clone();
    for i in 0..bodies {
        let mut more: LayoutPlaceholder = body.clone();
        more.placeholder.index = Some(format!("{}", 20 + i));
        many.placeholders.push(more);
    }
    let id = many.id;
    p.layouts.push(many);
    id
}

fn counts(p: &Presentation, slide: SlideId) -> (usize, usize) {
    let s = p.slide(slide).unwrap();
    (s.shapes.len(), s.placeholders.len())
}

/// 白紙 then another layout, in the model: the title and the bodies come back (empty).
#[test]
fn a_layout_after_blank_brings_the_placeholders_back() {
    let mut p = ibis_pptx::read(&ibis_pptx::blank_file(ibis_pptx::Kind::Presentation).unwrap()).unwrap();
    let blank = blank_layout(&p);
    let many = with_many(&mut p, 2);
    let slide = p.slides[0].id;
    let first = p.layouts.iter().find(|l| defined(l) >= 2 && has_body(l)).unwrap().id;
    let a = p.with_layout(slide, first).unwrap();
    p.slides[0] = a;
    let start = counts(&p, slide);
    assert_eq!(start.1, defined(p.layout(first).unwrap()), "the slide has what its layout defines");
    let b = p.with_layout(slide, blank).unwrap();
    p.slides[0] = b;
    assert_eq!(counts(&p, slide), (0, 0), "blank: no placeholder left");
    let c = p.with_layout(slide, many).unwrap();
    p.slides[0] = c;
    let want = defined(p.layout(many).unwrap());
    assert!(want >= 4);
    assert_eq!(counts(&p, slide), (want, want), "all the layout's placeholders, not two");
    // The same layout again changes nothing.
    assert_eq!(p.with_layout(slide, many).unwrap(), p.slides[0]);
}

/// Through the file: written, read back with the layout's formats and kept by saving and reopening.
#[test]
fn blank_then_a_layout_with_many_placeholders_through_the_file() {
    let opened = ibis_pptx::open(ibis_pptx::blank_file(ibis_pptx::Kind::Presentation).unwrap()).unwrap();
    let mut model = opened.presentation.clone();
    let blank = blank_layout(&model);
    let slide = model.slides[0].id;
    // Every layout of the file, from blank.
    let targets: Vec<LayoutId> = model.layouts.iter().filter(|l| defined(l) > 0).map(|l| l.id).collect();
    assert!(!targets.is_empty());
    let mut next = 900_000u64;
    for layout in targets {
        let mut m = model.clone();
        let to_blank = opened.source.with_layout(&m, slide, blank).unwrap();
        m.slides[0] = to_blank;
        assert_eq!(counts(&m, slide), (0, 0));
        let mut new_id = || {
            next += 1;
            ibis_doc_model::ShapeId(next)
        };
        let applied = opened.source.with_layout_ids(&m, slide, layout, &mut new_id).unwrap();
        let want = defined(m.layout(layout).unwrap());
        assert_eq!(applied.placeholders.len(), want, "layout {:?}", m.layout(layout).unwrap().name);
        assert_eq!(applied.shapes.len(), want);
        m.slides[0] = applied;
        let bytes = opened.source.save(&m, ibis_pptx::SaveOptions::default()).unwrap();
        let again = ibis_pptx::read(&bytes).unwrap();
        assert_eq!(again.slides[0].placeholders.len(), want, "saved and read back: {:?}", m.layout(layout).unwrap().name);
        assert_eq!(again.slides[0].shapes.len(), want);
    }
    let _ = &mut model;
}
