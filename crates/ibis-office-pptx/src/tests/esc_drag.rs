// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Esc that reaches the window's own handler (`SlideEditor::cancel_tool`: the focus is not in the slide's workspace, e.g.
//! in the slide list) drops the drag in progress: the divider's drag gives the slide list its width back, a shape stays
//! where it was, and the rubber band leaves the selection as it was before. (The crate split's stage 2.5-C2 once took
//! this away from the presentation screen: the call was moved to a hook that only the sheet screen ran.)
//!
//! While the focus is in the slide's workspace, Esc goes to the workspace's own handler (pptx/slide_view.rs), which
//! does not drop a drag (it ends the text edit, the tool, or the selection); that is older than the split and is not
//! what these check. So the two checks of a drag on the slide move the focus to the slide list by hand first.
use gpui_kit::*;
use ibis_doc_model::{AutoShape, Color, EmuRect, Fill, Geometry, Shape, ShapeId, ShapeKind};
use crate::shell::scenario::Scenario;
use crate::shell::shape_state::Drag;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::esc_gives_the_slide_list_its_width_back", esc_gives_the_slide_list_its_width_back),
        ("pptx::esc_at_the_window_drops_the_move_of_a_shape", esc_at_the_window_drops_the_move_of_a_shape),
        ("pptx::esc_at_the_window_drops_the_rubber_band", esc_at_the_window_drops_the_rubber_band),
    ]
}

/// A new presentation with two rectangles put on its slide.
fn fixture() -> (Scenario, Vec<ShapeId>) {
    let mut s = Scenario::new_document(ibis_pptx::Kind::Presentation);
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

fn frame(s: &mut Scenario, id: ShapeId) -> EmuRect {
    s.with(|e, _| e.slide().shape(id).expect("the shape is on the slide").frame)
}

/// Where the middle of shape `id` is, in the window.
fn middle(s: &mut Scenario, id: ShapeId) -> Point<Pixels> {
    let index = s.with(|e, _| e.slide().shapes.iter().position(|shape| shape.id == id)).expect("the shape is on the slide");
    let (x, y) = s.shape_position(index);
    point(px(x), px(y))
}

/// A point of the slide that no shape covers, in the window.
fn free_point(s: &mut Scenario) -> Point<Pixels> {
    s.with(|e, _| {
        let layout = e.slide_layout().expect("the slide is laid out");
        let size = e.presentation().slide_size;
        let margin = 150_000;
        let covered = |x: i64, y: i64| {
            e.slide().shapes.iter().any(|shape| {
                let f = shape.frame;
                x >= f.x - margin && x <= f.x + f.width + margin && y >= f.y - margin && y <= f.y + f.height + margin
            })
        };
        let (columns, rows) = (24, 14);
        for row in (1..rows).rev() {
            for column in (1..columns).rev() {
                let (x, y) = (size.width * column / columns, size.height * row / rows);
                if !covered(x, y) {
                    return point(layout.origin.x + layout.to_px(x), layout.origin.y + layout.to_px(y));
                }
            }
        }
        panic!("the slide has no free point");
    })
}

/// The slide list takes the focus, as after a press on it: Esc then reaches the window's handler.
fn focus_the_slide_list(s: &mut Scenario) {
    s.update(|e, window, cx| e.list_focus.focus(window, cx));
    s.settle();
}

fn esc_gives_the_slide_list_its_width_back() {
    let (mut s, _) = fixture();
    focus_the_slide_list(&mut s);
    let before = s.with(|e, _| e.pane_width_now());
    // The divider is the 4 px left of the workspace.
    let workspace = s.with(|e, _| e.workspace_bounds).expect("the workspace is laid out");
    let from = point(workspace.origin.x - px(2.), workspace.origin.y + workspace.size.height / 2.);
    let to = point(from.x + px(60.), from.y);
    s.dispatch(vec![
        PlatformInput::MouseMove(MouseMoveEvent { position: from, ..Default::default() }),
        PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: from, click_count: 1, ..Default::default() }),
        PlatformInput::MouseMove(MouseMoveEvent { position: to, pressed_button: Some(MouseButton::Left), ..Default::default() }),
    ]);
    assert!(s.with(|e, _| e.pptx().is_some_and(|p| p.pane_drag.is_some())), "the divider is held");
    assert_ne!(s.with(|e, _| e.pane_width_now()), before, "the drag widens the slide list");
    s.key("escape");
    assert!(s.with(|e, _| e.pptx().is_some_and(|p| p.pane_drag.is_none())), "Esc drops the divider");
    assert_eq!(s.with(|e, _| e.pane_width_now()), before, "the slide list has its width back");
    s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: to, click_count: 1, ..Default::default() })]);
    s.settle();
    assert_eq!(s.with(|e, _| e.pane_width_now()), before, "releasing the button changes nothing");
}

fn esc_at_the_window_drops_the_move_of_a_shape() {
    let (mut s, ids) = fixture();
    let before = frame(&mut s, ids[0]);
    let from = middle(&mut s, ids[0]);
    let to = point(from.x + px(80.), from.y + px(60.));
    s.dispatch(vec![
        PlatformInput::MouseMove(MouseMoveEvent { position: from, ..Default::default() }),
        PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: from, click_count: 1, ..Default::default() }),
        PlatformInput::MouseMove(MouseMoveEvent { position: to, pressed_button: Some(MouseButton::Left), ..Default::default() }),
    ]);
    focus_the_slide_list(&mut s);
    assert!(s.with(|e, _| matches!(e.shape_edit.drag, Some(Drag::Move { .. }))), "the shape is being moved");
    s.key("escape");
    assert!(s.with(|e, _| e.shape_edit.drag.is_none()), "Esc drops the drag");
    s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: to, click_count: 1, ..Default::default() })]);
    s.settle();
    assert_eq!(frame(&mut s, ids[0]), before, "the shape stays where it was");
    // The same drag without Esc moves it: the check above is not passed by a drag that never began.
    s.dispatch(vec![
        PlatformInput::MouseMove(MouseMoveEvent { position: from, ..Default::default() }),
        PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: from, click_count: 1, ..Default::default() }),
        PlatformInput::MouseMove(MouseMoveEvent { position: to, pressed_button: Some(MouseButton::Left), ..Default::default() }),
        PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: to, click_count: 1, ..Default::default() }),
    ]);
    s.settle();
    assert_ne!(frame(&mut s, ids[0]), before, "without Esc the drag moves the shape");
}

fn esc_at_the_window_drops_the_rubber_band() {
    let (mut s, ids) = fixture();
    s.update(|e, _, cx| {
        e.set_selection(vec![ids[0]]);
        cx.notify();
    });
    s.settle();
    let shift = Modifiers { shift: true, ..Default::default() };
    let from = free_point(&mut s);
    let to = point(from.x - px(40.), from.y - px(30.));
    s.dispatch(vec![
        PlatformInput::MouseMove(MouseMoveEvent { position: from, modifiers: shift, ..Default::default() }),
        PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: from, click_count: 1, modifiers: shift, ..Default::default() }),
        PlatformInput::MouseMove(MouseMoveEvent { position: to, pressed_button: Some(MouseButton::Left), modifiers: shift, ..Default::default() }),
    ]);
    focus_the_slide_list(&mut s);
    assert!(s.with(|e, _| matches!(e.shape_edit.drag, Some(Drag::Band { .. }))), "a rubber band is being drawn");
    s.key("escape");
    assert!(s.with(|e, _| e.shape_edit.drag.is_none()), "Esc drops the rubber band");
    assert_eq!(s.with(|e, _| e.selected_ids()), vec![ids[0]], "the selection is the one before the band");
    s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: to, click_count: 1, modifiers: shift, ..Default::default() })]);
    s.settle();
    assert_eq!(s.with(|e, _| e.selected_ids()), vec![ids[0]], "releasing the button changes nothing");
}
