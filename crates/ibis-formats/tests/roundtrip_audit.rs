// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 未対応の機能の保存の評価 (PPTX-AUDIT-011): every presentation of the public test files is opened, one shape of its
//! first slide with a frame is moved (as the editor moves one: its own place), and it is saved. What the program does not understand (SmartArt, charts, media,
//! embeddings, custom XML, extension lists, other masters…) has to come through: every part still there, and only the
//! part of the moved shape's slide (with the content types and the document properties) written again.

use ibis_doc_model::{Command, EmuRect};
use ibis_ooxml::Package;
use ibis_formats::SaveOptions;

fn files() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test");
    let mut found = Vec::new();
    for dir in ["editing", "benchmark"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "pptx") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn what_is_not_understood_comes_through_an_edit() {
    let mut checked = 0;
    for path in files() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let data = std::fs::read(&path).unwrap();
        let Ok(opened) = ibis_formats::open(data.clone()) else { continue };
        let slide = &opened.presentation.slides[0];
        let Some(shape) = slide.shapes.iter().find(|s| s.frame.width > 0) else { continue };
        let moved = EmuRect::new(shape.frame.x + 12_700, shape.frame.y + 12_700, shape.frame.width, shape.frame.height);
        let mut edited = opened.presentation.clone();
        edited.apply(Command::SetFrame { slide: slide.id, frames: vec![(shape.id, moved)], with_attached: true }).unwrap();
        let saved = opened.source.save(&edited, SaveOptions::default()).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let reread = ibis_formats::open(saved.clone()).unwrap();
        let frame_after = reread.presentation.slides[0].shapes.iter().find(|s| s.id == shape.id).map(|s| s.frame);
        assert_eq!(frame_after, Some(moved), "{name}: the move is in the saved file");
        let (before, after) = (Package::open(data).unwrap(), Package::open(saved).unwrap());
        let lost: Vec<&str> = before.names().filter(|n| !after.contains(n)).collect();
        assert!(lost.is_empty(), "{name}: parts lost in the edit: {lost:?}");
        let changed: Vec<&str> = before.names().filter(|n| before.read(n).ok() != after.read(n).ok()).collect();
        // Only the first slide, the content types and the properties are written again. `ppt/viewProps.xml` too, and only to get a `normalViewPr`
        // when the file has none: PowerPoint 365 makes it at every save, with nothing done to the notes pane (tasks/doc/m114-pptx-notes-pane-save.md, (c)).
        let unexpected: Vec<&&str> = changed.iter().filter(|n| !(n.starts_with("ppt/slides/slide") && !n.contains("_rels") || **n == "[Content_Types].xml" || n.starts_with("docProps/") || **n == "ppt/viewProps.xml")).collect();
        assert!(changed.contains(&"ppt/slides/slide1.xml"), "{name}: the slide of the moved shape is written again");
        assert!(unexpected.is_empty(), "{name}: parts changed that no edit touched: {unexpected:?}");
        if changed.contains(&"ppt/viewProps.xml") {
            let view = String::from_utf8(after.read("ppt/viewProps.xml").unwrap().to_vec()).unwrap();
            let had = String::from_utf8(before.read("ppt/viewProps.xml").unwrap().to_vec()).unwrap();
            assert!(!had.contains("normalViewPr") && view.contains("<p:normalViewPr><p:restoredLeft sz=\"15620\"/><p:restoredTop sz=\"94660\"/></p:normalViewPr>"), "{name}: viewProps changed beyond the added normalViewPr: {view}");
        }
        checked += 1;
    }
    assert!(checked >= 5, "the public test files were checked: {checked}");
}
