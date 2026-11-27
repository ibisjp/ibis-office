// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `swimlane-beta`: flowchart nodes and links grouped into full-width lanes (`subgraph … end`).
//! Steps take columns by their longest path from a start (back links ignored), lanes stack
//! across the flow (`LR`: lanes are rows; `TB`: lanes are columns).

use super::*;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub(super) struct Swimlanes {
    pub(super) horizontal: bool,
    pub(super) lanes: Vec<(String, String)>,
    pub(super) nodes: Vec<FlowNode>,
    pub(super) ids: Vec<String>,
    pub(super) edges: Vec<FlowEdge>,
    pub(super) class_defs: HashMap<String, FlowStyle>,
    pub(super) styles: Vec<(String, FlowStyle)>,
}

pub(super) fn parse_swimlanes(source: &str) -> Swimlanes {
    let mut lines = source.lines();
    let header = lines.next().unwrap_or("");
    let dir = header.split_whitespace().nth(1).unwrap_or("LR");
    let mut s = Swimlanes { horizontal: !matches!(dir, "TB" | "TD" | "BT"), ..Default::default() };
    let mut node_map: HashMap<String, usize> = HashMap::new();
    let mut current: Option<usize> = None;
    let mut assigns: Vec<(String, String)> = Vec::new();
    for line in lines {
        for stmt in split_flow_statements(line) {
            let l = stmt.trim();
            if l.is_empty() || l.starts_with("%%") || l.starts_with("accTitle") || l.starts_with("accDescr") || l.starts_with("direction ") {
                continue;
            }
            if let Some(rest) = l.strip_prefix("subgraph ") {
                let rest = rest.trim();
                let (id, title) = match rest.find('[') {
                    Some(open) => (rest[..open].trim().to_string(), unquote(rest[open + 1..].trim_end_matches(']').trim())),
                    None => (unquote(rest), unquote(rest)),
                };
                s.lanes.push((id, title));
                current = Some(s.lanes.len() - 1);
                continue;
            }
            if l == "end" {
                current = None;
                continue;
            }
            if let Some(rest) = l.strip_prefix("classDef ") {
                let (names, props) = rest.trim().split_once(char::is_whitespace).unwrap_or((rest, ""));
                let style = parse_flow_style(props.trim_end_matches(';'));
                for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                    s.class_defs.entry(name.to_string()).or_default().merge(&style);
                }
                continue;
            }
            if let Some(rest) = l.strip_prefix("class ") {
                if let Some((ids, class)) = rest.trim().rsplit_once(char::is_whitespace) {
                    for id in ids.split(',').map(str::trim).filter(|id| !id.is_empty()) {
                        assigns.push((id.to_string(), class.trim_end_matches(';').trim().to_string()));
                    }
                }
                continue;
            }
            if let Some(rest) = l.strip_prefix("style ") {
                if let Some((id, props)) = rest.trim().split_once(char::is_whitespace) {
                    s.styles.push((id.to_string(), parse_flow_style(props)));
                }
                continue;
            }
            let before = s.nodes.len();
            parse_flowchart_statement(l, current, &mut s.nodes, &mut node_map, &mut s.edges);
            if s.nodes.len() > before {
                let mut by_index: Vec<(&String, &usize)> = node_map.iter().filter(|(_, i)| **i >= before).collect();
                by_index.sort_by_key(|(_, i)| **i);
                s.ids.extend(by_index.into_iter().map(|(id, _)| id.clone()));
            }
        }
    }
    for (id, class) in assigns {
        if let Some(&i) = node_map.get(&id) {
            s.nodes[i].classes.push(class);
        }
    }
    s
}

/// Longest-path columns over the links that do not close a cycle (DFS back links are skipped).
pub(super) fn swimlane_ranks(n: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut out_edges = vec![Vec::new(); n];
    for (k, &(a, b)) in edges.iter().enumerate() {
        out_edges[a].push((b, k));
    }
    let mut back = vec![false; edges.len()];
    let mut state = vec![0u8; n];
    for root in 0..n {
        if state[root] != 0 {
            continue;
        }
        let mut stack = vec![(root, 0usize)];
        state[root] = 1;
        while let Some(&mut (v, ref mut next)) = stack.last_mut() {
            if *next < out_edges[v].len() {
                let (w, k) = out_edges[v][*next];
                *next += 1;
                match state[w] {
                    0 => {
                        state[w] = 1;
                        stack.push((w, 0));
                    }
                    1 => back[k] = true,
                    _ => {}
                }
            } else {
                state[v] = 2;
                stack.pop();
            }
        }
    }
    let mut rank = vec![0usize; n];
    let mut indeg = vec![0usize; n];
    for (k, &(_, b)) in edges.iter().enumerate() {
        if !back[k] {
            indeg[b] += 1;
        }
    }
    let mut queue: Vec<usize> = (0..n).filter(|&v| indeg[v] == 0).collect();
    while let Some(v) = queue.pop() {
        for &(w, k) in &out_edges[v] {
            if back[k] {
                continue;
            }
            rank[w] = rank[w].max(rank[v] + 1);
            indeg[w] -= 1;
            if indeg[w] == 0 {
                queue.push(w);
            }
        }
    }
    rank
}

pub(super) fn render_swimlanes_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let s = parse_swimlanes(source);
    if s.nodes.is_empty() {
        return None;
    }
    let n = s.nodes.len();
    let mut lanes = s.lanes.clone();
    let loose = s.nodes.iter().any(|node| node.subgraph.is_none());
    if loose {
        lanes.push((String::new(), String::new()));
    }
    let lane_of = |i: usize| s.nodes[i].subgraph.unwrap_or(lanes.len() - 1);
    let pairs: Vec<(usize, usize)> = s.edges.iter().map(|e| (e.from, e.to)).collect();
    let rank = swimlane_ranks(n, &pairs);
    let ranks = rank.iter().copied().max().unwrap_or(0) + 1;
    let sizes: Vec<(f32, f32)> = s.nodes.iter().map(|node| flow_node_size(node.shape, &node.label, s.horizontal)).collect();
    // Slot of each node inside its (lane, rank) cell.
    let mut slot = vec![0usize; n];
    let mut cell_count: HashMap<(usize, usize), usize> = HashMap::new();
    for i in 0..n {
        let c = cell_count.entry((lane_of(i), rank[i])).or_insert(0);
        slot[i] = *c;
        *c += 1;
    }
    let label_room = s.edges.iter().filter_map(|e| e.label.as_deref()).map(|l| flow_edge_label_size(l)).fold((0.0f32, 0.0f32), |a, b| (a.0.max(b.0), a.1.max(b.1)));
    // Along the flow: column (LR) or row (TB) extents; across: per-lane extents.
    let along = |sz: (f32, f32)| if s.horizontal { sz.0 } else { sz.1 };
    let across = |sz: (f32, f32)| if s.horizontal { sz.1 } else { sz.0 };
    let gap_along = if s.horizontal { 56.0f32.max(label_room.0 + 28.0) } else { 48.0f32.max(label_room.1 + 28.0) };
    let gap_across = 24.0;
    let mut rank_size = vec![0.0f32; ranks];
    for i in 0..n {
        rank_size[rank[i]] = rank_size[rank[i]].max(along(sizes[i]));
    }
    let mut lane_size = vec![0.0f32; lanes.len()];
    let mut cell_extent: HashMap<(usize, usize), f32> = HashMap::new();
    for i in 0..n {
        let e = cell_extent.entry((lane_of(i), rank[i])).or_insert(0.0);
        *e += across(sizes[i]) + if slot[i] > 0 { gap_across } else { 0.0 };
    }
    for (&(lane, _), &e) in &cell_extent {
        lane_size[lane] = lane_size[lane].max(e);
    }
    for size in lane_size.iter_mut() {
        *size = (*size + 2.0 * gap_across).max(if s.horizontal { 90.0 } else { 150.0 });
    }
    let header = if s.horizontal {
        lanes.iter().map(|(_, t)| estimate_text_width(t, 14.0)).fold(0.0f32, f32::max).clamp(60.0, 180.0) + 24.0
    } else {
        36.0
    };
    if !s.horizontal {
        for (k, (_, t)) in lanes.iter().enumerate() {
            lane_size[k] = lane_size[k].max(estimate_text_width(t, 14.0) + 24.0);
        }
    }
    let margin = 16.0;
    let mut rank_pos = vec![0.0f32; ranks];
    let mut cursor = margin + header + gap_along / 2.0;
    for r in 0..ranks {
        rank_pos[r] = cursor + rank_size[r] / 2.0;
        cursor += rank_size[r] + gap_along;
    }
    let flow_len = cursor - gap_along / 2.0 + margin;
    let mut lane_start = vec![0.0f32; lanes.len()];
    let mut acc = margin;
    for k in 0..lanes.len() {
        lane_start[k] = acc;
        acc += lane_size[k];
    }
    let cross_len = acc + margin;
    let (width, height) = if s.horizontal { (flow_len, cross_len) } else { (cross_len, flow_len) };
    let mut centers = vec![(0.0f32, 0.0f32); n];
    for i in 0..n {
        let lane = lane_of(i);
        let cell = cell_extent[&(lane, rank[i])];
        let mut offset = lane_start[lane] + (lane_size[lane] - cell) / 2.0;
        for j in 0..i {
            if lane_of(j) == lane && rank[j] == rank[i] {
                offset += across(sizes[j]) + gap_across;
            }
        }
        let c = offset + across(sizes[i]) / 2.0;
        centers[i] = if s.horizontal { (rank_pos[rank[i]], c) } else { (c, rank_pos[rank[i]]) };
    }
    let style_of = |i: usize| {
        let mut st = s.class_defs.get("default").cloned().unwrap_or_default();
        for c in &s.nodes[i].classes {
            if let Some(d) = s.class_defs.get(c) {
                st.merge(d);
            }
        }
        for (id, d) in &s.styles {
            if s.ids.get(i) == Some(id) {
                st.merge(d);
            }
        }
        st
    };

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.0} {height:.0}" font-family="sans-serif"><rect width="{width:.0}" height="{height:.0}" fill="#ffffff"/>"##
    ));
    svg.push_str(r##"<defs><marker id="sl-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerUnits="userSpaceOnUse" markerWidth="10" markerHeight="10" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#475569"/></marker></defs>"##);
    for (k, (id, title)) in lanes.iter().enumerate() {
        let fill = if k % 2 == 0 { "#f8fafc" } else { "#f1f5f9" };
        let (x, y, w, h) = if s.horizontal { (margin, lane_start[k], flow_len - 2.0 * margin, lane_size[k]) } else { (lane_start[k], margin, lane_size[k], flow_len - 2.0 * margin) };
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" fill="{fill}" stroke="#94a3b8" stroke-width="1" data-lane="{}"/>"##,
            escape_attr(id)
        ));
        let (hx, hy, hw, hh) = if s.horizontal { (x, y, header, h) } else { (x, y, w, header) };
        svg.push_str(&format!(r##"<rect x="{hx:.1}" y="{hy:.1}" width="{hw:.1}" height="{hh:.1}" fill="#e2e8f0" stroke="#94a3b8" stroke-width="1"/>"##));
        if !title.is_empty() {
            push_centered_label(&mut svg, hx + hw / 2.0, hy + hh / 2.0, title, 14.0, hw.max(60.0) - 12.0, "#0f172a", "600");
        }
    }
    let canvas = (width, height);
    let mut labels = String::new();
    for e in &s.edges {
        if e.style == EdgeStyle::Invisible {
            continue;
        }
        let route = route_layered_edge(e.from, e.to, &centers, &sizes, s.horizontal, true, canvas);
        let mut extra = String::new();
        if e.end != EdgeEnd::None {
            extra.push_str(r#" marker-end="url(#sl-arrow)""#);
        }
        if e.start != EdgeEnd::None {
            extra.push_str(r#" marker-start="url(#sl-arrow)""#);
        }
        if e.style == EdgeStyle::Dashed {
            extra.push_str(r#" stroke-dasharray="4,3""#);
        }
        let width = if e.style == EdgeStyle::Thick { "3" } else { "1.5" };
        push_routed_edge(&mut svg, &route, "#475569", width, &extra);
        if let Some(label) = e.label.as_deref().filter(|l| !l.is_empty()) {
            let (lw, lh) = flow_edge_label_size(label);
            let (lx, ly) = route.label_at;
            labels.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{lw:.1}" height="{lh:.1}" rx="3" fill="#ffffff" opacity="0.92"/>"##,
                lx - lw / 2.0,
                ly - lh / 2.0
            ));
            push_centered_label(&mut labels, lx, ly, label, 11.0, 140.0, "#334155", "normal");
        }
    }
    for (i, node) in s.nodes.iter().enumerate() {
        let (cx, cy) = centers[i];
        let (w, h) = sizes[i];
        let at = svg.len();
        let dy = push_flow_shape(&mut svg, node.shape, cx - w / 2.0, cy - h / 2.0, w, h, cx, cy);
        let style = style_of(i);
        if !style.is_empty() {
            let frag = svg.split_off(at);
            svg.push_str(&restyle_svg_fragment(&frag, &style));
        }
        let fill = style.color.as_deref().and_then(safe_css_value).unwrap_or_else(|| "#0f172a".to_string());
        let max_w = match node.shape {
            NodeShape::Diamond | NodeShape::Hexagon => w * 0.62,
            NodeShape::Circle => w * 0.66,
            _ => w - 16.0,
        };
        push_centered_label(&mut svg, cx, cy + dy, &node.label, 13.0, max_w.clamp(24.0, 168.0), &fill, "500");
    }
    svg.push_str(&labels);
    svg.push_str("</svg>");
    Some((svg.into_bytes(), width.ceil() as u32, height.ceil() as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn swimlane_lanes_nodes_and_ranks() {
        let s = parse_swimlanes("swimlane-beta LR\n  subgraph sales [Sales team]\n    lead[Qualify lead]\n  end\n  subgraph Ops\n    plan[Plan]\n  end\n  lead --> plan --> lead\n  classDef hot fill:#fee;\n  class plan hot;");
        assert!(s.horizontal);
        assert_eq!(s.lanes, vec![("sales".into(), "Sales team".into()), ("Ops".into(), "Ops".into())]);
        assert_eq!((s.nodes[0].subgraph, s.nodes[1].subgraph), (Some(0), Some(1)));
        assert_eq!(s.nodes[1].classes, vec!["hot".to_string()]);
        assert_eq!(swimlane_ranks(2, &[(0, 1), (1, 0)]), vec![0, 1]);
        assert_eq!(swimlane_ranks(4, &[(0, 1), (1, 2), (0, 3), (3, 2)]), vec![0, 1, 2, 1]);
    }

    #[test]
    fn swimlane_svg_puts_steps_in_their_lane_rows() {
        let svg = svg_of("swimlane-beta LR\n  subgraph A\n    a1[One]\n    a2[Two]\n  end\n  subgraph B\n    b1[Three]\n  end\n  a1 --> a2 --> b1");
        let (y1, y2, y3) = (text_y(&svg, "One"), text_y(&svg, "Two"), text_y(&svg, "Three"));
        assert!((y1 - y2).abs() < 0.5 && y3 > y1 + 40.0);
        assert!(text_x(&svg, "One") < text_x(&svg, "Two") && text_x(&svg, "Two") < text_x(&svg, "Three"));
        assert_eq!(svg.matches("data-lane=").count(), 2);
        let tb = svg_of("swimlane-beta TB\n  subgraph A\n    a1[One]\n  end\n  subgraph B\n    b1[Two]\n  end\n  a1 --> b1");
        assert!(text_x(&tb, "Two") > text_x(&tb, "One") + 40.0 && text_y(&tb, "Two") > text_y(&tb, "One"));
    }
}
