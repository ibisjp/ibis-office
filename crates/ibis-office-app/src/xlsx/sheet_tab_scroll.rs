// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

pub(super) const TAB_BUTTON_WIDTH: f32 = 28.;

/// Child bounds are unscrolled layout coordinates; GPUI offsets are negative.
pub(super) fn reveal_offset(view_left: f32, view_width: f32, tab_left: f32, tab_width: f32, offset: f32, max_offset: f32) -> f32 {
    if view_width <= 0. { return offset; }
    let tab_right = tab_left + tab_width;
    let view_right = view_left + view_width;
    if tab_left + offset >= view_left && tab_right + offset <= view_right { return offset; }
    // Keep the whole selected tab visible before reserving a neighbour preview.
    // An oversized tab, or one leaving no room for that preview, starts at left.
    let target = if tab_width + TAB_BUTTON_WIDTH >= view_width {
        view_left - tab_left
    } else if tab_right + offset > view_right {
        view_right - TAB_BUTTON_WIDTH - tab_right
    } else {
        view_left + TAB_BUTTON_WIDTH - tab_left
    };
    target.clamp(-max_offset.max(0.), 0.)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn right_clipping_leaves_a_button_width_of_the_next_tab() { assert_eq!(reveal_offset(85.,300.,325.,90.,0.,500.),-58.); }
    #[test] fn left_clipping_leaves_a_button_width_of_the_previous_tab() { assert_eq!(reveal_offset(85.,300.,205.,90.,-150.,500.),-92.); }
    #[test] fn visible_tab_does_not_move_the_strip() { assert_eq!(reveal_offset(85.,300.,205.,90.,-50.,500.),-50.); }
    #[test] fn oversized_tab_aligns_its_left_edge() { assert_eq!(reveal_offset(85.,200.,325.,400.,0.,700.),-240.); }
    #[test] fn nearly_full_width_tab_prioritizes_its_left_edge() { assert_eq!(reveal_offset(85.,200.,325.,185.,0.,700.),-240.); }
    #[test] fn last_tab_respects_the_content_end() { assert_eq!(reveal_offset(85.,300.,485.,100.,0.,200.),-200.); }
    #[test] fn first_tab_respects_the_content_start() { assert_eq!(reveal_offset(85.,300.,85.,100.,-40.,200.),0.); }
}
