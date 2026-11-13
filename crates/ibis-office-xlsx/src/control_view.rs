// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing a form control (フォーム コントロール): a button, a check box, an option button, a scroll bar, a spin
//! button, a drop-down list, a list box, a group box or a label, as Excel draws them on a sheet.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::form_control::{LINE, thumb};
use ibis_doc_model::{ControlKind, EMU_PER_POINT, EmuRect, FormControl};

const GREY: u32 = 0x808080;
const DARK_SHADOW: u32 = 0x696969;
const HIGHLIGHT: u32 = 0xFFFFFF;
const FACE: u32 = 0xF0F0F0;

pub fn control_element(c: &FormControl, frame: EmuRect, scale: f32) -> Div {
    let pt = |emu: i64| emu as f32 * scale;
    let font_pt = c.font_size.map(|sz| (sz as f32 / 100.0).max(1.0)).unwrap_or(11.0);
    let font = px((font_pt * EMU_PER_POINT as f32 * scale).max(1.0));
    let fg = c.text_color.map(|col| col.0).unwrap_or(0x000000);
    let (w, h) = (frame.width, frame.height);
    let root = div()
        .absolute()
        .left_0()
        .top_0()
        .w(px(pt(w)))
        .h(px(pt(h)))
        .overflow_hidden()
        .text_size(font)
        .text_color(rgb(fg))
        .when_some(c.font.clone(), |d, f| d.font_family(f))
        .when(c.bold, |d| d.font_weight(FontWeight::BOLD))
        .when(c.italic, |d| d.italic());
    let at = |x: i64, y: i64, cw: i64, ch: i64| div().absolute().left(px(pt(x))).top(px(pt(y))).w(px(pt(cw))).h(px(pt(ch)));
    let edge = (EMU_PER_POINT).max(1);
    let bevel_raised = |d: Div, cw: i64, ch: i64| {
        if cw <= 2 * edge || ch <= 2 * edge {
            return d;
        }
        d.child(at(0, 0, cw, edge).bg(rgb(HIGHLIGHT)))
            .child(at(0, 0, edge, ch).bg(rgb(HIGHLIGHT)))
            .child(at(0, ch - edge, cw, edge).bg(rgb(DARK_SHADOW)))
            .child(at(cw - edge, 0, edge, ch).bg(rgb(DARK_SHADOW)))
    };
    let bevel_sunken = |d: Div, cw: i64, ch: i64| {
        if cw <= 2 * edge || ch <= 2 * edge {
            return d;
        }
        d.child(at(0, 0, cw, edge).bg(rgb(DARK_SHADOW)))
            .child(at(0, 0, edge, ch).bg(rgb(DARK_SHADOW)))
            .child(at(0, ch - edge, cw, edge).bg(rgb(HIGHLIGHT)))
            .child(at(cw - edge, 0, edge, ch).bg(rgb(HIGHLIGHT)))
    };
    let arrow = |x: i64, y: i64, cw: i64, ch: i64, glyph: &'static str| {
        bevel_raised(
            at(x, y, cw, ch)
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(FACE))
                .border_1()
                .border_color(rgb(0xA0A0A0))
                .text_color(rgb(0x404040))
                .text_size(px((8.0 * EMU_PER_POINT as f32 * scale).max(1.0)))
                .child(glyph),
            cw,
            ch,
        )
    };
    match c.kind {
        ControlKind::Button => {
            let btn = root
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(FACE))
                .border_1()
                .border_color(rgb(GREY))
                .rounded(px(pt(30_000)))
                .child(c.text.clone());
            if c.three_d { bevel_raised(btn, w, h) } else { btn }
        }
        ControlKind::CheckBox | ControlKind::Radio => {
            let side = (13 * EMU_PER_POINT).min(h);
            let top = (h - side) / 2;
            let mark = match (c.kind, c.checked) {
                (ControlKind::CheckBox, 1) => "✓",
                (ControlKind::CheckBox, 2) => "■",
                (ControlKind::Radio, 1) => "●",
                _ => "",
            };
            let radio = c.kind == ControlKind::Radio;
            let box_bg = if c.three_d && c.checked == 2 { 0xE0E0E0 } else { 0xFFFFFF };
            let box_border = if c.three_d { DARK_SHADOW } else { GREY };
            let mut well = at(0, top, side, side)
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(box_bg))
                .border_1()
                .border_color(rgb(box_border))
                .when(radio, |d| d.rounded(px(pt(side))))
                .text_color(rgb(0x000000))
                .text_size(px((10.0 * EMU_PER_POINT as f32 * scale).max(1.0)))
                .child(mark);
            if c.three_d && !radio {
                well = bevel_sunken(well, side, side);
            }
            root.child(well)
                .child(at(side + 3 * EMU_PER_POINT, 0, (w - side - 3 * EMU_PER_POINT).max(0), h).flex().items_center().child(c.text.clone()))
        }
        ControlKind::Scroll => {
            let (length, across) = if c.horizontal { (w, h) } else { (h, w) };
            let square = across.min(length / 2).max(1);
            let (start, size) = thumb(c, length, square);
            let glyphs = if c.horizontal { ("◀", "▶") } else { ("▲", "▼") };
            let place = |along: i64, extent: i64, glyph: Option<&'static str>| {
                let (x, y, cw, ch) = if c.horizontal { (along, 0, extent, h) } else { (0, along, w, extent) };
                match glyph {
                    Some(g) => arrow(x, y, cw, ch, g),
                    None => bevel_raised(at(x, y, cw, ch).bg(rgb(FACE)).border_1().border_color(rgb(0xA0A0A0)), cw, ch),
                }
            };
            root.bg(rgb(0xE8E8E8)).child(place(0, square, Some(glyphs.0))).child(place(length - square, square, Some(glyphs.1))).child(place(start, size, None))
        }
        ControlKind::Spin => root.child(arrow(0, 0, w, h / 2, "▲")).child(arrow(0, h / 2, w, h - h / 2, "▼")),
        ControlKind::Drop => {
            let shown = usize::try_from(c.value - 1).ok().and_then(|i| c.items.get(i)).cloned().unwrap_or_default();
            let button = h.min(16 * EMU_PER_POINT);
            let drop_box = root
                .bg(rgb(0xFFFFFF))
                .border_1()
                .border_color(rgb(if c.three_d { DARK_SHADOW } else { GREY }))
                .child(at(3 * EMU_PER_POINT, 0, (w - button - 3 * EMU_PER_POINT).max(0), h).flex().items_center().child(shown))
                .child(arrow((w - button).max(0), 0, button, h, "▼"));
            if c.three_d { bevel_sunken(drop_box, w, h) } else { drop_box }
        }
        ControlKind::List => {
            let mut list = root
                .bg(rgb(0xFFFFFF))
                .border_1()
                .border_color(rgb(if c.three_d { DARK_SHADOW } else { GREY }));
            for (i, item) in c.items.iter().enumerate() {
                let y = i as i64 * LINE;
                if y >= h {
                    break;
                }
                let chosen = c.value == i as i64 + 1;
                list = list.child(at(0, y, w, LINE).flex().items_center().px(px(pt(3 * EMU_PER_POINT))).when(chosen, |d| d.bg(rgb(0x3875D7)).text_color(rgb(0xFFFFFF))).child(item.clone()));
            }
            if c.three_d { bevel_sunken(list, w, h) } else { list }
        }
        ControlKind::GroupBox => {
            let top = 6 * EMU_PER_POINT;
            let frame_h = (h - top).max(0);
            let mut box_frame = at(0, top, w, frame_h)
                .border_1()
                .border_color(rgb(if c.three_d { DARK_SHADOW } else { 0xB0B0B0 }))
                .rounded(px(pt(30_000)));
            if c.three_d && w > 2 * edge && frame_h > 2 * edge {
                box_frame = box_frame.child(at(edge, edge, w - 2 * edge, frame_h - 2 * edge).border_1().border_color(rgb(HIGHLIGHT)).rounded(px(pt(30_000))));
            }
            root.child(box_frame)
                .child(at(8 * EMU_PER_POINT, 0, w.saturating_sub(16 * EMU_PER_POINT), 12 * EMU_PER_POINT).child(div().bg(rgb(0xFFFFFF)).px(px(pt(2 * EMU_PER_POINT))).child(c.text.clone())))
        }
        ControlKind::Label => root.flex().items_center().child(c.text.clone()),
    }
}
