// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A row fitted to its wrapping text is saved as Excel writes it (`ht` without `customHeight`); one sized by hand keeps
//! `customHeight`; undo gives both the height and the mark back.

use ibis_doc_model::sheet_calc::sheet_of_slide;
use ibis_doc_model::sheet_command::Axis;
use ibis_doc_model::{Command, Document, EMU_PER_POINT, SheetCommand};
use ibis_pptx::SaveOptions;

fn row_tag(bytes: &[u8], row: u32) -> String {
    let package = ibis_ooxml::Package::open(bytes.to_vec()).unwrap();
    let names: Vec<String> = package.names().filter(|n| n.starts_with("xl/worksheets/sheet")).map(str::to_string).collect();
    for name in names {
        let xml = String::from_utf8_lossy(&package.read(&name).unwrap()).to_string();
        let key = format!("<row r=\"{row}\"");
        if let Some(at) = xml.find(&key) {
            return xml[at..xml[at..].find('>').map(|e| at + e).unwrap()].to_string();
        }
    }
    String::new()
}

#[test]
fn a_fitted_row_has_ht_and_no_custom_height() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx");
    let opened = ibis_pptx::open(std::fs::read(path).unwrap()).unwrap();
    let slide = opened.presentation.slides.iter().find(|s| s.name.as_deref() == Some("月次売上")).unwrap().id;
    let rows = |d: &Document| sheet_of_slide(d.presentation().slide(slide).unwrap()).unwrap().rows.clone();
    let mut d = Document::new(opened.presentation.clone());
    let before = rows(&d);
    d.execute(vec![
        Command::Sheet(SheetCommand::SetRowHeightFitted { slide, first: 4, last: 4, size: 45 * EMU_PER_POINT }),
        Command::Sheet(SheetCommand::SetSize { slide, axis: Axis::Rows, first: 5, last: 5, size: 30 * EMU_PER_POINT }),
    ])
    .unwrap();
    let saved = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
    let fitted = row_tag(&saved, 5);
    assert!(fitted.contains("ht=\"45\"") && !fitted.contains("customHeight"), "fitted row: {fitted}");
    let fixed = row_tag(&saved, 6);
    assert!(fixed.contains("ht=\"30\"") && fixed.contains("customHeight=\"1\""), "fixed row: {fixed}");
    // One undo gives both back, with their marks.
    assert!(d.undo());
    assert_eq!(rows(&d), before);
    let sheet = sheet_of_slide(d.presentation().slide(slide).unwrap()).unwrap();
    assert!(!sheet.custom_rows.contains(&5));
    // A fixed row given back to the sheet loses the mark, and the file loses `customHeight`.
    let mut d2 = Document::new(opened.presentation.clone());
    d2.execute(vec![Command::Sheet(SheetCommand::SetSize { slide, axis: Axis::Rows, first: 5, last: 5, size: 30 * EMU_PER_POINT })]).unwrap();
    d2.execute(vec![Command::Sheet(SheetCommand::SetRowHeightFitted { slide, first: 5, last: 5, size: 30 * EMU_PER_POINT })]).unwrap();
    let saved2 = opened.source.save(d2.presentation(), SaveOptions::default()).unwrap();
    let tag = row_tag(&saved2, 6);
    assert!(tag.contains("ht=\"30\"") && !tag.contains("customHeight"), "row given back: {tag}");
}
