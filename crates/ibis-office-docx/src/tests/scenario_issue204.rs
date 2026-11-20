// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #204: once the mail merge panel had been shown, opening the cross-reference dialog showed the merge panel again
//! (the merge panel came first in the chain of the document's panels, and nothing hid it when another panel opened).

use crate::shell::scenario::Scenario;
use crate::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue204_cross_reference_opens_while_the_merge_panel_is_open", cross_reference_opens_while_the_merge_panel_is_open),
        ("docx::issue204_cross_reference_opens_after_the_merge_panel_was_closed", cross_reference_opens_after_the_merge_panel_was_closed),
        ("docx::issue204_other_panels_open_over_the_merge_panel", other_panels_open_over_the_merge_panel),
    ]
}

fn merge_visible(s: &mut Scenario) -> bool {
    s.with(|e, _| e.doc_state().and_then(|st| st.merge.as_ref()).is_some_and(|m| m.visible))
}

fn xref_open(s: &mut Scenario) -> bool {
    s.with(|e, _| e.doc_state().is_some_and(|st| st.helper.cross_reference.is_some()))
}

fn cross_reference_opens_while_the_merge_panel_is_open() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.menu("docx.mail-merge");
    assert!(merge_visible(&mut s));
    s.menu("docx.cross-reference.dialog");
    assert!(xref_open(&mut s), "the cross-reference panel opens");
    assert!(!merge_visible(&mut s), "the merge panel gives way");
    assert_eq!(s.with(|e, _| e.dialog.key), Some("cross_reference_panel"), "the cross-reference dialog is the one shown");
}

fn cross_reference_opens_after_the_merge_panel_was_closed() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.menu("docx.mail-merge");
    s.update(|e, w, cx| e.close_dialog(w, cx));
    s.settle();
    s.menu("docx.cross-reference.dialog");
    assert!(xref_open(&mut s));
    assert_eq!(s.with(|e, _| e.dialog.key), Some("cross_reference_panel"), "the cross-reference dialog is the one shown");
}

fn other_panels_open_over_the_merge_panel() {
    for (menu, key) in [("docx.notes.settings", "notes_panel"), ("docx.bookmark", "bookmark_panel"), ("docx.caption.insert", "caption_panel"), ("docx.fields.insert", "field_panel"), ("docx.table-of-contents.update-dialog", "update_toc_panel")] {
        let mut s = Scenario::open("test/editing/docx-report.docx");
        s.menu("docx.mail-merge");
        s.menu(menu);
        assert!(!merge_visible(&mut s), "{menu}: the merge panel gives way");
        assert_eq!(s.with(|e, _| e.dialog.key), Some(key), "{menu}: its own dialog is shown");
    }
}
