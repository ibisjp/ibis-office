// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Semantic limits of a single series' chart type. Whole-chart conversion remains independent.
use crate::pptx::chart_panel::ChartPreset;
use ibis_doc_model::{Chart, ChartType};
#[derive(PartialEq, Eq)]
enum Family {
    Axes,
    Round,
    Scatter,
    Unsupported,
}
fn family(kind: ChartType) -> Family {
    match kind {
        ChartType::Bar { .. } | ChartType::Line | ChartType::Area => Family::Axes,
        ChartType::Pie { .. } | ChartType::Doughnut { .. } => Family::Round,
        ChartType::Scatter { .. } | ChartType::Bubble { .. } => Family::Scatter,
        _ => Family::Unsupported,
    }
}
fn horizontal(kind: ChartType) -> bool {
    matches!(
        kind,
        ChartType::Bar {
            horizontal: true,
            ..
        }
    )
}
/// `index=None` adds a series, using the first group's family and axis assignment.
pub(crate) fn restriction(
    chart: &Chart,
    index: Option<usize>,
    preset: ChartPreset,
) -> Option<&'static str> {
    let mut offset = 0;
    let source = chart.groups.iter().find(|g| {
        let found = index.unwrap_or(0) >= offset && index.unwrap_or(0) < offset + g.series.len();
        offset += g.series.len();
        found
    })?;
    let (target, _) = preset.parts();
    if family(source.kind) == Family::Unsupported || chart.ex.is_some() {
        return Some("chart.ui.type-unsupported");
    }
    if family(source.kind) != family(target) {
        return Some("chart.ui.type-family");
    }
    let others = chart
        .groups
        .iter()
        .flat_map(|g| g.series.iter().map(move |s| (g, s)))
        .enumerate()
        .filter(|(i, _)| Some(*i) != index)
        .map(|(_, pair)| pair)
        .collect::<Vec<_>>();
    let changed = ChartPreset::of(source.kind, source.grouping) != Some(preset);
    // Office's 3-D chart groups share one 3-D coordinate system; individual combination types are unsupported.
    if !others.is_empty()
        && changed
        && (source.three_d.is_some() || others.iter().any(|(g, _)| g.three_d.is_some()))
    {
        return Some("chart.ui.type-three-d");
    }
    if family(target) == Family::Round && !others.is_empty() && changed {
        return Some("chart.ui.type-round-mixed");
    }
    // Adding another value axis does not rotate the shared category axis.
    if family(target) == Family::Axes
        && others.iter().any(|(g, _)| {
            family(g.kind) == Family::Axes && horizontal(g.kind) != horizontal(target)
        })
    {
        return Some("chart.ui.type-orientation");
    }
    None
}
