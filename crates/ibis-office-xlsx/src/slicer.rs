// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pressing a slicer's buttons (スライサー): the items selected filter the
//! table or the pivot tables the slicer is on, the same way the ▼ panel of
//! the autofilter (and of the pivot table) does.

use gpui_kit::*;
use ibis_doc_model::slicer::{Slicer, SlicerSource, button_at, locate_pivot, locate_table, select_after_click, table_column};
use ibis_doc_model::{AutoFilter, ColumnFilter, Command, CellRange, ShapeId, ShapeKind, SheetCommand};

use crate::shell::editor::SlideEditor;
use crate::ext::*;

#[ibis_office_macros::editor_ext(SlicerExt)]
impl SlideEditor {
    /// A press on a slicer shape: a button of it selected (⌘/Ctrl: one more
    /// or less; Shift: a run of them), the header's clear icon, or neither
    /// (`false`: the press is for the shape).
    pub(crate) fn slicer_press(&mut self, shape: ShapeId, event: &MouseDownEvent, cx: &mut Context<Self>) -> bool {
        let Some(layout) = self.slide_layout() else { return false };
        let Some(sh) = self.slide().shape(shape) else { return false };
        let ShapeKind::Slicer(slicer) = &sh.kind else { return false };
        let slicer = slicer.clone();
        let p = layout.to_emu(event.position);
        let (x, y) = (p.x - sh.frame.x, p.y - sh.frame.y);
        let selected: Vec<bool> = slicer.items.iter().map(|i| i.selected).collect();
        let m = &event.modifiers;
        let next = if ibis_doc_model::slicer::clear_icon_at(&slicer, sh.frame.width, x, y) {
            vec![true; selected.len()]
        } else if let Some(i) = button_at(&slicer, sh.frame.width, sh.frame.height, x, y) {
            select_after_click(&selected, i, m.platform || m.control, m.shift)
        } else {
            return false;
        };
        if next != selected {
            self.select_slicer_items(&slicer, &next, cx);
        }
        true
    }

    /// Filter what `slicer` is on to keep the items `selected`.
    pub(crate) fn select_slicer_items(&mut self, slicer: &Slicer, selected: &[bool], cx: &mut Context<Self>) {
        let all = selected.iter().all(|&s| s);
        match &slicer.source {
            SlicerSource::Table { table, field } => {
                let Some((slide, index)) = locate_table(self.presentation(), table) else { return };
                let Some(sheet) = self.presentation().slide(slide).and_then(ibis_doc_model::sheet_calc::sheet_of_slide) else { return };
                let Some(t) = sheet.tables.get(index) else { return };
                let Some(column) = table_column(t, field) else { return };
                let mut filter = t.filter.clone().unwrap_or(AutoFilter { range: CellRange { last_row: t.range.last_row.saturating_sub(t.totals_rows), ..t.range }, columns: Vec::new() });
                filter.columns.retain(|(c, _)| *c != column);
                if !all {
                    let kept = slicer.items.iter().zip(selected).filter(|(_, s)| **s).map(|(i, _)| i.label.clone()).collect();
                    filter.columns.push((column, ColumnFilter::Values(kept)));
                }
                self.run_sheet(vec![Command::Sheet(SheetCommand::SetTableFilter { slide, index, filter: Some(filter) })], cx);
            }
            SlicerSource::Pivot { field, pivots } => {
                for name in pivots {
                    let Some(target) = locate_pivot(self.presentation(), name) else { continue };
                    let mut selected = selected.to_vec();
                    if slicer.descending {
                        selected.reverse();
                    }
                    let field = field.clone();
                    self.change_pivot(target, move |p| {
                        let Some(f) = p.fields.iter_mut().find(|f| f.name == field) else { return };
                        for (it, keep) in f.items.iter_mut().filter(|it| it.kind.is_none()).zip(selected) {
                            it.hidden = !keep;
                        }
                    }, cx);
                }
            }
        }
    }
}
