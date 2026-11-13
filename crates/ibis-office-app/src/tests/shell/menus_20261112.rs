// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of 挿入 > 図形 and 挿入 > グラフ of the document screen as grids of pictures (K0006, K0007): the shapes four to a row, each a
//! 4:3 picture of the shape over its name; the charts as pictures of fixed data; a click on a tile inserts. Pictures go to
//! `target/scenario/<name>/NN-<label>.png`.
use crate::shell::chrome::{MenuEntry, MenuId};
use crate::shell::scenario::{Scenario, button_probe as buttons};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::menus_20261112_docx_shape_submenu_is_a_grid_of_eight_small_tiles", shape_submenu),
        ("shell::menus_20261112_docx_text_box_item_is_above_the_pictures", text_box_item),
        ("shell::menus_20261112_pptx_text_box_item_is_above_the_pictures", pptx_text_box_item),
        ("shell::menus_20261112_docx_chart_submenu_pictures_insert", chart_submenu),
    ]
}

const DOCX: &str = "test/editing/docx-report.docx";

fn path_to(entries: &[MenuEntry], id: &str) -> Option<Vec<usize>> {
    let focusable = |e: &MenuEntry| match e {
        MenuEntry::Item(item) => item.enabled(),
        MenuEntry::Submenu(_) => true,
        _ => false,
    };
    for (i, entry) in entries.iter().enumerate() {
        if let MenuEntry::Submenu(sub) = entry {
            let downs = entries[..=i].iter().filter(|e| focusable(e)).count();
            if sub.id == id {
                return Some(vec![downs]);
            }
            if let Some(mut rest) = path_to(&sub.items, id) {
                rest.insert(0, downs);
                return Some(rest);
            }
        }
    }
    None
}

fn open_submenu(s: &mut Scenario, id: &str, label: &str) {
    let (index, path) = s.with(|e, _| {
        let menus = e.menu_bar_menus();
        let index = menus.menus.iter().position(|(m, _)| *m == MenuId::Insert).expect("the insert menu");
        (index, path_to(&menus.menus[index].1, id).unwrap_or_else(|| panic!("no submenu {id}")))
    });
    s.hover((0., 0.));
    s.update(move |e, w, cx| e.open_menu_at(index, w, cx));
    for downs in path {
        for _ in 0..downs {
            s.key("down");
        }
        s.key("right");
    }
    s.settle();
    s.screenshot(label);
}

/// The shapes and the charts (the pictures) in the body of the open document.
fn counts(s: &mut Scenario) -> (usize, usize) {
    s.with(|e, _| {
        let mut shapes = 0;
        let mut charts = 0;
        let flow = e.presentation().flow.as_ref().expect("a document");
        for block in &flow.body {
            if let ibis_doc_model::Block::Paragraph(p) = block {
                for run in &p.runs {
                    if let ibis_doc_model::FlowRunKind::Shape(shape) = &run.kind {
                        if matches!(shape.kind, ibis_doc_model::ShapeKind::Chart(_)) {
                            charts += 1;
                        } else {
                            shapes += 1;
                        }
                    }
                }
            }
        }
        (shapes, charts)
    })
}

fn center(id: &str) -> (f32, f32) {
    let b = buttons::bounds(id).unwrap_or_else(|| panic!("no tile {id}"));
    (f32::from(b.center().x), f32::from(b.center().y))
}

/// The shapes are small tiles, eight to a row, each picture half as wide as a chart's: the six lines and the rectangle and the rounded
/// rectangle share the first row, the ellipse starts the next; a click on a tile arms the shape and a drag on the page inserts it.
fn shape_submenu() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(DOCX);
    s.size(1200., 900.);
    open_submenu(&mut s, "insert.shape", "shape-submenu");
    let ids: Vec<String> = buttons::all().into_iter().filter(|id| id.starts_with("menu-item-docx.shape.") || id.starts_with("menu-item-shape.")).collect();
    assert!(ids.len() >= 12, "the lines and the shapes are tiles: {ids:?}");
    let at = |id: &str| buttons::bounds(id).unwrap_or_else(|| panic!("no tile {id}"));
    let (line, arrow, rect, round, ellipse) = (at("menu-item-docx.shape.line"), at("menu-item-docx.shape.arrow"), at("menu-item-shape.rect"), at("menu-item-shape.roundRect"), at("menu-item-shape.ellipse"));
    let curved_arrow = at("menu-item-docx.shape.curved-arrow");
    assert_eq!(line.origin.y, curved_arrow.origin.y, "the six lines share a row");
    assert!(arrow.origin.x > line.origin.x && curved_arrow.origin.x > arrow.origin.x, "left to right");
    assert_eq!(rect.origin.y, line.origin.y, "the grid is one block: the shapes fill the lines' row");
    assert_eq!(rect.origin.y, round.origin.y);
    assert!(ellipse.origin.y > round.origin.y, "eight to a row");
    // A tile is the chart tile's picture halved (116 / 2 = 58 plus the 4 px padding at both sides), a picture over a name.
    assert_eq!(f32::from(line.size.width), 58. + 8., "a tile's picture is half a chart tile's");
    assert_eq!(f32::from(round.origin.x - rect.origin.x), f32::from(rect.size.width), "tiles touch");
    assert!((f32::from(rect.size.width) / f32::from(rect.size.height)) < 1.0, "a tile is a picture over a name");
    // A click arms the shape; a drag on the page makes it.
    let before = counts(&mut s);
    s.click(center("menu-item-shape.roundRect"));
    s.settle();
    s.drag((300., 300.), (420., 380.));
    s.settle();
    s.screenshot("shape-inserted");
    let after = counts(&mut s);
    assert_eq!(after.0, before.0 + 1, "a shape was inserted ({before:?} -> {after:?})");
}

/// The charts are pictures of fixed data (the same whatever the document holds); a click on one inserts a chart.
fn chart_submenu() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(DOCX);
    s.size(1200., 900.);
    open_submenu(&mut s, "insert.chart", "chart-submenu");
    let pie = center("menu-item-pptx.insert-chart.column");
    let column = buttons::bounds("menu-item-pptx.insert-chart.column").expect("a tile");
    let stacked = buttons::bounds("menu-item-pptx.insert-chart.stacked-column").expect("a tile");
    assert_eq!(column.origin.y, stacked.origin.y, "tiles, four to a row");
    let third = buttons::bounds("menu-item-pptx.insert-chart.percent-stacked-column").expect("a tile");
    let fourth = buttons::bounds("menu-item-pptx.insert-chart.3d-clustered-column").expect("a tile");
    let fifth = buttons::bounds("menu-item-pptx.insert-chart.3d-stacked-column").expect("a tile");
    assert_eq!(column.origin.y, third.origin.y, "tiles, four to a row");
    assert_eq!(column.origin.y, fourth.origin.y, "tiles, four to a row");
    assert!(fifth.origin.y > column.origin.y, "the fifth tile is on the next row");
    // The pictures further down (lines, pies, scatter charts…): the submenu scrolled, and a picture of each stage.
    for stage in 1..=3 {
        s.wheel((636., 420.), -400.);
        s.settle();
        s.screenshot(&format!("chart-submenu-scrolled-{stage}"));
    }
    s.wheel((636., 420.), 9000.);
    s.settle();
    let before = counts(&mut s);
    s.click(pie);
    s.settle();
    s.screenshot("chart-inserted");
    let after = counts(&mut s);
    assert_eq!(after.1, before.1 + 1, "a chart was inserted ({before:?} -> {after:?})");
}

/// 挿入 > テキスト ボックス is one item above the pictures (no submenu, no vertical one); choosing it puts a horizontal text box at the caret.
fn text_box_item() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(DOCX);
    s.size(1200., 900.);
    let (index, downs) = s.with(|e, _| {
        let menus = e.menu_bar_menus();
        let index = menus.menus.iter().position(|(m, _)| *m == MenuId::Insert).expect("the insert menu");
        let entries = &menus.menus[index].1;
        let focusable = |e: &MenuEntry| match e {
            MenuEntry::Item(item) => item.enabled(),
            MenuEntry::Submenu(_) => true,
            _ => false,
        };
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "insert.text-box")).expect("a text box item, not a submenu");
        let image = entries.iter().position(|e| matches!(e, MenuEntry::Submenu(sub) if sub.id == "insert.image")).expect("the picture submenu");
        let shape = entries.iter().position(|e| matches!(e, MenuEntry::Submenu(sub) if sub.id == "insert.shape")).expect("the shape submenu");
        assert!(at < image && at < shape, "the text box is above the pictures and the shapes");
        fn has_vertical(entries: &[MenuEntry]) -> bool {
            entries.iter().any(|e| match e {
                MenuEntry::Item(i) => i.id.contains("vertical-text-box") || i.id.contains("textbox.vertical"),
                MenuEntry::Submenu(sub) => sub.id.contains("text-box") || has_vertical(&sub.items),
                _ => false,
            })
        }
        assert!(!has_vertical(entries), "no vertical text box in the menu");
        (index, entries[..=at].iter().filter(|e| focusable(e)).count())
    });
    s.hover((0., 0.));
    s.update(move |e, w, cx| e.open_menu_at(index, w, cx));
    for _ in 0..downs {
        s.key("down");
    }
    s.settle();
    s.screenshot("insert-menu");
    let before = counts(&mut s);
    s.key("enter");
    s.settle();
    s.screenshot("text-box-inserted");
    let after = counts(&mut s);
    assert_eq!(after.0, before.0 + 1, "a text box was inserted ({before:?} -> {after:?})");
}

/// K0030: on the presentation screen, 挿入 > テキスト ボックス is one item just above the pictures.
fn pptx_text_box_item() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1200., 900.);
    s.with(|e, _| {
        let menus = e.menu_bar_menus();
        let entries = &menus.menus.iter().find(|(m, _)| *m == MenuId::Insert).expect("the insert menu").1;
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Item(i) if i.id == "insert.text-box")).expect("a text box item");
        let image = entries.iter().position(|e| matches!(e, MenuEntry::Submenu(sub) if sub.id == "insert.image")).expect("the picture submenu");
        assert_eq!(at + 1, image, "the text box is just above the pictures");
    });
}
