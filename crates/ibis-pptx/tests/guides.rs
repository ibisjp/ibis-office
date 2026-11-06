// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! issue #84 ガイド線: the guides (`p15:sldGuideLst`) of a presentation, a master and a layout are read, kept and written.

use ibis_doc_model::{Command, Document, SlideGuide};
use ibis_pptx::{master_page, SaveOptions};

fn file(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn entry(data: &[u8], part: &str) -> String {
    let package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    String::from_utf8(package.read(part).unwrap()).unwrap()
}

fn guide(id: u32, horizontal: bool, pos: i32) -> SlideGuide {
    SlideGuide { id, horizontal, pos, color: Some(0xff0000), user_drawn: true }
}

#[test]
fn the_guides_of_a_layout_page_are_read() {
    // PowerPoint's layout guides: the middle (2160 horizontal, 3840 vertical) and the edges, in 1/8 pt.
    let data = file("benchmark/earnings-presentation.pptx");
    let pages = master_page::pages(&data).unwrap();
    let layout = pages.iter().find(|p| !p.master && p.part.ends_with("slideLayout1.xml")).unwrap();
    let opened = ibis_pptx::open(master_page::build(&data, &layout.part).unwrap()).unwrap();
    let guides = &opened.presentation.slides[0].guides;
    assert!(guides.len() >= 5, "{guides:?}");
    assert!(guides.iter().any(|g| g.horizontal && g.pos == 2160), "{guides:?}");
    assert!(guides.iter().any(|g| !g.horizontal && g.pos == 3840), "{guides:?}");
    // The slide is 12192000 EMU wide: 3840 units are its middle.
    assert_eq!(guides.iter().find(|g| !g.horizontal && g.pos == 3840).unwrap().emu(), 6_096_000);
    // A guide with no `pos` is at 0.
    assert!(guides.iter().any(|g| !g.horizontal && g.pos == 0), "{guides:?}");
}

#[test]
fn guides_of_a_layout_stay_when_the_page_is_edited_and_put_back() {
    let data = file("benchmark/earnings-presentation.pptx");
    let pages = master_page::pages(&data).unwrap();
    for page in &pages {
        let opened = ibis_pptx::open(master_page::build(&data, &page.part).unwrap()).unwrap();
        let before = opened.presentation.slides[0].guides.clone();
        let edited = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
        let back = master_page::apply(&data, &edited, &page.part).unwrap();
        let again = ibis_pptx::open(master_page::build(&back, &page.part).unwrap()).unwrap();
        assert_eq!(again.presentation.slides[0].guides, before, "{}", page.part);
        // The part still has them in its own extension.
        assert_eq!(entry(&back, &page.part).contains("sldGuideLst"), !before.is_empty(), "{}", page.part);
    }
}

#[test]
fn guides_added_on_a_master_page_are_the_masters() {
    let data = file("editing/pptx-plan.pptx");
    let pages = master_page::pages(&data).unwrap();
    let master = pages.iter().find(|p| p.master).unwrap();
    let opened = ibis_pptx::open(master_page::build(&data, &master.part).unwrap()).unwrap();
    let slide = opened.presentation.slides[0].id;
    let mut document = Document::new(opened.presentation.clone());
    let made = vec![guide(1, false, 370), guide(2, true, 831)];
    document.execute(vec![Command::SetGuides { slide: Some(slide), guides: made.clone() }]).unwrap();
    let edited = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let back = master_page::apply(&data, &edited, &master.part).unwrap();
    // In the master's own extension (not the layouts', not the slides').
    let xml = entry(&back, &master.part);
    assert!(xml.contains(&format!("uri=\"{}\"", "{27BBF7A9-308A-43DC-89C8-2F10F3537804}")) && xml.contains("pos=\"370\"") && xml.contains("orient=\"horz\""), "{xml}");
    let again = ibis_pptx::open(master_page::build(&back, &master.part).unwrap()).unwrap();
    assert_eq!(again.presentation.slides[0].guides, made);
    // And taken away again.
    let opened = ibis_pptx::open(master_page::build(&back, &master.part).unwrap()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    document.execute(vec![Command::SetGuides { slide: Some(opened.presentation.slides[0].id), guides: Vec::new() }]).unwrap();
    let edited = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let cleared = master_page::apply(&back, &edited, &master.part).unwrap();
    assert!(!entry(&cleared, &master.part).contains("sldGuideLst"));
}

#[test]
fn the_guides_of_the_normal_view_are_read_and_written() {
    let data = file("editing/pptx-footer-by-powerpoint.pptx");
    let opened = ibis_pptx::open(data.clone()).unwrap();
    assert!(opened.presentation.guides.is_empty());
    // Unchanged: the file as it was.
    let same = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
    assert_eq!(entry(&same, "ppt/presentation.xml"), entry(&data, "ppt/presentation.xml"));
    let mut document = Document::new(opened.presentation.clone());
    let made = vec![guide(1, false, 3160), guide(2, true, 1117)];
    document.execute(vec![Command::SetGuides { slide: None, guides: made.clone() }]).unwrap();
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let xml = entry(&saved, "ppt/presentation.xml");
    assert!(xml.contains("{EFAFB233-063F-42B5-8137-9DF3F51BA10A}") && xml.contains("pos=\"3160\"") && xml.contains("orient=\"horz\""), "{xml}");
    let again = ibis_pptx::open(saved).unwrap();
    assert_eq!(again.presentation.guides, made);
    // Undo.
    assert!(document.undo());
    assert!(document.presentation().guides.is_empty());
}

#[test]
fn a_deck_with_guides_keeps_them_through_an_edit_and_a_save() {
    let data = file("benchmark/earnings-presentation.pptx");
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let before = opened.presentation.guides.clone();
    let mut document = Document::new(opened.presentation.clone());
    let slide = document.presentation().slides[0].id;
    document.execute(vec![Command::SetBackdropHidden { slide, hidden: true }]).unwrap();
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    assert_eq!(ibis_pptx::open(saved.clone()).unwrap().presentation.guides, before);
    // The layouts' guides are in the saved file as they were.
    assert_eq!(entry(&saved, "ppt/slideLayouts/slideLayout1.xml").matches("<p15:guide ").count(), entry(&data, "ppt/slideLayouts/slideLayout1.xml").matches("<p15:guide ").count());
}
