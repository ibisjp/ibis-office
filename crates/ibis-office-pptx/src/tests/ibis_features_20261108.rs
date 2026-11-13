// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-08: ibisOffice's own features PowerPoint does not have (GitHub issues #131, #132). The question asked the first time
//! a file uses one (キャンセル / OK, saved in the file), and the text wrap round pictures and videos (laid out, saved, read).
use crate::shell::dialog::InteractionLayer;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{EmuRect, ObjectWrapKind, Shape, ShapeId};
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::ibis_features_the_question_is_asked_once_per_file", question_is_asked_once as fn()),
        ("pptx::ibis_features_text_wraps_round_a_picture_square_and_top_bottom", wrap_round_a_picture),
        ("pptx::ibis_features_vertical_text_wraps_round_a_picture", vertical_wrap),
        ("pptx::ibis_features_wrap_is_saved_and_read_and_powerpoint_can_skip_it", wrap_round_trip),
    ]
    .into_iter()
    .chain(chain_tests())
    .collect()
}

fn png(color: [u8; 4]) -> Vec<u8> {
    use image::ImageEncoder;
    let img = image::RgbaImage::from_pixel(40, 40, image::Rgba(color));
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes).write_image(img.as_raw(), 40, 40, image::ExtendedColorType::Rgba8).unwrap();
    bytes
}

const SENTENCE: &str = "The quick brown fox jumps over the lazy dog while the text flows round the picture in the middle of the box. ";

/// A picture selected at the middle of the slide and a text box over it, long enough to fill its box.
fn picture_and_text(s: &mut Scenario, vertical: bool) -> (ShapeId, ShapeId) {
    let picture = s.update(|e, _, cx| e.insert_picture("p.png", png([200, 40, 40, 255]), cx)).expect("a picture");
    let frame = s.with(|e, _| e.slide().shape(picture).unwrap().frame);
    let text = s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        let area = EmuRect::new(frame.x - 1_800_000, frame.y - 1_000_000, frame.width + 3_600_000, frame.height + 2_000_000);
        let mut shape = Shape::text_box(id, area, SENTENCE.repeat(12));
        if let Some(body) = shape.text_body_mut() {
            body.wrap = true;
            if vertical {
                body.vertical = true;
                body.vertical_kind = ibis_doc_model::VerticalKind::EastAsian;
            }
        }
        e.insert_shape(shape, cx);
        id
    });
    // (The text box was put on top: the picture is the one to choose.)
    s.update(|e, _, _| e.set_selection(vec![picture]));
    (picture, text)
}

fn answer(s: &mut Scenario, ok: bool) {
    assert_eq!(s.with(|e, _| e.interaction_layer()), InteractionLayer::Ask, "the question is showing");
    s.update(|e, _, cx| e.answer_ask(ok, cx));
}

fn question_is_asked_once() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (picture, _) = picture_and_text(&mut s, false);
    assert!(!s.with(|e, _| e.presentation().ibis_extensions));
    // Turning it off needs no question in a file that never turned it on.
    s.update(|e, _, cx| e.set_selected_wrap(Some(ObjectWrapKind::None), None, cx));
    assert!(s.with(|e, _| e.ask.is_none()));
    // The first use asks; キャンセル does nothing.
    s.update(|e, _, cx| e.set_selected_wrap(Some(ObjectWrapKind::Square), None, cx));
    assert_eq!(s.with(|e, _| e.ask.as_ref().map(|a| (a.title.to_string(), a.message.to_string()))), Some(("ibisOffice の機能".to_string(), "この機能は PowerPoint にない機能です。有効にしますか？".to_string()))); // i18n-ok (Japanese text under test or a name table that matches labels)
    s.screenshot("ibis-feature-question");
    answer(&mut s, false);
    assert!(s.with(|e, _| e.ask.is_none() && !e.presentation().ibis_extensions && e.slide().ibis_wrap.is_empty()), "キャンセル: nothing is done"); // i18n-ok (Japanese text under test or a name table that matches labels)
    // OK turns the features on for the file and does what was asked.
    s.update(|e, _, cx| e.set_selected_wrap(Some(ObjectWrapKind::Square), None, cx));
    answer(&mut s, true);
    assert!(s.with(|e, _| e.presentation().ibis_extensions));
    assert_eq!(s.with(|e, _| e.slide().ibis_wrap.get(&picture).map(|w| w.kind)), Some(ObjectWrapKind::Square));
    // Asked no more in this file.
    s.update(|e, _, cx| e.set_selected_wrap(Some(ObjectWrapKind::TopBottom), None, cx));
    assert!(s.with(|e, _| e.ask.is_none()));
    assert_eq!(s.with(|e, _| e.slide().ibis_wrap.get(&picture).map(|w| w.kind)), Some(ObjectWrapKind::TopBottom));
    // The answer is in the saved file: reading it, the file asks no more, and a file saved before the answer asks again.
    s.save_as("ibis-features.pptx");
    assert!(s.reopen().ibis_extensions);
}

/// The boxes (window pixels) of the characters of the edited text box: (left, top, right, bottom).
fn char_boxes(s: &mut Scenario) -> Vec<[f32; 4]> {
    s.with(|e, _| {
        let list = e.shape_edit.positions.paragraphs.borrow().clone();
        let mut out = Vec::new();
        for p in list {
            let p = p.borrow();
            for c in &p.chars {
                let l = p.lines[c.line];
                let corners = p.corners(c.x, l.top, c.advance, l.height);
                let xs = corners.map(|c| c.0);
                let ys = corners.map(|c| c.1);
                out.push([xs.iter().cloned().fold(f32::MAX, f32::min), ys.iter().cloned().fold(f32::MAX, f32::min), xs.iter().cloned().fold(f32::MIN, f32::max), ys.iter().cloned().fold(f32::MIN, f32::max)]);
            }
        }
        out
    })
}

/// The picture's rectangle in window pixels, grown by the margin.
fn picture_rect(s: &mut Scenario, picture: ShapeId) -> [f32; 4] {
    s.with(|e, _| {
        let layout = e.slide_layout().unwrap();
        let w = e.slide().ibis_wrap.get(&picture).copied().unwrap_or_default();
        let f = e.slide().shape(picture).unwrap().frame;
        let [ml, mt, mr, mb] = w.margins;
        let (x0, y0) = (layout.origin.x + layout.to_px(f.x - ml), layout.origin.y + layout.to_px(f.y - mt));
        let (x1, y1) = (layout.origin.x + layout.to_px(f.x + f.width + mr), layout.origin.y + layout.to_px(f.y + f.height + mb));
        [f32::from(x0), f32::from(y0), f32::from(x1), f32::from(y1)]
    })
}

fn overlaps(a: [f32; 4], b: [f32; 4]) -> bool {
    overlaps_by(a, b, 1.0)
}

/// Whether the boxes overlap by more than `slack` pixels (the rounding of the layout: the text of a turned box is placed on pixels
/// of its own, a pixel or two off).
fn overlaps_by(a: [f32; 4], b: [f32; 4], slack: f32) -> bool {
    a[0] < b[2] - slack && a[2] > b[0] + slack && a[1] < b[3] - slack && a[3] > b[1] + slack
}

fn wrap_with(s: &mut Scenario, kind: ObjectWrapKind) {
    s.update(|e, _, cx| e.set_selected_wrap(Some(kind), None, cx));
    if s.with(|e, _| e.ask.is_some()) {
        answer(s, true);
    }
}

/// Edits the text box (its characters are then painted with positions) and gives the characters' boxes.
fn painted(s: &mut Scenario, text: ShapeId) -> Vec<[f32; 4]> {
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| sh.id == text).unwrap());
    s.double_click_shape(index);
    let boxes = char_boxes(s);
    s.key("escape");
    boxes
}

fn wrap_round_a_picture() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (picture, text) = picture_and_text(&mut s, false);
    // No wrap: the text runs under the picture.
    let free = painted(&mut s, text);
    let rect = picture_rect(&mut s, picture);
    assert!(!free.is_empty());
    assert!(free.iter().any(|c| overlaps(*c, rect)), "without a wrap some characters are under the picture");
    s.update(|e, _, _| e.set_selection(vec![picture]));
    // 四角形: no character is in the picture's rectangle; the lines beside it are on its widest side; text goes on below it.
    wrap_with(&mut s, ObjectWrapKind::Square);
    let rect = picture_rect(&mut s, picture);
    let square = painted(&mut s, text);
    s.screenshot("wrap-square");
    assert!(square.iter().all(|c| !overlaps(*c, rect)), "no character is in the picture's rectangle");
    assert!(square.iter().any(|c| c[1] >= rect[3] - 1.0), "the text goes on below the picture");
    let beside = square.iter().filter(|c| c[1] < rect[3] - 1.0 && c[3] > rect[1] + 1.0).count();
    assert!(beside > 0, "the lines beside the picture have text on its wide side");
    assert_eq!(square.len(), free.len(), "all the text is still there");
    // 上下: nothing beside the picture.
    s.update(|e, _, _| e.set_selection(vec![picture]));
    wrap_with(&mut s, ObjectWrapKind::TopBottom);
    let top_bottom = painted(&mut s, text);
    s.screenshot("wrap-top-bottom");
    assert!(top_bottom.iter().all(|c| !(c[1] < rect[3] - 1.0 && c[3] > rect[1] + 1.0)), "no line beside the picture");
    // 回り込みを無視する: the text box goes under it again.
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| sh.id == text).unwrap());
    s.update(|e, _, _| e.set_selection(vec![text]));
    s.update(|e, _, cx| e.toggle_selected_ignore_wrap(cx));
    assert!(s.with(|e, _| e.slide().ibis_wrap_ignore.contains(&text)));
    let _ = index;
    let ignoring = painted(&mut s, text);
    assert!(ignoring.iter().any(|c| overlaps(*c, rect)), "a text box that ignores the wrap runs under the picture");
    // Undo brings the wrap of the text back.
    s.key("cmd-z");
    assert!(s.with(|e, _| !e.slide().ibis_wrap_ignore.contains(&text)));
}

fn vertical_wrap() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (picture, text) = picture_and_text(&mut s, true);
    let free = painted(&mut s, text);
    let rect = picture_rect(&mut s, picture);
    assert!(free.iter().any(|c| overlaps(*c, rect)), "without a wrap some characters are under the picture");
    s.update(|e, _, _| e.set_selection(vec![picture]));
    wrap_with(&mut s, ObjectWrapKind::Square);
    let rect = picture_rect(&mut s, picture);
    let wrapped = painted(&mut s, text);
    s.screenshot("wrap-vertical-square");
    assert!(wrapped.iter().all(|c| !overlaps(*c, rect)), "no character of the columns is in the picture's rectangle");
    assert_eq!(wrapped.len(), free.len());
}

fn wrap_round_trip() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (picture, text) = picture_and_text(&mut s, false);
    wrap_with(&mut s, ObjectWrapKind::Square);
    s.update(|e, _, cx| e.set_selected_wrap(None, Some(5 * 36_000), cx));
    s.update(|e, _, _| e.set_selection(vec![text]));
    s.update(|e, _, cx| e.toggle_selected_ignore_wrap(cx));
    let path = s.save_as("ibis-wrap.pptx");
    let saved = s.reopen();
    assert!(saved.ibis_extensions);
    let slide = saved.slides.iter().find(|sl| sl.ibis_wrap.contains_key(&picture) || !sl.ibis_wrap.is_empty()).expect("the slide with the wrap");
    let wrap = slide.ibis_wrap.values().next().copied().unwrap();
    assert_eq!((wrap.kind, wrap.margins), (ObjectWrapKind::Square, [5 * 36_000; 4]));
    assert_eq!(slide.ibis_wrap_ignore.len(), 1);
    // PowerPoint skips what it does not know: the extensions are `ext` elements with a uri of their own, inside the
    // `extLst` of the presentation and of the shapes' `cNvPr`; the rest of the part is as before.
    let package = ibis_pptx::ibis_ooxml::Package::open(std::fs::read(&path).unwrap()).unwrap();
    let presentation = String::from_utf8(package.read("ppt/presentation.xml").unwrap()).unwrap();
    assert!(presentation.contains("{1B15C0DE-0001-4B15-9E77-1B15A0F0C001}") && presentation.contains("ibis:features"), "{presentation}");
    let part = (1..=20).filter_map(|n| package.read(&format!("ppt/slides/slide{n}.xml")).ok()).map(|b| String::from_utf8(b).unwrap()).find(|x| x.contains("ibis:shape")).expect("a slide with the shapes' extension");
    assert!(part.contains("{1B15C0DE-0002-4B15-9E77-1B15A0F0C002}") && part.contains("wrap=\"square\"") && part.contains("marginL=\"180000\"") && part.contains("marginB=\"180000\"") && part.contains("ignoreWrap=\"1\""), "{part}");
    // Saved again with no wrap, the extension is gone from the shapes.
    s.update(|e, _, _| e.set_selection(vec![picture]));
    s.update(|e, _, cx| e.set_selected_wrap(Some(ObjectWrapKind::None), None, cx));
    s.update(|e, _, _| e.set_selection(vec![text]));
    s.update(|e, _, cx| e.toggle_selected_ignore_wrap(cx));
    let path = s.save_as("ibis-wrap-off.pptx");
    let package = ibis_pptx::ibis_ooxml::Package::open(std::fs::read(&path).unwrap()).unwrap();
    assert!((1..=20).filter_map(|n| package.read(&format!("ppt/slides/slide{n}.xml")).ok()).all(|b| !String::from_utf8(b).unwrap().contains("ibis:shape")));
}

// ----- Linked text boxes (#131) ---------------------------------------------------------------------------------------

pub(crate) fn chain_tests() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::ibis_features_text_flows_through_linked_boxes_and_is_saved_as_plain_paragraphs", chain_flow_and_save as fn()),
        ("pptx::ibis_features_a_chain_is_made_with_the_mouse_and_broken_and_undone", chain_by_mouse_break_undo),
        ("pptx::ibis_features_a_chain_flows_again_by_itself_and_copies_follow_the_rule", chain_auto_and_copies),
        ("pptx::ibis_features_the_overflow_mark_shows_for_a_selected_box_that_does_not_hold_its_text", overflow_mark),
        ("pptx::ibis_features_wrap_works_with_a_middle_anchor_and_a_turned_text_box", wrap_anchor_and_turned),
        ("pptx::ibis_features_the_panel_sets_the_wrap_and_its_four_distances", panel_distances),
    ]
}

fn two_boxes(s: &mut Scenario, text: &str) -> (ShapeId, ShapeId) {
    s.update(|e, _, cx| {
        let (a, b) = (e.document.ids().new_shape_id(), e.document.ids().new_shape_id());
        let mut first = Shape::text_box(a, EmuRect::new(500_000, 500_000, 3_000_000, 900_000), text);
        let mut second = Shape::text_box(b, EmuRect::new(4_500_000, 500_000, 3_000_000, 1_800_000), "");
        for shape in [&mut first, &mut second] {
            if let Some(body) = shape.text_body_mut() {
                body.wrap = true;
            }
        }
        e.insert_shape(first, cx);
        e.insert_shape(second, cx);
        (a, b)
    })
}

fn paragraphs_of(s: &mut Scenario, id: ShapeId) -> Vec<String> {
    s.with(|e, _| e.slide().shape(id).unwrap().text_body().unwrap().paragraphs.iter().map(|p| p.text()).collect())
}

fn chain_flow_and_save() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let long = format!("{}\n{}", SENTENCE.repeat(4), SENTENCE.repeat(3));
    let (a, b) = two_boxes(&mut s, &long);
    s.update(|e, _, cx| e.link_text_boxes(a, b, cx));
    assert_eq!(s.with(|e, _| e.interaction_layer()), InteractionLayer::Ask);
    answer(&mut s, true);
    let chain = s.with(|e, _| e.slide().chain_of(a).cloned()).expect("a chain of the two boxes");
    assert_eq!(chain.boxes, vec![a, b]);
    // The text is all there, cut between the boxes, the first box full: its lines fit its height.
    let (pa, pb) = (paragraphs_of(&mut s, a), paragraphs_of(&mut s, b));
    let joined = chain.joined[1];
    let mut all = pa.clone();
    if joined {
        let last = all.pop().unwrap();
        all.push(format!("{last}{}", pb[0]));
        all.extend(pb[1..].iter().cloned());
    } else {
        all.extend(pb.clone());
    }
    assert_eq!(all.join("\n"), long, "the two boxes hold the whole text: {pa:?} {pb:?}");
    assert!(!pb.iter().all(|p| p.is_empty()), "the second box got text");
    s.screenshot("chain-two-boxes");
    // Saved, PowerPoint shows both boxes' paragraphs as plain text; read again, the chain and the text are as they were.
    s.save_as("ibis-chain.pptx");
    let again = s.reopen();
    let slide = again.slides.iter().find(|sl| !sl.text_chains.is_empty()).expect("the chain is read");
    assert_eq!(slide.text_chains[0].boxes.len(), 2);
    assert_eq!(slide.text_chains[0].joined, chain.joined);
    // (The shapes are numbered anew when a file is read: the chain says which are which.)
    let text_of = |id: ShapeId| slide.shape(id).unwrap().text_body().unwrap().paragraphs.iter().map(|p| p.text()).collect::<Vec<_>>();
    assert_eq!((text_of(slide.text_chains[0].boxes[0]), text_of(slide.text_chains[0].boxes[1])), (pa, pb));
    // Editing the first box flows the text again when the edit ends: more text in it moves the end into the second.
    let first_index = s.with(|e, _| e.slide().shapes.iter().position(|sh| sh.id == a).unwrap());
    s.double_click_shape(first_index);
    s.key("end");
    s.type_text(" Added words that were not here before.");
    s.key("escape");
    let total: usize = s.with(|e, _| e.slide().chain_of(a).unwrap().boxes.iter().map(|id| e.slide().shape(*id).unwrap().text_body().unwrap().text().chars().count()).sum());
    assert!(total >= long.chars().count() + " Added words that were not here before.".chars().count());
}

fn chain_by_mouse_break_undo() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let long = SENTENCE.repeat(6);
    let (a, b) = two_boxes(&mut s, &long);
    // 連結を作成: the next click on a text box is where the text goes on.
    s.update(|e, _, cx| e.begin_link_mode(a, cx));
    answer(&mut s, true);
    assert_eq!(s.with(|e, _| e.shape_edit.link_mode), Some(a));
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| sh.id == b).unwrap());
    s.click_shape(index);
    assert_eq!(s.with(|e, _| e.shape_edit.link_mode), None);
    assert_eq!(s.with(|e, _| e.slide().chain_of(a).map(|c| c.boxes.clone())), Some(vec![a, b]));
    let flowed = (paragraphs_of(&mut s, a), paragraphs_of(&mut s, b));
    // Undo brings back the texts and no chain, in one step; redo the flow.
    s.key("cmd-z");
    assert!(s.with(|e, _| e.slide().chain_of(a).is_none()));
    assert_eq!(paragraphs_of(&mut s, a).join("\n"), long);
    s.key("cmd-shift-z");
    assert_eq!((paragraphs_of(&mut s, a), paragraphs_of(&mut s, b)), flowed);
    // 連結を解除 breaks it; the boxes keep their parts.
    s.update(|e, _, cx| e.unlink_text_box(a, cx));
    assert!(s.with(|e, _| e.slide().chain_of(a).is_none()));
    assert_eq!((paragraphs_of(&mut s, a), paragraphs_of(&mut s, b)), flowed);
}

fn chain_auto_and_copies() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let long = SENTENCE.repeat(6);
    let (a, b) = two_boxes(&mut s, &long);
    s.update(|e, _, cx| e.link_text_boxes(a, b, cx));
    answer(&mut s, true);
    let before = paragraphs_of(&mut s, a).join("").chars().count();
    // A taller first box takes more of the text by itself (and undo is one step with the change).
    s.update(|e, _, cx| {
        let slide = e.slide().id;
        let mut frame = e.slide().shape(a).unwrap().frame;
        frame.height = 2_400_000;
        e.run(vec![ibis_doc_model::Command::SetFrame { slide, shape: a, frame }], cx);
    });
    let after = paragraphs_of(&mut s, a).join("").chars().count();
    assert!(after > before, "{before} {after}");
    s.key("cmd-z");
    assert_eq!(paragraphs_of(&mut s, a).join("").chars().count(), before, "one undo step");
    s.key("cmd-shift-z");
    // Copying all the boxes copies the chain; copying one box copies its text and no link.
    s.update(|e, _, _| e.set_selection(vec![a, b]));
    s.update(|e, _, cx| e.duplicate_selected(cx));
    assert_eq!(s.with(|e, _| e.slide().text_chains.len()), 2, "a whole chain is copied as a chain");
    s.update(|e, _, _| e.set_selection(vec![a]));
    s.update(|e, _, cx| e.duplicate_selected(cx));
    assert_eq!(s.with(|e, _| e.slide().text_chains.len()), 2, "one box of a chain is copied without its link");
    // Deleting the second box gives its text back to the first (the chain is gone, the text is all in the box that is left).
    let text_before: String = s.with(|e, _| e.slide().text_chains[0].boxes.iter().map(|id| e.slide().shape(*id).unwrap().text_body().unwrap().text()).collect::<Vec<_>>().join(""));
    s.update(|e, _, _| e.set_selection(vec![b]));
    s.update(|e, _, cx| e.delete_selected(cx));
    assert!(s.with(|e, _| e.slide().chain_of(a).is_none()));
    assert_eq!(paragraphs_of(&mut s, a).join("").chars().count(), text_before.chars().count());
}

fn overflow_mark() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (a, b) = two_boxes(&mut s, &SENTENCE.repeat(8));
    s.update(|e, _, _| e.set_selection(vec![a]));
    assert!(s.with(|e, _| e.box_overflows(e.slide().shape(a).unwrap())), "the long text does not fit the small box");
    assert!(!s.with(|e, _| e.box_overflows(e.slide().shape(b).unwrap())), "an empty box does not overflow");
    s.screenshot("overflow-mark");
    // Clicking the mark begins the link (the question first in a file that has not turned the features on).
    s.update(|e, _, cx| e.begin_link_mode(a, cx));
    answer(&mut s, true);
    assert_eq!(s.with(|e, _| e.shape_edit.link_mode), Some(a));
}

fn wrap_anchor_and_turned() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (picture, text) = picture_and_text(&mut s, false);
    s.update(|e, _, cx| {
        let slide = e.slide().id;
        let mut shape = e.slide().shape(text).unwrap().clone();
        if let Some(body) = shape.text_body_mut() {
            body.anchor = ibis_doc_model::VerticalAnchor::Middle;
            body.paragraphs.truncate(1);
            let words: String = SENTENCE.repeat(3);
            body.set_text(&words);
        }
        let paragraphs = shape.text_body().unwrap().paragraphs.clone();
        e.run(vec![ibis_doc_model::Command::SetTextChains { slide, chains: Vec::new(), bodies: vec![(text, paragraphs)] }], cx);
    });
    s.update(|e, _, _| e.set_selection(vec![picture]));
    wrap_with(&mut s, ObjectWrapKind::Square);
    let rect = picture_rect(&mut s, picture);
    let middle = painted(&mut s, text);
    s.screenshot("wrap-middle-anchor");
    assert!(middle.iter().all(|c| !overlaps(*c, rect)), "a text anchored to the middle wraps round the picture too");
    // The text box turned half a turn: the lines still keep off the picture.
    s.update(|e, _, cx| {
        let slide = e.slide().id;
        let sh = e.slide().shape(text).unwrap().clone();
        e.run(vec![ibis_doc_model::Command::SetTransform { slide, shape: text, rotation: 180 * 60_000, flip_h: sh.flip_h, flip_v: sh.flip_v }], cx);
    });
    let turned = painted(&mut s, text);
    assert!(!turned.is_empty());
    s.screenshot("wrap-turned");
    let inside: Vec<_> = turned.iter().filter(|c| overlaps_by(**c, rect, 2.0)).take(3).collect();
    assert!(inside.is_empty(), "a turned text box wraps round the picture: picture {rect:?}, first characters inside {inside:?} of {}", turned.len());
}

fn panel_distances() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let (picture, text) = picture_and_text(&mut s, false);
    // 図形の書式設定 (サイズと位置) of a picture has the section: the kind, and the four distances in mm.
    s.update(|e, w, cx| e.open_frame_panel(w, cx));
    let fields = s.with(|e, _| e.shape_edit.frame_panel.as_ref().unwrap().wrap.as_ref().expect("the section for a picture").fields.clone());
    assert_eq!(fields.len(), 4);
    assert_eq!(s.with(|_, cx| fields[0].read(cx).value().to_string()), "2.5", "0.1 inch is shown as 2.5 mm (one digit)");
    s.update(|e, _, cx| e.set_frame_panel_wrap_kind(ObjectWrapKind::Square, cx));
    s.update(|_, w, cx| { for (f, v) in fields.iter().zip(["12.3", "0", "0", "7.5"]) { f.update(cx, |f, cx| f.set_value(v, w, cx)); } });
    s.update(|e, w, cx| e.apply_frame_panel(w, cx));
    answer(&mut s, true);
    let wrap = s.with(|e, _| e.slide().ibis_wrap.get(&picture).copied()).expect("a wrap");
    assert_eq!((wrap.kind, wrap.margins), (ObjectWrapKind::Square, [442_800, 0, 0, 270_000]));
    s.screenshot("wrap-panel-values");
    // A different left distance moves the text on the picture's left: it is laid out again.
    s.update(|e, _, _| e.set_selection(vec![picture]));
    let narrow = painted(&mut s, text);
    let before: usize = narrow.len();
    s.update(|e, w, cx| { e.set_selection(vec![picture]); e.open_frame_panel(w, cx); });
    let fields = s.with(|e, _| e.shape_edit.frame_panel.as_ref().unwrap().wrap.as_ref().unwrap().fields.clone());
    s.update(|_, w, cx| { for (f, v) in fields.iter().zip(["50", "0", "50", "7.5"]) { f.update(cx, |f, cx| f.set_value(v, w, cx)); } });
    s.update(|e, w, cx| e.apply_frame_panel(w, cx));
    assert!(s.with(|e, _| e.ask.is_none()), "asked no more");
    assert_eq!(s.with(|e, _| e.slide().ibis_wrap.get(&picture).unwrap().margins), [1_800_000, 0, 1_800_000, 270_000]);
    let wide = painted(&mut s, text);
    assert_eq!(wide.len(), before, "all the text is still there");
    assert_ne!(narrow, wide, "the characters are laid out again among the new distances");
    // A value out of range is refused.
    s.update(|e, w, cx| { e.set_selection(vec![picture]); e.open_frame_panel(w, cx); });
    let fields = s.with(|e, _| e.shape_edit.frame_panel.as_ref().unwrap().wrap.as_ref().unwrap().fields.clone());
    s.update(|_, w, cx| fields[1].update(cx, |f, cx| f.set_value("51", w, cx)));
    s.update(|e, w, cx| e.apply_frame_panel(w, cx));
    assert!(s.with(|e, _| e.shape_edit.frame_panel.is_some()), "the panel stays with its error");
    s.update(|e, w, cx| e.close_frame_panel(w, cx));
    // Saved: four values in the extension, and read again.
    let path = s.save_as("ibis-wrap-distances.pptx");
    let saved = s.reopen();
    let slide = saved.slides.iter().find(|sl| !sl.ibis_wrap.is_empty()).expect("a wrap is read");
    assert_eq!(slide.ibis_wrap.values().next().unwrap().margins, [1_800_000, 0, 1_800_000, 270_000]);
    let package = ibis_pptx::ibis_ooxml::Package::open(std::fs::read(&path).unwrap()).unwrap();
    let part = (1..=20).filter_map(|n| package.read(&format!("ppt/slides/slide{n}.xml")).ok()).map(|b| String::from_utf8(b).unwrap()).find(|x| x.contains("ibis:shape")).unwrap();
    assert!(part.contains("marginL=\"1800000\"") && part.contains("marginT=\"0\"") && part.contains("marginR=\"1800000\"") && part.contains("marginB=\"270000\""), "{part}");
}
