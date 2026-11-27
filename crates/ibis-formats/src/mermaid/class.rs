// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `classDiagram`.

use super::*;

pub(super) struct ClassNode {
    pub(super) name: String,
    /// `class Id["Label"]`, `Square~Shape~` (shown as `Square<Shape>`).
    pub(super) label: Option<String>,
    pub(super) stereotype: Option<String>,
    pub(super) fields: Vec<String>,
    pub(super) methods: Vec<String>,
    /// Innermost `namespace`.
    pub(super) namespace: Option<usize>,
    /// `:::name` / `cssClass "A" name`, in order.
    pub(super) classes: Vec<String>,
    /// A `note` box (its text is `name`).
    pub(super) note: bool,
    /// A lollipop interface (`bar ()-- foo`): a small circle named `name`.
    pub(super) interface: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ClassRelKind {
    Inheritance, // <|-- / --|>
    Composition, // *-- / --*
    Aggregation, // o-- / --o
    Association, // --> / <--
    Dependency,  // ..> / <..
    Realization, // ..|> / <|..
    Link,        // -- / ..
    /// `note for X` connector.
    Note,
}

pub(super) struct ClassEdge {
    pub(super) from: usize,
    pub(super) to: usize,
    pub(super) kind: ClassRelKind,
    pub(super) dashed: bool,
    pub(super) label: Option<String>,
    /// Multiplicity at each end (`Customer "1" --> "*" Order`).
    pub(super) from_card: Option<String>,
    pub(super) to_card: Option<String>,
}

/// `Name`, `Name:::style`, `Name~T~`, `Name["Label"]` → (key, label, class).
pub(super) fn class_name_parts(raw: &str) -> (String, Option<String>, Option<String>) {
    let raw = raw.trim();
    let (raw, css) = match raw.rfind(":::") {
        Some(at) if !raw[at + 3..].contains([']', '"']) => (raw[..at].trim(), Some(raw[at + 3..].trim().to_string())),
        _ => (raw, None),
    };
    if let Some(open) = raw.find('[')
        && raw.ends_with(']')
    {
        let label = unquote(&raw[open + 1..raw.len() - 1]);
        return (raw[..open].trim().to_string(), Some(label), css);
    }
    if let Some(tilde) = raw.find('~') {
        return (raw[..tilde].trim().to_string(), Some(class_generics(raw)), css);
    }
    (raw.to_string(), None, css)
}

pub(super) fn render_class_diagram_svg(source: &str, config: &MermaidConfig) -> Option<(Vec<u8>, u32, u32)> {
    // `class.hideEmptyMembersBox`: a class without members is just its name box.
    let hide_empty = config.flag("class.hideEmptyMembersBox", false);
    let mut horizontal = false;
    let mut reversed = false;
    let mut classes: Vec<ClassNode> = Vec::new();
    let mut map: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<ClassEdge> = Vec::new();
    let mut current_class: Option<usize> = None;
    let mut namespaces: Vec<NodeGroup> = Vec::new();
    let mut ns_stack: Vec<usize> = Vec::new();
    // (enclosing namespace, dotted path) → namespace, so `A.B` can reopen an implied group.
    let mut ns_paths: HashMap<(Option<usize>, String), usize> = HashMap::new();
    let hierarchical = config.flag("class.hierarchicalNamespaces", true);
    let mut class_defs: HashMap<String, FlowStyle> = HashMap::new();
    let mut styles: Vec<(String, FlowStyle)> = Vec::new();

    // Declares (or finds) a class; the innermost open namespace claims it the first time.
    let ensure_class = |raw: &str, classes: &mut Vec<ClassNode>, map: &mut HashMap<String, usize>, ns: Option<usize>| -> usize {
        let (key, label, css) = class_name_parts(raw);
        let idx = match map.get(&key) {
            Some(&idx) => idx,
            None => {
                let idx = classes.len();
                classes.push(ClassNode {
                    name: key.clone(),
                    label: None,
                    stereotype: None,
                    fields: Vec::new(),
                    methods: Vec::new(),
                    namespace: None,
                    classes: Vec::new(),
                    note: false,
                    interface: false,
                });
                map.insert(key, idx);
                idx
            }
        };
        if label.is_some() {
            classes[idx].label = label;
        }
        if let Some(css) = css {
            classes[idx].classes.push(css);
        }
        if classes[idx].namespace.is_none() {
            classes[idx].namespace = ns;
        }
        idx
    };
    let add_member = |class: &mut ClassNode, member: &str| {
        let member = class_generics(member.trim());
        if member.contains('(') && member.contains(')') {
            class.methods.push(member);
        } else {
            class.fields.push(member);
        }
    };
    let new_note = |text: &str, classes: &mut Vec<ClassNode>| -> usize {
        classes.push(ClassNode {
            name: unquote(text),
            label: None,
            stereotype: None,
            fields: Vec::new(),
            methods: Vec::new(),
            namespace: None,
            classes: Vec::new(),
            note: true,
            interface: false,
        });
        classes.len() - 1
    };

    for raw in source.lines().skip(1) {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if let Some(cidx) = current_class {
            if l == "}" {
                current_class = None;
                continue;
            }
            if l.starts_with("<<") && l.ends_with(">>") {
                classes[cidx].stereotype = Some(l.to_string());
            } else {
                add_member(&mut classes[cidx], l);
            }
            continue;
        }
        let ns = ns_stack.last().copied();
        if l == "}" {
            ns_stack.pop();
            continue;
        }
        if let Some(rest) = l.strip_prefix("namespace ") {
            // `namespace Id["Label"] {` shows the label.
            let (id, label, _) = class_name_parts(rest.trim().trim_end_matches('{'));
            let g = match label {
                Some(title) => {
                    namespaces.push(NodeGroup { title, parent: ns });
                    namespaces.len() - 1
                }
                // `A.B.C` nests inside `A.B` inside `A` (created when missing) unless
                // `class.hierarchicalNamespaces` is false.
                None if hierarchical && id.contains('.') => {
                    let mut parent = ns;
                    let mut path = String::new();
                    for segment in id.split('.') {
                        if !path.is_empty() {
                            path.push('.');
                        }
                        path.push_str(segment);
                        let key = (parent, path.clone());
                        parent = Some(match ns_paths.get(&key) {
                            Some(&g) => g,
                            None => {
                                namespaces.push(NodeGroup { title: segment.to_string(), parent });
                                ns_paths.insert(key, namespaces.len() - 1);
                                namespaces.len() - 1
                            }
                        });
                    }
                    parent.expect("a dotted namespace has a segment")
                }
                None => match ns_paths.get(&(ns, id.clone())) {
                    Some(&g) => g,
                    None => {
                        namespaces.push(NodeGroup { title: id.clone(), parent: ns });
                        ns_paths.insert((ns, id), namespaces.len() - 1);
                        namespaces.len() - 1
                    }
                },
            };
            ns_stack.push(g);
            continue;
        }
        if let Some(rest) = l.strip_prefix("direction ") {
            (horizontal, reversed) = match rest.trim().to_ascii_uppercase().as_str() {
                "LR" => (true, false),
                "RL" => (true, true),
                "BT" => (false, true),
                _ => (false, false),
            };
            continue;
        }
        if let Some(rest) = l.strip_prefix("note ") {
            let rest = rest.trim();
            if let Some(after_for) = rest.strip_prefix("for ") {
                let after_for = after_for.trim();
                let (target, text) = after_for.split_once(char::is_whitespace).unwrap_or((after_for, ""));
                let target = ensure_class(target, &mut classes, &mut map, ns);
                let note = new_note(text, &mut classes);
                edges.push(ClassEdge {
                    from: note,
                    to: target,
                    kind: ClassRelKind::Note,
                    dashed: true,
                    label: None,
                    from_card: None,
                    to_card: None,
                });
            } else {
                new_note(rest, &mut classes);
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("classDef ") {
            let rest = rest.trim();
            let (names, props) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
            let style = parse_flow_style(props);
            for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                class_defs.entry(name.to_string()).or_default().merge(&style);
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("style ") {
            if let Some((id, props)) = rest.trim().split_once(char::is_whitespace) {
                styles.push((id.to_string(), parse_flow_style(props)));
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("cssClass ") {
            if let Some((ids, css)) = rest.trim().rsplit_once(char::is_whitespace) {
                for id in unquote(ids).split(',').map(str::trim).filter(|id| !id.is_empty()) {
                    let idx = ensure_class(id, &mut classes, &mut map, ns);
                    classes[idx].classes.push(css.trim().to_string());
                }
            }
            continue;
        }
        if l.starts_with("click ") || l.starts_with("link ") || l.starts_with("callback ") || l.starts_with("accTitle") || l.starts_with("accDescr") {
            continue;
        }
        if l.starts_with("<<")
            && let Some(end) = l.find(">>")
        {
            let stereo = &l[..=end + 1];
            let cname = l[end + 2..].trim();
            if !cname.is_empty() {
                let idx = ensure_class(cname, &mut classes, &mut map, ns);
                classes[idx].stereotype = Some(stereo.to_string());
                continue;
            }
        }
        if let Some(rest) = l.strip_prefix("class ") {
            if let Some(before_brace) = rest.strip_suffix('{') {
                current_class = Some(ensure_class(before_brace, &mut classes, &mut map, ns));
                continue;
            }
            if !rest.trim().is_empty() {
                ensure_class(rest, &mut classes, &mut map, ns);
                continue;
            }
        }
        if split_class_relation(l, &mut classes, &mut map, &mut edges, &mut |raw, classes, map| ensure_class(raw, classes, map, ns)) {
            continue;
        }
        if let Some((cname, member)) = l.split_once(':') {
            let cn = cname.trim();
            let mb = member.trim();
            if !cn.is_empty() && !cn.contains(' ') && !mb.is_empty() {
                let idx = ensure_class(cn, &mut classes, &mut map, ns);
                add_member(&mut classes[idx], mb);
            }
        }
    }

    if classes.is_empty() {
        return None;
    }

    // classDef `default`, then the node's classes, then `style` statements.
    let mut node_style: Vec<FlowStyle> = classes
        .iter()
        .map(|c| {
            let mut style = class_defs.get("default").cloned().unwrap_or_default();
            for name in &c.classes {
                if let Some(def) = class_defs.get(name) {
                    style.merge(def);
                }
            }
            style
        })
        .collect();
    for (id, style) in &styles {
        if let Some(&idx) = map.get(id) {
            node_style[idx].merge(style);
        }
    }

    let note_lines = |c: &ClassNode| label_lines(&c.name, 200.0, 11.5);
    let node_sizes: Vec<(f32, f32)> = classes
        .iter()
        .map(|c| {
            if c.interface {
                return ((estimate_text_width(&c.name, 11.5) + 8.0).max(24.0), 38.0);
            }
            if c.note {
                let lines = note_lines(c);
                let w = lines.iter().map(|l| estimate_text_width(l, 11.5)).fold(0.0f32, f32::max) + 24.0;
                return (w.max(80.0), lines.len() as f32 * 15.0 + 16.0);
            }
            let title = c.label.as_deref().unwrap_or(&c.name);
            let mut max_w = estimate_text_width(title, 13.0) + 28.0;
            if let Some(st) = &c.stereotype {
                max_w = max_w.max(estimate_text_width(st, 10.5) + 24.0);
            }
            for f in &c.fields {
                max_w = max_w.max(estimate_text_width(f, 11.5) + 24.0);
            }
            for m in &c.methods {
                max_w = max_w.max(estimate_text_width(m, 11.5) + 24.0);
            }
            let header_h = if c.stereotype.is_some() { 40.0 } else { 28.0 };
            let fields_h = if c.fields.is_empty() { 0.0 } else { c.fields.len() as f32 * 18.0 + 10.0 };
            let methods_h = if c.methods.is_empty() { 0.0 } else { c.methods.len() as f32 * 18.0 + 10.0 };
            let min_h = if hide_empty { header_h } else { 36.0 };
            (max_w.max(110.0), (header_h + fields_h + methods_h).max(min_h))
        })
        .collect();

    let edge_pairs: Vec<(usize, usize)> = edges.iter().map(|e| (e.from, e.to)).collect();
    let member_of: Vec<Option<usize>> = classes.iter().map(|c| c.namespace).collect();
    let (centers, total_w, total_h, frames) =
        layout_with_groups(&node_sizes, &edge_pairs, &member_of, &namespaces, horizontal, reversed, 34.0, 70.0, 40.0);

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(
        r##"<defs>
  <marker id="cls-inherit" viewBox="0 0 12 12" refX="11" refY="6" markerWidth="10" markerHeight="10" orient="auto">
    <path d="M 1 1 L 11 6 L 1 11 z" fill="#ffffff" stroke="#334155" stroke-width="1.4"/>
  </marker>
  <marker id="cls-comp" viewBox="0 0 14 10" refX="13" refY="5" markerWidth="11" markerHeight="9" orient="auto">
    <path d="M 1 5 L 7 1 L 13 5 L 7 9 z" fill="#334155"/>
  </marker>
  <marker id="cls-agg" viewBox="0 0 14 10" refX="13" refY="5" markerWidth="11" markerHeight="9" orient="auto">
    <path d="M 1 5 L 7 1 L 13 5 L 7 9 z" fill="#ffffff" stroke="#334155" stroke-width="1.3"/>
  </marker>
  <marker id="cls-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto">
    <path d="M 1 1 L 9 5 L 1 9" fill="none" stroke="#334155" stroke-width="1.5"/>
  </marker>
</defs>"##,
    );
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));
    push_group_frames(&mut svg, &namespaces, &frames, "namespace");

    // Relationships
    for e in &edges {
        let route = route_layered_edge(e.from, e.to, &centers, &node_sizes, horizontal, false, (total_w as f32, total_h as f32));
        let dash = if e.dashed { r#" stroke-dasharray="5,4""# } else { "" };
        let marker = match e.kind {
            ClassRelKind::Inheritance | ClassRelKind::Realization => r#" marker-end="url(#cls-inherit)""#,
            ClassRelKind::Composition => r#" marker-end="url(#cls-comp)""#,
            ClassRelKind::Aggregation => r#" marker-end="url(#cls-agg)""#,
            ClassRelKind::Association | ClassRelKind::Dependency => r#" marker-end="url(#cls-arrow)""#,
            ClassRelKind::Link | ClassRelKind::Note => "",
        };
        let color = if e.kind == ClassRelKind::Note { "#ca8a04" } else { "#334155" };
        push_routed_edge(&mut svg, &route, color, "1.4", &format!("{dash}{marker}"));
        // Multiplicities sit just off each end, beside the line.
        for (card, (px, py), (dx, dy)) in [
            (&e.from_card, route.start, route.leave_dir),
            (&e.to_card, route.end, (-route.arrive_dir.0, -route.arrive_dir.1)),
        ] {
            if let Some(card) = card {
                let (tx, ty) = (px + dx * 18.0 - dy * 10.0, py + dy * 18.0 + dx * 10.0);
                svg.push_str(&format!(
                    r##"<text x="{tx:.1}" y="{:.1}" text-anchor="middle" font-size="10.5" fill="#0f766e" data-role="multiplicity">{}</text>"##,
                    ty + 4.0,
                    escape_xml(card)
                ));
            }
        }
        if let Some(lbl) = &e.label {
            let (mx, my) = route.label_at;
            let lw = estimate_text_width(lbl, 11.0) + 10.0;
            svg.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{lw:.1}" height="18" rx="3" fill="#ffffff" stroke="#e2e8f0" stroke-width="0.8"/>"##,
                mx - lw / 2.0,
                my - 9.0
            ));
            svg.push_str(&format!(
                r##"<text x="{mx:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#334155">{}</text>"##,
                my + 4.0,
                escape_xml(lbl)
            ));
        }
    }

    // Class boxes and notes
    for (idx, c) in classes.iter().enumerate() {
        let (cx, cy) = centers[idx];
        let (nw, nh) = node_sizes[idx];
        let x = cx - nw / 2.0;
        let y = cy - nh / 2.0;
        if c.interface {
            svg.push_str(&format!(
                r##"<circle cx="{cx:.1}" cy="{:.1}" r="7" fill="#ffffff" stroke="#334155" stroke-width="1.4" data-shape="interface"/>"##,
                y + 8.0
            ));
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="11.5" fill="#0f172a">{}</text>"##,
                y + 32.0,
                escape_xml(&c.name)
            ));
            continue;
        }
        if c.note {
            svg.push_str(&format!(
                r##"<path d="M {x:.1} {y:.1} L {:.1} {y:.1} L {:.1} {:.1} L {:.1} {:.1} L {x:.1} {:.1} Z" fill="#fef9c3" stroke="#ca8a04" stroke-width="1.2" data-shape="class-note"/>"##,
                x + nw - 10.0,
                x + nw,
                y + 10.0,
                x + nw,
                y + nh,
                y + nh
            ));
            for (i, line) in note_lines(c).iter().enumerate() {
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{:.1}" font-size="11.5" fill="#713f12">{}</text>"##,
                    x + 12.0,
                    y + 20.0 + i as f32 * 15.0,
                    escape_xml(line)
                ));
            }
            continue;
        }
        let style = &node_style[idx];
        let header_h = if c.stereotype.is_some() { 40.0 } else { 28.0 };
        let shape_at = svg.len();
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{nw:.1}" height="{nh:.1}" rx="4" fill="#ffffff" stroke="#334155" stroke-width="1.4" data-kind="class"/>"##
        ));
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{nw:.1}" height="{header_h:.1}" rx="4" fill="#f1f5f9" stroke="#334155" stroke-width="1.4"/>"##
        ));
        if !style.is_empty() {
            let shapes = svg.split_off(shape_at);
            svg.push_str(&restyle_svg_fragment(&shapes, style));
        }
        let ink = style.color.as_deref().map(escape_attr);
        let title_fill = ink.clone().unwrap_or_else(|| "#0f172a".to_string());
        let member_fill = ink.unwrap_or_else(|| "#1e293b".to_string());
        let title = c.label.as_deref().unwrap_or(&c.name);
        if let Some(st) = &c.stereotype {
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="10.5" fill="#475569">{}</text>"##,
                y + 15.0,
                escape_xml(st)
            ));
        }
        svg.push_str(&format!(
            r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="13" font-weight="bold" fill="{title_fill}">{}</text>"##,
            y + if c.stereotype.is_some() { 32.0 } else { 19.0 },
            escape_xml(title)
        ));

        let mut cur_y = y + header_h;
        if !c.fields.is_empty() {
            for (i, f) in c.fields.iter().enumerate() {
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{:.1}" font-size="11.5" fill="{member_fill}">{}</text>"##,
                    x + 10.0,
                    cur_y + 16.0 + i as f32 * 18.0,
                    escape_xml(f)
                ));
            }
            cur_y += c.fields.len() as f32 * 18.0 + 10.0;
        }
        if !c.methods.is_empty() {
            if !c.fields.is_empty() {
                svg.push_str(&format!(
                    r##"<line x1="{x:.1}" y1="{cur_y:.1}" x2="{:.1}" y2="{cur_y:.1}" stroke="#cbd5e1" stroke-width="1"/>"##,
                    x + nw
                ));
            }
            for (i, m) in c.methods.iter().enumerate() {
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{:.1}" font-size="11.5" fill="{member_fill}">{}</text>"##,
                    x + 10.0,
                    cur_y + 16.0 + i as f32 * 18.0,
                    escape_xml(m)
                ));
            }
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

/// `"1"` at the end of `Customer "1"` / start of `"*" Order`, and the class name.
pub(super) fn split_class_card(raw: &str, card_first: bool) -> (&str, Option<String>) {
    let raw = raw.trim();
    if card_first {
        if let Some(rest) = raw.strip_prefix('"')
            && let Some(end) = rest.find('"')
        {
            return (rest[end + 1..].trim(), Some(rest[..end].to_string()));
        }
    } else if let Some(body) = raw.strip_suffix('"')
        && let Some(start) = body.rfind('"')
    {
        return (body[..start].trim(), Some(body[start + 1..].to_string()));
    }
    (raw, None)
}

pub(super) fn split_class_relation(
    line: &str,
    classes: &mut Vec<ClassNode>,
    map: &mut HashMap<String, usize>,
    edges: &mut Vec<ClassEdge>,
    ensure_class: &mut impl FnMut(&str, &mut Vec<ClassNode>, &mut HashMap<String, usize>) -> usize,
) -> bool {
    // Operators ordered longest-first so `<|..` matches before `..`.
    // Tuple: (token, target_is_lhs, kind, dashed)
    const OPS: &[(&str, bool, ClassRelKind, bool)] = &[
        ("<|--", true, ClassRelKind::Inheritance, false),
        ("--|>", false, ClassRelKind::Inheritance, false),
        ("<|..", true, ClassRelKind::Realization, true),
        ("..|>", false, ClassRelKind::Realization, true),
        ("*--", true, ClassRelKind::Composition, false),
        ("--*", false, ClassRelKind::Composition, false),
        ("o--", true, ClassRelKind::Aggregation, false),
        ("--o", false, ClassRelKind::Aggregation, false),
        ("<--", true, ClassRelKind::Association, false),
        ("-->", false, ClassRelKind::Association, false),
        ("<..", true, ClassRelKind::Dependency, true),
        ("..>", false, ClassRelKind::Dependency, true),
        ("--", false, ClassRelKind::Link, false),
        ("..", false, ClassRelKind::Link, true),
    ];

    // The label follows the last `:` outside quotes; `::` style members never reach here.
    let (body, label) = match find_unquoted(line, ':') {
        Some(at) if !line[at..].starts_with(":::") => {
            (line[..at].trim(), Some(line[at + 1..].trim().to_string()).filter(|s| !s.is_empty()))
        }
        _ => (line.trim(), None),
    };

    // Lollipop interfaces: `bar ()-- foo` / `foo --() bar`. Each one is its own small node.
    for (op, iface_lhs) in [("()--", true), ("--()", false)] {
        let Some(pos) = body.find(op) else { continue };
        let (lhs, rhs) = (body[..pos].trim(), body[pos + op.len()..].trim());
        let (iface, class) = if iface_lhs { (lhs, rhs) } else { (rhs, lhs) };
        let iface = unquote(iface);
        if iface.is_empty() || class.is_empty() || class.contains(char::is_whitespace) {
            return false;
        }
        let class_idx = ensure_class(class, classes, map);
        classes.push(ClassNode {
            name: iface,
            label: None,
            stereotype: None,
            fields: Vec::new(),
            methods: Vec::new(),
            namespace: classes[class_idx].namespace,
            classes: Vec::new(),
            note: false,
            interface: true,
        });
        edges.push(ClassEdge {
            from: class_idx,
            to: classes.len() - 1,
            kind: ClassRelKind::Link,
            dashed: false,
            label,
            from_card: None,
            to_card: None,
        });
        return true;
    }

    for &(op, target_is_lhs, kind, dashed) in OPS {
        let Some(pos) = body.find(op) else { continue };
        let (lhs_raw, lhs_card) = split_class_card(&body[..pos], false);
        let (rhs_raw, rhs_card) = split_class_card(&body[pos + op.len()..], true);
        let lhs_name = lhs_raw.split_whitespace().next().unwrap_or("");
        let rhs_name = rhs_raw.split_whitespace().last().unwrap_or("");
        if lhs_name.is_empty() || rhs_name.is_empty() {
            return false;
        }
        let lhs_idx = ensure_class(lhs_name, classes, map);
        let rhs_idx = ensure_class(rhs_name, classes, map);
        let (from, to, from_card, to_card) = if target_is_lhs {
            (rhs_idx, lhs_idx, rhs_card, lhs_card)
        } else {
            (lhs_idx, rhs_idx, lhs_card, rhs_card)
        };
        edges.push(ClassEdge {
            from,
            to,
            kind,
            dashed,
            label,
            from_card,
            to_card,
        });
        return true;
    }
    false
}

/// `~T~` generics as `<T>`.
pub(super) fn class_generics(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut open = false;
    for ch in s.chars() {
        if ch == '~' {
            out.push(if open { '>' } else { '<' });
            open = !open;
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn renders_class_diagram_svg() {
        let src = "\
classDiagram
  class Animal {
    <<abstract>>
    +String name
    +speak() void
  }
  class Dog {
    +fetch() void
  }
  Animal <|-- Dog : inherits
";
        let Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        }) = render_mermaid(src)
        else {
            panic!("expected svg for classDiagram");
        };
        assert!(width_px > 120 && height_px > 120);
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(svg.contains(r#"data-kind="class""#), "{svg}");
        assert!(svg.contains("Animal"));
        assert!(svg.contains("Dog"));
        assert!(svg.contains("speak() void"));
        assert!(svg.contains("inherits"));
    }

    #[test]
    fn class_namespaces_nest_and_frame_their_classes() {
        let frames = |svg: &str| -> Vec<(f32, f32, f32, f32)> {
            tag_with(svg, r#"data-role="namespace""#)
                .iter()
                .map(|t| (attr_f32(t, "x").unwrap(), attr_f32(t, "y").unwrap(), attr_f32(t, "width").unwrap(), attr_f32(t, "height").unwrap()))
                .collect()
        };
        let svg = svg_of(
            "classDiagram\n  namespace Platform {\n    class Gateway\n    namespace Auth {\n      class UserService\n    }\n  }\n  class Loose\n  Gateway --> UserService\n",
        );
        let f = frames(&svg);
        assert_eq!(f.len(), 2, "{svg}");
        let inside = |(x, y, w, h): (f32, f32, f32, f32), label: &str| {
            let (tx, ty) = (text_x(&svg, label), text_y(&svg, label));
            tx > x && tx < x + w && ty > y && ty < y + h
        };
        // Outermost first: Platform holds both classes, Auth only UserService.
        assert!(inside(f[0], "Gateway") && inside(f[0], "UserService") && !inside(f[0], "Loose"), "{svg}");
        assert!(inside(f[1], "UserService") && !inside(f[1], "Gateway"), "{svg}");
        assert!(svg.contains(">Platform<") && svg.contains(">Auth<"));

        // `namespace Id["Label"]` shows the label.
        assert!(svg_of("classDiagram\n  namespace Auth[\"Authentication\"] {\n    class A\n  }\n").contains(">Authentication<"));

        // Dotted names nest (`Company` > `Engineering` > `Backend`) unless hierarchicalNamespaces is off.
        let dotted = "classDiagram\n  namespace Company.Engineering.Backend {\n    class Dev\n  }\n  namespace Company.Engineering {\n    class Lead\n  }\n  Lead --> Dev\n";
        let nested = svg_of(dotted);
        assert_eq!(frames(&nested).len(), 3, "{nested}");
        assert!(nested.contains(">Backend<") && nested.contains(">Company<"), "{nested}");
        let flat = svg_of(&format!("---\nconfig:\n  class:\n    hierarchicalNamespaces: false\n---\n{dotted}"));
        assert_eq!(frames(&flat).len(), 2, "{flat}");
        assert!(flat.contains(">Company.Engineering.Backend<"), "{flat}");
    }

    #[test]
    fn class_notes_cardinality_generics_and_styles() {
        assert_eq!(split_class_card(r#"Customer "1""#, false), ("Customer", Some("1".to_string())));
        assert_eq!(split_class_card(r#""many" Star"#, true), ("Star", Some("many".to_string())));
        assert_eq!(split_class_card("Star", true), ("Star", None));

        let svg = svg_of(
            "classDiagram\n  note \"General note\"\n  note for Duck \"Can fly\"\n  Customer \"1\" --> \"*\" Order : places\n  class Square~Shape~\n  Duck : +List~int~ eggs\n",
        );
        assert_eq!(tag_with(&svg, r#"data-shape="class-note""#).len(), 2, "{svg}");
        assert!(svg.contains(">General note<") && svg.contains(">Can fly<"), "{svg}");
        let cards = tag_with(&svg, r#"data-role="multiplicity""#);
        assert_eq!(cards.len(), 2, "{svg}");
        // Each multiplicity sits at its own end.
        let dist = |a: &str, b: &str| (text_x(&svg, a) - text_x(&svg, b)).hypot(text_y(&svg, a) - text_y(&svg, b));
        assert!(dist("*", "Order") < dist("*", "Customer"), "{svg}");
        assert!(dist("1", "Customer") < dist("1", "Order"), "{svg}");
        assert!(svg.contains(">Square&lt;Shape&gt;<") && svg.contains(">+List&lt;int&gt; eggs<"), "{svg}");

        let styled = svg_of(
            "classDiagram\n  class Animal:::pink\n  class Mineral\n  class Rock\n  classDef default fill:#f96,color:red\n  classDef pink color:#f9f\n  style Rock fill:#bbf\n",
        );
        assert!(styled.contains(r##"fill="#f9f">Animal<"##) && styled.contains(r##"fill="red">Mineral<"##), "{styled}");
        assert!(!painted_rects(&styled, "#f96").is_empty() && !painted_rects(&styled, "#bbf").is_empty(), "{styled}");
    }

    #[test]
    fn class_lollipop_interfaces() {
        let svg = svg_of("classDiagram\n  bar ()-- foo\n  Class01 --() bar\n  Class02 --() bar\n");
        // One interface per lollipop, even with the same name.
        assert_eq!(svg.matches(r#"data-shape="interface""#).count(), 3, "{svg}");
        assert_eq!(svg.matches(">bar</text>").count(), 3);
        for class in ["foo", "Class01", "Class02"] {
            assert!(svg.contains(&format!(">{class}</text>")), "{class}");
        }
        assert!(!svg.contains("()"), "{svg}");
    }
}
