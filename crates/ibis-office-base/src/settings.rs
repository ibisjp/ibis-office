// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The app's settings (ツール > 設定…): a few `key=value` lines kept in the
//! user's application data directory (`~/Library/Application
//! Support/ibisOffice/settings.txt` on macOS, `%APPDATA%\ibisOffice` on
//! Windows), beside the recent files; in the browser, its localStorage
//! (`ibisOffice.<key>`). `IBIS_SETTINGS_DIR` puts the file elsewhere (the
//! scenario tests). A missing or unreadable file is no settings; a failed
//! write is logged.

#[cfg(not(target_family = "wasm"))]
mod store {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    /// The file's path, its stamp (modified time and size) when read, and its entries. The stamp is looked at on every call
    /// (a stat, not a read) so that a change made from outside (another window, another process, an editor) is read again.
    type Stamp = Option<(std::time::SystemTime, u64)>;
    /// The path, the stamp, when the stamp was last looked at, and the entries.
    static CACHE: Mutex<Option<(PathBuf, Stamp, crate::clock::Instant, Vec<(String, String)>)>> = Mutex::new(None);
    /// A stat costs about 70 microseconds on Windows (more than the lookup), and the lookups come hundreds of times a frame; so the stamp is
    /// looked at again after this long. A change from outside shows within it; this process's own writes show at once (`refresh_stamp`).
    const RECHECK: std::time::Duration = std::time::Duration::from_millis(200);

    fn stamp(path: &Path) -> Stamp {
        let meta = std::fs::metadata(path).ok()?;
        Some((meta.modified().ok()?, meta.len()))
    }

    fn path() -> Option<PathBuf> {
        let env = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        if let Some(dir) = env("IBIS_SETTINGS_DIR") {
            return Some(dir.join("settings.txt"));
        }
        // The scenarios keep the settings apart from the user's own.
        if let Some(dir) = crate::app_data::scenario_dir() {
            return Some(dir.join("settings.txt"));
        }
        #[cfg(target_os = "macos")]
        let base = env("HOME").map(|home| home.join("Library/Application Support"));
        #[cfg(target_os = "windows")]
        let base = env("APPDATA");
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let base = env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|home| home.join(".config")));
        Some(base?.join("ibisOffice").join("settings.txt"))
    }

    fn read_disk(path: &Path) -> Vec<(String, String)> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect()
    }

    fn with_entries<R>(path: &Path, force: bool, f: impl FnOnce(&mut Vec<(String, String)>) -> R) -> R {
        let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        let fresh = !force && guard.as_ref().is_some_and(|(cached_path, _, checked, _)| cached_path == path && checked.elapsed() < RECHECK);
        if !fresh {
            let now = stamp(path);
            if guard.as_ref().is_none_or(|(cached_path, seen, _, _)| cached_path != path || *seen != now) {
                let entries = read_disk(path);
                *guard = Some((path.to_path_buf(), now, crate::clock::Instant::now(), entries));
            } else if let Some((_, _, checked, _)) = guard.as_mut() {
                *checked = crate::clock::Instant::now();
            }
        }
        let (_, _, _, entries) = guard.as_mut().expect("settings cache initialized");
        f(entries)
    }

    /// After this process wrote the file: its new stamp is the one the cache matches (so its own write is not read back).
    fn refresh_stamp(path: &Path) {
        let mut guard = CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((cached_path, seen, checked, _)) = guard.as_mut()
            && cached_path == path
        {
            *seen = stamp(path);
            *checked = crate::clock::Instant::now();
        }
    }

    pub(super) fn get(key: &str) -> Option<String> {
        let path = path()?;
        with_entries(&path, false, |entries| {
            entries.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
        })
    }

    pub(super) fn set(key: &str, value: &str) {
        let Some(path) = path() else { return };
        let text: String = with_entries(&path, true, |all| {
            all.retain(|(k, _)| k != key);
            all.push((key.to_string(), value.to_string()));
            all.iter().map(|(k, v)| format!("{k}={v}\n")).collect()
        });
        let result = match path.parent() {
            Some(dir) => std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&path, text)),
            None => std::fs::write(&path, text),
        };
        if let Err(err) = result {
            log::warn!("{}: {err}", path.display());
        }
        refresh_stamp(&path);
    }

    /// The line of `key` taken out of the file (nothing is written when it is not there).
    pub(super) fn remove(key: &str) {
        let Some(path) = path() else { return };
        let text: Option<String> = with_entries(&path, true, |all| {
            let before = all.len();
            all.retain(|(k, _)| k != key);
            if all.len() == before {
                return None;
            }
            Some(all.iter().map(|(k, v)| format!("{k}={v}\n")).collect())
        });
        let Some(text) = text else { return };
        if let Err(err) = std::fs::write(&path, text) {
            log::warn!("{}: {err}", path.display());
        }
        refresh_stamp(&path);
    }
}

#[cfg(target_family = "wasm")]
mod store {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use wasm_bindgen::JsValue;

    thread_local! {
        static CACHE: RefCell<HashMap<String, Option<String>>> = RefCell::new(HashMap::new());
    }

    fn storage() -> Option<JsValue> {
        js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("localStorage")).ok() // host-lint-allow: not used when a host embeds the editor (`use_memory_only`).filter(|s| !s.is_undefined() && !s.is_null())
    }

    fn call(name: &str, args: &[&str]) -> Option<JsValue> {
        let storage = storage()?;
        let f: js_sys::Function = js_sys::Reflect::get(&storage, &JsValue::from_str(name)).ok()?.into();
        let args: Vec<JsValue> = args.iter().map(|a| JsValue::from_str(a)).collect();
        match args.len() {
            1 => f.call1(&storage, &args[0]).ok(),
            _ => f.call2(&storage, &args[0], &args[1]).ok(),
        }
    }

    pub(super) fn get(key: &str) -> Option<String> {
        if let Some(cached) = CACHE.with(|c| c.borrow().get(key).cloned()) {
            return cached;
        }
        let value = call("getItem", &[&format!("ibisOffice.{key}")]).and_then(|v| v.as_string());
        CACHE.with(|c| {
            c.borrow_mut().insert(key.to_string(), value.clone());
        });
        value
    }

    pub(super) fn set(key: &str, value: &str) {
        call("setItem", &[&format!("ibisOffice.{key}"), value]);
        CACHE.with(|c| {
            c.borrow_mut().insert(key.to_string(), Some(value.to_string()));
        });
    }

    pub(super) fn remove(key: &str) {
        call("removeItem", &[&format!("ibisOffice.{key}")]);
        CACHE.with(|c| {
            c.borrow_mut().insert(key.to_string(), None);
        });
    }
}

/// The settings are kept in memory only (nothing is read from or written to the browser's `localStorage` or the settings file). The
/// browser version does this when a host page embeds it: the storage of the page is shared with the host, which the editor does not touch.
static MEMORY_ONLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static MEMORY: std::sync::Mutex<Vec<(String, String)>> = std::sync::Mutex::new(Vec::new());

/// Keep the settings in memory from now on (until the page is closed); those already set are forgotten.
pub fn use_memory_only() {
    MEMORY_ONLY.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// The settings that are kept in the browser's `localStorage` even when a host embeds the editor (K0037): the size of the host's file window and the place of its splitter. They
/// are the editor's own (`ibisOffice.` before the name, so they do not touch the host's), and a host that opens the editor again in a new iframe each time (ibisWorks) would
/// otherwise lose them each time.
const KEPT_WITH_A_HOST: &[&str] = &["dialog.host-file.width", "dialog.host-file.height", "dialog.host-file.split"];

fn kept_with_a_host(key: &str) -> bool {
    KEPT_WITH_A_HOST.contains(&key)
}

fn memory() -> Option<std::sync::MutexGuard<'static, Vec<(String, String)>>> {
    MEMORY_ONLY.load(std::sync::atomic::Ordering::Relaxed).then(|| MEMORY.lock().unwrap_or_else(|e| e.into_inner()))
}

/// The keys of the settings that were renamed to follow the naming rule (`{scope}.{area}.{name}`, each part in kebab-case; K0038):
/// `(new key, old key)`. The old key is still read: the first read of the new key moves a value stored under the old one to the new
/// key, so that nobody loses a setting by updating (`migrate_legacy_keys` does the same for all of them at startup).
pub const LEGACY_KEYS: &[(&str, &str)] = &[
    ("docx.default.paragraph-spacing", "docx.default.paragraph_spacing"),
    ("docx.default.style-set", "docx.default.style_set"),
    ("docx.default.theme-colors", "docx.default.theme_colors"),
    ("docx.default.theme-effects", "docx.default.theme_effects"),
    ("docx.default.theme-fonts", "docx.default.theme_fonts"),
    ("docx.building-blocks.library", "docx.building_blocks.library"),
    ("xlsx.formula-bar.font", "sheet.formula-bar-font"),
    ("app.check-updates", "check-updates"),
    ("app.update-attempt", "update-attempt"),
    ("app.language", "language"),
    ("app.quick-access-toolbar", "qat"),
    ("app.linux-desktop", "linux-desktop"),
];

/// The old name of the setting `key`, if it was renamed.
pub fn legacy_name(key: &str) -> Option<&'static str> {
    LEGACY_KEYS.iter().find(|(new, _)| *new == key).map(|(_, old)| *old)
}

/// Move the values stored under the old names of the renamed settings to the new names (a value already under the new name stays).
pub fn migrate_legacy_keys() {
    if memory().is_some() {
        return;
    }
    for (new, old) in LEGACY_KEYS {
        if let Some(value) = store::get(old) {
            if store::get(new).is_none() {
                store::set(new, &value);
            }
            store::remove(old);
        }
    }
}

/// The setting `key`, if it is set.
pub fn get(key: &str) -> Option<String> {
    if let Some(memory) = memory() {
        if let Some(value) = memory.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()) {
            return Some(value);
        }
        return if kept_with_a_host(key) { store::get(key) } else { None };
    }
    if let Some(value) = store::get(key) {
        return Some(value);
    }
    let old = legacy_name(key)?;
    let value = store::get(old)?;
    store::set(key, &value);
    store::remove(old);
    Some(value)
}

/// Set `key` to `value` (kept).
pub fn set(key: &str, value: &str) {
    if let Some(mut memory) = memory() {
        memory.retain(|(k, _)| k != key);
        memory.push((key.to_string(), value.to_string()));
        if kept_with_a_host(key) {
            store::set(key, value);
        }
        return;
    }
    if let Some(old) = legacy_name(key) {
        store::remove(old);
    }
    store::set(key, value)
}

/// Take the setting `key` away (it is as if it had never been set; modified by ibis inc.: added for the formula bar font's default).
pub fn remove(key: &str) {
    if let Some(mut memory) = memory() {
        memory.retain(|(k, _)| k != key);
        return;
    }
    if let Some(old) = legacy_name(key) {
        store::remove(old);
    }
    store::remove(key)
}

#[cfg(test)]
mod legacy_key_tests {
    use super::{legacy_name, LEGACY_KEYS};

    /// K0038: every renamed setting follows the rule (kebab-case parts, a scope in front), the old name is a different one, and no name is
    /// listed twice.
    #[test]
    fn the_new_names_follow_the_naming_rule_and_the_old_ones_are_found() {
        for (new, old) in LEGACY_KEYS {
            assert_ne!(new, old);
            assert!(new.contains('.'), "{new} has a scope");
            assert!(!new.contains('_') && new.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-'), "{new}");
            assert_eq!(legacy_name(new), Some(*old));
            assert_eq!(LEGACY_KEYS.iter().filter(|(n, o)| n == new || o == old).count(), 1);
        }
        assert_eq!(legacy_name("docx.unknown"), None);
    }

    #[cfg(not(target_family = "wasm"))]
    #[test]
    fn settings_cache_tracks_set_remove_and_dir_switch() {
        let base = std::env::temp_dir().join(format!("ibis-settings-cache-test-{}", std::process::id()));
        let dir_a = base.join("a");
        let dir_b = base.join("b");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::create_dir_all(&dir_b).unwrap();

        unsafe { std::env::set_var("IBIS_SETTINGS_DIR", &dir_a) };
        assert_eq!(super::get("test.cache-key"), None);
        super::set("test.cache-key", "alpha");
        assert_eq!(super::get("test.cache-key").as_deref(), Some("alpha"));

        // Switching IBIS_SETTINGS_DIR must invalidate the cached path immediately.
        unsafe { std::env::set_var("IBIS_SETTINGS_DIR", &dir_b) };
        assert_eq!(super::get("test.cache-key"), None);
        super::set("test.cache-key", "beta");
        assert_eq!(super::get("test.cache-key").as_deref(), Some("beta"));
        super::remove("test.cache-key");
        assert_eq!(super::get("test.cache-key"), None);

        // Switching back to dir_a sees the persisted value.
        unsafe { std::env::set_var("IBIS_SETTINGS_DIR", &dir_a) };
        assert_eq!(super::get("test.cache-key").as_deref(), Some("alpha"));

        // A change made from outside (another window or process, a text editor) is read again, with and without a size change.
        let file = dir_a.join("settings.txt");
        let outside = |text: &str| {
            std::fs::write(&file, text).unwrap();
            // `get` looks at the file again after the recheck time.
            std::thread::sleep(std::time::Duration::from_millis(250));
        };
        outside("test.cache-key=gamma\ntest.other=1\n");
        assert_eq!(super::get("test.cache-key").as_deref(), Some("gamma"));
        assert_eq!(super::get("test.other").as_deref(), Some("1"));
        outside("test.cache-key=delta\ntest.other=1\n");
        assert_eq!(super::get("test.cache-key").as_deref(), Some("delta"));
        // Deleted from outside.
        std::fs::remove_file(&file).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert_eq!(super::get("test.cache-key"), None);
        // Written from outside and at once by this process (`set` looks at the file first): both are kept.
        std::fs::write(&file, "test.outside=1\n").unwrap();
        super::set("test.inside", "2");
        assert_eq!(super::get("test.outside").as_deref(), Some("1"));
        assert_eq!(super::get("test.inside").as_deref(), Some("2"));
        assert!(std::fs::read_to_string(&file).unwrap().contains("test.inside=2"));
        unsafe { std::env::remove_var("IBIS_SETTINGS_DIR") };
        let _ = std::fs::remove_dir_all(&base);
    }
}
