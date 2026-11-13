// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The one gate for what the app starts or opens outside itself: another app (Word, Excel, PowerPoint, Preview, the
//! system's PDF app), a URL in the browser, a folder in Finder or Explorer, the updater, a tool of the desktop.
//!
//! In a test run (the scenarios, the unit tests: `running_under_test`) nothing is started: the gate records what would
//! have been (`launched`) and says no, so a scenario that presses every menu item (`menu_every_item_is_bound_and_acts_*`)
//! cannot open Word on the user's Mac (it did, 2026-11-11), and a scenario can still check that the item asked for it.
//! The app's own headless export (`--export`, a child process of ibisOffice itself) is not outside and does not come here.

#[cfg(not(target_family = "wasm"))]
use std::sync::Mutex;

use gpui_kit::App;

/// What the gate kept from starting, in order: what a test run asked for.
#[cfg(not(target_family = "wasm"))]
static LAUNCHED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Whether this is a test run: the headless switch (`IBIS_HEADLESS`), or a program that is not the app (the app is
/// `ibisOffice[.exe]`; a test program is named after its crate or file: `scenarios-…`, `ibis_office_shell-…`). Not by the
/// arguments: a file named `latest.docx` is an argument too (GitHub issue #19).
#[cfg(not(target_family = "wasm"))]
pub fn running_under_test() -> bool {
    if std::env::var_os("IBIS_HEADLESS").is_some() {
        return true;
    }
    let exe = std::env::current_exe().ok();
    let name = exe.as_deref().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("");
    is_test_program_name(name)
}

#[cfg(not(target_family = "wasm"))]
pub(crate) fn is_test_program_name(name: &str) -> bool {
    !name.starts_with("ibisOffice")
}

/// Whether `what` (a few words saying what would start: `open -a …/Word.app …`, `url https://…`) may be started now. In a
/// test run it is recorded and the answer is no; the caller then does nothing (or what it does when the start fails).
pub fn may_launch(what: impl FnOnce() -> String) -> bool {
    #[cfg(not(target_family = "wasm"))]
    if running_under_test() {
        let what = what();
        log::info!("test run: not started: {what}");
        LAUNCHED.lock().unwrap_or_else(|e| e.into_inner()).push(what);
        return false;
    }
    let _ = what;
    true
}

/// What the gate kept from starting since the last call, taken out (for a test to check that an item asked to open
/// something).
#[cfg(not(target_family = "wasm"))]
pub fn take_launched() -> Vec<String> {
    std::mem::take(&mut *LAUNCHED.lock().unwrap_or_else(|e| e.into_inner()))
}

/// `url` opened by the system (the browser, or the app of a file URL), through the gate.
pub fn open_url(url: &str, cx: &App) {
    if may_launch(|| format!("url {url}")) {
        cx.open_url(url);
    }
}

/// `path` opened by the system's app for it (a folder in Finder or Explorer), through the gate.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn open_with_system(path: &std::path::Path, cx: &App) {
    if may_launch(|| format!("open {}", path.display())) {
        cx.open_with_system(path);
    }
}

/// `path` shown in Finder or Explorer, through the gate.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn reveal_path(path: &std::path::Path, cx: &App) {
    if may_launch(|| format!("reveal {}", path.display())) {
        cx.reveal_path(path);
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::{may_launch, take_launched};

    #[test]
    fn a_test_program_starts_nothing_and_the_gate_says_what_it_kept() {
        let _ = take_launched();
        assert!(!may_launch(|| "open -a Word.app a.docx".into()));
        assert_eq!(take_launched(), ["open -a Word.app a.docx"]);
        assert!(take_launched().is_empty());
    }
}
