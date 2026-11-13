// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The chart's data in the workbook editor (the presentation screen's chart pane embeds the XLSX screen's grid; a category typed
//! under the table is the XLSX screen's `after_table_entry`). It names two formats, so it is the app's, not the presentation
//! crate's (tasks/doc/crate-split-plan.md, 14.2).

use ibis_doc_model::ShapeKind;

use crate::shell::scenario::Scenario;
use crate::xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("pptx::chart_data_in_the_workbook", chart_data_in_the_workbook)]
}

/// データの編集 in the workbook editor: a new chart's data opened in the
/// chart pane's embedded grid (its ranges framed), a value changed and a category
/// typed under the table (which grows, and the chart with it), the window
/// closed; saved, the chart and its workbook read back so.
fn chart_data_in_the_workbook() {
    use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand};
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.page(2);
    s.update(|e, w, cx| e.insert_slide_chart(w, cx));
    s.update(|e, w, cx| e.close_chart_panel(w, cx));
    s.toolbar("chart-data");
    let (view, shape) = s.with(|e, _| {
        let d = e.shape_edit.chart_data.as_ref().expect("the data grid");
        (d.view.clone(), d.shape)
    });
    let ranges = s.with(|_, cx| view.read(cx).chart_ranges.clone()).expect("the ranges");
    assert_eq!(ranges.iter().map(|r| r.values.clone().unwrap()).collect::<Vec<_>>(), vec!["Sheet1!$B$2:$B$5", "Sheet1!$C$2:$C$5", "Sheet1!$D$2:$D$5"]);
    s.screenshot("data");
    let set = |row: u32, column: u32, value: CellValue| CellEdit { row, column, input: CellInput::Value(value) };
    s.update(|_, _, cx| view.update(cx, |d, cx| {
        let slide = d.current_slide;
        d.run(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![set(3, 2, CellValue::Number(9.5))] })], cx);
    }));
    let chart = |s: &mut Scenario| {
        s.with(move |e, _| match &e.slide().shape(shape).unwrap().kind {
            ShapeKind::Chart(c) => (**c).clone(),
            _ => panic!("a chart"),
        })
    };
    assert_eq!(chart(&mut s).groups[0].series[1].values[2], Some(ibis_doc_model::Number(9.5)));
    // A category more, typed under the table.
    s.update(|_, _, cx| view.update(cx, |d, cx| {
        let slide = d.current_slide;
        let input = CellInput::Value(CellValue::Text("分類 5".into()));
        d.run(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: vec![CellEdit { row: 5, column: 0, input: input.clone() }] })], cx);
        d.after_table_entry(slide, 5, 0, &input, cx);
        d.run(vec![Command::Sheet(SheetCommand::SetCells { slide, edits: (1..=3).map(|c| set(5, c, CellValue::Number(c as f64))).collect() })], cx);
    }));
    let grown = s.with(|_, cx| view.read(cx).chart_ranges.clone()).unwrap();
    assert_eq!(grown[0].values.as_deref(), Some("Sheet1!$B$2:$B$6"));
    s.screenshot("grown");
    let c = chart(&mut s);
    assert_eq!(c.groups[0].series[0].categories.last().map(String::as_str), Some("分類 5"));
    assert_eq!(c.groups[0].series[2].values.last(), Some(&Some(ibis_doc_model::Number(3.0))));
    drop(view);
    s.update(|e, w, cx| e.close_chart_panel(w, cx));
    assert!(s.with(|e, _| e.shape_edit.chart_data.is_none()));
    s.screenshot("chart");
    s.save_as("chart-data.pptx");
    let saved = s.reopen();
    let read = saved.slides[1].shapes.iter().find_map(|sh| match &sh.kind {
        ShapeKind::Chart(c) => Some((**c).clone()),
        _ => None,
    });
    assert_eq!(read.as_ref(), Some(&c));
    crate::pptx::scenarios::undo_all_and_redo(&mut s);
}

