// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `venn-beta`: two or three overlapping sets (one works too), sized by `:N` when given,
//! with labels and `text` items in each region.

use super::*;

const PALETTE: &[&str] = &["#4e79a7", "#e15759", "#59a14f", "#f28e2c"];
const RADIUS: f32 = 120.0;
const MARGIN: f32 = 24.0;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct VennText {
    pub(super) id: String,
    pub(super) label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct VennRegion {
    /// Set ids; one for a set, several for a union.
    pub(super) sets: Vec<String>,
    pub(super) label: String,
    pub(super) size: Option<f64>,
    pub(super) texts: Vec<VennText>,
}

#[derive(Debug, Default)]
pub(super) struct Venn {
    pub(super) title: Option<String>,
    pub(super) sets: Vec<VennRegion>,
    pub(super) unions: Vec<VennRegion>,
    pub(super) styles: Vec<(String, FlowStyle)>,
}

/// `A["Alpha"]:20` / `A,B["AB"]:3` / `Desirable` → (ids, label, size).
fn parse_region(rest: &str) -> Option<(Vec<String>, Option<String>, Option<f64>)> {
    let rest = rest.trim();
    let (head, label, tail) = match rest.find('[') {
        Some(open) => {
            let close = rest.rfind(']')?;
            (&rest[..open], Some(unquote(rest[open + 1..close].trim())), &rest[close + 1..])
        }
        None => match rest.rsplit_once(':') {
            Some((h, t)) if t.trim().parse::<f64>().is_ok() => (h, None, &rest[h.len()..]),
            _ => (rest, None, ""),
        },
    };
    let size = tail.trim().strip_prefix(':').and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite() && *v > 0.0);
    let ids: Vec<String> = head.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    (!ids.is_empty()).then_some((ids, label, size))
}

pub(super) fn parse_venn(source: &str) -> Venn {
    let mut v = Venn::default();
    // Where `text` lines go: (is a union, index).
    let mut open: Option<(bool, usize)> = None;
    for raw in source.lines().skip(1) {
        let l = raw.split_once("%%").map_or(raw, |(c, _)| c).trim();
        if l.is_empty() {
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ") {
            v.title = Some(unquote(rest.trim()));
        } else if let Some(rest) = l.strip_prefix("set ") {
            if let Some((ids, label, size)) = parse_region(rest) {
                let id = ids[0].clone();
                v.sets.push(VennRegion { label: label.unwrap_or_else(|| id.clone()), sets: vec![id], size, texts: Vec::new() });
                open = Some((false, v.sets.len() - 1));
            }
        } else if let Some(rest) = l.strip_prefix("union ") {
            if let Some((ids, label, size)) = parse_region(rest) {
                v.unions.push(VennRegion { label: label.unwrap_or_default(), sets: ids, size, texts: Vec::new() });
                open = Some((true, v.unions.len() - 1));
            }
        } else if let Some(rest) = l.strip_prefix("text ") {
            if let (Some((ids, label, _)), Some((union, i))) = (parse_region(rest), open) {
                let text = VennText { label: label.unwrap_or_else(|| ids[0].clone()), id: ids[0].clone() };
                if union { v.unions[i].texts.push(text) } else { v.sets[i].texts.push(text) }
            }
        } else if let Some(rest) = l.strip_prefix("style ") {
            if let Some((ids, props)) = rest.trim().split_once(char::is_whitespace) {
                let style = parse_flow_style(props);
                for id in ids.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    v.styles.push((id.to_string(), style.clone()));
                }
            }
        }
    }
    v
}

/// Area of the overlap of two circles `d` apart.
pub(super) fn lens_area(r1: f32, r2: f32, d: f32) -> f32 {
    if d >= r1 + r2 {
        return 0.0;
    }
    if d <= (r1 - r2).abs() {
        return std::f32::consts::PI * r1.min(r2).powi(2);
    }
    let a = ((d * d + r1 * r1 - r2 * r2) / (2.0 * d * r1)).clamp(-1.0, 1.0).acos();
    let b = ((d * d + r2 * r2 - r1 * r1) / (2.0 * d * r2)).clamp(-1.0, 1.0).acos();
    r1 * r1 * (a - a.sin() * a.cos()) + r2 * r2 * (b - b.sin() * b.cos())
}

/// The distance at which two circles overlap by `area` (bisection).
pub(super) fn distance_for_overlap(r1: f32, r2: f32, area: f32) -> f32 {
    let (mut lo, mut hi) = ((r1 - r2).abs(), r1 + r2);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.0;
        if lens_area(r1, r2, mid) > area { lo = mid } else { hi = mid }
    }
    (lo + hi) / 2.0
}

pub(super) struct VennLayout {
    pub(super) centers: Vec<(f32, f32)>,
    pub(super) radii: Vec<f32>,
}

/// Circle radii from the set sizes (area in proportion), and centres from the union sizes.
pub(super) fn layout_venn(v: &Venn) -> VennLayout {
    let n = v.sets.len();
    let max = v.sets.iter().filter_map(|s| s.size).fold(0.0f64, f64::max);
    let radii: Vec<f32> = v
        .sets
        .iter()
        .map(|s| match s.size {
            Some(size) if max > 0.0 => (RADIUS * (size / max).sqrt() as f32).max(RADIUS * 0.35),
            _ => RADIUS,
        })
        .collect();
    let dist = |i: usize, j: usize| -> f32 {
        let (ri, rj) = (radii[i], radii[j]);
        let pair = v.unions.iter().find(|u| {
            u.sets.len() == 2 && u.sets.contains(&v.sets[i].sets[0]) && u.sets.contains(&v.sets[j].sets[0])
        });
        match (pair.and_then(|u| u.size), v.sets[i].size) {
            (Some(u), Some(si)) if si > 0.0 => {
                let area = std::f32::consts::PI * ri * ri * (u / si).min(1.0) as f32;
                distance_for_overlap(ri, rj, area).max((ri - rj).abs() + 10.0)
            }
            _ => (ri + rj) * 0.55,
        }
    };
    let centers = match n {
        0 => Vec::new(),
        1 => vec![(0.0, 0.0)],
        2 => vec![(0.0, 0.0), (dist(0, 1), 0.0)],
        _ => {
            // The first three sets on a triangle with the pair distances as sides.
            let (a, b, c) = (dist(0, 1), dist(0, 2), dist(1, 2));
            let x = ((a * a + b * b - c * c) / (2.0 * a)).clamp(-b, b);
            let y = (b * b - x * x).max(0.0).sqrt();
            let mut out = vec![(0.0, 0.0), (a, 0.0), (x, y)];
            // More sets go on a ring around the first three.
            let (gx, gy) = ((a + x) / 3.0, y / 3.0);
            for k in 3..n {
                let t = k as f32 / n as f32 * std::f32::consts::TAU;
                out.push((gx + t.cos() * RADIUS * 1.2, gy + t.sin() * RADIUS * 1.2));
            }
            out
        }
    };
    VennLayout { centers, radii }
}

fn normalize((x, y): (f32, f32)) -> (f32, f32) {
    let len = (x * x + y * y).sqrt();
    if len < 1e-3 { (0.0, -1.0) } else { (x / len, y / len) }
}

/// Where a region's text block goes: the part of the sets in `members` outside the others.
fn region_point(lay: &VennLayout, members: &[usize]) -> (f32, f32) {
    let n = lay.centers.len();
    let c = &lay.centers;
    let r = &lay.radii;
    let mean = |idx: &[usize]| {
        let k = idx.len().max(1) as f32;
        (idx.iter().map(|&i| c[i].0).sum::<f32>() / k, idx.iter().map(|&i| c[i].1).sum::<f32>() / k)
    };
    let others: Vec<usize> = (0..n).filter(|i| !members.contains(i)).collect();
    match members.len() {
        1 if n == 1 => c[0],
        1 => {
            let i = members[0];
            let m = mean(&others);
            let u = normalize((c[i].0 - m.0, c[i].1 - m.1));
            if n == 2 {
                // Between the far edge of this circle and the near edge of the other one.
                let j = others[0];
                let far = c[i].0 * u.0 + c[i].1 * u.1 + r[i];
                let near = (c[j].0 * u.0 + c[j].1 * u.1 + r[j]).max(c[i].0 * u.0 + c[i].1 * u.1 - r[i]);
                let t = (far + near) / 2.0 - (c[i].0 * u.0 + c[i].1 * u.1);
                (c[i].0 + u.0 * t, c[i].1 + u.1 * t)
            } else {
                (c[i].0 + u.0 * r[i] * 0.45, c[i].1 + u.1 * r[i] * 0.45)
            }
        }
        2 => {
            let (i, j) = (members[0], members[1]);
            let u = normalize((c[j].0 - c[i].0, c[j].1 - c[i].1));
            let d = (c[j].0 - c[i].0) * u.0 + (c[j].1 - c[i].1) * u.1;
            // The middle of the lens along the line between the centres.
            let t = ((d - r[j]) + r[i]) / 2.0;
            let lens = (c[i].0 + u.0 * t, c[i].1 + u.1 * t);
            if others.is_empty() {
                lens
            } else {
                let m = mean(&others);
                let away = normalize((lens.0 - m.0, lens.1 - m.1));
                let push = r[i].min(r[j]) * 0.22;
                (lens.0 + away.0 * push, lens.1 + away.1 * push)
            }
        }
        _ => mean(members),
    }
}

pub(super) fn render_venn_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let v = parse_venn(source);
    if v.sets.is_empty() {
        return None;
    }
    let lay = layout_venn(&v);
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (c, r) in lay.centers.iter().zip(&lay.radii) {
        x0 = x0.min(c.0 - r);
        y0 = y0.min(c.1 - r);
        x1 = x1.max(c.0 + r);
        y1 = y1.max(c.1 + r);
    }
    let (dx, dy) = (MARGIN - x0, MARGIN - y0);
    let total_w = (x1 - x0 + MARGIN * 2.0).ceil();
    let total_h = (y1 - y0 + MARGIN * 2.0).ceil();
    let style_of = |id: &str| {
        let mut st = FlowStyle::default();
        for (sid, s) in &v.styles {
            if sid == id {
                st.merge(s);
            }
        }
        st
    };
    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));
    for (i, set) in v.sets.iter().enumerate() {
        let (cx, cy) = (lay.centers[i].0 + dx, lay.centers[i].1 + dy);
        let st = style_of(&set.sets[0]);
        let base = PALETTE[i % PALETTE.len()];
        let fill = st.fill.as_deref().and_then(safe_css_value).unwrap_or_else(|| base.to_string());
        let stroke = st.stroke.as_deref().and_then(safe_css_value).unwrap_or_else(|| base.to_string());
        svg.push_str(&format!(
            r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{:.1}" fill="{fill}" fill-opacity="0.28" stroke="{stroke}" stroke-width="2" data-set="{}"/>"##,
            lay.radii[i],
            escape_attr(&set.sets[0])
        ));
    }
    let index_of = |id: &str| v.sets.iter().position(|s| s.sets[0] == id);
    let regions = v.sets.iter().enumerate().map(|(i, s)| (vec![i], s)).chain(
        v.unions.iter().filter_map(|u| u.sets.iter().map(|id| index_of(id)).collect::<Option<Vec<usize>>>().map(|m| (m, u))),
    );
    for (members, region) in regions {
        let p = region_point(&lay, &members);
        let (px, py) = (p.0 + dx, p.1 + dy);
        let key = region.sets.join(",");
        let st = if members.len() == 1 { style_of(&region.sets[0]) } else { style_of(&key) };
        let color = st.color.as_deref().and_then(safe_css_value).unwrap_or_else(|| "#0f172a".to_string());
        let mut lines: Vec<(String, f32, bool, String)> = Vec::new();
        if !region.label.is_empty() {
            lines.push((region.label.clone(), if members.len() == 1 { 15.0 } else { 13.0 }, true, color.clone()));
        }
        for t in &region.texts {
            let tc = style_of(&t.id).color.as_deref().and_then(safe_css_value).unwrap_or_else(|| color.clone());
            lines.push((t.label.clone(), 12.0, false, tc));
        }
        let block: f32 = lines.iter().map(|l| l.1 * 1.3).sum();
        let mut y = py - block / 2.0;
        for (text, size, bold, fill) in lines {
            y += size * 1.3;
            let weight = if bold { r#" font-weight="bold""# } else { "" };
            svg.push_str(&format!(
                r##"<text x="{px:.1}" y="{:.1}" text-anchor="middle" font-size="{size}"{weight} fill="{fill}" data-region="{}">{}</text>"##,
                y - size * 0.3,
                escape_attr(&key),
                escape_xml(&text)
            ));
        }
    }
    svg.push_str("</svg>");
    let bytes = svg.into_bytes();
    Some(match &v.title {
        Some(title) => add_svg_title(bytes, total_w as u32, total_h as u32, title),
        None => (bytes, total_w as u32, total_h as u32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn venn_sets_unions_texts_and_styles_parse() {
        let v = parse_venn(
            "venn-beta\n  title \"Team\"\n  set A[\"Alpha\"]:20\n    text A1[\"React\"]\n  set B:12\n  union A,B[\"AB\"]:3\n    text AB1[\"OpenAPI\"]\n  style A,B color:#333\n  style A1 color:red",
        );
        assert_eq!(v.title.as_deref(), Some("Team"));
        assert_eq!(v.sets[0], VennRegion { sets: vec!["A".into()], label: "Alpha".into(), size: Some(20.0), texts: vec![VennText { id: "A1".into(), label: "React".into() }] });
        assert_eq!((v.sets[1].label.as_str(), v.sets[1].size), ("B", Some(12.0)));
        assert_eq!(v.unions[0].sets, vec!["A", "B"]);
        assert_eq!(v.unions[0].texts[0].label, "OpenAPI");
        assert_eq!(v.styles.len(), 3);
    }

    #[test]
    fn venn_overlap_follows_the_union_size() {
        let a = lens_area(50.0, 50.0, 60.0);
        assert!((distance_for_overlap(50.0, 50.0, a) - 60.0).abs() < 0.01);
        assert_eq!(lens_area(10.0, 10.0, 25.0), 0.0);
        let v = parse_venn("venn-beta\n  set A:20\n  set B:12\n  union A,B:3");
        let lay = layout_venn(&v);
        assert!(lay.radii[1] < lay.radii[0]);
        let overlap = lens_area(lay.radii[0], lay.radii[1], lay.centers[1].0);
        let share = overlap / (std::f32::consts::PI * lay.radii[0] * lay.radii[0]);
        assert!((share - 3.0 / 20.0).abs() < 0.01, "{share}");
    }

    #[test]
    fn venn_three_sets_regions_sit_where_they_belong() {
        let svg = svg_of(
            "venn-beta\n  title T\n  set D\n  set F\n  set V\n  union D,F[\"DF\"]\n  union D,F,V[\"All\"]\n  style D fill:#ff6b6b",
        );
        assert!(svg.contains(r##"fill="#ff6b6b" fill-opacity="0.28""##));
        let circles: Vec<(f32, f32, f32)> = tag_with(&svg, "data-set=")
            .iter()
            .map(|t| (attr_f32(t, "cx").unwrap(), attr_f32(t, "cy").unwrap(), attr_f32(t, "r").unwrap()))
            .collect();
        let inside = |x: f32, y: f32, c: (f32, f32, f32)| (x - c.0).hypot(y - c.1) < c.2;
        let at = |label: &str| (text_x(&svg, label), text_y(&svg, label));
        let (x, y) = at("D");
        assert!(inside(x, y, circles[0]) && !inside(x, y, circles[1]) && !inside(x, y, circles[2]));
        let (x, y) = at("DF");
        assert!(inside(x, y, circles[0]) && inside(x, y, circles[1]) && !inside(x, y, circles[2]));
        let (x, y) = at("All");
        assert!(circles.iter().all(|&c| inside(x, y, c)));
    }
}
