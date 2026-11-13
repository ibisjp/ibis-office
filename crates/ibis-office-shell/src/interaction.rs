// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The geometry of working on a slide with the pointer, in slide EMU:
//! which shape a press hits (rotated shapes and thin lines too), where the
//! handles of a selected shape are, how a handle drag resizes a rotated
//! shape, which shapes a rubber band takes, and the frame of a shape or line
//! being placed. No UI here, so it can be tested on its own; `slide_view.rs`
//! turns pointer events into these.

use ibis_doc_model::{EmuPoint, EmuRect, ResizeHandle, Shape, ShapeId, ShapeKind, Slide};

/// The smallest side a shape can be resized to, in EMU (0.1 in).
pub(crate) const MIN_SIDE: i64 = 91_440;
/// A press this close to a line hits it, in pixels.
pub const LINE_SLOP_PX: f32 = 5.0;
/// The rotation handle is this far above the top edge, in pixels.
pub const ROTATE_HANDLE_PX: f32 = 22.0;
/// A handle is hit within this distance of its centre, in pixels.
pub const HANDLE_HIT_PX: f32 = 6.0;

/// A point in slide EMU as floats.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct P {
    pub x: f64,
    pub y: f64,
}

impl P {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn of(p: EmuPoint) -> Self {
        Self::new(p.x as f64, p.y as f64)
    }
}

/// Radians of a rotation in 60 000ths of a degree.
fn radians(rotation: i32) -> f64 {
    (rotation as f64 / 60_000.0).to_radians()
}

fn center(frame: EmuRect) -> P {
    P::new(frame.x as f64 + frame.width as f64 / 2.0, frame.y as f64 + frame.height as f64 / 2.0)
}

/// `p` turned by `rotation` (clockwise, on screen) about `c`.
pub(crate) fn rotate(p: P, c: P, rotation: i32) -> P {
    let (s, k) = radians(rotation).sin_cos();
    let (dx, dy) = (p.x - c.x, p.y - c.y);
    P::new(c.x + dx * k - dy * s, c.y + dx * s + dy * k)
}

/// A point of a shape's frame (given on the unrotated frame) where it is
/// drawn, with the shape's rotation.
pub(crate) fn on_slide(shape_frame: EmuRect, rotation: i32, p: P) -> P {
    rotate(p, center(shape_frame), rotation)
}

/// The corners of a shape as drawn (rotated), clockwise from the top left.
pub fn corners(frame: EmuRect, rotation: i32) -> [P; 4] {
    let (l, t, r, b) = (frame.x as f64, frame.y as f64, frame.right() as f64, frame.bottom() as f64);
    [P::new(l, t), P::new(r, t), P::new(r, b), P::new(l, b)].map(|p| on_slide(frame, rotation, p))
}

/// The box a shape takes on the slide, rotation included.
pub fn bounds(shape: &Shape) -> EmuRect {
    let pts = corners(shape.frame, shape.rotation);
    let (mut l, mut t, mut r, mut b) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in pts {
        l = l.min(p.x);
        t = t.min(p.y);
        r = r.max(p.x);
        b = b.max(p.y);
    }
    EmuRect::new(l.round() as i64, t.round() as i64, (r - l).round() as i64, (b - t).round() as i64)
}

/// Whether `p` hits `shape`; `slop` is how near (EMU) counts for a line.
/// The cell (row, column) of table `shape` at slide point `p`: the cell
/// that covers it (a merged cell's origin), with the rows as tall as they
/// fill the frame (`Table::row_heights`).
pub fn table_cell_at(shape: &Shape, p: P) -> Option<(usize, usize)> {
    let ShapeKind::Table(table) = &shape.kind else { return None };
    let f = shape.frame;
    let local = rotate(p, center(f), -shape.rotation);
    let (x, y) = (local.x - f.x as f64, local.y - f.y as f64);
    let find = |sizes: &[i64], at: f64| -> Option<usize> {
        let mut edge = 0.0;
        for (i, s) in sizes.iter().enumerate() {
            edge += *s as f64;
            if at < edge {
                return Some(i);
            }
        }
        None
    };
    let column = find(&table.columns, x)?;
    let row = find(&table.row_heights(f.height), y)?;
    // A cell covered by a neighbour's span: that neighbour.
    for r in (0..=row).rev() {
        for c in (0..=column).rev() {
            let Some(cell) = table.rows.get(r).and_then(|row| row.cells.get(c)) else { continue };
            if !cell.merged && r + cell.row_span.max(1) as usize > row && c + cell.column_span.max(1) as usize > column {
                return Some((r, c));
            }
        }
    }
    Some((row, column))
}

pub fn hits(shape: &Shape, p: P, slop: f64) -> bool {
    let f = shape.frame;
    // Into the shape's own (unrotated) frame.
    let local = rotate(p, center(f), -shape.rotation);
    if let ShapeKind::Line(_) = shape.kind {
        let (l, t, r, b) = (f.x as f64, f.y as f64, f.right() as f64, f.bottom() as f64);
        // The line runs corner to corner; flips pick the other diagonal.
        let (a, z) = match (shape.flip_h, shape.flip_v) {
            (false, false) | (true, true) => (P::new(l, t), P::new(r, b)),
            _ => (P::new(r, t), P::new(l, b)),
        };
        return distance_to_segment(local, a, z) <= slop;
    }
    let (l, t, r, b) = (f.x as f64, f.y as f64, f.right() as f64, f.bottom() as f64);
    local.x >= l && local.x <= r && local.y >= t && local.y <= b
}

fn distance_to_segment(p: P, a: P, b: P) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) };
    let (cx, cy) = (a.x + t * dx, a.y + t * dy);
    ((p.x - cx).powi(2) + (p.y - cy).powi(2)).sqrt()
}

/// The topmost shape of `slide` that `p` hits (shapes attached to another
/// one are part of it and are not hit on their own).
pub fn hit_test(slide: &Slide, p: P, line_slop: f64) -> Option<ShapeId> {
    slide.shown_shapes().collect::<Vec<_>>().into_iter().rev().filter(|s| !slide.is_attached(s.id)).find(|s| hits(s, p, line_slop)).map(|s| s.id)
}

/// The shapes a rubber band `band` takes: those wholly inside it (as in
/// PowerPoint), attached ones left out.
pub fn in_band(slide: &Slide, band: EmuRect) -> Vec<ShapeId> {
    slide
        .shown_shapes()
        .filter(|s| !slide.is_attached(s.id))
        .filter(|s| {
            let b = bounds(s);
            b.x >= band.x && b.y >= band.y && b.right() <= band.right() && b.bottom() <= band.bottom()
        })
        .map(|s| s.id)
        .collect()
}

/// Where handle `h` of a shape is drawn.
pub fn handle_point(frame: EmuRect, rotation: i32, h: ResizeHandle) -> P {
    let (ax, ay) = h.anchor();
    let p = P::new(frame.x as f64 + frame.width as f64 * ax as f64 / 2.0, frame.y as f64 + frame.height as f64 * ay as f64 / 2.0);
    on_slide(frame, rotation, p)
}

/// Where the rotation handle is drawn, `above` EMU over the top edge.
pub fn rotation_handle_point(frame: EmuRect, rotation: i32, above: f64) -> P {
    let p = P::new(frame.x as f64 + frame.width as f64 / 2.0, frame.y as f64 - above);
    on_slide(frame, rotation, p)
}

/// The handle opposite `h`, which stays put while `h` is dragged.
fn opposite(h: ResizeHandle) -> ResizeHandle {
    use ResizeHandle::*;
    match h {
        TopLeft => BottomRight,
        Top => Bottom,
        TopRight => BottomLeft,
        Right => Left,
        BottomRight => TopLeft,
        Bottom => Top,
        BottomLeft => TopRight,
        Left => Right,
    }
}

/// The frame of a shape (`origin`, turned by `rotation`) whose handle `h`
/// is dragged by `(dx, dy)` on the slide. The opposite handle stays where
/// it is on the slide; `keep_aspect` (Shift, on a corner) keeps the shape's
/// proportions.
pub fn resize(origin: EmuRect, rotation: i32, h: ResizeHandle, dx: f64, dy: f64, keep_aspect: bool) -> EmuRect {
    // The drag in the shape's own axes.
    let d = rotate(P::new(dx, dy), P::new(0.0, 0.0), -rotation);
    let resized = origin.resized_with_aspect(h, d.x.round() as i64, d.y.round() as i64, MIN_SIDE, keep_aspect);
    if rotation == 0 {
        return resized;
    }
    // Put the opposite handle back where it was on the slide.
    let fixed = opposite(h);
    let before = handle_point(origin, rotation, fixed);
    let after = handle_point(resized, rotation, fixed);
    resized.translated((before.x - after.x).round() as i64, (before.y - after.y).round() as i64)
}

/// The rotation (60 000ths of a degree, 0..21 600 000) of a shape turned
/// from `origin` by dragging its rotation handle from `start` to `now`
/// about `c`, in whole degrees; `snap` (Shift) keeps it to multiples of 15°.
pub fn rotated(origin: i32, c: P, start: P, now: P, snap: bool) -> i32 {
    let angle = |p: P| (p.y - c.y).atan2(p.x - c.x).to_degrees();
    let mut degrees = origin as f64 / 60_000.0 + angle(now) - angle(start);
    // Whole degrees, as PowerPoint's handle turns; Shift, 15° steps.
    degrees = if snap { (degrees / 15.0).round() * 15.0 } else { degrees.round() };
    ((degrees * 60_000.0).round() as i64).rem_euclid(21_600_000) as i32
}

/// A line drawn from `a` to `b`: its frame and flips (a line runs from the
/// frame's top left to its bottom right unless flipped). With `constrain`
/// (Shift) it is kept horizontal, vertical or at 45°.
pub fn line_frame(a: EmuPoint, b: EmuPoint, constrain: bool) -> (EmuRect, bool, bool) {
    let (mut dx, mut dy) = ((b.x - a.x) as f64, (b.y - a.y) as f64);
    if constrain {
        let len = (dx * dx + dy * dy).sqrt();
        let angle = (dy.atan2(dx).to_degrees() / 45.0).round() * 45.0;
        dx = len * angle.to_radians().cos();
        dy = len * angle.to_radians().sin();
        if dx.abs() < 1.0 {
            dx = 0.0;
        }
        if dy.abs() < 1.0 {
            dy = 0.0;
        }
    }
    let end = EmuPoint::new(a.x + dx.round() as i64, a.y + dy.round() as i64);
    (EmuRect::from_corners(a, end), end.x < a.x, end.y < a.y)
}

/// The frame of a shape drawn from `a` to `b`; with `square` (Shift) as
/// wide as it is tall.
pub fn drawn_frame(a: EmuPoint, b: EmuPoint, square: bool) -> EmuRect {
    let (mut dx, mut dy) = (b.x - a.x, b.y - a.y);
    if square {
        let side = dx.abs().max(dy.abs());
        dx = if dx < 0 { -side } else { side };
        dy = if dy < 0 { -side } else { side };
    }
    let r = EmuRect::from_corners(a, EmuPoint::new(a.x + dx, a.y + dy));
    EmuRect { width: r.width.max(MIN_SIDE), height: r.height.max(MIN_SIDE), ..r }
}

/// The default grid spacing in slide EMU (0.5 cm = 180 000 EMU).
pub const GRID_INTERVAL: i64 = 180_000;

/// Snap an EMU coordinate to the nearest grid line.
pub fn snap_coord(coord: i64, interval: i64) -> i64 {
    if interval <= 0 {
        return coord;
    }
    ((coord as f64 / interval as f64).round() as i64) * interval
}

/// Snap an EMU point to the nearest grid intersection.
pub fn snap_point(p: EmuPoint, interval: i64) -> EmuPoint {
    EmuPoint::new(snap_coord(p.x, interval), snap_coord(p.y, interval))
}

/// Snap an offset (dx, dy) such that `origin + delta` lands on the nearest grid coordinate.
pub fn snap_delta_to_grid(origin: EmuPoint, dx: i64, dy: i64, interval: i64) -> (i64, i64) {
    let target_x = snap_coord(origin.x + dx, interval);
    let target_y = snap_coord(origin.y + dy, interval);
    (target_x - origin.x, target_y - origin.y)
}

/// An alignment guide line displayed when shapes snap to edges/centers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AlignmentGuide {
    Vertical { x: i64, y1: i64, y2: i64 },
    Horizontal { y: i64, x1: i64, x2: i64 },
}

/// Snap a moving shape rectangle to other shapes or slide center.
/// Returns `(snap_dx, snap_dy, active_guides)`.
pub fn snap_to_shapes(
    rect: EmuRect,
    others: &[EmuRect],
    slide_width: i64,
    slide_height: i64,
    slop_emu: i64,
) -> (i64, i64, Vec<AlignmentGuide>) {
    snap_to_targets(rect, others, slide_width, slide_height, slop_emu, true, &[], &[], slop_emu)
}

/// `snap_to_shapes` with the guides' positions (`guide_xs` of the vertical, `guide_ys` of the horizontal
/// ones) also to snap to, and the slide's middle only when `center`.
pub fn snap_to_targets(
    rect: EmuRect,
    others: &[EmuRect],
    slide_width: i64,
    slide_height: i64,
    slop_emu: i64,
    center: bool,
    guide_xs: &[i64],
    guide_ys: &[i64],
    guide_slop_emu: i64,
) -> (i64, i64, Vec<AlignmentGuide>) {
    let mut snap_dx = 0;
    let mut snap_dy = 0;
    let mut min_dist_x = slop_emu + 1;
    let mut min_dist_y = slop_emu + 1;
    let mut guides = Vec::new();

    let my_xs = [rect.x, rect.x + rect.width / 2, rect.right()];
    let my_ys = [rect.y, rect.y + rect.height / 2, rect.bottom()];

    // (position, reach): the guides are taken from closer by (`guide_slop_emu`) than the shapes and the middle.
    let mut target_xs: Vec<(i64, i64)> = if center { vec![(slide_width / 2, slop_emu)] } else { Vec::new() };
    let mut target_ys: Vec<(i64, i64)> = if center { vec![(slide_height / 2, slop_emu)] } else { Vec::new() };
    target_xs.extend(guide_xs.iter().map(|g| (*g, guide_slop_emu)));
    target_ys.extend(guide_ys.iter().map(|g| (*g, guide_slop_emu)));
    for other in others {
        target_xs.extend([other.x, other.x + other.width / 2, other.right()].map(|v| (v, slop_emu)));
        target_ys.extend([other.y, other.y + other.height / 2, other.bottom()].map(|v| (v, slop_emu)));
    }

    let mut best_x_guide = None;
    for &mx in &my_xs {
        for &(tx, reach) in &target_xs {
            let diff = tx - mx;
            if diff.abs() <= reach && diff.abs() < min_dist_x {
                min_dist_x = diff.abs();
                snap_dx = diff;
                best_x_guide = Some(tx);
            }
        }
    }

    let mut best_y_guide = None;
    for &my in &my_ys {
        for &(ty, reach) in &target_ys {
            let diff = ty - my;
            if diff.abs() <= reach && diff.abs() < min_dist_y {
                min_dist_y = diff.abs();
                snap_dy = diff;
                best_y_guide = Some(ty);
            }
        }
    }

    if let Some(gx) = best_x_guide {
        guides.push(AlignmentGuide::Vertical { x: gx, y1: 0, y2: slide_height });
    }
    if let Some(gy) = best_y_guide {
        guides.push(AlignmentGuide::Horizontal { y: gy, x1: 0, x2: slide_width });
    }

    (snap_dx, snap_dy, guides)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_doc_model::{Color, Stroke};

    fn rect(id: u64, frame: EmuRect) -> Shape {
        Shape::text_box(ShapeId(id), frame, "")
    }

    #[test]
    fn rotated_shape_hits_where_it_is_drawn() {
        // A wide bar turned 90° stands upright about its centre.
        let mut s = rect(1, EmuRect::new(0, 400, 1000, 200));
        s.rotation = 5_400_000;
        assert!(hits(&s, P::new(500.0, 50.0), 0.0));
        assert!(!hits(&s, P::new(50.0, 500.0), 0.0));
        assert_eq!(bounds(&s), EmuRect::new(400, 0, 200, 1000));
    }

    #[test]
    fn a_line_is_hit_near_its_stroke_only() {
        let line = Shape::new(ShapeId(1), EmuRect::new(0, 0, 1000, 1000), ShapeKind::Line(Stroke::solid(Color::BLACK, 1)));
        assert!(hits(&line, P::new(500.0, 510.0), 20.0));
        assert!(!hits(&line, P::new(900.0, 100.0), 20.0));
        let mut flipped = line.clone();
        flipped.flip_h = true;
        assert!(hits(&flipped, P::new(900.0, 100.0), 20.0));
    }

    #[test]
    fn rubber_band_takes_shapes_wholly_inside() {
        let mut slide = Slide::new(ibis_doc_model::SlideId(1));
        slide.shapes.push(rect(1, EmuRect::new(10, 10, 100, 100)));
        slide.shapes.push(rect(2, EmuRect::new(90, 90, 100, 100)));
        assert_eq!(in_band(&slide, EmuRect::new(0, 0, 150, 150)), vec![ShapeId(1)]);
        assert_eq!(in_band(&slide, EmuRect::new(0, 0, 300, 300)), vec![ShapeId(1), ShapeId(2)]);
    }

    #[test]
    fn resizing_a_rotated_shape_keeps_the_opposite_corner() {
        let origin = EmuRect::new(1000, 1000, 400_000, 200_000);
        let rotation = 1_800_000; // 30°
        let fixed = handle_point(origin, rotation, ResizeHandle::TopLeft);
        let out = resize(origin, rotation, ResizeHandle::BottomRight, 50_000.0, 80_000.0, false);
        let after = handle_point(out, rotation, ResizeHandle::TopLeft);
        assert!((fixed.x - after.x).abs() <= 1.0 && (fixed.y - after.y).abs() <= 1.0, "{fixed:?} {after:?}");
        assert!(out.width > origin.width && out.height > origin.height);
    }

    #[test]
    fn shift_keeps_proportions() {
        let origin = EmuRect::new(0, 0, 200_000, 100_000);
        let out = resize(origin, 0, ResizeHandle::BottomRight, 200_000.0, 10_000.0, true);
        assert_eq!((out.width, out.height), (400_000, 200_000));
    }

    #[test]
    fn rotation_snaps_to_15_degrees() {
        let c = P::new(0.0, 0.0);
        let r = rotated(0, c, P::new(0.0, -100.0), P::new(100.0, -90.0), true);
        assert_eq!(r, 45 * 60_000); // about 48°, snapped to 45°
        assert_eq!(rotated(0, c, P::new(0.0, -100.0), P::new(-100.0, 0.0), false), 21_600_000 - 5_400_000);
    }

    #[test]
    fn lines_flip_by_direction() {
        let (f, h, v) = line_frame(EmuPoint::new(100, 100), EmuPoint::new(0, 200), false);
        assert_eq!((f, h, v), (EmuRect::new(0, 100, 100, 100), true, false));
        let (f, _, _) = line_frame(EmuPoint::new(0, 0), EmuPoint::new(100, 8), true);
        assert_eq!(f.height, 0);
    }

    #[test]
    fn grid_snapping_rounds_to_interval() {
        assert_eq!(snap_coord(100_000, 180_000), 180_000);
        assert_eq!(snap_coord(80_000, 180_000), 0);
        assert_eq!(snap_coord(350_000, 180_000), 360_000);

        let origin = EmuPoint::new(10_000, 20_000);
        let (dx, dy) = snap_delta_to_grid(origin, 150_000, 170_000, 180_000);
        assert_eq!(origin.x + dx, 180_000);
        assert_eq!(origin.y + dy, 180_000);
    }

    #[test]
    fn shape_snapping_finds_edge_and_center_alignment() {
        let slide_w = 10_000_000;
        let slide_h = 7_500_000;
        let slop = 50_000;

        // Other shape at (2_000_000, 1_000_000, 1_000_000, 1_000_000)
        let other = EmuRect::new(2_000_000, 1_000_000, 1_000_000, 1_000_000);
        // Moving shape near other's left edge (1_980_000 -> 2_000_000)
        let moving = EmuRect::new(1_980_000, 3_000_000, 1_000_000, 1_000_000);
        let (dx, dy, guides) = snap_to_shapes(moving, &[other], slide_w, slide_h, slop);
        assert_eq!(dx, 20_000);
        assert_eq!(dy, 0);
        assert_eq!(guides.len(), 1);
        assert!(matches!(guides[0], AlignmentGuide::Vertical { x: 2_000_000, .. }));

        // Near slide center (slide_w / 2 = 5_000_000)
        // moving's center: 4_510_000 + 500_000 = 5_010_000 -> diff = -10_000
        let moving_center = EmuRect::new(4_510_000, 3_000_000, 1_000_000, 1_000_000);
        let (dx, dy, guides) = snap_to_shapes(moving_center, &[], slide_w, slide_h, slop);
        assert_eq!(dx, -10_000);
        assert_eq!(dy, 0);
        assert_eq!(guides.len(), 1);
        assert!(matches!(guides[0], AlignmentGuide::Vertical { x: 5_000_000, .. }));
    }
}
