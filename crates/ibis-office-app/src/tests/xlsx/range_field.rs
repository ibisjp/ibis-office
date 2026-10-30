// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Actual pointer/keyboard checks of the shared sheet range field.
use crate::shell::scenario::{Scenario, button_probe as buttons};
use gpui_kit::*;

pub(crate) fn range_picker() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    let index = s
        .with(|e, _| {
            e.slide()
                .shapes
                .iter()
                .position(|shape| matches!(shape.kind, ibis_doc_model::ShapeKind::Chart(_)))
        })
        .unwrap();
    let chart_point = s.shape_position(index);
    s.click(chart_point);
    s.menu("xlsx.chart-format");
    s.update(|e, w, cx| e.open_chart_series_editor(Some(0), w, cx));
    let original_bounds = s.with(|e, _| e.panel_dialog.bounds.unwrap());
    assert_eq!(original_bounds.right(), px(1200.));
    let input = s.with(|e, _| {
        e.wb()
            .chart_edit
            .as_ref()
            .unwrap()
            .series_editor
            .as_ref()
            .unwrap()
            .values
            .clone()
    });
    let original = s.with(|_, cx| input.read(cx).value().to_string());
    let shape = s.with(|e, _| e.selection);
    let original_selection = s.with(|e, _| e.selected_ranges());
    let point = |s: &mut Scenario, column, row| s.with(|e, _| e.cell_center(column, row).unwrap());
    let drag = |s: &mut Scenario, from: Point<Pixels>, to: Point<Pixels>| {
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
    };
    buttons::click(&mut s, "chart-series-values");
    assert!(s.with(|e, _| e.sheet_range_pick_active()));
    assert_eq!(
        s.with(|e, _| e.panel_dialog.bounds.unwrap().size.width),
        px(320.)
    );
    let from = point(&mut s, 1, 3);
    let to = point(&mut s, 1, 5);
    drag(&mut s, from, to);
    assert_eq!(
        s.with(|e, _| e.selection),
        shape,
        "range picking keeps the chart selected"
    );
    let changed = s.with(|_, cx| input.read(cx).value().to_string());
    assert!(
        changed.ends_with("$B$4:$B$6"),
        "dragged reference: {changed}"
    );
    s.update(|_, w, cx| input.read(cx).focus_handle(cx).focus(w, cx));
    s.key("enter");
    assert!(
        s.with(|e, _| e.sheet_range_pick_active()),
        "Enter in a field does not accept a sheet selection"
    );
    s.update(|e, w, cx| e.focus_handle.focus(w, cx));
    s.key("escape");
    assert!(!s.with(|e, _| e.sheet_range_pick_active()));
    assert_eq!(s.with(|_, cx| input.read(cx).value().to_string()), original);
    assert_eq!(s.with(|e, _| e.selected_ranges()), original_selection);
    assert!(
        s.with(|e, _| e.wb().chart_edit.as_ref().unwrap().series_editor.is_some()),
        "Escape cancels only picking"
    );
    assert_eq!(
        s.with(|e, _| e.panel_dialog.bounds.unwrap().size.width),
        original_bounds.size.width
    );
    buttons::click(&mut s, "chart-series-values");
    drag(&mut s, from, to);
    s.key("enter");
    assert!(!s.with(|e, _| e.sheet_range_pick_active()));
    assert_eq!(s.with(|_, cx| input.read(cx).value().to_string()), changed);
    assert_eq!(
        s.with(|e, _| e.panel_dialog.bounds.unwrap().size.width),
        original_bounds.size.width
    );
    buttons::click(&mut s, "chart-series-values");
    // A press on OK while picking ends only picking, never commits the whole series draft.
    buttons::click(&mut s, "chart-series-close");
    assert!(s.with(|e, _| e.wb().chart_edit.as_ref().unwrap().series_editor.is_some()));
    assert!(
        !s.with(|e, _| e.sheet_range_pick_active()),
        "clicking the owner dialog ends picking"
    );
    assert_eq!(
        s.with(|e, _| e.panel_dialog.bounds.unwrap().size.width),
        original_bounds.size.width
    );
    s.screenshot("chart-range-picking-complete");
    s.update(|e, w, cx| {
        e.cancel_chart_series_editor(w, cx);
        e.wb_mut().chart_edit.as_mut().unwrap().tab = 2;
    });
    let before = s.with(|e, _| e.sheet_chart().unwrap().1.clone());
    let global = s.with(|e, _| e.wb().chart_edit.as_ref().unwrap().range.clone());
    let before_range = s.with(|_, cx| global.read(cx).value().to_string());
    s.update(|e, w, cx| e.start_sheet_range_pick(global.clone(), w, cx));
    let from = point(&mut s, 0, 2);
    let to = point(&mut s, 2, 5);
    drag(&mut s, from, to);
    s.key("enter");
    assert!(!s.with(|e, _| e.sheet_range_pick_active()));
    assert_ne!(s.with(|e, _| e.sheet_chart().unwrap().1.clone()), before);
    let changed_range = s.with(|_, cx| global.read(cx).value().to_string());
    s.key("cmd-z");
    assert_eq!(
        s.with(|e, _| e.sheet_chart().unwrap().1.clone()),
        before,
        "one undo restores the whole chart source change"
    );
    assert_eq!(
        s.with(|_, cx| global.read(cx).value().to_string()),
        before_range,
        "Undo synchronizes the pane source field"
    );
    s.key("cmd-shift-z");
    assert_eq!(
        s.with(|_, cx| global.read(cx).value().to_string()),
        changed_range,
        "Redo synchronizes the pane source field"
    );
}

/// Edges resize an existing reference; this is deliberately different from autofill.
pub(crate) fn range_picker_handles_and_cancel() {
    use crate::xlsx::range_field::RangeEdge;
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    let index = s
        .with(|e, _| {
            e.slide()
                .shapes
                .iter()
                .position(|shape| matches!(shape.kind, ibis_doc_model::ShapeKind::Chart(_)))
        })
        .unwrap();
    let at = s.shape_position(index);
    s.click(at);
    s.menu("xlsx.chart-format");
    s.update(|e, w, cx| e.open_chart_series_editor(Some(0), w, cx));
    let input = s.with(|e, _| {
        e.wb()
            .chart_edit
            .as_ref()
            .unwrap()
            .series_editor
            .as_ref()
            .unwrap()
            .values
            .clone()
    });
    let original = s.with(|_, cx| input.read(cx).value().to_string());
    let selected = s.with(|e, _| e.selected_ranges());
    let document = s.with(|e, _| e.presentation().clone());
    buttons::click(&mut s, "chart-series-values");
    assert!(
        buttons::all()
            .iter()
            .all(|id| !id.contains("chart-series-tab")),
        "range picking hides series tabs"
    );
    for edge in [
        RangeEdge::Top,
        RangeEdge::Bottom,
        RangeEdge::Left,
        RangeEdge::Right,
    ] {
        assert!(s.with(|e, _| e.sheet_range_handle_bounds(edge)).is_some());
    }
    let right = s.with(|e, _| {
        e.sheet_range_handle_bounds(RangeEdge::Right)
            .unwrap()
            .center()
    });
    let bottom = s.with(|e, _| {
        e.sheet_range_handle_bounds(RangeEdge::Bottom)
            .unwrap()
            .center()
    });
    let shot = s.screenshot("range-picker-four-edges-no-autofill");
    let image = image::open(shot).unwrap().to_rgba8();
    let scale = image.width() as f32 / 1200.;
    let green = image::Rgba([0x10, 0x7c, 0x41, 255]);
    for dx in 2..=4 {
        for dy in 2..=4 {
            let x = ((f32::from(right.x) + dx as f32) * scale).round() as u32;
            let y = ((f32::from(bottom.y) + dy as f32) * scale).round() as u32;
            assert_ne!(
                *image.get_pixel(x, y),
                green,
                "no autofill square beyond the bottom-right range corner"
            );
        }
    }
    let mut expected = s.with(|e, _| e.selected_range().unwrap());
    let initial = expected;
    let top = expected.first_row.saturating_sub(1);
    let bottom = expected.last_row + 1;
    let left = expected.first_column.saturating_sub(1);
    let right = expected.last_column + 1;
    for (edge, column, row) in [
        (RangeEdge::Top, expected.first_column, top),
        (RangeEdge::Bottom, expected.first_column, bottom),
        (RangeEdge::Left, left, top),
        (RangeEdge::Right, right, top),
    ] {
        let start = s.with(|e, _| e.sheet_range_handle_bounds(edge).unwrap().center());
        let end = s.with(|e, _| e.cell_center(column, row).unwrap());
        s.hover((f32::from(start.x), f32::from(start.y)));
        s.dispatch(vec![
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                click_count: 1,
                ..Default::default()
            }),
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                click_count: 1,
                ..Default::default()
            }),
        ]);
        match edge {
            RangeEdge::Top => expected.first_row = top,
            RangeEdge::Bottom => expected.last_row = bottom,
            RangeEdge::Left => expected.first_column = left,
            RangeEdge::Right => expected.last_column = right,
        }
        assert_eq!(
            s.with(|e, _| e.selected_range().unwrap()),
            expected,
            "edge {edge:?} changes only its own boundary"
        );
    }
    for (edge, column, row) in [
        (RangeEdge::Top, initial.first_column, initial.first_row),
        (RangeEdge::Bottom, initial.first_column, initial.last_row),
        (RangeEdge::Left, initial.first_column, initial.first_row),
        (RangeEdge::Right, initial.last_column, initial.first_row),
    ] {
        let start = s.with(|e, _| e.sheet_range_handle_bounds(edge).unwrap().center());
        let end = s.with(|e, _| e.cell_center(column, row).unwrap());
        s.dispatch(vec![
            PlatformInput::MouseDown(MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                click_count: 1,
                ..Default::default()
            }),
            PlatformInput::MouseMove(MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            }),
            PlatformInput::MouseUp(MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                click_count: 1,
                ..Default::default()
            }),
        ]);
        match edge {
            RangeEdge::Top => expected.first_row = initial.first_row,
            RangeEdge::Bottom => expected.last_row = initial.last_row,
            RangeEdge::Left => expected.first_column = initial.first_column,
            RangeEdge::Right => expected.last_column = initial.last_column,
        }
        assert_eq!(
            s.with(|e, _| e.selected_range().unwrap()),
            expected,
            "edge {edge:?} also shrinks its own boundary"
        );
    }
    let start = s.with(|e, _| {
        e.sheet_range_handle_bounds(RangeEdge::Right)
            .unwrap()
            .center()
    });
    let end = s.with(|e, _| e.cell_center(right, top).unwrap());
    s.dispatch(vec![
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position: start,
            click_count: 1,
            ..Default::default()
        }),
        PlatformInput::MouseMove(MouseMoveEvent {
            position: end,
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        }),
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position: end,
            click_count: 1,
            ..Default::default()
        }),
    ]);
    assert_ne!(s.with(|_, cx| input.read(cx).value().to_string()), original);
    s.key("escape");
    assert!(!s.with(|e, _| e.sheet_range_pick_active()));
    assert_eq!(s.with(|_, cx| input.read(cx).value().to_string()), original);
    assert_eq!(s.with(|e, _| e.selected_ranges()), selected);
    assert!(
        s.with(|e, _| e.wb().chart_edit.as_ref().unwrap().series_editor.is_some()),
        "Cancel discards the range session, not the owner dialog"
    );
    assert_eq!(s.with(|e, _| e.presentation().clone()), document);
}
