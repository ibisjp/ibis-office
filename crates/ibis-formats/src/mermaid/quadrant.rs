// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `quadrantChart`.

use super::*;

pub(super) struct QuadrantPoint {
    pub(super) label: String,
    pub(super) x: f32, // 0.0..=1.0
    pub(super) y: f32, // 0.0..=1.0
    /// `:::name` of a `classDef`.
    pub(super) class: Option<String>,
    /// Styles written after the point (`radius: 12, color: #ff3300`).
    pub(super) style: QuadrantPointStyle,
}

/// `radius`, `color`, `stroke-color` and `stroke-width` of a point or a `classDef`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct QuadrantPointStyle {
    pub(super) radius: Option<f32>,
    pub(super) color: Option<String>,
    pub(super) stroke_color: Option<String>,
    pub(super) stroke_width: Option<f32>,
}

impl QuadrantPointStyle {
    /// `self` with the gaps filled from `base`.
    fn over(&self, base: &QuadrantPointStyle) -> QuadrantPointStyle {
        QuadrantPointStyle {
            radius: self.radius.or(base.radius),
            color: self.color.clone().or_else(|| base.color.clone()),
            stroke_color: self.stroke_color.clone().or_else(|| base.stroke_color.clone()),
            stroke_width: self.stroke_width.or(base.stroke_width),
        }
    }
}

/// `radius: 12, color: #ff3300, stroke-color: #10f0f0, stroke-width: 5px` (spaces around `:` allowed).
pub(super) fn parse_quadrant_point_style(text: &str) -> QuadrantPointStyle {
    let mut style = QuadrantPointStyle::default();
    for part in text.split(',') {
        let Some((key, value)) = part.split_once(':') else { continue };
        let value = value.trim();
        match key.trim() {
            "radius" => style.radius = px_number(value).map(|v| (v as f32).clamp(1.0, 60.0)),
            "color" => style.color = css_color(value),
            "stroke-color" => style.stroke_color = css_color(value),
            "stroke-width" => style.stroke_width = px_number(value).map(|v| (v as f32).min(20.0)),
            _ => {}
        }
    }
    style
}

/// `Name:::class: [x, y] styles` as (name, class, x, y, styles).
pub(super) fn parse_quadrant_point(line: &str) -> Option<QuadrantPoint> {
    let open = line.find('[')?;
    let close = open + line[open..].find(']')?;
    let head = line[..open].trim_end().strip_suffix(':')?;
    let (label, class) = match head.split_once(":::") {
        Some((label, class)) => (label, Some(class.trim().to_string()).filter(|c| !c.is_empty())),
        None => (head, None),
    };
    let mut coords = line[open + 1..close].split(',').map(|v| v.trim().parse::<f32>());
    let (Some(Ok(x)), Some(Ok(y))) = (coords.next(), coords.next()) else { return None };
    Some(QuadrantPoint {
        label: unquote(label.trim()),
        x: x.clamp(0.0, 1.0),
        y: y.clamp(0.0, 1.0),
        class,
        style: parse_quadrant_point_style(&line[close + 1..]),
    })
}

pub(super) fn render_quadrant_chart_svg(source: &str, config: &MermaidConfig) -> Option<(Vec<u8>, u32, u32)> {
    let mut title: Option<String> = None;
    let mut x_left = String::from("Low");
    let mut x_right = String::from("High");
    let mut y_bottom = String::from("Low");
    let mut y_top = String::from("High");
    let mut q_labels = [
        String::from("Q1"),
        String::from("Q2"),
        String::from("Q3"),
        String::from("Q4"),
    ];
    let mut points: Vec<QuadrantPoint> = Vec::new();
    let mut classes: HashMap<String, QuadrantPointStyle> = HashMap::new();
    let mut anything = false;

    for raw in source.lines().skip(1) {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ") {
            title = Some(unquote(rest.trim()));
        } else if let Some(rest) = l.strip_prefix("x-axis ") {
            if let Some((a, b)) = rest.split_once("-->") {
                x_left = unquote(a.trim());
                x_right = unquote(b.trim());
            } else {
                x_left = unquote(rest.trim());
            }
        } else if let Some(rest) = l.strip_prefix("y-axis ") {
            if let Some((a, b)) = rest.split_once("-->") {
                y_bottom = unquote(a.trim());
                y_top = unquote(b.trim());
            } else {
                y_bottom = unquote(rest.trim());
            }
        } else if let Some((n, rest)) = l
            .strip_prefix("quadrant-")
            .and_then(|r| r.split_once(' '))
            .and_then(|(n, rest)| n.parse::<usize>().ok().filter(|n| (1..=4).contains(n)).map(|n| (n, rest)))
        {
            q_labels[n - 1] = unquote(rest.trim());
        } else if let Some(rest) = l.strip_prefix("classDef ") {
            let rest = rest.trim();
            let (name, styles) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            classes.insert(name.to_string(), parse_quadrant_point_style(styles));
            continue;
        } else if let Some(point) = parse_quadrant_point(l) {
            points.push(point);
        } else {
            continue;
        }
        anything = true;
    }

    // An empty chart (axes and quadrants only) is still drawn, like Mermaid.
    if !anything {
        return None;
    }

    // `quadrantChart.chartWidth` / `chartHeight` size the plot; `pointRadius` the default point.
    let dim = |key: &str| {
        config.option(key).and_then(|v| px_number(v)).map(|v| v as f32).filter(|v| (120.0..=2000.0).contains(v))
    };
    let plot_w = dim("quadrantChart.chartWidth").map_or(340.0, |w| w - 92.0).max(120.0);
    let plot_h = dim("quadrantChart.chartHeight").map_or(340.0, |h| h - 100.0).max(120.0);
    let default_radius = config
        .option("quadrantChart.pointRadius")
        .and_then(|v| px_number(v))
        .map_or(5.5, |v| (v as f32).clamp(1.0, 60.0));
    let var = |key: &str, fallback: &str| config.vars.get(key).and_then(|v| css_color(v)).unwrap_or_else(|| fallback.to_string());

    let pad_l = 56.0f32;
    let pad_r = 36.0f32;
    let pad_t = if title.is_some() { 52.0f32 } else { 28.0f32 };
    let pad_b = 46.0f32;
    let total_w = (pad_l + plot_w + pad_r).ceil() as u32;
    let total_h = (pad_t + plot_h + pad_b).ceil() as u32;
    let (half_w, half_h) = (plot_w / 2.0, plot_h / 2.0);

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(
        r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##
    ));
    if let Some(t) = &title {
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="26" text-anchor="middle" font-size="15" font-weight="bold" fill="{}">{}</text>"##,
            total_w as f32 / 2.0,
            var("quadrantTitleFill", "#0f172a"),
            escape_xml(t)
        ));
    }

    // 4 quadrants: Q2 (top-left), Q1 (top-right), Q3 (bottom-left), Q4 (bottom-right)
    let quads = [
        (pad_l + half_w, pad_t, "#eff6ff"),
        (pad_l, pad_t, "#f0fdf4"),
        (pad_l, pad_t + half_h, "#fefce8"),
        (pad_l + half_w, pad_t + half_h, "#fdf2f8"),
    ];
    let border = var("quadrantInternalBorderStrokeFill", "#cbd5e1");
    for (i, (qx, qy, bg)) in quads.into_iter().enumerate() {
        let n = i + 1;
        let bg = var(&format!("quadrant{n}Fill"), bg);
        let ink = var(&format!("quadrant{n}TextFill"), "#64748b");
        svg.push_str(&format!(
            r##"<rect x="{qx:.1}" y="{qy:.1}" width="{half_w:.1}" height="{half_h:.1}" fill="{bg}" stroke="{border}" stroke-width="1"/>"##
        ));
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" font-weight="bold" fill="{ink}" data-quadrant="{n}">{}</text>"##,
            qx + half_w / 2.0,
            qy + 18.0,
            escape_xml(&q_labels[i])
        ));
    }

    // Axis labels
    let x_ink = var("quadrantXAxisTextFill", "#334155");
    let y_ink = var("quadrantYAxisTextFill", "#334155");
    svg.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="{x_ink}">{}</text>"##,
        pad_l + half_w * 0.5,
        pad_t + plot_h + 22.0,
        escape_xml(&x_left)
    ));
    svg.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="{x_ink}">{}</text>"##,
        pad_l + half_w * 1.5,
        pad_t + plot_h + 22.0,
        escape_xml(&x_right)
    ));
    // The y-axis labels run up the left side, one per half, like Mermaid's.
    for (label, cy) in [(&y_bottom, pad_t + half_h * 1.5), (&y_top, pad_t + half_h * 0.5)] {
        let x = pad_l - 12.0;
        svg.push_str(&format!(
            r##"<text x="{x:.1}" y="{cy:.1}" text-anchor="middle" font-size="11" fill="{y_ink}" transform="rotate(-90 {x:.1} {cy:.1})">{}</text>"##,
            escape_xml(label)
        ));
    }

    let point_fill = var("quadrantPointFill", "#2563eb");
    let point_ink = var("quadrantPointTextFill", "#0f172a");
    for pt in &points {
        let class = pt.class.as_ref().and_then(|c| classes.get(c)).cloned().unwrap_or_default();
        let style = pt.style.over(&class);
        let r = style.radius.unwrap_or(default_radius);
        let fill = style.color.unwrap_or_else(|| point_fill.clone());
        let (stroke, stroke_w) = match (style.stroke_color, style.stroke_width) {
            (None, None) => ("#ffffff".to_string(), 1.5),
            (color, width) => (color.unwrap_or_else(|| fill.clone()), width.unwrap_or(1.5)),
        };
        let px = pad_l + pt.x * plot_w;
        let py = pad_t + (1.0 - pt.y) * plot_h;
        svg.push_str(&format!(
            r##"<circle cx="{px:.1}" cy="{py:.1}" r="{r:.1}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_w:.1}" data-role="quadrant-point"/>"##
        ));
        svg.push_str(&format!(
            r##"<text x="{px:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="{point_ink}">{}</text>"##,
            py - r - 3.5,
            escape_xml(&pt.label)
        ));
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn quadrant_point_styles_and_class_defs() {
        let src = "quadrantChart
  A: [0.9, 0.0] radius: 12
  B:::c1: [0.8, 0.1] color: #ff3300, radius: 10
  C: [0.7, 0.2] radius: 25, color: #00ff33, stroke-color: #10f0f0
  D:::c2: [0.5, 0.4]
  E:::c2: [0.4, 0.5] color: #0000ff
  classDef c1 color: #109060
  classDef c2 color: #908342, radius : 10, stroke-color: #310085, stroke-width: 10px
";
        let svg = svg_of(src);
        let circles = tag_with(&svg, "<circle");
        assert_eq!(circles.len(), 5, "{svg}");
        assert!(svg.contains(r#"data-role="quadrant-point""#), "{svg}");
        assert!(circles[0].contains(r#"r="12.0""#));
        // The point's own colour beats its class's.
        assert!(circles[1].contains(r##"fill="#ff3300""##) && circles[1].contains(r#"r="10.0""#));
        assert!(circles[2].contains(r##"stroke="#10f0f0""##) && circles[2].contains(r#"r="25.0""#));
        assert!(circles[3].contains(r##"fill="#908342""##) && circles[3].contains(r#"stroke-width="10.0""#), "{}", circles[3]);
        assert!(circles[4].contains(r##"fill="#0000ff""##) && circles[4].contains(r##"stroke="#310085""##));
        assert!(svg.contains(">B<") && svg.contains(">D<"), "the class name is not part of the label");

        let p = parse_quadrant_point("\"Name: x\":::k: [0.1, 0.2] stroke-width: 5px ,color: #ff33f0").expect("point");
        assert_eq!((p.label.as_str(), p.class.as_deref()), ("Name: x", Some("k")));
        assert_eq!(p.style.stroke_width, Some(5.0));
        assert_eq!(p.style.color.as_deref(), Some("#ff33f0"));
    }

    #[test]
    fn quadrant_config_size_and_theme_variables() {
        let src = "---
config:
  quadrantChart:
    chartWidth: 400
    chartHeight: 300
  themeVariables:
    quadrant1TextFill: \"ff0000\"
    quadrant2Fill: \"#123456\"
---
quadrantChart
  x-axis Urgent --> Not Urgent
  y-axis Not Important --> \"Important ❤\"
  quadrant-1 Plan
  quadrant-2 Do
";
        let svg = svg_of(src);
        // No points: the empty chart is still drawn.
        assert_eq!(view_box(&svg), (400.0, 274.0), "{svg}");
        assert!(svg.contains(r##"fill="#ff0000" data-quadrant="1">Plan<"##), "{svg}");
        assert!(svg.contains(r##"fill="#123456""##));
        assert!(svg.contains(">Important ❤<"), "quoted axis labels lose their quotes");
        assert!(!svg.contains("<circle"));
    }
}
