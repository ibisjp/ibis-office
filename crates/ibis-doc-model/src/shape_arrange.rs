// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Lining shapes up and spreading them out (配置 > 左揃え … 左右に整列): the calculation the window and the MCP tools share. The box of a shape is the one it takes on the
//! slide with its rotation; moving it moves the frame by the same amount.

use crate::command::Command;
use crate::geometry::EmuRect;
use crate::model::{Shape, SlideId};
use crate::ShapeId;

/// The edge or the middle shapes are lined up to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlignEdge {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

/// One shape to arrange: its id, its frame and the box it takes on the slide (rotation included).
#[derive(Clone, Copy, Debug)]
pub struct Arranged {
    pub id: ShapeId,
    pub frame: EmuRect,
    pub bounds: EmuRect,
}

impl Arranged {
    pub fn of(shape: &Shape) -> Self {
        Self { id: shape.id, frame: shape.frame, bounds: shape.bounds() }
    }
}

impl Shape {
    /// The box the shape takes on the slide, rotation included.
    pub fn bounds(&self) -> EmuRect {
        let (l, t, r, b) = (self.frame.x as f64, self.frame.y as f64, self.frame.right() as f64, self.frame.bottom() as f64);
        let (cx, cy) = (self.frame.x as f64 + self.frame.width as f64 / 2.0, self.frame.y as f64 + self.frame.height as f64 / 2.0);
        let (s, k) = (f64::from(self.rotation) / 60_000.0).to_radians().sin_cos();
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for (x, y) in [(l, t), (r, t), (r, b), (l, b)] {
            let (dx, dy) = (x - cx, y - cy);
            let (px, py) = (cx + dx * k - dy * s, cy + dx * s + dy * k);
            min_x = min_x.min(px);
            min_y = min_y.min(py);
            max_x = max_x.max(px);
            max_y = max_y.max(py);
        }
        EmuRect::new(min_x.round() as i64, min_y.round() as i64, (max_x - min_x).round() as i64, (max_y - min_y).round() as i64)
    }
}

/// The union of the boxes.
pub fn union_of(shapes: &[Arranged]) -> EmuRect {
    let l = shapes.iter().map(|s| s.bounds.x).min().unwrap_or(0);
    let t = shapes.iter().map(|s| s.bounds.y).min().unwrap_or(0);
    let r = shapes.iter().map(|s| s.bounds.right()).max().unwrap_or(0);
    let b = shapes.iter().map(|s| s.bounds.bottom()).max().unwrap_or(0);
    EmuRect::new(l, t, r - l, b - t)
}

/// The commands that line `shapes` up to `edge` of `area` (the slide, or the union of the shapes when `area` is `None`; one shape is lined up on `slide_area`).
pub fn align(slide: SlideId, shapes: &[Arranged], edge: AlignEdge, slide_area: EmuRect, to_slide: bool) -> Vec<Command> {
    if shapes.is_empty() {
        return Vec::new();
    }
    let area = if shapes.len() == 1 || to_slide { slide_area } else { union_of(shapes) };
    shapes
        .iter()
        .filter_map(|s| {
            let b = s.bounds;
            let (dx, dy) = match edge {
                AlignEdge::Left => (area.x - b.x, 0),
                AlignEdge::Center => (area.x + area.width / 2 - (b.x + b.width / 2), 0),
                AlignEdge::Right => (area.right() - b.right(), 0),
                AlignEdge::Top => (0, area.y - b.y),
                AlignEdge::Middle => (0, area.y + area.height / 2 - (b.y + b.height / 2)),
                AlignEdge::Bottom => (0, area.bottom() - b.bottom()),
            };
            ((dx, dy) != (0, 0)).then(|| Command::SetFrame { slide, frames: vec![(s.id, s.frame.translated(dx, dy))], with_attached: true })
        })
        .collect()
}

/// The commands that spread three or more shapes with equal gaps, across the slide (`slide_span`: its width or height) or between the outer shapes.
pub fn distribute(slide: SlideId, shapes: &[Arranged], horizontal: bool, slide_span: i64, to_slide: bool) -> Vec<Command> {
    if shapes.len() < 3 {
        return Vec::new();
    }
    let key = |b: &EmuRect| if horizontal { b.x } else { b.y };
    let len = |b: &EmuRect| if horizontal { b.width } else { b.height };
    let mut shapes = shapes.to_vec();
    shapes.sort_by_key(|s| key(&s.bounds));
    let (first, last) = if to_slide {
        (0, slide_span)
    } else {
        let first = key(&shapes[0].bounds);
        (first, shapes.last().map(|s| key(&s.bounds) + len(&s.bounds)).unwrap_or(first))
    };
    let total: i64 = shapes.iter().map(|s| len(&s.bounds)).sum();
    let gap = (last - first - total) as f64 / (shapes.len() - 1) as f64;
    let mut at = first as f64;
    let mut commands = Vec::new();
    for s in shapes {
        let d = at.round() as i64 - key(&s.bounds);
        if d != 0 {
            commands.push(Command::SetFrame { slide, frames: vec![(s.id, if horizontal { s.frame.translated(d, 0) } else { s.frame.translated(0, d) })], with_attached: true });
        }
        at += len(&s.bounds) as f64 + gap;
    }
    commands
}
