// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The strings lately searched in the menu search ("履歴", 2026-11-03).
//!
//! Each screen kind (document, spreadsheet, presentation) keeps its own most-recently-used list of at most
//! [`MAX`] strings in the app's settings file (`settings.rs`, one key per kind). A string is noted when a
//! command is run from the search or when the arrow down key moves through the found items; a string that is
//! already there moves to the front, and an empty string is never noted. The file is small, so it is written
//! at every change. The lists are read when the search opens ([`invalidate`]) and kept while it is open.

use std::cell::RefCell;
use std::collections::HashMap;

use ibis_pptx::Kind;

/// How many strings a kind remembers.
pub(crate) const MAX: usize = 5;

/// The setting that keeps the history of `kind`.
pub fn key(kind: Kind) -> &'static str {
    crate::format::info(kind).search_history_key
}

thread_local! {
    static CACHE: RefCell<HashMap<&'static str, Vec<String>>> = RefCell::new(HashMap::new());
}

/// The settings hold one line per key: the strings are joined by a tab (a typed string has neither a tab nor a
/// line break after [`clean`]).
const SEPARATOR: char = '\t';

/// `text` as it is kept: one line, no tabs, no space at the ends.
fn clean(text: &str) -> String {
    text.chars().map(|c| if c == SEPARATOR || c.is_control() { ' ' } else { c }).collect::<String>().trim().to_string()
}

/// `list` after `text` was searched: first, once, and at most [`MAX`]. Returns whether the list changed.
pub(crate) fn push(list: &mut Vec<String>, text: &str) -> bool {
    let text = clean(text);
    if text.is_empty() || list.first() == Some(&text) {
        return false;
    }
    list.retain(|old| *old != text);
    list.insert(0, text);
    list.truncate(MAX);
    true
}

fn decode(value: &str) -> Vec<String> {
    let mut list: Vec<String> = Vec::new();
    for part in value.split(SEPARATOR) {
        let part = clean(part);
        if !part.is_empty() && !list.contains(&part) && list.len() < MAX {
            list.push(part);
        }
    }
    list
}

/// Forget what was read: the next [`load`] reads the settings again.
pub fn invalidate() {
    CACHE.with(|cache| cache.borrow_mut().clear());
}

/// The strings of `kind`, the newest first.
pub fn load(kind: Kind) -> Vec<String> {
    let key = key(kind);
    CACHE.with(|cache| cache.borrow_mut().entry(key).or_insert_with(|| crate::settings::get(key).map(|v| decode(&v)).unwrap_or_default()).clone())
}

/// Note `text` as searched for `kind` and write the settings.
pub(crate) fn record(kind: Kind, text: &str) {
    let mut list = load(kind);
    if !push(&mut list, text) {
        return;
    }
    let key = key(kind);
    crate::settings::set(key, &list.join(&SEPARATOR.to_string()));
    CACHE.with(|cache| cache.borrow_mut().insert(key, list));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_moves_a_repeat_to_the_front_and_keeps_five() {
        let mut list = Vec::new();
        for word in ["a", "b", "c", "d", "e", "f"] {
            assert!(push(&mut list, word));
        }
        assert_eq!(list, ["f", "e", "d", "c", "b"]);
        assert!(push(&mut list, "d"));
        assert_eq!(list, ["d", "f", "e", "c", "b"]);
        assert!(!push(&mut list, "d"), "the newest again changes nothing");
        assert!(!push(&mut list, "  "), "an empty string is not noted");
        assert_eq!(list.len(), MAX);
    }

    #[test]
    fn decode_drops_blanks_and_repeats() {
        assert_eq!(decode("a\t\tb\ta\tc\td\te\tf"), ["a", "b", "c", "d", "e"]);
    }
}
