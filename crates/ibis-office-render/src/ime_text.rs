// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The underline of text being composed by an input method (GitHub issue #138): the one place that turns what the platform says about each
//! clause (`gpui_kit::ImeClause`: thick or thin, solid, dotted or wavy) into the line every text input draws. Word with the Microsoft IME
//! draws the part still being typed dotted, the converted clauses thin and the one being converted thick; the platform layers map the
//! attributes (`ATTR_INPUT`, `ATTR_TARGET_CONVERTED`, … on Windows, `NSUnderline` on macOS) as `ImeClause` documents.
//!
//! Every surface that shows composing text uses this: text in a line of the page (`paint_line`, `paint_column`), text in a text box, a shape
//! or a table cell (`ibis_doc_model::Underline` of each clause, `run_underline`), a cell being edited and the formula bar
//! (`paint_line` over the shaped text).

use std::ops::Range;

use gpui_kit::*;
use ibis_doc_model::UnderlineStyle;

/// A run of the composing text (characters, not UTF-16 units) with one underline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImeSpan {
    pub range: Range<usize>,
    pub thick: bool,
    pub line: ImeLine,
}

impl ImeSpan {
    /// The underline of the page's text that draws it (the same lines, thickness and dashes as the committed text's).
    pub fn style(&self) -> UnderlineStyle {
        match (self.thick, self.line) {
            (false, ImeLine::Solid) => UnderlineStyle::Single,
            (true, ImeLine::Solid) => UnderlineStyle::Thick,
            (false, ImeLine::Dotted) => UnderlineStyle::Dotted,
            (true, ImeLine::Dotted) => UnderlineStyle::DottedHeavy,
            (false, ImeLine::Wavy) => UnderlineStyle::Wave,
            (true, ImeLine::Wavy) => UnderlineStyle::WavyHeavy,
        }
    }
}

/// The spans of `text` (the composing text) from the clauses the platform gave (UTF-16 units). Characters no clause covers, and all of
/// them when the platform said nothing (Linux, the web), get a thin solid line, as composing text always had.
pub fn spans(text: &str, clauses: &[ImeClause]) -> Vec<ImeSpan> {
    let count = text.chars().count();
    let mut units = Vec::with_capacity(count + 1);
    let mut u = 0;
    for c in text.chars() {
        units.push(u);
        u += c.len_utf16();
    }
    units.push(u);
    // The index of the first character at or after UTF-16 offset `u`.
    let to_char = |u: usize| units.partition_point(|&x| x < u);
    let mut out: Vec<ImeSpan> = Vec::new();
    let mut at = 0;
    let push = |range: Range<usize>, thick: bool, line: ImeLine, out: &mut Vec<ImeSpan>| {
        if range.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(last) if last.thick == thick && last.line == line && last.range.end == range.start => last.range.end = range.end,
            _ => out.push(ImeSpan { range, thick, line }),
        }
    };
    let mut sorted: Vec<&ImeClause> = clauses.iter().collect();
    sorted.sort_by_key(|c| c.range.start);
    for c in sorted {
        let (a, b) = (to_char(c.range.start).max(at), to_char(c.range.end).min(count));
        if a >= b {
            continue;
        }
        push(at..a, false, ImeLine::Solid, &mut out);
        push(a..b, c.thick, c.line, &mut out);
        at = b;
    }
    push(at..count, false, ImeLine::Solid, &mut out);
    out
}

/// The underline of character `index` of the composing text.
pub(crate) fn span_at(spans: &[ImeSpan], index: usize) -> Option<&ImeSpan> {
    spans.iter().find(|s| s.range.contains(&index))
}

/// The underline of a composing run in a text body (`TextBody` runs: a text box, a shape, a cell), for `ibis_doc_model::CharacterFormat`.
pub fn run_underline(span: &ImeSpan) -> ibis_doc_model::Underline {
    span.style().to_drawing_underline()
}

/// The shapes of the underline of `style` under a run of text `size_pt` big, `width_pt` long from `x_pt`, whose baseline is
/// `baseline_pt` below the top: what the page draws for committed text (`flow_layout::underline_items`, the document's, so asked of
/// it through `shape_view::format_hooks`), as (x, y, w, h) boxes and (x, y, x2, y2, w) strokes in points.
pub fn underline_shapes(style: UnderlineStyle, size_pt: f32, x_pt: f32, width_pt: f32, baseline_pt: f32) -> (Vec<[f32; 4]>, Vec<[f32; 5]>) {
    (crate::shape_view::format_hooks::hooks().underline_shapes)(style, size_pt, x_pt, width_pt, baseline_pt)
}

/// Draw those shapes with `k` pixels to a point, the point (0, 0) at `origin`, in `color`.
pub(crate) fn paint_shapes(window: &mut Window, origin: Point<Pixels>, k: f32, shapes: &(Vec<[f32; 4]>, Vec<[f32; 5]>), color: Hsla) {
    for [x, y, w, h] in &shapes.0 {
        window.paint_quad(fill(Bounds::new(origin + point(px(x * k), px(y * k)), size(px(w * k), px((h * k).max(1.0)))), color));
    }
    for [x, y, x2, y2, w] in &shapes.1 {
        let mut path = PathBuilder::stroke(px((w * k).max(1.0)));
        path.move_to(origin + point(px(x * k), px(y * k)));
        path.line_to(origin + point(px(x2 * k), px(y2 * k)));
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    }
}

/// Underline the characters of a line of composing text. `cells` are (left, width, character index) in pixels from `origin` (the line's
/// top left); `baseline_px` is the baseline's distance from the top; `size_pt` the font size and `k` pixels to a point. Neighbouring
/// characters with the same underline are one run.
pub(crate) fn paint_line(window: &mut Window, origin: Point<Pixels>, cells: &[(f32, f32, usize)], spans: &[ImeSpan], size_pt: f32, k: f32, baseline_px: f32, color: Hsla) {
    for (style, from, to) in runs(cells, spans) {
        let shapes = underline_shapes(style, size_pt, from / k, (to - from) / k, baseline_px / k);
        paint_shapes(window, origin, k, &shapes, color);
    }
}

/// (underline, from, to) of the runs of `cells` (left, width, character index) that share one underline.
pub fn runs(cells: &[(f32, f32, usize)], spans: &[ImeSpan]) -> Vec<(UnderlineStyle, f32, f32)> {
    let mut out: Vec<(UnderlineStyle, f32, f32)> = Vec::new();
    for (at, width, index) in cells {
        let style = span_at(spans, *index).map_or(UnderlineStyle::Single, ImeSpan::style);
        match out.last_mut() {
            Some(last) if last.0 == style && (last.2 - at).abs() < 0.01 => last.2 = at + width,
            _ => out.push((style, *at, at + width)),
        }
    }
    out
}

/// Underline a column of composing text in vertical writing: cells along the column. The line is on the right of the characters, as
/// Word's (Microsoft IME, Windows 11, Word 16.0: tasks/doc/ime138-check-20261110/word-vertical.png), at `baseline_x_px` (the baseline
/// mirrored to the right of the column: the cell's width minus the descent) plus the underline's depth.
pub fn paint_column(window: &mut Window, origin: Point<Pixels>, cells: &[(f32, f32, usize)], spans: &[ImeSpan], size_pt: f32, k: f32, baseline_x_px: f32, color: Hsla) {
    for (style, from, to) in runs(cells, spans) {
        let (boxes, strokes) = underline_shapes(style, size_pt, from / k, (to - from) / k, 0.0);
        // Along the text (x) is down the page; below the baseline (y) is right of it; a box of height h reaches from y to y + h.
        for [x, y, w, h] in boxes {
            let left = baseline_x_px + y * k;
            window.paint_quad(fill(Bounds::new(origin + point(px(left), px(x * k)), size(px((h * k).max(1.0)), px(w * k))), color));
        }
        for [x, y, x2, y2, w] in strokes {
            let mut path = PathBuilder::stroke(px((w * k).max(1.0)));
            path.move_to(origin + point(px(baseline_x_px + y * k), px(x * k)));
            path.line_to(origin + point(px(baseline_x_px + y2 * k), px(x2 * k)));
            if let Ok(path) = path.build() {
                window.paint_path(path, color);
            }
        }
    }
}

/// One underline of `style` under text `size_pt` big, `width_px` long from `origin`'s x, its baseline `baseline_px` below `origin`'s y.
pub fn paint_span(window: &mut Window, origin: Point<Pixels>, k: f32, style: UnderlineStyle, size_pt: f32, width_px: f32, baseline_px: f32, color: Hsla) {
    let shapes = underline_shapes(style, size_pt, 0.0, width_px / k, baseline_px / k);
    paint_shapes(window, origin, k, &shapes, color);
}
