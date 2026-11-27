// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! C4 diagrams (`C4Context`, `C4Container`, `C4Component`, `C4Dynamic`, `C4Deployment`):
//! people, systems, containers and components laid out in rows inside nested boundaries,
//! like mermaid's own C4 layout (`c4ShapeInRow` shapes, then `c4BoundaryInRow` boundaries).

use super::*;

const SHAPE_W: f32 = 216.0;
const SHAPE_GAP: f32 = 50.0;
const BOUNDARY_GAP: f32 = 30.0;
const PAD: f32 = 20.0;
const HEADER: f32 = 44.0;
const MARGIN: f32 = 20.0;
const TEXT_W: f32 = 196.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum C4Kind {
    Person,
    System,
    Container,
    Component,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum C4Form {
    Box,
    Db,
    Queue,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct C4Element {
    pub(super) alias: String,
    pub(super) kind: C4Kind,
    pub(super) form: C4Form,
    pub(super) external: bool,
    pub(super) label: String,
    pub(super) techn: Option<String>,
    pub(super) descr: Option<String>,
    pub(super) bg: Option<String>,
    pub(super) font: Option<String>,
    pub(super) border: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct C4Boundary {
    pub(super) alias: String,
    pub(super) label: String,
    /// `[ENTERPRISE]`, `[SYSTEM]`, the type argument of `Boundary` / `Deployment_Node`.
    pub(super) kind: Option<String>,
    pub(super) descr: Option<String>,
    /// Deployment nodes have a solid frame.
    pub(super) node: bool,
    pub(super) parent: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct C4Rel {
    pub(super) from: String,
    pub(super) to: String,
    pub(super) label: String,
    pub(super) techn: Option<String>,
    pub(super) arrow_start: bool,
    pub(super) arrow_end: bool,
    pub(super) text_color: Option<String>,
    pub(super) line_color: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum C4Item {
    Element(usize),
    Boundary(usize),
}

#[derive(Debug)]
pub(super) struct C4Diagram {
    pub(super) dynamic: bool,
    pub(super) title: Option<String>,
    pub(super) elements: Vec<C4Element>,
    pub(super) boundaries: Vec<C4Boundary>,
    /// Items of each boundary; the last entry is the top level.
    pub(super) children: Vec<Vec<C4Item>>,
    pub(super) rels: Vec<C4Rel>,
    pub(super) shapes_in_row: usize,
    pub(super) boundaries_in_row: usize,
}

/// `Name(a, "b, c", $k="v") {` → (`Name`, positional args, named args, opens a block).
pub(super) fn parse_c4_call(line: &str) -> Option<(String, Vec<String>, Vec<(String, String)>, bool)> {
    let line = line.trim();
    let open = line.find('(')?;
    let name = line[..open].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let mut args: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quote = false;
    let mut close = None;
    for (i, ch) in line[open + 1..].char_indices() {
        match ch {
            '"' => {
                quote = !quote;
                current.push(ch);
            }
            ',' if !quote => args.push(std::mem::take(&mut current)),
            ')' if !quote => {
                close = Some(open + 1 + i);
                break;
            }
            _ => current.push(ch),
        }
    }
    let close = close?;
    if !current.trim().is_empty() || !args.is_empty() {
        args.push(current);
    }
    let block = line[close + 1..].trim_start().starts_with('{');
    let mut positional = Vec::new();
    let mut named = Vec::new();
    for arg in args {
        let arg = arg.trim();
        if let Some(rest) = arg.strip_prefix('$')
            && let Some((k, v)) = rest.split_once('=')
        {
            named.push((k.trim().to_string(), unquote(v.trim())));
        } else {
            positional.push(unquote(arg));
        }
    }
    Some((name.to_string(), positional, named, block))
}

fn element_kind(name: &str) -> Option<(C4Kind, C4Form, bool)> {
    let (base, external) = match name.strip_suffix("_Ext") {
        Some(b) => (b, true),
        None => (name, false),
    };
    let (kind, rest) = [("Person", C4Kind::Person), ("System", C4Kind::System), ("Container", C4Kind::Container), ("Component", C4Kind::Component)]
        .into_iter()
        .find_map(|(p, k)| base.strip_prefix(p).map(|r| (k, r)))?;
    let form = match rest {
        "" => C4Form::Box,
        "Db" if kind != C4Kind::Person => C4Form::Db,
        "Queue" if kind != C4Kind::Person => C4Form::Queue,
        _ => return None,
    };
    Some((kind, form, external))
}

/// Keep colour values to a conservative CSS character set.
fn safe_color(v: &str) -> Option<String> {
    let v = v.trim();
    (!v.is_empty() && v.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '#' | '(' | ')' | ',' | '.' | ' ' | '%'))).then(|| v.to_string())
}

pub(super) fn parse_c4(source: &str) -> C4Diagram {
    let header = source.lines().next().unwrap_or("").trim();
    let mut d = C4Diagram {
        dynamic: header.starts_with("C4Dynamic"),
        title: None,
        elements: Vec::new(),
        boundaries: Vec::new(),
        children: vec![Vec::new()],
        rels: Vec::new(),
        shapes_in_row: 4,
        boundaries_in_row: 2,
    };
    // Indices into `children`: 0 is the top level until boundaries exist; boundaries use b + 1.
    let mut stack: Vec<usize> = vec![0];
    let mut element_styles: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut rel_styles: Vec<(String, String, Vec<(String, String)>)> = Vec::new();
    for raw in source.lines().skip(1) {
        let l = raw.split_once("%%").map_or(raw, |(code, _)| code).trim();
        if l.is_empty() {
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ") {
            d.title = Some(rest.trim().to_string());
            continue;
        }
        if l == "}" {
            if stack.len() > 1 {
                stack.pop();
            }
            continue;
        }
        let Some((name, args, named, block)) = parse_c4_call(l) else { continue };
        let arg = |i: usize| args.get(i).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        let cur = *stack.last().unwrap();
        if let Some((kind, form, external)) = element_kind(&name) {
            let Some(alias) = arg(0) else { continue };
            let (techn, descr) = match kind {
                C4Kind::Container | C4Kind::Component => (arg(2), arg(3)),
                _ => (None, arg(2)),
            };
            let idx = d.elements.len();
            d.elements.push(C4Element { label: arg(1).unwrap_or_else(|| alias.clone()), alias, kind, form, external, techn, descr, bg: None, font: None, border: None });
            d.children[cur].push(C4Item::Element(idx));
            continue;
        }
        let boundary = match name.as_str() {
            "Boundary" => Some((arg(2), false, None)),
            "Enterprise_Boundary" => Some((Some("ENTERPRISE".into()), false, None)),
            "System_Boundary" => Some((Some("SYSTEM".into()), false, None)),
            "Container_Boundary" => Some((Some("CONTAINER".into()), false, None)),
            "Deployment_Node" | "Node" | "Node_L" | "Node_R" => Some((arg(2), true, arg(3))),
            _ => None,
        };
        if let Some((kind, node, descr)) = boundary {
            let Some(alias) = arg(0) else { continue };
            let b = d.boundaries.len();
            d.boundaries.push(C4Boundary {
                label: arg(1).unwrap_or_else(|| alias.clone()),
                alias,
                kind,
                descr,
                node,
                parent: (cur > 0).then(|| cur - 1),
            });
            d.children.push(Vec::new());
            d.children[cur].push(C4Item::Boundary(b));
            if block {
                stack.push(b + 1);
            }
            continue;
        }
        let rel = match name.as_str() {
            "Rel" | "Rel_U" | "Rel_Up" | "Rel_D" | "Rel_Down" | "Rel_L" | "Rel_Left" | "Rel_R" | "Rel_Right" => Some((false, true)),
            "BiRel" | "BiRel_U" | "BiRel_D" | "BiRel_L" | "BiRel_R" => Some((true, true)),
            "Rel_Back" => Some((true, false)),
            "RelIndex" => None,
            _ => None,
        };
        if let Some((arrow_start, arrow_end)) = rel {
            let (Some(from), Some(to)) = (arg(0), arg(1)) else { continue };
            d.rels.push(C4Rel { from, to, label: arg(2).unwrap_or_default(), techn: arg(3), arrow_start, arrow_end, text_color: None, line_color: None });
            continue;
        }
        match name.as_str() {
            "UpdateElementStyle" => {
                if let Some(alias) = arg(0) {
                    element_styles.push((alias, named));
                }
            }
            "UpdateRelStyle" => {
                if let (Some(a), Some(b)) = (arg(0), arg(1)) {
                    rel_styles.push((a, b, named));
                }
            }
            "UpdateLayoutConfig" => {
                for (k, v) in &named {
                    let n = v.trim().parse::<usize>().ok().filter(|&n| n > 0);
                    match (k.as_str(), n) {
                        ("c4ShapeInRow", Some(n)) => d.shapes_in_row = n,
                        ("c4BoundaryInRow", Some(n)) => d.boundaries_in_row = n,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    for (alias, named) in element_styles {
        if let Some(e) = d.elements.iter_mut().find(|e| e.alias == alias) {
            for (k, v) in named {
                match k.as_str() {
                    "bgColor" => e.bg = safe_color(&v),
                    "fontColor" => e.font = safe_color(&v),
                    "borderColor" => e.border = safe_color(&v),
                    _ => {}
                }
            }
        }
    }
    for (a, b, named) in rel_styles {
        for r in d.rels.iter_mut().filter(|r| r.from == a && r.to == b) {
            for (k, v) in &named {
                match k.as_str() {
                    "textColor" => r.text_color = safe_color(v),
                    "lineColor" => r.line_color = safe_color(v),
                    _ => {}
                }
            }
        }
    }
    // The top level is the last entry, after every boundary's own list.
    let top = d.children.remove(0);
    d.children.push(top);
    d
}

/// Fill, border and text colours of mermaid's C4 theme.
fn element_colors(e: &C4Element) -> (&'static str, &'static str, &'static str) {
    match (e.kind, e.external) {
        (C4Kind::Person, false) => ("#08427B", "#073B6F", "#FFFFFF"),
        (C4Kind::Person, true) => ("#686868", "#8A8A8A", "#FFFFFF"),
        (C4Kind::System, false) => ("#1168BD", "#3C7FC0", "#FFFFFF"),
        (C4Kind::System, true) => ("#999999", "#8A8A8A", "#FFFFFF"),
        (C4Kind::Container, false) => ("#438DD5", "#3C7FC0", "#FFFFFF"),
        (C4Kind::Container, true) => ("#B3B3B3", "#A6A6A6", "#FFFFFF"),
        (C4Kind::Component, false) => ("#85BBF0", "#78A8D8", "#000000"),
        (C4Kind::Component, true) => ("#CCCCCC", "#BFBFBF", "#000000"),
    }
}

/// mermaid.js typography: `[Person]`, `[Software System]`, `[Container: Java]`.
/// External elements use the same type text (grey fill marks them as external).
fn stereotype(e: &C4Element) -> String {
    let typ = match e.kind {
        C4Kind::Person => "Person",
        C4Kind::System => "Software System",
        C4Kind::Container => "Container",
        C4Kind::Component => "Component",
    };
    match e.techn.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => format!("[{typ}: {t}]"),
        None => format!("[{typ}]"),
    }
}

/// Text lines of an element: (text, font size, bold, italic).
/// Order matches mermaid.js: name, type (with technology), then description.
fn element_lines(e: &C4Element) -> Vec<(String, f32, bool, bool)> {
    let text_w = if e.form == C4Form::Queue { TEXT_W - 24.0 } else { TEXT_W };
    let mut out = Vec::new();
    for l in label_lines(&e.label, text_w, 15.0) {
        out.push((l, 15.0, true, false));
    }
    for l in label_lines(&stereotype(e), text_w, 11.0) {
        out.push((l, 11.0, false, true));
    }
    if let Some(desc) = &e.descr {
        for l in label_lines(desc, text_w, 12.0) {
            out.push((l, 12.0, false, false));
        }
    }
    out
}

fn line_height(size: f32) -> f32 {
    size * 1.3
}

fn element_size(e: &C4Element) -> (f32, f32) {
    let text: f32 = element_lines(e).iter().map(|l| line_height(l.1)).sum::<f32>() + 6.0;
    let icon = if e.kind == C4Kind::Person { 44.0 } else { 0.0 };
    let cap = if e.form == C4Form::Db { 14.0 } else { 0.0 };
    (SHAPE_W, (text + icon + cap + 28.0).max(100.0))
}

pub(super) struct C4Layout {
    pub(super) elements: Vec<RectF>,
    pub(super) boundaries: Vec<RectF>,
    pub(super) width: f32,
    pub(super) height: f32,
}

/// The title block of a boundary wrapped to `width`: (text, font size, bold).
fn header_lines(d: &C4Diagram, b: usize, width: f32) -> Vec<(String, f32, bool)> {
    let boundary = &d.boundaries[b];
    let width = (width - 20.0).max(100.0);
    let mut out: Vec<(String, f32, bool)> = label_lines(&boundary.label, width, 14.0).into_iter().map(|l| (l, 14.0, true)).collect();
    if let Some(kind) = &boundary.kind {
        let kind = format!("[{}]", break_label(kind).replace('\n', " "));
        out.extend(label_lines(&kind, width, 11.0).into_iter().map(|l| (l, 11.0, false)));
    }
    if let Some(desc) = &boundary.descr {
        out.extend(label_lines(desc, width, 11.0).into_iter().map(|l| (l, 11.0, false)));
    }
    out
}

fn boundary_header(d: &C4Diagram, b: usize, width: f32) -> f32 {
    let text: f32 = header_lines(d, b, width).iter().map(|l| line_height(l.1)).sum();
    (text + 14.0).max(HEADER)
}

/// Size of a list of items: shapes in rows of `shapes_in_row`, then boundaries below in rows of `boundaries_in_row`.
fn measure_items(d: &C4Diagram, list: usize, sizes: &[(f32, f32)], memo: &mut [Option<(f32, f32)>]) -> (f32, f32) {
    let (mut w, mut h) = (0.0f32, 0.0f32);
    let shapes: Vec<usize> = d.children[list].iter().filter_map(|i| if let C4Item::Element(e) = i { Some(*e) } else { None }).collect();
    let bounds: Vec<usize> = d.children[list].iter().filter_map(|i| if let C4Item::Boundary(b) = i { Some(*b) } else { None }).collect();
    for row in shapes.chunks(d.shapes_in_row) {
        let rw = row.iter().map(|&e| sizes[e].0).sum::<f32>() + (row.len() - 1) as f32 * SHAPE_GAP;
        let rh = row.iter().map(|&e| sizes[e].1).fold(0.0f32, f32::max);
        w = w.max(rw);
        h += rh + SHAPE_GAP;
    }
    if !shapes.is_empty() && bounds.is_empty() {
        h -= SHAPE_GAP;
    }
    for row in bounds.chunks(d.boundaries_in_row) {
        let ms: Vec<(f32, f32)> = row.iter().map(|&b| measure_boundary(d, b, sizes, memo)).collect();
        w = w.max(ms.iter().map(|m| m.0).sum::<f32>() + (ms.len() - 1) as f32 * BOUNDARY_GAP);
        h += ms.iter().map(|m| m.1).fold(0.0f32, f32::max) + BOUNDARY_GAP;
    }
    if !bounds.is_empty() {
        h -= BOUNDARY_GAP;
    }
    (w, h)
}

fn measure_boundary(d: &C4Diagram, b: usize, sizes: &[(f32, f32)], memo: &mut [Option<(f32, f32)>]) -> (f32, f32) {
    if let Some(m) = memo[b] {
        return m;
    }
    let (w, h) = measure_items(d, b, sizes, memo);
    let title_w = estimate_text_width(&d.boundaries[b].label, 14.0) + PAD * 2.0;
    let bw = (w + PAD * 2.0).max(title_w).max(160.0);
    let m = (bw, h + boundary_header(d, b, bw) + PAD);
    memo[b] = Some(m);
    m
}

#[allow(clippy::too_many_arguments)]
fn place_items(d: &C4Diagram, list: usize, x: f32, y: f32, sizes: &[(f32, f32)], memo: &mut [Option<(f32, f32)>], out: &mut C4Layout) {
    let shapes: Vec<usize> = d.children[list].iter().filter_map(|i| if let C4Item::Element(e) = i { Some(*e) } else { None }).collect();
    let bounds: Vec<usize> = d.children[list].iter().filter_map(|i| if let C4Item::Boundary(b) = i { Some(*b) } else { None }).collect();
    let mut cy = y;
    for row in shapes.chunks(d.shapes_in_row) {
        let rh = row.iter().map(|&e| sizes[e].1).fold(0.0f32, f32::max);
        let mut cx = x;
        for &e in row {
            out.elements[e] = RectF { x: cx, y: cy, w: sizes[e].0, h: sizes[e].1 };
            cx += sizes[e].0 + SHAPE_GAP;
        }
        cy += rh + SHAPE_GAP;
    }
    for row in bounds.chunks(d.boundaries_in_row) {
        let mut cx = x;
        let mut rh = 0.0f32;
        for &b in row {
            let (bw, bh) = measure_boundary(d, b, sizes, memo);
            out.boundaries[b] = RectF { x: cx, y: cy, w: bw, h: bh };
            place_items(d, b, cx + PAD, cy + boundary_header(d, b, bw), sizes, memo, out);
            cx += bw + BOUNDARY_GAP;
            rh = rh.max(bh);
        }
        cy += rh + BOUNDARY_GAP;
    }
}

pub(super) fn layout_c4(d: &C4Diagram, top: f32) -> C4Layout {
    let sizes: Vec<(f32, f32)> = d.elements.iter().map(element_size).collect();
    let mut memo = vec![None; d.boundaries.len()];
    let root = d.children.len() - 1;
    let (w, h) = measure_items(d, root, &sizes, &mut memo);
    let zero = RectF { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
    let mut out = C4Layout { elements: vec![zero; d.elements.len()], boundaries: vec![zero; d.boundaries.len()], width: w + MARGIN * 2.0, height: h + MARGIN * 2.0 + top };
    place_items(d, root, MARGIN, MARGIN + top, &sizes, &mut memo, &mut out);
    out
}

fn push_person_icon(svg: &mut String, cx: f32, y: f32, fill: &str) {
    svg.push_str(&format!(
        r##"<circle cx="{cx:.1}" cy="{:.1}" r="11" fill="{fill}" stroke="#ffffff" stroke-width="1.5"/><path d="M {:.1} {:.1} a 20 16 0 0 1 40 0 z" fill="{fill}" stroke="#ffffff" stroke-width="1.5"/>"##,
        y + 12.0,
        cx - 20.0,
        y + 42.0
    ));
}

pub(super) fn render_c4_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let d = parse_c4(source);
    if d.elements.is_empty() && d.boundaries.is_empty() {
        return None;
    }
    let title_band = if d.title.is_some() { 36.0 } else { 0.0 };
    let layout = layout_c4(&d, title_band);
    let title_w = d.title.as_deref().map_or(0.0, |t| estimate_text_width(t, 16.0) + MARGIN * 2.0);
    let total_w = layout.width.max(title_w).ceil();
    let total_h = layout.height.ceil();
    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));
    // One arrow marker per line colour (`context-stroke` is not understood everywhere).
    let mut colors: Vec<String> = vec!["#444444".to_string()];
    for rel in &d.rels {
        if let Some(c) = rel.line_color.as_deref().map(escape_attr)
            && !colors.contains(&c)
        {
            colors.push(c);
        }
    }
    svg.push_str("<defs>");
    for (i, c) in colors.iter().enumerate() {
        let id = if i == 0 { "c4-arrow".to_string() } else { format!("c4-arrow-{i}") };
        svg.push_str(&format!(
            r##"<marker id="{id}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="9" markerHeight="9" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="{c}"/></marker>"##
        ));
    }
    svg.push_str("</defs>");
    if let Some(title) = &d.title {
        svg.push_str(&format!(
            r##"<text x="{MARGIN}" y="{:.1}" font-size="16" font-weight="bold" fill="#0f172a" data-role="diagram-title">{}</text>"##,
            MARGIN + 14.0,
            escape_xml(title)
        ));
    }
    // Label placement keeps clear of shapes and boundary titles.
    let mut obstacles: Vec<RectF> = layout.elements.clone();
    for (b, boundary) in d.boundaries.iter().enumerate() {
        let r = layout.boundaries[b];
        let dash = if boundary.node { "" } else { r#" stroke-dasharray="7,7""# };
        svg.push_str(&format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="3" fill="none" stroke="#444444" stroke-width="1"{dash} data-boundary="{}"/>"##,
            r.x,
            r.y,
            r.w,
            r.h,
            escape_attr(&boundary.alias)
        ));
        let mut y = r.y + 6.0;
        for (line, size, bold) in header_lines(&d, b, r.w) {
            y += line_height(size);
            let weight = if bold { r#" font-weight="bold""# } else { "" };
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" font-size="{size}"{weight} fill="#444444">{}</text>"##,
                r.x + 10.0,
                y - size * 0.3,
                escape_xml(&line)
            ));
        }
        obstacles.push(RectF { x: r.x, y: r.y, w: r.w.min(240.0), h: y - r.y + 4.0 });
    }
    let rect_of = |alias: &str| {
        d.elements
            .iter()
            .position(|e| e.alias == alias)
            .map(|i| layout.elements[i])
            .or_else(|| d.boundaries.iter().position(|b| b.alias == alias).map(|b| layout.boundaries[b]))
    };
    // Relationship lines under the shapes; their labels on top, nudged apart along the line.
    let mut labels = String::new();
    let mut placed: Vec<RectF> = Vec::new();
    for (rel_index, rel) in d.rels.iter().enumerate().map(|(i, r)| (i + 1, r)) {
        let (Some(a), Some(b)) = (rect_of(&rel.from), rect_of(&rel.to)) else { continue };
        let (acx, acy, bcx, bcy) = (a.x + a.w / 2.0, a.y + a.h / 2.0, b.x + b.w / 2.0, b.y + b.h / 2.0);
        let (sx, sy) = edge_anchor(acx, acy, a.w, a.h, bcx, bcy);
        let (ex, ey) = edge_anchor(bcx, bcy, b.w, b.h, acx, acy);
        let line = rel.line_color.as_deref().map(escape_attr).unwrap_or_else(|| "#444444".to_string());
        let marker = match colors.iter().position(|c| *c == line) {
            Some(i) if i > 0 => format!("c4-arrow-{i}"),
            _ => "c4-arrow".to_string(),
        };
        let start = if rel.arrow_start { format!(r#" marker-start="url(#{marker})""#) } else { String::new() };
        let end = if rel.arrow_end { format!(r#" marker-end="url(#{marker})""#) } else { String::new() };
        svg.push_str(&format!(
            r##"<line x1="{sx:.1}" y1="{sy:.1}" x2="{ex:.1}" y2="{ey:.1}" stroke="{line}" stroke-width="1.2"{start}{end} data-rel="{}-{}"/>"##,
            escape_attr(&rel.from),
            escape_attr(&rel.to)
        ));
        let rel_label = if d.dynamic && !rel.label.is_empty() {
            format!("{}: {}", rel_index, rel.label)
        } else {
            rel.label.clone()
        };
        let mut text: Vec<(String, f32, bool)> = label_lines(&rel_label, 160.0, 12.0).into_iter().map(|l| (l, 12.0, true)).collect();
        if let Some(t) = &rel.techn {
            text.extend(label_lines(&format!("[{t}]"), 160.0, 11.0).into_iter().map(|l| (l, 11.0, false)));
        }
        if text.is_empty() {
            continue;
        }
        let lw = text.iter().map(|(l, s, _)| estimate_text_width(l, *s)).fold(0.0f32, f32::max) + 8.0;
        let lh = text.iter().map(|(_, s, _)| line_height(*s)).sum::<f32>() + 4.0;
        // The point along the line whose label box overlaps the least (labels first, then shapes).
        let overlap = |r: &RectF, q: &RectF| ((r.x + r.w).min(q.x + q.w) - r.x.max(q.x)).max(0.0) * ((r.y + r.h).min(q.y + q.h) - r.y.max(q.y)).max(0.0);
        let mut at = ((sx + ex) / 2.0, (sy + ey) / 2.0);
        let mut best = f32::MAX;
        for t in [0.5f32, 0.4, 0.6, 0.3, 0.7, 0.2, 0.8] {
            let p = (sx + (ex - sx) * t, sy + (ey - sy) * t);
            let r = RectF { x: p.0 - lw / 2.0, y: p.1 - lh / 2.0, w: lw, h: lh };
            let cost = placed.iter().map(|q| overlap(&r, q) * 4.0).sum::<f32>() + obstacles.iter().map(|q| overlap(&r, q)).sum::<f32>();
            if cost < best - 0.5 {
                best = cost;
                at = p;
            }
            if cost < 0.5 {
                break;
            }
        }
        let r = RectF { x: at.0 - lw / 2.0, y: at.1 - lh / 2.0, w: lw, h: lh };
        placed.push(r);
        let color = rel.text_color.as_deref().map(escape_attr).unwrap_or_else(|| "#444444".to_string());
        labels.push_str(&format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{lw:.1}" height="{lh:.1}" fill="#ffffff" opacity="0.85"/>"##,
            r.x, r.y
        ));
        let mut y = r.y + 2.0;
        for (l, size, bold) in text {
            y += line_height(size);
            let style = if bold { r#" font-weight="bold""# } else { r#" font-style="italic""# };
            labels.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="{size}"{style} fill="{color}">{}</text>"##,
                at.0,
                y - size * 0.35,
                escape_xml(&l)
            ));
        }
    }
    for (i, e) in d.elements.iter().enumerate() {
        let r = layout.elements[i];
        let (bg, border, font) = element_colors(e);
        let bg = e.bg.as_deref().map(escape_attr).unwrap_or_else(|| bg.to_string());
        let border = e.border.as_deref().map(escape_attr).unwrap_or_else(|| border.to_string());
        let font = e.font.as_deref().map(escape_attr).unwrap_or_else(|| font.to_string());
        let alias = escape_attr(&e.alias);
        let mut top = r.y + 14.0;
        match e.form {
            C4Form::Box => svg.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" rx="6" fill="{bg}" stroke="{border}" stroke-width="1" data-c4="{alias}"/>"##,
                r.x, r.y, r.w, r.h
            )),
            C4Form::Db => {
                let ry = 10.0;
                svg.push_str(&format!(
                    r##"<path d="M {x0:.1} {y0:.1} A {rx:.1} {ry} 0 0 0 {x1:.1} {y0:.1} V {y1:.1} A {rx:.1} {ry} 0 0 1 {x0:.1} {y1:.1} Z" fill="{bg}" stroke="{border}" stroke-width="1" data-c4="{alias}"/><path d="M {x0:.1} {y0:.1} A {rx:.1} {ry} 0 0 1 {x1:.1} {y0:.1}" fill="none" stroke="{border}" stroke-width="1"/>"##,
                    x0 = r.x,
                    x1 = r.x + r.w,
                    y0 = r.y + ry,
                    y1 = r.y + r.h - ry,
                    rx = r.w / 2.0
                ));
                top += 14.0;
            }
            C4Form::Queue => {
                let rx = 10.0;
                svg.push_str(&format!(
                    r##"<path d="M {x0:.1} {y0:.1} H {x1:.1} A {rx} {ry:.1} 0 0 1 {x1:.1} {y1:.1} H {x0:.1} A {rx} {ry:.1} 0 0 1 {x0:.1} {y0:.1} Z" fill="{bg}" stroke="{border}" stroke-width="1" data-c4="{alias}"/><path d="M {x1:.1} {y0:.1} A {rx} {ry:.1} 0 0 0 {x1:.1} {y1:.1}" fill="none" stroke="{border}" stroke-width="1"/>"##,
                    x0 = r.x + rx,
                    x1 = r.x + r.w - rx,
                    y0 = r.y,
                    y1 = r.y + r.h,
                    ry = r.h / 2.0
                ));
            }
        }
        let cx = r.x + r.w / 2.0;
        if e.kind == C4Kind::Person {
            push_person_icon(&mut svg, cx, top - 6.0, &bg);
            top += 44.0;
        }
        let lines = element_lines(e);
        let text_h: f32 = lines.iter().map(|l| line_height(l.1)).sum();
        // Centre the text in the space left below the icon.
        let mut y = top + ((r.y + r.h - 10.0 - top) - text_h).max(0.0) / 2.0;
        for (l, size, bold, italic) in lines {
            y += line_height(size);
            let style = if bold {
                r#" font-weight="bold""#
            } else if italic {
                r#" font-style="italic""#
            } else {
                ""
            };
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="{size}"{style} fill="{font}">{}</text>"##,
                y - size * 0.3,
                escape_xml(&l)
            ));
        }
    }
    svg.push_str(&labels);
    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w as u32, total_h as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn c4_call_arguments_quotes_and_named() {
        let (name, args, named, block) = parse_c4_call(r#"Person(customer, Customer, "A customer, with accounts", $tags="v1.0") {"#).unwrap();
        assert_eq!(name, "Person");
        assert_eq!(args, vec!["customer", "Customer", "A customer, with accounts"]);
        assert_eq!(named, vec![("tags".to_string(), "v1.0".to_string())]);
        assert!(block);
        assert_eq!(element_kind("SystemQueue_Ext"), Some((C4Kind::System, C4Form::Queue, true)));
        assert_eq!(element_kind("ContainerDb"), Some((C4Kind::Container, C4Form::Db, false)));
        assert_eq!(element_kind("PersonDb"), None);
        assert_eq!(safe_color("red\"/><script"), None);
    }

    #[test]
    fn c4_boundaries_nest_and_rows_wrap() {
        let d = parse_c4(
            "C4Context\n  title T\n  Enterprise_Boundary(b0, \"Bank\") {\n    Person(a, \"A\")\n    Person(b, \"B\")\n    System(c, \"C\")\n    System_Boundary(b1, \"Inner\") {\n      System(e, \"E\", \"desc\")\n    }\n  }\n  System_Ext(x, \"X\")\n  Rel(a, c, \"Uses\", \"HTTPS\")\n  UpdateElementStyle(a, $bgColor=\"grey\", $fontColor=\"red\")\n  UpdateLayoutConfig($c4ShapeInRow=\"2\", $c4BoundaryInRow=\"1\")",
        );
        assert_eq!(d.title.as_deref(), Some("T"));
        assert_eq!(d.boundaries[1].parent, Some(0));
        assert_eq!(d.boundaries[0].kind.as_deref(), Some("ENTERPRISE"));
        assert_eq!((d.shapes_in_row, d.boundaries_in_row), (2, 1));
        assert_eq!((d.elements[0].bg.as_deref(), d.elements[0].font.as_deref()), (Some("grey"), Some("red")));
        let l = layout_c4(&d, 0.0);
        let (a, b, c, e) = (l.elements[0], l.elements[1], l.elements[2], l.elements[3]);
        // Two shapes per row: `c` wraps under `a`.
        assert!((a.y - b.y).abs() < 0.1 && b.x > a.x + a.w && (c.x - a.x).abs() < 0.1 && c.y > a.y + a.h);
        let (b0, b1) = (l.boundaries[0], l.boundaries[1]);
        for r in [a, b, c, e] {
            assert!(r.x > b0.x && r.x + r.w < b0.x + b0.w && r.y > b0.y && r.y + r.h < b0.y + b0.h);
        }
        assert!(e.x > b1.x && e.y > b1.y && b1.y > c.y + c.h && b1.y + b1.h < b0.y + b0.h);
        // The external system sits outside the boundary.
        let x = l.elements[4];
        assert!(x.y >= b0.y + b0.h || x.x >= b0.x + b0.w || x.y + x.h <= b0.y);
    }

    #[test]
    fn c4_svg_shapes_colours_and_relations() {
        let svg = svg_of(
            "C4Container\n  Person(u, \"User\")\n  ContainerDb(db, \"DB\", \"SQL\", \"Stores data\")\n  SystemQueue_Ext(q, \"Queue\")\n  Container_Boundary(c1, \"App\") {\n    Component(api, \"API\", \"Rust\")\n  }\n  Rel(u, api, \"Uses\", \"HTTPS\")\n  BiRel(api, db, \"Reads\")\n  Rel_Back(q, api, \"Feeds\")\n  UpdateRelStyle(u, api, $textColor=\"blue\", $lineColor=\"blue\")",
        );
        assert!(svg.contains("[Person]") && svg.contains("[Container: SQL]") && svg.contains("[Software System]") && svg.contains("[Component: Rust]"));
        assert!(svg.contains(">Stores data<") && svg.contains(">[CONTAINER]<"));
        assert!(svg.contains(r##"fill="#08427B""##) && svg.contains(r##"fill="#85BBF0""##));
        assert!(svg.contains(r#"stroke="blue" stroke-width="1.2" marker-end="url(#c4-arrow-1)" data-rel="u-api""#));
        assert!(svg.contains(r#"marker-start="url(#c4-arrow)" marker-end="url(#c4-arrow)" data-rel="api-db""#));
        assert!(svg.contains(r#"marker-start="url(#c4-arrow)" data-rel="q-api""#));
        assert!(svg.contains(r##"fill="blue">Uses</text>"##) && svg.contains(">[HTTPS]<"));
        assert!(svg.contains(r#"data-boundary="c1""#));
    }

    #[test]
    fn c4_dynamic_numbers_relationships() {
        let svg = svg_of(
            "C4Dynamic\n  Container(c1, \"App\", \"Java\")\n  Component(c2, \"API\", \"Spring\")\n  Rel(c1, c2, \"Submits credentials to\", \"JSON/HTTPS\")\n  Rel(c2, c1, \"Calls isAuthenticated() on\")\n",
        );
        assert!(svg.contains(">1: Submits credentials") && svg.contains("to</text>"), "{svg}");
        assert!(svg.contains(">2: Calls") && svg.contains("isAuthenticated() on</text>"), "{svg}");
        assert!(svg.contains("[Container: Java]"));
    }
}
