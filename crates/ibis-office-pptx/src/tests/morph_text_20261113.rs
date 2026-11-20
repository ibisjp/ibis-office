// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-13 (#88): the morph of a long English text (about 2000 characters, the second slide's text a changed copy of the first's) by
//! word and by character: the words and characters both texts have move from where they were to where they are, the others fade.
use crate::morph::MorphCrate as _;
use crate::morph_text::{at, Move, Unit};
use crate::shell::scenario::Scenario;
use ibis_doc_model::{EmuRect, Shape, ShapeKind, TextBody};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("pptx::morph_text_20261113_a_long_text_moves_by_word_and_by_character", long_text)]
}

const SENTENCE: &str = "The river moved slowly past the old mill while the children counted boats and the farmers argued about rain. ";

fn text_box(id: ibis_doc_model::ShapeId, text: &str) -> Shape {
    Shape::new(id, EmuRect::new(500_000, 500_000, 8_000_000, 5_000_000), ShapeKind::TextBox(TextBody::plain(text, Default::default())))
}

fn long_text() {
    let old_text: String = (0..19).map(|_| SENTENCE).collect();
    let new_text = old_text.replace("old mill", "new bridge").replace("counted boats and ", "");
    assert!(old_text.chars().count() > 2000);
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        e.insert_shape(text_box(id, &old_text), cx);
    });
    s.page(2);
    s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        e.insert_shape(text_box(id, &new_text), cx);
    });
    let (old, new) = s.with(|e, _| (e.presentation().slides[0].clone(), e.presentation().slides[1].clone()));
    let last = |slide: &ibis_doc_model::Slide| slide.shown_shapes().last().unwrap().clone();
    let (a, b) = (last(&old), last(&new));
    for unit in [Unit::Word, Unit::Char] {
        let moves = s.with(|e, _| e.text_moves(&a, &b, unit, 1.0e-4)).expect("two plain texts are moved by their words");
        let both = moves.iter().filter(|m| matches!(m, Move::Both(..))).count();
        let out = moves.iter().filter(|m| matches!(m, Move::Out(_))).count();
        let inn = moves.iter().filter(|m| matches!(m, Move::In(_))).count();
        match unit {
            // Words: "old", "mill" go (2 each of 19) and "counted", "boats", "and" go (3 each); "new", "bridge" come (2 each).
            Unit::Word => assert_eq!((out, inn), (19 * 5, 19 * 2), "words that go and come"),
            // Characters: the new text is mostly the old text's characters.
            Unit::Char => assert!(both * 10 >= (both + inn) * 7, "most characters are kept: {both} kept, {inn} new"),
        }
        assert_eq!(both + out, moves.iter().filter(|m| !matches!(m, Move::In(_))).count());
        // The words that stay move on a straight line, from the old place (0) to the new one (100 %), on the measured curve (at 50 %: `move_curve`).
        let moved = moves.iter().filter_map(|m| if let Move::Both(x, y) = m { Some((x, y, m)) } else { None }).find(|(x, y, _)| (x.x - y.x).abs() > 1.0 || (x.y - y.y).abs() > 1.0).expect("the text re-flows, so some word has moved");
        let (x, y, m) = moved;
        let mid = crate::morph_text::move_curve(0.5);
        let close = |p: f32, q: f32| (p - q).abs() < 1e-3;
        assert!(close(at(m, 0.0).0.x, x.x) && close(at(m, 0.0).0.y, x.y));
        assert!(close(at(m, 0.5).0.x, x.x + (y.x - x.x) * mid) && close(at(m, 0.5).0.y, x.y + (y.y - x.y) * mid));
        assert!(close(at(m, 1.0).0.x, y.x) && close(at(m, 1.0).0.y, y.y));
        assert_eq!(at(m, 0.5).1, 1.0);
    }
}
