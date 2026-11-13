// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue #100: pasting cells whose formulas use defined names (ranges, constants, LAMBDA functions) brings the
//! names along, as Excel does (measured in Excel for Mac: `target/issue100/`).
//!
//! The source workbook (`issue100-names-src.xlsx`) has, on Sheet1: A1=5, A2=7, C1 `=MyRng*2`, C2 `=Dbl(3)`,
//! C3 `=DblPlus(1)`, C4 `=Cst+1`, C5 1, C6 `=Loc+1`, E1 `=OtherSh+1`; names MyRng (Sheet1!$A$1), Cst (42),
//! Dbl (LAMBDA), DblPlus (LAMBDA using Dbl and MyRng), Unused, OtherSh (Sheet2!$A$1) and Loc (Sheet1 only).

use super::SheetSteps;
use crate::shell::scenario::{Scenario, repo_root};
use crate::ext::*;

const SRC: &str = "test/editing/issue100-names-src.xlsx";
const DST: &str = "test/editing/issue100-names-dst.xlsx";
const CONFLICT: &str = "test/editing/issue100-names-conflict.xlsx";

/// Open another workbook in the window, as if it were another window (the copied cells stay "from another window").
fn switch_to(s: &mut Scenario, path: &str) {
    crate::clipboard::copied_in_another_window();
    let full = repo_root().join(path);
    let loaded = crate::read_file(&full).unwrap_or_else(|e| panic!("{path}: {e}"));
    let name: gpui_kit::SharedString = full.file_name().unwrap().to_string_lossy().into_owned().into();
    s.update(|e, window, cx| {
        e.load(loaded.presentation, name, window, cx);
        e.source = Some(loaded.source);
        e.set_file_path(Some(full.clone()));
    });
    s.settle();
}

fn copy(s: &mut Scenario, from: &str) {
    s.click_cell(from);
    s.update(|e, _, cx| e.copy_cells(false, cx));
}

fn paste(s: &mut Scenario, at: &str) {
    s.click_cell(at);
    s.update(|e, _, cx| e.paste_cells(false, cx));
}

/// The names of the workbook: (name, sheet index, what it stands for), sorted.
fn names(s: &mut Scenario) -> Vec<(String, Option<usize>, String)> {
    let mut out = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().names.iter().filter(|n| !n.name.starts_with("_xl")).map(|n| (n.name.clone(), n.sheet, n.formula.clone())).collect::<Vec<_>>());
    out.sort();
    out
}

fn name_of(s: &mut Scenario, name: &str) -> Option<(Option<usize>, String)> {
    names(s).into_iter().find(|n| n.0 == name).map(|n| (n.1, n.2))
}

fn question(s: &mut Scenario) -> Option<String> {
    s.with(|e, _| e.sheet_state().and_then(|st| st.panels.paste_names.as_ref()).map(|p| p.asking().to_string()))
}

/// Pasting into a workbook that has none of the names defines the ones the formula uses, with the names they use.
pub(super) fn other_window() {
    let mut s = Scenario::open(SRC);
    copy(&mut s, "C3");
    switch_to(&mut s, DST);
    paste(&mut s, "D1");
    assert_eq!(s.formula("D1").as_deref(), Some("DblPlus(1)"));
    let got = names(&mut s);
    let wanted: Vec<&str> = got.iter().map(|n| n.0.as_str()).collect();
    // The names the cell uses, the names those use; not Cst, Unused, Loc, OtherSh; the destination's own stays.
    assert_eq!(wanted, vec!["Dbl", "DblPlus", "MyRng", "Zed"], "{got:?}");
    // MyRng stood for Sheet1!$A$1 of the sheet copied: it follows the sheet pasted into.
    assert_eq!(name_of(&mut s, "MyRng"), Some((None, "Data!$A$1".to_string())));
    assert_eq!(name_of(&mut s, "Zed"), Some((None, "Data!$C$3".to_string())));
    assert!(name_of(&mut s, "Dbl").unwrap().1.contains("x*2"));
    // One undo takes the cell and its names.
    s.key("cmd-z");
    assert_eq!(names(&mut s).len(), 1, "{:?}", names(&mut s));
    assert_eq!(s.formula("D1"), None);

    // A constant.
    drop(s);
    let mut s = Scenario::open(SRC);
    copy(&mut s, "C4");
    switch_to(&mut s, DST);
    paste(&mut s, "D1");
    assert_eq!(name_of(&mut s, "Cst"), Some((None, "42".to_string())));
    assert_eq!(s.value("D1"), ibis_doc_model::CellValue::Number(43.0));

    // A name of the sheet copied (sheet scope) is a name of the sheet pasted into.
    drop(s);
    let mut s = Scenario::open(SRC);
    copy(&mut s, "C6");
    switch_to(&mut s, DST);
    paste(&mut s, "D2");
    assert_eq!(name_of(&mut s, "Loc"), Some((Some(0), "Data!$A$2".to_string())));

    // A name of another sheet of the copied workbook refers to that workbook.
    drop(s);
    let mut s = Scenario::open(SRC);
    copy(&mut s, "E1");
    switch_to(&mut s, DST);
    paste(&mut s, "D3");
    let other = name_of(&mut s, "OtherSh").expect("OtherSh is defined").1;
    assert!(other.contains("[issue100-names-src.xlsx]Sheet2") && other.ends_with("!$A$1"), "{other}");
}

/// Pasting values brings no names, pasting formulas does; a cell that names nothing brings nothing.
pub(super) fn kinds_of_paste() {
    use crate::clipboard::PasteSpecial;
    let mut s = Scenario::open(SRC);
    copy(&mut s, "C1");
    switch_to(&mut s, DST);
    s.click_cell("D1");
    s.update(|e, _, cx| e.paste_special(PasteSpecial::Values, cx));
    assert_eq!(names(&mut s).len(), 1, "values bring no names: {:?}", names(&mut s));
    s.click_cell("D2");
    s.update(|e, _, cx| e.paste_special(PasteSpecial::Formats, cx));
    assert_eq!(names(&mut s).len(), 1, "formats bring no names");
    s.click_cell("D3");
    s.update(|e, _, cx| e.paste_special(PasteSpecial::Formulas, cx));
    assert!(name_of(&mut s, "MyRng").is_some(), "formulas bring their names");

    drop(s);
    let mut s = Scenario::open(SRC);
    copy(&mut s, "C5");
    switch_to(&mut s, DST);
    paste(&mut s, "D1");
    assert_eq!(names(&mut s).len(), 1, "a cell with no name brings none");
    drop(s);
    let mut s = Scenario::open(SRC);
    copy(&mut s, "A1");
    switch_to(&mut s, DST);
    paste(&mut s, "D1");
    assert_eq!(names(&mut s).len(), 1, "the cell a name stands for is not enough");
}

/// A name the destination has: Excel asks. はい keeps the destination's name, いいえ takes another name, すべて answers
/// はい for the rest, キャンセル of the new name's step returns to the question.
pub(super) fn conflict() {
    // はい for both: nothing is overwritten, the pasted cell uses the destination's names.
    let mut s = Scenario::open(SRC);
    copy(&mut s, "C3");
    switch_to(&mut s, CONFLICT);
    paste(&mut s, "D1");
    let first = question(&mut s).expect("a name conflicts");
    assert!(first == "Dbl" || first == "MyRng", "{first}");
    assert_eq!(s.formula("D1"), None, "nothing is pasted before the answer");
    s.update(|e, _, cx| e.paste_names_yes(cx));
    assert!(question(&mut s).is_some(), "the other name asks too");
    s.update(|e, _, cx| e.paste_names_yes(cx));
    assert_eq!(question(&mut s), None);
    assert_eq!(s.formula("D1").as_deref(), Some("DblPlus(1)"));
    assert_eq!(name_of(&mut s, "MyRng"), Some((None, "Sheet1!$B$2".to_string())));
    assert!(name_of(&mut s, "Dbl").unwrap().1.contains("x*3"));
    assert!(name_of(&mut s, "DblPlus").is_some());

    // すべて: one answer for both.
    drop(s);
    let mut s = Scenario::open(SRC);
    copy(&mut s, "C3");
    switch_to(&mut s, CONFLICT);
    paste(&mut s, "D1");
    s.update(|e, _, cx| e.paste_names_yes_to_all(cx));
    assert_eq!(question(&mut s), None);
    assert_eq!(s.formula("D1").as_deref(), Some("DblPlus(1)"));

    // いいえ: another name for MyRng; Dbl is kept. DblPlus (new) uses the new name, the destination's stay.
    drop(s);
    let mut s = Scenario::open(SRC);
    copy(&mut s, "C3");
    switch_to(&mut s, CONFLICT);
    paste(&mut s, "D1");
    while let Some(asking) = question(&mut s) {
        if asking == "MyRng" {
            s.update(|e, window, cx| e.paste_names_no(window, cx));
            // An unusable name is refused (the destination has it); the question stays.
            s.update(|e, window, cx| {
                e.paste_names_set_new_name("Dbl", window, cx);
                e.paste_names_renamed(cx);
            });
            assert_eq!(question(&mut s).as_deref(), Some("MyRng"));
            assert!(s.with(|e, _| e.sheet_state().unwrap().panels.paste_names.as_ref().unwrap().message().len() > 0));
            // キャンセル returns to the question, いいえ again, and a free name goes.
            s.update(|e, _, cx| e.paste_names_back(cx));
            s.update(|e, window, cx| e.paste_names_no(window, cx));
            s.update(|e, window, cx| {
                e.paste_names_set_new_name("Rate", window, cx);
                e.paste_names_renamed(cx);
            });
        } else {
            s.update(|e, _, cx| e.paste_names_yes(cx));
        }
    }
    assert_eq!(s.formula("D1").as_deref(), Some("DblPlus(1)"));
    assert_eq!(name_of(&mut s, "Rate"), Some((None, "Sheet1!$A$1".to_string())));
    assert_eq!(name_of(&mut s, "MyRng"), Some((None, "Sheet1!$B$2".to_string())));
    assert!(name_of(&mut s, "Dbl").unwrap().1.contains("x*3"));
    assert!(name_of(&mut s, "DblPlus").unwrap().1.contains("Rate"), "{:?}", name_of(&mut s, "DblPlus"));

    // The cell itself uses the new name when the cell names it.
    drop(s);
    let mut s = Scenario::open(SRC);
    copy(&mut s, "C1");
    switch_to(&mut s, CONFLICT);
    paste(&mut s, "D1");
    assert_eq!(question(&mut s).as_deref(), Some("MyRng"));
    s.update(|e, window, cx| e.paste_names_no(window, cx));
    s.update(|e, window, cx| {
        e.paste_names_set_new_name("MyRng2", window, cx);
        e.paste_names_renamed(cx);
    });
    assert_eq!(s.formula("D1").as_deref(), Some("MyRng2*2"));
    assert_eq!(name_of(&mut s, "MyRng2"), Some((None, "Sheet1!$A$1".to_string())));

    // キャンセル (closing) of the question pastes nothing.
    drop(s);
    let mut s = Scenario::open(SRC);
    copy(&mut s, "C1");
    switch_to(&mut s, CONFLICT);
    paste(&mut s, "D1");
    assert!(question(&mut s).is_some());
    s.update(|e, _, _| e.sheet_state_mut().unwrap().panels.paste_names = None);
    assert_eq!(s.formula("D1"), None);
}

/// In one workbook: the names of the sheet copied (sheet scope) are made for the other sheet, as Excel does;
/// pasting on the same sheet changes no names.
pub(super) fn same_workbook() {
    let mut s = Scenario::open(SRC);
    copy(&mut s, "C6");
    paste(&mut s, "D6");
    assert_eq!(names(&mut s).iter().filter(|n| n.0 == "Loc").count(), 1, "same sheet: no new names");
    let sheet2 = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets[1].slide.unwrap());
    s.update(|e, window, cx| e.select_slide(sheet2, window, cx));
    paste(&mut s, "A5");
    assert_eq!(name_of(&mut s, "Loc"), Some((Some(0), "Sheet1!$A$2".to_string())));
    let local: Vec<_> = names(&mut s).into_iter().filter(|n| n.0 == "Loc").collect();
    assert_eq!(local, vec![("Loc".to_string(), Some(0), "Sheet1!$A$2".to_string()), ("Loc".to_string(), Some(1), "Sheet2!$A$2".to_string())]);
    // A workbook name is the same name on any sheet: nothing is made.
    let count = names(&mut s).len();
    s.click_cell("C1");
    s.key("escape");
    let sheet1 = s.with(|e, _| e.presentation().workbook.as_ref().unwrap().sheets[0].slide.unwrap());
    s.update(|e, window, cx| e.select_slide(sheet1, window, cx));
    copy(&mut s, "C1");
    s.update(|e, window, cx| e.select_slide(sheet2, window, cx));
    paste(&mut s, "B5");
    assert_eq!(names(&mut s).len(), count);
}

/// シートを複製 (measured in Excel): the sheet's names are made again for the copy, and so are the workbook names that
/// refer to the sheet (directly, or through other names); the others (a constant, a LAMBDA that uses no such name) are not.
pub(super) fn copy_sheet() {
    let mut s = Scenario::open(SRC);
    s.update(|e, window, cx| e.copy_sheet(window, cx));
    let copy = "'Sheet1 (2)'";
    let got: Vec<_> = names(&mut s).into_iter().filter(|n| n.1 == Some(1)).collect();
    assert_eq!(
        got,
        vec![
            ("DblPlus".to_string(), Some(1), "LAMBDA(x,Dbl(x)+MyRng)".to_string()),
            ("Loc".to_string(), Some(1), format!("{copy}!$A$2")),
            ("MyRng".to_string(), Some(1), format!("{copy}!$A$1")),
            ("Unused".to_string(), Some(1), format!("{copy}!$A$2")),
        ]
    );
    // The original's names are as they were.
    assert_eq!(name_of(&mut s, "MyRng"), Some((None, "Sheet1!$A$1".to_string())));
    assert_eq!(name_of(&mut s, "Loc"), Some((Some(0), "Sheet1!$A$2".to_string())));
    // The copy's own formula refers to the copy's name: 5 * 2 is the original's A1, the copy has the same cells.
    assert_eq!(s.value("C1"), ibis_doc_model::CellValue::Number(10.0));
    s.key("cmd-z");
    assert!(names(&mut s).iter().all(|n| n.1 != Some(1)), "{:?}", names(&mut s));
}
