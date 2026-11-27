// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `radar-beta`: axes around a centre, a graticule (circles or polygons) and one closed,
//! rounded curve per data series, with a legend.

use super::*;

const PALETTE: &[&str] = &["#4e79a7", "#e15759", "#59a14f", "#f28e2c", "#af7aa1", "#76b7b2", "#edc949", "#ff9da7"];

#[derive(Debug, Clone, PartialEq)]
pub(super) struct RadarAxis {
    pub(super) id: String,
    pub(super) label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct RadarCurve {
    pub(super) id: String,
    pub(super) label: String,
    /// One value per axis, in axis order.
    pub(super) values: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Radar {
    pub(super) title: Option<String>,
    pub(super) axes: Vec<RadarAxis>,
    pub(super) curves: Vec<RadarCurve>,
    pub(super) max: Option<f64>,
    pub(super) min: f64,
    pub(super) ticks: usize,
    pub(super) polygon: bool,
    pub(super) legend: bool,
}

/// Split on commas outside `[...]`, `{...}` and quotes.
fn split_top(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut quote, mut start) = (0i32, false, 0usize);
    for (i, ch) in s.char_indices() {
        match ch {
            '"' => quote = !quote,
            '[' | '{' if !quote => depth += 1,
            ']' | '}' if !quote => depth -= 1,
            ',' if !quote && depth == 0 => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(s[start..].trim());
    out.retain(|p| !p.is_empty());
    out
}

/// `id["Label"]` → (id, label).
fn id_label(s: &str) -> (String, String) {
    match s.find('[') {
        Some(open) if s.ends_with(']') => {
            let id = s[..open].trim().to_string();
            (id, unquote(s[open + 1..s.len() - 1].trim()))
        }
        _ => (s.trim().to_string(), s.trim().to_string()),
    }
}

pub(super) fn parse_radar(source: &str) -> Radar {
    let mut r = Radar { title: None, axes: Vec::new(), curves: Vec::new(), max: None, min: 0.0, ticks: 5, polygon: false, legend: true };
    // Curves keep their raw entries until every axis is known.
    let mut raw_curves: Vec<(String, String, Vec<String>)> = Vec::new();
    for raw in source.lines().skip(1) {
        let l = raw.split_once("%%").map_or(raw, |(c, _)| c).trim();
        if l.is_empty() {
            continue;
        }
        let (word, rest) = l.split_once(char::is_whitespace).unwrap_or((l, ""));
        let rest = rest.trim();
        match word {
            "title" => r.title = Some(unquote(rest)),
            "axis" => {
                for part in split_top(rest) {
                    let (id, label) = id_label(part);
                    r.axes.push(RadarAxis { id, label });
                }
            }
            "curve" => {
                for part in split_top(rest) {
                    let Some(open) = part.find('{') else { continue };
                    let Some(body) = part[open + 1..].strip_suffix('}') else { continue };
                    let (id, label) = id_label(part[..open].trim());
                    raw_curves.push((id, label, body.split(',').map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect()));
                }
            }
            "max" => r.max = rest.parse::<f64>().ok().filter(|v| v.is_finite()),
            "min" => r.min = rest.parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(0.0),
            "ticks" => r.ticks = rest.parse::<usize>().ok().filter(|&t| t > 0).unwrap_or(5).min(20),
            "graticule" => r.polygon = rest == "polygon",
            "showLegend" => r.legend = rest != "false",
            _ => {}
        }
    }
    for (id, label, entries) in raw_curves {
        let mut values = vec![0.0; r.axes.len()];
        for (k, entry) in entries.iter().enumerate() {
            // `{ m: 85, s: 90 }` names the axis; `{85, 90}` goes in axis order.
            let (axis, value) = match entry.split_once(':') {
                Some((a, v)) => (r.axes.iter().position(|x| x.id == a.trim()), v.trim()),
                None => (Some(k).filter(|&k| k < r.axes.len()), entry.as_str()),
            };
            if let (Some(a), Ok(v)) = (axis, value.parse::<f64>()) {
                values[a] = if v.is_finite() { v } else { 0.0 };
            }
        }
        r.curves.push(RadarCurve { id, label, values });
    }
    r
}

/// A closed curve through `points` with cardinal-spline tangents scaled by `tension`
/// (0 = straight polygon).
pub(super) fn closed_curve_d(points: &[(f32, f32)], tension: f32) -> String {
    let n = points.len();
    if n == 0 {
        return String::new();
    }
    let mut d = format!("M {:.1} {:.1}", points[0].0, points[0].1);
    for i in 0..n {
        let p0 = points[(i + n - 1) % n];
        let p1 = points[i];
        let p2 = points[(i + 1) % n];
        let p3 = points[(i + 2) % n];
        let c1 = (p1.0 + (p2.0 - p0.0) * tension, p1.1 + (p2.1 - p0.1) * tension);
        let c2 = (p2.0 - (p3.0 - p1.0) * tension, p2.1 - (p3.1 - p1.1) * tension);
        d.push_str(&format!(" C {:.1} {:.1} {:.1} {:.1} {:.1} {:.1}", c1.0, c1.1, c2.0, c2.1, p2.0, p2.1));
    }
    d.push_str(" Z");
    d
}

pub(super) fn render_radar_svg(source: &str, config: &MermaidConfig) -> Option<(Vec<u8>, u32, u32)> {
    let r = parse_radar(source);
    if r.axes.len() < 3 && r.curves.is_empty() {
        return None;
    }
    if r.axes.is_empty() {
        return None;
    }
    let number = |key: &str, default: f32, lo: f32, hi: f32| config.option(key).and_then(px_number).map_or(default, |v| (v as f32).clamp(lo, hi));
    let width = number("radar.width", 500.0, 100.0, 3000.0);
    let height = number("radar.height", 500.0, 100.0, 3000.0);
    let margin = |side: &str| number(&format!("radar.margin{side}"), 50.0, 0.0, 500.0);
    let (mt, mb, ml, mr) = (margin("Top"), margin("Bottom"), margin("Left"), margin("Right"));
    let scale = number("radar.axisScaleFactor", 1.0, 0.05, 2.0);
    let label_factor = number("radar.axisLabelFactor", 1.05, 0.5, 2.0);
    let tension = number("radar.curveTension", 0.17, 0.0, 1.0);
    let var = |key: &str| config.vars.get(&format!("radar.{key}")).cloned();
    let opacity = var("curveOpacity").and_then(|v| px_number(&v)).map_or(0.5, |v| v.clamp(0.0, 1.0) as f32);
    let stroke_w = var("curveStrokeWidth").and_then(|v| px_number(&v)).map_or(2.0, |v| v.clamp(0.0, 10.0) as f32);
    let axis_color = config.color_var("radar.axisColor").unwrap_or_else(|| "#334155".to_string());
    let grid_color = config.color_var("radar.graticuleColor").unwrap_or_else(|| "#94a3b8".to_string());
    let grid_opacity = var("graticuleOpacity").and_then(|v| px_number(&v)).map_or(0.6, |v| v.clamp(0.0, 1.0) as f32);

    let radius = width.min(height) / 2.0;
    let n = r.axes.len();
    // Axis labels stick out sideways: widen the left / right margins to hold them.
    let (mut over_l, mut over_r) = (0.0f32, 0.0f32);
    for (i, axis) in r.axes.iter().enumerate() {
        let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * i as f32 / n as f32;
        let lx = width / 2.0 + a.cos() * radius * label_factor;
        let tw = estimate_text_width(&axis.label, 13.0);
        let (x0, x1) = if a.cos() > 0.2 {
            (lx, lx + tw)
        } else if a.cos() < -0.2 {
            (lx - tw, lx)
        } else {
            (lx - tw / 2.0, lx + tw / 2.0)
        };
        over_l = over_l.max(-x0 - ml + 8.0);
        over_r = over_r.max(x1 - width - mr + 8.0);
    }
    let (ml, mr) = (ml + over_l.max(0.0), mr + over_r.max(0.0));
    let (cx, cy) = (ml + width / 2.0, mt + height / 2.0);
    let max = r.max.unwrap_or_else(|| r.curves.iter().flat_map(|c| c.values.iter().copied()).fold(0.0, f64::max)).max(r.min + 1e-9);
    let angle = |i: usize| -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * i as f32 / n as f32;
    let at = |i: usize, dist: f32| (cx + angle(i).cos() * dist, cy + angle(i).sin() * dist);
    let legend_w = if r.legend && !r.curves.is_empty() {
        r.curves.iter().map(|c| estimate_text_width(&c.label, 12.0)).fold(0.0f32, f32::max) + 40.0
    } else {
        0.0
    };
    let total_w = (ml + width + mr + legend_w).ceil();
    let total_h = (mt + height + mb).ceil();
    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));
    for k in 1..=r.ticks {
        let dist = radius * k as f32 / r.ticks as f32;
        if r.polygon {
            let pts: Vec<String> = (0..n).map(|i| at(i, dist)).map(|(x, y)| format!("{x:.1},{y:.1}")).collect();
            svg.push_str(&format!(
                r##"<polygon points="{}" fill="none" stroke="{grid_color}" stroke-opacity="{grid_opacity}" stroke-width="1" data-role="graticule"/>"##,
                pts.join(" ")
            ));
        } else {
            svg.push_str(&format!(
                r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{dist:.1}" fill="none" stroke="{grid_color}" stroke-opacity="{grid_opacity}" stroke-width="1" data-role="graticule"/>"##
            ));
        }
    }
    for (i, axis) in r.axes.iter().enumerate() {
        let (x, y) = at(i, radius * scale);
        svg.push_str(&format!(
            r##"<line x1="{cx:.1}" y1="{cy:.1}" x2="{x:.1}" y2="{y:.1}" stroke="{axis_color}" stroke-width="1.5" data-axis="{}"/>"##,
            escape_attr(&axis.id)
        ));
        let (lx, ly) = at(i, radius * label_factor);
        let cos = angle(i).cos();
        let anchor = if cos > 0.2 { "start" } else if cos < -0.2 { "end" } else { "middle" };
        let sin = angle(i).sin();
        let dy = if sin > 0.2 { 12.0 } else if sin < -0.2 { -4.0 } else { 4.0 };
        svg.push_str(&format!(
            r##"<text x="{lx:.1}" y="{:.1}" text-anchor="{anchor}" font-size="13" fill="#0f172a">{}</text>"##,
            ly + dy,
            escape_xml(&axis.label)
        ));
    }
    let color = |k: usize| config.color_var(&format!("cScale{}", k % 12)).unwrap_or_else(|| PALETTE[k % PALETTE.len()].to_string());
    for (k, curve) in r.curves.iter().enumerate() {
        let pts: Vec<(f32, f32)> = curve
            .values
            .iter()
            .enumerate()
            .map(|(i, &v)| at(i, radius * (((v - r.min) / (max - r.min)).clamp(0.0, 1.0)) as f32))
            .collect();
        let c = color(k);
        svg.push_str(&format!(
            r##"<path d="{}" fill="{c}" fill-opacity="{opacity}" stroke="{c}" stroke-width="{stroke_w}" data-curve="{}"/>"##,
            closed_curve_d(&pts, tension),
            escape_attr(&curve.id)
        ));
    }
    if legend_w > 0.0 {
        let x = ml + width + mr;
        for (k, curve) in r.curves.iter().enumerate() {
            let y = mt + k as f32 * 22.0;
            svg.push_str(&format!(
                r##"<rect x="{x:.1}" y="{y:.1}" width="12" height="12" fill="{}" fill-opacity="{opacity}" stroke="{}"/>"##,
                color(k),
                color(k)
            ));
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" font-size="12" fill="#0f172a">{}</text>"##,
                x + 18.0,
                y + 10.5,
                escape_xml(&curve.label)
            ));
        }
    }
    svg.push_str("</svg>");
    let bytes = svg.into_bytes();
    Some(match &r.title {
        Some(title) => add_svg_title(bytes, total_w as u32, total_h as u32, title),
        None => (bytes, total_w as u32, total_h as u32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn radar_axes_curves_and_options_parse() {
        let r = parse_radar(
            "radar-beta\n  title Grades\n  axis m[\"Math\"], s[\"Science\"]\n  axis e\n  curve a[\"Alice\"]{85, 90, 80}, b{70, 75}\n  curve c{ e: 3, m: 1 }\n  max 100\n  min 10\n  ticks 4\n  graticule polygon\n  showLegend false",
        );
        assert_eq!(r.title.as_deref(), Some("Grades"));
        assert_eq!(r.axes.iter().map(|a| a.label.as_str()).collect::<Vec<_>>(), vec!["Math", "Science", "e"]);
        assert_eq!(r.curves[0], RadarCurve { id: "a".into(), label: "Alice".into(), values: vec![85.0, 90.0, 80.0] });
        assert_eq!(r.curves[1].values, vec![70.0, 75.0, 0.0]);
        assert_eq!(r.curves[2].values, vec![1.0, 0.0, 3.0]);
        assert_eq!((r.max, r.min, r.ticks, r.polygon, r.legend), (Some(100.0), 10.0, 4, true, false));
    }

    #[test]
    fn closed_curve_passes_through_its_points() {
        let pts = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)];
        assert_eq!(closed_curve_d(&pts, 0.0), "M 0.0 0.0 C 0.0 0.0 10.0 0.0 10.0 0.0 C 10.0 0.0 10.0 10.0 10.0 10.0 C 10.0 10.0 0.0 0.0 0.0 0.0 Z");
        let d = closed_curve_d(&pts, 0.17);
        assert!(d.ends_with("0.0 0.0 Z") && d.contains(" 10.0 10.0 C"));
    }

    #[test]
    fn radar_svg_scale_graticule_legend_and_theme() {
        let svg = svg_of("---\nconfig:\n  themeVariables:\n    cScale0: \"#FF0000\"\n    radar:\n      curveOpacity: 0\n---\nradar-beta\n  axis A, B, C, D\n  curve c1[\"One\"]{4, 0, 2, 0}\n  max 4\n  graticule polygon");
        assert_eq!(svg.matches(r#"data-role="graticule""#).count(), 5);
        assert!(svg.contains("<polygon"));
        assert!(svg.contains(r##"fill="#FF0000" fill-opacity="0" stroke="#FF0000""##), "{svg}");
        assert!(svg.contains(">One</text>"));
        // Value 4 of 4 on the first (upward) axis reaches the full radius: 250 above the centre.
        assert!(svg.contains(r#"d="M 300.0 50.0 C"#), "{svg}");
    }
}
