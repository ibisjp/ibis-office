// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 禁則 is looked at past the spaces between the characters (GitHub issue #162, Word's PDF of `kinsoku-across-space.docx`, 4 paragraphs of
//! 40 kana and 「 + a space, or a full-width space + ？, then kana): a line does not end with 「 and a space, and does not start with the
//! full-width space and ？ that follows; the 「 and the character before the spaces go down with them.

use super::flow_layout::Item;
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("docx::kinsoku_is_looked_at_past_the_spaces", kinsoku_is_looked_at_past_the_spaces)]
}

fn kinsoku_is_looked_at_past_the_spaces() {
    let mut s = Scenario::open("test/editing/kinsoku-across-space.docx");
    let pages = s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources));
    let mut lines: Vec<(i32, String)> = Vec::new();
    for it in &pages.pages[0].items {
        if let Item::Text { y, text, .. } = it {
            let key = (*y * 10.0).round() as i32;
            match lines.iter_mut().find(|(k, _)| *k == key) {
                Some((_, t)) => t.push_str(text),
                None => lines.push((key, text.to_string())),
            }
        }
    }
    lines.sort();
    let lines: Vec<String> = lines.into_iter().map(|(_, t)| t).collect();
    assert_eq!(lines.len(), 8, "two lines a paragraph: {lines:?}");
    for (n, line) in lines.iter().enumerate() {
        let kana = line.chars().filter(|c| !c.is_whitespace()).count();
        let trimmed = line.trim_matches(|c: char| c.is_whitespace());
        assert!(!trimmed.ends_with('\u{300C}'), "line {n} ends with U+300C: {line:?}");
        assert!(!trimmed.starts_with('\u{FF1F}'), "line {n} starts with U+FF1F: {line:?}");
        if n % 2 == 0 {
            // Word's first lines: 40 kana before 「 and its space (the 「 goes down), 39 before the full-width space and ？ (the 40th goes too).
            let word = if n < 4 { 40 } else { 39 };
            assert_eq!(kana, word, "line {n}: {line:?}");
        }
    }
}
