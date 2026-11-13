// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue #105: the box of the cell being edited. It stretches to the right edge of the window, wraps its text (no `#` fill),
//! is as high as its text needs up to the main view (the area of the cells), is brought up by scrolling the sheet when it would
//! hang below the view, always shows a scroll bar when the text is longer than it, and keeps the room of the bar at its right.
use super::Scenario;
use crate::xlsx::scenarios::SheetSteps;
use gpui_kit::*;
use crate::xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::issue105_editor_long_one_line", long_one_line),
        ("xlsx::issue105_editor_tall_text", tall_text),
        ("xlsx::issue105_editor_near_bottom", near_bottom),
        ("xlsx::issue105_editor_panes", panes),
        ("xlsx::issue105_editor_zoom", zoom),
        ("xlsx::issue105_editor_wheel_keeps_working", wheel_keeps_working),
    ]
}

const BAR: f32 = 10.0;

fn workbook() -> Scenario {
    let mut s = Scenario::start_view();
    s.size(1000., 700.);
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    s
}

/// (box, workspace, laid out text, scroll of the text) of the cell being edited.
fn geometry(s: &mut Scenario) -> (Bounds<Pixels>, Bounds<Pixels>, std::rc::Rc<crate::xlsx::sheet_view::EditLayout>, f32) {
    s.settle();
    s.with(|e, _| {
        let c = e.cell_edit().expect("editing");
        (c.rect.get().expect("the box is placed"), e.workspace_bounds.unwrap(), c.layout.borrow().clone().unwrap(), c.scroll_y.get())
    })
}

fn paste(s: &mut Scenario, text: &str) {
    s.update(|_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.to_string())));
    s.key("cmd-v");
}

fn lines_of(n: usize) -> String {
    (1..=n).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n")
}

/// The box is inside the main view, and ends at the right of the window with the room of its bar.
fn within_view(tag: &str, rect: Bounds<Pixels>, ws: Bounds<Pixels>) {
    assert!(f32::from(rect.bottom()) <= f32::from(ws.bottom()) - crate::xlsx::sheet_ui::SCROLLBAR_ROOM + 1.0, "{tag}: not below the view: {rect:?} in {ws:?}");
    assert!(f32::from(rect.right()) <= f32::from(ws.right()) + 1.0, "{tag}: not right of the view: {rect:?} in {ws:?}");
    assert!(f32::from(rect.top()) >= f32::from(ws.top()) - 1.0, "{tag}: not above the view: {rect:?}");
}

/// A very long formula on one line: it wraps within the window, shows its whole text, and is as high as its last line.
fn long_one_line() {
    let mut s = workbook();
    s.click_cell("B3");
    s.key("f2");
    let text = format!("={}", vec!["100000000"; 120].join("+"));
    paste(&mut s, &text);
    let (rect, ws, layout, scroll) = geometry(&mut s);
    let shown: String = layout.lines.iter().map(|l| l.pieces.iter().map(|p| p.shaped.as_ref().map_or(String::new(), |sh| sh.text.to_string())).collect::<String>()).collect();
    assert_eq!(shown, text, "all of the text shows, wrapped, no #");
    assert!(layout.lines.len() > 2);
    within_view("long line", rect, ws);
    assert!(f32::from(rect.size.width) >= layout.width + BAR - 2.0, "the room of the bar is kept at the right: {:?} vs {}", rect.size.width, layout.width);
    assert!((f32::from(ws.right()) - f32::from(rect.right())).abs() < 30.0, "the box stretches to the right edge of the window");
    // As high as the last line needs (no more than the text's own height + the border).
    assert!((f32::from(rect.size.height) - layout.height).abs() <= 2.0, "height {:?} vs the text's {}", rect.size.height, layout.height);
    assert_eq!(scroll, 0.0);
    s.screenshot("issue105-long-one-line");
}

/// More lines than the main view holds: the box is the height of the view, the bar is shown, the caret is followed.
fn tall_text() {
    let mut s = workbook();
    s.click_cell("B2");
    s.key("f2");
    paste(&mut s, &lines_of(120));
    let (rect, ws, layout, _) = geometry(&mut s);
    within_view("tall", rect, ws);
    assert!(layout.height > f32::from(ws.size.height), "the text is taller than the view");
    let head = crate::xlsx::sheet_ui::COLUMN_HEADER_HEIGHT;
    assert!((f32::from(rect.top()) - (f32::from(ws.top()) + head)).abs() <= 2.0, "the box starts under the headers: {rect:?} {ws:?}");
    assert!((f32::from(rect.bottom()) - (f32::from(ws.bottom()) - crate::xlsx::sheet_ui::SCROLLBAR_ROOM)).abs() <= 4.0, "the box is as high as the main view: {rect:?} {ws:?}");
    s.screenshot("issue105-tall-text");
    // The caret is at the end: the text is scrolled to show the last line.
    let scroll = geometry(&mut s).3;
    assert!(scroll > 0.0, "scrolled to the caret at the end");
    s.key("ctrl-home");
    let scroll = geometry(&mut s).3;
    assert!(scroll < 1.0, "back to the top with the caret: {scroll}");
    s.screenshot("issue105-tall-text-top");
}

/// A cell low in the window with several lines: the sheet scrolls so the box stays above the bottom.
fn near_bottom() {
    let mut s = workbook();
    // The row of the cell about 40 pixels above the bottom of the view.
    let row = s.with(|e, _| {
        let ws = e.workspace_bounds.unwrap();
        e.cell_under(point(ws.origin.x + px(120.0), ws.bottom() - px(40.0))).expect("a cell there").1
    });
    s.click_cell(&format!("B{}", row + 1));
    let before = s.with(|e, _| e.workspace_scroll.offset().y);
    s.key("f2");
    paste(&mut s, &lines_of(10));
    let (rect, ws, _, _) = geometry(&mut s);
    within_view("near bottom", rect, ws);
    let after = s.with(|e, _| e.workspace_scroll.offset().y);
    assert!(after < before, "the sheet scrolled up: {before:?} -> {after:?}");
    s.screenshot("issue105-near-bottom");
    // It grows more: still above the bottom.
    paste(&mut s, &lines_of(10));
    let (rect, ws, _, _) = geometry(&mut s);
    within_view("near bottom, more", rect, ws);
}

/// Frozen rows and columns, and split panes: the box of a cell in the scrolled pane stays under the frozen rows; the one in a
/// frozen row grows down over the pane below.
fn panes() {
    for (menu, at) in [("xlsx.freeze", "C3"), ("xlsx.freeze", "A3"), ("xlsx.freeze", "C1"), ("xlsx.split", "C3")] {
        let mut s = workbook();
        s.click_cell(at);
        s.menu(menu);
        let tag = format!("{menu}-{at}");
        // A cell of the scrolled pane.
        s.click_cell("E10");
        s.key("f2");
        paste(&mut s, &lines_of(80));
        let (rect, ws, _, _) = geometry(&mut s);
        within_view(&format!("{tag} scrolled pane"), rect, ws);
        let split = s.with(|e, _| e.workspace_content().and_then(|c| e.frozen_split(c))).unwrap();
        if let Some(y) = split.1 {
            assert!(f32::from(rect.top()) >= f32::from(ws.top()) + y - 2.0, "{tag}: not above the first rows: {rect:?}");
        }
        s.screenshot(&format!("issue105-panes-{}-scrolled", tag.replace('.', "-")));
        s.key("escape");
        // A cell in the frozen rows (when there are some).
        if split.1.is_some() {
            let middle = cell_on_screen(&mut s, (4, 0));
            s.double_click(middle);
            assert_eq!(s.with(|e, _| e.cell_edit().map(|c| (c.column, c.row))), Some((4, 0)), "{tag}: editing a cell of the frozen rows");
            paste(&mut s, &lines_of(80));
            let (rect, ws, _, _) = geometry(&mut s);
            within_view(&format!("{tag} frozen row"), rect, ws);
            s.screenshot(&format!("issue105-panes-{}-frozen-row", tag.replace('.', "-")));
            s.key("escape");
        }
    }
}

/// Zoomed in and out: the box is still in the view, with the room of its bar.
fn zoom() {
    for zoom in [0.5f32, 2.0, 4.0] {
        let mut s = workbook();
        s.update(|e, w, cx| e.set_zoom(Some(zoom), w, cx));
        s.click_cell("B3");
        s.key("f2");
        paste(&mut s, &lines_of(40));
        let (rect, ws, layout, _) = geometry(&mut s);
        within_view(&format!("zoom {zoom}"), rect, ws);
        assert!(f32::from(rect.size.width) >= layout.width + BAR - 2.0, "{zoom}: the room of the bar");
        s.screenshot(&format!("issue105-zoom-{}", (zoom * 100.0) as i32));
        s.key("escape");
    }
}

/// The wheel still scrolls the sheet while a box is open (the fit does not hold it), and the box stays in the view.
fn wheel_keeps_working() {
    let mut s = workbook();
    s.click_cell("B12");
    s.key("f2");
    paste(&mut s, &lines_of(14));
    let (rect, ws, _, _) = geometry(&mut s);
    within_view("before wheel", rect, ws);
    let before = s.with(|e, _| e.workspace_scroll.offset().y);
    // Over the sheet, away from the box.
    s.wheel((700.0, 300.0), 60.0);
    let after = s.with(|e, _| e.workspace_scroll.offset().y);
    assert!(after > before, "the wheel scrolled the sheet back down: {before:?} -> {after:?}");
    s.screenshot("issue105-after-wheel");
}

/// The window position of the middle of what shows of a cell (column, row), found as a press finds it (`cell_under`).
fn cell_on_screen(s: &mut Scenario, cell: (u32, u32)) -> (f32, f32) {
    s.with(|e, _| {
        let ws = e.workspace_bounds.unwrap();
        let (mut sum, mut n) = ((0.0f32, 0.0f32), 0.0f32);
        let mut y = f32::from(ws.origin.y) + 2.0;
        while y < f32::from(ws.bottom()) - 14.0 {
            let mut x = f32::from(ws.origin.x) + 2.0;
            while x < f32::from(ws.right()) - 14.0 {
                if e.cell_under(point(px(x), px(y))) == Some(cell) {
                    sum = (sum.0 + x, sum.1 + y);
                    n += 1.0;
                }
                x += 2.0;
            }
            y += 2.0;
        }
        assert!(n > 0.0, "the cell {cell:?} is on screen");
        (sum.0 / n, sum.1 / n)
    })
}
