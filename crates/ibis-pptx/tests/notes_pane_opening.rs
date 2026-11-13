// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #M96 (4): how the notes pane stands when a presentation is opened is read from `normalViewPr` of `viewProps.xml`
//! (`restoredTop@sz`, `horzBarState`), as measured in PowerPoint 365 (tasks/doc/m96-pptx-m74-fixes-check.md), and the
//! saving keeps that element as it was.

use ibis_doc_model::{Command, Document, NotesPaneOpening, NotesPaneSave, SlideGuide};
use ibis_pptx::SaveOptions;

fn base() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/pptx-comments-modern.pptx")).unwrap()
}

/// The base file with its `normalViewPr` replaced by `normal` (`None`: the element taken away; `Some("")`: viewProps itself removed is not
/// done here, see `no_view_props`).
fn with_normal(normal: Option<&str>) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(base()).unwrap();
    let xml = String::from_utf8(package.read("ppt/viewProps.xml").unwrap()).unwrap();
    let (start, end) = (xml.find("<p:normalViewPr>").unwrap(), xml.find("</p:normalViewPr>").unwrap() + "</p:normalViewPr>".len());
    let xml = format!("{}{}{}", &xml[..start], normal.unwrap_or(""), &xml[end..]);
    package.set_bytes("ppt/viewProps.xml", xml.into_bytes());
    package.save(Default::default()).unwrap()
}

fn opening(data: Vec<u8>) -> NotesPaneOpening {
    ibis_pptx::open(data).unwrap().presentation.notes_pane
}

fn normal(attrs: &str, top: i64) -> String {
    format!("<p:normalViewPr{attrs}><p:restoredLeft sz=\"15621\"/><p:restoredTop sz=\"{top}\"/></p:normalViewPr>")
}

#[test]
fn the_opening_follows_the_normal_view_as_powerpoint_does() {
    // What PowerPoint writes (94606), 100000 and a missing element: the notes pane is folded (about 5%).
    assert_eq!(opening(base()), NotesPaneOpening::Folded);
    assert_eq!(opening(with_normal(Some(&normal("", 100_000)))), NotesPaneOpening::Folded);
    assert_eq!(opening(with_normal(None)), NotesPaneOpening::Folded);
    // A smaller slide pane leaves the rest to the notes: 70000 -> 30%, 30000 -> 70%.
    assert_eq!(opening(with_normal(Some(&normal("", 70_000)))), NotesPaneOpening::Open(30));
    assert_eq!(opening(with_normal(Some(&normal("", 30_000)))), NotesPaneOpening::Open(70));
    // `restored` is the same as no attribute.
    assert_eq!(opening(with_normal(Some(&normal(" horzBarState=\"restored\"", 94_606)))), NotesPaneOpening::Folded);
    // `minimized`: the largest notes pane (the screen stops it at the slide pane's least height), whatever restoredTop says; `maximized`: no notes pane.
    assert_eq!(opening(with_normal(Some(&normal(" horzBarState=\"minimized\"", 70_000)))), NotesPaneOpening::Open(100));
    assert_eq!(opening(with_normal(Some(&normal(" horzBarState=\"minimized\"", 94_606)))), NotesPaneOpening::Open(100));
    assert_eq!(opening(with_normal(Some(&normal(" horzBarState=\"maximized\"", 70_000)))), NotesPaneOpening::Hidden);
    // The file's share is not limited here (the screen keeps its own least heights): 10000 -> 90, 0 -> 100.
    assert_eq!(opening(with_normal(Some(&normal("", 10_000)))), NotesPaneOpening::Open(90));
    assert_eq!(opening(with_normal(Some(&normal("", 0)))), NotesPaneOpening::Open(100));
    assert_eq!(opening(with_normal(Some(&normal("", 29_000)))), NotesPaneOpening::Open(71));
}

/// The file `data` saved with `save` as what the user did to the notes pane, and its `normalViewPr` as written.
fn saved_with(data: Vec<u8>, save: Option<NotesPaneSave>) -> (String, Vec<u8>) {
    let opened = ibis_pptx::open(data).unwrap();
    let mut presentation = opened.presentation.clone();
    presentation.notes_pane_save = save;
    let saved = opened.source.save(&presentation, SaveOptions::default()).unwrap();
    let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
    let view = String::from_utf8(package.read("ppt/viewProps.xml").unwrap()).unwrap();
    let (start, end) = (view.find("<p:normalViewPr").unwrap(), view.find("</p:normalViewPr>").unwrap());
    (view[start..end].to_string(), saved)
}

#[test]
fn a_moved_or_hidden_notes_pane_is_written_as_powerpoint_writes_it() {
    // Measured in PowerPoint 365 (tasks/doc/m101-softedge-1b-notes-pane.md, (3)).
    let from_maximized = with_normal(Some(&normal(" horzBarState=\"maximized\"", 70_000)));
    // Moved: restoredTop with the share, autoAdjust="0", and horzBarState gone.
    let (view, saved) = saved_with(from_maximized.clone(), Some(NotesPaneSave::Moved(70_055)));
    assert!(view.starts_with("<p:normalViewPr>") && view.contains("<p:restoredTop sz=\"70055\" autoAdjust=\"0\"/>") && view.contains("<p:restoredLeft sz=\"15621\"/>"), "{view}");
    assert_eq!(opening(saved), NotesPaneOpening::Open(30));
    // Hidden, taken to the top, and shown again: horzBarState only, restoredTop as it was.
    let (view, saved) = saved_with(with_normal(Some(&normal("", 70_000))), Some(NotesPaneSave::Maximized(None)));
    assert!(view.starts_with("<p:normalViewPr horzBarState=\"maximized\">") && view.contains("<p:restoredTop sz=\"70000\"/>"), "{view}");
    assert_eq!(opening(saved), NotesPaneOpening::Hidden);
    let (view, saved) = saved_with(with_normal(Some(&normal("", 70_000))), Some(NotesPaneSave::Minimized(None)));
    assert!(view.starts_with("<p:normalViewPr horzBarState=\"minimized\">") && view.contains("<p:restoredTop sz=\"70000\"/>"), "{view}");
    assert_eq!(opening(saved), NotesPaneOpening::Open(100));
    let (view, _) = saved_with(from_maximized, Some(NotesPaneSave::Shown));
    assert!(view.starts_with("<p:normalViewPr>") && view.contains("<p:restoredTop sz=\"70000\"/>"), "{view}");
    // Not touched: the element as it was, with no autoAdjust.
    let (view, _) = saved_with(with_normal(Some(&normal(" horzBarState=\"minimized\"", 70_000))), None);
    assert_eq!(view, "<p:normalViewPr horzBarState=\"minimized\"><p:restoredLeft sz=\"15621\"/><p:restoredTop sz=\"70000\"/>");
}

#[test]
fn a_file_with_no_view_props_opens_folded() {
    let mut package = ibis_ooxml::Package::open(base()).unwrap();
    let rels = String::from_utf8(package.read("ppt/_rels/presentation.xml.rels").unwrap()).unwrap();
    let start = rels.find("<Relationship ").map(|_| rels.match_indices("<Relationship ").map(|(i, _)| i).filter(|&i| rels[i..].split("/>").next().unwrap().contains("viewProps")).next().unwrap()).unwrap();
    let end = start + rels[start..].find("/>").unwrap() + 2;
    package.set_bytes("ppt/_rels/presentation.xml.rels", format!("{}{}", &rels[..start], &rels[end..]).into_bytes());
    assert_eq!(opening(package.save(Default::default()).unwrap()), NotesPaneOpening::Folded);
}

#[test]
fn the_saving_keeps_the_normal_view_and_changed_guides_do_not_touch_it() {
    let data = with_normal(Some(&normal(" horzBarState=\"minimized\"", 70_000)));
    let opened = ibis_pptx::open(data).unwrap();
    // Unchanged, and with the guides changed: the normal view is the same element, and reads the same again.
    let mut document = Document::new(opened.presentation.clone());
    document.execute(vec![Command::SetGuides { slide: None, guides: vec![SlideGuide { id: 1, horizontal: false, pos: 3160, color: None, user_drawn: true }] }]).unwrap();
    for presentation in [&opened.presentation, document.presentation()] {
        let saved = opened.source.save(presentation, SaveOptions::default()).unwrap();
        let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
        let view = String::from_utf8(package.read("ppt/viewProps.xml").unwrap()).unwrap();
        assert!(view.contains(&normal(" horzBarState=\"minimized\"", 70_000)), "{view}");
        assert_eq!(opening(saved), NotesPaneOpening::Open(100));
    }
}

/// The file without `viewProps.xml`.
fn without_view_props() -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(base()).unwrap();
    let rels = String::from_utf8(package.read("ppt/_rels/presentation.xml.rels").unwrap()).unwrap();
    let start = rels.match_indices("<Relationship ").map(|(i, _)| i).find(|&i| rels[i..].split("/>").next().unwrap().contains("viewProps")).unwrap();
    let end = start + rels[start..].find("/>").unwrap() + 2;
    package.set_bytes("ppt/_rels/presentation.xml.rels", format!("{}{}", &rels[..start], &rels[end..]).into_bytes());
    package.remove("ppt/viewProps.xml");
    package.remove_override("/ppt/viewProps.xml").ok();
    package.save(Default::default()).unwrap()
}

/// #M114 (b): hidden or taken to the top after the border was moved, `restoredTop` is the border before (moved: with `autoAdjust="0"`).
#[test]
fn hidden_after_a_move_keeps_the_moved_border_as_restored_top() {
    for (save, state) in [(NotesPaneSave::Maximized(Some(50_092)), "maximized"), (NotesPaneSave::Minimized(Some(50_092)), "minimized")] {
        let (view, saved) = saved_with(with_normal(Some(&normal("", 70_000))), Some(save));
        assert!(view.starts_with(&format!("<p:normalViewPr horzBarState=\"{state}\">")) && view.contains("<p:restoredTop sz=\"50092\" autoAdjust=\"0\"/>"), "{view}");
        assert_eq!(opening(saved), if state == "maximized" { NotesPaneOpening::Hidden } else { NotesPaneOpening::Open(100) });
    }
}

/// #M114 (c): PowerPoint makes `viewProps.xml` and `normalViewPr` at every save: a file without them gets them, with what the user did to the pane.
#[test]
fn a_file_without_the_normal_view_gets_it_at_saving() {
    for none in [with_normal(None), without_view_props()] {
        // Nothing done to the pane but the file saved with a change: PowerPoint's defaults.
        let opened = ibis_pptx::open(none.clone()).unwrap();
        let mut document = Document::new(opened.presentation.clone());
        document.execute(vec![Command::SetFirstSlideNumber { number: 3 }]).unwrap();
        let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
        let package = ibis_ooxml::Package::open(saved.clone()).unwrap();
        let view = String::from_utf8(package.read("ppt/viewProps.xml").unwrap()).unwrap();
        assert!(view.contains("<p:normalViewPr><p:restoredLeft sz=\"15620\"/><p:restoredTop sz=\"94660\"/></p:normalViewPr>"), "{view}");
        assert!(view.find("<p:normalViewPr").unwrap() < view.find("<p:slideViewPr").unwrap(), "{view}");
        assert_eq!(opening(saved), NotesPaneOpening::Folded);
        // Moved, hidden, taken to the top.
        let (view, saved) = saved_with(none.clone(), Some(NotesPaneSave::Moved(50_092)));
        assert!(view.contains("<p:restoredTop sz=\"50092\" autoAdjust=\"0\"/>"), "{view}");
        assert_eq!(opening(saved), NotesPaneOpening::Open(50));
        let (view, saved) = saved_with(none.clone(), Some(NotesPaneSave::Maximized(None)));
        assert!(view.starts_with("<p:normalViewPr horzBarState=\"maximized\">") && view.contains("<p:restoredTop sz=\"94660\"/>"), "{view}");
        assert_eq!(opening(saved), NotesPaneOpening::Hidden);
        let (view, _) = saved_with(none, Some(NotesPaneSave::Minimized(None)));
        assert!(view.starts_with("<p:normalViewPr horzBarState=\"minimized\">"), "{view}");
    }
}

/// An unchanged file stays byte for byte as it was, also one without the normal view.
#[test]
fn an_unchanged_file_without_the_normal_view_is_not_rewritten() {
    let data = without_view_props();
    let opened = ibis_pptx::open(data.clone()).unwrap();
    let saved = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
    assert_eq!(saved, data);
}
