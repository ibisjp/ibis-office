// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0054: a window whose document was changed is never replaced by the file opened from it (a new workbook with one typed cell was closed, unasked, by a csv opened over
//! it); only a new document that was never changed is. The same rule for every way of opening (`SlideEditor::replaceable_by_open`).
use super::Scenario;
use crate::docx::scenarios::DocSteps;
use crate::xlsx::scenarios::SheetSteps;
use ibis_formats::Kind;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::k0054_a_changed_new_document_is_kept_when_a_file_is_opened", || changed_is_kept(Kind::Document)),
        ("shell::k0054_a_changed_new_workbook_is_kept_when_a_file_is_opened", || changed_is_kept(Kind::Workbook)),
        ("shell::k0054_a_changed_new_presentation_is_kept_when_a_file_is_opened", || changed_is_kept(Kind::Presentation)),
        ("shell::k0054_an_unchanged_new_document_is_replaced_by_the_file_opened", || unchanged_is_replaced(Kind::Document)),
        ("shell::k0054_an_unchanged_new_workbook_is_replaced_by_the_file_opened", || unchanged_is_replaced(Kind::Workbook)),
        ("shell::k0054_an_unchanged_new_presentation_is_replaced_by_the_file_opened", || unchanged_is_replaced(Kind::Presentation)),
        ("shell::k0054_a_saved_file_window_is_kept_when_another_file_is_opened", a_file_window_is_kept),
    ]
}

/// A file of another kind than `kind`.
fn other_file(kind: Kind) -> (std::path::PathBuf, Kind) {
    let (file, other) = match kind {
        Kind::Workbook => ("test/editing/docx-report.docx", Kind::Document),
        _ => ("test/editing/xlsx-sales.xlsx", Kind::Workbook),
    };
    (crate::shell::scenario::repo_root().join(file), other)
}

/// Change the new document of `kind` the way a user does: type a character (a cell, the first paragraph) or add a slide.
fn change(s: &mut Scenario, kind: Kind) {
    match kind {
        Kind::Workbook => {
            s.click_cell("A1");
            s.type_text("x");
            s.key("enter");
        }
        Kind::Presentation => s.update(|e, w, cx| e.add_slide(w, cx)),
        _ => s.type_text("x"),
    }
    s.settle();
}

fn changed_is_kept(kind: Kind) {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(kind, w, cx));
    s.size(1200., 720.);
    change(&mut s, kind);
    assert!(s.with(|e, _| e.is_dirty()), "{kind:?}: the change counts as one (closing the window asks to save it)");
    assert!(!s.with(|e, _| e.replaceable_by_open()), "{kind:?}: a changed new document is not replaceable");
    let (path, other) = other_file(kind);
    s.update(|e, w, cx| e.open_path_or_raise(path.clone(), w, cx));
    s.wait(1500);
    s.settle();
    assert_eq!(s.window_count(), 2, "{kind:?}: the file opens in a window of its own");
    assert!(s.with(|e, _| e.has_document && e.kind() == kind && e.file_path().is_none() && e.document.can_undo()), "{kind:?}: the window keeps its new, changed document");
    let _ = other;
}

fn unchanged_is_replaced(kind: Kind) {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(kind, w, cx));
    s.size(1200., 720.);
    s.settle();
    assert!(s.with(|e, _| e.replaceable_by_open()), "{kind:?}: a new document nobody changed is replaceable");
    let (path, other) = other_file(kind);
    s.update(|e, w, cx| e.open_path_or_raise(path.clone(), w, cx));
    s.wait(1500);
    s.settle();
    assert_eq!(s.window_count(), 1, "{kind:?}: the file takes the place of the unchanged new document");
    assert!(s.with(|e, _| e.has_document && e.kind() == other && e.file_path().is_some()), "{kind:?}: the window shows the file opened");
}

/// A window of a file (changed or not) is not replaced either: the Open command of Office opens another window.
fn a_file_window_is_kept() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.settle();
    assert!(!s.with(|e, _| e.replaceable_by_open()));
    let path = crate::shell::scenario::repo_root().join("test/editing/docx-report.docx");
    s.update(|e, w, cx| e.open_path_or_raise(path.clone(), w, cx));
    s.wait(1500);
    s.settle();
    assert_eq!(s.window_count(), 2);
    assert!(s.with(|e, _| e.kind() == Kind::Workbook));
}
