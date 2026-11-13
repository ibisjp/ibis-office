// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The shared segment control, panel header tabs and equal-width button group in the xlsx panels: the sparkline's
//! marker colour target (#33), the forecast sheet's choices (#46), the chart move panel (#73) and the table design
//! panel (#70).

use super::SheetSteps;
use crate::shell::scenario::{Scenario, button_probe as buttons};
use ibis_doc_model::{CellValue, SheetCommand};
use crate::ext::*;

const BOOK: &str = "test/editing/xlsx-sales.xlsx";

fn click_ui(s: &mut Scenario, b: gpui_kit::Bounds<gpui_kit::Pixels>) {
    s.click((f32::from(b.origin.x + b.size.width / 2.0), f32::from(b.origin.y + b.size.height / 2.0)));
    s.settle();
}

// ---- #33 sparkline -------------------------------------------------------------------------------------------

pub(crate) fn sparkline_point_target_segment() {
    use crate::sparkline_dialog::SparklinePointKind as K;
    let mut s = Scenario::open(BOOK);
    s.size(1400., 1100.);
    s.update(|e, w, cx| e.open_sparkline_panel(None, w, cx));
    s.settle();
    let ui = |s: &mut Scenario, id: &str| s.with(|e, _| e.wb().panels.sparkline.as_ref().unwrap().ui.borrow().get(id).copied());
    let ids = ["high", "low", "negative", "first", "last", "markers"];
    let kinds = [K::High, K::Low, K::Negative, K::First, K::Last, K::Markers];
    // Every target is one item of one segment control (same height, in order, no gaps in the order of the row).
    let boxes: Vec<_> = ids.iter().map(|i| ui(&mut s, &format!("xlsx.sparkline.point-target.{i}")).unwrap_or_else(|| panic!("{i} not drawn"))).collect();
    assert!(boxes.iter().all(|b| b.size.height == boxes[0].size.height), "one control: equal heights {boxes:?}");
    for (i, kind) in kinds.iter().enumerate() {
        click_ui(&mut s, boxes[i]);
        let got = s.with(|e, _| e.wb().panels.sparkline.as_ref().unwrap().selected_point_kind);
        assert_eq!(got, *kind, "clicking {} selects it", ids[i]);
    }
    s.screenshot("sparkline-point-target-segment");
}

// ---- #46 forecast sheet --------------------------------------------------------------------------------------

pub(crate) fn forecast_segments() {
    let mut s = Scenario::open(BOOK);
    s.size(1400., 1100.);
    s.update(|e, w, cx| e.open_forecast_panel(w, cx));
    s.settle();
    let ui = |s: &mut Scenario, id: &str| s.with(|e, _| e.wb().panels.forecast.as_ref().unwrap().ui.borrow().get(id).copied());
    let state = |s: &mut Scenario| s.with(|e, _| { let p = e.wb().panels.forecast.as_ref().unwrap(); (p.seasonality_manual, p.data_completion_zero, p.columns) });
    assert_eq!(state(&mut s), (false, false, false));
    let b = ui(&mut s, "seasonality-manual").expect("seasonality segment");
    // Items of one segment share a row and a height.
    let a = ui(&mut s, "seasonality-auto").unwrap();
    assert!(a.origin.y == b.origin.y && a.size.height == b.size.height && a.right() <= b.left() + gpui_kit::px(1.));
    click_ui(&mut s, b);
    assert_eq!(state(&mut s), (true, false, false));
    s.screenshot("forecast-seasonality-manual");
    let b = ui(&mut s, "completion-zero").expect("completion segment");
    click_ui(&mut s, b);
    let b = ui(&mut s, "chart-column").expect("chart segment");
    click_ui(&mut s, b);
    assert_eq!(state(&mut s), (true, true, true));
    let b = ui(&mut s, "completion-interpolate").unwrap();
    click_ui(&mut s, b);
    assert_eq!(state(&mut s), (true, false, true));
    s.screenshot("forecast-segments");
}

// ---- #73 move chart ------------------------------------------------------------------------------------------

pub(crate) fn move_chart_top_segment_switches_controls() {
    let mut s = Scenario::open(BOOK);
    s.size(1400., 1000.);
    let idx = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_))).expect("chart"));
    let pos = s.shape_position(idx);
    s.click(pos);
    s.settle();
    s.menu("xlsx.move-chart");
    s.settle();
    let _probe = buttons::Probe::new();
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let ui = |s: &mut Scenario, id: &str| s.with(|e, _| e.wb().panels.move_chart.as_ref().unwrap().ui.borrow().get(id).copied());
    let clear = |s: &mut Scenario| s.with(|e, _| e.wb().panels.move_chart.as_ref().unwrap().ui.borrow_mut().clear());
    let dropdown_shown = || buttons::all().iter().any(|k| k.contains("move-chart-sheets-button"));
    // New sheet: the name box, no sheet list; the segment is the top item.
    clear(&mut s);
    s.update(|_, _, cx| cx.notify());
    s.settle();
    let seg_new = ui(&mut s, "move-chart-mode-new").expect("segment");
    let name = ui(&mut s, "move-chart-new-name-input").expect("name box shown for a new sheet");
    assert!(seg_new.bottom() <= name.top(), "the segment is above the controls");
    assert!(!dropdown_shown(), "no sheet list for a new sheet");
    s.screenshot("move-chart-new-sheet");
    // Object: the sheet list, no name box.
    let seg_obj = ui(&mut s, "move-chart-mode-existing").unwrap();
    click_ui(&mut s, seg_obj);
    clear(&mut s);
    s.update(|_, _, cx| cx.notify());
    s.settle();
    assert!(ui(&mut s, "move-chart-new-name-input").is_none(), "name box hidden for an object");
    assert!(buttons::all().iter().any(|k| k.contains("move-chart-sheets-button")), "sheet list shown for an object");
    s.screenshot("move-chart-object");
    // And back.
    let seg_new = ui(&mut s, "move-chart-mode-new").unwrap();
    click_ui(&mut s, seg_new);
    clear(&mut s);
    s.update(|_, _, cx| cx.notify());
    s.settle();
    assert!(ui(&mut s, "move-chart-new-name-input").is_some());
}

// ---- #70 table design ----------------------------------------------------------------------------------------

fn table_book() -> Scenario {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1400., 3800.);
    s.menu("xlsx.insert-sheet");
    for (row, values) in [["item", "qty", "amount"], ["a", "1", "100"], ["b", "5", "250"], ["c", "3", "60"]].into_iter().enumerate() {
        for (col, value) in values.into_iter().enumerate() {
            s.click_cell(&ibis_doc_model::cell_name(col as u32, row as u32));
            s.enter(value, "enter");
        }
    }
    s.click_cell("A2");
    s.key("cmd-t");
    s.key("enter");
    s.menu("xlsx.table-design");
    s.settle();
    s
}

pub(crate) fn table_design_tabs_and_button_group() {
    let _probe = buttons::Probe::new();
    let mut s = table_book();
    let key = |id: &str| buttons::all().into_iter().find(|k| k.contains(id)).unwrap_or_else(|| panic!("{id} not drawn: {:?}", buttons::all()));
    let drawn = |id: &str| buttons::all().iter().any(|k| k.contains(id));
    // デザイン tab: the name to the filter button; no command buttons.
    assert!(drawn("table-filter-button") && drawn("table-header"), "design tab shows the style options");
    assert!(!drawn("table-design-resize"), "the command buttons are on the other tab");
    s.screenshot("table-design-tab-design");
    // The tabs are in the header area, above the body.
    let tab = |i: usize| buttons::all().into_iter().find(|k| k.contains("table-design-tabs") && k.ends_with(&format!(", {i})")))
        .unwrap_or_else(|| panic!("tab {i}: {:?}", buttons::all()));
    let tab1 = buttons::bounds(&tab(1)).unwrap();
    assert!(tab1.bottom() <= buttons::bounds(&key("table-header")).unwrap().top(), "tabs are above the body");
    buttons::click(&mut s, &tab(1));
    s.settle();
    assert_eq!(s.with(|e, _| e.wb().panels.table_design.as_ref().unwrap().tab), 1);
    assert!(drawn("table-design-resize") && !drawn("table-filter-button"), "the command tab replaces the body");
    let ids = ["table-design-resize", "table-design-pivot", "table-design-slicer", "table-design-timeline", "table-design-dedupe", "table-design-to-range"];
    let boxes: Vec<_> = ids.iter().map(|id| buttons::bounds(&key(id)).unwrap()).collect();
    assert!(boxes.iter().all(|b| (b.size.width - boxes[0].size.width).abs() < gpui_kit::px(0.5)), "equal widths {boxes:?}");
    assert!(boxes.windows(2).all(|w| w[1].origin.y > w[0].origin.y), "stacked");
    s.screenshot("table-design-tab-commands");
    // A command works: 範囲に変換 turns the table into a range.
    buttons::click(&mut s, &key("table-design-to-range"));
    s.settle();
    assert!(s.with(|e, _| e.current_sheet().map_or(true, |(_, sh)| sh.tables.is_empty())), "the table is converted");
    let _ = (CellValue::Empty, std::any::type_name::<SheetCommand>());
}
