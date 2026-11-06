// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A cell gradient (`gradientFill`) with more than two stops, edited with the gradient slider of セルの書式設定: the stops are
//! saved in the order of their positions however the editor keeps them, and read back with all their colours and positions.

use ibis_doc_model::sheet_calc::sheet_of_slide;
use ibis_doc_model::{CellGradient, CellRange, Command, Document, Rgb, SheetCommand, StyleChange};
use ibis_pptx::SaveOptions;

#[test]
fn gradient_with_three_unsorted_stops_saves_in_order_and_reads_back() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx");
    let opened = ibis_pptx::open(std::fs::read(path).unwrap()).unwrap();
    let mut d = Document::new(opened.presentation.clone());
    let slide = opened.presentation.slides.iter().find(|s| s.name.as_deref() == Some("月次売上")).unwrap().id;
    let a1 = CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 };
    // The slider keeps a stop added later at the end of the list: positions 0, 1000, 400 (not in order).
    let gradient = CellGradient::Linear { degree: 45, stops: vec![(0, Rgb(0xFF0000)), (1000, Rgb(0x0000FF)), (400, Rgb(0x00FF00))] };
    d.execute(vec![Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![a1], change: StyleChange::Gradient(Some(gradient.clone())) })]).unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let pkg = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let styles = String::from_utf8(pkg.read("xl/styles.xml").unwrap()).unwrap();
    let at = |needle: &str| styles.find(needle).unwrap_or_else(|| panic!("{needle} in the styles: {styles}"));
    assert!(at(r#"rgb="FFFF0000""#) < at(r#"rgb="FF00FF00""#) && at(r#"rgb="FF00FF00""#) < at(r#"rgb="FF0000FF""#), "the stops are written in the order of their positions");
    assert!(styles.contains(r#"position="0.40""#), "the middle stop is at 0.40");
    let reopened = ibis_pptx::open(saved).unwrap().presentation;
    let sheet = sheet_of_slide(reopened.slide(slide).unwrap()).unwrap();
    let read = sheet.styles[sheet.cell(0, 0).unwrap().style as usize].gradient.clone().expect("the gradient is read back");
    assert_eq!(read, CellGradient::Linear { degree: 45, stops: vec![(0, Rgb(0xFF0000)), (400, Rgb(0x00FF00)), (1000, Rgb(0x0000FF))] });
    // The drawing order of the model is the same as the file's.
    let fill = gradient.to_fill();
    let positions: Vec<u32> = match fill {
        ibis_doc_model::Fill::Linear { stops, .. } => stops.iter().map(|s| s.position).collect(),
        other => panic!("{other:?}"),
    };
    assert_eq!(positions, vec![0, 40_000, 100_000]);
}
