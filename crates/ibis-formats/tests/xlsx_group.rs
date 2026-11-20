// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #238: sheets selected together (`tabSelected` on more than one sheet) are read as the workbook's group (`selected_tabs`), written back, and a group
//! that is ended is ended in the file.
use ibis_doc_model::Document;
use ibis_formats::SaveOptions;

fn file(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing").join(name)).unwrap()
}

fn selected(bytes: &[u8]) -> Vec<bool> {
    let package = ibis_ooxml::Package::open(bytes.to_vec()).unwrap();
    let mut marks = Vec::new();
    for n in 1..=9 {
        let name = format!("xl/worksheets/sheet{n}.xml");
        let Ok(part) = package.read(&name).or_else(|_| package.read(&format!("/{name}"))) else { break };
        let text = String::from_utf8_lossy(&part).into_owned();
        let tag = text.find("<sheetView ").map(|at| &text[at..at + text[at..].find('>').unwrap()]).unwrap_or("");
        marks.push(tag.contains("tabSelected=\"1\""));
    }
    marks
}

#[test]
fn a_group_of_sheets_is_read_written_and_ended() {
    let data = file("xlsx-compatibility-v1.xlsx");
    let opened = ibis_formats::open(data.clone()).unwrap();
    assert!(opened.presentation.workbook.as_ref().unwrap().selected_tabs.is_empty(), "one sheet selected is no group");
    // Two sheets selected together: written.
    let mut doc = Document::new(opened.presentation.clone());
    std::sync::Arc::make_mut(doc.presentation_mut().workbook.as_mut().unwrap()).selected_tabs = vec![0, 2];
    let grouped = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    assert_eq!(selected(&grouped), vec![true, false, true]);
    // Opened again: the group is read, and saved unchanged the file is as it was.
    let again = ibis_formats::open(grouped.clone()).unwrap();
    assert_eq!(again.presentation.workbook.as_ref().unwrap().selected_tabs, vec![0, 2]);
    assert_eq!(again.source.save(&again.presentation, SaveOptions::default()).unwrap(), grouped, "an unchanged group is left as it is");
    // The group ended (the editor hands back no group): the marks go, and the present sheet alone is selected.
    let mut doc = Document::new(again.presentation.clone());
    std::sync::Arc::make_mut(doc.presentation_mut().workbook.as_mut().unwrap()).selected_tabs = Vec::new();
    let ended = again.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    assert_eq!(selected(&ended), vec![true, false, false], "{:?}", selected(&ended));
    assert!(ibis_formats::open(ended).unwrap().presentation.workbook.as_ref().unwrap().selected_tabs.is_empty());
}
