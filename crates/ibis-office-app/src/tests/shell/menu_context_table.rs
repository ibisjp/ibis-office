// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The shell's context menus (`menu_context.rs` of ibis-office-shell) as each format's table makes them: the outer contexts
//! are the formats' (`FormatInfo::outer_contexts`), so this test runs where the formats are registered.

use ibis_pptx::Kind;

use crate::shell::menu_context::ContextMenu::*;
use crate::shell::menu_context::ObjectKind as O;
use crate::shell::menu_context::{ContextState, visible_contexts};

fn state(kind: Kind) -> ContextState {
    ContextState::none(kind)
}

/// Every row of the table of spec 4.2.
#[test]
fn contexts_table() {
    crate::register_hooks();
    let d = Kind::Document;
    let x = Kind::Workbook;
    let p = Kind::Presentation;
    // Nothing selected, or text: none.
    for kind in [d, x, p] {
        assert_eq!(visible_contexts(&state(kind)), vec![]);
    }
    // A document: the caret in a table.
    let mut s = state(d);
    s.caret_in_table = true;
    assert_eq!(visible_contexts(&s), vec![TableDesign, TableLayout]);
    // A picture in a table: three menus, the table's first.
    s.objects = vec![O::Picture];
    assert_eq!(visible_contexts(&s), vec![TableDesign, TableLayout, PictureFormat]);
    // A shape in a table.
    s.objects = vec![O::Shape];
    assert_eq!(visible_contexts(&s), vec![TableDesign, TableLayout, ShapeFormat]);
    // Shapes, pictures and charts on the three screens.
    for kind in [d, x, p] {
        let mut s = state(kind);
        s.objects = vec![O::Shape];
        assert_eq!(visible_contexts(&s), vec![ShapeFormat]);
        s.objects = vec![O::Picture];
        assert_eq!(visible_contexts(&s), vec![PictureFormat]);
        s.objects = vec![O::Chart];
        assert_eq!(visible_contexts(&s), vec![ChartDesign, ChartFormat]);
        // Several of one kind: that kind's menus. Different kinds: what they share.
        s.objects = vec![O::Picture, O::Picture];
        assert_eq!(visible_contexts(&s), vec![PictureFormat]);
        s.objects = vec![O::Shape, O::Picture];
        assert_eq!(visible_contexts(&s), vec![ShapeFormat]);
        s.objects = vec![O::Chart, O::Shape];
        assert_eq!(visible_contexts(&s), vec![ShapeFormat]);
    }
    // An equation, the header and footer, the outline: of a document only.
    let mut s = state(d);
    s.in_equation = true;
    assert_eq!(visible_contexts(&s), vec![Equation]);
    let mut s = state(d);
    s.header_footer = true;
    assert_eq!(visible_contexts(&s), vec![HeaderFooter]);
    s.caret_in_table = true;
    assert_eq!(visible_contexts(&s), vec![HeaderFooter, TableDesign, TableLayout], "a mode's menu comes first");
    let mut s = state(d);
    s.outline_view = true;
    assert_eq!(visible_contexts(&s), vec![Outline]);
    let mut s = state(p);
    s.outline_view = true;
    s.header_footer = true;
    assert_eq!(visible_contexts(&s), vec![], "the document's modes do not exist on a slide");
    // A spreadsheet's cell.
    let mut s = state(x);
    s.in_sheet_table = true;
    assert_eq!(visible_contexts(&s), vec![TableDesign]);
    let mut s = state(x);
    s.in_pivot = true;
    assert_eq!(visible_contexts(&s), vec![PivotAnalyze, PivotDesign]);
    let mut s = state(x);
    s.in_sparkline = true;
    s.in_sheet_table = true;
    assert_eq!(visible_contexts(&s), vec![TableDesign, Sparkline], "a sparkline in a table: both");
    let mut s = state(x);
    s.objects = vec![O::Slicer];
    assert_eq!(visible_contexts(&s), vec![Slicer]);
    // A presentation.
    let mut s = state(p);
    s.objects = vec![O::Table];
    assert_eq!(visible_contexts(&s), vec![TableDesign, TableLayout]);
    s.objects = vec![O::SmartArt];
    assert_eq!(visible_contexts(&s), vec![SmartArtDesign, SmartArtFormat]);
    s.objects = vec![O::Video];
    assert_eq!(visible_contexts(&s), vec![VideoFormat, VideoPlayback]);
    let mut s = state(p);
    s.master_view = true;
    assert_eq!(visible_contexts(&s), vec![SlideMaster]);
    s.objects = vec![O::Shape];
    assert_eq!(visible_contexts(&s), vec![SlideMaster, ShapeFormat], "the master's menu, then the shape's");
}
