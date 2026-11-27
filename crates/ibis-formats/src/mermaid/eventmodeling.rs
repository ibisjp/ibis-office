// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `eventmodeling`: event models. Each `tf` / `timeframe` (or `rf` / `resetframe`) line is one
//! step in a column of its own; UI and processors sit in the top lane, commands and read
//! models in the middle, events at the bottom, and `Stream.Event` names get a lane per stream.
//! Steps link to the previous step (a reset frame starts a new chain) and to `->> NN` sources;
//! `{ … }` and `[[data]]` show the payload.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EmKind {
    Ui,
    Command,
    Event,
    ReadModel,
    Processor,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct EmStep {
    pub(super) frame: String,
    pub(super) reset: bool,
    pub(super) kind: EmKind,
    pub(super) name: String,
    pub(super) inline: Vec<String>,
    pub(super) data_ref: Option<String>,
    pub(super) sources: Vec<String>,
}

#[derive(Debug, Default)]
pub(super) struct EventModel {
    pub(super) steps: Vec<EmStep>,
    pub(super) data: Vec<(String, Vec<String>)>,
}

fn em_kind(word: &str) -> Option<EmKind> {
    Some(match word {
        "ui" => EmKind::Ui,
        "cmd" | "command" => EmKind::Command,
        "evt" | "event" => EmKind::Event,
        "rmo" | "readmodel" | "readModel" => EmKind::ReadModel,
        "pcr" | "processor" | "automation" => EmKind::Processor,
        _ => return None,
    })
}

fn body_lines(body: &str) -> Vec<String> {
    body.split(['\n', ',']).map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

pub(super) fn parse_event_model(source: &str) -> EventModel {
    let mut m = EventModel::default();
    let lines: Vec<&str> = source.lines().skip(1).collect();
    let mut i = 0;
    // Joins a `{ … }` body that may continue over several lines.
    let take_body = |first: &str, i: &mut usize| -> String {
        let mut body = first.to_string();
        while !body.contains('}') && *i + 1 < lines.len() {
            *i += 1;
            body.push('\n');
            body.push_str(lines[*i].trim());
        }
        let open = body.find('{').map_or(0, |o| o + 1);
        let close = body.rfind('}').unwrap_or(body.len()).max(open);
        body[open..close].to_string()
    };
    while i < lines.len() {
        let l = lines[i].split_once("%%").map_or(lines[i], |(c, _)| c).trim();
        let words: Vec<&str> = l.split_whitespace().collect();
        match words.first().copied() {
            Some("data") if words.len() >= 2 => {
                let name = words[1].trim_end_matches('{').to_string();
                let rest = l.find('{').map_or("{", |o| &l[o..]);
                let body = take_body(rest, &mut i);
                m.data.push((name, body_lines(&body)));
            }
            Some(w @ ("tf" | "timeframe" | "rf" | "resetframe")) if words.len() >= 4 => {
                if let Some(kind) = em_kind(words[2]) {
                    let name = words[3].to_string();
                    // Everything after the name.
                    let after = l.splitn(5, char::is_whitespace).nth(4).unwrap_or("").trim();
                    let mut inline = Vec::new();
                    if after.contains('{') {
                        inline = body_lines(&take_body(after, &mut i));
                    }
                    let data_ref = after.find("[[").and_then(|o| after[o + 2..].find("]]").map(|c| after[o + 2..o + 2 + c].trim().to_string()));
                    let sources = after.split("->>").skip(1).filter_map(|s| s.split_whitespace().next()).map(str::to_string).collect();
                    m.steps.push(EmStep { frame: words[1].to_string(), reset: matches!(w, "rf" | "resetframe"), kind, name, inline, data_ref, sources });
                }
            }
            _ => {}
        }
        i += 1;
    }
    m
}

/// The lane of a step: (sort key, title).
fn lane_of(step: &EmStep) -> (usize, String) {
    match step.kind {
        EmKind::Ui | EmKind::Processor => (0, "UI/Automation".into()),
        EmKind::Command | EmKind::ReadModel => (1, "Command/Read Model".into()),
        EmKind::Event => match step.name.split_once('.') {
            Some((stream, _)) => (3, format!("Stream: {stream}")),
            None => (2, "Events".into()),
        },
    }
}

fn colors(kind: EmKind) -> (&'static str, &'static str) {
    match kind {
        EmKind::Ui => ("#ffffff", "#cbd5e1"),
        EmKind::Command => ("#bfdbfe", "#93c5fd"),
        EmKind::Event => ("#fdba74", "#fb923c"),
        EmKind::ReadModel => ("#d9f99d", "#a3e635"),
        EmKind::Processor => ("#f0abfc", "#e879f9"),
    }
}

pub(super) fn render_eventmodeling_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let m = parse_event_model(source);
    if m.steps.is_empty() {
        return None;
    }
    // Lanes in order: UI, command / read model, events, then streams as they appear.
    let mut lanes: Vec<(usize, String)> = Vec::new();
    for s in &m.steps {
        let lane = lane_of(s);
        if !lanes.contains(&lane) {
            lanes.push(lane);
        }
    }
    lanes.sort_by_key(|(k, _)| *k);
    let lane_index = |s: &EmStep| lanes.iter().position(|l| *l == lane_of(s)).unwrap_or(0);
    let shown = |s: &EmStep| s.name.split_once('.').filter(|_| s.kind == EmKind::Event).map_or(s.name.clone(), |(_, n)| n.to_string());
    let payload = |s: &EmStep| -> Vec<String> {
        let mut rows = s.inline.clone();
        if let Some(r) = &s.data_ref
            && let Some((_, d)) = m.data.iter().find(|(n, _)| n == r)
        {
            rows.extend(d.iter().cloned());
        }
        rows
    };
    let mono = 11.0f32;
    let sizes: Vec<(f32, f32)> = m
        .steps
        .iter()
        .map(|s| {
            let rows = payload(s);
            let tw = estimate_text_width(&shown(s), 12.0) * 1.08 + 28.0;
            let dw = rows.iter().map(|r| r.chars().count() as f32 * mono * 0.62 + 20.0).fold(0.0f32, f32::max);
            (tw.max(dw).max(96.0), 46.0 + rows.len() as f32 * 15.0)
        })
        .collect();
    let label_w = lanes.iter().map(|(_, t)| estimate_text_width(t, 12.0)).fold(0.0f32, f32::max) + 32.0;
    let margin = 12.0;
    let mut lane_h = vec![0.0f32; lanes.len()];
    for (s, sz) in m.steps.iter().zip(&sizes) {
        let k = lane_index(s);
        lane_h[k] = lane_h[k].max(sz.1 + 28.0);
    }
    for h in lane_h.iter_mut() {
        *h = h.max(64.0);
    }
    let mut lane_y = vec![0.0f32; lanes.len()];
    let mut y = margin;
    for k in 0..lanes.len() {
        lane_y[k] = y;
        y += lane_h[k] + 6.0;
    }
    let height = y - 6.0 + margin;
    // Columns: a step overlaps the previous one horizontally when it sits in another lane.
    let mut xs = Vec::with_capacity(m.steps.len());
    let mut lane_right = vec![f32::MIN; lanes.len()];
    let mut prev_center = margin + label_w;
    for (i, s) in m.steps.iter().enumerate() {
        let k = lane_index(s);
        let w = sizes[i].0;
        let left = if i == 0 { margin + label_w + 16.0 } else { (prev_center + 24.0).max(lane_right[k] + 20.0) };
        xs.push(left);
        lane_right[k] = left + w;
        prev_center = left + w / 2.0;
    }
    let width = lane_right.iter().copied().fold(0.0f32, f32::max) + 24.0 + margin;
    let centers: Vec<(f32, f32)> = m.steps.iter().enumerate().map(|(i, s)| (xs[i] + sizes[i].0 / 2.0, lane_y[lane_index(s)] + lane_h[lane_index(s)] / 2.0)).collect();
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (i, s) in m.steps.iter().enumerate() {
        if i > 0 && !s.reset {
            edges.push((i - 1, i));
        }
        for src in &s.sources {
            if let Some(j) = m.steps.iter().position(|t| &t.frame == src)
                && j != i
                && !edges.contains(&(j, i))
            {
                edges.push((j, i));
            }
        }
    }

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0}" height="{height:.0}" viewBox="0 0 {width:.0} {height:.0}" font-family="sans-serif"><rect width="{width:.0}" height="{height:.0}" fill="#ffffff"/>"##
    ));
    svg.push_str(r##"<defs><marker id="em-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerUnits="userSpaceOnUse" markerWidth="9" markerHeight="9" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" fill="#64748b"/></marker></defs>"##);
    for (k, (_, title)) in lanes.iter().enumerate() {
        svg.push_str(&format!(
            r##"<rect x="{margin}" y="{:.1}" width="{:.1}" height="{:.1}" fill="#f8fafc" stroke="#e2e8f0" stroke-width="1" data-lane="{}"/><text x="{:.1}" y="{:.1}" font-size="12" font-weight="600" fill="#334155">{}</text>"##,
            lane_y[k],
            width - 2.0 * margin,
            lane_h[k],
            escape_attr(title),
            margin + 12.0,
            lane_y[k] + 22.0,
            escape_xml(title)
        ));
    }
    for &(a, b) in &edges {
        let (ax, ay) = centers[a];
        let (bx, by) = centers[b];
        let s = edge_anchor(ax, ay, sizes[a].0, sizes[a].1, bx, by);
        let e = edge_anchor(bx, by, sizes[b].0, sizes[b].1, ax, ay);
        svg.push_str(&format!(
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#64748b" stroke-width="1.2" marker-end="url(#em-arrow)" data-edge="{}-{}"/>"##,
            s.0, s.1, e.0, e.1, a, b
        ));
    }
    for (i, s) in m.steps.iter().enumerate() {
        let (w, h) = sizes[i];
        let (x, y) = (xs[i], centers[i].1 - h / 2.0);
        let (fill, stroke) = colors(s.kind);
        let kind = format!("{:?}", s.kind).to_lowercase();
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="3" fill="{fill}" stroke="{stroke}" stroke-width="1.2" data-step="{kind}"/><text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="12" font-weight="bold" fill="#0f172a">{}</text>"##,
            x + w / 2.0,
            y + 27.0,
            escape_xml(&shown(s))
        ));
        for (r, row) in payload(s).iter().enumerate() {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" font-size="{mono}" font-family="monospace" fill="#1e293b">{}</text>"##,
                x + 10.0,
                y + 48.0 + r as f32 * 15.0,
                escape_xml(row)
            ));
        }
    }
    svg.push_str("</svg>");
    Some((svg.into_bytes(), width.ceil() as u32, height.ceil() as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn event_model_parses_steps_data_and_sources() {
        let m = parse_event_model("eventmodeling\n\ntf 01 ui CartUI\ntimeframe 02 command AddItem { description: string }\ntf 03 evt ItemAdded [[Added]]\nrf 04 rmo Items ->> 03 ->> 01\n\ndata Added {\n  price: 20.4\n  image: 'x'\n}");
        assert_eq!(m.steps.len(), 4);
        assert_eq!((m.steps[1].kind, m.steps[1].inline.clone()), (EmKind::Command, vec!["description: string".to_string()]));
        assert_eq!(m.steps[2].data_ref.as_deref(), Some("Added"));
        assert_eq!((m.steps[3].reset, m.steps[3].sources.clone()), (true, vec!["03".to_string(), "01".to_string()]));
        assert_eq!(m.data, vec![("Added".to_string(), vec!["price: 20.4".to_string(), "image: 'x'".to_string()])]);
    }

    #[test]
    fn event_model_svg_lanes_streams_and_links() {
        let svg = svg_of("eventmodeling\ntf 01 ui CartUI\ntf 02 cmd AddItem\ntf 03 evt ItemAdded\nrf 04 evt External.Changed\ntf 05 pcr Proc");
        assert_eq!(svg.matches("data-lane=").count(), 4);
        assert!(svg.contains(">Stream: External<") && svg.contains(">Changed<"));
        assert!(text_y(&svg, "CartUI") < text_y(&svg, "AddItem") && text_y(&svg, "AddItem") < text_y(&svg, "ItemAdded"));
        assert!(text_x(&svg, "CartUI") < text_x(&svg, "AddItem"));
        // 0→1, 1→2, (reset) 3→4.
        assert!(svg.contains(r#"data-edge="0-1""#) && svg.contains(r#"data-edge="3-4""#) && !svg.contains(r#"data-edge="2-3""#));
    }
}
