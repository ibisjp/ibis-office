// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use super::*;
use ibis_doc_model::ShapeKind;
fn fixture() -> (
    XmlDocument,
    Chart,
    Vec<super::super::pptx_chart_data::SeriesRefs>,
) {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/editing/xlsx-sales.xlsx"
    ))
    .unwrap();
    let opened = crate::open(bytes.clone()).unwrap();
    let (shape, chart) = opened.presentation.slides[0]
        .shapes
        .iter()
        .find_map(|s| {
            if let ShapeKind::Chart(c) = &s.kind {
                Some((s.id, c.as_ref().clone()))
            } else {
                None
            }
        })
        .unwrap();
    let refs = opened.source.sheet_chart_refs(shape, &chart).unwrap();
    let package = ibis_ooxml::Package::open(bytes).unwrap();
    let part = package
        .names()
        .find(|s| s.starts_with("xl/charts/chart") && s.ends_with(".xml"))
        .unwrap();
    (package.xml(part).unwrap(), chart, refs)
}
#[test]
fn duplicate_series_names_preserve_unknown_metadata_by_reference() {
    let (mut doc, mut chart, mut refs) = fixture();
    let root = doc.root_element().unwrap();
    doc.set_attr(root, "xmlns:ibisTest", "urn:ibis:test:chart-preservation");
    let plot = doc.path(root, &["chart", "plotArea"]).unwrap();
    let series = doc
        .descendants(plot)
        .into_iter()
        .filter(|&s| doc.local_name(s) == "ser")
        .collect::<Vec<_>>();
    assert!(series.len() > 1);
    for (i, &ser) in series.iter().enumerate() {
        if let Some(tx) = doc.child(ser, "tx") {
            for v in doc
                .descendants(tx)
                .into_iter()
                .filter(|&n| doc.local_name(n) == "v")
                .collect::<Vec<_>>()
            {
                doc.set_text(v, "Same");
            }
        }
        let ext = doc
            .child(ser, "extLst")
            .unwrap_or_else(|| doc.append_element(ser, "c:extLst"));
        let unknown = doc.append_element(ext, "ibisTest:unknown");
        doc.set_attr(unknown, "identity", &i.to_string());
    }
    for s in chart.groups.iter_mut().flat_map(|g| &mut g.series) {
        s.name = "Same".into();
    }
    let first = chart.groups[0].series.remove(0);
    chart.groups[0].series.push(first);
    let r = refs.remove(0);
    refs.push(r);
    sync_series(&mut doc, &chart, &refs).unwrap();
    let actual = doc
        .descendants(plot)
        .into_iter()
        .filter(|&s| doc.local_name(s) == "ser")
        .map(|ser| {
            doc.descendants(ser)
                .into_iter()
                .find(|&n| doc.local_name(n) == "unknown")
                .and_then(|n| doc.attr(n, "identity"))
                .unwrap()
                .parse::<usize>()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let mut expected = (0..series.len()).collect::<Vec<_>>();
    expected.rotate_left(1);
    assert_eq!(
        actual, expected,
        "equal display names must not swap unmodeled XML between distinct source references"
    );
}

#[test]
fn ambiguous_identical_series_do_not_mix_unknown_metadata() {
    let (mut doc, mut chart, mut refs) = fixture();
    let root = doc.root_element().unwrap();
    let plot = doc.path(root, &["chart", "plotArea"]).unwrap();
    let series = doc
        .descendants(plot)
        .into_iter()
        .filter(|&n| doc.local_name(n) == "ser")
        .collect::<Vec<_>>();
    assert!(series.len() > 1);
    let first_ref = refs[0].values.clone().unwrap();
    for (&ser, r) in series.iter().zip(&mut refs) {
        r.values = Some(first_ref.clone());
        let val = doc.child(ser, "val").unwrap();
        for f in doc
            .descendants(val)
            .into_iter()
            .filter(|&n| doc.local_name(n) == "f")
            .collect::<Vec<_>>()
        {
            doc.set_text(f, &first_ref);
        }
        if let Some(tx) = doc.child(ser, "tx") {
            for v in doc
                .descendants(tx)
                .into_iter()
                .filter(|&n| doc.local_name(n) == "v")
                .collect::<Vec<_>>()
            {
                doc.set_text(v, "Same");
            }
        }
    }
    for s in chart.groups.iter_mut().flat_map(|g| &mut g.series) {
        s.name = "Same".into();
    }
    let mut split = chart.groups[0].clone();
    split.kind = ChartType::Line;
    split.series = vec![chart.groups[0].series.remove(0)];
    chart.groups.push(split);
    assert!(
        sync_series(&mut doc, &chart, &refs).is_err(),
        "indistinguishable original identities must not be assigned arbitrary extension metadata"
    );
}

#[test]
fn split_group_adds_secondary_axis_and_keeps_chart_settings_supported() {
    let (mut doc, old, mut refs) = fixture();
    let mut new = old.clone();
    let mut line = new.groups[0].clone();
    line.kind = ChartType::Line;
    line.series = vec![new.groups[0].series.remove(0)];
    let mut axis = new
        .axes
        .iter()
        .find(|a| a.kind == ibis_doc_model::chart::AxisKind::Value)
        .unwrap()
        .clone();
    axis.id = new.axes.iter().map(|a| a.id).max().unwrap() + 1;
    axis.source_id = Some(axis.id);
    axis.side = Side::Right;
    *line.axes.last_mut().unwrap() = axis.id;
    new.axes.push(axis);
    new.groups.push(line);
    let first = refs.remove(0);
    refs.push(first);
    sync_series(&mut doc, &new, &refs).unwrap();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/editing/xlsx-sales.xlsx"
    ))
    .unwrap();
    let mut package = ibis_ooxml::Package::open(bytes).unwrap();
    package.set_xml("xl/charts/chart1.xml", doc);
    super::super::xlsx::write_chart_settings(&mut package, "xl/charts/chart1.xml", &old, &new)
        .unwrap();
    let doc = package.xml("xl/charts/chart1.xml").unwrap();
    let plot = doc
        .path(doc.root_element().unwrap(), &["chart", "plotArea"])
        .unwrap();
    let right = doc
        .elements(plot)
        .find(|&n| {
            doc.local_name(n) == "valAx"
                && doc.child(n, "axPos").and_then(|p| doc.attr(p, "val")) == Some("r")
        })
        .unwrap();
    assert_eq!(
        doc.child(right, "axId")
            .and_then(|p| doc.attr(p, "val"))
            .unwrap(),
        new.axes.last().unwrap().id.to_string()
    );
    assert!(doc.child(plot, "lineChart").is_some());
    assert!(doc.child(plot, "barChart").is_some());
}
