// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Actual two-column scatter insertion, axis roles, retained pane and persisted insertion defaults.
use crate::shell::scenario::{Scenario, button_probe as buttons};
use gpui_kit::*;
use ibis_doc_model::{Chart, ChartType, Color, Fill, Rgb, ShapeKind};
#[track_caller]
fn selected(s: &mut Scenario) -> Chart {
    s.with(|e, _| e.sheet_chart().map(|(_,c)|c.clone())).expect("chart must stay selected")
}
fn select(s: &mut Scenario, a: (u32, u32), b: (u32, u32)) {
    let from = s.with(|e, _| e.cell_center(a.0, a.1).unwrap());
    let to = s.with(|e, _| e.cell_center(b.0, b.1).unwrap());
    s.dispatch(vec![
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position: from,
            click_count: 1,
            ..Default::default()
        }),
        PlatformInput::MouseMove(MouseMoveEvent {
            position: to,
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        }),
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position: to,
            click_count: 1,
            ..Default::default()
        }),
    ]);
}
fn defaults(chart: &Chart) {
    assert_eq!(chart.background, Fill::Solid(Color::opaque(Rgb(0xffffff))));
    assert_eq!(chart.border.as_ref().unwrap().color.rgb, Rgb(0xdadce0));
    assert!(chart.title.is_some());
    assert_eq!(chart.style, Some(2));
}
pub(crate) fn insertion_and_axes() {
    use crate::xlsx::chart_edit::{category_axis, secondary_value_axis, value_axis};
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    select(&mut s, (1, 2), (2, 7));
    s.menu("pptx.insert-chart.scatter");
    let chart = selected(&mut s);
    defaults(&chart);
    assert_eq!(
        chart.groups.iter().map(|g| g.series.len()).sum::<usize>(),
        1
    );
    let x = category_axis(&chart).unwrap();
    let y = value_axis(&chart).unwrap();
    assert_ne!(x, y);
    assert_eq!(chart.axes[x].kind, ibis_doc_model::chart::AxisKind::Value);
    assert_eq!(chart.axes[x].side, ibis_doc_model::chart::Side::Bottom);
    assert_eq!(chart.axes[y].side, ibis_doc_model::chart::Side::Left);
    assert!(
        secondary_value_axis(&chart).is_none(),
        "primary scatter X is not a secondary Y"
    );
    s.menu("xlsx.chart-format");
    s.update(|e, _, cx| {
        let p = e.wb_mut().chart_edit.as_mut().unwrap();
        p.tab = 1;
        p.axis_tab = 0;
        cx.notify();
    });
    let x_input = s.with(|e, _| e.wb().chart_edit.as_ref().unwrap().domain_min.clone());
    s.update(|_, w, cx| {
        x_input.read(cx).focus_handle(cx).focus(w, cx);
    });
    s.key("cmd-a");
    s.type_text("900000");
    s.key("enter");
    let chart = selected(&mut s);
    assert_eq!(
        chart.axes[category_axis(&chart).unwrap()].min,
        Some(ibis_doc_model::Number(900000.))
    );
    buttons::clear();
    s.update(|e, _, cx| {
        e.wb_mut().chart_edit.as_mut().unwrap().tab = 0;
        cx.notify();
    });

    s.update(|e, w, cx| e.open_chart_series_editor(Some(0), w, cx));
    assert!(!s.with(|e, _| {
        e.wb()
            .chart_edit
            .as_ref()
            .unwrap()
            .series_editor
            .as_ref()
            .unwrap()
            .secondary
    }));
    assert!(s.with(|e, _| e.sidebar_open));
    assert!(
        buttons::all().iter().any(|id| id == "chart-series-add"),
        "the chart pane is still drawn behind the series dialog"
    );
    assert!(
        !buttons::all().iter().any(|id| id == "chart-series-remove"),
        "the only series cannot be deleted"
    );
    s.screenshot("scatter-primary-y-left-with-pane");
    buttons::click(&mut s, "chart-series-close");
    s.update(|e, _, cx| {
        e.selection = None;
        e.sidebar_open = false;
        cx.notify();
    });
    select(&mut s, (0, 2), (3, 7));
    s.menu("pptx.insert-chart.column");
    let column = selected(&mut s);
    defaults(&column);
    let colors = column
        .groups
        .iter()
        .flat_map(|g| &g.series)
        .filter_map(|s| {
            if let Fill::Solid(c) = s.fill {
                Some(c.rgb.0)
            } else {
                None
            }
        })
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        colors.len() >= 3,
        "default style has a colorful series cycle"
    );
    s.save_as("chart-insertion-defaults.xlsx");
    let saved = s.reopen();
    let scatter = saved.slides[0]
        .shapes
        .iter()
        .find_map(|s| {
            if let ShapeKind::Chart(c) = &s.kind {
                matches!(c.groups[0].kind, ChartType::Scatter { .. }).then_some(c.as_ref())
            } else {
                None
            }
        })
        .unwrap();
    defaults(scatter);
    assert_eq!(
        scatter.axes[category_axis(scatter).unwrap()].min,
        Some(ibis_doc_model::Number(900000.))
    );
    assert!(secondary_value_axis(scatter).is_none());
    let column = saved.slides[0]
        .shapes
        .iter()
        .rev()
        .find_map(|s| {
            if let ShapeKind::Chart(c) = &s.kind {
                Some(c.as_ref())
            } else {
                None
            }
        })
        .unwrap();
    defaults(column);
    // The standard style selector and visual palette selector work through actual popup events.
    s.menu("xlsx.chart-format");
    s.update(|e, _, cx| {
        e.wb_mut().chart_edit.as_mut().unwrap().tab = 2;
        cx.notify();
    });
    buttons::click(&mut s, "chart-style-dropdown");
    s.screenshot("chart-style-dropdown-open");
    // Four entries: no search field. Choose the framed preset through actual list navigation.
    s.key("down");
    s.key("down");
    s.key("enter");
    assert!(crate::xlsx::chart_style::ChartStylePreset::Framed.matches(&selected(&mut s)));
    buttons::click(&mut s, "chart-palette-dropdown");
    let choice = buttons::all()
        .into_iter()
        .find(|id| id.contains("searchable-item-chart-palette-dropdown") && id.ends_with(", 1)"))
        .unwrap();
    let bound = buttons::bounds(&choice).unwrap();
    s.hover((
        f32::from(bound.origin.x + bound.size.width / 2.),
        f32::from(bound.origin.y + bound.size.height / 2.),
    ));
    s.wait(800);
    s.screenshot("chart-palette-dropdown-tooltip-left");
    buttons::click(&mut s, &choice);
    assert!(s.with(|e, _| e.toolbar_search_open.is_none()));
    assert_eq!(
        selected(&mut s).groups[0].series[0].fill,
        Fill::Solid(Color::opaque(Rgb(crate::xlsx::chart_edit::COLOR_PALETTES
            [1]
        .1[0])))
    );
    s.update(|e, _, cx| {
        e.selection = None;
        e.sidebar_open = false;
        cx.notify();
    });
    select(&mut s, (0, 2), (1, 7));
    s.menu("pptx.insert-chart.waterfall");
    let ex = selected(&mut s);
    assert!(ex.ex.is_some());
    assert_eq!(ex.background, Fill::Solid(Color::opaque(Rgb(0xffffff))));
    assert_eq!(ex.border.as_ref().unwrap().color.rgb, Rgb(0xdadce0));
    assert!(ex.title.is_some());
    s.screenshot("chart-ex-insertion-defaults");
    s.save_as("chart-insertion-chartex-defaults.xlsx");
    let saved = s.reopen();
    let ex = saved.slides[0]
        .shapes
        .iter()
        .rev()
        .find_map(|s| {
            if let ShapeKind::Chart(c) = &s.kind {
                c.ex.is_some().then_some(c.as_ref())
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(ex.background, Fill::Solid(Color::opaque(Rgb(0xffffff))));
    assert_eq!(ex.border.as_ref().unwrap().color.rgb, Rgb(0xdadce0));
    assert!(ex.title.is_some());
}
