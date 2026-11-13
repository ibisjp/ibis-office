// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibisOffice visual presets, written as explicit OOXML formatting, not imitations of numbered Office styles.
use ibis_doc_model::chart::AxisKind;
use ibis_doc_model::{Chart, Color, EMU_PER_POINT, Fill, Rgb, Stroke};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChartStylePreset {
    Standard,
    Simple,
    Framed,
    Dark,
}
pub(crate) const CHART_STYLES: [(&str, ChartStylePreset); 4] = [
    ("chart.ui.style-standard", ChartStylePreset::Standard),
    ("chart.ui.style-simple", ChartStylePreset::Simple),
    ("chart.ui.style-framed", ChartStylePreset::Framed),
    ("chart.ui.style-dark", ChartStylePreset::Dark),
];
impl ChartStylePreset {
    fn surface(self) -> (u32, Option<u32>, u32, Option<u32>, u32, u32, bool) {
        // Application design tokens. These are not Microsoft's c:style-number definitions.
        match self {
            Self::Standard => (
                0xffffff,
                Some(0xdadce0),
                0xffffff,
                None,
                0x202124,
                0xdadce0,
                true,
            ),
            Self::Simple => (0xffffff, None, 0xffffff, None, 0x202124, 0xdadce0, false),
            Self::Framed => (
                0xffffff,
                Some(0x5f6368),
                0xf1f3f4,
                Some(0xdadce0),
                0x202124,
                0xdadce0,
                true,
            ),
            Self::Dark => (0x202124, None, 0x202124, None, 0xe8eaed, 0x5f6368, true),
        }
    }
    pub(crate) fn apply(self, chart: &mut Chart) {
        let (background, border, plot, plot_border, text, grid, show_grid) = self.surface();
        let color = |v| Color::opaque(Rgb(v));
        let stroke = |v| Stroke::solid(color(v), EMU_PER_POINT);
        chart.style = None; // Explicit formatting has portable, defined semantics on save/reopen.
        chart.background = Fill::Solid(color(background));
        chart.border = border.map(stroke);
        chart.plot_background = Fill::Solid(color(plot));
        chart.plot_border = plot_border.map(stroke);
        if let Some(title) = &mut chart.title {
            title.style.color = Some(color(text));
        }
        if let Some(legend) = &mut chart.legend {
            legend.text.color = Some(color(text));
        }
        for axis in &mut chart.axes {
            axis.text.color = Some(color(text));
            axis.line = Some(stroke(text));
            axis.gridlines = (show_grid && axis.kind == AxisKind::Value).then(|| stroke(grid));
            axis.minor_gridlines = None;
            if let Some(title) = &mut axis.title {
                title.style.color = Some(color(text));
            }
        }
        for series in chart.groups.iter_mut().flat_map(|g| &mut g.series) {
            if let Some(labels) = &mut series.labels {
                labels.style.color = Some(color(text));
            }
        }
    }
    pub(crate) fn matches(self, chart: &Chart) -> bool {
        let (background, border, plot, plot_border, _, _, _) = self.surface();
        let fill = |v| Fill::Solid(Color::opaque(Rgb(v)));
        chart.background == fill(background)
            && chart.plot_background == fill(plot)
            && chart.border.as_ref().map(|s| s.color.rgb.0) == border
            && chart.plot_border.as_ref().map(|s| s.color.rgb.0) == plot_border
    }
}

pub(crate) fn inherit_new_text(before: &Chart, after: &mut Chart) {
    let Some((_, preset)) = CHART_STYLES.iter().find(|(_, p)| p.matches(before)) else {
        return;
    };
    let color = Some(Color::opaque(Rgb(preset.surface().4)));
    if before.title.is_none() {
        if let Some(t) = after.title.as_mut() {
            t.style.color = color;
        }
    }
    if before.legend.is_none() {
        if let Some(l) = after.legend.as_mut() {
            l.text.color = color;
        }
    }
    for axis in &mut after.axes {
        if before
            .axes
            .iter()
            .find(|a| a.id == axis.id)
            .is_some_and(|a| a.title.is_none())
        {
            if let Some(t) = axis.title.as_mut() {
                t.style.color = color;
            }
        }
    }
}
