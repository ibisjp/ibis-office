// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `journey` (user journey).

use super::*;

pub(super) struct JourneyStep {
    pub(super) section: String,
    pub(super) task: String,
    pub(super) score: f32, // 1..=5
    pub(super) actors: Vec<String>,
}

pub(super) fn render_journey_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let mut title: Option<String> = None;
    let mut section = String::from("Journey");
    let mut steps: Vec<JourneyStep> = Vec::new();

    for raw in source.lines().skip(1) {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ") {
            title = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = l.strip_prefix("section ") {
            section = rest.trim().to_string();
            continue;
        }
        let parts: Vec<&str> = l.split(':').map(str::trim).collect();
        if parts.len() >= 2 {
            let task = parts[0].to_string();
            let score = parts[1].parse::<f32>().unwrap_or(3.0).clamp(1.0, 5.0);
            let actors = if parts.len() >= 3 {
                parts[2]
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToString::to_string)
                    .collect()
            } else {
                Vec::new()
            };
            steps.push(JourneyStep {
                section: section.clone(),
                task,
                score,
                actors,
            });
        }
    }

    if steps.is_empty() {
        return None;
    }

    let col_w = 112.0f32;
    let pad_l = 32.0f32;
    let pad_r = 32.0f32;
    let top_y = if title.is_some() { 52.0f32 } else { 24.0f32 };
    let chart_h = 150.0f32;
    let total_w = (pad_l + pad_r + steps.len() as f32 * col_w).ceil() as u32;
    let total_h = (top_y + chart_h + 76.0).ceil() as u32;

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(
        r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##
    ));
    if let Some(t) = &title {
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="28" text-anchor="middle" font-size="15" font-weight="bold" fill="#0f172a">{}</text>"##,
            total_w as f32 / 2.0,
            escape_xml(t)
        ));
    }

    // Score guide lines (1..5)
    for s in 1..=5 {
        let gy = top_y + chart_h * (1.0 - (s as f32 - 1.0) / 4.0);
        svg.push_str(&format!(
            r##"<line x1="{pad_l:.1}" y1="{gy:.1}" x2="{:.1}" y2="{gy:.1}" stroke="#e2e8f0" stroke-width="1"/>"##,
            total_w as f32 - pad_r
        ));
    }

    // Polyline connecting scores
    let mut pts: Vec<(f32, f32)> = Vec::with_capacity(steps.len());
    for (i, st) in steps.iter().enumerate() {
        let cx = pad_l + (i as f32 + 0.5) * col_w;
        let cy = top_y + chart_h * (1.0 - (st.score - 1.0) / 4.0);
        pts.push((cx, cy));
    }
    if pts.len() >= 2 {
        let poly: String = pts
            .iter()
            .map(|(x, y)| format!("{x:.1},{y:.1}"))
            .collect::<Vec<_>>()
            .join(" ");
        svg.push_str(&format!(
            r##"<polyline points="{poly}" fill="none" stroke="#3b82f6" stroke-width="2.2"/>"##
        ));
    }

    let mut prev_sec: Option<&str> = None;
    for (i, st) in steps.iter().enumerate() {
        let (cx, cy) = pts[i];
        let color = if st.score >= 4.0 {
            "#10b981"
        } else if st.score >= 3.0 {
            "#f59e0b"
        } else {
            "#ef4444"
        };
        if prev_sec != Some(st.section.as_str()) {
            prev_sec = Some(st.section.as_str());
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="11" font-weight="bold" fill="#475569">{}</text>"##,
                top_y + chart_h + 22.0,
                escape_xml(&st.section)
            ));
        }
        svg.push_str(&format!(
            r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="11" fill="{color}" stroke="#ffffff" stroke-width="2" data-role="journey-task"/>"##
        ));
        svg.push_str(&format!(
            r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="11" font-weight="bold" fill="#ffffff">{:.0}</text>"##,
            cy + 4.0,
            st.score
        ));
        svg.push_str(&format!(
            r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="11.5" fill="#0f172a">{}</text>"##,
            top_y + chart_h + 40.0,
            escape_xml(&st.task)
        ));
        if !st.actors.is_empty() {
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="10" fill="#64748b">{}</text>"##,
                top_y + chart_h + 56.0,
                escape_xml(&st.actors.join(", "))
            ));
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}
