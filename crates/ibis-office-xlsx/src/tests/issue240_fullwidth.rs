// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #240: full-width digits typed into a cell are taken as the number they spell (half-width), as Excel does; a
//! full-width `＝` starts a formula; text that does not spell a number stays as typed.
use crate::shell::scenario::Scenario;
use crate::ext::*;
use super::SheetSteps;

fn cell(s: &mut Scenario, row: u32, column: u32) -> Option<ibis_doc_model::CellValue> {
    s.with(|e, _| ibis_doc_model::sheet_calc::sheet_of_slide(e.slide())?.cell(row, column).map(|c| c.value.clone()))
}

pub(crate) fn fullwidth_digits_become_numbers() {
    let mut s = Scenario::new_document(ibis_formats::Kind::Workbook);
    s.size(1200., 720.);
    let cases = ["\u{FF11}\u{FF12}\u{FF13}", "\u{FF0D}\u{FF11}\u{FF12}\u{FF0E}\u{FF15}", "\u{FF11}\u{FF0C}\u{FF12}\u{FF13}\u{FF14}", "\u{FF11}\u{FF12}\u{FF05}", "\u{FF12}\u{FF10}\u{FF12}\u{FF16}\u{FF0F}\u{FF19}\u{FF0F}\u{FF13}\u{FF10}", "\u{FF21}\u{FF22}\u{FF23}", "\u{FF11}\u{FF12}\u{FF13}\u{500B}", "\u{FF1D}\u{FF11}\u{FF0B}\u{FF12}"];
    for (row, typed) in cases.iter().enumerate() {
        s.click_cell(&format!("A{}", row + 1));
        s.type_text(typed);
        s.key("enter");
    }
    use ibis_doc_model::CellValue as V;
    assert_eq!(cell(&mut s, 0, 0), Some(V::Number(123.0)));
    assert_eq!(cell(&mut s, 1, 0), Some(V::Number(-12.5)));
    assert_eq!(cell(&mut s, 2, 0), Some(V::Number(1234.0)));
    assert_eq!(cell(&mut s, 3, 0), Some(V::Number(0.12)));
    assert_eq!(cell(&mut s, 4, 0), Some(V::Number(46295.0)));
    assert_eq!(cell(&mut s, 5, 0), Some(V::Text("\u{FF21}\u{FF22}\u{FF23}".into())));
    assert_eq!(cell(&mut s, 6, 0), Some(V::Text("\u{FF11}\u{FF12}\u{FF13}\u{500B}".into())));
    assert_eq!(cell(&mut s, 7, 0), Some(V::Number(3.0)));
}

/// GitHub issue #240 (Excel's table): a cell formatted as Text keeps full-width input as it was typed or pasted; `９／３０` typed into a cell
/// of `#,##0` is the fraction 0.3 (the date pasted).
pub(crate) fn fullwidth_in_text_and_number_cells() {
    use ibis_doc_model::CellValue as V;
    let mut s = Scenario::new_document(ibis_formats::Kind::Workbook);
    s.size(1200., 720.);
    s.click_cell("A1");
    s.update(|e, _, cx| e.format_cells(ibis_doc_model::StyleChange::NumberFormat("@".into()), cx));
    for (row, typed) in ["\u{FF11}\u{FF12}\u{FF13}", "\u{FF1D}\u{FF11}\u{FF0B}\u{FF12}", "\u{FF12}\u{FF10}\u{FF12}\u{FF16}\u{FF0F}\u{FF19}\u{FF0F}\u{FF13}\u{FF10}"].iter().enumerate() {
        s.update(|e, _, cx| e.format_cells(ibis_doc_model::StyleChange::NumberFormat("@".into()), cx));
        s.click_cell(&format!("A{}", row + 1));
        s.update(|e, _, cx| e.format_cells(ibis_doc_model::StyleChange::NumberFormat("@".into()), cx));
        s.type_text(typed);
        s.key("enter");
        assert_eq!(cell(&mut s, row as u32, 0), Some(V::Text(typed.to_string())), "{typed}");
    }
    // Pasted into a Text cell.
    s.click_cell("C1");
    s.update(|e, _, cx| e.format_cells(ibis_doc_model::StyleChange::NumberFormat("@".into()), cx));
    s.update(|_, _, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("\u{FF11}\u{FF12}\u{FF13}".to_string())));
    s.click_cell("C1");
    s.key("cmd-v");
    assert_eq!(cell(&mut s, 0, 2), Some(V::Text("\u{FF11}\u{FF12}\u{FF13}".into())));
    // `９／３０` in a cell of `#,##0`: typed, 0.3; pasted, the date; a General cell, the date.
    s.click_cell("D1");
    s.update(|e, _, cx| e.format_cells(ibis_doc_model::StyleChange::NumberFormat("#,##0".into()), cx));
    s.click_cell("D1");
    s.type_text("\u{FF19}\u{FF0F}\u{FF13}\u{FF10}");
    s.key("enter");
    assert!(matches!(cell(&mut s, 0, 3), Some(V::Number(n)) if (n - 0.3).abs() < 1e-12));
    s.click_cell("D2");
    s.update(|e, _, cx| e.format_cells(ibis_doc_model::StyleChange::NumberFormat("#,##0".into()), cx));
    s.update(|_, _, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("\u{FF19}\u{FF0F}\u{FF13}\u{FF10}".to_string())));
    s.click_cell("D2");
    s.key("cmd-v");
    let pasted = cell(&mut s, 1, 3);
    assert!(matches!(pasted, Some(V::Number(n)) if n > 40000.0), "{pasted:?}");
    s.click_cell("D3");
    s.type_text("\u{FF19}\u{FF0F}\u{FF13}\u{FF10}");
    s.key("enter");
    assert!(matches!(cell(&mut s, 2, 3), Some(V::Number(n)) if n > 40000.0));
}
