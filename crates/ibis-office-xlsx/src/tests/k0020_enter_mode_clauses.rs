// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0020: the clause being converted is drawn thick whether the cell was entered by typing straight into it (Enter mode) or with F2 (Edit mode).
use super::SheetSteps;
use crate::shell::format::with_format;
use crate::shell::scenario::Scenario;
use crate::ext::*;
use gpui_kit::{ImeClause, ImeLine};
use ibis_doc_model::UnderlineStyle;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::k0020_the_converting_clause_is_thick_in_enter_mode", || the_converting_clause_is_thick(false)),
        ("xlsx::k0020_the_converting_clause_is_thick_in_edit_mode", || the_converting_clause_is_thick(true)),
    ]
}

fn the_converting_clause_is_thick(f2: bool) {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("K20");
    if f2 {
        s.key("f2");
    }
    // As the platform does: the marked text, then its clauses (the first key is typed before any edit exists in Enter mode).
    let first = "\u{3042}";
    s.ime_compose(first);
    s.update(|e, w, cx| with_format!(e, set_ime_clauses(e, vec![ImeClause { range: 0..1, thick: false, line: ImeLine::Dotted }], w, cx)));
    s.settle();
    assert_eq!(s.with(|e, _| e.cell_edit().map(|c| c.marked_underlines().iter().map(|u| u.0).collect::<Vec<_>>())), Some(vec![UnderlineStyle::Dotted]));
    // Converted: the first clause is being converted (thick), the second is not.
    let text = "\u{3042}\u{3044}\u{3046}\u{3048}";
    s.ime_compose(text);
    s.update(|e, w, cx| {
        with_format!(e, set_ime_clauses(e, vec![ImeClause { range: 0..2, thick: true, line: ImeLine::Solid }, ImeClause { range: 2..4, thick: false, line: ImeLine::Solid }], w, cx))
    });
    s.settle();
    let styles = s.with(|e, _| e.cell_edit().map(|c| c.marked_underlines().iter().map(|u| u.0).collect::<Vec<_>>()));
    assert_eq!(styles, Some(vec![UnderlineStyle::Thick, UnderlineStyle::Single]), "f2={f2}");
}
