// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use ibis_doc_model::{EmuRect, Shape, ShapeKind};
#[test]
fn office_native_shape_clipboard_retains_geometry_and_text() {
    let opened =
        ibis_pptx::open(ibis_pptx::blank_file(ibis_pptx::Kind::Presentation).unwrap()).unwrap();
    let mut p = opened.presentation;
    let id = p.new_shape_id();
    let shape = Shape::text_box(
        id,
        EmuRect {
            x: 1234,
            y: 5678,
            width: 4000000,
            height: 1000000,
        },
        "Editable native text",
    );
    let bytes = ibis_pptx::shape_clipboard::write_gvml(&p, &[shape]).unwrap();
    if let Ok(path) = std::env::var("IBIS_GVML_OUT") {
        std::fs::write(path, &bytes).unwrap();
    }
    let restored = ibis_pptx::shape_clipboard::read_gvml(bytes).unwrap();
    let shapes = &restored.presentation.slides[0].shapes;
    assert_eq!(shapes.len(), 1);
    assert_eq!(
        shapes[0].text_body().unwrap().text(),
        "Editable native text"
    );
    assert!(!matches!(shapes[0].kind, ShapeKind::Picture(_)));
    assert_eq!(shapes[0].frame.width, 4000000);
}
#[test]
fn office_native_clipboard_fixture_when_provided() {
    let Ok(path) = std::env::var("IBIS_GVML_FILE") else {
        return;
    };
    let opened = ibis_pptx::shape_clipboard::read_gvml(std::fs::read(path).unwrap()).unwrap();
    assert!(!opened.presentation.slides[0].shapes.is_empty());
    assert!(
        opened.presentation.slides[0]
            .shapes
            .iter()
            .any(|s| s.text_body().is_some_and(|t| !t.text().is_empty()))
    );
    assert!(
        opened.presentation.slides[0]
            .shapes
            .iter()
            .all(|s| !matches!(s.kind, ShapeKind::Picture(_)))
    );
}
