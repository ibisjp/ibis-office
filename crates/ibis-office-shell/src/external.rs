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

/// Whether this is a test run: the headless switch (`IBIS_HEADLESS`), or a program named as a test program is (see `is_test_program_name`; the app is
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
    // A test program is named after its crate or its test file and a hash of the build (`scenarios-1a2b3c4d…`, `ibis_office_shell-4d5e6f…`). The app is
    // not named so: `ibisOffice[.exe]`, the iOS app `IbisOffice`, the Android process `app_process64` (K0087: they were taken for test programs because
    // they are not named `ibisOffice…`, and then the gate never let them open anything).
    let name = name.strip_suffix(".exe").unwrap_or(name);
    name.rsplit_once('-').is_some_and(|(_, hash)| hash.len() >= 6 && hash.chars().all(|c| c.is_ascii_hexdigit()))
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
///
/// With a host (`host.rs`) the link is the host's to open (`openUrl`: it checks the scheme and opens the tab with `noopener,noreferrer`, so
/// that is in one place); a host that gave no `openUrl`, or switched `features.openLinks` off, has no links to open. Only https, http and
/// mailto go to the host.
pub fn open_url(url: &str, cx: &App) {
    if crate::host::is_connected() {
        if !crate::host::options().open_links || !crate::host::is_allowed_url(url) {
            return;
        }
        let Some(bridge) = crate::host::bridge_with(|c| c.open_url) else { return };
        let url = url.to_string();
        cx.spawn(async move |_| {
            if let Err(failure) = bridge.open_url(url).await {
                log::warn!("the host did not open the link: {:?}", failure.message);
            }
        })
        .detach();
        return;
    }
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

    /// K0087: the apps' own programs are not test programs. The iOS app's executable is `IbisOffice` (capital I: `apps/ios/Info.plist`), the Android process
    /// is `app_process64`: `is_test_program_name` ("not named ibisOffice") takes both for test programs, so the gate never lets them start anything: the
    /// About window's links and ヘルプ > ソースコード open nothing there.
    #[test]
    fn the_apps_own_program_names_are_not_test_programs() {
        for name in ["ibisOffice", "ibisOffice.exe", "IbisOffice", "app_process64", "app_process32", "ibis-office-app"] {
            assert!(!super::is_test_program_name(name), "{name} is the app, not a test program");
        }
        for name in ["scenarios-1a2b3c", "ibis_office_shell-4d5e6f", "corpus_report-7a8b9c"] {
            assert!(super::is_test_program_name(name), "{name} is a test program");
        }
    }
}
