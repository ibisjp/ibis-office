// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `sequenceDiagram`.

use super::*;

#[derive(Debug, Clone)]
pub(super) struct SeqParticipant {
    pub(super) _id: String,
    pub(super) label: String,
    pub(super) is_actor: bool,
    /// `participant A@{ "type": "database" }`.
    pub(super) kind: Option<SeqKind>,
    /// Titles from `link` / `links` (URLs are not followed).
    pub(super) links: Vec<String>,
}

/// mermaid's participant types (`@{ "type": … }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SeqKind {
    Boundary,
    Control,
    Entity,
    Database,
    Collections,
    Queue,
}

impl SeqKind {
    pub(super) fn from_name(name: &str) -> Option<SeqKind> {
        match name.trim().to_ascii_lowercase().as_str() {
            "boundary" => Some(SeqKind::Boundary),
            "control" => Some(SeqKind::Control),
            "entity" => Some(SeqKind::Entity),
            "database" => Some(SeqKind::Database),
            "collections" => Some(SeqKind::Collections),
            "queue" => Some(SeqKind::Queue),
            _ => None,
        }
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            SeqKind::Boundary => "boundary",
            SeqKind::Control => "control",
            SeqKind::Entity => "entity",
            SeqKind::Database => "database",
            SeqKind::Collections => "collections",
            SeqKind::Queue => "queue",
        }
    }
}

/// `Id`, `Id as Label`, `Id@{ "type": "database", "alias": "Label" } [as Label]` → (id, label, type).
/// `as` wins over `alias`; `actor`/`participant` keywords are already stripped.
pub(super) fn seq_declaration(rest: &str) -> (&str, Option<String>, Option<SeqKind>) {
    if let Some(at) = rest.find("@{")
        && let Some(close) = find_unquoted(&rest[at..], '}')
    {
        let id = rest[..at].trim();
        let attrs = parse_brace_attrs(&rest[at + 2..at + close]);
        let after = rest[at + close + 1..].trim();
        let get = |key: &str| attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
        let kind = get("type").and_then(|t| SeqKind::from_name(&t));
        let label = after
            .strip_prefix("as ")
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .or_else(|| get("alias").filter(|a| !a.is_empty()));
        return (id, label, kind);
    }
    let (id, label) = split_alias(rest);
    (id, label.map(str::to_string), None)
}

pub(super) struct SeqBox {
    pub(super) title: String,
    pub(super) members: Vec<usize>,
}

pub(super) enum SeqOpen {
    Fragment,
    Box,
    /// `rect rgb(...)` background highlight.
    Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum SeqNoteShape {
    #[default]
    Rect,
    Hexagon,
    Box,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SeqAutonumberOp {
    Start { start: usize, step: usize },
    Stop,
    Resume,
}

#[derive(Debug, Clone)]
pub(super) enum SeqItem {
    Message {
        from: usize,
        to: usize,
        label: String,
        dashed: bool,
        open_arrow: bool,
        cross_end: bool,
        bidirectional: bool,
        circle_from: bool,
        circle_to: bool,
        activate_from: bool,
        deactivate_from: bool,
        activate_to: bool,
        deactivate_to: bool,
    },
    Note {
        from: usize,
        to: usize,
        pos: SeqNotePos,
        text: String,
        shape: SeqNoteShape,
        color: Option<String>,
    },
    BlockStart {
        kind: String,
        label: String,
        color: Option<String>,
    },
    BlockElse {
        label: String,
    },
    BlockEnd,
    Activate(usize),
    Deactivate(usize),
    /// `rect <color>` opens a highlighted background band.
    RectStart {
        color: String,
    },
    RectEnd,
    /// PlantUML `== label ==` section divider.
    Divider {
        label: String,
    },
    /// PlantUML `... [label] ...` delay break.
    Delay {
        label: String,
    },
    /// PlantUML `ref over A, B : label` interaction-use frame.
    Ref {
        from: usize,
        to: usize,
        label: String,
    },
    /// PlantUML incoming/outgoing boundary gate arrow (`[-> A`, `A ->]`).
    Gate {
        target: usize,
        incoming: bool,
        from_left: bool,
        label: String,
        dashed: bool,
        open_arrow: bool,
        cross_end: bool,
    },
    /// PlantUML `autonumber [start] [step]` / `stop` / `resume`.
    Autonumber(SeqAutonumberOp),
}

pub(super) struct SeqDiagram {
    pub(super) participants: Vec<SeqParticipant>,
    pub(super) items: Vec<SeqItem>,
    pub(super) autonumber: bool,
    pub(super) mirror_actors: bool,
    pub(super) boxes: Vec<SeqBox>,
    pub(super) title: Option<String>,
    pub(super) created_at: HashMap<usize, usize>,
    pub(super) destroyed_at: HashMap<usize, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SeqNotePos {
    Left,
    Right,
    Over,
}


fn push_seq_autonumber(svg: &mut String, x: f32, y: f32, num: usize) {
    svg.push_str(&format!(
        r##"<circle cx="{x:.1}" cy="{y:.1}" r="8" fill="#ffffff" stroke="#334155" stroke-width="1" data-role="seq-autonumber"/><text x="{x:.1}" y="{:.1}" text-anchor="middle" font-size="10" fill="#334155" data-role="seq-autonumber">{num}</text>"##,
        y + 3.5
    ));
}

pub(super) fn render_sequence_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let diagram = parse_mermaid_sequence(source)?;
    render_seq_diagram_svg(&diagram)
}

fn parse_mermaid_sequence(source: &str) -> Option<SeqDiagram> {
    let mut participants: Vec<SeqParticipant> = Vec::new();
    let mut part_map: HashMap<String, usize> = HashMap::new();
    let mut items: Vec<SeqItem> = Vec::new();
    let mut autonumber = false;
    let mut boxes: Vec<SeqBox> = Vec::new();
    let mut open: Vec<SeqOpen> = Vec::new();
    let mut title: Option<String> = None;
    // `create` / `destroy` wait for the next message that involves the participant.
    let mut pending_create: Vec<usize> = Vec::new();
    let mut pending_destroy: Vec<usize> = Vec::new();
    let mut created_at: HashMap<usize, usize> = HashMap::new();
    let mut destroyed_at: HashMap<usize, usize> = HashMap::new();

    let ensure_participant = |id: &str,
                              label: Option<&str>,
                              is_actor: bool,
                              participants: &mut Vec<SeqParticipant>,
                              part_map: &mut HashMap<String, usize>|
     -> usize {
        if let Some(&idx) = part_map.get(id) {
            if let Some(lbl) = label {
                participants[idx].label = lbl.to_string();
            }
            if is_actor {
                participants[idx].is_actor = true;
            }
            return idx;
        }
        let idx = participants.len();
        participants.push(SeqParticipant {
            _id: id.to_string(),
            label: label.unwrap_or(id).to_string(),
            is_actor,
            kind: None,
            links: Vec::new(),
        });
        part_map.insert(id.to_string(), idx);
        idx
    };

    for raw_line in source.lines().skip(1) {
        let l = raw_line.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if l == "autonumber" || l.starts_with("autonumber ") {
            autonumber = true;
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ").or_else(|| l.strip_prefix("title:")) {
            let t = unquote(rest.trim());
            title = (!t.is_empty()).then_some(t);
            continue;
        }
        if l.starts_with("accTitle") || l.starts_with("accDescr") {
            continue;
        }
        if let Some(rest) = l.strip_prefix("link ") {
            if let Some((who, rest)) = rest.split_once(':') {
                let title = rest.split_once(" @ ").or_else(|| rest.split_once('@')).map(|(t, _)| t).unwrap_or(rest);
                let idx = ensure_participant(who.trim(), None, false, &mut participants, &mut part_map);
                let title = unquote(title.trim());
                if !title.is_empty() {
                    participants[idx].links.push(title);
                }
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("links ") {
            if let Some((who, spec)) = rest.split_once(':') {
                let idx = ensure_participant(who.trim(), None, false, &mut participants, &mut part_map);
                let body = spec.trim().trim_start_matches('{').trim_end_matches('}');
                let mut start = 0usize;
                let mut depth = 0i32;
                let mut quote = false;
                let chars: Vec<(usize, char)> = body.char_indices().collect();
                let mut ends = Vec::new();
                for (i, ch) in chars {
                    match ch {
                        '"' => quote = !quote,
                        '{' | '[' if !quote => depth += 1,
                        '}' | ']' if !quote => depth -= 1,
                        ',' if !quote && depth == 0 => {
                            ends.push((start, i));
                            start = i + 1;
                        }
                        _ => {}
                    }
                }
                ends.push((start, body.len()));
                for (a, b) in ends {
                    let entry = body[a..b].trim();
                    let Some(colon) = find_unquoted(entry, ':') else { continue };
                    let title = unquote(entry[..colon].trim());
                    if !title.is_empty() {
                        participants[idx].links.push(title);
                    }
                }
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("create ") {
            let rest = rest.trim();
            let (is_actor, decl) = match (rest.strip_prefix("actor "), rest.strip_prefix("participant ")) {
                (Some(decl), _) => (true, decl),
                (None, Some(decl)) => (false, decl),
                (None, None) => (false, rest),
            };
            let (id, lbl, kind) = seq_declaration(decl);
            let idx = ensure_participant(id, lbl.as_deref(), is_actor, &mut participants, &mut part_map);
            participants[idx].kind = kind.or(participants[idx].kind);
            note_seq_box(open.last(), &mut boxes, idx);
            pending_create.push(idx);
            continue;
        }
        if let Some(rest) = l.strip_prefix("destroy ") {
            let id = rest.trim();
            let idx = ensure_participant(id, None, false, &mut participants, &mut part_map);
            pending_destroy.push(idx);
            continue;
        }
        if let Some(rest) = l.strip_prefix("rect ") {
            items.push(SeqItem::RectStart { color: rest.trim().to_string() });
            open.push(SeqOpen::Rect);
            continue;
        }
        if let Some(rest) = l.strip_prefix("and ").or_else(|| l.strip_prefix("option ")).or(if l == "and" { Some("") } else { None })
            && matches!(open.last(), Some(SeqOpen::Fragment))
        {
            items.push(SeqItem::BlockElse { label: rest.trim().to_string() });
            continue;
        }
        let declared = l.strip_prefix("participant ").map(|r| (r, false)).or_else(|| l.strip_prefix("actor ").map(|r| (r, true)));
        if let Some((rest, is_actor)) = declared {
            let (id, lbl, kind) = seq_declaration(rest);
            let idx = ensure_participant(id, lbl.as_deref(), is_actor, &mut participants, &mut part_map);
            participants[idx].kind = kind.or(participants[idx].kind);
            note_seq_box(open.last(), &mut boxes, idx);
            continue;
        }
        if l == "end" {
            match open.pop() {
                Some(SeqOpen::Box) => {}
                Some(SeqOpen::Rect) => items.push(SeqItem::RectEnd),
                Some(SeqOpen::Fragment) | None => items.push(SeqItem::BlockEnd),
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("box ") {
            boxes.push(SeqBox { title: seq_box_title(rest), members: Vec::new() });
            open.push(SeqOpen::Box);
            continue;
        }
        if let Some(rest) = l.strip_prefix("activate ") {
            let id = rest.split_whitespace().next().unwrap_or("");
            let idx = ensure_participant(id, None, false, &mut participants, &mut part_map);
            note_seq_box(open.last(), &mut boxes, idx);
            items.push(SeqItem::Activate(idx));
            continue;
        }
        if let Some(rest) = l.strip_prefix("deactivate ") {
            let id = rest.split_whitespace().next().unwrap_or("");
            let idx = ensure_participant(id, None, false, &mut participants, &mut part_map);
            note_seq_box(open.last(), &mut boxes, idx);
            items.push(SeqItem::Deactivate(idx));
            continue;
        }
        if let Some(rest) = l.strip_prefix("else") {
            items.push(SeqItem::BlockElse {
                label: rest.trim().to_string(),
            });
            continue;
        }
        for kw in ["loop ", "alt ", "opt ", "par ", "critical ", "break "] {
            if let Some(rest) = l.strip_prefix(kw) {
                items.push(SeqItem::BlockStart {
                    kind: kw.trim().to_string(),
                    label: rest.trim().to_string(),
                    color: None,
                });
                open.push(SeqOpen::Fragment);
                break;
            }
        }
        if matches!(items.last(), Some(SeqItem::BlockStart { .. })) && l.starts_with(|c: char| c.is_ascii_alphabetic())
            && ["loop ", "alt ", "opt ", "par ", "critical ", "break "].iter().any(|kw| l.starts_with(kw))
        {
            continue;
        }
        if let Some(note_item) = parse_seq_note(l, &mut participants, &mut part_map, &ensure_participant) {
            items.push(note_item);
            continue;
        }
        if let Some(msg_item) = parse_seq_message(l, &mut participants, &mut part_map, &ensure_participant) {
            if let SeqItem::Message { from, to, .. } = &msg_item {
                note_seq_box(open.last(), &mut boxes, *from);
                note_seq_box(open.last(), &mut boxes, *to);
                for who in [*to, *from] {
                    if let Some(pos) = pending_create.iter().position(|p| *p == who) {
                        pending_create.remove(pos);
                        created_at.insert(who, items.len());
                        break;
                    }
                }
                for who in [*from, *to] {
                    if let Some(pos) = pending_destroy.iter().position(|p| *p == who) {
                        pending_destroy.remove(pos);
                        destroyed_at.insert(who, items.len());
                    }
                }
            }
            items.push(msg_item);
            continue;
        }
    }

    if participants.is_empty() {
        return None;
    }

    Some(SeqDiagram {
        participants,
        items,
        autonumber,
        mirror_actors: true,
        boxes,
        title,
        created_at,
        destroyed_at,
    })
}

pub(super) fn render_seq_diagram_svg(diagram: &SeqDiagram) -> Option<(Vec<u8>, u32, u32)> {
    let participants = &diagram.participants;
    let items = &diagram.items;
    let boxes = &diagram.boxes;
    let created_at = &diagram.created_at;
    let destroyed_at = &diagram.destroyed_at;
    if participants.is_empty() {
        return None;
    }

    // Layout geometry
    let has_left_gate = items
        .iter()
        .any(|item| matches!(item, SeqItem::Gate { from_left: true, .. }));
    let has_right_gate = items
        .iter()
        .any(|item| matches!(item, SeqItem::Gate { from_left: false, .. }));
    let margin_left = if has_left_gate { 76.0f32 } else { 32.0f32 };
    let margin_right = if has_right_gate { 76.0f32 } else { 32.0f32 };
    let top_y = 24.0f32;
    let box_h = 38.0f32;
    let col_widths: Vec<f32> = participants
        .iter()
        .map(|p| (estimate_text_width(&p.label, 13.0) + if p.kind.is_some() { 56.0 } else { 32.0 }).max(96.0))
        .collect();

    // Compute horizontal centers ensuring message labels have room.
    let mut col_gaps: Vec<f32> = vec![56.0; participants.len().saturating_sub(1)];
    for item in items {
        if let SeqItem::Message { from, to, label, .. } = item
            && from != to
        {
            let a = (*from).min(*to);
            let b = (*from).max(*to);
            let needed = estimate_text_width(label, 12.0) + 44.0;
            let span_cols = (b - a) as f32;
            let per_gap = (needed / span_cols - 60.0).max(48.0);
            for g in &mut col_gaps[a..b] {
                if per_gap > *g {
                    *g = per_gap;
                }
            }
        }
    }

    let mut centers = Vec::with_capacity(participants.len());
    let mut cursor_x = margin_left;
    for (idx, w) in col_widths.iter().enumerate() {
        centers.push(cursor_x + w / 2.0);
        cursor_x += w;
        if idx < col_gaps.len() {
            cursor_x += col_gaps[idx];
        }
    }
    let total_w = (cursor_x + margin_right).ceil() as u32;

    let mut y = top_y + box_h + 28.0;
    let row_step = 44.0f32;
    let self_step = 54.0f32;
    let mut item_y = Vec::with_capacity(items.len());
    let mut block_stack: Vec<(usize, f32)> = Vec::new();
    let mut block_boxes: Vec<(String, String, Option<String>, f32, f32, usize)> = Vec::new();
    let mut rect_stack: Vec<(usize, f32)> = Vec::new();
    // (color, first item, last item, top, bottom, depth)
    let mut rect_boxes: Vec<(String, usize, usize, f32, f32, usize)> = Vec::new();
    let creating: Vec<usize> = created_at.values().copied().collect();

    for (idx, item) in items.iter().enumerate() {
        match item {
            SeqItem::Message { from, to, .. } => {
                // A message that creates its participant draws the participant's box on the arrow.
                let creates = creating.contains(&idx);
                if creates {
                    y += box_h / 2.0;
                }
                item_y.push(y);
                y += if from == to { self_step } else { row_step };
                if creates {
                    y += box_h / 2.0;
                }
            }
            SeqItem::Gate { .. } => {
                item_y.push(y);
                y += row_step;
            }
            SeqItem::RectStart { .. } => {
                item_y.push(y);
                rect_stack.push((idx, y - 22.0));
                y += 4.0;
            }
            SeqItem::RectEnd => {
                item_y.push(y);
                if let Some((start_idx, start_y)) = rect_stack.pop()
                    && let SeqItem::RectStart { color } = &items[start_idx]
                {
                    rect_boxes.push((color.clone(), start_idx, idx, start_y, y - 4.0, rect_stack.len()));
                }
                y += 24.0;
            }
            SeqItem::Activate(_) | SeqItem::Deactivate(_) => {
                item_y.push(y);
                y += 8.0;
            }
            SeqItem::Autonumber(_) => {
                item_y.push(y);
            }
            SeqItem::Note { .. } => {
                item_y.push(y);
                y += 42.0;
            }
            SeqItem::Divider { .. } => {
                item_y.push(y);
                y += 38.0;
            }
            SeqItem::Delay { .. } => {
                item_y.push(y);
                y += 36.0;
            }
            SeqItem::Ref { .. } => {
                item_y.push(y);
                y += 48.0;
            }
            SeqItem::BlockStart { kind, label, .. } => {
                item_y.push(y);
                block_stack.push((idx, y - 14.0));
                let _ = (kind, label);
                y += 28.0;
            }
            SeqItem::BlockElse { .. } => {
                item_y.push(y);
                y += 26.0;
            }
            SeqItem::BlockEnd => {
                item_y.push(y);
                if let Some((start_idx, start_y)) = block_stack.pop()
                    && let SeqItem::BlockStart { kind, label, color } = &items[start_idx]
                {
                    block_boxes.push((kind.clone(), label.clone(), color.clone(), start_y, y + 6.0, block_stack.len()));
                }
                y += 22.0;
            }
        }
    }

    let bottom_box_y = y + 12.0;
    let total_h = if diagram.mirror_actors {
        (bottom_box_y + box_h + 24.0).ceil() as u32
    } else {
        (bottom_box_y + 16.0).ceil() as u32
    };

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(
        r##"<defs>
  <marker id="seq-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 0 1 L 10 5 L 0 9 z" fill="#334155"/>
  </marker>
  <marker id="seq-open" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 1 1 L 9 5 L 1 9" fill="none" stroke="#334155" stroke-width="1.6"/>
  </marker>
</defs>"##,
    );
    svg.push_str(&format!(
        r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##
    ));

    for seq_box in boxes {
        if seq_box.members.is_empty() {
            continue;
        }
        let min_i = *seq_box.members.iter().min().unwrap_or(&0);
        let max_i = *seq_box.members.iter().max().unwrap_or(&0);
        let x = centers[min_i] - col_widths[min_i] / 2.0 - 10.0;
        let right = centers[max_i] + col_widths[max_i] / 2.0 + 10.0;
        let y = (top_y - 22.0).max(4.0);
        let bottom_extent = if diagram.mirror_actors { bottom_box_y + box_h } else { bottom_box_y };
        let height = bottom_extent - y + 8.0;
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{:.1}" height="{height:.1}" rx="8" fill="#f8fafc" stroke="#94a3b8" stroke-width="1.2" data-shape="seq-box"/>"##,
            right - x
        ));
        if !seq_box.title.is_empty() {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" font-size="11" font-weight="600" fill="#475569">{}</text>"##,
                x + 8.0,
                y + 14.0,
                escape_xml(&seq_box.title)
            ));
        }
    }

    // `rect` highlights: behind everything else, across the participants they involve.
    // Outer bands first, so a nested band stays visible on top of its parent.
    rect_boxes.sort_by_key(|band| band.5);
    for (color, first, last, sy, ey, depth) in &rect_boxes {
        let mut lo = usize::MAX;
        let mut hi = 0usize;
        for item in &items[*first..=*last] {
            let (a, b) = match item {
                SeqItem::Message { from, to, .. }
                | SeqItem::Note { from, to, .. }
                | SeqItem::Ref { from, to, .. } => (*from, *to),
                SeqItem::Activate(who) | SeqItem::Deactivate(who) | SeqItem::Gate { target: who, .. } => (*who, *who),
                _ => continue,
            };
            lo = lo.min(a.min(b));
            hi = hi.max(a.max(b));
        }
        let inset = *depth as f32 * 6.0;
        let (x, right) = if lo <= hi {
            (centers[lo] - col_widths[lo] / 2.0 - 6.0 + inset, centers[hi] + col_widths[hi] / 2.0 + 6.0 - inset)
        } else {
            (10.0 + inset, total_w as f32 - 10.0 - inset)
        };
        let (fill, opacity) = seq_rect_paint(color);
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{sy:.1}" width="{:.1}" height="{:.1}" fill="{fill}" fill-opacity="{opacity:.2}" data-shape="seq-rect"/>"##,
            (right - x).max(4.0),
            (ey - sy).max(8.0)
        ));
    }

    // Lifelines: a created participant's starts at its box, a destroyed one's ends at its cross.
    for (idx, &cx) in centers.iter().enumerate() {
        let start = created_at.get(&idx).map_or(top_y + box_h, |&item| item_y[item] + box_h / 2.0);
        let end = destroyed_at.get(&idx).map_or(bottom_box_y, |&item| item_y[item]);
        svg.push_str(&format!(
            r##"<line x1="{cx:.1}" y1="{start:.1}" x2="{cx:.1}" y2="{end:.1}" stroke="#94a3b8" stroke-width="1.2" stroke-dasharray="5,4"/>"##
        ));
    }

    // Combined fragment frames (`alt`, `loop`, `group`, etc.)
    for (kind, label, color, sy, ey, depth) in &block_boxes {
        let inset = (*depth as f32) * 8.0;
        let bx = 14.0 + inset;
        let bw = (total_w as f32) - 28.0 - inset * 2.0;
        let bh = (ey - sy).max(28.0);
        let (fill, opacity) = match color.as_deref() {
            Some(c) => {
                let (paint, op) = seq_rect_paint(c);
                (paint, (op * 0.25).clamp(0.10, 0.45))
            }
            None => ("#f8fafc".to_string(), 0.55),
        };
        svg.push_str(&format!(
            r##"<rect x="{bx:.1}" y="{sy:.1}" width="{bw:.1}" height="{bh:.1}" rx="4" fill="{fill}" fill-opacity="{opacity:.2}" stroke="#64748b" stroke-width="1.2"/>"##
        ));
        let tag_w = (estimate_text_width(kind, 11.0) + 18.0).max(46.0);
        svg.push_str(&format!(
            r##"<path d="M {bx:.1} {sy:.1} H {:.1} V {:.1} L {:.1} {:.1} H {bx:.1} Z" fill="#e2e8f0" stroke="#64748b" stroke-width="1"/>"##,
            bx + tag_w,
            sy + 14.0,
            bx + tag_w - 6.0,
            sy + 20.0
        ));
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" font-size="11" font-weight="bold" fill="#1e293b">{}</text>"##,
            bx + 8.0,
            sy + 14.0,
            escape_xml(kind)
        ));
        if !label.is_empty() {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" font-size="11" fill="#334155">[{}]</text>"##,
                bx + tag_w + 8.0,
                sy + 14.0,
                escape_xml(label)
            ));
        }
    }

    // Messages, notes, dividers, delays, refs, gates, else dividers
    let mut autonumber_active = diagram.autonumber;
    let mut msg_num = 1usize;
    let mut msg_step = 1usize;
    for (idx, item) in items.iter().enumerate() {
        let y = item_y[idx];
        match item {
            SeqItem::Autonumber(op) => match *op {
                SeqAutonumberOp::Start { start, step } => {
                    autonumber_active = true;
                    msg_num = start;
                    msg_step = step.max(1);
                }
                SeqAutonumberOp::Stop => {
                    autonumber_active = false;
                }
                SeqAutonumberOp::Resume => {
                    autonumber_active = true;
                }
            },
            SeqItem::Divider { label } => {
                let x1 = 16.0f32;
                let x2 = total_w as f32 - 16.0;
                svg.push_str(&format!(
                    r##"<line x1="{x1:.1}" y1="{:.1}" x2="{x2:.1}" y2="{:.1}" stroke="#64748b" stroke-width="1.2" data-shape="seq-divider"/><line x1="{x1:.1}" y1="{:.1}" x2="{x2:.1}" y2="{:.1}" stroke="#64748b" stroke-width="1.2"/>"##,
                    y - 2.0,
                    y - 2.0,
                    y + 2.0,
                    y + 2.0
                ));
                if !label.is_empty() {
                    let pw = (estimate_text_width(label, 11.5) + 24.0).max(72.0);
                    let px = total_w as f32 / 2.0 - pw / 2.0;
                    svg.push_str(&format!(
                        r##"<rect x="{px:.1}" y="{:.1}" width="{pw:.1}" height="22" rx="4" fill="#e2e8f0" stroke="#64748b" stroke-width="1"/>"##,
                        y - 11.0
                    ));
                    svg.push_str(&format!(
                        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11.5" font-weight="600" fill="#1e293b">{}</text>"##,
                        total_w as f32 / 2.0,
                        y + 4.0,
                        escape_xml(label)
                    ));
                }
            }
            SeqItem::Delay { label } => {
                svg.push_str(&format!(
                    r##"<rect x="12" y="{:.1}" width="{:.1}" height="22" fill="#ffffff" data-shape="seq-delay"/>"##,
                    y - 11.0,
                    total_w as f32 - 24.0
                ));
                for &cx in &centers {
                    svg.push_str(&format!(
                        r##"<line x1="{cx:.1}" y1="{:.1}" x2="{cx:.1}" y2="{:.1}" stroke="#94a3b8" stroke-width="1.4" stroke-dasharray="2,4"/>"##,
                        y - 10.0,
                        y + 10.0
                    ));
                }
                let text = if label.is_empty() { "..." } else { label.as_str() };
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11.5" font-style="italic" fill="#475569">{}</text>"##,
                    total_w as f32 / 2.0,
                    y + 4.0,
                    escape_xml(text)
                ));
            }
            SeqItem::Ref { from, to, label } => {
                let lo = (*from).min(*to);
                let hi = (*from).max(*to);
                let min_x = centers[lo] - col_widths[lo] / 2.0;
                let max_x = centers[hi] + col_widths[hi] / 2.0;
                let rw = (max_x - min_x).max(estimate_text_width(label, 12.0) + 48.0).max(110.0);
                let rx = ((min_x + max_x) / 2.0 - rw / 2.0).max(10.0);
                let ry = y - 16.0;
                let rh = 36.0f32;
                svg.push_str(&format!(
                    r##"<rect x="{rx:.1}" y="{ry:.1}" width="{rw:.1}" height="{rh:.1}" rx="4" fill="#f8fafc" stroke="#475569" stroke-width="1.2" data-shape="seq-ref"/>"##
                ));
                let tag_w = 36.0f32;
                svg.push_str(&format!(
                    r##"<path d="M {rx:.1} {ry:.1} H {:.1} V {:.1} L {:.1} {:.1} H {rx:.1} Z" fill="#e2e8f0" stroke="#475569" stroke-width="1"/>"##,
                    rx + tag_w,
                    ry + 12.0,
                    rx + tag_w - 5.0,
                    ry + 17.0
                ));
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{:.1}" font-size="10.5" font-weight="bold" fill="#1e293b">ref</text>"##,
                    rx + 8.0,
                    ry + 12.0
                ));
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="12" fill="#0f172a">{}</text>"##,
                    rx + rw / 2.0,
                    ry + 25.0,
                    escape_xml(label)
                ));
            }
            SeqItem::Gate {
                target,
                incoming,
                from_left,
                label,
                dashed,
                open_arrow,
                cross_end,
            } => {
                let edge_x = if *from_left { 14.0f32 } else { total_w as f32 - 14.0 };
                let target_x = centers[*target];
                let (x1, x2) = if *incoming {
                    (edge_x, target_x)
                } else {
                    (target_x, edge_x)
                };
                let dash_attr = if *dashed { r##" stroke-dasharray="6,4""## } else { "" };
                let marker_attr = if *cross_end {
                    ""
                } else if *open_arrow {
                    r##" marker-end="url(#seq-open)""##
                } else {
                    r##" marker-end="url(#seq-arrow)""##
                };
                svg.push_str(&format!(
                    r##"<line x1="{x1:.1}" y1="{y:.1}" x2="{x2:.1}" y2="{y:.1}" stroke="#334155" stroke-width="1.4"{dash_attr}{marker_attr} data-role="message" data-shape="seq-gate"/>"##
                ));
                if autonumber_active {
                    push_seq_autonumber(&mut svg, (x1 + x2) / 2.0, y - 10.0, msg_num);
                    msg_num += msg_step;
                }
                if *cross_end {
                    svg.push_str(&format!(
                        r##"<path d="M {:.1} {:.1} L {:.1} {:.1} M {:.1} {:.1} L {:.1} {:.1}" stroke="#dc2626" stroke-width="1.6"/>"##,
                        x2 - 5.0, y - 5.0, x2 + 5.0, y + 5.0, x2 + 5.0, y - 5.0, x2 - 5.0, y + 5.0
                    ));
                }
                if !label.is_empty() {
                    let mid_x = (x1 + x2) / 2.0;
                    let tw = estimate_text_width(label, 12.0) + 8.0;
                    svg.push_str(&format!(
                        r##"<rect x="{:.1}" y="{:.1}" width="{tw:.1}" height="16" fill="#ffffff" fill-opacity="0.88"/>"##,
                        mid_x - tw / 2.0,
                        y - 19.0
                    ));
                    svg.push_str(&format!(
                        r##"<text x="{mid_x:.1}" y="{:.1}" text-anchor="middle" font-size="12" fill="#0f172a">{}</text>"##,
                        y - 7.0,
                        escape_xml(label)
                    ));
                }
            }
            SeqItem::BlockElse { label } => {
                svg.push_str(&format!(
                    r##"<line x1="18" y1="{y:.1}" x2="{:.1}" y2="{y:.1}" stroke="#64748b" stroke-width="1" stroke-dasharray="4,4"/>"##,
                    total_w as f32 - 18.0
                ));
                if !label.is_empty() {
                    svg.push_str(&format!(
                        r##"<text x="28" y="{:.1}" font-size="11" fill="#475569">[{}]</text>"##,
                        y + 14.0,
                        escape_xml(label)
                    ));
                }
            }
            SeqItem::Note { from, to, pos, text, shape, color } => {
                let nw = (estimate_text_width(text, 11.5) + 22.0).max(80.0);
                let nh = 28.0f32;
                let nx = match pos {
                    SeqNotePos::Left => centers[*from] - nw - 12.0,
                    SeqNotePos::Right => centers[*from] + 12.0,
                    SeqNotePos::Over => (centers[*from] + centers[*to]) / 2.0 - nw / 2.0,
                }
                .max(8.0);
                let fill = match color.as_deref() {
                    Some(c) => seq_rect_paint(c).0,
                    None => "#fef9c3".to_string(),
                };
                match shape {
                    SeqNoteShape::Rect => {
                        svg.push_str(&format!(
                            r##"<rect x="{nx:.1}" y="{:.1}" width="{nw:.1}" height="{nh:.1}" rx="3" fill="{fill}" stroke="#ca8a04" stroke-width="1.1"/>"##,
                            y - 14.0
                        ));
                    }
                    SeqNoteShape::Box => {
                        svg.push_str(&format!(
                            r##"<rect x="{nx:.1}" y="{:.1}" width="{nw:.1}" height="{nh:.1}" rx="0" fill="{fill}" stroke="#ca8a04" stroke-width="1.1" data-shape="seq-rnote"/>"##,
                            y - 14.0
                        ));
                    }
                    SeqNoteShape::Hexagon => {
                        let top = y - 14.0;
                        let bot = top + nh;
                        let inset = 8.0f32;
                        svg.push_str(&format!(
                            r##"<polygon points="{:.1},{top:.1} {:.1},{top:.1} {:.1},{y:.1} {:.1},{bot:.1} {:.1},{bot:.1} {nx:.1},{y:.1}" fill="{fill}" stroke="#ca8a04" stroke-width="1.1" data-shape="seq-hnote"/>"##,
                            nx + inset,
                            nx + nw - inset,
                            nx + nw,
                            nx + nw - inset,
                            nx + inset
                        ));
                    }
                }
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11.5" fill="#713f12">{}</text>"##,
                    nx + nw / 2.0,
                    y + 4.0,
                    escape_xml(text)
                ));
            }
            SeqItem::Message {
                from,
                to,
                label,
                dashed,
                open_arrow,
                cross_end,
                bidirectional,
                circle_from,
                circle_to,
                activate_from: _,
                deactivate_from: _,
                activate_to: _,
                deactivate_to: _,
            } => {
                let current_num = if autonumber_active {
                    let n = msg_num;
                    msg_num += msg_step;
                    Some(n)
                } else {
                    None
                };
                let display_label = label.clone();
                let x1 = centers[*from];
                let mut x2 = centers[*to];
                // The creating message ends at the new participant's box.
                if created_at.get(to) == Some(&idx) && from != to {
                    x2 += if x2 > x1 { -col_widths[*to] / 2.0 } else { col_widths[*to] / 2.0 };
                }
                let dash_attr = if *dashed { r##" stroke-dasharray="6,4""## } else { "" };
                let marker_attr = if *cross_end {
                    ""
                } else if *open_arrow {
                    r##" marker-end="url(#seq-open)""##
                } else {
                    r##" marker-end="url(#seq-arrow)""##
                };

                let start_marker = if *bidirectional { r##" marker-start="url(#seq-arrow)""## } else { "" };
                if from == to {
                    let loop_w = 38.0;
                    let loop_h = 24.0;
                    svg.push_str(&format!(
                        r##"<path d="M {x1:.1} {y:.1} H {:.1} V {:.1} H {x1:.1}" fill="none" stroke="#334155" stroke-width="1.4"{dash_attr}{start_marker}{marker_attr} data-role="message"/>"##,
                        x1 + loop_w,
                        y + loop_h
                    ));
                    svg.push_str(&format!(
                        r##"<text x="{:.1}" y="{:.1}" font-size="12" fill="#0f172a">{}</text>"##,
                        x1 + loop_w + 6.0,
                        y + loop_h / 2.0 + 4.0,
                        escape_xml(&display_label)
                    ));
                    if let Some(num) = current_num {
                        push_seq_autonumber(&mut svg, x1 + loop_w, y - 2.0, num);
                    }
                } else {
                    svg.push_str(&format!(
                        r##"<line x1="{x1:.1}" y1="{y:.1}" x2="{x2:.1}" y2="{y:.1}" stroke="#334155" stroke-width="1.4"{dash_attr}{start_marker}{marker_attr} data-role="message"/>"##
                    ));
                    if let Some(num) = current_num {
                        push_seq_autonumber(&mut svg, (x1 + x2) / 2.0, y - 10.0, num);
                    }
                    if *cross_end {
                        svg.push_str(&format!(
                            r##"<path d="M {:.1} {:.1} L {:.1} {:.1} M {:.1} {:.1} L {:.1} {:.1}" stroke="#dc2626" stroke-width="1.6"/>"##,
                            x2 - 5.0, y - 5.0, x2 + 5.0, y + 5.0, x2 + 5.0, y - 5.0, x2 - 5.0, y + 5.0
                        ));
                    }
                    let mid_x = (x1 + x2) / 2.0;
                    let tw = estimate_text_width(&display_label, 12.0) + 8.0;
                    svg.push_str(&format!(
                        r##"<rect x="{:.1}" y="{:.1}" width="{tw:.1}" height="16" fill="#ffffff" fill-opacity="0.88"/>"##,
                        mid_x - tw / 2.0,
                        y - 19.0
                    ));
                    svg.push_str(&format!(
                        r##"<text x="{mid_x:.1}" y="{:.1}" text-anchor="middle" font-size="12" fill="#0f172a">{}</text>"##,
                        y - 7.0,
                        escape_xml(&display_label)
                    ));
                }
                // Mermaid's () markers sit on the participants, not in their names.
                for (enabled, cx, cy) in [(*circle_from, x1, y), (*circle_to, x2, if from == to { y + 24.0 } else { y })] {
                    if enabled {
                        svg.push_str(&format!(r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="4" fill="#334155" data-role="message-circle"/>"##));
                    }
                }
            }
            SeqItem::BlockStart { .. }
            | SeqItem::BlockEnd
            | SeqItem::Activate(_)
            | SeqItem::Deactivate(_)
            | SeqItem::RectStart { .. }
            | SeqItem::RectEnd => {}
        }
    }

    // Activation bars sit on the lifeline from `+` / `activate` until `-` / `deactivate`.
    for (who, y1, y2, depth) in seq_activations(items, &item_y, bottom_box_y) {
        let width = 10.0f32;
        let x = centers[who] - width / 2.0 + depth as f32 * 5.0;
        let top = y1.min(y2);
        let height = (y1 - y2).abs().max(12.0);
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{top:.1}" width="{width:.1}" height="{height:.1}" rx="2" fill="#bfdbfe" stroke="#2563eb" stroke-width="1.1" data-shape="activation"/>"##
        ));
    }

    // Participant boxes (top & bottom). A created participant's top box sits on its creating
    // message; a destroyed one has a cross where its lifeline ends and no bottom box.
    for (idx, p) in participants.iter().enumerate() {
        let cx = centers[idx];
        let w = col_widths[idx];
        let top = created_at.get(&idx).map_or(top_y, |&item| item_y[item] - box_h / 2.0);
        let mut rows = vec![top];
        if let Some(&item) = destroyed_at.get(&idx) {
            let dy = item_y[item];
            svg.push_str(&format!(
                r##"<path d="M {:.1} {:.1} L {:.1} {:.1} M {:.1} {:.1} L {:.1} {:.1}" stroke="#dc2626" stroke-width="2" data-shape="seq-destroy"/>"##,
                cx - 8.0, dy - 8.0, cx + 8.0, dy + 8.0, cx + 8.0, dy - 8.0, cx - 8.0, dy + 8.0
            ));
        } else if diagram.mirror_actors {
            rows.push(bottom_box_y);
        }
        for (row_i, by) in rows.iter().copied().enumerate() {
            let bx = cx - w / 2.0;
            let fill = if p.is_actor { "#eff6ff" } else { "#f1f5f9" };
            let stroke = if p.is_actor { "#2563eb" } else { "#475569" };
            let actor_attr = if row_i == 0 { r#" data-kind="actor""# } else { r#" data-role="actor-bottom""# };
            let text_x = match p.kind {
                Some(kind) => {
                    push_seq_kind_box(&mut svg, kind, bx, by, w, box_h, fill, stroke, actor_attr);
                    cx + 10.0
                }
                None => {
                    svg.push_str(&format!(
                        r##"<rect x="{bx:.1}" y="{by:.1}" width="{w:.1}" height="{box_h:.1}" rx="6" fill="{fill}" stroke="{stroke}" stroke-width="1.4"{actor_attr}/>"##
                    ));
                    cx
                }
            };
            svg.push_str(&format!(
                r##"<text x="{text_x:.1}" y="{:.1}" text-anchor="middle" font-size="13" font-weight="600" fill="#0f172a">{}</text>"##,
                by + box_h / 2.0 + 4.5,
                escape_xml(&p.label)
            ));
            // mermaid.js draws `link` / `links` titles as a menu beside the actor.
            if row_i == 0 {
                for (li, title) in p.links.iter().enumerate() {
                    svg.push_str(&format!(
                        r##"<text x="{:.1}" y="{:.1}" font-size="11" fill="#2563eb" text-decoration="underline" data-role="seq-link">{}</text>"##,
                        bx + w + 6.0,
                        by + 14.0 + li as f32 * 14.0,
                        escape_xml(title)
                    ));
                }
            }
        }
    }

    svg.push_str("</svg>");
    match &diagram.title {
        Some(title) => Some(add_svg_title(svg.into_bytes(), total_w, total_h, title)),
        None => Some((svg.into_bytes(), total_w, total_h)),
    }
}

/// A participant of a mermaid type: databases, queues and collections change the box itself;
/// boundary, control and entity keep the box and carry their UML symbol at its left.
#[allow(clippy::too_many_arguments)]
pub(super) fn push_seq_kind_box(svg: &mut String, kind: SeqKind, x: f32, y: f32, w: f32, h: f32, fill: &str, stroke: &str, extra: &str) {
    let name = kind.name();
    let attrs = format!(r##"fill="{fill}" stroke="{stroke}" stroke-width="1.4"{extra}"##);
    match kind {
        SeqKind::Database => {
            let ry = 5.0f32;
            svg.push_str(&format!(
                r##"<path d="M {x:.1} {:.1} L {x:.1} {:.1} A {:.1} {ry} 0 0 0 {:.1} {:.1} L {:.1} {:.1}" {attrs} data-shape="seq-{name}"/>"##,
                y + ry,
                y + h - ry,
                w / 2.0,
                x + w,
                y + h - ry,
                x + w,
                y + ry
            ));
            svg.push_str(&format!(
                r##"<ellipse cx="{:.1}" cy="{:.1}" rx="{:.1}" ry="{ry}" {attrs}/>"##,
                x + w / 2.0,
                y + ry,
                w / 2.0
            ));
        }
        SeqKind::Queue => {
            let r = 7.0f32;
            svg.push_str(&format!(
                r##"<path d="M {:.1} {y:.1} L {:.1} {y:.1} A {r} {:.1} 0 0 1 {:.1} {:.1} L {:.1} {:.1} A {r} {:.1} 0 0 1 {:.1} {y:.1} Z" {attrs} data-shape="seq-{name}"/>"##,
                x + r,
                x + w - r,
                h / 2.0,
                x + w - r,
                y + h,
                x + r,
                y + h,
                h / 2.0,
                x + r
            ));
            svg.push_str(&format!(
                r##"<path d="M {:.1} {y:.1} A {r} {:.1} 0 0 0 {:.1} {:.1}" fill="none" stroke="{stroke}" stroke-width="1.2"/>"##,
                x + w - r,
                h / 2.0,
                x + w - r,
                y + h
            ));
        }
        SeqKind::Collections => {
            svg.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{w:.1}" height="{h:.1}" rx="4" {attrs}/>"##,
                x + 4.0,
                y - 4.0
            ));
            svg.push_str(&format!(
                r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="4" {attrs} data-shape="seq-{name}"/>"##
            ));
        }
        SeqKind::Boundary | SeqKind::Control | SeqKind::Entity => {
            svg.push_str(&format!(
                r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="6" {attrs} data-shape="seq-{name}"/>"##
            ));
            let (ix, iy, r) = (x + 22.0, y + h / 2.0, 7.0f32);
            svg.push_str(&format!(
                r##"<circle cx="{ix:.1}" cy="{iy:.1}" r="{r}" fill="#ffffff" stroke="{stroke}" stroke-width="1.3"/>"##
            ));
            let mark = match kind {
                SeqKind::Boundary => format!(
                    "M {:.1} {:.1} L {:.1} {:.1} M {:.1} {iy:.1} L {:.1} {iy:.1}",
                    ix - 12.0,
                    iy - 7.0,
                    ix - 12.0,
                    iy + 7.0,
                    ix - 12.0,
                    ix - r
                ),
                SeqKind::Control => format!("M {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1}", ix - 1.0, iy - r - 3.0, ix + 3.0, iy - r, ix - 1.0, iy - r + 3.0),
                _ => format!("M {:.1} {:.1} L {:.1} {:.1}", ix - r, iy + r + 2.0, ix + r, iy + r + 2.0),
            };
            svg.push_str(&format!(r##"<path d="{mark}" fill="none" stroke="{stroke}" stroke-width="1.3"/>"##));
        }
    }
}

/// `rect` colour: `rgb(r, g, b)`, `rgba(r, g, b, a)`, `#hex` or a CSS name, as an SVG fill and opacity.
/// Anything else falls back to mermaid's default pale highlight.
pub(super) fn seq_rect_paint(color: &str) -> (String, f32) {
    let color = color.trim();
    let nums = |inner: &str| -> Vec<f32> { inner.split(',').filter_map(|n| n.trim().parse::<f32>().ok()).collect() };
    if let Some(inner) = color.strip_prefix("rgba(").and_then(|r| r.strip_suffix(')')) {
        let v = nums(inner);
        if v.len() == 4 {
            return (format!("rgb({},{},{})", v[0].clamp(0.0, 255.0), v[1].clamp(0.0, 255.0), v[2].clamp(0.0, 255.0)), v[3].clamp(0.0, 1.0));
        }
    }
    if let Some(inner) = color.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let v = nums(inner);
        if v.len() == 3 {
            return (format!("rgb({},{},{})", v[0].clamp(0.0, 255.0), v[1].clamp(0.0, 255.0), v[2].clamp(0.0, 255.0)), 1.0);
        }
    }
    let hex = color.strip_prefix('#').is_some_and(|h| matches!(h.len(), 3 | 6) && h.chars().all(|c| c.is_ascii_hexdigit()));
    if hex || (!color.is_empty() && color.len() <= 20 && color.chars().all(|c| c.is_ascii_alphabetic())) {
        return (color.to_string(), 1.0);
    }
    ("rgb(200,200,255)".to_string(), 0.35)
}

pub(super) fn note_seq_box(open: Option<&SeqOpen>, boxes: &mut [SeqBox], idx: usize) {
    if !matches!(open, Some(SeqOpen::Box)) {
        return;
    }
    if let Some(seq_box) = boxes.last_mut()
        && !seq_box.members.contains(&idx)
    {
        seq_box.members.push(idx);
    }
}

pub(super) fn seq_box_title(rest: &str) -> String {
    let rest = rest.trim();
    if let Some(after) = rest.strip_prefix("rgb(").or_else(|| rest.strip_prefix("rgba("))
        && let Some(end) = after.find(')')
    {
        return after[end + 1..].trim().to_string();
    }
    if let Some((first, title)) = rest.split_once(char::is_whitespace) {
        let title = title.trim();
        if seq_looks_like_color(first) && !title.is_empty() {
            return title.to_string();
        }
    }
    rest.to_string()
}

/// A CSS colour word or hex used as the optional first token of `box <color> Title`.
fn seq_looks_like_color(s: &str) -> bool {
    let s = s.trim();
    if s.len() >= 4 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit()) && matches!(s.len() - 1, 3 | 4 | 6 | 8) {
        return true;
    }
    if s.eq_ignore_ascii_case("transparent") || s.eq_ignore_ascii_case("currentColor") {
        return true;
    }
    // Common CSS colour names (mermaid `box Purple Alice`).
    matches!(
        s.to_ascii_lowercase().as_str(),
        "aliceblue" | "antiquewhite" | "aqua" | "aquamarine" | "azure" | "beige" | "bisque" | "black"
            | "blanchedalmond" | "blue" | "blueviolet" | "brown" | "burlywood" | "cadetblue" | "chartreuse"
            | "chocolate" | "coral" | "cornflowerblue" | "cornsilk" | "crimson" | "cyan" | "darkblue" | "darkcyan"
            | "darkgoldenrod" | "darkgray" | "darkgreen" | "darkgrey" | "darkkhaki" | "darkmagenta" | "darkolivegreen"
            | "darkorange" | "darkorchid" | "darkred" | "darksalmon" | "darkseagreen" | "darkslateblue" | "darkslategray"
            | "darkslategrey" | "darkturquoise" | "darkviolet" | "deeppink" | "deepskyblue" | "dimgray" | "dimgrey"
            | "dodgerblue" | "firebrick" | "floralwhite" | "forestgreen" | "fuchsia" | "gainsboro" | "ghostwhite"
            | "gold" | "goldenrod" | "gray" | "green" | "greenyellow" | "grey" | "honeydew" | "hotpink" | "indianred"
            | "indigo" | "ivory" | "khaki" | "lavender" | "lavenderblush" | "lawngreen" | "lemonchiffon" | "lightblue"
            | "lightcoral" | "lightcyan" | "lightgoldenrodyellow" | "lightgray" | "lightgreen" | "lightgrey"
            | "lightpink" | "lightsalmon" | "lightseagreen" | "lightskyblue" | "lightslategray" | "lightslategrey"
            | "lightsteelblue" | "lightyellow" | "lime" | "limegreen" | "linen" | "magenta" | "maroon"
            | "mediumaquamarine" | "mediumblue" | "mediumorchid" | "mediumpurple" | "mediumseagreen" | "mediumslateblue"
            | "mediumspringgreen" | "mediumturquoise" | "mediumvioletred" | "midnightblue" | "mintcream" | "mistyrose"
            | "moccasin" | "navajowhite" | "navy" | "oldlace" | "olive" | "olivedrab" | "orange" | "orangered"
            | "orchid" | "palegoldenrod" | "palegreen" | "paleturquoise" | "palevioletred" | "papayawhip" | "peachpuff"
            | "peru" | "pink" | "plum" | "powderblue" | "purple" | "rebeccapurple" | "red" | "rosybrown" | "royalblue"
            | "saddlebrown" | "salmon" | "sandybrown" | "seagreen" | "seashell" | "sienna" | "silver" | "skyblue"
            | "slateblue" | "slategray" | "slategrey" | "snow" | "springgreen" | "steelblue" | "tan" | "teal"
            | "thistle" | "tomato" | "turquoise" | "violet" | "wheat" | "white" | "whitesmoke" | "yellow" | "yellowgreen"
    )
}

pub(super) fn seq_actor_token(raw: &str) -> (&str, bool, bool) {
    let token = raw.trim();
    let activate = token.starts_with('+') || token.ends_with('+');
    let deactivate = token.starts_with('-') || token.ends_with('-');
    (token.trim_matches(['+', '-']).trim(), activate, deactivate)
}

pub(super) fn seq_activations(items: &[SeqItem], item_y: &[f32], bottom: f32) -> Vec<(usize, f32, f32, usize)> {
    let mut open: Vec<(usize, f32, usize)> = Vec::new();
    let mut bars = Vec::new();
    let mut depth = vec![0usize; 64];
    let depth_of = |depth: &mut Vec<usize>, who: usize| -> usize {
        if who >= depth.len() {
            depth.resize(who + 1, 0);
        }
        depth[who]
    };
    for (idx, item) in items.iter().enumerate() {
        let y = item_y.get(idx).copied().unwrap_or(bottom);
        let mut turn_on = Vec::new();
        let mut turn_off = Vec::new();
        match item {
            SeqItem::Activate(who) => turn_on.push(*who),
            SeqItem::Deactivate(who) => turn_off.push(*who),
            SeqItem::Message {
                from,
                to,
                activate_from,
                deactivate_from,
                activate_to,
                deactivate_to,
                ..
            } => {
                if *deactivate_from {
                    turn_off.push(*from);
                }
                if *deactivate_to {
                    turn_off.push(*to);
                }
                if *activate_from {
                    turn_on.push(*from);
                }
                if *activate_to {
                    turn_on.push(*to);
                }
            }
            _ => {}
        }
        for who in turn_off {
            if let Some(pos) = open.iter().rposition(|(id, _, _)| *id == who) {
                let (_, start, level) = open.remove(pos);
                bars.push((who, start, y, level));
                if who < depth.len() && depth[who] > 0 {
                    depth[who] -= 1;
                }
            }
        }
        for who in turn_on {
            let level = depth_of(&mut depth, who);
            open.push((who, y, level));
            depth[who] += 1;
        }
    }
    for (who, start, level) in open {
        bars.push((who, start, bottom, level));
    }
    bars
}

pub(super) fn split_alias(rest: &str) -> (&str, Option<&str>) {
    if let Some((id, lbl)) = rest.split_once(" as ") {
        (id.trim(), Some(lbl.trim()))
    } else {
        (rest.trim(), None)
    }
}

pub(super) fn parse_seq_note(
    l: &str,
    participants: &mut Vec<SeqParticipant>,
    part_map: &mut HashMap<String, usize>,
    ensure: &impl Fn(&str, Option<&str>, bool, &mut Vec<SeqParticipant>, &mut HashMap<String, usize>) -> usize,
) -> Option<SeqItem> {
    let rest = l.strip_prefix("Note ").or_else(|| l.strip_prefix("note "))?.trim();
    let (target_part, text) = rest.split_once(':')?;
    let text = text.trim().to_string();
    let target_part = target_part.trim();
    let (pos, who) = if let Some(w) = target_part.strip_prefix("right of ") {
        (SeqNotePos::Right, w.trim())
    } else if let Some(w) = target_part.strip_prefix("left of ") {
        (SeqNotePos::Left, w.trim())
    } else if let Some(w) = target_part.strip_prefix("over ") {
        (SeqNotePos::Over, w.trim())
    } else {
        return None;
    };
    if let Some((a, b)) = who.split_once(',') {
        let from = ensure(a.trim(), None, false, participants, part_map);
        let to = ensure(b.trim(), None, false, participants, part_map);
        Some(SeqItem::Note {
            from,
            to,
            pos,
            text,
            shape: SeqNoteShape::Rect,
            color: None,
        })
    } else {
        let idx = ensure(who, None, false, participants, part_map);
        Some(SeqItem::Note {
            from: idx,
            to: idx,
            pos,
            text,
            shape: SeqNoteShape::Rect,
            color: None,
        })
    }
}

pub(super) fn parse_seq_message(
    l: &str,
    participants: &mut Vec<SeqParticipant>,
    part_map: &mut HashMap<String, usize>,
    ensure: &impl Fn(&str, Option<&str>, bool, &mut Vec<SeqParticipant>, &mut HashMap<String, usize>) -> usize,
) -> Option<SeqItem> {
    let (lhs, label) = l.split_once(':')?;
    let arrows = [
        ("<<-->>", true, false, false),
        ("<<->>", false, false, false),
        ("-->>", true, false, false),
        ("->>", false, false, false),
        ("-->", true, true, false),
        ("->", false, true, false),
        ("--x", true, false, true),
        ("-x", false, false, true),
        ("--)", true, true, false),
        ("-)", false, true, false),
    ];
    for (arrow, dashed, open_arrow, cross_end) in arrows {
        if let Some((from_raw, to_raw)) = lhs.split_once(arrow) {
            let from_raw = from_raw.trim();
            let to_raw = to_raw.trim();
            let circle_from = from_raw.ends_with("()");
            let circle_to = to_raw.starts_with("()");
            let from_raw = from_raw.strip_suffix("()").unwrap_or(from_raw);
            let to_raw = to_raw.strip_prefix("()").unwrap_or(to_raw);
            let (from_id, activate_from, deactivate_from) = seq_actor_token(from_raw);
            let (to_id, activate_to, deactivate_to) = seq_actor_token(to_raw);
            if from_id.is_empty() || to_id.is_empty() {
                return None;
            }
            let from = ensure(from_id, None, false, participants, part_map);
            let to = ensure(to_id, None, false, participants, part_map);
            return Some(SeqItem::Message {
                from,
                to,
                label: label.trim().to_string(),
                dashed,
                open_arrow,
                cross_end,
                bidirectional: arrow.starts_with("<<"),
                circle_from,
                circle_to,
                activate_from,
                deactivate_from,
                activate_to,
                deactivate_to,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn sequence_bidirectional_messages_draw_both_arrowheads() {
        let svg = svg_of("sequenceDiagram\nAlice<<->>Bob: Request\nBob<<-->>Alice: Reply\n");
        let messages = tag_with(&svg, r#"data-role="message""#);
        assert_eq!(messages.len(), 2);
        for message in &messages {
            assert!(message.contains("marker-start=") && message.contains("marker-end="));
        }
        assert!(messages[1].contains("stroke-dasharray="));
        assert!(!svg.contains("Alice&lt;&lt;"));
    }

    #[test]
    fn sequence_circle_endpoints_keep_the_original_participants() {
        let svg = svg_of("sequenceDiagram\n  participant Alice\n  participant John\n  Alice->>()John: Hello\n  Alice()->>John: How are you?\n  John()->>()Alice: Great!\n");
        assert_eq!(svg.matches("data-role=\"message-circle\"").count(), 4, "{svg}");
        assert_eq!(svg.matches("data-role=\"message\"").count(), 3, "{svg}");
        assert!(!svg.contains(">()John<") && !svg.contains(">Alice()<") && !svg.contains(">John()<") && !svg.contains(">()Alice<"), "{svg}");
    }

    #[test]
    fn renders_sequence_diagram_svg() {
        let seq = "\
sequenceDiagram
  autonumber
  actor U as ユーザー
  participant API as バックエンドAPI
  U->>API: リクエスト送信
  alt 成功時
    API-->>U: 200 OK
  else エラー時
    API--xU: 500 Error
  end
  Note over U,API: 完了
";
        let Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        }) = render_mermaid(seq)
        else {
            panic!("expected svg for sequenceDiagram");
        };
        assert!(width_px > 200 && height_px > 200);
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(svg.contains(r#"data-kind="actor""#), "{svg}");
        assert!(svg.starts_with("<svg "));
        assert!(svg.contains("ユーザー"));
        assert!(svg.contains("バックエンドAPI"));
        assert!(svg.contains(">リクエスト送信<") && svg.contains(r#"data-role="seq-autonumber">1</text>"#), "{svg}");
        assert!(svg.contains("200 OK"));
        assert!(svg.contains("alt"));
    }

    #[test]
    fn sequence_activation_and_box() {
        let src = "\
sequenceDiagram
  box rgb(200,220,240) 利用者
    actor U as ユーザー
    participant API as API
  end
  U->>+API: 依頼
  API-->>-U: 応答
";
        let svg = svg_of(src);
        assert!(svg.contains("data-shape=\"activation\""), "{svg}");
        assert!(svg.contains("data-shape=\"seq-box\""), "{svg}");
        assert!(svg.contains("利用者"));
        assert!(svg.contains("依頼"));
        assert_eq!(seq_box_title("Purple Alice & John"), "Alice & John");
        assert_eq!(seq_box_title("Another Group"), "Another Group");
        assert_eq!(seq_box_title("rgb(200,220,240) 利用者"), "利用者");
        let nested = svg_of(
            "sequenceDiagram
    box Purple Alice & John
    participant A
    participant J
    end
    box Another Group
    participant B
    participant C
    end
    A->>J: Hello John, how are you?
",
        );
        assert!(nested.contains(">Another Group<"), "{nested}");
        assert!(nested.contains(">Alice &amp; John<") || nested.contains(">Alice & John<"), "{nested}");

        let linked = svg_of(
            "sequenceDiagram
    participant Alice
    link Alice: Dashboard @ https://example.com/dash
    links Alice: {\"Wiki\": \"https://example.com/wiki\"}
    Alice->>Alice: hi
",
        );
        assert!(linked.contains(">Dashboard<") && linked.contains(">Wiki<"), "{linked}");
    }

    #[test]
    fn sequence_rect_create_destroy_title_and_dividers() {
        let svg = svg_of(
            "sequenceDiagram
  title 予約の流れ
  participant Alice
  rect rgba(0, 0, 255, 0.1)
    Alice->>Bob: Hello
    rect rgb(200, 150, 255)
      Bob-->>Alice: Hi
    end
  end
  create participant Carl
  Alice->>Carl: Hi Carl
  par Alice to Bob
    Alice->>Bob: one
  and Alice to Carl
    Alice->>Carl: two
  end
  destroy Carl
  Alice-xCarl: bye
  note over Alice: done
",
        );
        assert!(svg.contains(r#"data-role="diagram-title">予約の流れ</text>"#), "{svg}");
        assert_eq!(svg.matches(r#"data-shape="seq-rect""#).count(), 2);
        assert!(svg.contains(r#"fill="rgb(0,0,255)" fill-opacity="0.10""#), "{svg}");
        assert!(svg.contains(r#"fill="rgb(200,150,255)" fill-opacity="1.00""#));
        // Nested band drawn after (on top of) its parent.
        assert!(svg.find("rgb(0,0,255)").unwrap() < svg.find("rgb(200,150,255)").unwrap());
        // `rect ... end` does not close a fragment: one `par` frame, with an `and` divider.
        assert!(svg.contains(">par<"), "{svg}");
        assert!(svg.contains(">[Alice to Carl]<"));
        // Carl's top box sits on the creating message, below Alice's; Carl has no bottom box.
        let alice_boxes = svg.matches(">Alice</text>").count();
        let carl_boxes = svg.matches(">Carl</text>").count();
        assert_eq!(alice_boxes, 2);
        assert_eq!(carl_boxes, 1, "{svg}");
        assert!(text_y(&svg, "Carl") > text_y(&svg, "Alice") + 60.0);
        assert!(svg.contains(r#"data-shape="seq-destroy""#));
        assert!(svg.contains(">done<"), "lower-case note");
        assert!(roxmltree::Document::parse(&svg).is_ok());

        assert_eq!(seq_rect_paint("rgb(1,2,3)"), ("rgb(1,2,3)".to_string(), 1.0));
        assert_eq!(seq_rect_paint("#abc"), ("#abc".to_string(), 1.0));
        assert_eq!(seq_rect_paint("aqua"), ("aqua".to_string(), 1.0));
        assert_eq!(seq_rect_paint("url(javascript:x)").0, "rgb(200,200,255)");
    }

    #[test]
    fn sequence_participant_types_and_aliases() {
        let (id, label, kind) = seq_declaration(r#"Q@{ "type" : "queue", "alias": "Jobs" }"#);
        assert_eq!((id, label.as_deref()), ("Q", Some("Jobs")));
        assert!(matches!(kind, Some(SeqKind::Queue)));
        // `as` wins over `alias`.
        let (id, label, kind) = seq_declaration(r#"C@{ "type": "control", "alias": "X" } as Ctl"#);
        assert_eq!((id, label.as_deref()), ("C", Some("Ctl")));
        assert!(matches!(kind, Some(SeqKind::Control)));
        assert!(seq_declaration("Plain").2.is_none());

        let svg = svg_of(
            r#"sequenceDiagram
    participant A@{ "type": "database" }
    participant Q@{ "type": "queue", "alias": "Jobs" }
    participant C@{ "type": "control" } as Ctl
    actor E@{ "type": "entity" }
    participant B@{ "type": "boundary" }
    participant L@{ "type": "collections" }
    A->>Q: put
    Q->>C: take
"#,
        );
        for shape in ["database", "queue", "control", "entity", "boundary", "collections"] {
            assert!(svg.contains(&format!(r#"data-shape="seq-{shape}""#)), "{shape}: {svg}");
        }
        assert!(svg.contains(">Jobs<") && svg.contains(">Ctl<"), "{svg}");
        assert!(!svg.contains("@{"), "{svg}");
    }
}
