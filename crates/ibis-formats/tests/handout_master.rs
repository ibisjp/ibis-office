// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 配布資料マスター: the places of a handout master made a slide of a file of
//! its own, edited, and put back (the handout master made if the file has none).

use ibis_doc_model::{Command, Document, EmuRect};
use ibis_formats::handout_master::{self, Mark};
use ibis_formats::SaveOptions;

fn file(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn a_file_without_a_handout_master_has_the_usual_places() {
    let data = file("editing/pptx-plan.pptx");
    let layout = handout_master::layout(&data).unwrap();
    assert_eq!(layout.page, (6_858_000, 9_144_000));
    assert_eq!(layout.places.iter().map(|p| p.mark).collect::<Vec<_>>(), [Mark::Header, Mark::Date, Mark::Footer, Mark::Number]);
    assert_eq!(layout.places[1].align, "r");
    assert!(layout.places[3].bottom && layout.places.iter().all(|p| p.size_pt == 12.0));
}

#[test]
fn the_places_are_a_slide_and_come_back_edited() {
    let data = file("editing/pptx-plan.pptx");
    let temp = handout_master::build(&data).unwrap();
    let opened = ibis_formats::open(temp).unwrap();
    assert_eq!(opened.presentation.slides.len(), 1);
    assert_eq!((opened.presentation.slide_size.width, opened.presentation.slide_size.height), (6_858_000, 9_144_000));
    let slide = &opened.presentation.slides[0];
    assert_eq!(slide.shapes.len(), 4, "the four places");
    // The header moved right and made 20 pt, centred and red; the footer taken away.
    let header = slide.shapes.iter().find(|s| s.text_body().is_some_and(|b| b.text() == "ヘッダー")).unwrap().clone();
    let footer = slide.shapes.iter().find(|s| s.text_body().is_some_and(|b| b.text() == "フッター")).unwrap().id;
    let mut document = Document::new(opened.presentation.clone());
    let moved = EmuRect::new(header.frame.x + 500_000, header.frame.y + 100_000, header.frame.width, header.frame.height);
    document
        .execute(vec![
            Command::SetFrame { slide: slide.id, frames: vec![(header.id, moved)], with_attached: true },
            Command::SetCharacterFormat { slide: slide.id, shape: header.id, start: 0, end: 4, format: ibis_doc_model::CharacterFormat { font_size: Some(2000), color: Some(ibis_doc_model::Rgb(0xC00000)), ..Default::default() } },
            Command::DeleteShape { slide: slide.id, shapes: vec![footer], with_attached: true },
        ])
        .unwrap();
    let edited = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let back = handout_master::apply(&data, &edited).unwrap();
    // The file has a handout master now, which opens as before and has the places edited.
    let again = ibis_formats::open(back.clone()).unwrap();
    assert_eq!(again.presentation.slides.len(), ibis_formats::open(data.clone()).unwrap().presentation.slides.len());
    let layout = handout_master::layout(&back).unwrap();
    assert_eq!(layout.places.iter().map(|p| p.mark).collect::<Vec<_>>(), [Mark::Header, Mark::Date, Mark::Number], "the footer is gone");
    let header = &layout.places[0];
    assert_eq!((header.frame.0, header.frame.1), (moved.x, moved.y));
    assert_eq!(header.size_pt, 20.0);
    let package = ibis_ooxml::Package::open(back).unwrap();
    assert!(package.names().any(|n| n == "ppt/handoutMasters/handoutMaster1.xml"));
    let xml = String::from_utf8(package.read("ppt/handoutMasters/handoutMaster1.xml").unwrap()).unwrap();
    assert!(xml.contains("C00000"), "{xml}");
    let main = String::from_utf8(package.read("ppt/presentation.xml").unwrap()).unwrap();
    assert!(main.contains("handoutMasterIdLst"));
    // The file's other parts are as they were.
    let before = ibis_ooxml::Package::open(data).unwrap();
    for part in before.names().filter(|n| !matches!(*n, "ppt/presentation.xml" | "ppt/_rels/presentation.xml.rels" | "[Content_Types].xml")) {
        assert_eq!(before.read(part).unwrap(), package.read(part).unwrap(), "{part}");
    }
}

#[test]
fn nothing_edited_keeps_the_places() {
    let data = file("editing/pptx-plan.pptx");
    let temp = handout_master::build(&data).unwrap();
    let opened = ibis_formats::open(temp).unwrap();
    let edited = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
    let back = handout_master::apply(&data, &edited).unwrap();
    assert_eq!(handout_master::layout(&back).unwrap(), handout_master::layout(&data).unwrap());
}

/// A file PowerPoint saved with a handout master of its own (outside git: set IBIS_HANDOUT).
#[test]
#[ignore = "Requires a file with a handout master; set IBIS_HANDOUT"]
fn a_real_handout_master_comes_back() {
    let data = std::fs::read(std::env::var("IBIS_HANDOUT").unwrap()).unwrap();
    let before = handout_master::layout(&data).unwrap();
    assert!(!before.places.is_empty());
    let temp = handout_master::build(&data).unwrap();
    let opened = ibis_formats::open(temp).unwrap();
    assert_eq!(opened.presentation.slides[0].shapes.len(), before.places.len());
    let edited = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
    let back = handout_master::apply(&data, &edited).unwrap();
    assert_eq!(handout_master::layout(&back).unwrap(), before);
    ibis_formats::open(back).unwrap();
}

#[test]
fn the_background_set_on_the_page_is_the_handout_masters() {
    use ibis_doc_model::{Color, Fill, Rgb};
    let data = file("editing/pptx-plan.pptx");
    let opened = ibis_formats::open(handout_master::build(&data).unwrap()).unwrap();
    let slide = opened.presentation.slides[0].id;
    let blue = Fill::Solid(Color::opaque(Rgb(0xDDEBF7)));
    let mut document = Document::new(opened.presentation.clone());
    document.execute(vec![Command::SetBackground { slide, fill: blue.clone() }]).unwrap();
    let edited = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let back = handout_master::apply(&data, &edited).unwrap();
    let package = ibis_ooxml::Package::open(back.clone()).unwrap();
    let xml = String::from_utf8(package.read("ppt/handoutMasters/handoutMaster1.xml").unwrap()).unwrap();
    assert!(xml.contains("DDEBF7"), "{xml}");
    // The page opened again has it; edited again with nothing changed, it stays.
    let again = ibis_formats::open(handout_master::build(&back).unwrap()).unwrap();
    assert_eq!(again.presentation.slides[0].background, blue);
    let edited = again.source.save(&again.presentation, SaveOptions::default()).unwrap();
    let back2 = handout_master::apply(&back, &edited).unwrap();
    let after = ibis_formats::open(handout_master::build(&back2).unwrap()).unwrap();
    assert_eq!(after.presentation.slides[0].background, blue, "kept");
}
