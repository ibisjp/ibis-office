// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `mindmap`.

use super::*;

/// Node shapes: `text`, `id[square]`, `id(rounded)`, `id((circle))`, `id))bang((`,
/// `id)cloud(`, `id{{hexagon}}`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum MindShape {
    Default,
    Square,
    Rounded,
    Circle,
    Bang,
    Cloud,
    Hexagon,
}

impl MindShape {
    pub(super) fn name(self) -> &'static str {
        match self {
            MindShape::Default => "default",
            MindShape::Square => "square",
            MindShape::Rounded => "rounded",
            MindShape::Circle => "circle",
            MindShape::Bang => "bang",
            MindShape::Cloud => "cloud",
            MindShape::Hexagon => "hexagon",
        }
    }
}

pub(super) struct MindNode {
    pub(super) label: String,
    pub(super) shape: MindShape,
    /// `::icon(fa fa-book)`: the icon classes.
    pub(super) icon: Option<String>,
    pub(super) depth: usize,
    pub(super) children: Vec<usize>,
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

const MIND_FONT: f32 = 12.5;
const MIND_LINE: f32 = 16.0;
const ICON_ROW: f32 = 20.0;

/// The short name shown for an icon: `fa fa-book` → `book`, `mdi mdi-skull-outline` → `skull-outline`.
pub(super) fn mindmap_icon_name(icon: &str) -> String {
    let last = icon.split_whitespace().last().unwrap_or(icon);
    let name = last.split_once('-').map_or(last, |(prefix, rest)| if ["fa", "fas", "far", "fab", "mdi", "bi", "ti"].contains(&prefix) { rest } else { last });
    name.to_string()
}

fn mindmap_size(node: &mut MindNode) {
    let lines = label_lines(&node.label, 200.0, MIND_FONT);
    let text_w = lines.iter().map(|l| estimate_text_width(l, MIND_FONT)).fold(0.0f32, f32::max);
    let icon_w = node.icon.as_deref().map_or(0.0, |i| estimate_text_width(&mindmap_icon_name(i), 9.5) + 34.0);
    let mut w = (text_w.max(icon_w) + 26.0).max(68.0);
    let base = if node.depth == 0 { 38.0 } else { 30.0 };
    let mut h = base + (lines.len().max(1) - 1) as f32 * MIND_LINE + if node.icon.is_some() { ICON_ROW } else { 0.0 };
    match node.shape {
        MindShape::Circle => {
            let d = (text_w.max(icon_w) + 22.0).max(h + 8.0).max(44.0);
            (w, h) = (d, d);
        }
        MindShape::Hexagon => w += 20.0,
        MindShape::Cloud | MindShape::Bang => {
            w += 28.0;
            h += 16.0;
        }
        _ => {}
    }
    node.w = w;
    node.h = h;
}

/// `id((label))` → (`label`, Circle); a line without a shape is the label itself.
pub(super) fn parse_mindmap_node(s: &str) -> (String, MindShape) {
    let s = s.trim();
    const DELIMS: &[(&str, &str, MindShape)] = &[
        ("((", "))", MindShape::Circle),
        ("))", "((", MindShape::Bang),
        ("{{", "}}", MindShape::Hexagon),
        ("(", ")", MindShape::Rounded),
        (")", "(", MindShape::Cloud),
        ("[", "]", MindShape::Square),
    ];
    if let Some(at) = s.find(['(', ')', '[', '{']) {
        let id = &s[..at];
        if !id.contains(char::is_whitespace) {
            for &(open, close, shape) in DELIMS {
                if s[at..].starts_with(open) && s.len() >= at + open.len() + close.len() && s.ends_with(close) {
                    let inner = &s[at + open.len()..s.len() - close.len()];
                    return (unquote(inner.trim()), shape);
                }
            }
        }
    }
    (unquote(s), MindShape::Default)
}

pub(super) fn render_mindmap_svg(source: &str, config: &MermaidConfig) -> Option<(Vec<u8>, u32, u32)> {
    let mut nodes: Vec<MindNode> = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new(); // (indent, node_idx)

    for raw in source.lines().skip(1) {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }
        // `::icon(...)` decorates the node above; `:::class` names CSS classes (not drawn).
        if let Some(rest) = trimmed.strip_prefix("::icon(") {
            if let Some(last) = nodes.last_mut() {
                last.icon = Some(rest.trim_end_matches(')').trim().to_string());
            }
            continue;
        }
        if trimmed.starts_with(":::") {
            continue;
        }
        let indent = raw.chars().take_while(|c| c.is_whitespace()).count();
        let (label, shape) = parse_mindmap_node(trimmed);
        if label.is_empty() {
            continue;
        }
        while let Some(&(top_indent, _)) = stack.last() {
            if top_indent >= indent {
                stack.pop();
            } else {
                break;
            }
        }
        let depth = stack.len();
        let idx = nodes.len();
        nodes.push(MindNode {
            label,
            shape,
            icon: None,
            depth,
            children: Vec::new(),
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        });
        if let Some(&(_, parent_idx)) = stack.last() {
            nodes[parent_idx].children.push(idx);
        }
        stack.push((indent, idx));
    }

    if nodes.is_empty() {
        return None;
    }
    for node in &mut nodes {
        mindmap_size(node);
    }

    let mut max_depth = 0usize;
    for n in &nodes {
        max_depth = max_depth.max(n.depth);
    }
    let mut depth_widths = vec![80.0f32; max_depth + 1];
    for n in &nodes {
        depth_widths[n.depth] = depth_widths[n.depth].max(n.w);
    }
    let mut depth_x = vec![28.0f32; max_depth + 1];
    for d in 1..=max_depth {
        depth_x[d] = depth_x[d - 1] + depth_widths[d - 1] + 44.0;
    }

    // Assign Y by leaf slots (each at least as tall as its node), X by depth.
    let leaf_h = 44.0f32;
    let margin_y = 24.0f32;
    fn layout_node(idx: usize, y_start: f32, leaf_h: f32, depth_x: &[f32], nodes: &mut [MindNode]) -> f32 {
        let children = nodes[idx].children.clone();
        nodes[idx].x = depth_x[nodes[idx].depth];
        let own = leaf_h.max(nodes[idx].h + 14.0);
        if children.is_empty() {
            nodes[idx].y = y_start + own / 2.0;
            return y_start + own;
        }
        let mut cursor = y_start;
        let mut first_cy = 0.0f32;
        let mut last_cy = 0.0f32;
        for (i, &c) in children.iter().enumerate() {
            cursor = layout_node(c, cursor, leaf_h, depth_x, nodes);
            let cy = nodes[c].y;
            if i == 0 {
                first_cy = cy;
            }
            last_cy = cy;
        }
        nodes[idx].y = (first_cy + last_cy) / 2.0;
        // A tall parent (a big circle) keeps room around itself.
        let top = nodes[idx].y - own / 2.0;
        if top < y_start {
            let shift = y_start - top;
            shift_subtree(idx, shift, nodes);
            cursor += shift;
        }
        cursor.max(nodes[idx].y + own / 2.0)
    }
    fn shift_subtree(idx: usize, dy: f32, nodes: &mut [MindNode]) {
        nodes[idx].y += dy;
        for c in nodes[idx].children.clone() {
            shift_subtree(c, dy, nodes);
        }
    }

    let mut total_y = margin_y;
    for i in 0..nodes.len() {
        if nodes[i].depth == 0 {
            total_y = layout_node(i, total_y, leaf_h, &depth_x, &mut nodes);
        }
    }

    let total_w = (depth_x[max_depth] + depth_widths[max_depth] + 28.0).ceil() as u32;
    let total_h = (total_y + margin_y).ceil() as u32;

    // Colours follow the branch (child of the root), or the depth with `mindmap.colorBy: depth`.
    // `themeVariables.cScaleN` / `cScaleLabelN` recolour colour N.
    const BRANCH_COLORS: &[&str] = &["#2563eb", "#059669", "#d97706", "#7c3aed", "#db2777", "#0891b2"];
    let by_depth = config.option("mindmap.colorBy").is_some_and(|v| v.eq_ignore_ascii_case("depth"));
    let mut branch = vec![0usize; nodes.len()];
    for n in &nodes {
        if n.depth == 0 {
            for (k, &c) in n.children.iter().enumerate() {
                branch[c] = k;
            }
        }
    }
    for idx in 0..nodes.len() {
        let b = branch[idx];
        for c in nodes[idx].children.clone() {
            if nodes[idx].depth > 0 {
                branch[c] = b;
            }
        }
    }
    let colour_of = |idx: usize| -> (String, String, String) {
        let n = if by_depth { nodes[idx].depth } else { branch[idx] };
        let scale = config.color_var(&format!("cScale{}", n % 12));
        let stroke = scale.clone().unwrap_or_else(|| BRANCH_COLORS[n % BRANCH_COLORS.len()].to_string());
        match scale {
            Some(fill) => (fill, stroke, config.color_var(&format!("cScaleLabel{}", n % 12)).unwrap_or_else(|| "#0f172a".to_string())),
            None => ("#f8fafc".to_string(), stroke, "#0f172a".to_string()),
        }
    };

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));

    // Curved edges
    for p in &nodes {
        for &c_idx in &p.children {
            let c = &nodes[c_idx];
            let sx = p.x + p.w;
            let sy = p.y;
            let ex = c.x;
            let ey = c.y;
            let mx = (sx + ex) / 2.0;
            let (_, color, _) = colour_of(c_idx);
            svg.push_str(&format!(
                r##"<path d="M {sx:.1} {sy:.1} C {mx:.1} {sy:.1}, {mx:.1} {ey:.1}, {ex:.1} {ey:.1}" fill="none" stroke="{color}" stroke-width="1.8" data-role="edge"/>"##
            ));
        }
    }

    // Nodes
    for (idx, n) in nodes.iter().enumerate() {
        let (fill, stroke, text_fill) = if n.depth == 0 {
            // `themeVariables.git0` / `gitBranchLabel0` colour the root, as in Mermaid.
            let fill = config.color_var("git0").unwrap_or_else(|| "#1e293b".to_string());
            let ink = config.color_var("gitBranchLabel0").unwrap_or_else(|| "#ffffff".to_string());
            (fill, "#0f172a".to_string(), ink)
        } else {
            colour_of(idx)
        };
        push_mind_shape(&mut svg, n, &fill, &stroke);
        let lines = label_lines(&n.label, 200.0, MIND_FONT);
        let icon_h = if n.icon.is_some() { ICON_ROW } else { 0.0 };
        let block = lines.len() as f32 * MIND_LINE;
        let cx = n.x + n.w / 2.0;
        let top = n.y - (block + icon_h) / 2.0;
        if let Some(icon) = &n.icon {
            // Icon fonts are not embedded: the icon is a badge with its name.
            let name = mindmap_icon_name(icon);
            let bw = estimate_text_width(&name, 9.5) + 26.0;
            let by = top - 2.0;
            svg.push_str(&format!(
                r##"<g data-icon="{}"><rect x="{:.1}" y="{by:.1}" width="{bw:.1}" height="16" rx="8" fill="#ffffff" fill-opacity="0.85" stroke="{stroke}" stroke-width="1"/><circle cx="{:.1}" cy="{:.1}" r="4" fill="{stroke}"/><text x="{:.1}" y="{:.1}" font-size="9.5" fill="#334155">{}</text></g>"##,
                escape_xml(icon),
                cx - bw / 2.0,
                cx - bw / 2.0 + 9.0,
                by + 8.0,
                cx - bw / 2.0 + 17.0,
                by + 11.5,
                escape_xml(&name)
            ));
        }
        // A Markdown string keeps its bold / italic runs; other labels are plain lines.
        let markups: Vec<String> = match markdown_string(&n.label) {
            Some(inner) => rich_lines(inner, 200.0, MIND_FONT).iter().map(|line| runs_markup(line)).collect(),
            None => lines.iter().map(|line| escape_xml(line)).collect(),
        };
        for (k, markup) in markups.iter().enumerate() {
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="{MIND_FONT}" font-weight="{}" fill="{text_fill}">{markup}</text>"##,
                top + icon_h + k as f32 * MIND_LINE + 12.0,
                if n.depth == 0 { "bold" } else { "normal" },
            ));
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

fn push_mind_shape(svg: &mut String, n: &MindNode, fill: &str, stroke: &str) {
    let (x, y, w, h) = (n.x, n.y - n.h / 2.0, n.w, n.h);
    let (cx, cy) = (x + w / 2.0, n.y);
    let attrs = format!(r##"fill="{fill}" stroke="{stroke}" stroke-width="1.5" data-shape="mindmap-{}""##, n.shape.name());
    match n.shape {
        MindShape::Default | MindShape::Square | MindShape::Rounded => {
            let rx = match n.shape {
                MindShape::Square => 0.0,
                MindShape::Rounded => (h / 2.0).min(14.0),
                _ if n.depth == 0 => 18.0f32.min(h / 2.0),
                _ => 8.0,
            };
            svg.push_str(&format!(r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="{rx:.1}" {attrs}/>"##));
        }
        MindShape::Circle => {
            svg.push_str(&format!(r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{:.1}" {attrs}/>"##, w / 2.0));
        }
        MindShape::Hexagon => {
            let inset = (h / 2.0).min(14.0);
            svg.push_str(&format!(
                r##"<polygon points="{:.1},{y:.1} {:.1},{y:.1} {:.1},{cy:.1} {:.1},{:.1} {:.1},{:.1} {x:.1},{cy:.1}" {attrs}/>"##,
                x + inset,
                x + w - inset,
                x + w,
                x + w - inset,
                y + h,
                x + inset,
                y + h
            ));
        }
        MindShape::Bang => {
            // A burst: alternating outer and inner points on an ellipse.
            let (rx, ry) = (w / 2.0, h / 2.0);
            let points: Vec<String> = (0..24)
                .map(|i| {
                    let a = i as f32 / 24.0 * std::f32::consts::TAU;
                    let k = if i % 2 == 0 { 1.0 } else { 0.8 };
                    format!("{:.1},{:.1}", cx + a.cos() * rx * k, cy + a.sin() * ry * k)
                })
                .collect();
            svg.push_str(&format!(r##"<polygon points="{}" {attrs}/>"##, points.join(" ")));
        }
        MindShape::Cloud => {
            // A cloud: arcs bulging out between points on an ellipse.
            let (rx, ry) = (w / 2.0 - 4.0, h / 2.0 - 4.0);
            let count = 10;
            let pts: Vec<(f32, f32)> = (0..count)
                .map(|i| {
                    let a = i as f32 / count as f32 * std::f32::consts::TAU;
                    (cx + a.cos() * rx, cy + a.sin() * ry)
                })
                .collect();
            let mut d = format!("M {:.1} {:.1}", pts[0].0, pts[0].1);
            for i in 0..count {
                let (x2, y2) = pts[(i + 1) % count];
                let (x1, y1) = pts[i];
                let r = ((x2 - x1).hypot(y2 - y1) / 2.0) * 1.05;
                d.push_str(&format!(" A {r:.1} {r:.1} 0 0 1 {x2:.1} {y2:.1}"));
            }
            svg.push_str(&format!(r##"<path d="{d} Z" {attrs}/>"##));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn mindmap_node_shapes() {
        let cases = [
            ("id[I am a square]", "I am a square", MindShape::Square),
            ("id(rounded)", "rounded", MindShape::Rounded),
            ("root((mindmap))", "mindmap", MindShape::Circle),
            ("id))I am a bang((", "I am a bang", MindShape::Bang),
            ("id)I am a cloud(", "I am a cloud", MindShape::Cloud),
            ("id{{hexagon}}", "hexagon", MindShape::Hexagon),
            ("a[\"Quoted label\"]", "Quoted label", MindShape::Square),
            ("Plain text (with a remark)", "Plain text (with a remark)", MindShape::Default),
            ("I am the default shape", "I am the default shape", MindShape::Default),
        ];
        for (src, label, shape) in cases {
            assert_eq!(parse_mindmap_node(src), (label.to_string(), shape), "{src}");
        }
        let svg = svg_of("mindmap\n  root((Root))\n    a[Square]\n    b(Rounded)\n    c))Bang((\n    d)Cloud(\n    e{{Hex}}\n    Plain\n");
        for shape in ["circle", "square", "rounded", "bang", "cloud", "hexagon", "default"] {
            assert!(svg.contains(&format!(r#"data-shape="mindmap-{shape}""#)), "{shape}: {svg}");
        }
        for label in ["Root", "Square", "Rounded", "Bang", "Cloud", "Hex", "Plain"] {
            assert!(svg.contains(&format!(">{label}<")), "{label}: {svg}");
        }
        assert!(!svg.contains("((") && !svg.contains("{{"), "{svg}");
        // A circle is round: its node is as tall as it is wide.
        let circle = tag_with(&svg, r#"data-shape="mindmap-circle""#)[0];
        assert!(attr_f32(circle, "r").unwrap() >= 22.0, "{circle}");
    }

    #[test]
    fn mindmap_icons_classes_and_colour_by_depth() {
        let svg = svg_of("mindmap\n  Root\n    A\n    ::icon(fa fa-book)\n    B(B)\n    ::icon(mdi mdi-skull-outline)\n    C\n    :::urgent large\n");
        assert!(svg.contains(r#"data-icon="fa fa-book""#) && svg.contains(">book<"), "{svg}");
        assert!(svg.contains(r#"data-icon="mdi mdi-skull-outline""#) && svg.contains(">skull-outline<"), "{svg}");
        assert!(!svg.contains("::icon") && !svg.contains("urgent"), "icon and class lines are not nodes: {svg}");
        assert_eq!(mindmap_icon_name("fa fa-book"), "book");
        assert_eq!(mindmap_icon_name("custom"), "custom");

        let body = "mindmap\n  Root\n    A\n      A1\n    B\n      B1\n";
        let stroke_of = |svg: &str, label: &str| -> String {
            let at = svg.find(&format!(">{label}<")).unwrap();
            let shape = svg[..at].rfind("data-shape=").unwrap();
            let tag_start = svg[..shape].rfind('<').unwrap();
            let tag = &svg[tag_start..shape];
            tag.split("stroke=\"").nth(1).unwrap().split('"').next().unwrap().to_string()
        };
        // By branch (default): a child shares its branch's colour, branches differ.
        let branch = svg_of(body);
        assert_eq!(stroke_of(&branch, "A"), stroke_of(&branch, "A1"));
        assert_ne!(stroke_of(&branch, "A"), stroke_of(&branch, "B"));
        // `colorBy: depth`: a level shares one colour.
        let depth = svg_of(&format!("---\nconfig:\n  mindmap:\n    colorBy: depth\n---\n{body}"));
        assert_eq!(stroke_of(&depth, "A"), stroke_of(&depth, "B"));
        assert_ne!(stroke_of(&depth, "A"), stroke_of(&depth, "A1"));
        let scaled = svg_of(&format!("---\nconfig:\n  themeVariables:\n    cScale0: '#bfdbfe'\n    cScaleLabel0: '#172554'\n---\n{body}"));
        assert!(scaled.contains(r##"fill="#bfdbfe""##) && scaled.contains(r##"fill="#172554">A<"##), "{scaled}");
    }

    #[test]
    fn mindmap_root_colours_and_markdown_strings() {
        let svg = svg_of("---\nconfig:\n  themeVariables:\n    git0: '#ff0000'\n    gitBranchLabel0: '#ffffff'\n---\nmindmap\n  root((mindmap))\n    A\n");
        assert!(svg.contains(r##"fill="#ff0000""##), "{svg}");
        let src = "mindmap\n    id1[\"`**Root** with\na second line`\"]\n      id2[\"`The dog in **the** hog... a *very long text* that wraps to a new line`\"]\n      id3[Regular labels still work]\n";
        let svg = svg_of(src);
        assert!(svg.contains(r#"<tspan font-weight="bold">Root</tspan> with</text>"#), "{svg}");
        assert!(svg.contains(">a second line</text>"));
        assert!(svg.contains(r#"<tspan font-style="italic">"#));
        assert!(!svg.contains('`') && !svg.contains("**"), "{svg}");
        assert!(svg.contains(">Regular labels still work</text>"));
    }
}
