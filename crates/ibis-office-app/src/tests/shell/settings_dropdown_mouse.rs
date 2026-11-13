// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A dropdown opened and closed inside the settings dialog, and then the dialog closed, leaves the worksheet's mouse working: a click
//! on a cell moves the selection (reported 2026-11-12: after the font list of スプレッドシート, the pointer did nothing).
use super::Scenario;
use crate::shell::scenario::button_probe as buttons;
use crate::xlsx::scenarios::SheetSteps;
use gpui_kit::*;
use crate::xlsx::ext::*;
use crate::shell::ink::is_ink_name;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::settings_dropdown_mouse_matrix", matrix),
        ("shell::settings_dropdown_mouse_no_dropdown", no_dropdown),
        ("shell::settings_closed_over_ink_panel_keeps_panel_visible", ink_panel_stays),
        ("shell::ink_closing_the_sidebar_ends_the_pen", ink_sidebar_toggle),
        ("shell::ink_erase_all_after_save_and_reopen", ink_erase_all_reopen),
        ("shell::ink_erase_all_unnamed_dots", ink_erase_all_unnamed),
    ]
}

#[derive(Clone, Copy, Debug)]
enum CloseList { OutsideClick, Escape, Choose, ClickRow(f32), OutsideWindow, HoverThenEscape, HoverThenOutside }
#[derive(Clone, Copy, Debug)]
enum CloseDialog { Button, Escape }

// A committed fixture: the triage workbook the test used before is edited every day (its saved view scrolls past C5).
const FILE: &str = "test/editing/xlsx-sales.xlsx";

fn open_settings(s: &mut Scenario) {
    s.update(|e, window, cx| e.open_settings(5, window, cx));
    s.settle();
}

fn close_dialog(s: &mut Scenario, how: CloseDialog) {
    match how {
        CloseDialog::Escape => s.key("escape"),
        CloseDialog::Button => {
            let b = buttons::bounds("settings-dialog-close").or_else(|| buttons::bounds("dialog-default-close")).unwrap_or_else(|| panic!("close button: {:?}", buttons::all()));
            s.click((f32::from(b.center().x), f32::from(b.center().y)));
        }
    }
    s.settle();
    assert!(!s.with(|e, _| e.settings.is_some()), "the dialog is closed ({how:?})");
}

/// After the dialog is closed: a click on C5 selects C5, and then one on B3 selects B3.
fn mouse_reaches_sheet(s: &mut Scenario) -> bool {
    s.click_cell("C5");
    s.settle();
    let first = s.selected() == "C5";
    s.click_cell("B3");
    s.settle();
    first && s.selected() == "B3"
}

fn run(list: Option<CloseList>, dialog: CloseDialog) -> bool {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open(FILE);
    s.size(1200., 800.);
    open_settings(&mut s);
    if let Some(list) = list {
        buttons::click(&mut s, "settings-formula-bar-font");
        s.settle();
        assert_eq!(s.with(|e, _| e.toolbar_search_open), Some("settings-formula-bar-font"), "the list is open");
        match list {
            CloseList::Escape => s.key("escape"),
            CloseList::Choose => {
                s.key("down");
                s.key("down");
                s.key("enter");
            }
            CloseList::ClickRow(dy) => {
                let b = buttons::bounds("settings-formula-bar-font").expect("the button");
                s.hover((f32::from(b.center().x), f32::from(b.bottom()) + dy));
                s.settle();
                s.click((f32::from(b.center().x), f32::from(b.bottom()) + dy));
            }
            CloseList::HoverThenEscape | CloseList::HoverThenOutside => {
                let b = buttons::bounds("settings-formula-bar-font").expect("the button");
                for dy in [60., 80., 100., 130.] {
                    s.hover((f32::from(b.center().x), f32::from(b.bottom()) + dy));
                    s.settle();
                    s.advance_ms(1500);
                    s.settle();
                }
                if matches!(list, CloseList::HoverThenEscape) { s.key("escape"); } else { s.click((5., 5.)); }
            }
            CloseList::OutsideWindow => s.click((5., 5.)),
            CloseList::OutsideClick => {
                // The dialog's title bar: inside the dialog, outside the list.
                let d = s.with(|e, _| e.dialog.bounds.expect("the dialog's place"));
                s.click((f32::from(d.center().x), f32::from(d.origin.y + px(20.))));
            }
        }
        s.settle();
        assert!(s.with(|e, _| e.toolbar_search_open.is_none()), "the list is closed ({list:?})");
        assert!(s.with(|e, _| e.settings.is_some()), "the dialog is still open after closing the list ({list:?})");
    }
    close_dialog(&mut s, dialog);
    mouse_reaches_sheet(&mut s)
}

/// Every way of closing the list and then the dialog; the worksheet takes the mouse in all of them.
fn matrix() {
    let mut failed = Vec::new();
    for list in [CloseList::OutsideClick, CloseList::OutsideWindow, CloseList::Escape, CloseList::Choose, CloseList::HoverThenEscape, CloseList::HoverThenOutside] {
        for dialog in [CloseDialog::Button, CloseDialog::Escape] {
            let ok = run(Some(list), dialog);
            eprintln!("settings_dropdown_mouse: list {list:?}, dialog {dialog:?}: {}", if ok { "the mouse works" } else { "the mouse is dead" });
            if !ok { failed.push((list, dialog)); }
        }
    }
    assert!(failed.is_empty(), "the mouse does not reach the worksheet after: {failed:?}");
}

/// The control: the dialog opened and closed with no list.
fn no_dropdown() {
    for dialog in [CloseDialog::Button, CloseDialog::Escape] {
        assert!(run(None, dialog), "the mouse works after the dialog alone ({dialog:?})");
    }
}

/// The Draw panel open (Bronze chosen), the settings dialog opened over it and closed: the panel is still on screen (the sidebar open), so the
/// pen mode the clicks go to is visible; and a click on the sheet then draws one stroke, not a dot nobody sees.
fn ink_panel_stays() {
    for dialog in [CloseDialog::Button, CloseDialog::Escape] {
        let _probe = buttons::Probe::new();
        let mut s = Scenario::open(FILE);
        s.size(1200., 800.);
        s.update(|e, window, cx| {
            e.open_ink_panel(window, cx);
            if let Some(p) = e.wb_mut().panels.ink.as_mut() { p.effect = Some(crate::shell::ink::InkEffect::Bronze); }
            cx.notify();
        });
        s.settle();
        open_settings(&mut s);
        close_dialog(&mut s, dialog);
        let (panel, sidebar) = s.with(|e, _| (e.wb().panels.ink.is_some(), e.sidebar_open));
        assert!(!panel || sidebar, "the Draw panel is still in the mode but not on screen ({dialog:?}): panel {panel}, sidebar {sidebar}");
    }
}

fn ink_count(s: &mut Scenario) -> usize {
    s.with(|e, _| e.slide().shapes.iter().filter(|sh| e.slide().shape_names.get(&sh.id).is_some_and(|n| is_ink_name(n))).count())
}

fn shape_count(s: &mut Scenario) -> usize { s.with(|e, _| e.slide().shapes.len()) }

/// The right pane hidden with the Draw panel open: the pen is over; a click on a cell selects it and draws nothing.
fn ink_sidebar_toggle() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    s.update(|e, window, cx| e.open_ink_panel(window, cx));
    s.settle();
    s.update(|e, window, cx| e.toggle_sidebar(window, cx));
    s.settle();
    let before = shape_count(&mut s);
    s.click_cell("C5");
    s.settle();
    assert_eq!(s.selected(), "C5", "a click selects the cell after the pane is hidden");
    assert_eq!(shape_count(&mut s), before, "no ink after the pane is hidden");
}

/// Ink drawn, the file saved and opened again: 「すべてのインクを消去」 removes it, before and after.
fn ink_erase_all_reopen() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    let base = shape_count(&mut s);
    s.update(|e, window, cx| {
        e.open_ink_panel(window, cx);
        if let Some(p) = e.wb_mut().panels.ink.as_mut() { p.effect = Some(crate::shell::ink::InkEffect::Bronze); }
        cx.notify();
    });
    s.settle();
    let at = |s: &mut Scenario, c: u32, r: u32| { let p = s.with(|e, _| e.cell_center(c, r)).expect("on screen"); (f32::from(p.x), f32::from(p.y)) };
    let (a, b, c) = (at(&mut s, 2, 4), at(&mut s, 4, 7), at(&mut s, 1, 2));
    s.drag(a, b);
    s.click(c);
    s.settle();
    assert_eq!(ink_count(&mut s), 2, "a stroke and a dot are drawn");
    let path = s.save_as("ink-reopen.xlsx");
    s.reload_saved();
    s.update(|e, window, cx| e.open_ink_panel(window, cx));
    s.settle();
    assert_eq!(shape_count(&mut s), base + 2, "the strokes are in the file ({})", path.display());
    assert_eq!(ink_count(&mut s), 2, "the strokes are still ink after opening the file again");
    buttons::click(&mut s, "ink-erase-all");
    s.settle();
    assert_eq!(shape_count(&mut s), base, "erase all removes the saved ink");
}

/// A workbook whose ink was saved with the names `Shape N` (20 one-point strokes, as this app once wrote them): 「すべてのインクを消去」 removes them.
fn ink_erase_all_unnamed() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/ink-dots-unnamed.xlsx");
    s.size(1200., 800.);
    let base = shape_count(&mut s);
    s.update(|e, window, cx| e.open_ink_panel(window, cx));
    s.settle();
    assert!(base >= 20, "the dots are shapes ({base})");
    buttons::click(&mut s, "ink-erase-all");
    s.settle();
    assert_eq!(shape_count(&mut s), base - 20, "the 20 dots are gone");
}
