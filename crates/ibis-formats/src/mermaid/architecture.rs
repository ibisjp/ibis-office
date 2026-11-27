// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `architecture-beta`: services and junctions on a grid derived from the edge sides
//! (`a:R -- L:b` puts `b` right of `a`), `align row|column`, and group frames.

use super::*;

const PITCH_X: f32 = 170.0;
const PITCH_Y: f32 = 170.0;
const ICON: f32 = 56.0;
const MARGIN: f32 = 24.0;
const GROUP_PAD: f32 = 18.0;
const GROUP_TITLE: f32 = 26.0;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ArchNode {
    pub(super) id: String,
    /// `None` for a junction.
    pub(super) icon: Option<String>,
    pub(super) label: String,
    pub(super) group: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ArchGroup {
    pub(super) id: String,
    pub(super) icon: Option<String>,
    pub(super) label: String,
    pub(super) parent: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ArchEdge {
    pub(super) from: usize,
    pub(super) from_side: char,
    pub(super) to: usize,
    pub(super) to_side: char,
    pub(super) arrow_start: bool,
    pub(super) arrow_end: bool,
    pub(super) label: Option<String>,
}

#[derive(Debug, Default)]
pub(super) struct ArchDiagram {
    pub(super) title: Option<String>,
    pub(super) nodes: Vec<ArchNode>,
    pub(super) groups: Vec<ArchGroup>,
    pub(super) edges: Vec<ArchEdge>,
    /// (`row`?, members).
    pub(super) aligns: Vec<(bool, Vec<usize>)>,
}

/// `id(icon)[Title] in parent` → (id, icon, title, parent).
fn parse_decl(rest: &str) -> (String, Option<String>, Option<String>, Option<String>) {
    let rest = rest.trim();
    let (head, parent) = match rest.rfind(" in ") {
        Some(at) if !rest[at..].contains(']') => (rest[..at].trim(), Some(rest[at + 4..].trim().to_string())),
        _ => (rest, None),
    };
    let id_end = head.find(['(', '[']).unwrap_or(head.len());
    let id = head[..id_end].trim().to_string();
    let mut icon = None;
    let mut title = None;
    let mut tail = &head[id_end..];
    if let Some(inner) = tail.strip_prefix('(')
        && let Some(close) = inner.find(')')
    {
        icon = Some(inner[..close].trim().to_string());
        tail = &inner[close + 1..];
    }
    if let Some(inner) = tail.trim().strip_prefix('[')
        && let Some(close) = inner.rfind(']')
    {
        title = Some(unquote(inner[..close].trim()));
    }
    (id, icon, title, parent)
}

/// `db:L` / `L:server` / `server{group}:B`: (id, side).
fn split_end(token: &str, side_first: bool) -> Option<(String, char)> {
    let token = token.trim();
    let (a, b) = token.split_once(':')?;
    let (id, side) = if side_first { (b, a) } else { (a, b) };
    let side = side.trim().chars().next()?.to_ascii_uppercase();
    matches!(side, 'L' | 'R' | 'T' | 'B').then(|| (id.trim().trim_end_matches("{group}").trim().to_string(), side))
}

/// `a:R --> L:b`, `a:R <-- L:b`, `a:R -- L:b`, `a:R -[label]- L:b`.
fn parse_arch_edge(line: &str) -> Option<(String, char, String, char, bool, bool, Option<String>)> {
    let line = line.trim();
    let mut label = None;
    let (lhs, link, rhs) = if let Some(open) = line.find("-[")
        && let Some(close) = line[open..].find("]-").map(|i| open + i)
    {
        label = Some(unquote(line[open + 2..close].trim()));
        let lhs_end = line[..open].trim_end_matches('<').len();
        let rhs_start = close + 2 + line[close + 2..].len() - line[close + 2..].trim_start_matches('>').len();
        (&line[..lhs_end], &line[lhs_end..rhs_start], &line[rhs_start..])
    } else {
        let start = line.find(['<', '-'])?;
        let len = line[start..].bytes().take_while(|b| matches!(b, b'<' | b'-' | b'>')).count();
        (&line[..start], &line[start..start + len], &line[start + len..])
    };
    if !link.contains('-') {
        return None;
    }
    let (from, fs) = split_end(lhs, false)?;
    let (to, ts) = split_end(rhs, true)?;
    Some((from, fs, to, ts, link.starts_with('<'), link.ends_with('>'), label))
}

pub(super) fn parse_architecture(source: &str) -> ArchDiagram {
    let mut d = ArchDiagram::default();
    let mut pending_parent: Vec<(usize, String)> = Vec::new();
    let mut pending_group: Vec<(usize, String)> = Vec::new();
    for raw in source.lines().skip(1) {
        let l = raw.split_once("%%").map_or(raw, |(code, _)| code).trim();
        if l.is_empty() {
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ") {
            d.title = Some(unquote(rest.trim()));
        } else if let Some(rest) = l.strip_prefix("group ") {
            let (id, icon, title, parent) = parse_decl(rest);
            if let Some(p) = parent {
                pending_parent.push((d.groups.len(), p));
            }
            d.groups.push(ArchGroup { label: title.unwrap_or_default(), id, icon, parent: None });
        } else if let Some(rest) = l.strip_prefix("service ") {
            let (id, icon, title, parent) = parse_decl(rest);
            if let Some(p) = parent {
                pending_group.push((d.nodes.len(), p));
            }
            d.nodes.push(ArchNode { label: title.unwrap_or_default(), id, icon: Some(icon.unwrap_or_default()), group: None });
        } else if let Some(rest) = l.strip_prefix("junction ") {
            let (id, _, _, parent) = parse_decl(rest);
            if let Some(p) = parent {
                pending_group.push((d.nodes.len(), p));
            }
            d.nodes.push(ArchNode { id, icon: None, label: String::new(), group: None });
        } else if let Some(rest) = l.strip_prefix("align ") {
            let mut words = rest.split_whitespace();
            let row = match words.next() {
                Some("row") => true,
                Some("column") => false,
                _ => continue,
            };
            let members: Vec<usize> = words.filter_map(|w| d.nodes.iter().position(|n| n.id == w)).collect();
            d.aligns.push((row, members));
        } else if let Some((from, fs, to, ts, start, end, label)) = parse_arch_edge(l) {
            let (Some(from), Some(to)) = (d.nodes.iter().position(|n| n.id == from), d.nodes.iter().position(|n| n.id == to)) else {
                continue;
            };
            d.edges.push(ArchEdge { from, from_side: fs, to, to_side: ts, arrow_start: start, arrow_end: end, label });
        }
    }
    for (g, p) in pending_parent {
        d.groups[g].parent = d.groups.iter().position(|x| x.id == p).filter(|&x| x != g);
    }
    for (n, p) in pending_group {
        d.nodes[n].group = d.groups.iter().position(|x| x.id == p);
    }
    d
}

fn side_delta(side: char) -> (i32, i32) {
    match side {
        'L' => (-1, 0),
        'R' => (1, 0),
        'T' => (0, -1),
        _ => (0, 1),
    }
}

/// Grid cells: each edge puts its far end one cell beyond the near end's side; a taken cell
/// moves the node sideways (perpendicular to the edge), so fans spread out. `align` lines
/// then share a row or column where the cell is free.
pub(super) fn place_architecture(d: &ArchDiagram) -> Vec<(i32, i32)> {
    let n = d.nodes.len();
    let mut pos: Vec<Option<(i32, i32)>> = vec![None; n];
    let mut taken: HashMap<(i32, i32), usize> = HashMap::new();
    let free_near = |taken: &HashMap<(i32, i32), usize>, (x, y): (i32, i32), (dx, dy): (i32, i32)| -> (i32, i32) {
        if !taken.contains_key(&(x, y)) {
            return (x, y);
        }
        let (px, py) = (dy.abs(), dx.abs());
        // Declaration order runs right / down first, so `a b c` fanning into one node stay in order.
        for k in 1..64 {
            let c = (x + px * k, y + py * k);
            if !taken.contains_key(&c) {
                return c;
            }
        }
        (x + dx * 64, y + dy * 64)
    };
    for seed in 0..n {
        if pos[seed].is_some() {
            continue;
        }
        // A new connected part starts in the first free cell of the top row.
        let top = taken.keys().map(|c| c.1).min().unwrap_or(0);
        let left = taken.keys().map(|c| c.0).min().unwrap_or(0);
        let start = free_near(&taken, (left, top), (0, 1));
        pos[seed] = Some(start);
        taken.insert(start, seed);
        let mut queue = std::collections::VecDeque::from([seed]);
        while let Some(cur) = queue.pop_front() {
            for e in &d.edges {
                let (other, side) = if e.from == cur {
                    (e.to, e.from_side)
                } else if e.to == cur {
                    (e.from, e.to_side)
                } else {
                    continue;
                };
                if pos[other].is_some() {
                    continue;
                }
                let (cx, cy) = pos[cur].unwrap();
                let delta = side_delta(side);
                let cell = free_near(&taken, (cx + delta.0, cy + delta.1), delta);
                pos[other] = Some(cell);
                taken.insert(cell, other);
                queue.push_back(other);
            }
        }
    }
    let mut pos: Vec<(i32, i32)> = pos.into_iter().map(|p| p.unwrap_or((0, 0))).collect();
    for (row, members) in &d.aligns {
        let Some(&first) = members.first() else { continue };
        for &m in &members[1..] {
            let target = if *row { (pos[m].0, pos[first].1) } else { (pos[first].0, pos[m].1) };
            if target == pos[m] || taken.get(&target).is_some_and(|&o| o != m) {
                continue;
            }
            taken.remove(&pos[m]);
            pos[m] = target;
            taken.insert(target, m);
        }
    }
    // Drop empty columns and rows (left behind by `align`).
    let mut xs: Vec<i32> = pos.iter().map(|p| p.0).collect();
    let mut ys: Vec<i32> = pos.iter().map(|p| p.1).collect();
    xs.sort_unstable();
    xs.dedup();
    ys.sort_unstable();
    ys.dedup();
    let rank = |v: &[i32], x: i32| v.binary_search(&x).unwrap_or(0) as i32;
    pos.iter().map(|p| (rank(&xs, p.0), rank(&ys, p.1))).collect()
}

/// A white pictogram on the service's blue square.
fn push_arch_icon(svg: &mut String, icon: &str, x: f32, y: f32, size: f32) {
    let s = size / 56.0;
    let p = |vx: f32, vy: f32| (x + vx * s, y + vy * s);
    svg.push_str(&format!(
        r##"<rect x="{x:.1}" y="{y:.1}" width="{size:.1}" height="{size:.1}" rx="{:.1}" fill="#087ebf" data-icon="{}"/>"##,
        4.0 * s,
        escape_attr(icon)
    ));
    let stroke = format!(r##" fill="none" stroke="#ffffff" stroke-width="{:.1}""##, 2.2 * s);
    match icon {
        "server" => {
            for row in 0..3 {
                let (rx, ry) = p(12.0, 11.0 + row as f32 * 12.0);
                svg.push_str(&format!(r##"<rect x="{rx:.1}" y="{ry:.1}" width="{:.1}" height="{:.1}" rx="{:.1}"{stroke}/>"##, 32.0 * s, 9.0 * s, 1.5 * s));
                let (dx, dy) = p(38.0, 15.5 + row as f32 * 12.0);
                svg.push_str(&format!(r##"<circle cx="{dx:.1}" cy="{dy:.1}" r="{:.1}" fill="#ffffff"/>"##, 1.6 * s));
            }
        }
        "database" => {
            let (cx, top) = p(28.0, 15.0);
            let (_, bottom) = p(28.0, 41.0);
            let (rx, ry) = (16.0 * s, 5.0 * s);
            svg.push_str(&format!(r##"<ellipse cx="{cx:.1}" cy="{top:.1}" rx="{rx:.1}" ry="{ry:.1}"{stroke}/>"##));
            svg.push_str(&format!(
                r##"<path d="M {:.1} {top:.1} V {bottom:.1} A {rx:.1} {ry:.1} 0 0 0 {:.1} {bottom:.1} V {top:.1} M {:.1} {:.1} A {rx:.1} {ry:.1} 0 0 0 {:.1} {:.1}"{stroke}/>"##,
                cx - rx,
                cx + rx,
                cx - rx,
                (top + bottom) / 2.0,
                cx + rx,
                (top + bottom) / 2.0
            ));
        }
        "disk" => {
            let (rx, ry) = p(13.0, 10.0);
            svg.push_str(&format!(r##"<rect x="{rx:.1}" y="{ry:.1}" width="{:.1}" height="{:.1}" rx="{:.1}"{stroke}/>"##, 30.0 * s, 36.0 * s, 3.0 * s));
            let (cx, cy) = p(28.0, 25.0);
            svg.push_str(&format!(r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{:.1}"{stroke}/>"##, 9.0 * s));
            svg.push_str(&format!(r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{:.1}" fill="#ffffff"/>"##, 2.0 * s));
            let (lx, ly) = p(18.0, 40.0);
            svg.push_str(&format!(r##"<path d="M {lx:.1} {ly:.1} h {:.1}"{stroke}/>"##, 6.0 * s));
        }
        "cloud" => {
            let (sx, sy) = p(15.0, 38.0);
            svg.push_str(&format!(
                r##"<path d="M {sx:.1} {sy:.1} a {r1:.1} {r1:.1} 0 0 1 {:.1} {:.1} a {r2:.1} {r2:.1} 0 0 1 {:.1} {:.1} a {r3:.1} {r3:.1} 0 0 1 {:.1} {:.1} z"{stroke}/>"##,
                1.0 * s,
                -14.0 * s,
                19.0 * s,
                -3.0 * s,
                6.0 * s,
                17.0 * s,
                r1 = 7.5 * s,
                r2 = 10.0 * s,
                r3 = 8.0 * s
            ));
        }
        "internet" => {
            let (cx, cy) = p(28.0, 28.0);
            let r = 16.0 * s;
            svg.push_str(&format!(r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}"{stroke}/>"##));
            svg.push_str(&format!(r##"<ellipse cx="{cx:.1}" cy="{cy:.1}" rx="{:.1}" ry="{r:.1}"{stroke}/>"##, 7.0 * s));
            svg.push_str(&format!(
                r##"<path d="M {:.1} {cy:.1} H {:.1} M {:.1} {:.1} H {:.1} M {:.1} {:.1} H {:.1}"{stroke}/>"##,
                cx - r,
                cx + r,
                cx - r * 0.85,
                cy - r * 0.5,
                cx + r * 0.85,
                cx - r * 0.85,
                cy + r * 0.5,
                cx + r * 0.85
            ));
        }
        _ => {
            // An icon from a pack that is not bundled (`logos:aws-s3`): mermaid's `?` box.
            let (cx, cy) = p(28.0, 38.0);
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{cy:.1}" text-anchor="middle" font-size="{:.1}" font-weight="bold" fill="#ffffff">?</text>"##,
                30.0 * s
            ));
        }
    }
}

fn label_height(label: &str) -> f32 {
    if label.is_empty() { 0.0 } else { label_lines(label, 140.0, 13.0).len() as f32 * 16.0 + 6.0 }
}

pub(super) fn render_architecture_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let d = parse_architecture(source);
    if d.nodes.is_empty() {
        return None;
    }
    let cells = place_architecture(&d);
    // Group depth, for nested frames.
    let depth_of = |mut g: usize| {
        let mut depth = 0;
        while let Some(p) = d.groups[g].parent {
            depth += 1;
            g = p;
            if depth > d.groups.len() {
                break;
            }
        }
        depth
    };
    let max_depth = (0..d.groups.len()).map(|g| depth_of(g) + 1).max().unwrap_or(0) as f32;
    let inset = max_depth * (GROUP_PAD + 4.0) + GROUP_TITLE * max_depth.min(1.0);
    let origin_x = MARGIN + inset + 70.0 - ICON / 2.0;
    let origin_y = MARGIN + inset + 4.0;
    // Icon box of each node (a junction is a dot in the middle of its cell).
    let boxes: Vec<RectF> = d
        .nodes
        .iter()
        .zip(&cells)
        .map(|(node, &(gx, gy))| {
            let (x, y) = (origin_x + gx as f32 * PITCH_X, origin_y + gy as f32 * PITCH_Y);
            if node.icon.is_some() { RectF { x, y, w: ICON, h: ICON } } else { RectF { x: x + ICON / 2.0 - 4.0, y: y + ICON / 2.0 - 4.0, w: 8.0, h: 8.0 } }
        })
        .collect();
    // Each group frames its own services and its child groups.
    let mut frames: Vec<Option<RectF>> = vec![None; d.groups.len()];
    let mut order: Vec<usize> = (0..d.groups.len()).collect();
    order.sort_by_key(|&g| std::cmp::Reverse(depth_of(g)));
    for &g in &order {
        let mut ext: Option<(f32, f32, f32, f32)> = None;
        let mut grow = |x0: f32, y0: f32, x1: f32, y1: f32| {
            ext = Some(ext.map_or((x0, y0, x1, y1), |e| (e.0.min(x0), e.1.min(y0), e.2.max(x1), e.3.max(y1))));
        };
        for (i, node) in d.nodes.iter().enumerate() {
            if node.group == Some(g) {
                let b = boxes[i];
                let lw = label_lines(&node.label, 140.0, 13.0).iter().map(|l| estimate_text_width(l, 13.0)).fold(0.0f32, f32::max);
                let cx = b.x + b.w / 2.0;
                grow((cx - lw / 2.0).min(b.x), b.y, (cx + lw / 2.0).max(b.x + b.w), b.y + b.h + label_height(&node.label));
            }
        }
        for (c, child) in d.groups.iter().enumerate() {
            if child.parent == Some(g)
                && let Some(f) = frames[c]
            {
                grow(f.x, f.y, f.x + f.w, f.y + f.h);
            }
        }
        if let Some((x0, y0, x1, y1)) = ext {
            frames[g] = Some(RectF { x: x0 - GROUP_PAD, y: y0 - GROUP_PAD - GROUP_TITLE, w: x1 - x0 + GROUP_PAD * 2.0, h: y1 - y0 + GROUP_PAD * 2.0 + GROUP_TITLE });
        }
    }
    let mut max_x = boxes.iter().map(|b| b.x + b.w + 60.0).fold(0.0f32, f32::max);
    let mut max_y = d.nodes.iter().zip(&boxes).map(|(n, b)| b.y + b.h + label_height(&n.label)).fold(0.0f32, f32::max);
    for f in frames.iter().flatten() {
        max_x = max_x.max(f.x + f.w);
        max_y = max_y.max(f.y + f.h);
    }
    let total_w = (max_x + MARGIN).ceil();
    let total_h = (max_y + MARGIN).ceil();
    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));
    svg.push_str(
        r##"<defs><marker id="arch-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#475569"/></marker></defs>"##,
    );
    for &g in order.iter().rev() {
        let Some(f) = frames[g] else { continue };
        let group = &d.groups[g];
        svg.push_str(&format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="4" fill="none" stroke="#64748b" stroke-width="1.4" stroke-dasharray="8,4" data-group="{}"/>"##,
            f.x,
            f.y,
            f.w,
            f.h,
            escape_attr(&group.id)
        ));
        let mut tx = f.x + 8.0;
        if let Some(icon) = &group.icon {
            push_arch_icon(&mut svg, icon, tx, f.y + 5.0, 18.0);
            tx += 24.0;
        }
        svg.push_str(&format!(
            r##"<text x="{tx:.1}" y="{:.1}" font-size="13" font-weight="bold" fill="#0f172a">{}</text>"##,
            f.y + 19.0,
            escape_xml(&group.label)
        ));
    }
    // Where an edge leaves a node on `side`: the bottom side starts below the label.
    let anchor = |i: usize, side: char| -> (f32, f32) {
        let b = boxes[i];
        let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
        match side {
            'L' => (b.x, cy),
            'R' => (b.x + b.w, cy),
            'T' => (cx, b.y),
            _ => (cx, b.y + b.h + label_height(&d.nodes[i].label)),
        }
    };
    let mut labels = String::new();
    for e in &d.edges {
        let (sx, sy) = anchor(e.from, e.from_side);
        let (ex, ey) = anchor(e.to, e.to_side);
        let horizontal = |s: char| matches!(s, 'L' | 'R');
        let path = match (horizontal(e.from_side), horizontal(e.to_side)) {
            (true, true) => {
                let mx = (sx + ex) / 2.0;
                format!("M {sx:.1} {sy:.1} H {mx:.1} V {ey:.1} H {ex:.1}")
            }
            (false, false) => {
                let my = (sy + ey) / 2.0;
                format!("M {sx:.1} {sy:.1} V {my:.1} H {ex:.1} V {ey:.1}")
            }
            (true, false) => format!("M {sx:.1} {sy:.1} H {ex:.1} V {ey:.1}"),
            (false, true) => format!("M {sx:.1} {sy:.1} V {ey:.1} H {ex:.1}"),
        };
        let start = if e.arrow_start { r#" marker-start="url(#arch-arrow)""# } else { "" };
        let end = if e.arrow_end { r#" marker-end="url(#arch-arrow)""# } else { "" };
        svg.push_str(&format!(
            r##"<path d="{path}" fill="none" stroke="#475569" stroke-width="2"{start}{end} data-edge="{}-{}"/>"##,
            escape_attr(&d.nodes[e.from].id),
            escape_attr(&d.nodes[e.to].id)
        ));
        if let Some(label) = e.label.as_deref().filter(|l| !l.is_empty()) {
            let (lw, lh) = flow_edge_label_size(label);
            let (mx, my) = ((sx + ex) / 2.0, (sy + ey) / 2.0);
            labels.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{lw:.1}" height="{lh:.1}" rx="3" fill="#ffffff" opacity="0.9"/>"##,
                mx - lw / 2.0,
                my - lh / 2.0
            ));
            push_centered_label(&mut labels, mx, my, label, 11.0, 140.0, "#334155", "normal");
        }
    }
    for (i, node) in d.nodes.iter().enumerate() {
        let b = boxes[i];
        match &node.icon {
            None => svg.push_str(&format!(
                r##"<circle cx="{:.1}" cy="{:.1}" r="4" fill="#475569" data-junction="{}"/>"##,
                b.x + 4.0,
                b.y + 4.0,
                escape_attr(&node.id)
            )),
            Some(icon) => {
                push_arch_icon(&mut svg, icon, b.x, b.y, ICON);
                if !node.label.is_empty() {
                    let lines = label_lines(&node.label, 140.0, 13.0);
                    let mut y = b.y + b.h + 16.0;
                    for line in lines {
                        svg.push_str(&format!(
                            r##"<text x="{:.1}" y="{y:.1}" text-anchor="middle" font-size="13" fill="#0f172a">{}</text>"##,
                            b.x + b.w / 2.0,
                            escape_xml(&line)
                        ));
                        y += 16.0;
                    }
                }
            }
        }
    }
    svg.push_str(&labels);
    svg.push_str("</svg>");
    let bytes = svg.into_bytes();
    let (bytes, w, h) = match &d.title {
        Some(title) => add_svg_title(bytes, total_w as u32, total_h as u32, title),
        None => (bytes, total_w as u32, total_h as u32),
    };
    Some((bytes, w, h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn architecture_declarations_and_edges_parse() {
        let d = parse_architecture(
            "architecture-beta\n  group api(cloud)[API]\n  group inner[Inner] in api\n  service db(database)[My DB] in inner\n  service s(server)[Server]\n  junction j in api\n  db:L -- R:s\n  s:B --> T:j\n  j:L <-- R:db\n  db:T -[reads]- B:s\n  align row db s",
        );
        assert_eq!(d.groups[1].parent, Some(0));
        assert_eq!(d.nodes[0], ArchNode { id: "db".into(), icon: Some("database".into()), label: "My DB".into(), group: Some(1) });
        assert_eq!((d.nodes[2].icon.clone(), d.nodes[2].group), (None, Some(0)));
        assert_eq!(d.edges.len(), 4);
        assert_eq!((d.edges[0].from_side, d.edges[0].to_side, d.edges[0].arrow_end), ('L', 'R', false));
        assert!(d.edges[1].arrow_end && !d.edges[1].arrow_start);
        assert!(d.edges[2].arrow_start && !d.edges[2].arrow_end);
        assert_eq!(d.edges[3].label.as_deref(), Some("reads"));
        assert_eq!(d.aligns, vec![(true, vec![0, 1])]);
    }

    #[test]
    fn architecture_sides_place_neighbours_and_fans_spread() {
        // `db:L -- R:server`: the server is left of the database; `disk1:T -- B:server`: below it.
        let d = parse_architecture(
            "architecture-beta\n  service db(database)[Database]\n  service server(server)[Server]\n  service disk1(disk)[Storage]\n  db:L -- R:server\n  disk1:T -- B:server",
        );
        let p = place_architecture(&d);
        assert_eq!(p[1].0 + 1, p[0].0);
        assert_eq!(p[1].1, p[0].1);
        assert_eq!((p[2].0, p[2].1), (p[1].0, p[1].1 + 1));
        // Three sources above one processor end on distinct cells of one row.
        let d = parse_architecture(
            "architecture-beta\n  service a(server)[A]\n  service b(server)[B]\n  service c(server)[C]\n  service p(server)[P]\n  a:B --> T:p\n  b:B --> T:p\n  c:B --> T:p\n  align row a b c",
        );
        let p = place_architecture(&d);
        assert!(p[0].1 == p[1].1 && p[1].1 == p[2].1 && p[3].1 == p[0].1 + 1);
        assert!(p[0] != p[1] && p[1] != p[2] && p[0] != p[2]);
    }

    #[test]
    fn architecture_svg_icons_groups_and_junctions() {
        let svg = svg_of(
            "architecture-beta\n  group api(cloud)[API]\n  service db(database)[Database] in api\n  service x(logos:aws-s3)[S3] in api\n  junction j\n  db:R --> L:j\n  x:T -- B:db",
        );
        assert!(svg.contains(r#"data-group="api""#) && svg.contains(">API</text>"));
        assert!(svg.contains(r#"data-icon="database""#) && svg.contains(r#"data-icon="logos:aws-s3""#) && svg.contains(">?</text>"));
        assert!(svg.contains(r#"data-junction="j""#));
        assert!(svg.contains(r#"marker-end="url(#arch-arrow)" data-edge="db-j""#));
        // The group frame holds both services.
        let frame = tag_with(&svg, r#"data-group="api""#)[0];
        let (fx, fy) = (attr_f32(frame, "x").unwrap(), attr_f32(frame, "y").unwrap());
        let (fw, fh) = (attr_f32(frame, "width").unwrap(), attr_f32(frame, "height").unwrap());
        for icon in [r#"data-icon="database""#, r#"data-icon="logos:aws-s3""#] {
            let tag = tag_with(&svg, icon)[0];
            let (x, y) = (attr_f32(tag, "x").unwrap(), attr_f32(tag, "y").unwrap());
            assert!(x > fx && y > fy && x + ICON < fx + fw && y + ICON < fy + fh);
        }
    }
}
