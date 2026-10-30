// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing a slicer (スライサー): its heading and a grid of buttons, one for
//! each item; the ones the filter keeps are filled. The colours follow the
//! slicer's style (`SlicerStyleLight1` … `Dark6`, `Other1`, `Other2`) with
//! Office's accent colours (the sheet's theme is not looked up).

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::slicer::{Slicer, layout, PAD};
use ibis_doc_model::{EMU_PER_POINT, EmuRect};

/// The colour (accent) of a style: Office's accent 1–6, from the style's number.
pub(crate) fn accent(style: &str) -> u32 {
    const ACCENTS: [u32; 6] = [0x4472C4, 0xED7D31, 0xA5A5A5, 0xFFC000, 0x5B9BD5, 0x70AD47];
    let n = style.chars().last().and_then(|c| c.to_digit(10)).unwrap_or(1) as usize;
    if style.starts_with("SlicerStyleOther") {
        return if n == 2 { 0x7F7F7F } else { 0x4472C4 };
    }
    ACCENTS[(n.clamp(1, 6)) - 1]
}

/// Mix `c` with white: `t` of white.
fn tint(c: u32, t: f32) -> u32 {
    let mix = |v: u32| (v as f32 + (255.0 - v as f32) * t).round() as u32;
    (mix((c >> 16) & 0xff) << 16) | (mix((c >> 8) & 0xff) << 8) | mix(c & 0xff)
}

pub(crate) fn slicer_element(slicer: &Slicer, frame: EmuRect, scale: f32) -> Div {
    let pt = |emu: i64| emu as f32 * scale;
    let dark = slicer.style.starts_with("SlicerStyleDark") || slicer.style.starts_with("SlicerStyleOther");
    let accent = accent(&slicer.style);
    let l = layout(slicer, frame.width, frame.height);
    let font = px((11.0 * EMU_PER_POINT as f32 * scale).max(1.0));
    let mut root = div()
        .absolute()
        .left_0()
        .top_0()
        .w(px(pt(frame.width)))
        .h(px(pt(frame.height)))
        .overflow_hidden()
        .bg(rgb(if dark { 0xF2F2F2 } else { 0xFFFFFF }))
        .border_1()
        .border_color(rgb(if dark { accent } else { tint(accent, 0.4) }))
        .text_size(font)
        .text_color(rgb(0x262626));
    if l.header > 0 {
        root = root.child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .w(px(pt(frame.width)))
                .h(px(pt(l.header)))
                .flex()
                .items_center()
                .px(px(pt(PAD * 2)))
                .when(dark, |d| d.bg(rgb(accent)).text_color(rgb(if accent == 0xFFC000 || accent == 0xA5A5A5 { 0x262626 } else { 0xFFFFFF })))
                .font_weight(FontWeight::BOLD)
                .child(div().flex_1().child(slicer.caption.clone()))
                // The clear-filter icon: a funnel's mouth with a cross, as a ✕ on the right.
                .child(div().text_color(rgb(if slicer.items.iter().all(|i| i.selected) { 0xBFBFBF } else if dark { 0xFFFFFF } else { 0x595959 })).child("✕")),
        );
    }
    for (i, item) in slicer.items.iter().enumerate() {
        let (col, row) = (i as i64 % l.columns as i64, i as i64 / l.columns as i64);
        let (x, y) = (l.body.0 + col * (l.button_width + l.gap), l.body.1 + row * (l.button_height + l.gap));
        if y + l.button_height > frame.height {
            break;
        }
        let (fill, text, border) = match (item.selected, item.has_data) {
            (true, true) => (if dark { accent } else { tint(accent, 0.6) }, if dark && accent != 0xFFC000 && accent != 0xA5A5A5 { 0xFFFFFF } else { 0x262626 }, accent),
            (true, false) => (tint(accent, 0.85), 0x8C8C8C, tint(accent, 0.5)),
            (false, true) => (if dark { 0xE7E6E6 } else { 0xFFFFFF }, 0x262626, 0xBFBFBF),
            (false, false) => (if dark { 0xF2F2F2 } else { 0xFFFFFF }, 0xB0B0B0, 0xD9D9D9),
        };
        root = root.child(
            div()
                .absolute()
                .left(px(pt(x)))
                .top(px(pt(y)))
                .w(px(pt(l.button_width)))
                .h(px(pt(l.button_height)))
                .overflow_hidden()
                .flex()
                .items_center()
                .px(px(pt(PAD)))
                .bg(rgb(fill))
                .border_1()
                .border_color(rgb(border))
                .text_color(rgb(text))
                .child(item.label.clone()),
        );
    }
    root
}
