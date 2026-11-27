// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `erDiagram`.

use super::*;

pub(super) struct ErAttr {
    pub(super) ty: String,
    pub(super) name: String,
    pub(super) key: String,
    pub(super) comment: String,
}

pub(super) struct ErEntity {
    pub(super) name: String,
    /// Display name for `id[Label]` / `id["Label"]`.
    pub(super) label: Option<String>,
    pub(super) attrs: Vec<ErAttr>,
    pub(super) group: Option<usize>,
    pub(super) classes: Vec<String>,
}

/// An end of a relationship: an entity or a whole `subgraph`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ErEnd {
    Entity(usize),
    Group(usize),
}

pub(super) struct ErEdge {
    pub(super) from: ErEnd,
    pub(super) to: ErEnd,
    pub(super) left_card: String,
    pub(super) right_card: String,
    pub(super) dashed: bool,
    pub(super) label: String,
}

/// Splits `REL : label` at the first lone `:` outside quotes (`A:::cls` is not a label).
pub(super) fn er_label_split(line: &str) -> (&str, Option<&str>) {
    let bytes = line.as_bytes();
    let mut in_quote = false;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => in_quote = !in_quote,
            b':' if !in_quote => {
                let mut j = i;
                while j < bytes.len() && bytes[j] == b':' {
                    j += 1;
                }
                if j - i == 1 {
                    return (line[..i].trim(), Some(line[i + 1..].trim()));
                }
                i = j;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    (line.trim(), None)
}

pub(super) fn render_er_diagram_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let mut entities: Vec<ErEntity> = Vec::new();
    let mut map: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<ErEdge> = Vec::new();
    let mut current_entity: Option<usize> = None;
    let mut groups: Vec<NodeGroup> = Vec::new();
    let mut group_ids: HashMap<String, usize> = HashMap::new();
    let mut group_stack: Vec<usize> = Vec::new();
    let mut horizontal = true;
    let mut reversed = false;
    let mut class_defs: HashMap<String, FlowStyle> = HashMap::new();
    let mut styles: Vec<(String, FlowStyle)> = Vec::new();

    let ensure_entity = |raw: &str, entities: &mut Vec<ErEntity>, map: &mut HashMap<String, usize>, group: Option<usize>| -> usize {
        let (key, label, css) = class_name_parts(raw);
        let key = unquote(&key);
        let idx = match map.get(&key) {
            Some(&idx) => idx,
            None => {
                entities.push(ErEntity {
                    name: key.clone(),
                    label: None,
                    attrs: Vec::new(),
                    group: None,
                    classes: Vec::new(),
                });
                map.insert(key, entities.len() - 1);
                entities.len() - 1
            }
        };
        if label.is_some() {
            entities[idx].label = label;
        }
        if let Some(css) = css {
            entities[idx].classes.push(css);
        }
        if entities[idx].group.is_none() {
            entities[idx].group = group;
        }
        idx
    };

    for raw in source.lines().skip(1) {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if let Some(eidx) = current_entity {
            if l == "}" {
                current_entity = None;
                continue;
            }
            let parts: Vec<&str> = l.split_whitespace().collect();
            if parts.len() >= 2 {
                let ty = parts[0].to_string();
                let name = parts[1].to_string();
                // Keys: `PK`, `FK`, `UK`, or a list like `PK, FK`; a comment is quoted.
                let key = parts[2..]
                    .iter()
                    .take_while(|s| !s.starts_with('"'))
                    .copied()
                    .collect::<Vec<_>>()
                    .join(" ");
                let comment = if let Some(at) = l.find('"') {
                    unquote(&l[at..])
                } else {
                    String::new()
                };
                entities[eidx].attrs.push(ErAttr { ty, name, key, comment });
            }
            continue;
        }
        let group = group_stack.last().copied();
        if l == "end" {
            group_stack.pop();
            continue;
        }
        if let Some(rest) = l.strip_prefix("subgraph ") {
            let rest = rest.trim();
            let (id, title) = match rest.find('[') {
                Some(open) if rest.ends_with(']') => (rest[..open].trim().to_string(), unquote(&rest[open + 1..rest.len() - 1])),
                _ => (unquote(rest), unquote(rest)),
            };
            groups.push(NodeGroup { title, parent: group });
            group_ids.insert(id, groups.len() - 1);
            group_stack.push(groups.len() - 1);
            continue;
        }
        if let Some(rest) = l.strip_prefix("direction ") {
            // Only the diagram-level direction; nested ones are not modelled.
            if group.is_none() {
                (horizontal, reversed) = match rest.trim().to_ascii_uppercase().as_str() {
                    "TB" | "TD" => (false, false),
                    "BT" => (false, true),
                    "RL" => (true, true),
                    _ => (true, false),
                };
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
        if let Some(rest) = l.strip_prefix("class ") {
            if let Some((ids, css)) = rest.trim().rsplit_once(char::is_whitespace) {
                for id in ids.split(',').map(str::trim).filter(|id| !id.is_empty()) {
                    let idx = ensure_entity(id, &mut entities, &mut map, group);
                    entities[idx].classes.push(css.trim().to_string());
                }
            }
            continue;
        }
        if l.starts_with("title ") || l.starts_with("accTitle") || l.starts_with("accDescr") {
            continue;
        }
        if let Some(before_brace) = l.strip_suffix('{') {
            let ename = before_brace.trim();
            if !ename.is_empty() && !ename.contains("--") && !ename.contains("..") {
                current_entity = Some(ensure_entity(ename, &mut entities, &mut map, group));
                continue;
            }
        }
        let (rel_part, label_raw) = er_label_split(l);
        if let Some((lhs, left_card, dashed, right_card, rhs)) = parse_er_relation(rel_part) {
            let end = |name: &str, entities: &mut Vec<ErEntity>, map: &mut HashMap<String, usize>| match group_ids.get(name) {
                Some(&g) if !map.contains_key(name) => ErEnd::Group(g),
                _ => ErEnd::Entity(ensure_entity(name, entities, map, group)),
            };
            let from = end(lhs, &mut entities, &mut map);
            let to = end(rhs, &mut entities, &mut map);
            edges.push(ErEdge {
                from,
                to,
                left_card,
                right_card,
                dashed,
                label: label_raw.map(unquote).unwrap_or_default(),
            });
            continue;
        }
        // A lone entity: `CUSTOMER`, `"Quoted name"`, `HOUSE:::cls`, `p[Person]`.
        if label_raw.is_none() && (l.starts_with('"') || !l.contains(char::is_whitespace) || l.contains('[')) {
            ensure_entity(l, &mut entities, &mut map, group);
        }
    }

    if entities.is_empty() {
        return None;
    }

    let mut node_style: Vec<FlowStyle> = entities
        .iter()
        .map(|e| {
            let mut style = class_defs.get("default").cloned().unwrap_or_default();
            for name in &e.classes {
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

    let mut node_sizes: Vec<(f32, f32)> = entities
        .iter()
        .map(|e| {
            let mut max_w = estimate_text_width(e.label.as_deref().unwrap_or(&e.name), 13.0) + 32.0;
            for a in &e.attrs {
                let row = if a.key.is_empty() {
                    format!("{}  {}", a.ty, a.name)
                } else {
                    format!("{}  {}  {}", a.ty, a.name, a.key)
                };
                max_w = max_w.max(estimate_text_width(&row, 11.5) + 28.0);
                if !a.comment.is_empty() {
                    max_w = max_w.max(estimate_text_width(&a.comment, 10.0) + 28.0);
                }
            }
            let h = 30.0
                + if e.attrs.is_empty() {
                    8.0
                } else {
                    e.attrs.iter().map(|a| if a.comment.is_empty() { 20.0 } else { 32.0 }).sum::<f32>() + 10.0
                };
            (max_w.max(120.0), h)
        })
        .collect();

    let member_of: Vec<Option<usize>> = entities.iter().map(|e| e.group).collect();
    // A subgraph end lays out next to the subgraph's first member.
    let group_anchor = |g: usize| -> Option<usize> {
        (0..entities.len()).find(|&i| {
            let mut cur = member_of[i];
            while let Some(c) = cur {
                if c == g {
                    return true;
                }
                cur = groups[c].parent;
            }
            false
        })
    };
    let layout_end = |end: ErEnd| match end {
        ErEnd::Entity(i) => Some(i),
        ErEnd::Group(g) => group_anchor(g),
    };
    let edge_pairs: Vec<(usize, usize)> = edges
        .iter()
        .filter_map(|e| Some((layout_end(e.from)?, layout_end(e.to)?)))
        .filter(|(a, b)| a != b)
        .collect();
    let (mut centers, total_w, total_h, frames) =
        layout_with_groups(&node_sizes, &edge_pairs, &member_of, &groups, horizontal, reversed, 36.0, 84.0, 40.0);

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));
    push_group_frames(&mut svg, &groups, &frames, "subgraph");

    // Subgraph ends route to the frame: append each frame as a virtual node.
    let mut group_node: HashMap<usize, usize> = HashMap::new();
    for (g, frame) in frames.iter().enumerate() {
        if let Some((fx, fy, fw, fh)) = *frame {
            group_node.insert(g, centers.len());
            centers.push((fx + fw / 2.0, fy + fh / 2.0));
            node_sizes.push((fw, fh));
        }
    }
    let route_end = |end: ErEnd| match end {
        ErEnd::Entity(i) => Some(i),
        ErEnd::Group(g) => group_node.get(&g).copied(),
    };

    // Edges
    for e in &edges {
        let (Some(from), Some(to)) = (route_end(e.from), route_end(e.to)) else { continue };
        if from == to {
            continue;
        }
        let route = route_layered_edge(from, to, &centers, &node_sizes, horizontal, false, (total_w as f32, total_h as f32));
        let dash = if e.dashed { r#" stroke-dasharray="5,4""# } else { "" };
        push_routed_edge(&mut svg, &route, "#475569", "1.5", dash);
        let (sx, sy) = route.start;
        let (ex, ey) = route.end;
        let (ux, uy) = route.leave_dir;
        let (vx, vy) = route.arrive_dir;
        // Cards sit beside the line: above it when horizontal, to its right when vertical.
        let (ox, oy) = if horizontal { (0.0, -5.0) } else { (12.0, 4.0) };
        for (card, x, y) in [
            (&e.left_card, sx + ux * 16.0 + ox, sy + uy * 16.0 + oy),
            (&e.right_card, ex - vx * 16.0 + ox, ey - vy * 16.0 + oy),
        ] {
            svg.push_str(&format!(
                r##"<text x="{x:.1}" y="{y:.1}" text-anchor="middle" font-size="10.5" font-weight="bold" fill="#0f766e" data-role="cardinality">{}</text>"##,
                escape_xml(card)
            ));
        }
        if !e.label.is_empty() {
            let (mx, my) = route.label_at;
            let lw = estimate_text_width(&e.label, 11.0) + 10.0;
            svg.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{lw:.1}" height="18" rx="3" fill="#ffffff" stroke="#e2e8f0" stroke-width="0.8"/>"##,
                mx - lw / 2.0,
                my - 9.0
            ));
            svg.push_str(&format!(
                r##"<text x="{mx:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#334155">{}</text>"##,
                my + 4.0,
                escape_xml(&e.label)
            ));
        }
    }

    // Entity boxes
    for (idx, ent) in entities.iter().enumerate() {
        let (cx, cy) = centers[idx];
        let (nw, nh) = node_sizes[idx];
        let x = cx - nw / 2.0;
        let y = cy - nh / 2.0;
        let style = &node_style[idx];
        let shape_at = svg.len();
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{nw:.1}" height="{nh:.1}" rx="4" fill="#ffffff" stroke="#0f766e" stroke-width="1.4" data-kind="entity"/>"##
        ));
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{nw:.1}" height="28" rx="4" fill="#f0fdfa" stroke="#0f766e" stroke-width="1.4"/>"##
        ));
        if !style.is_empty() {
            let shapes = svg.split_off(shape_at);
            svg.push_str(&restyle_svg_fragment(&shapes, style));
        }
        let ink = style.color.as_deref().map(escape_attr);
        svg.push_str(&format!(
            r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="13" font-weight="bold" fill="{}">{}</text>"##,
            y + 19.0,
            ink.clone().unwrap_or_else(|| "#134e4a".to_string()),
            // Entity names may use Markdown emphasis (`"This **is** _Markdown_"`).
            label_markup(ent.label.as_deref().unwrap_or(&ent.name), true)
        ));
        let mut ay = y + 46.0;
        for a in &ent.attrs {
            let row = if a.key.is_empty() {
                format!("{}  {}", a.ty, a.name)
            } else {
                // mermaid.js shows `PK` / `UK` as their own short labels, not `[PK]`.
                format!("{}  {}  {}", a.ty, a.name, a.key)
            };
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{ay:.1}" font-size="11.5" fill="{}">{}</text>"##,
                x + 10.0,
                ink.clone().unwrap_or_else(|| "#1e293b".to_string()),
                escape_xml(&row)
            ));
            if !a.comment.is_empty() {
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{:.1}" font-size="10" font-style="italic" fill="{}">{}</text>"##,
                    x + 10.0,
                    ay + 12.0,
                    ink.clone().unwrap_or_else(|| "#64748b".to_string()),
                    escape_xml(&a.comment)
                ));
                ay += 32.0;
            } else {
                ay += 20.0;
            }
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

pub(super) const ER_CARD_CHARS: &[char] = &['|', 'o', '}', '{'];

/// `A ||--o{ B`, `A||--||B`, `A only one to zero or more B` (`optionally to` is dashed) →
/// (lhs, left card, dashed, right card, rhs) with the cards as `1`, `0..1`, `1..*`, `0..*`.
pub(super) fn parse_er_relation(s: &str) -> Option<(&str, String, bool, String, &str)> {
    let symbolic = s.find("--").map(|i| (i, false)).or_else(|| s.find("..").map(|i| (i, true)));
    if let Some((sep, dashed)) = symbolic {
        let lhs_all = s[..sep].trim_end();
        let rhs_all = s[sep + 2..].trim_start();
        let (lhs, left_card) = match lhs_all.rfind(' ') {
            Some(at) => (lhs_all[..at].trim(), &lhs_all[at + 1..]),
            None if lhs_all.len() > 2 && lhs_all.is_char_boundary(lhs_all.len() - 2) => {
                let at = lhs_all.len() - 2;
                (&lhs_all[..at], &lhs_all[at..])
            }
            None => return None,
        };
        let (right_card, rhs) = match rhs_all.find(' ') {
            Some(at) => (&rhs_all[..at], rhs_all[at + 1..].trim()),
            None if rhs_all.len() > 2 && rhs_all.is_char_boundary(2) => (&rhs_all[..2], &rhs_all[2..]),
            None => return None,
        };
        if lhs.is_empty() || rhs.is_empty() || !left_card.chars().all(|c| ER_CARD_CHARS.contains(&c)) || !right_card.chars().all(|c| ER_CARD_CHARS.contains(&c)) {
            return None;
        }
        return Some((lhs, format_er_card(left_card), dashed, format_er_card(right_card), rhs));
    }
    let (sep, len, dashed) = match s.find(" optionally to ") {
        Some(i) => (i, " optionally to ".len(), true),
        None => (s.find(" to ")?, " to ".len(), false),
    };
    let (lhs, left_words) = er_word_end(s[..sep].trim(), true)?;
    let (rhs, right_words) = er_word_end(s[sep + len..].trim(), false)?;
    Some((lhs, er_word_card(left_words)?, dashed, er_word_card(right_words)?, rhs))
}

/// Splits `NAME card words` (entity first) or `card words NAME` (entity last).
pub(super) fn er_word_end(s: &str, entity_first: bool) -> Option<(&str, &str)> {
    let (name, words) = if entity_first {
        if let Some(rest) = s.strip_prefix('"') {
            let end = rest.find('"')? + 2;
            (&s[..end], s[end..].trim())
        } else {
            let (name, words) = s.split_once(char::is_whitespace)?;
            (name, words.trim())
        }
    } else if s.ends_with('"') {
        let start = s[..s.len() - 1].rfind('"')?;
        (&s[start..], s[..start].trim())
    } else {
        let (words, name) = s.rsplit_once(char::is_whitespace)?;
        (name, words.trim())
    };
    (!name.is_empty() && !words.is_empty()).then_some((name, words))
}

pub(super) fn er_word_card(words: &str) -> Option<String> {
    let card = match words.to_ascii_lowercase().as_str() {
        "1" | "one" | "only one" | "exactly one" => "1",
        "zero or one" | "one or zero" => "0..1",
        "one or more" | "one or many" | "many(1)" | "1+" => "1..*",
        "zero or more" | "zero or many" | "many(0)" | "0+" => "0..*",
        _ => return None,
    };
    Some(card.to_string())
}

pub(super) fn format_er_card(card: &str) -> String {
    match card {
        "||" => "1".to_string(),
        "o|" | "|o" => "0..1".to_string(),
        "}|" | "|{" => "1..*".to_string(),
        "}o" | "o{" => "0..*".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn renders_er_diagram_svg() {
        let src = "\
erDiagram
  CUSTOMER {
    int id PK
    string name
  }
  ORDER {
    int id PK
    int customer_id FK
  }
  CUSTOMER ||--o{ ORDER : places
";
        let Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        }) = render_mermaid(src)
        else {
            panic!("expected svg for erDiagram");
        };
        assert!(width_px > 200 && height_px > 100);
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(svg.contains(r#"data-kind="entity""#), "{svg}");
        assert!(svg.contains("CUSTOMER"));
        assert!(svg.contains("ORDER"));
        assert!(svg.contains("places"));
        assert!(svg.contains("0..*"));
    }

    #[test]
    fn er_relations_symbolic_word_and_unspaced() {
        let rel = |s| parse_er_relation(s).map(|(l, lc, d, rc, r)| (l.to_string(), lc, d, rc, r.to_string()));
        let t = |l: &str, lc: &str, d, rc: &str, r: &str| Some((l.to_string(), lc.to_string(), d, rc.to_string(), r.to_string()));
        assert_eq!(rel("A ||--o{ B"), t("A", "1", false, "0..*", "B"));
        assert_eq!(rel("id1||--||id2"), t("id1", "1", false, "1", "id2"));
        assert_eq!(rel("PERSON o{--|| HOUSE:::bar"), t("PERSON", "0..*", false, "1", "HOUSE:::bar"));
        assert_eq!(rel("A }|..|{ B"), t("A", "1..*", true, "1..*", "B"));
        assert_eq!(rel("CAR 1 to zero or more NAMED-DRIVER"), t("CAR", "1", false, "0..*", "NAMED-DRIVER"));
        assert_eq!(rel("PERSON many(0) optionally to 0+ NAMED-DRIVER"), t("PERSON", "0..*", true, "0..*", "NAMED-DRIVER"));
        assert_eq!(rel("MANUFACTURER only one to one or more CAR"), t("MANUFACTURER", "1", false, "1..*", "CAR"));
        assert_eq!(rel("A to B"), None);
        assert_eq!(rel("A banana to zero or more B"), None);
        assert_eq!(er_label_split("PERSON:::foo ||--|| CAR : \"owns\""), ("PERSON:::foo ||--|| CAR", Some("\"owns\"")));
        assert_eq!(er_label_split("HOUSE:::bar"), ("HOUSE:::bar", None));

        let svg = svg_of("erDiagram\n  CAR 1 to zero or more NAMED-DRIVER : allows\n  id1||--||id2 : label\n");
        for name in ["CAR", "NAMED-DRIVER", "id1", "id2", "allows", "label", "0..*"] {
            assert!(svg.contains(&format!(">{name}<")), "{name}: {svg}");
        }
    }

    #[test]
    fn er_entities_aliases_direction_styles_and_subgraphs() {
        // Entity-only lines render on their own.
        let lone = svg_of("erDiagram\n  \"This is quoted\"\n  CUSTOMER\n  HOUSE:::c\n");
        assert!(lone.contains(">This is quoted<") && lone.contains(">CUSTOMER<") && lone.contains(">HOUSE<"), "{lone}");
        let alias = svg_of("erDiagram\n  p[Person] {\n    string name\n  }\n  a[\"Customer Account\"]\n  p ||--o| a : has\n");
        assert!(alias.contains(">Person<") && alias.contains(">Customer Account<") && !alias.contains(">p<"), "{alias}");

        // LR by default, `direction TB` stacks the layers.
        let body = "  CUSTOMER ||--o{ ORDER : places\n";
        let lr = svg_of(&format!("erDiagram\n{body}"));
        assert!(text_x(&lr, "ORDER") > text_x(&lr, "CUSTOMER") + 50.0, "{lr}");
        let tb = svg_of(&format!("erDiagram\n  direction TB\n{body}"));
        assert!(text_y(&tb, "ORDER") > text_y(&tb, "CUSTOMER") + 30.0, "{tb}");
        assert!((text_x(&tb, "ORDER") - text_x(&tb, "CUSTOMER")).abs() < 1.0, "{tb}");

        let styled = svg_of(
            "erDiagram\n  CAR:::hot\n  PERSON\n  id1\n  classDef default stroke:#00f\n  classDef hot fill:#f96\n  style id1 fill:#bbf,color:#fff\n",
        );
        assert!(!painted_rects(&styled, "#f96").is_empty() && !painted_rects(&styled, "#bbf").is_empty(), "{styled}");
        assert!(styled.contains(r##"stroke="#00f""##) && styled.contains(r##"fill="#fff">id1<"##), "{styled}");

        // Nested subgraphs: siblings do not overlap, the parent holds both; a subgraph id as a
        // relationship end routes to its frame instead of becoming an entity.
        let svg = svg_of(
            "erDiagram\n  subgraph TOP\n    subgraph B1 [First]\n      I1 ||--|| F1 : links\n    end\n    subgraph \"B2\"\n      I2 ||--|| F2 : links\n    end\n  end\n  A ||--|| TOP : links\n  B1 ||--|| B2 : links\n",
        );
        let frames: Vec<(f32, f32, f32, f32)> = tag_with(&svg, r#"data-role="subgraph""#)
            .iter()
            .map(|t| (attr_f32(t, "x").unwrap(), attr_f32(t, "y").unwrap(), attr_f32(t, "width").unwrap(), attr_f32(t, "height").unwrap()))
            .collect();
        assert_eq!(frames.len(), 3, "{svg}");
        let (top, b1, b2) = (frames[0], frames[1], frames[2]);
        let within = |a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)| a.0 >= b.0 && a.1 >= b.1 && a.0 + a.2 <= b.0 + b.2 && a.1 + a.3 <= b.1 + b.3;
        let disjoint = |a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)| a.0 + a.2 <= b.0 || b.0 + b.2 <= a.0 || a.1 + a.3 <= b.1 || b.1 + b.3 <= a.1;
        assert!(within(b1, top) && within(b2, top) && disjoint(b1, b2), "{frames:?}");
        assert!(svg.contains(">First<"), "{svg}");
        assert_eq!(svg.matches(">TOP<").count(), 1, "TOP is a frame, not an entity: {svg}");
        assert_eq!(svg.matches(">B1<").count(), 0, "{svg}");
        assert!(text_x(&svg, "A") < top.0, "{svg}");
    }

    #[test]
    fn er_entity_names_with_markdown() {
        let svg = svg_of("erDiagram\n    \"This **is** _Markdown_\"\n    LINE_ITEM_ID ||--o{ ORDER : has\n");
        assert!(svg.contains(r#">This <tspan font-weight="bold">is</tspan> <tspan font-style="italic">Markdown</tspan></text>"#), "{svg}");
        assert!(svg.contains(">LINE_ITEM_ID</text>"), "underscores inside a word stay: {svg}");
    }

    #[test]
    fn er_attribute_keys_and_comments() {
        let svg = svg_of(
            "erDiagram
    PERSON {
        string driversLicense PK \"The license #\"
        string(99) firstName \"Only 99 characters are allowed\"
        string phone UK
        string lastName
    }
",
        );
        assert!(svg.contains(">string  driversLicense  PK<"), "{svg}");
        assert!(svg.contains(">string  phone  UK<"), "{svg}");
        assert!(svg.contains("The license #") && svg.contains("Only 99 characters are allowed"), "{svg}");
    }
}
