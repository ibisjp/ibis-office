// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The message box: a title, a message and the action bar, over the window's modal backdrop.
//!
//! A screen gives the strings and the `ActionBar` (its buttons are `ActionDefinition`s, so a
//! dangerous answer is `ActionButtonKind::Destructive`, red as everywhere); this part builds
//! the box, its padding and the separator of the bar.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::*;

use crate::shell::chrome::{ActionBar, action_bar_host};
use crate::shell::dialog::confirmation_surface;

/// The width of a message box. Wide enough for three answers in a row, in either language.
pub(crate) const MESSAGE_BOX_WIDTH: f32 = 580.;

/// A message box over the whole window: input behind it is blocked (`confirmation_surface`).
/// `backdrop` (default OFF; ON only for a very important question) dims the window behind it.
pub(crate) fn message_box(id: &'static str, title: SharedString, message: SharedString, actions: ActionBar, backdrop: bool, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    confirmation_surface(id, backdrop).child(
        v_flex()
            .id((id, 1usize))
            .w(px(MESSAGE_BOX_WIDTH))
            .max_w_full()
            .rounded_lg()
            .overflow_hidden()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .shadow_xl()
            .child(
                v_flex()
                    .p_6()
                    .gap_3()
                    .child(div().text_base().font_weight(FontWeight::SEMIBOLD).text_color(theme.foreground).child(title))
                    .child(div().text_sm().text_color(theme.muted_foreground).child(message)),
            )
            .child(action_bar_host(actions)),
    )
}
