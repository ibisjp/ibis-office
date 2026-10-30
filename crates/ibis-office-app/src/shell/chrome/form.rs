// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Shared flowing forms and tables for panes and dialogs.
use super::*;

pub(crate) const FORM_GAP: f32 = 8.;
pub(crate) const FORM_PADDING: f32 = 12.;

pub(crate) fn panel_form() -> Div {
    // The dialog/pane host supplies FORM_PADDING exactly once.
    v_flex().gap(px(FORM_GAP)).text_sm()
}

/// An absent message contributes neither a line box nor a flex gap.
pub(crate) fn panel_message(message: &str) -> Option<Div> {
    (!message.is_empty()).then(|| {
        div()
            .text_sm()
            .child(SharedString::from(message.to_string()))
    })
}

/// Matching cells in the header and body use the same flex sizing. The final cell holds row actions.
pub(crate) fn table_row(cells: Vec<AnyElement>, actions: AnyElement, selected: bool) -> Div {
    table_row_with_actions(cells,actions,selected,px(80.))
}
pub(crate) fn table_row_with_actions(cells:Vec<AnyElement>,actions:AnyElement,selected:bool,actions_width:Pixels)->Div {
    h_flex()
        .w_full()
        .flex_none()
        .items_center()
        .gap_2()
        .px_2()
        .h(px(super::data_table::TABLE_ROW_HEIGHT))
        .min_h(px(super::data_table::TABLE_ROW_HEIGHT))
        .py_0()
        .bg(rgb(if selected { SELECTED_ROW_BG } else { 0xffffff }))
        .border_b_1()
        .border_color(rgb(BORDER))
        .children(
            cells
                .into_iter()
                .map(|cell| div().flex_1().min_w_0().overflow_hidden().child(cell)),
        )
        .child(div().w(actions_width).flex_none().child(actions))
}

/// Fixed header and an independently scrolling body, shared by all document formats.
pub(crate) fn table(
    id: &'static str,
    header: AnyElement,
    rows: AnyElement,
    scroll: ScrollHandle,
    height: Pixels,
) -> Div {
    table_with_height(id, header, rows, scroll, Some(height))
}

/// Fill the remaining finite form height; only data rows scroll, never the header.
pub(crate) fn table_fill(id: &'static str, header: AnyElement, rows: AnyElement, scroll: ScrollHandle) -> Div {
    table_with_height(id, header, rows, scroll, None)
}
fn table_with_height(id: &'static str, header: AnyElement, rows: AnyElement, scroll: ScrollHandle, height: Option<Pixels>) -> Div {
    super::data_table::table_surface().min_h_0().when(height.is_none(), |d| d.flex_1())
        .child(div().flex_none().bg(rgb(0xf3f4f6)).child(header))
        .child(
            div()
                .relative()
                .when_some(height, |d,h| d.h(h)).when(height.is_none(), |d| d.flex_1())
                .min_h_0()
                .child(
                    div()
                        .id(id)
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&scroll)
                        .child(rows),
                )
                .child(always_visible_scrollbar_overlay(
                    SharedString::from(format!("{id}-scrollbar")),
                    scroll,
                    None,
                )),
        )
}

/// Headerless, compact lists share the same independent scrolling table surface.
pub(crate) fn table_list(id:&'static str,rows:AnyElement,scroll:ScrollHandle)->Div {
    super::data_table::table_surface().flex_1().min_h_0().child(div().id(id).flex_1().min_h_0().overflow_y_scroll().track_scroll(&scroll).child(rows))
}
