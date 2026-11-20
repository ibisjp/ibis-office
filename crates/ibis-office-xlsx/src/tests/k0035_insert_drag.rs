// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0035 (0.8.8): Insert > a shape on the 表計算画面 is an insert mode as on the 文書画面 and プレゼンテーション画面: the pointer is a cross,
//! the drag from the mouse-down point to the mouse-up point draws a translucent box with an outline and the shape takes that box,
//! Esc drops the drag, and a plain click puts the shape at its usual size. The box and the cross are the shell's one component
//! (`shell::insert_band`), which the other two screens use too.

use crate::shell::editor::Tool;
use crate::shell::shape_items::{ArrowKind, LineKind};
use crate::shell::scenario::Scenario;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput, point, px};
use ibis_doc_model::{EmuRect, ShapeKind};
use crate::ext::*;

fn cell(s: &mut Scenario, reference: &str) -> (f32, f32) {
    let split = reference.find(|c: char| c.is_ascii_digit()).unwrap();
    let column = ibis_calc::formula::parser::column_index(&reference[..split]).unwrap();
    let row: u32 = reference[split..].parse().unwrap();
    let p = s.with(|e, _| e.cell_center(column, row - 1)).expect("the cell on screen");
    (f32::from(p.x), f32::from(p.y))
}

fn tool(s: &mut Scenario) -> Tool {
    s.with(|e, _| e.tool)
}

/// The shapes on the sheet that are not the grid.
fn drawn(s: &mut Scenario) -> Vec<(ShapeKind, EmuRect)> {
    s.with(|e, _| e.slide().shapes.iter().filter(|sh| !matches!(sh.kind, ShapeKind::Sheet(_))).map(|sh| (sh.kind.clone(), sh.frame)).collect())
}

/// The frame of the sheet's grid and the left and top edges of the cells (EMU, the slide's).
fn edges(s: &mut Scenario, column: u32, row: u32) -> (i64, i64) {
    s.with(|e, _| {
        let (shape, sheet) = e.current_sheet().expect("a sheet");
        (shape.frame.x + sheet.column_offsets()[column as usize], shape.frame.y + sheet.row_offsets()[row as usize])
    })
}

fn at(s: &mut Scenario, event: impl FnOnce(gpui_kit::Point<gpui_kit::Pixels>) -> PlatformInput, (x, y): (f32, f32)) {
    s.dispatch(vec![event(point(px(x), px(y)))]);
}

fn down(position: gpui_kit::Point<gpui_kit::Pixels>) -> PlatformInput {
    PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() })
}

fn hold(position: gpui_kit::Point<gpui_kit::Pixels>) -> PlatformInput {
    PlatformInput::MouseMove(MouseMoveEvent { position, pressed_button: Some(MouseButton::Left), ..Default::default() })
}

fn up(position: gpui_kit::Point<gpui_kit::Pixels>) -> PlatformInput {
    PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position, click_count: 1, ..Default::default() })
}

pub(crate) fn insert_shape_starts_the_mode_and_a_drag_takes_its_bounds() {
    let mut s = super::new_sheet();
    assert_eq!(tool(&mut s), Tool::Select);
    let before = drawn(&mut s).len();
    s.menu("shape.rect");
    assert_eq!(tool(&mut s), Tool::Shape("rect"), "the menu arms the insert mode, it does not insert");
    assert_eq!(drawn(&mut s).len(), before, "nothing is inserted before the drag");
    assert_eq!(s.with(|e, _| e.status_texts().last().map(|t| t.to_string())), Some(crate::shell::i18n::text("status.hint.shape").to_string()), "the status bar tells what to do");
    let (from, to) = (cell(&mut s, "C3"), cell(&mut s, "F8"));
    // While the button is down the box of the drag is there (the shell's translucent box), and the mode is on.
    at(&mut s, down, from);
    at(&mut s, hold, to);
    let rubber = s.with(|e, _| e.creation_rect());
    assert!(rubber.is_some(), "the box is drawn while dragging");
    s.screenshot("k0035-insert-drag-box");
    at(&mut s, up, to);
    assert_eq!(tool(&mut s), Tool::Select, "the mode ends with the shape");
    let shapes = drawn(&mut s);
    assert_eq!(shapes.len(), before + 1, "one shape");
    let (kind, frame) = shapes.last().unwrap().clone();
    assert!(matches!(kind, ShapeKind::Auto(_)), "a preset shape");
    // The box is the one dragged: from the middle of C3 to the middle of F8 (so it starts inside C3 and ends inside F8).
    let (c3x, c3y) = edges(&mut s, 2, 2);
    let (d4x, d4y) = edges(&mut s, 3, 3);
    let (f8x, f8y) = edges(&mut s, 5, 7);
    let (g9x, g9y) = edges(&mut s, 6, 8);
    assert!(frame.x >= c3x && frame.x < d4x && frame.y >= c3y && frame.y < d4y, "it begins in C3: {frame:?}");
    assert!(frame.right() >= f8x && frame.right() < g9x && frame.bottom() >= f8y && frame.bottom() < g9y, "it ends in F8: {frame:?}");
    assert_eq!(rubber.map(|r| (r.x, r.y, r.width, r.height)), Some((frame.x, frame.y, frame.width, frame.height)), "the box shown is the box inserted");
    // The two-cell anchor of the file is made of this frame: saved and read again, the shape is where it was dragged.
    s.save_as("k0035-insert-drag.xlsx");
    let reopened = s.reopen();
    let slide = reopened.slides.iter().find(|sl| sl.shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Auto(_)) && (sh.frame.x - frame.x).abs() < 20_000 && (sh.frame.y - frame.y).abs() < 20_000)).is_some();
    assert!(slide, "the saved file's anchor puts the shape in the dragged box");
}

pub(crate) fn a_plain_click_puts_the_shape_at_its_usual_size() {
    let mut s = super::new_sheet();
    let before = drawn(&mut s).len();
    s.menu("shape.ellipse");
    let at_cell = cell(&mut s, "B2");
    s.click(at_cell);
    let shapes = drawn(&mut s);
    assert_eq!(shapes.len(), before + 1);
    let frame = shapes.last().unwrap().1;
    assert_eq!((frame.width, frame.height), super::super::shape_edit::SHEET_SHAPE_SIZE, "a click makes the usual size");
    assert_eq!(tool(&mut s), Tool::Select);
}

pub(crate) fn esc_during_the_drag_cancels_it() {
    let mut s = super::new_sheet();
    let before = drawn(&mut s).len();
    s.menu("shape.rect");
    let (from, to) = (cell(&mut s, "C3"), cell(&mut s, "F8"));
    at(&mut s, down, from);
    at(&mut s, hold, to);
    assert!(s.with(|e, _| e.creation_rect().is_some()));
    s.key("escape");
    assert!(s.with(|e, _| e.creation_rect().is_none() && e.drag.is_none()), "the drag is dropped");
    at(&mut s, up, to);
    assert_eq!(drawn(&mut s).len(), before, "releasing after Esc inserts nothing");
    // The mode is still on, as in the other two screens; the next Esc leaves it.
    assert_eq!(tool(&mut s), Tool::Shape("rect"));
    s.key("escape");
    assert_eq!(tool(&mut s), Tool::Select);
    assert_eq!(drawn(&mut s).len(), before);
}

pub(crate) fn a_line_and_a_text_box_use_the_mode_too() {
    let mut s = super::new_sheet();
    let before = drawn(&mut s).len();
    s.menu("docx.shape.arrow");
    assert_eq!(tool(&mut s), Tool::Line { kind: LineKind::Straight, arrow: ArrowKind::Single });
    let (from, to) = (cell(&mut s, "B2"), cell(&mut s, "E5"));
    s.drag(from, to);
    assert_eq!(drawn(&mut s).len(), before + 1, "the line");
    assert_eq!(tool(&mut s), Tool::Select);
    // An elbow and a curved connector are made of the drag too (a box, not a click's fixed size).
    s.menu("docx.shape.elbow-arrow");
    assert_eq!(tool(&mut s), Tool::Line { kind: LineKind::Bent, arrow: ArrowKind::Single });
    let (from, to) = (cell(&mut s, "H2"), cell(&mut s, "K5"));
    s.drag(from, to);
    assert_eq!(drawn(&mut s).len(), before + 2, "the elbow connector");
    s.menu("docx.shape.curved");
    assert_eq!(tool(&mut s), Tool::Line { kind: LineKind::Curved, arrow: ArrowKind::None });
    s.key("escape");
    assert_eq!(tool(&mut s), Tool::Select, "Esc leaves the mode");
    s.menu("insert.text-box");
    assert_eq!(tool(&mut s), Tool::TextBox);
    let (from, to) = (cell(&mut s, "B7"), cell(&mut s, "E10"));
    s.drag(from, to);
    assert_eq!(drawn(&mut s).len(), before + 3, "the text box");
}

/// The three screens draw the box and show the cross with the shell's one component, not with their own copies.
pub(crate) fn the_three_screens_use_the_same_component() {
    let docx = include_str!("../../../ibis-office-docx/src/draw_table.rs");
    let docx_page = include_str!("../../../ibis-office-docx/src/doc_ui.rs");
    let pptx = include_str!("../../../ibis-office-pptx/src/slide_view.rs");
    let shell = include_str!("../../../ibis-office-shell/src/workspace.rs");
    assert!(docx.contains("insert_band::band_element") && docx_page.contains("insert_band::insert_cursor"), "the document screen");
    assert!(pptx.contains("insert_band::paint_band") && pptx.contains("insert_band::insert_cursor"), "the presentation screen");
    assert!(shell.contains("insert_band::band_element") && shell.contains("insert_band::insert_cursor"), "the sheet screen's box (the workspace)");
    // No second copy of the box's colours or the cross.
    assert!(!docx.contains("0x1a73e8") && !docx.contains("hsla(0.6"), "the document screen has no colours of its own for the box");
    assert!(!pptx.contains("fill.a = 0.08"), "the presentation screen has no fill of its own for the box");
}
