// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! グラフのデータの編集 with an embedded workbook: a changed value and a
//! series' name are written into the cells the chart's references name
//! (`test_files::pptx_with_embedded_chart_data` gives the benchmark's
//! charts each an empty workbook).

use ibis_doc_model::{CellValue, Command, Document, Number, ShapeKind};

#[test]
fn edited_data_reaches_the_embedded_workbook() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/benchmark/earnings-presentation.pptx");
    let data = ibis_formats::test_files::pptx_with_embedded_chart_data(&std::fs::read(path).unwrap());
    let opened = ibis_formats::open(data).unwrap();
    let mut doc = Document::new(opened.presentation.clone());
    let (slide, shape, mut chart) = doc
        .presentation()
        .slides
        .iter()
        .find_map(|sl| sl.shapes.iter().find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) if c.groups.first().and_then(|g| g.series.first()).is_some_and(|s| s.values.len() > 1) => Some((sl.id, sh.id, (**c).clone())),
            _ => None,
        }))
        .expect("a chart");
    let series = &mut chart.groups[0].series[0];
    series.values[1] = Some(Number(777.25));
    series.name = "新しい名前".into();
    doc.execute(vec![Command::SetShapeKind { slide, shape, kind: Box::new(ShapeKind::Chart(Box::new(chart))) }]).unwrap();
    let saved = opened.source.save(doc.presentation(), ibis_formats::SaveOptions::default()).unwrap();
    // The chart reads back with them.
    let again = ibis_formats::read(&saved).unwrap();
    let shape_back = again.slides.iter().flat_map(|s| &s.shapes).find(|s| s.id == shape).unwrap();
    let ShapeKind::Chart(c) = &shape_back.kind else { panic!("a chart") };
    assert_eq!((c.groups[0].series[0].values[1], c.groups[0].series[0].name.as_str()), (Some(Number(777.25)), "新しい名前"));
    // And the workbooks have the cells (the first chart's series name is
    // at the cell its c:tx names; its value one row below the first).
    let package = ibis_ooxml::Package::open(saved).unwrap();
    let mut found = (false, false);
    for name in package.names().filter(|n| n.starts_with("ppt/embeddings/")).map(str::to_string).collect::<Vec<_>>() {
        let book = ibis_formats::read(&package.read(&name).unwrap()).unwrap();
        let ShapeKind::Sheet(sheet) = &book.slides[0].shapes[0].kind else { continue };
        for row in 0..60 {
            for column in 0..10 {
                match sheet.cell(row, column).map(|c| &c.value) {
                    Some(CellValue::Number(v)) if *v == 777.25 => found.0 = true,
                    Some(CellValue::Text(t)) if t == "新しい名前" => found.1 = true,
                    _ => {}
                }
            }
        }
    }
    assert_eq!(found, (true, true), "the value and the name are in a workbook");
}
