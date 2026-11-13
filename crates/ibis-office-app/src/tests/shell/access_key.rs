// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Alt access keys of the action bar (issue #118): assignment, the key press on message boxes and dialogs, the
//! macOS-like setting (no key, no underline), and the underline picture.
use super::{Scenario, button_probe as buttons};
use crate::shell::access_key::{self, resolve};
use crate::docx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::access_key_assignment", assignment),
        ("shell::access_key_alert_and_save_confirm", alert_and_save_confirm),
        ("shell::save_confirm_by_the_keyboard", save_confirm_by_the_keyboard),
        ("shell::message_box_keys_ask_and_alert", message_box_keys_ask_and_alert),
        ("shell::access_key_dialog", dialog),
        ("shell::access_key_off_on_mac", off_on_mac),
    ]
}

fn texts(labels: &[&str]) -> Vec<(String, Option<char>)> {
    resolve(labels, true).into_iter().map(|r| (r.text, r.key)).collect()
}

fn assignment() {
    // Automatic: word initials first, no repeats in a bar.
    assert_eq!(texts(&["Cancel", "Don't Save", "Save"]), vec![("Cancel".into(), Some('c')), ("Don't Save".into(), Some('d')), ("Save".into(), Some('s'))]);
    assert_eq!(texts(&["Close", "Copy"]).iter().map(|t| t.1).collect::<Vec<_>>(), vec![Some('c'), Some('o')]);
    assert_eq!(texts(&["Save", "Save As"]).iter().map(|t| t.1).collect::<Vec<_>>(), vec![Some('s'), Some('a')]);
    // Japanese: the well-known names get the Windows `保存(S)` form; an unknown one gets none.
    assert_eq!(texts(&["キャンセル", "保存しない", "保存", "謎のボタン"]), // i18n-ok (Japanese text under test or a name table that matches labels)
        vec![("キャンセル(C)".into(), Some('c')), ("保存しない(N)".into(), Some('n')), ("保存(S)".into(), Some('s')), ("謎のボタン".into(), None)]); // i18n-ok (Japanese text under test or a name table that matches labels)
    // Marks: `&X`, `(&X)`, `&&`, and the `&` that is not a mark.
    let r = resolve(&["&Save", "保存(&D)", "Fish && Chips", "R&D", "A & B"], true); // i18n-ok (Japanese text under test or a name table that matches labels)
    assert_eq!((r[0].text.as_str(), r[0].key, r[0].underline.clone()), ("Save", Some('s'), Some(0..1)));
    assert_eq!((r[1].text.as_str(), r[1].key, r[1].underline.clone()), ("保存(D)", Some('d'), Some(7..8))); // i18n-ok (Japanese text under test or a name table that matches labels)
    assert_eq!(r[2].text, "Fish & Chips");
    assert_eq!(r[3].text, "R&D");
    assert_eq!(r[4].text, "A & B");
    // A marked letter wins over an automatic one that came first.
    let r = resolve(&["Save", "Sa&ve"], true);
    assert_eq!((r[0].key, r[1].key), (Some('s'), Some('v')));
    // Off: plain text, no key.
    let r = resolve(&["&Save", "保存"], false); // i18n-ok (Japanese text under test or a name table that matches labels)
    assert_eq!((r[0].text.as_str(), r[0].key, r[1].text.as_str(), r[1].key), ("Save", None, "保存", None)); // i18n-ok (Japanese text under test or a name table that matches labels)
    assert_eq!(access_key::plain("保存(&S)"), "保存(S)"); // i18n-ok (Japanese text under test or a name table that matches labels)
}

fn alert_and_save_confirm() {
    let _on = access_key::override_supported(true);
    let _probe = buttons::Probe::new();
    // A message box: Alt+O presses OK.
    let mut s = Scenario::start_view();
    s.update(|e, _, cx| e.show_alert("Title".into(), "Message".into(), cx));
    s.settle();
    s.screenshot("access-key-alert");
    s.key("o");
    assert!(s.with(|e, _| e.alert.is_some()), "a plain letter is not the access key");
    s.key("alt-x");
    assert!(s.with(|e, _| e.alert.is_some()), "another letter does nothing");
    s.key("ctrl-alt-o");
    assert!(s.with(|e, _| e.alert.is_some()), "Ctrl+Alt is not Alt");
    s.key("alt-o");
    assert!(s.with(|e, _| e.alert.is_none()), "Alt+O presses OK");

    drop(s);
    // The save confirmation (save, cancel, don't save): Alt+C cancels, and the box closes.
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.update(|e, _, cx| { e.save_confirm_dialog_open = true; cx.notify(); });
    s.settle();
    s.screenshot("access-key-save-confirm");
    s.key("alt-c");
    assert!(s.with(|e, _| !e.save_confirm_dialog_open), "Alt+C cancels the save confirmation");
    assert_eq!(access_key::duplicate_marks(), 0);

    // Enter presses the primary button, 保存 (2026-11-12): the box closes and the file (a copy in the scenario's folder) is written.
    drop(s);
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let path = s.with(|e, _| e.path.clone()).expect("the copy's path");
    let before = std::fs::metadata(&path).and_then(|m| m.modified()).expect("the copy's time");
    s.update(|e, _, cx| { e.save_confirm_dialog_open = true; cx.notify(); });
    s.settle();
    s.key("enter");
    s.settle();
    assert!(s.with(|e, _| !e.save_confirm_dialog_open), "Enter closes the save confirmation");
    let after = std::fs::metadata(&path).and_then(|m| m.modified()).expect("the copy's time");
    assert!(after > before, "Enter saves the file, as the Save button does");
}

/// Every message box by the keyboard: ask (OK and キャンセル, then はい and いいえ) and alert (OK only).
fn message_box_keys_ask_and_alert() {
    use std::{cell::Cell, rc::Rc};
    use crate::shell::editor::SlideEditor;
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let ran = Rc::new(Cell::new(0));
    let ask = |s: &mut Scenario, yes_no: bool| {
        let ran = ran.clone();
        s.update(move |e, _, cx| {
            let on_ok: Rc<dyn Fn(&mut SlideEditor, &mut gpui_kit::Context<SlideEditor>)> = Rc::new(move |_, _| ran.set(ran.get() + 1));
            if yes_no { e.ask_yes_no("t".into(), "m".into(), on_ok, cx) } else { e.ask("t".into(), "m".into(), on_ok, cx) }
        });
        s.settle();
    };
    // OK is the primary button: it has the ring at first; Enter answers it.
    ask(&mut s, false);
    assert_eq!(s.with(|e, _| e.message_box_focus()), 0);
    s.key("tab");
    assert_eq!(s.with(|e, _| e.message_box_focus()), 1, "Tab: to cancel");
    s.key("tab");
    assert_eq!(s.with(|e, _| e.message_box_focus()), 0, "Tab: round to OK");
    s.key("shift-tab");
    s.settle();
    s.screenshot("ask-focus-cancel");
    s.key("space");
    s.settle();
    assert!(s.with(|e, _| e.ask.is_none()) && ran.get() == 0, "Space on cancel closes without running");
    // Esc is the cancel, whichever button the ring is on.
    ask(&mut s, true);
    s.key("escape");
    assert!(s.with(|e, _| e.ask.is_none()) && ran.get() == 0, "Esc answers no");
    ask(&mut s, true);
    s.key("enter");
    assert!(s.with(|e, _| e.ask.is_none()) && ran.get() == 1, "Enter answers yes");
    // Alert: one button; Tab stays, Enter and Esc close it.
    for key in ["enter", "escape", "space"] {
        s.update(|e, _, cx| e.show_alert("t".into(), "m".into(), cx));
        s.settle();
        s.key("tab");
        assert!(s.with(|e, _| e.alert.is_some()), "Tab keeps the alert");
        s.key(key);
        assert!(s.with(|e, _| e.alert.is_none()), "{key} closes the alert");
    }
}

fn dialog() {
    let _on = access_key::override_supported(true);
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.update(|e, window, cx| e.open_font_edit(window, cx));
    s.settle();
    assert!(s.with(|e, _| e.dialog_is_modal()));
    s.screenshot("access-key-dialog");
    s.key("alt-c");
    assert!(s.with(|e, _| !e.dialog.active || !e.dialog_is_modal()), "Alt+C presses Cancel of the dialog");
    assert_eq!(access_key::duplicate_marks(), 0);
}

fn off_on_mac() {
    let _off = access_key::override_supported(false);
    let _probe = buttons::Probe::new();
    let mut s = Scenario::start_view();
    s.update(|e, _, cx| e.show_alert("Title".into(), "Message".into(), cx));
    s.settle();
    s.screenshot("access-key-alert-mac");
    s.key("alt-o");
    assert!(s.with(|e, _| e.alert.is_some()), "no access key on macOS");
    buttons::click(&mut s, "alert-ok");
    assert!(s.with(|e, _| e.alert.is_none()));
}

/// K0012 (2026-11-12): the save prompt by the keyboard alone. Esc closes it (キャンセル); Tab and Shift+Tab go round 保存, キャンセル and
/// 保存しない (a focus ring on the one the keyboard is on); Enter presses that one.
fn save_confirm_by_the_keyboard() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    let open = |s: &mut Scenario| s.update(|e, _, cx| { e.ask_to_save(false, cx); });
    open(&mut s);
    s.settle();
    s.key("escape");
    assert!(s.with(|e, _| !e.save_confirm_dialog_open), "Esc closes the prompt");
    open(&mut s);
    s.settle();
    s.key("tab");
    assert_eq!(s.with(|e, _| e.message_box_focus()), 1, "Tab: to cancel");
    s.key("tab");
    assert_eq!(s.with(|e, _| e.message_box_focus()), 2, "Tab: to don't save");
    s.key("tab");
    assert_eq!(s.with(|e, _| e.message_box_focus()), 0, "Tab: round to save");
    s.key("shift-tab");
    assert_eq!(s.with(|e, _| e.message_box_focus()), 2, "Shift+Tab: back to don't save");
    s.key("shift-tab");
    s.settle();
    s.screenshot("save-confirm-focus-cancel");
    s.key("enter");
    assert!(s.with(|e, _| !e.save_confirm_dialog_open), "Enter on cancel closes the prompt");
    assert!(s.with(|e, _| e.path.is_some()), "the window is still open with its file");
}
