// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the context menus and the ▾ of the menu bar (menu bar redesign, stage 3, `tasks/doc/menu-redesign`): the coloured
//! menus that come and go with the selection (a table, a picture, a shape), that they never open by themselves, that one
//! that is open closes when its target goes away and another menu stays open, the rows of every context menu, and the menus
//! folded into the ▾ in a narrow window and brought back when it widens. Pictures: `target/scenario/shell__*/`.
use ibis_doc_model::FlowPos;

use super::{button_probe as buttons, Scenario};
use crate::docx::scenarios::DocSteps;
use crate::shell::chrome::MenuId;
use crate::shell::menu_bar::{BarMenu, OVERFLOW_IX};
use crate::shell::menu_context::ContextMenu;
use crate::shell::menu_layout::{layout_context, LayoutItem};
use ibis_pptx::Kind;
use crate::docx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::contexts_docx_table_picture", docx_table_picture),
        ("shell::contexts_pptx_shape", pptx_shape),
        ("shell::contexts_xlsx_shape", xlsx_shape),
        ("shell::context_menu_closes_when_target_gone", closes_when_target_gone),
        ("shell::context_rows_within_limit", rows_within_limit),
        ("shell::menu_overflow_order_and_restore", overflow_order_and_restore),
        ("shell::menu_overflow_open", overflow_open),
    ]
}

/// A 1 × 1 PNG (a picture to put in a document).
const PNG: [u8; 69] = [
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xff, 0xff, 0x3f, 0x00, 0x05, 0xfe, 0x02, 0xfe, 0xa7, 0x35, 0x81, 0x84, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// The names (keys of the tables of words) of the menus the bar shows, in order.
fn bar_keys(s: &mut Scenario) -> Vec<&'static str> {
    s.with(|e, _| e.bar_items().iter().map(|(m, _)| m.key()).collect())
}

/// The context menus the bar shows.
fn contexts(s: &mut Scenario) -> Vec<ContextMenu> {
    s.with(|e, _| e.bar_items().iter().filter_map(|(m, _)| if let BarMenu::Context(c) = m { Some(*c) } else { None }).collect())
}

fn menus_open(s: &mut Scenario) -> bool {
    s.with(|e, _| e.open_menu.is_some())
}

/// A picture selected in a table of a document: Table Design, Table Layout and Picture Format come, in that order, and go
/// when the selection moves out of the table.
fn docx_table_picture() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1400., 900.);
    assert!(contexts(&mut s).is_empty(), "nothing selected, no context menu");
    let (id, _) = s.paragraph(0);
    s.click_text(FlowPos::new(id, 0));
    s.menu("docx.table.insert-2x2");
    assert_eq!(contexts(&mut s), vec![ContextMenu::TableDesign, ContextMenu::TableLayout], "the caret in a table");
    // A picture in a cell, selected as a click on it selects it (the one position the picture takes).
    let at = s.with(|e, _| e.text_selection().expect("a caret").1);
    s.update(|e, _, cx| e.doc_insert_picture(PNG.to_vec(), ibis_doc_model::MediaFormat::Png, cx));
    s.update(move |e, _, cx| {
        e.set_caret(at, false, cx);
        e.set_caret(FlowPos::new(at.para, at.offset + 1), true, cx);
    });
    assert_eq!(contexts(&mut s), vec![ContextMenu::TableDesign, ContextMenu::TableLayout, ContextMenu::PictureFormat], "a picture in a table: three menus");
    let keys = bar_keys(&mut s);
    assert_eq!(&keys[keys.len() - 3..], ["menu.context.table-design", "menu.context.table-layout", "menu.context.picture-format"], "after the static menus");
    assert!(!menus_open(&mut s), "they come without opening by themselves");
    s.hover((0., 0.));
    s.screenshot("docx-table-picture-bar");
    // Each has lines to show.
    let rows: Vec<(ContextMenu, usize)> = s.with(|e, _| e.bar_items().into_iter().filter_map(|(m, entries)| if let BarMenu::Context(c) = m { Some((c, entries.len())) } else { None }).collect());
    for (menu, n) in &rows {
        assert!(*n >= 1, "{menu:?} has no lines");
    }
    // Open each and take a picture of it.
    let first_context = bar_keys(&mut s).iter().position(|k| k.starts_with("menu.context.")).unwrap();
    for (i, (menu, _)) in rows.iter().enumerate() {
        s.update(move |e, w, cx| e.open_menu_at(first_context + i, w, cx));
        s.screenshot(&format!("docx-{}", menu.key().trim_start_matches("menu.context.")));
        s.update(|e, _, cx| e.close_menu(cx));
    }
    // The caret out of the table: the menus go.
    for n in 0..30 {
        let (id, _) = s.paragraph(n);
        s.click_text(FlowPos::new(id, 0));
        if s.with(|e, _| e.table_at_caret().is_none()) {
            break;
        }
    }
    assert!(s.with(|e, _| e.table_at_caret().is_none()), "a paragraph outside the table was found");
    assert!(contexts(&mut s).is_empty(), "the caret out of the table: the menus go");
}

/// A shape of a slide: Shape Format, and only while it is selected; several of different kinds give the shape's menu too.
fn pptx_shape() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(1400., 900.);
    assert!(contexts(&mut s).is_empty());
    let n = s.with(|e, _| e.slide().shapes.len());
    assert!(n > 0, "the slide has shapes");
    s.select_shape(0);
    assert!(s.with(|e, _| !e.selected_ids().is_empty()), "a shape is selected");
    let found = contexts(&mut s);
    assert!(found.contains(&ContextMenu::ShapeFormat) || found.contains(&ContextMenu::PictureFormat) || found.contains(&ContextMenu::TableDesign) || found.contains(&ContextMenu::ChartDesign), "{found:?}");
    assert!(!menus_open(&mut s), "no menu opens by itself");
    s.screenshot("pptx-shape-bar");
    s.update(|e, _, cx| {
        e.set_selection(Vec::new());
        cx.notify();
    });
    assert!(contexts(&mut s).is_empty(), "the selection removed, the menus go");
}

/// A shape on a sheet: Shape Format.
fn xlsx_shape() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1400., 900.);
    assert!(contexts(&mut s).is_empty(), "a cell: no context menu");
    s.update(|e, _, cx| {
        let id = e.document.ids().new_shape_id();
        let frame = ibis_doc_model::EmuRect::new(1_000_000, 1_000_000, 1_500_000, 800_000);
        let slide = e.current_slide;
        e.run(vec![ibis_doc_model::Command::InsertShape { slide, index: e.slide().shapes.len(), shape: ibis_doc_model::Shape::text_box(id, frame, "x") }], cx);
        e.set_selection(vec![id]);
        cx.notify();
    });
    assert_eq!(contexts(&mut s), vec![ContextMenu::ShapeFormat]);
    s.update(|e, _, cx| {
        e.set_selection(Vec::new());
        cx.notify();
    });
    assert!(contexts(&mut s).is_empty());
}

/// A context menu that is open closes when its target is gone (and the bar says why); another menu that is open stays open
/// when context menus come and go.
fn closes_when_target_gone() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1400., 900.);
    let shape = |s: &mut Scenario| {
        s.update(|e, _, cx| {
            let id = e.document.ids().new_shape_id();
            let frame = ibis_doc_model::EmuRect::new(1_000_000, 1_000_000, 1_500_000, 800_000);
            let slide = e.current_slide;
            e.run(vec![ibis_doc_model::Command::InsertShape { slide, index: e.slide().shapes.len(), shape: ibis_doc_model::Shape::text_box(id, frame, "x") }], cx);
            e.set_selection(vec![id]);
            cx.notify();
        })
    };
    // Another menu open: the context menu comes and goes without closing it.
    s.update(|e, w, cx| e.open_menu_at(0, w, cx));
    shape(&mut s);
    assert_eq!(s.with(|e, _| e.open_menu.as_ref().map(|o| o.index())), Some(0), "the File menu stays open when a context menu comes");
    s.update(|e, _, cx| {
        e.set_selection(Vec::new());
        cx.notify();
    });
    assert_eq!(s.with(|e, _| e.open_menu.as_ref().map(|o| o.index())), Some(0), "and when it goes");
    s.update(|e, _, cx| e.close_menu(cx));
    // The open context menu itself: closes with its target.
    shape(&mut s);
    let ix = bar_keys(&mut s).iter().position(|k| *k == "menu.context.shape-format").expect("the shape's menu");
    s.update(move |e, w, cx| e.open_menu_at(ix, w, cx));
    assert!(menus_open(&mut s));
    s.update(|e, _, cx| {
        e.set_selection(Vec::new());
        cx.notify();
    });
    assert!(!menus_open(&mut s), "the open context menu closes when its target is gone");
    assert!(s.with(|e, _| e.menu_notice.is_some()), "and the bar says why");
    s.screenshot("closed-with-notice");
}

/// A context menu has 25 rows at most at its top (the lines of its table; the submenus' inner lines not counted).
fn rows_within_limit() {
    for kind in [Kind::Document, Kind::Workbook, Kind::Presentation] {
        for spec in layout_context(kind) {
            let rows = spec.items.iter().filter(|i| !matches!(i, LayoutItem::Sep)).count();
            assert!(rows <= 25, "{kind:?} {:?}: {rows} rows", spec.id);
        }
    }
}

/// In a narrow window the Help menu goes first into the ▾, then the least used menus; File, Edit, Insert, View and Window never; a
/// wide window brings them back as they were.
fn overflow_order_and_restore() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1400., 900.);
    let all = bar_keys(&mut s);
    let shown = |s: &mut Scenario| -> (Vec<&'static str>, bool) {
        let keys = bar_keys(s);
        let visible: Vec<&'static str> = keys.into_iter().filter(|k| buttons::bounds(&format!("menu-button-{k}")).is_some()).collect();
        (visible, buttons::bounds("menu-button-overflow").is_some())
    };
    s.settle();
    let (wide, more) = shown(&mut s);
    assert_eq!(wide, all, "every menu shows in a wide window");
    assert!(!more, "and there is no ▾ (and the Help menu is where it is)");
    // A window a little too narrow: only Help goes.
    let total: f32 = 1400.;
    let mut width = total;
    let mut previous = wide.clone();
    let mut order: Vec<&'static str> = Vec::new();
    while width > 300. {
        width -= 20.;
        s.size(width, 900.);
        s.settle();
        let (now, more) = shown(&mut s);
        for gone in previous.iter().filter(|k| !now.contains(k)) {
            order.push(gone);
        }
        assert_eq!(more, now.len() < all.len(), "the ▾ is there exactly when a menu is folded (width {width})");
        previous = now;
    }
    assert_eq!(order.first().copied(), Some("menu.help"), "Help is the first folded: {order:?}");
    for keep in ["menu.file", "menu.edit", "menu.insert", "menu.view", "menu.window"] {
        assert!(!order.contains(&keep) || width < 400., "{keep} folded too early: {order:?}");
    }
    // Back to wide: the same bar as before.
    s.size(1400., 900.);
    s.settle();
    let (again, more) = shown(&mut s);
    assert_eq!(again, all, "widening brings every menu back");
    assert!(!more);
}

/// The ▾ opens the folded menus as submenus; the picture of it.
fn overflow_open() {
    let mut s = Scenario::open("test/editing/pptx-plan.pptx");
    s.size(560., 700.);
    s.settle();
    let hidden = s.with(|e, _| e.bar_items().len());
    assert!(hidden > 0);
    s.update(|e, w, cx| e.open_menu_at(OVERFLOW_IX, w, cx));
    assert!(menus_open(&mut s), "the ▾ opens a menu");
    let entries = s.with(|e, _| e.open_menu.as_ref().map(|o| o.index()));
    assert_eq!(entries, Some(OVERFLOW_IX));
    s.screenshot("overflow-open");
    s.update(|e, _, cx| e.close_menu(cx));
    let _ = MenuId::Help;
}
