// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Native quit guards: the Dock's Quit must use the same save workflow as the app menu.
use crate::shell::scenario::Scenario;
use crate::docx::scenarios::DocSteps;
use crate::xlsx::scenarios::SheetSteps;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::native_quit_unsaved_each_format", unsaved_each_format),
        ("shell::native_quit_save_existing_file", save_existing_file),
        ("shell::native_quit_multiple_windows", multiple_windows),
        ("shell::native_quit_guard_reentry_and_unsubscribe", guard_reentry_and_unsubscribe),
    ]
}

fn new_file(kind: ibis_pptx::Kind) -> Scenario {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(kind, w, cx));
    s.quit_with_last_window();
    s
}

fn edit(s: &mut Scenario, kind: ibis_pptx::Kind) {
    match kind {
        ibis_pptx::Kind::Document => {
            let (id, _) = s.paragraph(0);
            s.click_text(ibis_doc_model::FlowPos::new(id, 0));
            s.type_text("unsaved");
        }
        ibis_pptx::Kind::Workbook => {
            s.click_cell("A1");
            // Do not commit the cell: quitting must catch pending typed input too.
            s.type_text("unsaved");
        }
        ibis_pptx::Kind::Presentation => s.update(|e, _, cx| {
            let id = e.document.ids().new_shape_id();
            let frame = ibis_doc_model::EmuRect::new(0, 0, 914_400, 914_400);
            e.execute(vec![ibis_doc_model::Command::InsertShape {
                slide: e.current_slide,
                index: 0,
                shape: ibis_doc_model::Shape::text_box(id, frame, "unsaved"),
            }], cx);
        }),
    }
    assert!(s.with(|e, _| e.is_dirty()));
}

fn unsaved_each_format() {
    for kind in [ibis_pptx::Kind::Document, ibis_pptx::Kind::Workbook, ibis_pptx::Kind::Presentation] {
        let mut s = new_file(kind);
        assert!(s.with_app(|app| app.should_quit()), "{kind:?}: clean files permit native termination");
        assert_eq!(s.window_count(), 1, "the guard itself does not remove clean windows");
        edit(&mut s, kind);
        assert!(!s.with_app(|app| app.should_quit()), "{kind:?}: refuse native termination before saving");
        s.settle();
        assert!(s.with(|e, _| e.save_confirm_dialog_open && e.close_window_after_confirm));
        assert_eq!(s.window_count(), 1);
        assert!(!s.quit_requested());
        // Repeated Dock requests reuse the save question rather than bypassing it.
        assert!(!s.with_app(|app| app.should_quit()));
        s.settle();
        s.update(|e, _, cx| e.close_save_confirm_dialog(cx));
        assert!(s.with(|e, _| e.is_dirty() && !e.save_confirm_dialog_open));
        assert!(!s.quit_requested(), "Cancel keeps the edited document");
        assert!(!s.with_app(|app| app.should_quit()));
        s.settle();
        s.update(|e, w, cx| e.confirm_discard_and_close(w, cx));
        assert_eq!(s.window_count(), 0);
        assert!(s.quit_requested(), "Discard completes the original native quit");
        assert!(s.with_app(|app| app.should_quit()), "the final quit is allowed, without a request loop");
    }
}

fn save_existing_file() {
    let mut s = new_file(ibis_pptx::Kind::Document);
    // Scenario::open copies an input into the scenario root. Save in a subdirectory so the
    // reopen step cannot copy the saved file onto itself (which would truncate its contents).
    let saved_dir = s.screenshot_path("saved");
    std::fs::create_dir_all(&saved_dir).unwrap();
    let path = s.save_as(saved_dir.join("native-quit.docx").to_str().unwrap());
    edit(&mut s, ibis_pptx::Kind::Document);
    assert!(!s.with_app(|app| app.should_quit()));
    s.settle();
    s.update(|e, w, cx| e.confirm_save_and_close(w, cx));
    assert_eq!(s.window_count(), 0);
    assert!(s.quit_requested());
    let written = std::fs::read(&path).expect("the native quit's Save writes the document");
    assert!(!written.is_empty(), "Save must produce a nonempty document before closing");
    let saved = crate::read_file(&path).expect("the saved document is readable");
    assert!(saved.presentation.flow.is_some(), "Save must preserve the Word document format");
    drop(s);
    let mut s = Scenario::open(path.to_str().unwrap());
    assert!(s.paragraph(0).1.contains("unsaved"), "Save preserves the edit before completing quit");
    assert_eq!(std::fs::read(&path).unwrap(), written, "reopening cannot change the saved source");
    assert!(s.with_app(|app| app.should_quit()));
}

fn multiple_windows() {
    let mut s = new_file(ibis_pptx::Kind::Document);
    edit(&mut s, ibis_pptx::Kind::Document);
    // A clean start window must not hide the dirty editor when the OS asks to quit.
    s.with_app(|app| crate::open_editor(None, None, app));
    s.settle();
    assert_eq!(s.window_count(), 2);
    assert!(!s.with_app(|app| app.should_quit()));
    s.settle();
    assert_eq!(s.window_count(), 1, "the clean start window closes, the edited document stays");
    assert!(s.with(|e, _| e.save_confirm_dialog_open));
    s.update(|e, _, cx| e.close_save_confirm_dialog(cx));
    assert!(!s.quit_requested());
    assert!(!s.with_app(|app| app.should_quit()));
    s.settle();
    s.update(|e, w, cx| e.confirm_discard_and_close(w, cx));
    assert_eq!(s.window_count(), 0);
    assert!(s.quit_requested());
}

fn guard_reentry_and_unsubscribe() {
    use std::{cell::Cell, rc::Rc};
    let mut s = Scenario::start_view();
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    let guard = s.with_app(|app| app.on_quit_requested(move |app| {
        seen.set(seen.get() + 1);
        assert!(!app.should_quit(), "a nested request cannot authorize termination");
        false
    }));
    assert!(!s.with_app(|app| app.should_quit()));
    assert_eq!(calls.get(), 1, "a reentrant request does not recurse through the guards");
    assert!(!s.with_app(|app| app.should_quit()));
    assert_eq!(calls.get(), 2, "a refused request keeps its guard installed");
    drop(guard);
    assert!(s.with_app(|app| app.should_quit()), "dropping a subscription removes its guard");
    assert_eq!(calls.get(), 2);
}
