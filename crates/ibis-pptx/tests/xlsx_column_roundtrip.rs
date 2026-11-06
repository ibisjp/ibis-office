// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A column shown again, hidden, or fitted to its contents is saved, and reads back as it was left (N1, N2 of the 0.8.5 review).

use ibis_doc_model::sheet_calc::sheet_of_slide;
use ibis_doc_model::sheet_command::Axis;
use ibis_doc_model::{Command, Document, EMU_PER_POINT, Presentation, SheetCommand, SlideId};
use ibis_pptx::SaveOptions;

fn open() -> (Vec<u8>, ibis_pptx::Opened) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx");
    let data = std::fs::read(path).unwrap();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    (data, opened)
}

fn first_slide(p: &Presentation) -> SlideId {
    p.slides.iter().find(|s| s.name.as_deref() == Some("月次売上")).unwrap().id
}

fn widths(p: &Presentation) -> Vec<i64> {
    let s = p.slides.iter().find(|s| s.name.as_deref() == Some("月次売上")).unwrap();
    sheet_of_slide(s).unwrap().columns.iter().map(|&w| w as i64).collect()
}

fn reopen(bytes: Vec<u8>) -> ibis_pptx::Opened {
    ibis_pptx::open(bytes).unwrap()
}

/// Hidden, saved, opened, shown again, saved, opened: the column is shown.
#[test]
fn a_column_shown_again_stays_shown_after_saving() {
    let (_, opened) = open();
    let slide = first_slide(&opened.presentation);
    let mut d = Document::new(opened.presentation.clone());
    let original = widths(d.presentation());
    assert!(original[6] > 0);
    d.execute(vec![Command::Sheet(SheetCommand::Hide { slide, axis: Axis::Columns, first: 6, last: 6, hidden: true })]).unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let hidden = reopen(saved);
    assert_eq!(widths(&hidden.presentation)[6], 0, "hidden is saved");
    // A column the file gives no width of its own (G): hidden, it is written without `customWidth`, as ibisOffice writes it.
    let mut d2 = Document::new(hidden.presentation.clone());
    d2.execute(vec![Command::Sheet(SheetCommand::Hide { slide, axis: Axis::Columns, first: 6, last: 6, hidden: false })]).unwrap();
    assert!(widths(d2.presentation())[6] > 0);
    let saved2 = hidden.source.save(d2.presentation(), SaveOptions::default()).unwrap();
    let shown = reopen(saved2);
    assert!(widths(&shown.presentation)[6] > 0, "shown again is saved");
}

/// A column fitted to its contents keeps the width in the saved file.
#[test]
fn a_fitted_column_width_is_saved() {
    let (_, opened) = open();
    let slide = first_slide(&opened.presentation);
    let mut d = Document::new(opened.presentation.clone());
    let before = widths(d.presentation())[6];
    let size = before + 60 * EMU_PER_POINT;
    d.execute(vec![Command::Sheet(SheetCommand::SetColumnWidthFitted { slide, first: 6, last: 6, size })]).unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let read = reopen(saved);
    let after = widths(&read.presentation)[6];
    assert!((after - widths(d.presentation())[6]).abs() < (EMU_PER_POINT / 2) as i64, "fitted width {after} read back");
    // The fitted column is not one fixed by hand: it still widens by itself.
    let s = sheet_of_slide(read.presentation.slide(slide).unwrap()).unwrap();
    assert!(!s.custom_columns.contains(&6));
    // A width the sheet chose by itself stays unsaved.
    let mut d3 = Document::new(opened.presentation.clone());
    d3.execute(vec![Command::Sheet(SheetCommand::SetColumnWidthAuto { slide, first: 6, last: 6, size })]).unwrap();
    let saved3 = opened.source.save(d3.presentation(), SaveOptions::default()).unwrap();
    assert_eq!(widths(&reopen(saved3).presentation)[6], before);
}
