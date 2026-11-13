// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The check box in a cell (Excel's Insert > Check Box, GitHub issue #32): a cell format whose `xfComplement` names a `Checkbox`
//! feature property bag is read as `CellStyle::checkbox`; saving keeps the bag, the relationship and the format's extension, a
//! value turned by a press is only a value change, and a check box made here is written as Excel writes its first one.

use ibis_doc_model::sheet_calc::sheet_of_slide;
use ibis_doc_model::{CellEdit, CellInput, CellRange, CellValue, Command, Document, SheetCommand, SlideId, StyleChange};
use ibis_pptx::SaveOptions;

const BAG: &str = "xl/featurePropertyBag/featurePropertyBag.xml";

fn a1() -> CellRange {
    CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 }
}

fn set(slide: SlideId, row: u32, value: CellValue) -> Command {
    Command::Sheet(SheetCommand::SetCells { slide, edits: vec![CellEdit { row, column: 0, input: CellInput::Value(value) }] })
}

fn part(data: &[u8], name: &str) -> Option<String> {
    ibis_pptx::ibis_ooxml::Package::open(data.to_vec()).ok()?.read(name).ok().map(|b| String::from_utf8(b).unwrap())
}

fn checkbox_of(presentation: &ibis_doc_model::Presentation, slide: SlideId, row: u32) -> (bool, Option<CellValue>) {
    let sheet = sheet_of_slide(presentation.slide(slide).unwrap()).unwrap();
    let cell = sheet.cell(row, 0);
    (cell.map_or(false, |c| sheet.styles[c.style as usize].checkbox), cell.map(|c| c.value.clone()))
}

/// A check box made in a new workbook is saved with the bag part, its relationship and content type and the format's extension, and read back.
#[test]
fn new_check_box_is_saved_as_excel_writes_it_and_read_back() {
    let opened = ibis_pptx::open(ibis_pptx::blank_file(ibis_pptx::Kind::Workbook).unwrap()).unwrap();
    let slide = opened.presentation.slides[0].id;
    let mut d = Document::new(opened.presentation.clone());
    d.execute(vec![
        Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![a1()], change: StyleChange::Checkbox(true) }),
        set(slide, 0, CellValue::Bool(false)),
    ])
    .unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let bag = part(&saved, BAG).expect("the feature property bag part");
    assert!(bag.contains(r#"<bag type="Checkbox"/>"#) && bag.contains("XFComplements"), "{bag}");
    assert!(part(&saved, "[Content_Types].xml").unwrap().contains("application/vnd.ms-excel.featurepropertybag+xml"));
    assert!(part(&saved, "xl/_rels/workbook.xml.rels").unwrap().contains("2022/11/relationships/FeaturePropertyBag"));
    let styles = part(&saved, "xl/styles.xml").unwrap();
    assert!(styles.contains("{C7286773-470A-42A8-94C5-96B5CB345126}") && styles.contains(r#"xfComplement i="0""#), "{styles}");
    let sheet = part(&saved, "xl/worksheets/sheet1.xml").unwrap();
    assert!(sheet.contains(r#"t="b""#), "{sheet}");

    let reopened = ibis_pptx::open(saved.clone()).unwrap();
    assert_eq!(checkbox_of(&reopened.presentation, slide, 0), (true, Some(CellValue::Bool(false))));

    // Turned (a value change): still a check box, the bag is the same part, and one undo step puts it back.
    let mut d = Document::new(reopened.presentation.clone());
    d.execute(vec![set(slide, 0, CellValue::Bool(true))]).unwrap();
    assert_eq!(checkbox_of(d.presentation(), slide, 0), (true, Some(CellValue::Bool(true))));
    let again = reopened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    assert_eq!(part(&again, BAG).unwrap(), bag);
    assert_eq!(checkbox_of(&ibis_pptx::open(again).unwrap().presentation, slide, 0), (true, Some(CellValue::Bool(true))));
    assert!(d.undo());
    assert_eq!(checkbox_of(d.presentation(), slide, 0).1, Some(CellValue::Bool(false)));
}

/// The sample of issue #32 (Microsoft 365 for Mac): two check boxes, FALSE. Opened and saved again (also with a change of its format), the
/// bag, its relationship and the extension stay. Skipped where the downloaded sample is not.
#[test]
fn excel_sample_is_read_and_its_bag_survives_saving() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/github-issue/32/CheckBox.xlsx");
    let Ok(data) = std::fs::read(path) else { return };
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let slide = opened.presentation.slides[0].id;
    for row in 0..2 {
        assert_eq!(checkbox_of(&opened.presentation, slide, row), (true, Some(CellValue::Bool(false))), "A{}", row + 1);
    }
    let sheet = sheet_of_slide(opened.presentation.slide(slide).unwrap()).unwrap();
    assert!(!sheet.styles[sheet.cell(0, 1).unwrap().style as usize].checkbox, "B1 is text");

    let mut d = Document::new(opened.presentation.clone());
    d.execute(vec![set(slide, 0, CellValue::Bool(true)), Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![a1()], change: StyleChange::Bold(true) })]).unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    assert_eq!(part(&saved, BAG), part(&data, BAG));
    assert!(part(&saved, "xl/_rels/workbook.xml.rels").unwrap().contains("FeaturePropertyBag"));
    let reopened = ibis_pptx::open(saved).unwrap();
    assert_eq!(checkbox_of(&reopened.presentation, slide, 0), (true, Some(CellValue::Bool(true))));
    assert_eq!(checkbox_of(&reopened.presentation, slide, 1), (true, Some(CellValue::Bool(false))));
}
