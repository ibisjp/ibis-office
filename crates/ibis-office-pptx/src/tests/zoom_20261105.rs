// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-05: PowerPoint's ズーム objects read from a file PowerPoint wrote (`test/zoom-sample/office-zoom.pptx`, not
//! published: the scenario returns without it), and played in the show: the card grows into the slide it goes to, and the
//! slide shrinks back into it when its section is over.
use crate::shell::scenario::Scenario;
use ibis_doc_model::SlideLink;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    let mut all = vec![("pptx::zoom_20261105_cards_grow_and_shrink_in_the_show", cards_grow_and_shrink as fn())];
    all.extend(summary());
    all
}

fn cards_grow_and_shrink() {
    if !std::path::Path::new("test/zoom-sample/office-zoom.pptx").exists() {
        return;
    }
    let mut s = Scenario::open("test/zoom-sample/office-zoom.pptx");
    // The section zoom is on the fourth slide (the first slide of 本編): it goes to the section まとめ.
    s.page(4);
    let (card, target_slide) = s.with(|e, _| {
        let id = *e.slide().zoom_objects.keys().next().expect("a zoom card on the slide");
        let p = e.presentation();
        // The slide the card links to (the first slide of the section it zooms to).
        let Some(SlideLink::Slide(to)) = e.slide().shape_links.get(&id) else { panic!("the card links to a slide") };
        assert!(p.slide(*to).is_some());
        (id, *to)
    });
    s.screenshot("zoom-card-on-slide");
    s.update(|e, w, cx| e.start_slideshow(w, cx));
    let origin = s.with(|e, _| e.current_slide);
    s.update(|e, w, cx| e.follow_link_from_shape_in_show(Some(card), &SlideLink::Slide(target_slide), w, cx));
    let anim = s.with(|e, _| e.slideshow().and_then(|sh| sh.zoom_anim.clone())).expect("a zoom is played");
    assert!(anim.grows && anim.other == origin && anim.duration_ms == 1000, "{anim:?}");
    s.screenshot("zoom-growing");
    // Past the section's last slide the show returns to the card's slide, the slide shrinking back into the card.
    s.update(|e, w, cx| e.step_slideshow(true, w, cx));
    s.update(|e, w, cx| e.step_slideshow(true, w, cx));
    assert_eq!(s.with(|e, _| e.current_slide), origin);
    let back = s.with(|e, _| e.slideshow().and_then(|sh| sh.zoom_anim.clone())).expect("the return is played");
    assert!(!back.grows && back.frame == anim.frame, "{back:?}");
    s.screenshot("zoom-shrinking");
    s.update(|e, w, cx| e.end_slideshow(w, cx));
}

pub(crate) fn summary() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::zoom_20261105_summary_zoom_makes_a_section_and_a_slide_and_follows_the_sections", summary_follows_sections),
        ("pptx::zoom_20261105_new_cards_are_saved_in_powerpoints_form", new_cards_round_trip),
        ("pptx::zoom_20261105_summary_grid_is_the_files", grid_is_the_files),
    ]
}

/// Slides 1..=3 of the plan in three sections; the first slide of each.
fn sectioned() -> (Scenario, Vec<ibis_doc_model::SlideId>, Vec<String>) {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let slides: Vec<_> = s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.id).collect());
    assert!(slides.len() >= 3);
    let sections = vec![
        ibis_doc_model::Section { id: "{AAAAAAAA-0000-0000-0000-000000000001}".into(), name: "A".into(), slides: vec![slides[0]] },
        ibis_doc_model::Section { id: "{AAAAAAAA-0000-0000-0000-000000000002}".into(), name: "B".into(), slides: vec![slides[1]] },
        ibis_doc_model::Section { id: "{AAAAAAAA-0000-0000-0000-000000000003}".into(), name: "C".into(), slides: slides[2..].to_vec() },
    ];
    s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::SetSections { sections: sections.clone() }], cx));
    let ids = sections.iter().map(|x| x.id.clone()).collect();
    (s, slides, ids)
}

fn summary_follows_sections() {
    use ibis_doc_model::ZoomTarget;
    let (mut s, slides, ids) = sectioned();
    let before = s.with(|e, _| e.presentation().slides.len());
    // 挿入 > ズーム > サマリー ズーム for the sections B and C: a new section at the front with a new slide, and a card for each.
    let cards = s.update(|e, w, cx| e.insert_summary_zoom_for_sections(&[ids[1].clone(), ids[2].clone()], w, cx));
    assert_eq!(cards.len(), 2);
    s.screenshot("summary-zoom");
    let (n, first_section, first_slide, summary_slide) = s.with(|e, _| {
        let p = e.presentation();
        (p.slides.len(), p.sections[0].clone(), p.slides[0].id, e.current_slide)
    });
    assert_eq!(n, before + 1);
    assert_eq!(first_slide, summary_slide);
    assert_eq!(first_section.slides, vec![summary_slide], "the summary is alone in the first section");
    assert!(!first_section.name.is_empty());
    s.with(|e, _| {
        let slide = e.slide();
        assert_eq!(slide.zoom_objects.len(), 2);
        for (i, (card, expected)) in cards.iter().zip([slides[1], slides[2]]).enumerate() {
            let z = &slide.zoom_objects[card];
            assert!(matches!(&z.target, ZoomTarget::Summary { index, .. } if *index == i), "{z:?}");
            assert_eq!(slide.shape_links.get(card), Some(&SlideLink::Slide(expected)));
            assert!(slide.shape_return_to_zoom.contains(card));
        }
    });
    // The first slide of section C changes (slide 3 moves behind slide 4): the card of C goes to the new first slide.
    let order = s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.id).collect::<Vec<_>>());
    let third = slides[2];
    let at = order.iter().position(|id| *id == third).unwrap();
    if order.len() > at + 1 {
        s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::MoveSlide { slide: third, index: at + 1 }], cx));
        let now = s.with(|e, _| e.presentation().sections.iter().find(|sec| sec.id == ids[2]).and_then(|sec| sec.slides.first().copied())).unwrap();
        assert_ne!(now, third);
        let linked = s.with(|e, _| e.presentation().slides[0].shape_links.get(&cards[1]).cloned());
        assert_eq!(linked, Some(SlideLink::Slide(now)), "the card follows the section's first slide");
    }
    // A section taken away: its card goes.
    let kept: Vec<_> = s.with(|e, _| e.presentation().sections.iter().filter(|sec| sec.id != ids[1]).cloned().collect());
    let mut kept = kept;
    // (Its slide joins the section before it, as removing a section does.)
    let b_slides = s.with(|e, _| e.presentation().sections.iter().find(|sec| sec.id == ids[1]).map(|sec| sec.slides.clone())).unwrap();
    let a = kept.iter_mut().find(|sec| sec.id == ids[0]).unwrap();
    a.slides.extend(b_slides);
    s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::SetSections { sections: kept }], cx));
    let cards_left = s.with(|e, _| e.presentation().slides[0].zoom_objects.len());
    assert_eq!(cards_left, 1, "the card of the removed section is gone");
}

fn new_cards_round_trip() {
    let (mut s, _, ids) = sectioned();
    s.update(|e, w, cx| {
        e.insert_summary_zoom_for_sections(&[ids[1].clone(), ids[2].clone()], w, cx);
    });
    let second = s.with(|e, _| e.presentation().slides[2].id);
    s.update(|e, w, cx| e.select_slide(second, w, cx));
    s.update(|e, _, cx| {
        e.insert_slide_zoom(e.presentation().slides[1].id, cx);
    });
    s.update(|e, _, cx| {
        let section = e.presentation().sections[2].id.clone();
        e.insert_section_zoom(&section, cx);
    });
    let bytes = s.with(|e, _| e.current_bytes()).expect("saved");
    let after = ibis_pptx::read(&bytes).unwrap();
    assert_eq!(after.sections.len(), 4);
    assert_eq!(after.slides[0].zoom_objects.len(), 2, "the summary's two cards");
    let on_slide = after.slides.iter().find(|sl| sl.zoom_objects.values().any(|z| matches!(z.target, ibis_doc_model::ZoomTarget::Slide { .. }))).expect("the slide zoom is read back");
    assert!(on_slide.zoom_objects.values().any(|z| matches!(z.target, ibis_doc_model::ZoomTarget::Section { .. })), "and the section zoom");
    for slide in &after.slides {
        for shape in slide.zoom_objects.keys() {
            assert!(slide.shape_links.contains_key(shape), "each card links to its slide");
        }
    }
}

/// The grid PowerPoint lays the cards of a summary in, as its own file has it (three cards, two columns, in the content frame of
/// the title-and-content layout).
fn grid_is_the_files() {
    let frame = ibis_doc_model::EmuRect::new(838_200, 1_825_625, 10_515_600, 4_351_338);
    let grid = crate::zoom_cards::grid_for_test(3, frame, 12_192_000.0 / 6_858_000.0);
    let expect = [(1_711_460, 152_297), (5_323_070, 152_297), (1_711_460, 2_240_939)];
    for (card, (x, y)) in grid.iter().zip(expect) {
        assert!((card.x - frame.x - x).abs() <= 2 && (card.y - frame.y - y).abs() <= 2, "{card:?}");
        assert!((card.width - 3_481_070).abs() <= 2 && (card.height - 1_958_102).abs() <= 2, "{card:?}");
    }
}
