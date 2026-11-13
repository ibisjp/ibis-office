// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Format Cells (the sidebar pane), second round (2026-11-03): the labels and the order of the rows, the icons of the Number Format
//! list, None first in the pattern list, Numeric for the text direction, the font row, ▲▼ fields without ✕ that select all
//! when focused, Live Update (ON / OFF with Apply, kept in the settings file) and the gradient slider.

use super::SheetSteps;
use crate::shell::chrome::MenuEntry;
use crate::shell::i18n;
use crate::shell::scenario::{Scenario, button_probe as buttons};
use ibis_doc_model::{CellGradient, Rgb, StyleChange};
use crate::ext::*;

const LIVE_KEY: &str = "xlsx.format-pane.live-update";

fn open() -> Scenario {
    // The default is ON: the setting is cleared (a file left by another scenario would say otherwise).
    crate::shell::settings::set(LIVE_KEY, "1");
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 1100.);
    s.click_cell("B4");
    s.key("cmd-1");
    s.settle();
    s
}
fn style(s: &mut Scenario) -> ibis_doc_model::CellStyle {
    s.with(|e, _| e.current_style())
}
fn panel_style(s: &mut Scenario) -> ibis_doc_model::CellStyle {
    s.with(|e, _| e.panel_style())
}
fn tab(s: &mut Scenario, i: usize) {
    s.update(move |e, _, cx| { e.wb_mut().format_tab = i; cx.notify(); });
    s.settle();
}
fn ui(s: &mut Scenario, id: &str) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    let key = id.to_string();
    s.with(move |e, _| e.sheet_state().unwrap().custom_format.as_ref().unwrap().ui.borrow().get(&key).copied()).unwrap_or_else(|| panic!("no part {id} in the pane"))
}
fn has_ui(s: &mut Scenario, id: &str) -> bool {
    let id = id.to_string();
    s.with(move |e, _| e.sheet_state().unwrap().custom_format.as_ref().unwrap().ui.borrow().contains_key(&id))
}
/// Press the part at its left end (the checkbox of a row is as wide as its label, not the row).
fn click_ui(s: &mut Scenario, id: &str) {
    let b = ui(s, id);
    s.click((f32::from(b.origin.x) + 12., f32::from(b.origin.y + b.size.height / 2.)));
    s.settle();
}
fn items(s: &mut Scenario, id: &str) -> Vec<crate::shell::chrome::MenuItem> {
    let id = id.to_string();
    s.with(move |e, _| {
        let menus = e.sheet_state().unwrap().custom_format.as_ref().unwrap().menus.borrow();
        menus.get(&id).unwrap_or_else(|| panic!("no drop-down {id}")).iter().filter_map(|m| if let MenuEntry::Item(i) = m { Some(i.clone()) } else { None }).collect()
    })
}
fn choose(s: &mut Scenario, id: &str, label: &str) {
    let handler = items(s, id).into_iter().find(|i| i.label.as_ref() == label).unwrap_or_else(|| panic!("no item {label}")).handler.expect("a handler");
    s.update(move |e, window, cx| handler(e, window, cx));
}
fn input(s: &mut Scenario, id: &str) -> gpui_kit::Entity<gpui_kit::component::input::InputState> {
    let id = id.to_string();
    s.with(move |e, _| e.sheet_state().unwrap().custom_format.as_ref().unwrap().fields.input(&id).unwrap_or_else(|| panic!("no field {id}")))
}
fn text(s: &mut Scenario, id: &str) -> String {
    let i = input(s, id);
    s.with(move |_, cx| i.read(cx).value().to_string())
}
fn focus(s: &mut Scenario, id: &str) {
    let i = input(s, id);
    s.update(move |_, window, cx| i.update(cx, |i, cx| i.focus(window, cx)));
    s.settle();
}
fn selected_text(s: &mut Scenario, id: &str) -> String {
    let i = input(s, id);
    s.with(move |_, cx| i.read(cx).selected_value().to_string())
}
fn mismatches(s: &mut Scenario) -> Vec<String> {
    let f = s.with(|e, _| e.sheet_state().unwrap().custom_format.as_ref().unwrap().fields.clone());
    s.with(move |e, cx| f.mismatches(e, cx))
}
fn undo_len(s: &mut Scenario) -> usize {
    s.with(|e, _| e.document.undo_len())
}
fn stops(s: &mut Scenario) -> Vec<(u16, Rgb)> {
    match style(s).gradient.expect("a gradient") {
        CellGradient::Linear { stops, .. } | CellGradient::Path { stops, .. } => stops,
    }
}

/// The rows: no group headings (Number Format, Alignment, Font), Category is Number Format and the code field Custom, Text Direction ends with
/// Numeric, the font is Font on the label's line, the style row is under the colour row and Underline has no shortcut.
pub(crate) fn labels_and_order() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    s.screenshot("round2-01-format-tab");
    for section in ["Number", "Alignment", "Font"] {
        assert!(!has_ui(&mut s, &format!("cell-format-section-{section}")), "no heading for {section}");
    }
    assert_eq!(i18n::text("xlsx.format-pane.category").to_string(), "\u{8868}\u{793A}\u{5F62}\u{5F0F}");
    assert_eq!(i18n::text("xlsx.format-pane.custom").to_string(), "\u{30E6}\u{30FC}\u{30B6}\u{30FC}\u{5B9A}\u{7FA9}");
    assert_eq!(i18n::text("_xlsx.font-name").to_string(), "\u{30D5}\u{30A9}\u{30F3}\u{30C8}");
    assert_eq!(i18n::text("xlsx.format-pane.underline").to_string(), "\u{4E0B}\u{7DDA}");
    let t = items(&mut s, "cell-format-rotation");
    assert_eq!(t.last().unwrap().label.to_string(), i18n::text("_xlsx.vertical-text").to_string());
    let numeric = i18n::text("xlsx.format-pane.rotation-numeric").to_string();
    assert_eq!(numeric, "\u{6570}\u{5024}\u{6307}\u{5B9A}");
    let at = t.iter().position(|i| i.label.as_ref() == "180°").unwrap();
    assert_eq!(t[at + 1].label.to_string(), numeric, "Numeric follows 180°");
    assert!(t.iter().all(|i| i.label.as_ref() != i18n::text("xlsx.format-pane.custom").as_ref()), "the text direction has no Custom");
    // The style buttons are under the font colour (the two exchanged), and both under the font and the size.
    let colour = ui(&mut s, "cell-format-color");
    let bold = buttons::bounds("cell-format-bold").expect("the style buttons");
    assert!(colour.origin.y < bold.origin.y, "the colour is above the style buttons");
    let font = buttons::bounds("panel-font-xlsx-custom-format-1").expect("the font dropdown");
    assert!(font.origin.y < colour.origin.y, "the font is above the colour");
    // The font dropdown shares the row of its label: it starts where the other dropdowns of the pane do (right of the label column).
    let halign = ui(&mut s, "cell-format-halign");
    assert!(font.left() >= halign.left() && font.right() <= halign.right() + gpui_kit::px(1.), "the font is in the control column: {font:?} / {halign:?}");
    assert!(font.size.width <= crate::shell::chrome::FONT_DROPDOWN_WIDTH + gpui_kit::px(1.), "at most the font dropdown's width");
}

/// The Number Format list: every item has an icon (the toolbar's 123 menu too), the list keeps the icons and shows the choice by its row.
pub(crate) fn number_format_items_have_icons() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    let list = items(&mut s, "cell-format-number");
    assert_eq!(list.len(), crate::toolbar::NUMBER_FORMATS.len() + 1, "the formats and Custom");
    assert!(list.iter().all(|i| i.icon.is_some_and(|n| !n.is_empty())), "every item has an icon: {:?}", list.iter().map(|i| (i.label.to_string(), i.icon)).collect::<Vec<_>>());
    let mut icons: Vec<_> = list.iter().take(crate::toolbar::NUMBER_FORMATS.len()).filter_map(|i| i.icon).collect();
    icons.sort();
    icons.dedup();
    assert_eq!(icons.len(), crate::toolbar::NUMBER_FORMATS.len(), "the formats have different icons");
    for name in icons.iter().copied().chain(["tune"]) {
        assert!(crate::shell::icons::icon_svg_bytes(name).is_some(), "the icon {name} is in icons/material");
    }
    // The toolbar's 123 menu.
    let menu = s.tool_button("number-format").and_then(|b| b.menu).expect("the 123 menu");
    let formats: Vec<_> = menu.iter().filter_map(|m| if let MenuEntry::Item(i) = m { Some(i) } else { None }).collect();
    assert_eq!(formats.len(), crate::toolbar::NUMBER_FORMATS.len() + 1);
    assert!(formats.iter().all(|i| i.icon.is_some()), "every item of the toolbar's menu has an icon");
    // Choosing keeps the icon of the choice on the closed button and the list.
    choose(&mut s, "cell-format-number", &i18n::text("docx.field.date"));
    s.settle();
    assert_eq!(style(&mut s).number_format, "yyyy/m/d");
    s.screenshot("round2-02-number-icons");
    buttons::click(&mut s, "cell-format-number");
    s.settle();
    s.screenshot("round2-03-number-list-open");
}

/// None is the first item of the pattern list, and choosing it takes the pattern off.
pub(crate) fn pattern_none_first() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    tab(&mut s, 1);
    let list = items(&mut s, "cell-format-pattern");
    assert_eq!(list[0].label.to_string(), i18n::text("_shell.none").to_string(), "None first");
    assert_eq!(list.len(), crate::format_more::PATTERNS.len());
    choose(&mut s, "cell-format-pattern", &list[3].label.to_string());
    assert!(style(&mut s).pattern.is_some());
    choose(&mut s, "cell-format-pattern", &list[0].label.to_string());
    assert!(style(&mut s).pattern.is_none(), "None takes the pattern off");
    s.screenshot("round2-04-fill-tab");
}

/// A field with ▲▼ has no ✕ and takes all its text when it gets the focus (programmatically and by a click); typing then replaces it.
pub(crate) fn spinner_fields_select_all_on_focus() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    // Alignment: the indent is a field with ▲▼ (in the Format tab, under the text direction).
    s.update(|e, _, cx| e.panel_change(StyleChange::Indent(12), cx));
    s.settle();
    assert_eq!(text(&mut s, "indent"), "12");
    focus(&mut s, "code");
    assert_eq!(selected_text(&mut s, "indent"), "", "nothing selected before");
    focus(&mut s, "indent");
    assert_eq!(selected_text(&mut s, "indent"), "12", "all selected at the focus");
    s.type_text("5");
    assert_eq!(style(&mut s).indent, 5, "typing replaces the selected text");
    // A click into the field (the field is left of its ▲▼): the focus comes with a click, so the caret goes where it was pressed.
    focus(&mut s, "code");
    // The row's control column starts where the pane's drop-downs do: a point 30 px into it is inside the field.
    let (row, column) = (ui(&mut s, "cell-format-indent"), ui(&mut s, "cell-format-halign"));
    s.click((f32::from(column.origin.x) + 30., f32::from(row.origin.y + row.size.height / 2.)));
    s.settle();
    assert_eq!(selected_text(&mut s, "indent"), "", "a click that gives the focus puts the caret where it was pressed (no select-all)");
    // The size: the same.
    focus(&mut s, "size");
    assert_eq!(selected_text(&mut s, "size"), text(&mut s, "size"));
    assert!(!selected_text(&mut s, "size").is_empty());
    // A field without ▲▼ keeps its way (no select-all): the code field.
    focus(&mut s, "code");
    assert_eq!(selected_text(&mut s, "code"), "", "a field without ▲▼ selects nothing by itself");
    s.screenshot("round2-05-spinner-selected");
}

/// Live Update: ON by default and kept in the settings file; OFF shows Apply (primary, in the action bar area) and keeps the
/// changes of the pane as a draft until it is pressed (one Undo step); a draft of other cells is dropped.
pub(crate) fn live_update_on_off_and_apply() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    assert!(s.with(|e, _| e.sheet_state().unwrap().custom_format.as_ref().unwrap().live), "ON by default");
    assert!(buttons::bounds("cell-format-apply").is_none(), "no Apply while it is live");
    // ON: a change is written at once.
    s.update(|e, _, cx| e.panel_change(StyleChange::Bold(true), cx));
    assert!(style(&mut s).bold, "written at once");
    s.update(|e, w, cx| e.undo(&crate::Undo, w, cx));
    s.settle();
    assert!(!style(&mut s).bold);
    // Turn it off with the checkbox.
    click_ui(&mut s, "cell-format-live");
    assert!(!s.with(|e, _| e.sheet_state().unwrap().custom_format.as_ref().unwrap().live));
    assert_eq!(crate::shell::settings::get(LIVE_KEY).as_deref(), Some("0"), "kept in the settings file");
    assert!(buttons::bounds("cell-format-apply").is_some(), "Apply is shown");
    s.screenshot("round2-06-live-off");
    let before = (undo_len(&mut s), style(&mut s));
    // The changes of the pane wait: a drop-down, a toggle, a typed size and the code field.
    choose(&mut s, "cell-format-number", &i18n::text("docx.field.date"));
    s.update(|e, _, cx| e.panel_change(StyleChange::Bold(true), cx));
    focus(&mut s, "size");
    s.type_text("20");
    s.settle();
    assert_eq!(undo_len(&mut s), before.0, "nothing written");
    assert_eq!(style(&mut s), before.1, "the cells are as they were");
    let shown = panel_style(&mut s);
    assert_eq!(shown.number_format, "yyyy/m/d");
    assert!(shown.bold);
    assert_eq!(shown.font_size, 2000);
    assert_eq!(text(&mut s, "code"), "yyyy/m/d", "the pane shows the draft");
    assert_eq!(mismatches(&mut s), Vec::<String>::new(), "the fields show the pane's style");
    s.screenshot("round2-07-live-off-draft");
    // Apply: all at once, one Undo step.
    buttons::click(&mut s, "cell-format-apply");
    s.settle();
    let after = style(&mut s);
    assert_eq!((after.number_format.as_str(), after.bold, after.font_size), ("yyyy/m/d", true, 2000));
    assert_eq!(undo_len(&mut s), before.0 + 1, "one Undo step");
    s.update(|e, w, cx| e.undo(&crate::Undo, w, cx));
    s.settle();
    assert_eq!(style(&mut s), before.1, "one Undo takes it all back");
    assert_eq!(mismatches(&mut s), Vec::<String>::new());
    // A draft for other cells is dropped when the selection moves.
    s.update(|e, _, cx| e.panel_change(StyleChange::Italic(true), cx));
    assert!(panel_style(&mut s).italic);
    s.click_cell("C6");
    s.settle();
    assert!(!panel_style(&mut s).italic, "the draft is gone with the selection");
    // Restored: a pane opened again (the settings file says OFF) starts OFF.
    s.update(|e, w, cx| e.open_custom_format(w, cx));
    s.settle();
    assert!(!s.with(|e, _| e.sheet_state().unwrap().custom_format.as_ref().unwrap().live), "OFF is restored");
    assert!(buttons::bounds("cell-format-apply").is_some());
    // ON again: what waits is written; the setting is ON.
    s.update(|e, _, cx| e.panel_change(StyleChange::Strike(true), cx));
    assert!(!style(&mut s).strike);
    click_ui(&mut s, "cell-format-live");
    assert!(style(&mut s).strike, "turning it on writes the draft");
    assert_eq!(crate::shell::settings::get(LIVE_KEY).as_deref(), Some("1"));
    assert!(buttons::bounds("cell-format-apply").is_none());
    crate::shell::settings::set(LIVE_KEY, "1");
}

/// The gradient slider: chosen from the list it appears under it; pressing the bar adds a stop, dragging a knob moves it (one Undo step
/// for the drag), the colour and position fields and the delete button change the selected stop; Undo and Redo follow.
pub(crate) fn gradient_slider_stops_and_undo() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    tab(&mut s, 1);
    assert!(!has_ui(&mut s, "cell-format-gradient-slider"), "no slider without a gradient");
    let list = items(&mut s, "cell-format-gradient");
    choose(&mut s, "cell-format-gradient", &list[1].label.to_string());
    s.settle();
    assert_eq!(stops(&mut s).len(), 2);
    assert!(has_ui(&mut s, "cell-format-gradient-slider"), "the slider is under the list");
    s.screenshot("round2-08-gradient-slider");
    let base = undo_len(&mut s);
    // Press the middle of the bar: a stop is added there.
    let bar = ui(&mut s, "cell-format-gradient-slider");
    let (left, width, y) = (f32::from(bar.origin.x) + 8., f32::from(bar.size.width) - 16., f32::from(bar.origin.y) + 10.);
    s.click((left + width * 0.5, y));
    s.settle();
    let added = stops(&mut s);
    assert_eq!(added.len(), 3, "a stop is added: {added:?}");
    assert!(added.iter().any(|(p, _)| (*p as i32 - 500).abs() <= 5), "at the middle: {added:?}");
    assert_eq!(undo_len(&mut s), base + 1);
    // Drag it to 75 %: one more Undo step, however many moves.
    s.drag((left + width * 0.5, y), (left + width * 0.75, y));
    s.settle();
    let moved = stops(&mut s);
    assert_eq!(moved.len(), 3);
    assert!(moved.iter().any(|(p, _)| (*p as i32 - 750).abs() <= 6), "moved to 75 %: {moved:?}");
    assert_eq!(undo_len(&mut s), base + 2, "the drag is one Undo step");
    let shown: f32 = text(&mut s, "grad-pos").parse().expect("a percent in the position field");
    assert!((shown * 10.0 - 750.0).abs() <= 6.0, "the position field follows the drag: {shown}");
    assert_eq!(mismatches(&mut s), Vec::<String>::new());
    s.screenshot("round2-09-gradient-three-stops");
    // The position field moves the selected stop; the colour field recolours it.
    focus(&mut s, "grad-pos");
    s.key("cmd-a");
    s.type_text("25");
    assert!(stops(&mut s).iter().any(|(p, _)| *p == 250), "typed position: {:?}", stops(&mut s));
    focus(&mut s, "grad-color");
    s.key("cmd-a");
    s.type_text("FF0000");
    assert!(stops(&mut s).iter().any(|(p, c)| *p == 250 && *c == Rgb(0xFF0000)), "typed colour: {:?}", stops(&mut s));
    assert_eq!(mismatches(&mut s), Vec::<String>::new());
    // Undo follows: back through the typing to the drag, the add and the two-stop gradient.
    for _ in 0..40 {
        if undo_len(&mut s) == base + 2 { break; }
        s.update(|e, w, cx| e.undo(&crate::Undo, w, cx));
        s.settle();
    }
    assert_eq!(undo_len(&mut s), base + 2);
    assert!(stops(&mut s).iter().any(|(p, _)| (*p as i32 - 750).abs() <= 6), "the drag's result");
    assert_eq!(mismatches(&mut s), Vec::<String>::new());
    s.update(|e, w, cx| e.undo(&crate::Undo, w, cx));
    s.settle();
    assert!(stops(&mut s).iter().any(|(p, _)| (*p as i32 - 500).abs() <= 5), "one Undo undoes the whole drag");
    s.update(|e, w, cx| e.undo(&crate::Undo, w, cx));
    s.settle();
    assert_eq!(stops(&mut s).len(), 2, "one more takes the added stop away");
    s.update(|e, w, cx| e.redo(&crate::Redo, w, cx));
    s.settle();
    assert_eq!(stops(&mut s).len(), 3, "Redo puts it back");
    assert_eq!(mismatches(&mut s), Vec::<String>::new());
    // Delete the selected stop (a gradient keeps two).
    let n = undo_len(&mut s);
    buttons::click(&mut s, "cell-format-gradient-remove-stop");
    s.settle();
    assert_eq!(stops(&mut s).len(), 2, "the stop is deleted");
    assert_eq!(undo_len(&mut s), n + 1);
    // Another direction keeps the stops.
    let kept = stops(&mut s);
    choose(&mut s, "cell-format-gradient", &list[2].label.to_string());
    assert_eq!(stops(&mut s), kept, "the direction changes, the stops stay");
}

/// Live Update OFF with the gradient: the slider edits the draft; Apply writes the gradient.
pub(crate) fn gradient_slider_with_draft() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    tab(&mut s, 1);
    s.update(|e, _, cx| e.panel_set_live(false, cx));
    s.settle();
    let list = items(&mut s, "cell-format-gradient");
    choose(&mut s, "cell-format-gradient", &list[1].label.to_string());
    s.settle();
    assert!(style(&mut s).gradient.is_none(), "the cells wait");
    assert!(panel_style(&mut s).gradient.is_some());
    let bar = ui(&mut s, "cell-format-gradient-slider");
    let (left, width, y) = (f32::from(bar.origin.x) + 8., f32::from(bar.size.width) - 16., f32::from(bar.origin.y) + 10.);
    s.click((left + width * 0.4, y));
    s.settle();
    assert!(style(&mut s).gradient.is_none());
    buttons::click(&mut s, "cell-format-apply");
    s.settle();
    assert_eq!(stops(&mut s).len(), 3, "Apply writes the three stops");
    s.update(|e, _, cx| e.panel_set_live(true, cx));
    crate::shell::settings::set(LIVE_KEY, "1");
}
