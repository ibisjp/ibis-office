// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The fonts the font lists (the toolbars' font boxes) offer: the families installed on this
//! system and the ones bundled with the app, dynamically enumerated and cached.
//!
//! * One row for a family (the styles of a family are not listed), by name, leaving out
//!   internal/hidden families (names starting with `.`, such as `.SystemUIFont`).
//! * The name a font is saved under in a document is the name the system lists it
//!   under, as Office saves it. On Windows that is the name in the system's language (DirectWrite);
//!   on macOS it is the family name in the system's language (CoreText), so the Hiragino fonts
//!   are saved by their Japanese names on a Japanese Mac, and by the English ones on an English Mac.
//!   The English UI shows the English family name.
//! * Japanese fonts are grouped at the top and sorted cleanly in phonetic/syllabary order,
//!   followed by Latin/Western fonts in alphabetical order (case-insensitive).
//! * Enumerated dynamically and cached once in `LIST` so subsequent accesses take 0 ms.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::chrome::{MenuEntry, MenuItem};
use crate::editor::SlideEditor;
use gpui_kit::{Context, Window};

/// A font choice offered in the font dropdown.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FontChoice {
    /// The name saved in a document (and shown in the Japanese UI).
    pub name: String,
    /// The family name in English (shown in the English UI).
    pub english: String,
}

struct FontList {
    choices: Vec<FontChoice>,
    /// Saved name -> English name, for the names that differ.
    english: HashMap<String, String>,
}

static LIST: OnceLock<FontList> = OnceLock::new();
/// The list made again after fonts were added while the app runs, with the generation it was made for (`fonts_generation`).
static LATE_LIST: std::sync::Mutex<Option<(u64, &'static FontList)>> = std::sync::Mutex::new(None);

/// The fonts to offer, sorted cleanly (Japanese fonts first in syllabary order, then Latin fonts alphabetically).
pub(crate) fn font_choices() -> &'static [FontChoice] {
    list().map_or(&[], |l| &l.choices)
}

/// The English name of the font saved as `name`, when it has a different one.
pub fn english_name(name: &str) -> Option<&'static str> {
    list()?.english.get(name).map(String::as_str)
}

/// The maximum number of recent fonts kept in settings.
const MAX_RECENT_FONTS: usize = 5;

/// The recent font names stored in settings.
pub(crate) fn recent_fonts() -> Vec<String> {
    let Some(raw) = crate::settings::get("font.recent") else {
        return Vec::new();
    };
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .take(MAX_RECENT_FONTS)
        .map(ToString::to_string)
        .collect()
}

/// Record a chosen font to the front of recent fonts, preserving at most 5 items.
pub(crate) fn record_recent_font(name: &str) {
    let name = name.trim();
    if name.is_empty() {
        return;
    }
    let mut current = recent_fonts();
    current.retain(|f| f != name);
    current.insert(0, name.to_string());
    current.truncate(MAX_RECENT_FONTS);
    let joined = current.join(",");
    crate::settings::set("font.recent", &joined);
}

/// Builds the complete font menu entries:
/// 1. Optional leading items (e.g. "Custom font...", "Replace fonts...")
/// 2. "Recent Fonts" section with up to 5 items
/// 3. Separator
/// 4. "System Fonts" section with all system fonts
pub fn build_font_menu_entries(
    item_id: &'static str,
    leading_items: Vec<MenuEntry>,
    current_font: Option<&str>,
    on_select: impl Fn(&str, &mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + Clone + 'static,
) -> Vec<MenuEntry> {
    let mut menu: Vec<MenuEntry> = Vec::new();
    push_font_menu_entries(&mut menu, item_id, leading_items, current_font, on_select);
    menu
}

/// The font menu grouped by where a font comes from: the section "used in this file" (`used`, in the given order),
/// then, when `include_all`, the section "all" (the recent fonts and every installed family, as in
/// [`build_font_menu_entries`]). With `include_all` false only the used fonts are offered (the "replace from" list of
/// the font replacement: Office lists only the fonts the file uses). Usable from any screen's font list.
pub fn build_grouped_font_menu_entries(
    item_id: &'static str,
    used: &[String],
    include_all: bool,
    current_font: Option<&str>,
    on_select: impl Fn(&str, &mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + Clone + 'static,
) -> Vec<MenuEntry> {
    let mut menu: Vec<MenuEntry> = Vec::new();
    if !used.is_empty() {
        menu.push(MenuItem::new("font.section.used", crate::i18n::text("_shell.font-used")).disabled(true).into());
        for name in used {
            let checked = current_font == Some(name.as_str()) || english_name(name).is_some_and(|en| current_font == Some(en));
            let (on_select, fn_name) = (on_select.clone(), name.clone());
            menu.push(
                MenuItem::new(item_id, crate::i18n::font_label(name))
                    .checked(checked)
                    .on(move |editor, window, cx| {
                        record_recent_font(&fn_name);
                        on_select(&fn_name, editor, window, cx);
                    })
                    .into(),
            );
        }
    }
    if include_all {
        if !menu.is_empty() {
            menu.push(MenuEntry::Separator);
        }
        let mut all = Vec::new();
        push_font_menu_entries(&mut all, item_id, Vec::new(), current_font, on_select);
        // The system section's header is "all" when the list is grouped.
        for entry in &mut all {
            if let MenuEntry::Item(item) = entry {
                if item.id == "font.section.system" {
                    item.label = crate::i18n::text("_shell.font-all").into();
                }
            }
        }
        menu.extend(all);
    }
    menu
}

fn push_font_menu_entries(
    menu: &mut Vec<MenuEntry>,
    item_id: &'static str,
    leading_items: Vec<MenuEntry>,
    current_font: Option<&str>,
    on_select: impl Fn(&str, &mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + Clone + 'static,
) {

    // 1. Leading items (if any, followed by a separator)
    if !leading_items.is_empty() {
        menu.extend(leading_items);
        menu.push(MenuEntry::Separator);
    }

    // 2. Recent fonts (up to 5)
    let recent = recent_fonts();
    if !recent.is_empty() {
        menu.push(
            MenuItem::new("font.section.recent", crate::i18n::text("_shell.font-recent"))
                .disabled(true)
                .into(),
        );
        for name in &recent {
            let font_name = name.clone();
            let label = crate::i18n::font_label(&font_name);
            let checked = current_font == Some(font_name.as_str())
                || english_name(&font_name).is_some_and(|en| current_font == Some(en));
            let on_select = on_select.clone();
            let fn_name = font_name.clone();
            menu.push(
                MenuItem::new("font.recent.item", label)
                    .checked(checked)
                    .on(move |editor, window, cx| {
                        record_recent_font(&fn_name);
                        on_select(&fn_name, editor, window, cx);
                    })
                    .into(),
            );
        }
        menu.push(MenuEntry::Separator);
    }

    // 3. System fonts
    menu.push(
        MenuItem::new("font.section.system", crate::i18n::text("_shell.font-system"))
            .disabled(true)
            .into(),
    );
    for choice in font_choices() {
        let font_name = choice.name.clone();
        let english = choice.english.clone();
        let label = crate::i18n::font_label(&font_name);
        let checked = current_font == Some(font_name.as_str())
            || current_font == Some(english.as_str());
        let on_select = on_select.clone();
        let fn_name = font_name.clone();
        menu.push(
            MenuItem::new(item_id, label)
                .checked(checked)
                .on(move |editor, window, cx| {
                    record_recent_font(&fn_name);
                    on_select(&fn_name, editor, window, cx);
                })
                .into(),
        );
    }
}

fn list() -> Option<&'static FontList> {
    // Fonts downloaded while the app runs (セットアップ): the list is made again from the names with them.
    let generation = crate::render::shape_view::fonts_generation();
    if generation > 0 {
        let mut late = LATE_LIST.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((g, list)) = *late
            && g == generation
        {
            return Some(list);
        }
        if let Some(system) = crate::render::shape_view::system_font_names() {
            let list: &'static FontList = Box::leak(Box::new(build(system.iter().map(String::as_str))));
            *late = Some((generation, list));
            return Some(list);
        }
    }
    if let Some(list) = LIST.get() {
        return Some(list);
    }
    if let Some(system) = crate::render::shape_view::system_font_names() {
        return Some(LIST.get_or_init(|| build(system.iter().map(String::as_str))));
    }
    #[cfg(target_os = "macos")]
    {
        let direct = enumerate_system_fonts_direct();
        if !direct.is_empty() {
            return Some(LIST.get_or_init(|| build(direct.iter().map(String::as_str))));
        }
    }
    // Fallback for environments without prior document draw.
    let fallback = ["IBM Plex Sans", "Carlito", "Arial", "Times New Roman", "Calibri"];
    Some(LIST.get_or_init(|| build(fallback.iter().copied())))
}

/// Families that are not listed in a web build's list besides the bundled ones: the ones a
/// browser draws (or the app stands in for with a bundled font).
#[cfg(target_family = "wasm")]
const WEB_EXTRA: &[&str] = &["Arial", "Times New Roman", "Calibri"];

/// Returns true if a font family name represents a Japanese typeface (contains
/// Hiragana, Katakana, CJK Unified Ideographs, or is a known Japanese font family).
pub(crate) fn is_japanese_font(name: &str) -> bool {
    name.chars().any(|c| {
        matches!(c,
            '\u{3040}'..='\u{309F}' | // Hiragana
            '\u{30A0}'..='\u{30FF}' | // Katakana
            '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        )
    })
}

/// Canonical phonetic/reading sort key for common Japanese font families.
fn japanese_font_sort_key(name: &str) -> &str {
    if name.starts_with("ＭＳ ゴシック") || name.starts_with("MS ゴシック") { // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
        "えむえすごしっく" // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
    } else if name.starts_with("ＭＳ 明朝") || name.starts_with("MS 明朝") { // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
        "えむえすみんちょう" // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
    } else if name.starts_with("游ゴシック") || name.starts_with("YuGothic") { // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
        "ゆうごしっく" // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
    } else if name.starts_with("游明朝") || name.starts_with("YuMincho") { // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
        "ゆうみんちょう" // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
    } else if name.starts_with("ヒラギノ角ゴ") || name.starts_with("Hiragino Sans") { // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
        "ひらぎのかくご" // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
    } else if name.starts_with("ヒラギノ丸ゴ") || name.starts_with("Hiragino Maru") { // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
        "ひらぎのまるご" // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
    } else if name.starts_with("ヒラギノ明朝") || name.starts_with("Hiragino Mincho") { // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
        "ひらぎのみんちょう" // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
    } else if name.starts_with("メイリオ") || name.starts_with("Meiryo") { // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
        "めいりお" // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
    } else if name.starts_with("BIZ UD") || name.starts_with("BIZ UDP") {
        "びずゆーでぃー" // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
    } else if name.starts_with("凸版文久") { // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
        "とっぱんぶんきゅう" // i18n-ok（日本語のフォント名とその読み（並べ替え用のデータ））
    } else {
        name
    }
}

fn build<'a>(system: impl Iterator<Item = &'a str>) -> FontList {
    let localized = localized_families();
    let mut choices: Vec<FontChoice> = Vec::new();
    let mut english = HashMap::new();
    #[cfg(target_family = "wasm")]
    let system = system.chain(WEB_EXTRA.iter().copied());
    for family in system {
        let family = family.trim();
        if family.is_empty() || family.starts_with('.') {
            continue;
        }
        let name = localized.get(family).map_or(family, String::as_str);
        if choices.iter().any(|c| c.name == name) {
            continue;
        }
        if name != family {
            english.insert(name.to_string(), family.to_string());
        }
        choices.push(FontChoice { name: name.to_string(), english: family.to_string() });
    }
    // Sort order:
    // 1. Japanese fonts first, sorted by Japanese syllabary order / canonical key.
    // 2. Latin and other fonts next, sorted alphabetically (case-insensitive).
    choices.sort_by(|a, b| {
        let a_jp = is_japanese_font(&a.name);
        let b_jp = is_japanese_font(&b.name);
        match (a_jp, b_jp) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (true, true) => {
                let ka = japanese_font_sort_key(&a.name);
                let kb = japanese_font_sort_key(&b.name);
                ka.cmp(kb).then_with(|| a.name.cmp(&b.name))
            }
            (false, false) => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });
    FontList { choices, english }
}

/// Enumerate available font family names directly on macOS using CoreText.
#[cfg(target_os = "macos")]
fn enumerate_system_fonts_direct() -> Vec<String> {
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::string::{CFString, CFStringRef};
    use core_text::font_collection::{CTFontCollectionRef, create_for_all_families};
    use core_text::font_descriptor::{CTFontDescriptor, CTFontDescriptorCopyAttribute, kCTFontFamilyNameAttribute};

    unsafe extern "C" {
        fn CTFontCollectionCreateMatchingFontDescriptors(collection: CTFontCollectionRef) -> CFArrayRef;
    }

    let collection = create_for_all_families();
    let descriptors: Option<CFArray<CTFontDescriptor>> = unsafe {
        let array = CTFontCollectionCreateMatchingFontDescriptors(collection.as_concrete_TypeRef());
        (!array.is_null()).then(|| CFArray::wrap_under_create_rule(array))
    };
    let mut names = Vec::new();
    let Some(descriptors) = descriptors else { return names };
    for descriptor in descriptors.iter() {
        let english_val = unsafe {
            let val = CTFontDescriptorCopyAttribute(descriptor.as_concrete_TypeRef(), kCTFontFamilyNameAttribute);
            if val.is_null() {
                None
            } else {
                let cf_val = CFType::wrap_under_create_rule(val);
                cf_val.instance_of::<CFString>().then(|| CFString::wrap_under_get_rule(cf_val.as_CFTypeRef() as CFStringRef).to_string())
            }
        };
        if let Some(name) = english_val {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

/// English family name -> the family name in the system's language (macOS, where the font
/// names the text system lists are the English ones).
#[cfg(target_os = "macos")]
fn localized_families() -> HashMap<String, String> {
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::string::{CFString, CFStringRef};
    use core_text::font_collection::{CTFontCollectionRef, create_for_all_families};
    use core_text::font_descriptor::{CTFontDescriptor, CTFontDescriptorCopyAttribute, CTFontDescriptorCopyLocalizedAttribute, kCTFontFamilyNameAttribute};

    unsafe extern "C" {
        // Follows the Create rule (the result is owned by the caller).
        fn CTFontCollectionCreateMatchingFontDescriptors(collection: CTFontCollectionRef) -> CFArrayRef;
    }
    let mut map = HashMap::new();
    let collection = create_for_all_families();
    let descriptors: Option<CFArray<CTFontDescriptor>> = unsafe {
        let array = CTFontCollectionCreateMatchingFontDescriptors(collection.as_concrete_TypeRef());
        (!array.is_null()).then(|| CFArray::wrap_under_create_rule(array))
    };
    let Some(descriptors) = descriptors else { return map };
    let string = |value: *const std::ffi::c_void| -> Option<String> {
        if value.is_null() {
            return None;
        }
        let value = unsafe { CFType::wrap_under_create_rule(value) };
        value.instance_of::<CFString>().then(|| unsafe { CFString::wrap_under_get_rule(value.as_CFTypeRef() as CFStringRef) }.to_string())
    };
    for descriptor in descriptors.iter() {
        let (english, localized) = unsafe {
            let english = string(CTFontDescriptorCopyAttribute(descriptor.as_concrete_TypeRef(), kCTFontFamilyNameAttribute));
            let mut language: CFStringRef = std::ptr::null();
            let localized = string(CTFontDescriptorCopyLocalizedAttribute(descriptor.as_concrete_TypeRef(), kCTFontFamilyNameAttribute, &mut language));
            if !language.is_null() {
                drop(CFString::wrap_under_create_rule(language));
            }
            (english, localized)
        };
        if let (Some(english), Some(localized)) = (english, localized)
            && english != localized
        {
            map.insert(english, localized);
        }
    }
    map
}

#[cfg(not(target_os = "macos"))]
fn localized_families() -> HashMap<String, String> {
    HashMap::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_and_duplicate_families_are_left_out_and_the_list_is_sorted() {
        let list = build(["Zapfino", ".SystemUIFont", "arial", "Arial", "Arial", "", "Courier", "ヒラギノ角ゴシック", "游明朝", "メイリオ"].into_iter());
        let names: Vec<&str> = list.choices.iter().map(|c| c.name.as_str()).collect();
        // Japanese fonts come first, sorted by Japanese reading/syllabary, followed by Latin fonts alphabetically.
        assert_eq!(names, ["ヒラギノ角ゴシック", "メイリオ", "游明朝", "arial", "Arial", "Courier", "Zapfino"]);
    }

    #[test]
    fn font_choices_returns_available_fonts() {
        let choices = font_choices();
        assert!(!choices.is_empty(), "Font choices should not be empty");
        // Verify that Japanese fonts appear before Latin fonts.
        let mut seen_latin = false;
        for choice in choices {
            if is_japanese_font(&choice.name) {
                assert!(!seen_latin, "Japanese font {} appeared after Latin fonts", choice.name);
            } else {
                seen_latin = true;
            }
        }
    }

    #[test]
    fn recent_fonts_ordering_and_limit() {
        crate::settings::set("font.recent", "");
        assert_eq!(recent_fonts(), Vec::<String>::new());

        record_recent_font("Arial");
        assert_eq!(recent_fonts(), vec!["Arial"]);

        record_recent_font("Times New Roman");
        assert_eq!(recent_fonts(), vec!["Times New Roman", "Arial"]);

        record_recent_font("Arial"); // Duplicate moves to front
        assert_eq!(recent_fonts(), vec!["Arial", "Times New Roman"]);

        record_recent_font("Font3");
        record_recent_font("Font4");
        record_recent_font("Font5");
        record_recent_font("Font6"); // Exceeds 5
        let list = recent_fonts();
        assert_eq!(list.len(), 5);
        assert_eq!(list, vec!["Font6", "Font5", "Font4", "Font3", "Arial"]);
        crate::settings::set("font.recent", "");
    }
}
