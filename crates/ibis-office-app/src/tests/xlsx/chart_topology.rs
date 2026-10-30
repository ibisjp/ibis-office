// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use crate::pptx::chart_panel::ChartPreset;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{Chart, ChartType, ShapeKind};
fn chart(p: &ibis_doc_model::Presentation) -> &Chart {
    p.slides[0]
        .shapes
        .iter()
        .find_map(|s| {
            if let ShapeKind::Chart(c) = &s.kind {
                Some(c.as_ref())
            } else {
                None
            }
        })
        .unwrap()
}
pub(crate) fn mixed_secondary_roundtrip() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let i = s
        .with(|e, _| {
            e.slide()
                .shapes
                .iter()
                .position(|s| matches!(s.kind, ShapeKind::Chart(_)))
        })
        .unwrap();
    let at = s.shape_position(i);
    s.click(at);
    s.menu("xlsx.chart-format");
    let before = s.with(|e, _| e.sheet_chart().unwrap().1.clone());
    let name = before.groups[0].series[0].name.clone();
    let other = before
        .groups
        .iter()
        .flat_map(|g| &g.series)
        .skip(1)
        .cloned()
        .collect::<Vec<_>>();
    s.update(|e, w, cx| {
        e.open_chart_series_editor(Some(0), w, cx);
        e.set_chart_ui_type(ChartPreset::Line, w, cx);
        e.wb_mut()
            .chart_edit
            .as_mut()
            .unwrap()
            .series_editor
            .as_mut()
            .unwrap()
            .secondary = true;
        e.apply_chart_series_editor(w, cx);
    });
    let after = s.with(|e, _| e.sheet_chart().unwrap().1.clone());
    assert_eq!(after.groups.len(), 2);
    let line = after
        .groups
        .iter()
        .find(|g| g.kind == ChartType::Line)
        .expect("one line group");
    assert_eq!(line.series[0].name, name);
    let axis = after
        .axes
        .iter()
        .find(|a| line.axes.contains(&a.id) && a.kind == ibis_doc_model::chart::AxisKind::Value)
        .unwrap();
    assert_eq!(axis.side, ibis_doc_model::chart::Side::Right);
    let unchanged = after
        .groups
        .iter()
        .filter(|g| g.kind != ChartType::Line)
        .flat_map(|g| &g.series)
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(unchanged, other);
    s.save_as("chart-series-mixed-secondary.xlsx");
    let reopened = s.reopen();
    let saved = chart(&reopened);
    assert_eq!(saved.groups.len(), 2);
    let line = saved
        .groups
        .iter()
        .find(|g| g.kind == ChartType::Line)
        .expect("saved line group");
    assert_eq!(line.series[0].name, name);
    let axis = saved
        .axes
        .iter()
        .find(|a| line.axes.contains(&a.id) && a.kind == ibis_doc_model::chart::AxisKind::Value)
        .unwrap();
    assert_eq!(axis.side, ibis_doc_model::chart::Side::Right);
    let unchanged = saved
        .groups
        .iter()
        .filter(|g| g.kind != ChartType::Line)
        .flat_map(|g| &g.series)
        .collect::<Vec<_>>();
    assert_eq!(
        unchanged.iter().map(|s| &s.name).collect::<Vec<_>>(),
        other.iter().map(|s| &s.name).collect::<Vec<_>>()
    );
    assert_eq!(
        unchanged.iter().map(|s| &s.values).collect::<Vec<_>>(),
        other.iter().map(|s| &s.values).collect::<Vec<_>>()
    );
    for _ in 0..16 {if s.with(|e,_|e.sheet_chart().unwrap().1.clone())==before{break;}s.key("cmd-z");}
    assert_eq!(s.with(|e, _| e.sheet_chart().unwrap().1.clone()), before);
    for _ in 0..16 {if s.with(|e,_|e.sheet_chart().unwrap().1.clone())==after{break;}s.key("cmd-shift-z");}
    assert_eq!(s.with(|e, _| e.sheet_chart().unwrap().1.clone()), after);
}
