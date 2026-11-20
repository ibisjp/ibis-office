// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Reads a user's private presentation (a real presentation file, which is
//! not distributed with the source), named by `IBIS_PRIVATE_PPTX`, when it is
//! set.

use ibis_doc_model::{Placeholder, ShapeKind};


#[test]
fn reads_test_asset_pptx() {
    let Some(data) = std::env::var_os("IBIS_PRIVATE_PPTX").and_then(|p| std::fs::read(p).ok()) else {
        eprintln!("skipped: IBIS_PRIVATE_PPTX is not set or not found");
        return;
    };
    let p = ibis_formats::read(&data).expect("the test data reads");
    assert_eq!(p.slides.len(), 31);

    let mut counts = std::collections::BTreeMap::<&str, usize>::new();
    for slide in &p.slides {
        for shape in slide.shapes.iter().chain(&slide.backdrop) {
            let kind = match &shape.kind {
                ShapeKind::TextBox(_) => "text box",
                ShapeKind::Auto(a) if a.text.is_some() => "shape with text",
                ShapeKind::Auto(_) => "shape",
                ShapeKind::Picture(_) => "picture",
                ShapeKind::Line(_) => "line",
                ShapeKind::Chart(_) => "chart",
                ShapeKind::Placeholder(Placeholder::Chart) => "unread chart",
                ShapeKind::Table(_) => "table",
                ShapeKind::Placeholder(Placeholder::Table) => "unread table",
                ShapeKind::Placeholder(Placeholder::Picture(_)) => "undrawn picture",
                ShapeKind::Placeholder(Placeholder::Diagram) => "smartart",
                ShapeKind::Sheet(_) => "sheet",
                ShapeKind::Page { .. } => "page",
                ShapeKind::Slicer(_) => "slicer",
                ShapeKind::Control(_) => "control",
                ShapeKind::Timeline(_) => "timeline",
            };
            *counts.entry(kind).or_default() += 1;
        }
    }
    eprintln!("{counts:#?}");
    assert_eq!(counts.get("chart"), Some(&32));
    assert_eq!(counts.get("table"), Some(&3));

    // Slide 2's table: 6 rows of 5 cells, with text from the cells.
    let table = p.slides[1]
        .shapes
        .iter()
        .find_map(|s| match &s.kind {
            ShapeKind::Table(t) => Some(t),
            _ => None,
        })
        .expect("slide 2 has a table");
    assert_eq!(table.columns.len(), 5);
    assert_eq!(table.rows.len(), 6);
    assert!(table.rows.iter().all(|r| r.cells.len() == 5));
    assert_eq!(table.rows[1].cells[0].text.text(), "サブスクリプション課金売上");
    assert!(counts.get("picture").copied().unwrap_or(0) >= 30);

    // Slide 4's title comes from its placeholder and the master's title style.
    let title = p.slides[3]
        .shapes
        .iter()
        .filter_map(|s| s.text_body())
        .map(|b| b.text())
        .find(|t| t.contains("収益基盤"))
        .expect("slide 4 title");
    assert!(title.starts_with("売上・利益共に"), "{title}");
}
