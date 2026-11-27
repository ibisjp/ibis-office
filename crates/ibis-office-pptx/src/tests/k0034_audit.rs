// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0034 (the audit): the slide show's questions (the ink annotations, the rehearsal's timings) are the shared message box with
//! the shared action bar: the primary button first, Esc is the cancel, Enter presses the primary button, each answer works.
use crate::shell::scenario::{Scenario, button_probe as buttons};
use crate::ext::*;
use gpui_kit::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::k0034_audit_slideshow_ink_question_is_shared", ink_question),
        ("pptx::k0034_audit_slideshow_rehearsal_question_is_shared", rehearsal_question),
        // #261: the primary buttons of the two questions work when they are clicked.
        ("pptx::k0034_audit_slideshow_questions_primary_click", primary_click),
    ]
}

fn x(id: &str) -> f32 {
    f32::from(buttons::bounds(id).unwrap_or_else(|| panic!("the action bar has {id}")).origin.x)
}

fn draw(s: &mut Scenario) {
    s.update(|e, window, cx| {
        let view = window.viewport_size();
        let p1 = Point::new(view.width * 0.2, view.height * 0.2);
        let p2 = Point::new(view.width * 0.3, view.height * 0.4);
        e.mouse_down_slideshow(p1, false, window, cx);
        e.mouse_move_slideshow(p2, false, window, cx);
        e.mouse_up_slideshow(p2, false, window, cx);
    });
}

/// Start the show with the pen, draw a stroke and press Esc: the question about the ink is open.
fn ink_question_open(s: &mut Scenario) {
    s.key(if cfg!(target_os = "macos") { "cmd-shift-enter" } else { "f5" });
    assert!(s.with(|e, _| e.slideshow().is_some()));
    s.key("ctrl-p");
    draw(s);
    s.key("escape");
    s.settle();
    assert!(s.with(|e, _| e.slideshow().unwrap().ink_dialog_open), "the ink question is open");
}

fn ink_question() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let shapes = |s: &mut Scenario| s.with(|e, _| e.slide().shapes.len());
    let base = shapes(&mut s);
    // The primary button (keep) first, then discard; Esc is the discard.
    ink_question_open(&mut s);
    s.screenshot("slideshow-ink-question");
    assert!(x("ink-dialog-keep") < x("ink-dialog-discard"), "primary first");
    s.key("escape");
    assert!(s.with(|e, _| e.slideshow().is_none()), "Esc answers with the cancel (discard) and ends the show");
    assert_eq!(shapes(&mut s), base, "discard keeps nothing");
    // Enter presses the primary button: keep.
    ink_question_open(&mut s);
    s.key("enter");
    assert!(s.with(|e, _| e.slideshow().is_none()), "Enter presses keep and ends the show");
    assert_eq!(shapes(&mut s), base + 1, "keep puts the stroke on the slide");
}

fn rehearsal_question() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let open = |s: &mut Scenario| {
        s.update(|e, window, cx| {
            let mut props = e.presentation().show_properties.clone();
            props.use_timings = false;
            e.execute(vec![ibis_doc_model::Command::SetShowProperties { properties: props }], cx);
            e.start_rehearsal(window, cx);
        });
        let slide_count = s.with(|e, _| e.presentation().slides.len());
        for _ in 0..slide_count {
            s.update(|e, window, cx| e.step_slideshow(true, window, cx));
        }
        s.settle();
        assert!(s.with(|e, _| e.slideshow().unwrap().rehearsal.as_ref().unwrap().dialog_open), "the rehearsal question is open");
    };
    open(&mut s);
    s.screenshot("slideshow-rehearsal-question");
    assert!(x("rehearsal-dialog-save") < x("rehearsal-dialog-discard"), "primary first");
    s.key("escape");
    assert!(s.with(|e, _| e.slideshow().is_none()), "Esc answers with the cancel (do not save) and ends the show");
    assert!(s.with(|e, _| !e.presentation().show_properties.use_timings), "not saved");
    open(&mut s);
    s.key("enter");
    assert!(s.with(|e, _| e.slideshow().is_none()), "Enter presses the primary button and ends the show");
    assert!(s.with(|e, _| e.presentation().show_properties.use_timings), "the timings are saved");
}

/// #261: a click on the primary button of each question does what it says and ends the show.
fn primary_click() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let base = s.with(|e, _| e.slide().shapes.len());
    ink_question_open(&mut s);
    buttons::click(&mut s, "ink-dialog-keep");
    s.settle();
    assert!(s.with(|e, _| e.slideshow().is_none()), "a click on keep ends the show");
    assert_eq!(s.with(|e, _| e.slide().shapes.len()), base + 1, "keep puts the stroke on the slide");
    s.update(|e, window, cx| {
        let mut props = e.presentation().show_properties.clone();
        props.use_timings = false;
        e.execute(vec![ibis_doc_model::Command::SetShowProperties { properties: props }], cx);
        e.start_rehearsal(window, cx);
    });
    let slide_count = s.with(|e, _| e.presentation().slides.len());
    for _ in 0..slide_count {
        s.update(|e, window, cx| e.step_slideshow(true, window, cx));
    }
    s.settle();
    buttons::click(&mut s, "rehearsal-dialog-save");
    s.settle();
    assert!(s.with(|e, _| e.slideshow().is_none()), "a click on the primary button ends the show");
    assert!(s.with(|e, _| e.presentation().show_properties.use_timings), "the timings are saved");
}
