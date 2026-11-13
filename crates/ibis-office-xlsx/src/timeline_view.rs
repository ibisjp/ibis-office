// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing a timeline (タイムライン): its heading, the selection and the level of the periods, a bar of the periods
//! (their years above) with the ones kept filled, and a scroll bar under it.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::timeline::{CAPTION, CELL_WIDTH, GROUP, Period, SCROLL, SUB, Timeline, first_shown, shown_count, thumb_rect};
use ibis_doc_model::{EMU_PER_POINT, EmuRect};

use crate::shell::i18n::{format, text};

fn accent(style: &str) -> u32 {
    const ACCENTS: [u32; 6] = [0x5B9BD5, 0xED7D31, 0xA5A5A5, 0xFFC000, 0x4472C4, 0x70AD47];
    let n = style.chars().last().and_then(|c| c.to_digit(10)).unwrap_or(1) as usize;
    ACCENTS[n.clamp(1, 6) - 1]
}

fn tint(c: u32, t: f32) -> u32 {
    let mix = |v: u32| (v as f32 + (255.0 - v as f32) * t).round() as u32;
    (mix((c >> 16) & 0xff) << 16) | (mix((c >> 8) & 0xff) << 8) | mix(c & 0xff)
}

/// The label of a period at `level`.
pub(crate) fn period_label(level: u8, p: &Period) -> String {
    match level {
        0 => format("xlsx.timeline.year", &[("y", &p.year)]),
        1 => format("xlsx.timeline.quarter", &[("n", &p.index)]),
        2 => format("xlsx.timeline.month", &[("n", &p.index)]),
        _ => p.index.to_string(),
    }
}

/// The text of the selection (`すべての期間`, or the first and last period).
pub(crate) fn selection_label(t: &Timeline) -> String {
    let Some((a, b)) = t.selection else { return text("xlsx.timeline.all-periods").to_string() };
    let periods = t.periods(t.selection_level);
    let first = periods.iter().find(|p| p.start >= a);
    let last = periods.iter().rev().find(|p| p.end <= b);
    match (first, last) {
        (Some(x), Some(y)) if x == y => full_label(t.selection_level, x),
        (Some(x), Some(y)) => format("xlsx.timeline.range", &[("a", &full_label(t.selection_level, x)), ("b", &full_label(t.selection_level, y))]),
        _ => text("xlsx.timeline.all-periods").to_string(),
    }
}

fn full_label(level: u8, p: &Period) -> String {
    match level {
        0 => format("xlsx.timeline.year", &[("y", &p.year)]),
        1 => format("xlsx.timeline.year-quarter", &[("y", &p.year), ("n", &p.index)]),
        2 => format("xlsx.timeline.year-month", &[("y", &p.year), ("n", &p.index)]),
        _ => {
            let (y, m, d) = ibis_calc::eval::convert::civil_from_days(p.start - 25_569);
            format("xlsx.timeline.date", &[("y", &y), ("m", &m), ("d", &d)])
        }
    }
}

pub fn timeline_element(t: &Timeline, frame: EmuRect, scale: f32) -> Div {
    let pt = |emu: i64| emu as f32 * scale;
    let font = px((10.0 * EMU_PER_POINT as f32 * scale).max(1.0));
    let (w, h) = (frame.width, frame.height);
    let dark = t.style.starts_with("TimeSlicerStyleDark") || t.style.starts_with("SlicerStyleDark");
    let acc = accent(&t.style);
    let light = tint(acc, 0.55);
    let root = div()
        .absolute()
        .left_0()
        .top_0()
        .w(px(pt(w)))
        .h(px(pt(h)))
        .overflow_hidden()
        .bg(rgb(if dark { 0xF2F2F2 } else { 0xFFFFFF }))
        .border_1()
        .border_color(rgb(if dark { acc } else if t.style.is_empty() { 0xA6A6A6 } else { tint(acc, 0.4) }))
        .text_size(font)
        .text_color(rgb(0x262626));
    let at = |x: i64, y: i64, cw: i64, ch: i64| div().absolute().left(px(pt(x))).top(px(pt(y))).w(px(pt(cw))).h(px(pt(ch)));
    let mut root = root;
    let mut y = 0;
    if t.show_header {
        let hdr_bg = if dark { acc } else { 0xF2F2F2 };
        let hdr_fg = if dark && acc != 0xFFC000 && acc != 0xA5A5A5 { 0xFFFFFF } else { 0x262626 };
        let clr_fg = if t.selection.is_some() {
            if dark && acc != 0xFFC000 && acc != 0xA5A5A5 { 0xFFFFFF } else { 0x595959 }
        } else {
            0xBFBFBF
        };
        root = root.child(
            at(0, 0, w, CAPTION)
                .flex()
                .items_center()
                .px(px(pt(4 * EMU_PER_POINT)))
                .bg(rgb(hdr_bg))
                .text_color(rgb(hdr_fg))
                .font_weight(FontWeight::BOLD)
                .child(div().flex_1().child(t.caption.clone()))
                .child(div().text_color(rgb(clr_fg)).child("✕")),
        );
        y += CAPTION;
    }
    if t.show_selection_label || t.show_time_level {
        let level = text(["xlsx.timeline.level.years", "xlsx.timeline.level.quarters", "xlsx.timeline.level.months", "xlsx.timeline.level.days"][t.level.min(3) as usize]).to_string();
        root = root.child(at(0, y, w, SUB).flex().items_center().px(px(pt(4 * EMU_PER_POINT))).child(div().flex_1().when(t.show_selection_label, |d| d.child(selection_label(t)))).when(t.show_time_level, |d| d.child(div().text_color(rgb(0x404040)).child(format!("{level} ▾")))));
        y += SUB;
    }
    // The periods: their years above, the ones kept filled.
    let periods = t.periods(t.selection_level);
    let first = first_shown(&periods, t.scroll);
    let count = shown_count(w);
    let cell_h = h - y - GROUP - SCROLL;
    let mut last_group: Option<i64> = None;
    for (k, p) in periods.iter().skip(first).take(count).enumerate() {
        let x = SCROLL + k as i64 * CELL_WIDTH;
        if t.selection_level > 0 && last_group != Some(p.year) {
            last_group = Some(p.year);
            root = root.child(at(x, y, CELL_WIDTH * 2, GROUP).flex().items_center().px(px(pt(2 * EMU_PER_POINT))).text_color(rgb(0x595959)).child(p.year.to_string()));
        }
        let kept = t.selection.is_some() && t.selected(p);
        root = root.child(
            at(x, y + GROUP, CELL_WIDTH, cell_h)
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(if kept { light } else { 0xFFFFFF }))
                .border_1()
                .border_color(rgb(if kept { acc } else { 0xD9D9D9 }))
                .child(period_label(t.selection_level, p)),
        );
    }
    if t.selection.is_some() {
        let sel_indices: Vec<usize> = periods.iter().enumerate().filter(|(_, p)| t.selected(p)).map(|(i, _)| i).collect();
        let grip_w = 3 * EMU_PER_POINT;
        if let (Some(&fs), Some(&ls)) = (sel_indices.first(), sel_indices.last()) {
            if fs >= first && fs < first + count {
                let x0 = SCROLL + (fs - first) as i64 * CELL_WIDTH;
                root = root.child(at(x0 - grip_w / 2, y + GROUP, grip_w, cell_h).bg(rgb(acc)));
            }
            if ls >= first && ls < first + count {
                let x1 = SCROLL + (ls - first + 1) as i64 * CELL_WIDTH;
                root = root.child(at(x1 - grip_w / 2, y + GROUP, grip_w, cell_h).bg(rgb(acc)));
            }
        }
    }
    if t.show_scroll_bar {
        let arrow = |x: i64, glyph: &'static str| at(x, h - SCROLL, SCROLL, SCROLL).flex().items_center().justify_center().bg(rgb(0xF0F0F0)).border_1().border_color(rgb(0xBFBFBF)).text_size(px((7.0 * EMU_PER_POINT as f32 * scale).max(1.0))).child(glyph);
        let (tx0, tx1) = thumb_rect(t, &periods, w);
        root = root.child(arrow(0, "◀")).child(arrow(w - SCROLL, "▶")).child(at(tx0, h - SCROLL, (tx1 - tx0).max(SCROLL), SCROLL).bg(rgb(0xD0D0D0)).border_1().border_color(rgb(0xA0A0A0)));
    }
    root
}
