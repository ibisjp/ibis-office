// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The underline of vertical writing (`tbRl`) is drawn on the right of its characters, as Word's (GitHub issue #160). A plain
//! across underline lies below the baseline; the page turned a quarter put it on the left of the column.

use super::flow_layout::Item;
use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("docx::vertical_underline_is_on_the_right_of_the_characters", the_underline_is_on_the_right_of_the_characters)]
}

fn the_underline_is_on_the_right_of_the_characters() {
    let mut s = Scenario::open("test/editing/vertical-underline.docx");
    let pages = s.with(|e, _| super::flow_layout::pages(e.flow().expect("a document"), &e.resources));
    let items = &pages.pages[0].items;
    // The first paragraph's four kana (the rightmost column), and its underline: a thin box as long as the four kana.
    let kana: Vec<(f32, f32)> = items.iter().filter_map(|i| if let Item::Text { x, y, text, size, .. } = i { text.starts_with('あ').then_some((*x + size / 2.0, *y)) } else { None }).collect();
    let right = kana.iter().map(|k| k.0).fold(f32::MIN, f32::max);
    let lines: Vec<(f32, f32, f32, f32)> = items.iter().filter_map(|i| if let Item::Rect { x, y, w, h, .. } = i { (*w < 2.0 && *h > 20.0).then_some((*x, *y, *w, *h)) } else { None }).collect();
    assert_eq!(lines.len(), 1, "one underline: {lines:?}");
    assert!(lines[0].0 > right, "the underline (x {:.2}) is on the right of the column's middle ({right:.2})", lines[0].0);
}
