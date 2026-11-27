// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `usecase-beta`: UML use case diagrams. Actors (stick figures), use cases (ellipses, or
//! boxes with `[ ]`), system boundaries (optionally drawn as packages), JSON data boxes,
//! notes, and associations with arrow / circle / cross / generalization ends.

use super::*;

const PALETTE: &[&str] = &["#2563eb", "#16a34a", "#d97706", "#9333ea", "#dc2626", "#0891b2"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UcEnd {
    None,
    Arrow,
    Circle,
    Cross,
    Triangle,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum UcKind {
    /// `type`: `normal`, `hollow`, `awesome`; `icon`: an `fa:` icon.
    Actor { style: String, icon: Option<String> },
    UseCase { rect: bool },
    /// Top-level `key: value` rows.
    Json(Vec<(String, String)>),
    Note,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct UcNode {
    pub(super) id: String,
    pub(super) label: String,
    pub(super) kind: UcKind,
    pub(super) business: bool,
    pub(super) stereotype: Option<String>,
    pub(super) classes: Vec<String>,
    pub(super) group: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct UcBoundary {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) package: bool,
    pub(super) classes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct UcEdge {
    pub(super) from: usize,
    pub(super) to: usize,
    pub(super) start: UcEnd,
    pub(super) end: UcEnd,
    pub(super) dashed: bool,
    pub(super) label: Option<String>,
    pub(super) id: Option<String>,
}

#[derive(Debug, Default)]
pub(super) struct Usecase {
    pub(super) horizontal: bool,
    pub(super) reversed: bool,
    pub(super) nodes: Vec<UcNode>,
    pub(super) boundaries: Vec<UcBoundary>,
    pub(super) edges: Vec<UcEdge>,
    pub(super) class_defs: HashMap<String, FlowStyle>,
    pub(super) styles: Vec<(String, FlowStyle)>,
    /// `class e1 cls` on an edge id.
    pub(super) edge_classes: Vec<(String, String)>,
}

/// One parsed link: (lhs, edge id, start, end, dashed, label, rhs).
type UcLink<'a> = (&'a str, Option<String>, UcEnd, UcEnd, bool, Option<String>, &'a str);

/// Where the first link starts in `s` (outside quotes): `--`, `..`, `<-`, or an `o` / `x` start marker.
fn find_link(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut quote = false;
    for i in 0..b.len() {
        if b[i] == b'"' {
            quote = !quote;
            continue;
        }
        if quote {
            continue;
        }
        let run = |at: usize| at + 1 < b.len() && matches!(b[at], b'-' | b'.') && matches!(b[at + 1], b'-' | b'.');
        let word_start = i == 0 || b[i - 1].is_ascii_whitespace();
        if run(i) || (b[i] == b'<' && run(i + 1)) || (matches!(b[i], b'o' | b'x') && word_start && run(i + 1)) {
            return Some(i);
        }
    }
    None
}

/// The link at the start of `s`: (start, end, dashed, bytes used).
fn link_at(s: &str) -> (UcEnd, UcEnd, bool, usize) {
    let b = s.as_bytes();
    let mut j = 0;
    let start = match b.first() {
        Some(b'<') => {
            j = 1;
            UcEnd::Arrow
        }
        Some(b'o') if b.get(1).is_some_and(|c| matches!(c, b'-' | b'.')) => {
            j = 1;
            UcEnd::Circle
        }
        Some(b'x') if b.get(1).is_some_and(|c| matches!(c, b'-' | b'.')) => {
            j = 1;
            UcEnd::Cross
        }
        _ => UcEnd::None,
    };
    let run = b[j..].iter().take_while(|c| matches!(c, b'-' | b'.')).count();
    let dashed = b[j..j + run].contains(&b'.');
    j += run;
    let boundary = |at: usize| b.get(at).is_none_or(|c| c.is_ascii_whitespace());
    let end = if s[j..].starts_with("|>") {
        j += 2;
        UcEnd::Triangle
    } else if b.get(j) == Some(&b'>') {
        j += 1;
        UcEnd::Arrow
    } else if b.get(j) == Some(&b'o') && boundary(j + 1) {
        j += 1;
        UcEnd::Circle
    } else if b.get(j) == Some(&b'x') && boundary(j + 1) {
        j += 1;
        UcEnd::Cross
    } else {
        UcEnd::None
    };
    (start, end, dashed, j)
}

/// `A --> B`, `A <-- B`, `A --o B`, `A --|> B`, `A -- text --> B`, `A ..> : include B`, `A e1@--> B`.
pub(super) fn parse_uc_link(s: &str) -> Option<UcLink<'_>> {
    let at = find_link(s)?;
    let mut lhs = s[..at].trim();
    let mut id = None;
    if let Some(before) = lhs.strip_suffix('@') {
        let (node, eid) = before.trim().rsplit_once(char::is_whitespace)?;
        id = Some(eid.trim().to_string());
        lhs = node.trim();
    }
    if lhs.is_empty() {
        return None;
    }
    let (mut start, mut end, mut dashed, used) = link_at(&s[at..]);
    let mut rest = s[at + used..].trim_start();
    let mut label = None;
    if let Some(stereo) = rest.strip_prefix(':') {
        let stereo = stereo.trim_start();
        let (word, after) = stereo.split_once(char::is_whitespace)?;
        label = Some(format!("«{}»", word.trim()));
        rest = after.trim();
    } else if start == UcEnd::None
        && end == UcEnd::None
        && let Some(k) = find_link(rest)
    {
        // `A -- text --> B`: the text runs up to the second half of the link.
        let text = rest[..k].trim();
        if !text.is_empty() {
            let (s2, e2, d2, used2) = link_at(&rest[k..]);
            label = Some(unquote(text));
            start = s2;
            end = e2;
            dashed |= d2;
            rest = rest[k + used2..].trim();
        }
    }
    let rhs = rest.trim();
    (!rhs.is_empty() && !rhs.contains(char::is_whitespace) || rhs.starts_with('"')).then_some((lhs, id, start, end, dashed, label, rhs))
}

/// A declaration's parts: (id, label, `[ ]` box, `@{ }` attributes, `<<stereotype>>`, classes).
#[allow(clippy::type_complexity)]
pub(super) fn parse_uc_decl(text: &str) -> (String, Option<String>, bool, Vec<(String, String)>, Option<String>, Vec<String>) {
    let mut s = text.trim();
    let mut classes = Vec::new();
    if let Some(at) = s.rfind(":::")
        && !s[at..].contains(['"', ']', ')', '}'])
    {
        classes = s[at + 3..].split(',').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
        s = s[..at].trim();
    }
    let mut stereotype = None;
    if s.ends_with(">>")
        && let Some(at) = s.rfind("<<")
    {
        stereotype = Some(s[at + 2..s.len() - 2].trim().to_string());
        s = s[..at].trim();
    }
    let mut attrs = Vec::new();
    if s.ends_with('}')
        && let Some(at) = s.find("@{")
    {
        attrs = parse_brace_attrs(&s[at + 2..s.len() - 1]);
        s = s[..at].trim();
    }
    if s.starts_with('"') {
        let label = unquote(s);
        return (label.clone(), Some(label), false, attrs, stereotype, classes);
    }
    match s.find(['(', '[']) {
        Some(open) => {
            let rect = s.as_bytes()[open] == b'[';
            let inner = s[open + 1..].trim_end_matches([')', ']']);
            (s[..open].trim().to_string(), Some(unquote(inner.trim())), rect, attrs, stereotype, classes)
        }
        None => (s.to_string(), None, false, attrs, stereotype, classes),
    }
}

/// Split on commas outside quotes, brackets and braces.
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

/// `{ "a": 1, "items": [{ "name": "Book" }] }` → rows `a: 1`, `items[0].name: Book`,
/// matching mermaid.js nested JSON tables.
pub(super) fn json_rows(body: &str) -> Vec<(String, String)> {
    let wrapped = format!("{{{body}}}");
    let mut rows = Vec::new();
    flatten_json(&parse_json_value(&wrapped), "", &mut rows);
    rows
}

#[derive(Debug, Clone)]
enum JsonVal {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<JsonVal>),
    Obj(Vec<(String, JsonVal)>),
}

fn parse_json_value(s: &str) -> JsonVal {
    let s = s.trim();
    if s == "null" {
        return JsonVal::Null;
    }
    if s == "true" || s == "false" {
        return JsonVal::Bool(s == "true");
    }
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1];
        if inner.trim().is_empty() {
            return JsonVal::Obj(Vec::new());
        }
        return JsonVal::Obj(
            split_top(inner)
                .into_iter()
                .filter_map(|entry| {
                    let colon = find_unquoted(entry, ':')?;
                    Some((unquote(entry[..colon].trim()), parse_json_value(entry[colon + 1..].trim())))
                })
                .collect(),
        );
    }
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        if inner.trim().is_empty() {
            return JsonVal::Arr(Vec::new());
        }
        return JsonVal::Arr(split_top(inner).into_iter().map(parse_json_value).collect());
    }
    if s.starts_with('"') {
        return JsonVal::Str(unquote(s));
    }
    JsonVal::Num(s.to_string())
}

fn json_disp(v: &JsonVal) -> String {
    match v {
        JsonVal::Null => "null".into(),
        JsonVal::Bool(b) => b.to_string(),
        JsonVal::Num(s) | JsonVal::Str(s) => s.clone(),
        JsonVal::Obj(_) => "{}".into(),
        JsonVal::Arr(_) => "[]".into(),
    }
}

fn flatten_json(v: &JsonVal, prefix: &str, rows: &mut Vec<(String, String)>) {
    match v {
        JsonVal::Obj(entries) if entries.is_empty() && !prefix.is_empty() => rows.push((prefix.to_string(), "{}".into())),
        JsonVal::Obj(entries) => {
            for (k, val) in entries {
                let path = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
                flatten_json(val, &path, rows);
            }
        }
        JsonVal::Arr(items) if items.is_empty() && !prefix.is_empty() => rows.push((prefix.to_string(), "[]".into())),
        JsonVal::Arr(items) if items.iter().all(|i| !matches!(i, JsonVal::Obj(_) | JsonVal::Arr(_))) => {
            for (i, item) in items.iter().enumerate() {
                let key = if i == 0 { prefix.to_string() } else { String::new() };
                rows.push((key, json_disp(item)));
            }
        }
        JsonVal::Arr(items) => {
            for (i, item) in items.iter().enumerate() {
                flatten_json(item, &format!("{prefix}[{i}]"), rows);
            }
        }
        _ if !prefix.is_empty() => rows.push((prefix.to_string(), json_disp(v))),
        _ => {}
    }
}

fn brace_depth(s: &str) -> i32 {
    let mut quote = false;
    let mut depth = 0;
    for ch in s.chars() {
        match ch {
            '"' => quote = !quote,
            '{' if !quote => depth += 1,
            '}' if !quote => depth -= 1,
            _ => {}
        }
    }
    depth
}

fn ensure(u: &mut Usecase, id: &str, group: Option<usize>) -> usize {
    let id = unquote(id);
    if let Some(i) = u.nodes.iter().position(|n| n.id == id) {
        return i;
    }
    u.nodes.push(UcNode {
        label: id.clone(),
        id,
        kind: UcKind::UseCase { rect: false },
        business: false,
        stereotype: None,
        classes: Vec::new(),
        group,
    });
    u.nodes.len() - 1
}

fn truthy(attrs: &[(String, String)], key: &str) -> bool {
    attrs.iter().any(|(k, v)| k == key && v == "true")
}

pub(super) fn parse_usecase(source: &str) -> Usecase {
    let mut u = Usecase { horizontal: true, ..Default::default() };
    let mut open: Option<usize> = None;
    let mut lines = source.lines().skip(1);
    let mut package_ids: Vec<String> = Vec::new();
    while let Some(raw) = lines.next() {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if let Some(d) = l.strip_prefix("direction ") {
            let (h, r) = match d.trim() {
                "LR" => (true, false),
                "RL" => (true, true),
                "BT" => (false, true),
                _ => (false, false),
            };
            u.horizontal = h;
            u.reversed = r;
            continue;
        }
        if l.starts_with("accTitle") || l.starts_with("accDescr") {
            if l.ends_with('{') {
                for more in lines.by_ref() {
                    if more.trim() == "}" {
                        break;
                    }
                }
            }
            continue;
        }
        if l == "end" {
            open = None;
            continue;
        }
        if let Some(rest) = l.strip_prefix("classDef ") {
            let rest = rest.trim().trim_end_matches(';');
            let (names, props) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let style = parse_flow_style(props);
            for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                u.class_defs.entry(name.to_string()).or_default().merge(&style);
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("class ") {
            if let Some((ids, classes)) = rest.trim().rsplit_once(char::is_whitespace) {
                for id in ids.split(',').map(str::trim) {
                    for c in classes.split(',').map(str::trim) {
                        if let Some(n) = u.nodes.iter_mut().find(|n| n.id == id) {
                            n.classes.push(c.to_string());
                        } else if let Some(b) = u.boundaries.iter_mut().find(|b| b.id == id) {
                            b.classes.push(c.to_string());
                        } else {
                            u.edge_classes.push((id.to_string(), c.to_string()));
                        }
                    }
                }
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("style ") {
            if let Some((ids, props)) = rest.trim().split_once(char::is_whitespace) {
                let style = parse_flow_style(props);
                for id in ids.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    u.styles.push((id.to_string(), style.clone()));
                }
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("note for ") {
            let rest = rest.trim();
            let (target, text) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let t = ensure(&mut u, target, open);
            let label = unquote(text.trim());
            u.nodes.push(UcNode {
                id: format!("note:{}", u.nodes.len()),
                label,
                kind: UcKind::Note,
                business: false,
                stereotype: None,
                classes: Vec::new(),
                group: u.nodes[t].group,
            });
            let n = u.nodes.len() - 1;
            u.edges.push(UcEdge { from: n, to: t, start: UcEnd::None, end: UcEnd::None, dashed: true, label: None, id: Some("note".into()) });
            continue;
        }
        if let Some(rest) = l.strip_prefix("json ") {
            let mut text = rest.trim().to_string();
            while brace_depth(&text) > 0 {
                let Some(more) = lines.next() else { break };
                text.push('\n');
                text.push_str(more.trim());
            }
            let (head, classes) = match text.rfind("}:::") {
                Some(at) => (text[..=at].to_string(), text[at + 4..].split(',').map(|c| c.trim().to_string()).collect()),
                None => (text.clone(), Vec::new()),
            };
            let Some(at) = head.find("@{") else { continue };
            let id = head[..at].trim().to_string();
            let body = head[at + 2..].trim_end().trim_end_matches('}').to_string();
            let i = ensure(&mut u, &id, open);
            u.nodes[i].kind = UcKind::Json(json_rows(&body));
            u.nodes[i].classes = classes;
            continue;
        }
        if let Some(rest) = l.strip_prefix("systemBoundary ") {
            let (id, title, _, attrs, _, classes) = parse_uc_decl(rest);
            let (id, title) = match title {
                Some(t) if rest.trim_start().starts_with('"') => (t.replace(' ', "_"), t),
                Some(t) => (id, t),
                None => (id.clone(), id),
            };
            let package = attrs.iter().any(|(k, v)| k == "type" && v == "package");
            u.boundaries.push(UcBoundary { id, title, package, classes });
            open = Some(u.boundaries.len() - 1);
            continue;
        }
        if let Some(rest) = l.strip_prefix("actor ") {
            let (id, label, _, attrs, stereotype, classes) = parse_uc_decl(rest);
            let i = ensure(&mut u, &id, open);
            let style = attrs.iter().find(|(k, _)| k == "type").map_or("normal".to_string(), |(_, v)| v.clone());
            let icon = attrs.iter().find(|(k, _)| k == "icon").map(|(_, v)| v.clone());
            let n = &mut u.nodes[i];
            n.kind = UcKind::Actor { style, icon };
            n.business = truthy(&attrs, "business");
            n.stereotype = stereotype;
            n.classes.extend(classes);
            if let Some(label) = label {
                n.label = label;
            }
            if n.group.is_none() {
                n.group = open;
            }
            continue;
        }
        if let Some((lhs, id, start, end, dashed, label, rhs)) = parse_uc_link(l) {
            let from = ensure(&mut u, lhs, None);
            let to = ensure(&mut u, rhs, None);
            u.edges.push(UcEdge { from, to, start, end, dashed, label, id });
            continue;
        }
        let (id, label, rect, attrs, stereotype, classes) = parse_uc_decl(l);
        if id.is_empty() {
            continue;
        }
        // `ID@{ … }` on its own changes an existing boundary or edge.
        if label.is_none() && !attrs.is_empty() && !u.nodes.iter().any(|n| n.id == id) {
            if attrs.iter().any(|(k, v)| k == "type" && v == "package") {
                package_ids.push(id);
            }
            continue;
        }
        let i = ensure(&mut u, &id, open);
        let n = &mut u.nodes[i];
        if let Some(label) = label {
            n.label = label;
        }
        if matches!(n.kind, UcKind::UseCase { .. }) {
            n.kind = UcKind::UseCase { rect };
        }
        n.business |= truthy(&attrs, "business");
        if stereotype.is_some() {
            n.stereotype = stereotype;
        }
        n.classes.extend(classes);
        if n.group.is_none() {
            n.group = open;
        }
    }
    for id in package_ids {
        if let Some(b) = u.boundaries.iter_mut().find(|b| b.id == id) {
            b.package = true;
        }
    }
    u
}

struct Fonts {
    actor_size: f32,
    actor_weight: String,
    actor_family: Option<String>,
    case_size: f32,
    case_weight: String,
    case_family: Option<String>,
}

fn node_size(n: &UcNode, f: &Fonts) -> (f32, f32) {
    let stereo_h = if n.stereotype.is_some() { 15.0 } else { 0.0 };
    match &n.kind {
        UcKind::Actor { .. } => {
            let widths = label_line_widths(&n.label, 140.0, f.actor_size);
            let tw = widths.iter().copied().fold(0.0f32, f32::max);
            ((tw + 10.0).max(56.0), 52.0 + widths.len() as f32 * f.actor_size * 1.25 + stereo_h + 4.0)
        }
        UcKind::UseCase { rect } => {
            let widths = label_line_widths(&n.label, 150.0, f.case_size);
            let tw = widths.iter().copied().fold(0.0f32, f32::max);
            let th = widths.len() as f32 * f.case_size * 1.25 + stereo_h;
            if *rect { ((tw + 32.0).max(90.0), (th + 22.0).max(42.0)) } else { ((tw * 1.2 + 36.0).max(110.0), (th + 28.0).max(50.0)) }
        }
        UcKind::Json(rows) => {
            let w = rows.iter().map(|(k, v)| estimate_text_width(&format!("{k}: {v}"), 11.0)).fold(estimate_text_width(&n.id, 12.0), f32::max);
            (w + 24.0, 28.0 + rows.len().max(1) as f32 * 15.0 + 6.0)
        }
        UcKind::Note => {
            let widths = label_line_widths(&n.label, 160.0, 11.0);
            (widths.iter().copied().fold(0.0f32, f32::max) + 24.0, widths.len() as f32 * 14.0 + 16.0)
        }
    }
}

fn marker_id(end: UcEnd) -> Option<&'static str> {
    match end {
        UcEnd::None => None,
        UcEnd::Arrow => Some("uc-arrow"),
        UcEnd::Circle => Some("uc-circle"),
        UcEnd::Cross => Some("uc-cross"),
        UcEnd::Triangle => Some("uc-tri"),
    }
}

fn push_actor(svg: &mut String, style: &str, cx: f32, top: f32, business: bool) {
    let stroke = r##"stroke="#334155" stroke-width="1.6""##;
    let hy = top + 9.0;
    match style {
        "awesome" => svg.push_str(&format!(
            r##"<circle cx="{cx:.1}" cy="{hy:.1}" r="8" fill="#334155" data-actor="awesome"/><path d="M {:.1} {:.1} a 15 14 0 0 1 30 0 v 6 h -30 z" fill="#334155"/>"##,
            cx - 15.0,
            top + 34.0
        )),
        "hollow" => svg.push_str(&format!(
            r##"<circle cx="{cx:.1}" cy="{hy:.1}" r="8" fill="#ffffff" {stroke} data-actor="hollow"/><path d="M {:.1} {:.1} q 13 -10 26 0 l -4 14 h -18 z" fill="#ffffff" {stroke}/><path d="M {:.1} {:.1} l -4 12 M {:.1} {:.1} l 4 12" fill="none" {stroke}/>"##,
            cx - 13.0,
            top + 24.0,
            cx - 5.0,
            top + 38.0,
            cx + 5.0,
            top + 38.0
        )),
        _ => svg.push_str(&format!(
            r##"<circle cx="{cx:.1}" cy="{hy:.1}" r="8" fill="#ffffff" {stroke} data-actor="normal"/><path d="M {cx:.1} {:.1} V {:.1} M {:.1} {:.1} H {:.1} M {:.1} {:.1} L {cx:.1} {:.1} L {:.1} {:.1}" fill="none" {stroke}/>"##,
            top + 17.0,
            top + 34.0,
            cx - 13.0,
            top + 23.0,
            cx + 13.0,
            cx - 11.0,
            top + 48.0,
            top + 34.0,
            cx + 11.0,
            top + 48.0
        )),
    }
    if business {
        svg.push_str(&format!(
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" {stroke} data-role="business"/>"##,
            cx - 9.0,
            hy + 7.0,
            cx + 2.0,
            hy - 9.0
        ));
    }
}

fn group_font(svg: &mut String, family: &Option<String>, inner: String) {
    match family {
        Some(f) => svg.push_str(&format!(r#"<g font-family="{f}">{inner}</g>"#)),
        None => svg.push_str(&inner),
    }
}

pub(super) fn render_usecase_svg(source: &str, config: &MermaidConfig) -> Option<(Vec<u8>, u32, u32)> {
    let u = parse_usecase(source);
    if u.nodes.is_empty() {
        return None;
    }
    let number = |key: &str, default: f32, lo: f32, hi: f32| config.option(key).and_then(px_number).map_or(default, |v| (v as f32).clamp(lo, hi));
    let weight = |key: &str, default: &str| config.option(key).map(unquote).and_then(|v| safe_css_value(&v)).unwrap_or_else(|| default.to_string());
    let family = |key: &str| config.option(key).map(unquote).and_then(|v| safe_css_value(&v));
    let fonts = Fonts {
        actor_size: number("usecase.actorFontSize", 13.0, 8.0, 32.0),
        actor_weight: weight("usecase.actorFontWeight", "500"),
        actor_family: family("usecase.actorFontFamily"),
        case_size: number("usecase.usecaseFontSize", 13.0, 8.0, 32.0),
        case_weight: weight("usecase.usecaseFontWeight", "500"),
        case_family: family("usecase.usecaseFontFamily"),
    };
    let node_gap = number("usecase.nodeSpacing", 34.0, 8.0, 200.0);
    let mut rank_gap = number("usecase.rankSpacing", 64.0, 20.0, 300.0);
    if u.horizontal {
        // Leave room for edge labels between ranks so arrows stay visible.
        let widest = u.edges.iter().filter_map(|e| e.label.as_deref()).map(|l| flow_edge_label_size(l).0).fold(0.0f32, f32::max);
        if widest > 0.0 {
            rank_gap = rank_gap.max(widest + 48.0);
        }
    }
    let margin = number("usecase.diagramPadding", 32.0, 4.0, 200.0);
    let rotate = config.option("usecase.colorScheme").map(unquote).as_deref() == Some("rotate");

    let sizes: Vec<(f32, f32)> = u.nodes.iter().map(|n| node_size(n, &fonts)).collect();
    let pairs: Vec<(usize, usize)> = u.edges.iter().map(|e| (e.from, e.to)).collect();
    let member_of: Vec<Option<usize>> = u.nodes.iter().map(|n| n.group).collect();
    let (centers, w, h, frames) = if u.boundaries.is_empty() {
        let (c, w, h) = compute_layered_layout(sizes.len(), &pairs, &sizes, u.horizontal, u.reversed, margin, rank_gap, node_gap);
        (c, w, h, Vec::new())
    } else {
        let groups: Vec<NodeGroup> = u.boundaries.iter().map(|b| NodeGroup { title: b.title.clone(), parent: None }).collect();
        let dirs = vec![None; groups.len()];
        layout_with_group_dirs(&sizes, &pairs, &member_of, &groups, &dirs, u.horizontal, u.reversed, margin.max(44.0), rank_gap, node_gap)
    };
    let canvas = (w as f32, h as f32);
    let style_for = |id: &str, classes: &[String]| {
        let mut st = u.class_defs.get("default").cloned().unwrap_or_default();
        for c in classes {
            if let Some(s) = u.class_defs.get(c) {
                st.merge(s);
            }
        }
        for (sid, s) in &u.styles {
            if sid == id {
                st.merge(s);
            }
        }
        st
    };
    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{w}" height="{h}" fill="#ffffff"/>"##));
    svg.push_str(concat!(
        r##"<defs><marker id="uc-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerUnits="userSpaceOnUse" markerWidth="11" markerHeight="11" orient="auto-start-reverse"><path d="M 0 1 L 10 5 L 0 9" fill="none" stroke="#475569" stroke-width="1.5"/></marker>"##,
        r##"<marker id="uc-tri" viewBox="0 0 12 12" refX="11" refY="6" markerUnits="userSpaceOnUse" markerWidth="14" markerHeight="14" orient="auto-start-reverse"><path d="M 1 1 L 11 6 L 1 11 z" fill="#ffffff" stroke="#475569" stroke-width="1.3"/></marker>"##,
        r##"<marker id="uc-circle" viewBox="0 0 10 10" refX="9" refY="5" markerUnits="userSpaceOnUse" markerWidth="10" markerHeight="10" orient="auto-start-reverse"><circle cx="5" cy="5" r="4" fill="#ffffff" stroke="#475569" stroke-width="1.5"/></marker>"##,
        r##"<marker id="uc-cross" viewBox="0 0 10 10" refX="5" refY="5" markerUnits="userSpaceOnUse" markerWidth="11" markerHeight="11" orient="auto-start-reverse"><path d="M 1 1 L 9 9 M 9 1 L 1 9" stroke="#475569" stroke-width="1.6"/></marker></defs>"##
    ));
    // System boundaries.
    for (b, boundary) in u.boundaries.iter().enumerate() {
        let Some(Some((x, y, fw, fh))) = frames.get(b).copied() else { continue };
        let at = svg.len();
        let id = escape_attr(&boundary.id);
        if boundary.package {
            let tab = estimate_text_width(&boundary.title, 13.0) + 20.0;
            svg.push_str(&format!(
                r##"<rect x="{x:.1}" y="{y:.1}" width="{tab:.1}" height="20" fill="#f8fafc" stroke="#64748b" stroke-width="1.4"/><rect x="{x:.1}" y="{:.1}" width="{fw:.1}" height="{:.1}" fill="#f8fafc" stroke="#64748b" stroke-width="1.4" data-boundary="{id}" data-package="true"/>"##,
                y + 20.0,
                fh - 20.0
            ));
        } else {
            svg.push_str(&format!(
                r##"<rect x="{x:.1}" y="{y:.1}" width="{fw:.1}" height="{fh:.1}" rx="4" fill="#f8fafc" stroke="#64748b" stroke-width="1.4" data-boundary="{id}"/>"##
            ));
        }
        let st = style_for(&boundary.id, &boundary.classes);
        if !st.is_empty() {
            let frag = svg.split_off(at);
            svg.push_str(&restyle_svg_fragment(&frag, &st));
        }
        let tx = if boundary.package { x + 10.0 } else { x + fw / 2.0 };
        let anchor = if boundary.package { "start" } else { "middle" };
        svg.push_str(&format!(
            r##"<text x="{tx:.1}" y="{:.1}" text-anchor="{anchor}" font-size="13" font-weight="bold" fill="#334155">{}</text>"##,
            y + 15.0,
            escape_xml(&boundary.title)
        ));
    }
    // Associations under the shapes, labels on top.
    let mut labels = String::new();
    for e in &u.edges {
        let route = route_layered_edge(e.from, e.to, &centers, &sizes, u.horizontal, true, canvas);
        let mut extra = String::new();
        if let Some(m) = marker_id(e.start) {
            extra.push_str(&format!(r#" marker-start="url(#{m})""#));
        }
        if let Some(m) = marker_id(e.end) {
            extra.push_str(&format!(r#" marker-end="url(#{m})""#));
        }
        if e.dashed {
            extra.push_str(r#" stroke-dasharray="6,4""#);
        }
        let mut st = FlowStyle::default();
        if let Some(id) = &e.id {
            for (eid, class) in &u.edge_classes {
                if eid == id
                    && let Some(c) = u.class_defs.get(class)
                {
                    st.merge(c);
                }
            }
            for (sid, s) in &u.styles {
                if sid == id {
                    st.merge(s);
                }
            }
        }
        let stroke = st.stroke.as_deref().and_then(safe_css_value).unwrap_or_else(|| "#475569".to_string());
        let width = st.stroke_width.as_deref().and_then(safe_css_value).map(|w| w.trim_end_matches("px").to_string()).unwrap_or_else(|| "1.5".to_string());
        if let Some(id) = &e.id {
            extra.push_str(&format!(r#" data-edge="{}""#, escape_attr(id)));
        }
        push_routed_edge(&mut svg, &route, &stroke, &width, &extra);
        if let Some(label) = e.label.as_deref().filter(|l| !l.is_empty()) {
            let (lw, lh) = flow_edge_label_size(label);
            let (lx, ly) = route.label_at;
            labels.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{lw:.1}" height="{lh:.1}" rx="3" fill="#ffffff" opacity="0.9"/>"##,
                lx - lw / 2.0,
                ly - lh / 2.0
            ));
            push_centered_label(&mut labels, lx, ly, label, 11.0, 140.0, "#334155", "normal");
        }
    }
    let mut use_case_index = 0;
    for (i, n) in u.nodes.iter().enumerate() {
        let (cx, cy) = centers[i];
        let (nw, nh) = sizes[i];
        let (x, y) = (cx - nw / 2.0, cy - nh / 2.0);
        let st = style_for(&n.id, &n.classes);
        let text_fill = st.color.as_deref().and_then(safe_css_value).unwrap_or_else(|| "#0f172a".to_string());
        let at = svg.len();
        match &n.kind {
            UcKind::Actor { style, icon } => {
                svg.push_str(r##"<g data-kind="actor">"##);
                match icon.as_deref().and_then(|i| i.strip_prefix("fa:")) {
                    Some(name) => svg.push_str(&format!(
                        r##"<circle cx="{cx:.1}" cy="{:.1}" r="22" fill="#eff6ff" stroke="#334155" stroke-width="1.6" data-actor="icon"/><text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="24" fill="#334155">{}</text>"##,
                        y + 24.0,
                        y + 32.0,
                        fa_symbol(name)
                    )),
                    None => push_actor(&mut svg, style, cx, y, n.business),
                }
                if !st.is_empty() {
                    let frag = svg.split_off(at);
                    svg.push_str(&restyle_svg_fragment(&frag, &st));
                }
                let mut text = String::new();
                let mut ty = y + 56.0;
                if let Some(s) = &n.stereotype {
                    text.push_str(&format!(
                        r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="11" font-style="italic" fill="#475569">«{}»</text>"##,
                        ty + 10.0,
                        escape_xml(s)
                    ));
                    ty += 15.0;
                }
                let lines = label_line_widths(&n.label, 140.0, fonts.actor_size).len() as f32;
                push_centered_label(&mut text, cx, ty + lines * fonts.actor_size * 1.25 / 2.0, &n.label, fonts.actor_size, 140.0, &text_fill, &fonts.actor_weight);
                group_font(&mut svg, &fonts.actor_family, text);
                svg.push_str("</g>");
            }
            UcKind::UseCase { rect } => {
                let (fill, stroke) = if rotate {
                    let c = PALETTE[use_case_index % PALETTE.len()];
                    (format!("{c}1a"), c.to_string())
                } else {
                    ("#eff6ff".to_string(), "#2563eb".to_string())
                };
                use_case_index += 1;
                if *rect {
                    svg.push_str(&format!(
                        r##"<rect x="{x:.1}" y="{y:.1}" width="{nw:.1}" height="{nh:.1}" rx="6" fill="{fill}" stroke="{stroke}" stroke-width="1.4" data-kind="usecase" data-usecase="{}"/>"##,
                        escape_attr(&n.id)
                    ));
                } else {
                    svg.push_str(&format!(
                        r##"<ellipse cx="{cx:.1}" cy="{cy:.1}" rx="{:.1}" ry="{:.1}" fill="{fill}" stroke="{stroke}" stroke-width="1.4" data-kind="usecase" data-usecase="{}"/>"##,
                        nw / 2.0,
                        nh / 2.0,
                        escape_attr(&n.id)
                    ));
                }
                if n.business {
                    // UML business use case: a slash at the lower right.
                    svg.push_str(&format!(
                        r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{stroke}" stroke-width="1.4" data-role="business"/>"##,
                        cx + nw * 0.28,
                        cy + nh * 0.42,
                        cx + nw * 0.40,
                        cy + nh * 0.18
                    ));
                }
                if !st.is_empty() {
                    let frag = svg.split_off(at);
                    svg.push_str(&restyle_svg_fragment(&frag, &st));
                }
                let mut text = String::new();
                let mut ly = cy;
                if let Some(s) = &n.stereotype {
                    let lines = label_line_widths(&n.label, 150.0, fonts.case_size).len() as f32;
                    let block = lines * fonts.case_size * 1.25 + 15.0;
                    text.push_str(&format!(
                        r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="11" font-style="italic" fill="#475569">«{}»</text>"##,
                        cy - block / 2.0 + 10.0,
                        escape_xml(s)
                    ));
                    ly += 7.5;
                }
                push_centered_label(&mut text, cx, ly, &n.label, fonts.case_size, 150.0, &text_fill, &fonts.case_weight);
                group_font(&mut svg, &fonts.case_family, text);
            }
            UcKind::Json(rows) => {
                svg.push_str(&format!(
                    r##"<rect x="{x:.1}" y="{y:.1}" width="{nw:.1}" height="{nh:.1}" rx="4" fill="#fefce8" stroke="#a16207" stroke-width="1.4" data-kind="json" data-json="{}"/><line x1="{x:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#a16207" stroke-width="1"/>"##,
                    escape_attr(&n.id),
                    y + 24.0,
                    x + nw,
                    y + 24.0
                ));
                if !st.is_empty() {
                    let frag = svg.split_off(at);
                    svg.push_str(&restyle_svg_fragment(&frag, &st));
                }
                svg.push_str(&format!(
                    r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="12" font-weight="bold" fill="{text_fill}">{}</text>"##,
                    y + 16.5,
                    escape_xml(&n.id)
                ));
                for (k, (key, value)) in rows.iter().enumerate() {
                    svg.push_str(&format!(
                        r##"<text x="{:.1}" y="{:.1}" font-size="11" fill="{text_fill}"><tspan font-weight="bold">{}</tspan> : {}</text>"##,
                        x + 10.0,
                        y + 40.0 + k as f32 * 15.0,
                        escape_attr(key),
                        escape_attr(value)
                    ));
                }
            }
            UcKind::Note => {
                svg.push_str(&format!(
                    r##"<path d="M {x:.1} {y:.1} H {:.1} L {:.1} {:.1} V {:.1} H {x:.1} Z" fill="#fff7d6" stroke="#ca8a04" stroke-width="1.2" data-role="note"/>"##,
                    x + nw - 10.0,
                    x + nw,
                    y + 10.0,
                    y + nh
                ));
                push_centered_label(&mut svg, cx, cy, &n.label, 11.0, 160.0, "#422006", "normal");
            }
        }
    }
    svg.push_str(&labels);
    svg.push_str("</svg>");
    Some((svg.into_bytes(), w, h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn usecase_links_ends_labels_and_ids() {
        let l = |s: &str| {
            let (a, id, st, en, dashed, label, b) = parse_uc_link(s).unwrap();
            (a.to_string(), id, st, en, dashed, label, b.to_string())
        };
        assert_eq!(l("A --> B"), ("A".into(), None, UcEnd::None, UcEnd::Arrow, false, None, "B".into()));
        assert_eq!(l("Start <-- Support").2, UcEnd::Arrow);
        assert_eq!(l("Start -- Finish").3, UcEnd::None);
        assert_eq!(l("User --o Start").3, UcEnd::Circle);
        assert_eq!(l("Start o-- Support").2, UcEnd::Circle);
        assert_eq!(l("Finish x-- Support").2, UcEnd::Cross);
        assert_eq!(l("Admin --|> Person").3, UcEnd::Triangle);
        let inc = l("Checkout ..> : include Payment");
        assert_eq!((inc.4, inc.5.as_deref(), inc.6.as_str()), (true, Some("«include»"), "Payment"));
        let lab = l("Analyst -- files R&D report --> Literal");
        assert_eq!((lab.5.as_deref(), lab.3, lab.6.as_str()), (Some("files R&D report"), UcEnd::Arrow, "Literal"));
        let id = l(r#"Customer opens@-- "starts checkout" ---> Checkout"#);
        assert_eq!((id.0.as_str(), id.1.as_deref(), id.5.as_deref()), ("Customer", Some("opens"), Some("starts checkout")));
        assert!(parse_uc_link("component real-time thing").is_none());
    }

    #[test]
    fn usecase_declarations_boundaries_json_and_notes() {
        let u = parse_usecase(
            "usecase-beta\nactor Staff(\"Order staff\")@{ type: hollow, business: true } <<Employee>>\nsystemBoundary ordering[\"Ordering System\"]@{ type: package }:::system\n  Checkout(\"Checkout\") <<Core>>:::critical\n  Review[Review order]\nend\nsystemBoundary \"Payment service\"\n  Pay\nend\nPayment_service@{ type: package }\njson Data@{\n  \"status\": \"pending\",\n  \"items\": [{ \"name\": \"Book\" }]\n}:::data\nnote for Checkout \"Validates\"\nStaff --> Review",
        );
        let n = |id: &str| u.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(n("Staff").kind, UcKind::Actor { style: "hollow".into(), icon: None });
        assert!(n("Staff").business && n("Staff").stereotype.as_deref() == Some("Employee"));
        assert_eq!((n("Checkout").stereotype.as_deref(), n("Checkout").classes.clone(), n("Checkout").group), (Some("Core"), vec!["critical".to_string()], Some(0)));
        assert_eq!(n("Review").kind, UcKind::UseCase { rect: true });
        assert_eq!(u.boundaries[0], UcBoundary { id: "ordering".into(), title: "Ordering System".into(), package: true, classes: vec!["system".into()] });
        assert!(u.boundaries[1].package && u.boundaries[1].id == "Payment_service");
                assert_eq!(n("Data").kind, UcKind::Json(vec![("status".into(), "pending".into()), ("items[0].name".into(), "Book".into())]));
        assert_eq!(n("Data").classes, vec!["data".to_string()]);
        assert!(u.nodes.iter().any(|n| n.kind == UcKind::Note && n.label == "Validates"));
        let rows = json_rows(r#""2": "second in source", "1": "first after 2", "colors": ["Red", "Green"], "address": { "city": "Oslo" }, "items": [{ "name": "Book", "quantity": 1 }], "emptyObject": {}, "emptyArray": []"#);
        assert!(rows.contains(&("2".into(), "second in source".into())));
        assert!(rows.contains(&("1".into(), "first after 2".into())));
        assert!(rows.contains(&("items[0].name".into(), "Book".into())));
        assert!(rows.contains(&("items[0].quantity".into(), "1".into())));
        assert!(rows.contains(&("address.city".into(), "Oslo".into())));
        assert!(rows.iter().any(|(k, v)| k == "colors" && v == "Red"));
        assert!(rows.iter().any(|(_, v)| v == "Green"));
        assert!(rows.contains(&("emptyObject".into(), "{}".into())));
        assert!(rows.contains(&("emptyArray".into(), "[]".into())));
    }

    #[test]
    fn usecase_svg_actors_cases_boundaries_and_ends() {
        let svg = svg_of(
            "usecase-beta\ndirection LR\nactor Customer\nactor Bot@{ type: awesome }\nsystemBoundary Shop\n  Browse(\"Browse catalogue\")\n  Pay[Pay]\nend\nCustomer --> Browse\nBot --o Pay\nBrowse ..> : include Pay\nPay --|> Browse\nstyle Browse stroke:#c33",
        );
        assert!(svg.contains(r#"data-actor="normal""#) && svg.contains(r#"data-actor="awesome""#));
        assert!(svg.contains(r#"data-kind="actor""#) && svg.contains(r#"data-kind="usecase""#));
        assert!(svg.contains(r#"<ellipse"#) && svg.contains(r#"data-usecase="Pay""#));
        assert!(svg.contains(r#"data-boundary="Shop""#) && svg.contains(">Shop</text>"));
        assert!(svg.contains(r#"marker-end="url(#uc-arrow)""#) && svg.contains(r#"marker-end="url(#uc-circle)""#) && svg.contains(r#"marker-end="url(#uc-tri)""#));
        assert!(svg.contains("«include»") && svg.contains(r#"stroke-dasharray="6,4""#));
        assert!(svg.contains(r##"stroke="#c33""##));
        // The actor stands left of the use case it talks to.
        assert!(text_x(&svg, "Customer") < text_x(&svg, "Browse catalogue"));
    }
}
