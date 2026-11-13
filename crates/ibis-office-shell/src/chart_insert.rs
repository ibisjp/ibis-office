// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 挿入 > グラフ: the kinds of chart the menu offers, in its order, shared by the formats (a sheet's menu draws each
//! kind as a picture of the selected cells and adds its own kinds, `xlsx/chart_insert.rs`; a document's is the list of
//! names, `docx/chart_insert.rs`). What inserting a kind does is the format's (`insert`).

use gpui_kit::*;
use ibis_pptx::NewChartKind;

use crate::chrome::{MenuEntry, MenuItem};
use crate::editor::SlideEditor;

/// The kinds of chart, in menu order, each item calling `insert` with its kind (a document's, a sheet's): a list of
/// names.
pub fn chart_items_with(insert: fn(&mut SlideEditor, NewChartKind, &mut Window, &mut Context<SlideEditor>)) -> Vec<MenuEntry> {
    chart_items_with_thumbs(insert, None)
}

/// The same, each item with its picture for a grid in the menu (`thumb` draws the chart of a kind, so many pixels
/// wide: a sheet's is the chart of the selected cells), or without for a list of names.
pub fn chart_items_with_thumbs(insert: fn(&mut SlideEditor, NewChartKind, &mut Window, &mut Context<SlideEditor>), thumb: Option<fn(&SlideEditor, NewChartKind, f32) -> AnyElement>) -> Vec<MenuEntry> {
    let item = move |id: &'static str, icon: &'static str, kind: NewChartKind| -> MenuEntry {
        let item = MenuItem::tr(id).icon(icon).on(move |this: &mut SlideEditor, window, cx| insert(this, kind, window, cx));
        match thumb {
            Some(thumb) => item.thumb(move |editor, width| thumb(editor, kind, width)).into(),
            None => item.into(),
        }
    };
    vec![
        item("pptx.insert-chart.column", "bar_chart", NewChartKind::ClusteredColumn),
        item("pptx.insert-chart.stacked-column", "bar_chart", NewChartKind::StackedColumn),
        item("pptx.insert-chart.percent-stacked-column", "bar_chart", NewChartKind::PercentStackedColumn),
        item("pptx.insert-chart.3d-clustered-column", "bar_chart", NewChartKind::ClusteredColumn3D),
        item("pptx.insert-chart.3d-stacked-column", "bar_chart", NewChartKind::StackedColumn3D),
        item("pptx.insert-chart.3d-percent-stacked-column", "bar_chart", NewChartKind::PercentStackedColumn3D),
        item("pptx.insert-chart.3d-column", "bar_chart", NewChartKind::Column3D),
        MenuEntry::Separator,
        item("pptx.insert-chart.bar", "bar_chart", NewChartKind::ClusteredBar),
        item("pptx.insert-chart.stacked-bar", "bar_chart", NewChartKind::StackedBar),
        item("pptx.insert-chart.percent-stacked-bar", "bar_chart", NewChartKind::PercentStackedBar),
        item("pptx.insert-chart.3d-clustered-bar", "bar_chart", NewChartKind::ClusteredBar3D),
        item("pptx.insert-chart.3d-stacked-bar", "bar_chart", NewChartKind::StackedBar3D),
        item("pptx.insert-chart.3d-percent-stacked-bar", "bar_chart", NewChartKind::PercentStackedBar3D),
        MenuEntry::Separator,
        item("pptx.insert-chart.line", "show_chart", NewChartKind::Line),
        item("pptx.insert-chart.stacked-line", "show_chart", NewChartKind::StackedLine),
        item("pptx.insert-chart.percent-stacked-line", "show_chart", NewChartKind::PercentStackedLine),
        item("pptx.insert-chart.3d-line", "show_chart", NewChartKind::Line3D),
        MenuEntry::Separator,
        item("pptx.insert-chart.area", "area_chart", NewChartKind::Area),
        item("pptx.insert-chart.stacked-area", "area_chart", NewChartKind::StackedArea),
        item("pptx.insert-chart.percent-stacked-area", "area_chart", NewChartKind::PercentStackedArea),
        item("pptx.insert-chart.3d-area", "area_chart", NewChartKind::Area3D),
        item("pptx.insert-chart.3d-stacked-area", "area_chart", NewChartKind::StackedArea3D),
        item("pptx.insert-chart.3d-percent-stacked-area", "area_chart", NewChartKind::PercentStackedArea3D),
        MenuEntry::Separator,
        item("pptx.insert-chart.pie", "pie_chart", NewChartKind::Pie),
        item("pptx.insert-chart.3d-pie", "pie_chart", NewChartKind::Pie3D),
        item("pptx.insert-chart.doughnut", "donut_large", NewChartKind::Doughnut),
        item("pptx.insert-chart.pie-of-pie", "pie_chart", NewChartKind::PieOfPie),
        item("pptx.insert-chart.bar-of-pie", "pie_chart", NewChartKind::BarOfPie),
        MenuEntry::Separator,
        item("pptx.insert-chart.scatter", "polyline", NewChartKind::Scatter),
        item("pptx.insert-chart.scatter-smooth", "polyline", NewChartKind::ScatterSmooth),
        item("pptx.insert-chart.scatter-lines", "polyline", NewChartKind::ScatterLines),
        item("pptx.insert-chart.bubble", "dataset", NewChartKind::Bubble),
        MenuEntry::Separator,
        item("pptx.chrome.radar", "show_chart", NewChartKind::Radar),
        item("pptx.chrome.filled-radar", "area_chart", NewChartKind::RadarFilled),
        MenuEntry::Separator,
        item("pptx.insert-chart.stock-hlc", "show_chart", NewChartKind::StockHLC),
        item("pptx.insert-chart.stock-ohlc", "show_chart", NewChartKind::StockOHLC),
        item("pptx.insert-chart.stock-vhlc", "show_chart", NewChartKind::StockVHLC),
        item("pptx.insert-chart.stock-vohlc", "show_chart", NewChartKind::StockVOHLC),
        MenuEntry::Separator,
        item("pptx.insert-chart.surface-3d", "area_chart", NewChartKind::Surface3D),
        item("pptx.insert-chart.surface-3d-wireframe", "area_chart", NewChartKind::Surface3DWireframe),
        item("pptx.insert-chart.contour", "area_chart", NewChartKind::Contour),
        item("pptx.insert-chart.contour-wireframe", "area_chart", NewChartKind::ContourWireframe),
        MenuEntry::Separator,
        item("pptx.insert-chart.combo-column-line", "bar_chart", NewChartKind::ComboColumnLine),
        item("pptx.insert-chart.combo-column-line-secondary", "bar_chart", NewChartKind::ComboColumnLineSecondaryAxis),
        item("pptx.insert-chart.combo-area-column", "area_chart", NewChartKind::ComboAreaColumn),
    ]
}

/// The picture of `chart` in a menu's grid (`menu_tiles.rs`): `width` pixels wide and `ratio` (height over width) tall, drawn by
/// the charts' own view. The 挿入 > グラフ menus of every screen and the sheet's recommended-charts panel draw their pictures with
/// this one part (a sheet's chart is of the selected cells; a document's and a presentation's is of the fixed data below).
pub fn chart_preview_view(editor: &SlideEditor, chart: &ibis_doc_model::Chart, width: f32, ratio: f32) -> AnyElement {
    const WIDTH_EMU: i64 = 4_572_000;
    let frame = ibis_doc_model::EmuRect::new(0, 0, WIDTH_EMU, (WIDTH_EMU as f32 * ratio).round() as i64);
    ibis_office_render::chart_view::chart_element(chart, frame, width / WIDTH_EMU as f32, &editor.resources).into_any_element()
}

/// The data a menu's picture of a chart kind shows, fixed in the code (not the document's): chosen so that the picture shows what the
/// kind is for (a line: a rise over time; a pie: shares of a whole; a scatter chart: two values that move together…).
struct PreviewData {
    categories: Vec<&'static str>,
    /// One list of values for each series, in the order the kind's series are.
    series: Vec<Vec<f64>>,
    /// The X values of a scatter or bubble chart's series.
    x: Vec<f64>,
    /// The sizes of a bubble chart's points.
    sizes: Vec<f64>,
}

fn preview_data(kind: NewChartKind) -> PreviewData {
    use NewChartKind as K;
    let plain = |categories: Vec<&'static str>, series: Vec<Vec<f64>>| PreviewData { categories, series, x: Vec::new(), sizes: Vec::new() };
    let days = vec!["1", "2", "3", "4", "5", "6"];
    match kind {
        K::Line | K::StackedLine | K::PercentStackedLine | K::Line3D => plain(days, vec![vec![2.0, 3.0, 3.5, 5.0, 6.5, 8.0], vec![1.5, 2.0, 2.8, 3.2, 4.5, 5.0]]),
        K::Area | K::StackedArea | K::PercentStackedArea | K::Area3D | K::StackedArea3D | K::PercentStackedArea3D => plain(days, vec![vec![2.0, 3.0, 3.5, 5.0, 6.5, 8.0], vec![1.5, 2.0, 2.8, 3.2, 4.5, 5.0]]),
        K::Pie | K::Pie3D | K::Doughnut => plain(vec!["A", "B", "C", "D"], vec![vec![50.0, 25.0, 15.0, 10.0]]),
        K::PieOfPie | K::BarOfPie => plain(vec!["A", "B", "C", "D", "E", "F"], vec![vec![40.0, 25.0, 15.0, 10.0, 6.0, 4.0]]),
        K::Scatter | K::ScatterSmooth | K::ScatterLines => PreviewData {
            categories: Vec::new(),
            series: vec![vec![1.2, 2.1, 2.8, 4.4, 4.9, 6.3, 7.2, 7.9]],
            x: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
            sizes: Vec::new(),
        },
        K::Bubble => PreviewData {
            categories: Vec::new(),
            series: vec![vec![2.0, 4.0, 3.0, 6.0, 5.0]],
            x: vec![1.0, 2.0, 3.0, 4.0, 5.0],
            sizes: vec![10.0, 25.0, 40.0, 15.0, 30.0],
        },
        K::Radar | K::RadarFilled => plain(vec!["A", "B", "C", "D", "E"], vec![vec![8.0, 6.0, 7.0, 5.0, 9.0], vec![5.0, 8.0, 4.0, 7.0, 6.0]]),
        // The days' high, low and close (and open, volume) of a share: the series are in the order the kind reads them.
        K::StockHLC | K::StockOHLC | K::StockVHLC | K::StockVOHLC => {
            let open = vec![20.0, 22.0, 21.0, 24.0, 23.0, 26.0];
            let high = vec![23.0, 25.0, 24.0, 27.0, 27.0, 29.0];
            let low = vec![18.0, 20.0, 19.0, 22.0, 21.0, 24.0];
            let close = vec![22.0, 21.0, 23.0, 23.0, 26.0, 28.0];
            let volume = vec![30.0, 50.0, 40.0, 60.0, 45.0, 70.0];
            let series = match kind {
                K::StockHLC => vec![high, low, close],
                K::StockOHLC => vec![open, high, low, close],
                K::StockVHLC => vec![volume, high, low, close],
                _ => vec![volume, open, high, low, close],
            };
            plain(days, series)
        }
        // A hill over a grid: the height is highest at the middle.
        K::Surface3D | K::Surface3DWireframe | K::Contour | K::ContourWireframe => {
            let hill = |row: f64| (0..5).map(|column| 9.0 - ((column as f64 - 2.0).powi(2) + (row - 1.5).powi(2)) * 1.1).collect::<Vec<f64>>();
            plain(vec!["A", "B", "C", "D", "E"], (0..4).map(|row| hill(row as f64)).collect())
        }
        K::ComboColumnLine | K::ComboColumnLineSecondaryAxis => plain(vec!["A", "B", "C", "D"], vec![vec![5.0, 8.0, 6.0, 9.0], vec![3.0, 5.0, 7.0, 6.0]]),
        K::ComboAreaColumn => plain(vec!["A", "B", "C", "D"], vec![vec![4.0, 6.0, 5.0, 7.0], vec![3.0, 4.0, 6.0, 5.0]]),
        _ => plain(vec!["A", "B", "C", "D"], vec![vec![5.0, 8.0, 6.0, 9.0], vec![3.0, 5.0, 7.0, 4.0]]),
    }
}

/// `template` (a new chart of `kind` as a document or presentation makes it) with the fixed data of the menu's picture instead of its sample
/// data, and no title, legend or data table that would crowd a picture of a hundred pixels. A chart of the new kinds (chartEx) is kept as it is.
pub fn chart_with_preview_data(mut template: ibis_doc_model::Chart, kind: NewChartKind) -> ibis_doc_model::Chart {
    use ibis_doc_model::Number;
    template.title = None;
    template.legend = None;
    template.data_table = None;
    if kind.is_chart_ex() {
        return template;
    }
    let data = preview_data(kind);
    // As many series as the data has: the last one repeated when the template has fewer, the rest dropped when it has more.
    // A combination chart: one series in each of its kinds (the first group's sample series would crowd the others out).
    if matches!(kind, NewChartKind::ComboColumnLine | NewChartKind::ComboColumnLineSecondaryAxis | NewChartKind::ComboAreaColumn) {
        for group in &mut template.groups {
            group.series.truncate(1);
        }
    }
    let mut want = data.series.len();
    let total: usize = template.groups.iter().map(|g| g.series.len()).sum();
    if total > 0 && total < want && let Some(last) = template.groups.last_mut().filter(|g| !g.series.is_empty()) {
        for _ in total..want {
            let copy = last.series.last().cloned().unwrap_or_else(|| unreachable!());
            last.series.push(copy);
        }
    }
    for group in &mut template.groups {
        group.series.truncate(want);
        want -= group.series.len();
    }
    template.groups.retain(|g| !g.series.is_empty());
    for (index, series) in template.groups.iter_mut().flat_map(|g| g.series.iter_mut()).enumerate() {
        let Some(values) = data.series.get(index) else { continue };
        series.categories = if data.categories.is_empty() { Vec::new() } else { data.categories.iter().map(|c| c.to_string()).collect() };
        series.values = values.iter().map(|&v| Some(Number(v))).collect();
        series.x_values = data.x.iter().map(|&v| Some(Number(v))).collect();
        series.bubble_sizes = data.sizes.iter().map(|&v| Some(Number(v))).collect();
        // The points the template coloured one by one (a pie's slices) are the template's four or six.
        series.point_fills.retain(|(point, _)| *point < values.len());
        series.point_strokes.retain(|(point, _)| *point < values.len());
    }
    template
}
