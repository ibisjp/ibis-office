// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A form control put on a sheet that has comments but no drawing yet: the new `<drawing>` goes before the comments'
//! `<legacyDrawing>` (CT_Worksheet's order). It was put after it, and Excel replaced the whole sheet as unreadable
//! (2026-11-12, a workbook with notes where a Checkbox Control was inserted).

use std::sync::Arc;

use ibis_doc_model::{ControlKind, EmuRect, FormControl, Shape, ShapeId, ShapeKind};
use ibis_pptx::SaveOptions;

fn sheet_xml(bytes: &[u8]) -> String {
    let data = ibis_pptx::ibis_ooxml::Package::open(bytes.to_vec()).expect("a package").read("xl/worksheets/sheet1.xml").expect("sheet1");
    String::from_utf8(data).expect("utf-8")
}

#[test]
fn a_control_on_a_sheet_with_comments_puts_drawing_before_legacy_drawing() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/xlsx-form-controls-2007.xlsx");
    let mut loaded = ibis_pptx::open(std::fs::read(path).expect("the test workbook")).expect("opens");
    let before = sheet_xml(&loaded.source.save(&loaded.presentation, SaveOptions::default()).expect("save"));
    assert!(before.contains("<legacyDrawing") && !before.contains("<drawing "), "the sample has VML (legacyDrawing) and no drawing");
    let control = FormControl::new(ControlKind::CheckBox, "Check Box 1");
    let frame = EmuRect { x: 914400, y: 914400, width: 914400, height: 228600 };
    loaded.presentation.slides[0].shapes.push(Shape::new(ShapeId(9999), frame, ShapeKind::Control(Arc::new(control))));
    let saved = loaded.source.save(&loaded.presentation, SaveOptions::default()).expect("save with the control");
    let xml = sheet_xml(&saved);
    let drawing = xml.find("<drawing ").expect("a drawing element");
    let legacy = xml.find("<legacyDrawing").expect("the comments' VML is kept");
    assert!(drawing < legacy, "<drawing> comes before <legacyDrawing>: {}", &xml[xml.len().saturating_sub(600)..]);
    assert!(ibis_pptx::open(saved).is_ok(), "it opens again");
}

/// A book saved before the fix (`<drawing>` after `<legacyDrawing>`) is put in order when it is saved again with a change.
#[test]
fn a_book_with_drawing_after_legacy_drawing_is_mended_when_saved() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/xlsx-form-controls-2007.xlsx");
    let mut loaded = ibis_pptx::open(std::fs::read(path).expect("the test workbook")).expect("opens");
    let control = FormControl::new(ControlKind::CheckBox, "Check Box 1");
    let frame = EmuRect { x: 914400, y: 914400, width: 914400, height: 228600 };
    loaded.presentation.slides[0].shapes.push(Shape::new(ShapeId(9999), frame, ShapeKind::Control(Arc::new(control))));
    let saved = loaded.source.save(&loaded.presentation, SaveOptions::default()).expect("save with the control");
    // Put the drawing after the VML as the old save did.
    let mut package = ibis_pptx::ibis_ooxml::Package::open(saved).expect("a package");
    let xml = String::from_utf8(package.read("xl/worksheets/sheet1.xml").unwrap()).unwrap();
    let drawing = regex_lite_find(&xml, "<drawing ", "/>");
    let broken = xml.replacen(&drawing, "", 1);
    let broken = {
        let at = broken.find("<legacyDrawing").unwrap();
        let end = at + broken[at..].find("/>").unwrap() + 2;
        format!("{}{}{}", &broken[..end], drawing, &broken[end..])
    };
    assert!(broken.find("<drawing ").unwrap() > broken.find("<legacyDrawing").unwrap(), "the old order");
    package.set_bytes("xl/worksheets/sheet1.xml", broken.into_bytes());
    let old = package.save(ibis_pptx::ibis_ooxml::SaveOptions::default()).expect("the old book");
    // Opened and saved again with a change to the sheet.
    let mut again = ibis_pptx::open(old).expect("the old book opens in ibisOffice");
    let control = FormControl::new(ControlKind::CheckBox, "Check Box 2");
    let frame = EmuRect { x: 914400 * 3, y: 914400, width: 914400, height: 228600 };
    again.presentation.slides[0].shapes.push(Shape::new(ShapeId(9998), frame, ShapeKind::Control(Arc::new(control))));
    let mended = sheet_xml(&again.source.save(&again.presentation, SaveOptions::default()).expect("save again"));
    assert!(mended.find("<drawing ").unwrap() < mended.find("<legacyDrawing").unwrap(), "put in order");
}

fn regex_lite_find(xml: &str, start: &str, end: &str) -> String {
    let at = xml.find(start).expect("the start");
    let to = at + xml[at..].find(end).expect("the end") + end.len();
    xml[at..to].to_string()
}
