// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-06 (#84 and the slide list): the title row's button that closes the master / format-editing modes (a pill),
//! the slide master view's list of masters and layouts on the left (no outline tab), the slide list's width kept when the
//! tab changes, and the clip bar's time label inside the bar. Pictures are in `target/scenario/<name>/`.
use crate::shell::scenario::Scenario;
use super::SlideSteps as _;
use ibis_doc_model::EmuPoint;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::fix_20261106_view_mode_close_buttons_in_the_title_row", mode_close_buttons),
        ("pptx::fix_20261106_view_master_view_lists_the_masters_on_the_left", master_list_on_the_left),
        ("pptx::fix_20261106_view_slide_list_keeps_its_width_between_tabs", list_width_between_tabs),
        ("pptx::fix_20261106_view_clip_bar_time_is_inside_the_bar", clip_bar_time_inside),
    ]
}

fn mode_close_buttons() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("pptx.master-view");
    assert!(s.with(|e, _| e.is_master_view()));
    s.settle();
    s.screenshot("close-slide-master");
    s.update(|e, w, cx| e.close_master_view(w, cx));
    for (id, check, shot) in [
        ("pptx.notes-format", (|e: &crate::shell::editor::SlideEditor| e.is_notes_format()) as fn(&_) -> bool, "close-notes-format"),
        ("pptx.notes-master", |e| e.is_notes_master(), "close-notes-master"),
        ("pptx.handout-master", |e| e.is_handout_master(), "close-handout-master"),
    ] {
        s.menu(id);
        assert!(s.with(|e, _| check(e)), "{id} opened");
        s.settle();
        s.screenshot(shot);
        s.update(|e, w, cx| e.close_notes_format(w, cx));
        assert!(!s.with(|e, _| e.is_page_view()));
    }
}

fn master_list_on_the_left() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("pptx.master-view");
    s.settle();
    let pages = s.with(|e, _| e.master_view_page_count());
    assert!(pages > 2);
    s.settle();
    s.screenshot("master-list");
}

fn list_width_between_tabs() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let slide_left = |s: &mut Scenario| s.with(|e, _| f32::from(e.workspace_bounds.unwrap().origin.x));
    let (w0, x0) = (s.with(|e, _| e.pane_width_now()), slide_left(&mut s));
    s.screenshot("tab-thumbnails");
    s.update(|e, w, cx| e.switch_to_outline_view(w, cx));
    s.settle();
    s.screenshot("tab-outline");
    assert_eq!(s.with(|e, _| e.pane_width_now()), w0);
    assert_eq!(slide_left(&mut s), x0, "the slide does not move");
    s.update(|e, w, cx| e.switch_to_normal_view(w, cx));
    s.settle();
    assert_eq!(slide_left(&mut s), x0);
}

fn clip_bar_time_inside() {
    use crate::clip_bar::{geometry, min_width};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing");
    let audio = root.join("clip-tone.wav");
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    let a = s.update(|e, _, cx| e.insert_clip(&audio, std::fs::read(&audio).unwrap(), true, cx)).unwrap();
    s.key("escape");
    let frame = s.with(|e, _| e.slide().shape(a).unwrap().frame);
    let center = s.at(EmuPoint::new(frame.x + frame.width / 2, frame.y + frame.height / 2));
    s.hover(center);
    assert_eq!(s.with(|e, _| e.clip_bars_shown()), vec![a]);
    let width = s.with(|e, cx| f32::from(e.clip_bar_bounds(a, e.slide_layout().unwrap(), cx).unwrap().size.width));
    // The label the bar shows (the clip's length, not yet played) is inside the bar's width, with the margin.
    let need = s.with(|e, cx| min_width(&e.clip_bar_time_text(a), cx));
    assert!(width >= need - 0.5, "the bar {width} holds the time label (needs {need})");
    assert!(width >= geometry(0.0).time_x, "the label starts inside the bar");
    s.settle();
    s.screenshot("clip-bar-audio");
}
