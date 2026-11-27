// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A touch that a second finger cancels (#209; `tasks/doc/design/touch-gesture-transaction.md`): gpui's
//! `TouchSessionEvent` opens a transaction at the first finger down, takes back everything at `Cancelled` (the document's
//! edits, the selection, the drag, the zoom, the scroll, the format's own state), and makes the edits one undo step at
//! `Ended`. Plus the shared pinch tracking (`gpui::PinchTracker`) that the platforms feed their fingers to.
use gpui_kit::*;
use ibis_doc_model::{AutoShape, Color, Command, EmuRect, Fill, Geometry, Shape, ShapeId, ShapeKind};

use ibis_office_docx::ext::*;
use ibis_office_xlsx::ext::*;
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::touch_gesture_cancel_puts_selection_and_drag_back", cancel_puts_selection_and_drag_back),
        ("shell::touch_gesture_cancel_takes_back_the_documents_edits", cancel_takes_back_the_documents_edits),
        ("shell::touch_gesture_ended_is_one_undo_step", ended_is_one_undo_step),
        ("shell::touch_gesture_cancel_puts_zoom_and_scroll_back", cancel_puts_zoom_and_scroll_back),
        ("shell::touch_gesture_cancel_drops_what_after_gesture_queued_and_ended_runs_it", after_gesture_runs_only_when_final),
        ("shell::touch_gesture_cancel_puts_back_the_sheets_cell_and_the_documents_caret", cancel_puts_back_each_formats_selection),
        ("shell::touch_gesture_a_session_outside_the_page_changes_nothing", a_session_outside_the_page_opens_nothing),
        ("shell::touch_gesture_the_raw_touch_of_the_web_makes_a_session", a_raw_touch_makes_a_session),
        ("shell::touch_pinch_second_finger_starts_a_pinch", pinch_second_finger_starts_a_pinch),
        ("shell::touch_pinch_remaining_finger_is_not_a_single_touch_until_all_are_lifted", pinch_remaining_finger_ignored),
        ("shell::touch_pinch_third_finger_and_a_finished_pinch", pinch_third_finger_and_a_finished_pinch),
    ]
}

/// A new presentation with two rectangles put on its slide.
fn fixture() -> (Scenario, Vec<ShapeId>) {
    let mut s = Scenario::new_document(ibis_formats::Kind::Presentation);
    s.size(1200., 900.);
    let ids = s.update(|e, _, cx| {
        let mut ids = Vec::new();
        for i in 0..2 {
            let id = e.document.ids().new_shape_id();
            let frame = EmuRect::new(500_000 + i * 1_000_000, 500_000, 800_000, 600_000);
            e.insert_shape(Shape::new(id, frame, ShapeKind::Auto(AutoShape::new(Geometry::Rect, Fill::Solid(Color::BLACK), None))), cx);
            ids.push(id);
        }
        e.set_selection(Vec::new());
        cx.notify();
        ids
    });
    s.settle();
    (s, ids)
}

fn middle(s: &mut Scenario, id: ShapeId) -> Point<Pixels> {
    let index = s.with(|e, _| e.slide().shapes.iter().position(|shape| shape.id == id)).expect("the shape is on the slide");
    let (x, y) = s.shape_position(index);
    point(px(x), px(y))
}

fn frame(s: &mut Scenario, id: ShapeId) -> EmuRect {
    s.with(|e, _| e.slide().shape(id).expect("the shape is on the slide").frame)
}

fn session(phase: TouchPhase, position: Point<Pixels>) -> PlatformInput {
    PlatformInput::TouchSession(TouchSessionEvent { phase, position })
}

fn press_and_drag(position: Point<Pixels>, to: Point<Pixels>) -> Vec<PlatformInput> {
    vec![
        PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }),
        PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }),
        PlatformInput::MouseMove(MouseMoveEvent { position: to, pressed_button: Some(MouseButton::Left), ..Default::default() }),
    ]
}

fn move_frame(s: &mut Scenario, id: ShapeId, dx: i64) {
    s.update(move |e, _, cx| {
        let slide = e.current_slide;
        let f = e.slide().shape(id).unwrap().frame;
        e.execute(vec![Command::SetFrame { slide, frames: vec![(id, EmuRect::new(f.x + dx, f.y, f.width, f.height))], with_attached: true }], cx);
    });
}

fn cancel_puts_selection_and_drag_back() {
    let (mut s, ids) = fixture();
    let before = frame(&mut s, ids[0]);
    let from = middle(&mut s, ids[0]);
    let to = point(from.x + px(80.), from.y + px(60.));
    assert!(s.with(|e, _| e.shape_edit.selection.is_empty() && e.shape_edit.drag.is_none()));

    // The first finger: a press on the shape and a drag. The second finger takes it back.
    let mut events = vec![session(TouchPhase::Started, from)];
    events.extend(press_and_drag(from, to));
    s.dispatch(events);
    assert!(s.with(|e, _| e.gesture_is_open()), "the touch opened a gesture");
    assert!(s.with(|e, _| !e.shape_edit.selection.is_empty() && e.shape_edit.drag.is_some()), "the press selected the shape and began the drag");

    s.dispatch(vec![session(TouchPhase::Cancelled, to)]);
    assert!(s.with(|e, _| !e.gesture_is_open()));
    assert!(s.with(|e, _| e.shape_edit.selection.is_empty()), "the selection is as it was");
    assert!(s.with(|e, _| e.shape_edit.drag.is_none() && e.drag.is_none()), "the drag is gone");
    assert_eq!(frame(&mut s, ids[0]), before);
    // (No MouseUp comes for a cancelled touch; if one did, nothing would move.)
    s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: to, click_count: 1, ..Default::default() })]);
    assert_eq!(frame(&mut s, ids[0]), before, "a stray release moves nothing");
}

fn cancel_takes_back_the_documents_edits() {
    let (mut s, ids) = fixture();
    let before = frame(&mut s, ids[0]);
    let steps = s.with(|e, _| e.document.undo_len());
    let at = middle(&mut s, ids[0]);
    s.dispatch(vec![session(TouchPhase::Started, at)]);
    move_frame(&mut s, ids[0], 123_456);
    move_frame(&mut s, ids[0], 1_000);
    assert_ne!(frame(&mut s, ids[0]), before);
    s.dispatch(vec![session(TouchPhase::Cancelled, at)]);
    assert_eq!(frame(&mut s, ids[0]), before, "the edits are taken back");
    assert_eq!(s.with(|e, _| e.document.undo_len()), steps, "and leave no undo step");
    assert!(s.with(|e, _| !e.document.can_redo()), "and nothing to redo");
}

fn ended_is_one_undo_step() {
    let (mut s, ids) = fixture();
    let before = frame(&mut s, ids[0]);
    let steps = s.with(|e, _| e.document.undo_len());
    let at = middle(&mut s, ids[0]);
    s.dispatch(vec![session(TouchPhase::Started, at)]);
    move_frame(&mut s, ids[0], 5_000);
    move_frame(&mut s, ids[0], 7_000);
    s.dispatch(vec![session(TouchPhase::Ended, at)]);
    assert_eq!(frame(&mut s, ids[0]).x, before.x + 12_000, "the edits stay");
    assert_eq!(s.with(|e, _| e.document.undo_len()), steps + 1, "as one undo step");
    s.update(|e, window, cx| e.undo(&crate::shell::actions::Undo, window, cx));
    assert_eq!(frame(&mut s, ids[0]), before, "Undo once takes all of the touch back");
}

fn cancel_puts_zoom_and_scroll_back() {
    let (mut s, ids) = fixture();
    let at = middle(&mut s, ids[0]);
    let (zoom, scroll) = s.with(|e, _| (e.zoom, e.workspace_scroll.offset()));
    s.dispatch(vec![session(TouchPhase::Started, at)]);
    s.update(|e, window, cx| {
        e.set_zoom(Some(2.5), window, cx);
        e.workspace_scroll.set_offset(point(px(-40.), px(-30.)));
    });
    s.settle();
    assert_ne!(s.with(|e, _| e.zoom), zoom, "the touch zoomed");
    s.dispatch(vec![session(TouchPhase::Cancelled, at)]);
    assert_eq!(s.with(|e, _| e.zoom), zoom);
    assert_eq!(s.with(|e, _| e.workspace_scroll.offset()), scroll);
}

fn after_gesture_runs_only_when_final() {
    use std::{cell::Cell, rc::Rc};
    let (mut s, ids) = fixture();
    let at = middle(&mut s, ids[0]);
    let ran = Rc::new(Cell::new(0u32));
    // Cancelled: dropped.
    s.dispatch(vec![session(TouchPhase::Started, at)]);
    let r = ran.clone();
    s.update(move |e, window, cx| e.after_gesture(window, cx, move |_, _, _| r.set(r.get() + 1)));
    assert_eq!(ran.get(), 0, "it waits for the lift");
    s.dispatch(vec![session(TouchPhase::Cancelled, at)]);
    assert_eq!(ran.get(), 0, "a cancelled touch never runs it");
    // Ended: runs once.
    s.dispatch(vec![session(TouchPhase::Started, at)]);
    let r = ran.clone();
    s.update(move |e, window, cx| e.after_gesture(window, cx, move |_, _, _| r.set(r.get() + 1)));
    s.dispatch(vec![session(TouchPhase::Ended, at)]);
    assert_eq!(ran.get(), 1);
    // No gesture open: at once.
    let r = ran.clone();
    s.update(move |e, window, cx| e.after_gesture(window, cx, move |_, _, _| r.set(r.get() + 1)));
    assert_eq!(ran.get(), 2);
}

fn cancel_puts_back_each_formats_selection() {
    // The spreadsheet screen: the selected cell.
    let mut s = Scenario::new_document(ibis_formats::Kind::Workbook);
    s.size(1200., 900.);
    s.settle();
    let at = s.with(|e, _| e.workspace_bounds).map(|b| b.center()).expect("the workspace is laid out");
    let sheet = s.with(|e, _| e.current_slide);
    let before = s.with(|e, _| e.sheet_state().map(|st| st.cells.get(&sheet).copied()));
    s.dispatch(vec![session(TouchPhase::Started, at)]);
    s.update(move |e, _, _| {
        if let Some(st) = e.sheet_state_mut() {
            st.cells.insert(sheet, (7, 5));
        }
    });
    s.dispatch(vec![session(TouchPhase::Cancelled, at)]);
    assert_eq!(s.with(|e, _| e.sheet_state().map(|st| st.cells.get(&sheet).copied())), before, "the cell is as it was");
    drop(s); // (one Scenario at a time)

    // The document screen: the caret.
    let mut s = Scenario::new_document(ibis_formats::Kind::Document);
    s.size(1200., 900.);
    s.settle();
    let at = s.with(|e, _| e.workspace_bounds).map(|b| b.center()).expect("the workspace is laid out");
    let before = s.with(|e, _| e.doc_state().map(|st| (st.caret, st.anchor)));
    s.dispatch(vec![session(TouchPhase::Started, at)]);
    s.update(|e, _, _| {
        if let Some(st) = e.doc_state_mut() {
            st.caret = Some(ibis_doc_model::flow_command::FlowPos::new(0, 3));
            st.anchor = Some(ibis_doc_model::flow_command::FlowPos::new(0, 1));
        }
    });
    s.dispatch(vec![session(TouchPhase::Cancelled, at)]);
    assert_eq!(s.with(|e, _| e.doc_state().map(|st| (st.caret, st.anchor))), before);
}

fn a_session_outside_the_page_opens_nothing() {
    let (mut s, ids) = fixture();
    let before = frame(&mut s, ids[0]);
    // The title row is not the page: a touch there is not a gesture of the page.
    s.dispatch(vec![session(TouchPhase::Started, point(px(5.), px(5.)))]);
    assert!(s.with(|e, _| !e.gesture_is_open()));
    move_frame(&mut s, ids[0], 4_000);
    s.dispatch(vec![session(TouchPhase::Cancelled, point(px(5.), px(5.)))]);
    assert_eq!(frame(&mut s, ids[0]).x, before.x + 4_000, "nothing is taken back");
}

fn a_raw_touch_makes_a_session() {
    // The web (and any platform that delivers raw touches): gpui makes the session from the touch events.
    let (mut s, ids) = fixture();
    let before = frame(&mut s, ids[0]);
    let at = middle(&mut s, ids[0]);
    let touch = |id: u64, phase: TouchPhase, position: Point<Pixels>| PlatformInput::Touch(TouchEvent { id: TouchId(id), phase, position, predicted_position: None, force: None });
    s.dispatch(vec![touch(1, TouchPhase::Started, at)]);
    assert!(s.with(|e, _| e.gesture_is_open()), "the first finger opens a gesture");
    move_frame(&mut s, ids[0], 9_000);
    // The second finger comes (a pinch); the platform cancels the first.
    s.dispatch(vec![touch(2, TouchPhase::Started, point(at.x + px(50.), at.y)), touch(1, TouchPhase::Cancelled, at)]);
    assert!(s.with(|e, _| !e.gesture_is_open()));
    assert_eq!(frame(&mut s, ids[0]), before, "what the first finger did is taken back");
    s.dispatch(vec![touch(2, TouchPhase::Ended, point(at.x + px(50.), at.y))]);
}

// ----- the shared pinch tracking -----

fn pt(x: f32, y: f32) -> Point<Pixels> {
    point(px(x), px(y))
}

fn pinch_second_finger_starts_a_pinch() {
    let mut t = PinchTracker::default();
    assert_eq!(t.down(1, pt(100.0, 100.0)), PinchDown::Single);
    assert_eq!(t.down(2, pt(200.0, 100.0)), PinchDown::Begin { cancelled: (1, pt(100.0, 100.0)), center: pt(150.0, 100.0) });
    assert!(t.is_pinching());
    // distance 100 -> 150: +50%, the center moves with the fingers
    assert_eq!(t.moved(2, pt(250.0, 100.0)), PinchMove::Update { center: pt(175.0, 100.0), delta: 0.5 });
    // 150 -> 75 (finger 1 moves in): -50%
    assert_eq!(t.moved(1, pt(175.0, 100.0)), PinchMove::Update { center: pt(212.5, 100.0), delta: -0.5 });
    assert_eq!(t.up(2, pt(250.0, 100.0)), PinchUp::End { center: pt(212.5, 100.0) });
    assert!(!t.is_pinching());
}

fn pinch_remaining_finger_ignored() {
    let mut t = PinchTracker::default();
    t.down(1, pt(0.0, 0.0));
    t.down(2, pt(10.0, 0.0));
    assert!(matches!(t.up(2, pt(10.0, 0.0)), PinchUp::End { .. }));
    assert_eq!(t.moved(1, pt(5.0, 5.0)), PinchMove::Ignored);
    assert_eq!(t.up(1, pt(5.0, 5.0)), PinchUp::Ignored);
    // Every finger is up: a new touch is a single one again.
    assert_eq!(t.down(3, pt(1.0, 1.0)), PinchDown::Single);
    assert_eq!(t.moved(3, pt(2.0, 2.0)), PinchMove::Single);
    assert_eq!(t.up(3, pt(2.0, 2.0)), PinchUp::Single);
}

fn pinch_third_finger_and_a_finished_pinch() {
    let mut t = PinchTracker::default();
    t.down(1, pt(0.0, 0.0));
    t.down(2, pt(0.0, 10.0));
    assert_eq!(t.down(3, pt(50.0, 50.0)), PinchDown::Ignored);
    assert_eq!(t.moved(3, pt(60.0, 60.0)), PinchMove::Ignored);
    assert_eq!(t.up(3, pt(60.0, 60.0)), PinchUp::Ignored);
    assert!(matches!(t.moved(2, pt(0.0, 20.0)), PinchMove::Update { .. }));
    // A pinch that ended does not start again with a finger left: a new finger is not a second finger.
    t.up(2, pt(0.0, 20.0));
    assert_eq!(t.down(4, pt(30.0, 30.0)), PinchDown::Ignored);
    assert!(!t.is_pinching());
    // Two fingers on the same spot: no division by zero.
    let mut t = PinchTracker::default();
    t.down(1, pt(10.0, 10.0));
    t.down(2, pt(10.0, 10.0));
    assert_eq!(t.moved(2, pt(20.0, 10.0)), PinchMove::Update { center: pt(15.0, 10.0), delta: 0.0 });
    t.clear();
    assert!(!t.has_fingers());
}
