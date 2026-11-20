// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Saving through the model (G1 of `tasks/doc/editing-plan.md`) and the
//! first edit saved end to end.
//!
//! Every file of the corpus (`test/benchmark`, `test/editing`) is opened as
//! the app opens it and saved: with no edit it is the same file byte for
//! byte, and with every XML part written again from the tree it means the
//! same (no difference in normalized XML).

use ibis_doc_model::{Command, Document, ShapeKind};
use ibis_ooxml::canon::{PartDiff, diff_packages};
use ibis_formats::SaveOptions;

fn corpus() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test");
    let mut files = Vec::new();
    for dir in ["benchmark", "editing"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            // (A password-protected file is opened with its password: `ibis-ooxml`'s tests.)
            if matches!(ext.as_str(), "pptx" | "xlsx" | "docx") && !std::fs::read(&path).is_ok_and(|d| ibis_formats::is_encrypted(&d)) {
                files.push(path);
            }
        }
    }
    files.sort();
    assert!(!files.is_empty());
    files
}

#[test]
fn save_without_edits_is_the_same_file() {
    for path in corpus() {
        let data = std::fs::read(&path).unwrap();
        let opened = ibis_formats::open(data.clone()).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let saved = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
        assert!(saved == data, "{} changed", path.display());
    }
}

#[test]
fn rewriting_every_part_keeps_the_meaning() {
    for path in corpus() {
        let data = std::fs::read(&path).unwrap();
        let opened = ibis_formats::open(data.clone()).unwrap();
        let saved = opened.source.save(&opened.presentation, SaveOptions { regenerate_xml: true }).unwrap();
        let report = diff_packages(&data, &saved, &[]).unwrap();
        assert!(report.is_same(), "{}: {}", path.display(), report.to_json());
        // And it reads back as the same model.
        let again = ibis_formats::read(&saved).unwrap();
        let mut expected = opened.presentation.clone();
        if let (Some(a), Some(e)) = (again.flow.as_ref(), expected.flow.as_mut()) {
            std::sync::Arc::make_mut(e).file_size = a.file_size;
        }
        for (sl_a, sl_e) in again.slides.iter().zip(&mut expected.slides) {
            for (sh_a, sh_e) in sl_a.shapes.iter().zip(&mut sl_e.shapes) {
                if let (ShapeKind::Page { document: doc_a, .. }, ShapeKind::Page { document: doc_e, .. }) = (&sh_a.kind, &mut sh_e.kind) {
                    std::sync::Arc::make_mut(doc_e).file_size = doc_a.file_size;
                }
            }
        }
        assert!(again == expected, "{} reads back differently", path.display());
    }
}

/// Change one character of a text box's text, save, and read it back: the
/// slide part is the only part that differs, the text is the edited one,
/// and undoing the edit saves the original file again.
#[test]
fn a_text_edit_is_saved_end_to_end() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/benchmark/earnings-presentation.pptx");
    let data = std::fs::read(&path).unwrap();
    let opened = ibis_formats::open(data.clone()).unwrap();
    // The first shape on a slide with some text.
    let (slide_index, shape_index) = opened
        .presentation
        .slides
        .iter()
        .enumerate()
        .find_map(|(i, s)| s.shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::TextBox(_) | ShapeKind::Auto(_)) && sh.text_body().is_some_and(|b| b.text().chars().count() > 2)).map(|j| (i, j)))
        .expect("a shape with text");
    let slide = &opened.presentation.slides[slide_index];
    let shape = &slide.shapes[shape_index];
    let (slide_id, shape_id) = (slide.id, shape.id);
    let old_text = shape.text_body().unwrap().text();
    let first: char = old_text.chars().next().unwrap();
    let replacement = if first == 'Z' { "Y" } else { "Z" };

    let mut document = Document::new(opened.presentation.clone());
    document.execute(vec![Command::ReplaceTextRange { slide: slide_id, shape: shape_id, at: 0, remove: 1, insert: replacement.into() }]).unwrap();
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();

    let report = diff_packages(&data, &saved, &[]).unwrap();
    assert_eq!(report.differences.len(), 1, "{}", report.to_json());
    let (part, diff) = &report.differences[0];
    assert!(part.starts_with("ppt/slides/slide"), "{part}");
    assert!(matches!(diff, PartDiff::Xml { differing: 1, .. }), "{diff:?}");

    let reread = ibis_formats::read(&saved).unwrap();
    let text = reread.slides[slide_index].shapes[shape_index].text_body().unwrap().text();
    let expected: String = std::iter::once(replacement.to_string()).chain(old_text.chars().skip(1).map(String::from)).collect();
    assert_eq!(text, expected);
    // Everything else reads back as it was.
    let mut original = opened.presentation.clone();
    let mut edited = reread.clone();
    original.slides[slide_index].shapes.remove(shape_index);
    edited.slides[slide_index].shapes.remove(shape_index);
    assert!(original == edited);

    assert!(document.undo());
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();
    assert!(saved == data);
}

#[test]
fn a_slide_size_edit_is_saved_end_to_end() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/benchmark/earnings-presentation.pptx");
    let opened = ibis_formats::open(std::fs::read(&path).unwrap()).unwrap();
    // Slide size saving is supported; other presentation content stays intact.
    let mut presentation = opened.presentation.clone();
    presentation.slide_size.width += 1000;
    let saved = opened.source.save(&presentation, SaveOptions::default()).unwrap();
    let reread = ibis_formats::read(&saved).unwrap();
    assert_eq!(reread.slide_size, presentation.slide_size);
    assert_eq!(reread.slides, opened.presentation.slides);
    let report = diff_packages(&std::fs::read(&path).unwrap(), &saved, &[]).unwrap();
    assert_eq!(report.differences.len(), 1, "{}", report.to_json());
    assert_eq!(report.differences[0].0, "ppt/presentation.xml");
}
