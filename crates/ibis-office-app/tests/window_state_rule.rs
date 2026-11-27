// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! UI state that belongs to a window (which menu or dropdown is open, hover, focus, drag, a popup's anchor, ...) is never kept in
//! a `thread_local!` (or a `static`): every window of the app runs on the one thread, so all windows would share it, and one
//! window's render would overwrite the other's (the start window's 「…」 menu closed when a sheet screen drew, 2026-11-04).
//! Put it in a field of the window's own editor (`SlideEditor`), or in a table keyed by the window's id that the code reads
//! through the `window` it is drawing. A document window's view of its document (the tracked-change display, the hidden-text display,
//! the reviewer filter: review 0.8.5, `code-shared-view-state`) is the same: it lives in the window's `Resources::markup_view`.
//! This test looks at the shell (`crates/ibis-office-shell/src`, named `shell/` below) and `src/docx/` with a plain search: a `thread_local!` there is allowed
//! only when it is listed below with the reason it is not per-window state. Add a line only after checking that two windows
//! open at once cannot disturb each other through it (the scenario `shell::followup_recent_menu_other_window`).
//! Rule: docs/rule/implimentationRule.md, 「ウィンドウごとの UI 状態」.

use std::path::{Path, PathBuf};

/// (file under src/, static, why it is not per-window UI state)
const ALLOWED: &[(&str, &str, &str)] = &[
    ("shell/access_key.rs", "DUPLICATES", "scenarios only: the count of duplicate access keys found while assigning"),
    ("shell/access_key.rs", "OVERRIDE", "scenarios only: forces the access keys on or off"),
    ("shell/access_key.rs", "FRAME", "a table keyed by the window's id (the keys assigned in the frame being drawn)"),
    ("shell/gradient_panel.rs", "IDS", "an interner of `&'static str` ids by prefix and index (the same for every window; holds no window state)"),
    ("shell/chrome.rs", "FORCED_SIDEBAR", "scenarios only: the panel a scenario forces"),
    ("shell/mobile_os.rs", "FORCED", "scenarios only (set by ForcedMobileOs): stands for the build's OS (iOS or Android), which is the process's and the same for every window (K0076)"),
    ("shell/chrome.rs", "LAST_SIDEBAR_TITLE", "scenarios only: the title the scenario reads back"),
    ("shell/chrome.rs", "DROPDOWN_LAYOUT", "scenarios only: measurements read back by the scenarios"),
    ("shell/chrome.rs", "DROPDOWN_ROW_HEIGHTS", "a measurement cache by dropdown id (the same id has the same row height in every window)"),
    ("shell/chrome.rs", "DROPDOWN_BUTTON_WIDTHS", "a measurement cache by dropdown id (the same id has the same width in every window)"),
    ("shell/chrome.rs", "DROPDOWN_BUTTON_BOTTOMS", "a table keyed by the window's id and the dropdown id (the button's bottom edge differs per window, so two windows do not disturb each other)"),
    ("shell/chrome.rs", "PANEL_MODE", "a hand-off inside one build of a panel: set by SidebarPanel's builder, taken by the host in the same render call"),
    ("shell/chrome.rs", "PANEL_KEY", "a hand-off inside one build of a panel (see PANEL_MODE)"),
    ("shell/chrome.rs", "PANEL_BACKDROP", "a hand-off inside one build of a panel (see PANEL_MODE)"),
    ("shell/chrome.rs", "PANEL_ESC_CLOSES", "a hand-off inside one build of a panel (see PANEL_MODE)"),
    ("shell/chrome.rs", "PANEL_FILL", "a hand-off inside one build of a panel (see PANEL_MODE)"),
    ("shell/chrome.rs", "SIDEBAR_FOOTER", "a hand-off inside one build of a panel (see PANEL_MODE)"),
    ("shell/chrome.rs", "PANEL_HEADER", "a hand-off inside one build of a panel (see PANEL_MODE)"),
    ("shell/chrome.rs", "PANEL_PINNED", "a hand-off inside one build of a panel (see PANEL_MODE)"),
    ("shell/chrome/synced_field.rs", "STRICT", "scenarios only: entity ids of the fields to check"),
    ("shell/dialog/form.rs", "OPTIONS", "a hand-off inside one build of a dialog form (set and read in the same call)"),
    ("shell/links.rs", "CAPTURE_URLS", "scenarios only: the URLs a scenario captures instead of opening"),
    ("shell/menu_bar.rs", "WIDTHS", "a cache of a menu label's width, keyed by the content (text system, font, bold, label): every window asking for the same label gets the same width (#299)"),
    ("shell/menu_search_history.rs", "CACHE", "the menu search history (data of the settings file, the same for every window)"),
    ("shell/menu_layout.rs", "PLANNED_OVERRIDE", "tests and scenarios only: forces the planned menu lines on or off for the thread that runs the check (both forms of the menus are checked in one process; not a state of a window)"),
    ("shell/shape_clipboard.rs", "TATE_SIDE", "the app's own side table of the last copy in a document (the 縦中横 the native bytes do not carry; the same for every window, as the clipboard is)"),
    ("shell/native_shape_clipboard.rs", "SIMULATED", "scenarios only: the scenario's fake clipboard of shapes (the system's clipboard, the same for every window; moved here from the app's root with the shell's crate)"),
    ("shell/native_shape_clipboard.rs", "SIMULATED_HTML", "scenarios only: the scenario's fake clipboard's HTML (the system's clipboard, the same for every window; moved here from the app's root with the shell's crate)"),
    ("shell/native_shape_clipboard.rs", "SIMULATED_EMBED", "scenarios only: the scenario's fake clipboard's embedded worksheet (the system's clipboard, the same for every window; moved here from the app's root with the shell's crate)"),
    ("shell/native_shape_clipboard.rs", "WRITTEN_VECTOR", "scenarios only: what a scenario's copy as a picture wrote (the system's clipboard, the same for every window; moved here from the app's root with the shell's crate)"),
    ("shell/native_shape_clipboard.rs", "SIMULATED_VECTOR", "scenarios only: the scenario's fake clipboard's vector picture (the system's clipboard, the same for every window; moved here from the app's root with the shell's crate)"),
    ("shell/spelling.rs", "LEARNED", "tests and scenarios only: the words learned (the user's dictionary, the same for every window; moved here from the app's root with the shell's crate)"),
    ("shell/qat.rs", "CACHE", "the quick access toolbar's items (data of the settings file, the same for every window)"),
    ("shell/scenario.rs", "CURRENT", "scenarios only"),
    ("shell/scenario.rs", "TEXT_SYSTEM", "scenarios only"),
    ("shell/update.rs", "AVAILABLE", "the newer version found (app-wide data)"),
    ("shell/update.rs", "CHECKING", "a check is running (app-wide)"),
    ("shell/update.rs", "CHECKED_NOW", "scenarios' counter (app-wide)"),
    ("shell/update.rs", "ALERTS", "scenarios' record of the update alerts shown (app-wide, test only)"),
    ("shell/update.rs", "ASKED_AGAIN", "scenarios' log (app-wide)"),
    ("shell/update.rs", "BACKGROUND", "a background check is running (app-wide)"),
    ("shell/update.rs", "LAST_BACKGROUND", "when the last background check started (app-wide)"),
    ("shell/file_actions.rs", "NEW_FILES", "the count of new files of each kind this session has made (app-wide: closing one does not give its number back, as in Office)"),
    ("shell/chart_insert.rs", "KIND_CHARTS", "the pictures of 挿入 > グラフ made from fixed data, by screen and kind (the data does not depend on the document or the window)"),
    ("shell/chart_insert.rs", "BLANK", "an empty presentation used to ask for a new chart of a kind (fixed data, the same for every window)"),
    ("shell/host.rs", "HOST", "the page's host connection (the web page has one, shared by all its windows)"),
    ("shell/host.rs", "RECENT", "the host's list of recent files (app-wide data, the same for every window)"),
    ("shell/color_editor.rs", "SQUARES", "a cache of the colour field's gradient squares by size and hue (the same picture in every window)"),
    ("shell/toolbar_customize.rs", "IDS", "an interner of `&'static str` ids by key (the same for every window; holds no window state)"),
    ("shell/toolbar_layout.rs", "KNOWN", "a cache of what the buttons seen so far look like, by key (the same button looks the same in every window)"),
    ("shell/toolbar_layout.rs", "SEEN", "the keys of each scene as the format made them (data of the format's toolbar, the same for every window)"),
    ("shell/start_menu.rs", "FORCED", "scenarios only: a scenario on a Mac asks for the bar the other systems draw"),
    ("shell/editor.rs", "SUBSET_FOLLOWED_AT", "the web page's font chunks follow (the fonts the page has are the page's, shared by every window; web only)"),
    ("shell/editor.rs", "SUBSET_FOLLOW_PENDING", "the web page's font chunks follow (see SUBSET_FOLLOWED_AT; web only)"),
    ("shell/host_file_dialog.rs", "NEXT_ID", "a counter of the ids of the host's file dialogs (app-wide)"),
    ("shell/host_file_dialog.rs", "LAST_FOLDER", "the folder last used by a file dialog, for the next one of this run (app-wide, as the system's dialogs remember it)"),
    ("shell/shape_format_pane.rs", "LAST_BUILT", "scenarios only: the pane built last, read back by the scenarios"),
    ("shell/dicom_dialog.rs", "FILE", "scenarios only: the file the picker would have given"),
];

/// (file under src/, static, why it is not per-window state): the statics of `src/docx/` that hold a value (`Atomic*`, `Mutex`,
/// `RwLock`, `OnceLock`) and its `thread_local!`s.
const ALLOWED_DOCX: &[(&str, &str, &str)] = &[
    ("docx/flow_layout.rs", "FONTS_SEEN", "the fonts' generation the layout caches (PIECES) were made for: the fonts are the process's, so a change clears the caches of every window"),
    ("docx/flow_layout.rs", "PIECES", "a cache of laid-out blocks, keyed by the block's content and by the window's `MarkupView`"),
    ("docx/flow_layout.rs", "PAGINATIONS", "a cache of paginations, keyed by the document and by the window's `MarkupView`"),
    ("docx/page_view.rs", "CLUSTER_SHAPES", "a cache of shaped text, keyed by the content (text system, text, font, size): any window asking for the same text gets the same shape (#299)"),
    ("docx/editing.rs", "FLOW_LINES", "a cache of the lines of a pagination, keyed by the pagination instance it holds (the lines are a pure function of it; no window state, #299)"),
    ("docx/clipboard.rs", "TEXT_CLIPBOARD", "the app's own clipboard (shared by every window, as the system's is)"),
    ("docx/clipboard.rs", "CLIPBOARD", "the app's own clipboard (shared by every window, as the system's is)"),
    ("docx/clipboard.rs", "NEXT", "a counter of the ids of pasted pictures"),
    ("docx/repair.rs", "DAMAGED_DOCUMENT", "the file that could not be opened most recently (app-wide: the File menu's repair)"),
    ("docx/toolbar.rs", "DATE_UPDATES", "the 日付と時刻 dialog's check box, remembered for the next time (app-wide, as Word's)"),
    ("docx/flow_layout.rs", "INSTALLED_FIRST", "a switch of the tools that count what asking the installed typeface's table first changes (`tests/docx/fallback_carry.rs`); off in the app, no window sets it"),
    ("docx/flow_layout.rs", "CARRY_OFF", "a switch of the tools that count what Word's carrying over of a fallback typeface changes (`tests/docx/fallback_carry.rs`); off in the app, no window sets it"),
    ("docx/art_border.rs", "PICTURES", "the art borders' pictures (the same for every document)"),
    ("docx/art_border.rs", "SOURCE", "where the art borders' pictures come from (app-wide)"),
    ("docx/art_border.rs", "STATES", "the art borders' pictures being fetched (app-wide)"),
    ("docx/art_border.rs", "IMAGES", "the art borders' decoded pictures (the same for every document)"),
    ("docx/art_border.rs", "GENERATION", "the art borders' pictures that have come (app-wide)"),
    ("docx/art_border.rs", "PENDING", "the art borders' pictures being fetched (app-wide)"),
    ("docx/art_border.rs", "DEVICE_SCALE", "the screen's scale (app-wide)"),
    ("docx/art_border.rs", "WARM_PX_PER_PT", "the art borders' pictures' scale (app-wide)"),
];

/// The names of the `static`s of `text` whose type holds a value shared by the whole process.
fn shared_value_statics(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in text.lines() {
        let line = line.trim_start();
        let line = line.strip_prefix("pub(crate) ").or_else(|| line.strip_prefix("pub(super) ")).or_else(|| line.strip_prefix("pub ")).unwrap_or(line);
        let Some(rest) = line.strip_prefix("static ") else { continue };
        let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        let declared = !name.is_empty() && name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') && rest[name.len()..].trim_start().starts_with(':');
        let ty = &rest[name.len()..];
        if declared && ["Atomic", "Mutex", "RwLock", "OnceLock"].iter().any(|k| ty.contains(k)) {
            names.push(name);
        }
    }
    names
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The names of the statics declared in the `thread_local!` blocks of `text`.
fn thread_local_statics(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("thread_local!") {
        let after = &rest[at + "thread_local!".len()..];
        let Some(open) = after.find('{') else { break };
        let mut depth = 0usize;
        let mut end = after.len();
        for (i, c) in after[open..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + i;
                        break;
                    }
                }
                _ => {}
            }
        }
        let body = &after[open..end];
        let mut b = body;
        while let Some(s) = b.find("static ") {
            let name: String = b[s + 7..].chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            // `static NAME:` declares one; `&'static str` inside a type does not (a lower-case name, or no `:` after it).
            let declared = !name.is_empty() && name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') && b[s + 7 + name.len()..].trim_start().starts_with(':');
            if declared {
                names.push(name);
            }
            b = &b[s + 7..];
        }
        rest = &after[end.min(after.len())..];
    }
    names
}

/// The shell's sources (crates/ibis-office-shell/src), named `shell/<path>` in `ALLOWED` as when they were `src/shell/` of
/// this crate (tasks/doc/crate-split-plan.md, 12).
fn shell_src() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../ibis-office-shell/src")
}

#[test]
fn no_window_ui_state_in_a_thread_local() {
    let src = shell_src();
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    // The shell's tests (`src/tests/`, this crate's `src/tests/shell/` before) are not looked at, as before.
    files.retain(|file| !file.starts_with(src.join("tests")));
    assert!(files.len() > 20, "found only {} files under {}", files.len(), src.display());
    let mut found = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        let relative = format!("shell/{}", file.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/"));
        for name in thread_local_statics(&text) {
            if !ALLOWED.iter().any(|(f, n, _)| *f == relative && *n == name) {
                found.push(format!("{relative}: {name}"));
            }
        }
    }
    assert!(
        found.is_empty(),
        "ウィンドウごとの UI 状態（開いているメニュー・ホバー・フォーカス・ドラッグ等）を thread_local に置かない（全ウィンドウで共有され、別ウィンドウの描画が上書きする）。\
         エディタ（SlideEditor）のフィールドか、ウィンドウ ID をキーにした表に置く。ウィンドウ状態でないと確かめたものは tests/window_state_rule.rs の ALLOWED に理由つきで足す:\n{}",
        found.join("\n")
    );
}

#[test]
fn every_allowed_entry_still_exists() {
    for (file, name, _) in ALLOWED {
        let path = shell_src().join(file.strip_prefix("shell/").expect("an entry of the shell"));
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert!(thread_local_statics(&text).iter().any(|n| n == name), "{file}: {name} is not a thread_local any more; remove it from ALLOWED");
    }
}

#[test]
fn the_search_finds_them() {
    let text = "thread_local! {\n    /// doc\n    static A: Cell<bool> = const { Cell::new(false) };\n    static B: u8 = 0;\n}\nthread_local! {static C: u8 = 1;}";
    assert_eq!(thread_local_statics(text), vec!["A", "B", "C"]);
}

/// The document screen's sources (crates/ibis-office-docx/src), named `docx/<path>` in `ALLOWED_DOCX` as when they were `src/docx/` of
/// this crate (tasks/doc/crate-split-plan.md, 15).
fn docx_src() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../ibis-office-docx/src")
}

#[test]
fn no_document_view_state_in_a_docx_static() {
    let src = docx_src();
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    // The checks (`src/tests/`) are not looked at, as before.
    files.retain(|file| !file.starts_with(src.join("tests")));
    assert!(files.len() > 20, "found only {} files under {}", files.len(), src.display());
    let mut found = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        let relative = format!("docx/{}", file.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/"));
        for name in thread_local_statics(&text).into_iter().chain(shared_value_statics(&text)) {
            if !ALLOWED_DOCX.iter().any(|(f, n, _)| *f == relative && *n == name) {
                found.push(format!("{relative}: {name}"));
            }
        }
    }
    assert!(
        found.is_empty(),
        "文書画面の窓ごとの状態（変更履歴の表示・隠し文字・校閲者の絞り込み等）を static / thread_local に置かない（全ウィンドウで共有され、別の文書にも効く）。\
         ウィンドウの `Resources`（`markup_view`）か、エディタのフィールドに置く。ウィンドウ状態でないと確かめたものは tests/window_state_rule.rs の ALLOWED_DOCX に理由つきで足す:\n{}",
        found.join("\n")
    );
}

#[test]
fn every_allowed_docx_entry_still_exists() {
    for (file, name, _) in ALLOWED_DOCX {
        let text = std::fs::read_to_string(docx_src().join(file.strip_prefix("docx/").expect("an entry of the document screen"))).unwrap_or_else(|e| panic!("{file}: {e}"));
        assert!(thread_local_statics(&text).iter().chain(shared_value_statics(&text).iter()).any(|n| n == name), "{file}: {name} is not a shared static any more; remove it from ALLOWED_DOCX");
    }
}
