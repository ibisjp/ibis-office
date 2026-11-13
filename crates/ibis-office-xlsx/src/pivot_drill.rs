// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The +/− buttons of a pivot table (ドリル ボタン): beside the label of a row (column) item that has fields under it,
//! a click folds the item (−, `PivotItem.ShowDetail` false: its row holds the group's values) or unfolds it (+). The state
//! is the item's own (`sd`), kept by its name when the table is built again, one step to undo.

use gpui_kit::{Context, IntoElement as _, ParentElement as _, SharedString, Styled as _, div, px};
use ibis_doc_model::pivot::PivotLabel;
use ibis_doc_model::{EMU_PER_POINT, PivotTable, SlideId};

use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;
use crate::ext::*;

/// The button's side in points (Excel draws a 9-pixel box) and its distance from the label column's left, plus a
/// level's indent (`PivotTable::label_pads` keeps 12 points of room for it).
const SIDE: f32 = 7.5;
const MARGIN: f32 = 2.5;

/// Where a label's +/− button is in the window, as (left, top, side) in pixels, if the label's cell is on screen.
pub(crate) fn drill_button_rect(g: &super::headers::HeaderGeometry, l: &PivotLabel) -> Option<(f32, f32, f32)> {
    let per_point = g.scale * EMU_PER_POINT as f32;
    let &(_, top, bottom) = g.rows.iter().find(|r| r.0 == l.row)?;
    let &(_, left, _) = g.columns.iter().find(|c| c.0 == l.column)?;
    let side = (SIDE * per_point).round().max(7.0);
    let x = left + (MARGIN + 13.0 * l.indent as f32) * per_point;
    let y = top + ((bottom - top - side) / 2.0).max(0.0);
    Some((x, y, side))
}

#[ibis_office_macros::editor_ext(PivotDrill)]
impl SlideEditor {
    /// The +/− buttons of the current sheet's pivot tables, over the cells of their labels.
    pub(crate) fn pivot_drill_buttons(&self, cx: &mut Context<Self>) -> Vec<gpui_kit::AnyElement> {
        use gpui_kit::{InteractiveElement as _, MouseButton, MouseDownEvent, rgb};
        let Some((_, sheet)) = self.current_sheet() else { return Vec::new() };
        let Some(g) = self.sheet_state().map(|s| s.header_geometry.borrow().clone()) else { return Vec::new() };
        let (ox, oy) = g.origin;
        let slide = self.current_slide;
        let mut out = Vec::new();
        for (index, p) in sheet.pivots.iter().enumerate() {
            for l in p.drill_buttons() {
                let Some((wx, wy, side)) = drill_button_rect(&g, &l) else { continue };
                let collapsed = p.fields.get(l.field).and_then(|f| f.items.get(l.item as usize)).is_some_and(|i| i.collapsed);
                let (x, y) = (wx - ox, wy - oy);
                let bar = (side * 0.6).round().max(3.0);
                let mark = rgb(0x404040);
                let mut button = div()
                    .id(SharedString::from(format!("pivot-drill-{index}-{}-{}", l.row, l.column)))
                    .absolute()
                    .left(px(x.round()))
                    .top(px(y.round()))
                    .size(px(side))
                    .bg(rgb(0xFFFFFF))
                    .border_1()
                    .border_color(rgb(0x8C8C8C))
                    .cursor_pointer()
                    .hover(|s| s.border_color(rgb(0x217346)))
                    .child(div().absolute().left(px(((side - 2.0 - bar) / 2.0).floor())).top(px(((side - 2.0) / 2.0).floor())).w(px(bar)).h(px(1.0)).bg(mark));
                if collapsed {
                    button = button.child(div().absolute().left(px(((side - 2.0) / 2.0).floor())).top(px(((side - 2.0 - bar) / 2.0).floor())).w(px(1.0)).h(px(bar)).bg(mark));
                }
                let (field, item) = (l.field, l.item);
                out.push(
                    button
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.toggle_pivot_item((slide, index), field, item, cx);
                        }))
                        .into_any_element(),
                );
            }
        }
        out
    }

    /// Fold or unfold one item of a field (the − / + button): the table built again, one step to undo.
    pub(crate) fn toggle_pivot_item(&mut self, target: (SlideId, usize), field: usize, item: u32, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        self.change_pivot(
            target,
            move |p| {
                let now = p.fields.get(field).and_then(|f| f.items.get(item as usize)).is_some_and(|i| i.collapsed);
                p.set_item_collapsed(field, item, !now);
            },
            cx,
        );
    }

    /// The label under the selected cell, with its pivot table: where the 展開/折りたたみ commands work.
    pub(crate) fn pivot_label_at_selection(&self) -> Option<(usize, PivotLabel)> {
        let (_, sheet) = self.current_sheet()?;
        let (column, row) = self.current_cell()?;
        sheet.pivots.iter().enumerate().find_map(|(i, p)| p.labels().into_iter().find(|l| l.row == row && l.column == column).map(|l| (i, l)))
    }

    /// 項目の展開 / 折りたたみ (`PivotItem.ShowDetail`) for the item in the selected cell.
    pub(crate) fn set_pivot_selected_item_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        let Some((index, l)) = self.pivot_label_at_selection().filter(|(_, l)| l.has_fields_under) else {
            self.status = Some(text("xlsx.pivot-select-an-item").into());
            return cx.notify();
        };
        self.commit_cell_edit(cx);
        let (field, item) = (l.field, l.item);
        self.change_pivot((self.current_slide, index), move |p| p.set_item_collapsed(field, item, collapsed), cx);
    }

    /// フィールドの展開 / 折りたたみ (`PivotField.ShowDetail`) for the field of the item in the selected cell.
    pub(crate) fn set_pivot_selected_field_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        let Some((index, l)) = self.pivot_label_at_selection().filter(|(_, l)| l.has_fields_under) else {
            self.status = Some(text("xlsx.pivot-select-an-item").into());
            return cx.notify();
        };
        self.commit_cell_edit(cx);
        let field = l.field;
        self.change_pivot((self.current_slide, index), move |p| p.set_field_collapsed(field, collapsed), cx);
    }

    /// The 展開/折りたたみ submenu of the right-click menu on a pivot table's label.
    pub(crate) fn pivot_expand_collapse_menu(&self) -> Option<MenuEntry> {
        let (_, l) = self.pivot_label_at_selection().filter(|(_, l)| l.has_fields_under)?;
        let folded = self.current_sheet().and_then(|(_, s)| s.pivots.iter().find_map(|p: &PivotTable| p.fields.get(l.field).and_then(|f| f.items.get(l.item as usize)))).is_some_and(|i| i.collapsed);
        Some(
            Submenu::new(
                "xlsx.cell-pivot-expand-collapse",
                "table_rows",
                text("xlsx.pivot-expand-collapse"),
                vec![
                    MenuItem::new("xlsx.cell-pivot-expand", text("xlsx.pivot-expand")).disabled(!folded).on(|this, window, cx| {
                        this.set_pivot_selected_item_collapsed(false, cx);
                        this.focus_handle.focus(window, cx);
                    }).into(),
                    MenuItem::new("xlsx.cell-pivot-collapse", text("xlsx.pivot-collapse")).disabled(folded).on(|this, window, cx| {
                        this.set_pivot_selected_item_collapsed(true, cx);
                        this.focus_handle.focus(window, cx);
                    }).into(),
                    MenuItem::new("xlsx.cell-pivot-expand-field", text("xlsx.pivot-expand-field")).on(|this, window, cx| {
                        this.set_pivot_selected_field_collapsed(false, cx);
                        this.focus_handle.focus(window, cx);
                    }).into(),
                    MenuItem::new("xlsx.cell-pivot-collapse-field", text("xlsx.pivot-collapse-field")).on(|this, window, cx| {
                        this.set_pivot_selected_field_collapsed(true, cx);
                        this.focus_handle.focus(window, cx);
                    }).into(),
                ],
            )
            .into(),
        )
    }
}
