// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Copying and pasting cells as Excel's embedded worksheet (`com.microsoft.Embed-Source`: a compound file whose `Package`
//! stream is a one-sheet workbook).
//!
//! Excel's own object is the one `test/github-issue/83/0-com.microsoft.Embed-Source.bin` kept (what Excel for Mac put there
//! for two cells of 1,000 and 2,000 under the texts A and B, with a fill and borders). Excel itself was not asked to paste
//! what ibisOffice writes (not tried).

use super::SheetSteps;
use crate::shell::scenario::{Scenario, repo_root};
use crate::clipboard_native::{embed_source, matches_text, package_of, read};
use ibis_doc_model::sheet_command::StyleChange;
use ibis_doc_model::{CellRange, CellValue, Command, Rgb, SheetCommand, SlideId, sheet_calc::sheet_of_slide};
use crate::ext::*;

const SRC: &str = "test/editing/issue100-names-src.xlsx";

fn excel_bytes() -> Option<Vec<u8>> {
    let path = repo_root().join("test/github-issue/83/0-com.microsoft.Embed-Source.bin");
    let bytes = std::fs::read(&path).ok();
    if bytes.is_none() {
        eprintln!("no {}: skipped", path.display());
    }
    bytes
}

/// What Excel for Mac put there: the workbook is found in the compound file and its two rows come out with their formats.
pub(super) fn excel_mac_object() {
    let Some(bytes) = excel_bytes() else { return };
    assert!(package_of(&bytes).is_some());
    let (clip, merges) = read(&bytes, SlideId(1), "A\tB\n1,000\t2,000\n").expect("a workbook");
    assert!(merges.is_empty());
    assert_eq!(clip.cells.len(), 4);
    let at = |r: u32, c: u32| clip.cell_at(r, c).unwrap();
    assert_eq!(at(0, 0).cell.text, "A");
    assert_eq!(at(0, 1).cell.text, "B");
    assert_eq!(at(1, 0).cell.value, CellValue::Number(1000.0));
    assert_eq!(at(1, 1).cell.value, CellValue::Number(2000.0));
    // The orange fill and the thousands separator of the sample.
    assert_eq!(at(0, 0).style.fill, Some(Rgb(0xE97132)));
    assert_eq!(at(1, 0).cell.text, "1,000");
    assert!(at(1, 0).style.number_format.contains("#,##0"), "{}", at(1, 0).style.number_format);
    assert!(at(0, 0).style.top.is_some() && at(1, 1).style.bottom.is_some());
    assert!(matches_text(&clip, "A\tB\n1,000\t2,000\n"));
    // Text that something else put there does not belong to the object.
    assert!(!matches_text(&clip, "other\ttext\n"));
    assert_eq!((clip.range.last_row, clip.range.last_column), (1, 1));
}

/// Anything that is not a compound file with a workbook in it is not read.
pub(super) fn not_an_object() {
    assert!(read(b"", SlideId(1), "").is_none());
    assert!(read(b"plain text", SlideId(1), "").is_none());
    let other = ibis_pptx::ibis_ooxml::cfb::write(&[ibis_pptx::ibis_ooxml::cfb::Item::stream("Package", b"not a zip".to_vec())]);
    assert!(read(&other, SlideId(1), "").is_none());
}

fn select(s: &mut Scenario, range: CellRange) {
    s.update(move |e, _, _| e.set_selected_ranges(&[range]));
}

/// Cells copied here are put on the clipboard as an embedded worksheet that reads back as the same cells (values, a formula,
/// formats, a merged range, a column width), and the object is a compound file of the class Excel names for a worksheet.
pub(super) fn copy_round_trip() {
    let mut s = Scenario::open(SRC);
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
    s.update(|e, _, _| {
        let slide = e.current_slide;
        let one = vec![CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 }];
        let mut commands: Vec<Command> = [StyleChange::Bold(true), StyleChange::Fill(Some(Rgb(0x336699))), StyleChange::NumberFormat("0.00".into())]
            .into_iter()
            .map(|change| Command::Sheet(SheetCommand::SetFormat { slide, ranges: one.clone(), change }))
            .collect();
        commands.push(Command::Sheet(SheetCommand::Merge { slide, range: CellRange { first_row: 1, first_column: 0, last_row: 1, last_column: 1 }, center: false }));
        e.document.execute(commands).unwrap();
    });
    select(&mut s, CellRange { first_row: 0, first_column: 0, last_row: 1, last_column: 2 });
    s.update(|e, _, cx| e.copy_cells(false, cx));
    let object = crate::native_shape_clipboard::read_embed_source().expect("the copy put a worksheet on the clipboard");
    // A compound file with the class of Excel.Sheet.12 on the root, the workbook, and the OLE header.
    assert!(ibis_pptx::ibis_ooxml::cfb::is_cfb(&object));
    assert!(object.windows(16).any(|w| w == [0x30, 0x08, 0x02, 0, 0, 0, 0, 0, 0xC0, 0, 0, 0, 0, 0, 0, 0x46]));
    assert!(ibis_pptx::ibis_ooxml::cfb::stream(&object, "\u{1}CompObj").unwrap().windows(14).any(|w| w == b"Excel.Sheet.12"));
    let text = "dummy";
    let (clip, merges) = read(&object, SlideId(1), text).expect("a workbook");
    let at = |r: u32, c: u32| clip.cell_at(r, c).unwrap();
    assert_eq!(at(0, 0).cell.value, CellValue::Number(5.0));
    assert!(at(0, 0).style.bold);
    assert_eq!(at(0, 0).style.fill, Some(Rgb(0x336699)));
    assert_eq!(at(0, 0).style.number_format, "0.00");
    assert_eq!(at(0, 0).cell.text, "5.00");
    // C1 holds `=MyRng*2` in the file.
    assert_eq!(at(0, 2).cell.formula.as_deref(), Some("MyRng*2"));
    assert_eq!(merges, vec![CellRange { first_row: 1, first_column: 0, last_row: 1, last_column: 1 }]);
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
}

/// Pasting what Excel put there (the text beside it differs from what was copied here) brings the cells with their formats.
pub(super) fn paste_excel_object() {
    let Some(bytes) = excel_bytes() else { return };
    let mut s = Scenario::open(SRC);
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(Some(bytes));
    s.update(|_, _, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("A\tB\n1,000\t2,000\n".to_string())));
    s.click_cell("H5");
    s.update(|e, _, cx| e.paste_cells(false, cx));
    assert_eq!(s.shown("H5"), "A");
    assert_eq!(s.shown("I5"), "B");
    assert_eq!(s.value("H6"), CellValue::Number(1000.0));
    assert_eq!(s.shown("H6"), "1,000");
    let fill = s.with(|e, _| {
        let sheet = sheet_of_slide(e.slide()).unwrap();
        sheet.styles[sheet.cell(4, 7).unwrap().style as usize].fill
    });
    assert_eq!(fill, Some(Rgb(0xE97132)));
    // An object beside text that does not match it is not pasted: the text is typed in.
    s.update(|_, _, cx| cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("other".to_string())));
    s.click_cell("H9");
    s.update(|e, _, cx| e.paste_cells(false, cx));
    assert_eq!(s.shown("H9"), "other");
    crate::native_shape_clipboard::simulate_embed_source(None);
}

/// The object made of the workbook of an object is read back unchanged (`embed_source` of a package is a compound file whose
/// `Package` is that package).
pub(super) fn package_survives_wrapping() {
    let Some(bytes) = excel_bytes() else { return };
    let package = package_of(&bytes).unwrap();
    let wrapped = embed_source(&package);
    assert_eq!(package_of(&wrapped).unwrap(), package);
}

/// Cells copied here as an XML Spreadsheet (what Excel for Windows pastes): the bold, the fill and the number format as a
/// style, the formula in R1C1, the merged range. `IBIS_XMLSS_OUT` names a file to write it to (to paste it into Excel).
pub(super) fn xml_spreadsheet_of_copy() {
    let mut s = Scenario::open(SRC);
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
    s.update(|e, _, _| {
        let slide = e.current_slide;
        let one = vec![CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 }];
        let mut commands: Vec<Command> = [StyleChange::Bold(true), StyleChange::Fill(Some(Rgb(0x336699))), StyleChange::NumberFormat("0.00".into())]
            .into_iter()
            .map(|change| Command::Sheet(SheetCommand::SetFormat { slide, ranges: one.clone(), change }))
            .collect();
        commands.push(Command::Sheet(SheetCommand::Merge { slide, range: CellRange { first_row: 1, first_column: 0, last_row: 1, last_column: 1 }, center: false }));
        e.document.execute(commands).unwrap();
    });
    select(&mut s, CellRange { first_row: 0, first_column: 0, last_row: 1, last_column: 2 });
    s.update(|e, _, cx| e.copy_cells(false, cx));
    let object = crate::native_shape_clipboard::read_embed_source().expect("the copy put a worksheet on the clipboard");
    let (clip, merges) = read(&object, SlideId(1), "dummy").expect("a workbook");
    let xml = crate::clipboard_xmlss::write(&clip, &merges).expect("cells");
    if let Ok(out) = std::env::var("IBIS_XMLSS_OUT") {
        std::fs::write(out, &xml).unwrap();
    }
    assert!(xml.contains("<Font ") && xml.contains(" ss:Bold=\"1\""), "{xml}");
    assert!(xml.contains("<Interior ss:Color=\"#336699\" ss:Pattern=\"Solid\"/>"), "{xml}");
    assert!(xml.contains("<NumberFormat ss:Format=\"0.00\"/>"), "{xml}");
    assert!(xml.contains("ss:Formula=\"=MyRng*2\""), "{xml}");
    assert!(xml.contains("ss:MergeAcross=\"1\""), "{xml}");
    assert!(xml.contains("<Data ss:Type=\"Number\">5</Data>"), "{xml}");
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
}
