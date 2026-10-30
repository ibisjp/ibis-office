// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A check box row for the side panels: the mark, then the caption, which
//! wraps over several lines when it is long (a button's label is cut with
//! "…" instead).
use gpui_kit::*;

pub(crate) fn toggle_row(id: impl Into<ElementId>, on: bool, label: impl Into<SharedString>, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .px(px(6.))
        .py(px(3.))
        .rounded_md()
        .cursor_pointer()
        .hover(|s| s.bg(crate::shell::palette::shade(0.0, 0.05)))
        .on_click(on_click)
        .child(crate::shell::chrome::checkbox_box(on))
        .child(div().flex_1().min_w_0().text_size(px(13.)).child(label.into()))
}
