// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `wardley-beta`: Wardley maps. Components and anchors at `[visibility, evolution]`,
//! dependency and flow links, evolution moves, pipelines, sourcing methods, inertia,
//! notes, numbered annotations, (de)accelerators and custom evolution stages.

use super::*;

const LEFT: f32 = 44.0;
const RIGHT: f32 = 24.0;
const TOP: f32 = 20.0;
const BOTTOM: f32 = 52.0;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct WComponent {
    pub(super) name: String,
    pub(super) vis: f32,
    pub(super) evo: f32,
    pub(super) anchor: bool,
    pub(super) label_offset: Option<(f32, f32)>,
    pub(super) inertia: bool,
    /// `build`, `buy`, `outsource`, `market`.
    pub(super) method: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WLinkKind {
    Dependency,
    /// `+>` (forward), `+<` (back), `+<>` (both).
    Flow { forward: bool, back: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct WLink {
    pub(super) from: String,
    pub(super) to: String,
    pub(super) kind: WLinkKind,
    pub(super) label: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct WAnnotation {
    pub(super) number: String,
    pub(super) at: Vec<(f32, f32)>,
    pub(super) text: String,
}

#[derive(Debug, Default)]
pub(super) struct Wardley {
    pub(super) title: Option<String>,
    pub(super) size: Option<(f32, f32)>,
    /// (label, where the stage ends on the evolution axis).
    pub(super) stages: Vec<(String, f32)>,
    pub(super) components: Vec<WComponent>,
    pub(super) links: Vec<WLink>,
    pub(super) evolves: Vec<(String, f32)>,
    pub(super) pipelines: Vec<(String, Vec<(String, f32)>)>,
    pub(super) notes: Vec<(String, f32, f32)>,
    pub(super) annotations_at: Option<(f32, f32)>,
    pub(super) annotations: Vec<WAnnotation>,
    /// (name, visibility, evolution, accelerator?).
    pub(super) forces: Vec<(String, f32, f32, bool)>,
}

/// `[0.79, 0.61]` → numbers.
fn coords(s: &str) -> Vec<f32> {
    s.trim().trim_start_matches('[').trim_end_matches(']').split(',').filter_map(|v| v.trim().parse::<f32>().ok()).collect()
}

fn pair(s: &str) -> Option<(f32, f32)> {
    match coords(s)[..] {
        [a, b] => Some((a, b)),
        _ => None,
    }
}

/// `Cup of Tea [0.79, 0.61] label [-50, 10] (build) (inertia)`.
pub(super) fn parse_component(rest: &str, anchor: bool) -> Option<WComponent> {
    let open = rest.find('[')?;
    let close = open + rest[open..].find(']')?;
    let name = unquote(rest[..open].trim());
    let (vis, evo) = pair(&rest[open..=close])?;
    let mut tail = &rest[close + 1..];
    let mut label_offset = None;
    if let Some(at) = tail.find("label")
        && let Some(o) = tail[at..].find('[')
        && let Some(c) = tail[at + o..].find(']')
    {
        label_offset = pair(&tail[at + o..=at + o + c]);
        tail = &tail[at + o + c + 1..];
    }
    let tags: Vec<&str> = tail.split(['(', ')']).map(str::trim).filter(|t| !t.is_empty()).collect();
    let method = tags.iter().find(|t| matches!(**t, "build" | "buy" | "outsource" | "market")).map(|t| t.to_string());
    Some(WComponent { name, vis, evo, anchor, label_offset, inertia: tags.contains(&"inertia"), method })
}

/// `A -> B`, `A +> B`, `A +< B`, `A +<> B`, `A +'label'> B`.
pub(super) fn parse_wardley_link(l: &str) -> Option<WLink> {
    let (lhs, op, rhs, label) = if let Some(at) = l.find("+'") {
        let close = at + 2 + l[at + 2..].find('\'')?;
        let label = l[at + 2..close].to_string();
        let after = &l[close + 1..];
        let op_len = after.chars().take_while(|c| matches!(c, '<' | '>')).count();
        (&l[..at], format!("+{}", &after[..op_len]), &after[op_len..], Some(label))
    } else {
        let (at, op) = ["+<>", "+>", "+<", "->"].iter().find_map(|op| l.find(op).map(|at| (at, *op)))?;
        (&l[..at], op.to_string(), &l[at + op.len()..], None)
    };
    let (from, to) = (unquote(lhs.trim()), unquote(rhs.trim()));
    if from.is_empty() || to.is_empty() {
        return None;
    }
    let kind = match op.as_str() {
        "->" => WLinkKind::Dependency,
        "+<>" => WLinkKind::Flow { forward: true, back: true },
        "+<" => WLinkKind::Flow { forward: false, back: true },
        _ => WLinkKind::Flow { forward: true, back: false },
    };
    Some(WLink { from, to, kind, label })
}

pub(super) fn default_stages() -> Vec<(String, f32)> {
    vec![("Genesis".into(), 0.25), ("Custom-Built".into(), 0.5), ("Product (+rental)".into(), 0.75), ("Commodity (+utility)".into(), 1.0)]
}

pub(super) fn parse_wardley(source: &str) -> Wardley {
    let mut w = Wardley::default();
    let mut lines = source.lines().skip(1);
    while let Some(raw) = lines.next() {
        let l = raw.split_once("%%").map_or(raw, |(c, _)| c).trim();
        if l.is_empty() {
            continue;
        }
        let (word, rest) = l.split_once(char::is_whitespace).unwrap_or((l, ""));
        let rest = rest.trim();
        match word {
            "title" => w.title = Some(unquote(rest)),
            "size" => w.size = pair(rest).filter(|(a, b)| *a >= 100.0 && *b >= 100.0).map(|(a, b)| (a.min(4000.0), b.min(4000.0))),
            "evolution" => {
                let parts: Vec<&str> = rest.split("->").map(str::trim).collect();
                let n = parts.len() as f32;
                w.stages = parts
                    .iter()
                    .enumerate()
                    .map(|(i, p)| match p.rsplit_once('@') {
                        Some((label, end)) => (label.trim().to_string(), end.trim().parse::<f32>().unwrap_or((i + 1) as f32 / n)),
                        None => (p.to_string(), (i + 1) as f32 / n),
                    })
                    .collect();
            }
            "component" | "anchor" => {
                if let Some(c) = parse_component(rest, word == "anchor") {
                    w.components.push(c);
                }
            }
            "evolve" => {
                if let Some((name, to)) = rest.rsplit_once(char::is_whitespace)
                    && let Ok(to) = to.parse::<f32>()
                {
                    w.evolves.push((unquote(name.trim()), to));
                }
            }
            "pipeline" => {
                let parent = unquote(rest.trim_end_matches('{').trim());
                let mut items = Vec::new();
                for more in lines.by_ref() {
                    let m = more.trim();
                    if m.starts_with('}') {
                        break;
                    }
                    if let Some(c) = m.strip_prefix("component ")
                        && let Some(open) = c.find('[')
                        && let [evo] = coords(&c[open..])[..]
                    {
                        items.push((unquote(c[..open].trim()), evo));
                    }
                }
                w.pipelines.push((parent, items));
            }
            "note" => {
                if let Some(open) = rest.rfind('[')
                    && let Some((v, e)) = pair(&rest[open..])
                {
                    w.notes.push((unquote(rest[..open].trim()), v, e));
                }
            }
            "annotations" => w.annotations_at = pair(rest),
            "annotation" => {
                let Some((number, tail)) = rest.split_once(',') else { continue };
                let Some(q) = tail.find('"') else { continue };
                let points = &tail[..q];
                // `[0.6, 0.65]` or `[[0.6, 0.65], [0.5, 0.4]]`.
                let nums = coords(&points.replace(['[', ']'], ""));
                let at: Vec<(f32, f32)> = nums.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0], c[1])).collect();
                w.annotations.push(WAnnotation { number: number.trim().to_string(), at, text: unquote(tail[q..].trim()) });
            }
            "accelerator" | "deaccelerator" => {
                if let Some(open) = rest.rfind('[')
                    && let Some((v, e)) = pair(&rest[open..])
                {
                    w.forces.push((unquote(rest[..open].trim()), v, e, word == "accelerator"));
                }
            }
            _ => {
                if let Some(link) = parse_wardley_link(l) {
                    w.links.push(link);
                }
            }
        }
    }
    if w.stages.is_empty() {
        w.stages = default_stages();
    }
    w
}

fn method_fill(method: &str) -> &'static str {
    match method {
        "build" => "#d6d6d6",
        "buy" => "#aaa5a9",
        "outsource" => "#444444",
        _ => "#ffffff",
    }
}

pub(super) fn render_wardley_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let w = parse_wardley(source);
    if w.title.is_none() && w.components.is_empty() && w.size.is_none() {
        return None;
    }
    let (width, height) = w.size.unwrap_or((800.0, 560.0));
    let (pw, ph) = (width - LEFT - RIGHT, height - TOP - BOTTOM);
    let pos = |v: f32, e: f32| (LEFT + e.clamp(0.0, 1.0) * pw, TOP + (1.0 - v.clamp(0.0, 1.0)) * ph);
    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{width}" height="{height}" fill="#ffffff"/>"##));
    svg.push_str(concat!(
        r##"<defs><marker id="wm-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerUnits="userSpaceOnUse" markerWidth="11" markerHeight="11" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#334155"/></marker>"##,
        r##"<marker id="wm-flow" viewBox="0 0 10 10" refX="9" refY="5" markerUnits="userSpaceOnUse" markerWidth="11" markerHeight="11" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#b91c1c"/></marker>"##,
        r##"<marker id="wm-evolve" viewBox="0 0 10 10" refX="9" refY="5" markerUnits="userSpaceOnUse" markerWidth="11" markerHeight="11" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#dc2626"/></marker></defs>"##
    ));
    // Stages: dashed separators, labels under the evolution axis.
    let base = TOP + ph;
    let mut prev = 0.0f32;
    for (k, (label, end)) in w.stages.iter().enumerate() {
        let end = end.clamp(prev, 1.0);
        let x = LEFT + end * pw;
        if k + 1 < w.stages.len() {
            svg.push_str(&format!(
                r##"<line x1="{x:.1}" y1="{TOP}" x2="{x:.1}" y2="{base:.1}" stroke="#94a3b8" stroke-width="1" stroke-dasharray="5,5" data-role="stage"/>"##
            ));
        }
        let mid = LEFT + (prev + end) / 2.0 * pw;
        for (i, part) in label.split(" / ").enumerate() {
            svg.push_str(&format!(
                r##"<text x="{mid:.1}" y="{:.1}" text-anchor="middle" font-size="12" fill="#334155">{}</text>"##,
                base + 16.0 + i as f32 * 14.0,
                escape_xml(part.trim())
            ));
        }
        prev = end;
    }
    svg.push_str(&format!(
        r##"<path d="M {LEFT} {base:.1} H {:.1} M {LEFT} {base:.1} V {TOP}" fill="none" stroke="#334155" stroke-width="1.5"/>"##,
        LEFT + pw
    ));
    svg.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="12" font-weight="bold" fill="#334155">Evolution</text>"##,
        LEFT + pw,
        height - 6.0
    ));
    svg.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="12" font-weight="bold" fill="#334155" transform="rotate(-90 {:.1} {:.1})">Visibility</text><text x="{:.1}" y="{:.1}" text-anchor="start" font-size="10" fill="#64748b" transform="rotate(-90 {:.1} {:.1})">Invisible</text><text x="{:.1}" y="{:.1}" text-anchor="end" font-size="10" fill="#64748b" transform="rotate(-90 {:.1} {:.1})">Visible</text>"##,
        LEFT - 16.0,
        TOP + ph / 2.0,
        LEFT - 16.0,
        TOP + ph / 2.0,
        LEFT - 16.0,
        base,
        LEFT - 16.0,
        base,
        LEFT - 16.0,
        TOP,
        LEFT - 16.0,
        TOP
    ));
    let find = |name: &str| w.components.iter().find(|c| c.name == name);
    let shorten = |a: (f32, f32), b: (f32, f32), ra: f32, rb: f32| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt().max(1e-3);
        ((a.0 + dx / len * ra, a.1 + dy / len * ra), (b.0 - dx / len * rb, b.1 - dy / len * rb))
    };
    let radius = |c: &WComponent| if c.anchor { 2.0 } else if c.method.is_some() { 14.0 } else { 6.0 };
    // Links.
    for link in &w.links {
        let (Some(a), Some(b)) = (find(&link.from), find(&link.to)) else { continue };
        let (p, q) = shorten(pos(a.vis, a.evo), pos(b.vis, b.evo), radius(a) + 2.0, radius(b) + 2.0);
        let data = format!(r#" data-link="{}-{}""#, escape_attr(&link.from), escape_attr(&link.to));
        match link.kind {
            WLinkKind::Dependency => svg.push_str(&format!(
                r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#334155" stroke-width="1.2"{data}/>"##,
                p.0, p.1, q.0, q.1
            )),
            WLinkKind::Flow { forward, back } => {
                let start = if back { r#" marker-start="url(#wm-flow)""# } else { "" };
                let end = if forward { r#" marker-end="url(#wm-flow)""# } else { "" };
                svg.push_str(&format!(
                    r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#b91c1c" stroke-width="2.2" stroke-dasharray="7,3"{start}{end}{data}/>"##,
                    p.0, p.1, q.0, q.1
                ));
            }
        }
        if let Some(label) = &link.label {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" font-style="italic" fill="#b91c1c">{}</text>"##,
                (p.0 + q.0) / 2.0 + 6.0,
                (p.1 + q.1) / 2.0 - 5.0,
                escape_xml(label)
            ));
        }
    }
    // Evolution moves: a dashed red arrow to a ghost of the component.
    for (name, to) in &w.evolves {
        let Some(c) = find(name) else { continue };
        let from = pos(c.vis, c.evo);
        let ghost = pos(c.vis, *to);
        let (p, q) = shorten(from, ghost, radius(c) + 2.0, 8.0);
        svg.push_str(&format!(
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#dc2626" stroke-width="1.4" stroke-dasharray="5,4" marker-end="url(#wm-evolve)" data-evolve="{}"/>"##,
            p.0,
            p.1,
            q.0,
            q.1,
            escape_attr(name)
        ));
        svg.push_str(&format!(
            r##"<circle cx="{:.1}" cy="{:.1}" r="6" fill="#ffffff" stroke="#dc2626" stroke-width="1.5"/><text x="{:.1}" y="{:.1}" font-size="11" fill="#dc2626">{}</text>"##,
            ghost.0,
            ghost.1,
            ghost.0 + 10.0,
            ghost.1 - 8.0,
            escape_xml(name)
        ));
    }
    // Pipelines: a box under the parent, its components on it.
    for (parent, items) in &w.pipelines {
        let Some(p) = find(parent) else { continue };
        if items.is_empty() {
            continue;
        }
        let lo = items.iter().map(|i| i.1).fold(f32::MAX, f32::min);
        let hi = items.iter().map(|i| i.1).fold(f32::MIN, f32::max);
        let (_, py) = pos(p.vis, p.evo);
        let y = py + 22.0;
        let (x0, _) = pos(0.0, lo);
        let (x1, _) = pos(0.0, hi);
        svg.push_str(&format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="22" fill="#ffffff" stroke="#334155" stroke-width="1.2" data-pipeline="{}"/>"##,
            x0 - 14.0,
            y - 11.0,
            x1 - x0 + 28.0,
            escape_attr(parent)
        ));
        for (name, evo) in items {
            let (x, _) = pos(0.0, *evo);
            svg.push_str(&format!(
                r##"<circle cx="{x:.1}" cy="{y:.1}" r="5" fill="#ffffff" stroke="#1e293b" stroke-width="1.5"/><text x="{x:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#0f172a">{}</text>"##,
                y + 24.0,
                escape_xml(name)
            ));
        }
    }
    // Components and anchors.
    for c in &w.components {
        let (x, y) = pos(c.vis, c.evo);
        let name = escape_xml(&c.name);
        if c.anchor {
            svg.push_str(&format!(
                r##"<text x="{x:.1}" y="{:.1}" text-anchor="middle" font-size="13" font-weight="bold" fill="#0f172a" data-anchor="{}">{name}</text>"##,
                y + 4.0,
                escape_attr(&c.name)
            ));
            continue;
        }
        if let Some(m) = &c.method {
            if m == "market" {
                svg.push_str(&format!(r##"<circle cx="{x:.1}" cy="{y:.1}" r="14" fill="#ffffff" stroke="#1e293b" stroke-width="1.5" data-method="market"/>"##));
                for k in 0..3 {
                    let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::TAU / 3.0;
                    svg.push_str(&format!(r##"<circle cx="{:.1}" cy="{:.1}" r="2.5" fill="#1e293b"/>"##, x + a.cos() * 7.0, y + a.sin() * 7.0));
                }
            } else {
                svg.push_str(&format!(
                    r##"<circle cx="{x:.1}" cy="{y:.1}" r="14" fill="{}" fill-opacity="0.7" stroke="#64748b" stroke-width="1" data-method="{}"/>"##,
                    method_fill(m),
                    escape_attr(m)
                ));
            }
        }
        if c.method.as_deref() != Some("market") {
            svg.push_str(&format!(
                r##"<circle cx="{x:.1}" cy="{y:.1}" r="6" fill="#ffffff" stroke="#1e293b" stroke-width="1.5" data-component="{}"/>"##,
                escape_attr(&c.name)
            ));
        }
        if c.inertia {
            let bx = x + radius(c) + 8.0;
            svg.push_str(&format!(
                r##"<rect x="{bx:.1}" y="{:.1}" width="6" height="26" fill="#1e293b" data-role="inertia"/>"##,
                y - 13.0
            ));
        }
        let (dx, dy) = c.label_offset.unwrap_or((radius(c) + 5.0, -radius(c) - 3.0));
        // Keep default labels inside the canvas: flip them to the left near the right edge.
        let flip = c.label_offset.is_none() && x + dx + estimate_text_width(&c.name, 12.0) > width - 4.0;
        let (tx, anchor) = if flip { (x - dx, "end") } else { (x + dx, "start") };
        svg.push_str(&format!(
            r##"<text x="{tx:.1}" y="{:.1}" text-anchor="{anchor}" font-size="12" fill="#0f172a">{name}</text>"##,
            y + dy
        ));
    }
    for (text, v, e) in &w.notes {
        let (x, y) = pos(*v, *e);
        let x = x.min(width - 4.0 - estimate_text_width(text, 12.0) * 1.08).max(4.0);
        svg.push_str(&format!(
            r##"<text x="{x:.1}" y="{y:.1}" font-size="12" font-weight="bold" fill="#334155" data-role="note">{}</text>"##,
            escape_xml(text)
        ));
    }
    for (name, v, e, accel) in &w.forces {
        let (x, y) = pos(*v, *e);
        let (len, half) = (64.0, 12.0);
        let pts = if *accel {
            [(x, y - 6.0), (x + len - 18.0, y - 6.0), (x + len - 18.0, y - half), (x + len, y), (x + len - 18.0, y + half), (x + len - 18.0, y + 6.0), (x, y + 6.0)]
        } else {
            [(x + len, y - 6.0), (x + 18.0, y - 6.0), (x + 18.0, y - half), (x, y), (x + 18.0, y + half), (x + 18.0, y + 6.0), (x + len, y + 6.0)]
        };
        let body: Vec<String> = pts.iter().map(|(a, b)| format!("{a:.1},{b:.1}")).collect();
        let kind = if *accel { "accelerator" } else { "deaccelerator" };
        svg.push_str(&format!(
            r##"<polygon points="{}" fill="#ffffff" stroke="#334155" stroke-width="1.4" data-role="{kind}"/><text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#334155">{}</text>"##,
            body.join(" "),
            x + len / 2.0,
            y + 26.0,
            escape_xml(name)
        ));
    }
    for a in &w.annotations {
        for (v, e) in &a.at {
            let (x, y) = pos(*v, *e);
            svg.push_str(&format!(
                r##"<circle cx="{x:.1}" cy="{y:.1}" r="9" fill="#ffffff" stroke="#334155" stroke-width="1.2" data-annotation="{}"/><text x="{x:.1}" y="{:.1}" text-anchor="middle" font-size="10" font-weight="bold" fill="#334155">{}</text>"##,
                escape_attr(&a.number),
                y + 3.5,
                escape_xml(&a.number)
            ));
        }
    }
    if !w.annotations.is_empty() {
        let (x, y) = w.annotations_at.map_or((LEFT + 10.0, TOP + 10.0), |(v, e)| pos(v, e));
        let lines: Vec<String> = w.annotations.iter().map(|a| format!("{}. {}", a.number, a.text)).collect();
        let bw = lines.iter().map(|l| estimate_text_width(l, 11.0)).fold(estimate_text_width("Annotations", 11.0), f32::max) + 16.0;
        let bh = 22.0 + lines.len() as f32 * 15.0;
        let x = x.min(width - RIGHT - bw);
        let y = y.min(TOP + ph - bh);
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{bw:.1}" height="{bh:.1}" fill="#ffffff" stroke="#94a3b8" stroke-width="1" data-role="annotations"/><text x="{:.1}" y="{:.1}" font-size="11" font-weight="bold" fill="#334155">Annotations</text>"##,
            x + 8.0,
            y + 15.0
        ));
        for (k, l) in lines.iter().enumerate() {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" font-size="11" fill="#334155">{}</text>"##,
                x + 8.0,
                y + 31.0 + k as f32 * 15.0,
                escape_xml(l)
            ));
        }
    }
    svg.push_str("</svg>");
    let bytes = svg.into_bytes();
    Some(match &w.title {
        Some(title) => add_svg_title(bytes, width as u32, height as u32, title),
        None => (bytes, width as u32, height as u32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn wardley_components_tags_and_labels() {
        let c = parse_component("\"Web App\" [0.75, 0.80] label [-60, 10] (build) (inertia)", false).unwrap();
        assert_eq!(c, WComponent { name: "Web App".into(), vis: 0.75, evo: 0.8, anchor: false, label_offset: Some((-60.0, 10.0)), inertia: true, method: Some("build".into()) });
        let c = parse_component("Cup of Tea [0.79, 0.61]", false).unwrap();
        assert_eq!((c.name.as_str(), c.label_offset, c.inertia, c.method), ("Cup of Tea", None, false, None));
    }

    #[test]
    fn wardley_links_stages_and_extras_parse() {
        assert_eq!(parse_wardley_link("end-user -> real-time processing").unwrap().to, "real-time processing");
        assert_eq!(parse_wardley_link("API +<> Cache").unwrap().kind, WLinkKind::Flow { forward: true, back: true });
        let l = parse_wardley_link("Cache +'backup'> Database").unwrap();
        assert_eq!((l.kind, l.label.as_deref(), l.to.as_str()), (WLinkKind::Flow { forward: true, back: false }, Some("backup"), "Database"));
        assert_eq!(parse_wardley_link("\"Mobile App\" -> \"API Gateway\"").unwrap().from, "Mobile App");
        let w = parse_wardley(
            "wardley-beta\ntitle T\nsize [1100, 800]\nevolution Genesis@0.2 -> Custom@0.4 -> Product -> Commodity@1.0\ncomponent Database [0.40, 0.60]\npipeline Database {\n  component \"SQL DB\" [0.50]\n}\nevolve Database 0.75\nnote \"Risk\" [0.40, 0.35]\nannotations [0.1, 0.9]\nannotation 1,[0.60, 0.65] \"Critical\"\naccelerator \"AI\" [0.55, 0.25]\ndeaccelerator \"Legacy\" [0.15, 0.75]",
        );
        assert_eq!(w.size, Some((1100.0, 800.0)));
        assert_eq!(w.stages, vec![("Genesis".into(), 0.2), ("Custom".into(), 0.4), ("Product".into(), 0.75), ("Commodity".into(), 1.0)]);
        assert_eq!(w.pipelines, vec![("Database".into(), vec![("SQL DB".into(), 0.5)])]);
        assert_eq!(w.evolves, vec![("Database".into(), 0.75)]);
        assert_eq!(w.notes, vec![("Risk".into(), 0.4, 0.35)]);
        assert_eq!(w.annotations, vec![WAnnotation { number: "1".into(), at: vec![(0.6, 0.65)], text: "Critical".into() }]);
        assert_eq!(w.forces, vec![("AI".into(), 0.55, 0.25, true), ("Legacy".into(), 0.15, 0.75, false)]);
    }

    #[test]
    fn wardley_svg_positions_and_marks() {
        let svg = svg_of(
            "wardley-beta\nanchor Business [0.95, 0.63]\ncomponent Kettle [0.43, 0.35] (inertia)\ncomponent Power [0.10, 0.70] (buy)\nBusiness -> Kettle\nKettle +> Power\nevolve Kettle 0.62",
        );
        let k = tag_with(&svg, r#"data-component="Kettle""#)[0];
        // 800 x 560: plot x from 44 to 776, y from 20 to 508.
        assert!((attr_f32(k, "cx").unwrap() - (44.0 + 0.35 * 732.0)).abs() < 0.2);
        assert!((attr_f32(k, "cy").unwrap() - (20.0 + 0.57 * 488.0)).abs() < 0.2);
        assert!(svg.contains(r#"data-anchor="Business""#) && svg.contains(r#"data-role="inertia""#) && svg.contains(r#"data-method="buy""#));
        assert!(svg.contains(r#"marker-end="url(#wm-flow)" data-link="Kettle-Power""#));
        assert!(svg.contains(r#"data-evolve="Kettle""#));
        assert_eq!(svg.matches(r#"data-role="stage""#).count(), 3);
        assert!(svg.contains(">Commodity (+utility)<"));
    }
}
