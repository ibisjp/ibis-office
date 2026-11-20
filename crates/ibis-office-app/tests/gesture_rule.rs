// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What a touch can start must be undoable (#209, `tasks/doc/design/touch-gesture-transaction.md`).
//!
//! The first finger of a touch screen works like the mouse; when a second finger comes down, the window takes back
//! everything the first one did (`shell/gesture.rs`): the document's edits (they all go through `Document::execute`),
//! and the view state. It cannot take back what leaves the window: the clipboard, files, the network. Those go through
//! `SlideEditor::after_gesture`, which runs them when the touch is final.
//!
//! The press, drag and tap handlers of the page live in the files below. They must not write to the clipboard or the file
//! system directly. A line that must (with the reason) is marked `// gesture-ok: <reason>`. A new file with pointer
//! handlers is added to the list.

use std::path::Path;

/// The files with the press, drag and tap handlers of the page, from the crates' `src` directories.
const POINTER_HANDLER_FILES: &[&str] = &[
    "ibis-office-shell/src/editor.rs",
    "ibis-office-shell/src/selection_handles.rs",
    "ibis-office-shell/src/crop_mode.rs",
    "ibis-office-shell/src/ink.rs",
    "ibis-office-docx/src/doc_ui.rs",
    "ibis-office-xlsx/src/sheet_ui.rs",
    "ibis-office-xlsx/src/headers.rs",
    "ibis-office-xlsx/src/fill.rs",
    "ibis-office-pptx/src/slide_view.rs",
    "ibis-office-pptx/src/grid_view.rs",
    "ibis-office-pptx/src/thumbnails.rs",
];

/// What a handler must leave to `after_gesture`: it cannot be taken back.
const OUTSIDE_THE_WINDOW: &[&str] = &["write_to_clipboard", "std::fs::write", "std::fs::remove", "std::fs::rename", "std::fs::create", "open_url(", "std::process::Command"];

fn outside_the_window(line: &str) -> Option<&'static str> {
    let code = line.trim_start();
    if code.starts_with("//") || code.contains("gesture-ok:") {
        return None;
    }
    OUTSIDE_THE_WINDOW.iter().copied().find(|word| code.contains(word))
}

#[test]
fn pointer_handlers_leave_what_cannot_be_undone_to_after_gesture() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut found = Vec::new();
    for file in POINTER_HANDLER_FILES {
        let path = crates.join(file);
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e} (the list in this test names a file that moved)", path.display()));
        for (i, line) in text.lines().enumerate() {
            if let Some(word) = outside_the_window(line) {
                found.push(format!("{}:{}: {word}: {}", path.display(), i + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "A press or drag handler does something that cannot be taken back when a second finger cancels the touch. Do it through `SlideEditor::after_gesture` (or mark the line `// gesture-ok: <reason>`):\n{}",
        found.join("\n")
    );
}

#[test]
fn the_search_finds_them() {
    assert!(outside_the_window("cx.write_to_clipboard(item);").is_some());
    assert!(outside_the_window("    std::fs::write(&path, bytes).ok();").is_some());
    assert!(outside_the_window("cx.write_to_clipboard(item); // gesture-ok: a copy command, not a press").is_none());
    assert!(outside_the_window("    // cx.write_to_clipboard is for after_gesture").is_none());
    assert!(outside_the_window("e.after_gesture(window, cx, |e, _, cx| e.copy(cx));").is_none());
}
