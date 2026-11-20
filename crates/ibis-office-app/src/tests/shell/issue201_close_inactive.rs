// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #201: the close button of an edited window that is not the active one asks whether to save. The question must be
//! seen: the window is made the active one (brought to the front) before it asks. Whether it is really in front is checked on a PC.
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::issue201_close_button_activates_window", close_button),
        ("shell::issue201_quit_activates_window_with_question", quit),
    ]
}

/// An edited document window (the scenario's), then another window opened after it, which is the active one.
fn edited_behind() -> (Scenario, gpui_kit::AnyWindowHandle) {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_formats::Kind::Document, w, cx));
    s.quit_with_last_window();
    let (id, _) = crate::docx::scenarios::DocSteps::paragraph(&mut s, 0);
    crate::docx::scenarios::DocSteps::click_text(&mut s, ibis_doc_model::FlowPos::new(id, 0));
    s.type_text("unsaved");
    assert!(s.with(|e, _| e.is_dirty()));
    let mine = s.with_app(|app| app.windows()[0]);
    s.with_app(|app| crate::open_editor(None, None, app));
    s.settle();
    assert_eq!(s.window_count(), 2);
    let active = s.with_app(|app| app.active_window());
    assert_ne!(active.map(|w| w.window_id()), Some(mine.window_id()), "the edited window is behind the other one");
    (s, mine)
}

fn close_button() {
    let (mut s, mine) = edited_behind();
    assert!(!s.press_close_button(), "the edited window stays and asks");
    s.settle();
    assert!(s.with(|e, _| e.save_confirm_dialog_open));
    let active = s.with_app(|app| app.active_window());
    assert_eq!(active.map(|w| w.window_id()), Some(mine.window_id()), "the window with the question is the active one");
}

fn quit() {
    let (mut s, mine) = edited_behind();
    assert!(!s.with_app(|app| app.should_quit()));
    s.settle();
    assert!(s.with(|e, _| e.save_confirm_dialog_open));
    let active = s.with_app(|app| app.active_window());
    assert_eq!(active.map(|w| w.window_id()), Some(mine.window_id()), "quitting brings the window with the question to the front");
}
