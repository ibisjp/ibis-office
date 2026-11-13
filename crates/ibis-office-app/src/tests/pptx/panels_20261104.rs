// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 2026-11-04: the font replacement's lists grouped by use (#48), the crop panel's unit and size and the crop on the
//! slide (#52), the chart data panel handing the grid to the workbook (#53), the pages of the master view as pictures (#58).
use super::{png, press_panel_button};
use crate::shell::scenario::Scenario;
use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PlatformInput, point, px};
use ibis_doc_model::{Crop, ShapeKind};
use crate::pptx::ext::*;
use crate::xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::panels_20261104_font_replace_lists_are_grouped_by_use", font_replace_lists),
        ("pptx::panels_20261104_crop_panel_unit_and_size", crop_panel_unit),
        ("pptx::panels_20261104_crop_on_the_slide", crop_on_the_slide),
        ("pptx::panels_20261104_chart_data_panel_uses_the_workbook_grid", chart_data_panel),
        ("pptx::panels_20261104_chart_without_workbook_gets_a_grid", chart_without_workbook),
        ("pptx::panels_20261104_master_pages_are_a_grid_of_pictures", master_grid),
    ]
}

fn font_replace_lists() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    let used = s.with(|e, _| crate::shell::replace_fonts::presentation_used_fonts(e.presentation()));
    // The "replace from" list: only the fonts the file uses, in a section of their own.
    let from = crate::shell::font_list::build_grouped_font_menu_entries("panel-font-test", &used, false, None, |_, _, _, _| {});
    let ids = |entries: &[crate::shell::chrome::MenuEntry]| -> Vec<&'static str> {
        entries.iter().filter_map(|e| if let crate::shell::chrome::MenuEntry::Item(i) = e { Some(i.id) } else { None }).collect()
    };
    let from_ids = ids(&from);
    if !used.is_empty() {
        assert_eq!(from_ids[0], "font.section.used");
        assert_eq!(from_ids.len(), 1 + used.len(), "only the used fonts: {from_ids:?}");
        assert!(!from_ids.contains(&"font.section.system"));
    }
    // The "replace with" list: the used fonts, then all of them under "all".
    let to = crate::shell::font_list::build_grouped_font_menu_entries("panel-font-test", &used, true, None, |_, _, _, _| {});
    let to_ids = ids(&to);
    assert!(to_ids.contains(&"font.section.system"));
    assert!(to_ids.len() > from_ids.len());
    let all = to.iter().find_map(|e| if let crate::shell::chrome::MenuEntry::Item(i) = e { (i.id == "font.section.system").then(|| i.label.to_string()) } else { None });
    assert!(matches!(all.as_deref(), Some("すべて" | "All")), "{all:?}"); // i18n-ok（試験：辞書の文言の確認）
    // The panel opens with the first used font as the font replaced; a replacement changes the file and can be undone.
    s.update(|e, w, cx| e.open_replace_fonts_panel(w, cx));
    s.screenshot("font-replace");
    if let Some(first) = used.first().cloned() {
        let before = s.with(|e, _| crate::shell::replace_fonts::replace_presentation_fonts_commands(e.presentation(), &first, "Arial Black").len());
        assert!(before > 0 || first.eq_ignore_ascii_case("Arial Black"));
    }
}

fn crop_panel_unit() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| e.insert_picture("custom.png", png(160, 120, [0x44, 0x88, 0xaa]), cx));
    let original = s.with(|e, _| e.selected_shapes()[0].clone());
    s.update(|e, w, cx| e.open_crop_panel(w, cx));
    let fields = s.with(|e, _| e.shape_edit.crop_panel.as_ref().unwrap().fields.clone());
    // In percent first; the unit switched to pixels: 10 % of a 160 px wide picture is 16 px, of a 120 px high one 12 px.
    s.update(|_, w, cx| { for (f, v) in fields.iter().zip(["10", "10", "0", "0"]) { f.update(cx, |f, cx| f.set_value(v, w, cx)); } });
    s.update(|e, w, cx| e.set_crop_unit(crate::shell::crop_units::CropUnit::Pixels, w, cx));
    let shown = s.with(|_, cx| fields.iter().map(|f| f.read(cx).value().to_string()).collect::<Vec<_>>());
    assert_eq!(shown, ["16", "12", "0", "0"]);
    s.screenshot("crop-panel-px");
    // Typed in pixels: 32 px off the left (20 %) and 24 px off the bottom (20 %); applied as the file's fractions.
    s.update(|_, w, cx| { for (f, v) in fields.iter().zip(["32", "0", "0", "24"]) { f.update(cx, |f, cx| f.set_value(v, w, cx)); } });
    press_panel_button(&mut s, "pptx-crop-apply");
    let changed = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &changed.kind else { panic!("picture") };
    assert_eq!(p.crop, Crop { left: 20_000, top: 0, right: 0, bottom: 20_000 });
    // What stays is not moved or stretched: the frame shrinks to the part that is left (20 % off the left, 20 % off the bottom).
    let o = original.frame;
    assert_eq!(
        changed.frame,
        ibis_doc_model::EmuRect { x: o.x + o.width / 5, y: o.y, width: o.width * 4 / 5, height: o.height * 4 / 5 },
        "the numeric crop keeps the part that stays where it was"
    );
    // Reopened, the fields show the crop in percent again (the default unit): 20, 0, 0, 20.
    s.update(|e, w, cx| e.open_crop_panel(w, cx));
    let fields = s.with(|e, _| e.shape_edit.crop_panel.as_ref().unwrap().fields.clone());
    let shown = s.with(|_, cx| fields.iter().map(|f| f.read(cx).value().to_string()).collect::<Vec<_>>());
    assert_eq!(shown, ["20", "0", "0", "20"]);
    s.screenshot("crop-panel-percent");
    press_panel_button(&mut s, "pptx-crop-cancel");
}

fn crop_on_the_slide() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.update(|e, _, cx| e.insert_picture("custom.png", png(160, 120, [0x44, 0x88, 0xaa]), cx));
    let original = s.with(|e, _| e.selected_shapes()[0].clone());
    s.update(|e, w, cx| e.begin_crop_mode(w, cx));
    assert!(s.with(|e, _| e.is_crop_mode()));
    s.settle();
    // Drag the left edge's handle a tenth of the picture's width to the right.
    let (from, to) = s.with(|e, _| {
        let layout = e.slide_layout().expect("laid out");
        let kept = e.crop_mode_kept().expect("crop mode");
        let at = |x: i64, y: i64| (f32::from(layout.origin.x + layout.to_px(x)), f32::from(layout.origin.y + layout.to_px(y)));
        (at(kept.x, kept.y + kept.height / 2), at(kept.x + kept.width / 10, kept.y + kept.height / 2))
    });
    s.screenshot("crop-before-drag");
    // The press takes the handle (not the picture, which would move it under the frame).
    s.dispatch(vec![
        PlatformInput::MouseMove(MouseMoveEvent { position: point(px(from.0), px(from.1)), ..Default::default() }),
        PlatformInput::MouseDown(MouseDownEvent { button: MouseButton::Left, position: point(px(from.0), px(from.1)), click_count: 1, ..Default::default() }),
    ]);
    assert_eq!(s.with(|e, _| e.crop_mode_drag_handle()), Some(Some(ibis_doc_model::ResizeHandle::Left)));
    s.dispatch(vec![
        PlatformInput::MouseMove(MouseMoveEvent { position: point(px(to.0), px(to.1)), pressed_button: Some(MouseButton::Left), ..Default::default() }),
        PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: point(px(to.0), px(to.1)), click_count: 1, ..Default::default() }),
    ]);
    let crop = s.with(|e, _| e.crop_mode_crop().expect("crop mode"));
    assert!((crop.left - 10_000).abs() <= 400 && crop.top == 0 && crop.right == 0 && crop.bottom == 0, "{crop:?}");
    s.screenshot("crop-on-slide");
    // Enter keeps it: the picture's crop and frame (its right and bottom edges stay), one step to undo.
    s.key("enter");
    assert!(!s.with(|e, _| e.is_crop_mode()));
    let changed = s.with(|e, _| e.selected_shapes()[0].clone());
    let ShapeKind::Picture(p) = &changed.kind else { panic!("picture") };
    assert_eq!(p.crop, crop);
    assert_eq!(changed.frame.right(), original.frame.right());
    assert_eq!(changed.frame.bottom(), original.frame.bottom());
    assert!(changed.frame.width < original.frame.width);
    s.key("cmd-z");
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), original);
    // Escape gives it up.
    s.update(|e, w, cx| e.begin_crop_mode(w, cx));
    s.settle();
    s.drag(from, to);
    s.key("escape");
    assert!(!s.with(|e, _| e.is_crop_mode()));
    assert_eq!(s.with(|e, _| e.selected_shapes()[0].clone()), original);
}

fn chart_data_panel() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    s.update(|e, w, cx| e.insert_slide_chart(w, cx));
    // The panel has no grid of its own: the workbook's grid (the spreadsheet screen's editor, grid only) is embedded in it.
    let workbook = s.with(|e, _| e.shape_edit.chart_panel.as_ref().and_then(|p| p.grid.clone())).expect("the embedded grid");
    assert!(s.with(|_, cx| workbook.read(cx).is_workbook() && workbook.read(cx).grid_only), "the spreadsheet screen's editor holds the data");
    // One sheet: the grid has no sheet tabs; a workbook read with several sheets shows them.
    assert!(!s.with(|_, cx| workbook.read(cx).shows_sheet_tabs()));
    s.screenshot("chart-data-panel");
    // Opening the data again shows the same grid (no second workbook, no window).
    s.update(|e, w, cx| e.open_chart_data(w, cx));
    let again = s.with(|e, _| e.shape_edit.chart_data.as_ref().map(|d| d.view.clone())).expect("the data grid");
    assert_eq!(again.entity_id(), workbook.entity_id());
    // Typing in the grid: the cell's text is put in the chart, and the chart's pane keeps the grid.
    s.update(|_, _, cx| workbook.update(cx, |d, cx| {
        let slide = d.current_slide;
        d.run(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetCells { slide, edits: vec![ibis_doc_model::CellEdit { row: 1, column: 1, input: ibis_doc_model::CellInput::Value(ibis_doc_model::CellValue::Number(12.5)) }] })], cx);
    }));
    let first = s.with(|e, _| e.selected_chart().and_then(|(_, _, c)| c.groups[0].series[0].values[0]));
    assert_eq!(first, Some(ibis_doc_model::Number(12.5)));
    s.screenshot("chart-data-panel-edited");
    s.update(|_, w, cx| workbook.update(cx, |d, cx| d.insert_sheet(w, cx)));
    assert!(s.with(|_, cx| workbook.read(cx).shows_sheet_tabs()));
    s.screenshot("chart-data-panel-two-sheets");
}

/// A chart that has only cached values (no workbook in the file) is edited in the same grid: a workbook is made from
/// the cache in memory, an edit goes into the chart, and a save writes the workbook into the file.
fn chart_without_workbook() {
    let mut s = Scenario::open("test/corpus-report/pptx/chart-pie-auto-plot.pptx");
    let shape = s.with(|e, _| e.slide().shapes.iter().position(|sh| matches!(sh.kind, ShapeKind::Chart(_)))).expect("a chart");
    s.click_shape(shape);
    s.update(|e, w, cx| e.open_chart_data(w, cx));
    let grid = s.with(|e, _| e.shape_edit.chart_panel.as_ref().and_then(|p| p.grid.clone())).expect("the grid, for a chart with no workbook too");
    assert!(s.with(|_, cx| grid.read(cx).is_workbook() && grid.read(cx).grid_only));
    s.screenshot("chart-no-workbook-grid");
    s.chart_grid_set(0, Some(0), "77");
    s.chart_grid_set(0, None, "Edited");
    let read = |p: &ibis_doc_model::Presentation| -> (Option<f64>, String) {
        let ShapeKind::Chart(c) = &p.slides[0].shapes[shape].kind else { panic!("a chart") };
        let x = &c.groups[0].series[0];
        (x.values[0].map(|n| n.0), x.name.clone())
    };
    assert_eq!(s.with(|e, _| read(e.presentation())), (Some(77.0), "Edited".to_string()));
    s.screenshot("chart-no-workbook-edited");
    let path = s.save_as("chart-no-workbook.pptx");
    let package = ibis_pptx::ibis_ooxml::Package::open(std::fs::read(&path).unwrap()).unwrap();
    assert!(package.names().any(|n| n.starts_with("ppt/embeddings/")), "the workbook is in the file");
    assert_eq!(read(&s.reopen()), (Some(77.0), "Edited".to_string()));
}

fn master_grid() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.menu("pptx.master-view");
    assert!(s.with(|e, _| e.is_master_view()));
    let pages = s.with(|e, _| e.master_view_page_count());
    assert!(pages > 2);
    s.update(|e, _, cx| { e.sidebar_open = true; cx.notify(); });
    s.settle();
    // Every page but the one shown has its own picture.
    let (current, thumbs) = s.with(|e, _| e.master_view_grid()).expect("the grid");
    assert_eq!(thumbs, pages - 1, "current {current}");
    s.screenshot("master-grid");
    // Arrows move the page shown (the list on the left is one column): down is the next page.
    s.update(|e, w, cx| e.focus_master_grid(w, cx));
    s.key("right");
    assert_eq!(s.with(|e, _| e.master_view_grid()).unwrap().0, (current + 1).min(pages - 1));
    let at = s.with(|e, _| e.master_view_grid()).unwrap().0;
    s.key("down");
    assert_eq!(s.with(|e, _| e.master_view_grid()).unwrap().0, if at + 1 < pages { at + 1 } else { at });
    s.key("home");
    assert_eq!(s.with(|e, _| e.master_view_grid()).unwrap().0, 0);
    s.update(|e, w, cx| e.close_master_view(w, cx));
}
