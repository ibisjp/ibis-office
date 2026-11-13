// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-04: the animation window (#38) as the shared table (one row per effect, dragged to reorder), the small
//! 効果のオプション dialog opened from a row at the main window's right edge beside the pane, and the trigger (#39) chosen
//! with radio buttons; the changes go through the timing model into the saved file.
use super::{press_panel_button, press_panel_choice};
use crate::shell::scenario::Scenario;
use ibis_doc_model::{AnimationClass as C, AnimationLook as L, AnimationSide, AnimationTrigger};
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("pptx::animation_window_rows_drag_edit_dialog_and_save", animation_window)]
}

const TABLE: &str = "anim-table";

fn order(s: &mut Scenario) -> Vec<(usize, C, L)> {
    s.with(|e, _| e.slide().animations.iter().map(|a| (e.slide().shape_index(a.shape).unwrap(), a.class, a.look)).collect())
}

/// The middle of the pane's row `i` (a little right of its left edge, off the buttons and the grip).
fn row_point(s: &mut Scenario, i: usize, y_fraction: f32) -> (f32, f32) {
    let dnd = s.with(|e, _| e.pptx().unwrap().animation_dnd.clone());
    let b = dnd.row_bounds(TABLE, i).unwrap_or_else(|| panic!("row {i} of the animation window"));
    (f32::from(b.origin.x) + 120., f32::from(b.origin.y) + f32::from(b.size.height) * y_fraction)
}

fn animation_window() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1280., 720.);
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    s.toolbar_menu("motion", "add.pptx.anim.em.spin");
    s.click_shape(1);
    s.toolbar_menu("motion", "pptx.anim.in.fly-bottom");
    s.toolbar_menu("motion", "pptx.anim.pane");
    s.settle();
    let start = order(&mut s);
    assert_eq!(start, vec![(0, C::Entrance, L::Fade), (0, C::Emphasis, L::Spin(21_600_000)), (1, C::Entrance, L::Fly(AnimationSide::Bottom))]);
    s.screenshot("animation-window");

    // Drag the first row under the last: the effect takes that place in the playing order (one undo step).
    let from = row_point(&mut s, 0, 0.5);
    let to = row_point(&mut s, 2, 0.85);
    s.drag_through(&[from, (from.0, from.1 + 8.), to, (to.0, to.1 + 0.5)]);
    s.settle();
    assert_eq!(order(&mut s), vec![start[1], start[2], start[0]], "the dragged row went to the end");
    s.key("cmd-z");
    assert_eq!(order(&mut s), start, "one undo restores the order");
    s.key("cmd-shift-z");
    assert_eq!(order(&mut s), vec![start[1], start[2], start[0]]);

    // A click on a row selects its shape; a double click opens the dialog beside the pane.
    s.settle();
    let row = row_point(&mut s, 1, 0.5);
    s.click(row);
    let (shape, selected) = s.with(|e, _| (e.slide().animations[1].shape, e.selected_ids()));
    assert_eq!(selected, vec![shape], "the click selected the row's shape");
    assert!(s.with(|e, _| e.pptx().unwrap().animation_options_panel.is_none()));
    s.double_click(row);
    s.settle();
    assert_eq!(s.with(|e, _| e.pptx().unwrap().animation_options_panel.as_ref().map(|p| p.index)), Some(1));
    let (bounds, pane) = s.with(|e, _| (e.dialog.bounds.expect("the dialog is open"), (1280., e.sidebar_width)));
    let right_edge = pane.0 - pane.1.max(crate::shell::chrome::SIDEBAR_WIDTH);
    assert!((f32::from(bounds.right()) - right_edge).abs() < 2., "the dialog is at the window's right edge, left of the pane: {bounds:?}");
    assert!(s.with(|e, _| e.pptx().unwrap().animation_pane), "the list stays");
    s.screenshot("animation-options-dialog");

    // The trigger is two radio buttons; the target shape's list shows once the shape click is chosen.
    let other = s.with(|e, _| e.slide().shapes.iter().map(|sh| sh.id).find(|id| *id != shape).unwrap());
    press_panel_choice(&mut s, "anim-opt-trigger-shape-click");
    assert!(matches!(s.with(|e, _| e.pptx().unwrap().animation_options_panel.as_ref().map(|p| p.trigger.clone())), Some(AnimationTrigger::OnShapeClick(_))));
    s.update(|e, _, cx| e.set_panel_trigger(AnimationTrigger::OnShapeClick(other), cx));
    s.screenshot("animation-options-trigger");
    press_panel_button(&mut s, "pptx-anim-opt-apply");
    assert!(s.with(|e, _| e.pptx().unwrap().animation_options_panel.is_none()), "applied: the dialog closed");
    assert_eq!(s.with(|e, _| e.slide().animations[1].trigger.clone()), AnimationTrigger::OnShapeClick(other));
    // Back to the normal progression with the first radio button.
    s.update(|e, _, cx| e.open_animation_options_panel(1, cx));
    s.settle();
    press_panel_choice(&mut s, "anim-opt-trigger-normal");
    press_panel_button(&mut s, "pptx-anim-opt-apply");
    assert_eq!(s.with(|e, _| e.slide().animations[1].trigger.clone()), AnimationTrigger::None);

    // The new order and the edited effect are in the saved file.
    let expected = order(&mut s);
    s.save_as("animation-window.pptx");
    let saved = s.reopen();
    assert_eq!(saved.slides[0].animations.iter().map(|a| (saved.slides[0].shape_index(a.shape).unwrap(), a.class, a.look)).collect::<Vec<_>>(), expected);
}
