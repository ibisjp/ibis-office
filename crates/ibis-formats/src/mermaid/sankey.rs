// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `sankey` / `sankey-beta`: CSV flows (`source,target,value`) as a d3-sankey style layout.

use super::*;

#[derive(Debug, PartialEq)]
pub(super) struct SankeyLink {
    pub(super) source: usize,
    pub(super) target: usize,
    pub(super) value: f64,
}

/// One CSV record; `"a, b"` and `""` (an escaped quote) inside quotes.
pub(super) fn sankey_csv_fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    fields.push(field);
    fields.into_iter().map(|f| f.trim().to_string()).collect()
}

/// Node names in order of first appearance, and the links between them.
pub(super) fn parse_sankey(source: &str) -> (Vec<String>, Vec<SankeyLink>) {
    let mut names: Vec<String> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut links = Vec::new();
    for raw in source.lines().skip(1) {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        let fields = sankey_csv_fields(l);
        let [from, to, value] = fields.as_slice() else { continue };
        let Ok(value) = value.parse::<f64>() else { continue };
        if !value.is_finite() || value <= 0.0 || from.is_empty() || to.is_empty() {
            continue;
        }
        let mut id = |name: &str| {
            *index.entry(name.to_string()).or_insert_with(|| {
                names.push(name.to_string());
                names.len() - 1
            })
        };
        let (source, target) = (id(from), id(to));
        if source != target {
            links.push(SankeyLink { source, target, value });
        }
    }
    (names, links)
}

const SANKEY_COLORS: &[&str] = &["#4e79a7", "#f28e2c", "#e15759", "#76b7b2", "#59a14f", "#edc949", "#af7aa1", "#ff9da7", "#9c755f", "#bab0ab"];

/// Node rectangles `(x, y, height)` and, per link, the link's width and its y at both ends.
pub(super) struct SankeyLayout {
    pub(super) depth: Vec<usize>,
    pub(super) nodes: Vec<(f32, f32, f32)>,
    pub(super) links: Vec<(f32, f32, f32)>,
}

/// A d3-sankey style layout: columns by longest path (sinks pushed right for `justify`),
/// heights by value, a few relaxation passes, then collisions resolved per column.
pub(super) fn layout_sankey(n: usize, links: &[SankeyLink], width: f32, height: f32, node_w: f32, padding: f32, align: &str) -> SankeyLayout {
    let mut depth = vec![0usize; n];
    // Longest path from the sources; at most n passes, so a cycle cannot loop forever.
    for _ in 0..n {
        let mut changed = false;
        for l in links {
            if depth[l.target] < depth[l.source] + 1 && depth[l.source] + 1 < n {
                depth[l.target] = depth[l.source] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let max_depth = depth.iter().copied().max().unwrap_or(0);
    let has_out: Vec<bool> = (0..n).map(|i| links.iter().any(|l| l.source == i)).collect();
    let has_in: Vec<bool> = (0..n).map(|i| links.iter().any(|l| l.target == i)).collect();
    match align {
        "left" => {}
        "right" => {
            // Height from the sinks.
            let mut height_of = vec![0usize; n];
            for _ in 0..n {
                for l in links {
                    height_of[l.source] = height_of[l.source].max(height_of[l.target] + 1).min(n);
                }
            }
            for i in 0..n {
                depth[i] = max_depth.saturating_sub(height_of[i]);
            }
        }
        "center" => {
            for i in 0..n {
                if !has_in[i] && has_out[i] {
                    let min_child = links.iter().filter(|l| l.source == i).map(|l| depth[l.target]).min().unwrap_or(1);
                    depth[i] = min_child.saturating_sub(1);
                }
            }
        }
        _ => {
            for i in 0..n {
                if !has_out[i] {
                    depth[i] = max_depth;
                }
            }
        }
    }
    let value: Vec<f64> = (0..n)
        .map(|i| {
            let out: f64 = links.iter().filter(|l| l.source == i).map(|l| l.value).sum();
            let inn: f64 = links.iter().filter(|l| l.target == i).map(|l| l.value).sum();
            out.max(inn)
        })
        .collect();
    let columns: Vec<Vec<usize>> = (0..=max_depth).map(|d| (0..n).filter(|&i| depth[i] == d).collect()).collect();
    let ky = columns
        .iter()
        .filter(|c| !c.is_empty())
        .map(|c| {
            let sum: f64 = c.iter().map(|&i| value[i]).sum();
            ((height - (c.len() as f32 - 1.0) * padding).max(10.0) as f64 / sum.max(f64::EPSILON)) as f32
        })
        .fold(f32::MAX, f32::min);
    let h: Vec<f32> = value.iter().map(|v| (*v as f32 * ky).max(1.0)).collect();
    let x_of = |d: usize| if max_depth == 0 { 0.0 } else { d as f32 * (width - node_w) / max_depth as f32 };
    let mut y = vec![0.0f32; n];
    for c in &columns {
        let mut cursor = 0.0;
        for &i in c {
            y[i] = cursor;
            cursor += h[i] + padding;
        }
    }
    let resolve = |y: &mut Vec<f32>| {
        for c in &columns {
            let mut order = c.clone();
            order.sort_by(|a, b| y[*a].total_cmp(&y[*b]));
            let mut cursor = 0.0f32;
            for &i in &order {
                y[i] = y[i].max(cursor);
                cursor = y[i] + h[i] + padding;
            }
            let mut bottom = height;
            for &i in order.iter().rev() {
                y[i] = y[i].min(bottom - h[i]).max(0.0);
                bottom = y[i] - padding;
            }
        }
    };
    // Relaxation: move each node toward the weighted centre of its neighbours.
    for pass in 0..6 {
        let alpha = 0.99f32.powi(pass);
        let mut next = y.clone();
        for i in 0..n {
            let (mut sum, mut weight) = (0.0f32, 0.0f32);
            for l in links {
                let other = if l.source == i { l.target } else if l.target == i { l.source } else { continue };
                sum += (y[other] + h[other] / 2.0) * l.value as f32;
                weight += l.value as f32;
            }
            if weight > 0.0 {
                next[i] += (sum / weight - h[i] / 2.0 - y[i]) * alpha * 0.5;
            }
        }
        y = next;
        resolve(&mut y);
    }

    // Link ends: stacked on each node in the order of the node at the other end.
    let mut out_order: Vec<usize> = (0..links.len()).collect();
    out_order.sort_by(|&a, &b| y[links[a].target].total_cmp(&y[links[b].target]));
    let mut in_order: Vec<usize> = (0..links.len()).collect();
    in_order.sort_by(|&a, &b| y[links[a].source].total_cmp(&y[links[b].source]));
    let mut out_used = vec![0.0f32; n];
    let mut in_used = vec![0.0f32; n];
    let mut ends = vec![(0.0f32, 0.0f32, 0.0f32); links.len()];
    for &k in &out_order {
        let w = links[k].value as f32 * ky;
        let s = links[k].source;
        ends[k].0 = w;
        ends[k].1 = y[s] + out_used[s] + w / 2.0;
        out_used[s] += w;
    }
    for &k in &in_order {
        let w = links[k].value as f32 * ky;
        let t = links[k].target;
        ends[k].2 = y[t] + in_used[t] + w / 2.0;
        in_used[t] += w;
    }
    SankeyLayout { nodes: (0..n).map(|i| (x_of(depth[i]), y[i], h[i])).collect(), links: ends, depth }
}

/// Baselines at least `gap` apart, as close to `desired` (sorted) as they can be, within `lo..=hi`.
pub(super) fn spread_labels(desired: &[f32], gap: f32, lo: f32, hi: f32) -> Vec<f32> {
    let mut out: Vec<f32> = Vec::with_capacity(desired.len());
    for &d in desired {
        let floor = out.last().map_or(lo, |&prev| prev + gap);
        out.push(d.max(floor));
    }
    // Push back up from the bottom when the stack ran past it.
    let mut ceiling = hi;
    for y in out.iter_mut().rev() {
        *y = y.min(ceiling);
        ceiling = *y - gap;
    }
    // A stack taller than the space starts at the top.
    let mut floor = lo;
    for y in out.iter_mut() {
        *y = y.max(floor);
        floor = *y + gap;
    }
    out
}

pub(super) fn render_sankey_svg(source: &str, config: &MermaidConfig) -> Option<(Vec<u8>, u32, u32)> {
    let (names, links) = parse_sankey(source);
    if links.is_empty() {
        return None;
    }
    let number = |key: &str, default: f32, lo: f32, hi: f32| {
        config.option(key).and_then(|v| px_number(v)).map_or(default, |v| (v as f32).clamp(lo, hi))
    };
    let width = number("sankey.width", 600.0, 200.0, 3000.0);
    let height = number("sankey.height", 400.0, 100.0, 3000.0);
    let node_w = number("sankey.nodeWidth", 10.0, 2.0, 60.0);
    let padding = number("sankey.nodePadding", 10.0, 0.0, 80.0);
    let show_values = config.flag("sankey.showValues", true);
    let outlined = config.option("sankey.labelStyle").is_some_and(|v| unquote(v) == "outlined");
    let align = config.option("sankey.nodeAlignment").map(unquote).unwrap_or_else(|| "justify".to_string());
    let link_color = config.option("sankey.linkColor").map(unquote).unwrap_or_else(|| "gradient".to_string());
    let prefix = config.option("sankey.prefix").map(unquote).unwrap_or_default();
    let suffix = config.option("sankey.suffix").map(unquote).unwrap_or_default();

    let layout = layout_sankey(names.len(), &links, width, height, node_w, padding, &align);
    // `sankey.nodeColors.<name>` overrides a node's colour.
    let colors: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            config
                .option(&format!("sankey.nodeColors.{name}"))
                .and_then(css_color)
                .unwrap_or_else(|| SANKEY_COLORS[i % SANKEY_COLORS.len()].to_string())
        })
        .collect();

    let margin = 8.0f32;
    let total_w = (width + 2.0 * margin).ceil() as u32;
    let total_h = (height + 2.0 * margin).ceil() as u32;
    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));
    svg.push_str(&format!(r#"<g transform="translate({margin} {margin})">"#));

    let mut defs = String::new();
    let mut paths = String::new();
    for (k, link) in links.iter().enumerate() {
        let (w, y0, y1) = layout.links[k];
        let x0 = layout.nodes[link.source].0 + node_w;
        let x1 = layout.nodes[link.target].0;
        let mx = (x0 + x1) / 2.0;
        let stroke = match link_color.as_str() {
            "source" => colors[link.source].clone(),
            "target" => colors[link.target].clone(),
            "gradient" => {
                defs.push_str(&format!(
                    r##"<linearGradient id="sankey-link-{k}" gradientUnits="userSpaceOnUse" x1="{x0:.1}" x2="{x1:.1}"><stop offset="0" stop-color="{}"/><stop offset="1" stop-color="{}"/></linearGradient>"##,
                    colors[link.source], colors[link.target]
                ));
                format!("url(#sankey-link-{k})")
            }
            other => css_color(other).unwrap_or_else(|| "#94a3b8".to_string()),
        };
        paths.push_str(&format!(
            r##"<path d="M {x0:.1} {y0:.1} C {mx:.1} {y0:.1}, {mx:.1} {y1:.1}, {x1:.1} {y1:.1}" fill="none" stroke="{stroke}" stroke-opacity="0.5" stroke-width="{:.1}" data-role="sankey-link"/>"##,
            w.max(1.0)
        ));
    }
    if !defs.is_empty() {
        svg.push_str(&format!("<defs>{defs}</defs>"));
    }
    svg.push_str(&paths);

    let value_of = |i: usize| {
        let out: f64 = links.iter().filter(|l| l.source == i).map(|l| l.value).sum();
        let inn: f64 = links.iter().filter(|l| l.target == i).map(|l| l.value).sum();
        out.max(inn)
    };
    // Labels face the middle: right of the left half's nodes, left of the right half's.
    let right_side: Vec<bool> = layout.nodes.iter().map(|&(x, _, _)| x > width / 2.0).collect();
    let mut label_y: Vec<f32> = layout.nodes.iter().map(|&(_, y, h)| y + h / 2.0 + 4.0).collect();
    // Labels of one column and side keep a line apart, so small neighbouring nodes stay readable.
    let mut sides: Vec<(usize, bool)> = (0..names.len()).map(|i| (layout.depth[i], right_side[i])).collect();
    sides.sort();
    sides.dedup();
    for side in sides {
        let mut idxs: Vec<usize> = (0..names.len()).filter(|&i| (layout.depth[i], right_side[i]) == side).collect();
        idxs.sort_by(|&a, &b| label_y[a].total_cmp(&label_y[b]));
        let desired: Vec<f32> = idxs.iter().map(|&i| label_y[i]).collect();
        for (&i, y) in idxs.iter().zip(spread_labels(&desired, 13.0, 10.0, height + 4.0)) {
            label_y[i] = y;
        }
    }
    for (i, name) in names.iter().enumerate() {
        let (x, y, h) = layout.nodes[i];
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{node_w:.1}" height="{h:.1}" fill="{}" data-role="sankey-node"/>"##,
            colors[i]
        ));
        let value = (value_of(i) * 100.0).round() / 100.0;
        let label = if show_values { format!("{name} {prefix}{value}{suffix}") } else { name.clone() };
        let (edge_x, lx, anchor) =
            if right_side[i] { (x, x - 6.0, "end") } else { (x + node_w, x + node_w + 6.0, "start") };
        let ty = label_y[i];
        let natural = y + h / 2.0 + 4.0;
        if (ty - natural).abs() > 2.0 {
            // A moved label points back at its node.
            let tip = if right_side[i] { lx + 1.0 } else { lx - 1.0 };
            svg.push_str(&format!(
                r##"<line x1="{edge_x:.1}" y1="{:.1}" x2="{tip:.1}" y2="{:.1}" stroke="#94a3b8" stroke-width="0.6" data-role="label-leader"/>"##,
                natural - 4.0,
                ty - 4.0
            ));
        }
        let halo = if outlined { r##" stroke="#ffffff" stroke-width="3" paint-order="stroke""## } else { "" };
        svg.push_str(&format!(
            r##"<text x="{lx:.1}" y="{ty:.1}" text-anchor="{anchor}" font-size="11" fill="#0f172a"{halo}>{}</text>"##,
            escape_xml(&label)
        ));
    }
    svg.push_str("</g></svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn sankey_csv_quotes_and_blank_lines() {
        assert_eq!(sankey_csv_fields(r#"Pumped heat,"Heating and cooling, ""homes""",193.026"#), vec!["Pumped heat", "Heating and cooling, \"homes\"", "193.026"]);
        let (names, links) = parse_sankey("sankey\n\n%% source,target,value\nA,B,10\n\nB,C,4\nB,D,6\nbad line\nA,A,1\n");
        assert_eq!(names, vec!["A", "B", "C", "D"]);
        assert_eq!(links.len(), 3);
        assert_eq!(links[1], SankeyLink { source: 1, target: 2, value: 4.0 });
    }

    #[test]
    fn sankey_layout_columns_heights_and_conservation() {
        let (names, links) = parse_sankey("sankey\nA,B,10\nB,C,4\nB,D,6\nE,D,5\n");
        let layout = layout_sankey(names.len(), &links, 600.0, 400.0, 10.0, 10.0, "justify");
        assert_eq!(layout.depth, vec![0, 1, 2, 2, 0]);
        // Justify: sources start on the left; sinks (C, D) go to the last column.
        let (_, _, h_b) = layout.nodes[1];
        let (_, _, h_d) = layout.nodes[3];
        assert!((h_d / h_b - 11.0 / 10.0).abs() < 0.01, "{h_d} {h_b}");
        // Nodes in one column do not overlap and stay inside the drawing.
        for (a, b) in [(2, 3), (0, 4)] {
            let (ya, ha) = (layout.nodes[a].1, layout.nodes[a].2);
            let (yb, hb) = (layout.nodes[b].1, layout.nodes[b].2);
            assert!(ya + ha <= yb + 0.1 || yb + hb <= ya + 0.1, "{a} {b}");
        }
        assert!(layout.nodes.iter().all(|&(_, y, h)| y >= -0.1 && y + h <= 400.1));
        // Outgoing link widths add up to the node's height.
        let out: f32 = layout.links.iter().zip(&links).filter(|(_, l)| l.source == 1).map(|(e, _)| e.0).sum();
        assert!((out - h_b).abs() < 0.1);
    }

    #[test]
    fn sankey_svg_values_colours_and_config() {
        let svg = svg_of("sankey\nElectricity grid,Industry,342.165\nElectricity grid,Losses,56.691\n");
        assert_eq!(svg.matches(r#"data-role="sankey-node""#).count(), 3);
        assert_eq!(svg.matches(r#"data-role="sankey-link""#).count(), 2);
        assert!(svg.contains(">Electricity grid 398.86<"), "{svg}");
        assert!(svg.contains("linearGradient"), "gradient links by default");
        let src = "---\nconfig:\n  sankey:\n    showValues: false\n    linkColor: source\n    labelStyle: outlined\n    width: 800\n    nodeColors:\n      Industry: \"#e15759\"\n---\nsankey\nElectricity grid,Industry,342.165\n";
        let svg = svg_of(src);
        assert!(svg.contains(">Industry<") && !svg.contains("342"), "{svg}");
        assert!(svg.contains(r##"fill="#e15759" data-role="sankey-node""##), "{svg}");
        assert!(!svg.contains("linearGradient") && svg.contains(r##"stroke="#4e79a7" stroke-opacity"##));
        assert!(svg.contains(r#"paint-order="stroke""#));
        assert_eq!(view_box(&svg).0, 816.0);
    }

    #[test]
    fn sankey_labels_of_small_nodes_keep_apart() {
        assert_eq!(spread_labels(&[20.0, 21.0, 22.0], 13.0, 10.0, 400.0), vec![20.0, 33.0, 46.0]);
        // Past the bottom, the stack moves up.
        assert_eq!(spread_labels(&[395.0, 398.0], 13.0, 10.0, 400.0), vec![387.0, 400.0]);
        let mut src = String::from("sankey\nSource,Big,500\n");
        for k in 0..6 {
            src.push_str(&format!("Source,Tiny{k},1\n"));
        }
        let svg = svg_of(&src);
        let mut ys: Vec<f32> = (0..6).map(|k| text_y(&svg, &format!("Tiny{k} 1"))).collect();
        ys.sort_by(f32::total_cmp);
        assert!(ys.windows(2).all(|w| w[1] - w[0] >= 12.9), "{ys:?}");
        assert!(svg.contains(r#"data-role="label-leader""#), "{svg}");
        let (_, h) = view_box(&svg);
        assert!(ys.iter().all(|&y| y <= h), "{ys:?}");
    }
}
