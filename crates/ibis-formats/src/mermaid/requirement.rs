// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `requirementDiagram`: requirements and elements as boxes, relationships as labelled lines.

use super::*;

/// A `requirement` (or one of its kinds) or an `element` block.
pub(super) struct ReqNode {
    pub(super) name: String,
    /// `<<Requirement>>`, `<<Functional Requirement>>`, ..., `<<Element>>`.
    pub(super) stereotype: &'static str,
    /// (label, value) rows: `Id`, `Text`, `Risk`, `Verification`, or `Type`, `Doc Ref`.
    pub(super) rows: Vec<(&'static str, String)>,
    pub(super) element: bool,
    pub(super) classes: Vec<String>,
}

pub(super) struct ReqEdge {
    pub(super) from: usize,
    pub(super) to: usize,
    pub(super) kind: String,
}

const REQUIREMENT_KINDS: &[(&str, &str)] = &[
    ("requirement", "Requirement"),
    ("functionalRequirement", "Functional Requirement"),
    ("interfaceRequirement", "Interface Requirement"),
    ("performanceRequirement", "Performance Requirement"),
    ("physicalRequirement", "Physical Requirement"),
    ("designConstraint", "Design Constraint"),
];

const RELATIONSHIPS: &[&str] = &["contains", "copies", "derives", "satisfies", "verifies", "refines", "traces"];

/// `Low` / `Test` etc. as Mermaid shows them (first letter upper case).
fn capitalised(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// `a - satisfies -> b` as (`a`, `satisfies`, `b`); `b <- copies - a` as (`a`, `copies`, `b`).
pub(super) fn parse_req_relation(line: &str) -> Option<(String, String, String)> {
    let split_kind = |part: &str| -> Option<(String, String)> {
        let (name, kind) = part.split_once(" - ").or_else(|| part.split_once('-'))?;
        Some((name.trim().to_string(), kind.trim().to_ascii_lowercase()))
    };
    let (from, kind, to) = if let Some((target, rest)) = line.split_once("<-") {
        // `target <- kind - source`
        let (kind, source) = rest.rsplit_once(" - ").or_else(|| rest.rsplit_once('-'))?;
        (source.trim().to_string(), kind.trim().to_ascii_lowercase(), target.trim().to_string())
    } else {
        let (left, target) = line.split_once("->")?;
        let (source, kind) = split_kind(left)?;
        (source, kind, target.trim().to_string())
    };
    (RELATIONSHIPS.contains(&kind.as_str()) && !from.is_empty() && !to.is_empty()).then(|| (unquote(&from), kind, unquote(&to)))
}

pub(super) fn render_requirement_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let mut nodes: Vec<ReqNode> = Vec::new();
    let mut map: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<ReqEdge> = Vec::new();
    let mut class_defs: HashMap<String, FlowStyle> = HashMap::new();
    let mut styles: Vec<(String, FlowStyle)> = Vec::new();
    let (mut horizontal, mut reversed) = (false, false);
    let mut open: Option<usize> = None;

    let ensure = |raw: &str, nodes: &mut Vec<ReqNode>, map: &mut HashMap<String, usize>| -> usize {
        let (name, class) = match raw.trim().split_once(":::") {
            Some((name, class)) => (unquote(name.trim()), Some(class.trim().to_string())),
            None => (unquote(raw.trim()), None),
        };
        let idx = *map.entry(name.clone()).or_insert_with(|| {
            nodes.push(ReqNode { name: name.clone(), stereotype: "Element", rows: Vec::new(), element: true, classes: Vec::new() });
            nodes.len() - 1
        });
        if let Some(class) = class.filter(|c| !c.is_empty()) {
            nodes[idx].classes.push(class);
        }
        idx
    };

    for raw in source.lines().skip(1) {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if let Some(idx) = open {
            if l == "}" {
                open = None;
                continue;
            }
            let Some((key, value)) = l.split_once(':') else { continue };
            let value = unquote(value.trim());
            let node = &mut nodes[idx];
            let row = match key.trim().to_ascii_lowercase().as_str() {
                "id" => ("Id", value),
                "text" => ("Text", value),
                "risk" => ("Risk", capitalised(&value)),
                "verifymethod" => ("Verification", capitalised(&value)),
                "type" => ("Type", value),
                "docref" => ("Doc Ref", value),
                _ => continue,
            };
            node.rows.push(row);
            continue;
        }
        if let Some(rest) = l.strip_prefix("direction ") {
            (horizontal, reversed) = match rest.trim().to_ascii_uppercase().as_str() {
                "LR" => (true, false),
                "RL" => (true, true),
                "BT" => (false, true),
                _ => (false, false),
            };
            continue;
        }
        if let Some(rest) = l.strip_prefix("classDef ") {
            let rest = rest.trim();
            let (names, props) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let style = parse_flow_style(props);
            for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                class_defs.entry(name.to_string()).or_default().merge(&style);
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("style ") {
            if let Some((ids, props)) = rest.trim().split_once(char::is_whitespace) {
                let style = parse_flow_style(props);
                for id in ids.split(',').map(str::trim).filter(|id| !id.is_empty()) {
                    styles.push((unquote(id), style.clone()));
                }
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("class ") {
            if let Some((ids, class)) = rest.trim().rsplit_once(char::is_whitespace) {
                for id in ids.split(',').map(str::trim).filter(|id| !id.is_empty()) {
                    let idx = ensure(id, &mut nodes, &mut map);
                    nodes[idx].classes.push(class.trim().to_string());
                }
            }
            continue;
        }
        if let Some(head) = l.strip_suffix('{') {
            let head = head.trim();
            let (word, name) = head.split_once(char::is_whitespace).unwrap_or((head, ""));
            let kind = REQUIREMENT_KINDS.iter().find(|(k, _)| k.eq_ignore_ascii_case(word));
            if (kind.is_some() || word == "element") && !name.trim().is_empty() {
                let idx = ensure(name, &mut nodes, &mut map);
                let node = &mut nodes[idx];
                node.element = kind.is_none();
                node.stereotype = kind.map_or("Element", |(_, label)| label);
                open = Some(idx);
            }
            continue;
        }
        if let Some((from, kind, to)) = parse_req_relation(l) {
            let from = ensure(&from, &mut nodes, &mut map);
            let to = ensure(&to, &mut nodes, &mut map);
            edges.push(ReqEdge { from, to, kind });
        }
    }
    if nodes.is_empty() {
        return None;
    }

    let mut node_style: Vec<FlowStyle> = nodes
        .iter()
        .map(|n| {
            let mut style = class_defs.get("default").cloned().unwrap_or_default();
            for name in &n.classes {
                if let Some(def) = class_defs.get(name) {
                    style.merge(def);
                }
            }
            style
        })
        .collect();
    for (id, style) in &styles {
        if let Some(&idx) = map.get(id) {
            node_style[idx].merge(style);
        }
    }

    // Each node: the stereotype, the name, a rule, then its rows (long text wraps).
    const ROW_FONT: f32 = 11.0;
    const ROW_H: f32 = 15.0;
    const MAX_W: f32 = 240.0;
    let row_lines: Vec<Vec<Vec<StyledRun>>> = nodes
        .iter()
        .map(|n| {
            n.rows
                .iter()
                .flat_map(|(label, value)| rich_lines(&format!("{label}: {value}"), MAX_W, ROW_FONT))
                .collect()
        })
        .collect();
    let width_of = |line: &[StyledRun], font: f32| {
        line.iter().map(|r| estimate_text_width(&r.text, font) * if r.bold { 1.06 } else { 1.0 }).sum::<f32>()
    };
    let sizes: Vec<(f32, f32)> = nodes
        .iter()
        .zip(&row_lines)
        .map(|(n, rows)| {
            let name_w = width_of(&markdown_runs(&n.name), 13.0) * 1.06;
            let stereo_w = estimate_text_width(&format!("<<{}>>", n.stereotype), 10.5);
            let rows_w = rows.iter().map(|l| width_of(l, ROW_FONT)).fold(0.0f32, f32::max);
            let w = (name_w.max(stereo_w).max(rows_w) + 28.0).max(150.0);
            let h = 44.0 + if rows.is_empty() { 0.0 } else { rows.len() as f32 * ROW_H + 14.0 };
            (w, h)
        })
        .collect();

    let pairs: Vec<(usize, usize)> = edges.iter().map(|e| (e.from, e.to)).collect();
    let (centers, total_w, total_h) = compute_layered_layout(nodes.len(), &pairs, &sizes, horizontal, reversed, 32.0, 70.0, 40.0);

    // Relationships: `contains` is a solid line with a crossed circle at the container,
    // the others dashed with an open arrow at the target. The label is `<<kind>>`.
    let mut edge_svg = String::new();
    let mut right = total_w as f32;
    for e in &edges {
        if e.from == e.to {
            continue;
        }
        let route = route_layered_edge(e.from, e.to, &centers, &sizes, horizontal, false, (total_w as f32, total_h as f32));
        let extra = if e.kind == "contains" {
            r#" marker-start="url(#req-contains)""#.to_string()
        } else {
            r#" stroke-dasharray="6,4" marker-end="url(#req-arrow)""#.to_string()
        };
        push_routed_edge(&mut edge_svg, &route, "#475569", "1.3", &format!(r#"{extra} data-relation="{}""#, escape_attr(&e.kind)));
        let label = format!("<<{}>>", e.kind);
        let (mx, my) = route.label_at;
        let lw = estimate_text_width(&label, 10.5) + 10.0;
        // A detour around the nodes runs along the edge of the drawing: keep its label inside.
        right = right.max(mx + lw / 2.0 + 6.0);
        edge_svg.push_str(&format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{lw:.1}" height="17" rx="3" fill="#ffffff" stroke="#e2e8f0" stroke-width="0.8"/><text x="{mx:.1}" y="{:.1}" text-anchor="middle" font-size="10.5" fill="#334155">{}</text>"##,
            mx - lw / 2.0,
            my - 8.5,
            my + 3.5,
            escape_xml(&label)
        ));
    }

    let total_w = right.ceil() as u32;
    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(
        r##"<defs><marker id="req-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M 0 1 L 10 5 L 0 9" fill="none" stroke="#475569" stroke-width="1.5"/></marker><marker id="req-contains" viewBox="0 0 20 20" refX="1" refY="10" markerWidth="12" markerHeight="12" orient="auto"><circle cx="10" cy="10" r="8.5" fill="#ffffff" stroke="#475569" stroke-width="1.5"/><path d="M 1.5 10 L 18.5 10 M 10 1.5 L 10 18.5" stroke="#475569" stroke-width="1.5"/></marker></defs>"##,
    );
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));

    svg.push_str(&edge_svg);

    for (idx, n) in nodes.iter().enumerate() {
        let (cx, cy) = centers[idx];
        let (w, h) = sizes[idx];
        let (x, y) = (cx - w / 2.0, cy - h / 2.0);
        let style = &node_style[idx];
        let (fill, stroke) = if n.element { ("#f0fdf4", "#15803d") } else { ("#eff6ff", "#2563eb") };
        let shape = format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="3" fill="{fill}" stroke="{stroke}" stroke-width="1.4" data-kind="{}"/>"##,
            if n.element { "element" } else { "requirement" }
        );
        svg.push_str(&if style.is_empty() { shape } else { restyle_svg_fragment(&shape, style) });
        let ink = style.color.as_deref().map(escape_attr).unwrap_or_else(|| "#0f172a".to_string());
        let weight = style.font_weight.as_deref().map(escape_attr);
        svg.push_str(&format!(
            r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="10.5" fill="{ink}">{}</text>"##,
            y + 15.0,
            escape_xml(&format!("<<{}>>", n.stereotype))
        ));
        svg.push_str(&format!(
            r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="13" font-weight="{}" fill="{ink}">{}</text>"##,
            y + 33.0,
            weight.clone().unwrap_or_else(|| "bold".to_string()),
            runs_markup(&markdown_runs(&n.name))
        ));
        let rows = &row_lines[idx];
        if !rows.is_empty() {
            svg.push_str(&format!(
                r##"<line x1="{x:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{stroke}" stroke-width="1"/>"##,
                y + 42.0,
                x + w,
                y + 42.0
            ));
            for (k, line) in rows.iter().enumerate() {
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{:.1}" font-size="{ROW_FONT}" fill="{ink}"{}>{}</text>"##,
                    x + 10.0,
                    y + 58.0 + k as f32 * ROW_H,
                    weight.as_deref().map(|v| format!(r#" font-weight="{v}""#)).unwrap_or_default(),
                    runs_markup(line)
                ));
            }
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn requirement_relations_parse_both_directions() {
        assert_eq!(parse_req_relation("test_entity - satisfies -> test_req"), Some(("test_entity".into(), "satisfies".into(), "test_req".into())));
        assert_eq!(parse_req_relation("test_req <- copies - test_entity2"), Some(("test_entity2".into(), "copies".into(), "test_req".into())));
        assert_eq!(parse_req_relation("a - likes -> b"), None);
        assert_eq!(parse_req_relation("id: 1"), None);
    }

    #[test]
    fn requirement_boxes_rows_and_relationships() {
        let src = "requirementDiagram
  requirement checkout_req {
    id: 1
    text: Orders must be payable online.
    risk: high
    verifymethod: test
  }
  functionalRequirement payment_req {
    id: 1.1
    text: Card payments must be authorised.
    risk: high
    verifymethod: test
  }
  element checkout_service {
    type: service
  }
  checkout_req - contains -> payment_req
  checkout_service - satisfies -> payment_req
";
        let svg = svg_of(src);
        assert_eq!(svg.matches(r#"data-kind="requirement""#).count(), 2, "{svg}");
        assert_eq!(svg.matches(r#"data-kind="element""#).count(), 1);
        for text in ["&lt;&lt;Requirement&gt;&gt;", "&lt;&lt;Functional Requirement&gt;&gt;", "&lt;&lt;Element&gt;&gt;", ">checkout_req<", ">Id: 1<", ">Risk: High<", ">Verification: Test<", ">Type: service<", "&lt;&lt;satisfies&gt;&gt;"] {
            assert!(svg.contains(text), "{text}: {svg}");
        }
        assert!(svg.contains(r#"marker-start="url(#req-contains)" data-relation="contains""#));
        assert!(svg.contains(r#"stroke-dasharray="6,4" marker-end="url(#req-arrow)" data-relation="satisfies""#));
        // The container is drawn above what it contains (TB).
        let y_of = |name: &str| attr_f32(tag_with(&svg, &format!(">{name}<"))[0], "y").unwrap();
        assert!(y_of("checkout_req") < y_of("payment_req"));
    }

    #[test]
    fn requirement_styles_classes_direction_and_markdown() {
        let src = "requirementDiagram
direction LR
requirement \"__test_req__\" {
    id: 1
    text: \"*italicized text* **bold text**\"
    risk: low
    verifymethod: test
}
element test_entity {
    type: simulation
}
element other:::important {
    type: x
}
test_entity - satisfies -> \"__test_req__\"
classDef important fill:#f96,stroke:#333
style test_entity fill:#f9f,stroke:#333, color: blue
";
        let svg = svg_of(src);
        assert!(svg.contains(r#"<tspan font-weight="bold">test_req</tspan>"#), "{svg}");
        assert!(svg.contains(r#"<tspan font-style="italic">italicized text</tspan>"#), "{svg}");
        assert!(svg.contains(r##"fill="#f9f" stroke="#333""##) && svg.contains(r#"fill="blue">"#), "{svg}");
        assert!(svg.contains(r##"fill="#f96" stroke="#333""##));
        // LR: the element sits left of the requirement it satisfies.
        let x_of = |marker: &str| attr_f32(tag_with(&svg, marker)[0], "x").unwrap();
        assert!(x_of(">test_entity<") < x_of(r#"><tspan font-weight="bold">test_req"#), "{svg}");
    }
}
