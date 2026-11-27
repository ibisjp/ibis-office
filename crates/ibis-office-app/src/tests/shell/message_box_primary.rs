// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue #261: the primary button of every kind of message box works when it is clicked: it closes the box, and does what the box
//! asked. (The table of the message boxes is `tasks/doc/reports/2026-11-21-261-message-boxes.md`.)
use super::{Scenario, button_probe as buttons};
use crate::shell::editor::SlideEditor;
use std::{cell::Cell, rc::Rc};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::message_box_primary_alert_and_panel_message", alert_and_panel_message),
        ("shell::message_box_primary_ask_kinds", ask_kinds),
        ("shell::message_box_primary_confirmations", confirmations),
    ]
}

/// Messages with one button: the OK closes them.
fn alert_and_panel_message() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.update(|e, _, cx| e.show_alert("Title".into(), "Message".into(), cx));
    s.settle();
    let ids = buttons::all();
    assert_eq!(ids.iter().filter(|id| id.contains("alert-")).count(), 1, "an alert has one button: {ids:?}");
    buttons::click(&mut s, "alert-ok");
    s.settle();
    assert!(s.with(|e, _| e.alert.is_none()), "OK closes the alert");
    s.update(|e, _, cx| e.show_panel_message("Message".into(), cx));
    s.settle();
    let ids = buttons::all();
    assert_eq!(ids.iter().filter(|id| id.contains("panel-message-")).count(), 1, "a panel message has one button: {ids:?}");
    buttons::click(&mut s, "panel-message-ok");
    s.settle();
    assert!(s.with(|e, _| e.panel_message.is_none()), "OK closes the panel message");
}

/// The questions (`ask`, `ask_yes_no`, `ask_with_buttons`): the primary button runs what was asked and closes; the other does not run it.
fn ask_kinds() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let ran = Rc::new(Cell::new(0));
    for kind in 0..3 {
        for answer in ["ask-cancel", "ask-ok"] {
            let r = ran.clone();
            s.update(move |e, _, cx| {
                let on_ok: Rc<dyn Fn(&mut SlideEditor, &mut gpui_kit::Context<SlideEditor>)> = Rc::new(move |_, _| r.set(r.get() + 1));
                match kind {
                    0 => e.ask("t".into(), "m".into(), on_ok, cx),
                    1 => e.ask_yes_no("t".into(), "m".into(), on_ok, cx),
                    _ => e.ask_with_buttons("t".into(), "m".into(), "Do".into(), "Close".into(), on_ok, cx),
                }
            });
            s.settle();
            let before = ran.get();
            buttons::click(&mut s, answer);
            s.settle();
            assert!(s.with(|e, _| e.ask.is_none()), "kind {kind}: {answer} closes the question");
            assert_eq!(ran.get(), before + usize::from(answer == "ask-ok"), "kind {kind}: only the primary runs what was asked ({answer})");
        }
    }
}

/// The boxes the editor draws itself: the save question, the question about the Office application's path, the comment delete.
fn confirmations() {
    let _probe = buttons::Probe::new();
    // 保存 writes the file and closes the box.
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let path = s.with(|e, _| e.path.clone()).expect("the copy's path");
    let before = std::fs::metadata(&path).and_then(|m| m.modified()).expect("the copy's time");
    s.update(|e, _, cx| { e.save_confirm_dialog_open = true; cx.notify(); });
    s.settle();
    buttons::click(&mut s, "save-confirm-save");
    s.settle();
    assert!(s.with(|e, _| !e.save_confirm_dialog_open), "保存 closes the save question");
    let after = std::fs::metadata(&path).and_then(|m| m.modified()).expect("the copy's time");
    assert!(after > before, "保存 writes the file");
    drop(s);
    // The path question: OK closes it (and goes on to ask for the application).
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.update(|e, _, cx| { e.office_path_confirm_dialog_open = true; cx.notify(); });
    s.settle();
    buttons::click(&mut s, "office-path-confirm-ok");
    s.settle();
    assert!(s.with(|e, _| !e.office_path_confirm_dialog_open), "OK closes the path question");
    drop(s);
    // The comment delete: the red button does the delete and closes the box.
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let ran = Rc::new(Cell::new(false));
    let r = ran.clone();
    s.update(move |e, _, cx| {
        e.comment_delete = Some(crate::shell::comments::PendingDelete { ui: Default::default(), confirm: Rc::new(move |_, _| r.set(true)) });
        cx.notify();
    });
    s.settle();
    buttons::click(&mut s, "comment-delete-ok");
    s.settle();
    assert!(ran.get() && s.with(|e, _| e.comment_delete.is_none()), "the delete button deletes and closes");
}
