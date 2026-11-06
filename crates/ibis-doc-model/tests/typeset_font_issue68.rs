// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue 68, D13: the euro sign and the narrow no-break space are in the Latin font of a run, not the East Asian one
//! (unless the run says `w:hint="eastAsia"`); the dashes and quotes beside Japanese text are in the East Asian one as before.

use ibis_doc_model::FlowRunStyle;

fn style(hint: bool) -> FlowRunStyle {
    FlowRunStyle { font: Some("Times New Roman".into()), east_asian_font: Some("Yu Gothic".into()), east_asia_hint: hint, ..Default::default() }
}

#[test]
fn euro_and_narrow_no_break_space_are_in_the_latin_font() {
    let s = style(false);
    for c in ['€', '\u{202f}', '\u{205f}', '₩', '₹'] {
        // Among Japanese text they follow the East Asian font (Word's PDFs of `kinsoku-latin-*-probe.docx` show them in the Latin font only in a Latin paragraph).
        assert_eq!(s.font_for_in(c, true), Some("Yu Gothic"), "{c:?} in a Japanese paragraph");
        assert_eq!(s.font_for_in(c, false), Some("Times New Roman"), "{c:?} in a Latin paragraph");
    }
    // With `w:hint="eastAsia"` they are East Asian.
    assert_eq!(style(true).font_for_in('€', true), Some("Yu Gothic"));
}

#[test]
fn dashes_and_symbols_beside_japanese_text_stay_east_asian() {
    let s = style(false);
    for c in ['―', '—', '※', '〜', '・'] {
        assert_eq!(s.font_for_in(c, true), Some("Yu Gothic"), "{c:?}");
    }
    assert_eq!(s.font_for_in('…', true), Some("Times New Roman"));
}
