// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Real formula-bar navigation, wrapping and resizing regressions.
use super::Scenario;
use crate::xlsx::scenarios::SheetSteps;
use gpui_kit::*;
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::issue65_formula_editor", editor),
        ("xlsx::issue64_general_numbers", numbers),
    ]
}
fn workbook() -> Scenario {
    let mut s = Scenario::start_view();
    s.size(1000., 700.);
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    s
}
fn editor() {
    let mut s = workbook();
    s.click_cell("A1");
    s.key("f2");
    let text = format!("={}", vec!["100000000"; 55].join("+"));
    s.update(|_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone())));
    s.key("cmd-v");
    let (width, lines, right) = s.with(|e, _| {
        let edit = e.cell_edit().unwrap();
        let layout = edit.layout.borrow();
        let l = layout.as_ref().unwrap();
        (
            l.width,
            l.lines.len(),
            f32::from(e.workspace_bounds.unwrap().size.width),
        )
    });
    assert!(lines > 1);
    assert!(width <= right - 10.);
    s.screenshot("cell-wrapped-long-formula");
    let bar = s.with(|e, _| e.wb().bar_bounds.get().unwrap());
    s.click((f32::from(bar.left()) + 12., f32::from(bar.top()) + 8.));
    assert!(
        s.with(|e, _| e.cell_edit().is_some_and(|c| c.dragging_in_bar)),
        "bar click enters edit"
    );
    s.key("ctrl-home");
    let before = s.with(|e, _| e.current_cell());
    s.key("down");
    assert!(s.with(|e, _| e.cell_edit().unwrap().caret) > 0);
    assert_eq!(s.with(|e, _| e.current_cell()), before);
    s.key("home");
    let first = s.with(|e, _| e.cell_edit().unwrap().caret);
    s.key("end");
    assert!(s.with(|e, _| e.cell_edit().unwrap().caret) > first);
    s.key("ctrl-end");
    assert_eq!(s.with(|e, _| e.cell_edit().unwrap().caret), text.len());
    s.screenshot("bar-end-caret");
    s.key("ctrl-home");
    s.key("ctrl-shift-right");
    assert!(s.with(|e, _| e.cell_edit().unwrap().selection().is_some()));
    s.update(|e, _, cx| e.toggle_formula_bar_expanded(cx));
    let count = s.with(|e, _| e.wb().bar_visual_lines.len());
    assert!(count > 2);
    s.screenshot("expanded-bar-wrapped");
    let b = s.with(|e, _| e.wb().bar_bounds.get().unwrap());
    let at = (500., f32::from(b.bottom()) + 7.);
    s.drag(at, (at.0, at.1 + 50.));
    let after = s.with(|e, _| f32::from(e.wb().bar_bounds.get().unwrap().size.height));
    assert!(
        after > f32::from(b.size.height) + 30.,
        "splitter grows {after} vs {:?}",
        b.size.height
    );
    s.update(|e, _, cx| {
        e.toggle_formula_bar_expanded(cx);
        e.toggle_formula_bar_expanded(cx);
    });
    assert!(
        (s.with(|e, _| f32::from(e.wb().bar_bounds.get().unwrap().size.height)) - after).abs() < 1.
    );
    s.screenshot("resized-bar");
    let b = s.with(|e, _| e.wb().bar_bounds.get().unwrap());
    s.drag((500., f32::from(b.bottom()) + 7.), (500., 1500.));
    assert!(
        s.with(|e, _| f32::from(e.workspace_bounds.unwrap().size.height)) >= 99.,
        "table remains at least 100px"
    );
    s.screenshot("splitter-table-minimum");
    s.key("ctrl-end");
    s.key("ctrl-shift-left");
    let caret = s.with(|e, _| e.cell_edit().unwrap().caret);
    assert!(caret < text.len());
    s.key("cmd-shift-left");
    assert!(s.with(|e, _| e.cell_edit().unwrap().caret) < caret);
    s.key("enter");
    assert!(s.with(|e, _| e.cell_edit().is_none()));
    s.screenshot("expanded-readonly-wrapped");
    s.update(|e, w, cx| {
        e.new_file(ibis_pptx::Kind::Workbook, w, cx);
        e.toggle_formula_bar_expanded(cx);
    });
    assert!(
        s.with(|e, _| f32::from(e.wb().bar_bounds.get().unwrap().size.height)) < 90.,
        "new document starts with default bar height"
    );
}
fn numbers() {
    let mut s = workbook();
    for (cell, value) in [
        ("A1", "1234567890"),
        ("A2", "12345678901"),
        ("A3", "123456789012"),
        ("A4", "-12345678901"),
    ] {
        s.click_cell(cell);
        s.enter(value, "enter");
    }
    let texts = s.update(|e, w, _| {
        crate::xlsx::sheet_view::shown_texts(e.current_sheet().unwrap().1, &e.resources, w)
    });
    eprintln!("general shown={texts:?}");
    // The default-width column widens on entry (Excel: 10 digits -> 11.0,
    // 11 digits -> 12.0); 12 digits stop at 12.29 and fall back to E notation.
    assert!(
        texts.iter().any(|(r, _, t)| *r == 2 && t.contains('E')),
        "narrow-column fallback"
    );
    assert!(
        texts.iter().all(|(r, _, t)| *r >= 2 || !t.contains('E')),
        "10 and 11 digits show in full after the column widens"
    );
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run_sheet(
            vec![ibis_doc_model::Command::Sheet(
                ibis_doc_model::SheetCommand::SetSize {
                    slide,
                    axis: ibis_doc_model::sheet_command::Axis::Columns,
                    first: 0,
                    last: 0,
                    size: 160 * 12700,
                },
            )],
            cx,
        );
    });
    let texts = s.update(|e, w, _| {
        crate::xlsx::sheet_view::shown_texts(e.current_sheet().unwrap().1, &e.resources, w)
    });
    assert_eq!(
        texts.iter().find(|(r, _, _)| *r == 0).unwrap().2,
        "1234567890"
    );
    assert_eq!(
        texts.iter().find(|(r, _, _)| *r == 1).unwrap().2,
        "12345678901"
    );
    assert_eq!(
        texts.iter().find(|(r, _, _)| *r == 2).unwrap().2,
        "1.23457E+11"
    );
    assert_eq!(
        s.value("A2"),
        ibis_doc_model::CellValue::Number(12345678901.)
    );
    assert_eq!(
        texts.iter().find(|(r, _, _)| *r == 3).unwrap().2,
        "-12345678901"
    );
    s.screenshot("general-number-digits");
}
