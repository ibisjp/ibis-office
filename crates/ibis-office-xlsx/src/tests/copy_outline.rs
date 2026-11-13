// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The user's K0011 (2026-11-12): ⌘C on cells showed no dashed outline (only a cut did). Excel outlines the copied cells until
//! Esc or typing into a cell; what was copied can still be pasted after that.

use super::SheetSteps;
use crate::shell::scenario::Scenario;
use ibis_doc_model::CellValue;
use crate::ext::*;

fn outlined(s: &mut Scenario) -> bool {
    s.with(|e, _| e.sheet_state().is_some_and(|st| st.copy_outline && st.clip.is_some()))
}

pub(crate) fn copy_shows_the_outline_until_esc_or_typing() {
    let mut s = super::new_sheet();
    crate::native_shape_clipboard::simulate_html(None);
    crate::native_shape_clipboard::simulate_embed_source(None);
    s.click_cell("A1");
    s.enter("7", "enter");
    s.click_cell("A1");
    assert!(!outlined(&mut s), "nothing copied yet");
    s.key("cmd-c");
    assert!(outlined(&mut s), "the copied cell is outlined");
    s.screenshot("k0011-copy-outline");
    s.key("escape");
    assert!(!outlined(&mut s), "Esc takes the outline away");
    s.key("cmd-c");
    assert!(outlined(&mut s), "copied again");
    s.click_cell("B1");
    s.key("cmd-v");
    assert_eq!(s.value("B1"), CellValue::Number(7.0), "pasted");
    s.click_cell("C1");
    s.enter("1", "enter");
    assert!(!outlined(&mut s), "typing into a cell takes the outline away");
}
