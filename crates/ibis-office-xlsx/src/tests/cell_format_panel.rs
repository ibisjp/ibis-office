// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! セルの書式設定 (the sidebar pane): the user-defined number format that follows the drop-down and applies as it is typed,
//! 文字の方向 Custom, the font drop-down that applies at once, and the pattern and gradient drop-downs whose items are pictures.

use super::SheetSteps;
use crate::shell::chrome::MenuEntry;
use crate::shell::i18n;
use crate::shell::scenario::{Scenario, button_probe as buttons};
use crate::ext::*;

fn open() -> Scenario {
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
fn labels(s: &mut Scenario, id: &str) -> Vec<String> {
    let id = id.to_string();
    s.with(move |e, _| {
        let menus = e.sheet_state().unwrap().custom_format.as_ref().unwrap().menus.borrow();
        menus.get(&id).unwrap_or_else(|| panic!("no drop-down {id}")).iter().filter_map(|m| if let MenuEntry::Item(i) = m { Some(i.label.to_string()) } else { None }).collect()
    })
}
fn choose(s: &mut Scenario, id: &str, label: &str) {
    let (id, label) = (id.to_string(), label.to_string());
    let wanted = label.clone();
    let handler = s.with(move |e, _| {
        let menus = e.sheet_state().unwrap().custom_format.as_ref().unwrap().menus.borrow();
        menus.get(&id).unwrap_or_else(|| panic!("no drop-down {id}")).iter().find_map(|m| match m {
            MenuEntry::Item(i) if i.label.as_ref() == label => i.handler.clone(),
            _ => None,
        })
    });
    let handler = handler.unwrap_or_else(|| panic!("no item {wanted}"));
    s.update(move |e, window, cx| handler(e, window, cx));
}
fn code_text(s: &mut Scenario) -> String {
    s.with(|e, cx| e.sheet_state().unwrap().custom_format.as_ref().unwrap().code.read(cx).value().to_string())
}
fn focus(s: &mut Scenario, which: &'static str) {
    let input = s.with(move |e, _| {
        let p = e.sheet_state().unwrap().custom_format.as_ref().unwrap();
        match which { "code" => p.code.clone(), _ => p.rotation.clone() }
    });
    s.update(move |_, window, cx| input.update(cx, |i, cx| i.focus(window, cx)));
}

/// The drop-down's choice shows its code in the user-defined field; after Custom the code is there to edit; typing applies
/// a complete code to the cell at once, and a code that is not complete (open quote, nothing) is not applied; Undo goes back by the typed steps.
pub(crate) fn number_code_follows_dropdown_and_applies_live() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    choose(&mut s, "cell-format-number", &i18n::text("docx.field.date"));
    assert_eq!(style(&mut s).number_format, "yyyy/m/d");
    assert_eq!(code_text(&mut s), "yyyy/m/d", "the drop-down's format shows in the field");
    s.screenshot("number-date-code");
    choose(&mut s, "cell-format-number", &i18n::text("xlsx.format-pane.custom"));
    assert_eq!(code_text(&mut s), "yyyy/m/d", "after Custom the code is there to edit");
    assert!(s.with(|e, _| e.sheet_state().unwrap().custom_format.as_ref().unwrap().custom_selected));
    focus(&mut s, "code");
    s.key("cmd-a");
    s.type_text("yyyy");
    assert_eq!(style(&mut s).number_format, "yyyy", "applied as it is typed");
    s.screenshot("number-custom-typed");
    // Not complete: an open quote, then nothing. The cells keep the last complete code.
    s.key("cmd-a");
    s.type_text("\"ab");
    assert_eq!(style(&mut s).number_format, "yyyy", "an open quote is not applied");
    s.key("cmd-a");
    s.key("backspace");
    assert_eq!(style(&mut s).number_format, "yyyy", "an empty code is not applied");
    s.type_text("0.00");
    assert_eq!(style(&mut s).number_format, "0.00");
    // 5 or more sections is not a format code either.
    s.key("cmd-a");
    s.type_text("0;0;0;0;0");
    assert_eq!(style(&mut s).number_format, "0;0;0;0", "the fifth section makes it incomplete; the code before it stays");
    s.click_cell("B4");
    let before = style(&mut s).number_format;
    s.key("cmd-z");
    assert_ne!(style(&mut s).number_format, before, "Undo steps back");
    for _ in 0..40 {
        if style(&mut s).number_format == "yyyy/m/d" { break; }
        s.key("cmd-z");
    }
    assert_eq!(style(&mut s).number_format, "yyyy/m/d", "Undo reaches the date format before the typing");
}

/// 文字の方向: Custom comes after 180°; the angle field is off until it is chosen.
pub(crate) fn rotation_custom_and_font_pick() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    let items = labels(&mut s, "cell-format-rotation");
    let at = items.iter().position(|l| l == "180°").expect("180°");
    assert_eq!(items[at + 1], i18n::text("xlsx.format-pane.rotation-numeric").to_string(), "Numeric follows 180°");
    // The angle field is off: typing in it changes nothing.
    focus(&mut s, "rot");
    s.type_text("30");
    assert_eq!(style(&mut s).rotation, 0, "the field is off until Custom is chosen");
    choose(&mut s, "cell-format-rotation", &i18n::text("xlsx.format-pane.rotation-numeric"));
    s.settle();
    focus(&mut s, "rot");
    s.key("cmd-a");
    s.type_text("30");
    assert_eq!(style(&mut s).rotation, 30, "the angle typed applies once Custom is chosen");
    s.screenshot("rotation-custom");
    // A listed angle turns Custom off again.
    choose(&mut s, "cell-format-rotation", "45°");
    assert_eq!(style(&mut s).rotation, 45);
    assert!(!s.with(|e, _| e.sheet_state().unwrap().custom_format.as_ref().unwrap().rotation_custom));
    // The font drop-down applies the font chosen at once (there is no 設定 button).
    let was = style(&mut s).font.clone();
    buttons::click(&mut s, "panel-font-xlsx-custom-format-1");
    let index = s.with(|e, _| {
        e.toolbar_search_popup_items.as_ref().and_then(|p| p.1.iter().position(|i| !i.id.starts_with("font.section.") && i.enabled() && Some(i.label.to_string()) != was))
    }).expect("a font in the list");
    let row = buttons::all().into_iter().find(|k| k.contains("searchable-item-panel-font-xlsx-custom-format-1") && k.ends_with(&format!(", {index})"))).expect("its row");
    buttons::click(&mut s, &row);
    assert_ne!(style(&mut s).font, was, "the font chosen is applied");
    assert!(buttons::all().iter().all(|k| !k.contains("cell-format-font-set")), "no set-font button");
}

/// The pattern and the gradient drop-downs list pictures; choosing one applies it; the name is a tooltip at the left of the item.
pub(crate) fn pattern_and_gradient_lists_are_pictures() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    s.update(|e, _, cx| { e.wb_mut().format_tab = 1; cx.notify(); });
    s.settle();
    assert_eq!(labels(&mut s, "cell-format-pattern").len(), crate::format_more::PATTERNS.len());
    assert_eq!(labels(&mut s, "cell-format-gradient").len(), 6);
    buttons::click(&mut s, "cell-format-pattern");
    let row = |_s: &mut Scenario, id: &str, i: usize| buttons::all().into_iter().find(|k| k.contains(&format!("searchable-item-{id}")) && k.ends_with(&format!(", {i})"))).unwrap_or_else(|| panic!("row {i} of {id}"));
    let r = row(&mut s, "cell-format-pattern", 5);
    let b = buttons::bounds(&r).unwrap();
    s.hover((f32::from(b.center().x), f32::from(b.center().y)));
    s.wait(1000);
    s.screenshot("pattern-list-tooltip");
    buttons::click(&mut s, &r);
    assert_eq!(style(&mut s).pattern.as_ref().map(|p| p.0.as_str()), Some(crate::format_more::PATTERNS[5].1));
    s.screenshot("pattern-chosen");
    buttons::click(&mut s, "cell-format-gradient");
    let r = row(&mut s, "cell-format-gradient", 2);
    let b = buttons::bounds(&r).unwrap();
    s.hover((f32::from(b.center().x), f32::from(b.center().y)));
    s.wait(1000);
    s.screenshot("gradient-list-tooltip");
    buttons::click(&mut s, &r);
    assert!(matches!(style(&mut s).gradient, Some(ibis_doc_model::CellGradient::Linear { degree: 90, .. })));
    s.screenshot("gradient-chosen");
}
