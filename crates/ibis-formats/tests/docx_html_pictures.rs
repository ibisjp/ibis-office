// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #208: a document's HTML page has the pictures the caller makes of its charts and SmartArt where their runs are (`docx_convert::export_html_with`).

use ibis_doc_model::{Block, EmuRect, FlowRunKind, Shape, ShapeId, ShapeKind};

#[test]
fn a_chart_of_a_document_is_a_picture_in_its_page() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/docx-report.docx");
    let opened = ibis_formats::open(std::fs::read(path).unwrap()).unwrap();
    let mut doc = opened.presentation.flow.clone().expect("a document");
    let doc = std::sync::Arc::make_mut(&mut doc);
    let chart = Shape::new(ShapeId(9001), EmuRect::new(0, 0, 3_000_000, 2_000_000), ShapeKind::Chart(Box::new(ibis_doc_model::Chart::empty())));
    let Some(Block::Paragraph(p)) = doc.body.iter_mut().find(|b| matches!(b, Block::Paragraph(p) if !p.runs.is_empty())) else { panic!("a paragraph") };
    let mut run = p.runs[0].clone();
    run.kind = FlowRunKind::Shape(Box::new(chart));
    p.runs.insert(0, run);
    let without = ibis_formats::docx_convert::export_html(doc);
    assert!(!without.contains("<img"), "no picture without a rasteriser");
    let pictures = ibis_formats::docx_convert::shapes_needing_pictures(doc);
    assert_eq!(pictures.len(), 1, "the chart is the one shape the page cannot draw");
    let with = ibis_formats::docx_convert::export_html_with(doc, &|_, _| Some(vec![0x89, b'P', b'N', b'G']));
    assert!(with.contains("<img alt=\"Chart\" src=\"data:image/png;base64,iVBORw==\">"), "the picture is in the page: {}", &with[..with.len().min(1500)]);
}
