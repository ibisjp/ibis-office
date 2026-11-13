// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue #158 (the part that needs no typeface of its own): an ideographic variation sequence (a kanji and a selector U+E0100..E01EF) and a character of
//! Plane 16 (U+100000, the private use area of the administrative affairs standard characters) read, move the caret, delete, save and read back whether or
//! not the PC has a typeface for them. `test/editing/docx-ivs-plane16.docx` (made by `scripts/corpus/gen_docx_ivs_158.py`) has the paragraphs
//! U+845B U+E0100 U+57CE, U+845B U+57CE, A U+100000 B and U+2460 U+FE00 C.
use super::scenarios::DocSteps as _;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{Block, FlowPos};
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::ivs_file_reads_and_saves_back_unchanged", reads_and_saves_back),
        ("docx::ivs_caret_moves_over_the_pair_as_one_character", caret_moves_over_the_pair),
        ("docx::ivs_backspace_and_delete_take_the_pair_together", backspace_and_delete),
        ("docx::ivs_selector_added_after_the_base_character_joins_it", selector_added_later),
        ("docx::ivs_plane16_character_is_one_character_for_caret_and_delete", plane16_character),
    ]
}

const FILE: &str = "test/editing/docx-ivs-plane16.docx";
const KATSU: &str = "\u{845b}";
const IVS: &str = "\u{845b}\u{e0100}";
const JO: &str = "\u{57ce}";

fn texts(s: &mut Scenario) -> Vec<String> {
    let doc = s.document();
    doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.text()) } else { None }).collect()
}

fn ids(s: &mut Scenario) -> Vec<u32> {
    let doc = s.document();
    doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).collect()
}

fn caret_at(s: &mut Scenario, pos: FlowPos) {
    s.update(|e, _, cx| e.set_caret(pos, false, cx));
}

fn caret(s: &mut Scenario) -> FlowPos {
    s.with(|e, _| e.doc_state().and_then(|d| d.caret)).expect("a caret")
}

fn reads_and_saves_back() {
    let mut s = Scenario::open(FILE);
    let want = vec![format!("{IVS}{JO}"), format!("{KATSU}{JO}"), "A\u{100000}B".to_string(), "\u{2460}\u{fe00}C".to_string()];
    assert_eq!(texts(&mut s), want);
    s.save_as("ivs-plane16.docx");
    s.reload_saved();
    assert_eq!(texts(&mut s), want, "the saved file has the selectors and the Plane 16 character as they were");
}

/// Right from before the pair goes after both; Left from after goes back before both (Word moves over an IVS as one character).
fn caret_moves_over_the_pair() {
    let mut s = Scenario::open(FILE);
    let id = ids(&mut s);
    caret_at(&mut s, FlowPos::new(id[0], 0));
    s.key("right");
    assert_eq!(caret(&mut s), FlowPos::new(id[0], 2), "past the kanji and its selector");
    s.key("right");
    assert_eq!(caret(&mut s), FlowPos::new(id[0], 3));
    s.key("left");
    s.key("left");
    assert_eq!(caret(&mut s), FlowPos::new(id[0], 0), "back before the kanji, not between it and the selector");
    // The standardized selector U+FE00 goes with its base the same way.
    caret_at(&mut s, FlowPos::new(id[3], 0));
    s.key("right");
    assert_eq!(caret(&mut s), FlowPos::new(id[3], 2));
}

fn backspace_and_delete() {
    let mut s = Scenario::open(FILE);
    let id = ids(&mut s);
    caret_at(&mut s, FlowPos::new(id[0], 2));
    s.key("backspace");
    assert_eq!(texts(&mut s)[0], JO, "Backspace after the pair takes the kanji and its selector");
    assert_eq!(caret(&mut s), FlowPos::new(id[0], 0));
    drop(s);
    let mut s = Scenario::open(FILE);
    caret_at(&mut s, FlowPos::new(id[0], 0));
    s.key("delete");
    assert_eq!(texts(&mut s)[0], JO, "Delete before the pair takes both");
    // A kanji alone is still one character.
    caret_at(&mut s, FlowPos::new(id[1], 1));
    s.key("backspace");
    assert_eq!(texts(&mut s)[1], JO);
}

/// A selector put after a kanji that was there alone: the pair is one character for the caret from then on.
fn selector_added_later() {
    let mut s = Scenario::open(FILE);
    let id = ids(&mut s);
    caret_at(&mut s, FlowPos::new(id[1], 1));
    s.type_text("\u{e0100}");
    assert_eq!(texts(&mut s)[1], format!("{IVS}{JO}"));
    caret_at(&mut s, FlowPos::new(id[1], 0));
    s.key("right");
    assert_eq!(caret(&mut s), FlowPos::new(id[1], 2));
    s.save_as("ivs-added.docx");
    s.reload_saved();
    assert_eq!(texts(&mut s)[1], format!("{IVS}{JO}"));
}

/// U+100000 is one character (one position, one Backspace) with or without a typeface for it.
fn plane16_character() {
    let mut s = Scenario::open(FILE);
    let id = ids(&mut s);
    caret_at(&mut s, FlowPos::new(id[2], 1));
    s.key("right");
    assert_eq!(caret(&mut s), FlowPos::new(id[2], 2));
    s.key("backspace");
    assert_eq!(texts(&mut s)[2], "AB");
}
