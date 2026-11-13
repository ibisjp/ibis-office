// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue #100, what the first round left: cells cut in one window and pasted in another window (the cells go from the
//! window they were cut in, the names come as they come with a copy), and 移動またはコピー of a sheet to another
//! workbook (the names the sheet uses or owns come, a name the destination has is asked about, a move takes the sheet out
//! once the destination has it). Two real windows, two workbooks.

use super::SheetSteps;
use crate::shell::editor::SlideEditor;
use crate::shell::scenario::{Scenario, repo_root};
use gpui_kit::{AnyWindowHandle, Entity};
use crate::ext::*;

const SRC: &str = "test/editing/issue100-names-src.xlsx";
const DST: &str = "test/editing/issue100-names-dst.xlsx";
const CONFLICT: &str = "test/editing/issue100-names-conflict.xlsx";

/// Open `path` in a second window.
fn open_second(s: &mut Scenario, path: &str) -> (AnyWindowHandle, Entity<SlideEditor>) {
    let full = repo_root().join(path);
    let loaded = crate::read_file(&full).unwrap_or_else(|e| panic!("{path}: {e}"));
    let name: gpui_kit::SharedString = full.file_name().unwrap().to_string_lossy().into_owned().into();
    s.with_app(|cx| {
        crate::open_window_with(cx, move |e, window, cx| {
            e.load(loaded.presentation, name, window, cx);
            e.source = Some(loaded.source);
            e.set_file_path(Some(full));
        })
    });
    s.settle();
    let handle = s.with_app(|cx| *cx.windows().last().expect("the second window"));
    let editor = s.with_app(|cx| {
        let root = handle.downcast::<gpui_kit::component::Root>().expect("a root");
        root.update(cx, |root, _, _| root.view().clone().downcast::<SlideEditor>().ok()).ok().flatten().expect("an editor")
    });
    s.update_window(handle, |w, _| w.resize(gpui_kit::size(gpui_kit::px(1000.), gpui_kit::px(700.))));
    s.settle();
    (handle, editor)
}

fn names_in(s: &mut Scenario, ed: &Entity<SlideEditor>) -> Vec<(String, Option<usize>, String)> {
    let mut out = s.with(|_, app| ed.read(app).presentation().workbook.as_ref().unwrap().names.iter().filter(|n| !n.name.starts_with("_xl")).map(|n| (n.name.clone(), n.sheet, n.formula.clone())).collect::<Vec<_>>());
    out.sort();
    out
}

fn name_in(s: &mut Scenario, ed: &Entity<SlideEditor>, name: &str) -> Option<(Option<usize>, String)> {
    names_in(s, ed).into_iter().find(|n| n.0 == name).map(|n| (n.1, n.2))
}

fn sheets_in(s: &mut Scenario, ed: &Entity<SlideEditor>) -> Vec<String> {
    s.with(|_, app| ed.read(app).presentation().workbook.as_ref().unwrap().sheets.iter().map(|x| x.name.clone()).collect())
}

fn formula_in(s: &mut Scenario, ed: &Entity<SlideEditor>, sheet: usize, row: u32, column: u32) -> Option<String> {
    s.with(|_, app| {
        let p = ed.read(app).presentation();
        let slide = p.workbook.as_ref().unwrap().sheets[sheet].slide?;
        ibis_doc_model::sheet_calc::sheet_of_slide(p.slide(slide)?)?.cell(row, column)?.formula.clone()
    })
}

fn value_in(s: &mut Scenario, ed: &Entity<SlideEditor>, sheet: usize, row: u32, column: u32) -> ibis_doc_model::CellValue {
    s.with(|_, app| {
        let p = ed.read(app).presentation();
        let slide = p.workbook.as_ref().unwrap().sheets[sheet].slide.unwrap();
        ibis_doc_model::sheet_calc::sheet_of_slide(p.slide(slide).unwrap()).unwrap().cell(row, column).map(|c| c.value.clone()).unwrap_or(ibis_doc_model::CellValue::Empty)
    })
}

fn question_in(s: &mut Scenario, ed: &Entity<SlideEditor>) -> Option<String> {
    s.with(|_, app| ed.read(app).sheet_state().and_then(|st| st.panels.paste_names.as_ref()).map(|p| p.asking().to_string()))
}

/// Move or copy the current sheet of the main window to the other workbook (the dialog's choices set the way a person sets them).
fn transfer(s: &mut Scenario, copy: bool, before: Option<usize>) {
    s.update(|e, _, cx| {
        e.open_sheet_transfer(cx);
        e.set_sheet_transfer_book(1, cx);
        e.set_sheet_transfer_copy(copy, cx);
        if let Some(b) = before {
            e.set_sheet_transfer_before(b, cx);
        }
        e.confirm_sheet_transfer(cx);
    });
    s.settle();
}

/// 切り取り in one window and 貼り付け in another: the cell arrives with its names, and is gone from the first window
/// (one undo of that window brings it back). Cancelling the question about a name leaves everything where it was.
pub(super) fn cut_and_paste_to_another_window() {
    let mut s = Scenario::open(SRC);
    let (w, ed2) = open_second(&mut s, DST);
    s.click_cell("C3");
    s.update(|e, _, cx| e.copy_cells(true, cx));
    s.update_in(w, &ed2, |e, _, cx| {
        e.set_cell(3, 0);
        e.paste_cells(false, cx);
    });
    assert_eq!(formula_in(&mut s, &ed2, 0, 0, 3).as_deref(), Some("DblPlus(1)"));
    let names: Vec<String> = names_in(&mut s, &ed2).into_iter().map(|n| n.0).collect();
    assert_eq!(names, vec!["Dbl", "DblPlus", "MyRng", "Zed"], "the names the cell uses come");
    assert_eq!(s.formula("C3"), None, "the cut cell is gone from the window it was cut in");
    s.screenshot("cut-source-after");
    s.key("cmd-z");
    assert_eq!(s.formula("C3").as_deref(), Some("DblPlus(1)"), "undo in that window brings it back");
    // The clipboard is spent: pasting again brings nothing.
    s.update_in(w, &ed2, |e, _, cx| {
        e.set_cell(3, 5);
        e.paste_cells(false, cx);
    });
    assert_eq!(formula_in(&mut s, &ed2, 0, 5, 3), None, "a cut is pasted once");
    assert_eq!(s.formula("C3").as_deref(), Some("DblPlus(1)"));

    // A copy (not a cut) leaves the cell, and can be pasted again.
    s.click_cell("C3");
    s.update(|e, _, cx| e.copy_cells(false, cx));
    s.update_in(w, &ed2, |e, _, cx| {
        e.set_cell(3, 6);
        e.paste_cells(false, cx);
    });
    // The names came with the cut: they are there now, so the copy asks about them as a paste does.
    assert!(question_in(&mut s, &ed2).is_some());
    s.update_in(w, &ed2, |e, _, cx| e.paste_names_yes_to_all(cx));
    assert_eq!(s.formula("C3").as_deref(), Some("DblPlus(1)"));
    assert_eq!(formula_in(&mut s, &ed2, 0, 6, 3).as_deref(), Some("DblPlus(1)"));
}

/// A cut that meets a name the destination has: nothing moves until the question is answered; closing the question
/// moves nothing (the cell stays where it was).
pub(super) fn cut_with_a_conflicting_name() {
    let mut s = Scenario::open(SRC);
    let (w, ed2) = open_second(&mut s, CONFLICT);
    s.click_cell("C1");
    s.update(|e, _, cx| e.copy_cells(true, cx));
    s.update_in(w, &ed2, |e, _, cx| {
        e.set_cell(3, 0);
        e.paste_cells(false, cx);
    });
    assert_eq!(question_in(&mut s, &ed2).as_deref(), Some("MyRng"));
    s.screenshot_of(w, "name-conflict-question");
    assert_eq!(s.formula("C1").as_deref(), Some("MyRng*2"), "still there while the question is open");
    s.update_in(w, &ed2, |e, _, cx| e.paste_names_yes(cx));
    assert_eq!(formula_in(&mut s, &ed2, 0, 0, 3).as_deref(), Some("MyRng*2"));
    assert_eq!(s.formula("C1"), None, "gone once pasted");

    // Closing the question: nothing pasted, nothing cut.
    drop(ed2);
    drop(s);
    let mut s = Scenario::open(SRC);
    let (w, ed2) = open_second(&mut s, CONFLICT);
    s.click_cell("C1");
    s.update(|e, _, cx| e.copy_cells(true, cx));
    s.update_in(w, &ed2, |e, _, cx| {
        e.set_cell(3, 0);
        e.paste_cells(false, cx);
        e.sheet_state_mut().unwrap().panels.paste_names = None;
    });
    assert_eq!(formula_in(&mut s, &ed2, 0, 0, 3), None);
    assert_eq!(s.formula("C1").as_deref(), Some("MyRng*2"));
}

/// 移動またはコピー: a copy to another workbook brings the sheet (cells, its own names, the workbook's names that refer to
/// no other sheet, and those its formulas use, as Excel does), keeps the source as it is, and is one undo of the destination.
pub(super) fn copy_sheet_to_another_workbook() {
    let mut s = Scenario::open(SRC);
    let (w, ed2) = open_second(&mut s, DST);
    // The dialog lists this workbook first, then the other.
    s.update(|e, _, cx| e.open_sheet_transfer(cx));
    let books: Vec<String> = s.with(|e, _| e.sheet_state().unwrap().panels.sheet_transfer.as_ref().unwrap().books.iter().map(|b| b.name.clone()).collect());
    assert_eq!(books, vec!["issue100-names-src.xlsx", "issue100-names-dst.xlsx"]);
    s.screenshot("transfer-dialog");
    s.update(|e, _, cx| e.set_sheet_transfer_book(1, cx));
    s.screenshot("transfer-dialog-other-book");
    s.update(|e, _, cx| e.set_sheet_transfer_copy(true, cx));
    s.update(|e, _, cx| e.confirm_sheet_transfer(cx));
    s.settle();
    assert_eq!(sheets_in(&mut s, &ed2), vec!["Data", "Sheet2", "Sheet1"]);
    let got = names_in(&mut s, &ed2);
    let list: Vec<(&str, Option<usize>)> = got.iter().map(|n| (n.0.as_str(), n.1)).collect();
    assert_eq!(list, vec![("Cst", None), ("Dbl", None), ("DblPlus", None), ("Loc", Some(2)), ("MyRng", None), ("OtherSh", None), ("Unused", None), ("Zed", None)], "{got:?}");
    assert_eq!(name_in(&mut s, &ed2, "MyRng"), Some((None, "Sheet1!$A$1".to_string())));
    assert_eq!(name_in(&mut s, &ed2, "Loc"), Some((Some(2), "Sheet1!$A$2".to_string())));
    assert_eq!(name_in(&mut s, &ed2, "Cst"), Some((None, "42".to_string())));
    let other = name_in(&mut s, &ed2, "OtherSh").unwrap().1;
    assert!(other.contains("[issue100-names-src.xlsx]Sheet2") && other.ends_with("!$A$1"), "{other}");
    // The sheet's cells: names kept, the other sheet of the source named as the source's.
    assert_eq!(formula_in(&mut s, &ed2, 2, 0, 2).as_deref(), Some("MyRng*2"));
    assert_eq!(value_in(&mut s, &ed2, 2, 0, 2), ibis_doc_model::CellValue::Number(10.0));
    assert_eq!(value_in(&mut s, &ed2, 2, 3, 2), ibis_doc_model::CellValue::Number(43.0));
    // The source is as it was.
    assert_eq!(s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.len()), 2);
    assert_eq!(s.formula("C1").as_deref(), Some("MyRng*2"));
    s.screenshot_of(w, "transfer-destination");
    // One undo of the destination takes the sheet and its names.
    s.update_in(w, &ed2, |e, _, cx| {
        e.document.undo();
        cx.notify();
    });
    assert_eq!(sheets_in(&mut s, &ed2), vec!["Data", "Sheet2"]);
    assert_eq!(names_in(&mut s, &ed2).len(), 1, "{:?}", names_in(&mut s, &ed2));
}

/// A move to another workbook leaves the source without the sheet (its names go with it), and refuses to take the last
/// sheet out of a workbook. A sheet with the name of one the destination has gets the name of a copy.
pub(super) fn move_sheet_to_another_workbook() {
    let mut s = Scenario::open(SRC);
    let (_, ed2) = open_second(&mut s, DST);
    transfer(&mut s, false, Some(0));
    // Before the destination's first sheet.
    assert_eq!(sheets_in(&mut s, &ed2), vec!["Sheet1", "Data", "Sheet2"]);
    let source_sheets: Vec<String> = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.iter().map(|x| x.name.clone()).collect());
    assert_eq!(source_sheets, vec!["Sheet2"], "the sheet is gone from the workbook it left");
    assert!(name_in(&mut s, &ed2, "Loc").is_some_and(|l| l.0 == Some(0)), "its own name goes with it, to the sheet's new place");

    // The last sheet cannot leave.
    s.update(|e, _, cx| {
        e.open_sheet_transfer(cx);
        e.set_sheet_transfer_book(1, cx);
        e.confirm_sheet_transfer(cx);
    });
    s.settle();
    assert!(s.with(|e, _| e.sheet_state().unwrap().panels.sheet_transfer.as_ref().is_some_and(|p| !p.message.is_empty())), "a message says why");
    assert_eq!(s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.len()), 1);
    s.screenshot("transfer-last-sheet");

    // The destination has a sheet of that name: the copy's name.
    drop(ed2);
    drop(s);
    let mut s = Scenario::open(DST);
    let (_, ed2) = open_second(&mut s, SRC);
    // Sheet2 of the destination is named like the source's Sheet2.
    s.update(|e, _, cx| {
        let slide = e.presentation().workbook.as_ref().unwrap().sheets[1].slide.unwrap();
        e.current_slide = slide;
        cx.notify();
    });
    transfer(&mut s, true, None);
    assert_eq!(sheets_in(&mut s, &ed2), vec!["Sheet1", "Sheet2", "Sheet2 (2)"]);
}

/// A name the destination has is asked about, in the destination's window, before the sheet goes: はい keeps the
/// destination's name; the move happens once the questions are answered; closing the question moves nothing.
pub(super) fn sheet_transfer_asks_about_names() {
    let mut s = Scenario::open(SRC);
    let (w, ed2) = open_second(&mut s, CONFLICT);
    transfer(&mut s, false, None);
    let first = question_in(&mut s, &ed2).expect("a name conflicts");
    assert!(first == "Dbl" || first == "MyRng", "{first}");
    s.screenshot_of(w, "sheet-name-conflict");
    assert_eq!(sheets_in(&mut s, &ed2), vec!["Sheet1", "Sheet2"], "nothing before the answers");
    assert_eq!(s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.len()), 2, "the source keeps its sheet meanwhile");
    s.update_in(w, &ed2, |e, _, cx| e.paste_names_yes(cx));
    assert!(question_in(&mut s, &ed2).is_some(), "the other name asks too");
    s.update_in(w, &ed2, |e, window, cx| e.paste_names_no(window, cx));
    s.screenshot_of(w, "sheet-name-conflict-rename");
    s.update_in(w, &ed2, |e, window, cx| {
        e.paste_names_set_new_name("Rate", window, cx);
        e.paste_names_renamed(cx);
    });
    s.settle();
    assert_eq!(question_in(&mut s, &ed2), None);
    assert_eq!(sheets_in(&mut s, &ed2), vec!["Sheet1", "Sheet2", "Sheet1 (2)"]);
    // The destination's names stay; the renamed one is the sheet's.
    assert!(name_in(&mut s, &ed2, "Dbl").unwrap().1.contains("x*3") || name_in(&mut s, &ed2, "MyRng").unwrap().1 == "Sheet1!$B$2");
    assert!(name_in(&mut s, &ed2, "Rate").is_some(), "{:?}", names_in(&mut s, &ed2));
    let source_sheets = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.len());
    assert_eq!(source_sheets, 1, "moved once answered");

    // Closing the question: nothing moves.
    drop(ed2);
    drop(s);
    let mut s = Scenario::open(SRC);
    let (w, ed2) = open_second(&mut s, CONFLICT);
    transfer(&mut s, false, None);
    assert!(question_in(&mut s, &ed2).is_some());
    s.update_in(w, &ed2, |e, _, _| e.sheet_state_mut().unwrap().panels.paste_names = None);
    assert_eq!(sheets_in(&mut s, &ed2), vec!["Sheet1", "Sheet2"]);
    assert_eq!(s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.len()), 2);
}

/// In one workbook the dialog places the sheet: move before another sheet, or copy before it.
pub(super) fn sheet_transfer_in_one_workbook() {
    let mut s = Scenario::open(SRC);
    let sheets = |s: &mut Scenario| -> Vec<String> { s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets.iter().map(|x| x.name.clone()).collect()) };
    // Sheet1 to the end.
    s.update(|e, _, cx| {
        e.open_sheet_transfer(cx);
        e.confirm_sheet_transfer(cx);
    });
    assert_eq!(sheets(&mut s), vec!["Sheet2", "Sheet1"]);
    // A copy of Sheet1 (now last), before Sheet2.
    s.update(|e, _, cx| {
        e.open_sheet_transfer(cx);
        e.set_sheet_transfer_copy(true, cx);
        e.set_sheet_transfer_before(0, cx);
        e.confirm_sheet_transfer(cx);
    });
    assert_eq!(sheets(&mut s), vec!["Sheet1 (2)", "Sheet2", "Sheet1"]);
    // The copy is the current sheet now. Moved before the next sheet changes nothing; before the last, it goes there.
    s.update(|e, _, cx| {
        e.open_sheet_transfer(cx);
        e.set_sheet_transfer_before(1, cx);
        e.confirm_sheet_transfer(cx);
    });
    assert_eq!(sheets(&mut s), vec!["Sheet1 (2)", "Sheet2", "Sheet1"]);
    s.update(|e, _, cx| {
        e.open_sheet_transfer(cx);
        e.set_sheet_transfer_before(2, cx);
        e.confirm_sheet_transfer(cx);
    });
    assert_eq!(sheets(&mut s), vec!["Sheet2", "Sheet1 (2)", "Sheet1"]);
    s.screenshot("one-workbook-after");
}
