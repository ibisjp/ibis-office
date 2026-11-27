// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `treemap-beta`: an indented hierarchy of sections and valued leaves, drawn as a
//! squarified treemap (sections get a title band, leaves show their value).

use super::*;

const PALETTE: &[&str] = &["#4e79a7", "#f28e2c", "#59a14f", "#e15759", "#76b7b2", "#af7aa1", "#edc949", "#ff9da7", "#9c755f", "#bab0ab"];
const HEADER: f32 = 22.0;
const INSET: f32 = 3.0;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct TreeNode {
    pub(super) name: String,
    /// `None` for a section.
    pub(super) value: Option<f64>,
    pub(super) class: Option<String>,
    pub(super) children: Vec<usize>,
    pub(super) parent: Option<usize>,
}

#[derive(Debug, Default)]
pub(super) struct Treemap {
    pub(super) title: Option<String>,
    pub(super) nodes: Vec<TreeNode>,
    pub(super) roots: Vec<usize>,
    pub(super) class_defs: HashMap<String, FlowStyle>,
}

/// `"Name"`, `"Name": 10`, `"Name":::cls`, `"Name": 10:::cls` → (name, value, class).
pub(super) fn parse_tree_line(line: &str) -> Option<(String, Option<f64>, Option<String>)> {
    let (body, class) = match line.rfind(":::") {
        Some(at) => (&line[..at], Some(line[at + 3..].trim().to_string()).filter(|c| !c.is_empty())),
        None => (line, None),
    };
    let body = body.trim();
    let (name, rest) = if let Some(inner) = body.strip_prefix('"') {
        let close = inner.find('"')?;
        (inner[..close].to_string(), inner[close + 1..].trim())
    } else {
        match body.split_once(':') {
            Some((n, r)) => (n.trim().to_string(), r.trim()),
            None => (body.to_string(), ""),
        }
    };
    let value = match rest.strip_prefix(':').unwrap_or(rest).trim() {
        "" => None,
        v => Some(v.parse::<f64>().ok().filter(|n| n.is_finite())?),
    };
    (!name.is_empty()).then_some((name, value, class))
}

pub(super) fn parse_treemap(source: &str) -> Treemap {
    let mut t = Treemap::default();
    // (indent, node) of the open sections.
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for raw in source.lines().skip(1) {
        let code = raw.split_once("%%").map_or(raw, |(c, _)| c);
        let line = code.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("title ") {
            t.title = Some(unquote(rest.trim()));
            continue;
        }
        if let Some(rest) = line.strip_prefix("classDef ") {
            let rest = rest.trim().trim_end_matches(';');
            let (names, props) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let style = parse_flow_style(props);
            for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                t.class_defs.entry(name.to_string()).or_default().merge(&style);
            }
            continue;
        }
        let Some((name, value, class)) = parse_tree_line(line) else { continue };
        let indent = code.len() - code.trim_start().len();
        while stack.last().is_some_and(|&(i, _)| i >= indent) {
            stack.pop();
        }
        let parent = stack.last().map(|&(_, n)| n);
        let idx = t.nodes.len();
        t.nodes.push(TreeNode { name, value, class, children: Vec::new(), parent });
        match parent {
            Some(p) => t.nodes[p].children.push(idx),
            None => t.roots.push(idx),
        }
        stack.push((indent, idx));
    }
    t
}

/// A leaf's value, or the sum below a section.
pub(super) fn tree_total(t: &Treemap, n: usize) -> f64 {
    let node = &t.nodes[n];
    if node.children.is_empty() {
        node.value.unwrap_or(0.0).max(0.0)
    } else {
        node.children.iter().map(|&c| tree_total(t, c)).sum()
    }
}

/// Squarified treemap (Bruls et al.): rectangles for `values` (in their order) filling the box.
pub(super) fn squarify(values: &[f64], x: f32, y: f32, w: f32, h: f32) -> Vec<(f32, f32, f32, f32)> {
    let mut out = vec![(x, y, 0.0, 0.0); values.len()];
    let total: f64 = values.iter().sum();
    if total <= 0.0 || w <= 0.0 || h <= 0.0 {
        return out;
    }
    let mut order: Vec<usize> = (0..values.len()).filter(|&i| values[i] > 0.0).collect();
    order.sort_by(|&a, &b| values[b].total_cmp(&values[a]));
    let scale = (w as f64 * h as f64) / total;
    let areas: Vec<f64> = order.iter().map(|&i| values[i] * scale).collect();
    let (mut bx, mut by, mut bw, mut bh) = (x as f64, y as f64, w as f64, h as f64);
    let worst = |row: &[f64], side: f64| {
        let sum: f64 = row.iter().sum();
        let (mx, mn) = row.iter().fold((0.0f64, f64::MAX), |(mx, mn), &a| (mx.max(a), mn.min(a)));
        ((side * side * mx) / (sum * sum)).max((sum * sum) / (side * side * mn))
    };
    let mut start = 0;
    while start < areas.len() {
        let side = bw.min(bh);
        let mut end = start + 1;
        while end < areas.len() && worst(&areas[start..=end], side) <= worst(&areas[start..end], side) {
            end += 1;
        }
        let row = &areas[start..end];
        let sum: f64 = row.iter().sum();
        if bw >= bh {
            // A column along the left side.
            let cw = sum / bh;
            let mut cy = by;
            for (k, &a) in row.iter().enumerate() {
                let ch = a / cw;
                out[order[start + k]] = (bx as f32, cy as f32, cw as f32, ch as f32);
                cy += ch;
            }
            bx += cw;
            bw -= cw;
        } else {
            let rh = sum / bw;
            let mut cx = bx;
            for (k, &a) in row.iter().enumerate() {
                let cw = a / rh;
                out[order[start + k]] = (cx as f32, by as f32, cw as f32, rh as f32);
                cx += cw;
            }
            by += rh;
            bh -= rh;
        }
        start = end;
    }
    out
}

fn group_thousands(int: &str) -> String {
    let mut out = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// The subset of d3-format that `treemap.valueFormat` uses: `$` prefix, `,` grouping,
/// `.Nf` decimals and `.N%` percentages (`$0,0`, `,.2f`, `.1%`).
pub(super) fn format_tree_value(value: f64, format: &str) -> String {
    let format = format.trim();
    let currency = format.starts_with('$');
    let precision = format
        .split_once('.')
        .and_then(|(_, p)| p.trim_end_matches(['f', '%']).parse::<usize>().ok());
    let percent = format.ends_with('%');
    let group = format.contains(',') || format.is_empty();
    let v = if percent { value * 100.0 } else { value };
    let text = match precision {
        Some(p) => format!("{:.*}", p.min(10), v.abs()),
        None if v.fract() == 0.0 || percent => format!("{:.0}", v.abs()),
        None => {
            let s = format!("{:.2}", v.abs());
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        }
    };
    let (int, frac) = text.split_once('.').map_or((text.as_str(), None), |(i, f)| (i, Some(f)));
    let mut out = String::new();
    if v < 0.0 {
        out.push('-');
    }
    if currency {
        out.push('$');
    }
    out.push_str(&if group { group_thousands(int) } else { int.to_string() });
    if let Some(f) = frac {
        out.push('.');
        out.push_str(f);
    }
    if percent {
        out.push('%');
    }
    out
}

fn mix_white(hex: &str, t: f32) -> String {
    match hex_rgb(hex) {
        Some((r, g, b)) => {
            let m = |c: f32| ((c + (1.0 - c) * t) * 255.0).round().clamp(0.0, 255.0) as u8;
            format!("#{:02x}{:02x}{:02x}", m(r), m(g), m(b))
        }
        None => hex.to_string(),
    }
}

/// Dark text on a light fill, white on a dark one.
fn text_on(fill: &str) -> String {
    match hex_rgb(fill) {
        Some((r, g, b)) if 0.299 * r + 0.587 * g + 0.114 * b > 0.62 => "#0f172a".to_string(),
        _ => "#ffffff".to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_node(
    t: &Treemap,
    n: usize,
    rect: (f32, f32, f32, f32),
    color: &str,
    depth: usize,
    format: &str,
    palette: Option<&[String]>,
    svg: &mut String,
) {
    let (x, y, w, h) = rect;
    if w < 1.0 || h < 1.0 {
        return;
    }
    let node = &t.nodes[n];
    let style = node.class.as_deref().and_then(|c| t.class_defs.get(c)).cloned().unwrap_or_default();
    let fill = style.fill.as_deref().and_then(safe_css_value);
    let stroke = style.stroke.as_deref().and_then(safe_css_value).unwrap_or_else(|| "#ffffff".to_string());
    let stroke_w = style.stroke_width.as_deref().and_then(safe_css_value).unwrap_or_else(|| "1.5".to_string());
    let text = style.color.as_deref().and_then(safe_css_value);
    let name = escape_attr(&node.name);
    if node.children.is_empty() {
        let fill = fill.unwrap_or_else(|| mix_white(color, 0.15 + 0.15 * depth.min(3) as f32));
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_w}" data-leaf="{name}"/>"##
        ));
        let text = text.unwrap_or_else(|| text_on(&fill));
        let value = format_tree_value(node.value.unwrap_or(0.0), format);
        let size = if w > 90.0 && h > 44.0 { 13.0 } else { 11.0 };
        let fits = |s: &str, f: f32| estimate_text_width(s, f) <= w - 8.0;
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        if h >= 34.0 && fits(&node.name, size) {
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="{size}" font-weight="bold" fill="{text}">{}</text>"##,
                cy - 2.0,
                escape_xml(&node.name)
            ));
            if fits(&value, size - 1.0) {
                svg.push_str(&format!(
                    r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="{}" fill="{text}">{}</text>"##,
                    cy + size + 1.0,
                    size - 1.0,
                    escape_xml(&value)
                ));
            }
        } else if h >= 16.0 && fits(&node.name, 10.0) {
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="10" fill="{text}">{}</text>"##,
                cy + 3.5,
                escape_xml(&node.name)
            ));
        }
        return;
    }
    let fill = fill.unwrap_or_else(|| if depth == 0 { color.to_string() } else { mix_white(color, 0.12 * depth as f32) });
    svg.push_str(&format!(
        r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_w}" data-section="{name}"/>"##
    ));
    let text = text.unwrap_or_else(|| text_on(&fill));
    let total = format_tree_value(tree_total(t, n), format);
    let header = HEADER.min(h / 3.0);
    if header >= 14.0 {
        let title = escape_xml(&node.name);
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" font-size="12" font-weight="bold" fill="{text}">{title}</text>"##,
            x + 6.0,
            y + header - 7.0
        ));
        if estimate_text_width(&node.name, 12.0) + estimate_text_width(&total, 11.0) + 20.0 < w {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="11" fill="{text}">{}</text>"##,
                x + w - 6.0,
                y + header - 7.0,
                escape_xml(&total)
            ));
        }
    }
    let values: Vec<f64> = node.children.iter().map(|&c| tree_total(t, c)).collect();
    let inner = (x + INSET, y + header, w - INSET * 2.0, h - header - INSET);
    for (k, r) in squarify(&values, inner.0, inner.1, inner.2, inner.3).into_iter().enumerate() {
        // Under a lone top section the colours start one level down, so siblings still differ.
        let child_color = match palette {
            Some(p) if depth == 0 => p[k % p.len()].clone(),
            _ => color.to_string(),
        };
        draw_node(t, node.children[k], r, &child_color, depth + 1, format, None, svg);
    }
}

pub(super) fn render_treemap_svg(source: &str, config: &MermaidConfig) -> Option<(Vec<u8>, u32, u32)> {
    let t = parse_treemap(source);
    if t.roots.is_empty() {
        return None;
    }
    let number = |key: &str, default: f32, lo: f32, hi: f32| config.option(key).and_then(px_number).map_or(default, |v| (v as f32).clamp(lo, hi));
    let pad = number("treemap.diagramPadding", 8.0, 0.0, 400.0);
    let width = number("treemap.width", 800.0, 200.0, 3000.0);
    let height = number("treemap.height", 480.0, 100.0, 3000.0);
    let format = config.option("treemap.valueFormat").map(unquote).unwrap_or_else(|| ",".to_string());
    let grand = t.roots.iter().map(|&r| tree_total(&t, r)).sum::<f64>();
    let total_label = format_tree_value(grand, &format);
    // mermaid.js shows the diagram total when there are several top-level sections.
    let top_band = if t.roots.len() > 1 { 22.0 } else { 0.0 };
    let total_w = (width + pad * 2.0).ceil();
    let total_h = (height + pad * 2.0 + top_band).ceil();
    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));
    if t.roots.len() > 1 {
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="13" fill="#334155" data-role="treemap-total">{}</text>"##,
            pad + width,
            pad + 14.0,
            escape_xml(&total_label)
        ));
    }
    let values: Vec<f64> = t.roots.iter().map(|&r| tree_total(&t, r)).collect();
    let palette: Vec<String> = (0..PALETTE.len())
        .map(|k| config.color_var(&format!("cScale{k}")).unwrap_or_else(|| PALETTE[k].to_string()))
        .collect();
    let lone = t.roots.len() == 1;
    for (k, rect) in squarify(&values, pad, pad + top_band, width, height).into_iter().enumerate() {
        // A lone top section is drawn neutral; its children carry the colours.
        let color = if lone { "#475569".to_string() } else { palette[k % palette.len()].clone() };
        draw_node(&t, t.roots[k], rect, &color, 0, &format, lone.then_some(palette.as_slice()), &mut svg);
    }
    svg.push_str("</svg>");
    let bytes = svg.into_bytes();
    Some(match &t.title {
        Some(title) => add_svg_title(bytes, total_w as u32, total_h as u32, title),
        None => (bytes, total_w as u32, total_h as u32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn treemap_lines_indentation_and_classes() {
        assert_eq!(parse_tree_line(r#""Leaf 2.1": 20:::class1"#), Some(("Leaf 2.1".into(), Some(20.0), Some("class1".into()))));
        assert_eq!(parse_tree_line(r#""B":::important"#), Some(("B".into(), None, Some("important".into()))));
        assert_eq!(parse_tree_line(r#""Men's": 40"#), Some(("Men's".into(), Some(40.0), None)));
        let t = parse_treemap("treemap-beta\n\"Products\"\n    \"Electronics\"\n        \"Phones\": 50\n        \"Computers\": 30\n    \"Clothing\"\n        \"Men's\": 40\n\"Other\": 5\nclassDef c fill:red;");
        assert_eq!(t.roots.len(), 2);
        assert_eq!(t.nodes[0].children, vec![1, 4]);
        assert_eq!(t.nodes[1].children, vec![2, 3]);
        assert_eq!(tree_total(&t, 0), 120.0);
        assert_eq!(t.class_defs["c"].fill.as_deref(), Some("red"));
    }

    #[test]
    fn squarify_fills_the_box_in_proportion() {
        let values = [6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0];
        let rects = squarify(&values, 0.0, 0.0, 600.0, 400.0);
        let total: f32 = rects.iter().map(|r| r.2 * r.3).sum();
        assert!((total - 240000.0).abs() < 1.0);
        for (r, v) in rects.iter().zip(values) {
            assert!(((r.2 * r.3) - v as f32 * 10000.0).abs() < 1.0, "{r:?} {v}");
            assert!(r.0 >= -0.01 && r.1 >= -0.01 && r.0 + r.2 <= 600.01 && r.1 + r.3 <= 400.01);
            // Squarified: no sliver.
            assert!(r.2.max(r.3) / r.2.min(r.3) < 3.0, "{r:?}");
        }
        for (i, a) in rects.iter().enumerate() {
            for b in &rects[i + 1..] {
                assert!(!rects_overlap(*a, *b), "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn value_formats() {
        assert_eq!(format_tree_value(700000.0, "$0,0"), "$700,000");
        assert_eq!(format_tree_value(0.35, "$.1%"), "$35.0%");
        assert_eq!(format_tree_value(0.35, ".1%"), "35.0%");
        assert_eq!(format_tree_value(1234.5, ",.2f"), "1,234.50");
        assert_eq!(format_tree_value(1234.0, ","), "1,234");
        assert_eq!(format_tree_value(12.25, ","), "12.25");
    }

    #[test]
    fn treemap_svg_sections_leaves_classes_and_config() {
        let svg = svg_of("---\nconfig:\n  treemap:\n    valueFormat: '$0,0'\n    diagramPadding: 50\n---\ntreemap-beta\n\"Budget\"\n    \"Ops\": 700000\n    \"Ads\":::hot\n        \"TV\": 300000\nclassDef hot fill:#f96,color:blue;");
        assert!(svg.contains(r#"data-section="Budget""#) && svg.contains(r#"data-leaf="Ops""#));
        assert!(svg.contains(">$700,000<") && svg.contains(">$1,000,000<"));
        assert!(svg.contains(r##"fill="#f96" stroke="#ffffff" stroke-width="1.5" data-section="Ads""##));
        let (w, h) = view_box(&svg);
        assert_eq!((w, h), (900.0, 580.0));
    }

    #[test]
    fn treemap_shows_the_grand_total_for_several_roots() {
        let svg = svg_of("treemap-beta\n\"Category A\"\n    \"Item A1\": 10\n    \"Item A2\": 20\n\"Category B\"\n    \"Item B1\": 15\n    \"Item B2\": 25\n");
        assert!(svg.contains(r#"data-role="treemap-total">70</text>"#), "{svg}");
        let nested = svg_of("treemap-beta\n\"Section 1\"\n    \"Leaf 1.1\": 12\n    \"Section 1.2\"\n      \"Leaf 1.2.1\": 12\n\"Section 2\"\n    \"Leaf 2.1\": 20\n    \"Leaf 2.2\": 25\n    \"Leaf 2.3\": 12\n");
        assert!(nested.contains(r#"data-role="treemap-total">81</text>"#), "{nested}");
    }
}
