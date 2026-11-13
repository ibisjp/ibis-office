// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Copying and pasting cells as the HTML table Excel puts on the clipboard (`public.html`, "HTML Format"): the table is read
//! with its formats, formulas and merged cells; copied cells are written as such a table and read back the same.
//!
//! Excel's own table is the one `test/github-issue/83/0-public.html.bin` kept (what Excel for Mac put there for two cells of
//! 1,000 and 2,000 with a fill and borders). Excel itself was not asked to paste anything here (not tried).

use super::SheetSteps;
use crate::shell::scenario::{Scenario, repo_root};
use crate::native_shape_clipboard::html_of_bytes;
use crate::clipboard_html::read;
use ibis_doc_model::sheet_command::StyleChange;
use ibis_doc_model::{Command, SheetCommand, BorderLine, CellAlign, CellInput, CellRange, CellValue, Rgb, sheet_calc::sheet_of_slide};
use crate::ext::*;

const SRC: &str = "test/editing/issue100-names-src.xlsx";

fn number(cell: &crate::clipboard_html::HtmlCell) -> Option<f64> {
    match &cell.input {
        CellInput::Value(CellValue::Number(n)) => Some(*n),
        _ => None,
    }
}

/// The table Excel for Mac copied: a fill, borders, a number format, `align`, a text cell and numbers typed as text of the display.
pub(super) fn excel_mac_table() {
    let path = repo_root().join("test/github-issue/83/0-public.html.bin");
    let Ok(bytes) = std::fs::read(&path) else {
        eprintln!("no {}: skipped", path.display());
        return;
    };
    let html = html_of_bytes(&bytes).unwrap();
    let cells = read(&html, false).expect("a table");
    assert_eq!(cells.len(), 4);
    let (a, b, c, d) = (&cells[0], &cells[1], &cells[2], &cells[3]);
    assert_eq!((a.row, a.column, a.shown.as_str()), (0, 0, "A"));
    assert_eq!((b.row, b.column, b.shown.as_str()), (0, 1, "B"));
    assert_eq!((c.row, c.column), (1, 0));
    assert_eq!((d.row, d.column), (1, 1));
    assert_eq!(a.style.fill, Some(Rgb(0xE97132)));
    // `align=left` of a text is the text's own side, not a choice.
    assert_eq!(a.style.align, CellAlign::General);
    // The first cell has a border on all sides; the second none on its left (`border-left:none`).
    assert_eq!(a.style.top.map(|b| b.line), Some(BorderLine::Thin));
    assert_eq!(a.style.left.map(|b| b.line), Some(BorderLine::Thin));
    assert!(b.style.left.is_none() && b.style.right.is_some());
    assert!(c.style.top.is_none() && c.style.bottom.is_some());
    // The number format of the class, with its escapes taken out; the display "1,000" is the number.
    assert_eq!(c.style.number_format, "#,##0;[Red]\\-#,##0");
    assert_eq!(number(c), Some(1000.0));
    assert_eq!(number(d), Some(2000.0));
    // What the `td` rule says of every cell.
    assert_eq!(a.style.font_size, 1200);
    assert_eq!(a.style.font.as_deref(), Some("游ゴシック")); // i18n-ok（試験の中の日本語のデータ）
    // `align=right` of a number is the number's own side, not a choice.
    assert_eq!(c.style.align, CellAlign::General);
}

/// Excel's attributes and a table that the `<style>` block formats: formulas, booleans, errors, text that looks like a number,
/// colspan and rowspan, line breaks, entities.
pub(super) fn attributes_and_spans() {
    let html = r##"<html xmlns:x="urn:schemas-microsoft-com:office:excel"><head><style>
<!--table {mso-displayed-decimal-separator:"\.";}
td {font-size:10.0pt; font-family:Meiryo; vertical-align:middle;}
.b {font-weight:700; font-style:italic; color:#FF0000; background:#FFFF00; text-decoration:underline;}
.p {mso-number-format:"0\.0%";}
-->
</style></head><body><table>
<!--StartFragment-->
<tr><td class=b x:num="3" x:fmla="=SUM(A1,B1)&gt;1">3</td><td colspan=2 rowspan=2 style='text-align:center'>M &amp; M<br>2nd</td><td x:bool="TRUE">TRUE</td></tr>
<tr><td class=p x:num="0.5">50.0%</td><td x:err="#DIV/0!">#DIV/0!</td></tr>
<tr><td x:str="'007">007</td><td>12</td><td></td><td>plain</td></tr>
<!--EndFragment-->
</table></body></html>"##;
    let cells = read(html, false).expect("a table");
    let at = |r: u32, c: u32| cells.iter().find(|x| x.row == r && x.column == c).unwrap_or_else(|| panic!("no cell at {r},{c}"));
    // The formula keeps its `>` (the attribute's entity), without the `=`.
    assert!(matches!(&at(0, 0).input, CellInput::Formula(f) if f == "SUM(A1,B1)>1"), "{:?}", at(0, 0).input);
    let a1 = &at(0, 0).style;
    assert!(a1.bold && a1.italic && a1.underline.enabled());
    assert_eq!((a1.color, a1.fill, a1.font_size, a1.font.as_deref()), (Rgb(0xFF0000), Some(Rgb(0xFFFF00)), 1000, Some("Meiryo")));
    // The merged cell takes columns 1-2 and rows 0-1, so the cell below it (row 1) comes after them: column 3.
    let m = at(0, 1);
    assert_eq!((m.rows, m.columns), (2, 2));
    assert_eq!(m.shown, "M & M\n2nd");
    assert_eq!(m.style.align, CellAlign::Center);
    assert!(matches!(at(0, 3).input, CellInput::Value(CellValue::Bool(true))));
    assert_eq!(number(at(1, 0)), Some(0.5));
    assert_eq!(at(1, 0).style.number_format, "0.0%");
    assert!(matches!(at(1, 3).input, CellInput::Value(CellValue::Error(_))));
    // Text that Excel says is text stays text; a plain cell is read as typed; an empty cell is empty.
    assert!(matches!(&at(2, 0).input, CellInput::Value(CellValue::Text(t)) if t == "007"));
    assert_eq!(number(at(2, 1)), Some(12.0));
    assert!(matches!(at(2, 2).input, CellInput::Clear));
    assert!(matches!(&at(2, 3).input, CellInput::Value(CellValue::Text(t)) if t == "plain"));
    // No table, no cells.
    assert!(read("<p>text</p>", false).is_none());
}

/// "HTML Format" of Windows wraps the HTML with offsets; reading it gives the HTML back.
pub(super) fn windows_format() {
    let html = "<html><body><table><!--StartFragment--><tr><td>日本語</td></tr><!--EndFragment--></table></body></html>"; // i18n-ok（試験の中の日本語のデータ）
    let bytes = crate::native_shape_clipboard::cf_html(html);
    let head = String::from_utf8_lossy(&bytes[..120]).to_string();
    assert!(head.starts_with("Version:0.9\r\nStartHTML:0000000105"), "{head}");
    assert_eq!(html_of_bytes(&bytes).as_deref(), Some(html));
    // The fragment offsets point at the table rows.
    let number = |key: &str| head.split("\r\n").find_map(|l| l.strip_prefix(key)).unwrap().parse::<usize>().unwrap();
    assert!(bytes[number("StartFragment:")..number("EndFragment:")].starts_with(b"<tr>"));
}

fn select(s: &mut Scenario, range: CellRange) {
    s.update(move |e, _, _| e.set_selected_ranges(&[range]));
}

/// Cells copied here are put on the clipboard as a table that reads back as the same cells (values, formulas, formats).
pub(super) fn copy_round_trip() {
    let mut s = Scenario::open(SRC);
    crate::native_shape_clipboard::simulate_html(None);
    // Make A1 carry formats: bold, a fill and two decimals.
    s.update(|e, _, _| {
        let slide = e.current_slide;
        let ranges = vec![CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 }];
        let commands = [StyleChange::Bold(true), StyleChange::Fill(Some(Rgb(0x336699))), StyleChange::NumberFormat("0.00".into())]
            .into_iter()
            .map(|change| Command::Sheet(SheetCommand::SetFormat { slide, ranges: ranges.clone(), change }))
            .collect();
        e.document.execute(commands).unwrap();
    });
    select(&mut s, CellRange { first_row: 0, first_column: 0, last_row: 1, last_column: 2 });
    s.update(|e, _, cx| e.copy_cells(false, cx));
    let html = crate::native_shape_clipboard::read_html().expect("the copy put a table on the clipboard");
    assert!(html.contains("ProgId content=Excel.Sheet"), "{html}");
    let cells = read(&html, false).expect("a table");
    let (a1, c1) = (cells.iter().find(|c| (c.row, c.column) == (0, 0)).unwrap(), cells.iter().find(|c| (c.row, c.column) == (0, 2)).unwrap());
    assert_eq!(number(a1), Some(5.0));
    assert!(a1.style.bold);
    assert_eq!(a1.style.fill, Some(Rgb(0x336699)));
    assert_eq!(a1.style.number_format, "0.00");
    assert_eq!(a1.shown, "5.00");
    // C1 holds `=MyRng*2` in the file: it is a formula in the table too.
    assert!(matches!(&c1.input, CellInput::Formula(f) if f == "MyRng*2"), "{:?}", c1.input);
}

/// Pasting a table that another program put on the clipboard (the text beside it differs from what was copied here) puts its
/// cells with formats, formulas and merges.
pub(super) fn paste_table() {
    let mut s = Scenario::open(SRC);
    let html = r#"<html><head><style>td {font-size:9.0pt;} .h {font-weight:700; background:#CCFFCC; border:.5pt solid windowtext;}</style></head><body><table>
<tr><td class=h>Item</td><td class=h colspan=2>Total</td></tr>
<tr><td>apples</td><td x:num="4">4</td><td x:num="6" x:fmla="=B2+2">6</td></tr></table></body></html>"#;
    crate::native_shape_clipboard::simulate_html(Some(html.to_string()));
    s.update(|_, _, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("Item\tTotal\napples\t4\t6\n".to_string())));
    s.click_cell("H5");
    s.update(|e, _, cx| e.paste_cells(false, cx));
    assert_eq!(s.shown("H5"), "Item");
    assert_eq!(s.shown("H6"), "apples");
    assert_eq!(s.value("I6"), CellValue::Number(4.0));
    // The formula's references move with the paste, as the copied cell's would: here the source is not known, so they stay.
    assert_eq!(s.formula("J6").as_deref(), Some("B2+2"));
    let (column, row) = (7u32, 4u32);
    let (fill, bold, size, merged, border) = s.with(move |e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        let st = &sheet.styles[sheet.cell(row, column).unwrap().style as usize];
        (st.fill, st.bold, st.font_size, sheet.merges.contains(&CellRange { first_row: 4, first_column: 8, last_row: 4, last_column: 9 }), st.left.is_some())
    });
    assert_eq!((fill, bold, size, merged, border), (Some(Rgb(0xCCFFCC)), true, 900, true, true));
    // Pasting as values only ignores the table.
    s.click_cell("H9");
    s.update(|e, _, cx| e.paste_cells(true, cx));
    assert_eq!(s.shown("H9"), "Item");
    let bold9 = s.with(move |e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.cell(8, 7).map(|c| sheet.styles[c.style as usize].bold).unwrap_or(false)
    });
    assert!(!bold9);
    crate::native_shape_clipboard::simulate_html(None);
}
