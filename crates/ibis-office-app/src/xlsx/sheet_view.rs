// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing worksheets: cell fills, grid lines, text and borders, painted on
//! one canvas (a sheet can hold tens of thousands of cells).
//!
//! Text follows Excel's rules: numbers go to the right and text to the left
//! unless aligned otherwise, text runs on over empty cells beside it, and a
//! number too wide for its cell shows as `###`.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::*;
use ibis_doc_model::{
    BorderLine, CellAlign, CellKind, CellRange, CellStyle, CellUnderline, CellVerticalAlign, EMU_PER_POINT, EmuRect, Sheet, TextStyle, VerticalPosition,
};

use crate::render::shape_view::{Resources, hsla, polygon};

/// Space between a cell's edge and its text, in points.
const CELL_PADDING_PT: f32 = 2.0;

/// The padding each side of a cell's text in the room it has to fit in (a number that does not fit shows `#`).
/// A column's width counts 5 pixels of padding besides its characters (ECMA-376 Part 1 18.3.1.13: 2 px a
/// side and 1 for the grid line), so a text has the column less 5 px: 2.5 px = 1.875 pt a side on Windows
/// (0.75 pt a pixel). Excel for Mac counts a pixel as a point and 4 pt in all.
const ROOM_PADDING_PT: f32 = match ibis_pptx::EXCEL_METRICS {
    ibis_pptx::ExcelMetrics::Windows => 2.5 * 0.75,
    ibis_pptx::ExcelMetrics::Mac => 2.0,
};
// The text's distance from the cell's sides and its shift from the padded
// box, in points, as Excel for Mac prints a cell: in its PDF of xlsx-XALN
// (columns by ECMA-376's width formula) the pen of left-aligned text starts
// 2.0 pt from the grid line, right-aligned text ends 3.0 pt from it (its
// glyphs' advances rounded to whole points, whole_point_advances), and a
// left-aligned number starts where text does.
const TEXT_PAD_LEFT_PT: f32 = 2.0;
const TEXT_PAD_RIGHT_PT: f32 = 3.0;
/// Additional left padding for left-aligned numbers (points): none in Excel's PDF.
const TEXT_PAD_NUM_LEFT_PT: f32 = 0.0;
const TEXT_SHIFT_TOP_PT: f32 = 0.9;
const TEXT_SHIFT_CENTER_PT: f32 = 0.0;
const TEXT_SHIFT_BOTTOM_PT: f32 = 0.9;
/// A horizontal offset in pixels (none: positions are Excel's, in points).
const TEXT_LEFT_OFFSET_PX: f32 = 0.0;

/// Excel's grid lines on screen.
const GRID_COLOR: u32 = 0xd4d4d4;

/// A line of a cell's text, ready to paint: its pieces (text and size in
/// pixels) are placed side by side when painted, by their shaped widths (a
/// substitute's proportional widths are known only then), and the line is
/// aligned between `left` and `right`.
/// A piece of a line: text at a size in pixels, in the plain font or not,
/// or (with `advance`) room of that width with nothing drawn.
#[derive(Clone, Debug)]
struct Piece {
    text: String,
    size: f32,
    plain: bool,
    advance: Option<f32>,
    /// A part of the text in its own font (rich text).
    run: Option<Rc<RunPaint>>,
}

/// What a part of a cell's text (a `CellRun`) is drawn in.
#[derive(Debug)]
struct RunPaint {
    font: Font,
    color: Option<Hsla>,
    strike: bool,
    underline: CellUnderline,
}

struct TextItem {
    pieces: Vec<Piece>,
    font: Font,
    plain: Font,
    color: Hsla,
    align: CellAlign,
    left: f32,
    right: f32,
    baseline: f32,
    underline: CellUnderline,
    strike: bool,
    /// What the text may cover: the cell, and the empty cells it runs on to.
    clip: (f32, f32, f32, f32),
    /// The cell's left and right.
    cell: (f32, f32),
    font_size: f32,
    rotation: u8,
}

/// A filled rectangle (a cell fill or a border line), relative to the sheet.
/// Excel lays a cell's text out with each glyph's advance rounded to a
/// whole point (half up) of the sheet: in Excel's PDFs every advance (the
/// font's width plus the `Tc` and TJ adjustments) is one, 43 893 of the
/// 43 902 advances in the xlsx references. `px_per_point` is the pixels of
/// one sheet point.
fn whole_point_advances(line: &mut ShapedLine, px_per_point: f32) {
    if px_per_point <= 0.0 {
        return;
    }
    // Excel for Windows rounds to a whole pixel (0.75 pt), the Mac's to a point.
    let unit = px_per_point * ibis_pptx::EXCEL_METRICS.pixel_pt() as f32;
    let old: Vec<f32> = line.runs.iter().flat_map(|r| r.glyphs.iter().map(|g| f32::from(g.position.x))).collect();
    if old.is_empty() {
        return;
    }
    let width = f32::from(line.width);
    let mut runs = line.runs.clone();
    let (mut i, mut x) = (0, 0.0f32);
    for run in &mut runs {
        for g in run.glyphs.iter_mut() {
            let next = old.get(i + 1).copied().unwrap_or(width);
            let advance = ((next - old[i]) / unit + 0.5).floor() * unit;
            g.position.x = px(x);
            x += advance.max(0.0);
            i += 1;
        }
    }
    let laid: &mut Arc<LineLayout> = &mut *line;
    *laid = Arc::new(LineLayout { font_size: laid.font_size, width: px(x), ascent: laid.ascent, descent: laid.descent, runs, len: laid.len });
}

struct Quad {
    rect: (f32, f32, f32, f32),
    color: Hsla,
}

/// A dashed line of a printed border: one path, and in a PDF one line with its dash pattern.
#[derive(Clone)]
struct DashedLine {
    /// Where it runs (pixels).
    points: Vec<(f32, f32)>,
    width: f32,
    color: Hsla,
    /// The lengths of its dashes and gaps in turn, and how far into them it starts (pixels).
    pattern: Vec<f32>,
    phase: f32,
}

fn on_pixels_bounds(r: Bounds<Pixels>, across_y: bool, scale_factor: f32) -> Bounds<Pixels> {
    let k = scale_factor;
    let (x0, y0, x1, y1) = (
        f32::from(r.origin.x) * k,
        f32::from(r.origin.y) * k,
        f32::from(r.origin.x + r.size.width) * k,
        f32::from(r.origin.y + r.size.height) * k,
    );
    let ((a0, a1), (c0, c1)) = if across_y { ((x0, x1), (y0, y1)) } else { ((y0, y1), (x0, x1)) };
    let (a0, a1) = (a0.round(), a1.round());
    // Anchor `c1` (the cell grid boundary edge) to `c1.round()`, matching `snap_fill`,
    // and step back by the integer pixel thickness `w_px`.
    let w_px = (c1 - c0 + 1e-3).round().max(1.0);
    let c1_snapped = c1.round();
    let (c0, c1) = (c1_snapped - w_px, c1_snapped);
    let ((x0, x1), (y0, y1)) = if across_y { ((a0, a1), (c0, c1)) } else { ((c0, c1), (a0, a1)) };
    Bounds::new(point(px(x0 / k), px(y0 / k)), size(px((x1 - x0) / k), px((y1 - y0) / k)))
}

fn snap_fill_bounds(r: Bounds<Pixels>, scale_factor: f32) -> Bounds<Pixels> {
    let k = scale_factor;
    let (x0, y0) = ((f32::from(r.origin.x) * k).round(), (f32::from(r.origin.y) * k).round());
    let (x1, y1) = ((f32::from(r.origin.x + r.size.width) * k).round(), (f32::from(r.origin.y + r.size.height) * k).round());
    Bounds::new(point(px(x0 / k), px(y0 / k)), size(px((x1 - x0) / k), px((y1 - y0) / k)))
}

/// Draw `sheet` from the frame's top-left corner. `print` leaves out the
/// grid lines, as Excel prints.
pub(crate) fn sheet_element(sheet: &Arc<Sheet>, frame: EmuRect, scale: f32, res: &Resources) -> Div {
    let extra_cols_px = if !res.print { 50.0 * (sheet.default_column as f32 * scale) } else { 0.0 };
    let extra_rows_px = if !res.print { 1000.0 * (sheet.default_row as f32 * scale) } else { 0.0 };
    let (w, h) = ((frame.width as f32 * scale).max(extra_cols_px), (frame.height as f32 * scale).max(extra_rows_px));
    let sheet_layout = cached_layout(sheet, scale, res);
    let px_per_point = EMU_PER_POINT as f32 * scale;
    // Printed, a page shows its cells once: the text measured for the
    // layout is kept to be drawn (GPUI keeps the last frame's only).
    let keep_measured = res.print;
    let print = res.print;
    let pictures = web_pictures(&sheet_layout, scale);
    let sheet = sheet.clone();
    let root = div().absolute().left_0().top_0().w(px(w)).h(px(h)).child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let painting = crate::debug_timing::start();
                let scale_factor = window.scale_factor();
                let o = if print {
                    bounds.origin
                } else {
                    point(
                        px((f32::from(bounds.origin.x) * scale_factor).round() / scale_factor),
                        px((f32::from(bounds.origin.y) * scale_factor).round() / scale_factor),
                    )
                };
                let visible = window.content_mask().bounds;
                let shown = |(x, y, w, h): (f32, f32, f32, f32)| {
                    let r = Bounds::new(point(o.x + px(x), o.y + px(y)), size(px(w), px(h)));
                    r.intersects(&visible).then_some(r)
                };
                // What lies in the bands shown, relative to the sheet.
                let (top, bottom) = (f32::from(visible.top() - o.y), f32::from(visible.bottom() - o.y));
                let (left, right) = (f32::from(visible.left() - o.x), f32::from(visible.right() - o.x));
                // The parts of the layout for the rows shown.
                let parts = sheet_layout.parts(top, bottom);
                // On the screen a border or grid line is a whole number of the
                // screen's pixels thick, the same at every place for a style
                // and zoom (#2): its far edge anchored to the integer-snapped
                // grid line (`c1.round()`) and its width rounded (at least one)
                // so borders, grid lines and cell fills share the exact same
                // integer pixel boundary with zero 1px gaps or overlaps.
                // Printed, lines stay where Excel prints them (the PDF has no pixels).
                let on_pixels = |r: Bounds<Pixels>, across_y: bool| -> Bounds<Pixels> {
                    if print {
                        return r;
                    }
                    on_pixels_bounds(r, across_y, scale_factor)
                };
                let snap_fill = |r: Bounds<Pixels>| -> Bounds<Pixels> {
                    if print {
                        return r;
                    }
                    snap_fill_bounds(r, scale_factor)
                };
                for &(rect, ref fill) in parts.iter().flat_map(|l| l.gradients.iter()) {
                    if let Some(r) = shown(rect) {
                        let r = snap_fill(r);
                        let (w, h) = (f32::from(r.size.width), f32::from(r.size.height));
                        let subpaths = &[vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]];
                        crate::render::gradient::paint_gradient_mode(
                            window,
                            fill,
                            subpaths,
                            (w, h),
                            1.0,
                            false,
                            &|pts| pts.iter().map(|&(px_x, px_y)| point(r.origin.x + px(px_x), r.origin.y + px(px_y))).collect(),
                        );
                    }
                }
                for (is_grid, bands, quads) in parts.iter().map(|l| (true, &l.bands[1], &l.grid)).chain(parts.iter().map(|l| (false, &l.bands[0], &l.fills))) {
                    for i in bands.find(top, bottom) {
                        if let Some(r) = shown(quads[i as usize].rect) {
                            let r = if is_grid { on_pixels(r, r.size.width > r.size.height) } else { snap_fill(r) };
                            window.paint_quad(fill(r, quads[i as usize].color));
                        }
                    }
                }
                if !print && sheet.grid_lines {
                    let color = hsla(opaque(ibis_doc_model::Rgb(GRID_COLOR)));
                    let default_row_px = sheet.default_row as f32 * scale;
                    let default_col_px = sheet.default_column as f32 * scale;
                    let xs = sheet.column_offsets();
                    let ys = sheet.row_offsets();
                    let extra_cols = 50u32;
                    let extra_rows = 1000u32;
                    let base_max_row = (sheet.rows.len() as u32 + extra_rows).min(1_048_576);
                    let base_max_col = ((xs.len().saturating_sub(1) as u32) + extra_cols).min(16_384);

                    let y_of = |r: u32| -> f32 {
                        let len = ys.len();
                        let raw = if (r as usize) < len {
                            ys[r as usize] as f32 * scale
                        } else {
                            let last = ys.last().copied().unwrap_or(0);
                            (last + (r as usize - (len - 1)) as i64 * sheet.default_row) as f32 * scale
                        };
                        raw.round()
                    };
                    let x_of = |c: u32| -> f32 {
                        let len = xs.len();
                        let raw = if (c as usize) < len {
                            xs[c as usize] as f32 * scale
                        } else {
                            let last = xs.last().copied().unwrap_or(0);
                            (last + (c as usize - (len - 1)) as i64 * sheet.default_column) as f32 * scale
                        };
                        raw.round()
                    };

                    let base_bottom_y = y_of(base_max_row);
                    let base_right_x = x_of(base_max_col);

                    if bottom > base_bottom_y {
                        let first_r = base_max_row + 1;
                        let last_r = (base_max_row + ((bottom - base_bottom_y) / default_row_px.max(1.0)).ceil() as u32 + 2).min(1_048_576);
                        let w_extent = w.max(base_right_x);
                        for r in first_r..=last_r {
                            let y = y_of(r);
                            if y >= top - default_row_px && y <= bottom + default_row_px {
                                if let Some(rect) = shown((0.0, y - 1.0, w_extent, 1.0)) {
                                    window.paint_quad(fill(on_pixels(rect, true), color));
                                }
                            }
                        }
                        let max_col_view = ((right.max(w_extent) / default_col_px.max(1.0)).ceil() as u32 + 2).min(16_384);
                        let seg_top = top.max(base_bottom_y);
                        let seg_bottom = bottom.min(y_of(last_r));
                        let seg_h = (seg_bottom - seg_top).max(0.0);
                        if seg_h > 0.0 {
                            for c in 1..=max_col_view {
                                let x = x_of(c);
                                if x >= left - default_col_px && x <= right + default_col_px {
                                    if let Some(rect) = shown((x - 1.0, seg_top, 1.0, seg_h)) {
                                        window.paint_quad(fill(on_pixels(rect, false), color));
                                    }
                                }
                            }
                        }
                    }
                }
                // The spans of text running on past its cell: left, right,
                // top and bottom, relative to the sheet.
                let mut run_on: Vec<(f32, f32, f32, f32)> = Vec::new();
                for layout in parts.iter() {
                // The text of the cells shown, laid out (measured) as they
                // are first shown: a long sheet's cells are not all
                // measured at once (a screenful is the first paint's).
                let cell_bands = layout.cell_bands.get_or_init(|| Bands::new(layout.cells.iter().map(|c| (c.rect.0, c.rect.1, c.rect.2 - c.rect.0, c.rect.3 - c.rect.1))));
                let found = cell_bands.find(top, bottom);
                let ts_key = (std::sync::Arc::as_ptr(window.text_system()) as usize as u64).wrapping_mul(0x517C_C1B7_2722_0A95);
                {
                    let mut items = layout.items.borrow_mut();
                    if items.len() != layout.cells.len() {
                        items.resize_with(layout.cells.len(), || None);
                    }
                    let measure = |text: &str, font: &Font, size: f32| -> f32 {
                        let key = measured_key(text, font, size) ^ ts_key;
                        // A text shaped before (the same characters, font and size, in this scale) keeps its width and
                        // shaping: the sheet's cells hold few different texts (a printed 5,000-page sheet shaped 590,000
                        // cells, 70 % of its export's time).
                        let cache_key = key ^ (px_per_point.to_bits() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                        if let Some((width, line)) = SHAPED_TEXTS.with(|c| c.borrow().get(&cache_key).cloned()) {
                            if keep_measured {
                                layout.measured.borrow_mut().insert(key, line);
                            }
                            return width;
                        }
                        let run = TextRun { len: text.len(), font: font.clone(), color: black(), background_color: None, underline: None, strikethrough: None };
                        let mut line = window.text_system().shape_line(SharedString::from(text.to_string()), px(size), &[run], None);
                        whole_point_advances(&mut line, px_per_point);
                        let width = f32::from(line.width);
                        let line = Rc::new(line);
                        SHAPED_TEXTS.with(|c| {
                            let mut c = c.borrow_mut();
                            if c.len() >= SHAPED_TEXTS_MAX {
                                c.clear();
                            }
                            c.insert(cache_key, (width, line.clone()));
                        });
                        if keep_measured {
                            layout.measured.borrow_mut().insert(key, line);
                        }
                        width
                    };
                    let measuring = crate::debug_timing::start();
                    for &i in &found {
                        let slot = &mut items[i as usize];
                        if slot.is_none() {
                            let mut out = Vec::new();
                            text_items(&layout.cells[i as usize], px_per_point, &measure, &mut out);
                            *slot = Some(out);
                        }
                    }
                    crate::debug_timing::report("sheet paint: text measured", measuring);
                }
                let items = layout.items.borrow();
                let measured = layout.measured.borrow();
                for t in found.iter().flat_map(|&i| items[i as usize].as_deref().unwrap_or(&[])) {
                    let Some(clip) = shown(t.clip) else { continue };
                    // Shaped pieces, or room of a fixed width.
                    let shaped: Vec<Result<ShapedLine, f32>> = t
                        .pieces
                        .iter()
                        .map(|p| {
                            if let Some(a) = p.advance {
                                return Err(a);
                            }
                            let size = p.size;
                            let (piece_color, piece_strike) = match &p.run {
                                Some(r) => (r.color.unwrap_or(t.color), r.strike),
                                None => (t.color, t.strike),
                            };
                            let font = match &p.run {
                                Some(r) => &r.font,
                                None if p.plain => &t.plain,
                                None => &t.font,
                            };
                            let run = TextRun {
                                len: p.text.len(),
                                font: font.clone(),
                                color: piece_color,
                                background_color: None,
                                underline: None,
                                strikethrough: piece_strike.then(|| StrikethroughStyle { thickness: px((size / 14.0).max(0.5)), color: Some(piece_color) }),
                            };
                            Ok(match measured.get(&(measured_key(&p.text, font, size) ^ ts_key)).filter(|l| l.text.as_ref() == p.text) {
                                Some(line) => line.restyled(&[run]),
                                None => {
                                    let mut line = window.text_system().shape_line(SharedString::from(p.text.clone()), px(size), &[run], None);
                                    whole_point_advances(&mut line, px_per_point);
                                    line
                                }
                            })
                        })
                        .collect();
                    let piece_width = |l: &Result<ShapedLine, f32>| match l {
                        Ok(l) => f32::from(l.width),
                        Err(a) => *a,
                    };
                    let total: f32 = shaped.iter().map(piece_width).sum();
                    let count = shaped.len();
                    let trailing = t.pieces.last().is_some_and(|p| p.advance.is_none() && p.text.ends_with(' '));
                    let (mut x, gap) = compute_text_x(t.align, t.left, t.right, total, count, trailing, t.rotation, px_per_point);
                    let d_px = 1.0 / window.scale_factor();
                    let (clip_left, clip_top, clip_w, clip_h) = t.clip;
                    let (start, end) = (x.max(clip_left), (x + total + gap * count.saturating_sub(1) as f32).min(clip_left + clip_w));
                    let is_angled = t.rotation >= 1 && t.rotation <= 180;
                    if !is_angled && (start < t.cell.0 - 0.5 || end > t.cell.1 + 0.5) {
                        run_on.push((start, end, clip_top, clip_top + clip_h));
                    }
                    let mask_bounds = Bounds::new(clip.origin, clip.size);
                    window.with_content_mask(Some(ContentMask { bounds: mask_bounds }), |window| {
                        if print && !is_angled && t.cell.1 <= left + 0.5 && end > left + 0.5 {
                            let bg_left = left.max(clip_left);
                            let bg_w = (end - bg_left).max(0.0);
                            if bg_w > 0.0 {
                                window.paint_quad(fill(
                                    Bounds::new(point(o.x + px(bg_left), o.y + px(clip_top)), size(px(bg_w), px(clip_h))),
                                    gpui_kit::white(),
                                ));
                            }
                        }
                        if is_angled {
                            let angle_degrees = if t.rotation <= 90 {
                                -(t.rotation as f32)
                            } else {
                                (t.rotation - 90) as f32
                            };
                            let angle = angle_degrees.to_radians();
                            let (sin, cos) = angle.sin_cos();
                            let scale = window.scale_factor();
                            let x0 = t.left;
                            let y0 = t.baseline;
                            let mut x_along = 0.0;
                            for (p_idx, piece) in shaped.iter().enumerate() {
                                if let Ok(line) = piece {
                                    let p_size = t.pieces.get(p_idx).map_or(t.font_size, |p| p.size);
                                    for run in &line.runs {
                                        for glyph in &run.glyphs {
                                            let gx = x_along + f32::from(glyph.position.x);
                                            let gy = f32::from(glyph.position.y);
                                            let turned_x = o.x + px(x0 + gx * cos - gy * sin);
                                            let turned_y = o.y + px(y0 + gx * sin + gy * cos);
                                            let turned = point(turned_x, turned_y);
                                            let pivot = point(ScaledPixels(f32::from(turned.x) * scale), ScaledPixels(f32::from(turned.y) * scale));
                                            let transformation = TransformationMatrix::unit()
                                                .translate(pivot)
                                                .rotate(Radians(angle))
                                                .translate(point(ScaledPixels(-pivot.x.0), ScaledPixels(-pivot.y.0)));
                                            let _ = window.paint_glyph_transformed(turned, run.font_id, glyph.id, px(p_size), t.color, transformation);
                                        }
                                    }
                                }
                                x_along += piece_width(piece);
                            }
                        } else {
                            let text_start_x = x;
                            // Spread text: each piece moved by its share of
                            // the room on a whole point (half up), as Excel.
                            let (mut natural, mut k) = (x, 0.0f32);
                            let mut piece_lines: Vec<(f32, f32, CellUnderline, Hsla)> = Vec::new();
                            for (pi, piece) in shaped.iter().enumerate() {
                                if let Some(r) = t.pieces.get(pi).and_then(|p| p.run.as_ref())
                                    && r.underline != CellUnderline::None
                                {
                                    piece_lines.push((x, piece_width(piece), r.underline, r.color.unwrap_or(t.color)));
                                }
                                if t.align == CellAlign::Distributed && gap > 0.0 {
                                    x = natural + (k * gap / px_per_point + 0.5 + 1e-3).floor() * px_per_point;
                                }
                                if let Ok(line) = piece {
                                    // A box the font's height, its top the ascent above the baseline.
                                    let origin = point(o.x + px(x), o.y + px(t.baseline) - line.ascent);
                                    line.paint(origin, line.ascent + line.descent, TextAlign::Left, None, window, cx).ok();
                                }
                                x += piece_width(piece) + gap;
                                natural += piece_width(piece);
                                k += 1.0;
                            }
                            let pad = CELL_PADDING_PT * px_per_point;
                            let font_pt = t.font_size / px_per_point;
                            let u_off_pt = (0.75 + (font_pt - 11.0) / 9.0).max(0.75);
                            let u_offset = u_off_pt * px_per_point;
                            let snap = |val: Pixels| px((f32::from(val) * window.scale_factor()).round() / window.scale_factor());
                            let u_y = snap(o.y + px(t.baseline + u_offset));
                            // Printed, as Excel does (its PDFs of xlsx-XFNT, Calibri 11 and 20 pt): the lines are the
                            // font's underline thickness (Calibri's 0.0654 em) down to whole sheet points, at least 1,
                            // the top the font's underline position (0.1133 em, 游ゴシック's 0.1001 for East Asian
                            // text) rounded to whole points below the baseline; a double underline's second line is
                            // at twice that position (rounded down) and its first two thicknesses above it (Excel's
                            // PDFs at 11 and 20 pt, Calibri and East Asian text).
                            let has_east_asian = t.pieces.iter().any(|p| !p.text.is_ascii());
                            let printed = print.then(|| {
                                let em = if has_east_asian { 0.1001 } else { 0.1133 };
                                let thick = ((font_pt * 0.0654).floor().max(1.0)) * px_per_point;
                                let top = o.y + px(t.baseline + (font_pt * em).round() * px_per_point);
                                let second = o.y + px(t.baseline + (font_pt * 2.0 * em + 1e-3).floor() * px_per_point);
                                (thick, top, second)
                            });
                            match t.underline {
                                CellUnderline::None => {}
                                CellUnderline::Single if printed.is_some() => {
                                    let (thick, top, _) = printed.unwrap();
                                    let x = o.x + px(text_start_x);
                                    window.paint_quad(fill(Bounds::new(point(x, top), size(px(total), px(thick))), t.color));
                                }
                                CellUnderline::Double if printed.is_some() => {
                                    let (thick, _, second) = printed.unwrap();
                                    let x = o.x + px(text_start_x);
                                    window.paint_quad(fill(Bounds::new(point(x, second - px(2.0 * thick)), size(px(total), px(thick))), t.color));
                                    window.paint_quad(fill(Bounds::new(point(x, second), size(px(total), px(thick))), t.color));
                                }
                                CellUnderline::Single => {
                                    let x = snap(o.x + px(text_start_x));
                                    let r = Bounds::new(point(x, u_y), size(px(total), px(d_px)));
                                    window.paint_quad(fill(r, t.color));
                                }
                                CellUnderline::Double => {
                                    let x = snap(o.x + px(text_start_x));
                                    let y2 = u_y;
                                    let y1 = y2 - px(2.0 * d_px);
                                    let r1 = Bounds::new(point(x, y1), size(px(total), px(d_px)));
                                    let r2 = Bounds::new(point(x, y2), size(px(total), px(d_px)));
                                    window.paint_quad(fill(r1, t.color));
                                    window.paint_quad(fill(r2, t.color));
                                }
                                CellUnderline::SingleAccounting => {
                                    let lx = snap(o.x + px(t.cell.0 + pad));
                                    let rx = snap(o.x + px(t.cell.1 - pad));
                                    let lw = (rx - lx).max(px(0.0));
                                    let acc_baseline = t.baseline + 3.5 * px_per_point;
                                    let y2 = snap(o.y + px(acc_baseline + u_offset));
                                    let y = y2 - px(2.0 * d_px);
                                    let r = Bounds::new(point(lx, y), size(lw, px(d_px)));
                                    window.paint_quad(fill(r, t.color));
                                }
                                CellUnderline::DoubleAccounting => {
                                    let lx = snap(o.x + px(t.cell.0 + pad));
                                    let rx = snap(o.x + px(t.cell.1 - pad));
                                    let lw = (rx - lx).max(px(0.0));
                                    let acc_baseline = t.baseline + 3.5 * px_per_point;
                                    let y2 = snap(o.y + px(acc_baseline + u_offset));
                                    let y1 = y2 - px(2.0 * d_px);
                                    let r1 = Bounds::new(point(lx, y1), size(lw, px(d_px)));
                                    let r2 = Bounds::new(point(lx, y2), size(lw, px(d_px)));
                                    window.paint_quad(fill(r1, t.color));
                                    window.paint_quad(fill(r2, t.color));
                                }
                            }
                            let sf = window.scale_factor();
                            let u_y2 = px((f32::from(o.y + px(t.baseline + u_offset)) * sf).round() / sf);
                            for (lx, lw, kind, color) in piece_lines {
                                let x = px((f32::from(o.x + px(lx)) * sf).round() / sf);
                                let double = matches!(kind, CellUnderline::Double | CellUnderline::DoubleAccounting);
                                window.paint_quad(fill(Bounds::new(point(x, u_y2), size(px(lw), px(d_px))), color));
                                if double {
                                    window.paint_quad(fill(Bounds::new(point(x, u_y2 - px(2.0 * d_px)), size(px(lw), px(d_px))), color));
                                }
                            }
                        }
                    });
                }
                }
                for q in parts.iter().flat_map(|l| l.bands[2].find(top, bottom).into_iter().map(move |i| &l.borders[i as usize])) {
                    if let Some(r) = shown(q.rect) {
                        window.paint_quad(fill(on_pixels(r, true), q.color));
                    }
                }
                for q in parts.iter().flat_map(|l| l.bands[3].find(top, bottom).into_iter().map(move |i| &l.vertical_borders[i as usize])) {
                    let (x, y, w, h) = q.rect;
                    let (middle, row) = (x + w / 2.0, y + h / 2.0);
                    if run_on.iter().any(|&(l, r, t, b)| l < middle && middle < r && t < row && row < b) {
                        continue;
                    }
                    if let Some(r) = shown(q.rect) {
                        window.paint_quad(fill(on_pixels(r, false), q.color));
                    }
                }
                for (outline, pen, color) in parts.iter().flat_map(|l| l.diagonal_shapes.iter()) {
                    let (x0, x1) = outline.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
                    let (y0, y1) = outline.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
                    if shown((x0, y0, x1 - x0, y1 - y0)).is_none() {
                        continue;
                    }
                    let points: Vec<_> = outline.iter().map(|&(x, y)| point(o.x + px(x), o.y + px(y))).collect();
                    if let Some(path) = polygon(&points, None, true) {
                        window.paint_path(path, *color);
                    }
                    if *pen > 0.0
                        && let Some(path) = polygon(&points, Some(px(*pen)), true)
                    {
                        window.paint_path(path, *color);
                    }
                }
                for line in parts.iter().flat_map(|l| l.dashed_lines.iter()) {
                    let (x0, x1) = line.points.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
                    let (y0, y1) = line.points.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
                    if shown((x0, y0, x1 - x0, y1 - y0)).is_none() {
                        continue;
                    }
                    let moved: Vec<_> = line.points.iter().map(|&(x, y)| point(o.x + px(x), o.y + px(y))).collect();
                    if let Some(path) = crate::render::shape_view::dashed_line_path(&moved, px(line.width), &line.pattern, line.phase) {
                        window.paint_path(path, line.color);
                    }
                }
                for (a, b, width, color) in parts.iter().flat_map(|l| l.diagonals.iter()) {
                    let rect = (a.0.min(b.0), a.1.min(b.1), (a.0 - b.0).abs(), (a.1 - b.1).abs());
                    if shown(rect).is_none() {
                        continue;
                    }
                    // Not clipped to the cell: Excel's thick line ends run
                    // past the corners.
                    let ends = [point(o.x + px(a.0), o.y + px(a.1)), point(o.x + px(b.0), o.y + px(b.1))];
                    if let Some(path) = polygon(&ends, Some(px(width.max(1.0 / window.scale_factor()))), false) {
                        window.paint_path(path, *color);
                    }
                }
                for (p1, p2, p3, color) in parts.iter().flat_map(|l| l.comments.iter()) {
                    let tri = [
                        point(o.x + px(p1.0), o.y + px(p1.1)),
                        point(o.x + px(p2.0), o.y + px(p2.1)),
                        point(o.x + px(p3.0), o.y + px(p3.1)),
                    ];
                    if let Some(path) = polygon(&tri, None, true) {
                        window.paint_path(path, *color);
                    }
                }
                crate::debug_timing::report("sheet paint", painting);
            },
        )
        .absolute()
        .size_full(),
    );
    if pictures.is_empty() {
        return root;
    }
    // The pictures IMAGE shows, over the cells (their text is not drawn).
    root.child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let o = bounds.origin;
                let visible = window.content_mask().bounds;
                let mut waiting = false;
                for (rect, spec) in &pictures {
                    let cell = Bounds::new(point(o.x + px(rect.0), o.y + px(rect.1)), size(px(rect.2), px(rect.3)));
                    if !cell.intersects(&visible) || rect.2 < 2.0 || rect.3 < 2.0 {
                        continue;
                    }
                    match crate::render::web_image::poll(&spec.source, cx) {
                        crate::render::web_image::Poll::Pending => waiting = true,
                        crate::render::web_image::Poll::Failed => {}
                        crate::render::web_image::Poll::Ready(image) => {
                            let natural = image.size(0);
                            let (iw, ih) = (f32::from(natural.width.0 as f32), f32::from(natural.height.0 as f32));
                            if iw <= 0.0 || ih <= 0.0 {
                                continue;
                            }
                            let (cw, ch) = (rect.2 - 1.0, rect.3 - 1.0);
                            let (w, h, dx, dy) = picture_box(spec, (iw, ih), (cw, ch), px_per_point);
                            let area = Bounds::new(point(o.x + px(rect.0), o.y + px(rect.1)), size(px(cw), px(ch)));
                            let at = Bounds::new(point(o.x + px(rect.0 + dx), o.y + px(rect.1 + dy)), size(px(w), px(h)));
                            let _ = window.paint_image(area, at, Corners::default(), image, 0, false);
                        }
                    }
                }
                if waiting {
                    window.request_animation_frame();
                }
            },
        )
        .absolute()
        .size_full(),
    )
}

/// Where a picture of IMAGE goes in its cell of `cw` x `ch` pixels, from the cell's left and top: its width and
/// height and the offset of its left and top. `natural` is the picture's own size in pixels (96 to the inch),
/// `px_per_point` the screen pixels in a point.
fn picture_box(spec: &ibis_calc::eval::ImageSpec, natural: (f32, f32), (cw, ch): (f32, f32), px_per_point: f32) -> (f32, f32, f32, f32) {
    let (iw, ih) = natural;
    // A pixel of the picture is 0.75 point.
    let own = |w: f32, h: f32| (w * 0.75 * px_per_point, h * 0.75 * px_per_point);
    match spec.sizing {
        // Fills the cell, the proportions not kept.
        1 => (cw, ch, 0.0, 0.0),
        // Its own size, from the cell's corner (what is past the cell is cut).
        2 => {
            let (w, h) = own(iw, ih);
            (w, h, 0.0, 0.0)
        }
        // The height and width given.
        3 => {
            let (w, h) = own(spec.width as f32, spec.height as f32);
            (w, h, 0.0, 0.0)
        }
        // As large as fits the cell with its proportions, in the middle.
        _ => {
            let k = (cw / iw).min(ch / ih);
            let (w, h) = (iw * k, ih * k);
            (w, h, (cw - w) / 2.0, (ch - h) / 2.0)
        }
    }
}

/// The cells' places in pixels from the sheet's corner, with what each shows.
fn web_pictures(layout: &SheetLayout, scale: f32) -> Vec<((f32, f32, f32, f32), ibis_calc::eval::ImageSpec)> {
    if layout.images.is_empty() {
        return Vec::new();
    }
    let sheet = layout.sheet();
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let x = |c: u32| xs[(c as usize).min(xs.len() - 1)] as f32 * scale;
    let y = |r: u32| ys[(r as usize).min(ys.len() - 1)] as f32 * scale;
    layout
        .images
        .iter()
        .map(|((row, column), image)| {
            let range = sheet.merge_at(*row, *column).unwrap_or(CellRange { first_row: *row, first_column: *column, last_row: *row, last_column: *column });
            let (x0, y0) = (x(range.first_column), y(range.first_row));
            ((x0, y0, x(range.last_column + 1) - x0, y(range.last_row + 1) - y0), image.clone())
        })
        .collect()
}

thread_local! {
    /// Layouts of the sheets drawn lately, by the sheet, the scale and
    /// whether printed: GPUI draws the whole window again on every scroll.
    static LAYOUTS: std::cell::RefCell<Vec<CachedLayout>> = const { std::cell::RefCell::new(Vec::new()) };
    static PRINT_REGIONS: std::cell::RefCell<Vec<(LayoutKey, std::sync::Weak<Sheet>, Vec<EmuRect>)>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Forget the layouts kept: they hold their sheets, so that an edit would copy every cell of a big sheet before changing one (the layout of the changed sheet is made again at the next draw).
pub(crate) fn release_layouts() {
    LAYOUTS.with(|cache| cache.borrow_mut().clear());
    PRINT_REGIONS.with(|cache| cache.borrow_mut().clear());
    SHAPED_TEXTS.with(|cache| cache.borrow_mut().clear());
}

/// The text displayed for a cell on screen: evaluates show_formulas and show_zeros.
pub(crate) fn cell_shown_text(sheet: &Sheet, cell: &ibis_doc_model::SheetCell) -> String {
    if sheet.show_formulas && cell.formula.is_some() {
        format!("={}", sheet.formula_shown(cell.row, cell.column, cell.formula.as_ref().unwrap()))
    } else if !sheet.show_zeros && matches!(cell.value, ibis_doc_model::CellValue::Number(n) if n == 0.0) {
        String::new()
    } else {
        cell.text.clone()
    }
}

/// A layout by its key (the sheet's address, the scale's bits and whether
/// printed), with the sheet it was made for.
type LayoutKey = (usize, usize, usize, u32, bool, u64, i64, u64);
type CachedLayout = (LayoutKey, Arc<Sheet>, Rc<SheetLayout>);

/// How many layouts to keep (a workbook's sheets at a zoom or two).
const CACHED_LAYOUTS: usize = 8;

fn layout_key(sheet: &Arc<Sheet>, scale: f32, res: &Resources) -> LayoutKey {
    // Keyed by the cell content generation so zoom changes on `Sheet`
    // retain cached layouts across zoom levels.
    let breaks = res.print_breaks.borrow();
    let breaks_key = breaks.0.iter().chain(&breaks.1).fold(breaks.0.len() as i64, |h, v| h.wrapping_mul(31).wrapping_add(*v));
    drop(breaks);
    (sheet.cells.content_id(), Arc::as_ptr(sheet) as usize, res.layout_generation, scale.to_bits(), res.print, res.print_scale.get().to_bits(), breaks_key, crate::render::web_image::generation())
}

fn cached_layout(sheet: &Arc<Sheet>, scale: f32, res: &Resources) -> Rc<SheetLayout> {
    let key = layout_key(sheet, scale, res);
    LAYOUTS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(i) = cache.iter().position(|(k, _, _)| *k == key) {
            let entry = cache.remove(i);
            let layout = entry.2.clone();
            cache.push(entry);
            return layout;
        }
        // Conditional formats: the sheet drawn with them applied
        // (the same at every zoom: kept apart, and worked out ahead).
        let started = crate::debug_timing::start();
        let applied = super::conditional::applied(sheet);
        crate::debug_timing::report("sheet conditional formats", started);
        let started = crate::debug_timing::start();
        let layout = Rc::new(SheetLayout::new(sheet.clone(), applied, scale, res));
        crate::debug_timing::report(&format!("sheet layout base ({} cells)", sheet.cells.len()), started);
        if cache.len() >= CACHED_LAYOUTS {
            cache.remove(0);
        }
        cache.push((key, sheet.clone(), layout.clone()));
        layout
    })
}

/// What a layout holds: the whole sheet (printed: a page is drawn once),
/// the grid lines and comment marks only (the screen's base), or the
/// cells of the rows from the first up to the second (the screen's parts).
#[derive(Clone, Copy, PartialEq)]
enum Part {
    All,
    Base,
    Rows(u32, u32),
}

/// How many rows a part of the screen's layout holds.
const PART_ROWS: u32 = 64;

/// What laying out takes from the resources: whether printed, and the
/// fonts of the sheet's formats (by index; the last for a missing one).
struct LayoutContext {
    print: bool,
    print_scale: f64,
    breaks: Arc<(Vec<i64>, Vec<i64>)>,
    fonts: Vec<CellFont>,
    sheets: HashMap<String, Arc<Sheet>>,
    /// The cells that show a picture from an address (IMAGE): their text (the file's `#VALUE!`) is not drawn.
    images: HashMap<(u32, u32), String>,
}

impl LayoutContext {
    fn font(&self, style: u32) -> CellFont {
        self.fonts.get(style as usize).or(self.fonts.last()).cloned().expect("the default font")
    }
}

/// A sheet laid out at a scale: on the screen, the rows laid out as they
/// are first shown (a zoom or a sheet changed lays out a screenful, not
/// the whole sheet); printed, all at once.
struct SheetLayout {
    source: Arc<Sheet>,
    applied: super::conditional::Applied,
    scale: f32,
    context: LayoutContext,
    base: Rc<Layout>,
    /// Each row's top, and the bottom of the last, in pixels.
    row_tops: Vec<f32>,
    /// Merged ranges' first and last rows.
    merges: Vec<(u32, u32)>,
    parts: std::cell::RefCell<HashMap<u32, Rc<Layout>>>,
    /// The pictures IMAGE shows in cells.
    images: Vec<((u32, u32), ibis_calc::eval::ImageSpec)>,
}

impl SheetLayout {
    fn new(source: Arc<Sheet>, applied: super::conditional::Applied, scale: f32, res: &Resources) -> Self {
        let sheet: &Sheet = match applied.as_ref() {
            Some((s, _)) => s,
            None => &source,
        };
        let mut fonts: Vec<CellFont> = sheet.styles.iter().map(|s| cell_font(s, res)).collect();
        fonts.push(cell_font(&CellStyle::default(), res));
        // What the calculation asked IMAGE for, of the cells that still hold a formula.
        let images: Vec<((u32, u32), ibis_calc::eval::ImageSpec)> = source.web_images.iter().filter(|((r, c), _)| source.cell(*r, *c).is_some_and(|x| x.formula.is_some())).map(|(k, v)| (*k, v.clone())).collect();
        let context = LayoutContext { print: res.print, print_scale: res.print_scale.get(), breaks: res.print_breaks.borrow().clone(), fonts, sheets: res.sheets.clone(), images: images.iter().map(|(k, i)| (*k, i.source.clone())).collect() };
        let row_tops = sheet
            .row_offsets()
            .iter()
            .map(|&y| {
                let raw = y as f32 * scale;
                if res.print { raw } else { raw.round() }
            })
            .collect();
        let merges = sheet.merges.iter().map(|m| (m.first_row, m.last_row)).collect();
        let overlay = applied.as_ref().as_ref().map(|(_, o)| o);
        let part = if res.print { Part::All } else { Part::Base };
        let base = Rc::new(layout(sheet, scale, &context, overlay, part));
        Self { source: source.clone(), applied: applied.clone(), scale, context, base, row_tops, merges, parts: Default::default(), images }
    }

    fn sheet(&self) -> &Sheet {
        match self.applied.as_ref() {
            Some((s, _)) => s,
            None => &self.source,
        }
    }

    /// The layouts to draw the band from `top` to `bottom` (pixels from
    /// the sheet's top): the base, and the parts of the rows in it (with a
    /// row more each way, whose lines and fills reach over, and the rows
    /// of merged ranges reaching into it from above).
    fn parts(&self, top: f32, bottom: f32) -> Vec<Rc<Layout>> {
        let mut out = vec![self.base.clone()];
        if self.context.print {
            return out;
        }
        let rows = self.row_tops.len().saturating_sub(1) as u32;
        let row_at = |y: f32| self.row_tops.partition_point(|&t| t <= y).saturating_sub(1) as u32;
        let mut first = row_at(top).saturating_sub(1);
        let last = row_at(bottom).saturating_add(1).min(rows);
        for &(a, b) in &self.merges {
            if a < first && b >= first {
                first = a;
            }
        }
        let last_part = rows / PART_ROWS;
        let mut parts = self.parts.borrow_mut();
        for index in (first / PART_ROWS).min(last_part)..=(last / PART_ROWS).min(last_part) {
            let part = parts.entry(index).or_insert_with(|| {
                let started = crate::debug_timing::start();
                let from = index * PART_ROWS;
                // The last part takes any cells past the sheet's rows.
                let to = if index == last_part { u32::MAX } else { from + PART_ROWS };
                let overlay = self.applied.as_ref().as_ref().map(|(_, o)| o);
                let laid = Rc::new(layout(self.sheet(), self.scale, &self.context, overlay, Part::Rows(from, to)));
                crate::debug_timing::report("sheet layout part", started);
                laid
            });
            out.push(part.clone());
        }
        out
    }
}

struct Layout {
    fills: Vec<Quad>,
    /// Gradient fills: the rectangle (left, top, width, height) and drawing Fill.
    gradients: Vec<((f32, f32, f32, f32), ibis_doc_model::Fill)>,
    grid: Vec<Quad>,
    /// The cells' text, laid out at the first paint (by shaped widths).
    cells: Vec<CellText>,
    /// The cells by the bands of the sheet they are in.
    cell_bands: std::cell::OnceCell<Bands>,
    /// Each cell's text items, made as the cell is first shown.
    items: std::cell::RefCell<Vec<Option<Vec<TextItem>>>>,
    /// The lines measured, by `measured_key` (printed only).
    measured: std::cell::RefCell<HashMap<u64, Rc<ShapedLine>>>,
    borders: Vec<Quad>,
    /// Vertical border lines, apart: text running on over cells hides
    /// those it crosses, as in Excel.
    vertical_borders: Vec<Quad>,
    diagonals: Vec<Diagonal>,
    /// Solid diagonal lines as Excel prints them: outlines filled and
    /// stroked with a pen this wide.
    diagonal_shapes: Vec<(Vec<(f32, f32)>, f32, Hsla)>,
    /// Lines of short dashes (the dots of dotted borders): the dashes, the line's width and its colour.
    dashed_lines: Vec<DashedLine>,
    comments: Vec<((f32, f32), (f32, f32), (f32, f32), Hsla)>,
    /// The fills, grid lines, borders and vertical borders by band.
    bands: [Bands; 4],
}

const SHAPED_TEXTS_MAX: usize = 100_000;

thread_local! {
    /// The texts shaped so far for measuring: (width, the shaped line), by `measured_key` and the scale.
    static SHAPED_TEXTS: std::cell::RefCell<HashMap<u64, (f32, Rc<ShapedLine>)>> = std::cell::RefCell::new(HashMap::new());
}

/// A measured line's key: a hash of its text, font and size in pixels.
fn measured_key(text: &str, font: &Font, size: f32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (text, font, size.to_bits()).hash(&mut h);
    h.finish()
}

/// A diagonal border line: its two ends, its width and its colour.
type Diagonal = ((f32, f32), (f32, f32), f32, Hsla);

/// Empty contiguous Fill cells repeat the entry to their left. Do not cross
/// unstyled gaps, values, formulas with an empty result, or merged cells.
/// `cells` are whole rows of the sheet (a part's), in order: the entries stay within a row.
fn fill_sources<'a>(sheet: &Sheet, cells: &'a [ibis_doc_model::SheetCell]) -> HashMap<(u32, u32), &'a ibis_doc_model::SheetCell> {
    let mut sources = HashMap::new();
    let mut previous = None;
    let mut source = None;
    for cell in cells {
        if previous != cell.column.checked_sub(1).map(|c| (cell.row, c)) {
            source = None;
        }
        previous = Some((cell.row, cell.column));
        if !sheet.styles.get(cell.style as usize).is_some_and(|s| s.align == CellAlign::Fill)
            || sheet.merge_at(cell.row, cell.column).is_some() {
            source = None;
        } else if !cell.text.is_empty() {
            source = Some(cell);
        } else if cell.value.is_empty() && cell.formula.is_none() {
            if let Some(entry) = source { sources.insert((cell.row, cell.column), entry); }
        } else {
            source = None;
        }
    }
    sources
}

fn layout(sheet: &Sheet, scale: f32, ctx: &LayoutContext, overlay: Option<&super::conditional::Overlay>, part: Part) -> Layout {
    let in_part = |row: u32| match part {
        Part::All => true,
        Part::Base => false,
        Part::Rows(from, to) => from <= row && row < to,
    };
    let part_cells: &[ibis_doc_model::SheetCell] = match part {
        Part::All => &sheet.cells,
        Part::Base => &[],
        Part::Rows(from, to) => {
            let (a, b) = (sheet.cells.partition_point(|c| c.row < from), sheet.cells.partition_point(|c| c.row < to));
            &sheet.cells[a..b]
        }
    };
    let whole = matches!(part, Part::All | Part::Base);
    let anchors = dash_anchors(sheet, if ctx.print { ctx.print_scale } else { 1.0 });
    DASH_ANCHOR.with(|a| a.set(anchors));
    PRINT_GRID.with(|g| g.set(ctx.print.then(|| (ctx.print_scale as f32, -anchors.0, -anchors.1))));
    BORDER_SHAPES.with(|s| s.borrow_mut().clear());
    BORDER_DASHES.with(|s| s.borrow_mut().clear());
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let x = |c: u32| {
        let len = xs.len();
        let raw = if (c as usize) < len {
            xs[c as usize] as f32 * scale
        } else {
            let last = xs.last().copied().unwrap_or(0);
            (last + (c as usize - (len - 1)) as i64 * sheet.default_column) as f32 * scale
        };
        if ctx.print { raw } else { raw.round() }
    };
    let y = |r: u32| {
        let len = ys.len();
        let raw = if (r as usize) < len {
            ys[r as usize] as f32 * scale
        } else {
            let last = ys.last().copied().unwrap_or(0);
            (last + (r as usize - (len - 1)) as i64 * sheet.default_row) as f32 * scale
        };
        if ctx.print { raw } else { raw.round() }
    };
    let px_per_point = EMU_PER_POINT as f32 * scale;
    let page_height_emu = sheet.print.page_size().1;
    let default_style = CellStyle::default();
    let style_of = |i: u32| sheet.styles.get(i as usize).unwrap_or(&default_style);
    // Pivot tables' row labels, drawn in by Excel's own indent.
    let pivot_pads: HashMap<(u32, u32), f32> = sheet.pivots.iter().flat_map(|p| p.label_pads()).map(|(r, c, pt)| ((r, c), pt)).collect();

    // Merged ranges by their top-left cell; the other cells they cover.
    let mut merge_at: HashMap<(u32, u32), CellRange> = HashMap::new();
    let mut covered: std::collections::HashSet<(u32, u32)> = Default::default();
    for m in &sheet.merges {
        merge_at.insert((m.first_row, m.first_column), *m);
        for r in m.first_row..=m.last_row {
            for c in m.first_column..=m.last_column {
                if (r, c) != (m.first_row, m.first_column) {
                    covered.insert((r, c));
                }
            }
        }
    }
    // Cells holding a value or a formula stop text running on from beside them (only the part's own rows matter: the text runs along a row).
    let filled: std::collections::HashSet<(u32, u32)> =
        part_cells.iter().filter(|c| !c.text.is_empty() || c.formula.is_some()).map(|c| (c.row, c.column)).chain(covered.iter().copied()).collect();

    let mut out = Layout {
        fills: Vec::new(),
        gradients: Vec::new(),
        grid: Vec::new(),
        cells: Vec::new(),
        cell_bands: Default::default(),
        items: Default::default(),
        measured: Default::default(),
        borders: Vec::new(),
        vertical_borders: Vec::new(),
        diagonals: Vec::new(),
        diagonal_shapes: Vec::new(),
        dashed_lines: Vec::new(),
        comments: Vec::new(),
        bands: Default::default(),
    };
    let fill_sources = fill_sources(sheet, part_cells);
    let columns = sheet.columns.len() as u32;
    let mut center_across_inner: std::collections::HashSet<(u32, u32)> = Default::default();
    for cell in part_cells {
        if covered.contains(&(cell.row, cell.column)) {
            continue;
        }
        let style = style_of(cell.style);
        let range = merge_at.get(&(cell.row, cell.column)).copied().unwrap_or(CellRange {
            first_row: cell.row,
            first_column: cell.column,
            last_row: cell.row,
            last_column: cell.column,
        });
        let (left, top) = (x(range.first_column), y(range.first_row));
        let (right, bottom) = (x(range.last_column + 1), y(range.last_row + 1));
        // Where a cell has a fill, it covers the grid line before the cell
        // (`[left - 1, right] x [top - 1, bottom]`, in points when printed and
        // in pixels on screen) so default grid lines never show around a
        // filled cell and borders painted on `[grid - t, grid]` meet the fill
        // without a 1px gap or overlap.
        let fill_unit = if ctx.print { px_per_point } else { 1.0 };
        let (grow_left, grow_top) = (1.0, 1.0);
        // A page's first row and column: the fill does not reach back over its edge (it is the previous page's).
        let (grow_left, grow_top) = (
            if ctx.print && ctx.breaks.1.contains(&xs[(range.first_column as usize).min(xs.len() - 1)]) { 0.0 } else { grow_left },
            if ctx.print && ctx.breaks.0.contains(&ys[(range.first_row as usize).min(ys.len() - 1)]) { 0.0 } else { grow_top },
        );
        let fill_left = if range.first_column == 0 { left } else { left - grow_left * fill_unit };
        let fill_top = if range.first_row == 0 { top } else { top - grow_top * fill_unit };
        if bottom <= top || right <= left {
            // A hidden row or column (one a filter hides): no fill, though
            // the fill's grown edge would show as a line (XAF-00003).
        } else if let Some(grad) = &style.gradient {
            out.gradients.push(((fill_left, fill_top, right - fill_left, bottom - fill_top), grad.to_fill()));
        } else if let Some((preset, fg, bg)) = &style.pattern {
            out.fills.push(Quad { rect: (fill_left, fill_top, right - fill_left, bottom - fill_top), color: hsla(opaque(*bg)) });
            // Excel's tiles for a cell's patterns (not PowerPoint's), 1 pt per cell.
            let tile = ibis_doc_model::spreadsheet_pattern_of_preset(preset).and_then(ibis_doc_model::spreadsheet_pattern_tile).or_else(|| ibis_doc_model::pattern_tile(preset));
            if let Some(tile) = tile {
                let fg_color = hsla(opaque(*fg));
                // Printed, the tile is anchored at the paper's corner and its cells are paper points (not the sheet's, which
                // the print scale shrinks): the cell a paper point `i` along and `j` down is `tile[(j + 8 - height % 8) % 8]`'s
                // bit `i % 8` (Excel's PDFs of xlsx-XFIL: the tile is laid up from the paper's bottom edge). On screen
                // the cells are the sheet's points from the sheet's corner.
                let paper = PRINT_GRID.with(|g| g.get());
                let (cell, ox, oy, row_shift) = match paper {
                    Some((sc, kx, ky)) => {
                        let height = (page_height_emu as f64 / 12_700.0).round() as i64;
                        (px_per_point / sc, sc * kx, sc * ky, (8 - height.rem_euclid(8)).rem_euclid(8))
                    }
                    None => (px_per_point, 0.0, 0.0, 0),
                };
                let first_row = ((fill_top / cell) + oy).floor() as i64;
                let last_row = (((bottom / cell) + oy)).ceil() as i64;
                let first_col = ((fill_left / cell) + ox).floor() as i64;
                let last_col = (((right / cell) + ox)).ceil() as i64;
                for r in first_row..last_row {
                    let bits = tile[(r + row_shift).rem_euclid(8) as usize];
                    if bits == 0 { continue; }
                    let y0 = ((r as f32 - oy) * cell).max(fill_top);
                    let y1 = (((r + 1) as f32 - oy) * cell).min(bottom);
                    if y1 <= y0 { continue; }
                    let mut c = first_col;
                    while c < last_col {
                        if bits & (0x80 >> c.rem_euclid(8)) != 0 {
                            let start = c;
                            while c + 1 < last_col && (bits & (0x80 >> (c + 1).rem_euclid(8)) != 0) {
                                c += 1;
                            }
                            let x0 = ((start as f32 - ox) * cell).max(fill_left);
                            let x1 = (((c + 1) as f32 - ox) * cell).min(right);
                            if x1 > x0 {
                                out.fills.push(Quad { rect: (x0, y0, x1 - x0, y1 - y0), color: fg_color });
                            }
                        }
                        c += 1;
                    }
                }
            }
        } else if let Some(color) = style.fill {
            out.fills.push(Quad { rect: (fill_left, fill_top, right - fill_left, bottom - fill_top), color: hsla(opaque(color)) });
        }
        let display = fill_sources.get(&(cell.row, cell.column)).copied().unwrap_or(cell);
        // Printed, Excel leaves out text of more than 32 pt that is well over its row's height (its PDFs of xlsx-XFNT:
        // a 26 pt row shows 28 pt text but nothing of 36, 48, 72, 96 and 409 pt text; xlsx-XBASE: a 15 pt row shows
        // its 28 pt text cut at the row's edges, so it is not the ratio alone; between 28 and 36 pt is not measured).
        let too_big = ctx.print && style.font_size > 3200 && style.font_size as f32 / 100.0 * px_per_point > (bottom - top) * 1.3;
        // IMAGE: the picture is drawn over the cell (its text is not), or the error Excel shows when it is not there.
        // While it is fetched Excel shows #BUSY!.
        let image_failure = ctx.images.get(&(cell.row, cell.column)).and_then(|u| match crate::render::web_image::status(u) {
            crate::render::web_image::Status::Failed(f) => Some(f.error()),
            crate::render::web_image::Status::Pending => Some(ibis_calc::CellError::Busy),
            crate::render::web_image::Status::Ready => None,
        });
        let hidden_by_image = ctx.images.contains_key(&(cell.row, cell.column)) && image_failure.is_none();
        let is_formula_display = sheet.show_formulas && cell.formula.is_some();
        let shown_text = if let Some(e) = image_failure {
            e.as_str().to_string()
        } else {
            cell_shown_text(sheet, display)
        };
        if !shown_text.is_empty() && bottom > top && right > left && !too_big && !hidden_by_image {
            // How far the text may run on: over the empty, unmerged cells
            // beside it, up to the sheet's edge (left, right, both).
            let run_on = |dir: i32| {
                let (mut from, mut to) = (range.first_column, range.last_column);
                if dir <= 0 {
                    while from > 0 && !filled.contains(&(cell.row, from - 1)) && range.first_row == range.last_row {
                        from -= 1;
                    }
                }
                if dir >= 0 {
                    while to + 1 < columns && !filled.contains(&(cell.row, to + 1)) && range.first_row == range.last_row {
                        to += 1;
                    }
                }
                // Printed text can reach unused columns beyond the stored grid.
                // The export measures its actual shaped end before choosing pages.
                let end = if ctx.print && dir >= 0 && to + 1 == columns && range.first_row == range.last_row { 16_384 } else { to + 1 };
                (x(from), x(end))
            };
            // The number format's colour ([Red]) over the font's.
            let mut style = style.clone();
            if !is_formula_display {
                // A number format names a colour only in brackets ([Red], [Color 3]): without a bracket there is nothing to work out.
                if style.number_format.contains('[')
                    && let Some(c) = ibis_calc::numfmt::format(&display.value, &style.number_format, false).color
                {
                    style.color = ibis_doc_model::Rgb(c.rgb());
                }
            }
            let general = match display.value {
                ibis_doc_model::CellValue::Number(n) if !is_formula_display && style.number_format.eq_ignore_ascii_case("General") => Some(n),
                _ => None,
            };
            let pivot_pad = pivot_pads.get(&(cell.row, cell.column)).map(|pt| {
                style.indent = 0;
                pt * px_per_point
            });
            let style = &style;
            // Centred across the selection (centerContinuous): the cell and the
            // empty cells after it that are centred across too are one box.
            if style.align == CellAlign::CenterAcross && range.first_row == range.last_row {
                let mut to = range.last_column;
                while to + 1 < columns
                    && !filled.contains(&(cell.row, to + 1))
                    && sheet.cell(cell.row, to + 1).is_some_and(|next| style_of(next.style).align == CellAlign::CenterAcross)
                {
                    to += 1;
                    center_across_inner.insert((cell.row, to));
                }
            }
            let across_right = if style.align == CellAlign::CenterAcross && range.first_row == range.last_row {
                let mut to = range.last_column;
                while to + 1 < columns
                    && !filled.contains(&(cell.row, to + 1))
                    && sheet.cell(cell.row, to + 1).is_some_and(|next| style_of(next.style).align == CellAlign::CenterAcross)
                {
                    to += 1;
                }
                x(to + 1)
            } else {
                right
            };
            out.cells.push(CellText {
                text: shown_text,
                kind: if image_failure.is_some() { CellKind::Error } else if is_formula_display { CellKind::Text } else { display.kind },
                general,
                style: style.clone(),
                font: ctx.font(cell.style),
                rect: (left + pivot_pad.unwrap_or(0.0), top, across_right, bottom),
                spans: [run_on(-1), run_on(0), run_on(1)],
                row_gap: None,
                runs: if image_failure.is_some() || is_formula_display { None } else { cell.runs.clone() },
                phonetic: sheet.show_phonetic.then(|| cell.phonetic.clone()).flatten(),
            });
        }
        let diag_up = style.diagonal_up.as_ref().or_else(|| {
            if merge_at.contains_key(&(cell.row, cell.column)) {
                for r in range.first_row..=range.last_row {
                    for c in range.first_column..=range.last_column {
                        if (r, c) != (range.first_row, range.first_column) {
                            if let Some(sc) = sheet.cell(r, c) {
                                if let Some(d) = &style_of(sc.style).diagonal_up {
                                    return Some(d);
                                }
                            }
                        }
                    }
                }
            }
            None
        });
        let diag_down = style.diagonal_down.as_ref().or_else(|| {
            if merge_at.contains_key(&(cell.row, cell.column)) {
                for r in range.first_row..=range.last_row {
                    for c in range.first_column..=range.last_column {
                        if (r, c) != (range.first_row, range.first_column) {
                            if let Some(sc) = sheet.cell(r, c) {
                                if let Some(d) = &style_of(sc.style).diagonal_down {
                                    return Some(d);
                                }
                            }
                        }
                    }
                }
            }
            None
        });
        for (b, up) in [(diag_up, true), (diag_down, false)] {
            let Some(b) = b else { continue };
            // Solid lines: the outline Excel for Mac prints (its PDFs of
            // xlsx-XBRD), a pen `pw` wide and `ph` tall drawn along the
            // diagonal, the whole within [left - ½, right - ½] ×
            // [top - ½, bottom - ½] pt: thin 1 × 0, medium 2 × 1, thick 4 × 1,
            // filled and its outline stroked 1 pt wide;
            // double the thick one with a thin line of the cell's
            // background down its middle.
            let pen = match b.line {
                BorderLine::Thin => Some((1.0, 0.0)),
                BorderLine::Medium => Some((2.0, 1.0)),
                BorderLine::Thick | BorderLine::Double => Some((4.0, 1.0)),
                _ => None,
            };
            if let Some((pw, ph)) = pen {
                let p = px_per_point;
                let outline = |pw: f32, ph: f32| -> Vec<(f32, f32)> {
                    let (hw, hh) = (pw * p / 2.0, ph * p / 2.0);
                    let (x0, x1) = (left - 0.5 * p, right - 0.5 * p);
                    let (y0, y1) = (top - 0.5 * p, bottom - 0.5 * p);
                    let s = (x0 + hw, y1 - hh);
                    let e = (x1 - hw, y0 + hh);
                    let pts = [(e.0 + hw, e.1 - hh), (e.0 - hw, e.1 - hh), (s.0 - hw, s.1 - hh), (s.0 - hw, s.1 + hh), (s.0 + hw, s.1 + hh), (e.0 + hw, e.1 + hh)];
                    // Down: the up line mirrored across the cell's middle.
                    pts.iter().map(|&(x, y)| if up { (x, y) } else { (x0 + x1 - x, y) }).collect()
                };
                out.diagonal_shapes.push((outline(pw, ph), px_per_point, hsla(opaque(b.color))));
                if b.line == BorderLine::Double {
                    let gap = style.fill.unwrap_or(ibis_doc_model::Rgb::WHITE);
                    out.diagonal_shapes.push((outline(1.0, 0.0), px_per_point, hsla(opaque(gap))));
                }
                continue;
            }
            // Hair, printed: the thin line's outline (pen 1 × 0, not
            // stroked) filled with the paper's checkerboard of points, as
            // the straight hair line (Excel's PDFs of xlsx-XBRD: a pattern
            // fill of that outline).
            if ibis_pptx::EXCEL_METRICS == ibis_pptx::ExcelMetrics::Mac
                && let (BorderLine::Hair, Some((sc, kx, ky))) = (b.line, PRINT_GRID.with(|g| g.get()))
            {
                let p = px_per_point;
                let (x0, x1) = (left / p - 0.5, right / p - 0.5);
                let (y0, y1) = (top / p - 0.5, bottom / p - 0.5);
                let pts = [(x1 + 0.0, y0), (x1 - 1.0, y0), (x0, y1), (x0 + 1.0, y1)];
                let outline: Vec<(f32, f32)> = pts.iter().map(|&(x, y)| if up { (x, y) } else { (x0 + x1 - x, y) }).collect();
                let color = hsla(opaque(b.color));
                let cell = |v: f32, k: f32| (sc * (k + v)).floor() as i64;
                for i in cell(x0, kx)..=cell(x1, kx) {
                    for j in cell(y0, ky)..=cell(y1, ky) {
                        if (i + j).rem_euclid(2) != 0 {
                            continue;
                        }
                        let (ca, cb) = (i as f32 / sc - kx, (i + 1) as f32 / sc - kx);
                        let (cc, cd) = (j as f32 / sc - ky, (j + 1) as f32 / sc - ky);
                        let piece = clip_to_box(&outline, (ca, cc, cb, cd));
                        if piece.len() >= 3 {
                            out.diagonal_shapes.push((piece.iter().map(|&(x, y)| (x * p, y * p)).collect(), 0.0, color));
                        }
                    }
                }
                continue;
            }
            // Dashed and dotted ones, printed: the same pen drawn along the
            // pieces of the pattern, which Excel counts along x (not along
            // the line), from the line's top end, a point outside the
            // outline's box (its PDFs of xlsx-XBRD: a right-down `dashDot`
            // from x = left − 1½ pt, [9, 3, 3, 3] there; a right-up one the
            // same from the right).
            let dashed_pen = match b.line {
                BorderLine::Dashed | BorderLine::Dotted | BorderLine::DashDot | BorderLine::DashDotDot => Some((0.0, 1.0)),
                BorderLine::MediumDashed | BorderLine::MediumDashDot | BorderLine::MediumDashDotDot => Some((1.0, 2.0)),
                // slantDashDot: a pen 3 × 1, its pieces those of the thin
                // lines a point to the right (Excel's PDFs of xlsx-XBRD).
                // (Right-up not measured to the point yet: as before.)
                BorderLine::SlantDashDot if up => Some((1.0, 2.0)),
                BorderLine::SlantDashDot => Some((3.0, 1.0)),
                _ => None,
            };
            if let (Some((pw, ph)), true) = (dashed_pen, ctx.print) {
                // Each piece [a, z] (x, from the top end) an outline whose
                // top edge runs at y0 + ceil((x − x0 − pw) · h / w) and whose
                // bottom edge is `ph` lower and `pw` back (thin 0 × 1, medium
                // 1 × 2), filled and stroked 1 pt (measured on the PDFs).
                let p = px_per_point;
                let pattern: &[f32] = match b.line {
                    BorderLine::SlantDashDot => &[9.0, 3.0, 3.0, 3.0],
                    line => dash_pattern(line).unwrap_or(&[]),
                };
                let (x0, x1) = ((left / p - 0.5), (right / p - 0.5));
                let (y0, y1) = ((top / p - 0.5), (bottom / p - 0.5));
                let (w, h) = (x1 - x0, (y1 - y0).max(0.0));
                let slant = b.line == BorderLine::SlantDashDot && !up;
                let old_slant = b.line == BorderLine::SlantDashDot && up;
                let pe = if slant { 0.0 } else { pw };
                let edge = |x: f32| y0 + ((x - x0 - pe) * h / w - 1e-3).ceil();
                // Medium pens (not slantDashDot): the start's top on the
                // whole point at or above the slope, and a right-up line not
                // moved up (measured on the PDFs).
                let medium_rule = pw > 0.0 && b.line != BorderLine::SlantDashDot;
                let edge_start = |x: f32| if medium_rule { y0 + ((x - x0 - pw) * h / w + 1e-3).floor() } else if b.line != BorderLine::Dashed && !old_slant { y0 + ((x - 0.5 - x0 - pe) * h / w - 1e-3).ceil() } else { edge(x) };
                let edge_end = |x: f32| if medium_rule { y0 + ((x - x0 - pw) * h / w + 0.5 - 1e-3).floor() } else if !matches!(b.line, BorderLine::Dashed | BorderLine::Dotted) && !old_slant { y0 + ((x - 0.5 - x0 - pe) * h / w - 1e-3).ceil() } else { edge(x) };
                let color = hsla(opaque(b.color));
                let mut at = x0 - 1.0;
                let mut first_period = true;
                'pieces: loop {
                    for (i, &len) in pattern.iter().enumerate() {
                        let (a, z) = (at, at + len);
                        at = z;
                        if a >= x1 {
                            break 'pieces;
                        }
                        if i % 2 == 1 {
                            continue;
                        }
                        // In the first round of the pattern, the pieces after
                        // the first start a point later (Excel's PDFs of
                        // xlsx-XBRD: the dots of dashDot and dashDotDot).
                        let a = if first_period && i > 0 && !old_slant { a + 1.0 } else { a };
                        let z_whole = z;
                        let (a, z) = (a.max(x0), z.min(x1));
                        if z <= a {
                            continue;
                        }
                        // A thin dash cut at the line's end keeps the slope of
                        // its whole length there (Excel's PDFs of xlsx-XBRD).
                        let end_y = if b.line == BorderLine::Dashed { edge_end(z_whole) } else { edge_end(z) };
                        let shift = if slant { 1.0 } else { 0.0 };
                        let pts = [(a + shift, edge_start(a)), (z + shift, end_y), (z + shift - pw, end_y + ph), (a + shift - pw, edge_start(a) + ph)];
                        // Right-up: the right-down one mirrored, a point up and
                        // the pen's width left (measured on the PDFs).
                        let rise = if medium_rule { 0.0 } else { 1.0 };
                        let outline = pts.iter().map(|&(x, y)| if up { ((x0 + x1 - x - pw) * p, (y - rise) * p) } else { (x * p, y * p) }).collect();
                        out.diagonal_shapes.push((outline, p, color));
                    }
                    first_period = false;
                }
                continue;
            }
            let (from, to) = if up { ((left, bottom), (right, top)) } else { ((left, top), (right, bottom)) };
            {
                let dx = to.0 - from.0;
                let dy = to.1 - from.1;
                let len = (dx * dx + dy * dy).sqrt();
                let (from, to) = if len > 0.0 && b.line.width() <= 1.0 {
                    let w = (b.line.width() * px_per_point).max(thinnest_line());
                    let min_trig = (dx.abs() / len).min(dy.abs() / len).max(0.2);
                    let ext = ((0.5 * w) / min_trig).min(2.0 * px_per_point);
                    let ux = dx / len * ext;
                    let uy = dy / len * ext;
                    let f = (from.0 - ux - 0.5 * px_per_point, (from.1 - uy).min(bottom));
                    let t = (to.0 + ux + 0.5 * px_per_point, (to.1 + uy).min(bottom));
                    (f, t)
                } else {
                    (from, to)
                };
                out.diagonals.push((from, to, (b.line.width() * px_per_point).max(thinnest_line()), hsla(opaque(b.color))));
            }
        }
    }
    // Conditional formats' data bars and icons, over the fills.
    if let Some(o) = overlay {
        for bar in o.bars.iter().filter(|b| in_part(b.cell.0)) {
            let (r, c) = bar.cell;
            let (left, right, top, bottom) = (x(c), x(c + 1), y(r), y(r + 1));
            let (w, inset) = (right - left, px_per_point.max(1.0));
            let (from, to) = if bar.mirrored { (1.0 - bar.to, 1.0 - bar.from) } else { (bar.from, bar.to) };
            let a = left + inset + (w - 2.0 * inset) * from;
            let z = left + inset + (w - 2.0 * inset) * to;
            // A gradient bar fades from its colour to white at its end, in steps.
            let (h, steps) = ((bottom - top - 2.0 * inset).max(0.0), if bar.gradient { 32 } else { 1 });
            let width = (z - a).max(0.0);
            for k in 0..steps {
                let fade_ratio = if bar.is_negative {
                    0.9 * (steps - 1 - k) as f32 / (steps - 1).max(1) as f32
                } else {
                    0.9 * k as f32 / (steps - 1).max(1) as f32
                };
                let shade = if bar.gradient { mix_white(bar.color, fade_ratio) } else { bar.color };
                let (s0, s1) = (a + width * k as f32 / steps as f32, a + width * (k + 1) as f32 / steps as f32);
                out.fills.push(Quad { rect: (s0, top + inset, (s1 - s0).max(0.0), h), color: hsla(opaque(shade)) });
            }
            // The border: a line of 0.75 pt round the bar.
            if let (Some(border), true) = (bar.border, width > 0.0) {
                let t = (0.75 * px_per_point).min(width / 2.0).min(h / 2.0);
                let c = hsla(opaque(border));
                let top_y = top + inset;
                out.fills.push(Quad { rect: (a, top_y, width, t), color: c });
                out.fills.push(Quad { rect: (a, top_y + h - t, width, t), color: c });
                out.fills.push(Quad { rect: (a, top_y + t, t, (h - 2.0 * t).max(0.0)), color: c });
                out.fills.push(Quad { rect: (z - t, top_y + t, t, (h - 2.0 * t).max(0.0)), color: c });
            }
            if let Some(axis_pos) = bar.axis {
                let axis_pos = if bar.mirrored { 1.0 - axis_pos } else { axis_pos };
                let axis_x = left + inset + (w - 2.0 * inset) * axis_pos;
                out.fills.push(Quad {
                    rect: (axis_x - 0.5 * px_per_point, top + inset, px_per_point.max(1.0), h),
                    color: hsla(opaque(bar.axis_color)),
                });
            }
        }
        for &((r, c), color, shape) in o.icons.iter().filter(|i| in_part(i.0 .0)) {
            let (left, top, bottom) = (x(c), y(r), y(r + 1));
            let size = ((bottom - top) * 0.6).min(11.0 * px_per_point);
            let (x0, y0) = (left + 2.0 * px_per_point, top + (bottom - top - size) / 2.0);
            // The icon's outline in a unit square: arrows, geometric shapes, symbols, flags, ratings.
            let unit: Vec<(f32, f32)> = match shape {
                1 => vec![(0.5, 0.0), (1.0, 0.5), (0.68, 0.5), (0.68, 1.0), (0.32, 1.0), (0.32, 0.5), (0.0, 0.5)],
                2 => vec![(1.0, 0.5), (0.5, 0.0), (0.5, 0.32), (0.0, 0.32), (0.0, 0.68), (0.5, 0.68), (0.5, 1.0)],
                3 => vec![(0.5, 1.0), (1.0, 0.5), (0.68, 0.5), (0.68, 0.0), (0.32, 0.0), (0.32, 0.5), (0.0, 0.5)],
                4 => vec![(1.0, 0.0), (1.0, 0.55), (0.78, 0.33), (0.35, 0.76), (0.24, 0.65), (0.67, 0.22), (0.45, 0.0)],
                5 => vec![(1.0, 1.0), (0.45, 1.0), (0.67, 0.78), (0.24, 0.35), (0.35, 0.24), (0.78, 0.67), (1.0, 0.45)],
                6 => vec![(0.5, 0.05), (0.95, 0.95), (0.05, 0.95)],
                7 => vec![(0.5, 0.05), (0.95, 0.5), (0.5, 0.95), (0.05, 0.5)],
                8 => vec![(0.12, 0.05), (0.88, 0.35), (0.12, 0.65), (0.12, 0.95), (0.05, 0.95), (0.05, 0.05)],
                9 => vec![(0.38, 0.92), (0.95, 0.15), (0.82, 0.05), (0.35, 0.68), (0.15, 0.48), (0.05, 0.58)],
                10 => vec![(0.12, 0.26), (0.36, 0.5), (0.12, 0.74), (0.26, 0.88), (0.5, 0.64), (0.74, 0.88), (0.88, 0.74), (0.64, 0.5), (0.88, 0.26), (0.74, 0.12), (0.5, 0.36), (0.26, 0.12)],
                11 => vec![(0.4, 0.05), (0.6, 0.05), (0.56, 0.62), (0.44, 0.62), (0.44, 0.75), (0.56, 0.75), (0.56, 0.95), (0.44, 0.95)],
                12 => vec![(0.1, 0.1), (0.9, 0.1), (0.9, 0.9), (0.1, 0.9)],
                13 => vec![(0.1, 0.7), (0.25, 0.7), (0.25, 0.9), (0.1, 0.9)],
                14 => vec![(0.1, 0.7), (0.25, 0.7), (0.25, 0.5), (0.45, 0.5), (0.45, 0.9), (0.1, 0.9)],
                15 => vec![(0.1, 0.7), (0.25, 0.7), (0.25, 0.5), (0.45, 0.5), (0.45, 0.3), (0.65, 0.3), (0.65, 0.9), (0.1, 0.9)],
                16 => vec![(0.1, 0.7), (0.25, 0.7), (0.25, 0.5), (0.45, 0.5), (0.45, 0.3), (0.65, 0.3), (0.65, 0.1), (0.85, 0.1), (0.85, 0.9), (0.1, 0.9)],
                17 => vec![(0.05, 0.8), (0.2, 0.8), (0.2, 0.6), (0.4, 0.6), (0.4, 0.4), (0.6, 0.4), (0.6, 0.2), (0.8, 0.2), (0.8, 0.05), (0.95, 0.05), (0.95, 0.95), (0.05, 0.95)],
                18 => (0..20).map(|k| { let a = k as f32 / 20.0 * std::f32::consts::TAU; (0.5 + 0.45 * a.cos(), 0.5 + 0.45 * a.sin()) }).collect(),
                19 => vec![(0.5, 0.5), (0.5, 0.05), (0.95, 0.05), (0.95, 0.5)],
                20 => vec![(0.5, 0.05), (0.95, 0.05), (0.95, 0.95), (0.5, 0.95)],
                21 => vec![(0.5, 0.05), (0.95, 0.05), (0.95, 0.95), (0.05, 0.95), (0.05, 0.5), (0.5, 0.5)],
                // A five-pointed star, a triangle pointing down, a bar.
                23 => (0..10).map(|k| { let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0; let r = if k % 2 == 0 { 0.5 } else { 0.2 }; (0.5 + r * a.cos(), 0.55 + r * a.sin()) }).collect(),
                24 => vec![(0.05, 0.1), (0.95, 0.1), (0.5, 0.9)],
                // A box filled by none, a quarter, a half, three quarters and all of it (5Boxes).
                26 => vec![(0.1, 0.1), (0.9, 0.1), (0.9, 0.9), (0.1, 0.9), (0.1, 0.8), (0.8, 0.8), (0.8, 0.2), (0.2, 0.2), (0.2, 0.8), (0.1, 0.8)],
                27 => vec![(0.1, 0.5), (0.5, 0.5), (0.5, 0.9), (0.1, 0.9)],
                28 => vec![(0.1, 0.5), (0.9, 0.5), (0.9, 0.9), (0.1, 0.9)],
                29 => vec![(0.1, 0.1), (0.5, 0.1), (0.5, 0.5), (0.9, 0.5), (0.9, 0.9), (0.1, 0.9)],
                30 => vec![(0.1, 0.1), (0.9, 0.1), (0.9, 0.9), (0.1, 0.9)],
                25 => vec![(0.05, 0.4), (0.95, 0.4), (0.95, 0.6), (0.05, 0.6)],
                _ => (0..20).map(|k| { let a = k as f32 / 20.0 * std::f32::consts::TAU; (0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin()) }).collect(),
            };
            let polygon = unit.into_iter().map(|(u, v)| (x0 + u * size, y0 + v * size)).collect();
            out.diagonal_shapes.push((polygon, 0.0, hsla(opaque(color))));
        }
    }
    // Borders: unmerged cells and merged ranges draw their own outer edges.
    let mut in_merge: HashMap<(u32, u32), CellRange> = HashMap::new();
    let mut merge_fallback_tl: std::collections::HashSet<(u32, u32)> = Default::default();
    for m in &sheet.merges {
        let mut has_covered = false;
        for r in m.first_row..=m.last_row {
            for c in m.first_column..=m.last_column {
                in_merge.insert((r, c), *m);
                if (r, c) != (m.first_row, m.first_column) && sheet.cell(r, c).is_some() {
                    has_covered = true;
                }
            }
        }
        if !has_covered {
            merge_fallback_tl.insert((m.first_row, m.first_column));
        }
    }
    let row_break = |r: u32| ctx.print && r > 0 && (r as usize) < ys.len() && ctx.breaks.0.contains(&ys[r as usize]);
    let col_break = |c: u32| ctx.print && c > 0 && (c as usize) < xs.len() && ctx.breaks.1.contains(&xs[c as usize]);
    let side_of = |s: &CellStyle, edge: u8| -> Option<ibis_doc_model::Border> {
        match edge {
            0 => s.top,
            1 => s.bottom,
            2 => s.left,
            _ => s.right,
        }
    };
    let border_at = |r: Option<u32>, c: Option<u32>, edge: u8| -> Option<ibis_doc_model::Border> {
        let (r, c) = (r?, c?);
        if let Some(m) = in_merge.get(&(r, c)) {
            let on_perimeter = match edge {
                0 => r == m.first_row,
                1 => r == m.last_row,
                2 => c == m.first_column,
                _ => c == m.last_column,
            };
            if !on_perimeter {
                return None;
            }
            let own = sheet.cell(r, c).and_then(|cell| side_of(style_of(cell.style), edge));
            if own.is_some() || !merge_fallback_tl.contains(&(m.first_row, m.first_column)) {
                return own;
            }
            return sheet.cell(m.first_row, m.first_column).and_then(|cell| side_of(style_of(cell.style), edge));
        }
        if (edge == 2 && center_across_inner.contains(&(r, c)))
            || (edge == 3 && center_across_inner.contains(&(r, c + 1)))
        {
            return None;
        }
        sheet.cell(r, c).and_then(|cell| side_of(style_of(cell.style), edge))
    };
    let neighbour_edge = |r: u32, c: u32, edge: u8| -> Option<ibis_doc_model::Border> {
        match edge {
            0 => (!row_break(r)).then(|| border_at(r.checked_sub(1), Some(c), 1)).flatten(),
            1 => (!row_break(r + 1)).then(|| border_at(Some(r + 1), Some(c), 0)).flatten(),
            2 => (!col_break(c)).then(|| border_at(Some(r), c.checked_sub(1), 3)).flatten(),
            _ => (!col_break(c + 1)).then(|| border_at(Some(r), Some(c + 1), 2)).flatten(),
        }
    };
    let beaten = |own: &Option<ibis_doc_model::Border>, other: Option<ibis_doc_model::Border>, other_first: bool| match (own, other) {
        (Some(o), Some(n)) => {
            let (a, b) = (border_rank(o.line), border_rank(n.line));
            b > a || (b == a && other_first)
        }
        _ => false,
    };
    for cell in part_cells {
        let (r, c) = (cell.row, cell.column);
        if in_merge.contains_key(&(r, c)) {
            continue;
        }
        let own_top = border_at(Some(r), Some(c), 0);
        let own_bottom = border_at(Some(r), Some(c), 1);
        let own_left = border_at(Some(r), Some(c), 2);
        let own_right = border_at(Some(r), Some(c), 3);
        let (left, right, top, bottom) = (x(c), x(c + 1), y(r), y(r + 1));
        // Excel's PDF of xlsx-XBRD: a row's line runs [left - 1, right] pt,
        // whatever its style and the cell's other lines; a column's line
        // from the top of the line above its top end ([top − 1] for none or
        // a thin one, top − 2 for a medium or thick one) down to the
        // bottom of the line below its bottom end (bottom, or bottom + 1
        // under a thick or double one).
        let effective_top_border = own_top.or_else(|| neighbour_edge(r, c, 0));
        let effective_bottom_border = own_bottom.or_else(|| neighbour_edge(r, c, 1));
        let border_unit = if ctx.print { px_per_point } else { 1.0 };
        let line_units = |line: BorderLine| if ctx.print { line.width() } else { screen_border_pixels(line) };
        let near = |b: &Option<ibis_doc_model::Border>| b.map_or(1.0, |b| line_units(b.line).min(2.0).max(1.0)) * border_unit;
        let far = |b: &Option<ibis_doc_model::Border>| b.map_or(0.0, |b| (line_units(b.line) - line_units(b.line).min(2.0)).max(0.0)) * border_unit;
        // Where lines meet, the left one reaches over the top one's end
        // and the top one starts at the grid line; the right one starts
        // under the top line, which runs on over it (Excel's PDF of
        // xlsx-XBRD, "all" of each style: the dashes there show it).
        // border_quads takes a column's line from a point above `from`.
        let effective_left_line = own_left.or_else(|| neighbour_edge(r, c, 2));
        let effective_right_line = own_right.or_else(|| neighbour_edge(r, c, 3));
        let v_from = top - (near(&effective_top_border) - border_unit);
        let v_from_right = if effective_top_border.is_some() { top + border_unit + far(&effective_top_border) } else { top };
        let v_to = bottom + far(&effective_bottom_border);
        let h_from = if effective_left_line.is_some() { left + far(&effective_left_line) } else { left - border_unit };
        let h_to = right + far(&effective_right_line);
        let edges = [
            (&own_top, true, row_break(r), top, h_from, h_to, beaten(&own_top, neighbour_edge(r, c, 0), true)),
            (&own_bottom, true, false, bottom, h_from, h_to, beaten(&own_bottom, neighbour_edge(r, c, 1), false)),
            (&own_left, false, col_break(c), left, v_from, v_to, beaten(&own_left, neighbour_edge(r, c, 2), true)),
            (&own_right, false, false, right, v_from_right, v_to, beaten(&own_right, neighbour_edge(r, c, 3), false)),
        ];
        // Where two double lines meet, Excel draws two boxes: the outer
        // lines run on to the outer corner, the inner ones stop at the inner
        // one (its PDF of xlsx-XBRD, "double all").
        let is_double = |b: &Option<ibis_doc_model::Border>| b.is_some_and(|b| b.line == BorderLine::Double);
        let effective_left = own_left.or_else(|| neighbour_edge(r, c, 2));
        let effective_right = own_right.or_else(|| neighbour_edge(r, c, 3));
        let corners = [
            (is_double(&effective_left), is_double(&effective_right)),
            (is_double(&effective_left), is_double(&effective_right)),
            (is_double(&effective_top_border), is_double(&effective_bottom_border)),
            (is_double(&effective_top_border), is_double(&effective_bottom_border)),
        ];
        for (i, (b, horizontal, at_page_start, at, from, to, beaten)) in edges.into_iter().enumerate() {
            if beaten {
                continue;
            }
            if let Some(b) = b {
                let lines = if horizontal { &mut out.borders } else { &mut out.vertical_borders };
                let (start_double, end_double) = corners[i];
                if b.line == BorderLine::Double && (start_double || end_double) {
                    let p = border_unit;
                    let color = hsla(opaque(b.color));
                    // The line nearer the cell's start side ([at − 2, at − 1])
                    // and the one after it ([at, at + 1]); which is outer
                    // depends on the side (top and left: the first).
                    let first_outer = i == 0 || i == 2;
                    let (lo, hi) = if horizontal { (left, right) } else { (top, bottom) };
                    let at_shifted = if at_page_start { at + 2.0 * p } else { at };
                    for (offset, outer) in [(-2.0, first_outer), (0.0, !first_outer)] {
                        let a = if start_double { if outer { lo - 2.0 * p } else { lo } } else if horizontal { from } else { from - p };
                        let z = if end_double { if outer { hi + p } else { hi - p } } else { to };
                        let edge = at_shifted + offset * p;
                        let rect = if horizontal { (a, edge, z - a, p) } else { (edge, a, p, z - a) };
                        lines.push(Quad { rect, color });
                    }
                    continue;
                }
                border_quads(b, horizontal, at_page_start, at, from, to, px_per_point, lines);
            }
        }
    }
    // Merged ranges: draw outer perimeter borders. If perimeter cells define their own
    // borders, use them; otherwise (when covered cells are omitted) fall back to the top-left cell's border styling.
    // Adjacent cells with identical borders along an edge are grouped into contiguous segments.
    for m in sheet.merges.iter().filter(|m| in_part(m.first_row)) {
        // Top edge: at y(m.first_row), from x(m.first_column) to x(m.last_column + 1)
        draw_contiguous_borders(
            m.first_column,
            m.last_column,
            &x,
            |c| {
                let own = border_at(Some(m.first_row), Some(c), 0);
                (!beaten(&own, neighbour_edge(m.first_row, c, 0), true)).then_some(own).flatten()
            },
            true,
            row_break(m.first_row),
            y(m.first_row),
            px_per_point,
            &mut out.borders,
        );

        // Bottom edge: at y(m.last_row + 1), from x(m.first_column) to x(m.last_column + 1)
        draw_contiguous_borders(
            m.first_column,
            m.last_column,
            &x,
            |c| {
                let own = border_at(Some(m.last_row), Some(c), 1);
                (!beaten(&own, neighbour_edge(m.last_row, c, 1), false)).then_some(own).flatten()
            },
            true,
            false,
            y(m.last_row + 1),
            px_per_point,
            &mut out.borders,
        );

        // Left edge: at x(m.first_column), from y(m.first_row) to y(m.last_row + 1)
        draw_contiguous_borders(
            m.first_row,
            m.last_row,
            &y,
            |r| {
                let own = border_at(Some(r), Some(m.first_column), 2);
                (!beaten(&own, neighbour_edge(r, m.first_column, 2), true)).then_some(own).flatten()
            },
            false,
            col_break(m.first_column),
            x(m.first_column),
            px_per_point,
            &mut out.vertical_borders,
        );

        // Right edge: at x(m.last_column + 1), from y(m.first_row) to y(m.last_row + 1)
        draw_contiguous_borders(
            m.first_row,
            m.last_row,
            &y,
            |r| {
                let own = border_at(Some(r), Some(m.last_column), 3);
                (!beaten(&own, neighbour_edge(r, m.last_column, 3), false)).then_some(own).flatten()
            },
            false,
            false,
            x(m.last_column + 1),
            px_per_point,
            &mut out.vertical_borders,
        );
    }
    let show_grid = if ctx.print { sheet.print.grid_lines } else { sheet.grid_lines };
    if show_grid && whole {
        // Printed, black and round the sheet's edge too (Excel for Mac's
        // PDF of xlsx-XPAGE-26: 1 pt black lines, a box round the cells).
        let color = hsla(opaque(ibis_doc_model::Rgb(if ctx.print { 0 } else { GRID_COLOR })));
        let first = if ctx.print { 0 } else { 1 };
        let extra_cols = if !ctx.print { 50 } else { 0 };
        let extra_rows = if !ctx.print { 1000 } else { 0 };
        let total_cols = (columns + extra_cols).min(16_384);
        let total_rows = (sheet.rows.len() as u32 + extra_rows).min(1_048_576);
        let (w, h) = (x(total_cols), y(total_rows));
        let t = 1.0;
        // Both printed and on screen, a grid line takes the unit before the
        // cell's edge (`[grid - back, grid]`), matching thin borders and cell fills.
        let back = if ctx.print { px_per_point } else { t };
        for c in first..=total_cols {
            let cutouts: Vec<(f32, f32)> = sheet
                .merges
                .iter()
                .filter(|m| m.first_column < c && c <= m.last_column)
                .map(|m| (y(m.first_row), y(m.last_row + 1)))
                .collect();
            push_segmented_line(false, x(c) - back, if ctx.print { px_per_point } else { t }, h, cutouts, color, &mut out.grid);
        }
        for r in first..=total_rows {
            let cutouts: Vec<(f32, f32)> = sheet
                .merges
                .iter()
                .filter(|m| m.first_row < r && r <= m.last_row)
                .map(|m| (x(m.first_column), x(m.last_column + 1)))
                .collect();
            push_segmented_line(true, y(r) - back, if ctx.print { px_per_point } else { t }, w, cutouts, color, &mut out.grid);
        }
    }
    if !ctx.print && whole {
        // A cell with a threaded comment shows the thread's mark, not its
        // placeholder note's (Excel 365 keeps a note on such a cell for
        // older versions).
        let threaded = |r: u32, c: u32| sheet.threads.iter().any(|t| t.row == r && t.column == c);
        for thread in &sheet.threads {
            let range = merge_at.get(&(thread.row, thread.column)).copied().unwrap_or(CellRange {
                first_row: thread.row,
                first_column: thread.column,
                last_row: thread.row,
                last_column: thread.column,
            });
            let (right, top) = (x(range.last_column + 1), y(range.first_row));
            if right <= x(range.first_column) || y(range.last_row + 1) <= top {
                continue;
            }
            // Excel 365's purple mark (its colour and size not measured);
            // a resolved thread's in grey.
            let size = 6.0 * px_per_point;
            let color = hsla(opaque(ibis_doc_model::Rgb(if thread.done { 0xA6A6A6 } else { 0x7B3FBF })));
            out.comments.push(((right, top), (right - size, top), (right, top + size), color));
        }
        for comment in sheet.comments.iter().filter(|c| !threaded(c.row, c.column)) {
            let range = merge_at.get(&(comment.row, comment.column)).copied().unwrap_or(CellRange {
                first_row: comment.row,
                first_column: comment.column,
                last_row: comment.row,
                last_column: comment.column,
            });
            let right = x(range.last_column + 1);
            let top = y(range.first_row);
            let size = 4.5 * px_per_point;
            let red = hsla(opaque(ibis_doc_model::Rgb(0xED1C24)));
            out.comments.push(((right, top), (right - size, top), (right, top + size), red));
        }
    }
    // エラー チェック: the green triangle at the top left of a cell with an
    // error not ignored. Each part of rows has those of its own cells (the
    // check of every cell of a sheet at once took 0.5 to 1 s for each sheet
    // of a large workbook switched to).
    if !ctx.print && !whole {
        let green = hsla(opaque(ibis_doc_model::Rgb(0x00B050)));
        let size = 4.5 * px_per_point;
        super::error_check::ensure_rules();
        for (row, column, _) in ibis_doc_model::error_check::flagged_among(sheet, part_cells) {
            let range = merge_at.get(&(row, column)).copied().unwrap_or(CellRange { first_row: row, first_column: column, last_row: row, last_column: column });
            let (left, top) = (x(range.first_column), y(range.first_row));
            if x(range.last_column + 1) <= left || y(range.last_row + 1) <= top {
                continue;
            }
            out.comments.push(((left, top), (left + size, top), (left, top + size), green));
        }
    }
    let rects = |quads: &[Quad]| Bands::new(quads.iter().map(|q| q.rect));
    out.diagonal_shapes.extend(BORDER_SHAPES.with(|s| std::mem::take(&mut *s.borrow_mut())));
    out.dashed_lines.extend(BORDER_DASHES.with(|s| std::mem::take(&mut *s.borrow_mut())));
    row_gaps(&mut out.cells);
    // Each part draws the sparklines in its rows (the base none).
    for group in &sheet.sparklines {
        sparkline_marks(sheet, group, &x, &y, &in_part, px_per_point, &ctx.sheets, &mut out);
    }
    out.bands = [rects(&out.fills), rects(&out.grid), rects(&out.borders), rects(&out.vertical_borders)];
    out
}

/// Items by the bands of the sheet, from top to bottom, that they reach
/// into: a page or screenful of a long sheet paints only what is in it.
#[derive(Default)]
struct Bands {
    bands: Vec<Vec<u32>>,
    /// Each item's first band.
    first: Vec<u32>,
}

impl Bands {
    /// A band's height in pixels.
    const HEIGHT: f32 = 256.0;

    fn band(y: f32) -> usize {
        (y / Self::HEIGHT).max(0.0) as usize
    }

    /// Index the items at `rects` (left, top, width, height).
    fn new(rects: impl Iterator<Item = (f32, f32, f32, f32)>) -> Self {
        let mut out = Self::default();
        for (i, (_, top, _, height)) in rects.enumerate() {
            let (first, last) = (Self::band(top), Self::band(top + height.max(0.0)));
            if out.bands.len() <= last {
                out.bands.resize_with(last + 1, Vec::new);
            }
            for band in &mut out.bands[first..=last] {
                band.push(i as u32);
            }
            out.first.push(first as u32);
        }
        out
    }

    /// The items in the bands between `top` and `bottom`, each once: band
    /// by band, in their order within each.
    fn find(&self, top: f32, bottom: f32) -> Vec<u32> {
        if self.bands.is_empty() || bottom < top {
            return Vec::new();
        }
        let (first, last) = (Self::band(top), Self::band(bottom).min(self.bands.len() - 1));
        let mut out = Vec::new();
        for b in first..=last.max(first) {
            let Some(band) = self.bands.get(b) else { break };
            // An item reaching over several bands comes with the first.
            out.extend(band.iter().copied().filter(|i| b == first || self.first[*i as usize] as usize == b));
        }
        out
    }
}

/// The thinnest a border is drawn, in pixels: half a pixel on screen and in
/// pictures; in a PDF (drawn a pixel a point), under a third of a point, as
/// thin as Excel prints a thin border shrunk to fit its page.
fn thinnest_line() -> f32 {
    if crate::render::chart_view::VECTOR_EXPORT.with(|v| v.get()) { 0.3 } else { 0.5 }
}

// The rectangles of a border line along an edge from `from` to `to` at
// `at`: solid, two thin lines (double), or dots and dashes.
thread_local! {
    /// Where the dash patterns start along a row and a column, in points
    /// from the sheet's origin, for the sheet being laid out (`layout`
    /// sets it: see `dash_anchors`).
    static DASH_ANCHOR: std::cell::Cell<(f32, f32)> = const { std::cell::Cell::new((-39.0, -39.0)) };
    /// For a printed sheet, its scale on the paper and where its origin is
    /// on the paper in sheet points (`dash_anchors`, negated): dotted and
    /// hair lines are drawn on the paper's point grid (`hair_cells`).
    static PRINT_GRID: std::cell::Cell<Option<(f32, f32, f32)>> = const { std::cell::Cell::new(None) };
    /// Outlines `border_quads` makes besides rectangles (slanted dashes),
    /// moved into the layout's `diagonal_shapes`.
    static BORDER_SHAPES: std::cell::RefCell<Vec<(Vec<(f32, f32)>, f32, Hsla)>> = const { std::cell::RefCell::new(Vec::new()) };
    /// The dashed lines of printed borders (`border_quads`), moved into the layout's `dashed_lines`.
    static BORDER_DASHES: std::cell::RefCell<Vec<DashedLine>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Cut a dotted or hair line's rectangle (sheet points, x0, y0, x1, y1) as
/// Excel prints it: patterns of whole points of the paper, from its top-left
/// corner (its PDFs of xlsx-XBRD fill these lines with 8×8 pt tiles in the
/// page's own space): a dotted line along a row on the paper's even points
/// across, down a column on its even points down; a hair line on the
/// paper's checkerboard (column + row even). `grid` is (scale, origin x,
/// origin y) from `PRINT_GRID`.
/// `poly` (convex) cut to the box `(x0, y0, x1, y1)` (Sutherland–Hodgman).
fn clip_to_box(poly: &[(f32, f32)], (x0, y0, x1, y1): (f32, f32, f32, f32)) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = poly.to_vec();
    let sides: [(usize, f32, bool); 4] = [(0, x0, true), (0, x1, false), (1, y0, true), (1, y1, false)];
    for (axis, at, keep_above) in sides {
        let input = std::mem::take(&mut out);
        let n = input.len();
        let inside = |q: (f32, f32)| {
            let v = if axis == 0 { q.0 } else { q.1 };
            if keep_above { v >= at } else { v <= at }
        };
        for k in 0..n {
            let (a, b) = (input[k], input[(k + 1) % n]);
            let (ia, ib) = (inside(a), inside(b));
            if ia {
                out.push(a);
            }
            if ia != ib {
                let (va, vb) = if axis == 0 { (a.0, b.0) } else { (a.1, b.1) };
                let t = (at - va) / (vb - va);
                out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
            }
        }
    }
    out
}

/// A hair line's squares on the paper's checkerboard (`r`: the line's rectangle in sheet points).
fn hair_cells(r: (f32, f32, f32, f32), grid: (f32, f32, f32)) -> Vec<(f32, f32, f32, f32)> {
    let (s, kx, ky) = grid;
    let (x0, y0, x1, y1) = r;
    let cells = |a: f32, b: f32, k: f32| -> Vec<(i64, f32, f32)> {
        let (first, last) = ((s * (k + a)).floor() as i64, (s * (k + b)).ceil() as i64);
        (first..last)
            .filter_map(|i| {
                let (lo, hi) = ((i as f32 / s - k).max(a), ((i + 1) as f32 / s - k).min(b));
                (hi > lo).then_some((i, lo, hi))
            })
            .collect()
    };
    let mut out = Vec::new();
    for (i, xa, xb) in cells(x0, x1, kx) {
        for (j, ya, yb) in cells(y0, y1, ky) {
            if (i + j).rem_euclid(2) == 0 {
                out.push((xa, ya, xb, yb));
            }
        }
    }
    out
}

/// The step, on the paper in points, of the tile Excel 365 fills a dotted border with: 8 x 8 pixels drawn 0.96 pt
/// square (its PDF's pattern: `XStep 0.96`), each row 4 pixels grey and 4 white, from the paper's corner: dots of
/// half the step, a step apart.
const DOT_STEP: f32 = 0.96;

/// The dash pattern of a dotted border printed at `scale` (paper points per sheet point), in pixels (`px_per_point`
/// a sheet point): the lengths of a dot and a gap, and how far into them the line starts, `start` sheet points
/// from the sheet's origin, which is `origin` sheet points from the paper's corner.
fn dotted_pattern(scale: f32, origin: f32, start: f32, px_per_point: f32) -> (Vec<f32>, f32) {
    let px_per_paper_point = px_per_point / scale;
    let half = DOT_STEP / 2.0 * px_per_paper_point;
    (vec![half, half], (scale * (origin + start)).rem_euclid(DOT_STEP) * px_per_paper_point)
}

/// Where Excel starts the dash patterns of a sheet's borders, along a row
/// and down a column, in points from the sheet's origin: at the paper's
/// top-left corner, counted in the sheet's points, the sheet printed at
/// `scale`. Excel puts the sheet's origin `floor(ceil(margin) / scale)` sheet
/// points from the paper's edge: its PDFs (left and top margins 0.4 in =
/// 28.8 pt) put it 39, 33, 35, 34, 30 and 29 sheet points in at 73, 87, 81,
/// 83, 96 and 100 % (xlsx-XBRD, XALN, XFNT), and the dashes of `dashed`
/// (4 pt) and `mediumDashed` (12 pt) on those borders run on from there.
/// On screen (not printed) the same as printed at 100 %.
fn dash_anchors(sheet: &Sheet, scale: f64) -> (f32, f32) {
    let [left, _, top, _] = sheet.print.margins;
    let inset = |margin: i64| -((margin as f64 / 12_700.0).ceil() / scale.max(0.1)).floor() as f32;
    (inset(left), inset(top))
}

/// On and off lengths, in points, of a dashed border as Excel prints it
/// (measured from its PDF); `None` for a solid one. SlantDashDot prints solid.
fn dash_pattern(line: BorderLine) -> Option<&'static [f32]> {
    Some(match line {
        BorderLine::Dashed => &[3.0, 1.0],
        BorderLine::Dotted => &[1.0, 1.0],
        BorderLine::Hair => &[0.75, 0.75],
        BorderLine::MediumDashed => &[9.0, 3.0],
        BorderLine::DashDot | BorderLine::MediumDashDot => &[9.0, 3.0, 3.0, 3.0],
        BorderLine::DashDotDot | BorderLine::MediumDashDotDot => &[9.0, 3.0, 3.0, 3.0, 3.0, 3.0],
        _ => return None,
    })
}

/// Which of two lines on a shared edge Excel shows: the higher rank (as its
/// PDF prints every pair of styles).
fn border_rank(line: BorderLine) -> u8 {
    match line {
        BorderLine::Hair => 0,
        BorderLine::DashDotDot => 1,
        BorderLine::DashDot => 2,
        BorderLine::Dotted => 3,
        BorderLine::Dashed => 4,
        BorderLine::Thin => 5,
        BorderLine::MediumDashDotDot => 6,
        BorderLine::SlantDashDot => 7,
        BorderLine::MediumDashDot => 8,
        BorderLine::MediumDashed => 9,
        BorderLine::Medium => 10,
        BorderLine::Thick => 11,
        BorderLine::Double => 12,
    }
}

/// How many of the screen's (logical) pixels thick Excel draws a border of
/// each style, at every zoom (#2, the user's decision 2026-10-07): thin,
/// hair, dotted and dashed ones 1, medium ones 2, thick 3, double two lines
/// of 1 with 1 between.
fn screen_border_pixels(line: BorderLine) -> f32 {
    match line {
        BorderLine::Medium | BorderLine::MediumDashed | BorderLine::MediumDashDot | BorderLine::MediumDashDotDot => 2.0,
        BorderLine::Thick | BorderLine::Double => 3.0,
        _ => 1.0,
    }
}

fn border_quads(b: &ibis_doc_model::Border, horizontal: bool, at_page_start: bool, at: f32, from: f32, to: f32, px_per_point: f32, out: &mut Vec<Quad>) {
    // On the screen (not printed) a fixed number of pixels, whatever the zoom.
    let screen = PRINT_GRID.with(|g| g.get()).is_none() && !crate::render::chart_view::VECTOR_EXPORT.with(|v| v.get());
    let win_printed_hair = !screen && b.line == BorderLine::Hair && ibis_pptx::EXCEL_METRICS == ibis_pptx::ExcelMetrics::Windows;
    let t = if screen {
        screen_border_pixels(b.line)
    } else if win_printed_hair {
        (DOT_STEP / 4.0 * px_per_point).max(DOT_STEP / 4.0)
    } else {
        (b.line.width() * px_per_point).max(thinnest_line())
    };
    let border_unit = if screen { 1.0 } else { px_per_point };
    let color = hsla(opaque(b.color));
    // Across the line, Excel's printed borders lie on the cell grid by their
    // width w (Excel for Mac's PDFs, xlsx-XBRD): w = 1 pt and 2 pt end on the
    // grid line ([g - w, g]), 3 pt (thick) is [g - 2, g + 1], and a double
    // line is two 1 pt lines, [g - 2, g - 1] and [g, g + 1]; the same on
    // every side of a cell. `rect` takes the near edge of the line.
    // On a page's first row or column after a break (`at_page_start`), shift
    // so the line starts at `at` on its own page instead of reaching into the previous page.
    let grid = at;
    let near = |t: f32| if at_page_start { grid } else { grid - t.min(2.0 * border_unit) };
    let rect = |lo: f32, t: f32, a: f32, z: f32| {
        if horizontal {
            (a, lo, z - a, t)
        } else {
            (lo.max(0.0), a, t, z - a)
        }
    };
    if (b.line == BorderLine::Dotted || (b.line == BorderLine::Hair && ibis_pptx::EXCEL_METRICS == ibis_pptx::ExcelMetrics::Mac))
        && let Some(grid) = PRINT_GRID.with(|g| g.get())
    {
        // Printed: a 1 pt line, on the paper's points.
        let t = px_per_point;
        let (from, to) = if horizontal { (from, to) } else { (from - px_per_point, to) };
        let (x, y, w, h) = rect(near(t), t, from, to);
        let p = px_per_point;
        if b.line == BorderLine::Dotted {
            // The dots as the dashes of one line (a drawing of each is thousands of objects in a page of notes).
            let (scale, kx, ky) = grid;
            let (origin, start) = if horizontal { (kx, x / p) } else { (ky, y / p) };
            let (pattern, phase) = dotted_pattern(scale, origin, start, p);
            let (points, width) = if horizontal { (vec![(x, y + h / 2.0), (x + w, y + h / 2.0)], h) } else { (vec![(x + w / 2.0, y), (x + w / 2.0, y + h)], w) };
            BORDER_DASHES.with(|d| d.borrow_mut().push(DashedLine { points, width, color, pattern, phase }));
            return;
        }
        for (a, b2, c, d) in hair_cells((x / p, y / p, (x + w) / p, (y + h) / p), grid) {
            out.push(Quad { rect: (a * p, b2 * p, (c - a) * p, (d - b2) * p), color });
        }
        return;
    }
    if b.line == BorderLine::SlantDashDot && !screen {
        // The dash-dot pattern of `mediumDashDot` (from the paper's corner) in a
        // 2 pt band [g − 2, g], each dash a parallelogram slanted 2 pt across the band,
        // clipped to the line's ends.
        let p = px_per_point;
        let (anchor_x, anchor_y) = DASH_ANCHOR.with(|a| a.get());
        let anchor = if horizontal { anchor_x } else { anchor_y };
        let (from, to) = if horizontal { (from, to) } else { (from - p, to) };
        let (start, end) = (from / p, to / p);
        let far = if at_page_start { grid / p + 2.0 } else { grid / p };
        let nearer = if at_page_start { grid / p } else { grid / p - 2.0 };
        let (y_lo, y_hi) = (nearer.min(far), nearer.max(far));
        let pattern = [9.0f32, 3.0, 3.0, 3.0];
        let period: f32 = pattern.iter().sum();
        let mut q0 = start - 3.0 - (start - 3.0 - anchor).rem_euclid(period);
        let mut shapes = Vec::new();
        while q0 < end {
            let mut q = q0;
            for (i, &len) in pattern.iter().enumerate() {
                if i % 2 == 0 {
                    let pts = [(q, far), (q + len, far), (q + len + 2.0, nearer), (q + 2.0, nearer)];
                    let clipped = clip_to_box(&pts, (start, y_lo, end, y_hi));
                    if clipped.len() >= 3 {
                        let outline: Vec<(f32, f32)> = clipped
                            .into_iter()
                            .map(|(along, across)| if horizontal { (along * p, across * p) } else { (across * p, along * p) })
                            .collect();
                        shapes.push((outline, 0.0, color));
                    }
                }
                q += len;
            }
            q0 += period;
        }
        BORDER_SHAPES.with(|s| s.borrow_mut().extend(shapes));
        return;
    }
    match b.line {
        BorderLine::Double => {
            let thin = t / 3.0;
            let (from, to) = if horizontal { (from, to) } else { (from - border_unit, to) };
            let g0 = if at_page_start { grid } else { grid - 2.0 * thin };
            out.push(Quad { rect: rect(g0, thin, from, to), color });
            out.push(Quad { rect: rect(g0 + 2.0 * thin, thin, from, to), color });
        }
        line if (!win_printed_hair && dash_pattern(line).is_some()) || (screen && line == BorderLine::SlantDashDot) => {
            // The pattern runs on from the paper's corner (Excel prints a
            // dashed edge continuous across cells), in points. On the
            // screen a slanted dash-dot is drawn as a dash-dot, 1 pixel.
            let pattern = dash_pattern(line).unwrap_or(&[9.0, 3.0, 3.0, 3.0]);
            let period: f32 = pattern.iter().sum();
            let (anchor_x, anchor_y) = DASH_ANCHOR.with(|a| a.get());
            let anchor = if horizontal { anchor_x } else { anchor_y };
            // A column's dashes start, as its solid line does, a unit
            // above (Excel's PDF of xlsx-XBRD).
            let from = if horizontal { from } else { from - border_unit };
            let (start, end) = (from / px_per_point, to / px_per_point);
            if !screen {
                // Printed, the dashes are one line with its pattern, from where the pattern's period starts.
                let across = if horizontal { near(t) } else { near(t).max(0.0) } + t / 2.0;
                let points = if horizontal { vec![(start * px_per_point, across), (end * px_per_point, across)] } else { vec![(across, start * px_per_point), (across, end * px_per_point)] };
                let phase = (start - anchor).rem_euclid(period) * px_per_point;
                if end > start {
                    BORDER_DASHES.with(|d| d.borrow_mut().push(DashedLine { points, width: t, color, pattern: pattern.iter().map(|l| l * px_per_point).collect(), phase }));
                }
                return;
            }
            // On the screen each dash is a rectangle on whole pixels.
            let mut p = start - (start - anchor).rem_euclid(period);
            while p < end {
                let mut q = p;
                for (i, &len) in pattern.iter().enumerate() {
                    let (a, z) = (q.max(start), (q + len).min(end));
                    if i % 2 == 0 && z > a {
                        out.push(Quad { rect: rect(near(t), t, a * px_per_point, z * px_per_point), color });
                    }
                    q += len;
                }
                p += period;
            }
        }
        _ => {
            // Along a column a line of any width runs from the top grid
            // line's 1 unit to the bottom one: [g - 1, g'] (Excel's PDF of
            // xlsx-XBRD).
            let (from, to) = if horizontal {
                (from, to)
            } else {
                (from - border_unit, to)
            };
            out.push(Quad { rect: rect(near(t), t, from, to), color });
        }
    }
}

/// Groups contiguous spans having identical borders along an edge and draws each
/// as a single quad, avoiding pattern resets and seams between adjacent cells.
fn draw_contiguous_borders<F, G>(
    start: u32,
    end: u32,
    get_coord: F,
    get_border: G,
    horizontal: bool,
    is_left: bool,
    at: f32,
    px_per_point: f32,
    out: &mut Vec<Quad>,
) where
    F: Fn(u32) -> f32,
    G: Fn(u32) -> Option<ibis_doc_model::Border>,
{
    let mut current_border: Option<ibis_doc_model::Border> = None;
    let mut start_idx = start;

    for idx in start..=end {
        let border = get_border(idx);
        match current_border {
            Some(cur) => {
                if Some(cur) != border {
                    border_quads(&cur, horizontal, is_left, at, get_coord(start_idx), get_coord(idx), px_per_point, out);
                    current_border = border;
                    start_idx = idx;
                }
            }
            None => {
                if border.is_some() {
                    current_border = border;
                    start_idx = idx;
                }
            }
        }
    }
    if let Some(cur) = current_border {
        border_quads(&cur, horizontal, is_left, at, get_coord(start_idx), get_coord(end + 1), px_per_point, out);
    }
}

/// Push a line segmented across cutout intervals (e.g. skipping merged cells).
fn push_segmented_line(
    horizontal: bool,
    line_coord: f32,
    thickness: f32,
    total_len: f32,
    mut cutouts: Vec<(f32, f32)>,
    color: gpui_kit::Hsla,
    out: &mut Vec<Quad>,
) {
    if cutouts.is_empty() {
        let rect = if horizontal {
            (0.0, line_coord, total_len, thickness)
        } else {
            (line_coord, 0.0, thickness, total_len)
        };
        out.push(Quad { rect, color });
        return;
    }

    cutouts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut cursor = 0.0;
    for (start, end) in cutouts {
        let s = start.max(0.0).min(total_len);
        let e = end.max(0.0).min(total_len);
        if s > cursor {
            let rect = if horizontal {
                (cursor, line_coord, s - cursor, thickness)
            } else {
                (line_coord, cursor, thickness, s - cursor)
            };
            out.push(Quad { rect, color });
        }
        cursor = cursor.max(e);
    }
    if cursor < total_len {
        let rect = if horizontal {
            (cursor, line_coord, total_len - cursor, thickness)
        } else {
            (line_coord, cursor, thickness, total_len - cursor)
        };
        out.push(Quad { rect, color });
    }
}


fn mix_white(c: ibis_doc_model::Rgb, t: f32) -> ibis_doc_model::Rgb {
    let ch = |shift: u32| {
        let v = ((c.0 >> shift) & 0xff) as f32;
        ((v + (255.0 - v) * t).round() as u32) << shift
    };
    ibis_doc_model::Rgb(ch(16) | ch(8) | ch(0))
}

fn opaque(rgb: ibis_doc_model::Rgb) -> ibis_doc_model::Color {
    ibis_doc_model::Color::opaque(rgb)
}

/// A cell's text and what laying it out needs.
struct CellText {
    text: String,
    kind: CellKind,
    /// A number shown in General, which shows fewer digits in a narrow cell.
    general: Option<f64>,
    style: CellStyle,
    font: CellFont,
    /// Left, top, right, bottom.
    rect: (f32, f32, f32, f32),
    /// The horizontal span the text may cover when wider than the cell,
    /// running on to the left, both ways and to the right.
    spans: [(f32, f32); 3],
    /// Bottom-aligned single lines: the points from the row's bottom to the
    /// baseline the row's such lines share (`row_gaps`).
    row_gap: Option<f32>,
    /// The text's parts in their own fonts (rich text).
    runs: Option<Arc<Vec<ibis_doc_model::CellRun>>>,
    /// The phonetic reading of the cell (furigana/ruby), shown when sheet's show_phonetic is true.
    phonetic: Option<Arc<str>>,
}

/// The font a cell's text is drawn in, with its measures.
#[derive(Clone)]
struct CellFont {
    font: Font,
    /// The same without proportional widths, for the characters `palt`
    /// narrows more than the typeface does (the middle dot).
    plain: Font,
    /// How much smaller its Latin letters and digits are drawn (see
    /// `Resources::substitute_scale`).
    latin_scale: f32,
    ascent: f32,
    descent: f32,
}

/// How wide a cell's text is drawn, in points (for fitting a column to its
/// content, as Excel's double-click on a column's line does): the glyphs'
/// advances, Latin letters and digits smaller in a substitute as
/// `text_items` draws them.
pub(crate) fn text_width_pt(text: &str, style: &CellStyle, res: &Resources, ts: &TextSystem) -> f32 {
    GlyphWidths::new(style, res, ts).width(text)
}

fn printed_shaped_width(ts: &WindowTextSystem, text: &str, font: &Font, size: f32, px_per_point: f32) -> f32 {
    let run = TextRun { len: text.len(), font: font.clone(), color: black(), background_color: None, underline: None, strikethrough: None };
    let mut line = ts.shape_line(SharedString::from(text.to_owned()), px(size * px_per_point), &[run], None);
    whole_point_advances(&mut line, px_per_point);
    f32::from(line.width) / px_per_point
}

#[cfg(all(test, target_os = "windows", feature = "export"))]
#[path = "../tests/xlsx/print_bounds.rs"]
mod print_bounds_tests;

/// Printed text's horizontal coverage, using the same line layout, font shaping,
/// advance rounding and alignment as painting. Coordinates are sheet-local EMU.
/// Empty values and explicit formatting are accounted for separately by export.
pub(crate) fn printed_text_regions(sheet: &Arc<Sheet>, res: &Resources) -> Vec<EmuRect> {
    let key = layout_key(sheet, 1.0 / EMU_PER_POINT as f32, res);
    PRINT_REGIONS.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.retain(|(_, sheet, _)| sheet.strong_count() > 0);
        if let Some(at) = cache.iter().position(|(cached, _, _)| *cached == key) {
            let entry = cache.remove(at);
            let regions = entry.2.clone();
            cache.push(entry);
            return regions;
        }
        let regions = measure_printed_text_regions(sheet, res);
        if cache.len() >= CACHED_LAYOUTS { cache.remove(0); }
        // Weak keeps the allocation identity reserved without forcing a full
        // Sheet clone on edit. Arc::make_mut dissociates it on mutation.
        cache.push((key, Arc::downgrade(sheet), regions.clone()));
        regions
    })
}

fn measure_printed_text_regions(sheet: &Sheet, res: &Resources) -> Vec<EmuRect> {
    let scale = 1.0 / EMU_PER_POINT as f32;
    let mut fonts: Vec<CellFont> = sheet.styles.iter().map(|s| cell_font(s, res)).collect();
    fonts.push(cell_font(&CellStyle::default(), res));
    let context = LayoutContext { print: true, print_scale: 1.0, breaks: Default::default(), fonts, sheets: res.sheets.clone(), images: Default::default() };
    let mut regions = Vec::new();
    // Only one band is laid out at a time: print extent discovery must not
    // retain another full-sheet set of fills, borders and text alongside paint.
    let mut first = 0;
    while first < sheet.cells.len() {
        let from = sheet.cells[first].row;
        let to = from.saturating_add(PART_ROWS);
        let next = sheet.cells.partition_point(|cell| cell.row < to);
        let laid = layout(sheet, scale, &context, None, Part::Rows(from, to));
        let text_system = WindowTextSystem::new(Arc::clone(res.text_system()));
        let measure = |text: &str, font: &Font, size: f32| {
            printed_shaped_width(&text_system, text, font, size, 1.0)
        };
        for cell in &laid.cells {
            let mut items = Vec::new();
            text_items(cell, 1.0, &measure, &mut items);
            for item in items {
                // Horizontal run-on is enabled only at rotation zero in text_items;
                // the cell/merge establishes the extent of other text modes.
                if item.rotation != 0 { continue }
                let width: f32 = item.pieces.iter().map(|piece| {
                    piece.advance.unwrap_or_else(|| {
                        let font = piece.run.as_ref().map(|r| &r.font).unwrap_or(if piece.plain { &item.plain } else { &item.font });
                        measure(&piece.text, font, piece.size)
                    })
                }).sum();
                let count = item.pieces.len();
                let trailing = item.pieces.last().is_some_and(|p| p.advance.is_none() && p.text.ends_with(' '));
                let (x, gap) = compute_text_x(item.align, item.left, item.right, width, count, trailing, item.rotation, 1.0);
                let left = x.max(item.clip.0);
                let right = (x + width + gap * count.saturating_sub(1) as f32).min(item.clip.0 + item.clip.2);
                if right > left && item.clip.3 > 0.0 && (left < item.cell.0 || right > item.cell.1) {
                    let emu = |v: f32| (v as f64 * EMU_PER_POINT as f64).round() as i64;
                    regions.push(EmuRect::new(emu(left), emu(item.clip.1), emu(right) - emu(left), emu(item.clip.3)));
                }
            }
        }
        first = next;
    }
    regions
}

/// The widths of the glyphs of a format's font, found once each: the width of many texts in one format (a column of a million cells) without resolving the font and asking for each glyph again every time.
pub(crate) struct GlyphWidths<'a> {
    ts: &'a TextSystem,
    id: FontId,
    size: f32,
    latin_scale: f32,
    advances: HashMap<char, f32>,
}

impl<'a> GlyphWidths<'a> {
    pub(crate) fn new(style: &CellStyle, res: &Resources, ts: &'a TextSystem) -> Self {
        let cell = cell_font(style, res);
        Self { ts, id: ts.resolve_font(&cell.font), size: style.font_size as f32 / 100.0, latin_scale: cell.latin_scale, advances: HashMap::new() }
    }

    /// `text_width_pt`: the glyphs' advances added up.
    pub(crate) fn width(&mut self, text: &str) -> f32 {
        let Self { ts, id, size, latin_scale, advances } = self;
        text.chars()
            .map(|c| {
                *advances.entry(c).or_insert_with(|| {
                    let k = if c.is_ascii() { *latin_scale } else { 1.0 };
                    ts.advance(*id, px(*size * k), c).map(|a| f32::from(a.width)).unwrap_or(if c.is_ascii() { *size * 0.5 } else { *size })
                })
            })
            .sum()
    }
}

/// The text each cell shows, laid out as `text_items` lays it out (lines
/// joined by `\n`, `#`s and all), as (row, column, text): for the scenarios.
#[cfg(all(target_os = "macos", feature = "scenarios"))]
pub(crate) fn shown_texts(sheet: &Sheet, res: &Resources, window: &Window) -> Vec<(u32, u32, String)> {
    let scale = 1.0 / EMU_PER_POINT as f32;
    let mut fonts: Vec<CellFont> = sheet.styles.iter().map(|s| cell_font(s, res)).collect();
    fonts.push(cell_font(&CellStyle::default(), res));
    let images = sheet.web_images.iter().filter(|((r, c), _)| sheet.cell(*r, *c).is_some_and(|x| x.formula.is_some())).map(|(k, v)| (*k, v.source.clone())).collect();
    let context = LayoutContext { print: res.print, print_scale: res.print_scale.get(), breaks: Default::default(), fonts, sheets: res.sheets.clone(), images };
    let laid = layout(sheet, scale, &context, None, Part::All);
    let measure = |text: &str, font: &Font, size: f32| -> f32 {
        let run = TextRun { len: text.len(), font: font.clone(), color: black(), background_color: None, underline: None, strikethrough: None };
        let mut line = window.text_system().shape_line(SharedString::from(text.to_string()), px(size), &[run], None);
        whole_point_advances(&mut line, 1.0);
        f32::from(line.width)
    };
    let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
    let index = |edges: &[i64], at: f32| {
        edges
            .partition_point(|&e| {
                let v = e as f32 * scale;
                (if res.print { v } else { v.round() }) <= at + 0.01
            })
            .saturating_sub(1) as u32
    };
    laid.cells
        .iter()
        .map(|cell| {
            let mut items = Vec::new();
            text_items(cell, 1.0, &measure, &mut items);
            let text = items.iter().map(|t| t.pieces.iter().map(|p| p.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\n");
            (index(&ys, cell.rect.1), index(&xs, cell.rect.0), text)
        })
        .collect()
}

fn cell_font(style: &CellStyle, res: &Resources) -> CellFont {
    let mut font = res.font(&TextStyle {
        font_size: style.font_size,
        bold: style.bold,
        italic: style.italic,
        font: style.font.clone(),
        ..TextStyle::default()
    });
    let plain = font.clone();
    let latin_scale = res.substitute_scale(style.font.as_deref(), &font);
    // A proportional Japanese typeface (MS P Gothic, Meiryo UI) narrows its
    // kana and punctuation; its substitute does so with proportional widths
    // (`palt`).
    if style.rotation != 255 && (proportional_japanese(style.font.as_deref()) && latin_scale < 1.0 || style.font.as_deref().is_some_and(|n| n.starts_with("Meiryo UI"))) {
        let mut features: Vec<(String, u32)> = font.features.0.iter().cloned().collect();
        features.push(("palt".into(), 1));
        font.features = FontFeatures(std::sync::Arc::new(features));
    }
    let (ascent, descent) = style
        .font
        .as_deref()
        .filter(|name| !res.has_typeface(name) && name.trim().starts_with("BIZ UD"))
        .map(|_| (0.8799, 0.1201))
        .unwrap_or_else(|| res.metrics(&font));
    CellFont { font, plain, latin_scale, ascent, descent }
}

/// Lay out a cell's text into lines, measuring with `measure` (text, font,
/// size in pixels → width).
/// The marks of a group's sparklines on this sheet (data on another sheet
/// is not drawn yet): lines, columns or win/loss bars inside the cell,
/// with the markers and the axis the group asks for (MS-XLSX 2.6.89). Excel
/// for Mac prints each sparkline as a bitmap of the cell; the insets and
/// the marker size are read from those, the column and win/loss bars'
/// widths are not measured yet.
fn sparkline_marks(
    sheet: &Sheet,
    group: &ibis_doc_model::SparklineGroup,
    x: &dyn Fn(u32) -> f32,
    y: &dyn Fn(u32) -> f32,
    drawn: &dyn Fn(u32) -> bool,
    p: f32,
    sheets: &HashMap<String, Arc<Sheet>>,
    out: &mut Layout,
) {
    use ibis_doc_model::{CellValue, Rgb, SparklineAxis, SparklineEmpty, SparklineKind};
    let target_sheet = |s: Option<&str>| -> &Sheet {
        match s {
            None => sheet,
            Some(name) => {
                sheets
                    .get(name)
                    .or_else(|| sheets.get(&name.to_lowercase()))
                    .map(|arc| &**arc)
                    .unwrap_or(sheet)
            }
        }
    };
    let values = |sp: &ibis_doc_model::Sparkline| -> Vec<Option<f64>> {
        let src_sheet = target_sheet(sp.sheet.as_deref());
        let d = &sp.data;
        let cells: Vec<(u32, u32)> = if d.first_row == d.last_row {
            (d.first_column..=d.last_column).map(|c| (d.first_row, c)).collect()
        } else {
            (d.first_row..=d.last_row).flat_map(|r| (d.first_column..=d.last_column).map(move |c| (r, c))).collect()
        };
        let cells: Vec<(u32, u32)> = if group.display_hidden {
            cells
        } else {
            cells.into_iter().filter(|&(r, c)| !src_sheet.is_row_hidden(r) && !src_sheet.is_col_hidden(c)).collect()
        };
        let mut v: Vec<Option<f64>> = cells
            .into_iter()
            .map(|(r, c)| match src_sheet.cell(r, c).map(|c| &c.value) {
                Some(CellValue::Number(n)) => Some(*n),
                _ => None,
            })
            .map(|v| if v.is_none() && group.empty == SparklineEmpty::Zero { Some(0.0) } else { v })
            .collect();
        if group.right_to_left {
            v.reverse();
        }
        v
    };
    let ours: Vec<&ibis_doc_model::Sparkline> = group.sparklines.iter().collect();
    let all: Vec<Vec<Option<f64>>> = ours.iter().map(|s| values(s)).collect();
    let group_range = all.iter().flatten().flatten().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    let custom = |a: &SparklineAxis| match a {
        SparklineAxis::Custom(v) => v.trim().parse::<f64>().ok(),
        _ => None,
    };
    let date_values: Option<Vec<f64>> = if group.date_axis {
        group.date_range.as_ref().and_then(|spec| {
            let (sh_name, area) = match spec.rsplit_once('!') {
                Some((sh, a)) => (Some(sh.trim_matches('\'').replace("''", "'")), a),
                None => (None, spec.as_str()),
            };
            let date_sheet = target_sheet(sh_name.as_deref());
            let range = super::dedupe_edit::parse_range(&area.replace('$', ""))?;
            let cells: Vec<(u32, u32)> = if range.first_row == range.last_row {
                (range.first_column..=range.last_column).map(|c| (range.first_row, c)).collect()
            } else {
                (range.first_row..=range.last_row).flat_map(|r| (range.first_column..=range.last_column).map(move |c| (r, c))).collect()
            };
            let cells: Vec<(u32, u32)> = if group.display_hidden {
                cells
            } else {
                cells.into_iter().filter(|&(r, c)| !date_sheet.is_row_hidden(r) && !date_sheet.is_col_hidden(c)).collect()
            };
            let mut dates: Vec<f64> = cells
                .into_iter()
                .enumerate()
                .map(|(idx, (r, c))| match date_sheet.cell(r, c).map(|cell| &cell.value) {
                    Some(CellValue::Number(n)) => *n,
                    _ => idx as f64,
                })
                .collect();
            if group.right_to_left {
                dates.reverse();
            }
            Some(dates)
        })
    } else {
        None
    };

    let date_t = |i: usize, n: usize| -> f32 {
        if let Some(ref dates) = date_values {
            if let Some(&d) = dates.get(i) {
                let min_d = dates.iter().copied().fold(f64::MAX, f64::min);
                let max_d = dates.iter().copied().fold(f64::MIN, f64::max);
                if max_d > min_d {
                    return ((d - min_d) / (max_d - min_d)) as f32;
                } else {
                    return 0.5;
                }
            }
        }
        if n <= 1 { 0.5 } else { i as f32 / (n - 1) as f32 }
    };

    for (sp, vals) in ours.iter().zip(&all) {
        if !drawn(sp.row) {
            continue;
        }
        let numbers: Vec<f64> = vals.iter().flatten().copied().collect();
        if numbers.is_empty() {
            continue;
        }
        let own = numbers.iter().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        let mut lo = custom(&group.min_axis).unwrap_or(if group.min_axis == SparklineAxis::Group { group_range.0 } else { own.0 });
        let mut hi = custom(&group.max_axis).unwrap_or(if group.max_axis == SparklineAxis::Group { group_range.1 } else { own.1 });
        if group.kind == SparklineKind::Column {
            // Columns grow from zero when the data allow it.
            lo = lo.min(0.0);
            hi = hi.max(0.0);
        }
        if hi <= lo {
            let mid = lo;
            lo = mid - 1.0;
            hi = mid + 1.0;
        }
        // The insets read from Excel's 53 x 31 bitmaps of a 34.45 x 20.15 pt
        // cell: the first point 4.2 pt from the left, the last 4.9 pt from the
        // right, the highest and lowest about 2.5 pt from the edges.
        let (left, right) = (x(sp.column), x(sp.column + 1));
        let (x0, x1) = (left + 4.2 * p, right - 4.9 * p);
        let (y0, y1) = (y(sp.row) + 2.5 * p, y(sp.row + 1) - 2.5 * p);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        // Without a series colour Excel draws a line black and no columns
        // or win/loss bars.
        let series = match (group.series, group.kind) {
            (Some(c), _) => c,
            (None, SparklineKind::Line) => Rgb(0),
            (None, _) => continue,
        };
        let n = vals.len().max(1);
        let to_y = |v: f64| y1 - ((v - lo) / (hi - lo)) as f32 * (y1 - y0);
        let (hi_v, lo_v) = (own.1, own.0);
        let first = vals.iter().position(|v| v.is_some());
        let last = vals.iter().rposition(|v| v.is_some());
        let special = |i: usize, v: f64| -> Option<Rgb> {
            // A point that is several of these takes the first colour.
            [
                (group.high && v == hi_v, group.high_color),
                (group.low && v == lo_v, group.low_color),
                (group.first && Some(i) == first, group.first_color),
                (group.last && Some(i) == last, group.last_color),
                (group.negative_points && v < 0.0, group.negative),
            ]
            .into_iter()
            .find_map(|(on, c)| if on { c } else { None })
        };
        let quad = |out: &mut Layout, l: f32, t: f32, r: f32, b: f32, c: Rgb| {
            if r > l && b > t {
                out.fills.push(Quad { rect: (l, t, r - l, b - t), color: hsla(opaque(c)) });
            }
        };
        match group.kind {
            SparklineKind::Line => {
                let at = |i: usize| x0 + (x1 - x0) * date_t(i, n);
                let width = (group.line_weight as f32 / 100.0 * p).max(thinnest_line());
                let color = hsla(opaque(series));
                let mut prev: Option<(f32, f32)> = None;
                for (i, v) in vals.iter().enumerate() {
                    match v {
                        Some(v) => {
                            let pt = (at(i), to_y(*v));
                            if let Some(q) = prev {
                                out.diagonals.push((q, pt, width, color));
                            }
                            prev = Some(pt);
                        }
                        None if group.empty == SparklineEmpty::Span => {}
                        None => prev = None,
                    }
                }
                // Markers: squares of 2 px in Excel's bitmaps (1.3 pt).
                let m = 1.3 * p / 2.0;
                for (i, v) in vals.iter().enumerate() {
                    let Some(v) = v else { continue };
                    let c = special(i, *v).or(if group.markers { group.markers_color } else { None });
                    if let Some(c) = c {
                        let (cx, cy) = (at(i), to_y(*v));
                        quad(out, cx - m, cy - m, cx + m, cy + m, c);
                    }
                }
            }
            SparklineKind::Column | SparklineKind::WinLoss => {
                let slot = (x1 - x0) / n as f32;
                let bar = slot * 0.75;
                let column = group.kind == SparklineKind::Column;
                let base = if column { to_y(0.0_f64.clamp(lo, hi)) } else { (y0 + y1) / 2.0 };
                let span = |v: f64| -> (f32, f32) {
                    if column {
                        let t = to_y(v);
                        (t.min(base), t.max(base))
                    } else if v > 0.0 {
                        (y0, base)
                    } else if v < 0.0 {
                        (base, y1)
                    } else {
                        (base, base)
                    }
                };
                for (i, v) in vals.iter().enumerate() {
                    let Some(v) = v else { continue };
                    let l = if date_values.is_some() {
                        x0 + (x1 - x0) * date_t(i, n) - bar / 2.0
                    } else {
                        x0 + slot * i as f32 + (slot - bar) / 2.0
                    };
                    let (t, b) = span(*v);
                    let c = special(i, *v).unwrap_or(series);
                    quad(out, l, t, l + bar, b.max(t + p * 0.5), c);
                }
                if let Some(axis) = group.axis.filter(|_| group.display_x_axis && base > y0 && base < y1) {
                    out.diagonals.push(((left + 1.3 * p, base), (right - 1.3 * p, base), thinnest_line().max(0.75 * p), hsla(opaque(axis))));
                }
            }
        }
        // The axis at zero, across the cell but 2 px (1.3 pt) either end.
        if let Some(axis) = group.axis.filter(|_| group.kind == SparklineKind::Line && group.display_x_axis && lo < 0.0 && hi > 0.0) {
            let z = to_y(0.0);
            out.diagonals.push(((left + 1.3 * p, z), (right - 1.3 * p, z), thinnest_line().max(0.75 * p), hsla(opaque(axis))));
        }
    }
}

/// Whether a typeface is one of the MS fonts (MS Gothic, MS P Mincho, …),
/// which Excel sets on other line measures (its PDF of xlsx-XBASE).
fn ms_font(name: Option<&str>) -> bool {
    name.is_some_and(|n| n.starts_with("ＭＳ") || n.starts_with("MS ")) // i18n-ok（フォント名の判定（表示名ではなく、フォント名の接頭辞））
}

/// The points from a cell's bottom to the baseline of a bottom-aligned
/// single line, as Excel prints it. Fitted to its PDFs; the cause is not
/// confirmed:
/// - A cell whose alignment names `vertical="bottom"` (xlsx-XBASE, XSUP,
///   XMIX): `round(0.4707 · size)` (5 pt at 11 pt, 7 at 14, 9 at 20, 13 at
///   28), a subscript `ceil(0.57 · size)`, 3 more for an accounting
///   underline; in an MS font 5 pt, a subscript `max(5, round(0.33 ·
///   size))`, 1 more for an accounting underline.
/// - A cell left at the default (xlsx-XFNT): 5 pt at every size, 4 more for
///   an accounting underline (1 in an MS font), 1 more for a single or
///   double underline.
/// `None` for text that does not take part (not bottom-aligned, or
/// wrapped, turned or stacked).
fn bottom_gap(cell: &CellText) -> Option<f32> {
    let style = &cell.style;
    if style.vertical != CellVerticalAlign::Bottom || style.wrap || style.rotation != 0 || style.shrink || cell.text.contains('\n') {
        return None;
    }
    if matches!(style.align, CellAlign::Justify | CellAlign::Distributed | CellAlign::Fill) {
        return None;
    }
    let ms = ms_font(style.font.as_deref());
    let size = style.font_size as f32 / 100.0;
    let accounting = matches!(style.underline, CellUnderline::SingleAccounting | CellUnderline::DoubleAccounting);
    let sub = style.vert_align == VerticalPosition::Subscript;
    let gap = if style.vertical_set {
        let base = match (ms, sub) {
            (false, false) => (0.4707 * size + 0.5).floor(),
            (false, true) => (0.57 * size - 1e-3).ceil(),
            (true, false) => 5.0,
            (true, true) => (0.33 * size + 0.5).floor().max(5.0),
        };
        base + if accounting { if ms { 1.0 } else { 3.0 } } else { 0.0 }
    } else {
        5.0 + if accounting {
            if ms { 1.0 } else { 4.0 }
        } else if !ms && matches!(style.underline, CellUnderline::Single | CellUnderline::Double) {
            1.0
        } else {
            0.0
        }
    };
    Some(gap)
}

/// Bottom-aligned single lines of a row share one baseline, the highest
/// any of them asks for: an 11 pt line beside a 28 pt one rises to its
/// baseline (Excel's PDF of xlsx-XBASE).
fn row_gaps(cells: &mut [CellText]) {
    let mut most: HashMap<i64, f32> = HashMap::new();
    let key = |c: &CellText| (c.rect.3 * 64.0).round() as i64;
    for c in cells.iter() {
        if let Some(g) = bottom_gap(c) {
            let e = most.entry(key(c)).or_insert(g);
            *e = e.max(g);
        }
    }
    for c in cells.iter_mut() {
        if bottom_gap(c).is_some() {
            c.row_gap = most.get(&key(c)).copied();
        }
    }
}

fn text_items(cell: &CellText, px_per_point: f32, measure: &dyn Fn(&str, &Font, f32) -> f32, out: &mut Vec<TextItem>) {
    let (text, kind, style) = (cell.text.as_str(), cell.kind, &cell.style);
    let (left, top, right, bottom) = cell.rect;
    let font = cell.font.font.clone();
    let base = style.font_size as f32 / 100.0 * px_per_point;
    // A substitute for a typeface that is not installed is drawn smaller
    // where its Latin letters and digits are wider (MS P Gothic's digit is
    // 0.54 em as Excel prints it, Hiragino's 0.66); its Japanese is drawn
    // at the cell's size (both full-width).
    let latin_scale = cell.font.latin_scale;
    // Pieces: text, size, whether drawn without proportional widths, and a
    // fixed advance for a character drawn as room only (a proportional MS
    // font's ideographic space, narrower than its substitute's).
    let proportional = proportional_japanese(style.font.as_deref()) && latin_scale < 1.0;
    let segments = |text: &str, k: f32| -> Vec<Piece> {
        let mut out: Vec<Piece> = Vec::new();
        for c in text.chars() {
            if c == '\t' {
                continue;
            }
            let size = base * k * if c.is_ascii() { latin_scale } else { 1.0 };
            let advance = proportional.then(|| fixed_advance(c)).flatten().map(|em| em * base * k);
            if let Some(a) = advance {
                match out.last_mut() {
                    Some(p) if p.advance.is_some() => p.advance = Some(p.advance.unwrap_or(0.0) + a),
                    _ => out.push(Piece { text: String::new(), size, plain: false, advance: Some(a), run: None }),
                }
                continue;
            }
            let plain = style.bold && keeps_width(c);
            match out.last_mut() {
                Some(p) if p.advance.is_none() && p.size == size && p.plain == plain => p.text.push(c),
                _ => out.push(Piece { text: c.to_string(), size, plain, advance: None, run: None }),
            }
        }
        out
    };
    let plain_font = cell.font.plain.clone();
    let font_of = |plain: bool| if plain { &plain_font } else { &font };
    let width = |text: &str, k: f32| -> f32 { segments(text, k).iter().map(|p| p.advance.unwrap_or_else(|| measure(&p.text, font_of(p.plain), p.size))).sum() };
    let (ascent, descent) = (cell.font.ascent, cell.font.descent);
    let pad = CELL_PADDING_PT * px_per_point;
    // 9 pt a level: measured on Excel for Mac's PDF of xlsx-XALN, whose
    // workbook's standard font is Calibri 11 pt (shown in 游ゴシック), levels
    // 1, 2 and 5 all 9 pt apart. Whether Excel scales it with the standard
    // (Normal style) font is not checked: no reference with another one.
    let indent = style.indent as f32 * 9.0 * px_per_point;
    let align = match style.align {
        CellAlign::General => match kind {
            // A number in a text (@) cell shows as text, on the left, as Excel does.
            CellKind::Number if style.number_format == "@" => CellAlign::Left,
            CellKind::Number => CellAlign::Right,
            CellKind::Boolean | CellKind::Error => CellAlign::Center,
            // Text that starts right to left (Hebrew, Arabic) goes to the
            // right, whatever its readingOrder (Excel for Mac's PDF of
            // xlsx-XALN, readingOrder 0, 1 and 2 alike).
            _ if starts_right_to_left(text) => CellAlign::Right,
            _ => CellAlign::Left,
        },
        // Excel centres a number it is asked to distribute.
        CellAlign::Distributed if kind == CellKind::Number => CellAlign::Center,
        other => other,
    };
    // Repeated (fill) text sits at the bottom when asked to justify or
    // distribute vertically, as Excel prints it.
    let vertical = match style.vertical {
        CellVerticalAlign::Justify | CellVerticalAlign::Distributed if align == CellAlign::Fill => CellVerticalAlign::Bottom,
        other => other,
    };
    // Justified and distributed text wraps whether or not the cell says so,
    // across and down (Excel for Mac's PDF of xlsx-XALN, long text with
    // wrapText="0").
    // (A number or a logical value is not wrapped by the cell's wrap setting: it shows as `#` if it does not fit.)
    let wrap_text = align != CellAlign::Fill
        && ((style.wrap && !matches!(kind, CellKind::Number | CellKind::Boolean))
            || matches!(align, CellAlign::Justify | CellAlign::Distributed)
            || matches!(vertical, CellVerticalAlign::Justify | CellVerticalAlign::Distributed));
    let room = (right - left - 2.0 * ROOM_PADDING_PT * px_per_point - if align == CellAlign::Distributed { 2.0 * indent } else { indent }).max(0.0);

    let is_vertical = style.rotation == 255;
    // Each line, and whether it ends its paragraph (justified text is
    // spread on the other lines only).
    let (mut lines, ends): (Vec<String>, Vec<bool>) = if is_vertical {
        // (A line break in a stacked text is not a character to draw: a piece with a newline cannot be shaped.)
        text.chars().filter(|c| !matches!(c, '\n' | '\r')).map(|c| (c.to_string(), true)).unzip()
    } else if wrap_text {
        wrap(text, room, |t| width(t, 1.0)).into_iter().unzip()
    } else {
        text.lines().map(|l| (l.to_string(), true)).unzip()
    };
    if lines.is_empty() {
        return;
    }
    // Lines broken in the text show one under another, wrapped or not (issue #17); a turned text keeps its first line.
    if !wrap_text && !is_vertical && (1..=180).contains(&style.rotation) {
        lines.truncate(1);
    }
    let mut k = if matches!(style.vert_align, VerticalPosition::Superscript | VerticalPosition::Subscript) { 0.65 } else { 1.0 };
    let mut widths: Vec<f32> = lines.iter().map(|l| width(l, k)).collect();
    if style.shrink && !wrap_text && !is_vertical {
        let widest = widths.iter().cloned().fold(0.0, f32::max);
        if widest > room && widest > 0.0 {
            k = room / widest;
            widths.iter_mut().for_each(|w| *w *= k);
        }
    }
    // General shows fewer digits (rounded) in a narrow cell, as Excel does.
    if let Some(v) = cell.general
        && !wrap_text
        && widths[0] > room + 0.01
    {
        for w in (1..11).rev() {
            let t = ibis_calc::numfmt::general(v, w);
            let tw = width(&t, k);
            if tw <= room + 0.01 {
                lines[0] = t;
                widths[0] = tw;
                break;
            }
        }
    }
    // The fill of a number format (`_ * #,##0_ `) is the room left over, so it
    // never makes the number not fit: the long run of spaces the model holds
    // (counted by the column's characters, not by this font's) is cut down to
    // what the cell has room for.
    if kind == CellKind::Number && !wrap_text && widths[0] > room + 0.01 {
        let bytes = lines[0].as_bytes();
        let mut best: Option<(usize, usize)> = None;
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b' ' {
                let start = i;
                while i < bytes.len() && bytes[i] == b' ' {
                    i += 1;
                }
                if i - start >= 4 && best.is_none_or(|(a, b)| b - a < i - start) {
                    best = Some((start, i));
                }
            } else {
                i += 1;
            }
        }
        if let Some((start, end)) = best {
            let without = format!("{}{}", &lines[0][..start], &lines[0][end..]);
            let without_width = width(&without, k);
            if without_width <= room + 0.01 {
                let space = width(" ", k).max(0.1);
                let count = (((room - without_width) / space).floor().max(0.0) as usize).min(end - start);
                lines[0] = format!("{}{}{}", &lines[0][..start], " ".repeat(count), &lines[0][end..]);
                widths[0] = width(&lines[0], k);
            }
        }
    }
    // A number that does not fit shows as `#` across the cell, as does one
    // its format cannot show (the model's text is then `#`s).
    let unshowable = kind == CellKind::Number && !lines[0].is_empty() && lines[0].chars().all(|c| c == '#');
    if (kind == CellKind::Number && !wrap_text && (widths[0] > room + 0.01 || unshowable)) || (kind == CellKind::Boolean && !wrap_text && widths[0] > room + 0.01) {
        let hash = width("#", k).max(0.1);
        let count = (room / hash).floor().max(0.0) as usize;
        lines[0] = "#".repeat(count);
        widths[0] = hash * count as f32;
    }

    // ECMA-376 §18.18.40: fill on a number is not repeated and displays left-aligned.
    if align == CellAlign::Fill && kind != CellKind::Number {
        // Repeat complete entries; a partial entry is clipped rather than overflowing.
        for (text, measured) in lines.iter_mut().zip(&mut widths) {
            if *measured > 0.0 && measured.is_finite() && room.is_finite() {
                let copies = (room / *measured).floor().max(1.0) as usize;
                // Bound allocation for zero-width glyph sequences mixed with visible text.
                let copies = copies.min((1_048_576 / text.len().max(1)).max(1));
                *text = text.repeat(copies);
                *measured *= copies as f32;
            }
        }
    }

    let orig_text_size = base * if text.is_ascii() { latin_scale } else { 1.0 };
    let text_size = orig_text_size * k;
    let orig_line = (ascent + descent) * orig_text_size;
    let line = (ascent + descent) * text_size;
    let japanese = text.chars().any(|c| !c.is_ascii());
    let mut pitch = if is_vertical {
        line_pitch(style.font.as_deref(), false).map_or(line, |p| p * base * k)
    } else {
        line_pitch(style.font.as_deref(), japanese).map_or(line, |p| p * base * k)
    };
    if matches!(vertical, CellVerticalAlign::Justify | CellVerticalAlign::Distributed) && lines.len() > 1 && !is_vertical {
        pitch = pitch.max((bottom - top - pad - line) / (lines.len() - 1) as f32);
    }
    let rich = cell.runs.as_ref().filter(|_| !is_vertical && style.rotation == 0 && !matches!(align, CellAlign::Distributed | CellAlign::Fill | CellAlign::Justify));
    let mut line_places: Option<Vec<(usize, usize)>> = rich.filter(|r| (wrap_text || lines.len() > 1) && r.iter().map(|x| x.text.as_str()).collect::<String>() == cell.text).and_then(|_| {
        let chars: Vec<char> = cell.text.chars().collect();
        let mut cursor = 0;
        lines
            .iter()
            .map(|l| {
                let l: Vec<char> = l.chars().collect();
                let at = (cursor..=chars.len().saturating_sub(l.len())).find(|&p| chars[p..p + l.len()] == l[..])?;
                cursor = at + l.len();
                Some((at, l.len()))
            })
            .collect()
    });
    let mut line_ratios: Vec<f32> = match (rich, &line_places) {
        (Some(runs), Some(places)) => places
            .iter()
            .map(|&(start, len)| {
                let r = runs_between(runs.as_slice(), start, start + len)
                    .iter()
                    .filter(|r| !r.text.is_empty())
                    .map(|r| r.size.unwrap_or(style.font_size) as f32 / style.font_size.max(1) as f32)
                    .fold(0.0f32, f32::max);
                if r > 0.0 { r } else { 1.0 }
            })
            .collect(),
        _ => vec![1.0; lines.len()],
    };
    let mut line_offsets = Vec::with_capacity(lines.len());
    let mut acc_y = 0.0f32;
    for (idx, &r_i) in line_ratios.iter().enumerate() {
        line_offsets.push(acc_y);
        if idx + 1 < line_ratios.len() {
            acc_y += pitch * r_i;
        }
    }
    // Where Excel prints the text box against the cell's padding (points,
    // measured against Excel for Mac's PDF at Calibri 11).
    let (v_shift_top, v_shift_center, v_shift_bottom) = (
        TEXT_SHIFT_TOP_PT * px_per_point,
        TEXT_SHIFT_CENTER_PT * px_per_point,
        TEXT_SHIFT_BOTTOM_PT * px_per_point,
    );
    let acc_shift = if matches!(style.underline, CellUnderline::SingleAccounting | CellUnderline::DoubleAccounting) {
        3.5 * px_per_point
    } else {
        0.0
    };
    // Ruby occupies its own font-metric line above the base text. Place both
    // lines as one block, rather than painting the ruby inside the base line.
    let ruby_height = if cell.phonetic.as_deref().is_some_and(|s|!s.is_empty()) && !is_vertical && style.rotation == 0 {
        (ascent + descent) * base * 0.5 * 1.1 // Requested gap: ten percent of the ruby line.
    } else { 0. };
    let min_top = top + pad * 0.5 + v_shift_top + ruby_height;
    if !is_vertical && lines.len() > 1 {
        let mut max_lines = 1;
        while max_lines < lines.len() {
            let line_top = min_top + line_offsets[max_lines];
            if line_top >= bottom {
                break;
            }
            max_lines += 1;
        }
        lines.truncate(max_lines);
        line_ratios.truncate(max_lines);
        line_offsets.truncate(max_lines);
        if let Some(p) = &mut line_places {
            p.truncate(max_lines);
        }
    }
    let last_ratio = line_ratios.last().copied().unwrap_or(1.0);
    let last_offset = line_offsets.last().copied().unwrap_or(0.0);
    let block = last_offset + line * last_ratio;
    let first_top = match vertical {
        CellVerticalAlign::Top | CellVerticalAlign::Justify => min_top,
        CellVerticalAlign::Distributed if lines.len() > 1 && !is_vertical => min_top,
        CellVerticalAlign::Center | CellVerticalAlign::Distributed => {
            top + (bottom - top - block + ruby_height) / 2.0 + v_shift_center
        }
        CellVerticalAlign::Bottom => {
            let base_block = if style.shrink { last_offset + orig_line * last_ratio } else { block };
            let t = bottom - pad * 0.5 - base_block - v_shift_bottom - acc_shift;
            if is_vertical { t - 1.0 * px_per_point } else { t }
        }
    };
    // Text wider than its cell runs on over empty cells (never when wrapped,
    // or for numbers, which show `#` instead).
    let widest = widths.iter().cloned().fold(0.0, f32::max);
    let overflows = !wrap_text && !is_vertical && style.rotation == 0 && align != CellAlign::Fill && kind != CellKind::Number && widest > room;
    let (clip_left, clip_right) = if overflows {
        cell.spans[match align {
            CellAlign::Right => 0,
            CellAlign::Center | CellAlign::CenterAcross => 1,
            _ => 2,
        }]
    } else {
        (left, right)
    };
    let color = hsla(opaque(style.color));

    if style.rotation >= 1 && style.rotation <= 180 {
        let total_width = widths[0];
        let a = ascent * text_size;
        let d = descent * text_size;
        let w = total_width;

        let angle_degrees = if style.rotation <= 90 {
            -(style.rotation as f32)
        } else {
            (style.rotation - 90) as f32
        };
        let angle = angle_degrees.to_radians();
        let (sin, cos) = angle.sin_cos();

        let c1_x = a * sin;
        let c1_y = -a * cos;
        let c2_x = w * cos + a * sin;
        let c2_y = w * sin - a * cos;
        let c3_x = w * cos - d * sin;
        let c3_y = w * sin + d * cos;
        let c4_x = -d * sin;
        let c4_y = d * cos;

        let min_bx = c1_x.min(c2_x).min(c3_x).min(c4_x);
        let max_bx = c1_x.max(c2_x).max(c3_x).max(c4_x);
        let min_by = c1_y.min(c2_y).min(c3_y).min(c4_y);
        let max_by = c1_y.max(c2_y).max(c3_y).max(c4_y);

        let baseline_y0 = match vertical {
            CellVerticalAlign::Top | CellVerticalAlign::Justify => {
                top + pad * 0.5 + v_shift_top - min_by
            }
            CellVerticalAlign::Center | CellVerticalAlign::Distributed => {
                (top + bottom - (min_by + max_by)) / 2.0 + v_shift_center
            }
            CellVerticalAlign::Bottom => {
                bottom - pad * 0.5 - v_shift_bottom - acc_shift - max_by
            }
        };

        let (pad_left, pad_right) = (TEXT_PAD_LEFT_PT * px_per_point, TEXT_PAD_RIGHT_PT * px_per_point);
        let baseline_x0 = match style.align {
            CellAlign::Center | CellAlign::CenterAcross | CellAlign::Distributed => {
                if style.rotation >= 90 {
                    (left + right - (min_bx + max_bx)) / 2.0 - 3.5 * px_per_point
                } else {
                    (left + right - w * cos) / 2.0 - 1.0 * px_per_point
                }
            }
            CellAlign::Right => {
                right - pad_right - indent - max_bx
            }
            CellAlign::General => {
                if kind == CellKind::Number || (style.rotation >= 90 && style.rotation < 180) {
                    right - pad_right - indent - max_bx - 3.5 * px_per_point
                } else {
                    left + pad_left + indent - min_bx
                }
            }
            CellAlign::Fill => {
                left + pad_left - min_bx
            }
            _ => {
                left + pad_left + indent - min_bx
            }
        };

        let pieces = segments(&lines[0], k);
        out.push(TextItem {
            pieces,
            font: font.clone(),
            plain: plain_font.clone(),
            color,
            align,
            left: baseline_x0,
            right: baseline_x0 + total_width,
            baseline: baseline_y0,
            underline: style.underline,
            strike: style.strike,
            clip: (left, top, right - left, bottom - top),
            cell: (left, right),
            font_size: base,
            rotation: style.rotation,
        });
        return;
    }

    let align = if is_vertical && style.align == CellAlign::General { CellAlign::Center } else { align };
    if is_vertical && wrap_text && lines.len() > 1 {
        let max_per_col = (((bottom - top - base * k) / pitch.max(1.0)).floor().max(0.0) as usize + 1).max(1);
        if lines.len() > max_per_col {
            let cols: Vec<&[String]> = lines.chunks(max_per_col).collect();
            let num_cols = cols.len();
            let col_step = (base * k * 1.2).min(((right - left - pad) / num_cols as f32).max(base * k * 0.5));
            for (col_idx, col) in cols.iter().enumerate() {
                let col_block = (col.len().saturating_sub(1)) as f32 * pitch + line;
                let col_first_top = match vertical {
                    CellVerticalAlign::Top | CellVerticalAlign::Justify => min_top,
                    CellVerticalAlign::Center | CellVerticalAlign::Distributed => {
                        top + (bottom - top - col_block) / 2.0 + v_shift_center
                    }
                    CellVerticalAlign::Bottom => {
                        bottom - pad * 0.5 - col_block - v_shift_bottom - acc_shift - 1.0 * px_per_point
                    }
                };
                let col_center_x = match align {
                    CellAlign::Right => right - TEXT_PAD_RIGHT_PT * px_per_point - indent - base * k * 0.5 - col_idx as f32 * col_step,
                    CellAlign::Left => left + TEXT_PAD_LEFT_PT * px_per_point + indent + base * k * 0.5 + (num_cols - 1 - col_idx) as f32 * col_step,
                    _ => (left + right + (num_cols - 1) as f32 * col_step) / 2.0 - col_idx as f32 * col_step,
                };
                let (l, r) = (col_center_x - base * k, col_center_x + base * k);
                for (j, ch_text) in col.iter().enumerate() {
                    let mut baseline = col_first_top + j as f32 * pitch + (line - (ascent + descent) * text_size) / 2.0 + ascent * text_size;
                    if style.vert_align == VerticalPosition::Superscript {
                        baseline -= base * 0.58;
                    } else if style.vert_align == VerticalPosition::Subscript {
                        baseline += base * 0.18;
                    }
                    out.push(TextItem {
                        pieces: segments(ch_text, k),
                        font: font.clone(),
                        plain: plain_font.clone(),
                        color,
                        align: CellAlign::Center,
                        left: l,
                        right: r,
                        baseline,
                        underline: style.underline,
                        strike: style.strike,
                        clip: (clip_left, top, clip_right - clip_left, bottom - top),
                        cell: (left, right),
                        font_size: base,
                        rotation: style.rotation,
                    });
                }
            }
            return;
        }
    }
    let line_count = lines.len();
    for (i, text) in lines.into_iter().enumerate() {
        // Justified text is spread as distributed text is, but on the
        // lines that do not end a paragraph (Excel's PDF of xlsx-XALN).
        let align = match align {
            CellAlign::Justify if !ends.get(i).copied().unwrap_or(true) => CellAlign::Distributed,
            CellAlign::Justify => CellAlign::Left,
            CellAlign::Distributed if ibis_pptx::EXCEL_METRICS == ibis_pptx::ExcelMetrics::Windows => CellAlign::Center,
            a => a,
        };
        let r_i = line_ratios.get(i).copied().unwrap_or(1.0);
        let y_off = line_offsets.get(i).copied().unwrap_or(i as f32 * pitch);
        let mut baseline = if style.shrink && vertical == CellVerticalAlign::Bottom {
            first_top + y_off + ascent * orig_text_size * r_i
        } else {
            first_top + y_off + (line - (ascent + descent) * text_size) * r_i / 2.0 + ascent * text_size * r_i
        };
        if style.vert_align == VerticalPosition::Superscript {
            baseline -= base * 0.58;
        } else if style.vert_align == VerticalPosition::Subscript {
            baseline += base * 0.18;
        }
        // Excel prints a line's baseline on a whole point from the sheet's
        // top, the next one down (its PDFs of xlsx-XALN: top, centred and
        // bottom lines in 48 pt rows at 13, 28 and 43 pt where these are
        // 12.73, 27.75 and 42.78).
        if !is_vertical && (line_count == 1 || ibis_pptx::EXCEL_METRICS != ibis_pptx::ExcelMetrics::Windows || matches!(vertical, CellVerticalAlign::Justify | CellVerticalAlign::Distributed)) {
            baseline = (baseline / px_per_point - 1e-3).ceil() * px_per_point;
        }
        // (Fitted to Excel's PDFs of xlsx-XBASE and XFNT; the cause not
        // confirmed.) A single line on Excel's measures (its PDF of xlsx-XBASE): bottom-
        // aligned on the row's shared baseline, a superscript 4 pt above
        // it (5 in an MS font) and a subscript 3 pt below (2); centred with
        // its baseline 3.5 pt (at 11 pt) below the middle, on the next whole
        // point; top-aligned in an MS font at its size below the top.
        if !is_vertical && style.rotation == 0 && line_count == 1 {
            let size = style.font_size as f32 / 100.0;
            let ms = ms_font(style.font.as_deref());
            if let (CellVerticalAlign::Bottom, Some(gap)) = (vertical, cell.row_gap) {
                // A superscript above the row's baseline: `floor(0.43 · size)`
                // (half the size in an MS font or a cell left at the default:
                // 7 pt at 14, XFNT-00023); a subscript below it: in a cell
                // naming its alignment, `ceil(0.3 · size)` above the cell's
                // bottom (in an MS font `floor(0.18 · size + 0.45)` below the
                // baseline), else 2 pt below the baseline (XSUP, XFNT).
                let row_pt = (bottom - top) / px_per_point;
                let gap = if row_pt - gap < size + 2.0 {
                    let desc = if ms { 0.1406 } else { cell.font.descent };
                    desc * size + 0.4
                } else {
                    gap
                };
                let set = style.vertical_set;
                let shift = match style.vert_align {
                    VerticalPosition::Superscript if ms || !set => -(0.5 * size + 1e-3).floor(),
                    VerticalPosition::Superscript => -(0.43 * size + 1e-3).floor(),
                    VerticalPosition::Subscript if set && !ms => gap - (0.3 * size - 1e-3).ceil(),
                    VerticalPosition::Subscript if set => (0.18 * size + 0.45).floor(),
                    VerticalPosition::Subscript => 2.0,
                    _ => 0.0,
                };
                baseline = bottom - (gap - shift) * px_per_point;
            } else if vertical == CellVerticalAlign::Center && style.vert_align == VerticalPosition::Baseline {
                let offset = (size * 0.5 - CELL_PADDING_PT).max(size * 0.25);
                baseline = (((top + bottom) / 2.0 / px_per_point) + offset - 1e-3).ceil() * px_per_point;
            } else if vertical == CellVerticalAlign::Top && ms && style.vert_align == VerticalPosition::Baseline {
                baseline = (top / px_per_point + size - 1e-3).ceil() * px_per_point;
            }
        }
        let pieces: Vec<Piece> = if align == CellAlign::Distributed {
            // One piece a word with the spaces after it, and one a wide
            // (Japanese) character: Excel spreads the room between them
            // (room keeps its own piece).
            segments(&text, k)
                .into_iter()
                .flat_map(|p| {
                    if p.advance.is_some() {
                        return vec![p];
                    }
                    let wide = |c: char| c as u32 >= 0x2E80;
                    let mut units: Vec<String> = Vec::new();
                    let mut prev: Option<char> = None;
                    for c in p.text.chars() {
                        let starts = c != ' ' && prev.is_none_or(|q| q == ' ' || wide(q) || wide(c));
                        match units.last_mut() {
                            Some(u) if !starts => u.push(c),
                            _ => units.push(c.to_string()),
                        }
                        prev = Some(c);
                    }
                    units.into_iter().map(|t| Piece { text: t, ..p.clone() }).collect()
                })
                .collect()
        } else if let Some(runs) = rich
            .filter(|r| (!wrap_text && line_count == 1 && r.iter().map(|x| x.text.as_str()).collect::<String>() == text) || line_places.is_some())
            .map(|r| match line_places.as_ref().and_then(|p| p.get(i)) {
                Some(&(start, len)) => std::borrow::Cow::Owned(runs_between(r.as_slice(), start, start + len)),
                None => std::borrow::Cow::Borrowed(r.as_slice()),
            })
        {
            // Parts in their own fonts: bold, italic, size, colour, strike and underline.
            let mut out: Vec<Piece> = Vec::new();
            for run in runs.iter() {
                let (bold, italic) = (run.bold.unwrap_or(style.bold), run.italic.unwrap_or(style.italic));
                let mut f = font.clone();
                f.weight = if bold { FontWeight::BOLD } else { FontWeight::NORMAL };
                f.style = if italic { FontStyle::Italic } else { FontStyle::Normal };
                let paint = Rc::new(RunPaint { font: f, color: run.color.map(|c| hsla(opaque(c))), strike: run.strike.unwrap_or(style.strike), underline: run.underline.unwrap_or(style.underline) });
                let ratio = run.size.map_or(1.0, |s| s as f32 / style.font_size.max(1) as f32);
                for mut p in segments(&run.text, k * ratio) {
                    p.plain = false;
                    p.run = Some(paint.clone());
                    out.push(p);
                }
            }
            out
        } else {
            segments(&text, k)
        };
        let pieces_rich = pieces.iter().any(|p| p.run.is_some());
        let (pad_left, pad_right) = if align == CellAlign::Fill {
            (CELL_PADDING_PT * px_per_point, CELL_PADDING_PT * px_per_point)
        } else {
            (TEXT_PAD_LEFT_PT * px_per_point, TEXT_PAD_RIGHT_PT * px_per_point)
        };
        let num_pad = if kind == CellKind::Number { TEXT_PAD_NUM_LEFT_PT * px_per_point } else { 0.0 };
        let (l, r) = match align {
            CellAlign::Right => (right - pad_right - indent, right - pad_right - indent),
            // Centred text is centred on the cell itself (compute_text_x).
            CellAlign::Center | CellAlign::CenterAcross => (left, right),
            CellAlign::Left if kind == CellKind::Number => (left + pad_left + num_pad + indent, right - pad_right),
            // Distributed text is indented at both ends (XALN-00254…).
            CellAlign::Distributed => (left + pad_left + indent, right - pad_right - indent),
            _ => (left + pad_left + indent, right - pad_right),
        };
        out.push(TextItem {
            pieces,
            font: font.clone(),
            plain: plain_font.clone(),
            color,
            align,
            left: l,
            right: r,
            baseline,
            underline: if rich.is_some() && pieces_rich { CellUnderline::None } else { style.underline },
            strike: style.strike && !pieces_rich,
            clip: (clip_left, top, clip_right - clip_left, bottom - top),
            cell: (left, right),
            font_size: base,
            rotation: style.rotation,
        });
    }
    if let Some(phonetic) = cell.phonetic.as_deref() && !phonetic.is_empty() && !is_vertical && style.rotation == 0 {
        let ruby_scale = 0.5f32;
        let ruby_size = base * ruby_scale;
        let ruby_pieces = segments(phonetic, ruby_scale);
        let main=out.last();
        let ruby_baseline = main.map_or(first_top,|m|m.baseline-ascent*base) - descent * ruby_size - (ascent + descent) * ruby_size * 0.1;
        let (pad_left, pad_right) = (TEXT_PAD_LEFT_PT * px_per_point, TEXT_PAD_RIGHT_PT * px_per_point);
        let (mut l, r) = (left + pad_left + indent, right - pad_right);
        if let Some(main)=main {
            let main_width=main.pieces.iter().map(|p|p.advance.unwrap_or_else(||measure(&p.text,font_of(p.plain),p.size))).sum::<f32>();
            l=match main.align {CellAlign::Center=>(main.left+main.right-main_width)/2.,CellAlign::Right=>main.right-main_width,_=>main.left};
        }
        out.push(TextItem {
            pieces: ruby_pieces,
            font: font.clone(),
            plain: plain_font.clone(),
            color,
            align: CellAlign::Left,
            left: l,
            right: r,
            baseline: ruby_baseline,
            underline: CellUnderline::None,
            strike: false,
            clip: (clip_left, top, clip_right - clip_left, bottom - top),
            cell: (left, right),
            font_size: ruby_size,
            rotation: 0,
        });
    }
}

/// A piece of the text being typed, shaped, `x` pixels from the cell's left.
pub(crate) struct EditPiece {
    /// None for room of a fixed width.
    pub(crate) shaped: Option<ShapedLine>,
    pub(crate) x: f32,
    pub(crate) width: f32,
    /// Where it starts in the text (bytes) and its length.
    pub(crate) start: usize,
    pub(crate) len: usize,
}

/// A line of the text being typed: where its glyphs are, in pixels from the cell's top-left.
pub(crate) struct EditLine {
    pub(crate) pieces: Vec<EditPiece>,
    pub(crate) baseline: f32,
    pub(crate) ascent: f32,
    pub(crate) descent: f32,
    /// The bytes of the text it covers.
    pub(crate) start: usize,
    pub(crate) end: usize,
    /// Where an empty line's caret goes.
    pub(crate) x0: f32,
}

impl EditLine {
    /// The x (from the cell's left) of the boundary before byte `b` of the text.
    pub(crate) fn x_at(&self, b: usize) -> f32 {
        let b = b.clamp(self.start, self.end);
        for p in &self.pieces {
            if b >= p.start && b <= p.start + p.len {
                return p.x + p.shaped.as_ref().map_or(if b > p.start { p.width } else { 0.0 }, |s| f32::from(s.x_for_index(b - p.start)));
            }
        }
        self.pieces.last().map_or(self.x0, |p| p.x + p.width)
    }
}

/// The text being typed into a cell, laid out by the rules of a cell not being edited (`text_items`),
/// so that it does not move when editing starts. The box it needs: `width` and `height` in pixels.
pub(crate) struct EditLayout {
    pub(crate) lines: Vec<EditLine>,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl std::fmt::Debug for EditLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EditLayout({} lines)", self.lines.len())
    }
}

impl EditLayout {
    /// The line the byte `b` of the text is in.
    pub(crate) fn line_of(&self, b: usize) -> usize {
        self.lines.iter().position(|l| b <= l.end).unwrap_or(self.lines.len().saturating_sub(1))
    }
}

/// Style of the cell at (`row`, `column`), falling back to the workbook's
/// default style (vertically centred in a new workbook or CSV) when the cell
/// has not been created yet.
pub(crate) fn cell_or_default_style(sheet: &Sheet, default_style: Option<&CellStyle>, row: u32, column: u32) -> CellStyle {
    sheet
        .cell(row, column)
        .map(|c| sheet.styles[c.style as usize].clone())
        .or_else(|| default_style.cloned())
        .unwrap_or_else(|| CellStyle {
            vertical: CellVerticalAlign::Center,
            vertical_set: true,
            ..Default::default()
        })
}

/// `text` typed into the cell at (`row`, `column`) whose box is `cell` (width, height) pixels, `px_per_point`
/// pixels to a point, laid out as `text_items` lays out that cell's shown text.
pub(crate) fn edit_layout(ts: &WindowTextSystem, sheet: &Sheet, default_style: Option<&CellStyle>, res: &Resources, row: u32, column: u32, text: &str, runs: Option<Arc<Vec<ibis_doc_model::CellRun>>>, origin: (f32, f32), cell: (f32, f32), px_per_point: f32, max_width: f32) -> EditLayout {
    let own = sheet.cell(row, column);
    let mut style = cell_or_default_style(sheet, default_style, row, column);
    let kind = own.map_or(CellKind::Text, |c| c.kind);
    let original_wrap = style.wrap;
    let multi = text.contains('\n');
    style.wrap = style.wrap || multi;
    style.rotation = 0;
    style.shrink = false;
    style.vert_align = VerticalPosition::Baseline;
    style.align = match style.align {
        CellAlign::Fill | CellAlign::Justify | CellAlign::Distributed => CellAlign::Left,
        CellAlign::CenterAcross => CellAlign::Center,
        a => a,
    };
    if matches!(style.vertical, CellVerticalAlign::Justify | CellVerticalAlign::Distributed) {
        style.vertical = CellVerticalAlign::Top;
    }
    let font = cell_font(&style, res);
    let measure = |t: &str, f: &Font, size: f32| -> f32 {
        let run = TextRun { len: t.len(), font: f.clone(), color: black(), background_color: None, underline: None, strikethrough: None };
        let mut line = ts.shape_line(SharedString::from(t.to_string()), px(size), &[run], None);
        whole_point_advances(&mut line, px_per_point);
        f32::from(line.width)
    };
    // Bottom-aligned single lines share their row's baseline, the highest any of them asks for.
    let gap_of = |s: &CellStyle, t: &str| -> Option<f32> {
        let probe = CellText { text: t.to_string(), kind: CellKind::Text, general: None, style: s.clone(), font: font.clone(), rect: (0.0, 0.0, 0.0, 0.0), spans: [(0.0, 0.0); 3], row_gap: None, runs: None, phonetic: None };
        bottom_gap(&probe)
    };
    let mut row_gap = gap_of(&style, text);
    if row_gap.is_some() {
        let first = sheet.cells.partition_point(|c| c.row < row);
        for c in sheet.cells[first..].iter().take_while(|c| c.row == row) {
            if c.column != column && !c.text.is_empty() {
                if let Some(g) = gap_of(&sheet.styles[c.style as usize], &c.text) {
                    row_gap = Some(row_gap.map_or(g, |m| m.max(g)));
                }
            }
        }
    }
    let make = |right: f32, bottom: f32| CellText {
        text: text.to_string(),
        kind,
        general: None,
        style: style.clone(),
        font: font.clone(),
        rect: (origin.0, origin.1, origin.0 + right, origin.1 + bottom),
        spans: [(origin.0, origin.0 + right); 3],
        row_gap,
        runs: runs.clone(),
        phonetic: None,
    };
    let total = |t: &TextItem| -> f32 { t.pieces.iter().map(|p| p.advance.unwrap_or_else(|| measure(&p.text, if p.plain { &t.plain } else { &t.font }, p.size))).sum() };
    // The widest line, to give a box that does not wrap the room it needs.
    let mut right = cell.0;
    if !original_wrap {
        let mut probe = Vec::new();
        text_items(&make(1.0e6, 1.0e6), px_per_point, &measure, &mut probe);
        let widest = probe.iter().map(&total).fold(0.0, f32::max);
        let indent = style.indent as f32 * 9.0 * px_per_point;
        right = right.max(widest + (TEXT_PAD_LEFT_PT + TEXT_PAD_RIGHT_PT) * px_per_point + indent + 2.0 * px_per_point);
    }
    let capped=right>max_width;
    right=right.min(max_width.max(1.));
    let make=|r,b|{let mut cell=make(r,b);if capped{cell.style.wrap=true;cell.style.align=CellAlign::Left;}cell};
    // The room the lines need: all of them laid out (text_items drops those below the box), the box then as high as they are.
    let biggest = |t: &TextItem| t.pieces.iter().map(|p| p.size).fold(t.font_size, f32::max);
    let mut items = Vec::new();
    text_items(&make(right, 1.0e6), px_per_point, &measure, &mut items);
    let mut bottom = cell.1;
    if let (Some(first), Some(last)) = (items.first(), items.last()) {
        let block = last.baseline - first.baseline + font.ascent * biggest(first) + font.descent * biggest(last);
        bottom = bottom.max(block + 4.0 * px_per_point);
    }
    items.clear();
    text_items(&make(right, bottom), px_per_point, &measure, &mut items);
    let mut lines = Vec::new();
    let mut cursor = 0usize;
    let ascent = font.ascent;
    for t in &items {
        let line_text: String = t.pieces.iter().map(|p| p.text.as_str()).collect();
        // The line's place in the text: after the previous line, over a line break.
        let start = text[cursor..].find(&line_text).map_or(cursor, |i| cursor + i);
        let end = start + line_text.len();
        let mut piece_at=start;
        let shaped: Vec<(Option<ShapedLine>, f32)> = t
            .pieces
            .iter()
            .map(|p| {let piece_start=piece_at;piece_at+=p.text.len();match p.advance {
                Some(a) => (None, a),
                None => {
                    let f = match &p.run {
                        Some(r) => r.font.clone(),
                        None if p.plain => t.plain.clone(),
                        None => t.font.clone(),
                    };
                    let color = p.run.as_ref().and_then(|r| r.color).unwrap_or(t.color);
                    let run = TextRun {
                        len: p.text.len(),
                        font: f,
                        color,
                        background_color: None,
                        underline: None,
                        strikethrough: p.run.as_ref().map_or(t.strike, |r| r.strike).then(|| StrikethroughStyle { thickness: px((p.size / 14.0).max(0.5)), color: Some(color) }),
                    };
                    let runs=super::formula_reference::color_runs(text,piece_start,p.text.len(),run);
                    let mut line = ts.shape_line(SharedString::from(p.text.clone()), px(p.size), &runs, None);
                    whole_point_advances(&mut line, px_per_point);
                    let w = f32::from(line.width);
                    (Some(line), w)
                }
            }})
            .collect();
        let sum: f32 = shaped.iter().map(|(_, w)| *w).sum();
        let trailing = t.pieces.last().is_some_and(|p| p.advance.is_none() && p.text.ends_with(' '));
        let (mut x, _) = compute_text_x(t.align, t.left, t.right, sum, shaped.len(), trailing, 0, px_per_point);
        x -= origin.0;
        let x0 = x;
        let mut at = start;
        let mut pieces = Vec::new();
        for ((line, w), p) in shaped.into_iter().zip(&t.pieces) {
            pieces.push(EditPiece { shaped: line, x, width: w, start: at, len: p.text.len() });
            at += p.text.len();
            x += w;
        }
        let big = biggest(t);
        let a = ascent * big;
        lines.push(EditLine { pieces, baseline: t.baseline - origin.1, ascent: a, descent: font.descent * big, start, end, x0 });
        cursor = if text[end..].starts_with("\r\n") { end + 2 } else if text[end..].starts_with('\n') { end + 1 } else { end };
    }
    if lines.is_empty() {
        // Nothing typed: the caret waits where text would start.
        let probe = CellText { text: "x".into(), ..make(right, bottom) };
        let mut one = Vec::new();
        text_items(&probe, px_per_point, &measure, &mut one);
        if let Some(t) = one.first() {
            let (x0, _) = compute_text_x(t.align, t.left, t.right, 0.0, 1, false, 0, px_per_point);
            lines.push(EditLine { pieces: Vec::new(), baseline: t.baseline - origin.1, ascent: ascent * t.font_size, descent: font.descent * t.font_size, start: 0, end: 0, x0: x0 - origin.0 });
        }
    }
    // Text ending in a line break: the caret waits on an empty line after it.
    if text.ends_with('\n') {
        if let Some(last) = lines.last() {
            let pitch = line_pitch(style.font.as_deref(), text.chars().any(|c| !c.is_ascii())).unwrap_or(1.2) * style.font_size as f32 / 100.0 * px_per_point;
            let (a, d, x0) = (last.ascent, last.descent, lines[0].x0);
            let baseline = last.baseline + pitch;
            lines.push(EditLine { pieces: Vec::new(), baseline, ascent: a, descent: d, start: text.len(), end: text.len(), x0 });
            bottom = bottom.max(baseline + d + CELL_PADDING_PT * px_per_point);
        }
    }
    EditLayout { lines, width: right, height: bottom }
}

/// The parts of `runs` between the characters `from` and `to` of their text.
pub(crate) fn runs_between(runs: &[ibis_doc_model::CellRun], from: usize, to: usize) -> Vec<ibis_doc_model::CellRun> {
    let mut out = Vec::new();
    let mut at = 0;
    for run in runs {
        let n = run.text.chars().count();
        let (a, b) = (from.max(at), to.min(at + n));
        if a < b {
            out.push(ibis_doc_model::CellRun { text: run.text.chars().skip(a - at).take(b - a).collect(), ..run.clone() });
        }
        at += n;
    }
    out
}

/// Whether the first letter with a direction in `text` is a right-to-left
/// one (Hebrew, Arabic, Syriac, Thaana, N'Ko and their presentation forms).
fn starts_right_to_left(text: &str) -> bool {
    text.chars()
        .find(|c| c.is_alphabetic())
        .is_some_and(|c| matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF))
}

/// Horizontal position and intra-piece gap for a line of text, between
/// `left` and `right` as `text_items` sets them for the alignment.
///
/// Centred text is centred on the cell itself, its start put on the whole
/// point at or left of the exact place (Excel's PDF, XALN: "Ab 字" 27 pt
/// wide in a cell from 7 to 89 pt starts at 34, a number 51 pt wide in one
/// from 407 to 489 at 422). Vertical text (rotation 255) is a point left
/// and on a whole pixel (measured on XALN-00311/00312).
fn compute_text_x(align: CellAlign, left: f32, right: f32, total: f32, count: usize, trailing: bool, rotation: u8, px_per_point: f32) -> (f32, f32) {
    match align {
        CellAlign::Right => (right - total, 0.0),
        CellAlign::Center | CellAlign::CenterAcross if rotation == 255 => (((left + right - total) / 2.0 - 1.0 * px_per_point).round(), 0.0),
        CellAlign::Center | CellAlign::CenterAcross => ((((left + right - total) / 2.0) / px_per_point + 1e-3).floor() * px_per_point, 0.0),
        // Spread: the room shared between the pieces, and after a line's
        // closing space (its trailing space shows as room at the end).
        CellAlign::Distributed if count > 1 => (left, ((right - left - total) / (count - 1 + trailing as usize) as f32).max(0.0)),
        CellAlign::Distributed => ((left + right - total) / 2.0, 0.0),
        CellAlign::Fill => (left, 0.0),
        _ => (left + TEXT_LEFT_OFFSET_PX, 0.0),
    }
}

/// Characters `palt` narrows more than bold MS P Gothic does: drawn at
/// their full width when bold (the middle dot, which Excel sets with room
/// around it in bold; in regular weight it is as narrow as `palt` makes it,
/// as the PDF of a user's private workbook shows).
fn keeps_width(c: char) -> bool {
    matches!(c, '・' | '･')
}

/// The width, in ems, of a character of MS P Gothic (and its kin) that is
/// not close to its substitute's (measured on Excel's PDF output: 小＋８
/// ideographic spaces＋計, and 消・費・税 with three between, at 10.5 pt).
fn fixed_advance(c: char) -> Option<f32> {
    match c {
        '\u{3000}' => Some(0.654),
        _ => None,
    }
}

/// MS P Gothic, MS P Mincho and the like, whose kana and punctuation are
/// narrower than full-width.
fn proportional_japanese(typeface: Option<&str>) -> bool {
    typeface.is_some_and(|n| n.starts_with("ＭＳ Ｐ") || n.starts_with("MS P")) // i18n-ok（フォント名の判定（表示名ではなく、フォント名の接頭辞））
}

/// How far apart Excel sets the lines of wrapped text, in ems of the
/// cell's size, for typefaces measured on Excel for Mac's PDF output (MS P
/// Gothic 9 pt: 13 pt; Meiryo UI 8 pt: 11 pt); `None` for the font's own
/// height.
pub(crate) fn line_pitch(typeface: Option<&str>, japanese: bool) -> Option<f32> {
    let name = typeface?;
    if name.starts_with("ＭＳ") || name.starts_with("MS ") { // i18n-ok（フォント名の判定（表示名ではなく、フォント名の接頭辞））
        Some(if ibis_pptx::EXCEL_METRICS == ibis_pptx::ExcelMetrics::Windows { 13.5 / 11.0 } else { 13.0 / 9.0 })
    } else if name.starts_with("Meiryo") || name.starts_with("メイリオ") { // i18n-ok（フォント名の判定（表示名ではなく、フォント名の接頭辞））
        Some(11.0 / 8.0)
    } else if name.starts_with("Calibri") || name.starts_with("Carlito") {
        if japanese {
            Some(18.0 / 11.0)
        } else {
            Some(15.66 / 11.0)
        }
    } else if name.starts_with("游ゴシック") || name.starts_with("Yu Gothic") || name.starts_with("YuGothic") { // i18n-ok（フォント名の判定（表示名ではなく、フォント名の接頭辞））
        Some(18.0 / 11.0)
    } else {
        None
    }
}

/// Break `text` into lines no wider than `room` (by `width`), as Excel
/// wraps: between any two characters of Japanese, but back to the last
/// space when a line would end inside a Latin word, and never putting a
/// closing mark (、。）」…) at the start of a line. A line broken at a space
/// keeps the space at its end: Excel aligns it with the space's width (in
/// its PDF of XALN, "The quick " right-aligned starts 3 pt left of "The
/// quick", centred 1 pt left).
fn wrap(text: &str, room: f32, width: impl Fn(&str) -> f32) -> Vec<(String, bool)> {
    let closing = |c: char| "、。，．・：；？！）］｝」』】〕〉》’”)]}.,:;!?ー々ぁぃぅぇぉっゃゅょァィゥェォッャュョ".contains(c); // i18n-ok（閉じ括弧の類の文字（禁則処理の文字集合））
    let mut out = Vec::new();
    for paragraph in text.lines() {
        let mut line: Vec<char> = Vec::new();
        for c in paragraph.chars() {
            line.push(c);
            let s: String = line.iter().collect();
            if line.len() == 1 || width(&s) <= room {
                continue;
            }
            // `c` does not fit: where does the line end?
            let mut at = line.len() - 1;
            if let Some(space) = line[..at].iter().rposition(|&x| x == ' ') {
                // Back to the last ASCII space on the line, if any.
                at = space + 1;
            } else if closing(c) && at > 1 {
                // A closing mark takes the character before it along.
                at -= 1;
            }
            let rest: Vec<char> = line.split_off(at);
            let done: String = line.iter().collect();
            out.push((done, false));
            line = rest.into_iter().skip_while(|&x| x == ' ').collect();
        }
        out.push((line.into_iter().collect(), true));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Bands, CellText, CellFont, text_items, fill_sources};
    use ibis_doc_model::{CellAlign, CellKind, CellStyle};
    use gpui_kit::font;

    /// A stacked (rotation 255) text with a line break: the break is not a character to shape (`shape_line` panics on a
    /// newline in debug), so no piece holds one and the letters stack on.
    #[test]
    fn stacked_text_with_a_line_break_shapes_no_newline() {
        let cell = CellText { runs: None, phonetic: None,
            text: "ab\ncd".into(), kind: CellKind::Text, general: None,
            style: CellStyle { rotation: 255, wrap: true, font_size: 1000, ..Default::default() },
            font: CellFont { font: font("Arial"), plain: font("Arial"), latin_scale: 1., ascent: 0.8, descent: 0.2 },
            rect: (0., 0., 100., 200.), spans: [(0., 100.); 3], row_gap: None,
        };
        let measure = |t: &str, _: &gpui_kit::Font, _: f32| {
            assert!(!t.contains('\n') && !t.contains('\r'), "a newline is measured: {t:?}");
            t.chars().count() as f32 * 5.
        };
        let mut out = Vec::new();
        text_items(&cell, 1., &measure, &mut out);
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn vertical_justify_places_single_line_at_top_and_spreads_multiple_lines() {
        use ibis_doc_model::CellVerticalAlign;
        let make = |text: &str| CellText { runs: None, phonetic: None,
            text: text.into(), kind: CellKind::Text, general: None,
            style: CellStyle { vertical: CellVerticalAlign::Justify, wrap: true, font_size: 1000, ..Default::default() },
            font: CellFont { font: font("Arial"), plain: font("Arial"), latin_scale: 1., ascent: 0.8, descent: 0.2 },
            rect: (0., 0., 100., 60.), spans: [(0., 100.); 3], row_gap: None,
        };
        let measure = |t: &str, _: &gpui_kit::Font, _: f32| t.chars().count() as f32 * 5.;
        // The first line's top: half the padding and the top shift; its
        // baseline the ascent (8) below. Three lines spread over the rest.
        // (On the next whole point, as Excel prints a baseline.)
        let first = (super::CELL_PADDING_PT * 0.5 + super::TEXT_SHIFT_TOP_PT + 8. - 1e-3_f32).ceil();
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        let mut out = Vec::new(); text_items(&make("A"), 1., &measure, &mut out);
        assert!(near(out[0].baseline, first), "{}", out[0].baseline);
        let mut out = Vec::new(); text_items(&make("A\nB\nC"), 1., &measure, &mut out);
        assert_eq!(out.len(), 3); assert!(near(out[0].baseline, first));
        assert!(near(out[1].baseline, first + 24.)); assert!(near(out[2].baseline, first + 48.));
    }

    #[test]
    fn vertical_distributed_places_single_line_at_center_and_spreads_multiple_lines() {
        use ibis_doc_model::CellVerticalAlign;
        let make = |text: &str| CellText { runs: None, phonetic: None,
            text: text.into(), kind: CellKind::Text, general: None,
            style: CellStyle { vertical: CellVerticalAlign::Distributed, wrap: true, font_size: 1000, ..Default::default() },
            font: CellFont { font: font("Arial"), plain: font("Arial"), latin_scale: 1., ascent: 0.8, descent: 0.2 },
            rect: (0., 0., 100., 60.), spans: [(0., 100.); 3], row_gap: None,
        };
        let measure = |t: &str, _: &gpui_kit::Font, _: f32| t.chars().count() as f32 * 5.;
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        let mut out = Vec::new(); text_items(&make("A"), 1., &measure, &mut out);
        assert!(near(out[0].baseline, 33.), "{}", out[0].baseline);
        // (On the next whole point, as Excel prints a baseline.)
        let first = (super::CELL_PADDING_PT * 0.5 + super::TEXT_SHIFT_TOP_PT + 8. - 1e-3_f32).ceil();
        let mut out = Vec::new(); text_items(&make("A\nB\nC"), 1., &measure, &mut out);
        assert_eq!(out.len(), 3); assert!(near(out[0].baseline, first));
        assert!(near(out[1].baseline, first + 24.)); assert!(near(out[2].baseline, first + 48.));
    }

    #[test]
    fn fill_continues_only_through_adjacent_empty_fill_cells() {
        use ibis_doc_model::{Sheet, SheetCell, CellValue, CellRange};
        let mut sheet = Sheet::default();
        sheet.styles = vec![CellStyle::default(), CellStyle { align: CellAlign::Fill, ..Default::default() }];
        for column in 0..7 {
            let text = if column == 0 { "AB" } else { "" };
            sheet.put_cell(0, column, Some(SheetCell { runs: None, phonetic: None, row: 0, column, value: if text.is_empty() { CellValue::Empty } else { CellValue::Text(text.into()) }, text: text.into(), kind: if text.is_empty() { CellKind::Empty } else { CellKind::Text }, formula: None, style: 1 }));
        }
        assert_eq!(fill_sources(&sheet, &sheet.cells).len(), 6);
        sheet.cells[3].style = 0;
        let sources = fill_sources(&sheet, &sheet.cells); assert_eq!(sources.len(), 2); assert!(!sources.contains_key(&(0, 4)));
        sheet.cells[3].style = 1; sheet.cells[2].formula = Some("IF(TRUE,\"\",1)".into());
        assert_eq!(fill_sources(&sheet, &sheet.cells).len(), 1);
        sheet.cells[2].formula = None;
        sheet.merges.push(CellRange { first_row: 0, last_row: 0, first_column: 1, last_column: 2 });
        assert!(fill_sources(&sheet, &sheet.cells).is_empty());
    }

    #[test]
    fn wrapped_text_keeps_its_parts_on_each_line() {
        use ibis_doc_model::CellRun;
        let run = |t: &str, bold: bool| CellRun { text: t.into(), font: None, size: None, bold: Some(bold), italic: None, underline: None, strike: None, color: None };
        let runs = vec![run("AAAA BBBB ", true), run("CCCC DDDD", false)];
        let cell = CellText { runs: Some(std::sync::Arc::new(runs.clone())), phonetic: None,
            text: "AAAA BBBB CCCC DDDD".into(), kind: CellKind::Text, general: None,
            style: CellStyle { wrap: true, font_size: 1000, ..Default::default() },
            font: CellFont { font: font("Arial"), plain: font("Arial"), latin_scale: 1., ascent: 0.8, descent: 0.2 },
            rect: (0., 0., 60., 100.), spans: [(0., 100.); 3], row_gap: None,
        };
        let measure = |t: &str, _: &gpui_kit::Font, _: f32| t.chars().count() as f32 * 5.;
        let mut out = Vec::new(); text_items(&cell, 1., &measure, &mut out);
        assert!(out.len() >= 2, "wrapped onto lines");
        let text_of = |i: usize| out[i].pieces.iter().map(|p| p.text.as_str()).collect::<String>();
        assert!(out.iter().all(|t| t.pieces.iter().all(|p| p.run.is_some())), "every piece is in a part's font");
        assert!(text_of(0).starts_with("AAAA"));
        let bold = |i: usize| out[i].pieces.iter().map(|p| p.run.as_ref().unwrap().font.weight == gpui_kit::FontWeight::BOLD).collect::<Vec<_>>();
        assert!(bold(0).iter().all(|&b| b), "the first line is in the bold part");
        assert!(bold(out.len() - 1).iter().all(|&b| !b), "the last line is in the plain part");
        assert_eq!(super::runs_between(&runs, 5, 12).iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["BBBB ", "CC"]);
    }

    #[test]
    fn fill_repeats_complete_entries_and_clips_long_entries() {
        let make = |text: &str| CellText { runs: None, phonetic: None,
            text: text.into(), kind: CellKind::Text, general: None,
            style: CellStyle { align: CellAlign::Fill, font_size: 1000, ..Default::default() },
            font: CellFont { font: font("Arial"), plain: font("Arial"), latin_scale: 1., ascent: 0.8, descent: 0.2 },
            rect: (0., 0., 34., 20.), spans: [(0., 100.); 3], row_gap: None,
        };
        let measure = |t: &str, _: &gpui_kit::Font, _: f32| t.chars().count() as f32 * 5.;
        let mut out = Vec::new(); text_items(&make("AB"), 1., &measure, &mut out);
        assert_eq!(out[0].pieces.iter().map(|p| p.text.as_str()).collect::<String>(), "ABABAB");
        assert_eq!(out[0].clip.2, 34.);
        let mut out = Vec::new(); text_items(&make("ABCDEFGHIJ"), 1., &measure, &mut out);
        assert_eq!(out[0].pieces.iter().map(|p| p.text.as_str()).collect::<String>(), "ABCDEFGHIJ");
        assert_eq!(out[0].clip.2, 34., "fill cannot flow into an adjacent cell");
        let mut wrapped = make("ABCDEFGHIJ"); wrapped.style.wrap = true;
        let mut out = Vec::new(); text_items(&wrapped, 1., &measure, &mut out);
        assert_eq!(out.len(), 1, "fill ignores wrapping, as Office renders the entry on one line");
    }

    #[test]
    fn bands_find_each_item_once() {
        let h = Bands::HEIGHT;
        // A short item in the first band, a tall one over three, one in the third.
        let bands = Bands::new([(0.0, 10.0, 5.0, 5.0), (0.0, 0.0, 5.0, h * 2.5), (0.0, h * 2.2, 5.0, 5.0)].into_iter());
        assert_eq!(bands.find(0.0, h * 3.0), vec![0, 1, 2]);
        assert_eq!(bands.find(h * 2.0, h * 2.9), vec![1, 2]);
        assert_eq!(bands.find(h * 1.1, h * 1.2), vec![1]);
        assert!(bands.find(h * 9.0, h * 10.0).is_empty());
    }

    #[test]
    fn draw_contiguous_borders_merges_identical_adjacent_borders() {
        use super::{draw_contiguous_borders, Quad};
        use ibis_doc_model::{Border, BorderLine, Rgb};

        let border = Border { line: BorderLine::Thin, color: Rgb(0x000000) };
        let coords = [0.0, 50.0, 100.0, 150.0];
        let mut quads: Vec<Quad> = Vec::new();

        // Columns 0, 1, 2 all have the same border.
        draw_contiguous_borders(
            0,
            2,
            |c| coords[c as usize],
            |_| Some(border),
            true,
            false,
            10.0,
            1.0,
            &mut quads,
        );

        // Should produce exactly one merged quad from 0.0 to 150.0.
        assert_eq!(quads.len(), 1);
        let q = &quads[0];
        assert_eq!(q.rect.1, 10.0 - 1.0); // a 1 pt line ends on the grid line: [at - 1, at]
        assert!((q.rect.0 - 0.0).abs() < 1e-3);
        assert!((q.rect.2 - 150.0).abs() < 1e-3);
    }

    #[test]
    fn draw_contiguous_borders_splits_differing_borders_and_handles_gaps() {
        use super::{draw_contiguous_borders, Quad};
        use ibis_doc_model::{Border, BorderLine, Rgb};

        let b1 = Border { line: BorderLine::Thin, color: Rgb(0x111111) };
        let b2 = Border { line: BorderLine::Double, color: Rgb(0x222222) };
        let coords = [0.0, 50.0, 100.0, 150.0, 200.0];
        let mut quads: Vec<Quad> = Vec::new();

        // Col 0: b1, Col 1: None (gap), Col 2: b2, Col 3: b2
        draw_contiguous_borders(
            0,
            3,
            |c| coords[c as usize],
            |c| [Some(b1), None, Some(b2), Some(b2)][c as usize],
            true,
            false,
            20.0,
            1.0,
            &mut quads,
        );

        // Col 0 produces 1 quad (b1).
        // Col 1 produces 0 quads.
        // Col 2..=3 produces 2 quads (b2 is Double).
        assert_eq!(quads.len(), 3);
        assert!((quads[0].rect.0 - 0.0).abs() < 1e-3);
        assert!((quads[0].rect.2 - 50.0).abs() < 1e-3);
        assert_eq!(quads[1].rect.0, 100.0);
        assert_eq!(quads[1].rect.2, 100.0);
        assert_eq!(quads[2].rect.0, 100.0);
        assert_eq!(quads[2].rect.2, 100.0);
    }

    #[test]
    fn push_segmented_line_skips_cutouts_and_merges_overlaps() {
        use super::{push_segmented_line, Quad};
        use gpui_kit::hsla;

        let color = hsla(0.0, 0.0, 0.0, 1.0);

        // Test 1: No cutouts
        let mut quads: Vec<Quad> = Vec::new();
        push_segmented_line(true, 10.0, 1.0, 100.0, vec![], color, &mut quads);
        assert_eq!(quads.len(), 1);
        assert_eq!(quads[0].rect, (0.0, 10.0, 100.0, 1.0));

        // Test 2: Two cutouts (20..40, 60..80) along total_len = 100.0
        let mut quads: Vec<Quad> = Vec::new();
        push_segmented_line(false, 5.0, 1.0, 100.0, vec![(20.0, 40.0), (60.0, 80.0)], color, &mut quads);
        assert_eq!(quads.len(), 3);
        assert_eq!(quads[0].rect, (5.0, 0.0, 1.0, 20.0));
        assert_eq!(quads[1].rect, (5.0, 40.0, 1.0, 20.0));
        assert_eq!(quads[2].rect, (5.0, 80.0, 1.0, 20.0));

        // Test 3: Overlapping cutouts (10..40, 30..60)
        let mut quads: Vec<Quad> = Vec::new();
        push_segmented_line(true, 15.0, 1.0, 100.0, vec![(10.0, 40.0), (30.0, 60.0)], color, &mut quads);
        assert_eq!(quads.len(), 2);
        assert_eq!(quads[0].rect, (0.0, 15.0, 10.0, 1.0));
        assert_eq!(quads[1].rect, (60.0, 15.0, 40.0, 1.0));
    }

    #[test]
    fn centred_text_sits_on_the_cells_middle_at_a_whole_point() {
        use super::{compute_text_x, TEXT_LEFT_OFFSET_PX, TEXT_PAD_LEFT_PT, TEXT_PAD_NUM_LEFT_PT, TEXT_PAD_RIGHT_PT};
        let make = |text: &str, kind: CellKind, align: CellAlign| CellText { runs: None, phonetic: None,
            text: text.into(), kind, general: None,
            style: CellStyle { align, font_size: 1000, ..Default::default() },
            font: CellFont { font: font("Arial"), plain: font("Arial"), latin_scale: 1., ascent: 0.8, descent: 0.2 },
            rect: (10., 0., 110., 20.), spans: [(10., 110.); 3], row_gap: None,
        };
        let measure = |t: &str, _: &gpui_kit::Font, _: f32| t.chars().count() as f32 * 5.;
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        let x_of = |c: CellText| {
            let mut out = Vec::new();
            text_items(&c, 1., &measure, &mut out);
            compute_text_x(out[0].align, out[0].left, out[0].right, 30., 1, false, out[0].rotation, 1.).0
        };
        let start = 10. + TEXT_PAD_LEFT_PT + TEXT_LEFT_OFFSET_PX;
        let end = 110. - TEXT_PAD_RIGHT_PT;
        assert!(near(x_of(make("ABCDEF", CellKind::Text, CellAlign::Left)), start));
        assert!(near(x_of(make("ABCDEF", CellKind::Text, CellAlign::Right)) + 30., end));
        // Centred, text and numbers alike: on the cell's middle, on a whole point.
        assert!(near(x_of(make("ABCDEF", CellKind::Text, CellAlign::Center)), 45.));
        assert!(near(x_of(make("ABCDEF", CellKind::Text, CellAlign::CenterAcross)), 45.));
        let num_start = start + TEXT_PAD_NUM_LEFT_PT;
        assert!(near(x_of(make("123456", CellKind::Number, CellAlign::Left)), num_start));
        assert!(near(x_of(make("123456", CellKind::Number, CellAlign::Center)), 45.));
        assert!(near(compute_text_x(CellAlign::Center, 10., 111., 30., 1, false, 0, 1.).0, 45.));
    }

    #[test]
    fn broken_lines_show_unwrapped_as_far_as_the_cell_goes() {
        use ibis_doc_model::CellVerticalAlign;
        // Issue #17: a cell of a text file, its lines broken but not wrapped, shows them one under another.
        let make = |vert: CellVerticalAlign, height: f32| CellText { runs: None, phonetic: None,
            text: "Line 1\nLine 2\nLine 3\nLine 4".into(),
            kind: CellKind::Text,
            general: None,
            style: CellStyle { vertical: vert, font_size: 1000, ..Default::default() },
            font: CellFont { font: font("Arial"), plain: font("Arial"), latin_scale: 1., ascent: 0.8, descent: 0.2 },
            rect: (0., 0., 100., height),
            spans: [(0., 100.); 3], row_gap: None,
        };
        let measure = |_: &str, _: &gpui_kit::Font, _: f32| 10.;
        let count = |c: CellText| {
            let mut out = Vec::new();
            text_items(&c, 1., &measure, &mut out);
            out.len()
        };
        assert_eq!(count(make(CellVerticalAlign::Top, 100.)), 4);
        assert_eq!(count(make(CellVerticalAlign::Center, 100.)), 4);
        // A row too low for all of them shows those that start in it.
        let low = count(make(CellVerticalAlign::Top, 25.));
        assert!(low > 1 && low < 4, "{low}");
    }

    #[test]
    fn vertical_overflow_places_center_and_bottom_without_clamping_to_top() {
        use ibis_doc_model::CellVerticalAlign;
        let make = |vert: CellVerticalAlign| CellText { runs: None, phonetic: None,
            text: "Line 1\nLine 2\nLine 3".into(),
            kind: CellKind::Text,
            general: None,
            style: CellStyle {
                vertical: vert,
                font_size: 1000,
                wrap: true,
                ..Default::default()
            },
            font: CellFont { font: font("Arial"), plain: font("Arial"), latin_scale: 1., ascent: 0.8, descent: 0.2 },
            // Cell height = 15.0, where each line height is 10.0, so 3 lines overflow the cell.
            rect: (0., 0., 100., 15.),
            spans: [(0., 100.); 3], row_gap: None,
        };
        let measure = |_: &str, _: &gpui_kit::Font, _: f32| 10.;
        let baseline_of = |c: CellText| {
            let mut out = Vec::new();
            text_items(&c, 1., &measure, &mut out);
            out[0].baseline
        };

        let top_baseline = baseline_of(make(CellVerticalAlign::Top));
        let center_baseline = baseline_of(make(CellVerticalAlign::Center));
        let bottom_baseline = baseline_of(make(CellVerticalAlign::Bottom));

        // When content overflows vertically, Center sits higher than Top,
        // and Bottom sits even higher than Center, rather than clamping to Top.
        assert!(center_baseline < top_baseline, "center ({center_baseline}) should be higher than top ({top_baseline})");
        assert!(bottom_baseline < center_baseline, "bottom ({bottom_baseline}) should be higher than center ({center_baseline})");
    }

    #[test]
    fn shrink_to_fit_bottom_align_maintains_original_font_size_baseline() {
        use ibis_doc_model::CellVerticalAlign;
        let make = |shrink: bool, text_len: usize| CellText { runs: None, phonetic: None,
            text: "A".repeat(text_len),
            kind: CellKind::Text,
            general: None,
            style: CellStyle {
                vertical: CellVerticalAlign::Bottom,
                font_size: 1000,
                shrink,
                ..Default::default()
            },
            font: CellFont { font: font("Arial"), plain: font("Arial"), latin_scale: 1., ascent: 0.8, descent: 0.2 },
            rect: (0., 0., 50., 30.),
            spans: [(0., 50.); 3], row_gap: None,
        };
        let measure = |t: &str, _: &gpui_kit::Font, _: f32| t.chars().count() as f32 * 10.;
        let baseline_of = |c: CellText| {
            let mut out = Vec::new();
            text_items(&c, 1., &measure, &mut out);
            out[0].baseline
        };

        let normal_baseline = baseline_of(make(false, 1));
        let shrink_baseline = baseline_of(make(true, 10));

        let diff = (normal_baseline - shrink_baseline).abs();
        assert!(
            diff < 1e-4,
            "shrunk text baseline ({shrink_baseline}) should match normal font size baseline ({normal_baseline}), diff: {diff}"
        );
    }

    #[test]
    fn ruby_phonetic_rendered_when_present() {
        use ibis_doc_model::CellVerticalAlign;
        let mut cell = CellText {
            runs: None,
            phonetic: Some("トウキョウ".into()),
            text: "東京".into(),
            kind: CellKind::Text,
            general: None,
            style: CellStyle { font_size: 1200, ..Default::default() },
            font: CellFont { font: font("Arial"), plain: font("Arial"), latin_scale: 1., ascent: 0.8, descent: 0.2 },
            rect: (0., 0., 100., 40.),
            spans: [(0., 100.); 3],
            row_gap: None,
        };
        let measure = |t: &str, _: &gpui_kit::Font, _: f32| t.chars().count() as f32 * 6.;
        let mut out = Vec::new();
        text_items(&cell, 1., &measure, &mut out);
        // Main text + ruby phonetic item
        assert_eq!(out.len(), 2);
        let main = &out[0];
        let ruby = &out[1];
        assert_eq!(main.pieces[0].text, "東京");
        assert_eq!(ruby.pieces[0].text, "トウキョウ");
        // Ruby font size is half of main text size
        assert_eq!(ruby.font_size, 6.0);
        // Ruby baseline is above main text baseline
        assert!(ruby.baseline < main.baseline, "ruby baseline ({}) should be above main text baseline ({})", ruby.baseline, main.baseline);
        // Horizontal alignment must keep the reading at the base text's left,
        // and vertical centring must preserve the same typographic separation.
        let mut gaps = Vec::new();
        for horizontal in [CellAlign::Left, CellAlign::Center, CellAlign::Right] {
            for vertical in [CellVerticalAlign::Top, CellVerticalAlign::Center, CellVerticalAlign::Bottom] {
                let candidate = &mut cell;
                candidate.style.align = horizontal;
                candidate.style.vertical = vertical;
                candidate.style.vertical_set = true;
                let mut items = Vec::new();
                text_items(candidate, 1., &measure, &mut items);
                let (base, reading) = (&items[0], &items[1]);
                let text_width = measure("東京", &candidate.font.font, base.font_size);
                let expected = match horizontal {
                    CellAlign::Center => (base.left + base.right - text_width) / 2.,
                    CellAlign::Right => base.right - text_width,
                    _ => base.left,
                };
                assert!((reading.left - expected).abs() < 0.001);
                gaps.push(base.baseline - reading.baseline);
            }
        }
        assert!(gaps.iter().all(|gap| (gap - gaps[0]).abs() < 0.001));
    }

    /// GitHub #22: Excel 365 fills a dotted border with a tile 0.96 pt across, half of it grey: dots of 0.48 pt a
    /// step of 0.96 pt apart, from the paper's corner (its PDF's pattern), whatever the print scale.
    #[test]
    fn a_dotted_border_is_dots_of_half_the_tile_step_from_the_papers_corner() {
        use super::dotted_pattern;
        let near = |a: f32, b: f32| (a - b).abs() < 1e-4;
        // At 100 %, a pixel a point: dots of 0.48, gaps of 0.48, and a line that starts on the origin starts a dot.
        let (pattern, phase) = dotted_pattern(1.0, 0.0, 0.0, 1.0);
        assert!(pattern.len() == 2 && near(pattern[0], 0.48) && near(pattern[1], 0.48), "{pattern:?}");
        assert!(near(phase, 0.0));
        // A line that starts 1.2 pt in starts 0.24 into the tile (1.2 - 0.96), which is half a dot in.
        assert!(near(dotted_pattern(1.0, 0.0, 1.2, 1.0).1, 0.24));
        // The sheet's origin 10 pt from the paper's corner moves the start by as much: 10 + 1.2 = 11.2 = 11 x 0.96 + 0.64.
        assert!(near(dotted_pattern(1.0, 10.0, 1.2, 1.0).1, 0.64), "{}", dotted_pattern(1.0, 10.0, 1.2, 1.0).1);
        // Printed at 50 %, the tile stays 0.96 pt on the paper: a sheet point is half a paper point, at 2 px a sheet
        // point a paper point is 4 px, and a dot 0.48 of them.
        let (half, _) = dotted_pattern(0.5, 0.0, 0.0, 2.0);
        assert!(near(half[0], 1.92) && near(half[1], 1.92), "{half:?}");
        // At 200 % on the paper (a sheet point two paper points), half the pixels per paper point.
        let (double, _) = dotted_pattern(2.0, 0.0, 0.0, 2.0);
        assert!(near(double[0], 0.48) && near(double[1], 0.48), "{double:?}");
    }

    /// What `border_quads` draws of a border along a row or a column: the rectangles (the screen) and the dashed lines
    /// (printed) it makes.
    fn border_marks(line: ibis_doc_model::BorderLine, horizontal: bool, printed: bool, scale: f32, length: f32) -> (usize, Vec<super::DashedLine>) {
        use super::{BORDER_DASHES, PRINT_GRID, border_quads};
        BORDER_DASHES.with(|d| d.borrow_mut().clear());
        PRINT_GRID.with(|g| g.set(printed.then_some((1.0, 0.0, 0.0))));
        let mut quads = Vec::new();
        let border = ibis_doc_model::Border { line, color: ibis_doc_model::Rgb(0x808080) };
        border_quads(&border, horizontal, false, 100.0 * scale, 10.0 * scale, (10.0 + length) * scale, scale, &mut quads);
        PRINT_GRID.with(|g| g.set(None));
        (quads.len(), BORDER_DASHES.with(|d| std::mem::take(&mut *d.borrow_mut())))
    }

    /// GitHub #22: a printed dashed or dotted border (any style, along a row or a column, at any size) is one dashed
    /// line, not a rectangle for each dash; on the screen each dash is a rectangle.
    #[test]
    fn a_printed_dashed_border_is_one_dashed_line_and_a_screen_one_is_rectangles() {
        use ibis_doc_model::BorderLine::*;
        for line in [Dashed, Dotted, DashDot, DashDotDot, MediumDashed, MediumDashDot, MediumDashDotDot] {
            for horizontal in [true, false] {
                for scale in [1.0f32, 2.0] {
                    let what = format!("{line:?} horizontal {horizontal} at {scale}");
                    let (quads, lines) = border_marks(line, horizontal, true, scale, 100.0);
                    assert_eq!(quads, 0, "printed {what}: no rectangles");
                    assert_eq!(lines.len(), 1, "printed {what}: one dashed line");
                    let l = &lines[0];
                    assert!(l.width >= 0.9 * scale, "printed {what}: {} wide", l.width);
                    assert!(l.pattern.len() >= 2 && l.pattern.len() % 2 == 0 && l.pattern.iter().all(|v| *v > 0.0), "{what}: {:?}", l.pattern);
                    assert_eq!(l.points.len(), 2, "{what}");
                    // The line runs along the border, 100 sheet points of it, from its start.
                    let (a, b) = (l.points[0], l.points[1]);
                    let (along, across) = if horizontal { (b.0 - a.0, b.1 - a.1) } else { (b.1 - a.1, b.0 - a.0) };
                    // (A column's line starts a point above, as its solid line does.)
                    assert!(across.abs() < 1e-4 && (along - 100.0 * scale).abs() < 1.5 * scale, "{what}: {:?}", l.points);
                    let (quads, lines) = border_marks(line, horizontal, false, scale, 100.0);
                    assert!(quads > 5 && lines.is_empty(), "screen {what}: {quads} rectangles, {} lines", lines.len());
                }
            }
        }
    }

    /// The lengths of a dashed border's dashes and gaps are its pattern's (`dash_pattern`, in sheet points): 3 and 1 for
    /// `dashed`, 9 and 3 for `mediumDashed`, 9 3 3 3 for a dash-dot, at any size.
    #[test]
    fn the_dashes_of_a_dashed_border_are_its_patterns() {
        use ibis_doc_model::BorderLine::*;
        for (line, pattern) in [(Dashed, vec![3.0f32, 1.0]), (MediumDashed, vec![9.0, 3.0]), (DashDot, vec![9.0, 3.0, 3.0, 3.0]), (MediumDashDotDot, vec![9.0, 3.0, 3.0, 3.0, 3.0, 3.0])] {
            for scale in [1.0f32, 2.0] {
                let (_, lines) = border_marks(line, true, true, scale, 120.0);
                let want: Vec<f32> = pattern.iter().map(|l| l * scale).collect();
                assert_eq!(lines[0].pattern, want, "{line:?} at {scale}");
            }
        }
    }

    /// A printed dotted border is the Excel tile's dots whatever its length, along a row and down a column, at 100 %
    /// and 200 %: dots and gaps of 0.48 paper points, starting where the tile does on the paper.
    #[test]
    fn a_printed_dotted_border_is_the_excel_tile_at_any_size() {
        use ibis_doc_model::BorderLine::Dotted;
        for scale in [1.0f32, 2.0] {
            for horizontal in [true, false] {
                let (_, lines) = border_marks(Dotted, horizontal, true, scale, 100.0);
                let l = &lines[0];
                // The grid scale is 1 here: a sheet point is a paper point, and `scale` pixels.
                assert!(l.pattern.iter().all(|v| (v - 0.48 * scale).abs() < 1e-3 * scale), "scale {scale} horizontal {horizontal}: {:?}", l.pattern);
                assert!(l.phase >= 0.0 && l.phase < 0.96 * scale, "the phase is in the tile: {}", l.phase);
                // The line starts 10 sheet points along (a column starts a point earlier: 9), which is 0.4 (0.36) into a tile.
                let want = if horizontal { 0.4 } else { 0.36 };
                assert!((l.phase - want * scale).abs() < 1e-3 * scale, "scale {scale} horizontal {horizontal}: phase {}", l.phase);
            }
        }
    }

    /// #157: Across arbitrary zoom levels (100%, 130%, 150%, 175%, 200%, 400%) and display scale factors
    /// (1.0x, 2.0x Retina), `snap_fill_bounds` of a filled cell `[left - 1, right] x [top - 1, bottom]` and
    /// `on_pixels_bounds` of a thin solid border `[at - 1, at]` share the exact same device-pixel boundary
    /// without any 1px gap or overlap.
    #[test]
    fn fill_border_and_grid_snap_to_identical_device_pixels_across_zooms() {
        use super::{PRINT_GRID, border_quads, on_pixels_bounds, snap_fill_bounds};
        use gpui_kit::{Bounds, point, px, size};
        use ibis_doc_model::{Border, BorderLine, Rgb};

        PRINT_GRID.with(|g| g.set(None));
        let thin = Border {
            line: BorderLine::Thin,
            color: Rgb(0x000000),
        };
        let to_bounds = |(x, y, w, h): (f32, f32, f32, f32)| Bounds::new(point(px(x), px(y)), size(px(w), px(h)));

        for scale_factor in [1.0f32, 2.0] {
            for zoom in [1.0f32, 1.3, 1.5, 1.75, 2.0, 4.0] {
                let px_per_point = zoom * (96.0 / 72.0);
                // Simulate cumulative cell boundaries in screen pixels (as `layout_part` computes `x(col)` / `y(row)`).
                let mut edges = Vec::new();
                let mut acc = 36.0f32;
                for col in 0..8 {
                    edges.push((acc * px_per_point).round());
                    acc += 54.0 + (col as f32) * 3.7;
                }
                for w in edges.windows(2) {
                    let (left, right) = (w[0], w[1]);
                    let (top, bottom) = (w[0], w[1]);
                    // Filled cell quad before `snap_fill_bounds`: `(left - 1.0, top - 1.0, right - left + 1.0, bottom - top + 1.0)`.
                    let fill_b = snap_fill_bounds(
                        to_bounds((left - 1.0, top - 1.0, right - left + 1.0, bottom - top + 1.0)),
                        scale_factor,
                    );
                    let fill_left_dev = (f32::from(fill_b.origin.x) * scale_factor).round() as i32;
                    let fill_right_dev = (f32::from(fill_b.origin.x + fill_b.size.width) * scale_factor).round() as i32;
                    let fill_top_dev = (f32::from(fill_b.origin.y) * scale_factor).round() as i32;
                    let fill_bottom_dev = (f32::from(fill_b.origin.y + fill_b.size.height) * scale_factor).round() as i32;

                    // Thin horizontal border on the top edge `top` and bottom edge `bottom` (screen uses border_unit = 1.0).
                    let mut top_quads = Vec::new();
                    border_quads(&thin, true, false, top, left - 1.0, right, 1.0, &mut top_quads);
                    assert_eq!(top_quads.len(), 1);
                    let top_b = on_pixels_bounds(to_bounds(top_quads[0].rect), true, scale_factor);
                    let b_top_0 = (f32::from(top_b.origin.y) * scale_factor).round() as i32;
                    let b_top_1 = (f32::from(top_b.origin.y + top_b.size.height) * scale_factor).round() as i32;
                    let b_left_0 = (f32::from(top_b.origin.x) * scale_factor).round() as i32;
                    let b_right_1 = (f32::from(top_b.origin.x + top_b.size.width) * scale_factor).round() as i32;

                    assert_eq!(b_top_0, fill_top_dev, "top border start must match fill top at zoom={zoom}, sf={scale_factor}");
                    assert_eq!(b_top_1 - b_top_0, scale_factor as i32, "thin border must be 1 logical px thick at zoom={zoom}, sf={scale_factor}");
                    assert_eq!(b_left_0, fill_left_dev, "top border left must match fill left at zoom={zoom}, sf={scale_factor}");
                    assert_eq!(b_right_1, fill_right_dev, "top border right must match fill right at zoom={zoom}, sf={scale_factor}");

                    let mut bottom_quads = Vec::new();
                    border_quads(&thin, true, false, bottom, left - 1.0, right, 1.0, &mut bottom_quads);
                    let bottom_b = on_pixels_bounds(to_bounds(bottom_quads[0].rect), true, scale_factor);
                    let bb_1 = (f32::from(bottom_b.origin.y + bottom_b.size.height) * scale_factor).round() as i32;
                    assert_eq!(bb_1, fill_bottom_dev, "bottom border outer edge must match fill bottom at zoom={zoom}, sf={scale_factor}");
                }
            }
        }
    }
}
