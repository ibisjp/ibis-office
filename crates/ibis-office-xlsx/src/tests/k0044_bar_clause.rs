// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0044: the formula bar draws the clause being converted thick, the others thin, as the cell does; moving to the next clause moves the thick line.
use super::SheetSteps;
use crate::shell::format::with_format;
use crate::shell::scenario::Scenario;
use crate::ext::*;
use gpui_kit::{ImeClause, ImeLine};
use ibis_doc_model::UnderlineStyle;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::k0044_the_formula_bar_thick_line_follows_the_current_clause", the_bar_thick_line_follows_the_current_clause),
        ("xlsx::k0020_the_formula_bar_thick_line_is_thicker_than_the_thin_one_and_clear_of_the_border", the_bar_thick_line_in_pixels),
        ("xlsx::k0044_the_underlines_stay_inside_the_bars_clip_for_any_typeface", the_underlines_stay_inside_the_clip),
    ]
}

fn the_bar_thick_line_follows_the_current_clause() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("K20");
    s.ime_compose("\u{3042}\u{3044}\u{3046}\u{3048}");
    for (first_thick, shot) in [(true, "bar-clause-1"), (false, "bar-clause-2")] {
        let clauses = vec![ImeClause { range: 0..2, thick: first_thick, line: ImeLine::Solid }, ImeClause { range: 2..4, thick: !first_thick, line: ImeLine::Solid }];
        s.update(|e, w, cx| with_format!(e, set_ime_clauses(e, clauses, w, cx)));
        s.settle();
        // What the formula bar is given to draw: the line of the text with each clause's underline.
        let lines = s.with(|e, _| e.cell_edit().map(|c| c.bar_line(0, &c.text).marked_underlines));
        let thick: Vec<_> = lines.unwrap().into_iter().filter(|(st, _)| *st == UnderlineStyle::Thick).map(|(_, r)| r).collect();
        let expected = if first_thick { 0..6 } else { 6..12 };
        assert_eq!(thick, vec![expected], "{shot}");
        s.screenshot(shot);
    }
}

/// K0020: in pixels, the line under the clause being converted is thicker than the others' in the formula bar (as the cell's is), and
/// no line touches the bar's frame. The same text is drawn twice, the thick clause first and then second: the pixels that differ are the
/// extra rows of the thick line (the lines are under the text, at the same place in both).
fn the_bar_thick_line_in_pixels() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("K20");
    s.ime_compose("\u{3042}\u{3044}\u{3046}\u{3048}");
    let mut shots = Vec::new();
    for first_thick in [true, false] {
        let clauses = vec![ImeClause { range: 0..2, thick: first_thick, line: ImeLine::Solid }, ImeClause { range: 2..4, thick: !first_thick, line: ImeLine::Solid }];
        s.update(|e, w, cx| with_format!(e, set_ime_clauses(e, clauses, w, cx)));
        s.settle();
        let shot = s.screenshot(if first_thick { "bar-thick-first" } else { "bar-thick-second" });
        shots.push(image::open(shot).unwrap().to_rgba8());
    }
    let bar = s.with(|e, _| e.sheet_state().and_then(|st| st.bar_bounds.get())).expect("the bar's text is painted");
    let (a, b) = (&shots[0], &shots[1]);
    let scale = a.width() as f32 / 1200.;
    let dark = |i: &image::RgbaImage, x: u32, y: u32| { let p = i.get_pixel(x, y).0; (p[0] as u32 + p[1] as u32 + p[2] as u32) < 350 };
    let (x0, x1) = ((f32::from(bar.origin.x) * scale) as u32, (f32::from(bar.origin.x + bar.size.width) * scale) as u32);
    let (y0, y1) = ((f32::from(bar.origin.y) * scale) as u32, (f32::from(bar.origin.y + bar.size.height) * scale) as u32 + 40);
    // Where the two pictures differ: only the lines (and nothing of the text).
    let differing: Vec<(u32, u32)> = (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| dark(a, x, y) != dark(b, x, y)).collect();
    assert!(!differing.is_empty(), "the thick and the thin lines differ");
    let (left, right) = (differing.iter().map(|p| p.0).min().unwrap(), differing.iter().map(|p| p.0).max().unwrap());
    let mid = (left + right) / 2;
    // The frame under the bar's text: a row of blue (the clip is its inner edge).
    let frame = (y0..y1).find(|&y| { let p = a.get_pixel(x0 + 20, y).0; p[2] > 120 && p[0] < 90 && dark(a, x0 + 20, y - 1) == false && y > y0 + 20 }).expect("the bar's frame");
    // The rows of lines (dark for 8 pixels or more) in the 4.5 logical pixels above the frame, in the part [from, to) of a picture: under the text.
    let rows = |i: &image::RgbaImage, from: u32, to: u32| -> Vec<u32> {
        (frame - (4.5 * scale) as u32..frame).filter(|&y| (from..to).filter(|&x| dark(i, x, y)).count() >= 8).collect()
    };
    let (thick, thin) = (rows(a, left, mid), rows(a, mid, right + 1));
    let (thick_b, thin_b) = (rows(b, mid, right + 1), rows(b, left, mid));
    assert_eq!((thick.clone(), thin.clone()).0.len(), thick_b.len(), "the thick line is the same in both pictures");
    // Not cut off by the clip (the bar's text is `overflow_hidden`): the thick line shows its whole thickness, twice the thin line's
    // (the thick line was cut to the thin one's width in the bar: K0020).
    assert!(thin.len() >= scale as usize && thin.len() == thin_b.len(), "the thin line: {thin:?}");
    assert_eq!(thick.len(), 2 * thin.len(), "the thick line is twice as thick as the thin one and is not cut: {thick:?} against {thin:?}");
    for i in [a, b] {
        assert!((x0..x1).filter(|&x| dark(i, x, frame - 1)).count() < 8, "a line touches the frame at row {frame}");
    }
}

/// K0044 (Windows: no underline seen in the formula bar): every underline, the thick and the dotted ones too, ends inside the clip of the
/// bar's text whatever the typeface's ascent and descent are (Meiryo UI, Yu Gothic UI, Segoe UI, Hiragino Sans: their `hhea`/win metrics in ems).
fn the_underlines_stay_inside_the_clip() {
    use ibis_doc_model::UnderlineStyle as U;
    let (clip, row_top, size_px) = (18.0f32, 2.0f32, 14.0f32);
    let k = 4.0 / 3.0;
    for (font, ascent_em) in [("Meiryo UI", 1.060f32), ("Yu Gothic UI", 1.079), ("Segoe UI", 1.079), ("Hiragino Sans", 0.88)] {
        let natural = row_top + ascent_em * size_px;
        let baseline = crate::render::ime_text::underline_baseline_within(natural, clip, size_px / k, k);
        assert!(baseline <= natural + 1e-3, "{font}: never lower than the typeface's own baseline");
        for style in [U::Single, U::Thick, U::Dotted, U::DottedHeavy, U::Wave, U::WavyHeavy] {
            let (boxes, strokes) = crate::render::ime_text::underline_shapes(style, size_px / k, 0.0, 1.0, baseline / k);
            let bottom = boxes.iter().map(|[_, y, _, h]| (y + h) * k).chain(strokes.iter().map(|[_, y, _, y2, w]| (y.max(*y2) + w / 2.0) * k)).fold(0.0f32, f32::max);
            assert!(bottom <= clip - 1.0 + 1e-3, "{font} {style:?}: the line ends at {bottom} px, inside the clip at {clip}");
        }
    }
}
