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
    use std::path::PathBuf;

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

    fn read() -> Vec<(String, String)> {
        let Some(path) = path() else { return Vec::new() };
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            .collect()
    }

    pub(super) fn get(key: &str) -> Option<String> {
        read().into_iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub(super) fn set(key: &str, value: &str) {
        let Some(path) = path() else { return };
        let mut all = read();
        all.retain(|(k, _)| k != key);
        all.push((key.to_string(), value.to_string()));
        let text: String = all.iter().map(|(k, v)| format!("{k}={v}\n")).collect();
        let result = match path.parent() {
            Some(dir) => std::fs::create_dir_all(dir).and_then(|()| std::fs::write(&path, text)),
            None => std::fs::write(&path, text),
        };
        if let Err(err) = result {
            log::warn!("{}: {err}", path.display());
        }
    }

    /// The line of `key` taken out of the file (nothing is written when it is not there).
    pub(super) fn remove(key: &str) {
        let Some(path) = path() else { return };
        let all = read();
        if !all.iter().any(|(k, _)| k == key) {
            return;
        }
        let text: String = all.iter().filter(|(k, _)| k != key).map(|(k, v)| format!("{k}={v}
")).collect();
        if let Err(err) = std::fs::write(&path, text) {
            log::warn!("{}: {err}", path.display());
        }
    }
}

#[cfg(target_family = "wasm")]
mod store {
    use wasm_bindgen::JsValue;

    fn storage() -> Option<JsValue> {
        js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("localStorage")).ok().filter(|s| !s.is_undefined() && !s.is_null())
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
        call("getItem", &[&format!("ibisOffice.{key}")])?.as_string()
    }

    pub(super) fn set(key: &str, value: &str) {
        call("setItem", &[&format!("ibisOffice.{key}"), value]);
    }

    pub(super) fn remove(key: &str) {
        call("removeItem", &[&format!("ibisOffice.{key}")]);
    }
}

/// The setting `key`, if it is set.
pub fn get(key: &str) -> Option<String> {
    store::get(key)
}

/// Set `key` to `value` (kept).
pub fn set(key: &str, value: &str) {
    store::set(key, value)
}

/// Take the setting `key` away (it is as if it had never been set; modified by ibis inc.: added for the formula bar font's default).
pub fn remove(key: &str) {
    store::remove(key)
}
