// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue #134: 形式を選択して貼り付け (the dialog) pastes the formats of copied cells, from cells copied here and from cells
//! another program put on the clipboard (Excel's worksheet or HTML table, read as an ordinary paste reads them).

use super::SheetSteps;
use crate::shell::scenario::Scenario;
use crate::clipboard::PasteType;
use ibis_doc_model::sheet_command::StyleChange;
use ibis_doc_model::{CellRange, CellValue, Command, Rgb, SheetCommand, sheet_calc::sheet_of_slide};
use crate::ext::*;

fn style_of(s: &mut Scenario, row: u32, column: u32) -> (bool, Option<Rgb>, String) {
    s.with(move |e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        match sheet.cell(row, column) {
            Some(c) => {
                let st = &sheet.styles[c.style as usize];
                (st.bold, st.fill, st.number_format.clone())
            }
            None => (false, None, "General".into()),
        }
    })
}

fn format_cells(s: &mut Scenario, range: CellRange) {
    s.update(move |e, _, _| {
        let slide = e.current_slide;
        let commands: Vec<Command> = [StyleChange::Bold(true), StyleChange::Fill(Some(Rgb(0x336699))), StyleChange::NumberFormat("0.00".into())]
            .into_iter()
            .map(|change| Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change }))
            .collect();
        e.document.execute(commands).unwrap();
    });
}

/// Through the dialog (type chosen, OK): the formats of a formatted cell and of a formatted blank cell come, the target's
/// contents stay; the dialog opens and applies the same for cells copied here.
fn through_dialog(s: &mut Scenario, kind: PasteType) {
    s.key("cmd-alt-v");
    assert!(s.with(|e, _| e.wb().panels.paste_special_dialog.is_some()), "the dialog opened");
    s.update(move |e, _, cx| {
        if let Some(d) = e.sheet_state_mut().and_then(|s| s.panels.paste_special_dialog.as_mut()) {
            d.paste_type = kind;
        }
        e.apply_paste_special_dialog(cx);
    });
}

pub(super) fn formats_from_cells_copied_here() {
    let mut s = super::new_sheet();
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
    s.enter("5", "enter");
    format_cells(&mut s, CellRange { first_row: 0, first_column: 0, last_row: 1, last_column: 0 });
    s.click_cell("D1");
    s.enter("keep", "enter");
    s.click_cell("A1");
    s.update(|e, _, _| e.set_selected_ranges(&[CellRange { first_row: 0, first_column: 0, last_row: 1, last_column: 0 }]));
    s.update(|e, _, cx| e.copy_cells(false, cx));
    s.click_cell("D1");
    through_dialog(&mut s, PasteType::Formats);
    assert_eq!(s.value("D1"), CellValue::Text("keep".into()), "the contents stay");
    assert_eq!(style_of(&mut s, 0, 3), (true, Some(Rgb(0x336699)), "0.00".into()));
    // A1 was a number, A2 is blank but formatted: D2 takes the format too.
    assert_eq!(style_of(&mut s, 1, 3), (true, Some(Rgb(0x336699)), "0.00".into()));
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
}

/// The system clipboard holds cells that this window did not copy (what Excel put there): the dialog opens and pastes them.
pub(super) fn formats_from_cells_of_another_program() {
    let mut s = super::new_sheet();
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
    s.enter("5", "enter");
    format_cells(&mut s, CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 });
    s.click_cell("A1");
    s.update(|e, _, cx| e.copy_cells(false, cx));
    // Nothing is left of the copy here: only the clipboard holds the cells.
    s.update(|e, _, _| {
        if let Some(state) = e.sheet_state_mut() {
            state.clip = None;
        }
    });
    s.click_cell("D1");
    s.enter("keep", "enter");
    s.click_cell("D1");
    through_dialog(&mut s, PasteType::Formats);
    assert_eq!(s.value("D1"), CellValue::Text("keep".into()));
    assert_eq!(style_of(&mut s, 0, 3), (true, Some(Rgb(0x336699)), "0.00".into()));
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
}
