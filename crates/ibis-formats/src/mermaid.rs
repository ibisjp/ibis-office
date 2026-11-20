// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pure-Rust Mermaid diagram parser and SVG / native-chart renderer.
//!
//! Zero external dependencies: converts Mermaid code blocks into either:
//! - Native Office charts (`pie`, `xychart-beta`) via [`MermaidChartSpec`], or
//! - Clean, self-contained SVG vector graphics (`sequenceDiagram`, `flowchart` / `graph`)
//!   embedded as `MediaFormat::Svg` with the original Mermaid source preserved in `PictureLook::alt_text`
//!   for lossless `.md` round-tripping.

use std::collections::HashMap;

use ibis_doc_model::Number;

use crate::save::pptx_new_chart::NewChartKind;

/// Parsed native chart specification from Mermaid `pie` or `xychart-beta`.
#[derive(Debug, Clone, PartialEq)]
pub struct MermaidChartSpec {
    pub kind: NewChartKind,
    pub title: Option<String>,
    pub categories: Vec<String>,
    pub series: Vec<(String, Vec<Option<Number>>)>,
}

/// Result of converting a Mermaid block.
#[derive(Debug, Clone, PartialEq)]
pub enum MermaidOutput {
    /// Native Office chart (`pie`, `xychart-beta`).
    Chart(MermaidChartSpec),
    /// Self-contained SVG vector graphic (`svg_bytes`, `width_px`, `height_px`).
    Svg {
        bytes: Vec<u8>,
        width_px: u32,
        height_px: u32,
    },
}

fn strip_mermaid_frontmatter(source: &str) -> &str {
    let mut s = source.trim();
    if let Some(after_open) = s.strip_prefix("---") {
        if after_open.starts_with('\n') || after_open.starts_with("\r\n") {
            let mut offset = 3;
            for line in after_open.split_inclusive('\n') {
                offset += line.len();
                if line.trim() == "---" {
                    s = s[offset..].trim_start();
                    break;
                }
            }
        }
    }
    // Advance past any leading blank or `%%` directive/comment lines so line 0 is the diagram header.
    loop {
        let trimmed_start = s.trim_start();
        if trimmed_start.starts_with("%%") {
            if let Some(nl) = trimmed_start.find('\n') {
                s = &trimmed_start[nl + 1..];
                continue;
            }
            return "";
        }
        return trimmed_start;
    }
}

/// Parse and render a Mermaid diagram into either a native chart or an SVG.
/// Returns `None` if the diagram type is unsupported (caller falls back to a fenced code block).
pub fn render_mermaid(source: &str) -> Option<MermaidOutput> {
    let trimmed = strip_mermaid_frontmatter(source);
    if trimmed.is_empty() {
        return None;
    }
    let first_line = trimmed.lines().next()?.trim();

    if first_line == "pie" || first_line.starts_with("pie ") {
        return parse_pie_chart(trimmed).map(MermaidOutput::Chart);
    }
    if first_line == "xychart-beta" || first_line.starts_with("xychart-beta ") {
        return parse_xy_chart(trimmed).map(MermaidOutput::Chart);
    }
    if first_line == "sequenceDiagram" || first_line.starts_with("sequenceDiagram ") {
        let (bytes, width_px, height_px) = render_sequence_svg(trimmed)?;
        return Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        });
    }
    if first_line.starts_with("flowchart ")
        || first_line == "flowchart"
        || first_line.starts_with("graph ")
        || first_line == "graph"
    {
        let (bytes, width_px, height_px) = render_flowchart_svg(trimmed)?;
        return Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        });
    }
    if first_line == "stateDiagram"
        || first_line == "stateDiagram-v2"
        || first_line.starts_with("stateDiagram ")
        || first_line.starts_with("stateDiagram-v2 ")
    {
        let (bytes, width_px, height_px) = render_state_diagram_svg(trimmed)?;
        return Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        });
    }
    if first_line == "classDiagram" || first_line.starts_with("classDiagram ") {
        let (bytes, width_px, height_px) = render_class_diagram_svg(trimmed)?;
        return Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        });
    }
    if first_line == "erDiagram" || first_line.starts_with("erDiagram ") {
        let (bytes, width_px, height_px) = render_er_diagram_svg(trimmed)?;
        return Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        });
    }
    if first_line == "gantt" || first_line.starts_with("gantt ") {
        let (bytes, width_px, height_px) = render_gantt_svg(trimmed)?;
        return Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        });
    }
    if first_line == "mindmap" || first_line.starts_with("mindmap ") {
        let (bytes, width_px, height_px) = render_mindmap_svg(trimmed)?;
        return Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        });
    }
    if first_line == "gitGraph" || first_line.starts_with("gitGraph ") {
        let (bytes, width_px, height_px) = render_git_graph_svg(trimmed)?;
        return Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        });
    }
    if first_line == "journey" || first_line.starts_with("journey ") {
        let (bytes, width_px, height_px) = render_journey_svg(trimmed)?;
        return Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        });
    }
    if first_line == "timeline" || first_line.starts_with("timeline ") {
        let (bytes, width_px, height_px) = render_timeline_svg(trimmed)?;
        return Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        });
    }
    if first_line == "quadrantChart" || first_line.starts_with("quadrantChart ") {
        let (bytes, width_px, height_px) = render_quadrant_chart_svg(trimmed)?;
        return Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        });
    }
    None
}

// ============================================================================
// 1. Native Chart Parsers (`pie`, `xychart-beta`)
// ============================================================================

fn parse_pie_chart(source: &str) -> Option<MermaidChartSpec> {
    let mut title = None;
    let mut categories = Vec::new();
    let mut values = Vec::new();

    for line in source.lines().skip(1) {
        let l = line.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ") {
            let t = unquote(rest.trim());
            if !t.is_empty() {
                title = Some(t);
            }
            continue;
        }
        if let Some((label_part, val_part)) = l.split_once(':') {
            let label = unquote(label_part.trim());
            if let Ok(v) = val_part.trim().parse::<f64>()
                && !label.is_empty()
            {
                categories.push(label);
                values.push(Some(Number(v)));
            }
        }
    }

    if categories.is_empty() {
        return None;
    }
    Some(MermaidChartSpec {
        kind: NewChartKind::Pie,
        title,
        categories,
        series: vec![("Series 1".to_string(), values)],
    })
}

fn parse_xy_chart(source: &str) -> Option<MermaidChartSpec> {
    let mut title = None;
    let mut categories = Vec::new();
    let mut series = Vec::new();
    let mut has_bar = false;
    let mut has_line = false;
    let mut bar_idx = 1usize;
    let mut line_idx = 1usize;

    for line in source.lines().skip(1) {
        let l = line.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ") {
            let t = unquote(rest.trim());
            if !t.is_empty() {
                title = Some(t);
            }
        } else if let Some(rest) = l.strip_prefix("x-axis ") {
            if let Some(list) = extract_bracket_list(rest) {
                categories = list.into_iter().map(|s| unquote(&s)).collect();
            }
        } else if let Some(rest) = l.strip_prefix("bar ") {
            if let Some(list) = extract_bracket_list(rest) {
                has_bar = true;
                let vals: Vec<_> = list
                    .into_iter()
                    .map(|s| s.trim().parse::<f64>().ok().map(Number))
                    .collect();
                series.push((format!("Bar {bar_idx}"), vals));
                bar_idx += 1;
            }
        } else if let Some(rest) = l.strip_prefix("line ") {
            if let Some(list) = extract_bracket_list(rest) {
                has_line = true;
                let vals: Vec<_> = list
                    .into_iter()
                    .map(|s| s.trim().parse::<f64>().ok().map(Number))
                    .collect();
                series.push((format!("Line {line_idx}"), vals));
                line_idx += 1;
            }
        }
    }

    if series.is_empty() {
        return None;
    }
    if categories.is_empty() {
        let max_len = series.iter().map(|(_, v)| v.len()).max().unwrap_or(0);
        categories = (1..=max_len).map(|i| i.to_string()).collect();
    }
    let kind = if has_bar {
        NewChartKind::ClusteredColumn
    } else if has_line {
        NewChartKind::Line
    } else {
        NewChartKind::ClusteredColumn
    };
    Some(MermaidChartSpec {
        kind,
        title,
        categories,
        series,
    })
}

fn extract_bracket_list(s: &str) -> Option<Vec<String>> {
    let start = s.find('[')?;
    let end = s.rfind(']')?;
    if end <= start {
        return None;
    }
    Some(
        s[start + 1..end]
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect(),
    )
}

// ============================================================================
// 2. Sequence Diagram -> Pure Rust SVG
// ============================================================================

#[derive(Debug, Clone)]
struct SeqParticipant {
    _id: String,
    label: String,
    is_actor: bool,
}

#[derive(Debug, Clone)]
enum SeqItem {
    Message {
        from: usize,
        to: usize,
        label: String,
        dashed: bool,
        open_arrow: bool,
        cross_end: bool,
    },
    Note {
        from: usize,
        to: usize,
        pos: SeqNotePos,
        text: String,
    },
    BlockStart {
        kind: String,
        label: String,
    },
    BlockElse {
        label: String,
    },
    BlockEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SeqNotePos {
    Left,
    Right,
    Over,
}

fn render_sequence_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let mut participants: Vec<SeqParticipant> = Vec::new();
    let mut part_map: HashMap<String, usize> = HashMap::new();
    let mut items: Vec<SeqItem> = Vec::new();
    let mut autonumber = false;

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
        });
        part_map.insert(id.to_string(), idx);
        idx
    };

    for raw_line in source.lines().skip(1) {
        let l = raw_line.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if l == "autonumber" {
            autonumber = true;
            continue;
        }
        if let Some(rest) = l.strip_prefix("participant ") {
            let (id, lbl) = split_alias(rest);
            ensure_participant(id, lbl, false, &mut participants, &mut part_map);
            continue;
        }
        if let Some(rest) = l.strip_prefix("actor ") {
            let (id, lbl) = split_alias(rest);
            ensure_participant(id, lbl, true, &mut participants, &mut part_map);
            continue;
        }
        if l == "end" {
            items.push(SeqItem::BlockEnd);
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
                });
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
            items.push(msg_item);
            continue;
        }
    }

    if participants.is_empty() {
        return None;
    }

    // Layout geometry
    let margin_x = 32.0f32;
    let top_y = 24.0f32;
    let box_h = 38.0f32;
    let col_widths: Vec<f32> = participants
        .iter()
        .map(|p| (estimate_text_width(&p.label, 13.0) + 32.0).max(96.0))
        .collect();

    // Compute horizontal centers ensuring message labels have room.
    let mut col_gaps: Vec<f32> = vec![56.0; participants.len().saturating_sub(1)];
    for item in &items {
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
    let mut cursor_x = margin_x;
    for (idx, w) in col_widths.iter().enumerate() {
        centers.push(cursor_x + w / 2.0);
        cursor_x += w;
        if idx < col_gaps.len() {
            cursor_x += col_gaps[idx];
        }
    }
    let total_w = (cursor_x + margin_x).ceil() as u32;

    let mut y = top_y + box_h + 28.0;
    let row_step = 44.0f32;
    let self_step = 54.0f32;
    let mut item_y = Vec::with_capacity(items.len());
    let mut block_stack: Vec<(usize, f32)> = Vec::new();
    let mut block_boxes: Vec<(String, String, f32, f32, usize)> = Vec::new();

    for (idx, item) in items.iter().enumerate() {
        match item {
            SeqItem::Message { from, to, .. } => {
                item_y.push(y);
                y += if from == to { self_step } else { row_step };
            }
            SeqItem::Note { .. } => {
                item_y.push(y);
                y += 42.0;
            }
            SeqItem::BlockStart { kind, label } => {
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
                    && let SeqItem::BlockStart { kind, label } = &items[start_idx]
                {
                    block_boxes.push((kind.clone(), label.clone(), start_y, y + 6.0, block_stack.len()));
                }
                y += 22.0;
            }
        }
    }

    let bottom_box_y = y + 12.0;
    let total_h = (bottom_box_y + box_h + 24.0).ceil() as u32;

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

    // Lifelines
    for &cx in &centers {
        svg.push_str(&format!(
            r##"<line x1="{cx:.1}" y1="{:.1}" x2="{cx:.1}" y2="{bottom_box_y:.1}" stroke="#94a3b8" stroke-width="1.2" stroke-dasharray="5,4"/>"##,
            top_y + box_h
        ));
    }

    // Combined fragment frames (`alt`, `loop`, etc.)
    for (kind, label, sy, ey, depth) in &block_boxes {
        let inset = (*depth as f32) * 8.0;
        let bx = 14.0 + inset;
        let bw = (total_w as f32) - 28.0 - inset * 2.0;
        let bh = (ey - sy).max(28.0);
        svg.push_str(&format!(
            r##"<rect x="{bx:.1}" y="{sy:.1}" width="{bw:.1}" height="{bh:.1}" rx="4" fill="#f8fafc" fill-opacity="0.55" stroke="#64748b" stroke-width="1.2"/>"##
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

    // Messages, notes, else dividers
    let mut msg_num = 0usize;
    for (idx, item) in items.iter().enumerate() {
        let y = item_y[idx];
        match item {
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
            SeqItem::Note { from, to, pos, text } => {
                let nw = (estimate_text_width(text, 11.5) + 22.0).max(80.0);
                let nh = 28.0f32;
                let nx = match pos {
                    SeqNotePos::Left => centers[*from] - nw - 12.0,
                    SeqNotePos::Right => centers[*from] + 12.0,
                    SeqNotePos::Over => (centers[*from] + centers[*to]) / 2.0 - nw / 2.0,
                }
                .max(8.0);
                svg.push_str(&format!(
                    r##"<rect x="{nx:.1}" y="{:.1}" width="{nw:.1}" height="{nh:.1}" rx="3" fill="#fef9c3" stroke="#ca8a04" stroke-width="1.1"/>"##,
                    y - 14.0
                ));
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
            } => {
                msg_num += 1;
                let display_label = if autonumber {
                    format!("{msg_num}. {label}")
                } else {
                    label.clone()
                };
                let x1 = centers[*from];
                let x2 = centers[*to];
                let dash_attr = if *dashed { r##" stroke-dasharray="6,4""## } else { "" };
                let marker_attr = if *cross_end {
                    ""
                } else if *open_arrow {
                    r##" marker-end="url(#seq-open)""##
                } else {
                    r##" marker-end="url(#seq-arrow)""##
                };

                if from == to {
                    let loop_w = 38.0;
                    let loop_h = 24.0;
                    svg.push_str(&format!(
                        r##"<path d="M {x1:.1} {y:.1} H {:.1} V {:.1} H {x1:.1}" fill="none" stroke="#334155" stroke-width="1.4"{dash_attr}{marker_attr}/>"##,
                        x1 + loop_w,
                        y + loop_h
                    ));
                    svg.push_str(&format!(
                        r##"<text x="{:.1}" y="{:.1}" font-size="12" fill="#0f172a">{}</text>"##,
                        x1 + loop_w + 6.0,
                        y + loop_h / 2.0 + 4.0,
                        escape_xml(&display_label)
                    ));
                } else {
                    svg.push_str(&format!(
                        r##"<line x1="{x1:.1}" y1="{y:.1}" x2="{x2:.1}" y2="{y:.1}" stroke="#334155" stroke-width="1.4"{dash_attr}{marker_attr}/>"##
                    ));
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
            }
            SeqItem::BlockStart { .. } | SeqItem::BlockEnd => {}
        }
    }

    // Participant boxes (top & bottom)
    for (idx, p) in participants.iter().enumerate() {
        let cx = centers[idx];
        let w = col_widths[idx];
        for by in [top_y, bottom_box_y] {
            let bx = cx - w / 2.0;
            let fill = if p.is_actor { "#eff6ff" } else { "#f1f5f9" };
            let stroke = if p.is_actor { "#2563eb" } else { "#475569" };
            svg.push_str(&format!(
                r##"<rect x="{bx:.1}" y="{by:.1}" width="{w:.1}" height="{box_h:.1}" rx="6" fill="{fill}" stroke="{stroke}" stroke-width="1.4"/>"##
            ));
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="13" font-weight="600" fill="#0f172a">{}</text>"##,
                by + box_h / 2.0 + 4.5,
                escape_xml(&p.label)
            ));
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

fn split_alias(rest: &str) -> (&str, Option<&str>) {
    if let Some((id, lbl)) = rest.split_once(" as ") {
        (id.trim(), Some(lbl.trim()))
    } else {
        (rest.trim(), None)
    }
}

fn parse_seq_note(
    l: &str,
    participants: &mut Vec<SeqParticipant>,
    part_map: &mut HashMap<String, usize>,
    ensure: &impl Fn(&str, Option<&str>, bool, &mut Vec<SeqParticipant>, &mut HashMap<String, usize>) -> usize,
) -> Option<SeqItem> {
    let rest = l.strip_prefix("Note ")?.trim();
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
        Some(SeqItem::Note { from, to, pos, text })
    } else {
        let idx = ensure(who, None, false, participants, part_map);
        Some(SeqItem::Note {
            from: idx,
            to: idx,
            pos,
            text,
        })
    }
}

fn parse_seq_message(
    l: &str,
    participants: &mut Vec<SeqParticipant>,
    part_map: &mut HashMap<String, usize>,
    ensure: &impl Fn(&str, Option<&str>, bool, &mut Vec<SeqParticipant>, &mut HashMap<String, usize>) -> usize,
) -> Option<SeqItem> {
    let (lhs, label) = l.split_once(':')?;
    let arrows = [
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
            let from_id = from_raw.trim().trim_end_matches(['+', '-']);
            let to_id = to_raw.trim().trim_start_matches(['+', '-']).trim_end_matches(['+', '-']);
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
            });
        }
    }
    None
}

// ============================================================================
// 3. Flowchart / Graph -> Pure Rust SVG
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlowDir {
    Td,
    Bt,
    Lr,
    Rl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeShape {
    Rect,
    Round,
    Stadium,
    Diamond,
    Circle,
    Subroutine,
}

#[derive(Debug, Clone)]
struct FlowNode {
    _id: String,
    label: String,
    shape: NodeShape,
    subgraph: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeStyle {
    Solid,
    Dashed,
    Thick,
}

#[derive(Debug, Clone)]
struct FlowEdge {
    from: usize,
    to: usize,
    label: Option<String>,
    style: EdgeStyle,
    arrow: bool,
}

#[derive(Debug, Clone)]
struct SubgraphInfo {
    title: String,
}

fn render_flowchart_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let mut lines = source.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("%%"));
    let header = lines.next()?;
    let dir_token = header
        .split_whitespace()
        .nth(1)
        .unwrap_or("TD")
        .to_ascii_uppercase();
    let dir = match dir_token.as_str() {
        "LR" => FlowDir::Lr,
        "RL" => FlowDir::Rl,
        "BT" => FlowDir::Bt,
        _ => FlowDir::Td,
    };

    let mut nodes: Vec<FlowNode> = Vec::new();
    let mut node_map: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<FlowEdge> = Vec::new();
    let mut subgraphs: Vec<SubgraphInfo> = Vec::new();
    let mut sg_stack: Vec<usize> = Vec::new();

    for line in lines {
        let l = line.trim_end_matches(';').trim();
        if l.is_empty() || l.starts_with("classDef ") || l.starts_with("class ") || l.starts_with("style ") || l.starts_with("linkStyle ") {
            continue;
        }
        if l == "end" {
            sg_stack.pop();
            continue;
        }
        if let Some(rest) = l.strip_prefix("subgraph ") {
            let title = parse_subgraph_title(rest.trim());
            let sg_idx = subgraphs.len();
            subgraphs.push(SubgraphInfo { title });
            sg_stack.push(sg_idx);
            continue;
        }
        if l.starts_with("direction ") {
            continue;
        }
        parse_flowchart_statement(
            l,
            sg_stack.last().copied(),
            &mut nodes,
            &mut node_map,
            &mut edges,
        );
    }

    if nodes.is_empty() {
        return None;
    }

    // Node dimensions
    let node_sizes: Vec<(f32, f32)> = nodes
        .iter()
        .map(|node| {
            let tw = estimate_text_width(&node.label, 13.0);
            match node.shape {
                NodeShape::Diamond => ((tw + 44.0).max(88.0), 54.0),
                NodeShape::Circle => {
                    let d = (tw + 24.0).max(54.0);
                    (d, d)
                }
                _ => ((tw + 32.0).max(84.0), 40.0),
            }
        })
        .collect();

    let horizontal = matches!(dir, FlowDir::Lr | FlowDir::Rl);
    let reversed = matches!(dir, FlowDir::Bt | FlowDir::Rl);
    let margin = if subgraphs.is_empty() { 32.0f32 } else { 44.0f32 };
    let layer_gap = 64.0f32;
    let sibling_gap = 34.0f32;

    let edge_pairs: Vec<(usize, usize)> = edges.iter().map(|e| (e.from, e.to)).collect();
    let (centers, total_w, total_h) = compute_layered_layout(
        nodes.len(),
        &edge_pairs,
        &node_sizes,
        horizontal,
        reversed,
        margin,
        layer_gap,
        sibling_gap,
    );

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(
        r##"<defs>
  <marker id="flow-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 0 1 L 10 5 L 0 9 z" fill="#475569"/>
  </marker>
</defs>"##,
    );
    svg.push_str(&format!(
        r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##
    ));

    // Subgraph bounding boxes
    for (sg_idx, sg) in subgraphs.iter().enumerate() {
        let members: Vec<usize> = nodes
            .iter()
            .enumerate()
            .filter_map(|(i, node)| (node.subgraph == Some(sg_idx)).then_some(i))
            .collect();
        if members.is_empty() {
            continue;
        }
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for u in members {
            let (cx, cy) = centers[u];
            let (w, h) = node_sizes[u];
            min_x = min_x.min(cx - w / 2.0);
            min_y = min_y.min(cy - h / 2.0);
            max_x = max_x.max(cx + w / 2.0);
            max_y = max_y.max(cy + h / 2.0);
        }
        let sx = (min_x - 14.0).max(4.0);
        let sy = (min_y - 24.0).max(4.0);
        let sw = (max_x + 14.0) - sx;
        let sh = (max_y + 12.0) - sy;
        svg.push_str(&format!(
            r##"<rect x="{sx:.1}" y="{sy:.1}" width="{sw:.1}" height="{sh:.1}" rx="8" fill="#f8fafc" stroke="#cbd5e1" stroke-width="1.2" stroke-dasharray="5,3"/>"##
        ));
        if !sg.title.is_empty() {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" font-size="11.5" font-weight="600" fill="#475569">{}</text>"##,
                sx + 10.0,
                sy + 15.0,
                escape_xml(&sg.title)
            ));
        }
    }

    // Edges
    for e in &edges {
        let (x1, y1) = centers[e.from];
        let (x2, y2) = centers[e.to];
        let (w1, h1) = node_sizes[e.from];
        let (w2, h2) = node_sizes[e.to];
        let (sx, sy) = edge_anchor(x1, y1, w1, h1, x2, y2);
        let (ex, ey) = edge_anchor(x2, y2, w2, h2, x1, y1);
        let stroke_w = match e.style {
            EdgeStyle::Thick => "2.6",
            _ => "1.4",
        };
        let dash = match e.style {
            EdgeStyle::Dashed => r##" stroke-dasharray="5,4""##,
            _ => "",
        };
        let marker = if e.arrow {
            r##" marker-end="url(#flow-arrow)""##
        } else {
            ""
        };
        let path_d = if horizontal {
            let mx = (sx + ex) / 2.0;
            format!("M {sx:.1} {sy:.1} C {mx:.1} {sy:.1}, {mx:.1} {ey:.1}, {ex:.1} {ey:.1}")
        } else {
            let my = (sy + ey) / 2.0;
            format!("M {sx:.1} {sy:.1} C {sx:.1} {my:.1}, {ex:.1} {my:.1}, {ex:.1} {ey:.1}")
        };
        svg.push_str(&format!(
            r##"<path d="{path_d}" fill="none" stroke="#475569" stroke-width="{stroke_w}"{dash}{marker}/>"##
        ));
        if let Some(lbl) = &e.label {
            let lx = (sx + ex) / 2.0;
            let ly = (sy + ey) / 2.0;
            let lw = estimate_text_width(lbl, 11.0) + 10.0;
            svg.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{lw:.1}" height="18" rx="3" fill="#ffffff" stroke="#e2e8f0" stroke-width="0.8"/>"##,
                lx - lw / 2.0,
                ly - 9.0
            ));
            svg.push_str(&format!(
                r##"<text x="{lx:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#334155">{}</text>"##,
                ly + 3.5,
                escape_xml(lbl)
            ));
        }
    }

    // Nodes
    for (idx, node) in nodes.iter().enumerate() {
        let (cx, cy) = centers[idx];
        let (w, h) = node_sizes[idx];
        let x = cx - w / 2.0;
        let y = cy - h / 2.0;
        match node.shape {
            NodeShape::Rect => {
                svg.push_str(&format!(
                    r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="3" fill="#eff6ff" stroke="#2563eb" stroke-width="1.4"/>"##
                ));
            }
            NodeShape::Round => {
                svg.push_str(&format!(
                    r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="10" fill="#eff6ff" stroke="#2563eb" stroke-width="1.4"/>"##
                ));
            }
            NodeShape::Stadium => {
                svg.push_str(&format!(
                    r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="{:.1}" fill="#f0fdf4" stroke="#16a34a" stroke-width="1.4"/>"##,
                    h / 2.0
                ));
            }
            NodeShape::Subroutine => {
                svg.push_str(&format!(
                    r##"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="3" fill="#eff6ff" stroke="#2563eb" stroke-width="1.4"/>"##
                ));
                svg.push_str(&format!(
                    r##"<line x1="{:.1}" y1="{y:.1}" x2="{:.1}" y2="{:.1}" stroke="#2563eb" stroke-width="1.2"/>"##,
                    x + 7.0, x + 7.0, y + h
                ));
                svg.push_str(&format!(
                    r##"<line x1="{:.1}" y1="{y:.1}" x2="{:.1}" y2="{:.1}" stroke="#2563eb" stroke-width="1.2"/>"##,
                    x + w - 7.0, x + w - 7.0, y + h
                ));
            }
            NodeShape::Diamond => {
                svg.push_str(&format!(
                    r##"<polygon points="{cx:.1},{y:.1} {:.1},{cy:.1} {cx:.1},{:.1} {x:.1},{cy:.1}" fill="#fefce8" stroke="#ca8a04" stroke-width="1.4"/>"##,
                    x + w, y + h
                ));
            }
            NodeShape::Circle => {
                svg.push_str(&format!(
                    r##"<ellipse cx="{cx:.1}" cy="{cy:.1}" rx="{:.1}" ry="{:.1}" fill="#f5f3ff" stroke="#7c3aed" stroke-width="1.4"/>"##,
                    w / 2.0, h / 2.0
                ));
            }
        }
        svg.push_str(&format!(
            r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="13" font-weight="500" fill="#0f172a">{}</text>"##,
            cy + 4.5,
            escape_xml(&node.label)
        ));
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

fn edge_anchor(cx: f32, cy: f32, w: f32, h: f32, tx: f32, ty: f32) -> (f32, f32) {
    let dx = tx - cx;
    let dy = ty - cy;
    if dx.abs() < 1e-3 && dy.abs() < 1e-3 {
        return (cx, cy);
    }
    let rx = w / 2.0;
    let ry = h / 2.0;
    let sx = if dx.abs() > 1e-3 { rx / dx.abs() } else { f32::MAX };
    let sy = if dy.abs() > 1e-3 { ry / dy.abs() } else { f32::MAX };
    let scale = sx.min(sy);
    (cx + dx * scale, cy + dy * scale)
}

fn parse_subgraph_title(rest: &str) -> String {
    if let Some(open) = rest.find('[')
        && let Some(close) = rest.rfind(']')
        && close > open
    {
        return unquote(rest[open + 1..close].trim());
    }
    unquote(rest)
}

fn parse_flowchart_statement(
    stmt: &str,
    subgraph: Option<usize>,
    nodes: &mut Vec<FlowNode>,
    node_map: &mut HashMap<String, usize>,
    edges: &mut Vec<FlowEdge>,
) {
    // Split statement by edges while keeping track of edge tokens.
    let mut parts: Vec<(&str, Option<(EdgeStyle, bool, Option<String>)>)> = Vec::new();
    let mut cursor = 0usize;
    let bytes = stmt.as_bytes();
    while cursor < bytes.len() {
        // Skip bracketed/quoted node labels so `--` inside `[a-b]` isn't treated as an edge.
        if matches!(bytes[cursor], b'[' | b'(' | b'{') {
            let close = match bytes[cursor] {
                b'[' => b']',
                b'(' => b')',
                _ => b'}',
            };
            if let Some(rel_end) = stmt[cursor + 1..].find(close as char) {
                cursor += 1 + rel_end + 1;
                continue;
            }
        }
        if let Some((edge_len, style, arrow, label, next_start)) = match_flowchart_edge(&stmt[cursor..]) {
            let lhs = stmt[..cursor].trim();
            let _ = edge_len;
            parts.push((lhs, Some((style, arrow, label.clone()))));
            // Continue parsing the remainder recursively by slicing `stmt`
            let rest = &stmt[cursor + next_start..];
            let mut prev_group = ensure_node_group(lhs, subgraph, nodes, node_map);
            let mut rest_str = rest;
            let mut pending = (style, arrow, label);
            loop {
                if let Some((next_lhs, next_edge, next_rest)) = split_next_flowchart_edge(rest_str) {
                    let cur_group = ensure_node_group(next_lhs, subgraph, nodes, node_map);
                    for &u in &prev_group {
                        for &v in &cur_group {
                            edges.push(FlowEdge {
                                from: u,
                                to: v,
                                label: pending.2.clone(),
                                style: pending.0,
                                arrow: pending.1,
                            });
                        }
                    }
                    prev_group = cur_group;
                    pending = next_edge;
                    rest_str = next_rest;
                } else {
                    let cur_group = ensure_node_group(rest_str, subgraph, nodes, node_map);
                    for &u in &prev_group {
                        for &v in &cur_group {
                            edges.push(FlowEdge {
                                from: u,
                                to: v,
                                label: pending.2.clone(),
                                style: pending.0,
                                arrow: pending.1,
                            });
                        }
                    }
                    break;
                }
            }
            return;
        }
        let ch_len = stmt[cursor..].chars().next().map_or(1, char::len_utf8);
        cursor += ch_len;
    }

    if parts.is_empty() {
        let _ = ensure_node_group(stmt, subgraph, nodes, node_map);
    }
}

fn split_next_flowchart_edge(s: &str) -> Option<(&str, (EdgeStyle, bool, Option<String>), &str)> {
    let bytes = s.as_bytes();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        if matches!(bytes[cursor], b'[' | b'(' | b'{') {
            let close = match bytes[cursor] {
                b'[' => b']',
                b'(' => b')',
                _ => b'}',
            };
            if let Some(rel_end) = s[cursor + 1..].find(close as char) {
                cursor += 1 + rel_end + 1;
                continue;
            }
        }
        if let Some((_len, style, arrow, label, next_start)) = match_flowchart_edge(&s[cursor..]) {
            return Some((s[..cursor].trim(), (style, arrow, label), &s[cursor + next_start..]));
        }
        let ch_len = s[cursor..].chars().next().map_or(1, char::len_utf8);
        cursor += ch_len;
    }
    None
}

fn match_flowchart_edge(s: &str) -> Option<(usize, EdgeStyle, bool, Option<String>, usize)> {
    // Check `-- label -->`, `-. label .->`, `== label ==>`
    for (prefix, suffix, style, arrow) in [
        ("-- ", " -->", EdgeStyle::Solid, true),
        ("-- ", " ---", EdgeStyle::Solid, false),
        ("-. ", " .->", EdgeStyle::Dashed, true),
        ("== ", " ==>", EdgeStyle::Thick, true),
    ] {
        if let Some(after) = s.strip_prefix(prefix)
            && let Some(end_idx) = after.find(suffix)
        {
            let lbl = after[..end_idx].trim();
            let consumed = prefix.len() + end_idx + suffix.len();
            return Some((
                consumed,
                style,
                arrow,
                (!lbl.is_empty()).then(|| unquote(lbl)),
                consumed,
            ));
        }
    }

    // Standard arrows with optional `|label|`
    for (tok, style, arrow) in [
        ("-.->", EdgeStyle::Dashed, true),
        ("-.-", EdgeStyle::Dashed, false),
        ("==>", EdgeStyle::Thick, true),
        ("===", EdgeStyle::Thick, false),
        ("-->", EdgeStyle::Solid, true),
        ("---", EdgeStyle::Solid, false),
    ] {
        if let Some(after) = s.strip_prefix(tok) {
            let after_ws = after.trim_start();
            let ws_len = after.len() - after_ws.len();
            if let Some(after_pipe) = after_ws.strip_prefix('|')
                && let Some(close_pipe) = after_pipe.find('|')
            {
                let lbl = after_pipe[..close_pipe].trim();
                let consumed = tok.len() + ws_len + 1 + close_pipe + 1;
                return Some((
                    consumed,
                    style,
                    arrow,
                    (!lbl.is_empty()).then(|| unquote(lbl)),
                    consumed,
                ));
            }
            return Some((tok.len(), style, arrow, None, tok.len()));
        }
    }
    None
}

fn ensure_node_group(
    expr: &str,
    subgraph: Option<usize>,
    nodes: &mut Vec<FlowNode>,
    node_map: &mut HashMap<String, usize>,
) -> Vec<usize> {
    let mut ids = Vec::new();
    for part in expr.split('&') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let (id, label_opt, shape_opt) = parse_node_token(p);
        if id.is_empty() {
            continue;
        }
        if let Some(&idx) = node_map.get(id) {
            if let Some(lbl) = label_opt {
                nodes[idx].label = lbl;
            }
            if let Some(sh) = shape_opt {
                nodes[idx].shape = sh;
            }
            if nodes[idx].subgraph.is_none() {
                nodes[idx].subgraph = subgraph;
            }
            ids.push(idx);
        } else {
            let idx = nodes.len();
            nodes.push(FlowNode {
                _id: id.to_string(),
                label: label_opt.unwrap_or_else(|| id.to_string()),
                shape: shape_opt.unwrap_or(NodeShape::Rect),
                subgraph,
            });
            node_map.insert(id.to_string(), idx);
            ids.push(idx);
        }
    }
    ids
}

fn parse_node_token(token: &str) -> (&str, Option<String>, Option<NodeShape>) {
    let t = token.trim();
    for (open, close, shape) in [
        ("([", "])", NodeShape::Stadium),
        ("[[", "]]", NodeShape::Subroutine),
        ("((", "))", NodeShape::Circle),
        ("[", "]", NodeShape::Rect),
        ("(", ")", NodeShape::Round),
        ("{", "}", NodeShape::Diamond),
    ] {
        if let Some(open_idx) = t.find(open)
            && t.ends_with(close)
            && t.len() >= open_idx + open.len() + close.len()
        {
            let id = t[..open_idx].trim();
            let inner = &t[open_idx + open.len()..t.len() - close.len()];
            return (id, Some(unquote(inner.trim())), Some(shape));
        }
    }
    (t, None, None)
}

// ============================================================================
// Shared Layered Layout (used by flowchart, stateDiagram, classDiagram, erDiagram)
// ============================================================================

#[allow(clippy::too_many_arguments)]
fn compute_layered_layout(
    n: usize,
    edges: &[(usize, usize)],
    node_sizes: &[(f32, f32)],
    horizontal: bool,
    reversed: bool,
    margin: f32,
    layer_gap: f32,
    sibling_gap: f32,
) -> (Vec<(f32, f32)>, u32, u32) {
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(from, to) in edges {
        if from != to {
            adj[from].push(to);
        }
    }
    let mut in_deg = vec![0usize; n];
    for list in &adj {
        for &v in list {
            in_deg[v] += 1;
        }
    }
    let mut layer = vec![0usize; n];
    let mut state = vec![0u8; n];
    let mut topo = Vec::with_capacity(n);
    fn dfs(u: usize, adj: &[Vec<usize>], state: &mut [u8], topo: &mut Vec<usize>) {
        state[u] = 1;
        for &v in &adj[u] {
            if state[v] == 0 {
                dfs(v, adj, state, topo);
            }
        }
        state[u] = 2;
        topo.push(u);
    }
    for i in 0..n {
        if in_deg[i] == 0 && state[i] == 0 {
            dfs(i, &adj, &mut state, &mut topo);
        }
    }
    for i in 0..n {
        if state[i] == 0 {
            dfs(i, &adj, &mut state, &mut topo);
        }
    }
    topo.reverse();
    let mut rank_pos = vec![0usize; n];
    for (pos, &u) in topo.iter().enumerate() {
        rank_pos[u] = pos;
    }
    for &u in &topo {
        for &v in &adj[u] {
            if rank_pos[u] < rank_pos[v] {
                layer[v] = layer[v].max(layer[u] + 1);
            }
        }
    }

    let max_layer = layer.iter().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); max_layer + 1];
    for (idx, &l) in layer.iter().enumerate() {
        layers[l].push(idx);
    }

    let mut rev_adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(from, to) in edges {
        if from != to {
            rev_adj[to].push(from);
        }
    }
    for _ in 0..2 {
        for l in 1..=max_layer {
            let prev_pos: HashMap<usize, usize> = layers[l - 1]
                .iter()
                .enumerate()
                .map(|(i, &u)| (u, i))
                .collect();
            layers[l].sort_by(|&a, &b| {
                let bary = |u: usize| -> f32 {
                    let mut sum = 0.0;
                    let mut cnt = 0.0;
                    for &p in &rev_adj[u] {
                        if let Some(&pos) = prev_pos.get(&p) {
                            sum += pos as f32;
                            cnt += 1.0;
                        }
                    }
                    if cnt > 0.0 { sum / cnt } else { f32::MAX }
                };
                bary(a)
                    .partial_cmp(&bary(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }

    let mut layer_primary_sizes: Vec<f32> = Vec::with_capacity(layers.len());
    let mut layer_cross_sizes: Vec<f32> = Vec::with_capacity(layers.len());
    for l_nodes in &layers {
        let mut p_max = 32.0f32;
        let mut c_sum = 0.0f32;
        for (i, &u) in l_nodes.iter().enumerate() {
            let (nw, nh) = node_sizes[u];
            let (p, c) = if horizontal { (nw, nh) } else { (nh, nw) };
            p_max = p_max.max(p);
            c_sum += c;
            if i + 1 < l_nodes.len() {
                c_sum += sibling_gap;
            }
        }
        layer_primary_sizes.push(p_max);
        layer_cross_sizes.push(c_sum);
    }

    let max_cross = layer_cross_sizes.iter().copied().fold(0.0f32, f32::max);
    let total_primary: f32 = layer_primary_sizes.iter().sum::<f32>()
        + layer_gap * (layers.len().saturating_sub(1) as f32);

    let mut centers = vec![(0.0f32, 0.0f32); n];
    let mut p_cursor = margin;
    let layer_iter: Vec<usize> = if reversed {
        (0..layers.len()).rev().collect()
    } else {
        (0..layers.len()).collect()
    };
    for l in layer_iter {
        let p_size = layer_primary_sizes[l];
        let c_total = layer_cross_sizes[l];
        let mut c_cursor = margin + (max_cross - c_total) / 2.0;
        for &u in &layers[l] {
            let (nw, nh) = node_sizes[u];
            let (p_mid, c_span) = (p_cursor + p_size / 2.0, if horizontal { nh } else { nw });
            let c_mid = c_cursor + c_span / 2.0;
            centers[u] = if horizontal {
                (p_mid, c_mid)
            } else {
                (c_mid, p_mid)
            };
            c_cursor += c_span + sibling_gap;
        }
        p_cursor += p_size + layer_gap;
    }

    let (total_w, total_h) = if horizontal {
        (
            (total_primary + margin * 2.0).ceil() as u32,
            (max_cross + margin * 2.0).ceil() as u32,
        )
    } else {
        (
            (max_cross + margin * 2.0).ceil() as u32,
            (total_primary + margin * 2.0).ceil() as u32,
        )
    };
    (centers, total_w.max(120), total_h.max(80))
}

// ============================================================================
// State Diagram (stateDiagram / stateDiagram-v2)
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum StateKind {
    Start,
    End,
    Normal,
}

struct StateNode {
    label: String,
    description: Option<String>,
    kind: StateKind,
    composite: Option<String>,
}

struct StateEdge {
    from: usize,
    to: usize,
    label: Option<String>,
}

fn render_state_diagram_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let mut horizontal = false;
    let mut reversed = false;
    let mut nodes: Vec<StateNode> = Vec::new();
    let mut node_map: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<StateEdge> = Vec::new();
    let mut composite_stack: Vec<String> = Vec::new();

    let ensure_state = |raw_id: &str, nodes: &mut Vec<StateNode>, map: &mut HashMap<String, usize>, comp: Option<&str>| -> usize {
        let id = raw_id.trim();
        if let Some(&idx) = map.get(id) {
            if nodes[idx].composite.is_none() {
                nodes[idx].composite = comp.map(ToString::to_string);
            }
            return idx;
        }
        let idx = nodes.len();
        nodes.push(StateNode {
            label: id.to_string(),
            description: None,
            kind: StateKind::Normal,
            composite: comp.map(ToString::to_string),
        });
        map.insert(id.to_string(), idx);
        idx
    };

    for raw in source.lines().skip(1) {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if l == "}" {
            composite_stack.pop();
            continue;
        }
        if let Some(rest) = l.strip_prefix("direction ") {
            match rest.trim().to_ascii_uppercase().as_str() {
                "LR" => {
                    horizontal = true;
                    reversed = false;
                }
                "RL" => {
                    horizontal = true;
                    reversed = true;
                }
                "BT" => {
                    horizontal = false;
                    reversed = true;
                }
                _ => {
                    horizontal = false;
                    reversed = false;
                }
            }
            continue;
        }
        if l.starts_with("note ") || l.starts_with("end note") {
            continue;
        }
        let cur_comp = composite_stack.last().map(String::as_str);
        if let Some(rest) = l.strip_prefix("state ") {
            if let Some(before_brace) = rest.strip_suffix('{') {
                let name = unquote(before_brace.trim());
                composite_stack.push(name);
                continue;
            }
            if let Some((disp, alias)) = rest.split_once(" as ") {
                let idx = ensure_state(alias.trim(), &mut nodes, &mut node_map, cur_comp);
                nodes[idx].label = unquote(disp.trim());
                continue;
            }
        }
        if let Some((lhs_raw, rhs_all)) = l.split_once("-->") {
            let lhs = lhs_raw.trim();
            let (rhs_raw, label) = if let Some((r, lbl)) = rhs_all.split_once(':') {
                (r.trim(), Some(lbl.trim().to_string()).filter(|s| !s.is_empty()))
            } else {
                (rhs_all.trim(), None)
            };
            let from = if lhs == "[*]" {
                if let Some(&idx) = node_map.get("__start__") {
                    idx
                } else {
                    let idx = nodes.len();
                    nodes.push(StateNode {
                        label: String::new(),
                        description: None,
                        kind: StateKind::Start,
                        composite: cur_comp.map(ToString::to_string),
                    });
                    node_map.insert("__start__".to_string(), idx);
                    idx
                }
            } else {
                ensure_state(lhs, &mut nodes, &mut node_map, cur_comp)
            };
            let to = if rhs_raw == "[*]" {
                if let Some(&idx) = node_map.get("__end__") {
                    idx
                } else {
                    let idx = nodes.len();
                    nodes.push(StateNode {
                        label: String::new(),
                        description: None,
                        kind: StateKind::End,
                        composite: cur_comp.map(ToString::to_string),
                    });
                    node_map.insert("__end__".to_string(), idx);
                    idx
                }
            } else {
                ensure_state(rhs_raw, &mut nodes, &mut node_map, cur_comp)
            };
            edges.push(StateEdge { from, to, label });
            continue;
        }
        if let Some((state_id, desc)) = l.split_once(':') {
            let sid = state_id.trim();
            if !sid.is_empty() && !sid.contains(' ') {
                let idx = ensure_state(sid, &mut nodes, &mut node_map, cur_comp);
                nodes[idx].description = Some(desc.trim().to_string());
            }
        }
    }

    if nodes.is_empty() {
        return None;
    }

    let node_sizes: Vec<(f32, f32)> = nodes
        .iter()
        .map(|node| match node.kind {
            StateKind::Start | StateKind::End => (26.0, 26.0),
            StateKind::Normal => {
                let title_w = estimate_text_width(&node.label, 13.0);
                let desc_w = node
                    .description
                    .as_deref()
                    .map(|d| estimate_text_width(d, 11.0))
                    .unwrap_or(0.0);
                let w = (title_w.max(desc_w) + 32.0).max(88.0);
                let h = if node.description.is_some() { 54.0 } else { 38.0 };
                (w, h)
            }
        })
        .collect();

    let has_comp = nodes.iter().any(|n| n.composite.is_some());
    let margin = if has_comp { 44.0 } else { 32.0 };
    let edge_pairs: Vec<(usize, usize)> = edges.iter().map(|e| (e.from, e.to)).collect();
    let (centers, total_w, total_h) = compute_layered_layout(
        nodes.len(),
        &edge_pairs,
        &node_sizes,
        horizontal,
        reversed,
        margin,
        62.0,
        36.0,
    );

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(
        r##"<defs>
  <marker id="st-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto">
    <path d="M 0 1 L 10 5 L 0 9 z" fill="#475569"/>
  </marker>
</defs>"##,
    );
    svg.push_str(&format!(
        r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##
    ));

    // Composite state containers
    let mut comp_groups: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut comp_order: Vec<&str> = Vec::new();
    for (idx, node) in nodes.iter().enumerate() {
        if let Some(c) = node.composite.as_deref() {
            if !comp_groups.contains_key(c) {
                comp_order.push(c);
            }
            comp_groups.entry(c).or_default().push(idx);
        }
    }
    for comp in comp_order {
        let idxs = &comp_groups[comp];
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for &i in idxs {
            let (cx, cy) = centers[i];
            let (nw, nh) = node_sizes[i];
            min_x = min_x.min(cx - nw / 2.0);
            min_y = min_y.min(cy - nh / 2.0);
            max_x = max_x.max(cx + nw / 2.0);
            max_y = max_y.max(cy + nh / 2.0);
        }
        let sx = min_x - 14.0;
        let sy = min_y - 32.0;
        let sw = (max_x - min_x) + 28.0;
        let sh = (max_y - min_y) + 46.0;
        svg.push_str(&format!(
            r##"<rect x="{sx:.1}" y="{sy:.1}" width="{sw:.1}" height="{sh:.1}" rx="10" fill="#f8fafc" stroke="#94a3b8" stroke-width="1.2"/>"##
        ));
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" font-size="11.5" font-weight="bold" fill="#334155">{}</text>"##,
            sx + 10.0,
            sy + 18.0,
            escape_xml(comp)
        ));
    }

    // Transitions
    for e in &edges {
        let (x1, y1) = centers[e.from];
        let (x2, y2) = centers[e.to];
        let (w1, h1) = node_sizes[e.from];
        let (w2, h2) = node_sizes[e.to];
        if e.from == e.to {
            let lx = x1 + w1 / 2.0;
            let ty = y1 - h1 / 4.0;
            let by = y1 + h1 / 4.0;
            svg.push_str(&format!(
                r##"<path d="M {lx:.1} {ty:.1} C {:.1} {:.1}, {:.1} {:.1}, {lx:.1} {by:.1}" fill="none" stroke="#475569" stroke-width="1.4" marker-end="url(#st-arrow)"/>"##,
                lx + 32.0,
                ty - 14.0,
                lx + 32.0,
                by + 14.0
            ));
            if let Some(lbl) = &e.label {
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{:.1}" font-size="11" fill="#334155">{}</text>"##,
                    lx + 36.0,
                    y1 + 4.0,
                    escape_xml(lbl)
                ));
            }
            continue;
        }
        let (sx, sy) = edge_anchor(x1, y1, w1, h1, x2, y2);
        let (ex, ey) = edge_anchor(x2, y2, w2, h2, x1, y1);
        svg.push_str(&format!(
            r##"<line x1="{sx:.1}" y1="{sy:.1}" x2="{ex:.1}" y2="{ey:.1}" stroke="#475569" stroke-width="1.4" marker-end="url(#st-arrow)"/>"##
        ));
        if let Some(lbl) = &e.label {
            let mx = (sx + ex) / 2.0;
            let my = (sy + ey) / 2.0;
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

    // State nodes
    for (idx, node) in nodes.iter().enumerate() {
        let (cx, cy) = centers[idx];
        let (nw, nh) = node_sizes[idx];
        match node.kind {
            StateKind::Start => {
                svg.push_str(&format!(
                    r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="10" fill="#1e293b"/>"##
                ));
            }
            StateKind::End => {
                svg.push_str(&format!(
                    r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="11" fill="#ffffff" stroke="#1e293b" stroke-width="2"/>"##
                ));
                svg.push_str(&format!(
                    r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="6" fill="#1e293b"/>"##
                ));
            }
            StateKind::Normal => {
                let x = cx - nw / 2.0;
                let y = cy - nh / 2.0;
                svg.push_str(&format!(
                    r##"<rect x="{x:.1}" y="{y:.1}" width="{nw:.1}" height="{nh:.1}" rx="10" fill="#eff6ff" stroke="#2563eb" stroke-width="1.4"/>"##
                ));
                if let Some(desc) = &node.description {
                    svg.push_str(&format!(
                        r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="12.5" font-weight="bold" fill="#0f172a">{}</text>"##,
                        y + 18.0,
                        escape_xml(&node.label)
                    ));
                    svg.push_str(&format!(
                        r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#bfdbfe" stroke-width="1"/>"##,
                        x + 6.0,
                        y + 25.0,
                        x + nw - 6.0,
                        y + 25.0
                    ));
                    svg.push_str(&format!(
                        r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#475569">{}</text>"##,
                        y + 42.0,
                        escape_xml(desc)
                    ));
                } else {
                    svg.push_str(&format!(
                        r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="13" fill="#0f172a">{}</text>"##,
                        cy + 4.5,
                        escape_xml(&node.label)
                    ));
                }
            }
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

// ============================================================================
// Class Diagram (classDiagram)
// ============================================================================

struct ClassNode {
    name: String,
    stereotype: Option<String>,
    fields: Vec<String>,
    methods: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClassRelKind {
    Inheritance, // <|-- / --|>
    Composition, // *-- / --*
    Aggregation, // o-- / --o
    Association, // --> / <--
    Dependency,  // ..> / <..
    Realization, // ..|> / <|..
    Link,        // -- / ..
}

struct ClassEdge {
    from: usize,
    to: usize,
    kind: ClassRelKind,
    dashed: bool,
    label: Option<String>,
}

fn render_class_diagram_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let mut horizontal = false;
    let mut reversed = false;
    let mut classes: Vec<ClassNode> = Vec::new();
    let mut map: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<ClassEdge> = Vec::new();
    let mut current_class: Option<usize> = None;

    let mut ensure_class = |name: &str, classes: &mut Vec<ClassNode>, map: &mut HashMap<String, usize>| -> usize {
        let key = name.trim();
        if let Some(&idx) = map.get(key) {
            return idx;
        }
        let idx = classes.len();
        classes.push(ClassNode {
            name: key.to_string(),
            stereotype: None,
            fields: Vec::new(),
            methods: Vec::new(),
        });
        map.insert(key.to_string(), idx);
        idx
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
            } else if l.contains('(') && l.contains(')') {
                classes[cidx].methods.push(l.to_string());
            } else {
                classes[cidx].fields.push(l.to_string());
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("direction ") {
            match rest.trim().to_ascii_uppercase().as_str() {
                "LR" => {
                    horizontal = true;
                    reversed = false;
                }
                "RL" => {
                    horizontal = true;
                    reversed = true;
                }
                "BT" => {
                    horizontal = false;
                    reversed = true;
                }
                _ => {
                    horizontal = false;
                    reversed = false;
                }
            }
            continue;
        }
        if l.starts_with("<<") {
            if let Some(end) = l.find(">>") {
                let stereo = &l[..=end + 1];
                let cname = l[end + 2..].trim();
                if !cname.is_empty() {
                    let idx = ensure_class(cname, &mut classes, &mut map);
                    classes[idx].stereotype = Some(stereo.to_string());
                    continue;
                }
            }
        }
        if let Some(rest) = l.strip_prefix("class ") {
            if let Some(before_brace) = rest.strip_suffix('{') {
                let cname = before_brace.trim();
                let idx = ensure_class(cname, &mut classes, &mut map);
                current_class = Some(idx);
                continue;
            }
            let cname = rest.trim();
            if !cname.is_empty() {
                ensure_class(cname, &mut classes, &mut map);
                continue;
            }
        }
        if let Some((lhs_part, label_opt)) = split_class_relation(l, &mut classes, &mut map, &mut edges, &mut ensure_class) {
            let _ = (lhs_part, label_opt);
            continue;
        }
        if let Some((cname, member)) = l.split_once(':') {
            let cn = cname.trim();
            let mb = member.trim();
            if !cn.is_empty() && !cn.contains(' ') && !mb.is_empty() {
                let idx = ensure_class(cn, &mut classes, &mut map);
                if mb.contains('(') && mb.contains(')') {
                    classes[idx].methods.push(mb.to_string());
                } else {
                    classes[idx].fields.push(mb.to_string());
                }
            }
        }
    }

    if classes.is_empty() {
        return None;
    }

    let node_sizes: Vec<(f32, f32)> = classes
        .iter()
        .map(|c| {
            let mut max_w = estimate_text_width(&c.name, 13.0) + 28.0;
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
            let fields_h = if c.fields.is_empty() {
                0.0
            } else {
                c.fields.len() as f32 * 18.0 + 10.0
            };
            let methods_h = if c.methods.is_empty() {
                0.0
            } else {
                c.methods.len() as f32 * 18.0 + 10.0
            };
            (max_w.max(110.0), (header_h + fields_h + methods_h).max(36.0))
        })
        .collect();

    let edge_pairs: Vec<(usize, usize)> = edges.iter().map(|e| (e.from, e.to)).collect();
    let (centers, total_w, total_h) = compute_layered_layout(
        classes.len(),
        &edge_pairs,
        &node_sizes,
        horizontal,
        reversed,
        34.0,
        70.0,
        40.0,
    );

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
    svg.push_str(&format!(
        r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##
    ));

    // Relationships
    for e in &edges {
        let (x1, y1) = centers[e.from];
        let (x2, y2) = centers[e.to];
        let (w1, h1) = node_sizes[e.from];
        let (w2, h2) = node_sizes[e.to];
        let (sx, sy) = edge_anchor(x1, y1, w1, h1, x2, y2);
        let (ex, ey) = edge_anchor(x2, y2, w2, h2, x1, y1);
        let dash = if e.dashed { r#" stroke-dasharray="5,4""# } else { "" };
        let marker = match e.kind {
            ClassRelKind::Inheritance | ClassRelKind::Realization => r#" marker-end="url(#cls-inherit)""#,
            ClassRelKind::Composition => r#" marker-end="url(#cls-comp)""#,
            ClassRelKind::Aggregation => r#" marker-end="url(#cls-agg)""#,
            ClassRelKind::Association | ClassRelKind::Dependency => r#" marker-end="url(#cls-arrow)""#,
            ClassRelKind::Link => "",
        };
        svg.push_str(&format!(
            r##"<line x1="{sx:.1}" y1="{sy:.1}" x2="{ex:.1}" y2="{ey:.1}" stroke="#334155" stroke-width="1.4"{dash}{marker}/>"##
        ));
        if let Some(lbl) = &e.label {
            let mx = (sx + ex) / 2.0;
            let my = (sy + ey) / 2.0;
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

    // Class boxes
    for (idx, c) in classes.iter().enumerate() {
        let (cx, cy) = centers[idx];
        let (nw, nh) = node_sizes[idx];
        let x = cx - nw / 2.0;
        let y = cy - nh / 2.0;
        let header_h = if c.stereotype.is_some() { 40.0 } else { 28.0 };
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{nw:.1}" height="{nh:.1}" rx="4" fill="#ffffff" stroke="#334155" stroke-width="1.4"/>"##
        ));
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{nw:.1}" height="{header_h:.1}" rx="4" fill="#f1f5f9" stroke="#334155" stroke-width="1.4"/>"##
        ));
        if let Some(st) = &c.stereotype {
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="10.5" fill="#475569">{}</text>"##,
                y + 15.0,
                escape_xml(st)
            ));
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="13" font-weight="bold" fill="#0f172a">{}</text>"##,
                y + 32.0,
                escape_xml(&c.name)
            ));
        } else {
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="13" font-weight="bold" fill="#0f172a">{}</text>"##,
                y + 19.0,
                escape_xml(&c.name)
            ));
        }

        let mut cur_y = y + header_h;
        if !c.fields.is_empty() {
            for (i, f) in c.fields.iter().enumerate() {
                svg.push_str(&format!(
                    r##"<text x="{:.1}" y="{:.1}" font-size="11.5" fill="#1e293b">{}</text>"##,
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
                    r##"<text x="{:.1}" y="{:.1}" font-size="11.5" fill="#1e293b">{}</text>"##,
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

fn split_class_relation<'a>(
    line: &'a str,
    classes: &mut Vec<ClassNode>,
    map: &mut HashMap<String, usize>,
    edges: &mut Vec<ClassEdge>,
    ensure_class: &mut impl FnMut(&str, &mut Vec<ClassNode>, &mut HashMap<String, usize>) -> usize,
) -> Option<(&'a str, Option<String>)> {
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

    let (body, label) = if let Some((b, lbl)) = line.split_once(':') {
        (b.trim(), Some(lbl.trim().to_string()).filter(|s| !s.is_empty()))
    } else {
        (line.trim(), None)
    };

    for &(op, target_is_lhs, kind, dashed) in OPS {
        if let Some(pos) = body.find(op) {
            let lhs_raw = body[..pos].trim();
            let rhs_raw = body[pos + op.len()..].trim();
            // Strip optional quoted cardinalities (`Foo "1" --> "*" Bar`)
            let lhs_name = lhs_raw.split_whitespace().next().unwrap_or("");
            let rhs_name = rhs_raw.split_whitespace().last().unwrap_or("");
            if lhs_name.is_empty() || rhs_name.is_empty() {
                return None;
            }
            let lhs_idx = ensure_class(lhs_name, classes, map);
            let rhs_idx = ensure_class(rhs_name, classes, map);
            let (from, to) = if target_is_lhs {
                (rhs_idx, lhs_idx)
            } else {
                (lhs_idx, rhs_idx)
            };
            edges.push(ClassEdge {
                from,
                to,
                kind,
                dashed,
                label: label.clone(),
            });
            return Some((lhs_name, label));
        }
    }
    None
}

// ============================================================================
// ER Diagram (erDiagram)
// ============================================================================

struct ErAttr {
    ty: String,
    name: String,
    key: String,
}

struct ErEntity {
    name: String,
    attrs: Vec<ErAttr>,
}

struct ErEdge {
    from: usize,
    to: usize,
    left_card: String,
    right_card: String,
    dashed: bool,
    label: String,
}

fn render_er_diagram_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let mut entities: Vec<ErEntity> = Vec::new();
    let mut map: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<ErEdge> = Vec::new();
    let mut current_entity: Option<usize> = None;

    let ensure_entity = |name: &str, entities: &mut Vec<ErEntity>, map: &mut HashMap<String, usize>| -> usize {
        let key = unquote(name.trim());
        if let Some(&idx) = map.get(&key) {
            return idx;
        }
        let idx = entities.len();
        entities.push(ErEntity {
            name: key.clone(),
            attrs: Vec::new(),
        });
        map.insert(key, idx);
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
                let key = parts
                    .get(2)
                    .filter(|s| !s.starts_with('"'))
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                entities[eidx].attrs.push(ErAttr { ty, name, key });
            }
            continue;
        }
        if let Some(before_brace) = l.strip_suffix('{') {
            let ename = before_brace.trim();
            if !ename.is_empty() && !ename.contains("--") && !ename.contains("..") {
                let idx = ensure_entity(ename, &mut entities, &mut map);
                current_entity = Some(idx);
                continue;
            }
        }
        if let Some((rel_part, label_raw)) = l.split_once(':') {
            if let Some((lhs, left_card, dashed, right_card, rhs)) = parse_er_relation(rel_part.trim()) {
                let from = ensure_entity(lhs, &mut entities, &mut map);
                let to = ensure_entity(rhs, &mut entities, &mut map);
                edges.push(ErEdge {
                    from,
                    to,
                    left_card: format_er_card(left_card),
                    right_card: format_er_card(right_card),
                    dashed,
                    label: unquote(label_raw.trim()),
                });
            }
        }
    }

    if entities.is_empty() {
        return None;
    }

    let node_sizes: Vec<(f32, f32)> = entities
        .iter()
        .map(|e| {
            let mut max_w = estimate_text_width(&e.name, 13.0) + 32.0;
            for a in &e.attrs {
                let row = if a.key.is_empty() {
                    format!("{}  {}", a.ty, a.name)
                } else {
                    format!("{}  {}  {}", a.ty, a.name, a.key)
                };
                max_w = max_w.max(estimate_text_width(&row, 11.5) + 28.0);
            }
            let h = 30.0
                + if e.attrs.is_empty() {
                    8.0
                } else {
                    e.attrs.len() as f32 * 20.0 + 10.0
                };
            (max_w.max(120.0), h)
        })
        .collect();

    let edge_pairs: Vec<(usize, usize)> = edges.iter().map(|e| (e.from, e.to)).collect();
    let (centers, total_w, total_h) = compute_layered_layout(
        entities.len(),
        &edge_pairs,
        &node_sizes,
        true,
        false,
        36.0,
        84.0,
        40.0,
    );

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(
        r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##
    ));

    // Edges
    for e in &edges {
        let (x1, y1) = centers[e.from];
        let (x2, y2) = centers[e.to];
        let (w1, h1) = node_sizes[e.from];
        let (w2, h2) = node_sizes[e.to];
        let (sx, sy) = edge_anchor(x1, y1, w1, h1, x2, y2);
        let (ex, ey) = edge_anchor(x2, y2, w2, h2, x1, y1);
        let dash = if e.dashed { r#" stroke-dasharray="5,4""# } else { "" };
        svg.push_str(&format!(
            r##"<line x1="{sx:.1}" y1="{sy:.1}" x2="{ex:.1}" y2="{ey:.1}" stroke="#475569" stroke-width="1.5"{dash}/>"##
        ));
        let dx = ex - sx;
        let dy = ey - sy;
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        let ux = dx / len;
        let uy = dy / len;
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="10.5" font-weight="bold" fill="#0f766e">{}</text>"##,
            sx + ux * 16.0,
            sy + uy * 16.0 - 5.0,
            escape_xml(&e.left_card)
        ));
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="10.5" font-weight="bold" fill="#0f766e">{}</text>"##,
            ex - ux * 16.0,
            ey - uy * 16.0 - 5.0,
            escape_xml(&e.right_card)
        ));
        if !e.label.is_empty() {
            let mx = (sx + ex) / 2.0;
            let my = (sy + ey) / 2.0;
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
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{nw:.1}" height="{nh:.1}" rx="4" fill="#ffffff" stroke="#0f766e" stroke-width="1.4"/>"##
        ));
        svg.push_str(&format!(
            r##"<rect x="{x:.1}" y="{y:.1}" width="{nw:.1}" height="28" rx="4" fill="#f0fdfa" stroke="#0f766e" stroke-width="1.4"/>"##
        ));
        svg.push_str(&format!(
            r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="13" font-weight="bold" fill="#134e4a">{}</text>"##,
            y + 19.0,
            escape_xml(&ent.name)
        ));
        for (i, a) in ent.attrs.iter().enumerate() {
            let ay = y + 46.0 + i as f32 * 20.0;
            let row = if a.key.is_empty() {
                format!("{}  {}", a.ty, a.name)
            } else {
                format!("{}  {}  [{}]", a.ty, a.name, a.key)
            };
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{ay:.1}" font-size="11.5" fill="#1e293b">{}</text>"##,
                x + 10.0,
                escape_xml(&row)
            ));
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

fn parse_er_relation(s: &str) -> Option<(&str, &str, bool, &str, &str)> {
    let (sep, dashed) = if let Some(idx) = s.find("--") {
        (idx, false)
    } else if let Some(idx) = s.find("..") {
        (idx, true)
    } else {
        return None;
    };
    let lhs_all = s[..sep].trim();
    let rhs_all = s[sep + 2..].trim();
    let lhs_split = lhs_all.rfind(' ')?;
    let rhs_split = rhs_all.find(' ')?;
    let lhs = lhs_all[..lhs_split].trim();
    let left_card = lhs_all[lhs_split + 1..].trim();
    let right_card = rhs_all[..rhs_split].trim();
    let rhs = rhs_all[rhs_split + 1..].trim();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    Some((lhs, left_card, dashed, right_card, rhs))
}

fn format_er_card(card: &str) -> String {
    match card {
        "||" => "1".to_string(),
        "o|" | "|o" => "0..1".to_string(),
        "}|" | "|{" => "1..*".to_string(),
        "}o" | "o{" => "0..*".to_string(),
        other => other.to_string(),
    }
}

// ============================================================================
// Gantt Chart (gantt)
// ============================================================================

#[derive(Clone, Copy, PartialEq, Eq)]
enum GanttStatus {
    Normal,
    Done,
    Active,
    Crit,
    Milestone,
}

struct GanttTask {
    section: String,
    name: String,
    status: GanttStatus,
    start_day: i64,
    end_day: i64,
}

fn render_gantt_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let mut title: Option<String> = None;
    let mut current_section = String::from("Tasks");
    let mut tasks: Vec<GanttTask> = Vec::new();
    let mut id_end_day: HashMap<String, i64> = HashMap::new();
    let mut last_end_day: Option<i64> = None;

    for raw in source.lines().skip(1) {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ") {
            title = Some(rest.trim().to_string());
            continue;
        }
        if l.starts_with("dateFormat ") || l.starts_with("axisFormat ") || l.starts_with("excludes ") {
            continue;
        }
        if let Some(rest) = l.strip_prefix("section ") {
            current_section = rest.trim().to_string();
            continue;
        }
        if let Some((name_part, meta_part)) = l.split_once(':') {
            let task_name = name_part.trim().to_string();
            let tokens: Vec<&str> = meta_part
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            if tokens.is_empty() {
                continue;
            }
            let mut status = GanttStatus::Normal;
            let mut idx = 0;
            while idx < tokens.len() {
                match tokens[idx] {
                    "done" => {
                        status = GanttStatus::Done;
                        idx += 1;
                    }
                    "active" => {
                        status = GanttStatus::Active;
                        idx += 1;
                    }
                    "crit" => {
                        status = GanttStatus::Crit;
                        idx += 1;
                    }
                    "milestone" => {
                        status = GanttStatus::Milestone;
                        idx += 1;
                    }
                    _ => break,
                }
            }
            let rem = &tokens[idx..];
            if rem.is_empty() {
                continue;
            }
            // Parse remaining tokens: optional `id`, `start_or_after`, `duration_or_end`
            let (task_id, start_tok, dur_tok) = match rem.len() {
                1 => (None, None, rem[0]),
                2 => {
                    if parse_gantt_date(rem[0]).is_some() || rem[0].starts_with("after ") {
                        (None, Some(rem[0]), rem[1])
                    } else {
                        (Some(rem[0].to_string()), None, rem[1])
                    }
                }
                _ => (Some(rem[0].to_string()), Some(rem[1]), rem[2]),
            };

            let start_day = if let Some(st) = start_tok {
                if let Some(dep_id) = st.strip_prefix("after ") {
                    id_end_day
                        .get(dep_id.trim())
                        .copied()
                        .or(last_end_day)
                        .unwrap_or_else(|| days_from_civil(2026, 1, 1))
                } else {
                    parse_gantt_date(st)
                        .or(last_end_day)
                        .unwrap_or_else(|| days_from_civil(2026, 1, 1))
                }
            } else {
                last_end_day.unwrap_or_else(|| days_from_civil(2026, 1, 1))
            };

            let end_day = if let Some(ed) = parse_gantt_date(dur_tok) {
                ed.max(start_day + 1)
            } else {
                let span = parse_gantt_duration_days(dur_tok);
                if status == GanttStatus::Milestone {
                    start_day + 1
                } else {
                    start_day + span.max(1)
                }
            };

            if let Some(id) = task_id {
                id_end_day.insert(id, end_day);
            }
            last_end_day = Some(end_day);
            tasks.push(GanttTask {
                section: current_section.clone(),
                name: task_name,
                status,
                start_day,
                end_day,
            });
        }
    }

    if tasks.is_empty() {
        return None;
    }

    let min_day = tasks.iter().map(|t| t.start_day).min().unwrap_or(0);
    let max_day = tasks.iter().map(|t| t.end_day).max().unwrap_or(min_day + 1).max(min_day + 1);
    let total_days = (max_day - min_day) as f32;

    let label_col_w = tasks
        .iter()
        .map(|t| estimate_text_width(&t.name, 12.0))
        .fold(120.0f32, f32::max)
        .min(240.0)
        + 24.0;
    let chart_w = 480.0f32;
    let right_pad = 24.0f32;
    let top_pad = if title.is_some() { 52.0f32 } else { 28.0f32 };
    let row_h = 30.0f32;
    let axis_h = 28.0f32;
    let total_w = (label_col_w + chart_w + right_pad).ceil() as u32;
    let total_h = (top_pad + tasks.len() as f32 * row_h + axis_h + 12.0).ceil() as u32;

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

    // Vertical grid lines & date ticks (up to 6 ticks)
    let ticks = 5usize;
    let bottom_y = top_pad + tasks.len() as f32 * row_h;
    for i in 0..=ticks {
        let frac = i as f32 / ticks as f32;
        let gx = label_col_w + frac * chart_w;
        svg.push_str(&format!(
            r##"<line x1="{gx:.1}" y1="{top_pad:.1}" x2="{gx:.1}" y2="{bottom_y:.1}" stroke="#e2e8f0" stroke-width="1"/>"##
        ));
        let day = min_day + ((total_days * frac).round() as i64);
        let (y, m, d) = civil_from_days(day);
        svg.push_str(&format!(
            r##"<text x="{gx:.1}" y="{:.1}" text-anchor="middle" font-size="10.5" fill="#64748b">{y:04}-{m:02}-{d:02}</text>"##,
            bottom_y + 18.0
        ));
    }

    // Rows & bars
    let mut prev_section: Option<&str> = None;
    for (idx, task) in tasks.iter().enumerate() {
        let ry = top_pad + idx as f32 * row_h;
        let bg = if idx % 2 == 0 { "#f8fafc" } else { "#ffffff" };
        svg.push_str(&format!(
            r##"<rect x="8" y="{ry:.1}" width="{:.1}" height="{row_h:.1}" fill="{bg}" fill-opacity="0.7"/>"##,
            total_w as f32 - 16.0
        ));
        let label = if prev_section != Some(task.section.as_str()) {
            prev_section = Some(task.section.as_str());
            format!("[{}] {}", task.section, task.name)
        } else {
            task.name.clone()
        };
        svg.push_str(&format!(
            r##"<text x="14" y="{:.1}" font-size="12" fill="#1e293b">{}</text>"##,
            ry + 19.0,
            escape_xml(&label)
        ));

        let x1 = label_col_w + ((task.start_day - min_day) as f32 / total_days) * chart_w;
        let x2 = label_col_w + ((task.end_day - min_day) as f32 / total_days) * chart_w;
        let bw = (x2 - x1).max(6.0);
        let (fill, stroke) = match task.status {
            GanttStatus::Done => ("#cbd5e1", "#64748b"),
            GanttStatus::Active => ("#93c5fd", "#2563eb"),
            GanttStatus::Crit => ("#fca5a5", "#dc2626"),
            GanttStatus::Milestone => ("#fde047", "#ca8a04"),
            GanttStatus::Normal => ("#a5b4fc", "#4f46e5"),
        };
        if task.status == GanttStatus::Milestone {
            let cy = ry + row_h / 2.0;
            svg.push_str(&format!(
                r##"<polygon points="{x1:.1},{:.1} {:.1},{cy:.1} {x1:.1},{:.1} {:.1},{cy:.1}" fill="{fill}" stroke="{stroke}" stroke-width="1.3"/>"##,
                cy - 8.0,
                x1 + 8.0,
                cy + 8.0,
                x1 - 8.0
            ));
        } else {
            svg.push_str(&format!(
                r##"<rect x="{x1:.1}" y="{:.1}" width="{bw:.1}" height="18" rx="4" fill="{fill}" stroke="{stroke}" stroke-width="1.2"/>"##,
                ry + 6.0
            ));
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

fn parse_gantt_date(s: &str) -> Option<i64> {
    let t = s.trim();
    let sep = if t.contains('-') { '-' } else if t.contains('/') { '/' } else { return None };
    let parts: Vec<&str> = t.split(sep).collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    Some(days_from_civil(y, m, d))
}

fn parse_gantt_duration_days(s: &str) -> i64 {
    let t = s.trim().to_ascii_lowercase();
    if let Some(n) = t.strip_suffix('w').and_then(|v| v.parse::<i64>().ok()) {
        return n * 7;
    }
    if let Some(n) = t.strip_suffix('d').and_then(|v| v.parse::<i64>().ok()) {
        return n;
    }
    if let Some(n) = t.strip_suffix('h').and_then(|v| v.parse::<i64>().ok()) {
        return ((n + 23) / 24).max(1);
    }
    1
}

/// Howard Hinnant's civil-from-days / days-from-civil algorithms (proleptic Gregorian).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = m as i64;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m as u32, d as u32)
}

// ============================================================================
// Chart SVG Fallback (`pie`, `xychart-beta` when native chart isn't available, e.g. clipboard paste)
// ============================================================================

/// Render a parsed `MermaidChartSpec` (`pie` or `xychart-beta`) to a standalone SVG.
///
/// Used as a fallback when a native Office chart cannot be instantiated (e.g., clipboard fragment paste).
pub fn render_chart_spec_svg(spec: &MermaidChartSpec) -> Option<(Vec<u8>, u32, u32)> {
    const PALETTE: &[&str] = &[
        "#3b82f6", "#10b981", "#f59e0b", "#ef4444", "#8b5cf6", "#ec4899", "#06b6d4", "#84cc16",
    ];
    if spec.series.is_empty() || spec.categories.is_empty() {
        return None;
    }

    if spec.kind == NewChartKind::Pie {
        let vals: Vec<f64> = spec.series[0]
            .1
            .iter()
            .map(|v| v.map(|n| n.0.max(0.0)).unwrap_or(0.0))
            .collect();
        let total: f64 = vals.iter().sum();
        if total <= 0.0 {
            return None;
        }
        let width_px = 460u32;
        let legend_rows = spec.categories.len().max(1) as u32;
        let height_px = (legend_rows * 24 + 80).max(250);
        let cx = 130.0f64;
        let cy = (height_px as f64) / 2.0 + if spec.title.is_some() { 12.0 } else { 0.0 };
        let r = 86.0f64;

        let mut svg = String::new();
        svg.push_str(&format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width_px}" height="{height_px}" viewBox="0 0 {width_px} {height_px}" font-family="sans-serif">"##
        ));
        svg.push_str(&format!(
            r##"<rect width="{width_px}" height="{height_px}" fill="#ffffff"/>"##
        ));
        if let Some(t) = &spec.title {
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="26" text-anchor="middle" font-size="14" font-weight="bold" fill="#0f172a">{}</text>"##,
                width_px as f32 / 2.0,
                escape_xml(t)
            ));
        }

        let mut angle = -std::f64::consts::FRAC_PI_2;
        for (i, &val) in vals.iter().enumerate() {
            let frac = val / total;
            if frac <= 0.0 {
                continue;
            }
            let color = PALETTE[i % PALETTE.len()];
            if frac >= 0.9999 {
                svg.push_str(&format!(
                    r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="{r:.1}" fill="{color}" stroke="#ffffff" stroke-width="1.5"/>"##
                ));
            } else {
                let next_angle = angle + frac * 2.0 * std::f64::consts::PI;
                let x1 = cx + r * angle.cos();
                let y1 = cy + r * angle.sin();
                let x2 = cx + r * next_angle.cos();
                let y2 = cy + r * next_angle.sin();
                let large_arc = if frac > 0.5 { 1 } else { 0 };
                svg.push_str(&format!(
                    r##"<path d="M {cx:.1} {cy:.1} L {x1:.1} {y1:.1} A {r:.1} {r:.1} 0 {large_arc} 1 {x2:.1} {y2:.1} Z" fill="{color}" stroke="#ffffff" stroke-width="1.5"/>"##
                ));
                angle = next_angle;
            }
        }

        // Legend
        let lx = 248.0f32;
        let ly_start = (cy as f32) - (spec.categories.len() as f32 * 22.0) / 2.0;
        for (i, cat) in spec.categories.iter().enumerate() {
            let color = PALETTE[i % PALETTE.len()];
            let ly = ly_start + i as f32 * 22.0;
            let pct = (vals.get(i).copied().unwrap_or(0.0) / total) * 100.0;
            svg.push_str(&format!(
                r##"<rect x="{lx:.1}" y="{:.1}" width="12" height="12" rx="2" fill="{color}"/>"##,
                ly
            ));
            svg.push_str(&format!(
                r##"<text x="{:.1}" y="{:.1}" font-size="11.5" fill="#1e293b">{} ({pct:.1}%)</text>"##,
                lx + 18.0,
                ly + 10.5,
                escape_xml(cat)
            ));
        }

        svg.push_str("</svg>");
        return Some((svg.into_bytes(), width_px, height_px));
    }

    // XY chart (bar / line)
    let mut max_val = 1.0f64;
    for (_, vals) in &spec.series {
        for v in vals.iter().flatten() {
            max_val = max_val.max(v.0);
        }
    }
    let width_px = 520u32;
    let height_px = 280u32;
    let pad_l = 52.0f32;
    let pad_r = 24.0f32;
    let pad_t = if spec.title.is_some() { 44.0f32 } else { 24.0f32 };
    let pad_b = 42.0f32;
    let plot_w = width_px as f32 - pad_l - pad_r;
    let plot_h = height_px as f32 - pad_t - pad_b;

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width_px}" height="{height_px}" viewBox="0 0 {width_px} {height_px}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(
        r##"<rect width="{width_px}" height="{height_px}" fill="#ffffff"/>"##
    ));
    if let Some(t) = &spec.title {
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="24" text-anchor="middle" font-size="14" font-weight="bold" fill="#0f172a">{}</text>"##,
            width_px as f32 / 2.0,
            escape_xml(t)
        ));
    }
    // Grid & axes
    for i in 0..=4 {
        let frac = i as f32 / 4.0;
        let gy = pad_t + plot_h * (1.0 - frac);
        svg.push_str(&format!(
            r##"<line x1="{pad_l:.1}" y1="{gy:.1}" x2="{:.1}" y2="{gy:.1}" stroke="#e2e8f0" stroke-width="1"/>"##,
            pad_l + plot_w
        ));
        let tick_val = max_val * frac as f64;
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="10" fill="#64748b">{tick_val:.0}</text>"##,
            pad_l - 6.0,
            gy + 3.5
        ));
    }
    svg.push_str(&format!(
        r##"<line x1="{pad_l:.1}" y1="{pad_t:.1}" x2="{pad_l:.1}" y2="{:.1}" stroke="#64748b" stroke-width="1.2"/>"##,
        pad_t + plot_h
    ));
    svg.push_str(&format!(
        r##"<line x1="{pad_l:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#64748b" stroke-width="1.2"/>"##,
        pad_t + plot_h,
        pad_l + plot_w,
        pad_t + plot_h
    ));

    let cat_count = spec.categories.len().max(1);
    let slot_w = plot_w / cat_count as f32;
    for (ci, cat) in spec.categories.iter().enumerate() {
        let cx = pad_l + (ci as f32 + 0.5) * slot_w;
        svg.push_str(&format!(
            r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#334155">{}</text>"##,
            pad_t + plot_h + 18.0,
            escape_xml(cat)
        ));
    }

    for (si, (sname, vals)) in spec.series.iter().enumerate() {
        let color = PALETTE[si % PALETTE.len()];
        let is_line = spec.kind == NewChartKind::Line || sname.starts_with("Line");
        if is_line {
            let mut pts = Vec::new();
            for (ci, v_opt) in vals.iter().enumerate() {
                if let Some(v) = v_opt {
                    let px = pad_l + (ci as f32 + 0.5) * slot_w;
                    let py = pad_t + plot_h * (1.0 - (v.0 / max_val) as f32);
                    pts.push((px, py));
                }
            }
            if pts.len() >= 2 {
                let poly: String = pts
                    .iter()
                    .map(|(x, y)| format!("{x:.1},{y:.1}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                svg.push_str(&format!(
                    r##"<polyline points="{poly}" fill="none" stroke="{color}" stroke-width="2.2"/>"##
                ));
            }
            for (px, py) in pts {
                svg.push_str(&format!(
                    r##"<circle cx="{px:.1}" cy="{py:.1}" r="3.5" fill="{color}" stroke="#ffffff" stroke-width="1"/>"##
                ));
            }
        } else {
            let bar_w = (slot_w * 0.54).min(40.0);
            for (ci, v_opt) in vals.iter().enumerate() {
                if let Some(v) = v_opt {
                    let bh = (plot_h * (v.0 / max_val) as f32).max(2.0);
                    let bx = pad_l + (ci as f32 + 0.5) * slot_w - bar_w / 2.0;
                    let by = pad_t + plot_h - bh;
                    svg.push_str(&format!(
                        r##"<rect x="{bx:.1}" y="{by:.1}" width="{bar_w:.1}" height="{bh:.1}" rx="2" fill="{color}" fill-opacity="0.88"/>"##
                    ));
                }
            }
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), width_px, height_px))
}

// ============================================================================
// Mindmap (mindmap)
// ============================================================================

struct MindNode {
    label: String,
    depth: usize,
    children: Vec<usize>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn render_mindmap_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let mut nodes: Vec<MindNode> = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new(); // (indent, node_idx)

    for raw in source.lines().skip(1) {
        if raw.trim().is_empty() || raw.trim_start().starts_with("%%") {
            continue;
        }
        let indent = raw.chars().take_while(|c| c.is_whitespace()).count();
        let label = clean_mindmap_label(raw.trim());
        if label.is_empty() {
            continue;
        }
        while let Some(&(top_indent, _)) = stack.last() {
            if top_indent >= indent {
                stack.pop();
            } else {
                break;
            }
        }
        let depth = stack.len();
        let w = (estimate_text_width(&label, 12.5) + 26.0).max(68.0);
        let h = if depth == 0 { 38.0 } else { 30.0 };
        let idx = nodes.len();
        nodes.push(MindNode {
            label,
            depth,
            children: Vec::new(),
            x: 0.0,
            y: 0.0,
            w,
            h,
        });
        if let Some(&(_, parent_idx)) = stack.last() {
            nodes[parent_idx].children.push(idx);
        }
        stack.push((indent, idx));
    }

    if nodes.is_empty() {
        return None;
    }

    // Assign Y by leaf slots, X by depth.
    fn subtree_leaves(idx: usize, nodes: &[MindNode]) -> usize {
        if nodes[idx].children.is_empty() {
            1
        } else {
            nodes[idx]
                .children
                .iter()
                .map(|&c| subtree_leaves(c, nodes))
                .sum()
        }
    }

    let mut max_depth = 0usize;
    for n in &nodes {
        max_depth = max_depth.max(n.depth);
    }
    let mut depth_widths = vec![80.0f32; max_depth + 1];
    for n in &nodes {
        depth_widths[n.depth] = depth_widths[n.depth].max(n.w);
    }
    let mut depth_x = vec![28.0f32; max_depth + 1];
    for d in 1..=max_depth {
        depth_x[d] = depth_x[d - 1] + depth_widths[d - 1] + 44.0;
    }

    let leaf_h = 44.0f32;
    let margin_y = 24.0f32;
    fn layout_node(
        idx: usize,
        y_start: f32,
        leaf_h: f32,
        depth_x: &[f32],
        nodes: &mut [MindNode],
    ) -> f32 {
        let children = nodes[idx].children.clone();
        nodes[idx].x = depth_x[nodes[idx].depth];
        if children.is_empty() {
            nodes[idx].y = y_start + leaf_h / 2.0;
            return y_start + leaf_h;
        }
        let mut cursor = y_start;
        let mut first_cy = 0.0f32;
        let mut last_cy = 0.0f32;
        for (i, &c) in children.iter().enumerate() {
            cursor = layout_node(c, cursor, leaf_h, depth_x, nodes);
            let cy = nodes[c].y;
            if i == 0 {
                first_cy = cy;
            }
            last_cy = cy;
        }
        nodes[idx].y = (first_cy + last_cy) / 2.0;
        cursor
    }

    let mut total_y = margin_y;
    for i in 0..nodes.len() {
        if nodes[i].depth == 0 {
            let _ = subtree_leaves(i, &nodes);
            total_y = layout_node(i, total_y, leaf_h, &depth_x, &mut nodes);
        }
    }

    let total_w = (depth_x[max_depth] + depth_widths[max_depth] + 28.0).ceil() as u32;
    let total_h = (total_y + margin_y).ceil() as u32;

    const BRANCH_COLORS: &[&str] = &["#2563eb", "#059669", "#d97706", "#7c3aed", "#db2777", "#0891b2"];
    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(
        r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##
    ));

    // Curved edges
    for (p_idx, p) in nodes.iter().enumerate() {
        for (ci, &c_idx) in p.children.iter().enumerate() {
            let c = &nodes[c_idx];
            let sx = p.x + p.w;
            let sy = p.y;
            let ex = c.x;
            let ey = c.y;
            let mx = (sx + ex) / 2.0;
            let color = BRANCH_COLORS[(p_idx + ci) % BRANCH_COLORS.len()];
            svg.push_str(&format!(
                r##"<path d="M {sx:.1} {sy:.1} C {mx:.1} {sy:.1}, {mx:.1} {ey:.1}, {ex:.1} {ey:.1}" fill="none" stroke="{color}" stroke-width="1.8"/>"##
            ));
        }
    }

    // Nodes
    for (idx, n) in nodes.iter().enumerate() {
        let ny = n.y - n.h / 2.0;
        let (fill, stroke, text_fill, rx) = if n.depth == 0 {
            ("#1e293b", "#0f172a", "#ffffff", 18.0)
        } else {
            let c = BRANCH_COLORS[idx % BRANCH_COLORS.len()];
            ("#f8fafc", c, "#0f172a", 8.0)
        };
        svg.push_str(&format!(
            r##"<rect x="{:.1}" y="{ny:.1}" width="{:.1}" height="{:.1}" rx="{rx:.1}" fill="{fill}" stroke="{stroke}" stroke-width="1.5"/>"##,
            n.x, n.w, n.h
        ));
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="12.5" font-weight="{}" fill="{text_fill}">{}</text>"##,
            n.x + n.w / 2.0,
            n.y + 4.5,
            if n.depth == 0 { "bold" } else { "normal" },
            escape_xml(&n.label)
        ));
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

fn clean_mindmap_label(s: &str) -> String {
    let mut t = s.trim();
    for (open, close) in [
        ("((", "))"),
        ("{{", "}}"),
        ("(", ")"),
        ("[", "]"),
    ] {
        if let Some(pos) = t.find(open) {
            if let Some(end) = t.rfind(close) {
                if end > pos + open.len() {
                    t = t[pos + open.len()..end].trim();
                    break;
                }
            }
        }
    }
    unquote(t)
}

// ============================================================================
// Git Graph (gitGraph)
// ============================================================================

struct GitCommit {
    branch: usize,
    id: String,
    tag: Option<String>,
    parent: Option<usize>,
    merge_parent: Option<usize>,
}

fn render_git_graph_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let mut branches: Vec<String> = vec!["main".to_string()];
    let mut branch_head: Vec<Option<usize>> = vec![None];
    let mut current_branch = 0usize;
    let mut commits: Vec<GitCommit> = Vec::new();
    let mut auto_idx = 1usize;

    let ensure_branch = |name: &str, branches: &mut Vec<String>, branch_head: &mut Vec<Option<usize>>, head: Option<usize>| -> usize {
        if let Some(pos) = branches.iter().position(|b| b == name) {
            return pos;
        }
        let idx = branches.len();
        branches.push(name.to_string());
        branch_head.push(head);
        idx
    };

    for raw in source.lines().skip(1) {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if l.starts_with("commit") {
            let id = extract_git_attr(l, "id:").unwrap_or_else(|| {
                let s = format!("c{auto_idx}");
                auto_idx += 1;
                s
            });
            let tag = extract_git_attr(l, "tag:");
            let parent = branch_head[current_branch];
            let idx = commits.len();
            commits.push(GitCommit {
                branch: current_branch,
                id,
                tag,
                parent,
                merge_parent: None,
            });
            branch_head[current_branch] = Some(idx);
        } else if let Some(rest) = l.strip_prefix("branch ") {
            let bname = unquote(rest.split_whitespace().next().unwrap_or(""));
            if !bname.is_empty() {
                let head = branch_head[current_branch];
                current_branch = ensure_branch(&bname, &mut branches, &mut branch_head, head);
            }
        } else if let Some(rest) = l.strip_prefix("checkout ").or_else(|| l.strip_prefix("switch ")) {
            let bname = unquote(rest.split_whitespace().next().unwrap_or(""));
            if let Some(pos) = branches.iter().position(|b| b == &bname) {
                current_branch = pos;
            }
        } else if let Some(rest) = l.strip_prefix("merge ") {
            let bname = unquote(rest.split_whitespace().next().unwrap_or(""));
            if let Some(src_branch) = branches.iter().position(|b| b == &bname) {
                let parent = branch_head[current_branch];
                let merge_parent = branch_head[src_branch];
                let id = extract_git_attr(l, "id:").unwrap_or_else(|| format!("merge {bname}"));
                let tag = extract_git_attr(l, "tag:");
                let idx = commits.len();
                commits.push(GitCommit {
                    branch: current_branch,
                    id,
                    tag,
                    parent,
                    merge_parent,
                });
                branch_head[current_branch] = Some(idx);
            }
        }
    }

    if commits.is_empty() {
        return None;
    }

    const COLORS: &[&str] = &["#2563eb", "#10b981", "#f59e0b", "#8b5cf6", "#ef4444", "#06b6d4"];
    let label_col = branches
        .iter()
        .map(|b| estimate_text_width(b, 12.0))
        .fold(64.0f32, f32::max)
        + 28.0;
    let step_x = 64.0f32;
    let lane_h = 46.0f32;
    let top_pad = 28.0f32;
    let bottom_pad = 36.0f32;
    let total_w = (label_col + commits.len() as f32 * step_x + 36.0).ceil() as u32;
    let total_h = (top_pad + branches.len() as f32 * lane_h + bottom_pad).ceil() as u32;

    let commit_pos = |idx: usize| -> (f32, f32) {
        let x = label_col + (idx as f32 + 0.5) * step_x;
        let y = top_pad + (commits[idx].branch as f32 + 0.5) * lane_h;
        (x, y)
    };

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(
        r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##
    ));

    // Branch guide lines & labels
    for (bi, bname) in branches.iter().enumerate() {
        let ly = top_pad + (bi as f32 + 0.5) * lane_h;
        let color = COLORS[bi % COLORS.len()];
        svg.push_str(&format!(
            r##"<line x1="{label_col:.1}" y1="{ly:.1}" x2="{:.1}" y2="{ly:.1}" stroke="#e2e8f0" stroke-width="1.5" stroke-dasharray="4,4"/>"##,
            total_w as f32 - 18.0
        ));
        svg.push_str(&format!(
            r##"<text x="14" y="{:.1}" font-size="12" font-weight="bold" fill="{color}">{}</text>"##,
            ly + 4.0,
            escape_xml(bname)
        ));
    }

    // Commit links
    for (idx, c) in commits.iter().enumerate() {
        let (x2, y2) = commit_pos(idx);
        if let Some(p) = c.parent {
            let (x1, y1) = commit_pos(p);
            let color = COLORS[c.branch % COLORS.len()];
            let mx = (x1 + x2) / 2.0;
            svg.push_str(&format!(
                r##"<path d="M {x1:.1} {y1:.1} C {mx:.1} {y1:.1}, {mx:.1} {y2:.1}, {x2:.1} {y2:.1}" fill="none" stroke="{color}" stroke-width="2.2"/>"##
            ));
        }
        if let Some(mp) = c.merge_parent {
            let (x1, y1) = commit_pos(mp);
            let color = COLORS[commits[mp].branch % COLORS.len()];
            let mx = (x1 + x2) / 2.0;
            svg.push_str(&format!(
                r##"<path d="M {x1:.1} {y1:.1} C {mx:.1} {y1:.1}, {mx:.1} {y2:.1}, {x2:.1} {y2:.1}" fill="none" stroke="{color}" stroke-width="2" stroke-dasharray="4,3"/>"##
            ));
        }
    }

    // Commits & tags
    for (idx, c) in commits.iter().enumerate() {
        let (cx, cy) = commit_pos(idx);
        let color = COLORS[c.branch % COLORS.len()];
        svg.push_str(&format!(
            r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="7.5" fill="{color}" stroke="#ffffff" stroke-width="2"/>"##
        ));
        svg.push_str(&format!(
            r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="10.5" fill="#334155">{}</text>"##,
            cy + 20.0,
            escape_xml(&c.id)
        ));
        if let Some(tag) = &c.tag {
            let tw = estimate_text_width(tag, 10.0) + 12.0;
            svg.push_str(&format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{tw:.1}" height="16" rx="4" fill="#fef9c3" stroke="#ca8a04" stroke-width="1"/>"##,
                cx - tw / 2.0,
                cy - 27.0
            ));
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="10" font-weight="bold" fill="#854d0e">{}</text>"##,
                cy - 15.5,
                escape_xml(tag)
            ));
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

fn extract_git_attr(line: &str, key: &str) -> Option<String> {
    let pos = line.find(key)?;
    let rest = line[pos + key.len()..].trim_start();
    if let Some(after_q) = rest.strip_prefix('"') {
        let end = after_q.find('"')?;
        Some(after_q[..end].to_string())
    } else {
        rest.split_whitespace().next().map(ToString::to_string)
    }
}

// ============================================================================
// User Journey (`journey`)
// ============================================================================

struct JourneyStep {
    section: String,
    task: String,
    score: f32, // 1..=5
    actors: Vec<String>,
}

fn render_journey_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
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
            r##"<circle cx="{cx:.1}" cy="{cy:.1}" r="11" fill="{color}" stroke="#ffffff" stroke-width="2"/>"##
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

// ============================================================================
// Timeline (`timeline`)
// ============================================================================

struct TimelineEntry {
    period: String,
    events: Vec<String>,
}

fn render_timeline_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let mut title: Option<String> = None;
    let mut entries: Vec<TimelineEntry> = Vec::new();

    for raw in source.lines().skip(1) {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") || l.starts_with("section ") {
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ") {
            title = Some(rest.trim().to_string());
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
                });
            }
        }
    }

    if entries.is_empty() {
        return None;
    }

    let col_w = 136.0f32;
    let pad_x = 28.0f32;
    let top_y = if title.is_some() { 54.0f32 } else { 26.0f32 };
    let max_events = entries.iter().map(|e| e.events.len()).max().unwrap_or(1).max(1);
    let total_w = (pad_x * 2.0 + entries.len() as f32 * col_w).ceil() as u32;
    let total_h = (top_y + 68.0 + max_events as f32 * 38.0 + 20.0).ceil() as u32;
    let axis_y = top_y + 34.0;

    const ACCENTS: &[&str] = &["#2563eb", "#059669", "#d97706", "#7c3aed", "#db2777"];
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

    svg.push_str(&format!(
        r##"<line x1="{pad_x:.1}" y1="{axis_y:.1}" x2="{:.1}" y2="{axis_y:.1}" stroke="#94a3b8" stroke-width="2.5"/>"##,
        total_w as f32 - pad_x
    ));

    for (i, entry) in entries.iter().enumerate() {
        let cx = pad_x + (i as f32 + 0.5) * col_w;
        let color = ACCENTS[i % ACCENTS.len()];
        svg.push_str(&format!(
            r##"<circle cx="{cx:.1}" cy="{axis_y:.1}" r="7" fill="{color}" stroke="#ffffff" stroke-width="2"/>"##
        ));
        svg.push_str(&format!(
            r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="12.5" font-weight="bold" fill="#0f172a">{}</text>"##,
            axis_y - 14.0,
            escape_xml(&entry.period)
        ));
        for (ei, ev) in entry.events.iter().enumerate() {
            let ey = axis_y + 20.0 + ei as f32 * 38.0;
            let bw = col_w - 16.0;
            svg.push_str(&format!(
                r##"<rect x="{:.1}" y="{ey:.1}" width="{bw:.1}" height="28" rx="6" fill="#f8fafc" stroke="{color}" stroke-width="1.3"/>"##,
                cx - bw / 2.0
            ));
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{:.1}" text-anchor="middle" font-size="11.5" fill="#1e293b">{}</text>"##,
                ey + 18.0,
                escape_xml(ev)
            ));
        }
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

// ============================================================================
// Quadrant Chart (`quadrantChart`)
// ============================================================================

struct QuadrantPoint {
    label: String,
    x: f32, // 0.0..=1.0
    y: f32, // 0.0..=1.0
}

fn render_quadrant_chart_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let mut title: Option<String> = None;
    let mut x_left = String::from("Low");
    let mut x_right = String::from("High");
    let mut y_bottom = String::from("Low");
    let mut y_top = String::from("High");
    let mut q_labels = [
        String::from("Q1"),
        String::from("Q2"),
        String::from("Q3"),
        String::from("Q4"),
    ];
    let mut points: Vec<QuadrantPoint> = Vec::new();

    for raw in source.lines().skip(1) {
        let l = raw.trim();
        if l.is_empty() || l.starts_with("%%") {
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ") {
            title = Some(rest.trim().to_string());
        } else if let Some(rest) = l.strip_prefix("x-axis ") {
            if let Some((a, b)) = rest.split_once("-->") {
                x_left = a.trim().to_string();
                x_right = b.trim().to_string();
            } else {
                x_left = rest.trim().to_string();
            }
        } else if let Some(rest) = l.strip_prefix("y-axis ") {
            if let Some((a, b)) = rest.split_once("-->") {
                y_bottom = a.trim().to_string();
                y_top = b.trim().to_string();
            } else {
                y_bottom = rest.trim().to_string();
            }
        } else if let Some(rest) = l.strip_prefix("quadrant-1 ") {
            q_labels[0] = rest.trim().to_string();
        } else if let Some(rest) = l.strip_prefix("quadrant-2 ") {
            q_labels[1] = rest.trim().to_string();
        } else if let Some(rest) = l.strip_prefix("quadrant-3 ") {
            q_labels[2] = rest.trim().to_string();
        } else if let Some(rest) = l.strip_prefix("quadrant-4 ") {
            q_labels[3] = rest.trim().to_string();
        } else if let Some((lbl, coords)) = l.split_once(':') {
            if let Some(vals) = extract_bracket_list(coords) {
                if vals.len() >= 2 {
                    if let (Ok(x), Ok(y)) = (vals[0].trim().parse::<f32>(), vals[1].trim().parse::<f32>()) {
                        points.push(QuadrantPoint {
                            label: lbl.trim().to_string(),
                            x: x.clamp(0.0, 1.0),
                            y: y.clamp(0.0, 1.0),
                        });
                    }
                }
            }
        }
    }

    if points.is_empty() {
        return None;
    }

    let size = 340.0f32;
    let pad_l = 56.0f32;
    let pad_r = 36.0f32;
    let pad_t = if title.is_some() { 52.0f32 } else { 28.0f32 };
    let pad_b = 46.0f32;
    let total_w = (pad_l + size + pad_r).ceil() as u32;
    let total_h = (pad_t + size + pad_b).ceil() as u32;
    let half = size / 2.0;

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(
        r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##
    ));
    if let Some(t) = &title {
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="26" text-anchor="middle" font-size="15" font-weight="bold" fill="#0f172a">{}</text>"##,
            total_w as f32 / 2.0,
            escape_xml(t)
        ));
    }

    // 4 quadrants: Q2 (top-left), Q1 (top-right), Q3 (bottom-left), Q4 (bottom-right)
    let quads = [
        (pad_l + half, pad_t, "#eff6ff", &q_labels[0]),
        (pad_l, pad_t, "#f0fdf4", &q_labels[1]),
        (pad_l, pad_t + half, "#fefce8", &q_labels[2]),
        (pad_l + half, pad_t + half, "#fdf2f8", &q_labels[3]),
    ];
    for (qx, qy, bg, qlbl) in quads {
        svg.push_str(&format!(
            r##"<rect x="{qx:.1}" y="{qy:.1}" width="{half:.1}" height="{half:.1}" fill="{bg}" stroke="#cbd5e1" stroke-width="1"/>"##
        ));
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" font-weight="bold" fill="#64748b">{}</text>"##,
            qx + half / 2.0,
            qy + 18.0,
            escape_xml(qlbl)
        ));
    }

    // Axis labels
    svg.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#334155">{}</text>"##,
        pad_l + half * 0.5,
        pad_t + size + 22.0,
        escape_xml(&x_left)
    ));
    svg.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#334155">{}</text>"##,
        pad_l + half * 1.5,
        pad_t + size + 22.0,
        escape_xml(&x_right)
    ));
    svg.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="10.5" fill="#334155">{}</text>"##,
        pad_l - 6.0,
        pad_t + 14.0,
        escape_xml(&y_top)
    ));
    svg.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="end" font-size="10.5" fill="#334155">{}</text>"##,
        pad_l - 6.0,
        pad_t + size - 6.0,
        escape_xml(&y_bottom)
    ));

    for pt in &points {
        let px = pad_l + pt.x * size;
        let py = pad_t + (1.0 - pt.y) * size;
        svg.push_str(&format!(
            r##"<circle cx="{px:.1}" cy="{py:.1}" r="5.5" fill="#2563eb" stroke="#ffffff" stroke-width="1.5"/>"##
        ));
        svg.push_str(&format!(
            r##"<text x="{px:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#0f172a">{}</text>"##,
            py - 9.0,
            escape_xml(&pt.label)
        ));
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

// ============================================================================
// Helpers
// ============================================================================

fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && ((t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\''))) {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

fn estimate_text_width(s: &str, font_size: f32) -> f32 {
    let mut units = 0.0f32;
    for ch in s.chars() {
        if ch.is_ascii() {
            units += 0.56;
        } else {
            units += 1.0;
        }
    }
    units * font_size
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pie_and_xychart_to_native_chart_specs() {
        let pie = "pie showData\n  title ブラウザシェア\n  \"Chrome\" : 65\n  \"Safari\" : 20\n  \"Other\" : 15\n";
        let Some(MermaidOutput::Chart(spec)) = render_mermaid(pie) else {
            panic!("expected native chart for pie");
        };
        assert_eq!(spec.kind, NewChartKind::Pie);
        assert_eq!(spec.title.as_deref(), Some("ブラウザシェア"));
        assert_eq!(spec.categories, vec!["Chrome", "Safari", "Other"]);
        assert_eq!(spec.series[0].1.len(), 3);

        let xy = "xychart-beta\n  title \"月次売上\"\n  x-axis [Jan, Feb, Mar]\n  bar [100, 140, 180]\n  line [90, 120, 170]\n";
        let Some(MermaidOutput::Chart(xy_spec)) = render_mermaid(xy) else {
            panic!("expected native chart for xychart-beta");
        };
        assert_eq!(xy_spec.kind, NewChartKind::ClusteredColumn);
        assert_eq!(xy_spec.title.as_deref(), Some("月次売上"));
        assert_eq!(xy_spec.series.len(), 2);
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
        assert!(svg.starts_with("<svg "));
        assert!(svg.contains("ユーザー"));
        assert!(svg.contains("バックエンドAPI"));
        assert!(svg.contains("1. リクエスト送信"));
        assert!(svg.contains("200 OK"));
        assert!(svg.contains("alt"));
    }

    #[test]
    fn renders_flowchart_svg_with_shapes_subgraphs_and_cycles() {
        let flow = "\
flowchart LR
  subgraph Input
    A([開始]) --> B[データ検証]
  end
  B -->|OK| C{分岐}
  C -->|Yes| D((完了))
  C -->|Retry| B
";
        let Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        }) = render_mermaid(flow)
        else {
            panic!("expected svg for flowchart");
        };
        assert!(width_px > 250 && height_px > 100);
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(svg.contains("開始"));
        assert!(svg.contains("データ検証"));
        assert!(svg.contains("分岐"));
        assert!(svg.contains("完了"));
        assert!(svg.contains("Input"));
        assert!(svg.contains("OK"));
    }

    #[test]
    fn renders_state_diagram_svg() {
        let src = "\
stateDiagram-v2
  [*] --> Idle
  Idle --> Running : start
  state Active {
    Running --> Paused : pause
    Paused --> Running : resume
  }
  Running --> [*] : finish
";
        let Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        }) = render_mermaid(src)
        else {
            panic!("expected svg for stateDiagram-v2");
        };
        assert!(width_px > 120 && height_px > 120);
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(svg.contains("Idle"));
        assert!(svg.contains("Running"));
        assert!(svg.contains("Active"));
        assert!(svg.contains("start"));
    }

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
        assert!(svg.contains("Animal"));
        assert!(svg.contains("Dog"));
        assert!(svg.contains("speak() void"));
        assert!(svg.contains("inherits"));
    }

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
        assert!(svg.contains("CUSTOMER"));
        assert!(svg.contains("ORDER"));
        assert!(svg.contains("places"));
        assert!(svg.contains("0..*"));
    }

    #[test]
    fn renders_gantt_svg() {
        let src = "\
gantt
  title 開発スケジュール
  dateFormat YYYY-MM-DD
  section 設計
  要件定義 :done, req, 2026-01-01, 5d
  基本設計 :active, des, after req, 7d
  section 実装
  プロトタイプ :crit, impl, after des, 10d
  リリース :milestone, rel, after impl, 1d
";
        let Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        }) = render_mermaid(src)
        else {
            panic!("expected svg for gantt");
        };
        assert!(width_px > 400 && height_px > 150);
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(svg.contains("開発スケジュール"));
        assert!(svg.contains("要件定義"));
        assert!(svg.contains("基本設計"));
        assert!(svg.contains("2026-01-01"));
    }

    #[test]
    fn renders_mindmap_and_gitgraph_and_chart_svg_fallback() {
        let mm = "\
mindmap
  root((ibis-office))
    Markdown
      Mermaid
      Math
    Formats
      DOCX
      PPTX
";
        let Some(MermaidOutput::Svg { bytes, width_px, height_px }) = render_mermaid(mm) else {
            panic!("expected svg for mindmap");
        };
        assert!(width_px > 200 && height_px > 120);
        let mm_svg = std::str::from_utf8(&bytes).unwrap();
        assert!(mm_svg.contains("ibis-office"));
        assert!(mm_svg.contains("Mermaid"));

        let git = "\
gitGraph
  commit id: \"init\"
  branch feat
  checkout feat
  commit id: \"wip\"
  checkout main
  merge feat tag: \"v1.0\"
";
        let Some(MermaidOutput::Svg { bytes: gbytes, width_px: gw, height_px: gh }) = render_mermaid(git) else {
            panic!("expected svg for gitGraph");
        };
        assert!(gw > 250 && gh > 80);
        let git_svg = std::str::from_utf8(&gbytes).unwrap();
        assert!(git_svg.contains("main"));
        assert!(git_svg.contains("feat"));
        assert!(git_svg.contains("v1.0"));

        let pie = "pie\n  title Share\n  \"A\" : 60\n  \"B\" : 40\n";
        let Some(MermaidOutput::Chart(spec)) = render_mermaid(pie) else {
            panic!("expected chart spec");
        };
        let (svg_bytes, pw, ph) = render_chart_spec_svg(&spec).expect("svg fallback");
        assert!(pw > 300 && ph > 200);
        let pie_svg = std::str::from_utf8(&svg_bytes).unwrap();
        assert!(pie_svg.contains("Share"));
        assert!(pie_svg.contains("60.0%"));
    }

    #[test]
    fn renders_journey_timeline_quadrant_and_frontmatter() {
        let journey = "\
---
title: User Onboarding
---
journey
  title My Day
  section Morning
    Coffee: 5: Me
    Commute: 2: Me, Train
";
        let Some(MermaidOutput::Svg { bytes: jbytes, width_px: jw, height_px: jh }) = render_mermaid(journey) else {
            panic!("expected svg for journey with YAML frontmatter");
        };
        assert!(jw > 200 && jh > 180);
        let jsvg = std::str::from_utf8(&jbytes).unwrap();
        assert!(jsvg.contains("Coffee"));
        assert!(jsvg.contains("Commute"));

        let tl = "\
timeline
  title History
  2024 : Alpha
  2025 : Beta : GA
";
        let Some(MermaidOutput::Svg { bytes: tbytes, .. }) = render_mermaid(tl) else {
            panic!("expected svg for timeline");
        };
        let tsvg = std::str::from_utf8(&tbytes).unwrap();
        assert!(tsvg.contains("2024"));
        assert!(tsvg.contains("Alpha"));
        assert!(tsvg.contains("GA"));

        let qc = "\
quadrantChart
  title Priority Matrix
  x-axis Low Effort --> High Effort
  y-axis Low Impact --> High Impact
  quadrant-1 Strategic
  quadrant-2 Quick Wins
  Task A: [0.2, 0.8]
  Task B: [0.75, 0.3]
";
        let Some(MermaidOutput::Svg { bytes: qbytes, .. }) = render_mermaid(qc) else {
            panic!("expected svg for quadrantChart");
        };
        let qsvg = std::str::from_utf8(&qbytes).unwrap();
        assert!(qsvg.contains("Priority Matrix"));
        assert!(qsvg.contains("Quick Wins"));
        assert!(qsvg.contains("Task A"));
    }
}
