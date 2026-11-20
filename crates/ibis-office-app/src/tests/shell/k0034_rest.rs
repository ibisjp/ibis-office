// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0034 (the rest): the message of `show_panel_message`, the zoom dialog, the slide, section and summary zoom dialogs, the table
//! dialog and the Go To dialog are the shared dialog (a modal panel with the shared action bar, primary button first, Escape
//! closes) or the shared message box; each action still works.
use crate::shell::scenario::{Scenario, button_probe as buttons};
use crate::xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0034_rest_panel_message_is_the_message_box", panel_message),
        ("shell::k0034_rest_zoom_dialog_is_shared", zoom_dialog),
        ("shell::k0034_rest_slide_zoom_dialogs_are_shared", slide_zoom_dialogs),
        ("shell::k0034_rest_table_dialog_is_shared", table_dialog),
        ("shell::k0034_rest_goto_dialog_is_shared", goto_dialog),
    ]
}

fn x(id: &str) -> f32 {
    f32::from(buttons::bounds(id).unwrap_or_else(|| panic!("the action bar has {id}")).origin.x)
}

/// The dialog `key` is the open modal shared dialog.
fn modal(s: &mut Scenario, key: &'static str) -> bool {
    s.with(|e, _| e.dialog_is_modal() && e.dialog.key == Some(key))
}

fn panel_message() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.update(|e, _, cx| e.show_panel_message("message".into(), cx));
    s.settle();
    s.screenshot("panel-message");
    assert!(s.with(|e, _| e.panel_message.is_some()));
    assert!(buttons::bounds("panel-message-ok").is_some(), "the message box has the shared OK button");
    s.key("escape");
    assert!(s.with(|e, _| e.panel_message.is_none()), "Esc closes the message");
    s.update(|e, _, cx| e.show_panel_message("message".into(), cx));
    s.settle();
    s.key("enter");
    assert!(s.with(|e, _| e.panel_message.is_none()), "Enter presses OK");
    s.update(|e, _, cx| e.show_panel_message("message".into(), cx));
    s.settle();
    buttons::click(&mut s, "panel-message-ok");
    assert!(s.with(|e, _| e.panel_message.is_none()), "the OK button closes the message");
}

fn zoom_dialog() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.update(|e, w, cx| e.open_zoom_dialog(w, cx));
    s.settle();
    assert!(modal(&mut s, "zoom_dialog"), "the zoom dialog is a modal shared dialog");
    assert!(s.with(|e, _| e.zoom_dialog_open));
    s.screenshot("zoom-dialog");
    assert!(x("zoom-dialog-ok") < x("dialog-cancel"), "primary, then cancel");
    s.key("escape");
    assert!(s.with(|e, _| !e.zoom_dialog_open && !e.dialog.active), "Esc closes");
    s.update(|e, w, cx| e.open_zoom_dialog(w, cx));
    s.settle();
    s.update(|e, w, cx| e.zoom_dialog_input.update(cx, |i, cx| i.set_value("75", w, cx)));
    buttons::click(&mut s, "zoom-dialog-ok");
    assert!(s.with(|e, _| !e.zoom_dialog_open && !e.dialog.active));
    assert_eq!(s.with(|e, _| e.zoom), Some(0.75), "OK applies the percent");
    s.update(|e, w, cx| e.open_zoom_dialog(w, cx));
    s.settle();
    s.update(|e, w, cx| e.fit_zoom_dialog(w, cx));
    assert!(s.with(|e, _| !e.zoom_dialog_open && e.zoom.is_none()), "the fit button applies and closes");
}

fn slide_zoom_dialogs() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(1);
    let ids: Vec<_> = s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.id).collect());
    s.update(|e, _, cx| e.run(vec![ibis_doc_model::Command::SetSections { sections: vec![
        ibis_doc_model::Section { id: "{11111111-1111-1111-1111-111111111111}".into(), name: "a".into(), slides: vec![ids[0]] },
        ibis_doc_model::Section { id: "{22222222-2222-2222-2222-222222222222}".into(), name: "b".into(), slides: ids[1..].to_vec() },
    ] }], cx));
    let shapes = |s: &mut Scenario| s.with(|e, _| e.presentation().slides.iter().map(|sl| sl.shapes.len()).sum::<usize>());
    for (menu, key, ok) in [
        ("pptx.insert.slide-zoom", "slide_zoom_dialog", "zoom-insert"),
        ("pptx.insert.section-zoom", "section_zoom_dialog", "section-zoom-insert"),
        ("pptx.insert.summary-zoom", "summary_zoom_dialog", "summary-zoom-insert"),
    ] {
        s.menu(menu);
        s.settle();
        assert!(modal(&mut s, key), "{key} is a modal shared dialog");
        s.screenshot(key);
        assert!(x(ok) < x("dialog-cancel"), "{key}: primary, then cancel");
        s.key("escape");
        assert!(s.with(|e, _| !e.dialog.active), "{key}: Esc closes");
        let before = shapes(&mut s);
        s.menu(menu);
        s.settle();
        buttons::click(&mut s, ok);
        assert!(s.with(|e, _| !e.dialog.active), "{key}: the insert button closes");
        assert!(shapes(&mut s) > before, "{key}: a zoom is inserted");
    }
}

fn table_dialog() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-compatibility-v1.xlsx");
    s.menu("xlsx.insert-table");
    s.settle();
    assert!(modal(&mut s, "table_dialog"), "the table dialog is a modal shared dialog");
    s.screenshot("table-dialog");
    assert!(x("table-ok") < x("dialog-cancel"), "primary, then cancel");
    s.key("escape");
    assert!(s.with(|e, _| !e.dialog.active), "Esc closes");
    s.menu("xlsx.insert-table");
    s.settle();
    // A free range far from the data (the sheet may already have tables around the selection).
    s.key(if cfg!(target_os = "macos") { "cmd-a" } else { "ctrl-a" });
    s.type_text("=Z50:AA52");
    buttons::click(&mut s, "table-ok");
    assert!(s.with(|e, _| !e.dialog.active), "OK makes the table and closes");
}

fn goto_dialog() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-compatibility-v1.xlsx");
    let open = |s: &mut Scenario| { s.menu("edit.go-to"); s.settle(); };
    open(&mut s);
    assert!(modal(&mut s, "goto_dialog"), "the Go To dialog is a modal shared dialog");
    s.screenshot("goto-dialog");
    assert!(x("goto-ok") < x("goto-special") && x("goto-special") < x("dialog-cancel"), "primary, special, cancel");
    s.key("escape");
    assert!(s.with(|e, _| !e.dialog.active), "Esc closes");
    open(&mut s);
    s.type_text("C3");
    buttons::click(&mut s, "goto-ok");
    assert!(s.with(|e, _| !e.dialog.active), "OK closes");
    assert_eq!(s.with(|e, _| e.current_cell()), Some((2, 2)), "OK jumps to the reference");
}
