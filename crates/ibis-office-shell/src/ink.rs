// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 描画 (Excel's Draw tab) on a sheet: the pen and the highlighter draw a
//! stroke by dragging over the sheet, the eraser takes a stroke away by
//! touching it, 選択 leaves the pointer to select and move strokes as any
//! shape, and インクを図形に変換 turns a stroke into the line, rectangle,
//! ellipse or triangle it was meant to be.
//!
//! A stroke is a freeform shape (a custom geometry of straight pieces, as
//! Excel saves a freeform) named `インク n`; it is saved like any shape.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::*;
use ibis_doc_model::{Color, Geometry, LineCap, LineJoin, PathOp, Rgb, Shape, ShapeKind, Stroke};


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PenKind {
    Pen,
    Pencil,
    Highlighter,
    Eraser,
    /// No drawing: the pointer selects and moves.
    Select,
}

/// Special pen effects (レインボー・ギャラクシー・溶岩・海洋・ローズ ゴールド・ゴールド・シルバー・ブロンズ).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InkEffect {
    Rainbow,
    Galaxy,
    Lava,
    Ocean,
    RoseGold,
    Gold,
    Silver,
    Bronze,
}

impl InkEffect {
    pub const ALL: [Self; 8] = [
        Self::Rainbow,
        Self::Galaxy,
        Self::Lava,
        Self::Ocean,
        Self::RoseGold,
        Self::Gold,
        Self::Silver,
        Self::Bronze,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Rainbow => "ink-effect-rainbow",
            Self::Galaxy => "ink-effect-galaxy",
            Self::Lava => "ink-effect-lava",
            Self::Ocean => "ink-effect-ocean",
            Self::RoseGold => "ink-effect-rose-gold",
            Self::Gold => "ink-effect-gold",
            Self::Silver => "ink-effect-silver",
            Self::Bronze => "ink-effect-bronze",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Rainbow => "xlsx.ink.effect.rainbow",
            Self::Galaxy => "xlsx.ink.effect.galaxy",
            Self::Lava => "xlsx.ink.effect.lava",
            Self::Ocean => "xlsx.ink.effect.ocean",
            Self::RoseGold => "xlsx.ink.effect.rose-gold",
            Self::Gold => "xlsx.ink.effect.gold",
            Self::Silver => "xlsx.ink.effect.silver",
            Self::Bronze => "xlsx.ink.effect.bronze",
        }
    }

    pub fn stops(self) -> Vec<ibis_doc_model::GradientStop> {
        use ibis_doc_model::{Color, GradientStop, Rgb};
        let c = |rgb| Color::opaque(Rgb(rgb));
        match self {
            Self::Rainbow => vec![
                GradientStop { position: 0, color: c(0xFF0000) },
                GradientStop { position: 17_000, color: c(0xFF7F00) },
                GradientStop { position: 33_000, color: c(0xFFFF00) },
                GradientStop { position: 50_000, color: c(0x00E000) },
                GradientStop { position: 67_000, color: c(0x00E0FF) },
                GradientStop { position: 83_000, color: c(0x0040FF) },
                GradientStop { position: 100_000, color: c(0x9B30FF) },
            ],
            Self::Galaxy => vec![
                GradientStop { position: 0, color: c(0x1A053A) },
                GradientStop { position: 25_000, color: c(0x5C1D8D) },
                GradientStop { position: 50_000, color: c(0x9C27B0) },
                GradientStop { position: 75_000, color: c(0x00E5FF) },
                GradientStop { position: 100_000, color: c(0x0B1021) },
            ],
            Self::Lava => vec![
                GradientStop { position: 0, color: c(0x4A0000) },
                GradientStop { position: 25_000, color: c(0xB71C1C) },
                GradientStop { position: 50_000, color: c(0xFF4500) },
                GradientStop { position: 75_000, color: c(0xFF9100) },
                GradientStop { position: 100_000, color: c(0xFFEA00) },
            ],
            Self::Ocean => vec![
                GradientStop { position: 0, color: c(0x001220) },
                GradientStop { position: 25_000, color: c(0x005F73) },
                GradientStop { position: 50_000, color: c(0x0A9396) },
                GradientStop { position: 75_000, color: c(0x94D2BD) },
                GradientStop { position: 100_000, color: c(0xE0FBFC) },
            ],
            Self::RoseGold => vec![
                GradientStop { position: 0, color: c(0x8C4A56) },
                GradientStop { position: 25_000, color: c(0xB76E79) },
                GradientStop { position: 50_000, color: c(0xFAD6D6) },
                GradientStop { position: 75_000, color: c(0xC38D9E) },
                GradientStop { position: 100_000, color: c(0x9E5B6A) },
            ],
            Self::Gold => vec![
                GradientStop { position: 0, color: c(0x704E00) },
                GradientStop { position: 25_000, color: c(0xB8860B) },
                GradientStop { position: 50_000, color: c(0xFFF3B0) },
                GradientStop { position: 75_000, color: c(0xDAA520) },
                GradientStop { position: 100_000, color: c(0xC5A059) },
            ],
            Self::Silver => vec![
                GradientStop { position: 0, color: c(0x4A525D) },
                GradientStop { position: 25_000, color: c(0x9CA3AF) },
                GradientStop { position: 50_000, color: c(0xFFFFFF) },
                GradientStop { position: 75_000, color: c(0xCBD5E1) },
                GradientStop { position: 100_000, color: c(0x64748B) },
            ],
            Self::Bronze => vec![
                GradientStop { position: 0, color: c(0x3E2415) },
                GradientStop { position: 25_000, color: c(0x784824) },
                GradientStop { position: 50_000, color: c(0xE4A672) },
                GradientStop { position: 75_000, color: c(0xCD7F32) },
                GradientStop { position: 100_000, color: c(0x5D371A) },
            ],
        }
    }
}

pub const COLORS: [u32; 16] = [
    0x000000, 0xC00000, 0xFF6600, 0xFFD700, 0x00A050, 0x0070C0, 0x7030A0, 0x7F7F7F,
    0x595959, 0x800000, 0xED7D31, 0xFFC000, 0x70AD47, 0x4472C4, 0x1F4E79, 0xFFFFFF,
];
/// Thicknesses in points.
pub const WIDTHS: [f32; 4] = [1.0, 2.25, 4.5, 6.0];

pub struct InkPanel {
    pub pen: PenKind,
    /// Whether freeform lasso selection mode is active (`PenKind::Select` + `lasso == true`).
    pub lasso: bool,
    pub color: Rgb,
    pub effect: Option<InkEffect>,
    pub width: f32,
    /// Whether the mouse draws (off: only a pen or a finger would).
    pub mouse: bool,
    /// The stroke being drawn (sheet EMU), while the button is down.
    pub stroke: Option<Vec<(i64, i64)>>,
    pub message: String,
    pub ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

#[cfg(test)]
pub(crate) const IDS: &[&str] = &[
    "xlsx.ink",
    "xlsx.ink.title",
    "xlsx.ink.pen",
    "xlsx.ink.pencil",
    "xlsx.ink.highlighter",
    "xlsx.ink.eraser",
    "xlsx.ink.lasso",
    "xlsx.ink.select",
    "xlsx.ink.color",
    "xlsx.ink.effects",
    "xlsx.ink.effect.rainbow",
    "xlsx.ink.effect.galaxy",
    "xlsx.ink.effect.lava",
    "xlsx.ink.effect.ocean",
    "xlsx.ink.effect.rose-gold",
    "xlsx.ink.effect.gold",
    "xlsx.ink.effect.silver",
    "xlsx.ink.effect.bronze",
    "xlsx.ink.width",
    "xlsx.ink.width-pt",
    "xlsx.ink.mouse",
    "xlsx.ink.to-shape",
    "xlsx.ink.name",
    "xlsx.ink.message.no-ink",
    "xlsx.ink.message.not-recognized",
    "xlsx.ink.message.converted",
];

pub fn emu_per_pt() -> i64 {
    ibis_doc_model::EMU_PER_POINT
}

/// The points of an ink stroke (absolute sheet EMU), if the shape is one.
pub fn stroke_points(shape: &Shape) -> Option<Vec<(i64, i64)>> {
    let ShapeKind::Auto(auto) = &shape.kind else { return None };
    let Geometry::Formula(f) = &auto.geometry else { return None };
    if f.preset.is_some() {
        return None;
    }
    let path = f.def.paths.first()?;
    let sx = if path.width > 0 { shape.frame.width as f64 / path.width as f64 } else { 1.0 };
    let sy = if path.height > 0 { shape.frame.height as f64 / path.height as f64 } else { 1.0 };
    let mut out = Vec::new();
    for op in &path.ops {
        match op {
            PathOp::MoveTo(x, y) | PathOp::LineTo(x, y) => {
                let (x, y): (f64, f64) = (x.parse().ok()?, y.parse().ok()?);
                out.push((shape.frame.x + (x * sx) as i64, shape.frame.y + (y * sy) as i64));
            }
            _ => return None,
        }
    }
    (out.len() >= 2).then_some(out)
}

fn distance_to_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
    ((p.0 - (a.0 + t * dx)).powi(2) + (p.1 - (a.1 + t * dy)).powi(2)).sqrt()
}

/// How near (EMU) `p` is to the polyline.
pub fn distance_to_stroke(points: &[(i64, i64)], p: (i64, i64)) -> f64 {
    let f = |q: (i64, i64)| (q.0 as f64, q.1 as f64);
    points.windows(2).map(|w| distance_to_segment(f(p), f(w[0]), f(w[1]))).fold(f64::INFINITY, f64::min)
}

/// Whether `p` is inside the closed polygon `poly` (even-odd ray casting).
pub(crate) fn point_in_polygon(p: (i64, i64), poly: &[(i64, i64)]) -> bool {
    if poly.len() < 3 {
        return false;
    }
    let (px, py) = (p.0 as f64, p.1 as f64);
    let mut inside = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = (poly[i].0 as f64, poly[i].1 as f64);
        let (xj, yj) = (poly[j].0 as f64, poly[j].1 as f64);
        if ((yi > py) != (yj > py)) && (px < (xj - xi) * (py - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Whether a lasso polygon encloses an ink stroke (at least half of its points, or its bounding box center).
pub fn lasso_selects_stroke(lasso: &[(i64, i64)], stroke: &[(i64, i64)]) -> bool {
    if lasso.len() < 3 || stroke.is_empty() {
        return false;
    }
    let inside_count = stroke.iter().filter(|&&pt| point_in_polygon(pt, lasso)).count();
    if inside_count * 2 >= stroke.len() {
        return true;
    }
    let (min_x, max_x) = stroke.iter().fold((i64::MAX, i64::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
    let (min_y, max_y) = stroke.iter().fold((i64::MAX, i64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    point_in_polygon(((min_x + max_x) / 2, (min_y + max_y) / 2), lasso)
}

/// Basic four-shape recognition shared with Word and PowerPoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recognized {
    Line,
    Rectangle,
    Ellipse,
    Triangle,
}

/// Extended shape recognition for Excel's Ink-to-Shape (`XLSX-INK-006`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XlsxRecognized {
    Line,
    Rectangle,
    Ellipse,
    Triangle,
    Diamond,
    Pentagon,
    Hexagon,
    Star,
    RightArrow,
    LeftArrow,
    UpArrow,
    DownArrow,
}

impl XlsxRecognized {
    pub fn preset(self) -> Option<&'static str> {
        match self {
            Self::Line => None,
            Self::Rectangle => Some("rect"),
            Self::Ellipse => Some("ellipse"),
            Self::Triangle => Some("triangle"),
            Self::Diamond => Some("diamond"),
            Self::Pentagon => Some("pentagon"),
            Self::Hexagon => Some("hexagon"),
            Self::Star => Some("star5"),
            Self::RightArrow => Some("rightArrow"),
            Self::LeftArrow => Some("leftArrow"),
            Self::UpArrow => Some("upArrow"),
            Self::DownArrow => Some("downArrow"),
        }
    }
}

fn rdp_simplify(pts: &[(f64, f64)], eps: f64) -> Vec<(f64, f64)> {
    if pts.len() <= 2 {
        return pts.to_vec();
    }
    let first = pts[0];
    let last = pts[pts.len() - 1];
    let mut max_d = -1.0;
    let mut idx = 0;
    for (i, &p) in pts.iter().enumerate().skip(1).take(pts.len() - 2) {
        let d = distance_to_segment(p, first, last);
        if d > max_d {
            max_d = d;
            idx = i;
        }
    }
    if max_d > eps {
        let mut left = rdp_simplify(&pts[..=idx], eps);
        let right = rdp_simplify(&pts[idx..], eps);
        left.pop();
        left.extend(right);
        left
    } else {
        vec![first, last]
    }
}

fn simplify_closed_polygon(points: &[(i64, i64)], eps: f64, close_gap: f64) -> Vec<(f64, f64)> {
    let pts: Vec<(f64, f64)> = points.iter().map(|&(x, y)| (x as f64, y as f64)).collect();
    let mut corners = rdp_simplify(&pts, eps);
    if corners.len() > 2 {
        let (a, b) = (corners[0], *corners.last().unwrap());
        if (a.0 - b.0).hypot(a.1 - b.1) <= close_gap {
            corners.pop();
        }
    }
    // Remove collinear vertices (including across the seam where the stroke started/ended).
    loop {
        if corners.len() <= 3 {
            break;
        }
        let n = corners.len();
        let mut removed = false;
        for i in 0..n {
            let prev = corners[(i + n - 1) % n];
            let curr = corners[i];
            let next = corners[(i + 1) % n];
            if distance_to_segment(curr, prev, next) <= eps || (curr.0 - prev.0).hypot(curr.1 - prev.1) <= eps {
                corners.remove(i);
                removed = true;
                break;
            }
        }
        if !removed {
            break;
        }
    }
    corners
}

fn recognize_open_arrow(points: &[(i64, i64)], diagonal: f64) -> Option<XlsxRecognized> {
    if points.len() < 4 {
        return None;
    }
    let f = |q: &(i64, i64)| (q.0 as f64, q.1 as f64);
    let first = f(&points[0]);
    let (tip_idx, tip, shaft_len) = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let q = f(p);
            (i, q, (q.0 - first.0).hypot(q.1 - first.1))
        })
        .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))?;
    if tip_idx < 1 || tip_idx + 1 >= points.len() || shaft_len < 0.72 * diagonal {
        return None;
    }
    // All points up to the tip must follow the straight shaft.
    let shaft_dev = points[..=tip_idx]
        .iter()
        .map(|p| distance_to_segment(f(p), first, tip))
        .fold(0.0f64, f64::max);
    if shaft_dev > 0.10 * shaft_len {
        return None;
    }
    // All points after the tip must stay in the arrowhead region near `tip` and form at least one barb.
    let head_pts = &points[tip_idx..];
    let max_dist_from_tip = head_pts
        .iter()
        .map(|p| {
            let q = f(p);
            (q.0 - tip.0).hypot(q.1 - tip.1)
        })
        .fold(0.0f64, f64::max);
    let max_barb_spread = head_pts
        .iter()
        .map(|p| distance_to_segment(f(p), first, tip))
        .fold(0.0f64, f64::max);
    if max_dist_from_tip > 0.60 * shaft_len || max_barb_spread < 0.08 * shaft_len {
        return None;
    }
    let (dx, dy) = (tip.0 - first.0, tip.1 - first.1);
    if dx.abs() >= dy.abs() {
        Some(if dx >= 0.0 { XlsxRecognized::RightArrow } else { XlsxRecognized::LeftArrow })
    } else {
        Some(if dy >= 0.0 { XlsxRecognized::DownArrow } else { XlsxRecognized::UpArrow })
    }
}

/// Basic four-shape recognizer shared with Word and PowerPoint.
pub fn recognize(points: &[(i64, i64)]) -> Option<Recognized> {
    match recognize_xlsx(points)? {
        XlsxRecognized::Line => Some(Recognized::Line),
        XlsxRecognized::Rectangle => Some(Recognized::Rectangle),
        XlsxRecognized::Ellipse => Some(Recognized::Ellipse),
        XlsxRecognized::Triangle | XlsxRecognized::Diamond => Some(Recognized::Triangle),
        _ => None,
    }
}

/// Extended stroke recognizer for Excel (`XLSX-INK-006`): a straight stroke is a
/// line; a shaft with a barb at its tip is an arrow; a closed stroke is judged
/// by its simplified corners and how much of its box it fills (rectangle,
/// ellipse, triangle, diamond, pentagon, hexagon, star, or block arrow).
pub fn recognize_xlsx(points: &[(i64, i64)]) -> Option<XlsxRecognized> {
    if points.len() < 3 {
        return None;
    }
    let f = |q: &(i64, i64)| (q.0 as f64, q.1 as f64);
    let (min_x, max_x) = points.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.0 as f64), b.max(p.0 as f64)));
    let (min_y, max_y) = points.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.1 as f64), b.max(p.1 as f64)));
    let (w, h) = (max_x - min_x, max_y - min_y);
    let diagonal = (w * w + h * h).sqrt();
    if diagonal == 0.0 {
        return None;
    }
    let (first, last) = (f(&points[0]), f(points.last()?));
    let gap = ((first.0 - last.0).powi(2) + (first.1 - last.1).powi(2)).sqrt();
    // Straight: every point near the line from the first to the last.
    let deviation = points.iter().map(|p| distance_to_segment(f(p), first, last)).fold(0.0f64, f64::max);
    if gap > 0.6 * diagonal && deviation < 0.08 * gap {
        return Some(XlsxRecognized::Line);
    }
    if w < 0.15 * diagonal || h < 0.15 * diagonal {
        return None;
    }
    if let Some(arrow) = recognize_open_arrow(points, diagonal) {
        return Some(arrow);
    }
    if gap > 0.25 * diagonal {
        return None;
    }
    // The area the closed stroke surrounds (shoelace) against its box.
    let mut area = 0.0;
    for i in 0..points.len() {
        let (a, b) = (f(&points[i]), f(&points[(i + 1) % points.len()]));
        area += a.0 * b.1 - b.0 * a.1;
    }
    let ratio = area.abs() / 2.0 / (w * h);
    // How many of the points lie near the box's edge: a rectangle's all do.
    let edge = (0.04 * w.min(h)).max(1.0);
    let near_edge = points.iter().filter(|p| {
        let (x, y) = f(p);
        (x - min_x).abs() < edge || (max_x - x).abs() < edge || (y - min_y).abs() < edge || (max_y - y).abs() < edge
    }).count() as f64
        / points.len() as f64;
    if ratio > 0.85 && near_edge > 0.8 {
        return Some(XlsxRecognized::Rectangle);
    }
    let (cx, cy) = ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
    let corners = simplify_closed_polygon(points, 0.028 * diagonal, 0.25 * diagonal);
    // 10-vertex star outline (alternating outer tips and inner valleys) or 5-vertex self-intersecting pentagram.
    if corners.len() == 10 && (0.20..0.58).contains(&ratio) {
        let radii: Vec<f64> = corners.iter().map(|&(x, y)| (x - cx).hypot(y - cy)).collect();
        let (mut even, mut odd) = (0.0, 0.0);
        for (i, &r) in radii.iter().enumerate() {
            if i % 2 == 0 {
                even += r;
            } else {
                odd += r;
            }
        }
        let (r_min, r_max) = if even < odd { (even, odd) } else { (odd, even) };
        if r_max > 0.0 && r_min / r_max < 0.72 {
            return Some(XlsxRecognized::Star);
        }
    }
    if corners.len() == 5 && ratio < 0.36 {
        return Some(XlsxRecognized::Star);
    }
    // 4-vertex diamond: vertices near the 4 edge midpoints (top, right, bottom, left).
    if corners.len() == 4 && (0.38..0.64).contains(&ratio) && near_edge < 0.5 {
        let mids = [(cx, min_y), (max_x, cy), (cx, max_y), (min_x, cy)];
        let all_near_mids = mids.iter().all(|&(mx, my)| {
            corners.iter().any(|&(x, y)| (x - mx).hypot(y - my) < 0.22 * diagonal)
        });
        if all_near_mids {
            return Some(XlsxRecognized::Diamond);
        }
    }
    // Closed 7-vertex block arrow.
    if corners.len() == 7 && (0.40..0.82).contains(&ratio) {
        let tol = 0.06 * diagonal;
        let right_extremes = corners.iter().filter(|&&(x, _)| (max_x - x).abs() < tol).count();
        let left_extremes = corners.iter().filter(|&&(x, _)| (x - min_x).abs() < tol).count();
        let bottom_extremes = corners.iter().filter(|&&(_, y)| (max_y - y).abs() < tol).count();
        let top_extremes = corners.iter().filter(|&&(_, y)| (y - min_y).abs() < tol).count();
        if right_extremes == 1 && left_extremes >= 2 {
            return Some(XlsxRecognized::RightArrow);
        } else if left_extremes == 1 && right_extremes >= 2 {
            return Some(XlsxRecognized::LeftArrow);
        } else if bottom_extremes == 1 && top_extremes >= 2 {
            return Some(XlsxRecognized::DownArrow);
        } else if top_extremes == 1 && bottom_extremes >= 2 {
            return Some(XlsxRecognized::UpArrow);
        }
    }
    // 5-vertex pentagon or 6-vertex hexagon (straight-sided polygons).
    if corners.len() == 5 && (0.55..0.82).contains(&ratio) {
        return Some(XlsxRecognized::Pentagon);
    }
    if corners.len() == 6 && (0.60..0.86).contains(&ratio) {
        return Some(XlsxRecognized::Hexagon);
    }
    if (0.6..=0.88).contains(&ratio) && near_edge < 0.7 {
        Some(XlsxRecognized::Ellipse)
    } else if (0.35..0.62).contains(&ratio) {
        Some(XlsxRecognized::Triangle)
    } else {
        None
    }
}


/// The stroke a pen draws: round ends, a highlighter half see-through.
pub fn ink_stroke(pen: PenKind, color: Rgb, width_pt: f32) -> Stroke {
    ink_stroke_with_effect(pen, color, None, width_pt)
}

/// A pen stroke with an optional ink effect (e.g. Rainbow, Galaxy, etc.).
pub fn ink_stroke_with_effect(pen: PenKind, color: Rgb, effect: Option<InkEffect>, width_pt: f32) -> Stroke {
    let alpha = match pen {
        PenKind::Highlighter => 128,
        PenKind::Pencil => 192,
        _ => 255,
    };
    let mut stroke = Stroke::solid(Color { rgb: color, alpha }, (width_pt * emu_per_pt() as f32) as i64);
    stroke.cap = LineCap::Round;
    stroke.join = LineJoin::Round;
    if let Some(effect) = effect {
        let stops = effect.stops();
        if let Some(first) = stops.first() {
            stroke.color = first.color;
        }
        stroke.fill = Some(ibis_doc_model::Fill::Linear { angle: 0, scaled: false, stops });
    }
    stroke
}

/// Whether a shape's name is an ink stroke's (`インク 3`, `Ink 3`, `インク 1 (レインボー)`).
pub fn is_ink_name(name: &str) -> bool {
    ["インク ", "Ink "].iter().any(|p| {
        let Some(rest) = name.strip_prefix(p) else { return false };
        let num = rest.split_whitespace().next().unwrap_or("").trim_end_matches(|c: char| !c.is_ascii_digit());
        !num.is_empty() && num.chars().all(|c| c.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    // Named, not `super::*` (gpui_kit's glob brings a `test` macro that replaces `#[test]`).
    use super::{IDS, Recognized, XlsxRecognized, distance_to_stroke, is_ink_name, lasso_selects_stroke, recognize, recognize_xlsx};

    fn circle(n: usize) -> Vec<(i64, i64)> {
        (0..=n).map(|i| {
            let t = std::f64::consts::TAU * i as f64 / n as f64;
            ((1_000_000.0 + 500_000.0 * t.cos()) as i64, (1_000_000.0 + 400_000.0 * t.sin()) as i64)
        }).collect()
    }

    fn regular_polygon_stroke(sides: usize, steps_per_edge: usize) -> Vec<(i64, i64)> {
        let verts: Vec<(f64, f64)> = (0..sides)
            .map(|i| {
                let angle = -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * (i as f64) / (sides as f64);
                (1_000_000.0 + 500_000.0 * angle.cos(), 1_000_000.0 + 500_000.0 * angle.sin())
            })
            .collect();
        let mut pts = Vec::new();
        for i in 0..sides {
            let (x0, y0) = verts[i];
            let (x1, y1) = verts[(i + 1) % sides];
            let start = usize::from(i > 0);
            for s in start..=steps_per_edge {
                let t = s as f64 / steps_per_edge as f64;
                pts.push(((x0 + (x1 - x0) * t) as i64, (y0 + (y1 - y0) * t) as i64));
            }
        }
        pts
    }

    #[test]
    fn strokes_are_recognized_as_lines_and_closed_shapes() {
        // A slightly wavering straight stroke.
        let line: Vec<(i64, i64)> = (0..=20).map(|i| (i * 50_000, 100_000 + if i % 2 == 0 { 3000 } else { -3000 })).collect();
        assert_eq!(recognize(&line), Some(Recognized::Line));
        // A rectangle drawn round its edges, coming back to the start.
        let mut rect = Vec::new();
        for i in 0..=10 {
            rect.push((i * 100_000, 0));
        }
        for i in 1..=6 {
            rect.push((1_000_000, i * 100_000));
        }
        for i in 1..=10 {
            rect.push((1_000_000 - i * 100_000, 600_000));
        }
        for i in 1..=5 {
            rect.push((0, 600_000 - i * 100_000));
        }
        assert_eq!(recognize(&rect), Some(Recognized::Rectangle));
        assert_eq!(recognize(&circle(40)), Some(Recognized::Ellipse));
        // A triangle.
        let mut tri = Vec::new();
        for i in 0..=10 {
            tri.push((i * 100_000, 800_000));
        }
        for i in 1..=10 {
            tri.push((1_000_000 - i * 50_000, 800_000 - i * 80_000));
        }
        for i in 1..=10 {
            tri.push((500_000 - i * 50_000, i * 80_000));
        }
        assert_eq!(recognize(&tri), Some(Recognized::Triangle));
        // A diamond (rhombus with top/right/bottom/left vertices).
        assert_eq!(recognize_xlsx(&regular_polygon_stroke(4, 10)), Some(XlsxRecognized::Diamond));
        // A pentagon and a hexagon.
        assert_eq!(recognize_xlsx(&regular_polygon_stroke(5, 8)), Some(XlsxRecognized::Pentagon));
        assert_eq!(recognize_xlsx(&regular_polygon_stroke(6, 8)), Some(XlsxRecognized::Hexagon));
        // A 5-pointed star (10 alternating outer/inner vertices).
        let star_verts: Vec<(f64, f64)> = (0..10)
            .map(|i| {
                let r = if i % 2 == 0 { 500_000.0 } else { 200_000.0 };
                let angle = -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * (i as f64) / 10.0;
                (1_000_000.0 + r * angle.cos(), 1_000_000.0 + r * angle.sin())
            })
            .collect();
        let mut star = Vec::new();
        for i in 0..10 {
            let (x0, y0) = star_verts[i];
            let (x1, y1) = star_verts[(i + 1) % 10];
            for s in usize::from(i > 0)..=6 {
                let t = s as f64 / 6.0;
                star.push(((x0 + (x1 - x0) * t) as i64, (y0 + (y1 - y0) * t) as i64));
            }
        }
        assert_eq!(recognize_xlsx(&star), Some(XlsxRecognized::Star));
        // Open arrow strokes (shaft + arrowhead barb).
        let mut right_arrow: Vec<(i64, i64)> = (0..=10).map(|i| (i * 80_000, 500_000)).collect();
        for i in 1..=5 {
            right_arrow.push((800_000 - i * 40_000, 500_000 - i * 40_000));
        }
        assert_eq!(recognize_xlsx(&right_arrow), Some(XlsxRecognized::RightArrow));
        // A scribble that is none of them.
        let scribble: Vec<(i64, i64)> = (0..30).map(|i| ((i * 37_000) % 900_000, (i * 91_000) % 700_000)).collect();
        assert_eq!(recognize_xlsx(&scribble), None);
    }

    #[test]
    fn lasso_selects_enclosed_strokes_only() {
        let lasso = vec![(0, 0), (1_000_000, 0), (1_000_000, 1_000_000), (0, 1_000_000)];
        let inside = vec![(200_000, 300_000), (600_000, 300_000), (800_000, 500_000)];
        let outside = vec![(1_200_000, 300_000), (1_600_000, 300_000)];
        assert!(lasso_selects_stroke(&lasso, &inside));
        assert!(!lasso_selects_stroke(&lasso, &outside));
    }

    #[test]
    fn a_pen_touches_a_stroke_only_near_its_line() {
        let line = vec![(0, 0), (1_000_000, 0)];
        assert!(distance_to_stroke(&line, (500_000, 30_000)) < 40_000.0);
        assert!(distance_to_stroke(&line, (500_000, 400_000)) > 300_000.0);
        assert!(is_ink_name("インク 12") && is_ink_name("Ink 3") && is_ink_name("インク 1 (レインボー)") && is_ink_name("Ink 2 (Galaxy)") && !is_ink_name("Ink") && !is_ink_name("図 1"));
    }

    #[test]
    fn pen_colors_and_widths_and_effects_are_defined() {
        assert_eq!(super::COLORS.len(), 16);
        assert_eq!(super::WIDTHS.len(), 4);
        assert_eq!(super::InkEffect::ALL.len(), 8);
        for effect in super::InkEffect::ALL {
            let stops = effect.stops();
            assert!(stops.len() >= 5);
            assert_eq!(stops[0].position, 0);
            assert_eq!(stops.last().unwrap().position, 100_000);
            assert!(!effect.id().is_empty());
            assert!(!effect.label_key().is_empty());
        }
    }

    #[test]
    fn ink_stroke_with_effect_creates_linear_gradient() {
        use ibis_doc_model::{Fill, Rgb};
        let solid = super::ink_stroke(super::PenKind::Pen, Rgb(0), 2.25);
        assert_eq!(solid.fill, None);

        let rainbow = super::ink_stroke_with_effect(super::PenKind::Pen, Rgb(0), Some(super::InkEffect::Rainbow), 2.25);
        assert!(matches!(rainbow.fill, Some(Fill::Linear { .. })));
        if let Some(Fill::Linear { stops, .. }) = rainbow.fill {
            assert_eq!(stops.len(), 7);
            assert_eq!(rainbow.color, stops[0].color);
        }
    }

    #[test]
    fn every_caption_is_in_both_languages() {
        for table in [include_str!("../../ibis-office-base/i18n/ja/xlsx.tsv"), include_str!("../../ibis-office-base/i18n/en/xlsx.tsv")] {
            for id in IDS {
                assert!(table.lines().any(|l| l.split('\t').next() == Some(*id)), "{id} is missing");
            }
        }
    }
}
