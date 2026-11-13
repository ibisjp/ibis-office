// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The folder the save dialog opens in for a file that has no folder yet (a new file) (issue #143).
//!
//! The first save after installing opens in the user's documents folder of the OS (Windows: the Documents known
//! folder, which is OneDrive's when it is redirected there; macOS: `~/Documents`; Linux: `XDG_DOCUMENTS_DIR`). A save
//! through the dialog remembers its folder in the settings (`save.last-folder`), and the next new file's dialog opens
//! there. A remembered folder that is gone is not used: the documents folder is. A file that has a folder is saved
//! as in it (名前を付けて保存 starts in the file's own folder), and that is not decided here.
use std::path::{Path, PathBuf};

/// The settings key of the folder saved to last.
pub const LAST_FOLDER_KEY: &str = "save.last-folder";

/// The user's documents folder of the OS (`None` if it cannot be found; the home folder is then used).
#[cfg(not(target_family = "wasm"))]
pub fn documents_dir() -> Option<PathBuf> {
    let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    #[cfg(target_os = "windows")]
    {
        windows_documents_dir().or_else(|| env("USERPROFILE").map(|p| p.join("Documents")))
    }
    #[cfg(target_os = "macos")]
    {
        env("HOME").map(|home| home.join("Documents"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let home = env("HOME")?;
        if let Some(dir) = env("XDG_DOCUMENTS_DIR") {
            return Some(dir);
        }
        let config = env("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        let text = std::fs::read_to_string(config.join("user-dirs.dirs")).unwrap_or_default();
        Some(xdg_user_dir(&text, "XDG_DOCUMENTS_DIR", &home).unwrap_or_else(|| home.join("Documents")))
    }
}

/// The Documents known folder (`FOLDERID_Documents`): where Windows says it is, which is OneDrive's folder when the
/// folder is redirected there (not `%USERPROFILE%\Documents`).
#[cfg(target_os = "windows")]
fn windows_documents_dir() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{FOLDERID_Documents, KF_FLAG_DEFAULT, SHGetKnownFolderPath};
    // SAFETY: the returned string is read up to its terminating 0 and freed once, as the API says.
    unsafe {
        let raw = SHGetKnownFolderPath(&FOLDERID_Documents, KF_FLAG_DEFAULT, None).ok()?;
        let path = std::ffi::OsString::from_wide(raw.as_wide());
        CoTaskMemFree(Some(raw.0 as *const std::ffi::c_void));
        Some(PathBuf::from(path)).filter(|p| !p.as_os_str().is_empty())
    }
}

/// The value of `name` in the text of `user-dirs.dirs` (`XDG_DOCUMENTS_DIR="$HOME/Documents"`), with `$HOME` filled in.
#[allow(dead_code)] // Linux reads the file; the scenario tests read it on every OS.
pub fn xdg_user_dir(text: &str, name: &str, home: &Path) -> Option<PathBuf> {
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix(name).and_then(|r| r.trim_start().strip_prefix('=')) else { continue };
        let value = rest.trim().trim_matches('"');
        let path = match value.strip_prefix("$HOME") {
            Some(tail) => home.join(tail.trim_start_matches('/')),
            None => PathBuf::from(value),
        };
        // `XDG_DOCUMENTS_DIR="$HOME"` is what the file says when the folder is turned off: the home folder is no documents folder.
        return (!value.is_empty() && path != home).then_some(path);
    }
    None
}

/// The folder to open in: the remembered one when it is still a folder, else the documents folder, else the home
/// folder, else the current one. Pure so that the fallbacks are tested.
pub fn resolve_default_folder(last: Option<&str>, documents: Option<PathBuf>, home: Option<PathBuf>) -> PathBuf {
    if let Some(last) = last.map(str::trim).filter(|l| !l.is_empty()) {
        let last = PathBuf::from(last);
        if last.is_dir() {
            return last;
        }
    }
    documents.filter(|d| d.is_dir()).or(home).unwrap_or_else(|| PathBuf::from("."))
}

/// The folder a new file's save dialog opens in (see the module's words).
#[cfg(not(target_family = "wasm"))]
pub fn default_save_folder() -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).filter(|v| !v.is_empty()).map(PathBuf::from);
    resolve_default_folder(crate::settings::get(LAST_FOLDER_KEY).as_deref(), documents_dir(), home)
}

/// A file was saved to `path` through the dialog: its folder is the next new file's.
pub fn remember_save_folder(path: &Path) {
    if let Some(folder) = path.parent().filter(|f| !f.as_os_str().is_empty()) {
        crate::settings::set(LAST_FOLDER_KEY, &folder.to_string_lossy());
    }
}
