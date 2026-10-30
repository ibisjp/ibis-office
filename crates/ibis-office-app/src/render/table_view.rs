// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Drawing tables: cell fills and text, then the borders on top.

use gpui_kit::*;
use ibis_doc_model::{EmuRect, Fill, Stroke, Table};

use crate::render::rich_text::TextPositions;
use crate::render::shape_view::{Resources, exact_origin, hsla, text_body_element, with_fill};

/// Draw `table` filling `frame` (frame-relative, so x and y are ignored).
pub(crate) fn table_element(table: &Table, frame: EmuRect, scale: f32, res: &Resources) -> Div {
    table_element_editing(table, frame, scale, res, None)
}

/// `table_element`, recording where the text of cell `edited.0` (row,
/// column) is painted in `edited.1` (for the caret while it is edited).
pub(crate) fn table_element_editing(
    table: &Table,
    frame: EmuRect,
    scale: f32,
    res: &Resources,
    edited: Option<((usize, usize), &TextPositions)>,
) -> Div {
    table_element_recording(table,frame,scale,res,edited,None)
}

/// Also record linked cells so pointer hit tests use the painted character geometry.
pub(crate) fn table_element_recording(
    table:&Table,frame:EmuRect,scale:f32,res:&Resources,
    edited:Option<((usize,usize),&TextPositions)>,
    recorded:Option<&std::collections::HashMap<(usize,usize),TextPositions>>,
) -> Div {
    // PowerPoint saves the rows it laid out, adding up to the frame; rows
    // that fall short of it were not laid out (in users' private
    // presentations, one table adds up, another is half its frame) and are
    // laid out here.
    let stated: i64 = table.rows.iter().map(|r| if r.height > 0 { r.height } else { table.row_text_height(r) }).sum();
    let laid_out = (stated - frame.height).abs() <= frame.height / 100 && !row_text_overflows(table);
    if !laid_out && table.rows.iter().all(|r| r.cells.iter().all(|c| c.row_span <= 1)) {
        return growing_table(table, frame, scale, res, edited,recorded);
    }
    fixed_table(table, frame, scale, res, edited,recorded)
}

/// Whether any row's explicit `height` (> 0) is shorter than the wrapped height of one of its cells' text.
fn row_text_overflows(table: &Table) -> bool {
    table.rows.iter().any(|row| row.height > 0 && table.row_text_height(row) > row.height + ibis_doc_model::EMU_PER_POINT)
}

/// Paint cell fill: supports solid, linear, path (e.g. tileRect rect gradient), and pattern fills.
/// `at` is where the cell is on the slide (pixels from its corner): a pattern's cells are anchored at the slide's corner, as PowerPoint
/// anchors them (none: at the cell's own).
fn cell_fill_element(div: Div, fill: &Fill, res: &Resources, scale: f32, at: Option<(f32, f32)>) -> Div {
    let mut div = with_fill(div, fill, res);
    if crate::render::gradient::is_gradient(fill) {
        let gradient = fill.clone();
        div = div.child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let o = bounds.origin;
                    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                    let rect = vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)];
                    crate::render::gradient::paint_gradient(window, &gradient, &[rect], (w, h), 1.0, &|pts| {
                        pts.iter().map(|&(x, y)| point(o.x + px(x), o.y + px(y))).collect()
                    });
                },
            )
            .absolute()
            .size_full(),
        );
    } else if let Fill::Pattern { .. } = fill {
        let pattern = fill.clone();
        div = div.child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let (o, s) = (bounds.origin, bounds.size);
                    let rect = vec![o, point(o.x + s.width, o.y), point(o.x + s.width, o.y + s.height), point(o.x, o.y + s.height)];
                    let anchor = at.map_or(o, |(ax, ay)| point(o.x - px(ax), o.y - px(ay)));
                    crate::render::shape_view::paint_pattern(window, &pattern, &[rect], anchor, scale);
                },
            )
            .absolute()
            .size_full(),
        );
    }
    div
}

/// A table whose rows are as high as they say, or higher where a cell's
/// text needs more, as PowerPoint lays them out (the frame is not filled:
/// measured on a table of a user's private presentation, whose rows grow
/// with their text and end
/// short of the frame). Rows are laid out by GPUI, which measures the text.
fn growing_table(table: &Table, frame: EmuRect, scale: f32, res: &Resources, edited: Option<((usize, usize), &TextPositions)>,recorded:Option<&std::collections::HashMap<(usize,usize),TextPositions>>) -> Div {
    let px_of = |e: i64| e as f32 * scale;
    let rows = table.rows.iter().enumerate().map(|(r, row)| {
        let cells = row.cells.iter().enumerate().filter_map(|(c, cell)| {
            if cell.merged {
                return None;
            }
            let positions = edited.filter(|(at, _)| *at == (r, c)).map(|(_, p)| p).or_else(||recorded?.get(&(r,c)));
            let span = cell.column_span.max(1) as usize;
            let at = c;
            let at_emu: i64 = table.columns[..at.min(table.columns.len())].iter().sum();
            let cell_origin_x = (frame.x + at_emu) as f32 * scale;
            let w: f32 = table.columns[at.min(table.columns.len())..(at + span).min(table.columns.len())].iter().map(|c| px_of(*c)).sum();
            let b = &cell.borders;
            // Each border centred on its edge, drawn over the cell without protruding outside cell corners.
            let line = |stroke: &Option<Stroke>, edge: u8| {
                let stroke = stroke.as_ref()?;
                let t = (stroke.width as f32 * scale).max(1.0);
                if stroke.dashed || stroke.compound != ibis_doc_model::LineCompound::Single {
                    let stroke = stroke.clone();
                    let c = canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            let (ox, oy) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
                            let (bw, bh) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                            // A thinnest of one pixel on a screen; none in a PDF (a window that only records), which has the border's own thickness.
                            let min_px = if window.is_recording_paints_only() { 0.0 } else { 1.0 };
                            let mut segs = Vec::new();
                            if edge < 2 {
                                render_h_line(&mut segs, 0.0, bw, bh * 0.5, &stroke, scale, min_px, edge == 1, false, false);
                            } else {
                                render_v_line(&mut segs, bw * 0.5, 0.0, bh, &stroke, scale, min_px, edge == 3, false, false);
                            }
                            for seg in &segs {
                                let r = Bounds::new(
                                    point(px(ox + seg.bounds[0]), px(oy + seg.bounds[1])),
                                    size(px(seg.bounds[2]), px(seg.bounds[3])),
                                );
                                window.paint_quad(gpui_kit::fill(r, seg.color));
                            }
                        },
                    )
                    .size_full();
                    let d = div().absolute().child(c);
                    return Some(match edge {
                        0 => d.top(px(-t / 2.0)).left_0().right_0().h(px(t)),
                        1 => d.bottom(px(-t / 2.0)).left_0().right_0().h(px(t)),
                        2 => d.left(px(-t / 2.0)).top_0().bottom_0().w(px(t)),
                        _ => d.right(px(-t / 2.0)).top_0().bottom_0().w(px(t)),
                    });
                }
                // The box keeps the thinnest of one pixel for the layout; the line is painted at its own thickness in a PDF (a window that only records).
                let (raw, color) = (stroke.width as f32 * scale, hsla(stroke.color));
                let c = canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let (x, y) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
                        let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                        let thick = if window.is_recording_paints_only() { raw } else { t };
                        let r = if edge < 2 {
                            Bounds::new(point(px(x), px(y + (h - thick) / 2.0)), size(px(w), px(thick)))
                        } else {
                            Bounds::new(point(px(x + (w - thick) / 2.0), px(y)), size(px(thick), px(h)))
                        };
                        window.paint_quad(gpui_kit::fill(r, color));
                    },
                )
                .size_full();
                let d = div().absolute().child(c);
                Some(match edge {
                    0 => d.top(px(-t / 2.0)).left_0().right_0().h(px(t)),
                    1 => d.bottom(px(-t / 2.0)).left_0().right_0().h(px(t)),
                    2 => d.left(px(-t / 2.0)).top_0().bottom_0().w(px(t)),
                    _ => d.right(px(-t / 2.0)).top_0().bottom_0().w(px(t)),
                })
            };
            let diag_canvas = if b.tl_to_br.is_some() || b.bl_to_tr.is_some() {
                let tl_br = b.tl_to_br.clone();
                let bl_tr = b.bl_to_tr.clone();
                Some(canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                        let o = bounds.origin;
                        let min_px = if window.is_recording_paints_only() { 0.0 } else { 1.0 };
                        if let Some(s) = &tl_br {
                            let width = px((s.width as f32 * scale).max(min_px));
                            if let Some(p) = crate::render::shape_view::polygon_styled(&[o, point(o.x + px(w), o.y + px(h))], Some(width), false, s.cap, s.join) {
                                window.paint_path(p, hsla(s.color));
                            }
                        }
                        if let Some(s) = &bl_tr {
                            let width = px((s.width as f32 * scale).max(min_px));
                            if let Some(p) = crate::render::shape_view::polygon_styled(&[point(o.x, o.y + px(h)), point(o.x + px(w), o.y)], Some(width), false, s.cap, s.join) {
                                window.paint_path(p, hsla(s.color));
                            }
                        }
                    },
                ).absolute().size_full())
            } else {
                None
            };
            Some(
                cell_fill_element(div().relative().flex_none().w(px(w)).flex().flex_col(), &cell.fill, res, scale, None)
                    .child(text_body_element(&cell.text, Some((w / scale) as f64), None, scale, res, positions, None, Some(cell_origin_x)))
                    .children([line(&b.top, 0), line(&b.bottom, 1), line(&b.left, 2), line(&b.right, 3)].into_iter().flatten())
                    .children(diag_canvas),
            )
        });
        div().flex().flex_row().items_stretch().min_h(px(px_of(row.height))).children(cells)
    });
    div().absolute().left_0().top_0().flex().flex_col().children(rows)
}

fn merge_stroke(existing: &mut Option<Stroke>, new_s: &Option<Stroke>) {
    let Some(new_s) = new_s else { return };
    match existing {
        None => *existing = Some(new_s.clone()),
        Some(cur) => {
            if new_s.width > cur.width {
                *existing = Some(new_s.clone());
            }
        }
    }
}

fn fixed_table(table: &Table, frame: EmuRect, scale: f32, res: &Resources, edited: Option<((usize, usize), &TextPositions)>,recorded:Option<&std::collections::HashMap<(usize,usize),TextPositions>>) -> Div {
    let heights = table.row_heights(frame.height);
    let offsets = |sizes: &[i64]| -> Vec<i64> {
        let mut at = 0;
        let mut out = vec![0];
        for s in sizes {
            at += s;
            out.push(at);
        }
        out
    };
    let (xs_emu, ys_emu) = (offsets(&table.columns), offsets(&heights));
    let num_rows = table.rows.len();
    let num_cols = table.columns.len();
    let edge_x = |c: usize| xs_emu[c.min(xs_emu.len() - 1)] as f32 * scale;
    let edge_y = |r: usize| ys_emu[r.min(ys_emu.len() - 1)] as f32 * scale;

    let mut cells = Vec::new();
    let mut diagonals: Vec<([f32; 2], [f32; 2], Stroke)> = Vec::new();

    // H lines: (num_rows + 1) boundaries, each with num_cols segments
    let mut h_edges: Vec<Vec<Option<Stroke>>> = vec![vec![None; num_cols]; num_rows + 1];
    // V lines: (num_cols + 1) boundaries, each with num_rows segments
    let mut v_edges: Vec<Vec<Option<Stroke>>> = vec![vec![None; num_rows]; num_cols + 1];

    for (r, row) in table.rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            if cell.merged {
                continue;
            }
            let c_span = (cell.column_span.max(1) as usize).min(num_cols.saturating_sub(c));
            let r_span = (cell.row_span.max(1) as usize).min(num_rows.saturating_sub(r));
            let (left, right) = (edge_x(c), edge_x(c + c_span));
            let (top, bottom) = (edge_y(r), edge_y(r + r_span));
            let (w, h) = (right - left, bottom - top);
            let positions = edited.filter(|(at, _)| *at == (r, c)).map(|(_, p)| p).or_else(||recorded?.get(&(r,c)));
            let cell_origin_x = (frame.x + xs_emu[c]) as f32 * scale;
            // A fill reaches one pixel into the next column and row (painted later, so the neighbour's own fill covers it):
            // the device-pixel snapping of the boxes otherwise leaves a hairline of the background between two fills.
            let slide_at = Some((frame.x as f32 * scale + left, frame.y as f32 * scale + top));
            let (ow, oh) = (if c + c_span < num_cols { 1.0 } else { 0.0 }, if r + r_span < num_rows { 1.0 } else { 0.0 });
            if !matches!(&cell.fill, Fill::None) && !matches!(&cell.fill, Fill::Solid(c) if c.alpha < 255) && (ow > 0.0 || oh > 0.0) {
                cells.push(cell_fill_element(div().absolute().left(px(left)).top(px(top)).w(px(w + ow)).h(px(h + oh)), &cell.fill, res, scale, slide_at));
                cells.push(div().absolute().left(px(left)).top(px(top)).w(px(w)).h(px(h)).child(text_body_element(&cell.text, Some((w / scale) as f64), None, scale, res, positions, None, Some(cell_origin_x))));
            } else {
                cells.push(
                    cell_fill_element(div().absolute().left(px(left)).top(px(top)).w(px(w)).h(px(h)), &cell.fill, res, scale, slide_at)
                        .child(text_body_element(&cell.text, Some((w / scale) as f64), None, scale, res, positions, None, Some(cell_origin_x))),
                );
            }

            let b = &cell.borders;
            if let Some(s) = &b.tl_to_br {
                diagonals.push(([left, top], [right, bottom], s.clone()));
            }
            if let Some(s) = &b.bl_to_tr {
                diagonals.push(([left, bottom], [right, top], s.clone()));
            }
            for col_idx in c..(c + c_span).min(num_cols) {
                merge_stroke(&mut h_edges[r][col_idx], &b.top);
                merge_stroke(&mut h_edges[r + r_span][col_idx], &b.bottom);
            }
            for row_idx in r..(r + r_span).min(num_rows) {
                merge_stroke(&mut v_edges[c][row_idx], &b.left);
                merge_stroke(&mut v_edges[c + c_span][row_idx], &b.right);
            }
        }
    }


    // The border segments for a thinnest border of `min_px` pixels: one on a screen, so a thin border does not vanish; none in a PDF (a window that only
    // records), which has the border's own thickness (PowerPoint's PDF: 0.5 pt, where one pixel of a window a point to a pixel made 1 pt).
    let build_segments = |min_px: f32| -> Vec<BorderSegment> {
    let mut border_segments = Vec::new();

    // Render horizontal segments (run-length merged where adjacent segments share stroke)
    for (r, row_edges) in h_edges.iter().enumerate() {
        let at_y = edge_y(r);
        let is_outer_top = r == 0;
        let is_outer_bottom = r == num_rows;
        let is_outer_h = is_outer_top || is_outer_bottom;

        let mut c_start = 0;
        while c_start < num_cols {
            let Some(stroke) = &row_edges[c_start] else {
                c_start += 1;
                continue;
            };
            let stroke = stroke.clone();
            let mut c_end = c_start + 1;
            while c_end < num_cols && row_edges[c_end].as_ref().is_some_and(|s| stroke_eq(s, &stroke)) {
                c_end += 1;
            }

            let from_x = edge_x(c_start);
            let to_x = edge_x(c_end);
            let is_outer_left = is_outer_h && c_start == 0;
            let is_outer_right = is_outer_h && c_end == num_cols;

            render_h_line(
                &mut border_segments,
                from_x,
                to_x,
                at_y,
                &stroke,
                scale,
                min_px,
                is_outer_bottom,
                is_outer_left,
                is_outer_right,
            );

            c_start = c_end;
        }
    }

    // Render vertical segments (run-length merged)
    for (c, col_edges) in v_edges.iter().enumerate() {
        let at_x = edge_x(c);
        let is_outer_left = c == 0;
        let is_outer_right = c == num_cols;
        let is_outer_v = is_outer_left || is_outer_right;

        let mut r_start = 0;
        while r_start < num_rows {
            let Some(stroke) = &col_edges[r_start] else {
                r_start += 1;
                continue;
            };
            let stroke = stroke.clone();
            let mut r_end = r_start + 1;
            while r_end < num_rows && col_edges[r_end].as_ref().is_some_and(|s| stroke_eq(s, &stroke)) {
                r_end += 1;
            }

            let from_y = edge_y(r_start);
            let to_y = edge_y(r_end);
            let is_outer_top = is_outer_v && r_start == 0;
            let is_outer_bottom = is_outer_v && r_end == num_rows;

            render_v_line(
                &mut border_segments,
                at_x,
                from_y,
                to_y,
                &stroke,
                scale,
                min_px,
                is_outer_right,
                is_outer_top,
                is_outer_bottom,
            );

            r_start = r_end;
        }
    }

    border_segments
    };
    let border_segments = build_segments(1.0);
    let vector_segments = build_segments(0.0);

    let at = (frame.x as f32 * scale, frame.y as f32 * scale);
    let border_canvas = canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let sf = window.scale_factor();
            let exact = exact_origin(bounds.origin, at, sf);
            let ox = f32::from(exact.x);
            let oy = f32::from(exact.y);
            let recording = window.is_recording_paints_only();
            for seg in if recording { &vector_segments } else { &border_segments } {
                let r = Bounds::new(
                    point(px(ox + seg.bounds[0]), px(oy + seg.bounds[1])),
                    size(px(seg.bounds[2]), px(seg.bounds[3])),
                );
                window.paint_quad(gpui_kit::fill(r, seg.color));
            }
            for (p0, p1, stroke) in &diagonals {
                let start = point(px(ox + p0[0]), px(oy + p0[1]));
                let end = point(px(ox + p1[0]), px(oy + p1[1]));
                let width = px((stroke.width as f32 * scale).max(if recording { 0.0 } else { 1.0 }));
                if let Some(p) = crate::render::shape_view::polygon_styled(&[start, end], Some(width), false, stroke.cap, stroke.join) {
                    window.paint_path(p, hsla(stroke.color));
                }
            }
        },
    )
    .absolute()
    .size_full();

    div().absolute().left_0().top_0().size_full().children(cells).child(border_canvas)
}

// Now render collapsed border segments via canvas paint_quad for subpixel precision
struct BorderSegment {
    bounds: [f32; 4],
    color: Hsla,
}

fn stroke_eq(a: &Stroke, b: &Stroke) -> bool {
    a.width == b.width
        && a.color == b.color
        && a.compound == b.compound
        && a.dashed == b.dashed
        && a.dash == b.dash
        && a.cap == b.cap
}

fn render_h_line(
    segments: &mut Vec<BorderSegment>,
    from_x: f32,
    to_x: f32,
    at_y: f32,
    stroke: &Stroke,
    scale: f32,
    min_px: f32,
    is_bottom: bool,
    is_outer_left: bool,
    is_outer_right: bool,
) {
    let t = (stroke.width as f32 * scale).max(min_px);
    let color = hsla(stroke.color);
    for &(mut a, mut b) in stroke.compound.bands() {
        if is_bottom {
            (a, b) = (1.0 - b, 1.0 - a);
        }
        let band_t = ((b - a) * t).max(min_px * 0.5);
        let offset = ((a + b) / 2.0 - 0.5) * t;
        let band_top = at_y + offset - band_t / 2.0;

        let mut b_from_x = from_x;
        let mut b_to_x = to_x;
        if is_outer_left {
            let is_outer_band = if is_bottom { offset > 0.0 } else { offset < 0.0 };
            b_from_x = if stroke.compound == ibis_doc_model::LineCompound::Single || is_outer_band {
                from_x - t / 2.0
            } else {
                from_x + (t / 2.0 - band_t)
            };
        }
        if is_outer_right {
            let is_outer_band = if is_bottom { offset > 0.0 } else { offset < 0.0 };
            b_to_x = if stroke.compound == ibis_doc_model::LineCompound::Single || is_outer_band {
                to_x + t / 2.0
            } else {
                to_x - (t / 2.0 - band_t)
            };
        }

        if stroke.dashed {
            let pattern = match stroke.dash.pattern() {
                [] => &[4.0, 3.0][..],
                p => p,
            };
            let mut pos = 0.0f32;
            let mut pat_idx = 0usize;
            let mut is_dash = true;
            let len = b_to_x - b_from_x;
            let unit = t;
            let cap_extra = if stroke.cap == ibis_doc_model::LineCap::Square { band_t / 2.0 } else { 0.0 };
            while pos < len {
                let dash_len = pattern[pat_idx] * unit;
                let next_pos = (pos + dash_len).min(len);
                if is_dash && next_pos > pos {
                    let seg_from = (b_from_x + pos - cap_extra).max(b_from_x);
                    let seg_to = (b_from_x + next_pos + cap_extra).min(b_to_x);
                    segments.push(BorderSegment {
                        bounds: [seg_from, band_top, seg_to - seg_from, band_t],
                        color,
                    });
                }
                pos = next_pos;
                is_dash = !is_dash;
                pat_idx = (pat_idx + 1) % pattern.len();
            }
        } else {
            segments.push(BorderSegment {
                bounds: [b_from_x, band_top, b_to_x - b_from_x, band_t],
                color,
            });
        }
    }
}

fn render_v_line(
    segments: &mut Vec<BorderSegment>,
    at_x: f32,
    from_y: f32,
    to_y: f32,
    stroke: &Stroke,
    scale: f32,
    min_px: f32,
    is_right: bool,
    is_outer_top: bool,
    is_outer_bottom: bool,
) {
    let t = (stroke.width as f32 * scale).max(min_px);
    let color = hsla(stroke.color);
    for &(mut a, mut b) in stroke.compound.bands() {
        if is_right {
            (a, b) = (1.0 - b, 1.0 - a);
        }
        let band_t = ((b - a) * t).max(min_px * 0.5);
        let offset = ((a + b) / 2.0 - 0.5) * t;
        let band_left = at_x + offset - band_t / 2.0;

        let mut b_from_y = from_y;
        let mut b_to_y = to_y;
        if is_outer_top {
            let is_outer_band = if is_right { offset > 0.0 } else { offset < 0.0 };
            b_from_y = if stroke.compound == ibis_doc_model::LineCompound::Single || is_outer_band {
                from_y - t / 2.0
            } else {
                from_y + (t / 2.0 - band_t)
            };
        }
        if is_outer_bottom {
            let is_outer_band = if is_right { offset > 0.0 } else { offset < 0.0 };
            b_to_y = if stroke.compound == ibis_doc_model::LineCompound::Single || is_outer_band {
                to_y + t / 2.0
            } else {
                to_y - (t / 2.0 - band_t)
            };
        }

        if stroke.dashed {
            let pattern = match stroke.dash.pattern() {
                [] => &[4.0, 3.0][..],
                p => p,
            };
            let mut pos = 0.0f32;
            let mut pat_idx = 0usize;
            let mut is_dash = true;
            let len = b_to_y - b_from_y;
            let unit = t;
            let cap_extra = if stroke.cap == ibis_doc_model::LineCap::Square { band_t / 2.0 } else { 0.0 };
            while pos < len {
                let dash_len = pattern[pat_idx] * unit;
                let next_pos = (pos + dash_len).min(len);
                if is_dash && next_pos > pos {
                    let seg_from = (b_from_y + pos - cap_extra).max(b_from_y);
                    let seg_to = (b_from_y + next_pos + cap_extra).min(b_to_y);
                    segments.push(BorderSegment {
                        bounds: [band_left, seg_from, band_t, seg_to - seg_from],
                        color,
                    });
                }
                pos = next_pos;
                is_dash = !is_dash;
                pat_idx = (pat_idx + 1) % pattern.len();
            }
        } else {
            segments.push(BorderSegment {
                bounds: [band_left, b_from_y, band_t, b_to_y - b_from_y],
                color,
            });
        }
    }
}
