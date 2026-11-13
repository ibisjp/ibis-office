// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue #108: pasting text that holds a tab (and other control characters) into the cell being edited must not crash.
use super::Scenario;
use crate::xlsx::scenarios::SheetSteps;
use gpui_kit::*;
use crate::xlsx::ext::*;
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("xlsx::issue108_paste_tab_cell_editor", cell_editor), ("xlsx::issue108_paste_tab_wide_characters", tab_and_wide_characters), ("xlsx::issue108_paste_matrix_cell_editor", matrix_cell_editor), ("xlsx::issue108_paste_matrix_formula_bar", matrix_formula_bar), ("xlsx::issue108_paste_matrix_name_box", matrix_name_box), ("xlsx::issue108_paste_matrix_find_field", matrix_find_field)]
}
fn paste(s: &mut Scenario, text: &str) {
    s.update(|_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.to_string())));
    s.key("cmd-v");
}
fn workbook() -> Scenario {
    let mut s = Scenario::start_view();
    s.size(1000., 700.);
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    s
}
fn cell_editor() {
    let mut s = workbook();
    s.click_cell("A1");
    s.key("f2");
    s.update(|_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string("a\tb".to_string())));
    s.key("cmd-v");
    let text = s.with(|e, _| e.cell_edit().unwrap().text.clone());
    assert_eq!(text, "a\tb");
    // The tab is kept in the text and drawn as nothing: the caret before and after it is at one place, and "b" lies after it.
    let (before, after, end) = s.with(|e, _| {
        let layout = e.cell_edit().unwrap().layout.borrow().clone().unwrap();
        let line = &layout.lines[0];
        (line.x_at(1), line.x_at(2), line.x_at(3))
    });
    assert_eq!(before, after, "a tab is no room");
    assert!(end > after, "b is drawn after the tab");
    s.screenshot("issue108-tab");
}

/// Tabs between characters of more than one byte: every caret place is found and the lines are cut at characters.
fn tab_and_wide_characters() {
    let mut s = workbook();
    s.click_cell("A1");
    s.key("f2");
    let text = "\u{1F600}\t\u{1F600}\t日本語"; // i18n-ok: Japanese test fixture, not a UI label.
    paste(&mut s, text);
    assert_eq!(s.with(|e, _| e.cell_edit().unwrap().text.clone()), text);
    let xs: Vec<f32> = s.with(|e, _| {
        let layout = e.cell_edit().unwrap().layout.borrow().clone().unwrap();
        text.char_indices().map(|(b, _)| layout.lines[0].x_at(b)).chain([layout.lines[0].x_at(text.len())]).collect()
    });
    assert!(xs.windows(2).all(|w| w[0] <= w[1]), "the caret goes right: {xs:?}");
    assert_eq!(xs[1], xs[2], "no room for the tab");
    for _ in 0..8 {
        s.key("left");
    }
    s.key("shift-end");
    s.screenshot("issue108-tab-wide");
}

/// Texts that hold what a text field might not take: tabs, line breaks of every kind, NUL, other control characters,
/// surrogate pairs, combining marks, a very long run.
fn texts() -> Vec<String> {
    let mut v: Vec<String> = ["a\tb", "\t", "\t\t", "a\t", "\ta", "a\t\tb", "a\tb\nc\td", "a\tb\r\nc\td\r\n", "a\rb", "a\r", "a\0b", "\0", "a\u{b}b\u{c}c\u{7}d\u{1b}e\u{7f}", "\u{1F600}\t\u{1F600}", "e\u{301}\t\u{301}", "\u{200d}\t\u{200b}", "=A1\t+B1", "=SUM(A1\t,B2)\t", "\u{2028}x\u{2029}y\u{85}z", "日本語\t文字\tです", "x\ty ", " \t "] // i18n-ok: Japanese test fixture, not a UI label.
        .iter().map(|s| s.to_string()).collect();
    v.push(format!("{}\t{}", "a".repeat(3000), "b".repeat(3000)));
    v.push(vec!["col\tval"; 400].join("\n"));
    v
}
fn matrix_cell_editor() {
    for (i, text) in texts().into_iter().enumerate() {
        eprintln!("issue108 cell editor case {i}: {:?}", text.chars().take(24).collect::<String>());
        let mut s = workbook();
        s.click_cell("B2");
        s.key("f2");
        s.update(|_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone())));
        s.key("cmd-v");
        s.screenshot(&format!("issue108-cell-{i}"));
        for k in ["left", "shift-left", "home", "end", "ctrl-a", "right", "up", "down"] {
            s.key(k);
        }
        s.key("cmd-v");
        s.screenshot(&format!("issue108-cell-{i}-again"));
        s.key("enter");
    }
}

/// The formula bar: a press on it begins the edit of the cell, and the paste goes to the same text.
fn matrix_formula_bar() {
    for (i, text) in texts().into_iter().enumerate() {
        eprintln!("issue108 formula bar case {i}: {:?}", text.chars().take(24).collect::<String>());
        let mut s = workbook();
        s.click_cell("B2");
        let bar = s.with(|e, _| e.wb().bar_bounds.get().unwrap());
        s.click((f32::from(bar.left()) + 12., f32::from(bar.top()) + 8.));
        s.update(|_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone())));
        s.key("cmd-v");
        s.screenshot(&format!("issue108-bar-{i}"));
        for k in ["left", "shift-left", "home", "end", "up", "down", "ctrl-home"] {
            s.key(k);
        }
        s.update(|e, _, cx| e.toggle_formula_bar_expanded(cx));
        s.screenshot(&format!("issue108-bar-{i}-expanded"));
        s.key("enter");
    }
}

/// The name box (cell reference field) at the left of the formula bar.
fn matrix_name_box() {
    for (i, text) in texts().into_iter().enumerate() {
        eprintln!("issue108 name box case {i}: {:?}", text.chars().take(24).collect::<String>());
        let mut s = workbook();
        s.click_cell("B2");
        s.update(|e, w, cx| {
            let name_box = e.sheet_state().unwrap().jump.name_box.clone();
            name_box.update(cx, |input, cx| input.focus(w, cx));
        });
        s.update(|_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone())));
        s.key("cmd-v");
        if i == 0 {
            let value = s.with(|e, cx| e.sheet_state().unwrap().jump.name_box.read(cx).value().to_string());
            assert!(value.contains('a') && value.contains('b'), "the name box took the paste: {value:?}");
        }
        s.screenshot(&format!("issue108-name-{i}"));
        s.key("enter");
        s.key("escape");
    }
}

/// The find field of the right pane.
fn matrix_find_field() {
    for (i, text) in texts().into_iter().enumerate() {
        eprintln!("issue108 find field case {i}: {:?}", text.chars().take(24).collect::<String>());
        let mut s = workbook();
        s.click_cell("B2");
        s.key("secondary-f");
        s.update(|_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone())));
        s.key("cmd-v");
        if i == 0 {
            let value = s.with(|e, cx| e.sheet_state().and_then(|st| st.find.as_ref()).map(|f| f.query.read(cx).value().to_string()));
            assert!(value.as_deref().is_some_and(|v| v.contains('a') && v.contains('b')), "the find field took the paste: {value:?}");
        }
        s.screenshot(&format!("issue108-find-{i}"));
        s.key("enter");
        s.key("shift-enter");
        s.key("escape");
    }
}
