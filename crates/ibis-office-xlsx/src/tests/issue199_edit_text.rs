// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #199: editing a cell formatted as a date or time showed the serial number (46340, 0.5625). Excel shows the
//! date `2026/11/14`, the time `13:30:00`, both, and a percentage as `12%`; entering it again gives the same value.
use super::SheetSteps;
use crate::shell::scenario::Scenario;
use crate::ext::*;

fn value_of(s: &mut Scenario, row: u32, column: u32) -> Option<f64> {
    s.with(|e, _| {
        let sheet = ibis_doc_model::sheet_calc::sheet_of_slide(e.slide())?;
        match sheet.cell(row, column)?.value {
            ibis_doc_model::CellValue::Number(n) => Some(n),
            _ => None,
        }
    })
}

pub(crate) fn edit_text_of_date_time_percent() {
    let mut s = Scenario::new_document(ibis_formats::Kind::Workbook);
    s.size(1200., 720.);
    let cases = [("A1", "2026/11/14", "2026/11/14"), ("A2", "13:30:00", "13:30:00"), ("A3", "2026/11/14 13:30", "2026/11/14 13:30:00"), ("A4", "12%", "12%")];
    for (at, typed, _) in cases {
        s.click_cell(at);
        s.type_text(typed);
        s.key("enter");
    }
    for (row, (at, _, shown)) in cases.iter().enumerate() {
        let before = value_of(&mut s, row as u32, 0).expect("a number");
        s.click_cell(at);
        s.key("f2");
        let text = s.with(|e, _| e.cell_edit().map(|c| c.text.clone())).expect("editing");
        assert_eq!(&text, shown, "{at}: edit box");
        s.key("enter");
        assert_eq!(value_of(&mut s, row as u32, 0), Some(before), "{at}: same value after entering it again");
    }
}
