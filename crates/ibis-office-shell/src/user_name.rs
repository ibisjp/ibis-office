// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The name comments are written under: the user's full name as the system
//! knows it (macOS: `id -F`), else the account name. (There is no setting
//! for it yet.)

use std::sync::{OnceLock, RwLock};

static CUSTOM_NAME: RwLock<Option<String>> = RwLock::new(None);
static CUSTOM_INITIALS: RwLock<Option<String>> = RwLock::new(None);

/// The custom name and initials forgotten (the scenarios start each from the defaults).
pub(crate) fn reset() {
    if let Ok(mut g) = CUSTOM_NAME.write() {
        *g = None;
    }
    if let Ok(mut g) = CUSTOM_INITIALS.write() {
        *g = None;
    }
}

/// The default user name discovered from system or environment.
pub fn default_user_name() -> &'static str {
    static NAME: OnceLock<String> = OnceLock::new();
    NAME.get_or_init(|| {
        #[cfg(target_os = "macos")]
        if let Ok(out) = std::process::Command::new("/usr/bin/id").arg("-F").output()
            && out.status.success()
            && let Ok(name) = String::from_utf8(out.stdout)
            && !name.trim().is_empty()
        {
            return name.trim().to_string();
        }
        ["USER", "USERNAME"].iter().find_map(|v| std::env::var(v).ok().filter(|u| !u.is_empty())).unwrap_or_else(|| "ibisOffice".into())
    })
}

/// The user's name: custom override if set, else system default.
pub fn user_name() -> String {
    if let Ok(guard) = CUSTOM_NAME.read() {
        if let Some(ref name) = *guard {
            if !name.trim().is_empty() {
                return name.clone();
            }
        }
    }
    default_user_name().to_string()
}

/// The user's initials: custom override if set, else initials derived from user_name.
pub fn user_initials() -> String {
    if let Ok(guard) = CUSTOM_INITIALS.read() {
        if let Some(ref inits) = *guard {
            if !inits.trim().is_empty() {
                return inits.clone();
            }
        }
    }
    initials(&user_name())
}

/// Set or reset custom user name and initials override.
pub fn set_custom_user_info(name: Option<String>, initials: Option<String>) {
    if let Ok(mut guard) = CUSTOM_NAME.write() {
        *guard = name;
    }
    if let Ok(mut guard) = CUSTOM_INITIALS.write() {
        *guard = initials;
    }
}

/// Get current custom overrides if configured.
#[allow(dead_code)]
pub(crate) fn custom_user_info() -> (Option<String>, Option<String>) {
    let n = CUSTOM_NAME.read().ok().and_then(|g| g.clone());
    let i = CUSTOM_INITIALS.read().ok().and_then(|g| g.clone());
    (n, i)
}

/// Initials of `name`: the first letter of each of its words (up to two),
/// or its first character.
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    if words.len() >= 2 && words.iter().all(|w| w.chars().next().is_some_and(|c| c.is_ascii_alphabetic())) {
        return words.iter().take(2).filter_map(|w| w.chars().next()).flat_map(char::to_uppercase).collect();
    }
    name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::initials;

    #[test]
    fn initials_of_names() {
        assert_eq!(initials("Taro Test"), "TT");
        assert_eq!(initials("山田 花子"), "山");
        assert_eq!(initials("yamada"), "Y");
    }
}
