// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A changed cell that a sheet chart reads rewrites the chart's caches only:
//! the series' own XML (no `c:spPr` in the file, the theme's colours) stays as it
//! was (scenario xsc37: the saved chart had gained a fixed-colour `c:spPr` in each
//! series, placed after `c:invertIfNegative` and with `a:ln` before the fill).
use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, Document, SheetCommand, ShapeKind};
use ibis_ooxml::{Package, SaveOptions};

const CHART: &str = "xl/charts/chart1.xml";

fn sales() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap()
}

/// The chart part with each cached value (`c:v` text) blanked: what must not change.
fn without_values(xml: &[u8]) -> String {
    let text = String::from_utf8(xml.to_vec()).unwrap();
    let mut out = String::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find("<c:v>") {
        out.push_str(&rest[..at + 5]);
        rest = &rest[at + 5..];
        let end = rest.find("</c:v>").unwrap();
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[test]
fn changed_cell_rewrites_the_chart_cache_only() {
    let data = sales();
    let original = Package::open(data.clone()).unwrap().read(CHART).unwrap();
    assert!(!String::from_utf8_lossy(&original).contains("spPr><a:solidFill"), "the fixture's series have no fill of their own");
    let opened = ibis_pptx::open(data).unwrap();
    let mut doc = Document::new(opened.presentation.clone());
    let slide = doc.presentation().slides.iter().find(|s| s.shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Chart(_)))).unwrap().id;
    // B4 (北町店, 4月): 1,250,000 → 1,260,000, as in xsc37.
    doc.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![CellEdit { row: 3, column: 1, input: CellInput::Value(CellValue::Number(1_260_000.0)) }] })]).unwrap();
    let saved = opened.source.save(doc.presentation(), SaveOptions::default()).unwrap();
    let chart = Package::open(saved).unwrap().read(CHART).unwrap();
    let text = String::from_utf8_lossy(&chart);
    assert!(text.contains("<c:v>1260000</c:v>"), "the cache holds the new number: {text}");
    assert!(!text.contains("<c:v>1250000</c:v>"), "the old number is gone: {text}");
    assert_eq!(without_values(&chart), without_values(&original), "only cached values change");
}
