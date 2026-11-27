// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `timeline`.

use super::*;

pub(super) struct TimelineEntry {
    pub(super) period: String,
    pub(super) events: Vec<String>,
    /// Index into the `section`s.
    pub(super) section: Option<usize>,
}

pub(super) fn render_timeline_svg(source: &str, config: &MermaidConfig) -> Option<(Vec<u8>, u32, u32)> {
    let mut title: Option<String> = None;
    let mut entries: Vec<TimelineEntry> = Vec::new();
    let mut sections: Vec<String> = Vec::new();

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
            sections.push(rest.trim().to_string());
            continue;
        }
        if let Some((period, rest)) = l.split_once(':') {
            let p = period.trim();
            let evs: Vec<String> = rest
                .split(':')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect();
            if p.is_empty() && !entries.is_empty() {
                if let Some(last) = entries.last_mut() {
                    last.events.extend(evs);
                }
            } else if !p.is_empty() {
                entries.push(TimelineEntry {
                    period: p.to_string(),
                    events: evs,
                    section: sections.len().checked_sub(1),
                });
            }
        }
    }

    if entries.is_empty() {
        return None;
    }

    let col_w = 136.0f32;
    let pad_x = 28.0f32;
    let box_w = col_w - 16.0;
    let line_h = 14.0f32;
    let label_lines = |text: &str, max_w: f32, size: f32| -> Vec<String> {
        label_lines(text, max_w, size).into_iter().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
    };
    // Each event box grows with its lines (`<br>` or wrapping); one line is 28 px.
    let event_lines: Vec<Vec<Vec<String>>> = entries
        .iter()
        .map(|e| e.events.iter().map(|ev| label_lines(ev, box_w - 12.0, 11.5)).collect())
        .collect();
    let box_h = |lines: usize| 28.0 + lines.saturating_sub(1) as f32 * line_h;
    let column_h = |col: &Vec<Vec<String>>| col.iter().map(|l| box_h(l.len()) + 10.0).sum::<f32>();
    let events_h = event_lines.iter().map(column_h).fold(38.0f32, f32::max);
    let period_lines: Vec<Vec<String>> = entries.iter().map(|e| label_lines(&e.period, col_w - 8.0, 12.5)).collect();
    let period_extra = period_lines.iter().map(|l| l.len().saturating_sub(1)).max().unwrap_or(0) as f32 * 15.0;
    let section_lines: Vec<Vec<String>> = sections.iter().map(|s| label_lines(s, col_w * 2.0 - 16.0, 12.0)).collect();
    let section_h = if entries.iter().any(|e| e.section.is_some()) {
        section_lines.iter().map(Vec::len).max().unwrap_or(1) as f32 * 15.0 + 16.0
    } else {
        0.0
    };

    let top_y = if title.is_some() { 54.0f32 } else { 26.0f32 };
    let band_y = top_y;
    let top_y = top_y + if section_h > 0.0 { section_h + 10.0 } else { 0.0 } + period_extra;
    let total_w = (pad_x * 2.0 + entries.len() as f32 * col_w).ceil() as u32;
    let axis_y = top_y + 34.0;
    let total_h = (axis_y + 20.0 + events_h + 34.0).ceil() as u32;

    const ACCENTS: &[&str] = &["#2563eb", "#059669", "#d97706", "#7c3aed", "#db2777"];
    let multicolor = !config.flag("timeline.disableMulticolor", false);
    // Periods in one section share its colour; without sections each period has its own.
    let colour_index = |i: usize| if multicolor { entries[i].section.unwrap_or(i) } else { 0 };
    let colours = |n: usize| {
        let color = config.color_var(&format!("cScale{}", n % 12)).unwrap_or_else(|| ACCENTS[n % ACCENTS.len()].to_string());
        let (box_fill, text_fill) = match config.color_var(&format!("cScaleLabel{}", n % 12)) {
            Some(label) => (color.clone(), label),
            None => ("#f8fafc".to_string(), "#1e293b".to_string()),
        };
        (color, box_fill, text_fill)
    };

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));
    if let Some(t) = &title {
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="28" text-anchor="middle" font-size="15" font-weight="bold" fill="#0f172a">{}</text>"##,
            total_w as f32 / 2.0,
            escape_xml(t)
        ));
    }

    // Section bands span their periods.
    for (s, lines) in section_lines.iter().enumerate() {
        let cols: Vec<usize> = (0..entries.len()).filter(|&i| entries[i].section == Some(s)).collect();
        let (Some(&first), Some(&last)) = (cols.first(), cols.last()) else { continue };
        let (color, _, _) = colours(if multicolor { s } else { 0 });
        let x = pad_x + first as f32 * col_w + 4.0;
        let w = (last - first + 1) as f32 * col_w - 8.0;
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{band_y:.1}" width="{w:.1}" height="{section_h:.1}" rx="6" fill="{color}" fill-opacity="0.16" stroke="{color}" stroke-width="1.3" data-role="section"/>"##
        ));
        for (k, line) in lines.iter().enumerate() {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="12" font-weight="bold" fill="#0f172a">{}</text>"##,
                x + w / 2.0,
                band_y + 19.0 + k as f32 * 15.0,
                escape_xml(line)
            ));
        }
    }

    svg.push_str(&format!(
        r##"<line x1="{pad_x:.1}" y1="{axis_y:.1}" x2="{:.1}" y2="{axis_y:.1}" stroke="#94a3b8" stroke-width="2.5"/>"##,
        total_w as f32 - pad_x
    ));

    for i in 0..entries.len() {
        let cx = pad_x + (i as f32 + 0.5) * col_w;
        // `themeVariables.cScaleN` recolours period (or section) N; `timeline.disableMulticolor` uses one colour.
        let (color, box_fill, text_fill) = colours(colour_index(i));
        svg.push_str(&format!(
            r##"<circle cx="{cx:.1}" cy="{axis_y:.1}" r="7" fill="{color}" stroke="#ffffff" stroke-width="2" data-role="timeline-period"/>"##
        ));
        let plines = &period_lines[i];
        for (k, line) in plines.iter().enumerate() {
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="12.5" font-weight="bold" fill="#0f172a">{}</text>"##,
                axis_y - 14.0 - (plines.len() - 1 - k) as f32 * 15.0,
                escape_xml(line)
            ));
        }
        let mut ey = axis_y + 20.0;
        for lines in &event_lines[i] {
            let bh = box_h(lines.len());
            svg.push_str(&format!(
                r##"<rect x="{:.1}" y="{ey:.1}" width="{box_w:.1}" height="{bh:.1}" rx="6" fill="{box_fill}" stroke="{color}" stroke-width="1.3" data-role="timeline-event"/>"##,
                cx - box_w / 2.0
            ));
            for (k, line) in lines.iter().enumerate() {
                svg.push_str(&format!(
                    r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="11.5" fill="{text_fill}">{}</text>"##,
                    ey + 18.0 + k as f32 * line_h,
                    escape_xml(line)
                ));
            }
            ey += bh + 10.0;
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
    fn timeline_period_colours_and_class_empty_members_from_config() {
        let body = "timeline\n  2001 : A\n  2002 : B\n";
        let svg = svg_of(&format!("---\nconfig:\n  themeVariables:\n    cScale0: '#ff0000'\n    cScaleLabel0: '#ffffff'\n---\n{body}"));
        assert!(svg.contains(r##"fill="#ff0000""##) && svg.contains(r##"fill="#ffffff">A<"##), "{svg}");
        let mono = svg_of(&format!("---\nconfig:\n  timeline:\n    disableMulticolor: true\n---\n{body}"));
        assert!(!mono.contains("#059669"), "second period uses the first colour");
        assert!(svg_of(body).contains("#059669"));

        let height = |src: &str| match render_mermaid(src) {
            Some(MermaidOutput::Svg { height_px, .. }) => height_px,
            _ => panic!("svg"),
        };
        let class = "classDiagram\n  class Duck\n";
        let hidden = height(&format!("---\nconfig:\n  class:\n    hideEmptyMembersBox: true\n---\n{class}"));
        assert!(hidden < height(class), "{hidden}");
    }

    #[test]
    fn timeline_sections_group_periods_and_share_a_colour() {
        let svg = svg_of(
            "timeline\n  title T\n  section 2023 Q1 <br> Release\n    Bullet 1 : a : b\n    Bullet 2 : c<br>d\n  section Q2\n    Bullet 3 : e\n",
        );
        let bands = tag_with(&svg, r#"data-role="section""#);
        assert!(!tag_with(&svg, r#"data-role="timeline-period""#).is_empty(), "{svg}");
        assert!(!tag_with(&svg, r#"data-role="timeline-event""#).is_empty(), "{svg}");
        assert_eq!(bands.len(), 2, "{svg}");
        // The first section spans two periods.
        assert!(attr_f32(bands[0], "width").unwrap() > attr_f32(bands[1], "width").unwrap() * 1.8, "{svg}");
        assert!(svg.contains(">2023 Q1<") && svg.contains(">Release<"), "<br> splits the title: {svg}");
        assert!(svg.contains(">c<") && svg.contains(">d<"), "<br> splits an event: {svg}");
        assert!(!svg.contains("section "), "{svg}");
        let circles: Vec<&str> = tag_with(&svg, "<circle");
        assert_eq!(circles.len(), 3);
        let fill = |t: &str| t.split("fill=\"").nth(1).unwrap().split('"').next().unwrap().to_string();
        assert_eq!(fill(circles[0]), fill(circles[1]), "same section, same colour");
        assert_ne!(fill(circles[1]), fill(circles[2]), "next section, next colour");
        // Without sections each period keeps its own colour.
        let plain = svg_of("timeline\n  2001 : A\n  2002 : B\n");
        let c: Vec<&str> = tag_with(&plain, "<circle");
        assert_ne!(fill(c[0]), fill(c[1]));
    }
}
