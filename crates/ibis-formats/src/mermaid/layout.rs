// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Layered layout, edge routing and grouped (framed) layout shared by the graph diagrams.

use super::*;

pub(super) fn edge_anchor(cx: f32, cy: f32, w: f32, h: f32, tx: f32, ty: f32) -> (f32, f32) {
    let dx = tx - cx;
    let dy = ty - cy;
    if dx.abs() < 1e-3 && dy.abs() < 1e-3 {
        return (cx, cy);
    }
    let rx = w / 2.0;
    let ry = h / 2.0;
    let sx = if dx.abs() > 1e-3 { rx / dx.abs() } else { f32::MAX };
    let sy = if dy.abs() > 1e-3 { ry / dy.abs() } else { f32::MAX };
    let scale = sx.min(sy);
    (cx + dx * scale, cy + dy * scale)
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RectF {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

pub(super) fn node_rect(cx: f32, cy: f32, w: f32, h: f32) -> RectF {
    RectF { x: cx - w / 2.0, y: cy - h / 2.0, w, h }
}

pub(super) fn pad_rect(rect: RectF, pad: f32) -> RectF {
    RectF {
        x: rect.x - pad,
        y: rect.y - pad,
        w: (rect.w + pad * 2.0).max(0.0),
        h: (rect.h + pad * 2.0).max(0.0),
    }
}

pub(super) fn point_in_rect(px: f32, py: f32, rect: &RectF) -> bool {
    px > rect.x + 0.15 && py > rect.y + 0.15 && px < rect.x + rect.w - 0.15 && py < rect.y + rect.h - 0.15
}

pub(super) fn orient(ax: f32, ay: f32, bx: f32, by: f32, cx: f32, cy: f32) -> f32 {
    (bx - ax) * (cy - ay) - (by - ay) * (cx - ax)
}

pub(super) fn on_segment(ax: f32, ay: f32, bx: f32, by: f32, px: f32, py: f32) -> bool {
    px >= ax.min(bx) - 0.05 && px <= ax.max(bx) + 0.05 && py >= ay.min(by) - 0.05 && py <= ay.max(by) + 0.05
}

pub(super) fn segments_cross(x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32, x4: f32, y4: f32) -> bool {
    let o1 = orient(x1, y1, x2, y2, x3, y3);
    let o2 = orient(x1, y1, x2, y2, x4, y4);
    let o3 = orient(x3, y3, x4, y4, x1, y1);
    let o4 = orient(x3, y3, x4, y4, x2, y2);
    if o1 * o2 < 0.0 && o3 * o4 < 0.0 {
        return true;
    }
    o1.abs() <= 0.05 && on_segment(x1, y1, x2, y2, x3, y3)
        || o2.abs() <= 0.05 && on_segment(x1, y1, x2, y2, x4, y4)
        || o3.abs() <= 0.05 && on_segment(x3, y3, x4, y4, x1, y1)
        || o4.abs() <= 0.05 && on_segment(x3, y3, x4, y4, x2, y2)
}

pub(super) fn segment_hits_rect(x1: f32, y1: f32, x2: f32, y2: f32, rect: &RectF) -> bool {
    if (x1 - x2).abs() < 0.4 && (y1 - y2).abs() < 0.4 {
        return point_in_rect(x1, y1, rect);
    }
    if point_in_rect(x1, y1, rect) || point_in_rect(x2, y2, rect) {
        return true;
    }
    let left = rect.x;
    let right = rect.x + rect.w;
    let top = rect.y;
    let bottom = rect.y + rect.h;
    segments_cross(x1, y1, x2, y2, left, top, right, top)
        || segments_cross(x1, y1, x2, y2, right, top, right, bottom)
        || segments_cross(x1, y1, x2, y2, right, bottom, left, bottom)
        || segments_cross(x1, y1, x2, y2, left, bottom, left, top)
}

pub(super) fn segment_hits_any(x1: f32, y1: f32, x2: f32, y2: f32, obstacles: &[RectF]) -> bool {
    obstacles.iter().any(|rect| segment_hits_rect(x1, y1, x2, y2, rect))
}

pub(super) fn cubic_point(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    let a = u * u * u;
    let b = 3.0 * u * u * t;
    let c = 3.0 * u * t * t;
    let d = t * t * t;
    (a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0, a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1)
}

pub(super) fn curve_hits(sx: f32, sy: f32, ex: f32, ey: f32, horizontal: bool, obstacles: &[RectF]) -> bool {
    let (p1, p2) = if horizontal {
        let mx = (sx + ex) / 2.0;
        ((mx, sy), (mx, ey))
    } else {
        let my = (sy + ey) / 2.0;
        ((sx, my), (ex, my))
    };
    let mut prev = (sx, sy);
    for step in 1..=24 {
        let p = cubic_point((sx, sy), p1, p2, (ex, ey), step as f32 / 24.0);
        if segment_hits_any(prev.0, prev.1, p.0, p.1, obstacles) {
            return true;
        }
        prev = p;
    }
    false
}

pub(super) fn curve_d(sx: f32, sy: f32, ex: f32, ey: f32, horizontal: bool) -> String {
    if horizontal {
        let mx = (sx + ex) / 2.0;
        format!("M {sx:.1} {sy:.1} C {mx:.1} {sy:.1}, {mx:.1} {ey:.1}, {ex:.1} {ey:.1}")
    } else {
        let my = (sy + ey) / 2.0;
        format!("M {sx:.1} {sy:.1} C {sx:.1} {my:.1}, {ex:.1} {my:.1}, {ex:.1} {ey:.1}")
    }
}

pub(super) fn unit_dir(x1: f32, y1: f32, x2: f32, y2: f32) -> (f32, f32) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-3 { (0.0, 0.0) } else { (dx / len, dy / len) }
}

pub(super) fn point_in_canvas(px: f32, py: f32, canvas: (f32, f32)) -> bool {
    px >= -0.05 && py >= -0.05 && px <= canvas.0 + 0.05 && py <= canvas.1 + 0.05
}

/// Free lane coordinates inside `[lo, hi]`, inset from obstacle intervals.
pub(super) fn lane_coords(lo: f32, hi: f32, blocked: &[(f32, f32)]) -> Vec<f32> {
    if hi - lo < 8.0 {
        return Vec::new();
    }
    let mut spans: Vec<(f32, f32)> = blocked.iter().copied().filter(|(a, b)| b - a > 0.5).collect();
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut merged: Vec<(f32, f32)> = Vec::new();
    for (a, b) in spans {
        if let Some((_, end)) = merged.last_mut()
            && a <= *end
        {
            *end = end.max(b);
            continue;
        }
        merged.push((a, b));
    }
    let mut gaps = Vec::new();
    let mut cursor = lo;
    for (a, b) in merged {
        let gap_end = a.min(hi);
        if gap_end - cursor >= 8.0 {
            gaps.push((cursor, gap_end));
        }
        cursor = cursor.max(b);
        if cursor >= hi {
            break;
        }
    }
    if hi - cursor >= 8.0 {
        gaps.push((cursor, hi));
    }
    let mut lanes = Vec::new();
    for (a, b) in gaps {
        let inset_a = a + 6.0;
        let inset_b = b - 6.0;
        if inset_b >= inset_a {
            lanes.push((inset_a + inset_b) / 2.0);
            if inset_b - inset_a > 12.0 {
                lanes.push(inset_a);
                lanes.push(inset_b);
            }
        }
    }
    lanes
}

pub(super) fn exit_points(anchor: (f32, f32), center: (f32, f32), own: RectF, obstacles: &[RectF], canvas: (f32, f32)) -> Vec<(f32, f32)> {
    let own = pad_rect(own, -0.5);
    let dx = anchor.0 - center.0;
    let dy = anchor.1 - center.1;
    let mut dirs = Vec::new();
    if dx.abs() >= dy.abs() && dx.abs() > 1e-3 {
        dirs.push((dx.signum(), 0.0));
        dirs.push((0.0, 1.0));
        dirs.push((0.0, -1.0));
    } else if dy.abs() > 1e-3 {
        dirs.push((0.0, dy.signum()));
        dirs.push((1.0, 0.0));
        dirs.push((-1.0, 0.0));
    }
    let mut out = Vec::new();
    for (ux, uy) in dirs {
        for len in [14.0_f32, 8.0] {
            let p = (anchor.0 + ux * len, anchor.1 + uy * len);
            if !point_in_canvas(p.0, p.1, canvas) {
                continue;
            }
            if point_in_rect(p.0, p.1, &own) || segment_hits_rect(anchor.0, anchor.1, p.0, p.1, &own) {
                continue;
            }
            if obstacles.iter().any(|rect| point_in_rect(p.0, p.1, rect) || segment_hits_rect(anchor.0, anchor.1, p.0, p.1, rect)) {
                continue;
            }
            out.push(p);
            break;
        }
    }
    if out.is_empty() {
        out.push(anchor);
    }
    out
}

pub(super) fn simplify_points(points: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut cleaned = Vec::new();
    for &p in points {
        if cleaned.last().is_some_and(|q: &(f32, f32)| (q.0 - p.0).abs() < 0.4 && (q.1 - p.1).abs() < 0.4) {
            continue;
        }
        cleaned.push(p);
    }
    if cleaned.len() < 3 {
        return cleaned;
    }
    let mut slim = vec![cleaned[0]];
    for i in 1..cleaned.len() - 1 {
        let a = *slim.last().unwrap();
        let b = cleaned[i];
        let c = cleaned[i + 1];
        let cross = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        if cross.abs() > 0.5 {
            slim.push(b);
        }
    }
    slim.push(*cleaned.last().unwrap());
    slim
}

pub(super) fn polyline_clear(points: &[(f32, f32)], obstacles: &[RectF], canvas: (f32, f32)) -> bool {
    if points.iter().any(|p| !point_in_canvas(p.0, p.1, canvas)) {
        return false;
    }
    points.windows(2).all(|w| !segment_hits_any(w[0].0, w[0].1, w[1].0, w[1].1, obstacles))
}

pub(super) fn label_on_polyline(points: &[(f32, f32)], obstacles: &[RectF]) -> (f32, f32) {
    let mut best_len = -1.0f32;
    let mut best = points[0];
    for w in points.windows(2) {
        let dx = w[1].0 - w[0].0;
        let dy = w[1].1 - w[0].1;
        let len = (dx * dx + dy * dy).sqrt();
        if len <= best_len {
            continue;
        }
        let mid = ((w[0].0 + w[1].0) / 2.0, (w[0].1 + w[1].1) / 2.0);
        if obstacles.iter().any(|rect| point_in_rect(mid.0, mid.1, rect)) {
            continue;
        }
        best_len = len;
        best = mid;
    }
    best
}

pub(super) fn path_from_points(points: &[(f32, f32)]) -> String {
    let mut d = format!("M {:.1} {:.1}", points[0].0, points[0].1);
    for p in &points[1..] {
        d.push_str(&format!(" L {:.1} {:.1}", p.0, p.1));
    }
    d
}

pub(super) fn first_dir(points: &[(f32, f32)]) -> (f32, f32) {
    for w in points.windows(2) {
        let d = unit_dir(w[0].0, w[0].1, w[1].0, w[1].1);
        if d.0 != 0.0 || d.1 != 0.0 {
            return d;
        }
    }
    (0.0, 0.0)
}

pub(super) fn last_dir(points: &[(f32, f32)]) -> (f32, f32) {
    for i in (1..points.len()).rev() {
        let d = unit_dir(points[i - 1].0, points[i - 1].1, points[i].0, points[i].1);
        if d.0 != 0.0 || d.1 != 0.0 {
            return d;
        }
    }
    (0.0, 0.0)
}

#[derive(Debug)]
pub(super) enum EdgeRouteKind {
    Line,
    Path { d: String, detour: bool },
}

#[derive(Debug)]
pub(super) struct EdgeRoute {
    pub(super) kind: EdgeRouteKind,
    pub(super) label_at: (f32, f32),
    pub(super) start: (f32, f32),
    pub(super) end: (f32, f32),
    pub(super) leave_dir: (f32, f32),
    pub(super) arrive_dir: (f32, f32),
}

pub(super) fn straight_route(start: (f32, f32), end: (f32, f32)) -> EdgeRoute {
    let dir = unit_dir(start.0, start.1, end.0, end.1);
    EdgeRoute {
        kind: EdgeRouteKind::Line,
        label_at: ((start.0 + end.0) / 2.0, (start.1 + end.1) / 2.0),
        start,
        end,
        leave_dir: dir,
        arrive_dir: dir,
    }
}

pub(super) fn path_route(d: String, detour: bool, start: (f32, f32), end: (f32, f32), label_at: (f32, f32), points: &[(f32, f32)]) -> EdgeRoute {
    let (leave_dir, arrive_dir) = if points.len() >= 2 {
        (first_dir(points), last_dir(points))
    } else {
        let dir = unit_dir(start.0, start.1, end.0, end.1);
        (dir, dir)
    };
    EdgeRoute { kind: EdgeRouteKind::Path { d, detour }, label_at, start, end, leave_dir, arrive_dir }
}

/// Border-to-border route. A curve or line that enters another node becomes an
/// orthogonal polyline. When no lane stays outside the nodes, the original curve remains.
pub(super) fn route_layered_edge(
    from: usize,
    to: usize,
    centers: &[(f32, f32)],
    sizes: &[(f32, f32)],
    horizontal: bool,
    curved: bool,
    canvas: (f32, f32),
) -> EdgeRoute {
    let (x1, y1) = centers[from];
    let (x2, y2) = centers[to];
    let (w1, h1) = sizes[from];
    let (w2, h2) = sizes[to];
    let start = edge_anchor(x1, y1, w1, h1, x2, y2);
    let end = edge_anchor(x2, y2, w2, h2, x1, y1);
    let fallback = |start: (f32, f32), end: (f32, f32)| -> EdgeRoute {
        if curved {
            let d = curve_d(start.0, start.1, end.0, end.1, horizontal);
            let dir = unit_dir(start.0, start.1, end.0, end.1);
            EdgeRoute {
                kind: EdgeRouteKind::Path { d, detour: false },
                label_at: ((start.0 + end.0) / 2.0, (start.1 + end.1) / 2.0),
                start,
                end,
                leave_dir: dir,
                arrive_dir: dir,
            }
        } else {
            straight_route(start, end)
        }
    };
    if from == to {
        return fallback(start, end);
    }
    let mut blockers = Vec::new();
    for idx in 0..centers.len() {
        if idx == from || idx == to {
            continue;
        }
        blockers.push(node_rect(centers[idx].0, centers[idx].1, sizes[idx].0, sizes[idx].1));
    }
    let hits = if curved {
        curve_hits(start.0, start.1, end.0, end.1, horizontal, &blockers)
    } else {
        segment_hits_any(start.0, start.1, end.0, end.1, &blockers)
    };
    if !hits {
        return fallback(start, end);
    }

    let source = node_rect(x1, y1, w1, h1);
    let target = node_rect(x2, y2, w2, h2);
    let mut avoid: Vec<RectF> = blockers.iter().copied().map(|rect| pad_rect(rect, 4.0)).collect();
    let shrunk_source = pad_rect(source, -0.5);
    let shrunk_target = pad_rect(target, -0.5);
    avoid.push(shrunk_source);
    avoid.push(shrunk_target);
    let exits_s = exit_points(start, (x1, y1), source, &blockers, canvas);
    let exits_t = exit_points(end, (x2, y2), target, &blockers, canvas);
    let pad = 8.0;
    let y_span = (start.1.min(end.1) - 14.0, start.1.max(end.1) + 14.0);
    let x_span = (start.0.min(end.0) - 14.0, start.0.max(end.0) + 14.0);
    let blocked_x: Vec<(f32, f32)> = avoid
        .iter()
        .filter(|rect| rect.y < y_span.1 && rect.y + rect.h > y_span.0)
        .map(|rect| (rect.x, rect.x + rect.w))
        .collect();
    let blocked_y: Vec<(f32, f32)> = avoid
        .iter()
        .filter(|rect| rect.x < x_span.1 && rect.x + rect.w > x_span.0)
        .map(|rect| (rect.y, rect.y + rect.h))
        .collect();
    // `vertical` lanes are constant-x. Flow-direction gaps are tried first.
    let axes = if horizontal { [false, true] } else { [true, false] };
    for vertical_lane in axes {
        let mut lanes = if vertical_lane {
            lane_coords(pad, canvas.0 - pad, &blocked_x)
        } else {
            lane_coords(pad, canvas.1 - pad, &blocked_y)
        };
        let ideal = if vertical_lane { (start.0 + end.0) / 2.0 } else { (start.1 + end.1) / 2.0 };
        lanes.sort_by(|a, b| {
            (a - ideal)
                .abs()
                .partial_cmp(&(b - ideal).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        });
        lanes.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        for lane in lanes {
            for &s in &exits_s {
                for &t in &exits_t {
                    let raw = if vertical_lane {
                        vec![start, s, (lane, s.1), (lane, t.1), t, end]
                    } else {
                        vec![start, s, (s.0, lane), (t.0, lane), t, end]
                    };
                    let points = simplify_points(&raw);
                    if points.len() < 2 || !polyline_clear(&points, &avoid, canvas) {
                        continue;
                    }
                    let label_at = label_on_polyline(&points, &avoid);
                    return path_route(path_from_points(&points), true, start, end, label_at, &points);
                }
            }
        }
    }
    fallback(start, end)
}

pub(super) fn push_routed_edge(svg: &mut String, route: &EdgeRoute, stroke: &str, width: &str, extra: &str) {
    match &route.kind {
        EdgeRouteKind::Line => {
            let (x1, y1) = route.start;
            let (x2, y2) = route.end;
            svg.push_str(&format!(
                r##"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="{stroke}" stroke-width="{width}"{extra} data-role="edge"/>"##
            ));
        }
        EdgeRouteKind::Path { d, detour } => {
            let flag = if *detour { r#" data-route="detour""# } else { "" };
            svg.push_str(&format!(
                r##"<path d="{d}" fill="none" stroke="{stroke}" stroke-width="{width}"{extra}{flag} data-role="edge"/>"##
            ));
        }
    }
}

#[cfg(test)]
pub(super) fn sample_path(d: &str) -> Vec<(f32, f32)> {
    let mut samples = Vec::new();
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    let mut cmd = ' ';
    let mut number = String::new();
    let mut nums = Vec::new();
    let flush_num = |number: &mut String, nums: &mut Vec<f32>| {
        if !number.is_empty() {
            if let Ok(value) = number.parse::<f32>() {
                nums.push(value);
            }
            number.clear();
        }
    };
    let apply = |cmd: char, nums: &mut Vec<f32>, cx: &mut f32, cy: &mut f32, samples: &mut Vec<(f32, f32)>| {
        match cmd {
            'M' => {
                if nums.len() >= 2 {
                    *cx = nums[0];
                    *cy = nums[1];
                    samples.push((*cx, *cy));
                }
                let mut i = 2;
                while i + 1 < nums.len() {
                    let next = (nums[i], nums[i + 1]);
                    let dist = ((next.0 - *cx).powi(2) + (next.1 - *cy).powi(2)).sqrt();
                    let steps = ((dist / 4.0).ceil() as i32).max(1);
                    for step in 1..=steps {
                        let t = step as f32 / steps as f32;
                        samples.push((*cx + (next.0 - *cx) * t, *cy + (next.1 - *cy) * t));
                    }
                    *cx = next.0;
                    *cy = next.1;
                    i += 2;
                }
            }
            'L' => {
                let mut i = 0;
                while i + 1 < nums.len() {
                    let next = (nums[i], nums[i + 1]);
                    let dist = ((next.0 - *cx).powi(2) + (next.1 - *cy).powi(2)).sqrt();
                    let steps = ((dist / 4.0).ceil() as i32).max(1);
                    for step in 1..=steps {
                        let t = step as f32 / steps as f32;
                        samples.push((*cx + (next.0 - *cx) * t, *cy + (next.1 - *cy) * t));
                    }
                    *cx = next.0;
                    *cy = next.1;
                    i += 2;
                }
            }
            'C' => {
                let mut i = 0;
                while i + 5 < nums.len() {
                    let p0 = (*cx, *cy);
                    let p1 = (nums[i], nums[i + 1]);
                    let p2 = (nums[i + 2], nums[i + 3]);
                    let p3 = (nums[i + 4], nums[i + 5]);
                    for step in 1..=16 {
                        samples.push(cubic_point(p0, p1, p2, p3, step as f32 / 16.0));
                    }
                    *cx = p3.0;
                    *cy = p3.1;
                    i += 6;
                }
            }
            _ => {}
        }
        nums.clear();
    };
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() {
            flush_num(&mut number, &mut nums);
            apply(cmd, &mut nums, &mut cx, &mut cy, &mut samples);
            cmd = ch;
        } else if ch == ',' || ch.is_whitespace() {
            flush_num(&mut number, &mut nums);
        } else {
            number.push(ch);
        }
    }
    flush_num(&mut number, &mut nums);
    apply(cmd, &mut nums, &mut cx, &mut cy, &mut samples);
    samples
}

#[allow(clippy::too_many_arguments)]
pub(super) fn compute_layered_layout(
    n: usize,
    edges: &[(usize, usize)],
    node_sizes: &[(f32, f32)],
    horizontal: bool,
    reversed: bool,
    margin: f32,
    layer_gap: f32,
    sibling_gap: f32,
) -> (Vec<(f32, f32)>, u32, u32) {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(from, to) in edges {
        if from != to {
            adj[from].push(to);
        }
    }
    let mut in_deg = vec![0usize; n];
    for list in &adj {
        for &v in list {
            in_deg[v] += 1;
        }
    }
    let mut layer = vec![0usize; n];
    let mut state = vec![0u8; n];
    let mut topo = Vec::with_capacity(n);
    fn dfs(u: usize, adj: &[Vec<usize>], state: &mut [u8], topo: &mut Vec<usize>) {
        state[u] = 1;
        for &v in &adj[u] {
            if state[v] == 0 {
                dfs(v, adj, state, topo);
            }
        }
        state[u] = 2;
        topo.push(u);
    }
    for i in 0..n {
        if in_deg[i] == 0 && state[i] == 0 {
            dfs(i, &adj, &mut state, &mut topo);
        }
    }
    for i in 0..n {
        if state[i] == 0 {
            dfs(i, &adj, &mut state, &mut topo);
        }
    }
    topo.reverse();
    let mut rank_pos = vec![0usize; n];
    for (pos, &u) in topo.iter().enumerate() {
        rank_pos[u] = pos;
    }
    for &u in &topo {
        for &v in &adj[u] {
            if rank_pos[u] < rank_pos[v] {
                layer[v] = layer[v].max(layer[u] + 1);
            }
        }
    }

    let max_layer = layer.iter().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); max_layer + 1];
    for (idx, &l) in layer.iter().enumerate() {
        layers[l].push(idx);
    }

    let mut rev_adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(from, to) in edges {
        if from != to {
            rev_adj[to].push(from);
        }
    }
    for _ in 0..2 {
        for l in 1..=max_layer {
            let prev_pos: HashMap<usize, usize> = layers[l - 1]
                .iter()
                .enumerate()
                .map(|(i, &u)| (u, i))
                .collect();
            layers[l].sort_by(|&a, &b| {
                let bary = |u: usize| -> f32 {
                    let mut sum = 0.0;
                    let mut cnt = 0.0;
                    for &p in &rev_adj[u] {
                        if let Some(&pos) = prev_pos.get(&p) {
                            sum += pos as f32;
                            cnt += 1.0;
                        }
                    }
                    if cnt > 0.0 { sum / cnt } else { f32::MAX }
                };
                bary(a)
                    .partial_cmp(&bary(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }

    let mut layer_primary_sizes: Vec<f32> = Vec::with_capacity(layers.len());
    let mut layer_cross_sizes: Vec<f32> = Vec::with_capacity(layers.len());
    for l_nodes in &layers {
        let mut p_max = 32.0f32;
        let mut c_sum = 0.0f32;
        for (i, &u) in l_nodes.iter().enumerate() {
            let (nw, nh) = node_sizes[u];
            let (p, c) = if horizontal { (nw, nh) } else { (nh, nw) };
            p_max = p_max.max(p);
            c_sum += c;
            if i + 1 < l_nodes.len() {
                c_sum += sibling_gap;
            }
        }
        layer_primary_sizes.push(p_max);
        layer_cross_sizes.push(c_sum);
    }

    let max_cross = layer_cross_sizes.iter().copied().fold(0.0f32, f32::max);
    let total_primary: f32 = layer_primary_sizes.iter().sum::<f32>()
        + layer_gap * (layers.len().saturating_sub(1) as f32);

    let mut centers = vec![(0.0f32, 0.0f32); n];
    let mut p_cursor = margin;
    let layer_iter: Vec<usize> = if reversed {
        (0..layers.len()).rev().collect()
    } else {
        (0..layers.len()).collect()
    };
    for l in layer_iter {
        let p_size = layer_primary_sizes[l];
        let c_total = layer_cross_sizes[l];
        let mut c_cursor = margin + (max_cross - c_total) / 2.0;
        for &u in &layers[l] {
            let (nw, nh) = node_sizes[u];
            let (p_mid, c_span) = (p_cursor + p_size / 2.0, if horizontal { nh } else { nw });
            let c_mid = c_cursor + c_span / 2.0;
            centers[u] = if horizontal {
                (p_mid, c_mid)
            } else {
                (c_mid, p_mid)
            };
            c_cursor += c_span + sibling_gap;
        }
        p_cursor += p_size + layer_gap;
    }

    let (total_w, total_h) = if horizontal {
        (
            (total_primary + margin * 2.0).ceil() as u32,
            (max_cross + margin * 2.0).ceil() as u32,
        )
    } else {
        (
            (max_cross + margin * 2.0).ceil() as u32,
            (total_primary + margin * 2.0).ceil() as u32,
        )
    };
    (centers, total_w.max(120), total_h.max(80))
}


/// A titled frame around diagram nodes: class `namespace`s, ER `subgraph`s.
pub(super) struct NodeGroup {
    pub(super) title: String,
    pub(super) parent: Option<usize>,
}

pub(super) const GROUP_SIDE: f32 = 16.0;
pub(super) const GROUP_TOP: f32 = 34.0;

/// A laid-out diagram with groups: node centers, canvas size, and each group's frame
/// `(x, y, w, h)` (`None` for a group without nodes).
pub(super) type GroupedLayout = (Vec<(f32, f32)>, u32, u32, Vec<Option<(f32, f32, f32, f32)>>);

/// Layered layout that keeps every group together: each group is laid out on its own
/// (nested groups first, as blocks), then placed as one block among its siblings.
/// Without groups this is exactly [`compute_layered_layout`].
#[allow(clippy::too_many_arguments)]
pub(super) fn layout_with_groups(
    sizes: &[(f32, f32)],
    edges: &[(usize, usize)],
    member_of: &[Option<usize>],
    groups: &[NodeGroup],
    horizontal: bool,
    reversed: bool,
    margin: f32,
    layer_gap: f32,
    sibling_gap: f32,
) -> GroupedLayout {
    layout_with_group_dirs(sizes, edges, member_of, groups, &[], horizontal, reversed, margin, layer_gap, sibling_gap)
}

/// [`layout_with_groups`] where a group may have its own direction `(horizontal, reversed)`
/// (flowchart `subgraph ... direction LR`); a group without one follows its parent.
#[allow(clippy::too_many_arguments)]
pub(super) fn layout_with_group_dirs(
    sizes: &[(f32, f32)],
    edges: &[(usize, usize)],
    member_of: &[Option<usize>],
    groups: &[NodeGroup],
    dirs: &[Option<(bool, bool)>],
    horizontal: bool,
    reversed: bool,
    margin: f32,
    layer_gap: f32,
    sibling_gap: f32,
) -> GroupedLayout {
    let n = sizes.len();
    // The child of `scope` on the way up from `cur` (`None` when `cur` is outside `scope`).
    let slot_group = |scope: Option<usize>, mut cur: Option<usize>| -> Option<Option<usize>> {
        let mut last = None;
        loop {
            if cur == scope {
                return Some(last);
            }
            let g = cur?;
            last = Some(g);
            cur = groups[g].parent;
        }
    };
    struct Placed {
        centers: Vec<(usize, f32, f32)>,
        frames: Vec<(usize, (f32, f32, f32, f32))>,
        w: f32,
        h: f32,
    }
    #[allow(clippy::too_many_arguments)]
    fn place(
        scope: Option<usize>,
        n: usize,
        sizes: &[(f32, f32)],
        edges: &[(usize, usize)],
        member_of: &[Option<usize>],
        groups: &[NodeGroup],
        dirs: &[Option<(bool, bool)>],
        slot_group: &dyn Fn(Option<usize>, Option<usize>) -> Option<Option<usize>>,
        opts: (bool, bool, f32, f32, f32),
    ) -> Option<Placed> {
        let (horizontal, reversed, margin, layer_gap, sibling_gap) = opts;
        // Slots: direct nodes, and child groups (as laid-out blocks).
        let mut slot_of: HashMap<usize, usize> = HashMap::new();
        let mut slot_sizes: Vec<(f32, f32)> = Vec::new();
        let mut slot_kind: Vec<Result<usize, Placed>> = Vec::new();
        let mut child_slot: HashMap<usize, usize> = HashMap::new();
        for i in 0..n {
            match slot_group(scope, member_of[i]) {
                None => {}
                Some(None) => {
                    slot_of.insert(i, slot_sizes.len());
                    slot_sizes.push(sizes[i]);
                    slot_kind.push(Ok(i));
                }
                Some(Some(child)) => {
                    let slot = match child_slot.get(&child) {
                        Some(&slot) => slot,
                        None => {
                            let (h, r) = dirs.get(child).copied().flatten().unwrap_or((horizontal, reversed));
                            let child_opts = (h, r, margin, layer_gap, sibling_gap);
                            let block = place(Some(child), n, sizes, edges, member_of, groups, dirs, slot_group, child_opts)?;
                            child_slot.insert(child, slot_sizes.len());
                            slot_sizes.push((block.w, block.h));
                            slot_kind.push(Err(block));
                            slot_sizes.len() - 1
                        }
                    };
                    slot_of.insert(i, slot);
                }
            }
        }
        if slot_sizes.is_empty() {
            return None;
        }
        let mut slot_edges: Vec<(usize, usize)> = Vec::new();
        for (a, b) in edges {
            if let (Some(&sa), Some(&sb)) = (slot_of.get(a), slot_of.get(b))
                && sa != sb
                && !slot_edges.contains(&(sa, sb))
            {
                slot_edges.push((sa, sb));
            }
        }
        let side = if scope.is_some() { GROUP_SIDE } else { margin };
        let (centers, w, h) =
            compute_layered_layout(slot_sizes.len(), &slot_edges, &slot_sizes, horizontal, reversed, side, layer_gap, sibling_gap);
        // A group hugs its content: the layout's minimum canvas size is for whole diagrams.
        let (w, h) = if scope.is_some() {
            let right = centers.iter().zip(&slot_sizes).map(|(c, s)| c.0 + s.0 / 2.0).fold(0.0f32, f32::max);
            let bottom = centers.iter().zip(&slot_sizes).map(|(c, s)| c.1 + s.1 / 2.0).fold(0.0f32, f32::max);
            ((right + side).ceil() as u32, (bottom + side).ceil() as u32)
        } else {
            (w, h)
        };
        // A group's title band sits above its content.
        // An untitled group (a state's concurrent region) has no band.
        let extra_top = if scope.is_some_and(|g| !groups[g].title.is_empty()) { GROUP_TOP - GROUP_SIDE } else { 0.0 };
        // A frame is at least as wide as its title.
        let title_w = scope.map_or(0.0, |g| estimate_text_width(&groups[g].title, 11.5) + 24.0);
        let extra_x = ((title_w - w as f32) / 2.0).max(0.0);
        let mut placed = Placed {
            centers: Vec::new(),
            frames: Vec::new(),
            w: w as f32 + extra_x * 2.0,
            h: h as f32 + extra_top,
        };
        for ((cx, cy), kind) in centers.into_iter().zip(slot_kind) {
            let (cx, cy) = (cx + extra_x, cy + extra_top);
            match kind {
                Ok(i) => placed.centers.push((i, cx, cy)),
                Err(block) => {
                    let (ox, oy) = (cx - block.w / 2.0, cy - block.h / 2.0);
                    placed.centers.extend(block.centers.into_iter().map(|(i, x, y)| (i, x + ox, y + oy)));
                    placed.frames.extend(block.frames.into_iter().map(|(g, (x, y, fw, fh))| (g, (x + ox, y + oy, fw, fh))));
                }
            }
        }
        if let Some(g) = scope {
            placed.frames.push((g, (0.0, 0.0, placed.w, placed.h)));
        }
        Some(placed)
    }

    let opts = (horizontal, reversed, margin, layer_gap, sibling_gap);
    let Some(placed) = place(None, n, sizes, edges, member_of, groups, dirs, &slot_group, opts) else {
        return (Vec::new(), 0, 0, vec![None; groups.len()]);
    };
    let mut centers = vec![(0.0f32, 0.0f32); n];
    for (i, x, y) in placed.centers {
        centers[i] = (x, y);
    }
    let mut frames = vec![None; groups.len()];
    for (g, rect) in placed.frames {
        frames[g] = Some(rect);
    }
    (centers, placed.w as u32, placed.h as u32, frames)
}

/// Frames for every group, outermost first.
pub(super) fn push_group_frames(svg: &mut String, groups: &[NodeGroup], frames: &[Option<(f32, f32, f32, f32)>], role: &str) {
    let depth = |mut g: usize| {
        let mut d = 0;
        while let Some(p) = groups[g].parent {
            d += 1;
            g = p;
        }
        d
    };
    let mut order: Vec<usize> = (0..groups.len()).collect();
    order.sort_by_key(|&g| depth(g));
    for g in order {
        let Some((fx, fy, fw, fh)) = frames[g] else { continue };
        svg.push_str(&format!(
            r##"<rect x="{fx:.1}" y="{fy:.1}" width="{fw:.1}" height="{fh:.1}" rx="6" fill="#f8fafc" fill-opacity="0.6" stroke="#94a3b8" stroke-width="1.1" stroke-dasharray="5,3" data-role="{role}"/>"##
        ));
        if !groups[g].title.is_empty() {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" font-size="11.5" font-weight="600" fill="#475569">{}</text>"##,
                fx + 10.0,
                fy + 16.0,
                label_markup(&groups[g].title, false)
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layered_layout_sibling_gap_uses_each_node_width() {
        let sizes = [(200.0f32, 40.0), (80.0, 40.0)];
        let (centers, total_w, total_h) = compute_layered_layout(2, &[], &sizes, false, false, 32.0, 64.0, 34.0);
        let (x0, y0) = centers[0];
        let (x1, y1) = centers[1];
        assert!(x0 < x1, "insertion order keeps the wider node on the left");
        assert!((y0 - y1).abs() < 0.01, "same layer shares a center line");
        let gap = (x1 - 40.0) - (x0 + 100.0);
        assert!((gap - 34.0).abs() < 0.01, "gap {gap} should be the sibling gap");
        assert!(x0 - 100.0 >= 0.0 && x1 + 40.0 <= total_w as f32 + 0.01);
        assert!(y0 - 20.0 >= 0.0 && y0 + 20.0 <= total_h as f32 + 0.01);
    }
}
