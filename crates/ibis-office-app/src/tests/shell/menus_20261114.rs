// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the shared 挿入 > 図形 and 挿入 > グラフ grids on the three screens (K0031, K0032): the shapes four to a row and the
//! charts three to a row on the document, spreadsheet and presentation screens, each built by the one shared part; choosing a tile
//! inserts the shape or the chart on each screen. Pictures go to `target/scenario/<name>/NN-<label>.png`.
use super::menus_20261112_scenarios::center;
use crate::shell::chrome::{MenuEntry, MenuId};
use crate::shell::scenario::{Scenario, button_probe as buttons};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::menus_20261114_docx_shape_grid_is_four_columns", docx_shape_columns),
        ("shell::menus_20261114_xlsx_shape_grid_is_four_columns_and_inserts", xlsx_shapes),
        ("shell::menus_20261114_pptx_shape_grid_is_four_columns_and_arms", pptx_shapes),
        ("shell::menus_20261114_docx_chart_grid_is_three_columns", docx_chart_columns),
        ("shell::menus_20261114_xlsx_chart_grid_is_three_columns_and_inserts", xlsx_charts),
        ("shell::menus_20261114_pptx_chart_grid_is_three_columns_and_inserts", pptx_charts),
    ]
}

/// Opens the submenu `id` of the 挿入 menu with the keys. A disabled item (the presentation screen's first one) still takes a key press.
fn open_submenu(s: &mut Scenario, id: &str, label: &str) {
    fn path(entries: &[MenuEntry], id: &str) -> Option<Vec<usize>> {
        for (i, entry) in entries.iter().enumerate() {
            if let MenuEntry::Submenu(sub) = entry {
                let downs = entries[..=i].iter().filter(|e| matches!(e, MenuEntry::Item(_) | MenuEntry::Submenu(_))).count();
                if sub.id == id {
                    return Some(vec![downs]);
                }
                if let Some(mut rest) = path(&sub.items, id) {
                    rest.insert(0, downs);
                    return Some(rest);
                }
            }
        }
        None
    }
    let (index, steps) = s.with(|e, _| {
        let menus = e.menu_bar_menus();
        let index = menus.menus.iter().position(|(m, _)| *m == MenuId::Insert).expect("the insert menu");
        (index, path(&menus.menus[index].1, id).unwrap_or_else(|| panic!("no submenu {id}")))
    });
    s.hover((0., 0.));
    s.update(move |e, w, cx| e.open_menu_at(index, w, cx));
    for downs in steps {
        for _ in 0..downs {
            s.key("down");
        }
        s.key("right");
    }
    s.settle();
    s.screenshot(label);
}

const DOCX: &str = "test/editing/docx-report.docx";
const XLSX: &str = "test/editing/ibis-src.xlsx";
const PPTX: &str = "test/editing/pptx-plan.pptx";

/// The tiles of a grid in the order of the menu, by the ids of the open submenu's items.
fn rows(ids: &[&str]) -> Vec<usize> {
    let mut found: Vec<(f32, f32)> = ids.iter().map(|id| buttons::bounds(&format!("menu-item-{id}")).unwrap_or_else(|| panic!("no tile {id}"))).map(|b| (f32::from(b.origin.y), f32::from(b.origin.x))).collect();
    found.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut rows: Vec<usize> = Vec::new();
    let mut last_y = f32::NAN;
    for (y, _) in found {
        if (y - last_y).abs() < 0.5 {
            *rows.last_mut().unwrap() += 1;
        } else {
            rows.push(1);
            last_y = y;
        }
    }
    rows
}

/// The ids of the shapes' tiles in menu order: the lines, then the preset shapes up to the first ones of the grid's rows.
const SHAPE_TILES: [&str; 12] = [
    "docx.shape.line", "docx.shape.arrow", "docx.shape.elbow", "docx.shape.elbow-arrow", "docx.shape.curved", "docx.shape.curved-arrow", "shape.rect",
    "shape.roundRect", "shape.ellipse", "shape.triangle", "shape.rtTriangle", "shape.diamond",
];

fn assert_shape_columns() {
    // Four a row: 4 + 4 + 4 for the first twelve tiles.
    assert_eq!(rows(&SHAPE_TILES), vec![4, 4, 4], "the shapes' grid is four columns");
}

fn docx_shape_columns() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(DOCX);
    s.size(1200., 900.);
    open_submenu(&mut s, "insert.shape", "shape-submenu");
    assert_shape_columns();
}

fn xlsx_shapes() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(XLSX);
    s.size(1200., 900.);
    open_submenu(&mut s, "insert.shape", "shape-submenu");
    assert_shape_columns();
    let shapes = |s: &mut Scenario| s.with(|e, _| e.slide().shapes.len());
    let before = shapes(&mut s);
    // A click arms the shape (the insert mode, K0035); a drag on the sheet makes it.
    s.click(center("menu-item-shape.roundRect"));
    s.settle();
    s.drag((500., 400.), (640., 480.));
    s.settle();
    s.screenshot("shape-inserted");
    assert_eq!(shapes(&mut s), before + 1, "the rounded rectangle was put on the sheet");
    // A connector tile of the same grid inserts a connector.
    open_submenu(&mut s, "insert.shape", "shape-submenu-again");
    s.click(center("menu-item-docx.shape.elbow-arrow"));
    s.settle();
    s.drag((500., 500.), (640., 580.));
    s.settle();
    assert_eq!(shapes(&mut s), before + 2, "the elbow arrow was put on the sheet");
}

fn pptx_shapes() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(PPTX);
    s.size(1200., 900.);
    open_submenu(&mut s, "insert.shape", "shape-submenu");
    assert_shape_columns();
    let shapes = |s: &mut Scenario| s.with(|e, _| e.slide().shapes.len());
    let before = shapes(&mut s);
    // A click arms the shape; a drag on the slide makes it.
    s.click(center("menu-item-shape.roundRect"));
    s.settle();
    s.drag((500., 400.), (640., 480.));
    s.settle();
    s.screenshot("shape-inserted");
    assert_eq!(shapes(&mut s), before + 1, "the rounded rectangle was drawn on the slide");
}

/// The first rows of the charts' grid: the column kinds, three to a row.
const CHART_TILES: [&str; 6] = [
    "pptx.insert-chart.column", "pptx.insert-chart.stacked-column", "pptx.insert-chart.percent-stacked-column", "pptx.insert-chart.3d-clustered-column",
    "pptx.insert-chart.3d-stacked-column", "pptx.insert-chart.3d-percent-stacked-column",
];

fn assert_chart_columns() {
    assert_eq!(rows(&CHART_TILES), vec![3, 3], "the charts' grid is three columns");
}

fn docx_chart_columns() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(DOCX);
    s.size(1200., 900.);
    open_submenu(&mut s, "insert.chart", "chart-submenu");
    assert_chart_columns();
}

fn charts_in(s: &mut Scenario) -> usize {
    s.with(|e, _| e.slide().shapes.iter().filter(|shape| matches!(shape.kind, ibis_doc_model::ShapeKind::Chart(_))).count())
}

fn xlsx_charts() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(XLSX);
    s.size(1200., 900.);
    open_submenu(&mut s, "insert.chart", "chart-submenu");
    assert_chart_columns();
    let before = charts_in(&mut s);
    s.click(center("menu-item-pptx.insert-chart.stacked-column"));
    s.settle();
    s.screenshot("chart-inserted");
    assert_eq!(charts_in(&mut s), before + 1, "a chart was put on the sheet");
}

fn pptx_charts() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(PPTX);
    s.size(1200., 900.);
    open_submenu(&mut s, "insert.chart", "chart-submenu");
    assert_chart_columns();
    let before = charts_in(&mut s);
    s.click(center("menu-item-pptx.insert-chart.stacked-column"));
    s.settle();
    s.screenshot("chart-inserted");
    assert_eq!(charts_in(&mut s), before + 1, "a chart was put on the slide");
}
