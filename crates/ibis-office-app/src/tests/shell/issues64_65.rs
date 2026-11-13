// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Real formula-bar navigation, wrapping and resizing regressions.
use super::Scenario;
use crate::xlsx::scenarios::SheetSteps;
use gpui_kit::*;
use crate::xlsx::ext::*;
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::issue65_formula_editor", editor),
        ("xlsx::issue64_general_numbers", numbers),
        ("xlsx::issue105_reedit_long_formula", reedit_long_formula),
        ("xlsx::cell_editor_frozen_panes", frozen_panes_editor),
    ]
}
fn workbook() -> Scenario {
    let mut s = Scenario::start_view();
    s.size(1000., 700.);
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    s
}
fn editor() {
    let mut s = workbook();
    s.click_cell("A1");
    s.key("f2");
    let text = format!("={}", vec!["100000000"; 55].join("+"));
    s.update(|_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone())));
    s.key("cmd-v");
    let (width, lines, right) = s.with(|e, _| {
        let edit = e.cell_edit().unwrap();
        let layout = edit.layout.borrow();
        let l = layout.as_ref().unwrap();
        (
            l.width,
            l.lines.len(),
            f32::from(e.workspace_bounds.unwrap().size.width),
        )
    });
    assert!(lines > 1);
    assert!(width <= right - 10.);
    s.screenshot("cell-wrapped-long-formula");
    let bar = s.with(|e, _| e.wb().bar_bounds.get().unwrap());
    s.click((f32::from(bar.left()) + 12., f32::from(bar.top()) + 8.));
    assert!(
        s.with(|e, _| e.cell_edit().is_some_and(|c| c.dragging_in_bar)),
        "bar click enters edit"
    );
    s.key("ctrl-home");
    let before = s.with(|e, _| e.current_cell());
    s.key("down");
    assert!(s.with(|e, _| e.cell_edit().unwrap().caret) > 0);
    assert_eq!(s.with(|e, _| e.current_cell()), before);
    s.key("home");
    let first = s.with(|e, _| e.cell_edit().unwrap().caret);
    s.key("end");
    assert!(s.with(|e, _| e.cell_edit().unwrap().caret) > first);
    s.key("ctrl-end");
    assert_eq!(s.with(|e, _| e.cell_edit().unwrap().caret), text.len());
    s.screenshot("bar-end-caret");
    s.key("ctrl-home");
    s.key("ctrl-shift-right");
    assert!(s.with(|e, _| e.cell_edit().unwrap().selection().is_some()));
    s.update(|e, _, cx| e.toggle_formula_bar_expanded(cx));
    let count = s.with(|e, _| e.wb().bar_visual_lines.len());
    assert!(count > 2);
    s.screenshot("expanded-bar-wrapped");
    let b = s.with(|e, _| e.wb().bar_bounds.get().unwrap());
    let at = (500., f32::from(b.bottom()) + 7.);
    s.drag(at, (at.0, at.1 + 50.));
    let after = s.with(|e, _| f32::from(e.wb().bar_bounds.get().unwrap().size.height));
    assert!(
        after > f32::from(b.size.height) + 30.,
        "splitter grows {after} vs {:?}",
        b.size.height
    );
    s.update(|e, _, cx| {
        e.toggle_formula_bar_expanded(cx);
        e.toggle_formula_bar_expanded(cx);
    });
    assert!(
        (s.with(|e, _| f32::from(e.wb().bar_bounds.get().unwrap().size.height)) - after).abs() < 1.
    );
    s.screenshot("resized-bar");
    let b = s.with(|e, _| e.wb().bar_bounds.get().unwrap());
    s.drag((500., f32::from(b.bottom()) + 7.), (500., 1500.));
    assert!(
        s.with(|e, _| f32::from(e.workspace_bounds.unwrap().size.height)) >= 99.,
        "table remains at least 100px"
    );
    s.screenshot("splitter-table-minimum");
    s.key("ctrl-end");
    s.key("ctrl-shift-left");
    let caret = s.with(|e, _| e.cell_edit().unwrap().caret);
    assert!(caret < text.len());
    s.key("cmd-shift-left");
    assert!(s.with(|e, _| e.cell_edit().unwrap().caret) < caret);
    s.key("enter");
    assert!(s.with(|e, _| e.cell_edit().is_none()));
    s.screenshot("expanded-readonly-wrapped");
    s.update(|e, w, cx| {
        e.new_file(ibis_pptx::Kind::Workbook, w, cx);
        e.toggle_formula_bar_expanded(cx);
    });
    assert!(
        s.with(|e, _| f32::from(e.wb().bar_bounds.get().unwrap().size.height)) < 90.,
        "new document starts with default bar height"
    );
}
fn numbers() {
    let mut s = workbook();
    for (cell, value) in [
        ("A1", "1234567890"),
        ("A2", "12345678901"),
        ("A3", "123456789012"),
        ("A4", "-12345678901"),
    ] {
        s.click_cell(cell);
        s.enter(value, "enter");
    }
    let texts = s.update(|e, w, _| {
        crate::xlsx::sheet_view::shown_texts(e.current_sheet().unwrap().1, &e.resources, w)
    });
    eprintln!("general shown={texts:?}");
    // The default-width column widens on entry (Excel: 10 digits -> 11.0,
    // 11 digits -> 12.0); 12 digits stop at 12.29 and fall back to E notation.
    assert!(
        texts.iter().any(|(r, _, t)| *r == 2 && t.contains('E')),
        "narrow-column fallback"
    );
    assert!(
        texts.iter().all(|(r, _, t)| *r >= 2 || !t.contains('E')),
        "10 and 11 digits show in full after the column widens"
    );
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        e.run_sheet(
            vec![ibis_doc_model::Command::Sheet(
                ibis_doc_model::SheetCommand::SetSize {
                    slide,
                    axis: ibis_doc_model::sheet_command::Axis::Columns,
                    first: 0,
                    last: 0,
                    size: 160 * 12700,
                },
            )],
            cx,
        );
    });
    let texts = s.update(|e, w, _| {
        crate::xlsx::sheet_view::shown_texts(e.current_sheet().unwrap().1, &e.resources, w)
    });
    assert_eq!(
        texts.iter().find(|(r, _, _)| *r == 0).unwrap().2,
        "1234567890"
    );
    assert_eq!(
        texts.iter().find(|(r, _, _)| *r == 1).unwrap().2,
        "12345678901"
    );
    assert_eq!(
        texts.iter().find(|(r, _, _)| *r == 2).unwrap().2,
        "1.23457E+11"
    );
    assert_eq!(
        s.value("A2"),
        ibis_doc_model::CellValue::Number(12345678901.)
    );
    assert_eq!(
        texts.iter().find(|(r, _, _)| *r == 3).unwrap().2,
        "-12345678901"
    );
    s.screenshot("general-number-digits");
}

/// Issue #105: a long formula committed and edited again shows its text (wrapped from the cell), not `#` filled
/// across the cell as a number that does not fit.
fn reedit_long_formula() {
    let mut s = workbook();
    s.click_cell("A1");
    let text = format!("={}", vec!["100000000"; 55].join("+"));
    s.enter(&text, "enter");
    s.click_cell("A1");
    s.key("f2");
    let shown: Vec<String> = s.with(|e, _| {
        let layout = e.cell_edit().unwrap().layout.borrow().clone().unwrap();
        layout.lines.iter().map(|l| l.pieces.iter().map(|p| {
            p.shaped.as_ref().map_or(String::new(), |sh| sh.text.to_string())
        }).collect::<String>()).collect()
    });
    assert!(shown.len() > 1, "wrapped: {shown:?}");
    assert_eq!(shown.concat(), text, "the whole formula shows, no #");
    s.screenshot("reedit-long-formula");
}

/// The window position (the middle of what shows of it) and top-left of a cell, found as a press finds it (`cell_under`).
fn cell_on_screen(s: &mut Scenario, cell: (u32, u32)) -> ((f32, f32), (f32, f32)) {
    s.with(|e, _| {
        let ws = e.workspace_bounds.unwrap();
        let (mut sum, mut n, mut min) = ((0.0f32, 0.0f32), 0.0f32, (f32::MAX, f32::MAX));
        let mut y = f32::from(ws.origin.y) + 2.0;
        while y < f32::from(ws.bottom()) - 14.0 {
            let mut x = f32::from(ws.origin.x) + 2.0;
            while x < f32::from(ws.right()) - 14.0 {
                if e.cell_under(point(px(x), px(y))) == Some(cell) {
                    sum = (sum.0 + x, sum.1 + y);
                    n += 1.0;
                    min = (min.0.min(x), min.1.min(y));
                }
                x += 2.0;
            }
            y += 2.0;
        }
        assert!(n > 0.0, "the cell {cell:?} is on screen");
        ((sum.0 / n, sum.1 / n), min)
    })
}

/// The box of the cell being typed into (one rect, painted once), with panes frozen at C3, columns only, rows only, and split.
fn frozen_panes_editor() {
    let both = [("corner", (1u32, 1u32)), ("frozen-column", (0, 7)), ("frozen-row", (4, 1)), ("scrolled", (4, 7))];
    pane_editor("C3", "xlsx.freeze", &both, true);
    pane_editor("C1", "xlsx.freeze", &[("frozen-column", (0, 7)), ("scrolled", (4, 7))], true);
    pane_editor("A3", "xlsx.freeze", &[("frozen-row", (4, 1)), ("scrolled", (4, 7))], true);
    pane_editor("C3", "xlsx.split", &both, false);
}

fn pane_editor(at: &str, menu: &str, cells: &[(&str, (u32, u32))], scroll_both: bool) {
    use crate::xlsx::scenarios::SheetSteps as _;
    let mut s = workbook();
    for (cell, text) in [("A8", "alpha beta gamma"), ("B2", "bravo charlie delta"), ("E2", "echo foxtrot golf"), ("E8", "hotel india juliet"), ("A1", "x"), ("A2", "kilo lima mike"), ("B1", "november oscar papa"), ("E1", "quebec romeo sierra")] {
        s.click_cell(cell);
        s.enter(text, "enter");
    }
    s.click_cell(at);
    s.menu(menu);
    let tag = format!("{menu}-{at}").replace('.', "-");
    let centre = s.with(|e, _| {
        let ws = e.workspace_bounds.unwrap();
        (f32::from(ws.origin.x + ws.size.width / 2.0), f32::from(ws.origin.y + ws.size.height / 2.0))
    });
    // Scrolled down and to the right: the cells in the scrolled panes are now other ones.
    s.wheel(centre, -45.0);
    s.wheel_xy(centre, -200.0, 0.0);
    let offset = s.with(|e, _| e.workspace_scroll.offset());
    if scroll_both {
        assert!(offset.x < px(-100.0) && offset.y < px(-30.0), "{tag}: the sheet scrolls both ways: {offset:?}");
    }
    let split = s.with(|e, _| e.workspace_content().and_then(|c| e.frozen_split(c))).unwrap();
    let (frozen_columns, frozen_rows) = s.with(|e, _| {
        let (columns, rows) = e.current_sheet().unwrap().1.frozen.or_else(|| e.split_panes().map(|p| p.at)).unwrap();
        (columns, rows)
    });
    for &(name, cell) in cells {
        let (middle, corner) = cell_on_screen(&mut s, cell);
        s.double_click(middle);
        let (row, column) = s.with(|e, _| { let c = e.cell_edit().expect("editing"); (c.row, c.column) });
        assert_eq!((column, row), cell, "{tag} {name}: the press begins the edit of the cell under it");
        let (rect, ws, lines, text) = s.with(|e, _| {
            let c = e.cell_edit().unwrap();
            (c.rect.get().expect("the box is placed"), e.workspace_bounds.unwrap(), c.layout.borrow().clone().unwrap(), c.text.clone())
        });
        s.screenshot(&format!("pane-edit-{tag}-{name}"));
        // One box, at the cell's own corner: where the press finds that cell.
        assert!((f32::from(rect.origin.x) - corner.0).abs() <= 3.0 && (f32::from(rect.origin.y) - corner.1).abs() <= 3.0, "{tag} {name}: box at the cell {rect:?} vs {corner:?}");
        // Not under the panes its cell is not in.
        if let (Some(split_x), true) = (split.0, cell.0 >= frozen_columns) {
            assert!(f32::from(rect.origin.x) >= f32::from(ws.origin.x) + split_x - 3.0, "{tag} {name}: not left of the first columns");
        }
        if let (Some(split_y), true) = (split.1, cell.1 >= frozen_rows) {
            assert!(f32::from(rect.origin.y) >= f32::from(ws.origin.y) + split_y - 3.0, "{tag} {name}: not above the first rows");
        }
        // The press at a character puts the caret before it.
        let line = &lines.lines[0];
        for byte in [5usize, 11] {
            let (x, y) = (f32::from(rect.origin.x) + line.x_at(byte) + 0.5, f32::from(rect.origin.y) + line.baseline - line.ascent / 2.0);
            s.click((x, y));
            let caret = s.with(|e, _| e.cell_edit().expect("still editing").caret);
            assert_eq!(caret, byte, "{tag} {name}: caret at the pressed character of {text:?}");
        }
        s.key("escape");
        assert!(s.with(|e, _| e.cell_edit().is_none()));
    }
}
