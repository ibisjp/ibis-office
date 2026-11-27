// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the ＜ and the format's icon of the title row that go back to the start window (K0067): shown on a device with a finger
//! (not a mouse-only computer); where one window shows the start view and the file in turn the file is closed (asking first when it has
//! unsaved edits) and the window stays; on a computer the start window comes to the front and the file stays open.
use crate::shell::scenario::{Scenario, button_probe as buttons};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0067_back_buttons_follow_touch", back_buttons_follow_touch),
        ("shell::k0067_single_window_closes_the_file_and_stays", single_window_closes_the_file),
        ("shell::k0067_single_window_asks_before_losing_edits", single_window_asks),
        ("shell::k0067_computer_keeps_the_file_open", computer_keeps_the_file),
    ]
}

fn document(touch: bool) -> Scenario {
    document_of(ibis_formats::Kind::Document, touch)
}

fn document_of(kind: ibis_formats::Kind, touch: bool) -> Scenario {
    let mut s = Scenario::new_document(kind);
    s.size(1000., 700.);
    s.update(move |e, _, cx| {
        e.touch_seen = touch;
        cx.notify();
    });
    s.settle();
    s
}

fn ids(s: &mut Scenario) -> Vec<String> {
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    buttons::all().into_iter().filter(|id| id.starts_with("back-to-start")).collect()
}

fn back_buttons_follow_touch() {
    let mut s = document(false);
    assert!(ids(&mut s).is_empty(), "a mouse-only computer has no ＜");
    s.update(|e, _, cx| {
        e.touch_seen = true;
        cx.notify();
    });
    s.settle();
    let shown = ids(&mut s);
    assert!(shown.iter().any(|id| id == "back-to-start") && shown.iter().any(|id| id == "back-to-start-icon"), "the ＜ and the icon: {shown:?}");
}

fn single_window_closes_the_file() {
    let mut s = document(true);
    s.update(|e, window, cx| e.back_to_start_on(true, window, cx));
    s.settle();
    assert!(!s.with(|e, _| e.has_document), "the start view is shown in the same window");
    assert!(!s.with(|e, _| e.save_confirm_dialog_open));
}

fn single_window_asks() {
    let mut s = document_of(ibis_formats::Kind::Workbook, true);
    s.type_text("abc");
    assert!(s.with(|e, _| e.is_dirty()));
    s.update(|e, window, cx| e.back_to_start_on(true, window, cx));
    s.settle();
    assert!(s.with(|e, _| e.has_document && e.save_confirm_dialog_open), "it asks first, and the file stays until it is answered");
}

fn computer_keeps_the_file() {
    let mut s = document_of(ibis_formats::Kind::Workbook, true);
    s.type_text("abc");
    s.update(|e, window, cx| e.back_to_start_on(false, window, cx));
    s.settle();
    assert!(s.with(|e, _| e.has_document && !e.save_confirm_dialog_open), "the file is open and nothing was asked");
    assert!(s.with(|e, _| e.is_dirty()), "and unchanged");
}
