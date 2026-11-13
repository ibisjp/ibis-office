// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-05: the presentation screen's panels (animation pane and its options dialog, the shared ink pane of the three
//! screens, print, recording preview, slide show setup, 3D model) after the layout fixes. Each takes its picture
//! (`target/scenario/<name>/NN-*.png`); the recording preview's time is checked.
use crate::shell::scenario::Scenario;
use crate::pptx::ext::*;
use crate::docx::ext::*;
use crate::xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::panels_20261105_group1_animation_pane_and_options", animation_pane),
        ("pptx::panels_20261105_group1_ink_pane_pptx", ink_pptx),
        ("pptx::panels_20261105_group1_ink_pane_docx", ink_docx),
        ("pptx::panels_20261105_group1_ink_pane_xlsx", ink_xlsx),
        ("pptx::panels_20261105_group1_print_show_3d_panels", print_show_3d),
        ("pptx::panels_20261105_group1_record_preview_sets_the_slide_time_at_once", record_preview),
    ]
}

fn animation_pane() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1200., 900.);
    s.click_shape(0);
    s.toolbar_menu("motion", "pptx.anim.in.fade");
    s.click_shape(1);
    s.toolbar_menu("motion", "pptx.anim.in.fly-bottom");
    s.toolbar_menu("motion", "pptx.anim.pane");
    s.screenshot("animation-pane");
    s.update(|e, _, cx| e.open_animation_options_panel(0, cx));
    s.screenshot("animation-options");
    // The delete button of the dialog removes the effect and closes the dialog.
    let before = s.with(|e, _| e.slide().animations.len());
    s.update(|e, _, cx| { e.close_animation_options_panel(cx); e.remove_effect(0, cx); });
    assert_eq!(s.with(|e, _| e.slide().animations.len()), before - 1);
}

fn ink_pptx() {
    let mut p = Scenario::open("test/editing/pptx-plan.pptx");
    p.size(1200., 900.);
    p.update(|e, w, cx| e.open_pptx_ink_panel(w, cx));
    p.screenshot("ink-pptx");
    // The width dropdown opened: its items draw the line and the label.
    p.click((1114., 289.));
    p.settle();
    p.screenshot("ink-pptx-width-list");
}

fn ink_docx() {
    let mut d = Scenario::open("test/editing/docx-report.docx");
    d.size(1200., 900.);
    d.update(|e, _, cx| e.open_docx_ink_panel(cx));
    d.screenshot("ink-docx");
    // The width dropdown opened: its items draw the line and the label.
    d.click((1114., 289.));
    d.settle();
    d.screenshot("ink-docx-width-list");
}

fn ink_xlsx() {
    let mut x = Scenario::open("test/editing/xlsx-sales.xlsx");
    x.size(1200., 900.);
    x.update(|e, w, cx| e.open_ink_panel(w, cx));
    x.screenshot("ink-xlsx");
    // The width dropdown opened: its items draw the line and the label.
    x.click((1114., 289.));
    x.settle();
    x.screenshot("ink-xlsx-width-list");
}

fn print_show_3d() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1200., 900.);
    s.update(|e, w, cx| e.open_print_panel(w, cx));
    s.screenshot("print");
    s.update(|e, w, cx| e.close_print_panel(w, cx));
    s.update(|e, w, cx| e.open_show_panel(w, cx));
    s.screenshot("show-setup");
    s.update(|e, w, cx| e.close_show_panel(w, cx));
    s.update(|e, _, cx| e.insert_3d_model_object(std::path::Path::new("spacecraft.glb"), cx));
    let id = s.with(|e, _| e.target_3d_model_shape_id().expect("3D model"));
    s.update(|e, _, cx| { e.selection = Some(id); cx.notify(); });
    s.menu("pptx.3d-model.panel");
    s.screenshot("threed");
}

fn record_preview() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1200., 900.);
    s.menu("pptx.record-preview");
    // No time on the slide: "on click"; the stepper is off. Choosing "automatic" gives the slide the seconds at once.
    assert_eq!(s.with(|e, _| e.slide().transition.as_ref().and_then(|t| t.after)), None);
    s.screenshot("record-preview-click");
    s.update(|e, _, cx| { e.record_preview_set_seconds(7.5, cx); e.record_preview_set_slide_timing_ms(Some(7500), cx); });
    assert_eq!(s.with(|e, _| e.slide().transition.as_ref().and_then(|t| t.after)), Some(7500));
    s.screenshot("record-preview-auto");
    s.update(|e, _, cx| e.record_preview_set_slide_timing_ms(None, cx));
    assert_eq!(s.with(|e, _| e.slide().transition.as_ref().and_then(|t| t.after)), None);
}
