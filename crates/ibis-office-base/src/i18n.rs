// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The UI's language: Japanese or English (#8). The words are kept in
//! tables, one per language, in `crates/ibis-office-base/i18n/` (`ja.tsv`,
//! `en.tsv`: a key, a tab and the text, per line), not in the source: the
//! code asks for a key (a menu item's id, such as `file.save`) and gets the
//! text of the language shown, the Japanese one if the language's table has
//! none, the key itself if neither has.
//!
//! The language is the user's choice (ツール > 設定… > 一般 > 言語), saved with
//! the app's settings (`settings.rs`), or by default the system's: Japanese
//! if the user's first preferred language is Japanese, else English.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use gpui_kit::SharedString;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    Japanese,
    English,
}

/// What the user chose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LanguageSetting {
    /// The system's language.
    System,
    Fixed(Language),
}

impl LanguageSetting {
    /// As saved in the settings.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Fixed(Language::Japanese) => "ja",
            Self::Fixed(Language::English) => "en",
        }
    }

    pub fn parse(text: &str) -> Self {
        match text.trim() {
            "ja" => Self::Fixed(Language::Japanese),
            "en" => Self::Fixed(Language::English),
            _ => Self::System,
        }
    }
}

/// The tables, one file per owner (`i18n/<language>/<owner>.tsv`: the
/// shell's and each format's, the fix担当's), each owner adding to its own
/// so that additions do not meet. Every file of the folders is listed here
/// (a test checks it), and a key is in one file only (checked too).
#[cfg(test)]
const FILES: [&str; 6] = ["shell", "docx", "xlsx", "pptx", "fix", "menu_planned"];
const JA: [&str; 6] = [
    include_str!("../i18n/ja/shell.tsv"),
    include_str!("../i18n/ja/docx.tsv"),
    include_str!("../i18n/ja/xlsx.tsv"),
    include_str!("../i18n/ja/pptx.tsv"),
    include_str!("../i18n/ja/fix.tsv"),
    include_str!("../i18n/ja/menu_planned.tsv"),
];
const EN: [&str; 6] = [
    include_str!("../i18n/en/shell.tsv"),
    include_str!("../i18n/en/docx.tsv"),
    include_str!("../i18n/en/xlsx.tsv"),
    include_str!("../i18n/en/pptx.tsv"),
    include_str!("../i18n/en/fix.tsv"),
    include_str!("../i18n/en/menu_planned.tsv"),
];

fn rows(text: &'static str) -> impl Iterator<Item = (&'static str, &'static str)> {
    text.lines().filter(|line| !line.is_empty() && !line.starts_with('#')).filter_map(|line| line.split_once('\t'))
}

fn table(files: &[&'static str]) -> HashMap<&'static str, &'static str> {
    files.iter().flat_map(|text| rows(text)).collect()
}

fn tables() -> &'static (HashMap<&'static str, &'static str>, HashMap<&'static str, &'static str>) {
    static TABLES: OnceLock<(HashMap<&'static str, &'static str>, HashMap<&'static str, &'static str>)> = OnceLock::new();
    TABLES.get_or_init(|| (table(&JA), table(&EN)))
}

/// 0: not set yet (the setting is read at the first use), 1 Japanese, 2 English.
static CURRENT: AtomicU8 = AtomicU8::new(0);

/// The language shown.
pub fn language() -> Language {
    match CURRENT.load(Ordering::Relaxed) {
        1 => Language::Japanese,
        2 => Language::English,
        _ => {
            let language = resolve(crate::settings::get("language").map_or(LanguageSetting::System, |s| LanguageSetting::parse(&s)));
            set_language(language);
            language
        }
    }
}

fn set_language(language: Language) {
    CURRENT.store(if language == Language::Japanese { 1 } else { 2 }, Ordering::Relaxed);
    // The names saved in documents follow the UI too (ibis-pptx has no tables).
    ibis_pptx::set_english_names(language == Language::English);
}

/// The language a setting shows.
pub fn resolve(setting: LanguageSetting) -> Language {
    match setting {
        LanguageSetting::Fixed(language) => language,
        LanguageSetting::System => system_language(),
    }
}

/// The saved setting.
pub fn setting() -> LanguageSetting {
    crate::settings::get("language").map_or(LanguageSetting::System, |s| LanguageSetting::parse(&s))
}

/// Choose the language (saved; the windows are to be drawn again).
pub fn set_setting(setting: LanguageSetting) {
    crate::settings::set("language", setting.as_str());
    set_language(resolve(setting));
}

/// Forget the language shown: the next use reads the setting again (as a
/// new start of the app does; the scenario tests).
#[allow(dead_code)]
pub fn forget() {
    CURRENT.store(0, Ordering::Relaxed);
}

/// Show `language` without saving it (the scenario tests).
#[allow(dead_code)]
pub fn show(language: Language) {
    set_language(language);
}

/// The text of `key` in the language shown.
pub fn text(key: &str) -> SharedString {
    let (ja, en) = tables();
    let found = match language() {
        Language::English => en.get(key).or_else(|| ja.get(key)),
        Language::Japanese => ja.get(key),
    };
    #[cfg(feature = "scenarios")]
    log::record(language() == Language::English, key, found.copied());
    match found {
        Some(text) => match localize_keys(text, cfg!(target_os = "macos")) {
            std::borrow::Cow::Borrowed(_) => SharedString::new_static(text),
            std::borrow::Cow::Owned(own) => SharedString::from(own),
        },
        None => SharedString::from(key.to_string()),
    }
}

const MODIFIER_KEYS: [char; 4] = ['\u{2318}', '\u{2303}', '\u{2325}', '\u{21e7}'];

/// A key of a shortcut for the system it is shown on (GitHub issue #150). The tables write the keys as the Mac does (the command key,
/// control, option and shift marks, then the key), and every text of the tables goes through here: on a Mac they stay; elsewhere a run
/// of marks and the key after it become `Ctrl+Alt+Shift+Key` (the command and control keys are both Ctrl, option is Alt), so
/// 太字（⌘B）is 太字（Ctrl+B）, ⇧⌘J is Ctrl+Shift+J and ⌘↩ is Ctrl+Enter. Text with no marks is returned as it is.
pub fn localize_keys(text: &str, mac: bool) -> std::borrow::Cow<'_, str> {
    if mac || !text.contains(MODIFIER_KEYS) {
        return std::borrow::Cow::Borrowed(text);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 8);
    let mut i = 0;
    while i < chars.len() {
        if !MODIFIER_KEYS.contains(&chars[i]) {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let (mut ctrl, mut alt, mut shift) = (false, false, false);
        while i < chars.len() && MODIFIER_KEYS.contains(&chars[i]) {
            match chars[i] {
                '\u{2318}' | '\u{2303}' => ctrl = true,
                '\u{2325}' => alt = true,
                _ => shift = true,
            }
            i += 1;
        }
        let mut key = String::new();
        match chars.get(i) {
            Some('\u{232b}') => key.push_str("Backspace"),
            Some('\u{2326}') => key.push_str("Del"),
            Some('\u{21a9}') => key.push_str("Enter"),
            Some('\u{238b}') => key.push_str("Esc"),
            Some('\u{21e5}') => key.push_str("Tab"),
            Some(c) if c.is_ascii_alphanumeric() => {
                while let Some(c) = chars.get(i).filter(|c| c.is_ascii_alphanumeric()) {
                    key.push(*c);
                    i += 1;
                }
                i -= 1;
            }
            Some(c) if c.is_ascii_punctuation() => key.push(*c),
            _ => {
                // Marks with no key after them: shown as the words.
                i -= 1;
            }
        }
        i += 1;
        if ctrl {
            out.push_str("Ctrl+");
        }
        if alt {
            out.push_str("Alt+");
        }
        if shift {
            out.push_str("Shift+");
        }
        out.push_str(&key);
    }
    std::borrow::Cow::Owned(out)
}

/// What the scenarios asked the tables for: every (language, key, text or
/// none) once, for the check that no id is asked for that the tables lack
/// (an id shown as it is on the screen) and, by `IBIS_I18N_LOG`, for
/// comparing the texts before and after the keys were changed.
#[cfg(feature = "scenarios")]
pub mod log {
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    static LOG: Mutex<BTreeSet<(bool, String, Option<String>)>> = Mutex::new(BTreeSet::new());

    pub(super) fn record(english: bool, key: &str, found: Option<&str>) {
        if let Ok(mut log) = LOG.lock() {
            log.insert((english, key.to_string(), found.map(str::to_string)));
        }
    }

    /// Whether `key` looks like an id (`pptx.text.wrap`, or a provisional
    /// one made from the English text, `_xlsx.insert-row-above`), not a text.
    pub(crate) fn is_id(key: &str) -> bool {
        let key = key.strip_prefix('_').unwrap_or(key);
        key.contains('.')
            && key.starts_with(|c: char| c.is_ascii_lowercase())
            && key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
    }

    /// The ids asked for that the tables lack.
    pub fn missing_ids() -> Vec<String> {
        LOG.lock().map(|log| log.iter().filter(|(_, key, found)| found.is_none() && is_id(key)).map(|(_, key, _)| key.clone()).collect()).unwrap_or_default()
    }

    /// The log, one line per entry: `ja|en`, key, hit or miss, text.
    pub fn lines() -> Vec<String> {
        LOG.lock()
            .map(|log| log.iter().map(|(en, key, found)| format!("{}\t{key}\t{}\t{}", if *en { "en" } else { "ja" }, if found.is_some() { "hit" } else { "miss" }, found.as_deref().unwrap_or(key))).collect())
            .unwrap_or_default()
    }
}

/// The fonts whose names the lists show in English in the English UI (the
/// name of a font in a document is never changed: only what the lists show).
const FONT_IDS: [(&str, &str); 6] = [
    ("游明朝", "fix.font-name.yu-mincho"), // i18n-ok（文書の中のフォント名（照合）。FONT_IDS。表示の訳は font_label）
    ("游ゴシック", "fix.font-name.yu-gothic"), // i18n-ok（文書の中のフォント名（照合）。FONT_IDS）
    ("ＭＳ 明朝", "fix.font-name.ms-mincho"), // i18n-ok（文書の中のフォント名（照合）。FONT_IDS）
    ("ＭＳ ゴシック", "fix.font-name.ms-gothic"), // i18n-ok（文書の中のフォント名（照合）。FONT_IDS）
    ("BIZ UDP明朝", "fix.font-name.biz-udp-mincho"), // i18n-ok（文書の中のフォント名（照合）。FONT_IDS）
    ("BIZ UDPゴシック", "fix.font-name.biz-udp-gothic"), // i18n-ok（文書の中のフォント名（照合）。FONT_IDS）
];

/// How the window finds the English name of a font saved under `name` (`shell::font_list::english_name`, which enumerates the
/// system's fonts and so is not here); set once by the app's `init`. Unset, no font has an English name.
static ENGLISH_FONT_NAME: OnceLock<fn(&str) -> Option<&'static str>> = OnceLock::new();

/// Tells `font_label` where the English names of the system's fonts are (see `ENGLISH_FONT_NAME`).
pub fn set_english_font_name(find: fn(&str) -> Option<&'static str>) {
    let _ = ENGLISH_FONT_NAME.set(find);
}

fn english_font_name(name: &str) -> Option<&'static str> {
    ENGLISH_FONT_NAME.get().and_then(|find| find(name))
}

/// A font's name as a list shows it: English in the English UI for the
/// fonts that have an English name here, else as it is.
pub fn font_label(name: &str) -> SharedString {
    match FONT_IDS.iter().find(|(n, _)| *n == name) {
        Some((_, id)) => text(id),
        // A font of the system's list under a name in the system's language (macOS): its English name in the English UI.
        None => match english_font_name(name) {
            Some(english) if language() == Language::English => SharedString::from(english.to_string()),
            _ => SharedString::from(name.to_string()),
        },
    }
}

/// Whether `label` is the name of a font `font_label` knows.
pub fn is_font_name(label: &str) -> bool {
    FONT_IDS.iter().any(|(n, _)| *n == label)
}

/// The text of `key` with its `{name}`s filled in from `args` (a message
/// with numbers or names in it: the tables give each language its own
/// order of words).
pub fn format(key: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut out = text(key).to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

/// Whether `text` is an id of the tables (a key of the id's shape that either
/// language has), not a text.
#[allow(dead_code)]
pub fn is_known_id(text: &str) -> bool {
    let (ja, en) = tables();
    let bare = text.strip_prefix('_').unwrap_or(text);
    bare.contains('.') && bare.starts_with(|c: char| c.is_ascii_lowercase()) && !text.contains(char::is_whitespace) && (ja.contains_key(text) || en.contains_key(text))
}

/// The text of `key` in the language shown, if the tables have it (nothing is logged for a miss: the access letters are looked up for
/// every menu line, most of which have none).
pub fn lookup(key: &str) -> Option<&'static str> {
    let (ja, en) = tables();
    match language() {
        Language::English => en.get(key).or_else(|| ja.get(key)),
        Language::Japanese => ja.get(key),
    }
    .copied()
}

/// Whether the tables have `key` (the code's own labels are used for
/// those they have not).
#[allow(dead_code)]
pub fn has(key: &str) -> bool {
    tables().0.contains_key(key)
}

/// The system's language: Japanese if the user's first preferred language
/// is, else English.
pub fn system_language() -> Language {
    match preferred_languages().first() {
        Some(tag) if tag.to_ascii_lowercase().starts_with("ja") => Language::Japanese,
        _ => Language::English,
    }
}

/// The user's preferred languages, most preferred first (BCP 47 tags).
fn preferred_languages() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        macos::preferred_languages()
    }
    #[cfg(target_os = "windows")]
    {
        windows::preferred_languages()
    }
    #[cfg(target_family = "wasm")]
    {
        web::preferred_languages()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_family = "wasm")))]
    {
        ["LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|v| std::env::var(v).ok()).filter(|v| !v.is_empty()).take(1).collect()
    }
}

/// macOS: NSLocale's preferredLanguages (CFLocaleCopyPreferredLanguages).
#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{c_char, c_void};

    type CFIndex = isize;
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFLocaleCopyPreferredLanguages() -> *const c_void;
        fn CFArrayGetCount(array: *const c_void) -> CFIndex;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: CFIndex) -> *const c_void;
        fn CFStringGetCString(string: *const c_void, buffer: *mut c_char, size: CFIndex, encoding: u32) -> bool;
        fn CFRelease(object: *const c_void);
    }
    const UTF8: u32 = 0x0800_0100;

    pub(super) fn preferred_languages() -> Vec<String> {
        let mut out = Vec::new();
        // SAFETY: the array is a CFArray of CFStrings, owned here (released
        // at the end); its values are read while it is alive.
        unsafe {
            let array = CFLocaleCopyPreferredLanguages();
            if array.is_null() {
                return out;
            }
            for i in 0..CFArrayGetCount(array) {
                let string = CFArrayGetValueAtIndex(array, i);
                let mut buffer = [0 as c_char; 64];
                if CFStringGetCString(string, buffer.as_mut_ptr(), buffer.len() as CFIndex, UTF8) {
                    out.push(std::ffi::CStr::from_ptr(buffer.as_ptr()).to_string_lossy().into_owned());
                }
            }
            CFRelease(array);
        }
        out
    }
}

/// Windows: GetUserPreferredUILanguages.
#[cfg(target_os = "windows")]
mod windows {
    const MUI_LANGUAGE_NAME: u32 = 0x8;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserPreferredUILanguages(flags: u32, count: *mut u32, buffer: *mut u16, size: *mut u32) -> i32;
    }

    pub(super) fn preferred_languages() -> Vec<String> {
        let (mut count, mut size) = (0u32, 0u32);
        // SAFETY: the first call asks for the size; the second fills a
        // buffer of that size (a list of NUL-terminated names, NUL at the end).
        unsafe {
            if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, std::ptr::null_mut(), &mut size) == 0 || size == 0 {
                return Vec::new();
            }
            let mut buffer = vec![0u16; size as usize];
            if GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut count, buffer.as_mut_ptr(), &mut size) == 0 {
                return Vec::new();
            }
            buffer.split(|&c| c == 0).filter(|s| !s.is_empty()).map(String::from_utf16_lossy).collect()
        }
    }
}

/// The browser: navigator.languages.
#[cfg(target_family = "wasm")]
mod web {
    use wasm_bindgen::JsValue;

    pub(super) fn preferred_languages() -> Vec<String> {
        let navigator = js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("navigator")).ok();
        let Some(languages) = navigator.and_then(|n| js_sys::Reflect::get(&n, &JsValue::from_str("languages")).ok()) else { return Vec::new() };
        js_sys::Array::from(&languages).iter().filter_map(|v| v.as_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{EN, FILES, JA, localize_keys, rows};

    /// GitHub issue #150: the Mac's marks become Ctrl, Alt and Shift off the Mac, in a shortcut alone and in a sentence.
    #[test]
    fn keys_are_shown_for_the_system() {
        let (cmd, shift, opt, ctrl, enter) = ('\u{2318}', '\u{21e7}', '\u{2325}', '\u{2303}', '\u{21a9}');
        assert_eq!(localize_keys(&format!("Bold ({cmd}B)"), false), "Bold (Ctrl+B)");
        assert_eq!(localize_keys(&format!("{shift}{cmd}J"), false), "Ctrl+Shift+J");
        assert_eq!(localize_keys(&format!("{opt}{cmd}M"), false), "Ctrl+Alt+M");
        assert_eq!(localize_keys(&format!("{ctrl}{cmd}F"), false), "Ctrl+F");
        assert_eq!(localize_keys(&format!("{cmd}{enter}"), false), "Ctrl+Enter");
        assert_eq!(localize_keys(&format!("{cmd}Enter to post"), false), "Ctrl+Enter to post");
        assert_eq!(localize_keys(&format!("{cmd}\\ and {cmd},"), false), "Ctrl+\\ and Ctrl+,");
        // The Mac keeps its marks, and text with none is as it is.
        assert_eq!(localize_keys(&format!("{cmd}B"), true), format!("{cmd}B"));
        assert_eq!(localize_keys("Bold", false), "Bold");
    }

    /// Every id the source asks the tables for (`i18n::text("pptx.text.wrap")`,
    /// `i18n::format(…)`, `MenuItem::tr(…)`, `text(…)`, `tr(…)` with a string
    /// of an id's shape) has a row in both languages: an id without one is
    /// shown as it is on the screen (the scenarios check what they reach).
    #[test]
    fn the_ids_the_source_asks_for_are_in_both_tables() {
        fn files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    files(&path, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    out.push(path);
                }
            }
        }
        let is_id = |s: &str| {
            s.contains('.') && s.starts_with(|c: char| c.is_ascii_lowercase()) && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
        };
        let ja: std::collections::HashSet<&str> = JA.iter().flat_map(|t| rows(t)).map(|(k, _)| k).collect();
        let en: std::collections::HashSet<&str> = EN.iter().flat_map(|t| rows(t)).map(|(k, _)| k).collect();
        let mut sources = Vec::new();
        // The source of every crate (the words are asked for by the window, the formats and the drawing alike).
        for krate in std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..")).unwrap().flatten() {
            if krate.path().join("src").is_dir() {
                files(&krate.path().join("src"), &mut sources);
            }
        }
        let mut missing = Vec::new();
        for path in sources {
            let text = std::fs::read_to_string(&path).unwrap();
            for call in ["i18n::text(\"", "i18n::format(\"", "::tr(\"", " text(\"", "(text(\"", ".tr(\""] {
                for (at, _) in text.match_indices(call) {
                    let rest = &text[at + call.len()..];
                    let Some(end) = rest.find('"') else { continue };
                    let id = &rest[..end];
                    if is_id(id) && !(ja.contains(id) && en.contains(id)) {
                        missing.push(format!("{}: {id} (ja {}, en {})", path.file_name().unwrap().to_string_lossy(), ja.contains(id), en.contains(id)));
                    }
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "ids asked for but not in both tables:\n{}", missing.join("\n"));
    }

    /// Each folder holds the files listed, and no key is in two of them.
    #[test]
    fn tables_are_split_by_owner_without_repeats() {
        for (language, files) in [("ja", &JA), ("en", &EN)] {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("i18n").join(language);
            let mut found: Vec<String> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()?.file_name().to_str()?.strip_suffix(".tsv").map(str::to_string)).collect();
            found.sort();
            let mut listed: Vec<String> = FILES.iter().map(|f| f.to_string()).collect();
            listed.sort();
            assert_eq!(found, listed, "the files of i18n/{language} and FILES in i18n.rs");
            let mut seen: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
            for (name, text) in FILES.iter().zip(files.iter()) {
                for (key, _) in rows(text) {
                    if let Some(other) = seen.insert(key, name) {
                        panic!("{language}: the key {key:?} is in both {other}.tsv and {name}.tsv");
                    }
                }
            }
        }
    }
}
