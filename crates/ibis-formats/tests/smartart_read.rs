// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! SmartArt (図表): a graphic's shapes read from the drawing PowerPoint
//! keeps of it (`dsp:drawing`), placed in its frame and attached to it.
//! The file PowerPoint saved is outside git (it holds Office's layout
//! definitions): set IBIS_PSMT to the coverage suite PSMT's office.pptx.

use ibis_doc_model::{Placeholder, ShapeKind};

#[test]
#[ignore = "Requires the PSMT file PowerPoint saved; set IBIS_PSMT"]
fn smartart_shapes_are_read_from_its_drawing() {
    let data = std::fs::read(std::env::var("IBIS_PSMT").unwrap()).unwrap();
    let p = ibis_formats::read(&data).unwrap();
    let mut graphics = 0;
    for slide in &p.slides {
        for shape in slide.shapes.iter().filter(|s| matches!(s.kind, ShapeKind::Placeholder(Placeholder::Diagram))) {
            graphics += 1;
            let parts = slide.attached.get(&shape.id).cloned().unwrap_or_default();
            assert!(!parts.is_empty(), "the graphic's shapes are attached to it");
            for id in parts {
                let part = slide.shape(id).unwrap();
                // Inside the graphic's frame.
                let (f, g) = (shape.frame, part.frame);
                assert!(g.x >= f.x && g.y >= f.y && g.right() <= f.right() + 1 && g.bottom() <= f.bottom() + 1, "{g:?} in {f:?}");
            }
        }
    }
    assert_eq!(graphics, 7);
    // Their text: the first graphic's boxes.
    let first = p.slides[0].shapes.iter().find(|s| matches!(s.kind, ShapeKind::Placeholder(Placeholder::Diagram))).unwrap();
    let texts: Vec<String> = p.slides[0].attached[&first.id].iter().filter_map(|id| p.slides[0].shape(*id)?.text_body().map(|b| b.text())).collect();
    assert_eq!(texts, vec!["計画", "設計", "実装", "試験", "公開"]);
}

#[test]
fn smartart_handwritten_pptx_read() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/pptx-smartart.pptx");
    if !path.exists() {
        return;
    }
    let data = std::fs::read(path).unwrap();
    let p = ibis_formats::read(&data).unwrap();
    let first = p.slides[0].shapes.iter().find(|s| matches!(s.kind, ShapeKind::Placeholder(Placeholder::Diagram))).expect("diagram shape");
    let parts = p.slides[0].attached.get(&first.id).cloned().unwrap_or_default();
    assert_eq!(parts.len(), 2, "attached shapes");
    let texts: Vec<String> = parts.iter().filter_map(|id| p.slides[0].shape(*id)?.text_body().map(|b| b.text())).collect();
    assert_eq!(texts, vec!["項目1", "項目2"]);
}
