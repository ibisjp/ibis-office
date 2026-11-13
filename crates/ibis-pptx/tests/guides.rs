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
    // (The view properties keep no id and no colour: the ids are the order.)
    let made = vec![
        SlideGuide { id: 1, horizontal: false, pos: 3160, color: None, user_drawn: true },
        SlideGuide { id: 2, horizontal: true, pos: 1117, color: None, user_drawn: true },
    ];
    document.execute(vec![Command::SetGuides { slide: None, guides: made.clone() }]).unwrap();
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    // PowerPoint's place: `p:guideLst` of the view properties, after the normal view's, and not the extension.
    let view = entry(&saved, "ppt/viewProps.xml");
    assert!(view.contains("<p:guideLst>") && view.contains("pos=\"3160\"") && view.contains("orient=\"horz\""), "{view}");
    assert!(view.find("normalViewPr").unwrap() < view.find("slideViewPr").unwrap(), "{view}");
    // And the extension, as PowerPoint 365 writes both.
    let presentation = entry(&saved, "ppt/presentation.xml");
    assert!(presentation.contains("sldGuideLst"));
    // Every p15:guide has its p15:clr: PowerPoint cannot open the file without it (#M132); a guide without a colour of its own
    // is written in PowerPoint's gray for the guides a user draws.
    assert_eq!(presentation.matches("<p15:guide ").count(), presentation.matches("<p15:clr>").count(), "{presentation}");
    assert!(presentation.contains("A4A3A4"), "{presentation}");
    let again = ibis_pptx::open(saved.clone()).unwrap();
    let gray: Vec<SlideGuide> = made.iter().cloned().map(|mut g| { g.color = Some(0xA4A3A4); g }).collect();
    assert!(again.presentation.guides == made || again.presentation.guides == gray, "{:?}", again.presentation.guides);
    // Taken away again.
    let mut document = Document::new(again.presentation.clone());
    document.execute(vec![Command::SetGuides { slide: None, guides: Vec::new() }]).unwrap();
    let cleared = again.source.save(document.presentation(), SaveOptions::default()).unwrap();
    assert!(!entry(&cleared, "ppt/viewProps.xml").contains("guideLst"));
    assert!(!entry(&cleared, "ppt/presentation.xml").contains("p15:guide "));
    assert!(ibis_pptx::open(cleared).unwrap().presentation.guides.is_empty());
    // Undo.
    let mut document = Document::new(opened.presentation.clone());
    document.execute(vec![Command::SetGuides { slide: None, guides: made }]).unwrap();
    assert!(document.undo());
    assert!(document.presentation().guides.is_empty());
}

#[test]
fn guides_in_the_extension_of_the_presentation_only_are_read_and_written_to_both() {
    // A deck of an older writer: the normal view's guides in `presentation.xml`.
    let data = file("editing/pptx-footer-by-powerpoint.pptx");
    let mut package = ibis_ooxml::Package::open(data).unwrap();
    let old = vec![guide(1, false, 3160), guide(2, true, 1117)];
    let xml = String::from_utf8(package.read("ppt/presentation.xml").unwrap()).unwrap();
    let ext = "<p:ext uri=\"{EFAFB233-063F-42B5-8137-9DF3F51BA10A}\"><p15:sldGuideLst xmlns:p15=\"http://schemas.microsoft.com/office/powerpoint/2012/main\"><p15:guide id=\"1\" pos=\"3160\" userDrawn=\"1\"><p15:clr><a:srgbClr val=\"FF0000\"/></p15:clr></p15:guide><p15:guide id=\"2\" orient=\"horz\" pos=\"1117\" userDrawn=\"1\"><p15:clr><a:srgbClr val=\"FF0000\"/></p15:clr></p15:guide></p15:sldGuideLst></p:ext>";
    let xml = if xml.contains("</p:extLst>") { xml.replace("</p:extLst>", &format!("{ext}</p:extLst>")) } else { xml.replace("</p:presentation>", &format!("<p:extLst>{ext}</p:extLst></p:presentation>")) };
    package.set_bytes("ppt/presentation.xml", xml.into_bytes());
    let data = package.save(Default::default()).unwrap();
    let opened = ibis_pptx::open(data).unwrap();
    assert_eq!(opened.presentation.guides, old);
    // Written to both when the guides change.
    let mut document = Document::new(opened.presentation.clone());
    let mut changed = old.clone();
    changed[0].pos = 3000;
    document.execute(vec![Command::SetGuides { slide: None, guides: changed }]).unwrap();
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    assert!(entry(&saved, "ppt/viewProps.xml").contains("pos=\"3000\""), "{}", entry(&saved, "ppt/viewProps.xml"));
    assert!(entry(&saved, "ppt/presentation.xml").contains("pos=\"3000\""));
    let again = ibis_pptx::open(saved).unwrap();
    assert_eq!(again.presentation.guides.iter().map(|g| (g.horizontal, g.pos)).collect::<Vec<_>>(), vec![(false, 3000), (true, 1117)]);
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

#[test]
fn a_layout_carries_its_own_guides_and_its_masters() {
    // A deck whose layouts have guides of their own: kept in the layout, and none from the master.
    let opened = ibis_pptx::open(file("benchmark/earnings-presentation.pptx")).unwrap();
    let first = &opened.presentation.layouts[0];
    assert!(first.guides.len() >= 5, "{:?}", first.guides);
    assert!(first.master_guides.is_empty());
    // Guides put on a master page reach every layout of the master as its master's, and no layout's own.
    let data = file("editing/pptx-plan.pptx");
    let pages = master_page::pages(&data).unwrap();
    let master = pages.iter().find(|p| p.master).unwrap();
    let page = ibis_pptx::open(master_page::build(&data, &master.part).unwrap()).unwrap();
    let mut document = Document::new(page.presentation.clone());
    let made = vec![guide(1, false, 370), guide(2, true, 831)];
    document.execute(vec![Command::SetGuides { slide: Some(page.presentation.slides[0].id), guides: made.clone() }]).unwrap();
    let edited = page.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let back = master_page::apply(&data, &edited, &master.part).unwrap();
    let whole = ibis_pptx::open(back.clone()).unwrap();
    assert!(whole.presentation.layouts.len() > 1);
    for layout in &whole.presentation.layouts {
        assert_eq!(layout.master_guides, made, "{}", layout.name);
        assert!(layout.guides.is_empty(), "{}", layout.name);
    }
    // A layout page's guides are that layout's own.
    let layout_page = pages.iter().find(|p| !p.master).unwrap();
    let page = ibis_pptx::open(master_page::build(&back, &layout_page.part).unwrap()).unwrap();
    let mut document = Document::new(page.presentation.clone());
    let own = vec![guide(1, true, 100)];
    document.execute(vec![Command::SetGuides { slide: Some(page.presentation.slides[0].id), guides: own.clone() }]).unwrap();
    let edited = page.source.save(document.presentation(), SaveOptions::default()).unwrap();
    let back = master_page::apply(&back, &edited, &layout_page.part).unwrap();
    let whole = ibis_pptx::open(back).unwrap();
    assert_eq!(whole.presentation.layouts.iter().filter(|l| l.guides == own).count(), 1);
    assert!(whole.presentation.layouts.iter().all(|l| l.master_guides == made));
}
