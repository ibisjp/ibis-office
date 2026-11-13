// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Dragging the sheet tabs (the dragged tab follows the pointer sideways only, the others make room, the drop is
//! one step to undo, Escape gives up) and the selection that is stretched with Shift+arrows: the end that moves
//! stays in view.
use super::SheetSteps;
use crate::shell::scenario::Scenario;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput, point, px};
use ibis_doc_model::{Command, SheetCommand, SlideId};
use crate::ext::*;

fn order(s: &mut Scenario) -> Vec<SlideId> {
    s.with(|e, _| e.presentation().slides.iter().map(|s| s.id).collect())
}

/// A workbook of 7 sheets with names that fill a part of the strip.
fn tabs_book() -> (Scenario, Vec<SlideId>) {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1100., 720.);
    s.update(|e, w, cx| {
        while e.presentation().slides.len() < 7 {
            e.insert_sheet(w, cx);
        }
        let commands = e.presentation().slides.iter().enumerate().map(|(i, sl)| Command::Sheet(SheetCommand::RenameSheet { slide: sl.id, name: format!("Tab_{i}_data") })).collect();
        e.run_sheet(commands, cx);
        let first = e.presentation().slides[0].id;
        e.select_slide(first, w, cx);
    });
    s.settle();
    let ids = order(&mut s);
    (s, ids)
}

/// The tab's box in the window (the strip not scrolled).
fn tab_box(s: &mut Scenario, index: usize) -> (f32, f32, f32, f32) {
    s.with(|e, _| {
        let scroll = &e.wb().tab_scroll;
        let (b, offset) = (scroll.bounds_for_item(index).expect("tab laid out"), f32::from(scroll.offset().x));
        (f32::from(b.left()) + offset, f32::from(b.top()), f32::from(b.size.width), f32::from(b.size.height))
    })
}

fn at(x: f32, y: f32) -> gpui_kit::Point<gpui_kit::Pixels> {
    point(px(x), px(y))
}
fn down(x: f32, y: f32) -> PlatformInput {
    PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: at(x, y), click_count: 1, ..Default::default() })
}
fn moved(x: f32, y: f32) -> PlatformInput {
    PlatformInput::MouseMove(MouseMoveEvent { position: at(x, y), pressed_button: Some(MouseButton::Left), ..Default::default() })
}
fn up(x: f32, y: f32) -> PlatformInput {
    PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: at(x, y), click_count: 1, ..Default::default() })
}
fn drag_state(s: &mut Scenario) -> Option<(bool, Vec<SlideId>, f32)> {
    s.with(|e, _| e.wb().tab_drag.as_ref().map(|d| (d.active, d.order.clone(), d.dragged_left())))
}

pub(super) fn tab_drag_follows_pointer_and_commits() {
    let (mut s, ids) = tabs_book();
    let (x1, y, w1, _) = tab_box(&mut s, 1);
    let (x4, _, w4, _) = tab_box(&mut s, 4);
    let press_x = x1 + w1 / 2.;
    // A small movement is no drag yet.
    s.dispatch(vec![down(press_x, y + 12.), moved(press_x + 2., y + 12.)]);
    assert_eq!(drag_state(&mut s).map(|d| d.0), Some(false), "pressed, not yet a drag");
    // The tab follows the pointer sideways; the pointer going up and down changes nothing.
    s.dispatch(vec![moved(press_x + 30., y + 12.)]);
    let (active, _, left_a) = drag_state(&mut s).expect("a drag");
    assert!(active);
    s.dispatch(vec![moved(press_x + 30., y - 200.)]);
    let (_, _, left_b) = drag_state(&mut s).unwrap();
    assert_eq!(left_a, left_b, "the dragged tab keeps to its row (horizontal constraint)");
    assert!((left_a - (x1 + 30.)).abs() < 1., "it moves with the pointer: {left_a} vs {}", x1 + 30.);
    // Past the middle of tab 2 and 3: they make room.
    s.dispatch(vec![moved(x4 + w4 * 0.25, y + 12.)]);
    let (_, shown, _) = drag_state(&mut s).unwrap();
    assert_eq!(shown[..5], [ids[0], ids[2], ids[3], ids[1], ids[4]], "tab 1 would come before tab 4 (left half of tab 4)");
    assert_eq!(order(&mut s), ids, "nothing is changed in the workbook before the drop");
    s.settle();
    s.screenshot("tab-dnd-dragging");
    // The right half of tab 4: after it.
    s.dispatch(vec![moved(x4 + w4 * 0.75, y + 12.)]);
    let (_, shown, _) = drag_state(&mut s).unwrap();
    assert_eq!(shown[..5], [ids[0], ids[2], ids[3], ids[4], ids[1]]);
    // Back over the left half of tab 4 (which is now where the dragged tab was): the tab slides back again.
    s.dispatch(vec![moved(x4 - w1 + w4 * 0.25, y + 12.)]);
    let (_, shown, _) = drag_state(&mut s).unwrap();
    assert_eq!(shown[..5], [ids[0], ids[2], ids[3], ids[1], ids[4]]);
    // The drop.
    s.dispatch(vec![up(x4 - w1 + w4 * 0.25, y + 12.)]);
    let mut expected = ids.clone();
    let moved_tab = expected.remove(1);
    expected.insert(3, moved_tab);
    assert_eq!(order(&mut s), expected, "the order that was shown is the order that is kept");
    assert!(drag_state(&mut s).is_none());
    assert_eq!(s.with(|e, _| e.current_slide), ids[1], "the sheet that was dragged is the active one");
    // One step to undo.
    s.key("cmd-z");
    assert_eq!(order(&mut s), ids, "undone in one step");
    s.key("cmd-shift-z");
    assert_eq!(order(&mut s), expected);
    s.key("cmd-z");
}

pub(super) fn tab_drag_escape_restores() {
    let (mut s, ids) = tabs_book();
    let (x1, y, w1, _) = tab_box(&mut s, 1);
    let (x5, _, w5, _) = tab_box(&mut s, 5);
    s.dispatch(vec![down(x1 + w1 / 2., y + 12.), moved(x1 + w1 / 2. + 40., y + 12.), moved(x5 + w5 * 0.75, y + 12.)]);
    let (_, shown, _) = drag_state(&mut s).expect("a drag");
    assert_ne!(shown, ids, "the tabs made room");
    s.key("escape");
    assert!(drag_state(&mut s).is_none(), "Escape gives the drag up");
    assert_eq!(order(&mut s), ids);
    // The button going up afterwards, and moves with the button down, do nothing.
    s.dispatch(vec![moved(x5, y + 12.), up(x5, y + 12.)]);
    assert_eq!(order(&mut s), ids, "no move after Escape");
    assert_eq!(s.with(|e, _| e.current_slide), ids[0], "the active sheet is the one it was");
    // Dropping where it started changes nothing.
    s.dispatch(vec![down(x1 + w1 / 2., y + 12.), moved(x1 + w1 / 2. + 20., y + 12.), up(x1 + w1 / 2. + 20., y + 12.)]);
    assert_eq!(order(&mut s), ids);
}

pub(super) fn tab_drag_scrolling_strip_and_protection() {
    let (mut s, ids) = tabs_book();
    // A narrow window: the strip scrolls.
    s.size(520., 720.);
    s.update(|e, _, cx| { e.wb().tab_scroll.set_offset(point(px(0.), px(0.))); cx.notify(); });
    s.settle();
    let (view_left, view_right, max) = s.with(|e, _| {
        let sc = &e.wb().tab_scroll;
        (f32::from(sc.bounds().left()), f32::from(sc.bounds().right()), f32::from(sc.max_offset().x))
    });
    assert!(max > 50., "the tabs overflow the strip: {max}");
    let (x0, y, w0, _) = tab_box(&mut s, 0);
    // Dragged to the right end of the strip, the strip scrolls and the tab can reach a tab that was hidden.
    s.dispatch(vec![down(x0 + w0 / 2., y + 12.), moved(x0 + w0 / 2. + 30., y + 12.)]);
    for _ in 0..40 {
        s.dispatch(vec![moved(view_right - 4., y + 12.)]);
    }
    let offset = s.with(|e, _| f32::from(e.wb().tab_scroll.offset().x));
    assert!(offset < -10., "the strip scrolled toward the end: {offset}");
    let (_, shown, left) = drag_state(&mut s).unwrap();
    assert!(left >= view_left && left <= view_right, "the dragged tab stays in the strip: {left}");
    assert_eq!(*shown.last().unwrap(), ids[0], "it went after the last tab");
    s.dispatch(vec![up(view_right - 4., y + 12.)]);
    let mut expected = ids.clone();
    let first = expected.remove(0);
    expected.push(first);
    assert_eq!(order(&mut s), expected);
    s.key("cmd-z");
    assert_eq!(order(&mut s), ids);

    // A workbook whose structure is protected: the tabs do not move.
    s.update(|e, _, cx| {
        e.run_sheet(vec![Command::Sheet(SheetCommand::SetWorkbookProtection {
            protection: Some(ibis_doc_model::sheet::WorkbookProtection { lock_structure: true, lock_windows: false, lock_revision: false, hash: None, legacy: None }),
        })], cx);
        e.wb().tab_scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    });
    s.settle();
    let (x1, y, w1, _) = tab_box(&mut s, 1);
    let (x2, _, w2, _) = tab_box(&mut s, 2);
    s.dispatch(vec![down(x1 + w1 / 2., y + 12.), moved(x1 + w1 / 2. + 30., y + 12.), moved(x2 + w2 * 0.8, y + 12.), up(x2 + w2 * 0.8, y + 12.)]);
    assert_eq!(order(&mut s), ids, "a protected workbook keeps its tabs in place");
    assert!(drag_state(&mut s).is_none());
}

// ---- Shift + arrows --------------------------------------------------------------------------------------------

/// Whether the cell is wholly inside the part of the workspace where cells are shown (not under the headers,
/// the scrollbars or the frozen panes).
fn cell_in_view(s: &mut Scenario, frozen: Option<(u32, u32)>, column: u32, row: u32) -> bool {
    let (frozen_columns, frozen_rows) = frozen.unwrap_or((0, 0));
    s.with(move |e, _| {
        let (Some(center), Some(ws), Some(content)) = (e.cell_center(column, row), e.workspace_bounds, e.workspace_content()) else { return false };
        let (split_x, split_y) = e.frozen_split(content).unwrap_or((None, None));
        let left = f32::from(ws.origin.x) + if column < frozen_columns { crate::sheet_ui::ROW_HEADER_WIDTH } else { split_x.unwrap_or(crate::sheet_ui::ROW_HEADER_WIDTH) };
        let top = f32::from(ws.origin.y) + if row < frozen_rows { crate::sheet_ui::COLUMN_HEADER_HEIGHT } else { split_y.unwrap_or(crate::sheet_ui::COLUMN_HEADER_HEIGHT) };
        let right = f32::from(ws.right()) - crate::sheet_ui::SCROLLBAR_ROOM;
        let bottom = f32::from(ws.bottom()) - crate::sheet_ui::SCROLLBAR_ROOM;
        let (x, y) = (f32::from(center.x), f32::from(center.y));
        // A cell of the frozen columns or rows stays where it is whatever the scrolling (`cell_center` is of the unfrozen part).
        (column < frozen_columns || (x > left && x < right)) && (row < frozen_rows || (y > top && y < bottom))
    })
}

fn end_cell(s: &mut Scenario) -> (u32, u32) {
    s.with(|e, _| e.sheet_state().and_then(|st| st.cell_ends.get(&e.current_slide).copied()).or_else(|| e.current_cell()).expect("a cell"))
}

fn stretch(frozen: Option<(u32, u32)>, zoom: Option<u16>) {
    let mut s = super::new_sheet();
    s.size(900., 700.);
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run_sheet(vec![Command::Sheet(SheetCommand::SetZoom { slide, zoom }), Command::Sheet(SheetCommand::Freeze { slide, frozen })], cx);
        // The view's own zoom (the status bar's), which the sheet's saved zoom sets when the sheet is shown.
        e.zoom = Some(zoom.map_or(1.0, |z| f32::from(z) / 100.));
        cx.notify();
    });
    s.settle();
    let start = frozen.map_or("B2", |_| "D5");
    s.click_cell(start);
    let anchor = s.with(|e, _| e.current_cell().unwrap());
    // Down: far more rows than a screen shows.
    for n in 0..80 {
        s.key("shift-down");
        let (c, r) = end_cell(&mut s);
        assert!(cell_in_view(&mut s, frozen, c, r), "frozen {frozen:?} zoom {zoom:?}: the end ({c},{r}) is in view after {n} presses down");
    }
    assert_eq!(end_cell(&mut s).1, anchor.1 + 80);
    for n in 0..40 {
        s.key("shift-right");
        let (c, r) = end_cell(&mut s);
        assert!(cell_in_view(&mut s, frozen, c, r), "frozen {frozen:?} zoom {zoom:?}: the end ({c},{r}) is in view after {n} presses right");
    }
    assert_eq!(end_cell(&mut s).0, anchor.0 + 40);
    // And back: the end moving up and left comes into view.
    for n in 0..80 {
        s.key("shift-up");
        let (c, r) = end_cell(&mut s);
        assert!(cell_in_view(&mut s, frozen, c, r), "frozen {frozen:?} zoom {zoom:?}: the end ({c},{r}) is in view after {n} presses up");
    }
    for n in 0..40 {
        s.key("shift-left");
        let (c, r) = end_cell(&mut s);
        assert!(cell_in_view(&mut s, frozen, c, r), "frozen {frozen:?} zoom {zoom:?}: the end ({c},{r}) is in view after {n} presses left");
    }
    assert_eq!(end_cell(&mut s), anchor, "back at the selected cell");
    // A screen at a time and to the ends of the row.
    for n in 0..6 {
        s.key("shift-pagedown");
        let (c, r) = end_cell(&mut s);
        assert!(cell_in_view(&mut s, frozen, c, r), "frozen {frozen:?} zoom {zoom:?}: the end ({c},{r}) is in view after {n} pages down");
    }
    assert!(end_cell(&mut s).1 > anchor.1 + 20, "Shift+PageDown stretches by a screen");
    for _ in 0..3 {
        s.key("shift-pageup");
        let (c, r) = end_cell(&mut s);
        assert!(cell_in_view(&mut s, frozen, c, r), "the end is in view after page up");
    }
    s.key("shift-right");
    s.key("shift-home");
    let (c, r) = end_cell(&mut s);
    assert_eq!(c, 0, "Shift+Home stretches to column A");
    assert!(cell_in_view(&mut s, frozen, c, r), "frozen {frozen:?} zoom {zoom:?}: the end at column A is in view");
    // Ctrl+Shift+arrow (to the edge of the data region, here far away).
    s.click_cell(start);
    s.key("ctrl-shift-right");
    let (c, r) = end_cell(&mut s);
    assert!(cell_in_view(&mut s, frozen, c, r), "frozen {frozen:?} zoom {zoom:?}: the end after Ctrl+Shift+→ is in view ({c},{r})");
    s.screenshot(&format!("shift-stretch-{}-{}", frozen.is_some(), zoom.unwrap_or(100)));
}

pub(super) fn shift_arrows_follow_the_moving_end() {
    for frozen in [None, Some((2, 3))] {
        for zoom in [None, Some(200)] {
            stretch(frozen, zoom);
        }
    }
}

/// Hidden rows and columns, and a merged range at the end that moves.
pub(super) fn shift_arrows_follow_past_hidden_and_merged() {
    use ibis_doc_model::{CellRange, sheet_command::Axis};
    let mut s = super::new_sheet();
    s.size(900., 700.);
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        let mut commands = vec![
            Command::Sheet(SheetCommand::Hide { slide, axis: Axis::Rows, first: 10, last: 14, hidden: true }),
            Command::Sheet(SheetCommand::Hide { slide, axis: Axis::Columns, first: 8, last: 10, hidden: true }),
        ];
        // A merged range tall as three rows, in the way down.
        commands.push(Command::Sheet(SheetCommand::Merge { slide, range: CellRange { first_row: 39, last_row: 41, first_column: 1, last_column: 2 }, center: false }));
        e.run_sheet(commands, cx);
    });
    s.settle();
    s.click_cell("B2");
    for n in 0..60 {
        s.key("shift-down");
        let (c, r) = end_cell(&mut s);
        assert!(cell_in_view(&mut s, None, c, r), "the end ({c},{r}) is in view after {n} presses down");
        assert!(!(10..=14).contains(&r), "hidden rows are passed");
    }
    for n in 0..30 {
        s.key("shift-right");
        let (c, r) = end_cell(&mut s);
        assert!(cell_in_view(&mut s, None, c, r), "the end ({c},{r}) is in view after {n} presses right");
        assert!(!(8..=10).contains(&c), "hidden columns are passed");
    }
}
