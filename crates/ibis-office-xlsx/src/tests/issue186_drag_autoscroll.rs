// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #186: selecting with the mouse past the edge of the cells did not scroll the view. As in Excel, the view scrolls
//! on a timer while the button is held at or past the edge (the beats are driven by hand here), the farther the pointer the more
//! cells a beat passes, and the selection follows to the cell at the edge.
use super::SheetSteps;
use crate::shell::scenario::Scenario;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput, point, px};
use ibis_doc_model::{Command, SheetCommand};
use crate::ext::*;

fn press(s: &mut Scenario, (x, y): (f32, f32)) {
    let position = point(px(x), px(y));
    s.dispatch(vec![
        PlatformInput::MouseMove(MouseMoveEvent { position, ..Default::default() }),
        PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() }),
    ]);
}

fn hold_at(s: &mut Scenario, (x, y): (f32, f32)) {
    s.dispatch(vec![PlatformInput::MouseMove(MouseMoveEvent { position: point(px(x), px(y)), pressed_button: Some(MouseButton::Left), ..Default::default() })]);
}

fn release(s: &mut Scenario, (x, y): (f32, f32)) {
    s.dispatch(vec![PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: point(px(x), px(y)), click_count: 1, ..Default::default() })]);
}

fn beat(s: &mut Scenario) -> bool {
    s.update(|e, _, cx| e.drag_autoscroll_beat(cx))
}

fn offset(s: &mut Scenario) -> (f32, f32) {
    s.with(|e, _| {
        let o = e.workspace_scroll.offset();
        (f32::from(o.x), f32::from(o.y))
    })
}

fn range(s: &mut Scenario) -> ibis_doc_model::CellRange {
    s.with(|e, _| e.selected_range().expect("a selection"))
}

/// The right and bottom edges the scrolling starts at: the cells' area less the room of the scrollbars.
fn edges(s: &mut Scenario) -> (f32, f32) {
    s.with(|e, _| {
        let ws = e.workspace_bounds.expect("laid out");
        (f32::from(ws.origin.x + ws.size.width) - crate::sheet_ui::SCROLLBAR_ROOM, f32::from(ws.origin.y + ws.size.height) - crate::sheet_ui::SCROLLBAR_ROOM)
    })
}

fn book() -> Scenario {
    let mut s = super::new_sheet();
    s.size(900., 700.);
    s
}

pub(crate) fn step_grows_with_the_distance() {
    assert_eq!(crate::drag_autoscroll::step_for(-1.0), 0);
    assert_eq!(crate::drag_autoscroll::step_for(0.0), 1);
    assert!(crate::drag_autoscroll::step_for(100.0) > crate::drag_autoscroll::step_for(30.0));
    assert!(crate::drag_autoscroll::step_for(1.0e6) <= 40);
}

pub(crate) fn cell_drag_below_the_edge_scrolls_and_grows() {
    let mut s = book();
    s.click_cell("B2");
    let start = s.with(|e, _| e.cell_center(1, 1)).unwrap();
    let (_, bottom) = edges(&mut s);
    press(&mut s, (f32::from(start.x), f32::from(start.y)));
    // The pointer is held below the cells, not moving.
    let held = (f32::from(start.x), bottom + 6.0);
    hold_at(&mut s, held);
    assert!(s.with(|e, _| e.sheet_state().unwrap().drag_scroll.is_some()), "the timer is running");
    let (_, y0) = offset(&mut s);
    let r0 = range(&mut s).last_row;
    for _ in 0..10 {
        assert!(beat(&mut s));
    }
    let (_, y1) = offset(&mut s);
    let r1 = range(&mut s).last_row;
    assert!(y1 < y0 - 100.0, "the view scrolled down: {y0} -> {y1}");
    assert!(r1 > r0 + 8, "the selection grew: {r0} -> {r1}");
    assert_eq!(range(&mut s).first_row, 1, "from the cell pressed");
    for _ in 0..10 {
        beat(&mut s);
    }
    assert!(range(&mut s).last_row > r1, "and goes on while the pointer rests there");
    let before = range(&mut s).last_row;
    release(&mut s, held);
    assert!(!beat(&mut s), "the beats stop with the button");
    assert_eq!(range(&mut s).last_row, before);
}

pub(crate) fn the_farther_the_pointer_the_more_rows_a_beat() {
    let mut s = book();
    s.click_cell("B2");
    let start = s.with(|e, _| e.cell_center(1, 1)).unwrap();
    let (_, bottom) = edges(&mut s);
    let x = f32::from(start.x);
    press(&mut s, (x, f32::from(start.y)));
    hold_at(&mut s, (x, bottom + 1.0));
    let before = range(&mut s).last_row;
    beat(&mut s);
    let near = range(&mut s).last_row - before;
    // As far past the edge as the window goes (the whole of the status bar).
    let window_bottom = s.with(|e, _| f32::from(e.workspace_bounds.unwrap().origin.y + e.workspace_bounds.unwrap().size.height)) + 20.0;
    hold_at(&mut s, (x, window_bottom));
    let before = range(&mut s).last_row;
    beat(&mut s);
    let far = range(&mut s).last_row - before;
    assert!(near >= 1 && far > near, "rows a beat: near {near}, far {far}");
    release(&mut s, (x, window_bottom));
}

pub(crate) fn cell_drag_inside_does_not_scroll() {
    let mut s = book();
    s.click_cell("B2");
    let start = s.with(|e, _| e.cell_center(1, 1)).unwrap();
    press(&mut s, (f32::from(start.x), f32::from(start.y)));
    let to = s.with(|e, _| e.cell_center(3, 4)).unwrap();
    hold_at(&mut s, (f32::from(to.x), f32::from(to.y)));
    assert!(s.with(|e, _| e.sheet_state().unwrap().drag_scroll.is_none()));
    assert!(!beat(&mut s));
    assert_eq!(offset(&mut s), (0.0, 0.0));
    assert_eq!((range(&mut s).last_column, range(&mut s).last_row), (3, 4));
    release(&mut s, (f32::from(to.x), f32::from(to.y)));
}

pub(crate) fn cell_drag_past_the_right_edge_scrolls_right() {
    let mut s = book();
    s.click_cell("B2");
    let start = s.with(|e, _| e.cell_center(1, 1)).unwrap();
    let (right, _) = edges(&mut s);
    press(&mut s, (f32::from(start.x), f32::from(start.y)));
    hold_at(&mut s, (right + 4.0, f32::from(start.y)));
    for _ in 0..8 {
        beat(&mut s);
    }
    let (x, y) = offset(&mut s);
    assert!(x < -100.0 && y == 0.0, "scrolled right only: {x}, {y}");
    assert!(range(&mut s).last_column > 8);
    assert_eq!(range(&mut s).last_row, 1, "the row did not change: the pointer is on it");
    release(&mut s, (right + 4.0, f32::from(start.y)));
}

pub(crate) fn column_header_drag_scrolls_horizontally() {
    let mut s = book();
    let b = s.header("B");
    let (right, _) = edges(&mut s);
    press(&mut s, b);
    hold_at(&mut s, (right + 4.0, b.1));
    assert!(s.with(|e, _| e.sheet_state().unwrap().drag_scroll.is_some()));
    let e0 = range(&mut s).last_column;
    for _ in 0..30 {
        beat(&mut s);
    }
    let r = range(&mut s);
    assert!(r.last_column > e0 + 25, "whole columns grew to the right: {e0} -> {}", r.last_column);
    assert_eq!(r.first_column, 1);
    assert!(offset(&mut s).0 < -100.0, "the view followed");
    release(&mut s, (right + 4.0, b.1));
    assert!(!beat(&mut s));
}

pub(crate) fn row_header_drag_scrolls_vertically() {
    let mut s = book();
    let r2 = s.header("2");
    let (_, bottom) = edges(&mut s);
    press(&mut s, r2);
    hold_at(&mut s, (r2.0, bottom + 4.0));
    for _ in 0..30 {
        beat(&mut s);
    }
    let r = range(&mut s);
    assert!(r.last_row > 25 && r.first_row == 1, "rows 2.. grew: {:?}", r);
    assert!(offset(&mut s).1 < -100.0);
    // Dragging back up past the top scrolls up again.
    let top = s.with(|e, _| f32::from(e.workspace_bounds.unwrap().origin.y));
    hold_at(&mut s, (r2.0, top + 2.0));
    let (_, y_down) = offset(&mut s);
    for _ in 0..4 {
        beat(&mut s);
    }
    assert!(offset(&mut s).1 > y_down, "scrolled back up");
    release(&mut s, (r2.0, top + 2.0));
}

pub(crate) fn frozen_rows_stay_and_the_drag_scrolls_the_rest() {
    let mut s = book();
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run_sheet(vec![Command::Sheet(SheetCommand::Freeze { slide, frozen: Some((1, 2)) })], cx);
    });
    s.settle();
    s.click_cell("C5");
    let start = s.with(|e, _| e.cell_center(2, 4)).unwrap();
    let (_, bottom) = edges(&mut s);
    press(&mut s, (f32::from(start.x), f32::from(start.y)));
    hold_at(&mut s, (f32::from(start.x), bottom + 6.0));
    for _ in 0..6 {
        beat(&mut s);
    }
    assert!(range(&mut s).last_row > 15 && range(&mut s).first_row == 4);
    // The frozen rows are still the first two: A1 stays where it was.
    let a1 = s.with(|e, _| e.cell_center(0, 0)).unwrap();
    assert!(f32::from(a1.y) > 0.0 && f32::from(a1.y) < 60.0, "the frozen row 1 is still at the top: {:?}", a1);
    // Dragging up past the frozen rows' lower edge scrolls back up.
    let top = s.with(|e, _| {
        let c = e.workspace_content().unwrap();
        f32::from(e.workspace_bounds.unwrap().origin.y) + e.frozen_split(c).unwrap().1.unwrap()
    });
    hold_at(&mut s, (f32::from(start.x), top - 4.0));
    let (_, y) = offset(&mut s);
    for _ in 0..4 {
        beat(&mut s);
    }
    assert!(offset(&mut s).1 > y, "back up");
    release(&mut s, (f32::from(start.x), top - 4.0));
}

pub(crate) fn fill_handle_drag_below_the_edge_scrolls() {
    let mut s = book();
    s.click_cell("B2");
    s.enter("1", "enter");
    s.click_cell("B2");
    let (_, bottom) = edges(&mut s);
    let a = s.with(|e, _| e.cell_corner(1, 1)).unwrap();
    press(&mut s, (f32::from(a.x) - 1.0, f32::from(a.y) - 1.0));
    hold_at(&mut s, (f32::from(a.x) - 1.0, bottom + 6.0));
    assert!(s.with(|e, _| e.sheet_state().unwrap().drag_scroll.is_some()), "the fill handle drag scrolls too");
    for _ in 0..6 {
        beat(&mut s);
    }
    assert!(offset(&mut s).1 < -100.0);
    assert!(range(&mut s).last_row > 10, "the fill range grew: {:?}", range(&mut s));
    release(&mut s, (f32::from(a.x) - 1.0, bottom + 6.0));
}

/// The pointer held outside the window to the right (the window keeps the pointer while the button is down): the cells go on
/// scrolling right, the farther the pointer the faster, as with the pointer below the window.
pub(crate) fn cell_drag_outside_the_window_to_the_right_scrolls_right() {
    let mut s = book();
    s.click_cell("B2");
    let start = s.with(|e, _| e.cell_center(1, 1)).unwrap();
    let window_right = s.with(|e, _| f32::from(e.workspace_bounds.unwrap().origin.x + e.workspace_bounds.unwrap().size.width));
    press(&mut s, (f32::from(start.x), f32::from(start.y)));
    // Through the cells to the edge, then out of the window.
    let (right, _) = edges(&mut s);
    hold_at(&mut s, (right - 20.0, f32::from(start.y)));
    let before = range(&mut s).last_column;
    let width = s.update(|_, w, _| f32::from(w.viewport_size().width));
    let outside = (width.max(window_right) + 30.0, f32::from(start.y));
    hold_at(&mut s, outside);
    assert!(s.with(|e, _| e.sheet_state().unwrap().drag_scroll.is_some()), "the timer is running outside the window");
    for _ in 0..3 {
        assert!(beat(&mut s));
    }
    let after = range(&mut s).last_column;
    assert!(after > before + 3, "the selection grew to the right: {before} -> {after}");
    let (x, y) = offset(&mut s);
    assert!(x < -100.0 && y == 0.0, "scrolled right only: {x}, {y}");
    assert_eq!(range(&mut s).last_row, 1);
    release(&mut s, outside);
}

fn allocated(s: &mut Scenario) -> (u32, u32) {
    s.with(|e, _| {
        let (_, sheet) = e.current_sheet().unwrap();
        (sheet.columns.len() as u32, sheet.rows.len() as u32)
    })
}

/// As in Excel the drag goes on past the cells the sheet holds, down the empty rows (the view grows its grid as it goes).
pub(crate) fn cell_drag_goes_on_past_the_allocated_rows() {
    let mut s = book();
    s.click_cell("B2");
    let start = s.with(|e, _| e.cell_center(1, 1)).unwrap();
    let (_, bottom) = edges(&mut s);
    let (_, rows) = allocated(&mut s);
    press(&mut s, (f32::from(start.x), f32::from(start.y)));
    let held = (f32::from(start.x), bottom + 200.0);
    hold_at(&mut s, held);
    let mut last = range(&mut s).last_row;
    for _ in 0..40 {
        beat(&mut s);
        let now = range(&mut s).last_row;
        assert!(now >= last, "never goes back: {last} -> {now}");
        last = now;
    }
    for _ in 0..200 {
        beat(&mut s);
    }
    let r = range(&mut s);
    assert!(r.last_row > rows + 300, "the selection goes past the {rows} rows the sheet holds: {}", r.last_row);
    assert!(r.last_row < 1_048_576);
    assert_eq!(r.first_row, 1);
    release(&mut s, held);
}

pub(crate) fn cell_drag_goes_on_past_the_allocated_columns() {
    let mut s = book();
    s.click_cell("B2");
    let start = s.with(|e, _| e.cell_center(1, 1)).unwrap();
    let (right, _) = edges(&mut s);
    let (columns, _) = allocated(&mut s);
    press(&mut s, (f32::from(start.x), f32::from(start.y)));
    let held = (right + 200.0, f32::from(start.y));
    hold_at(&mut s, held);
    for _ in 0..300 {
        beat(&mut s);
    }
    let r = range(&mut s);
    assert!(r.last_column > columns + 60, "the selection goes past the {columns} columns the sheet holds: {}", r.last_column);
    assert_eq!(r.first_column, 1);
    release(&mut s, held);
}

pub(crate) fn column_header_drag_goes_on_past_the_allocated_columns() {
    let mut s = book();
    let (columns, _) = allocated(&mut s);
    let b = s.header("B");
    let (right, _) = edges(&mut s);
    press(&mut s, b);
    hold_at(&mut s, (right + 200.0, b.1));
    for _ in 0..300 {
        beat(&mut s);
    }
    assert!(range(&mut s).last_column > columns + 60, "columns: {}", range(&mut s).last_column);
    release(&mut s, (right + 200.0, b.1));
}

pub(crate) fn row_header_drag_goes_on_past_the_allocated_rows() {
    let mut s = book();
    let (_, rows) = allocated(&mut s);
    let r2 = s.header("2");
    let (_, bottom) = edges(&mut s);
    press(&mut s, r2);
    hold_at(&mut s, (r2.0, bottom + 200.0));
    for _ in 0..300 {
        beat(&mut s);
    }
    assert!(range(&mut s).last_row > rows + 300, "rows: {}", range(&mut s).last_row);
    release(&mut s, (r2.0, bottom + 200.0));
}

pub(crate) fn fill_handle_goes_on_past_the_allocated_rows() {
    let mut s = book();
    s.click_cell("B2");
    s.enter("1", "enter");
    s.click_cell("B2");
    let (_, rows) = allocated(&mut s);
    let (_, bottom) = edges(&mut s);
    let a = s.with(|e, _| e.cell_corner(1, 1)).unwrap();
    press(&mut s, (f32::from(a.x) - 1.0, f32::from(a.y) - 1.0));
    let held = (f32::from(a.x) - 1.0, bottom + 200.0);
    hold_at(&mut s, held);
    for _ in 0..250 {
        beat(&mut s);
    }
    assert!(range(&mut s).last_row > rows + 300, "the fill range: {:?}", range(&mut s));
    release(&mut s, held);
}
