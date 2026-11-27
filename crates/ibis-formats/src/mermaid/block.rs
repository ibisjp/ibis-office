// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `block` / `block-beta`: blocks placed on a grid of `columns`, with nested blocks,
//! spaces, block arrows and edges drawn straight between the blocks.

use super::*;

const GAP: f32 = 10.0;
const PAD: f32 = 10.0;
const MARGIN: f32 = 20.0;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum BlockShape {
    Flow(NodeShape),
    /// `id<["label"]>(right)`: the directions its heads point to.
    Arrow(Vec<char>),
}

#[derive(Debug, Clone)]
pub(super) struct BlockNode {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) shape: BlockShape,
    pub(super) classes: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum BlockItem {
    Node(usize, u32),
    Space(u32),
    Group(usize, u32),
}

#[derive(Debug, Clone, Default)]
pub(super) struct BlockGroup {
    pub(super) id: Option<String>,
    /// `None`: `columns auto`, one row.
    pub(super) columns: Option<u32>,
    pub(super) items: Vec<BlockItem>,
}

#[derive(Debug, Clone)]
pub(super) struct BlockEdge {
    pub(super) from: String,
    pub(super) to: String,
    pub(super) tok: EdgeTok,
}

#[derive(Debug, Default)]
pub(super) struct BlockDiagram {
    pub(super) nodes: Vec<BlockNode>,
    /// `groups[0]` is the whole diagram.
    pub(super) groups: Vec<BlockGroup>,
    pub(super) edges: Vec<BlockEdge>,
    pub(super) class_defs: HashMap<String, FlowStyle>,
    pub(super) styles: Vec<(String, FlowStyle)>,
}

/// Split a line on whitespace outside quotes and brackets.
fn block_tokens(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote = false;
    let mut start: Option<usize> = None;
    for (i, ch) in line.char_indices() {
        if ch == '"' {
            quote = !quote;
        } else if !quote {
            match ch {
                '[' | '(' | '{' => depth += 1,
                ']' | ')' | '}' => depth = (depth - 1).max(0),
                _ => {}
            }
        }
        if ch.is_whitespace() && depth == 0 && !quote {
            if let Some(s) = start.take() {
                out.push(&line[s..i]);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        out.push(&line[s..]);
    }
    out
}

/// `b:2` → (`b`, 2). A colon inside a label is not a span.
fn split_span(token: &str) -> (&str, u32) {
    if let Some((head, n)) = token.rsplit_once(':')
        && !head.is_empty()
        && !n.is_empty()
        && n.bytes().all(|b| b.is_ascii_digit())
        && !head.ends_with(':')
    {
        return (head, n.parse::<u32>().unwrap_or(1).max(1));
    }
    (token, 1)
}

fn clean_label(label: &str) -> String {
    let label = label.replace("&nbsp;", " ");
    if label.trim().is_empty() { String::new() } else { label }
}

/// `id`, `id["x"]`, `id((("x")))`, `id<["x"]>(right)` → (id, label, shape).
pub(super) fn parse_block_node(token: &str) -> (String, Option<String>, Option<BlockShape>) {
    let t = token.trim();
    if let Some(open) = t.find("<[")
        && let Some(close) = t.rfind("]>(")
        && close > open
        && t.ends_with(')')
    {
        let id = t[..open].trim().to_string();
        let label = clean_label(&unquote(t[open + 2..close].trim()));
        let dirs = &t[close + 3..t.len() - 1];
        let mut heads = Vec::new();
        for d in dirs.split(',').map(|d| d.trim().to_ascii_lowercase()) {
            let add: &[char] = match d.as_str() {
                "right" => &['R'],
                "left" => &['L'],
                "up" => &['U'],
                "down" => &['D'],
                "x" => &['L', 'R'],
                "y" => &['U', 'D'],
                _ => &[],
            };
            for c in add {
                if !heads.contains(c) {
                    heads.push(*c);
                }
            }
        }
        if heads.is_empty() {
            heads.push('R');
        }
        return (id, Some(label), Some(BlockShape::Arrow(heads)));
    }
    if let Some(open) = t.find("(((")
        && t.ends_with(")))")
        && t.len() >= open + 6
    {
        let label = clean_label(&unquote(t[open + 3..t.len() - 3].trim()));
        return (t[..open].trim().to_string(), Some(label), Some(BlockShape::Flow(NodeShape::Ext(ExtShape::DoubleCircle))));
    }
    let (id, label, shape) = parse_node_token(t);
    (id.to_string(), label.map(|l| clean_label(&l)), shape.map(BlockShape::Flow))
}

fn ensure_block(d: &mut BlockDiagram, map: &mut HashMap<String, usize>, token: &str) -> (usize, bool) {
    let (id, label, shape) = parse_block_node(token);
    if let Some(&idx) = map.get(&id) {
        if let Some(label) = label {
            d.nodes[idx].label = label;
        }
        if let Some(shape) = shape {
            d.nodes[idx].shape = shape;
        }
        return (idx, false);
    }
    let idx = d.nodes.len();
    d.nodes.push(BlockNode {
        label: label.unwrap_or_else(|| id.clone()),
        id: id.clone(),
        shape: shape.unwrap_or(BlockShape::Flow(NodeShape::Rect)),
        classes: Vec::new(),
    });
    map.insert(id, idx);
    (idx, true)
}

fn is_group(d: &BlockDiagram, token: &str) -> bool {
    d.groups.iter().any(|g| g.id.as_deref() == Some(token.trim()))
}

pub(super) fn parse_block(source: &str) -> BlockDiagram {
    let mut d = BlockDiagram { groups: vec![BlockGroup::default()], ..Default::default() };
    let mut map: HashMap<String, usize> = HashMap::new();
    let mut stack = vec![0usize];
    for raw in source.lines().skip(1) {
        let line = raw.split_once("%%").map_or(raw, |(code, _)| code).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("classDef ") {
            let rest = rest.trim().trim_end_matches(';');
            let (names, props) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let style = parse_flow_style(props);
            for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                d.class_defs.entry(name.to_string()).or_default().merge(&style);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("style ") {
            if let Some((ids, props)) = rest.trim().trim_end_matches(';').split_once(char::is_whitespace) {
                let style = parse_flow_style(props);
                for id in ids.split(',').map(str::trim).filter(|id| !id.is_empty()) {
                    d.styles.push((id.to_string(), style.clone()));
                }
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("class ") {
            if let Some((ids, class)) = rest.trim().trim_end_matches(';').rsplit_once(char::is_whitespace) {
                for id in ids.split(',').map(str::trim).filter(|id| !id.is_empty()) {
                    if let Some(&idx) = map.get(id) {
                        d.nodes[idx].classes.push(class.trim().to_string());
                    }
                }
            }
            continue;
        }
        if split_next_flowchart_edge(line).is_some() {
            // `A --> B`, `A-- "X" -->B`, `id1("Start")-->id2("Stop")`: the ends keep their
            // grid cell; a block first named in an edge joins the current grid.
            let mut rest = line;
            let mut prev: Option<String> = None;
            while let Some((lhs, tok, after)) = split_next_flowchart_edge(rest) {
                let from = match prev.take() {
                    Some(p) => p,
                    None if is_group(&d, lhs) => lhs.trim().to_string(),
                    None => {
                        let (idx, new) = ensure_block(&mut d, &mut map, lhs);
                        if new {
                            let g = *stack.last().unwrap();
                            d.groups[g].items.push(BlockItem::Node(idx, 1));
                        }
                        d.nodes[idx].id.clone()
                    }
                };
                let rhs = split_next_flowchart_edge(after).map_or(after, |(l, _, _)| l).trim();
                let to = if is_group(&d, rhs) {
                    rhs.to_string()
                } else {
                    let (idx, new) = ensure_block(&mut d, &mut map, rhs);
                    if new {
                        let g = *stack.last().unwrap();
                        d.groups[g].items.push(BlockItem::Node(idx, 1));
                    }
                    d.nodes[idx].id.clone()
                };
                d.edges.push(BlockEdge { from, to: to.clone(), tok });
                prev = Some(to);
                rest = after.trim_start();
                rest = rest.strip_prefix(rhs).unwrap_or(rest);
            }
            continue;
        }
        for token in block_tokens(line) {
            let cur = *stack.last().unwrap();
            if token == "end" {
                if stack.len() > 1 {
                    stack.pop();
                }
                continue;
            }
            if token == "columns" {
                continue;
            }
            if token == "block" || token.starts_with("block:") {
                let mut parts = token.splitn(3, ':').skip(1);
                let id = parts.next().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
                let span = parts.next().and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(1).max(1);
                let g = d.groups.len();
                d.groups.push(BlockGroup { id, ..Default::default() });
                d.groups[cur].items.push(BlockItem::Group(g, span));
                stack.push(g);
                continue;
            }
            if token == "space" || token.starts_with("space:") {
                let n = token.strip_prefix("space:").and_then(|n| n.parse::<u32>().ok()).unwrap_or(1).max(1);
                d.groups[cur].items.push(BlockItem::Space(n));
                continue;
            }
            if token == "auto" || token.parse::<u32>().is_ok() && line.starts_with("columns") {
                d.groups[cur].columns = token.parse::<u32>().ok().filter(|&n| n > 0);
                continue;
            }
            let (head, span) = split_span(token);
            let (idx, _) = ensure_block(&mut d, &mut map, head);
            d.groups[cur].items.push(BlockItem::Node(idx, span));
        }
    }
    d
}

fn arrow_size(heads: &[char], label: &str) -> (f32, f32) {
    let tw = estimate_text_width(label, 13.0);
    let horizontal = heads.iter().any(|c| matches!(c, 'L' | 'R'));
    let vertical = heads.iter().any(|c| matches!(c, 'U' | 'D'));
    match (horizontal, vertical) {
        (true, true) => ((tw * 2.0 + 40.0).max(80.0), 80.0),
        // The shaft is half the width: it has to hold the label.
        (false, true) => (((tw + 14.0) * 2.0).max(50.0), 70.0),
        _ => ((tw + 48.0).max(70.0), 50.0),
    }
}

fn node_size(node: &BlockNode) -> (f32, f32) {
    match &node.shape {
        BlockShape::Arrow(heads) => arrow_size(heads, &node.label),
        BlockShape::Flow(shape) => {
            let (w, h) = flow_node_size(*shape, &node.label, false);
            // A cylinder's caps take height away from its text.
            if *shape == NodeShape::Cylinder { (w, h + 16.0) } else { (w, h) }
        }
    }
}

/// Keeps its own width in a wide cell: round shapes, diamonds and arrows.
fn keeps_width(shape: &BlockShape) -> bool {
    matches!(
        shape,
        BlockShape::Arrow(_) | BlockShape::Flow(NodeShape::Circle | NodeShape::Diamond | NodeShape::Ext(ExtShape::DoubleCircle))
    )
}

struct GridMeasure {
    cols: u32,
    cell_w: f32,
    /// (item, column, row).
    cells: Vec<(BlockItem, u32, usize)>,
    row_h: Vec<f32>,
}

pub(super) struct BlockLayout {
    pub(super) node_rects: Vec<Option<RectF>>,
    pub(super) group_rects: Vec<Option<RectF>>,
    pub(super) width: f32,
    pub(super) height: f32,
}

fn measure(d: &BlockDiagram, g: usize, sizes: &mut [Option<(f32, f32)>], memo: &mut HashMap<usize, (f32, f32)>) -> (f32, f32) {
    let m = grid(d, g, sizes, memo);
    let pad = if g == 0 { 0.0 } else { PAD };
    let rows = m.row_h.len();
    let w = m.cols as f32 * m.cell_w + (m.cols.saturating_sub(1)) as f32 * GAP + pad * 2.0;
    let h = m.row_h.iter().sum::<f32>() + rows.saturating_sub(1) as f32 * GAP + pad * 2.0;
    let size = (w.max(if g == 0 { 0.0 } else { 40.0 }), h.max(if g == 0 { 0.0 } else { 40.0 }));
    memo.insert(g, size);
    size
}

fn item_size(d: &BlockDiagram, item: BlockItem, sizes: &mut [Option<(f32, f32)>], memo: &mut HashMap<usize, (f32, f32)>) -> (f32, f32) {
    match item {
        BlockItem::Node(n, _) => *sizes[n].get_or_insert_with(|| node_size(&d.nodes[n])),
        BlockItem::Space(_) => (0.0, 0.0),
        BlockItem::Group(c, _) => match memo.get(&c) {
            Some(s) => *s,
            None => measure(d, c, sizes, memo),
        },
    }
}

fn grid(d: &BlockDiagram, g: usize, sizes: &mut [Option<(f32, f32)>], memo: &mut HashMap<usize, (f32, f32)>) -> GridMeasure {
    let group = &d.groups[g];
    let span_of = |item: &BlockItem| match *item {
        BlockItem::Node(_, s) | BlockItem::Space(s) | BlockItem::Group(_, s) => s,
    };
    let cols = group.columns.unwrap_or_else(|| group.items.iter().map(span_of).sum::<u32>()).max(1);
    let mut cell_w = 30.0f32;
    let mut cells = Vec::new();
    let mut row_h: Vec<f32> = Vec::new();
    let (mut col, mut row) = (0u32, 0usize);
    for item in &group.items {
        let span = span_of(item).min(cols);
        if col + span > cols {
            col = 0;
            row += 1;
        }
        let (w, h) = item_size(d, *item, sizes, memo);
        cell_w = cell_w.max((w - (span - 1) as f32 * GAP) / span as f32);
        if row_h.len() <= row {
            row_h.push(0.0);
        }
        row_h[row] = row_h[row].max(h);
        cells.push((*item, col, row));
        col += span;
    }
    for h in &mut row_h {
        if *h <= 0.0 {
            *h = 40.0;
        }
    }
    GridMeasure { cols, cell_w, cells, row_h }
}

#[allow(clippy::too_many_arguments)]
fn place(d: &BlockDiagram, g: usize, x: f32, y: f32, w: f32, sizes: &mut [Option<(f32, f32)>], memo: &mut HashMap<usize, (f32, f32)>, out: &mut BlockLayout) {
    let m = grid(d, g, sizes, memo);
    let pad = if g == 0 { 0.0 } else { PAD };
    let cell_w = m.cell_w.max((w - pad * 2.0 - (m.cols - 1) as f32 * GAP) / m.cols as f32);
    let mut row_y = Vec::with_capacity(m.row_h.len());
    let mut cy = y + pad;
    for h in &m.row_h {
        row_y.push(cy);
        cy += h + GAP;
    }
    for (item, col, row) in m.cells {
        let span = match item {
            BlockItem::Node(_, s) | BlockItem::Space(s) | BlockItem::Group(_, s) => s.min(m.cols),
        };
        let cx = x + pad + col as f32 * (cell_w + GAP);
        let cw = span as f32 * cell_w + (span - 1) as f32 * GAP;
        let rh = m.row_h[row];
        match item {
            BlockItem::Node(n, _) => {
                let (nw, nh) = item_size(d, item, sizes, memo);
                let bw = if keeps_width(&d.nodes[n].shape) { nw } else { cw };
                out.node_rects[n] = Some(RectF { x: cx + (cw - bw) / 2.0, y: row_y[row] + (rh - nh) / 2.0, w: bw, h: nh });
            }
            BlockItem::Group(c, _) => {
                let (_, gh) = item_size(d, item, sizes, memo);
                out.group_rects[c] = Some(RectF { x: cx, y: row_y[row], w: cw, h: gh.max(rh) });
                place(d, c, cx, row_y[row], cw, sizes, memo, out);
            }
            BlockItem::Space(_) => {}
        }
    }
}

pub(super) fn layout_block(d: &BlockDiagram) -> BlockLayout {
    let mut sizes = vec![None; d.nodes.len()];
    let mut memo = HashMap::new();
    let (w, h) = measure(d, 0, &mut sizes, &mut memo);
    let mut out = BlockLayout { node_rects: vec![None; d.nodes.len()], group_rects: vec![None; d.groups.len()], width: w, height: h };
    place(d, 0, MARGIN, MARGIN, w, &mut sizes, &mut memo, &mut out);
    out.width = w + MARGIN * 2.0;
    out.height = h + MARGIN * 2.0;
    out
}

/// A plus-shaped outline with heads on `heads` (an axis with one head still has its tail arm).
pub(super) fn block_arrow_points(heads: &[char], x: f32, y: f32, w: f32, h: f32) -> Vec<(f32, f32)> {
    let horizontal = heads.iter().any(|c| matches!(c, 'L' | 'R'));
    let vertical = heads.iter().any(|c| matches!(c, 'U' | 'D'));
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let a = match (horizontal, vertical) {
        (true, true) => w.min(h) * 0.14,
        (false, true) => w / 4.0,
        _ => h / 4.0,
    };
    let head_half = a * 2.0;
    let mut pts = Vec::new();
    // Clockwise from the top right corner of the centre: right, down, left, up arms.
    for (dir, present) in [('R', horizontal), ('D', vertical), ('L', horizontal), ('U', vertical)] {
        // (along, across) unit vectors in screen space; `across` points clockwise.
        let (ax, ay, px, py, reach) = match dir {
            'R' => (1.0, 0.0, 0.0, 1.0, w / 2.0),
            'D' => (0.0, 1.0, -1.0, 0.0, h / 2.0),
            'L' => (-1.0, 0.0, 0.0, -1.0, w / 2.0),
            _ => (0.0, -1.0, 1.0, 0.0, h / 2.0),
        };
        let at = |along: f32, across: f32| (cx + ax * along + px * across, cy + ay * along + py * across);
        if !present {
            pts.push(at(a, -a));
            pts.push(at(a, a));
            continue;
        }
        if heads.contains(&dir) {
            let hl = (head_half * 1.1).min((reach - a) * 0.7).max(4.0);
            pts.push(at(reach - hl, -a));
            pts.push(at(reach - hl, -head_half));
            pts.push(at(reach, 0.0));
            pts.push(at(reach - hl, head_half));
            pts.push(at(reach - hl, a));
        } else {
            pts.push(at(reach, -a));
            pts.push(at(reach, a));
        }
    }
    pts.dedup_by(|p, q| (p.0 - q.0).abs() < 0.01 && (p.1 - q.1).abs() < 0.01);
    pts
}

pub(super) fn render_block_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let d = parse_block(source);
    if d.nodes.is_empty() && d.groups.len() == 1 {
        return None;
    }
    let layout = layout_block(&d);
    let total_w = layout.width.ceil().max(60.0);
    let total_h = layout.height.ceil().max(60.0);
    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));
    svg.push_str(
        r##"<defs><marker id="block-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#475569"/></marker></defs>"##,
    );
    let style_of = |id: &str, classes: &[String]| {
        let mut style = FlowStyle::default();
        for c in classes {
            if let Some(s) = d.class_defs.get(c) {
                style.merge(s);
            }
        }
        for (sid, s) in &d.styles {
            if sid == id {
                style.merge(s);
            }
        }
        style
    };
    // Nested blocks: a light frame behind their children.
    for (g, group) in d.groups.iter().enumerate().skip(1) {
        let Some(r) = layout.group_rects[g] else { continue };
        let at = svg.len();
        let id_attr = group.id.as_deref().map(|id| format!(r#" data-block="{}""#, escape_attr(id))).unwrap_or_default();
        svg.push_str(&format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="4" fill="#f8fafc" stroke="#94a3b8" stroke-width="1.2" data-role="block-group"{id_attr}/>"##,
            r.x, r.y, r.w, r.h
        ));
        if let Some(id) = &group.id {
            let style = style_of(id, &[]);
            if !style.is_empty() {
                let frag = svg.split_off(at);
                svg.push_str(&restyle_svg_fragment(&frag, &style));
            }
        }
    }
    let rect_of = |id: &str| -> Option<RectF> {
        d.nodes.iter().position(|n| n.id == id).and_then(|i| layout.node_rects[i]).or_else(|| {
            d.groups.iter().position(|g| g.id.as_deref() == Some(id)).and_then(|g| layout.group_rects[g])
        })
    };
    let mut labels = String::new();
    for edge in &d.edges {
        let (Some(a), Some(b)) = (rect_of(&edge.from), rect_of(&edge.to)) else { continue };
        if edge.tok.style == EdgeStyle::Invisible {
            continue;
        }
        let (acx, acy, bcx, bcy) = (a.x + a.w / 2.0, a.y + a.h / 2.0, b.x + b.w / 2.0, b.y + b.h / 2.0);
        let (sx, sy) = edge_anchor(acx, acy, a.w, a.h, bcx, bcy);
        let (ex, ey) = edge_anchor(bcx, bcy, b.w, b.h, acx, acy);
        let (width, dash) = match edge.tok.style {
            EdgeStyle::Thick => ("3", ""),
            EdgeStyle::Dashed => ("1.5", r#" stroke-dasharray="5,4""#),
            _ => ("1.5", ""),
        };
        let start = if edge.tok.start == EdgeEnd::None { "" } else { r#" marker-start="url(#block-arrow)""# };
        let end = if edge.tok.end == EdgeEnd::None { "" } else { r#" marker-end="url(#block-arrow)""# };
        svg.push_str(&format!(
            r##"<line x1="{sx:.1}" y1="{sy:.1}" x2="{ex:.1}" y2="{ey:.1}" stroke="#475569" stroke-width="{width}"{dash}{start}{end} data-edge="{}-{}"/>"##,
            escape_attr(&edge.from),
            escape_attr(&edge.to)
        ));
        if let Some(label) = edge.tok.label.as_deref().filter(|l| !l.trim().is_empty()) {
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
    for (idx, node) in d.nodes.iter().enumerate() {
        let Some(r) = layout.node_rects[idx] else { continue };
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let at = svg.len();
        let mut label_dy = 0.0;
        match &node.shape {
            BlockShape::Arrow(heads) => {
                push_flow_polygon(&mut svg, &block_arrow_points(heads, r.x, r.y, r.w, r.h), "#eff6ff", "#2563eb", "block-arrow");
            }
            BlockShape::Flow(shape) => label_dy = push_flow_shape(&mut svg, *shape, r.x, r.y, r.w, r.h, cx, cy),
        }
        let style = style_of(&node.id, &node.classes);
        if !style.is_empty() {
            let frag = svg.split_off(at);
            svg.push_str(&restyle_svg_fragment(&frag, &style));
        }
        if node.label.is_empty() {
            continue;
        }
        let max_w = match &node.shape {
            BlockShape::Flow(NodeShape::Diamond | NodeShape::Hexagon) => r.w * 0.62,
            BlockShape::Flow(NodeShape::Circle | NodeShape::Ext(ExtShape::DoubleCircle)) => r.w * 0.66,
            BlockShape::Arrow(_) => r.w,
            _ => r.w - 16.0,
        };
        let fill = style.color.as_deref().map(escape_attr).unwrap_or_else(|| "#0f172a".to_string());
        push_centered_label(&mut svg, cx, cy + label_dy, &node.label, 13.0, max_w.clamp(24.0, 168.0), &fill, "500");
    }
    svg.push_str(&labels);
    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w as u32, total_h as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn block_tokens_spans_and_shapes() {
        assert_eq!(block_tokens(r#"a["A label"] b:2 c<["x y"]>(x, down)"#), vec![r#"a["A label"]"#, "b:2", r#"c<["x y"]>(x, down)"#]);
        assert_eq!(split_span("b:2"), ("b", 2));
        assert_eq!(split_span("block"), ("block", 1));
        let (id, label, shape) = parse_block_node(r#"id1((("circle")))"#);
        assert_eq!((id.as_str(), label.as_deref()), ("id1", Some("circle")));
        assert_eq!(shape, Some(BlockShape::Flow(NodeShape::Ext(ExtShape::DoubleCircle))));
        let (id, label, shape) = parse_block_node(r#"blockArrowId7<["Label"]>(x, down)"#);
        assert_eq!((id.as_str(), label.as_deref()), ("blockArrowId7", Some("Label")));
        assert_eq!(shape, Some(BlockShape::Arrow(vec!['L', 'R', 'D'])));
        let (_, label, _) = parse_block_node(r#"a<["&nbsp;&nbsp;"]>(down)"#);
        assert_eq!(label.as_deref(), Some(""));
        assert_eq!(parse_block_node("id1[(\"Database\")]").2, Some(BlockShape::Flow(NodeShape::Cylinder)));
    }

    #[test]
    fn block_grid_columns_spans_spaces_and_nesting() {
        let d = parse_block("block\n  columns 3\n  a:3\n  block:group1:2\n    columns 2\n    h i j k\n  end\n  g\n  space b");
        assert_eq!(d.groups[0].columns, Some(3));
        assert_eq!(d.groups[1].id.as_deref(), Some("group1"));
        assert_eq!(d.groups[1].items.len(), 4);
        let l = layout_block(&d);
        let r = |id: &str| l.node_rects[d.nodes.iter().position(|n| n.id == id).unwrap()].unwrap();
        // `a:3` spans the whole first row; the group takes two columns, `g` the third.
        let (a, g, h, i, j, b) = (r("a"), r("g"), r("h"), r("i"), r("j"), r("b"));
        let group = l.group_rects[1].unwrap();
        assert!((a.w - (group.w + GAP + g.w)).abs() < 0.5, "{a:?} {group:?} {g:?}");
        assert!(group.y > a.y + a.h && (g.x - (group.x + group.w + GAP)).abs() < 0.5);
        assert!((h.y - i.y).abs() < 0.1 && j.y > h.y + h.h && (h.x - j.x).abs() < 0.1);
        for n in [h, i, j] {
            assert!(n.x > group.x && n.x + n.w < group.x + group.w && n.y > group.y && n.y + n.h < group.y + group.h);
        }
        // `space` leaves the first column of the last row empty.
        assert!(b.y > group.y + group.h && (b.x - (a.x + g.w + GAP)).abs() < 0.5, "{b:?}");
        // `columns auto`: everything on one row.
        let d = parse_block("block\n  a b c d");
        let l = layout_block(&d);
        let ys: Vec<f32> = l.node_rects.iter().map(|r| r.unwrap().y).collect();
        assert!(ys.windows(2).all(|w| (w[0] - w[1]).abs() < 0.1));
    }

    #[test]
    fn block_edges_styles_and_arrows_render() {
        let svg = svg_of(
            "block-beta\n  columns 3\n  A space:2 B\n  block:ID\n    C\n  end\n  A-- \"X\" -->B\n  ID --> A\n  r<[\"Go\"]>(right)\n  classDef blue fill:#6e6ce6,stroke:#333;\n  class A blue\n  style B fill:#bbf,stroke:#f66,color:#fff",
        );
        assert!(svg.contains(r#"data-edge="A-B""#) && svg.contains(r#"data-edge="ID-A""#));
        assert!(svg.contains(">X</text>"));
        assert!(svg.contains(r##"fill="#6e6ce6""##) && svg.contains(r##"fill="#bbf""##) && svg.contains(r##"fill="#fff""##));
        assert!(svg.contains(r#"data-shape="block-arrow""#) && svg.contains(">Go</text>"));
        assert!(svg.contains(r#"data-role="block-group" data-block="ID""#));
        // `ID --> A` starts on the group frame, not on a separate box.
        assert_eq!(svg.matches(">ID<").count(), 0);
    }

    #[test]
    fn block_arrow_outline_points_its_heads() {
        let pts = block_arrow_points(&['R'], 0.0, 0.0, 100.0, 40.0);
        let max_x = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
        assert_eq!(pts.iter().filter(|p| (p.0 - max_x).abs() < 0.01).count(), 1, "one tip on the right: {pts:?}");
        assert_eq!(pts.iter().filter(|p| p.0.abs() < 0.01).count(), 2, "flat tail on the left");
        let pts = block_arrow_points(&['U', 'D'], 0.0, 0.0, 40.0, 100.0);
        assert!(pts.contains(&(20.0, 0.0)) && pts.contains(&(20.0, 100.0)));
    }
}
