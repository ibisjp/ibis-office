// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #148 (reopened): after `@sum(` a range is pointed at with the arrow keys or the mouse as after `=sum(` (and `+sum(`, `-sum(`);
//! the entry is then the formula `=SUM(range)` as Excel makes it (`@sum(D5:E7)` is `=SUM(D5:E7)`).
use super::SheetSteps;
use crate::ext::*;
use crate::shell::scenario::Scenario;

const STARTS: [(&str, &str); 4] = [("=", "SUM(E4:F4)"), ("@", "SUM(E4:F4)"), ("+", "+SUM(E4:F4)"), ("-", "-SUM(E4:F4)")];

fn text(s: &mut Scenario) -> Option<String> {
    s.with(|e, _| e.cell_edit().map(|c| c.text.clone()))
}

/// The arrow keys: the cell to the left of the one being edited, then the range grown one more cell to the left with Shift.
pub(crate) fn keys() {
    for (start, expected) in STARTS {
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        s.click_cell("G4");
        s.type_text(&format!("{start}SUM("));
        s.key("left");
        assert_eq!(text(&mut s).as_deref(), Some(format!("{start}SUM(F4").as_str()), "the arrow key points at a cell after {start}");
        s.key("shift-left");
        assert_eq!(text(&mut s).as_deref(), Some(format!("{start}SUM(E4:F4").as_str()), "Shift grows it to a range after {start}");
        s.type_text(")");
        s.key("enter");
        assert_eq!(s.formula("G4").as_deref(), Some(expected), "the formula after {start}");
        assert!(s.with(|e, _| e.cell_edit().is_none()), "the entry is done");
    }
}

/// The mouse: a range dragged over cells, as in XSC-44.
pub(crate) fn mouse() {
    for (start, expected) in STARTS {
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        s.click_cell("G4");
        s.type_text(&format!("{start}SUM("));
        let (a, b) = (s.with(|e, _| e.cell_center(4, 3)).unwrap(), s.with(|e, _| e.cell_center(5, 3)).unwrap());
        s.drag((f32::from(b.x), f32::from(b.y)), (f32::from(a.x), f32::from(a.y)));
        assert_eq!(text(&mut s).as_deref(), Some(format!("{start}SUM(E4:F4").as_str()), "the mouse points at a range after {start}");
        s.type_text(")");
        s.key("enter");
        assert_eq!(s.formula("G4").as_deref(), Some(expected), "the formula after {start}");
    }
}
