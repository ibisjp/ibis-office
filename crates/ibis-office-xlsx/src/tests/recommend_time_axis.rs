// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! おすすめグラフ: a time axis (dates down a column or across a row) is the X axis, every other line a series,
//! and the line chart comes first. The layouts are those Excel 16.0.20527 gives `Shapes.AddChart2` over the same
//! cells. The synthetic workbook is `test/editing/xlsx-recommend-time-axis.xlsx` (`scripts/gen-recommend-time-axis-xlsx.py`);
//! the private workbook `test/private/graph/graph.xlsx` is checked only where it exists.
use crate::shell::scenario::Scenario;
use ibis_pptx::NewChartKind;
use ibis_doc_model::{CellRange, ShapeKind};
use crate::ext::*;

/// What the recommended-charts panel offers first and the chart it inserts, over `range` of the sheet at `index`.
struct Outcome {
    first: NewChartKind,
    /// (name, categories, values) of each series of the inserted chart.
    series: Vec<(String, usize, usize)>,
    /// The first category as shown.
    first_category: String,
}

fn recommend(s: &mut Scenario, index: usize, range: CellRange) -> Outcome {
    let sheet = s.with(move |e, _| e.presentation().slides[index].id);
    s.update(move |e, window, cx| e.select_slide(sheet, window, cx));
    s.update(move |e, _, _| {
        let slide = e.current_slide;
        e.wb_mut().cells.insert(slide, (range.first_column, range.first_row));
        e.wb_mut().cell_ends.insert(slide, (range.last_column, range.last_row));
    });
    s.menu("xlsx.recommended-charts");
    s.settle();
    if std::env::var_os("IBIS_RECOMMEND_SHOT").is_some() {
        s.screenshot(&format!("recommended-charts-sheet{index}"));
    }
    let first = s.with(|e, _| e.recommended_chart_candidates()[0].0);
    s.update(|e, w, cx| e.insert_recommended_chart(w, cx));
    s.settle();
    s.with(move |e, _| {
        let chart = e.slide().shapes.iter().rev().find_map(|sh| match &sh.kind {
            ShapeKind::Chart(c) => Some(c),
            _ => None,
        });
        let chart = chart.expect("the recommended chart is inserted");
        let series = chart.groups.iter().flat_map(|g| &g.series).map(|v| (v.name.clone(), v.categories.len(), v.values.len())).collect();
        let first_category = chart.groups.iter().flat_map(|g| &g.series).next().and_then(|v| v.categories.first().cloned()).unwrap_or_default();
        Outcome { first, series, first_category }
    })
}

fn range(first_row: u32, first_column: u32, last_row: u32, last_column: u32) -> CellRange {
    CellRange { first_row, first_column, last_row, last_column }
}

fn names(o: &Outcome) -> Vec<&str> {
    o.series.iter().map(|(n, _, _)| n.as_str()).collect()
}

/// Dates down the first column: the line chart first, the dates the X values, 売上 and 利益 the series.
pub(crate) fn synthetic_layouts() {
    let mut s = Scenario::open("test/editing/xlsx-recommend-time-axis.xlsx");
    s.size(1200., 800.);
    // 縦の時間軸 A1:C20
    let o = recommend(&mut s, 0, range(0, 0, 19, 2));
    assert_eq!(o.first, NewChartKind::Line, "dates down a column: the line chart is first");
    assert_eq!(names(&o), ["売上", "利益"], "the date column is the X axis, not a series"); // i18n-ok: Japanese test fixture, not a UI label.
    assert!(o.series.iter().all(|(_, c, v)| *c == 19 && *v == 19), "{:?}", o.series);
    assert!(o.first_category.contains("2026"), "the categories are the dates: {:?}", o.first_category);
    // 縦の時間軸 with one series only (A1:B20) is still a time axis.
    let o = recommend(&mut s, 0, range(0, 0, 19, 1));
    assert_eq!(o.first, NewChartKind::Line);
    assert_eq!(names(&o), ["売上"]); // i18n-ok: Japanese test fixture, not a UI label.
    // 横の時間軸 A1:M3: dates across the first row, one series per row.
    let o = recommend(&mut s, 1, range(0, 0, 2, 12));
    assert_eq!(o.first, NewChartKind::Line, "dates across a row: the line chart is first");
    assert_eq!(names(&o), ["売上", "利益"]); // i18n-ok: Japanese test fixture, not a UI label.
    assert!(o.series.iter().all(|(_, c, v)| *c == 12 && *v == 12), "{:?}", o.series);
    // 文字の項目: text labels, as before.
    let o = recommend(&mut s, 2, range(0, 0, 8, 2));
    assert_eq!(names(&o), ["売上", "利益"]); // i18n-ok: Japanese test fixture, not a UI label.
    assert_eq!(o.first_category, "品目1"); // i18n-ok: Japanese test fixture, not a UI label.
    // 見出しつきの連番: numbers under a text heading are a series (Excel does the same).
    let o = recommend(&mut s, 3, range(0, 0, 16, 2));
    assert_eq!(names(&o), ["x", "y", "z"]);
    // 左上が空の連番: the same numbers under a blank corner are the X values (Excel does the same).
    let o = recommend(&mut s, 4, range(0, 0, 16, 2));
    assert_eq!(names(&o), ["y", "z"]);
    assert_eq!(o.first_category, "1");
}

/// The private workbook `graph.xlsx` (not published): over every sheet's used cells a time axis gives the line
/// chart first, with the date line as the X values. Skipped where the file is absent.
pub(crate) fn private_graph() {
    let path = "test/private/graph/graph.xlsx";
    if !std::path::Path::new(path).exists() {
        eprintln!("skip: {path} is absent");
        return;
    }
    let mut s = Scenario::open(path);
    s.size(1200., 800.);
    let sheets = s.with(|e, _| e.presentation().slides.len());
    for index in 0..sheets {
        let used = s.with(move |e, _| {
            let sheet = ibis_pptx::sheet_of(e.presentation(), &ibis_pptx::quote_sheet(e.presentation().slides[index].name.as_deref().unwrap_or_default()))?;
            let cells = sheet.cells.iter().filter(|c| !matches!(c.value, ibis_doc_model::CellValue::Empty));
            let (mut r0, mut c0, mut r1, mut c1) = (u32::MAX, u32::MAX, 0, 0);
            for c in cells {
                r0 = r0.min(c.row);
                c0 = c0.min(c.column);
                r1 = r1.max(c.row);
                c1 = c1.max(c.column);
            }
            (r0 != u32::MAX).then_some(CellRange { first_row: r0, first_column: c0, last_row: r1, last_column: c1 })
        });
        let Some(used) = used else { continue };
        let o = recommend(&mut s, index, used);
        let lines = (used.last_column - used.first_column + 1).min(used.last_row - used.first_row + 1);
        eprintln!("graph.xlsx sheet {index}: first={:?} series={:?}", o.first, o.series);
        assert_eq!(o.first, NewChartKind::Line, "sheet {index}: a time axis puts the line chart first");
        // Sheet1 is the table with the dates down column A; Sheet2 is its transpose (the dates across row 1).
        if index <= 1 && sheets == 2 {
            assert_eq!(names(&o), ["売上", "利益"], "sheet {index}: the dates are the X axis, 売上 and 利益 the series"); // i18n-ok: Japanese test fixture, not a UI label.
            assert!(o.series.iter().all(|(_, c, v)| *c == 19 && *v == 19), "sheet {index}: {:?}", o.series);
        }
        assert!(!o.series.is_empty() && o.series.len() < (used.last_column - used.first_column + 1).max(used.last_row - used.first_row + 1) as usize, "sheet {index}: the date line is the X axis, not a series ({lines} lines)");
    }
}
