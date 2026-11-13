// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue 68, D13: the euro sign and the narrow no-break space are in the Latin (`hAnsi`) font of a run, not the East Asian one, unless the run
//! says `w:hint="eastAsia"`. So are the dashes, quotes and symbols, beside Japanese text too: the font slot is the character's Unicode range and the hint
//! (Word's PDFs, tasks/doc/m76-word-font-slots.md), not the paragraph's text.

use ibis_doc_model::FlowRunStyle;

fn style(hint: bool) -> FlowRunStyle {
    FlowRunStyle { font: Some("Times New Roman".into()), east_asian_font: Some("Yu Gothic".into()), east_asia_hint: hint, ..Default::default() }
}

#[test]
fn euro_and_narrow_no_break_space_are_in_the_latin_font() {
    let s = style(false);
    for c in ['€', '\u{202f}', '\u{205f}', '₩', '₹'] {
        assert_eq!(s.font_for(c), Some("Times New Roman"), "{c:?}");
    }
    // With `w:hint="eastAsia"` they are East Asian.
    assert_eq!(style(true).font_for('€'), Some("Yu Gothic"));
}

#[test]
fn dashes_and_symbols_are_in_the_latin_font_without_the_hint() {
    let s = style(false);
    for c in ['―', '—', '※', '…', '‘', '“'] {
        assert_eq!(s.font_for(c), Some("Times New Roman"), "{c:?}");
    }
    // Katakana middle dot and the wave dash are of the CJK Symbols range (3000 – 303F, 30A0 – 30FF).
    for c in ['〜', '・'] {
        assert_eq!(s.font_for(c), Some("Yu Gothic"), "{c:?}");
    }
}
