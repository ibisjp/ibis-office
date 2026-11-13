// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The input fields of セルの書式設定 follow the document (shell/chrome/synced_field.rs): after Undo, Redo, another cell, a range,
//! another sheet and an external reload every field shows the document's value; typing then Undo drops the draft; a field that
//! is not declared with `SyncedFields::add` is refused. The invariant "every field shows the document's value" is
//! `SyncedFields::mismatches`, checked after every step by `in_step`.

use super::SheetSteps;
use crate::shell::chrome::synced_field::SyncedFields;
use crate::shell::scenario::{Scenario, button_probe as buttons};
use crate::{Redo, Undo};
use gpui_kit::AppContext as _;
use ibis_doc_model::StyleChange;
use crate::ext::*;

fn open() -> Scenario {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 1100.);
    s.click_cell("B4");
    s.key("cmd-1");
    s.settle();
    s
}
fn fields(s: &mut Scenario) -> SyncedFields {
    s.with(|e, _| e.sheet_state().unwrap().custom_format.as_ref().expect("the cell-format pane is open").fields.clone())
}
fn text(s: &mut Scenario, id: &str) -> String {
    let (f, id) = (fields(s), id.to_string());
    s.with(move |_, cx| f.input(&id).unwrap_or_else(|| panic!("no field {id}")).read(cx).value().to_string())
}
/// The invariant: every field of the pane shows the document's value. Also: every input the pane paints is declared.
fn in_step(s: &mut Scenario, when: &str) {
    s.settle();
    let f = fields(s);
    let bad = s.with(move |e, cx| f.mismatches(e, cx));
    assert!(bad.is_empty(), "{when}: {bad:?}");
}
fn apply(s: &mut Scenario, change: StyleChange) {
    s.update(move |e, _, cx| e.format_cells(change, cx));
    in_step(s, "after a change");
}
fn undo(s: &mut Scenario) {
    s.update(|e, w, cx| e.undo(&Undo, w, cx));
    in_step(s, "after Undo");
}
fn redo(s: &mut Scenario) {
    s.update(|e, w, cx| e.redo(&Redo, w, cx));
    in_step(s, "after Redo");
}
fn focus(s: &mut Scenario, id: &str) {
    let (f, id) = (fields(s), id.to_string());
    let input = s.with(move |_, _| f.input(&id).expect("field"));
    s.update(move |_, window, cx| input.update(cx, |i, cx| i.focus(window, cx)));
    s.settle();
}
fn style(s: &mut Scenario) -> ibis_doc_model::CellStyle {
    s.with(|e, _| e.current_style())
}

/// Every field, changed in the document by something other than the field (as the toolbar and the menus do), is followed through Undo and Redo.
pub(crate) fn fields_follow_undo_and_redo() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    in_step(&mut s, "opened");
    let changes: Vec<(&str, StyleChange, &str)> = vec![
        ("code", StyleChange::NumberFormat("0.000".into()), "0.000"),
        ("font", StyleChange::Font(Some("Arial".into())), "Arial"),
        ("size", StyleChange::FontSize(2050), "20.5"),
        ("rotation", StyleChange::Rotation(135), "-45"),
        ("indent", StyleChange::Indent(3), "3"),
        ("font-color", StyleChange::Color(ibis_doc_model::Rgb(0x112233)), "112233"),
        ("fill-color", StyleChange::Fill(Some(ibis_doc_model::Rgb(0xAABBCC))), "AABBCC"),
    ];
    let before: Vec<String> = changes.iter().map(|(id, _, _)| text(&mut s, id)).collect();
    for (id, change, shown) in &changes {
        apply(&mut s, change.clone());
        assert_eq!(&text(&mut s, id), shown, "{id} shows the change");
    }
    for (i, (id, _, _)) in changes.iter().enumerate().rev() {
        undo(&mut s);
        assert_eq!(text(&mut s, id), before[i], "{id} after Undo");
    }
    for (id, _, shown) in &changes {
        redo(&mut s);
        assert_eq!(&text(&mut s, id), shown, "{id} after Redo");
    }
    // The pattern colour (a pattern gives it a value) and the border colour (the workbook's, which the toolbar sets).
    apply(&mut s, StyleChange::Pattern(Some(("gray125".into(), ibis_doc_model::Rgb(0x445566), ibis_doc_model::Rgb(0xFFFFFF)))));
    assert_eq!(text(&mut s, "pattern-color"), "445566");
    undo(&mut s);
    assert_eq!(text(&mut s, "pattern-color"), "");
    s.update(|e, _, cx| { e.wb_mut().border_color = ibis_doc_model::Rgb(0x778899); cx.notify(); });
    in_step(&mut s, "border colour set from outside");
    assert_eq!(text(&mut s, "border-color"), "778899");
    s.screenshot("after-undo-redo");
}

/// Another cell, a range, another sheet: the fields show the active cell of the selection.
pub(crate) fn fields_follow_selection_range_and_sheet() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    apply(&mut s, StyleChange::FontSize(2000));
    apply(&mut s, StyleChange::Fill(Some(ibis_doc_model::Rgb(0x00FF00))));
    assert_eq!(text(&mut s, "size"), "20");
    s.click_cell("D9");
    in_step(&mut s, "another cell");
    let d9 = style(&mut s);
    assert_eq!(text(&mut s, "size"), format!("{}", d9.font_size as f32 / 100.0), "D9 has its own size");
    assert_eq!(text(&mut s, "fill-color"), d9.fill.map(|c| format!("{:06X}", c.0)).unwrap_or_default());
    s.click_cell("B4");
    in_step(&mut s, "back to B4");
    assert_eq!(text(&mut s, "size"), "20");
    assert_eq!(text(&mut s, "fill-color"), "00FF00");
    // A range from B4: the active cell is B4, the change goes to all of it.
    let to = s.with(|e, _| e.cell_center(3, 6).expect("D6 is shown"));
    s.shift_click((f32::from(to.x), f32::from(to.y)));
    in_step(&mut s, "range selected");
    assert_eq!(text(&mut s, "size"), "20");
    apply(&mut s, StyleChange::FontSize(1400));
    assert_eq!(text(&mut s, "size"), "14");
    undo(&mut s);
    assert_eq!(text(&mut s, "size"), "20", "Undo of a change of the range");
    // Another sheet (same cell address): its own value.
    let other = s.with(|e, _| e.presentation().slides.iter().map(|x| x.id).nth(1)).expect("a second sheet");
    s.update(move |e, _, cx| { e.current_slide = other; cx.notify(); });
    in_step(&mut s, "another sheet");
    assert_ne!(text(&mut s, "fill-color"), "00FF00", "the other sheet's cell has no such fill");
}

/// An external update (the file read again) replaces the document; the fields show what was read.
pub(crate) fn fields_follow_external_reload() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    apply(&mut s, StyleChange::FontSize(1700));
    s.save_as("saved.xlsx");
    apply(&mut s, StyleChange::FontSize(900));
    assert_eq!(text(&mut s, "size"), "9");
    s.update(|e, w, cx| e.reload_external_update(w, cx));
    s.settle();
    s.click_cell("B4");
    s.settle();
    if s.with(|e, _| e.sheet_state().is_some_and(|w| w.custom_format.is_none())) {
        s.key("cmd-1");
        s.settle();
    }
    in_step(&mut s, "after the reload");
    assert_eq!(text(&mut s, "size"), "17", "the reloaded file's size");
}

/// While the user types, the document's change (Undo) wins over the draft; a draft that is not a value yet stays while the
/// document does not change; a field left with a draft goes back to the document's value.
pub(crate) fn typing_and_undo() {
    let _probe = buttons::Probe::new();
    let mut s = open();
    focus(&mut s, "size");
    s.key("cmd-a");
    s.type_text("18");
    assert_eq!(style(&mut s).font_size, 1800, "applied as it is typed");
    in_step(&mut s, "typed");
    undo(&mut s);
    assert_ne!(style(&mut s).font_size, 1800);
    assert_eq!(text(&mut s, "size"), (style(&mut s).font_size as f32 / 100.0).to_string(), "the document wins over the typed text");
    redo(&mut s);
    assert_eq!(text(&mut s, "size"), "18");
    // An unfinished draft of a colour stays while the field has the focus and the document does not change.
    focus(&mut s, "font-color");
    s.key("cmd-a");
    s.type_text("FF0");
    s.settle();
    assert_eq!(text(&mut s, "font-color"), "FF0", "the draft is kept");
    assert_ne!(style(&mut s).color.0, 0xFF0, "the draft is not applied");
    // Undo while the draft is in the field: the document's value replaces it.
    undo(&mut s);
    assert_eq!(text(&mut s, "font-color"), format!("{:06X}", style(&mut s).color.0), "the draft is dropped");
    assert_ne!(text(&mut s, "size"), "18");
    // A draft in a field that has lost the focus goes back to the document's value.
    focus(&mut s, "font-color");
    s.key("cmd-a");
    s.type_text("FF0");
    focus(&mut s, "indent");
    in_step(&mut s, "the draft field lost the focus");
    assert_eq!(text(&mut s, "font-color"), format!("{:06X}", style(&mut s).color.0));
    s.screenshot("typing-and-undo");
}

/// An input the pane paints that is not declared with `SyncedFields::add` is refused (in the strict scope the shared widgets
/// panic), and every input the pane paints is declared.
pub(crate) fn undeclared_input_is_refused() {
    let _probe = buttons::Probe::new();
    buttons::input_scope(true);
    let mut s = open();
    s.settle();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let f = fields(&mut s);
    let (painted, undeclared) = s.with(move |_, cx| {
        let painted = buttons::inputs(cx);
        let undeclared = painted.iter().filter(|i| !f.contains(i)).count();
        (painted.len(), undeclared)
    });
    assert!(painted >= 3, "the pane paints its fields ({painted})");
    assert_eq!(undeclared, 0, "every painted input is declared");
    // A stray input in the strict scope panics.
    let f = fields(&mut s);
    let refused = s.update(move |_, window, cx| {
        let stray = cx.new(|cx| gpui_kit::component::input::InputState::new(window, cx));
        let _scope = f.strict_scope();
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| { let _ = crate::shell::chrome::text_field(&stray); })).is_err()
    });
    assert!(refused, "an undeclared input is refused");
    buttons::input_scope(false);
}
