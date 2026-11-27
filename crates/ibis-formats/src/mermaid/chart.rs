// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `pie` / `xychart-beta`: native chart specs and their SVG fallback.

use super::*;

/// `pie [showData] [title Title words]` on the first line, as mermaid.js accepts.
pub(super) fn parse_pie_header(line: &str) -> (bool, Option<String>) {
    let rest = line.trim();
    let rest = rest
        .strip_prefix("pie-beta")
        .or_else(|| rest.strip_prefix("pie"))
        .unwrap_or(rest)
        .trim();
    let mut show_data = false;
    let mut title = None;
    let mut parts = rest.split_whitespace().peekable();
    while let Some(tok) = parts.next() {
        if tok == "showData" {
            show_data = true;
            continue;
        }
        if tok == "title" {
            let t = unquote(parts.collect::<Vec<_>>().join(" ").trim());
            if !t.is_empty() {
                title = Some(t);
            }
            break;
        }
    }
    (show_data, title)
}

pub(super) fn parse_pie_chart(source: &str, config: &MermaidConfig) -> Option<MermaidChartSpec> {
    let mut show_data = false;
    let mut title = None;
    if let Some(first) = source.lines().next() {
        let (sd, t) = parse_pie_header(first);
        show_data = sd;
        title = t;
    }
    let mut categories = Vec::new();
    let mut values = Vec::new();

    for line in source.lines().skip(1) {
        let l = line.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ") {
            let t = unquote(rest.trim());
            if !t.is_empty() {
                title = Some(t);
            }
            continue;
        }
        if let Some((label_part, val_part)) = l.split_once(':') {
            let label = unquote(label_part.trim());
            if let Ok(v) = val_part.trim().parse::<f64>()
                && !label.is_empty()
            {
                categories.push(label);
                values.push(Some(Number(v)));
            }
        }
    }

    if categories.is_empty() {
        return None;
    }
    let style = pie_style(config, categories.len());
    // `pie.donutHole` turns the pie into a doughnut (native `c:doughnutChart`).
    let kind = if style.donut_hole.is_some() { NewChartKind::Doughnut } else { NewChartKind::Pie };
    Some(MermaidChartSpec {
        horizontal: false,
        kind,
        title,
        categories,
        series: vec![("Series 1".to_string(), values)],
        series_kinds: vec![kind],
        show_data,
        x_axis_title: None,
        x_range: None,
        y_axis_title: None,
        y_range: None,
        point_labels: Vec::new(),
        style,
    })
}

/// `config.pie.*` and the `pie1`..`pie12` / `pieOuterStrokeWidth` theme variables.
pub(super) fn pie_style(config: &MermaidConfig, slices: usize) -> MermaidChartStyle {
    let number = |key: &str| config.option(key).and_then(|v| unquote(v).trim().parse::<f64>().ok()).filter(|v| v.is_finite());
    let custom: Vec<Option<String>> =
        (1..=slices.min(12)).map(|i| config.vars.get(&format!("pie{i}")).and_then(|v| hex_color(v))).collect();
    let palette = if custom.iter().any(Option::is_some) {
        custom
            .into_iter()
            .enumerate()
            .map(|(i, c)| c.unwrap_or_else(|| CHART_PALETTE[i % CHART_PALETTE.len()].to_string()))
            .collect()
    } else {
        Vec::new()
    };
    MermaidChartStyle {
        palette,
        text_position: number("pie.textPosition").map(|v| v.clamp(0.0, 1.0)),
        donut_hole: number("pie.donutHole").map(|v| v.clamp(0.0, 0.9)).filter(|v| *v > 0.0),
        pie_legend: config.option("pie.legendPosition").and_then(|value| {
            match unquote(value).as_str() {
                "top" => Some(PieLegendPosition::Side(Side::Top)),
                "bottom" => Some(PieLegendPosition::Side(Side::Bottom)),
                "left" => Some(PieLegendPosition::Side(Side::Left)),
                "right" => Some(PieLegendPosition::Side(Side::Right)),
                "center" => Some(PieLegendPosition::Center),
                _ => None,
            }
        }),
        highlight: config
            .option("pie.highlightSlice")
            .map(unquote)
            .filter(|label| !label.is_empty() && label != "hover"),
        outer_stroke_width: config.vars.get("pieOuterStrokeWidth").and_then(|v| px_number(v)),
        ..MermaidChartStyle::default()
    }
}

/// `config.xyChart.*` and `themeVariables.xyChart.*`.
pub(super) fn xy_style(config: &MermaidConfig) -> MermaidChartStyle {
    let size = |key: &str| {
        config
            .option(key)
            .and_then(|v| unquote(v).trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && (100.0..=4000.0).contains(v))
            .map(|v| v.round() as u32)
    };
    let palette = config
        .vars
        .get("xyChart.plotColorPalette")
        .map(|list| list.split(',').filter_map(hex_color).collect())
        .unwrap_or_default();
    let size_px = match (size("xyChart.width"), size("xyChart.height")) {
        (None, None) => None,
        (w, h) => Some((w.unwrap_or(XY_WIDTH), h.unwrap_or(XY_HEIGHT))),
    };
    MermaidChartStyle {
        palette,
        data_labels: config.flag("xyChart.showDataLabel", false),
        data_labels_outside: config.flag("xyChart.showDataLabelOutsideBar", false),
        size_px,
        title_color: config.vars.get("xyChart.titleColor").and_then(|v| hex_color(v)),
        ..MermaidChartStyle::default()
    }
}

/// One `bar` / `line` value: `12`, or `540 "PaLM"` with a point label (Mermaid 11.16).
pub(super) fn parse_xy_value(token: &str) -> (Option<Number>, Option<String>) {
    let token = token.trim();
    let (number, label) = match token.find(|c: char| c.is_whitespace() || c == '"') {
        Some(at) => (&token[..at], Some(unquote(token[at..].trim()))),
        None => (token, None),
    };
    (number.parse::<f64>().ok().map(Number), label.filter(|l| !l.is_empty()))
}

pub(super) fn parse_xy_chart(source: &str, config: &MermaidConfig) -> Option<MermaidChartSpec> {
    let header = source.lines().next()?;
    let orientation = header
        .split("%%")
        .next()?
        .split_whitespace()
        .nth(1)
        .or_else(|| config.option("xyChart.chartOrientation"));
    let horizontal = orientation == Some("horizontal");
    let bar_kind = if horizontal { NewChartKind::ClusteredBar } else { NewChartKind::ClusteredColumn };
    let mut title = None;
    let mut categories = Vec::new();
    let mut series = Vec::new();
    let mut series_kinds = Vec::new();
    let mut point_labels: Vec<Vec<Option<String>>> = Vec::new();
    let mut has_bar = false;
    let mut bar_idx = 1usize;
    let mut line_idx = 1usize;
    let mut x_axis_title = None;
    let mut x_range = None;
    let mut y_axis_title = None;
    let mut y_range = None;

    for line in source.lines().skip(1) {
        let l = line.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ") {
            let t = unquote(rest.trim());
            if !t.is_empty() {
                title = Some(t);
            }
        } else if let Some(rest) = l.strip_prefix("x-axis ") {
            if let Some(list) = extract_bracket_list(rest) {
                categories = list.into_iter().map(|s| unquote(&s)).collect();
                let title = unquote(rest[..rest.find('[').unwrap_or(0)].trim());
                x_axis_title = (!title.is_empty()).then_some(title);
                x_range = None;
            } else {
                let (title, range) = parse_xy_axis(rest);
                x_axis_title = title;
                if range.is_some() {
                    categories.clear();
                    x_range = range;
                }
            }
        } else if let Some(rest) = l.strip_prefix("y-axis ") {
            (y_axis_title, y_range) = parse_xy_axis(rest);
        } else if let Some((is_bar, rest)) =
            l.strip_prefix("bar ").map(|r| (true, r)).or_else(|| l.strip_prefix("line ").map(|r| (false, r)))
        {
            if let Some(list) = extract_bracket_list(rest) {
                let (vals, labels): (Vec<_>, Vec<_>) = list.iter().map(|s| parse_xy_value(s)).unzip();
                let name = if is_bar {
                    has_bar = true;
                    bar_idx += 1;
                    xy_series_name(rest, &format!("Bar {}", bar_idx - 1))
                } else {
                    line_idx += 1;
                    xy_series_name(rest, &format!("Line {}", line_idx - 1))
                };
                series.push((name, vals));
                series_kinds.push(if is_bar { bar_kind } else { NewChartKind::Line });
                point_labels.push(labels);
            }
        }
    }

    if series.is_empty() {
        return None;
    }
    if categories.is_empty() {
        let max_len = series.iter().map(|(_, v)| v.len()).max().unwrap_or(0);
        categories = (1..=max_len).map(|i| i.to_string()).collect();
    }
    if point_labels.iter().all(|labels| labels.iter().all(Option::is_none)) {
        point_labels.clear();
    }
    Some(MermaidChartSpec {
        kind: if has_bar { bar_kind } else { NewChartKind::Line },
        horizontal,
        title,
        categories,
        series,
        series_kinds,
        show_data: false,
        x_axis_title,
        x_range,
        y_axis_title,
        y_range,
        point_labels,
        style: xy_style(config),
    })
}

/// `"title" min --> max`, `title`, or `min --> max` of an `xychart` axis line.
pub(super) fn parse_xy_axis(rest: &str) -> (Option<String>, Option<(f64, f64)>) {
    let rest = rest.trim();
    let (head, range) = match rest.split_once("-->") {
        Some((left, right)) => {
            let left = left.trim();
            let (title_part, min_token) = match left.rfind(char::is_whitespace) {
                Some(at) => (&left[..at], left[at..].trim()),
                None => ("", left),
            };
            match (min_token.parse::<f64>(), right.trim().parse::<f64>()) {
                (Ok(min), Ok(max)) if min.is_finite() && max.is_finite() && max > min && (max - min).is_finite() => (title_part, Some((min, max))),
                _ => (rest, None),
            }
        }
        None => (rest, None),
    };
    let title = unquote(head.trim());
    ((!title.is_empty()).then_some(title), range)
}

pub(super) fn xy_series_name(rest: &str, fallback: &str) -> String {
    let Some(list_at) = rest.find('[') else {
        return fallback.to_string();
    };
    let name = unquote(rest[..list_at].trim());
    if name.is_empty() { fallback.to_string() } else { name }
}

pub(super) fn extract_bracket_list(s: &str) -> Option<Vec<String>> {
    let start = s.find('[')?;
    let end = s.rfind(']')?;
    if end <= start {
        return None;
    }
    Some(
        s[start + 1..end]
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect(),
    )
}


/// The colours the chart fallback draws with, before `config` overrides them.
pub(super) const CHART_PALETTE: &[&str] = &[
    "#3b82f6", "#10b981", "#f59e0b", "#ef4444", "#8b5cf6", "#ec4899", "#06b6d4", "#84cc16",
];
pub(super) const XY_WIDTH: u32 = 520;
pub(super) const XY_HEIGHT: u32 = 280;

/// A number shown on a chart: whole numbers without a fraction.
fn chart_number(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 { format!("{v:.0}") } else { format!("{v}") }
}

/// Evenly spaced "nice" ticks between `min` and `max`, aimed at about `target` values
/// (the same kind of 1-2-5 steps mermaid.js / d3 use on xyChart axes).
pub(super) fn nice_ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    let lo = min.min(max);
    let hi = min.max(max);
    let span = (hi - lo).max(f64::EPSILON);
    let target = target.max(2) as f64;
    let raw = span / target;
    let mag = 10f64.powf(raw.max(f64::EPSILON).log10().floor());
    let residual = raw / mag;
    let step = if residual <= 1.5 {
        mag
    } else if residual <= 3.0 {
        2.0 * mag
    } else if residual <= 7.0 {
        5.0 * mag
    } else {
        10.0 * mag
    };
    let mut t = (lo / step).ceil() * step;
    if ((lo / step) - (lo / step).round()).abs() < 1e-9 {
        t = lo;
    }
    let mut out = Vec::new();
    let end = hi + step * 1e-9;
    while t <= end && out.len() < 40 {
        let v = (t / step).round() * step;
        if v >= lo - step * 1e-9 && v <= hi + step * 1e-9 {
            out.push(v);
        }
        t += step;
    }
    if out.is_empty() {
        out.push(lo);
        if (hi - lo).abs() > f64::EPSILON {
            out.push(hi);
        }
    }
    out
}

pub(super) fn format_axis_tick(v: f64) -> String {
    if v.abs() >= 1.0 && (v - v.round()).abs() < 1e-8 {
        format!("{:.0}", v.round())
    } else {
        let s = format!("{v:.4}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn xy_series_is_named(name: &str) -> bool {
    !name.is_empty() && !name.starts_with("Bar ") && !name.starts_with("Line ") && name != "Series 1"
}

/// Render a parsed `MermaidChartSpec` (`pie` or `xychart-beta`) to a standalone SVG.
///
/// Used as a fallback when a native Office chart cannot be instantiated (e.g., clipboard fragment paste).
pub fn render_chart_spec_svg(spec: &MermaidChartSpec) -> Option<(Vec<u8>, u32, u32)> {
    if spec.series.is_empty() || spec.categories.is_empty() {
        return None;
    }
    let style = &spec.style;
    let color_of = |i: usize| -> String {
        if style.palette.is_empty() {
            CHART_PALETTE[i % CHART_PALETTE.len()].to_string()
        } else {
            style.palette[i % style.palette.len()].clone()
        }
    };
    let title_fill = style.title_color.clone().unwrap_or_else(|| "#0f172a".to_string());

    if matches!(spec.kind, NewChartKind::Pie | NewChartKind::Doughnut) {
        let vals: Vec<f64> = spec.series[0]
            .1
            .iter()
            .map(|v| v.map(|n| n.0.max(0.0)).unwrap_or(0.0))
            .collect();
        let total: f64 = vals.iter().sum();
        if total <= 0.0 {
            return None;
        }
        let legend_labels: Vec<String> = spec.categories.iter().enumerate().map(|(i, category)| {
            let val = vals.get(i).copied().unwrap_or(0.0);
            let shown = if spec.show_data { format!(" [{}]", chart_number(val)) } else { String::new() };
            format!("{category}{shown} ({:.1}%)", val / total * 100.0)
        }).collect();
        let legend_width = 18.0 + f64::from(legend_labels.iter()
            .map(|label| estimate_text_width(label, 11.5))
            .fold(0.0, f32::max));
        let legend_height = legend_labels.len() as f64 * 22.0;
        let title_width = spec.title.as_deref().map_or(0.0, |title| f64::from(estimate_text_width(title, 14.0)));
        let title_offset = if spec.title.is_some() { 12.0 } else { 0.0 };
        let plot_height = (legend_labels.len() as f64 * 24.0 + 80.0).max(250.0);
        let r = 86.0f64;
        let position = style.pie_legend.unwrap_or(PieLegendPosition::Side(Side::Right));
        let side_legend = matches!(position, PieLegendPosition::Side(Side::Left | Side::Right));
        let width = 460.0_f64.max(title_width + 32.0).max(legend_width + if side_legend { 264.0 } else { 32.0 });
        let mut cy = plot_height / 2.0 + title_offset;
        let mut height = plot_height;
        let cx = match position {
            PieLegendPosition::Side(Side::Left) => width - 130.0,
            PieLegendPosition::Side(Side::Right) => 130.0,
            _ => width / 2.0,
        };
        let (legend_x, legend_y) = match position {
            PieLegendPosition::Side(Side::Left) => (cx - 118.0 - legend_width, cy - legend_height / 2.0),
            PieLegendPosition::Side(Side::Right) => (cx + 118.0, cy - legend_height / 2.0),
            PieLegendPosition::Side(Side::Top) => {
                height += legend_height;
                cy += legend_height;
                ((width - legend_width) / 2.0, 20.0 + title_offset * 2.0)
            }
            PieLegendPosition::Side(Side::Bottom) => {
                height += legend_height;
                ((width - legend_width) / 2.0, plot_height)
            }
            PieLegendPosition::Center => ((width - legend_width) / 2.0, cy - legend_height / 2.0),
        };
        let (width_px, height_px) = (width.ceil() as u32, height.ceil() as u32);
        // `pie.donutHole`: the inner radius as a fraction of the outer one.
        let hole = style.donut_hole.unwrap_or(0.0) * r;
        // `pie.textPosition`: where the percentage sits, from the centre (0) to the rim (1).
        let text_at = style.text_position.unwrap_or(0.75) * r;

        let mut svg = String::new();
        svg.push_str(&format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width_px}" height="{height_px}" viewBox="0 0 {width_px} {height_px}" font-family="sans-serif">"##
        ));
        svg.push_str(&format!(
            r##"<rect width="{width_px}" height="{height_px}" fill="#ffffff"/>"##
        ));
        if let Some(t) = &spec.title {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="26" text-anchor="middle" font-size="14" font-weight="bold" fill="{title_fill}">{}</text>"##,
                width_px as f32 / 2.0,
                escape_xml(t)
            ));
        }

        let mut angle = -std::f64::consts::FRAC_PI_2;
        let mut labels = String::new();
        for (i, &val) in vals.iter().enumerate() {
            let frac = val / total;
            if frac <= 0.0 {
                continue;
            }
            let color = color_of(i);
            let next_angle = angle + frac * 2.0 * std::f64::consts::PI;
            let mid = (angle + next_angle) / 2.0;
            // `pie.highlightSlice`: that slice is pulled out of the pie and outlined.
            let highlighted = style.highlight.as_deref() == Some(spec.categories[i].as_str());
            let (ox, oy) = if highlighted { (8.0 * mid.cos(), 8.0 * mid.sin()) } else { (0.0, 0.0) };
            let (scx, scy) = (cx + ox, cy + oy);
            let mark = if highlighted { r##" data-highlight="1" stroke-width="3""## } else { r##" stroke-width="1.5""## };
            if frac >= 0.9999 {
                if hole > 0.0 {
                    svg.push_str(&format!(
                        r##"<path d="M {:.1} {scy:.1} A {r:.1} {r:.1} 0 1 1 {:.1} {scy:.1} A {r:.1} {r:.1} 0 1 1 {:.1} {scy:.1} Z M {:.1} {scy:.1} A {hole:.1} {hole:.1} 0 1 0 {:.1} {scy:.1} A {hole:.1} {hole:.1} 0 1 0 {:.1} {scy:.1} Z" fill="{color}" fill-rule="evenodd" stroke="#ffffff"{mark} data-role="slice"/>"##,
                        scx - r, scx + r, scx - r, scx - hole, scx + hole, scx - hole
                    ));
                } else {
                    svg.push_str(&format!(
                        r##"<circle cx="{scx:.1}" cy="{scy:.1}" r="{r:.1}" fill="{color}" stroke="#ffffff"{mark} data-role="slice"/>"##
                    ));
                }
            } else {
                let x1 = scx + r * angle.cos();
                let y1 = scy + r * angle.sin();
                let x2 = scx + r * next_angle.cos();
                let y2 = scy + r * next_angle.sin();
                let large_arc = if frac > 0.5 { 1 } else { 0 };
                let d = if hole > 0.0 {
                    let ix1 = scx + hole * next_angle.cos();
                    let iy1 = scy + hole * next_angle.sin();
                    let ix2 = scx + hole * angle.cos();
                    let iy2 = scy + hole * angle.sin();
                    format!(
                        "M {x1:.1} {y1:.1} A {r:.1} {r:.1} 0 {large_arc} 1 {x2:.1} {y2:.1} L {ix1:.1} {iy1:.1} A {hole:.1} {hole:.1} 0 {large_arc} 0 {ix2:.1} {iy2:.1} Z"
                    )
                } else {
                    format!("M {scx:.1} {scy:.1} L {x1:.1} {y1:.1} A {r:.1} {r:.1} 0 {large_arc} 1 {x2:.1} {y2:.1} Z")
                };
                svg.push_str(&format!(r##"<path d="{d}" fill="{color}" stroke="#ffffff"{mark} data-role="slice"/>"##));
            }
            // The slice's share, like Mermaid's label on each slice.
            let lr = if frac >= 0.9999 { if hole > 0.0 { (hole + r) / 2.0 } else { 0.0 } } else { text_at.max(hole) };
            let (lx, ly) = (scx + lr * mid.cos(), scy + lr * mid.sin());
            // A narrow slice may put white text across the white separator.
            // A dark outline keeps the percentage legible on both backgrounds.
            let outline = if frac < 0.05 {
                r##" stroke="#1e293b" stroke-width="2" stroke-linejoin="round" paint-order="stroke""##
            } else {
                ""
            };
            labels.push_str(&format!(
                r##"<text x="{lx:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#ffffff"{outline} data-role="slice-label">{:.0}%</text>"##,
                ly + 4.0,
                frac * 100.0
            ));
            angle = next_angle;
        }
        svg.push_str(&labels);
        // `themeVariables.pieOuterStrokeWidth`: a rim around the whole pie.
        if let Some(w) = style.outer_stroke_width.filter(|w| *w > 0.0) {
            svg.push_str(&format!(
                r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{:.1}" fill="none" stroke="#000000" stroke-width="{w:.1}" data-role="pie-rim"/>"##,
                r + w / 2.0
            ));
        }

        if position == PieLegendPosition::Center {
            svg.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{legend_height:.1}" rx="4" fill="#ffffff" fill-opacity="0.88"/>"##,
                legend_x - 6.0, legend_y - 4.0, legend_width + 12.0
            ));
        }
        for (i, label) in legend_labels.iter().enumerate() {
            let color = color_of(i);
            let ly = legend_y + i as f64 * 22.0;
            svg.push_str(&format!(
                r##"<rect x="{legend_x:.1}" y="{ly:.1}" width="12" height="12" rx="2" fill="{color}" data-role="legend-key"/>"##
            ));
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" font-size="11.5" fill="#1e293b">{}</text>"##,
                legend_x + 18.0, ly + 10.5, escape_xml(label)
            ));
        }

        svg.push_str("</svg>");
        return Some((svg.into_bytes(), width_px, height_px));
    }

    // XY chart (bar / line)
    let mut min_val = 0.0f64;
    let mut max_val = 1.0f64;
    for (_, vals) in &spec.series {
        for v in vals.iter().flatten() {
            min_val = min_val.min(v.0);
            max_val = max_val.max(v.0);
        }
    }
    // An explicit `y-axis` range takes precedence over the automatic scale.
    let (y_lo, y_hi) = spec.y_range.unwrap_or((if spec.horizontal || spec.x_range.is_some() { min_val } else { 0.0 }, max_val));
    let y_span = (y_hi - y_lo).max(f64::EPSILON);
    let y_frac = |v: f64| ((v - y_lo) / y_span).clamp(0.0, 1.0) as f32;
    let ticks = nice_ticks(y_lo, y_hi, 10);
    let tick_labels: Vec<String> = ticks.iter().copied().map(format_axis_tick).collect();
    let tick_w = tick_labels
        .iter()
        .map(|l| estimate_text_width(l, 10.0))
        .fold(0.0f32, f32::max);
    let has_legend = spec.series.iter().any(|(n, _)| xy_series_is_named(n));
    let legend_h: f32 = if has_legend { 22.0 } else { 0.0 };
    let x_ticks: Vec<_> = spec.x_range.map_or_else(Vec::new, |(min, max)| {
        let span = max - min;
        let ticks = if span < 1e-3 { vec![min, max] } else { nice_ticks(min, max, 10) };
        ticks.into_iter().map(|tick| {
            let label = if span < 1e-3 || tick.abs() >= 1e9 {
                format!("{tick:e}")
            } else {
                format_axis_tick(tick)
            };
            (tick, label)
        }).collect()
    });
    let horizontal = spec.horizontal;
    let (left_title, bottom_title) = if horizontal {
        (&spec.x_axis_title, &spec.y_axis_title)
    } else {
        (&spec.y_axis_title, &spec.x_axis_title)
    };
    let left_label_width = if horizontal {
        let labels = spec.categories.iter().filter(|_| spec.x_range.is_none())
            .chain(x_ticks.iter().map(|(_, label)| label));
        labels.map(|label| estimate_text_width(label, 11.0)).fold(0.0, f32::max)
    } else {
        tick_w
    };
    let (width_px, height_px) = style.size_px.unwrap_or((
        XY_WIDTH,
        if bottom_title.is_some() { XY_HEIGHT + 20 } else { XY_HEIGHT },
    ));
    let height_px = height_px + legend_h.ceil() as u32;
    let mut pad_l = ((if left_title.is_some() { 36.0 } else { 18.0 }) + left_label_width)
        .max(if left_title.is_some() { 70.0 } else { 52.0 })
        .min(width_px as f32 * 0.5);
    let bottom_tick_w = if horizontal {
        tick_w
    } else {
        x_ticks.iter().map(|(_, label)| estimate_text_width(label, 10.0)).fold(0.0, f32::max)
    };
    if spec.x_range.is_some() && !horizontal {
        pad_l = pad_l.max(bottom_tick_w / 2.0 + 6.0);
    }
    let pad_r = (bottom_tick_w / 2.0 + 6.0).max(24.0);
    let pad_t = if spec.title.is_some() { 44.0f32 } else { 24.0f32 };
    let pad_b = if bottom_title.is_some() { 62.0f32 } else { 42.0f32 } + legend_h;
    let plot_w = width_px as f32 - pad_l - pad_r;
    let plot_h = height_px as f32 - pad_t - pad_b;
    let plot_bottom = pad_t + plot_h;
    let category_extent = if horizontal { plot_h } else { plot_w };
    let slot = category_extent / spec.categories.len() as f32;
    let is_line_at = |si: usize, sname: &str| {
        spec.series_kinds.get(si) == Some(&NewChartKind::Line)
            || (spec.series_kinds.is_empty() && (spec.kind == NewChartKind::Line || sname.starts_with("Line")))
    };
    let bar_series: Vec<usize> =
        (0..spec.series.len()).filter(|&si| !is_line_at(si, &spec.series[si].0)).collect();
    let lanes = bar_series.len().max(1) as f32;
    let thickness = (slot * 0.54 / lanes).min(40.0);
    // Leave room for the bars centered on the endpoints of the numeric scale.
    let inset = if spec.x_range.is_some() && !bar_series.is_empty() { thickness * lanes / 2.0 } else { 0.0 };
    let x_at = |fraction: f32| inset + fraction * (category_extent - 2.0 * inset);
    let category_at = |index: usize| (index as f32 + 0.5) * slot;
    let sample_at = |index: usize, len: usize| {
        if spec.x_range.is_some() {
            x_at(index as f32 / len.saturating_sub(1).max(1) as f32)
        } else {
            category_at(index)
        }
    };
    let plot_at = |category: f32, value: f32| {
        if horizontal {
            (pad_l + plot_w * value, pad_t + category)
        } else {
            (pad_l + category, plot_bottom - plot_h * value)
        }
    };

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width_px}" height="{height_px}" viewBox="0 0 {width_px} {height_px}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(
        r##"<rect width="{width_px}" height="{height_px}" fill="#ffffff"/>"##
    ));
    if let Some(t) = &spec.title {
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="24" text-anchor="middle" font-size="14" font-weight="bold" fill="{title_fill}">{}</text>"##,
            width_px as f32 / 2.0,
            escape_xml(t)
        ));
    }
    // Grid & axes: mermaid.js-style nice ticks, not a fixed 5-step split.
    for (tick_val, tick) in ticks.iter().copied().zip(&tick_labels) {
        let fraction = y_frac(tick_val);
        let (x1, y1) = plot_at(0.0, fraction);
        let (x2, y2) = plot_at(if horizontal { plot_h } else { plot_w }, fraction);
        let (tx, ty, anchor) = if horizontal {
            (x2, y2 + 18.0, "middle")
        } else {
            (x1 - 6.0, y1 + 3.5, "end")
        };
        svg.push_str(&format!(
            r##"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="#e2e8f0" stroke-width="1"/><text x="{tx:.1}" y="{ty:.1}" text-anchor="{anchor}" font-size="10" fill="#64748b" data-role="tick">{}</text>"##,
            escape_xml(tick)
        ));
    }
    if let Some(t) = left_title {
        let ty = pad_t + plot_h / 2.0;
        let axis = if horizontal { "x" } else { "y" };
        svg.push_str(&format!(
            r##"<text x="16" y="{ty:.1}" text-anchor="middle" font-size="11" fill="#334155" transform="rotate(-90 16 {ty:.1})" data-role="{axis}-axis-title">{}</text>"##,
            escape_xml(t)
        ));
    }
    if let Some(t) = bottom_title {
        let axis = if horizontal { "y" } else { "x" };
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#334155" data-role="{axis}-axis-title">{}</text>"##,
            pad_l + plot_w / 2.0,
            height_px as f32 - legend_h - 12.0,
            escape_xml(t)
        ));
    }
    svg.push_str(&format!(
        r##"<line x1="{pad_l:.1}" y1="{pad_t:.1}" x2="{pad_l:.1}" y2="{plot_bottom:.1}" stroke="#64748b" stroke-width="1.2"/><line x1="{pad_l:.1}" y1="{plot_bottom:.1}" x2="{:.1}" y2="{plot_bottom:.1}" stroke="#64748b" stroke-width="1.2"/>"##,
        pad_l + plot_w
    ));
    for (ci, cat) in spec.categories.iter().enumerate().filter(|_| spec.x_range.is_none()) {
        let (x, y, anchor) = if horizontal {
            (pad_l - 6.0, pad_t + category_at(ci) + 3.5, "end")
        } else {
            (pad_l + category_at(ci), plot_bottom + 18.0, "middle")
        };
        svg.push_str(&format!(
            r##"<text x="{x:.1}" y="{y:.1}" text-anchor="{anchor}" font-size="11" fill="#334155">{}</text>"##,
            escape_xml(cat)
        ));
    }

    if let Some((min, max)) = spec.x_range {
        for (tick, label) in &x_ticks {
            let (px, py) = plot_at(x_at(((tick - min) / (max - min)).clamp(0.0, 1.0) as f32), 0.0);
            let (x, y, anchor) = if horizontal { (px - 6.0, py + 3.5, "end") } else { (px, py + 18.0, "middle") };
            svg.push_str(&format!(
                r##"<text x="{x:.1}" y="{y:.1}" text-anchor="{anchor}" font-size="10" fill="#64748b" data-role="x-tick">{label}</text>"##
            ));
        }
    }

    let mut labels = String::new();
    for (si, (sname, vals)) in spec.series.iter().enumerate() {
        let color = color_of(si);
        let point_label = |ci: usize| spec.point_labels.get(si).and_then(|l| l.get(ci)).and_then(|l| l.as_deref());
        svg.push_str(r##"<g data-role="series">"##);
        if is_line_at(si, sname) {
            let mut pts = Vec::new();
            for (ci, v_opt) in vals.iter().enumerate() {
                if let Some(v) = v_opt {
                    let (px, py) = plot_at(sample_at(ci, vals.len()), y_frac(v.0));
                    pts.push((ci, v.0, px, py));
                }
            }
            if pts.len() >= 2 {
                let poly: String = pts
                    .iter()
                    .map(|(_, _, x, y)| format!("{x:.1},{y:.1}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                svg.push_str(&format!(
                    r##"<polyline points="{poly}" fill="none" stroke="{color}" stroke-width="2.2"/>"##
                ));
            }
            for &(ci, v, px, py) in &pts {
                svg.push_str(&format!(
                    r##"<circle cx="{px:.1}" cy="{py:.1}" r="3.5" fill="{color}" stroke="#ffffff" stroke-width="1"/>"##
                ));
                // A point label (`540 "PaLM"`) and / or the value (`showDataLabel`) above the point.
                let text = match (point_label(ci), style.data_labels) {
                    (Some(l), true) => format!("{} {l}", chart_number(v)),
                    (Some(l), false) => l.to_string(),
                    (None, true) => chart_number(v),
                    (None, false) => continue,
                };
                let (tx, ty, anchor) = if horizontal { (px + 7.0, py + 3.5, "start") } else { (px, py - 8.0, "middle") };
                labels.push_str(&format!(
                    r##"<text x="{tx:.1}" y="{ty:.1}" text-anchor="{anchor}" font-size="10" fill="#1e293b" data-role="data-label">{}</text>"##,
                    escape_xml(&text)
                ));
            }
        } else {
            let lane = bar_series.iter().position(|&b| b == si).unwrap_or(0) as f32;
            for (ci, v_opt) in vals.iter().enumerate() {
                if let Some(v) = v_opt {
                    let category = sample_at(ci, vals.len()) + (lane - lanes / 2.0) * thickness;
                    let (end_x, end_y) = plot_at(category, y_frac(v.0));
                    let (start_x, start_y) = plot_at(category, y_frac(0.0));
                    let (bx, by, bw, bh) = if horizontal {
                        (start_x.min(end_x), end_y, (end_x - start_x).abs().max(2.0), thickness)
                    } else if spec.x_range.is_some() {
                        (end_x, start_y.min(end_y).min(plot_bottom - 2.0), thickness, (end_y - start_y).abs().max(2.0))
                    } else {
                        (end_x, end_y.min(plot_bottom - 2.0), thickness, (plot_bottom - end_y).max(2.0))
                    };
                    svg.push_str(&format!(
                        r##"<rect x="{bx:.1}" y="{by:.1}" width="{bw:.1}" height="{bh:.1}" rx="2" fill="{color}" fill-opacity="0.88"/>"##
                    ));
                    if style.data_labels || point_label(ci).is_some() {
                        let text = match point_label(ci) {
                            Some(l) if style.data_labels => format!("{} {l}", chart_number(v.0)),
                            Some(l) => l.to_string(),
                            None => chart_number(v.0),
                        };
                        let outside = style.data_labels_outside || if horizontal {
                            bw < estimate_text_width(&text, 10.0) + 8.0
                        } else {
                            bh < 18.0
                        };
                        let (tx, ty, anchor) = if horizontal {
                            let right = (end_x >= start_x) == outside;
                            (end_x + if right { 5.0 } else { -5.0 }, by + bh / 2.0 + 3.5, if right { "start" } else { "end" })
                        } else if spec.x_range.is_some() && end_y > start_y {
                            (bx + bw / 2.0, end_y + if outside { 12.0 } else { -5.0 }, "middle")
                        } else {
                            (bx + bw / 2.0, by + if outside { -5.0 } else { 14.0 }, "middle")
                        };
                        let fill = if outside { "#1e293b" } else { "#ffffff" };
                        labels.push_str(&format!(
                            r##"<text x="{tx:.1}" y="{ty:.1}" text-anchor="{anchor}" font-size="10" fill="{fill}" data-role="data-label">{}</text>"##,
                            escape_xml(&text)
                        ));
                    }
                }
            }
        }
        svg.push_str("</g>");
    }
    svg.push_str(&labels);

    if has_legend {
        let ly = height_px as f32 - 14.0;
        let mut lx = pad_l;
        for (si, (sname, _)) in spec.series.iter().enumerate() {
            let color = color_of(si);
            svg.push_str(&format!(
                r##"<rect x="{lx:.1}" y="{:.1}" width="10" height="10" rx="1" fill="{color}" data-role="legend-swatch"/><text x="{:.1}" y="{ly:.1}" font-size="11" fill="#334155" data-role="legend">{}</text>"##,
                ly - 9.0,
                lx + 14.0,
                escape_xml(sname)
            ));
            lx += estimate_text_width(sname, 11.0) + 28.0;
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), width_px, height_px))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn parses_pie_and_xychart_to_native_chart_specs() {
        let pie = "pie showData\n  title ブラウザシェア\n  \"Chrome\" : 65\n  \"Safari\" : 20\n  \"Other\" : 15\n";
        let Some(MermaidOutput::Chart(spec)) = render_mermaid(pie) else {
            panic!("expected native chart for pie");
        };
        assert_eq!(spec.kind, NewChartKind::Pie);
        assert!(spec.show_data);
        assert_eq!(spec.title.as_deref(), Some("ブラウザシェア"));
        assert_eq!(spec.categories, vec!["Chrome", "Safari", "Other"]);
        assert_eq!(spec.series[0].1.len(), 3);
        assert_eq!(spec.series_kinds, vec![NewChartKind::Pie]);

        let xy = "xychart-beta\n  title \"月次売上\"\n  x-axis [Jan, Feb, Mar]\n  bar [100, 140, 180]\n  line [90, 120, 170]\n";
        let Some(MermaidOutput::Chart(xy_spec)) = render_mermaid(xy) else {
            panic!("expected native chart for xychart-beta");
        };
        assert_eq!(xy_spec.kind, NewChartKind::ClusteredColumn);
        assert_eq!(xy_spec.title.as_deref(), Some("月次売上"));
        assert_eq!(xy_spec.series.len(), 2);
        assert_eq!(xy_spec.series_kinds, vec![NewChartKind::ClusteredColumn, NewChartKind::Line]);
    }

    #[test]
    fn xychart_alias_names_and_mixed_series_kinds() {
        let src = "\
xychart
  title Mix
  x-axis [Jan, Feb]
  bar \"売上\" [10, 20]
  line \"目標\" [8, 18]
  bar \"再\" [1, 2]
";
        let Some(MermaidOutput::Chart(spec)) = render_mermaid(src) else {
            panic!("expected chart");
        };
        assert_eq!(spec.kind, NewChartKind::ClusteredColumn);
        assert_eq!(spec.series.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), vec!["売上", "目標", "再"]);
        assert_eq!(
            spec.series_kinds,
            vec![NewChartKind::ClusteredColumn, NewChartKind::Line, NewChartKind::ClusteredColumn]
        );
    }

    #[test]
    fn horizontal_xychart_swaps_geometry_without_rotating_labels() {
        let source = "xychart horizontal\n  x-axis \"月\" [一月, 二月]\n  y-axis \"差分\" -10 --> 10\n  bar [-8, 3]\n";
        let spec = chart_of(source);
        assert_eq!(spec.kind, NewChartKind::ClusteredBar);
        let configured = format!(
            "---\nconfig:\n  xyChart:\n    chartOrientation: horizontal\n---\n{}",
            source.replacen(" horizontal", "", 1)
        );
        assert_eq!(chart_of(&configured), spec);
        let svg = chart_svg(&spec);
        assert!(svg.contains(r#"data-role="x-axis-title">月"#));
        assert!(svg.contains(r#"data-role="y-axis-title">差分"#));
        assert!(svg.contains(r##"text-anchor="end" font-size="11" fill="#334155">一月"##));
        let mixed = format!("{source}  line [-2, 6 \"目標\"]\n");
        let Some(MermaidOutput::Svg { bytes, .. }) = render_mermaid(&mixed) else {
            panic!("horizontal bar and line should render as SVG");
        };
        let svg = String::from_utf8(bytes).unwrap();
        assert!(svg.contains("<polyline"));
        assert!(svg.contains(r#"data-role="data-label">目標"#));
    }

    #[test]
    fn numeric_xychart_places_each_series_across_the_full_range() {
        for (orientation, coordinate) in [("vertical", "x"), ("horizontal", "y")] {
            let source = format!(
                "xychart {orientation}\n  x-axis \"経過秒\" -10 --> 10\n  y-axis -5 --> 20\n  bar [-2, 7, 9]\n  line [2, 4, 6]\n  line [8, 10]\n  line [12 \"開始\"]\n"
            );
            let Some(MermaidOutput::Svg { bytes, .. }) = render_mermaid(&source) else {
                panic!("numeric axis should render as SVG");
            };
            let svg = std::str::from_utf8(&bytes).unwrap();
            let doc = roxmltree::Document::parse(svg).unwrap();
            let nodes = doc.descendants().filter(|n| n.attribute("data-role") == Some("x-tick"));
            let ticks: Vec<_> = nodes.map(|n| (n.text().unwrap(), n.attribute(coordinate).unwrap().parse::<f32>().unwrap())).collect();
            let points: Vec<_> = doc.descendants().filter(|n| n.has_tag_name("circle"))
                .map(|n| n.attribute(if coordinate == "x" { "cx" } else { "cy" }).unwrap().parse::<f32>().unwrap())
                .collect();
            let offset = if coordinate == "y" { 3.5 } else { 0.0 };
            assert_eq!(ticks.first().unwrap().0, "-10");
            assert_eq!(ticks.last().unwrap().0, "10");
            assert!((ticks.first().unwrap().1 - offset - points[0]).abs() < 0.11);
            assert!((ticks.last().unwrap().1 - offset - points[2]).abs() < 0.11);
            assert_eq!(points[3..], [points[0], points[2], points[0]]);
            assert!(svg.contains(r#"data-role="x-axis-title">経過秒"#));
            assert!(svg.contains(r#"data-role="data-label">開始"#));
        }
    }

    #[test]
    fn xychart_axis_titles_and_value_range() {
        let src = "xychart
  title \"Sales Revenue\"
  x-axis \"Month\" [jan, feb, mar]
  y-axis \"Revenue (in $)\" 4000 --> 11000
  bar [5000, 6000, 7500]
";
        let Some(MermaidOutput::Chart(spec)) = render_mermaid(src) else {
            panic!("chart");
        };
        assert_eq!(spec.x_axis_title.as_deref(), Some("Month"));
        assert_eq!(spec.y_axis_title.as_deref(), Some("Revenue (in $)"));
        assert_eq!(spec.y_range, Some((4000.0, 11000.0)));
        assert_eq!(spec.categories, vec!["jan", "feb", "mar"]);
        let (bytes, _, _) = render_chart_spec_svg(&spec).expect("svg");
        let svg = String::from_utf8(bytes).unwrap();
        assert!(svg.contains(r#"data-role="y-axis-title">Revenue (in $)</text>"#), "{svg}");
        assert!(svg.contains(r#"data-role="x-axis-title">Month</text>"#));
        assert!(svg.contains(">4000<") && svg.contains(">11000<"), "{svg}");

        assert_eq!(parse_xy_axis("\"Revenue\""), (Some("Revenue".into()), None));
        assert_eq!(parse_xy_axis("0 --> 100"), (None, Some((0.0, 100.0))));
        assert_eq!(parse_xy_axis("Score 1.5 --> 2.5"), (Some("Score".into()), Some((1.5, 2.5))));
        assert_eq!(parse_xy_axis("\"Low --> High\""), (Some("Low --> High".into()), None));
        assert_eq!(parse_xy_axis("10 --> 5"), (Some("10 --> 5".into()), None));
    }

    fn chart_of(src: &str) -> MermaidChartSpec {
        match render_mermaid(src) {
            Some(MermaidOutput::Chart(spec)) => spec,
            _ => panic!("expected chart"),
        }
    }

    fn chart_svg(spec: &MermaidChartSpec) -> String {
        String::from_utf8(render_chart_spec_svg(spec).expect("svg").0).unwrap()
    }

    #[test]
    fn xychart_config_palette_labels_size_and_title_colour() {
        let src = "---
config:
  xyChart:
    width: 900
    height: 600
    showDataLabel: true
    showDataLabelOutsideBar: true
  themeVariables:
    xyChart:
      titleColor: \"#ff0000\"
      plotColorPalette: '#000000, #0000FF, nope, #0F0'
---
xychart
  title \"T\"
  x-axis [a, b]
  bar [12, 20]
  line [5, 15]
";
        let spec = chart_of(src);
        assert_eq!(spec.style.palette, vec!["#000000", "#0000ff", "#00ff00"]);
        assert!(spec.style.data_labels && spec.style.data_labels_outside);
        assert_eq!(spec.style.size_px, Some((900, 600)));
        assert_eq!(spec.style.title_color.as_deref(), Some("#ff0000"));
        let svg = chart_svg(&spec);
        assert!(svg.contains(r#"width="900" height="600""#), "{svg}");
        assert!(svg.contains(r##"font-weight="bold" fill="#ff0000">T<"##), "{svg}");
        assert!(svg.contains(r##"fill="#000000" fill-opacity"##), "bar takes the first palette colour");
        assert!(svg.contains(r##"stroke="#0000ff" stroke-width="2.2""##), "line takes the second");
        let labels: Vec<_> = svg.match_indices(r#"data-role="data-label">"#).collect();
        assert_eq!(labels.len(), 4, "{svg}");
        assert!(svg.contains(r##"fill="#1e293b" data-role="data-label">12<"##), "outside bar: dark text above");

        // Without `config`: no labels, the built-in colours, the default size.
        let plain = chart_of("xychart\n  x-axis [a, b]\n  bar [12, 20]\n");
        assert_eq!(plain.style, MermaidChartStyle::default());
        let svg = chart_svg(&plain);
        assert!(!svg.contains("data-label") && svg.contains(r#"width="520" height="280""#));
    }

    #[test]
    fn xychart_point_labels() {
        let spec = chart_of("xychart\n  x-axis [Q1, Q2, Q3]\n  line [25 \"Launch\", 45, 90 \"Target Hit\"]\n");
        assert_eq!(spec.series[0].1, vec![Some(Number(25.0)), Some(Number(45.0)), Some(Number(90.0))]);
        assert_eq!(spec.point_labels, vec![vec![Some("Launch".to_string()), None, Some("Target Hit".to_string())]]);
        let svg = chart_svg(&spec);
        assert!(svg.contains(">Launch<") && svg.contains(">Target Hit<"), "{svg}");
        assert_eq!(parse_xy_value("540 \"PaLM\""), (Some(Number(540.0)), Some("PaLM".into())));
        assert_eq!(parse_xy_value(" 3.8 "), (Some(Number(3.8)), None));
        // No labels at all: nothing is kept.
        assert!(chart_of("xychart\n  line [1, 2]\n").point_labels.is_empty());
    }

    #[test]
    fn pie_config_donut_text_position_highlight_and_rim() {
        let src = "---
config:
  pie:
    textPosition: 0.5
    donutHole: 0.2
    highlightSlice: Potassium
  themeVariables:
    pieOuterStrokeWidth: \"5px\"
    pie2: \"#123456\"
---
pie showData
    \"Calcium\" : 42.96
    \"Potassium\" : 50.05
    \"Iron\" :  5
";
        let spec = chart_of(src);
        assert_eq!(spec.kind, NewChartKind::Doughnut);
        assert_eq!(spec.style.text_position, Some(0.5));
        assert_eq!(spec.style.donut_hole, Some(0.2));
        assert_eq!(spec.style.highlight.as_deref(), Some("Potassium"));
        assert_eq!(spec.style.outer_stroke_width, Some(5.0));
        assert_eq!(spec.style.palette[1], "#123456");
        let svg = chart_svg(&spec);
        assert_eq!(svg.matches(r#"data-role="slice-label""#).count(), 3, "{svg}");
        assert_eq!(svg.matches(r#"data-highlight="1""#).count(), 1);
        assert!(svg.contains(r#"data-role="pie-rim""#) && svg.contains(r#"stroke-width="5.0""#));
        assert!(svg.contains(r##"fill="#123456""##));
        assert!(svg.contains("[50.05]"), "showData puts the value in the legend");
        // A doughnut's slices are rings: two arcs each.
        let ring = svg.split("<path d=\"").nth(1).unwrap();
        assert_eq!(ring.split('"').next().unwrap().matches(" A ").count(), 2, "{ring}");
        // textPosition 0.5 puts the first label at half the radius from the centre.
        let label = tag_with(&svg, r#"data-role="slice-label""#)[0];
        let (x, y) = (attr_f32(&label, "x").unwrap(), attr_f32(&label, "y").unwrap() - 4.0);
        let cy = (3 * 24 + 80).max(250) as f32 / 2.0;
        assert!(((x - 130.0).hypot(y - cy) - 43.0).abs() < 1.0, "{label}");

        let plain = chart_of("pie\n  \"A\" : 1\n  \"B\" : 3\n");
        assert_eq!(plain.kind, NewChartKind::Pie);
        let svg = chart_svg(&plain);
        assert!(svg.contains(">25%<") && svg.contains(">75%<"), "{svg}");
        assert!(!svg.contains("pie-rim") && !svg.contains("data-highlight"));
    }
    #[test]
    fn pie_legend_positions_in_svg() {
        let body = "pie showData\n  \"一つだけ\" : 5\n";
        for position in ["top", "bottom", "left", "right", "center", "unknown"] {
            let source = format!("---\ntitle: Report\nconfig:\n  pie:\n    legendPosition: {position}\n---\n{body}");
            let svg = match render_mermaid(&source).unwrap() {
                MermaidOutput::Chart(spec) => {
                    assert_ne!(position, "center");
                    chart_svg(&spec)
                }
                MermaidOutput::Svg { bytes, .. } => {
                    assert_eq!(position, "center");
                    String::from_utf8(bytes).unwrap()
                }
            };
            assert_eq!(svg.matches(">Report<").count(), 1);
            assert!(svg.contains("一つだけ [5] (100.0%)"));
            let circle = tag_with(&svg, r#"data-role="slice""#)[0];
            let cx = attr_f32(circle, "cx").unwrap();
            let cy = attr_f32(circle, "cy").unwrap();
            let radius = attr_f32(circle, "r").unwrap();
            let key = tag_with(&svg, r#"data-role="legend-key""#)[0];
            let x = attr_f32(key, "x").unwrap();
            let y = attr_f32(key, "y").unwrap();
            match position {
                "top" => assert!(y + 12.0 < cy - radius),
                "bottom" => assert!(y > cy + radius),
                "left" => assert!(x + 12.0 < cx - radius),
                "center" => assert!((x - cx).hypot(y - cy) < radius),
                _ => assert!(x > cx + radius),
            }
        }
    }

    #[test]
    fn pie_title_on_the_header_line() {
        let Some(MermaidOutput::Chart(spec)) = render_mermaid("pie title Pets adopted by volunteers\n    \"Dogs\" : 386\n    \"Cats\" : 85\n    \"Rats\" : 15\n") else {
            panic!("expected pie");
        };
        assert_eq!(spec.title.as_deref(), Some("Pets adopted by volunteers"));
        let Some((bytes, _, _)) = render_chart_spec_svg(&spec) else { panic!("svg") };
        let svg = String::from_utf8(bytes).unwrap();
        assert!(svg.contains(">Pets adopted by volunteers</text>"), "{svg}");
        assert!(svg.contains(r#"data-role="slice""#), "{svg}");
    }

    #[test]
    fn xychart_nice_ticks_and_named_series_legend() {
        let ticks = nice_ticks(4000.0, 11000.0, 10);
        assert!(ticks.contains(&5000.0) && ticks.contains(&10500.0) && ticks.contains(&11000.0), "{ticks:?}");
        assert_eq!(format_axis_tick(10500.0), "10500");
        let src = "xychart-beta\n  title \"An Example Chart\"\n  x-axis [\"90d\", \"60d\", \"30d\"]\n  y-axis \"Seconds\" 0 --> 198.2\n  line \"avg\" [48.1, 41.5, 45.7]\n  line \"p50\" [38.2, 36.8, 39.7]\n  line \"p95\" [112.2, 75.3, 103.0]\n";
        let Some(MermaidOutput::Chart(spec)) = render_mermaid(src) else { panic!("expected xy") };
        assert_eq!(spec.series.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["avg", "p50", "p95"]);
        let Some((bytes, _, _)) = render_chart_spec_svg(&spec) else { panic!("svg") };
        let svg = String::from_utf8(bytes).unwrap();
        for label in ["avg", "p50", "p95", "180", "160", "0"] {
            assert!(svg.contains(&format!(">{label}<")), "{label}: {svg}");
        }
        assert!(svg.contains(r#"data-role="legend""#) && svg.contains(r#"data-role="tick""#), "{svg}");
        assert!(svg.contains(r#"data-role="series""#), "{svg}");
    }
}

