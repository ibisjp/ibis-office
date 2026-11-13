// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-06 (#95): a list that opens from a drop-down button hangs from the button's lower left corner (the list's upper
//! left corner on it, so the two left edges line up), not from where the pointer was. The one rule is `dropdown_list::below_button`. Pictures are in
//! `target/scenario/<name>/`.
use crate::shell::scenario::Scenario;
use ibis_doc_model::{ControlKind, ShapeKind};
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("xlsx::dropdown_position_20261106_form_control_list_hangs_from_the_button", form_control_list)]
}

fn form_control_list() {
    let mut s = Scenario::open("test/editing/xlsx-form-controls.xlsx");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(&sh.kind, ShapeKind::Control(c) if c.kind == ControlKind::Drop))).expect("a drop-down");
    // The pointer goes to the middle of the control, away from the button at its right end.
    let (frame, origin, scale) = s.with(|e, _| {
        let layout = e.slide_layout().expect("laid out");
        (e.slide().shapes[index].frame, layout.origin, layout.scale)
    });
    s.click_shape(index);
    s.settle();
    let (position, anchor) = s.with(|e, _| e.wb().list_popup.as_ref().map(|p| (p.position, p.anchor))).expect("the list is open");
    let button = frame.height.min(16 * ibis_doc_model::EMU_PER_POINT);
    let want_x = f32::from(origin.x) + (frame.x + frame.width - button) as f32 * scale;
    let want_y = f32::from(origin.y) + (frame.y + frame.height) as f32 * scale;
    assert_eq!(anchor, gpui_kit::Anchor::TopLeft, "the list's upper left corner is the one put at the button");
    assert!((f32::from(position.x) - want_x).abs() < 0.5 && (f32::from(position.y) - want_y).abs() < 0.5, "at the button's lower left: {position:?} want ({want_x}, {want_y})");
    s.screenshot("form-control-list");
}
