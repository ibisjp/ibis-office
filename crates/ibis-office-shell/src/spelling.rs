// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スペルチェック: the words of a text the system's spell checker does not
//! know (`NSSpellChecker` on macOS, with the user's own dictionary), and what
//! it suggests for them. Other platforms check nothing yet.

/// A word the checker does not know: where it is in the text, in characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Misspelling {
    pub start: usize,
    pub end: usize,
    pub word: String,
}

/// Whether the system can check spelling.
pub fn available() -> bool {
    cfg!(target_os = "macos")
}

/// The words of `text` not known in `language` (`None`: the system's own choice from the text).
pub(crate) fn check(text: &str, language: Option<&str>) -> Vec<Misspelling> {
    #[cfg(target_os = "macos")]
    return mac::check(text, language);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (text, language);
        Vec::new()
    }
}

/// The words of `text` not known, as `check` finds them but in the language the settings choose (when `language` is
/// not given) and without the kinds of words they leave out (UPPERCASE, with numbers, addresses).
pub fn check_by_settings(text: &str, language: Option<&str>) -> Vec<Misspelling> {
    let settings = crate::autocorrect::settings();
    // The system names a language with an underscore (`en_US`).
    let chosen = (!settings.spell_language.is_empty()).then(|| settings.spell_language.replace('-', "_"));
    let mut found = check(text, language.or(chosen.as_deref()));
    found.retain(|m| keeps(&settings, &m.word));
    found
}

/// Whether the settings let the unknown `word` be marked.
fn keeps(settings: &crate::autocorrect::AutoCorrect, word: &str) -> bool {
    let letters = word.chars().filter(|c| c.is_alphabetic());
    let upper = letters.clone().count() > 1 && letters.clone().all(char::is_uppercase);
    let numbers = word.chars().any(|c| c.is_ascii_digit());
    let address = word.contains("://") || word.starts_with("www.") || word.contains('@') || word.contains('/') || word.contains('\\');
    !(settings.spell_ignore_upper && upper || settings.spell_ignore_numbers && numbers || settings.spell_ignore_internet && address)
}

/// What the checker suggests for the word at characters `start..end` of `text`.
pub fn suggestions(text: &str, start: usize, end: usize, language: Option<&str>) -> Vec<String> {
    #[cfg(target_os = "macos")]
    return mac::suggestions(text, start, end, language);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (text, start, end, language);
        Vec::new()
    }
}

/// 辞書に追加: `word` learned by the system's dictionary of the user. In tests the
/// word is only noted (`learned_in_tests`): the user's own dictionary is not touched.
pub fn learn(word: &str) {
    #[cfg(any(test, feature = "scenarios"))]
    {
        LEARNED.with(|l| l.borrow_mut().push(word.to_string()));
        return;
    }
    #[cfg(all(target_os = "macos", not(any(test, feature = "scenarios"))))]
    mac::learn(word);
    #[cfg(all(not(target_os = "macos"), not(any(test, feature = "scenarios"))))]
    let _ = word;
}

#[cfg(any(test, feature = "scenarios"))]
thread_local! {
    static LEARNED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The words `learn` was asked for in this test.
#[cfg(any(test, feature = "scenarios"))]
pub fn learned_in_tests() -> Vec<String> {
    LEARNED.with(|l| l.borrow().clone())
}

/// The offset in UTF-16 units that characters `chars` of `text` come to (for the macOS spell checker, `mac`).
#[cfg(any(target_os = "macos", test))]
pub(crate) fn utf16_offset(text: &str, chars: usize) -> usize {
    text.chars().take(chars).map(char::len_utf16).sum()
}

/// The character count that UTF-16 offset `units` of `text` is.
#[cfg(any(target_os = "macos", test))]
pub(crate) fn char_offset(text: &str, units: usize) -> usize {
    let mut seen = 0;
    for (i, c) in text.chars().enumerate() {
        if seen >= units {
            return i;
        }
        seen += c.len_utf16();
    }
    text.chars().count()
}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
mod mac {
    use std::ffi::c_void;

    use objc::runtime::{BOOL, NO, Object};
    use objc::{Encode, Encoding, class, msg_send, sel, sel_impl};

    use super::{Misspelling, char_offset, utf16_offset};

    type Id = *mut Object;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct NSRange {
        location: usize,
        length: usize,
    }

    unsafe impl Encode for NSRange {
        fn encode() -> Encoding {
            unsafe { Encoding::from_str("{_NSRange=QQ}") }
        }
    }

    #[link(name = "AppKit", kind = "framework")]
    #[link(name = "Foundation", kind = "framework")]
    unsafe extern "C" {}

    const NOT_FOUND: usize = isize::MAX as usize;

    fn string(text: &str) -> Id {
        unsafe {
            let s: Id = msg_send![class!(NSString), alloc];
            let s: Id = msg_send![s, initWithBytes: text.as_ptr() as *const c_void length: text.len() encoding: 4usize];
            msg_send![s, autorelease]
        }
    }

    fn rust_string(s: Id) -> String {
        unsafe {
            let bytes: *const std::os::raw::c_char = msg_send![s, UTF8String];
            if bytes.is_null() {
                return String::new();
            }
            std::ffi::CStr::from_ptr(bytes).to_string_lossy().into_owned()
        }
    }

    fn checker() -> Id {
        unsafe { msg_send![class!(NSSpellChecker), sharedSpellChecker] }
    }

    fn language_arg(language: Option<&str>) -> Id {
        language.map_or(std::ptr::null_mut(), string)
    }

    pub(super) fn check(text: &str, language: Option<&str>) -> Vec<Misspelling> {
        let mut found = Vec::new();
        if text.trim().is_empty() {
            return found;
        }
        unsafe {
            let pool: Id = msg_send![class!(NSAutoreleasePool), new];
            let checker = checker();
            let ns = string(text);
            let language = language_arg(language);
            let total = utf16_offset(text, usize::MAX);
            let mut at = 0usize;
            while at < total {
                let range: NSRange = msg_send![checker, checkSpellingOfString: ns startingAt: at as isize language: language wrap: NO inSpellDocumentWithTag: 0isize wordCount: std::ptr::null_mut::<isize>()];
                if range.location >= NOT_FOUND || range.length == 0 {
                    break;
                }
                let (start, end) = (char_offset(text, range.location), char_offset(text, range.location + range.length));
                let word: String = text.chars().skip(start).take(end - start).collect();
                found.push(Misspelling { start, end, word });
                at = range.location + range.length;
            }
            let _: () = msg_send![pool, drain];
        }
        found
    }

    pub(super) fn suggestions(text: &str, start: usize, end: usize, language: Option<&str>) -> Vec<String> {
        let mut out = Vec::new();
        unsafe {
            let pool: Id = msg_send![class!(NSAutoreleasePool), new];
            let checker = checker();
            let (a, b) = (utf16_offset(text, start), utf16_offset(text, end));
            let range = NSRange { location: a, length: b - a };
            let guesses: Id = msg_send![checker, guessesForWordRange: range inString: string(text) language: language_arg(language) inSpellDocumentWithTag: 0isize];
            if !guesses.is_null() {
                let count: usize = msg_send![guesses, count];
                for i in 0..count.min(8) {
                    let item: Id = msg_send![guesses, objectAtIndex: i];
                    out.push(rust_string(item));
                }
            }
            let _: () = msg_send![pool, drain];
        }
        out
    }

    #[cfg_attr(any(test, feature = "scenarios"), allow(dead_code))]
    pub(super) fn learn(word: &str) {
        unsafe {
            let pool: Id = msg_send![class!(NSAutoreleasePool), new];
            let _: () = msg_send![checker(), learnWord: string(word)];
            let _: () = msg_send![pool, drain];
        }
    }

    #[allow(dead_code)]
    const _: BOOL = NO;
}

#[cfg(test)]
mod tests {
    use super::{char_offset, utf16_offset};

    #[test]
    fn offsets_between_characters_and_utf16() {
        // A character outside the BMP takes two UTF-16 units.
        let text = "a😀bc";
        assert_eq!(utf16_offset(text, 2), 3);
        assert_eq!(char_offset(text, 3), 2);
        assert_eq!(char_offset(text, 4), 3);
        assert_eq!(char_offset(text, 100), 4);
        assert_eq!(utf16_offset(text, usize::MAX), 5);
    }

    #[test]
    fn the_settings_leave_out_kinds_of_words() {
        let mut settings = crate::autocorrect::AutoCorrect::default();
        for (word, kept) in [("recieve", true), ("NASA", false), ("abc123", false), ("www.example.jp", false), ("me@example.jp", false), ("A", true)] {
            assert_eq!(super::keeps(&settings, word), kept, "{word}");
        }
        settings.spell_ignore_upper = false;
        settings.spell_ignore_numbers = false;
        settings.spell_ignore_internet = false;
        for word in ["NASA", "abc123", "www.example.jp"] {
            assert!(super::keeps(&settings, word), "{word}");
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_misspelled_english_word_is_found() {
        let found = super::check("This is a recieve test", Some("en_US"));
        eprintln!("spell check found: {found:?}");
        // Where the system has an English dictionary.
        if let Some(first) = found.first() {
            assert_eq!((first.start, first.end, first.word.as_str()), (10, 17, "recieve"));
            assert!(super::suggestions("This is a recieve test", 10, 17, Some("en_US")).iter().any(|s| s == "receive"));
        }
    }
}
