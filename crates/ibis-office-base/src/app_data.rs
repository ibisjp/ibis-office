// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Where the app keeps what is the user's (the recent files, AutoRecover, the backup and chart-template settings, ...):
//! the user's application data directory (`~/Library/Application Support` on macOS, `%APPDATA%` on Windows,
//! `$XDG_DATA_HOME` or `~/.local/share` elsewhere), and `ibisOffice` under it.
//!
//! The scenarios (built with the `scenarios` feature) never touch the user's own: they keep it all under
//! `target/scenario-user-data` (todo-fix #83: the user's recent-files list had the scenarios' test files in it).
//! `IBIS_DATA_DIR` puts it anywhere else.

use std::path::PathBuf;

fn env(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The directory the scenarios keep the user's data in (instead of the user's own), if this is a scenario or test build.
pub(crate) fn scenario_dir() -> Option<PathBuf> {
    #[cfg(all(any(target_os = "macos", target_os = "linux"), feature = "scenarios"))]
    {
        // The repository's root: this crate is crates/ibis-office-base.
        let root = std::fs::canonicalize(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).expect("the repository");
        return Some(root.join("target/scenario-user-data"));
    }
    // Unreachable when the scenarios block above is compiled in too (it returns first).
    #[allow(unreachable_code)]
    #[cfg(any(test, feature = "test-user-data"))]
    {
        return Some(std::env::temp_dir().join("ibisOffice-test-user-data"));
    }
    #[allow(unreachable_code)]
    None
}

/// The directory under which the app's own folder (`ibisOffice`) lies: where the user's data goes.
pub fn base() -> Option<PathBuf> {
    if let Some(dir) = env("IBIS_DATA_DIR").or_else(scenario_dir) {
        return Some(dir);
    }
    #[cfg(target_os = "macos")]
    let base = env("HOME").map(|home| home.join("Library/Application Support"));
    #[cfg(target_os = "windows")]
    let base = env("APPDATA");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let base = env("XDG_DATA_HOME").or_else(|| env("HOME").map(|home| home.join(".local/share")));
    base
}

/// Where the fonts downloaded by セットアップ (`ibis_office_shell::font_setup`) are kept: `ibisOffice/fonts` under [`base`], one
/// folder for each font (`fonts/<id>/<font file>`, with its licence file beside it). Not the operating system's font folder: the
/// fonts are the app's own (they are removed with the app's data, need no administrator and are not offered to other apps).
#[cfg(not(target_family = "wasm"))]
pub fn fonts_dir() -> Option<PathBuf> {
    Some(base()?.join("ibisOffice").join("fonts"))
}

/// The font files in [`fonts_dir`] (`.ttf`, `.otf`, `.ttc` in the folder of each font), sorted. The app gives them to its text
/// system at start, so they are fonts of the app (a document that names one draws with it).
#[cfg(not(target_family = "wasm"))]
pub fn downloaded_font_files() -> Vec<PathBuf> {
    let Some(root) = fonts_dir() else { return Vec::new() };
    let Ok(folders) = std::fs::read_dir(root) else { return Vec::new() };
    let mut files = Vec::new();
    for folder in folders.flatten() {
        let Ok(inside) = std::fs::read_dir(folder.path()) else { continue };
        for file in inside.flatten() {
            let path = file.path();
            let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
            if matches!(ext.as_deref(), Some("ttf" | "otf" | "ttc")) {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}
